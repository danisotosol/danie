mod cli;
mod doctor;
mod logging;
mod textutil;
mod ui;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use danie_core::DanieStore;

use cli::{Cli, Command, MapCommand};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let store_dir = cli.store.unwrap_or_else(|| PathBuf::from(".danie"));
    let rt = tokio::runtime::Runtime::new().expect("failed to start the tokio runtime");

    match cli.command {
        Command::Doctor => ExitCode::from(doctor::run(&rt)),
        Command::Map { cmd } => run_map(cmd, &store_dir),
        Command::Teach { topic } => run_tui(ui::Mode::Teach, topic, &store_dir, &rt),
        Command::Probe { topic } => run_tui(ui::Mode::ProbeOnly, topic, &store_dir, &rt),
        Command::Review { .. } => run_tui(ui::Mode::Review, None, &store_dir, &rt),
    }
}

fn run_tui(
    mode: ui::Mode,
    topic: Option<String>,
    store_dir: &Path,
    rt: &tokio::runtime::Runtime,
) -> ExitCode {
    match ui::run_app(mode, topic, store_dir, rt) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_map(cmd: MapCommand, store_dir: &Path) -> ExitCode {
    if let Err(error) = logging::init(store_dir) {
        eprintln!("error: could not open the log file: {error}");
        return ExitCode::FAILURE;
    }
    let store = match DanieStore::open(store_dir) {
        Ok(store) => store,
        Err(error) => {
            eprintln!(
                "error: could not open the store at {}: {error}",
                store_dir.display()
            );
            return ExitCode::FAILURE;
        }
    };
    match cmd {
        MapCommand::List => {
            let slugs = store.list_maps();
            if slugs.is_empty() {
                println!("No maps stored yet. Run `danie teach <topic>` to create one.");
            } else {
                for slug in slugs {
                    println!("{slug}");
                }
            }
            ExitCode::SUCCESS
        }
        MapCommand::Show { slug } => match store.load_map(&slug) {
            Ok(map) => {
                print!("{}", map.to_markdown());
                ExitCode::SUCCESS
            }
            Err(danie_core::CoreError::NotFound(path)) => {
                eprintln!("error: no stored map matches \"{slug}\" ({path})");
                eprintln!("hint: list available maps with `danie map list`");
                ExitCode::FAILURE
            }
            Err(error) => {
                eprintln!("error: could not load map \"{slug}\": {error}");
                ExitCode::FAILURE
            }
        },
    }
}
