import {
	DATABASE_UNAVAILABLE_PROBE,
	HEALTHY_PROBE,
	LOCAL_BUILD_PROBE,
	createMockFetch,
	systemProbeDatabaseUnavailable,
	systemProbeHandler,
	systemProbeHealthy,
	systemProbeNetworkFailure,
	systemProbeServerError,
	type RequestHandler
} from '@repolens/msw';
import { afterEach, expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import SystemProbe from '$lib/components/SystemProbe.svelte';

/**
 * The walking-skeleton probe, in a real browser.
 *
 * The component models four states and three of them are failure or unknown states, which
 * are exactly the ones a hand-clicked check never sees. They are driven here through the
 * shared MSW handlers, so a change to the Rust DTOs breaks these tests at the mock rather
 * than leaving them green against a shape the API no longer returns.
 */

const transport = vi.hoisted(() => {
	/*
	 * `openapi-fetch` captures `globalThis.fetch` when the client is constructed, and
	 * `$lib/api/client` constructs it at import time — so a stub installed in `beforeEach`
	 * would arrive after the capture and never be called. Hoisting installs one stable
	 * indirection instead: its identity never changes, so the capture stays valid while
	 * each test redirects where it dispatches.
	 */
	const network = globalThis.fetch.bind(globalThis);
	const state: { dispatch: typeof globalThis.fetch } = { dispatch: network };
	globalThis.fetch = (input, init) => state.dispatch(input, init);
	return { state, network };
});

afterEach(() => {
	transport.state.dispatch = transport.network;
});

/** Serve the probe from the given handlers; anything unmatched still hits the real network. */
function serve(...handlers: RequestHandler[]): void {
	transport.state.dispatch = createMockFetch(handlers, transport.network);
}

/**
 * The probe renders one sentence out of several elements, and the markup's indentation
 * lands in `textContent`. Normalising is what lets a test assert the sentence a user reads
 * rather than the element tree that happens to produce it.
 */
function probeLine(container: Element): string {
	return (container.textContent ?? '').replace(/\s+/g, ' ').trim();
}

test('reports that it is still checking while the request is in flight', async () => {
	// Never settles: "loading" is only a real state for as long as nothing has answered.
	transport.state.dispatch = () => new Promise<Response>(() => {});

	const screen = await render(SystemProbe);

	await expect.element(screen.getByText('checking…')).toBeInTheDocument();
});

test('renders API, database, short build SHA and schema version when healthy', async () => {
	serve(systemProbeHealthy());

	const screen = await render(SystemProbe);
	await expect.poll(() => probeLine(screen.container)).toContain('API OK');

	const line = probeLine(screen.container);
	expect(line).toContain('database OK');
	expect(line).toContain('schema v1');

	const shown = /build ([0-9a-f]+)/.exec(line)?.[1];
	expect(shown).toBe(HEALTHY_PROBE.build_sha.slice(0, 7));
	expect(shown).toHaveLength(7);
	// A footer is not the place for a full commit hash; the short form is the contract.
	expect(line).not.toContain(HEALTHY_PROBE.build_sha);
});

test('renders a null schema version as unknown, never as zero', async () => {
	// Guards the fixture as well as the component: `undefined` or `0` here would make the
	// assertions below pass while testing something else entirely.
	expect(DATABASE_UNAVAILABLE_PROBE.schema_version).toBeNull();

	serve(systemProbeDatabaseUnavailable());

	const screen = await render(SystemProbe);
	await expect.poll(() => probeLine(screen.container)).toContain('database UNAVAILABLE');

	const line = probeLine(screen.container);
	expect(line).toContain('schema unknown');
	// The whole point of the nullable field: a connection failure must not be able to read
	// as an empty database.
	expect(line).not.toContain('schema v0');
	expect(line).not.toContain('schema 0');
	// The API itself is still up, and the probe must keep saying so.
	expect(line).toContain('API OK');
});

test('renders an unknown build SHA verbatim', async () => {
	serve(systemProbeHandler(LOCAL_BUILD_PROBE));

	const screen = await render(SystemProbe);
	await expect.poll(() => probeLine(screen.container)).toContain('API OK');

	// `unknown` is itself seven characters, so this pins the rendered value rather than
	// distinguishing the two branches of `shortSha`. It still earns its place: it fails if
	// the placeholder is ever shortened, padded, or swapped for an em dash.
	expect(probeLine(screen.container)).toContain('build unknown');
});

test('reports the API as unreachable when the request fails at the transport', async () => {
	serve(systemProbeNetworkFailure());

	const screen = await render(SystemProbe);
	await expect.poll(() => probeLine(screen.container)).toContain('API unavailable');

	expect(probeLine(screen.container)).toContain('the API could not be reached');
});

test('reports the API as unreachable when the server answers without a probe', async () => {
	serve(systemProbeServerError());

	const screen = await render(SystemProbe);
	await expect.poll(() => probeLine(screen.container)).toContain('API unavailable');

	// Distinct wording from the transport failure: the origin and CORS were fine, so this
	// is a server fault rather than a configuration one, and the copy has to say which.
	expect(probeLine(screen.container)).toContain('the API responded without a probe result');
});

test('announces the result politely rather than assertively', async () => {
	serve(systemProbeHealthy());

	const screen = await render(SystemProbe);
	await expect.poll(() => probeLine(screen.container)).toContain('API OK');

	// The probe resolves after paint. `assertive` would interrupt a screen-reader user who
	// has already moved on, for a diagnostic they did not ask for.
	const live = screen.container.querySelector('[aria-live]');
	expect(live?.getAttribute('aria-live')).toBe('polite');
});
