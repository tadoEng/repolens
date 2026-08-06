import { COMPLETED_REPORT_FIXTURE, FAILED_RETRIABLE_FIXTURE } from '@repolens/api-client';
import { analysisScenario, createMockFetch, type RequestHandler } from '@repolens/msw';
import { afterEach, expect, test, vi } from 'vitest';

import * as transport from '$lib/api/analysis';
import { fetchAnalysis, fetchReport } from '$lib/api/analysis';

/**
 * The transport seam, against the shared MSW handlers.
 *
 * `$lib/api/analysis` no longer builds a URL: `api.GET('/api/v1/analyses/{analysis_id}')` is
 * typed against the generated `paths`, so a route that does not exist is a compile error and
 * needs no test. What still needs one is everything the generated client does *not* decide —
 * which of the three `Fetched` outcomes each kind of answer produces, and whether an
 * arbitrary id can escape its path segment on the way out.
 *
 * The handlers in `@repolens/msw` intercept `/api/v1/analyses/:id` and its `/report`, and
 * `createMockFetch` **throws** on an unmatched request rather than answering 404. So a client
 * that asks for a different URL than the contract's fails here loudly, instead of shipping
 * and reporting "analysis not found".
 *
 * Every read through this module is a `GET`, and that is a boundary rather than a
 * coincidence — the last test asserts the module exposes no mutation, now that the generated
 * `paths` publishes `POST /api/v1/analyses` and calling it would only be a compile error
 * away.
 */

const ANALYSIS_ID = COMPLETED_REPORT_FIXTURE.analysis.id;

/**
 * The error body a failure actually carries, taken from the fixture that carries one.
 *
 * `@repolens/msw` has no handler that answers an analysis read with an `ApiError`, and this
 * test must not invent the body — so it serves the exact `ApiError` the `failed-retriable`
 * fixture already contains. Written as a plain `Response` rather than an MSW handler because
 * `msw` is deliberately not a dependency of `web`; only `@repolens/msw` depends on it.
 */
const CONTRACT_ERROR = FAILED_RETRIABLE_FIXTURE.analysis.error;

const net = vi.hoisted(() => {
	/*
	 * `openapi-fetch` captures `globalThis.fetch` when the client is constructed, and
	 * `$lib/api/client` constructs it at import time — so a stub installed in a test would
	 * arrive after the capture and never be called. Hoisting installs one stable indirection
	 * instead: its identity never changes, so the capture stays valid while each test
	 * redirects where it dispatches.
	 */
	const real = globalThis.fetch.bind(globalThis);
	const state: { dispatch: typeof globalThis.fetch } = { dispatch: real };
	globalThis.fetch = (input, init) => state.dispatch(input, init);
	return { state, real };
});

afterEach(() => {
	net.state.dispatch = net.real;
});

function serve(handlers: RequestHandler[]): void {
	// No passthrough: an unmatched URL must be an error, not a quiet 404 from the network.
	net.state.dispatch = createMockFetch(handlers);
}

/** Answer every request with one fixed response, for bodies no shared handler produces. */
function answerWith(status: number, body: string, contentType: string): void {
	net.state.dispatch = () =>
		Promise.resolve(new Response(body, { status, headers: { 'content-type': contentType } }));
}

/** Record what the client asks for, then let the handlers answer it. */
function recordRequests(handlers: RequestHandler[]): Request[] {
	const issued: Request[] = [];
	const mock = createMockFetch(handlers);

	net.state.dispatch = (input, init) => {
		issued.push(input instanceof Request && init === undefined ? input : new Request(input, init));
		return mock(input, init);
	};

	return issued;
}

function onlyRequest(issued: Request[]): Request {
	const [first] = issued;
	if (!first || issued.length !== 1) {
		throw new Error(`expected exactly one request, saw ${issued.length}`);
	}
	return first;
}

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

test('the two reads ask for the two different paths the contract declares', async () => {
	// The generated client owns the URLs now, but "owns" is not "was observed to be right":
	// a report read that hit the analysis path would still return a well-typed 200 here,
	// because both fixtures answer for any id.
	const issued = recordRequests(analysisScenario('completed-report'));

	await fetchAnalysis(ANALYSIS_ID);
	await fetchReport(ANALYSIS_ID);

	expect(issued.map((request) => new URL(request.url).pathname)).toEqual([
		`/api/v1/analyses/${ANALYSIS_ID}`,
		// Nested under the analysis: a report is identified by the analysis that produced it,
		// and there is no second identifier for it.
		`/api/v1/analyses/${ANALYSIS_ID}/report`
	]);
});

test('an error status is rejected with the parsed ApiError, not swallowed', async () => {
	answerWith(503, JSON.stringify(CONTRACT_ERROR), 'application/json');

	const result = await fetchAnalysis(ANALYSIS_ID);

	expect(result.kind).toBe('rejected');
	if (result.kind !== 'rejected') return;
	expect(result.status).toBe(503);
	// Verbatim, including `retry_after_seconds`: a countdown the server sent is the one piece
	// of a failure a reader can act on, and dropping it is a silent downgrade.
	expect(result.error).toEqual(CONTRACT_ERROR);
});

test('an error body that is not an ApiError is rejected with no error, not thrown', async () => {
	// The realistic shape of this: a proxy in front of the API answering with its own HTML
	// page. The status is the only trustworthy fact in it, so it is the only one kept.
	answerWith(502, '<html><body>502 Bad Gateway</body></html>', 'text/html');

	const result = await fetchAnalysis(ANALYSIS_ID);

	expect(result.kind).toBe('rejected');
	if (result.kind !== 'rejected') return;
	expect(result.status).toBe(502);
	expect(result.error).toBeNull();
});

test('a success status carrying an unusable body is rejected with that status', async () => {
	// A 200 whose body is not the object the contract promises is a server fault, not a
	// missing resource. Reporting the status says which; returning `ok` would hand a
	// component a `string` typed as an `Analysis`.
	answerWith(200, JSON.stringify('not an analysis'), 'application/json');

	const result = await fetchAnalysis(ANALYSIS_ID);

	expect(result.kind).toBe('rejected');
	if (result.kind !== 'rejected') return;
	expect(result.status).toBe(200);
	expect(result.error).toBeNull();
});

test('a report that does not exist yet is rejected with 404, not treated as unreachable', async () => {
	// Every state before COMPLETED has no report. Collapsing this into "unreachable" would
	// report a perfectly healthy in-flight analysis as a network failure.
	serve(analysisScenario('queued'));

	const result = await fetchReport(ANALYSIS_ID);

	expect(result.kind).toBe('rejected');
	if (result.kind !== 'rejected') return;
	expect(result.status).toBe(404);
	// The handler answers with an empty body, and nothing is invented to fill it.
	expect(result.error).toBeNull();
});

test('a request that never reaches a server is unreachable, not rejected', async () => {
	// No handlers: `createMockFetch` throws, exactly as a real transport failure would — and
	// exactly as `openapi-fetch` propagates one, since a thrown `fetch` never becomes an
	// `error` on the result.
	serve([]);

	const result = await fetchAnalysis(ANALYSIS_ID);

	/*
	 * Three outcomes rather than two, because a CSP `connect-src` mismatch and a missing
	 * analysis are different facts about the deployment — and the first is the one that
	 * gets misdiagnosed as the second.
	 */
	expect(result.kind).toBe('unreachable');
});

test('an id with URL-significant characters is encoded, not interpolated raw', async () => {
	// An id is opaque. Pasting one into a path without encoding is how a single segment
	// becomes two segments and a query string — asserted on the request the client actually
	// issued, since the encoding is now the generated client's job rather than this module's.
	const issued = recordRequests(analysisScenario('completed-report'));

	const result = await fetchAnalysis('a/b?c');

	const url = new URL(onlyRequest(issued).url);
	expect(url.pathname).toBe('/api/v1/analyses/a%2Fb%3Fc');
	expect(url.search).toBe('');
	// And the handler still matched it: an id that had escaped its segment would miss
	// `/api/v1/analyses/:id`, which `createMockFetch` reports by throwing.
	expect(result.kind).toBe('ok');
});

test('the module exposes no mutation, and every request it makes is a GET', async () => {
	/*
	 * The blocker this locks down. A hand-written `POST …/retry` shipped here once: absent
	 * from OpenAPI, absent from the MSW handlers, carrying no Firebase credential, with no
	 * declared idempotency semantics — and it *starts paid work*. The generated `paths` now
	 * publishes `POST /api/v1/analyses`, so the compiler no longer stands in the way of
	 * re-adding one; this test does.
	 *
	 * Asserted over the module's own surface rather than by naming the function that was
	 * deleted, so re-adding it under any name fails here.
	 */
	const exported = Object.keys(transport);
	expect(exported).not.toContain('retryPath');
	expect(exported.filter((name) => /retry|create|submit|cancel|delete/i.test(name))).toEqual([]);

	const issued = recordRequests(analysisScenario('completed-report'));

	await fetchAnalysis(ANALYSIS_ID);
	await fetchReport(ANALYSIS_ID);

	expect(issued.map((request) => request.method)).toEqual(['GET', 'GET']);
});
