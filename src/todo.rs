// todo — 任務管理核心模塊。
// 管理 todos.json 的讀寫與變更，支持狀態機轉換：
//   scheduled → active（自動計算） → expired（自動計算）
//   scheduled → done（標記完成）
//   scheduled → cancelled（取消）
// 提供 add/list/done/cancel/delete/edit/reschedule 等管理命令，
// 以及 find_timer_todo / update_todo_after_timer 供 timer 模塊調用。
// DaySummary 是今日摘要結構，同時被 workbench 和 review 模塊使用。
// overlap 檢查確保同一時間段不會出現多個 open todo（可通過 --force 繞過）。
// 下一輪迭代先保持 todo 作為核心模塊，在本文件內抽出 TodoManager，
// 讓 CLI 交互變成 thin wrapper，核心管理邏輯返回結構化結果供 CLI/GUI 復用。

use std::fs;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Days, Duration, Local, NaiveDate};
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
                    "- [{}] {} {}\n",
                    todo.effective_status(now),
                    todo_time_label(todo),
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
    // None means the item is in the today workbench TODO lane but has not
    // been scheduled into a concrete focus block yet.
    pub start: Option<DateTime<Local>>,
    pub end: Option<DateTime<Local>>,
    pub duration_minutes: i64,
    pub status: TodoStatus,
    pub focused_minutes: i64,
    pub created_at: DateTime<Local>,
    // Workbench date used by today add/defer. This is not a deadline; it only
    // decides which day's TODO lane should surface the unscheduled item.
    #[serde(default)]
    pub deferred_until: Option<NaiveDate>,
}

impl Todo {
    pub fn effective_status(&self, now: DateTime<Local>) -> TodoStatus {
        match self.status {
            TodoStatus::Done | TodoStatus::Cancelled => self.status.clone(),
            _ => match (self.start, self.end) {
                (Some(_), Some(end)) if now >= end => TodoStatus::Expired,
                (Some(start), Some(end)) if now >= start && now < end => TodoStatus::Active,
                _ => TodoStatus::Scheduled,
            },
        }
    }
}

pub fn add_todo(
    title_arg: Option<String>,
    start_arg: Option<String>,
    duration_arg: Option<String>,
    force: bool,
) -> Result<()> {
    // TODO(manager): split this into CLI input collection + TodoManager::add.
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
        start: Some(start),
        end: Some(end),
        duration_minutes,
        status: TodoStatus::Scheduled,
        focused_minutes: 0,
        created_at: now,
        deferred_until: None,
    };

    let mut todos = load_todos()?;
    ensure_no_overlap(&todos, None, start, end, now, force)?;
    todos.push(todo.clone());
    sort_todos(&mut todos);
    save_todos(&todos)?;

    println!(
        "Added {} [{} - {}, {}]",
        todo.title,
        start.format("%Y-%m-%d %H:%M"),
        end.format("%H:%M"),
        format_minutes(todo.duration_minutes)
    );
    Ok(())
}

pub fn list_todos(all: bool) -> Result<()> {
    // TODO(manager): have TodoManager return filtered todos; keep printing here.
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

pub fn add_today_todo(title: String, duration_arg: Option<String>) -> Result<()> {
    let todo = add_today_todo_record(title, duration_arg, Local::now())?;
    println!(
        "Added {} to today's TODO [{}].",
        todo.title,
        format_minutes(todo.duration_minutes)
    );
    Ok(())
}

pub fn add_today_todo_record(
    title: String,
    duration_arg: Option<String>,
    now: DateTime<Local>,
) -> Result<Todo> {
    config::ensure_todos_file()?;
    if title.trim().is_empty() {
        return Err(anyhow!("todo title cannot be empty"));
    }

    let duration_minutes = match duration_arg {
        Some(value) => parse_duration_minutes(&value)?,
        None => 25,
    };
    // today add intentionally creates an unscheduled item. It belongs to
    // today's workbench immediately, but does not become a time block until
    // today start assigns start/end.
    let todo = Todo {
        id: Uuid::new_v4().simple().to_string()[..8].to_string(),
        title,
        start: None,
        end: None,
        duration_minutes,
        status: TodoStatus::Scheduled,
        focused_minutes: 0,
        created_at: now,
        deferred_until: Some(now.date_naive()),
    };

    let mut todos = load_todos()?;
    todos.push(todo.clone());
    sort_todos(&mut todos);
    save_todos(&todos)?;
    Ok(todo)
}

pub fn start_today_todo(id: &str, duration_arg: Option<String>, force: bool) -> Result<Todo> {
    let edited = schedule_today_todo(id, duration_arg, force, Local::now())?;
    println!(
        "Started {id} [{}].",
        edited
            .start
            .map(|start| start.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "--".to_string())
    );
    Ok(edited)
}

pub fn schedule_today_todo(
    id: &str,
    duration_arg: Option<String>,
    force: bool,
    now: DateTime<Local>,
) -> Result<Todo> {
    config::ensure_todos_file()?;
    let mut todos = load_todos()?;
    let index = find_todo_index(&todos, id)?;
    let mut edited = todos[index].clone();

    if matches!(edited.status, TodoStatus::Done | TodoStatus::Cancelled) {
        return Err(anyhow!("cannot start a closed todo: {id}"));
    }

    if let Some(value) = duration_arg {
        edited.duration_minutes = parse_duration_minutes(&value)?;
    }
    // Starting is the point where a workbench TODO becomes a scheduled focus
    // block. The terminal timer is launched by main.rs after this state change.
    edited.start = Some(now);
    edited.end = Some(now + Duration::minutes(edited.duration_minutes));
    edited.status = TodoStatus::Scheduled;
    edited.deferred_until = None;

    ensure_no_overlap(
        &todos,
        Some(id),
        edited.start.expect("start was just assigned"),
        edited.end.expect("end was just assigned"),
        now,
        force,
    )?;
    todos[index] = edited.clone();
    sort_todos(&mut todos);
    save_todos(&todos)?;
    Ok(edited)
}

pub fn defer_todo(id: String, to_arg: String) -> Result<()> {
    let target_date = defer_todo_record(&id, &to_arg, Local::now().date_naive())?
        .deferred_until
        .expect("deferred todo has target date");
    println!("Deferred {id} to {target_date}.");
    Ok(())
}

pub fn defer_todo_record(id: &str, to_arg: &str, today: NaiveDate) -> Result<Todo> {
    config::ensure_todos_file()?;
    let target_date = parse_defer_date(&to_arg, today)?;
    let mut todos = load_todos()?;
    let todo = find_todo_mut(&mut todos, id)?;

    if matches!(todo.status, TodoStatus::Done | TodoStatus::Cancelled) {
        return Err(anyhow!("cannot defer a closed todo: {id}"));
    }

    // Deferring removes concrete schedule information and re-surfaces the item
    // as an unscheduled TODO on the target workbench date.
    todo.start = None;
    todo.end = None;
    todo.status = TodoStatus::Scheduled;
    todo.deferred_until = Some(target_date);
    let updated = todo.clone();
    save_todos(&todos)?;
    Ok(updated)
}

pub fn summarize_day(mut todos: Vec<Todo>, date: NaiveDate, now: DateTime<Local>) -> DaySummary {
    sort_todos(&mut todos);
    let todos: Vec<Todo> = todos
        .into_iter()
        .filter(|todo| todo_belongs_to_day(todo, date))
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
        todo.start
            .map(|start| start.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "--".to_string()),
        todo.end
            .map(|end| end.format("%H:%M").to_string())
            .unwrap_or_else(|| "--".to_string()),
        format_minutes(todo.duration_minutes),
        todo.title
    );
}

pub fn todo_time_label(todo: &Todo) -> String {
    match (todo.start, todo.end, todo.deferred_until) {
        (Some(start), Some(end), _) => {
            format!("{}-{}", start.format("%Y-%m-%d %H:%M"), end.format("%H:%M"))
        }
        (_, _, Some(date)) => format!("deferred {date}"),
        _ => "unscheduled".to_string(),
    }
}

fn todo_belongs_to_day(todo: &Todo, date: NaiveDate) -> bool {
    // A todo can belong to a day either because it has a concrete time block,
    // because it was deferred to that workbench date, or because it was created
    // today before being scheduled.
    todo.start
        .map(|start| start.date_naive() == date)
        .unwrap_or(false)
        || todo.deferred_until == Some(date)
        || (todo.start.is_none()
            && todo.deferred_until.is_none()
            && todo.created_at.date_naive() == date)
}

fn parse_defer_date(input: &str, today: NaiveDate) -> Result<NaiveDate> {
    let value = input.trim().to_lowercase();
    match value.as_str() {
        "today" => Ok(today),
        "tomorrow" => today
            .checked_add_days(Days::new(1))
            .ok_or_else(|| anyhow!("invalid defer target: {input}")),
        _ => NaiveDate::parse_from_str(input.trim(), "%Y-%m-%d")
            .with_context(|| "defer target must be today, tomorrow, or YYYY-MM-DD"),
    }
}

pub fn mark_done(id_arg: Option<String>) -> Result<()> {
    // TODO(manager): keep optional interactive selection here, move mutation into TodoManager::done.
    config::ensure_todos_file()?;
    let todos = load_todos()?;
    let id = match id_arg {
        Some(value) => value,
        None => choose_open_todo(&todos)?,
    };

    mark_done_by_id(&id)?;
    println!("Marked {id} done.");
    Ok(())
}

pub fn mark_done_by_id(id: &str) -> Result<Todo> {
    config::ensure_todos_file()?;
    let mut todos = load_todos()?;
    let index = find_todo_index(&todos, id)?;
    // Manual completion is allowed as a personal-management escape hatch, but
    // it must not synthesize focus time. Timer completion records focus through
    // update_todo_after_timer and timer_sessions.json.
    todos[index].status = TodoStatus::Done;
    let updated = todos[index].clone();
    save_todos(&todos)?;
    Ok(updated)
}

pub fn cancel_todo(id_arg: Option<String>) -> Result<()> {
    // TODO(manager): keep optional interactive selection here, move mutation into TodoManager::cancel.
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
    // TODO(manager): keep confirmation here, move deletion into TodoManager::delete.
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
    // TODO(manager): build EditTodoRequest here, then delegate validation and persistence.
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
        edited.start = Some(parse_start_time(&value, now)?);
        edited.deferred_until = None;
    }
    if let Some(value) = duration_arg {
        edited.duration_minutes = parse_duration_minutes(&value)?;
    }
    edited.end = edited
        .start
        .map(|start| start + Duration::minutes(edited.duration_minutes));

    if let (Some(start), Some(end)) = (edited.start, edited.end) {
        ensure_no_overlap(&todos, Some(&id), start, end, now, force)?;
    }
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
    // TODO(manager): build RescheduleTodoRequest here, then delegate validation and persistence.
    config::ensure_todos_file()?;
    let now = Local::now();
    let mut todos = load_todos()?;
    let index = find_todo_index(&todos, &id)?;
    let mut edited = todos[index].clone();

    edited.start = Some(parse_start_time(&start_arg, now)?);
    if let Some(value) = duration_arg {
        edited.duration_minutes = parse_duration_minutes(&value)?;
    }
    edited.end = edited
        .start
        .map(|start| start + Duration::minutes(edited.duration_minutes));
    edited.deferred_until = None;

    if let (Some(start), Some(end)) = (edited.start, edited.end) {
        ensure_no_overlap(&todos, Some(&id), start, end, now, force)?;
    }
    todos[index] = edited;
    sort_todos(&mut todos);
    save_todos(&todos)?;
    println!("Rescheduled {id}.");
    Ok(())
}

pub fn load_todos() -> Result<Vec<Todo>> {
    // TODO(manager): storage can stay here initially; TodoManager should be the primary caller.
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
    todos.sort_by_key(|todo| todo.start.unwrap_or(todo.created_at));
}

pub fn find_timer_todo(todos: &[Todo], now: DateTime<Local>) -> Option<Todo> {
    // TODO(manager): expose this through TodoManager so timer.rs stops loading todos directly.
    let mut open: Vec<_> = todos
        .iter()
        .filter(|todo| !matches!(todo.status, TodoStatus::Done | TodoStatus::Cancelled))
        .filter(|todo| todo.start.is_some() && todo.end.is_some())
        .cloned()
        .collect();
    sort_todos(&mut open);

    open.iter()
        .find(|todo| {
            let start = todo.start.expect("scheduled timer todo has start");
            let end = todo.end.expect("scheduled timer todo has end");
            now >= start && now < end
        })
        .cloned()
        .or_else(|| {
            open.into_iter()
                .find(|todo| todo.start.expect("scheduled timer todo has start") > now)
        })
}

pub fn update_todo_after_timer(id: &str, done: bool, focused_minutes: i64) -> Result<()> {
    // TODO(manager): rename conceptually to record_focus and return the updated Todo.
    let _ = record_focus_for_todo(id, done, focused_minutes)?;
    Ok(())
}

pub fn record_focus_for_todo(id: &str, done: bool, focused_minutes: i64) -> Result<Todo> {
    let mut todos = load_todos()?;
    let index = find_todo_index(&todos, id)?;
    let todo = &mut todos[index];
    let focused = (todo.focused_minutes + focused_minutes)
        .min(todo.duration_minutes)
        .max(0);
    todo.focused_minutes = focused;
    todo.status = if done {
        TodoStatus::Done
    } else if todo.end.map(|end| Local::now() >= end).unwrap_or(false) {
        TodoStatus::Expired
    } else {
        TodoStatus::Scheduled
    };
    let updated = todo.clone();
    save_todos(&todos)?;
    Ok(updated)
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
    // TODO(manager): eventually return structured conflicts instead of printing from core logic.
    let conflicts: Vec<_> = todos
        .iter()
        .filter(|todo| editing_id != Some(todo.id.as_str()))
        .filter(|todo| {
            !matches!(
                todo.effective_status(now),
                TodoStatus::Done | TodoStatus::Cancelled | TodoStatus::Expired
            )
        })
        .filter(|todo| {
            if let (Some(existing_start), Some(existing_end)) = (todo.start, todo.end) {
                start < existing_end && end > existing_start
            } else {
                false
            }
        })
        .collect();

    if conflicts.is_empty() {
        return Ok(());
    }

    println!("Overlapping todo(s):");
    for todo in &conflicts {
        println!("{}  {}  {}", todo.id, todo_time_label(todo), todo.title);
    }

    if force {
        return Ok(());
    }

    Err(anyhow!(
        "time block overlaps existing todo; pass --force to allow it"
    ))
}

fn choose_open_todo(todos: &[Todo]) -> Result<String> {
    // TODO(manager): this remains CLI-only selection logic; do not move it into TodoManager.
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
                todo.start
                    .map(|start| start.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "--".to_string()),
                todo.end
                    .map(|end| end.format("%H:%M").to_string())
                    .unwrap_or_else(|| "--".to_string()),
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
    todos.sort_by_key(|todo| todo.start.unwrap_or(todo.created_at));
}
