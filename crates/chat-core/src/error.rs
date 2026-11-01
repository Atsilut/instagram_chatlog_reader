// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("ZIP error in {path}: {source}")]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },

    #[error("library database version {found} is newer than this app supports ({supported})")]
    SchemaTooNew { found: i64, supported: i64 },

    #[error("library upgrade to version {version} failed: {message}")]
    Migration { version: i64, message: String },

    #[error("cancelled")]
    Cancelled,

    #[error("search index error: {0}")]
    Search(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) trait IoContext<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|source| Error::Io {
            path: path.into(),
            source,
        })
    }
}
