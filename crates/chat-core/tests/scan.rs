// SPDX-License-Identifier: AGPL-3.0-or-later

//! One test per import case from the plan (a-i), using small synthetic exports.

use std::io::Write;
use std::path::{Path, PathBuf};

use chat_core::scan::{scan, Scan};
use zip::write::SimpleFileOptions;

const MSG: &str = "your_instagram_activity/messages";

/// Files of one export, relative to its `instagram-*` root.
fn export_files() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            format!("{MSG}/inbox/alice_111/message_1.html"),
            b"alice page 1".to_vec(),
        ),
        (
            format!("{MSG}/inbox/alice_111/message_2.html"),
            b"alice page 2".to_vec(),
        ),
        (
            format!("{MSG}/inbox/alice_111/photos/1.jpg"),
            b"jpg".to_vec(),
        ),
        (
            format!("{MSG}/inbox/alice_111/audio/2.mp4"),
            b"mp4".to_vec(),
        ),
        (
            format!("{MSG}/inbox/carol_222/message_1.html"),
            b"carol page 1".to_vec(),
        ),
        (
            format!("{MSG}/message_requests/spam_333/message_1.html"),
            b"spam".to_vec(),
        ),
        (format!("{MSG}/photos/shared.jpg"), b"shared".to_vec()),
        (format!("{MSG}/chats.html"), b"index".to_vec()),
        ("files/Instagram-Logo.png".to_string(), b"png".to_vec()),
    ]
}

fn prefixed(prefix: &str, files: Vec<(String, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
    files
        .into_iter()
        .map(|(p, b)| (format!("{prefix}{p}"), b))
        .collect()
}

fn write_dir(root: &Path, files: &[(String, Vec<u8>)]) {
    for (rel, bytes) in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

fn zip_bytes(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        w.start_file(name.as_str(), SimpleFileOptions::default())
            .unwrap();
        w.write_all(bytes).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn write_zip(path: &Path, files: &[(String, Vec<u8>)]) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, zip_bytes(files)).unwrap();
    path.to_path_buf()
}

fn run(inputs: &[PathBuf], tmp: &Path) -> Scan {
    scan(inputs, &tmp.join("scratch")).unwrap()
}

fn page_text(s: &Scan, thread: usize, page: usize) -> String {
    String::from_utf8(s.vfs.read(s.threads[thread].pages[page].file).unwrap()).unwrap()
}

fn thread_folders(s: &Scan) -> Vec<String> {
    let mut v: Vec<String> = s
        .threads
        .iter()
        .map(|t| t.folder.clone().unwrap_or_default())
        .collect();
    v.sort();
    v
}

/// Checks one full export: alice (2 pages, 2 media) + carol, requests skipped.
fn assert_full_export(s: &Scan, export: usize) {
    let mine: Vec<_> = s.threads.iter().filter(|t| t.export == export).collect();
    assert_eq!(mine.len(), 2);
    let alice = mine
        .iter()
        .find(|t| t.folder.as_deref() == Some("alice_111"))
        .unwrap();
    assert_eq!(alice.thread_id.as_deref(), Some("111"));
    assert_eq!(
        alice.pages.iter().map(|p| p.number).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(alice.media.len(), 2);
    assert!(s.exports[export].full_export);
}

#[test]
fn case_d_i_folder_with_meta_wrapper_and_two_instagram_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let meta = tmp.path().join("meta-2026-Jun-17-22-21-58");
    write_dir(
        &meta,
        &prefixed("instagram-bob-2026-06-17-abc/", export_files()),
    );
    write_dir(
        &meta,
        &prefixed("instagram-bob-2026-06-10-def/", export_files()),
    );

    let s = run(&[meta], tmp.path());
    assert_eq!(s.exports.len(), 2);
    assert_eq!(s.threads.len(), 4);
    assert_eq!(s.report.skipped_request_pages, 2);
    for (i, e) in s.exports.iter().enumerate() {
        assert_eq!(e.owner_username.as_deref(), Some("bob"));
        assert_eq!(e.exported_at.unwrap().to_string(), "2026-06-17 22:21:58");
        assert_full_export(&s, i);
    }
    let alice = s
        .threads
        .iter()
        .position(|t| t.folder.as_deref() == Some("alice_111"))
        .unwrap();
    assert_eq!(page_text(&s, alice, 0), "alice page 1");
}

#[test]
fn case_a_one_zip_with_several_meta_folders() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = prefixed(
        "meta-2026-Jun-17-22-21-58/instagram-bob-2026-06-17-abc/",
        export_files(),
    );
    files.extend(prefixed(
        "meta-2026-Jul-01-08-00-00/instagram-bob-2026-07-01-xyz/",
        export_files(),
    ));
    let zip = write_zip(&tmp.path().join("all backups.zip"), &files);

    let s = run(&[zip], tmp.path());
    let mut times: Vec<String> = s
        .exports
        .iter()
        .map(|e| e.exported_at.unwrap().to_string())
        .collect();
    times.sort();
    assert_eq!(times, vec!["2026-06-17 22:21:58", "2026-07-01 08:00:00"]);
    for i in 0..s.exports.len() {
        assert_full_export(&s, i);
    }
    let alice = s
        .threads
        .iter()
        .position(|t| t.folder.as_deref() == Some("alice_111"))
        .unwrap();
    assert_eq!(page_text(&s, alice, 1), "alice page 2");
}

#[test]
fn case_b_c_several_zips_named_after_the_export() {
    let tmp = tempfile::tempdir().unwrap();
    let a = write_zip(
        &tmp.path().join("instagram-bob-2026-06-17-abc.zip"),
        &export_files(),
    );
    let b = write_zip(
        &tmp.path().join("meta-2026-Jul-01-08-00-00.zip"),
        &prefixed("instagram-bob-2026-07-01-xyz/", export_files()),
    );

    let s = run(&[a, b], tmp.path());
    assert_eq!(s.exports.len(), 2);
    let e0 = s
        .exports
        .iter()
        .find(|e| e.key == "instagram-bob-2026-06-17-abc")
        .unwrap();
    assert_eq!(e0.owner_username.as_deref(), Some("bob"));
    assert_eq!(e0.exported_at.unwrap().to_string(), "2026-06-17 00:00:00");
    let e1 = s
        .exports
        .iter()
        .find(|e| e.key.starts_with("meta-"))
        .unwrap();
    assert_eq!(e1.exported_at.unwrap().to_string(), "2026-07-01 08:00:00");
    assert_eq!(s.threads.len(), 4);
}

#[test]
fn case_e_g_bare_thread_folders_in_zip_and_on_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = write_zip(
        &tmp.path().join("chats.zip"),
        &[
            ("alice_111/message_1.html".into(), b"z1".to_vec()),
            ("alice_111/photos/1.jpg".into(), b"jpg".to_vec()),
            ("carol_222/message_3.html".into(), b"z3".to_vec()),
        ],
    );
    let dir = tmp.path().join("dave_444");
    write_dir(
        &dir,
        &[
            ("message_1.html".into(), b"d1".to_vec()),
            ("videos/9.mp4".into(), b"mp4".to_vec()),
        ],
    );

    let s = run(&[zip, dir], tmp.path());
    assert_eq!(
        thread_folders(&s),
        vec!["alice_111", "carol_222", "dave_444"]
    );
    for t in &s.threads {
        assert!(!s.exports[t.export].full_export);
        assert!(s.exports[t.export].owner_username.is_none());
    }
    assert!(!s
        .find_media("your_instagram_activity/messages/inbox/alice_111/photos/1.jpg")
        .is_empty());
    assert!(!s
        .find_media("your_instagram_activity/messages/inbox/dave_444/videos/9.mp4")
        .is_empty());
}

#[test]
fn case_f_g_loose_message_files() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = write_zip(
        &tmp.path().join("pages.zip"),
        &[
            ("message_1.html".into(), b"p1".to_vec()),
            ("message_7.html".into(), b"p7".to_vec()),
        ],
    );
    let loose_dir = tmp.path().join("downloads");
    write_dir(&loose_dir, &[("message_5.html".into(), b"p5".to_vec())]);
    let single = tmp.path().join("joy_999").join("message_2.html");
    write_dir(
        tmp.path(),
        &[("joy_999/message_2.html".into(), b"p2".to_vec())],
    );

    let s = run(&[zip, loose_dir, single], tmp.path());
    assert_eq!(s.threads.len(), 3);
    let in_zip = s.threads.iter().find(|t| t.dir == "pages").unwrap();
    assert_eq!(in_zip.folder, None);
    assert_eq!(
        in_zip.pages.iter().map(|p| p.number).collect::<Vec<_>>(),
        vec![1, 7]
    );
    let single = s.threads.iter().find(|t| t.dir == "joy_999").unwrap();
    assert_eq!(single.thread_id.as_deref(), Some("999"));
    assert_eq!(s.report.pages, 4);
}

#[test]
fn case_h_zip_with_meta_folder_holding_several_instagram_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = prefixed(
        "meta-2026-Jun-17-22-21-58/instagram-bob-2026-06-17-abc/",
        export_files(),
    );
    files.extend(prefixed(
        "meta-2026-Jun-17-22-21-58/instagram-amy-2026-06-17-qrs/",
        export_files(),
    ));
    let zip = write_zip(&tmp.path().join("h.zip"), &files);

    let s = run(&[zip], tmp.path());
    let mut owners: Vec<String> = s
        .exports
        .iter()
        .filter_map(|e| e.owner_username.clone())
        .collect();
    owners.sort();
    assert_eq!(owners, vec!["amy", "bob"]);
    assert_eq!(s.threads.len(), 4);
}

#[test]
fn nested_zip_inside_zip() {
    let tmp = tempfile::tempdir().unwrap();
    let inner = zip_bytes(&export_files());
    let outer = write_zip(
        &tmp.path().join("outer.zip"),
        &[("backups/instagram-bob-2026-06-17-abc.zip".into(), inner)],
    );

    let s = run(&[outer], tmp.path());
    assert_eq!(s.exports.len(), 1);
    assert_eq!(
        s.exports[0].key,
        "outer/backups/instagram-bob-2026-06-17-abc"
    );
    assert_full_export(&s, 0);
    let alice = s
        .threads
        .iter()
        .position(|t| t.folder.as_deref() == Some("alice_111"))
        .unwrap();
    assert_eq!(page_text(&s, alice, 0), "alice page 1");
    let media = s.find_media(&format!("{MSG}/inbox/alice_111/audio/2.mp4"));
    assert_eq!(s.vfs.read(media[0]).unwrap(), b"mp4");
}

#[test]
fn skips_macos_junk_and_ignores_requests() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = export_files();
    files.push((
        format!("__MACOSX/{MSG}/inbox/alice_111/._message_1.html"),
        b"junk".to_vec(),
    ));
    let zip = write_zip(&tmp.path().join("instagram-bob-2026-06-17-abc.zip"), &files);

    let s = run(&[zip], tmp.path());
    assert_eq!(s.threads.len(), 2);
    assert_eq!(s.report.skipped_request_pages, 1);
    assert!(s.vfs.files().iter().all(|f| !f.path.contains("__MACOSX")));
}

#[test]
fn nested_zip_scratch_is_cleaned_up() {
    let tmp = tempfile::tempdir().unwrap();
    let outer = write_zip(
        &tmp.path().join("outer.zip"),
        &[("inner.zip".into(), zip_bytes(&export_files()))],
    );
    let scratch = tmp.path().join("scratch");
    {
        let s = scan(&[outer], &scratch).unwrap();
        assert_eq!(s.threads.len(), 2);
        assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 1);
    }
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
}

/// `outer.zip` → `meta-*.zip` → `instagram-*.zip` → the export tree.
#[test]
fn three_levels_of_zip_keep_owner_time_pages_and_media() {
    let tmp = tempfile::tempdir().unwrap();
    // The zip's own name is the instagram/meta folder. Its entries start at
    // your_instagram_activity, the same shape as a Meta download.
    let instagram = zip_bytes(&export_files());
    let meta = zip_bytes(&[("instagram-bob-2026-06-17-abc.zip".into(), instagram)]);
    let outer = write_zip(
        &tmp.path().join("outer.zip"),
        &[("year/meta-2026-Jun-17-22-21-58.zip".into(), meta)],
    );

    let scratch = tmp.path().join("scratch");
    let s = scan(&[outer], &scratch).unwrap();
    assert_eq!(s.exports.len(), 1);
    assert_eq!(s.exports[0].owner_username.as_deref(), Some("bob"));
    assert_eq!(
        s.exports[0].exported_at.unwrap().to_string(),
        "2026-06-17 22:21:58"
    );
    assert_eq!(
        s.exports[0].key,
        "outer/year/meta-2026-Jun-17-22-21-58/instagram-bob-2026-06-17-abc"
    );
    assert_full_export(&s, 0);
    let alice = s
        .threads
        .iter()
        .position(|t| t.folder.as_deref() == Some("alice_111"))
        .unwrap();
    assert_eq!(page_text(&s, alice, 0), "alice page 1");
    let media = s.find_media(&format!("{MSG}/inbox/alice_111/audio/2.mp4"));
    assert_eq!(s.vfs.read(media[0]).unwrap(), b"mp4");
    drop(s);
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
}

/// One zip holds two export zips, and one of those holds a further export zip.
#[test]
fn sibling_zips_and_a_zip_inside_one_of_them() {
    let tmp = tempfile::tempdir().unwrap();
    let bob = zip_bytes(&prefixed("instagram-bob-2026-06-17-abc/", export_files()));
    let amy = zip_bytes(&prefixed("instagram-amy-2026-07-01-qrs/", export_files()));
    let pack = zip_bytes(&[("instagram-amy-2026-07-01-qrs.zip".into(), amy)]);
    let outer = write_zip(
        &tmp.path().join("outer.zip"),
        &[
            ("bob/instagram-bob-2026-06-17-abc.zip".into(), bob),
            ("packs/amy-pack.zip".into(), pack),
        ],
    );

    let s = run(&[outer], tmp.path());
    let mut owners: Vec<_> = s
        .exports
        .iter()
        .filter_map(|e| e.owner_username.clone())
        .collect();
    owners.sort();
    assert_eq!(owners, vec!["amy", "bob"]);
    assert_eq!(s.threads.len(), 4);
    assert_eq!(s.report.skipped_request_pages, 2);
}

#[test]
fn folder_whose_zip_contains_another_zip() {
    let tmp = tempfile::tempdir().unwrap();
    let inner = zip_bytes(&prefixed("instagram-bob-2026-06-17-abc/", export_files()));
    let dir = tmp.path().join("backups");
    let zip = write_zip(
        &dir.join("june.zip"),
        &[("instagram-bob-2026-06-17-abc.zip".into(), inner)],
    );

    let s = run(&[zip], tmp.path());
    assert_eq!(s.exports.len(), 1);
    assert_eq!(s.exports[0].owner_username.as_deref(), Some("bob"));
    assert_full_export(&s, 0);
    let alice = s
        .threads
        .iter()
        .position(|t| t.folder.as_deref() == Some("alice_111"))
        .unwrap();
    assert_eq!(page_text(&s, alice, 1), "alice page 2");
}

#[test]
fn corrupt_nested_zip_names_the_archive_chain() {
    let tmp = tempfile::tempdir().unwrap();
    let outer = write_zip(
        &tmp.path().join("outer.zip"),
        &[("year/inner.zip".into(), b"not a zip".to_vec())],
    );
    let msg = match scan(&[outer], &tmp.path().join("scratch")) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("corrupt nested zip should fail"),
    };
    assert!(
        msg.contains("outer.zip!/year/inner.zip"),
        "error should name the nested archive, got {msg}"
    );
}

/// `n` zip layers. Layer 1 is the file on disk; each deeper layer is a `.zip` entry.
fn zip_chain(layers: usize) -> Vec<u8> {
    assert!(layers >= 1);
    let mut bytes = zip_bytes(&[("note.txt".into(), b"hi".to_vec())]);
    for i in 1..layers {
        bytes = zip_bytes(&[(format!("level{i}.zip"), bytes)]);
    }
    bytes
}

#[test]
fn sixteen_zip_layers_open() {
    let tmp = tempfile::tempdir().unwrap();
    // outer.zip plus a 15-layer chain stored as one entry is 16 archives.
    let outer = write_zip(
        &tmp.path().join("outer.zip"),
        &[("wrapped.zip".into(), zip_chain(15))],
    );
    scan(&[outer], &tmp.path().join("scratch")).expect("16 layers");
}

#[test]
fn seventeen_zip_layers_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    // outer.zip (1) contains chain.zip, and zip_chain(16) is 16 more layers: 17 total.
    let outer = write_zip(
        &tmp.path().join("outer.zip"),
        &[("chain.zip".into(), zip_chain(16))],
    );
    let msg = match scan(&[outer], &tmp.path().join("scratch")) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("17 zip layers should be refused"),
    };
    assert!(
        msg.contains("16") && msg.contains("outer.zip!/"),
        "expected a depth-cap error naming the chain, got {msg}"
    );
}

#[test]
fn nested_zip_that_expands_too_far_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let zeros = vec![0u8; 8 * 1024 * 1024];
    let outer = write_zip(&tmp.path().join("outer.zip"), &[("bomb.zip".into(), zeros)]);
    let msg = match scan(&[outer], &tmp.path().join("scratch")) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("oversized nested zip should be refused"),
    };
    assert!(
        msg.contains("bomb.zip") && msg.contains("expands"),
        "expected an expansion refusal, got {msg}"
    );
}
