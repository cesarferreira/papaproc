use crate::snapshot::render_snapshot;
use crate::state::{SessionState, TaskStatus};
use crate::supervisor::Supervisor;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
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
    let page = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(frame.area());
    let root = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(page[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(root[1]);

    let tasks: Vec<_> = state.tasks.values().collect();
    let items = tasks
        .iter()
        .enumerate()
        .map(|(index, task)| {
            let selected_row = index == selected;
            let prefix = if selected_row { ">" } else { " " };
            let marker = status_marker(task.status);
            let detail = task.detail.as_deref().unwrap_or("");
            let mut item = ListItem::new(Line::from(vec![
                Span::styled(prefix.to_string(), Style::default().fg(Color::Magenta)),
                Span::raw(" "),
                Span::styled(marker, status_style(task.status)),
                Span::raw(" "),
                Span::styled(
                    format!("{:<16}", task.name),
                    Style::default()
                        .fg(if selected_row {
                            Color::White
                        } else {
                            Color::Gray
                        })
                        .add_modifier(if selected_row {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    format!("{:<10}", task.status.label()),
                    status_style(task.status),
                ),
                Span::styled(detail.to_string(), Style::default().fg(Color::DarkGray)),
            ]));
            if selected_row {
                item = item.style(Style::default().bg(Color::DarkGray));
            }
            item
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .title(format!("Stack: {}", state.project))
                .title_style(
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                )
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
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
            .style(Style::default().fg(Color::Gray))
            .block(
                Block::default()
                    .title(title)
                    .title_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: false }),
        right[0],
    );

    let diagnosis = detail_text(state, selected_task.copied(), panel, graph);
    frame.render_widget(
        Paragraph::new(diagnosis)
            .style(Style::default().fg(Color::LightYellow))
            .block(
                Block::default()
                    .title("Diagnosis / Last Event")
                    .title_style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: false }),
        right[1],
    );

    frame.render_widget(
        Paragraph::new(shortcut_footer())
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
        page[1],
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

fn shortcut_footer() -> Line<'static> {
    let mut spans = Vec::new();
    push_shortcut(&mut spans, "j/k", "select");
    spans.extend([separator()]);
    push_shortcut(&mut spans, "enter", "start");
    spans.extend([separator()]);
    push_shortcut(&mut spans, "x", "stop");
    spans.extend([separator()]);
    push_shortcut(&mut spans, "r", "restart");
    spans.extend([separator()]);
    push_shortcut(&mut spans, "R", "deps");
    spans.extend([separator()]);
    push_shortcut(&mut spans, "e", "errors");
    spans.extend([separator()]);
    push_shortcut(&mut spans, "g", "graph");
    spans.extend([separator()]);
    push_shortcut(&mut spans, "f", "failures");
    spans.extend([separator()]);
    push_shortcut(&mut spans, "s", "snapshot");
    spans.extend([separator()]);
    push_shortcut(&mut spans, "q", "quit");
    Line::from(spans)
}

fn push_shortcut(spans: &mut Vec<Span<'static>>, key: &'static str, action: &'static str) {
    spans.push(Span::styled(
        key,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        format!(" {action}"),
        Style::default().fg(Color::Gray),
    ));
}

fn separator() -> Span<'static> {
    Span::styled("  |  ", Style::default().fg(Color::DarkGray))
}

fn status_style(status: TaskStatus) -> Style {
    match status {
        TaskStatus::Ready => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        TaskStatus::Running => Style::default().fg(Color::Cyan),
        TaskStatus::Waiting | TaskStatus::Starting => Style::default().fg(Color::Yellow),
        TaskStatus::Failed => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        TaskStatus::CrashLoop => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        TaskStatus::Idle | TaskStatus::Stopped => Style::default().fg(Color::DarkGray),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};

    #[test]
    fn footer_exposes_mprocs_style_shortcuts() {
        let footer = shortcut_footer();
        let text = footer
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("");

        assert!(text.contains("j/k select"));
        assert!(text.contains("enter start"));
        assert!(text.contains("x stop"));
        assert!(text.contains("r restart"));
        assert!(text.contains("g graph"));
        assert!(text.contains("q quit"));
    }

    #[test]
    fn footer_colors_shortcut_keys_separately_from_actions() {
        let footer = shortcut_footer();
        let first_key = &footer.spans[0];
        let first_action = &footer.spans[1];

        assert_eq!(first_key.content.as_ref(), "j/k");
        assert_eq!(first_key.style.fg, Some(Color::Cyan));
        assert!(first_key.style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(first_action.content.as_ref(), " select");
        assert_eq!(first_action.style.fg, Some(Color::Gray));
    }

    #[test]
    fn statuses_have_distinct_colors() {
        assert_eq!(status_style(TaskStatus::Ready).fg, Some(Color::Green));
        assert_eq!(status_style(TaskStatus::Running).fg, Some(Color::Cyan));
        assert_eq!(status_style(TaskStatus::Waiting).fg, Some(Color::Yellow));
        assert_eq!(status_style(TaskStatus::Failed).fg, Some(Color::Red));
        assert!(
            status_style(TaskStatus::CrashLoop)
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }
}
