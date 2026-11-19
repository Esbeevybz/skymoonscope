-- Migration 003: Create shared results table
-- Enables generation and retrieval of shareable links for contract analysis results.
--
-- Design decisions:
--   • share_id is an 8-character base-62 string (rather than a UUID) for
--     short, URL-friendly links (~218 trillion combinations).
--   • result_data stores a gzip-compressed JSON snapshot of the profiling
--     output.  The column type is BLOB (SQLite) / BYTEA (Postgres); both
--     engines store it identically as raw bytes.
--   • access_token_hash holds the SHA-256 hex digest of a 32-byte random
--     access token for protected shares; NULL means the share is public.
--   • is_active allows soft-deletion without losing historical data for
--     the background sweeper's audit log.
--   • view_count / access_count is incremented atomically by the repository
--     on every successful GET so owners can see how often a link was used.
--   • created_by is optional; filled in when the caller supplies a JWT
--     subject (user identifier) but left NULL for anonymous shares.

-- ── Main table ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS shared_results (
    -- Surrogate UUID primary key — kept for stable foreign-key references and
    -- internal row identification.  External URLs use share_id, not this value.
    id                 TEXT        NOT NULL PRIMARY KEY,

    -- Short, URL-safe random identifier surfaced in the shareable URL.
    -- Unique constraint enforces collision-safety; the application retries
    -- on the rare collision.
    share_id           TEXT        NOT NULL UNIQUE,

    -- Gzip-compressed JSON blob containing the full ShareSnapshot payload.
    -- Stored as a binary blob to keep sizes small (typically 2–8 KB vs
    -- 10–50 KB for raw JSON).
    result_data        BLOB        NOT NULL,

    -- Human-readable title supplied by the creator (e.g. "swap_tokens profiling").
    -- NULL when the creator does not supply one.
    title              TEXT,

    -- Soroban contract identifier (C… Strkey).  Stored denormalised so the
    -- sweeper can do efficient range deletes by contract without decoding blobs.
    contract_id        TEXT,

    -- Name of the invoked contract function.
    function_name      TEXT,

    -- Network the simulation was run against ("testnet", "mainnet", "futurenet").
    network            TEXT,

    -- SHA-256 hex digest of the 32-byte random access token for protected shares.
    -- NULL → public share; non-NULL → caller must supply matching token.
    access_token_hash  TEXT,

    -- Whether the share is currently active.  Set to 0 (false) on deletion /
    -- expiry rather than hard-deleting rows so the sweeper can audit.
    is_active          INTEGER     NOT NULL DEFAULT 1,

    -- Monotonically incrementing counter of successful reads.
    -- Named access_count to match the requirements; stored as view_count alias.
    access_count       INTEGER     NOT NULL DEFAULT 0,

    -- Optional creator identifier (JWT sub claim).  NULL for anonymous shares.
    created_by         TEXT,

    -- Timestamps (stored as ISO-8601 UTC strings for SQLite compatibility;
    -- Postgres maps these to TIMESTAMPTZ via the sqlx driver).
    created_at         TEXT        NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at         TEXT        NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    expires_at         TEXT                -- NULL = never expires (requires JWT auth)
);

-- ── Indexes ───────────────────────────────────────────────────────────────────

-- Primary lookup: resolve a shareable URL to a row.
CREATE INDEX IF NOT EXISTS idx_shared_results_share_id
    ON shared_results (share_id);

-- Expiry sweeper: find all rows that have passed their expiry date.
CREATE INDEX IF NOT EXISTS idx_shared_results_expires_at
    ON shared_results (expires_at)
    WHERE expires_at IS NOT NULL;

-- Creator-scoped listing (Phase 2: "my shares" dashboard).
CREATE INDEX IF NOT EXISTS idx_shared_results_created_by
    ON shared_results (created_by)
    WHERE created_by IS NOT NULL;

-- Active shares only (used by the sweeper and listing queries).
CREATE INDEX IF NOT EXISTS idx_shared_results_is_active
    ON shared_results (is_active);

-- Contract-scoped analytics (used by the gas-golfing comparison view).
CREATE INDEX IF NOT EXISTS idx_shared_results_contract_id
    ON shared_results (contract_id)
    WHERE contract_id IS NOT NULL;

-- ── updated_at trigger (SQLite) ────────────────────────────────────────────────
-- SQLite does not have ON UPDATE DEFAULT; use a trigger instead.
-- This mirrors the pattern in 001_create_jobs_table.sql.

CREATE TRIGGER IF NOT EXISTS trg_shared_results_updated_at
AFTER UPDATE ON shared_results
FOR EACH ROW
BEGIN
    UPDATE shared_results
    SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
    WHERE id = OLD.id;
END;
