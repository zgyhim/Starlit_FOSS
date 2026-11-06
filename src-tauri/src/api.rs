use std::collections::{BTreeMap, HashSet};
use std::path::Path;

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

    let reply_text = text.clone();
    let reply_conversation = payload.conversation.clone();
    crate::llm::spawn_reply(app, reply_conversation, id, reply_text);

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
            "reasoning_effort": "",
            "api_key_set": !cfg.llm.api_key.trim().is_empty(),
            "api_key_hint": mask_key(&cfg.llm.api_key),
        },
        "tts": {
            "mode": if cfg.tts.provider.is_empty() { "off" } else { &cfg.tts.provider },
            "endpoint": cfg.tts.endpoint,
            "model": "",
            "max_chars": 800,
            "api_key_set": false,
            "api_key_hint": "",
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
        if let Some(value) = llm.get("api_key").and_then(Value::as_str) {
            if !value.trim().is_empty() {
                cfg.llm.api_key = value.trim().to_string();
            }
        }
    }
    if let Some(tts) = payload.get("tts") {
        if let Some(value) = tts.get("mode").and_then(Value::as_str) {
            cfg.tts.provider = match value {
                "local" | "cloud" => value.to_string(),
                _ => String::new(),
            };
        }
        if let Some(value) = tts.get("endpoint").and_then(Value::as_str) {
            cfg.tts.endpoint = value.trim().to_string();
        }
    }
    crate::paths::write_config(&state.paths.config_file(), &cfg)?;
    *state.config.write().map_err(|e| e.to_string())? = cfg.clone();
    Ok(settings_body(&cfg))
}

#[tauri::command]
pub fn api_request(
    state: State<'_, AppState>,
    url: String,
    method: String,
    body: Option<String>,
) -> ApiOut {
    let path = url.split('?').next().unwrap_or("");
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
