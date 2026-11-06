use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::api::Conversation;
use crate::state::{AppState, MergeState};

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

fn quiet_hours(cfg: &crate::config::Config) -> bool {
    use chrono::Timelike;
    let parse = |value: &str| -> Option<i64> {
        let (h, m) = value.split_once(':')?;
        Some(h.parse::<i64>().ok()? * 60 + m.parse::<i64>().ok()?)
    };
    let now = chrono::Local::now();
    let current = now.hour() as i64 * 60 + now.minute() as i64;
    match (parse(&cfg.autonomy.quiet_hours[0]), parse(&cfg.autonomy.quiet_hours[1])) {
        (Some(start), Some(end)) if start <= end => current >= start && current < end,
        (Some(start), Some(end)) => current >= start || current < end,
        _ => false,
    }
}

/// 合并窗口：短时间连发多条只触发一次回复。
pub fn schedule_reply(app: AppHandle, conv: Conversation, message_id: i64, text: String) {
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let cfg = state.config();
        let key = format!("{}:{}", conv.kind, conv.id);
        let now = chrono::Utc::now().timestamp();
        let (generation, count, first_ts) = {
            let Ok(mut merge) = state.merge.lock() else {
                return;
            };
            let entry = merge.entry(key.clone()).or_insert(MergeState {
                generation: 0,
                first_ts: now,
                count: 0,
            });
            if entry.count == 0 {
                entry.first_ts = now;
            }
            entry.generation += 1;
            entry.count += 1;
            (entry.generation, entry.count, entry.first_ts)
        };

        let reached_max = count >= cfg.behavior.merge_max_msgs.max(1);
        let max_wait = now - first_ts >= cfg.behavior.merge_max_wait_sec.max(0.0) as i64;
        if !reached_max && !max_wait {
            tokio::time::sleep(Duration::from_millis(
                (cfg.behavior.merge_window_sec.max(0.0) * 1000.0) as u64,
            ))
            .await;
        }

        {
            let Ok(mut merge) = state.merge.lock() else {
                return;
            };
            match merge.get(&key) {
                Some(entry) if entry.generation == generation => {
                    merge.remove(&key);
                }
                _ => return,
            }
        }

        crate::llm::spawn_reply(app, conv, message_id, text);
    });
}

/// 对方一段时间没回时，补一句自然的追问。
pub fn schedule_followup(app: AppHandle, conv: Conversation, after_ts: i64) {
    if conv.kind != "Private" {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let cfg = state.config();
        if !cfg.conversation.enabled || quiet_hours(&cfg) {
            return;
        }
        let key = format!("{}:{}", conv.kind, conv.id);
        let delay = {
            use rand::Rng;
            let low = cfg.conversation.followup_delay_sec[0];
            let high = cfg.conversation.followup_delay_sec[1].max(low);
            rand::rng().random_range(low..=high)
        };
        tokio::time::sleep(Duration::from_secs(delay)).await;

        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let count_key = format!("followup.count.{key}");
        let mut count = kv_get(&state, &count_key)
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        if count >= cfg.conversation.max_followups_per_topic as i64 {
            return;
        }

        // 用户在这之后说过话 → 重置追问计数。
        let replied: i64 = state
            .db
            .with(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM messages
                     WHERE conversation_kind = 'Private' AND conversation_id = ?1
                       AND direction = 'Outgoing' AND sent_at > ?2",
                    rusqlite::params![conv.id, after_ts],
                    |row| row.get(0),
                )
            })
            .unwrap_or(0);
        if replied > 0 {
            kv_set(&state, &count_key, "0");
            return;
        }

        // 连续主动发言过多时停手。
        let bot_turns: i64 = state
            .db
            .with(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM messages
                     WHERE conversation_kind = 'Private' AND conversation_id = ?1
                       AND direction = 'Incoming' AND sent_at > ?2",
                    rusqlite::params![conv.id, after_ts],
                    |row| row.get(0),
                )
            })
            .unwrap_or(0);
        if bot_turns >= cfg.conversation.max_bot_turns_without_reply as i64 + 1 {
            return;
        }

        let (name, slug) = crate::llm::character_of(&state, &conv.id);
        let persona = crate::llm::persona_of(&state, &slug);
        let history = crate::llm::recent_messages(&state, &conv, 10);
        let owner = cfg.qq.server.owner_nickname.clone();
        let mut messages = vec![serde_json::json!({
            "role": "system",
            "content": format!(
                "你是{name}，正在和{owner}私聊。以下是你的人设：\n{persona}\n\n\
                 要求：口语化、简短；不要 Markdown；不要解释你是 AI。"
            ),
        })];
        for (direction, text) in &history {
            if text.trim().is_empty() {
                continue;
            }
            messages.push(serde_json::json!({
                "role": if direction == "Outgoing" { "user" } else { "assistant" },
                "content": text,
            }));
        }
        messages.push(serde_json::json!({
            "role": "user",
            "content": "对方还没回。你可以自然补一句轻松的追问或分享，不要催、不要抱怨，1 句。",
        }));

        let Ok(reply) = crate::llm::chat(&cfg, messages).await else {
            return;
        };
        let text = reply.trim();
        if text.is_empty() {
            return;
        }
        if let Some(message) = crate::llm::insert_reply(&state, &conv, &name, text) {
            use tauri::Emitter;
            let _ = app.emit("message:new", serde_json::json!({ "message": message }));
        }
        count += 1;
        kv_set(&state, &count_key, &count.to_string());
        drop(state);
        schedule_followup(app, conv, after_ts);
    });
}

pub fn reset_followup(state: &AppState, conv: &Conversation) {
    let key = format!("followup.count.{}:{}", conv.kind, conv.id);
    kv_set(state, &key, "0");
}
