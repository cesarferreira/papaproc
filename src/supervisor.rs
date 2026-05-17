use crate::config::{LoadConfig, Mode, TaskConfig};
use crate::diagnostics::DiagnosticEngine;
use crate::graph::TaskGraph;
use crate::readiness::{declared_ports, port_is_open, wait_for_ready};
use crate::state::{SessionState, TaskStatus};
use anyhow::{Context, Result, bail};
use chrono::Local;
use std::collections::{BTreeMap, BTreeSet};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};

#[derive(Clone)]
pub struct Supervisor {
    config: LoadConfig,
    graph: TaskGraph,
    state: Arc<Mutex<SessionState>>,
    children: Arc<Mutex<BTreeMap<String, Arc<Mutex<Child>>>>>,
    diagnostics: Arc<DiagnosticEngine>,
}

impl Supervisor {
    pub fn new(config: LoadConfig) -> Result<Self> {
        let graph = TaskGraph::new(&config)?;
        let project = config.project.clone().unwrap_or_else(|| "papaproc".into());
        let state = SessionState::new(project, config.tasks.keys().cloned());
        let mut rules = config.diagnostics.clone();
        for task in config.tasks.values() {
            rules.extend(task.diagnostics.clone());
        }
        Ok(Self {
            config,
            graph,
            state: Arc::new(Mutex::new(state)),
            children: Arc::new(Mutex::new(BTreeMap::new())),
            diagnostics: Arc::new(DiagnosticEngine::with_builtin_rules(rules)?),
        })
    }

    pub fn state(&self) -> Arc<Mutex<SessionState>> {
        self.state.clone()
    }

    pub fn graph_text(&self) -> String {
        self.graph.render()
    }

    pub async fn start_selected(&self, selectors: &[String]) -> Result<()> {
        let selected = if selectors.is_empty() {
            self.config
                .tasks
                .iter()
                .filter(|(_, task)| matches!(task.mode, Mode::Auto | Mode::Once))
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>()
        } else {
            self.graph.expand_selectors(selectors)?
        };
        let order = self.graph.start_order(&selected)?;
        let selected_set: BTreeSet<_> = selected.into_iter().collect();

        for name in order {
            let task = self
                .config
                .tasks
                .get(&name)
                .context("task missing after graph build")?;
            if task.mode == Mode::Manual && !selected_set.contains(&name) {
                continue;
            }
            self.wait_for_dependencies(&name).await?;
            self.start_task(&name).await?;
        }
        Ok(())
    }

    pub async fn start_task(&self, name: &str) -> Result<()> {
        let task = self
            .config
            .tasks
            .get(name)
            .with_context(|| format!("unknown task '{name}'"))?
            .clone();

        if self.children.lock().await.contains_key(name) {
            self.stop_task(name).await?;
        }
        self.preflight_ports(name, &task).await?;

        self.set_status(name, TaskStatus::Starting, None).await;
        let mut child = spawn_child(&task).await?;

        if let Some(stdout) = child.stdout.take() {
            self.spawn_log_reader(name.to_string(), stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            self.spawn_log_reader(name.to_string(), stderr);
        }

        let child = Arc::new(Mutex::new(child));
        self.children
            .lock()
            .await
            .insert(name.to_string(), child.clone());
        self.clone().spawn_exit_monitor(name.to_string(), child);

        let state = self.state.clone();
        let task_name = name.to_string();
        let task_for_ready = task.clone();
        match wait_for_ready(&task, move || {
            state
                .try_lock()
                .ok()
                .and_then(|state| state.tasks.get(&task_name).map(|task| task.log_snapshot()))
                .unwrap_or_default()
        })
        .await
        {
            Ok(()) => {
                let status = if task_for_ready.ready.probes.is_empty() {
                    TaskStatus::Running
                } else {
                    TaskStatus::Ready
                };
                self.set_status(name, status, None).await;
            }
            Err(error) => {
                self.mark_failure(name, format!("readiness failed: {error}"))
                    .await;
                bail!("task '{name}' failed readiness: {error}");
            }
        }

        Ok(())
    }

    pub async fn stop_task(&self, name: &str) -> Result<()> {
        if let Some(child) = self.children.lock().await.remove(name) {
            self.set_status(name, TaskStatus::Stopped, None).await;
            let mut child = child.lock().await;
            terminate_child(&mut child).await;
        } else {
            self.set_status(name, TaskStatus::Stopped, None).await;
        }
        Ok(())
    }

    pub async fn restart_task(&self, name: &str, with_dependants: bool) -> Result<()> {
        self.stop_task(name).await?;
        self.start_task(name).await?;
        if with_dependants {
            for dependant in self.graph.downstream_order(name)? {
                let restart = self
                    .config
                    .tasks
                    .get(&dependant)
                    .is_some_and(|task| task.restart.on_dependency_restart);
                if restart {
                    self.stop_task(&dependant).await?;
                    self.wait_for_dependencies(&dependant).await?;
                    self.start_task(&dependant).await?;
                }
            }
        }
        self.state.lock().await.last_event = Some(format!("restarted {name}"));
        Ok(())
    }

    async fn restart_dependants(&self, name: &str) -> Result<()> {
        for dependant in self.graph.downstream_order(name)? {
            let restart = self
                .config
                .tasks
                .get(&dependant)
                .is_some_and(|task| task.restart.on_dependency_restart);
            if restart {
                self.stop_task(&dependant).await?;
                self.wait_for_dependencies(&dependant).await?;
                self.start_task(&dependant).await?;
            }
        }
        Ok(())
    }

    pub async fn stop_all(&self) {
        let names: Vec<String> = self.children.lock().await.keys().cloned().collect();
        for name in names {
            let _ = self.stop_task(&name).await;
        }
    }

    async fn wait_for_dependencies(&self, name: &str) -> Result<()> {
        for dependency in self.graph.dependencies_of(name)? {
            loop {
                let ready = self
                    .state
                    .lock()
                    .await
                    .tasks
                    .get(&dependency)
                    .is_some_and(|state| state.status.is_healthy());
                if ready {
                    break;
                }
                self.set_status(
                    name,
                    TaskStatus::Waiting,
                    Some(format!("waiting on {dependency}")),
                )
                .await;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
        Ok(())
    }

    async fn preflight_ports(&self, name: &str, task: &TaskConfig) -> Result<()> {
        for (host, port) in declared_ports(task) {
            if port_is_open(&host, port).await {
                self.mark_failure(name, format!("port {host}:{port} is already in use"))
                    .await;
                bail!("port {host}:{port} is already in use");
            }
        }
        Ok(())
    }

    fn spawn_log_reader<R>(&self, name: String, stream: R)
    where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
    {
        let state = self.state.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stream).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut state = state.lock().await;
                if let Some(task) = state.tasks.get_mut(&name) {
                    task.push_log(line);
                }
            }
        });
    }

    fn spawn_exit_monitor(self, name: String, child: Arc<Mutex<Child>>) {
        tokio::spawn(async move {
            let status = loop {
                let status = {
                    let mut child = child.lock().await;
                    child.try_wait()
                };
                match status {
                    Ok(Some(status)) => break Ok(status),
                    Ok(None) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
                    Err(error) => break Err(error),
                }
            };
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            if let Err(error) = self.handle_child_exit(&name, status).await {
                self.state.lock().await.last_event =
                    Some(format!("{name} exit handler failed: {error}"));
            }
        });
    }

    async fn handle_child_exit(
        &self,
        name: &str,
        status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        self.children.lock().await.remove(name);

        let was_stopped = self
            .state
            .lock()
            .await
            .tasks
            .get(name)
            .is_some_and(|task| task.status == TaskStatus::Stopped);
        if was_stopped {
            return Ok(());
        }

        match status {
            Ok(status) if status.success() => {
                let mut state = self.state.lock().await;
                if let Some(task) = state.tasks.get_mut(name)
                    && task.status != TaskStatus::Stopped
                {
                    task.status = TaskStatus::Stopped;
                    task.last_exit = Some(status.to_string());
                }
                state.last_event = Some(format!("{name} exited with {status}"));
            }
            Ok(status) => {
                self.handle_task_failure(name, status.to_string()).await?;
            }
            Err(error) => {
                self.handle_task_failure(name, format!("wait failed: {error}"))
                    .await?;
            }
        }
        Ok(())
    }

    async fn handle_task_failure(&self, name: &str, reason: String) -> Result<()> {
        let status = self.mark_failure(name, reason).await;
        let Some(task) = self.config.tasks.get(name) else {
            return Ok(());
        };
        if task.mode != Mode::Auto || status == TaskStatus::CrashLoop {
            return Ok(());
        }

        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        self.start_task(name).await?;
        self.restart_dependants(name).await?;
        self.state.lock().await.last_event =
            Some(format!("self-healed {name} and restarted dependants"));
        Ok(())
    }

    async fn set_status(&self, name: &str, status: TaskStatus, detail: Option<String>) {
        let mut state = self.state.lock().await;
        if let Some(task) = state.tasks.get_mut(name) {
            task.status = status;
            task.detail = detail;
            if matches!(
                status,
                TaskStatus::Starting | TaskStatus::Running | TaskStatus::Ready
            ) {
                task.started_at.get_or_insert_with(Local::now);
            }
        }
        state.last_event = Some(format!("{name} is {}", status.label()));
    }

    async fn mark_failure(&self, name: &str, reason: String) -> TaskStatus {
        let mut state = self.state.lock().await;
        let now = Instant::now();
        let mut status = TaskStatus::Failed;
        if let Some(task) = state.tasks.get_mut(name) {
            task.last_exit = Some(reason.clone());
            task.recent_failures.push_back(now);
            if let Some(config) = self.config.tasks.get(name) {
                while task
                    .recent_failures
                    .front()
                    .is_some_and(|first| now.duration_since(*first) > config.restart.window)
                {
                    task.recent_failures.pop_front();
                }
                status = if task.recent_failures.len() >= config.restart.attempts {
                    TaskStatus::CrashLoop
                } else {
                    TaskStatus::Failed
                };
                task.status = status;
            } else {
                task.status = TaskStatus::Failed;
            }
            let logs = task.log_snapshot();
            task.diagnosis = self.diagnostics.diagnose(name, &logs);
        }
        state.last_event = Some(format!("{name} failed: {reason}"));
        status
    }
}

async fn spawn_child(task: &TaskConfig) -> Result<Child> {
    let parts = shell_words::split(&task.cmd)?;
    let (program, args) = parts.split_first().context("task cmd cannot be empty")?;
    let mut command = Command::new(program);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = &task.cwd {
        command.current_dir(cwd);
    }
    command.envs(&task.env);
    #[cfg(unix)]
    command.process_group(0);
    command
        .spawn()
        .with_context(|| format!("failed to spawn '{}'; command: {}", program, task.cmd))
}

async fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            signal_process_group(pid, libc::SIGTERM);
            if timeout(Duration::from_secs(2), child.wait()).await.is_ok() {
                return;
            }
            signal_process_group(pid, libc::SIGKILL);
            let _ = child.wait().await;
            return;
        }
    }

    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[cfg(unix)]
fn signal_process_group(pid: u32, signal: i32) {
    unsafe {
        libc::kill(-(pid as i32), signal);
    }
}
