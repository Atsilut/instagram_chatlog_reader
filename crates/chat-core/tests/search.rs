// SPDX-License-Identifier: AGPL-3.0-or-later

mod common;

use std::path::Path;

use chat_core::query;
use chat_core::search::deep::deep_search;
use chat_core::search::{Filters, SearchIndex, SearchQuery, SortOrder};
use chat_core::Library;
use common::{library, m, run, write_chat, M, ME};

fn chat() -> Vec<M> {
    vec![
        m("Joy", "Jul 01, 2025 9:00 am", "good morning sunshine"),
        m(ME, "Jul 01, 2025 9:01 am", "I'm studying japanese today"),
        m("Joy", "Jul 02, 2025 8:00 pm", "인절미를 먹었어"),
        m(ME, "Jul 03, 2025 7:00 pm", "Café later? soooo tired"),
        m("Joy", "Jul 04, 2025 6:00 pm", "the weather is wierd"),
    ]
}

fn setup(tmp: &Path) -> (Library, SearchIndex) {
    let export = write_chat(tmp, "2025-08-01", "joy_111", "Joy", &chat(), 100, &[]);
    write_chat(
        tmp,
        "2025-08-01",
        "amy_222",
        "Amy",
        &[m("Amy", "Jul 05, 2025 9:00 am", "japanese class tomorrow")],
        100,
        &[],
    );
    let mut lib = library(tmp);
    run(&mut lib, &[export]);
    let index = SearchIndex::open(&lib.root().join("search_index"), lib.conn()).unwrap();
    index.sync(lib.conn(), None, &mut |_| {}).unwrap();
    (lib, index)
}

fn texts(lib: &Library, index: &SearchIndex, q: SearchQuery) -> Vec<String> {
    let r = index.search(&q).unwrap();
    query::messages_by_ids(lib.conn(), &r.message_ids)
        .unwrap()
        .into_iter()
        .map(|h| h.message.text.unwrap_or_default())
        .collect()
}

fn q(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        limit: 20,
        ..Default::default()
    }
}

#[test]
fn finds_exact_words_and_both_chats() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, index) = setup(tmp.path());
    let found = texts(&lib, &index, q("japanese"));
    assert_eq!(found.len(), 2);
    assert!(found.contains(&"I'm studying japanese today".to_string()));
}

#[test]
fn tolerates_typos_in_the_query_and_in_the_chat() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, index) = setup(tmp.path());
    assert!(texts(&lib, &index, q("japanse")).contains(&"I'm studying japanese today".to_string()));
    assert_eq!(
        texts(&lib, &index, q("weird weather")),
        vec!["the weather is wierd"]
    );
    assert_eq!(
        texts(&lib, &index, q("so tired cafe")),
        vec!["Café later? soooo tired"]
    );
}

#[test]
fn handles_missing_spaces_and_korean() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, index) = setup(tmp.path());
    assert_eq!(
        texts(&lib, &index, q("goodmorning")),
        vec!["good morning sunshine"]
    );
    assert_eq!(
        texts(&lib, &index, q("인절미")),
        vec!["인절미를 먹었어"],
        "word prefix covers particles"
    );
    assert_eq!(
        texts(&lib, &index, q("인잘미")),
        vec!["인절미를 먹었어"],
        "one-jamo typo"
    );
}

#[test]
fn filters_and_order() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, index) = setup(tmp.path());
    let threads = query::threads(lib.conn()).unwrap();
    let joy = threads.iter().find(|t| t.title == "Joy").unwrap();

    let mut only_joy = q("japanese");
    only_joy.filters.thread_id = Some(joy.id);
    assert_eq!(
        texts(&lib, &index, only_joy),
        vec!["I'm studying japanese today"]
    );

    let me = joy.participants.iter().find(|p| p.is_owner).unwrap();
    let mut mine = q("japanese");
    mine.filters.participant_id = Some(me.id);
    assert_eq!(texts(&lib, &index, mine).len(), 1);

    let mut newest = q("japanese");
    newest.order = SortOrder::Newest;
    assert_eq!(texts(&lib, &index, newest)[0], "japanese class tomorrow");

    let first_minute =
        query::messages(lib.conn(), joy.id, query::Window::Latest, 10).unwrap()[0].ts_minute;
    let mut early = q("japanese");
    early.filters = Filters {
        to_minute: Some(first_minute + 60),
        ..Default::default()
    };
    assert_eq!(
        texts(&lib, &index, early),
        vec!["I'm studying japanese today"]
    );
}

#[test]
fn reports_total_beyond_the_page() {
    let tmp = tempfile::tempdir().unwrap();
    let (_lib, index) = setup(tmp.path());
    let mut first = q("japanese");
    first.limit = 1;
    let r = index.search(&first).unwrap();
    assert_eq!((r.total, r.message_ids.len()), (2, 1));
    first.offset = 1;
    assert_eq!(index.search(&first).unwrap().message_ids.len(), 1);
}

#[test]
fn later_imports_are_indexed_incrementally() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut lib, index) = setup(tmp.path());
    assert_eq!(SearchIndex::pending(lib.conn()).unwrap(), 0);

    let mut more = chat();
    more.push(m(ME, "Jul 09, 2025 9:00 am", "see you at the airport"));
    run(
        &mut lib,
        &[write_chat(
            tmp.path(),
            "2025-08-02",
            "joy_111",
            "Joy",
            &more,
            100,
            &[],
        )],
    );
    assert_eq!(SearchIndex::pending(lib.conn()).unwrap(), 1);
    assert_eq!(index.sync(lib.conn(), None, &mut |_| {}).unwrap(), 1);
    assert_eq!(
        texts(&lib, &index, q("airport")),
        vec!["see you at the airport"]
    );

    drop(index);
    let reopened = SearchIndex::open(&lib.root().join("search_index"), lib.conn()).unwrap();
    assert_eq!(
        SearchIndex::pending(lib.conn()).unwrap(),
        0,
        "existing index is reused"
    );
    assert_eq!(texts(&lib, &reopened, q("airport")).len(), 1);
}

#[test]
fn deep_search_finds_heavily_misspelled_text() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, _index) = setup(tmp.path());
    let query_text = "studyng japnes";

    let mut ids = Vec::new();
    let mut last = None;
    let n = deep_search(
        lib.conn(),
        query_text,
        &Filters::default(),
        None,
        None,
        &mut |hits| ids.extend(hits),
        &mut |p| last = Some((p.scanned, p.total)),
    )
    .unwrap();
    assert_eq!(n, 1);
    let hits = query::messages_by_ids(lib.conn(), &ids).unwrap();
    assert_eq!(
        hits[0].message.text.as_deref(),
        Some("I'm studying japanese today")
    );
    assert_eq!(last, Some((6, 6)));
}

/// Indexing and query speed on synthetic data:
/// `ICV_BENCH_MESSAGES=1000000 cargo test -p chat-core --release --test search bench -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_large_library() {
    let n: i64 = std::env::var("ICV_BENCH_MESSAGES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000_000);
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = library(tmp.path());
    let words = [
        "hello",
        "morning",
        "dinner",
        "japanese",
        "study",
        "weird",
        "tomorrow",
        "game",
        "sleep",
        "coffee",
        "인절미",
        "고마워",
        "진짜",
        "ㅋㅋㅋ",
        "airport",
        "movie",
        "school",
        "pizza",
        "rain",
        "beach",
    ];
    let start = std::time::Instant::now();
    {
        let tx = lib.conn_mut().transaction().unwrap();
        tx.execute(
            "INSERT INTO threads (id, ig_thread_id, slug, title) VALUES (1, '1', 'x_1', 'X')",
            [],
        )
        .unwrap();
        tx.execute("INSERT INTO participants (id, thread_id, sender_key, display_name) VALUES (1, 1, '@other', 'X')", []).unwrap();
        let mut stmt = tx
            .prepare(
                "INSERT INTO messages (thread_id, participant_id, ts_minute, sort_key, fp, occurrence, kind, text)
                 VALUES (1, 1, ?1, ?1 * 10000, ?2, 0, 'text', ?3)",
            )
            .unwrap();
        let mut seed = 42u64;
        for i in 0..n {
            let mut text = String::new();
            for _ in 0..6 {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                text.push_str(words[(seed >> 33) as usize % words.len()]);
                text.push(' ');
            }
            stmt.execute(rusqlite::params![i, i.to_le_bytes().to_vec(), text])
                .unwrap();
        }
        drop(stmt);
        tx.execute(
            "INSERT INTO index_queue (message_id) SELECT id FROM messages",
            [],
        )
        .unwrap();
        tx.commit().unwrap();
    }
    eprintln!("inserted {n} messages in {:.1?}", start.elapsed());

    let index = SearchIndex::open(&lib.root().join("search_index"), lib.conn()).unwrap();
    let start = std::time::Instant::now();
    index.sync(lib.conn(), None, &mut |_| {}).unwrap();
    eprintln!("indexed in {:.1?}", start.elapsed());

    for text in [
        "japanese",
        "japanse",
        "airport dinner",
        "인절미",
        "goodmorning",
        "weird coffee tomorow",
    ] {
        let start = std::time::Instant::now();
        let r = index.search(&q(text)).unwrap();
        eprintln!(
            "{text:>22}: {:>8} hits, first page in {:.1?}",
            r.total,
            start.elapsed()
        );
    }
    let start = std::time::Instant::now();
    let found = deep_search(
        lib.conn(),
        "japnese studdy",
        &Filters::default(),
        None,
        None,
        &mut |_| {},
        &mut |_| {},
    )
    .unwrap();
    eprintln!("deep search: {found} matches in {:.1?}", start.elapsed());
}

#[test]
fn deep_search_can_be_cancelled() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, _index) = setup(tmp.path());
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let r = deep_search(
        lib.conn(),
        "japanese",
        &Filters::default(),
        None,
        Some(&cancel),
        &mut |_| {},
        &mut |_| {},
    );
    assert!(matches!(r, Err(chat_core::Error::Cancelled)));
}
