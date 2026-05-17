# Fake Stack Sample

A tiny papaproc playground that demonstrates dependency-aware startup, readiness probes, delayed services, and noisy logs.

Run from the repository root:

```bash
cargo run -- --config examples/fake-stack/papaproc.yaml run
```

Run the sample smoke test without opening the TUI:

```bash
bash examples/fake-stack/smoke-test.sh
```

What happens:

- `fake-db` waits 2 seconds, then opens TCP port `15432`.
- `fake-api` waits for `fake-db`, waits 2 seconds, then serves `GET /health` on `18080`.
- `fake-web` waits for `fake-api`, waits 2 seconds, then emits frontend-style logs.
- `noisy-logs` is manual and emits warnings/errors for testing the errors-only log filter.

Try these keys in the TUI:

- `g` to view the graph.
- `f` to view failures.
- `e` to toggle errors-only logs.
- `r` to restart a selected task.
- `R` to restart a selected task and configured dependants.
