// timer — 倒計時器。
// 支持兩種模式：
//   - todo timer：自動查找當前或下一個 scheduled todo 並啟動倒計時。
//   - standalone timer：不關聯 todo，純粹倒計時。
// 默認使用 TUI (ratatui + crossterm) 全屏顯示，支持 --plain 純文本模式
// 用於測試或簡單場景，以及 --no-notify 跳過系統通知。
// timer 不是後台服務，終端關閉後計時停止。

use std::{
    fs, io,
    time::{Duration as StdDuration, Instant},
};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, Local, NaiveDate};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use dialoguer::Confirm;
use notify_rust::Notification;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    config, todo,
    util::{format_minutes, parse_duration_seconds},
};

struct TimerSession {
    title: String,
    start: DateTime<Local>,
    end: DateTime<Local>,
    todo_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimerSessionOutcome {
    Completed,
    Quit,
}

impl std::fmt::Display for TimerSessionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            TimerSessionOutcome::Completed => "completed",
            TimerSessionOutcome::Quit => "quit",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerSessionRecord {
    pub id: String,
    pub todo_id: Option<String>,
    pub title: String,
    pub started_at: DateTime<Local>,
    pub ended_at: DateTime<Local>,
    pub planned_seconds: i64,
    pub focused_seconds: i64,
    pub outcome: TimerSessionOutcome,
}

#[derive(Debug, Clone, Copy)]
enum TimerOutcome {
    Completed,
    Quit,
}

impl TimerOutcome {
    fn session_outcome(&self) -> TimerSessionOutcome {
        match self {
            TimerOutcome::Completed => TimerSessionOutcome::Completed,
            TimerOutcome::Quit => TimerSessionOutcome::Quit,
        }
    }
}

pub fn run_timer(
    title_arg: Option<String>,
    duration_arg: Option<String>,
    plain: bool,
    no_notify: bool,
) -> Result<()> {
    let session = match duration_arg {
        Some(duration) => standalone_session(title_arg, &duration)?,
        None => todo_session()?,
    };

    let outcome = if plain {
        run_plain(&session)?
    } else {
        run_tui(&session)?
    };
    let ended_at = Local::now();
    let focused_seconds = focused_seconds(&session, ended_at);
    record_timer_session(&session, &outcome, ended_at, focused_seconds)?;

    if matches!(outcome, TimerOutcome::Completed) && !no_notify {
        notify_finished(&session.title);
    }

    if let Some(id) = &session.todo_id {
        // TODO(manager): keep the terminal timer mode, but record focus through TodoManager.
        let focused = focused_minutes(focused_seconds);
        let done = if matches!(outcome, TimerOutcome::Completed) {
            Confirm::new()
                .with_prompt("Mark this todo as done?")
                .default(true)
                .interact()?
        } else {
            false
        };
        todo::update_todo_after_timer(id, done, focused)?;
    }

    Ok(())
}

pub fn run_timer_for_todo(id: String, plain: bool, no_notify: bool) -> Result<()> {
    let session = todo_session_by_id(&id)?;

    let outcome = if plain {
        run_plain(&session)?
    } else {
        run_tui(&session)?
    };
    let ended_at = Local::now();
    let focused_seconds = focused_seconds(&session, ended_at);
    record_timer_session(&session, &outcome, ended_at, focused_seconds)?;

    if matches!(outcome, TimerOutcome::Completed) && !no_notify {
        notify_finished(&session.title);
    }

    let focused = focused_minutes(focused_seconds);
    let done = if matches!(outcome, TimerOutcome::Completed) {
        Confirm::new()
            .with_prompt("Mark this todo as done?")
            .default(true)
            .interact()?
    } else {
        false
    };
    todo::update_todo_after_timer(&id, done, focused)?;
    Ok(())
}

fn standalone_session(title_arg: Option<String>, duration_arg: &str) -> Result<TimerSession> {
    let seconds = parse_duration_seconds(duration_arg)?;
    let start = Local::now();
    let end = start + Duration::seconds(seconds);
    Ok(TimerSession {
        title: title_arg.unwrap_or_else(|| "Timer".to_string()),
        start,
        end,
        todo_id: None,
    })
}

fn todo_session() -> Result<TimerSession> {
    // TODO(manager): ask TodoManager for current/next timer todo instead of loading JSON here.
    let todos = todo::load_todos()?;
    let now = Local::now();
    let todo = todo::find_timer_todo(&todos, now).ok_or_else(|| {
        anyhow!("no scheduled todo found. Use `tsml timer --duration 25m --title Break`.")
    })?;

    Ok(TimerSession {
        title: todo.title.clone(),
        start: todo
            .start
            .ok_or_else(|| anyhow!("selected todo is not scheduled"))?,
        end: todo
            .end
            .ok_or_else(|| anyhow!("selected todo is not scheduled"))?,
        todo_id: Some(todo.id),
    })
}

fn todo_session_by_id(id: &str) -> Result<TimerSession> {
    let todos = todo::load_todos()?;
    let todo = todos
        .into_iter()
        .find(|todo| todo.id == id)
        .ok_or_else(|| anyhow!("todo not found: {id}"))?;
    Ok(TimerSession {
        title: todo.title,
        start: todo
            .start
            .ok_or_else(|| anyhow!("todo is not scheduled; use `tsml today start {id}` first"))?,
        end: todo
            .end
            .ok_or_else(|| anyhow!("todo is not scheduled; use `tsml today start {id}` first"))?,
        todo_id: Some(id.to_string()),
    })
}

fn run_plain(session: &TimerSession) -> Result<TimerOutcome> {
    loop {
        let now = Local::now();
        let remaining = (session.end - now).num_seconds().max(0);
        let focused = if now > session.start {
            (now.min(session.end) - session.start).num_seconds().max(0)
        } else {
            0
        };

        if now < session.start {
            println!(
                "{} | starts in {} | remaining {} | focused {}",
                session.title,
                format_seconds((session.start - now).num_seconds()),
                format_seconds(remaining),
                format_seconds(focused)
            );
        } else {
            println!(
                "{} | remaining {} | focused {} | now {}",
                session.title,
                format_seconds(remaining),
                format_seconds(focused),
                now.format("%H:%M:%S")
            );
        }

        if now >= session.end {
            println!("Timer finished: {}", session.title);
            return Ok(TimerOutcome::Completed);
        }

        std::thread::sleep(StdDuration::from_secs(1));
    }
}

fn run_tui(session: &TimerSession) -> Result<TimerOutcome> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, session);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &TimerSession,
) -> Result<TimerOutcome> {
    let tick_rate = StdDuration::from_millis(250);
    let mut last_tick = Instant::now();

    loop {
        let now = Local::now();
        terminal.draw(|frame| draw(frame, session, now))?;

        if now >= session.end {
            return Ok(TimerOutcome::Completed);
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| StdDuration::from_secs(0));
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(TimerOutcome::Quit),
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}

fn draw(frame: &mut ratatui::Frame, session: &TimerSession, now: DateTime<Local>) {
    let area = frame.area();
    let block = Block::default()
        .title(" to-see-my-life ")
        .borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .margin(2)
        .split(inner);

    let phase = if now < session.start {
        "Scheduled"
    } else {
        "Focus"
    };
    let title = Paragraph::new(vec![
        Line::from(Span::styled(
            &session.title,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(
            "{}  {} - {}",
            phase,
            session.start.format("%H:%M"),
            session.end.format("%H:%M")
        )),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    let remaining_to_start = (session.start - now).num_seconds();
    let remaining_to_end = (session.end - now).num_seconds().max(0);
    let focused = if now > session.start {
        (now.min(session.end) - session.start).num_seconds().max(0)
    } else {
        0
    };

    let stats = if remaining_to_start > 0 {
        vec![
            Line::from(format!("Starts in: {}", format_seconds(remaining_to_start))),
            Line::from(format!("Remaining: {}", format_seconds(remaining_to_end))),
            Line::from(format!("Focused:   {}", format_seconds(focused))),
        ]
    } else {
        vec![
            Line::from(format!("Remaining: {}", format_seconds(remaining_to_end))),
            Line::from(format!("Focused:   {}", format_seconds(focused))),
            Line::from(format!("Now:       {}", now.format("%H:%M:%S"))),
        ]
    };
    frame.render_widget(
        Paragraph::new(stats)
            .block(Block::default().borders(Borders::ALL).title(" timer "))
            .alignment(Alignment::Center),
        chunks[1],
    );

    frame.render_widget(
        Paragraph::new("[q] quit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center),
        chunks[2],
    );
}

fn notify_finished(title: &str) {
    let _ = Notification::new()
        .summary("tsml timer finished")
        .body(title)
        .show();
}

pub fn load_timer_sessions() -> Result<Vec<TimerSessionRecord>> {
    let path = config::timer_sessions_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn save_timer_sessions(sessions: &[TimerSessionRecord]) -> Result<()> {
    let path = config::timer_sessions_file()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let raw =
        serde_json::to_string_pretty(sessions).context("failed to serialize timer sessions")?;
    fs::write(&path, format!("{raw}\n"))
        .with_context(|| format!("failed to write {}", path.display()))
}

fn record_timer_session(
    session: &TimerSession,
    outcome: &TimerOutcome,
    ended_at: DateTime<Local>,
    focused_seconds: i64,
) -> Result<()> {
    append_timer_session(
        session.todo_id.clone(),
        session.title.clone(),
        session.start,
        ended_at,
        (session.end - session.start).num_seconds().max(0),
        focused_seconds,
        outcome.session_outcome(),
    )?;
    Ok(())
}

pub fn append_timer_session(
    todo_id: Option<String>,
    title: String,
    started_at: DateTime<Local>,
    ended_at: DateTime<Local>,
    planned_seconds: i64,
    focused_seconds: i64,
    outcome: TimerSessionOutcome,
) -> Result<TimerSessionRecord> {
    // Session history is append-only at the product level, but implemented as
    // load-all/save-all for now to keep storage simple and consistent with todos.
    config::ensure_timer_sessions_file()?;
    let mut sessions = load_timer_sessions()?;
    let record = TimerSessionRecord {
        id: Uuid::new_v4().simple().to_string()[..8].to_string(),
        todo_id,
        title,
        started_at,
        ended_at,
        planned_seconds: planned_seconds.max(0),
        focused_seconds: focused_seconds.max(0),
        outcome,
    };
    sessions.push(record.clone());
    save_timer_sessions(&sessions)?;
    Ok(record)
}

pub fn timer_sessions_markdown(date: NaiveDate) -> Result<String> {
    // Review groups sessions by their planned/recorded start date. A session
    // crossing midnight currently remains on the day it started.
    let mut sessions: Vec<_> = load_timer_sessions()?
        .into_iter()
        .filter(|session| session.started_at.date_naive() == date)
        .collect();
    sessions.sort_by_key(|session| session.started_at);

    let total_seconds: i64 = sessions.iter().map(|session| session.focused_seconds).sum();
    let mut content = format!(
        "## 今日專注記錄\n\n- Session 數：{}\n- 總專注：{}\n\n",
        sessions.len(),
        format_minutes(minutes_from_seconds(total_seconds))
    );
    content.push_str("### Sessions\n\n");

    if sessions.is_empty() {
        content.push_str("- 今天沒有 timer session。\n");
    } else {
        for session in &sessions {
            content.push_str(&format!(
                "- [{}] {}-{} {} ({})\n",
                session.outcome,
                session.started_at.format("%H:%M"),
                session.ended_at.format("%H:%M"),
                session.title,
                format_minutes(minutes_from_seconds(session.focused_seconds))
            ));
        }
    }

    Ok(content)
}

fn focused_seconds(session: &TimerSession, ended_at: DateTime<Local>) -> i64 {
    let finished_at = ended_at.min(session.end);
    if finished_at <= session.start {
        0
    } else {
        (finished_at - session.start).num_seconds().max(0)
    }
}

fn focused_minutes(seconds: i64) -> i64 {
    minutes_from_seconds(seconds)
}

fn minutes_from_seconds(seconds: i64) -> i64 {
    if seconds <= 0 { 0 } else { (seconds + 59) / 60 }
}

fn format_seconds(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{hours:02}:{minutes:02}:{secs:02}")
}
