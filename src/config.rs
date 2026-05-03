// config — 配置與數據文件路徑管理。
// 默認使用 portable 模式，配置目錄為可執行文件同目錄下的 .config 文件夾；
// 可設定環境變量 TSML_HOME 來覆蓋數據目錄，適用於測試或臨時隔離場景。
// 所有模塊應通過 config_dir() / config_file() / todos_file() 獲取路徑，
// 不直接拼接路徑字符串。

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use dialoguer::{Confirm, Input};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub notes_dir: PathBuf,
}

impl AppConfig {
    pub fn load_or_init() -> Result<Self> {
        let path = config_file()?;
        if path.exists() {
            return read_config(&path);
        }

        println!("No config found. Initializing tsml.");
        init_config_interactive()
    }
}

pub fn init_config_interactive() -> Result<AppConfig> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create config directory {}", dir.display()))?;

    let default_notes_dir = dir.join("notes");
    let default_notes_dir_text = default_notes_dir.display().to_string();
    let notes_dir_text: String = Input::new()
        .with_prompt("Markdown notes directory")
        .default(default_notes_dir_text)
        .interact_text()?;

    let notes_dir = PathBuf::from(notes_dir_text);
    fs::create_dir_all(&notes_dir)
        .with_context(|| format!("failed to create notes directory {}", notes_dir.display()))?;

    let config = AppConfig { notes_dir };
    write_config(&config)?;
    ensure_todos_file()?;

    println!("Initialized {}", config_file()?.display());
    Ok(config)
}

pub fn show_config() -> Result<()> {
    let config = AppConfig::load_or_init()?;
    println!("config: {}", config_file()?.display());
    println!("notes_dir: {}", config.notes_dir.display());
    println!("todos: {}", todos_file()?.display());
    Ok(())
}

pub fn set_notes_dir(path: String) -> Result<()> {
    let mut config = AppConfig::load_or_init()?;
    let notes_dir = PathBuf::from(path);

    if !notes_dir.exists() {
        let create = Confirm::new()
            .with_prompt(format!(
                "{} does not exist. Create it?",
                notes_dir.display()
            ))
            .default(true)
            .interact()?;
        if create {
            fs::create_dir_all(&notes_dir)
                .with_context(|| format!("failed to create {}", notes_dir.display()))?;
        }
    }

    config.notes_dir = notes_dir;
    write_config(&config)?;
    println!("Updated notes_dir.");
    Ok(())
}

pub fn config_dir() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("TSML_HOME") {
        return Ok(PathBuf::from(home));
    }

    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let exe_dir = exe
        .parent()
        .map(Path::to_path_buf)
        .context("failed to locate executable directory")?;
    Ok(exe_dir.join(".config"))
}

pub fn config_file() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn todos_file() -> Result<PathBuf> {
    Ok(config_dir()?.join("todos.json"))
}

fn read_config(path: &Path) -> Result<AppConfig> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_config(config: &AppConfig) -> Result<()> {
    let path = config_file()?;
    let raw = toml::to_string_pretty(config).context("failed to serialize config")?;
    fs::write(&path, raw).with_context(|| format!("failed to write {}", path.display()))
}

pub fn ensure_todos_file() -> Result<()> {
    let path = todos_file()?;
    if !path.exists() {
        fs::write(&path, "[]\n").with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}
