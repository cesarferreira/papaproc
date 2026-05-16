# papaproc

A dependency-aware process runner for local development.

papaproc runs your local stack as a graph instead of a pile of terminals. Databases can become ready before APIs start, APIs can become ready before frontends start, and failures are summarized instead of buried in logs.

## Status

This is an MVP implementation. It uses stdout/stderr pipes, not pseudo-terminals. That keeps process supervision, readiness probes, diagnostics, snapshots, and tests simple for the first version.

## Install

```bash
cargo install --path .
```

## Commands

```bash
papaproc init
papaproc validate
papaproc run
papaproc run backend
papaproc snapshot
```

## Config

papaproc reads `papaproc.yaml` by default.

```yaml
version: 1
project: demo

groups:
  backend:
    tasks: [db, api]
  frontend:
    tasks: [web]

tasks:
  db:
    cmd: docker compose up db
    ready:
      tcp: localhost:5432
      timeout: 30s

  api:
    cmd: cargo run
    cwd: apps/api
    depends_on:
      db: ready
    ready:
      http: http://localhost:8080/health
    restart:
      on_dependency_restart: true

  web:
    cmd: bun dev
    cwd: apps/web
    depends_on:
      api: ready
    ready:
      http: http://localhost:5173

  tests:
    cmd: bun test --watch
    cwd: apps/web
    mode: manual
```

## Readiness Probes

- `tcp: host:port`
- `http: http://host:port/path`
- `command: ./script-that-exits-zero-when-ready`
- `log_contains: Ready on port`

Readiness supports `timeout` and `interval`, for example `30s` and `500ms`.

## TUI Keys

- `j` / `Down`: select next task
- `k` / `Up`: select previous task
- `Enter`: start selected task
- `x`: stop selected task
- `r`: restart selected task
- `R`: restart selected task and configured dependants
- `s`: render a snapshot into the event panel
- `q` / `Esc`: quit and stop children

## Diagnostics

Built-in diagnostics detect common log patterns:

- `EADDRINUSE` or `address already in use`: port conflict
- `connection refused`: dependency not ready
- `Cannot find module` or `Module not found`: missing dependency
- `database system is starting up`: database still starting

You can add root-level or task-level rules:

```yaml
diagnostics:
  - match: "connection refused"
    title: "Dependency not ready"
    suggest: "Check readiness probe or dependency order."
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
