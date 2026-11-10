-- Plan 130 PI-28: a code the site now lists under another name that conflicts with the link's
-- product holds its row, every run, until the seller says the new name is the same part. The link
-- remembers the rename it last met — product, code, the site's name, the conflicting reasons, when
-- it was seen and when the seller confirmed it (JSON). A confirmation counts for that product, code
-- and name alone; a new decision that moves the link's version forgets it. NULL: none met.
ALTER TABLE supplier_links ADD COLUMN renamed_json TEXT;
