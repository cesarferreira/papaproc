use anyhow::{Context, Result};
use clap::Parser;
use papaproc::cli::{Cli, Command};
use papaproc::config::LoadConfig;
use papaproc::snapshot::render_snapshot;
use papaproc::state::{SessionState, TaskStatus};
use papaproc::supervisor::Supervisor;
use std::fs;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => init_config(&cli.config),
        Command::Validate => {
            let config = LoadConfig::from_path(&cli.config)?;
            println!(
                "{} is valid ({} tasks)",
                cli.config.display(),
                config.tasks.len()
            );
            Ok(())
        }
        Command::Snapshot => {
            let config = LoadConfig::from_path(&cli.config)?;
            let mut state = SessionState::new(
                config.project.clone().unwrap_or_else(|| "papaproc".into()),
                config.tasks.keys().cloned(),
            );
            for (name, task) in &config.tasks {
                if task.mode == papaproc::config::Mode::Manual {
                    state.tasks.get_mut(name).unwrap().status = TaskStatus::Idle;
                } else {
                    state.tasks.get_mut(name).unwrap().status = TaskStatus::Waiting;
                }
            }
            print!("{}", render_snapshot(&state));
            Ok(())
        }
        Command::Run { selectors } => {
            let config = LoadConfig::from_path(&cli.config)?;
            let supervisor = Supervisor::new(config)?;
            papaproc::tui::run_tui(supervisor, selectors).await
        }
    }
}

fn init_config(path: &std::path::Path) -> Result<()> {
    if path.exists() {
        anyhow::bail!("{} already exists", path.display());
    }
    fs::write(path, SAMPLE_CONFIG)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}

const SAMPLE_CONFIG: &str = r#"version: 1
project: papaproc-demo

groups:
  backend:
    tasks: [db, api]
  frontend:
    tasks: [web]

tasks:
  db:
    cmd: docker compose up db
    ready:
      tcp: localhost:5432
      timeout: 30s

  api:
    cmd: cargo run
    depends_on:
      db: ready
    ready:
      http: http://localhost:8080/health
    restart:
      on_dependency_restart: true

  web:
    cmd: bun dev
    depends_on:
      api: ready
    ready:
      http: http://localhost:5173

  tests:
    cmd: bun test --watch
    mode: manual
"#;
