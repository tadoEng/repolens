/**
 * The application's API client instance.
 *
 * This module is the *only* place the SvelteKit environment meets the contract package.
 * `@repolens/api-client` must not import `$env/static/public` — it is a contract package,
 * not a UI one, and dragging a SvelteKit virtual module into it would make it unusable
 * from Node scripts and from the schema staleness gate. So the origin is resolved here,
 * on the app side of the boundary, and injected.
 *
 * `PUBLIC_API_ORIGIN` is a build-time constant: `$env/static/public` inlines it, which
 * means a missing variable fails the build rather than producing a broken bundle. It is
 * the same value `svelte.config.js` bakes into the CSP `connect-src` allowlist, so a
 * request this client makes is a request the policy already permits.
 */

import { PUBLIC_API_ORIGIN } from '$env/static/public';
import { createRepoLensClient, type RepoLensClient } from '@repolens/api-client';

export const api: RepoLensClient = createRepoLensClient({ baseUrl: PUBLIC_API_ORIGIN });
