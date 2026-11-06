use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::paths::Paths;

#[derive(Debug, Deserialize)]
struct Manifest {
    #[allow(dead_code)]
    version: u32,
    archives: Vec<ArchiveEntry>,
}

#[derive(Debug, Deserialize)]
struct ArchiveEntry {
    name: String,
    target: String,
    sha256: String,
    /// 解压后的总字节数，用于进度条。
    #[serde(default)]
    uncompressed: u64,
    /// startup: 启动时同步解压；background: 后台解压。
    scope: String,
}

fn load_manifest(resources: &Path) -> Option<Manifest> {
    let path = resources.join("media").join("manifest.json");
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn marker_path(paths: &Paths, sha: &str) -> PathBuf {
    paths.root.join(".cache").join("media").join(format!("{sha}.done"))
}

fn looks_extracted(paths: &Paths, entry: &ArchiveEntry) -> bool {
    marker_path(paths, &entry.sha256).is_file()
}

fn record_path(paths: &Paths, sha: &str) -> PathBuf {
    paths.root.join(".cache").join("media").join(format!("{sha}.files"))
}

/// 读取归档条目，生成“解压产物清单”，供退出清理时精确删除。
fn write_file_record(paths: &Paths, entry: &ArchiveEntry) -> Result<(), String> {
    let record = record_path(paths, &entry.sha256);
    if record.is_file() {
        return Ok(());
    }
    let archive = paths.resources.join("media").join(&entry.name);
    let file = fs::File::open(&archive).map_err(|e| e.to_string())?;
    let decoder = zstd::Decoder::new(io::BufReader::new(file)).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(decoder);
    let mut lines = String::new();
    for item in tar.entries().map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        if !item.header().entry_type().is_file() {
            continue;
        }
        let path = item.path().map_err(|e| e.to_string())?;
        if path.is_absolute()
            || path.components().any(|c| matches!(c, std::path::Component::ParentDir))
        {
            continue;
        }
        lines.push_str(&entry.target);
        lines.push('/');
        lines.push_str(&path.to_string_lossy());
        lines.push('\n');
    }
    if let Some(parent) = record.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(record, lines).map_err(|e| e.to_string())
}

/// 解压单个归档；失败时保留部分文件但不写缓存标记，下次重试。
fn extract_one(
    paths: &Paths,
    entry: &ArchiveEntry,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<(), String> {
    if looks_extracted(paths, entry) {
        let _ = write_file_record(paths, entry);
        on_bytes(entry.uncompressed);
        return Ok(());
    }
    let archive = paths.resources.join("media").join(&entry.name);
    let file = fs::File::open(&archive)
        .map_err(|e| format!("打开资源包 {} 失败: {e}", archive.display()))?;
    let decoder = zstd::Decoder::new(io::BufReader::new(file))
        .map_err(|e| format!("解压 {} 失败: {e}", entry.name))?;
    let mut tar = tar::Archive::new(decoder);
    let base = paths.root.join(&entry.target);
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;

    let entries = tar.entries().map_err(|e| format!("读取 {} 失败: {e}", entry.name))?;
    for item in entries {
        let mut item = item.map_err(|e| format!("读取条目失败: {e}"))?;
        let path = item
            .path()
            .map_err(|e| format!("非法路径: {e}"))?
            .to_path_buf();
        if path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            continue;
        }
        let size = item.header().size().unwrap_or(0);
        // unpack_in 会在目标目录外时拒绝写入。
        item.unpack_in(&base)
            .map_err(|e| format!("写入 {} 失败: {e}", path.display()))?;
        on_bytes(size);
    }

    let _ = write_file_record(paths, entry);

    let marker = marker_path(paths, &entry.sha256);
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&marker, format!("{}\n", entry.name)).map_err(|e| e.to_string())?;
    Ok(())
}

/// 解压全部资源包，并在每完成一个后回调进度。
pub fn extract_all(paths: &Paths, progress: impl Fn(usize, usize)) {
    let Some(manifest) = load_manifest(&paths.resources) else {
        progress(0, 0);
        return;
    };
    let total: u64 = manifest.archives.iter().map(|entry| entry.uncompressed).sum();
    let total = if total == 0 { manifest.archives.len() as u64 } else { total };
    progress(0, total as usize);
    let mut done = 0u64;
    for entry in &manifest.archives {
        let weight = if entry.uncompressed == 0 { 1 } else { entry.uncompressed };
        let mut on_bytes = |bytes: u64| {
            done = done.saturating_add(bytes);
            progress(done.min(total) as usize, total as usize);
        };
        if let Err(error) = extract_one(paths, entry, &mut on_bytes) {
            crate::state::append_log(&paths.log_file(), &format!("media: {error}"));
        }
        if entry.uncompressed == 0 {
            done = done.saturating_add(1);
            progress(done.min(total) as usize, total as usize);
        }
    }
}

/// 删除所有由资源包解压出来的文件，保留用户自己上传的内容。
pub fn clear_cache(paths: &Paths) -> Result<(), String> {
    let cache = paths.root.join(".cache").join("media");
    let Ok(entries) = fs::read_dir(&cache) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("files") {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.contains("..") {
                    continue;
                }
                let target = paths.root.join(line);
                if target.starts_with(&paths.root) && target.is_file() {
                    let _ = fs::remove_file(target);
                }
            }
        }
    }
    if let Ok(entries) = fs::read_dir(&cache) {
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

pub fn cache_stats(paths: &Paths) -> (usize, u64) {
    let cache = paths.root.join(".cache").join("media");
    let Ok(entries) = fs::read_dir(&cache) else {
        return (0, 0);
    };
    let mut archives = 0;
    let mut bytes = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("files") {
            archives += 1;
            if let Ok(text) = fs::read_to_string(&path) {
                for line in text.lines() {
                    let target = paths.root.join(line.trim());
                    if let Ok(meta) = fs::metadata(&target) {
                        bytes += meta.len();
                    }
                }
            }
        }
    }
    (archives, bytes)
}

#[allow(dead_code)]
pub fn read_entry(paths: &Paths, archive: &str, inner: &str) -> Option<Vec<u8>> {
    let file = fs::File::open(paths.resources.join("media").join(archive)).ok()?;
    let decoder = zstd::Decoder::new(io::BufReader::new(file)).ok()?;
    let mut tar = tar::Archive::new(decoder);
    let entries = tar.entries().ok()?;
    for item in entries.flatten() {
        let path = item.path().ok()?.to_path_buf();
        if path.to_string_lossy() == inner {
            let mut buf = Vec::new();
            let mut item = item;
            item.read_to_end(&mut buf).ok()?;
            return Some(buf);
        }
    }
    None
}
