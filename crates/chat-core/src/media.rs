// SPDX-License-Identifier: AGPL-3.0-or-later

//! Content-addressed media store.
//!
//! Every file is stored once under `media/{first two hex chars}/{blake3}.{ext}`, so the
//! same photo in ten backups takes the space of one. Import is split into
//! [`MediaStore::stage`] (hashing and type detection, no database, safe to run on many
//! threads) and [`MediaStore::commit`] (one short database write).
//!
//! Pages can reference files that weren't supplied. Those get a `missing` placeholder
//! via [`MediaStore::placeholder`]; when a later import provides the file,
//! [`MediaStore::link`] swaps the placeholder for the real file everywhere.

use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use tempfile::NamedTempFile;

use crate::error::IoContext;
use crate::library::MEDIA_DIR;
use crate::Result;

const STAGING_DIR: &str = ".staging";
/// Enough bytes for type detection and image headers.
const HEAD_LEN: usize = 64 * 1024;

pub type MediaId = i64;

/// A file hashed and written to staging, not yet in the database.
///
/// The descriptor is closed after the bytes are written. One chat page can
/// reference more photos than macOS will let a process keep open.
pub struct Staged {
    path: tempfile::TempPath,
    pub blake3: [u8; 32],
    pub size: u64,
    pub mime: String,
    pub ext: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaInfo {
    pub id: MediaId,
    pub mime: Option<String>,
    pub size: Option<u64>,
    /// Relative to the library root; `None` while missing.
    pub store_path: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub present: bool,
}

pub struct MediaStore {
    library_root: PathBuf,
}

impl MediaStore {
    pub fn new(library_root: impl Into<PathBuf>) -> Self {
        Self {
            library_root: library_root.into(),
        }
    }

    /// Removes staging files left behind by an interrupted import.
    pub fn clean_staging(&self) -> Result<()> {
        let dir = self.staging_dir();
        if dir.exists() {
            std::fs::remove_dir_all(&dir).at(&dir)?;
        }
        Ok(())
    }

    /// Streams `reader` into staging while hashing it. `original_name` is the export's
    /// `{kind}/{file}` (e.g. `photos/123.jpg`) and only guides type detection.
    pub fn stage(&self, reader: &mut dyn Read, original_name: &str) -> Result<Staged> {
        let dir = self.staging_dir();
        std::fs::create_dir_all(&dir).at(&dir)?;
        let mut temp = NamedTempFile::new_in(&dir).at(&dir)?;

        let mut hasher = blake3::Hasher::new();
        let mut head = Vec::with_capacity(HEAD_LEN);
        let mut size = 0u64;
        let mut buf = vec![0u8; 256 * 1024];
        {
            let mut w = BufWriter::new(temp.as_file_mut());
            loop {
                let n = reader.read(&mut buf).at(original_name)?;
                if n == 0 {
                    break;
                }
                let chunk = &buf[..n];
                hasher.update(chunk);
                if head.len() < HEAD_LEN {
                    let take = (HEAD_LEN - head.len()).min(n);
                    head.extend_from_slice(&chunk[..take]);
                }
                w.write_all(chunk).at(temp_path(&dir))?;
                size += n as u64;
            }
            w.flush().at(temp_path(&dir))?;
        }
        let path = temp.into_temp_path();

        let (mime, ext) = detect_type(&head, original_name);
        let (width, height) = if mime.starts_with("image/") {
            imagesize::blob_size(&head)
                .ok()
                .map(|s| (u32::try_from(s.width).ok(), u32::try_from(s.height).ok()))
                .unwrap_or((None, None))
        } else {
            (None, None)
        };

        Ok(Staged {
            path,
            blake3: *hasher.finalize().as_bytes(),
            size,
            mime,
            ext,
            width,
            height,
        })
    }

    /// Moves a staged file into the store and records it. Returns the existing id if
    /// identical content is already stored.
    pub fn commit(&self, conn: &Connection, staged: Staged) -> Result<MediaId> {
        if let Some(id) = media_id_by_hash(conn, &staged.blake3)? {
            return Ok(id);
        }
        let Staged {
            path,
            blake3,
            size,
            mime,
            ext,
            width,
            height,
        } = staged;
        let hex = blake3::Hash::from_bytes(blake3).to_hex();
        let rel = format!("{MEDIA_DIR}/{}/{hex}.{}", &hex[..2], ext);
        let dest = self.library_root.join(&rel);
        let parent = dest.parent().expect("media path has a parent");
        std::fs::create_dir_all(parent).at(parent)?;
        if !dest.exists() {
            path.persist(&dest).map_err(|e| e.error).at(&dest)?;
        }

        conn.execute(
            "INSERT INTO media (blake3, mime, size, store_path, width, height, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'present')",
            params![&blake3[..], mime, size as i64, rel, width, height],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Media id for `original_name` in a thread, creating a `missing` placeholder if
    /// the file hasn't been seen yet.
    pub fn placeholder(
        &self,
        conn: &Connection,
        thread_id: i64,
        original_name: &str,
    ) -> Result<MediaId> {
        if let Some(id) = ref_media_id(conn, thread_id, original_name)? {
            return Ok(id);
        }
        conn.execute("INSERT INTO media (status) VALUES ('missing')", [])?;
        let id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO media_refs (thread_id, original_name, media_id) VALUES (?1, ?2, ?3)",
            params![thread_id, original_name, id],
        )?;
        Ok(id)
    }

    /// Points `original_name` in a thread at stored media. If it pointed at a
    /// placeholder, every message part using the placeholder is moved over and the
    /// placeholder is deleted. Run inside the import transaction.
    pub fn link(
        &self,
        conn: &Connection,
        thread_id: i64,
        original_name: &str,
        media_id: MediaId,
    ) -> Result<()> {
        match ref_media_id(conn, thread_id, original_name)? {
            Some(old) if old == media_id => {}
            Some(old) => {
                conn.execute(
                    "UPDATE media_refs SET media_id = ?1 WHERE thread_id = ?2 AND original_name = ?3",
                    params![media_id, thread_id, original_name],
                )?;
                conn.execute(
                    "UPDATE message_parts SET media_id = ?1
                     WHERE media_id = ?2
                       AND message_id IN (SELECT id FROM messages WHERE thread_id = ?3)",
                    params![media_id, old, thread_id],
                )?;
                let still_used: bool = conn.query_row(
                    "SELECT EXISTS (
                         SELECT 1 FROM media_refs WHERE media_id = ?1
                         UNION ALL
                         SELECT 1 FROM message_parts WHERE media_id = ?1
                     )",
                    [old],
                    |r| r.get(0),
                )?;
                if !still_used {
                    conn.execute("DELETE FROM media WHERE id = ?1", [old])?;
                }
            }
            None => {
                conn.execute(
                    "INSERT INTO media_refs (thread_id, original_name, media_id) VALUES (?1, ?2, ?3)",
                    params![thread_id, original_name, media_id],
                )?;
            }
        }
        Ok(())
    }

    pub fn info(&self, conn: &Connection, id: MediaId) -> Result<Option<MediaInfo>> {
        Ok(conn
            .query_row(
                "SELECT id, mime, size, store_path, width, height, status FROM media WHERE id = ?1",
                [id],
                |r| {
                    Ok(MediaInfo {
                        id: r.get(0)?,
                        mime: r.get(1)?,
                        size: r.get::<_, Option<i64>>(2)?.map(|s| s as u64),
                        store_path: r.get(3)?,
                        width: r.get(4)?,
                        height: r.get(5)?,
                        present: r.get::<_, String>(6)? == "present",
                    })
                },
            )
            .optional()?)
    }

    /// Absolute path of a stored file.
    pub fn absolute(&self, store_path: &str) -> PathBuf {
        self.library_root.join(store_path)
    }

    fn staging_dir(&self) -> PathBuf {
        self.library_root.join(MEDIA_DIR).join(STAGING_DIR)
    }
}

fn temp_path(dir: &Path) -> PathBuf {
    dir.join("<staging>")
}

fn media_id_by_hash(conn: &Connection, hash: &[u8; 32]) -> Result<Option<MediaId>> {
    Ok(conn
        .query_row("SELECT id FROM media WHERE blake3 = ?1", [&hash[..]], |r| {
            r.get(0)
        })
        .optional()?)
}

fn ref_media_id(conn: &Connection, thread_id: i64, original_name: &str) -> Result<Option<MediaId>> {
    Ok(conn
        .query_row(
            "SELECT media_id FROM media_refs WHERE thread_id = ?1 AND original_name = ?2",
            params![thread_id, original_name],
            |r| r.get(0),
        )
        .optional()?)
}

/// MIME type and file extension from magic bytes, falling back to the export's file
/// extension. Voice messages are MP4 containers that sniff as video.
fn detect_type(head: &[u8], original_name: &str) -> (String, String) {
    let kind = original_name.split('/').next().unwrap_or("");
    let name_ext = original_name
        .rsplit('/')
        .next()
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 5 && e.bytes().all(|b| b.is_ascii_alphanumeric()));

    let (mut mime, mut ext) = match infer::get(head) {
        Some(t) => (t.mime_type().to_string(), t.extension().to_string()),
        None => match name_ext.as_deref() {
            Some(e) => (mime_for_ext(e).to_string(), e.to_string()),
            None => ("application/octet-stream".to_string(), "bin".to_string()),
        },
    };
    if kind == "audio" && mime == "video/mp4" {
        mime = "audio/mp4".to_string();
        ext = "m4a".to_string();
    }
    (mime, ext)
}

fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn detects_by_magic_bytes_even_without_extension() {
        let (mime, ext) = detect_type(PNG_1X1, "photos/123");
        assert_eq!((mime.as_str(), ext.as_str()), ("image/png", "png"));
    }

    #[test]
    fn falls_back_to_extension_then_bin() {
        assert_eq!(
            detect_type(b"hello", "files/notes.txt"),
            ("text/plain".into(), "txt".into())
        );
        assert_eq!(
            detect_type(b"hello", "files/noext"),
            ("application/octet-stream".into(), "bin".into())
        );
    }

    #[test]
    fn voice_messages_are_audio() {
        let mp4 = [
            0, 0, 0, 0x18, b'f', b't', b'y', b'p', b'm', b'p', b'4', b'2', 0, 0, 0, 0,
        ];
        assert_eq!(detect_type(&mp4, "audio/1.mp4").0, "audio/mp4");
        assert_eq!(detect_type(&mp4, "videos/1.mp4").0, "video/mp4");
    }

    #[test]
    fn png_dimensions_from_header() {
        let size = imagesize::blob_size(PNG_1X1).unwrap();
        assert_eq!((size.width, size.height), (3, 2));
    }
}
