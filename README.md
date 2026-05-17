<div align="center">
  <h1>papaproc</h1>

  <p><strong>A dependency-aware process runner for local development.</strong></p>

  <p>
    <a href="https://github.com/cesarferreira/papaproc/actions/workflows/rust-tests.yml"><img alt="CI" src="https://github.com/cesarferreira/papaproc/actions/workflows/rust-tests.yml/badge.svg"></a>
    <img alt="License" src="https://img.shields.io/badge/license-MIT-green">
    <img alt="Rust" src="https://img.shields.io/badge/rust-2024-orange">
  </p>

  <p>
    <a href="#install">Install</a>
    &nbsp;·&nbsp;
    <a href="#quickstart">Quickstart</a>
    &nbsp;·&nbsp;
    <a href="#config">Config</a>
    &nbsp;·&nbsp;
    <a href="#commands">Commands</a>
  </p>
</div>

---

## Why papaproc

Most process runners answer one question: “are my commands running?”

**papaproc** answers the question local development actually needs: “is my stack healthy?”

Run your local environment as a graph instead of a pile of terminals. Databases become ready before APIs start, APIs become ready before frontends start, and common failures are explained close to the logs that caused them.

- **Dependency-aware startup.** `db -> api -> web` starts in order and waits for real readiness, not arbitrary sleeps.
- **Self-healing sessions.** Failed auto tasks restart until a crash-loop threshold is reached; configured dependants restart when an upstream task recovers.
- **Readiness probes built in.** TCP, HTTP, command, and log-based probes are supported in `papaproc.yaml`.
- **Failure summaries.** Built-in diagnostics catch port conflicts, dependency readiness issues, missing packages, and common database startup messages.
- **Mission-control TUI.** Run `papaproc run` to see task health, logs, graph, failures, and restart controls in one screen.
- **Pasteable snapshots.** `papaproc snapshot` produces a compact report for issues, Slack, or an AI coding agent.

papaproc is for monorepos, web apps with backends, Docker-backed services, workers, queues, mobile projects with local APIs, and agent-driven development sessions.

<a id="install"></a>
## Install

Build from source:

```bash
cargo install --git https://github.com/cesarferreira/papaproc --locked
```

Or from a local checkout:

```bash
cargo install --path . --locked
```

Verify the install:

```bash
papaproc --version
```

<a id="quickstart"></a>
## Quickstart

Create a config:

```bash
papaproc init
```

Edit `papaproc.yaml` for your stack:

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

Validate it:

```bash
papaproc validate
```

Start the whole stack:

```bash
papaproc run
```

Start a group:

```bash
papaproc run backend
```

Generate a report:

```bash
papaproc snapshot
```

## Highlights

### Graph-first dev sessions

```text
db -> api
api -> web
api -> tests
```

`depends_on` turns your commands into a dependency graph. papaproc rejects cycles, starts dependencies first, and keeps dependants waiting until upstream tasks are healthy.

### Readiness over sleeps

```yaml
ready:
  tcp: localhost:5432
  timeout: 30s
```

Supported probes:

- `tcp: host:port`
- `http: http://host:port/path`
- `command: ./script-that-exits-zero-when-ready`
- `log_contains: Ready on port`

Readiness supports `timeout` and `interval`, for example `30s`, `10s`, or `500ms`.

### Self-healing restarts

```yaml
restart:
  attempts: 3
  window: 30s
  on_dependency_restart: true
```

Auto tasks restart after unexpected failure until they hit the crash-loop threshold. When an upstream task recovers, downstream tasks with `on_dependency_restart: true` restart in dependency order.

### Failure summaries

Built-in diagnostics detect common log patterns:

- `EADDRINUSE` or `address already in use`: port conflict
- `connection refused`: dependency not ready
- `Cannot find module` or `Module not found`: missing dependency
- `database system is starting up`: database still starting

Add root-level or task-level rules:

```yaml
diagnostics:
  - match: "connection refused"
    title: "Dependency not ready"
    suggest: "Check readiness probe or dependency order."
```

### TUI controls

Bare `papaproc run` launches the mission-control dashboard.

| Key | Action |
|---|---|
| `j` / `Down` | Select next task |
| `k` / `Up` | Select previous task |
| `Enter` | Start selected task |
| `x` | Stop selected task |
| `r` | Restart selected task |
| `R` | Restart selected task and configured dependants |
| `e` | Toggle errors-only logs |
| `g` | Show graph panel |
| `f` | Show failures panel |
| `s` | Render snapshot into the event panel |
| `?` | Show help/event panel |
| `q` / `Esc` | Quit and stop children |

<a id="config"></a>
## Config

papaproc reads `papaproc.yaml` by default. Use `--config` to choose another file:

```bash
papaproc --config local.yaml run
```

Root fields:

| Field | Purpose |
|---|---|
| `version` | Config version. Use `1`. |
| `project` | Display name for the session. |
| `groups` | Named task sets for `papaproc run backend`. |
| `tasks` | Task definitions. |
| `diagnostics` | Global regex diagnosis rules. |

Task fields:

| Field | Purpose |
|---|---|
| `cmd` | Command to run. Required. |
| `cwd` | Working directory. Relative paths are resolved from the config file's directory. |
| `env` | Environment variables. |
| `mode` | `auto`, `manual`, or `once`. Defaults to `auto`. |
| `depends_on` | Map of upstream task to `ready`. |
| `ready` | Readiness probes and timing. |
| `restart` | Crash-loop and dependency restart behavior. |
| `diagnostics` | Task-specific diagnosis rules. |

See [`examples/papaproc.yaml`](examples/papaproc.yaml) for a full example.

<a id="commands"></a>
## Commands

| Command | What it does |
|---|---|
| `papaproc init` | Write a sample `papaproc.yaml`. |
| `papaproc validate` | Parse and validate the config. |
| `papaproc run` | Start auto tasks in the TUI. |
| `papaproc run <task-or-group>` | Start a selected task or group plus dependencies. |
| `papaproc snapshot` | Print a pasteable session/config report. |

## Design choices

papaproc v0.1 uses stdout/stderr pipes instead of pseudo-terminals. That makes supervision, readiness, snapshots, and tests deterministic. PTY support can be added later per task for interactive CLIs that need it.

## Roadmap

- PTY support with `pty: true` per task
- Persistent live session snapshots
- Native desktop/browser open support for `open`
- Richer graph visualization
- Optional local/remote AI explanations for failures
- Release binaries and Homebrew tap

## Development

```bash
make check
```

Equivalent commands:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## License

MIT &copy; Cesar Ferreira
