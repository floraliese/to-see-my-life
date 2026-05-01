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
    },
    /// List scheduled todos in start-time order.
    List {
        /// Include done and cancelled todos.
        #[arg(long)]
        all: bool,
    },
    /// Mark a todo as done. If omitted, choose from active/scheduled todos.
    Done { id: Option<String> },
}
