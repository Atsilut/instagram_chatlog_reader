// SPDX-License-Identifier: AGPL-3.0-or-later

//! Exhaustive search: every message is checked for an approximate occurrence of the
//! query anywhere in its text, allowing a number of typos, missing letters or extra
//! letters and ignoring spaces. It finds related words ("love" in "lovely"), heavy
//! typos and split words that the strict normal search doesn't. Slow but complete.

use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;
use rusqlite::Connection;

use super::normalize::{compact, normalize};
use super::{Filters, SortOrder};
use crate::{Error, Result};

const CHUNK: i64 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepHit {
    pub message_id: i64,
    /// Edits between the query and the closest part of the message (0 = contains it).
    pub distance: usize,
    pub ts_minute: i64,
}

pub struct DeepProgress {
    pub scanned: usize,
    pub total: usize,
    pub matches: usize,
}

/// Edits allowed for a query of `len` characters (after normalization).
pub fn default_max_edits(len: usize) -> usize {
    match len {
        0..=3 => 0,
        4..=7 => 1,
        8..=15 => 2,
        _ => len / 6,
    }
}

/// Scans every message matching `filters`, calling `on_hits` with matches in id order.
/// Returns the number of matches.
pub fn deep_search(
    conn: &Connection,
    text: &str,
    filters: &Filters,
    max_edits: Option<usize>,
    cancel: Option<&AtomicBool>,
    on_hits: &mut dyn FnMut(Vec<DeepHit>),
    on_progress: &mut dyn FnMut(&DeepProgress),
) -> Result<usize> {
    let pattern: Vec<char> = compact(&normalize(text)).chars().collect();
    if pattern.is_empty() {
        return Ok(0);
    }
    let k = max_edits.unwrap_or_else(|| default_max_edits(pattern.len()));

    let (cond, args) = filters.sql();
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM messages m WHERE 1 = 1 {cond}"),
        rusqlite::params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let sql = format!(
        "SELECT m.id, m.ts_minute, COALESCE(m.text, '') || ' ' || COALESCE(
             (SELECT group_concat(COALESCE(p.caption, '') || ' ' || COALESCE(p.author, ''), ' ')
              FROM message_parts p WHERE p.message_id = m.id AND p.type IN ('share', 'link')), '')
         FROM messages m WHERE m.id > ?1 {cond} ORDER BY m.id LIMIT {CHUNK}"
    );
    let mut stmt = conn.prepare(&sql)?;

    let mut progress = DeepProgress {
        scanned: 0,
        total: total as usize,
        matches: 0,
    };
    let mut last_id = 0i64;
    loop {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(Error::Cancelled);
        }
        let mut bound: Vec<&dyn rusqlite::ToSql> = vec![&last_id];
        bound.extend(args.iter().map(|a| a as &dyn rusqlite::ToSql));
        let rows: Vec<(i64, i64, String)> = stmt
            .query_map(bound.as_slice(), |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?;
        let Some(&(last, ..)) = rows.last() else {
            break;
        };
        last_id = last;

        let hits: Vec<DeepHit> = rows
            .par_iter()
            .filter_map(|(id, ts, text)| {
                approx_distance(&compact(&normalize(text)), &pattern, k).map(|distance| DeepHit {
                    message_id: *id,
                    distance,
                    ts_minute: *ts,
                })
            })
            .collect();
        progress.scanned += rows.len();
        progress.matches += hits.len();
        if !hits.is_empty() {
            on_hits(hits);
        }
        on_progress(&progress);
    }
    Ok(progress.matches)
}

/// Orders deep search hits. Best match puts the closest matches first, then oldest.
pub fn sort_hits(hits: &mut [DeepHit], order: SortOrder) {
    match order {
        SortOrder::BestMatch => hits.sort_by_key(|h| (h.distance, h.ts_minute, h.message_id)),
        SortOrder::Oldest => hits.sort_by_key(|h| (h.ts_minute, h.message_id)),
        SortOrder::Newest => hits.sort_by_key(|h| std::cmp::Reverse((h.ts_minute, h.message_id))),
    }
}

/// True when some substring of `text` is within `k` edits of `pattern`.
pub fn approx_contains(text: &str, pattern: &[char], k: usize) -> bool {
    approx_distance(text, pattern, k).is_some()
}

/// Fewest edits between `pattern` and any substring of `text`, if at most `k`
/// (Sellers' approximate string matching).
pub fn approx_distance(text: &str, pattern: &[char], k: usize) -> Option<usize> {
    let m = pattern.len();
    if m == 0 {
        return Some(0);
    }
    // col[i] = edits to match pattern[..i] ending at the current text position.
    let mut col: Vec<usize> = (0..=m).collect();
    let mut best = col[m];
    for c in text.chars() {
        let mut diag = col[0];
        col[0] = 0;
        for i in 1..=m {
            let cost = usize::from(pattern[i - 1] != c);
            let next = (diag + cost).min(col[i] + 1).min(col[i - 1] + 1);
            diag = col[i];
            col[i] = next;
        }
        best = best.min(col[m]);
        if best == 0 {
            break;
        }
    }
    (best <= k).then_some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Vec<char> {
        compact(&normalize(s)).chars().collect()
    }

    fn t(s: &str) -> String {
        compact(&normalize(s))
    }

    #[test]
    fn finds_typos_inside_longer_text() {
        assert!(approx_contains(
            &t("I told you I startd HT for japanese"),
            &p("started"),
            1
        ));
        assert!(approx_contains(&t("thats so wierd lol"), &p("weird"), 2));
        assert!(!approx_contains(&t("completely different"), &p("weird"), 1));
    }

    #[test]
    fn reports_the_closest_distance() {
        assert_eq!(approx_distance(&t("lovely day"), &p("love"), 1), Some(0));
        assert_eq!(approx_distance(&t("i lvoe it"), &p("love"), 2), Some(2));
        assert_eq!(approx_distance(&t("nothing here"), &p("love"), 1), None);
    }

    #[test]
    fn ignores_spacing() {
        assert!(approx_contains(&t("we can not go"), &p("cannot"), 0));
    }

    #[test]
    fn korean_one_jamo_typo() {
        assert!(approx_contains(&t("인절미를 먹었어"), &p("인잘미"), 1));
    }
}
