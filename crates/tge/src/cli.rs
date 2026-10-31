use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "tge", version, about = "Telegram premium emoji catalog tool")]
pub struct Cli {
    /// Emit exactly one JSON document on stdout
    #[arg(long, global = true)]
    pub json: bool,

    /// State root (default: TG_PREMIUM_EMOJI_HOME or ~/.tg-premium-emoji)
    #[arg(long, global = true, value_name = "PATH")]
    pub home: Option<PathBuf>,

    /// Built-in catalog directory
    #[arg(long, global = true, value_name = "PATH")]
    pub builtin: Option<PathBuf>,

    /// Extra catalog directory
    #[arg(long, global = true, value_name = "PATH")]
    pub catalog_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Subcommands are added by later tasks.
#[derive(Debug, Subcommand)]
pub enum Command {}
