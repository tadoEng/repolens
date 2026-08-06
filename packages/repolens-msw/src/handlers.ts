/**
 * Shared MSW request handlers for the RepoLens API.
 *
 * One source of truth for mocked API behaviour, consumed by three places at once:
 * component tests (Vitest browser mode), Playwright end-to-end runs, and a `dev:msw`
 * mode that lets the frontend be developed against a working API without the backend
 * running.
 *
 * This module handles `GET /api/v1/system/probe`. The analysis and report endpoints live in
 * `analysis-handlers.ts`, which serves the executable fixtures from #14 rather than bodies
 * written out here — an invented mock is worse than no mock, because it lets a UI be built,
 * reviewed, and merged against a contract that never existed.
 *
 * Every body below is typed as a schema from the *generated* client rather than written
 * out by hand. That is the entire point of this package — when the Rust DTOs change, the
 * regenerated schema breaks these mocks at compile time instead of letting them drift.
 */

import type { SystemProbeResponse } from '@repolens/api-client';
import { HttpResponse, http, type RequestHandler } from 'msw';

import { apiUrl, type HandlerOptions } from './api-url';

const SYSTEM_PROBE_PATH = '/api/v1/system/probe';

export type { HandlerOptions };

function systemProbeUrl(options: HandlerOptions): string {
	return apiUrl(SYSTEM_PROBE_PATH, options);
}

/** Every dependency answered: the shape a healthy deployment returns. */
export const HEALTHY_PROBE: SystemProbeResponse = {
	api: 'OK',
	database: 'OK',
	// Deliberately longer than the seven characters the footer shows, so that a test can
	// tell "shortened" apart from "happened to be short".
	build_sha: '0f1e2d3c4b5a69788796a5b4c3d2e1f009182736',
	schema_version: 1
};

/**
 * The API answered but its database did not.
 *
 * `api` stays `OK`: reaching the handler at all means the process is serving, and keeping
 * that distinction visible is the reason the probe reports dependency health as data
 * instead of failing the request. `schema_version` is `null` rather than `0` — "no
 * migrations have been applied" and "we could not find out" are different facts.
 */
export const DATABASE_UNAVAILABLE_PROBE: SystemProbeResponse = {
	api: 'OK',
	database: 'UNAVAILABLE',
	build_sha: '0f1e2d3c4b5a69788796a5b4c3d2e1f009182736',
	schema_version: null
};

/** A binary built outside CI, where there is no commit to name. */
export const LOCAL_BUILD_PROBE: SystemProbeResponse = {
	api: 'OK',
	database: 'OK',
	build_sha: 'unknown',
	schema_version: 1
};

/** Answer the probe with an arbitrary contract-shaped body. */
export function systemProbeHandler(
	probe: SystemProbeResponse,
	options: HandlerOptions = {}
): RequestHandler {
	// The third type argument makes the resolver's return type the generated DTO, so a
	// body that no longer matches the contract is a type error rather than a stale mock.
	return http.get<never, never, SystemProbeResponse>(systemProbeUrl(options), () =>
		HttpResponse.json(probe)
	);
}

/** Scenario: everything is up. */
export function systemProbeHealthy(options: HandlerOptions = {}): RequestHandler {
	return systemProbeHandler(HEALTHY_PROBE, options);
}

/** Scenario: the service is serving but cannot reach Neon. */
export function systemProbeDatabaseUnavailable(options: HandlerOptions = {}): RequestHandler {
	return systemProbeHandler(DATABASE_UNAVAILABLE_PROBE, options);
}

/**
 * Scenario: the service failed outright.
 *
 * The body is empty because the contract declares no error schema for this path — a JSON
 * error DTO invented here would be exactly the drift this package exists to prevent.
 */
export function systemProbeServerError(options: HandlerOptions = {}): RequestHandler {
	return http.get(systemProbeUrl(options), () => new HttpResponse(null, { status: 500 }));
}

/**
 * Scenario: the request never reached a server.
 *
 * Distinct from a 500: the origin was wrong, DNS failed, or — the case this project cares
 * about most — the CSP `connect-src` allowlist and `PUBLIC_API_ORIGIN` disagree.
 */
export function systemProbeNetworkFailure(options: HandlerOptions = {}): RequestHandler {
	return http.get(systemProbeUrl(options), () => HttpResponse.error());
}

/**
 * Build the handler set for a given API origin.
 *
 * A factory rather than a constant, because Playwright and the browser-mode component
 * tests do not necessarily run against the same origin, and a module-level array would
 * bake one in at import time.
 */
export function createHandlers(options: HandlerOptions = {}): RequestHandler[] {
	return [systemProbeHealthy(options)];
}

/** Default handler set, for consumers that do not need to override the origin. */
export const handlers: RequestHandler[] = createHandlers();
