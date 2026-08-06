/**
 * Fixture tests for the agent-contract guard.
 *
 * Each case builds a throwaway repository tree, breaks exactly one thing, and
 * asserts the guard names it. A guard nobody has watched fail is decoration, so
 * every rule in `check-agent-contracts.mjs` has a case here and the valid tree
 * is asserted clean — otherwise a rule that always fires would look like
 * thoroughness.
 *
 * Paths are written in both POSIX and Windows form where the distinction can
 * matter, because the repository is developed on Windows and gated on Linux.
 */

import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { after, describe, test } from 'node:test';

import { validateAgentContracts } from './check-agent-contracts.mjs';

const temporaryRoots = [];

after(() => {
	for (const root of temporaryRoots) {
		rmSync(root, { recursive: true, force: true });
	}
});

/** Materializes `{ 'relative/path': 'contents' }` into a fresh directory. */
function tree(files) {
	const root = mkdtempSync(join(tmpdir(), 'repolens-agent-contracts-'));
	temporaryRoots.push(root);
	for (const [path, contents] of Object.entries(files)) {
		const target = join(root, ...path.split('/'));
		mkdirSync(dirname(target), { recursive: true });
		writeFileSync(target, contents, 'utf8');
	}
	return root;
}

function manifest(contracts, extra = {}) {
	return `${JSON.stringify({ contracts, ...extra }, null, 2)}\n`;
}

const VALID_CONTRACTS = [
	{ path: 'AGENTS.md', scope: '.', kind: 'root' },
	{ path: 'crates/demo/AGENTS.md', scope: 'crates/demo', kind: 'scoped' }
];

const ROOT_FILE = `# Demo

The repository map lives in \`crates/demo\` and the pipeline is generated.

## Verification

\`\`\`sh
cargo test --workspace --locked
\`\`\`

See [the crate contract](crates/demo/AGENTS.md).
`;

const SCOPED_FILE = `# demo

> The nearest \`AGENTS.md\` governs this subtree. It extends the root
> [\`AGENTS.md\`](../../AGENTS.md) rather than replacing it.

Local rule: this crate never opens a socket, and \`src/lib.rs\` is the whole of it.

## Commands

\`\`\`sh
cargo test -p demo
\`\`\`
`;

const WORKFLOW = `name: CI
jobs:
  rust:
    steps:
      # A comment mentioning cargo doc --workspace --no-deps must not count.
      - run: cargo test --workspace --locked
`;

/** A tree that must validate clean; individual cases override one entry. */
function baseline(overrides = {}) {
	return tree({
		'docs/agent-contracts.json': manifest(VALID_CONTRACTS),
		'AGENTS.md': ROOT_FILE,
		'crates/demo/AGENTS.md': SCOPED_FILE,
		'crates/demo/src/lib.rs': '// demo\n',
		'.github/workflows/ci.yml': WORKFLOW,
		'package.json': '{ "name": "demo", "scripts": { "check": "tsc --noEmit" } }\n',
		...overrides
	});
}

function errorsFor(root) {
	return validateAgentContracts({ repoRoot: root }).errors;
}

function assertMentions(errors, needle) {
	assert.ok(
		errors.some((error) => error.includes(needle)),
		`expected an error mentioning ${JSON.stringify(needle)}, got:\n${errors.join('\n') || '(none)'}`
	);
}

describe('agent-contract guard', () => {
	test('a valid root-plus-scoped instruction set passes', () => {
		assert.deepEqual(errorsFor(baseline()), []);
	});

	test('a declared scoped file that does not exist fails', () => {
		const root = tree({
			'docs/agent-contracts.json': manifest(VALID_CONTRACTS),
			'AGENTS.md': ROOT_FILE,
			'crates/demo/src/lib.rs': '// demo\n',
			'.github/workflows/ci.yml': WORKFLOW
		});
		assertMentions(errorsFor(root), 'crates/demo/AGENTS.md: declared');
	});

	test('a declared scope that does not exist fails', () => {
		const root = baseline({
			'docs/agent-contracts.json': manifest([
				...VALID_CONTRACTS,
				{ path: 'web/AGENTS.md', scope: 'web', kind: 'scoped' }
			])
		});
		assertMentions(errorsFor(root), 'declared scope "web" does not exist');
	});

	test('an undeclared nested instruction file fails', () => {
		const root = baseline({ 'web/AGENTS.md': '# web\n' });
		assertMentions(errorsFor(root), 'web/AGENTS.md: undeclared instruction file');
	});

	test('a stale repository path fails', () => {
		const root = baseline({
			'crates/demo/AGENTS.md': SCOPED_FILE.replace('src/lib.rs', 'src/deleted.rs')
		});
		assertMentions(errorsFor(root), 'stale repository path `src/deleted.rs`');
	});

	test('a broken Markdown link fails', () => {
		const root = baseline({
			'AGENTS.md': ROOT_FILE.replace('crates/demo/AGENTS.md)', 'crates/gone/AGENTS.md)')
		});
		assertMentions(errorsFor(root), 'broken link target "crates/gone/AGENTS.md"');
	});

	test('an absolute developer path fails in both platform forms', () => {
		const windows = baseline({
			'crates/demo/AGENTS.md': `${SCOPED_FILE}\nClone into C:\\Users\\dev\\repolens first.\n`
		});
		assertMentions(errorsFor(windows), 'absolute local path (Windows drive path)');

		const posix = baseline({
			'crates/demo/AGENTS.md': `${SCOPED_FILE}\nClone into /home/dev/repolens first.\n`
		});
		assertMentions(errorsFor(posix), 'absolute local path (POSIX home path)');
	});

	test('a populated secret assignment fails', () => {
		const root = baseline({
			'crates/demo/AGENTS.md': `${SCOPED_FILE}\nRun with GH_ANALYSIS_TOKEN=ghp_notarealtokenbutshapedlikeone01 set.\n`
		});
		assertMentions(errorsFor(root), 'secret-named environment assignment with a value');
	});

	test('a private-key marker fails', () => {
		const root = baseline({
			'crates/demo/AGENTS.md': `${SCOPED_FILE}\n-----BEGIN RSA PRIVATE KEY-----\n`
		});
		assertMentions(errorsFor(root), 'private-key marker');
	});

	test('a transient commit SHA fails', () => {
		const root = baseline({
			'AGENTS.md': `${ROOT_FILE}\nVerified at 0584a2df65968a4e9e6859ef46bbed430408a3f1.\n`
		});
		assertMentions(errorsFor(root), 'transient commit SHA');
	});

	test('a branch-shaped token fails but a dotted path does not', () => {
		const root = baseline({
			'AGENTS.md': `${ROOT_FILE}\nMerge feat/report-ui before reading this.\n`
		});
		assertMentions(errorsFor(root), 'transient branch-shaped token');

		const dotted = baseline({
			'AGENTS.md': `${ROOT_FILE}\nThe manifest is docs/agent-contracts.json and nothing else.\n`,
			'docs/agent-contracts.json': manifest(VALID_CONTRACTS)
		});
		assert.deepEqual(errorsFor(dotted), []);
	});

	test('an oversized root file fails', () => {
		const filler = Array.from({ length: 300 }, (_, index) => `Line ${index} of padding.`).join(
			'\n'
		);
		const root = baseline({ 'AGENTS.md': `${ROOT_FILE}\n${filler}\n` });
		assertMentions(errorsFor(root), 'exceeds the 260-line ceiling');
	});

	test('a scoped file that copies the root fails', () => {
		const copied = [
			'The pipeline is generated end to end and never hand-edited by anyone.',
			'Database rows are never public data-transfer objects on the wire.',
			'Severity and confidence are separate axes and are never merged into one.',
			'Unknown enum values render in a neutral fallback naming the raw value.'
		].join('\n');
		const root = baseline({
			'AGENTS.md': `${ROOT_FILE}\n${copied}\n`,
			'crates/demo/AGENTS.md': `${SCOPED_FILE}\n${copied}\n`
		});
		const errors = errorsFor(root);
		assertMentions(errors, 'consecutive lines copied from the root AGENTS.md');
		assertMentions(errors, 'also appear in the root AGENTS.md');
	});

	test('a required command no CI job runs fails', () => {
		const root = baseline({
			'AGENTS.md': ROOT_FILE.replace('cargo test --workspace --locked', 'cargo test --all-features')
		});
		assertMentions(errorsFor(root), 'required command "cargo test --all-features" is run by no CI job');
	});

	test('a command mentioned only in a CI comment does not count as executed', () => {
		const root = baseline({
			'AGENTS.md': ROOT_FILE.replace(
				'cargo test --workspace --locked',
				'cargo doc --workspace --no-deps'
			)
		});
		assertMentions(errorsFor(root), 'required command "cargo doc --workspace --no-deps"');
	});

	test('a local-only command with a reason is accepted', () => {
		const root = baseline({
			'AGENTS.md': ROOT_FILE.replace(
				'cargo test --workspace --locked',
				'cargo doc --workspace --no-deps'
			),
			'docs/agent-contracts.json': manifest(VALID_CONTRACTS, {
				local_only_commands: [
					{ command: 'cargo doc --workspace --no-deps', reason: 'no CI job runs rustdoc' }
				]
			})
		});
		assert.deepEqual(errorsFor(root), []);
	});

	test('a local-only command without a reason fails', () => {
		const root = baseline({
			'AGENTS.md': ROOT_FILE.replace(
				'cargo test --workspace --locked',
				'cargo doc --workspace --no-deps'
			),
			'docs/agent-contracts.json': manifest(VALID_CONTRACTS, {
				local_only_commands: [{ command: 'cargo doc --workspace --no-deps' }]
			})
		});
		assertMentions(errorsFor(root), 'needs a reason');
	});

	test('manifest paths declared in Windows form validate the same tree', () => {
		const root = baseline({
			'docs/agent-contracts.json': manifest([
				{ path: 'AGENTS.md', scope: '.', kind: 'root' },
				{ path: 'crates\\demo\\AGENTS.md', scope: 'crates\\demo', kind: 'scoped' }
			])
		});
		assert.deepEqual(errorsFor(root), []);
	});

	test('a missing manifest is reported rather than passing silently', () => {
		const root = tree({ 'AGENTS.md': ROOT_FILE });
		assertMentions(errorsFor(root), 'manifest not found');
	});
});
