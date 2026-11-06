mod api;
mod config;
mod db;
mod ipc;
mod lock;
mod logging;
mod media;
mod paths;
mod state;

use tauri::Manager;

pub fn run() {
    let app = tauri::Builder::default()
        .register_uri_scheme_protocol("starlit", |ctx, request| {
            use tauri::Manager;

            let not_found = || -> tauri::http::Response<Vec<u8>> {
                tauri::http::Response::builder()
                    .status(404)
                    .body(Vec::new())
                    .expect("valid response")
            };
            let Some(state) = ctx.app_handle().try_state::<state::AppState>() else {
                return not_found();
            };
            let decoded = percent_encoding::percent_decode_str(request.uri().path())
                .decode_utf8_lossy();
            let relative = decoded.trim_start_matches('/');
            let base = match state.paths.data.canonicalize() {
                Ok(base) => base,
                Err(_) => return not_found(),
            };
            let full = match base.join(relative).canonicalize() {
                Ok(path) => path,
                Err(_) => return not_found(),
            };
            if !full.starts_with(&base) {
                return not_found();
            }
            let bytes = match std::fs::read(&full) {
                Ok(bytes) => bytes,
                Err(_) => return not_found(),
            };
            let mime = match full
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref()
            {
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("webp") => "image/webp",
                Some("gif") => "image/gif",
                Some("mp3") => "audio/mpeg",
                Some("wav") => "audio/wav",
                Some("webm") => "audio/webm",
                _ => "application/octet-stream",
            };
            tauri::http::Response::builder()
                .status(200)
                .header("Content-Type", mime)
                .header("Cache-Control", "private, max-age=86400")
                .body(bytes)
                .expect("valid response")
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let paths = paths::Paths::resolve(app.handle())?;
            paths.ensure()?;

            let instance_lock = match lock::InstanceLock::acquire(&paths.root) {
                Ok(lock) => lock,
                Err(lock::LockError::Busy) => {
                    use tauri_plugin_dialog::DialogExt;
                    let handle = app.handle().clone();
                    app.dialog()
                        .message("系统文件正在被占用，为避免数据损坏，请退出另一个实例")
                        .title("星伴")
                        .show(move |_| {
                            handle.exit(1);
                        });
                    return Ok(());
                }
            };

            logging::init(&paths.log_file())?;
            paths.seed()?;
            let config = paths.load_config()?;
            let db = db::Db::open(&paths.db_file())?;
            let log_file = paths.log_file();
            let paths_for_media = paths.clone();
            app.manage(state::AppState::new(paths, config, db, instance_lock));

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                use std::sync::atomic::Ordering;
                use tauri::Manager;

                let target = handle.clone();
                let errors = media::extract_all(&paths_for_media, |done, total| {
                    use tauri::Manager;
                    if let Some(state) = target.try_state::<state::AppState>() {
                        state.media_done.store(done, Ordering::Relaxed);
                        state.media_total.store(total, Ordering::Relaxed);
                    }
                });
                state::append_log(&log_file, "media cache ready");
                if let Some(state) = handle.try_state::<state::AppState>() {
                    if !errors.is_empty() {
                        *state.media_error.lock().expect("media error lock") = errors.join("; ");
                    }
                    state.media_ready.store(true, Ordering::Relaxed);
                }
                let _ = tauri::Emitter::emit(&handle, "media:ready", true);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::app_info,
            ipc::bootstrap,
            ipc::window_control,
            ipc::window_state,
            ipc::open_data_dir,
            ipc::read_log_tail,
            ipc::log_message,
            api::identity,
            api::list_characters,
            api::list_groups,
            api::list_messages,
            api::send_message,
            api::recall_message,
            api::mark_read,
            api::retry_reply,
            api::ping,
            api::api_request,
            api::media_cache_info,
            api::set_media_auto_clear,
            api::clear_media_cache,
            api::media_status,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            use std::sync::atomic::Ordering;
            use tauri::Manager;
            if let Some(state) = app_handle.try_state::<state::AppState>() {
                let pending = state.pending_media_clear.load(Ordering::Relaxed);
                if pending || api::auto_clear_enabled(&state) {
                    let _ = media::clear_cache(&state.paths);
                }
                lock::release(&state.paths.root);
            }
        }
    });
}
