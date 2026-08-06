#!/usr/bin/env node
/**
 * Drift guard for the `AGENTS.md` instruction set.
 *
 * Polished instruction files are worse than none when they are false: an agent
 * follows a stale path or a command nothing runs, and does so confidently. This
 * checks the handful of properties that can be decided mechanically, and
 * nothing else — it deliberately lints no writing style, executes no Markdown
 * code block, and holds no authority over code, tests, or CI.
 *
 * Ten rules, each of which has a fixture test in `check-agent-contracts.test.mjs`:
 *
 *   inventory      every declared file and scope exists; every `AGENTS.md` in the
 *                  tree is declared
 *   paths          repository-relative Markdown links, inline-code paths, and
 *                  paths inside fenced blocks resolve on disk
 *   absolute-path  no `C:\…`, `/home/…`, `/Users/…`, `~/…`
 *   secret         no credential material, private-key marker, secret-named
 *                  assignment with a value, or token-shaped string
 *   transient      no 40-character commit SHA, `commit <hex>`, branch-shaped
 *                  token, or ISO date — none of which stay true
 *   size           the root file stays scannable; scoped files stay deltas
 *   duplication    a scoped file that repeats the root is worse than no scoped
 *                  file, so shared runs of prose fail
 *   command        every required verification command is executed by CI or is
 *                  declared local-only with a reason
 *
 * Path resolution is narrow on purpose. A token is treated as a repository path
 * only when it contains a slash, uses no shell metacharacters, and its first
 * segment names something that exists — so `@repolens/api-client` and
 * `sqlx::migrate!` are ignored rather than misclassified. Inline-code paths
 * resolve against the contract's scope and then the repository root; Markdown
 * links resolve against the file's own directory, as Markdown requires.
 *
 * Usage:
 *
 *     node scripts/check-agent-contracts.mjs [repository-root]
 */

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Root files stay scannable; scoped files stay deltas rather than handbooks. */
const MAX_LINES = { root: 260, scoped: 120 };

/** Fraction of a scoped file's substantial lines allowed to also appear in the root. */
const MAX_SHARED_LINE_RATIO = 0.3;

/** A shorter run than this is a coincidence; this long is a copy. */
const MAX_SHARED_RUN = 2;

/** Below this length a line carries too little to call duplication. */
const SIGNIFICANT_LINE_LENGTH = 20;

/** Never walked: generated, vendored, or another tool's working state. */
const SKIPPED_DIRECTORIES = new Set([
	'.git',
	'.claude',
	'.agents',
	'node_modules',
	'target',
	'.svelte-kit',
	'build',
	'dist',
	'coverage',
	'test-results',
	'playwright-report',
	'blob-report'
]);

/** A path containing one of these is generated, so its absence proves nothing. */
const GENERATED_SEGMENTS = new Set([
	'node_modules',
	'target',
	'build',
	'dist',
	'.svelte-kit',
	'.sqlx'
]);

const ABSOLUTE_PATH_PATTERNS = [
	{ name: 'Windows drive path', pattern: /(^|[\s"'`([])[A-Za-z]:[\\/]/ },
	{ name: 'POSIX home path', pattern: /(^|[\s"'`([])(\/home\/|\/Users\/|\/root\/|~\/)/ },
	{ name: 'UNC path', pattern: /\\\\[A-Za-z0-9_.$-]+\\/ }
];

const SECRET_PATTERNS = [
	{ name: 'database URL with an inline password', pattern: /postgres(?:ql)?:\/\/[^\s]*:[^\s]*@/i },
	{ name: 'Neon role secret', pattern: /\bnpg_[A-Za-z0-9]{16,}/ },
	{ name: 'GitHub token', pattern: /\bgh[pousr]_[A-Za-z0-9]{36}/ },
	{ name: 'GitHub fine-grained token', pattern: /\bgithub_pat_[A-Za-z0-9_]{50,}/ },
	{ name: 'private-key marker', pattern: /BEGIN [A-Z ]*PRIVATE KEY/ },
	{ name: 'service-account credential', pattern: /"type"\s*:\s*"service_account"/ },
	{ name: 'Google OAuth client secret', pattern: /\bGOCSPX-[A-Za-z0-9_-]{20,}/ },
	{ name: 'Google API key', pattern: /\bAIza[0-9A-Za-z_-]{35}/ },
	{
		name: 'secret-named environment assignment with a value',
		pattern:
			/\b(?:[A-Z][A-Z0-9_]*(?:SECRET|TOKEN|PASSWORD|PASSWD|CREDENTIALS?|PRIVATE|_KEY)[A-Z0-9_]*|DATABASE_URL|DATABASE_DIRECT_URL)\s*=\s*[^\s`'"]/
	},
	{ name: 'token-shaped string', pattern: /\b(?=[A-Za-z0-9]*\d)(?=[A-Za-z0-9]*[A-Za-z])[A-Za-z0-9]{32,}\b/ }
];

const TRANSIENT_PATTERNS = [
	{ name: 'commit SHA', pattern: /\b[0-9a-f]{40}\b/i },
	{ name: 'commit reference', pattern: /\bcommit\s+[0-9a-f]{7,40}\b/i },
	{
		name: 'branch-shaped token',
		pattern: /\b(?:feat|fix|chore|docs|refactor|perf|test|ci|build|release|hotfix)\/[a-z0-9][a-z0-9-]*(?![-.\w])/
	},
	{ name: 'dated statement', pattern: /\b20\d\d-\d\d-\d\d\b/ }
];

/** A repository path: at least one slash, no shell metacharacters, not absolute. */
const PATH_TOKEN = /^[A-Za-z0-9_.@+-]+(?:\/[A-Za-z0-9_.@+-]+)+\/?$/;

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/** Repository-relative POSIX form, so messages read the same on both platforms. */
function toPosix(value) {
	return value.split(sep).join('/').replace(/\\/g, '/');
}

function relativePosix(root, target) {
	return toPosix(relative(root, target));
}

function isDirectory(path) {
	try {
		return statSync(path).isDirectory();
	} catch {
		return false;
	}
}

/** Splits a Markdown document into fenced blocks and everything else. */
function partition(source) {
	const lines = source.split(/\r?\n/);
	const prose = [];
	const fenced = [];
	let inFence = false;
	let fenceMarker = '';

	for (const line of lines) {
		const fence = /^\s*(`{3,}|~{3,})/.exec(line);
		if (fence) {
			if (!inFence) {
				inFence = true;
				fenceMarker = fence[1][0];
				continue;
			}
			if (fence[1][0] === fenceMarker) {
				inFence = false;
				continue;
			}
		}
		(inFence ? fenced : prose).push(line);
	}

	return { prose, fenced };
}

/** Candidate repository paths, with the line they appeared on. */
function pathCandidates(source) {
	const { prose, fenced } = partition(source);
	const found = [];

	prose.forEach((line, index) => {
		for (const match of line.matchAll(/`([^`\n]+)`/g)) {
			found.push({ token: match[1], line: index + 1, kind: 'code' });
		}
	});

	fenced.forEach((line, index) => {
		for (const token of line.split(/\s+/)) {
			if (token) found.push({ token, line: index + 1, kind: 'fenced' });
		}
	});

	return found.filter(({ token }) => PATH_TOKEN.test(token));
}

/** Repository-relative Markdown link targets, ignoring URLs and bare anchors. */
function linkTargets(source) {
	const targets = [];
	source.split(/\r?\n/).forEach((line, index) => {
		for (const match of line.matchAll(/\]\(([^)\s]+)\)/g)) {
			const target = match[1].split('#')[0];
			if (!target) continue;
			if (/^[a-z][a-z0-9+.-]*:/i.test(target)) continue;
			targets.push({ target, line: index + 1 });
		}
	});
	return targets;
}

/**
 * Commands from the `## Verification` section's fenced blocks.
 *
 * Only that heading. A scoped file's `## Commands` block narrows a gate for
 * convenience; it is not itself a required check, and demanding CI run
 * `cargo test -p repolens-core` would be inventing policy rather than checking it.
 */
function requiredCommands(source) {
	const lines = source.split(/\r?\n/);
	const commands = [];
	let inSection = false;
	let inFence = false;

	for (const line of lines) {
		const heading = /^(#{1,6})\s+(.*?)\s*$/.exec(line);
		if (heading && !inFence) {
			inSection = heading[1].length <= 2 && heading[2].toLowerCase() === 'verification';
			continue;
		}
		if (/^\s*(`{3,}|~{3,})/.test(line)) {
			inFence = !inFence;
			continue;
		}
		if (inSection && inFence) {
			const command = line.trim();
			if (command) commands.push(command);
		}
	}

	return commands;
}

/** Workflow text with whole-line comments removed, so a mention is not a run. */
function executedByCi(root) {
	const directory = join(root, '.github', 'workflows');
	if (!isDirectory(directory)) return '';
	return readdirSync(directory)
		.filter((name) => name.endsWith('.yml') || name.endsWith('.yaml'))
		.map((name) => readFileSync(join(directory, name), 'utf8'))
		.join('\n')
		.split(/\r?\n/)
		.filter((line) => !/^\s*#/.test(line))
		.join('\n');
}

/** Every `scripts` value from every committed package manifest. */
function packageScripts(root) {
	const values = [];
	walk(root, root, (path) => {
		if (!path.endsWith('package.json')) return;
		try {
			const manifest = JSON.parse(readFileSync(path, 'utf8'));
			if (manifest && typeof manifest.scripts === 'object') {
				values.push(...Object.values(manifest.scripts).filter((v) => typeof v === 'string'));
			}
		} catch {
			// A malformed manifest is somebody else's gate to fail.
		}
	});
	return values;
}

function walk(root, directory, visit) {
	let entries;
	try {
		entries = readdirSync(directory, { withFileTypes: true });
	} catch {
		return;
	}
	for (const entry of entries) {
		const path = join(directory, entry.name);
		if (entry.isDirectory()) {
			if (SKIPPED_DIRECTORIES.has(entry.name)) continue;
			walk(root, path, visit);
		} else if (entry.isFile()) {
			visit(path);
		}
	}
}

function significantLines(source) {
	const seen = [];
	for (const line of source.split(/\r?\n/)) {
		const trimmed = line.trim();
		if (trimmed.startsWith('>')) {
			seen.push(null); // The precedence statement is allowed to be identical.
			continue;
		}
		const normalized = trimmed.replace(/\s+/g, ' ').toLowerCase();
		seen.push(normalized.length >= SIGNIFICANT_LINE_LENGTH ? normalized : null);
	}
	return seen;
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

function checkInventory(root, contracts, errors) {
	const declared = new Map();

	for (const contract of contracts) {
		const path = toPosix(contract.path ?? '');
		const scope = toPosix(contract.scope ?? '');
		if (!path || !scope) {
			errors.push(`docs/agent-contracts.json: every entry needs a "path" and a "scope"`);
			continue;
		}
		declared.set(path, contract);

		const scopeDirectory = scope === '.' ? root : join(root, scope);
		if (!isDirectory(scopeDirectory)) {
			errors.push(`docs/agent-contracts.json: declared scope "${scope}" does not exist`);
			continue;
		}
		if (!existsSync(join(root, path))) {
			errors.push(
				`${path}: declared in docs/agent-contracts.json but missing, while its scope "${scope}" exists`
			);
		}
	}

	walk(root, root, (file) => {
		if (!file.endsWith(`${sep}AGENTS.md`) && relativePosix(root, file) !== 'AGENTS.md') return;
		const found = relativePosix(root, file);
		if (!declared.has(found)) {
			errors.push(
				`${found}: undeclared instruction file — add it to docs/agent-contracts.json or delete it`
			);
		}
	});

	return declared;
}

function checkContent(root, contract, source, errors) {
	const path = toPosix(contract.path);
	const scope = toPosix(contract.scope);
	const scopeDirectory = scope === '.' ? root : join(root, scope);
	const fileDirectory = dirname(join(root, path));
	const lines = source.split(/\r?\n/);

	// size
	const ceiling = contract.kind === 'root' ? MAX_LINES.root : MAX_LINES.scoped;
	if (lines.length > ceiling) {
		errors.push(
			`${path}: ${lines.length} lines exceeds the ${ceiling}-line ceiling for a ${contract.kind} contract`
		);
	}

	// absolute paths, secrets, transient markers
	lines.forEach((line, index) => {
		for (const { name, pattern } of ABSOLUTE_PATH_PATTERNS) {
			if (pattern.test(line)) errors.push(`${path}:${index + 1}: absolute local path (${name})`);
		}
		for (const { name, pattern } of SECRET_PATTERNS) {
			if (pattern.test(line)) errors.push(`${path}:${index + 1}: ${name}`);
		}
		for (const { name, pattern } of TRANSIENT_PATTERNS) {
			if (pattern.test(line)) {
				errors.push(`${path}:${index + 1}: transient ${name} in a durable instruction file`);
			}
		}
	});

	// inline and fenced repository paths
	for (const { token, line } of pathCandidates(source)) {
		const clean = token.replace(/\/$/, '');
		if (clean.split('/').some((segment) => GENERATED_SEGMENTS.has(segment))) continue;

		const first = clean.split('/')[0];
		const base = existsSync(join(scopeDirectory, first))
			? scopeDirectory
			: existsSync(join(root, first))
				? root
				: null;
		if (base === null) continue; // Not a repository path — a package name or a namespace.

		if (!existsSync(join(base, clean))) {
			errors.push(`${path}:${line}: stale repository path \`${token}\``);
		}
	}

	// Markdown links
	for (const { target, line } of linkTargets(source)) {
		if (isAbsolute(target)) {
			errors.push(`${path}:${line}: link target "${target}" is not repository-relative`);
			continue;
		}
		const resolved = resolve(fileDirectory, target);
		if (relative(root, resolved).startsWith('..')) {
			errors.push(`${path}:${line}: link target "${target}" escapes the repository`);
		} else if (!existsSync(resolved)) {
			errors.push(`${path}:${line}: broken link target "${target}"`);
		}
	}
}

function checkDuplication(rootSource, contract, source, errors) {
	const path = toPosix(contract.path);
	const rootLines = new Set(significantLines(rootSource).filter(Boolean));
	const scopedLines = significantLines(source);

	let run = 0;
	let shared = 0;
	let total = 0;

	scopedLines.forEach((line, index) => {
		if (line === null) {
			run = 0;
			return;
		}
		total += 1;
		if (rootLines.has(line)) {
			shared += 1;
			run += 1;
			if (run === MAX_SHARED_RUN + 1) {
				errors.push(
					`${path}:${index + 1}: ${run} consecutive lines copied from the root AGENTS.md — a scoped file states local deltas`
				);
			}
		} else {
			run = 0;
		}
	});

	if (total > 0 && shared / total > MAX_SHARED_LINE_RATIO) {
		const percent = Math.round((shared / total) * 100);
		errors.push(
			`${path}: ${percent}% of its substantial lines also appear in the root AGENTS.md (ceiling ${Math.round(
				MAX_SHARED_LINE_RATIO * 100
			)}%)`
		);
	}
}

function checkCommands(root, contract, source, localOnly, errors) {
	const path = toPosix(contract.path);
	const commands = requiredCommands(source);
	if (commands.length === 0) return;

	const ci = executedByCi(root);
	const scripts = packageScripts(root);

	for (const command of commands) {
		if (ci.includes(command)) continue;
		if (scripts.some((script) => script.includes(command))) continue;
		const declared = localOnly.find((entry) => entry.command === command);
		if (declared) {
			if (!declared.reason) {
				errors.push(
					`docs/agent-contracts.json: local-only command "${command}" needs a reason`
				);
			}
			continue;
		}
		errors.push(
			`${path}: required command "${command}" is run by no CI job and no package script — ` +
				`add it to CI, or declare it in docs/agent-contracts.json under "local_only_commands" with a reason`
		);
	}
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/**
 * Validates one repository tree. Returns the errors rather than exiting, so the
 * fixture tests can assert on them.
 */
export function validateAgentContracts({
	repoRoot,
	manifestPath = join(repoRoot, 'docs', 'agent-contracts.json')
} = {}) {
	const errors = [];

	if (!existsSync(manifestPath)) {
		return { errors: [`${toPosix(relative(repoRoot, manifestPath))}: manifest not found`] };
	}

	let manifest;
	try {
		manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
	} catch (cause) {
		return { errors: [`docs/agent-contracts.json: not valid JSON — ${cause.message}`] };
	}

	const contracts = Array.isArray(manifest.contracts) ? manifest.contracts : null;
	if (!contracts) {
		return { errors: ['docs/agent-contracts.json: "contracts" must be an array'] };
	}
	const localOnly = Array.isArray(manifest.local_only_commands) ? manifest.local_only_commands : [];

	checkInventory(repoRoot, contracts, errors);

	const rootContract = contracts.find((entry) => entry.kind === 'root');
	const rootPath = rootContract ? join(repoRoot, toPosix(rootContract.path)) : null;
	const rootSource = rootPath && existsSync(rootPath) ? readFileSync(rootPath, 'utf8') : null;

	for (const contract of contracts) {
		const file = join(repoRoot, toPosix(contract.path ?? ''));
		if (!contract.path || !existsSync(file)) continue;
		const source = readFileSync(file, 'utf8');

		checkContent(repoRoot, contract, source, errors);
		checkCommands(repoRoot, contract, source, localOnly, errors);
		if (contract.kind !== 'root' && rootSource !== null) {
			checkDuplication(rootSource, contract, source, errors);
		}
	}

	return { errors };
}

const invokedDirectly =
	process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url));

if (invokedDirectly) {
	const repoRoot = resolve(process.argv[2] ?? join(dirname(fileURLToPath(import.meta.url)), '..'));
	const { errors } = validateAgentContracts({ repoRoot });

	if (errors.length > 0) {
		for (const error of errors) console.error(`error: ${error}`);
		console.error(`\n${errors.length} agent-contract problem(s) in ${toPosix(repoRoot)}`);
		process.exit(1);
	}
	console.log('agent contracts: inventory, paths, secrets, size, duplication and commands all check out');
}
