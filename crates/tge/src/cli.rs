use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::model::pack::{By, Fit, Format};

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
    /// Search labeled emoji across the visible packs
    Find {
        /// Search words (Russian or English)
        #[arg(required = true)]
        query: Vec<String>,
        /// Only this pack (short name)
        #[arg(long)]
        pack: Option<String>,
        /// Only this catalog (id)
        #[arg(long)]
        catalog: Option<String>,
        /// Only emoji that fit this context
        #[arg(long, value_enum)]
        fits: Option<Fit>,
        /// Only emoji with this repaint flag
        #[arg(long)]
        repaint: Option<bool>,
        /// Only emoji of this format
        #[arg(long, value_enum)]
        format: Option<Format>,
        /// Maximum number of hits
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Print ready-to-paste snippets for one emoji
    Snippet {
        /// Pack short name
        pack: String,
        /// Emoji number `n`, or the custom emoji id (all digits, longer than 4)
        target: String,
        /// Button label
        #[arg(long, default_value = "LABEL")]
        text: String,
    },
    /// Apply a batch of label edits to a pack
    Label {
        /// Pack short name
        pack: String,
        /// JSON array of edits, a file path or `-` for stdin
        #[arg(long, value_name = "FILE|-")]
        edits: String,
        /// Who made the edits (`human` protects them from later agent edits)
        #[arg(long, value_enum, default_value_t = By::Agent)]
        by: By,
        /// Let agent edits overwrite emoji labeled by a human
        #[arg(long)]
        force: bool,
    },
    /// List groups of look-alike emoji within a pack
    Similar {
        /// Pack short name
        pack: String,
    },
    /// Show what is left to label in one pack or in all visible packs
    Report {
        /// Pack short name (default: all visible packs)
        pack: Option<String>,
    },
    /// Work with emoji packs
    Pack {
        #[command(subcommand)]
        command: PackCommand,
    },
    /// Regenerate the JSON Schemas (developer tool)
    #[command(hide = true)]
    Schema {
        /// Output directory
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum PackCommand {
    /// Download a custom emoji set through the Bot API (BOT_TOKEN), render its preview sheets and
    /// write pack.json, keeping existing labels
    Build {
        /// Set name, as in t.me/addemoji/<SHORT_NAME>
        short_name: String,
        /// Catalog to write into (default: local); not together with --catalog-dir
        #[arg(long, value_enum)]
        catalog: Option<CatalogChoice>,
    },
    /// Print the preview sheets of a pack with the emoji numbers each one shows
    Preview {
        /// Pack short name
        short_name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CatalogChoice {
    Local,
    Project,
}
