-- Plan 130 PI-19: the seller's word that a shelf row's stock was counted after the transfers listed
-- with it — units quantity imports moved onto the row from items whose link does not prove there.
-- The audit leaves a counted transfer out; one made later is listed again. A count is withdrawn,
-- never deleted, so the record of what the seller said stays.
CREATE TABLE unit_counts (
  id           INTEGER PRIMARY KEY,
  store        TEXT NOT NULL,
  stock_id     TEXT NOT NULL,
  inventory    INTEGER NOT NULL,          -- the row's inventory as the audit read it
  read_at      TEXT NOT NULL,             -- when the audit read it
  counted_at   TEXT NOT NULL,             -- when the seller said so
  withdrawn_at TEXT
);
CREATE TABLE unit_count_items (
  count_id INTEGER NOT NULL REFERENCES unit_counts(id),
  item_id  INTEGER NOT NULL REFERENCES job_items(id),  -- the transfer: a quantity import's write
  qty      INTEGER NOT NULL,                           -- the units it moved onto the row
  PRIMARY KEY (count_id, item_id)
);
CREATE INDEX unit_counts_row ON unit_counts(store, stock_id);
