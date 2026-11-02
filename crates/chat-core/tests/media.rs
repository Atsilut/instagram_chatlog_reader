// SPDX-License-Identifier: AGPL-3.0-or-later

use chat_core::Library;
use rusqlite::params;

const PNG_3X2: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0xAA,
];

fn library() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open(dir.path().join("lib")).unwrap();
    (dir, lib)
}

fn store(lib: &Library, bytes: &[u8], name: &str) -> i64 {
    let media = lib.media();
    let staged = media.stage(&mut &bytes[..], name).unwrap();
    media.commit(lib.conn(), staged).unwrap()
}

fn count(lib: &Library, sql: &str) -> i64 {
    lib.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}

fn create_thread(lib: &Library) -> i64 {
    let c = lib.conn();
    c.execute(
        "INSERT INTO threads (ig_thread_id, slug, title) VALUES ('111', 'alice_111', 'Alice')",
        [],
    )
    .unwrap();
    let thread = c.last_insert_rowid();
    c.execute(
        "INSERT INTO participants (thread_id, sender_key, display_name) VALUES (?1, '@other', 'Alice')",
        [thread],
    )
    .unwrap();
    thread
}

/// Adds a photo message to `thread` whose part points at `media_id`.
fn add_photo_message(lib: &Library, thread: i64, media_id: i64) -> i64 {
    let c = lib.conn();
    let who: i64 = c
        .query_row(
            "SELECT id FROM participants WHERE thread_id = ?1",
            [thread],
            |r| r.get(0),
        )
        .unwrap();
    c.execute(
        "INSERT INTO messages (thread_id, participant_id, ts_minute, sort_key, fp, occurrence, kind)
         VALUES (?1, ?2, 1, 10000, x'00', 0, 'media')",
        params![thread, who],
    )
    .unwrap();
    let msg = c.last_insert_rowid();
    c.execute(
        "INSERT INTO message_parts (message_id, idx, type, media_id) VALUES (?1, 0, 'photo', ?2)",
        params![msg, media_id],
    )
    .unwrap();
    msg
}

#[test]
fn stores_by_content_hash_with_detected_type() {
    let (_d, lib) = library();
    let id = store(&lib, PNG_3X2, "photos/1234");
    let info = lib.media().info(lib.conn(), id).unwrap().unwrap();

    assert!(info.present);
    assert_eq!(info.mime.as_deref(), Some("image/png"));
    assert_eq!((info.width, info.height), (Some(3), Some(2)));
    assert_eq!(info.size, Some(PNG_3X2.len() as u64));
    let rel = info.store_path.unwrap();
    let hex = blake3::hash(PNG_3X2).to_hex();
    assert_eq!(rel, format!("media/{}/{hex}.png", &hex[..2]));
    assert_eq!(std::fs::read(lib.media().absolute(&rel)).unwrap(), PNG_3X2);
}

#[test]
fn identical_content_is_stored_once() {
    let (_d, lib) = library();
    let a = store(&lib, b"same bytes", "files/a.txt");
    let b = store(&lib, b"same bytes", "files/copy-from-another-backup.txt");
    let c = store(&lib, b"other bytes", "files/c.txt");

    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(count(&lib, "SELECT COUNT(*) FROM media"), 2);
    let files = walk_files(&lib.root().join("media"));
    assert_eq!(files.len(), 2, "{files:?}");
}

#[test]
fn placeholder_is_replaced_when_file_arrives_later() {
    let (_d, lib) = library();
    let media = lib.media();

    // First import: the page references photos/9.jpg but the file isn't in the backup.
    let thread = create_thread(&lib);
    let ph = media
        .placeholder(lib.conn(), thread, "photos/9.jpg")
        .unwrap();
    assert_eq!(
        media
            .placeholder(lib.conn(), thread, "photos/9.jpg")
            .unwrap(),
        ph,
        "idempotent"
    );
    assert!(!media.info(lib.conn(), ph).unwrap().unwrap().present);
    let msg = add_photo_message(&lib, thread, ph);

    // Second import supplies the file.
    let real = store(&lib, PNG_3X2, "photos/9.jpg");
    media
        .link(lib.conn(), thread, "photos/9.jpg", real)
        .unwrap();

    let part_media: i64 = lib
        .conn()
        .query_row(
            "SELECT media_id FROM message_parts WHERE message_id = ?1",
            [msg],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(part_media, real);
    assert_eq!(
        count(&lib, "SELECT COUNT(*) FROM media WHERE status = 'missing'"),
        0
    );
    assert_eq!(
        media
            .placeholder(lib.conn(), thread, "photos/9.jpg")
            .unwrap(),
        real
    );
}

#[test]
fn link_creates_ref_and_is_idempotent() {
    let (_d, lib) = library();
    let media = lib.media();
    let id = store(&lib, b"voice", "audio/1.mp4");
    let thread = create_thread(&lib);

    media.link(lib.conn(), thread, "audio/1.mp4", id).unwrap();
    media.link(lib.conn(), thread, "audio/1.mp4", id).unwrap();
    assert_eq!(count(&lib, "SELECT COUNT(*) FROM media_refs"), 1);
    assert_eq!(
        media
            .placeholder(lib.conn(), thread, "audio/1.mp4")
            .unwrap(),
        id
    );
}

#[test]
fn interrupted_staging_is_cleaned_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("lib");
    {
        let lib = Library::open(&root).unwrap();
        // Simulates a crash: the staged temp file is never committed or deleted.
        let abandoned = lib
            .media()
            .stage(&mut &b"partial"[..], "photos/1.jpg")
            .unwrap();
        std::mem::forget(abandoned);
    }
    let staging = root.join("media").join(".staging");
    assert!(staging.exists());
    Library::open(&root).unwrap();
    assert!(!staging.exists());
}

fn walk_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk_files(&p));
        } else {
            out.push(p);
        }
    }
    out
}
