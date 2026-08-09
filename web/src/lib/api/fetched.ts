/**
 * One request, one outcome — shared by every resource this app reads.
 *
 * Extracted from `analysis.ts` when `/admin` became a second caller. The alternative was a
 * second copy of the union and of the rules below, and the way two copies drift is that one
 * of them stops distinguishing a transport failure from a refusal while its own tests keep
 * passing. There is one definition because there is one contract for what a failed request
 * means.
 */

import type { ApiError } from '@repolens/api-client';

/**
 * The result of one request.
 *
 * Three outcomes, not two, and the third is why: a transport failure and a well-formed
 * error response are different facts about the deployment. Collapsing them produces the
 * classic misdiagnosis where a CSP `connect-src` mismatch is reported to the reader as
 * "analysis not found".
 */
export type Fetched<T> =
	| { kind: 'ok'; value: T }
	/** The API answered with a status we cannot use. `error` is present when it sent one. */
	| { kind: 'rejected'; status: number; error: ApiError | null }
	/** The request never reached a server: DNS, CORS, offline, or a CSP violation. */
	| { kind: 'unreachable' };

/**
 * Recognise an `ApiError` body without trusting it.
 *
 * A response body is untyped data no matter what the contract says, and this one arrives on
 * the failure path — precisely where a server is most likely to answer with a proxy's HTML
 * error page instead. The generated types describe what the *API* promises to send, not what
 * a load balancer between it and the browser actually sent, so this stays a runtime check
 * rather than a cast: the shape is verified and anything else becomes `null`, which the UI
 * renders as "the server did not explain" rather than crashing on `error.code.toUpperCase()`.
 *
 * `code` is not validated against the enum here on purpose: an unrecognised code is data the
 * UI must still display, and rejecting it would be the silent drop the contract forbids.
 */
function asApiError(body: unknown): ApiError | null {
	if (typeof body !== 'object' || body === null) return null;
	const candidate = body as Record<string, unknown>;
	if (typeof candidate.code !== 'string' || typeof candidate.message !== 'string') return null;

	const retryAfter = candidate.retry_after_seconds;
	return {
		code: candidate.code as ApiError['code'],
		message: candidate.message,
		retry_after_seconds: typeof retryAfter === 'number' ? retryAfter : null
	};
}

/**
 * What `openapi-fetch` resolves with, narrowed to the three fields this module reads.
 *
 * Structural rather than the exported `FetchResponse<…>` generic, which would have to be
 * threaded an operation type to be written down. Every call site is still checked against
 * the real result — an operation whose success body is not `T` fails to compile here.
 */
interface ApiResult<T> {
	/** Absent when the response carried no body, `null` when the body was literally `null`. */
	data?: T | null;
	/** The failure body, parsed when it was JSON and the raw text when it was not. */
	error?: unknown;
	response: Response;
}

/**
 * Turn one client call into one outcome.
 *
 * `response.ok` decides, never the truthiness of `error`: `openapi-fetch` leaves `error` as
 * the raw text when a failure body is not JSON, so an empty 404 arrives as `''` and an HTML
 * error page as a string — both falsy or truthy for reasons that have nothing to do with
 * whether the request succeeded.
 */
export async function fetched<T>(call: Promise<ApiResult<T>>): Promise<Fetched<T>> {
	let result: ApiResult<T>;

	try {
		result = await call;
	} catch {
		/*
		 * A transport failure is *thrown* by `openapi-fetch` rather than returned as `error`:
		 * there is no response, so there is no status to report and nothing to parse. This is
		 * the branch that keeps a CORS or CSP failure from being rendered as "not found".
		 *
		 * One known imprecision, recorded rather than left to be discovered: a **2xx** whose
		 * body is not valid JSON also lands here, because `openapi-fetch` throws inside its
		 * own parse step and hands back no response to read a status from. That reports
		 * `unreachable` for a server that demonstrably answered — the same conflation this
		 * union exists to prevent, in the opposite direction. It is accepted only because
		 * this API sends JSON on every path, and because the realistic case of a proxy
		 * substituting an HTML error page carries a non-2xx status and so still lands in
		 * `rejected` with its status intact. Recovering the status would mean shadowing
		 * `fetch` per request to buffer the body ahead of the client — more machinery in the
		 * transport than the case has earned. Revisit if it ever fires in practice.
		 */
		return { kind: 'unreachable' };
	}

	const { data, response } = result;

	if (!response.ok) {
		return { kind: 'rejected', status: response.status, error: asApiError(result.error) };
	}

	if (typeof data !== 'object' || data === null) {
		// A success status with no usable body is a server fault, not a missing resource, and
		// saying so with the status is more useful than pretending the resource is absent.
		// Catches all three ways that happens: no body at all, a literal `null`, and a scalar
		// where the contract promises an object.
		return { kind: 'rejected', status: response.status, error: null };
	}

	return { kind: 'ok', value: data };
}
