// stats — 統計模塊。
// 從 todos.json 讀取任務歷史，按時間範圍（目前只支持本週）聚合完成數、
// 過期數、專注分鐘、常見任務標題，輸出純文本統計報告。
// 打印時顯示週一至今天的範圍和按開始時間排序的所有相關 todo。
// 只做展示和簡單聚合，不修改 JSON 數據。

use anyhow::Result;
use chrono::{Datelike, Days, Local};

use crate::todo::{TodoStatus, load_todos, sort_todos};
use crate::util::format_minutes;

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
    let top_titles = &top_titles[..top_titles.len().min(5)];

    println!(
        "{} - {} ({})\n",
        monday.format("%Y-%m-%d"),
        today.format("%Y-%m-%d"),
        today.format("%A")
    );
    println!("Todos:     {} total", week_todos.len());
    println!("Completed: {}", completed.len());
    println!("Expired:   {}", expired.len());
    println!("Unfinished:{}", unfinished.len());
    println!("Cancelled: {}", cancelled.len());
    println!("Planned:   {}", format_minutes(planned));
    println!("Focused:   {}", format_minutes(focused));

    if !top_titles.is_empty() {
        println!("\nTop tasks:");
        for (title, count) in top_titles {
            println!("  {}x  {}", count, title);
        }
    }

    Ok(())
}
