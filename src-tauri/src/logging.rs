use std::fs::{self, OpenOptions};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Fatal = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl Level {
    fn letter(self) -> &'static str {
        match self {
            Level::Fatal => "F",
            Level::Error => "E",
            Level::Warn => "W",
            Level::Info => "I",
            Level::Debug => "D",
            Level::Trace => "V",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Level::Fatal => "\x1b[1;35m",
            Level::Error => "\x1b[1;31m",
            Level::Warn => "\x1b[1;33m",
            Level::Info => "\x1b[1;32m",
            Level::Debug => "\x1b[1;34m",
            Level::Trace => "\x1b[1;90m",
        }
    }
}

static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();
static EMITTER: OnceLock<tauri::AppHandle> = OnceLock::new();
static MIN_LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);
static NEXT_TID: AtomicU64 = AtomicU64::new(1);
static WRITE_LOCK: Mutex<()> = Mutex::new(());

const TAG_COLOR: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";
const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;

thread_local! {
    static TID: u64 = NEXT_TID.fetch_add(1, Ordering::Relaxed);
}

fn force_exit(app: Option<tauri::AppHandle>) -> ! {
    if let Some(app) = app {
        app.exit(1);
    }
    std::thread::sleep(std::time::Duration::from_millis(150));
    #[cfg(unix)]
    unsafe {
        libc::_exit(1);
    }
    #[cfg(not(unix))]
    std::process::exit(1);
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
            let detail = if let Some(text) = info.payload().downcast_ref::<&str>() {
                (*text).to_string()
            } else if let Some(text) = info.payload().downcast_ref::<String>() {
                text.clone()
            } else {
                "未知错误".to_string()
            };
            fatal("panic", &format!("{detail} @ {location}"));
            // 致命错误不推给前端；尝试用系统对话框告知用户，并附上详情。
            let mut dialog_message = String::new();
            if let Some(app) = EMITTER.get() {
                use tauri_plugin_dialog::DialogExt;
                let log_path = LOG_FILE
                    .get()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "desktop.log".into());
                let detail = if detail.chars().count() > 800 {
                    format!("{}…", detail.chars().take(800).collect::<String>())
                } else {
                    detail
                };
                dialog_message = format!(
                    "应用程序遇到错误，无法继续运行。\n\n\
                     错误：{detail}\n\
                     位置：{location}\n\
                     日志：{log_path}\n\n\
                     请重新启动星伴；如果反复出现，请把日志一并反馈。"
                );

                let is_main = std::thread::current().name() == Some("main");
                if is_main {
                    // 主线程 panic 时事件循环无法再处理对话框，只能短暂展示后退出。
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        app.dialog()
                            .message(&dialog_message)
                            .title("星伴 · 致命错误")
                            .show(|_| {});
                    }));
                    std::thread::sleep(std::time::Duration::from_millis(600));
                } else {
                    // 后台线程：对话框能正常运行时，阻塞等待用户关闭。
                    let app_for_watchdog = app.clone();
                    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let finished_for_watchdog = finished.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(120));
                        if !finished_for_watchdog.load(std::sync::atomic::Ordering::Relaxed) {
                            force_exit(Some(app_for_watchdog));
                        }
                    });
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        app.dialog()
                            .message(&dialog_message)
                            .title("星伴 · 致命错误")
                            .blocking_show();
                    }));
                    finished.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            default_hook(info);

            // 明确退出进程：多个手段依次兜底。
            force_exit(EMITTER.get().cloned());
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

/// 把 ERROR 级别日志同时推给前端。
pub fn set_emitter(app: tauri::AppHandle) {
    let _ = EMITTER.set(app);
}

pub fn set_level(level: Level) {
    MIN_LEVEL.store(level as u8, Ordering::Relaxed);
}

/// 致命级别：只用于 panic / 无法恢复的错误，不推送到前端。
pub fn fatal(tag: &str, message: &str) {
    log(Level::Fatal, tag, message);
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
    if level != Level::Fatal && (level as u8) > MIN_LEVEL.load(Ordering::Relaxed) {
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

    if level == Level::Error {
        if let Some(app) = EMITTER.get() {
            use tauri::Emitter;
            let _ = app.emit(
                "log:error",
                serde_json::json!({
                    "tag": tag,
                    "message": message,
                    "ts": now.timestamp_millis(),
                }),
            );
        }
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
