// SPDX-License-Identifier: AGPL-3.0-or-later

//! Import pipeline: scan inputs, parse pages, store media, and merge messages into the
//! library without duplicates.
//!
//! - Backups are processed oldest first, so the newest backup's names and titles win.
//! - Each page is merged day by day in one transaction. A day whose fingerprints and
//!   reactions already match the library is skipped (the speed path); otherwise each
//!   minute is merged with [`merge_minute`] (the correctness path).
//! - Nothing is ever deleted. A message missing from a newer complete backup keeps
//!   an older `last_seen_at` than its thread's `latest_full_export_at`.

mod fingerprint;
mod merge;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::media::{MediaId, MediaStore, Staged};
use crate::parse::{parse_page, ParsedPage, Part};
use crate::scan::{scan, Page, Scan, ScanReport, ThreadSource};
use crate::{Error, Library, Result};
use fingerprint::{
    content_key, fingerprint, kind_str, looks_direct, original_name, part_type, sender_key, Hash,
    SELF_KEY,
};
pub use merge::{merge_minute, MinuteMerge, Slot};

const MINUTES_PER_DAY: i64 = 1440;
/// `sort_key = ts_minute * SEQ_PER_MINUTE + position within the minute`.
pub const SEQ_PER_MINUTE: i64 = 10_000;
const MAX_TZ_SHIFT_MIN: i64 = 14 * 60;
const TZ_SAMPLES: usize = 400;
const TZ_MIN_EVIDENCE: usize = 5;

#[derive(Default)]
pub struct ImportOptions<'a> {
    /// Checked between page batches; the current page is always finished first.
    pub cancel: Option<&'a AtomicBool>,
    pub progress: Option<&'a (dyn Fn(&Progress) + Sync)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub pages_done: usize,
    pub pages_total: usize,
    pub chat: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportReport {
    pub scan: ScanReport,
    pub exports: usize,
    pub threads: usize,
    pub pages_parsed: usize,
    pub pages_skipped_requests: usize,
    pub parse_issues: usize,
    pub days_skipped: usize,
    pub days_merged: usize,
    pub messages_new: usize,
    /// Messages already in the library (counted once per backup that contained them).
    pub messages_existing: usize,
    pub media_files_stored: usize,
    /// Referenced files not supplied by any backup so far (library total).
    pub media_missing: usize,
    pub tz_shifts: Vec<TzShift>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TzShift {
    pub chat: String,
    pub export: String,
    /// Added to this backup's times to line up with the library.
    pub minutes: i64,
    pub evidence: usize,
}

pub fn import(lib: &mut Library, inputs: &[PathBuf], opts: &ImportOptions) -> Result<ImportReport> {
    let scan = scan(inputs, &lib.root().join("tmp"))?;
    let media = lib.media();
    let conn = lib.conn_mut();
    let present_before = count(conn, "SELECT COUNT(*) FROM media WHERE status = 'present'")?;

    let mut run = Run {
        scan: &scan,
        media: &media,
        opts,
        report: ImportReport {
            scan: scan.report.clone(),
            ..Default::default()
        },
        pages_done: 0,
        threads_seen: HashSet::new(),
    };

    let mut order: Vec<(usize, Option<i64>)> = (0..scan.exports.len())
        .map(|i| (i, export_time(&scan, i)))
        .collect();
    order.sort_by_key(|&(i, t)| (t.is_none(), t, i));
    for (export, export_at) in order {
        let export_id = insert_export(conn, &scan, export, export_at)?;
        for src in scan.threads.iter().filter(|t| t.export == export) {
            run.thread_source(conn, src, export_id, export_at)?;
        }
        run.report.exports += 1;
    }

    mark_group_owners(conn)?;
    let mut report = run.report;
    report.threads = run.threads_seen.len();
    report.media_files_stored =
        (count(conn, "SELECT COUNT(*) FROM media WHERE status = 'present'")? - present_before)
            as usize;
    report.media_missing =
        count(conn, "SELECT COUNT(*) FROM media WHERE status = 'missing'")? as usize;
    Ok(report)
}

/// Export time in Unix seconds: from folder/ZIP names, else the first page's
/// "Generated" time.
fn export_time(scan: &Scan, export: usize) -> Option<i64> {
    if let Some(t) = scan.exports[export].exported_at {
        return Some(t.and_utc().timestamp());
    }
    let src = scan.threads.iter().find(|t| t.export == export)?;
    let bytes = scan.vfs.read(src.pages.first()?.file).ok()?;
    parse_page(&String::from_utf8_lossy(&bytes))
        .generated_at
        .map(|t| t.timestamp())
}

fn insert_export(
    conn: &Connection,
    scan: &Scan,
    export: usize,
    export_at: Option<i64>,
) -> Result<i64> {
    let e = &scan.exports[export];
    conn.execute(
        "INSERT INTO exports (owner_username, exported_at, root_label, imported_at) VALUES (?1, ?2, ?3, ?4)",
        params![e.owner_username, export_at, e.key, now()],
    )?;
    Ok(conn.last_insert_rowid())
}

struct Run<'a> {
    scan: &'a Scan,
    media: &'a MediaStore,
    opts: &'a ImportOptions<'a>,
    report: ImportReport,
    pages_done: usize,
    threads_seen: HashSet<i64>,
}

/// One chat as seen through one thread source of one backup.
struct ThreadCtx {
    id: i64,
    direct: bool,
    /// This backup is at least as new as the stored title/names.
    names_newer: bool,
    /// Minutes added to this backup's times (timezone correction).
    shift: i64,
    participants: HashMap<String, i64>,
    pages_seen: HashSet<u32>,
    page_count: Option<u32>,
}

struct Incoming<'p> {
    msg: &'p crate::parse::ParsedMessage,
    sender_key: String,
    ts: i64,
    ck: Hash,
    fp: Hash,
    /// `(actor key, emoji, actor display name)`, sorted and deduplicated.
    reactions: Vec<(String, String, String)>,
}

struct StoredRow {
    id: i64,
    ts: i64,
    fp: Hash,
    occurrence: i64,
    sort_key: i64,
}

struct MediaPlan {
    known: HashMap<String, MediaId>,
    staged: Vec<(String, Staged)>,
    missing: Vec<String>,
}

impl Run<'_> {
    fn thread_source(
        &mut self,
        conn: &mut Connection,
        src: &ThreadSource,
        export_id: i64,
        export_at: Option<i64>,
    ) -> Result<()> {
        let mut threads: HashMap<String, ThreadCtx> = HashMap::new();
        // message_N with the highest N is the oldest.
        let pages: Vec<Page> = src.pages.iter().rev().copied().collect();
        let batch = rayon::current_num_threads().max(2);

        for chunk in pages.chunks(batch) {
            if self.opts.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Err(Error::Cancelled);
            }
            let raw = chunk
                .iter()
                .map(|p| Ok((*p, self.scan.vfs.read(p.file)?)))
                .collect::<Result<Vec<_>>>()?;
            let parsed: Vec<(Page, Hash, u64, ParsedPage)> = raw
                .into_par_iter()
                .map(|(p, bytes)| {
                    let hash = *blake3::hash(&bytes).as_bytes();
                    (
                        p,
                        hash,
                        bytes.len() as u64,
                        parse_page(&String::from_utf8_lossy(&bytes)),
                    )
                })
                .collect();
            for (page, hash, size, parsed) in &parsed {
                self.page(
                    conn,
                    src,
                    &mut threads,
                    *page,
                    hash,
                    *size,
                    parsed,
                    export_id,
                    export_at,
                )?;
            }
        }

        for ctx in threads.values() {
            finish_thread(conn, ctx, export_at)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn page(
        &mut self,
        conn: &mut Connection,
        src: &ThreadSource,
        threads: &mut HashMap<String, ThreadCtx>,
        page: Page,
        hash: &Hash,
        size: u64,
        parsed: &ParsedPage,
        export_id: i64,
        export_at: Option<i64>,
    ) -> Result<()> {
        self.report.parse_issues += parsed.issues.len();
        if parsed
            .thread
            .as_ref()
            .is_some_and(|t| t.mailbox == "message_requests")
        {
            self.report.pages_skipped_requests += 1;
            return self.progressed(&parsed.title);
        }

        let key = parsed
            .thread
            .as_ref()
            .map(|t| t.thread_id.clone())
            .or_else(|| src.thread_id.clone())
            .unwrap_or_else(|| format!("title:{}", parsed.title));
        if !threads.contains_key(&key) {
            let ctx = self.open_thread(conn, src, &key, parsed, page, export_id, export_at)?;
            threads.insert(key.clone(), ctx);
        }
        let ctx = threads.get_mut(&key).expect("inserted above");
        ctx.pages_seen.insert(page.number);
        if let Some(c) = parsed.page_count {
            ctx.page_count = Some(ctx.page_count.map_or(c, |p| p.max(c)));
        }
        self.threads_seen.insert(ctx.id);

        let incoming = prepare(parsed, ctx);
        let plan = self.plan_media(conn, ctx.id, parsed)?;

        let tx = conn.transaction()?;
        let media_ids = self.apply_media(&tx, ctx.id, plan)?;
        tx.execute(
            "INSERT INTO source_files (export_id, logical_path, size, blake3, imported_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![export_id, self.scan.vfs.file(page.file).path, size as i64, &hash[..], now()],
        )?;
        let source_file = tx.last_insert_rowid();
        let new_before = self.report.messages_new;
        // Even when every day is skipped, a newer backup refreshes display names.
        for inc in &incoming {
            participant(&tx, ctx, &inc.sender_key, &inc.msg.sender)?;
        }

        let mut days: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
        for (i, inc) in incoming.iter().enumerate() {
            days.entry(inc.ts.div_euclid(MINUTES_PER_DAY))
                .or_default()
                .push(i);
        }
        for (day, idxs) in days {
            self.merge_day(
                &tx,
                ctx,
                &incoming,
                &idxs,
                day,
                &media_ids,
                source_file,
                export_at,
            )?;
        }
        // Keep the chat list current page by page, so a cancelled or interrupted
        // import still shows correct counts.
        tx.prepare_cached(
            "UPDATE threads SET
                 message_count = message_count + ?1,
                 last_sort_key = (SELECT MAX(sort_key) FROM messages WHERE thread_id = ?2)
             WHERE id = ?2",
        )?
        .execute(params![
            (self.report.messages_new - new_before) as i64,
            ctx.id
        ])?;
        tx.commit()?;

        self.report.pages_parsed += 1;
        self.progressed(&parsed.title)
    }

    #[allow(clippy::too_many_arguments)]
    fn merge_day(
        &mut self,
        tx: &Connection,
        ctx: &mut ThreadCtx,
        incoming: &[Incoming],
        idxs: &[usize],
        day: i64,
        media_ids: &HashMap<String, MediaId>,
        source_file: i64,
        export_at: Option<i64>,
    ) -> Result<()> {
        let lo = day * MINUTES_PER_DAY * SEQ_PER_MINUTE;
        let hi = (day + 1) * MINUTES_PER_DAY * SEQ_PER_MINUTE - 1;

        let digest_in = digest(
            idxs.iter()
                .map(|&i| (&incoming[i].fp, reaction_keys(&incoming[i]))),
        );
        let stored: Option<Vec<u8>> = tx
            .prepare_cached("SELECT digest FROM day_digests WHERE thread_id = ?1 AND day = ?2")?
            .query_row(params![ctx.id, day], |r| r.get(0))
            .optional()?;
        if stored.as_deref() == Some(&digest_in[..]) {
            if let Some(t) = export_at {
                tx.prepare_cached(
                    "UPDATE messages SET last_seen_at = MAX(COALESCE(last_seen_at, 0), ?1)
                     WHERE thread_id = ?2 AND sort_key BETWEEN ?3 AND ?4",
                )?
                .execute(params![t, ctx.id, lo, hi])?;
            }
            self.report.days_skipped += 1;
            self.report.messages_existing += idxs.len();
            return Ok(());
        }

        let mut existing: BTreeMap<i64, Vec<StoredRow>> = BTreeMap::new();
        {
            let mut stmt = tx.prepare_cached(
                "SELECT id, ts_minute, fp, occurrence, sort_key FROM messages
                 WHERE thread_id = ?1 AND sort_key BETWEEN ?2 AND ?3 ORDER BY sort_key",
            )?;
            let rows = stmt.query_map(params![ctx.id, lo, hi], |r| {
                Ok(StoredRow {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    fp: blob32(r.get::<_, Vec<u8>>(2)?),
                    occurrence: r.get(3)?,
                    sort_key: r.get(4)?,
                })
            })?;
            for row in rows {
                let row = row?;
                existing.entry(row.ts).or_default().push(row);
            }
        }

        let mut by_minute: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
        for &i in idxs {
            by_minute.entry(incoming[i].ts).or_default().push(i);
        }
        for (minute, new_idxs) in by_minute {
            let ex: &[StoredRow] = existing.get(&minute).map(Vec::as_slice).unwrap_or(&[]);
            let ex_fps: Vec<Hash> = ex.iter().map(|r| r.fp).collect();
            let in_fps: Vec<Hash> = new_idxs.iter().map(|&i| incoming[i].fp).collect();
            let mm = merge_minute(&ex_fps, &in_fps);

            let mut next_occ: HashMap<Hash, i64> = HashMap::new();
            for r in ex {
                let e = next_occ.entry(r.fp).or_insert(0);
                *e = (*e).max(r.occurrence + 1);
            }
            for (seq, slot) in mm.order.iter().enumerate() {
                let sort_key = minute * SEQ_PER_MINUTE + seq as i64;
                match *slot {
                    Slot::Existing(i) => {
                        if ex[i].sort_key != sort_key {
                            tx.prepare_cached("UPDATE messages SET sort_key = ?1 WHERE id = ?2")?
                                .execute(params![sort_key, ex[i].id])?;
                        }
                    }
                    Slot::New(j) => {
                        let inc = &incoming[new_idxs[j]];
                        let occ = next_occ.entry(inc.fp).or_insert(0);
                        let occurrence = *occ;
                        *occ += 1;
                        self.insert_message(
                            tx,
                            ctx,
                            inc,
                            sort_key,
                            occurrence,
                            media_ids,
                            source_file,
                            export_at,
                        )?;
                    }
                }
            }
            for &(i, j) in &mm.matched {
                let inc = &incoming[new_idxs[j]];
                add_reactions(tx, ctx, ex[i].id, inc)?;
                if let Some(t) = export_at {
                    tx.prepare_cached("UPDATE messages SET last_seen_at = MAX(COALESCE(last_seen_at, 0), ?1) WHERE id = ?2")?
                        .execute(params![t, ex[i].id])?;
                }
                self.report.messages_existing += 1;
            }
        }

        let stored_digest = stored_day_digest(tx, ctx.id, lo, hi)?;
        tx.prepare_cached(
            "INSERT INTO day_digests (thread_id, day, digest) VALUES (?1, ?2, ?3)
             ON CONFLICT (thread_id, day) DO UPDATE SET digest = excluded.digest",
        )?
        .execute(params![ctx.id, day, &stored_digest[..]])?;
        self.report.days_merged += 1;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_message(
        &mut self,
        tx: &Connection,
        ctx: &mut ThreadCtx,
        inc: &Incoming,
        sort_key: i64,
        occurrence: i64,
        media_ids: &HashMap<String, MediaId>,
        source_file: i64,
        export_at: Option<i64>,
    ) -> Result<()> {
        let m = inc.msg;
        let participant = participant(tx, ctx, &inc.sender_key, &m.sender)?;
        tx.prepare_cached(
            "INSERT INTO messages (thread_id, participant_id, ts_minute, sort_key, fp, occurrence, kind, text,
                                   raw_html, last_seen_at, content_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?
        .execute(params![
            ctx.id,
            participant,
            inc.ts,
            sort_key,
            &inc.fp[..],
            occurrence,
            kind_str(m.kind),
            m.text,
            m.raw_html,
            export_at,
            &inc.ck[..]
        ])?;
        let id = tx.last_insert_rowid();

        for (idx, p) in m.parts.iter().enumerate() {
            let (url, caption, author, media_id): (
                Option<String>,
                Option<&str>,
                Option<&str>,
                Option<MediaId>,
            ) = match p {
                Part::Photo { path }
                | Part::Video { path }
                | Part::Audio { path }
                | Part::Gif { path }
                | Part::File { path } => {
                    let name = original_name(path);
                    let media = media_ids.get(&name).copied();
                    (Some(name), None, None, media)
                }
                Part::Share {
                    caption,
                    author,
                    url,
                } => (url.clone(), caption.as_deref(), author.as_deref(), None),
                Part::Link { url, title } => (Some(url.clone()), title.as_deref(), None, None),
                Part::RemoteGif { url } => (Some(url.clone()), None, None, None),
            };
            tx.prepare_cached(
                "INSERT INTO message_parts (message_id, idx, type, url, caption, author, media_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?
            .execute(params![
                id,
                idx as i64,
                part_type(p),
                url,
                caption,
                author,
                media_id
            ])?;
        }
        add_reactions(tx, ctx, id, inc)?;
        tx.prepare_cached(
            "INSERT INTO message_sources (message_id, source_file_id) VALUES (?1, ?2)",
        )?
        .execute(params![id, source_file])?;
        tx.prepare_cached("INSERT OR IGNORE INTO index_queue (message_id) VALUES (?1)")?
            .execute([id])?;
        self.report.messages_new += 1;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn open_thread(
        &mut self,
        conn: &Connection,
        src: &ThreadSource,
        key: &str,
        parsed: &ParsedPage,
        page: Page,
        export_id: i64,
        export_at: Option<i64>,
    ) -> Result<ThreadCtx> {
        let slug = parsed
            .thread
            .as_ref()
            .map(|t| t.folder.clone())
            .or_else(|| src.folder.clone())
            .unwrap_or_else(|| key.to_string());
        let guess = looks_direct(
            &parsed.title,
            parsed.messages.iter().map(|m| m.sender.as_str()),
        );
        let kind_of = |direct: bool| if direct { "direct" } else { "group" };

        let row: Option<(i64, Option<String>, Option<i64>)> = conn
            .query_row(
                "SELECT id, kind, title_at FROM threads WHERE ig_thread_id = ?1",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let (id, direct, names_newer) = match row {
            None => {
                conn.execute(
                    "INSERT INTO threads (ig_thread_id, slug, title, kind, title_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![key, slug, parsed.title, kind_of(guess), export_at],
                )?;
                (conn.last_insert_rowid(), guess, true)
            }
            Some((id, kind, title_at)) => {
                let direct = match kind.as_deref() {
                    Some("direct") => true,
                    Some("group") => false,
                    _ => {
                        conn.execute(
                            "UPDATE threads SET kind = ?1 WHERE id = ?2",
                            params![kind_of(guess), id],
                        )?;
                        guess
                    }
                };
                let newer = match (export_at, title_at) {
                    (Some(e), Some(t)) => e >= t,
                    (Some(_), None) => true,
                    (None, t) => t.is_none(),
                };
                if newer {
                    conn.execute(
                        "UPDATE threads SET title = ?1, slug = ?2, title_at = ?3 WHERE id = ?4",
                        params![parsed.title, slug, export_at, id],
                    )?;
                }
                (id, direct, newer)
            }
        };

        let mut ctx = ThreadCtx {
            id,
            direct,
            names_newer,
            shift: 0,
            participants: HashMap::new(),
            pages_seen: HashSet::new(),
            page_count: None,
        };
        let has_messages: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM messages WHERE thread_id = ?1)",
            [id],
            |r| r.get(0),
        )?;
        if has_messages {
            ctx.shift = self.detect_shift(conn, src, &ctx, parsed, page, export_id)?;
        }
        Ok(ctx)
    }

    /// Compares this backup's messages with the library by content alone. If they
    /// consistently match at a whole-hour offset instead of at the same minute, the
    /// backup was made in a different timezone and its times are shifted.
    fn detect_shift(
        &mut self,
        conn: &Connection,
        src: &ThreadSource,
        ctx: &ThreadCtx,
        parsed: &ParsedPage,
        page: Page,
        export_id: i64,
    ) -> Result<i64> {
        let newest = match src.pages.first() {
            Some(p) if p.number != page.number => {
                let bytes = self.scan.vfs.read(p.file)?;
                Some(parse_page(&String::from_utf8_lossy(&bytes)))
            }
            _ => None,
        };
        let pages = std::iter::once(parsed).chain(newest.as_ref());

        let mut stmt = conn.prepare_cached(
            "SELECT ts_minute FROM messages WHERE thread_id = ?1 AND content_key = ?2 LIMIT 2",
        )?;
        let mut hist: HashMap<i64, usize> = HashMap::new();
        let mut matches = 0usize;
        let mut sampled = 0usize;
        'pages: for p in pages {
            for m in &p.messages {
                if m.text.as_deref().is_none_or(|t| t.chars().count() < 6) {
                    continue;
                }
                if sampled == TZ_SAMPLES {
                    break 'pages;
                }
                sampled += 1;
                let ck = content_key(&sender_key(&m.sender, &p.title, ctx.direct), m);
                let found: Vec<i64> = stmt
                    .query_map(params![ctx.id, &ck[..]], |r| r.get(0))?
                    .collect::<std::result::Result<_, _>>()?;
                if let [stored] = found[..] {
                    matches += 1;
                    let delta = stored - minutes(m.timestamp);
                    if delta % 60 == 0 && delta.abs() <= MAX_TZ_SHIFT_MIN {
                        *hist.entry(delta).or_default() += 1;
                    }
                }
            }
        }

        let same = hist.get(&0).copied().unwrap_or(0);
        let Some((&delta, &n)) = hist
            .iter()
            .filter(|(d, _)| **d != 0)
            .max_by_key(|(_, n)| **n)
        else {
            return Ok(0);
        };
        if n <= same {
            return Ok(0);
        }
        let label = self.scan.exports[src.export].key.clone();
        if n >= TZ_MIN_EVIDENCE && n * 10 >= matches * 6 {
            conn.execute(
                "UPDATE exports SET tz_offset_min = ?1 WHERE id = ?2",
                params![delta, export_id],
            )?;
            self.report.tz_shifts.push(TzShift {
                chat: parsed.title.clone(),
                export: label,
                minutes: delta,
                evidence: n,
            });
            Ok(delta)
        } else {
            self.report.warnings.push(format!(
                "\"{}\" in {label}: {n} of {matches} sampled messages match the library {delta} minutes apart; \
                 not enough evidence to correct the timezone",
                parsed.title
            ));
            Ok(0)
        }
    }

    /// Finds which media a page needs and stages files not yet in the library, in
    /// parallel. No database writes.
    fn plan_media(&self, conn: &Connection, thread: i64, parsed: &ParsedPage) -> Result<MediaPlan> {
        let mut plan = MediaPlan {
            known: HashMap::new(),
            staged: Vec::new(),
            missing: Vec::new(),
        };
        let mut seen = HashSet::new();
        let mut to_stage = Vec::new();
        let mut stmt = conn.prepare_cached(
            "SELECT m.id FROM media_refs r JOIN media m ON m.id = r.media_id
             WHERE r.thread_id = ?1 AND r.original_name = ?2 AND m.status = 'present'",
        )?;
        for m in &parsed.messages {
            for p in &m.parts {
                let (Part::Photo { path }
                | Part::Video { path }
                | Part::Audio { path }
                | Part::Gif { path }
                | Part::File { path }) = p
                else {
                    continue;
                };
                let name = original_name(path);
                if !seen.insert(name.clone()) {
                    continue;
                }
                if let Some(id) = stmt
                    .query_row(params![thread, name], |r| r.get(0))
                    .optional()?
                {
                    plan.known.insert(name, id);
                } else if let Some(&file) = self.scan.find_media(path).first() {
                    to_stage.push((name, file));
                } else {
                    plan.missing.push(name);
                }
            }
        }
        plan.staged = to_stage
            .into_par_iter()
            .map(|(name, file)| {
                let staged = self.scan.vfs.with_reader(file, |r| {
                    self.media.stage(r, &name).map_err(std::io::Error::other)
                })?;
                Ok((name, staged))
            })
            .collect::<Result<_>>()?;
        Ok(plan)
    }

    fn apply_media(
        &self,
        tx: &Connection,
        thread: i64,
        plan: MediaPlan,
    ) -> Result<HashMap<String, MediaId>> {
        let mut ids = plan.known;
        for (name, staged) in plan.staged {
            let id = self.media.commit(tx, staged)?;
            self.media.link(tx, thread, &name, id)?;
            ids.insert(name, id);
        }
        for name in plan.missing {
            let id = self.media.placeholder(tx, thread, &name)?;
            ids.insert(name, id);
        }
        Ok(ids)
    }

    fn progressed(&mut self, chat: &str) -> Result<()> {
        self.pages_done += 1;
        if let Some(f) = self.opts.progress {
            f(&Progress {
                pages_done: self.pages_done,
                pages_total: self.scan.report.pages,
                chat: chat.to_string(),
            });
        }
        Ok(())
    }
}

/// Messages of a page in chronological order with their keys.
fn prepare<'p>(parsed: &'p ParsedPage, ctx: &ThreadCtx) -> Vec<Incoming<'p>> {
    parsed
        .messages
        .iter()
        .rev()
        .map(|m| {
            let sk = sender_key(&m.sender, &parsed.title, ctx.direct);
            let ts = minutes(m.timestamp) + ctx.shift;
            let ck = content_key(&sk, m);
            let mut reactions: Vec<(String, String, String)> = m
                .reactions
                .iter()
                .map(|r| {
                    (
                        sender_key(&r.actor, &parsed.title, ctx.direct),
                        r.emoji.clone(),
                        r.actor.clone(),
                    )
                })
                .collect();
            reactions.sort();
            reactions.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
            Incoming {
                msg: m,
                sender_key: sk,
                ts,
                ck,
                fp: fingerprint(ts, &ck),
                reactions,
            }
        })
        .collect()
}

fn minutes(t: chrono::NaiveDateTime) -> i64 {
    t.and_utc().timestamp().div_euclid(60)
}

fn reaction_keys(inc: &Incoming) -> Vec<(String, String)> {
    inc.reactions
        .iter()
        .map(|(k, e, _)| (k.clone(), e.clone()))
        .collect()
}

fn add_reactions(tx: &Connection, ctx: &mut ThreadCtx, message: i64, inc: &Incoming) -> Result<()> {
    for (key, emoji, name) in &inc.reactions {
        let who = participant(tx, ctx, key, name)?;
        tx.prepare_cached("INSERT OR IGNORE INTO reactions (message_id, participant_id, emoji) VALUES (?1, ?2, ?3)")?
            .execute(params![message, who, emoji])?;
    }
    Ok(())
}

/// Participant id for a sender key, creating it or refreshing its display name when
/// this backup is the newest seen for the thread.
fn participant(tx: &Connection, ctx: &mut ThreadCtx, key: &str, display_name: &str) -> Result<i64> {
    if let Some(&id) = ctx.participants.get(key) {
        return Ok(id);
    }
    let existing: Option<i64> = tx
        .prepare_cached("SELECT id FROM participants WHERE thread_id = ?1 AND sender_key = ?2")?
        .query_row(params![ctx.id, key], |r| r.get(0))
        .optional()?;
    let id = match existing {
        Some(id) => {
            if ctx.names_newer {
                tx.execute(
                    "UPDATE participants SET display_name = ?1 WHERE id = ?2",
                    params![display_name, id],
                )?;
            }
            id
        }
        None => {
            tx.execute(
                "INSERT INTO participants (thread_id, sender_key, display_name, is_owner) VALUES (?1, ?2, ?3, ?4)",
                params![ctx.id, key, display_name, key == SELF_KEY],
            )?;
            tx.last_insert_rowid()
        }
    };
    ctx.participants.insert(key.to_string(), id);
    Ok(id)
}

fn digest<'a>(items: impl Iterator<Item = (&'a Hash, Vec<(String, String)>)>) -> Hash {
    let mut h = blake3::Hasher::new();
    for (fp, mut reactions) in items {
        reactions.sort();
        reactions.dedup();
        h.update(fp);
        h.update(&(reactions.len() as u64).to_le_bytes());
        for (key, emoji) in reactions {
            h.update(&(key.len() as u64).to_le_bytes());
            h.update(key.as_bytes());
            h.update(&(emoji.len() as u64).to_le_bytes());
            h.update(emoji.as_bytes());
        }
    }
    *h.finalize().as_bytes()
}

fn stored_day_digest(tx: &Connection, thread: i64, lo: i64, hi: i64) -> Result<Hash> {
    let mut reactions: HashMap<i64, Vec<(String, String)>> = HashMap::new();
    {
        let mut stmt = tx.prepare_cached(
            "SELECT r.message_id, p.sender_key, r.emoji FROM reactions r
             JOIN messages m ON m.id = r.message_id
             JOIN participants p ON p.id = r.participant_id
             WHERE m.thread_id = ?1 AND m.sort_key BETWEEN ?2 AND ?3",
        )?;
        let rows = stmt.query_map(params![thread, lo, hi], |r| {
            Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?))
        })?;
        for row in rows {
            let (id, key, emoji) = row?;
            reactions.entry(id).or_default().push((key, emoji));
        }
    }
    let mut stmt = tx.prepare_cached("SELECT id, fp FROM messages WHERE thread_id = ?1 AND sort_key BETWEEN ?2 AND ?3 ORDER BY sort_key")?;
    let rows: Vec<(i64, Hash)> = stmt
        .query_map(params![thread, lo, hi], |r| {
            Ok((r.get(0)?, blob32(r.get::<_, Vec<u8>>(1)?)))
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(digest(rows.iter().map(|(id, fp)| {
        (fp, reactions.remove(id).unwrap_or_default())
    })))
}

/// Group chats key senders by display name, so "you" is found by the display names
/// you have in one-to-one chats.
fn mark_group_owners(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE participants SET is_owner = 1
         WHERE is_owner = 0
           AND thread_id IN (SELECT id FROM threads WHERE kind = 'group')
           AND display_name IN (SELECT display_name FROM participants WHERE sender_key = ?1)",
        [SELF_KEY],
    )?;
    Ok(())
}

fn finish_thread(conn: &Connection, ctx: &ThreadCtx, export_at: Option<i64>) -> Result<()> {
    conn.execute(
        "UPDATE threads SET
             message_count = (SELECT COUNT(*) FROM messages WHERE thread_id = ?1),
             last_sort_key = (SELECT MAX(sort_key) FROM messages WHERE thread_id = ?1)
         WHERE id = ?1",
        [ctx.id],
    )?;
    let complete = ctx
        .page_count
        .is_some_and(|c| (1..=c).all(|n| ctx.pages_seen.contains(&n)));
    if let (true, Some(t)) = (complete, export_at) {
        conn.execute(
            "UPDATE threads SET latest_full_export_at = MAX(COALESCE(latest_full_export_at, 0), ?1) WHERE id = ?2",
            params![t, ctx.id],
        )?;
    }
    Ok(())
}

fn blob32(v: Vec<u8>) -> Hash {
    let mut out = [0u8; 32];
    let n = v.len().min(32);
    out[..n].copy_from_slice(&v[..n]);
    out
}

fn count(conn: &Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |r| r.get(0))?)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
