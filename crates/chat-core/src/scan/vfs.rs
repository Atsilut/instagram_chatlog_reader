// SPDX-License-Identifier: AGPL-3.0-or-later

//! A read-only virtual file system over folders and ZIP files (including ZIPs inside
//! ZIPs), so the rest of the import never cares where a file physically lives.
//!
//! Every file gets a virtual path that starts with the name of the input it came
//! from. A ZIP behaves like a folder named after its file stem, so `backup.zip`
//! containing `meta-.../instagram-...` yields `backup/meta-.../instagram-.../...`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tempfile::TempDir;
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::error::IoContext;
use crate::{Error, Result};

/// How many ZIP archives may wrap each other. The file the user picked is level 1.
const MAX_ZIP_DEPTH: usize = 16;
/// Nested ZIPs smaller than this are extracted even when they compress very well.
const ZIP_BOMB_MIN_UNPACKED: u64 = 8 * 1024 * 1024;
/// Unpacked size must be more than this many times the compressed size to be refused.
const ZIP_BOMB_MAX_RATIO: u64 = 100;

pub type SourceId = usize;
pub type FileId = usize;

#[derive(Debug, Clone)]
pub enum SourceKind {
    Dir(PathBuf),
    Zip(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Source {
    pub kind: SourceKind,
    /// Human-readable origin, e.g. `D:\backups\a.zip` or `D:\backups\a.zip!/inner.zip`.
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct VFile {
    pub source: SourceId,
    /// `/`-separated virtual path, starting with the input's name.
    pub path: String,
    pub size: u64,
    loc: Loc,
}

#[derive(Debug, Clone)]
enum Loc {
    Disk(PathBuf),
    ZipEntry(usize),
}

impl VFile {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

pub struct Vfs {
    sources: Vec<Source>,
    files: Vec<VFile>,
    zips: Mutex<HashMap<SourceId, ZipArchive<BufReader<File>>>>,
    /// Nested ZIPs extracted for reading; deleted when the Vfs is dropped.
    _scratch: Option<TempDir>,
}

impl Vfs {
    /// Indexes every file under `inputs` (folders, ZIP files, or single files).
    /// Nested ZIPs are extracted into a temporary folder inside `scratch_root`.
    pub fn build(inputs: &[PathBuf], scratch_root: &Path) -> Result<Self> {
        let mut b = Builder {
            sources: Vec::new(),
            files: Vec::new(),
            zips: HashMap::new(),
            scratch_root: scratch_root.to_path_buf(),
            scratch: None,
            nested_count: 0,
        };
        for input in inputs {
            b.add_input(input)?;
        }
        Ok(Self {
            sources: b.sources,
            files: b.files,
            zips: Mutex::new(b.zips),
            _scratch: b.scratch,
        })
    }

    pub fn sources(&self) -> &[Source] {
        &self.sources
    }

    pub fn files(&self) -> &[VFile] {
        &self.files
    }

    pub fn file(&self, id: FileId) -> &VFile {
        &self.files[id]
    }

    /// Streams one file's bytes through `f`.
    pub fn with_reader<T>(
        &self,
        id: FileId,
        f: impl FnOnce(&mut dyn Read) -> std::io::Result<T>,
    ) -> Result<T> {
        let file = &self.files[id];
        match &file.loc {
            Loc::Disk(path) => {
                let mut r = BufReader::new(File::open(path).at(path)?);
                f(&mut r).at(path)
            }
            Loc::ZipEntry(index) => {
                let mut zips = self.zips.lock().expect("zip cache poisoned");
                let archive = zips
                    .get_mut(&file.source)
                    .expect("zip source registered at build time");
                let zip_path = self.source_path(file.source);
                let mut entry = archive.by_index(*index).map_err(|source| Error::Zip {
                    path: zip_path.clone(),
                    source,
                })?;
                f(&mut entry).at(zip_path.join(&file.path))
            }
        }
    }

    pub fn read(&self, id: FileId) -> Result<Vec<u8>> {
        self.with_reader(id, |r| {
            let mut buf = Vec::new();
            r.read_to_end(&mut buf)?;
            Ok(buf)
        })
    }

    fn source_path(&self, id: SourceId) -> PathBuf {
        match &self.sources[id].kind {
            SourceKind::Dir(p) | SourceKind::Zip(p) => p.clone(),
        }
    }
}

struct Builder {
    sources: Vec<Source>,
    files: Vec<VFile>,
    zips: HashMap<SourceId, ZipArchive<BufReader<File>>>,
    scratch_root: PathBuf,
    scratch: Option<TempDir>,
    nested_count: usize,
}

impl Builder {
    fn add_input(&mut self, input: &Path) -> Result<()> {
        let meta = std::fs::metadata(input).at(input)?;
        let name = file_name(input);
        if meta.is_dir() {
            self.add_dir(input, &name)
        } else if is_zip_name(&name) {
            self.add_zip(input, strip_zip_ext(&name), input.display().to_string(), 1)
        } else {
            // A single loose file keeps its parent folder name, which may be the
            // thread folder (e.g. `joy_1590353702351299/message_3.html`).
            let parent = input.parent().unwrap_or(Path::new("."));
            let parent_name = file_name(parent);
            let source = self.push_source(SourceKind::Dir(parent.to_path_buf()), parent);
            self.files.push(VFile {
                source,
                path: join(&parent_name, &name),
                size: meta.len(),
                loc: Loc::Disk(input.to_path_buf()),
            });
            Ok(())
        }
    }

    fn add_dir(&mut self, root: &Path, prefix: &str) -> Result<()> {
        let source = self.push_source(SourceKind::Dir(root.to_path_buf()), root);
        for entry in WalkDir::new(root).follow_links(false) {
            let entry = entry.map_err(|e| {
                let path = e.path().unwrap_or(root).to_path_buf();
                Error::Io {
                    path,
                    source: e.into(),
                }
            })?;
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(root)
                .expect("walkdir yields paths under root")
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if is_junk(&rel) {
                continue;
            }
            let vpath = join(prefix, &rel);
            if is_zip_name(&rel) {
                self.add_zip(
                    entry.path(),
                    strip_zip_ext(&vpath),
                    entry.path().display().to_string(),
                    1,
                )?;
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            self.files.push(VFile {
                source,
                path: vpath,
                size,
                loc: Loc::Disk(entry.path().to_path_buf()),
            });
        }
        Ok(())
    }

    fn add_zip(&mut self, path: &Path, prefix: &str, label: String, depth: usize) -> Result<()> {
        if depth > MAX_ZIP_DEPTH {
            return Err(Error::NestedTooDeep {
                label,
                max: MAX_ZIP_DEPTH,
            });
        }
        let label_path = PathBuf::from(&label);
        let zip_err = |source| Error::Zip {
            path: label_path.clone(),
            source,
        };
        let file = File::open(path).at(label_path.clone())?;
        let mut archive = ZipArchive::new(BufReader::new(file)).map_err(zip_err)?;
        let source = self.sources.len();
        self.sources.push(Source {
            kind: SourceKind::Zip(path.to_path_buf()),
            label: label.clone(),
        });

        let mut nested = Vec::new();
        for index in 0..archive.len() {
            let entry = archive.by_index_raw(index).map_err(zip_err)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry
                .name()
                .map_err(zip_err)?
                .trim_start_matches('/')
                .to_string();
            if name.is_empty() || is_junk(&name) {
                continue;
            }
            let vpath = join(prefix, &name);
            if is_zip_name(&name) {
                nested.push((index, name, vpath, entry.compressed_size(), entry.size()));
                continue;
            }
            self.files.push(VFile {
                source,
                path: vpath,
                size: entry.size(),
                loc: Loc::ZipEntry(index),
            });
        }

        for (index, name, vpath, compressed, uncompressed) in nested {
            if nested_zip_expands_too_far(compressed, uncompressed) {
                return Err(Error::ZipExpandsTooFar {
                    label: label.clone(),
                    entry: name,
                    compressed,
                    uncompressed,
                });
            }
            let child = format!("{label}!/{name}");
            let out = self.scratch_file()?;
            {
                let mut entry = archive.by_index(index).map_err(zip_err)?;
                let mut w = File::create(&out).at(&*child)?;
                std::io::copy(&mut entry, &mut w).at(&*child)?;
            }
            self.add_zip(&out, strip_zip_ext(&vpath), child, depth + 1)?;
        }

        self.zips.insert(source, archive);
        Ok(())
    }

    fn scratch_file(&mut self) -> Result<PathBuf> {
        if self.scratch.is_none() {
            std::fs::create_dir_all(&self.scratch_root).at(&self.scratch_root)?;
            let dir = tempfile::Builder::new()
                .prefix("icv-scan-")
                .tempdir_in(&self.scratch_root)
                .at(&self.scratch_root)?;
            self.scratch = Some(dir);
        }
        self.nested_count += 1;
        let dir = self.scratch.as_ref().expect("just created").path();
        Ok(dir.join(format!("nested-{}.zip", self.nested_count)))
    }

    fn push_source(&mut self, kind: SourceKind, label: &Path) -> SourceId {
        self.sources.push(Source {
            kind,
            label: label.display().to_string(),
        });
        self.sources.len() - 1
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn join(prefix: &str, rel: &str) -> String {
    if prefix.is_empty() {
        rel.to_string()
    } else {
        format!("{prefix}/{rel}")
    }
}

/// A nested ZIP that would unpack to at least 8 MiB and more than 100× its
/// compressed size is refused before it is written to the scratch directory.
fn nested_zip_expands_too_far(compressed: u64, uncompressed: u64) -> bool {
    uncompressed >= ZIP_BOMB_MIN_UNPACKED
        && uncompressed > compressed.saturating_mul(ZIP_BOMB_MAX_RATIO)
}

fn is_zip_name(name: &str) -> bool {
    name.len() > 4 && name[name.len() - 4..].eq_ignore_ascii_case(".zip")
}

fn strip_zip_ext(name: &str) -> &str {
    if is_zip_name(name) {
        &name[..name.len() - 4]
    } else {
        name
    }
}

/// OS metadata that archivers add (macOS resource forks, Finder and Explorer files).
fn is_junk(path: &str) -> bool {
    path.split('/').any(|seg| {
        seg == "__MACOSX"
            || seg == ".DS_Store"
            || seg.starts_with("._")
            || seg.eq_ignore_ascii_case("Thumbs.db")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_expansion_limit() {
        let packed = 100_000;
        assert!(!nested_zip_expands_too_far(
            packed,
            packed * ZIP_BOMB_MAX_RATIO
        ));
        assert!(nested_zip_expands_too_far(
            packed,
            packed * ZIP_BOMB_MAX_RATIO + 1
        ));
        assert!(!nested_zip_expands_too_far(1, ZIP_BOMB_MIN_UNPACKED - 1));
        assert!(!nested_zip_expands_too_far(
            ZIP_BOMB_MIN_UNPACKED,
            ZIP_BOMB_MIN_UNPACKED
        ));
    }

    #[test]
    fn zip_names() {
        assert!(is_zip_name("a/B.ZIP"));
        assert!(!is_zip_name(".zip"));
        assert_eq!(strip_zip_ext("x/backup.zip"), "x/backup");
        assert_eq!(strip_zip_ext("x/backup"), "x/backup");
    }

    #[test]
    fn junk() {
        assert!(is_junk("__MACOSX/a/message_1.html"));
        assert!(is_junk("a/._message_1.html"));
        assert!(is_junk("a/.DS_Store"));
        assert!(!is_junk("a/message_1.html"));
    }
}
