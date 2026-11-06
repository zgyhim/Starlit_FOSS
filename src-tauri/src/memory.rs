use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::state::AppState;

const DIM: usize = 128;

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || (ch as u32) >= 0x4e00
}

/// 轻量本地向量：字符二元组哈希到固定维度，避免依赖外部 embedding 服务。
pub fn embed(text: &str) -> Vec<f32> {
    let chars: Vec<char> = text
        .chars()
        .filter(|c| is_word(*c))
        .flat_map(|c| c.to_lowercase())
        .collect();
    let mut vec = vec![0f32; DIM];
    if chars.is_empty() {
        return vec;
    }
    for window in chars.windows(2) {
        let mut hasher = Sha256::new();
        hasher.update(window[0].to_string().as_bytes());
        hasher.update(window[1].to_string().as_bytes());
        let digest = hasher.finalize();
        let index = (digest[0] as usize | ((digest[1] as usize) << 8)) % DIM;
        vec[index] += 1.0;
    }
    let norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vec {
            *value /= norm;
        }
    }
    vec
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn text_hash(text: &str) -> String {
    let digest = Sha256::digest(text.trim().as_bytes());
    digest.iter().take(12).map(|b| format!("{b:02x}")).collect()
}

/// 召回：向量相似度 + 权重 + 时间衰减。
pub fn recall(state: &AppState, query: &str, top_k: usize) -> Vec<String> {
    if query.trim().is_empty() || top_k == 0 {
        return Vec::new();
    }
    let query_vec = embed(query);
    let now = chrono::Utc::now().timestamp() as f64;
    let rows = state
        .db
        .with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT m.text, m.vec, m.weight, m.ts, COALESCE(mm.pinned, 0)
                 FROM memories m
                 LEFT JOIN memory_meta mm
                   ON mm.source = 'memory' AND mm.local_id = CAST(m.id AS TEXT)
                 WHERE m.status = 'active' ORDER BY m.ts DESC LIMIT 400",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, f64>(2).unwrap_or(1.0),
                        row.get::<_, f64>(3).unwrap_or(0.0),
                        row.get::<_, i64>(4).unwrap_or(0),
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .unwrap_or_default();

    let mut scored: Vec<(f32, String)> = rows
        .into_iter()
        .filter_map(|(text, vec_json, weight, ts, pinned)| {
            let vec: Vec<f32> = serde_json::from_str(&vec_json).ok()?;
            let similarity = cosine(&query_vec, &vec);
            if similarity <= 0.02 {
                return None;
            }
            let age_days = ((now - ts).max(0.0)) / 86400.0;
            let recency = (-age_days / 30.0).exp() as f32;
            let score = similarity * 0.7 + (weight as f32) * 0.2 + recency * 0.1
                + if pinned > 0 { 0.15 } else { 0.0 };
            Some((score, text))
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    scored.into_iter().map(|(_, text)| text).collect()
}

pub fn store(
    state: &AppState,
    kind: &str,
    text: &str,
    emotion: &str,
    importance: f64,
) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    let hash = text_hash(text);
    let now = chrono::Utc::now().timestamp() as f64;
    let vec = embed(text);

    let (exists, previous) = state
        .db
        .with(|conn| {
            let exact: i64 = conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE text_hash = ?1 AND status = 'active'",
                rusqlite::params![hash],
                |row| row.get(0),
            )?;
            if exact > 0 {
                return Ok((true, None));
            }
            let mut stmt = conn.prepare(
                "SELECT id, text, vec, root_id, version FROM memories
                 WHERE status = 'active' ORDER BY ts DESC LIMIT 400",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, i64>(4).unwrap_or(1),
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut best: Option<(f32, i64, Option<i64>, i64)> = None;
            for (id, _other, vec_json, root_id, version) in rows {
                let Ok(other_vec) = serde_json::from_str::<Vec<f32>>(&vec_json) else {
                    continue;
                };
                let similarity = cosine(&vec, &other_vec);
                if similarity >= 0.82
                    && best.map(|(score, ..)| similarity > score).unwrap_or(true)
                {
                    best = Some((similarity, id, root_id, version));
                }
            }
            Ok((false, best))
        })?;
    if exists {
        return Ok(());
    }

    let vec_json = serde_json::to_string(&vec).map_err(|e| e.to_string())?;
    state.db.with(|conn| {
        match previous {
            Some((_, previous_id, root_id, version)) => {
                let root = root_id.unwrap_or(previous_id);
                conn.execute(
                    "UPDATE memories SET status = 'superseded', superseded_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, previous_id],
                )?;
                conn.execute(
                    "INSERT INTO memories
                     (ts, kind, text, vec, emotion, weight, text_hash, updated_at,
                      root_id, previous_id, version, status)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?1, ?8, ?9, ?10, 'active')",
                    rusqlite::params![
                        now,
                        kind,
                        text,
                        vec_json,
                        if emotion.is_empty() { None } else { Some(emotion) },
                        importance.clamp(0.0, 1.0),
                        hash,
                        root,
                        previous_id,
                        version + 1,
                    ],
                )?;
                let new_id = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO memory_meta (source, local_id, pinned, category, updated_at)
                     SELECT source, ?1, pinned, category, ?2 FROM memory_meta
                     WHERE source = 'memory' AND local_id = ?3
                     ON CONFLICT(source, local_id) DO NOTHING",
                    rusqlite::params![new_id.to_string(), now, previous_id.to_string()],
                )?;
            }
            None => {
                conn.execute(
                    "INSERT INTO memories
                     (ts, kind, text, vec, emotion, weight, text_hash, updated_at, version, status)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?1, 1, 'active')",
                    rusqlite::params![
                        now,
                        kind,
                        text,
                        vec_json,
                        if emotion.is_empty() { None } else { Some(emotion) },
                        importance.clamp(0.0, 1.0),
                        hash,
                    ],
                )?;
                let new_id = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO memory_meta (source, local_id, pinned, category, updated_at)
                     VALUES ('memory', ?1, 0, 'auto', ?2)
                     ON CONFLICT(source, local_id) DO NOTHING",
                    rusqlite::params![new_id.to_string(), now],
                )?;
            }
        }
        Ok(())
    })
}

pub fn list(
    state: &AppState,
    query: &str,
    offset: i64,
    limit: i64,
) -> Result<(Vec<Value>, i64), String> {
    let limit = limit.clamp(1, 200);
    state.db.with(|conn| {
        let pattern = format!("%{}%", query.trim());
        let filter = if query.trim().is_empty() {
            "1=1"
        } else {
            "m.text LIKE ?1"
        };
        let total: i64 = if query.trim().is_empty() {
            conn.query_row(
                "SELECT COUNT(*) FROM memories m WHERE m.status = 'active'",
                [],
                |row| row.get(0),
            )?
        } else {
            conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM memories m WHERE m.status = 'active' AND {filter}"
                ),
                rusqlite::params![pattern],
                |row| row.get(0),
            )?
        };
        let sql = format!(
            "SELECT m.id, m.text, m.ts, m.updated_at, m.version,
                    COALESCE(mm.pinned, 0), COALESCE(mm.category, 'auto'), mm.corrected_at
             FROM memories m
             LEFT JOIN memory_meta mm
               ON mm.source = 'memory' AND mm.local_id = CAST(m.id AS TEXT)
             WHERE m.status = 'active' AND {filter}
             ORDER BY COALESCE(mm.pinned, 0) DESC, m.ts DESC
             LIMIT ?2 OFFSET ?3"
        );
        let mut stmt = conn.prepare(&sql)?;
        let map = |row: &rusqlite::Row<'_>| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?.to_string(),
                "source": "memory",
                "text": row.get::<_, String>(1)?,
                "ts": row.get::<_, f64>(2).unwrap_or(0.0),
                "updated_at": row.get::<_, Option<f64>>(3)?,
                "version": row.get::<_, i64>(4).unwrap_or(1),
                "pinned": row.get::<_, i64>(5).unwrap_or(0) > 0,
                "category": row.get::<_, String>(6).unwrap_or_else(|_| "auto".into()),
                "corrected_at": row.get::<_, Option<f64>>(7)?,
            }))
        };
        let items = if query.trim().is_empty() {
            stmt.query_map(rusqlite::params![limit, offset], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map(rusqlite::params![pattern, limit, offset], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok((items, total))
    })
}

pub fn create(
    state: &AppState,
    text: &str,
    category: &str,
    pinned: bool,
) -> Result<String, String> {
    store(state, "fact", text, "", 0.8)?;
    let id = state.db.with(|conn| {
        conn.query_row(
            "SELECT id FROM memories WHERE text_hash = ?1 AND status = 'active'
             ORDER BY id DESC LIMIT 1",
            rusqlite::params![text_hash(text)],
            |row| row.get::<_, i64>(0),
        )
    })?;
    let now = chrono::Utc::now().timestamp() as f64;
    state.db.with(|conn| {
        conn.execute(
            "INSERT INTO memory_meta (source, local_id, pinned, category, updated_at)
             VALUES ('memory', ?1, ?2, ?3, ?4)
             ON CONFLICT(source, local_id) DO UPDATE SET pinned = excluded.pinned,
               category = excluded.category, updated_at = excluded.updated_at",
            rusqlite::params![id.to_string(), pinned as i64, category, now],
        )?;
        Ok(())
    })?;
    Ok(id.to_string())
}

pub fn update(
    state: &AppState,
    id: i64,
    text: Option<&str>,
    category: Option<&str>,
    pinned: Option<bool>,
    corrected: bool,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp() as f64;
    if let Some(text) = text {
        let text = text.trim();
        if !text.is_empty() {
            let vec = serde_json::to_string(&embed(text)).map_err(|e| e.to_string())?;
            state.db.with(|conn| {
                conn.execute(
                    "UPDATE memories SET text = ?1, vec = ?2, updated_at = ?3,
                     text_hash = ?4 WHERE id = ?5",
                    rusqlite::params![text, vec, now, text_hash(text), id],
                )?;
                Ok(())
            })?;
        }
    }
    if category.is_some() || pinned.is_some() || corrected {
        state.db.with(|conn| {
            conn.execute(
                "INSERT INTO memory_meta (source, local_id, pinned, category, updated_at, corrected_at)
                 VALUES ('memory', ?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(source, local_id) DO UPDATE SET
                   pinned = excluded.pinned,
                   category = excluded.category,
                   updated_at = excluded.updated_at,
                   corrected_at = COALESCE(excluded.corrected_at, memory_meta.corrected_at)",
                rusqlite::params![
                    id.to_string(),
                    pinned.unwrap_or(false) as i64,
                    category.unwrap_or("auto"),
                    now,
                    if corrected { Some(now) } else { None },
                ],
            )?;
            Ok(())
        })?;
    }
    Ok(())
}

pub fn delete(state: &AppState, id: i64) -> Result<(), String> {
    state.db.with(|conn| {
        conn.execute("DELETE FROM memories WHERE id = ?1", rusqlite::params![id])?;
        conn.execute(
            "DELETE FROM memory_meta WHERE source = 'memory' AND local_id = ?1",
            rusqlite::params![id.to_string()],
        )?;
        Ok(())
    })
}

/// 从一轮对话里抽取长期记忆；失败时静默。
pub async fn extract(
    state: &AppState,
    character: &str,
    user_text: &str,
    reply_text: &str,
) -> Result<(), String> {
    let cfg = state.config();
    if cfg.llm.api_key.trim().is_empty() {
        return Ok(());
    }
    let prompt = format!(
        "下面是一轮聊天。请抽取值得长期记住的信息（用户的事实、偏好、约定、重要事件）。\n\
         只输出 JSON 数组，元素格式 {{\"text\":\"一句话\",\"kind\":\"fact|preference|event\",\"emotion\":\"\",\"importance\":0.0}}。\n\
         没有值得记住的就输出 []。不要输出解释。\n\n\
         用户：{user_text}\n{character}：{reply_text}"
    );
    let body = serde_json::json!({
        "model": cfg.llm.model,
        "messages": [{ "role": "user", "content": prompt }],
        "temperature": 0.2,
        "stream": false,
    });
    let base = cfg.llm.base_url.trim_end_matches('/');
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(cfg.llm.timeout.max(10)))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(&cfg.llm.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Ok(());
    }
    let body: Value = response.json().await.unwrap_or_default();
    let content = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let cleaned = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let Ok(items) = serde_json::from_str::<Vec<Value>>(cleaned) else {
        return Ok(());
    };
    for item in items.into_iter().take(2) {
        let text = item.get("text").and_then(Value::as_str).unwrap_or("");
        let kind = item.get("kind").and_then(Value::as_str).unwrap_or("fact");
        let emotion = item.get("emotion").and_then(Value::as_str).unwrap_or("");
        let importance = item
            .get("importance")
            .and_then(Value::as_f64)
            .unwrap_or(0.5);
        let _ = store(state, kind, text, emotion, importance);
    }
    Ok(())
}
