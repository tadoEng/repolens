/**
 * What the analysis handlers actually serve.
 *
 * `tsc` already proves the bodies match the contract — that is what the generated types are
 * for. These tests cover what types cannot: that the URLs match, that a fixture without a
 * report answers 404 rather than something plausible-looking, and that the polling sequence
 * advances exactly once per request and then stops.
 *
 * Every expected body is read from `@repolens/api-client`, never written out here. A test
 * that re-declared a fixture would pass while the handler served something else entirely.
 */

import { ANALYSIS_FIXTURES, ANALYSIS_FIXTURE_NAMES } from '@repolens/api-client';
import { describe, expect, test } from 'vitest';

import {
	FIXTURE_ANALYSIS_ID,
	POLLING_SEQUENCE_STATES,
	analysisScenario,
	pollingSequence,
	pollingSequenceAnalyses
} from './src/analysis-handlers';
import { createMockFetch } from './src/mock-fetch';

const ORIGIN = 'https://api.repolens.test';
const ANALYSIS_URL = `${ORIGIN}/api/v1/analyses/${FIXTURE_ANALYSIS_ID}`;
const REPORT_URL = `${ANALYSIS_URL}/report`;

describe('analysisScenario', () => {
	test('every fixture is reachable as a scenario', () => {
		// The scenario surface is derived from the fixture directory, so this asserts the
		// derivation still holds rather than re-listing the six names.
		expect(ANALYSIS_FIXTURE_NAMES).toHaveLength(Object.keys(ANALYSIS_FIXTURES).length);
		expect(ANALYSIS_FIXTURE_NAMES.length).toBeGreaterThanOrEqual(6);
	});

	for (const name of ANALYSIS_FIXTURE_NAMES) {
		describe(name, () => {
			const fixture = ANALYSIS_FIXTURES[name];

			test('serves the fixture analysis verbatim', async () => {
				const fetch = createMockFetch(analysisScenario(name, { apiOrigin: ORIGIN }));

				const response = await fetch(ANALYSIS_URL);

				expect(response.status).toBe(200);
				await expect(response.json()).resolves.toEqual(fixture.analysis);
			});

			test('serves the report only when the fixture has one', async () => {
				const fetch = createMockFetch(analysisScenario(name, { apiOrigin: ORIGIN }));

				const response = await fetch(REPORT_URL);

				if (fixture.report) {
					expect(response.status).toBe(200);
					await expect(response.json()).resolves.toEqual(fixture.report);
				} else {
					// Empty body on purpose: the contract declares no error schema for this
					// path, and `ErrorCode` has no member that means "no report".
					expect(response.status).toBe(404);
					await expect(response.text()).resolves.toBe('');
				}
			});
		});
	}

	test('matches any origin when none is given', async () => {
		const fetch = createMockFetch(analysisScenario('queued'));

		const response = await fetch(`https://somewhere-else.example/api/v1/analyses/abc`);

		expect(response.status).toBe(200);
	});

	test('the report path is not swallowed by the analysis path', async () => {
		// `:id` never matches across a `/`. If it did, the report request would resolve to
		// the analysis handler and a test would compare a report against an analysis.
		const fetch = createMockFetch(analysisScenario('completed-report', { apiOrigin: ORIGIN }));

		const response = await fetch(REPORT_URL);

		await expect(response.json()).resolves.toEqual(
			ANALYSIS_FIXTURES['completed-report'].report
		);
	});

	test('an unhandled path is a loud failure, not a 404', async () => {
		const fetch = createMockFetch(analysisScenario('queued', { apiOrigin: ORIGIN }));

		await expect(fetch(`${ORIGIN}/api/v1/analyses`)).rejects.toThrow(/No MSW handler matched/);
	});
});

describe('pollingSequence', () => {
	test('walks the progress states in order', () => {
		expect(pollingSequenceAnalyses().map((analysis) => analysis.state)).toEqual([
			...POLLING_SEQUENCE_STATES
		]);
	});

	test('commit_sha is null only while it is genuinely unresolved', () => {
		const [queued, resolving, collecting] = pollingSequenceAnalyses();

		expect(queued?.commit_sha).toBeNull();
		expect(resolving?.commit_sha).toBeNull();
		expect(collecting?.commit_sha).toEqual(expect.any(String));
	});

	test('serves one state per request and then stays completed', async () => {
		const fetch = createMockFetch(pollingSequence({ apiOrigin: ORIGIN }));

		const observed: string[] = [];
		// Two extra polls past the end: a UI that keeps polling after a terminal state must
		// not see the sequence wrap around to QUEUED.
		for (let poll = 0; poll < POLLING_SEQUENCE_STATES.length + 2; poll += 1) {
			const response = await fetch(ANALYSIS_URL);
			const analysis = (await response.json()) as { state: string };
			observed.push(analysis.state);
		}

		expect(observed).toEqual([...POLLING_SEQUENCE_STATES, 'COMPLETED', 'COMPLETED']);
	});

	test('the report appears only after completion has been observed', async () => {
		const fetch = createMockFetch(pollingSequence({ apiOrigin: ORIGIN }));

		expect((await fetch(REPORT_URL)).status).toBe(404);

		for (let poll = 0; poll < POLLING_SEQUENCE_STATES.length - 1; poll += 1) {
			await fetch(ANALYSIS_URL);
			expect((await fetch(REPORT_URL)).status).toBe(404);
		}

		await fetch(ANALYSIS_URL); // the poll that returns COMPLETED

		const response = await fetch(REPORT_URL);
		expect(response.status).toBe(200);
		await expect(response.json()).resolves.toEqual(
			ANALYSIS_FIXTURES['completed-report'].report
		);
	});

	test('each call owns its own cursor', async () => {
		// The sequence is stateful, so a shared handler set would leak progress between
		// tests — the failure mode this factory exists to prevent.
		const first = createMockFetch(pollingSequence({ apiOrigin: ORIGIN }));
		const second = createMockFetch(pollingSequence({ apiOrigin: ORIGIN }));

		await first(ANALYSIS_URL);
		await first(ANALYSIS_URL);

		const response = await second(ANALYSIS_URL);
		const analysis = (await response.json()) as { state: string };

		expect(analysis.state).toBe('QUEUED');
	});
});
