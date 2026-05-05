// gui — native desktop workbench.
// This module is intentionally a GUI shell over the existing local-first core:
// todo/workbench/timer/stats still own persistence and product rules, while
// eframe/egui owns window layout, focus-session controls, and inspector state.

use std::{path::PathBuf, time::Duration as StdDuration};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Local};
use eframe::egui::{self, Align, Color32, Layout, RichText, Stroke};
use egui_extras::{Column, TableBuilder};

use crate::{
    config, logging, stats, timer,
    timer::{TimerSessionOutcome, TimerSessionRecord},
    todo,
    util::{format_minutes, parse_duration_seconds},
    workbench::{self, TodayWorkbench},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Today,
    Sessions,
    Review,
    Settings,
}

#[derive(Debug, Clone)]
struct GuiData {
    loaded_at: DateTime<Local>,
    data_dir: PathBuf,
    log_file: PathBuf,
    notes_dir: Option<PathBuf>,
    workbench: TodayWorkbench,
    weekly: stats::WeeklyStats,
    recent_sessions: Vec<TimerSessionRecord>,
    review_markdown: String,
}

#[derive(Debug, Clone)]
struct ActiveFocus {
    todo_id: Option<String>,
    title: String,
    started_at: DateTime<Local>,
    ends_at: DateTime<Local>,
    planned_seconds: i64,
}

impl ActiveFocus {
    fn focused_seconds(&self, now: DateTime<Local>) -> i64 {
        if now <= self.started_at {
            0
        } else {
            (now.min(self.ends_at) - self.started_at)
                .num_seconds()
                .clamp(0, self.planned_seconds)
        }
    }

    fn remaining_seconds(&self, now: DateTime<Local>) -> i64 {
        (self.ends_at - now).num_seconds().max(0)
    }

    fn progress(&self, now: DateTime<Local>) -> f32 {
        if self.planned_seconds <= 0 {
            0.0
        } else {
            self.focused_seconds(now) as f32 / self.planned_seconds as f32
        }
    }
}

#[derive(Debug, Clone)]
enum GuiAction {
    Select(String),
    Start(String),
    Done(String),
    DeferTomorrow(String),
}

pub fn run_gui() -> Result<()> {
    config::ensure_todos_file()?;
    config::ensure_timer_sessions_file()?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("to-see-my-life")
            .with_inner_size([1320.0, 840.0])
            .with_min_inner_size([980.0, 680.0]),
        ..Default::default()
    };

    eframe::run_native(
        "to-see-my-life",
        options,
        Box::new(|cc| Ok(Box::new(TsmlGuiApp::new(cc)))),
    )
    .map_err(|err| anyhow!("failed to run GUI: {err}"))
}

struct TsmlGuiApp {
    view: View,
    data: Option<GuiData>,
    selected_id: Option<String>,
    active_focus: Option<ActiveFocus>,
    new_title: String,
    new_duration: String,
    focus_duration_override: String,
    standalone_title: String,
    standalone_duration: String,
    allow_overlap: bool,
    mark_done_after_focus: bool,
    message: Option<String>,
}

impl TsmlGuiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_macos_style(&cc.egui_ctx);

        let mut app = Self {
            view: View::Today,
            data: None,
            selected_id: None,
            active_focus: None,
            new_title: String::new(),
            new_duration: "25m".to_string(),
            focus_duration_override: String::new(),
            standalone_title: "Focus".to_string(),
            standalone_duration: "25m".to_string(),
            allow_overlap: false,
            mark_done_after_focus: true,
            message: None,
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        match load_gui_data() {
            Ok(data) => {
                self.data = Some(data);
            }
            Err(err) => {
                logging::append_timestamped_line_lossy(
                    "gui.log",
                    "ERROR",
                    "refresh GUI data",
                    &format!("{err:#}"),
                );
            }
        }
    }

    fn refresh_with_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
        self.refresh();
    }

    fn log_error(&self, context: &str, err: impl std::fmt::Display) {
        logging::append_timestamped_line_lossy("gui.log", "ERROR", context, &err.to_string());
    }

    fn add_today_todo(&mut self) {
        let title = self.new_title.trim().to_string();
        let duration = optional_text(&self.new_duration);
        match todo::add_today_todo_record(title, duration, Local::now()) {
            Ok(todo) => {
                self.new_title.clear();
                self.selected_id = Some(todo.id);
                self.refresh_with_message("Todo added to today's workbench.");
            }
            Err(err) => self.log_error("add today todo", format!("{err:#}")),
        }
    }

    fn start_todo_focus(&mut self, id: &str) {
        match todo::schedule_today_todo(
            id,
            optional_text(&self.focus_duration_override),
            self.allow_overlap,
            Local::now(),
        ) {
            Ok(todo) => {
                if let (Some(started_at), Some(ends_at)) = (todo.start, todo.end) {
                    self.active_focus = Some(ActiveFocus {
                        todo_id: Some(todo.id.clone()),
                        title: todo.title,
                        started_at,
                        ends_at,
                        planned_seconds: (ends_at - started_at).num_seconds().max(0),
                    });
                    self.selected_id = Some(todo.id);
                    self.view = View::Today;
                    self.refresh_with_message("Focus session started.");
                } else {
                    self.log_error(
                        "start todo focus",
                        "scheduled todo is missing start/end time",
                    );
                }
            }
            Err(err) => self.log_error("start todo focus", format!("{err:#}")),
        }
    }

    fn start_standalone_focus(&mut self) {
        match parse_duration_seconds(&self.standalone_duration) {
            Ok(seconds) => {
                let now = Local::now();
                self.active_focus = Some(ActiveFocus {
                    todo_id: None,
                    title: clean_title(&self.standalone_title, "Focus"),
                    started_at: now,
                    ends_at: now + chrono::Duration::seconds(seconds),
                    planned_seconds: seconds,
                });
                self.view = View::Today;
                self.message = Some("Standalone focus started.".to_string());
            }
            Err(err) => self.log_error("start standalone focus", format!("{err:#}")),
        }
    }

    fn mark_done(&mut self, id: &str) {
        match todo::mark_done_by_id(id) {
            Ok(_) => self.refresh_with_message("Todo marked done."),
            Err(err) => self.log_error("mark todo done", format!("{err:#}")),
        }
    }

    fn defer_tomorrow(&mut self, id: &str) {
        match todo::defer_todo_record(id, "tomorrow", Local::now().date_naive()) {
            Ok(_) => self.refresh_with_message("Todo deferred to tomorrow."),
            Err(err) => self.log_error("defer todo", format!("{err:#}")),
        }
    }

    fn finish_focus(&mut self, outcome: TimerSessionOutcome) {
        let Some(focus) = self.active_focus.clone() else {
            return;
        };
        let now = Local::now();
        let focused_seconds = focus.focused_seconds(now);

        let result = timer::append_timer_session(
            focus.todo_id.clone(),
            focus.title.clone(),
            focus.started_at,
            now,
            focus.planned_seconds,
            focused_seconds,
            outcome,
        )
        .and_then(|_| {
            if let Some(id) = &focus.todo_id {
                let should_mark_done =
                    outcome == TimerSessionOutcome::Completed && self.mark_done_after_focus;
                todo::record_focus_for_todo(
                    id,
                    should_mark_done,
                    minutes_from_seconds(focused_seconds),
                )?;
            }
            Ok(())
        });

        match result {
            Ok(()) => {
                self.active_focus = None;
                self.refresh_with_message("Focus session recorded.");
            }
            Err(err) => self.log_error("finish focus session", format!("{err:#}")),
        }
    }

    fn copy_review_markdown(&mut self) {
        let Some(data) = &self.data else {
            return;
        };
        match arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(data.review_markdown.clone()))
        {
            Ok(()) => self.message = Some("Review markdown copied.".to_string()),
            Err(err) => self.log_error("copy review markdown", err),
        }
    }

    fn open_data_dir(&mut self) {
        let Some(data) = &self.data else {
            return;
        };
        if let Err(err) = open::that(&data.data_dir) {
            self.log_error("open data directory", err);
        }
    }

    fn open_notes_dir(&mut self) {
        let Some(data) = &self.data else {
            return;
        };
        match &data.notes_dir {
            Some(path) => {
                if let Err(err) = open::that(path) {
                    self.log_error("open notes directory", err);
                }
            }
            None => self.log_error("open notes directory", "notes_dir is not configured yet"),
        }
    }

    fn choose_notes_dir(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Choose tsml notes directory")
            .pick_folder()
        {
            match config::set_notes_dir_path(path) {
                Ok(()) => self.refresh_with_message("Notes directory updated."),
                Err(err) => self.log_error("choose notes directory", format!("{err:#}")),
            }
        }
    }

    fn handle_action(&mut self, action: GuiAction) {
        match action {
            GuiAction::Select(id) => self.selected_id = Some(id),
            GuiAction::Start(id) => self.start_todo_focus(&id),
            GuiAction::Done(id) => self.mark_done(&id),
            GuiAction::DeferTomorrow(id) => self.defer_tomorrow(&id),
        }
    }

    fn selected_todo(&self) -> Option<todo::Todo> {
        let data = self.data.as_ref()?;
        let selected_id = self.selected_id.as_deref()?;
        data.workbench
            .summary
            .todos
            .iter()
            .find(|todo| todo.id == selected_id)
            .cloned()
    }
}

impl eframe::App for TsmlGuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.active_focus.is_some() {
            ui.ctx()
                .request_repaint_after(StdDuration::from_millis(250));
        }

        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(220.0)
            .show_inside(ui, |ui| self.sidebar(ui));

        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(360.0)
            .size_range(310.0..=460.0)
            .show_inside(ui, |ui| self.inspector(ui));

        egui::CentralPanel::default().show_inside(ui, |ui| match self.view {
            View::Today => self.today_view(ui),
            View::Sessions => self.sessions_view(ui),
            View::Review => self.review_view(ui),
            View::Settings => self.settings_view(ui),
        });
    }
}

impl TsmlGuiApp {
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.label(RichText::new("tsml").size(30.0).strong());
        ui.label(RichText::new("to-see-my-life").color(muteds()));
        ui.add_space(22.0);

        nav_button(ui, &mut self.view, View::Today, "Today");
        nav_button(ui, &mut self.view, View::Sessions, "Sessions");
        nav_button(ui, &mut self.view, View::Review, "Review");
        nav_button(ui, &mut self.view, View::Settings, "Settings");

        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            if ui.button("Refresh").clicked() {
                self.refresh_with_message("Refreshed.");
            }
            ui.add_space(8.0);
            if let Some(data) = &self.data {
                ui.label(
                    RichText::new(format!("Loaded {}", data.loaded_at.format("%H:%M:%S")))
                        .color(muteds()),
                );
            }
        });
    }

    fn today_view(&mut self, ui: &mut egui::Ui) {
        let Some(data) = self.data.clone() else {
            self.empty_state(ui);
            return;
        };
        let now = Local::now();

        header(
            ui,
            "Today Workbench",
            &data.workbench.summary.date.to_string(),
        );
        self.message_bar(ui);

        soft_panel(ui, |ui| {
            ui.horizontal(|ui| {
                stat_tile(
                    ui,
                    "Planned",
                    &format_minutes(data.workbench.summary.total_minutes),
                );
                stat_tile(
                    ui,
                    "Focused",
                    &format_minutes(data.workbench.summary.focused_minutes),
                );
                stat_tile(
                    ui,
                    "Done",
                    &data.workbench.summary.completed_count.to_string(),
                );
                stat_tile(
                    ui,
                    "Open",
                    &data.workbench.summary.unfinished_count.to_string(),
                );
            });
            if data.workbench.carried_over_count > 0 {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!(
                        "Carried over {} deferred todo(s).",
                        data.workbench.carried_over_count
                    ))
                    .color(accent()),
                );
            }
        });

        ui.add_space(12.0);
        soft_panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [ui.available_width() * 0.58, 30.0],
                    egui::TextEdit::singleline(&mut self.new_title).hint_text("New todo"),
                );
                ui.add_sized(
                    [88.0, 30.0],
                    egui::TextEdit::singleline(&mut self.new_duration).hint_text("25m"),
                );
                if ui.button("Add").clicked() {
                    self.add_today_todo();
                }
            });
        });

        ui.add_space(12.0);
        let selected = self.selected_id.as_deref();
        let mut action = None;
        ui.columns(3, |columns| {
            if action.is_none() {
                action = lane_ui(&mut columns[0], "TODO", &data.workbench.todo, selected, now);
            }
            if action.is_none() {
                action = lane_ui(
                    &mut columns[1],
                    "DOING",
                    &data.workbench.doing,
                    selected,
                    now,
                );
            }
            if action.is_none() {
                action = lane_ui(&mut columns[2], "DONE", &data.workbench.done, selected, now);
            }
        });

        if let Some(action) = action {
            self.handle_action(action);
        }
    }

    fn sessions_view(&mut self, ui: &mut egui::Ui) {
        let Some(data) = self.data.clone() else {
            self.empty_state(ui);
            return;
        };
        header(ui, "Session History", "recent focus records");
        self.message_bar(ui);

        soft_panel(ui, |ui| {
            ui.horizontal(|ui| {
                stat_tile(
                    ui,
                    "This week",
                    &format_minutes(data.weekly.focused_minutes),
                );
                stat_tile(ui, "Planned", &format_minutes(data.weekly.planned_minutes));
                stat_tile(ui, "Completed", &data.weekly.completed_count.to_string());
                stat_tile(ui, "Expired", &data.weekly.expired_count.to_string());
            });
        });

        ui.add_space(12.0);
        soft_panel(ui, |ui| {
            ui.heading("Recent sessions");
            ui.add_space(8.0);
            session_table(ui, &data.recent_sessions);
        });
    }

    fn review_view(&mut self, ui: &mut egui::Ui) {
        let Some(data) = self.data.clone() else {
            self.empty_state(ui);
            return;
        };
        header(ui, "Review Draft", "structured facts for Obsidian");
        self.message_bar(ui);

        ui.horizontal(|ui| {
            if ui.button("Copy Markdown").clicked() {
                self.copy_review_markdown();
            }
            if ui.button("Open Notes").clicked() {
                self.open_notes_dir();
            }
        });
        ui.add_space(12.0);

        let mut preview = data.review_markdown;
        soft_panel(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut preview)
                    .desired_rows(28)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace)
                    .interactive(false),
            );
        });
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        let Some(data) = self.data.clone() else {
            self.empty_state(ui);
            return;
        };
        header(ui, "Settings", "local files and integration points");
        self.message_bar(ui);

        soft_panel(ui, |ui| {
            ui.heading("Storage");
            ui.add_space(6.0);
            path_row(ui, "Data", &data.data_dir);
            path_row(ui, "Log", &data.log_file);
            match &data.notes_dir {
                Some(path) => path_row(ui, "Notes", path),
                None => {
                    ui.label(RichText::new("Notes: not configured").color(muteds()));
                }
            };
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Open Data Directory").clicked() {
                    self.open_data_dir();
                }
                if ui.button("Choose Notes Directory").clicked() {
                    self.choose_notes_dir();
                }
            });
        });

        ui.add_space(12.0);
        soft_panel(ui, |ui| {
            ui.heading("GUI boundary");
            ui.label("The GUI uses the same todos.json and timer_sessions.json as the CLI.");
            ui.label(
                "Terminal timer mode remains available through tsml today start and tsml timer.",
            );
        });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.heading("Focus");
        ui.add_space(8.0);

        if let Some(focus) = self.active_focus.clone() {
            self.active_focus_panel(ui, focus);
            return;
        }

        if let Some(todo) = self.selected_todo() {
            self.todo_inspector(ui, todo);
        } else {
            ui.label(RichText::new("Select a todo from the workbench.").color(muteds()));
        }

        ui.add_space(16.0);
        self.standalone_focus_panel(ui);
    }

    fn active_focus_panel(&mut self, ui: &mut egui::Ui, focus: ActiveFocus) {
        let now = Local::now();
        let remaining = focus.remaining_seconds(now);
        let completed = remaining == 0;

        soft_panel(ui, |ui| {
            ui.label(RichText::new(&focus.title).size(20.0).strong());
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(format_seconds(remaining))
                        .size(42.0)
                        .monospace()
                        .strong(),
                );
            });
            ui.add(
                egui::ProgressBar::new(focus.progress(now))
                    .desired_width(f32::INFINITY)
                    .text(format!(
                        "{} focused",
                        format_seconds(focus.focused_seconds(now))
                    )),
            );
            ui.add_space(8.0);
            ui.checkbox(
                &mut self.mark_done_after_focus,
                "Mark todo done when completed",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button(if completed {
                        "Record Completed"
                    } else {
                        "Complete Now"
                    })
                    .clicked()
                {
                    self.finish_focus(TimerSessionOutcome::Completed);
                }
                if ui.button("Stop").clicked() {
                    self.finish_focus(TimerSessionOutcome::Quit);
                }
            });
        });
    }

    fn todo_inspector(&mut self, ui: &mut egui::Ui, todo: todo::Todo) {
        soft_panel(ui, |ui| {
            ui.label(RichText::new(&todo.title).size(19.0).strong());
            ui.label(RichText::new(todo.id.clone()).monospace().color(muteds()));
            ui.add_space(8.0);
            ui.label(todo::todo_time_label(&todo));
            ui.label(format!(
                "Estimate {}",
                format_minutes(todo.duration_minutes)
            ));
            ui.label(format!("Focused {}", format_minutes(todo.focused_minutes)));
            let progress = if todo.duration_minutes <= 0 {
                0.0
            } else {
                todo.focused_minutes as f32 / todo.duration_minutes as f32
            };
            ui.add(egui::ProgressBar::new(progress.clamp(0.0, 1.0)).desired_width(f32::INFINITY));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label("Override");
                ui.add_sized(
                    [80.0, 28.0],
                    egui::TextEdit::singleline(&mut self.focus_duration_override)
                        .hint_text(format!("{}m", todo.duration_minutes)),
                );
            });
            ui.checkbox(&mut self.allow_overlap, "Allow overlap");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Start Focus").clicked() {
                    self.start_todo_focus(&todo.id);
                }
                if ui.button("Done").clicked() {
                    self.mark_done(&todo.id);
                }
            });
            if ui.button("Defer Tomorrow").clicked() {
                self.defer_tomorrow(&todo.id);
            }
        });
    }

    fn standalone_focus_panel(&mut self, ui: &mut egui::Ui) {
        soft_panel(ui, |ui| {
            ui.heading("Standalone");
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.standalone_title)
                    .desired_width(f32::INFINITY)
                    .hint_text("Title"),
            );
            ui.add_sized(
                [96.0, 28.0],
                egui::TextEdit::singleline(&mut self.standalone_duration).hint_text("25m"),
            );
            if ui.button("Start Standalone Focus").clicked() {
                self.start_standalone_focus();
            }
        });
    }

    fn message_bar(&mut self, ui: &mut egui::Ui) {
        if let Some(message) = &self.message {
            ui.label(RichText::new(message).color(accent()));
        }
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        self.message_bar(ui);
        if ui.button("Retry").clicked() {
            self.refresh();
        }
    }
}

fn load_gui_data() -> Result<GuiData> {
    config::ensure_todos_file()?;
    config::ensure_timer_sessions_file()?;

    let now = Local::now();
    let workbench = workbench::load_today_workbench(now)?;
    let weekly = stats::load_weekly_stats(now)?;
    let mut recent_sessions = timer::load_timer_sessions()?;
    recent_sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    recent_sessions.truncate(16);
    let review_markdown = format!(
        "{}\n{}",
        workbench.summary.markdown(now),
        timer::timer_sessions_markdown(now.date_naive())?
    );

    let data_dir = config::config_dir()?;
    let log_file = logging::log_file_path("gui.log")?;

    Ok(GuiData {
        loaded_at: now,
        log_file,
        data_dir,
        notes_dir: config::load_config_if_exists()?.map(|config| config.notes_dir),
        workbench,
        weekly,
        recent_sessions,
        review_markdown,
    })
}

fn lane_ui(
    ui: &mut egui::Ui,
    title: &str,
    todos: &[todo::Todo],
    selected: Option<&str>,
    now: DateTime<Local>,
) -> Option<GuiAction> {
    let mut action = None;
    soft_panel(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading(title);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(todos.len().to_string()).color(muteds()));
            });
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .id_salt(("workbench-lane", title))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if todos.is_empty() {
                    ui.label(RichText::new("No items").color(muteds()));
                }
                for item in todos {
                    if action.is_none() {
                        action = todo_card(ui, item, selected, now);
                    } else {
                        todo_card(ui, item, selected, now);
                    }
                    ui.add_space(8.0);
                }
            });
    });
    action
}

fn todo_card(
    ui: &mut egui::Ui,
    item: &todo::Todo,
    selected: Option<&str>,
    now: DateTime<Local>,
) -> Option<GuiAction> {
    let mut action = None;
    let selected = selected == Some(item.id.as_str());
    let frame = egui::Frame::group(ui.style())
        .fill(if selected {
            Color32::from_rgb(231, 241, 255)
        } else {
            Color32::from_rgb(252, 252, 253)
        })
        .stroke(Stroke::new(
            1.0,
            if selected {
                Color32::from_rgb(90, 145, 220)
            } else {
                Color32::from_rgb(226, 228, 232)
            },
        ));

    let response = frame
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&item.title).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(&item.id).monospace().color(muteds()));
                });
            });
            ui.label(RichText::new(todo::todo_time_label(item)).color(muteds()));
            ui.add_space(4.0);
            let progress = if item.duration_minutes <= 0 {
                0.0
            } else {
                item.focused_minutes as f32 / item.duration_minutes as f32
            };
            ui.add(
                egui::ProgressBar::new(progress.clamp(0.0, 1.0))
                    .desired_width(f32::INFINITY)
                    .text(format!(
                        "{} / {}",
                        format_minutes(item.focused_minutes),
                        format_minutes(item.duration_minutes)
                    )),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let status = item.effective_status(now);
                let closed = item.status == todo::TodoStatus::Done
                    || item.status == todo::TodoStatus::Cancelled;
                ui.label(RichText::new(status.to_string()).color(accent()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Inspect").clicked() {
                        action = Some(GuiAction::Select(item.id.clone()));
                    }
                    if !closed {
                        if ui.small_button("Tomorrow").clicked() {
                            action = Some(GuiAction::DeferTomorrow(item.id.clone()));
                        }
                        if ui.small_button("Done").clicked() {
                            action = Some(GuiAction::Done(item.id.clone()));
                        }
                        if ui.small_button("Start").clicked() {
                            action = Some(GuiAction::Start(item.id.clone()));
                        }
                    }
                });
            });
        })
        .response;

    if action.is_none() && response.clicked() {
        action = Some(GuiAction::Select(item.id.clone()));
    }
    action
}

fn session_table(ui: &mut egui::Ui, sessions: &[TimerSessionRecord]) {
    if sessions.is_empty() {
        ui.label(RichText::new("No focus sessions yet.").color(muteds()));
        return;
    }

    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::auto())
        .column(Column::remainder())
        .column(Column::auto())
        .column(Column::auto())
        .header(22.0, |mut header| {
            header.col(|ui| {
                ui.strong("Start");
            });
            header.col(|ui| {
                ui.strong("Title");
            });
            header.col(|ui| {
                ui.strong("Focused");
            });
            header.col(|ui| {
                ui.strong("Outcome");
            });
        })
        .body(|mut body| {
            for session in sessions {
                body.row(26.0, |mut row| {
                    row.col(|ui| {
                        ui.label(session.started_at.format("%m-%d %H:%M").to_string());
                    });
                    row.col(|ui| {
                        ui.label(&session.title);
                    });
                    row.col(|ui| {
                        ui.label(format_minutes(minutes_from_seconds(
                            session.focused_seconds,
                        )));
                    });
                    row.col(|ui| {
                        ui.label(session.outcome.to_string());
                    });
                });
            }
        });
}

fn header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(28.0).strong());
            ui.label(RichText::new(subtitle).color(muteds()));
        });
    });
    ui.add_space(12.0);
}

fn nav_button(ui: &mut egui::Ui, view: &mut View, target: View, label: &str) {
    let selected = *view == target;
    if ui
        .add_sized([180.0, 34.0], egui::Button::selectable(selected, label))
        .clicked()
    {
        *view = target;
    }
}

fn soft_panel<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::group(ui.style())
        .fill(Color32::from_rgb(248, 249, 251))
        .stroke(Stroke::new(1.0, Color32::from_rgb(224, 226, 231)))
        .show(ui, add_contents)
}

fn stat_tile(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(value).size(20.0).strong());
        ui.label(RichText::new(label).color(muteds()));
    });
    ui.add_space(22.0);
}

fn path_row(ui: &mut egui::Ui, label: &str, path: &PathBuf) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{label}:")).strong());
        ui.label(
            RichText::new(path.display().to_string())
                .monospace()
                .color(muteds()),
        );
    });
}

fn apply_macos_style(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = Color32::from_rgb(242, 244, 247);
    style.visuals.window_fill = Color32::from_rgb(247, 248, 250);
    ctx.set_global_style(style);
}

fn optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn clean_title(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
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

fn accent() -> Color32 {
    Color32::from_rgb(36, 99, 180)
}

fn muteds() -> Color32 {
    Color32::from_rgb(102, 112, 128)
}
