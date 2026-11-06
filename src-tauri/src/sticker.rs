use rand::Rng;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::api::{Author, Conversation, Message};
use crate::state::AppState;

fn kv_get(state: &AppState, key: &str) -> Option<String> {
    state
        .db
        .with(|conn| {
            conn.query_row(
                "SELECT value FROM kv WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get::<_, String>(0),
            )
        })
        .ok()
}

fn kv_set(state: &AppState, key: &str, value: &str) {
    let now = chrono::Utc::now().timestamp();
    let _ = state.db.with(|conn| {
        conn.execute(
            "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![key, value, now],
        )?;
        Ok(())
    });
}

fn sticker_files(state: &AppState) -> Vec<String> {
    let index_path = state.paths.data.join("stickers").join("index.json");
    let root = state.paths.data.join("stickers");
    let mut files = Vec::new();
    if let Ok(text) = std::fs::read_to_string(index_path) {
        if let Ok(items) = serde_json::from_str::<Vec<Value>>(&text) {
            for item in items {
                if let Some(file) = item.get("file").and_then(Value::as_str) {
                    if root.join(file).is_file() {
                        files.push(file.to_string());
                    }
                }
            }
        }
    }
    files
}

fn sticker_url(file: &str) -> String {
    let encoded = file
        .split('/')
        .map(|part| percent_encoding::utf8_percent_encode(part, percent_encoding::NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/");
    format!("starlit://local/stickers/{encoded}")
}

/// 在回复后按概率补一张表情包，受每日上限与最小间隔约束。
pub fn maybe_send(app: &AppHandle, state: &AppState, conv: &Conversation, author_name: &str) {
    let cfg = state.config();
    if !cfg.sticker.enabled || conv.kind != "Private" {
        return;
    }
    if rand::rng().random::<f64>() >= cfg.sticker.base_probability {
        return;
    }
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let day_key = "sticker.day";
    let count_key = "sticker.count";
    let stored_day = kv_get(state, day_key).unwrap_or_default();
    let count: i64 = if stored_day == today {
        kv_get(state, count_key)
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0)
    } else {
        0
    };
    if count >= cfg.sticker.daily_max as i64 {
        return;
    }
    let now = chrono::Utc::now().timestamp();
    let last = kv_get(state, "sticker.last_ts")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    if now - last < cfg.sticker.min_gap_min as i64 * 60 {
        return;
    }
    let files = sticker_files(state);
    if files.is_empty() {
        return;
    }
    let index = rand::rng().random_range(0..files.len());
    let file = &files[index];
    let name = file.rsplit('/').next().unwrap_or(file);
    let parts = json!([{
        "kind": "sticker",
        "url": sticker_url(file),
        "name": name,
    }]);
    let message = Message {
        id: state
            .db
            .with(|conn| {
                conn.execute(
                    "INSERT INTO messages
                     (conversation_kind, conversation_id, direction, author_id, author_name,
                      parts, text, sent_at, status)
                     VALUES ('Private', ?1, 'Incoming', ?1, ?2, ?3, '[表情包]', ?4, 'Sent')",
                    rusqlite::params![conv.id, author_name, serde_json::to_string(&parts).unwrap_or_default(), now],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .unwrap_or(0),
        conversation: conv.clone(),
        direction: "Incoming".into(),
        author: Author {
            id: conv.id.clone(),
            name: author_name.to_string(),
            role: "member".into(),
        },
        parts,
        text: "[表情包]".into(),
        reply_to: None,
        reply_preview: None,
        sent_at: now,
        client_id: None,
        status: "Sent".into(),
        system: false,
    };
    let _ = app.emit("message:new", json!({ "message": message }));
    kv_set(state, day_key, &today);
    kv_set(state, count_key, &(count + 1).to_string());
    kv_set(state, "sticker.last_ts", &now.to_string());
}
