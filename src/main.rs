mod cli;
mod config;
mod review;
mod timer;
mod todo;
mod util;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, ConfigCommand, TodoCommand};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            config::init_config_interactive()?;
        }
        Commands::Config { command } => match command {
            ConfigCommand::Show => config::show_config()?,
            ConfigCommand::SetNotesDir { path } => config::set_notes_dir(path)?,
        },
        Commands::Review => review::run_review()?,
        Commands::Todo { command } => match command {
            TodoCommand::Add {
                title,
                start,
                duration,
            } => todo::add_todo(title, start, duration)?,
            TodoCommand::List { all } => todo::list_todos(all)?,
            TodoCommand::Done { id } => todo::mark_done(id)?,
        },
        Commands::Timer { title, duration } => timer::run_timer(title, duration)?,
    }

    Ok(())
}
