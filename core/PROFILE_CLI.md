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

### Exporting Profiling Results (JSON & CSV)

By default, profiling results are formatted as JSON and printed to standard output.
You can export results in either **JSON** or **CSV** formats, or save them directly
to a file using `--output` / `-o`:

```bash
# Export as CSV to stdout
cargo run --manifest-path core/Cargo.toml --bin wasm-profile -- \
  contracts/hello_soroban/target/wasm32-unknown-unknown/release/hello_soroban.wasm \
  --function hello --format csv

# Export directly to a CSV file (format automatically inferred from .csv extension)
cargo run --manifest-path core/Cargo.toml --bin wasm-profile -- \
  contracts/hello_soroban/target/wasm32-unknown-unknown/release/hello_soroban.wasm \
  --function hello -o profile_report.csv

# Export directly to a JSON file
cargo run --manifest-path core/Cargo.toml --bin wasm-profile -- \
  contracts/hello_soroban/target/wasm32-unknown-unknown/release/hello_soroban.wasm \
  --function hello -o profile_report.json

# Export to both JSON and CSV files simultaneously
cargo run --manifest-path core/Cargo.toml --bin wasm-profile -- \
  contracts/hello_soroban/target/wasm32-unknown-unknown/release/hello_soroban.wasm \
  --function hello --export-json results.json --export-csv results.csv
```

#### CSV Columns

The CSV export provides a structured table of per-function and resource metrics:
- `function`: The contract function name (or `func[N]`, with a `total` summary row for multi-function calls)
- `instructions`: Executed instruction count for the function
- `percentage`: Share of total instructions (`XX.XX%`)
- `cpu_instructions`: Total Soroban host CPU instructions
- `ram_bytes`: Total RAM consumed (bytes)
- `ledger_read_bytes`: Ledger read bytes
- `ledger_write_bytes`: Ledger write bytes
- `transaction_size_bytes`: Transaction size (bytes)
- `granularity`: Measurement granularity (`instrumented` or `budget`)
- `wasm_file`: Path to the profiled WASM artifact

Contracts containing custom sections (e.g., Soroban metadata,
spec entries, or debug symbols) are safely handled and preserved without causing CLI
panics. Stop watch mode with Ctrl+C.
