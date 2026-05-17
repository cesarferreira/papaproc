use crate::diagnostics::DiagnosticRule;
use crate::graph::TaskGraph;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadConfig {
    pub version: u32,
    pub project: Option<String>,
    pub groups: BTreeMap<String, GroupConfig>,
    pub tasks: BTreeMap<String, TaskConfig>,
    pub diagnostics: Vec<DiagnosticRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupConfig {
    pub tasks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskConfig {
    pub cmd: String,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub mode: Mode,
    pub group: Option<String>,
    pub depends_on: BTreeMap<String, DependencyCondition>,
    pub ready: ReadyConfig,
    pub restart: RestartConfig,
    pub diagnostics: Vec<DiagnosticRule>,
    pub open: OpenTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Manual,
    Once,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyCondition {
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyConfig {
    pub probes: Vec<ReadyProbe>,
    pub timeout: Duration,
    pub interval: Duration,
}

impl Default for ReadyConfig {
    fn default() -> Self {
        Self {
            probes: Vec::new(),
            timeout: Duration::from_secs(30),
            interval: Duration::from_millis(500),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadyProbe {
    Tcp(String),
    Http(String),
    Command(String),
    LogContains(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartConfig {
    pub on_dependency_restart: bool,
    pub attempts: usize,
    pub window: Duration,
}

impl Default for RestartConfig {
    fn default() -> Self {
        Self {
            on_dependency_restart: false,
            attempts: 3,
            window: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenTarget {
    None,
    Browser(String),
}

impl LoadConfig {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let yaml = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_yaml_with_base(&yaml, Some(base_dir))
    }

    pub fn from_yaml(yaml: &str) -> Result<Self> {
        Self::from_yaml_with_base(yaml, None)
    }

    fn from_yaml_with_base(yaml: &str, base_dir: Option<&Path>) -> Result<Self> {
        let raw: RawConfig = serde_yaml::from_str(yaml).context("failed to parse YAML config")?;
        let config = raw.into_config(base_dir)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("unsupported config version {}; expected 1", self.version);
        }

        if self.tasks.is_empty() {
            bail!("config must define at least one task");
        }

        for (name, task) in &self.tasks {
            if task.cmd.trim().is_empty() {
                bail!("task '{name}' must define a non-empty cmd");
            }

            for dependency in task.depends_on.keys() {
                if !self.tasks.contains_key(dependency) {
                    bail!("task '{name}' depends on unknown task '{dependency}'");
                }
            }
        }

        for (name, group) in &self.groups {
            let mut seen = BTreeSet::new();
            for task in &group.tasks {
                if !self.tasks.contains_key(task) {
                    bail!("group '{name}' references unknown task '{task}'");
                }
                if !seen.insert(task) {
                    bail!("group '{name}' references task '{task}' more than once");
                }
            }
        }

        TaskGraph::new(self)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    version: u32,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    groups: BTreeMap<String, RawGroup>,
    tasks: BTreeMap<String, RawTask>,
    #[serde(default)]
    diagnostics: Vec<RawDiagnosticRule>,
}

impl RawConfig {
    fn into_config(self, base_dir: Option<&Path>) -> Result<LoadConfig> {
        let groups = self
            .groups
            .into_iter()
            .map(|(name, group)| (name, GroupConfig { tasks: group.tasks }))
            .collect();

        let mut tasks = BTreeMap::new();
        for (name, raw) in self.tasks {
            tasks.insert(name, raw.into_task(base_dir)?);
        }

        Ok(LoadConfig {
            version: self.version,
            project: self.project,
            groups,
            tasks,
            diagnostics: self
                .diagnostics
                .into_iter()
                .map(RawDiagnosticRule::into_rule)
                .collect(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct RawGroup {
    #[serde(default)]
    tasks: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawTask {
    cmd: String,
    #[serde(default)]
    cwd: Option<PathBuf>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    depends_on: BTreeMap<String, String>,
    #[serde(default)]
    ready: RawReady,
    #[serde(default)]
    restart: RawRestart,
    #[serde(default)]
    diagnostics: Vec<RawDiagnosticRule>,
    #[serde(default)]
    open: Option<RawOpen>,
}

impl RawTask {
    fn into_task(self, base_dir: Option<&Path>) -> Result<TaskConfig> {
        let mode = match self.mode.as_deref().unwrap_or("auto") {
            "auto" => Mode::Auto,
            "manual" => Mode::Manual,
            "once" => Mode::Once,
            other => bail!("invalid mode '{other}'; expected auto, manual, or once"),
        };

        let mut depends_on = BTreeMap::new();
        for (task, condition) in self.depends_on {
            match condition.as_str() {
                "ready" => {
                    depends_on.insert(task, DependencyCondition::Ready);
                }
                other => bail!("invalid dependency condition '{other}'; expected ready"),
            }
        }

        Ok(TaskConfig {
            cmd: self.cmd,
            cwd: self.cwd.map(|cwd| match base_dir {
                Some(base_dir) if cwd.is_relative() => base_dir.join(cwd),
                _ => cwd,
            }),
            env: self.env,
            mode,
            group: self.group,
            depends_on,
            ready: self.ready.into_ready()?,
            restart: self.restart.into_restart()?,
            diagnostics: self
                .diagnostics
                .into_iter()
                .map(RawDiagnosticRule::into_rule)
                .collect(),
            open: self.open.map_or(OpenTarget::None, RawOpen::into_open),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawReady {
    #[serde(default)]
    tcp: Option<String>,
    #[serde(default)]
    http: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    log_contains: Option<String>,
    #[serde(default)]
    timeout: Option<String>,
    #[serde(default)]
    interval: Option<String>,
}

impl RawReady {
    fn into_ready(self) -> Result<ReadyConfig> {
        let mut ready = ReadyConfig::default();
        if let Some(tcp) = self.tcp {
            ready.probes.push(ReadyProbe::Tcp(tcp));
        }
        if let Some(http) = self.http {
            ready.probes.push(ReadyProbe::Http(http));
        }
        if let Some(command) = self.command {
            ready.probes.push(ReadyProbe::Command(command));
        }
        if let Some(log_contains) = self.log_contains {
            ready.probes.push(ReadyProbe::LogContains(log_contains));
        }
        if let Some(timeout) = self.timeout {
            ready.timeout = parse_duration(&timeout)?;
        }
        if let Some(interval) = self.interval {
            ready.interval = parse_duration(&interval)?;
        }
        Ok(ready)
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawRestart {
    #[serde(default)]
    on_dependency_restart: bool,
    #[serde(default)]
    attempts: Option<usize>,
    #[serde(default)]
    window: Option<String>,
}

impl RawRestart {
    fn into_restart(self) -> Result<RestartConfig> {
        let mut restart = RestartConfig {
            on_dependency_restart: self.on_dependency_restart,
            ..RestartConfig::default()
        };
        if let Some(attempts) = self.attempts {
            restart.attempts = attempts;
        }
        if let Some(window) = self.window {
            restart.window = parse_duration(&window)?;
        }
        Ok(restart)
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawOpen {
    Bool(bool),
    Url(String),
}

impl RawOpen {
    fn into_open(self) -> OpenTarget {
        match self {
            RawOpen::Bool(false) => OpenTarget::None,
            RawOpen::Bool(true) => OpenTarget::Browser(String::new()),
            RawOpen::Url(url) => OpenTarget::Browser(url),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawDiagnosticRule {
    #[serde(rename = "match")]
    pattern: String,
    title: String,
    suggest: String,
}

impl RawDiagnosticRule {
    fn into_rule(self) -> DiagnosticRule {
        DiagnosticRule {
            pattern: self.pattern,
            title: self.title,
            suggest: self.suggest,
        }
    }
}

fn parse_duration(input: &str) -> Result<Duration> {
    let input = input.trim();
    if let Some(ms) = input.strip_suffix("ms") {
        return Ok(Duration::from_millis(ms.parse()?));
    }
    if let Some(seconds) = input.strip_suffix('s') {
        return Ok(Duration::from_secs(seconds.parse()?));
    }
    if let Some(minutes) = input.strip_suffix('m') {
        return Ok(Duration::from_secs(minutes.parse::<u64>()? * 60));
    }
    bail!("invalid duration '{input}'; expected suffix ms, s, or m")
}
