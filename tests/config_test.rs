use papaproc::config::{LoadConfig, Mode, ReadyProbe};

#[test]
fn parses_valid_config_with_defaults() {
    let yaml = r#"
version: 1
project: demo
groups:
  backend:
    tasks: [db, api]
tasks:
  db:
    cmd: docker compose up db
    ready:
      tcp: localhost:5432
  api:
    cmd: cargo run
    depends_on:
      db: ready
    ready:
      http: http://localhost:8080/health
    restart:
      on_dependency_restart: true
  tests:
    cmd: bun test --watch
    mode: manual
"#;

    let config = LoadConfig::from_yaml(yaml).expect("config should parse");

    assert_eq!(config.project.as_deref(), Some("demo"));
    assert_eq!(config.tasks["db"].mode, Mode::Auto);
    assert_eq!(config.tasks["tests"].mode, Mode::Manual);
    assert_eq!(
        config.tasks["db"].ready.probes,
        vec![ReadyProbe::Tcp("localhost:5432".into())]
    );
    assert_eq!(config.tasks["db"].ready.timeout.as_secs(), 30);
    assert!(config.tasks["api"].restart.on_dependency_restart);
}

#[test]
fn rejects_unknown_dependency() {
    let yaml = r#"
version: 1
tasks:
  api:
    cmd: cargo run
    depends_on:
      db: ready
"#;

    let error = LoadConfig::from_yaml(yaml).expect_err("unknown dependency should fail");

    assert!(error.to_string().contains("depends on unknown task 'db'"));
}

#[test]
fn rejects_dependency_cycles() {
    let yaml = r#"
version: 1
tasks:
  api:
    cmd: cargo run
    depends_on:
      web: ready
  web:
    cmd: bun dev
    depends_on:
      api: ready
"#;

    let error = LoadConfig::from_yaml(yaml).expect_err("cycle should fail");

    assert!(error.to_string().contains("dependency cycle"));
}
