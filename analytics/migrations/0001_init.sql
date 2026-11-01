-- A `Building` (with its apartments) is persisted whole as a serialised document, so
-- the Rust aggregate is the single source of truth — no column can drift out of sync
-- with the struct. `developers` stays a lookup table the `developer` reference is
-- validated against on write.
CREATE TABLE developers (
	name TEXT PRIMARY KEY,
	note TEXT NOT NULL DEFAULT '',
	page TEXT
);
CREATE TABLE properties (
	id TEXT PRIMARY KEY,
	doc TEXT NOT NULL
);
CREATE TABLE property_files (
	id TEXT PRIMARY KEY,
	property_id TEXT NOT NULL REFERENCES properties(id),
	apt INTEGER,
	kind TEXT NOT NULL,
	filename TEXT NOT NULL,
	content_type TEXT NOT NULL
);
