use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;

pub struct Paths {
    pub root: PathBuf,
    pub data: PathBuf,
    pub persona: PathBuf,
    pub backups: PathBuf,
    pub run: PathBuf,
    pub resources: PathBuf,
}

impl Paths {
    pub fn resolve(app: &tauri::AppHandle) -> Result<Self, String> {
        let root = match std::env::var_os("STARLIT_DATA_ROOT") {
            Some(v) if !v.is_empty() => PathBuf::from(v),
            _ => data_dir()?.join("starlit-desktop"),
        };
        let resources = match std::env::var_os("STARLIT_RESOURCE_ROOT") {
            Some(v) if !v.is_empty() => PathBuf::from(v),
            _ => resolve_resources(app),
        };
        Ok(Self {
            data: root.join("data"),
            persona: root.join("persona"),
            backups: root.join("backups"),
            run: root.join("run"),
            root,
            resources,
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.yaml")
    }

    pub fn db_file(&self) -> PathBuf {
        self.data.join("companion.db")
    }

    pub fn log_file(&self) -> PathBuf {
        self.data.join("desktop.log")
    }

    pub fn template_dir(&self) -> PathBuf {
        let nested = self.resources.join("resources").join("template");
        if nested.is_dir() {
            nested
        } else {
            self.resources.join("template")
        }
    }

    pub fn ensure(&self) -> Result<(), String> {
        for dir in [
            &self.data,
            &self.persona,
            &self.backups.join("lan-sync"),
            &self.backups.join("updates"),
            &self.run,
            &self.data.join("stickers"),
            &self.data.join("sticker_thumbs"),
            &self.data.join("avatars"),
            &self.data.join("groups"),
            &self.data.join("sync-temp"),
        ] {
            fs::create_dir_all(dir).map_err(|e| format!("创建目录 {} 失败: {e}", dir.display()))?;
        }
        Ok(())
    }

    /// 首次运行：复制人格模板与旧数据索引（表情包本体体积过大，后续按需处理）。
    pub fn seed(&self) -> Result<(), String> {
        let template = self.template_dir();
        if !template.is_dir() {
            return Ok(());
        }
        if !self.persona.exists() {
            copy_dir(&template.join("persona"), &self.persona)?;
        }
        let seed_data = template.join("data");
        if seed_data.is_dir() {
            for name in [
                "characters.json",
                "character_traits.json",
                "preset.json",
                "presence.json",
                "groups.json",
                "stickers/index.json",
            ] {
                let from = seed_data.join(name);
                let to = self.data.join(name);
                if from.is_file() && !to.exists() {
                    if let Some(parent) = to.parent() {
                        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    fs::copy(&from, &to).map_err(|e| {
                        format!("复制 {} 失败: {e}", from.display())
                    })?;
                }
            }
        }
        Ok(())
    }

    pub fn load_config(&self) -> Result<Config, String> {
        let file = self.config_file();
        if !file.exists() {
            if let Some(parent) = file.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let template = self.template_dir().join("config.yaml");
            if template.is_file() {
                fs::copy(&template, &file).map_err(|e| e.to_string())?;
                let mut cfg = read_config(&file)?;
                // 模板中可能带有作者账号，重实现必须留空。
                cfg.qq.self_id.clear();
                cfg.qq.owner_id.clear();
                write_config(&file, &cfg)?;
                return Ok(cfg);
            }
            let cfg = Config::default();
            write_config(&file, &cfg)?;
            return Ok(cfg);
        }
        read_config(&file)
    }
}

fn read_config(file: &Path) -> Result<Config, String> {
    let text = fs::read_to_string(file).map_err(|e| format!("读取配置失败: {e}"))?;
    serde_yaml::from_str(&text).map_err(|e| format!("解析 {} 失败: {e}", file.display()))
}

pub fn write_config(file: &Path, cfg: &Config) -> Result<(), String> {
    let text = serde_yaml::to_string(cfg).map_err(|e| e.to_string())?;
    let tmp = file.with_extension("yaml.tmp");
    fs::write(&tmp, text).map_err(|e| format!("写入配置失败: {e}"))?;
    fs::rename(&tmp, file).map_err(|e| format!("替换配置失败: {e}"))
}

fn data_dir() -> Result<PathBuf, String> {
    if cfg!(target_os = "windows") {
        if let Some(v) = std::env::var_os("APPDATA") {
            return Ok(PathBuf::from(v));
        }
    } else if cfg!(target_os = "macos") {
        if let Some(v) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(v).join("Library/Application Support"));
        }
    } else {
        if let Some(v) = std::env::var_os("XDG_DATA_HOME") {
            return Ok(PathBuf::from(v));
        }
        if let Some(v) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(v).join(".local/share"));
        }
    }
    Err("无法确定用户数据目录，请设置 STARLIT_DATA_ROOT".into())
}

fn resolve_resources(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    if cfg!(debug_assertions) {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../resources");
        if dev.is_dir() {
            return dev;
        }
    }
    app.path()
        .resource_dir()
        .map(|p| p.join("resources"))
        .unwrap_or_else(|_| PathBuf::from("resources"))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| format!("读取 {} 失败: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
