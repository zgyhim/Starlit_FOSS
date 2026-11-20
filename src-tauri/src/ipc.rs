use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Serialize;
use tauri::{Emitter, State};

use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_root: String,
    pub resource_root: String,
    pub config_path: String,
    pub db_path: String,
    pub started_at: i64,
    pub release: bool,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        data_root: state.paths.root.display().to_string(),
        resource_root: state.paths.resources.display().to_string(),
        config_path: state.paths.config_file().display().to_string(),
        db_path: state.paths.db_file().display().to_string(),
        started_at: state.started_at,
        release: !cfg!(debug_assertions),
    }
}

#[derive(Serialize)]
pub struct WindowState {
    pub maximized: bool,
}

#[tauri::command]
pub fn window_control(window: tauri::WebviewWindow, action: String) -> Result<bool, String> {
    match action.as_str() {
        "close" => window.close().map_err(|e| e.to_string())?,
        #[cfg(desktop)]
        "minimize" => window.minimize().map_err(|e| e.to_string())?,
        #[cfg(desktop)]
        "maximize" => window.maximize().map_err(|e| e.to_string())?,
        #[cfg(desktop)]
        "unmaximize" => window.unmaximize().map_err(|e| e.to_string())?,
        _ => return Err(format!("当前平台不支持窗口操作: {action}")),
    }
    Ok(true)
}

#[tauri::command]
pub fn window_state(window: tauri::WebviewWindow) -> Result<WindowState, String> {
    #[cfg(desktop)]
    let maximized = window.is_maximized().map_err(|e| e.to_string())?;
    #[cfg(mobile)]
    let maximized = {
        let _ = &window;
        false
    };
    Ok(WindowState { maximized })
}

#[tauri::command]
pub fn show_about(app: tauri::AppHandle) {
    use tauri_plugin_dialog::DialogExt;
    let text = "星伴 FOSS 是「星伴」桌面应用的独立重实现。\n\n        前端来自对原版的逆向分析，保留界面、角色、人格和表情包；\n        后端改为 Tauri 2 + Rust 单进程，没有 OneBot、没有本地 HTTP 服务、没有 Python 后端。\n\n        聊天记录、长期记忆和配置只保存在本机。\n        项目以 AGPL-3.0 发布。非官方项目，原版角色与美术资源版权归原作者及相应权利人所有。\n\n        仓库：https://github.com/zgyhim/Starlit_FOSS";
    app.dialog()
        .message(text)
        .title("关于星伴 FOSS")
        .show(|_| {});
}

#[tauri::command]
pub fn toggle_devtools(window: tauri::WebviewWindow) -> Result<bool, String> {
    #[cfg(all(desktop, debug_assertions))]
    {
        if window.is_devtools_open() {
            window.close_devtools();
            return Ok(false);
        }
        window.open_devtools();
        return Ok(true);
    }
    #[cfg(not(all(desktop, debug_assertions)))]
    {
        let _ = window;
        Err("当前构建未启用开发者工具".into())
    }
}


#[tauri::command]
pub fn export_logs(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let dir = state.paths.log_dir();
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("log") {
                    continue;
                }
                let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("log");
                builder
                    .append_path_with_name(&path, format!("log/{name}"))
                    .map_err(|e| e.to_string())?;
            }
        }
        builder.finish().map_err(|e| e.to_string())?;
    }
    let compressed = zstd::stream::encode_all(&tar_bytes[..], 19).map_err(|e| e.to_string())?;
    let filename = format!(
        "starlit-logs-{}.tar.zst",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    Ok(serde_json::json!({
        "filename": filename,
        "size": compressed.len(),
        "data": BASE64.encode(compressed),
    }))
}

#[tauri::command]
pub fn clear_logs(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let dir = state.paths.log_dir();
    let mut removed = 0u64;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("log")
                && std::fs::remove_file(&path).is_ok()
            {
                removed += 1;
            }
        }
    }
    crate::logging::info("logs", &format!("cleared {removed} log file(s)"));
    Ok(serde_json::json!({ "removed": removed }))
}

#[tauri::command]
pub fn open_data_dir(state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(desktop)]
    {
        open::that(&state.paths.root).map_err(|e| format!("打开数据目录失败: {e}"))
    }
    #[cfg(mobile)]
    {
        let _ = state;
        Err("Android 暂不支持直接打开数据目录".into())
    }
}

#[tauri::command]
pub fn read_log_tail(state: State<'_, AppState>, lines: Option<usize>) -> Result<String, String> {
    let path = state.paths.log_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(e.to_string()),
    };
    let limit = lines.unwrap_or(200);
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(limit);
    Ok(all[start..].join("\n"))
}

#[tauri::command]
pub fn log_message(_state: State<'_, AppState>, message: String) {
    crate::logging::info("ui", &message);
}

/// 前端启动时读取，避免首屏需要多次 IPC。
#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> AppInfo {
    app_info(state)
}

#[allow(dead_code)]
pub fn emit(app: &tauri::AppHandle, event: &str, payload: impl Serialize + Clone) {
    let _ = app.emit(event, payload);
}
