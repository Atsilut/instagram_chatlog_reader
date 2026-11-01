// SPDX-License-Identifier: AGPL-3.0-or-later

use chat_core::parse::{parse_page, MessageKind, ParsedMessage, ParsedPage, Part};

const FIXTURE: &str = include_str!("fixtures/message_2.html");
const INBOX: &str = "your_instagram_activity/messages/inbox/alice_1234567890123456";

fn page() -> ParsedPage {
    parse_page(FIXTURE)
}

fn msg(page: &ParsedPage, i: usize) -> &ParsedMessage {
    &page.messages[i]
}

#[test]
fn header() {
    let p = page();
    assert_eq!(p.title, "Alice");
    assert_eq!(p.owner_username.as_deref(), Some("bob_owner"));
    assert_eq!(
        p.generated_at.unwrap().to_rfc3339(),
        "2026-10-31T05:05:00+00:00"
    );
    let t = p.thread.as_ref().unwrap();
    assert_eq!(
        (t.mailbox.as_str(), t.folder.as_str(), t.thread_id.as_str()),
        ("inbox", "alice_1234567890123456", "1234567890123456")
    );
    assert_eq!(p.page, Some(2));
    assert_eq!(p.page_count, Some(2));
}

#[test]
fn counts_and_issues() {
    let p = page();
    assert_eq!(p.messages.len(), 19);
    assert_eq!(p.issues.len(), 1);
    assert_eq!(p.issues[0].block, 19);
    assert!(p.issues[0].message.contains("sometime"));
}

#[test]
fn plain_and_multiline_text() {
    let p = page();
    let m = msg(&p, 0);
    assert_eq!(m.sender, "Bob");
    assert_eq!(m.timestamp.to_string(), "2025-07-18 14:15:00");
    assert_eq!(m.kind, MessageKind::Text);
    assert_eq!(m.text.as_deref(), Some("It's just a test"));
    assert!(m.raw_html.is_none());

    assert_eq!(msg(&p, 1).text.as_deref(), Some("first line\nsecond  line"));
}

#[test]
fn reactions_keep_multi_codepoint_emoji() {
    let p = page();
    let r = &msg(&p, 2).reactions;
    assert_eq!(r.len(), 2);
    assert_eq!((r[0].emoji.as_str(), r[0].actor.as_str()), ("🙄", "Alice"));
    assert_eq!((r[1].emoji.as_str(), r[1].actor.as_str()), ("👍🏽", "Bob"));
}

#[test]
fn shares_drop_attachment_placeholder() {
    let p = page();

    let post = msg(&p, 3);
    assert_eq!(post.kind, MessageKind::Share);
    assert_eq!(post.text, None);
    assert_eq!(
        post.parts,
        vec![Part::Share {
            caption: Some("Drink water".into()),
            author: Some("some_creator".into()),
            url: Some("https://www.instagram.com/p/AAAA/".into()),
        }]
    );
    assert_eq!(post.reactions.len(), 1);

    let reel = msg(&p, 4);
    assert_eq!(reel.text, None);
    assert!(
        matches!(&reel.parts[0], Part::Share { caption: None, author: Some(a), .. } if a == "reel_maker")
    );

    let story = msg(&p, 5);
    assert!(
        matches!(&story.parts[0], Part::Share { author: Some(a), url: Some(u), .. } if a == "alice_story" && u.contains("/stories/"))
    );

    let profile = msg(&p, 6);
    assert_eq!(
        profile.parts,
        vec![Part::Share {
            caption: None,
            author: Some("alice_profile".into()),
            url: None
        }]
    );
}

#[test]
fn links_and_remote_gifs() {
    let p = page();
    let link = msg(&p, 7);
    assert_eq!(link.kind, MessageKind::Link);
    assert_eq!(link.text.as_deref(), Some("https://youtu.be/xyz"));
    assert_eq!(
        link.parts,
        vec![Part::Link {
            url: "https://youtu.be/xyz".into(),
            title: None
        }]
    );

    let titled = msg(&p, 18);
    assert_eq!(titled.kind, MessageKind::Link);
    assert_eq!(
        titled.parts,
        vec![Part::Link {
            url: "https://dict.example.com/word?x=1".into(),
            title: Some("word, meanings and more".into()),
        }]
    );

    let gif = msg(&p, 8);
    assert_eq!(gif.kind, MessageKind::Media);
    assert!(matches!(&gif.parts[0], Part::RemoteGif { url } if url.contains("giphy.com")));
}

#[test]
fn local_media() {
    let p = page();
    assert_eq!(
        msg(&p, 9).parts,
        vec![
            Part::Photo {
                path: format!("{INBOX}/photos/111.jpg")
            },
            Part::Photo {
                path: format!("{INBOX}/photos/222")
            },
        ]
    );
    assert_eq!(
        msg(&p, 10).parts,
        vec![Part::Video {
            path: format!("{INBOX}/videos/333.mp4")
        }]
    );
    assert_eq!(
        msg(&p, 11).parts,
        vec![Part::Audio {
            path: format!("{INBOX}/audio/444.mp4")
        }]
    );
    assert_eq!(
        msg(&p, 12).parts,
        vec![Part::File {
            path: format!("{INBOX}/files/555.pdf")
        }]
    );
    assert_eq!(
        msg(&p, 13).parts,
        vec![Part::Gif {
            path: format!("{INBOX}/gifs/666.gif")
        }]
    );
    for i in 9..=13 {
        assert_eq!(msg(&p, i).kind, MessageKind::Media, "message {i}");
    }
}

#[test]
fn notices() {
    let p = page();
    assert_eq!(msg(&p, 14).kind, MessageKind::ReactionNotice);
    assert_eq!(
        msg(&p, 14).text.as_deref(),
        Some("Reacted 😂 to your message")
    );
    assert_eq!(msg(&p, 15).kind, MessageKind::Like);
}

#[test]
fn empty_and_unknown_blocks_keep_raw_html() {
    let p = page();
    let empty = msg(&p, 16);
    assert_eq!(empty.kind, MessageKind::Empty);
    assert!(empty.raw_html.is_some());

    let unknown = msg(&p, 17);
    assert_eq!(unknown.kind, MessageKind::Text);
    assert_eq!(unknown.text.as_deref(), Some("hello"));
    assert!(unknown
        .raw_html
        .as_deref()
        .unwrap()
        .contains("a new kind of slot"));
}

/// Parses every `message_*.html` under `ICV_SAMPLE_DIR` (a real export kept out of git).
/// Run with: `ICV_SAMPLE_DIR=examples cargo test -p chat-core --release -- --ignored --nocapture`
#[test]
#[ignore]
fn real_sample() {
    let Ok(dir) = std::env::var("ICV_SAMPLE_DIR") else {
        eprintln!("ICV_SAMPLE_DIR not set; skipping");
        return;
    };
    let mut files = Vec::new();
    collect_message_files(std::path::Path::new(&dir), &mut files);
    assert!(!files.is_empty(), "no message_*.html under {dir}");

    for f in files {
        let html = std::fs::read_to_string(&f).unwrap();
        let start = std::time::Instant::now();
        let p = parse_page(&html);
        let elapsed = start.elapsed();

        let mut kinds = std::collections::BTreeMap::new();
        for m in &p.messages {
            *kinds.entry(format!("{:?}", m.kind)).or_insert(0usize) += 1;
        }
        let raw = p.messages.iter().filter(|m| m.raw_html.is_some()).count();
        eprintln!(
            "{}: {} messages, {} issues, {} with raw html, {:?} in {:.1?}",
            f.display(),
            p.messages.len(),
            p.issues.len(),
            raw,
            kinds,
            elapsed
        );
        for issue in p.issues.iter().take(5) {
            eprintln!("  issue: {issue:?}");
        }
        assert!(p.issues.is_empty(), "parse issues in {}", f.display());
    }
}

fn collect_message_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            collect_message_files(&path, out);
        } else if path.components().any(|c| c.as_os_str() == "inbox")
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("message_") && n.ends_with(".html"))
        {
            out.push(path);
        }
    }
}
