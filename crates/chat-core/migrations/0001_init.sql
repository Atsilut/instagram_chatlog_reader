-- SPDX-License-Identifier: AGPL-3.0-or-later

-- One imported Instagram export (an instagram-{user}-{date}-{hash} root, or a
-- synthetic root for loose thread folders / message files).
CREATE TABLE exports (
    id              INTEGER PRIMARY KEY,
    owner_username  TEXT,
    -- Unix seconds; from the meta-* wrapper, the instagram-* folder date, or the HTML "Generated" time.
    exported_at     INTEGER,
    root_label      TEXT NOT NULL,
    tz_offset_min   INTEGER NOT NULL DEFAULT 0,
    imported_at     INTEGER NOT NULL
);

-- Every HTML or media file seen during import. blake3 lets re-imports skip known files.
CREATE TABLE source_files (
    id              INTEGER PRIMARY KEY,
    export_id       INTEGER NOT NULL REFERENCES exports(id) ON DELETE CASCADE,
    logical_path    TEXT NOT NULL,
    size            INTEGER NOT NULL,
    blake3          BLOB NOT NULL,
    imported_at     INTEGER NOT NULL
);
CREATE INDEX source_files_blake3 ON source_files(blake3);

CREATE TABLE threads (
    id              INTEGER PRIMARY KEY,
    ig_thread_id    TEXT NOT NULL UNIQUE,
    slug            TEXT NOT NULL,
    title           TEXT NOT NULL,
    message_count   INTEGER NOT NULL DEFAULT 0,
    last_sort_key   INTEGER
);

CREATE TABLE participants (
    id              INTEGER PRIMARY KEY,
    thread_id       INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    display_name    TEXT NOT NULL,
    alias           TEXT,
    is_owner        INTEGER NOT NULL DEFAULT 0,
    UNIQUE (thread_id, display_name)
);

CREATE TABLE media (
    id              INTEGER PRIMARY KEY,
    blake3          BLOB UNIQUE,
    mime            TEXT,
    size            INTEGER,
    -- Relative to the library root, e.g. media/ab/<hash>.jpg. NULL while missing.
    store_path      TEXT,
    width           INTEGER,
    height          INTEGER,
    duration_ms     INTEGER,
    status          TEXT NOT NULL CHECK (status IN ('present', 'missing'))
);

-- Maps the export's original file name (e.g. 1285859153169735.jpg) to stored media,
-- so a file supplied by a later import links up with earlier placeholders.
CREATE TABLE media_refs (
    thread_id       INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    original_name   TEXT NOT NULL,
    media_id        INTEGER NOT NULL REFERENCES media(id),
    PRIMARY KEY (thread_id, original_name)
) WITHOUT ROWID;

CREATE TABLE messages (
    id              INTEGER PRIMARY KEY,
    thread_id       INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    participant_id  INTEGER NOT NULL REFERENCES participants(id),
    -- Unix minutes (exports only have minute precision).
    ts_minute       INTEGER NOT NULL,
    -- ts_minute * 10000 + order within that minute.
    sort_key        INTEGER NOT NULL,
    fp              BLOB NOT NULL,
    occurrence      INTEGER NOT NULL,
    kind            TEXT NOT NULL,
    text            TEXT,
    raw_html        TEXT,
    seen_in_latest  INTEGER NOT NULL DEFAULT 1,
    UNIQUE (thread_id, fp, occurrence)
);
CREATE INDEX messages_thread_sort ON messages(thread_id, sort_key);

CREATE TABLE message_parts (
    message_id      INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    idx             INTEGER NOT NULL,
    type            TEXT NOT NULL,
    url             TEXT,
    caption         TEXT,
    author          TEXT,
    media_id        INTEGER REFERENCES media(id),
    PRIMARY KEY (message_id, idx)
) WITHOUT ROWID;

CREATE TABLE reactions (
    message_id      INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    participant_id  INTEGER NOT NULL REFERENCES participants(id),
    emoji           TEXT NOT NULL,
    PRIMARY KEY (message_id, participant_id, emoji)
) WITHOUT ROWID;

CREATE TABLE message_sources (
    message_id      INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    source_file_id  INTEGER NOT NULL REFERENCES source_files(id) ON DELETE CASCADE,
    PRIMARY KEY (message_id, source_file_id)
) WITHOUT ROWID;

-- Hash of one day's fingerprints and reactions per thread. Lets a re-import skip
-- unchanged days; deduplication itself relies on messages(thread_id, fp, occurrence).
CREATE TABLE day_digests (
    thread_id       INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    day             INTEGER NOT NULL,
    digest          BLOB NOT NULL,
    PRIMARY KEY (thread_id, day)
) WITHOUT ROWID;

-- Messages changed since the search index was last updated.
CREATE TABLE index_queue (
    message_id      INTEGER PRIMARY KEY
);
