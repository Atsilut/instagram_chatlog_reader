// SPDX-License-Identifier: AGPL-3.0-or-later

//! Exhaustive search: every message is checked for an approximate occurrence of the
//! query, allowing a number of typos, missing letters or extra letters. Slow but
//! complete; it finds what the index can't (heavily misspelled or split words).

use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;
use rusqlite::Connection;

use super::normalize::{compact, normalize};
use super::Filters;
use crate::{Error, Result};

const CHUNK: i64 = 20_000;

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

/// Scans every message matching `filters`, calling `on_hits` with the ids of matching
/// messages in id order. Returns the number of matches.
pub fn deep_search(
    conn: &Connection,
    text: &str,
    filters: &Filters,
    max_edits: Option<usize>,
    cancel: Option<&AtomicBool>,
    on_hits: &mut dyn FnMut(Vec<i64>),
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
        "SELECT m.id, COALESCE(m.text, '') || ' ' || COALESCE(
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
        let rows: Vec<(i64, String)> = stmt
            .query_map(bound.as_slice(), |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        let Some(&(last, _)) = rows.last() else { break };
        last_id = last;

        let hits: Vec<i64> = rows
            .par_iter()
            .filter(|(_, text)| approx_contains(&compact(&normalize(text)), &pattern, k))
            .map(|(id, _)| *id)
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

/// True when some substring of `text` is within `k` edits of `pattern`
/// (Sellers' approximate string matching).
pub fn approx_contains(text: &str, pattern: &[char], k: usize) -> bool {
    let m = pattern.len();
    if m == 0 {
        return true;
    }
    // col[i] = edits to match pattern[..i] ending at the current text position.
    let mut col: Vec<usize> = (0..=m).collect();
    if col[m] <= k {
        return true;
    }
    for c in text.chars() {
        let mut diag = col[0];
        col[0] = 0;
        for i in 1..=m {
            let cost = usize::from(pattern[i - 1] != c);
            let next = (diag + cost).min(col[i] + 1).min(col[i - 1] + 1);
            diag = col[i];
            col[i] = next;
        }
        if col[m] <= k {
            return true;
        }
    }
    false
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
    fn ignores_spacing() {
        assert!(approx_contains(&t("we can not go"), &p("cannot"), 0));
    }

    #[test]
    fn korean_one_jamo_typo() {
        assert!(approx_contains(&t("인절미를 먹었어"), &p("인잘미"), 1));
    }
}
