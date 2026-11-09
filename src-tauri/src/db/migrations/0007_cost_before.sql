-- Plan 130 R-i: a job's report shows a shelf row's cost before the write beside the cost after it,
-- so the correction of a row the audit listed can be read in the refresh that made it. NULL: the
-- row was new (an attach), or the item predates this column.
ALTER TABLE job_items ADD COLUMN cost_before INTEGER;
