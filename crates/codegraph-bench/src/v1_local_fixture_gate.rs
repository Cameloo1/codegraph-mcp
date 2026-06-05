//! Benchmark Layer v1 deterministic local fixture gate.
//!
//! This gate runs the same-agent A/B plan scaffold across the local v1 patch
//! fixtures before any external SWE-bench-style patch work. It aggregates the
//! existing harness artifacts instead of creating a second execution path.

use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    default_v1_same_agent_ab_harness_options, load_v1_patch_task_registry,
    run_v1_same_agent_ab_harness, v1_registry::v1_registry_path, BenchResult, BenchmarkError,
    V1AgentMode, V1CodeGraphContextScenario, V1PatchTask, V1SameAgentAbHarnessArtifacts,
    CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV,
};

pub const V1_LOCAL_FIXTURE_GATE_SCHEMA_VERSION: u32 = 1;
pub const V1_LOCAL_FIXTURE_GATE_NAME: &str = "benchmark_v1_local_fixture_gate";
pub const REQUIRED_LOCAL_FIXTURE_TASK_COUNT: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V1LocalFixtureGateOptions {
    pub run_id: Option<String>,
    pub run_root: Option<PathBuf>,
    pub registry_path: Option<PathBuf>,
    pub include_mock_mode: bool,
    pub include_real_patch_modes_when_external_ready: bool,
}

pub fn default_v1_local_fixture_gate_options() -> V1LocalFixtureGateOptions {
    V1LocalFixtureGateOptions {
        run_id: None,
        run_root: None,
        registry_path: None,
        include_mock_mode: true,
        include_real_patch_modes_when_external_ready: false,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V1LocalFixtureGateReport {
    pub schema_version: u32,
    pub gate: String,
    pub run_id: String,
    pub run_root: String,
    pub local_fixture_tasks_run: Vec<String>,
    pub task_results: Vec<V1LocalFixtureTaskGateResult>,
    pub same_agent_ab_invariant_passed: bool,
    pub plan_quality_scored: bool,
    pub proof_discipline_scored: bool,
    pub hallucination_traps_scored: bool,
    pub patch_modes_status: String,
    pub mock_mode_scaffold_only: bool,
    pub gold_leakage_violations: u64,
    pub claimability_violations: u64,
    pub unsupported_claim_violations: u64,
    pub graph_proof_overclaim_count: u64,
    pub normal_dot_codegraph_mutated: bool,
    pub public_claim: bool,
    pub real_agent_patch_quality_claim: bool,
    pub expected_evidence: V1ExpectedEvidenceSummary,
    pub artifact_manifest_json: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V1LocalFixtureTaskGateResult {
    pub task_id: String,
    pub task_family: String,
    pub run_root: String,
    pub artifact_manifest_json: String,
    pub invariant_passed: bool,
    pub a_arm_normal_tools: bool,
    pub b_arm_normal_tools_plus_codegraph: bool,
    pub hidden_gold_absent_from_prompts: bool,
    pub mock_mode_scaffold_only: bool,
    pub patch_modes_status: String,
    pub a_arm_metrics: V1LocalFixtureArmMetrics,
    pub b_arm_metrics: V1LocalFixtureArmMetrics,
    pub b_improvement: V1LocalFixtureBImprovement,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V1LocalFixtureArmMetrics {
    pub mode: String,
    pub status: String,
    pub plan_accuracy_score: f64,
    pub proof_discipline_score: f64,
    pub hallucination_trap_score: f64,
    pub evidence_alignment_score: f64,
    pub wrong_file_edits: u64,
    pub nonexistent_symbol_refs: u64,
    pub unsupported_claims: u64,
    pub codegraph_context_available: bool,
    pub codegraph_attribution_status: String,
    pub context_bytes: u64,
    pub codegraph_context_bytes: u64,
    pub tool_calls: u64,
    pub wall_ms: u64,
    pub graph_proof_overclaim_count: u64,
    pub proof_discipline_violation_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1LocalFixtureBImprovement {
    pub plan_quality_improved: bool,
    pub hallucination_risk_reduced: bool,
    pub proof_discipline_improved: bool,
    pub evidence_alignment_improved: bool,
    pub proof_boundary_explicitness_improved: bool,
    pub classification: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1ExpectedEvidenceSummary {
    pub b_arm_improvement_found: bool,
    pub classification: String,
    pub task_ids: Vec<String>,
}

pub fn run_v1_local_fixture_gate(
    options: V1LocalFixtureGateOptions,
) -> BenchResult<V1LocalFixtureGateReport> {
    let workspace_root = workspace_root();
    let dot_codegraph_before = dot_codegraph_state(&workspace_root);
    let registry_path = options
        .registry_path
        .clone()
        .unwrap_or_else(|| v1_registry_path(&workspace_root));
    let registry = load_v1_patch_task_registry(&registry_path)?;
    let local_tasks = registry
        .tasks
        .iter()
        .filter(|task| task.repo_source == "local_fixture" && task.status == "ready")
        .cloned()
        .collect::<Vec<_>>();
    if local_tasks.len() != REQUIRED_LOCAL_FIXTURE_TASK_COUNT {
        return Err(BenchmarkError::Validation(format!(
            "expected {REQUIRED_LOCAL_FIXTURE_TASK_COUNT} local v1 fixtures, got {}",
            local_tasks.len()
        )));
    }

    let run_id = options
        .run_id
        .clone()
        .unwrap_or_else(|| format!("v1-local-fixture-gate-{}", unix_ms()));
    let run_root = options.run_root.clone().unwrap_or_else(|| {
        PathBuf::from("target")
            .join("codegraph-bench-runs")
            .join(&run_id)
    });
    fs::create_dir_all(&run_root)?;

    let external_agent_configured = env::var(CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .is_some();
    let include_real_patch =
        options.include_real_patch_modes_when_external_ready && external_agent_configured;

    let mut task_results = Vec::new();
    for task in &local_tasks {
        let mut modes = vec![
            V1AgentMode::RgOnlyAgentPlan,
            V1AgentMode::RgPlusCodegraphAgentPlan,
        ];
        if options.include_mock_mode {
            modes.push(V1AgentMode::MockAgentScaffoldOnly);
        }
        if include_real_patch {
            modes.extend([
                V1AgentMode::RgOnlyAgentPatch,
                V1AgentMode::RgPlusCodegraphAgentPatch,
            ]);
        }

        let mut harness_options = default_v1_same_agent_ab_harness_options();
        harness_options.registry_path = Some(registry_path.clone());
        harness_options.task_id = Some(task.task_id.clone());
        harness_options.modes = modes;
        harness_options.run_id = Some(format!("{}-{}", run_id, task.task_id));
        harness_options.run_root = Some(run_root.join(&task.task_id));
        harness_options.external_agent_command =
            env::var(CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV).ok();
        harness_options.allow_real_patch_execution = include_real_patch;
        harness_options.codegraph_context_scenario = scenario_for_task(task);

        let artifacts = run_v1_same_agent_ab_harness(harness_options)?;
        task_results.push(task_gate_result(
            task,
            &artifacts,
            external_agent_configured,
        )?);
    }

    let dot_codegraph_after = dot_codegraph_state(&workspace_root);
    let normal_dot_codegraph_mutated = dot_codegraph_before != dot_codegraph_after
        || task_results.iter().any(|result| {
            read_manifest_bool(
                &result.artifact_manifest_json,
                "normal_dot_codegraph_mutated",
            )
        });
    let patch_modes_status = aggregate_patch_modes_status(&task_results, external_agent_configured);
    let expected_evidence = expected_evidence_summary(&task_results);
    let artifact_manifest_json = path_string(&run_root.join("artifact_manifest.json"));
    let report = V1LocalFixtureGateReport {
        schema_version: V1_LOCAL_FIXTURE_GATE_SCHEMA_VERSION,
        gate: V1_LOCAL_FIXTURE_GATE_NAME.to_string(),
        run_id,
        run_root: path_string(&run_root),
        local_fixture_tasks_run: local_tasks
            .iter()
            .map(|task| task.task_id.clone())
            .collect(),
        task_results,
        same_agent_ab_invariant_passed: true,
        plan_quality_scored: true,
        proof_discipline_scored: true,
        hallucination_traps_scored: true,
        patch_modes_status,
        mock_mode_scaffold_only: true,
        gold_leakage_violations: 0,
        claimability_violations: 0,
        unsupported_claim_violations: 0,
        graph_proof_overclaim_count: 0,
        normal_dot_codegraph_mutated,
        public_claim: false,
        real_agent_patch_quality_claim: false,
        expected_evidence,
        artifact_manifest_json,
    };
    let report = recompute_gate_totals(report)?;
    write_json(
        &run_root.join("artifact_manifest.json"),
        &gate_artifact_manifest(&report),
    )?;
    Ok(report)
}

pub fn validate_v1_local_fixture_gate_report(report: &V1LocalFixtureGateReport) -> BenchResult<()> {
    if report.schema_version != V1_LOCAL_FIXTURE_GATE_SCHEMA_VERSION {
        return Err(BenchmarkError::Validation(format!(
            "expected local fixture gate schema {}, got {}",
            V1_LOCAL_FIXTURE_GATE_SCHEMA_VERSION, report.schema_version
        )));
    }
    if report.local_fixture_tasks_run.len() != REQUIRED_LOCAL_FIXTURE_TASK_COUNT {
        return Err(BenchmarkError::Validation(
            "local fixture gate did not run all required local tasks".to_string(),
        ));
    }
    for (field, passed) in [
        (
            "same_agent_ab_invariant_passed",
            report.same_agent_ab_invariant_passed,
        ),
        ("plan_quality_scored", report.plan_quality_scored),
        ("proof_discipline_scored", report.proof_discipline_scored),
        (
            "hallucination_traps_scored",
            report.hallucination_traps_scored,
        ),
        ("mock_mode_scaffold_only", report.mock_mode_scaffold_only),
        (
            "gold_leakage_violations_zero",
            report.gold_leakage_violations == 0,
        ),
        (
            "claimability_violations_zero",
            report.claimability_violations == 0,
        ),
        (
            "unsupported_claim_violations_zero",
            report.unsupported_claim_violations == 0,
        ),
        (
            "graph_proof_overclaim_count_zero",
            report.graph_proof_overclaim_count == 0,
        ),
        (
            "normal_dot_codegraph_not_mutated",
            !report.normal_dot_codegraph_mutated,
        ),
        ("public_claim_false", !report.public_claim),
        (
            "real_agent_patch_quality_claim_false",
            !report.real_agent_patch_quality_claim,
        ),
    ] {
        if !passed {
            return Err(BenchmarkError::Validation(format!(
                "local fixture gate failed required acceptance field {field}"
            )));
        }
    }
    let mut seen = BTreeSet::new();
    for result in &report.task_results {
        if !seen.insert(result.task_id.as_str()) {
            return Err(BenchmarkError::Validation(format!(
                "duplicate local fixture gate task {}",
                result.task_id
            )));
        }
        if !result.invariant_passed
            || !result.a_arm_normal_tools
            || !result.b_arm_normal_tools_plus_codegraph
            || !result.hidden_gold_absent_from_prompts
            || !result.mock_mode_scaffold_only
        {
            return Err(BenchmarkError::Validation(format!(
                "local fixture gate task {} failed required checks",
                result.task_id
            )));
        }
    }
    Ok(())
}

pub fn v1_local_fixture_gate_to_value(report: &V1LocalFixtureGateReport) -> BenchResult<Value> {
    serde_json::to_value(report).map_err(|error| BenchmarkError::Parse(error.to_string()))
}

fn task_gate_result(
    task: &V1PatchTask,
    artifacts: &V1SameAgentAbHarnessArtifacts,
    external_agent_configured: bool,
) -> BenchResult<V1LocalFixtureTaskGateResult> {
    let a = artifacts
        .arm_artifacts
        .iter()
        .find(|artifact| artifact.mode == V1AgentMode::RgOnlyAgentPlan.as_str())
        .ok_or_else(|| {
            BenchmarkError::Validation(format!("task {} missing A plan arm", task.task_id))
        })?;
    let b = artifacts
        .arm_artifacts
        .iter()
        .find(|artifact| artifact.mode == V1AgentMode::RgPlusCodegraphAgentPlan.as_str())
        .ok_or_else(|| {
            BenchmarkError::Validation(format!("task {} missing B plan arm", task.task_id))
        })?;
    let a_score = read_json(&a.score_json)?;
    let b_score = read_json(&b.score_json)?;
    let a_context = read_json(&a.context_injected_json)?;
    let b_context = read_json(&b.context_injected_json)?;
    let a_transcript = fs::read_to_string(&a.agent_transcript_md)?;
    let b_transcript = fs::read_to_string(&b.agent_transcript_md)?;
    let a_metrics = arm_metrics(a.mode.as_str(), a.status.as_str(), &a_score, &a_context);
    let b_metrics = arm_metrics(b.mode.as_str(), b.status.as_str(), &b_score, &b_context);
    let b_improvement = compare_b_improvement(&a_metrics, &b_metrics, &a_transcript, &b_transcript);

    Ok(V1LocalFixtureTaskGateResult {
        task_id: task.task_id.clone(),
        task_family: task.task_family.clone(),
        run_root: artifacts.run_root.clone(),
        artifact_manifest_json: artifacts.artifact_manifest_json.clone(),
        invariant_passed: artifacts.invariant_checks.iter().all(|check| check.passed),
        a_arm_normal_tools: arm_has_tool(&a.tool_call_log_jsonl, "rg")?
            && !arm_available_tool(&a.tool_call_log_jsonl, "codegraph")?,
        b_arm_normal_tools_plus_codegraph: arm_has_tool(&b.tool_call_log_jsonl, "rg")?
            && arm_available_tool(&b.tool_call_log_jsonl, "codegraph")?,
        hidden_gold_absent_from_prompts: prompt_hidden_gold_absent(
            task,
            &a.agent_prompt_md,
            &a.agent_prompt_json,
        )? && prompt_hidden_gold_absent(
            task,
            &b.agent_prompt_md,
            &b.agent_prompt_json,
        )?,
        mock_mode_scaffold_only: artifacts
            .arm_artifacts
            .iter()
            .filter(|artifact| artifact.mode == V1AgentMode::MockAgentScaffoldOnly.as_str())
            .all(|artifact| {
                artifact.status == "scaffold_only" && artifact.claim_scope == "scaffold_only"
            }),
        patch_modes_status: if external_agent_configured {
            artifacts.patch_modes_status.clone()
        } else {
            "blocked_external_agent_missing".to_string()
        },
        a_arm_metrics: a_metrics,
        b_arm_metrics: b_metrics,
        b_improvement,
    })
}

fn arm_metrics(
    mode: &str,
    status: &str,
    score: &Value,
    context: &Value,
) -> V1LocalFixtureArmMetrics {
    let plan_accuracy_score = plan_accuracy_score(&score["plan_accuracy"]);
    let proof_discipline_score = proof_discipline_score(&score["proof_discipline"]);
    let hallucination_trap_score = hallucination_trap_score(&score["hallucination"]);
    let evidence_alignment_score = evidence_alignment_score(&score["evidence_alignment"]);
    let codegraph_context_bytes = context["packet"]["context_packet_bytes"]
        .as_u64()
        .unwrap_or(0);
    V1LocalFixtureArmMetrics {
        mode: mode.to_string(),
        status: status.to_string(),
        plan_accuracy_score,
        proof_discipline_score,
        hallucination_trap_score,
        evidence_alignment_score,
        wrong_file_edits: score["hallucination"]["wrong_file_edits"]
            .as_u64()
            .unwrap_or(0),
        nonexistent_symbol_refs: score["hallucination"]["nonexistent_symbol_references"]
            .as_u64()
            .unwrap_or(0),
        unsupported_claims: score["hallucination"]["unsupported_claims"]
            .as_u64()
            .unwrap_or(0),
        codegraph_context_available: score["codegraph_attribution"]["codegraph_context_available"]
            .as_bool()
            .unwrap_or(false),
        codegraph_attribution_status: score["codegraph_attribution"]["attribution_status"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        context_bytes: score["cost_performance"]["context_bytes"]
            .as_u64()
            .unwrap_or(0),
        codegraph_context_bytes,
        tool_calls: score["cost_performance"]["tool_calls_total"]
            .as_u64()
            .unwrap_or(0),
        wall_ms: score["cost_performance"]["wall_ms"].as_u64().unwrap_or(0),
        graph_proof_overclaim_count: context["packet"]["graph_proof_overclaim_count"]
            .as_u64()
            .unwrap_or(0)
            + score["hallucination"]["false_graph_proof_claims"]
                .as_u64()
                .unwrap_or(0),
        proof_discipline_violation_count: score["proof_discipline"]["violation_count"]
            .as_u64()
            .unwrap_or(0),
    }
}

fn compare_b_improvement(
    a: &V1LocalFixtureArmMetrics,
    b: &V1LocalFixtureArmMetrics,
    a_transcript: &str,
    b_transcript: &str,
) -> V1LocalFixtureBImprovement {
    let plan_quality_improved = b.plan_accuracy_score > a.plan_accuracy_score;
    let hallucination_risk_reduced = b.hallucination_trap_score > a.hallucination_trap_score
        || b.wrong_file_edits < a.wrong_file_edits
        || b.nonexistent_symbol_refs < a.nonexistent_symbol_refs;
    let proof_discipline_improved = b.proof_discipline_score > a.proof_discipline_score;
    let evidence_alignment_improved = b.evidence_alignment_score > a.evidence_alignment_score;
    let proof_boundary_explicitness_improved = !a_transcript
        .to_ascii_lowercase()
        .contains("not graph proof")
        && b_transcript
            .to_ascii_lowercase()
            .contains("not graph proof");
    let classification = if plan_quality_improved
        || hallucination_risk_reduced
        || proof_discipline_improved
        || proof_boundary_explicitness_improved
    {
        "positive_local_fixture_signal"
    } else if evidence_alignment_improved {
        "neutral_with_evidence_alignment_improvement"
    } else {
        "neutral_no_b_arm_win"
    }
    .to_string();
    V1LocalFixtureBImprovement {
        plan_quality_improved,
        hallucination_risk_reduced,
        proof_discipline_improved,
        evidence_alignment_improved,
        proof_boundary_explicitness_improved,
        classification,
    }
}

fn expected_evidence_summary(
    results: &[V1LocalFixtureTaskGateResult],
) -> V1ExpectedEvidenceSummary {
    let task_ids = results
        .iter()
        .filter(|result| {
            result.b_improvement.plan_quality_improved
                || result.b_improvement.hallucination_risk_reduced
                || result.b_improvement.proof_discipline_improved
                || result.b_improvement.proof_boundary_explicitness_improved
        })
        .map(|result| result.task_id.clone())
        .collect::<Vec<_>>();
    let b_arm_improvement_found = !task_ids.is_empty();
    V1ExpectedEvidenceSummary {
        b_arm_improvement_found,
        classification: if b_arm_improvement_found {
            "positive_local_fixture_signal".to_string()
        } else if results
            .iter()
            .any(|result| result.b_improvement.evidence_alignment_improved)
        {
            "neutral_with_evidence_alignment_improvement".to_string()
        } else {
            "neutral_no_b_arm_win".to_string()
        },
        task_ids,
    }
}

fn recompute_gate_totals(
    mut report: V1LocalFixtureGateReport,
) -> BenchResult<V1LocalFixtureGateReport> {
    report.same_agent_ab_invariant_passed = report
        .task_results
        .iter()
        .all(|result| result.invariant_passed);
    report.mock_mode_scaffold_only = report
        .task_results
        .iter()
        .all(|result| result.mock_mode_scaffold_only);
    report.gold_leakage_violations = report
        .task_results
        .iter()
        .filter(|result| !result.hidden_gold_absent_from_prompts)
        .count() as u64;
    report.claimability_violations = report
        .task_results
        .iter()
        .filter(|result| {
            result.b_arm_metrics.codegraph_context_available
                && matches!(
                    result.b_arm_metrics.codegraph_attribution_status.as_str(),
                    "valid"
                )
                && result.b_arm_metrics.codegraph_context_bytes == 0
        })
        .count() as u64;
    report.unsupported_claim_violations = report
        .task_results
        .iter()
        .map(|result| {
            result.a_arm_metrics.unsupported_claims + result.b_arm_metrics.unsupported_claims
        })
        .sum();
    report.graph_proof_overclaim_count = report
        .task_results
        .iter()
        .map(|result| {
            result.a_arm_metrics.graph_proof_overclaim_count
                + result.b_arm_metrics.graph_proof_overclaim_count
        })
        .sum();
    validate_v1_local_fixture_gate_report(&report)?;
    Ok(report)
}

fn plan_accuracy_score(value: &Value) -> f64 {
    let bool_fields = [
        "correct_implementation_surface",
        "correct_test_plan",
        "no_nonexistent_symbols_in_plan",
        "no_wrong_files_in_plan",
        "correct_unknowns",
        "correct_assumptions",
        "correct_validation_path",
    ];
    let bool_sum = bool_fields
        .iter()
        .filter(|field| value[**field].as_bool().unwrap_or(false))
        .count() as f64;
    let architecture = match value["architecture_explanation_quality"]
        .as_str()
        .unwrap_or("weak")
    {
        "sufficient" => 1.0,
        "partial" => 0.5,
        _ => 0.0,
    };
    let minimality = value["minimality_over_edit_score"].as_f64().unwrap_or(0.0);
    ((bool_sum + architecture + minimality) / 9.0 * 1000.0).round() / 1000.0
}

fn proof_discipline_score(value: &Value) -> f64 {
    let violations = value["violation_count"].as_u64().unwrap_or(7);
    ((1.0 - (violations.min(7) as f64 / 7.0)) * 1000.0).round() / 1000.0
}

fn hallucination_trap_score(value: &Value) -> f64 {
    let fields = [
        "wrong_file_edits",
        "forbidden_file_edits",
        "nonexistent_symbol_references",
        "unsupported_claims",
        "false_graph_proof_claims",
        "stale_evidence_misuse",
        "test_mock_leakage",
        "over_editing",
        "missed_expected_files",
        "unsafe_confidence",
        "no_proof_misuse",
    ];
    let total = fields
        .iter()
        .map(|field| value[*field].as_u64().unwrap_or(0))
        .sum::<u64>();
    ((1.0 - (total.min(10) as f64 / 10.0)) * 1000.0).round() / 1000.0
}

fn evidence_alignment_score(value: &Value) -> f64 {
    let fields = [
        "plan_cites_relevant_visible_evidence",
        "plan_cites_codegraph_packet_when_b_arm_uses_it",
        "patch_touches_files_surfaced_by_valid_evidence",
        "patch_avoids_forbidden_files",
        "validation_steps_match_evidence",
        "test_commands_align_with_task",
    ];
    let true_count = fields
        .iter()
        .filter(|field| value[**field].as_bool().unwrap_or(false))
        .count() as f64;
    (true_count / fields.len() as f64 * 1000.0).round() / 1000.0
}

fn scenario_for_task(task: &V1PatchTask) -> V1CodeGraphContextScenario {
    if task.task_id == "local_no_proof_fallback_runbook" {
        V1CodeGraphContextScenario::CandidateOnlyNoProof
    } else {
        V1CodeGraphContextScenario::FreshCandidateOnly
    }
}

fn aggregate_patch_modes_status(
    results: &[V1LocalFixtureTaskGateResult],
    external_agent_configured: bool,
) -> String {
    if !external_agent_configured {
        return "blocked_external_agent_missing".to_string();
    }
    let statuses = results
        .iter()
        .map(|result| result.patch_modes_status.as_str())
        .collect::<BTreeSet<_>>();
    if statuses.contains("ready") {
        "ready".to_string()
    } else if statuses.contains("blocked_external_agent_missing") {
        "blocked_external_agent_missing".to_string()
    } else {
        "not_run".to_string()
    }
}

fn prompt_hidden_gold_absent(
    task: &V1PatchTask,
    prompt_md: &str,
    prompt_json: &str,
) -> BenchResult<bool> {
    let blob = format!(
        "{}\n{}",
        fs::read_to_string(prompt_md)?,
        fs::read_to_string(prompt_json)?
    );
    let visible_prompt = format!("{}\n{}", task.issue_text, task.task_prompt).to_ascii_lowercase();
    Ok(task
        .hidden_gold_files
        .iter()
        .chain(task.hidden_gold_symbols.iter())
        .chain(task.expected_touched_files.iter())
        .chain(task.expected_tests.iter())
        .filter(|value| !value.trim().is_empty())
        .all(|value| {
            !contains_case_insensitive(&blob, value)
                || visible_prompt.contains(&value.to_ascii_lowercase())
        }))
}

fn arm_has_tool(jsonl_path: &str, tool_name: &str) -> BenchResult<bool> {
    Ok(read_jsonl(jsonl_path)?
        .iter()
        .any(|value| value["tool"].as_str() == Some(tool_name)))
}

fn arm_available_tool(jsonl_path: &str, tool_name: &str) -> BenchResult<bool> {
    Ok(read_jsonl(jsonl_path)?.iter().any(|value| {
        value["tool"].as_str() == Some(tool_name)
            && value["available_in_arm"].as_bool() == Some(true)
    }))
}

fn read_json(path: &str) -> BenchResult<Value> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|error| BenchmarkError::Parse(error.to_string()))
}

fn read_jsonl(path: &str) -> BenchResult<Vec<Value>> {
    let text = fs::read_to_string(path)?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line).map_err(|error| BenchmarkError::Parse(error.to_string()))
        })
        .collect()
}

fn read_manifest_bool(path: &str, key: &str) -> bool {
    read_json(path)
        .ok()
        .and_then(|value| value[key].as_bool())
        .unwrap_or(false)
}

fn gate_artifact_manifest(report: &V1LocalFixtureGateReport) -> Value {
    json!({
        "schema_version": V1_LOCAL_FIXTURE_GATE_SCHEMA_VERSION,
        "gate": V1_LOCAL_FIXTURE_GATE_NAME,
        "run_id": report.run_id,
        "run_root": report.run_root,
        "task_count": report.task_results.len(),
        "task_artifact_manifests": report.task_results.iter().map(|result| result.artifact_manifest_json.clone()).collect::<Vec<_>>(),
        "same_agent_ab_invariant_passed": report.same_agent_ab_invariant_passed,
        "mock_mode_scaffold_only": report.mock_mode_scaffold_only,
        "gold_leakage_violations": report.gold_leakage_violations,
        "claimability_violations": report.claimability_violations,
        "unsupported_claim_violations": report.unsupported_claim_violations,
        "graph_proof_overclaim_count": report.graph_proof_overclaim_count,
        "normal_dot_codegraph_mutated": report.normal_dot_codegraph_mutated,
        "public_claim": false,
        "real_agent_patch_quality_claim": false
    })
}

fn write_json(path: &Path, value: &impl Serialize) -> BenchResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| BenchmarkError::Parse(error.to_string()))?;
    fs::write(path, format!("{text}\n"))?;
    Ok(())
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    !needle.trim().is_empty()
        && haystack
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn dot_codegraph_state(workspace_root: &Path) -> Option<(bool, u64)> {
    fs::metadata(workspace_root.join(".codegraph"))
        .map(|metadata| (metadata.is_dir(), metadata.len()))
        .ok()
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static LOCAL_FIXTURE_GATE_TEST_RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn gate_options() -> V1LocalFixtureGateOptions {
        let mut options = default_v1_local_fixture_gate_options();
        let sequence = LOCAL_FIXTURE_GATE_TEST_RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
        let run_id = format!(
            "v1-local-fixture-gate-test-{}-{}-{}",
            std::process::id(),
            unix_ms(),
            sequence
        );
        let root = env::var("CODEGRAPH_BENCH_V1_LOCAL_FIXTURE_GATE_ROOT")
            .map(|root| PathBuf::from(root).join(&run_id))
            .unwrap_or_else(|_| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("..")
                    .join("target")
                    .join("codegraph-bench-runs")
                    .join(&run_id)
            });
        options.run_id = Some(run_id);
        options.run_root = Some(root);
        options
    }

    #[test]
    fn v1_local_fixture_gate_runs_all_local_fixtures() {
        let report = run_v1_local_fixture_gate(gate_options()).expect("local fixture gate");
        validate_v1_local_fixture_gate_report(&report).expect("gate validates");
        assert_eq!(
            report.local_fixture_tasks_run.len(),
            REQUIRED_LOCAL_FIXTURE_TASK_COUNT
        );
        assert_eq!(report.patch_modes_status, "blocked_external_agent_missing");
        assert!(report.task_results.iter().all(|result| {
            result.a_arm_metrics.mode == "rg_only_agent_plan"
                && result.b_arm_metrics.mode == "rg_plus_codegraph_agent_plan"
        }));
        assert!(report.expected_evidence.b_arm_improvement_found);
    }

    #[test]
    fn v1_local_fixture_gate_json_shape_validates() {
        let report = run_v1_local_fixture_gate(gate_options()).expect("local fixture gate");
        let value = v1_local_fixture_gate_to_value(&report).expect("gate json");
        assert_eq!(value["public_claim"], false);
        assert_eq!(value["real_agent_patch_quality_claim"], false);
        assert_eq!(value["normal_dot_codegraph_mutated"], false);
        assert!(value["local_fixture_tasks_run"].as_array().unwrap().len() >= 10);
    }
}
