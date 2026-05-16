use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "papaproc",
    version,
    about = "Dependency-aware process runner for local development"
)]
pub struct Cli {
    #[arg(short, long, default_value = "papaproc.yaml", global = true)]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Write a sample papaproc.yaml.
    Init,
    /// Validate the config file.
    Validate,
    /// Run a TUI dev session for all auto tasks or selected tasks/groups.
    Run { selectors: Vec<String> },
    /// Render a pasteable static snapshot from config.
    Snapshot,
}
