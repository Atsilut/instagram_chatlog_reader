// SPDX-License-Identifier: AGPL-3.0-or-later

//! Text normalization shared by indexing, queries and deep search, so all three agree
//! on what counts as the same text.
//!
//! - NFKD with combining marks removed: "café" == "cafe". NFKD also splits Hangul
//!   syllables into jamo, so a one-key Korean typo costs one edit, not a whole
//!   syllable, and "인절미" is a prefix of "인절미를".
//! - Lowercase; punctuation and symbols become spaces.
//! - Runs of 3+ identical characters shrink to 2: "soooo" -> "soo".

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in fold(text) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(shrink_runs(&word, 3, 2));
    }
    out
}

/// Extra index forms for stretched words: runs of 3+ identical characters shrunk to
/// one, so "soooo" is also found as "so" (its normalized form is "soo"). Ordinary
/// double letters are kept: "good" gets no extra form.
pub fn stretched_forms(text: &str) -> Vec<String> {
    let mut out: Vec<String> = fold(text)
        .into_iter()
        .filter(|w| shrink_runs(w, 3, 2).len() != w.len())
        .map(|w| shrink_runs(&w, 3, 1).into_iter().collect())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Joins of two to four neighboring words, so a missing or extra space still matches:
/// "good morning" is also stored as "goodmorning".
pub fn space_joins(normalized: &str) -> Vec<String> {
    let words: Vec<&str> = normalized.split_whitespace().collect();
    let mut out = Vec::new();
    for window in 2..=4 {
        if words.len() < window {
            break;
        }
        for start in 0..=words.len() - window {
            let joined: String = words[start..start + window].concat();
            if joined.chars().count() <= 64 {
                out.push(joined);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Normalized text without spaces: substring search then ignores spacing differences
/// ("can not" finds "cannot").
pub fn compact(normalized: &str) -> String {
    normalized.chars().filter(|c| *c != ' ').collect()
}

/// Words after NFKD, mark removal and lowercasing, before run shrinking.
fn fold(text: &str) -> Vec<Vec<char>> {
    let mut words = Vec::new();
    let mut current = Vec::new();
    for c in text
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
    {
        if c.is_alphanumeric() {
            current.push(c);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Shrinks every run of at least `min_run` identical characters to `keep` characters.
fn shrink_runs(word: &[char], min_run: usize, keep: usize) -> Vec<char> {
    let mut out = Vec::with_capacity(word.len());
    let mut i = 0;
    while i < word.len() {
        let mut j = i;
        while j < word.len() && word[j] == word[i] {
            j += 1;
        }
        let run = j - i;
        let n = if run >= min_run { keep } else { run };
        out.extend(std::iter::repeat_n(word[i], n));
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_accents_punctuation_and_repeats() {
        assert_eq!(normalize("  Café!!  SOOOO   good...  "), "cafe soo good");
        assert_eq!(normalize("hahahaha"), "hahahaha");
        assert_eq!(normalize("It's"), "it s");
    }

    #[test]
    fn stretched_words_get_a_squeezed_form() {
        assert_eq!(stretched_forms("soooo good, yesss"), vec!["so", "yes"]);
        assert!(stretched_forms("good book").is_empty());
    }

    #[test]
    fn hangul_becomes_jamo_and_keeps_prefixes() {
        let word = normalize("인절미");
        let with_particle = normalize("인절미를");
        assert!(with_particle.starts_with(&word));
        assert!(word.chars().count() > 3, "decomposed into jamo");
    }

    #[test]
    fn neighboring_words_are_joined() {
        assert_eq!(
            space_joins("good morning sunshine"),
            vec!["goodmorning", "goodmorningsunshine", "morningsunshine"]
        );
        assert!(space_joins("hello").is_empty());
    }

    #[test]
    fn compact_removes_spaces() {
        assert_eq!(compact("can not"), "cannot");
    }
}
