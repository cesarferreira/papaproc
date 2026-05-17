use papaproc::config::{LoadConfig, Mode, ReadyProbe};
use std::path::Path;

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

#[test]
fn fake_stack_sample_config_is_valid() {
    let yaml = std::fs::read_to_string("examples/fake-stack/papaproc.yaml")
        .expect("fake stack sample config should exist");

    let config = LoadConfig::from_yaml(&yaml).expect("fake stack sample config should parse");

    assert_eq!(config.project.as_deref(), Some("fake-stack"));
    assert!(config.groups.contains_key("demo"));
    assert!(config.tasks.contains_key("fake-db"));
    assert!(config.tasks.contains_key("fake-api"));
    assert!(config.tasks.contains_key("fake-web"));
    assert!(config.tasks.contains_key("noisy-logs"));
}

#[test]
fn top_level_example_config_is_runnable_fake_stack() {
    let yaml = std::fs::read_to_string("examples/papaproc.yaml")
        .expect("top-level example config should exist");

    let config = LoadConfig::from_yaml(&yaml).expect("top-level example config should parse");

    assert_eq!(config.project.as_deref(), Some("fake-stack"));
    assert_eq!(
        config.tasks["fake-db"].cwd.as_deref(),
        Some(Path::new("fake-stack"))
    );
    assert!(config.tasks.contains_key("fake-api"));
    assert!(config.tasks.contains_key("fake-web"));
}

#[test]
fn top_level_example_config_resolves_cwd_from_its_directory() {
    let config = LoadConfig::from_path("examples/papaproc.yaml")
        .expect("top-level example config should parse");

    assert_eq!(
        config.tasks["fake-db"].cwd.as_deref(),
        Some(Path::new("examples/fake-stack"))
    );
}

#[test]
fn from_path_resolves_relative_cwd_from_config_file_directory() {
    let root = std::env::temp_dir().join(format!(
        "papaproc-config-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let config_dir = root.join("examples");
    std::fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("papaproc.yaml");
    std::fs::write(
        &config_path,
        r#"
version: 1
tasks:
  api:
    cwd: fake-stack
    cmd: bash scripts/fake-api.sh
"#,
    )
    .unwrap();

    let config = LoadConfig::from_path(&config_path).expect("config should parse");

    assert_eq!(
        config.tasks["api"].cwd.as_deref(),
        Some(config_dir.join("fake-stack").as_path())
    );

    std::fs::remove_dir_all(root).unwrap();
}
