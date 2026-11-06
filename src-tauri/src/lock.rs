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
        for file in &files {
            if !file.exists() {
                continue;
            }
            if let Some(pid) = read_pid(file) {
                if pid != process::id() && process_alive(pid) {
                    return Err(LockError::Busy);
                }
            }
            // 上个进程异常退出留下的锁：进程已不在，接管。
            let _ = fs::remove_file(file);
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

fn read_pid(path: &Path) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("pid="))
        .and_then(|value| value.trim().parse().ok())
}

#[cfg(target_os = "linux")]
fn process_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
fn process_alive(_pid: u32) -> bool {
    true
}
