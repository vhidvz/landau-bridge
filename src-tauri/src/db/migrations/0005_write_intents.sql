-- Plan 130 task 10 (R-g, R-o): every shelf write is recorded before it is sent, with the row as it
-- was read just before (the snapshot) and the absolute values the write sets (the target). An
-- intent stays OPEN until its outcome is known: a reply, or the next job's reconciliation, which
-- reads the row and compares it with both.
CREATE TABLE write_intents (
  id              INTEGER PRIMARY KEY,
  job_id          INTEGER NOT NULL,
  kind            TEXT NOT NULL,           -- IMPORT | REFRESH
  store           TEXT NOT NULL,
  product         TEXT NOT NULL,
  authenticity    TEXT NOT NULL,
  stock_id        TEXT,                    -- NULL until known (an attach before its reply)
  verb            TEXT NOT NULL,           -- ATTACH | PATCH | RESTORE
  snap_where      TEXT NOT NULL,           -- LIVE | TRASH | ABSENT: where the row was
  snap_inventory  INTEGER,
  snap_cost       INTEGER,
  snap_updated_at TEXT,
  target_inventory INTEGER,                -- NULL: the write leaves inventory alone
  target_cost     INTEGER NOT NULL,
  items_json      TEXT NOT NULL,           -- the job items this write carries
  lines_json      TEXT NOT NULL DEFAULT '[]', -- [[invoice_id, code], …] marked when it lands
  status          TEXT NOT NULL DEFAULT 'OPEN', -- OPEN | LANDED | NOT_LANDED | SUPERSEDED
  outcome_json    TEXT,                    -- how it closed, and what the row held then
  opened_at       TEXT NOT NULL,
  attempted_at    TEXT,                    -- the last send whose outcome stayed unknown
  closed_at       TEXT,
  resolved_at     TEXT,                    -- a SUPERSEDED intent the seller has settled
  resolution      TEXT                     -- ON_SHELF | TRANSFER_AGAIN
);
CREATE INDEX write_intents_open ON write_intents(status, id);
CREATE INDEX write_intents_row ON write_intents(store, product, authenticity);
