use std::fs;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, Local};
use dialoguer::{Input, Select, theme::ColorfulTheme};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    config,
    util::{format_minutes, parse_duration_minutes, parse_start_time},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Scheduled,
    Active,
    Done,
    Cancelled,
    Expired,
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            TodoStatus::Scheduled => "scheduled",
            TodoStatus::Active => "active",
            TodoStatus::Done => "done",
            TodoStatus::Cancelled => "cancelled",
            TodoStatus::Expired => "expired",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: String,
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub duration_minutes: i64,
    pub status: TodoStatus,
    pub focused_minutes: i64,
    pub created_at: DateTime<Local>,
}

impl Todo {
    pub fn effective_status(&self, now: DateTime<Local>) -> TodoStatus {
        match self.status {
            TodoStatus::Done | TodoStatus::Cancelled => self.status.clone(),
            _ if now >= self.end => TodoStatus::Expired,
            _ if now >= self.start => TodoStatus::Active,
            _ => TodoStatus::Scheduled,
        }
    }
}

pub fn add_todo(
    title_arg: Option<String>,
    start_arg: Option<String>,
    duration_arg: Option<String>,
) -> Result<()> {
    config::ensure_todos_file()?;
    let theme = ColorfulTheme::default();
    let now = Local::now();

    let title = match title_arg {
        Some(value) if !value.trim().is_empty() => value,
        _ => Input::with_theme(&theme)
            .with_prompt("Todo title")
            .interact_text()?,
    };

    let start = match start_arg {
        Some(value) => parse_start_time(&value, now)?,
        None => {
            let value: String = Input::with_theme(&theme)
                .with_prompt("Start time")
                .default("now".to_string())
                .interact_text()?;
            parse_start_time(&value, now)?
        }
    };

    let duration_minutes = match duration_arg {
        Some(value) => parse_duration_minutes(&value)?,
        None => {
            let value: String = Input::with_theme(&theme)
                .with_prompt("Duration")
                .default("25m".to_string())
                .interact_text()?;
            parse_duration_minutes(&value)?
        }
    };

    let end = start + Duration::minutes(duration_minutes);
    let todo = Todo {
        id: Uuid::new_v4().simple().to_string()[..8].to_string(),
        title,
        start,
        end,
        duration_minutes,
        status: TodoStatus::Scheduled,
        focused_minutes: 0,
        created_at: now,
    };

    let mut todos = load_todos()?;
    todos.push(todo.clone());
    sort_todos(&mut todos);
    save_todos(&todos)?;

    println!(
        "Added {} [{} - {}, {}]",
        todo.title,
        todo.start.format("%Y-%m-%d %H:%M"),
        todo.end.format("%H:%M"),
        format_minutes(todo.duration_minutes)
    );
    Ok(())
}

pub fn list_todos(all: bool) -> Result<()> {
    config::ensure_todos_file()?;
    let now = Local::now();
    let mut todos = load_todos()?;
    sort_todos(&mut todos);

    let visible: Vec<_> = todos
        .iter()
        .filter(|todo| all || !matches!(todo.status, TodoStatus::Done | TodoStatus::Cancelled))
        .collect();

    if visible.is_empty() {
        println!("No todos.");
        return Ok(());
    }

    for todo in visible {
        let status = todo.effective_status(now);
        println!(
            "{}  {:<9}  {}-{}  {:<8}  {}",
            todo.id,
            status,
            todo.start.format("%Y-%m-%d %H:%M"),
            todo.end.format("%H:%M"),
            format_minutes(todo.duration_minutes),
            todo.title
        );
    }
    Ok(())
}

pub fn mark_done(id_arg: Option<String>) -> Result<()> {
    config::ensure_todos_file()?;
    let mut todos = load_todos()?;
    let id = match id_arg {
        Some(value) => value,
        None => choose_open_todo(&todos)?,
    };

    let todo = todos
        .iter_mut()
        .find(|todo| todo.id == id)
        .ok_or_else(|| anyhow!("todo not found: {id}"))?;
    todo.status = TodoStatus::Done;
    todo.focused_minutes = todo.duration_minutes;
    save_todos(&todos)?;
    println!("Marked {id} done.");
    Ok(())
}

pub fn load_todos() -> Result<Vec<Todo>> {
    let path = config::todos_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn save_todos(todos: &[Todo]) -> Result<()> {
    let path = config::todos_file()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(todos).context("failed to serialize todos")?;
    fs::write(&path, format!("{raw}\n"))
        .with_context(|| format!("failed to write {}", path.display()))
}

pub fn sort_todos(todos: &mut [Todo]) {
    todos.sort_by_key(|todo| todo.start);
}

pub fn find_timer_todo(todos: &[Todo], now: DateTime<Local>) -> Option<Todo> {
    let mut open: Vec<_> = todos
        .iter()
        .filter(|todo| !matches!(todo.status, TodoStatus::Done | TodoStatus::Cancelled))
        .cloned()
        .collect();
    sort_todos(&mut open);

    open.iter()
        .find(|todo| now >= todo.start && now < todo.end)
        .cloned()
        .or_else(|| open.into_iter().find(|todo| todo.start > now))
}

pub fn update_todo_after_timer(id: &str, done: bool, focused_minutes: i64) -> Result<()> {
    let mut todos = load_todos()?;
    if let Some(todo) = todos.iter_mut().find(|todo| todo.id == id) {
        todo.focused_minutes = focused_minutes.min(todo.duration_minutes).max(0);
        todo.status = if done {
            TodoStatus::Done
        } else if Local::now() >= todo.end {
            TodoStatus::Expired
        } else {
            TodoStatus::Scheduled
        };
        save_todos(&todos)?;
    }
    Ok(())
}

fn choose_open_todo(todos: &[Todo]) -> Result<String> {
    let mut open: Vec<_> = todos
        .iter()
        .filter(|todo| !matches!(todo.status, TodoStatus::Done | TodoStatus::Cancelled))
        .collect();
    sort_todo_refs(&mut open);

    if open.is_empty() {
        return Err(anyhow!("no open todos to mark done"));
    }

    let items: Vec<String> = open
        .iter()
        .map(|todo| {
            format!(
                "{}  {}-{}  {}",
                todo.id,
                todo.start.format("%Y-%m-%d %H:%M"),
                todo.end.format("%H:%M"),
                todo.title
            )
        })
        .collect();
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Choose todo")
        .items(&items)
        .default(0)
        .interact()?;
    Ok(open[selection].id.clone())
}

fn sort_todo_refs(todos: &mut [&Todo]) {
    todos.sort_by_key(|todo| todo.start);
}
