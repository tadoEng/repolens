/**
 * Transport for the operational snapshot.
 *
 * Two reads, because the page needs two facts the API publishes in two places, and neither
 * is worth inventing a third endpoint for. `/api/v1/admin/overview` carries what the
 * process has measured about itself; `/api/v1/system/probe` carries the schema version and
 * whether the database answered. Both go through the generated client, so the URLs, the
 * response shapes, and the fact that these operations exist at all come from
 * `contracts/openapi.json` rather than from anything written here.
 *
 * ## Why the overview carries a credential and the probe does not
 *
 * The overview is authorised: Axum verifies a Firebase ID token and checks the uid against
 * `ADMIN_FIREBASE_UIDS`, answering `401` without a credential and `403` for a signed-in
 * caller who is not allow-listed. The probe is deliberately anonymous — it is the same
 * endpoint the home page already renders, and asking for a token would be inventing a
 * restriction the server does not have.
 *
 * ## The browser is not the gate
 *
 * A missing token is **not** short-circuited into a local refusal, exactly as
 * `createAnalysis` does not short-circuit one. The request goes out and the API answers, so
 * there is one authority for who may read this. A page that decided for itself would show a
 * different outcome than the server produced, and the outcome that matters is the server's.
 *
 * `/admin` being hard to find is not access control, and neither is a `hidden` attribute.
 */

import { api } from '$lib/api/client';
import { session } from '$lib/auth/session.svelte';
import type { AdminOverview, SystemProbeResponse } from '@repolens/api-client';

import { fetched, type Fetched } from '$lib/api/fetched';

export type { AdminOverview, SystemProbeResponse };

/**
 * The operational snapshot of the process that answered.
 *
 * Every figure in the result describes **one process**: there is no aggregation across
 * instances and no history, so a restart resets the counters. That is a property of the
 * contract rather than of this function, and the page says it on screen.
 */
export async function fetchAdminOverview(): Promise<Fetched<AdminOverview>> {
	const token = await session.idToken();

	return fetched<AdminOverview>(
		api.GET('/api/v1/admin/overview', {
			...(token ? { headers: { Authorization: `Bearer ${token}` } } : {})
		})
	);
}

/**
 * Reachability of the API and its database, plus the applied schema version.
 *
 * Read here as well as on the home page, and that is not duplication of a *shape* — both
 * call the same generated operation and receive the same DTO. What the admin page adds is
 * a second reader for facts it would otherwise have to do without: `schema_version` is the
 * deployment fact the overview does not carry, and `database` is the only truthful thing
 * this build can say about PostgreSQL at all.
 */
export function fetchSystemProbe(): Promise<Fetched<SystemProbeResponse>> {
	return fetched<SystemProbeResponse>(api.GET('/api/v1/system/probe', {}));
}
