use std::{path::PathBuf, process::ExitCode, time::Duration};

use codegraph_bench::{
    default_mvp4_language_readiness_runner_options, run_mvp4_language_readiness,
};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("mvp4 language readiness runner failed: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let mut release_binary = None;
    let mut manifest_path = None;
    let mut run_root = None;
    let mut timeout_seconds = 120u64;
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--binary" => {
                index += 1;
                release_binary = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--binary requires a path".to_string())?,
                ));
            }
            "--manifest" => {
                index += 1;
                manifest_path = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--manifest requires a path".to_string())?,
                ));
            }
            "--run-root" => {
                index += 1;
                run_root = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--run-root requires a path".to_string())?,
                ));
            }
            "--timeout-seconds" => {
                index += 1;
                timeout_seconds = args
                    .get(index)
                    .ok_or_else(|| "--timeout-seconds requires a value".to_string())?
                    .parse::<u64>()
                    .map_err(|error| format!("invalid --timeout-seconds: {error}"))?;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: mvp4_language_readiness --binary <release-codegraph-mcp> [--manifest <manifest.json>] [--run-root <external-empty-dir>] [--timeout-seconds <n>]"
                );
                return Ok(ExitCode::SUCCESS);
            }
            value => return Err(format!("unknown option: {value}")),
        }
        index += 1;
    }
    let release_binary = release_binary.ok_or_else(|| {
        "--binary is required; manifest validation alone is not a readiness run".to_string()
    })?;
    let mut options = default_mvp4_language_readiness_runner_options(release_binary);
    if let Some(value) = manifest_path {
        options.manifest_path = value;
    }
    if let Some(value) = run_root {
        options.run_root = value;
    }
    options.command_timeout = Duration::from_secs(timeout_seconds);
    let report = run_mvp4_language_readiness(&options).map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
    );
    if report.ready_to_enter_mvp4_4 {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(2))
    }
}
