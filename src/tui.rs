use crate::snapshot::render_snapshot;
use crate::state::{SessionState, TaskStatus};
use crate::supervisor::Supervisor;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

pub async fn run_tui(supervisor: Supervisor, selectors: Vec<String>) -> Result<()> {
    let supervisor = Arc::new(supervisor);
    let state = supervisor.state();
    let graph = supervisor.graph_text();
    let startup_supervisor = supervisor.clone();
    let start = tokio::spawn(async move { startup_supervisor.start_selected(&selectors).await });

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut selected = 0usize;
    let mut panel = DetailPanel::Help;
    let mut errors_only = false;

    let mut start = Some(start);
    let result = loop {
        let snapshot = state.lock().await.clone();
        terminal.draw(|frame| draw(frame, &snapshot, selected, panel, errors_only, &graph))?;

        if event::poll(Duration::from_millis(150))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    supervisor.stop_all().await;
                    break Ok(());
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = selected
                        .saturating_add(1)
                        .min(snapshot.tasks.len().saturating_sub(1));
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    selected = selected.saturating_sub(1);
                }
                KeyCode::Enter => {
                    if let Some(name) = selected_task_name(&snapshot, selected) {
                        let supervisor = supervisor.clone();
                        tokio::spawn(async move {
                            let _ = supervisor.start_task(&name).await;
                        });
                    }
                }
                KeyCode::Char('x') => {
                    if let Some(name) = selected_task_name(&snapshot, selected) {
                        let supervisor = supervisor.clone();
                        tokio::spawn(async move {
                            let _ = supervisor.stop_task(&name).await;
                        });
                    }
                }
                KeyCode::Char('r') => {
                    if let Some(name) = selected_task_name(&snapshot, selected) {
                        let supervisor = supervisor.clone();
                        tokio::spawn(async move {
                            let _ = supervisor.restart_task(&name, false).await;
                        });
                    }
                }
                KeyCode::Char('R') => {
                    if let Some(name) = selected_task_name(&snapshot, selected) {
                        let supervisor = supervisor.clone();
                        tokio::spawn(async move {
                            let _ = supervisor.restart_task(&name, true).await;
                        });
                    }
                }
                KeyCode::Char('s') => {
                    let text = render_snapshot(&snapshot);
                    let mut state = state.lock().await;
                    state.last_event = Some(format!("snapshot generated:\n{text}"));
                }
                KeyCode::Char('e') => {
                    errors_only = !errors_only;
                }
                KeyCode::Char('g') => {
                    panel = DetailPanel::Graph;
                }
                KeyCode::Char('f') => {
                    panel = DetailPanel::Failures;
                }
                KeyCode::Char('?') => {
                    panel = DetailPanel::Help;
                }
                _ => {}
            }
        }

        if start.as_ref().is_some_and(|handle| handle.is_finished()) {
            let handle = start.take().unwrap();
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    state.lock().await.last_event = Some(format!("startup failed: {error}"));
                }
                Err(error) => {
                    state.lock().await.last_event = Some(format!("startup task failed: {error}"));
                }
            }
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailPanel {
    Help,
    Graph,
    Failures,
}

fn draw(
    frame: &mut Frame<'_>,
    state: &SessionState,
    selected: usize,
    panel: DetailPanel,
    errors_only: bool,
    graph: &str,
) {
    let root = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(frame.area());
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(root[1]);

    let tasks: Vec<_> = state.tasks.values().collect();
    let items = tasks
        .iter()
        .enumerate()
        .map(|(index, task)| {
            let prefix = if index == selected { ">" } else { " " };
            let marker = status_marker(task.status);
            let detail = task.detail.as_deref().unwrap_or("");
            ListItem::new(format!(
                "{prefix} {marker} {:16} {:10} {detail}",
                task.name,
                task.status.label()
            ))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .title(format!("Stack: {}", state.project))
                .borders(Borders::ALL),
        ),
        root[0],
    );

    let selected_task = tasks.get(selected).or_else(|| tasks.first());
    let logs = selected_task
        .map(|task| {
            task.logs
                .iter()
                .filter(|line| !errors_only || is_error_line(line))
                .rev()
                .take(200)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let title = selected_task
        .map(|task| {
            if errors_only {
                format!("Logs: {} (errors only)", task.name)
            } else {
                format!("Logs: {}", task.name)
            }
        })
        .unwrap_or_else(|| "Logs".into());
    frame.render_widget(
        Paragraph::new(logs)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        right[0],
    );

    let diagnosis = detail_text(state, selected_task.copied(), panel, graph);
    frame.render_widget(
        Paragraph::new(diagnosis)
            .block(
                Block::default()
                    .title("Diagnosis / Last Event")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        right[1],
    );
}

fn detail_text(
    state: &SessionState,
    selected_task: Option<&crate::state::TaskState>,
    panel: DetailPanel,
    graph: &str,
) -> String {
    if let Some(diagnosis) = selected_task.and_then(|task| task.diagnosis.as_ref()) {
        return format!(
            "Likely cause: {}\nSuggested action: {}\nEvidence:\n{}",
            diagnosis.title,
            diagnosis.suggest,
            diagnosis.evidence.join("\n")
        );
    }

    match panel {
        DetailPanel::Help => state.last_event.clone().unwrap_or_else(|| {
            "j/k move  enter start  x stop  r restart  R restart dependants  e errors  g graph  f failures  s snapshot  ? help  q quit".into()
        }),
        DetailPanel::Graph => graph.to_string(),
        DetailPanel::Failures => {
            let failures = state
                .tasks
                .values()
                .filter(|task| matches!(task.status, TaskStatus::Failed | TaskStatus::CrashLoop))
                .map(|task| {
                    let detail = task.detail.as_deref().unwrap_or("");
                    format!("{}: {} {detail}", task.name, task.status.label())
                })
                .collect::<Vec<_>>();
            if failures.is_empty() {
                "No failing tasks.".into()
            } else {
                failures.join("\n")
            }
        }
    }
}

fn is_error_line(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    line.contains("error")
        || line.contains("failed")
        || line.contains("panic")
        || line.contains("exception")
        || line.contains("refused")
}

fn selected_task_name(state: &SessionState, selected: usize) -> Option<String> {
    state.tasks.keys().nth(selected).cloned()
}

fn status_marker(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Ready | TaskStatus::Running => "*",
        TaskStatus::Failed | TaskStatus::CrashLoop => "x",
        TaskStatus::Idle | TaskStatus::Waiting | TaskStatus::Starting | TaskStatus::Stopped => "o",
    }
}

#[allow(dead_code)]
fn _assert_state_send_sync(_: Arc<Mutex<SessionState>>) {}
