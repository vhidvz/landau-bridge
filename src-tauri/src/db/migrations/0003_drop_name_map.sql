-- The landau-parts name map is gone (2026-10-30). Links it made stay as automatic matches, so a
-- refresh keeps costing those rows; the table and its two settings go with the feature.
UPDATE matches SET how = 'AUTO' WHERE how = 'MAP';
DROP TABLE IF EXISTS name_map;
DELETE FROM settings WHERE key IN ('name_map_file', 'name_map_loaded_at');
