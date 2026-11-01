// SPDX-License-Identifier: AGPL-3.0-or-later

//! Developer tool: store every chat media file found in the inputs into a library and
//! report deduplication and detected types.
//!
//! cargo run -p chat-core --release --bin icv-media -- --library <dir> <folder-or-zip>...

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use chat_core::media::Staged;
use chat_core::scan::scan;
use chat_core::Library;
use rayon::prelude::*;

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
        eprintln!("usage: icv-media --library <dir> <folder-or-zip>...");
        return ExitCode::from(2);
    };
    match run(library, &inputs) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(library: PathBuf, inputs: &[PathBuf]) -> chat_core::Result<()> {
    let lib = Library::open(&library)?;
    let s = scan(inputs, &library.join("tmp"))?;
    let media = lib.media();

    let files: Vec<usize> = s
        .threads
        .iter()
        .flat_map(|t| t.media.iter().copied())
        .collect();
    let bytes: u64 = files.iter().map(|&id| s.vfs.file(id).size).sum();
    println!("{} media files, {:.1} MB", files.len(), bytes as f64 / 1e6);

    let start = Instant::now();
    let staged: Vec<chat_core::Result<Staged>> = files
        .par_iter()
        .map(|&id| {
            let f = s.vfs.file(id);
            let original = original_name(&f.path);
            s.vfs.with_reader(id, |r| {
                media
                    .stage(r, &original)
                    .map_err(|e| std::io::Error::other(e.to_string()))
            })
        })
        .collect();
    let staged_in = start.elapsed();

    let commit_start = Instant::now();
    let before: i64 = lib
        .conn()
        .query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0))?;
    let mut by_mime: BTreeMap<String, usize> = BTreeMap::new();
    let mut extensionless_detected = 0;
    let tx = lib.conn().unchecked_transaction()?;
    for (st, &id) in staged.into_iter().zip(&files) {
        let st = st?;
        if !s.vfs.file(id).name().contains('.') && st.mime != "application/octet-stream" {
            extensionless_detected += 1;
        }
        *by_mime.entry(st.mime.clone()).or_default() += 1;
        media.commit(&tx, st)?;
    }
    tx.commit()?;
    let after: i64 = lib
        .conn()
        .query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0))?;

    println!(
        "hashed in {staged_in:.1?} ({:.0} MB/s), committed in {:.1?}",
        bytes as f64 / 1e6 / staged_in.as_secs_f64(),
        commit_start.elapsed()
    );
    println!(
        "new stored files: {} (library now has {after})",
        after - before
    );
    println!("extensionless files with a detected type: {extensionless_detected}");
    for (mime, n) in by_mime {
        println!("  {mime}: {n}");
    }
    Ok(())
}

/// `.../inbox/joy_1/photos/123.jpg` -> `photos/123.jpg`.
fn original_name(path: &str) -> String {
    let mut it = path.rsplit('/');
    let name = it.next().unwrap_or_default();
    let kind = it.next().unwrap_or_default();
    format!("{kind}/{name}")
}
