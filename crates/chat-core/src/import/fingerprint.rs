// SPDX-License-Identifier: AGPL-3.0-or-later

//! Message identity across backups. Exports have no message IDs, so a message is
//! identified by its minute, sender and content.

use crate::parse::{MessageKind, ParsedMessage, Part};

pub type Hash = [u8; 32];

pub const SELF_KEY: &str = "@self";
pub const OTHER_KEY: &str = "@other";

/// In one-to-one chats the title is the other person's display name, so senders are
/// keyed by role. That keeps fingerprints stable when someone changes their display
/// name between backups. Group chats key by display name.
pub fn sender_key(sender: &str, title: &str, direct: bool) -> String {
    match (direct, sender == title) {
        (true, true) => OTHER_KEY.to_string(),
        (true, false) => SELF_KEY.to_string(),
        (false, _) => sender.to_string(),
    }
}

/// A page is one-to-one when the title is one of at most two senders.
pub fn looks_direct<'a>(title: &str, senders: impl Iterator<Item = &'a str>) -> bool {
    let mut distinct: Vec<&str> = Vec::new();
    for s in senders {
        if !distinct.contains(&s) {
            distinct.push(s);
            if distinct.len() > 2 {
                return false;
            }
        }
    }
    distinct.contains(&title)
}

/// Sender and content, without the time. Reactions are excluded because they change
/// between backups; link preview titles too.
pub fn content_key(sender_key: &str, m: &ParsedMessage) -> Hash {
    let mut h = blake3::Hasher::new();
    field(&mut h, sender_key.as_bytes());
    field(&mut h, kind_str(m.kind).as_bytes());
    opt(&mut h, m.text.as_deref());
    for p in &m.parts {
        match p {
            Part::Photo { path }
            | Part::Video { path }
            | Part::Audio { path }
            | Part::Gif { path }
            | Part::File { path } => {
                field(&mut h, part_type(p).as_bytes());
                field(&mut h, original_name(path).as_bytes());
            }
            Part::Share {
                caption,
                author,
                url,
            } => {
                field(&mut h, b"share");
                opt(&mut h, caption.as_deref());
                opt(&mut h, author.as_deref());
                opt(&mut h, url.as_deref());
            }
            Part::Link { url, .. } => {
                field(&mut h, b"link");
                field(&mut h, url.as_bytes());
            }
            Part::RemoteGif { url } => {
                field(&mut h, b"remote_gif");
                field(&mut h, url.as_bytes());
            }
        }
    }
    *h.finalize().as_bytes()
}

pub fn fingerprint(ts_minute: i64, content_key: &Hash) -> Hash {
    let mut h = blake3::Hasher::new();
    h.update(&ts_minute.to_le_bytes());
    h.update(content_key);
    *h.finalize().as_bytes()
}

/// `your_instagram_activity/messages/inbox/joy_1/photos/123.jpg` -> `photos/123.jpg`.
/// The chat folder is left out because it can be renamed between backups.
pub fn original_name(path: &str) -> String {
    let mut it = path.rsplit('/');
    let name = it.next().unwrap_or_default();
    match it.next() {
        Some(kind) => format!("{kind}/{name}"),
        None => name.to_string(),
    }
}

pub fn kind_str(kind: MessageKind) -> &'static str {
    match kind {
        MessageKind::Text => "text",
        MessageKind::Media => "media",
        MessageKind::Share => "share",
        MessageKind::Link => "link",
        MessageKind::ReactionNotice => "reaction_notice",
        MessageKind::Like => "like",
        MessageKind::Empty => "empty",
    }
}

pub fn part_type(p: &Part) -> &'static str {
    match p {
        Part::Photo { .. } => "photo",
        Part::Video { .. } => "video",
        Part::Audio { .. } => "audio",
        Part::Gif { .. } => "gif",
        Part::File { .. } => "file",
        Part::Share { .. } => "share",
        Part::Link { .. } => "link",
        Part::RemoteGif { .. } => "remote_gif",
    }
}

fn field(h: &mut blake3::Hasher, bytes: &[u8]) {
    h.update(&(bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

fn opt(h: &mut blake3::Hasher, s: Option<&str>) {
    match s {
        Some(s) => {
            h.update(&[1]);
            field(h, s.as_bytes());
        }
        None => {
            h.update(&[0]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn msg(sender: &str, text: &str) -> ParsedMessage {
        ParsedMessage {
            sender: sender.into(),
            timestamp: NaiveDateTime::default(),
            kind: MessageKind::Text,
            text: Some(text.into()),
            parts: vec![],
            reactions: vec![],
            raw_html: None,
        }
    }

    #[test]
    fn roles_survive_display_name_changes() {
        let before = content_key(&sender_key("Joy", "Joy", true), &msg("Joy", "hi"));
        let after = content_key(&sender_key("Joyy", "Joyy", true), &msg("Joyy", "hi"));
        assert_eq!(before, after);
        let me = content_key(&sender_key("Bob", "Joy", true), &msg("Bob", "hi"));
        assert_ne!(before, me);
    }

    #[test]
    fn group_chats_key_by_name() {
        assert_eq!(sender_key("Ann", "Weekend trip", false), "Ann");
    }

    #[test]
    fn direct_detection() {
        assert!(looks_direct("Joy", ["Joy", "Bob", "Joy"].into_iter()));
        assert!(looks_direct("Joy", ["Joy"].into_iter()));
        assert!(!looks_direct("Trip", ["Ann", "Bob"].into_iter()));
        assert!(!looks_direct("Joy", ["Joy", "Bob", "Cat"].into_iter()));
    }

    #[test]
    fn fields_are_unambiguous() {
        assert_ne!(
            content_key("@self", &msg("x", "ab")),
            content_key("@sel", &msg("x", "fab"))
        );
        let mut no_text = msg("x", "");
        no_text.text = None;
        assert_ne!(
            content_key("@self", &msg("x", "")),
            content_key("@self", &no_text)
        );
    }

    #[test]
    fn minute_is_part_of_fingerprint() {
        let ck = content_key("@self", &msg("x", "hi"));
        assert_ne!(fingerprint(1, &ck), fingerprint(2, &ck));
    }

    #[test]
    fn original_names() {
        assert_eq!(
            original_name("your_instagram_activity/messages/inbox/joy_1/photos/1.jpg"),
            "photos/1.jpg"
        );
        assert_eq!(original_name("1.jpg"), "1.jpg");
    }
}
