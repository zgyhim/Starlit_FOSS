use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::api::{Author, Conversation, Message};
use crate::state::AppState;

fn range_u64(low: u64, high: u64) -> u64 {
    use rand::Rng;
    rand::rng().random_range(low..=high)
}

fn chance(probability: f64) -> bool {
    use rand::Rng;
    rand::rng().random::<f64>() < probability
}


fn read_json(state: &AppState, name: &str) -> Value {
    std::fs::read_to_string(state.paths.data.join(name))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({}))
}

fn group_name(state: &AppState, id: &str) -> String {
    read_json(state, "groups.json")
        .get("groups")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                (item.get("id").and_then(Value::as_str) == Some(id))
                    .then(|| item.get("name").and_then(Value::as_str).unwrap_or("群聊").to_string())
            })
        })
        .unwrap_or_else(|| "群聊".into())
}

fn characters(state: &AppState) -> Vec<(String, String)> {
    read_json(state, "characters.json")
        .get("characters")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some((
                        item.get("id")?.as_str()?.to_string(),
                        item.get("name").and_then(Value::as_str).unwrap_or("角色").to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn transcript(state: &AppState, conv: &Conversation, limit: i64) -> String {
    let rows = state
        .db
        .with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT direction, author_name, text FROM messages
                 WHERE conversation_kind = 'Group' AND conversation_id = ?1
                 ORDER BY id DESC LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![conv.id, limit], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .unwrap_or_default();
    rows.into_iter()
        .rev()
        .map(|(direction, name, text)| {
            let who = if direction == "Outgoing" { "老师" } else { &name };
            format!("{who}: {text}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn persona_of(state: &AppState, name: &str) -> String {
    let dir = state.paths.persona.join("characters").join(name);
    let mut parts = Vec::new();
    for file in ["persona.md", "interests.md"] {
        if let Ok(text) = std::fs::read_to_string(dir.join(file)) {
            if !text.trim().is_empty() {
                parts.push(text);
            }
        }
    }
    parts.join("\n\n")
}

fn insert_group_message(
    state: &AppState,
    conv: &Conversation,
    author_id: &str,
    author_name: &str,
    text: &str,
) -> Option<Message> {
    let now = chrono::Utc::now().timestamp();
    let parts = json!([{ "kind": "text", "text": text }]);
    let parts_json = serde_json::to_string(&parts).ok()?;
    let id = state
        .db
        .with(|conn| {
            conn.execute(
                "INSERT INTO messages
                 (conversation_kind, conversation_id, direction, author_id, author_name,
                  parts, text, sent_at, status)
                 VALUES ('Group', ?1, 'Incoming', ?2, ?3, ?4, ?5, ?6, 'Sent')",
                rusqlite::params![conv.id, author_id, author_name, parts_json, text, now],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .ok()?;
    Some(Message {
        id,
        conversation: conv.clone(),
        direction: "Incoming".into(),
        author: Author {
            id: author_id.to_string(),
            name: author_name.to_string(),
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

fn group_daily_left(state: &AppState, cfg: &crate::config::Config) -> bool {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let day = state
        .db
        .with(|conn| {
            conn.query_row(
                "SELECT value FROM kv WHERE key = 'group.day'",
                [],
                |row| row.get::<_, String>(0),
            )
        })
        .ok()
        .unwrap_or_default();
    let count: i64 = if day == today {
        state
            .db
            .with(|conn| {
                conn.query_row(
                    "SELECT value FROM kv WHERE key = 'group.count'",
                    [],
                    |row| row.get::<_, String>(0),
                )
            })
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    } else {
        0
    };
    (count as usize) < cfg.group.daily_max_total
}

fn bump_group_count(state: &AppState) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let day = state
        .db
        .with(|conn| {
            conn.query_row(
                "SELECT value FROM kv WHERE key = 'group.day'",
                [],
                |row| row.get::<_, String>(0),
            )
        })
        .ok()
        .unwrap_or_default();
    let count: i64 = if day == today {
        state
            .db
            .with(|conn| {
                conn.query_row(
                    "SELECT value FROM kv WHERE key = 'group.count'",
                    [],
                    |row| row.get::<_, String>(0),
                )
            })
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    } else {
        0
    };
    let now = chrono::Utc::now().timestamp();
    let _ = state.db.with(|conn| {
        conn.execute(
            "INSERT INTO kv (key, value, updated_at) VALUES ('group.day', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![today, now],
        )?;
        conn.execute(
            "INSERT INTO kv (key, value, updated_at) VALUES ('group.count', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![(count + 1).to_string(), now],
        )?;
        Ok(())
    });
}

fn hourly_count(state: &AppState, conv: &Conversation) -> i64 {
    let since = chrono::Utc::now().timestamp() - 3600;
    state
        .db
        .with(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM messages
                 WHERE conversation_kind = 'Group' AND conversation_id = ?1
                   AND direction = 'Incoming' AND sent_at >= ?2",
                rusqlite::params![conv.id, since],
                |row| row.get(0),
            )
        })
        .unwrap_or(0)
}

fn member_gap_ok(state: &AppState, member_id: &str, min_gap_sec: u64) -> bool {
    let key = format!("group.last.{member_id}");
    let now = chrono::Utc::now().timestamp();
    let last = state
        .db
        .with(|conn| {
            conn.query_row(
                "SELECT value FROM kv WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get::<_, String>(0),
            )
        })
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    if now - last < min_gap_sec as i64 {
        return false;
    }
    let _ = state.db.with(|conn| {
        conn.execute(
            "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![key, now.to_string()],
        )?;
        Ok(())
    });
    true
}

pub fn spawn_group_reply(app: AppHandle, conv: Conversation, _message_id: i64, user_text: String) {
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let cfg = state.config();
        if !cfg.group.enabled || !group_daily_left(&state, &cfg) {
            return;
        }
        if hourly_count(&state, &conv) >= cfg.group.msgs_per_hour as i64 {
            return;
        }
        let members = characters(&state);
        if members.is_empty() {
            return;
        }
        let group = group_name(&state, &conv.id);
        let history = transcript(&state, &conv, 20);
        let owner = cfg.qq.server.owner_nickname.clone();

        // 被 @ 的角色优先；没有被 @ 时，按概率决定是否插话。
        let mut responders: Vec<(String, String)> = members
            .iter()
            .filter(|(_, name)| user_text.contains(&format!("@{name}")))
            .cloned()
            .collect();
        let mentioned = !responders.is_empty();
        if mentioned {
            responders.truncate(3);
        }
        if responders.is_empty() {
            if !chance(cfg.group.chain_reply_probability) {
                return;
            }
            let count = if chance(cfg.group.chain_reply_probability) {
                2
            } else {
                1
            };
            for _ in 0..count {
                let index = {
                    use rand::Rng;
                    rand::rng().random_range(0..members.len())
                };
                let pick = &members[index];
                if !responders.iter().any(|(id, _)| id == &pick.0) {
                    responders.push(pick.clone());
                }
            }
        }

        responders.retain(|(cid, _)| {
            member_gap_ok(&state, cid, cfg.group.min_gap_member_sec)
        });
        let bubbles = if mentioned {
            cfg.group.bubbles_when_mentioned
        } else {
            cfg.group.bubbles_per_turn
        };
        for (index, (cid, name)) in responders.iter().enumerate() {
            if index > 0 {
                let delay = range_u64(1500, 6000);
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            let persona = persona_of(&state, name);
            let member_names = members
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>()
                .join("、");
            let system = format!(
                "你是{name}，正在群聊「{group}」里聊天。群成员有：{member_names}，还有{owner}。\n\
                 以下是你的人设资料：\n{persona}\n\n\
                 要求：以{name}的身份自然接话，1-2 句，口语化；不要使用 Markdown；不要解释你是 AI；不要代替别人说话。"
            );
            let messages = vec![
                json!({ "role": "system", "content": system }),
                json!({
                    "role": "user",
                    "content": format!("最近的群聊：\n{history}\n\n最新一条：{owner}: {user_text}\n\n请你回应。")
                }),
            ];
            let reply = match crate::llm::chat(&cfg, messages).await {
                Ok(text) => text,
                Err(_) => continue,
            };
            let text = reply.trim();
            if text.is_empty() {
                continue;
            }
            for line in crate::llm::split_reply(text, cfg.behavior.split_threshold, bubbles.max(1)) {
                if let Some(message) = insert_group_message(&state, &conv, cid, name, &line) {
                    let _ = app.emit("message:new", json!({ "message": message }));
                }
                let delay = range_u64(800, 2500);
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            bump_group_count(&state);
        }
    });
}
