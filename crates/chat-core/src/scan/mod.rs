// SPDX-License-Identifier: AGPL-3.0-or-later

//! Finds chats inside any mix of ZIPs and folders.
//!
//! Inputs are classified by what they contain, never by what they're called:
//! - a full export: `.../your_instagram_activity/messages/inbox/{folder}/message_N.html`
//! - a bare thread folder: `{slug}_{id}/message_N.html`
//! - loose pages: `message_N.html` anywhere else (the chat is identified later from
//!   the page's own pagination links)
//!
//! Only the inbox is imported; `message_requests` is skipped.

pub mod names;
pub mod vfs;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use serde::Serialize;

use crate::Result;
use names::{page_number, parse_instagram_root, parse_meta_wrapper, thread_id_of_folder};
pub use vfs::{FileId, Vfs};

pub const MEDIA_KINDS: [&str; 5] = ["audio", "files", "gifs", "photos", "videos"];
const INBOX: &str = "inbox";
const REQUESTS: &str = "message_requests";

/// One backup the data came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportSource {
    /// Virtual path of the export root (or of the folder holding loose chats).
    pub key: String,
    pub owner_username: Option<String>,
    /// From the `meta-*` wrapper, else the `instagram-*` folder date. When `None`,
    /// the import falls back to the "Generated" time inside the pages.
    pub exported_at: Option<NaiveDateTime>,
    /// True when found inside a `your_instagram_activity/messages` tree.
    pub full_export: bool,
}

/// One chat folder (or one folder of loose pages) inside one export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThreadSource {
    pub export: usize,
    /// Virtual path of the folder holding the pages.
    pub dir: String,
    /// e.g. `joy_1590353702351299`; `None` for loose pages in an unrelated folder.
    pub folder: Option<String>,
    pub thread_id: Option<String>,
    /// Ascending by page number.
    pub pages: Vec<Page>,
    pub media: Vec<FileId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Page {
    pub number: u32,
    pub file: FileId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ScanReport {
    pub sources: usize,
    pub files: usize,
    pub exports: usize,
    pub threads: usize,
    pub pages: usize,
    pub media_files: usize,
    pub skipped_request_pages: usize,
}

pub struct Scan {
    pub vfs: Vfs,
    pub exports: Vec<ExportSource>,
    pub threads: Vec<ThreadSource>,
    pub report: ScanReport,
    /// `{folder}/{kind}/{file name}` -> every copy of that media file across inputs.
    media_index: HashMap<String, Vec<FileId>>,
}

/// Scans `inputs` (folders, ZIP files, or single files). Nested ZIPs are extracted
/// under `scratch_root` and removed when the returned [`Scan`] is dropped.
pub fn scan(inputs: &[PathBuf], scratch_root: &Path) -> Result<Scan> {
    let vfs = Vfs::build(inputs, scratch_root)?;
    Ok(classify(vfs))
}

impl Scan {
    /// Finds stored copies of a media file referenced from a page, e.g.
    /// `your_instagram_activity/messages/inbox/joy_1/photos/123.jpg`.
    pub fn find_media(&self, html_path: &str) -> &[FileId] {
        media_key(html_path)
            .and_then(|k| self.media_index.get(&k))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

fn classify(vfs: Vfs) -> Scan {
    let mut by_dir: BTreeMap<&str, Vec<Page>> = BTreeMap::new();
    for (id, f) in vfs.files().iter().enumerate() {
        if let Some(number) = page_number(f.name()) {
            by_dir
                .entry(parent(&f.path))
                .or_default()
                .push(Page { number, file: id });
        }
    }

    let mut exports: Vec<ExportSource> = Vec::new();
    let mut export_by_key: HashMap<String, usize> = HashMap::new();
    let mut threads: Vec<ThreadSource> = Vec::new();
    let mut thread_by_dir: HashMap<String, usize> = HashMap::new();
    let mut report = ScanReport::default();

    for (dir, mut pages) in by_dir {
        let segs: Vec<&str> = dir.split('/').collect();
        let last = segs.len() - 1;
        let folder_seg = segs[last];
        let thread_id = thread_id_of_folder(folder_seg);

        let mailbox_idx = (thread_id.is_some() && last >= 1).then(|| last - 1);
        let mailbox = mailbox_idx
            .map(|i| segs[i])
            .filter(|m| *m == INBOX || *m == REQUESTS);
        if mailbox == Some(REQUESTS) {
            report.skipped_request_pages += pages.len();
            continue;
        }

        // Walk up from the chat folder past `inbox`, `messages` and
        // `your_instagram_activity` to find the export root.
        let mut root_end = if thread_id.is_some() { last } else { last + 1 };
        let mut full_export = false;
        if mailbox.is_some() {
            root_end -= 1;
            if root_end >= 2
                && segs[root_end - 1] == "messages"
                && segs[root_end - 2] == "your_instagram_activity"
            {
                root_end -= 2;
                full_export = true;
            }
        }
        let root_segs = &segs[..root_end];
        let key = root_segs.join("/");

        let export = *export_by_key.entry(key.clone()).or_insert_with(|| {
            exports.push(export_source(key, root_segs, full_export));
            exports.len() - 1
        });

        pages.sort_by_key(|p| p.number);
        report.pages += pages.len();
        thread_by_dir.insert(dir.to_string(), threads.len());
        threads.push(ThreadSource {
            export,
            dir: dir.to_string(),
            folder: thread_id.map(|_| folder_seg.to_string()),
            thread_id: thread_id.map(str::to_string),
            pages,
            media: Vec::new(),
        });
    }

    let mut media_index: HashMap<String, Vec<FileId>> = HashMap::new();
    for (id, f) in vfs.files().iter().enumerate() {
        let segs: Vec<&str> = f.path.split('/').collect();
        if segs.len() < 3 || !MEDIA_KINDS.contains(&segs[segs.len() - 2]) {
            continue;
        }
        let thread_dir = segs[..segs.len() - 2].join("/");
        let owner = thread_by_dir.get(&thread_dir).copied();
        if owner.is_none() && thread_id_of_folder(segs[segs.len() - 3]).is_none() {
            continue;
        }
        if let Some(t) = owner {
            threads[t].media.push(id);
        }
        if let Some(k) = media_key(&f.path) {
            media_index.entry(k).or_default().push(id);
            report.media_files += 1;
        }
    }

    report.sources = vfs.sources().len();
    report.files = vfs.files().len();
    report.exports = exports.len();
    report.threads = threads.len();

    Scan {
        vfs,
        exports,
        threads,
        report,
        media_index,
    }
}

fn export_source(key: String, root_segs: &[&str], full_export: bool) -> ExportSource {
    let mut owner_username = None;
    let mut root_date = None;
    let mut wrapper_time = None;
    for seg in root_segs.iter().rev() {
        if owner_username.is_none() {
            if let Some((user, date)) = parse_instagram_root(seg) {
                owner_username = Some(user);
                root_date = Some(date);
            }
        }
        if wrapper_time.is_none() {
            wrapper_time = parse_meta_wrapper(seg);
        }
    }
    let exported_at = wrapper_time.or_else(|| root_date.and_then(|d| d.and_hms_opt(0, 0, 0)));
    ExportSource {
        key,
        owner_username,
        exported_at,
        full_export,
    }
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map(|(p, _)| p).unwrap_or("")
}

/// Last three segments (`{folder}/{kind}/{name}`) identify a media file regardless
/// of where the export was unpacked.
fn media_key(path: &str) -> Option<String> {
    let mut it = path.rsplit('/');
    let name = it.next()?;
    let kind = it.next()?;
    let folder = it.next()?;
    MEDIA_KINDS
        .contains(&kind)
        .then(|| format!("{folder}/{kind}/{name}"))
}
