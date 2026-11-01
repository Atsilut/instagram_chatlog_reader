// SPDX-License-Identifier: AGPL-3.0-or-later

//! Metadata carried by Meta's export folder and ZIP names.

use chrono::{NaiveDate, NaiveDateTime};

/// `meta-2026-Jun-17-22-21-58` is the time the backup was made.
pub fn parse_meta_wrapper(name: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(name, "meta-%Y-%b-%d-%H-%M-%S").ok()
}

/// `instagram-{username}-{YYYY}-{MM}-{DD}-{hash}`. Instagram usernames can't contain
/// `-`, so splitting from the right is unambiguous.
pub fn parse_instagram_root(name: &str) -> Option<(String, NaiveDate)> {
    let rest = name.strip_prefix("instagram-")?;
    let mut parts = rest.rsplitn(5, '-');
    let _hash = parts.next()?;
    let day: u32 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let year: i32 = parts.next()?.parse().ok()?;
    let user = parts.next()?;
    if user.is_empty() {
        return None;
    }
    Some((user.to_string(), NaiveDate::from_ymd_opt(year, month, day)?))
}

/// `{slug}_{numeric thread id}`, e.g. `joy_1590353702351299`.
pub fn thread_id_of_folder(name: &str) -> Option<&str> {
    let (_, id) = name.rsplit_once('_')?;
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then_some(id)
}

/// `message_{N}.html` -> N.
pub fn page_number(file_name: &str) -> Option<u32> {
    file_name
        .strip_prefix("message_")?
        .strip_suffix(".html")?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_wrapper() {
        let t = parse_meta_wrapper("meta-2026-Jun-17-22-21-58").unwrap();
        assert_eq!(t.to_string(), "2026-06-17 22:21:58");
        assert!(parse_meta_wrapper("meta-2026-06-17").is_none());
    }

    #[test]
    fn instagram_root() {
        let (user, date) =
            parse_instagram_root("instagram-desmina1109-2026-10-31-pdcqPfos").unwrap();
        assert_eq!(user, "desmina1109");
        assert_eq!(date.to_string(), "2026-10-31");
        let (user, _) = parse_instagram_root("instagram-some_user.name-2025-01-02-x").unwrap();
        assert_eq!(user, "some_user.name");
        assert!(parse_instagram_root("instagram-2026-10-31-x").is_none());
        assert!(parse_instagram_root("facebook-a-2026-10-31-x").is_none());
    }

    #[test]
    fn folders_and_pages() {
        assert_eq!(
            thread_id_of_folder("joy_1590353702351299"),
            Some("1590353702351299")
        );
        assert_eq!(thread_id_of_folder("my_chat_export"), None);
        assert_eq!(page_number("message_41.html"), Some(41));
        assert_eq!(page_number("message_x.html"), None);
        assert_eq!(page_number("chats.html"), None);
    }
}
