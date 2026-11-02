-- Blocks a page could not parse. Re-importing that page replaces these rows,
-- so a newer parser can recover them instead of leaving the HTML only in the export.

CREATE TABLE parse_failures (
    id INTEGER PRIMARY KEY,
    thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    source_file_id INTEGER REFERENCES source_files(id) ON DELETE SET NULL,
    block_index INTEGER NOT NULL,
    issue TEXT NOT NULL,
    raw_html TEXT NOT NULL
);

CREATE INDEX parse_failures_thread ON parse_failures(thread_id);
