use std::{
    env, fs,
    path::{Path, PathBuf},
};

use codegraph_bench::{
    measure_mvp4_sparse_sidecar_projection, run_mvp4_ast_census, Mvp4AstCensusOptions,
    Mvp4StorageProjectionOptions,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let mut census_output: Option<PathBuf> = None;
    let mut storage_output: Option<PathBuf> = None;
    let mut work_dir: Option<PathBuf> = None;
    let mut roots = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => census_output = Some(next_path(&mut args, "--output")?),
            "--storage-output" => storage_output = Some(next_path(&mut args, "--storage-output")?),
            "--work-dir" => work_dir = Some(next_path(&mut args, "--work-dir")?),
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag: {other}"));
            }
            other => roots.push(PathBuf::from(other)),
        }
    }

    if roots.is_empty() {
        return Err("usage: mvp4_measurement --output <census.json> --storage-output <projection.json> --work-dir <dir> <root>...".to_string());
    }

    if let Some(output) = census_output {
        let report = run_mvp4_ast_census(&roots, &Mvp4AstCensusOptions::default())
            .map_err(|error| error.to_string())?;
        write_json(&output, &report)?;
    }

    if let Some(output) = storage_output {
        let work_dir =
            work_dir.unwrap_or_else(|| env::temp_dir().join("codegraph-mvp4-measurement"));
        let projection = measure_mvp4_sparse_sidecar_projection(
            &work_dir,
            &Mvp4StorageProjectionOptions::default(),
        )
        .map_err(|error| error.to_string())?;
        write_json(&output, &projection)?;
    }

    Ok(())
}

fn next_path(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{flag} requires a path"))
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, format!("{json}\n")).map_err(|error| error.to_string())
}

fn print_help() {
    println!(
        "usage: mvp4_measurement --output <census.json> --storage-output <projection.json> --work-dir <dir> <root>..."
    );
}
