use std::fs;
use std::path::{Path, PathBuf};
use std::process;

/// 运行期“君子锁”：声明数据库与资源目录正在被本进程占用。
pub struct InstanceLock {
    files: Vec<PathBuf>,
}

pub enum LockError {
    Busy,
}

impl InstanceLock {
    pub fn acquire(root: &Path) -> Result<Self, LockError> {
        let files = vec![
            root.join(".starlit.lock"),
            root.join("data").join("companion.db.lock"),
            root.join("data").join("media.lock"),
        ];
        // 君子锁：只要文件存在就视为被占用，不再根据 PID 判断死活。
        if files.iter().any(|file| file.exists()) {
            return Err(LockError::Busy);
        }

        let exe = std::env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        for file in &files {
            if let Some(parent) = file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let content = format!("pid={}\nexe={}\n", process::id(), exe);
            if fs::write(file, content).is_err() {
                for created in &files {
                    let _ = fs::remove_file(created);
                }
                return Err(LockError::Busy);
            }
        }
        Ok(Self { files })
    }
}

/// 退出时显式移除；Tauri 的 managed state 不保证 Drop。
pub fn release(root: &Path) {
    for file in [
        root.join(".starlit.lock"),
        root.join("data").join("companion.db.lock"),
        root.join("data").join("media.lock"),
    ] {
        let _ = fs::remove_file(file);
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        for file in &self.files {
            let _ = fs::remove_file(file);
        }
    }
}
