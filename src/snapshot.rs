use crate::state::SessionState;
use chrono::Local;

pub fn render_snapshot(state: &SessionState) -> String {
    let mut output = String::new();
    output.push_str(&format!("Project: {}\n", state.project));
    output.push_str(&format!(
        "Time: {}\n\n",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    ));

    for task in state.tasks.values() {
        let detail = task
            .detail
            .as_ref()
            .map(|value| format!(" ({value})"))
            .unwrap_or_default();
        output.push_str(&format!(
            "{}: {}{}\n",
            task.name,
            task.status.label(),
            detail
        ));
    }

    let failures: Vec<_> = state
        .tasks
        .values()
        .filter(|task| task.diagnosis.is_some() || task.last_exit.is_some())
        .collect();
    if !failures.is_empty() {
        output.push_str("\nFailures:\n");
        for task in failures {
            output.push_str(&format!("\n{} failure:\n", task.name));
            if let Some(exit) = &task.last_exit {
                output.push_str(&format!("exit: {exit}\n"));
            }
            if let Some(diagnosis) = &task.diagnosis {
                output.push_str(&format!("likely cause: {}\n", diagnosis.title));
                output.push_str(&format!("suggested action: {}\n", diagnosis.suggest));
                output.push_str("evidence:\n");
                for line in &diagnosis.evidence {
                    output.push_str(&format!("- {line}\n"));
                }
            }
            output.push_str("last relevant log lines:\n");
            for line in task
                .logs
                .iter()
                .rev()
                .take(40)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                output.push_str(line);
                output.push('\n');
            }
        }
    }

    if let Some(event) = &state.last_event {
        output.push_str(&format!("\nLast event: {event}\n"));
    }

    output
}
