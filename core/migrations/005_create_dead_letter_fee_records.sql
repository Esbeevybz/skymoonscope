-- Migration 005: Durable dead-letter store for fee records (ISSUE-057).
--
-- The fee collector holds failed fee samples in a bounded in-memory buffer.
-- Records that overflow that buffer were previously discarded permanently;
-- they are now written here (stamped with when they were dropped) so operators
-- can replay them after investigating the root cause.

CREATE TABLE IF NOT EXISTS dead_letter_fee_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ledger_sequence BIGINT NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    base_reserve BIGINT NOT NULL,
    base_fee BIGINT NOT NULL,
    max_fee BIGINT NOT NULL,
    fee_charged BIGINT NOT NULL,
    transaction_count INTEGER NOT NULL,
    ledger_close_time TIMESTAMP NOT NULL,
    attempts INTEGER NOT NULL,
    last_error TEXT NOT NULL,
    enqueued_at TIMESTAMP NOT NULL,
    dropped_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_dead_letter_fee_dropped_at
    ON dead_letter_fee_records(dropped_at);
CREATE INDEX IF NOT EXISTS idx_dead_letter_fee_ledger
    ON dead_letter_fee_records(ledger_sequence);
