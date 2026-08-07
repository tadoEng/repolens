-- Analyses and their reports.
--
-- Two tables, not eight. Findings, evidence, limitations and composition are
-- stored as one JSONB document rather than normalised into rows, because
-- nothing in Phase 0 queries *inside* a report: it is written once when the
-- analysis completes, and read back whole. Normalising it would buy join
-- complexity and a second definition of a shape the contract already fixes.
--
-- If a later phase needs "every repository where rule X fired", that is a
-- deliberate migration with a query behind it, not a structure to build in
-- advance.

CREATE TABLE analyses (
    id uuid PRIMARY KEY,

    -- Known at creation, before anything is resolved. This is what lets the UI
    -- render owner/name immediately instead of a blank header.
    owner text NOT NULL,
    name text NOT NULL,

    -- Null until RESOLVING completes. The contract publishes this as
    -- required-but-nullable for the same reason it is nullable here: "not
    -- resolved yet" is a real state, not a missing value.
    commit_sha text,

    -- Matches the AnalysisState enum on the wire. Stored as text rather than a
    -- PostgreSQL enum: adding a state should be a code change and a migration
    -- of data, not a migration that rewrites a type every reader depends on.
    state text NOT NULL,

    -- Populated only in a failed state. Split into code and message because the
    -- frontend switches on the code and displays the message; storing a single
    -- rendered string would lose the distinction.
    error_code text,
    error_message text,
    retry_after_seconds integer,

    -- The server's decision, persisted rather than derived at read time, so two
    -- readers cannot disagree about whether a retry is currently permitted.
    retry_allowed boolean NOT NULL DEFAULT false,
    retry_reason text,

    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),

    -- An error code without a failed state, or a failed state without an error,
    -- would both reach the frontend as an unrenderable analysis. Rejected here
    -- rather than defended against in every reader.
    CONSTRAINT analyses_failure_is_explained CHECK (
        (state NOT IN ('FAILED_RETRIABLE', 'FAILED_PERMANENT') AND error_code IS NULL)
        OR (state IN ('FAILED_RETRIABLE', 'FAILED_PERMANENT') AND error_code IS NOT NULL)
    )
);

-- The progress page polls one analysis by id, and the home page will later list
-- recent ones. Nothing scans by owner yet, so no index pretends otherwise.
CREATE INDEX analyses_created_at_idx ON analyses (created_at DESC);

CREATE TABLE reports (
    -- One report per analysis. The primary key says so, rather than a unique
    -- constraint bolted onto a surrogate id that nothing would use.
    analysis_id uuid PRIMARY KEY REFERENCES analyses (id) ON DELETE CASCADE,

    -- The full report-v1 document. Immutable once written: a report describes
    -- one commit under one ruleset, so "updating" it would mean it no longer
    -- describes what it claims to.
    document jsonb NOT NULL,

    created_at timestamptz NOT NULL DEFAULT now()
);
