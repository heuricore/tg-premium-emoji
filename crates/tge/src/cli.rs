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

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Validate catalogs against the schemas and semantic rules
    Validate {
        /// Catalog directory (default: every discovered catalog)
        path: Option<PathBuf>,
    },
    /// List the discovered catalogs
    Catalogs,
    /// Check the installation and state directory
    Doctor {
        /// Remove installed versions other than the one `bin/latest` points to
        #[arg(long)]
        prune: bool,
    },
    /// Regenerate the JSON Schemas (developer tool)
    #[command(hide = true)]
    Schema {
        /// Output directory
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
    },
}
