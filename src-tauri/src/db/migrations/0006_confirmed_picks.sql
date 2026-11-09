-- Plan 130 slice 4 (R-d): a seller's pick confirmed against a conflict remembers exactly the
-- reasons the seller was shown. A later VALIDATE writes the stored pick while its conflicts are all
-- among them (Q-5: a confirmation counts for what the seller saw); any new conflict reopens it. A
-- new pick or a clear writes the column again. NULL: nothing was confirmed.
ALTER TABLE supplier_links ADD COLUMN confirmed_json TEXT;
