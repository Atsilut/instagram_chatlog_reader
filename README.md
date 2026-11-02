# Instagram Chat Viewer

A local, cross-platform desktop app for reading, merging, and searching your Instagram direct message history.

> **Status:** early development. The design is settled; implementation is starting. Features below describe the intended behavior.

## Depends on Meta's Instagram export format

This app does **not** connect to Instagram or Meta. It reads only the files produced by Meta's **"Download your information"** export, so it depends entirely on that export's format.

- Request the export in Instagram (Accounts Center > Your information and permissions > Download your information). Choose **HTML** format and include **Messages**.
- Supported layout (as of late 2026):

  ```
  meta-{YYYY}-{Mon}-{DD}-{HH}-{mm}-{ss}/            optional wrapper, e.g. meta-2026-Jun-17-22-21-58
    instagram-{username}-{YYYY}-{MM}-{DD}-{hash}/
      your_instagram_activity/messages/inbox/{name}_{threadId}/
        message_1.html (newest) ... message_N.html (oldest)
        audio/  files/  gifs/  photos/  videos/
  ```

- Each message in the HTML has a sender display name, content, and a timestamp with **minute precision and no timezone**. There are no message IDs, so duplicates across backups are detected by content fingerprints.
- If Meta changes the export structure, HTML markup, date format, or language, parsing may break until the parser is updated. Unrecognized message blocks are kept as raw HTML so they can be re-parsed later instead of being lost.
- JSON-format exports and `message_requests/` are not supported yet.

## Features

- **Import anything:** single or multiple ZIP files or folders, at any level: whole `meta-*` wrappers, `instagram-*` exports, single chat folders, or individual `message_N.html` files. Nested ZIPs are supported.
- **Merge overlapping backups:** import manual and periodic (daily, weekly, monthly, annual) backups in any order. Messages are stored once, real repeats are preserved, and nothing is deleted even if it is missing from a newer backup.
- **Fast, typo-tolerant search:** matches the exact word, ignoring capitals and accents. Spaces are ignored for phrases of up to four words ("goodmorning" finds "good morning"). "Include similar words" also matches English verb forms (searching "love" finds "loved") and one-key typos ("kove", "lovef"), including a slip on a Korean keyboard. A Korean word also matches a longer word that starts with it, so a particle still matches. Deep search scans every message: "love" finds "lovely" because the letters occur together, and it also finds heavier typos. Jump from any result straight to that message.
- **Media:** photos, videos, voice messages, GIFs, and files are stored once (deduplicated by content) and shown inline.
- **Rename people:** override any participant's display name.
- **Hide or delete a chat:** hiding keeps the chat in the library and off the main list. Deleting removes it until you import a backup that contains it again.
- **Fully local and private:** all data stays on your machine.

## Tech stack

- [Tauri 2](https://tauri.app/) (Rust) desktop shell, React + TypeScript UI
- SQLite for messages and metadata, [Tantivy](https://github.com/quickwit-oss/tantivy) for search
- Windows first, then macOS and Linux. A Docker / browser mode is planned after the desktop release.

## Development

Example exports for local testing go in `examples/`. That folder is git-ignored. **Never commit real chat data.** Automated tests use synthetic fixtures. An optional real-data test reads from the path in the `ICV_SAMPLE_DIR` environment variable.

### Prerequisites

- [Rust](https://rustup.rs/) (the toolchain version is pinned in `rust-toolchain.toml` and installed automatically by rustup)
- [Node.js](https://nodejs.org/) 20 or newer
- Tauri system dependencies for your OS: see [Tauri prerequisites](https://tauri.app/start/prerequisites/). WebView2 is preinstalled on Windows 10/11.

### Run

```sh
cd apps/desktop
npm install
npm run tauri dev
```

The app creates its library (database, media, search index) in the OS app-data folder, for example `%APPDATA%\com.atsilut.instagramchatviewer\library` on Windows. To try things without touching that library, point the app at a scratch folder:

```powershell
$env:ICV_LIBRARY_DIR = "C:\temp\icv-lib"; npm run tauri dev
```

### Test

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# inspect how one exported page is parsed (summary, or full JSON with --json)
cargo run -p chat-core --release --bin icv-parse -- path/to/message_1.html

# list the exports and chats found in any mix of folders and ZIPs (reads names only)
cargo run -p chat-core --release --bin icv-scan -- path/to/backup.zip path/to/folder

# store all chat media from the inputs into a scratch library and report dedup/types
cargo run -p chat-core --release --bin icv-media -- --library path/to/scratch-library path/to/backup.zip

# import backups into a scratch library and print the merge report (run again with more backups)
cargo run -p chat-core --release --bin icv-import -- --library path/to/scratch-library path/to/backup.zip

# search index speed on synthetic data (indexing time, query latency, deep search)
ICV_BENCH_MESSAGES=1000000 cargo test -p chat-core --release --test search bench -- --ignored --nocapture

# parse every inbox message_*.html under a real export (absolute path)
ICV_SAMPLE_DIR=/abs/path/to/examples cargo test -p chat-core --release --test parse -- --ignored --nocapture
```

### Project layout

```
crates/chat-core/      import, merge, storage, search (shared by every app)
apps/desktop/          React UI (src/) + Tauri shell (src-tauri/)
```

## License

Copyright (C) 2026 [Atsilut](https://github.com/Atsilut)

This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the [GNU Affero General Public License](LICENSE) for more details.

## Disclaimer

Not affiliated with, endorsed by, or connected to Meta Platforms, Inc. or Instagram. "Instagram" and "Meta" are trademarks of Meta Platforms, Inc.
