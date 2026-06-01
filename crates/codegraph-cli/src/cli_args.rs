//! CLI argument parsers for the index/watch/bench/gate/trace/context-pack/
//! status commands and their option structs.
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use std::path::PathBuf;

use serde_json::Value;

use crate::*;

pub(crate) fn parse_index_bool(raw: &str, flag: &str) -> Result<bool, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("{flag} requires true or false, got {raw}")),
    }
}

pub(crate) fn parse_index_usize_arg(raw: Option<&String>, flag: &str) -> Result<usize, String> {
    let Some(raw) = raw else {
        return Err(format!("{flag} requires a value"));
    };
    let value = raw
        .parse::<usize>()
        .map_err(|_| format!("invalid {flag} value: {raw}"))?;
    if value == 0 {
        return Err(format!("{flag} must be at least 1"));
    }
    Ok(value)
}

pub(crate) fn parse_index_positive_f64_arg(
    raw: Option<&String>,
    flag: &str,
) -> Result<f64, String> {
    let Some(raw) = raw else {
        return Err(format!("{flag} requires a value"));
    };
    let value = raw
        .parse::<f64>()
        .map_err(|_| format!("invalid {flag} value: {raw}"))?;
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(value)
}

pub(crate) fn parse_watch_options(args: &[String]) -> Result<WatchOptions, String> {
    let mut repo = None;
    let mut db = None;
    let mut debounce = Duration::from_millis(250);
    let mut once = false;
    let mut changed_paths = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--debounce-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--debounce-ms requires a value".to_string());
                };
                let millis = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid debounce milliseconds: {value}"))?;
                debounce = Duration::from_millis(millis);
            }
            "--db" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--db requires a path".to_string());
                };
                db = Some(PathBuf::from(value));
            }
            "--once" => once = true,
            "--changed" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--changed requires a path".to_string());
                };
                changed_paths.push(PathBuf::from(value));
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown watch option: {value}"));
            }
            value => {
                if repo.is_some() {
                    return Err(format!("unexpected watch argument: {value}"));
                }
                repo = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }

    Ok(WatchOptions {
        repo: repo.unwrap_or_else(|| PathBuf::from(".")),
        db,
        debounce,
        once,
        changed_paths,
    })
}

pub(crate) fn parse_ui_options(args: &[String]) -> Result<UiOptions, String> {
    let mut repo = None;
    let mut host = "127.0.0.1".to_string();
    let mut port = 7878_u16;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--host" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--host requires a value".to_string());
                };
                host = value.clone();
            }
            "--port" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--port requires a value".to_string());
                };
                port = value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid UI port: {value}"))?;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown serve-ui option: {value}"));
            }
            value => {
                if repo.is_some() {
                    return Err(format!("unexpected serve-ui argument: {value}"));
                }
                repo = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }

    if !is_local_host(&host) {
        return Err(format!(
            "serve-ui is local-only by default; refusing host {host}"
        ));
    }

    Ok(UiOptions {
        repo: repo.unwrap_or_else(|| PathBuf::from(".")),
        host,
        port,
    })
}

pub(crate) fn parse_bench_options(args: &[String]) -> Result<BenchOptions, String> {
    let mut baselines = Vec::new();
    let mut format = BenchReportFormat::Json;
    let mut output = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--baseline" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--baseline requires a value".to_string());
                };
                for value in raw.split(',').filter(|value| !value.trim().is_empty()) {
                    baselines.push(
                        value
                            .parse::<BaselineMode>()
                            .map_err(|error| error.to_string())?,
                    );
                }
            }
            "--format" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--format requires json or markdown".to_string());
                };
                format = match raw.trim().to_ascii_lowercase().as_str() {
                    "json" => BenchReportFormat::Json,
                    "markdown" | "md" => BenchReportFormat::Markdown,
                    other => return Err(format!("unknown benchmark report format: {other}")),
                };
            }
            "--output" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--output requires a path".to_string());
                };
                output = Some(PathBuf::from(raw));
            }
            value => return Err(format!("unknown benchmark option: {value}")),
        }
        index += 1;
    }

    Ok(BenchOptions {
        baselines,
        format,
        output,
    })
}

pub(crate) fn parse_graph_truth_gate_options(
    args: &[String],
) -> Result<GraphTruthGateOptions, String> {
    let mut options = codegraph_bench::default_graph_truth_gate_options();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--cases" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--cases requires a path".to_string());
                };
                options.cases = PathBuf::from(value);
            }
            "--fixture-root" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--fixture-root requires a path".to_string());
                };
                options.fixture_root = PathBuf::from(value);
            }
            "--out-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-json requires a path".to_string());
                };
                options.out_json = PathBuf::from(value);
            }
            "--out-md" | "--out-markdown" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-md requires a path".to_string());
                };
                options.out_md = PathBuf::from(value);
            }
            "--fail-on-forbidden" => {
                options.fail_on_forbidden = true;
            }
            "--fail-on-missing-source-span" => {
                options.fail_on_missing_source_span = true;
            }
            "--fail-on-unresolved-exact" => {
                options.fail_on_unresolved_exact = true;
            }
            "--fail-on-derived-without-provenance" => {
                options.fail_on_derived_without_provenance = true;
            }
            "--fail-on-test-mock-production-leak" => {
                options.fail_on_test_mock_production_leak = true;
            }
            "--update-mode" => {
                options.update_mode = true;
            }
            "--keep-workdirs" => {
                options.keep_workdirs = true;
            }
            "--verbose" => {
                options.verbose = true;
            }
            value => return Err(format!("unknown graph-truth option: {value}")),
        }
        index += 1;
    }
    Ok(options)
}

pub(crate) fn parse_context_packet_gate_options(
    args: &[String],
) -> Result<ContextPacketGateOptions, String> {
    let mut options = codegraph_bench::default_context_packet_gate_options();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--cases" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--cases requires a path".to_string());
                };
                options.cases = PathBuf::from(value);
            }
            "--fixture-root" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--fixture-root requires a path".to_string());
                };
                options.fixture_root = PathBuf::from(value);
            }
            "--out-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-json requires a path".to_string());
                };
                options.out_json = PathBuf::from(value);
            }
            "--out-md" | "--out-markdown" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-md requires a path".to_string());
                };
                options.out_md = PathBuf::from(value);
            }
            "--top-k" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--top-k requires a positive integer".to_string());
                };
                options.top_k = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --top-k: {error}"))?;
                if options.top_k == 0 {
                    return Err("--top-k must be greater than zero".to_string());
                }
            }
            "--budget" | "--token-budget" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--budget requires a positive integer".to_string());
                };
                options.token_budget = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --budget: {error}"))?;
                if options.token_budget == 0 {
                    return Err("--budget must be greater than zero".to_string());
                }
            }
            value => return Err(format!("unknown context-packet option: {value}")),
        }
        index += 1;
    }
    Ok(options)
}

pub(crate) fn parse_retrieval_ablation_options(
    args: &[String],
) -> Result<RetrievalAblationOptions, String> {
    let mut options = codegraph_bench::default_retrieval_ablation_options();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--cases" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--cases requires a path".to_string());
                };
                options.cases = PathBuf::from(value);
            }
            "--fixture-root" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--fixture-root requires a path".to_string());
                };
                options.fixture_root = PathBuf::from(value);
            }
            "--out-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-json requires a path".to_string());
                };
                options.out_json = PathBuf::from(value);
            }
            "--out-md" | "--out-markdown" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-md requires a path".to_string());
                };
                options.out_md = PathBuf::from(value);
            }
            "--top-k" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--top-k requires a positive integer".to_string());
                };
                options.top_k = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --top-k: {error}"))?;
            }
            "--mode" | "--modes" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--mode requires a retrieval-ablation mode".to_string());
                };
                for raw in value.split(',') {
                    options.modes.push(
                        raw.parse::<RetrievalAblationMode>()
                            .map_err(|error| error.to_string())?,
                    );
                }
            }
            value => return Err(format!("unknown retrieval-ablation option: {value}")),
        }
        index += 1;
    }
    Ok(options)
}

pub(crate) fn parse_two_layer_bench_options(
    args: &[String],
) -> Result<TwoLayerBenchOptions, String> {
    let mut options = codegraph_bench::default_two_layer_bench_options();
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--run-id" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--run-id requires a value".to_string());
                };
                options.run_id = Some(raw.clone());
            }
            "--timeout-ms" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--timeout-ms requires a non-negative integer".to_string());
                };
                options.timeout_ms = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}"))?
                    .min(codegraph_bench::MAX_BENCH_TASK_MS);
            }
            "--top-k" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--top-k requires a positive integer".to_string());
                };
                options.top_k = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --top-k value: {raw}"))?;
                if options.top_k == 0 {
                    return Err("--top-k must be greater than zero".to_string());
                }
            }
            "--competitor-bin" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--competitor-bin requires a path".to_string());
                };
                options.competitor_executable = Some(PathBuf::from(raw));
            }
            "--autoresearch-repo" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--autoresearch-repo requires a path".to_string());
                };
                options.autoresearch_repo = Some(PathBuf::from(raw));
            }
            "--skip-autoresearch" => {
                options.include_autoresearch = false;
            }
            "--fake-agent" | "--dry-run" => {
                options.fake_agent = true;
                options.dry_run = true;
            }
            "--run-root" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--run-root requires a path".to_string());
                };
                let run_root = PathBuf::from(raw);
                if !run_root
                    .components()
                    .any(|component| component.as_os_str() == "codegraph-bench-runs")
                {
                    return Err(
                        "--run-root must be under target/codegraph-bench-runs/<run_id>".to_string(),
                    );
                }
                options.run_root = Some(run_root);
            }
            value => return Err(format!("unknown two-layer benchmark option: {value}")),
        }
        index += 1;
    }

    Ok(options)
}

pub(crate) fn parse_trace_append_options(args: &[String]) -> Result<TraceAppendOptions, String> {
    let mut repo = PathBuf::from(".");
    let mut repo_id = None;
    let mut trace_root = PathBuf::from(DEFAULT_TRACE_ROOT);
    let mut run_id = None;
    let mut task_id = None;
    let mut event_type = None;
    let mut trace_id = None;
    let mut tool = None;
    let mut status = None;
    let mut actor = None;
    let mut action_kind = None;
    let mut result_status = None;
    let mut latency_ms = 0u128;
    let mut token_estimate = None;
    let mut evidence_refs = Vec::new();
    let mut edited_files = Vec::new();
    let mut test_command = None;
    let mut test_status = None;
    let mut input = None;
    let mut output = None;
    let mut error = None;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--repo" => {
                index += 1;
                repo = PathBuf::from(required_cli_value(args, index, "--repo")?);
            }
            "--repo-id" => {
                index += 1;
                repo_id = Some(required_cli_value(args, index, "--repo-id")?.to_string());
            }
            "--trace-root" => {
                index += 1;
                trace_root = PathBuf::from(required_cli_value(args, index, "--trace-root")?);
            }
            "--run-id" => {
                index += 1;
                run_id = Some(required_cli_value(args, index, "--run-id")?.to_string());
            }
            "--task-id" => {
                index += 1;
                task_id = Some(required_cli_value(args, index, "--task-id")?.to_string());
            }
            "--event-type" => {
                index += 1;
                let raw = required_cli_value(args, index, "--event-type")?;
                event_type = Some(
                    raw.parse::<TraceEventType>()
                        .map_err(|error| error.to_string())?,
                );
            }
            "--trace-id" => {
                index += 1;
                trace_id = Some(required_cli_value(args, index, "--trace-id")?.to_string());
            }
            "--tool" => {
                index += 1;
                tool = Some(required_cli_value(args, index, "--tool")?.to_string());
            }
            "--status" => {
                index += 1;
                status = Some(required_cli_value(args, index, "--status")?.to_string());
            }
            "--actor" => {
                index += 1;
                actor = Some(required_cli_value(args, index, "--actor")?.to_string());
            }
            "--action-kind" => {
                index += 1;
                action_kind = Some(required_cli_value(args, index, "--action-kind")?.to_string());
            }
            "--result-status" => {
                index += 1;
                result_status =
                    Some(required_cli_value(args, index, "--result-status")?.to_string());
            }
            "--latency-ms" => {
                index += 1;
                let raw = required_cli_value(args, index, "--latency-ms")?;
                latency_ms = raw
                    .parse::<u128>()
                    .map_err(|_| format!("invalid --latency-ms value: {raw}"))?;
            }
            "--token-estimate" => {
                index += 1;
                token_estimate = Some(parse_token_estimate(required_cli_value(
                    args,
                    index,
                    "--token-estimate",
                )?));
            }
            "--evidence-ref" => {
                index += 1;
                evidence_refs.push(required_cli_value(args, index, "--evidence-ref")?.to_string());
            }
            "--edited-file" => {
                index += 1;
                edited_files.push(required_cli_value(args, index, "--edited-file")?.to_string());
            }
            "--test-command" => {
                index += 1;
                test_command = Some(required_cli_value(args, index, "--test-command")?.to_string());
            }
            "--test-status" => {
                index += 1;
                test_status = Some(required_cli_value(args, index, "--test-status")?.to_string());
            }
            "--input-json" => {
                index += 1;
                input = Some(parse_json_arg(required_cli_value(
                    args,
                    index,
                    "--input-json",
                )?)?);
            }
            "--output-json" => {
                index += 1;
                output = Some(parse_json_arg(required_cli_value(
                    args,
                    index,
                    "--output-json",
                )?)?);
            }
            "--error" => {
                index += 1;
                error = Some(required_cli_value(args, index, "--error")?.to_string());
            }
            value => return Err(format!("unknown trace append option: {value}")),
        }
        index += 1;
    }

    let event = TraceAppendEvent {
        event_type: event_type.ok_or_else(|| "--event-type is required".to_string())?,
        trace_id: trace_id.unwrap_or_else(|| "unknown".to_string()),
        tool: tool.unwrap_or_else(|| "unknown".to_string()),
        status: status.unwrap_or_else(|| "unknown".to_string()),
        actor,
        action_kind,
        result_status,
        latency_ms,
        token_estimate,
        evidence_refs: (!evidence_refs.is_empty()).then(|| json!(evidence_refs)),
        edited_files: (!edited_files.is_empty()).then(|| json!(edited_files)),
        test_command,
        test_status,
        input,
        output,
        error,
    };
    Ok(TraceAppendOptions {
        repo,
        repo_id,
        trace_root,
        run_id,
        task_id,
        event,
    })
}

pub(crate) fn parse_trace_replay_options(args: &[String]) -> Result<PathBuf, String> {
    let mut events_path = None;
    let mut trace_root = PathBuf::from(DEFAULT_TRACE_ROOT);
    let mut run_id = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--events" => {
                index += 1;
                events_path = Some(PathBuf::from(required_cli_value(args, index, "--events")?));
            }
            "--trace-root" => {
                index += 1;
                trace_root = PathBuf::from(required_cli_value(args, index, "--trace-root")?);
            }
            "--run-id" => {
                index += 1;
                run_id = Some(required_cli_value(args, index, "--run-id")?.to_string());
            }
            value => return Err(format!("unknown trace replay option: {value}")),
        }
        index += 1;
    }
    events_path
        .or_else(|| run_id.map(|id| trace_root.join(id).join("events.jsonl")))
        .ok_or_else(|| "trace replay requires --events or --run-id".to_string())
}

pub(crate) fn required_cli_value<'a>(
    args: &'a [String],
    index: usize,
    flag: &str,
) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

pub(crate) fn parse_json_arg(raw: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|error| format!("invalid JSON argument: {error}"))
}

pub(crate) fn parse_token_estimate(raw: &str) -> Value {
    if raw.eq_ignore_ascii_case("unknown") {
        json!("unknown")
    } else {
        raw.parse::<u64>()
            .map(Value::from)
            .unwrap_or_else(|_| json!({"kind": "character_estimate", "value": raw}))
    }
}

pub(crate) fn parse_synthetic_index_options(
    args: &[String],
) -> Result<SyntheticIndexOptions, String> {
    let mut output_dir = None;
    let mut files = 250usize;
    let mut storage_budget = storage_budget::StorageBudgetOptions::fixture_smoke();
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--output-dir" | "--output" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--output-dir requires a path".to_string());
                };
                output_dir = Some(PathBuf::from(raw));
            }
            "--files" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--files requires a positive integer".to_string());
                };
                files = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --files value: {raw}"))?;
                if files == 0 {
                    return Err("--files must be greater than zero".to_string());
                }
            }
            value => {
                if storage_budget::parse_storage_budget_flag(args, &mut index, &mut storage_budget)?
                {
                    index += 1;
                    continue;
                }
                return Err(format!("unknown synthetic-index option: {value}"));
            }
        }
        index += 1;
    }

    Ok(SyntheticIndexOptions {
        output_dir: output_dir.ok_or_else(|| {
            "Usage: codegraph-mcp bench synthetic-index --output-dir <dir> [--files <n>]"
                .to_string()
        })?,
        files,
        storage_budget,
    })
}

pub(crate) fn parse_update_integrity_harness_options(
    args: &[String],
) -> Result<UpdateIntegrityHarnessOptions, String> {
    let mut iterations = 20usize;
    let mut autoresearch_iterations = 1usize;
    let mut workers = 4usize;
    let mut medium_files = 48usize;
    let mut mode = UpdateBenchmarkMode::Fast;
    let mut loop_kind = UpdateLoopKind::Combined;
    let mut timeout_ms = None;
    let mut out_json = PathBuf::from("reports")
        .join("audit")
        .join("artifacts")
        .join("autoresearch_update_repro_fix.json");
    let mut out_md = PathBuf::from("reports")
        .join("audit")
        .join("autoresearch_update_repro_fix.md");
    let mut workdir = PathBuf::from("reports")
        .join("audit")
        .join("artifacts")
        .join("update_integrity_harness");
    let mut skip_autoresearch = false;
    let mut only_autoresearch = false;
    let mut autoresearch_repo = std::env::var_os("CODEGRAPH_AUTORESEARCH_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("..").join("autoresearch-codexlab"));
    let mut autoresearch_seed_db = None;
    let mut storage_budget = storage_budget::StorageBudgetOptions::normal_self_use();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--iterations" => {
                index += 1;
                let raw = args
                    .get(index)
                    .ok_or_else(|| "--iterations requires a positive integer".to_string())?;
                iterations = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --iterations value: {raw}"))?;
                if iterations == 0 {
                    return Err("--iterations must be greater than zero".to_string());
                }
            }
            "--autoresearch-iterations" => {
                index += 1;
                let raw = args.get(index).ok_or_else(|| {
                    "--autoresearch-iterations requires a positive integer".to_string()
                })?;
                autoresearch_iterations = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --autoresearch-iterations value: {raw}"))?;
            }
            "--workers" => {
                index += 1;
                let raw = args
                    .get(index)
                    .ok_or_else(|| "--workers requires a positive integer".to_string())?;
                workers = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --workers value: {raw}"))?;
                if workers == 0 {
                    return Err("--workers must be greater than zero".to_string());
                }
            }
            "--medium-files" => {
                index += 1;
                let raw = args
                    .get(index)
                    .ok_or_else(|| "--medium-files requires a positive integer".to_string())?;
                medium_files = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --medium-files value: {raw}"))?;
                if medium_files < 2 {
                    return Err("--medium-files must be at least 2".to_string());
                }
            }
            "--mode" => {
                index += 1;
                let raw = args.get(index).ok_or_else(|| {
                    "--mode requires update-fast, update-validated, or update-debug".to_string()
                })?;
                mode = UpdateBenchmarkMode::parse(raw)?;
            }
            "--loop-kind" | "--loop" => {
                index += 1;
                let raw = args.get(index).ok_or_else(|| {
                    "--loop-kind requires combined, repeat-fast, or update-fast".to_string()
                })?;
                loop_kind = UpdateLoopKind::parse(raw)?;
            }
            "--timeout-ms" => {
                index += 1;
                let raw = args
                    .get(index)
                    .ok_or_else(|| "--timeout-ms requires a positive integer".to_string())?;
                let parsed = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}"))?;
                if parsed == 0 {
                    return Err("--timeout-ms must be greater than zero".to_string());
                }
                timeout_ms = Some(parsed);
            }
            "--out-json" => {
                index += 1;
                out_json = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--out-json requires a path".to_string())?,
                );
            }
            "--out-md" => {
                index += 1;
                out_md = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--out-md requires a path".to_string())?,
                );
            }
            "--workdir" => {
                index += 1;
                workdir = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--workdir requires a path".to_string())?,
                );
            }
            "--skip-autoresearch" => {
                skip_autoresearch = true;
            }
            "--only-autoresearch" => {
                only_autoresearch = true;
            }
            "--autoresearch-repo" => {
                index += 1;
                autoresearch_repo = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--autoresearch-repo requires a path".to_string())?,
                );
            }
            "--autoresearch-seed-db" => {
                index += 1;
                autoresearch_seed_db =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--autoresearch-seed-db requires a path".to_string()
                    })?));
            }
            value => {
                if storage_budget::parse_storage_budget_flag(args, &mut index, &mut storage_budget)?
                {
                    index += 1;
                    continue;
                }
                return Err(format!("unknown update-integrity option: {value}"));
            }
        }
        index += 1;
    }
    if skip_autoresearch && only_autoresearch {
        return Err("--only-autoresearch cannot be combined with --skip-autoresearch".to_string());
    }

    Ok(UpdateIntegrityHarnessOptions {
        iterations,
        autoresearch_iterations,
        workers,
        medium_files,
        mode,
        loop_kind,
        timeout_ms,
        out_json: absolute_cli_path(&out_json)?,
        out_md: absolute_cli_path(&out_md)?,
        workdir: absolute_cli_path(&workdir)?,
        skip_autoresearch,
        only_autoresearch,
        autoresearch_repo: absolute_cli_path(&autoresearch_repo)?,
        autoresearch_seed_db: autoresearch_seed_db
            .as_deref()
            .map(absolute_cli_path)
            .transpose()?,
        storage_budget,
    })
}

pub(crate) fn parse_cgc_comparison_options(
    args: &[String],
) -> Result<CgcComparisonOptions, String> {
    let mut report_dir = default_report_dir();
    let mut timeout_ms = codegraph_bench::competitors::codegraphcontext::DEFAULT_TIMEOUT_MS;
    let mut top_k = codegraph_bench::competitors::codegraphcontext::DEFAULT_TOP_K;
    let mut competitor_executable = None;
    let mut storage_budget = storage_budget::StorageBudgetOptions::normal_self_use();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output-dir" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--output-dir requires a path".to_string());
                };
                report_dir = PathBuf::from(raw);
            }
            "--timeout-ms" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--timeout-ms requires a positive integer".to_string());
                };
                timeout_ms = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}"))?;
                if timeout_ms == 0 {
                    return Err("--timeout-ms must be greater than zero".to_string());
                }
            }
            "--top-k" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--top-k requires a positive integer".to_string());
                };
                top_k = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --top-k value: {raw}"))?;
                if top_k == 0 {
                    return Err("--top-k must be greater than zero".to_string());
                }
            }
            "--competitor-bin" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--competitor-bin requires a path".to_string());
                };
                competitor_executable = Some(PathBuf::from(raw));
            }
            value => {
                if storage_budget::parse_storage_budget_flag(args, &mut index, &mut storage_budget)?
                {
                    index += 1;
                    continue;
                }
                return Err(format!("unknown cgc-comparison option: {value}"));
            }
        }
        index += 1;
    }

    Ok(CgcComparisonOptions {
        report_dir,
        timeout_ms,
        top_k,
        competitor_executable,
        storage_budget,
    })
}

pub(crate) fn parse_gap_scoreboard_options(
    args: &[String],
) -> Result<GapScoreboardOptions, String> {
    let mut report_dir = PathBuf::from("reports").join("phase26-gaps");
    let mut timeout_ms = codegraph_bench::competitors::codegraphcontext::DEFAULT_TIMEOUT_MS;
    let mut top_k = codegraph_bench::competitors::codegraphcontext::DEFAULT_TOP_K;
    let mut competitor_executable = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output-dir" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--output-dir requires a path".to_string());
                };
                report_dir = PathBuf::from(raw);
            }
            "--timeout-ms" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--timeout-ms requires a positive integer".to_string());
                };
                timeout_ms = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}"))?;
                if timeout_ms == 0 {
                    return Err("--timeout-ms must be greater than zero".to_string());
                }
            }
            "--top-k" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--top-k requires a positive integer".to_string());
                };
                top_k = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --top-k value: {raw}"))?;
                if top_k == 0 {
                    return Err("--top-k must be greater than zero".to_string());
                }
            }
            "--competitor-bin" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--competitor-bin requires a path".to_string());
                };
                competitor_executable = Some(PathBuf::from(raw));
            }
            value => return Err(format!("unknown gaps option: {value}")),
        }
        index += 1;
    }

    Ok(GapScoreboardOptions {
        report_dir,
        timeout_ms,
        top_k,
        competitor_executable,
    })
}

pub(crate) fn parse_final_gate_options(
    args: &[String],
) -> Result<FinalAcceptanceGateOptions, String> {
    let mut options =
        FinalAcceptanceGateOptions::with_report_dir(PathBuf::from("reports").join("final-gate"));
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output-dir" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--output-dir requires a path".to_string());
                };
                options.report_dir = PathBuf::from(raw);
            }
            "--workspace-root" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--workspace-root requires a path".to_string());
                };
                options.workspace_root = Some(PathBuf::from(raw));
            }
            "--timeout-ms" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--timeout-ms requires a positive integer".to_string());
                };
                let timeout_ms = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}"))?;
                if timeout_ms == 0 {
                    return Err("--timeout-ms must be greater than zero".to_string());
                }
                options.timeout_ms = timeout_ms.min(codegraph_bench::MAX_BENCH_TASK_MS);
            }
            "--competitor-bin" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--competitor-bin requires a path".to_string());
                };
                options.competitor_executable = Some(PathBuf::from(raw));
            }
            "--cgc-db-size-bytes" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--cgc-db-size-bytes requires an integer".to_string());
                };
                options.cgc_db_size_bytes = Some(
                    raw.parse::<u64>()
                        .map_err(|_| format!("invalid --cgc-db-size-bytes value: {raw}"))?,
                );
            }
            value => return Err(format!("unknown final-gate option: {value}")),
        }
        index += 1;
    }

    Ok(options)
}

#[derive(Debug, Clone)]
pub(crate) struct ContextPackOptions {
    pub(crate) task: String,
    pub(crate) mode: String,
    pub(crate) token_budget: usize,
    pub(crate) seeds: Vec<String>,
    pub(crate) stage0_candidates: Vec<String>,
    pub(crate) profile: bool,
    pub(crate) explain: bool,
    pub(crate) output_mode: QueryOutputMode,
    pub(crate) limit_paths: Option<usize>,
    pub(crate) limit_snippets: Option<usize>,
    pub(crate) max_output_bytes: Option<usize>,
    pub(crate) allow_stale_read: bool,
    pub(crate) allow_foreign_db: bool,
    pub(crate) explicit_scope_policy: Option<IndexScopeOptions>,
    pub(crate) enable_vector_candidates: bool,
    pub(crate) enable_nuance_rescue_candidates: bool,
    pub(crate) vector_index_path: Option<PathBuf>,
    pub(crate) vector_audit_artifact_path: Option<PathBuf>,
    pub(crate) enable_candidate_spool: bool,
    pub(crate) candidate_spool_path: Option<PathBuf>,
    pub(crate) allow_stale_candidate_spool: bool,
}

pub(crate) fn parse_context_pack_args(args: &[String]) -> Result<ContextPackOptions, String> {
    let mut args = args.to_vec();
    let explicit_scope_policy = parse_read_scope_options(&mut args)?;
    let mut task = None;
    let mut mode = "impact".to_string();
    let mut token_budget = 2_000usize;
    let mut seeds = Vec::new();
    let mut stage0_candidates = Vec::new();
    let mut profile = false;
    let mut explain = false;
    let mut output_mode = QueryOutputMode::RichJson;
    let mut limit_paths = None;
    let mut limit_snippets = None;
    let mut max_output_bytes = None;
    let mut allow_stale_read = false;
    let mut allow_foreign_db = false;
    let mut enable_vector_candidates = false;
    let mut enable_nuance_rescue_candidates = false;
    let mut vector_index_path = None;
    let mut vector_audit_artifact_path = None;
    let mut enable_candidate_spool = false;
    let mut candidate_spool_path = None;
    let mut allow_stale_candidate_spool = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--task" => {
                index += 1;
                task = args.get(index).cloned();
            }
            "--budget" | "--token-budget" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--budget requires a value".to_string());
                };
                token_budget = value
                    .parse::<usize>()
                    .map_err(|_| "--budget must be an integer".to_string())?;
            }
            "--mode" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--mode requires a value".to_string());
                };
                mode = value.clone();
            }
            "--seed" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--seed requires a value".to_string());
                };
                seeds.push(value.clone());
            }
            "--candidate" | "--stage0-candidate" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--candidate requires a value".to_string());
                };
                stage0_candidates.push(value.clone());
            }
            "--profile" => {
                profile = true;
            }
            "--explain" => {
                explain = true;
                profile = true;
                output_mode = QueryOutputMode::AgentJson;
            }
            "--audit-json" | "--audit_json" => {
                explain = true;
                profile = true;
                output_mode = QueryOutputMode::AgentJson;
            }
            "--agent-json" | "--agent_json" => {
                output_mode = QueryOutputMode::AgentJson;
            }
            "--concise" => {
                if output_mode != QueryOutputMode::AgentJson {
                    output_mode = QueryOutputMode::Concise;
                }
            }
            "--limit-paths" | "--limit_paths" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--limit-paths requires a value".to_string());
                };
                limit_paths = Some(parse_context_pack_limit(
                    value,
                    "--limit-paths",
                    MAX_CONTEXT_AGENT_PATH_LIMIT,
                )?);
            }
            "--limit-snippets" | "--limit_snippets" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--limit-snippets requires a value".to_string());
                };
                limit_snippets = Some(parse_context_pack_limit(
                    value,
                    "--limit-snippets",
                    MAX_CONTEXT_AGENT_SNIPPET_LIMIT,
                )?);
            }
            "--max-output-bytes" | "--max-bytes" | "--max_output_bytes" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--max-output-bytes requires a value".to_string());
                };
                max_output_bytes = Some(parse_context_pack_max_output_bytes(value)?);
            }
            "--verbose" => {
                profile = true;
            }
            "--debug" => {
                mode = "debug".to_string();
                profile = true;
                explain = true;
            }
            "--allow-stale-read" => {
                allow_stale_read = true;
            }
            "--allow-foreign-db" => {
                allow_foreign_db = true;
            }
            "--enable-vector-candidates" | "--enable_vector_candidates" => {
                enable_vector_candidates = true;
            }
            "--enable-candidate-spool"
            | "--enable_candidate_spool"
            | "--early-candidates"
            | "--early_candidates" => {
                enable_candidate_spool = true;
            }
            "--candidate-spool" | "--candidate_spool" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--candidate-spool requires a path".to_string());
                };
                candidate_spool_path = Some(PathBuf::from(value));
                enable_candidate_spool = true;
            }
            "--allow-stale-candidate-spool"
            | "--allow_stale_candidate_spool"
            | "--diagnostic-candidate-spool" => {
                allow_stale_candidate_spool = true;
            }
            "--enable-nuance-rescue-candidates" | "--enable_nuance_rescue_candidates" => {
                enable_nuance_rescue_candidates = true;
            }
            "--vector-index"
            | "--vector-index-path"
            | "--vector_index"
            | "--vector_index_path"
            | "--vector-runtime-sidecar"
            | "--vector_runtime_sidecar" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--vector-index requires a path".to_string());
                };
                vector_index_path = Some(PathBuf::from(value));
            }
            "--vector-audit-artifact" | "--vector_audit_artifact" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--vector-audit-artifact requires a path".to_string());
                };
                vector_audit_artifact_path = Some(PathBuf::from(value));
            }
            other => return Err(format!("unknown context-pack option: {other}")),
        }
        index += 1;
    }
    let Some(task) = task else {
        return Err(
            "Usage: codegraph-mcp context-pack --task <task> [--budget <tokens>] [--mode <mode>]"
                .to_string(),
        );
    };
    Ok(ContextPackOptions {
        task,
        mode,
        token_budget,
        seeds,
        stage0_candidates,
        profile,
        explain,
        output_mode,
        limit_paths,
        limit_snippets,
        max_output_bytes,
        allow_stale_read,
        allow_foreign_db,
        explicit_scope_policy,
        enable_vector_candidates,
        enable_nuance_rescue_candidates,
        vector_index_path,
        vector_audit_artifact_path,
        enable_candidate_spool,
        candidate_spool_path,
        allow_stale_candidate_spool,
    })
}

pub(crate) fn parse_context_pack_limit(
    value: &str,
    flag: &str,
    max_limit: usize,
) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map(|limit| limit.clamp(1, max_limit))
        .map_err(|_| format!("{flag} must be an integer"))
}

pub(crate) fn parse_context_pack_max_output_bytes(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map(|bytes| {
            bytes.clamp(
                MIN_CONTEXT_AGENT_MAX_OUTPUT_BYTES,
                MAX_CONTEXT_AGENT_MAX_OUTPUT_BYTES,
            )
        })
        .map_err(|_| "--max-output-bytes must be an integer".to_string())
}

pub(crate) fn parse_output_arg(args: &[String]) -> Result<PathBuf, String> {
    if args.len() == 2 && args[0] == "--output" {
        return Ok(PathBuf::from(&args[1]));
    }
    Err("Usage: codegraph-mcp bundle export --output repo.cgc-bundle".to_string())
}

#[derive(Debug, Clone)]
pub(crate) struct StatusOptions {
    pub(crate) repo: PathBuf,
    pub(crate) candidate_spool_path: Option<PathBuf>,
    pub(crate) vector_runtime_path: Option<PathBuf>,
    pub(crate) vector_audit_path: Option<PathBuf>,
}

pub(crate) fn parse_status_args(args: &[String]) -> Result<StatusOptions, String> {
    let mut repo = None;
    let mut candidate_spool_path = None;
    let mut vector_runtime_path = None;
    let mut vector_audit_path = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {}
            "--candidate-spool" | "--candidate_spool" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--candidate-spool requires a path".to_string());
                };
                candidate_spool_path = Some(PathBuf::from(value));
            }
            "--vector-runtime-sidecar"
            | "--vector_runtime_sidecar"
            | "--vector-index"
            | "--vector_index" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--vector-runtime-sidecar requires a path".to_string());
                };
                vector_runtime_path = Some(PathBuf::from(value));
            }
            "--vector-audit-artifact" | "--vector_audit_artifact" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--vector-audit-artifact requires a path".to_string());
                };
                vector_audit_path = Some(PathBuf::from(value));
            }
            value if value.starts_with('-') => {
                if let Some(error) = misplaced_global_flag_error(value, "status") {
                    return Err(error);
                }
                return Err(format!("unknown status option: {value}"));
            }
            value => {
                if repo.is_some() {
                    return Err(
                        "Usage: codegraph-mcp status [repo] [--json] [--candidate-spool <path>] [--vector-runtime-sidecar <path>] [--vector-audit-artifact <path>]".to_string(),
                    );
                }
                repo = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    Ok(StatusOptions {
        repo: repo.unwrap_or_else(|| PathBuf::from(".")),
        candidate_spool_path,
        vector_runtime_path,
        vector_audit_path,
    })
}
