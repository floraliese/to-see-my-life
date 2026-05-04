// review — 每日回顧模塊。
// 互動式提問（做了什麼、完成了什麼目標、心情如何、明天想做什麼），
// 並在 Markdown 文件末尾自動追加今日 todo 摘要（完成/過期/未完成數量與列表）。
// 回顧文件保存在 config.notes_dir 目錄下，文件名為 YYYY-MM-DD-daily-review.md。

use std::fs;

use anyhow::{Context, Result};
use chrono::Local;
use dialoguer::{Input, theme::ColorfulTheme};

use crate::{config::AppConfig, timer, todo};

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
    let now = Local::now();
    // TODO(manager): get this summary via TodoManager once todo management is extracted.
    let todo_summary = todo::summarize_day(todo::load_todos()?, today, now).markdown(now);
    // Obsidian owns the free-form template for now; tsml only appends the
    // structured facts it knows how to compute reliably.
    let timer_summary = timer::timer_sessions_markdown(today)?;
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

{todo_summary}

{timer_summary}
"
    );

    fs::write(&file, content).with_context(|| format!("failed to write {}", file.display()))?;
    println!("Wrote {}", file.display());
    Ok(())
}
