//! Profiling results export utilities (JSON and CSV).
//!
//! Provides formatting and file export routines for WASM contract profiling results,
//! supporting both JSON and CSV output formats.

use crate::simulation::{ProfileResult, SorobanResources};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;

/// Supported export formats for profiling results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// Pretty-printed JSON format.
    Json,
    /// Comma-separated values format.
    Csv,
}

impl FromStr for ExportFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(ExportFormat::Json),
            "csv" => Ok(ExportFormat::Csv),
            other => Err(format!(
                "invalid format '{other}': expected 'json' or 'csv'"
            )),
        }
    }
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportFormat::Json => write!(f, "json"),
            ExportFormat::Csv => write!(f, "csv"),
        }
    }
}

/// Escapes a string for RFC 4180 compliant CSV output.
pub fn escape_csv(val: &str) -> String {
    if val.contains(',') || val.contains('"') || val.contains('\n') || val.contains('\r') {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val.to_string()
    }
}

/// Formats profiling results as a pretty-printed JSON string.
pub fn format_profiling_json(
    wasm_file: &Path,
    function: &str,
    resources: &SorobanResources,
    profile: &ProfileResult,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&serde_json::json!({
        "wasm_file": wasm_file,
        "function": function,
        "resources": resources,
        "profile": profile,
    }))
}

/// Formats profiling results as an RFC 4180 compliant CSV string.
///
/// Columns:
/// `function,instructions,percentage,cpu_instructions,ram_bytes,ledger_read_bytes,ledger_write_bytes,transaction_size_bytes,granularity,wasm_file`
pub fn format_profiling_csv(
    wasm_file: &Path,
    function: &str,
    resources: &SorobanResources,
    profile: &ProfileResult,
) -> String {
    let wasm_path_str = wasm_file.to_string_lossy();
    let wasm_escaped = escape_csv(&wasm_path_str);
    let granularity_escaped = escape_csv(&profile.granularity);

    let mut lines = Vec::new();
    lines.push(
        "function,instructions,percentage,cpu_instructions,ram_bytes,ledger_read_bytes,ledger_write_bytes,transaction_size_bytes,granularity,wasm_file".to_string(),
    );

    if profile.per_function.is_empty() {
        let fn_escaped = escape_csv(function);
        lines.push(format!(
            "{},{},100.00%,{},{},{},{},{},{},{}",
            fn_escaped,
            profile.total_instructions,
            resources.cpu_instructions,
            resources.ram_bytes,
            resources.ledger_read_bytes,
            resources.ledger_write_bytes,
            resources.transaction_size_bytes,
            granularity_escaped,
            wasm_escaped,
        ));
    } else {
        let mut entries: Vec<(&String, &u64)> = profile.per_function.iter().collect();
        // Sort by instructions descending, then by function name ascending
        entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

        for (fn_name, count) in &entries {
            let percentage = if profile.total_instructions > 0 {
                (**count as f64 / profile.total_instructions as f64) * 100.0
            } else {
                0.0
            };
            let fn_escaped = escape_csv(fn_name);
            lines.push(format!(
                "{},{},{:.2}%,{},{},{},{},{},{},{}",
                fn_escaped,
                count,
                percentage,
                resources.cpu_instructions,
                resources.ram_bytes,
                resources.ledger_read_bytes,
                resources.ledger_write_bytes,
                resources.transaction_size_bytes,
                granularity_escaped,
                wasm_escaped,
            ));
        }

        if entries.len() > 1 {
            lines.push(format!(
                "total,{},100.00%,{},{},{},{},{},{},{}",
                profile.total_instructions,
                resources.cpu_instructions,
                resources.ram_bytes,
                resources.ledger_read_bytes,
                resources.ledger_write_bytes,
                resources.transaction_size_bytes,
                granularity_escaped,
                wasm_escaped,
            ));
        }
    }

    lines.join("\n")
}

/// Exports profiling results to stdout or files based on user configuration.
pub fn export_profiling_results(
    wasm_file: &Path,
    function: &str,
    resources: &SorobanResources,
    profile: &ProfileResult,
    format: ExportFormat,
    output: Option<&Path>,
    export_json: Option<&Path>,
    export_csv: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    let json_content = if format == ExportFormat::Json || export_json.is_some() {
        Some(format_profiling_json(
            wasm_file, function, resources, profile,
        )?)
    } else {
        None
    };

    let csv_content = if format == ExportFormat::Csv || export_csv.is_some() {
        Some(format_profiling_csv(
            wasm_file, function, resources, profile,
        ))
    } else {
        None
    };

    if let Some(json_path) = export_json {
        if let Some(parent) = json_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let content = json_content.as_deref().unwrap();
        fs::write(json_path, content)?;
        println!(
            "Profiling results exported to JSON: {}",
            json_path.display()
        );
    }

    if let Some(csv_path) = export_csv {
        if let Some(parent) = csv_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let content = csv_content.as_deref().unwrap();
        fs::write(csv_path, content)?;
        println!("Profiling results exported to CSV: {}", csv_path.display());
    }

    match output {
        Some(out_path) => {
            if let Some(parent) = out_path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            let content = match format {
                ExportFormat::Json => json_content.as_deref().unwrap(),
                ExportFormat::Csv => csv_content.as_deref().unwrap(),
            };
            fs::write(out_path, content)?;
            println!("Profiling results exported to {}", out_path.display());
        }
        None => {
            if export_json.is_none() && export_csv.is_none() {
                let content = match format {
                    ExportFormat::Json => json_content.as_deref().unwrap(),
                    ExportFormat::Csv => csv_content.as_deref().unwrap(),
                };
                println!("{content}");
            }
        }
    }

    Ok(())
}

/// Resolves the effective export format from optional format argument and output path.
pub fn resolve_export_format(
    format_arg: Option<ExportFormat>,
    output_path: Option<&Path>,
) -> ExportFormat {
    if let Some(fmt) = format_arg {
        return fmt;
    }
    if let Some(out) = output_path {
        if let Some(ext) = out.extension().and_then(|s| s.to_str()) {
            if ext.eq_ignore_ascii_case("csv") {
                return ExportFormat::Csv;
            } else if ext.eq_ignore_ascii_case("json") {
                return ExportFormat::Json;
            }
        }
    }
    ExportFormat::Json
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn sample_resources() -> SorobanResources {
        SorobanResources {
            cpu_instructions: 15_000,
            ram_bytes: 4096,
            ledger_read_bytes: 64,
            ledger_write_bytes: 128,
            transaction_size_bytes: 256,
        }
    }

    fn sample_profile_multi() -> ProfileResult {
        let mut per_function = HashMap::new();
        per_function.insert("calc".to_string(), 8000);
        per_function.insert("helper".to_string(), 2000);
        ProfileResult {
            flamegraph: "calc 8000\nhelper 2000\n".to_string(),
            per_function,
            total_instructions: 10_000,
            granularity: "instrumented".to_string(),
        }
    }

    fn sample_profile_single() -> ProfileResult {
        let mut per_function = HashMap::new();
        per_function.insert("hello".to_string(), 5000);
        ProfileResult {
            flamegraph: "hello 5000\n".to_string(),
            per_function,
            total_instructions: 5000,
            granularity: "instrumented".to_string(),
        }
    }

    fn sample_profile_budget() -> ProfileResult {
        ProfileResult {
            flamegraph: String::new(),
            per_function: HashMap::new(),
            total_instructions: 12_000,
            granularity: "budget".to_string(),
        }
    }

    #[test]
    fn test_export_format_from_str() {
        assert_eq!("json".parse::<ExportFormat>().unwrap(), ExportFormat::Json);
        assert_eq!("JSON".parse::<ExportFormat>().unwrap(), ExportFormat::Json);
        assert_eq!("csv".parse::<ExportFormat>().unwrap(), ExportFormat::Csv);
        assert_eq!("CSV".parse::<ExportFormat>().unwrap(), ExportFormat::Csv);
        assert!("xml".parse::<ExportFormat>().is_err());
    }

    #[test]
    fn test_escape_csv() {
        assert_eq!(escape_csv("simple"), "simple");
        assert_eq!(escape_csv("hello, world"), "\"hello, world\"");
        assert_eq!(escape_csv("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(escape_csv("multi\nline"), "\"multi\nline\"");
    }

    #[test]
    fn test_resolve_export_format() {
        assert_eq!(
            resolve_export_format(Some(ExportFormat::Csv), None),
            ExportFormat::Csv
        );
        assert_eq!(
            resolve_export_format(Some(ExportFormat::Json), Some(Path::new("file.csv"))),
            ExportFormat::Json
        );
        assert_eq!(
            resolve_export_format(None, Some(Path::new("output.csv"))),
            ExportFormat::Csv
        );
        assert_eq!(
            resolve_export_format(None, Some(Path::new("output.JSON"))),
            ExportFormat::Json
        );
        assert_eq!(
            resolve_export_format(None, Some(Path::new("output.txt"))),
            ExportFormat::Json
        );
        assert_eq!(resolve_export_format(None, None), ExportFormat::Json);
    }

    #[test]
    fn test_format_profiling_json() {
        let wasm = Path::new("test.wasm");
        let res = sample_resources();
        let prof = sample_profile_single();

        let json_str = format_profiling_json(wasm, "hello", &res, &prof).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["wasm_file"], "test.wasm");
        assert_eq!(parsed["function"], "hello");
        assert_eq!(parsed["resources"]["cpu_instructions"], 15_000);
        assert_eq!(parsed["profile"]["total_instructions"], 5000);
        assert_eq!(parsed["profile"]["per_function"]["hello"], 5000);
        assert_eq!(parsed["profile"]["granularity"], "instrumented");
    }

    #[test]
    fn test_format_profiling_csv_single_function() {
        let wasm = Path::new("contract.wasm");
        let res = sample_resources();
        let prof = sample_profile_single();

        let csv_str = format_profiling_csv(wasm, "hello", &res, &prof);
        let lines: Vec<&str> = csv_str.lines().collect();

        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            "function,instructions,percentage,cpu_instructions,ram_bytes,ledger_read_bytes,ledger_write_bytes,transaction_size_bytes,granularity,wasm_file"
        );
        assert_eq!(
            lines[1],
            "hello,5000,100.00%,15000,4096,64,128,256,instrumented,contract.wasm"
        );
    }

    #[test]
    fn test_format_profiling_csv_multiple_functions() {
        let wasm = Path::new("multi.wasm");
        let res = sample_resources();
        let prof = sample_profile_multi();

        let csv_str = format_profiling_csv(wasm, "calc", &res, &prof);
        let lines: Vec<&str> = csv_str.lines().collect();

        assert_eq!(lines.len(), 4);
        assert_eq!(
            lines[0],
            "function,instructions,percentage,cpu_instructions,ram_bytes,ledger_read_bytes,ledger_write_bytes,transaction_size_bytes,granularity,wasm_file"
        );
        // calc is 8000/10000 = 80.00%
        assert_eq!(
            lines[1],
            "calc,8000,80.00%,15000,4096,64,128,256,instrumented,multi.wasm"
        );
        // helper is 2000/10000 = 20.00%
        assert_eq!(
            lines[2],
            "helper,2000,20.00%,15000,4096,64,128,256,instrumented,multi.wasm"
        );
        // total row
        assert_eq!(
            lines[3],
            "total,10000,100.00%,15000,4096,64,128,256,instrumented,multi.wasm"
        );
    }

    #[test]
    fn test_format_profiling_csv_empty_per_function_fallback() {
        let wasm = Path::new("budget.wasm");
        let res = sample_resources();
        let prof = sample_profile_budget();

        let csv_str = format_profiling_csv(wasm, "run", &res, &prof);
        let lines: Vec<&str> = csv_str.lines().collect();

        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[1],
            "run,12000,100.00%,15000,4096,64,128,256,budget,budget.wasm"
        );
    }

    #[test]
    fn test_export_profiling_results_to_files() {
        let dir = tempfile::tempdir().unwrap();
        let wasm = PathBuf::from("test.wasm");
        let res = sample_resources();
        let prof = sample_profile_multi();

        let json_out = dir.path().join("results.json");
        let csv_out = dir.path().join("results.csv");

        export_profiling_results(
            &wasm,
            "calc",
            &res,
            &prof,
            ExportFormat::Json,
            None,
            Some(&json_out),
            Some(&csv_out),
        )
        .unwrap();

        assert!(json_out.exists());
        assert!(csv_out.exists());

        let json_read = fs::read_to_string(&json_out).unwrap();
        assert!(json_read.contains("\"total_instructions\": 10000"));

        let csv_read = fs::read_to_string(&csv_out).unwrap();
        assert!(csv_read.contains("calc,8000,80.00%"));
    }
}
