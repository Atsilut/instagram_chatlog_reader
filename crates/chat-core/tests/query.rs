// SPDX-License-Identifier: AGPL-3.0-or-later

mod common;

use chat_core::query::{self, Window};
use chat_core::search::SearchIndex;
use common::{library, m, run, write_chat, M, ME};

fn day_chat(prefix: &str, count: usize) -> Vec<M> {
    (0..count)
        .map(|i| {
            let sender = if i % 2 == 0 { "Joy" } else { ME };
            m(
                sender,
                &format!(
                    "Jul {:02}, 2025 {}:{:02} am",
                    1 + i / 600,
                    1 + (i / 60) % 10,
                    i % 60
                ),
                &format!("{prefix} {i}"),
            )
        })
        .collect()
}

fn texts(v: &[query::MessageView]) -> Vec<String> {
    v.iter()
        .map(|m| m.text.clone().unwrap_or_default())
        .collect()
}

#[test]
fn chat_list_is_newest_first_with_preview() {
    let tmp = tempfile::tempdir().unwrap();
    write_chat(
        tmp.path(),
        "2025-08-01",
        "joy_111",
        "Joy",
        &[m("Joy", "Jul 01, 2025 9:00 am", "old chat")],
        10,
        &[],
    );
    let export = write_chat(
        tmp.path(),
        "2025-08-01",
        "amy_222",
        "Amy",
        &[
            m("Amy", "Jul 05, 2025 9:00 am", "hi"),
            m(ME, "Jul 05, 2025 9:01 am", "newest"),
        ],
        10,
        &[],
    );
    let mut lib = library(tmp.path());
    run(&mut lib, &[export]);

    let threads = query::threads(lib.conn()).unwrap();
    assert_eq!(
        threads.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
        vec!["Amy", "Joy"]
    );
    let amy = &threads[0];
    assert_eq!(amy.message_count, 2);
    assert_eq!(amy.last_text.as_deref(), Some("newest"));
    let owner = amy.participants.iter().find(|p| p.is_owner).unwrap();
    assert_eq!(owner.display_name, ME);
    assert_eq!(amy.last_participant_id, Some(owner.id));
}

#[test]
fn pages_through_a_chat_in_both_directions() {
    let tmp = tempfile::tempdir().unwrap();
    let chat = day_chat("msg", 250);
    let export = write_chat(tmp.path(), "2025-08-01", "joy_111", "Joy", &chat, 100, &[]);
    let mut lib = library(tmp.path());
    run(&mut lib, &[export]);
    let thread = query::threads(lib.conn()).unwrap()[0].id;

    let latest = query::messages(lib.conn(), thread, Window::Latest, 100).unwrap();
    assert_eq!(latest.len(), 100);
    assert_eq!(latest.last().unwrap().text.as_deref(), Some("msg 249"));
    assert_eq!(latest.first().unwrap().text.as_deref(), Some("msg 150"));

    let older =
        query::messages(lib.conn(), thread, Window::Before(latest[0].sort_key), 100).unwrap();
    assert_eq!(texts(&older)[0], "msg 50");
    assert_eq!(texts(&older)[99], "msg 149");

    let oldest =
        query::messages(lib.conn(), thread, Window::Before(older[0].sort_key), 100).unwrap();
    assert_eq!(oldest.len(), 50);
    assert_eq!(texts(&oldest)[0], "msg 0");

    let newer = query::messages(lib.conn(), thread, Window::After(oldest[49].sort_key), 3).unwrap();
    assert_eq!(texts(&newer), vec!["msg 50", "msg 51", "msg 52"]);
    assert!(
        query::messages(lib.conn(), thread, Window::After(latest[99].sort_key), 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn jumps_to_a_message_with_context_on_both_sides() {
    let tmp = tempfile::tempdir().unwrap();
    let export = write_chat(
        tmp.path(),
        "2025-08-01",
        "joy_111",
        "Joy",
        &day_chat("msg", 100),
        100,
        &[],
    );
    let mut lib = library(tmp.path());
    run(&mut lib, &[export]);
    let thread = query::threads(lib.conn()).unwrap()[0].id;
    let all = query::messages(lib.conn(), thread, Window::Latest, 1000).unwrap();
    let target = &all[40];

    let around = query::messages(lib.conn(), thread, Window::Around(target.sort_key), 10).unwrap();
    assert_eq!(
        texts(&around),
        (35..45).map(|i| format!("msg {i}")).collect::<Vec<_>>()
    );
    assert_eq!(around[5].id, target.id);

    let at = query::sort_key_at(lib.conn(), thread, all[60].ts_minute).unwrap();
    assert_eq!(at, Some(all[60].sort_key));
}

#[test]
fn messages_carry_media_reactions_and_missing_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let chat = vec![
        m("Joy", "Jul 01, 2025 9:00 am", "").photo("1.jpg"),
        m(ME, "Jul 01, 2025 9:01 am", "ha").reacted("😂", "Joy"),
        m("Joy", "Jul 01, 2025 9:02 am", "oops"),
    ];
    let a = write_chat(
        tmp.path(),
        "2025-08-01",
        "joy_111",
        "Joy",
        &chat,
        100,
        &["1.jpg"],
    );
    let b = write_chat(
        tmp.path(),
        "2025-08-05",
        "joy_111",
        "Joy",
        &chat[..2],
        100,
        &["1.jpg"],
    );
    let mut lib = library(tmp.path());
    run(&mut lib, &[a, b]);
    let t = &query::threads(lib.conn()).unwrap()[0];
    let msgs = query::messages(lib.conn(), t.id, Window::Latest, 10).unwrap();

    let photo = &msgs[0].parts[0];
    assert_eq!(photo.kind, "photo");
    let media = photo.media.as_ref().unwrap();
    assert!(media.present);
    assert!(media.store_path.as_deref().unwrap().starts_with("media/"));

    let joy = t.participants.iter().find(|p| !p.is_owner).unwrap();
    assert_eq!(msgs[1].reactions.len(), 1);
    assert_eq!(msgs[1].reactions[0].emoji, "😂");
    assert_eq!(msgs[1].reactions[0].participant_id, joy.id);

    assert!(!msgs[1].missing_from_latest);
    assert!(
        msgs[2].missing_from_latest,
        "unsent before the newer backup"
    );
}

#[test]
fn owner_is_recognised_in_group_chats() {
    let tmp = tempfile::tempdir().unwrap();
    write_chat(
        tmp.path(),
        "2025-08-01",
        "joy_111",
        "Joy",
        &[
            m("Joy", "Jul 01, 2025 9:00 am", "hi"),
            m(ME, "Jul 01, 2025 9:01 am", "hey"),
        ],
        10,
        &[],
    );
    let group = vec![
        m("Ann", "Jul 02, 2025 9:00 am", "trip?"),
        m(ME, "Jul 02, 2025 9:01 am", "yes"),
        m("Cat", "Jul 02, 2025 9:02 am", "me too"),
    ];
    let export = write_chat(
        tmp.path(),
        "2025-08-01",
        "trip_222",
        "Weekend trip",
        &group,
        10,
        &[],
    );
    let mut lib = library(tmp.path());
    run(&mut lib, &[export]);

    let trip = query::threads(lib.conn())
        .unwrap()
        .into_iter()
        .find(|t| t.title == "Weekend trip")
        .unwrap();
    assert_eq!(trip.kind.as_deref(), Some("group"));
    let owners: Vec<&str> = trip
        .participants
        .iter()
        .filter(|p| p.is_owner)
        .map(|p| p.display_name.as_str())
        .collect();
    assert_eq!(owners, vec![ME]);
}

#[test]
fn aliases_can_be_set_and_cleared() {
    let tmp = tempfile::tempdir().unwrap();
    let export = write_chat(
        tmp.path(),
        "2025-08-01",
        "joy_111",
        "Joy",
        &day_chat("x", 2),
        10,
        &[],
    );
    let mut lib = library(tmp.path());
    run(&mut lib, &[export]);
    let joy = query::threads(lib.conn()).unwrap()[0]
        .participants
        .iter()
        .find(|p| !p.is_owner)
        .unwrap()
        .clone();

    query::set_alias(lib.conn(), joy.id, Some("  Joy (school)  ")).unwrap();
    let p = query::participants(lib.conn(), joy.thread_id).unwrap();
    assert_eq!(
        p.iter().find(|x| x.id == joy.id).unwrap().alias.as_deref(),
        Some("Joy (school)")
    );

    query::set_alias(lib.conn(), joy.id, Some("   ")).unwrap();
    let p = query::participants(lib.conn(), joy.thread_id).unwrap();
    assert_eq!(p.iter().find(|x| x.id == joy.id).unwrap().alias, None);
}

#[test]
fn hiding_a_chat_keeps_its_messages_and_can_be_undone() {
    let tmp = tempfile::tempdir().unwrap();
    let export = write_chat(
        tmp.path(),
        "2025-08-01",
        "joy_111",
        "Joy",
        &[m("Joy", "Jul 01, 2025 9:00 am", "still here")],
        10,
        &[],
    );
    write_chat(
        tmp.path(),
        "2025-08-01",
        "amy_222",
        "Amy",
        &[m("Amy", "Jul 02, 2025 9:00 am", "visible")],
        10,
        &[],
    );
    let mut lib = library(tmp.path());
    run(&mut lib, &[export]);

    let threads = query::threads(lib.conn()).unwrap();
    assert!(threads.iter().all(|t| !t.hidden));
    let joy = threads.iter().find(|t| t.title == "Joy").unwrap().id;
    query::set_hidden(lib.conn(), joy, true).unwrap();

    let threads = query::threads(lib.conn()).unwrap();
    assert_eq!(threads.len(), 2);
    assert!(threads.iter().find(|t| t.id == joy).unwrap().hidden);
    assert!(!threads.iter().find(|t| t.title == "Amy").unwrap().hidden);
    assert_eq!(
        texts(&query::messages(lib.conn(), joy, Window::Latest, 10).unwrap()),
        vec!["still here"]
    );
    assert_eq!(query::hidden_thread_ids(lib.conn()).unwrap(), vec![joy]);

    query::set_hidden(lib.conn(), joy, false).unwrap();
    assert!(!query::threads(lib.conn()).unwrap().iter().any(|t| t.hidden));
    assert!(query::hidden_thread_ids(lib.conn()).unwrap().is_empty());
}

#[test]
fn delete_drops_exclusive_media_and_keeps_a_shared_file() {
    let tmp = tempfile::tempdir().unwrap();
    let joy = vec![
        m("Joy", "Jul 01, 2025 9:00 am", "only mine").photo("joy-only.jpg"),
        m("Joy", "Jul 01, 2025 9:01 am", "ours").photo("shared.jpg"),
    ];
    let amy = vec![
        m("Amy", "Jul 02, 2025 9:00 am", "only amy").photo("amy-only.jpg"),
        m("Amy", "Jul 02, 2025 9:01 am", "ours too").photo("shared.jpg"),
    ];
    write_chat(
        tmp.path(),
        "2025-08-01",
        "joy_111",
        "Joy",
        &joy,
        10,
        &["joy-only.jpg", "shared.jpg"],
    );
    let export = write_chat(
        tmp.path(),
        "2025-08-01",
        "amy_222",
        "Amy",
        &amy,
        10,
        &["amy-only.jpg", "shared.jpg"],
    );
    let mut lib = library(tmp.path());
    run(&mut lib, &[export]);
    let index = SearchIndex::open(&lib.root().join("search_index"), lib.conn()).unwrap();
    index.sync(lib.conn(), None, &mut |_| {}).unwrap();

    let joy_id = query::threads(lib.conn())
        .unwrap()
        .into_iter()
        .find(|t| t.title == "Joy")
        .unwrap()
        .id;
    lib.conn()
        .execute(
            "INSERT OR IGNORE INTO index_queue (message_id) SELECT id FROM messages WHERE thread_id = ?1",
            [joy_id],
        )
        .unwrap();

    lib.delete_thread(&index, joy_id).unwrap();

    let titles: Vec<String> = query::threads(lib.conn())
        .unwrap()
        .into_iter()
        .map(|t| t.title)
        .collect();
    assert_eq!(titles, vec!["Amy"]);
    assert!(query::messages(lib.conn(), joy_id, Window::Latest, 10)
        .unwrap()
        .is_empty());
    assert_eq!(
        scalar_count(lib.conn(), "SELECT COUNT(*) FROM index_queue"),
        0
    );
    let bodies = media_bodies(&lib);
    assert_eq!(bodies.len(), 2);
    assert!(!bodies.iter().any(|b| b == b"photo bytes joy-only.jpg"));
    assert!(bodies.iter().any(|b| b == b"photo bytes shared.jpg"));
    assert!(bodies.iter().any(|b| b == b"photo bytes amy-only.jpg"));
}

#[test]
fn importing_again_restores_a_deleted_chat() {
    let tmp = tempfile::tempdir().unwrap();
    let export = write_chat(
        tmp.path(),
        "2025-08-01",
        "joy_111",
        "Joy",
        &[m("Joy", "Jul 01, 2025 9:00 am", "hello again")],
        10,
        &[],
    );
    let mut lib = library(tmp.path());
    run(&mut lib, std::slice::from_ref(&export));
    let id = query::threads(lib.conn()).unwrap()[0].id;
    let index = SearchIndex::open(&lib.root().join("search_index"), lib.conn()).unwrap();
    lib.delete_thread(&index, id).unwrap();
    assert!(query::threads(lib.conn()).unwrap().is_empty());

    run(&mut lib, &[export]);
    let threads = query::threads(lib.conn()).unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].title, "Joy");
    assert!(!threads[0].hidden);
    assert_eq!(
        texts(&query::messages(lib.conn(), threads[0].id, Window::Latest, 10).unwrap()),
        vec!["hello again"]
    );
}

fn scalar_count(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn media_bodies(lib: &chat_core::Library) -> Vec<Vec<u8>> {
    let mut stmt = lib
        .conn()
        .prepare("SELECT store_path FROM media WHERE store_path IS NOT NULL ORDER BY store_path")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|path| {
            let path = path.unwrap();
            let full = lib.root().join(&path);
            assert!(full.is_file(), "missing stored file {path}");
            std::fs::read(full).unwrap()
        })
        .collect()
}
