-- Plan 130 (R-a, R-n): a link writes only once a job has validated it. Every stored link moves to
-- supplier_links as UNVERIFIED, and `matches` is dropped in the same transaction, so a build older
-- than the schema guard (0.1.0) finds no links and writes nothing from them.
CREATE TABLE supplier_links (
  supplier        TEXT NOT NULL,
  name_key        TEXT NOT NULL,           -- name_key(name_seen)
  code            TEXT NOT NULL DEFAULT '',
  product_id      TEXT,
  product_name    TEXT,
  product_brand   TEXT,                    -- NULL: never recorded; '': recorded as none
  product_alias   TEXT,                    -- the same convention
  how             TEXT NOT NULL,           -- AUTO | USER | NONE
  similarity      INTEGER,
  name_seen       TEXT NOT NULL,
  priced_at       TEXT,
  decided_at      TEXT NOT NULL,
  state           TEXT NOT NULL DEFAULT 'UNVERIFIED', -- UNVERIFIED | VALID | REBUILT | NEEDS_REVIEW | STALE
  state_reason    TEXT,                    -- JSON: the write table's reasons, or why it went stale
  state_at        TEXT,
  validated_job   INTEGER,                 -- the job whose VALIDATE made it writable (R-b)
  prev_product_id TEXT,                    -- the dead product a rebuild replaced (Q-1)
  link_version    INTEGER NOT NULL DEFAULT 1, -- bumped on every change of product or decision
  PRIMARY KEY (supplier, name_key)
);
CREATE INDEX supplier_links_code ON supplier_links(supplier, code);
CREATE INDEX supplier_links_product ON supplier_links(supplier, product_id);

INSERT INTO supplier_links(supplier, name_key, code, product_id, product_name, how, similarity,
                           name_seen, priced_at, decided_at, state, state_at)
  SELECT supplier, name_key, code, product_id, product_name, how, similarity,
         name_seen, priced_at, decided_at, 'UNVERIFIED', strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
  FROM matches;

-- The brand and alias a link's product carried, from the candidates the review screen saved for it
-- (the latest one wins). A product never offered as a candidate keeps NULL: never recorded.
CREATE TABLE link_identity_backfill AS
  SELECT json_extract(c.value, '$.candidate.id')    AS product_id,
         json_extract(c.value, '$.candidate.brand') AS brand,
         json_extract(c.value, '$.candidate.alias') AS alias,
         ji.id                                       AS item_id
  FROM job_items ji, json_each(ji.candidates_json) c
  WHERE ji.candidates_json IS NOT NULL AND json_valid(ji.candidates_json)
    AND json_type(ji.candidates_json) = 'array';
CREATE INDEX link_identity_backfill_product ON link_identity_backfill(product_id, item_id);
UPDATE supplier_links SET
  product_brand = (SELECT IFNULL(b.brand, '') FROM link_identity_backfill b
                   WHERE b.product_id = supplier_links.product_id ORDER BY b.item_id DESC LIMIT 1),
  product_alias = (SELECT IFNULL(b.alias, '') FROM link_identity_backfill b
                   WHERE b.product_id = supplier_links.product_id ORDER BY b.item_id DESC LIMIT 1)
WHERE product_id IS NOT NULL;
DROP TABLE link_identity_backfill;

DROP TABLE matches;

-- Every change of a link, append-only (R-a): who, from which product to which, in which job, and
-- whether the evidence was shown to the seller or reconstructed afterwards.
CREATE TABLE match_events (
  id             INTEGER PRIMARY KEY,
  supplier       TEXT NOT NULL,
  name_key       TEXT NOT NULL,
  at             TEXT NOT NULL,
  job_id         INTEGER,
  actor          TEXT NOT NULL,            -- SELLER | AUTO | REBUILD | IMPORT | VALIDATE | WRITE
  old_product_id TEXT,
  new_product_id TEXT,
  old_state      TEXT,
  new_state      TEXT,
  evidence       TEXT,                     -- shown | reconstructed | NULL
  detail_json    TEXT
);
CREATE INDEX match_events_link ON match_events(supplier, name_key, id);

-- Which link an item was planned from, at which version, and the product a link still under review
-- points at (collisions, R-e); the write table's reasons for the screens (R-k).
ALTER TABLE job_items ADD COLUMN link_key TEXT;
ALTER TABLE job_items ADD COLUMN link_version INTEGER;
ALTER TABLE job_items ADD COLUMN link_product TEXT;
ALTER TABLE job_items ADD COLUMN reasons_json TEXT;
