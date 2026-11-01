-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Participants are keyed by sender_key instead of display name. In one-to-one chats
-- the key is a role ('@self' / '@other'), so a display-name change between backups
-- doesn't split one person into two (and duplicate their messages).
CREATE TABLE participants_new (
    id              INTEGER PRIMARY KEY,
    thread_id       INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    sender_key      TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    alias           TEXT,
    is_owner        INTEGER NOT NULL DEFAULT 0,
    UNIQUE (thread_id, sender_key)
);
INSERT INTO participants_new (id, thread_id, sender_key, display_name, alias, is_owner)
    SELECT id, thread_id, display_name, display_name, alias, is_owner FROM participants;
DROP TABLE participants;
ALTER TABLE participants_new RENAME TO participants;

ALTER TABLE threads ADD COLUMN kind TEXT CHECK (kind IN ('direct', 'group'));
-- Export time (Unix seconds) the current title and display names came from.
ALTER TABLE threads ADD COLUMN title_at INTEGER;
-- Newest export (Unix seconds) that contained every page of this chat.
ALTER TABLE threads ADD COLUMN latest_full_export_at INTEGER;

-- Replaced by last_seen_at: a message is missing from the latest backup when
-- last_seen_at < threads.latest_full_export_at.
ALTER TABLE messages DROP COLUMN seen_in_latest;
ALTER TABLE messages ADD COLUMN last_seen_at INTEGER;
-- Hash of sender and content without the time; used to detect timezone shifts.
ALTER TABLE messages ADD COLUMN content_key BLOB;
CREATE INDEX messages_thread_content ON messages(thread_id, content_key);
