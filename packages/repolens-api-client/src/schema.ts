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
    "/api/v1/analyses": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["create"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/analyses/{analysis_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["read"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/analyses/{analysis_id}/report": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["read_report"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
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
        /** @description One analysis run. */
        Analysis: {
            /**
             * @description The exact commit, once resolved.
             *
             *     **Null during `QUEUED` and `RESOLVING`** — there genuinely is no commit
             *     yet. Required-but-nullable rather than optional, so a consumer cannot
             *     forget the case: the field is always present, its value is not.
             */
            commit_sha: string | null;
            /**
             * Format: date-time
             * @description When the analysis was created.
             */
            created_at: string;
            error?: null | components["schemas"]["ApiError"];
            /** @description Scheduling facts, kept out of `state`. */
            execution: components["schemas"]["ExecutionMetadata"];
            /**
             * Format: uuid
             * @description Stable identifier. `UUIDv7`: time-ordered for index locality, and with 74
             *     random bits it remains unguessable, which is what allows anonymous
             *     progress viewing by URL.
             */
            id: string;
            /**
             * Format: int32
             * @description How long the client should wait before polling again.
             *
             *     Server-supplied so the interval can widen as an analysis ages, and so a
             *     hardcoded frontend value cannot multiply cold starts and cost. Absent in
             *     terminal states — there is nothing left to poll for.
             */
            poll_after_ms?: number | null;
            /**
             * @description Whether `GET /reports/{id}` will return a report.
             *
             *     Explicit rather than `state == COMPLETED`, because report availability
             *     and analysis completion are separate facts once reports are retained,
             *     pruned, or regenerated under a newer ruleset.
             */
            report_available: boolean;
            /** @description Owner and name, known from creation. */
            repository: components["schemas"]["RepositoryIdentity"];
            /** @description Server's decision on whether a retry is currently permitted. */
            retry: components["schemas"]["RetryPolicy"];
            /** @description Where the analysis has reached. */
            state: components["schemas"]["AnalysisState"];
            /**
             * Format: date-time
             * @description When it last changed state. Distinct from `created_at` so a UI can show
             *     "stuck for 20 minutes" rather than only "started 20 minutes ago".
             */
            updated_at: string;
        };
        /**
         * @description Where an analysis has reached.
         *
         *     Ordered as the work actually proceeds, so a UI can render a checklist by
         *     position without a second table mapping states to steps.
         *
         *     Infrastructure state is deliberately absent — there is no `TRIGGERING` or
         *     `WAITING_FOR_WORKER`. Whether a Cloud Run Job execution was accepted is a
         *     property of the *execution*, not of the analysis, and mixing them would mean
         *     every consumer of this enum had to learn how the work is scheduled. See
         *     [`ExecutionMetadata`].
         * @enum {string}
         */
        AnalysisState: "QUEUED" | "RESOLVING" | "COLLECTING" | "ANALYZING" | "BUILDING_REPORT" | "COMPLETED" | "FAILED_RETRIABLE" | "FAILED_PERMANENT";
        /**
         * @description A failure, as the browser receives it.
         *
         *     Fields are private and deserialization is validated, so
         *     `ANALYZER_FAILED_PERMANENT` carrying a 900-second countdown cannot be
         *     constructed *or* parsed. A safe constructor alone would not achieve that:
         *     derived `Deserialize` would still accept the combination off the wire, and a
         *     public field would still accept it in Rust.
         */
        ApiError: {
            /** @description Stable machine code. Switch on this, never on `message`. */
            code: components["schemas"]["ErrorCode"];
            /**
             * @description Human-readable explanation. Safe to display, and deliberately free of
             *     internal identifiers, hostnames, and credentials — this string crosses
             *     into a browser.
             */
            message: string;
            /**
             * Format: int32
             * @description How long to wait before retrying, when that is actually knowable.
             *
             *     Absent rather than zero when unknown: a UI that renders "retry in 0s"
             *     from a missing value is worse than one that renders no countdown at all.
             */
            retry_after_seconds?: number | null;
        };
        /** @description Line counts for one top-level area of the repository. */
        AreaLineCount: {
            /** @description Top-level path, e.g. `crates/` or `web/`. */
            area: string;
            /**
             * Format: int64
             * @description Lines of code in it.
             */
            code_lines: number;
        };
        /**
         * @description How a counted file was classified by role.
         *
         *     Structural evidence only. This is **not** a test-quality score: a repository
         *     with little test code may be thoroughly tested elsewhere, and a repository
         *     with a great deal of it may test the wrong things.
         * @enum {string}
         */
        CodeRole: "PRODUCTION" | "TEST" | "GENERATED" | "TOOLING";
        /**
         * @description Why some files were left out of the counts.
         *
         *     Structured rather than prose so the UI can make the ledger expandable. LOC
         *     misleads exactly when nobody can see what was excluded.
         */
        CompositionExclusion: {
            /**
             * Format: int64
             * @description How many bytes it covered.
             */
            bytes: number;
            /**
             * Format: int64
             * @description How many files it covered.
             */
            file_count: number;
            /** @description Which policy rule matched, so the decision is traceable. */
            matched_rule: string;
            /** @description Path or glob that was excluded. */
            path_or_rule: string;
            /** @description Why, in words. */
            reason: string;
        };
        /**
         * @description Strength of the evidence. Independent of [`Severity`].
         * @enum {string}
         */
        Confidence: "LOW" | "MEDIUM" | "HIGH";
        /** @description What a caller submits to start an analysis. */
        CreateAnalysisRequest: {
            /**
             * @description A public GitHub repository URL, e.g. `https://github.com/rust-lang/crates.io`.
             *
             *     A URL rather than separate owner and name fields, because a URL is what a
             *     person has in their clipboard. Parsing it is our job, not theirs.
             */
            repository_url: string;
        };
        /**
         * @description Why a request or an analysis failed.
         *
         *     `SCREAMING_SNAKE_CASE` per the settled convention (#14). The set is closed on
         *     purpose — see [`super::UNKNOWN_VARIANT_POLICY`] for what the frontend does
         *     when a future backend adds one it has never seen.
         * @enum {string}
         */
        ErrorCode: "INVALID_REPOSITORY_URL" | "REPOSITORY_NOT_FOUND" | "REPOSITORY_INACCESSIBLE" | "REPOSITORY_ARCHIVED" | "REPOSITORY_TOO_LARGE" | "RATE_LIMITED" | "WORKER_FAILED_RETRIABLE" | "ANALYZER_FAILED_PERMANENT" | "ANALYSIS_NOT_FOUND" | "REPORT_NOT_AVAILABLE" | "UNAUTHENTICATED" | "AUTHENTICATION_UNAVAILABLE" | "MALFORMED_REQUEST" | "REQUEST_TOO_LARGE" | "REQUEST_TIMED_OUT" | "INTERNAL_ERROR";
        /**
         * @description One checkable fact supporting a finding.
         *
         *     Every excerpt is truncated **server-side**. The frontend must never be the
         *     thing that prevents a five-megabyte payload: by the time the browser could
         *     decide, the bytes have already crossed the network and been parsed.
         */
        Evidence: {
            /**
             * @description Digest of the **full** source content, not the excerpt — which is what
             *     makes the evidence checkable against the commit.
             *
             *     Typed rather than a bare string so the format is owned in one place. The
             *     ingestion boundary produces it and this contract publishes it; two
             *     independent spellings would not surface until integration, and would
             *     surface as evidence that silently fails to match the commit it pins.
             */
            digest?: string | null;
            /** @description Short excerpt, already truncated to the server's cap. */
            excerpt?: string | null;
            /** @description What sort of evidence this is. */
            kind: components["schemas"]["EvidenceKind"];
            line_range?: null | components["schemas"]["LineRange"];
            /** @description Repository-relative path, when the evidence has one. */
            path?: string | null;
            /**
             * @description Whether `excerpt` was cut short. Required so the UI can say "truncated"
             *     rather than implying the file ends there.
             */
            truncated: boolean;
        };
        /**
         * @description What kind of thing a piece of evidence is.
         * @enum {string}
         */
        EvidenceKind: "FILE_PRESENCE" | "FILE_EXCERPT" | "DEPENDENCY_ENTRY" | "WORKFLOW_DEFINITION" | "STATISTIC" | "REPOSITORY_METADATA";
        /** @description Scheduling facts about an analysis. */
        ExecutionMetadata: {
            /** @description Runner-assigned execution identifier, for correlating logs. */
            execution_id?: string | null;
            /** @description Whether the work was successfully handed to a runner. */
            trigger_status: components["schemas"]["TriggerStatus"];
            /**
             * Format: date-time
             * @description When the trigger was attempted.
             */
            triggered_at?: string | null;
        };
        /** @description One conclusion, with everything needed to check it. */
        Finding: {
            /** @description Section this belongs to. */
            category: components["schemas"]["FindingCategory"];
            /** @description Evidence strength. **Never merged with `severity`.** */
            confidence: components["schemas"]["Confidence"];
            /**
             * @description Facts supporting the conclusion. May be empty for `UNABLE_TO_VERIFY`,
             *     which is precisely the case where there is nothing to show.
             */
            evidence: components["schemas"]["Evidence"][];
            /** @description Prose explanation, including why it matters. */
            explanation: string;
            /**
             * Format: uuid
             * @description Stable identifier within this report.
             */
            id: string;
            /** @description What this finding does not establish. */
            limitations: components["schemas"]["Limitation"][];
            /** @description Suggested next step. Absent when the honest answer is "nothing to do". */
            recommended_action?: string | null;
            /** @description Which rule produced this, e.g. `rust.workspace.detected`. */
            rule_id: string;
            /**
             * @description Ruleset that produced it. Carried per-finding as well as per-report so a
             *     stored finding stays interpretable after the report is regenerated.
             */
            ruleset_version: string;
            /** @description Impact if valid. **Never merged with `confidence`.** */
            severity: components["schemas"]["Severity"];
            /** @description What the analyzer concluded. */
            state: components["schemas"]["FindingState"];
            /** @description One-line summary. */
            title: string;
        };
        /**
         * @description Grouping for the engineering-system section.
         * @enum {string}
         */
        FindingCategory: "TECHNOLOGY" | "ARCHITECTURE" | "SOURCE_AND_DOCUMENTATION" | "BUILD_AND_DEPENDENCIES" | "TESTING" | "CI_CD" | "OPERATIONS" | "SECURITY_AND_MAINTENANCE";
        /**
         * @description What the analyzer concluded about one checked property.
         * @enum {string}
         */
        FindingState: "DETECTED" | "DOCUMENTED" | "MISSING" | "NOT_APPLICABLE" | "UNABLE_TO_VERIFY";
        /** @description Line counts for one language. */
        LanguageLineCount: {
            /**
             * Format: int64
             * @description Blank lines.
             */
            blank_lines: number;
            /**
             * Format: int64
             * @description Lines of code, excluding comments and blanks. The headline number.
             */
            code_lines: number;
            /**
             * Format: int64
             * @description Comment lines.
             */
            comment_lines: number;
            /**
             * Format: int64
             * @description Files attributed to this language.
             */
            files: number;
            /** @description Language name as the counter reports it. */
            language: string;
        };
        /**
         * @description One of the largest files by line count.
         *
         *     Size alone is not a defect. It is a **review-priority signal**: a large file
         *     that also concentrates several responsibilities is where a reader's
         *     attention is best spent first.
         */
        LargestSourceFile: {
            /**
             * Format: int64
             * @description Lines of code, excluding comments and blanks.
             */
            code_lines: number;
            /** @description Language the counter attributed it to. */
            language: string;
            /** @description Repository-relative path. */
            path: string;
            /**
             * @description Role, so a large generated file is not mistaken for a large hand-written
             *     one — which is the most common way this list misleads.
             */
            role: components["schemas"]["CodeRole"];
        };
        /** @description Something the analyzer could not establish. */
        Limitation: {
            /** @description Stable code, so the UI can group and explain limitations consistently. */
            code: string;
            /** @description What could not be established, and why. */
            explanation: string;
        };
        /**
         * @description Repository composition and line counts.
         *
         *     Measures composition, **not** productivity or quality. The report says so
         *     visibly, because this is the easiest section to misread as a score.
         */
        LineCountSummary: {
            /** @description Per-area breakdown, server-ordered. */
            areas: components["schemas"]["AreaLineCount"][];
            /**
             * Format: int64
             * @description Blank lines.
             */
            blank_lines: number;
            /**
             * Format: int64
             * @description Lines of code.
             */
            code_lines: number;
            /**
             * Format: int64
             * @description Comment lines.
             */
            comment_lines: number;
            /** @description Counting tool, e.g. `tokei`. */
            counter: string;
            /**
             * @description Exact counter version — different versions count differently, so this is
             *     part of what makes a count reproducible.
             */
            counter_version: string;
            /** @description Version of the exclusion policy applied. */
            exclusion_policy_version: string;
            /** @description What was left out, and why. */
            exclusions: components["schemas"]["CompositionExclusion"][];
            /** @description Per-language breakdown, server-ordered. */
            languages: components["schemas"]["LanguageLineCount"][];
            /**
             * @description Largest files by line count, server-ordered, descending.
             *
             *     Bounded in both directions by [`LargestSourceFiles`]: a producer cannot
             *     construct an over-long list, and an over-long one cannot be parsed.
             */
            largest_files: components["schemas"]["LargestSourceFile"][];
            /**
             * @description Breakdown by role, server-ordered.
             *
             *     Present so the report can show what proportion of the repository is
             *     production code without implying a judgement about it.
             */
            roles: components["schemas"]["RoleLineCount"][];
            /**
             * Format: int64
             * @description Files counted.
             */
            total_files: number;
            /**
             * Format: int64
             * @description All physical lines.
             */
            total_lines: number;
            /**
             * Format: int64
             * @description Files the policy could not classify. Reported rather than silently
             *     folded into a bucket.
             */
            unclassified_files: number;
        };
        /** @description A line span within a file. */
        LineRange: {
            /**
             * Format: int32
             * @description Last line, inclusive.
             */
            end: number;
            /**
             * Format: int32
             * @description First line, 1-indexed.
             */
            start: number;
        };
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
         * @description An evidence-backed statement for the executive overview.
         *
         *     The overview carries the entire summarization load, because there is no
         *     score to skim. Each statement therefore points at the findings that support
         *     it rather than asserting on its own authority.
         */
        OverviewStatement: {
            /** @description Confidence in the statement as a whole. */
            confidence: components["schemas"]["Confidence"];
            /** @description The statement itself. */
            statement: string;
            /** @description Findings that support it, by `rule_id`. */
            supporting_rule_ids: string[];
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
        /** @description A complete report for one repository at one commit. */
        Report: {
            /**
             * Format: uuid
             * @description Analysis that produced it.
             */
            analysis_id: string;
            /** @description Analyzer version that produced this report. First-class, not a footnote. */
            analyzer_version: string;
            /**
             * @description Exact commit. Non-null here, unlike on the analysis: a report cannot
             *     exist without a resolved commit.
             */
            commit_sha: string;
            /**
             * Format: date-time
             * @description When the analysis completed.
             */
            completed_at: string;
            composition: null | components["schemas"]["LineCountSummary"];
            /**
             * @description All findings, in a **server-decided order**.
             *
             *     Ordering is part of the contract. A report that listed findings
             *     differently on each load would contradict the determinism it claims,
             *     and no client-side sort can restore an order the server never fixed.
             */
            findings: components["schemas"]["Finding"][];
            /**
             * @description What this report as a whole does not establish.
             *
             *     Report-level, not merely per-finding, so "absence of evidence is not
             *     evidence of absence" stays visible in the overview rather than buried
             *     inside an expanded finding nobody opened.
             */
            limitations: components["schemas"]["Limitation"][];
            /** @description Evidence-backed summary statements. */
            overview: components["schemas"]["OverviewStatement"][];
            /** @description Repository analyzed. */
            repository: components["schemas"]["RepositoryIdentity"];
            /** @description Ruleset version evaluated. First-class, not a footnote. */
            ruleset_version: string;
            /**
             * @description Root tree the collectors walked. Part of the reproducibility key, since
             *     two commits sharing a tree yield identical evidence.
             */
            tree_sha: string;
        };
        /**
         * @description Repository identity, available from the moment an analysis is created.
         *
         *     Present before `commit_sha` exists, which is what lets the header render
         *     `owner/name` immediately instead of a blank space that looks broken.
         */
        RepositoryIdentity: {
            /** @description Repository name, without the owner prefix. */
            name: string;
            /** @description User or organization. */
            owner: string;
        };
        /**
         * @description Whether the caller may retry, decided by the server.
         *
         *     Never inferred from the state name. `FAILED_RETRIABLE` describes the *kind*
         *     of failure; whether a retry is permitted also depends on how many attempts
         *     have already been spent and whether the work is still claimable — facts only
         *     the server holds. A frontend that derived this would offer a button that
         *     does nothing.
         */
        RetryPolicy: {
            /** @description Whether a retry request would be accepted right now. */
            allowed: boolean;
            /**
             * @description Why not, when `allowed` is false. Displayed verbatim, so it explains
             *     rather than merely denies.
             */
            reason?: string | null;
        };
        /** @description Line counts for one role. */
        RoleLineCount: {
            /**
             * Format: int64
             * @description Lines of code in it.
             */
            code_lines: number;
            /**
             * Format: int64
             * @description Files attributed to it.
             */
            files: number;
            /** @description Which role. */
            role: components["schemas"]["CodeRole"];
        };
        /**
         * @description Impact if the finding is valid. Independent of [`Confidence`].
         * @enum {string}
         */
        Severity: "INFO" | "LOW" | "MEDIUM" | "HIGH";
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
             *
             *     `required` is explicit because utoipa treats `Option<T>` as optional by
             *     default, which would generate `schema_version?: number | null` in
             *     TypeScript. The field is always present — its *value* is nullable — and
             *     the two are different contracts: an optional field lets a consumer
             *     forget the null case entirely, which is the case that matters here.
             */
            schema_version: number | null;
        };
        /**
         * @description Whether the scheduler accepted the work.
         *
         *     Separate from [`AnalysisState`] because they fail independently: an analysis
         *     can be `QUEUED` with the trigger *succeeded* (normal, waiting for a worker)
         *     or `QUEUED` with the trigger *failed* (stuck, and nothing will ever pick it
         *     up). Those look identical without this, and the second one is the outage.
         * @enum {string}
         */
        TriggerStatus: "PENDING" | "SUCCEEDED" | "FAILED";
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
    create: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateAnalysisRequest"];
            };
        };
        responses: {
            /** @description Accepted; the analysis is queued */
            202: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Analysis"];
                };
            };
            /** @description The URL is not a public GitHub repository */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description No valid Firebase ID token was presented */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description The request exceeded the server time budget */
            408: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description The request body is over the limit */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description Content-Type is not application/json */
            415: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description The body is JSON but not this request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description An unhandled fault in this service */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description The analysis store is unavailable */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    read: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Analysis identifier */
                analysis_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current state of the analysis */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Analysis"];
                };
            };
            /** @description The identifier is not a UUID */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description No such analysis */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description The request exceeded the server time budget */
            408: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description An unhandled fault in this service */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description The analysis store is unavailable */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
    read_report: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Analysis identifier */
                analysis_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description The completed report */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Report"];
                };
            };
            /** @description The identifier is not a UUID */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description No report for that analysis */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description The request exceeded the server time budget */
            408: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description An unhandled fault in this service */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description The analysis store is unavailable */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
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
            /** @description The request exceeded the server time budget */
            408: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description An unhandled fault in this service */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
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
            /** @description The request exceeded the server time budget */
            408: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
            /** @description An unhandled fault in this service */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiError"];
                };
            };
        };
    };
}
