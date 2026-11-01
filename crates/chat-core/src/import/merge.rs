// SPDX-License-Identifier: AGPL-3.0-or-later

//! Order-preserving merge of one minute's messages from two sources.
//!
//! Messages are aligned by their longest common subsequence of fingerprints, so
//! identical messages pair up one-to-one: two backups each holding "lol" twice in a
//! minute keep two, and a backup holding it three times adds exactly one.

use super::fingerprint::Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Index into the stored messages.
    Existing(usize),
    /// Index into the incoming messages (not stored yet).
    New(usize),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MinuteMerge {
    /// Final order of the minute.
    pub order: Vec<Slot>,
    /// `(existing, incoming)` pairs that are the same message.
    pub matched: Vec<(usize, usize)>,
}

/// Unmatched messages between two aligned ones keep stored messages first, so a
/// minute split across two pages of one backup is appended in order.
pub fn merge_minute(existing: &[Hash], incoming: &[Hash]) -> MinuteMerge {
    let (n, m) = (existing.len(), incoming.len());
    // lcs[i][j] = LCS length of existing[i..] and incoming[j..].
    let mut lcs = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[at(i, j)] = if existing[i] == incoming[j] {
                lcs[at(i + 1, j + 1)] + 1
            } else {
                lcs[at(i + 1, j)].max(lcs[at(i, j + 1)])
            };
        }
    }

    let mut out = MinuteMerge::default();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if existing[i] == incoming[j] && lcs[at(i, j)] == lcs[at(i + 1, j + 1)] + 1 {
            out.order.push(Slot::Existing(i));
            out.matched.push((i, j));
            i += 1;
            j += 1;
        } else if lcs[at(i + 1, j)] >= lcs[at(i, j + 1)] {
            out.order.push(Slot::Existing(i));
            i += 1;
        } else {
            out.order.push(Slot::New(j));
            j += 1;
        }
    }
    out.order.extend((i..n).map(Slot::Existing));
    out.order.extend((j..m).map(Slot::New));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use Slot::{Existing as E, New as N};

    fn h(c: u8) -> Hash {
        [c; 32]
    }

    fn hs(s: &str) -> Vec<Hash> {
        s.bytes().map(h).collect()
    }

    #[test]
    fn identical_minutes_match_fully() {
        let r = merge_minute(&hs("abc"), &hs("abc"));
        assert_eq!(r.order, vec![E(0), E(1), E(2)]);
        assert_eq!(r.matched, vec![(0, 0), (1, 1), (2, 2)]);
    }

    #[test]
    fn new_message_lands_between_its_neighbours() {
        let r = merge_minute(&hs("ab"), &hs("axb"));
        assert_eq!(r.order, vec![E(0), N(1), E(1)]);
    }

    #[test]
    fn disjoint_minutes_append_incoming_after_stored() {
        let r = merge_minute(&hs("ab"), &hs("cd"));
        assert_eq!(r.order, vec![E(0), E(1), N(0), N(1)]);
        assert!(r.matched.is_empty());
    }

    #[test]
    fn repeated_messages_are_a_multiset_union() {
        assert_eq!(merge_minute(&hs("ll"), &hs("ll")).order, vec![E(0), E(1)]);
        let r = merge_minute(&hs("ll"), &hs("lll"));
        assert_eq!(r.order.iter().filter(|s| matches!(s, N(_))).count(), 1);
        let r = merge_minute(&hs("ll"), &hs("l"));
        assert_eq!(r.order, vec![E(0), E(1)]);
    }

    #[test]
    fn empty_sides() {
        assert_eq!(merge_minute(&[], &hs("ab")).order, vec![N(0), N(1)]);
        assert_eq!(merge_minute(&hs("ab"), &[]).order, vec![E(0), E(1)]);
    }

    #[test]
    fn subset_adds_nothing() {
        let r = merge_minute(&hs("abcd"), &hs("bd"));
        assert_eq!(r.order, vec![E(0), E(1), E(2), E(3)]);
        assert_eq!(r.matched, vec![(1, 0), (3, 1)]);
    }
}
