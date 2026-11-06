use std::path::Path;

pub fn init(log_file: &Path) -> Result<(), String> {
    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::state::append_log(log_file, "desktop started");
    Ok(())
}
