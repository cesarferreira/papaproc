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
