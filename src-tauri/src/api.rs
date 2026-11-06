use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
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


fn safe_segment(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.contains('/') || value.contains('\\') || value == "." || value == ".." {
        return None;
    }
    Some(value.to_string())
}

fn is_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
    )
}

fn sticker_url(file: &str) -> String {
    let encoded = file
        .split('/')
        .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/");
    format!("starlit://local/stickers/{encoded}")
}

fn sticker_item(file: &str, meta: Option<&Value>) -> Value {
    let group = file.split('/').next().unwrap_or("").to_string();
    let name = file.rsplit('/').next().unwrap_or(file).to_string();
    let animated = matches!(
        file.rsplit('.')
            .next()
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("gif" | "webp")
    );
    json!({
        "url": sticker_url(file),
        "path": file,
        "group": group,
        "name": name,
        "animated": animated,
        "desc": meta.and_then(|m| m.get("desc")).cloned().unwrap_or(Value::Null),
        "emotions": meta.and_then(|m| m.get("emotions")).cloned().unwrap_or_else(|| json!([])),
        "source": meta.and_then(|m| m.get("source")).cloned().unwrap_or(Value::Null),
    })
}

fn read_sticker_index(state: &AppState) -> Vec<Value> {
    let path = state.paths.data.join("stickers").join("index.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<Value>>(&text).ok())
        .unwrap_or_default()
}

fn write_sticker_index(state: &AppState, index: &[Value]) -> Result<(), String> {
    let path = state.paths.data.join("stickers").join("index.json");
    let text = serde_json::to_string_pretty(index).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn collect_files(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, root, out);
        } else if path.is_file() && is_image(&path) {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

fn sticker_groups(state: &AppState) -> Vec<Value> {
    let root = state.paths.data.join("stickers");
    let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut seen = HashSet::new();

    for entry in read_sticker_index(state) {
        let Some(file) = entry.get("file").and_then(Value::as_str) else { continue };
        if !root.join(file).is_file() || !seen.insert(file.to_string()) {
            continue;
        }
        let group = file.split('/').next().unwrap_or("其他").to_string();
        groups.entry(group).or_default().push(sticker_item(file, Some(&entry)));
    }

    let mut files = Vec::new();
    collect_files(&root, &root, &mut files);
    for file in files {
        if !seen.insert(file.clone()) {
            continue;
        }
        let group = file.split('/').next().unwrap_or("其他").to_string();
        groups.entry(group).or_default().push(sticker_item(&file, None));
    }

    groups
        .into_iter()
        .map(|(name, mut items)| {
            items.sort_by(|a, b| {
                a.get("path").and_then(Value::as_str)
                    .cmp(&b.get("path").and_then(Value::as_str))
            });
            json!({ "key": name, "name": name, "items": items })
        })
        .collect()
}

fn upload_sticker(state: &AppState, body: Option<String>) -> Result<Value, String> {
    let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
        .map_err(|e| format!("请求格式错误: {e}"))?;
    let data = payload.get("data").and_then(Value::as_str).unwrap_or("");
    let (_, encoded) = data.split_once(',').ok_or("图片数据无效")?;
    let bytes = BASE64.decode(encoded).map_err(|e| format!("图片解码失败: {e}"))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("图片超过 8MB".into());
    }
    let raw_name = payload.get("name").and_then(Value::as_str).unwrap_or("sticker.png");
    let mut name = raw_name.rsplit('/').next().unwrap_or("sticker.png").to_string();
    if name.is_empty() {
        name = "sticker.png".into();
    }
    if !is_image(Path::new(&name)) {
        name.push_str(".png");
    }
    let group = payload
        .get("group")
        .and_then(Value::as_str)
        .and_then(safe_segment)
        .unwrap_or_else(|| "custom".into());
    let root = state.paths.data.join("stickers");
    let dir = root.join(&group);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut target = dir.join(&name);
    let stem = target.file_stem().and_then(|s| s.to_str()).unwrap_or("sticker").to_string();
    let ext = target.extension().and_then(|s| s.to_str()).unwrap_or("png").to_string();
    let mut seq = 1;
    while target.exists() {
        target = dir.join(format!("{stem}-{seq}.{ext}"));
        seq += 1;
    }
    std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
    let rel = format!(
        "{group}/{}",
        target.file_name().and_then(|s| s.to_str()).unwrap_or(&name)
    );
    let mut index = read_sticker_index(state);
    index.push(json!({ "file": rel, "desc": stem, "emotions": [], "source": "custom" }));
    write_sticker_index(state, &index)?;
    Ok(json!({ "ok": true, "path": rel, "url": sticker_url(&rel) }))
}

fn delete_sticker(state: &AppState, body: Option<String>) -> Result<Value, String> {
    let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
        .map_err(|e| e.to_string())?;
    let rel = payload.get("path").and_then(Value::as_str).unwrap_or("");
    if rel.is_empty() || rel.contains("..") {
        return Err("无效路径".into());
    }
    let root = state.paths.data.join("stickers");
    let target = root.join(rel);
    if target.is_file() {
        std::fs::remove_file(&target).map_err(|e| e.to_string())?;
    }
    let index: Vec<Value> = read_sticker_index(state)
        .into_iter()
        .filter(|entry| entry.get("file").and_then(Value::as_str) != Some(rel))
        .collect();
    write_sticker_index(state, &index)?;
    Ok(json!({ "ok": true }))
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
                Some("image") => text.push_str("[图片]"),
                Some("sticker") => text.push_str("[表情包]"),
                Some("voice") => text.push_str("[语音]"),
                Some("mention") => {
                    let label = part.get("label").and_then(Value::as_str).unwrap_or("");
                    if !label.starts_with('@') {
                        text.push('@');
                    }
                    text.push_str(label);
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
pub fn send_message(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    payload: SendPayload,
) -> Result<Message, String> {
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

    crate::autonomy::on_user_message(&state);

    crate::pipeline::reset_followup(&state, &payload.conversation);
    if payload.conversation.kind == "Private" {
        touch_bond(&state, &payload.conversation.id);
    }
    let reply_text = text.clone();
    let reply_conversation = payload.conversation.clone();
    crate::pipeline::schedule_reply(app, reply_conversation, id, reply_text);

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

fn kv_set(state: &AppState, key: &str, value: &str) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    state.db.with(|conn| {
        conn.execute(
            "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![key, value, now],
        )?;
        Ok(())
    })
}

pub fn auto_clear_enabled(state: &AppState) -> bool {
    kv_get(state, "media.auto_clear_on_exit").as_deref() == Some("1")
}

#[tauri::command]
pub fn media_cache_info(state: State<'_, AppState>) -> Value {
    let (archives, bytes) = crate::media::cache_stats(&state.paths);
    json!({
        "autoClear": auto_clear_enabled(&state),
        "pending": state.pending_media_clear.load(std::sync::atomic::Ordering::Relaxed),
        "archives": archives,
        "bytes": bytes,
    })
}

#[tauri::command]
pub fn set_media_auto_clear(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    kv_set(&state, "media.auto_clear_on_exit", if enabled { "1" } else { "0" })
}

#[tauri::command]
pub fn media_status(state: State<'_, AppState>) -> Value {
    json!({
        "ready": state.media_ready.load(std::sync::atomic::Ordering::Relaxed),
        "done": state.media_done.load(std::sync::atomic::Ordering::Relaxed),
        "total": state.media_total.load(std::sync::atomic::Ordering::Relaxed),
        "error": state.media_error.lock().map(|e| e.clone()).unwrap_or_default(),
    })
}

#[tauri::command]
pub fn clear_media_cache(state: State<'_, AppState>) -> Value {
    state
        .pending_media_clear
        .store(true, std::sync::atomic::Ordering::Relaxed);
    json!({ "scheduled": true })
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


fn characters_file(state: &AppState) -> PathBuf {
    state.paths.data.join("characters.json")
}

fn read_characters(state: &AppState) -> Value {
    std::fs::read_to_string(characters_file(state))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({ "characters": [] }))
}

fn write_characters(state: &AppState, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(characters_file(state), text).map_err(|e| e.to_string())
}

fn find_character(state: &AppState, id: &str) -> Option<(String, String)> {
    read_characters(state)
        .get("characters")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                if item.get("id").and_then(Value::as_str) == Some(id) {
                    Some((
                        item.get("name").and_then(Value::as_str).unwrap_or("角色").to_string(),
                        item.get("slug").and_then(Value::as_str).unwrap_or(id).to_string(),
                    ))
                } else {
                    None
                }
            })
        })
}

fn persona_dir(state: &AppState, slug: &str) -> PathBuf {
    state.paths.persona.join("characters").join(slug)
}

fn revision_of(text: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn read_persona(state: &AppState, slug: &str) -> String {
    std::fs::read_to_string(persona_dir(state, slug).join("persona.md")).unwrap_or_default()
}

fn persona_versions(state: &AppState, slug: &str) -> Vec<Value> {
    let dir = state.paths.persona.join(".versions").join(slug);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut items: Vec<Value> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                return None;
            }
            let text = std::fs::read_to_string(&path).ok()?;
            let name = path.file_name()?.to_str()?.to_string();
            let created = entry.metadata().ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Some(json!({
                "id": name,
                "reason": "修改前",
                "created_at": chrono::DateTime::from_timestamp(created as i64, 0)
                    .map(|t| t.to_rfc3339()).unwrap_or_default(),
                "preview": text.chars().take(100).collect::<String>(),
                "length": text.chars().count(),
            }))
        })
        .collect();
    items.sort_by(|a, b| {
        b.get("created_at").and_then(Value::as_str)
            .cmp(&a.get("created_at").and_then(Value::as_str))
    });
    items
}

fn backup_persona(state: &AppState, slug: &str, text: &str) {
    let dir = state.paths.persona.join(".versions").join(slug);
    let _ = std::fs::create_dir_all(&dir);
    let stamp = chrono::Utc::now().timestamp();
    let name = format!("{stamp}-{}.md", &revision_of(text)[..8]);
    let _ = std::fs::write(dir.join(name), text);
}

fn save_data_url(state: &AppState, relative: &str, data: &str) -> Result<String, String> {
    let (_, encoded) = data.split_once(',').ok_or("图片数据无效")?;
    let bytes = BASE64.decode(encoded).map_err(|e| format!("图片解码失败: {e}"))?;
    let path = state.paths.data.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(format!("starlit://local/{relative}"))
}

fn draft_from_card(body: &Value) -> Value {
    let card = body.get("card").cloned().unwrap_or_else(|| json!({}));
    let data = card.get("data").cloned().unwrap_or_else(|| card.clone());
    let pick = |key: &str| data.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    let name = {
        let value = pick("name");
        if value.is_empty() { pick("nickname") } else { value }
    };
    let mut persona = pick("persona_md");
    if persona.is_empty() {
        let mut sections = Vec::new();
        for key in ["description", "personality", "scenario"] {
            let value = pick(key);
            if !value.is_empty() {
                sections.push(value);
            }
        }
        persona = sections.join("\n\n");
    }
    let summary = {
        let value = pick("summary");
        if value.is_empty() { pick("description") } else { value }
    };
    let aliases = data.get("aliases").cloned().unwrap_or_else(|| json!([]));
    let interests = data.get("interests").cloned().unwrap_or_else(|| json!([]));
    json!({
        "id": format!("draft-{}", chrono::Utc::now().timestamp_millis()),
        "revision": 1,
        "name": if name.is_empty() { "新角色".to_string() } else { name },
        "series": pick("series"),
        "aliases": aliases,
        "summary": summary,
        "persona_md": persona,
        "interests": interests,
        "avatar": body.get("avatar").cloned().unwrap_or(Value::Null),
    })
}

fn commit_draft(state: &AppState, body: &Value) -> Result<Value, String> {
    let draft_id = body.get("draft_id").and_then(Value::as_str).unwrap_or("");
    let draft = state
        .character_drafts
        .lock()
        .map_err(|e| e.to_string())?
        .get(draft_id)
        .cloned()
        .ok_or("草稿不存在或已过期")?;
    let name = draft.get("name").and_then(Value::as_str).unwrap_or("新角色").to_string();
    let mut characters = read_characters(state);
    let list = characters
        .get_mut("characters")
        .and_then(Value::as_array_mut)
        .ok_or("角色数据格式错误")?;
    let next_id = list
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str)?.parse::<u64>().ok())
        .max()
        .unwrap_or(3901000100)
        + 1;
    let id = next_id.to_string();
    list.push(json!({
        "id": id,
        "name": name,
        "slug": name,
        "series": draft.get("series").and_then(Value::as_str).unwrap_or(""),
        "source": "character-card",
    }));
    write_characters(state, &characters)?;

    let dir = persona_dir(state, &name);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let persona = draft.get("persona_md").and_then(Value::as_str).unwrap_or("");
    std::fs::write(dir.join("persona.md"), persona).map_err(|e| e.to_string())?;
    let interests = draft
        .get("interests")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n"))
        .unwrap_or_default();
    std::fs::write(dir.join("interests.md"), interests).map_err(|e| e.to_string())?;

    if let Some(avatar) = draft.get("avatar").and_then(Value::as_str) {
        let _ = save_data_url(state, &format!("avatars/{id}.png"), avatar);
    }
    let _ = state
        .character_drafts
        .lock()
        .map_err(|e| e.to_string())?
        .remove(draft_id);
    Ok(json!({ "ok": true, "character": { "id": id, "name": name,
        "series": draft.get("series").and_then(Value::as_str).unwrap_or("") } }))
}


fn groups_index_file(state: &AppState) -> PathBuf {
    state.paths.data.join("groups.json")
}

fn read_groups_index(state: &AppState) -> Value {
    std::fs::read_to_string(groups_index_file(state))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({ "groups": [] }))
}

fn write_groups_index(state: &AppState, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(groups_index_file(state), text).map_err(|e| e.to_string())
}

fn group_dir(state: &AppState, gid: &str) -> PathBuf {
    state.paths.data.join("groups").join(gid)
}

fn read_group_members(state: &AppState, gid: &str) -> Vec<String> {
    let path = group_dir(state, gid).join("members.json");
    let from_file = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| {
            value.get("members").and_then(Value::as_array).map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        item.as_str()
                            .map(str::to_string)
                            .or_else(|| item.get("id").and_then(Value::as_str).map(str::to_string))
                    })
                    .collect::<Vec<_>>()
            })
        });
    match from_file {
        Some(members) => members,
        None => characters(state)
            .into_iter()
            .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
            .collect(),
    }
}

fn read_member_records(state: &AppState, gid: &str) -> Vec<(String, String, i64)> {
    let path = group_dir(state, gid).join("members.json");
    let Some(items) = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("members").and_then(Value::as_array).cloned())
    else {
        return characters(state)
            .into_iter()
            .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
            .map(|id| (id, "member".to_string(), 1))
            .collect();
    };
    items
        .into_iter()
        .filter_map(|item| {
            let id = item.as_str().map(str::to_string)
                .or_else(|| item.get("id").and_then(Value::as_str).map(str::to_string))?;
            let role = item.get("role").and_then(Value::as_str).unwrap_or("member").to_string();
            let level = item.get("level").and_then(Value::as_i64).unwrap_or(1);
            Some((id, role, level))
        })
        .collect()
}

fn write_member_records(
    state: &AppState,
    gid: &str,
    members: &[(String, String, i64)],
) -> Result<(), String> {
    let dir = group_dir(state, gid);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let items: Vec<Value> = members
        .iter()
        .map(|(id, role, level)| json!({ "id": id, "role": role, "level": level }))
        .collect();
    let text = serde_json::to_string_pretty(&json!({ "members": items }))
        .map_err(|e| e.to_string())?;
    std::fs::write(dir.join("members.json"), text).map_err(|e| e.to_string())
}

fn write_group_members(state: &AppState, gid: &str, members: &[String]) -> Result<(), String> {
    let dir = group_dir(state, gid);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&json!({ "members": members }))
        .map_err(|e| e.to_string())?;
    std::fs::write(dir.join("members.json"), text).map_err(|e| e.to_string())
}

fn group_value(state: &AppState, gid: &str) -> Option<Value> {
    read_groups_index(state)
        .get("groups")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(gid))
                .cloned()
        })
}

fn update_group_value(state: &AppState, gid: &str, update: impl FnOnce(&mut Value)) -> Result<(), String> {
    let mut index = read_groups_index(state);
    if let Some(list) = index.get_mut("groups").and_then(Value::as_array_mut) {
        if let Some(item) = list
            .iter_mut()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(gid))
        {
            update(item);
        }
    }
    write_groups_index(state, &index)
}

fn read_group_requests_list(state: &AppState, gid: &str) -> Vec<Value> {
    std::fs::read_to_string(group_dir(state, gid).join("requests.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<Value>>(&text).ok())
        .unwrap_or_default()
}

fn read_group_requests(state: &AppState, gid: &str) -> Vec<Value> {
    read_group_requests_list(state, gid)
}

fn write_group_requests_list(state: &AppState, gid: &str, items: &[Value]) -> Result<(), String> {
    let dir = group_dir(state, gid);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("requests.json"),
        serde_json::to_string_pretty(items).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn group_detail_value(state: &AppState, gid: &str) -> Value {
    let cfg = state.config();
    let members: Vec<Value> = read_member_records(state, gid)
        .into_iter()
        .filter_map(|(id, role, level)| {
            let (name, _) = find_character(state, &id)?;
            Some(json!({
                "userId": id,
                "nickname": name,
                "role": role,
                "level": level,
                "is_me": false,
                "title": "",
            }))
        })
        .chain(std::iter::once(json!({
            "userId": cfg.qq.owner_id.parse::<i64>().unwrap_or(10001).to_string(),
            "nickname": cfg.qq.server.owner_nickname,
            "role": "owner",
            "level": 1,
            "is_me": true,
            "title": "",
        })))
        .collect();
    let group = group_value(state, gid).unwrap_or_else(|| json!({}));
    json!({
        "ok": true,
        "groupId": gid,
        "groupName": group.get("name").and_then(Value::as_str).unwrap_or("群聊"),
        "member_count": members.len(),
        "members": members,
        "mute_all": group.get("mute_all").and_then(Value::as_bool).unwrap_or(false),
        "searchable": group.get("searchable").and_then(Value::as_bool).unwrap_or(true),
        "announce": group.get("announce").cloned().unwrap_or(json!({})),
        "pending": read_group_requests(state, gid),
    })
}

fn group_files_index(state: &AppState, gid: &str) -> PathBuf {
    group_dir(state, gid).join("files.json")
}

fn read_group_files(state: &AppState, gid: &str) -> Vec<Value> {
    std::fs::read_to_string(group_files_index(state, gid))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<Value>>(&text).ok())
        .unwrap_or_default()
}

fn write_group_files(state: &AppState, gid: &str, files: &[Value]) -> Result<(), String> {
    let dir = group_dir(state, gid);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(
        group_files_index(state, gid),
        serde_json::to_string_pretty(files).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn query_params(url: &str) -> HashMap<String, String> {
    let Some((_, query)) = url.split_once('?') else {
        return HashMap::new();
    };
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| {
            (
                percent_encoding::percent_decode_str(key).decode_utf8_lossy().to_string(),
                percent_encoding::percent_decode_str(value).decode_utf8_lossy().to_string(),
            )
        })
        .collect()
}

fn touch_bond(state: &AppState, character_id: &str) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let inc_key = format!("bond.inc.{character_id}.{today}");
    let count = state
        .db
        .with(|conn| {
            conn.query_row(
                "SELECT value FROM kv WHERE key = ?1",
                rusqlite::params![inc_key],
                |row| row.get::<_, String>(0),
            )
        })
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    if count >= 3 {
        return;
    }
    let key = format!("bond.{character_id}");
    let mut value: Value = state
        .db
        .with(|conn| {
            conn.query_row(
                "SELECT value FROM kv WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get::<_, String>(0),
            )
        })
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({ "affection": 0, "since": 0, "bound": false }));
    let affection = value.get("affection").and_then(Value::as_i64).unwrap_or(0) + 1;
    value["affection"] = json!(affection.min(100));
    let now = chrono::Utc::now().timestamp();
    let _ = state.db.with(|conn| {
        conn.execute(
            "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![format!("bond.{character_id}"), serde_json::to_string(&value).unwrap_or_default(), now],
        )?;
        conn.execute(
            "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![inc_key, (count + 1).to_string(), now],
        )?;
        Ok(())
    });
}

fn mask_key(key: &str) -> String {
    let key = key.trim();
    if key.len() <= 4 {
        return "****".into();
    }
    format!("****{}", &key[key.len() - 4..])
}

fn settings_body(cfg: &crate::config::Config) -> Value {
    json!({
        "llm": {
            "base_url": cfg.llm.base_url,
            "model": cfg.llm.model,
            "reasoning_effort": cfg.llm.reasoning_effort,
            "api_key_set": !cfg.llm.api_key.trim().is_empty(),
            "api_key_hint": mask_key(&cfg.llm.api_key),
        },
        "setup": { "required": false },
    })
}

fn update_settings(state: &AppState, body: Option<String>) -> Result<Value, String> {
    let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
        .map_err(|e| format!("请求格式错误: {e}"))?;
    let mut cfg = state.config();
    if let Some(llm) = payload.get("llm") {
        if let Some(value) = llm.get("base_url").and_then(Value::as_str) {
            if !value.trim().is_empty() {
                cfg.llm.base_url = value.trim().to_string();
            }
        }
        if let Some(value) = llm.get("model").and_then(Value::as_str) {
            if !value.trim().is_empty() {
                cfg.llm.model = value.trim().to_string();
            }
        }
        if let Some(value) = llm.get("reasoning_effort").and_then(Value::as_str) {
            cfg.llm.reasoning_effort = value.trim().to_string();
        }
        if let Some(value) = llm.get("api_key").and_then(Value::as_str) {
            if !value.trim().is_empty() {
                cfg.llm.api_key = value.trim().to_string();
            }
        }
    }
    crate::paths::write_config(&state.paths.config_file(), &cfg)?;
    *state.config.write().map_err(|e| e.to_string())? = cfg.clone();
    Ok(settings_body(&cfg))
}


#[tauri::command]
pub async fn character_draft_chat(
    state: State<'_, AppState>,
    draft_id: String,
    revision: Option<i64>,
    text: String,
) -> Result<Value, String> {
    let _ = revision;
    let draft = {
        let drafts = state.character_drafts.lock().map_err(|e| e.to_string())?;
        drafts.get(&draft_id).cloned().ok_or("草稿不存在")?
    };
    let cfg = state.config();
    let name = draft.get("name").and_then(Value::as_str).unwrap_or("角色");
    let persona = draft.get("persona_md").and_then(Value::as_str).unwrap_or("");
    let mut trial = draft.get("trial").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut messages = vec![json!({
        "role": "system",
        "content": format!(
            "你是{name}。以下是你的人设：\n{persona}\n\n要求：以{name}的身份自然回复，口语化、简短；不要解释你是 AI。"
        ),
    })];
    for turn in &trial {
        let role = turn.get("role").and_then(Value::as_str).unwrap_or("assistant");
        let content = turn.get("text").and_then(Value::as_str).unwrap_or("");
        if !content.is_empty() {
            messages.push(json!({
                "role": if role == "user" { "user" } else { "assistant" },
                "content": content,
            }));
        }
    }
    messages.push(json!({ "role": "user", "content": text }));
    let reply = crate::llm::chat(&cfg, messages).await?;
    trial.push(json!({ "role": "user", "text": text }));
    trial.push(json!({ "role": "assistant", "text": reply.trim() }));
    if trial.len() > 20 {
        let drain = trial.len() - 20;
        trial.drain(0..drain);
    }
    if let Ok(mut drafts) = state.character_drafts.lock() {
        if let Some(item) = drafts.get_mut(&draft_id) {
            item["trial"] = json!(trial);
        }
    }
    Ok(json!({ "ok": true, "trial": trial }))
}

#[tauri::command]
pub async fn character_draft_generate(
    state: State<'_, AppState>,
    name: String,
    avatar: Option<String>,
) -> Result<Value, String> {
    let cfg = state.config();
    let messages = vec![json!({
        "role": "user",
        "content": format!(
            "请为角色「{name}」生成一份中文人设，使用 Markdown。包含：是谁、说话方式、性格、喜欢/讨厌、聊天习惯。\n\
             只输出人设正文，不要解释，不要编造现实人物隐私。"
        ),
    })];
    let persona = crate::llm::chat(&cfg, messages).await?;
    let draft = json!({
        "id": format!("draft-{}", chrono::Utc::now().timestamp_millis()),
        "revision": 1,
        "name": name,
        "series": "",
        "aliases": [],
        "summary": "",
        "persona_md": persona.trim(),
        "interests": [],
        "avatar": json!(avatar),
        "trial": [],
    });
    let id = draft.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    state.character_drafts.lock().map_err(|e| e.to_string())?.insert(id, draft.clone());
    Ok(json!({ "ok": true, "draft": draft }))
}

#[tauri::command]
pub async fn diagnostics_test(
    state: State<'_, AppState>,
    target: String,
) -> Result<Value, String> {
    if target != "llm" {
        return Ok(json!({ "ok": false, "error": "该目标已停用" }));
    }
    let cfg = state.config();
    let messages = vec![json!({ "role": "user", "content": "请回复 OK 两个字母。" })];
    match crate::llm::chat(&cfg, messages).await {
        Ok(text) => Ok(json!({
            "ok": true, "success": !text.trim().is_empty(),
            "message": format!("模型返回：{}", text.trim().chars().take(60).collect::<String>()),
        })),
        Err(error) => Ok(json!({
            "ok": true, "success": false, "message": format!("实测失败：{error}")
        })),
    }
}


fn add_zip_dir(
    zip: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>,
    root: &std::path::Path,
    prefix: &str,
) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let archive_path = format!("{prefix}/{name}");
        if path.is_dir() {
            add_zip_dir(zip, &path, &archive_path)?;
        } else if path.is_file() {
            use std::io::Write;
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            zip.start_file(archive_path, zip::write::SimpleFileOptions::default())
                .map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}


#[tauri::command]
pub async fn generate_diary(
    state: State<'_, AppState>,
    peer_id: String,
    day: String,
) -> Result<Value, String> {
    let date = chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .map_err(|_| "day must be YYYY-MM-DD".to_string())?;
    let start = date.and_hms_opt(0, 0, 0).ok_or("invalid day")?;
    let end = date.succ_opt().and_then(|d| d.and_hms_opt(0, 0, 0)).ok_or("invalid day")?;
    use chrono::TimeZone;
    let start = chrono::Local.from_local_datetime(&start).single()
        .map(|t| t.timestamp()).unwrap_or(0);
    let end = chrono::Local.from_local_datetime(&end).single()
        .map(|t| t.timestamp()).unwrap_or(start + 86400);
    let rows = state.db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT direction, text FROM messages
             WHERE conversation_kind = 'Private' AND conversation_id = ?1
               AND direction IN ('Incoming','Outgoing') AND sent_at >= ?2 AND sent_at < ?3
             ORDER BY sent_at ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![peer_id, start, end], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }).unwrap_or_default();
    if rows.is_empty() {
        return Err("diary requires user and assistant messages".into());
    }
    let messages_count = rows.len();
    let transcript = rows
        .iter()
        .map(|(direction, text)| {
            format!("{}: {text}", if direction == "Outgoing" { "老师" } else { "她" })
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (name, slug) = find_character(&state, &peer_id).unwrap_or(("角色".into(), peer_id.clone()));
    let persona = read_persona(&state, &slug);
    let cfg = state.config();
    let messages = vec![json!({
        "role": "user",
        "content": format!(
            "你是{name}。这是你和老师今天（{day}）的聊天记录：\n{transcript}\n\n\
             请以{name}的第一人称写一篇日记，150-300 字，口语、私密、自然；不要使用 Markdown。\n\
             你的人设参考：\n{persona}"
        ),
    })];
    let content = crate::llm::chat(&cfg, messages).await?;
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("模型没有返回日记".into());
    }
    let now = chrono::Utc::now().timestamp() as f64;
    let title = format!("{day} 的日记");
    state.db.with(|conn| {
        conn.execute(
            "INSERT INTO shared_diaries_v2
             (peer_id, day, title, content, source_turn_ids, message_count, generated_at)
             VALUES (?1, ?2, ?3, ?4, '[]', ?5, ?6)
             ON CONFLICT(peer_id, day) DO UPDATE SET
               title = excluded.title, content = excluded.content,
               message_count = excluded.message_count, generated_at = excluded.generated_at",
            rusqlite::params![peer_id, day, title, content, messages_count as i64, now],
        )?;
        Ok(())
    })?;
    Ok(json!({
        "ok": true,
        "item": { "day": day, "title": title, "content": content,
                  "message_count": messages_count, "generated_at": now },
    }))
}

#[tauri::command]
pub fn restore_backup(state: State<'_, AppState>, data: String) -> Result<Value, String> {
    let bytes = BASE64.decode(data).map_err(|e| format!("备份解码失败: {e}"))?;
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("备份格式无效: {e}"))?;
    let stamp = chrono::Utc::now().timestamp();
    let staging = state.paths.backups.join(format!("restore-{stamp}"));
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if name.starts_with('/') || name.contains("..") {
            continue;
        }
        let target = staging.join(&name);
        if entry.is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut file = std::fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
    }
    let marker = state.paths.root.join(".restore_pending");
    std::fs::write(&marker, staging.display().to_string()).map_err(|e| e.to_string())?;
    Ok(json!({
        "ok": true,
        "requiresRestart": true,
        "message": "备份已准备好，重启星伴后生效。",
    }))
}

#[tauri::command]
pub fn create_backup(state: State<'_, AppState>) -> Result<Value, String> {
    let _ = state.db.with(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    });
    let bytes = {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        use std::io::Write;

        for file in ["characters.json", "groups.json", "presence.json", "character_traits.json"] {
            let path = state.paths.data.join(file);
            if path.is_file() {
                let data = std::fs::read(&path).map_err(|e| e.to_string())?;
                zip.start_file(format!("data/{file}"), options).map_err(|e| e.to_string())?;
                zip.write_all(&data).map_err(|e| e.to_string())?;
            }
        }
        let db = state.paths.db_file();
        if db.is_file() {
            let data = std::fs::read(&db).map_err(|e| e.to_string())?;
            zip.start_file("data/companion.db", options).map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
        let mut config = state.config();
        config.llm.api_key.clear();
        let text = serde_yaml::to_string(&config).map_err(|e| e.to_string())?;
        zip.start_file("config.yaml", options).map_err(|e| e.to_string())?;
        zip.write_all(text.as_bytes()).map_err(|e| e.to_string())?;

        add_zip_dir(&mut zip, &state.paths.persona, "persona")?;
        let index = state.paths.data.join("stickers").join("index.json");
        if index.is_file() {
            let data = std::fs::read(&index).map_err(|e| e.to_string())?;
            zip.start_file("data/stickers/index.json", options).map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?.into_inner()
    };
    let filename = format!(
        "companion-backup-{}.zip",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    Ok(json!({
        "ok": true,
        "filename": filename,
        "size": bytes.len(),
        "data": BASE64.encode(bytes),
    }))
}

#[tauri::command]
pub fn list_memories(
    state: State<'_, AppState>,
    query: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<Value, String> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(50);
    let (items, total) = crate::memory::list(
        &state,
        &query.unwrap_or_default(),
        offset,
        limit,
    )?;
    Ok(serde_json::json!({
        "items": items,
        "total": total,
        "offset": offset,
        "limit": limit,
    }))
}

#[tauri::command]
pub fn create_memory(
    state: State<'_, AppState>,
    text: String,
    category: Option<String>,
    pinned: Option<bool>,
) -> Result<Value, String> {
    let id = crate::memory::create(
        &state,
        &text,
        &category.unwrap_or_else(|| "auto".into()),
        pinned.unwrap_or(false),
    )?;
    Ok(serde_json::json!({ "id": id }))
}

#[tauri::command]
pub fn update_memory(
    state: State<'_, AppState>,
    id: String,
    text: Option<String>,
    category: Option<String>,
    pinned: Option<bool>,
    corrected: Option<bool>,
) -> Result<(), String> {
    let id: i64 = id.parse().map_err(|_| "无效记忆 ID".to_string())?;
    crate::memory::update(
        &state,
        id,
        text.as_deref(),
        category.as_deref(),
        pinned,
        corrected.unwrap_or(false),
    )
}

#[tauri::command]
pub fn delete_memory(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let id: i64 = id.parse().map_err(|_| "无效记忆 ID".to_string())?;
    crate::memory::delete(&state, id)
}

#[tauri::command]
pub fn api_request(
    state: State<'_, AppState>,
    url: String,
    method: String,
    body: Option<String>,
) -> ApiOut {
    let path = url.split('?').next().unwrap_or("");


    if path == "/api/groups/detail" {
        let params = query_params(&url);
        let gid = params.get("groupId").or_else(|| params.get("group_id")).cloned().unwrap_or_default();
        return ApiOut { status: 200, body: group_detail_value(&state, &gid) };
    }

    if path == "/api/groups/create" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let name = payload.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if name.is_empty() {
            return ApiOut { status: 200, body: json!({ "ok": false, "error": "群名不能为空" }) };
        }
        let mut index = read_groups_index(&state);
        let list = index.get_mut("groups").and_then(Value::as_array_mut);
        let Some(list) = list else {
            return ApiOut { status: 500, body: json!({ "ok": false, "error": "群数据格式错误" }) };
        };
        let next = list
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_str)?.parse::<u64>().ok())
            .max()
            .unwrap_or(8800000)
            + 1;
        let gid = next.to_string();
        let mut members: Vec<String> = payload
            .get("members")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        item.as_u64().map(|v| v.to_string())
                            .or_else(|| item.as_str().map(str::to_string))
                    })
                    .collect()
            })
            .unwrap_or_default();
        members.sort();
        members.dedup();
        list.push(json!({
            "id": gid,
            "name": name,
            "avatar": "",
            "created_ts": chrono::Utc::now().timestamp(),
            "mute_all": false,
            "searchable": true,
        }));
        if let Err(error) = write_groups_index(&state, &index) {
            return ApiOut { status: 500, body: json!({ "ok": false, "error": error }) };
        }
        let _ = write_group_members(&state, &gid, &members);
        return ApiOut { status: 200, body: group_detail_value(&state, &gid) };
    }

    if path == "/api/groups/manage" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let op = payload.get("op").and_then(Value::as_str).unwrap_or("");
        let gid = payload.get("groupId").and_then(Value::as_str).unwrap_or("");
        if gid.is_empty() {
            return ApiOut { status: 200, body: json!({ "ok": false, "error": "缺少群号" }) };
        }
        match op {
            "rename" => {
                let name = payload.get("name").and_then(Value::as_str).unwrap_or("").trim();
                let _ = update_group_value(&state, gid, |item| item["name"] = json!(name));
            }
            "mute" => {
                let on = payload.get("on").and_then(Value::as_bool).unwrap_or(false);
                let _ = update_group_value(&state, gid, |item| item["mute_all"] = json!(on));
            }
            "searchable" => {
                let on = payload.get("on").and_then(Value::as_bool).unwrap_or(true);
                let _ = update_group_value(&state, gid, |item| item["searchable"] = json!(on));
            }
            "announce" => {
                let text = payload.get("text").and_then(Value::as_str).unwrap_or("");
                let _ = update_group_value(&state, gid, |item| {
                    item["announce"] = json!({
                        "text": text,
                        "ts": chrono::Utc::now().timestamp(),
                    })
                });
            }
            "invite" => {
                if let Some(user) = payload.get("userId").and_then(Value::as_str) {
                    let mut records = read_member_records(&state, gid);
                    if !records.iter().any(|(id, ..)| id == user) {
                        records.push((user.to_string(), "member".into(), 1));
                        let _ = write_member_records(&state, gid, &records);
                    }
                }
            }
            "admin" => {
                let user = payload.get("userId").and_then(Value::as_str).unwrap_or("");
                let on = payload.get("on").and_then(Value::as_bool).unwrap_or(false);
                let mut records = read_member_records(&state, gid);
                for item in records.iter_mut() {
                    if item.0 == user {
                        item.1 = if on { "admin".into() } else { "member".into() };
                    }
                }
                let _ = write_member_records(&state, gid, &records);
            }
            "level" => {
                let user = payload.get("userId").and_then(Value::as_str).unwrap_or("");
                let level = payload.get("level").and_then(Value::as_i64).unwrap_or(1).clamp(1, 100);
                let mut records = read_member_records(&state, gid);
                for item in records.iter_mut() {
                    if item.0 == user {
                        item.2 = level;
                    }
                }
                let _ = write_member_records(&state, gid, &records);
            }
            "resolve" => {
                let user = payload.get("userId").and_then(Value::as_str).unwrap_or("");
                let approve = payload.get("approve").and_then(Value::as_bool).unwrap_or(false);
                let mut requests = read_group_requests_list(&state, gid);
                requests.retain(|item| item.get("userId").and_then(Value::as_str) != Some(user));
                let _ = write_group_requests_list(&state, gid, &requests);
                if approve && !user.is_empty() {
                    let mut records = read_member_records(&state, gid);
                    if !records.iter().any(|(id, ..)| id == user) {
                        records.push((user.to_string(), "member".into(), 1));
                        let _ = write_member_records(&state, gid, &records);
                    }
                }
            }
            "request" => {
                let user = payload.get("userId").and_then(Value::as_str).unwrap_or("");
                let name = payload.get("name").and_then(Value::as_str).unwrap_or(user);
                let note = payload.get("note").and_then(Value::as_str).unwrap_or("想加入这个群");
                if !user.is_empty() {
                    let mut requests = read_group_requests_list(&state, gid);
                    if !requests.iter().any(|item| item.get("userId").and_then(Value::as_str) == Some(user)) {
                        requests.push(json!({
                            "userId": user,
                            "name": name,
                            "note": note,
                            "ts": chrono::Utc::now().timestamp(),
                        }));
                        let _ = write_group_requests_list(&state, gid, &requests);
                    }
                }
            }
            "kick" => {
                if let Some(user) = payload.get("userId").and_then(Value::as_str) {
                    let mut records = read_member_records(&state, gid);
                    records.retain(|(id, ..)| id != user);
                    let _ = write_member_records(&state, gid, &records);
                }
            }
            "delete" => {
                let mut index = read_groups_index(&state);
                if let Some(list) = index.get_mut("groups").and_then(Value::as_array_mut) {
                    list.retain(|item| item.get("id").and_then(Value::as_str) != Some(gid));
                }
                let _ = write_groups_index(&state, &index);
                let src = group_dir(&state, gid);
                let dst = state.paths.backups.join("deleted_groups").join(gid);
                if let Some(parent) = dst.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::rename(src, dst);
                return ApiOut { status: 200, body: json!({ "ok": true, "deleted": true }) };
            }
            _ => {}
        }
        return ApiOut { status: 200, body: group_detail_value(&state, gid) };
    }

    if path == "/api/groups/upload" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let gid = payload.get("groupId").and_then(Value::as_str).unwrap_or("");
        let name = payload.get("name").and_then(Value::as_str).unwrap_or("file");
        let data = payload.get("data").and_then(Value::as_str).unwrap_or("");
        if gid.is_empty() || data.is_empty() {
            return ApiOut { status: 200, body: json!({ "ok": false, "error": "缺少群号或文件数据" }) };
        }
        let (_, encoded) = data.split_once(',').unwrap_or(("", data));
        let Ok(bytes) = BASE64.decode(encoded) else {
            return ApiOut { status: 200, body: json!({ "ok": false, "error": "文件解码失败" }) };
        };
        let safe_name = name.rsplit('/').next().unwrap_or("file").replace(['/', '\\'], "_");
        let stamp = chrono::Utc::now().timestamp_millis();
        let stored = format!("{stamp}-{safe_name}");
        let dir = group_dir(&state, gid).join("files");
        let _ = std::fs::create_dir_all(&dir);
        if let Err(error) = std::fs::write(dir.join(&stored), &bytes) {
            return ApiOut { status: 500, body: json!({ "ok": false, "error": error.to_string() }) };
        }
        let mut files = read_group_files(&state, gid);
        let id = stamp.to_string();
        files.push(json!({
            "id": id,
            "name": safe_name,
            "size": bytes.len(),
            "ts": chrono::Utc::now().timestamp(),
            "stored": stored,
            "url": format!("starlit://local/groups/{gid}/files/{stored}"),
        }));
        let _ = write_group_files(&state, gid, &files);
        return ApiOut { status: 200, body: json!({ "ok": true, "file_id": id }) };
    }

    if path == "/api/groups/files" {
        let params = query_params(&url);
        let gid = params.get("groupId").cloned().unwrap_or_default();
        let files = read_group_files(&state, &gid);
        return ApiOut { status: 200, body: json!({ "ok": true, "files": files }) };
    }

    if path == "/api/groups/files/delete" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let gid = payload.get("groupId").and_then(Value::as_str).unwrap_or("");
        let file_id = payload.get("file_id").and_then(Value::as_str).unwrap_or("");
        let mut files = read_group_files(&state, gid);
        if let Some(pos) = files.iter().position(|item| {
            item.get("id").and_then(Value::as_str) == Some(file_id)
        }) {
            let file = files.remove(pos);
            if let Some(stored) = file.get("stored").and_then(Value::as_str) {
                let _ = std::fs::remove_file(group_dir(&state, gid).join("files").join(stored));
            }
            let _ = write_group_files(&state, gid, &files);
        }
        return ApiOut { status: 200, body: json!({ "ok": true }) };
    }


    if path == "/api/conversation/clear" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let peer = payload.get("peer_id").and_then(Value::as_str).unwrap_or("");
        if peer.is_empty() {
            return ApiOut { status: 200, body: json!({ "ok": false, "error": "缺少角色" }) };
        }
        let _ = state.db.with(|conn| {
            conn.execute(
                "DELETE FROM messages WHERE conversation_kind = 'Private' AND conversation_id = ?1",
                rusqlite::params![peer],
            )?;
            Ok(())
        });
        return ApiOut { status: 200, body: json!({ "ok": true }) };
    }

    if path == "/api/profile" {
        let mut cfg = state.config();
        if method == "POST" {
            let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
                .unwrap_or_else(|_| json!({}));
            if let Some(nickname) = payload.get("nickname").and_then(Value::as_str) {
                if !nickname.trim().is_empty() {
                    cfg.qq.server.owner_nickname = nickname.trim().to_string();
                    if let Err(error) = crate::paths::write_config(&state.paths.config_file(), &cfg) {
                        return ApiOut { status: 500, body: json!({ "ok": false, "error": error }) };
                    }
                    if let Ok(mut guard) = state.config.write() {
                        *guard = cfg.clone();
                    }
                }
            }
        }
        return ApiOut { status: 200, body: json!({
            "ok": true,
            "profile": { "nickname": cfg.qq.server.owner_nickname },
            "owner": { "nickname": cfg.qq.server.owner_nickname },
        })};
    }

    if path == "/api/avatar" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let peer = payload.get("peer_id").and_then(Value::as_str).unwrap_or("");
        let data = payload.get("data").and_then(Value::as_str).unwrap_or("");
        let body = match safe_segment(peer) {
            Some(peer) => match save_data_url(&state, &format!("avatars/{peer}.png"), data) {
                Ok(url) => json!({ "ok": true, "url": url }),
                Err(error) => json!({ "ok": false, "error": error }),
            },
            None => json!({ "ok": false, "error": "无效的头像标识" }),
        };
        return ApiOut { status: 200, body };
    }

    if path == "/api/avatar/delete" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let peer = payload.get("peer_id").and_then(Value::as_str).unwrap_or("");
        if let Some(peer) = safe_segment(peer) {
            let target = state.paths.data.join("avatars").join(format!("{peer}.png"));
            let _ = std::fs::remove_file(target);
        }
        return ApiOut { status: 200, body: json!({ "ok": true }) };
    }

    if path == "/api/character-card" {
        let params = query_params(&url);
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let peer = payload.get("peer_id").and_then(Value::as_str)
            .or_else(|| params.get("peer_id").map(String::as_str))
            .unwrap_or("");
        let Some((name, slug)) = find_character(&state, peer) else {
            return ApiOut { status: 404, body: json!({ "ok": false, "error": "角色不存在" }) };
        };
        let current = read_persona(&state, &slug);
        if method == "PUT" {
            let Some(expected) = payload.get("revision").and_then(Value::as_str) else {
                return ApiOut { status: 400, body: json!({ "ok": false, "error": "缺少 revision" }) };
            };
            if !current.is_empty() && expected != revision_of(&current) {
                return ApiOut { status: 409, body: json!({ "ok": false, "error": "人设已被修改" }) };
            }
            let next = payload.get("persona_md").and_then(Value::as_str).unwrap_or("");
            backup_persona(&state, &slug, &current);
            let dir = persona_dir(&state, &slug);
            let _ = std::fs::create_dir_all(&dir);
            if let Err(error) = std::fs::write(dir.join("persona.md"), next) {
                return ApiOut { status: 500, body: json!({ "ok": false, "error": error.to_string() }) };
            }
        }
        let text = read_persona(&state, &slug);
        return ApiOut { status: 200, body: json!({
            "ok": true,
            "card": {
                "name": name,
                "persona_md": text,
                "revision": revision_of(&text),
                "versions": persona_versions(&state, &slug),
            }
        })};
    }

    if path == "/api/character-card/restore" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let peer = payload.get("peer_id").and_then(Value::as_str).unwrap_or("");
        let Some((name, slug)) = find_character(&state, peer) else {
            return ApiOut { status: 404, body: json!({ "ok": false, "error": "角色不存在" }) };
        };
        let current = read_persona(&state, &slug);
        if let Some(expected) = payload.get("revision").and_then(Value::as_str) {
            if !current.is_empty() && expected != revision_of(&current) {
                return ApiOut { status: 409, body: json!({ "ok": false, "error": "人设已被修改" }) };
            }
        }
        let version = payload.get("version_id").and_then(Value::as_str).unwrap_or("");
        let version = safe_segment(version).unwrap_or_default();
        let source = state.paths.persona.join(".versions").join(&slug).join(&version);
        let Ok(restored) = std::fs::read_to_string(&source) else {
            return ApiOut { status: 404, body: json!({ "ok": false, "error": "版本不存在" }) };
        };
        backup_persona(&state, &slug, &current);
        let dir = persona_dir(&state, &slug);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("persona.md"), &restored);
        return ApiOut { status: 200, body: json!({
            "ok": true,
            "card": {
                "name": name,
                "persona_md": restored,
                "revision": revision_of(&restored),
                "versions": persona_versions(&state, &slug),
            }
        })};
    }

    if path == "/api/character-drafts/import" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let draft = draft_from_card(&payload);
        let id = draft.get("id").and_then(Value::as_str).unwrap_or("").to_string();
        match state.character_drafts.lock() {
            Ok(mut drafts) => { drafts.insert(id, draft.clone()); }
            Err(error) => return ApiOut { status: 500, body: json!({ "ok": false, "error": error.to_string() }) },
        }
        return ApiOut { status: 200, body: json!({ "ok": true, "draft": draft }) };
    }

    if path == "/api/character-drafts" && method == "PATCH" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let id = payload.get("draft_id").and_then(Value::as_str).unwrap_or("").to_string();
        let Ok(mut drafts) = state.character_drafts.lock() else {
            return ApiOut { status: 500, body: json!({ "ok": false, "error": "草稿锁不可用" }) };
        };
        let Some(draft) = drafts.get_mut(&id) else {
            return ApiOut { status: 404, body: json!({ "ok": false, "error": "草稿不存在" }) };
        };
        for key in ["name", "series", "summary", "persona_md", "avatar"] {
            if let Some(value) = payload.get(key) {
                draft[key] = value.clone();
            }
        }
        for key in ["aliases", "interests"] {
            if let Some(value) = payload.get(key) {
                draft[key] = value.clone();
            }
        }
        let revision = draft.get("revision").and_then(Value::as_i64).unwrap_or(1) + 1;
        draft["revision"] = json!(revision);
        let draft = draft.clone();
        return ApiOut { status: 200, body: json!({ "ok": true, "draft": draft }) };
    }

    if path == "/api/character-drafts/commit" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let body = match commit_draft(&state, &payload) {
            Ok(body) => body,
            Err(error) => json!({ "ok": false, "error": error }),
        };
        return ApiOut { status: 200, body };
    }

    if path == "/api/character-drafts/chat" || path == "/api/character-drafts/generate" {
        return ApiOut { status: 200, body: json!({
            "ok": false,
            "error": "试聊与 AI 生成暂未实现，请直接编辑人设后添加。",
        })};
    }

    if path == "/api/characters/delete" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let peer = payload.get("peer_id").and_then(Value::as_str).unwrap_or("");
        let Some((name, slug)) = find_character(&state, peer) else {
            return ApiOut { status: 404, body: json!({ "ok": false, "error": "角色不存在" }) };
        };
        let mut characters = read_characters(&state);
        if let Some(list) = characters.get_mut("characters").and_then(Value::as_array_mut) {
            list.retain(|item| item.get("id").and_then(Value::as_str) != Some(peer));
        }
        let _ = write_characters(&state, &characters);
        let src = persona_dir(&state, &slug);
        let dst = state.paths.backups.join(format!(
            "deleted-{}-{slug}",
            chrono::Utc::now().timestamp()
        ));
        let _ = std::fs::rename(src, dst);
        let _ = name;
        return ApiOut { status: 200, body: json!({ "ok": true }) };
    }



    if path == "/api/shared-life" {
        let params = query_params(&url);
        let peer = params.get("peer_id").cloned().unwrap_or_default();
        let anniversaries = state.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, date, yearly, note, created_at, updated_at
                 FROM shared_anniversaries WHERE peer_id = ?1 ORDER BY date DESC",
            )?;
            let items = stmt.query_map(rusqlite::params![peer], |row| {
                Ok(json!({
                    "id": row.get::<_, i64>(0)?.to_string(),
                    "title": row.get::<_, String>(1)?,
                    "date": row.get::<_, String>(2)?,
                    "yearly": row.get::<_, i64>(3).unwrap_or(0) > 0,
                    "note": row.get::<_, String>(4)?,
                    "created_at": row.get::<_, f64>(5).unwrap_or(0.0),
                    "updated_at": row.get::<_, f64>(6).unwrap_or(0.0),
                }))
            })?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(items)
        }).unwrap_or_default();
        let agreements = state.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, details, due_at, status FROM shared_agreements
                 WHERE peer_id = ?1 ORDER BY due_at ASC",
            )?;
            let items = stmt.query_map(rusqlite::params![peer], |row| {
                Ok(json!({
                    "id": row.get::<_, i64>(0)?.to_string(),
                    "title": row.get::<_, String>(1)?,
                    "details": row.get::<_, String>(2)?,
                    "due_at": row.get::<_, f64>(3).unwrap_or(0.0),
                    "status": row.get::<_, String>(4)?,
                }))
            })?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(items)
        }).unwrap_or_default();
        let diaries = state.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT day, title, content, message_count, generated_at
                 FROM shared_diaries_v2 WHERE peer_id = ?1 ORDER BY day DESC",
            )?;
            let items = stmt.query_map(rusqlite::params![peer], |row| {
                Ok(json!({
                    "day": row.get::<_, String>(0)?,
                    "title": row.get::<_, String>(1)?,
                    "content": row.get::<_, String>(2)?,
                    "message_count": row.get::<_, i64>(3).unwrap_or(0),
                    "generated_at": row.get::<_, f64>(4).unwrap_or(0.0),
                }))
            })?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(items)
        }).unwrap_or_default();
        return ApiOut { status: 200, body: json!({
            "ok": true, "peer_id": peer,
            "anniversaries": anniversaries,
            "agreements": agreements,
            "diaries": diaries,
            "overview": {},
        })};
    }

    if path == "/api/shared-life/anniversaries" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let peer = payload.get("peer_id").and_then(Value::as_str).unwrap_or("");
        let now = chrono::Utc::now().timestamp() as f64;
        if method == "DELETE" {
            let id = payload.get("id").and_then(Value::as_str).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let _ = state.db.with(|conn| {
                conn.execute("DELETE FROM shared_anniversaries WHERE id = ?1 AND peer_id = ?2",
                    rusqlite::params![id, peer])?;
                Ok(())
            });
            return ApiOut { status: 200, body: json!({ "ok": true }) };
        }
        let title = payload.get("title").and_then(Value::as_str).unwrap_or("").trim();
        let date = payload.get("date").and_then(Value::as_str).unwrap_or("");
        let yearly = payload.get("yearly").and_then(Value::as_bool).unwrap_or(false);
        let note = payload.get("note").and_then(Value::as_str).unwrap_or("");
        if title.is_empty() || date.is_empty() {
            return ApiOut { status: 200, body: json!({ "ok": false, "error": "标题和日期不能为空" }) };
        }
        let id = payload.get("id").and_then(Value::as_str).and_then(|v| v.parse::<i64>().ok());
        let item_id = match id {
            Some(id) => {
                let _ = state.db.with(|conn| {
                    conn.execute(
                        "UPDATE shared_anniversaries SET title = ?1, date = ?2, yearly = ?3,
                         note = ?4, updated_at = ?5 WHERE id = ?6 AND peer_id = ?7",
                        rusqlite::params![title, date, yearly as i64, note, now, id, peer],
                    )?;
                    Ok(())
                });
                id
            }
            None => state.db.with(|conn| {
                conn.execute(
                    "INSERT INTO shared_anniversaries
                     (title, date, yearly, note, created_at, updated_at, peer_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
                    rusqlite::params![title, date, yearly as i64, note, now, peer],
                )?;
                Ok(conn.last_insert_rowid())
            }).unwrap_or(0),
        };
        return ApiOut { status: 200, body: json!({
            "ok": true,
            "item": { "id": item_id.to_string(), "title": title, "date": date,
                      "yearly": yearly, "note": note, "created_at": now, "updated_at": now },
        })};
    }

    if path == "/api/shared-life/agreements" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let peer = payload.get("peer_id").and_then(Value::as_str).unwrap_or("");
        if method == "DELETE" {
            let id = payload.get("id").and_then(Value::as_str).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let _ = state.db.with(|conn| {
                conn.execute("DELETE FROM shared_agreements WHERE id = ?1 AND peer_id = ?2",
                    rusqlite::params![id, peer])?;
                Ok(())
            });
            return ApiOut { status: 200, body: json!({ "ok": true }) };
        }
        let title = payload.get("title").and_then(Value::as_str).unwrap_or("").trim();
        let details = payload.get("details").and_then(Value::as_str).unwrap_or("");
        let due_at = payload.get("due_at").and_then(Value::as_f64).unwrap_or(0.0);
        if title.is_empty() {
            return ApiOut { status: 200, body: json!({ "ok": false, "error": "标题不能为空" }) };
        }
        let id = payload.get("id").and_then(Value::as_str).and_then(|v| v.parse::<i64>().ok());
        let item_id = match id {
            Some(id) => {
                let _ = state.db.with(|conn| {
                    conn.execute(
                        "UPDATE shared_agreements SET title = ?1, details = ?2, due_at = ?3,
                         status = 'pending' WHERE id = ?4 AND peer_id = ?5",
                        rusqlite::params![title, details, due_at, id, peer],
                    )?;
                    Ok(())
                });
                id
            }
            None => state.db.with(|conn| {
                conn.execute(
                    "INSERT INTO shared_agreements (title, details, due_at, status, peer_id)
                     VALUES (?1, ?2, ?3, 'pending', ?4)",
                    rusqlite::params![title, details, due_at, peer],
                )?;
                Ok(conn.last_insert_rowid())
            }).unwrap_or(0),
        };
        return ApiOut { status: 200, body: json!({
            "ok": true,
            "item": { "id": item_id.to_string(), "title": title, "details": details,
                      "due_at": due_at, "status": "pending" },
        })};
    }

    if path == "/api/shared-life/agreements/confirm" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let peer = payload.get("peer_id").and_then(Value::as_str).unwrap_or("");
        let id = payload.get("id").and_then(Value::as_str).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        let _ = state.db.with(|conn| {
            conn.execute(
                "UPDATE shared_agreements SET status = 'confirmed' WHERE id = ?1 AND peer_id = ?2",
                rusqlite::params![id, peer],
            )?;
            Ok(())
        });
        return ApiOut { status: 200, body: json!({ "ok": true }) };
    }

    if path == "/api/shared-life/diaries" && method == "DELETE" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let peer = payload.get("peer_id").and_then(Value::as_str).unwrap_or("");
        let day = payload.get("day").and_then(Value::as_str).unwrap_or("");
        let _ = state.db.with(|conn| {
            conn.execute(
                "DELETE FROM shared_diaries_v2 WHERE peer_id = ?1 AND day = ?2",
                rusqlite::params![peer, day],
            )?;
            Ok(())
        });
        return ApiOut { status: 200, body: json!({ "ok": true }) };
    }

    if path == "/api/history/search" {
        let params = query_params(&url);
        let query = params.get("query").cloned().unwrap_or_default();
        let limit = params.get("limit").and_then(|v| v.parse::<i64>().ok()).unwrap_or(50).clamp(1, 200);
        let before = params.get("before_id").and_then(|v| v.parse::<i64>().ok());
        let kind;
        let id;
        if let Some(group) = params.get("group_id") {
            kind = "Group";
            id = group.clone();
        } else {
            kind = "Private";
            id = params.get("peer_id").cloned().unwrap_or_default();
        }
        let rows = state.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, direction, author_id, author_name, parts, text, reply_to,
                        sent_at, client_id, status
                 FROM messages
                 WHERE conversation_kind = ?1 AND conversation_id = ?2
                   AND text LIKE ?3 AND (?4 IS NULL OR id < ?4)
                 ORDER BY id DESC LIMIT ?5",
            )?;
            let pattern = format!("%{query}%");
            let items = stmt
                .query_map(rusqlite::params![kind, id, pattern, before, limit + 1], |row| {
                    let parts_text: String = row.get(4)?;
                    Ok(Message {
                        id: row.get(0)?,
                        conversation: Conversation { kind: kind.to_string(), id: id.clone() },
                        direction: row.get(1)?,
                        author: Author {
                            id: row.get(2)?,
                            name: row.get(3)?,
                            role: String::new(),
                        },
                        parts: serde_json::from_str(&parts_text).unwrap_or_else(|_| json!([])),
                        text: row.get(5)?,
                        reply_to: row.get(6)?,
                        reply_preview: None,
                        sent_at: row.get(7)?,
                        client_id: row.get(8)?,
                        status: row.get(9)?,
                        system: false,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(items)
        }).unwrap_or_default();
        let has_more = rows.len() as i64 > limit;
        let mut rows = rows;
        if has_more {
            rows.truncate(limit as usize);
        }
        let next = rows.last().map(|m| m.id);
        return ApiOut { status: 200, body: json!({
            "ok": true, "messages": rows, "has_more": has_more,
            "next_before_id": next,
        })};
    }

    if path == "/api/character-traits" {
        let params = query_params(&url);
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let peer = payload.get("peer_id").and_then(Value::as_str)
            .or_else(|| params.get("peer_id").map(String::as_str))
            .unwrap_or("");
        let key = format!("traits.{peer}");
        let mut traits = state.db.with(|conn| {
            conn.query_row("SELECT value FROM kv WHERE key = ?1", rusqlite::params![key], |row| {
                row.get::<_, String>(0)
            })
        }).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok())
          .unwrap_or_else(|| json!({
              "enthusiasm": 50, "affection": 60, "initiative_scale": 1.0,
              "quiet_until": 0, "low_pressure": false,
          }));
        if method == "POST" {
            if let Some(obj) = payload.as_object() {
                for (k, v) in obj {
                    if k != "peer_id" {
                        traits[k] = v.clone();
                    }
                }
            }
            let now = chrono::Utc::now().timestamp();
            let _ = state.db.with(|conn| {
                conn.execute(
                    "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                    rusqlite::params![format!("traits.{peer}"), serde_json::to_string(&traits).unwrap_or_default(), now],
                )?;
                Ok(())
            });
        }
        traits["ok"] = json!(true);
        traits["voice"] = json!({ "enabled": false });
        return ApiOut { status: 200, body: traits };
    }

    if path == "/api/bond" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let mut allow_multiple = state.db.with(|conn| {
            conn.query_row("SELECT value FROM kv WHERE key = 'bond.allow_multiple'", [], |row| {
                row.get::<_, String>(0)
            })
        }).ok().as_deref() == Some("1");
        if method == "POST" {
            match payload.get("action").and_then(Value::as_str).unwrap_or("") {
                "set_multiple" => {
                    allow_multiple = payload.get("enabled").and_then(Value::as_bool).unwrap_or(false);
                    let now = chrono::Utc::now().timestamp();
                    let _ = state.db.with(|conn| {
                        conn.execute(
                            "INSERT INTO kv (key, value, updated_at) VALUES ('bond.allow_multiple', ?1, ?2)
                             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                            rusqlite::params![if allow_multiple { "1" } else { "0" }, now],
                        )?;
                        Ok(())
                    });
                }
                "bind" | "unbind" => {
                    if let Some(cid) = payload.get("character_id").and_then(Value::as_str) {
                        let key = format!("bond.{cid}");
                        let now = chrono::Utc::now().timestamp();
                        let mut value: Value = state.db.with(|conn| {
                            conn.query_row("SELECT value FROM kv WHERE key = ?1", rusqlite::params![key], |row| {
                                row.get::<_, String>(0)
                            })
                        }).ok().and_then(|text| serde_json::from_str(&text).ok())
                          .unwrap_or_else(|| json!({ "affection": 0, "since": 0, "bound": false }));
                        value["bound"] = json!(payload.get("action").and_then(Value::as_str) == Some("bind"));
                        if value["bound"] == json!(true) {
                            value["since"] = json!(now);
                        }
                        let _ = state.db.with(|conn| {
                            conn.execute(
                                "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?3)
                                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                                rusqlite::params![format!("bond.{cid}"), serde_json::to_string(&value).unwrap_or_default(), now],
                            )?;
                            Ok(())
                        });
                    }
                }
                _ => {}
            }
        }
        let mut partners = Vec::new();
        for (id, name) in characters(&state).into_iter().filter_map(|c| {
            Some((c.get("id")?.as_str()?.to_string(), c.get("name")?.as_str()?.to_string()))
        }) {
            let key = format!("bond.{id}");
            let value: Option<Value> = state.db.with(|conn| {
                conn.query_row("SELECT value FROM kv WHERE key = ?1", rusqlite::params![key], |row| {
                    row.get::<_, String>(0)
                })
            }).ok().and_then(|text| serde_json::from_str(&text).ok());
            let Some(value) = value else { continue };
            let bound = value.get("bound").and_then(Value::as_bool).unwrap_or(false);
            let affection = value.get("affection").and_then(Value::as_i64).unwrap_or(0);
            if !bound && affection <= 0 {
                continue;
            }
            let since = value.get("since").and_then(Value::as_i64).unwrap_or(0);
            let days = if bound && since > 0 {
                ((chrono::Utc::now().timestamp() - since) / 86400).max(1)
            } else {
                0
            };
            partners.push(json!({
                "character_id": id,
                "name": name,
                "affection": affection,
                "bound": bound,
                "days": days,
            }));
        }
        let active = partners.iter().any(|p| p.get("bound") == Some(&json!(true)));
        return ApiOut { status: 200, body: json!({
            "ok": true,
            "bond": { "active": active, "allow_multiple": allow_multiple, "partners": partners },
            "candidates": [],
        })};
    }

    if path == "/api/diagnostics" {
        let key_set = !state.config().llm.api_key.trim().is_empty();
        let (archives, _) = crate::media::cache_stats(&state.paths);
        let count: i64 = state.db.with(|conn| {
            conn.query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
        }).unwrap_or(0);
        let memories: i64 = state.db.with(|conn| {
            conn.query_row("SELECT COUNT(*) FROM memories WHERE status = 'active'", [], |row| row.get(0))
        }).unwrap_or(0);
        return ApiOut { status: 200, body: json!({
            "ok": true,
            "setup": { "required": false },
            "checks": [
                { "id": "db", "label": "本地数据库", "status": "ok",
                  "summary": format!("{count} 条消息 · {memories} 条有效记忆"),
                  "details": [state.paths.db_file().display().to_string()] },
                { "id": "llm", "label": "模型配置", "status": if key_set { "ok" } else { "warning" },
                  "summary": if key_set { "API Key 已配置" } else { "尚未配置 API Key" },
                  "details": [format!("{} · {}", state.config().llm.base_url, state.config().llm.model)] },
                { "id": "media", "label": "媒体缓存", "status": if archives > 0 { "ok" } else { "warning" },
                  "summary": format!("{archives} 个资源包已解压"),
                  "details": [] },
            ],
        })};
    }

    if path == "/api/diagnostics/test" && method == "POST" {
        let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let target = payload.get("target").and_then(Value::as_str).unwrap_or("");
        if target != "llm" {
            return ApiOut { status: 200, body: json!({ "ok": false, "error": "该目标已停用" }) };
        }
        let cfg = state.config();
        let configured = !cfg.llm.api_key.trim().is_empty();
        return ApiOut { status: 200, body: json!({
            "ok": true,
            "success": configured,
            "message": if configured {
                "模型配置已就绪；聊天回复会实际请求该接口。"
            } else {
                "尚未配置 API Key。"
            },
        })};
    }

    if path == "/api/settings" {
        if method == "POST" {
            return ApiOut {
                status: 200,
                body: match update_settings(&state, body) {
                    Ok(body) => body,
                    Err(error) => json!({ "ok": false, "error": error }),
                },
            };
        }
        return ApiOut {
            status: 200,
            body: ok(settings_body(&state.config())),
        };
    }
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
        "/api/stickers" => json!({ "groups": sticker_groups(&state) }),
        "/api/stickers/upload" => return ApiOut {
            status: 200,
            body: match upload_sticker(&state, body) {
                Ok(value) => value,
                Err(error) => json!({ "ok": false, "error": error }),
            },
        },
        "/api/stickers/delete" => return ApiOut {
            status: 200,
            body: match delete_sticker(&state, body) {
                Ok(value) => value,
                Err(error) => json!({ "ok": false, "error": error }),
            },
        },
        "/api/sticker-category/create" => {
            let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}")).unwrap_or_default();
            let name = payload.get("name").and_then(Value::as_str).unwrap_or("");
            match safe_segment(name) {
                Some(key) => {
                    std::fs::create_dir_all(state.paths.data.join("stickers").join(&key))
                        .map_err(|e| e.to_string())
                        .ok();
                    json!({ "ok": true, "name": key, "key": key })
                }
                None => json!({ "ok": false, "error": "分类名称无效" }),
            }
        }
        "/api/sticker-category/delete" => {
            let payload: Value = serde_json::from_str(body.as_deref().unwrap_or("{}")).unwrap_or_default();
            let key = payload.get("key").and_then(Value::as_str).unwrap_or("");
            match safe_segment(key) {
                Some(key) => {
                    let dir = state.paths.data.join("stickers").join(&key);
                    let _ = std::fs::remove_dir_all(&dir);
                    let prefix = format!("{key}/");
                    let index: Vec<Value> = read_sticker_index(&state)
                        .into_iter()
                        .filter(|entry| {
                            entry.get("file").and_then(Value::as_str)
                                .map(|file| !file.starts_with(&prefix))
                                .unwrap_or(true)
                        })
                        .collect();
                    let _ = write_sticker_index(&state, &index);
                    json!({ "ok": true })
                }
                None => json!({ "ok": false, "error": "分类无效" }),
            }
        }
        "/api/character-traits" => json!({ "voice": { "enabled": false } }),
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
