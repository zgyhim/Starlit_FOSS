use std::collections::HashSet;
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

fn group_muted(state: &AppState, gid: &str) -> bool {
    read_json(state, "groups.json")
        .get("groups")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                (item.get("id").and_then(Value::as_str) == Some(gid))
                    .then(|| item.get("mute_all").and_then(Value::as_bool).unwrap_or(false))
            })
        })
        .unwrap_or(false)
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

/// 群成员：返回 (id, 名字, 角色, 等级)。兼容旧的纯字符串数组。
fn group_members(state: &AppState, gid: &str) -> Vec<(String, String, String, i64)> {
    let path = state.paths.data.join("groups").join(gid).join("members.json");
    let records: Option<Vec<Value>> = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("members").and_then(Value::as_array).cloned());
    let all = characters(state);
    match records {
        Some(items) => items
            .into_iter()
            .filter_map(|item| {
                let id = item.as_str().map(str::to_string)
                    .or_else(|| item.get("id").and_then(Value::as_str).map(str::to_string))?;
                let name = all.iter().find(|(cid, _)| cid == &id)?.1.clone();
                let role = item.get("role").and_then(Value::as_str).unwrap_or("member").to_string();
                let level = item.get("level").and_then(Value::as_i64).unwrap_or(1);
                Some((id, name, role, level))
            })
            .collect(),
        None => all
            .into_iter()
            .map(|(id, name)| (id, name, "member".to_string(), 1))
            .collect(),
    }
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

fn daily_left(state: &AppState, cfg: &crate::config::Config) -> bool {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let day = state
        .db
        .with(|conn| {
            conn.query_row("SELECT value FROM kv WHERE key = 'group.day'", [], |row| {
                row.get::<_, String>(0)
            })
        })
        .ok()
        .unwrap_or_default();
    let count: i64 = if day == today {
        state
            .db
            .with(|conn| {
                conn.query_row("SELECT value FROM kv WHERE key = 'group.count'", [], |row| {
                    row.get::<_, String>(0)
                })
            })
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    } else {
        0
    };
    (count as usize) < cfg.group.daily_max_total
}

fn bump_daily(state: &AppState) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let day = state
        .db
        .with(|conn| {
            conn.query_row("SELECT value FROM kv WHERE key = 'group.day'", [], |row| {
                row.get::<_, String>(0)
            })
        })
        .ok()
        .unwrap_or_default();
    let count: i64 = if day == today {
        state
            .db
            .with(|conn| {
                conn.query_row("SELECT value FROM kv WHERE key = 'group.count'", [], |row| {
                    row.get::<_, String>(0)
                })
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

fn member_daily_count(state: &AppState, conv: &Conversation, member: &str) -> i64 {
    let start = chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|dt| dt.and_utc().timestamp())
        .unwrap_or(0);
    state
        .db
        .with(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM messages
                 WHERE conversation_kind = 'Group' AND conversation_id = ?1
                   AND direction = 'Incoming' AND author_id = ?2 AND sent_at >= ?3",
                rusqlite::params![conv.id, member, start],
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

fn normalized_bigrams(text: &str) -> HashSet<u64> {
    let chars: Vec<char> = text
        .chars()
        .filter(|c| c.is_alphanumeric() || (*c as u32) >= 0x4e00)
        .collect();
    chars
        .windows(2)
        .map(|w| (w[0] as u64) << 32 | w[1] as u64)
        .collect()
}

fn too_similar(state: &AppState, conv: &Conversation, candidate: &str, window: usize, threshold: f64) -> bool {
    let rows = state
        .db
        .with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT text FROM messages
                 WHERE conversation_kind = 'Group' AND conversation_id = ?1
                   AND direction = 'Incoming' ORDER BY id DESC LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![conv.id, window as i64], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .unwrap_or_default();
    let candidate_set = normalized_bigrams(candidate);
    if candidate_set.is_empty() {
        return false;
    }
    rows.iter().any(|text| {
        let other = normalized_bigrams(text);
        if other.is_empty() {
            return false;
        }
        let overlap = candidate_set.intersection(&other).count() as f64;
        let ratio = overlap / candidate_set.len().min(other.len()) as f64;
        ratio >= threshold
    })
}

async fn generate(
    app: &AppHandle,
    state: &AppState,
    conv: &Conversation,
    member: &(String, String, String, i64),
    instruction: &str,
    max_bubbles: usize,
    extra_memory: bool,
) -> bool {
    let cfg = state.config();
    let (cid, name, _, _) = member;
    let persona = persona_of(state, name);
    let history = transcript(state, conv, 20);
    let owner = cfg.qq.server.owner_nickname.clone();
    let memories = crate::memory::recall(state, &history, cfg.memory.recall_top_k);
    let memory_block = if memories.is_empty() {
        String::new()
    } else {
        format!(
            "\n你可能记得这些事：\n{}",
            memories.iter().map(|m| format!("- {m}")).collect::<Vec<_>>().join("\n")
        )
    };
    let share = if extra_memory && !memories.is_empty() && chance(cfg.group.share_probability) {
        format!("\n可以自然地提起你记得的这件事：{}", memories[0])
    } else {
        String::new()
    };
    let members = group_members(state, &conv.id)
        .iter()
        .map(|(_, name, _, _)| name.as_str())
        .collect::<Vec<_>>()
        .join("、");
    let system = format!(
        "你是{name}，正在群聊「{}」里聊天。群成员有：{members}，还有{owner}。\n\
         以下是你的人设资料：\n{persona}\n\n\
         要求：以{name}的身份自然说话，1-2 句，口语化；不要使用 Markdown；不要解释你是 AI；不要代替别人说话。{memory_block}{share}",
        group_name(state, &conv.id)
    );
    let messages = vec![
        json!({ "role": "system", "content": system }),
        json!({
            "role": "user",
            "content": format!("最近的群聊：\n{history}\n\n{instruction}")
        }),
    ];
    let reply = match crate::llm::chat(&cfg, messages).await {
        Ok(text) => text,
        Err(_) => return false,
    };
    let text = reply.trim();
    if text.is_empty() {
        return false;
    }
    if too_similar(state, conv, text, cfg.group.dedupe_window, cfg.group.dedupe_overlap) {
        return false;
    }
    let mut sent = 0;
    for line in crate::llm::split_reply(text, cfg.behavior.split_threshold, max_bubbles.max(1)) {
        if let Some(message) = insert_group_message(state, conv, cid, name, &line) {
            let _ = app.emit("message:new", json!({ "message": message }));
            sent += 1;
            bump_daily(state);
        }
        if sent >= max_bubbles || sent >= 3 {
            break;
        }
        let delay = range_u64(800, 2500);
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    sent > 0
}

pub fn spawn_group_reply(app: AppHandle, conv: Conversation, _message_id: i64, user_text: String) {
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let cfg = state.config();
        if !cfg.group.enabled || !daily_left(&state, &cfg) || group_muted(&state, &conv.id) {
            return;
        }
        if hourly_count(&state, &conv) >= cfg.group.msgs_per_hour as i64 {
            return;
        }
        let members = group_members(&state, &conv.id);
        if members.is_empty() {
            return;
        }

        let mut responders: Vec<(String, String, String, i64)> = members
            .iter()
            .filter(|(_, name, _, _)| user_text.contains(&format!("@{name}")))
            .cloned()
            .collect();
        let mentioned = !responders.is_empty();
        if mentioned {
            responders.truncate(cfg.group.responders_all_mention.min(3).max(1));
        } else {
            if !chance(cfg.group.chain_reply_probability) {
                return;
            }
            let count = cfg.group.responders.min(2).max(1);
            for _ in 0..count {
                let index = range_u64(0, (members.len() - 1) as u64) as usize;
                let pick = members[index].clone();
                if !responders.iter().any(|(id, ..)| id == &pick.0) {
                    responders.push(pick);
                }
            }
        }
        responders.retain(|(cid, ..)| {
            member_daily_count(&state, &conv, cid) < cfg.group.daily_max_per_member as i64
                && member_gap_ok(&state, cid, cfg.group.min_gap_member_sec)
        });
        if responders.is_empty() {
            return;
        }
        let bubbles = if mentioned {
            cfg.group.bubbles_when_mentioned
        } else {
            cfg.group.bubbles_per_turn
        };

        for (index, member) in responders.iter().enumerate() {
            if index > 0 {
                let delay = range_u64(1500, 4500);
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            let jealous = !mentioned && user_text.contains('@') && chance(0.3);
            let instruction = if jealous {
                "老师刚才只叫了别人。你可以自然表达一点点在意，但不要吵架，1 句话。"
            } else {
                "请回应最新的群聊消息。"
            };
            let _ = generate(&app, &state, &conv, member, instruction, bubbles, true).await;
        }

        // 接话链：另一个成员随后补一句。
        if cfg.group.ai_chain_max > 1
            && chance(cfg.group.chain_reply_probability * 0.5)
            && hourly_count(&state, &conv) + 1 < cfg.group.msgs_per_hour as i64
        {
            let candidates: Vec<_> = members
                .iter()
                .filter(|(id, ..)| !responders.iter().any(|(rid, ..)| rid == id))
                .cloned()
                .collect();
            if !candidates.is_empty() {
                let delay = range_u64(
                    cfg.group.chain_reply_gap_sec[0],
                    cfg.group.chain_reply_gap_sec[1],
                );
                tokio::time::sleep(Duration::from_millis(delay * 1000)).await;
                let index = range_u64(0, (candidates.len() - 1) as u64) as usize;
                let member = &candidates[index];
                if member_daily_count(&state, &conv, &member.0)
                    < cfg.group.daily_max_per_member as i64
                    && member_gap_ok(&state, &member.0, cfg.group.min_gap_member_sec)
                {
                    let _ = generate(
                        &app,
                        &state,
                        &conv,
                        member,
                        "接着刚才的话题自然补一句，不要重复别人说过的话。",
                        1,
                        false,
                    )
                    .await;
                }
            }
        }
    });
}

/// 群聊 ambient：长时间没人说话时，让一位成员主动开口。
pub fn spawn_ambient(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let Some(state) = app.try_state::<AppState>() else {
                return;
            };
            let cfg = state.config();
            if !cfg.group.enabled || !cfg.group.ambient_enabled {
                continue;
            }
            let groups = read_json(&state, "groups.json");
            let Some(list) = groups.get("groups").and_then(Value::as_array) else {
                continue;
            };
            for group in list {
                let Some(gid) = group.get("id").and_then(Value::as_str) else {
                    continue;
                };
                if group_muted(&state, gid) || !daily_left(&state, &cfg) {
                    continue;
                }
                let now = chrono::Utc::now().timestamp();
                let next_key = format!("group.ambient.next.{gid}");
                let next = state
                    .db
                    .with(|conn| {
                        conn.query_row(
                            "SELECT value FROM kv WHERE key = ?1",
                            rusqlite::params![next_key],
                            |row| row.get::<_, String>(0),
                        )
                    })
                    .ok()
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or(0);
                if now < next {
                    continue;
                }
                let last: i64 = state
                    .db
                    .with(|conn| {
                        conn.query_row(
                            "SELECT COALESCE(MAX(sent_at), 0) FROM messages
                             WHERE conversation_kind = 'Group' AND conversation_id = ?1",
                            rusqlite::params![gid],
                            |row| row.get(0),
                        )
                    })
                    .unwrap_or(0);
                if now - last < 120 {
                    continue;
                }
                let members = group_members(&state, gid);
                if members.is_empty() {
                    continue;
                }
                let conv = Conversation {
                    kind: "Group".into(),
                    id: gid.to_string(),
                };
                let index = range_u64(0, (members.len() - 1) as u64) as usize;
                let member = &members[index];
                if member_daily_count(&state, &conv, &member.0) >= cfg.group.daily_max_per_member as i64 {
                    continue;
                }
                let _ = generate(
                    &app,
                    &state,
                    &conv,
                    member,
                    "群里安静了一阵，你主动说一句话，可以分享此刻的心情或想起的事，1-2 句。",
                    2,
                    true,
                )
                .await;
                let gap = range_u64(cfg.group.ambient_gap_min[0], cfg.group.ambient_gap_min[1]);
                let next_at = now + gap as i64 * 60;
                let _ = state.db.with(|conn| {
                    conn.execute(
                        "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?2)
                         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                        rusqlite::params![next_key, next_at.to_string()],
                    )?;
                    Ok(())
                });
            }
        }
    });
}
