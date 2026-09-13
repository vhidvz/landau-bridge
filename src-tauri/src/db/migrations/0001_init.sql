CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE invoices (
  id          INTEGER PRIMARY KEY,
  supplier    TEXT NOT NULL,
  number      TEXT NOT NULL,
  kind        TEXT NOT NULL,            -- SALE | RETURN
  date_jalali TEXT NOT NULL,            -- YYYY/MM/DD zero-padded
  date_iso    TEXT NOT NULL,
  paid        INTEGER NOT NULL,
  url         TEXT NOT NULL,
  fetched_at  TEXT,                     -- NULL until the lines were fetched
  UNIQUE (supplier, number, kind)
);

CREATE TABLE invoice_lines (
  id         INTEGER PRIMARY KEY,
  invoice_id INTEGER NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
  line_no    INTEGER NOT NULL,
  code       TEXT NOT NULL,
  name       TEXT NOT NULL,
  qty        INTEGER NOT NULL,
  unit       TEXT NOT NULL,
  unit_price INTEGER NOT NULL
);
CREATE INDEX invoice_lines_code ON invoice_lines(code);

CREATE TABLE price_snapshots (
  id          INTEGER PRIMARY KEY,
  supplier    TEXT NOT NULL,
  code        TEXT NOT NULL,
  list_price  INTEGER NOT NULL,
  cost        INTEGER NOT NULL,
  vat_percent REAL,
  available   INTEGER NOT NULL,
  fetched_at  TEXT NOT NULL
);
CREATE INDEX price_snapshots_code ON price_snapshots(supplier, code, fetched_at);

CREATE TABLE matches (
  supplier     TEXT NOT NULL,
  code         TEXT NOT NULL,
  product_id   TEXT,
  product_name TEXT,
  how          TEXT NOT NULL,           -- AUTO | USER | NONE
  similarity   INTEGER,
  name_seen    TEXT NOT NULL,
  decided_at   TEXT NOT NULL,
  PRIMARY KEY (supplier, code)
);

CREATE TABLE jobs (
  id           INTEGER PRIMARY KEY,
  kind         TEXT NOT NULL,           -- IMPORT | REFRESH
  mode         TEXT,                    -- QUANTITY | ZERO (import only)
  since_date   TEXT,
  store        TEXT NOT NULL,
  overwrite    INTEGER NOT NULL DEFAULT 0,
  status       TEXT NOT NULL,           -- RUNNING | REVIEW | DONE | FAILED | CANCELLED | INTERRUPTED
  phase        TEXT NOT NULL DEFAULT 'PREFLIGHT',
  started_at   TEXT NOT NULL,
  finished_at  TEXT,
  summary_json TEXT
);

CREATE TABLE job_items (
  id              INTEGER PRIMARY KEY,
  job_id          INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  code            TEXT NOT NULL,
  name            TEXT NOT NULL,
  qty             INTEGER NOT NULL,
  product_id      TEXT,
  product_name    TEXT,
  stock_id        TEXT,
  authenticity    TEXT,
  action          TEXT,                 -- ATTACH | PATCH | SKIP
  qty_before      INTEGER,
  qty_after       INTEGER,
  cost            INTEGER,
  price_before    INTEGER,
  price_after     INTEGER,
  reprice_verdict TEXT,
  error_kind      TEXT,
  error_msg       TEXT,
  candidates_json TEXT,                 -- review candidates, best first (Scored[])
  status          TEXT NOT NULL DEFAULT 'PLANNED'  -- PLANNED | REVIEW | SKIPPED | WRITTEN | FAILED | DONE
);
CREATE INDEX job_items_job ON job_items(job_id);

CREATE TABLE job_events (
  id          INTEGER PRIMARY KEY,
  job_id      INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  at          TEXT NOT NULL,
  level       TEXT NOT NULL,            -- INFO | WARN | ERROR
  phase       TEXT NOT NULL,
  message     TEXT NOT NULL,
  detail_json TEXT
);
CREATE INDEX job_events_job ON job_events(job_id);

CREATE TABLE invoice_imports (
  invoice_id INTEGER PRIMARY KEY REFERENCES invoices(id),
  job_id     INTEGER NOT NULL
);

CREATE TABLE errors (
  id           INTEGER PRIMARY KEY,
  at           TEXT NOT NULL,
  job_id       INTEGER,
  kind         TEXT NOT NULL,
  message      TEXT NOT NULL,
  context_json TEXT
);
