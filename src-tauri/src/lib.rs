mod api;
mod config;
mod db;
mod ipc;
mod logging;
mod media;
mod paths;
mod state;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
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
            logging::init(&paths.log_file())?;
            paths.seed()?;
            let config = paths.load_config()?;
            let db = db::Db::open(&paths.db_file())?;
            media::extract_startup(&paths)?;

            let background_paths = paths.clone();
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let _ = media::extract_background(&background_paths);
                let _ = tauri::Emitter::emit(&handle, "media:ready", true);
            });

            app.manage(state::AppState::new(paths, config, db));
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
