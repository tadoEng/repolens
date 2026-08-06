import { COMPLETED_REPORT_FIXTURE } from '@repolens/api-client';
import { analysisScenario, createMockFetch, type RequestHandler } from '@repolens/msw';
import { afterEach, expect, test } from 'vitest';

import * as transport from '$lib/api/analysis';
import { analysisPath, apiUrl, fetchAnalysis, fetchReport, reportPath } from '$lib/api/analysis';

/**
 * The transport seam, against the shared MSW handlers.
 *
 * `$lib/api/analysis` hand-writes exactly one thing — the paths — because issue #6 has not
 * landed the endpoints and the generated client therefore has no operation to call. These
 * tests are what keeps that from drifting: the handlers in `@repolens/msw` intercept
 * `/api/v1/analyses/:id` and `/api/v1/analyses/:id/report`, and `createMockFetch` **throws**
 * on an unmatched request rather than answering 404. So a client that builds a different
 * URL fails here loudly, instead of shipping and reporting "analysis not found".
 *
 * The provisional seam is acceptable **only because every path through it is a `GET`**: a
 * read is idempotent, is anonymous by design, and is pinned to a matching mock. A mutation
 * is none of those, so this file also asserts that the module exposes none.
 */

const ANALYSIS_ID = COMPLETED_REPORT_FIXTURE.analysis.id;

const realFetch = globalThis.fetch.bind(globalThis);

afterEach(() => {
	globalThis.fetch = realFetch;
});

function serve(handlers: RequestHandler[]): void {
	// No passthrough: an unmatched URL must be an error, not a quiet 404 from the network.
	globalThis.fetch = createMockFetch(handlers);
}

test('the paths are the ones the shared handlers intercept', () => {
	expect(analysisPath(ANALYSIS_ID)).toBe(`/api/v1/analyses/${ANALYSIS_ID}`);
	// Nested under the analysis: a report is identified by the analysis that produced it,
	// and there is no second identifier for it.
	expect(reportPath(ANALYSIS_ID)).toBe(`/api/v1/analyses/${ANALYSIS_ID}/report`);

	// The origin comes from PUBLIC_API_ORIGIN and is never hardcoded, so assert the shape.
	expect(apiUrl(analysisPath(ANALYSIS_ID))).toMatch(
		new RegExp(`^https?://\\S+/api/v1/analyses/${ANALYSIS_ID}$`)
	);
});

test('the module exposes no mutation, and every request it makes is a GET', async () => {
	/*
	 * The blocker this locks down. A hand-written `POST …/retry` shipped here once: absent
	 * from OpenAPI, absent from the MSW handlers, carrying no Firebase credential, with no
	 * declared idempotency semantics — and it *starts paid work*. A provisional path is
	 * defensible for a read and is not defensible for that.
	 *
	 * Asserted over the module's own surface rather than by naming the function that was
	 * deleted, so re-adding it under any name fails here.
	 */
	const exported = Object.keys(transport);
	expect(exported).not.toContain('retryPath');
	expect(exported.filter((name) => /retry|create|submit|cancel|delete/i.test(name))).toEqual([]);

	const methods: (string | undefined)[] = [];
	const mock = createMockFetch(analysisScenario('completed-report'));
	globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
		methods.push(init?.method);
		return mock(input, init);
	}) as typeof globalThis.fetch;

	await fetchAnalysis(ANALYSIS_ID);
	await fetchReport(ANALYSIS_ID);

	// Explicit, not defaulted: `request` fixes the method so that adding one is a visible
	// change to this file rather than a new argument at a call site.
	expect(methods).toEqual(['GET', 'GET']);
});

test('an id with URL-significant characters is encoded, not interpolated raw', () => {
	// An id is opaque. Pasting one into a template without encoding is how a path segment
	// becomes two.
	expect(analysisPath('a/b?c')).toBe('/api/v1/analyses/a%2Fb%3Fc');
});

test('fetchAnalysis returns the fixture analysis verbatim', async () => {
	serve(analysisScenario('completed-report'));

	const result = await fetchAnalysis(ANALYSIS_ID);

	expect(result.kind).toBe('ok');
	if (result.kind !== 'ok') return;
	expect(result.value).toEqual(COMPLETED_REPORT_FIXTURE.analysis);
});

test('fetchReport returns the fixture report verbatim', async () => {
	serve(analysisScenario('completed-report'));

	const result = await fetchReport(ANALYSIS_ID);

	expect(result.kind).toBe('ok');
	if (result.kind !== 'ok') return;
	expect(result.value).toEqual(COMPLETED_REPORT_FIXTURE.report);
});

test('a report that does not exist yet is rejected with 404, not treated as unreachable', async () => {
	// Every state before COMPLETED has no report. Collapsing this into "unreachable" would
	// report a perfectly healthy in-flight analysis as a network failure.
	serve(analysisScenario('queued'));

	const result = await fetchReport(ANALYSIS_ID);

	expect(result.kind).toBe('rejected');
	if (result.kind !== 'rejected') return;
	expect(result.status).toBe(404);
	// The contract declares no error schema for this path, so there is nothing to parse —
	// and nothing is invented.
	expect(result.error).toBeNull();
});

test('a request that never reaches a server is unreachable, not rejected', async () => {
	// No handlers: `createMockFetch` throws, exactly as a real transport failure would.
	serve([]);

	const result = await fetchAnalysis(ANALYSIS_ID);

	/*
	 * Three outcomes rather than two, because a CSP `connect-src` mismatch and a missing
	 * analysis are different facts about the deployment — and the first is the one that
	 * gets misdiagnosed as the second.
	 */
	expect(result.kind).toBe('unreachable');
});
