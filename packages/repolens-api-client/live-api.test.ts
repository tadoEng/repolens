/**
 * Drives a real, running API through the generated client.
 *
 * **Skipped unless `REPOLENS_LIVE_API_ORIGIN` is set**, because it needs a
 * server, a database, and the network. CI never sets it.
 *
 * It exists because the other gates each stop short of this. `schema.test.ts`
 * proves the generated types match the committed document; the browser
 * integration suite proves the probe is reachable over a real socket with real
 * CSP and CORS — but it runs against a server with no database, so it can never
 * touch an analysis. Neither shows the analysis operations, *as generated*,
 * driving a real run.
 *
 * Typed end to end and deliberately free of casts: every field read below has
 * to exist in `schema.ts` or `tsc` refuses the file. A hand-written `fetch`
 * here would prove the server works while proving nothing about the contract,
 * which is the only thing this file is for.
 *
 * ```sh
 * REPOLENS_LIVE_API_ORIGIN=http://localhost:8080 \
 *   pnpm --filter @repolens/api-client test
 * ```
 */

import { describe, expect, it } from 'vitest';

import { createRepoLensClient } from './src/client';

const origin = process.env.REPOLENS_LIVE_API_ORIGIN;
const repository =
	process.env.REPOLENS_LIVE_API_REPOSITORY ?? 'https://github.com/rust-lang/crates.io';

describe.skipIf(origin === undefined || origin === '')('against a live API', () => {
	it('creates, polls, and reads a report through the generated operations', async () => {
		const client = createRepoLensClient({ baseUrl: origin });

		const created = await client.POST('/api/v1/analyses', {
			body: { repository_url: repository }
		});

		expect(created.error, JSON.stringify(created.error)).toBeUndefined();
		const analysis = created.data;
		if (analysis === undefined) throw new Error('unreachable: checked above');

		expect(created.response.status).toBe(202);
		// Nullable during QUEUED and RESOLVING: there is no commit until one is
		// resolved, and the header has to render "resolving…" rather than blank.
		expect(analysis.commit_sha).toBeNull();
		expect(analysis.state).toBe('QUEUED');
		expect(analysis.retry.allowed).toBe(false);

		const observed: string[] = [analysis.state];
		let state = analysis.state;
		let pollAfterMs = analysis.poll_after_ms ?? 1500;

		for (let attempt = 0; attempt < 200 && !isTerminal(state); attempt += 1) {
			await new Promise((resolve) => setTimeout(resolve, Math.min(pollAfterMs, 250)));

			const progress = await client.GET('/api/v1/analyses/{analysis_id}', {
				params: { path: { analysis_id: analysis.id } }
			});
			expect(progress.error, JSON.stringify(progress.error)).toBeUndefined();
			const current = progress.data;
			if (current === undefined) throw new Error('unreachable: checked above');

			if (current.state !== state) {
				state = current.state;
				observed.push(state);
			}
			pollAfterMs = current.poll_after_ms ?? pollAfterMs;
		}

		expect(observed, `observed ${observed.join(' -> ')}`).toContain('COMPLETED');

		const fetched = await client.GET('/api/v1/analyses/{analysis_id}/report', {
			params: { path: { analysis_id: analysis.id } }
		});
		expect(fetched.error, JSON.stringify(fetched.error)).toBeUndefined();
		const report = fetched.data;
		if (report === undefined) throw new Error('unreachable: checked above');

		expect(report.analysis_id).toBe(analysis.id);
		expect(report.commit_sha).toMatch(/^[0-9a-f]{40}$/);
		expect(report.tree_sha).toMatch(/^[0-9a-f]{40}$/);

		// A commit and its root tree are different objects. GitHub's tree endpoint
		// echoes back whichever SHA it was asked for, so reading the listing's own
		// `sha` after fetching by commit writes the commit SHA into both halves of
		// the identity — well-formed, and carrying no tree.
		expect(report.tree_sha).not.toBe(report.commit_sha);

		expect(report.findings.length).toBeGreaterThan(0);
		// Absence of evidence is not evidence of absence: this ruleset reads paths
		// only, and says so at report level rather than per finding.
		expect(report.limitations.length).toBeGreaterThan(0);
		// Null, not zero. Line counts need the archive path, which is issue #12,
		// and zero would be a measurement nobody took.
		expect(report.composition).toBeNull();
	}, 120_000);
});

function isTerminal(state: string): boolean {
	return state === 'COMPLETED' || state === 'FAILED_RETRIABLE' || state === 'FAILED_PERMANENT';
}
