// SPDX-License-Identifier: AGPL-3.0-or-later

//! Common typos of a word: one letter replaced by a neighbouring key (love -> kove,
//! polite -> politw) or two neighbouring letters swapped (weird -> wierd). Covers the
//! QWERTY layout and the Korean 2-set (두벌식) layout, which share key positions.
//!
//! Works on normalized text, where Hangul syllables are split into conjoining jamo.

const LATIN_ROWS: [&str; 3] = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];
const HANGUL_ROWS: [&str; 3] = [
    "ㅂㅈㄷㄱㅅㅛㅕㅑㅐㅔ",
    "ㅁㄴㅇㄹㅎㅗㅓㅏㅣ",
    "ㅋㅌㅊㅍㅠㅜㅡ",
];

/// Conjoining jamo blocks in Unicode order, written as compatibility jamo.
const CHOSEONG: (u32, &str) = (0x1100, "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ");
const JUNGSEONG: (u32, &str) = (0x1161, "ㅏㅐㅑㅒㅓㅔㅕㅖㅗㅘㅙㅚㅛㅜㅝㅞㅟㅠㅡㅢㅣ");
const JONGSEONG: (u32, &str) = (
    0x11A8,
    "ㄱㄲㄳㄴㄵㄶㄷㄹㄺㄻㄼㄽㄾㄿㅀㅁㅂㅄㅅㅆㅇㅈㅊㅋㅌㅍㅎ",
);

/// Words shorter than this get no typo variants; short words collide too easily.
const MIN_LEN: usize = 4;

/// Typo variants of a normalized word, excluding the word itself.
pub fn common_typos(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < MIN_LEN {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..chars.len() {
        for n in neighbours(chars[i]) {
            let mut v = chars.clone();
            v[i] = n;
            out.push(v.into_iter().collect());
        }
        if i + 1 < chars.len() && chars[i] != chars[i + 1] {
            let mut v = chars.clone();
            v.swap(i, i + 1);
            out.push(v.into_iter().collect());
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn is_hangul_jamo(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF)
}

fn neighbours(c: char) -> Vec<char> {
    if c.is_ascii_lowercase() {
        return key_neighbours(&LATIN_ROWS, c);
    }
    for (base, table) in [CHOSEONG, JUNGSEONG, JONGSEONG] {
        let jamo: Vec<char> = table.chars().collect();
        let Some(index) = (c as u32)
            .checked_sub(base)
            .filter(|i| (*i as usize) < jamo.len())
        else {
            continue;
        };
        // Swap keys on the Korean layout, keeping the jamo's role in the syllable.
        return key_neighbours(&HANGUL_ROWS, jamo[index as usize])
            .into_iter()
            .filter_map(|n| jamo.iter().position(|j| *j == n))
            .filter_map(|i| char::from_u32(base + i as u32))
            .collect();
    }
    Vec::new()
}

/// Keys touching `c` on a staggered keyboard: left, right, and the two above and below.
fn key_neighbours(rows: &[&str; 3], c: char) -> Vec<char> {
    let grid: Vec<Vec<char>> = rows.iter().map(|r| r.chars().collect()).collect();
    let Some((r, col)) = grid
        .iter()
        .enumerate()
        .find_map(|(r, row)| row.iter().position(|k| *k == c).map(|col| (r, col)))
    else {
        return Vec::new();
    };
    let mut spots = vec![(r, col.wrapping_sub(1)), (r, col + 1)];
    if r > 0 {
        spots.extend([(r - 1, col), (r - 1, col + 1)]);
    }
    if r + 1 < grid.len() {
        spots.extend([(r + 1, col.wrapping_sub(1)), (r + 1, col)]);
    }
    spots
        .into_iter()
        .filter_map(|(r, c)| grid.get(r).and_then(|row| row.get(c)).copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::normalize::normalize;

    fn has(word: &str, typo: &str) -> bool {
        common_typos(&normalize(word)).contains(&normalize(typo))
    }

    #[test]
    fn neighbouring_keys_and_swaps() {
        assert!(has("love", "kove"));
        assert!(has("polite", "politw"));
        assert!(has("weird", "wierd"));
        assert!(!has("love", "lose"), "v and s are not neighbours");
        assert!(!has("love", "lovely"));
        assert!(common_typos("cat").is_empty());
    }

    #[test]
    fn korean_layout() {
        // ㅓ (j) and ㅏ (k) are neighbouring keys.
        assert!(has("인절미", "인잘미"));
        // ㅅ (t) and ㅎ (g) are neighbours; both are consonants.
        assert!(has("사랑해", "하랑해"));
        assert!(
            !has("사랑해", "자랑해"),
            "ㅅ (t) and ㅈ (w) are not neighbours"
        );
    }
}
