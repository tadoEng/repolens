/**
 * How a handler decides which URLs it intercepts.
 *
 * Shared by every handler module so that "which origin does this mock answer for?" has one
 * answer. Two modules each building their own URL is how a test ends up mocking the probe
 * but not the analysis endpoints, and then failing for a reason that looks like CORS.
 */

export interface HandlerOptions {
	/**
	 * Origin the handlers intercept, matching `PUBLIC_API_ORIGIN`.
	 *
	 * Passed in rather than read from the environment so a test can point handlers at a
	 * different origin without mutating process state.
	 */
	apiOrigin?: string;
}

/**
 * Strip trailing slashes without a regex.
 *
 * `/\/+$/` is a polynomial ReDoS (CodeQL `js/polynomial-redos`) — the same pattern
 * `repolens-api-client`'s `normalizeOrigin` avoids, and for the same reason. The input here
 * is test configuration rather than user input, so it was never reachable by an attacker;
 * but this file is the copy that comment predicted, which is the whole argument for not
 * writing the pattern anywhere.
 */
function stripTrailingSlashes(origin: string): string {
	const SLASH = 47; // '/'
	let end = origin.length;
	while (end > 0 && origin.charCodeAt(end - 1) === SLASH) {
		end -= 1;
	}
	return origin.slice(0, end);
}

/**
 * Build the URL predicate for an API path.
 *
 * A leading `*` matches any origin. That is the right default: a component test knows which
 * endpoint it is mocking but has no business knowing which origin the app was built
 * against, and pinning one here would make the mocks fail for the wrong reason.
 */
export function apiUrl(path: string, { apiOrigin }: HandlerOptions = {}): string {
	return apiOrigin ? `${stripTrailingSlashes(apiOrigin)}${path}` : `*${path}`;
}
