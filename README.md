# Instagram Chat Viewer

Instagram Chat Viewer is a free desktop app for your Instagram chat log. Open an Instagram chatlog from Instagram's "Download your information" export, then read, search, and merge your direct messages on your own computer.

<p align="center">
  <a href="https://github.com/Atsilut/instagram_chatlog_reader/releases/latest">
    <img alt="Download Instagram Chat Viewer" src="https://img.shields.io/github/v/release/Atsilut/instagram_chatlog_reader?style=for-the-badge&label=Download&color=4f46e5">
  </a>
</p>

My partner and I are in a long-distance relationship, so we need to read our old texts well. Meta's basic chat log from the export makes that hard, and there was no project like this, so I built it. I hope it helps other people too.

> **Linux and Intel Mac builds are very experimental.** I do not have a Linux machine or an Intel Mac, so those packages are published without being run on the real hardware. Windows and Apple Silicon are the platforms I use.

Ideas, bugs, and suggestions are welcome. Open a [GitHub issue](https://github.com/Atsilut/instagram_chatlog_reader/issues) or email [ahh8096@gmail.com](mailto:ahh8096@gmail.com).

## Download

[Download the latest release](https://github.com/Atsilut/instagram_chatlog_reader/releases/latest) and pick the file for your computer. The `*` in the names below is the version number.

| Your computer | File |
| --- | --- |
| Windows 10 or 11 | `Instagram.Chat.Viewer_*_x64-setup.exe` |
| Mac with Apple silicon (M1 or later) | `Instagram.Chat.Viewer_*_aarch64.dmg` |
| Mac with an Intel processor | `Instagram.Chat.Viewer_*_x64.dmg` |
| Linux (64-bit) | `Instagram.Chat.Viewer_*_amd64.deb` or `Instagram.Chat.Viewer_*_amd64.AppImage` |

**Windows.** Run the setup file. It installs for your user account. Windows SmartScreen may warn you, because the installer is signed with the `Atsilut.cer` certificate in that same release (the name on the certificate is Atsilut).

**Mac.** Open the disk image and drag Instagram Chat Viewer to Applications. The first time you open it, Control-click the app and choose Open.

**Linux.** Debian and Ubuntu can install the `.deb`. On other distributions, use the `.AppImage`. If it will not start, install `libfuse2`, or launch it with `APPIMAGE_EXTRACT_AND_RUN=1`.

The Intel Mac and Linux files are very experimental. If one fails, please tell me.

## How to read your chats

Instagram Chat Viewer only reads the export files you give it. Your messages stay on this computer.

### 1. Download your messages from Instagram

In Instagram, open **Accounts Center > Your information and permissions > Download your information**.

- Choose **HTML**.
- Include **Messages**.
- When Instagram emails you, download the ZIP.

### 2. Import it

Open the app and click **Import backups**, or drop the ZIP onto the window. A folder you already unzipped works too. You can add several backups at once, including older copies of the same chats.

Overlapping backups are merged. A message that appears in more than one backup is kept once, and a real repeated message stays repeated. Something that is missing from a newer backup stays in your library.

### 3. Read a conversation

Pick a person on the left. Photos, videos, voice messages, GIFs, and files from the export show inside the conversation.

The **Search users** box on the left finds a person by name. Message text is under **Search**.

### 4. Search the messages

Click **Search**, or press **Ctrl+F** (**Command+F** on a Mac), to search the open chat. **Ctrl+Shift+F** (**Command+Shift+F**) searches every chat. Click a result to jump to that message.

- Capitals and accents still match, so "Cafe" finds "café".
- A singular and its plural always match, so "plan" finds "plans" and "datum" finds "data".
- **Include similar words** also matches simple typos and English verb forms. Searching "love" can find "loved" and "kove". "plans" still does not find "planned".
- **Deep search** looks through every message, so "love" can find "lovely". It is slower.
- You can limit results to one person, or to a range of dates.

### 5. Names, language, and old chats

- **People** gives someone a name you prefer. The new name is used in your library.
- The gear icon sets the language (English, 한국어, 日本語, Español) and how dates are written (United States, Australia, United Kingdom, or Korea).
- The **⋯** menu on a chat can **Hide** it. Hidden chats move to the Hidden tab, and **Unhide** brings one back.
- **Delete** removes that chat from this library. Importing a backup that contains it brings the chat back.
- When you download a newer export later, import that too. Older messages remain.

## If no chats are found

The import needs the **HTML** export, with Messages included. JSON exports and `message_requests/` are not supported yet.

A normal download looks like this. You can import the whole download, one chat folder, or a single `message_N.html` file. Nested ZIPs are fine.

```
meta-{YYYY}-{Mon}-{DD}-{HH}-{mm}-{ss}/            optional wrapper, e.g. meta-2026-Jun-17-22-21-58
  instagram-{username}-{YYYY}-{MM}-{DD}-{hash}/
    your_instagram_activity/messages/inbox/{name}_{threadId}/
      message_1.html (newest) ... message_N.html (oldest)
      audio/  files/  gifs/  photos/  videos/
```

Each message has a sender name, the content, and a timestamp with minute precision and no timezone. There are no message IDs, so the same message in two backups is recognized by its content. If Meta changes the export, parsing can break until the app is updated. A message block the parser does not recognize is kept as raw HTML so it can be read again later.

This app depends on that export. It never signs in to Instagram or Meta.

## For developers

Example exports for local testing go in `examples/`. That folder is git-ignored. **Never commit real chat data.** Automated tests use synthetic fixtures. An optional real-data test reads from the path in the `ICV_SAMPLE_DIR` environment variable.

### Tech stack

- [Tauri 2](https://tauri.app/) (Rust) desktop shell, React + TypeScript UI
- SQLite for messages and metadata, [Tantivy](https://github.com/quickwit-oss/tantivy) for search
- Windows, macOS, and Linux. Windows and Apple Silicon are the builds I can test myself. Linux and Intel Mac are experimental. A Docker / browser mode is still planned.

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
