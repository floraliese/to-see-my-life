// workbench — Today 工作台模型和 CLI 呈現。
// 這是 GUI 分支的第一個抽象點：把 TODO / DOING / DONE 三欄、
// carry-over、Today 摘要整理成結構化 TodayWorkbench，讓 CLI 和未來 GUI
// 可以消費同一份工作台數據。

use anyhow::Result;
use chrono::{DateTime, Local, NaiveDate};

use crate::{
    config,
    todo::{self, DaySummary, Todo, TodoStatus},
    util::format_minutes,
};

#[derive(Debug, Clone)]
pub struct TodayWorkbench {
    pub summary: DaySummary,
    pub todo: Vec<Todo>,
    pub doing: Vec<Todo>,
    pub done: Vec<Todo>,
    pub carried_over_count: usize,
}

impl TodayWorkbench {
    fn from_summary(summary: DaySummary, now: DateTime<Local>, carried_over_count: usize) -> Self {
        let mut todo = Vec::new();
        let mut doing = Vec::new();
        let mut done = Vec::new();

        for item in &summary.todos {
            match item.effective_status(now) {
                TodoStatus::Done => done.push(item.clone()),
                TodoStatus::Active => doing.push(item.clone()),
                TodoStatus::Cancelled => {}
                _ => todo.push(item.clone()),
            }
        }

        Self {
            summary,
            todo,
            doing,
            done,
            carried_over_count,
        }
    }
}

pub fn load_today_workbench(now: DateTime<Local>) -> Result<TodayWorkbench> {
    config::ensure_todos_file()?;
    let today = now.date_naive();
    let mut todos = todo::load_todos()?;
    let carried_over_count = carry_over_due_todos(&mut todos, today);
    if carried_over_count > 0 {
        todo::save_todos(&todos)?;
    }
    let summary = todo::summarize_day(todos, today, now);
    Ok(TodayWorkbench::from_summary(
        summary,
        now,
        carried_over_count,
    ))
}

pub fn print_today() -> Result<()> {
    let now = Local::now();
    let workbench = load_today_workbench(now)?;
    let summary = &workbench.summary;

    println!("Today workbench - {}", summary.date);
    if workbench.carried_over_count > 0 {
        println!(
            "Carried over: {} deferred todo(s)",
            workbench.carried_over_count
        );
    }
    println!("Total planned: {}", format_minutes(summary.total_minutes));
    println!("Focused: {}", format_minutes(summary.focused_minutes));
    println!("Completed: {}", summary.completed_count);
    println!("Expired: {}", summary.expired_count);
    println!("Unfinished: {}", summary.unfinished_count);
    println!("Cancelled: {}", summary.cancelled_count);
    if let Some(next) = &summary.next {
        println!(
            "Next: {} {} {}",
            next.id,
            todo::todo_time_label(next),
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

    print_today_board(&workbench, now);
    Ok(())
}

fn print_today_board(workbench: &TodayWorkbench, now: DateTime<Local>) {
    print_today_section("TODO", &workbench.todo, now);
    print_today_section("DOING", &workbench.doing, now);
    print_today_section("DONE", &workbench.done, now);
    println!();
    println!("Commands:");
    println!("  tsml today add \"Task\" --duration 25m");
    println!("  tsml today start <id> [--duration 25m]");
    println!("  tsml today done <id>");
    println!("  tsml today defer <id> [--to tomorrow]");
}

fn print_today_section(label: &str, todos: &[Todo], now: DateTime<Local>) {
    println!("{label} ({})", todos.len());
    if todos.is_empty() {
        println!("  --");
        return;
    }

    for todo in todos {
        println!(
            "  {}  {:<9}  {:<22}  {:<8}  {}",
            todo.id,
            todo.effective_status(now),
            todo::todo_time_label(todo),
            format_minutes(todo.duration_minutes),
            todo.title
        );
    }
}

fn carry_over_due_todos(todos: &mut [Todo], today: NaiveDate) -> usize {
    let mut count = 0;
    for todo in todos {
        if !matches!(todo.status, TodoStatus::Done | TodoStatus::Cancelled)
            && todo.start.is_none()
            && todo.deferred_until.is_some_and(|date| date < today)
        {
            todo.deferred_until = Some(today);
            count += 1;
        }
    }
    count
}
