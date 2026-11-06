#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub llm: LlmConfig,
    pub qq: QqConfig,
    pub behavior: BehaviorConfig,
    pub conversation: ConversationConfig,
    pub autonomy: AutonomyConfig,
    pub memory: MemoryConfig,
    pub sticker: StickerConfig,
    pub search: SearchConfig,
    pub tts: TtsConfig,
    pub group: GroupConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout: u64,
    pub temperature: f64,
    /// OpenAI 兼容的 reasoning_effort；空字符串表示交给服务商默认。
    pub reasoning_effort: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.deepseek.com".into(),
            api_key: String::new(),
            model: "deepseek-flash".into(),
            timeout: 90,
            temperature: 0.9,
            reasoning_effort: "low".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct QqConfig {
    pub self_id: String,
    pub owner_id: String,
    pub server: ServerConfig,
    pub group_mode: String,
    pub group_allowlist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub owner_nickname: String,
    pub bot_nickname: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            owner_nickname: "老师".into(),
            bot_nickname: "星野".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorConfig {
    pub outbound_delay_ms: [u64; 2],
    pub chunk_delay_ms: [u64; 2],
    pub sticker_delay_ms: [u64; 2],
    pub split_long_reply: bool,
    pub split_threshold: usize,
    pub max_chunks: usize,
    pub bubble_delay_ms: [u64; 2],
    pub short_bubble_delay_ms: [u64; 2],
    pub long_bubble_delay_ms: [u64; 2],
    pub merge_window_sec: f64,
    pub merge_max_wait_sec: f64,
    pub merge_max_msgs: usize,
    pub sticker_only_reply_prob: f64,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            outbound_delay_ms: [5000, 20000],
            chunk_delay_ms: [5000, 20000],
            sticker_delay_ms: [4000, 9000],
            split_long_reply: true,
            split_threshold: 28,
            max_chunks: 5,
            bubble_delay_ms: [5000, 20000],
            short_bubble_delay_ms: [4000, 12000],
            long_bubble_delay_ms: [7000, 22000],
            merge_window_sec: 9.0,
            merge_max_wait_sec: 30.0,
            merge_max_msgs: 10,
            sticker_only_reply_prob: 0.35,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConversationConfig {
    pub enabled: bool,
    pub active_window_min: u64,
    pub followup_delay_sec: [u64; 2],
    pub max_bot_turns_without_reply: usize,
    pub max_followups_per_topic: usize,
    pub deep_max_bot_turns: usize,
    pub deep_max_followups: usize,
    pub quiet_hours_max_turns: usize,
    pub energy_threshold: i64,
    pub max_burst_bubbles: usize,
    pub deep_max_burst_bubbles: usize,
    pub followup_backoff_sec: Vec<[u64; 2]>,
    pub max_probes_per_topic: usize,
    pub reopen_delay_sec: [u64; 2],
    pub max_reopens: usize,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            active_window_min: 25,
            followup_delay_sec: [60, 180],
            max_bot_turns_without_reply: 5,
            max_followups_per_topic: 3,
            deep_max_bot_turns: 8,
            deep_max_followups: 8,
            quiet_hours_max_turns: 1,
            energy_threshold: 25,
            max_burst_bubbles: 5,
            deep_max_burst_bubbles: 9,
            followup_backoff_sec: vec![[120, 300], [600, 2400]],
            max_probes_per_topic: 1,
            reopen_delay_sec: [1200, 5400],
            max_reopens: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutonomyConfig {
    pub enabled: bool,
    pub rhythm: RhythmConfig,
    pub state_influence: f64,
    pub skip_probability: f64,
    pub heartbeat: HeartbeatConfig,
    pub quiet_hours: [String; 2],
    pub min_gap_min: u64,
    pub cold_streak_limit: usize,
    pub energy_threshold: i64,
    pub daily_max: usize,
    pub silence_min: u64,
    pub silence_growth: f64,
    pub silence_max_min: u64,
    pub busy_grace_min: u64,
    pub checkin_kinds: Vec<String>,
    pub digest_enabled: bool,
    pub digest_interval_min: u64,
    pub digest_max_share_per_day: usize,
    pub digest_threshold: f64,
    pub greet_on_start: bool,
    pub greet_delay_sec: u64,
}

impl Default for AutonomyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rhythm: RhythmConfig::default(),
            state_influence: 0.7,
            skip_probability: 0.15,
            heartbeat: HeartbeatConfig::default(),
            quiet_hours: ["00:30".into(), "08:30".into()],
            min_gap_min: 25,
            cold_streak_limit: 2,
            energy_threshold: 30,
            daily_max: 14,
            silence_min: 25,
            silence_growth: 1.2,
            silence_max_min: 60,
            busy_grace_min: 45,
            checkin_kinds: vec!["miss".into(), "mood".into(), "miss".into(), "miss".into()],
            digest_enabled: true,
            digest_interval_min: 90,
            digest_max_share_per_day: 3,
            digest_threshold: 0.42,
            greet_on_start: true,
            greet_delay_sec: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RhythmConfig {
    pub instant: [u64; 2],
    pub near: [u64; 2],
    pub normal: [u64; 2],
    pub long: [u64; 2],
    pub weights: [f64; 4],
}

impl Default for RhythmConfig {
    fn default() -> Self {
        Self {
            instant: [1, 8],
            near: [20, 120],
            normal: [120, 480],
            long: [720, 2880],
            weights: [0.3, 0.32, 0.27, 0.11],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HeartbeatConfig {
    pub base_interval_min: u64,
    pub jitter_min: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            base_interval_min: 210,
            jitter_min: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryConfig {
    pub db_path: String,
    pub sticker_dir: String,
    pub sticker_index: String,
    pub recall_top_k: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            db_path: "./data/companion.db".into(),
            sticker_dir: "./data/stickers".into(),
            sticker_index: "./data/stickers/index.json".into(),
            recall_top_k: 6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StickerConfig {
    pub enabled: bool,
    pub base_probability: f64,
    pub min_gap_min: u64,
    pub daily_max: usize,
    pub energy_threshold: i64,
    pub extra_dirs: Vec<String>,
    pub max_per_turn: usize,
    pub steal_enabled: bool,
    pub steal_probability: f64,
    pub steal_daily_max: usize,
    pub steal_from_groups: bool,
}

impl Default for StickerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_probability: 0.5,
            min_gap_min: 0,
            daily_max: 45,
            energy_threshold: 25,
            extra_dirs: Vec::new(),
            max_per_turn: 2,
            steal_enabled: true,
            steal_probability: 0.55,
            steal_daily_max: 8,
            steal_from_groups: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    pub provider: String,
    pub endpoint: String,
    pub api_key: String,
    pub cx: String,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            provider: "searx".into(),
            endpoint: "http://127.0.0.1:8888".into(),
            api_key: String::new(),
            cx: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsConfig {
    pub provider: String,
    pub endpoint: String,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            endpoint: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GroupConfig {
    pub enabled: bool,
    pub ai_chain_max: usize,
    pub ai_no_owner_max: usize,
    pub chain_cooldown_min: u64,
    pub round_gap_reset_min: u64,
    pub msgs_per_hour: usize,
    pub msgs_per_hour_burst: usize,
    pub daily_max_total: usize,
    pub daily_max_per_member: usize,
    pub min_gap_member_sec: u64,
    pub bubbles_per_turn: usize,
    pub bubbles_when_mentioned: usize,
    pub responders: usize,
    pub responders_all_mention: usize,
    pub responder_gap_sec: [u64; 2],
    pub ambient_enabled: bool,
    pub ambient_gap_min: [u64; 2],
    pub chain_reply_probability: f64,
    pub chain_reply_gap_sec: [u64; 2],
    pub dedupe_window: usize,
    pub dedupe_overlap: f64,
    pub backlog_responders: usize,
    pub backlog_span_sec: [u64; 2],
    pub share_probability: f64,
    pub share_daily_max: usize,
    pub jealousy_cooldown_min: u64,
    pub jealousy_min_affection: i64,
    pub ambient_share_probability: f64,
    pub dm_share_probability: f64,
    pub dm_share_daily_max: usize,
    pub max_members: usize,
    pub join_request_cooldown_h: u64,
    pub index_path: String,
    pub dir_path: String,
}

impl Default for GroupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ai_chain_max: 5,
            ai_no_owner_max: 8,
            chain_cooldown_min: 15,
            round_gap_reset_min: 6,
            msgs_per_hour: 18,
            msgs_per_hour_burst: 5,
            daily_max_total: 120,
            daily_max_per_member: 10,
            min_gap_member_sec: 20,
            bubbles_per_turn: 2,
            bubbles_when_mentioned: 3,
            responders: 4,
            responders_all_mention: 5,
            responder_gap_sec: [2, 18],
            ambient_enabled: true,
            ambient_gap_min: [8, 25],
            chain_reply_probability: 0.8,
            chain_reply_gap_sec: [10, 50],
            dedupe_window: 15,
            dedupe_overlap: 0.5,
            backlog_responders: 5,
            backlog_span_sec: [20, 90],
            share_probability: 0.25,
            share_daily_max: 1,
            jealousy_cooldown_min: 30,
            jealousy_min_affection: 50,
            ambient_share_probability: 0.3,
            dm_share_probability: 0.15,
            dm_share_daily_max: 1,
            max_members: 20,
            join_request_cooldown_h: 24,
            index_path: "./data/groups.json".into(),
            dir_path: "./data/groups".into(),
        }
    }
}

/// 仅用于兼容旧配置里可能存在、但本实现不再使用的额外键。
pub type ExtraConfig = BTreeMap<String, serde_yaml::Value>;
