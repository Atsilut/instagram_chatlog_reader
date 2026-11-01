// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

use rusqlite::Connection;

use crate::{Error, Result};

/// Ordered schema migrations. Index + 1 is the schema version stored in `PRAGMA user_version`.
/// Never edit a released migration; append a new one.
const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_init.sql")];

pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

pub fn open(path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         PRAGMA temp_store = MEMORY;
         PRAGMA mmap_size = 268435456;",
    )?;
    Ok(())
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(Error::SchemaTooNew {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn creates_schema_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");

        let conn = open(&path).unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let first = tables(&conn);
        for t in ["exports", "threads", "messages", "media", "day_digests"] {
            assert!(first.iter().any(|n| n == t), "missing table {t}");
        }
        drop(conn);

        let conn = open(&path).unwrap();
        assert_eq!(tables(&conn), first);
    }

    #[test]
    fn uses_wal_and_foreign_keys() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("library.db")).unwrap();
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        let fk: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, "wal");
        assert_eq!(fk, 1);
    }

    #[test]
    fn rejects_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(matches!(open(&path), Err(Error::SchemaTooNew { .. })));
    }
}
