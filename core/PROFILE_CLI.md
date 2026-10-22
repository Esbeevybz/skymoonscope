# WASM Profiler CLI

The `wasm-profile` binary profiles a local Soroban WASM file using the same
instrumentation and budget fallback as the profiling API.

```bash
cargo run --manifest-path core/Cargo.toml --bin wasm-profile -- \
  contracts/hello_soroban/target/wasm32-unknown-unknown/release/hello_soroban.wasm \
  --function hello
```

Use `--arg VALUE` once for each function argument. Add `--watch` to profile
immediately and rerun whenever the file contents change. The watcher polls
every 500 ms by default; set `--watch-interval-ms` to change that interval.

```bash
cargo run --manifest-path core/Cargo.toml --bin wasm-profile -- \
  /path/to/contract.wasm --function hello --watch --watch-interval-ms 1000
```

Each profiling run prints a JSON object containing resource measurements and
the flamegraph profile. Contracts containing custom sections (e.g., Soroban metadata,
spec entries, or debug symbols) are safely handled and preserved without causing CLI
panics. Stop watch mode with Ctrl+C.