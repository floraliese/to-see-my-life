// stats — 統計模塊。
// 從 todos.json 讀取任務歷史，按時間範圍（目前只支持本週）聚合完成數、
// 過期數、專注分鐘、常見任務標題，輸出純文本統計報告。
// 打印時顯示週一至今天的範圍和按開始時間排序的所有相關 todo。
// 只做展示和簡單聚合，不修改 JSON 數據。

use anyhow::Result;
use chrono::{DateTime, Datelike, Days, Local, NaiveDate};

use crate::todo::{TodoStatus, load_todos, sort_todos};
use crate::util::format_minutes;

#[derive(Debug, Clone)]
pub struct WeeklyStats {
    pub monday: NaiveDate,
    pub today: NaiveDate,
    pub total_count: usize,
    pub completed_count: usize,
    pub expired_count: usize,
    pub unfinished_count: usize,
    pub cancelled_count: usize,
    pub planned_minutes: i64,
    pub focused_minutes: i64,
    pub top_tasks: Vec<TaskFrequency>,
}

#[derive(Debug, Clone)]
pub struct TaskFrequency {
    pub title: String,
    pub count: usize,
}

pub fn print_stats(week: bool) -> Result<()> {
    if week {
        print_weekly_stats()
    } else {
        print_weekly_stats()
    }
}

fn print_weekly_stats() -> Result<()> {
    // TODO(manager): weekly stats can keep its own aggregation, but should read via TodoManager later.
    let now = Local::now();
    let stats = load_weekly_stats(now)?;

    println!(
        "{} - {} ({})\n",
        stats.monday.format("%Y-%m-%d"),
        stats.today.format("%Y-%m-%d"),
        stats.today.format("%A")
    );
    println!("Todos:     {} total", stats.total_count);
    println!("Completed: {}", stats.completed_count);
    println!("Expired:   {}", stats.expired_count);
    println!("Unfinished:{}", stats.unfinished_count);
    println!("Cancelled: {}", stats.cancelled_count);
    println!("Planned:   {}", format_minutes(stats.planned_minutes));
    println!("Focused:   {}", format_minutes(stats.focused_minutes));

    if !stats.top_tasks.is_empty() {
        println!("\nTop tasks:");
        for task in &stats.top_tasks {
            println!("  {}x  {}", task.count, task.title);
        }
    }

    Ok(())
}

pub fn load_weekly_stats(now: DateTime<Local>) -> Result<WeeklyStats> {
    let today = now.date_naive();
    let monday = today - Days::new((today.weekday().num_days_from_monday()) as u64);

    let mut todos = load_todos()?;
    sort_todos(&mut todos);
    let week_todos: Vec<_> = todos
        .iter()
        .filter(|todo| {
            // Unscheduled workbench items are counted on their workbench date;
            // scheduled items are counted by their concrete start date.
            let d = todo
                .start
                .map(|start| start.date_naive())
                .or(todo.deferred_until)
                .unwrap_or_else(|| todo.created_at.date_naive());
            d >= monday && d <= today
        })
        .cloned()
        .collect();

    let completed: Vec<_> = week_todos
        .iter()
        .filter(|todo| todo.status == TodoStatus::Done)
        .collect();
    let expired: Vec<_> = week_todos
        .iter()
        .filter(|todo| {
            todo.effective_status(now) == TodoStatus::Expired
                && todo.status != TodoStatus::Done
                && todo.status != TodoStatus::Cancelled
        })
        .collect();
    let unfinished: Vec<_> = week_todos
        .iter()
        .filter(|todo| {
            matches!(
                todo.effective_status(now),
                TodoStatus::Scheduled | TodoStatus::Active
            )
        })
        .collect();
    let cancelled: Vec<_> = week_todos
        .iter()
        .filter(|todo| todo.status == TodoStatus::Cancelled)
        .collect();

    let focused = week_todos.iter().map(|todo| todo.focused_minutes).sum();
    let planned = week_todos
        .iter()
        .filter(|todo| todo.status != TodoStatus::Cancelled)
        .map(|todo| todo.duration_minutes)
        .sum();

    use std::collections::HashMap;
    let mut title_counts: HashMap<&str, usize> = HashMap::new();
    for todo in &week_todos {
        *title_counts.entry(&todo.title).or_insert(0) += 1;
    }
    let mut top_titles: Vec<_> = title_counts.into_iter().collect();
    top_titles.sort_by_key(|(_, count)| -(*count as i64));
    let top_tasks = top_titles
        .into_iter()
        .take(5)
        .map(|(title, count)| TaskFrequency {
            title: title.to_string(),
            count,
        })
        .collect();

    Ok(WeeklyStats {
        monday,
        today,
        total_count: week_todos.len(),
        completed_count: completed.len(),
        expired_count: expired.len(),
        unfinished_count: unfinished.len(),
        cancelled_count: cancelled.len(),
        planned_minutes: planned,
        focused_minutes: focused,
        top_tasks,
    })
}
