// SPDX-License-Identifier: AGPL-3.0-or-later

//! Developer tool: scan folders/ZIPs and show which exports and chats were found.
//! Reads file names only, not contents.
//!
//! cargo run -p chat-core --release --bin icv-scan -- <folder-or-zip>... [--json]

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use chat_core::scan::scan;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let inputs: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    if inputs.is_empty() {
        eprintln!("usage: icv-scan <folder-or-zip>... [--json]");
        return ExitCode::from(2);
    }

    let start = Instant::now();
    let s = match scan(&inputs, &std::env::temp_dir()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("scan failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let elapsed = start.elapsed();

    if json {
        let out = serde_json::json!({
            "report": s.report,
            "exports": s.exports,
            "threads": s.threads,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializable")
        );
        return ExitCode::SUCCESS;
    }

    let r = &s.report;
    println!(
        "scanned {} files from {} sources in {elapsed:.1?}: {} exports, {} chats, {} pages, {} media files, {} message-request pages skipped",
        r.files, r.sources, r.exports, r.threads, r.pages, r.media_files, r.skipped_request_pages
    );
    for (i, e) in s.exports.iter().enumerate() {
        println!(
            "\nexport {i}: {}\n  owner: {}  exported at: {}  full export: {}",
            if e.key.is_empty() {
                "(input root)"
            } else {
                &e.key
            },
            e.owner_username.as_deref().unwrap_or("?"),
            e.exported_at
                .map(|t| t.to_string())
                .unwrap_or_else(|| "? (from pages)".into()),
            e.full_export
        );
        for t in s.threads.iter().filter(|t| t.export == i) {
            let numbers: Vec<u32> = t.pages.iter().map(|p| p.number).collect();
            println!(
                "  chat {}: pages {:?}, {} media files",
                t.folder.as_deref().unwrap_or(&t.dir),
                compact(&numbers),
                t.media.len()
            );
        }
    }
    ExitCode::SUCCESS
}

/// `[1, 2, 3, 7]` -> `"1-3, 7"`.
fn compact(n: &[u32]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < n.len() {
        let mut j = i;
        while j + 1 < n.len() && n[j + 1] == n[j] + 1 {
            j += 1;
        }
        out.push(if i == j {
            n[i].to_string()
        } else {
            format!("{}-{}", n[i], n[j])
        });
        i = j + 1;
    }
    out.join(", ")
}
