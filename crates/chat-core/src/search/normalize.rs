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
    let mut last: Option<char> = None;
    let mut run = 0;
    for c in text
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
    {
        let c = if c.is_alphanumeric() { c } else { ' ' };
        if c == ' ' && (last == Some(' ') || last.is_none()) {
            continue;
        }
        if Some(c) == last {
            run += 1;
            if run >= 2 && c != ' ' {
                continue;
            }
        } else {
            run = 0;
        }
        out.push(c);
        last = Some(c);
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Normalized text without spaces: substring search then ignores spacing differences
/// ("can not" finds "cannot").
pub fn compact(normalized: &str) -> String {
    normalized.chars().filter(|c| *c != ' ').collect()
}

/// Distinct character bigrams of compact text.
pub fn bigrams(compact: &str) -> Vec<String> {
    let chars: Vec<char> = compact.chars().collect();
    let mut out: Vec<String> = chars.windows(2).map(|w| w.iter().collect()).collect();
    out.sort();
    out.dedup();
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
    fn hangul_becomes_jamo_and_keeps_prefixes() {
        let word = normalize("인절미");
        let with_particle = normalize("인절미를");
        assert!(with_particle.starts_with(&word));
        assert!(word.chars().count() > 3, "decomposed into jamo");
    }

    #[test]
    fn compact_and_bigrams() {
        assert_eq!(compact("can not"), "cannot");
        assert_eq!(bigrams("abab"), vec!["ab", "ba"]);
        assert!(bigrams("a").is_empty());
    }
}
