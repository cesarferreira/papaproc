use papaproc::snapshot::render_snapshot;
use papaproc::state::{SessionState, TaskStatus};

#[test]
fn snapshot_includes_status_failure_diagnosis_and_logs() {
    let mut state = SessionState::new("demo", vec!["api".to_string()]);
    let task = state.tasks.get_mut("api").unwrap();
    task.status = TaskStatus::Failed;
    task.last_exit = Some("exit status: 1".into());
    task.push_log("connection refused localhost:5432");

    let snapshot = render_snapshot(&state);

    assert!(snapshot.contains("Project: demo"));
    assert!(snapshot.contains("api: failed"));
    assert!(snapshot.contains("exit status: 1"));
    assert!(snapshot.contains("connection refused localhost:5432"));
}
