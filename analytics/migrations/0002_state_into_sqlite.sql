-- ALL state moves into this file so litestream covers 100% of it: file bytes
-- become BLOBs (was: files on disk next to the DB), the dashboard layout and
-- the Places pin cache become tables (were: json files). Table rebuild instead
-- of nullable-then-tighten: the final interface has no nullable content. Rows
-- predating this migration get x'' and are filled by the startup legacy import,
-- which hard-errors on any missing source file.
CREATE TABLE property_files2 (
	id TEXT PRIMARY KEY,
	property_id TEXT NOT NULL REFERENCES properties(id),
	apt INTEGER,
	kind TEXT NOT NULL,
	filename TEXT NOT NULL,
	content_type TEXT NOT NULL,
	content BLOB NOT NULL
);
INSERT INTO property_files2 SELECT id, property_id, apt, kind, filename, content_type, x'' FROM property_files;
DROP TABLE property_files;
ALTER TABLE property_files2 RENAME TO property_files;

-- '' = the global layout; per-user rows will key on the concierge user id
-- (no FK — different store).
CREATE TABLE layouts (
	user TEXT NOT NULL DEFAULT '',
	breakpoint TEXT NOT NULL,
	doc TEXT NOT NULL,
	PRIMARY KEY (user, breakpoint)
);

-- Replaces `<data_dir>/<building>/place.json`; a cache, regenerated on miss.
CREATE TABLE place_cache (
	building TEXT PRIMARY KEY,
	doc TEXT NOT NULL,
	fetched_at TEXT NOT NULL
);
