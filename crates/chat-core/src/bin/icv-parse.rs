// SPDX-License-Identifier: AGPL-3.0-or-later

//! Developer tool: parse one `message_N.html` and print a summary, or JSON with `--json`.
//!
//! cargo run -p chat-core --release --bin icv-parse -- path/to/message_1.html [--json]

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::time::Instant;

use chat_core::parse::parse_page;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: icv-parse <message_N.html> [--json]");
        return ExitCode::from(2);
    };
    let html = match std::fs::read_to_string(path) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let start = Instant::now();
    let page = parse_page(&html);
    let elapsed = start.elapsed();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&page).expect("serializable")
        );
        return ExitCode::SUCCESS;
    }

    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut senders: BTreeMap<&str, usize> = BTreeMap::new();
    for m in &page.messages {
        *kinds.entry(format!("{:?}", m.kind)).or_default() += 1;
        *senders.entry(m.sender.as_str()).or_default() += 1;
    }
    println!("title:     {}", page.title);
    println!(
        "owner:     {}",
        page.owner_username.as_deref().unwrap_or("?")
    );
    if let Some(t) = &page.thread {
        println!("thread:    {}/{} (id {})", t.mailbox, t.folder, t.thread_id);
    }
    if let Some(n) = page.page {
        println!("page:      message_{n}.html");
    }
    if let (Some(newest), Some(oldest)) = (page.messages.first(), page.messages.last()) {
        println!("range:     {} .. {}", oldest.timestamp, newest.timestamp);
    }
    println!(
        "messages:  {} (parsed in {elapsed:.1?})",
        page.messages.len()
    );
    println!("senders:   {senders:?}");
    println!("kinds:     {kinds:?}");
    println!("issues:    {}", page.issues.len());
    for issue in page.issues.iter().take(10) {
        println!("  block {}: {}", issue.block, issue.message);
    }
    ExitCode::SUCCESS
}
