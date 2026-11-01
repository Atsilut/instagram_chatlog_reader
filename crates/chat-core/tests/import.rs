// SPDX-License-Identifier: AGPL-3.0-or-later

//! End-to-end merge tests: synthetic backups are written to disk and imported.

mod common;

use std::path::PathBuf;

use chat_core::import::{import, ImportOptions};
use chat_core::Library;
use common::{library, m, run, write_export, M, ME};

/// `(sender display name, text)` of the chat in display order.
fn timeline(lib: &Library) -> Vec<(String, String)> {
    let mut stmt = lib
        .conn()
        .prepare(
            "SELECT p.display_name, COALESCE(m.text, '') FROM messages m JOIN participants p ON p.id = m.participant_id
             ORDER BY m.sort_key",
        )
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn texts(lib: &Library) -> Vec<String> {
    timeline(lib).into_iter().map(|(_, t)| t).collect()
}

fn scalar(lib: &Library, sql: &str) -> i64 {
    lib.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}

fn base_chat() -> Vec<M> {
    vec![
        m("Joy", "Jul 01, 2025 9:00 am", "good morning"),
        m(ME, "Jul 01, 2025 9:00 am", "morning!"),
        m("Joy", "Jul 01, 2025 9:05 pm", "what are you doing"),
        m(ME, "Jul 02, 2025 10:00 am", "studying japanese"),
        m("Joy", "Jul 02, 2025 10:01 am", "nice"),
        m(ME, "Jul 03, 2025 8:00 pm", "dinner?"),
    ]
}

#[test]
fn a_third_person_on_a_later_page_keeps_the_chat_a_group() {
    let tmp = tempfile::tempdir().unwrap();
    let msgs = vec![
        m("Ann", "Jul 01, 2025 9:00 am", "hello there friend"),
        m(ME, "Jul 01, 2025 9:01 am", "hello there back"),
        m("Cat", "Jul 01, 2025 9:02 am", "joining this conversation"),
    ];
    // Two per page, so the oldest page is only Ann and Bob and the title is Ann.
    let export = write_export(tmp.path(), "2025-07-02", "Ann", &msgs, 2, &[]);
    let mut lib = library(tmp.path());
    run(&mut lib, &[export]);

    let kind: String = lib
        .conn()
        .query_row("SELECT kind FROM threads", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kind, "group");
    assert_eq!(scalar(&lib, "SELECT COUNT(*) FROM messages"), 3);
    let keys = sender_keys(&lib);
    assert!(
        keys.iter().all(|k| !k.starts_with('@')),
        "group senders keep display names, got {keys:?}"
    );
}

#[test]
fn a_later_backup_that_reveals_a_group_does_not_duplicate() {
    let tmp = tempfile::tempdir().unwrap();
    let msgs = vec![
        m("Ann", "Jul 01, 2025 9:00 am", "hello there friend"),
        m(ME, "Jul 01, 2025 9:01 am", "hello there back"),
        m("Cat", "Jul 01, 2025 9:02 am", "joining this conversation"),
    ];
    let first = write_export(tmp.path(), "2025-07-02", "Ann", &msgs[..2], 10, &[]);
    let second = write_export(tmp.path(), "2025-07-03", "Ann", &msgs, 10, &[]);
    let mut lib = library(tmp.path());
    run(&mut lib, &[first]);
    run(&mut lib, &[second]);

    assert_eq!(scalar(&lib, "SELECT COUNT(*) FROM messages"), 3);
    let kind: String = lib
        .conn()
        .query_row("SELECT kind FROM threads", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kind, "group");
    assert_eq!(
        texts(&lib),
        vec![
            "hello there friend",
            "hello there back",
            "joining this conversation",
        ]
    );
}

fn sender_keys(lib: &Library) -> Vec<String> {
    let mut stmt = lib
        .conn()
        .prepare("SELECT sender_key FROM participants ORDER BY sender_key")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn single_import_stores_everything_in_order() {
    let tmp = tempfile::tempdir().unwrap();
    let export = write_export(tmp.path(), "2025-07-04", "Joy", &base_chat(), 4, &[]);
    let mut lib = library(tmp.path());

    let r = run(&mut lib, &[export]);
    assert_eq!(r.messages_new, 6);
    assert_eq!(r.pages_parsed, 2);
    assert_eq!(
        texts(&lib),
        vec![
            "good morning",
            "morning!",
            "what are you doing",
            "studying japanese",
            "nice",
            "dinner?"
        ]
    );
    let owner: String = lib
        .conn()
        .query_row(
            "SELECT display_name FROM participants WHERE is_owner = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(owner, ME);
    assert_eq!(scalar(&lib, "SELECT message_count FROM threads"), 6);
    assert_eq!(scalar(&lib, "SELECT kind = 'direct' FROM threads"), 1);
}

#[test]
fn reimporting_the_same_backup_adds_nothing_and_skips_days() {
    let tmp = tempfile::tempdir().unwrap();
    let export = write_export(tmp.path(), "2025-07-04", "Joy", &base_chat(), 100, &[]);
    let mut lib = library(tmp.path());
    run(&mut lib, std::slice::from_ref(&export));

    let r = run(&mut lib, &[export]);
    assert_eq!(r.messages_new, 0);
    assert_eq!(r.messages_existing, 6);
    assert_eq!(r.days_skipped, 3);
    assert_eq!(r.days_merged, 0);
    assert_eq!(scalar(&lib, "SELECT COUNT(*) FROM messages"), 6);
}

#[test]
fn overlapping_backups_merge_without_duplicates() {
    let tmp = tempfile::tempdir().unwrap();
    let chat = base_chat();
    let mut later = chat[2..].to_vec();
    later.push(m("Joy", "Jul 04, 2025 7:00 am", "yes dinner"));
    let a = write_export(tmp.path(), "2025-07-03", "Joy", &chat[..4], 2, &[]);
    let b = write_export(tmp.path(), "2025-07-05", "Joy", &later, 3, &[]);
    let mut lib = library(tmp.path());

    let r = run(&mut lib, &[b, a]);
    assert_eq!(r.exports, 2);
    assert_eq!(r.messages_new, 7);
    assert_eq!(
        texts(&lib),
        vec![
            "good morning",
            "morning!",
            "what are you doing",
            "studying japanese",
            "nice",
            "dinner?",
            "yes dinner"
        ]
    );
}

#[test]
fn repeated_messages_in_one_minute_are_kept_but_not_multiplied() {
    let tmp = tempfile::tempdir().unwrap();
    let lol = |n: usize| -> Vec<M> {
        (0..n)
            .map(|_| m("Joy", "Jul 01, 2025 9:00 am", "lol"))
            .collect()
    };
    let a = write_export(tmp.path(), "2025-07-02", "Joy", &lol(2), 10, &[]);
    let b = write_export(tmp.path(), "2025-07-03", "Joy", &lol(2), 10, &[]);
    let mut lib = library(tmp.path());
    run(&mut lib, &[a, b]);
    assert_eq!(scalar(&lib, "SELECT COUNT(*) FROM messages"), 2);

    let c = write_export(tmp.path(), "2025-07-04", "Joy", &lol(3), 10, &[]);
    run(&mut lib, &[c]);
    assert_eq!(scalar(&lib, "SELECT COUNT(*) FROM messages"), 3);
}

#[test]
fn new_message_is_placed_inside_its_minute() {
    let tmp = tempfile::tempdir().unwrap();
    let t = "Jul 01, 2025 9:00 am";
    let a = write_export(
        tmp.path(),
        "2025-07-02",
        "Joy",
        &[m("Joy", t, "a"), m("Joy", t, "b")],
        10,
        &[],
    );
    let b = write_export(
        tmp.path(),
        "2025-07-03",
        "Joy",
        &[m("Joy", t, "a"), m(ME, t, "x"), m("Joy", t, "b")],
        10,
        &[],
    );
    let mut lib = library(tmp.path());
    run(&mut lib, &[a]);
    run(&mut lib, &[b]);
    assert_eq!(texts(&lib), vec!["a", "x", "b"]);
}

#[test]
fn a_minute_split_across_pages_keeps_its_order() {
    let tmp = tempfile::tempdir().unwrap();
    let t = "Jul 01, 2025 9:00 am";
    let msgs: Vec<M> = ["1", "2", "3", "4", "5"]
        .iter()
        .map(|s| m("Joy", t, s))
        .collect();
    let export = write_export(tmp.path(), "2025-07-02", "Joy", &msgs, 2, &[]);
    let mut lib = library(tmp.path());
    run(&mut lib, &[export]);
    assert_eq!(texts(&lib), vec!["1", "2", "3", "4", "5"]);
}

#[test]
fn reactions_from_a_later_backup_are_added() {
    let tmp = tempfile::tempdir().unwrap();
    let a = write_export(tmp.path(), "2025-07-02", "Joy", &base_chat(), 100, &[]);
    let mut chat = base_chat();
    chat[1] = chat[1].clone().reacted("😂", "Joy");
    let b = write_export(tmp.path(), "2025-07-03", "Joy", &chat, 100, &[]);
    let mut lib = library(tmp.path());
    run(&mut lib, &[a]);
    let r = run(&mut lib, &[b]);

    assert_eq!(r.messages_new, 0);
    assert_eq!(
        r.days_merged, 1,
        "only the day with the new reaction is merged"
    );
    let (emoji, by): (String, String) = lib
        .conn()
        .query_row(
            "SELECT r.emoji, p.sender_key FROM reactions r JOIN participants p ON p.id = r.participant_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((emoji.as_str(), by.as_str()), ("😂", "@other"));
}

#[test]
fn display_name_change_does_not_duplicate_history() {
    let tmp = tempfile::tempdir().unwrap();
    let a = write_export(tmp.path(), "2025-07-02", "Joy", &base_chat(), 100, &[]);
    let renamed: Vec<M> = base_chat()
        .into_iter()
        .map(|mut x| {
            if x.sender == "Joy" {
                x.sender = "Joy 🌸".into();
            }
            x
        })
        .collect();
    let b = write_export(tmp.path(), "2025-07-09", "Joy 🌸", &renamed, 100, &[]);
    let mut lib = library(tmp.path());
    run(&mut lib, &[a, b]);

    assert_eq!(scalar(&lib, "SELECT COUNT(*) FROM messages"), 6);
    assert_eq!(timeline(&lib)[0].0, "Joy 🌸", "newest backup's name wins");
    let title: String = lib
        .conn()
        .query_row("SELECT title FROM threads", [], |r| r.get(0))
        .unwrap();
    assert_eq!(title, "Joy 🌸");
}

#[test]
fn timezone_shifted_backup_is_detected_and_aligned() {
    let tmp = tempfile::tempdir().unwrap();
    let chat: Vec<M> = (0..12)
        .map(|i| {
            m(
                if i % 2 == 0 { "Joy" } else { ME },
                &format!("Jul 01, 2025 {}:30 am", i % 12 + 1),
                &format!("message number {i}"),
            )
        })
        .collect();
    // Same messages exported from a device one hour ahead.
    let shifted: Vec<M> = (0..12)
        .map(|i| {
            m(
                if i % 2 == 0 { "Joy" } else { ME },
                &format!("Jul 01, 2025 {}:30 am", i % 12 + 2),
                &format!("message number {i}"),
            )
        })
        .filter(|x| !x.time.contains(" 13:"))
        .collect();
    let a = write_export(tmp.path(), "2025-07-02", "Joy", &chat[..11], 100, &[]);
    let b = write_export(tmp.path(), "2025-07-03", "Joy", &shifted, 100, &[]);
    let mut lib = library(tmp.path());
    run(&mut lib, &[a]);
    let r = run(&mut lib, &[b]);

    assert_eq!(r.tz_shifts.len(), 1, "{:?}", r.warnings);
    assert_eq!(r.tz_shifts[0].minutes, -60);
    assert_eq!(scalar(&lib, "SELECT COUNT(*) FROM messages"), 12);
}

#[test]
fn messages_missing_from_the_latest_complete_backup_are_kept_and_flagged() {
    let tmp = tempfile::tempdir().unwrap();
    let chat = base_chat();
    let mut unsent = chat.clone();
    unsent.remove(2);
    let a = write_export(tmp.path(), "2025-07-04", "Joy", &chat, 100, &[]);
    let b = write_export(tmp.path(), "2025-07-10", "Joy", &unsent, 100, &[]);
    let mut lib = library(tmp.path());
    run(&mut lib, &[a, b]);

    assert_eq!(scalar(&lib, "SELECT COUNT(*) FROM messages"), 6);
    let missing: String = lib
        .conn()
        .query_row(
            "SELECT m.text FROM messages m JOIN threads t ON t.id = m.thread_id
             WHERE m.last_seen_at < t.latest_full_export_at",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(missing, "what are you doing");
}

#[test]
fn media_is_linked_and_missing_files_are_filled_in_later() {
    let tmp = tempfile::tempdir().unwrap();
    let chat = vec![
        m("Joy", "Jul 01, 2025 9:00 am", "").photo("1.jpg"),
        m(ME, "Jul 01, 2025 9:01 am", "").photo("2.jpg"),
    ];
    let a = write_export(tmp.path(), "2025-07-02", "Joy", &chat, 100, &["1.jpg"]);
    let mut lib = library(tmp.path());
    let r = run(&mut lib, &[a]);
    assert_eq!(r.media_files_stored, 1);
    assert_eq!(r.media_missing, 1);

    let b = write_export(
        tmp.path(),
        "2025-07-03",
        "Joy",
        &chat,
        100,
        &["1.jpg", "2.jpg"],
    );
    let r = run(&mut lib, &[b]);
    assert_eq!(r.media_files_stored, 1);
    assert_eq!(r.media_missing, 0);
    assert_eq!(
        scalar(&lib, "SELECT COUNT(*) FROM message_parts p JOIN media m ON m.id = p.media_id WHERE m.status = 'present'"),
        2
    );
}

#[test]
fn owner_and_counts_are_saved_page_by_page() {
    let tmp = tempfile::tempdir().unwrap();
    let export = write_export(tmp.path(), "2025-07-04", "Joy", &base_chat(), 2, &[]);
    let mut lib = library(tmp.path());
    let db = lib.root().join("library.db");
    let seen = std::sync::Mutex::new(Vec::new());
    let progress = |_: &chat_core::import::Progress| {
        // A second connection sees what an interrupted import would leave behind.
        let c = rusqlite::Connection::open(&db).unwrap();
        let state: (i64, i64, i64) = c
            .query_row(
                "SELECT t.message_count, (SELECT COUNT(*) FROM messages),
                        (SELECT COUNT(*) FROM participants WHERE is_owner = 1 AND display_name = 'Bob')
                 FROM threads t",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        seen.lock().unwrap().push(state);
    };
    let opts = ImportOptions {
        cancel: None,
        progress: Some(&progress),
    };
    import(&mut lib, &[export], &opts).unwrap();

    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.len(), 3);
    for (count, actual, owners) in seen {
        assert_eq!(count, actual, "message_count matches after every page");
        assert_eq!(owners, 1, "owner is marked from the first page");
    }
}

#[test]
fn cancel_stops_before_the_next_batch() {
    let tmp = tempfile::tempdir().unwrap();
    let export = write_export(tmp.path(), "2025-07-04", "Joy", &base_chat(), 1, &[]);
    let mut lib = library(tmp.path());
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let opts = ImportOptions {
        cancel: Some(&cancel),
        progress: None,
    };
    assert!(matches!(
        import(&mut lib, &[export], &opts),
        Err(chat_core::Error::Cancelled)
    ));
}

/// Imports a real export from `ICV_SAMPLE_DIR` into a temporary library, twice.
#[test]
#[ignore]
fn real_sample_import() {
    let Ok(dir) = std::env::var("ICV_SAMPLE_DIR") else {
        eprintln!("ICV_SAMPLE_DIR not set; skipping");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = library(tmp.path());
    for round in 1..=2 {
        let start = std::time::Instant::now();
        let r = run(&mut lib, &[PathBuf::from(&dir)]);
        eprintln!(
            "round {round} in {:.1?}: {}",
            start.elapsed(),
            serde_json::to_string_pretty(&r).unwrap()
        );
    }
    eprintln!("library: {:?}", lib.stats().unwrap());
}
