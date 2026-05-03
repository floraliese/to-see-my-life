// main — 程序入口與命令分發。
// 使用 clap 解析命令行參數後直接調度到對應的子模塊。
// 不包含業務邏輯，只做匹配和轉發。
// 新增子命令時需要在 cli.rs 中定義、main.rs 中分發、對應模塊中實現。

mod cli;
mod config;
mod review;
mod stats;
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
                force,
            } => todo::add_todo(title, start, duration, force)?,
            TodoCommand::List { all } => todo::list_todos(all)?,
            TodoCommand::Done { id } => todo::mark_done(id)?,
            TodoCommand::Cancel { id } => todo::cancel_todo(id)?,
            TodoCommand::Delete { id, yes } => todo::delete_todo(id, yes)?,
            TodoCommand::Edit {
                id,
                title,
                start,
                duration,
                force,
            } => todo::edit_todo(id, title, start, duration, force)?,
            TodoCommand::Reschedule {
                id,
                start,
                duration,
                force,
            } => todo::reschedule_todo(id, start, duration, force)?,
        },
        Commands::Timer {
            title,
            duration,
            plain,
            no_notify,
        } => timer::run_timer(title, duration, plain, no_notify)?,
        Commands::Today => todo::print_today()?,
        Commands::Stats { week } => stats::print_stats(week)?,
    }

    Ok(())
}
