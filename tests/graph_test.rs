use papaproc::config::LoadConfig;
use papaproc::graph::TaskGraph;

#[test]
fn orders_dependencies_before_dependants() {
    let config = LoadConfig::from_yaml(
        r#"
version: 1
tasks:
  db:
    cmd: db
  api:
    cmd: api
    depends_on:
      db: ready
  web:
    cmd: web
    depends_on:
      api: ready
  worker:
    cmd: worker
    depends_on:
      db: ready
"#,
    )
    .unwrap();

    let graph = TaskGraph::new(&config).unwrap();
    let order = graph
        .start_order(&["web".to_string(), "worker".to_string()])
        .unwrap();

    assert!(position(&order, "db") < position(&order, "api"));
    assert!(position(&order, "api") < position(&order, "web"));
    assert!(position(&order, "db") < position(&order, "worker"));
}

#[test]
fn expands_group_selectors() {
    let config = LoadConfig::from_yaml(
        r#"
version: 1
groups:
  backend:
    tasks: [db, api]
tasks:
  db:
    cmd: db
  api:
    cmd: api
    depends_on:
      db: ready
  web:
    cmd: web
"#,
    )
    .unwrap();

    let graph = TaskGraph::new(&config).unwrap();
    let selected = graph.expand_selectors(&["backend".to_string()]).unwrap();

    assert_eq!(selected, vec!["db".to_string(), "api".to_string()]);
}

#[test]
fn downstream_restart_order_is_topological() {
    let config = LoadConfig::from_yaml(
        r#"
version: 1
tasks:
  db:
    cmd: db
  api:
    cmd: api
    depends_on:
      db: ready
  web:
    cmd: web
    depends_on:
      api: ready
  worker:
    cmd: worker
    depends_on:
      db: ready
"#,
    )
    .unwrap();

    let graph = TaskGraph::new(&config).unwrap();
    let order = graph.downstream_order("db").unwrap();

    assert!(position(&order, "api") < position(&order, "web"));
    assert!(order.contains(&"worker".to_string()));
}

#[test]
fn renders_dependency_graph_edges() {
    let config = LoadConfig::from_yaml(
        r#"
version: 1
tasks:
  db:
    cmd: db
  api:
    cmd: api
    depends_on:
      db: ready
  web:
    cmd: web
    depends_on:
      api: ready
"#,
    )
    .unwrap();

    let graph = TaskGraph::new(&config).unwrap();
    let rendered = graph.render();

    assert!(rendered.contains("db"));
    assert!(rendered.contains("db -> api"));
    assert!(rendered.contains("api -> web"));
}

fn position(order: &[String], task: &str) -> usize {
    order.iter().position(|name| name == task).unwrap()
}
