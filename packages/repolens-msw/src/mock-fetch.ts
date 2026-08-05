/**
 * Resolving requests against the handlers without a Service Worker.
 *
 * MSW's browser integration works by registering `mockServiceWorker.js`, which has to be
 * served from the origin root — for this app that means `web/static/`, and everything in
 * `web/static/` is copied into `build/` and shipped. Registering a request-rewriting
 * Service Worker on the production origin to make tests pass is not a trade worth making,
 * and it is why `pnpm-workspace.yaml` keeps msw's `postinstall` disabled.
 *
 * `getResponse` is MSW's own handler-matching entry point, so a `fetch` built on it runs
 * the same handlers the worker would, with none of the deployment consequences.
 */

import { getResponse, type RequestHandler } from 'msw';

/**
 * Build a `fetch` implementation backed by MSW handlers.
 *
 * Requests no handler matches are forwarded to `passthrough` when one is supplied, and
 * rejected loudly otherwise. Silently returning a 404 there would let a typo in a URL read
 * as a legitimate "not found" from the API.
 */
export function createMockFetch(
	handlers: RequestHandler[],
	passthrough?: typeof globalThis.fetch
): typeof globalThis.fetch {
	return async (input, init) => {
		const request =
			input instanceof Request && init === undefined ? input : new Request(input, init);
		const response = await getResponse(handlers, request.clone());

		if (!response) {
			if (!passthrough) {
				throw new Error(`No MSW handler matched ${request.method} ${request.url}.`);
			}
			return passthrough(input, init);
		}

		if (response.type === 'error') {
			// `HttpResponse.error()` models a transport failure, which a real `fetch` reports
			// by rejecting — not by resolving with an error-typed `Response`. Without this the
			// caller's `catch` never runs and the network-failure scenario proves nothing.
			throw new TypeError('Failed to fetch');
		}

		return response;
	};
}
