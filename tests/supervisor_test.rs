use papaproc::config::LoadConfig;
use papaproc::state::{SessionState, TaskStatus};
use papaproc::supervisor::Supervisor;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant, sleep};

#[tokio::test]
async fn marks_task_failed_when_child_exits_unexpectedly() {
    let config = LoadConfig::from_yaml(
        r#"
version: 1
tasks:
  api:
    cmd: sh -c 'echo connection refused; exit 42'
    mode: once
    restart:
      attempts: 3
"#,
    )
    .unwrap();
    let supervisor = Supervisor::new(config).unwrap();

    supervisor.start_task("api").await.unwrap();
    wait_for_status(supervisor.state(), "api", TaskStatus::Failed).await;

    let state = supervisor.state().lock().await.clone();
    let task = state.tasks.get("api").unwrap();
    assert_eq!(task.status, TaskStatus::Failed);
    assert!(task.last_exit.as_ref().unwrap().contains("exit status"));
    assert_eq!(
        task.diagnosis.as_ref().unwrap().title,
        "Dependency not ready"
    );
}

#[tokio::test]
async fn marks_crash_loop_when_failure_threshold_is_reached() {
    let config = LoadConfig::from_yaml(
        r#"
version: 1
tasks:
  api:
    cmd: sh -c 'echo boom; exit 1'
    restart:
      attempts: 1
      window: 10s
"#,
    )
    .unwrap();
    let supervisor = Supervisor::new(config).unwrap();

    supervisor.start_task("api").await.unwrap();
    wait_for_status(supervisor.state(), "api", TaskStatus::CrashLoop).await;

    let state = supervisor.state().lock().await.clone();
    assert_eq!(
        state.tasks.get("api").unwrap().status,
        TaskStatus::CrashLoop
    );
}

#[tokio::test]
async fn automatically_restarts_failed_auto_tasks_before_crash_loop_threshold() {
    let temp = tempfile::tempdir().unwrap();
    let flag = temp.path().join("failed-once");
    let flag = flag.display();
    let config = LoadConfig::from_yaml(&format!(
        r#"
version: 1
tasks:
  api:
    cmd: sh -c 'if [ -f "{flag}" ]; then echo recovered; sleep 5; else touch "{flag}"; echo connection refused; exit 1; fi'
    restart:
      attempts: 3
      window: 10s
"#
    ))
    .unwrap();
    let supervisor = Supervisor::new(config).unwrap();

    supervisor.start_task("api").await.unwrap();
    wait_for_log(supervisor.state(), "api", "recovered").await;

    let state = supervisor.state().lock().await.clone();
    let task = state.tasks.get("api").unwrap();
    assert_eq!(task.status, TaskStatus::Running);
    assert_eq!(task.recent_failures.len(), 1);
}

#[tokio::test]
async fn restarts_configured_dependants_after_dependency_auto_restart() {
    let temp = tempfile::tempdir().unwrap();
    let flag = temp.path().join("db-failed-once");
    let starts = temp.path().join("api-starts");
    let flag = flag.display();
    let starts = starts.display();
    let config = LoadConfig::from_yaml(&format!(
        r#"
version: 1
tasks:
  db:
    cmd: sh -c 'if [ -f "{flag}" ]; then echo db recovered; sleep 5; else touch "{flag}"; echo db crashed; exit 1; fi'
    restart:
      attempts: 3
      window: 10s
  api:
    cmd: sh -c 'echo api start >> "{starts}"; sleep 5'
    depends_on:
      db: ready
    restart:
      on_dependency_restart: true
"#
    ))
    .unwrap();
    let supervisor = Supervisor::new(config).unwrap();

    supervisor.start_selected(&[]).await.unwrap();
    wait_for_log(supervisor.state(), "db", "db recovered").await;
    wait_for_file_lines(starts.to_string(), 2).await;

    let starts = std::fs::read_to_string(starts.to_string()).unwrap();
    assert_eq!(starts.lines().count(), 2);
}

#[tokio::test]
async fn stop_task_kills_shell_descendants() {
    let temp = tempfile::tempdir().unwrap();
    let pid_file = temp.path().join("grandchild.pid");
    let pid_file_display = pid_file.display();
    let config = LoadConfig::from_yaml(&format!(
        r#"
version: 1
tasks:
  worker:
    cmd: sh -c 'sleep 30 & echo $! > "{pid_file_display}"; wait'
    mode: once
"#
    ))
    .unwrap();
    let supervisor = Supervisor::new(config).unwrap();

    supervisor.start_task("worker").await.unwrap();
    wait_for_file(&pid_file).await;
    let grandchild_pid = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();

    supervisor.stop_task("worker").await.unwrap();
    wait_for_process_to_exit(grandchild_pid).await;

    assert!(!process_exists(grandchild_pid));
}

async fn wait_for_status(state: Arc<Mutex<SessionState>>, task: &str, status: TaskStatus) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let current = state.lock().await.tasks.get(task).unwrap().status;
        if current == status {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {task} to become {status:?}; current status is {current:?}"
        );
        sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_log(state: Arc<Mutex<SessionState>>, task: &str, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let has_log = state
            .lock()
            .await
            .tasks
            .get(task)
            .unwrap()
            .logs
            .iter()
            .any(|line| line.contains(needle));
        if has_log {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {task} log containing {needle}"
        );
        sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_file_lines(path: String, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let lines = std::fs::read_to_string(&path)
            .map(|content| content.lines().count())
            .unwrap_or_default();
        if lines >= count {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {path} to have {count} lines; got {lines}"
        );
        sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_file(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if path.exists() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {} to exist",
            path.display()
        );
        sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_process_to_exit(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if !process_exists(pid) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for process {pid} to exit"
        );
        sleep(Duration::from_millis(25)).await;
    }
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn process_exists(_pid: u32) -> bool {
    false
}
