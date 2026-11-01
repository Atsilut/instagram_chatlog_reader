// SPDX-License-Identifier: AGPL-3.0-or-later

//! Read API for the UI: chat list, message pages around any position, and renaming.
//!
//! Message pages use keyset pagination on `sort_key`, so opening a chat or jumping to
//! a search hit costs the same at message 10 or message 10 million.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::Result;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantInfo {
    pub id: i64,
    pub thread_id: i64,
    pub display_name: String,
    pub alias: Option<String>,
    pub is_owner: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub id: i64,
    pub title: String,
    pub kind: Option<String>,
    pub message_count: i64,
    pub last_sort_key: Option<i64>,
    pub last_text: Option<String>,
    pub last_kind: Option<String>,
    pub last_participant_id: Option<i64>,
    pub participants: Vec<ParticipantInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaView {
    pub id: i64,
    pub mime: Option<String>,
    /// Relative to the library root; `None` when the file wasn't in any backup.
    pub store_path: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartView {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: Option<String>,
    pub caption: Option<String>,
    pub author: Option<String>,
    pub media: Option<MediaView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionView {
    pub emoji: String,
    pub participant_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: i64,
    pub sort_key: i64,
    /// Wall-clock minutes since 1970 as printed in the export (no timezone).
    pub ts_minute: i64,
    pub participant_id: i64,
    pub kind: String,
    pub text: Option<String>,
    pub parts: Vec<PartView>,
    pub reactions: Vec<ReactionView>,
    /// Absent from the newest backup that contained the whole chat (unsent or deleted).
    pub missing_from_latest: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// The newest messages.
    Latest,
    /// Messages strictly before a sort key (scrolling up).
    Before(i64),
    /// Messages strictly after a sort key (scrolling down).
    After(i64),
    /// Messages centred on a sort key, which is included (jump to a search hit).
    Around(i64),
}

pub fn threads(conn: &Connection) -> Result<Vec<ThreadSummary>> {
    let participants = all_participants(conn)?;
    let mut stmt = conn.prepare(
        "SELECT t.id, t.title, t.kind, t.message_count, t.last_sort_key, m.text, m.kind, m.participant_id
         FROM threads t
         LEFT JOIN messages m ON m.thread_id = t.id AND m.sort_key = t.last_sort_key
         ORDER BY t.last_sort_key DESC NULLS LAST, t.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ThreadSummary {
            id: r.get(0)?,
            title: r.get(1)?,
            kind: r.get(2)?,
            message_count: r.get(3)?,
            last_sort_key: r.get(4)?,
            last_text: r.get(5)?,
            last_kind: r.get(6)?,
            last_participant_id: r.get(7)?,
            participants: Vec::new(),
        })
    })?;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        let mut t = row?;
        // Several messages can share the last sort key only if data is corrupt; keep one row.
        if !seen.insert(t.id) {
            continue;
        }
        t.participants = participants.get(&t.id).cloned().unwrap_or_default();
        out.push(t);
    }
    Ok(out)
}

pub fn participants(conn: &Connection, thread: i64) -> Result<Vec<ParticipantInfo>> {
    Ok(all_participants(conn)?.remove(&thread).unwrap_or_default())
}

fn all_participants(conn: &Connection) -> Result<HashMap<i64, Vec<ParticipantInfo>>> {
    let mut stmt = conn.prepare(
        "SELECT id, thread_id, display_name, alias, is_owner FROM participants ORDER BY thread_id, is_owner, id",
    )?;
    let mut map: HashMap<i64, Vec<ParticipantInfo>> = HashMap::new();
    for row in stmt.query_map([], |r| {
        Ok(ParticipantInfo {
            id: r.get(0)?,
            thread_id: r.get(1)?,
            display_name: r.get(2)?,
            alias: r.get(3)?,
            is_owner: r.get(4)?,
        })
    })? {
        let p = row?;
        map.entry(p.thread_id).or_default().push(p);
    }
    Ok(map)
}

/// Sets or clears (`None` or blank) a participant's display alias.
pub fn set_alias(conn: &Connection, participant: i64, alias: Option<&str>) -> Result<()> {
    let alias = alias.map(str::trim).filter(|a| !a.is_empty());
    conn.execute(
        "UPDATE participants SET alias = ?1 WHERE id = ?2",
        params![alias, participant],
    )?;
    Ok(())
}

/// Marks `participant` as the library owner ("you") in its chat, for when automatic
/// detection got it wrong. Everyone else in that chat is unmarked.
pub fn set_owner(conn: &Connection, participant: i64) -> Result<()> {
    conn.execute(
        "UPDATE participants SET is_owner = (id = ?1)
         WHERE thread_id = (SELECT thread_id FROM participants WHERE id = ?1)",
        [participant],
    )?;
    Ok(())
}

/// Up to `limit` messages of a thread in chronological order.
pub fn messages(
    conn: &Connection,
    thread: i64,
    window: Window,
    limit: usize,
) -> Result<Vec<MessageView>> {
    let limit = limit.max(1) as i64;
    let mut out = match window {
        Window::Latest => {
            let mut v = select_messages(conn, thread, "1", "DESC", limit, None)?;
            v.reverse();
            v
        }
        Window::Before(k) => {
            let mut v = select_messages(conn, thread, "m.sort_key < ?2", "DESC", limit, Some(k))?;
            v.reverse();
            v
        }
        Window::After(k) => {
            select_messages(conn, thread, "m.sort_key > ?2", "ASC", limit, Some(k))?
        }
        Window::Around(k) => {
            let half = (limit / 2).max(1);
            let mut v = select_messages(conn, thread, "m.sort_key < ?2", "DESC", half, Some(k))?;
            v.reverse();
            v.extend(select_messages(
                conn,
                thread,
                "m.sort_key >= ?2",
                "ASC",
                limit - half,
                Some(k),
            )?);
            v
        }
    };
    attach_details(conn, thread, &mut out)?;
    Ok(out)
}

fn select_messages(
    conn: &Connection,
    thread: i64,
    cond: &str,
    dir: &str,
    limit: i64,
    key: Option<i64>,
) -> Result<Vec<MessageView>> {
    let sql = format!(
        "SELECT m.id, m.sort_key, m.ts_minute, m.participant_id, m.kind, m.text,
                COALESCE(m.last_seen_at < t.latest_full_export_at, 0)
         FROM messages m JOIN threads t ON t.id = m.thread_id
         WHERE m.thread_id = ?1 AND {cond}
         ORDER BY m.sort_key {dir} LIMIT ?3"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let map = |r: &rusqlite::Row| {
        Ok(MessageView {
            id: r.get(0)?,
            sort_key: r.get(1)?,
            ts_minute: r.get(2)?,
            participant_id: r.get(3)?,
            kind: r.get(4)?,
            text: r.get(5)?,
            parts: Vec::new(),
            reactions: Vec::new(),
            missing_from_latest: r.get(6)?,
        })
    };
    let rows = stmt.query_map(params![thread, key.unwrap_or(0), limit], map)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Loads parts (with media) and reactions for a contiguous run of messages.
fn attach_details(conn: &Connection, thread: i64, msgs: &mut [MessageView]) -> Result<()> {
    let (Some(lo), Some(hi)) = (
        msgs.first().map(|m| m.sort_key),
        msgs.last().map(|m| m.sort_key),
    ) else {
        return Ok(());
    };
    let index: HashMap<i64, usize> = msgs.iter().enumerate().map(|(i, m)| (m.id, i)).collect();

    let mut stmt = conn.prepare_cached(
        "SELECT p.message_id, p.type, p.url, p.caption, p.author,
                md.id, md.mime, md.store_path, md.width, md.height, md.status
         FROM message_parts p
         JOIN messages m ON m.id = p.message_id
         LEFT JOIN media md ON md.id = p.media_id
         WHERE m.thread_id = ?1 AND m.sort_key BETWEEN ?2 AND ?3
         ORDER BY p.message_id, p.idx",
    )?;
    let rows = stmt.query_map(params![thread, lo, hi], |r| {
        let media = match r.get::<_, Option<i64>>(5)? {
            Some(id) => Some(MediaView {
                id,
                mime: r.get(6)?,
                store_path: r.get(7)?,
                width: r.get(8)?,
                height: r.get(9)?,
                present: r.get::<_, String>(10)? == "present",
            }),
            None => None,
        };
        Ok((
            r.get::<_, i64>(0)?,
            PartView {
                kind: r.get(1)?,
                url: r.get(2)?,
                caption: r.get(3)?,
                author: r.get(4)?,
                media,
            },
        ))
    })?;
    for row in rows {
        let (id, part) = row?;
        if let Some(&i) = index.get(&id) {
            msgs[i].parts.push(part);
        }
    }

    let mut stmt = conn.prepare_cached(
        "SELECT r.message_id, r.emoji, r.participant_id
         FROM reactions r JOIN messages m ON m.id = r.message_id
         WHERE m.thread_id = ?1 AND m.sort_key BETWEEN ?2 AND ?3
         ORDER BY r.message_id, r.participant_id",
    )?;
    let rows = stmt.query_map(params![thread, lo, hi], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            ReactionView {
                emoji: r.get(1)?,
                participant_id: r.get(2)?,
            },
        ))
    })?;
    for row in rows {
        let (id, reaction) = row?;
        if let Some(&i) = index.get(&id) {
            msgs[i].reactions.push(reaction);
        }
    }
    Ok(())
}

/// Sort key of the first message at or after a wall-clock minute (jump to date).
pub fn sort_key_at(conn: &Connection, thread: i64, ts_minute: i64) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT sort_key FROM messages WHERE thread_id = ?1 AND ts_minute >= ?2 ORDER BY sort_key LIMIT 1",
            params![thread, ts_minute],
            |r| r.get(0),
        )
        .optional()?)
}
