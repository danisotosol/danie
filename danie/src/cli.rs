use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "danie",
    version,
    about = "One-to-one AI tutor in your terminal: probe, plan a DAG, teach one node at a time"
)]
pub struct Cli {
    #[arg(
        long,
        value_name = "DIR",
        global = true,
        help = "Directory of the danie store (default: .danie)"
    )]
    pub store: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Start or resume a tutoring session (TUI)")]
    Teach {
        #[arg(
            value_name = "TOPIC",
            help = "Topic to study; asked interactively when omitted"
        )]
        topic: Option<String>,
    },
    #[command(about = "Run only the diagnostic probe (TUI)")]
    Probe {
        #[arg(
            value_name = "TOPIC",
            help = "Topic to probe; asked interactively when omitted"
        )]
        topic: Option<String>,
    },
    #[command(about = "Spaced-repetition review of due cards (TUI)")]
    Review {
        #[arg(value_name = "TOPIC", help = "Optional; unused by the review flow")]
        topic: Option<String>,
    },
    #[command(about = "Check configuration and provider connectivity")]
    Doctor,
    #[command(about = "Inspect stored knowledge maps")]
    Map {
        #[command(subcommand)]
        cmd: MapCommand,
    },
}

#[derive(Subcommand)]
pub enum MapCommand {
    #[command(about = "List stored map slugs")]
    List,
    #[command(about = "Print a stored map's markdown")]
    Show {
        #[arg(value_name = "SLUG", help = "Map slug as listed by `danie map list`")]
        slug: String,
    },
}
