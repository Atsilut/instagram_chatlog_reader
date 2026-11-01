// SPDX-License-Identifier: AGPL-3.0-or-later

//! Full-text search over every message.
//!
//! The Tantivy index lives on disk next to the database and is kept in sync from
//! `index_queue`, which the import fills. Messages are indexed as normalized words
//! (see [`normalize`]). The normal search matches each query word as itself, as one of
//! its forms (loved, have loved, used to love; see [`inflect`]) or as a common typo of
//! any of those (a neighbouring key or two swapped letters; see [`typos`]). Korean
//! words also match with a particle attached ("인절미" finds "인절미를").
//!
//! Related words, heavier typos and missing spaces are left to [`deep::deep_search`],
//! which checks every message.

pub mod deep;
pub mod inflect;
pub mod normalize;
pub mod typos;

use std::ops::Bound;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tantivy::collector::{Count, TopDocs};
use tantivy::query::{
    BooleanQuery, BoostQuery, FuzzyTermQuery, Occur, Query, RangeQuery, TermQuery,
};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, FAST, INDEXED, STORED,
};
use tantivy::tokenizer::{RemoveLongFilter, TextAnalyzer, WhitespaceTokenizer};
use tantivy::{
    DocAddress, Index, IndexReader, IndexWriter, Order, ReloadPolicy, TantivyDocument, Term,
};

use crate::error::IoContext;
use crate::{Error, Result};
use inflect::query_terms;
use normalize::{normalize, stretched_forms};
use typos::{common_typos, is_hangul_jamo};

const WORDS_TOKENIZER: &str = "icv_words";
const WRITER_MEMORY: usize = 64 * 1024 * 1024;
const SYNC_BATCH: i64 = 20_000;

/// Restricts a search to one chat, one sender and/or a time range (wall-clock minutes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filters {
    pub thread_id: Option<i64>,
    pub participant_id: Option<i64>,
    pub from_minute: Option<i64>,
    pub to_minute: Option<i64>,
}

impl Filters {
    /// SQL conditions on `messages m` with anonymous parameters.
    pub(crate) fn sql(&self) -> (String, Vec<i64>) {
        let mut cond = String::new();
        let mut args = Vec::new();
        for (col, op, val) in [
            ("m.thread_id", "=", self.thread_id),
            ("m.participant_id", "=", self.participant_id),
            ("m.ts_minute", ">=", self.from_minute),
            ("m.ts_minute", "<=", self.to_minute),
        ] {
            if let Some(v) = val {
                cond.push_str(&format!(" AND {col} {op} ?"));
                args.push(v);
            }
        }
        (cond, args)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortOrder {
    /// Closest matches first (deep search); the normal search treats it as oldest first.
    BestMatch,
    #[default]
    Oldest,
    Newest,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub text: String,
    #[serde(default)]
    pub filters: Filters,
    #[serde(default)]
    pub order: SortOrder,
    #[serde(default)]
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    /// All matches, not just this page.
    pub total: usize,
    /// Message ids of this page in the requested order.
    pub message_ids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub done: usize,
    pub total: usize,
}

#[derive(Clone, Copy)]
struct Fields {
    message_id: Field,
    thread_id: Field,
    participant_id: Field,
    ts_minute: Field,
    words: Field,
}

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    f: Fields,
}

fn schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let words = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(WORDS_TOKENIZER)
            .set_index_option(IndexRecordOption::WithFreqs),
    );
    let f = Fields {
        message_id: b.add_i64_field("message_id", INDEXED | FAST | STORED),
        thread_id: b.add_i64_field("thread_id", INDEXED | FAST),
        participant_id: b.add_i64_field("participant_id", INDEXED | FAST),
        ts_minute: b.add_i64_field("ts_minute", INDEXED | FAST),
        words: b.add_text_field("words", words),
    };
    (b.build(), f)
}

impl SearchIndex {
    /// Opens the index in `dir`, creating it if needed. A new index (or one built by an
    /// older version, which is recreated) is filled by queueing every message for
    /// [`Self::sync`].
    pub fn open(dir: &Path, conn: &Connection) -> Result<Self> {
        std::fs::create_dir_all(dir).at(dir)?;
        let (schema, f) = schema();
        let index = match Index::open_in_dir(dir) {
            Ok(index) if index.schema() == schema => index,
            _ => {
                clear_dir(dir)?;
                let index = Index::create_in_dir(dir, schema)?;
                conn.execute(
                    "INSERT OR IGNORE INTO index_queue (message_id) SELECT id FROM messages",
                    [],
                )?;
                index
            }
        };
        index.tokenizers().register(
            WORDS_TOKENIZER,
            TextAnalyzer::builder(WhitespaceTokenizer::default())
                .filter(RemoveLongFilter::limit(64))
                .build(),
        );
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        Ok(Self { index, reader, f })
    }

    /// Messages waiting to be indexed.
    pub fn pending(conn: &Connection) -> Result<usize> {
        Ok(conn.query_row("SELECT COUNT(*) FROM index_queue", [], |r| {
            r.get::<_, i64>(0)
        })? as usize)
    }

    /// Indexes everything in `index_queue`, committing in batches so an interrupted
    /// run resumes where it stopped. Returns the number of messages indexed.
    pub fn sync(
        &self,
        conn: &Connection,
        cancel: Option<&AtomicBool>,
        on_progress: &mut dyn FnMut(IndexProgress),
    ) -> Result<usize> {
        let total = Self::pending(conn)?;
        if total == 0 {
            return Ok(0);
        }
        let mut writer: IndexWriter = self.index.writer(WRITER_MEMORY)?;
        let mut stmt = conn.prepare(
            "SELECT q.message_id, m.thread_id, m.participant_id, m.ts_minute,
                    COALESCE(m.text, '') || ' ' || COALESCE(
                        (SELECT group_concat(COALESCE(p.caption, '') || ' ' || COALESCE(p.author, ''), ' ')
                         FROM message_parts p WHERE p.message_id = m.id AND p.type IN ('share', 'link')), '')
             FROM index_queue q JOIN messages m ON m.id = q.message_id
             WHERE q.message_id > ?1 ORDER BY q.message_id LIMIT ?2",
        )?;
        let mut done = 0;
        let mut last = 0i64;
        loop {
            if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                break;
            }
            let rows: Vec<(i64, i64, i64, i64, String)> = stmt
                .query_map(params![last, SYNC_BATCH], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<std::result::Result<_, _>>()?;
            let Some(&(batch_last, ..)) = rows.last() else {
                break;
            };
            for (id, thread, participant, ts, text) in &rows {
                writer.delete_term(Term::from_field_i64(self.f.message_id, *id));
                let mut doc = TantivyDocument::default();
                doc.add_i64(self.f.message_id, *id);
                doc.add_i64(self.f.thread_id, *thread);
                doc.add_i64(self.f.participant_id, *participant);
                doc.add_i64(self.f.ts_minute, *ts);
                let mut words = normalize(text);
                for extra in stretched_forms(text) {
                    words.push(' ');
                    words.push_str(&extra);
                }
                doc.add_text(self.f.words, words);
                writer.add_document(doc)?;
            }
            writer.commit()?;
            self.reader.reload()?;
            conn.execute(
                "DELETE FROM index_queue WHERE message_id > ?1 AND message_id <= ?2",
                params![last, batch_last],
            )?;
            last = batch_last;
            done += rows.len();
            on_progress(IndexProgress { done, total });
        }
        writer.wait_merging_threads()?;
        self.reader.reload()?;
        Ok(done)
    }

    pub fn search(&self, q: &SearchQuery) -> Result<SearchResults> {
        let Some(query) = self.build_query(&q.text, &q.filters) else {
            return Ok(SearchResults::default());
        };
        let searcher = self.reader.searcher();
        let order = if q.order == SortOrder::Newest {
            Order::Desc
        } else {
            Order::Asc
        };
        let top = TopDocs::with_limit(q.limit.max(1)).and_offset(q.offset);
        let (docs, total) = searcher.search(
            &query,
            &(top.order_by_fast_field::<i64>("ts_minute", order), Count),
        )?;
        let addresses: Vec<DocAddress> = docs.into_iter().map(|(_, a)| a).collect();

        let mut message_ids = Vec::with_capacity(addresses.len());
        for addr in addresses {
            let doc: TantivyDocument = searcher.doc(addr)?;
            if let Some(id) = doc.get_first(self.f.message_id).and_then(|v| v.as_i64()) {
                message_ids.push(id);
            }
        }
        Ok(SearchResults { total, message_ids })
    }

    fn build_query(&self, text: &str, filters: &Filters) -> Option<Box<dyn Query>> {
        let norm = normalize(text);
        let words: Vec<&str> = norm.split_whitespace().collect();
        if words.is_empty() {
            return None;
        }

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = words
            .iter()
            .map(|w| (Occur::Must, self.word_query(w)))
            .collect();
        for (field, value) in [
            (self.f.thread_id, filters.thread_id),
            (self.f.participant_id, filters.participant_id),
        ] {
            if let Some(v) = value {
                clauses.push((
                    Occur::Must,
                    Box::new(TermQuery::new(
                        Term::from_field_i64(field, v),
                        IndexRecordOption::Basic,
                    )),
                ));
            }
        }
        if filters.from_minute.is_some() || filters.to_minute.is_some() {
            let bound = |v: Option<i64>| match v {
                Some(v) => Bound::Included(Term::from_field_i64(self.f.ts_minute, v)),
                None => Bound::Unbounded,
            };
            clauses.push((
                Occur::Must,
                Box::new(RangeQuery::new(
                    bound(filters.from_minute),
                    bound(filters.to_minute),
                )),
            ));
        }
        Some(Box::new(BooleanQuery::new(clauses)))
    }

    /// The word, its forms and one typo of each. Korean words may carry a particle.
    fn word_query(&self, word: &str) -> Box<dyn Query> {
        let hangul = word.chars().any(is_hangul_jamo);
        let one = |w: &str| -> Box<dyn Query> {
            let term = Term::from_field_text(self.f.words, w);
            if hangul {
                Box::new(FuzzyTermQuery::new_prefix(term, 0, false))
            } else {
                Box::new(TermQuery::new(term, IndexRecordOption::WithFreqs))
            }
        };
        let mut terms = if hangul {
            let mut terms = common_typos(word);
            terms.push(word.to_string());
            terms
        } else {
            query_terms(word)
        };
        terms.sort();
        terms.dedup();
        let alternatives = terms
            .iter()
            .map(|t| {
                let query = one(t);
                let query: Box<dyn Query> = if t == word {
                    Box::new(BoostQuery::new(query, 2.0))
                } else {
                    query
                };
                (Occur::Should, query)
            })
            .collect();
        Box::new(BooleanQuery::new(alternatives))
    }
}

fn clear_dir(dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(dir).at(dir)? {
        let path = entry.at(dir)?.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path).at(&path)?;
        } else {
            std::fs::remove_file(&path).at(&path)?;
        }
    }
    Ok(())
}

impl From<tantivy::TantivyError> for Error {
    fn from(e: tantivy::TantivyError) -> Self {
        Error::Search(e.to_string())
    }
}
