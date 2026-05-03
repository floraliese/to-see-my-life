// timer — 倒計時器。
// 支持兩種模式：
//   - todo timer：自動查找當前或下一個 scheduled todo 並啟動倒計時。
//   - standalone timer：不關聯 todo，純粹倒計時。
// 默認使用 TUI (ratatui + crossterm) 全屏顯示，支持 --plain 純文本模式
// 用於測試或簡單場景，以及 --no-notify 跳過系統通知。
// timer 不是後台服務，終端關閉後計時停止。

use std::{
    io,
    time::{Duration as StdDuration, Instant},
};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Local};
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

use crate::{todo, util::parse_duration_seconds};

struct TimerSession {
    title: String,
    start: DateTime<Local>,
    end: DateTime<Local>,
    todo_id: Option<String>,
}

enum TimerOutcome {
    Completed,
    Quit,
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

    if matches!(outcome, TimerOutcome::Completed) && !no_notify {
        notify_finished(&session.title);
    }

    if let Some(id) = &session.todo_id {
        let focused = focused_minutes(session.start, session.end);
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
    let todos = todo::load_todos()?;
    let now = Local::now();
    let todo = todo::find_timer_todo(&todos, now).ok_or_else(|| {
        anyhow!("no scheduled todo found. Use `tsml timer --duration 25m --title Break`.")
    })?;

    Ok(TimerSession {
        title: todo.title.clone(),
        start: todo.start,
        end: todo.end,
        todo_id: Some(todo.id),
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

fn focused_minutes(start: DateTime<Local>, end: DateTime<Local>) -> i64 {
    let now = Local::now().min(end);
    if now <= start {
        0
    } else {
        (now - start).num_minutes()
    }
}

fn format_seconds(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{hours:02}:{minutes:02}:{secs:02}")
}
