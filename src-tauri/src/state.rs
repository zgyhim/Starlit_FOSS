use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Mutex, RwLock};

use crate::config::Config;
use crate::db::Db;
use crate::paths::Paths;

pub struct AppState {
    pub paths: Paths,
    pub config: RwLock<Config>,
    pub db: Db,
    pub started_at: i64,
    /// 本次退出时强制清理解压缓存。
    pub pending_media_clear: AtomicBool,
    /// 媒体资源包是否已全部解压完成。
    pub media_ready: AtomicBool,
    pub media_done: AtomicUsize,
    pub media_total: AtomicUsize,
    pub media_error: Mutex<String>,
    pub character_drafts: Mutex<HashMap<String, serde_json::Value>>,
    pub _instance_lock: crate::lock::InstanceLock,
}

impl AppState {
    pub fn new(
        paths: Paths,
        config: Config,
        db: Db,
        instance_lock: crate::lock::InstanceLock,
    ) -> Self {
        Self {
            paths,
            config: RwLock::new(config),
            db,
            started_at: chrono::Utc::now().timestamp(),
            pending_media_clear: AtomicBool::new(false),
            media_ready: AtomicBool::new(false),
            media_done: AtomicUsize::new(0),
            media_total: AtomicUsize::new(0),
            media_error: Mutex::new(String::new()),
            character_drafts: Mutex::new(HashMap::new()),
            _instance_lock: instance_lock,
        }
    }

    pub fn config(&self) -> Config {
        self.config.read().expect("config lock poisoned").clone()
    }
}

static LOG_LOCK: Mutex<()> = Mutex::new(());

pub fn append_log(path: &std::path::Path, message: &str) {
    let _guard = LOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "[{}] {}", chrono::Local::now().to_rfc3339(), message);
    }
}
