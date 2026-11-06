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

/// 解压单个归档；失败时保留部分文件但不写缓存标记，下次重试。
fn extract_one(paths: &Paths, entry: &ArchiveEntry) -> Result<(), String> {
    if looks_extracted(paths, entry) {
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
        // unpack_in 会在目标目录外时拒绝写入。
        item.unpack_in(&base)
            .map_err(|e| format!("写入 {} 失败: {e}", path.display()))?;
    }

    let marker = marker_path(paths, &entry.sha256);
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&marker, format!("{}\n", entry.name)).map_err(|e| e.to_string())?;
    Ok(())
}

/// 启动阶段：解压体积小、首屏需要的资源。
pub fn extract_startup(paths: &Paths) -> Result<(), String> {
    let Some(manifest) = load_manifest(&paths.resources) else {
        return Ok(());
    };
    for entry in manifest
        .archives
        .iter()
        .filter(|entry| entry.scope == "startup")
    {
        if let Err(error) = extract_one(paths, entry) {
            crate::state::append_log(&paths.log_file(), &format!("media: {error}"));
        }
    }
    Ok(())
}

/// 后台阶段：表情包合集较大，不阻塞窗口。
pub fn extract_background(paths: &Paths) -> Result<(), String> {
    let Some(manifest) = load_manifest(&paths.resources) else {
        return Ok(());
    };
    for entry in manifest
        .archives
        .iter()
        .filter(|entry| entry.scope == "background")
    {
        if let Err(error) = extract_one(paths, entry) {
            crate::state::append_log(&paths.log_file(), &format!("media: {error}"));
        }
    }
    Ok(())
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
