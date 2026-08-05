/**
 * GENERATED FILE — DO NOT EDIT BY HAND.
 *
 * Produced by `openapi-typescript` from the OpenAPI document that the Axum service emits
 * via utoipa. Regenerate with:
 *
 *     pnpm --filter @repolens/api-client schema:update
 *
 * `schema.test.ts` is the staleness gate: it regenerates this file from the committed
 * OpenAPI document and fails if the result differs. A hand edit here is therefore not a
 * shortcut — it is a build break waiting for the next CI run, and it silently decouples
 * the frontend's idea of the API from the backend's.
 */

export interface paths {
    "/api/v1/system/probe": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["system_probe"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/healthz": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["liveness"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
}
export type webhooks = Record<string, never>;
export interface components {
    schemas: {
        /**
         * @description Process liveness.
         *
         *     Deliberately *not* the system probe. `GET /api/v1/system/probe`, which also
         *     reports database reachability, build SHA, and schema version, is owned by
         *     the walking-skeleton work (issue #11); answering "is the database up?" here
         *     before there is a pool would be a claim this binary cannot support.
         */
        LivenessResponse: {
            /** @description Always `ok` when the process can serve a request at all. */
            status: string;
        };
        /**
         * @description Reachability of one dependency.
         *
         *     Enum values are `SCREAMING_SNAKE_CASE` per the settled contract convention
         *     (issue #14). Unlike object fields — where Rust's own naming already matches
         *     and no rename is used — enum variants are `PascalCase` in Rust, so a rename
         *     is unavoidable here. It is applied once, on the enum, rather than per
         *     variant.
         * @enum {string}
         */
        ProbeStatus: "OK" | "DEGRADED" | "UNAVAILABLE";
        /**
         * @description Result of the system probe.
         *
         *     Deliberately the *whole* hosting path in one response: the process answered
         *     (`api`), the database answered a real query (`database`), the running code
         *     is identifiable (`build_sha`), and the schema is at a known version
         *     (`schema_version`). A liveness endpoint proves only the first.
         */
        SystemProbeResponse: {
            /** @description Always `OK`: reaching this handler means the process is serving. */
            api: components["schemas"]["ProbeStatus"];
            /** @description Commit this binary was built from, or `unknown` for a local build. */
            build_sha: string;
            /** @description Whether a real query against the configured database succeeded. */
            database: components["schemas"]["ProbeStatus"];
            /**
             * Format: int64
             * @description Highest applied migration version.
             *
             *     Null rather than zero when the database could not be reached: "no
             *     migrations have been applied" and "we could not find out" are different
             *     facts, and collapsing them into `0` would let a connection failure read
             *     as an empty database. The frontend must render the null case, which is
             *     also the cheapest available exercise of its unknown-value handling.
             */
            schema_version?: number | null;
        };
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
    system_probe: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Reachability of the API and its database */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SystemProbeResponse"];
                };
            };
        };
    };
    liveness: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description The process is serving requests */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LivenessResponse"];
                };
            };
        };
    };
}
