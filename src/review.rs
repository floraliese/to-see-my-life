use std::fs;

use anyhow::{Context, Result};
use chrono::Local;
use dialoguer::{Input, theme::ColorfulTheme};

use crate::config::AppConfig;

pub fn run_review() -> Result<()> {
    let config = AppConfig::load_or_init()?;
    fs::create_dir_all(&config.notes_dir).with_context(|| {
        format!(
            "failed to create notes directory {}",
            config.notes_dir.display()
        )
    })?;

    let theme = ColorfulTheme::default();
    let did: String = Input::with_theme(&theme)
        .with_prompt("今天做了什麼")
        .interact_text()?;
    let goals: String = Input::with_theme(&theme)
        .with_prompt("完成了什麼目標")
        .interact_text()?;
    let mood: String = Input::with_theme(&theme)
        .with_prompt("心情如何")
        .interact_text()?;
    let tomorrow: String = Input::with_theme(&theme)
        .with_prompt("明天想做什麼")
        .interact_text()?;

    let today = Local::now().date_naive();
    let file = config.notes_dir.join(format!("{}-daily-review.md", today));
    let content = format!(
        "\
# Daily Review - {today}

## 今天做了什麼？

{did}

## 完成了什麼目標？

{goals}

## 心情如何？

{mood}

## 明天想做什麼？

{tomorrow}
"
    );

    fs::write(&file, content).with_context(|| format!("failed to write {}", file.display()))?;
    println!("Wrote {}", file.display());
    Ok(())
}
