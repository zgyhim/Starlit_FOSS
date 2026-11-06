use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub kind: String,
    pub id: String,
}

impl Conversation {
    fn is_group(&self) -> bool {
        self.kind == "Group"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: i64,
    pub conversation: Conversation,
    pub direction: String,
    pub author: Author,
    pub parts: Value,
    pub text: String,
    pub reply_to: Option<i64>,
    pub reply_preview: Option<Value>,
    pub sent_at: i64,
    pub client_id: Option<String>,
    pub status: String,
    #[serde(default)]
    pub system: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendPayload {
    pub conversation: Conversation,
    pub parts: Value,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub reply_to: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePage {
    pub messages: Vec<Message>,
    pub has_more: bool,
    pub next_before_id: Option<i64>,
}

fn read_json(state: &AppState, name: &str) -> Value {
    let path = state.paths.data.join(name);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({}))
}

fn characters(state: &AppState) -> Vec<Value> {
    read_json(state, "characters.json")
        .get("characters")
        .and_then(Value::as_array)
        .map(|items| items.to_vec())
        .unwrap_or_default()
}

fn avatar_items(state: &AppState) -> Value {
    let mut items = serde_json::Map::new();
    for dir in ["character_avatars", "avatars"] {
        let root = state.paths.data.join(dir);
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if !path.is_file() {
                continue;
            }
            let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
                continue;
            };
            if !matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif") {
                continue;
            }
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            items.insert(
                stem.to_string(),
                Value::String(format!("starlit://local/{dir}/{name}")),
            );
        }
    }
    Value::Object(items)
}

fn groups(state: &AppState) -> Vec<Value> {
    read_json(state, "groups.json")
        .get("groups")
        .and_then(Value::as_array)
        .map(|items| items.to_vec())
        .unwrap_or_default()
}

#[tauri::command]
pub fn identity(state: State<'_, AppState>) -> Value {
    let cfg = state.config();
    json!({
        "ownerId": cfg.qq.owner_id.parse::<i64>().unwrap_or(10001),
        "ownerName": cfg.qq.server.owner_nickname,
        "botName": cfg.qq.server.bot_nickname,
    })
}

#[tauri::command]
pub fn list_characters(state: State<'_, AppState>) -> Vec<Value> {
    characters(&state)
        .into_iter()
        .map(|c| {
            json!({
                "id": c.get("id").and_then(Value::as_str).unwrap_or(""),
                "name": c.get("name").and_then(Value::as_str).unwrap_or("角色"),
                "series": c.get("series").and_then(Value::as_str).unwrap_or(""),
            })
        })
        .collect()
}

#[tauri::command]
pub fn list_groups(state: State<'_, AppState>) -> Vec<Value> {
    groups(&state)
        .into_iter()
        .map(|g| {
            json!({
                "id": g.get("id").and_then(Value::as_str).unwrap_or(""),
                "name": g.get("name").and_then(Value::as_str).unwrap_or("群聊"),
                "avatar": g.get("avatar").and_then(Value::as_str).unwrap_or(""),
                "memberCount": 0,
            })
        })
        .collect()
}

fn parts_text(parts: &Value) -> String {
    let mut text = String::new();
    if let Some(items) = parts.as_array() {
        for part in items {
            match part.get("kind").and_then(Value::as_str) {
                Some("text") => {
                    text.push_str(part.get("text").and_then(Value::as_str).unwrap_or(""))
                }
                Some("image") | Some("sticker") => text.push_str("[图片]"),
                Some("voice") => text.push_str("[语音]"),
                Some("mention") => {
                    text.push('@');
                    text.push_str(part.get("label").and_then(Value::as_str).unwrap_or(""));
                }
                Some("share") => {
                    text.push_str("[分享]");
                    text.push_str(part.get("title").and_then(Value::as_str).unwrap_or(""));
                }
                Some("forward") => text.push_str(
                    part.get("title").and_then(Value::as_str).unwrap_or("聊天记录"),
                ),
                _ => {}
            }
        }
    }
    text
}

#[tauri::command]
pub fn send_message(state: State<'_, AppState>, payload: SendPayload) -> Result<Message, String> {
    let now = chrono::Utc::now().timestamp();
    let client_id = payload.client_id.clone().unwrap_or_default();
    let text = parts_text(&payload.parts);
    let parts_json = serde_json::to_string(&payload.parts).map_err(|e| e.to_string())?;
    let conversation_kind = payload.conversation.kind.clone();
    let conversation_id = payload.conversation.id.clone();

    let cfg = state.config();
    let author_id = cfg.qq.owner_id.clone();
    let author_name = cfg.qq.server.owner_nickname.clone();

    let id = state.db.with(|conn| {
        conn.execute(
            "INSERT INTO messages
             (conversation_kind, conversation_id, direction, author_id, author_name,
              parts, text, reply_to, sent_at, client_id, status)
             VALUES (?1, ?2, 'Outgoing', ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'Sent')",
            rusqlite::params![
                conversation_kind,
                conversation_id,
                author_id,
                author_name,
                parts_json,
                text,
                payload.reply_to,
                now,
                client_id,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    })?;

    Ok(Message {
        id,
        conversation: payload.conversation,
        direction: "Outgoing".into(),
        author: Author {
            id: cfg.qq.owner_id.clone(),
            name: author_name,
            role: "owner".into(),
        },
        parts: payload.parts,
        text,
        reply_to: payload.reply_to,
        reply_preview: None,
        sent_at: now,
        client_id: Some(client_id),
        status: "Sent".into(),
        system: false,
    })
}

#[tauri::command]
pub fn list_messages(
    state: State<'_, AppState>,
    conversation: Conversation,
    before_id: Option<i64>,
    limit: Option<i64>,
) -> Result<MessagePage, String> {
    let limit = limit.unwrap_or(80).clamp(1, 500);
    let group = conversation.is_group();
    let mut rows: Vec<Message> = Vec::new();
    let mut next_before_id = None;
    state.db.with(|conn| {
        let sql = if before_id.is_some() {
            "SELECT id, direction, author_id, author_name, parts, text, reply_to, sent_at,
                    client_id, status FROM messages
             WHERE conversation_kind = ?1 AND conversation_id = ?2 AND id < ?3
             ORDER BY id DESC LIMIT ?4"
        } else {
            "SELECT id, direction, author_id, author_name, parts, text, reply_to, sent_at,
                    client_id, status FROM messages
             WHERE conversation_kind = ?1 AND conversation_id = ?2
             ORDER BY id DESC LIMIT ?3"
        };
        let mut stmt = conn.prepare(sql)?;
        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<Message> {
            let parts_text: String = row.get(4)?;
            let parts: Value = serde_json::from_str(&parts_text).unwrap_or_else(|_| json!([]));
            Ok(Message {
                id: row.get(0)?,
                conversation: conversation.clone(),
                direction: row.get(1)?,
                author: Author {
                    id: row.get(2)?,
                    name: row.get(3)?,
                    role: String::new(),
                },
                parts,
                text: row.get(5)?,
                reply_to: row.get(6)?,
                reply_preview: None,
                sent_at: row.get(7)?,
                client_id: row.get(8)?,
                status: row.get(9)?,
                system: false,
            })
        };
        if let Some(before) = before_id {
            rows = stmt
                .query_map(rusqlite::params![
                    if group { "Group" } else { "Private" },
                    conversation.id,
                    before,
                    limit + 1
                ], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        } else {
            rows = stmt
                .query_map(rusqlite::params![
                    if group { "Group" } else { "Private" },
                    conversation.id,
                    limit + 1
                ], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        if rows.len() as i64 > limit {
            rows.truncate(limit as usize);
        }
        next_before_id = rows.last().map(|m| m.id);
        rows.reverse();
        Ok(())
    })?;
    let has_more = rows.len() as i64 == limit;
    Ok(MessagePage {
        messages: rows,
        has_more,
        next_before_id,
    })
}

#[tauri::command]
pub fn recall_message(
    state: State<'_, AppState>,
    conversation: Conversation,
    message_id: String,
) -> Result<(), String> {
    let id: i64 = message_id.parse().map_err(|_| "无效消息 ID".to_string())?;
    let group = if conversation.is_group() { "Group" } else { "Private" };
    state.db.with(|conn| {
        conn.execute(
            "UPDATE messages SET status = 'Recalled'
             WHERE id = ?1 AND conversation_kind = ?2 AND conversation_id = ?3",
            rusqlite::params![id, group, conversation.id],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn mark_read(
    state: State<'_, AppState>,
    conversation: Conversation,
    read_ts: i64,
) -> Result<(), String> {
    let group = if conversation.is_group() { "Group" } else { "Private" };
    state.db.with(|conn| {
        conn.execute(
            "INSERT INTO read_state (conversation_kind, conversation_id, read_ts)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(conversation_kind, conversation_id)
             DO UPDATE SET read_ts = MAX(read_ts, excluded.read_ts)",
            rusqlite::params![group, conversation.id, read_ts],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn retry_reply(_conversation: Conversation, _client_id: String) -> Value {
    json!({ "retrying": false, "reason": "not_available" })
}

#[tauri::command]
pub fn ping() -> bool {
    true
}

#[derive(Serialize)]
pub struct ApiOut {
    pub status: u16,
    pub body: Value,
}

fn ok(mut body: Value) -> Value {
    if body.get("ok").is_none() {
        body["ok"] = json!(true);
    }
    body
}

#[tauri::command]
pub fn api_request(
    state: State<'_, AppState>,
    url: String,
    method: String,
    body: Option<String>,
) -> ApiOut {
    let _ = (method, body);
    let path = url.split('?').next().unwrap_or("");
    let cfg = state.config();
    let payload = match path {
        "/api/groups" => json!({
            "groups": groups(&state).into_iter().map(|g| json!({
                "groupId": g.get("id").and_then(Value::as_str).unwrap_or(""),
                "groupName": g.get("name").and_then(Value::as_str).unwrap_or("群聊"),
                "avatar": g.get("avatar").and_then(Value::as_str).unwrap_or(""),
                "memberCount": 0,
            })).collect::<Vec<_>>(),
            "requests": [],
        }),
        "/api/moods" => json!({ "items": {} }),
        "/api/bond" => json!({ "bond": null, "candidates": [] }),
        "/api/presence" => json!({ "status": "online", "label": "在线" }),
        "/api/avatars" => json!({ "items": avatar_items(&state) }),
        "/api/sticker-sets" => json!({ "sets": {} }),
        "/api/character-traits" => json!({ "voice": { "enabled": false } }),
        "/api/settings" => json!({
            "llm": {
                "base_url": cfg.llm.base_url,
                "api_key": cfg.llm.api_key,
                "model": cfg.llm.model,
            },
            "tts": {
                "provider": cfg.tts.provider,
                "endpoint": cfg.tts.endpoint,
                "mode": "off",
                "voice": "",
                "speed": 1.0,
            },
            "setup": { "required": false },
        }),
        "/api/identity" => json!({
            "owner_id": cfg.qq.owner_id.parse::<i64>().unwrap_or(10001),
            "owner_nickname": cfg.qq.server.owner_nickname,
            "bot_nickname": cfg.qq.server.bot_nickname,
        }),
        "/api/profile" => json!({ "profile": {} }),
        "/api/memories" => json!({ "items": [], "total": 0, "next_offset": 0 }),
        "/api/shared-life" => json!({
            "agreements": [], "anniversaries": [], "diaries": [], "overview": {},
        }),
        "/api/diagnostics" => json!({ "checks": [] }),
        _ => json!({}),
    };
    ApiOut {
        status: 200,
        body: ok(payload),
    }
}
