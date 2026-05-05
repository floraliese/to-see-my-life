// cli — 命令行參數定義，基於 clap derive。
// 定義 tsml 的所有子命令、標誌和參數，不包含業務邏輯。
// 子命令包括：init, config, review, todo(today)/add/list/done/cancel/
// delete/edit/reschedule, timer(title,duration,plain,no_notify),
// Today(add/start/done/defer), Stats(week)。
// 新增子命令後需同步修改 main.rs 的 dispatch 和對應模塊的實現。

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "tsml")]
#[command(
    version,
    about = "A local CLI for reviewing days, scheduling todos, and timing focus blocks."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize .config beside the executable.
    Init,
    /// Read or update local configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Answer daily questions and generate a Markdown note.
    Review,
    /// Manage scheduled todos.
    Todo {
        #[command(subcommand)]
        command: TodoCommand,
    },
    /// Start a countdown for the current todo, next todo, or a standalone timer.
    Timer {
        /// Timer title for standalone mode.
        #[arg(short, long)]
        title: Option<String>,
        /// Duration for standalone mode, for example 25m, 1h, or 1h30m.
        #[arg(short, long)]
        duration: Option<String>,
        /// Print timer progress as plain text instead of opening the TUI.
        #[arg(long)]
        plain: bool,
        /// Skip desktop notifications when the timer finishes.
        #[arg(long)]
        no_notify: bool,
    },
    /// Show or manage today's workbench.
    Today {
        #[command(subcommand)]
        command: Option<TodayCommand>,
    },
    /// Open the native desktop GUI workbench.
    Gui,
    /// Show personal productivity statistics.
    Stats {
        /// Show statistics for the current week.
        #[arg(long)]
        week: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    /// Print the current configuration.
    Show,
    /// Set the directory used for generated Markdown notes.
    SetNotesDir { path: String },
}

#[derive(Subcommand, Debug)]
pub enum TodoCommand {
    /// Add a scheduled todo. Missing fields are asked interactively.
    Add {
        /// Todo title.
        title: Option<String>,
        /// Start time, for example 14:00 or 2026-05-02 14:00.
        #[arg(short, long)]
        start: Option<String>,
        /// Duration, for example 30m, 1h, or 1h30m.
        #[arg(short, long)]
        duration: Option<String>,
        /// Allow overlapping an existing open todo.
        #[arg(long)]
        force: bool,
    },
    /// List scheduled todos in start-time order.
    List {
        /// Include done and cancelled todos.
        #[arg(long)]
        all: bool,
    },
    /// Mark a todo as done. If omitted, choose from active/scheduled todos.
    Done { id: Option<String> },
    /// Mark a todo as cancelled while keeping it in history.
    Cancel { id: Option<String> },
    /// Delete a todo permanently.
    Delete {
        id: String,
        /// Delete without asking for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Edit a todo title, start time, or duration.
    Edit {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        start: Option<String>,
        #[arg(long)]
        duration: Option<String>,
        /// Allow overlapping an existing open todo.
        #[arg(long)]
        force: bool,
    },
    /// Move a todo to a new time block.
    Reschedule {
        id: String,
        #[arg(long)]
        start: String,
        #[arg(long)]
        duration: Option<String>,
        /// Allow overlapping an existing open todo.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum TodayCommand {
    /// Add an unscheduled todo to today's workbench.
    Add {
        /// Todo title.
        title: String,
        /// Estimated focus duration, for example 25m, 1h, or 1h30m.
        #[arg(short, long)]
        duration: Option<String>,
    },
    /// Move a todo into Doing by scheduling it now, then start the terminal timer.
    Start {
        id: String,
        /// Override the todo estimate before starting.
        #[arg(short, long)]
        duration: Option<String>,
        /// Allow overlapping an existing open todo.
        #[arg(long)]
        force: bool,
        /// Print timer progress as plain text instead of opening the TUI.
        #[arg(long)]
        plain: bool,
        /// Skip desktop notifications when the timer finishes.
        #[arg(long)]
        no_notify: bool,
    },
    /// Mark a todo done from the today workbench.
    Done { id: String },
    /// Defer an unfinished todo to another day.
    Defer {
        id: String,
        /// Target day: tomorrow, today, or YYYY-MM-DD.
        #[arg(long, default_value = "tomorrow")]
        to: String,
    },
}
