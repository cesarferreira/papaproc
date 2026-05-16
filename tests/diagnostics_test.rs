use papaproc::diagnostics::{DiagnosticEngine, DiagnosticRule};

#[test]
fn detects_port_conflict_from_logs() {
    let engine = DiagnosticEngine::with_builtin_rules(Vec::new()).unwrap();
    let logs = vec!["Error: listen EADDRINUSE: address already in use 127.0.0.1:8080".to_string()];

    let diagnosis = engine
        .diagnose("api", &logs)
        .expect("diagnosis should match");

    assert_eq!(diagnosis.title, "Port already in use");
    assert!(diagnosis.evidence[0].contains("EADDRINUSE"));
}

#[test]
fn user_rules_override_builtin_rules() {
    let engine = DiagnosticEngine::with_builtin_rules(vec![DiagnosticRule {
        pattern: "connection refused".into(),
        title: "Database unavailable".into(),
        suggest: "Start postgres".into(),
    }])
    .unwrap();
    let logs = vec!["database connection refused".to_string()];

    let diagnosis = engine
        .diagnose("api", &logs)
        .expect("diagnosis should match");

    assert_eq!(diagnosis.title, "Database unavailable");
    assert_eq!(diagnosis.suggest, "Start postgres");
}
