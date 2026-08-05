-- Schema baseline.
--
-- Deliberately domain-neutral. Analysis tables — repositories, snapshots,
-- analysis_runs, findings, reports — belong to issue #6 and are not invented
-- here; a walking skeleton that guessed at them would have to be unpicked.
--
-- What this does establish is the migration mechanism itself, and a row the
-- system probe can read. That makes `GET /api/v1/system/probe` prove the whole
-- database leg in one call: the connection works, migrations applied, and a
-- real query returns real data. `SELECT 1` would prove only the first.

CREATE TABLE schema_meta (
    -- Single-row table. The CHECK constraint is what enforces that, rather
    -- than convention plus hope.
    id smallint PRIMARY KEY DEFAULT 1 CONSTRAINT schema_meta_single_row CHECK (id = 1),
    -- When this baseline was first applied. Useful when reconciling a
    -- deployment against a database whose history is otherwise opaque.
    baseline_applied_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO schema_meta (id) VALUES (1);
