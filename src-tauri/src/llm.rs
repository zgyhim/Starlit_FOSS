use std::time::Duration;

use rand::Rng;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::api::{Author, Conversation, Message};
use crate::state::AppState;

const RECENT_MESSAGES: i64 = 24;

pub(crate) fn character_of(state: &AppState, id: &str) -> (String, String) {
    let path = state.paths.data.join("characters.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        return ("角色".into(), id.into());
    };
    let Ok(data) = serde_json::from_str::<Value>(&text) else {
        return ("角色".into(), id.into());
    };
    for item in data.get("characters").and_then(Value::as_array).into_iter().flatten() {
        if item.get("id").and_then(Value::as_str) == Some(id) {
            return (
                item.get("name").and_then(Value::as_str).unwrap_or("角色").to_string(),
                item.get("slug").and_then(Value::as_str).unwrap_or(id).to_string(),
            );
        }
    }
    ("角色".into(), id.into())
}

pub(crate) fn persona_of(state: &AppState, name: &str) -> String {
    let dir = state.paths.persona.join("characters").join(name);
    let mut parts = Vec::new();
    for file in ["persona.md", "interests.md"] {
        if let Ok(text) = std::fs::read_to_string(dir.join(file)) {
            if !text.trim().is_empty() {
                parts.push(text);
            }
        }
    }
    if parts.is_empty() {
        if let Ok(text) = std::fs::read_to_string(state.paths.persona.join("persona.md")) {
            parts.push(text);
        }
    }
    parts.join("\n\n")
}

pub(crate) fn recent_messages(state: &AppState, conv: &Conversation, limit: i64) -> Vec<(String, String)> {
    let kind = if conv.kind == "Group" { "Group" } else { "Private" };
    state
        .db
        .with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT direction, text FROM messages
                 WHERE conversation_kind = ?1 AND conversation_id = ?2
                 ORDER BY id DESC LIMIT ?3",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![kind, conv.id, limit], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .unwrap_or_default()
        .into_iter()
        .rev()
        .collect()
}

pub(crate) fn insert_reply(state: &AppState, conv: &Conversation, name: &str, text: &str) -> Option<Message> {
    let now = chrono::Utc::now().timestamp();
    let parts = json!([{ "kind": "text", "text": text }]);
    let parts_json = serde_json::to_string(&parts).ok()?;
    let kind = if conv.kind == "Group" { "Group" } else { "Private" };
    let id = state
        .db
        .with(|conn| {
            conn.execute(
                "INSERT INTO messages
                 (conversation_kind, conversation_id, direction, author_id, author_name,
                  parts, text, sent_at, status)
                 VALUES (?1, ?2, 'Incoming', ?3, ?4, ?5, ?6, ?7, 'Sent')",
                rusqlite::params![kind, conv.id, conv.id, name, parts_json, text, now],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .ok()?;
    Some(Message {
        id,
        conversation: conv.clone(),
        direction: "Incoming".into(),
        author: Author {
            id: conv.id.clone(),
            name: name.to_string(),
            role: "member".into(),
        },
        parts,
        text: text.to_string(),
        reply_to: None,
        reply_preview: None,
        sent_at: now,
        client_id: None,
        status: "Sent".into(),
        system: false,
    })
}

pub(crate) fn split_reply(text: &str, threshold: usize, max_chunks: usize) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let threshold = threshold.max(8);
    if text.chars().count() <= threshold {
        return vec![text.to_string()];
    }
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        current.push(ch);
        count += 1;
        let sentence_end = matches!(ch, '。' | '！' | '？' | '!' | '?' | '…' | '\n');
        if (sentence_end && count >= threshold / 2) || count >= threshold {
            if !current.trim().is_empty() {
                chunks.push(current.trim().to_string());
            }
            current.clear();
            count = 0;
            if chunks.len() >= max_chunks.saturating_sub(1) {
                break;
            }
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    if chunks.is_empty() {
        chunks.push(text.to_string());
    }
    let joined_len: usize = chunks.iter().map(|c| c.chars().count()).sum();
    if joined_len < text.chars().count() {
        let rest: String = text.chars().skip(joined_len).collect();
        if let Some(last) = chunks.last_mut() {
            last.push_str(rest.trim());
        }
    }
    chunks.truncate(max_chunks.max(1));
    chunks
}

pub(crate) fn delay_ms(range: [u64; 2]) -> u64 {
    let low = range[0].min(range[1]);
    let high = range[1].max(range[0]);
    rand::rng().random_range(low..=high)
}

/// 单次非流式补全，供回复与主动消息共用。
pub async fn chat(
    cfg: &crate::config::Config,
    messages: Vec<Value>,
) -> Result<String, String> {
    if cfg.llm.api_key.trim().is_empty() {
        return Err("missing_api_key".into());
    }
    let base = cfg.llm.base_url.trim_end_matches('/');
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(cfg.llm.timeout.max(1)))
        .build()
        .map_err(|e| e.to_string())?;
    let mut body = json!({
        "model": cfg.llm.model,
        "messages": messages,
        "temperature": cfg.llm.temperature,
        "stream": false,
    });
    if !cfg.llm.reasoning_effort.trim().is_empty() {
        body["reasoning_effort"] = json!(cfg.llm.reasoning_effort.trim());
    }
    let response = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(&cfg.llm.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or_else(|_| json!({}));
    if !status.is_success() {
        let message = body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("模型请求失败");
        return Err(message.to_string());
    }
    Ok(body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string())
}

fn emit_status(app: &AppHandle, conv: &Conversation, message_id: i64, status: &str, reason: &str) {
    let _ = app.emit(
        "message:status",
        json!({
            "conversation": conv,
            "messageId": message_id.to_string(),
            "status": status,
            "reason": reason,
        }),
    );
}

pub fn spawn_reply(app: AppHandle, conv: Conversation, message_id: i64, user_text: String) {
    if conv.kind == "Group" {
        crate::group::spawn_group_reply(app, conv, message_id, user_text);
        return;
    }
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let cfg = state.config();
        if cfg.llm.api_key.trim().is_empty() {
            emit_status(&app, &conv, message_id, "failed", "missing_api_key");
            return;
        }

        let (name, slug) = character_of(&state, &conv.id);
        let persona = persona_of(&state, &slug);
        let history = recent_messages(&state, &conv, RECENT_MESSAGES);
        let owner = cfg.qq.server.owner_nickname.clone();
        let memories = crate::memory::recall(&state, &user_text, cfg.memory.recall_top_k);
        let memory_block = if memories.is_empty() {
            String::new()
        } else {
            format!(
                "\n\n你可能记得这些事：\n{}",
                memories.iter().map(|m| format!("- {m}")).collect::<Vec<_>>().join("\n")
            )
        };

        let mut messages = vec![json!({
            "role": "system",
            "content": format!(
                "你是{name}，正在和{owner}私聊。以下是你的人设资料，请始终以{name}的身份说话：\n\n{persona}\n\n\
                 要求：口语化、自然、简短，像真实聊天；不要使用 Markdown；不要解释你是 AI；不要复述人设。{memory_block}"
            ),
        })];
        for (direction, text) in history {
            if text.trim().is_empty() {
                continue;
            }
            messages.push(json!({
                "role": if direction == "Outgoing" { "user" } else { "assistant" },
                "content": text,
            }));
        }

        tokio::time::sleep(Duration::from_millis(400)).await;
        emit_status(&app, &conv, message_id, "processing", "");

        let reply = match chat(&cfg, messages).await {
            Ok(text) => text,
            Err(error) => {
                emit_status(&app, &conv, message_id, "failed", &error);
                return;
            }
        };

        let chunks = split_reply(&reply, cfg.behavior.split_threshold, cfg.behavior.max_chunks);
        if chunks.is_empty() {
            emit_status(&app, &conv, message_id, "failed", "empty_reply");
            return;
        }

        tokio::time::sleep(Duration::from_millis(delay_ms(cfg.behavior.outbound_delay_ms))).await;
        for (index, chunk) in chunks.iter().enumerate() {
            if index > 0 {
                tokio::time::sleep(Duration::from_millis(delay_ms(cfg.behavior.chunk_delay_ms))).await;
            }
            if let Some(message) = insert_reply(&state, &conv, &name, chunk) {
                let _ = app.emit("message:new", json!({ "message": message }));
            }
        }
        crate::sticker::maybe_send(&app, &state, &conv, &name);
        emit_status(&app, &conv, message_id, "replied", "");
        let reply_text = chunks.join("");
        let _ = crate::memory::extract(&state, &name, &user_text, &reply_text).await;
        let followup_conv = conv.clone();
        drop(state);
        crate::pipeline::schedule_followup(app, followup_conv, chrono::Utc::now().timestamp());
    });
}

#[cfg(test)]
mod tests {
    use super::split_reply;

    #[test]
    fn short_reply_stays_single() {
        assert_eq!(split_reply("你好呀", 28, 5), vec!["你好呀"]);
    }

    #[test]
    fn long_reply_is_bounded() {
        let text = "今天天气很好，我们一起去吃拉面吧。".repeat(8);
        let chunks = split_reply(&text, 28, 5);
        assert!(chunks.len() <= 5);
        assert!(chunks.iter().all(|c| !c.is_empty()));
    }
}
