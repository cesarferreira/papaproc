use crate::diagnostics::Diagnosis;
use chrono::{DateTime, Local};
use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

pub const MAX_LOG_LINES: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Idle,
    Waiting,
    Starting,
    Running,
    Ready,
    Failed,
    Stopped,
    CrashLoop,
}

impl TaskStatus {
    pub fn label(self) -> &'static str {
        match self {
            TaskStatus::Idle => "idle",
            TaskStatus::Waiting => "waiting",
            TaskStatus::Starting => "starting",
            TaskStatus::Running => "running",
            TaskStatus::Ready => "ready",
            TaskStatus::Failed => "failed",
            TaskStatus::Stopped => "stopped",
            TaskStatus::CrashLoop => "crash_loop",
        }
    }

    pub fn is_healthy(self) -> bool {
        matches!(self, TaskStatus::Running | TaskStatus::Ready)
    }
}

#[derive(Debug, Clone)]
pub struct TaskState {
    pub name: String,
    pub status: TaskStatus,
    pub detail: Option<String>,
    pub started_at: Option<DateTime<Local>>,
    pub last_exit: Option<String>,
    pub logs: VecDeque<String>,
    pub diagnosis: Option<Diagnosis>,
    pub recent_failures: VecDeque<Instant>,
}

impl TaskState {
    pub fn new(name: impl Into<String>, status: TaskStatus) -> Self {
        Self {
            name: name.into(),
            status,
            detail: None,
            started_at: None,
            last_exit: None,
            logs: VecDeque::new(),
            diagnosis: None,
            recent_failures: VecDeque::new(),
        }
    }

    pub fn push_log(&mut self, line: impl Into<String>) {
        if self.logs.len() >= MAX_LOG_LINES {
            self.logs.pop_front();
        }
        self.logs.push_back(line.into());
    }

    pub fn log_snapshot(&self) -> Vec<String> {
        self.logs.iter().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub project: String,
    pub tasks: BTreeMap<String, TaskState>,
    pub last_event: Option<String>,
}

impl SessionState {
    pub fn new(project: impl Into<String>, tasks: impl IntoIterator<Item = String>) -> Self {
        Self {
            project: project.into(),
            tasks: tasks
                .into_iter()
                .map(|name| {
                    let state = TaskState::new(name.clone(), TaskStatus::Idle);
                    (name, state)
                })
                .collect(),
            last_event: None,
        }
    }

    pub fn task_names(&self) -> Vec<String> {
        self.tasks.keys().cloned().collect()
    }
}
