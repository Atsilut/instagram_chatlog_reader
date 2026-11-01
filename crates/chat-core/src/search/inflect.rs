// SPDX-License-Identifier: AGPL-3.0-or-later

//! English word forms for the normal search.
//!
//! "love" matches loved, loves, loving and therefore "have loved", "had loved" and
//! "used to love". A common typo of any of those forms matches too, so "lovef" and
//! "havr loved" find "have loved". Derivations such as "lovely" are not included.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use super::typos::common_typos;

/// Every token a normal-search word should match: its forms and one typo of each.
pub fn query_terms(word: &str) -> Vec<String> {
    let mut lemmas = HashSet::new();
    lemmas.insert(lemma(word));
    for typo in common_typos(word) {
        if let Some(found) = recognized_lemma(&typo) {
            lemmas.insert(found);
        }
    }

    let mut terms = HashSet::new();
    for found in lemmas {
        for form in surface_forms(&found) {
            terms.extend(common_typos(&form));
            terms.insert(form);
        }
    }
    terms.insert(word.to_string());
    let mut out: Vec<String> = terms.into_iter().collect();
    out.sort();
    out
}

fn lemma(word: &str) -> String {
    recognized_lemma(word).unwrap_or_else(|| word.to_string())
}

/// A lemma found by an inflection rule or the irregular table, not a plain word.
fn recognized_lemma(word: &str) -> Option<String> {
    if let Some(found) = irregular_lemma(word) {
        return Some(found.to_string());
    }
    strip_regular(word)
}

fn surface_forms(lemma: &str) -> Vec<String> {
    if let Some(forms) = irregular_forms(lemma) {
        return forms.iter().map(|s| (*s).to_string()).collect();
    }
    let mut forms = vec![lemma.to_string(), add_s(lemma), past(lemma), gerund(lemma)];
    forms.sort();
    forms.dedup();
    forms
}

/// The first form of each group is the lemma.
const IRREGULARS: &[&[&str]] = &[
    &["be", "am", "is", "are", "was", "were", "been", "being"],
    &["have", "has", "had", "having"],
    &["do", "does", "did", "done", "doing"],
    &["go", "goes", "went", "gone", "going"],
    &["get", "gets", "got", "gotten", "getting"],
    &["make", "makes", "made", "making"],
    &["say", "says", "said", "saying"],
    &["know", "knows", "knew", "known", "knowing"],
    &["think", "thinks", "thought", "thinking"],
    &["take", "takes", "took", "taken", "taking"],
    &["see", "sees", "saw", "seen", "seeing"],
    &["come", "comes", "came", "coming"],
    &["give", "gives", "gave", "given", "giving"],
    &["find", "finds", "found", "finding"],
    &["tell", "tells", "told", "telling"],
    &["feel", "feels", "felt", "feeling"],
    &["leave", "leaves", "left", "leaving"],
    &["put", "puts", "putting"],
    &["mean", "means", "meant", "meaning"],
    &["keep", "keeps", "kept", "keeping"],
    &["let", "lets", "letting"],
    &["begin", "begins", "began", "begun", "beginning"],
    &["hear", "hears", "heard", "hearing"],
    &["run", "runs", "ran", "running"],
    &["bring", "brings", "brought", "bringing"],
    &["write", "writes", "wrote", "written", "writing"],
    &["sit", "sits", "sat", "sitting"],
    &["stand", "stands", "stood", "standing"],
    &["lose", "loses", "lost", "losing"],
    &["pay", "pays", "paid", "paying"],
    &["meet", "meets", "met", "meeting"],
    &["set", "sets", "setting"],
    &["buy", "buys", "bought", "buying"],
    &["send", "sends", "sent", "sending"],
    &["build", "builds", "built", "building"],
    &["spend", "spends", "spent", "spending"],
    &["cut", "cuts", "cutting"],
    &["grow", "grows", "grew", "grown", "growing"],
    &["win", "wins", "won", "winning"],
    &["break", "breaks", "broke", "broken", "breaking"],
    &["eat", "eats", "ate", "eaten", "eating"],
    &["drink", "drinks", "drank", "drunk", "drinking"],
    &["sleep", "sleeps", "slept", "sleeping"],
    &["speak", "speaks", "spoke", "spoken", "speaking"],
    &["drive", "drives", "drove", "driven", "driving"],
    &["forget", "forgets", "forgot", "forgotten", "forgetting"],
    &["understand", "understands", "understood", "understanding"],
    &["become", "becomes", "became", "becoming"],
    &["choose", "chooses", "chose", "chosen", "choosing"],
    &["fall", "falls", "fell", "fallen", "falling"],
    &["hold", "holds", "held", "holding"],
    &["lead", "leads", "led", "leading"],
    &["sell", "sells", "sold", "selling"],
    &["sing", "sings", "sang", "sung", "singing"],
    &["teach", "teaches", "taught", "teaching"],
    &["catch", "catches", "caught", "catching"],
];

type IrregularTables = (
    HashMap<&'static str, &'static str>,
    HashMap<&'static str, &'static [&'static str]>,
);

fn irregular_tables() -> &'static IrregularTables {
    static TABLES: OnceLock<IrregularTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut lemma_of = HashMap::new();
        let mut forms_of = HashMap::new();
        for group in IRREGULARS {
            let lemma = group[0];
            forms_of.insert(lemma, *group);
            for form in *group {
                lemma_of.insert(*form, lemma);
            }
        }
        (lemma_of, forms_of)
    })
}

fn irregular_lemma(word: &str) -> Option<&'static str> {
    irregular_tables().0.get(word).copied()
}

fn irregular_forms(lemma: &str) -> Option<&'static [&'static str]> {
    irregular_tables().1.get(lemma).copied()
}

fn strip_regular(word: &str) -> Option<String> {
    if word.len() < 4 {
        return None;
    }
    if let Some(stem) = word.strip_suffix("ies") {
        let lemma = format!("{stem}y");
        return (add_s(&lemma) == word).then_some(lemma);
    }
    if let Some(stem) = word.strip_suffix("ied") {
        let lemma = format!("{stem}y");
        return (past(&lemma) == word).then_some(lemma);
    }
    if let Some(rest) = word.strip_suffix("ing") {
        return undo_suffix(rest, word, gerund);
    }
    if word.ends_with("eed") {
        let lemma = word[..word.len() - 1].to_string();
        if past(&lemma) == word {
            return Some(lemma);
        }
    }
    if let Some(rest) = word.strip_suffix("ed") {
        return undo_suffix(rest, word, past);
    }
    if let Some(rest) = word.strip_suffix("es") {
        return (add_s(rest) == word && rest.len() >= 2).then_some(rest.to_string());
    }
    if let Some(rest) = word.strip_suffix('s') {
        if !word.ends_with("ss") && add_s(rest) == word && rest.len() >= 3 {
            return Some(rest.to_string());
        }
    }
    None
}

/// Recovers a stem from the part before -ed or -ing.
fn undo_suffix(rest: &str, word: &str, form: fn(&str) -> String) -> Option<String> {
    if let Some(shorter) = undouble(rest) {
        if form(&shorter) == word {
            return Some(shorter);
        }
    }
    if form(rest) == word {
        return Some(rest.to_string());
    }
    let with_e = format!("{rest}e");
    (form(&with_e) == word).then_some(with_e)
}

fn undouble(rest: &str) -> Option<String> {
    let chars: Vec<char> = rest.chars().collect();
    let n = chars.len();
    (n >= 2 && chars[n - 1] == chars[n - 2] && is_consonant(chars[n - 1]))
        .then(|| chars[..n - 1].iter().collect())
}

fn add_s(lemma: &str) -> String {
    if lemma.ends_with("ies") {
        return format!("{lemma}s");
    }
    let chars: Vec<char> = lemma.chars().collect();
    if let Some('y') = chars.last() {
        if chars.len() >= 2 && is_consonant(chars[chars.len() - 2]) {
            return format!("{}ies", &lemma[..lemma.len() - 1]);
        }
    }
    if lemma.ends_with(['s', 'x', 'z', 'o']) || lemma.ends_with("ch") || lemma.ends_with("sh") {
        format!("{lemma}es")
    } else {
        format!("{lemma}s")
    }
}

fn past(lemma: &str) -> String {
    if lemma.ends_with('e') {
        return format!("{lemma}d");
    }
    let chars: Vec<char> = lemma.chars().collect();
    if let Some('y') = chars.last() {
        if chars.len() >= 2 && is_consonant(chars[chars.len() - 2]) {
            return format!("{}ied", &lemma[..lemma.len() - 1]);
        }
    }
    if is_cvc(lemma) {
        return format!("{lemma}{}ed", last_char(lemma));
    }
    format!("{lemma}ed")
}

fn gerund(lemma: &str) -> String {
    // "agreeing", "seeing": a double vowel keeps its e. "loving": a single e drops.
    if lemma.ends_with("ee") || lemma.ends_with("oe") || lemma.ends_with("ye") {
        return format!("{lemma}ing");
    }
    if let Some(stem) = lemma.strip_suffix('e') {
        return format!("{stem}ing");
    }
    if is_cvc(lemma) {
        return format!("{lemma}{}ing", last_char(lemma));
    }
    format!("{lemma}ing")
}

/// consonant-vowel-consonant, the pattern that doubles its last letter.
fn is_cvc(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < 3 {
        return false;
    }
    let [.., a, b, c] = chars[..] else {
        return false;
    };
    is_consonant(a) && is_vowel(b) && is_consonant(c) && !matches!(c, 'w' | 'x' | 'y')
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

fn is_consonant(c: char) -> bool {
    c.is_ascii_alphabetic() && !is_vowel(c)
}

fn last_char(word: &str) -> char {
    word.chars().last().unwrap_or(' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(query: &str, token: &str) -> bool {
        query_terms(query).iter().any(|t| t == token)
    }

    #[test]
    fn verb_forms_and_their_typos() {
        for token in ["love", "loves", "loved", "loving", "lovef", "kove"] {
            assert!(has("love", token), "{token}");
        }
        assert!(!has("love", "lovely"));
        assert!(has("lovef", "loved"));
        assert!(has("lovef", "love"));
        assert!(has("kove", "loved"));
    }

    #[test]
    fn irregulars_and_phrases() {
        assert!(has("have", "had"));
        assert!(has("have", "has"));
        assert!(has("havr", "have"));
        assert!(has("havr", "had"));
        assert!(has("go", "went"));
        assert!(has("be", "were"));
    }

    #[test]
    fn regular_families() {
        assert!(has("try", "tried") && has("tried", "try") && has("trying", "try"));
        assert!(has("stop", "stopped") && has("stopping", "stop"));
        assert!(has("walk", "walked") && has("walking", "walk"));
        assert!(has("agree", "agreed") && has("agreeing", "agree"));
        assert!(has("box", "boxes"));
    }
}
