// SPDX-License-Identifier: AGPL-3.0-or-later

mod common;

use std::path::Path;

use chat_core::query;
use chat_core::search::deep::{deep_search, sort_hits, DeepHit};
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
        m(ME, "Jul 05, 2025 6:00 pm", "I love that polite cat"),
        m("Joy", "Jul 06, 2025 6:00 pm", "what a lovely day"),
        m(ME, "Jul 06, 2025 7:00 pm", "we have loved it ever since"),
        m("Joy", "Jul 06, 2025 8:00 pm", "she had loved it too"),
        m(ME, "Jul 06, 2025 9:00 pm", "I used to love it"),
        m("Joy", "Jul 08, 2025 9:00 am", "say goodmorning"),
    ]
}

fn setup(tmp: &Path) -> (Library, SearchIndex) {
    let export = write_chat(tmp, "2025-08-01", "joy_111", "Joy", &chat(), 100, &[]);
    write_chat(
        tmp,
        "2025-08-01",
        "amy_222",
        "Amy",
        &[m("Amy", "Jul 07, 2025 9:00 am", "japanese class tomorrow")],
        100,
        &[],
    );
    let mut lib = library(tmp);
    run(&mut lib, &[export]);
    let index = SearchIndex::open(&lib.root().join("search_index"), lib.conn()).unwrap();
    index.sync(lib.conn(), None, &mut |_| {}).unwrap();
    (lib, index)
}

fn texts_of(lib: &Library, ids: &[i64]) -> Vec<String> {
    query::messages_by_ids(lib.conn(), ids)
        .unwrap()
        .into_iter()
        .map(|h| h.message.text.unwrap_or_default())
        .collect()
}

fn texts(lib: &Library, index: &SearchIndex, q: SearchQuery) -> Vec<String> {
    texts_of(lib, &index.search(&q).unwrap().message_ids)
}

fn q(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        limit: 20,
        similar_words: true,
        ..Default::default()
    }
}

fn deep(lib: &Library, text: &str) -> Vec<DeepHit> {
    let mut hits = Vec::new();
    deep_search(
        lib.conn(),
        text,
        &Filters::default(),
        None,
        None,
        &mut |h| hits.extend(h),
        &mut |_| {},
    )
    .unwrap();
    hits
}

#[test]
fn finds_the_same_word_in_every_chat_oldest_first() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, index) = setup(tmp.path());
    assert_eq!(
        texts(&lib, &index, q("Japanese")),
        vec!["I'm studying japanese today", "japanese class tomorrow"]
    );
    assert_eq!(
        texts(&lib, &index, q("so tired cafe")),
        vec!["Café later? soooo tired"],
        "case, accents and stretched letters"
    );
}

#[test]
fn accepts_common_typos_only() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, index) = setup(tmp.path());
    assert!(
        texts(&lib, &index, q("kove")).contains(&"I love that polite cat".to_string()),
        "neighbouring key"
    );
    assert_eq!(
        texts(&lib, &index, q("politw")),
        vec!["I love that polite cat"]
    );
    assert_eq!(
        texts(&lib, &index, q("weird weather")),
        vec!["the weather is wierd"],
        "swapped letters"
    );

    let love = texts(&lib, &index, q("love"));
    assert!(love.contains(&"I love that polite cat".to_string()));
    assert!(!love.iter().any(|t| t.contains("lovely")), "not lovely");
    assert!(
        texts(&lib, &index, q("japanse")).is_empty(),
        "missing letter is for deep search"
    );
}

#[test]
fn word_forms_match_and_derivations_do_not() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, index) = setup(tmp.path());
    let found = texts(&lib, &index, q("love"));
    for text in [
        "I love that polite cat",
        "we have loved it ever since",
        "she had loved it too",
        "I used to love it",
    ] {
        assert!(found.contains(&text.to_string()), "missing {text}");
    }
    assert!(
        !found.iter().any(|t| t.contains("lovely")),
        "lovely stays a deep search result"
    );

    let lovef = texts(&lib, &index, q("lovef"));
    for text in [
        "I love that polite cat",
        "we have loved it ever since",
        "she had loved it too",
        "I used to love it",
    ] {
        assert!(
            lovef.contains(&text.to_string()),
            "lovef should find {text}"
        );
    }
    assert!(!lovef.iter().any(|t| t.contains("lovely")));
    assert_eq!(
        texts(&lib, &index, q("havr loved")),
        vec!["we have loved it ever since", "she had loved it too"]
    );
    assert!(texts(&lib, &index, q("kove")).contains(&"I used to love it".to_string()));

    let loved = texts(&lib, &index, q("loved"));
    assert!(loved.contains(&"we have loved it ever since".to_string()));
    assert!(loved.contains(&"she had loved it too".to_string()));
    assert!(
        !loved.contains(&"I love that polite cat".to_string()),
        "an inflected query does not match the base word"
    );
    assert!(!loved.contains(&"I used to love it".to_string()));

    let mut deep_loved = deep(&lib, "loved");
    sort_hits(&mut deep_loved, SortOrder::Oldest);
    let deep_texts = texts_of(
        &lib,
        &deep_loved.iter().map(|h| h.message_id).collect::<Vec<_>>(),
    );
    assert!(
        deep_texts.contains(&"I love that polite cat".to_string()),
        "deep search still reaches the base word"
    );
    assert!(deep_texts.contains(&"I used to love it".to_string()));
}

#[test]
fn missing_or_extra_spaces_always_match() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, index) = setup(tmp.path());
    let mut joined = q("goodmorning");
    joined.similar_words = false;
    assert!(texts(&lib, &index, joined).contains(&"good morning sunshine".to_string()));

    let mut split = q("good morning");
    split.similar_words = false;
    assert!(texts(&lib, &index, split).contains(&"say goodmorning".to_string()));
}

#[test]
fn similar_words_can_be_turned_off() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, index) = setup(tmp.path());
    let mut exact = q("love");
    exact.similar_words = false;
    let found = texts(&lib, &index, exact);
    assert!(found.contains(&"I love that polite cat".to_string()));
    assert!(found.contains(&"I used to love it".to_string()));
    assert!(
        !found.iter().any(|t| t.contains("loved")),
        "word forms need the option or deep search"
    );

    let mut typo = q("kove");
    typo.similar_words = false;
    assert!(
        texts(&lib, &index, typo).is_empty(),
        "simple typos need the option"
    );
}

#[test]
fn korean_particles_and_keyboard_typos() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, index) = setup(tmp.path());
    assert_eq!(texts(&lib, &index, q("인절미")), vec!["인절미를 먹었어"]);
    assert_eq!(
        texts(&lib, &index, q("인잘미")),
        vec!["인절미를 먹었어"],
        "ㅓ and ㅏ are neighbouring keys"
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
fn deep_search_finds_related_words_heavy_typos_and_split_words() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, _index) = setup(tmp.path());

    let mut love = deep(&lib, "love");
    sort_hits(&mut love, SortOrder::Oldest);
    let ids: Vec<i64> = love.iter().map(|h| h.message_id).collect();
    assert_eq!(
        texts_of(&lib, &ids),
        vec![
            "I love that polite cat",
            "what a lovely day",
            "we have loved it ever since",
            "she had loved it too",
            "I used to love it",
        ]
    );

    let japanese = deep(&lib, "studyng japnes");
    assert_eq!(
        texts_of(&lib, &[japanese[0].message_id]),
        vec!["I'm studying japanese today"]
    );
    let morning = deep(&lib, "goodmorning");
    assert_eq!(
        texts_of(&lib, &[morning[0].message_id]),
        vec!["good morning sunshine"]
    );
}

#[test]
fn deep_best_match_ranks_closest_first() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, _index) = setup(tmp.path());
    let mut hits = deep(&lib, "japanes");
    sort_hits(&mut hits, SortOrder::BestMatch);
    assert!(hits.windows(2).all(|w| w[0].distance <= w[1].distance));
    let mut newest = hits.clone();
    sort_hits(&mut newest, SortOrder::Newest);
    assert!(newest.windows(2).all(|w| w[0].ts_minute >= w[1].ts_minute));
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
        "japansee",
        "airport dinner",
        "인절미",
        "weird coffee tomorrow",
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
    let found = deep(&lib, "japnese studdy").len();
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
