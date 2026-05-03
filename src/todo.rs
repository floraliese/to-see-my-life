// todo — 任務管理核心模塊。
// 管理 todos.json 的讀寫與變更，支持狀態機轉換：
//   scheduled → active（自動計算） → expired（自動計算）
//   scheduled → done（標記完成）
//   scheduled → cancelled（取消）
// 提供 add/list/done/cancel/delete/edit/reschedule 等管理命令，
// 以及 find_timer_todo / update_todo_after_timer 供 timer 模塊調用。
// DaySummary 是今日摘要結構，同時被 today 和 review 模塊使用。
// overlap 檢查確保同一時間段不會出現多個 open todo（可通過 --force 繞過）。

use std::fs;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, Local, NaiveDate};
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};
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

#[derive(Debug, Clone)]
pub struct DaySummary {
    pub date: NaiveDate,
    pub todos: Vec<Todo>,
    pub total_minutes: i64,
    pub focused_minutes: i64,
    pub completed_count: usize,
    pub expired_count: usize,
    pub unfinished_count: usize,
    pub cancelled_count: usize,
    pub next: Option<Todo>,
}

impl DaySummary {
    pub fn markdown(&self, now: DateTime<Local>) -> String {
        let mut content = format!(
            "## 今日 Todo 摘要\n\n- 預估時間：{}\n- 已專注：{}\n- 完成：{}\n- 過期：{}\n- 未完成：{}\n\n",
            format_minutes(self.total_minutes),
            format_minutes(self.focused_minutes),
            self.completed_count,
            self.expired_count,
            self.unfinished_count
        );
        content.push_str("### Todos\n\n");
        if self.todos.is_empty() {
            content.push_str("- 今天沒有 todo。\n");
        } else {
            for todo in &self.todos {
                content.push_str(&format!(
                    "- [{}] {}-{} {}\n",
                    todo.effective_status(now),
                    todo.start.format("%H:%M"),
                    todo.end.format("%H:%M"),
                    todo.title
                ));
            }
        }
        content
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
    force: bool,
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
    ensure_no_overlap(&todos, None, todo.start, todo.end, now, force)?;
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
        print_todo_line(todo, now);
    }
    Ok(())
}

pub fn print_today() -> Result<()> {
    config::ensure_todos_file()?;
    let now = Local::now();
    let summary = summarize_day(load_todos()?, now.date_naive(), now);

    println!("Today - {}", summary.date);
    println!("Total planned: {}", format_minutes(summary.total_minutes));
    println!("Focused: {}", format_minutes(summary.focused_minutes));
    println!("Completed: {}", summary.completed_count);
    println!("Expired: {}", summary.expired_count);
    println!("Unfinished: {}", summary.unfinished_count);
    println!("Cancelled: {}", summary.cancelled_count);
    if let Some(next) = &summary.next {
        println!(
            "Next: {} {}-{} {}",
            next.id,
            next.start.format("%H:%M"),
            next.end.format("%H:%M"),
            next.title
        );
    } else {
        println!("Next: none");
    }
    println!();

    if summary.todos.is_empty() {
        println!("No todos today.");
        return Ok(());
    }

    for todo in &summary.todos {
        print_todo_line(todo, now);
    }
    Ok(())
}

pub fn summarize_day(mut todos: Vec<Todo>, date: NaiveDate, now: DateTime<Local>) -> DaySummary {
    sort_todos(&mut todos);
    let todos: Vec<Todo> = todos
        .into_iter()
        .filter(|todo| todo.start.date_naive() == date)
        .collect();
    let total_minutes = todos
        .iter()
        .filter(|todo| !matches!(todo.status, TodoStatus::Cancelled))
        .map(|todo| todo.duration_minutes)
        .sum();
    let focused_minutes = todos.iter().map(|todo| todo.focused_minutes).sum();
    let completed_count = todos
        .iter()
        .filter(|todo| todo.status == TodoStatus::Done)
        .count();
    let cancelled_count = todos
        .iter()
        .filter(|todo| todo.status == TodoStatus::Cancelled)
        .count();
    let expired_count = todos
        .iter()
        .filter(|todo| todo.effective_status(now) == TodoStatus::Expired)
        .count();
    let unfinished_count = todos
        .iter()
        .filter(|todo| {
            matches!(
                todo.effective_status(now),
                TodoStatus::Scheduled | TodoStatus::Active
            )
        })
        .count();
    let next = todos
        .iter()
        .find(|todo| {
            matches!(
                todo.effective_status(now),
                TodoStatus::Scheduled | TodoStatus::Active
            )
        })
        .cloned();

    DaySummary {
        date,
        todos,
        total_minutes,
        focused_minutes,
        completed_count,
        expired_count,
        unfinished_count,
        cancelled_count,
        next,
    }
}

fn print_todo_line(todo: &Todo, now: DateTime<Local>) {
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

pub fn cancel_todo(id_arg: Option<String>) -> Result<()> {
    config::ensure_todos_file()?;
    let mut todos = load_todos()?;
    let id = match id_arg {
        Some(value) => value,
        None => choose_open_todo(&todos)?,
    };
    let todo = find_todo_mut(&mut todos, &id)?;
    todo.status = TodoStatus::Cancelled;
    save_todos(&todos)?;
    println!("Cancelled {id}.");
    Ok(())
}

pub fn delete_todo(id: String, yes: bool) -> Result<()> {
    config::ensure_todos_file()?;
    let mut todos = load_todos()?;
    let index = todos
        .iter()
        .position(|todo| todo.id == id)
        .ok_or_else(|| anyhow!("todo not found: {id}"))?;

    if !yes {
        let delete = Confirm::new()
            .with_prompt(format!("Delete {} permanently?", todos[index].title))
            .default(false)
            .interact()?;
        if !delete {
            println!("Delete cancelled.");
            return Ok(());
        }
    }

    todos.remove(index);
    save_todos(&todos)?;
    println!("Deleted {id}.");
    Ok(())
}

pub fn edit_todo(
    id: String,
    title: Option<String>,
    start_arg: Option<String>,
    duration_arg: Option<String>,
    force: bool,
) -> Result<()> {
    config::ensure_todos_file()?;
    if title.is_none() && start_arg.is_none() && duration_arg.is_none() {
        return Err(anyhow!(
            "nothing to edit; pass --title, --start, or --duration"
        ));
    }

    let now = Local::now();
    let mut todos = load_todos()?;
    let index = find_todo_index(&todos, &id)?;
    let mut edited = todos[index].clone();

    if let Some(value) = title {
        if value.trim().is_empty() {
            return Err(anyhow!("todo title cannot be empty"));
        }
        edited.title = value;
    }
    if let Some(value) = start_arg {
        edited.start = parse_start_time(&value, now)?;
    }
    if let Some(value) = duration_arg {
        edited.duration_minutes = parse_duration_minutes(&value)?;
    }
    edited.end = edited.start + Duration::minutes(edited.duration_minutes);

    ensure_no_overlap(&todos, Some(&id), edited.start, edited.end, now, force)?;
    todos[index] = edited;
    sort_todos(&mut todos);
    save_todos(&todos)?;
    println!("Edited {id}.");
    Ok(())
}

pub fn reschedule_todo(
    id: String,
    start_arg: String,
    duration_arg: Option<String>,
    force: bool,
) -> Result<()> {
    config::ensure_todos_file()?;
    let now = Local::now();
    let mut todos = load_todos()?;
    let index = find_todo_index(&todos, &id)?;
    let mut edited = todos[index].clone();

    edited.start = parse_start_time(&start_arg, now)?;
    if let Some(value) = duration_arg {
        edited.duration_minutes = parse_duration_minutes(&value)?;
    }
    edited.end = edited.start + Duration::minutes(edited.duration_minutes);

    ensure_no_overlap(&todos, Some(&id), edited.start, edited.end, now, force)?;
    todos[index] = edited;
    sort_todos(&mut todos);
    save_todos(&todos)?;
    println!("Rescheduled {id}.");
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

fn find_todo_index(todos: &[Todo], id: &str) -> Result<usize> {
    todos
        .iter()
        .position(|todo| todo.id == id)
        .ok_or_else(|| anyhow!("todo not found: {id}"))
}

fn find_todo_mut<'a>(todos: &'a mut [Todo], id: &str) -> Result<&'a mut Todo> {
    todos
        .iter_mut()
        .find(|todo| todo.id == id)
        .ok_or_else(|| anyhow!("todo not found: {id}"))
}

fn ensure_no_overlap(
    todos: &[Todo],
    editing_id: Option<&str>,
    start: DateTime<Local>,
    end: DateTime<Local>,
    now: DateTime<Local>,
    force: bool,
) -> Result<()> {
    let conflicts: Vec<_> = todos
        .iter()
        .filter(|todo| editing_id != Some(todo.id.as_str()))
        .filter(|todo| {
            !matches!(
                todo.effective_status(now),
                TodoStatus::Done | TodoStatus::Cancelled | TodoStatus::Expired
            )
        })
        .filter(|todo| start < todo.end && end > todo.start)
        .collect();

    if conflicts.is_empty() {
        return Ok(());
    }

    println!("Overlapping todo(s):");
    for todo in &conflicts {
        println!(
            "{}  {}-{}  {}",
            todo.id,
            todo.start.format("%Y-%m-%d %H:%M"),
            todo.end.format("%H:%M"),
            todo.title
        );
    }

    if force {
        return Ok(());
    }

    Err(anyhow!(
        "time block overlaps existing todo; pass --force to allow it"
    ))
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
