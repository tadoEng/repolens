import { COMPLETED_REPORT_FIXTURE } from '@repolens/api-client';
import { analysisScenario, createMockFetch, type RequestHandler } from '@repolens/msw';
import { afterEach, expect, test } from 'vitest';

import {
	analysisPath,
	apiUrl,
	fetchAnalysis,
	fetchReport,
	reportPath,
	retryPath
} from '$lib/api/analysis';

/**
 * The transport seam, against the shared MSW handlers.
 *
 * `$lib/api/analysis` hand-writes exactly one thing — the paths — because issue #6 has not
 * landed the endpoints and the generated client therefore has no operation to call. These
 * tests are what keeps that from drifting: the handlers in `@repolens/msw` intercept
 * `/api/v1/analyses/:id` and `/api/v1/analyses/:id/report`, and `createMockFetch` **throws**
 * on an unmatched request rather than answering 404. So a client that builds a different
 * URL fails here loudly, instead of shipping and reporting "analysis not found".
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
	expect(retryPath(ANALYSIS_ID)).toBe(`/api/v1/analyses/${ANALYSIS_ID}/retry`);

	// The origin comes from PUBLIC_API_ORIGIN and is never hardcoded, so assert the shape.
	expect(apiUrl(analysisPath(ANALYSIS_ID))).toMatch(
		new RegExp(`^https?://\\S+/api/v1/analyses/${ANALYSIS_ID}$`)
	);
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
