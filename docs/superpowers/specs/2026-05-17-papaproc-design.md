# papaproc Design

## Goal

Build `papaproc`, a smart process runner for modern local development environments. It should run a stack as a dependency graph, wait for readiness, surface health, and explain common failures.

## Scope

V1 includes a Rust TUI, YAML config, stdout/stderr pipe process supervision, dependency ordering, TCP/HTTP/command/log readiness, restart propagation hooks, crash-loop detection, port conflict detection, diagnostics, and snapshots.

V1 does not include pseudo-terminal support or AI explanations.

## Architecture

`papaproc` is a single Rust binary crate with focused internal modules:

- `cli`: parses commands.
- `config`: loads and validates `papaproc.yaml`.
- `graph`: builds and queries the dependency DAG.
- `state`: stores task/session status and bounded logs.
- `supervisor`: owns process lifecycle and readiness transitions.
- `readiness`: implements TCP, HTTP, command, and log probes.
- `diagnostics`: matches logs against built-in and user-defined rules.
- `snapshot`: renders pasteable session reports.
- `tui`: renders the dashboard and maps keypresses to supervisor actions.

The supervisor publishes state updates. The TUI reads state and triggers supervisor actions, but does not directly manage child processes.

## Behavior

`papaproc run` loads config, validates the graph, expands optional task/group selectors, and starts eligible tasks in topological order. A task waits until dependencies are healthy before it starts.

Task states are `idle`, `waiting`, `starting`, `running`, `ready`, `failed`, `stopped`, and `crash_loop`.

Unexpected process exits become failures. Failures inside a configured window are counted. When failures reach the configured threshold, the task becomes `crash_loop`.

## Config

The config root supports `version`, `project`, `groups`, `tasks`, and `diagnostics`.

Tasks support `cmd`, `cwd`, `env`, `mode`, `group`, `depends_on`, `ready`, `restart`, `diagnostics`, and `open`.

Readiness supports `tcp`, `http`, `command`, `log_contains`, `timeout`, and `interval`.

## Testing

Tests cover config parsing and validation, graph ordering, diagnostics, readiness probes, snapshots, and supervisor failure/crash-loop behavior. TUI tests focus on compiling state-to-view behavior rather than terminal pixel snapshots.
