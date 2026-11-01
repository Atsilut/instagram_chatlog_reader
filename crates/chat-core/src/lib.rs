// SPDX-License-Identifier: AGPL-3.0-or-later

//! Core logic for Instagram Chat Viewer: library storage, import, merge and search.
//! Shared by the desktop app and the planned HTTP server.

pub mod db;
pub mod error;
pub mod import;
pub mod library;
pub mod media;
pub mod parse;
pub mod query;
pub mod scan;

pub use error::{Error, Result};
pub use library::Library;
