use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

use anyhow::{Context, Result};
use chrono::Local;

use crate::config;

pub fn log_file_path(name: &str) -> Result<PathBuf> {
    Ok(config::config_dir()?.join(name))
}

pub fn append_timestamped_line(
    name: &str,
    level: &str,
    context: &str,
    message: &str,
) -> Result<()> {
    let path = log_file_path(name)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    let line = format!(
        "[{}] {level} {context}: {message}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    file.write_all(line.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))
}

pub fn append_timestamped_line_lossy(name: &str, level: &str, context: &str, message: &str) {
    if let Err(err) = append_timestamped_line(name, level, context, message) {
        eprintln!("failed to write {name}: {err:#}; original {level} {context}: {message}");
    }
}
