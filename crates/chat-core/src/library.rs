// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::IoContext;
use crate::media::MediaStore;
use crate::search::SearchIndex;
use crate::{db, Result};

pub const DB_FILE: &str = "library.db";
pub const MEDIA_DIR: &str = "media";
pub const THUMBS_DIR: &str = "thumbs";
pub const SEARCH_INDEX_DIR: &str = "search_index";

/// A library folder: the SQLite database plus media, thumbnails and the search index.
pub struct Library {
    root: PathBuf,
    conn: Connection,
}

impl Library {
    /// Opens the library at `root`, creating the folder layout and database if needed.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        for dir in [MEDIA_DIR, THUMBS_DIR, SEARCH_INDEX_DIR] {
            let path = root.join(dir);
            std::fs::create_dir_all(&path).at(path)?;
        }
        let conn = db::open(&root.join(DB_FILE))?;
        let lib = Self { root, conn };
        lib.media().clean_staging()?;
        Ok(lib)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn media(&self) -> MediaStore {
        MediaStore::new(&self.root)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Removes a chat, its messages, and media files no other chat uses.
    /// Search documents for the chat are removed after the database commit.
    /// Importing a backup that still contains the chat creates it again.
    pub fn delete_thread(&self, index: &SearchIndex, thread_id: i64) -> Result<()> {
        let media = exclusive_media(self.conn(), thread_id)?;
        let tx = self.conn().unchecked_transaction()?;
        tx.execute(
            "DELETE FROM index_queue WHERE message_id IN (SELECT id FROM messages WHERE thread_id = ?1)",
            [thread_id],
        )?;
        let deleted = tx.execute("DELETE FROM threads WHERE id = ?1", [thread_id])?;
        if deleted == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        for (id, _) in &media {
            tx.execute("DELETE FROM media WHERE id = ?1", [id])?;
        }
        tx.commit()?;

        let indexed = index.remove_thread(thread_id);
        for (_, path) in media {
            let Some(path) = path else { continue };
            let full = self.media().absolute(&path);
            if full.is_file() {
                std::fs::remove_file(&full).at(&full)?;
            }
        }
        indexed
    }

    pub fn stats(&self) -> Result<LibraryStats> {
        Ok(self.conn.query_row(
            "SELECT (SELECT COUNT(*) FROM threads),
                    (SELECT COUNT(*) FROM messages),
                    (SELECT COUNT(*) FROM media WHERE status = 'present')",
            [],
            |r| {
                Ok(LibraryStats {
                    threads: r.get(0)?,
                    messages: r.get(1)?,
                    media: r.get(2)?,
                })
            },
        )?)
    }
}

/// Media referenced by this chat and by no other chat, including missing placeholders.
fn exclusive_media(conn: &Connection, thread_id: i64) -> Result<Vec<(i64, Option<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.store_path
         FROM media m
         WHERE (
             EXISTS (SELECT 1 FROM media_refs r WHERE r.media_id = m.id AND r.thread_id = ?1)
             OR EXISTS (
                 SELECT 1 FROM message_parts p
                 JOIN messages msg ON msg.id = p.message_id
                 WHERE p.media_id = m.id AND msg.thread_id = ?1
             )
         )
         AND NOT EXISTS (
             SELECT 1 FROM media_refs r WHERE r.media_id = m.id AND r.thread_id != ?1
         )
         AND NOT EXISTS (
             SELECT 1 FROM message_parts p
             JOIN messages msg ON msg.id = p.message_id
             WHERE p.media_id = m.id AND msg.thread_id != ?1
         )",
    )?;
    let rows = stmt
        .query_map([thread_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct LibraryStats {
    pub threads: i64,
    pub messages: i64,
    pub media: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_layout_and_empty_stats() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        let lib = Library::open(&root).unwrap();

        for d in [MEDIA_DIR, THUMBS_DIR, SEARCH_INDEX_DIR] {
            assert!(root.join(d).is_dir(), "missing {d}");
        }
        assert!(root.join(DB_FILE).is_file());
        assert_eq!(
            lib.stats().unwrap(),
            LibraryStats {
                threads: 0,
                messages: 0,
                media: 0
            }
        );
    }
}
