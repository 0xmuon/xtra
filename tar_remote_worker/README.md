# tar_remote_worker_fuzzer

An fuzzer example for pre-gsoc,short demo fuzzer showing the in-process Remote Worker Stage design with a real Rust target (`tar` crate).

## What this demonstrates

- `InProcessRemoteWorkerQueue<BytesInput>` as in-memory transport
- `RemoteWorkerMutationalLauncherStage` to enqueue mutated jobs
- `RemoteWorkerCollectorStage` to dequeue, execute, and integrate results (Variant v1-style on main side)
- A slow-ish harness (`src/harness.rs`) that parses tar archives and scans entry bodies

## Run

From this directory:

```bash
cargo run --release
```

No-TUI mode:

```bash
cargo run --release --no-default-features --features std
```

Verbose logs+tuned in-process worker settings:

```bash
TAR_REMOTE_WORKERS=7 TAR_REMOTE_JOBS_PER_INPUT=8 TAR_REMOTE_COLLECT_BATCH=7 RUST_LOG=info cargo run --release --no-default-features --features std
```
change according to need.
## Notes

- `crashes/` may stay empty for long periods; this is normal.
- The main signal of progress is growing executions/coverage/interesting inputs.
- This is an in-process prototype to validate stage boundaries before queue-backed multi-process wiring.
- Worker settings are simulated in-process and can be tuned with env vars:
  - `TAR_REMOTE_WORKERS` (default: `cpu_cores - 1`, clamped to `[1, 2*cpu_cores]`)
  - `TAR_REMOTE_JOBS_PER_INPUT` (default: `max(workers, 2)`)
  - `TAR_REMOTE_COLLECT_BATCH` (default: `workers`)

