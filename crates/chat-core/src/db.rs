// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

use rusqlite::Connection;

use crate::{Error, Result};

/// Ordered schema migrations. Index + 1 is the schema version stored in `PRAGMA user_version`.
/// Never edit a released migration; append a new one.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_merge.sql"),
    include_str!("../migrations/0003_repair_owner_counts.sql"),
    include_str!("../migrations/0004_parse_failures.sql"),
];

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
    if current == SCHEMA_VERSION {
        return Ok(());
    }
    // Table rebuilds (create new, copy, drop old, rename) need foreign keys off;
    // the pragma has no effect inside a transaction. Integrity is checked before
    // each commit instead.
    conn.pragma_update(None, "foreign_keys", false)?;
    let result = run_migrations(conn, current);
    conn.pragma_update(None, "foreign_keys", true)?;
    result
}

fn run_migrations(conn: &mut Connection, current: i64) -> Result<()> {
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        let violations: i64 =
            tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })?;
        if violations > 0 {
            return Err(Error::Migration {
                version: i as i64 + 1,
                message: format!("{violations} foreign key violations"),
            });
        }
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
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
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
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        assert_eq!(fk, 1);
    }

    #[test]
    fn upgrades_v1_library_with_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute_batch(
                "INSERT INTO threads (id, ig_thread_id, slug, title) VALUES (1, '1', 'joy_1', 'Joy');
                 INSERT INTO participants (id, thread_id, display_name, alias) VALUES (7, 1, 'Joy', 'Joyful');
                 INSERT INTO messages (thread_id, participant_id, ts_minute, sort_key, fp, occurrence, kind)
                     VALUES (1, 7, 1, 10000, x'00', 0, 'text');",
            )
            .unwrap();
        }

        let conn = open(&path).unwrap();
        let (key, alias): (String, String) = conn
            .query_row(
                "SELECT sender_key, alias FROM participants WHERE id = 7",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((key.as_str(), alias.as_str()), ("Joy", "Joyful"));
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let violations: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
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
