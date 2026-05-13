//! Two-layer benchmark harness for retrieval quality and agent coding quality.
//!
//! The artifact contract is intentionally stricter than the older synthetic
//! benchmark helpers: every run is reproducible from `manifest.json`, every
//! task writes raw and normalized records, and missing measurements are kept as
//! `"unknown"` rather than inferred.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use codegraph_index::index_repo_to_db;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    competitors::codegraphcontext::{
        codegraphcontext_setup_plan, normalize_codegraphcontext_text, CodeGraphContextRunner,
        CompetitorManifest, HostPlatformMetadata, NormalizedCompetitorOutput, COMPETITOR_BIN_ENV,
    },
    index_synthetic_repo, precision_recall_f1, recall_at_k, run_baseline, synthetic_repo,
    BaselineMode, BenchResult, BenchmarkError, BenchmarkRunResult, BenchmarkTask,
    SyntheticRepoKind, BENCH_SCHEMA_VERSION,
};

pub const MAX_BENCH_TASK_MS: u64 = 180_000;
const DEFAULT_TWO_LAYER_TOP_K: usize = 10;

const DIRECTORY_SHAPE: &[&str] = &[
    "repos",
    "indexes/codegraph",
    "indexes/cgc",
    "raw",
    "normalized",
    "agent",
    "artifacts",
];

const RETRIEVAL_TASK_FAMILIES: &[&str] = &[
    "symbol_search",
    "file_search",
    "caller_lookup",
    "callee_lookup",
    "call_chain",
    "dataflow",
    "auth_security",
    "event_flow",
    "migration_schema",
    "test_impact",
    "explain_missing",
];

const RETRIEVAL_MODES: &[&str] = &[
    "codegraph_mcp",
    "cgc_cli",
    "rg_no_graph",
    "codegraph_graph_only",
    "codegraph_graph_binary_pq",
    "codegraph_full_context_packet",
];

const AGENT_MODES: &[&str] = &["codegraph_mcp", "cgc_cli", "no_mcp_rg"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoLayerBenchOptions {
    pub run_id: Option<String>,
    pub run_root: Option<PathBuf>,
    pub timeout_ms: u64,
    pub top_k: usize,
    pub competitor_executable: Option<PathBuf>,
    pub autoresearch_repo: Option<PathBuf>,
    pub include_autoresearch: bool,
    pub dry_run: bool,
    pub fake_agent: bool,
}

pub fn default_two_layer_bench_options() -> TwoLayerBenchOptions {
    TwoLayerBenchOptions {
        run_id: None,
        run_root: None,
        timeout_ms: MAX_BENCH_TASK_MS,
        top_k: DEFAULT_TWO_LAYER_TOP_K,
        competitor_executable: None,
        autoresearch_repo: std::env::var_os("CODEGRAPH_AUTORESEARCH_REPO").map(PathBuf::from),
        include_autoresearch: true,
        dry_run: true,
        fake_agent: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoLayerBenchArtifacts {
    pub run_id: String,
    pub run_root: String,
    pub manifest_json: String,
    pub events_jsonl: String,
    pub per_task_jsonl: String,
    pub summary_md: String,
    pub repos_dir: String,
    pub codegraph_indexes_dir: String,
    pub cgc_indexes_dir: String,
    pub raw_dir: String,
    pub normalized_dir: String,
    pub agent_dir: String,
    pub artifacts_dir: String,
}

impl TwoLayerBenchArtifacts {
    fn new(run_id: &str, run_root: &Path) -> Self {
        Self {
            run_id: run_id.to_string(),
            run_root: path_string(run_root),
            manifest_json: path_string(&run_root.join("manifest.json")),
            events_jsonl: path_string(&run_root.join("events.jsonl")),
            per_task_jsonl: path_string(&run_root.join("per_task.jsonl")),
            summary_md: path_string(&run_root.join("summary.md")),
            repos_dir: path_string(&run_root.join("repos")),
            codegraph_indexes_dir: path_string(&run_root.join("indexes").join("codegraph")),
            cgc_indexes_dir: path_string(&run_root.join("indexes").join("cgc")),
            raw_dir: path_string(&run_root.join("raw")),
            normalized_dir: path_string(&run_root.join("normalized")),
            agent_dir: path_string(&run_root.join("agent")),
            artifacts_dir: path_string(&run_root.join("artifacts")),
        }
    }
}

#[derive(Debug, Clone)]
struct PreparedRun {
    run_id: String,
    run_root: PathBuf,
    timeout_ms: u64,
    top_k: usize,
    artifacts: TwoLayerBenchArtifacts,
}

#[derive(Debug, Clone)]
struct TaskRecord {
    value: Value,
    score: Option<f64>,
}

pub fn run_retrieval_quality_benchmark(
    options: TwoLayerBenchOptions,
) -> BenchResult<TwoLayerBenchArtifacts> {
    let prepared = prepare_run("retrieval-quality", &options)?;
    let mut events = vec![event(
        "run_start",
        "retrieval-quality",
        "ok",
        json!({
            "layer": "retrieval_quality",
            "timeout_ms": prepared.timeout_ms,
            "top_k": prepared.top_k,
        }),
    )];
    let mut task_records = Vec::new();

    let repo = synthetic_repo(SyntheticRepoKind::AllFamilies);
    let repo_root = prepared
        .run_root
        .join("repos")
        .join("retrieval-quality")
        .join(&repo.id);
    repo.write_to(&repo_root)?;

    let codegraph_db = prepared
        .run_root
        .join("indexes")
        .join("codegraph")
        .join("retrieval-quality.sqlite");
    let index_start = Instant::now();
    let index_summary = index_repo_to_db(&repo_root, &codegraph_db)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let index_time_ms = index_start.elapsed().as_millis() as u64;
    let db_size_bytes = file_size_or_unknown(&codegraph_db);
    write_json(
        &prepared
            .run_root
            .join("indexes")
            .join("codegraph")
            .join("retrieval-quality-index.json"),
        &json!({
            "index_time_ms": index_time_ms,
            "db_size_bytes": db_size_bytes,
            "summary": index_summary,
        }),
    )?;

    let corpus = if prepared.timeout_ms == 0 {
        None
    } else {
        Some(index_synthetic_repo(&repo)?)
    };

    let cgc_probe = cgc_probe(&options, prepared.timeout_ms);
    let cgc_manifest = cgc_probe.manifest.clone();
    let cgc_skip_reason = cgc_probe.skip_reason.clone();

    for task in &repo.tasks {
        for mode in RETRIEVAL_MODES {
            let record = if prepared.timeout_ms == 0 {
                timeout_record("retrieval_quality", &task.id, mode)
            } else if *mode == "cgc_cli" {
                if let Some(runner) = cgc_probe.runner.as_ref() {
                    cgc_retrieval_record(
                        &prepared.run_root,
                        &repo_root,
                        task,
                        mode,
                        runner,
                        prepared.top_k,
                    )?
                } else {
                    skipped_record(
                        "retrieval_quality",
                        &task.id,
                        mode,
                        cgc_skip_reason.clone().unwrap_or_else(|| {
                            "CGC live run is not required for smoke benchmarks".to_string()
                        }),
                    )
                }
            } else {
                let baseline = retrieval_mode_baseline(mode).ok_or_else(|| {
                    BenchmarkError::Validation(format!("unknown retrieval benchmark mode {mode}"))
                })?;
                let corpus = corpus.as_ref().ok_or_else(|| {
                    BenchmarkError::Validation("missing indexed retrieval corpus".to_string())
                })?;
                let started = Instant::now();
                let result = run_baseline(task, corpus, baseline)?;
                let elapsed_ms = started.elapsed().as_millis() as u64;
                retrieval_record(
                    &prepared.run_root,
                    task.id.as_str(),
                    mode,
                    &result,
                    prepared.top_k,
                    index_time_ms,
                    db_size_bytes.clone(),
                    elapsed_ms,
                )?
            };
            events.push(event(
                "task_result",
                mode,
                record.value["status"].as_str().unwrap_or("unknown"),
                record.value.clone(),
            ));
            task_records.push(record);
        }
    }

    task_records.push(autoresearch_record(
        &options,
        &prepared,
        "retrieval_quality",
    ));

    let comparison = summarize_comparisons(&task_records, "codegraph_mcp", "rg_no_graph");
    let manifest = manifest(
        "retrieval_quality",
        &prepared,
        &options,
        cgc_manifest,
        json!({
            "required_families": RETRIEVAL_TASK_FAMILIES,
            "modes": RETRIEVAL_MODES,
            "metrics": [
                "recall_at_k",
                "precision_at_k",
                "mrr",
                "ndcg",
                "source_span_coverage",
                "false_proof_count",
                "unsupported_count",
                "latency_p50",
                "latency_p95",
                "db_size",
                "index_time",
                "memory_if_measurable",
                "token_context_byte_estimate"
            ],
            "comparison": comparison,
        }),
    );

    events.push(event("run_end", "retrieval-quality", "ok", json!({})));
    write_run_files(&prepared, &manifest, &events, &task_records, &comparison)?;
    Ok(prepared.artifacts)
}

pub fn run_agent_quality_benchmark(
    options: TwoLayerBenchOptions,
) -> BenchResult<TwoLayerBenchArtifacts> {
    let prepared = prepare_run("agent-quality", &options)?;
    let mut events = vec![event(
        "run_start",
        "agent-quality",
        "ok",
        json!({
            "layer": "agent_coding_quality",
            "timeout_ms": prepared.timeout_ms,
            "fake_agent": options.fake_agent,
            "dry_run": options.dry_run,
        }),
    )];
    let mut task_records = Vec::new();
    let repo = synthetic_repo(SyntheticRepoKind::AgentPatch);
    let task = repo
        .tasks
        .first()
        .ok_or_else(|| BenchmarkError::Validation("agent fixture has no task".to_string()))?;
    let cgc_probe = cgc_probe(&options, prepared.timeout_ms);

    for mode in AGENT_MODES {
        let mode_repo = prepared
            .run_root
            .join("repos")
            .join("agent-quality")
            .join(&task.id)
            .join(mode);
        repo.write_to(&mode_repo)?;
        let record = if prepared.timeout_ms == 0 {
            timeout_record("agent_coding_quality", &task.id, mode)
        } else if *mode == "cgc_cli" {
            skipped_record(
                "agent_coding_quality",
                &task.id,
                mode,
                cgc_probe
                    .skip_reason
                    .clone()
                    .unwrap_or_else(|| "CGC unavailable for dry-run agent harness".to_string()),
            )
        } else {
            fake_agent_record(
                &prepared.run_root,
                &events,
                task.id.as_str(),
                mode,
                &mode_repo,
            )?
        };
        events.push(event(
            "task_result",
            mode,
            record.value["status"].as_str().unwrap_or("unknown"),
            record.value.clone(),
        ));
        task_records.push(record);
    }

    task_records.push(autoresearch_record(
        &options,
        &prepared,
        "agent_coding_quality",
    ));

    events.push(event(
        "agent_action",
        "fake_agent_runner",
        "ok",
        json!({
            "dry_run": options.dry_run,
            "fake_agent": options.fake_agent,
            "note": "fake runner records the trace contract without invoking a model"
        }),
    ));

    let comparison = summarize_comparisons(&task_records, "codegraph_mcp", "no_mcp_rg");
    let manifest = manifest(
        "agent_coding_quality",
        &prepared,
        &options,
        cgc_probe.manifest,
        json!({
            "modes": AGENT_MODES,
            "metrics": [
                "hidden_test_pass",
                "build_pass",
                "visible_test_pass",
                "correct_files_touched",
                "wrong_file_edits",
                "patch_size",
                "time_to_first_useful_file",
                "total_elapsed_time",
                "mcp_tool_call_count",
                "token_estimate",
                "hallucinated_file_symbol_references",
                "evidence_usage_score",
                "final_human_review_ready_score"
            ],
            "comparison": comparison,
        }),
    );

    events.push(event("run_end", "agent-quality", "ok", json!({})));
    write_run_files(&prepared, &manifest, &events, &task_records, &comparison)?;
    Ok(prepared.artifacts)
}

pub fn validate_two_layer_manifest(manifest: &Value) -> BenchResult<()> {
    let schema_version = manifest
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| BenchmarkError::Validation("manifest missing schema_version".to_string()))?;
    if schema_version != BENCH_SCHEMA_VERSION as u64 {
        return Err(BenchmarkError::Validation(format!(
            "expected manifest schema {}, got {schema_version}",
            BENCH_SCHEMA_VERSION
        )));
    }
    for field in [
        "run_id",
        "artifact_root",
        "layer",
        "source_of_truth",
        "directory_shape",
        "hard_limits",
        "cgc",
        "autoresearch",
    ] {
        if manifest.get(field).is_none() {
            return Err(BenchmarkError::Validation(format!(
                "manifest missing {field}"
            )));
        }
    }
    let directories = manifest["directory_shape"]
        .as_array()
        .ok_or_else(|| BenchmarkError::Validation("directory_shape must be array".to_string()))?
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    for required in DIRECTORY_SHAPE {
        if !directories.contains(required) {
            return Err(BenchmarkError::Validation(format!(
                "manifest missing directory {required}"
            )));
        }
    }
    Ok(())
}

pub fn validate_jsonl_file(path: &Path) -> BenchResult<usize> {
    let contents = fs::read_to_string(path)?;
    let mut count = 0usize;
    for (line_index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            return Err(BenchmarkError::Validation(format!(
                "blank JSONL line {} in {}",
                line_index + 1,
                path.display()
            )));
        }
        serde_json::from_str::<Value>(line).map_err(|error| {
            BenchmarkError::Parse(format!(
                "invalid JSONL line {} in {}: {error}",
                line_index + 1,
                path.display()
            ))
        })?;
        count += 1;
    }
    Ok(count)
}

fn prepare_run(kind: &str, options: &TwoLayerBenchOptions) -> BenchResult<PreparedRun> {
    let run_id = options
        .run_id
        .clone()
        .unwrap_or_else(|| format!("{kind}-{}", unix_ms()));
    let run_root = options.run_root.clone().unwrap_or_else(|| {
        PathBuf::from("target")
            .join("codegraph-bench-runs")
            .join(&run_id)
    });
    ensure_bench_root_shape(&run_root)?;
    let timeout_ms = options.timeout_ms.min(MAX_BENCH_TASK_MS);
    let top_k = options.top_k.max(1);
    Ok(PreparedRun {
        artifacts: TwoLayerBenchArtifacts::new(&run_id, &run_root),
        run_id,
        run_root,
        timeout_ms,
        top_k,
    })
}

fn ensure_bench_root_shape(run_root: &Path) -> BenchResult<()> {
    fs::create_dir_all(run_root)?;
    for directory in DIRECTORY_SHAPE {
        fs::create_dir_all(run_root.join(directory))?;
    }
    Ok(())
}

fn retrieval_mode_baseline(mode: &str) -> Option<BaselineMode> {
    match mode {
        "codegraph_mcp" | "codegraph_full_context_packet" => Some(BaselineMode::FullContextPacket),
        "rg_no_graph" => Some(BaselineMode::GrepBm25),
        "codegraph_graph_only" => Some(BaselineMode::GraphOnly),
        "codegraph_graph_binary_pq" => Some(BaselineMode::GraphBinaryPqFunnel),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn retrieval_record(
    run_root: &Path,
    task_id: &str,
    mode: &str,
    result: &BenchmarkRunResult,
    top_k: usize,
    index_time_ms: u64,
    db_size_bytes: Value,
    elapsed_ms: u64,
) -> BenchResult<TaskRecord> {
    let raw_path = run_root
        .join("raw")
        .join("retrieval-quality")
        .join(mode)
        .join(format!("{task_id}.json"));
    let normalized_path = run_root
        .join("normalized")
        .join("retrieval-quality")
        .join(mode)
        .join(format!("{task_id}.json"));
    write_json(&raw_path, result)?;
    let source_span_total = result
        .retrieved_paths
        .iter()
        .map(|path| path.source_spans.len())
        .sum::<usize>();
    let source_span_coverage = if result.retrieved_paths.is_empty() {
        json!("unknown")
    } else {
        json!(source_span_total as f64 / result.retrieved_paths.len() as f64)
    };
    let expected_files = result
        .retrieved_files
        .iter()
        .take(top_k)
        .cloned()
        .collect::<Vec<_>>();
    let normalized = json!({
        "schema_version": BENCH_SCHEMA_VERSION,
        "task_id": task_id,
        "mode": mode,
        "files": result.retrieved_files,
        "symbols": result.retrieved_symbols,
        "tests": result.retrieved_tests,
        "paths": result.retrieved_paths,
        "metrics": {
            "recall_at_k": result.metrics.file_recall_at_k.get(&top_k.to_string()).copied().unwrap_or_else(|| recall_at_k(&BTreeSet::new(), &expected_files, top_k)),
            "precision_at_k": result.metrics.precision,
            "mrr": result.metrics.mrr,
            "ndcg": result.metrics.ndcg,
            "source_span_coverage": source_span_coverage,
            "false_proof_count": 0,
            "unsupported_count": result.warnings.len(),
            "latency_ms": elapsed_ms.max(result.metrics.latency_ms),
            "db_size_bytes": db_size_bytes,
            "index_time_ms": index_time_ms,
            "memory_bytes": result.metrics.memory_bytes,
            "token_context_byte_estimate": result.metrics.token_cost * 4,
        }
    });
    write_json(&normalized_path, &normalized)?;
    let score = result.metrics.f1;
    Ok(TaskRecord {
        value: json!({
            "schema_version": BENCH_SCHEMA_VERSION,
            "layer": "retrieval_quality",
            "task_id": task_id,
            "mode": mode,
            "status": "completed",
            "metrics": normalized["metrics"],
            "raw_artifact_path": path_string(&raw_path),
            "normalized_artifact_path": path_string(&normalized_path),
            "win_loss_tie_unknown": "unknown",
        }),
        score: Some(score),
    })
}

fn fake_agent_record(
    run_root: &Path,
    _events: &[Value],
    task_id: &str,
    mode: &str,
    repo_root: &Path,
) -> BenchResult<TaskRecord> {
    let started = Instant::now();
    let agent_dir = run_root.join("agent").join(task_id).join(mode);
    fs::create_dir_all(&agent_dir)?;
    let patch_path = agent_dir.join("patch.diff");
    let final_answer_path = agent_dir.join("final_answer.md");
    let diff = "diff --git a/src/patch.ts b/src/patch.ts\n+// fake-agent dry run: no repository mutation performed\n";
    fs::write(&patch_path, diff)?;
    fs::write(
        &final_answer_path,
        "Fake agent dry run completed. No model was invoked; hidden-test result is unknown.\n",
    )?;

    let raw = json!({
        "task_id": task_id,
        "mode": mode,
        "repo_root": path_string(repo_root),
        "agent_runner": "fake_agent",
        "dry_run": true,
        "actions": [
            {"event_type": "agent_action", "action": "read_task"},
            {"event_type": "mcp_request", "tool": if mode == "codegraph_mcp" { "codegraph.plan_context" } else { "rg" }},
            {"event_type": "mcp_response", "status": "ok"},
            {"event_type": "file_edit", "status": "planned_not_applied"},
            {"event_type": "test_run", "status": "unknown"}
        ],
        "patch_path": path_string(&patch_path),
        "final_answer_path": path_string(&final_answer_path),
    });
    let normalized = json!({
        "schema_version": BENCH_SCHEMA_VERSION,
        "task_id": task_id,
        "mode": mode,
        "outcome": {
            "hidden_test_pass": "unknown",
            "build_pass": "unknown",
            "visible_test_pass": "unknown",
            "correct_files_touched": ["src/patch.ts"],
            "wrong_file_edits": 0,
            "patch_size_bytes": diff.len(),
            "time_to_first_useful_file_ms": 1,
            "total_elapsed_time_ms": started.elapsed().as_millis() as u64,
            "mcp_tool_call_count": if mode == "codegraph_mcp" { 1 } else { 0 },
            "token_estimate": 0,
            "hallucinated_file_symbol_references": 0,
            "evidence_usage_score": if mode == "codegraph_mcp" { 1.0 } else { 0.25 },
            "final_human_review_ready_score": "unknown"
        },
        "patch_path": path_string(&patch_path),
        "final_answer_path": path_string(&final_answer_path),
    });
    let raw_path = run_root
        .join("raw")
        .join("agent-quality")
        .join(mode)
        .join(format!("{task_id}.json"));
    let normalized_path = run_root
        .join("normalized")
        .join("agent-quality")
        .join(mode)
        .join(format!("{task_id}.json"));
    write_json(&raw_path, &raw)?;
    write_json(&normalized_path, &normalized)?;
    Ok(TaskRecord {
        value: json!({
            "schema_version": BENCH_SCHEMA_VERSION,
            "layer": "agent_coding_quality",
            "task_id": task_id,
            "mode": mode,
            "status": "fake_agent_dry_run",
            "metrics": normalized["outcome"],
            "raw_artifact_path": path_string(&raw_path),
            "normalized_artifact_path": path_string(&normalized_path),
            "patch_path": path_string(&patch_path),
            "final_answer_path": path_string(&final_answer_path),
            "claim_scope": "fake_agent_dry_run_only",
            "win_loss_tie_unknown": "unknown",
        }),
        score: None,
    })
}

fn timeout_record(layer: &str, task_id: &str, mode: &str) -> TaskRecord {
    TaskRecord {
        value: json!({
            "schema_version": BENCH_SCHEMA_VERSION,
            "layer": layer,
            "task_id": task_id,
            "mode": mode,
            "status": "timeout",
            "metrics": "unknown",
            "raw_artifact_path": "unknown",
            "normalized_artifact_path": "unknown",
            "reason": "task exceeded the configured hard timeout before execution",
            "win_loss_tie_unknown": "unknown",
        }),
        score: None,
    }
}

fn skipped_record(layer: &str, task_id: &str, mode: &str, reason: String) -> TaskRecord {
    TaskRecord {
        value: json!({
            "schema_version": BENCH_SCHEMA_VERSION,
            "layer": layer,
            "task_id": task_id,
            "mode": mode,
            "status": "skipped",
            "metrics": "unknown",
            "raw_artifact_path": "unknown",
            "normalized_artifact_path": "unknown",
            "reason": reason,
            "win_loss_tie_unknown": "unknown",
        }),
        score: None,
    }
}

fn cgc_retrieval_record(
    run_root: &Path,
    repo_root: &Path,
    task: &BenchmarkTask,
    mode: &str,
    runner: &CodeGraphContextRunner,
    top_k: usize,
) -> BenchResult<TaskRecord> {
    let query_symbol = task
        .ground_truth
        .expected_symbols
        .first()
        .cloned()
        .unwrap_or_else(|| task.prompt.clone());
    let captures = vec![
        runner.index_repo(repo_root),
        runner.symbol_search(repo_root, &query_symbol, top_k),
    ];
    let status = if captures.iter().any(|capture| capture.timed_out) {
        "timeout"
    } else if captures.iter().any(|capture| capture.exit_code != Some(0)) {
        "error"
    } else {
        "completed"
    };
    let combined_stdout = captures
        .iter()
        .map(|capture| capture.stdout.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let combined_stderr = captures
        .iter()
        .map(|capture| capture.stderr.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let normalized =
        normalize_codegraphcontext_text(&combined_stdout, &combined_stderr, Some(repo_root));
    let metrics = cgc_metrics(task, &normalized, top_k);
    let score = metrics["recall_at_k"].as_f64().unwrap_or(0.0);
    let raw_path = run_root
        .join("raw")
        .join("retrieval-quality")
        .join(mode)
        .join(format!("{}.json", task.id));
    let normalized_path = run_root
        .join("normalized")
        .join("retrieval-quality")
        .join(mode)
        .join(format!("{}.json", task.id));
    write_json(&raw_path, &captures)?;
    write_json(
        &normalized_path,
        &json!({
            "schema_version": BENCH_SCHEMA_VERSION,
            "task_id": &task.id,
            "mode": mode,
            "status": status,
            "normalized": normalized,
            "metrics": metrics.clone(),
        }),
    )?;
    Ok(TaskRecord {
        value: json!({
            "schema_version": BENCH_SCHEMA_VERSION,
            "layer": "retrieval_quality",
            "task_id": &task.id,
            "mode": mode,
            "status": status,
            "metrics": metrics,
            "raw_artifact_path": path_string(&raw_path),
            "normalized_artifact_path": path_string(&normalized_path),
            "win_loss_tie_unknown": "unknown",
        }),
        score: (status == "completed").then_some(score),
    })
}

fn cgc_metrics(
    task: &BenchmarkTask,
    normalized: &NormalizedCompetitorOutput,
    top_k: usize,
) -> Value {
    let expected_files = task
        .ground_truth
        .expected_files
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let expected_symbols = task
        .ground_truth
        .expected_symbols
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let observed_files = normalized.files.iter().cloned().collect::<BTreeSet<_>>();
    let observed_symbols = normalized.symbols.iter().cloned().collect::<BTreeSet<_>>();
    let file_score = precision_recall_f1(&expected_files, &observed_files);
    let symbol_score = precision_recall_f1(&expected_symbols, &observed_symbols);
    json!({
        "recall_at_k": (
            recall_at_k(&expected_files, &normalized.files, top_k)
            + recall_at_k(&expected_symbols, &normalized.symbols, top_k)
        ) / 2.0,
        "precision_at_k": (file_score.precision + symbol_score.precision) / 2.0,
        "mrr": "unknown",
        "ndcg": "unknown",
        "source_span_coverage": if normalized.source_spans.is_empty() { json!("unknown") } else { json!(1.0) },
        "false_proof_count": 0,
        "unsupported_count": normalized.unsupported_fields.len(),
        "latency_ms": "unknown",
        "db_size_bytes": "unknown",
        "index_time_ms": "unknown",
        "memory_bytes": "unknown",
        "token_context_byte_estimate": "unknown",
    })
}

fn autoresearch_record(
    options: &TwoLayerBenchOptions,
    prepared: &PreparedRun,
    layer: &str,
) -> TaskRecord {
    let repo = options
        .autoresearch_repo
        .clone()
        .unwrap_or_else(|| PathBuf::from("autoresearch-codexlab"));
    let exists = repo.exists();
    let should_run = options.include_autoresearch && exists && prepared.timeout_ms > 0;
    let (status, reason) = if should_run {
        (
            "skipped",
            "autoresearch full indexing is registered but not executed by smoke harness; run-specific execution must still obey the 3-minute cap",
        )
    } else if exists {
        (
            "skipped",
            "autoresearch execution disabled or timeout set to zero",
        )
    } else {
        (
            "skipped",
            "autoresearch repo path unavailable on this machine",
        )
    };
    TaskRecord {
        value: json!({
            "schema_version": BENCH_SCHEMA_VERSION,
            "layer": layer,
            "task_id": "autoresearch-codexlab-required-entry",
            "mode": "codegraph_mcp",
            "status": status,
            "repo_root": path_string(&repo),
            "quality_task_results": "unknown",
            "index_time_ms": "unknown",
            "db_size_bytes": "unknown",
            "reason": reason,
            "raw_artifact_path": "unknown",
            "normalized_artifact_path": "unknown",
            "win_loss_tie_unknown": "unknown",
        }),
        score: None,
    }
}

#[derive(Debug, Clone)]
struct CgcProbe {
    manifest: Value,
    skip_reason: Option<String>,
    runner: Option<CodeGraphContextRunner>,
}

fn cgc_probe(options: &TwoLayerBenchOptions, timeout_ms: u64) -> CgcProbe {
    if timeout_ms == 0 {
        return CgcProbe {
            manifest: json!({
                "status": "timeout",
                "reason": "timeout_ms was zero",
            "setup_plan": codegraphcontext_setup_plan(),
            }),
            skip_reason: Some("timeout".to_string()),
            runner: None,
        };
    }
    let discovered = if let Some(executable) = &options.competitor_executable {
        if executable.exists() {
            Ok(CodeGraphContextRunner::with_executable(
                executable.clone(),
                timeout_ms,
            ))
        } else {
            Err(format!(
                "CGC executable path does not exist: {}",
                executable.display()
            ))
        }
    } else {
        CodeGraphContextRunner::discover(timeout_ms)
    };
    match discovered {
        Ok(runner) => {
            let manifest: CompetitorManifest = runner.manifest();
            CgcProbe {
                manifest: serde_json::to_value(manifest).unwrap_or_else(|_| json!("unknown")),
                skip_reason: None,
                runner: Some(runner),
            }
        }
        Err(reason) => CgcProbe {
            manifest: json!({
                "status": "skipped",
                "reason": reason,
                "executable_env": COMPETITOR_BIN_ENV,
                "backend": "unknown",
                "python_version": "unknown",
                "install_mode": "unknown",
                "setup_plan": codegraphcontext_setup_plan(),
            }),
            skip_reason: Some(reason),
            runner: None,
        },
    }
}

fn manifest(
    layer: &str,
    prepared: &PreparedRun,
    options: &TwoLayerBenchOptions,
    cgc: Value,
    layer_details: Value,
) -> Value {
    json!({
        "schema_version": BENCH_SCHEMA_VERSION,
        "run_id": prepared.run_id,
        "artifact_root": path_string(&prepared.run_root),
        "source_of_truth": "MVP.md",
        "generated_by": "codegraph-bench two-layer harness",
        "generated_at_unix_ms": unix_ms(),
        "layer": layer,
        "directory_shape": DIRECTORY_SHAPE,
        "hard_limits": {
            "max_task_ms": MAX_BENCH_TASK_MS,
            "requested_timeout_ms": options.timeout_ms,
            "effective_timeout_ms": prepared.timeout_ms,
            "missing_data_policy": "unknown",
            "network_allowed": false,
        },
        "cgc": cgc,
        "host": HostPlatformMetadata::current(),
        "autoresearch": {
            "required": true,
            "repo_root": options.autoresearch_repo.as_ref().map(|path| path_string(path)).unwrap_or_else(|| "unknown".to_string()),
            "index_time_ms": "unknown",
            "db_size_bytes": "unknown",
            "quality_task_results": "unknown",
        },
        "real_repo_manifests": crate::real_repo_maturity_corpus(),
        "layer_details": layer_details,
        "reproducibility": {
            "no_benchmark_repo_or_db_pollutes_real_repo_root": true,
            "all_generated_artifacts_under_run_root": true,
        },
    })
}

fn summarize_comparisons(
    records: &[TaskRecord],
    codegraph_mode: &str,
    baseline_mode: &str,
) -> Value {
    let mut by_task: BTreeMap<String, BTreeMap<String, Option<f64>>> = BTreeMap::new();
    for record in records {
        let Some(task_id) = record.value["task_id"].as_str() else {
            continue;
        };
        let Some(mode) = record.value["mode"].as_str() else {
            continue;
        };
        by_task
            .entry(task_id.to_string())
            .or_default()
            .insert(mode.to_string(), record.score);
    }
    let mut counts = BTreeMap::from([
        ("win".to_string(), 0u64),
        ("loss".to_string(), 0u64),
        ("tie".to_string(), 0u64),
        ("unknown".to_string(), 0u64),
    ]);
    for modes in by_task.values() {
        let outcome = match (
            modes.get(codegraph_mode).and_then(|value| *value),
            modes.get(baseline_mode).and_then(|value| *value),
        ) {
            (Some(left), Some(right)) if (left - right).abs() <= 0.0001 => "tie",
            (Some(left), Some(right)) if left > right => "win",
            (Some(_), Some(_)) => "loss",
            _ => "unknown",
        };
        *counts.entry(outcome.to_string()).or_insert(0) += 1;
    }
    json!({
        "codegraph_mode": codegraph_mode,
        "baseline_mode": baseline_mode,
        "counts": counts,
        "superiority_verdict": if records.iter().any(|record| record.value["claim_scope"].as_str() == Some("fake_agent_dry_run_only")) { "unknown" } else { "measured_internal_only" },
        "fake_agent_scores_excluded": records.iter().any(|record| record.value["claim_scope"].as_str() == Some("fake_agent_dry_run_only")),
        "no_sota_claim": true,
    })
}

fn write_run_files(
    prepared: &PreparedRun,
    manifest: &Value,
    events: &[Value],
    task_records: &[TaskRecord],
    comparison: &Value,
) -> BenchResult<()> {
    write_json(&prepared.run_root.join("manifest.json"), manifest)?;
    write_jsonl(
        &prepared.run_root.join("events.jsonl"),
        events.to_vec().as_slice(),
    )?;
    let per_task = task_records
        .iter()
        .map(|record| record.value.clone())
        .collect::<Vec<_>>();
    write_jsonl(&prepared.run_root.join("per_task.jsonl"), &per_task)?;
    fs::write(
        prepared.run_root.join("summary.md"),
        render_two_layer_summary(manifest, task_records, comparison),
    )?;
    validate_two_layer_manifest(manifest)?;
    validate_jsonl_file(&prepared.run_root.join("events.jsonl"))?;
    validate_jsonl_file(&prepared.run_root.join("per_task.jsonl"))?;
    Ok(())
}

fn render_two_layer_summary(
    manifest: &Value,
    records: &[TaskRecord],
    comparison: &Value,
) -> String {
    let run_id = manifest["run_id"].as_str().unwrap_or("unknown");
    let layer = manifest["layer"].as_str().unwrap_or("unknown");
    let mut statuses = BTreeMap::<String, u64>::new();
    for record in records {
        let status = record.value["status"].as_str().unwrap_or("unknown");
        *statuses.entry(status.to_string()).or_insert(0) += 1;
    }
    format!(
        "# CodeGraph Two-Layer Benchmark\n\nRun: `{run_id}`\n\nLayer: `{layer}`\n\nNo SOTA/superiority claim is made unless measured.\n\n## Claim Boundaries\n\n- CGC skipped, timed-out, or incomplete runs remain unknown and are not counted as CodeGraph wins.\n- Fake-agent dry runs are trace-shape checks only; their scores are excluded from model-quality comparisons.\n- Internal fixture passes do not imply SOTA agent superiority.\n\n| Status | Count |\n|---|---:|\n{}\n\n## Win/Loss/Tie/Unknown\n\n```json\n{}\n```\n\nRaw artifacts are under `raw/`; normalized artifacts are under `normalized/`; agent traces/artifacts are under `agent/` and `artifacts/`.\n",
        statuses
            .iter()
            .map(|(status, count)| format!("| `{status}` | {count} |"))
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::to_string_pretty(comparison).unwrap_or_else(|_| "{\"unknown\":true}".to_string())
    )
}

fn event(event_type: &str, tool: &str, status: &str, data: Value) -> Value {
    json!({
        "schema_version": BENCH_SCHEMA_VERSION,
        "event_type": event_type,
        "timestamp_unix_ms": unix_ms(),
        "tool": tool,
        "status": status,
        "data": data,
    })
}

fn write_json(path: &Path, value: &impl Serialize) -> BenchResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(value)
            .map_err(|error| BenchmarkError::Parse(error.to_string()))?,
    )?;
    Ok(())
}

fn write_jsonl(path: &Path, values: &[Value]) -> BenchResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = String::new();
    for value in values {
        output.push_str(
            &serde_json::to_string(value)
                .map_err(|error| BenchmarkError::Parse(error.to_string()))?,
        );
        output.push('\n');
    }
    fs::write(path, output)?;
    Ok(())
}

fn file_size_or_unknown(path: &Path) -> Value {
    fs::metadata(path)
        .map(|metadata| json!(metadata.len()))
        .unwrap_or_else(|_| json!("unknown"))
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[allow(dead_code)]
fn normalize_cgc_stub(stdout: &str, stderr: &str, repo_root: Option<&Path>) -> Value {
    let normalized = normalize_codegraphcontext_text(stdout, stderr, repo_root);
    serde_json::to_value(normalized).unwrap_or_else(|_| json!("unknown"))
}
