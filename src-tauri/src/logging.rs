use std::fs::{self, OpenOptions};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl Level {
    fn letter(self) -> &'static str {
        match self {
            Level::Error => "E",
            Level::Warn => "W",
            Level::Info => "I",
            Level::Debug => "D",
            Level::Trace => "V",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Level::Error => "\x1b[1;31m",
            Level::Warn => "\x1b[1;33m",
            Level::Info => "\x1b[1;32m",
            Level::Debug => "\x1b[1;34m",
            Level::Trace => "\x1b[1;90m",
        }
    }
}

static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();
static MIN_LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);
static NEXT_TID: AtomicU64 = AtomicU64::new(1);
static WRITE_LOCK: Mutex<()> = Mutex::new(());

const TAG_COLOR: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";
const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;

thread_local! {
    static TID: u64 = NEXT_TID.fetch_add(1, Ordering::Relaxed);
}

/// 初始化日志文件、级别与 panic hook。可重复调用，只生效一次。
pub fn init(log_file: &Path) -> Result<(), String> {
    if let Some(parent) = log_file.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    rotate_if_needed(log_file);

    if LOG_FILE.set(log_file.to_path_buf()).is_ok() {
        if let Ok(value) = std::env::var("STARLIT_LOG_LEVEL") {
            let level = match value.to_ascii_lowercase().as_str() {
                "error" | "e" => Level::Error,
                "warn" | "w" => Level::Warn,
                "debug" | "d" => Level::Debug,
                "trace" | "v" => Level::Trace,
                _ => Level::Info,
            };
            MIN_LEVEL.store(level as u8, Ordering::Relaxed);
        }
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let location = info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_else(|| "unknown".into());
            error("panic", &format!("{info} @ {location}"));
            default_hook(info);
        }));
        info(
            "app",
            &format!(
                "desktop started · pid {} · v{}",
                std::process::id(),
                env!("CARGO_PKG_VERSION")
            ),
        );
    }
    Ok(())
}

fn rotate_if_needed(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() > MAX_LOG_BYTES {
            let backup = path.with_extension("log.1");
            let _ = fs::remove_file(&backup);
            let _ = fs::rename(path, backup);
        }
    }
}

pub fn set_level(level: Level) {
    MIN_LEVEL.store(level as u8, Ordering::Relaxed);
}

pub fn error(tag: &str, message: &str) {
    log(Level::Error, tag, message);
}

pub fn warn(tag: &str, message: &str) {
    log(Level::Warn, tag, message);
}

pub fn info(tag: &str, message: &str) {
    log(Level::Info, tag, message);
}

pub fn debug(tag: &str, message: &str) {
    log(Level::Debug, tag, message);
}

pub fn trace(tag: &str, message: &str) {
    log(Level::Trace, tag, message);
}

/// logcat 风格：`11-14 23:45:01.123  1234  5678 I/StarLit(media): message`
pub fn log(level: Level, tag: &str, message: &str) {
    if (level as u8) > MIN_LEVEL.load(Ordering::Relaxed) {
        return;
    }
    let now = chrono::Local::now();
    let timestamp = now.format("%m-%d %H:%M:%S%.3f");
    let pid = std::process::id();
    let tid = TID.with(|id| *id);
    let tag = if tag.is_empty() { "app" } else { tag };

    let color_mode = std::env::var("STARLIT_LOG_COLOR").unwrap_or_default();
    let use_color = color_mode == "always"
        || (color_mode != "never" && std::io::stderr().is_terminal());
    if use_color {
        eprintln!(
            "\x1b[90m{timestamp}\x1b[0m {pid:>6} {tid:>5} {}{}{RESET}/{TAG_COLOR}StarLit({tag}){RESET}: {message}",
            level.color(),
            level.letter(),
        );
    } else {
        eprintln!("{timestamp} {pid:>6} {tid:>5} {}/StarLit({tag}): {message}", level.letter());
    }

    let Some(path) = LOG_FILE.get() else {
        return;
    };
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(
            file,
            "{timestamp} {pid:>6} {tid:>5} {}/StarLit({tag}): {message}",
            level.letter()
        );
    }
}
