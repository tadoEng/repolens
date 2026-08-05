/**
 * Guards the origin normalizer against the ReDoS that CodeQL flagged
 * (`js/polynomial-redos`) and against the behaviour changing while it is fixed.
 */

import { describe, expect, test } from 'vitest';

import { createRepoLensClient } from './src/client';

/** Reads back the base URL the client was constructed with. */
function baseUrlFor(origin: string): string {
	// openapi-fetch does not expose baseUrl, so the normalizer is observed through the
	// only surface that reveals it: the URL of a request the client builds.
	let seen = '';
	const client = createRepoLensClient({
		baseUrl: origin,
		fetch: (async (request: Request) => {
			seen = request.url;
			return new Response('{}', { status: 200, headers: { 'content-type': 'application/json' } });
		}) as typeof fetch
	});
	void client.GET('/healthz');
	return seen;
}

describe('origin normalization', () => {
	test('strips trailing slashes without producing a double slash', async () => {
		for (const origin of ['https://api.test', 'https://api.test/', 'https://api.test///']) {
			const url = baseUrlFor(origin);
			await new Promise((resolve) => setTimeout(resolve, 0));
			expect(url, `origin: ${origin}`).toBe('https://api.test/healthz');
		}
	});

	test('a hostile origin completes in linear time', async () => {
		// The attack string must make the match *fail*, not succeed. `/\/+$/` is anchored,
		// so a string ending in slashes matches on the first attempt and costs nothing — an
		// earlier version of this test used that shape and passed against the vulnerable
		// regex, proving nothing. Slashes followed by a non-slash force the engine to retry
		// from every position, which is where the quadratic cost appears.
		const hostile = `https://api.test${'/'.repeat(100_000)}a`;

		const started = performance.now();
		const url = baseUrlFor(hostile);
		await new Promise((resolve) => setTimeout(resolve, 0));
		const elapsed = performance.now() - started;

		// No trailing slash to strip, so the origin passes through unchanged.
		expect(url).toBe(`${hostile}/healthz`);
		expect(elapsed, `took ${elapsed.toFixed(1)}ms`).toBeLessThan(1_000);
	});
});
