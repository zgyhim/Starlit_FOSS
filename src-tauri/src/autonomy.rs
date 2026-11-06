use std::time::Duration;

use chrono::Timelike;
use rand::Rng;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::api::Conversation;
use crate::llm::{self, character_of, insert_reply, persona_of, recent_messages};
use crate::state::AppState;

const ENERGY_MAX: f64 = 100.0;
const ENERGY_DECAY_PER_MIN: f64 = 0.6;

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

fn kv_i64(state: &AppState, key: &str, default: i64) -> i64 {
    kv_get(state, key)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default)
}

fn kv_f64(state: &AppState, key: &str, default: f64) -> f64 {
    kv_get(state, key)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
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
    rand::rng().random_range(rhythm.near[0]..=rhythm.near[1])
}

/// 能量值：随时间衰减，用户互动时恢复。
fn current_energy(state: &AppState, now: i64) -> f64 {
    let stored = kv_f64(state, "autonomy.energy", 60.0);
    let since = kv_i64(state, "autonomy.energy_ts", now);
    let decay = ((now - since).max(0) as f64 / 60.0) * ENERGY_DECAY_PER_MIN;
    (stored - decay).clamp(0.0, ENERGY_MAX)
}

fn set_energy(state: &AppState, energy: f64) {
    let now = chrono::Utc::now().timestamp();
    kv_set(state, "autonomy.energy", &format!("{:.2}", energy.clamp(0.0, ENERGY_MAX)));
    kv_set(state, "autonomy.energy_ts", &now.to_string());
}

pub fn on_user_message(state: &AppState) {
    let now = chrono::Utc::now().timestamp();
    let energy = current_energy(state, now) + 15.0;
    set_energy(state, energy);
    kv_set(state, "autonomy.last_user_ts", &now.to_string());
    kv_set(state, "autonomy.cold", "0");
    let silence = state.config().autonomy.silence_min as i64;
    kv_set(state, "autonomy.silence_min", &silence.to_string());
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
        kv_i64(state, "autonomy.count", 0)
    } else {
        0
    };
    (today, count)
}

#[derive(Clone, Copy)]
enum Kind {
    Greet,
    Miss,
    Mood,
    Digest,
}

fn instruction(cfg: &crate::config::Config, kind: Kind, owner: &str) -> String {
    match kind {
        Kind::Greet => format!(
            "你刚打开电脑上线，主动和{owner}打个招呼，1-2 句，自然一点，不要提到系统提示。"
        ),
        Kind::Miss => format!(
            "你有点想念{owner}，主动发一句话。可以关心近况或提起想一起做的事，1-2 句，不要腻。"
        ),
        Kind::Mood => format!(
            "你想和{owner}分享此刻的心情或正在做的事，1-2 句，自然一点。"
        ),
        Kind::Digest => format!(
            "用 1-2 句话随口回顾今天和{owner}聊过的事，像突然想起来一样；不要写成报告。"
        ),
    }
}

fn next_checkin_kind(state: &AppState, cfg: &crate::config::Config) -> Kind {
    if cfg.autonomy.checkin_kinds.is_empty() {
        return Kind::Miss;
    }
    let index = kv_i64(state, "autonomy.checkin_idx", 0);
    let kinds = &cfg.autonomy.checkin_kinds;
    let value = kinds[(index as usize) % kinds.len()].as_str();
    kv_set(state, "autonomy.checkin_idx", &(index + 1).to_string());
    if value == "mood" {
        Kind::Mood
    } else {
        Kind::Miss
    }
}

async fn generate(app: &AppHandle, kind: Kind) -> Result<(), String> {
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
    messages.push(json!({
        "role": "user",
        "content": instruction(&cfg, kind, &owner),
    }));

    let reply = llm::chat(&cfg, messages).await?;
    let text = reply.trim();
    if text.is_empty() {
        return Ok(());
    }
    if let Some(message) = insert_reply(&state, &conv, &name, text) {
        let _ = app.emit("message:new", json!({ "message": message }));
    }

    let now = chrono::Utc::now().timestamp();
    let last_proactive = kv_i64(&state, "autonomy.last_ts", 0);
    let last_user = kv_i64(&state, "autonomy.last_user_ts", 0);
    let replied = last_user > last_proactive;
    let cold = if replied { 0 } else { kv_i64(&state, "autonomy.cold", 0) + 1 };

    let mut silence = if replied {
        cfg.autonomy.silence_min as f64
    } else {
        let previous = kv_f64(&state, "autonomy.silence_min", cfg.autonomy.silence_min as f64);
        (previous * cfg.autonomy.silence_growth)
            .clamp(cfg.autonomy.silence_min as f64, cfg.autonomy.silence_max_min as f64)
    };
    if !replied && (cold as usize) > cfg.autonomy.cold_streak_limit {
        let extra = cold as f64 - cfg.autonomy.cold_streak_limit as f64;
        silence = (silence * cfg.autonomy.silence_growth.powi(extra as i32))
            .min(cfg.autonomy.silence_max_min as f64);
    }

    kv_set(&state, "autonomy.last_ts", &now.to_string());
    kv_set(&state, "autonomy.cold", &cold.to_string());
    kv_set(&state, "autonomy.silence_min", &format!("{silence:.1}"));
    set_energy(&state, current_energy(&state, now) - 10.0);
    let (today, count) = next_day_reset(&state);
    kv_set(&state, "autonomy.day", &today);
    kv_set(&state, "autonomy.count", &(count + 1).to_string());

    let interval = pick_interval(&cfg).max(cfg.autonomy.min_gap_min).max(silence as u64);
    kv_set(&state, "autonomy.next_ts", &(now + interval as i64 * 60).to_string());
    Ok(())
}

async fn maybe_digest(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    let cfg = state.config();
    if !cfg.autonomy.digest_enabled {
        return false;
    }
    let now = chrono::Utc::now().timestamp();
    let last = kv_i64(&state, "autonomy.last_digest_ts", 0);
    if now - last < cfg.autonomy.digest_interval_min as i64 * 60 {
        return false;
    }
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let digest_day = kv_get(&state, "autonomy.digest_day").unwrap_or_default();
    let mut count = if digest_day == today {
        kv_i64(&state, "autonomy.digest_count", 0)
    } else {
        0
    };
    if count >= cfg.autonomy.digest_max_share_per_day as i64 {
        return false;
    }
    let messages: i64 = state
        .db
        .with(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM messages WHERE sent_at > ?1",
                rusqlite::params![last],
                |row| row.get(0),
            )
        })
        .unwrap_or(0);
    if messages < 6 {
        return false;
    }
    if generate(app, Kind::Digest).await.is_err() {
        return false;
    }
    count += 1;
    kv_set(&state, "autonomy.last_digest_ts", &now.to_string());
    kv_set(&state, "autonomy.digest_day", &today);
    kv_set(&state, "autonomy.digest_count", &count.to_string());
    true
}

/// 到期的约定主动跟进；成功后标记 followed_up。
async fn agreement_tick(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    let cfg = state.config();
    let now = chrono::Utc::now().timestamp() as f64;
    let due = state
        .db
        .with(|conn| {
            conn.query_row(
                "SELECT id, peer_id, title, details FROM shared_agreements
                 WHERE status = 'confirmed' AND due_at > 0 AND due_at <= ?1
                 ORDER BY due_at ASC LIMIT 1",
                rusqlite::params![now],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
        })
        .ok();
    let Some((id, peer, title, details)) = due else {
        return false;
    };
    let conv = Conversation {
        kind: "Private".into(),
        id: peer,
    };
    let (name, slug) = llm::character_of(&state, &conv.id);
    let persona = llm::persona_of(&state, &slug);
    let history = llm::recent_messages(&state, &conv, 12);
    let owner = cfg.qq.server.owner_nickname.clone();
    let mut messages = vec![json!({
        "role": "system",
        "content": format!(
            "你是{name}，正在和{owner}私聊。以下是你的人设：\n{persona}\n\n             要求：口语化、简短；不要 Markdown；不要解释你是 AI。"
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
    messages.push(json!({
        "role": "user",
        "content": format!(
            "你们之前有一个约定：「{title}」。补充：{details}\n             现在到期了，请自然地提起这件事，1-2 句，不要像系统提醒。"
        ),
    }));
    let Ok(reply) = llm::chat(&cfg, messages).await else {
        return false;
    };
    let text = reply.trim();
    if text.is_empty() {
        return false;
    }
    if let Some(message) = llm::insert_reply(&state, &conv, &name, text) {
        let _ = app.emit("message:new", json!({ "message": message }));
    }
    let _ = state.db.with(|conn| {
        conn.execute(
            "UPDATE shared_agreements SET status = 'followed_up' WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    });
    let now = chrono::Utc::now().timestamp();
    kv_set(&state, "autonomy.last_ts", &now.to_string());
    let (today, count) = next_day_reset(&state);
    kv_set(&state, "autonomy.day", &today);
    kv_set(&state, "autonomy.count", &(count + 1).to_string());
    true
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

    let energy = current_energy(&state, now);
    if energy < cfg.autonomy.energy_threshold as f64 {
        kv_set(&state, "autonomy.next_ts", &(now + 600).to_string());
        return;
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
    if let Some(last_msg) = kv_get(&state, "autonomy.last_user_ts").and_then(|v| v.parse::<i64>().ok()) {
        if now - last_msg < 120 {
            return;
        }
    }

    // 能量越低越容易放弃主动开口。
    let skip_probability = (cfg.autonomy.skip_probability
        + (1.0 - energy / ENERGY_MAX) * cfg.autonomy.state_influence)
        .clamp(0.0, 0.95);
    if agreement_tick(app).await {
        return;
    }

    if rand::rng().random::<f64>() < skip_probability {
        let interval = pick_interval(&cfg).max(cfg.autonomy.min_gap_min);
        kv_set(&state, "autonomy.next_ts", &(now + interval as i64 * 60).to_string());
        return;
    }

    if maybe_digest(app).await {
        return;
    }
    let kind = next_checkin_kind(&state, &cfg);
    let _ = generate(app, kind).await;
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
                    let _ = generate(&app, Kind::Greet).await;
                }
            }
        }
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            tick(&app).await;
        }
    });
}
