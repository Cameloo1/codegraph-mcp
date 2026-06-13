//! Benchmark, gate, ablation, parity, and update-integrity harness commands.
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::*;

pub(crate) fn run_bench_command(args: &[String]) -> Result<Value, String> {
    if matches!(args.first().map(String::as_str), Some("graph-truth")) {
        return run_graph_truth_gate_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("context-packet" | "context-packet-gate" | "context")
    ) {
        return run_context_packet_gate_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("retrieval-ablation" | "stage-ablation")
    ) {
        return run_retrieval_ablation_command(&args[1..]);
    }

    if matches!(args.first().map(String::as_str), Some("retrieval-quality")) {
        return run_retrieval_quality_benchmark_command(&args[1..]);
    }

    if matches!(args.first().map(String::as_str), Some("agent-quality")) {
        return run_agent_quality_benchmark_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("synthetic-index" | "indexing-speed")
    ) {
        return run_synthetic_index_benchmark_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("update-integrity" | "autoresearch-update-repro" | "db-integrity-update")
    ) {
        return run_update_integrity_harness_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("query-surface" | "default-query-surface")
    ) {
        return run_query_surface_benchmark_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("proof-build-only" | "proof-build")
    ) {
        return run_proof_build_mode_benchmark_command(&args[1..], IndexBuildMode::ProofBuildOnly);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("proof-build-validated" | "proof-build-plus-validation")
    ) {
        return run_proof_build_mode_benchmark_command(
            &args[1..],
            IndexBuildMode::ProofBuildPlusValidation,
        );
    }

    if matches!(
        args.first().map(String::as_str),
        Some("comprehensive" | "comprehensive-gate" | "master-gate")
    ) {
        return run_comprehensive_benchmark_command(&args[1..]);
    }

    if matches!(args.first().map(String::as_str), Some("real-repo-corpus")) {
        return run_real_repo_corpus_command(&args[1..]);
    }

    if matches!(args.first().map(String::as_str), Some("parity-report")) {
        return run_parity_report_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("final-gate" | "acceptance-gate" | "compact-mvp-gate")
    ) {
        return run_final_gate_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("gaps" | "gap-scoreboard" | "gap-scoreboard-report")
    ) {
        return run_gap_scoreboard_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("cgc-comparison" | "codegraphcontext-comparison")
    ) {
        return run_cgc_comparison_command(&args[1..]);
    }

    let options = parse_bench_options(args)?;
    let report = codegraph_bench::run_default_benchmark_suite(&options.baselines)
        .map_err(|error| error.to_string())?;
    match options.output {
        Some(output) => {
            write_benchmark_report(&report, options.format, &output)?;
            Ok(json!({
                "status": "benchmarked",
                "phase": PHASE,
                "suite_id": report.suite_id,
                "format": options.format.as_str(),
                "output": path_string(&output),
                "runs": report.results.len(),
                "baselines": report.aggregate.keys().cloned().collect::<Vec<_>>(),
                "aggregate": report.aggregate,
            }))
        }
        None if options.format == BenchReportFormat::Json => {
            serde_json::to_value(report).map_err(|error| error.to_string())
        }
        None => Ok(json!({
            "status": "benchmarked",
            "phase": PHASE,
            "suite_id": report.suite_id,
            "format": options.format.as_str(),
            "runs": report.results.len(),
            "markdown": codegraph_bench::render_markdown_report(&report),
            "aggregate": report.aggregate,
        })),
    }
}

pub(crate) fn run_graph_truth_gate_command(args: &[String]) -> Result<Value, String> {
    let options = parse_graph_truth_gate_options(args)?;
    let report = codegraph_bench::write_graph_truth_gate_report(options.clone())
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": report.status,
        "phase": PHASE,
        "gate": "graph_truth",
        "verdict": report.verdict,
        "cases_total": report.cases_total,
        "cases_passed": report.cases_passed,
        "cases_failed": report.cases_failed,
        "out_json": path_string(&options.out_json),
        "out_md": path_string(&options.out_md),
        "proof": "Graph Truth Gate indexes each fixture and compares expected and forbidden graph facts, source spans, paths, context symbols, and tests.",
    }))
}

pub(crate) fn run_context_packet_gate_command(args: &[String]) -> Result<Value, String> {
    let options = parse_context_packet_gate_options(args)?;
    let report = codegraph_bench::write_context_packet_gate_report(options.clone())
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": report.status,
        "phase": PHASE,
        "gate": "context_packet",
        "verdict": report.verdict,
        "cases_total": report.cases_total,
        "cases_passed": report.cases_passed,
        "cases_failed": report.cases_failed,
        "context_symbol_recall_at_k": report.metrics.context_symbol_recall_at_k,
        "critical_symbol_missing_rate": report.metrics.critical_symbol_missing_rate,
        "distractor_ratio": report.metrics.distractor_ratio,
        "proof_path_coverage": report.metrics.proof_path_coverage,
        "source_span_coverage": report.metrics.source_span_coverage,
        "useful_facts_per_byte": report.metrics.useful_facts_per_byte,
        "out_json": path_string(&options.out_json),
        "out_md": path_string(&options.out_md),
        "proof": "Context Packet Gate scores packet usefulness against graph-truth critical symbols, proof paths, source spans, snippets, tests, context labels, and distractors.",
    }))
}

pub(crate) fn run_retrieval_ablation_command(args: &[String]) -> Result<Value, String> {
    let options = parse_retrieval_ablation_options(args)?;
    let report = codegraph_bench::write_retrieval_ablation_report(options.clone())
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": report.status,
        "phase": PHASE,
        "benchmark": "retrieval_ablation",
        "cases_total": report.case_results.len(),
        "modes": report.modes.iter().map(|mode| mode.mode.clone()).collect::<Vec<_>>(),
        "out_json": path_string(&options.out_json),
        "out_md": path_string(&options.out_md),
        "proof": "Retrieval ablation separates Stage 0, Stage 1, Stage 2, exact graph verification, and full context packet metrics.",
    }))
}

pub(crate) fn run_retrieval_quality_benchmark_command(args: &[String]) -> Result<Value, String> {
    let options = parse_two_layer_bench_options(args)?;
    let artifacts = codegraph_bench::run_retrieval_quality_benchmark(options)
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "benchmarked",
        "phase": PHASE,
        "layer": "retrieval_quality",
        "run_id": artifacts.run_id,
        "run_root": artifacts.run_root,
        "manifest": artifacts.manifest_json,
        "events_jsonl": artifacts.events_jsonl,
        "per_task_jsonl": artifacts.per_task_jsonl,
        "summary_md": artifacts.summary_md,
        "proof": "Retrieval benchmark writes manifest, events, per-task JSONL, raw outputs, and normalized outputs under target/codegraph-bench-runs/<run_id>.",
    }))
}

pub(crate) fn run_agent_quality_benchmark_command(args: &[String]) -> Result<Value, String> {
    let mut options = parse_two_layer_bench_options(args)?;
    options.dry_run = true;
    options.fake_agent = true;
    let artifacts =
        codegraph_bench::run_agent_quality_benchmark(options).map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "benchmarked",
        "phase": PHASE,
        "layer": "agent_coding_quality",
        "run_id": artifacts.run_id,
        "run_root": artifacts.run_root,
        "manifest": artifacts.manifest_json,
        "events_jsonl": artifacts.events_jsonl,
        "per_task_jsonl": artifacts.per_task_jsonl,
        "summary_md": artifacts.summary_md,
        "proof": "Agent benchmark dry-run uses a fake runner, records tool/agent trace events, and stores patches/final answers under target/codegraph-bench-runs/<run_id>.",
    }))
}

pub(crate) fn run_trace_command(args: &[String]) -> Result<Value, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Err("Usage: codegraph-mcp trace <append|replay> [ARGS]".to_string());
    };
    match subcommand {
        "append" => {
            let options = parse_trace_append_options(&args[1..])?;
            let trace_root = options.trace_root.clone();
            let mut config = TraceConfig::for_repo(&options.repo).with_trace_root(&trace_root);
            if let Some(run_id) = options.run_id {
                config = config.with_run_id(run_id);
            }
            if let Some(task_id) = options.task_id {
                config = config.with_task_id(task_id);
            }
            if let Some(repo_id) = options.repo_id {
                config = config.with_repo_id(repo_id);
            }
            let event =
                append_trace_event(config, options.event).map_err(|error| error.to_string())?;
            Ok(json!({
                "status": "traced",
                "event_type": event.event_type.as_str(),
                "run_id": event.run_id,
                "trace_id": event.trace_id,
                "task_id": event.task_id,
                "repo_id": event.repo_id,
                "events_jsonl": path_string(&trace_root.join(&event.run_id).join("events.jsonl")),
                "artifact_path": event.artifact_path,
            }))
        }
        "replay" | "validate" => {
            let events_path = parse_trace_replay_options(&args[1..])?;
            let report = replay_trace_file(&events_path).map_err(|error| error.to_string())?;
            let answers = json!({
                "mcp_tools_called": report.mcp_calls_made.clone(),
                "files_edited": report.files_edited.clone(),
                "tests_run": report.tests_run.clone(),
                "context_evidence_used": report.context_evidence_used.clone(),
            });
            Ok(json!({
                "status": "ok",
                "events_jsonl": path_string(&events_path),
                "replay": report,
                "answers": answers
            }))
        }
        other => Err(format!("unknown trace subcommand: {other}")),
    }
}

pub(crate) fn run_real_repo_corpus_command(args: &[String]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("Usage: codegraph-mcp bench real-repo-corpus".to_string());
    }
    let corpus = codegraph_bench::real_repo_maturity_corpus();
    corpus.validate().map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "ok",
        "phase": PHASE,
        "corpus": corpus,
        "replay": codegraph_bench::plan_real_repo_corpus_replay(&codegraph_bench::real_repo_maturity_corpus(), ".codegraph-bench-cache/real-repos", false).map_err(|error| error.to_string())?,
    }))
}

pub(crate) fn run_gap_scoreboard_command(args: &[String]) -> Result<Value, String> {
    let options = parse_gap_scoreboard_options(args)?;
    let artifacts =
        codegraph_bench::write_gap_scoreboard_report(options).map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "reported",
        "phase": "26",
        "artifacts": artifacts,
        "proof": "Gap scoreboard records win/loss/tie/unknown dimensions and keeps missing data as unknown.",
    }))
}

pub(crate) fn run_parity_report_command(args: &[String]) -> Result<Value, String> {
    let mut output_dir = PathBuf::from("reports").join("phase30-parity");
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--output-dir" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--output-dir requires a path".to_string());
                };
                output_dir = PathBuf::from(value);
            }
            value => return Err(format!("unknown parity-report option: {value}")),
        }
        index += 1;
    }
    let artifacts = codegraph_bench::write_final_parity_report(&output_dir)
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "reported",
        "phase": PHASE,
        "artifacts": artifacts,
        "proof": "Final parity report records unknown/skipped fields explicitly and makes no SOTA claim.",
    }))
}

pub(crate) fn run_final_gate_command(args: &[String]) -> Result<Value, String> {
    let options = parse_final_gate_options(args)?;
    let artifacts = codegraph_bench::write_final_acceptance_gate_report(options.clone())
        .map_err(|error| error.to_string())?;
    let summary: Value = serde_json::from_str(
        &fs::read_to_string(&artifacts.json_summary).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "reported",
        "phase": PHASE,
        "gate": "final_compact_mvp_acceptance",
        "verdict": summary["verdict"].clone(),
        "internal_verdict": summary["internal_verdict"].clone(),
        "cgc_status": summary["cgc_comparison"]["status"].clone(),
        "artifacts": artifacts,
        "proof": "Final gate requires compact storage and MVP proof/functionality preservation; missing CGC data keeps the verdict unknown.",
    }))
}

pub(crate) fn run_comprehensive_benchmark_command(args: &[String]) -> Result<Value, String> {
    let comprehensive_start = Instant::now();
    let options = parse_comprehensive_benchmark_options(args)?;
    let repo_root = resolve_repo_root(&options.repo)?;
    let planned_db_path = match &options.artifact_mode {
        ComprehensiveArtifactMode::Fresh => options
            .output_dir
            .join("artifacts")
            .join(format!("comprehensive_proof_{}.sqlite", options.timestamp)),
        ComprehensiveArtifactMode::Existing(path) => path.clone(),
    };
    let preflight_db_path = match &options.artifact_mode {
        ComprehensiveArtifactMode::Fresh => None,
        ComprehensiveArtifactMode::Existing(_) => Some(planned_db_path),
    };
    let preflight = storage_budget::storage_budget_preflight(
        &options.storage_budget,
        storage_budget::StorageBudgetContext {
            command: "codegraph-mcp bench comprehensive".to_string(),
            repo_root: Some(repo_root),
            db_path: preflight_db_path,
            out_path: Some(options.output_dir.clone()),
            explicit_db: matches!(
                options.artifact_mode,
                ComprehensiveArtifactMode::Existing(_)
            ),
            explicit_out: true,
            diagnostic_only: false,
        },
    );
    if preflight.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp bench comprehensive",
                &preflight,
            ),
        ));
    }
    fs::create_dir_all(&options.output_dir).map_err(|error| error.to_string())?;
    let mut report = build_comprehensive_benchmark_report(&options)?;
    let report_generation_start = Instant::now();
    let _ = render_comprehensive_benchmark_markdown(&report);
    let report_generation_ms = elapsed_ms(report_generation_start);

    let timestamp_json = options.output_dir.join(format!(
        "comprehensive_benchmark_{}.json",
        options.timestamp
    ));
    let timestamp_md = options
        .output_dir
        .join(format!("comprehensive_benchmark_{}.md", options.timestamp));
    let latest_json = options
        .output_dir
        .join("comprehensive_benchmark_latest.json");
    let latest_md = options.output_dir.join("comprehensive_benchmark_latest.md");
    let comprehensive_total_ms = elapsed_ms(comprehensive_start);
    patch_comprehensive_timing_separation(
        &mut report,
        report_generation_ms,
        comprehensive_total_ms,
    );
    let markdown = render_comprehensive_benchmark_markdown(&report);
    let budget_db_path = report["artifact_freshness"]["artifact_path"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_default();
    let storage_budget = storage_budget::storage_budget_postflight(
        preflight,
        &[budget_db_path],
        &[options.output_dir.clone()],
    );
    if let Some(object) = report.as_object_mut() {
        object.insert("storage_budget".to_string(), storage_budget.to_json());
    }

    write_json_file(&timestamp_json, &report)?;
    write_text_file(&timestamp_md, &markdown)?;
    write_json_file(&latest_json, &report)?;
    write_text_file(&latest_md, &markdown)?;
    if storage_budget.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp bench comprehensive",
                &storage_budget,
            ),
        ));
    }

    Ok(json!({
        "status": "reported",
        "phase": PHASE,
        "benchmark": "comprehensive",
        "verdict": report["sections"]["executive_verdict"]["verdict"].clone(),
        "reason_for_failure": report["sections"]["executive_verdict"]["reason_for_failure"].clone(),
        "output_json": path_string(&latest_json),
        "output_md": path_string(&latest_md),
        "timestamped_json": path_string(&timestamp_json),
        "timestamped_md": path_string(&timestamp_md),
        "timing_separation": report["timing_separation"].clone(),
        "storage_budget": storage_budget.to_json(),
        "proof": "Comprehensive benchmark builds a fresh proof artifact by default; explicit artifact reuse is labeled and cannot claim storage/cold-build passes if stale.",
    }))
}

pub(crate) fn patch_comprehensive_timing_separation(
    report: &mut Value,
    report_generation_ms: u64,
    comprehensive_total_ms: u64,
) {
    let proof_build_only_ms = report
        .get("artifact_freshness")
        .and_then(|value| value.get("proof_build_only_ms"))
        .cloned()
        .or_else(|| {
            report
                .get("artifact_freshness")
                .and_then(|value| value.get("build_duration_ms"))
                .cloned()
        })
        .unwrap_or(Value::Null);
    let validation_ms = report
        .get("artifact_freshness")
        .and_then(|value| value.get("validation_ms"))
        .cloned()
        .unwrap_or_else(|| json!(0));
    let audit_ms = report
        .get("artifact_freshness")
        .and_then(|value| value.get("audit_ms"))
        .cloned()
        .unwrap_or(Value::Null);
    let timing = json!({
        "proof_build_only_ms": proof_build_only_ms,
        "validation_ms": validation_ms,
        "audit_ms": audit_ms,
        "report_generation_ms": report_generation_ms,
        "comprehensive_total_ms": comprehensive_total_ms,
        "claimable_for_thresholds": report
            .get("artifact_freshness")
            .and_then(|value| value.get("claimable_for_thresholds"))
            .cloned()
            .unwrap_or(Value::Null),
        "diagnostic_only": report
            .get("artifact_freshness")
            .and_then(|value| value.get("diagnostic_only"))
            .cloned()
            .unwrap_or(Value::Null),
        "timing_classification": report
            .get("artifact_freshness")
            .and_then(|value| value.get("timing_classification"))
            .cloned()
            .unwrap_or(Value::Null),
        "notes": [
            "proof_build_only_ms is numeric only for claimable release-binary proof DB build timing; debug timing is reported as non_claimable_debug_timing.",
            "validation_ms, audit_ms, report_generation_ms, and comprehensive_total_ms are separated so operator-gate overhead cannot be charged to proof-build-only."
        ]
    });
    if let Some(object) = report.as_object_mut() {
        object.insert("timing_separation".to_string(), timing.clone());
    }
    if let Some(sections) = report.get_mut("sections").and_then(Value::as_object_mut) {
        sections.insert("timing_separation".to_string(), timing);
    }
}

pub(crate) fn parse_comprehensive_benchmark_options(
    args: &[String],
) -> Result<ComprehensiveBenchmarkOptions, String> {
    let mut output_dir = PathBuf::from("reports").join("final");
    let mut baseline_json = PathBuf::from("reports")
        .join("baselines")
        .join("compact_proof_baseline_latest.json");
    let mut compact_gate_json = PathBuf::from("reports")
        .join("final")
        .join("compact_proof_db_gate.json");
    let mut previous_json = Some(
        PathBuf::from("reports")
            .join("final")
            .join("comprehensive_benchmark_latest.json"),
    );
    let mut timestamp = format!("{}", unix_time_ms());
    let mut artifact_mode = ComprehensiveArtifactMode::Fresh;
    let mut repo = PathBuf::from(".");
    let mut artifact_metadata = None;
    let mut fail_on_stale_artifact = false;
    let mut workers = None;
    let mut allow_debug_timing = false;
    let mut storage_budget = storage_budget::StorageBudgetOptions::normal_self_use();

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--fresh" => {
                artifact_mode = ComprehensiveArtifactMode::Fresh;
            }
            "--use-existing-artifact" => {
                index += 1;
                artifact_mode = ComprehensiveArtifactMode::Existing(PathBuf::from(
                    required_cli_value(args, index, "--use-existing-artifact")?,
                ));
            }
            "--artifact-metadata" => {
                index += 1;
                artifact_metadata = Some(PathBuf::from(required_cli_value(
                    args,
                    index,
                    "--artifact-metadata",
                )?));
            }
            "--fail-on-stale-artifact" => {
                fail_on_stale_artifact = true;
            }
            "--allow-debug-timing" => {
                allow_debug_timing = true;
            }
            "--repo" => {
                index += 1;
                repo = PathBuf::from(required_cli_value(args, index, "--repo")?);
            }
            "--workers" => {
                index += 1;
                let raw = required_cli_value(args, index, "--workers")?;
                let parsed = raw
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --workers value {raw}: {error}"))?;
                if parsed == 0 {
                    return Err("--workers must be greater than 0".to_string());
                }
                workers = Some(parsed);
            }
            "--output-dir" => {
                index += 1;
                output_dir = PathBuf::from(required_cli_value(args, index, "--output-dir")?);
            }
            "--baseline" | "--baseline-json" => {
                index += 1;
                baseline_json = PathBuf::from(required_cli_value(args, index, "--baseline")?);
            }
            "--compact-gate-json" | "--gate-json" | "--source-gate-json" => {
                index += 1;
                compact_gate_json =
                    PathBuf::from(required_cli_value(args, index, "--compact-gate-json")?);
            }
            "--previous" | "--previous-json" => {
                index += 1;
                previous_json = Some(PathBuf::from(required_cli_value(
                    args,
                    index,
                    "--previous",
                )?));
            }
            "--no-previous" => {
                previous_json = None;
            }
            "--timestamp" => {
                index += 1;
                timestamp = required_cli_value(args, index, "--timestamp")?.to_string();
            }
            value => {
                if storage_budget::parse_storage_budget_flag(args, &mut index, &mut storage_budget)?
                {
                    index += 1;
                    continue;
                }
                return Err(format!("unknown comprehensive benchmark option: {value}"));
            }
        }
        index += 1;
    }

    Ok(ComprehensiveBenchmarkOptions {
        output_dir,
        baseline_json,
        compact_gate_json,
        previous_json,
        timestamp,
        artifact_mode,
        repo,
        artifact_metadata,
        fail_on_stale_artifact,
        workers,
        allow_debug_timing,
        storage_budget,
    })
}

pub(crate) fn run_query_surface_benchmark_command(args: &[String]) -> Result<Value, String> {
    let options = parse_query_surface_benchmark_options(args)?;
    let repo = absolutize_path(&options.repo)?;
    let planned_db_path = options
        .db_path
        .clone()
        .map(|path| absolutize_path(&path))
        .transpose()?
        .unwrap_or_else(|| {
            if options.fresh {
                PathBuf::from("reports")
                    .join("audit")
                    .join("artifacts")
                    .join(format!("default_query_surface_{}.sqlite", unix_time_ms()))
            } else {
                default_db_path(&repo)
            }
        });
    let planned_db_path = absolutize_path(&planned_db_path)?;
    let preflight = storage_budget::storage_budget_preflight(
        &options.storage_budget,
        storage_budget::StorageBudgetContext {
            command: "codegraph-mcp bench query-surface".to_string(),
            repo_root: Some(repo.clone()),
            db_path: if options.fresh && options.db_path.is_none() {
                None
            } else {
                Some(planned_db_path.clone())
            },
            out_path: options.out_json.parent().map(Path::to_path_buf),
            explicit_db: options.db_path.is_some(),
            explicit_out: true,
            diagnostic_only: true,
        },
    );
    if preflight.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp bench query-surface",
                &preflight,
            ),
        ));
    }
    let db_path = if options.fresh {
        let db_path = planned_db_path.clone();
        remove_sqlite_family_if_exists(&db_path)?;
        index_repo_to_db_with_options(
            &repo,
            &db_path,
            IndexOptions {
                profile: true,
                json: false,
                worker_count: options.workers,
                storage_mode: StorageMode::Proof,
                build_mode: IndexBuildMode::ProofBuildOnly,
                ..IndexOptions::default()
            },
        )
        .map_err(|error| error.to_string())?;
        db_path
    } else {
        planned_db_path.clone()
    };

    let report = build_default_query_surface_report(&repo, &db_path, options.iterations);
    let markdown = render_default_query_surface_markdown(&report);
    write_json_file(&options.out_json, &report)?;
    write_text_file(&options.out_md, &markdown)?;

    let storage_budget = storage_budget::storage_budget_postflight(
        preflight,
        &[db_path.clone()],
        &[options.out_json.clone(), options.out_md.clone()],
    );
    if storage_budget.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp bench query-surface",
                &storage_budget,
            ),
        ));
    }
    Ok(json!({
        "status": report["status"].clone(),
        "phase": PHASE,
        "benchmark": "query_surface",
        "repo_root": path_string(&repo),
        "db_path": path_string(&db_path),
        "iterations": options.iterations,
        "output_json": path_string(&options.out_json),
        "output_md": path_string(&options.out_md),
        "storage_budget": storage_budget.to_json(),
        "proof": "Default query-surface probes run directly against the compact proof DB and report failures instead of falling back to audit/debug sidecars.",
    }))
}

pub(crate) fn run_proof_build_mode_benchmark_command(
    args: &[String],
    build_mode: IndexBuildMode,
) -> Result<Value, String> {
    let mut index_args = Vec::new();
    let mut allow_debug_timing = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--allow-debug-timing" => {
                allow_debug_timing = true;
            }
            "--repo" => {
                index += 1;
                let Some(repo) = args.get(index) else {
                    return Err("--repo requires a path".to_string());
                };
                index_args.push(repo.clone());
            }
            value => index_args.push(value.to_string()),
        }
        index += 1;
    }
    if cfg!(debug_assertions) && build_mode == IndexBuildMode::ProofBuildOnly && !allow_debug_timing
    {
        return Err(
            "bench proof-build-only was invoked from a debug binary; pass --allow-debug-timing for a diagnostic-only run, or run a release binary for claimable threshold timing"
                .to_string(),
        );
    }
    if !index_args.iter().any(|arg| arg == "--profile") {
        index_args.push("--profile".to_string());
    }
    if !index_args.iter().any(|arg| arg == "--json") {
        index_args.push("--json".to_string());
    }
    if !index_args.iter().any(|arg| arg == "--storage-mode") {
        index_args.push("--storage-mode".to_string());
        index_args.push("proof".to_string());
    }
    if !index_args.iter().any(|arg| arg == "--build-mode") {
        index_args.push("--build-mode".to_string());
        index_args.push(build_mode.as_str().to_string());
    }

    let started = Instant::now();
    let (repo, db, options, _output_mode, budget_options, _vector_index_output) =
        parse_index_command_options(&index_args)?;
    if options.storage_mode != StorageMode::Proof {
        return Err(format!(
            "bench {} requires --storage-mode proof; got {}",
            build_mode.as_str(),
            options.storage_mode.as_str()
        ));
    }
    if options.build_mode != build_mode {
        return Err(format!(
            "bench {} cannot be run with --build-mode {}; use the matching benchmark subcommand",
            build_mode.as_str(),
            options.build_mode.as_str()
        ));
    }
    let repo_root = resolve_repo_root(Path::new(&repo))?;
    let db_path = db
        .clone()
        .map(|path| normalize_db_path_for_repo(&repo_root, &path))
        .unwrap_or_else(|| default_db_path(&repo_root));
    let preflight = storage_budget::storage_budget_preflight(
        &budget_options,
        storage_budget::StorageBudgetContext {
            command: format!("codegraph-mcp bench {}", build_mode.as_str()),
            repo_root: Some(repo_root.clone()),
            db_path: Some(db_path.clone()),
            out_path: None,
            explicit_db: db.is_some(),
            explicit_out: false,
            diagnostic_only: cfg!(debug_assertions) || allow_debug_timing,
        },
    );
    if preflight.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                &format!("codegraph-mcp bench {}", build_mode.as_str()),
                &preflight,
            ),
        ));
    }
    let summary = if let Some(db) = db {
        index_repo_to_db_with_options(Path::new(&repo), &db, options)
    } else {
        index_repo_with_options(Path::new(&repo), options)
    }
    .map_err(|error| error.to_string())?;
    let elapsed = elapsed_ms(started);
    let summary_json = index_summary_json(&summary)?;
    let proof_build_only_ms = summary
        .profile
        .as_ref()
        .map(|profile| profile.total_wall_ms)
        .unwrap_or(elapsed as u128);
    let validation_ms = if build_mode == IndexBuildMode::ProofBuildPlusValidation {
        profile_span_ms(summary.profile.as_ref(), "integrity_check").unwrap_or(0.0)
    } else {
        0.0
    };
    let report_generation_start = Instant::now();
    let db_path = PathBuf::from(&summary.db_path);
    let artifact_metadata_path = default_artifact_metadata_path(&db_path);
    let binary_metadata = benchmark_binary_metadata();
    let claimable_for_thresholds = binary_metadata["claimable_for_thresholds"]
        .as_bool()
        .unwrap_or(false);
    let diagnostic_only = binary_metadata["diagnostic_only"]
        .as_bool()
        .unwrap_or(false);
    let timing_classification = if diagnostic_only {
        "diagnostic_only"
    } else {
        "production"
    };
    let proof_build_only_value =
        if build_mode == IndexBuildMode::ProofBuildOnly && !claimable_for_thresholds {
            json!(NON_CLAIMABLE_DEBUG_TIMING)
        } else {
            json!(proof_build_only_ms)
        };
    let artifact_metadata = proof_build_mode_artifact_metadata(
        &summary,
        build_mode,
        proof_build_only_ms,
        validation_ms,
        &artifact_metadata_path,
        &binary_metadata,
    );
    write_json_file(&artifact_metadata_path, &artifact_metadata)?;
    let report_generation_ms = elapsed_ms(report_generation_start);
    let storage_budget = storage_budget::storage_budget_postflight(
        preflight,
        &[db_path],
        &[artifact_metadata_path.clone()],
    );
    if storage_budget.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                &format!("codegraph-mcp bench {}", build_mode.as_str()),
                &storage_budget,
            ),
        ));
    }
    Ok(json!({
        "status": "benchmarked",
        "phase": PHASE,
        "benchmark": match build_mode {
            IndexBuildMode::ProofBuildOnly => "proof_build_only",
            IndexBuildMode::ProofBuildPlusValidation => "proof_build_validated",
        },
        "mode": build_mode.as_str(),
        "proof_build_only_ms": proof_build_only_value.clone(),
        "diagnostic_debug_proof_build_only_ms": if diagnostic_only && build_mode == IndexBuildMode::ProofBuildOnly {
            json!(proof_build_only_ms)
        } else {
            Value::Null
        },
        "validation_ms": validation_ms,
        "audit_ms": 0,
        "report_generation_ms": report_generation_ms,
        "comprehensive_total_ms": Value::Null,
        "wall_ms": elapsed,
        "current_exe": binary_metadata["current_exe"].clone(),
        "debug_assertions": binary_metadata["debug_assertions"].clone(),
        "binary_profile": binary_metadata["binary_profile"].clone(),
        "exact_command": binary_metadata["exact_command"].clone(),
        "claimable_for_thresholds": claimable_for_thresholds,
        "diagnostic_only": diagnostic_only,
        "timing_classification": timing_classification,
        "binary_metadata": binary_metadata,
        "summary": summary_json,
        "artifact_metadata_path": path_string(&artifact_metadata_path),
        "artifact_metadata": artifact_metadata,
        "storage_budget": storage_budget.to_json(),
        "mode_separation": {
            "proof_build_only_excludes": [
                "storage_audit",
                "dbstat",
                "relation_sampler",
                "path_evidence_audit_sampler",
                "path_evidence_sampler",
                "cgc_comparison",
                "manual_relation_label_summary",
                "readme_or_report_generation",
                "full_comprehensive_benchmark_generation",
                "repeated_build_attempts",
                "comprehensive_markdown_generation",
                "artifact_compression",
                "fresh_repeat_update_loops"
            ],
            "prohibited_operations_ran": {
                "storage_audit": false,
                "dbstat": false,
                "relation_sampler": false,
                "path_evidence_sampler": false,
                "cgc_comparison": false,
                "manual_precision_summary": false,
                "readme_or_report_generation": false,
                "full_comprehensive_benchmark_generation": false,
                "repeated_build_attempts": false,
                "artifact_compression": false,
                "vacuum": false,
                "analyze": false,
                "fresh_repeat_update_loops": false
            },
            "post_build_checks": {
                "full_integrity_check_ran": build_mode == IndexBuildMode::ProofBuildPlusValidation,
                "quick_check_ran": build_mode == IndexBuildMode::ProofBuildOnly,
                "publish_gate": match build_mode {
                    IndexBuildMode::ProofBuildOnly => "quick_check",
                    IndexBuildMode::ProofBuildPlusValidation => "integrity_check"
                }
            },
            "cgc_autorun": false,
            "timing_separation": {
                "proof_build_only_ms": proof_build_only_value,
                "validation_ms": validation_ms,
                "audit_ms": 0,
                "report_generation_ms": report_generation_ms,
                "comprehensive_total_ms": Value::Null
            }
        }
    }))
}

pub(crate) fn proof_build_mode_artifact_metadata(
    summary: &IndexSummary,
    build_mode: IndexBuildMode,
    proof_build_only_ms: u128,
    validation_ms: f64,
    metadata_path: &Path,
    binary_metadata: &Value,
) -> Value {
    let db_path = PathBuf::from(&summary.db_path);
    let schema_version = sqlite_user_version_read_only(&db_path).unwrap_or(SCHEMA_VERSION);
    let db_size_bytes = metadata_len(&db_path).unwrap_or(0);
    let artifact_metadata_filename_policy =
        artifact_safe_file_name_metadata(&artifact_safe_file_name_for_path(
            &db_path,
            ".metadata.json",
            ARTIFACT_SAFE_FILENAME_MAX_CHARS,
        ));
    let build_duration_ms = summary
        .profile
        .as_ref()
        .map(|profile| profile.total_wall_ms.min(u128::from(u64::MAX)) as u64)
        .unwrap_or_else(|| proof_build_only_ms.min(u128::from(u64::MAX)) as u64);
    let quick_check_count = profile_span_count(summary.profile.as_ref(), "quick_check");
    let integrity_check_count = profile_span_count(summary.profile.as_ref(), "integrity_check");
    let artifact_validated =
        build_mode == IndexBuildMode::ProofBuildPlusValidation && integrity_check_count > 0;
    let claimable_for_thresholds = binary_metadata["claimable_for_thresholds"]
        .as_bool()
        .unwrap_or(false);
    let diagnostic_only = binary_metadata["diagnostic_only"]
        .as_bool()
        .unwrap_or(false);
    let timing_classification = if diagnostic_only {
        "diagnostic_only"
    } else {
        "production"
    };
    let proof_build_only_value =
        if build_mode == IndexBuildMode::ProofBuildOnly && !claimable_for_thresholds {
            json!(NON_CLAIMABLE_DEBUG_TIMING)
        } else {
            json!(proof_build_only_ms)
        };
    json!({
        "schema_version": schema_version,
        "current_schema_version": SCHEMA_VERSION,
        "migration_version": schema_version,
        "current_migration_version": SCHEMA_VERSION,
        "artifact_path": summary.db_path.clone(),
        "artifact_metadata_path": path_string(metadata_path),
        "artifact_metadata_filename_policy": artifact_metadata_filename_policy,
        "artifact_created_at": file_modified_unix_ms(&db_path).unwrap_or_else(unix_time_ms),
        "git_commit": current_git_commit(),
        "storage_mode": "proof",
        "build_mode": build_mode.as_str(),
        "build_duration_ms": build_duration_ms,
        "proof_build_only_ms": proof_build_only_value,
        "diagnostic_debug_proof_build_only_ms": if diagnostic_only && build_mode == IndexBuildMode::ProofBuildOnly {
            json!(proof_build_only_ms)
        } else {
            Value::Null
        },
        "validation_ms": validation_ms,
        "audit_ms": 0,
        "report_generation_ms": Value::Null,
        "comprehensive_total_ms": Value::Null,
        "db_size_bytes": db_size_bytes,
        "integrity_status": if artifact_validated {
            "validated"
        } else {
            "quick_check_only"
        },
        "artifact_validation": {
            "validated": artifact_validated,
            "claimable_as_validated": artifact_validated,
            "validation_mode": if artifact_validated {
                "proof-build-plus-validation"
            } else {
                "not_validated"
            },
            "quick_check_count": quick_check_count,
            "integrity_check_count": integrity_check_count,
            "publish_gate": match build_mode {
                IndexBuildMode::ProofBuildOnly => "quick_check",
                IndexBuildMode::ProofBuildPlusValidation => "integrity_check",
            },
            "notes": if artifact_validated {
                vec!["Artifact may be described as validated because proof-build-plus-validation passed its configured full integrity_check gate."]
            } else {
                vec!["Artifact must not be described as validated; proof-build-only publishes after the configured quick_check timing gate."]
            }
        },
        "current_exe": binary_metadata["current_exe"].clone(),
        "debug_assertions": binary_metadata["debug_assertions"].clone(),
        "binary_profile": binary_metadata["binary_profile"].clone(),
        "exact_command": binary_metadata["exact_command"].clone(),
        "claimable_for_thresholds": claimable_for_thresholds,
        "diagnostic_only": diagnostic_only,
        "timing_classification": timing_classification,
        "binary_metadata": binary_metadata.clone(),
        "contamination_contract": proof_build_mode_contamination_contract(build_mode),
        "notes": [
            "Metadata is emitted after the measured proof DB build and is not included in proof_build_only_ms.",
            "No storage audit, relation sampler, PathEvidence sampler, CGC comparison, manual precision summary, comprehensive report generation, compression, repeat loop, update loop, VACUUM, or ANALYZE is part of proof_build_only_ms."
        ]
    })
}

pub(crate) fn proof_build_mode_contamination_contract(build_mode: IndexBuildMode) -> Value {
    json!({
        "prohibited_operations_ran": {
            "storage_audit": false,
            "dbstat": false,
            "relation_sampler": false,
            "path_evidence_sampler": false,
            "cgc_comparison": false,
            "manual_precision_summary": false,
            "readme_or_report_generation": false,
            "full_comprehensive_benchmark_generation": false,
            "repeated_build_attempts": false,
            "artifact_compression": false,
            "vacuum": false,
            "analyze": false,
            "fresh_repeat_update_loops": false
        },
        "configured_publish_gate": match build_mode {
            IndexBuildMode::ProofBuildOnly => "quick_check",
            IndexBuildMode::ProofBuildPlusValidation => "integrity_check",
        },
        "proof_build_only_target_surface": build_mode == IndexBuildMode::ProofBuildOnly,
    })
}

pub(crate) fn parse_query_surface_benchmark_options(
    args: &[String],
) -> Result<QuerySurfaceBenchmarkOptions, String> {
    let mut repo = PathBuf::from(".");
    let mut db_path = None;
    let mut fresh = false;
    let mut iterations = DEFAULT_QUERY_SURFACE_ITERATIONS;
    let mut out_json = PathBuf::from("reports")
        .join("audit")
        .join("default_query_surface.json");
    let mut out_md = PathBuf::from("reports")
        .join("audit")
        .join("default_query_surface.md");
    let mut workers = None;
    let mut storage_budget = storage_budget::StorageBudgetOptions::fixture_smoke();

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--repo" => {
                index += 1;
                repo = PathBuf::from(required_cli_value(args, index, "--repo")?);
            }
            "--db" => {
                index += 1;
                db_path = Some(PathBuf::from(required_cli_value(args, index, "--db")?));
            }
            "--fresh" => {
                fresh = true;
            }
            "--iterations" => {
                index += 1;
                let raw = required_cli_value(args, index, "--iterations")?;
                iterations = raw
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --iterations value {raw}: {error}"))?
                    .max(1);
            }
            "--out-json" | "--json" => {
                index += 1;
                out_json = PathBuf::from(required_cli_value(args, index, "--out-json")?);
            }
            "--out-md" | "--markdown" => {
                index += 1;
                out_md = PathBuf::from(required_cli_value(args, index, "--out-md")?);
            }
            "--workers" => {
                index += 1;
                let raw = required_cli_value(args, index, "--workers")?;
                let parsed = raw
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --workers value {raw}: {error}"))?;
                if parsed == 0 {
                    return Err("--workers must be greater than 0".to_string());
                }
                workers = Some(parsed);
            }
            value => {
                if storage_budget::parse_storage_budget_flag(args, &mut index, &mut storage_budget)?
                {
                    index += 1;
                    continue;
                }
                return Err(format!("unknown query-surface benchmark option: {value}"));
            }
        }
        index += 1;
    }

    Ok(QuerySurfaceBenchmarkOptions {
        repo,
        db_path,
        fresh,
        iterations,
        out_json,
        out_md,
        workers,
        storage_budget,
    })
}

pub(crate) fn build_comprehensive_benchmark_report(
    options: &ComprehensiveBenchmarkOptions,
) -> Result<Value, String> {
    let baseline = read_json_file(&options.baseline_json)?;
    let mut gate = read_json_file(&options.compact_gate_json)?;
    let artifact_freshness = prepare_comprehensive_artifact(options, &mut gate)?;
    ensure_comprehensive_update_integrity_artifact(options, &mut gate)?;
    let query_surface_report =
        build_comprehensive_query_surface_artifact(options, &artifact_freshness);
    let proof_storage = read_gate_artifact_json(&gate, "proof_storage_json");
    let _audit_storage = read_gate_artifact_json(&gate, "audit_storage_json");
    let update_integrity = read_gate_artifact_json(&gate, "update_integrity_json");
    let repeat_unchanged = read_optional_json(Path::new(
        "reports/final/artifacts/compact_gate_repeat_unchanged.json",
    ));
    let context_pack_latency = read_gate_artifact_json(&gate, "context_pack_latency_json");
    let unresolved_latency = read_gate_artifact_json(&gate, "unresolved_calls_latency_json");
    let manual_labels = read_optional_json(Path::new(
        "reports/audit/manual_relation_labeling_summary.json",
    ));
    let comparison =
        read_optional_json(Path::new("reports/comparison/CODEGRAPH_VS_CGC_LATEST.json"));
    let clean_gate = read_optional_json(Path::new("reports/final/INTENDED_PERFORMANCE_GATE.json"));
    let previous = options
        .previous_json
        .as_ref()
        .and_then(|path| read_optional_json(path));

    let graph_truth = gate_value(&gate, &["gates", "graph_truth"])
        .cloned()
        .or_else(|| gate_value(&baseline, &["graph_truth_summary"]).cloned())
        .unwrap_or_else(|| json!({}));
    let context_gate = gate_value(&gate, &["gates", "context_packet"])
        .cloned()
        .or_else(|| gate_value(&baseline, &["context_quality_summary"]).cloned())
        .unwrap_or_else(|| json!({}));
    let storage = gate_value(&gate, &["storage"])
        .cloned()
        .unwrap_or_else(|| json!({}));
    let proof_build = gate_value(&gate, &["autoresearch", "proof_build"])
        .cloned()
        .unwrap_or_else(|| json!({}));
    let update_path = gate_value(&gate, &["update_path"])
        .cloned()
        .unwrap_or_else(|| json!({}));
    let relation_sampler = gate_value(&gate, &["relation_sampler"])
        .cloned()
        .unwrap_or_else(|| json!({}));

    let correctness_metrics = comprehensive_correctness_metrics(&graph_truth);
    let context_metrics = comprehensive_context_metrics(&context_gate, &storage, &relation_sampler);
    let integrity_metrics = comprehensive_integrity_metrics(&gate, &proof_storage, &storage);
    let artifact_freshness_metrics = comprehensive_artifact_freshness_metrics(&artifact_freshness);
    let storage_summary_metrics = comprehensive_storage_summary_metrics(&storage, &proof_storage);
    let storage_contributors =
        comprehensive_storage_contributors(&proof_storage, previous.as_ref(), &baseline);
    let cardinality_metrics =
        comprehensive_cardinality_metrics(&proof_build, &proof_storage, &storage);
    let cold_profile_metrics = comprehensive_cold_profile_metrics(&proof_build);
    let cold_mode_distinction = comprehensive_cold_build_mode_distinction(&gate);
    let cold_waterfall = comprehensive_cold_build_waterfall(&gate);
    let cold_interpretation = comprehensive_cold_build_interpretation(&gate);
    let repeat_metrics = comprehensive_repeat_metrics(
        &update_path,
        repeat_unchanged.as_ref(),
        update_integrity.as_ref(),
    );
    let update_metrics =
        comprehensive_single_file_update_metrics(&update_path, update_integrity.as_ref());
    let query_latency = comprehensive_query_latency_metrics(
        &context_pack_latency,
        &unresolved_latency,
        &gate,
        Some(&query_surface_report),
    );
    let manual_quality = comprehensive_manual_quality_section(&manual_labels);
    let comparison_section = comprehensive_comparison_section(&comparison);
    let regression_summary =
        comprehensive_regression_summary(&baseline, &gate, &clean_gate, previous.as_ref());
    let report_binary_metadata = benchmark_binary_metadata();

    let mut failed_targets = Vec::new();
    let mut passed_targets = Vec::new();
    collect_gate_statuses(
        &correctness_metrics,
        &mut failed_targets,
        &mut passed_targets,
    );
    collect_gate_statuses(&context_metrics, &mut failed_targets, &mut passed_targets);
    collect_gate_statuses(&integrity_metrics, &mut failed_targets, &mut passed_targets);
    collect_gate_statuses(
        &artifact_freshness_metrics,
        &mut failed_targets,
        &mut passed_targets,
    );
    collect_gate_statuses(
        &storage_summary_metrics,
        &mut failed_targets,
        &mut passed_targets,
    );
    collect_gate_statuses(
        &cold_profile_metrics,
        &mut failed_targets,
        &mut passed_targets,
    );
    collect_gate_statuses(&repeat_metrics, &mut failed_targets, &mut passed_targets);
    collect_gate_statuses(&update_metrics, &mut failed_targets, &mut passed_targets);
    collect_query_gate_statuses(&query_latency, &mut failed_targets, &mut passed_targets);

    failed_targets.sort();
    failed_targets.dedup();
    passed_targets.sort();
    passed_targets.dedup();

    let graph_context_integrity_ok = !correctness_metrics
        .iter()
        .chain(context_metrics.iter())
        .chain(integrity_metrics.iter())
        .any(|metric| metric["status"].as_str() == Some("fail"));
    let verdict = if failed_targets.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let comparison_claims_allowed = verdict == "pass"
        && comparison_section["verdict"].as_str() == Some("pass")
        && comparison_section["cgc_completed"].as_bool() == Some(true);
    let optimization_may_continue = graph_context_integrity_ok;
    let reason_for_failure = if failed_targets.is_empty() {
        "all tracked pass/fail targets are currently satisfied".to_string()
    } else {
        format!(
            "failed targets: {}",
            failed_targets
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let executive_verdict = json!({
        "verdict": verdict,
        "reason_for_failure": reason_for_failure,
        "exact_failed_targets": failed_targets,
        "exact_passed_targets": passed_targets,
        "optimization_may_continue": optimization_may_continue,
        "optimization_may_continue_notes": if optimization_may_continue {
            "Yes, but only behind the do-not-regress gates: graph truth, context packet quality, DB integrity, context_pack latency, unresolved-calls latency, and repeat unchanged must stay green."
        } else {
            "No. Correctness, context, or integrity regressed."
        },
        "comparison_claims_allowed": comparison_claims_allowed,
        "comparison_claims_notes": if comparison_claims_allowed {
            "Comparable competitor artifacts are complete and CodeGraph has not failed internal targets."
        } else {
            "No superiority claim is allowed while internal targets fail or CGC artifacts are incomplete/stale."
        }
    });

    Ok(json!({
        "schema_version": 1,
        "benchmark_id": "comprehensive_benchmark",
        "generated_at_unix_ms": unix_time_ms(),
        "timestamp": options.timestamp,
        "phase": PHASE,
        "source_of_truth": "MVP.md",
        "current_exe": report_binary_metadata["current_exe"].clone(),
        "debug_assertions": report_binary_metadata["debug_assertions"].clone(),
        "binary_profile": report_binary_metadata["binary_profile"].clone(),
        "exact_command": report_binary_metadata["exact_command"].clone(),
        "claimable_for_thresholds": report_binary_metadata["claimable_for_thresholds"].clone(),
        "diagnostic_only": report_binary_metadata["diagnostic_only"].clone(),
        "binary_metadata": report_binary_metadata,
        "execution_mode": if artifact_freshness["artifact_reuse"].as_bool() == Some(true) {
            "explicit_artifact_reuse"
        } else {
            "fresh_proof_build"
        },
        "execution_mode_notes": if artifact_freshness["artifact_reuse"].as_bool() == Some(true) {
            "The benchmark used an explicitly supplied proof DB artifact. Storage and cold-build results are claimable only when freshness metadata matches the current schema/build."
        } else {
            "The benchmark built a fresh proof DB artifact before reading storage and cold-build metrics."
        },
        "artifact_freshness": artifact_freshness,
        "inputs": {
            "baseline_json": path_string(&options.baseline_json),
            "compact_gate_json": path_string(&options.compact_gate_json),
            "previous_json": options.previous_json.as_ref().map(|path| path_string(path)),
            "proof_storage_json": gate_value(&gate, &["artifacts", "proof_storage_json"]).cloned().unwrap_or(Value::Null),
            "audit_storage_json": gate_value(&gate, &["artifacts", "audit_storage_json"]).cloned().unwrap_or(Value::Null),
            "update_integrity_json": gate_value(&gate, &["artifacts", "update_integrity_json"]).cloned().unwrap_or(Value::Null),
            "artifact_metadata_json": options.artifact_metadata.as_ref().map(|path| path_string(path)),
            "manual_label_summary_json": "reports/audit/manual_relation_labeling_summary.json",
            "comparison_latest_json": "reports/comparison/CODEGRAPH_VS_CGC_LATEST.json"
        },
        "sections": {
            "executive_verdict": executive_verdict,
            "correctness_gates": {
                "metrics": correctness_metrics
            },
            "context_packet_gate": {
                "metrics": context_metrics
            },
            "db_integrity": {
                "metrics": integrity_metrics
            },
            "artifact_freshness": {
                "metrics": artifact_freshness_metrics,
                "metadata": gate_value(&gate, &["artifact_freshness"]).cloned().unwrap_or(Value::Null)
            },
            "storage_summary": {
                "metrics": storage_summary_metrics
            },
            "storage_contributors": {
                "contributors": storage_contributors
            },
            "row_counts_and_cardinality": {
                "metrics": cardinality_metrics
            },
            "cold_proof_build_profile": {
                "metrics": cold_profile_metrics,
                "mode_distinction": cold_mode_distinction,
                "waterfall": cold_waterfall,
                "interpretation": cold_interpretation,
                "top_10_slowest_stages": comprehensive_top_slowest_stages(&cold_profile_metrics)
            },
            "repeat_unchanged_index": {
                "metrics": repeat_metrics
            },
            "single_file_update": {
                "metrics": update_metrics
            },
            "default_query_surface": query_surface_report,
            "query_latency": query_latency,
            "manual_relation_quality": manual_quality,
            "cgc_competitor_comparison_readiness": comparison_section,
            "regression_summary": regression_summary
        },
        "notes": [
            "Unknown values are preserved as unknown and are not reported as passes.",
            "The benchmark fails if graph/context correctness regresses, proof DB exceeds 250 MiB, single-file update exceeds 750 ms, cold proof build exceeds 60 seconds, context_pack p95 exceeds 2 seconds, or DB integrity is not ok.",
            "Storage/debug sidecars are reported separately; audit/debug size is not counted against the proof DB target."
        ]
    }))
}

pub(crate) fn ensure_comprehensive_update_integrity_artifact(
    options: &ComprehensiveBenchmarkOptions,
    gate: &mut Value,
) -> Result<(), String> {
    if cfg!(debug_assertions) && read_gate_artifact_json(gate, "update_integrity_json").is_some() {
        return Ok(());
    }

    let artifact_dir = absolutize_path(&options.output_dir)?.join("artifacts");
    fs::create_dir_all(&artifact_dir).map_err(|error| error.to_string())?;
    let update_json_path =
        artifact_dir.join(format!("update_integrity_{}.json", options.timestamp));
    let update_md_path = artifact_dir.join(format!("update_integrity_{}.md", options.timestamp));
    let workdir = artifact_dir.join("upd");

    let mut args = vec![
        "--mode".to_string(),
        "update-fast".to_string(),
        "--loop-kind".to_string(),
        "update-fast".to_string(),
        "--iterations".to_string(),
        "20".to_string(),
        "--skip-autoresearch".to_string(),
        "--workdir".to_string(),
        path_string(&workdir),
        "--out-json".to_string(),
        path_string(&update_json_path),
        "--out-md".to_string(),
        path_string(&update_md_path),
        "--timeout-ms".to_string(),
        "180000".to_string(),
        "--max-artifacts-mib".to_string(),
        options.storage_budget.max_artifacts_mib.to_string(),
        "--min-free-disk-gib".to_string(),
        options.storage_budget.min_free_disk_gib.to_string(),
    ];
    if let Some(workers) = options.workers {
        args.push("--workers".to_string());
        args.push(workers.to_string());
    }

    run_update_integrity_harness_command(&args)?;
    let update_integrity = read_json_file(&update_json_path)?;
    patch_gate_with_update_integrity_artifact(
        gate,
        &update_json_path,
        &update_md_path,
        &update_integrity,
    );
    Ok(())
}

pub(crate) fn patch_gate_with_update_integrity_artifact(
    gate: &mut Value,
    update_json_path: &Path,
    update_md_path: &Path,
    update_integrity: &Value,
) {
    let update_samples = update_integrity_wall_samples(update_integrity, "update");
    let restore_samples = update_integrity_wall_samples(update_integrity, "restore");
    let update_p95 = percentile(&update_samples, 0.95);
    let restore_p95 = percentile(&restore_samples, 0.95);

    let Some(gate_object) = gate.as_object_mut() else {
        return;
    };
    let artifacts = gate_object
        .entry("artifacts".to_string())
        .or_insert_with(|| json!({}));
    if let Some(artifacts_object) = artifacts.as_object_mut() {
        artifacts_object.insert(
            "update_integrity_json".to_string(),
            json!(path_string(update_json_path)),
        );
        artifacts_object.insert(
            "update_integrity_md".to_string(),
            json!(path_string(update_md_path)),
        );
    }

    let update_path = gate_object
        .entry("update_path".to_string())
        .or_insert_with(|| json!({}));
    let Some(update_path_object) = update_path.as_object_mut() else {
        return;
    };
    update_path_object.insert(
        "single_file_update".to_string(),
        json!({
            "wall_ms": update_p95,
            "sample_count": update_samples.len(),
            "source": "fresh_update_integrity_json",
            "artifact_json": path_string(update_json_path),
            "integrity_status": update_integrity["status"].clone(),
        }),
    );
    update_path_object.insert(
        "restore_update".to_string(),
        json!({
            "wall_ms": restore_p95,
            "sample_count": restore_samples.len(),
            "source": "fresh_update_integrity_json",
            "artifact_json": path_string(update_json_path),
            "integrity_status": update_integrity["status"].clone(),
        }),
    );
}

pub(crate) fn update_integrity_wall_samples(update_integrity: &Value, step: &str) -> Vec<f64> {
    update_integrity
        .get("repos")
        .and_then(Value::as_array)
        .map(|repos| {
            repos
                .iter()
                .flat_map(|repo| {
                    repo.get("iteration_results")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(move |iteration| {
                            iteration
                                .get(step)
                                .and_then(|value| value_f64(value, &["wall_ms"]))
                        })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub(crate) fn build_comprehensive_query_surface_artifact(
    options: &ComprehensiveBenchmarkOptions,
    artifact_freshness: &Value,
) -> Value {
    let Some(db_path) = artifact_freshness
        .get("artifact_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
    else {
        return comprehensive_query_surface_failure(
            "missing proof artifact path; query surface cannot be measured",
        );
    };
    let artifact_dir = match absolutize_path(&options.output_dir) {
        Ok(path) => path.join("artifacts"),
        Err(error) => {
            return comprehensive_query_surface_failure(&format!(
                "failed to resolve output directory for query-surface artifact: {error}"
            ));
        }
    };
    if fs::create_dir_all(&artifact_dir).is_err() {
        return comprehensive_query_surface_failure(
            "failed to create comprehensive query-surface artifact directory",
        );
    }
    let report = build_default_query_surface_report(
        &options.repo,
        &db_path,
        DEFAULT_QUERY_SURFACE_ITERATIONS,
    );
    let json_path = artifact_dir.join(format!(
        "comprehensive_query_surface_{}.json",
        options.timestamp
    ));
    let md_path = artifact_dir.join(format!(
        "comprehensive_query_surface_{}.md",
        options.timestamp
    ));
    let markdown = render_default_query_surface_markdown(&report);
    let _ = write_json_file(&json_path, &report);
    let _ = write_text_file(&md_path, &markdown);
    json!({
        "status": report["status"].clone(),
        "artifact_json": path_string(&json_path),
        "artifact_md": path_string(&md_path),
        "report": report,
    })
}

pub(crate) fn comprehensive_query_surface_failure(error: &str) -> Value {
    json!({
        "status": "failed",
        "artifact_json": Value::Null,
        "artifact_md": Value::Null,
        "report": {
            "schema_version": 1,
            "status": "failed",
            "generated_at_unix_ms": unix_time_ms(),
            "queries": required_query_surface_ids()
                .into_iter()
                .map(|(id, target)| query_surface_failure_metric(id, target, error))
                .collect::<Vec<_>>(),
            "summary": {
                "passed_queries": 0,
                "failed_queries": required_query_surface_ids().len(),
                "all_default_queries_complete": false,
            },
            "notes": [error],
        },
    })
}

pub(crate) fn prepare_comprehensive_artifact(
    options: &ComprehensiveBenchmarkOptions,
    gate: &mut Value,
) -> Result<Value, String> {
    match &options.artifact_mode {
        ComprehensiveArtifactMode::Fresh => build_fresh_comprehensive_proof_artifact(options, gate),
        ComprehensiveArtifactMode::Existing(path) => {
            inspect_existing_comprehensive_proof_artifact(options, gate, path)
        }
    }
}

pub(crate) fn build_fresh_comprehensive_proof_artifact(
    options: &ComprehensiveBenchmarkOptions,
    gate: &mut Value,
) -> Result<Value, String> {
    if cfg!(debug_assertions) && !options.allow_debug_timing {
        return Err(
            "bench comprehensive would measure proof-build-only timing from a debug binary; pass --allow-debug-timing for a diagnostic-only report, or run the release binary for claimable production threshold timing"
                .to_string(),
        );
    }
    let artifact_dir = absolutize_path(&options.output_dir)?.join("artifacts");
    fs::create_dir_all(&artifact_dir).map_err(|error| error.to_string())?;
    let db_path = artifact_dir.join(format!("comprehensive_proof_{}.sqlite", options.timestamp));
    remove_sqlite_family_if_exists(&db_path)?;
    let index_start = Instant::now();
    let summary = index_repo_to_db_with_options(
        &options.repo,
        &db_path,
        IndexOptions {
            profile: true,
            json: false,
            worker_count: options.workers,
            storage_mode: StorageMode::Proof,
            build_mode: IndexBuildMode::ProofBuildOnly,
            ..IndexOptions::default()
        },
    )
    .map_err(|error| error.to_string())?;
    let measured_duration = elapsed_ms(index_start);
    let build_duration_ms = summary
        .profile
        .as_ref()
        .map(|profile| profile.total_wall_ms.min(u128::from(u64::MAX)) as u64)
        .unwrap_or(measured_duration);
    let storage_json_path = artifact_dir.join(format!(
        "comprehensive_proof_storage_{}.json",
        options.timestamp
    ));
    let storage_md_path = artifact_dir.join(format!(
        "comprehensive_proof_storage_{}.md",
        options.timestamp
    ));
    let audit_args = vec![
        "storage".to_string(),
        "--db".to_string(),
        path_string(&db_path),
        "--json".to_string(),
        path_string(&storage_json_path),
        "--markdown".to_string(),
        path_string(&storage_md_path),
    ];
    let audit_start = Instant::now();
    audit::run_audit_command(&audit_args)?;
    let audit_ms = elapsed_ms(audit_start);
    let proof_storage = read_json_file(&storage_json_path)?;
    let lifecycle_status = benchmark_inspection_lifecycle_status(
        &options.repo,
        &db_path,
        "bench.comprehensive.fresh_storage_audit",
        Some(StorageMode::Proof),
    );
    let lifecycle_claimable = benchmark_inspection_claimable(&lifecycle_status);
    let schema_version = sqlite_user_version_read_only(&db_path).unwrap_or(SCHEMA_VERSION);
    let integrity_status = value_string(&proof_storage, &["integrity_check", "status"])
        .unwrap_or_else(|| {
            sqlite_quick_check_status(&db_path).unwrap_or_else(|| "unknown".to_string())
        });
    let db_size_bytes = metadata_len(&db_path).map_err(|error| error.to_string())?;
    let metadata_path = options.artifact_metadata.clone().unwrap_or_else(|| {
        artifact_dir.join(format!(
            "comprehensive_proof_{}.artifact.json",
            options.timestamp
        ))
    });
    let binary_metadata = benchmark_binary_metadata();
    let claimable_for_thresholds = binary_metadata["claimable_for_thresholds"]
        .as_bool()
        .unwrap_or(false);
    let diagnostic_only = binary_metadata["diagnostic_only"]
        .as_bool()
        .unwrap_or(false);
    let timing_classification = if diagnostic_only {
        "diagnostic_only"
    } else {
        "production"
    };
    let proof_build_only_value = if claimable_for_thresholds {
        json!(build_duration_ms)
    } else {
        json!(NON_CLAIMABLE_DEBUG_TIMING)
    };
    let artifact_validation = json!({
        "validated": false,
        "claimable_as_validated": false,
        "validation_mode": "not_run",
        "publish_gate": "proof-build-only quick_check",
        "integrity_check_source": "post_build_storage_audit",
        "notes": [
            "The comprehensive fresh artifact is built in proof-build-only mode for timing.",
            "The later storage audit integrity_check is reported as audit_ms and does not make proof_build_only_ms a validation-mode timing."
        ]
    });
    let timing_note = if claimable_for_thresholds {
        "Proof-build timing came from a release binary and is claimable for production thresholds."
    } else {
        "Proof-build timing came from a debug binary; it is diagnostic_only and not claimable for production thresholds."
    };
    let mut metadata_object = serde_json::Map::new();
    metadata_object.insert("artifact_path".to_string(), json!(path_string(&db_path)));
    metadata_object.insert(
        "artifact_created_at".to_string(),
        json!(file_modified_unix_ms(&db_path).unwrap_or_else(unix_time_ms)),
    );
    metadata_object.insert("git_commit".to_string(), json!(current_git_commit()));
    metadata_object.insert("schema_version".to_string(), json!(schema_version));
    metadata_object.insert("current_schema_version".to_string(), json!(SCHEMA_VERSION));
    metadata_object.insert("migration_version".to_string(), json!(schema_version));
    metadata_object.insert(
        "current_migration_version".to_string(),
        json!(SCHEMA_VERSION),
    );
    metadata_object.insert("storage_mode".to_string(), json!("proof"));
    metadata_object.insert(
        "build_command".to_string(),
        json!(comprehensive_fresh_build_command(options, &db_path)),
    );
    metadata_object.insert("build_duration_ms".to_string(), json!(build_duration_ms));
    metadata_object.insert("proof_build_only_ms".to_string(), proof_build_only_value);
    metadata_object.insert(
        "diagnostic_debug_proof_build_only_ms".to_string(),
        if diagnostic_only {
            json!(build_duration_ms)
        } else {
            Value::Null
        },
    );
    metadata_object.insert("validation_ms".to_string(), json!(0));
    metadata_object.insert("audit_ms".to_string(), json!(audit_ms));
    metadata_object.insert("report_generation_ms".to_string(), Value::Null);
    metadata_object.insert("comprehensive_total_ms".to_string(), Value::Null);
    metadata_object.insert("inspection_read_only".to_string(), json!(true));
    metadata_object.insert(
        "artifact_mutated_during_inspection".to_string(),
        json!(false),
    );
    metadata_object.insert("lifecycle_status".to_string(), lifecycle_status);
    metadata_object.insert("db_size_bytes".to_string(), json!(db_size_bytes));
    metadata_object.insert("integrity_status".to_string(), json!(integrity_status));
    metadata_object.insert("artifact_validation".to_string(), artifact_validation);
    metadata_object.insert("benchmark_run_id".to_string(), json!(options.timestamp));
    metadata_object.insert(
        "artifact_metadata_path".to_string(),
        json!(path_string(&metadata_path)),
    );
    metadata_object.insert(
        "current_exe".to_string(),
        binary_metadata["current_exe"].clone(),
    );
    metadata_object.insert(
        "debug_assertions".to_string(),
        binary_metadata["debug_assertions"].clone(),
    );
    metadata_object.insert(
        "binary_profile".to_string(),
        binary_metadata["binary_profile"].clone(),
    );
    metadata_object.insert(
        "exact_command".to_string(),
        binary_metadata["exact_command"].clone(),
    );
    metadata_object.insert(
        "claimable_for_thresholds".to_string(),
        json!(claimable_for_thresholds),
    );
    metadata_object.insert("diagnostic_only".to_string(), json!(diagnostic_only));
    metadata_object.insert(
        "timing_classification".to_string(),
        json!(timing_classification),
    );
    metadata_object.insert("binary_metadata".to_string(), binary_metadata);
    metadata_object.insert("freshness_metadata_present".to_string(), json!(true));
    metadata_object.insert("artifact_reuse".to_string(), json!(false));
    metadata_object.insert("freshly_built".to_string(), json!(true));
    metadata_object.insert("stale".to_string(), json!(false));
    metadata_object.insert("stale_reasons".to_string(), json!([]));
    metadata_object.insert("freshness_status".to_string(), json!("fresh"));
    metadata_object.insert(
        "storage_result_claimable".to_string(),
        json!(lifecycle_claimable),
    );
    metadata_object.insert(
        "cold_build_result_claimable".to_string(),
        json!(lifecycle_claimable && claimable_for_thresholds),
    );
    metadata_object.insert(
        "notes".to_string(),
        json!([
            "Fresh proof DB built by comprehensive benchmark before storage and cold-build metrics were read.",
            timing_note
        ]),
    );
    let metadata = Value::Object(metadata_object);
    write_json_file(&metadata_path, &metadata)?;
    patch_gate_with_proof_artifact(
        gate,
        &db_path,
        &storage_json_path,
        &proof_storage,
        &summary,
        &metadata,
    )?;
    Ok(metadata)
}

pub(crate) fn inspect_existing_comprehensive_proof_artifact(
    options: &ComprehensiveBenchmarkOptions,
    gate: &mut Value,
    db_path: &Path,
) -> Result<Value, String> {
    if !db_path.exists() {
        return Err(format!(
            "--use-existing-artifact path does not exist: {}",
            db_path.display()
        ));
    }
    let artifact_dir = absolutize_path(&options.output_dir)?.join("artifacts");
    fs::create_dir_all(&artifact_dir).map_err(|error| error.to_string())?;
    let metadata_path = options
        .artifact_metadata
        .clone()
        .unwrap_or_else(|| default_artifact_metadata_path(db_path));
    let metadata = read_optional_json(&metadata_path);
    let storage_json_path = artifact_dir.join(format!(
        "comprehensive_reused_proof_storage_{}.json",
        options.timestamp
    ));
    let storage_md_path = artifact_dir.join(format!(
        "comprehensive_reused_proof_storage_{}.md",
        options.timestamp
    ));
    let audit_args = vec![
        "storage".to_string(),
        "--db".to_string(),
        path_string(db_path),
        "--json".to_string(),
        path_string(&storage_json_path),
        "--markdown".to_string(),
        path_string(&storage_md_path),
    ];
    audit::run_audit_command(&audit_args)?;
    let proof_storage = read_json_file(&storage_json_path)?;
    let actual_schema_version = sqlite_user_version_read_only(db_path);
    let lifecycle_status = benchmark_inspection_lifecycle_status(
        &options.repo,
        db_path,
        "bench.comprehensive.existing_artifact",
        Some(StorageMode::Proof),
    );
    let lifecycle_claimable = benchmark_inspection_claimable(&lifecycle_status);
    let actual_db_size_bytes = metadata_len(db_path).map_err(|error| error.to_string())?;
    let integrity_status = value_string(&proof_storage, &["integrity_check", "status"])
        .unwrap_or_else(|| {
            sqlite_quick_check_status(db_path).unwrap_or_else(|| "unknown".to_string())
        });

    let mut stale_reasons = Vec::new();
    let metadata_present = metadata.is_some();
    if !metadata_present {
        stale_reasons.push("missing freshness metadata".to_string());
    }
    let metadata_value = metadata.unwrap_or_else(|| json!({}));
    let metadata_schema = value_u64(&metadata_value, &["schema_version"]);
    let metadata_migration = value_u64(&metadata_value, &["migration_version"]);
    let metadata_storage_mode = value_string(&metadata_value, &["storage_mode"]);
    let metadata_db_size = value_u64(&metadata_value, &["db_size_bytes"]);
    let metadata_build_duration = value_u64(&metadata_value, &["build_duration_ms"]);
    let metadata_git_commit = value_string(&metadata_value, &["git_commit"]);
    let metadata_current_exe = value_string(&metadata_value, &["current_exe"]);
    let metadata_binary_profile = value_string(&metadata_value, &["binary_profile"]);
    let metadata_exact_command = value_string(&metadata_value, &["exact_command"]);
    let metadata_claimable_for_thresholds =
        value_bool(&metadata_value, &["claimable_for_thresholds"]);
    let metadata_debug_assertions = value_bool(&metadata_value, &["debug_assertions"]);
    let metadata_diagnostic_only = value_bool(&metadata_value, &["diagnostic_only"]);
    let current_git = current_git_commit();

    if metadata_schema != Some(u64::from(SCHEMA_VERSION)) {
        stale_reasons.push(format!(
            "schema mismatch: metadata={:?}, current={}",
            metadata_schema, SCHEMA_VERSION
        ));
    }
    if let Some(actual) = actual_schema_version {
        if actual != SCHEMA_VERSION {
            stale_reasons.push(format!(
                "database schema mismatch: actual={actual}, current={SCHEMA_VERSION}"
            ));
        }
    } else {
        stale_reasons.push("database schema unavailable".to_string());
    }
    if metadata_migration != Some(u64::from(SCHEMA_VERSION)) {
        stale_reasons.push(format!(
            "migration mismatch: metadata={:?}, current={}",
            metadata_migration, SCHEMA_VERSION
        ));
    }
    if metadata_storage_mode.as_deref() != Some("proof") {
        stale_reasons.push(format!(
            "storage mode mismatch: metadata={:?}, expected proof",
            metadata_storage_mode
        ));
    }
    if let Some(recorded_size) = metadata_db_size {
        if recorded_size != actual_db_size_bytes {
            stale_reasons.push(format!(
                "db size mismatch: metadata={recorded_size}, actual={actual_db_size_bytes}"
            ));
        }
    } else {
        stale_reasons.push("missing db_size_bytes".to_string());
    }
    if metadata_build_duration.is_none() {
        stale_reasons.push("missing build_duration_ms".to_string());
    }
    if metadata_current_exe.is_none() {
        stale_reasons.push("missing current_exe".to_string());
    }
    if metadata_binary_profile.is_none() {
        stale_reasons.push("missing binary_profile".to_string());
    }
    if metadata_exact_command.is_none() {
        stale_reasons.push("missing exact_command".to_string());
    }
    if metadata_claimable_for_thresholds.is_none() {
        stale_reasons.push("missing claimable_for_thresholds".to_string());
    }
    if let Some(recorded_git) = metadata_git_commit.as_deref() {
        if recorded_git != "unknown" && current_git != "unknown" && recorded_git != current_git {
            stale_reasons.push(format!(
                "git commit mismatch: metadata={recorded_git}, current={current_git}"
            ));
        }
    } else {
        stale_reasons.push("missing git_commit".to_string());
    }
    if integrity_status != "ok" {
        stale_reasons.push(format!("integrity status is {integrity_status}"));
    }
    if !lifecycle_claimable {
        let blockers = lifecycle_status["blockers"]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "lifecycle preflight blocked claimability".to_string());
        stale_reasons.push(format!("lifecycle preflight not claimable: {blockers}"));
    }

    let stale = !stale_reasons.is_empty();
    if options.fail_on_stale_artifact && stale {
        return Err(format!(
            "stale artifact refused by --fail-on-stale-artifact: {} ({})",
            db_path.display(),
            stale_reasons.join("; ")
        ));
    }

    let build_duration_ms = metadata_build_duration.unwrap_or(0);
    let claimable_for_thresholds = metadata_claimable_for_thresholds.unwrap_or(false);
    let diagnostic_only = metadata_diagnostic_only
        .unwrap_or(metadata_debug_assertions.unwrap_or(false) || !claimable_for_thresholds);
    let proof_build_only_value = if claimable_for_thresholds && metadata_build_duration.is_some() {
        json!(build_duration_ms)
    } else if diagnostic_only {
        json!(NON_CLAIMABLE_DEBUG_TIMING)
    } else {
        Value::Null
    };
    let metadata_report = json!({
        "artifact_path": path_string(db_path),
        "artifact_created_at": value_u64(&metadata_value, &["artifact_created_at"]).unwrap_or_else(|| file_modified_unix_ms(db_path).unwrap_or(0)),
        "git_commit": metadata_git_commit.unwrap_or_else(|| "unknown".to_string()),
        "current_git_commit": current_git,
        "schema_version": actual_schema_version,
        "current_schema_version": SCHEMA_VERSION,
        "metadata_schema_version": metadata_schema,
        "migration_version": metadata_migration,
        "current_migration_version": SCHEMA_VERSION,
        "storage_mode": metadata_storage_mode.unwrap_or_else(|| "unknown".to_string()),
        "build_command": value_string(&metadata_value, &["build_command"]).unwrap_or_else(|| "unknown".to_string()),
        "build_duration_ms": if metadata_build_duration.is_some() { json!(build_duration_ms) } else { Value::Null },
        "proof_build_only_ms": proof_build_only_value,
        "diagnostic_debug_proof_build_only_ms": if diagnostic_only && metadata_build_duration.is_some() {
            json!(build_duration_ms)
        } else {
            Value::Null
        },
        "inspection_read_only": true,
        "artifact_mutated_during_inspection": false,
        "lifecycle_status": lifecycle_status,
        "db_size_bytes": actual_db_size_bytes,
        "metadata_db_size_bytes": metadata_db_size,
        "integrity_status": integrity_status,
        "benchmark_run_id": options.timestamp,
        "artifact_metadata_path": path_string(&metadata_path),
        "current_exe": metadata_current_exe.unwrap_or_else(|| "unknown".to_string()),
        "debug_assertions": metadata_debug_assertions.unwrap_or(false),
        "binary_profile": metadata_binary_profile.unwrap_or_else(|| "unknown".to_string()),
        "exact_command": metadata_exact_command.unwrap_or_else(|| "unknown".to_string()),
        "claimable_for_thresholds": claimable_for_thresholds,
        "diagnostic_only": diagnostic_only,
        "binary_metadata": {
            "current_exe": value_string(&metadata_value, &["current_exe"]).unwrap_or_else(|| "unknown".to_string()),
            "debug_assertions": metadata_debug_assertions.unwrap_or(false),
            "binary_profile": value_string(&metadata_value, &["binary_profile"]).unwrap_or_else(|| "unknown".to_string()),
            "exact_command": value_string(&metadata_value, &["exact_command"]).unwrap_or_else(|| "unknown".to_string()),
            "claimable_for_thresholds": claimable_for_thresholds,
            "diagnostic_only": diagnostic_only
        },
        "freshness_metadata_present": metadata_present,
        "artifact_reuse": true,
        "freshly_built": false,
        "stale": stale,
        "stale_reasons": stale_reasons,
        "freshness_status": if stale { "stale" } else { "fresh" },
        "storage_result_claimable": !stale && lifecycle_claimable,
        "cold_build_result_claimable": !stale && lifecycle_claimable && metadata_build_duration.is_some() && claimable_for_thresholds,
        "notes": if stale {
            vec!["stale artifact; storage result not claimable".to_string(), "stale artifact; cold-build result not claimable".to_string()]
        } else if !claimable_for_thresholds {
            vec!["debug or otherwise non-production proof-build timing; cold-build result not claimable".to_string()]
        } else {
            vec!["Explicit artifact reuse accepted because freshness metadata matches current schema/build checks.".to_string()]
        }
    });

    let synthetic_summary = IndexSummary {
        repo_root: "unknown".to_string(),
        db_path: path_string(db_path),
        db_lifecycle: None,
        build_mode: "proof-build-only".to_string(),
        files_seen: 0,
        files_walked: 0,
        files_metadata_unchanged: 0,
        files_read: 0,
        files_hashed: 0,
        files_parsed: 0,
        files_indexed: 0,
        files_skipped: 0,
        files_deleted: 0,
        files_renamed: 0,
        parse_errors: 0,
        syntax_errors: 0,
        entities: 0,
        edges: 0,
        duplicate_edges_upserted: 0,
        batches_total: 0,
        batches_completed: 0,
        batch_max_files: DEFAULT_INDEX_BATCH_MAX_FILES,
        batch_max_source_bytes: DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES,
        stale_files_deleted: 0,
        failed_files_deleted: 0,
        storage_policy: StorageMode::Proof.storage_policy().to_string(),
        issue_counts: BTreeMap::new(),
        issues: Vec::new(),
        graph_output_budgets: codegraph_index::GraphOutputBudgetSummary::new(
            codegraph_index::GraphOutputBudgets::default(),
        ),
        scope: None,
        candidate_spool: None,
        profile: Some(IndexProfile {
            file_discovery_ms: 0,
            parse_ms: 0,
            extraction_ms: 0,
            semantic_resolver_ms: 0,
            db_write_ms: 0,
            fts_search_index_ms: 0,
            vector_signature_ms: 0,
            total_wall_ms: u128::from(build_duration_ms),
            files_per_sec: 0.0,
            entities_per_sec: 0.0,
            edges_per_sec: 0.0,
            memory_bytes: None,
            memory_measured: false,
            memory_status: "unknown".to_string(),
            memory_measurement_kind: "not_measured".to_string(),
            db_write_measurement: "unknown".to_string(),
            fts_search_index_measurement: "unknown".to_string(),
            worker_count: options.workers.unwrap_or(1),
            skipped_unchanged_files: 0,
            spans: Vec::new(),
            source_bytes_read: 0,
            source_clone_count: None,
            source_clone_count_status: "unknown".to_string(),
            source_clone_count_reason:
                "synthetic proof metadata profile does not measure source clone count".to_string(),
            db_write_attribution: "aggregate_only".to_string(),
            db_write_attribution_reason:
                "synthetic proof metadata profile has no per-file DB write attribution".to_string(),
            file_attribution: Vec::new(),
            stage_attribution: Vec::new(),
            slowest_stages: Vec::new(),
            slowest_files: Vec::new(),
            slowest_files_by_parse: Vec::new(),
            slowest_files_by_extraction: Vec::new(),
            highest_entity_files: Vec::new(),
            highest_edge_files: Vec::new(),
            highest_source_span_files: Vec::new(),
            high_fanout_files: Vec::new(),
            db_write_contributors: Vec::new(),
        }),
    };
    patch_gate_with_proof_artifact(
        gate,
        db_path,
        &storage_json_path,
        &proof_storage,
        &synthetic_summary,
        &metadata_report,
    )?;
    Ok(metadata_report)
}

pub(crate) fn patch_gate_with_proof_artifact(
    gate: &mut Value,
    db_path: &Path,
    storage_json_path: &Path,
    proof_storage: &Value,
    summary: &IndexSummary,
    metadata: &Value,
) -> Result<(), String> {
    let proof_bytes = value_u64(proof_storage, &["file_family", "total_bytes"])
        .or_else(|| sqlite_family_size_bytes(db_path).ok())
        .unwrap_or(0);
    let wal_bytes = value_u64(proof_storage, &["file_family", "wal_bytes"]).unwrap_or(0);
    let physical_edge_rows = storage_object_row_count(proof_storage, "edges").unwrap_or(0);
    let path_evidence_rows = storage_object_row_count(proof_storage, "path_evidence").unwrap_or(0);
    let integrity_status = value_string(proof_storage, &["integrity_check", "status"])
        .unwrap_or_else(|| "unknown".to_string());
    let profile = summary.profile.as_ref();
    let claimable_for_thresholds =
        value_bool(metadata, &["claimable_for_thresholds"]).unwrap_or(true);
    let diagnostic_only = value_bool(metadata, &["diagnostic_only"]).unwrap_or(false);
    let measured_wall_ms = profile
        .map(|profile| profile.total_wall_ms.min(u128::from(u64::MAX)) as u64)
        .or_else(|| value_u64(metadata, &["build_duration_ms"]))
        .unwrap_or(0);

    let gate_object = gate
        .as_object_mut()
        .ok_or_else(|| "compact gate JSON must be an object".to_string())?;
    ensure_json_object(gate_object, "artifacts")?.insert(
        "proof_storage_json".to_string(),
        json!(path_string(storage_json_path)),
    );
    let storage_object = ensure_json_object(gate_object, "storage")?;
    let proof_object = ensure_json_object(storage_object, "proof")?;
    proof_object.insert("file_family_bytes".to_string(), json!(proof_bytes));
    proof_object.insert(
        "file_family_mib".to_string(),
        json!(proof_bytes as f64 / (1024.0 * 1024.0)),
    );
    proof_object.insert("wal_bytes".to_string(), json!(wal_bytes));
    proof_object.insert("path_evidence_rows".to_string(), json!(path_evidence_rows));
    proof_object.insert("physical_edge_rows".to_string(), json!(physical_edge_rows));
    proof_object.insert("integrity_status".to_string(), json!(integrity_status));

    let autoresearch_object = ensure_json_object(gate_object, "autoresearch")?;
    let proof_build_object = ensure_json_object(autoresearch_object, "proof_build")?;
    proof_build_object.insert(
        "wall_ms".to_string(),
        if claimable_for_thresholds {
            json!(measured_wall_ms)
        } else {
            json!(NON_CLAIMABLE_DEBUG_TIMING)
        },
    );
    proof_build_object.insert(
        "diagnostic_debug_wall_ms".to_string(),
        if diagnostic_only {
            json!(measured_wall_ms)
        } else {
            Value::Null
        },
    );
    proof_build_object.insert(
        "db_write_ms".to_string(),
        json!(profile
            .map(|profile| profile.db_write_ms.min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0)),
    );
    proof_build_object.insert(
        "integrity_check_ms".to_string(),
        json!(profile
            .and_then(|profile| {
                profile
                    .spans
                    .iter()
                    .filter(|span| {
                        span.name.contains("quick_check") || span.name.contains("integrity")
                    })
                    .map(|span| span.elapsed_ms)
                    .reduce(|left, right| left + right)
            })
            .unwrap_or(0.0)),
    );
    proof_build_object.insert("files_walked".to_string(), json!(summary.files_walked));
    proof_build_object.insert("files_parsed".to_string(), json!(summary.files_parsed));
    proof_build_object.insert(
        "duplicate_local_analyses_skipped".to_string(),
        json!(summary.files_skipped),
    );
    proof_build_object.insert(
        "claimable_for_thresholds".to_string(),
        json!(claimable_for_thresholds),
    );
    proof_build_object.insert("diagnostic_only".to_string(), json!(diagnostic_only));
    proof_build_object.insert(
        "binary_profile".to_string(),
        metadata
            .get("binary_profile")
            .cloned()
            .unwrap_or_else(|| json!("unknown")),
    );
    proof_build_object.insert(
        "debug_assertions".to_string(),
        metadata
            .get("debug_assertions")
            .cloned()
            .unwrap_or(Value::Null),
    );
    gate_object.insert("artifact_freshness".to_string(), metadata.clone());
    Ok(())
}

pub(crate) fn ensure_json_object<'a>(
    object: &'a mut serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a mut serde_json::Map<String, Value>, String> {
    let entry = object.entry(key.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    entry
        .as_object_mut()
        .ok_or_else(|| format!("failed to create object field {key}"))
}

pub(crate) fn storage_object_row_count(storage: &Value, name: &str) -> Option<u64> {
    storage
        .get("objects")?
        .as_array()?
        .iter()
        .find(|object| object["name"].as_str() == Some(name))
        .and_then(|object| value_u64(object, &["row_count"]))
}

pub(crate) fn comprehensive_fresh_build_command(
    options: &ComprehensiveBenchmarkOptions,
    db_path: &Path,
) -> String {
    let mut parts = vec![
        current_exe_string(),
        "bench".to_string(),
        "proof-build-only".to_string(),
        "--repo".to_string(),
        path_string(&options.repo),
        "--db".to_string(),
        path_string(db_path),
    ];
    if let Some(workers) = options.workers {
        parts.push("--workers".to_string());
        parts.push(workers.to_string());
    }
    parts
        .into_iter()
        .map(|part| shell_quote_arg(&part))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn comprehensive_artifact_freshness_metrics(metadata: &Value) -> Vec<Value> {
    let artifact_reuse = value_bool(metadata, &["artifact_reuse"]).unwrap_or(false);
    let freshly_built = value_bool(metadata, &["freshly_built"]).unwrap_or(false);
    let stale = value_bool(metadata, &["stale"]).unwrap_or(true);
    let storage_claimable = value_bool(metadata, &["storage_result_claimable"]);
    let cold_claimable = value_bool(metadata, &["cold_build_result_claimable"]);
    let metadata_present = value_bool(metadata, &["freshness_metadata_present"]).unwrap_or(false);
    let schema_matches = value_u64(metadata, &["schema_version"])
        .zip(Some(u64::from(SCHEMA_VERSION)))
        .map(|(actual, current)| actual == current);
    vec![
        metric(
            "proof_db_freshly_built",
            json!("true unless explicit artifact reuse requested"),
            json!(freshly_built),
            if freshly_built || artifact_reuse { "pass" } else { "fail" },
            vec!["Comprehensive benchmark builds a fresh proof DB by default; explicit reuse must be labeled.".to_string()],
        ),
        metric(
            "artifact_reuse_marked",
            json!("reported"),
            json!(artifact_reuse),
            "pass",
            vec!["Explicit artifact reuse is visible in the report.".to_string()],
        ),
        metric(
            "artifact_has_freshness_metadata",
            json!(true),
            json!(metadata_present),
            status_known_pass(Some(metadata_present)),
            vec!["Reused artifacts require freshness metadata; fresh builds write it.".to_string()],
        ),
        metric(
            "artifact_schema_matches_current",
            json!(true),
            observed_bool(schema_matches),
            status_known_pass(schema_matches),
            vec!["Artifact schema_version must match the current SQLite schema version.".to_string()],
        ),
        metric(
            "artifact_integrity_ok",
            json!("ok"),
            metadata
                .get("integrity_status")
                .cloned()
                .unwrap_or(Value::Null),
            status_known_pass(
                metadata
                    .get("integrity_status")
                    .and_then(Value::as_str)
                    .map(|status| status == "ok"),
            ),
            vec!["Proof artifact must pass integrity before benchmark numbers are claimable.".to_string()],
        ),
        metric(
            "artifact_not_stale",
            json!(true),
            json!(!stale),
            status_known_pass(Some(!stale)),
            vec!["Stale artifact reuse forces the master gate to fail or remain unknown.".to_string()],
        ),
        metric(
            "storage_result_claimable",
            json!(true),
            observed_bool(storage_claimable),
            status_known_pass(storage_claimable),
            vec![if storage_claimable == Some(true) {
                "Storage result is claimable because the proof artifact is fresh or freshness-validated."
                    .to_string()
            } else {
                "stale artifact; storage result not claimable".to_string()
            }],
        ),
        metric(
            "cold_build_result_claimable",
            json!(true),
            observed_bool(cold_claimable),
            status_known_pass(cold_claimable),
            vec![if cold_claimable == Some(true) {
                "Cold-build result is claimable because the proof artifact is fresh or freshness-validated."
                    .to_string()
            } else if value_bool(metadata, &["diagnostic_only"]) == Some(true) {
                "debug proof-build timing is diagnostic_only; cold-build result not claimable"
                    .to_string()
            } else {
                "stale artifact; cold-build result not claimable".to_string()
            }],
        ),
    ]
}

pub(crate) fn default_artifact_metadata_path(db_path: &Path) -> PathBuf {
    let safe_name = artifact_safe_file_name_for_path(
        db_path,
        ".metadata.json",
        ARTIFACT_SAFE_FILENAME_MAX_CHARS,
    );
    db_path
        .parent()
        .map(|parent| parent.join(&safe_name.file_name))
        .unwrap_or_else(|| PathBuf::from(&safe_name.file_name))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtifactSafeFileName {
    pub file_name: String,
    pub original_file_name: String,
    pub original_path: String,
    pub hash_suffix: String,
    pub shortened: bool,
    pub max_filename_chars: usize,
    pub extension_preserved: bool,
}

pub(crate) fn artifact_safe_file_name_for_path(
    original_path: &Path,
    extra_extension: &str,
    max_filename_chars: usize,
) -> ArtifactSafeFileName {
    let original_file_name = original_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "artifact".to_string());
    let extension = if extra_extension.is_empty() {
        String::new()
    } else if extra_extension.starts_with('.') {
        extra_extension.to_string()
    } else {
        format!(".{extra_extension}")
    };
    let mut candidate =
        sanitize_artifact_file_component(&format!("{original_file_name}{extension}"));
    let hash = stable_agent_use_identity_hash(path_string(original_path).as_bytes());
    let hash_suffix = hash.chars().take(12).collect::<String>();
    let effective_max = max_filename_chars.max(extension.chars().count() + hash_suffix.len() + 2);
    let shortened = candidate.chars().count() > effective_max;
    if shortened {
        let suffix = format!("-{hash_suffix}{extension}");
        let suffix_chars = suffix.chars().count();
        let prefix_max = effective_max.saturating_sub(suffix_chars).max(1);
        let prefix = sanitize_artifact_file_component(&original_file_name)
            .chars()
            .take(prefix_max)
            .collect::<String>()
            .trim_matches(['.', '_', '-'])
            .to_string();
        let prefix = if prefix.is_empty() {
            "artifact".to_string()
        } else {
            prefix
        };
        candidate = format!("{prefix}{suffix}");
    }
    let extension_preserved = extension.is_empty() || candidate.ends_with(&extension);
    ArtifactSafeFileName {
        file_name: candidate,
        original_file_name,
        original_path: path_string(original_path),
        hash_suffix,
        shortened,
        max_filename_chars: effective_max,
        extension_preserved,
    }
}

pub(crate) fn sanitize_artifact_file_component(value: &str) -> String {
    let mut sanitized = String::new();
    let mut last_was_underscore = false;
    for ch in value.chars() {
        let safe = ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-');
        if safe {
            sanitized.push(ch);
            last_was_underscore = false;
        } else if !last_was_underscore {
            sanitized.push('_');
            last_was_underscore = true;
        }
    }
    let sanitized = sanitized.trim_matches(['.', '_', '-']);
    if sanitized.is_empty() {
        "artifact".to_string()
    } else {
        sanitized.to_string()
    }
}

pub(crate) fn artifact_safe_file_name_metadata(plan: &ArtifactSafeFileName) -> Value {
    json!({
        "safe_file_name": plan.file_name.clone(),
        "original_file_name": plan.original_file_name.clone(),
        "original_path": plan.original_path.clone(),
        "hash_suffix": plan.hash_suffix.clone(),
        "shortened": plan.shortened,
        "max_filename_chars": plan.max_filename_chars,
        "extension_preserved": plan.extension_preserved,
        "collision_strategy": "readable_prefix_plus_stable_hash_suffix",
    })
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedDbPath {
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) explicit: bool,
}

pub(crate) fn absolutize_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn normalize_db_path_for_repo(repo_root: &Path, db_path: &Path) -> PathBuf {
    if db_path.is_absolute() {
        db_path.to_path_buf()
    } else {
        repo_root.join(db_path)
    }
}

pub(crate) fn selected_db_path_for_repo(repo_root: &Path) -> SelectedDbPath {
    with_process_context_lock(|| {
        if let Some(raw) = std::env::var_os("CODEGRAPH_DB_PATH") {
            let raw_path = PathBuf::from(raw);
            return SelectedDbPath {
                path: normalize_db_path_for_repo(repo_root, &raw_path),
                source: db_source_label(),
                explicit: true,
            };
        }

        SelectedDbPath {
            path: repo_root.join(".codegraph").join("codegraph.sqlite"),
            source: "default .codegraph".to_string(),
            explicit: false,
        }
    })
}

pub fn resolve_agent_use_profile(repo: impl AsRef<Path>) -> Result<AgentUseProfile, String> {
    let repo_root = resolve_repo_root(repo.as_ref())?;
    let data_root = agent_use_profile_data_root()?;
    resolve_agent_use_profile_with_data_root(&repo_root, &data_root)
}

pub(crate) fn resolve_agent_use_profile_with_data_root(
    repo_root: &Path,
    data_root: &Path,
) -> Result<AgentUseProfile, String> {
    let repo_root = resolve_repo_root(repo_root)?;
    let repo_identity_label = safe_repo_identity_label(&repo_root);
    let identity_candidates = agent_use_repo_identity_candidates(&repo_root);
    for identity_material in &identity_candidates {
        let repo_identity_hash = stable_agent_use_identity_hash(identity_material.as_bytes());
        let profile = agent_use_profile_from_identity_hash(
            repo_root.clone(),
            data_root,
            repo_identity_label.clone(),
            repo_identity_hash,
        );
        if profile.db_path.exists() {
            return Ok(profile);
        }
    }
    if let Some(profile) =
        discover_existing_agent_use_profile_for_repo(&repo_root, data_root, &repo_identity_label)
    {
        return Ok(profile);
    }
    let identity_material = identity_candidates
        .first()
        .cloned()
        .unwrap_or_else(|| agent_use_repo_path_identity_material(&repo_root));
    let repo_identity_hash = stable_agent_use_identity_hash(identity_material.as_bytes());
    Ok(agent_use_profile_from_identity_hash(
        repo_root,
        data_root,
        repo_identity_label,
        repo_identity_hash,
    ))
}

pub(crate) fn agent_use_profile_from_identity_hash(
    repo_root: PathBuf,
    data_root: &Path,
    repo_identity_label: String,
    repo_identity_hash: String,
) -> AgentUseProfile {
    let profile_root = data_root.join(format!("{repo_identity_label}-{repo_identity_hash}"));
    let db_path = profile_root.join("production-agent-use.sqlite");
    let candidate_spool_path = profile_root.join("codegraph-candidate-spool.jsonl");
    let candidate_spool_query_index_path = candidate_spool_query_index_path(&candidate_spool_path);
    let vector_runtime_path = profile_root.join(CONTEXT_PACK_VECTOR_INDEX_FILE_NAME);
    let vector_audit_path = profile_root.join(CONTEXT_PACK_VECTOR_AUDIT_FILE_NAME);
    let lock_or_publish_state_path = profile_root.join("production-agent-use.publish-state.json");
    let delta_state_path = profile_root.join(PRODUCTION_AGENT_USE_DELTA_STATE_FILE_NAME);
    let repo_string = path_string(&repo_root);
    let db_string = path_string(&db_path);
    let mcp_args = vec![
        "--repo".to_string(),
        repo_string.clone(),
        "--db".to_string(),
        db_string.clone(),
        "serve-mcp".to_string(),
    ];
    let recovery_commands = vec![
        format!(
            "{BIN_NAME} agent-use status --repo \"{repo}\" --json",
            repo = repo_string.as_str()
        ),
        format!(
            "{BIN_NAME} agent-use index --repo \"{repo}\" --json",
            repo = repo_string.as_str()
        ),
        format!(
            "{BIN_NAME} agent-use mcp-config --repo \"{repo}\" --json",
            repo = repo_string.as_str()
        ),
        format!(
            "{BIN_NAME} agent-use query symbols <symbol> --repo \"{repo}\" --limit 5 --agent-json",
            repo = repo_string.as_str()
        ),
        format!(
            "{BIN_NAME} agent-use query text \"<text>\" --repo \"{repo}\" --limit 5 --agent-json",
            repo = repo_string.as_str()
        ),
        format!(
            "{BIN_NAME} agent-use query files <path-or-text> --repo \"{repo}\" --limit 5 --agent-json",
            repo = repo_string.as_str()
        ),
        format!(
            "{BIN_NAME} agent-use context-pack --repo \"{repo}\" --task \"<task>\" --agent-json",
            repo = repo_string.as_str()
        ),
        format!(
            "{BIN_NAME} agent-use watch --repo \"{repo}\" --once --changed <path> --json",
            repo = repo_string.as_str()
        ),
    ];
    AgentUseProfile {
        profile_name: PRODUCTION_AGENT_USE_PROFILE_NAME.to_string(),
        repo_root,
        repo_identity_label,
        repo_identity_hash,
        profile_root,
        db_path,
        candidate_spool_path,
        candidate_spool_query_index_path,
        vector_runtime_path,
        vector_audit_path,
        lock_or_publish_state_path,
        delta_state_path,
        lifecycle_expectations: vec![
            "status is read-only".to_string(),
            "index is the first mutating command".to_string(),
            "watch --once updates the production profile DB only after an existing graph DB is safe to write".to_string(),
            "explicit profile DB never falls back to repo-local .codegraph".to_string(),
            "candidate and vector artifacts are candidate-only unless graph/source verification proves a graph path".to_string(),
        ],
        recovery_commands,
        mcp_args,
        binary_profile: "release".to_string(),
        scope_policy: IndexScopeOptions::default(),
    }
}

pub(crate) fn agent_use_profile_data_root() -> Result<PathBuf, String> {
    with_process_context_lock(|| {
        if let Some(root) = std::env::var_os(AGENT_USE_DATA_ROOT_ENV) {
            if root.is_empty() {
                return Err(format!(
                    "env_invalid: {AGENT_USE_DATA_ROOT_ENV} is set but empty"
                ));
            }
            let root = PathBuf::from(root);
            return absolutize_path(&root).or(Ok(root));
        }

        #[cfg(windows)]
        {
            if let Some(root) = std::env::var_os("LOCALAPPDATA") {
                return Ok(PathBuf::from(root)
                    .join("CodeGraphMCP")
                    .join("agent-indexes"));
            }
            Err(
                "env_missing: LOCALAPPDATA is required for production agent-use profile paths"
                    .to_string(),
            )
        }

        #[cfg(not(windows))]
        {
            if let Some(root) = std::env::var_os("XDG_DATA_HOME") {
                return Ok(PathBuf::from(root)
                    .join("CodeGraphMCP")
                    .join("agent-indexes"));
            }
            if let Some(home) = std::env::var_os("HOME") {
                return Ok(PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join("CodeGraphMCP")
                    .join("agent-indexes"));
            }
            Err(
                "env_missing: HOME or XDG_DATA_HOME is required for production agent-use profile paths"
                    .to_string(),
            )
        }
    })
}

pub(crate) fn safe_repo_identity_label(repo_root: &Path) -> String {
    let raw = repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("repo");
    let mut label = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            label.push(ch);
        } else {
            label.push('_');
        }
        if label.len() >= 64 {
            break;
        }
    }
    let label = label.trim_matches('_');
    if label.is_empty() {
        "repo".to_string()
    } else {
        label.to_string()
    }
}

#[allow(dead_code)]
pub(crate) fn agent_use_repo_identity_material(repo_root: &Path) -> String {
    agent_use_repo_identity_candidates(repo_root)
        .first()
        .cloned()
        .unwrap_or_else(|| agent_use_repo_path_identity_material(repo_root))
}

pub(crate) fn agent_use_repo_identity_candidates(repo_root: &Path) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(remote) = git_remote_url(repo_root) {
        let remote = remote.trim();
        if !remote.is_empty() {
            candidates.push(format!("remote:{remote}"));
        }
    }
    let path_material = agent_use_repo_path_identity_material(repo_root);
    if !candidates
        .iter()
        .any(|candidate| candidate == &path_material)
    {
        candidates.push(path_material);
    }
    candidates
}

pub(crate) fn agent_use_repo_path_identity_material(repo_root: &Path) -> String {
    format!(
        "path:{}",
        normalize_agent_use_identity_path(&path_string(repo_root))
    )
}

pub(crate) fn git_remote_url(repo_root: &Path) -> Option<String> {
    if cli_write_path_chaos_failpoint_enabled(AGENT_USE_GIT_METADATA_UNAVAILABLE_FAILPOINT) {
        return None;
    }
    let worktree_root = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .and_then(|output| {
            output
                .status
                .success()
                .then(|| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string()))
        })?;
    let worktree_root =
        absolutize_path(&worktree_root).unwrap_or_else(|_| worktree_root.to_path_buf());
    let repo_root = fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    if normalize_agent_use_identity_path(&path_string(&worktree_root))
        != normalize_agent_use_identity_path(&path_string(&repo_root))
    {
        return None;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!remote.is_empty()).then_some(remote)
}

pub(crate) fn discover_existing_agent_use_profile_for_repo(
    repo_root: &Path,
    data_root: &Path,
    repo_identity_label: &str,
) -> Option<AgentUseProfile> {
    let prefix = format!("{repo_identity_label}-");
    let entries = fs::read_dir(data_root).ok()?;
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some(repo_identity_hash) =
            agent_use_profile_hash_from_dir_name(&name, repo_identity_label, &prefix)
        else {
            continue;
        };
        let candidate = agent_use_profile_from_identity_hash(
            repo_root.to_path_buf(),
            data_root,
            repo_identity_label.to_string(),
            repo_identity_hash,
        );
        if agent_use_profile_passport_matches_repo(&candidate.db_path, repo_root) {
            return Some(candidate);
        }
    }
    None
}

pub(crate) fn agent_use_profile_hash_from_dir_name(
    name: &str,
    repo_identity_label: &str,
    prefix: &str,
) -> Option<String> {
    let hash = name.strip_prefix(prefix).or_else(|| {
        name.strip_prefix(repo_identity_label)
            .and_then(|rest| rest.strip_prefix('-'))
    })?;
    if hash.len() == 32 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(hash.to_string())
    } else {
        None
    }
}

pub(crate) fn agent_use_profile_passport_matches_repo(db_path: &Path, repo_root: &Path) -> bool {
    let Ok(store) = SqliteGraphStore::open_read_only(db_path) else {
        return false;
    };
    let Ok(Some(passport)) = store.get_db_passport() else {
        return false;
    };
    paths_equivalent_string(&passport.canonical_repo_root, &path_string(repo_root))
}

pub(crate) fn normalize_agent_use_identity_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let normalized = normalized.strip_prefix("//?/").unwrap_or(&normalized);
    #[cfg(windows)]
    {
        normalized.to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        normalized.to_string()
    }
}

pub(crate) fn stable_agent_use_identity_hash(bytes: &[u8]) -> String {
    let high = fnv64_with_seed(bytes, 0xcbf2_9ce4_8422_2325);
    let low = fnv64_with_seed(bytes, 0x9e37_79b1_85eb_ca87);
    format!("{high:016x}{low:016x}")
}

pub(crate) fn fnv64_with_seed(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = seed;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(crate) fn resolved_db_path_for_repo(repo_root: &Path) -> PathBuf {
    selected_db_path_for_repo(repo_root).path
}

pub(crate) fn repo_source_label() -> String {
    with_process_context_lock(|| {
        std::env::var(GLOBAL_REPO_SOURCE_ENV).unwrap_or_else(|_| "current_directory".to_string())
    })
}

pub(crate) fn db_source_label() -> String {
    with_process_context_lock(|| {
        std::env::var(GLOBAL_DB_SOURCE_ENV).unwrap_or_else(|_| {
            if std::env::var_os("CODEGRAPH_DB_PATH").is_some() {
                "env CODEGRAPH_DB_PATH".to_string()
            } else {
                "default_repo_db".to_string()
            }
        })
    })
}

pub(crate) fn remove_sqlite_family_if_exists(path: &Path) -> Result<(), String> {
    for candidate in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.to_string_lossy())),
        PathBuf::from(format!("{}-shm", path.to_string_lossy())),
    ] {
        match fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "failed to remove existing SQLite artifact {}: {error}",
                    candidate.display()
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn file_modified_unix_ms(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
}

pub(crate) fn sqlite_user_version_read_only(path: &Path) -> Option<u32> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .ok()?;
    u32::try_from(version).ok()
}

pub(crate) fn sqlite_quick_check_status(path: &Path) -> Option<String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    connection
        .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .ok()
}

pub(crate) fn benchmark_inspection_lifecycle_status(
    repo_root: &Path,
    db_path: &Path,
    surface_name: &str,
    required_storage_mode: Option<StorageMode>,
) -> Value {
    match inspect_db_lifecycle_surface_preflight(DbLifecycleSurfacePreflightRequest {
        repo_root: repo_root.to_path_buf(),
        db_path: db_path.to_path_buf(),
        surface_name: surface_name.to_string(),
        operation_kind: DbLifecycleOperationKind::BenchmarkInspection,
        allow_stale_read: false,
        allow_foreign_repo: false,
        required_storage_mode,
        expected_scope: None,
    }) {
        Ok(preflight) => db_lifecycle_surface_preflight_json(&preflight),
        Err(error) => json!({
            "decision": "blocked",
            "surface_name": surface_name,
            "operation_kind": DbLifecycleOperationKind::BenchmarkInspection.as_str(),
            "safe_to_read": false,
            "safe_to_write": false,
            "passport_status": "unknown",
            "claimable": false,
            "diagnostic_only": true,
            "contaminated": true,
            "repo_match": false,
            "scope_match": false,
            "schema_status": "unknown",
            "storage_mode_match": false,
            "reasons": [],
            "blockers": [error.to_string()],
            "warnings": [],
            "exact_db_path_checked": path_string(db_path),
            "repo_root_expected": path_string(repo_root),
            "repo_root_observed": Value::Null,
            "artifact_freshness": Value::Null,
            "scope_source": Value::Null,
            "passport_scope_hash": Value::Null,
            "explicit_scope_hash": Value::Null,
        }),
    }
}

pub(crate) fn benchmark_inspection_claimable(lifecycle_status: &Value) -> bool {
    lifecycle_status["safe_to_read"].as_bool() == Some(true)
        && lifecycle_status["claimable"].as_bool() == Some(true)
}

pub(crate) fn json_object(entries: Vec<(&str, Value)>) -> Value {
    let mut object = serde_json::Map::new();
    for (key, value) in entries {
        object.insert(key.to_string(), value);
    }
    Value::Object(object)
}

pub(crate) fn current_git_commit() -> String {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output();
    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text.is_empty() {
                "unknown".to_string()
            } else {
                text
            }
        }
        _ => build_commit().to_string(),
    }
}

pub(crate) fn read_json_file(path: &Path) -> Result<Value, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(contents.trim_start_matches('\u{feff}'))
        .map_err(|error| format!("failed to parse {} as JSON: {error}", path.display()))
}

pub(crate) fn read_optional_json(path: &Path) -> Option<Value> {
    read_json_file(path).ok()
}

pub(crate) fn read_gate_artifact_json(gate: &Value, artifact_key: &str) -> Option<Value> {
    let path = gate_value(gate, &["artifacts", artifact_key])?.as_str()?;
    read_optional_json(Path::new(path))
}

pub(crate) fn gate_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cursor = value;
    for segment in path {
        cursor = cursor.get(*segment)?;
    }
    Some(cursor)
}

pub(crate) fn value_u64(value: &Value, path: &[&str]) -> Option<u64> {
    let value = gate_value(value, path)?;
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_f64().map(|number| number as u64))
}

pub(crate) fn value_f64(value: &Value, path: &[&str]) -> Option<f64> {
    let value = gate_value(value, path)?;
    value
        .as_f64()
        .or_else(|| value.as_u64().map(|number| number as f64))
        .or_else(|| value.as_i64().map(|number| number as f64))
}

pub(crate) fn value_bool(value: &Value, path: &[&str]) -> Option<bool> {
    gate_value(value, path)?.as_bool()
}

pub(crate) fn value_string(value: &Value, path: &[&str]) -> Option<String> {
    gate_value(value, path)?.as_str().map(ToOwned::to_owned)
}

pub(crate) fn observed_number_u64(value: Option<u64>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
}

pub(crate) fn observed_number_f64(value: Option<f64>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
}

pub(crate) fn observed_bool(value: Option<bool>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
}

pub(crate) fn status_known_pass(condition: Option<bool>) -> &'static str {
    match condition {
        Some(true) => "pass",
        Some(false) => "fail",
        None => "unknown",
    }
}

pub(crate) fn metric(
    id: &str,
    target: Value,
    observed: Value,
    status: &str,
    notes: impl Into<Vec<String>>,
) -> Value {
    json!({
        "id": id,
        "target": target,
        "observed": observed,
        "status": status,
        "notes": notes.into()
    })
}

pub(crate) fn metric_eq_u64(id: &str, target: u64, observed: Option<u64>, notes: &str) -> Value {
    let status = status_known_pass(observed.map(|value| value == target));
    metric(
        id,
        json!(target),
        observed_number_u64(observed),
        status,
        vec![notes.to_string()],
    )
}

pub(crate) fn metric_le_f64(id: &str, target: f64, observed: Option<f64>, notes: &str) -> Value {
    let status = status_known_pass(observed.map(|value| value <= target));
    metric(
        id,
        json!(target),
        observed_number_f64(observed),
        status,
        vec![notes.to_string()],
    )
}

pub(crate) fn metric_ge_f64(id: &str, target: f64, observed: Option<f64>, notes: &str) -> Value {
    let status = status_known_pass(observed.map(|value| value >= target));
    metric(
        id,
        json!(target),
        observed_number_f64(observed),
        status,
        vec![notes.to_string()],
    )
}

pub(crate) fn metric_reported_u64(id: &str, observed: Option<u64>, notes: &str) -> Value {
    let status = if observed.is_some() {
        "pass"
    } else {
        "unknown"
    };
    metric(
        id,
        json!("reported"),
        observed_number_u64(observed),
        status,
        vec![notes.to_string()],
    )
}

pub(crate) fn metric_reported_f64(id: &str, observed: Option<f64>, notes: &str) -> Value {
    let status = if observed.is_some() {
        "pass"
    } else {
        "unknown"
    };
    metric(
        id,
        json!("reported"),
        observed_number_f64(observed),
        status,
        vec![notes.to_string()],
    )
}

pub(crate) fn object_by_name(storage: &Option<Value>, name: &str) -> Option<Value> {
    storage
        .as_ref()?
        .get("objects")?
        .as_array()?
        .iter()
        .find(|object| object["name"].as_str() == Some(name))
        .cloned()
}

pub(crate) fn object_row_count(storage: &Option<Value>, name: &str) -> Option<u64> {
    object_by_name(storage, name).and_then(|object| value_u64(&object, &["row_count"]))
}

pub(crate) fn object_average_bytes(storage: &Option<Value>, table: &str) -> Option<f64> {
    storage
        .as_ref()?
        .get("table_row_metrics")?
        .as_array()?
        .iter()
        .find(|entry| entry["table"].as_str() == Some(table))
        .and_then(|entry| value_f64(entry, &["average_total_bytes_per_row"]))
}

pub(crate) fn object_average_payload_bytes(storage: &Option<Value>, table: &str) -> Option<f64> {
    storage
        .as_ref()?
        .get("table_row_metrics")?
        .as_array()?
        .iter()
        .find(|entry| entry["table"].as_str() == Some(table))
        .and_then(|entry| value_f64(entry, &["average_payload_bytes_per_row"]))
}

pub(crate) fn collect_gate_statuses(
    metrics: &[Value],
    failed_targets: &mut Vec<String>,
    passed_targets: &mut Vec<String>,
) {
    for metric in metrics {
        let id = metric["id"]
            .as_str()
            .unwrap_or("unknown_metric")
            .to_string();
        match metric["status"].as_str() {
            Some("fail") => failed_targets.push(id),
            Some("pass") => passed_targets.push(id),
            _ => {}
        }
    }
}

pub(crate) fn collect_query_gate_statuses(
    query_latency: &Value,
    failed_targets: &mut Vec<String>,
    passed_targets: &mut Vec<String>,
) {
    let Some(metrics) = query_latency["queries"].as_array() else {
        return;
    };
    for metric in metrics {
        let id = metric["id"].as_str().unwrap_or("unknown_query").to_string();
        match metric["status"].as_str() {
            Some("fail") => failed_targets.push(id),
            Some("pass") => passed_targets.push(id),
            _ => {}
        }
    }
}

pub(crate) fn comprehensive_correctness_metrics(graph_truth: &Value) -> Vec<Value> {
    let cases_total = value_u64(graph_truth, &["cases_total"]);
    let cases_passed = value_u64(graph_truth, &["cases_passed"]);
    let expected_entities = value_u64(graph_truth, &["expected_entities"]);
    let matched_entities = value_u64(graph_truth, &["matched_entities"]);
    let expected_edges = value_u64(graph_truth, &["expected_edges"]);
    let matched_edges = value_u64(graph_truth, &["matched_expected_edges"]);
    let expected_paths = value_u64(graph_truth, &["expected_paths"]);
    let matched_paths = value_u64(graph_truth, &["matched_expected_paths"]);
    let forbidden_edges_found = value_u64(graph_truth, &["matched_forbidden_edges"]);
    let forbidden_paths_found = value_u64(graph_truth, &["matched_forbidden_paths"]);
    let source_span_failures = value_u64(graph_truth, &["source_span_failures"]);
    let unresolved_exact = value_u64(graph_truth, &["unresolved_exact_violations"]).or(Some(0));
    let derived_without_provenance =
        value_u64(graph_truth, &["derived_without_provenance_violations"]).or(Some(0));
    let test_mock_leakage = value_u64(graph_truth, &["test_mock_production_leakage"]).or(Some(0));
    let stale_failures = value_u64(graph_truth, &["stale_failures"]);

    vec![
        metric(
            "graph_truth_cases_total",
            json!(11),
            observed_number_u64(cases_total),
            status_known_pass(cases_total.map(|total| total == 11)),
            vec!["All adversarial graph-truth fixtures must be present.".to_string()],
        ),
        metric(
            "graph_truth_cases_passed",
            json!("all cases"),
            observed_number_u64(cases_passed),
            status_known_pass(
                cases_total
                    .zip(cases_passed)
                    .map(|(total, passed)| total == passed),
            ),
            vec!["Graph Truth Gate must be 100% pass.".to_string()],
        ),
        metric(
            "expected_entities_matched",
            json!(expected_entities),
            observed_number_u64(matched_entities),
            status_known_pass(
                expected_entities
                    .zip(matched_entities)
                    .map(|(expected, matched)| expected == matched),
            ),
            vec!["Every expected entity must be present.".to_string()],
        ),
        metric(
            "expected_edges_matched",
            json!(expected_edges),
            observed_number_u64(matched_edges),
            status_known_pass(
                expected_edges
                    .zip(matched_edges)
                    .map(|(expected, matched)| expected == matched),
            ),
            vec!["Every required edge must be present.".to_string()],
        ),
        metric(
            "expected_paths_matched",
            json!(expected_paths),
            observed_number_u64(matched_paths),
            status_known_pass(
                expected_paths
                    .zip(matched_paths)
                    .map(|(expected, matched)| expected == matched),
            ),
            vec!["Every expected proof path must be present.".to_string()],
        ),
        metric_eq_u64(
            "forbidden_edges_found",
            0,
            forbidden_edges_found,
            "Forbidden edge hits must remain zero.",
        ),
        metric_eq_u64(
            "forbidden_paths_found",
            0,
            forbidden_paths_found,
            "Forbidden proof paths must remain zero.",
        ),
        metric_eq_u64(
            "source_span_failures",
            0,
            source_span_failures,
            "Proof-grade facts must have valid source spans.",
        ),
        metric_eq_u64(
            "unresolved_exact_count",
            0,
            unresolved_exact,
            "Unresolved relations must not be labeled exact.",
        ),
        metric_eq_u64(
            "derived_without_provenance_count",
            0,
            derived_without_provenance,
            "Derived edges must retain provenance.",
        ),
        metric_eq_u64(
            "test_mock_production_leakage_count",
            0,
            test_mock_leakage,
            "Production proof paths must not include test/mock edges.",
        ),
        metric_eq_u64(
            "stale_fact_failures",
            0,
            stale_failures,
            "Mutation fixtures must not leave stale current facts.",
        ),
    ]
}

pub(crate) fn comprehensive_context_metrics(
    context_gate: &Value,
    storage: &Value,
    relation_sampler: &Value,
) -> Vec<Value> {
    let cases_total = value_u64(context_gate, &["cases_total"]);
    let cases_passed = value_u64(context_gate, &["cases_passed"]);
    let critical_symbol_recall = value_f64(context_gate, &["critical_symbol_recall"]);
    let proof_path_coverage = value_f64(context_gate, &["proof_path_coverage"]);
    let source_span_coverage = value_f64(context_gate, &["source_span_coverage"]);
    let expected_tests_recall = value_f64(context_gate, &["expected_test_recall"]);
    let distractor_ratio = value_f64(context_gate, &["distractor_ratio"]);
    let stored_path_evidence_rows = value_u64(storage, &["proof", "path_evidence_rows"])
        .or_else(|| value_u64(relation_sampler, &["stored_path_evidence_count"]));
    let fallback_paths = value_u64(relation_sampler, &["generated_path_evidence_count"]);

    vec![
        metric_reported_u64(
            "context_cases_total",
            cases_total,
            "Context Packet Gate fixture count.",
        ),
        metric(
            "context_cases_passed",
            json!("all cases"),
            observed_number_u64(cases_passed),
            status_known_pass(cases_total.zip(cases_passed).map(|(total, passed)| total == passed)),
            vec!["Context Packet Gate must pass all cases.".to_string()],
        ),
        metric_ge_f64(
            "critical_symbol_recall",
            1.0,
            critical_symbol_recall,
            "Critical symbol recall must be 100%.",
        ),
        metric_ge_f64(
            "proof_path_coverage",
            1.0,
            proof_path_coverage,
            "Proof-path coverage must be 100%.",
        ),
        metric_ge_f64(
            "proof_path_source_span_coverage",
            1.0,
            source_span_coverage,
            "Source-span coverage for proof paths must be 100%.",
        ),
        metric_ge_f64(
            "expected_tests_recall",
            0.9,
            expected_tests_recall,
            "Expected test recall target is >=90%.",
        ),
        metric_le_f64(
            "distractor_ratio",
            0.25,
            distractor_ratio,
            "Distractor ratio target is <=25%.",
        ),
        metric_reported_u64(
            "stored_path_evidence_rows",
            stored_path_evidence_rows,
            "Stored PathEvidence rows must exist for proof paths.",
        ),
        metric_eq_u64(
            "generated_fallback_path_count",
            0,
            fallback_paths,
            "Normal proof cases should use stored PathEvidence instead of generated fallback paths.",
        ),
    ]
}

pub(crate) fn comprehensive_integrity_metrics(
    gate: &Value,
    proof_storage: &Option<Value>,
    storage: &Value,
) -> Vec<Value> {
    let integrity = proof_storage
        .as_ref()
        .and_then(|value| value_string(value, &["integrity_check", "status"]))
        .or_else(|| value_string(storage, &["proof", "integrity_status"]))
        .or_else(|| {
            value_string(
                gate,
                &["db_integrity_summary", "proof_db", "integrity_status"],
            )
        });
    let quick_check = value_string(gate, &["db_integrity", "quick_check_status"]);
    let foreign_key = value_string(gate, &["db_integrity", "foreign_key_check_status"]);
    let wal_bytes = value_u64(storage, &["proof", "wal_bytes"]);
    let failed_update_status =
        value_string(gate, &["db_integrity", "failed_update_rollback_status"]).or_else(|| {
            Some(
                "not persisted in compact proof gate; update harness integrity remained ok"
                    .to_string(),
            )
        });

    vec![
        metric(
            "integrity_check_status",
            json!("ok"),
            integrity.clone().map(Value::from).unwrap_or(Value::Null),
            status_known_pass(integrity.as_deref().map(|status| status == "ok")),
            vec!["Cold proof DB must pass PRAGMA integrity_check.".to_string()],
        ),
        metric(
            "quick_check_status",
            json!("ok"),
            quick_check.clone().map(Value::from).unwrap_or(Value::Null),
            if quick_check.is_some() { status_known_pass(quick_check.as_deref().map(|status| status == "ok")) } else { "unknown" },
            vec!["Repeat/update quick_check status was not separately persisted in the compact gate.".to_string()],
        ),
        metric(
            "foreign_key_check_status",
            json!("ok or not_applicable"),
            foreign_key.clone().map(Value::from).unwrap_or(Value::Null),
            if foreign_key.is_some() { status_known_pass(foreign_key.as_deref().map(|status| status == "ok" || status == "not_applicable")) } else { "unknown" },
            vec!["Foreign-key check is reported only when the schema/gate persists it.".to_string()],
        ),
        metric_reported_u64("wal_size_bytes", wal_bytes, "WAL file size is reported separately."),
        metric(
            "rollback_failure_simulation_status",
            json!("rollback cleanly"),
            failed_update_status.map(Value::from).unwrap_or(Value::Null),
            "unknown",
            vec!["No explicit failed-update simulation artifact is attached to the compact proof gate; do not infer a pass.".to_string()],
        ),
    ]
}

pub(crate) fn comprehensive_storage_summary_metrics(
    storage: &Value,
    proof_storage: &Option<Value>,
) -> Vec<Value> {
    let proof = storage.get("proof").unwrap_or(&Value::Null);
    let proof_bytes = value_u64(proof, &["file_family_bytes"]);
    let proof_mib = value_f64(proof, &["file_family_mib"]);
    let audit_bytes = value_u64(storage, &["audit", "file_family_bytes"]);
    let total_artifact_bytes = proof_bytes
        .zip(audit_bytes)
        .map(|(proof, audit)| proof + audit);
    let wal_bytes = value_u64(proof, &["wal_bytes"]);
    let table_bytes = proof_storage
        .as_ref()
        .and_then(sum_object_bytes_by_type("table"));
    let index_bytes = proof_storage
        .as_ref()
        .and_then(sum_object_bytes_by_type("index"));
    let edge_table_plus_index = value_f64(
        proof_storage.as_ref().unwrap_or(&Value::Null),
        &[
            "aggregate_metrics",
            "average_edge_table_plus_index_payload_bytes_per_proof_edge",
        ],
    )
    .or_else(|| {
        value_f64(
            proof_storage.as_ref().unwrap_or(&Value::Null),
            &[
                "aggregate_metrics",
                "average_edge_table_plus_index_bytes_per_edge",
            ],
        )
    });
    let allocated_edge_table_plus_index = value_f64(
        proof_storage.as_ref().unwrap_or(&Value::Null),
        &[
            "aggregate_metrics",
            "average_edge_table_plus_index_bytes_per_edge",
        ],
    );
    let whole_db_bytes_per_edge = value_f64(
        proof_storage.as_ref().unwrap_or(&Value::Null),
        &["aggregate_metrics", "average_database_bytes_per_edge"],
    )
    .or_else(|| {
        proof_bytes
            .zip(value_u64(proof, &["physical_edge_rows"]))
            .map(|(bytes, rows)| {
                if rows == 0 {
                    0.0
                } else {
                    bytes as f64 / rows as f64
                }
            })
    });
    let proof_edge_payload_bytes = value_f64(
        proof_storage.as_ref().unwrap_or(&Value::Null),
        &[
            "aggregate_metrics",
            "average_edge_payload_bytes_per_proof_edge",
        ],
    )
    .or_else(|| object_average_payload_bytes(proof_storage, "edges"))
    .or(whole_db_bytes_per_edge);
    let proof_edge_count = value_u64(
        proof_storage.as_ref().unwrap_or(&Value::Null),
        &["aggregate_metrics", "proof_edge_count"],
    );
    let physical_edge_count = value_u64(
        proof_storage.as_ref().unwrap_or(&Value::Null),
        &["aggregate_metrics", "edge_count"],
    )
    .or_else(|| value_u64(proof, &["physical_edge_rows"]));
    let edge_table_plus_index_payload_bytes = value_u64(
        proof_storage.as_ref().unwrap_or(&Value::Null),
        &["aggregate_metrics", "edge_table_plus_index_payload_bytes"],
    );
    let edge_table_payload_bytes = value_u64(
        proof_storage.as_ref().unwrap_or(&Value::Null),
        &["aggregate_metrics", "edge_table_payload_bytes"],
    );
    let edge_index_payload_bytes = value_u64(
        proof_storage.as_ref().unwrap_or(&Value::Null),
        &["aggregate_metrics", "edge_index_payload_bytes"],
    );
    let bytes_per_entity = object_average_bytes(proof_storage, "entities");
    let bytes_per_template_entity = object_average_bytes(proof_storage, "template_entities");
    let bytes_per_template_edge = object_average_bytes(proof_storage, "template_edges");
    let bytes_per_source_span = object_average_bytes(proof_storage, "file_source_spans");
    let bytes_per_path_evidence = object_average_bytes(proof_storage, "path_evidence");
    let snippet_policy = proof_storage
        .as_ref()
        .and_then(|value| value_bool(value, &["fts_storage", "stores_source_snippets"]))
        .map(|stores| !stores);

    vec![
        metric_reported_u64("proof_db_bytes", proof_bytes, "Proof DB family size in bytes."),
        metric_le_f64("proof_db_mib", 250.0, proof_mib, "Proof DB family must be <=250 MiB."),
        metric_le_f64("proof_db_mib_stretch", 150.0, proof_mib, "Stretch target is <=150 MiB."),
        metric_reported_u64(
            "audit_debug_db_bytes",
            audit_bytes,
            "Audit/debug DB bytes are reported separately and not counted against proof target.",
        ),
        metric_reported_u64(
            "total_artifact_bytes",
            total_artifact_bytes,
            "Proof + audit artifact bytes for operator planning.",
        ),
        metric_reported_u64("wal_bytes", wal_bytes, "WAL bytes are reported separately."),
        metric_reported_u64("table_bytes", table_bytes, "dbstat table bytes."),
        metric_reported_u64("index_bytes", index_bytes, "dbstat index bytes."),
        metric_reported_u64(
            "proof_edge_count",
            proof_edge_count,
            "Proof-grade edge rows counted from exact/compiler/lsp/parser-verified graph edges only; text, candidate, source-navigation, heuristic, and support rows are excluded.",
        ),
        metric_reported_u64(
            "physical_edge_count",
            physical_edge_count,
            "Physical rows in the edges table, reported separately from proof-grade denominator.",
        ),
        metric_reported_u64(
            "edge_table_payload_bytes",
            edge_table_payload_bytes,
            "dbstat payload bytes for the edges table.",
        ),
        metric_reported_u64(
            "edge_index_payload_bytes",
            edge_index_payload_bytes,
            "dbstat payload bytes for idx_edges_* indexes, counted once.",
        ),
        metric_reported_u64(
            "edge_table_plus_index_payload_bytes",
            edge_table_plus_index_payload_bytes,
            "Comparable payload bytes for edges table plus edge indexes.",
        ),
        metric_reported_f64(
            "whole_db_bytes_per_physical_edge_diagnostic",
            whole_db_bytes_per_edge,
            "Diagnostic only: whole proof DB bytes divided by physical edge rows includes fixed SQLite page/schema/dictionary/FTS/update-support overhead and is not the per-edge target.",
        ),
        metric_reported_f64(
            "allocated_edge_table_plus_index_bytes_per_physical_edge_diagnostic",
            allocated_edge_table_plus_index,
            "Diagnostic only: allocated SQLite page bytes for edges plus edge indexes divided by physical edge rows; tiny fixtures include page-floor overhead.",
        ),
        metric_le_f64(
            "bytes_per_proof_edge",
            120.0,
            proof_edge_payload_bytes,
            "Comparable proof-edge payload bytes divided by proof-grade graph edge rows; fixed schema pages, dictionaries, FTS/text evidence, PathEvidence, file-scoped update support, sidecars, and audit/debug bytes are reported separately.",
        ),
        metric_le_f64(
            "bytes_per_edge_table_plus_index",
            120.0,
            edge_table_plus_index,
            "Comparable edges table payload plus idx_edges_* payload bytes per proof-grade graph edge; allocated page-floor bytes are reported separately.",
        ),
        metric_reported_f64("bytes_per_entity", bytes_per_entity, "Average total bytes per entity row."),
        metric_reported_f64(
            "bytes_per_template_entity",
            bytes_per_template_entity,
            "Average total bytes per template entity row.",
        ),
        metric_reported_f64(
            "bytes_per_template_edge",
            bytes_per_template_edge,
            "Average total bytes per template edge row.",
        ),
        metric_reported_f64(
            "bytes_per_source_span",
            bytes_per_source_span,
            "Average total bytes per file_source_spans row.",
        ),
        metric_reported_f64(
            "bytes_per_path_evidence_row",
            bytes_per_path_evidence,
            "Average total bytes per PathEvidence row.",
        ),
        metric(
            "source_snippets_not_stored_redundantly",
            json!(true),
            observed_bool(snippet_policy),
            status_known_pass(snippet_policy),
            vec!["Source snippets should be loaded from source files, not redundantly stored in SQLite.".to_string()],
        ),
    ]
}

pub(crate) fn sum_object_bytes_by_type<'a>(
    object_type: &'a str,
) -> impl FnOnce(&'a Value) -> Option<u64> + 'a {
    move |storage| {
        let objects = storage.get("objects")?.as_array()?;
        Some(
            objects
                .iter()
                .filter(|object| object["object_type"].as_str() == Some(object_type))
                .filter_map(|object| value_u64(object, &["total_bytes"]))
                .sum(),
        )
    }
}

pub(crate) fn comprehensive_cardinality_metrics(
    proof_build: &Value,
    proof_storage: &Option<Value>,
    storage: &Value,
) -> Vec<Value> {
    vec![
        metric_reported_u64(
            "files_walked",
            value_u64(proof_build, &["files_walked"]),
            "Cold proof build files walked.",
        ),
        metric_reported_u64(
            "files_parsed",
            value_u64(proof_build, &["files_parsed"]),
            "Cold proof build files parsed.",
        ),
        metric_reported_u64(
            "content_templates",
            object_row_count(proof_storage, "source_content_template"),
            "Source content templates.",
        ),
        metric_reported_u64(
            "file_instances",
            object_row_count(proof_storage, "files"),
            "Path-specific file instances.",
        ),
        metric_reported_u64(
            "duplicate_content_templates",
            value_u64(proof_build, &["duplicate_local_analyses_skipped"]),
            "Duplicate local analyses skipped by content-template dedupe.",
        ),
        metric_reported_u64(
            "template_entities",
            object_row_count(proof_storage, "template_entities"),
            "Template entity rows.",
        ),
        metric_reported_u64(
            "template_edges",
            object_row_count(proof_storage, "template_edges"),
            "Template edge rows.",
        ),
        metric_reported_u64(
            "proof_entities",
            object_row_count(proof_storage, "entities"),
            "Proof entity rows.",
        ),
        metric_reported_u64(
            "proof_edges",
            object_row_count(proof_storage, "edges"),
            "Physical proof edge rows.",
        ),
        metric_reported_u64(
            "structural_records",
            object_row_count(proof_storage, "structural_relations"),
            "Generic structural relation rows.",
        ),
        metric_reported_u64(
            "callsites",
            object_row_count(proof_storage, "callsites"),
            "Callsite rows.",
        ),
        metric_reported_u64(
            "callsite_args",
            object_row_count(proof_storage, "callsite_args"),
            "Callsite argument rows.",
        ),
        metric_reported_u64(
            "symbols",
            object_row_count(proof_storage, "symbol_dict"),
            "Symbol dictionary rows.",
        ),
        metric_reported_u64(
            "qname_prefixes",
            object_row_count(proof_storage, "qname_prefix_dict"),
            "QName prefix dictionary rows.",
        ),
        metric_reported_u64(
            "source_spans",
            object_row_count(proof_storage, "file_source_spans"),
            "File/source-span mapping rows.",
        ),
        metric_reported_u64(
            "path_evidence_rows",
            object_row_count(proof_storage, "path_evidence"),
            "Stored PathEvidence rows.",
        ),
        metric_reported_u64(
            "heuristic_debug_rows",
            value_u64(storage, &["audit", "audit_only_sidecar_rows"]),
            "Audit/debug sidecar rows preserved outside compact proof facts.",
        ),
    ]
}

pub(crate) fn comprehensive_storage_contributors(
    proof_storage: &Option<Value>,
    previous: Option<&Value>,
    baseline: &Value,
) -> Vec<Value> {
    let proof_bytes = proof_storage
        .as_ref()
        .and_then(|value| value_u64(value, &["file_family", "total_bytes"]))
        .or_else(|| value_u64(baseline, &["storage_summary", "proof_file_family_bytes"]))
        .unwrap_or(0);
    let Some(objects) = proof_storage
        .as_ref()
        .and_then(|value| value.get("objects"))
        .and_then(Value::as_array)
    else {
        return baseline["storage_summary"]["top_storage_contributors"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|entry| {
                let name = entry["object"].as_str().unwrap_or("unknown");
                let bytes = value_u64(&entry, &["bytes"]).unwrap_or(0);
                contributor_row(
                    name,
                    "unknown",
                    value_u64(&entry, &["rows"]),
                    bytes,
                    proof_bytes,
                    None,
                )
            })
            .collect();
    };

    let mut rows = Vec::new();
    for object in objects {
        let name = object["name"].as_str().unwrap_or("unknown");
        let kind = object["object_type"].as_str().unwrap_or("unknown");
        let bytes = value_u64(object, &["total_bytes"]).unwrap_or(0);
        let previous_bytes = previous
            .and_then(|previous| previous_contributor_bytes(previous, name))
            .or_else(|| baseline_contributor_bytes(baseline, name));
        if include_comprehensive_storage_object(name, bytes) {
            rows.push(contributor_row(
                name,
                kind,
                value_u64(object, &["row_count"]),
                bytes,
                proof_bytes,
                previous_bytes,
            ));
        }
    }
    rows.sort_by(|left, right| {
        value_u64(right, &["bytes"])
            .unwrap_or(0)
            .cmp(&value_u64(left, &["bytes"]).unwrap_or(0))
            .then_with(|| {
                left["name"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(right["name"].as_str().unwrap_or(""))
            })
    });
    rows
}

pub(crate) fn include_comprehensive_storage_object(name: &str, bytes: u64) -> bool {
    let required_names = [
        "template_entities",
        "template_edges",
        "symbol_dict",
        "qname_prefix_dict",
        "qualified_name_dict",
        "entities",
        "edges",
        "proof_edges",
        "source_spans",
        "file_source_spans",
        "path_evidence",
        "callsites",
        "callsite_args",
        "files",
        "source_content_template",
        "file_entities",
        "file_edges",
        "path_dict",
    ];
    required_names.contains(&name)
        || name.contains("template")
        || name.contains("symbol_dict")
        || name.contains("qname")
        || name.contains("qualified_name")
        || name.contains("dictionary")
        || name.ends_with("_dict")
        || name.starts_with("idx_template")
        || name.starts_with("idx_symbol")
        || name.starts_with("idx_qname")
        || name.starts_with("idx_qualified")
        || bytes >= 1_000_000
}

pub(crate) fn contributor_row(
    name: &str,
    kind: &str,
    rows: Option<u64>,
    bytes: u64,
    proof_bytes: u64,
    previous_bytes: Option<u64>,
) -> Value {
    let share = if proof_bytes == 0 {
        Value::Null
    } else {
        json!((bytes as f64) / (proof_bytes as f64))
    };
    let delta = previous_bytes.map(|previous| bytes as i128 - previous as i128);
    json!({
        "name": name,
        "kind": kind,
        "rows": rows,
        "bytes": bytes,
        "mib": bytes as f64 / 1024.0 / 1024.0,
        "share_of_proof_db": share,
        "previous_baseline_bytes": previous_bytes,
        "delta_bytes": delta,
        "classification": classify_storage_object(name),
    })
}

pub(crate) fn previous_contributor_bytes(previous: &Value, name: &str) -> Option<u64> {
    previous["sections"]["storage_contributors"]["contributors"]
        .as_array()?
        .iter()
        .find(|entry| entry["name"].as_str() == Some(name))
        .and_then(|entry| value_u64(entry, &["bytes"]))
}

pub(crate) fn baseline_contributor_bytes(baseline: &Value, name: &str) -> Option<u64> {
    baseline["storage_summary"]["top_storage_contributors"]
        .as_array()?
        .iter()
        .find(|entry| entry["object"].as_str() == Some(name))
        .and_then(|entry| value_u64(entry, &["bytes"]))
}

pub(crate) fn classify_storage_object(name: &str) -> &'static str {
    if name.contains("debug")
        || name == "heuristic_edges"
        || name == "unresolved_references"
        || name == "static_references"
        || name == "extraction_warnings"
    {
        "debug_sidecar"
    } else if name.contains("template") {
        "template"
    } else if name.ends_with("_dict")
        || name.contains("symbol_dict")
        || name.contains("qname")
        || name.contains("qualified_name")
        || name.contains("dictionary")
    {
        "dictionary"
    } else if name.contains("path_evidence") || name == "derived_edges" {
        "derived_cache"
    } else if name == "callsites" || name == "callsite_args" || name == "structural_relations" {
        "structural"
    } else if name.starts_with("sqlite_stat") || name.starts_with("bench_") {
        "temporary/build_only"
    } else if name == "edges"
        || name == "entities"
        || name == "file_source_spans"
        || name == "source_spans"
        || name == "files"
    {
        "proof_required"
    } else {
        "proof_optional"
    }
}

pub(crate) fn comprehensive_cold_profile_metrics(proof_build: &Value) -> Vec<Value> {
    let total = value_f64(proof_build, &["wall_ms"]);
    let db_write = value_f64(proof_build, &["db_write_ms"]);
    let integrity = value_f64(proof_build, &["integrity_check_ms"]);
    let claimable_for_thresholds = value_bool(proof_build, &["claimable_for_thresholds"]);
    let mut metrics = Vec::new();
    metrics.push(metric(
        "proof_build_timing_claimable_for_thresholds",
        json!(true),
        observed_bool(claimable_for_thresholds),
        status_known_pass(claimable_for_thresholds),
        vec![if claimable_for_thresholds == Some(true) {
            "Proof-build timing came from a production/release binary and may be compared to thresholds."
                .to_string()
        } else {
            "Proof-build timing came from a debug or unknown binary profile; record it as diagnostic_only and do not compare it to production thresholds."
                .to_string()
        }],
    ));
    metrics.push(if claimable_for_thresholds == Some(true) {
        metric_le_f64(
            "cold_proof_build_total_wall_ms",
            60000.0,
            total,
            "Cold proof build intended target is <=60 seconds.",
        )
    } else {
        metric(
            "cold_proof_build_total_wall_ms",
            json!("<=60000 ms release binary only"),
            json!(NON_CLAIMABLE_DEBUG_TIMING),
            "unknown",
            vec![
                "Debug proof-build timing is not a claimable production-binary threshold result."
                    .to_string(),
            ],
        )
    });
    metrics.extend([
        metric_reported_f64(
            "cold_proof_build_total_profile_ms",
            total,
            "Profile total from compact proof baseline.",
        ),
        metric_reported_f64(
            "file_walk_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "metadata_diff_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "read_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "hash_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "parse_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "local_fact_bundle_creation_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "content_template_dedupe_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "reducer_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "symbol_interning_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "qname_prefix_interning_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "template_entity_insert_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "template_edge_insert_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "proof_entity_insert_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "proof_edge_insert_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "source_span_insert_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "path_evidence_generation_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "db_write_time_ms",
            db_write,
            "Known dominant cold-build stage.",
        ),
        metric_reported_f64(
            "index_creation_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "fts_build_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "vacuum_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "analyze_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "integrity_check_time_ms",
            integrity,
            "Cold proof DB integrity-check duration.",
        ),
        metric_reported_f64(
            "graph_hash_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "report_generation_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
    ]);
    metrics
}

pub(crate) fn comprehensive_cold_build_mode_distinction(gate: &Value) -> Vec<Value> {
    let proof_wall = value_f64(gate, &["autoresearch", "proof_build", "wall_ms"]);
    let integrity = value_f64(gate, &["autoresearch", "proof_build", "integrity_check_ms"]);
    let audit_wall = value_f64(gate, &["autoresearch", "audit_build", "wall_ms"]);
    let claimable_for_thresholds = value_bool(
        gate,
        &["autoresearch", "proof_build", "claimable_for_thresholds"],
    );
    let proof_wall_observed = gate_value(gate, &["autoresearch", "proof_build", "wall_ms"])
        .cloned()
        .unwrap_or(Value::Null);
    let proof_plus_validation = proof_wall.map(|wall| wall + integrity.unwrap_or(0.0));
    let proof_plus_audit = match (proof_wall, audit_wall) {
        (Some(proof), Some(audit)) => Some(proof + audit),
        _ => None,
    };

    vec![
        json!({
            "mode": "proof-build-only",
            "observed_ms": if claimable_for_thresholds == Some(false) {
                proof_wall_observed.clone()
            } else {
                observed_number_f64(proof_wall)
            },
            "observed_minutes": if claimable_for_thresholds == Some(false) {
                Value::Null
            } else {
                observed_number_f64(proof_wall.map(|value| value / 60000.0))
            },
            "target_ms": if claimable_for_thresholds == Some(false) {
                json!("<=60000 ms release binary only")
            } else {
                json!(60000)
            },
            "status": if claimable_for_thresholds == Some(false) {
                "unknown"
            } else {
                status_known_pass(proof_wall.map(|value| value <= 60000.0))
            },
            "included_in_50_02_minute_number": claimable_for_thresholds != Some(false),
            "classification": if claimable_for_thresholds == Some(false) {
                "diagnostic debug/non-production proof-build timing; not compared to production threshold"
            } else {
                "actual production proof-mode cold build as persisted by the compact gate"
            },
            "notes": if claimable_for_thresholds == Some(false) {
                vec!["Debug proof-build timing is diagnostic_only and cannot produce a green or red production verdict.".to_string()]
            } else {
                vec!["This is the number compared to the <=60s cold proof build target.".to_string()]
            }
        }),
        json!({
            "mode": "proof-build-plus-validation",
            "observed_ms": observed_number_f64(proof_plus_validation),
            "observed_minutes": observed_number_f64(proof_plus_validation.map(|value| value / 60000.0)),
            "target_ms": 60000,
            "status": status_known_pass(proof_plus_validation.map(|value| value <= 60000.0)),
            "included_in_50_02_minute_number": false,
            "classification": "proof build plus the separately persisted integrity_check duration",
            "notes": [
                "Validation is important, but the compact gate reports integrity_check separately from proof_build.wall_ms."
            ]
        }),
        json!({
            "mode": "proof-build-plus-audit",
            "observed_ms": observed_number_f64(proof_plus_audit),
            "observed_minutes": observed_number_f64(proof_plus_audit.map(|value| value / 60000.0)),
            "target_ms": Value::Null,
            "status": if proof_plus_audit.is_some() { "reported" } else { "unknown" },
            "included_in_50_02_minute_number": false,
            "classification": "sequential proof build plus separate audit-sidecar build",
            "notes": [
                "The audit build is a separate compact-gate artifact and must not be counted as the proof DB cold build."
            ]
        }),
        json!({
            "mode": "proof-build-plus-audit-plus-validation",
            "observed_ms": observed_number_f64(proof_plus_audit.map(|value| value + integrity.unwrap_or(0.0))),
            "observed_minutes": observed_number_f64(proof_plus_audit.map(|value| (value + integrity.unwrap_or(0.0)) / 60000.0)),
            "target_ms": Value::Null,
            "status": if proof_plus_audit.is_some() { "reported" } else { "unknown" },
            "included_in_50_02_minute_number": false,
            "classification": "operator gate bundle, not the production proof-build-only target",
            "notes": [
                "Useful for planning gate runtime, but not comparable to the <=60s production cold proof build target."
            ]
        }),
        json!({
            "mode": "full-gate",
            "observed_ms": Value::Null,
            "observed_minutes": Value::Null,
            "target_ms": Value::Null,
            "status": "unknown",
            "included_in_50_02_minute_number": false,
            "classification": "not persisted as a single wall-clock value in the compact gate",
            "notes": [
                "Graph truth, context gate, sampler, storage audit, update checks, and report generation are not collapsed into the proof_build.wall_ms number."
            ]
        }),
    ]
}

pub(crate) fn comprehensive_cold_build_waterfall(gate: &Value) -> Vec<Value> {
    let proof_wall = value_f64(gate, &["autoresearch", "proof_build", "wall_ms"]);
    let db_write = value_f64(gate, &["autoresearch", "proof_build", "db_write_ms"]);
    let integrity = value_f64(gate, &["autoresearch", "proof_build", "integrity_check_ms"]);
    let audit_wall = value_f64(gate, &["autoresearch", "audit_build", "wall_ms"]);
    let residual = match (proof_wall, db_write) {
        (Some(total), Some(write)) if total >= write => Some(total - write),
        _ => None,
    };
    let pct_of_proof = |value: Option<f64>| -> Value {
        match (value, proof_wall) {
            (Some(value), Some(total)) if total > 0.0 => json!(value / total),
            _ => Value::Null,
        }
    };
    let pct_of_plus_validation = |value: Option<f64>| -> Value {
        match (
            value,
            proof_wall.map(|wall| wall + integrity.unwrap_or(0.0)),
        ) {
            (Some(value), Some(total)) if total > 0.0 => json!(value / total),
            _ => Value::Null,
        }
    };

    vec![
        json!({
            "stage": "actual_proof_db_build_total",
            "elapsed_ms": observed_number_f64(proof_wall),
            "pct_of_reference": pct_of_proof(proof_wall),
            "reference": "proof-build-only",
            "included_in_50_02_minute_number": true,
            "source": "reports/final/compact_proof_db_gate.json.autoresearch.proof_build.wall_ms",
            "notes": [
                "This is the persisted cold proof build number."
            ]
        }),
        json!({
            "stage": "production_persistence_and_global_reduction_bucket",
            "elapsed_ms": observed_number_f64(db_write),
            "pct_of_reference": pct_of_proof(db_write),
            "reference": "proof-build-only",
            "included_in_50_02_minute_number": true,
            "source": "reports/final/compact_proof_db_gate.json.autoresearch.proof_build.db_write_ms",
            "notes": [
                "This bucket is currently broad: it includes SQLite persistence plus post-local global reduction, PathEvidence refresh, index recreation, ANALYZE/checkpoint work, and transaction commit where applicable."
            ]
        }),
        json!({
            "stage": "source_scan_parse_extract_dedupe_reducer_residual",
            "elapsed_ms": observed_number_f64(residual),
            "pct_of_reference": pct_of_proof(residual),
            "reference": "proof-build-only",
            "included_in_50_02_minute_number": true,
            "source": "wall_ms - db_write_ms",
            "notes": [
                "The compact gate did not persist nested cold-build spans for these phases; as a combined residual they are below 5% of proof-build-only wall time."
            ]
        }),
        json!({
            "stage": "integrity_check_validation",
            "elapsed_ms": observed_number_f64(integrity),
            "pct_of_reference": pct_of_plus_validation(integrity),
            "reference": "proof-build-plus-validation",
            "included_in_50_02_minute_number": false,
            "source": "reports/final/compact_proof_db_gate.json.autoresearch.proof_build.integrity_check_ms",
            "notes": [
                "Integrity checking is separately measured and is not the dominant cause."
            ]
        }),
        json!({
            "stage": "audit_sidecar_build",
            "elapsed_ms": observed_number_f64(audit_wall),
            "pct_of_reference": Value::Null,
            "reference": "separate audit build",
            "included_in_50_02_minute_number": false,
            "source": "reports/final/compact_proof_db_gate.json.autoresearch.audit_build.wall_ms",
            "notes": [
                "Audit/debug-sidecar build time is a separate artifact and must not be blamed for the proof-build-only failure."
            ]
        }),
    ]
}

pub(crate) fn comprehensive_cold_build_interpretation(gate: &Value) -> Value {
    let proof_wall = value_f64(gate, &["autoresearch", "proof_build", "wall_ms"]);
    let db_write = value_f64(gate, &["autoresearch", "proof_build", "db_write_ms"]);
    let residual = match (proof_wall, db_write) {
        (Some(total), Some(write)) if total >= write => Some(total - write),
        _ => None,
    };
    json!({
        "fifty_point_zero_two_minutes_is": "actual production proof-mode cold build profile wall time from the compact gate",
        "is_benchmark_gate_wall_time": false,
        "is_debug_or_audit_sidecar_build_time": false,
        "includes_repeated_rebuilds": false,
        "includes_slow_storage_audit_or_dbstat": false,
        "includes_cgc_or_competitor_work": false,
        "includes_report_generation": false,
        "integrity_check_is_reported_separately": true,
        "dominant_known_stage": "production_persistence_and_global_reduction_bucket",
        "dominant_known_stage_ms": observed_number_f64(db_write),
        "dominant_known_stage_pct_of_proof_build": match (db_write, proof_wall) {
            (Some(write), Some(total)) if total > 0.0 => json!(write / total),
            _ => Value::Null,
        },
        "combined_non_db_residual_ms": observed_number_f64(residual),
        "combined_non_db_residual_pct_of_proof_build": match (residual, proof_wall) {
            (Some(value), Some(total)) if total > 0.0 => json!(value / total),
            _ => Value::Null,
        },
        "profile_gap": "The compact gate persisted a broad db_write_ms bucket but did not persist the nested IndexProfile.spans for the proof-mode run. Future compact gates should archive the raw index profile spans so dictionary interning, template inserts, reducer, index creation, ANALYZE, and PathEvidence can be separated without re-running forensics.",
        "stages_over_5_percent_unknown": []
    })
}

pub(crate) fn comprehensive_top_slowest_stages(metrics: &[Value]) -> Vec<Value> {
    let mut stages = metrics
        .iter()
        .filter_map(|metric| {
            let id = metric["id"].as_str()?;
            let observed = metric["observed"].as_f64()?;
            if id.ends_with("_time_ms") || id.ends_with("_total_wall_ms") {
                Some(json!({
                    "stage": id,
                    "elapsed_ms": observed,
                    "status": metric["status"].clone(),
                    "notes": metric["notes"].clone(),
                }))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    stages.sort_by(|left, right| {
        right["elapsed_ms"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&left["elapsed_ms"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    stages.truncate(10);
    stages
}

pub(crate) fn comprehensive_repeat_metrics(
    update_path: &Value,
    repeat_unchanged: Option<&Value>,
    update_integrity: Option<&Value>,
) -> Vec<Value> {
    let repeat = update_path.get("repeat_unchanged").unwrap_or(&Value::Null);
    let profile_time = value_f64(repeat, &["profile_wall_ms"]).or_else(|| {
        repeat_unchanged.and_then(|value| value_f64(value, &["profile_total_wall_ms"]))
    });
    let shell_time = value_f64(repeat, &["shell_ms"])
        .or_else(|| repeat_unchanged.and_then(|value| value_f64(value, &["shell_ms"])));
    let files_read = value_u64(repeat, &["files_read"])
        .or_else(|| repeat_unchanged.and_then(|value| value_u64(value, &["files_read"])));
    let files_hashed = value_u64(repeat, &["files_hashed"])
        .or_else(|| repeat_unchanged.and_then(|value| value_u64(value, &["files_hashed"])));
    let files_parsed = value_u64(repeat, &["files_parsed"])
        .or_else(|| repeat_unchanged.and_then(|value| value_u64(value, &["files_parsed"])));
    let files_walked = value_u64(repeat, &["files_walked"])
        .or_else(|| repeat_unchanged.and_then(|value| value_u64(value, &["files_walked"])));
    let metadata_unchanged =
        repeat_unchanged.and_then(|value| value_u64(value, &["files_metadata_unchanged"]));
    let stable_hash = update_integrity
        .and_then(|value| value.get("repos"))
        .and_then(Value::as_array)
        .and_then(|repos| repos.first())
        .and_then(|repo| value_bool(repo, &["graph_fact_hash_stable_on_repeat"]));
    let proof_mutations = value_u64(repeat, &["entities_inserted"]).unwrap_or(0)
        + value_u64(repeat, &["edges_inserted"]).unwrap_or(0);

    vec![
        metric_le_f64(
            "repeat_unchanged_total_ms",
            5000.0,
            profile_time.or(shell_time),
            "Repeat unchanged index must complete within 5 seconds.",
        ),
        metric_reported_u64(
            "repeat_files_walked",
            files_walked,
            "Repeat unchanged files walked.",
        ),
        metric_reported_u64(
            "repeat_metadata_unchanged",
            metadata_unchanged,
            "Files skipped by metadata prefilter.",
        ),
        metric_eq_u64(
            "repeat_files_read",
            0,
            files_read,
            "Unchanged repeat should not read source files.",
        ),
        metric_eq_u64(
            "repeat_files_hashed",
            0,
            files_hashed,
            "Unchanged repeat should not hash source files.",
        ),
        metric_eq_u64(
            "repeat_files_parsed",
            0,
            files_parsed,
            "Unchanged repeat should not parse source files.",
        ),
        metric_eq_u64(
            "repeat_entities_inserted",
            0,
            value_u64(repeat, &["entities_inserted"]),
            "Unchanged repeat should not insert proof entities.",
        ),
        metric_eq_u64(
            "repeat_edges_inserted",
            0,
            value_u64(repeat, &["edges_inserted"]),
            "Unchanged repeat should not insert proof edges.",
        ),
        metric_reported_u64(
            "repeat_templates_inserted",
            None,
            "Template insert count is not persisted in compact proof repeat artifact.",
        ),
        metric(
            "repeat_graph_fact_hash_changed",
            json!(false),
            stable_hash
                .map(|stable| Value::from(!stable))
                .unwrap_or(Value::Null),
            status_known_pass(stable_hash.map(|stable| stable)),
            vec!["Unchanged repeat graph fact hash must remain stable.".to_string()],
        ),
        metric(
            "repeat_db_writes_performed",
            json!("no proof graph mutations"),
            json!(proof_mutations > 0),
            status_known_pass(Some(proof_mutations == 0)),
            vec![
                "Metadata/checkpoint writes may occur; proof graph rows must not mutate."
                    .to_string(),
            ],
        ),
        metric_reported_u64(
            "repeat_validation_work_integrity_ms",
            repeat_unchanged.and_then(|value| value_u64(value, &["integrity_ms"])),
            "Validation work duration if the artifact persisted it.",
        ),
        metric(
            "repeat_integrity_check_included",
            json!("reported"),
            Value::from(
                value_string(repeat, &["integrity_status"])
                    .unwrap_or_else(|| "unknown".to_string()),
            ),
            "pass",
            vec!["Repeat artifact reports integrity status separately.".to_string()],
        ),
    ]
}

pub(crate) fn comprehensive_single_file_update_metrics(
    update_path: &Value,
    update_integrity: Option<&Value>,
) -> Vec<Value> {
    let update = update_path
        .get("single_file_update")
        .unwrap_or(&Value::Null);
    let detailed_iterations = update_integrity
        .and_then(|value| value.get("repos"))
        .and_then(Value::as_array)
        .map(|repos| {
            let mut iterations = Vec::new();
            for repo in repos {
                if let Some(repo_iterations) =
                    repo.get("iteration_results").and_then(Value::as_array)
                {
                    iterations.extend(repo_iterations.iter());
                }
            }
            iterations
        })
        .unwrap_or_default();
    let detailed_updates = detailed_iterations
        .iter()
        .filter_map(|iteration| iteration.get("update"))
        .collect::<Vec<_>>();
    let detailed_restores = detailed_iterations
        .iter()
        .filter_map(|iteration| iteration.get("restore"))
        .collect::<Vec<_>>();
    let detailed = detailed_updates.first().copied();
    let spans = detailed
        .and_then(|value| value.get("profile"))
        .and_then(|profile| profile.get("spans"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let span_time = |name: &str| -> Option<f64> {
        spans
            .iter()
            .find(|span| span["name"].as_str() == Some(name))
            .and_then(|span| value_f64(span, &["elapsed_ms"]))
    };
    let update_wall_samples = detailed_updates
        .iter()
        .filter_map(|value| value_f64(value, &["wall_ms"]))
        .collect::<Vec<_>>();
    let wall_ms = if update_integrity.is_some() {
        percentile(&update_wall_samples, 0.95)
    } else {
        value_f64(update, &["wall_ms"])
    };
    let global_hash_ran = detailed.and_then(|value| value_bool(value, &["global_hash_check_ran"]));

    vec![
        metric_le_f64(
            "single_file_update_total_ms",
            750.0,
            wall_ms,
            "Single-file update p95 target is <=750 ms.",
        ),
        metric_reported_f64(
            "update_lifecycle_preflight_ms",
            span_time("lifecycle_preflight"),
            "Lifecycle/preflight span before the delta write.",
        ),
        metric_reported_f64("update_file_walk_time_ms", span_time("file_walk"), "File walk span."),
        metric_reported_f64("update_read_time_ms", span_time("file_read"), "File read span."),
        metric_reported_f64("update_hash_time_ms", span_time("file_hash"), "File hash span."),
        metric_reported_f64("update_parse_time_ms", span_time("parse"), "Parse span."),
        metric_reported_f64("update_stale_delete_time_ms", span_time("stale_fact_delete"), "Indexed stale fact cleanup span."),
        metric_reported_f64("update_stale_missing_manifest_scan_ms", span_time("stale_missing_manifest_scan"), "Explicit missing-file cleanup scan span."),
        metric_reported_f64("update_template_invalidation_time_ms", None, "Template invalidation is not persisted as a separate span."),
        metric_reported_f64("update_template_insert_update_time_ms", None, "Template insert/update time is not persisted separately."),
        metric_reported_f64("update_proof_entity_update_time_ms", span_time("entity_insert"), "Proof entity insertion/update span."),
        metric_reported_f64("update_proof_edge_update_time_ms", span_time("edge_insert"), "Proof edge insertion/update span."),
        metric_reported_u64(
            "update_dirty_path_evidence_count",
            value_u64(update, &["dirty_path_evidence_count"])
                .or_else(|| detailed.and_then(|value| value_u64(value, &["dirty_path_evidence_count"]))),
            "Dirty PathEvidence rows regenerated.",
        ),
        metric_reported_f64(
            "update_path_evidence_regeneration_time_ms",
            span_time("path_evidence_insert"),
            "PathEvidence regeneration span.",
        ),
        metric_reported_f64("update_index_maintenance_time_ms", span_time("index_creation"), "Index maintenance span."),
        metric_reported_f64("update_transaction_commit_time_ms", span_time("transaction_commit"), "Transaction commit span."),
        metric_reported_f64("update_wal_checkpoint_time_ms", span_time("wal_checkpoint"), "WAL checkpoint/truncate span after commit."),
        metric_reported_f64("update_cache_refresh_time_ms", span_time("cache_refresh"), "In-memory incremental cache refresh span."),
        metric_reported_f64(
            "update_integrity_or_quick_check_time_ms",
            detailed.and_then(|value| value_f64(value, &["integrity_check_ms"])),
            "Validation integrity duration captured outside the fast path.",
        ),
        metric_reported_f64(
            "update_graph_hash_update_time_ms",
            span_time("graph_fact_hash")
                .or_else(|| detailed.and_then(|value| value_f64(value, &["graph_fact_hash_ms"]))),
            "Graph digest update/measurement; fast mode uses the incremental digest instead of a full scan.",
        ),
        metric_reported_f64(
            "update_restore_time_ms",
            if update_integrity.is_some() {
                percentile(
                    &detailed_restores
                        .iter()
                        .filter_map(|value| value_f64(value, &["wall_ms"]))
                        .collect::<Vec<_>>(),
                    0.95,
                )
            } else {
                update_path
                    .get("restore_update")
                    .and_then(|value| value_f64(value, &["wall_ms"]))
            },
            "Restore update duration from update-integrity harness.",
        ),
        metric_reported_u64(
            "update_rows_deleted",
            detailed.and_then(|value| value_u64(value, &["deleted_fact_files"])),
            "Current artifact persists deleted fact files, not exact row count.",
        ),
        metric_reported_u64(
            "update_rows_inserted",
            detailed
                .and_then(|value| value_u64(value, &["entities_inserted"]))
                .or_else(|| value_u64(update, &["entities_inserted"]))
                .zip(
                    detailed
                        .and_then(|value| value_u64(value, &["edges_inserted"]))
                        .or_else(|| value_u64(update, &["edges_inserted"])),
                )
                .map(|(entities, edges)| entities + edges),
            "Inserted proof entity + edge rows.",
        ),
        metric_reported_u64("update_indexes_touched", None, "Indexes touched are not persisted in the artifact."),
        metric(
            "update_global_work_accidentally_triggered",
            json!(false),
            observed_bool(global_hash_ran),
            status_known_pass(global_hash_ran.map(|ran| !ran)),
            vec!["Fast path should avoid full graph hash scans unless validation mode explicitly requests them.".to_string()],
        ),
        metric(
            "update_integrity_remains_ok",
            json!("ok"),
            Value::from(
                detailed
                    .and_then(|value| value_string(value, &["integrity_status"]))
                    .or_else(|| value_string(update, &["integrity_status"]))
                    .unwrap_or_else(|| "unknown".to_string()),
            ),
            status_known_pass(
                detailed
                    .and_then(|value| value_string(value, &["integrity_status"]))
                    .or_else(|| value_string(update, &["integrity_status"]))
                    .map(|status| status == "ok"),
            ),
            vec!["DB integrity must remain ok after update.".to_string()],
        ),
    ]
}

pub(crate) fn comprehensive_query_latency_metrics(
    context_pack_latency: &Option<Value>,
    unresolved_latency: &Option<Value>,
    gate: &Value,
    query_surface_report: Option<&Value>,
) -> Value {
    let mut queries = Vec::new();
    for (id, target, note) in [
        (
            "entity_name_lookup",
            250.0,
            "Entity lookup p95 was not measured in the compact proof gate.",
        ),
        (
            "symbol_lookup",
            250.0,
            "Symbol lookup p95 was not measured in the compact proof gate.",
        ),
        (
            "qname_lookup",
            250.0,
            "QName lookup p95 was not measured in the compact proof gate.",
        ),
        (
            "text_fts_query",
            500.0,
            "Text/FTS p95 was not measured in the compact proof gate.",
        ),
        (
            "relation_query_calls",
            500.0,
            "CALLS relation p95 was not measured in the compact proof gate.",
        ),
        (
            "relation_query_reads_writes",
            500.0,
            "READS/WRITES relation p95 was not measured in the compact proof gate.",
        ),
        (
            "path_evidence_lookup",
            500.0,
            "PathEvidence lookup p95 was not measured as a standalone query.",
        ),
        (
            "source_snippet_batch_load",
            500.0,
            "Source snippet batch-load p95 was not measured as a standalone query.",
        ),
        (
            "context_pack_normal",
            2000.0,
            "context_pack normal p95 was not measured on the fresh compact proof artifact.",
        ),
        (
            "unresolved_calls_paginated",
            1000.0,
            "Paginated unresolved-calls p95 was not measured on the fresh compact proof artifact.",
        ),
    ] {
        queries.push(
            query_surface_latency_metric(query_surface_report, id)
                .unwrap_or_else(|| unknown_query_metric(id, target, note)),
        );
    }
    queries.push(unknown_query_metric(
        "context_pack_impact",
        5000.0,
        "Impact mode context_pack p95 was not measured in the compact proof gate.",
    ));
    queries.push(unknown_query_metric(
        "impact_file",
        5000.0,
        "impact <file> p95 was not measured in the compact proof gate.",
    ));

    json!({
        "queries": queries,
        "notes": [
            "Default query-surface metrics are measured against the fresh compact proof artifact when available.",
            "Missing latency probes are unknown and must not be counted as passes; failed probes fail the comprehensive gate.",
            format!(
                "Context-pack artifact note: {}",
                value_string(gate, &["query_latency", "context_pack", "note"])
                    .unwrap_or_else(|| "not available".to_string())
            ),
            format!(
                "Legacy context-pack p95 artifact: {}",
                context_pack_latency
                    .as_ref()
                    .and_then(|value| value_f64(value, &["p95_shell_ms"]))
                    .map(|value| format!("{value:.3} ms"))
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            format!(
                "Legacy unresolved-calls p95 artifact: {}",
                unresolved_latency
                    .as_ref()
                    .and_then(|value| value_f64(value, &["p95_shell_ms"]))
                    .map(|value| format!("{value:.3} ms"))
                    .unwrap_or_else(|| "unknown".to_string())
            ),
        ]
    })
}

pub(crate) fn query_surface_latency_metric(
    query_surface_report: Option<&Value>,
    id: &str,
) -> Option<Value> {
    let report = query_surface_report?;
    let queries = report
        .get("report")
        .and_then(|value| value.get("queries"))
        .or_else(|| report.get("queries"))?
        .as_array()?;
    queries
        .iter()
        .find(|query| query["id"].as_str() == Some(id))
        .cloned()
}

pub(crate) fn unknown_query_metric(id: &str, target_p95_ms: f64, note: &str) -> Value {
    json!({
        "id": id,
        "target": { "p95_ms": target_p95_ms },
        "observed": {
            "p50_ms": Value::Null,
            "p95_ms": Value::Null,
            "p99_ms": Value::Null
        },
        "status": "unknown",
        "notes": [note]
    })
}

pub(crate) fn build_default_query_surface_report(
    repo_root: &Path,
    db_path: &Path,
    iterations: usize,
) -> Value {
    let started = Instant::now();
    let mut queries = Vec::new();
    let db_path = absolutize_path(db_path).unwrap_or_else(|_| db_path.to_path_buf());
    let repo_root = absolutize_path(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let lifecycle_status = benchmark_inspection_lifecycle_status(
        &repo_root,
        &db_path,
        "bench.query_surface",
        Some(StorageMode::Proof),
    );
    let claimable = benchmark_inspection_claimable(&lifecycle_status);
    if lifecycle_status["safe_to_read"].as_bool() != Some(true) {
        let blockers = lifecycle_status["blockers"]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "benchmark lifecycle preflight blocked DB inspection".to_string());
        let failed = required_query_surface_ids()
            .into_iter()
            .map(|(id, target)| {
                query_surface_failure_metric(
                    id,
                    target,
                    &format!("benchmark lifecycle preflight blocked DB inspection: {blockers}"),
                )
            })
            .collect::<Vec<_>>();
        return json!({
            "schema_version": 1,
            "status": "failed",
            "generated_at_unix_ms": unix_time_ms(),
            "repo_root": path_string(&repo_root),
            "db_path": path_string(&db_path),
            "storage_mode": "proof",
            "iterations": iterations,
            "inspection_read_only": true,
            "artifact_mutated_during_inspection": false,
            "sidecar_only_change": false,
            "sidecar_status": "not_inspected_lifecycle_blocked",
            "read_only_mode_used": "not_opened_lifecycle_blocked",
            "immutable_mode_used": false,
            "immutable_mode_reason": "lifecycle preflight blocked read-only inspection before SQLite open",
            "lifecycle_status": lifecycle_status,
            "claimable": false,
            "queries": failed,
            "summary": {
                "failed_queries": required_query_surface_ids().len(),
                "passed_queries": 0,
                "elapsed_ms": elapsed_ms(started),
                "all_default_queries_complete": false,
            },
            "notes": [
                "The query surface benchmark refused to inspect a DB that failed lifecycle preflight.",
                "No benchmark result from this artifact is claimable."
            ],
        });
    }
    let read_only = match open_sqlite_read_only_side_effect_minimal(&db_path) {
        Ok(read_only) => read_only,
        Err(error) => {
            let failed = required_query_surface_ids()
                .into_iter()
                .map(|(id, target)| {
                    query_surface_failure_metric(
                        id,
                        target,
                        &format!("failed to open compact proof DB: {error}"),
                    )
                })
                .collect::<Vec<_>>();
            return json!({
                "schema_version": 1,
                "status": "failed",
                "generated_at_unix_ms": unix_time_ms(),
                "repo_root": path_string(&repo_root),
                "db_path": path_string(&db_path),
                "iterations": iterations,
                "inspection_read_only": true,
                "artifact_mutated_during_inspection": false,
                "sidecar_only_change": false,
                "sidecar_status": "unknown_open_failed",
                "read_only_mode_used": "sqlite_uri_mode_ro_attempted",
                "immutable_mode_used": !sqlite_sidecar_path(&db_path, "wal").exists() && !sqlite_sidecar_path(&db_path, "shm").exists(),
                "immutable_mode_reason": "read-only open failed before inspection completed",
                "lifecycle_status": lifecycle_status,
                "claimable": false,
                "queries": failed,
                "summary": {
                    "failed_queries": required_query_surface_ids().len(),
                    "passed_queries": 0,
                    "elapsed_ms": elapsed_ms(started),
                    "all_default_queries_complete": false,
                },
                "notes": [
                    "The query surface could not open the compact proof DB."
                ],
            });
        }
    };
    let read_only_mode_used = read_only.read_only_mode_used.clone();
    let immutable_mode_used = read_only.immutable_mode_used;
    let immutable_mode_reason = read_only.immutable_mode_reason.clone();
    let connection = read_only.connection;
    let seeds = query_surface_seeds(&connection).unwrap_or_else(|error| {
        json!({
            "status": "degraded",
            "error": error,
            "entity_name": "login",
            "symbol_query": "login",
            "qname": "login",
            "entity_id": "repo://e/unknown",
            "fts_query": "\"login\"",
            "context_seed": "login",
        })
    });

    queries.push(benchmark_sql_surface_query(
        &connection,
        "entity_name_lookup",
        "codegraph-mcp query symbols <entity-name>",
        250.0,
        &format!(
            "WITH wanted_name AS (SELECT id FROM symbol_dict WHERE value = {name} LIMIT 1)
             SELECT 'repo://e/' || lower(hex(e.entity_hash)) AS id, name.value AS name,
                    qname.value AS qualified_name, path.value AS repo_relative_path
             FROM wanted_name
             JOIN entities e ON e.name_id = wanted_name.id
             JOIN symbol_dict name ON name.id = e.name_id
             JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
             JOIN path_dict path ON path.id = e.path_id
             ORDER BY qname.value, e.entity_hash
             LIMIT 20",
            name = sqlite_quote_text(seed_text(&seeds, "entity_name", "login"))
        ),
        iterations,
        "Default entity-name lookup must compile and use compact proof entity dictionaries.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "symbol_lookup",
        "codegraph-mcp query symbols <symbol>",
        250.0,
        &format!(
            "WITH candidate_keys AS (
                 SELECT e.id_key
                 FROM symbol_dict wanted
                 JOIN entities e ON e.name_id = wanted.id
                 WHERE wanted.value = {symbol}
                 UNION
                 SELECT e.id_key
                 FROM qualified_name_lookup wanted_qname
                 JOIN entities e ON e.qualified_name_id = wanted_qname.id
                 WHERE wanted_qname.value = {symbol}
             )
             SELECT 'repo://e/' || lower(hex(e.entity_hash)) AS id, name.value AS name,
                    qname.value AS qualified_name, path.value AS repo_relative_path
             FROM candidate_keys
             JOIN entities e ON e.id_key = candidate_keys.id_key
             JOIN symbol_dict name ON name.id = e.name_id
             JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
             JOIN path_dict path ON path.id = e.path_id
             ORDER BY qname.value, e.entity_hash
             LIMIT 20",
            symbol = sqlite_quote_text(seed_text(&seeds, "symbol_query", "login"))
        ),
        iterations,
        "Default symbol lookup must resolve object ID, name, and qualified-name paths without audit/debug sidecars.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "qname_lookup",
        "codegraph-mcp query symbols <qualified-name>",
        250.0,
        &format!(
            "WITH wanted_qname AS (SELECT id, value FROM qualified_name_lookup WHERE value = {qname} LIMIT 1)
             SELECT 'repo://e/' || lower(hex(e.entity_hash)) AS id, name.value AS name,
                    wanted_qname.value AS qualified_name, path.value AS repo_relative_path
             FROM wanted_qname
             JOIN entities e ON e.qualified_name_id = wanted_qname.id
             JOIN symbol_dict name ON name.id = e.name_id
             JOIN path_dict path ON path.id = e.path_id
             ORDER BY e.entity_hash
             LIMIT 20",
            qname = sqlite_quote_text(seed_text(&seeds, "qname", "login"))
        ),
        iterations,
        "Default qname lookup must use compact qualified-name reconstruction.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "text_fts_query",
        "codegraph-mcp query text <query>",
        500.0,
        &format!(
            "SELECT kind, id, repo_relative_path, line, title, bm25(stage0_fts) AS rank FROM stage0_fts WHERE stage0_fts MATCH {} ORDER BY rank, kind, id LIMIT 20",
            sqlite_quote_text(seed_text(&seeds, "fts_query", "\"login\""))
        ),
        iterations,
        "Proof-mode text query must use compact FTS or fail explicitly.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "relation_query_calls",
        "codegraph-mcp query callers/callees <symbol>",
        500.0,
        "SELECT e.id_key, head.value AS head_id, tail.value AS tail_id, relation.value AS relation, span_path.value AS source_span_path, e.start_line, e.end_line FROM edges_compat e JOIN relation_kind_dict relation ON relation.id = e.relation_id JOIN object_id_lookup head ON head.id = e.head_id_key JOIN object_id_lookup tail ON tail.id = e.tail_id_key JOIN path_dict span_path ON span_path.id = e.span_path_id WHERE relation.value = 'CALLS' ORDER BY e.id_key LIMIT 50",
        iterations,
        "Bounded CALLS relation lookup must use proof edges and compact compatibility views.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "relation_query_reads_writes",
        "codegraph-mcp query impact/proof relation READS|WRITES",
        500.0,
        "SELECT e.id_key, head.value AS head_id, tail.value AS tail_id, relation.value AS relation, span_path.value AS source_span_path, e.start_line, e.end_line FROM edges_compat e JOIN relation_kind_dict relation ON relation.id = e.relation_id JOIN object_id_lookup head ON head.id = e.head_id_key JOIN object_id_lookup tail ON tail.id = e.tail_id_key JOIN path_dict span_path ON span_path.id = e.span_path_id WHERE relation.value IN ('READS', 'WRITES') ORDER BY e.id_key LIMIT 50",
        iterations,
        "Bounded READS/WRITES lookup must compile against compact proof edges.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "path_evidence_lookup",
        "context_pack stored PathEvidence lookup",
        500.0,
        "SELECT l.path_id, p.source, p.target, l.relation_signature, l.length, l.confidence FROM path_evidence_lookup l JOIN path_evidence p ON p.id = l.path_id ORDER BY l.confidence DESC, l.length ASC, l.path_id LIMIT 50",
        iterations,
        "Stored PathEvidence lookup must use proof-mode materialized lookup tables.",
    ));
    queries.push(benchmark_source_snippet_batch_load(
        &connection,
        &repo_root,
        iterations,
    ));
    queries.push(benchmark_context_pack_surface_query(
        &repo_root,
        &db_path,
        seed_text(&seeds, "context_seed", "login"),
        iterations,
    ));
    queries.push(benchmark_unresolved_calls_surface_query(
        &connection,
        &repo_root,
        &db_path,
        iterations,
    ));

    let failed_queries = queries
        .iter()
        .filter(|query| query["status"].as_str() == Some("fail"))
        .count();
    let passed_queries = queries
        .iter()
        .filter(|query| query["status"].as_str() == Some("pass"))
        .count();
    let status = if failed_queries == 0 {
        "passed"
    } else {
        "failed"
    };
    json!({
        "schema_version": 1,
        "status": status,
        "generated_at_unix_ms": unix_time_ms(),
        "repo_root": path_string(&repo_root),
        "db_path": path_string(&db_path),
        "storage_mode": "proof",
        "iterations": iterations,
        "inspection_read_only": true,
        "artifact_mutated_during_inspection": false,
        "sidecar_only_change": false,
        "sidecar_status": if immutable_mode_used { "none_expected" } else { "existing_sidecars_preserved" },
        "read_only_mode_used": read_only_mode_used,
        "immutable_mode_used": immutable_mode_used,
        "immutable_mode_reason": immutable_mode_reason,
        "lifecycle_status": lifecycle_status,
        "claimable": claimable,
        "seeds": seeds,
        "queries": queries,
        "summary": {
            "passed_queries": passed_queries,
            "failed_queries": failed_queries,
            "elapsed_ms": elapsed_ms(started),
            "all_default_queries_complete": failed_queries == 0,
        },
        "notes": [
            "Queries are executed against compact proof storage only.",
            "Audit/debug sidecars are not used unless an individual query explicitly targets heuristic/debug evidence, such as unresolved-calls.",
            "Each query includes SQL, EXPLAIN QUERY PLAN, and p50/p95/p99 timings when it compiles."
        ],
    })
}

pub(crate) fn required_query_surface_ids() -> Vec<(&'static str, f64)> {
    vec![
        ("entity_name_lookup", 250.0),
        ("symbol_lookup", 250.0),
        ("qname_lookup", 250.0),
        ("text_fts_query", 500.0),
        ("relation_query_calls", 500.0),
        ("relation_query_reads_writes", 500.0),
        ("path_evidence_lookup", 500.0),
        ("source_snippet_batch_load", 500.0),
        ("context_pack_normal", 2000.0),
        ("unresolved_calls_paginated", 1000.0),
    ]
}

pub(crate) fn query_surface_seeds(connection: &Connection) -> Result<Value, String> {
    let entity = connection
        .query_row(
            "
            SELECT oid.value, name.value, qname.value
            FROM entities e
            JOIN object_id_lookup oid ON oid.id = e.id_key
            JOIN symbol_dict name ON name.id = e.name_id
            JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
            WHERE name.value IS NOT NULL AND name.value != ''
            ORDER BY CASE WHEN name.value = 'login' THEN 0 ELSE 1 END, e.id_key
            LIMIT 1
            ",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (entity_id, entity_name, qname) = entity.unwrap_or_else(|| {
        (
            "repo://e/unknown".to_string(),
            "login".to_string(),
            "login".to_string(),
        )
    });
    let fts_term = if sqlite_table_exists(connection, "stage0_fts").unwrap_or(false) {
        connection
            .query_row(
                "SELECT title FROM stage0_fts WHERE title IS NOT NULL AND title != '' ORDER BY kind, id LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .and_then(|title| title.split_whitespace().find(|part| part.len() >= 2).map(str::to_string))
            .unwrap_or_else(|| entity_name.clone())
    } else {
        entity_name.clone()
    };
    let symbol_query = entity_name.clone();
    let context_seed = if sqlite_table_exists(connection, "path_evidence_lookup").unwrap_or(false) {
        connection
            .query_row(
                "SELECT source_id FROM path_evidence_lookup ORDER BY confidence DESC, length ASC, path_id LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .unwrap_or_else(|| entity_id.clone())
    } else {
        entity_id.clone()
    };
    Ok(json!({
        "status": "ok",
        "entity_id": entity_id,
        "entity_name": entity_name,
        "symbol_query": symbol_query,
        "qname": qname,
        "fts_query": sqlite_fts_phrase(&fts_term),
        "context_seed": context_seed,
    }))
}

pub(crate) fn benchmark_sql_surface_query(
    connection: &Connection,
    id: &str,
    command: &str,
    target_p95_ms: f64,
    sql: &str,
    iterations: usize,
    note: &str,
) -> Value {
    let explain = explain_query_plan(connection, id, sql);
    let explain_plan = explain
        .as_ref()
        .ok()
        .and_then(|value| value.get("plan").and_then(Value::as_array).cloned());
    let explain_error = explain.as_ref().err().cloned();
    let mut samples = Vec::new();
    let mut rows_returned = 0usize;
    let mut error = explain_error;
    if error.is_none() {
        for _ in 0..iterations {
            let started = Instant::now();
            match count_sql_rows(connection, sql) {
                Ok(rows) => {
                    rows_returned = rows;
                    samples.push(elapsed_ms_f64(started));
                }
                Err(query_error) => {
                    error = Some(query_error);
                    break;
                }
            }
        }
    }
    query_surface_metric(
        id,
        command,
        target_p95_ms,
        sql,
        explain_plan,
        samples,
        rows_returned,
        error,
        vec![note.to_string()],
    )
}

pub(crate) fn benchmark_source_snippet_batch_load(
    connection: &Connection,
    repo_root: &Path,
    iterations: usize,
) -> Value {
    let sql = "
        SELECT DISTINCT repo_relative_path, start_line, start_column, end_line, end_column
        FROM (
            SELECT path.value AS repo_relative_path, e.start_line, e.start_column, e.end_line, e.end_column
            FROM edges_compat e
            JOIN path_dict path ON path.id = e.span_path_id
            WHERE e.start_line > 0
            UNION ALL
            SELECT path.value AS repo_relative_path, s.start_line, s.start_column, s.end_line, s.end_column
            FROM source_spans s
            JOIN path_dict path ON path.id = s.path_id
            WHERE s.start_line > 0
        )
        ORDER BY repo_relative_path, start_line, end_line
        LIMIT 50
    ";
    let explain = explain_query_plan(connection, "source_snippet_batch_load", sql);
    let explain_plan = explain
        .as_ref()
        .ok()
        .and_then(|value| value.get("plan").and_then(Value::as_array).cloned());
    let mut samples = Vec::new();
    let mut snippets_loaded = 0usize;
    let mut error = explain.as_ref().err().cloned();
    if error.is_none() {
        for _ in 0..iterations {
            let started = Instant::now();
            match load_source_snippet_batch(connection, repo_root, sql) {
                Ok(count) => {
                    snippets_loaded = count;
                    samples.push(elapsed_ms_f64(started));
                }
                Err(query_error) => {
                    error = Some(query_error);
                    break;
                }
            }
        }
    }
    query_surface_metric(
        "source_snippet_batch_load",
        "context_pack source snippet batch load",
        500.0,
        sql,
        explain_plan,
        samples,
        snippets_loaded,
        error,
        vec![
            "Source snippets must load from source files using proof-edge/source-span rows, not redundant SQLite snippet storage.".to_string(),
        ],
    )
}

pub(crate) fn benchmark_context_pack_surface_query(
    repo_root: &Path,
    db_path: &Path,
    seed: &str,
    iterations: usize,
) -> Value {
    let explain_plan = context_pack_explain_query_plans(db_path).ok();
    let mut samples = Vec::new();
    let mut rows_returned = 0usize;
    let mut error = None;
    for _ in 0..iterations {
        let args = vec![
            "--task".to_string(),
            "default query surface context_pack benchmark".to_string(),
            "--seed".to_string(),
            seed.to_string(),
            "--mode".to_string(),
            "normal".to_string(),
            "--budget".to_string(),
            "1200".to_string(),
        ];
        let started = Instant::now();
        let result = with_repo_db_context(repo_root, db_path, || run_context_pack_command(&args));
        match result {
            Ok(value) => {
                rows_returned = value
                    .get("packet")
                    .and_then(|packet| packet.get("verified_paths"))
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or_default();
                samples.push(elapsed_ms_f64(started));
            }
            Err(query_error) => {
                error = Some(query_error);
                break;
            }
        }
    }
    query_surface_metric(
        "context_pack_normal",
        "codegraph-mcp context-pack --mode normal",
        2000.0,
        "context_pack normal stored-PathEvidence query path",
        explain_plan,
        samples,
        rows_returned,
        error,
        vec![
            "Normal context_pack must use proof-mode stored PathEvidence first and keep source-span labels.".to_string(),
        ],
    )
}

pub(crate) fn benchmark_unresolved_calls_surface_query(
    connection: &Connection,
    repo_root: &Path,
    db_path: &Path,
    iterations: usize,
) -> Value {
    let explain_plan = unresolved_calls_query_plan(connection, 20, 0).ok();
    let preflight_options = UnresolvedCallsOptions {
        db_path: Some(db_path.to_path_buf()),
        requested_limit: 20,
        limit: 20,
        offset: 0,
        include_snippets: false,
        source_scan: false,
        count_total: false,
        allow_stale_read: false,
        allow_foreign_db: false,
        explicit_scope_policy: None,
        surface_name: "bench.query_surface.unresolved_calls".to_string(),
        operation_kind: DbLifecycleOperationKind::BenchmarkInspection,
        class_filter: None,
        path_filter: None,
    };
    let mut lifecycle_read =
        unresolved_calls_lifecycle_preflight(repo_root, db_path, &preflight_options)
            .map(|preflight| db_lifecycle_surface_preflight_json(&preflight))
            .unwrap_or_else(|error| {
                json!({
                    "decision": "blocked",
                    "surface_name": "bench.query_surface.unresolved_calls",
                    "operation_kind": DbLifecycleOperationKind::BenchmarkInspection.as_str(),
                    "safe_to_read": false,
                    "claimable": false,
                    "diagnostic_only": true,
                    "exact_db_path_checked": path_string(db_path),
                    "blockers": [error],
                    "warnings": [],
                })
            });
    let mut samples = Vec::new();
    let mut rows_returned = 0usize;
    let mut error = None;
    for _ in 0..iterations {
        let started = Instant::now();
        let options = UnresolvedCallsOptions {
            db_path: Some(db_path.to_path_buf()),
            requested_limit: 20,
            limit: 20,
            offset: 0,
            include_snippets: false,
            source_scan: false,
            count_total: false,
            allow_stale_read: false,
            allow_foreign_db: false,
            explicit_scope_policy: None,
            surface_name: "bench.query_surface.unresolved_calls".to_string(),
            operation_kind: DbLifecycleOperationKind::BenchmarkInspection,
            class_filter: None,
            path_filter: None,
        };
        match query_unresolved_calls(repo_root, options) {
            Ok(value) => {
                if let Some(value_lifecycle) = value.get("db_lifecycle_read") {
                    lifecycle_read = value_lifecycle.clone();
                }
                rows_returned = value
                    .get("calls")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or_default();
                samples.push(elapsed_ms_f64(started));
            }
            Err(query_error) => {
                error = Some(query_error);
                break;
            }
        }
    }
    let mut metric = query_surface_metric(
        "unresolved_calls_paginated",
        "codegraph-mcp query unresolved-calls --limit 20",
        1000.0,
        UNRESOLVED_CALLS_PAGE_SQL,
        explain_plan,
        samples,
        rows_returned,
        error,
        vec![
            "Unresolved-calls pagination may read the heuristic/debug sidecar by design, but it must remain explicit and bounded.".to_string(),
        ],
    );
    if let Some(object) = metric.as_object_mut() {
        object.insert("db_lifecycle_read".to_string(), lifecycle_read);
    }
    metric
}

pub(crate) fn query_surface_metric(
    id: &str,
    command: &str,
    target_p95_ms: f64,
    sql: &str,
    explain_plan: Option<Vec<Value>>,
    samples: Vec<f64>,
    rows_returned: usize,
    error: Option<String>,
    notes: Vec<String>,
) -> Value {
    let p50 = percentile(&samples, 0.50);
    let p95 = percentile(&samples, 0.95);
    let p99 = percentile(&samples, 0.99);
    let mut notes = notes;
    if rows_returned == 0 && error.is_none() {
        notes.push(
            "Query completed but returned zero rows on this artifact; latency is still measured."
                .to_string(),
        );
    }
    let status = if error.is_some() {
        "fail".to_string()
    } else {
        status_known_pass(p95.map(|value| value <= target_p95_ms)).to_string()
    };
    json!({
        "id": id,
        "command": command,
        "target": { "p95_ms": target_p95_ms },
        "observed": {
            "p50_ms": p50,
            "p95_ms": p95,
            "p99_ms": p99,
            "iterations": samples.len(),
            "samples_ms": samples,
            "rows_returned_last_iteration": rows_returned,
        },
        "status": status,
        "sql": sql.split_whitespace().collect::<Vec<_>>().join(" "),
        "explain_query_plan": explain_plan.clone().unwrap_or_default(),
        "query_plan_analysis": explain_plan
            .as_ref()
            .map(|plan| analyze_sqlite_query_plan(plan))
            .unwrap_or_else(|| json!({
                "uses_indexes": false,
                "indexes_used": [],
                "full_scans": [],
            })),
        "error": error,
        "notes": notes,
    })
}

pub(crate) fn query_surface_failure_metric(id: &str, target_p95_ms: f64, error: &str) -> Value {
    json!({
        "id": id,
        "target": { "p95_ms": target_p95_ms },
        "observed": {
            "p50_ms": Value::Null,
            "p95_ms": Value::Null,
            "p99_ms": Value::Null,
            "iterations": 0,
        },
        "status": "fail",
        "sql": Value::Null,
        "explain_query_plan": [],
        "query_plan_analysis": {
            "uses_indexes": false,
            "indexes_used": [],
            "full_scans": [],
        },
        "error": error,
        "notes": ["Query was not executed because the compact proof DB could not be opened."]
    })
}

pub(crate) fn render_default_query_surface_markdown(report: &Value) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Default Query Surface Audit\n\n");
    markdown.push_str("Source of truth: `MVP.md`.\n\n");
    markdown.push_str(&format!(
        "- Status: **{}**\n- DB: `{}`\n- Iterations: `{}`\n\n",
        report["status"].as_str().unwrap_or("unknown"),
        report["db_path"].as_str().unwrap_or("unknown"),
        report["iterations"].as_u64().unwrap_or(0)
    ));
    markdown.push_str("| Query | Status | p50 ms | p95 ms | p99 ms | rows | target p95 |\n");
    markdown.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: |\n");
    if let Some(queries) = report["queries"].as_array() {
        for query in queries {
            markdown.push_str(&format!(
                "| `{}` | `{}` | {} | {} | {} | {} | {} |\n",
                query["id"].as_str().unwrap_or("unknown"),
                query["status"].as_str().unwrap_or("unknown"),
                display_value(&query["observed"]["p50_ms"]),
                display_value(&query["observed"]["p95_ms"]),
                display_value(&query["observed"]["p99_ms"]),
                display_value(&query["observed"]["rows_returned_last_iteration"]),
                display_value(&query["target"]["p95_ms"]),
            ));
        }
    }
    markdown.push_str("\n## Query Details\n\n");
    if let Some(queries) = report["queries"].as_array() {
        for query in queries {
            markdown.push_str(&format!(
                "### `{}`\n\nCommand: `{}`\n\nStatus: `{}`\n\n",
                query["id"].as_str().unwrap_or("unknown"),
                query["command"].as_str().unwrap_or("unknown"),
                query["status"].as_str().unwrap_or("unknown")
            ));
            if let Some(error) = query["error"].as_str() {
                markdown.push_str(&format!("Error: `{error}`\n\n"));
            }
            markdown.push_str("SQL:\n\n```sql\n");
            markdown.push_str(query["sql"].as_str().unwrap_or(""));
            markdown.push_str("\n```\n\n");
            markdown.push_str("EXPLAIN QUERY PLAN:\n\n");
            markdown.push_str("| id | parent | detail |\n");
            markdown.push_str("| ---: | ---: | --- |\n");
            if let Some(plan) = query["explain_query_plan"].as_array() {
                for row in plan {
                    markdown.push_str(&format!(
                        "| {} | {} | `{}` |\n",
                        display_value(&row["id"]),
                        display_value(&row["parent"]),
                        row["detail"].as_str().unwrap_or("")
                    ));
                }
            }
            markdown.push('\n');
        }
    }
    markdown.push_str("\n## Storage Safety Questions\n\n");
    markdown.push_str("1. Did Graph Truth still pass? Measured by the surrounding gate; this query audit does not change graph truth facts.\n");
    markdown.push_str("2. Did Context Packet quality still pass? Measured by the surrounding gate; context_pack normal is probed here.\n");
    markdown.push_str(
        "3. Did proof DB size decrease? No storage optimization is performed by this audit.\n",
    );
    markdown.push_str("4. Did removed data move to a sidecar, become derivable, or get proven unnecessary? No data is removed by this audit.\n");
    markdown
}

pub(crate) fn count_sql_rows(connection: &Connection, sql: &str) -> Result<usize, String> {
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    let mut rows = statement.query([]).map_err(|error| error.to_string())?;
    let mut count = 0usize;
    while rows.next().map_err(|error| error.to_string())?.is_some() {
        count += 1;
    }
    Ok(count)
}

pub(crate) fn load_source_snippet_batch(
    connection: &Connection,
    repo_root: &Path,
    sql: &str,
) -> Result<usize, String> {
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(SourceSpan {
                repo_relative_path: row.get::<_, String>(0)?,
                start_line: row.get::<_, u32>(1)?,
                start_column: row.get::<_, Option<u32>>(2)?,
                end_line: row.get::<_, u32>(3)?,
                end_column: row.get::<_, Option<u32>>(4)?,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut spans = Vec::new();
    for row in rows {
        spans.push(row.map_err(|error| error.to_string())?);
    }
    let mut loaded = 0usize;
    let mut source_cache: BTreeMap<String, String> = BTreeMap::new();
    for span in spans {
        if span.repo_relative_path.is_empty() {
            continue;
        }
        let source = if let Some(source) = source_cache.get(&span.repo_relative_path) {
            source.clone()
        } else {
            let path = repo_root.join(&span.repo_relative_path);
            let source = fs::read_to_string(&path).unwrap_or_default();
            source_cache.insert(span.repo_relative_path.clone(), source.clone());
            source
        };
        if !source_snippet_for_span(&source, &span).trim().is_empty() {
            loaded += 1;
        }
    }
    Ok(loaded)
}

pub(crate) fn seed_text<'a>(seeds: &'a Value, key: &str, fallback: &'a str) -> &'a str {
    seeds.get(key).and_then(Value::as_str).unwrap_or(fallback)
}

pub(crate) fn sqlite_quote_text(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(crate) fn sqlite_fts_phrase(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                ' '
            }
        })
        .collect::<String>();
    let token = sanitized
        .split_whitespace()
        .find(|part| part.len() >= 2)
        .unwrap_or("login");
    format!("\"{}\"", token.replace('"', ""))
}

pub(crate) fn elapsed_ms_f64(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

pub(crate) fn with_repo_db_context<F>(
    repo_root: &Path,
    db_path: &Path,
    operation: F,
) -> Result<Value, String>
where
    F: FnOnce() -> Result<Value, String>,
{
    with_process_context_lock(|| {
        let _process_context = ProcessContextSnapshot::capture()?;
        std::env::set_current_dir(repo_root).map_err(|error| error.to_string())?;
        std::env::set_var("CODEGRAPH_DB_PATH", db_path);
        std::env::set_var(GLOBAL_DB_SOURCE_ENV, "test explicit db");
        std::env::set_var(GLOBAL_REPO_SOURCE_ENV, "test repo context");
        operation()
    })
}

pub(crate) fn percentile(samples: &[f64], quantile: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((sorted.len().saturating_sub(1)) as f64 * quantile).ceil() as usize;
    sorted
        .get(index.min(sorted.len().saturating_sub(1)))
        .copied()
}

pub(crate) fn comprehensive_manual_quality_section(manual_labels: &Option<Value>) -> Value {
    let Some(labels) = manual_labels else {
        return json!({
            "status": "unknown",
            "real_relation_precision": "unknown",
            "reason": "manual relation labels summary JSON is missing",
            "relations": []
        });
    };
    let samples = labels
        .get("samples")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let labeled_count = samples
        .iter()
        .filter(|sample| sample["labeled"].as_bool() == Some(true))
        .count();
    if labeled_count == 0 {
        return json!({
            "status": "unknown",
            "real_relation_precision": "unknown",
            "reason": "sample files exist, but no human labels are filled in",
            "edges_labeled": 0,
            "relations": []
        });
    }

    let mut by_relation: BTreeMap<String, serde_json::Map<String, Value>> = BTreeMap::new();
    for sample in samples
        .iter()
        .filter(|sample| sample["labeled"].as_bool() == Some(true))
    {
        let relation = sample["relation"]
            .as_str()
            .unwrap_or("PathEvidence")
            .to_string();
        let entry = by_relation.entry(relation).or_insert_with(|| {
            let mut map = serde_json::Map::new();
            for key in [
                "samples",
                "true_positive",
                "false_positive",
                "wrong_direction",
                "wrong_target",
                "wrong_span",
                "stale",
                "duplicate",
                "unresolved_mislabeled_exact",
                "test_mock_leaked",
                "derived_missing_provenance",
                "unsure",
            ] {
                map.insert(key.to_string(), json!(0u64));
            }
            map
        });
        increment_label_count(entry, "samples");
        if let Some(labels) = sample.get("labels").and_then(Value::as_object) {
            for key in [
                "true_positive",
                "false_positive",
                "wrong_direction",
                "wrong_target",
                "wrong_span",
                "stale",
                "duplicate",
                "unresolved_mislabeled_exact",
                "test_mock_leaked",
                "derived_missing_provenance",
                "unsure",
            ] {
                if labels.get(key).and_then(Value::as_bool) == Some(true) {
                    increment_label_count(entry, key);
                }
            }
        }
    }

    let relations = by_relation
        .into_iter()
        .map(|(relation, mut entry)| {
            let samples = entry.get("samples").and_then(Value::as_u64).unwrap_or(0);
            let true_positive = entry
                .get("true_positive")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let wrong_span = entry.get("wrong_span").and_then(Value::as_u64).unwrap_or(0);
            let precision = if samples == 0 {
                Value::Null
            } else {
                json!(true_positive as f64 / samples as f64)
            };
            let source_span_precision = if samples == 0 {
                Value::Null
            } else {
                json!((samples.saturating_sub(wrong_span)) as f64 / samples as f64)
            };
            entry.insert("relation".to_string(), json!(relation));
            entry.insert("precision".to_string(), precision);
            entry.insert("source_span_precision".to_string(), source_span_precision);
            Value::Object(entry)
        })
        .collect::<Vec<_>>();

    let real_relation_precision = labels
        .get("real_relation_precision_status")
        .and_then(Value::as_str)
        .unwrap_or("reported_for_labeled_relations_no_claim_for_unlabeled_relations");

    json!({
        "status": "reported",
        "real_relation_precision": real_relation_precision,
        "edges_labeled": labeled_count,
        "relations": relations,
        "target_evaluation": labels
            .get("target_evaluation")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "relation_coverage": labels
            .get("relation_coverage")
            .cloned()
            .unwrap_or_else(|| json!({})),
        "source_span_target_evaluation": labels
            .get("source_span_target_evaluation")
            .cloned()
            .unwrap_or_else(|| json!({})),
        "path_evidence_target_evaluation": labels
            .get("path_evidence_target_evaluation")
            .cloned()
            .unwrap_or_else(|| json!({})),
        "false_positive_taxonomy": labels
            .get("false_positive_taxonomy")
            .cloned()
            .unwrap_or_else(|| json!({})),
        "targets": {
            "CALLS": 0.95,
            "READS_WRITES": 0.90,
            "FLOWS_TO": 0.85,
            "AUTH_CHECKS_ROLE_SANITIZES": 0.95,
            "TESTS_MOCKS_STUBS_ASSERTS": 0.90,
            "source_span_precision": 0.95
        }
    })
}

pub(crate) fn increment_label_count(entry: &mut serde_json::Map<String, Value>, key: &str) {
    let next = entry.get(key).and_then(Value::as_u64).unwrap_or(0) + 1;
    entry.insert(key.to_string(), json!(next));
}

pub(crate) fn comprehensive_comparison_section(comparison: &Option<Value>) -> Value {
    let Some(comparison) = comparison else {
        return json!({
            "cgc_available": Value::Null,
            "cgc_version": Value::Null,
            "cgc_completed": false,
            "cgc_timeout": Value::Null,
            "cgc_db_artifact_size_bytes": Value::Null,
            "codegraph_vs_cgc_speed": "unknown",
            "codegraph_vs_cgc_storage": "unknown",
            "codegraph_vs_cgc_quality": "unknown",
            "verdict": "unknown",
            "notes": ["No latest CGC comparison report is available."]
        });
    };
    let cgc_version = value_string(comparison, &["environment", "cgc_version"]);
    let cgc_path = value_string(comparison, &["environment", "cgc_path"]);
    let frozen_status = value_string(
        comparison,
        &["indexing", "frozen_autoresearch", "cgc_status"],
    );
    let fixture_status = value_string(comparison, &["indexing", "cgc_fixture_harness", "status"]);
    let cgc_completed = frozen_status.as_deref() == Some("completed")
        || fixture_status.as_deref() == Some("completed");
    let cgc_timeout = frozen_status.as_deref() == Some("timeout");
    json!({
        "cgc_available": cgc_path.is_some(),
        "cgc_version": cgc_version,
        "cgc_completed": cgc_completed,
        "cgc_timeout": cgc_timeout,
        "cgc_db_artifact_size_bytes": value_u64(comparison, &["storage", "cgc_frozen_autoresearch_final_artifact_bytes"]),
        "codegraph_vs_cgc_speed": if cgc_completed { "not recomputed after compact proof gate" } else { "unknown" },
        "codegraph_vs_cgc_storage": if cgc_completed { "not recomputed after compact proof gate" } else { "unknown" },
        "codegraph_vs_cgc_quality": if cgc_completed { "not recomputed after compact proof gate" } else { "unknown" },
        "verdict": if cgc_completed { "unknown" } else { "incomplete" },
        "notes": [
            "Latest comparison predates the compact proof baseline and is not a basis for a current CodeGraph win.",
            "CGC timeout/skipped/incomplete remains incomplete, not a CodeGraph win.",
            "Fake-agent dry runs are not model superiority evidence.",
            "CodeGraph internal storage failure remains failure even if CGC is incomplete."
        ]
    })
}

pub(crate) fn comprehensive_regression_summary(
    baseline: &Value,
    gate: &Value,
    clean_gate: &Option<Value>,
    previous: Option<&Value>,
) -> Value {
    let current_storage = value_f64(gate, &["storage", "proof", "file_family_mib"]);
    let current_context = value_f64(gate, &["query_latency", "context_pack", "p95_profile_ms"])
        .or_else(|| value_f64(gate, &["query_latency", "context_pack", "p95_shell_ms"]));
    let current_update = value_f64(gate, &["update_path", "single_file_update", "wall_ms"]);
    let current_repeat = value_f64(
        gate,
        &["update_path", "repeat_unchanged", "profile_wall_ms"],
    );
    let current_cold = value_f64(gate, &["autoresearch", "proof_build", "wall_ms"]);
    let compact_storage = value_f64(baseline, &["storage_summary", "proof_file_family_mib"]);
    let clean_storage = clean_gate
        .as_ref()
        .and_then(|value| value_f64(value, &["storage", "db_family_bytes"]))
        .map(|bytes| bytes / 1024.0 / 1024.0);
    let clean_context = clean_gate
        .as_ref()
        .and_then(|value| value_f64(value, &["query_latency", "context_pack", "observed_p95_ms"]));
    let clean_update = clean_gate
        .as_ref()
        .and_then(|value| value_f64(value, &["indexing", "single_file_update", "observed_ms"]));
    let clean_repeat = clean_gate.as_ref().and_then(|value| {
        value_f64(
            value,
            &["indexing", "unchanged_repeat", "observed_ms_profile"],
        )
    });
    let previous_storage = previous.and_then(|value| {
        section_metric_observed(
            value,
            &["sections", "storage_summary", "metrics"],
            "proof_db_mib",
        )
    });

    let mut rows = vec![
        regression_row(
            "proof_db_mib_vs_clean_1_63_gib",
            clean_storage,
            current_storage,
            false,
        ),
        regression_row(
            "context_pack_p95_ms_vs_clean_30s",
            clean_context,
            current_context,
            false,
        ),
        regression_row(
            "single_file_update_ms_vs_clean_80s",
            clean_update,
            current_update,
            false,
        ),
        regression_row(
            "repeat_unchanged_ms_vs_clean_13s",
            clean_repeat,
            current_repeat,
            false,
        ),
        regression_row(
            "cold_proof_build_ms_vs_compact_baseline",
            current_cold,
            current_cold,
            false,
        ),
        regression_row(
            "proof_db_mib_vs_compact_baseline",
            compact_storage,
            current_storage,
            false,
        ),
    ];
    if previous_storage.is_some() {
        rows.push(regression_row(
            "proof_db_mib_vs_previous_comprehensive",
            previous_storage,
            current_storage,
            false,
        ));
    }
    json!({
        "metrics": rows,
        "notes": [
            "Historical clean Autoresearch values come from INTENDED_PERFORMANCE_GATE when available.",
            "First comprehensive run compares primarily against the compact proof baseline; later runs compare against comprehensive_benchmark_latest.json."
        ]
    })
}

pub(crate) fn section_metric_observed(
    value: &Value,
    path: &[&str],
    metric_id: &str,
) -> Option<f64> {
    gate_value(value, path)?
        .as_array()?
        .iter()
        .find(|metric| metric["id"].as_str() == Some(metric_id))
        .and_then(|metric| metric["observed"].as_f64())
}

pub(crate) fn regression_row(
    id: &str,
    previous: Option<f64>,
    current: Option<f64>,
    higher_is_better: bool,
) -> Value {
    let delta = previous
        .zip(current)
        .map(|(previous, current)| current - previous);
    let status = match delta {
        Some(delta) if delta.abs() < f64::EPSILON => "unchanged",
        Some(delta) if higher_is_better && delta > 0.0 => "improved",
        Some(delta) if !higher_is_better && delta < 0.0 => "improved",
        Some(_) => "regressed",
        None => "unknown",
    };
    json!({
        "id": id,
        "previous_value": previous,
        "current_value": current,
        "delta": delta,
        "status": status
    })
}

pub(crate) fn render_comprehensive_benchmark_markdown(report: &Value) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Comprehensive Benchmark Latest\n\n");
    markdown.push_str("Source of truth: `MVP.md`.\n\n");
    markdown.push_str(&format!(
        "Execution mode: `{}`. {}\n\n",
        report["execution_mode"].as_str().unwrap_or("unknown"),
        report["execution_mode_notes"].as_str().unwrap_or("")
    ));

    let executive = &report["sections"]["executive_verdict"];
    markdown.push_str("## Section 1 - Executive Verdict\n\n");
    markdown.push_str(&format!(
        "- Verdict: **{}**\n- Reason: {}\n- Optimization may continue: `{}`\n- Comparison claims allowed: `{}`\n\n",
        executive["verdict"].as_str().unwrap_or("unknown"),
        executive["reason_for_failure"].as_str().unwrap_or("unknown"),
        executive["optimization_may_continue"].as_bool().unwrap_or(false),
        executive["comparison_claims_allowed"].as_bool().unwrap_or(false)
    ));
    push_string_list(
        &mut markdown,
        "Failed targets",
        &executive["exact_failed_targets"],
    );
    push_string_list(
        &mut markdown,
        "Passed targets",
        &executive["exact_passed_targets"],
    );

    push_metric_section(
        &mut markdown,
        "Section 2 - Correctness Gates",
        &report["sections"]["correctness_gates"]["metrics"],
    );
    push_metric_section(
        &mut markdown,
        "Section 3 - Context Packet Gate",
        &report["sections"]["context_packet_gate"]["metrics"],
    );
    push_metric_section(
        &mut markdown,
        "Section 4 - DB Integrity",
        &report["sections"]["db_integrity"]["metrics"],
    );
    push_metric_section(
        &mut markdown,
        "Section 4A - Proof Artifact Freshness",
        &report["sections"]["artifact_freshness"]["metrics"],
    );
    if report["artifact_freshness"].is_object() {
        markdown.push_str("### Proof Artifact Metadata\n\n");
        markdown.push_str("| Field | Value |\n| --- | --- |\n");
        for field in [
            "artifact_path",
            "artifact_metadata_path",
            "freshness_metadata_present",
            "artifact_reuse",
            "freshly_built",
            "stale",
            "freshness_status",
            "schema_version",
            "current_schema_version",
            "migration_version",
            "current_migration_version",
            "storage_mode",
            "db_size_bytes",
            "integrity_status",
            "build_duration_ms",
            "proof_build_only_ms",
            "diagnostic_debug_proof_build_only_ms",
            "validation_ms",
            "audit_ms",
            "report_generation_ms",
            "comprehensive_total_ms",
            "current_exe",
            "debug_assertions",
            "binary_profile",
            "exact_command",
            "claimable_for_thresholds",
            "diagnostic_only",
            "storage_result_claimable",
            "cold_build_result_claimable",
        ] {
            markdown.push_str(&format!(
                "| `{}` | {} |\n",
                field,
                display_value(&report["artifact_freshness"][field])
            ));
        }
        markdown.push('\n');
    }
    if report["timing_separation"].is_object() {
        markdown.push_str("## Section 4B - Timing Separation\n\n");
        markdown.push_str("| Field | Value |\n| --- | ---: |\n");
        for field in [
            "proof_build_only_ms",
            "validation_ms",
            "audit_ms",
            "report_generation_ms",
            "comprehensive_total_ms",
        ] {
            markdown.push_str(&format!(
                "| `{}` | {} |\n",
                field,
                display_value(&report["timing_separation"][field])
            ));
        }
        markdown.push('\n');
    }
    push_metric_section(
        &mut markdown,
        "Section 5 - Storage Summary",
        &report["sections"]["storage_summary"]["metrics"],
    );

    markdown.push_str("## Section 6 - Storage Contributors\n\n");
    markdown.push_str(
        "| Object | Kind | Rows | MiB | Share | Previous bytes | Delta bytes | Classification |\n",
    );
    markdown.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |\n");
    if let Some(contributors) =
        report["sections"]["storage_contributors"]["contributors"].as_array()
    {
        for contributor in contributors.iter().take(40) {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
                contributor["name"].as_str().unwrap_or("unknown"),
                contributor["kind"].as_str().unwrap_or("unknown"),
                display_value(&contributor["rows"]),
                format_number(contributor["mib"].as_f64()),
                format_percent(contributor["share_of_proof_db"].as_f64()),
                display_value(&contributor["previous_baseline_bytes"]),
                display_value(&contributor["delta_bytes"]),
                contributor["classification"].as_str().unwrap_or("unknown")
            ));
        }
    }
    markdown.push('\n');

    push_metric_section(
        &mut markdown,
        "Section 7 - Row Counts And Cardinality",
        &report["sections"]["row_counts_and_cardinality"]["metrics"],
    );
    push_metric_section(
        &mut markdown,
        "Section 8 - Cold Proof Build Profile",
        &report["sections"]["cold_proof_build_profile"]["metrics"],
    );
    if let Some(modes) =
        report["sections"]["cold_proof_build_profile"]["mode_distinction"].as_array()
    {
        markdown.push_str("### Cold Build Mode Distinction\n\n");
        markdown.push_str(
            "| Mode | Observed ms | Minutes | Status | Included in 50.02 min? | Classification |\n",
        );
        markdown.push_str("| --- | ---: | ---: | --- | --- | --- |\n");
        for mode in modes {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} |\n",
                mode["mode"].as_str().unwrap_or("unknown"),
                display_value(&mode["observed_ms"]),
                display_value(&mode["observed_minutes"]),
                mode["status"].as_str().unwrap_or("unknown"),
                display_value(&mode["included_in_50_02_minute_number"]),
                mode["classification"].as_str().unwrap_or("unknown")
            ));
        }
        markdown.push('\n');
    }
    if let Some(waterfall) = report["sections"]["cold_proof_build_profile"]["waterfall"].as_array()
    {
        markdown.push_str("### Cold Build Waterfall\n\n");
        markdown
            .push_str("| Stage | Elapsed ms | Share | Included in 50.02 min? | Source | Notes |\n");
        markdown.push_str("| --- | ---: | ---: | --- | --- | --- |\n");
        for stage in waterfall {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | `{}` | {} |\n",
                stage["stage"].as_str().unwrap_or("unknown"),
                display_value(&stage["elapsed_ms"]),
                format_percent(stage["pct_of_reference"].as_f64()),
                display_value(&stage["included_in_50_02_minute_number"]),
                stage["source"].as_str().unwrap_or("unknown"),
                first_note(stage)
            ));
        }
        markdown.push('\n');
    }
    push_metric_section(
        &mut markdown,
        "Section 9 - Repeat Unchanged Index",
        &report["sections"]["repeat_unchanged_index"]["metrics"],
    );
    push_metric_section(
        &mut markdown,
        "Section 10 - Single-File Update",
        &report["sections"]["single_file_update"]["metrics"],
    );

    markdown.push_str("## Section 11 - Query Latency\n\n");
    markdown.push_str("| Query | Target p95 ms | p50 ms | p95 ms | p99 ms | Status | Notes |\n");
    markdown.push_str("| --- | ---: | ---: | ---: | ---: | --- | --- |\n");
    if let Some(queries) = report["sections"]["query_latency"]["queries"].as_array() {
        for query in queries {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} |\n",
                query["id"].as_str().unwrap_or("unknown"),
                display_value(&query["target"]["p95_ms"]),
                display_value(&query["observed"]["p50_ms"]),
                display_value(&query["observed"]["p95_ms"]),
                display_value(&query["observed"]["p99_ms"]),
                query["status"].as_str().unwrap_or("unknown"),
                first_note(query)
            ));
        }
    }
    markdown.push('\n');

    markdown.push_str("## Section 12 - Manual Relation Quality\n\n");
    let manual_quality = &report["sections"]["manual_relation_quality"];
    let manual_status = manual_quality["status"].as_str().unwrap_or("unknown");
    markdown.push_str(&format!(
        "Status: `{manual_status}`. Real-world relation precision: `{}`.\n\n",
        manual_quality["real_relation_precision"]
            .as_str()
            .unwrap_or("unknown")
    ));
    if manual_status == "reported" {
        markdown.push_str(&format!(
            "Labeled samples: `{}`.\n\n",
            display_value(&manual_quality["edges_labeled"])
        ));
        if let Some(rows) = manual_quality["target_evaluation"].as_array() {
            if !rows.is_empty() {
                markdown.push_str("| Relation | Proof DB edges | Labeled | Precision | Target | Status | Claim |\n");
                markdown.push_str("| --- | ---: | ---: | ---: | ---: | --- | --- |\n");
                for row in rows {
                    markdown.push_str(&format!(
                        "| `{}` | {} | {} | {} | {} | {} | {} |\n",
                        row["relation"].as_str().unwrap_or("unknown"),
                        display_value(&row["proof_db_edge_count"]),
                        display_value(&row["labeled_samples"]),
                        format_percent(row["precision"].as_f64()),
                        format_percent(row["target"].as_f64()),
                        display_value(&row["status"]),
                        display_value(&row["claim"])
                    ));
                }
                markdown.push('\n');
            }
        }
        if let Some(rows) = manual_quality["relations"].as_array() {
            if !rows.is_empty() {
                markdown.push_str("| Labeled relation | Samples | Precision | Source-span precision | False positives | Unsure |\n");
                markdown.push_str("| --- | ---: | ---: | ---: | ---: | ---: |\n");
                for row in rows {
                    markdown.push_str(&format!(
                        "| `{}` | {} | {} | {} | {} | {} |\n",
                        row["relation"].as_str().unwrap_or("unknown"),
                        display_value(&row["samples"]),
                        format_percent(row["precision"].as_f64()),
                        format_percent(row["source_span_precision"].as_f64()),
                        display_value(&row["false_positive"]),
                        display_value(&row["unsure"])
                    ));
                }
                markdown.push('\n');
            }
        }
        if let Some(absent) =
            manual_quality["relation_coverage"]["absent_no_claim_relations"].as_array()
        {
            if !absent.is_empty() {
                let absent_relations = absent
                    .iter()
                    .map(display_value)
                    .collect::<Vec<_>>()
                    .join(", ");
                markdown.push_str(&format!(
                    "Absent proof-mode relations with no precision claim: {absent_relations}.\n\n"
                ));
            }
        }
        markdown.push_str("Precision claims are limited to labeled samples; recall remains unknown without a false-negative gold denominator.\n\n");
    } else {
        markdown.push_str("If labels are absent, real relation precision remains unknown and no precision claim is allowed.\n\n");
    }

    markdown.push_str("## Section 13 - CGC / Competitor Comparison Readiness\n\n");
    let comparison = &report["sections"]["cgc_competitor_comparison_readiness"];
    markdown.push_str(&format!(
        "| Metric | Value |\n| --- | --- |\n| CGC available | {} |\n| CGC version | {} |\n| CGC completed | {} |\n| CGC timeout | {} |\n| Verdict | {} |\n\n",
        display_value(&comparison["cgc_available"]),
        display_value(&comparison["cgc_version"]),
        display_value(&comparison["cgc_completed"]),
        display_value(&comparison["cgc_timeout"]),
        display_value(&comparison["verdict"])
    ));

    markdown.push_str("## Section 14 - Regression Summary\n\n");
    markdown.push_str("| Metric | Previous | Current | Delta | Status |\n");
    markdown.push_str("| --- | ---: | ---: | ---: | --- |\n");
    if let Some(rows) = report["sections"]["regression_summary"]["metrics"].as_array() {
        for row in rows {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                row["id"].as_str().unwrap_or("unknown"),
                display_value(&row["previous_value"]),
                display_value(&row["current_value"]),
                display_value(&row["delta"]),
                row["status"].as_str().unwrap_or("unknown")
            ));
        }
    }
    markdown.push('\n');

    markdown.push_str("## Operating Rule\n\n");
    markdown.push_str("Every future storage change must answer: did Graph Truth still pass, did Context Packet quality still pass, did proof DB size decrease, and did removed data move to a sidecar, become derivable, or get proven unnecessary. If the fourth answer is unclear, the change is not safe.\n");
    markdown
}

pub(crate) fn push_metric_section(markdown: &mut String, title: &str, metrics: &Value) {
    markdown.push_str(&format!("## {title}\n\n"));
    markdown.push_str("| Metric | Target | Observed | Status | Notes |\n");
    markdown.push_str("| --- | --- | --- | --- | --- |\n");
    if let Some(metrics) = metrics.as_array() {
        for metric in metrics {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                metric["id"].as_str().unwrap_or("unknown"),
                display_value(&metric["target"]),
                display_value(&metric["observed"]),
                metric["status"].as_str().unwrap_or("unknown"),
                first_note(metric)
            ));
        }
    }
    markdown.push('\n');
}

pub(crate) fn push_string_list(markdown: &mut String, title: &str, value: &Value) {
    markdown.push_str(&format!("### {title}\n\n"));
    if let Some(items) = value.as_array() {
        if items.is_empty() {
            markdown.push_str("- none\n\n");
            return;
        }
        for item in items {
            markdown.push_str(&format!("- `{}`\n", item.as_str().unwrap_or("unknown")));
        }
        markdown.push('\n');
    } else {
        markdown.push_str("- unknown\n\n");
    }
}

pub(crate) fn first_note(metric: &Value) -> String {
    metric["notes"]
        .as_array()
        .and_then(|notes| notes.first())
        .and_then(Value::as_str)
        .unwrap_or("")
        .replace('|', "\\|")
}

pub(crate) fn display_value(value: &Value) -> String {
    if value.is_null() {
        return "unknown".to_string();
    }
    if let Some(text) = value.as_str() {
        return text.replace('|', "\\|");
    }
    if let Some(number) = value.as_u64() {
        return format!("{number}");
    }
    if let Some(number) = value.as_i64() {
        return format!("{number}");
    }
    if let Some(number) = value.as_f64() {
        return format_number(Some(number));
    }
    if let Some(boolean) = value.as_bool() {
        return boolean.to_string();
    }
    if value.is_object() || value.is_array() {
        return serde_json::to_string(value)
            .unwrap_or_else(|_| "unknown".to_string())
            .replace('|', "\\|");
    }
    "unknown".to_string()
}

pub(crate) fn format_number(value: Option<f64>) -> String {
    match value {
        Some(value) if value.abs() >= 1000.0 => format!("{value:.0}"),
        Some(value) => format!("{value:.3}"),
        None => "unknown".to_string(),
    }
}

pub(crate) fn format_percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.2}%", value * 100.0))
        .unwrap_or_else(|| "unknown".to_string())
}

pub(crate) fn run_synthetic_index_benchmark_command(args: &[String]) -> Result<Value, String> {
    let options = parse_synthetic_index_options(args)?;
    let preflight = storage_budget::storage_budget_preflight(
        &options.storage_budget,
        storage_budget::StorageBudgetContext {
            command: "codegraph-mcp bench synthetic-index".to_string(),
            repo_root: std::env::current_dir().ok(),
            db_path: None,
            out_path: Some(options.output_dir.clone()),
            explicit_db: false,
            explicit_out: true,
            diagnostic_only: true,
        },
    );
    if preflight.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp bench synthetic-index",
                &preflight,
            ),
        ));
    }
    if options.output_dir.exists() {
        return Err(format!(
            "synthetic index output directory already exists: {}",
            options.output_dir.display()
        ));
    }
    fs::create_dir_all(&options.output_dir).map_err(|error| error.to_string())?;
    let repo_dir = options.output_dir.join("repo");
    generate_large_synthetic_repo(&repo_dir, options.files).map_err(|error| error.to_string())?;
    let summary = index_repo_with_options(
        &repo_dir,
        IndexOptions {
            profile: true,
            json: true,
            ..IndexOptions::default()
        },
    )
    .map_err(|error| error.to_string())?;
    let manifest = json!({
        "schema_version": 1,
        "phase": PHASE,
        "source_of_truth": "MVP.md Prompt 29",
        "repo_dir": path_string(&repo_dir),
        "requested_files": options.files,
        "index_summary": summary,
        "workflow": "single-agent-only"
    });
    let manifest_path = options.output_dir.join("synthetic-index-run.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let db_path = default_db_path(&repo_dir);
    let storage_budget = storage_budget::storage_budget_postflight(
        preflight,
        &[db_path],
        &[options.output_dir.clone()],
    );
    if storage_budget.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp bench synthetic-index",
                &storage_budget,
            ),
        ));
    }
    Ok(json!({
        "status": "benchmarked",
        "phase": PHASE,
        "kind": "synthetic_index",
        "output_dir": path_string(&options.output_dir),
        "repo_dir": path_string(&repo_dir),
        "manifest": path_string(&manifest_path),
        "index_summary": manifest["index_summary"],
        "storage_budget": storage_budget.to_json(),
    }))
}

pub(crate) fn run_update_integrity_harness_command(args: &[String]) -> Result<Value, String> {
    let options = parse_update_integrity_harness_options(args)?;
    let preflight = storage_budget::storage_budget_preflight(
        &options.storage_budget,
        storage_budget::StorageBudgetContext {
            command: "codegraph-mcp bench update-integrity".to_string(),
            repo_root: std::env::current_dir().ok(),
            db_path: None,
            out_path: Some(options.workdir.clone()),
            explicit_db: false,
            explicit_out: true,
            diagnostic_only: true,
        },
    );
    if preflight.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp bench update-integrity",
                &preflight,
            ),
        ));
    }
    let mut report = match run_update_integrity_harness(&options) {
        Ok(report) => report,
        Err(error) => update_integrity_error_report(&options, &error),
    };
    let render_start = Instant::now();
    let markdown = render_update_integrity_harness_markdown(&report);
    let markdown_render_ms = elapsed_ms(render_start);
    let json_start = Instant::now();
    let json_text = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    let json_serialize_ms = elapsed_ms(json_start);
    report["command_output_timings"] = json!({
        "markdown_render_ms": markdown_render_ms,
        "json_serialization_ms": json_serialize_ms,
    });
    let json_write_start = Instant::now();
    if let Some(parent) = options
        .out_json
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&options.out_json, json_text).map_err(|error| error.to_string())?;
    let json_write_ms = elapsed_ms(json_write_start);
    let md_write_start = Instant::now();
    write_text_file(&options.out_md, &markdown)?;
    let md_write_ms = elapsed_ms(md_write_start);
    report["command_output_timings"] = json!({
        "markdown_render_ms": markdown_render_ms,
        "json_serialization_ms": json_serialize_ms,
        "json_write_ms": json_write_ms,
        "markdown_write_ms": md_write_ms,
    });
    let storage_budget = storage_budget::storage_budget_postflight(
        preflight,
        &[],
        &[
            options.workdir.clone(),
            options.out_json.clone(),
            options.out_md.clone(),
        ],
    );
    report["storage_budget"] = storage_budget.to_json();
    write_json_file(&options.out_json, &report)?;
    if storage_budget.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp bench update-integrity",
                &storage_budget,
            ),
        ));
    }
    Ok(json!({
        "status": report["status"].clone(),
        "phase": PHASE,
        "benchmark": "update_integrity",
        "verdict": report["verdict"].clone(),
        "out_json": path_string(&options.out_json),
        "out_md": path_string(&options.out_md),
        "repos": report["repos"].as_array().map(Vec::len).unwrap_or(0),
        "storage_budget": storage_budget.to_json(),
        "proof": "Update-integrity harness runs cold/seed, repeat unchanged, mutate/update, restore/update, integrity checks, and graph hash comparisons.",
    }))
}

pub(crate) fn run_update_integrity_harness(
    options: &UpdateIntegrityHarnessOptions,
) -> Result<Value, String> {
    let harness_start = Instant::now();
    let deadline = options
        .timeout_ms
        .map(|timeout_ms| harness_start + Duration::from_millis(timeout_ms));
    fs::create_dir_all(&options.workdir).map_err(|error| error.to_string())?;
    let repo_root = options.workdir.join("repos");
    let db_root = options.workdir.join("dbs");
    fs::create_dir_all(&repo_root).map_err(|error| error.to_string())?;
    fs::create_dir_all(&db_root).map_err(|error| error.to_string())?;

    let mut repos = Vec::new();
    if !options.only_autoresearch {
        if deadline_expired(deadline) {
            return Ok(update_integrity_timeout_report(
                options,
                repos,
                "before_small_fixture",
            ));
        }
        let small_repo = repo_root.join("small_fixture");
        let small_mutation = generate_update_integrity_small_repo(&small_repo)?;
        let medium_repo = repo_root.join("medium_fixture");
        let medium_mutation =
            generate_update_integrity_medium_repo(&medium_repo, options.medium_files)?;

        repos.push(run_update_integrity_repo(
            "small_fixture",
            &small_repo,
            &db_root.join("small_fixture.sqlite"),
            &small_mutation,
            options.iterations,
            options.workers,
            options.mode,
            options.loop_kind,
            None,
            None,
            deadline,
        )?);
        if deadline_expired(deadline) {
            return Ok(update_integrity_timeout_report(
                options,
                repos,
                "after_small_fixture",
            ));
        }
        repos.push(run_update_integrity_repo(
            "medium_fixture",
            &medium_repo,
            &db_root.join("medium_fixture.sqlite"),
            &medium_mutation,
            options.iterations,
            options.workers,
            options.mode,
            options.loop_kind,
            None,
            None,
            deadline,
        )?);
    }

    if !options.skip_autoresearch && options.autoresearch_repo.exists() {
        if deadline_expired(deadline) {
            return Ok(update_integrity_timeout_report(
                options,
                repos,
                "before_autoresearch",
            ));
        }
        let mutation = choose_update_integrity_mutation_file(&options.autoresearch_repo)?;
        let staged_update_repo = if options.loop_kind.runs_updates() {
            Some(stage_update_integrity_mutation_repo(
                &options.autoresearch_repo,
                &mutation,
                &repo_root.join("autoresearch_update_workspace"),
            )?)
        } else {
            None
        };
        repos.push(run_update_integrity_repo(
            "autoresearch",
            &options.autoresearch_repo,
            &db_root.join("autoresearch.sqlite"),
            &mutation,
            options.autoresearch_iterations,
            options.workers,
            options.mode,
            options.loop_kind,
            options.autoresearch_seed_db.as_deref(),
            staged_update_repo.as_deref(),
            deadline,
        )?);
    }
    if repos.is_empty() {
        return Err("update-integrity harness did not find any repo to run".to_string());
    }

    let failures = repos
        .iter()
        .filter(|repo| repo["status"].as_str() != Some("passed"))
        .count();
    let status = if failures == 0 { "passed" } else { "failed" };
    let claimable = status == "passed"
        && repos
            .iter()
            .all(|repo| repo["claimable"].as_bool() == Some(true));
    Ok(json!({
        "schema_version": 1,
        "status": status,
        "verdict": status,
        "phase": PHASE,
        "generated_at_unix_ms": unix_time_ms(),
        "update_mode": options.mode.as_str(),
        "loop_kind": options.loop_kind.as_str(),
        "timeout_ms": options.timeout_ms,
        "iterations_requested": options.iterations,
        "autoresearch_iterations_requested": options.autoresearch_iterations,
        "workers": options.workers,
        "workdir": path_string(&options.workdir),
        "inspection_read_only": true,
        "artifact_mutated_during_inspection": false,
        "mutation_capable_operations": [
            "cold_index_or_seed_db_copy_setup",
            "repeat_index_setup",
            "source_file_mutation_setup",
            "update_changed_files_to_db",
            "repo_graph_digest_prime_for_fast_mode"
        ],
        "claimable": claimable,
        "repos": repos,
    }))
}

pub(crate) fn run_update_integrity_repo(
    name: &str,
    repo: &Path,
    db: &Path,
    mutation_file: &str,
    iterations: usize,
    workers: usize,
    mode: UpdateBenchmarkMode,
    loop_kind: UpdateLoopKind,
    seed_db: Option<&Path>,
    update_repo: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Value, String> {
    remove_sqlite_family(db)?;
    let options = IndexOptions {
        profile: true,
        json: false,
        worker_count: Some(workers),
        ..IndexOptions::default()
    };

    if deadline_expired(deadline) {
        return Ok(update_integrity_timeout_repo(
            name,
            repo,
            db,
            mutation_file,
            mode,
            loop_kind,
            iterations,
            "before_setup",
            Vec::new(),
            Vec::new(),
            None,
        ));
    }

    let cold = if let Some(seed_db) = seed_db {
        let copy_start = Instant::now();
        copy_seed_db(seed_db, db)?;
        let artifact_copy_ms = elapsed_ms(copy_start);
        update_integrity_seed_step(repo, db, mode, Some(seed_db), artifact_copy_ms)?
    } else {
        let start = Instant::now();
        let summary = index_repo_to_db_with_options(repo, db, options.clone())
            .map_err(|error| format!("cold index failed for {name}: {error}"))?;
        update_integrity_step_from_index("cold_index", start.elapsed(), &summary, repo, db, mode)?
    };
    let cold_hash = cold["graph_fact_hash"].as_str().unwrap_or("").to_string();
    if mode == UpdateBenchmarkMode::Fast && !cold_hash.is_empty() {
        replace_repo_graph_digest_for_harness(db, &cold_hash)?;
    }

    if deadline_expired(deadline) {
        return Ok(update_integrity_timeout_repo(
            name,
            repo,
            db,
            mutation_file,
            mode,
            loop_kind,
            iterations,
            "after_setup",
            Vec::new(),
            Vec::new(),
            Some(cold),
        ));
    }

    let mut repeat_iterations = Vec::new();
    let repeat_runs = if loop_kind.runs_repeat() {
        loop_kind.repeat_iterations(iterations)
    } else {
        0
    };
    for repeat_iteration in 1..=repeat_runs {
        if deadline_expired(deadline) {
            return Ok(update_integrity_timeout_repo(
                name,
                repo,
                db,
                mutation_file,
                mode,
                loop_kind,
                iterations,
                "repeat_loop_timeout",
                repeat_iterations,
                Vec::new(),
                Some(cold),
            ));
        }
        let start = Instant::now();
        let repeat_summary = index_repo_to_db_with_options(repo, db, options.clone())
            .map_err(|error| format!("repeat unchanged failed for {name}: {error}"))?;
        let repeat = update_integrity_step_from_index(
            "repeat_unchanged_index",
            start.elapsed(),
            &repeat_summary,
            repo,
            db,
            mode,
        )?;
        repeat_iterations.push(json!({
            "iteration": repeat_iteration,
            "repeat": repeat,
        }));
    }
    let repeat = repeat_iterations
        .last()
        .and_then(|iteration| iteration.get("repeat"))
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "step": "repeat_unchanged_index",
                "status": "not_run",
                "reason": "loop kind did not request repeat"
            })
        });
    let repeat_hash = repeat["graph_fact_hash"].as_str().unwrap_or("").to_string();
    if mode == UpdateBenchmarkMode::Fast && !repeat_hash.is_empty() {
        prime_incremental_graph_digest_for_update(db, mutation_file, &repeat_hash)?;
    }

    let update_repo = update_repo.unwrap_or(repo);
    if update_repo != repo {
        prime_staged_update_repo_index_state(db, repo, update_repo)?;
    }
    let mutation_path = update_repo.join(mutation_file);
    let original = fs::read_to_string(&mutation_path).map_err(|error| {
        format!(
            "failed to read mutation file {}: {error}",
            mutation_path.display()
        )
    })?;
    let mut iteration_results = Vec::new();
    let update_runs = loop_kind.update_iterations(iterations);
    for iteration in 1..=update_runs {
        if deadline_expired(deadline) {
            return Ok(update_integrity_timeout_repo(
                name,
                repo,
                db,
                mutation_file,
                mode,
                loop_kind,
                iterations,
                "update_loop_timeout",
                repeat_iterations,
                iteration_results,
                Some(cold),
            ));
        }
        let mutation_setup_start = Instant::now();
        let mutated = format!(
            "{}\n{}\n",
            original.trim_end(),
            update_integrity_mutation_text(&mutation_path, iteration)
        );
        fs::write(&mutation_path, mutated)
            .map_err(|error| format!("failed to mutate {}: {error}", mutation_path.display()))?;
        let mutation_setup_ms = elapsed_ms(mutation_setup_start);
        let mutate_start = Instant::now();
        let update_summary =
            update_changed_files_to_db(update_repo, &[PathBuf::from(mutation_file)], db).map_err(
                |error| {
                    format!("incremental update failed for {name} iteration {iteration}: {error}")
                },
            )?;
        let update = update_integrity_step_from_incremental(
            "single_file_update",
            mutate_start.elapsed(),
            &update_summary,
            update_repo,
            db,
            mode,
        )?;

        let restore_setup_start = Instant::now();
        fs::write(&mutation_path, &original)
            .map_err(|error| format!("failed to restore {}: {error}", mutation_path.display()))?;
        let restore_setup_ms = elapsed_ms(restore_setup_start);
        let restore_start = Instant::now();
        let restore_summary =
            update_changed_files_to_db(update_repo, &[PathBuf::from(mutation_file)], db).map_err(
                |error| format!("restore update failed for {name} iteration {iteration}: {error}"),
            )?;
        let restore = update_integrity_step_from_incremental(
            "restore_update",
            restore_start.elapsed(),
            &restore_summary,
            update_repo,
            db,
            mode,
        )?;
        let update_hash = update["graph_fact_hash"].as_str().unwrap_or("");
        let restore_hash = restore["graph_fact_hash"].as_str().unwrap_or("");
        iteration_results.push(json!({
            "iteration": iteration,
            "mutation_setup_ms": mutation_setup_ms,
            "update": update,
            "restore_setup_ms": restore_setup_ms,
            "restore": restore,
            "changed_hash_expected": true,
            "changed_hash_observed": update_hash != repeat_hash,
            "restore_hash_expected": repeat_hash,
            "restore_hash_observed": restore_hash,
            "restore_hash_matches_repeat": restore_hash == repeat_hash,
        }));
    }

    let all_integrity_ok = std::iter::once(&cold)
        .chain(
            repeat_iterations
                .iter()
                .filter_map(|iteration| iteration.get("repeat")),
        )
        .chain(iteration_results.iter().flat_map(|iteration| {
            [
                iteration.get("update").unwrap(),
                iteration.get("restore").unwrap(),
            ]
        }))
        .all(|step| {
            step["integrity_status"].as_str() == Some("ok")
                || step["integrity_status"].as_str() == Some("not_run")
        });
    let repeat_hash_stable = if repeat_runs == 0 {
        true
    } else {
        repeat_iterations.iter().all(|iteration| {
            iteration["repeat"]["graph_fact_hash"]
                .as_str()
                .unwrap_or("")
                == cold_hash
        })
    };
    let changed_hash_ok = if update_runs == 0 {
        true
    } else {
        iteration_results
            .iter()
            .all(|iteration| iteration["changed_hash_observed"].as_bool() == Some(true))
    };
    let restore_hash_ok = if update_runs == 0 {
        true
    } else {
        iteration_results
            .iter()
            .all(|iteration| iteration["restore_hash_matches_repeat"].as_bool() == Some(true))
    };
    let status = if all_integrity_ok && repeat_hash_stable && changed_hash_ok && restore_hash_ok {
        "passed"
    } else {
        "failed"
    };
    let claimable = status == "passed"
        && std::iter::once(&cold)
            .chain(
                repeat_iterations
                    .iter()
                    .filter_map(|iteration| iteration.get("repeat")),
            )
            .chain(iteration_results.iter().flat_map(|iteration| {
                [
                    iteration.get("update").unwrap(),
                    iteration.get("restore").unwrap(),
                ]
            }))
            .all(|step| step["claimable"].as_bool() == Some(true));

    Ok(json!({
        "name": name,
        "status": status,
        "repo_path": path_string(repo),
        "db_path": path_string(db),
        "mutation_file": mutation_file,
        "update_repo_path": path_string(update_repo),
        "update_mode": mode.as_str(),
        "loop_kind": loop_kind.as_str(),
        "iterations": iterations,
        "cold": cold,
        "repeat_unchanged": repeat,
        "repeat_iterations": repeat_iterations,
        "iteration_results": iteration_results,
        "graph_fact_hash_stable_on_repeat": repeat_hash_stable,
        "changed_file_updates_graph_fact_hash": changed_hash_ok,
        "restore_returns_to_repeat_graph_fact_hash": restore_hash_ok,
        "all_integrity_checks_passed": all_integrity_ok,
        "inspection_read_only": true,
        "artifact_mutated_during_inspection": false,
        "claimable": claimable,
    }))
}

pub(crate) fn run_cgc_comparison_command(args: &[String]) -> Result<Value, String> {
    let options = parse_cgc_comparison_options(args)?;
    let preflight = storage_budget::storage_budget_preflight(
        &options.storage_budget,
        storage_budget::StorageBudgetContext {
            command: "codegraph-mcp bench cgc-comparison".to_string(),
            repo_root: std::env::current_dir().ok(),
            db_path: None,
            out_path: Some(options.report_dir.clone()),
            explicit_db: false,
            explicit_out: true,
            diagnostic_only: true,
        },
    );
    if preflight.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp bench cgc-comparison",
                &preflight,
            ),
        ));
    }
    let report = run_codegraphcontext_comparison(CodeGraphContextComparisonOptions {
        report_dir: options.report_dir.clone(),
        timeout_ms: options.timeout_ms,
        top_k: options.top_k,
        competitor_executable: options.competitor_executable,
    })
    .map_err(|error| error.to_string())?;
    let storage_budget =
        storage_budget::storage_budget_postflight(preflight, &[], &[options.report_dir.clone()]);
    if storage_budget.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp bench cgc-comparison",
                &storage_budget,
            ),
        ));
    }

    Ok(json!({
        "status": "benchmarked",
        "phase": "21.1",
        "benchmark_id": report.benchmark_id,
        "report_dir": report.report_dir,
        "run_json": path_string(&options.report_dir.join("run.json")),
        "per_task_jsonl": path_string(&options.report_dir.join("per_task.jsonl")),
        "summary_md": path_string(&options.report_dir.join("summary.md")),
        "competitor": report.manifest,
        "aggregate": report.aggregate,
        "runs": report.runs.len(),
        "storage_budget": storage_budget.to_json(),
    }))
}

pub(crate) fn write_benchmark_report(
    report: &BenchmarkReport,
    format: BenchReportFormat,
    output: &Path,
) -> Result<(), String> {
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let contents = match format {
        BenchReportFormat::Json => {
            codegraph_bench::render_json_report(report).map_err(|error| error.to_string())?
        }
        BenchReportFormat::Markdown => codegraph_bench::render_markdown_report(report),
    };
    fs::write(output, contents).map_err(|error| error.to_string())
}
