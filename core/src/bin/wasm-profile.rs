use clap::Parser;
use sha2::{Digest, Sha256};
use sky_moon_scope_core::profiling_export::{
    export_profiling_results, resolve_export_format, ExportFormat,
};
use sky_moon_scope_core::simulation::profile_contract_with_flamegraph;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(about = "Profile a local Soroban WASM contract")]
struct Args {
    /// Path to the WASM file to profile.
    wasm_file: PathBuf,
    /// Exported contract function to invoke.
    #[arg(short, long)]
    function: String,
    /// Argument to pass to the contract; may be repeated.
    #[arg(long = "arg")]
    args: Vec<String>,
    /// Re-run profiling whenever the WASM file changes.
    #[arg(long)]
    watch: bool,
    /// Polling interval in milliseconds while watching.
    #[arg(long, default_value_t = 500)]
    watch_interval_ms: u64,
    /// Export format: json or csv. Defaults to json (or inferred from --output file extension).
    #[arg(short, long, value_enum)]
    format: Option<ExportFormat>,
    /// Output file path to export profiling results. If omitted, prints to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Export profiling results as JSON to the specified file path.
    #[arg(long)]
    export_json: Option<PathBuf>,
    /// Export profiling results as CSV to the specified file path.
    #[arg(long)]
    export_csv: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let wasm_bytes = match fs::read(&args.wasm_file) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("Could not read {}: {error}", args.wasm_file.display());
            std::process::exit(1);
        }
    };

    let format = resolve_export_format(args.format, args.output.as_deref());

    if let Err(error) = run_profile_with_export(
        &args.wasm_file,
        wasm_bytes.clone(),
        &args.function,
        &args.args,
        format,
        args.output.as_deref(),
        args.export_json.as_deref(),
        args.export_csv.as_deref(),
    ) {
        eprintln!("Profiling failed: {error}");
        if !args.watch {
            std::process::exit(1);
        }
    }
    if !args.watch {
        return Ok(());
    }

    println!("Watching {} for changes...", args.wasm_file.display());
    let mut previous_hash = content_hash(&wasm_bytes);
    let interval = Duration::from_millis(args.watch_interval_ms.max(1));

    loop {
        thread::sleep(interval);
        let current_bytes = match fs::read(&args.wasm_file) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("Could not read {}: {error}", args.wasm_file.display());
                continue;
            }
        };
        let current_hash = content_hash(&current_bytes);
        if !content_changed(&previous_hash, &current_hash) {
            continue;
        }

        previous_hash = current_hash;
        println!("\nWASM file changed; profiling again...");
        if let Err(error) = run_profile_with_export(
            &args.wasm_file,
            current_bytes,
            &args.function,
            &args.args,
            format,
            args.output.as_deref(),
            args.export_json.as_deref(),
            args.export_csv.as_deref(),
        ) {
            eprintln!("Profiling failed: {error}");
        }
    }
}

/// Backwards-compatible profiling runner that prints pretty JSON to stdout.
pub fn run_profile(
    wasm_file: &PathBuf,
    wasm_bytes: Vec<u8>,
    function: &str,
    args: &[String],
) -> Result<(), Box<dyn Error>> {
    run_profile_with_export(
        wasm_file,
        wasm_bytes,
        function,
        args,
        ExportFormat::Json,
        None,
        None,
        None,
    )
}

/// Runs profiling and exports the results in JSON or CSV format to stdout or files.
pub fn run_profile_with_export(
    wasm_file: &Path,
    wasm_bytes: Vec<u8>,
    function: &str,
    args: &[String],
    format: ExportFormat,
    output: Option<&Path>,
    export_json: Option<&Path>,
    export_csv: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    let profile_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        profile_contract_with_flamegraph(wasm_bytes, function.to_owned(), args.to_vec())
    }));

    let (resources, profile) = match profile_res {
        Ok(Ok(pair)) => pair,
        Ok(Err(err)) => return Err(Box::new(err)),
        Err(_) => {
            return Err(
                "profiling panicked unexpectedly; check contract custom sections and ABI".into(),
            );
        }
    };

    export_profiling_results(
        wasm_file,
        function,
        &resources,
        &profile,
        format,
        output,
        export_json,
        export_csv,
    )
}

fn content_hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn content_changed(previous_hash: &[u8; 32], current_hash: &[u8; 32]) -> bool {
    current_hash != previous_hash
}

#[cfg(test)]
mod tests {
    use super::{content_changed, content_hash, run_profile, run_profile_with_export, Args};
    use clap::Parser;
    use sky_moon_scope_core::profiling_export::ExportFormat;
    use std::path::{Path, PathBuf};

    #[test]
    fn detects_same_length_wasm_content_changes() {
        let original = b"wasm-v1";
        let changed = b"wasm-v2";

        assert_eq!(original.len(), changed.len());
        let original_hash = content_hash(original);
        let changed_hash = content_hash(changed);
        assert!(content_changed(&original_hash, &changed_hash));
        assert!(!content_changed(&original_hash, &original_hash));
    }

    #[test]
    fn run_profile_handles_corrupt_wasm_without_panic() {
        let bad_wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let path = PathBuf::from("mock.wasm");
        let res = run_profile(&path, bad_wasm, "nonexistent", &[]);
        assert!(
            res.is_err(),
            "should gracefully return error for invalid wasm"
        );
    }

    #[test]
    fn run_profile_with_export_handles_corrupt_wasm_without_panic() {
        let bad_wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let path = Path::new("mock.wasm");
        let res = run_profile_with_export(
            path,
            bad_wasm,
            "nonexistent",
            &[],
            ExportFormat::Csv,
            None,
            None,
            None,
        );
        assert!(
            res.is_err(),
            "should gracefully return error for invalid wasm"
        );
    }

    #[test]
    fn args_default_format_none_when_omitted() {
        let args =
            Args::try_parse_from(["wasm-profile", "test.wasm", "--function", "hello"]).unwrap();
        assert_eq!(args.format, None);
        assert_eq!(args.output, None);
        assert_eq!(args.export_json, None);
        assert_eq!(args.export_csv, None);
    }

    #[test]
    fn args_parse_csv_format() {
        let args = Args::try_parse_from([
            "wasm-profile",
            "test.wasm",
            "--function",
            "hello",
            "--format",
            "csv",
        ])
        .unwrap();
        assert_eq!(args.format, Some(ExportFormat::Csv));
    }

    #[test]
    fn args_parse_output_and_dedicated_exports() {
        let args = Args::try_parse_from([
            "wasm-profile",
            "test.wasm",
            "--function",
            "hello",
            "-o",
            "out.csv",
            "--export-json",
            "out.json",
            "--export-csv",
            "dedicated.csv",
        ])
        .unwrap();
        assert_eq!(args.output, Some(PathBuf::from("out.csv")));
        assert_eq!(args.export_json, Some(PathBuf::from("out.json")));
        assert_eq!(args.export_csv, Some(PathBuf::from("dedicated.csv")));
    }
}
