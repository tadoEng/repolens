/**
 * MSW handlers for the analysis and report endpoints.
 *
 * `GET /api/v1/analyses/{id}` and `GET /api/v1/analyses/{id}/report` do not exist
 * server-side yet — issue #6 owns the routes. The contract they will serve does exist, as
 * `contracts/fixtures/analysis-v1/*.json`, which is why these handlers can be written now
 * without inventing anything.
 *
 * **Every body served here comes from a fixture.** Not a copy of one: the fixtures are
 * bound into TypeScript by `@repolens/api-client`, which generates that binding from the
 * JSON and fails CI if the two disagree. So a fixture edited in `contracts/` changes what
 * these handlers return, and there is no second place to update. That is the difference
 * between a mock and a guess.
 */

import {
	ANALYSIS_FIXTURES,
	COMPLETED_REPORT_FIXTURE,
	QUEUED_FIXTURE,
	RESOLVING_FIXTURE,
	type Analysis,
	type AnalysisFixture,
	type AnalysisFixtureName,
	type AnalysisState,
	type Report
} from '@repolens/api-client';
import { HttpResponse, http, type RequestHandler } from 'msw';

import { apiUrl, type HandlerOptions } from './api-url';

/**
 * `:id` is MSW's path-parameter syntax, not the OpenAPI document's `{id}`.
 *
 * The two never match across a `/`, so the analysis path cannot swallow a request for the
 * report path — ordering between the two handlers is therefore not load-bearing.
 */
const ANALYSIS_PATH = '/api/v1/analyses/:id';
const REPORT_PATH = '/api/v1/analyses/:id/report';

interface AnalysisPathParams {
	id: string;
}

/**
 * The analysis id every fixture carries.
 *
 * Exported because the handlers answer for *any* id and always serve the fixture verbatim.
 * Rewriting `analysis.id` to echo the requested path would make the response no longer
 * equal to the fixture, and the fixture is the contract; a test that needs the body and the
 * URL to agree should build the URL from this constant instead.
 */
export const FIXTURE_ANALYSIS_ID: string = QUEUED_FIXTURE.analysis.id;

function analysisHandler(analysis: Analysis, options: HandlerOptions): RequestHandler {
	// The third type argument makes the resolver's return type the generated DTO, so a body
	// that no longer matches the contract is a type error rather than a stale mock.
	return http.get<AnalysisPathParams, never, Analysis>(apiUrl(ANALYSIS_PATH, options), () =>
		HttpResponse.json(analysis)
	);
}

function reportHandler(report: Report | undefined, options: HandlerOptions): RequestHandler {
	const url = apiUrl(REPORT_PATH, options);

	if (!report) {
		// 404 with an empty body. The contract declares no error schema for this path, and
		// `ErrorCode` has no member meaning "this analysis produced no report" — inventing
		// one here is exactly the drift this package exists to prevent. Untyped for the same
		// reason MSW requires: a handler declaring a response body type must always return
		// one.
		return http.get(url, () => new HttpResponse(null, { status: 404 }));
	}

	return http.get<AnalysisPathParams, never, Report>(url, () => HttpResponse.json(report));
}

/**
 * Both endpoints for one fixture.
 *
 * The report endpoint answers 404 whenever the fixture has no report, which is every state
 * before `COMPLETED`. A handler set that served a report for a `QUEUED` analysis would let
 * a UI skip the state it will spend most of its time in.
 */
export function analysisFixtureHandlers(
	fixture: AnalysisFixture,
	options: HandlerOptions = {}
): RequestHandler[] {
	return [analysisHandler(fixture.analysis, options), reportHandler(fixture.report, options)];
}

/**
 * Every fixture, available as a scenario by name.
 *
 * One function taking a name rather than six hand-written scenario functions: the name is
 * typed as `AnalysisFixtureName`, so the set of scenarios is derived from the fixture
 * directory and cannot fall behind it. Six wrappers would each need remembering when a
 * seventh fixture lands, and the one nobody remembers is the scenario that stays untested.
 */
export function analysisScenario(
	name: AnalysisFixtureName,
	options: HandlerOptions = {}
): RequestHandler[] {
	return analysisFixtureHandlers(ANALYSIS_FIXTURES[name], options);
}

/**
 * The states a successful analysis passes through, in order.
 *
 * `satisfies readonly AnalysisState[]` rather than a bare array: a state renamed in the
 * Rust enum fails to compile here instead of producing a progress UI that silently stops
 * advancing.
 */
export const POLLING_SEQUENCE_STATES = [
	'QUEUED',
	'RESOLVING',
	'COLLECTING',
	'ANALYZING',
	'BUILDING_REPORT',
	'COMPLETED'
] as const satisfies readonly AnalysisState[];

/**
 * The polling sequence as plain analyses, for tests that drive a store directly.
 *
 * `QUEUED`, `RESOLVING` and `COMPLETED` are served from their fixtures verbatim. The three
 * working states in between have no fixture of their own, so they are derived from the
 * completed fixture with the state, retry policy and polling hint of an in-flight analysis.
 * Deriving rather than authoring keeps every field a value the contract already produced —
 * in particular `commit_sha`, which is resolved by `COLLECTING` and null before it.
 */
export function pollingSequenceAnalyses(): Analysis[] {
	const queued = QUEUED_FIXTURE.analysis;
	const resolving = RESOLVING_FIXTURE.analysis;
	const completed = COMPLETED_REPORT_FIXTURE.analysis;

	return POLLING_SEQUENCE_STATES.map((state): Analysis => {
		if (state === 'QUEUED') return queued;
		if (state === 'RESOLVING') return resolving;
		if (state === 'COMPLETED') return completed;

		return {
			...completed,
			state,
			retry: queued.retry,
			poll_after_ms: queued.poll_after_ms,
			// Explicit rather than inherited from the completed fixture: report availability
			// is a separate fact from analysis state, and a progress UI that saw it true
			// early would offer a link to a report that does not exist yet.
			report_available: false
		};
	});
}

/**
 * A progress run: `QUEUED → RESOLVING → COLLECTING → ANALYZING → BUILDING_REPORT →
 * COMPLETED`, one state per request.
 *
 * The sequence advances on each poll and then stays on `COMPLETED`. It does not wrap: a
 * terminal state that reverted to `QUEUED` would let a polling loop run forever, and would
 * let a UI bug that ignores terminal states pass its test.
 *
 * A factory, and stateful. Each call owns its own cursor, so tests must not share one
 * handler set between cases — which is also why this is not part of `createHandlers`.
 */
export function pollingSequence(options: HandlerOptions = {}): RequestHandler[] {
	const analyses = pollingSequenceAnalyses();
	const report: Report | undefined = COMPLETED_REPORT_FIXTURE.report;

	let next = 0;
	let servedState: AnalysisState | null = null;

	return [
		http.get<AnalysisPathParams, never, Analysis>(apiUrl(ANALYSIS_PATH, options), () => {
			const position = Math.min(next, analyses.length - 1);
			const analysis = analyses[position];

			if (!analysis) {
				// Unreachable — `POLLING_SEQUENCE_STATES` is non-empty — but `noUncheckedIndexedAccess`
				// is on, and throwing beats a non-null assertion that would silently serve
				// `undefined` if that ever stopped being true.
				throw new Error('pollingSequence produced no analyses');
			}

			next = position + 1;
			servedState = analysis.state;
			return HttpResponse.json(analysis);
		}),
		http.get(apiUrl(REPORT_PATH, options), () => {
			// Keyed off what was actually served, not off the cursor: the report becomes
			// available when the client has *observed* completion, which is the sequencing a
			// polling UI has to get right.
			if (servedState !== 'COMPLETED' || !report) {
				return new HttpResponse(null, { status: 404 });
			}
			return HttpResponse.json(report);
		})
	];
}
