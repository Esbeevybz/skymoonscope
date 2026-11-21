-- Migration 004: Persist the verified contract whitelist (ISSUE-058).
--
-- The in-memory `ContractRegister` is seeded from the static `VERIFIED_CONTRACTS`
-- list at startup. Dynamic registrations are written through to this table so
-- they are no longer lost when the process restarts.

CREATE TABLE IF NOT EXISTS contract_whitelist (
    contract_id TEXT PRIMARY KEY,
    wasm_hash TEXT NOT NULL,
    registered_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
