use std::time::Duration;

use chrono::Timelike;
use rand::Rng;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::api::Conversation;
use crate::llm::{self, character_of, insert_reply, persona_of, recent_messages};
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

fn minutes_of_day(value: &str) -> Option<i64> {
    let (hour, minute) = value.trim().split_once(':')?;
    Some(hour.parse::<i64>().ok()? * 60 + minute.parse::<i64>().ok()?)
}

fn in_quiet_hours(cfg: &crate::config::Config) -> bool {
    let now = chrono::Local::now();
    let current = now.time().hour() as i64 * 60 + now.time().minute() as i64;
    let start = minutes_of_day(&cfg.autonomy.quiet_hours[0]);
    let end = minutes_of_day(&cfg.autonomy.quiet_hours[1]);
    match (start, end) {
        (Some(start), Some(end)) if start <= end => current >= start && current < end,
        (Some(start), Some(end)) => current >= start || current < end,
        _ => false,
    }
}

fn pick_interval(cfg: &crate::config::Config) -> u64 {
    let rhythm = &cfg.autonomy.rhythm;
    let weights = rhythm.weights;
    let total: f64 = weights.iter().sum();
    let mut roll = rand::rng().random_range(0.0..total.max(f64::EPSILON));
    let ranges = [rhythm.instant, rhythm.near, rhythm.normal, rhythm.long];
    for (index, weight) in weights.iter().enumerate() {
        if roll < *weight {
            return rand::rng().random_range(ranges[index][0]..=ranges[index][1]);
        }
        roll -= weight;
    }
    rand::rng().random_range(20..=120)
}

fn pick_conversation(state: &AppState) -> Option<Conversation> {
    let from_db = state
        .db
        .with(|conn| {
            conn.query_row(
                "SELECT conversation_id FROM messages
                 WHERE conversation_kind = 'Private'
                 GROUP BY conversation_id
                 ORDER BY MAX(sent_at) DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
        })
        .ok();
    if let Some(id) = from_db {
        return Some(Conversation {
            kind: "Private".into(),
            id,
        });
    }
    let path = state.paths.data.join("characters.json");
    let text = std::fs::read_to_string(path).ok()?;
    let data: Value = serde_json::from_str(&text).ok()?;
    let id = data
        .get("characters")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)?;
    Some(Conversation {
        kind: "Private".into(),
        id: id.to_string(),
    })
}

fn next_day_reset(state: &AppState) -> (String, i64) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let stored_day = kv_get(state, "autonomy.day").unwrap_or_default();
    let count = if stored_day == today {
        kv_get(state, "autonomy.count")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    } else {
        0
    };
    (today, count)
}

async fn generate(app: &AppHandle, greet: bool) -> Result<(), String> {
    let Some(state) = app.try_state::<AppState>() else {
        return Ok(());
    };
    let cfg = state.config();
    let Some(conv) = pick_conversation(&state) else {
        return Ok(());
    };
    let (name, slug) = character_of(&state, &conv.id);
    let persona = persona_of(&state, &slug);
    let history = recent_messages(&state, &conv, 10);
    let owner = cfg.qq.server.owner_nickname.clone();
    let query = history
        .last()
        .map(|(_, text)| text.clone())
        .unwrap_or_else(|| "最近怎么样".into());
    let memories = crate::memory::recall(&state, &query, cfg.memory.recall_top_k);
    let memory_block = if memories.is_empty() {
        String::new()
    } else {
        format!(
            "\n你可能记得这些事：\n{}",
            memories.iter().map(|m| format!("- {m}")).collect::<Vec<_>>().join("\n")
        )
    };

    let task = if greet {
        format!("你刚打开电脑上线，主动和{owner}打个招呼，1-2 句，自然一点，不要提到系统提示。")
    } else {
        format!(
            "现在没有新消息。你想主动找{owner}说点什么，结合最近的聊天或你记得的事，1-2 句，自然一点，不要重复上次的话。"
        )
    };

    let mut messages = vec![json!({
        "role": "system",
        "content": format!(
            "你是{name}，正在和{owner}私聊。以下是你的人设资料，请始终以{name}的身份说话：\n\n{persona}\n\n\
             要求：口语化、简短、像真实聊天；不要使用 Markdown；不要解释你是 AI。{memory_block}"
        ),
    })];
    for (direction, text) in &history {
        if text.trim().is_empty() {
            continue;
        }
        messages.push(json!({
            "role": if direction == "Outgoing" { "user" } else { "assistant" },
            "content": text,
        }));
    }
    messages.push(json!({ "role": "user", "content": task }));

    let reply = llm::chat(&cfg, messages).await?;
    let text = reply.trim();
    if text.is_empty() {
        return Ok(());
    }
    if let Some(message) = insert_reply(&state, &conv, &name, text) {
        let _ = app.emit("message:new", json!({ "message": message }));
    }

    let now = chrono::Utc::now().timestamp();
    kv_set(&state, "autonomy.last_ts", &now.to_string());
    let (today, count) = next_day_reset(&state);
    kv_set(&state, "autonomy.day", &today);
    kv_set(&state, "autonomy.count", &(count + 1).to_string());
    let interval = pick_interval(&cfg).max(cfg.autonomy.min_gap_min);
    kv_set(&state, "autonomy.next_ts", &(now + interval as i64 * 60).to_string());
    Ok(())
}

async fn tick(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let cfg = state.config();
    if !cfg.autonomy.enabled || in_quiet_hours(&cfg) {
        return;
    }
    let now = chrono::Utc::now().timestamp();
    if let Some(next) = kv_get(&state, "autonomy.next_ts").and_then(|v| v.parse::<i64>().ok()) {
        if now < next {
            return;
        }
    }
    if let Some(last) = kv_get(&state, "autonomy.last_ts").and_then(|v| v.parse::<i64>().ok()) {
        if now - last < cfg.autonomy.min_gap_min as i64 * 60 {
            return;
        }
    }
    let (_, count) = next_day_reset(&state);
    if count >= cfg.autonomy.daily_max as i64 {
        return;
    }
    // 用户刚说过话时先别插嘴。
    if let Some(last_msg) = kv_get(&state, "autonomy.last_user_ts").and_then(|v| v.parse::<i64>().ok()) {
        if now - last_msg < 120 {
            return;
        }
    }
    if rand::rng().random::<f64>() < cfg.autonomy.skip_probability {
        let interval = pick_interval(&cfg).max(cfg.autonomy.min_gap_min);
        kv_set(&state, "autonomy.next_ts", &(now + interval as i64 * 60).to_string());
        return;
    }
    let _ = generate(app, false).await;
}

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let greet_delay = {
            let Some(state) = app.try_state::<AppState>() else {
                return;
            };
            state.config().autonomy.greet_delay_sec.max(1)
        };
        tokio::time::sleep(Duration::from_secs(greet_delay)).await;
        {
            let Some(state) = app.try_state::<AppState>() else {
                return;
            };
            let cfg = state.config();
            if cfg.autonomy.enabled && cfg.autonomy.greet_on_start && !in_quiet_hours(&cfg) {
                let now = chrono::Utc::now().timestamp();
                let last = kv_get(&state, "autonomy.last_ts")
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                if now - last >= cfg.autonomy.min_gap_min as i64 * 60 {
                    let _ = generate(&app, true).await;
                }
            }
        }
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            tick(&app).await;
        }
    });
}
