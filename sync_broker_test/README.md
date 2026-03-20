This is an fuzzer built via LibAFL, its just an example, if you are looking for using SyncFromBrokerStage specific stage.
# sync_broker_test

Minimal fuzzer to test **SyncFromBrokerStage** and that `corpus_size` is reported correctly to the broker (the fix in `crates/libafl/src/stages/sync.rs` that sends `state.corpus().count()` instead of `0`).

## What it does

- Uses the **Launcher** (broker + clients over LLMP).
- Each client runs **SyncFromBrokerStage**, which sends the client's corpus entries to the broker with `corpus_size: state.corpus().count()`.
- The broker updates per-client stats and the **SimpleMonitor** prints lines like `corpus: 8` (or similar). With the fix, you should see **non-zero** corpus for clients; without it you would see `corpus: 0`.

## Build

From this directory:

```bash
cargo build --release
```

Binary: `target/release/sync_broker_test` (or `target/debug/sync_broker_test` for debug).

## Run (WSL / Linux)

**One core (broker only, then re-run for client):**

```bash
# First run: starts broker
./target/release/sync_broker_test --cores 0

# Second run (in another terminal or after stopping): runs as client
./target/release/sync_broker_test --cores 0
```

**Broker + one client (needs fork; Linux):**

```bash
./target/release/sync_broker_test --cores 0,1
```

Watch the monitor output: you should see `corpus: 8` (or another non-zero number) for the client, confirming that the broker is receiving the real corpus size from SyncFromBrokerStage.
This runs the fuzzer for a short time and checks that the output contains a non-zero corpus line (e.g. `corpus: 8`). Best run in WSL/Linux where the launcher can use fork and multiple cores.

## Notes

- **Windows**: The launcher does not use fork; multi-core runs may re-exec the same binary. For the most reliable test of SyncFromBrokerStage + corpus_size, use **WSL** or Linux with `--cores 0,1`.
- The fuzzer uses a trivial in-process harness (no C++ or libafl_cc). It only exists to exercise the sync/broker path and corpus_size reporting.
