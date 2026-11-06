use serde::Serialize;
use tauri::{Emitter, State};

use crate::state::{append_log, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_root: String,
    pub resource_root: String,
    pub config_path: String,
    pub db_path: String,
    pub started_at: i64,
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
    }
}

#[derive(Serialize)]
pub struct WindowState {
    pub maximized: bool,
}

#[tauri::command]
pub fn window_control(window: tauri::WebviewWindow, action: String) -> Result<bool, String> {
    match action.as_str() {
        "minimize" => window.minimize().map_err(|e| e.to_string())?,
        "maximize" => window.maximize().map_err(|e| e.to_string())?,
        "unmaximize" => window.unmaximize().map_err(|e| e.to_string())?,
        "close" => window.close().map_err(|e| e.to_string())?,
        _ => return Err(format!("未知窗口操作: {action}")),
    }
    Ok(true)
}

#[tauri::command]
pub fn window_state(window: tauri::WebviewWindow) -> Result<WindowState, String> {
    Ok(WindowState {
        maximized: window.is_maximized().map_err(|e| e.to_string())?,
    })
}

#[tauri::command]
pub fn open_data_dir(state: State<'_, AppState>) -> Result<(), String> {
    open::that(&state.paths.root).map_err(|e| format!("打开数据目录失败: {e}"))
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
pub fn log_message(state: State<'_, AppState>, message: String) {
    append_log(&state.paths.log_file(), &message);
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
