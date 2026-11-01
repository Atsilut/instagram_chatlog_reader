// SPDX-License-Identifier: AGPL-3.0-or-later

//! Developer tool: import backups into a library and print the import report.
//! Run it again with more backups to see them merged.
//!
//! cargo run -p chat-core --release --bin icv-import -- --library <dir> <folder-or-zip>...

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use chat_core::import::{import, ImportOptions, Progress};
use chat_core::Library;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut library = None;
    let mut inputs = Vec::new();
    while let Some(a) = args.next() {
        if a == "--library" {
            library = args.next().map(PathBuf::from);
        } else {
            inputs.push(PathBuf::from(a));
        }
    }
    let (Some(library), false) = (library, inputs.is_empty()) else {
        eprintln!("usage: icv-import --library <dir> <folder-or-zip>...");
        return ExitCode::from(2);
    };

    let mut lib = match Library::open(&library) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot open library: {e}");
            return ExitCode::FAILURE;
        }
    };
    let progress =
        |p: &Progress| eprint!("\r{}/{} pages  {:<40}", p.pages_done, p.pages_total, p.chat);
    let opts = ImportOptions {
        cancel: None,
        progress: Some(&progress),
    };

    let start = Instant::now();
    match import(&mut lib, &inputs, &opts) {
        Ok(report) => {
            eprintln!();
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("serializable")
            );
            println!(
                "finished in {:.1?}; library now: {:?}",
                start.elapsed(),
                lib.stats().ok()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("\nimport failed: {e}");
            ExitCode::FAILURE
        }
    }
}
