// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::IoContext;
use crate::media::MediaStore;
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
