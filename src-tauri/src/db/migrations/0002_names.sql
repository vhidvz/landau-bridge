-- Supplier links keyed on the raw Barezpakhsh name (spec §14 a). The app moves the old rows
-- back under their name key on the next launch (Db::backfill_name_keys) and drops matches_old.
ALTER TABLE matches RENAME TO matches_old;
CREATE TABLE matches (
  supplier     TEXT NOT NULL,
  name_key     TEXT NOT NULL,           -- name_key(name_seen)
  code         TEXT NOT NULL DEFAULT '',
  product_id   TEXT,
  product_name TEXT,
  how          TEXT NOT NULL,           -- AUTO | USER | NONE | MAP
  similarity   INTEGER,
  name_seen    TEXT NOT NULL,
  priced_at    TEXT,                    -- when this link last answered a price
  decided_at   TEXT NOT NULL,
  PRIMARY KEY (supplier, name_key)
);
CREATE INDEX matches_code ON matches(supplier, code);

-- Report columns (spec §14 d, g).
ALTER TABLE job_items ADD COLUMN available INTEGER;   -- 1/0 per the supplier's quote, NULL unknown
ALTER TABLE job_items ADD COLUMN note TEXT;           -- «؛»-joined remarks for the report

-- The landau-parts map: which raw Barezpakhsh names were imported under which Landau product
-- (spec §14 c). Replaced whole on every load.
CREATE TABLE name_map (
  product_key TEXT NOT NULL,            -- name_key(product name)
  brand_key   TEXT NOT NULL,            -- name_key(brand), '' when none
  raw_name    TEXT NOT NULL,
  code        TEXT NOT NULL,
  available   INTEGER NOT NULL,
  seq         INTEGER NOT NULL,         -- file order
  PRIMARY KEY (product_key, brand_key, raw_name)
);
