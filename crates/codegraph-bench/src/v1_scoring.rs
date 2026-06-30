//! Benchmark Layer v1 patch-outcome and evidence scoring.
//!
//! These scorers intentionally keep patch outcome, agent reliability, evidence
//! discipline, CodeGraph attribution, and cost/performance separate. Passing
//! tests never implies CodeGraph helped; mock, scaffold, blocked, and plan-only
//! runs never become real patch-quality outcomes.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    evaluate_v1_codegraph_attribution, BenchResult, BenchmarkError, V1AgentMode, V1ArmRunPlan,
    V1CodeGraphAttribution, V1CodeGraphContextInjection, V1CommandRecord, V1PatchTask,
};

pub const V1_PATCH_OUTCOME_SCORING_SCHEMA_VERSION: u32 = 1;
pub const V1_PATCH_OUTCOME_SCORING_NAME: &str = "benchmark_v1_patch_outcome_evidence_scoring";

#[derive(Debug, Clone, Copy)]
pub struct V1PatchOutcomeScoringInput<'a> {
    pub task: &'a V1PatchTask,
    pub arm_plan: &'a V1ArmRunPlan,
    pub status: &'a str,
    pub claim_scope: &'a str,
    pub patch_diff: Option<&'a str>,
    pub transcript: &'a str,
    pub tool_calls: &'a [Value],
    pub command_records: &'a [V1CommandRecord],
    pub context_injection: &'a V1CodeGraphContextInjection,
    pub wall_ms: u64,
    pub cost_estimate: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V1PatchOutcomeEvidenceScore {
    pub schema_version: u32,
    pub scorer: String,
    pub task_id: String,
    pub mode: String,
    pub arm: String,
    pub status: String,
    pub claim_scope: String,
    pub real_agent_patch_quality: bool,
    pub patch_outcome: V1PatchOutcomeScore,
    pub hallucination: V1WrongContextHallucinationScore,
    pub plan_accuracy: V1PlanAccuracyScore,
    pub proof_discipline: V1ProofDisciplineScore,
    pub evidence_alignment: V1EvidenceAlignmentScore,
    pub codegraph_attribution: V1CodeGraphAttributionScore,
    pub cost_performance: V1CostPerformanceScore,
    pub mock_or_plan_counted_as_real_patch_quality: bool,
    pub public_claim: bool,
    pub real_agent_patch_quality_claim: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1PatchOutcomeScore {
    pub patch_applies: Option<bool>,
    pub tests_run: Option<bool>,
    pub tests_pass: Option<bool>,
    pub resolved: Option<bool>,
    pub no_syntax_error: Option<bool>,
    pub no_runtime_failure_if_tests_reveal: Option<bool>,
    pub patch_size_lines: Option<u64>,
    pub changed_files_count: Option<u64>,
    pub retry_count: Option<u64>,
    pub final_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1WrongContextHallucinationScore {
    pub wrong_file_edits: u64,
    pub forbidden_file_edits: u64,
    pub nonexistent_symbol_references: u64,
    pub unsupported_claims: u64,
    pub false_graph_proof_claims: u64,
    pub stale_evidence_misuse: u64,
    pub test_mock_leakage: u64,
    pub over_editing: u64,
    pub missed_expected_files: u64,
    pub unsafe_confidence: u64,
    pub no_proof_misuse: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V1PlanAccuracyScore {
    pub correct_implementation_surface: bool,
    pub correct_test_plan: bool,
    pub no_nonexistent_symbols_in_plan: bool,
    pub no_wrong_files_in_plan: bool,
    pub correct_unknowns: bool,
    pub correct_assumptions: bool,
    pub correct_validation_path: bool,
    pub architecture_explanation_quality: String,
    pub minimality_over_edit_score: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1ProofDisciplineScore {
    pub text_evidence_not_treated_as_behavior_proof: bool,
    pub symbol_existence_not_treated_as_correctness_proof: bool,
    pub candidate_evidence_not_treated_as_graph_proof: bool,
    pub source_navigation_evidence_not_treated_as_graph_proof: bool,
    pub graph_proof_claims_require_graph_source_verification: bool,
    pub unknown_no_proof_path_found_respected: bool,
    pub diagnostic_only_evidence_not_treated_as_claimable: bool,
    pub violation_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1EvidenceAlignmentScore {
    pub plan_cites_relevant_visible_evidence: bool,
    pub plan_cites_codegraph_packet_when_b_arm_uses_it: bool,
    pub patch_touches_files_surfaced_by_valid_evidence: bool,
    pub patch_avoids_forbidden_files: bool,
    pub validation_steps_match_evidence: bool,
    pub test_commands_align_with_task: bool,
    pub edit_order_follows_evidence_where_observable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1CodeGraphAttributionScore {
    pub codegraph_context_available: bool,
    pub codegraph_context_used_by_agent: bool,
    pub codegraph_context_cited_in_plan: bool,
    pub codegraph_context_aligned_with_patch: bool,
    pub codegraph_context_preceded_correct_edit: bool,
    pub codegraph_warning_ignored: bool,
    pub codegraph_context_attribution_valid: bool,
    pub attribution_status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V1CostPerformanceScore {
    pub wall_ms: Option<u64>,
    pub external_agent_ms: Option<u64>,
    pub codegraph_context_ms: Option<u64>,
    pub rg_ms: Option<u64>,
    pub test_ms: Option<u64>,
    pub tool_calls_total: u64,
    pub rg_calls: u64,
    pub codegraph_calls: u64,
    pub edit_calls: u64,
    pub test_calls: u64,
    pub estimated_tokens_in: Option<u64>,
    pub estimated_tokens_out: Option<u64>,
    pub context_bytes: Option<u64>,
    pub codegraph_context_bytes: Option<u64>,
    pub cost_estimate: Option<f64>,
    pub cost_per_solved_task: Option<f64>,
}

pub fn score_v1_patch_outcome_evidence(
    input: V1PatchOutcomeScoringInput<'_>,
) -> V1PatchOutcomeEvidenceScore {
    let patch_diff = input.patch_diff.unwrap_or("");
    let changed_files = changed_files_from_diff(patch_diff);
    let blob = evidence_blob(input.transcript, patch_diff);
    let lower_blob = blob.to_ascii_lowercase();
    let real_agent_patch_quality = real_patch_execution_ran(input.arm_plan.mode, input.status);

    let hallucination = score_hallucination(input.task, &changed_files, &lower_blob, input);
    let patch_outcome = score_patch_outcome(
        input,
        real_agent_patch_quality,
        &changed_files,
        patch_diff,
        &hallucination,
    );
    let proof_discipline = score_proof_discipline(input, &lower_blob);
    let plan_accuracy = score_plan_accuracy(
        input,
        &changed_files,
        &lower_blob,
        &hallucination,
        &proof_discipline,
    );
    let evidence_alignment = score_evidence_alignment(input, &changed_files, &lower_blob);
    let codegraph_attribution = score_codegraph_attribution(input, patch_diff);
    let cost_performance = score_cost_performance(input, &patch_outcome);

    V1PatchOutcomeEvidenceScore {
        schema_version: V1_PATCH_OUTCOME_SCORING_SCHEMA_VERSION,
        scorer: V1_PATCH_OUTCOME_SCORING_NAME.to_string(),
        task_id: input.task.task_id.clone(),
        mode: input.arm_plan.mode.as_str().to_string(),
        arm: input.arm_plan.arm.clone(),
        status: input.status.to_string(),
        claim_scope: input.claim_scope.to_string(),
        real_agent_patch_quality,
        patch_outcome,
        hallucination,
        plan_accuracy,
        proof_discipline,
        evidence_alignment,
        codegraph_attribution,
        cost_performance,
        mock_or_plan_counted_as_real_patch_quality: false,
        public_claim: false,
        real_agent_patch_quality_claim: false,
    }
}

pub fn score_v1_patch_outcome_evidence_value(
    input: V1PatchOutcomeScoringInput<'_>,
) -> BenchResult<Value> {
    serde_json::to_value(score_v1_patch_outcome_evidence(input))
        .map_err(|error| BenchmarkError::Parse(error.to_string()))
}

pub fn validate_v1_patch_outcome_evidence_score_json(value: &Value) -> BenchResult<()> {
    for field in [
        "schema_version",
        "scorer",
        "task_id",
        "mode",
        "status",
        "real_agent_patch_quality",
        "patch_outcome",
        "hallucination",
        "plan_accuracy",
        "proof_discipline",
        "evidence_alignment",
        "codegraph_attribution",
        "cost_performance",
        "public_claim",
        "real_agent_patch_quality_claim",
    ] {
        if value.get(field).is_none() {
            return Err(BenchmarkError::Validation(format!(
                "v1 scoring JSON missing required field {field}"
            )));
        }
    }
    for field in [
        "patch_applies",
        "tests_run",
        "tests_pass",
        "resolved",
        "no_syntax_error",
        "no_runtime_failure_if_tests_reveal",
        "patch_size_lines",
        "changed_files_count",
        "retry_count",
        "final_status",
    ] {
        if value["patch_outcome"].get(field).is_none() {
            return Err(BenchmarkError::Validation(format!(
                "v1 patch outcome score missing required field {field}"
            )));
        }
    }
    for field in [
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
    ] {
        if value["hallucination"].get(field).is_none() {
            return Err(BenchmarkError::Validation(format!(
                "v1 hallucination score missing required field {field}"
            )));
        }
    }
    if value["public_claim"].as_bool() != Some(false)
        || value["real_agent_patch_quality_claim"].as_bool() != Some(false)
    {
        return Err(BenchmarkError::Validation(
            "v1 score must not make public or patch-quality claims".to_string(),
        ));
    }
    Ok(())
}

fn score_patch_outcome(
    input: V1PatchOutcomeScoringInput<'_>,
    real_agent_patch_quality: bool,
    changed_files: &BTreeSet<String>,
    patch_diff: &str,
    hallucination: &V1WrongContextHallucinationScore,
) -> V1PatchOutcomeScore {
    if !real_agent_patch_quality {
        return V1PatchOutcomeScore {
            patch_applies: None,
            tests_run: None,
            tests_pass: None,
            resolved: None,
            no_syntax_error: None,
            no_runtime_failure_if_tests_reveal: None,
            patch_size_lines: None,
            changed_files_count: None,
            retry_count: None,
            final_status: not_applicable_status(input.arm_plan.mode, input.status),
        };
    }

    let patch_applies = !patch_diff.trim().is_empty() && input.status != "timed_out";
    let tests_run = test_was_run(input.tool_calls, input.command_records);
    let tests_pass = tests_run && test_passed(input.tool_calls, input.status);
    let lower_blob = evidence_blob(input.transcript, patch_diff).to_ascii_lowercase();
    let no_syntax_error = !contains_any(&lower_blob, &["syntaxerror", "syntax error"]);
    let no_runtime_failure =
        !tests_run || !contains_any(&lower_blob, &["runtimeerror", "traceback", "exception"]);
    let resolved = patch_applies
        && tests_run
        && tests_pass
        && hallucination.forbidden_file_edits == 0
        && hallucination.nonexistent_symbol_references == 0;

    V1PatchOutcomeScore {
        patch_applies: Some(patch_applies),
        tests_run: Some(tests_run),
        tests_pass: Some(tests_pass),
        resolved: Some(resolved),
        no_syntax_error: Some(no_syntax_error),
        no_runtime_failure_if_tests_reveal: Some(no_runtime_failure),
        patch_size_lines: Some(patch_size_lines(patch_diff)),
        changed_files_count: Some(changed_files.len() as u64),
        retry_count: Some(retry_count(input.command_records)),
        final_status: input.status.to_string(),
    }
}

fn score_hallucination(
    task: &V1PatchTask,
    changed_files: &BTreeSet<String>,
    lower_blob: &str,
    input: V1PatchOutcomeScoringInput<'_>,
) -> V1WrongContextHallucinationScore {
    let forbidden_file_edits = changed_files
        .iter()
        .filter(|file| matches_any_path(file, &task.forbidden_files))
        .count() as u64;
    let allowed_files = task
        .expected_touched_files
        .iter()
        .chain(task.expected_tests.iter())
        .cloned()
        .collect::<Vec<_>>();
    let wrong_file_edits = changed_files
        .iter()
        .filter(|file| !matches_any_path(file, &allowed_files))
        .count() as u64;
    let nonexistent_symbol_references = task
        .forbidden_symbols
        .iter()
        .filter(|symbol| contains_case_insensitive(lower_blob, symbol))
        .count() as u64;
    let unsupported_claims = unsupported_claim_count(lower_blob, input);
    let false_graph_proof_claims = false_graph_proof_claim_count(lower_blob, input);
    let stale_evidence_misuse = stale_evidence_misuse_count(lower_blob, input);
    let test_mock_leakage = changed_files
        .iter()
        .filter(|file| {
            let lower = file.to_ascii_lowercase();
            (lower.contains("mock") || lower.contains("fixture") || lower.contains("test"))
                && !matches_any_path(file, &task.expected_tests)
        })
        .count() as u64;
    let allowed_count = allowed_files.len().max(1);
    let over_editing = changed_files.len().saturating_sub(allowed_count + 1) as u64;
    let missed_expected_files = task
        .expected_touched_files
        .iter()
        .filter(|file| !matches_any_path(file, &changed_files.iter().cloned().collect::<Vec<_>>()))
        .count() as u64;
    let unsafe_confidence = count_phrases(
        lower_blob,
        &[
            "guaranteed",
            "definitely correct",
            "cannot fail",
            "proves correctness",
            "no risk",
        ],
    );
    let no_proof_misuse = no_proof_misuse_count(lower_blob, input);

    V1WrongContextHallucinationScore {
        wrong_file_edits,
        forbidden_file_edits,
        nonexistent_symbol_references,
        unsupported_claims,
        false_graph_proof_claims,
        stale_evidence_misuse,
        test_mock_leakage,
        over_editing,
        missed_expected_files,
        unsafe_confidence,
        no_proof_misuse,
    }
}

fn score_plan_accuracy(
    input: V1PatchOutcomeScoringInput<'_>,
    changed_files: &BTreeSet<String>,
    lower_blob: &str,
    hallucination: &V1WrongContextHallucinationScore,
    proof_discipline: &V1ProofDisciplineScore,
) -> V1PlanAccuracyScore {
    let correct_implementation_surface = changed_files
        .iter()
        .any(|file| matches_any_path(file, &input.task.expected_touched_files))
        || any_visible_evidence_mentioned(input.task, lower_blob);
    let correct_test_plan = lower_blob.contains("test")
        || lower_blob.contains("pytest")
        || contains_case_insensitive(lower_blob, &input.task.test_command);
    let correct_unknowns = no_unknown_context(input)
        || lower_blob.contains("unknown")
        || lower_blob.contains("no proof")
        || lower_blob.contains("no_proof_path_found")
        || lower_blob.contains("stale");
    let correct_validation_path = correct_test_plan
        || input
            .tool_calls
            .iter()
            .any(|call| tool_name(call).contains("test"));
    let minimality_over_edit_score = minimality_score(changed_files.len(), hallucination);
    let architecture_explanation_quality = if correct_implementation_surface
        && correct_validation_path
        && proof_discipline.violation_count == 0
    {
        "sufficient"
    } else if correct_implementation_surface || correct_validation_path {
        "partial"
    } else {
        "weak"
    }
    .to_string();

    V1PlanAccuracyScore {
        correct_implementation_surface,
        correct_test_plan,
        no_nonexistent_symbols_in_plan: hallucination.nonexistent_symbol_references == 0,
        no_wrong_files_in_plan: hallucination.wrong_file_edits == 0,
        correct_unknowns,
        correct_assumptions: hallucination.unsafe_confidence == 0,
        correct_validation_path,
        architecture_explanation_quality,
        minimality_over_edit_score,
    }
}

fn score_proof_discipline(
    input: V1PatchOutcomeScoringInput<'_>,
    lower_blob: &str,
) -> V1ProofDisciplineScore {
    let text_ok = !contains_any(
        lower_blob,
        &[
            "text evidence is graph proof",
            "text evidence proves behavior",
            "text evidence proves correctness",
        ],
    );
    let symbol_ok = !contains_any(
        lower_blob,
        &[
            "symbol exists therefore correct",
            "symbol existence proves correctness",
            "symbol evidence proves behavior",
        ],
    );
    let candidate_ok = !contains_any(
        lower_blob,
        &[
            "candidate evidence is graph proof",
            "candidate evidence proves",
            "candidate-only graph proof",
        ],
    );
    let source_nav_ok = !contains_any(
        lower_blob,
        &[
            "source-navigation evidence is graph proof",
            "source navigation evidence is graph proof",
            "source navigation proves correctness",
        ],
    );
    let graph_claims_require_proof = !claims_graph_proof(lower_blob)
        || input
            .context_injection
            .packet
            .as_ref()
            .map(|packet| packet.db_claimable && packet.graph_proof_available)
            .unwrap_or(false);
    let unknown_no_proof_respected = input
        .context_injection
        .packet
        .as_ref()
        .map(|packet| {
            packet.codegraph_context_status != "no_proof_path_found"
                || lower_blob.contains("no proof")
                || lower_blob.contains("no_proof_path_found")
                || lower_blob.contains("unknown")
                || !claims_graph_proof(lower_blob)
        })
        .unwrap_or(true);
    let diagnostic_ok = !contains_any(
        lower_blob,
        &[
            "diagnostic evidence is claimable",
            "scaffold proves patch quality",
            "plan-only proves patch quality",
            "mock proves patch quality",
        ],
    );
    let checks = [
        text_ok,
        symbol_ok,
        candidate_ok,
        source_nav_ok,
        graph_claims_require_proof,
        unknown_no_proof_respected,
        diagnostic_ok,
    ];

    V1ProofDisciplineScore {
        text_evidence_not_treated_as_behavior_proof: text_ok,
        symbol_existence_not_treated_as_correctness_proof: symbol_ok,
        candidate_evidence_not_treated_as_graph_proof: candidate_ok,
        source_navigation_evidence_not_treated_as_graph_proof: source_nav_ok,
        graph_proof_claims_require_graph_source_verification: graph_claims_require_proof,
        unknown_no_proof_path_found_respected: unknown_no_proof_respected,
        diagnostic_only_evidence_not_treated_as_claimable: diagnostic_ok,
        violation_count: checks.iter().filter(|ok| !**ok).count() as u64,
    }
}

fn score_evidence_alignment(
    input: V1PatchOutcomeScoringInput<'_>,
    changed_files: &BTreeSet<String>,
    lower_blob: &str,
) -> V1EvidenceAlignmentScore {
    let attribution = codegraph_attribution(input, input.patch_diff.unwrap_or(""));
    let b_arm_uses_context = input.arm_plan.codegraph_context_injected
        && input
            .context_injection
            .packet
            .as_ref()
            .map(|packet| packet.codegraph_context_available)
            .unwrap_or(false);
    let test_commands_align_with_task =
        contains_case_insensitive(lower_blob, &input.task.test_command)
            || input
                .tool_calls
                .iter()
                .any(|call| tool_name(call).contains("test"));
    let patch_touches_valid_evidence = changed_files
        .iter()
        .any(|file| matches_any_path(file, &input.task.expected_touched_files))
        || attribution.codegraph_context_aligned_with_patch;

    V1EvidenceAlignmentScore {
        plan_cites_relevant_visible_evidence: any_visible_evidence_mentioned(
            input.task, lower_blob,
        ),
        plan_cites_codegraph_packet_when_b_arm_uses_it: !b_arm_uses_context
            || attribution.codegraph_context_cited_in_plan,
        patch_touches_files_surfaced_by_valid_evidence: patch_touches_valid_evidence,
        patch_avoids_forbidden_files: !changed_files
            .iter()
            .any(|file| matches_any_path(file, &input.task.forbidden_files)),
        validation_steps_match_evidence: test_commands_align_with_task,
        test_commands_align_with_task,
        edit_order_follows_evidence_where_observable: if changed_files.is_empty() {
            None
        } else {
            Some(
                lower_blob.contains("before editing")
                    || lower_blob.contains("then edit")
                    || lower_blob.contains("after inspecting"),
            )
        },
    }
}

fn score_codegraph_attribution(
    input: V1PatchOutcomeScoringInput<'_>,
    patch_diff: &str,
) -> V1CodeGraphAttributionScore {
    let attribution = codegraph_attribution(input, patch_diff);
    V1CodeGraphAttributionScore {
        codegraph_context_available: attribution.codegraph_context_available,
        codegraph_context_used_by_agent: attribution.codegraph_context_used_by_agent,
        codegraph_context_cited_in_plan: attribution.codegraph_context_cited_in_plan,
        codegraph_context_aligned_with_patch: attribution.codegraph_context_aligned_with_patch,
        codegraph_context_preceded_correct_edit: attribution
            .codegraph_context_preceded_correct_edit,
        codegraph_warning_ignored: attribution.codegraph_context_warning_ignored,
        codegraph_context_attribution_valid: attribution.codegraph_context_attribution_valid,
        attribution_status: attribution.attribution_status,
    }
}

fn score_cost_performance(
    input: V1PatchOutcomeScoringInput<'_>,
    patch_outcome: &V1PatchOutcomeScore,
) -> V1CostPerformanceScore {
    let tool_calls_total = input.tool_calls.len() as u64 + input.command_records.len() as u64;
    let rg_calls = count_tool_calls(input, "rg");
    let codegraph_calls = count_tool_calls(input, "codegraph");
    let edit_calls = count_tool_calls(input, "edit") + count_tool_calls(input, "apply_patch");
    let test_calls = count_tool_calls(input, "test") + count_tool_calls(input, "pytest");
    let context_bytes = visible_context_bytes(input.task)
        + input
            .context_injection
            .packet
            .as_ref()
            .map(|packet| packet.context_packet_bytes as u64)
            .unwrap_or(0);
    let codegraph_context_bytes = input
        .context_injection
        .packet
        .as_ref()
        .map(|packet| packet.context_packet_bytes as u64);
    let cost_per_solved_task = match (input.cost_estimate, patch_outcome.resolved) {
        (Some(cost), Some(true)) => Some(cost),
        _ => None,
    };

    V1CostPerformanceScore {
        wall_ms: Some(input.wall_ms),
        external_agent_ms: Some(sum_command_ms(input.command_records, "external_agent")),
        codegraph_context_ms: Some(sum_command_ms(input.command_records, "codegraph")),
        rg_ms: Some(sum_command_ms(input.command_records, "rg")),
        test_ms: Some(
            sum_command_ms(input.command_records, "test")
                + sum_command_ms(input.command_records, "pytest"),
        ),
        tool_calls_total,
        rg_calls,
        codegraph_calls,
        edit_calls,
        test_calls,
        estimated_tokens_in: Some(context_bytes.div_ceil(4)),
        estimated_tokens_out: Some(input.transcript.len().div_ceil(4) as u64),
        context_bytes: Some(context_bytes),
        codegraph_context_bytes,
        cost_estimate: input.cost_estimate,
        cost_per_solved_task,
    }
}

fn codegraph_attribution(
    input: V1PatchOutcomeScoringInput<'_>,
    patch_diff: &str,
) -> V1CodeGraphAttribution {
    input
        .context_injection
        .packet
        .as_ref()
        .map(|packet| evaluate_v1_codegraph_attribution(packet, input.transcript, patch_diff))
        .unwrap_or_else(|| input.context_injection.attribution.clone())
}

fn real_patch_execution_ran(mode: V1AgentMode, status: &str) -> bool {
    mode.is_patch_mode() && matches!(status, "completed_real_agent_run" | "failed" | "timed_out")
}

fn not_applicable_status(mode: V1AgentMode, status: &str) -> String {
    if status.starts_with("blocked_") || status == "ready_for_real_patch_run" {
        return status.to_string();
    }
    match mode {
        V1AgentMode::MockAgentScaffoldOnly => "not_applicable_scaffold_only".to_string(),
        mode if mode.is_plan_mode() => "not_applicable_plan_only_diagnostic".to_string(),
        _ => status.to_string(),
    }
}

fn changed_files_from_diff(diff: &str) -> BTreeSet<String> {
    let mut files = BTreeSet::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let parts = rest.split_whitespace().collect::<Vec<_>>();
            if let Some(path) = parts.get(1).or_else(|| parts.first()) {
                files.insert(clean_diff_path(path));
            }
        } else if let Some(path) = line.strip_prefix("+++ b/") {
            if path != "/dev/null" {
                files.insert(normalize_path(path));
            }
        } else if let Some(path) = line.strip_prefix("--- a/") {
            if path != "/dev/null" {
                files.insert(normalize_path(path));
            }
        }
    }
    files.into_iter().filter(|file| !file.is_empty()).collect()
}

fn clean_diff_path(path: &str) -> String {
    normalize_path(
        path.trim()
            .trim_start_matches("a/")
            .trim_start_matches("b/")
            .trim_matches('"'),
    )
}

fn patch_size_lines(diff: &str) -> u64 {
    diff.lines()
        .filter(|line| {
            (line.starts_with('+') || line.starts_with('-'))
                && !line.starts_with("+++")
                && !line.starts_with("---")
        })
        .count() as u64
}

fn retry_count(records: &[V1CommandRecord]) -> u64 {
    records
        .iter()
        .filter(|record| record.command_kind == "external_agent")
        .count()
        .saturating_sub(1) as u64
}

fn test_was_run(tool_calls: &[Value], command_records: &[V1CommandRecord]) -> bool {
    tool_calls.iter().any(|call| {
        let tool = tool_name(call);
        tool.contains("test") || tool.contains("pytest")
    }) || command_records.iter().any(|record| {
        record.command_kind.contains("test") || record.command_kind.contains("pytest")
    })
}

fn test_passed(tool_calls: &[Value], status: &str) -> bool {
    let explicit_failure = tool_calls.iter().any(|call| {
        let status = value_string(call, "status")
            .unwrap_or_default()
            .to_ascii_lowercase();
        contains_any(&status, &["fail", "failed", "error"])
    });
    let explicit_pass = tool_calls.iter().any(|call| {
        let status = value_string(call, "status")
            .unwrap_or_default()
            .to_ascii_lowercase();
        contains_any(&status, &["pass", "passed", "ok"])
    });
    explicit_pass && !explicit_failure && status == "completed_real_agent_run"
}

fn evidence_blob(transcript: &str, patch_diff: &str) -> String {
    format!("{transcript}\n{patch_diff}")
}

fn matches_any_path(path: &str, candidates: &[String]) -> bool {
    let normalized_path = normalize_path(path);
    candidates.iter().any(|candidate| {
        let normalized_candidate = normalize_path(candidate);
        !normalized_candidate.is_empty()
            && (normalized_path == normalized_candidate
                || normalized_path.ends_with(&format!("/{normalized_candidate}"))
                || normalized_candidate.ends_with(&format!("/{normalized_path}")))
    })
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim()
        .trim_start_matches("./")
        .trim_start_matches("a/")
        .trim_start_matches("b/")
        .to_ascii_lowercase()
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    !needle.trim().is_empty()
        && haystack
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn count_phrases(haystack: &str, needles: &[&str]) -> u64 {
    needles
        .iter()
        .filter(|needle| haystack.contains(**needle))
        .count() as u64
}

fn claims_graph_proof(lower_blob: &str) -> bool {
    contains_any(
        lower_blob,
        &[
            "is graph proof",
            "as graph proof",
            "graph proof proves",
            "graph-proven",
            "codegraph proved",
            "codegraph proves",
            "proof path proves",
        ],
    )
}

fn unsupported_claim_count(lower_blob: &str, input: V1PatchOutcomeScoringInput<'_>) -> u64 {
    let mut count = count_phrases(
        lower_blob,
        &[
            "beats rg",
            "official swe-bench score",
            "public benchmark claim",
            "guaranteed",
            "definitely correct",
            "proves correctness",
        ],
    );
    if claims_graph_proof(lower_blob)
        && !input
            .context_injection
            .packet
            .as_ref()
            .map(|packet| packet.graph_proof_available && packet.db_claimable)
            .unwrap_or(false)
    {
        count += 1;
    }
    count
}

fn false_graph_proof_claim_count(lower_blob: &str, input: V1PatchOutcomeScoringInput<'_>) -> u64 {
    let mut count = 0;
    if claims_graph_proof(lower_blob)
        && !input
            .context_injection
            .packet
            .as_ref()
            .map(|packet| packet.graph_proof_available && packet.db_claimable)
            .unwrap_or(false)
    {
        count += 1;
    }
    count
        + count_phrases(
            lower_blob,
            &[
                "text evidence is graph proof",
                "candidate evidence is graph proof",
                "source-navigation evidence is graph proof",
                "source navigation evidence is graph proof",
            ],
        )
}

fn stale_evidence_misuse_count(lower_blob: &str, input: V1PatchOutcomeScoringInput<'_>) -> u64 {
    input
        .context_injection
        .packet
        .as_ref()
        .filter(|packet| packet.stale_evidence_present)
        .map(|_| {
            if lower_blob.contains("codegraph") && !lower_blob.contains("stale") {
                1
            } else {
                0
            }
        })
        .unwrap_or(0)
}

fn no_proof_misuse_count(lower_blob: &str, input: V1PatchOutcomeScoringInput<'_>) -> u64 {
    input
        .context_injection
        .packet
        .as_ref()
        .filter(|packet| packet.codegraph_context_status == "no_proof_path_found")
        .map(|_| u64::from(claims_graph_proof(lower_blob)))
        .unwrap_or(0)
}

fn any_visible_evidence_mentioned(task: &V1PatchTask, lower_blob: &str) -> bool {
    task.visible_query_terms
        .iter()
        .chain(task.visible_file_hints.iter())
        .chain(task.visible_symbol_hints.iter())
        .any(|value| contains_case_insensitive(lower_blob, value))
}

fn no_unknown_context(input: V1PatchOutcomeScoringInput<'_>) -> bool {
    input
        .context_injection
        .packet
        .as_ref()
        .map(|packet| {
            packet.unknowns.is_empty()
                && packet.codegraph_context_status != "no_proof_path_found"
                && !packet.stale_evidence_present
        })
        .unwrap_or(true)
}

fn minimality_score(
    changed_file_count: usize,
    hallucination: &V1WrongContextHallucinationScore,
) -> f64 {
    if changed_file_count == 0 {
        return 1.0;
    }
    let penalty = hallucination.over_editing
        + hallucination.wrong_file_edits
        + hallucination.forbidden_file_edits;
    let score = 1.0 - (penalty as f64 / changed_file_count.max(1) as f64);
    score.clamp(0.0, 1.0)
}

fn count_tool_calls(input: V1PatchOutcomeScoringInput<'_>, needle: &str) -> u64 {
    let tool_calls = input
        .tool_calls
        .iter()
        .filter(|call| tool_name(call).contains(needle))
        .count() as u64;
    let command_calls = input
        .command_records
        .iter()
        .filter(|record| record.command_kind.contains(needle))
        .count() as u64;
    tool_calls + command_calls
}

fn sum_command_ms(records: &[V1CommandRecord], command_kind: &str) -> u64 {
    records
        .iter()
        .filter(|record| record.command_kind.contains(command_kind))
        .map(|record| record.elapsed_ms)
        .sum()
}

fn tool_name(value: &Value) -> String {
    value_string(value, "tool")
        .or_else(|| value_string(value, "command_kind"))
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn visible_context_bytes(task: &V1PatchTask) -> u64 {
    let visible = json!({
        "issue_text": task.issue_text,
        "task_prompt": task.task_prompt,
        "visible_query_terms": task.visible_query_terms,
        "visible_file_hints": task.visible_file_hints,
        "visible_symbol_hints": task.visible_symbol_hints,
        "visible_error_messages": task.visible_error_messages,
    });
    serde_json::to_vec(&visible)
        .map(|bytes| bytes.len() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_v1_codegraph_context_injection, default_v1_codegraph_context_budget,
        load_v1_patch_task_registry, v1_registry::v1_registry_path, V1CodeGraphContextScenario,
    };
    use std::path::{Path, PathBuf};

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    }

    fn task(task_id: &str) -> V1PatchTask {
        load_v1_patch_task_registry(&v1_registry_path(&workspace_root()))
            .expect("registry")
            .tasks
            .into_iter()
            .find(|task| task.task_id == task_id)
            .expect("task")
    }

    fn run_plan(task: &V1PatchTask, mode: V1AgentMode) -> V1ArmRunPlan {
        V1ArmRunPlan {
            schema_version: 1,
            harness: "test".to_string(),
            generated_before_execution: true,
            run_id: "score-test".to_string(),
            task_id: task.task_id.clone(),
            mode,
            arm: mode.arm().to_string(),
            claim_scope: mode.claim_scope().to_string(),
            model: "fixed".to_string(),
            external_agent_command: vec!["agent".to_string()],
            external_agent_env_var: "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND".to_string(),
            agent_scaffold_version: "v1".to_string(),
            system_prompt_base: "system".to_string(),
            task_prompt_base: task.task_prompt.clone(),
            repo_source: task.repo_source.clone(),
            repo_commit: task.repo_commit.clone(),
            setup_command: task.setup_command.clone(),
            test_command: task.test_command.clone(),
            timeout_ms: task.timeout_ms,
            token_budget: task.token_budget,
            tool_budget: task.tool_budget,
            environment_allowlist: vec!["PATH".to_string()],
            docker_image: task.docker_image_or_env.clone(),
            evaluator_kind: task.evaluator_kind.clone(),
            seed: Some(1),
            visible_task: task.agent_visible(),
            workspace_path: "workspace".to_string(),
            artifact_dir: "artifact".to_string(),
            codegraph_available: mode.is_codegraph_arm(),
            codegraph_context_injected: mode.is_codegraph_arm(),
            codegraph_validation_commands_available: mode.is_codegraph_arm(),
            hidden_gold_fields_excluded: true,
            public_claim_allowed: false,
            no_gold_leakage_expected: true,
            real_agent_patch_quality_claim: false,
        }
    }

    fn command(kind: &str, elapsed_ms: u64) -> V1CommandRecord {
        V1CommandRecord {
            schema_version: 1,
            run_id: "score-test".to_string(),
            task_id: "task".to_string(),
            mode: "rg_plus_codegraph_agent_patch".to_string(),
            command_kind: kind.to_string(),
            status: "completed".to_string(),
            argv: vec![kind.to_string()],
            cwd: "workspace".to_string(),
            timeout_ms: 120_000,
            stdout_path: "stdout".to_string(),
            stderr_path: "stderr".to_string(),
            exit_code: Some(0),
            elapsed_ms,
            blocked_reason: None,
        }
    }

    fn context(
        task: &V1PatchTask,
        scenario: V1CodeGraphContextScenario,
    ) -> V1CodeGraphContextInjection {
        build_v1_codegraph_context_injection(
            task,
            true,
            scenario,
            default_v1_codegraph_context_budget(),
        )
        .expect("context")
    }

    fn aligned_graph_context(task: &V1PatchTask, file: &str) -> V1CodeGraphContextInjection {
        let mut injection = context(task, V1CodeGraphContextScenario::GraphProof);
        let packet = injection.packet.as_mut().expect("packet");
        packet.critical_files = vec![file.to_string()];
        packet.proof_paths[0].files = vec![file.to_string()];
        injection
    }

    fn score_with(
        task: &V1PatchTask,
        mode: V1AgentMode,
        status: &str,
        patch: Option<&str>,
        transcript: &str,
        context: &V1CodeGraphContextInjection,
        tool_calls: Vec<Value>,
        command_records: Vec<V1CommandRecord>,
    ) -> V1PatchOutcomeEvidenceScore {
        let plan = run_plan(task, mode);
        score_v1_patch_outcome_evidence(V1PatchOutcomeScoringInput {
            task,
            arm_plan: &plan,
            status,
            claim_scope: mode.claim_scope(),
            patch_diff: patch,
            transcript,
            tool_calls: &tool_calls,
            command_records: &command_records,
            context_injection: context,
            wall_ms: 1_234,
            cost_estimate: Some(0.25),
        })
    }

    #[test]
    fn v1_scoring_perfect_patch_with_aligned_evidence() {
        let task = task("local_wrong_file_trap_discount");
        let file = "src/payments/discounts.py";
        let context = aligned_graph_context(&task, file);
        let patch = "diff --git a/src/payments/discounts.py b/src/payments/discounts.py\n--- a/src/payments/discounts.py\n+++ b/src/payments/discounts.py\n+fixed\n";
        let score = score_with(
            &task,
            V1AgentMode::RgPlusCodegraphAgentPatch,
            "completed_real_agent_run",
            Some(patch),
            "Plan: inspect checkout discount cap with CodeGraph cg-proof-visible-anchor before editing. Then run python -m pytest.",
            &context,
            vec![json!({"tool": "test", "status": "passed"})],
            vec![command("external_agent", 100), command("test", 200)],
        );
        assert_eq!(score.patch_outcome.resolved, Some(true));
        assert!(
            score
                .codegraph_attribution
                .codegraph_context_attribution_valid
        );
        assert_eq!(score.codegraph_attribution.attribution_status, "valid");
        assert_eq!(score.hallucination.forbidden_file_edits, 0);
    }

    #[test]
    fn v1_scoring_patch_passes_but_codegraph_unavailable_invalidates_attribution() {
        let task = task("local_wrong_file_trap_discount");
        let context = context(&task, V1CodeGraphContextScenario::Unavailable);
        let patch = "diff --git a/src/payments/discounts.py b/src/payments/discounts.py\n+fixed\n";
        let score = score_with(
            &task,
            V1AgentMode::RgPlusCodegraphAgentPatch,
            "completed_real_agent_run",
            Some(patch),
            "Plan: use rg self-exploration and run python -m pytest.",
            &context,
            vec![json!({"tool": "test", "status": "passed"})],
            vec![command("external_agent", 100), command("test", 100)],
        );
        assert_eq!(score.patch_outcome.resolved, Some(true));
        assert!(
            !score
                .codegraph_attribution
                .codegraph_context_attribution_valid
        );
        assert_eq!(
            score.codegraph_attribution.attribution_status,
            "invalid_unavailable"
        );
    }

    #[test]
    fn v1_scoring_patch_fails_despite_codegraph_context() {
        let task = task("local_wrong_file_trap_discount");
        let file = "src/payments/discounts.py";
        let context = aligned_graph_context(&task, file);
        let patch = "diff --git a/src/payments/discounts.py b/src/payments/discounts.py\n+broken\n";
        let score = score_with(
            &task,
            V1AgentMode::RgPlusCodegraphAgentPatch,
            "failed",
            Some(patch),
            "Plan: CodeGraph cg-proof-visible-anchor before editing. Test failed with assertion error.",
            &context,
            vec![json!({"tool": "test", "status": "failed"})],
            vec![command("external_agent", 100), command("test", 100)],
        );
        assert_eq!(score.patch_outcome.resolved, Some(false));
        assert!(score.codegraph_attribution.codegraph_context_used_by_agent);
        assert_eq!(score.patch_outcome.tests_pass, Some(false));
    }

    #[test]
    fn v1_scoring_wrong_file_edit_detected() {
        let task = task("local_wrong_file_trap_discount");
        let context = context(&task, V1CodeGraphContextScenario::FreshCandidateOnly);
        let patch = "diff --git a/src/reports/discounts.py b/src/reports/discounts.py\n+wrong\n";
        let score = score_with(
            &task,
            V1AgentMode::RgOnlyAgentPatch,
            "completed_real_agent_run",
            Some(patch),
            "Plan: edit reporting area.",
            &context,
            vec![json!({"tool": "test", "status": "passed"})],
            vec![command("external_agent", 100), command("test", 100)],
        );
        assert!(score.hallucination.wrong_file_edits > 0);
        assert!(score.hallucination.forbidden_file_edits > 0);
    }

    #[test]
    fn v1_scoring_nonexistent_symbol_reference_detected() {
        let task = task("local_nonexistent_symbol_session");
        let context = context(&task, V1CodeGraphContextScenario::FreshCandidateOnly);
        let score = score_with(
            &task,
            V1AgentMode::RgOnlyAgentPlan,
            "plan_only_diagnostic",
            None,
            "Plan: call invalidate_everywhere even though it is not present.",
            &context,
            vec![],
            vec![],
        );
        assert!(score.hallucination.nonexistent_symbol_references > 0);
        assert!(!score.plan_accuracy.no_nonexistent_symbols_in_plan);
    }

    #[test]
    fn v1_scoring_unsupported_claim_detected() {
        let task = task("local_wrong_file_trap_discount");
        let context = context(&task, V1CodeGraphContextScenario::FreshCandidateOnly);
        let score = score_with(
            &task,
            V1AgentMode::RgOnlyAgentPlan,
            "plan_only_diagnostic",
            None,
            "This plan is guaranteed and proves correctness.",
            &context,
            vec![],
            vec![],
        );
        assert!(score.hallucination.unsupported_claims > 0);
        assert!(score.hallucination.unsafe_confidence > 0);
    }

    #[test]
    fn v1_scoring_text_evidence_overclaimed_as_graph_proof_detected() {
        let task = task("local_stale_docs_trap_worker");
        let context = context(&task, V1CodeGraphContextScenario::FreshCandidateOnly);
        let score = score_with(
            &task,
            V1AgentMode::RgPlusCodegraphAgentPlan,
            "plan_only_diagnostic",
            None,
            "Text evidence is graph proof for this behavior.",
            &context,
            vec![],
            vec![],
        );
        assert!(
            !score
                .proof_discipline
                .text_evidence_not_treated_as_behavior_proof
        );
        assert!(score.hallucination.false_graph_proof_claims > 0);
    }

    #[test]
    fn v1_scoring_candidate_evidence_overclaimed_as_graph_proof_detected() {
        let task = task("local_config_driven_behavior_routes");
        let context = context(&task, V1CodeGraphContextScenario::FreshCandidateOnly);
        let score = score_with(
            &task,
            V1AgentMode::RgPlusCodegraphAgentPlan,
            "plan_only_diagnostic",
            None,
            "Candidate evidence is graph proof for the route behavior.",
            &context,
            vec![],
            vec![],
        );
        assert!(
            !score
                .proof_discipline
                .candidate_evidence_not_treated_as_graph_proof
        );
        assert!(score.hallucination.false_graph_proof_claims > 0);
    }

    #[test]
    fn v1_scoring_stale_evidence_used_detected() {
        let task = task("local_wrong_file_trap_discount");
        let context = context(&task, V1CodeGraphContextScenario::StaleGraphDb);
        let score = score_with(
            &task,
            V1AgentMode::RgPlusCodegraphAgentPlan,
            "plan_only_diagnostic",
            None,
            "Use CodeGraph evidence and edit directly.",
            &context,
            vec![],
            vec![],
        );
        assert!(score.hallucination.stale_evidence_misuse > 0);
        assert_eq!(
            score.codegraph_attribution.attribution_status,
            "invalid_stale"
        );
    }

    #[test]
    fn v1_scoring_mock_scaffold_not_counted_as_real_patch_quality() {
        let task = task("local_wrong_file_trap_discount");
        let context = context(&task, V1CodeGraphContextScenario::FreshCandidateOnly);
        let score = score_with(
            &task,
            V1AgentMode::MockAgentScaffoldOnly,
            "scaffold_only",
            Some("diff --git a/src/payments/discounts.py b/src/payments/discounts.py\n+mock\n"),
            "Mock scaffold only.",
            &context,
            vec![],
            vec![],
        );
        assert!(!score.real_agent_patch_quality);
        assert_eq!(score.patch_outcome.patch_applies, None);
        assert_eq!(
            score.patch_outcome.final_status,
            "not_applicable_scaffold_only"
        );
    }

    #[test]
    fn v1_scoring_no_external_agent_patch_metrics_are_null() {
        let task = task("local_wrong_file_trap_discount");
        let context = context(&task, V1CodeGraphContextScenario::FreshCandidateOnly);
        let score = score_with(
            &task,
            V1AgentMode::RgOnlyAgentPatch,
            "blocked_external_agent_missing",
            None,
            "Blocked because external agent is missing.",
            &context,
            vec![],
            vec![],
        );
        assert_eq!(score.patch_outcome.patch_applies, None);
        assert_eq!(score.patch_outcome.tests_run, None);
        assert_eq!(
            score.patch_outcome.final_status,
            "blocked_external_agent_missing"
        );
    }

    #[test]
    fn v1_scoring_cost_metrics_aggregate_correctly() {
        let task = task("local_wrong_file_trap_discount");
        let context = context(&task, V1CodeGraphContextScenario::FreshCandidateOnly);
        let score = score_with(
            &task,
            V1AgentMode::RgPlusCodegraphAgentPatch,
            "completed_real_agent_run",
            Some("diff --git a/src/payments/discounts.py b/src/payments/discounts.py\n+fixed\n"),
            "Plan with pytest.",
            &context,
            vec![
                json!({"tool": "rg", "status": "ok"}),
                json!({"tool": "codegraph", "status": "ok"}),
                json!({"tool": "edit", "status": "ok"}),
                json!({"tool": "test", "status": "passed"}),
            ],
            vec![
                command("external_agent", 100),
                command("rg", 10),
                command("codegraph", 20),
                command("test", 30),
            ],
        );
        assert_eq!(score.cost_performance.wall_ms, Some(1_234));
        assert_eq!(score.cost_performance.external_agent_ms, Some(100));
        assert_eq!(score.cost_performance.rg_calls, 2);
        assert_eq!(score.cost_performance.codegraph_calls, 2);
        assert_eq!(score.cost_performance.test_ms, Some(30));
    }

    #[test]
    fn v1_scoring_json_schema_snapshot_validates() {
        let task = task("local_wrong_file_trap_discount");
        let context = context(&task, V1CodeGraphContextScenario::Unavailable);
        let score = score_with(
            &task,
            V1AgentMode::RgPlusCodegraphAgentPatch,
            "blocked_external_agent_missing",
            None,
            "Blocked.",
            &context,
            vec![],
            vec![],
        );
        let value = serde_json::to_value(&score).expect("score JSON");
        validate_v1_patch_outcome_evidence_score_json(&value).expect("valid score JSON");
        assert_eq!(value["patch_outcome"]["patch_applies"], Value::Null);
        assert_eq!(value["public_claim"], false);
        assert_eq!(value["real_agent_patch_quality_claim"], false);
    }
}
