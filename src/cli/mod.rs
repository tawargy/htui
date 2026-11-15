pub mod events;
pub mod ls;
pub mod open;
pub mod status;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "htui",
    version,
    about = "Terminal multiplexer for herdr-core (hc)"
)]
pub struct Cli {
    /// Path to hc Unix domain socket
    #[arg(long, global = true)]
    pub socket: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// List machines, spaces, and terminals
    Ls {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        label: Vec<String>,
    },
    /// Show core identity, version, uptime, and machine status
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Stream live events from hc core
    Events {
        #[arg(long)]
        json: bool,
    },
    /// Open TUI focused on a specific space
    Open {
        space: String,
        #[arg(long)]
        create: bool,
    },
}

pub fn parse_label_filter(raw: &str) -> Result<(String, String), i32> {
    let Some((k, v)) = raw.split_once('=') else {
        return Err(2);
    };
    if k.trim().is_empty() || v.trim().is_empty() {
        return Err(2);
    }
    Ok((k.trim().to_string(), v.trim().to_string()))
}
