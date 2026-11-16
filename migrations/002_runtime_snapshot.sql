-- FPECS runtime state: additive migration, no demo accounts, no table deletes.
-- Contains simulated telemetry, Argon2id password hashes, profile metadata, and queue state.
-- PostgreSQL must be configured with restricted privileges. Do not use a public database.
BEGIN;
CREATE TABLE IF NOT EXISTS fpecs_runtime_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    payload JSONB NOT NULL,
    updated_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMIT;
