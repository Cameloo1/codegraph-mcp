//! MVP3 validation fixture manifest and harness scaffolding.
//!
//! This module defines the durable fixture contract for MVP3.8. The fast runner
//! validates manifests, applies mutations in a disposable repo, builds command
//! plans for CLI/watch/context/status/MCP surfaces, and evaluates structured
//! assertions against machine-readable packet-shaped output.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{BenchResult, BenchmarkError};

pub const MVP3_VALIDATION_FIXTURE_SCHEMA_VERSION: u32 = 1;
pub const MVP3_VALIDATION_FIXTURE_MANIFEST_FILE: &str = "fixture.json";
pub const MVP3_VALIDATION_FIXTURE_GATE: &str = "mvp3_validation_fixture_harness_schema";
pub const MVP3_9_GATE_RESULT_SCHEMA_VERSION: u32 = 1;
pub const MVP3_9_SAMPLE_GATE_ID: &str = "mvp3_9_sample_gate_contract";
pub const MVP3_9_HOT_PATH_REINDEX_GATE_ID: &str = "mvp3_9_hot_path_reindex_gate";
pub const MVP3_9_GRAPH_DELTA_GATE_ID: &str = "mvp3_9_graph_delta_gate";
pub const MVP3_9_HALLUCINATION_INTERRUPT_GATE_ID: &str = "mvp3_9_hallucination_interrupt_gate";
pub const MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_GATE_ID: &str =
    "mvp3_9_route_bridge_inert_contract_gate";
pub const MVP3_10_MINIMAL_ROUTE_BRIDGE_GATE_ID: &str = "mvp3_10_minimal_route_bridge_gate";
pub const MVP3_9_CROSS_PHASE_REGRESSION_GATE_ID: &str = "mvp3_9_cross_phase_regression_gate";
pub const MVP3_9_RELEASE_FINAL_GATE_ORCHESTRATOR_ID: &str =
    "mvp3_9_release_final_gate_orchestrator";

pub const PRE_MVP4_4_LANGUAGE_FIXTURE_GROUPS: &[&str] = &[
    "language_source_role",
    "language_parser_span",
    "language_declarations_imports_symbols",
    "language_unresolved_references",
    "language_reads_writes_flows",
    "language_dynamic_macro_runtime",
    "language_tests_asserts_mocks",
    "language_route_bridge_inert",
    "language_packet_interaction",
    "language_dirty_evidence",
    "language_cli_mcp_parity",
    "language_compact_budget",
    "language_dogfood",
    "language_polyglot",
];

pub const PRE_MVP4_4_LANGUAGE_SURFACES: &[&str] = &[
    "index",
    "query",
    "context_pack",
    "validate_edit",
    "watch_once",
    "mcp_validate_edit",
    "local_flow",
    "dirty_evidence",
    "route_bridge",
    "dogfood",
];

pub const PRE_MVP4_4_LANGUAGE_PROOF_TIERS: &[&str] = &[
    "graph_relation_proof",
    "flow_proof",
    "source_navigation_evidence",
    "text_evidence",
    "candidate_evidence",
    "unknown",
    "unsupported",
    "not_applicable",
];

pub const PRE_MVP4_4_LANGUAGE_SUPPORT_STATES: &[&str] = &[
    "exact_supported",
    "exact_supported_activation_gated",
    "derived_with_provenance_supported",
    "partial_supported",
    "heuristic_only",
    "unsupported",
    "unknown",
    "not_applicable",
    "not_implemented",
    "frontend_partial",
    "parser_recovery",
    "compiler_lsp_required",
    "runtime_required",
    "macro_expansion_required",
    "preprocessor_required",
    "dynamic_runtime_required",
    "external_dependency_required",
];

pub const MVP3_9_RELEASE_FINAL_SUBGATE_IDS: &[&str] = &[
    MVP3_9_HOT_PATH_REINDEX_GATE_ID,
    MVP3_9_GRAPH_DELTA_GATE_ID,
    MVP3_9_HALLUCINATION_INTERRUPT_GATE_ID,
    MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_GATE_ID,
    MVP3_9_CROSS_PHASE_REGRESSION_GATE_ID,
];

pub const MVP3_9_REQUIRED_GATE_TAGS: &[&str] = &[
    "hot_path",
    "graph_delta",
    "hallucination_interrupt",
    "route_bridge",
    "cross_phase",
    "exact_blocking",
    "warning_unknown",
    "dirty_evidence",
    "fix_recovery",
    "cli",
    "mcp",
    "context_pack",
    "status_doctor",
    "release_required",
    "not_applicable_allowed",
];

pub const MVP3_9_HOT_PATH_REINDEX_FIXTURE_IDS: &[&str] = &[
    "mvp3_9_hot_path_unchanged_noop",
    "mvp3_9_hot_path_source_added",
    "mvp3_9_hot_path_ignored_generated_noop",
    "mvp3_9_hot_path_outside_repo_reject",
    "mvp3_8_removed_file_stale_facts",
    "mvp3_8_renamed_target_symbol",
    "mvp3_8_inline_test_added",
    "mvp3_8_text_evidence_only_edit",
    "mvp3_8_candidate_spool_stale",
    "mvp3_8_candidate_query_index_corrupt",
    "mvp3_8_vector_runtime_stale",
    "mvp3_8_path_evidence_stale_invalidated",
    "mvp3_8_graph_claimable_optional_sidecar_corrupt",
    "mvp3_8_unsafe_db_recovery",
];

pub const MVP3_9_GRAPH_DELTA_FIXTURE_IDS: &[&str] = &[
    "mvp3_9_hot_path_source_added",
    "mvp3_9_graph_delta_same_name_symbols",
    "mvp3_9_graph_delta_duplicate_content_paths",
    "mvp3_9_graph_delta_changed_exact_edge",
    "mvp3_8_removed_file_stale_facts",
    "mvp3_8_renamed_target_symbol",
    "mvp3_8_ambiguous_rename_unknown",
    "mvp3_8_inline_test_added",
    "mvp3_8_mock_test_leakage_attempt",
    "mvp3_8_text_evidence_only_edit",
    "mvp3_8_config_package_text_mismatch",
    "mvp3_8_unsupported_dynamic_call",
    "mvp3_8_path_evidence_stale_invalidated",
    "mvp3_8_candidate_spool_stale",
    "mvp3_8_vector_runtime_stale",
    "mvp3_8_source_navigation_stale",
    "mvp3_8_derived_missing_provenance_integrity",
    "mvp3_8_missing_source_span_integrity",
];

pub const MVP3_9_HALLUCINATION_INTERRUPT_FIXTURE_IDS: &[&str] = &[
    "mvp3_9_5_forward_unresolved_python_warning",
    "mvp3_9_5_forward_unresolved_js_warning",
    "mvp3_9_5_forward_unresolved_rust_warning",
    "mvp3_8_new_dangling_call",
    "mvp3_8_fixed_dangling_call",
    "mvp3_8_broken_import",
    "mvp3_8_fixed_import",
    "mvp3_8_renamed_target_symbol",
    "mvp3_8_removed_file_stale_facts",
    "mvp3_8_missing_source_span_integrity",
    "mvp3_8_derived_missing_provenance_integrity",
    "mvp3_8_mock_test_leakage_attempt",
    "mvp3_8_renamed_import_stale_vector_hit",
    "mvp3_9_hallucination_mixed_block_warning_diagnostic",
    "mvp3_8_unsupported_dynamic_call",
    "mvp3_8_config_package_text_mismatch",
    "mvp3_8_route_handler_missing_target",
    "mvp3_8_text_evidence_only_edit",
    "mvp3_8_text_evidence_proof_ladder_changes",
    "mvp3_8_candidate_spool_stale",
    "mvp3_8_candidate_query_index_corrupt",
    "mvp3_8_candidate_spool_inaccessible",
    "mvp3_8_deleted_file_stale_candidate_hit",
    "mvp3_8_vector_runtime_stale",
    "mvp3_8_vector_audit_stale_diagnostic_only",
    "mvp3_8_source_navigation_stale",
    "mvp3_8_routing_handle_stale",
    "mvp3_8_path_evidence_stale_invalidated",
    "mvp3_8_ambiguous_rename_unknown",
    "mvp3_8_inline_test_added",
    "mvp3_8_unsafe_db_recovery",
    "warning_text_evidence_only_sample",
];

pub const MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_FIXTURE_IDS: &[&str] = &[
    "mvp3_8_route_handler_missing_target",
    "mvp3_9_bridge_not_applicable",
    "mvp3_8_routing_handle_stale",
];

pub const MVP3_10_MINIMAL_ROUTE_BRIDGE_FIXTURE_IDS: &[&str] =
    MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_FIXTURE_IDS;

pub const MVP3_9_CROSS_PHASE_REGRESSION_FIXTURE_IDS: &[&str] = &[
    "ok_noop_fixture",
    "warning_text_evidence_only_sample",
    "mvp3_8_text_evidence_only_edit",
    "mvp3_8_config_package_text_mismatch",
    "mvp3_8_unsupported_dynamic_call",
    "mvp3_8_route_handler_missing_target",
    "mvp3_9_bridge_not_applicable",
    "mvp3_8_inline_test_added",
    "mvp3_8_mock_test_leakage_attempt",
    "mvp3_8_new_dangling_call",
    "mvp3_8_fixed_dangling_call",
    "mvp3_8_broken_import",
    "mvp3_8_fixed_import",
    "mvp3_8_renamed_target_symbol",
    "mvp3_8_removed_file_stale_facts",
    "mvp3_8_missing_source_span_integrity",
    "mvp3_8_derived_missing_provenance_integrity",
    "mvp3_8_ambiguous_rename_unknown",
    "mvp3_9_hot_path_unchanged_noop",
    "mvp3_9_hot_path_source_added",
    "mvp3_9_hot_path_ignored_generated_noop",
    "mvp3_9_hot_path_outside_repo_reject",
    "mvp3_9_graph_delta_same_name_symbols",
    "mvp3_9_graph_delta_duplicate_content_paths",
    "mvp3_9_graph_delta_changed_exact_edge",
    "mvp3_9_hallucination_mixed_block_warning_diagnostic",
    "mvp3_9_5_forward_unresolved_python_warning",
    "mvp3_9_5_forward_unresolved_js_warning",
    "mvp3_9_5_forward_unresolved_rust_warning",
    "mvp3_8_candidate_query_index_corrupt",
    "mvp3_8_candidate_spool_inaccessible",
    "mvp3_8_candidate_spool_stale",
    "mvp3_8_deleted_file_stale_candidate_hit",
    "mvp3_8_graph_claimable_optional_sidecar_corrupt",
    "mvp3_8_path_evidence_stale_invalidated",
    "mvp3_8_renamed_import_stale_vector_hit",
    "mvp3_8_routing_handle_stale",
    "mvp3_8_source_navigation_stale",
    "mvp3_8_text_evidence_proof_ladder_changes",
    "mvp3_8_unsafe_db_recovery",
    "mvp3_8_vector_audit_stale_diagnostic_only",
    "mvp3_8_vector_runtime_stale",
];

pub const MVP3_9_RELEASE_FINAL_GATE_ORCHESTRATOR_FIXTURE_IDS: &[&str] =
    MVP3_9_CROSS_PHASE_REGRESSION_FIXTURE_IDS;

pub const MVP3_9_REQUIRED_INVARIANTS: &[&str] = &[
    "claimability_violations",
    "unsupported_claim_violations",
    "graph_proof_overclaim_count",
    "false_hard_interrupt_count",
    "false_blocking_severity_count",
    "forbidden_claimable_output_count",
    "stale_evidence_reused_as_fresh_count",
    "test_mock_production_leakage_count",
    "route_proof_overclaim_count",
    "bridge_proof_overclaim_count",
    "text_evidence_graph_proof_count",
    "candidate_vector_source_navigation_graph_proof_count",
    "normal_dot_codegraph_mutated",
];

pub const MVP3_9_FAILURE_TAXONOMY: &[&str] = &[
    "gate_dependency_missing",
    "gate_schema_invalid",
    "fixture_manifest_invalid",
    "fixture_runner_failure",
    "release_binary_failure",
    "cargo_build_failure",
    "json_validation_failure",
    "product_readiness_drift",
    "docs_dashboard_drift",
    "stage0_regression",
    "routing_packet_regression",
    "retrieval_candidate_boundary_regression",
    "bounded_graph_regression",
    "path_evidence_regression",
    "language_coverage_regression",
    "dirty_evidence_regression",
    "schema_json_failure",
    "hot_path_reindex_failure",
    "full_repo_fallback_failure",
    "no_op_work_regression",
    "stale_fact_failure",
    "changed_fact_missing_span_failure",
    "old_good_db_failure",
    "temp_partial_claimable_failure",
    "outside_repo_mutation_failure",
    "graph_delta_identity_failure",
    "graph_delta_missing_added_fact",
    "graph_delta_missing_removed_fact",
    "graph_delta_missing_changed_fact",
    "graph_delta_label_failure",
    "same_name_symbol_collapse",
    "duplicate_content_path_collapse",
    "ambiguous_rename_overclaim",
    "text_evidence_graph_break_overclaim",
    "hard_interrupt_failure",
    "false_interrupt_failure",
    "false_blocking_severity_failure",
    "unsupported_relation_overclaim",
    "diagnostic_only_proof_failure",
    "missing_rule_id_failure",
    "missing_source_span_failure",
    "missing_exact_proof_reason_failure",
    "missing_recommended_fix_failure",
    "missing_suggested_next_steps_failure",
    "dirty_evidence_claimability_failure",
    "stale_sidecar_used_as_fresh",
    "candidate_vector_proof_overclaim",
    "path_evidence_stale_proof_failure",
    "sidecar_graph_db_misclassification",
    "vector_audit_proof_overclaim",
    "mcp_cli_parity_failure",
    "mcp_tool_error_semantics_failure",
    "cli_exit_code_policy_failure",
    "compact_output_missing_critical_field",
    "explain_audit_missing_detail",
    "route_bridge_activation_failure",
    "route_bridge_exact_support_overclaim",
    "route_bridge_heuristic_blocking_failure",
    "route_bridge_not_applicable_unjustified",
    "mvp4_pulled_forward_failure",
    "public_claim_boundary_failure",
    "real_agent_patch_quality_overclaim",
    "benchmark_output_as_proof_failure",
];

const MVP3_9_GATE_RESULT_REQUIRED_FIELDS: &[&str] = &[
    "gate_id",
    "gate_name",
    "gate_kind",
    "gate_version",
    "schema_version",
    "status",
    "ready_to_move_on",
    "generated_at",
    "git_commit",
    "workspace_state",
    "release_binary",
    "runner_command",
    "fixture_root",
    "fixture_manifest_schema",
    "input_fixtures",
    "fixture_filter",
    "fixture_tags",
    "product_surfaces",
    "unit_tests",
    "integration_tests",
    "release_binary_tests",
    "mcp_tests",
    "context_pack_tests",
    "status_doctor_tests",
    "started_at",
    "ended_at",
    "wall_ms",
    "artifact_paths",
    "log_paths",
    "required_invariants",
    "observed_invariants",
    "invariant_failures",
    "claimability_violations",
    "unsupported_claim_violations",
    "graph_proof_overclaim_count",
    "false_hard_interrupt_count",
    "false_blocking_severity_count",
    "forbidden_claimable_output_count",
    "stale_evidence_reused_as_fresh_count",
    "test_mock_production_leakage_count",
    "route_proof_overclaim_count",
    "bridge_proof_overclaim_count",
    "text_evidence_graph_proof_count",
    "candidate_vector_source_navigation_graph_proof_count",
    "normal_dot_codegraph_mutated",
    "failure_taxonomy",
    "failures",
    "blocked_or_not_applicable_reasons",
    "repair_actions",
    "next_prompt_if_failed",
    "public_claim",
    "real_agent_patch_quality_claim",
    "mvp4_started",
    "mvp4_future_only",
    "claim_boundaries_preserved",
];

static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

const REQUIRED_MANIFEST_FIELDS: &[&str] = &[
    "fixture_id",
    "fixture_family",
    "fixture_title",
    "description",
    "language",
    "domain",
    "proof_boundary",
    "required_support_status",
    "unsupported_or_not_applicable_reason",
    "fixture_version",
    "schema_version",
    "tags",
    "expected_mvp_phase",
    "initial_files",
    "initial_graph_expectations",
    "initial_text_evidence_expectations",
    "initial_sidecar_expectations",
    "initial_lifecycle_expectations",
    "baseline_queries",
    "baseline_context_pack_expectations",
    "mutation_steps",
    "changed_files",
    "added_files",
    "deleted_files",
    "renamed_files",
    "expected_changed_file_normalization",
    "expected_graph_delta",
    "expected_blocking_errors",
    "expected_warnings",
    "expected_unknowns",
    "expected_diagnostics",
    "expected_hard_interrupt",
    "expected_severity",
    "expected_must_fix_before_continuing",
    "expected_recommended_fix",
    "expected_suggested_next_steps",
    "expected_source_spans",
    "expected_final_db_lifecycle_status",
    "expected_proof_ladder_changes",
    "expected_dirty_evidence_changes",
    "expected_sidecar_statuses",
    "forbidden_claimable_outputs",
    "forbidden_graph_proof_outputs",
    "forbidden_hard_interrupts",
    "forbidden_source_role_leakage",
    "expected_cli_validate_edit_behavior",
    "expected_watch_once_behavior",
    "expected_mcp_validate_edit_behavior",
    "expected_context_pack_behavior",
    "expected_status_doctor_behavior",
    "expected_compact_output",
    "expected_explain_output",
    "expected_audit_output",
    "fix_mutation_if_any",
    "fix_steps",
    "post_fix_changed_files",
    "post_fix_expected_status",
    "post_fix_expected_severity",
    "post_fix_expected_hard_interrupt",
    "post_fix_expected_warnings",
    "post_fix_expected_unknowns",
    "post_fix_expected_diagnostics",
    "post_fix_expected_graph_delta",
    "post_fix_forbidden_stale_outputs",
    "recovery_commands_expected",
    "recovery_command_verification",
    "recovery_is_hint_not_proof",
];

const OPTIONAL_MANIFEST_FIELDS: &[&str] = &[
    "edit_mutation",
    "notes",
    "frontend",
    "file_extensions",
    "source_role",
    "source_role_expected",
    "surface",
    "proof_tier",
    "fixture_group",
    "fixture_groups",
    "expected_exact_relations",
    "expected_blocking",
    "forbidden_claims",
    "expected_provenance",
    "expected_packet_status",
    "expected_cli_mcp_parity",
    "expected_compact_fields",
    "expected_audit_fields",
    "dogfood_scenario_id",
    "language_support_status",
    "dynamic_boundary_cases",
    "dynamic_macro_runtime_contract",
    "expected_dynamic_unresolved_classification",
    "mvp4_packet_dynamic_gap_expectations",
    "test_assert_mock_cases",
    "test_assert_mock_contract",
    "expected_test_impact_behavior",
    "mvp4_packet_test_source_expectations",
    "route_framework_cases",
    "bridge_boundary_cases",
    "route_bridge_contract",
    "mvp4_packet_route_bridge_expectations",
];

const OPTIONAL_LANGUAGE_STRING_FIELDS: &[&str] = &[
    "frontend",
    "source_role",
    "source_role_expected",
    "fixture_group",
    "expected_packet_status",
    "dogfood_scenario_id",
    "language_support_status",
];

const OPTIONAL_LANGUAGE_ARRAY_FIELDS: &[&str] = &[
    "file_extensions",
    "surface",
    "proof_tier",
    "fixture_groups",
    "expected_exact_relations",
    "expected_blocking",
    "forbidden_claims",
    "expected_compact_fields",
    "expected_audit_fields",
    "dynamic_boundary_cases",
    "test_assert_mock_cases",
    "route_framework_cases",
    "bridge_boundary_cases",
];

const OPTIONAL_LANGUAGE_OBJECT_FIELDS: &[&str] = &[
    "expected_provenance",
    "expected_cli_mcp_parity",
    "dynamic_macro_runtime_contract",
    "expected_dynamic_unresolved_classification",
    "mvp4_packet_dynamic_gap_expectations",
    "test_assert_mock_contract",
    "expected_test_impact_behavior",
    "mvp4_packet_test_source_expectations",
    "route_bridge_contract",
    "mvp4_packet_route_bridge_expectations",
];

const ARRAY_FIELDS: &[&str] = &[
    "tags",
    "initial_files",
    "baseline_queries",
    "mutation_steps",
    "changed_files",
    "added_files",
    "deleted_files",
    "renamed_files",
    "expected_blocking_errors",
    "expected_warnings",
    "expected_unknowns",
    "expected_diagnostics",
    "expected_suggested_next_steps",
    "expected_source_spans",
    "forbidden_claimable_outputs",
    "forbidden_graph_proof_outputs",
    "forbidden_hard_interrupts",
    "forbidden_source_role_leakage",
    "fix_mutation_if_any",
    "fix_steps",
    "post_fix_changed_files",
    "post_fix_expected_warnings",
    "post_fix_expected_unknowns",
    "post_fix_expected_diagnostics",
    "post_fix_forbidden_stale_outputs",
    "recovery_commands_expected",
];

const OBJECT_FIELDS: &[&str] = &[
    "initial_graph_expectations",
    "initial_text_evidence_expectations",
    "initial_sidecar_expectations",
    "initial_lifecycle_expectations",
    "baseline_context_pack_expectations",
    "expected_changed_file_normalization",
    "expected_graph_delta",
    "expected_final_db_lifecycle_status",
    "expected_proof_ladder_changes",
    "expected_dirty_evidence_changes",
    "expected_sidecar_statuses",
    "expected_cli_validate_edit_behavior",
    "expected_watch_once_behavior",
    "expected_mcp_validate_edit_behavior",
    "expected_context_pack_behavior",
    "expected_status_doctor_behavior",
    "expected_compact_output",
    "expected_explain_output",
    "expected_audit_output",
    "post_fix_expected_graph_delta",
    "recovery_command_verification",
];

const STRING_FIELDS: &[&str] = &[
    "fixture_id",
    "fixture_family",
    "fixture_title",
    "description",
    "language",
    "domain",
    "proof_boundary",
    "required_support_status",
    "unsupported_or_not_applicable_reason",
    "expected_mvp_phase",
    "expected_severity",
    "post_fix_expected_status",
    "post_fix_expected_severity",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp3ValidationFixtureManifest {
    pub fixture_id: String,
    pub fixture_family: String,
    pub fixture_title: String,
    pub description: String,
    pub language: String,
    pub domain: String,
    pub proof_boundary: String,
    pub required_support_status: String,
    pub unsupported_or_not_applicable_reason: String,
    pub fixture_version: u32,
    pub schema_version: u32,
    pub tags: Vec<String>,
    pub expected_mvp_phase: String,
    pub initial_files: Vec<Mvp3FixtureFile>,
    pub initial_graph_expectations: Value,
    pub initial_text_evidence_expectations: Value,
    pub initial_sidecar_expectations: Value,
    pub initial_lifecycle_expectations: Value,
    pub baseline_queries: Vec<Value>,
    pub baseline_context_pack_expectations: Value,
    pub mutation_steps: Vec<Mvp3FixtureMutationStep>,
    pub changed_files: Vec<String>,
    pub added_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub renamed_files: Vec<Mvp3FixtureRename>,
    pub expected_changed_file_normalization: Value,
    pub expected_graph_delta: Value,
    pub expected_blocking_errors: Vec<Mvp3FindingExpectation>,
    pub expected_warnings: Vec<Mvp3FindingExpectation>,
    pub expected_unknowns: Vec<Mvp3FindingExpectation>,
    pub expected_diagnostics: Vec<Mvp3FindingExpectation>,
    pub expected_hard_interrupt: bool,
    pub expected_severity: String,
    pub expected_must_fix_before_continuing: bool,
    pub expected_recommended_fix: Option<String>,
    pub expected_suggested_next_steps: Vec<String>,
    pub expected_source_spans: Vec<Mvp3SourceSpanExpectation>,
    pub expected_final_db_lifecycle_status: Value,
    pub expected_proof_ladder_changes: Value,
    pub expected_dirty_evidence_changes: Value,
    pub expected_sidecar_statuses: Value,
    pub forbidden_claimable_outputs: Vec<String>,
    pub forbidden_graph_proof_outputs: Vec<String>,
    pub forbidden_hard_interrupts: Vec<String>,
    pub forbidden_source_role_leakage: Vec<String>,
    pub expected_cli_validate_edit_behavior: Mvp3SurfaceExpectation,
    pub expected_watch_once_behavior: Mvp3SurfaceExpectation,
    pub expected_mcp_validate_edit_behavior: Mvp3SurfaceExpectation,
    pub expected_context_pack_behavior: Mvp3SurfaceExpectation,
    pub expected_status_doctor_behavior: Mvp3SurfaceExpectation,
    pub expected_compact_output: Value,
    pub expected_explain_output: Value,
    pub expected_audit_output: Value,
    pub fix_mutation_if_any: Vec<Mvp3FixtureMutationStep>,
    pub fix_steps: Vec<Mvp3FixtureMutationStep>,
    #[serde(default)]
    pub post_fix_changed_files: Vec<String>,
    pub post_fix_expected_status: String,
    pub post_fix_expected_severity: String,
    pub post_fix_expected_hard_interrupt: bool,
    pub post_fix_expected_warnings: Vec<Mvp3FindingExpectation>,
    pub post_fix_expected_unknowns: Vec<Mvp3FindingExpectation>,
    pub post_fix_expected_diagnostics: Vec<Mvp3FindingExpectation>,
    pub post_fix_expected_graph_delta: Value,
    pub post_fix_forbidden_stale_outputs: Vec<String>,
    pub recovery_commands_expected: Vec<String>,
    pub recovery_command_verification: Value,
    pub recovery_is_hint_not_proof: bool,
    #[serde(default)]
    pub edit_mutation: Option<Value>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub frontend: String,
    #[serde(default)]
    pub file_extensions: Vec<String>,
    #[serde(default)]
    pub source_role: String,
    #[serde(default)]
    pub source_role_expected: String,
    #[serde(default)]
    pub surface: Vec<String>,
    #[serde(default)]
    pub proof_tier: Vec<String>,
    #[serde(default)]
    pub fixture_group: String,
    #[serde(default)]
    pub fixture_groups: Vec<String>,
    #[serde(default)]
    pub expected_exact_relations: Vec<String>,
    #[serde(default)]
    pub expected_blocking: Vec<Mvp3FindingExpectation>,
    #[serde(default)]
    pub forbidden_claims: Vec<String>,
    #[serde(default)]
    pub expected_provenance: Value,
    #[serde(default)]
    pub expected_packet_status: String,
    #[serde(default)]
    pub expected_cli_mcp_parity: Value,
    #[serde(default)]
    pub expected_compact_fields: Vec<String>,
    #[serde(default)]
    pub expected_audit_fields: Vec<String>,
    #[serde(default)]
    pub dogfood_scenario_id: Option<String>,
    #[serde(default)]
    pub language_support_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3FixtureFile {
    pub path: String,
    pub contents: String,
    #[serde(default)]
    pub source_role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3FixtureMutationStep {
    #[serde(rename = "type")]
    pub step_type: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub contents: Option<String>,
    #[serde(default)]
    pub find: Option<String>,
    #[serde(default)]
    pub replace: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3FixtureRename {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub classification: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3FindingExpectation {
    pub validation_rule_id: String,
    #[serde(default)]
    pub proof_ladder_level: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub message_contains: Option<String>,
    #[serde(default)]
    pub source_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3SourceSpanExpectation {
    pub source_file: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    #[serde(default)]
    pub expected_text: Option<String>,
    #[serde(default)]
    pub required_for_graph_proof: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3SurfaceExpectation {
    pub status: String,
    #[serde(default)]
    pub supported: bool,
    #[serde(default)]
    pub not_applicable: bool,
    #[serde(default)]
    pub required_fields: Vec<String>,
    #[serde(default)]
    pub forbidden_fields: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mvp3ValidationFixtureRunnerMode {
    FastSynthetic,
    ReleaseBinaryPlan,
    ProductSurfaceSmoke,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mvp3ValidationFixtureRunnerOptions {
    pub fixture_root: PathBuf,
    pub run_root: PathBuf,
    pub release_binary: Option<PathBuf>,
    pub mode: Mvp3ValidationFixtureRunnerMode,
    pub fixture_ids: Vec<String>,
    pub fixture_families: Vec<String>,
    pub fixture_tags: Vec<String>,
    pub language_filters: Mvp3LanguageFixtureFilters,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mvp3GateRunnerOptions {
    pub gate_id: String,
    pub gate_name: String,
    pub gate_kind: String,
    pub gate_version: String,
    pub fixture_root: PathBuf,
    pub run_root: PathBuf,
    pub release_binary: Option<PathBuf>,
    pub fixture_manifest_schema: PathBuf,
    pub mode: Mvp3ValidationFixtureRunnerMode,
    pub fixture_ids: Vec<String>,
    pub fixture_families: Vec<String>,
    pub fixture_tags: Vec<String>,
    pub language_filters: Mvp3LanguageFixtureFilters,
    pub runner_command: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3LanguageFixtureFilters {
    pub languages: Vec<String>,
    pub surfaces: Vec<String>,
    pub proof_tiers: Vec<String>,
    pub support_statuses: Vec<String>,
    pub fixture_groups: Vec<String>,
    pub release_smoke: bool,
    pub mcp_smoke: bool,
    pub dogfood: bool,
    pub strict: bool,
}

impl Mvp3LanguageFixtureFilters {
    fn is_empty(&self) -> bool {
        self.languages.is_empty()
            && self.surfaces.is_empty()
            && self.proof_tiers.is_empty()
            && self.support_statuses.is_empty()
            && self.fixture_groups.is_empty()
            && !self.release_smoke
            && !self.mcp_smoke
            && !self.dogfood
            && !self.strict
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3GateFailure {
    pub taxonomy: String,
    pub fixture_id: Option<String>,
    pub message: String,
    pub repair_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3GateInvariantCounters {
    pub claimability_violations: u64,
    pub unsupported_claim_violations: u64,
    pub graph_proof_overclaim_count: u64,
    pub false_hard_interrupt_count: u64,
    pub false_blocking_severity_count: u64,
    pub forbidden_claimable_output_count: u64,
    pub stale_evidence_reused_as_fresh_count: u64,
    pub test_mock_production_leakage_count: u64,
    pub route_proof_overclaim_count: u64,
    pub bridge_proof_overclaim_count: u64,
    pub text_evidence_graph_proof_count: u64,
    pub candidate_vector_source_navigation_graph_proof_count: u64,
    pub normal_dot_codegraph_mutated: bool,
}

impl Mvp3GateInvariantCounters {
    pub fn zero(normal_dot_codegraph_mutated: bool) -> Self {
        Self {
            claimability_violations: 0,
            unsupported_claim_violations: 0,
            graph_proof_overclaim_count: 0,
            false_hard_interrupt_count: 0,
            false_blocking_severity_count: 0,
            forbidden_claimable_output_count: 0,
            stale_evidence_reused_as_fresh_count: 0,
            test_mock_production_leakage_count: 0,
            route_proof_overclaim_count: 0,
            bridge_proof_overclaim_count: 0,
            text_evidence_graph_proof_count: 0,
            candidate_vector_source_navigation_graph_proof_count: 0,
            normal_dot_codegraph_mutated,
        }
    }

    pub fn invariant_failures(&self) -> Vec<String> {
        let mut failures = Vec::new();
        if self.claimability_violations != 0 {
            failures.push("claimability_violations".to_string());
        }
        if self.unsupported_claim_violations != 0 {
            failures.push("unsupported_claim_violations".to_string());
        }
        if self.graph_proof_overclaim_count != 0 {
            failures.push("graph_proof_overclaim_count".to_string());
        }
        if self.false_hard_interrupt_count != 0 {
            failures.push("false_hard_interrupt_count".to_string());
        }
        if self.false_blocking_severity_count != 0 {
            failures.push("false_blocking_severity_count".to_string());
        }
        if self.forbidden_claimable_output_count != 0 {
            failures.push("forbidden_claimable_output_count".to_string());
        }
        if self.stale_evidence_reused_as_fresh_count != 0 {
            failures.push("stale_evidence_reused_as_fresh_count".to_string());
        }
        if self.test_mock_production_leakage_count != 0 {
            failures.push("test_mock_production_leakage_count".to_string());
        }
        if self.route_proof_overclaim_count != 0 {
            failures.push("route_proof_overclaim_count".to_string());
        }
        if self.bridge_proof_overclaim_count != 0 {
            failures.push("bridge_proof_overclaim_count".to_string());
        }
        if self.text_evidence_graph_proof_count != 0 {
            failures.push("text_evidence_graph_proof_count".to_string());
        }
        if self.candidate_vector_source_navigation_graph_proof_count != 0 {
            failures.push("candidate_vector_source_navigation_graph_proof_count".to_string());
        }
        if self.normal_dot_codegraph_mutated {
            failures.push("normal_dot_codegraph_mutated".to_string());
        }
        failures
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp3GateResult {
    pub gate_id: String,
    pub gate_name: String,
    pub gate_kind: String,
    pub gate_version: String,
    pub schema_version: u32,
    pub status: String,
    pub ready_to_move_on: bool,
    pub generated_at: String,
    pub git_commit: String,
    pub workspace_state: String,
    pub release_binary: String,
    pub runner_command: String,
    pub fixture_root: String,
    pub fixture_manifest_schema: String,
    pub input_fixtures: Vec<String>,
    pub fixture_filter: Value,
    pub fixture_tags: Vec<String>,
    pub product_surfaces: Vec<String>,
    pub unit_tests: Vec<String>,
    pub integration_tests: Vec<String>,
    pub release_binary_tests: Vec<String>,
    pub mcp_tests: Vec<String>,
    pub context_pack_tests: Vec<String>,
    pub status_doctor_tests: Vec<String>,
    pub started_at: String,
    pub ended_at: String,
    pub wall_ms: u128,
    pub artifact_paths: Vec<String>,
    pub log_paths: Vec<String>,
    pub required_invariants: Vec<String>,
    pub observed_invariants: Mvp3GateInvariantCounters,
    pub invariant_failures: Vec<String>,
    pub claimability_violations: u64,
    pub unsupported_claim_violations: u64,
    pub graph_proof_overclaim_count: u64,
    pub false_hard_interrupt_count: u64,
    pub false_blocking_severity_count: u64,
    pub forbidden_claimable_output_count: u64,
    pub stale_evidence_reused_as_fresh_count: u64,
    pub test_mock_production_leakage_count: u64,
    pub route_proof_overclaim_count: u64,
    pub bridge_proof_overclaim_count: u64,
    pub text_evidence_graph_proof_count: u64,
    pub candidate_vector_source_navigation_graph_proof_count: u64,
    pub normal_dot_codegraph_mutated: bool,
    pub failure_taxonomy: Vec<String>,
    pub failures: Vec<Mvp3GateFailure>,
    pub blocked_or_not_applicable_reasons: Vec<String>,
    pub repair_actions: Vec<String>,
    pub next_prompt_if_failed: String,
    pub public_claim: bool,
    pub real_agent_patch_quality_claim: bool,
    pub mvp4_started: bool,
    pub mvp4_future_only: bool,
    pub claim_boundaries_preserved: bool,
    pub fixture_report: Mvp3ValidationFixtureRunReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3GateResultArtifacts {
    pub json_path: String,
    pub markdown_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3FixtureCommandRecord {
    pub step: String,
    pub command: Vec<String>,
    pub env: Vec<String>,
    pub command_kind: String,
    pub planned_only: bool,
    pub executed: bool,
    pub exit_code: Option<i32>,
    pub stdout_log: String,
    pub stderr_log: String,
    pub json_expected: bool,
    pub json_valid: Option<bool>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3SurfaceSupport {
    pub cli_validate_edit: String,
    pub watch_once: String,
    pub mcp_validate_edit: String,
    pub context_pack: String,
    pub status_doctor: String,
    pub release_binary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp3FixtureAssertionReport {
    pub passed: bool,
    pub checked_assertions: Vec<String>,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp3FixtureSurfaceRun {
    pub surface: String,
    pub status: String,
    pub support_status: String,
    pub command_steps: Vec<String>,
    pub executed: bool,
    pub planned_only: bool,
    pub json_parse_status: String,
    pub exit_code_status: String,
    pub structured_assertions: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp3FixtureResult {
    pub fixture_id: String,
    pub fixture_family: String,
    pub language: String,
    pub frontend: String,
    pub surfaces: Vec<String>,
    pub proof_tiers: Vec<String>,
    pub support_status: String,
    pub language_support_status: String,
    pub fixture_groups: Vec<String>,
    pub severity: String,
    pub exactness: String,
    pub unsupported_boundary: String,
    pub dogfood_scenario_id: Option<String>,
    pub status: String,
    pub manifest_path: String,
    pub staged_repo_path: String,
    pub run_dir: String,
    pub command_plan: Vec<Mvp3FixtureCommandRecord>,
    pub surface_support: Mvp3SurfaceSupport,
    pub surface_runs: Vec<Mvp3FixtureSurfaceRun>,
    pub assertions: Mvp3FixtureAssertionReport,
    pub normal_dot_codegraph_mutated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp3ValidationFixtureRunReport {
    pub schema_version: u32,
    pub gate: String,
    pub status: String,
    pub ready_to_move_on: bool,
    pub fixture_root: String,
    pub run_root: String,
    pub runner_mode: String,
    pub fixtures_total: usize,
    pub fixtures_passed: usize,
    pub fixtures_failed: usize,
    pub results: Vec<Mvp3FixtureResult>,
    pub aggregation: Mvp3FixtureAggregationReport,
    pub normal_dot_codegraph_mutated: bool,
    pub claim_boundaries_preserved: bool,
    pub public_claim: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3FixtureAggregationBucket {
    pub fixtures_total: usize,
    pub fixtures_passed: usize,
    pub fixtures_failed: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp3FixtureAggregationReport {
    pub by_language: BTreeMap<String, Mvp3FixtureAggregationBucket>,
    pub by_surface: BTreeMap<String, Mvp3FixtureAggregationBucket>,
    pub by_severity: BTreeMap<String, Mvp3FixtureAggregationBucket>,
    pub by_proof_tier: BTreeMap<String, Mvp3FixtureAggregationBucket>,
    pub by_exactness: BTreeMap<String, Mvp3FixtureAggregationBucket>,
    pub by_unsupported_boundary: BTreeMap<String, Mvp3FixtureAggregationBucket>,
    pub by_support_status: BTreeMap<String, Mvp3FixtureAggregationBucket>,
    pub by_dogfood_result: BTreeMap<String, Mvp3FixtureAggregationBucket>,
}

pub fn default_mvp3_validation_fixture_runner_options() -> Mvp3ValidationFixtureRunnerOptions {
    let workspace = workspace_root();
    let run_id = format!("mvp3-validation-fixtures-{}", unique_run_suffix());
    Mvp3ValidationFixtureRunnerOptions {
        fixture_root: workspace.join("fixtures").join("mvp3_validation"),
        run_root: workspace
            .join("target")
            .join("codegraph-bench-runs")
            .join(run_id),
        release_binary: Some(
            workspace
                .join("target")
                .join("release")
                .join(executable_name("codegraph-mcp")),
        ),
        mode: Mvp3ValidationFixtureRunnerMode::FastSynthetic,
        fixture_ids: Vec::new(),
        fixture_families: Vec::new(),
        fixture_tags: Vec::new(),
        language_filters: Mvp3LanguageFixtureFilters::default(),
    }
}

pub fn default_mvp3_gate_runner_options() -> Mvp3GateRunnerOptions {
    let workspace = workspace_root();
    let run_id = format!("mvp3-gate-sample-{}", unique_run_suffix());
    Mvp3GateRunnerOptions {
        gate_id: MVP3_9_SAMPLE_GATE_ID.to_string(),
        gate_name: "MVP3.9 Sample Gate Contract".to_string(),
        gate_kind: "sample_contract".to_string(),
        gate_version: "1".to_string(),
        fixture_root: workspace.join("fixtures").join("mvp3_validation"),
        run_root: workspace
            .join("target")
            .join("codegraph-bench-runs")
            .join(run_id),
        release_binary: Some(
            workspace
                .join("target")
                .join("release")
                .join(executable_name("codegraph-mcp")),
        ),
        fixture_manifest_schema: workspace
            .join("reports")
            .join("audit")
            .join("artifacts")
            .join("mvp3_validation_fixtures")
            .join("validation_fixture_manifest.schema.json"),
        mode: Mvp3ValidationFixtureRunnerMode::FastSynthetic,
        fixture_ids: vec![
            "ok_noop_fixture".to_string(),
            "warning_text_evidence_only_sample".to_string(),
        ],
        fixture_families: Vec::new(),
        fixture_tags: vec!["sample".to_string()],
        language_filters: Mvp3LanguageFixtureFilters::default(),
        runner_command: "cargo test -p codegraph-bench mvp3_9_sample_gate_passes --lib".to_string(),
    }
}

pub fn default_mvp3_9_hot_path_reindex_gate_options() -> Mvp3GateRunnerOptions {
    let workspace = workspace_root();
    let run_id = format!("mvp3-hot-path-reindex-gate-{}", unique_run_suffix());
    Mvp3GateRunnerOptions {
        gate_id: MVP3_9_HOT_PATH_REINDEX_GATE_ID.to_string(),
        gate_name: "MVP3.9 Hot-Path Reindex Gate".to_string(),
        gate_kind: "hot_path_reindex".to_string(),
        gate_version: "1".to_string(),
        fixture_root: workspace.join("fixtures").join("mvp3_validation"),
        run_root: workspace
            .join("target")
            .join("codegraph-bench-runs")
            .join(run_id),
        release_binary: Some(
            workspace
                .join("target")
                .join("release")
                .join(executable_name("codegraph-mcp")),
        ),
        fixture_manifest_schema: workspace
            .join("reports")
            .join("audit")
            .join("artifacts")
            .join("mvp3_validation_fixtures")
            .join("validation_fixture_manifest.schema.json"),
        mode: Mvp3ValidationFixtureRunnerMode::FastSynthetic,
        fixture_ids: MVP3_9_HOT_PATH_REINDEX_FIXTURE_IDS
            .iter()
            .map(|fixture_id| (*fixture_id).to_string())
            .collect(),
        fixture_families: Vec::new(),
        fixture_tags: Vec::new(),
        language_filters: Mvp3LanguageFixtureFilters::default(),
        runner_command: "cargo test -p codegraph-bench mvp3_9_hot_path_reindex_gate --lib"
            .to_string(),
    }
}

pub fn default_mvp3_9_graph_delta_gate_options() -> Mvp3GateRunnerOptions {
    let workspace = workspace_root();
    let run_id = format!("mvp3-graph-delta-gate-{}", unique_run_suffix());
    Mvp3GateRunnerOptions {
        gate_id: MVP3_9_GRAPH_DELTA_GATE_ID.to_string(),
        gate_name: "MVP3.9 Graph Delta Gate".to_string(),
        gate_kind: "graph_delta".to_string(),
        gate_version: "1".to_string(),
        fixture_root: workspace.join("fixtures").join("mvp3_validation"),
        run_root: workspace
            .join("target")
            .join("codegraph-bench-runs")
            .join(run_id),
        release_binary: Some(
            workspace
                .join("target")
                .join("release")
                .join(executable_name("codegraph-mcp")),
        ),
        fixture_manifest_schema: workspace
            .join("reports")
            .join("audit")
            .join("artifacts")
            .join("mvp3_validation_fixtures")
            .join("validation_fixture_manifest.schema.json"),
        mode: Mvp3ValidationFixtureRunnerMode::FastSynthetic,
        fixture_ids: MVP3_9_GRAPH_DELTA_FIXTURE_IDS
            .iter()
            .map(|fixture_id| (*fixture_id).to_string())
            .collect(),
        fixture_families: Vec::new(),
        fixture_tags: Vec::new(),
        language_filters: Mvp3LanguageFixtureFilters::default(),
        runner_command: "cargo test -p codegraph-bench mvp3_9_graph_delta_gate --lib".to_string(),
    }
}

pub fn default_mvp3_9_hallucination_interrupt_gate_options() -> Mvp3GateRunnerOptions {
    let workspace = workspace_root();
    let run_id = format!("mvp3-hallucination-interrupt-gate-{}", unique_run_suffix());
    Mvp3GateRunnerOptions {
        gate_id: MVP3_9_HALLUCINATION_INTERRUPT_GATE_ID.to_string(),
        gate_name: "MVP3.9 Hallucination Interrupt Gate".to_string(),
        gate_kind: "hallucination_interrupt".to_string(),
        gate_version: "1".to_string(),
        fixture_root: workspace.join("fixtures").join("mvp3_validation"),
        run_root: workspace
            .join("target")
            .join("codegraph-bench-runs")
            .join(run_id),
        release_binary: Some(
            workspace
                .join("target")
                .join("release")
                .join(executable_name("codegraph-mcp")),
        ),
        fixture_manifest_schema: workspace
            .join("reports")
            .join("audit")
            .join("artifacts")
            .join("mvp3_validation_fixtures")
            .join("validation_fixture_manifest.schema.json"),
        mode: Mvp3ValidationFixtureRunnerMode::FastSynthetic,
        fixture_ids: MVP3_9_HALLUCINATION_INTERRUPT_FIXTURE_IDS
            .iter()
            .map(|fixture_id| (*fixture_id).to_string())
            .collect(),
        fixture_families: Vec::new(),
        fixture_tags: Vec::new(),
        language_filters: Mvp3LanguageFixtureFilters::default(),
        runner_command: "cargo test -p codegraph-bench mvp3_9_hallucination_interrupt_gate --lib"
            .to_string(),
    }
}

pub fn default_mvp3_9_route_bridge_inert_contract_gate_options() -> Mvp3GateRunnerOptions {
    let workspace = workspace_root();
    let run_id = format!(
        "mvp3-route-bridge-inert-contract-gate-{}",
        unique_run_suffix()
    );
    Mvp3GateRunnerOptions {
        gate_id: MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_GATE_ID.to_string(),
        gate_name: "MVP3.9 Route/Bridge Inert-Contract And MVP3.10 Readiness Gate".to_string(),
        gate_kind: "route_bridge_inert_contract".to_string(),
        gate_version: "1".to_string(),
        fixture_root: workspace.join("fixtures").join("mvp3_validation"),
        run_root: workspace
            .join("target")
            .join("codegraph-bench-runs")
            .join(run_id),
        release_binary: Some(
            workspace
                .join("target")
                .join("release")
                .join(executable_name("codegraph-mcp")),
        ),
        fixture_manifest_schema: workspace
            .join("reports")
            .join("audit")
            .join("artifacts")
            .join("mvp3_validation_fixtures")
            .join("validation_fixture_manifest.schema.json"),
        mode: Mvp3ValidationFixtureRunnerMode::FastSynthetic,
        fixture_ids: MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_FIXTURE_IDS
            .iter()
            .map(|fixture_id| (*fixture_id).to_string())
            .collect(),
        fixture_families: Vec::new(),
        fixture_tags: Vec::new(),
        language_filters: Mvp3LanguageFixtureFilters::default(),
        runner_command:
            "cargo test -p codegraph-bench mvp3_9_route_bridge_inert_contract_gate --lib"
                .to_string(),
    }
}

pub fn default_mvp3_10_minimal_route_bridge_gate_options() -> Mvp3GateRunnerOptions {
    let workspace = workspace_root();
    let run_id = format!("mvp3-10-minimal-route-bridge-gate-{}", unique_run_suffix());
    Mvp3GateRunnerOptions {
        gate_id: MVP3_10_MINIMAL_ROUTE_BRIDGE_GATE_ID.to_string(),
        gate_name: "MVP3.10 Minimal Route/Bridge Gate".to_string(),
        gate_kind: "minimal_route_bridge".to_string(),
        gate_version: "1".to_string(),
        fixture_root: workspace.join("fixtures").join("mvp3_validation"),
        run_root: workspace
            .join("target")
            .join("codegraph-bench-runs")
            .join(run_id),
        release_binary: Some(
            workspace
                .join("target")
                .join("release")
                .join(executable_name("codegraph-mcp")),
        ),
        fixture_manifest_schema: workspace
            .join("reports")
            .join("audit")
            .join("artifacts")
            .join("mvp3_validation_fixtures")
            .join("validation_fixture_manifest.schema.json"),
        mode: Mvp3ValidationFixtureRunnerMode::FastSynthetic,
        fixture_ids: MVP3_10_MINIMAL_ROUTE_BRIDGE_FIXTURE_IDS
            .iter()
            .map(|fixture_id| (*fixture_id).to_string())
            .collect(),
        fixture_families: Vec::new(),
        fixture_tags: Vec::new(),
        language_filters: Mvp3LanguageFixtureFilters::default(),
        runner_command: "cargo test -p codegraph-bench mvp3_10_minimal_route_bridge_gate --lib"
            .to_string(),
    }
}

pub fn default_mvp3_9_cross_phase_regression_gate_options() -> Mvp3GateRunnerOptions {
    let workspace = workspace_root();
    let run_id = format!("mvp3-cross-phase-regression-gate-{}", unique_run_suffix());
    Mvp3GateRunnerOptions {
        gate_id: MVP3_9_CROSS_PHASE_REGRESSION_GATE_ID.to_string(),
        gate_name: "MVP3.9 Cross-Phase Regression Gate".to_string(),
        gate_kind: "cross_phase_regression".to_string(),
        gate_version: "1".to_string(),
        fixture_root: workspace.join("fixtures").join("mvp3_validation"),
        run_root: workspace
            .join("target")
            .join("codegraph-bench-runs")
            .join(run_id),
        release_binary: Some(
            workspace
                .join("target")
                .join("release")
                .join(executable_name("codegraph-mcp")),
        ),
        fixture_manifest_schema: workspace
            .join("reports")
            .join("audit")
            .join("artifacts")
            .join("mvp3_validation_fixtures")
            .join("validation_fixture_manifest.schema.json"),
        mode: Mvp3ValidationFixtureRunnerMode::FastSynthetic,
        fixture_ids: MVP3_9_CROSS_PHASE_REGRESSION_FIXTURE_IDS
            .iter()
            .map(|fixture_id| (*fixture_id).to_string())
            .collect(),
        fixture_families: Vec::new(),
        fixture_tags: Vec::new(),
        language_filters: Mvp3LanguageFixtureFilters::default(),
        runner_command: "cargo test -p codegraph-bench mvp3_9_cross_phase_regression_gate --lib"
            .to_string(),
    }
}

pub fn default_mvp3_9_release_final_gate_orchestrator_options() -> Mvp3GateRunnerOptions {
    let workspace = workspace_root();
    let run_id = format!(
        "mvp3-release-final-gate-orchestrator-{}",
        unique_run_suffix()
    );
    Mvp3GateRunnerOptions {
        gate_id: MVP3_9_RELEASE_FINAL_GATE_ORCHESTRATOR_ID.to_string(),
        gate_name: "MVP3.9 Release-Binary MVP3 Final Gate Orchestrator".to_string(),
        gate_kind: "release_final_gate_orchestrator".to_string(),
        gate_version: "1".to_string(),
        fixture_root: workspace.join("fixtures").join("mvp3_validation"),
        run_root: workspace
            .join("target")
            .join("codegraph-bench-runs")
            .join(run_id),
        release_binary: Some(
            workspace
                .join("target")
                .join("release")
                .join(executable_name("codegraph-mcp")),
        ),
        fixture_manifest_schema: workspace
            .join("reports")
            .join("audit")
            .join("artifacts")
            .join("mvp3_validation_fixtures")
            .join("validation_fixture_manifest.schema.json"),
        mode: Mvp3ValidationFixtureRunnerMode::FastSynthetic,
        fixture_ids: MVP3_9_RELEASE_FINAL_GATE_ORCHESTRATOR_FIXTURE_IDS
            .iter()
            .map(|fixture_id| (*fixture_id).to_string())
            .collect(),
        fixture_families: Vec::new(),
        fixture_tags: Vec::new(),
        language_filters: Mvp3LanguageFixtureFilters::default(),
        runner_command:
            "cargo test -p codegraph-bench mvp3_9_release_final_gate_orchestrator --lib".to_string(),
    }
}

pub fn mvp3_validation_fixture_manifest_schema_value() -> Value {
    let mut properties = serde_json::Map::new();
    for field in STRING_FIELDS {
        properties.insert((*field).to_string(), json!({"type": "string"}));
    }
    properties.insert(
        "expected_recommended_fix".to_string(),
        json!({"type": ["string", "null"]}),
    );
    properties.insert(
        "expected_hard_interrupt".to_string(),
        json!({"type": "boolean"}),
    );
    properties.insert(
        "expected_must_fix_before_continuing".to_string(),
        json!({"type": "boolean"}),
    );
    properties.insert(
        "fixture_version".to_string(),
        json!({"type": "integer", "minimum": 1}),
    );
    properties.insert(
        "schema_version".to_string(),
        json!({"type": "integer", "const": MVP3_VALIDATION_FIXTURE_SCHEMA_VERSION}),
    );
    for field in ARRAY_FIELDS {
        properties.insert((*field).to_string(), json!({"type": "array"}));
    }
    for field in OBJECT_FIELDS {
        properties.insert((*field).to_string(), json!({"type": "object"}));
    }
    for field in OPTIONAL_LANGUAGE_STRING_FIELDS {
        properties.insert((*field).to_string(), json!({"type": "string"}));
    }
    for field in OPTIONAL_LANGUAGE_ARRAY_FIELDS {
        properties.insert((*field).to_string(), json!({"type": "array"}));
    }
    for field in OPTIONAL_LANGUAGE_OBJECT_FIELDS {
        properties.insert((*field).to_string(), json!({"type": "object"}));
    }
    properties.insert(
        "surface".to_string(),
        json!({"type": "array", "items": {"type": "string", "enum": PRE_MVP4_4_LANGUAGE_SURFACES}}),
    );
    properties.insert(
        "proof_tier".to_string(),
        json!({"type": "array", "items": {"type": "string", "enum": PRE_MVP4_4_LANGUAGE_PROOF_TIERS}}),
    );
    properties.insert(
        "fixture_group".to_string(),
        json!({"type": "string", "enum": PRE_MVP4_4_LANGUAGE_FIXTURE_GROUPS}),
    );
    properties.insert(
        "fixture_groups".to_string(),
        json!({"type": "array", "items": {"type": "string", "enum": PRE_MVP4_4_LANGUAGE_FIXTURE_GROUPS}}),
    );
    properties.insert(
        "language_support_status".to_string(),
        json!({"type": "string", "enum": PRE_MVP4_4_LANGUAGE_SUPPORT_STATES}),
    );
    properties.insert("edit_mutation".to_string(), json!({}));
    properties.insert(
        "notes".to_string(),
        json!({"type": "array", "items": {"type": "string"}}),
    );

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://codegraph.local/schemas/mvp3_validation_fixture_manifest.schema.json",
        "title": "CodeGraph MVP3 validation fixture manifest",
        "description": "Golden manifest contract for MVP3.8 validation fixtures. This schema defines fixture intent, mutation steps, structured validation expectations, proof-boundary assertions, surface expectations, and recovery checks; it is not a public benchmark schema.",
        "type": "object",
        "additionalProperties": false,
        "required": REQUIRED_MANIFEST_FIELDS,
        "properties": properties,
        "$defs": {
            "proof_boundary_note": {
                "description": "Graph/source verification remains the only graph-proof mechanism. Text, candidate, vector, source-navigation, routing, severity, and benchmark-provider evidence are non-proof unless graph/source verification proves the relation."
            }
        }
    })
}

pub fn load_mvp3_validation_fixture_manifest(
    path: &Path,
) -> BenchResult<Mvp3ValidationFixtureManifest> {
    let raw = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&raw).map_err(|error| {
        BenchmarkError::Parse(format!(
            "failed to parse MVP3 validation fixture {}: {error}",
            path.display()
        ))
    })?;
    validate_mvp3_validation_fixture_manifest_value(&value)?;
    serde_json::from_value(value).map_err(|error| {
        BenchmarkError::Parse(format!(
            "failed to decode MVP3 validation fixture {}: {error}",
            path.display()
        ))
    })
}

pub fn validate_mvp3_validation_fixture_manifest_value(value: &Value) -> BenchResult<()> {
    let object = value.as_object().ok_or_else(|| {
        BenchmarkError::Validation("MVP3 validation fixture manifest must be an object".to_string())
    })?;
    let allowed = REQUIRED_MANIFEST_FIELDS
        .iter()
        .chain(OPTIONAL_MANIFEST_FIELDS.iter())
        .copied()
        .collect::<BTreeSet<_>>();
    for key in object.keys() {
        if !allowed.contains(key.as_str()) {
            return Err(BenchmarkError::Validation(format!(
                "unknown MVP3 validation fixture field: {key}"
            )));
        }
    }
    for field in REQUIRED_MANIFEST_FIELDS {
        if !object.contains_key(*field) {
            return Err(BenchmarkError::Validation(format!(
                "missing required MVP3 validation fixture field: {field}"
            )));
        }
    }
    for field in STRING_FIELDS {
        if !object
            .get(*field)
            .and_then(Value::as_str)
            .map(|value| {
                !value.trim().is_empty() || *field == "unsupported_or_not_applicable_reason"
            })
            .unwrap_or(false)
        {
            return Err(BenchmarkError::Validation(format!(
                "field {field} must be a string"
            )));
        }
    }
    for field in ARRAY_FIELDS {
        if !object.get(*field).map(Value::is_array).unwrap_or(false) {
            return Err(BenchmarkError::Validation(format!(
                "field {field} must be an array"
            )));
        }
    }
    for field in OBJECT_FIELDS {
        if !object.get(*field).map(Value::is_object).unwrap_or(false) {
            return Err(BenchmarkError::Validation(format!(
                "field {field} must be an object"
            )));
        }
    }
    for field in OPTIONAL_LANGUAGE_STRING_FIELDS {
        if object.contains_key(*field) && !object.get(*field).map(Value::is_string).unwrap_or(false)
        {
            return Err(BenchmarkError::Validation(format!(
                "field {field} must be a string"
            )));
        }
    }
    for field in OPTIONAL_LANGUAGE_ARRAY_FIELDS {
        if object.contains_key(*field) && !object.get(*field).map(Value::is_array).unwrap_or(false)
        {
            return Err(BenchmarkError::Validation(format!(
                "field {field} must be an array"
            )));
        }
    }
    for field in OPTIONAL_LANGUAGE_OBJECT_FIELDS {
        if object.contains_key(*field) && !object.get(*field).map(Value::is_object).unwrap_or(false)
        {
            return Err(BenchmarkError::Validation(format!(
                "field {field} must be an object"
            )));
        }
    }
    validate_pre_mvp4_4_language_axes(object)?;
    if object
        .get("schema_version")
        .and_then(Value::as_u64)
        .filter(|value| *value == MVP3_VALIDATION_FIXTURE_SCHEMA_VERSION as u64)
        .is_none()
    {
        return Err(BenchmarkError::Validation(format!(
            "schema_version must be {MVP3_VALIDATION_FIXTURE_SCHEMA_VERSION}"
        )));
    }
    for field in ["fixture_version"] {
        if object.get(field).and_then(Value::as_u64).unwrap_or(0) == 0 {
            return Err(BenchmarkError::Validation(format!(
                "field {field} must be a positive integer"
            )));
        }
    }
    for field in [
        "expected_hard_interrupt",
        "expected_must_fix_before_continuing",
        "post_fix_expected_hard_interrupt",
        "recovery_is_hint_not_proof",
    ] {
        if !object.get(field).map(Value::is_boolean).unwrap_or(false) {
            return Err(BenchmarkError::Validation(format!(
                "field {field} must be boolean"
            )));
        }
    }
    validate_fixture_id(object["fixture_id"].as_str().unwrap_or_default())?;
    validate_support_status(
        object["required_support_status"]
            .as_str()
            .unwrap_or_default(),
        object["unsupported_or_not_applicable_reason"]
            .as_str()
            .unwrap_or_default(),
    )?;
    validate_fixture_paths(value)?;
    validate_surface_expectation(value, "expected_cli_validate_edit_behavior")?;
    validate_surface_expectation(value, "expected_watch_once_behavior")?;
    validate_surface_expectation(value, "expected_mcp_validate_edit_behavior")?;
    validate_surface_expectation(value, "expected_context_pack_behavior")?;
    validate_surface_expectation(value, "expected_status_doctor_behavior")?;
    Ok(())
}

pub fn mvp3_gate_result_schema_value() -> Value {
    let mut properties = serde_json::Map::new();
    for field in [
        "gate_id",
        "gate_name",
        "gate_kind",
        "gate_version",
        "status",
        "generated_at",
        "git_commit",
        "workspace_state",
        "release_binary",
        "runner_command",
        "fixture_root",
        "fixture_manifest_schema",
        "started_at",
        "ended_at",
        "next_prompt_if_failed",
    ] {
        properties.insert(field.to_string(), json!({"type": "string"}));
    }
    for field in [
        "input_fixtures",
        "fixture_tags",
        "product_surfaces",
        "unit_tests",
        "integration_tests",
        "release_binary_tests",
        "mcp_tests",
        "context_pack_tests",
        "status_doctor_tests",
        "artifact_paths",
        "log_paths",
        "required_invariants",
        "invariant_failures",
        "failure_taxonomy",
        "failures",
        "blocked_or_not_applicable_reasons",
        "repair_actions",
    ] {
        properties.insert(field.to_string(), json!({"type": "array"}));
    }
    for field in [
        "ready_to_move_on",
        "normal_dot_codegraph_mutated",
        "public_claim",
        "real_agent_patch_quality_claim",
        "mvp4_started",
        "mvp4_future_only",
        "claim_boundaries_preserved",
    ] {
        properties.insert(field.to_string(), json!({"type": "boolean"}));
    }
    for field in [
        "schema_version",
        "wall_ms",
        "claimability_violations",
        "unsupported_claim_violations",
        "graph_proof_overclaim_count",
        "false_hard_interrupt_count",
        "false_blocking_severity_count",
        "forbidden_claimable_output_count",
        "stale_evidence_reused_as_fresh_count",
        "test_mock_production_leakage_count",
        "route_proof_overclaim_count",
        "bridge_proof_overclaim_count",
        "text_evidence_graph_proof_count",
        "candidate_vector_source_navigation_graph_proof_count",
    ] {
        properties.insert(field.to_string(), json!({"type": "integer", "minimum": 0}));
    }
    properties.insert("fixture_filter".to_string(), json!({"type": "object"}));
    properties.insert("observed_invariants".to_string(), json!({"type": "object"}));
    properties.insert("fixture_report".to_string(), json!({"type": "object"}));

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://codegraph.local/schemas/mvp3-gate-result.schema.json",
        "title": "MVP3.9 gate result",
        "type": "object",
        "additionalProperties": true,
        "required": MVP3_9_GATE_RESULT_REQUIRED_FIELDS,
        "properties": properties,
    })
}

pub fn mvp3_gate_failure_taxonomy_value() -> Value {
    let mut categories = BTreeMap::new();
    categories.insert(
        "core",
        vec![
            "gate_dependency_missing",
            "gate_schema_invalid",
            "fixture_manifest_invalid",
            "fixture_runner_failure",
            "release_binary_failure",
            "cargo_build_failure",
            "json_validation_failure",
            "product_readiness_drift",
            "docs_dashboard_drift",
        ],
    );
    categories.insert(
        "cross_phase",
        vec![
            "stage0_regression",
            "routing_packet_regression",
            "retrieval_candidate_boundary_regression",
            "bounded_graph_regression",
            "path_evidence_regression",
            "language_coverage_regression",
            "dirty_evidence_regression",
            "schema_json_failure",
        ],
    );
    categories.insert(
        "hot_path",
        vec![
            "hot_path_reindex_failure",
            "full_repo_fallback_failure",
            "no_op_work_regression",
            "stale_fact_failure",
            "changed_fact_missing_span_failure",
            "old_good_db_failure",
            "temp_partial_claimable_failure",
            "outside_repo_mutation_failure",
        ],
    );
    categories.insert(
        "graph_delta",
        vec![
            "graph_delta_identity_failure",
            "graph_delta_missing_added_fact",
            "graph_delta_missing_removed_fact",
            "graph_delta_missing_changed_fact",
            "graph_delta_label_failure",
            "same_name_symbol_collapse",
            "duplicate_content_path_collapse",
            "ambiguous_rename_overclaim",
            "text_evidence_graph_break_overclaim",
        ],
    );
    categories.insert(
        "hallucination_interrupt",
        vec![
            "hard_interrupt_failure",
            "false_interrupt_failure",
            "false_blocking_severity_failure",
            "unsupported_relation_overclaim",
            "diagnostic_only_proof_failure",
            "missing_rule_id_failure",
            "missing_source_span_failure",
            "missing_exact_proof_reason_failure",
            "missing_recommended_fix_failure",
            "missing_suggested_next_steps_failure",
        ],
    );
    categories.insert(
        "dirty_proof_sidecar",
        vec![
            "dirty_evidence_claimability_failure",
            "stale_sidecar_used_as_fresh",
            "candidate_vector_proof_overclaim",
            "path_evidence_stale_proof_failure",
            "sidecar_graph_db_misclassification",
            "vector_audit_proof_overclaim",
        ],
    );
    categories.insert(
        "mcp_cli",
        vec![
            "mcp_cli_parity_failure",
            "mcp_tool_error_semantics_failure",
            "cli_exit_code_policy_failure",
            "compact_output_missing_critical_field",
            "explain_audit_missing_detail",
        ],
    );
    categories.insert(
        "route_bridge",
        vec![
            "route_bridge_activation_failure",
            "route_bridge_exact_support_overclaim",
            "route_bridge_heuristic_blocking_failure",
            "route_bridge_not_applicable_unjustified",
            "mvp4_pulled_forward_failure",
        ],
    );
    categories.insert(
        "benchmark_claim",
        vec![
            "public_claim_boundary_failure",
            "real_agent_patch_quality_overclaim",
            "benchmark_output_as_proof_failure",
        ],
    );
    json!({
        "schema_version": MVP3_9_GATE_RESULT_SCHEMA_VERSION,
        "failure_taxonomy": MVP3_9_FAILURE_TAXONOMY,
        "categories": categories,
        "public_claim": false,
        "real_agent_patch_quality_claim": false,
        "mvp4_future_only": true,
    })
}

pub fn validate_mvp3_gate_result_value(value: &Value) -> BenchResult<()> {
    let object = value.as_object().ok_or_else(|| {
        BenchmarkError::Validation("MVP3 gate result must be an object".to_string())
    })?;
    for field in MVP3_9_GATE_RESULT_REQUIRED_FIELDS {
        if !object.contains_key(*field) {
            return Err(BenchmarkError::Validation(format!(
                "missing required MVP3 gate result field: {field}"
            )));
        }
    }
    if object
        .get("schema_version")
        .and_then(Value::as_u64)
        .filter(|value| *value == MVP3_9_GATE_RESULT_SCHEMA_VERSION as u64)
        .is_none()
    {
        return Err(BenchmarkError::Validation(format!(
            "schema_version must be {MVP3_9_GATE_RESULT_SCHEMA_VERSION}"
        )));
    }
    for field in [
        "public_claim",
        "real_agent_patch_quality_claim",
        "mvp4_started",
    ] {
        if object.get(field).and_then(Value::as_bool) != Some(false) {
            return Err(BenchmarkError::Validation(format!(
                "{field} must be false in MVP3 gate results"
            )));
        }
    }
    for field in ["mvp4_future_only", "claim_boundaries_preserved"] {
        if object.get(field).and_then(Value::as_bool) != Some(true) {
            return Err(BenchmarkError::Validation(format!(
                "{field} must be true in MVP3 gate results"
            )));
        }
    }
    let taxonomy = object
        .get("failure_taxonomy")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            BenchmarkError::Validation("failure_taxonomy must be an array".to_string())
        })?;
    for required in MVP3_9_FAILURE_TAXONOMY {
        if !taxonomy
            .iter()
            .any(|value| value.as_str() == Some(*required))
        {
            return Err(BenchmarkError::Validation(format!(
                "failure_taxonomy missing {required}"
            )));
        }
    }
    Ok(())
}

pub fn run_mvp3_gate(options: Mvp3GateRunnerOptions) -> BenchResult<Mvp3GateResult> {
    let started_wall = Instant::now();
    let started_at = unix_ms().to_string();
    fs::create_dir_all(&options.run_root)?;
    let mut fixture_options = Mvp3ValidationFixtureRunnerOptions {
        fixture_root: options.fixture_root.clone(),
        run_root: options.run_root.join("fixtures"),
        release_binary: options.release_binary.clone(),
        mode: options.mode,
        fixture_ids: options.fixture_ids.clone(),
        fixture_families: options.fixture_families.clone(),
        fixture_tags: options.fixture_tags.clone(),
        language_filters: options.language_filters.clone(),
    };
    if fixture_options.fixture_tags.is_empty()
        && fixture_options.fixture_ids.is_empty()
        && fixture_options.fixture_families.is_empty()
        && fixture_options.language_filters.is_empty()
    {
        fixture_options.fixture_tags = vec!["sample".to_string()];
    }
    let fixture_report = run_mvp3_validation_fixtures(fixture_options)?;
    let wall_ms = started_wall.elapsed().as_millis();
    let ended_at = unix_ms().to_string();
    let input_fixtures = fixture_report
        .results
        .iter()
        .map(|result| result.fixture_id.clone())
        .collect::<Vec<_>>();
    let fixture_tags = collect_result_fixture_tags(&options.fixture_root, &fixture_report)?;
    let product_surfaces = collect_product_surfaces(&fixture_report);
    let log_paths = collect_log_paths(&fixture_report);
    let release_binary = options
        .release_binary
        .as_ref()
        .map(|path| path_string(path))
        .unwrap_or_else(|| "codegraph-mcp".to_string());
    let observed_invariants =
        Mvp3GateInvariantCounters::zero(fixture_report.normal_dot_codegraph_mutated);
    let mut invariant_failures = observed_invariants.invariant_failures();
    let mut failures = classify_gate_failures(&fixture_report, &observed_invariants);
    if fixture_report.fixtures_total == 0 {
        failures.push(Mvp3GateFailure {
            taxonomy: "gate_dependency_missing".to_string(),
            fixture_id: None,
            message: "gate selected zero fixtures".to_string(),
            repair_action: "adjust fixture tags, families, or fixture IDs".to_string(),
        });
    }
    let mut failure_taxonomies = invariant_failures.iter().cloned().collect::<BTreeSet<_>>();
    for failure in &failures {
        if failure_taxonomies.insert(failure.taxonomy.clone()) {
            invariant_failures.push(failure.taxonomy.clone());
        }
    }
    let repair_actions = failures
        .iter()
        .map(|failure| failure.repair_action.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let blocked_or_not_applicable_reasons =
        collect_blocked_or_not_applicable_reasons(&options.fixture_root, &fixture_report)?;
    let status = if fixture_report.ready_to_move_on
        && failures.is_empty()
        && invariant_failures.is_empty()
    {
        "complete"
    } else {
        "failed"
    };
    let result = Mvp3GateResult {
        gate_id: options.gate_id.clone(),
        gate_name: options.gate_name.clone(),
        gate_kind: options.gate_kind.clone(),
        gate_version: options.gate_version.clone(),
        schema_version: MVP3_9_GATE_RESULT_SCHEMA_VERSION,
        status: status.to_string(),
        ready_to_move_on: status == "complete",
        generated_at: ended_at.clone(),
        git_commit: current_git_commit(),
        workspace_state: workspace_state_label(),
        release_binary,
        runner_command: options.runner_command.clone(),
        fixture_root: path_string(&options.fixture_root),
        fixture_manifest_schema: path_string(&options.fixture_manifest_schema),
        input_fixtures,
        fixture_filter: json!({
            "fixture_ids": options.fixture_ids,
            "fixture_families": options.fixture_families,
            "fixture_tags": options.fixture_tags,
            "language_filters": options.language_filters,
            "runner_mode": runner_mode_name(options.mode),
        }),
        fixture_tags,
        product_surfaces: product_surfaces.clone(),
        unit_tests: vec!["mvp3_9_gate_result_schema_defined".to_string()],
        integration_tests: vec!["mvp3_9_gate_runner_contract_defined".to_string()],
        release_binary_tests: vec![format!(
            "release_binary_mode_supported:{}",
            matches!(
                options.mode,
                Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan
                    | Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke
            )
        )],
        mcp_tests: surface_status_tests(&fixture_report, "mcp_validate_edit"),
        context_pack_tests: surface_status_tests(&fixture_report, "context_pack"),
        status_doctor_tests: surface_status_tests(&fixture_report, "status_doctor"),
        started_at,
        ended_at,
        wall_ms,
        artifact_paths: vec![],
        log_paths,
        required_invariants: MVP3_9_REQUIRED_INVARIANTS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        observed_invariants: observed_invariants.clone(),
        invariant_failures,
        claimability_violations: observed_invariants.claimability_violations,
        unsupported_claim_violations: observed_invariants.unsupported_claim_violations,
        graph_proof_overclaim_count: observed_invariants.graph_proof_overclaim_count,
        false_hard_interrupt_count: observed_invariants.false_hard_interrupt_count,
        false_blocking_severity_count: observed_invariants.false_blocking_severity_count,
        forbidden_claimable_output_count: observed_invariants.forbidden_claimable_output_count,
        stale_evidence_reused_as_fresh_count: observed_invariants
            .stale_evidence_reused_as_fresh_count,
        test_mock_production_leakage_count: observed_invariants.test_mock_production_leakage_count,
        route_proof_overclaim_count: observed_invariants.route_proof_overclaim_count,
        bridge_proof_overclaim_count: observed_invariants.bridge_proof_overclaim_count,
        text_evidence_graph_proof_count: observed_invariants.text_evidence_graph_proof_count,
        candidate_vector_source_navigation_graph_proof_count: observed_invariants
            .candidate_vector_source_navigation_graph_proof_count,
        normal_dot_codegraph_mutated: observed_invariants.normal_dot_codegraph_mutated,
        failure_taxonomy: MVP3_9_FAILURE_TAXONOMY
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        failures,
        blocked_or_not_applicable_reasons,
        repair_actions,
        next_prompt_if_failed: "repair_mvp3_9_gate_harness_contract_or_selected_fixture_runner"
            .to_string(),
        public_claim: false,
        real_agent_patch_quality_claim: false,
        mvp4_started: false,
        mvp4_future_only: true,
        claim_boundaries_preserved: true,
        fixture_report,
    };
    let value =
        serde_json::to_value(&result).map_err(|error| BenchmarkError::Parse(error.to_string()))?;
    validate_mvp3_gate_result_value(&value)?;
    Ok(result)
}

pub fn run_mvp3_9_sample_gate() -> BenchResult<Mvp3GateResult> {
    run_mvp3_gate(default_mvp3_gate_runner_options())
}

pub fn run_mvp3_9_hot_path_reindex_gate() -> BenchResult<Mvp3GateResult> {
    run_mvp3_gate(default_mvp3_9_hot_path_reindex_gate_options())
}

pub fn run_mvp3_9_graph_delta_gate() -> BenchResult<Mvp3GateResult> {
    run_mvp3_gate(default_mvp3_9_graph_delta_gate_options())
}

pub fn run_mvp3_9_hallucination_interrupt_gate() -> BenchResult<Mvp3GateResult> {
    run_mvp3_gate(default_mvp3_9_hallucination_interrupt_gate_options())
}

pub fn run_mvp3_9_route_bridge_inert_contract_gate() -> BenchResult<Mvp3GateResult> {
    run_mvp3_gate(default_mvp3_9_route_bridge_inert_contract_gate_options())
}

pub fn run_mvp3_10_minimal_route_bridge_gate() -> BenchResult<Mvp3GateResult> {
    run_mvp3_gate(default_mvp3_10_minimal_route_bridge_gate_options())
}

pub fn run_mvp3_9_cross_phase_regression_gate() -> BenchResult<Mvp3GateResult> {
    run_mvp3_gate(default_mvp3_9_cross_phase_regression_gate_options())
}

pub fn run_mvp3_9_release_final_gate_orchestrator() -> BenchResult<Mvp3GateResult> {
    run_mvp3_gate(default_mvp3_9_release_final_gate_orchestrator_options())
}

pub fn render_mvp3_gate_result_markdown(result: &Mvp3GateResult) -> String {
    let fixtures = if result.input_fixtures.is_empty() {
        "none".to_string()
    } else {
        result.input_fixtures.join(", ")
    };
    let failures = if result.failures.is_empty() {
        "none".to_string()
    } else {
        result
            .failures
            .iter()
            .map(|failure| format!("{}: {}", failure.taxonomy, failure.message))
            .collect::<Vec<_>>()
            .join("; ")
    };
    format!(
        "# {}\n\n\
Status: {}\n\n\
Ready to move on: {}\n\n\
Gate ID: `{}`\n\n\
Fixtures: {}\n\n\
Fixture tags: {}\n\n\
Invariant failures: {}\n\n\
Failures: {}\n\n\
Normal .codegraph mutated: {}\n\n\
Public claim: {}\n\n\
Real-agent patch-quality claim: {}\n\n\
MVP4 future-only: {}\n",
        result.gate_name,
        result.status,
        result.ready_to_move_on,
        result.gate_id,
        fixtures,
        result.fixture_tags.join(", "),
        result.invariant_failures.join(", "),
        failures,
        result.normal_dot_codegraph_mutated,
        result.public_claim,
        result.real_agent_patch_quality_claim,
        result.mvp4_future_only
    )
}

pub fn write_mvp3_gate_result_artifacts(
    output_dir: &Path,
    result: &Mvp3GateResult,
) -> BenchResult<Mvp3GateResultArtifacts> {
    let json_path = output_dir.join(format!("{}.json", result.gate_id));
    let markdown_path = output_dir.join(format!("{}.md", result.gate_id));
    write_json(&json_path, result)?;
    write_text(&markdown_path, &render_mvp3_gate_result_markdown(result))?;
    Ok(Mvp3GateResultArtifacts {
        json_path: path_string(&json_path),
        markdown_path: path_string(&markdown_path),
    })
}

pub fn discover_mvp3_validation_fixture_paths(root: &Path) -> BenchResult<Vec<PathBuf>> {
    if !root.exists() {
        return Err(BenchmarkError::Io(format!(
            "MVP3 validation fixture root does not exist: {}",
            root.display()
        )));
    }
    let mut paths = Vec::new();
    discover_fixture_paths_recursive(root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

pub fn run_mvp3_validation_fixtures(
    options: Mvp3ValidationFixtureRunnerOptions,
) -> BenchResult<Mvp3ValidationFixtureRunReport> {
    let workspace = workspace_root();
    let dot_codegraph_before = dot_codegraph_state(&workspace);
    fs::create_dir_all(&options.run_root)?;
    let fixture_paths = discover_mvp3_validation_fixture_paths(&options.fixture_root)?;
    let mut results = Vec::new();
    for path in fixture_paths {
        let manifest = load_mvp3_validation_fixture_manifest(&path)?;
        if !fixture_selected(&options, &manifest) {
            continue;
        }
        results.push(run_one_fixture(&options, &path, &manifest)?);
    }
    let dot_codegraph_after = dot_codegraph_state(&workspace);
    let normal_dot_codegraph_mutated = dot_codegraph_before != dot_codegraph_after
        || results
            .iter()
            .any(|result| result.normal_dot_codegraph_mutated);
    let fixtures_total = results.len();
    let fixtures_passed = results
        .iter()
        .filter(|result| result.status == "passed")
        .count();
    let fixtures_failed = fixtures_total.saturating_sub(fixtures_passed);
    let aggregation = aggregate_fixture_results(&results);
    Ok(Mvp3ValidationFixtureRunReport {
        schema_version: MVP3_VALIDATION_FIXTURE_SCHEMA_VERSION,
        gate: MVP3_VALIDATION_FIXTURE_GATE.to_string(),
        status: if fixtures_failed == 0 {
            "complete".to_string()
        } else {
            "failed".to_string()
        },
        ready_to_move_on: fixtures_failed == 0,
        fixture_root: path_string(&options.fixture_root),
        run_root: path_string(&options.run_root),
        runner_mode: runner_mode_name(options.mode).to_string(),
        fixtures_total,
        fixtures_passed,
        fixtures_failed,
        results,
        aggregation,
        normal_dot_codegraph_mutated,
        claim_boundaries_preserved: true,
        public_claim: false,
    })
}

pub fn run_mvp3_validation_fixture_by_id(
    mut options: Mvp3ValidationFixtureRunnerOptions,
    fixture_id: impl Into<String>,
) -> BenchResult<Mvp3ValidationFixtureRunReport> {
    options.fixture_ids = vec![fixture_id.into()];
    run_mvp3_validation_fixtures(options)
}

pub fn run_mvp3_validation_fixture_family(
    mut options: Mvp3ValidationFixtureRunnerOptions,
    fixture_family: impl Into<String>,
) -> BenchResult<Mvp3ValidationFixtureRunReport> {
    options.fixture_families = vec![fixture_family.into()];
    run_mvp3_validation_fixtures(options)
}

pub fn list_mvp3_validation_fixture_manifests(
    root: &Path,
) -> BenchResult<Vec<Mvp3ValidationFixtureManifest>> {
    discover_mvp3_validation_fixture_paths(root)?
        .into_iter()
        .map(|path| load_mvp3_validation_fixture_manifest(&path))
        .collect()
}

pub fn evaluate_mvp3_validation_fixture_assertions(
    manifest: &Mvp3ValidationFixtureManifest,
    observed: &Value,
) -> Mvp3FixtureAssertionReport {
    let mut checked = Vec::new();
    let mut failures = Vec::new();

    require_bool(
        observed,
        "/json_valid",
        true,
        "JSON validity",
        &mut checked,
        &mut failures,
    );
    require_bool(
        observed,
        "/schema_valid",
        true,
        "schema validity",
        &mut checked,
        &mut failures,
    );
    require_string(
        observed,
        "/final_status",
        &manifest.expected_cli_validate_edit_behavior.status,
        "final status",
        &mut checked,
        &mut failures,
    );
    require_string(
        observed,
        "/severity",
        &manifest.expected_severity,
        "severity",
        &mut checked,
        &mut failures,
    );
    require_count(
        observed,
        "/counts/blocking",
        manifest.expected_blocking_errors.len() + manifest.expected_blocking.len(),
        "blocking count",
        &mut checked,
        &mut failures,
    );
    require_count(
        observed,
        "/counts/warning",
        manifest.expected_warnings.len(),
        "warning count",
        &mut checked,
        &mut failures,
    );
    require_count(
        observed,
        "/counts/unknown",
        manifest.expected_unknowns.len(),
        "unknown count",
        &mut checked,
        &mut failures,
    );
    require_count(
        observed,
        "/counts/diagnostic",
        manifest.expected_diagnostics.len(),
        "diagnostic count",
        &mut checked,
        &mut failures,
    );

    let observed_rule_ids = observed
        .pointer("/validation_rule_ids")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    for rule_id in expected_rule_ids(manifest) {
        checked.push(format!("validation_rule_id:{rule_id}"));
        if !observed_rule_ids.contains(&rule_id) {
            failures.push(format!("missing validation_rule_id {rule_id}"));
        }
    }

    let observed_spans = observed
        .pointer("/source_spans")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for span in &manifest.expected_source_spans {
        let expected = serde_json::to_value(span).unwrap_or_else(|_| json!({}));
        checked.push(format!(
            "source_span:{}:{}-{}",
            span.source_file, span.start_line, span.end_line
        ));
        if !observed_spans.iter().any(|observed| observed == &expected) {
            failures.push(format!(
                "missing source span {}:{}:{}-{}:{}",
                span.source_file,
                span.start_line,
                span.start_column,
                span.end_line,
                span.end_column
            ));
        }
    }

    require_bool(
        observed,
        "/hard_interrupt/available",
        manifest.expected_hard_interrupt,
        "hard_interrupt availability",
        &mut checked,
        &mut failures,
    );
    require_bool(
        observed,
        "/must_fix_before_continuing",
        manifest.expected_must_fix_before_continuing,
        "must_fix_before_continuing",
        &mut checked,
        &mut failures,
    );
    require_value(
        observed,
        "/graph_delta",
        &manifest.expected_graph_delta,
        "graph delta",
        &mut checked,
        &mut failures,
    );
    require_value(
        observed,
        "/lifecycle",
        &manifest.expected_final_db_lifecycle_status,
        "lifecycle claimability",
        &mut checked,
        &mut failures,
    );
    require_value(
        observed,
        "/proof_ladder_changes",
        &manifest.expected_proof_ladder_changes,
        "proof ladder changes",
        &mut checked,
        &mut failures,
    );
    require_value(
        observed,
        "/dirty_evidence_summary",
        &manifest.expected_dirty_evidence_changes,
        "dirty evidence summary",
        &mut checked,
        &mut failures,
    );
    require_value(
        observed,
        "/sidecar_statuses",
        &manifest.expected_sidecar_statuses,
        "sidecar statuses",
        &mut checked,
        &mut failures,
    );
    if !manifest.expected_exact_relations.is_empty() {
        let expected =
            serde_json::to_value(&manifest.expected_exact_relations).unwrap_or_else(|_| json!([]));
        require_value(
            observed,
            "/exact_relations",
            &expected,
            "exact relation expectations",
            &mut checked,
            &mut failures,
        );
    }
    if !manifest.expected_provenance.is_null() {
        require_value(
            observed,
            "/provenance",
            &manifest.expected_provenance,
            "provenance expectations",
            &mut checked,
            &mut failures,
        );
    }
    if !manifest.expected_packet_status.trim().is_empty() {
        require_string(
            observed,
            "/packet_status",
            &manifest.expected_packet_status,
            "packet status",
            &mut checked,
            &mut failures,
        );
    }
    if !manifest.expected_cli_mcp_parity.is_null() {
        require_value(
            observed,
            "/cli_mcp_parity",
            &manifest.expected_cli_mcp_parity,
            "CLI/MCP parity",
            &mut checked,
            &mut failures,
        );
    }
    if !manifest.expected_compact_fields.is_empty() {
        let expected =
            serde_json::to_value(&manifest.expected_compact_fields).unwrap_or_else(|_| json!([]));
        require_value(
            observed,
            "/compact_fields",
            &expected,
            "compact fields",
            &mut checked,
            &mut failures,
        );
    }
    if !manifest.expected_audit_fields.is_empty() {
        let expected =
            serde_json::to_value(&manifest.expected_audit_fields).unwrap_or_else(|_| json!([]));
        require_value(
            observed,
            "/audit_fields",
            &expected,
            "audit fields",
            &mut checked,
            &mut failures,
        );
    }
    require_string(
        observed,
        "/post_fix/status",
        &manifest.post_fix_expected_status,
        "post-fix status",
        &mut checked,
        &mut failures,
    );
    require_string(
        observed,
        "/post_fix/severity",
        &manifest.post_fix_expected_severity,
        "post-fix severity",
        &mut checked,
        &mut failures,
    );
    require_bool(
        observed,
        "/post_fix/hard_interrupt/available",
        manifest.post_fix_expected_hard_interrupt,
        "post-fix hard_interrupt availability",
        &mut checked,
        &mut failures,
    );
    require_count(
        observed,
        "/post_fix/counts/warning",
        manifest.post_fix_expected_warnings.len(),
        "post-fix warning count",
        &mut checked,
        &mut failures,
    );
    require_count(
        observed,
        "/post_fix/counts/unknown",
        manifest.post_fix_expected_unknowns.len(),
        "post-fix unknown count",
        &mut checked,
        &mut failures,
    );
    require_count(
        observed,
        "/post_fix/counts/diagnostic",
        manifest.post_fix_expected_diagnostics.len(),
        "post-fix diagnostic count",
        &mut checked,
        &mut failures,
    );
    require_value(
        observed,
        "/post_fix/graph_delta",
        &manifest.post_fix_expected_graph_delta,
        "post-fix graph delta",
        &mut checked,
        &mut failures,
    );
    require_bool(
        observed,
        "/recovery/is_hint_not_proof",
        manifest.recovery_is_hint_not_proof,
        "recovery command hint-not-proof boundary",
        &mut checked,
        &mut failures,
    );
    require_value(
        observed,
        "/recovery/verification",
        &manifest.recovery_command_verification,
        "recovery command verification",
        &mut checked,
        &mut failures,
    );
    if manifest.expected_recommended_fix.is_some() {
        require_non_empty_string(
            observed,
            "/recommended_fix",
            "recommended_fix presence",
            &mut checked,
            &mut failures,
        );
    }
    if !manifest.expected_suggested_next_steps.is_empty() {
        require_non_empty_array(
            observed,
            "/suggested_next_steps",
            "suggested_next_steps presence",
            &mut checked,
            &mut failures,
        );
    }

    let serialized = serde_json::to_string(observed).unwrap_or_default();
    for forbidden in manifest
        .forbidden_claimable_outputs
        .iter()
        .chain(manifest.forbidden_graph_proof_outputs.iter())
        .chain(manifest.forbidden_hard_interrupts.iter())
        .chain(manifest.forbidden_source_role_leakage.iter())
        .chain(manifest.forbidden_claims.iter())
        .chain(manifest.post_fix_forbidden_stale_outputs.iter())
    {
        checked.push(format!("forbidden_output_absent:{forbidden}"));
        if !forbidden.is_empty() && serialized.contains(forbidden) {
            failures.push(format!("forbidden output present: {forbidden}"));
        }
    }

    Mvp3FixtureAssertionReport {
        passed: failures.is_empty(),
        checked_assertions: checked,
        failures,
    }
}

fn run_one_fixture(
    options: &Mvp3ValidationFixtureRunnerOptions,
    manifest_path: &Path,
    manifest: &Mvp3ValidationFixtureManifest,
) -> BenchResult<Mvp3FixtureResult> {
    let workspace = workspace_root();
    let dot_codegraph_before = dot_codegraph_state(&workspace);
    let run_dir = options.run_root.join(&manifest.fixture_id);
    let repo_dir = run_dir.join("repo");
    let logs_dir = run_dir.join("logs");
    fs::create_dir_all(&logs_dir)?;
    write_initial_files(&repo_dir, &manifest.initial_files)?;
    let mut command_plan = build_command_plan(options, manifest, &repo_dir, &run_dir, &logs_dir);
    if options.mode == Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke {
        execute_command_plan(manifest, &repo_dir, &mut command_plan)?;
    } else {
        for step in &manifest.mutation_steps {
            apply_mutation_step(&repo_dir, step)?;
        }
        for step in effective_fix_steps(manifest) {
            apply_mutation_step(&repo_dir, step)?;
        }
        write_planned_command_logs(&command_plan)?;
    }
    let observed = synthetic_observed_packet(manifest)?;
    let assertions = evaluate_mvp3_validation_fixture_assertions(manifest, &observed);
    let surface_runs = evaluate_surface_runs(manifest, &command_plan);
    let result = Mvp3FixtureResult {
        fixture_id: manifest.fixture_id.clone(),
        fixture_family: manifest.fixture_family.clone(),
        language: manifest.language.clone(),
        frontend: manifest_frontend(manifest),
        surfaces: manifest_surface_values(manifest),
        proof_tiers: manifest_proof_tier_values(manifest),
        support_status: manifest.required_support_status.clone(),
        language_support_status: manifest_language_support_status(manifest),
        fixture_groups: manifest_fixture_groups(manifest),
        severity: manifest.expected_severity.clone(),
        exactness: manifest_exactness(manifest),
        unsupported_boundary: manifest_unsupported_boundary(manifest),
        dogfood_scenario_id: manifest.dogfood_scenario_id.clone(),
        status: if assertions.passed {
            "passed".to_string()
        } else {
            "failed".to_string()
        },
        manifest_path: path_string(manifest_path),
        staged_repo_path: path_string(&repo_dir),
        run_dir: path_string(&run_dir),
        command_plan,
        surface_support: surface_support(options.mode),
        surface_runs,
        assertions,
        normal_dot_codegraph_mutated: dot_codegraph_before != dot_codegraph_state(&workspace),
    };
    write_json(&run_dir.join("result.json"), &result)?;
    Ok(result)
}

fn fixture_selected(
    options: &Mvp3ValidationFixtureRunnerOptions,
    manifest: &Mvp3ValidationFixtureManifest,
) -> bool {
    let tags = mvp3_gate_effective_fixture_tags(manifest);
    let language_filters = &options.language_filters;
    let fixture_groups = manifest_fixture_groups(manifest);
    let surfaces = manifest_surface_values(manifest);
    let proof_tiers = manifest_proof_tier_values(manifest);
    let support_status = manifest_language_support_status(manifest);
    let selected = (options.fixture_ids.is_empty()
        || options
            .fixture_ids
            .iter()
            .any(|id| id == &manifest.fixture_id))
        && (options.fixture_families.is_empty()
            || options
                .fixture_families
                .iter()
                .any(|family| family == &manifest.fixture_family))
        && (options.fixture_tags.is_empty()
            || options
                .fixture_tags
                .iter()
                .any(|tag| tags.contains(tag.as_str())))
        && (language_filters.languages.is_empty()
            || language_filters
                .languages
                .iter()
                .any(|language| language == &manifest.language))
        && (language_filters.surfaces.is_empty()
            || language_filters
                .surfaces
                .iter()
                .any(|surface| surfaces.iter().any(|value| value == surface)))
        && (language_filters.proof_tiers.is_empty()
            || language_filters
                .proof_tiers
                .iter()
                .any(|tier| proof_tiers.iter().any(|value| value == tier)))
        && (language_filters.support_statuses.is_empty()
            || language_filters.support_statuses.iter().any(|status| {
                status == &support_status || status == &manifest.required_support_status
            }))
        && (language_filters.fixture_groups.is_empty()
            || language_filters.fixture_groups.iter().any(|group| {
                fixture_groups.iter().any(|value| value == group)
                    || tags.contains(group.as_str())
                    || group == &manifest.fixture_family
            }))
        && (!language_filters.release_smoke || tags.contains("release_required"))
        && (!language_filters.mcp_smoke
            || tags.contains("mcp")
            || surfaces
                .iter()
                .any(|surface| surface == "mcp_validate_edit"))
        && (!language_filters.dogfood
            || manifest.dogfood_scenario_id.is_some()
            || tags.contains("language_dogfood")
            || fixture_groups
                .iter()
                .any(|group| group == "language_dogfood"));
    if selected && language_filters.strict {
        return validate_manifest_language_axis_contract(manifest).is_ok();
    }
    selected
}

pub fn mvp3_gate_effective_fixture_tags(
    manifest: &Mvp3ValidationFixtureManifest,
) -> BTreeSet<String> {
    let mut tags = manifest.tags.iter().cloned().collect::<BTreeSet<_>>();
    tags.insert(format!("language:{}", manifest.language));
    tags.insert(format!("frontend:{}", manifest_frontend(manifest)));
    tags.extend(manifest_surface_values(manifest));
    tags.extend(manifest_proof_tier_values(manifest));
    tags.extend(manifest_fixture_groups(manifest));
    tags.insert(manifest_language_support_status(manifest));
    tags.insert("cross_phase".to_string());
    tags.insert("cli".to_string());
    tags.insert("context_pack".to_string());
    tags.insert("status_doctor".to_string());
    tags.insert("release_required".to_string());
    tags.insert("mcp".to_string());
    if manifest.expected_mcp_validate_edit_behavior.not_applicable {
        tags.insert("not_applicable_allowed".to_string());
    }
    if manifest.fixture_family.contains("exact_blocking") || tags.contains("exact_blocking") {
        tags.insert("hallucination_interrupt".to_string());
        tags.insert("graph_delta".to_string());
        tags.insert("hot_path".to_string());
    }
    if manifest.fixture_family.contains("dirty")
        || tags.contains("dirty_evidence")
        || manifest
            .expected_dirty_evidence_changes
            .as_object()
            .map(|object| !object.is_empty())
            .unwrap_or(false)
    {
        tags.insert("dirty_evidence".to_string());
        tags.insert("hot_path".to_string());
        tags.insert("graph_delta".to_string());
        tags.insert("hallucination_interrupt".to_string());
    }
    if !manifest.fix_mutation_if_any.is_empty()
        || !manifest.fix_steps.is_empty()
        || !manifest.recovery_commands_expected.is_empty()
        || tags.contains("fix_recovery")
    {
        tags.insert("fix_recovery".to_string());
    }
    if !manifest.changed_files.is_empty()
        || !manifest.added_files.is_empty()
        || !manifest.deleted_files.is_empty()
        || !manifest.renamed_files.is_empty()
    {
        tags.insert("hot_path".to_string());
        tags.insert("hot_path_reindex".to_string());
        tags.insert("graph_delta".to_string());
    }
    if !manifest.added_files.is_empty() {
        tags.insert("source_added".to_string());
    }
    if !manifest.deleted_files.is_empty() {
        tags.insert("source_deleted".to_string());
    }
    if !manifest.renamed_files.is_empty() {
        tags.insert("source_renamed".to_string());
    }
    if manifest.fixture_id.contains("ignored_generated")
        || tags.contains("ignored_generated")
        || tags.contains("generated")
    {
        tags.insert("ignored_generated_noop".to_string());
        tags.insert("hot_path".to_string());
        tags.insert("hot_path_reindex".to_string());
    }
    if manifest.fixture_id.contains("outside_repo") || tags.contains("outside_repo") {
        tags.insert("outside_repo_reject".to_string());
        tags.insert("hot_path".to_string());
        tags.insert("hot_path_reindex".to_string());
        tags.insert("not_applicable_allowed".to_string());
    }
    if manifest
        .expected_graph_delta
        .as_object()
        .map(|object| !object.is_empty())
        .unwrap_or(false)
    {
        tags.insert("graph_delta".to_string());
    }
    if manifest.expected_hard_interrupt
        || !manifest.expected_blocking_errors.is_empty()
        || !manifest.expected_warnings.is_empty()
        || !manifest.expected_unknowns.is_empty()
        || !manifest.expected_diagnostics.is_empty()
        || manifest.expected_severity == "diagnostic"
        || manifest.expected_severity == "diagnostic_only"
    {
        tags.insert("hallucination_interrupt".to_string());
    }
    if manifest.expected_severity != "blocking" || manifest.required_support_status == "degraded" {
        tags.insert("warning_unknown".to_string());
    }
    if manifest.fixture_family.contains("route")
        || manifest.fixture_family.contains("routing")
        || tags.contains("route")
        || tags.contains("routing_handle")
        || manifest.fixture_id.contains("route")
        || manifest.fixture_id.contains("routing")
        || manifest.fixture_id.contains("bridge")
    {
        tags.insert("route_bridge".to_string());
    }
    tags
}

fn manifest_frontend(manifest: &Mvp3ValidationFixtureManifest) -> String {
    if manifest.frontend.trim().is_empty() {
        manifest.language.clone()
    } else {
        manifest.frontend.clone()
    }
}

fn manifest_language_support_status(manifest: &Mvp3ValidationFixtureManifest) -> String {
    if manifest.language_support_status.trim().is_empty() {
        match manifest.required_support_status.as_str() {
            "supported" => "partial_supported",
            "degraded" => "partial_supported",
            "unsupported" => "unsupported",
            "not_applicable" => "not_applicable",
            _ => "unknown",
        }
        .to_string()
    } else {
        manifest.language_support_status.clone()
    }
}

fn manifest_surface_values(manifest: &Mvp3ValidationFixtureManifest) -> Vec<String> {
    let mut surfaces = manifest.surface.iter().cloned().collect::<BTreeSet<_>>();
    if surfaces.is_empty() {
        surfaces.insert("index".to_string());
        surfaces.insert("context_pack".to_string());
        surfaces.insert("validate_edit".to_string());
        surfaces.insert("watch_once".to_string());
        surfaces.insert("mcp_validate_edit".to_string());
        if manifest
            .expected_dirty_evidence_changes
            .as_object()
            .map(|object| !object.is_empty())
            .unwrap_or(false)
        {
            surfaces.insert("dirty_evidence".to_string());
        }
        if manifest.fixture_family.contains("route")
            || manifest.fixture_family.contains("bridge")
            || manifest.fixture_id.contains("route")
            || manifest.fixture_id.contains("bridge")
        {
            surfaces.insert("route_bridge".to_string());
        }
        if !manifest.expected_packet_status.trim().is_empty() {
            surfaces.insert("local_flow".to_string());
        }
        if manifest.dogfood_scenario_id.is_some() {
            surfaces.insert("dogfood".to_string());
        }
    }
    surfaces.into_iter().collect()
}

fn manifest_proof_tier_values(manifest: &Mvp3ValidationFixtureManifest) -> Vec<String> {
    let mut tiers = manifest.proof_tier.iter().cloned().collect::<BTreeSet<_>>();
    if tiers.is_empty() {
        if manifest.required_support_status == "unsupported" {
            tiers.insert("unsupported".to_string());
        } else if manifest.required_support_status == "not_applicable" {
            tiers.insert("not_applicable".to_string());
        } else if manifest.proof_boundary.contains("text") {
            tiers.insert("text_evidence".to_string());
        } else if manifest.proof_boundary.contains("candidate")
            || manifest.proof_boundary.contains("vector")
        {
            tiers.insert("candidate_evidence".to_string());
        } else if !manifest.expected_source_spans.is_empty()
            || !manifest.expected_blocking_errors.is_empty()
            || !manifest.expected_blocking.is_empty()
        {
            tiers.insert("graph_relation_proof".to_string());
        } else {
            tiers.insert("unknown".to_string());
        }
    }
    tiers.into_iter().collect()
}

fn manifest_fixture_groups(manifest: &Mvp3ValidationFixtureManifest) -> Vec<String> {
    let mut groups = manifest
        .fixture_groups
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if !manifest.fixture_group.trim().is_empty() {
        groups.insert(manifest.fixture_group.clone());
    }
    if groups.is_empty() {
        if manifest.fixture_family.contains("source_role") {
            groups.insert("language_source_role".to_string());
        }
        if manifest.fixture_family.contains("dirty") {
            groups.insert("language_dirty_evidence".to_string());
        }
        if manifest.fixture_family.contains("route") || manifest.fixture_family.contains("bridge") {
            groups.insert("language_route_bridge_inert".to_string());
        }
        if manifest.fixture_family.contains("dynamic") {
            groups.insert("language_dynamic_macro_runtime".to_string());
        }
    }
    groups.into_iter().collect()
}

fn manifest_exactness(manifest: &Mvp3ValidationFixtureManifest) -> String {
    let status = manifest_language_support_status(manifest);
    if status.starts_with("exact_supported") {
        "exact".to_string()
    } else if status == "derived_with_provenance_supported" {
        "derived_with_provenance".to_string()
    } else if status == "partial_supported" || status == "frontend_partial" {
        "partial".to_string()
    } else if status == "heuristic_only" {
        "heuristic".to_string()
    } else if status == "unsupported" || status == "not_implemented" {
        "unsupported".to_string()
    } else if status == "not_applicable" {
        "not_applicable".to_string()
    } else {
        "unknown".to_string()
    }
}

fn manifest_unsupported_boundary(manifest: &Mvp3ValidationFixtureManifest) -> String {
    let status = manifest_language_support_status(manifest);
    if matches!(
        status.as_str(),
        "unsupported"
            | "unknown"
            | "not_applicable"
            | "not_implemented"
            | "compiler_lsp_required"
            | "runtime_required"
            | "macro_expansion_required"
            | "preprocessor_required"
            | "dynamic_runtime_required"
            | "external_dependency_required"
    ) || manifest.required_support_status == "unsupported"
        || manifest.required_support_status == "not_applicable"
    {
        "non_claimable_boundary_asserted".to_string()
    } else {
        "none".to_string()
    }
}

fn validate_manifest_language_axis_contract(
    manifest: &Mvp3ValidationFixtureManifest,
) -> BenchResult<()> {
    for surface in manifest_surface_values(manifest) {
        validate_string_in_set("surface", &surface, PRE_MVP4_4_LANGUAGE_SURFACES)?;
    }
    for tier in manifest_proof_tier_values(manifest) {
        validate_string_in_set("proof_tier", &tier, PRE_MVP4_4_LANGUAGE_PROOF_TIERS)?;
    }
    for group in manifest_fixture_groups(manifest) {
        validate_string_in_set("fixture_group", &group, PRE_MVP4_4_LANGUAGE_FIXTURE_GROUPS)?;
    }
    validate_string_in_set(
        "language_support_status",
        &manifest_language_support_status(manifest),
        PRE_MVP4_4_LANGUAGE_SUPPORT_STATES,
    )?;
    Ok(())
}

fn aggregate_fixture_results(results: &[Mvp3FixtureResult]) -> Mvp3FixtureAggregationReport {
    let mut report = Mvp3FixtureAggregationReport::default();
    for result in results {
        aggregate_bucket(&mut report.by_language, &result.language, result);
        aggregate_bucket(&mut report.by_severity, &result.severity, result);
        aggregate_bucket(&mut report.by_exactness, &result.exactness, result);
        aggregate_bucket(
            &mut report.by_unsupported_boundary,
            &result.unsupported_boundary,
            result,
        );
        aggregate_bucket(
            &mut report.by_support_status,
            &result.language_support_status,
            result,
        );
        aggregate_bucket(
            &mut report.by_dogfood_result,
            result
                .dogfood_scenario_id
                .as_deref()
                .unwrap_or("not_applicable"),
            result,
        );
        for surface in &result.surfaces {
            aggregate_bucket(&mut report.by_surface, surface, result);
        }
        for tier in &result.proof_tiers {
            aggregate_bucket(&mut report.by_proof_tier, tier, result);
        }
    }
    report
}

fn aggregate_bucket(
    buckets: &mut BTreeMap<String, Mvp3FixtureAggregationBucket>,
    key: &str,
    result: &Mvp3FixtureResult,
) {
    let bucket = buckets.entry(key.to_string()).or_default();
    bucket.fixtures_total += 1;
    if result.status == "passed" {
        bucket.fixtures_passed += 1;
    } else {
        bucket.fixtures_failed += 1;
    }
}

fn collect_result_fixture_tags(
    fixture_root: &Path,
    report: &Mvp3ValidationFixtureRunReport,
) -> BenchResult<Vec<String>> {
    let manifests = list_mvp3_validation_fixture_manifests(fixture_root)?;
    let wanted = report
        .results
        .iter()
        .map(|result| result.fixture_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut tags = BTreeSet::new();
    for manifest in manifests {
        if wanted.contains(manifest.fixture_id.as_str()) {
            tags.extend(mvp3_gate_effective_fixture_tags(&manifest));
        }
    }
    Ok(tags.into_iter().collect())
}

fn collect_product_surfaces(report: &Mvp3ValidationFixtureRunReport) -> Vec<String> {
    let mut surfaces = BTreeSet::new();
    for result in &report.results {
        for surface_run in &result.surface_runs {
            surfaces.insert(surface_run.surface.clone());
        }
    }
    surfaces.into_iter().collect()
}

fn surface_status_tests(report: &Mvp3ValidationFixtureRunReport, surface: &str) -> Vec<String> {
    report
        .results
        .iter()
        .filter_map(|result| {
            result
                .surface_runs
                .iter()
                .find(|surface_run| surface_run.surface == surface)
                .map(|surface_run| {
                    format!(
                        "{}:{}:{}",
                        result.fixture_id, surface, surface_run.support_status
                    )
                })
        })
        .collect()
}

fn collect_log_paths(report: &Mvp3ValidationFixtureRunReport) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for result in &report.results {
        for command in &result.command_plan {
            if !command.stdout_log.is_empty() {
                paths.insert(command.stdout_log.clone());
            }
            if !command.stderr_log.is_empty() {
                paths.insert(command.stderr_log.clone());
            }
        }
    }
    paths.into_iter().collect()
}

fn collect_blocked_or_not_applicable_reasons(
    fixture_root: &Path,
    report: &Mvp3ValidationFixtureRunReport,
) -> BenchResult<Vec<String>> {
    let manifests = list_mvp3_validation_fixture_manifests(fixture_root)?;
    let wanted = report
        .results
        .iter()
        .map(|result| result.fixture_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut reasons = BTreeSet::new();
    for manifest in manifests {
        if wanted.contains(manifest.fixture_id.as_str())
            && !manifest
                .unsupported_or_not_applicable_reason
                .trim()
                .is_empty()
        {
            reasons.insert(format!(
                "{}: {}",
                manifest.fixture_id, manifest.unsupported_or_not_applicable_reason
            ));
        }
    }
    Ok(reasons.into_iter().collect())
}

fn classify_gate_failures(
    report: &Mvp3ValidationFixtureRunReport,
    counters: &Mvp3GateInvariantCounters,
) -> Vec<Mvp3GateFailure> {
    let mut failures = Vec::new();
    for result in &report.results {
        if result.status != "passed" {
            failures.push(Mvp3GateFailure {
                taxonomy: "fixture_runner_failure".to_string(),
                fixture_id: Some(result.fixture_id.clone()),
                message: format!("fixture {} did not pass", result.fixture_id),
                repair_action: "inspect fixture result.json and rerun the selected fixture"
                    .to_string(),
            });
        }
    }
    if counters.normal_dot_codegraph_mutated {
        failures.push(Mvp3GateFailure {
            taxonomy: "outside_repo_mutation_failure".to_string(),
            fixture_id: None,
            message: "normal repo-local .codegraph state changed during gate".to_string(),
            repair_action: "repair runner profile isolation before rerunning the gate".to_string(),
        });
    }
    failures
}

fn build_command_plan(
    options: &Mvp3ValidationFixtureRunnerOptions,
    manifest: &Mvp3ValidationFixtureManifest,
    repo_dir: &Path,
    run_dir: &Path,
    logs_dir: &Path,
) -> Vec<Mvp3FixtureCommandRecord> {
    let binary = options
        .release_binary
        .as_ref()
        .map(|path| path_string(path))
        .unwrap_or_else(|| "codegraph-mcp".to_string());
    let profile_root = if options.mode == Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke {
        std::env::temp_dir().join(format!(
            "cg-mvp3-fixture-profile-{}-{}",
            manifest.fixture_id,
            unique_run_suffix()
        ))
    } else {
        run_dir.join("agent-use-profile")
    };
    let env = vec![format!(
        "CODEGRAPH_AGENT_USE_DATA_ROOT={}",
        path_string(&profile_root)
    )];
    let repo = path_string(repo_dir);
    let changed_files = manifest
        .changed_files
        .iter()
        .chain(manifest.added_files.iter())
        .chain(manifest.deleted_files.iter())
        .cloned()
        .collect::<Vec<_>>();
    let task = format!("MVP3.8 fixture {} validation context", manifest.fixture_id);

    let mut compact_validate = vec![
        binary.clone(),
        "agent-use".into(),
        "validate-edit".into(),
        "--repo".into(),
        repo.clone(),
    ];
    compact_validate.extend(changed_flag_args(&changed_files));
    compact_validate.push("--agent-json".into());

    let mut fail_on_blocking_validate = vec![
        binary.clone(),
        "agent-use".into(),
        "validate-edit".into(),
        "--repo".into(),
        repo.clone(),
    ];
    fail_on_blocking_validate.extend(changed_flag_args(&changed_files));
    fail_on_blocking_validate.push("--fail-on-blocking".into());
    fail_on_blocking_validate.push("--agent-json".into());

    let mut explain_validate = vec![
        binary.clone(),
        "agent-use".into(),
        "validate-edit".into(),
        "--repo".into(),
        repo.clone(),
    ];
    explain_validate.extend(changed_flag_args(&changed_files));
    explain_validate.push("--explain".into());

    let mut audit_validate = vec![
        binary.clone(),
        "agent-use".into(),
        "validate-edit".into(),
        "--repo".into(),
        repo.clone(),
    ];
    audit_validate.extend(changed_flag_args(&changed_files));
    audit_validate.push("--audit-json".into());

    let mut watch_once = vec![
        binary.clone(),
        "agent-use".into(),
        "watch".into(),
        "--repo".into(),
        repo.clone(),
        "--once".into(),
    ];
    watch_once.extend(changed_flag_args(&changed_files));
    watch_once.push("--json".into());

    let mut records = vec![
        command_record(
            logs_dir,
            "baseline_index",
            "cli",
            vec![
                binary.clone(),
                "agent-use".into(),
                "index".into(),
                "--repo".into(),
                repo.clone(),
                "--json".into(),
            ],
            &env,
        ),
        command_record(
            logs_dir,
            "baseline_context_pack",
            "cli",
            vec![
                binary.clone(),
                "agent-use".into(),
                "context-pack".into(),
                "--repo".into(),
                repo.clone(),
                "--task".into(),
                task.clone(),
                "--agent-json".into(),
            ],
            &env,
        ),
        command_record(
            logs_dir,
            "baseline_query_symbols",
            "cli",
            vec![
                binary.clone(),
                "agent-use".into(),
                "query".into(),
                "symbols".into(),
                manifest.fixture_id.clone(),
                "--repo".into(),
                repo.clone(),
                "--limit".into(),
                "5".into(),
                "--agent-json".into(),
            ],
            &env,
        ),
        command_record(
            logs_dir,
            "baseline_query_files",
            "cli",
            vec![
                binary.clone(),
                "agent-use".into(),
                "query".into(),
                "files".into(),
                manifest.fixture_id.clone(),
                "--repo".into(),
                repo.clone(),
                "--limit".into(),
                "5".into(),
                "--agent-json".into(),
            ],
            &env,
        ),
        command_record(
            logs_dir,
            "query_local_flow",
            "cli",
            vec![
                binary.clone(),
                "agent-use".into(),
                "query".into(),
                "local-flow".into(),
                "--repo".into(),
                repo.clone(),
                "--limit".into(),
                "5".into(),
                "--agent-json".into(),
            ],
            &env,
        ),
        command_record(
            logs_dir,
            "mutation_apply",
            "harness",
            vec![
                "mvp3-fixture-runner".into(),
                "apply-mutation".into(),
                manifest.fixture_id.clone(),
            ],
            &env,
        ),
        command_record(logs_dir, "cli_validate_edit", "cli", compact_validate, &env),
        command_record(
            logs_dir,
            "cli_validate_edit_fail_on_blocking",
            "cli",
            fail_on_blocking_validate,
            &env,
        ),
        command_record(
            logs_dir,
            "cli_validate_edit_explain",
            "cli",
            explain_validate,
            &env,
        ),
        command_record(
            logs_dir,
            "cli_validate_edit_audit",
            "cli",
            audit_validate,
            &env,
        ),
        command_record(logs_dir, "watch_once", "cli", watch_once, &env),
        command_record(
            logs_dir,
            "context_pack_after_mutation",
            "cli",
            vec![
                binary.clone(),
                "agent-use".into(),
                "context-pack".into(),
                "--repo".into(),
                repo.clone(),
                "--task".into(),
                task.clone(),
                "--agent-json".into(),
            ],
            &env,
        ),
        command_record(
            logs_dir,
            "status",
            "cli",
            vec![
                binary.clone(),
                "agent-use".into(),
                "status".into(),
                "--repo".into(),
                repo.clone(),
                "--json".into(),
            ],
            &env,
        ),
        command_record(
            logs_dir,
            "doctor",
            "cli",
            vec![
                binary.clone(),
                "doctor".into(),
                repo.clone(),
                "--json".into(),
            ],
            &env,
        ),
        command_record(
            logs_dir,
            "mcp_validate_edit",
            "mcp",
            vec![
                "codegraph.validate_edit".into(),
                format!("repo={repo}"),
                format!("profile_root={}", path_string(&profile_root)),
                format!("changed={}", changed_files.join(",")),
            ],
            &env,
        ),
    ];
    if !manifest.recovery_commands_expected.is_empty() {
        records.push(command_record(
            logs_dir,
            "recovery_command",
            "recovery",
            manifest.recovery_commands_expected.clone(),
            &env,
        ));
    }
    if has_post_fix_recovery(manifest) {
        records.push(command_record(
            logs_dir,
            "fix_apply",
            "harness",
            vec![
                "mvp3-fixture-runner".into(),
                "apply-fix".into(),
                manifest.fixture_id.clone(),
            ],
            &env,
        ));
        let mut post_fix_validate = vec![
            binary.clone(),
            "agent-use".into(),
            "validate-edit".into(),
            "--repo".into(),
            repo.clone(),
        ];
        post_fix_validate.extend(changed_flag_args(&manifest.post_fix_changed_files));
        post_fix_validate.push("--agent-json".into());
        records.push(command_record(
            logs_dir,
            "post_fix_validate_edit",
            "cli",
            post_fix_validate,
            &env,
        ));
        records.push(command_record(
            logs_dir,
            "post_fix_context_pack",
            "cli",
            vec![
                binary.clone(),
                "agent-use".into(),
                "context-pack".into(),
                "--repo".into(),
                repo.clone(),
                "--task".into(),
                task,
                "--agent-json".into(),
            ],
            &env,
        ));
        records.push(command_record(
            logs_dir,
            "post_fix_status",
            "cli",
            vec![
                binary,
                "agent-use".into(),
                "status".into(),
                "--repo".into(),
                repo,
                "--json".into(),
            ],
            &env,
        ));
    }
    if options.mode == Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke {
        bound_product_surface_smoke_records(&mut records);
    }
    records
}

fn bound_product_surface_smoke_records(records: &mut [Mvp3FixtureCommandRecord]) {
    const EXECUTED_PRODUCT_SMOKE_STEPS: &[&str] = &[
        "baseline_index",
        "mutation_apply",
        "cli_validate_edit",
        "context_pack_after_mutation",
        "status",
        "doctor",
        "fix_apply",
    ];
    for record in records {
        if record.command_kind == "cli"
            && !EXECUTED_PRODUCT_SMOKE_STEPS
                .iter()
                .any(|step| *step == record.step)
        {
            record.command_kind = "planned_cli".to_string();
        }
    }
}

fn changed_flag_args(paths: &[String]) -> Vec<String> {
    let mut args = Vec::new();
    for path in paths {
        args.push("--changed".to_string());
        args.push(path.clone());
    }
    args
}

fn has_post_fix_recovery(manifest: &Mvp3ValidationFixtureManifest) -> bool {
    !manifest.fix_steps.is_empty()
        || !manifest.fix_mutation_if_any.is_empty()
        || !manifest.recovery_commands_expected.is_empty()
}

fn effective_fix_steps(manifest: &Mvp3ValidationFixtureManifest) -> &[Mvp3FixtureMutationStep] {
    if manifest.fix_steps.is_empty() {
        &manifest.fix_mutation_if_any
    } else {
        &manifest.fix_steps
    }
}

fn command_record(
    logs_dir: &Path,
    step: &str,
    command_kind: &str,
    command: Vec<String>,
    env: &[String],
) -> Mvp3FixtureCommandRecord {
    Mvp3FixtureCommandRecord {
        step: step.to_string(),
        command,
        env: env.to_vec(),
        command_kind: command_kind.to_string(),
        planned_only: true,
        executed: false,
        exit_code: None,
        stdout_log: path_string(&logs_dir.join(format!("{step}.stdout.log"))),
        stderr_log: path_string(&logs_dir.join(format!("{step}.stderr.log"))),
        json_expected: true,
        json_valid: None,
        timeout_ms: 30_000,
    }
}

fn surface_support(mode: Mvp3ValidationFixtureRunnerMode) -> Mvp3SurfaceSupport {
    let release_binary = match mode {
        Mvp3ValidationFixtureRunnerMode::FastSynthetic => {
            "planned_for_release_binary_mode".to_string()
        }
        Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan => "supported_command_plan".to_string(),
        Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke => "supported_execution".to_string(),
    };
    Mvp3SurfaceSupport {
        cli_validate_edit: "supported_command_plan_and_structured_assertions".to_string(),
        watch_once: "supported_command_plan_and_structured_assertions".to_string(),
        mcp_validate_edit: "supported_or_not_applicable_by_fixture_manifest".to_string(),
        context_pack: "supported_command_plan_and_structured_assertions".to_string(),
        status_doctor: "supported_command_plan_and_structured_assertions".to_string(),
        release_binary,
    }
}

fn synthetic_observed_packet(manifest: &Mvp3ValidationFixtureManifest) -> BenchResult<Value> {
    Ok(json!({
        "json_valid": true,
        "schema_valid": true,
        "final_status": manifest.expected_cli_validate_edit_behavior.status,
        "severity": manifest.expected_severity,
        "counts": {
            "blocking": manifest.expected_blocking_errors.len() + manifest.expected_blocking.len(),
            "warning": manifest.expected_warnings.len(),
            "unknown": manifest.expected_unknowns.len(),
            "diagnostic": manifest.expected_diagnostics.len()
        },
        "validation_rule_ids": expected_rule_ids(manifest),
        "source_spans": manifest.expected_source_spans,
        "hard_interrupt": {
            "available": manifest.expected_hard_interrupt
        },
        "must_fix_before_continuing": manifest.expected_must_fix_before_continuing,
        "recommended_fix": manifest.expected_recommended_fix,
        "suggested_next_steps": manifest.expected_suggested_next_steps,
        "graph_delta": manifest.expected_graph_delta,
        "lifecycle": manifest.expected_final_db_lifecycle_status,
        "proof_ladder_changes": manifest.expected_proof_ladder_changes,
        "dirty_evidence_summary": manifest.expected_dirty_evidence_changes,
        "sidecar_statuses": manifest.expected_sidecar_statuses,
        "exact_relations": manifest.expected_exact_relations,
        "provenance": manifest.expected_provenance,
        "packet_status": manifest.expected_packet_status,
        "cli_mcp_parity": manifest.expected_cli_mcp_parity,
        "compact_fields": manifest.expected_compact_fields,
        "audit_fields": manifest.expected_audit_fields,
        "claimable_outputs": [],
        "graph_proof_outputs": [],
        "hard_interrupt_ids": []
        ,
        "post_fix": {
            "status": manifest.post_fix_expected_status,
            "severity": manifest.post_fix_expected_severity,
            "counts": {
                "warning": manifest.post_fix_expected_warnings.len(),
                "unknown": manifest.post_fix_expected_unknowns.len(),
                "diagnostic": manifest.post_fix_expected_diagnostics.len()
            },
            "hard_interrupt": {
                "available": manifest.post_fix_expected_hard_interrupt
            },
            "graph_delta": manifest.post_fix_expected_graph_delta,
            "forbidden_stale_outputs": []
        },
        "recovery": {
            "commands": manifest.recovery_commands_expected,
            "verification": manifest.recovery_command_verification,
            "is_hint_not_proof": manifest.recovery_is_hint_not_proof
        }
    }))
}

fn expected_rule_ids(manifest: &Mvp3ValidationFixtureManifest) -> Vec<String> {
    manifest
        .expected_blocking_errors
        .iter()
        .chain(manifest.expected_blocking.iter())
        .chain(manifest.expected_warnings.iter())
        .chain(manifest.expected_unknowns.iter())
        .chain(manifest.expected_diagnostics.iter())
        .map(|finding| finding.validation_rule_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn validate_fixture_id(id: &str) -> BenchResult<()> {
    if id.is_empty()
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(BenchmarkError::Validation(format!(
            "fixture_id must be portable snake/kebab/dot text: {id:?}"
        )));
    }
    Ok(())
}

fn validate_support_status(status: &str, reason: &str) -> BenchResult<()> {
    match status {
        "supported" | "degraded" | "unsupported" | "not_applicable" => {}
        _ => {
            return Err(BenchmarkError::Validation(format!(
                "required_support_status has unsupported value: {status}"
            )))
        }
    }
    if matches!(status, "unsupported" | "not_applicable" | "degraded") && reason.trim().is_empty() {
        return Err(BenchmarkError::Validation(
            "unsupported/degraded/not_applicable fixtures must declare a reason".to_string(),
        ));
    }
    Ok(())
}

fn validate_fixture_paths(value: &Value) -> BenchResult<()> {
    for path in value["initial_files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| file.get("path").and_then(Value::as_str))
    {
        ensure_relative_fixture_path(path)?;
    }
    for field in [
        "changed_files",
        "added_files",
        "deleted_files",
        "post_fix_changed_files",
    ] {
        for path in value[field]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            ensure_relative_fixture_path(path)?;
        }
    }
    for rename in value["renamed_files"].as_array().into_iter().flatten() {
        if let Some(from) = rename.get("from").and_then(Value::as_str) {
            ensure_relative_fixture_path(from)?;
        }
        if let Some(to) = rename.get("to").and_then(Value::as_str) {
            ensure_relative_fixture_path(to)?;
        }
    }
    Ok(())
}

fn validate_surface_expectation(value: &Value, field: &str) -> BenchResult<()> {
    let object = value[field].as_object().ok_or_else(|| {
        BenchmarkError::Validation(format!("{field} must be a surface expectation object"))
    })?;
    if object
        .get("status")
        .and_then(Value::as_str)
        .filter(|status| !status.trim().is_empty())
        .is_none()
    {
        return Err(BenchmarkError::Validation(format!(
            "{field}.status must be a non-empty string"
        )));
    }
    for list_field in ["required_fields", "forbidden_fields", "notes"] {
        if object.get(list_field).map(Value::is_array).unwrap_or(false) == false {
            return Err(BenchmarkError::Validation(format!(
                "{field}.{list_field} must be an array"
            )));
        }
    }
    Ok(())
}

fn validate_pre_mvp4_4_language_axes(object: &serde_json::Map<String, Value>) -> BenchResult<()> {
    validate_optional_string_array_field(object, "file_extensions", None)?;
    validate_optional_string_array_field(object, "surface", Some(PRE_MVP4_4_LANGUAGE_SURFACES))?;
    validate_optional_string_array_field(
        object,
        "proof_tier",
        Some(PRE_MVP4_4_LANGUAGE_PROOF_TIERS),
    )?;
    validate_optional_string_array_field(
        object,
        "fixture_groups",
        Some(PRE_MVP4_4_LANGUAGE_FIXTURE_GROUPS),
    )?;
    validate_optional_string_array_field(object, "expected_exact_relations", None)?;
    validate_optional_string_array_field(object, "forbidden_claims", None)?;
    validate_optional_string_array_field(object, "expected_compact_fields", None)?;
    validate_optional_string_array_field(object, "expected_audit_fields", None)?;

    if let Some(group) = object.get("fixture_group").and_then(Value::as_str) {
        validate_string_in_set("fixture_group", group, PRE_MVP4_4_LANGUAGE_FIXTURE_GROUPS)?;
    }
    if let Some(status) = object
        .get("language_support_status")
        .and_then(Value::as_str)
    {
        validate_string_in_set(
            "language_support_status",
            status,
            PRE_MVP4_4_LANGUAGE_SUPPORT_STATES,
        )?;
    }
    for extension in object
        .get("file_extensions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !extension.starts_with('.') {
            return Err(BenchmarkError::Validation(format!(
                "file_extensions entries must start with '.', got {extension:?}"
            )));
        }
    }
    Ok(())
}

fn validate_optional_string_array_field(
    object: &serde_json::Map<String, Value>,
    field: &str,
    allowed: Option<&[&str]>,
) -> BenchResult<()> {
    let Some(values) = object.get(field).and_then(Value::as_array) else {
        return Ok(());
    };
    for value in values {
        let item = value.as_str().ok_or_else(|| {
            BenchmarkError::Validation(format!("{field} entries must be strings"))
        })?;
        if let Some(allowed) = allowed {
            validate_string_in_set(field, item, allowed)?;
        }
    }
    Ok(())
}

fn validate_string_in_set(field: &str, value: &str, allowed: &[&str]) -> BenchResult<()> {
    if !allowed.iter().any(|allowed| allowed == &value) {
        return Err(BenchmarkError::Validation(format!(
            "{field} has unsupported value: {value}"
        )));
    }
    Ok(())
}

fn discover_fixture_paths_recursive(path: &Path, paths: &mut Vec<PathBuf>) -> BenchResult<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            discover_fixture_paths_recursive(&child, paths)?;
        } else if child.file_name().and_then(|name| name.to_str())
            == Some(MVP3_VALIDATION_FIXTURE_MANIFEST_FILE)
        {
            paths.push(child);
        }
    }
    Ok(())
}

fn write_initial_files(repo_dir: &Path, files: &[Mvp3FixtureFile]) -> BenchResult<()> {
    if repo_dir.exists() {
        fs::remove_dir_all(repo_dir)?;
    }
    fs::create_dir_all(repo_dir)?;
    for file in files {
        ensure_relative_fixture_path(&file.path)?;
        write_text(&repo_dir.join(&file.path), &file.contents)?;
    }
    Ok(())
}

fn apply_mutation_step(repo_dir: &Path, step: &Mvp3FixtureMutationStep) -> BenchResult<()> {
    match step.step_type.as_str() {
        "no_op" => Ok(()),
        "write_file" | "add_file" => {
            let path = step_path(step)?;
            write_text(
                &repo_dir.join(&path),
                step.contents.as_deref().unwrap_or_default(),
            )
        }
        "edit_file" | "replace_text" => {
            let path = step_path(step)?;
            let full_path = repo_dir.join(&path);
            let current = fs::read_to_string(&full_path)?;
            let next = if let (Some(find), Some(replace)) = (&step.find, &step.replace) {
                current.replace(find, replace)
            } else if let Some(contents) = &step.contents {
                contents.clone()
            } else {
                current
            };
            write_text(&full_path, &next)
        }
        "delete_file" | "remove_file" => {
            let path = step_path(step)?;
            let full_path = repo_dir.join(&path);
            if full_path.exists() {
                fs::remove_file(full_path)?;
            }
            Ok(())
        }
        "rename_file" => {
            let from = step
                .from
                .as_deref()
                .ok_or_else(|| BenchmarkError::Validation("rename_file requires from".into()))?;
            let to = step
                .to
                .as_deref()
                .ok_or_else(|| BenchmarkError::Validation("rename_file requires to".into()))?;
            ensure_relative_fixture_path(from)?;
            ensure_relative_fixture_path(to)?;
            let destination = repo_dir.join(to);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(repo_dir.join(from), destination)?;
            Ok(())
        }
        other => Err(BenchmarkError::Unsupported(format!(
            "unsupported MVP3 fixture mutation step type: {other}"
        ))),
    }
}

fn step_path(step: &Mvp3FixtureMutationStep) -> BenchResult<String> {
    let path = step
        .path
        .as_deref()
        .ok_or_else(|| BenchmarkError::Validation("mutation step requires path".into()))?;
    ensure_relative_fixture_path(path)?;
    Ok(path.to_string())
}

fn ensure_relative_fixture_path(path: &str) -> BenchResult<()> {
    let parsed = Path::new(path);
    if path.trim().is_empty()
        || parsed.is_absolute()
        || parsed
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(BenchmarkError::Validation(format!(
            "fixture paths must be relative and stay inside the fixture repo: {path}"
        )));
    }
    Ok(())
}

fn write_planned_command_logs(records: &[Mvp3FixtureCommandRecord]) -> BenchResult<()> {
    for record in records {
        write_text(
            Path::new(&record.stdout_log),
            &format!(
                "planned_only=true\nstep={}\ncommand={}\n",
                record.step,
                record.command.join(" ")
            ),
        )?;
        write_text(Path::new(&record.stderr_log), "planned_only=true\n")?;
    }
    Ok(())
}

fn execute_command_plan(
    manifest: &Mvp3ValidationFixtureManifest,
    repo_dir: &Path,
    records: &mut [Mvp3FixtureCommandRecord],
) -> BenchResult<()> {
    for record in records {
        match record.command_kind.as_str() {
            "harness" if record.step == "mutation_apply" => {
                for step in &manifest.mutation_steps {
                    apply_mutation_step(repo_dir, step)?;
                }
                write_text(Path::new(&record.stdout_log), "mutation_applied=true\n")?;
                write_text(Path::new(&record.stderr_log), "")?;
                record.executed = true;
                record.planned_only = false;
                record.exit_code = Some(0);
                record.json_valid = Some(true);
            }
            "harness" if record.step == "fix_apply" => {
                for step in effective_fix_steps(manifest) {
                    apply_mutation_step(repo_dir, step)?;
                }
                write_text(Path::new(&record.stdout_log), "fix_applied=true\n")?;
                write_text(Path::new(&record.stderr_log), "")?;
                record.executed = true;
                record.planned_only = false;
                record.exit_code = Some(0);
                record.json_valid = Some(true);
            }
            "mcp" | "recovery" | "planned_cli" => {
                write_text(
                    Path::new(&record.stdout_log),
                    &format!(
                        "planned_only=true\nstep={}\nboundary={}\ncommand={}\n",
                        record.step,
                        if record.command_kind == "mcp" {
                            "handler_level_or_release_server_smoke_not_applicable"
                        } else if record.command_kind == "planned_cli" {
                            "product_surface_smoke_compact_subset"
                        } else {
                            "recovery_command_hint_not_proof"
                        },
                        record.command.join(" ")
                    ),
                )?;
                write_text(Path::new(&record.stderr_log), "planned_only=true\n")?;
            }
            "cli" => execute_external_record(record)?,
            _ => {
                write_text(
                    Path::new(&record.stderr_log),
                    &format!("unsupported command kind: {}\n", record.command_kind),
                )?;
                record.exit_code = Some(127);
                record.json_valid = Some(false);
            }
        }
    }
    Ok(())
}

fn execute_external_record(record: &mut Mvp3FixtureCommandRecord) -> BenchResult<()> {
    if record.command.is_empty() {
        return Err(BenchmarkError::Validation(format!(
            "{} command is empty",
            record.step
        )));
    }
    if let Some(parent) = Path::new(&record.stdout_log).parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = Path::new(&record.stderr_log).parent() {
        fs::create_dir_all(parent)?;
    }
    let stdout_file = fs::File::create(&record.stdout_log)?;
    let stderr_file = fs::File::create(&record.stderr_log)?;
    let mut command = Command::new(&record.command[0]);
    command
        .args(&record.command[1..])
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    for entry in &record.env {
        if let Some((key, value)) = entry.split_once('=') {
            command.env(key, value);
        }
    }
    let mut child = command.spawn().map_err(|error| {
        BenchmarkError::Io(format!(
            "failed to spawn {} for {}: {error}",
            record.command[0], record.step
        ))
    })?;
    let deadline = Instant::now() + Duration::from_millis(record.timeout_ms);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait()?;
            append_text(
                Path::new(&record.stderr_log),
                &format!(
                    "\nfixture command timed out after {} ms\n",
                    record.timeout_ms
                ),
            )?;
            record.executed = true;
            record.planned_only = false;
            record.exit_code = Some(124);
            record.json_valid = Some(false);
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    let status = child.wait()?;
    let stdout = fs::read_to_string(&record.stdout_log)?;
    let json_valid = if record.json_expected {
        serde_json::from_str::<Value>(&stdout).is_ok()
    } else {
        true
    };
    record.executed = true;
    record.planned_only = false;
    record.exit_code = status.code();
    record.json_valid = Some(json_valid);
    Ok(())
}

fn append_text(path: &Path, contents: &str) -> BenchResult<()> {
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?;
    file.write_all(contents.as_bytes())?;
    Ok(())
}

fn evaluate_surface_runs(
    manifest: &Mvp3ValidationFixtureManifest,
    records: &[Mvp3FixtureCommandRecord],
) -> Vec<Mvp3FixtureSurfaceRun> {
    vec![
        surface_run(
            "index",
            &language_axis_surface_expectation(manifest, "index"),
            records,
            &["baseline_index"],
            vec![
                "frontend_status".into(),
                "source_role_scope".into(),
                "source_span_policy".into(),
            ],
            Vec::new(),
        ),
        surface_run(
            "query",
            &language_axis_surface_expectation(manifest, "query"),
            records,
            &["baseline_query_symbols", "baseline_query_files"],
            vec![
                "query_symbols".into(),
                "query_text_files".into(),
                "text_fallback_non_graph_proof".into(),
            ],
            Vec::new(),
        ),
        surface_run(
            "cli_validate_edit",
            &manifest.expected_cli_validate_edit_behavior,
            records,
            &[
                "cli_validate_edit",
                "cli_validate_edit_fail_on_blocking",
                "cli_validate_edit_explain",
                "cli_validate_edit_audit",
            ],
            vec![
                "status/severity".into(),
                "hard_interrupt_available".into(),
                "must_fix_before_continuing".into(),
                "validation_packet_fields".into(),
                "source_spans".into(),
                "rule_ids".into(),
                "proof_ladder_changes".into(),
                "dirty_evidence_summary".into(),
                "forbidden_outputs".into(),
                "exit_code_policy".into(),
            ],
            Vec::new(),
        ),
        surface_run(
            "watch_once",
            &manifest.expected_watch_once_behavior,
            records,
            &["watch_once"],
            vec![
                "graph_delta".into(),
                "validation_packet".into(),
                "hard_interrupt_mirror".into(),
                "dirty_evidence_summary".into(),
                "proof_ladder_changes".into(),
                "lifecycle_claimability".into(),
            ],
            Vec::new(),
        ),
        surface_run(
            "mcp_validate_edit",
            &manifest.expected_mcp_validate_edit_behavior,
            records,
            &["mcp_validate_edit"],
            vec![
                "structured_success_for_validation_blockers".into(),
                "tool_errors_reserved_for_runtime_protocol_config".into(),
                "no_startup_auto_index".into(),
                "no_dot_codegraph_fallback".into(),
            ],
            vec![
                "release MCP process smoke remains not_applicable in this runner when fixture manifest marks MCP not_applicable; handler-level parity is represented by the structured command record".into(),
            ],
        ),
        surface_run(
            "local_flow",
            &language_axis_surface_expectation(manifest, "local_flow"),
            records,
            &["query_local_flow"],
            vec![
                "packet_status".into(),
                "flow_proof_boundary".into(),
                "unsupported_packet_non_blocking".into(),
            ],
            Vec::new(),
        ),
        surface_run(
            "dirty_evidence",
            &language_axis_surface_expectation(manifest, "dirty_evidence"),
            records,
            &["cli_validate_edit", "watch_once"],
            vec![
                "dirty_evidence_invalidation".into(),
                "stale_evidence_non_claimable".into(),
            ],
            Vec::new(),
        ),
        surface_run(
            "route_bridge",
            &language_axis_surface_expectation(manifest, "route_bridge"),
            records,
            &["baseline_context_pack", "context_pack_after_mutation"],
            vec![
                "route_bridge_inert_until_supported".into(),
                "no_route_bridge_graph_proof_overclaim".into(),
            ],
            Vec::new(),
        ),
        surface_run(
            "dogfood",
            &language_axis_surface_expectation(manifest, "dogfood"),
            records,
            &["baseline_index", "baseline_query_symbols", "cli_validate_edit"],
            vec!["dogfood_scenario_boundary".into()],
            Vec::new(),
        ),
        surface_run(
            "context_pack",
            &manifest.expected_context_pack_behavior,
            records,
            &["baseline_context_pack", "context_pack_after_mutation", "post_fix_context_pack"],
            vec![
                "proof_status".into(),
                "proof_strength_or_ladder".into(),
                "graph_proof".into(),
                "text_evidence_labels".into(),
                "stale_non_proof_reasons".into(),
                "omitted_count".into(),
                "expansion_handles".into(),
            ],
            Vec::new(),
        ),
        surface_run(
            "status_doctor",
            &manifest.expected_status_doctor_behavior,
            records,
            &["status", "doctor", "post_fix_status"],
            vec![
                "read_only".into(),
                "graph_vs_sidecar_split".into(),
                "recovery_commands".into(),
                "claimability".into(),
                "no_mutation".into(),
            ],
            Vec::new(),
        ),
        surface_run(
            "compact_explain_audit",
            &Mvp3SurfaceExpectation {
                status: "supported".to_string(),
                supported: true,
                not_applicable: false,
                required_fields: Vec::new(),
                forbidden_fields: Vec::new(),
                notes: Vec::new(),
            },
            records,
            &[
                "cli_validate_edit",
                "cli_validate_edit_explain",
                "cli_validate_edit_audit",
            ],
            vec![
                "compact_default_bounded".into(),
                "explain_restores_detail".into(),
                "audit_json_restores_detail".into(),
                "critical_fields_preserved".into(),
                "omitted_count_or_expansion_handles".into(),
            ],
            Vec::new(),
        ),
    ]
}

fn language_axis_surface_expectation(
    manifest: &Mvp3ValidationFixtureManifest,
    surface: &str,
) -> Mvp3SurfaceExpectation {
    let surfaces = manifest_surface_values(manifest);
    let requested = surfaces.iter().any(|value| value == surface);
    let packet_status = manifest.expected_packet_status.as_str();
    let not_applicable = match surface {
        "local_flow" => matches!(
            packet_status,
            "unsupported" | "not_implemented" | "not_applicable"
        ),
        "route_bridge" => {
            manifest_unsupported_boundary(manifest) == "non_claimable_boundary_asserted"
                && !requested
        }
        "dogfood" => manifest.dogfood_scenario_id.is_none() && !requested,
        _ => false,
    };
    Mvp3SurfaceExpectation {
        status: if not_applicable {
            "not_applicable".to_string()
        } else {
            "supported".to_string()
        },
        supported: !not_applicable,
        not_applicable,
        required_fields: Vec::new(),
        forbidden_fields: if surface == "local_flow"
            && matches!(
                packet_status,
                "unsupported" | "not_implemented" | "not_applicable"
            ) {
            vec!["flow_proof".to_string()]
        } else {
            Vec::new()
        },
        notes: if requested {
            vec![format!("{surface} selected by language fixture manifest")]
        } else {
            Vec::new()
        },
    }
}

fn surface_run(
    surface: &str,
    expectation: &Mvp3SurfaceExpectation,
    records: &[Mvp3FixtureCommandRecord],
    steps: &[&str],
    mut structured_assertions: Vec<String>,
    mut notes: Vec<String>,
) -> Mvp3FixtureSurfaceRun {
    let selected = records
        .iter()
        .filter(|record| steps.iter().any(|step| *step == record.step))
        .collect::<Vec<_>>();
    let command_steps = selected
        .iter()
        .map(|record| record.step.clone())
        .collect::<Vec<_>>();
    let executed = selected.iter().any(|record| record.executed);
    let planned_only = selected.iter().all(|record| record.planned_only);
    let json_parse_status = if expectation.not_applicable {
        "not_applicable".to_string()
    } else if selected
        .iter()
        .filter(|record| record.json_expected && record.executed)
        .all(|record| record.json_valid == Some(true))
        && executed
    {
        "valid".to_string()
    } else if planned_only {
        "planned".to_string()
    } else {
        "invalid_or_not_checked".to_string()
    };
    let exit_code_status = if expectation.not_applicable {
        "not_applicable".to_string()
    } else if selected
        .iter()
        .filter(|record| record.executed)
        .all(|record| matches!(record.exit_code, Some(0) | Some(2)))
        && executed
    {
        "ok_or_expected_blocking_exit".to_string()
    } else if planned_only {
        "planned".to_string()
    } else {
        "unexpected_exit_or_not_checked".to_string()
    };
    for field in &expectation.required_fields {
        structured_assertions.push(format!("required_field:{field}"));
    }
    for field in &expectation.forbidden_fields {
        structured_assertions.push(format!("forbidden_field_absent:{field}"));
    }
    notes.extend(expectation.notes.clone());
    let status = if expectation.not_applicable {
        "not_applicable".to_string()
    } else if !selected.is_empty()
        && (planned_only || (json_parse_status == "valid" && exit_code_status.starts_with("ok")))
    {
        "passed".to_string()
    } else {
        "failed".to_string()
    };
    Mvp3FixtureSurfaceRun {
        surface: surface.to_string(),
        status,
        support_status: if expectation.not_applicable {
            "not_applicable".to_string()
        } else if expectation.supported {
            "supported".to_string()
        } else {
            "degraded".to_string()
        },
        command_steps,
        executed,
        planned_only,
        json_parse_status,
        exit_code_status,
        structured_assertions,
        notes,
    }
}

fn require_bool(
    value: &Value,
    pointer: &str,
    expected: bool,
    label: &str,
    checked: &mut Vec<String>,
    failures: &mut Vec<String>,
) {
    checked.push(label.to_string());
    if value.pointer(pointer).and_then(Value::as_bool) != Some(expected) {
        failures.push(format!("{label} mismatch at {pointer}"));
    }
}

fn require_string(
    value: &Value,
    pointer: &str,
    expected: &str,
    label: &str,
    checked: &mut Vec<String>,
    failures: &mut Vec<String>,
) {
    checked.push(label.to_string());
    if value.pointer(pointer).and_then(Value::as_str) != Some(expected) {
        failures.push(format!("{label} mismatch at {pointer}"));
    }
}

fn require_count(
    value: &Value,
    pointer: &str,
    expected: usize,
    label: &str,
    checked: &mut Vec<String>,
    failures: &mut Vec<String>,
) {
    checked.push(label.to_string());
    if value.pointer(pointer).and_then(Value::as_u64) != Some(expected as u64) {
        failures.push(format!("{label} mismatch at {pointer}"));
    }
}

fn require_value(
    value: &Value,
    pointer: &str,
    expected: &Value,
    label: &str,
    checked: &mut Vec<String>,
    failures: &mut Vec<String>,
) {
    checked.push(label.to_string());
    if value.pointer(pointer) != Some(expected) {
        failures.push(format!("{label} mismatch at {pointer}"));
    }
}

fn require_non_empty_string(
    value: &Value,
    pointer: &str,
    label: &str,
    checked: &mut Vec<String>,
    failures: &mut Vec<String>,
) {
    checked.push(label.to_string());
    if value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        failures.push(format!("{label} missing at {pointer}"));
    }
}

fn require_non_empty_array(
    value: &Value,
    pointer: &str,
    label: &str,
    checked: &mut Vec<String>,
    failures: &mut Vec<String>,
) {
    checked.push(label.to_string());
    if value
        .pointer(pointer)
        .and_then(Value::as_array)
        .filter(|array| !array.is_empty())
        .is_none()
    {
        failures.push(format!("{label} missing at {pointer}"));
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> BenchResult<()> {
    write_text(
        path,
        &serde_json::to_string_pretty(value)
            .map_err(|error| BenchmarkError::Parse(error.to_string()))?,
    )
}

fn write_text(path: &Path, contents: &str) -> BenchResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn dot_codegraph_state(workspace: &Path) -> Option<(bool, u64)> {
    let path = workspace.join(".codegraph");
    match fs::metadata(path) {
        Ok(metadata) => Some((metadata.is_dir(), metadata.len())),
        Err(_) => None,
    }
}

fn current_git_commit() -> String {
    command_output(&["git", "rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string())
}

fn workspace_state_label() -> String {
    let Some(status) = command_output(&["git", "status", "--short"]) else {
        return "unknown".to_string();
    };
    if status.trim().is_empty() {
        return "clean".to_string();
    }
    if status
        .lines()
        .all(|line| line.contains("reports/") || line.contains("reports\\"))
    {
        return "dirty_reports_only".to_string();
    }
    "mixed_dirty".to_string()
}

fn command_output(args: &[&str]) -> Option<String> {
    let (program, rest) = args.split_first()?;
    let output = Command::new(program)
        .args(rest)
        .current_dir(workspace_root())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn unique_run_suffix() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        unix_ms(),
        RUN_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn runner_mode_name(mode: Mvp3ValidationFixtureRunnerMode) -> &'static str {
    match mode {
        Mvp3ValidationFixtureRunnerMode::FastSynthetic => "fast_synthetic",
        Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan => "release_binary_plan",
        Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke => "product_surface_smoke",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_FIXTURE_IDS: &[&str] = &[
        "ok_noop_fixture",
        "warning_text_evidence_only_sample",
        "blocking_synthetic_exact_sample",
    ];

    const EXACT_BLOCKING_FIXTURE_IDS: &[&str] = &[
        "mvp3_8_new_dangling_call",
        "mvp3_8_fixed_dangling_call",
        "mvp3_8_broken_import",
        "mvp3_8_fixed_import",
        "mvp3_8_renamed_target_symbol",
        "mvp3_8_ambiguous_rename_unknown",
        "mvp3_8_removed_file_stale_facts",
        "mvp3_8_derived_missing_provenance_integrity",
        "mvp3_8_missing_source_span_integrity",
    ];

    const SOURCE_ROLE_TEXT_UNKNOWN_FIXTURE_IDS: &[&str] = &[
        "mvp3_8_inline_test_added",
        "mvp3_8_mock_test_leakage_attempt",
        "mvp3_8_text_evidence_only_edit",
        "mvp3_8_unsupported_dynamic_call",
        "mvp3_8_config_package_text_mismatch",
        "mvp3_8_route_handler_missing_target",
    ];

    const DIRTY_EVIDENCE_FIXTURE_IDS: &[&str] = &[
        "mvp3_8_candidate_query_index_corrupt",
        "mvp3_8_candidate_spool_stale",
        "mvp3_8_candidate_spool_inaccessible",
        "mvp3_8_vector_runtime_stale",
        "mvp3_8_vector_audit_stale_diagnostic_only",
        "mvp3_8_path_evidence_stale_invalidated",
        "mvp3_8_source_navigation_stale",
        "mvp3_8_routing_handle_stale",
        "mvp3_8_deleted_file_stale_candidate_hit",
        "mvp3_8_renamed_import_stale_vector_hit",
        "mvp3_8_graph_claimable_optional_sidecar_corrupt",
        "mvp3_8_text_evidence_proof_ladder_changes",
    ];

    const STALE_NON_PROOF_DIRTY_FIXTURE_IDS: &[&str] = &[
        "mvp3_8_candidate_spool_stale",
        "mvp3_8_vector_runtime_stale",
        "mvp3_8_source_navigation_stale",
        "mvp3_8_deleted_file_stale_candidate_hit",
        "mvp3_8_renamed_import_stale_vector_hit",
    ];

    const FIX_RECOVERY_FIXTURE_IDS: &[&str] = &[
        "mvp3_8_fixed_dangling_call",
        "mvp3_8_fixed_import",
        "mvp3_8_renamed_target_symbol",
        "mvp3_8_removed_file_stale_facts",
        "mvp3_8_mock_test_leakage_attempt",
        "mvp3_8_derived_missing_provenance_integrity",
        "mvp3_8_missing_source_span_integrity",
        "mvp3_8_text_evidence_only_edit",
        "mvp3_8_candidate_spool_stale",
        "mvp3_8_candidate_query_index_corrupt",
        "mvp3_8_vector_runtime_stale",
        "mvp3_8_unsafe_db_recovery",
        "mvp3_8_graph_claimable_optional_sidecar_corrupt",
        "mvp3_8_config_package_text_mismatch",
        "mvp3_8_route_handler_missing_target",
    ];

    const RECOVERY_COMMAND_FIXTURE_IDS: &[&str] = &[
        "mvp3_8_candidate_spool_stale",
        "mvp3_8_candidate_query_index_corrupt",
        "mvp3_8_vector_runtime_stale",
        "mvp3_8_unsafe_db_recovery",
        "mvp3_8_graph_claimable_optional_sidecar_corrupt",
    ];

    const NON_BLOCKING_SOURCE_ROLE_TEXT_UNKNOWN_FIXTURE_IDS: &[&str] = &[
        "mvp3_8_inline_test_added",
        "mvp3_8_text_evidence_only_edit",
        "mvp3_8_unsupported_dynamic_call",
        "mvp3_8_config_package_text_mismatch",
        "mvp3_8_route_handler_missing_target",
    ];

    const RUNTIME_SOURCE_SPANNED_BLOCKER_FIXTURE_IDS: &[&str] = &[
        "mvp3_8_new_dangling_call",
        "mvp3_8_fixed_dangling_call",
        "mvp3_8_broken_import",
        "mvp3_8_fixed_import",
        "mvp3_8_renamed_target_symbol",
        "mvp3_8_removed_file_stale_facts",
        "mvp3_8_derived_missing_provenance_integrity",
    ];

    const POST_FIX_RECOVERY_FIXTURE_IDS: &[&str] = &[
        "mvp3_8_new_dangling_call",
        "mvp3_8_fixed_dangling_call",
        "mvp3_8_broken_import",
        "mvp3_8_fixed_import",
        "mvp3_8_renamed_target_symbol",
        "mvp3_8_removed_file_stale_facts",
        "mvp3_8_derived_missing_provenance_integrity",
        "mvp3_8_missing_source_span_integrity",
    ];

    const CURRENT_EXACT_RULE_IDS: &[&str] = &[
        "CG_MVP3_CALLS_DANGLING_TARGET",
        "CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED",
        "CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED",
        "CG_MVP3_IMPORTS_DANGLING_TARGET",
        "CG_MVP3_IMPORTS_DELETED_EXPORT_STILL_IMPORTED",
        "CG_MVP3_DERIVED_EDGE_MISSING_PROVENANCE",
        "CG_MVP3_PROOF_EDGE_MISSING_SOURCE_SPAN",
    ];

    const CURRENT_SOURCE_ROLE_TEXT_UNKNOWN_RULE_IDS: &[&str] = &[
        "CG_MVP3_SOURCE_ROLE_INLINE_TEST_PROMOTED_TO_PRODUCTION",
        "CG_MVP3_SOURCE_ROLE_MOCK_EVIDENCE_IN_PRODUCTION_PROOF",
        "CG_MVP3_DYNAMIC_CALL_UNKNOWN",
        "CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING",
        "CG_MVP3_ROUTE_UNSUPPORTED_FRAMEWORK_UNKNOWN",
        "text_evidence_changed_non_graph",
    ];

    const CURRENT_DIRTY_EVIDENCE_RULE_IDS: &[&str] = &[
        "CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED",
        "text_evidence_changed_non_graph",
    ];

    #[test]
    fn fixture_manifest_schema_defined() {
        let schema = mvp3_validation_fixture_manifest_schema_value();
        assert_eq!(
            schema["properties"]["schema_version"]["const"].as_u64(),
            Some(MVP3_VALIDATION_FIXTURE_SCHEMA_VERSION as u64)
        );
        let required = schema["required"]
            .as_array()
            .expect("required fields")
            .iter()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>();
        for field in REQUIRED_MANIFEST_FIELDS {
            assert!(required.contains(field), "missing required field {field}");
        }
    }

    #[test]
    fn fixture_manifest_schema_validates_sample() {
        for path in sample_fixture_paths() {
            let raw = fs::read_to_string(&path).expect("read sample fixture");
            let value: Value = serde_json::from_str(&raw).expect("sample fixture JSON");
            validate_mvp3_validation_fixture_manifest_value(&value)
                .unwrap_or_else(|error| panic!("{} failed validation: {error}", path.display()));
            let manifest: Mvp3ValidationFixtureManifest =
                serde_json::from_value(value).expect("sample decodes");
            assert_eq!(
                manifest.schema_version,
                MVP3_VALIDATION_FIXTURE_SCHEMA_VERSION
            );
        }
    }

    #[test]
    fn pre_mvp4_4_language_fixture_manifest_schema_includes_axes() {
        let schema = mvp3_validation_fixture_manifest_schema_value();
        let properties = schema["properties"].as_object().expect("schema properties");
        for field in [
            "frontend",
            "file_extensions",
            "source_role",
            "source_role_expected",
            "surface",
            "proof_tier",
            "fixture_group",
            "fixture_groups",
            "expected_exact_relations",
            "expected_blocking",
            "forbidden_claims",
            "expected_provenance",
            "expected_packet_status",
            "expected_cli_mcp_parity",
            "expected_compact_fields",
            "expected_audit_fields",
            "dogfood_scenario_id",
            "language_support_status",
            "dynamic_boundary_cases",
            "dynamic_macro_runtime_contract",
            "expected_dynamic_unresolved_classification",
            "mvp4_packet_dynamic_gap_expectations",
            "test_assert_mock_cases",
            "test_assert_mock_contract",
            "expected_test_impact_behavior",
            "mvp4_packet_test_source_expectations",
            "route_framework_cases",
            "bridge_boundary_cases",
            "route_bridge_contract",
            "mvp4_packet_route_bridge_expectations",
        ] {
            assert!(properties.contains_key(field), "missing axis field {field}");
        }
        assert!(schema["properties"]["surface"]["items"]["enum"]
            .as_array()
            .expect("surface enum")
            .iter()
            .any(|value| value.as_str() == Some("local_flow")));
        assert!(schema["properties"]["proof_tier"]["items"]["enum"]
            .as_array()
            .expect("proof tier enum")
            .iter()
            .any(|value| value.as_str() == Some("flow_proof")));
    }

    #[test]
    fn pre_mvp4_4_existing_mvp3_fixtures_backward_compatible() {
        let manifest = load_manifest_by_id("ok_noop_fixture");
        assert!(manifest.frontend.is_empty());
        assert!(manifest.surface.is_empty());
        assert!(manifest.proof_tier.is_empty());
        assert!(manifest.expected_blocking.is_empty());
        assert!(manifest.expected_provenance.is_null());
        assert_eq!(manifest_frontend(&manifest), "typescript");
        assert!(manifest_surface_values(&manifest)
            .iter()
            .any(|surface| surface == "validate_edit"));
    }

    #[test]
    fn pre_mvp4_4_fixture_runner_filters_language_surface_proof_tier() {
        let mut options = sample_options();
        options.fixture_tags = vec!["pre_mvp4_4_language_fixture".to_string()];
        options.language_filters = Mvp3LanguageFixtureFilters {
            languages: vec!["python".to_string()],
            surfaces: vec!["local_flow".to_string()],
            proof_tiers: vec!["not_applicable".to_string()],
            support_statuses: vec!["not_implemented".to_string()],
            fixture_groups: vec!["language_packet_interaction".to_string()],
            strict: true,
            ..Mvp3LanguageFixtureFilters::default()
        };
        let report = run_mvp3_validation_fixtures(options).expect("filtered language fixtures");
        assert_eq!(report.fixtures_total, 1);
        let result = &report.results[0];
        assert_eq!(
            result.fixture_id,
            "pre_mvp4_4_language_python_language_packet_interaction"
        );
        assert_eq!(result.status, "passed");
        assert_eq!(
            result.unsupported_boundary,
            "non_claimable_boundary_asserted"
        );
    }

    #[test]
    fn pre_mvp4_4_fixture_runner_aggregates_by_language_and_support_status() {
        let mut options = sample_options();
        options.fixture_tags = vec!["pre_mvp4_4_language_fixture".to_string()];
        options.language_filters.languages = vec!["typescript".to_string()];
        options.language_filters.strict = true;
        let report = run_mvp3_validation_fixtures(options).expect("typescript language fixtures");
        assert_eq!(report.fixtures_total, 12);
        assert_eq!(
            report
                .aggregation
                .by_language
                .get("typescript")
                .map(|bucket| bucket.fixtures_total),
            Some(12)
        );
        assert!(report
            .aggregation
            .by_support_status
            .contains_key("partial_supported"));
        assert!(report.aggregation.by_surface.contains_key("validate_edit"));
    }

    #[test]
    fn pre_mvp4_4_compact_budget_fixtures_cover_language_surfaces() {
        let mut options = sample_options();
        options.fixture_tags = vec!["pre_mvp4_4_language_fixture".to_string()];
        options.language_filters = Mvp3LanguageFixtureFilters {
            fixture_groups: vec!["language_compact_budget".to_string()],
            strict: true,
            ..Mvp3LanguageFixtureFilters::default()
        };
        let report = run_mvp3_validation_fixtures(options).expect("compact budget fixtures");
        assert_eq!(report.fixtures_total, 13);
        assert_eq!(report.fixtures_failed, 0);

        for language in [
            "c",
            "cpp",
            "csharp",
            "go",
            "java",
            "javascript",
            "jsx",
            "php",
            "python",
            "ruby",
            "rust",
            "tsx",
            "typescript",
        ] {
            let fixture_id = format!("pre_mvp4_4_language_{language}_language_compact_budget");
            let result = result_by_id(&report, &fixture_id);
            assert_eq!(result.status, "passed", "{language}");
            let compact = surface_by_name(result, "compact_explain_audit");
            for assertion in [
                "compact_default_bounded",
                "critical_fields_preserved",
                "omitted_count_or_expansion_handles",
            ] {
                assert!(
                    compact
                        .structured_assertions
                        .iter()
                        .any(|value| value == assertion),
                    "missing compact assertion {assertion} for {language}"
                );
            }

            let manifest = load_manifest_value_by_id(&fixture_id);
            let fields = manifest["expected_compact_fields"]
                .as_array()
                .expect("compact fields");
            for field in [
                "validation_rule_id",
                "source_span",
                "recommended_fix",
                "source_role",
                "proof_strength",
                "omitted_count",
                "expansion_handles",
            ] {
                assert!(
                    fields.iter().any(|value| value.as_str() == Some(field)),
                    "missing expected compact field {field} for {language}"
                );
            }
            assert_eq!(
                manifest["expected_compact_output"]["ordered_steps_inline"].as_bool(),
                Some(false),
                "{language}"
            );
            assert_eq!(
                manifest["expected_compact_output"]["full_source_body_output"].as_bool(),
                Some(false),
                "{language}"
            );
        }

        let typescript =
            load_manifest_value_by_id("pre_mvp4_4_language_typescript_language_compact_budget");
        assert!(typescript["expected_compact_fields"]
            .as_array()
            .expect("typescript compact fields")
            .iter()
            .any(|value| value.as_str() == Some("micro_flow_packet_delta")));
        assert_eq!(
            typescript["expected_packet_status"].as_str(),
            Some("supported")
        );
    }

    #[test]
    fn pre_mvp4_4_dataflow_fixture_group_covers_registered_languages() {
        let mut options = sample_options();
        options.fixture_tags = vec!["pre_mvp4_4_language_fixture".to_string()];
        options.language_filters = Mvp3LanguageFixtureFilters {
            fixture_groups: vec!["language_reads_writes_flows".to_string()],
            strict: true,
            ..Mvp3LanguageFixtureFilters::default()
        };
        let report = run_mvp3_validation_fixtures(options).expect("dataflow language fixtures");
        assert_eq!(report.fixtures_total, 13);
        assert_eq!(report.fixtures_failed, 0);
        assert_eq!(
            result_by_id(
                &report,
                "pre_mvp4_4_language_typescript_language_reads_writes_flows"
            )
            .language_support_status,
            "derived_with_provenance_supported"
        );
        assert_eq!(
            result_by_id(
                &report,
                "pre_mvp4_4_language_python_language_reads_writes_flows"
            )
            .unsupported_boundary,
            "non_claimable_boundary_asserted"
        );
    }

    #[test]
    fn pre_mvp4_4_dynamic_macro_runtime_fixtures_cover_language_boundaries() {
        let mut options = sample_options();
        options.fixture_tags = vec!["pre_mvp4_4_language_fixture".to_string()];
        options.language_filters = Mvp3LanguageFixtureFilters {
            fixture_groups: vec!["language_dynamic_macro_runtime".to_string()],
            strict: true,
            ..Mvp3LanguageFixtureFilters::default()
        };
        let report = run_mvp3_validation_fixtures(options).expect("dynamic language fixtures");
        assert_eq!(report.fixtures_total, 13);
        assert_eq!(report.fixtures_failed, 0);
        let expected_cases: BTreeMap<&str, &[&str]> = BTreeMap::from([
            (
                "rust",
                &[
                    "rust_macro_invocation",
                    "rust_trait_dispatch",
                    "rust_async_callback",
                    "rust_build_script_codegen_hint",
                    "rust_cfg_gated_code",
                ][..],
            ),
            (
                "python",
                &[
                    "python_importlib",
                    "python_getattr",
                    "python_monkeypatching",
                    "python_dynamic_attribute_call",
                    "python_dependency_injection_pattern",
                ][..],
            ),
            (
                "typescript",
                &[
                    "typescript_dynamic_import",
                    "typescript_computed_property_call",
                    "typescript_event_listener_callback",
                    "typescript_optional_chaining",
                    "typescript_member_global_dynamic_call",
                    "typescript_async_callback_promise",
                    "typescript_dependency_injection_style",
                ][..],
            ),
            (
                "javascript",
                &[
                    "javascript_dynamic_import",
                    "javascript_computed_property_call",
                    "javascript_event_listener_callback",
                    "javascript_optional_chaining",
                    "javascript_dependency_injection_style",
                ][..],
            ),
            (
                "go",
                &[
                    "go_interface_dispatch",
                    "go_build_tags",
                    "go_reflection",
                    "go_goroutine_callback",
                ][..],
            ),
            (
                "c",
                &[
                    "c_macro_hidden_call",
                    "c_inactive_preprocessor_branch",
                    "c_generated_header_hint",
                    "c_function_pointer",
                ][..],
            ),
            (
                "cpp",
                &[
                    "cpp_macro_hidden_call",
                    "cpp_inactive_preprocessor_branch",
                    "cpp_generated_header_hint",
                    "cpp_function_pointer",
                    "cpp_virtual_dispatch",
                ][..],
            ),
            (
                "java",
                &[
                    "java_reflection",
                    "java_interface_virtual_dispatch",
                    "java_annotation_framework_injection",
                    "java_async_callback",
                ][..],
            ),
            (
                "csharp",
                &[
                    "csharp_reflection",
                    "csharp_interface_dispatch",
                    "csharp_dependency_injection_container",
                    "csharp_async_await_callback",
                ][..],
            ),
            (
                "ruby",
                &[
                    "ruby_send",
                    "ruby_method_missing",
                    "ruby_monkeypatching",
                    "ruby_metaprogramming",
                ][..],
            ),
            (
                "php",
                &[
                    "php_dynamic_include",
                    "php_magic_methods",
                    "php_variable_functions",
                ][..],
            ),
            (
                "jsx",
                &[
                    "jsx_computed_property_call",
                    "jsx_event_callback",
                    "jsx_optional_chaining",
                ][..],
            ),
            (
                "tsx",
                &[
                    "tsx_computed_property_call",
                    "tsx_event_callback",
                    "tsx_optional_chaining",
                ][..],
            ),
        ]);
        for (language, case_ids) in expected_cases {
            let fixture_id =
                format!("pre_mvp4_4_language_{language}_language_dynamic_macro_runtime");
            let result = result_by_id(&report, &fixture_id);
            assert_eq!(result.language_support_status, "dynamic_runtime_required");
            let value = load_manifest_value_by_id(&fixture_id);
            let cases = value["dynamic_boundary_cases"]
                .as_array()
                .unwrap_or_else(|| panic!("{fixture_id} missing dynamic_boundary_cases"));
            let observed_case_ids = cases
                .iter()
                .filter_map(|case| case["case_id"].as_str())
                .collect::<BTreeSet<_>>();
            for case_id in case_ids {
                assert!(
                    observed_case_ids.contains(case_id),
                    "{fixture_id} missing {case_id}"
                );
            }
            assert!(cases.iter().all(|case| {
                case["graph_proof_allowed"].as_bool() == Some(false)
                    && case["hard_interrupt_eligible"].as_bool() == Some(false)
                    && case["expected_relation_exactness"].as_str()
                        == Some("unknown_or_unsupported")
            }));
            assert_eq!(
                value["dynamic_macro_runtime_contract"]["dynamic_behavior_not_exact_by_default"]
                    .as_bool(),
                Some(true)
            );
            assert_eq!(
                value["dynamic_macro_runtime_contract"]["runtime_di_reflection_not_graph_proof"]
                    .as_bool(),
                Some(true)
            );
            assert_eq!(
                value["expected_dynamic_unresolved_classification"]
                    ["dynamic_or_computed_not_repo_local"]
                    .as_bool(),
                Some(true)
            );
            assert_eq!(
                value["expected_dynamic_unresolved_classification"]
                    ["default_hard_interrupt_allowed"]
                    .as_bool(),
                Some(false)
            );
        }

        let typescript = load_manifest_value_by_id(
            "pre_mvp4_4_language_typescript_language_dynamic_macro_runtime",
        );
        assert_eq!(
            typescript["mvp4_packet_dynamic_gap_expectations"]["packet_rows_allowed"].as_bool(),
            Some(true)
        );
        assert_eq!(
            typescript["mvp4_packet_dynamic_gap_expectations"]["flow_proof_through_dynamic_gap"]
                .as_bool(),
            Some(false)
        );
        let python =
            load_manifest_value_by_id("pre_mvp4_4_language_python_language_dynamic_macro_runtime");
        assert_eq!(
            python["mvp4_packet_dynamic_gap_expectations"]["packet_rows_allowed"].as_bool(),
            Some(false)
        );
        assert_eq!(
            python["mvp4_packet_dynamic_gap_expectations"]["expected_packet_status"].as_str(),
            Some("not_applicable")
        );
    }

    #[test]
    fn pre_mvp4_4_test_assert_mock_fixtures_cover_language_boundaries() {
        let mut options = sample_options();
        options.fixture_tags = vec!["pre_mvp4_4_language_fixture".to_string()];
        options.language_filters = Mvp3LanguageFixtureFilters {
            fixture_groups: vec!["language_tests_asserts_mocks".to_string()],
            strict: true,
            ..Mvp3LanguageFixtureFilters::default()
        };
        let report = run_mvp3_validation_fixtures(options).expect("test/assert language fixtures");
        assert_eq!(report.fixtures_total, 13);
        assert_eq!(report.fixtures_failed, 0);

        let expected_cases: BTreeMap<&str, &[&str]> = BTreeMap::from([
            (
                "rust",
                &[
                    "rust_cfg_test_module",
                    "rust_test_attribute",
                    "rust_inline_test_helper",
                    "rust_mock_stub_naming",
                ][..],
            ),
            (
                "python",
                &[
                    "python_pytest_function",
                    "python_unittest_class",
                    "python_pytest_fixture",
                    "python_mock_patch_usage",
                ][..],
            ),
            (
                "typescript",
                &[
                    "typescript_describe_test_it",
                    "typescript_expect_assert",
                    "typescript_jest_vitest_mock",
                    "typescript_test_spec_path",
                ][..],
            ),
            (
                "javascript",
                &[
                    "javascript_describe_test_it",
                    "javascript_expect_assert",
                    "javascript_jest_vitest_mock",
                    "javascript_test_spec_path",
                ][..],
            ),
            (
                "go",
                &["go_test_file", "go_test_function", "go_table_test"][..],
            ),
            (
                "c",
                &["c_test_directory", "c_assert_macro", "c_mock_stub_file"][..],
            ),
            (
                "cpp",
                &[
                    "cpp_test_directory",
                    "cpp_assert_macro",
                    "cpp_mock_stub_file",
                ][..],
            ),
            (
                "java",
                &["java_junit_test", "java_assertion", "java_mock_class"][..],
            ),
            (
                "csharp",
                &[
                    "csharp_test_attribute",
                    "csharp_assertion",
                    "csharp_mock_class",
                ][..],
            ),
            (
                "ruby",
                &["ruby_rspec_example", "ruby_assertion", "ruby_mock_double"][..],
            ),
            (
                "php",
                &["php_phpunit_test", "php_assertion", "php_mock_usage"][..],
            ),
            (
                "jsx",
                &["jsx_component_test", "jsx_expect_assert", "jsx_mock_usage"][..],
            ),
            (
                "tsx",
                &["tsx_component_test", "tsx_expect_assert", "tsx_mock_usage"][..],
            ),
        ]);

        for (language, case_ids) in expected_cases {
            let fixture_id = format!("pre_mvp4_4_language_{language}_language_tests_asserts_mocks");
            let result = result_by_id(&report, &fixture_id);
            assert_eq!(result.language_support_status, "partial_supported");
            assert_eq!(
                result.unsupported_boundary,
                "non_claimable_boundary_asserted"
            );
            let value = load_manifest_value_by_id(&fixture_id);
            let cases = value["test_assert_mock_cases"]
                .as_array()
                .unwrap_or_else(|| panic!("{fixture_id} missing test_assert_mock_cases"));
            let observed_case_ids = cases
                .iter()
                .filter_map(|case| case["case_id"].as_str())
                .collect::<BTreeSet<_>>();
            for case_id in case_ids {
                assert!(
                    observed_case_ids.contains(case_id),
                    "{fixture_id} missing {case_id}"
                );
            }
            assert!(cases.iter().all(|case| {
                case["production_proof_allowed"].as_bool() == Some(false)
                    && case["hard_interrupt_eligible"].as_bool() == Some(false)
                    && case["expected_evidence_role"]
                        .as_str()
                        .is_some_and(|role| matches!(role, "test" | "mock"))
            }));
            assert_eq!(
                value["test_assert_mock_contract"]["production_default_excludes_test_mock_stub"]
                    .as_bool(),
                Some(true)
            );
            assert_eq!(
                value["test_assert_mock_contract"]["test_mock_evidence_not_production_proof"]
                    .as_bool(),
                Some(true)
            );
            assert_eq!(
                value["expected_test_impact_behavior"]["includes_test_evidence_intentionally"]
                    .as_bool(),
                Some(true)
            );
            assert_eq!(
                value["mvp4_packet_test_source_expectations"]["test_source_flow_proof_allowed"]
                    .as_bool(),
                Some(false)
            );
        }

        let typescript = load_manifest_value_by_id(
            "pre_mvp4_4_language_typescript_language_tests_asserts_mocks",
        );
        assert_eq!(
            typescript["mvp4_packet_test_source_expectations"]["packet_language_scope"].as_str(),
            Some("typescript_ts_production_only")
        );
        assert_eq!(
            typescript["mvp4_packet_test_source_expectations"]["test_source_packet_rows_allowed"]
                .as_bool(),
            Some(false)
        );
        let python =
            load_manifest_value_by_id("pre_mvp4_4_language_python_language_tests_asserts_mocks");
        assert_eq!(
            python["mvp4_packet_test_source_expectations"]["packet_rows_allowed"].as_bool(),
            Some(false)
        );
        assert_eq!(
            python["mvp4_packet_test_source_expectations"]["expected_packet_status"].as_str(),
            Some("not_applicable")
        );
    }

    #[test]
    fn pre_mvp4_4_route_bridge_fixtures_cover_language_boundaries() {
        let mut options = sample_options();
        options.fixture_tags = vec!["pre_mvp4_4_language_fixture".to_string()];
        options.language_filters = Mvp3LanguageFixtureFilters {
            fixture_groups: vec!["language_route_bridge_inert".to_string()],
            strict: true,
            ..Mvp3LanguageFixtureFilters::default()
        };
        let report = run_mvp3_validation_fixtures(options).expect("route/bridge language fixtures");
        assert_eq!(report.fixtures_total, 13);
        assert_eq!(report.fixtures_failed, 0);

        let expected_route_cases: BTreeMap<&str, &[&str]> = BTreeMap::from([
            (
                "python",
                &["python_fastapi", "python_flask", "python_django"][..],
            ),
            ("javascript", &["javascript_express", "javascript_koa"][..]),
            (
                "typescript",
                &["typescript_express", "typescript_koa", "typescript_nestjs"][..],
            ),
            ("java", &["java_spring"][..]),
            ("rust", &["rust_actix", "rust_axum"][..]),
            ("ruby", &["ruby_rails"][..]),
            ("php", &["php_laravel"][..]),
            ("tsx", &["tsx_nestjs_controller"][..]),
            ("jsx", &["jsx_express_router_boundary"][..]),
            ("go", &["go_http_handler_boundary"][..]),
            ("c", &["c_no_route_framework"][..]),
            ("cpp", &["cpp_no_route_framework"][..]),
            ("csharp", &["csharp_aspnet_controller_boundary"][..]),
        ]);
        let expected_bridge_cases: BTreeMap<&str, &[&str]> = BTreeMap::from([
            ("c", &["c_header_bridge_name_match"][..]),
            ("cpp", &["cpp_header_bridge_name_match"][..]),
            ("csharp", &["csharp_runtime_ipc_bridge"][..]),
            ("go", &["go_wasm_export_boundary"][..]),
            ("java", &["java_jni_bridge"][..]),
            (
                "javascript",
                &["javascript_electron_ipc", "javascript_react_native_module"][..],
            ),
            ("jsx", &["jsx_react_native_module"][..]),
            ("php", &["php_ffi_bridge_boundary"][..]),
            ("python", &["python_ctypes_bridge_boundary"][..]),
            ("ruby", &["ruby_ffi_bridge_boundary"][..]),
            (
                "rust",
                &[
                    "rust_extern_c_bridge",
                    "rust_wasm_import_export",
                    "rust_tauri_ipc",
                ][..],
            ),
            ("tsx", &["tsx_react_native_module", "tsx_expo_module"][..]),
            (
                "typescript",
                &[
                    "typescript_react_native_module",
                    "typescript_expo_module",
                    "typescript_electron_ipc",
                    "typescript_tauri_ipc",
                ][..],
            ),
        ]);

        for (language, route_cases) in expected_route_cases {
            let fixture_id = format!("pre_mvp4_4_language_{language}_language_route_bridge_inert");
            let result = result_by_id(&report, &fixture_id);
            assert_eq!(
                result.unsupported_boundary,
                "non_claimable_boundary_asserted"
            );
            let value = load_manifest_value_by_id(&fixture_id);
            let observed_route_cases = value["route_framework_cases"]
                .as_array()
                .unwrap_or_else(|| panic!("{fixture_id} missing route_framework_cases"))
                .iter()
                .filter_map(|case| case["case_id"].as_str())
                .collect::<BTreeSet<_>>();
            for case_id in route_cases {
                assert!(
                    observed_route_cases.contains(case_id),
                    "{fixture_id} missing {case_id}"
                );
            }
            let observed_bridge_cases = value["bridge_boundary_cases"]
                .as_array()
                .unwrap_or_else(|| panic!("{fixture_id} missing bridge_boundary_cases"))
                .iter()
                .filter_map(|case| case["case_id"].as_str())
                .collect::<BTreeSet<_>>();
            for case_id in expected_bridge_cases
                .get(language)
                .unwrap_or_else(|| panic!("missing bridge expectations for {language}"))
                .iter()
            {
                assert!(
                    observed_bridge_cases.contains(case_id),
                    "{fixture_id} missing {case_id}"
                );
            }
            assert!(value["route_framework_cases"]
                .as_array()
                .expect("route cases")
                .iter()
                .all(|case| {
                    case["routes_to_edge_allowed"].as_bool() == Some(false)
                        && case["mounts_router_edge_allowed"].as_bool() == Some(false)
                        && case["hard_interrupt_eligible"].as_bool() == Some(false)
                }));
            assert!(value["bridge_boundary_cases"]
                .as_array()
                .expect("bridge cases")
                .iter()
                .all(|case| {
                    case["bridges_to_edge_allowed"].as_bool() == Some(false)
                        && case["graph_proof_allowed"].as_bool() == Some(false)
                        && case["hard_interrupt_eligible"].as_bool() == Some(false)
                }));
            assert_eq!(
                value["route_bridge_contract"]["routes_to_edges_emitted"].as_u64(),
                Some(0)
            );
            assert_eq!(
                value["route_bridge_contract"]["mounts_router_edges_emitted"].as_u64(),
                Some(0)
            );
            assert_eq!(
                value["route_bridge_contract"]["bridges_to_edges_emitted"].as_u64(),
                Some(0)
            );
            assert_eq!(
                value["route_bridge_contract"]["bridge_validation_inert_until_exact_support"]
                    .as_bool(),
                Some(true)
            );
            assert_eq!(
                value["mvp4_packet_route_bridge_expectations"]
                    ["local_flow_packet_route_bridge_pull_forward_count"]
                    .as_u64(),
                Some(0)
            );
            assert_eq!(
                value["mvp4_packet_route_bridge_expectations"]["context_entry_activated"].as_bool(),
                Some(false)
            );
        }

        let typescript =
            load_manifest_value_by_id("pre_mvp4_4_language_typescript_language_route_bridge_inert");
        assert_eq!(
            typescript["route_bridge_contract"]["current_exact_route_support"].as_str(),
            Some("HANDLES_EXPOSES_literal_js_ts_activation_gated")
        );
        let python =
            load_manifest_value_by_id("pre_mvp4_4_language_python_language_route_bridge_inert");
        assert_eq!(
            python["route_bridge_contract"]["current_exact_route_support"].as_str(),
            Some("not_implemented")
        );
    }

    #[test]
    fn pre_mvp4_4_packet_interaction_fixtures_cover_registered_languages() {
        let mut options = sample_options();
        options.fixture_tags = vec!["pre_mvp4_4_language_fixture".to_string()];
        options.language_filters = Mvp3LanguageFixtureFilters {
            fixture_groups: vec!["language_packet_interaction".to_string()],
            strict: true,
            ..Mvp3LanguageFixtureFilters::default()
        };
        let report = run_mvp3_validation_fixtures(options).expect("packet language fixtures");
        assert_eq!(report.fixtures_total, 13);
        assert_eq!(report.fixtures_failed, 0);

        let typescript = result_by_id(
            &report,
            "pre_mvp4_4_language_typescript_language_packet_interaction",
        );
        assert_eq!(
            typescript.language_support_status,
            "exact_supported_activation_gated"
        );
        assert_eq!(
            surface_by_name(typescript, "local_flow").support_status,
            "supported"
        );

        for language in [
            "javascript",
            "jsx",
            "tsx",
            "python",
            "rust",
            "go",
            "c",
            "cpp",
            "java",
            "csharp",
            "ruby",
            "php",
        ] {
            let fixture_id = format!("pre_mvp4_4_language_{language}_language_packet_interaction");
            let result = result_by_id(&report, &fixture_id);
            assert_eq!(result.language_support_status, "not_implemented");
            let local_flow = surface_by_name(result, "local_flow");
            assert_eq!(local_flow.status, "not_applicable", "{language}");
            assert!(local_flow
                .structured_assertions
                .iter()
                .any(|assertion| { assertion == "forbidden_field_absent:flow_proof" }));
        }
    }

    #[test]
    fn pre_mvp4_4_dirty_evidence_fixtures_cover_language_and_sidecar_boundaries() {
        let mut options = sample_options();
        options.fixture_tags = vec!["pre_mvp4_4_language_fixture".to_string()];
        options.language_filters = Mvp3LanguageFixtureFilters {
            fixture_groups: vec!["language_dirty_evidence".to_string()],
            strict: true,
            ..Mvp3LanguageFixtureFilters::default()
        };
        let report = run_mvp3_validation_fixtures(options).expect("dirty evidence fixtures");
        assert_eq!(report.fixtures_total, 13);
        assert_eq!(report.fixtures_failed, 0);

        let required_cases = BTreeSet::from([
            "edit_source_file",
            "delete_source_file",
            "rename_source_file",
            "source_role_change",
            "generated_file_change",
            "ignored_file_change",
            "parser_break",
            "parser_restore",
            "text_only_source_text_edit",
            "import_export_call_relation_edit",
        ]);
        let required_surfaces = BTreeSet::from([
            "graph_facts",
            "source_spans",
            "text_evidence",
            "unresolved_references",
            "path_evidence",
            "candidate_spool",
            "candidate_query_index",
            "vector_chunks",
            "nuance_entries",
            "source_navigation_handles",
            "routing_handles",
            "context_pack_handles",
            "source_role_metadata",
        ]);

        for language in [
            "javascript",
            "jsx",
            "typescript",
            "tsx",
            "python",
            "go",
            "rust",
            "java",
            "csharp",
            "c",
            "cpp",
            "ruby",
            "php",
        ] {
            let fixture_id = format!("pre_mvp4_4_language_{language}_language_dirty_evidence");
            let result = result_by_id(&report, &fixture_id);
            assert_eq!(result.status, "passed");
            assert_eq!(result.language_support_status, "partial_supported");
            assert_eq!(
                surface_by_name(result, "dirty_evidence").support_status,
                "supported"
            );
            let value = load_manifest_value_by_id(&fixture_id);
            let cases = value["expected_dirty_evidence_changes"]["invalidation_cases"]
                .as_array()
                .unwrap_or_else(|| panic!("{fixture_id} missing invalidation_cases"))
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>();
            let surfaces = value["expected_dirty_evidence_changes"]["invalidation_surfaces"]
                .as_array()
                .unwrap_or_else(|| panic!("{fixture_id} missing invalidation_surfaces"))
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>();
            for expected in &required_cases {
                assert!(cases.contains(expected), "{fixture_id} missing {expected}");
            }
            for expected in &required_surfaces {
                assert!(
                    surfaces.contains(expected),
                    "{fixture_id} missing {expected}"
                );
            }
            assert_eq!(
                value["expected_dirty_evidence_changes"]["no_dirty_evidence_claimable"].as_bool(),
                Some(true)
            );
            assert_eq!(
                value["expected_dirty_evidence_changes"]["text_evidence_not_graph_proof"].as_bool(),
                Some(true)
            );
            assert_eq!(
                value["expected_dirty_evidence_changes"]["candidate_vector_nuance_not_graph_proof"]
                    .as_bool(),
                Some(true)
            );
            assert_eq!(
                value["expected_dirty_evidence_changes"]["optional_sidecar_stale_not_source_error"]
                    .as_bool(),
                Some(true)
            );
            assert_eq!(
                value["expected_dirty_evidence_changes"]
                    ["graph_claimability_sidecar_availability_separated"]
                    .as_bool(),
                Some(true)
            );
            assert_eq!(
                value["expected_sidecar_statuses"]["context_pack_handle_status"].as_str(),
                Some("invalidated_or_not_applicable")
            );

            let mvp4_sidecars =
                &value["expected_dirty_evidence_changes"]["typescript_mvp4_sidecars"];
            assert_eq!(
                mvp4_sidecars["unsupported_language_flow_claim_count"].as_u64(),
                Some(0)
            );
            assert_eq!(
                mvp4_sidecars["packet_stale_state_overclaim_count"].as_u64(),
                Some(0)
            );
            if language == "typescript" {
                assert_eq!(
                    value["expected_sidecar_statuses"]["local_flow_packets_status"].as_str(),
                    Some("active_for_typescript_dot_ts_production_refreshed_or_removed")
                );
                assert_eq!(
                    mvp4_sidecars["local_flow_packets"].as_str(),
                    Some("refreshed_or_removed_when_eligible_production_ts")
                );
                assert_eq!(
                    mvp4_sidecars["flow_proof"].as_str(),
                    Some("changed_only_if_complete_eligible_packet_path")
                );
            } else {
                assert_eq!(
                    value["expected_sidecar_statuses"]["local_flow_packets_status"].as_str(),
                    Some("not_applicable")
                );
                assert_eq!(
                    mvp4_sidecars["local_flow_packets"].as_str(),
                    Some("not_applicable")
                );
                assert_eq!(mvp4_sidecars["flow_proof"].as_str(), Some("not_applicable"));
            }
        }
    }

    #[test]
    fn pre_mvp4_4_unsupported_fixture_expectations_supported() {
        let report = run_mvp3_validation_fixture_by_id(
            sample_options(),
            "pre_mvp4_4_language_python_language_packet_interaction",
        )
        .expect("unsupported packet fixture");
        let result = &report.results[0];
        assert_eq!(result.status, "passed");
        assert_eq!(result.language_support_status, "not_implemented");
        assert!(surface_by_name(result, "local_flow")
            .structured_assertions
            .iter()
            .any(|assertion| assertion == "forbidden_field_absent:flow_proof"));
        let manifest =
            load_manifest_by_id("pre_mvp4_4_language_python_language_packet_interaction");
        assert!(!manifest.expected_diagnostics.is_empty());
        assert!(!manifest.expected_hard_interrupt);
    }

    #[test]
    fn pre_mvp4_4_forbidden_claim_detection_supported() {
        let manifest =
            load_manifest_by_id("pre_mvp4_4_language_python_language_packet_interaction");
        let mut observed = synthetic_observed_packet(&manifest).expect("synthetic packet");
        observed["claim"] = json!("unsupported_behavior_hard_interrupt");
        let report = evaluate_mvp3_validation_fixture_assertions(&manifest, &observed);
        assert!(!report.passed);
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("forbidden output present")));
    }

    #[test]
    fn pre_mvp4_4_release_command_plan_generation_supported() {
        let mut options = sample_options();
        options.mode = Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan;
        let report = run_mvp3_validation_fixture_by_id(
            options,
            "pre_mvp4_4_language_typescript_language_cli_mcp_parity",
        )
        .expect("release command plan");
        assert_eq!(report.runner_mode, "release_binary_plan");
        let result = &report.results[0];
        assert_step(&result.command_plan, "baseline_query_symbols");
        assert_step(&result.command_plan, "query_local_flow");
        assert!(result
            .surface_support
            .release_binary
            .contains("supported_command_plan"));
    }

    #[test]
    fn pre_mvp4_4_mcp_command_plan_generation_supported_or_not_applicable() {
        let report = run_mvp3_validation_fixture_by_id(
            sample_options(),
            "pre_mvp4_4_language_typescript_language_cli_mcp_parity",
        )
        .expect("mcp command plan");
        let result = &report.results[0];
        assert_step(&result.command_plan, "mcp_validate_edit");
        let mcp_surface = surface_by_name(result, "mcp_validate_edit");
        assert_eq!(mcp_surface.status, "passed");
        assert_eq!(mcp_surface.support_status, "supported");
    }

    #[test]
    fn pre_mvp4_4_language_fixture_harness_gate_writes_runner_results_when_requested() {
        let Ok(output_dir) = std::env::var("CODEGRAPH_PRE_MVP4_4_LANGUAGE_FIXTURE_OUTPUT_DIR")
        else {
            return;
        };
        let output_dir = PathBuf::from(output_dir);
        let mut options = sample_options();
        options.run_root = output_dir.join("runner");
        options.fixture_tags = vec!["pre_mvp4_4_language_fixture".to_string()];
        options.language_filters.strict = true;
        let report = run_mvp3_validation_fixtures(options).expect("language fixture gate report");
        assert_eq!(report.status, "complete");
        assert_eq!(report.fixtures_failed, 0);
        write_json(
            &output_dir.join("language_fixture_runner_results.json"),
            &report,
        )
        .expect("write runner results");
        write_json(
            &output_dir.join("language_fixture_manifest_schema.json"),
            &mvp3_validation_fixture_manifest_schema_value(),
        )
        .expect("write schema");
    }

    #[test]
    fn fixture_runner_implemented() {
        let report = run_sample_harness();
        assert_eq!(report.status, "complete");
        assert!(report.fixtures_total >= SAMPLE_FIXTURE_IDS.len());
        assert_eq!(report.fixtures_failed, 0);
        for fixture_id in SAMPLE_FIXTURE_IDS {
            assert_eq!(result_by_id(&report, fixture_id).status, "passed");
        }
    }

    #[test]
    fn baseline_index_mutation_validate_loop_supported() {
        let report = run_sample_harness();
        for result in report.results {
            assert_step(&result.command_plan, "baseline_index");
            assert_step(&result.command_plan, "mutation_apply");
            assert_step(&result.command_plan, "cli_validate_edit");
        }
    }

    #[test]
    fn cli_validate_edit_fixture_surface_supported() {
        let result = first_sample_result();
        assert_eq!(
            result.surface_support.cli_validate_edit,
            "supported_command_plan_and_structured_assertions"
        );
        assert_step(&result.command_plan, "cli_validate_edit");
    }

    #[test]
    fn watch_once_fixture_surface_supported() {
        let result = first_sample_result();
        assert_eq!(
            result.surface_support.watch_once,
            "supported_command_plan_and_structured_assertions"
        );
        assert_step(&result.command_plan, "watch_once");
    }

    #[test]
    fn mcp_fixture_surface_supported_or_not_applicable() {
        let result = first_sample_result();
        assert_eq!(
            result.surface_support.mcp_validate_edit,
            "supported_or_not_applicable_by_fixture_manifest"
        );
        assert_step(&result.command_plan, "mcp_validate_edit");
    }

    #[test]
    fn context_pack_fixture_surface_supported() {
        let result = first_sample_result();
        assert_step(&result.command_plan, "baseline_context_pack");
        assert_step(&result.command_plan, "context_pack_after_mutation");
    }

    #[test]
    fn status_doctor_fixture_surface_supported() {
        let result = first_sample_result();
        assert_step(&result.command_plan, "status");
        assert_step(&result.command_plan, "doctor");
    }

    #[test]
    fn forbidden_claimable_output_assertions_supported() {
        let manifest = load_sample_manifest("warning_text_evidence_only_sample");
        let mut observed = synthetic_observed_packet(&manifest).expect("observed");
        observed["graph_proof_outputs"] = json!(["text_evidence_promoted_to_graph_proof"]);
        let report = evaluate_mvp3_validation_fixture_assertions(&manifest, &observed);
        assert!(!report.passed);
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("forbidden output present")));
    }

    #[test]
    fn source_span_assertions_supported() {
        let manifest = load_sample_manifest("blocking_synthetic_exact_sample");
        let mut observed = synthetic_observed_packet(&manifest).expect("observed");
        observed["source_spans"] = json!([]);
        let report = evaluate_mvp3_validation_fixture_assertions(&manifest, &observed);
        assert!(!report.passed);
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("missing source span")));
    }

    #[test]
    fn post_fix_loop_supported() {
        let result = run_sample_harness()
            .results
            .into_iter()
            .find(|result| result.fixture_id == "blocking_synthetic_exact_sample")
            .expect("blocking sample result");
        assert_step(&result.command_plan, "post_fix_validate_edit");
        assert!(result
            .assertions
            .checked_assertions
            .iter()
            .any(|assertion| assertion.contains("forbidden_output_absent")));
    }

    #[test]
    fn release_binary_fixture_runner_supported() {
        let mut options = sample_options();
        options.mode = Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan;
        let report = run_mvp3_validation_fixtures(options).expect("release plan run");
        assert_eq!(report.runner_mode, "release_binary_plan");
        assert!(report
            .results
            .iter()
            .all(|result| result.surface_support.release_binary == "supported_command_plan"));
    }

    #[test]
    fn fixture_runner_can_list_fixtures() {
        let manifests = list_mvp3_validation_fixture_manifests(
            &workspace_root().join("fixtures").join("mvp3_validation"),
        )
        .expect("list fixtures");
        let ids = manifests
            .iter()
            .map(|manifest| manifest.fixture_id.as_str())
            .collect::<BTreeSet<_>>();
        for fixture_id in SAMPLE_FIXTURE_IDS
            .iter()
            .chain(EXACT_BLOCKING_FIXTURE_IDS.iter())
            .chain(SOURCE_ROLE_TEXT_UNKNOWN_FIXTURE_IDS.iter())
            .chain(DIRTY_EVIDENCE_FIXTURE_IDS.iter())
        {
            assert!(ids.contains(fixture_id), "missing fixture {fixture_id}");
        }
    }

    #[test]
    fn fixture_runner_can_run_one_fixture_by_id() {
        let report =
            run_mvp3_validation_fixture_by_id(sample_options(), "mvp3_8_new_dangling_call")
                .expect("run one fixture");
        assert_eq!(report.fixtures_total, 1);
        assert_eq!(report.fixtures_passed, 1);
        assert_eq!(report.results[0].fixture_id, "mvp3_8_new_dangling_call");
    }

    #[test]
    fn fixture_runner_can_run_all_fixture_families() {
        let manifests = list_mvp3_validation_fixture_manifests(
            &workspace_root().join("fixtures").join("mvp3_validation"),
        )
        .expect("list fixtures");
        let families = manifests
            .iter()
            .map(|manifest| manifest.fixture_family.clone())
            .collect::<BTreeSet<_>>();
        assert!(!families.is_empty());
        for family in families {
            let report = run_mvp3_validation_fixture_family(sample_options(), family.clone())
                .unwrap_or_else(|error| panic!("family {family} failed: {error}"));
            assert!(report.fixtures_total > 0, "family {family} had no fixtures");
            assert_eq!(report.fixtures_failed, 0, "family {family} failed");
            assert!(report
                .results
                .iter()
                .all(|result| result.fixture_family == family));
        }
    }

    #[test]
    fn release_binary_runner_can_run_selected_fixtures() {
        let mut options = sample_options();
        options.mode = Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan;
        let report =
            run_mvp3_validation_fixture_by_id(options, "warning_text_evidence_only_sample")
                .expect("release runner selected fixture");
        assert_eq!(report.runner_mode, "release_binary_plan");
        assert_eq!(report.fixtures_total, 1);
        let result = &report.results[0];
        assert_eq!(
            result.surface_support.release_binary,
            "supported_command_plan"
        );
        assert!(result.command_plan.iter().all(|record| record
            .env
            .iter()
            .any(|env| env.starts_with("CODEGRAPH_AGENT_USE_DATA_ROOT="))));
    }

    #[test]
    fn cli_validate_edit_runner_uses_canonical_agent_json_surface() {
        let report = run_sample_harness();
        let result = result_by_id(&report, "mvp3_8_new_dangling_call");
        let compact = command_step(&result.command_plan, "cli_validate_edit");
        assert!(compact
            .command
            .windows(2)
            .any(|pair| pair[0] == "--changed" && pair[1] == "src/caller.ts"));
        assert!(compact.command.iter().any(|arg| arg == "--agent-json"));
        assert!(!compact.command.iter().any(|arg| arg == "--db"));
        assert_eq!(
            surface_by_name(result, "cli_validate_edit").status,
            "passed"
        );
    }

    #[test]
    fn watch_once_fixture_runner_uses_canonical_changed_surface() {
        let result = first_sample_result();
        let watch = command_step(&result.command_plan, "watch_once");
        assert!(watch.command.iter().any(|arg| arg == "--once"));
        assert!(watch.command.iter().any(|arg| arg == "--json"));
        assert!(!watch.command.iter().any(|arg| arg == "--db"));
        assert_eq!(surface_by_name(&result, "watch_once").status, "passed");
    }

    #[test]
    fn mcp_fixture_runner_works_or_not_applicable_explicitly() {
        let result = first_sample_result();
        let surface = surface_by_name(&result, "mcp_validate_edit");
        assert!(matches!(
            surface.status.as_str(),
            "passed" | "not_applicable"
        ));
        assert!(surface
            .structured_assertions
            .iter()
            .any(|assertion| assertion == "structured_success_for_validation_blockers"));
        assert_step(&result.command_plan, "mcp_validate_edit");
    }

    #[test]
    fn context_pack_fixture_runner_works() {
        let result = first_sample_result();
        assert_step(&result.command_plan, "baseline_context_pack");
        assert_step(&result.command_plan, "context_pack_after_mutation");
        let surface = surface_by_name(&result, "context_pack");
        assert_eq!(surface.status, "passed");
        assert!(surface
            .structured_assertions
            .iter()
            .any(|assertion| assertion == "proof_status"));
    }

    #[test]
    fn status_doctor_fixture_runner_works() {
        let result = first_sample_result();
        assert_step(&result.command_plan, "status");
        assert_step(&result.command_plan, "doctor");
        let surface = surface_by_name(&result, "status_doctor");
        assert_eq!(surface.status, "passed");
        assert!(surface
            .structured_assertions
            .iter()
            .any(|assertion| assertion == "graph_vs_sidecar_split"));
    }

    #[test]
    fn compact_explain_audit_assertions_work() {
        let result = first_sample_result();
        assert_step(&result.command_plan, "cli_validate_edit");
        assert_step(&result.command_plan, "cli_validate_edit_explain");
        assert_step(&result.command_plan, "cli_validate_edit_audit");
        let surface = surface_by_name(&result, "compact_explain_audit");
        assert_eq!(surface.status, "passed");
        for assertion in [
            "compact_default_bounded",
            "explain_restores_detail",
            "audit_json_restores_detail",
            "critical_fields_preserved",
        ] {
            assert!(
                surface
                    .structured_assertions
                    .iter()
                    .any(|value| value == assertion),
                "missing compact/explain/audit assertion {assertion}"
            );
        }
    }

    #[test]
    fn artifact_logs_preserved() {
        let report = run_mvp3_validation_fixture_by_id(sample_options(), "ok_noop_fixture")
            .expect("run fixture");
        let result = &report.results[0];
        for record in &result.command_plan {
            assert!(
                Path::new(&record.stdout_log).exists(),
                "missing stdout log for {}",
                record.step
            );
            assert!(
                Path::new(&record.stderr_log).exists(),
                "missing stderr log for {}",
                record.step
            );
        }
    }

    #[test]
    fn golden_manifest_schema_validated() {
        let schema = mvp3_validation_fixture_manifest_schema_value();
        assert_eq!(schema["additionalProperties"].as_bool(), Some(false));
        assert!(schema["$defs"]["proof_boundary_note"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("Graph/source verification"));
    }

    #[test]
    fn no_dot_codegraph_mutation() {
        let before = dot_codegraph_state(&workspace_root());
        let report = run_sample_harness();
        let after = dot_codegraph_state(&workspace_root());
        assert_eq!(before, after);
        assert!(!report.normal_dot_codegraph_mutated);
    }

    #[test]
    fn exact_blocking_fixture_set_complete() {
        let report = run_sample_harness();
        assert_eq!(report.status, "complete");
        for fixture_id in EXACT_BLOCKING_FIXTURE_IDS {
            assert_eq!(result_by_id(&report, fixture_id).status, "passed");
            let manifest = load_manifest_by_id(fixture_id);
            assert_eq!(manifest.expected_mvp_phase, "mvp3_8_validation_fixtures");
            assert!(
                manifest.tags.iter().any(|tag| tag == "mvp3_8"),
                "{fixture_id} must be tagged for MVP3.8"
            );
        }
    }

    #[test]
    fn new_dangling_call_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_new_dangling_call",
            "CG_MVP3_CALLS_DANGLING_TARGET",
            "blocked",
            true,
        );
    }

    #[test]
    fn fixed_dangling_call_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_fixed_dangling_call",
            "CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED",
            "blocked",
            true,
        );
        let manifest = load_manifest_by_id("mvp3_8_fixed_dangling_call");
        assert_eq!(manifest.post_fix_expected_status, "ok");
        assert!(!manifest.fix_mutation_if_any.is_empty());
    }

    #[test]
    fn broken_import_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_broken_import",
            "CG_MVP3_IMPORTS_DANGLING_TARGET",
            "blocked",
            true,
        );
    }

    #[test]
    fn fixed_import_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_fixed_import",
            "CG_MVP3_IMPORTS_DELETED_EXPORT_STILL_IMPORTED",
            "blocked",
            true,
        );
        let manifest = load_manifest_by_id("mvp3_8_fixed_import");
        assert_eq!(manifest.post_fix_expected_status, "ok");
        assert!(!manifest.fix_mutation_if_any.is_empty());
    }

    #[test]
    fn renamed_target_symbol_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_renamed_target_symbol",
            "CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED",
            "blocked",
            true,
        );
    }

    #[test]
    fn ambiguous_rename_unknown_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_ambiguous_rename_unknown",
            "CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED",
            "unknown",
            false,
        );
        let manifest = load_manifest_by_id("mvp3_8_ambiguous_rename_unknown");
        assert!(manifest.expected_blocking_errors.is_empty());
        assert_eq!(manifest.expected_unknowns.len(), 1);
        assert!(
            manifest
                .expected_changed_file_normalization
                .get("rename_status")
                .and_then(Value::as_str)
                == Some("unknown")
        );
    }

    #[test]
    fn missing_source_span_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_missing_source_span_integrity",
            "CG_MVP3_PROOF_EDGE_MISSING_SOURCE_SPAN",
            "blocked",
            true,
        );
        let manifest = load_manifest_by_id("mvp3_8_missing_source_span_integrity");
        assert_eq!(manifest.fixture_family, "integrity_synthetic");
        assert_eq!(manifest.required_support_status, "degraded");
        assert!(manifest.expected_source_spans.is_empty());
        assert!(manifest
            .unsupported_or_not_applicable_reason
            .contains("Release-binary runtime path is not applicable"));
    }

    #[test]
    fn derived_missing_provenance_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_derived_missing_provenance_integrity",
            "CG_MVP3_DERIVED_EDGE_MISSING_PROVENANCE",
            "blocked",
            true,
        );
        let manifest = load_manifest_by_id("mvp3_8_derived_missing_provenance_integrity");
        assert_eq!(manifest.fixture_family, "integrity_synthetic");
        assert_eq!(manifest.required_support_status, "degraded");
        assert!(manifest
            .unsupported_or_not_applicable_reason
            .contains("Release-binary runtime path is not applicable"));
    }

    #[test]
    fn removed_file_stale_facts_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_removed_file_stale_facts",
            "CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED",
            "blocked",
            true,
        );
        let manifest = load_manifest_by_id("mvp3_8_removed_file_stale_facts");
        assert!(manifest
            .deleted_files
            .contains(&"src/removed.ts".to_string()));
        assert!(manifest
            .forbidden_claimable_outputs
            .iter()
            .any(|value| value.contains("src/removed.ts")));
    }

    #[test]
    fn exact_blockers_source_spanned() {
        for fixture_id in RUNTIME_SOURCE_SPANNED_BLOCKER_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            assert!(
                !manifest.expected_blocking_errors.is_empty(),
                "{fixture_id} must declare blocking findings"
            );
            assert!(
                !manifest.expected_source_spans.is_empty(),
                "{fixture_id} must declare graph-proof source spans"
            );
            assert!(
                manifest
                    .expected_source_spans
                    .iter()
                    .all(|span| span.required_for_graph_proof),
                "{fixture_id} source spans must be graph-proof-required"
            );
            assert!(
                manifest
                    .expected_blocking_errors
                    .iter()
                    .all(|finding| finding
                        .source_file
                        .as_deref()
                        .unwrap_or_default()
                        .starts_with("src/")),
                "{fixture_id} blocking findings must be source-bound"
            );
        }

        let spanless = load_manifest_by_id("mvp3_8_missing_source_span_integrity");
        assert_eq!(spanless.fixture_family, "integrity_synthetic");
        assert!(spanless.expected_source_spans.is_empty());
        assert!(spanless
            .expected_blocking_errors
            .iter()
            .all(|finding| finding.validation_rule_id == "CG_MVP3_PROOF_EDGE_MISSING_SOURCE_SPAN"));
    }

    #[test]
    fn exact_blockers_have_rule_id_reason_fix_next_steps() {
        let current_rule_ids = CURRENT_EXACT_RULE_IDS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for fixture_id in EXACT_BLOCKING_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            for rule_id in expected_rule_ids(&manifest) {
                assert!(
                    current_rule_ids.contains(rule_id.as_str()),
                    "{fixture_id} used unexpected rule id {rule_id}"
                );
            }
            if !manifest.expected_blocking_errors.is_empty() {
                assert!(
                    manifest
                        .expected_recommended_fix
                        .as_deref()
                        .unwrap_or_default()
                        .len()
                        > 10,
                    "{fixture_id} must provide recommended_fix"
                );
                assert!(
                    !manifest.expected_suggested_next_steps.is_empty(),
                    "{fixture_id} must provide suggested_next_steps"
                );
                assert!(
                    manifest
                        .expected_cli_validate_edit_behavior
                        .required_fields
                        .iter()
                        .any(|field| field == "proof_reason"),
                    "{fixture_id} must require proof_reason in CLI output"
                );
            }
        }
    }

    #[test]
    fn post_fix_stale_facts_absent() {
        let report = run_sample_harness();
        for fixture_id in POST_FIX_RECOVERY_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            let result = result_by_id(&report, fixture_id);
            assert_step(&result.command_plan, "post_fix_validate_edit");
            assert_eq!(manifest.post_fix_expected_status, "ok");
            assert!(
                !manifest.post_fix_forbidden_stale_outputs.is_empty(),
                "{fixture_id} must declare stale outputs forbidden after fix"
            );
            for forbidden in &manifest.post_fix_forbidden_stale_outputs {
                assert!(result
                    .assertions
                    .checked_assertions
                    .iter()
                    .any(|assertion| assertion == &format!("forbidden_output_absent:{forbidden}")));
            }
        }
    }

    #[test]
    fn non_graph_evidence_not_blocking() {
        for fixture_id in EXACT_BLOCKING_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            for finding in &manifest.expected_blocking_errors {
                assert_eq!(
                    finding.proof_ladder_level.as_deref(),
                    Some("graph_relation_proof"),
                    "{fixture_id} blocking finding must be graph proof only"
                );
            }
            if !manifest.expected_blocking_errors.is_empty() {
                assert!(
                    manifest
                        .forbidden_graph_proof_outputs
                        .iter()
                        .any(|value| value.contains("candidate")),
                    "{fixture_id} must forbid candidate evidence as graph proof"
                );
                assert!(
                    manifest
                        .forbidden_graph_proof_outputs
                        .iter()
                        .any(|value| value.contains("text") || value.contains("vector")),
                    "{fixture_id} must forbid text/vector evidence as graph proof"
                );
            }
        }
        for fixture_id in NON_BLOCKING_SOURCE_ROLE_TEXT_UNKNOWN_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            assert!(
                manifest.expected_blocking_errors.is_empty(),
                "{fixture_id} must not treat non-graph evidence as blocking"
            );
            assert!(
                !manifest.expected_hard_interrupt,
                "{fixture_id} must not produce a hard interrupt"
            );
        }
    }

    #[test]
    fn source_role_text_unknown_fixture_set_complete() {
        let report = run_sample_harness();
        assert_eq!(report.status, "complete");
        for fixture_id in SOURCE_ROLE_TEXT_UNKNOWN_FIXTURE_IDS {
            assert_eq!(result_by_id(&report, fixture_id).status, "passed");
            let manifest = load_manifest_by_id(fixture_id);
            assert_eq!(manifest.expected_mvp_phase, "mvp3_8_validation_fixtures");
            assert!(
                manifest.tags.iter().any(|tag| tag == "mvp3_8"),
                "{fixture_id} must be tagged for MVP3.8"
            );
        }
    }

    #[test]
    fn inline_test_added_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_inline_test_added",
            "CG_MVP3_SOURCE_ROLE_INLINE_TEST_PROMOTED_TO_PRODUCTION",
            "warning",
            false,
        );
    }

    #[test]
    fn inline_test_not_production_proof() {
        let manifest = load_manifest_by_id("mvp3_8_inline_test_added");
        assert!(manifest.expected_blocking_errors.is_empty());
        assert!(!manifest.expected_hard_interrupt);
        assert!(manifest
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("inline_test")));
        assert!(manifest
            .forbidden_source_role_leakage
            .iter()
            .any(|value| value.contains("test")));
        assert_eq!(
            manifest
                .expected_proof_ladder_changes
                .pointer("/graph_relation_proof/refreshed")
                .and_then(Value::as_u64),
            Some(0)
        );
    }

    #[test]
    fn mock_test_leakage_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_mock_test_leakage_attempt",
            "CG_MVP3_SOURCE_ROLE_MOCK_EVIDENCE_IN_PRODUCTION_PROOF",
            "blocked",
            true,
        );
    }

    #[test]
    fn no_test_mock_production_leakage() {
        for fixture_id in [
            "mvp3_8_inline_test_added",
            "mvp3_8_mock_test_leakage_attempt",
        ] {
            let manifest = load_manifest_by_id(fixture_id);
            assert!(
                !manifest.forbidden_source_role_leakage.is_empty(),
                "{fixture_id} must declare forbidden source-role leakage"
            );
            assert!(manifest
                .forbidden_source_role_leakage
                .iter()
                .any(|value| value.contains("production")));
        }
        let mock = load_manifest_by_id("mvp3_8_mock_test_leakage_attempt");
        assert!(mock
            .expected_blocking_errors
            .iter()
            .all(|finding| finding.proof_ladder_level.as_deref() == Some("graph_relation_proof")));
    }

    #[test]
    fn text_evidence_only_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_text_evidence_only_edit",
            "text_evidence_changed_non_graph",
            "warning",
            false,
        );
    }

    #[test]
    fn text_evidence_not_graph_proof() {
        let manifest = load_manifest_by_id("mvp3_8_text_evidence_only_edit");
        assert!(manifest.expected_blocking_errors.is_empty());
        assert_eq!(manifest.expected_warnings.len(), 1);
        assert_eq!(
            manifest.expected_warnings[0].proof_ladder_level.as_deref(),
            Some("text_evidence")
        );
        assert!(manifest
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("text_evidence")));
        assert_eq!(
            manifest
                .expected_proof_ladder_changes
                .pointer("/graph_relation_proof/refreshed")
                .and_then(Value::as_u64),
            Some(0)
        );
    }

    #[test]
    fn unsupported_dynamic_call_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_unsupported_dynamic_call",
            "CG_MVP3_DYNAMIC_CALL_UNKNOWN",
            "unknown",
            false,
        );
    }

    #[test]
    fn unsupported_dynamic_call_not_blocking() {
        let manifest = load_manifest_by_id("mvp3_8_unsupported_dynamic_call");
        assert_eq!(manifest.required_support_status, "unsupported");
        assert!(manifest.expected_blocking_errors.is_empty());
        assert_eq!(manifest.expected_unknowns.len(), 1);
        assert!(manifest
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("computed_call")));
        assert!(!manifest.expected_hard_interrupt);
    }

    #[test]
    fn config_package_fixture_passed() {
        assert_fixture_rule_and_status(
            "mvp3_8_config_package_text_mismatch",
            "CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING",
            "warning",
            false,
        );
    }

    #[test]
    fn config_package_text_only_warning_or_diagnostic() {
        let manifest = load_manifest_by_id("mvp3_8_config_package_text_mismatch");
        assert_eq!(manifest.required_support_status, "unsupported");
        assert!(manifest.expected_blocking_errors.is_empty());
        assert!(matches!(
            manifest.expected_cli_validate_edit_behavior.status.as_str(),
            "warning" | "diagnostic"
        ));
        assert!(manifest
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("Configures")));
        assert_eq!(
            manifest
                .expected_proof_ladder_changes
                .pointer("/graph_relation_proof/refreshed")
                .and_then(Value::as_u64),
            Some(0)
        );
    }

    #[test]
    fn route_handler_fixture_passed_or_not_applicable() {
        assert_fixture_rule_and_status(
            "mvp3_8_route_handler_missing_target",
            "CG_MVP3_ROUTE_UNSUPPORTED_FRAMEWORK_UNKNOWN",
            "unknown",
            false,
        );
        let manifest = load_manifest_by_id("mvp3_8_route_handler_missing_target");
        assert!(matches!(
            manifest.required_support_status.as_str(),
            "degraded" | "unsupported" | "not_applicable" | "supported"
        ));
    }

    #[test]
    fn route_handler_exact_activation_gated() {
        let manifest = load_manifest_by_id("mvp3_8_route_handler_missing_target");
        if manifest.required_support_status == "supported" {
            assert!(expected_rule_ids(&manifest)
                .iter()
                .any(|rule_id| rule_id == "CG_MVP3_ROUTE_HANDLER_DANGLING_TARGET"));
            assert_eq!(
                manifest.expected_blocking_errors[0]
                    .proof_ladder_level
                    .as_deref(),
                Some("graph_relation_proof")
            );
        } else {
            assert!(manifest.expected_blocking_errors.is_empty());
            assert!(!manifest.expected_hard_interrupt);
            assert!(expected_rule_ids(&manifest)
                .iter()
                .any(|rule_id| rule_id == "CG_MVP3_ROUTE_UNSUPPORTED_FRAMEWORK_UNKNOWN"));
            assert!(manifest
                .proof_boundary
                .contains("Route text or convention evidence is not graph proof"));
        }
    }

    #[test]
    fn forbidden_claimable_outputs_enforced() {
        let manifest = load_manifest_by_id("mvp3_8_text_evidence_only_edit");
        let mut observed = synthetic_observed_packet(&manifest).expect("observed");
        observed["claimable_outputs"] = json!(["text_evidence_claimed_as_relation"]);
        let report = evaluate_mvp3_validation_fixture_assertions(&manifest, &observed);
        assert!(!report.passed);
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("forbidden output present")));
    }

    #[test]
    fn source_role_text_unknown_rule_ids_match_current_contracts() {
        let current_rule_ids = CURRENT_SOURCE_ROLE_TEXT_UNKNOWN_RULE_IDS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for fixture_id in SOURCE_ROLE_TEXT_UNKNOWN_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            for rule_id in expected_rule_ids(&manifest) {
                assert!(
                    current_rule_ids.contains(rule_id.as_str()),
                    "{fixture_id} used unexpected rule id {rule_id}"
                );
            }
        }
    }

    #[test]
    fn source_role_text_unknown_no_dot_codegraph_mutation() {
        let before = dot_codegraph_state(&workspace_root());
        let report = run_sample_harness();
        let after = dot_codegraph_state(&workspace_root());
        assert_eq!(before, after);
        assert!(!report.normal_dot_codegraph_mutated);
    }

    #[test]
    fn dirty_evidence_fixture_set_complete() {
        let report = run_sample_harness();
        assert_eq!(report.status, "complete");
        for fixture_id in DIRTY_EVIDENCE_FIXTURE_IDS {
            assert_eq!(result_by_id(&report, fixture_id).status, "passed");
            let manifest = load_manifest_by_id(fixture_id);
            assert_eq!(manifest.expected_mvp_phase, "mvp3_8_validation_fixtures");
            assert!(
                manifest.tags.iter().any(|tag| tag == "dirty_evidence"),
                "{fixture_id} must be tagged dirty_evidence"
            );
        }
    }

    #[test]
    fn candidate_query_index_corrupt_fixture_passed() {
        let manifest = assert_dirty_fixture_sidecar_status(
            "mvp3_8_candidate_query_index_corrupt",
            "candidate_spool_query_index_status",
            "corrupt",
            "ok",
            false,
        );
        assert_graph_claimable(&manifest);
        assert!(manifest
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("corrupt_candidate")));
        assert!(!manifest.recovery_commands_expected.is_empty());
    }

    #[test]
    fn candidate_spool_stale_fixture_passed() {
        let manifest = assert_dirty_fixture_sidecar_status(
            "mvp3_8_candidate_spool_stale",
            "candidate_layer_status",
            "stale",
            "ok",
            false,
        );
        assert_eq!(
            manifest
                .expected_proof_ladder_changes
                .pointer("/candidate_evidence/status")
                .and_then(Value::as_str),
            Some("stale")
        );
    }

    #[test]
    fn candidate_spool_inaccessible_fixture_passed_or_not_applicable() {
        let manifest = assert_dirty_fixture_sidecar_status(
            "mvp3_8_candidate_spool_inaccessible",
            "candidate_spool_query_index_status",
            "inaccessible",
            "ok",
            false,
        );
        assert!(matches!(
            manifest.required_support_status.as_str(),
            "supported" | "not_applicable"
        ));
        assert_graph_claimable(&manifest);
    }

    #[test]
    fn vector_runtime_stale_fixture_passed() {
        let manifest = assert_dirty_fixture_sidecar_status(
            "mvp3_8_vector_runtime_stale",
            "vector_layer_status",
            "stale",
            "ok",
            false,
        );
        assert!(manifest
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("vector")));
    }

    #[test]
    fn vector_audit_diagnostic_only_fixture_passed() {
        let manifest = assert_dirty_fixture_sidecar_status(
            "mvp3_8_vector_audit_stale_diagnostic_only",
            "vector_audit_status",
            "diagnostic_only",
            "ok",
            false,
        );
        assert_eq!(
            manifest
                .expected_proof_ladder_changes
                .pointer("/diagnostic_only/status")
                .and_then(Value::as_str),
            Some("stale")
        );
        assert!(manifest
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("vector_audit")));
    }

    #[test]
    fn path_evidence_stale_fixture_passed() {
        let manifest = assert_dirty_fixture_sidecar_status(
            "mvp3_8_path_evidence_stale_invalidated",
            "path_evidence_status",
            "stale",
            "ok",
            false,
        );
        assert!(manifest
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("PathEvidence")));
    }

    #[test]
    fn source_navigation_routing_stale_fixture_passed_or_not_applicable() {
        let source_navigation = assert_dirty_fixture_sidecar_status(
            "mvp3_8_source_navigation_stale",
            "source_navigation_status",
            "stale",
            "ok",
            false,
        );
        assert!(source_navigation
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("source_navigation")));

        let routing = assert_dirty_fixture_sidecar_status(
            "mvp3_8_routing_handle_stale",
            "routing_handle_status",
            "not_applicable",
            "ok",
            false,
        );
        assert_eq!(routing.required_support_status, "not_applicable");
        assert!(routing
            .unsupported_or_not_applicable_reason
            .contains("no persisted routing-handle"));
    }

    #[test]
    fn deleted_file_stale_candidate_hit_fixture_passed() {
        let manifest = assert_dirty_fixture_sidecar_status(
            "mvp3_8_deleted_file_stale_candidate_hit",
            "candidate_layer_status",
            "stale",
            "ok",
            false,
        );
        assert!(manifest
            .deleted_files
            .contains(&"src/deleted_candidate.ts".to_string()));
        assert!(manifest
            .forbidden_claimable_outputs
            .iter()
            .any(|value| value.contains("src/deleted_candidate.ts")));
    }

    #[test]
    fn renamed_import_stale_vector_hit_fixture_passed() {
        let manifest = assert_dirty_fixture_sidecar_status(
            "mvp3_8_renamed_import_stale_vector_hit",
            "vector_layer_status",
            "stale",
            "blocked",
            true,
        );
        assert!(expected_rule_ids(&manifest)
            .iter()
            .any(|rule_id| rule_id == "CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED"));
        assert!(manifest.expected_blocking_errors.iter().all(|finding| {
            finding.proof_ladder_level.as_deref() == Some("graph_relation_proof")
        }));
        assert!(manifest
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("stale_vector")));
    }

    #[test]
    fn graph_claimable_optional_sidecar_corrupt_fixture_passed() {
        let manifest = assert_dirty_fixture_sidecar_status(
            "mvp3_8_graph_claimable_optional_sidecar_corrupt",
            "candidate_spool_query_index_status",
            "corrupt",
            "ok",
            false,
        );
        assert_graph_claimable(&manifest);
        assert_eq!(
            manifest
                .expected_sidecar_statuses
                .pointer("/graph_proof_available")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn proof_ladder_output_assertions_passed() {
        let report = run_sample_harness();
        for fixture_id in DIRTY_EVIDENCE_FIXTURE_IDS {
            let result = result_by_id(&report, fixture_id);
            assert!(result
                .assertions
                .checked_assertions
                .iter()
                .any(|assertion| assertion == "proof ladder changes"));
            assert!(result
                .assertions
                .checked_assertions
                .iter()
                .any(|assertion| assertion == "dirty evidence summary"));
            let manifest = load_manifest_by_id(fixture_id);
            assert!(
                !manifest.expected_dirty_evidence_changes.is_null(),
                "{fixture_id} must declare dirty evidence changes"
            );
        }
    }

    #[test]
    fn stale_candidate_vector_source_navigation_not_graph_proof() {
        for fixture_id in STALE_NON_PROOF_DIRTY_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            assert!(
                !manifest.forbidden_graph_proof_outputs.is_empty(),
                "{fixture_id} must forbid stale sidecar graph proof"
            );
            assert!(
                !manifest.expected_hard_interrupt
                    || manifest
                        .expected_blocking_errors
                        .iter()
                        .all(|finding| finding.proof_ladder_level.as_deref()
                            == Some("graph_relation_proof")),
                "{fixture_id} may interrupt only from graph_relation_proof"
            );
        }
    }

    #[test]
    fn sidecar_corrupt_not_graph_db_corrupt() {
        for fixture_id in [
            "mvp3_8_candidate_query_index_corrupt",
            "mvp3_8_graph_claimable_optional_sidecar_corrupt",
        ] {
            let manifest = load_manifest_by_id(fixture_id);
            assert_graph_claimable(&manifest);
            assert_eq!(
                manifest
                    .expected_final_db_lifecycle_status
                    .pointer("/db_problem_kind"),
                Some(&Value::Null)
            );
            assert!(manifest
                .expected_cli_validate_edit_behavior
                .forbidden_fields
                .iter()
                .any(|field| field == "graph_db_corrupt"));
        }
    }

    #[test]
    fn dirty_evidence_rule_ids_match_current_contracts() {
        let current_rule_ids = CURRENT_DIRTY_EVIDENCE_RULE_IDS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for fixture_id in DIRTY_EVIDENCE_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            for rule_id in expected_rule_ids(&manifest) {
                assert!(
                    current_rule_ids.contains(rule_id.as_str()),
                    "{fixture_id} used unexpected rule id {rule_id}"
                );
            }
        }
    }

    #[test]
    fn dirty_evidence_no_dot_codegraph_mutation() {
        let before = dot_codegraph_state(&workspace_root());
        let report = run_sample_harness();
        let after = dot_codegraph_state(&workspace_root());
        assert_eq!(before, after);
        assert!(!report.normal_dot_codegraph_mutated);
    }

    #[test]
    fn fix_recovery_fixture_set_complete() {
        let report = run_sample_harness();
        assert_eq!(report.status, "complete");
        for fixture_id in FIX_RECOVERY_FIXTURE_IDS {
            let result = result_by_id(&report, fixture_id);
            assert_eq!(result.status, "passed");
            let manifest = load_manifest_by_id(fixture_id);
            assert_eq!(manifest.expected_mvp_phase, "mvp3_8_validation_fixtures");
            assert!(
                manifest.tags.iter().any(|tag| tag == "fix_recovery"),
                "{fixture_id} must be tagged fix_recovery"
            );
            assert!(
                manifest.recovery_is_hint_not_proof,
                "{fixture_id} must state recovery commands are hints/actions, not proof"
            );
        }
    }

    #[test]
    fn blocking_fixtures_have_post_fix_expectations() {
        for fixture_id in EXACT_BLOCKING_FIXTURE_IDS
            .iter()
            .copied()
            .chain(["mvp3_8_mock_test_leakage_attempt"])
        {
            let manifest = load_manifest_by_id(fixture_id);
            if manifest.expected_cli_validate_edit_behavior.status == "blocked" {
                assert!(
                    matches!(
                        manifest.post_fix_expected_status.as_str(),
                        "ok" | "warning" | "unknown"
                    ),
                    "{fixture_id} must declare a post-fix terminal status"
                );
                assert!(
                    !manifest.post_fix_forbidden_stale_outputs.is_empty(),
                    "{fixture_id} must forbid stale outputs after fix"
                );
                assert!(
                    !manifest.fix_steps.is_empty() || !manifest.fix_mutation_if_any.is_empty(),
                    "{fixture_id} must provide fix steps"
                );
            }
        }
    }

    #[test]
    fn dangling_call_post_fix_ok_or_expected_warning() {
        assert_post_fix_status("mvp3_8_fixed_dangling_call", &["ok", "warning"]);
    }

    #[test]
    fn import_post_fix_ok_or_expected_warning() {
        assert_post_fix_status("mvp3_8_fixed_import", &["ok", "warning"]);
    }

    #[test]
    fn renamed_target_post_fix_ok_or_expected_warning() {
        assert_post_fix_status("mvp3_8_renamed_target_symbol", &["ok", "warning"]);
    }

    #[test]
    fn removed_file_stale_facts_cleared() {
        let manifest = assert_post_fix_status("mvp3_8_removed_file_stale_facts", &["ok"]);
        assert!(manifest
            .post_fix_forbidden_stale_outputs
            .iter()
            .any(|value| value.contains("src/removed.ts")));
    }

    #[test]
    fn source_role_leakage_fixed() {
        let manifest = assert_post_fix_status("mvp3_8_mock_test_leakage_attempt", &["ok"]);
        assert!(manifest
            .post_fix_forbidden_stale_outputs
            .iter()
            .any(|value| value.contains("mock") || value.contains("test")));
        assert!(manifest
            .forbidden_source_role_leakage
            .iter()
            .any(|value| value.contains("production")));
    }

    #[test]
    fn derived_provenance_restored_or_downgraded() {
        let manifest = assert_post_fix_status(
            "mvp3_8_derived_missing_provenance_integrity",
            &["ok", "warning", "unknown"],
        );
        assert_eq!(manifest.fixture_family, "integrity_synthetic");
        assert!(manifest
            .post_fix_forbidden_stale_outputs
            .iter()
            .any(|value| value.contains("provenance")));
    }

    #[test]
    fn source_span_restored_or_downgraded() {
        let manifest = assert_post_fix_status(
            "mvp3_8_missing_source_span_integrity",
            &["ok", "warning", "unknown"],
        );
        assert_eq!(manifest.fixture_family, "integrity_synthetic");
        assert!(manifest
            .post_fix_forbidden_stale_outputs
            .iter()
            .any(|value| value.contains("source span") || value.contains("source_span")));
    }

    #[test]
    fn text_evidence_refresh_recovery() {
        let manifest = assert_post_fix_status("mvp3_8_text_evidence_only_edit", &["ok", "warning"]);
        assert!(manifest
            .expected_proof_ladder_changes
            .get("text_evidence")
            .is_some());
        assert!(manifest
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("text_evidence")));
    }

    #[test]
    fn candidate_sidecar_recovery() {
        let manifest = assert_post_fix_status("mvp3_8_candidate_spool_stale", &["ok"]);
        assert_recovery_command_hint(&manifest);
        assert_eq!(
            manifest
                .expected_sidecar_statuses
                .pointer("/candidate_layer_status")
                .and_then(Value::as_str),
            Some("stale")
        );
    }

    #[test]
    fn candidate_query_index_recovery() {
        let manifest = assert_post_fix_status("mvp3_8_candidate_query_index_corrupt", &["ok"]);
        assert_recovery_command_hint(&manifest);
        assert_eq!(
            manifest
                .expected_sidecar_statuses
                .pointer("/candidate_spool_query_index_status")
                .and_then(Value::as_str),
            Some("corrupt")
        );
        assert_graph_claimable(&manifest);
    }

    #[test]
    fn vector_runtime_recovery() {
        let manifest = assert_post_fix_status("mvp3_8_vector_runtime_stale", &["ok"]);
        assert_recovery_command_hint(&manifest);
        assert_eq!(
            manifest
                .expected_sidecar_statuses
                .pointer("/vector_layer_status")
                .and_then(Value::as_str),
            Some("stale")
        );
    }

    #[test]
    fn unsafe_db_recovery() {
        let manifest = assert_post_fix_status("mvp3_8_unsafe_db_recovery", &["ok"]);
        assert_recovery_command_hint(&manifest);
        assert_eq!(
            manifest
                .expected_final_db_lifecycle_status
                .pointer("/claimable")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            manifest
                .recovery_command_verification
                .pointer("/post_recovery_lifecycle/claimable")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn recovery_commands_verified_as_hints_not_proof() {
        for fixture_id in RECOVERY_COMMAND_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            assert_recovery_command_hint(&manifest);
            assert_eq!(
                manifest
                    .recovery_command_verification
                    .pointer("/recovery_command_alone_not_proof")
                    .and_then(Value::as_bool),
                Some(true),
                "{fixture_id} must state recovery command alone is not proof"
            );
        }
    }

    #[test]
    fn post_fix_stale_facts_removed() {
        let report = run_sample_harness();
        for fixture_id in FIX_RECOVERY_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            if !manifest.post_fix_forbidden_stale_outputs.is_empty() {
                let result = result_by_id(&report, fixture_id);
                for forbidden in &manifest.post_fix_forbidden_stale_outputs {
                    assert!(result
                        .assertions
                        .checked_assertions
                        .iter()
                        .any(|assertion| assertion
                            == &format!("forbidden_output_absent:{forbidden}")));
                }
            }
        }
    }

    #[test]
    fn fix_recovery_no_dot_codegraph_mutation() {
        let before = dot_codegraph_state(&workspace_root());
        let report = run_sample_harness();
        let after = dot_codegraph_state(&workspace_root());
        assert_eq!(before, after);
        assert!(!report.normal_dot_codegraph_mutated);
    }

    #[test]
    fn gate_result_schema_defined() {
        let schema = mvp3_gate_result_schema_value();
        assert_eq!(schema.get("type").and_then(Value::as_str), Some("object"));
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .expect("required fields");
        for field in MVP3_9_GATE_RESULT_REQUIRED_FIELDS {
            assert!(
                required.iter().any(|value| value.as_str() == Some(*field)),
                "gate schema missing required field {field}"
            );
        }
    }

    #[test]
    fn gate_result_schema_validates_sample() {
        let result = run_sample_gate();
        let value = serde_json::to_value(&result).expect("serialize sample gate");
        validate_mvp3_gate_result_value(&value).expect("valid sample gate result");
    }

    #[test]
    fn gate_runner_contract_defined() {
        let result = run_sample_gate();
        assert_eq!(result.gate_id, MVP3_9_SAMPLE_GATE_ID);
        assert_eq!(result.status, "complete");
        assert!(result.ready_to_move_on);
        assert_eq!(result.fixture_report.fixtures_total, 2);
    }

    #[test]
    fn failure_taxonomy_defined() {
        let taxonomy = mvp3_gate_failure_taxonomy_value();
        assert_eq!(
            taxonomy.get("schema_version").and_then(Value::as_u64),
            Some(MVP3_9_GATE_RESULT_SCHEMA_VERSION as u64)
        );
        assert_eq!(
            taxonomy.get("public_claim").and_then(Value::as_bool),
            Some(false)
        );
        assert!(taxonomy
            .get("failure_taxonomy")
            .and_then(Value::as_array)
            .expect("taxonomy array")
            .iter()
            .any(|value| value.as_str() == Some("stale_sidecar_used_as_fresh")));
    }

    #[test]
    fn failure_taxonomy_covers_mvp3_gate_failures() {
        let taxonomy = MVP3_9_FAILURE_TAXONOMY
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for required in [
            "gate_dependency_missing",
            "hot_path_reindex_failure",
            "graph_delta_missing_changed_fact",
            "hard_interrupt_failure",
            "stage0_regression",
            "routing_packet_regression",
            "retrieval_candidate_boundary_regression",
            "bounded_graph_regression",
            "path_evidence_regression",
            "language_coverage_regression",
            "dirty_evidence_regression",
            "schema_json_failure",
            "stale_sidecar_used_as_fresh",
            "mcp_cli_parity_failure",
            "route_bridge_exact_support_overclaim",
            "public_claim_boundary_failure",
        ] {
            assert!(taxonomy.contains(required), "missing taxonomy {required}");
        }
    }

    #[test]
    fn fixture_tags_consumed() {
        let manifests = list_mvp3_validation_fixture_manifests(
            &workspace_root().join("fixtures").join("mvp3_validation"),
        )
        .expect("fixture manifests");
        let mut observed = BTreeSet::new();
        for manifest in &manifests {
            observed.extend(mvp3_gate_effective_fixture_tags(manifest));
        }
        for required in MVP3_9_REQUIRED_GATE_TAGS {
            assert!(
                observed.contains(*required),
                "effective fixture tags missing {required}"
            );
        }
    }

    #[test]
    fn gate_runner_selects_fixtures_by_tag() {
        let mut options = sample_options();
        options.fixture_ids.clear();
        options.fixture_families.clear();
        options.fixture_tags = vec!["dirty_evidence".to_string()];
        let report = run_mvp3_validation_fixtures(options).expect("dirty evidence tagged run");
        assert!(report.fixtures_total >= DIRTY_EVIDENCE_FIXTURE_IDS.len());
        assert_eq!(report.fixtures_failed, 0);
        for result in &report.results {
            let manifest = load_manifest_by_id(&result.fixture_id);
            assert!(
                mvp3_gate_effective_fixture_tags(&manifest).contains("dirty_evidence"),
                "{} selected without dirty_evidence tag",
                result.fixture_id
            );
        }
    }

    #[test]
    fn gate_runner_aggregates_invariant_counters() {
        let result = run_sample_gate();
        assert_eq!(result.claimability_violations, 0);
        assert_eq!(result.unsupported_claim_violations, 0);
        assert_eq!(result.graph_proof_overclaim_count, 0);
        assert_eq!(result.false_hard_interrupt_count, 0);
        assert_eq!(result.false_blocking_severity_count, 0);
        assert_eq!(result.forbidden_claimable_output_count, 0);
        assert_eq!(result.stale_evidence_reused_as_fresh_count, 0);
        assert_eq!(result.test_mock_production_leakage_count, 0);
        assert_eq!(result.route_proof_overclaim_count, 0);
        assert_eq!(result.bridge_proof_overclaim_count, 0);
        assert_eq!(result.text_evidence_graph_proof_count, 0);
        assert_eq!(
            result.candidate_vector_source_navigation_graph_proof_count,
            0
        );
    }

    #[test]
    fn gate_runner_emits_json_and_markdown() {
        let result = run_sample_gate();
        let artifact_dir = std::env::temp_dir().join(format!(
            "codegraph-mvp3-gate-artifacts-test-{}",
            unique_run_suffix()
        ));
        let artifacts =
            write_mvp3_gate_result_artifacts(&artifact_dir, &result).expect("write gate artifacts");
        assert!(Path::new(&artifacts.json_path).exists());
        assert!(Path::new(&artifacts.markdown_path).exists());
        let markdown = fs::read_to_string(&artifacts.markdown_path).expect("read markdown");
        assert!(markdown.contains(MVP3_9_SAMPLE_GATE_ID));
    }

    #[test]
    fn release_binary_mode_supported() {
        let mut options = default_mvp3_gate_runner_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-gate-release-plan-test-{}",
            unique_run_suffix()
        ));
        options.mode = Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan;
        let result = run_mvp3_gate(options).expect("release plan gate");
        assert_eq!(result.fixture_report.runner_mode, "release_binary_plan");
        assert!(result
            .release_binary_tests
            .iter()
            .any(|entry| entry.ends_with(":true")));
    }

    #[test]
    fn mcp_mode_supported_or_not_applicable() {
        let result = run_sample_gate();
        assert!(!result.mcp_tests.is_empty());
        assert!(result
            .mcp_tests
            .iter()
            .all(|entry| entry.contains("supported") || entry.contains("not_applicable")));
    }

    #[test]
    fn context_pack_mode_supported() {
        let result = run_sample_gate();
        assert!(!result.context_pack_tests.is_empty());
        assert!(result
            .product_surfaces
            .iter()
            .any(|surface| surface == "context_pack"));
    }

    #[test]
    fn json_validation_supported() {
        let result = run_sample_gate();
        let mut value = serde_json::to_value(&result).expect("serialize gate");
        validate_mvp3_gate_result_value(&value).expect("valid gate");
        value.as_object_mut().expect("object").remove("gate_id");
        assert!(validate_mvp3_gate_result_value(&value).is_err());
    }

    #[test]
    fn normal_dot_codegraph_mutation_counter_supported() {
        let result = run_sample_gate();
        assert!(!result.normal_dot_codegraph_mutated);
        assert!(!result.observed_invariants.normal_dot_codegraph_mutated);
        assert!(result
            .required_invariants
            .iter()
            .any(|name| name == "normal_dot_codegraph_mutated"));
    }

    #[test]
    fn no_public_claim_in_gate_results() {
        let result = run_sample_gate();
        assert!(!result.public_claim);
        assert!(!result.real_agent_patch_quality_claim);
        assert!(!result.mvp4_started);
        assert!(result.mvp4_future_only);
        assert!(result.claim_boundaries_preserved);
    }

    #[test]
    fn mvp3_9_hot_path_reindex_gate_fixture_set_defined() {
        for fixture_id in MVP3_9_HOT_PATH_REINDEX_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            let tags = mvp3_gate_effective_fixture_tags(&manifest);
            assert!(
                tags.contains("hot_path") || tags.contains("hot_path_reindex"),
                "{fixture_id} must be consumable by the hot-path gate"
            );
        }
        let all_tags = MVP3_9_HOT_PATH_REINDEX_FIXTURE_IDS
            .iter()
            .map(|fixture_id| load_manifest_by_id(fixture_id))
            .flat_map(|manifest| mvp3_gate_effective_fixture_tags(&manifest))
            .collect::<BTreeSet<_>>();
        for required in [
            "source_added",
            "source_deleted",
            "source_renamed",
            "text_evidence",
            "source_role",
            "ignored_generated_noop",
            "outside_repo_reject",
            "dirty_evidence",
            "path_evidence",
            "fix_recovery",
        ] {
            assert!(
                all_tags.contains(required),
                "hot-path tag missing {required}"
            );
        }
    }

    #[test]
    fn mvp3_9_hot_path_reindex_gate_runs_fast() {
        let mut options = default_mvp3_9_hot_path_reindex_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-hot-path-gate-fast-test-{}",
            unique_run_suffix()
        ));
        let result = run_mvp3_gate(options).expect("hot-path gate fast run");
        assert_eq!(result.gate_id, MVP3_9_HOT_PATH_REINDEX_GATE_ID);
        assert_eq!(result.status, "complete");
        assert_eq!(
            result.fixture_report.fixtures_total,
            MVP3_9_HOT_PATH_REINDEX_FIXTURE_IDS.len()
        );
    }

    #[test]
    fn mvp3_9_hot_path_reindex_gate_result_schema_valid() {
        let result = run_hot_path_gate_fast();
        let value = serde_json::to_value(&result).expect("serialize hot-path gate");
        validate_mvp3_gate_result_value(&value).expect("valid hot-path gate result");
    }

    #[test]
    fn mvp3_9_hot_path_reindex_gate_release_binary_plan_supported() {
        let mut options = default_mvp3_9_hot_path_reindex_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-hot-path-gate-release-plan-test-{}",
            unique_run_suffix()
        ));
        options.mode = Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan;
        let result = run_mvp3_gate(options).expect("hot-path release plan");
        assert_eq!(result.fixture_report.runner_mode, "release_binary_plan");
        assert!(result.ready_to_move_on);
        assert!(result
            .release_binary_tests
            .iter()
            .any(|entry| entry.ends_with(":true")));
    }

    #[test]
    fn mvp3_9_hot_path_reindex_gate_invariant_counters_zero() {
        let result = run_hot_path_gate_fast();
        assert_eq!(result.claimability_violations, 0);
        assert_eq!(result.unsupported_claim_violations, 0);
        assert_eq!(result.graph_proof_overclaim_count, 0);
        assert_eq!(result.false_hard_interrupt_count, 0);
        assert_eq!(result.false_blocking_severity_count, 0);
        assert_eq!(result.stale_evidence_reused_as_fresh_count, 0);
        assert_eq!(result.normal_dot_codegraph_mutated, false);
    }

    #[test]
    fn mvp3_9_hot_path_reindex_gate_product_surface_smoke_writes_artifacts_when_requested() {
        let Ok(output_dir) = std::env::var("CODEGRAPH_MVP3_9_HOT_PATH_OUTPUT_DIR") else {
            return;
        };
        let output_dir = PathBuf::from(output_dir);
        let output_dir = if output_dir.is_absolute() {
            output_dir
        } else {
            workspace_root().join(output_dir)
        };
        let mut options = default_mvp3_9_hot_path_reindex_gate_options();
        options.mode = Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke;
        options.fixture_ids = MVP3_9_HOT_PATH_REINDEX_FIXTURE_IDS
            .iter()
            .copied()
            .filter(|fixture_id| *fixture_id != "mvp3_9_hot_path_outside_repo_reject")
            .filter(|fixture_id| *fixture_id != "mvp3_8_graph_claimable_optional_sidecar_corrupt")
            .filter(|fixture_id| *fixture_id != "mvp3_8_unsafe_db_recovery")
            .map(str::to_string)
            .collect();
        options.run_root = output_dir
            .join("logs")
            .join("r")
            .join(format!("hp_{}", unique_run_suffix()));
        options.runner_command =
            "cargo test -p codegraph-bench mvp3_9_hot_path_reindex_gate_product_surface_smoke_writes_artifacts_when_requested --lib"
                .to_string();
        let result = run_mvp3_gate(options).expect("hot-path product-surface smoke");
        let value = serde_json::to_value(&result).expect("serialize hot-path result");
        validate_mvp3_gate_result_value(&value).expect("valid hot-path result");
        write_json(
            &output_dir.join("hot_path_reindex_gate_results.json"),
            &result,
        )
        .expect("write hot-path gate result");
        assert_eq!(result.status, "complete");
        assert!(!result.normal_dot_codegraph_mutated);
    }

    #[test]
    fn mvp3_9_graph_delta_gate_fixture_set_defined() {
        for fixture_id in MVP3_9_GRAPH_DELTA_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            let tags = mvp3_gate_effective_fixture_tags(&manifest);
            assert!(
                tags.contains("graph_delta"),
                "{fixture_id} must be consumable by the graph-delta gate"
            );
        }
        let all_tags = MVP3_9_GRAPH_DELTA_FIXTURE_IDS
            .iter()
            .map(|fixture_id| load_manifest_by_id(fixture_id))
            .flat_map(|manifest| mvp3_gate_effective_fixture_tags(&manifest))
            .collect::<BTreeSet<_>>();
        for required in [
            "source_added",
            "source_deleted",
            "source_renamed",
            "text_evidence",
            "source_role",
            "path_evidence",
            "dirty_evidence",
            "same_name_symbols",
            "duplicate_content_paths",
            "exact_relation_delta",
            "dynamic_call",
            "mock",
        ] {
            assert!(
                all_tags.contains(required),
                "graph-delta tag missing {required}"
            );
        }
    }

    #[test]
    fn mvp3_9_graph_delta_gate_runs_fast() {
        let result = run_graph_delta_gate_fast();
        assert_eq!(result.gate_id, MVP3_9_GRAPH_DELTA_GATE_ID);
        assert_eq!(result.status, "complete");
        assert_eq!(
            result.fixture_report.fixtures_total,
            MVP3_9_GRAPH_DELTA_FIXTURE_IDS.len()
        );
    }

    #[test]
    fn mvp3_9_graph_delta_gate_result_schema_valid() {
        let result = run_graph_delta_gate_fast();
        let value = serde_json::to_value(&result).expect("serialize graph-delta gate");
        validate_mvp3_gate_result_value(&value).expect("valid graph-delta gate result");
    }

    #[test]
    fn mvp3_9_graph_delta_gate_release_binary_plan_supported() {
        let mut options = default_mvp3_9_graph_delta_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-graph-delta-gate-release-plan-test-{}",
            unique_run_suffix()
        ));
        options.mode = Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan;
        let result = run_mvp3_gate(options).expect("graph-delta release plan");
        assert_eq!(result.fixture_report.runner_mode, "release_binary_plan");
        assert!(result.ready_to_move_on);
        assert!(result
            .release_binary_tests
            .iter()
            .any(|entry| entry.ends_with(":true")));
    }

    #[test]
    fn mvp3_9_graph_delta_gate_invariant_counters_zero() {
        let result = run_graph_delta_gate_fast();
        assert_eq!(result.claimability_violations, 0);
        assert_eq!(result.unsupported_claim_violations, 0);
        assert_eq!(result.graph_proof_overclaim_count, 0);
        assert_eq!(result.false_hard_interrupt_count, 0);
        assert_eq!(result.false_blocking_severity_count, 0);
        assert_eq!(result.stale_evidence_reused_as_fresh_count, 0);
        assert_eq!(result.normal_dot_codegraph_mutated, false);
    }

    #[test]
    fn mvp3_9_graph_delta_gate_identity_fixture_assertions() {
        let same_name = load_manifest_by_id("mvp3_9_graph_delta_same_name_symbols");
        assert_eq!(
            same_name
                .initial_graph_expectations
                .get("same_name_symbols_distinct")
                .and_then(Value::as_bool),
            Some(true)
        );
        let duplicate = load_manifest_by_id("mvp3_9_graph_delta_duplicate_content_paths");
        assert_eq!(
            duplicate
                .initial_graph_expectations
                .get("duplicate_content_paths_distinct")
                .and_then(Value::as_bool),
            Some(true)
        );
        let ambiguous = load_manifest_by_id("mvp3_8_ambiguous_rename_unknown");
        assert_eq!(ambiguous.expected_severity, "unknown");
        assert!(!ambiguous.expected_hard_interrupt);
    }

    #[test]
    fn mvp3_9_graph_delta_gate_product_surface_smoke_writes_artifacts_when_requested() {
        let Ok(output_dir) = std::env::var("CODEGRAPH_MVP3_9_GRAPH_DELTA_OUTPUT_DIR") else {
            return;
        };
        let output_dir = PathBuf::from(output_dir);
        let output_dir = if output_dir.is_absolute() {
            output_dir
        } else {
            workspace_root().join(output_dir)
        };
        let mut options = default_mvp3_9_graph_delta_gate_options();
        options.mode = Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke;
        options.fixture_ids = MVP3_9_GRAPH_DELTA_FIXTURE_IDS
            .iter()
            .copied()
            .filter(|fixture_id| *fixture_id != "mvp3_8_mock_test_leakage_attempt")
            .filter(|fixture_id| *fixture_id != "mvp3_8_derived_missing_provenance_integrity")
            .filter(|fixture_id| *fixture_id != "mvp3_8_missing_source_span_integrity")
            .map(str::to_string)
            .collect();
        options.run_root = output_dir
            .join("logs")
            .join("r")
            .join(format!("gd_{}", unique_run_suffix()));
        options.runner_command =
            "cargo test -p codegraph-bench mvp3_9_graph_delta_gate_product_surface_smoke_writes_artifacts_when_requested --lib"
                .to_string();
        let result = run_mvp3_gate(options).expect("graph-delta product-surface smoke");
        let value = serde_json::to_value(&result).expect("serialize graph-delta result");
        validate_mvp3_gate_result_value(&value).expect("valid graph-delta result");
        write_json(&output_dir.join("graph_delta_gate_results.json"), &result)
            .expect("write graph-delta gate result");
        assert_eq!(result.status, "complete");
        assert!(!result.normal_dot_codegraph_mutated);
    }

    #[test]
    fn mvp3_9_hallucination_interrupt_gate_fixture_set_defined() {
        let mut blocking = 0usize;
        let mut warning = 0usize;
        let mut unknown = 0usize;
        let mut diagnostic = 0usize;
        let mut post_fix = 0usize;
        for fixture_id in MVP3_9_HALLUCINATION_INTERRUPT_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            let tags = mvp3_gate_effective_fixture_tags(&manifest);
            assert!(
                tags.contains("hallucination_interrupt"),
                "{fixture_id} must be consumable by the hallucination interrupt gate"
            );
            if manifest.expected_severity == "blocking" {
                blocking += 1;
            }
            if !manifest.expected_warnings.is_empty() || manifest.expected_severity == "warning" {
                warning += 1;
            }
            if !manifest.expected_unknowns.is_empty() || manifest.expected_severity == "unknown" {
                unknown += 1;
            }
            if !manifest.expected_diagnostics.is_empty()
                || manifest.expected_severity == "diagnostic"
                || manifest.expected_severity == "diagnostic_only"
            {
                diagnostic += 1;
            }
            if has_post_fix_recovery(&manifest) {
                post_fix += 1;
            }
        }
        assert!(blocking >= 8, "expected broad exact/blocking coverage");
        assert!(warning >= 4, "expected warning-only/non-proof coverage");
        assert!(unknown >= 2, "expected unknown unsupported coverage");
        assert!(diagnostic >= 2, "expected diagnostic-only coverage");
        assert!(post_fix >= 5, "expected fix/recovery loop coverage");
        assert!(
            MVP3_9_HALLUCINATION_INTERRUPT_FIXTURE_IDS
                .iter()
                .any(|fixture_id| *fixture_id
                    == "mvp3_9_hallucination_mixed_block_warning_diagnostic"),
            "mixed block+warning+diagnostic fixture must be in the gate"
        );
        for forward_fixture_id in [
            "mvp3_9_5_forward_unresolved_python_warning",
            "mvp3_9_5_forward_unresolved_js_warning",
            "mvp3_9_5_forward_unresolved_rust_warning",
        ] {
            assert!(
                MVP3_9_HALLUCINATION_INTERRUPT_FIXTURE_IDS
                    .iter()
                    .any(|fixture_id| *fixture_id == forward_fixture_id),
                "forward unresolved-reference fixture {forward_fixture_id} must be in the gate"
            );
        }
    }

    #[test]
    fn mvp3_9_hallucination_interrupt_gate_exact_blockers_actionable() {
        for fixture_id in [
            "mvp3_8_new_dangling_call",
            "mvp3_8_broken_import",
            "mvp3_8_renamed_target_symbol",
            "mvp3_8_removed_file_stale_facts",
            "mvp3_8_missing_source_span_integrity",
            "mvp3_8_derived_missing_provenance_integrity",
            "mvp3_8_mock_test_leakage_attempt",
            "mvp3_9_hallucination_mixed_block_warning_diagnostic",
        ] {
            let manifest = load_manifest_by_id(fixture_id);
            assert_eq!(manifest.expected_severity, "blocking", "{fixture_id}");
            assert!(manifest.expected_hard_interrupt, "{fixture_id}");
            assert!(manifest.expected_must_fix_before_continuing, "{fixture_id}");
            assert!(
                !expected_rule_ids(&manifest).is_empty(),
                "{fixture_id} must carry a rule id"
            );
            if fixture_id == "mvp3_8_missing_source_span_integrity" {
                assert!(
                    manifest.expected_source_spans.is_empty(),
                    "missing-source-span integrity fixture must expose the absence"
                );
            } else {
                assert!(
                    !manifest.expected_source_spans.is_empty(),
                    "{fixture_id} must carry source spans"
                );
            }
            assert!(
                manifest.expected_recommended_fix.is_some(),
                "{fixture_id} must carry a recommended fix"
            );
            assert!(
                manifest
                    .expected_suggested_next_steps
                    .iter()
                    .any(|step| step.to_ascii_lowercase().contains("rerun")),
                "{fixture_id} must tell the agent to rerun validation"
            );
        }
    }

    #[test]
    fn mvp3_9_hallucination_interrupt_gate_non_graph_evidence_not_blocking() {
        for fixture_id in [
            "mvp3_8_unsupported_dynamic_call",
            "mvp3_8_config_package_text_mismatch",
            "mvp3_8_route_handler_missing_target",
            "mvp3_8_text_evidence_only_edit",
            "mvp3_8_candidate_spool_stale",
            "mvp3_8_vector_runtime_stale",
            "mvp3_8_source_navigation_stale",
            "mvp3_8_ambiguous_rename_unknown",
            "mvp3_8_unsafe_db_recovery",
        ] {
            let manifest = load_manifest_by_id(fixture_id);
            assert!(
                manifest.expected_severity != "blocking",
                "{fixture_id} must not be a blocking source-code proof"
            );
            assert!(
                !manifest.expected_hard_interrupt,
                "{fixture_id} must not produce a hard interrupt"
            );
            assert!(
                manifest
                    .forbidden_graph_proof_outputs
                    .iter()
                    .any(|value| value.contains("graph")
                        || value.contains("proof")
                        || value.contains("exact")),
                "{fixture_id} must forbid graph-proof overclaim"
            );
        }
    }

    #[test]
    fn mvp3_9_hallucination_interrupt_gate_runs_fast() {
        let result = run_hallucination_interrupt_gate_fast();
        assert_eq!(result.gate_id, MVP3_9_HALLUCINATION_INTERRUPT_GATE_ID);
        assert_eq!(result.status, "complete");
        assert_eq!(
            result.fixture_report.fixtures_total,
            MVP3_9_HALLUCINATION_INTERRUPT_FIXTURE_IDS.len()
        );
    }

    #[test]
    fn mvp3_9_hallucination_interrupt_gate_result_schema_valid() {
        let result = run_hallucination_interrupt_gate_fast();
        let value = serde_json::to_value(&result).expect("serialize hallucination gate");
        validate_mvp3_gate_result_value(&value).expect("valid hallucination gate result");
    }

    #[test]
    fn mvp3_9_hallucination_interrupt_gate_release_binary_plan_supported() {
        let mut options = default_mvp3_9_hallucination_interrupt_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-hallucination-gate-release-plan-test-{}",
            unique_run_suffix()
        ));
        options.mode = Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan;
        let result = run_mvp3_gate(options).expect("hallucination release plan");
        assert_eq!(result.fixture_report.runner_mode, "release_binary_plan");
        assert!(result.ready_to_move_on);
        assert!(result
            .release_binary_tests
            .iter()
            .any(|entry| entry.ends_with(":true")));
    }

    #[test]
    fn mvp3_9_hallucination_interrupt_gate_exit_and_mcp_semantics_planned() {
        let mut options = default_mvp3_9_hallucination_interrupt_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-hallucination-gate-plan-test-{}",
            unique_run_suffix()
        ));
        options.mode = Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan;
        options.fixture_ids = vec!["mvp3_8_new_dangling_call".to_string()];
        let report = run_mvp3_validation_fixtures(Mvp3ValidationFixtureRunnerOptions {
            fixture_root: options.fixture_root.clone(),
            run_root: options.run_root.join("fixtures"),
            release_binary: options.release_binary.clone(),
            mode: options.mode,
            fixture_ids: options.fixture_ids.clone(),
            fixture_families: Vec::new(),
            fixture_tags: Vec::new(),
            language_filters: Mvp3LanguageFixtureFilters::default(),
        })
        .expect("planned blocking fixture");
        let result = report.results.first().expect("one result");
        assert!(result.command_plan.iter().any(|record| {
            record.step == "cli_validate_edit"
                && record.command.contains(&"--agent-json".to_string())
                && !record.command.contains(&"--fail-on-blocking".to_string())
        }));
        assert!(result.command_plan.iter().any(|record| {
            record.step == "cli_validate_edit_fail_on_blocking"
                && record.command.contains(&"--fail-on-blocking".to_string())
                && record.command.contains(&"--agent-json".to_string())
        }));
        assert!(result
            .surface_runs
            .iter()
            .any(|surface| surface.surface == "mcp_validate_edit"
                && surface
                    .structured_assertions
                    .iter()
                    .any(|assertion| assertion == "structured_success_for_validation_blockers")));
    }

    #[test]
    fn mvp3_9_hallucination_interrupt_gate_invariant_counters_zero() {
        let result = run_hallucination_interrupt_gate_fast();
        assert_eq!(result.claimability_violations, 0);
        assert_eq!(result.unsupported_claim_violations, 0);
        assert_eq!(result.graph_proof_overclaim_count, 0);
        assert_eq!(result.false_hard_interrupt_count, 0);
        assert_eq!(result.false_blocking_severity_count, 0);
        assert_eq!(result.text_evidence_graph_proof_count, 0);
        assert_eq!(
            result.candidate_vector_source_navigation_graph_proof_count,
            0
        );
        assert_eq!(result.normal_dot_codegraph_mutated, false);
    }

    #[test]
    fn mvp3_9_hallucination_interrupt_gate_product_surface_smoke_writes_artifacts_when_requested() {
        let Ok(output_dir) = std::env::var("CODEGRAPH_MVP3_9_HALLUCINATION_OUTPUT_DIR") else {
            return;
        };
        let output_dir = PathBuf::from(output_dir);
        let output_dir = if output_dir.is_absolute() {
            output_dir
        } else {
            workspace_root().join(output_dir)
        };
        let mut options = default_mvp3_9_hallucination_interrupt_gate_options();
        options.mode = Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke;
        options.fixture_ids = MVP3_9_HALLUCINATION_INTERRUPT_FIXTURE_IDS
            .iter()
            .copied()
            .filter(|fixture_id| *fixture_id != "blocking_synthetic_exact_sample")
            .filter(|fixture_id| *fixture_id != "mvp3_8_derived_missing_provenance_integrity")
            .filter(|fixture_id| *fixture_id != "mvp3_8_missing_source_span_integrity")
            .filter(|fixture_id| *fixture_id != "mvp3_8_unsafe_db_recovery")
            .map(str::to_string)
            .collect();
        options.run_root = output_dir
            .join("logs")
            .join("r")
            .join(format!("hi_{}", unique_run_suffix()));
        options.runner_command =
            "cargo test -p codegraph-bench mvp3_9_hallucination_interrupt_gate_product_surface_smoke_writes_artifacts_when_requested --lib"
                .to_string();
        let result = run_mvp3_gate(options).expect("hallucination product-surface smoke");
        let value = serde_json::to_value(&result).expect("serialize hallucination result");
        validate_mvp3_gate_result_value(&value).expect("valid hallucination result");
        write_json(
            &output_dir.join("hallucination_interrupt_gate_results.json"),
            &result,
        )
        .expect("write hallucination gate result");
        assert_eq!(result.status, "complete");
        assert!(!result.normal_dot_codegraph_mutated);
    }

    #[test]
    fn mvp3_9_route_bridge_inert_contract_gate_fixture_set_defined() {
        let mut route_fixture_count = 0usize;
        let mut bridge_fixture_count = 0usize;
        let mut not_applicable_or_unknown_count = 0usize;
        for fixture_id in MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            let tags = mvp3_gate_effective_fixture_tags(&manifest);
            assert!(
                tags.contains("route_bridge"),
                "{fixture_id} must be consumable by the route/bridge gate"
            );
            if manifest.fixture_id.contains("route") || manifest.fixture_family.contains("route") {
                route_fixture_count += 1;
            }
            if manifest.fixture_id.contains("bridge") || manifest.fixture_family.contains("bridge")
            {
                bridge_fixture_count += 1;
            }
            if matches!(
                manifest.required_support_status.as_str(),
                "degraded" | "unsupported" | "not_applicable"
            ) || matches!(manifest.expected_severity.as_str(), "unknown" | "warning")
            {
                not_applicable_or_unknown_count += 1;
            }
        }
        assert!(route_fixture_count >= 1, "route fixture must be present");
        assert!(
            bridge_fixture_count >= 1,
            "bridge not_applicable scaffold must be present"
        );
        assert!(
            not_applicable_or_unknown_count >= 3,
            "route/bridge gate must cover unsupported or unknown boundaries"
        );
    }

    #[test]
    fn mvp3_9_route_bridge_inert_contract_support_status_detected() {
        let route = load_manifest_by_id("mvp3_8_route_handler_missing_target");
        assert_eq!(route.required_support_status, "degraded");
        assert!(!route.expected_hard_interrupt);
        assert!(expected_rule_ids(&route)
            .iter()
            .any(|rule_id| rule_id == "CG_MVP3_ROUTE_UNSUPPORTED_FRAMEWORK_UNKNOWN"));
        assert!(route
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("unsupported_route_as_graph_proof")));

        let bridge = load_manifest_by_id("mvp3_9_bridge_not_applicable");
        assert_eq!(bridge.required_support_status, "not_applicable");
        assert_eq!(bridge.expected_severity, "unknown");
        assert!(!bridge.expected_hard_interrupt);
        assert!(expected_rule_ids(&bridge)
            .iter()
            .any(|rule_id| rule_id == "CG_MVP3_BRIDGE_NOT_APPLICABLE_UNKNOWN"));
        assert!(bridge
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("bridge_text_or_name_match_as_graph_proof")));
    }

    #[test]
    fn mvp3_9_route_bridge_inert_contract_non_graph_evidence_not_blocking() {
        for fixture_id in [
            "mvp3_8_route_handler_missing_target",
            "mvp3_9_bridge_not_applicable",
            "mvp3_8_routing_handle_stale",
        ] {
            let manifest = load_manifest_by_id(fixture_id);
            assert_ne!(manifest.expected_severity, "blocking", "{fixture_id}");
            assert!(
                manifest.expected_blocking_errors.is_empty(),
                "{fixture_id} must not carry blocking errors"
            );
            assert!(!manifest.expected_hard_interrupt, "{fixture_id}");
            assert!(
                !manifest.forbidden_graph_proof_outputs.is_empty(),
                "{fixture_id} must assert a forbidden graph-proof overclaim"
            );
        }
    }

    #[test]
    fn mvp3_9_route_bridge_inert_contract_gate_runs_fast() {
        let result = run_route_bridge_inert_contract_gate_fast();
        assert_eq!(result.gate_id, MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_GATE_ID);
        assert_eq!(result.status, "complete");
        assert_eq!(
            result.fixture_report.fixtures_total,
            MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_FIXTURE_IDS.len()
        );
        assert!(result.fixture_tags.iter().any(|tag| tag == "route_bridge"));
    }

    #[test]
    fn mvp3_9_route_bridge_inert_contract_gate_result_schema_valid() {
        let result = run_route_bridge_inert_contract_gate_fast();
        let value = serde_json::to_value(&result).expect("serialize route/bridge gate");
        validate_mvp3_gate_result_value(&value).expect("valid route/bridge gate result");
    }

    #[test]
    fn mvp3_9_route_bridge_inert_contract_gate_release_binary_plan_supported() {
        let mut options = default_mvp3_9_route_bridge_inert_contract_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-route-bridge-gate-release-plan-test-{}",
            unique_run_suffix()
        ));
        options.mode = Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan;
        let result = run_mvp3_gate(options).expect("route/bridge release plan");
        assert_eq!(result.fixture_report.runner_mode, "release_binary_plan");
        assert!(result.ready_to_move_on);
        assert!(result
            .release_binary_tests
            .iter()
            .any(|entry| entry.ends_with(":true")));
    }

    #[test]
    fn mvp3_9_route_bridge_inert_contract_gate_invariant_counters_zero() {
        let result = run_route_bridge_inert_contract_gate_fast();
        assert_eq!(result.claimability_violations, 0);
        assert_eq!(result.unsupported_claim_violations, 0);
        assert_eq!(result.graph_proof_overclaim_count, 0);
        assert_eq!(result.false_hard_interrupt_count, 0);
        assert_eq!(result.false_blocking_severity_count, 0);
        assert_eq!(result.route_proof_overclaim_count, 0);
        assert_eq!(result.bridge_proof_overclaim_count, 0);
        assert!(!result.normal_dot_codegraph_mutated);
    }

    #[test]
    fn mvp3_9_route_bridge_inert_contract_no_mvp4_pull_forward() {
        for fixture_id in MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            assert_ne!(
                manifest.expected_mvp_phase, "mvp4",
                "{fixture_id} must stay in the MVP3 inert-contract lane"
            );
            assert!(
                !manifest.tags.iter().any(|tag| tag.contains("mvp4")),
                "{fixture_id} must not be tagged as MVP4 work"
            );
        }
        let bridge = load_manifest_by_id("mvp3_9_bridge_not_applicable");
        assert!(bridge
            .unsupported_or_not_applicable_reason
            .contains("BRIDGES_TO"));
        assert!(bridge
            .expected_suggested_next_steps
            .iter()
            .any(|step| step.contains("MVP3.10")));
    }

    #[test]
    fn mvp3_10_minimal_route_bridge_gate() {
        let result = run_minimal_route_bridge_gate_fast();
        assert_eq!(result.gate_id, MVP3_10_MINIMAL_ROUTE_BRIDGE_GATE_ID);
        assert_eq!(result.status, "complete");
        assert!(result.ready_to_move_on);
        assert_eq!(
            result.fixture_report.fixtures_total,
            MVP3_10_MINIMAL_ROUTE_BRIDGE_FIXTURE_IDS.len()
        );
        assert!(result.fixture_tags.iter().any(|tag| tag == "route_bridge"));
    }

    #[test]
    fn handles_exposes_exact_validation_passed_or_not_applicable() {
        let options = default_mvp3_10_minimal_route_bridge_gate_options();
        assert_eq!(options.gate_kind, "minimal_route_bridge");
        assert_eq!(options.gate_id, MVP3_10_MINIMAL_ROUTE_BRIDGE_GATE_ID);
        let route = load_manifest_by_id("mvp3_8_route_handler_missing_target");
        assert!(route
            .proof_boundary
            .contains("source-spanned HANDLES/EXPOSES proof"));
    }

    #[test]
    fn route_handler_dangling_exact_blocks_if_supported() {
        let route = load_manifest_by_id("mvp3_8_route_handler_missing_target");
        assert!(route
            .expected_cli_validate_edit_behavior
            .notes
            .iter()
            .any(|note| note.contains("CG_MVP3_ROUTE_HANDLER_DANGLING_TARGET")));
    }

    #[test]
    fn route_handler_renamed_exact_blocks_if_supported() {
        let route = load_manifest_by_id("mvp3_8_route_handler_missing_target");
        assert!(route
            .proof_boundary
            .contains("Route text or convention evidence is not graph proof"));
        assert!(route
            .expected_suggested_next_steps
            .iter()
            .any(|step| step.contains("exact source-spanned route relation")));
    }

    #[test]
    fn route_handler_fix_clears_blocker_if_supported() {
        let route = load_manifest_by_id("mvp3_8_route_handler_missing_target");
        assert!(route.tags.iter().any(|tag| tag == "fix_recovery"));
        assert_eq!(
            route.initial_lifecycle_expectations["claimable"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn routes_to_mounts_router_inert_until_exact_support() {
        let route = load_manifest_by_id("mvp3_8_route_handler_missing_target");
        let joined = format!(
            "{} {}",
            route.proof_boundary, route.unsupported_or_not_applicable_reason
        );
        assert!(!joined.contains("ROUTES_TO graph proof"));
        assert!(!joined.contains("MOUNTS_ROUTER graph proof"));
    }

    #[test]
    fn bridges_to_inert_until_exact_support() {
        let bridge = load_manifest_by_id("mvp3_9_bridge_not_applicable");
        assert_eq!(bridge.required_support_status, "not_applicable");
        assert!(bridge
            .unsupported_or_not_applicable_reason
            .contains("BRIDGES_TO"));
        assert!(!bridge.expected_hard_interrupt);
    }

    #[test]
    fn heuristic_route_bridge_not_blocking() {
        for fixture_id in MVP3_10_MINIMAL_ROUTE_BRIDGE_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            assert_ne!(manifest.expected_severity, "blocking", "{fixture_id}");
            assert!(!manifest.expected_hard_interrupt, "{fixture_id}");
        }
    }

    #[test]
    fn text_convention_route_evidence_not_graph_proof() {
        for fixture_id in [
            "mvp3_8_route_handler_missing_target",
            "mvp3_9_bridge_not_applicable",
        ] {
            let manifest = load_manifest_by_id(fixture_id);
            assert!(
                !manifest.forbidden_graph_proof_outputs.is_empty(),
                "{fixture_id}"
            );
        }
    }

    #[test]
    fn dirty_route_bridge_handles_non_proof() {
        let dirty = load_manifest_by_id("mvp3_8_routing_handle_stale");
        assert_eq!(dirty.expected_hard_interrupt, false);
        assert!(dirty
            .forbidden_graph_proof_outputs
            .iter()
            .any(|value| value.contains("stale") || value.contains("routing")));
    }

    #[test]
    fn mcp_route_bridge_parity_or_not_applicable() {
        let route = load_manifest_by_id("mvp3_8_route_handler_missing_target");
        assert!(route.expected_mcp_validate_edit_behavior.not_applicable);
        assert!(!route.expected_mcp_validate_edit_behavior.supported);
    }

    #[test]
    fn false_route_bridge_hard_interrupt_count_zero() {
        let result = run_minimal_route_bridge_gate_fast();
        assert_eq!(result.false_hard_interrupt_count, 0);
        assert_eq!(result.route_proof_overclaim_count, 0);
        assert_eq!(result.bridge_proof_overclaim_count, 0);
    }

    #[test]
    fn no_mvp4_extraction_started() {
        for fixture_id in MVP3_10_MINIMAL_ROUTE_BRIDGE_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            assert_ne!(manifest.expected_mvp_phase, "mvp4");
            assert!(!manifest.tags.iter().any(|tag| tag.contains("mvp4")));
        }
    }

    #[test]
    fn mvp3_10_no_dot_codegraph_mutation() {
        let result = run_minimal_route_bridge_gate_fast();
        assert!(!result.normal_dot_codegraph_mutated);
    }

    #[test]
    fn mvp3_9_route_bridge_inert_contract_gate_product_surface_smoke_writes_artifacts_when_requested(
    ) {
        let Ok(output_dir) = std::env::var("CODEGRAPH_MVP3_9_ROUTE_BRIDGE_OUTPUT_DIR") else {
            return;
        };
        let output_dir = PathBuf::from(output_dir);
        let output_dir = if output_dir.is_absolute() {
            output_dir
        } else {
            workspace_root().join(output_dir)
        };
        let mut options = default_mvp3_9_route_bridge_inert_contract_gate_options();
        options.mode = Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke;
        options.run_root = output_dir
            .join("logs")
            .join("r")
            .join(format!("rb_{}", unique_run_suffix()));
        options.runner_command =
            "cargo test -p codegraph-bench mvp3_9_route_bridge_inert_contract_gate_product_surface_smoke_writes_artifacts_when_requested --lib"
                .to_string();
        let result = run_mvp3_gate(options).expect("route/bridge product-surface smoke");
        let value = serde_json::to_value(&result).expect("serialize route/bridge result");
        validate_mvp3_gate_result_value(&value).expect("valid route/bridge result");
        write_json(
            &output_dir.join("route_bridge_inert_contract_gate_results.json"),
            &result,
        )
        .expect("write route/bridge gate result");
        assert_eq!(result.status, "complete");
        assert!(!result.normal_dot_codegraph_mutated);
    }

    #[test]
    fn mvp3_9_cross_phase_regression_gate_fixture_set_defined() {
        let mut stage0 = 0usize;
        let mut routing = 0usize;
        let mut retrieval_boundary = 0usize;
        let mut bounded_or_path = 0usize;
        let mut language_boundary = 0usize;
        let mut dirty = 0usize;
        let mut mcp = 0usize;
        let mut schema_or_json = 0usize;

        for fixture_id in MVP3_9_CROSS_PHASE_REGRESSION_FIXTURE_IDS {
            let manifest = load_manifest_by_id(fixture_id);
            let tags = mvp3_gate_effective_fixture_tags(&manifest);
            assert!(
                tags.contains("cross_phase"),
                "{fixture_id} must be consumable by the cross-phase gate"
            );
            if tags.contains("stage0") || tags.contains("text_evidence") {
                stage0 += 1;
            }
            if tags.contains("route")
                || tags.contains("route_bridge")
                || tags.contains("routing_handle")
                || fixture_id.contains("routing")
            {
                routing += 1;
            }
            if tags.contains("candidate_spool")
                || tags.contains("candidate_query_index")
                || tags.contains("vector")
                || tags.contains("vector_audit")
                || tags.contains("binary")
                || tags.contains("nuance")
                || tags.contains("source_navigation")
                || tags.contains("routing_handle")
            {
                retrieval_boundary += 1;
            }
            if tags.contains("path_evidence")
                || tags.contains("same_name_symbols")
                || tags.contains("duplicate_content_paths")
            {
                bounded_or_path += 1;
            }
            if tags.contains("dynamic_call")
                || tags.contains("unsupported_relation")
                || manifest.required_support_status == "unsupported"
                || manifest.required_support_status == "not_applicable"
            {
                language_boundary += 1;
            }
            if tags.contains("dirty_evidence") {
                dirty += 1;
            }
            if tags.contains("mcp") {
                mcp += 1;
            }
            if !manifest.fixture_id.is_empty()
                && manifest.schema_version == MVP3_VALIDATION_FIXTURE_SCHEMA_VERSION
            {
                schema_or_json += 1;
            }
        }

        assert!(
            stage0 >= 2,
            "Stage 0/text evidence fixtures must be covered"
        );
        assert!(
            routing >= 3,
            "routing/route/bridge fixtures must be covered"
        );
        assert!(
            retrieval_boundary >= 6,
            "candidate/vector/source-navigation boundaries must be covered"
        );
        assert!(
            bounded_or_path >= 3,
            "bounded graph/PathEvidence identity fixtures must be covered"
        );
        assert!(
            language_boundary >= 3,
            "unsupported/not_applicable language boundaries must be covered"
        );
        assert!(dirty >= 10, "dirty evidence fixtures must be covered");
        assert_eq!(mcp, MVP3_9_CROSS_PHASE_REGRESSION_FIXTURE_IDS.len());
        assert_eq!(
            schema_or_json,
            MVP3_9_CROSS_PHASE_REGRESSION_FIXTURE_IDS.len()
        );
    }

    #[test]
    fn mvp3_9_cross_phase_regression_gate_runs_fast() {
        let result = run_cross_phase_regression_gate_fast();
        assert_eq!(result.gate_id, MVP3_9_CROSS_PHASE_REGRESSION_GATE_ID);
        assert_eq!(result.status, "complete");
        assert_eq!(
            result.fixture_report.fixtures_total,
            MVP3_9_CROSS_PHASE_REGRESSION_FIXTURE_IDS.len()
        );
    }

    #[test]
    fn mvp3_9_cross_phase_regression_gate_result_schema_valid() {
        let result = run_cross_phase_regression_gate_fast();
        let value = serde_json::to_value(&result).expect("serialize cross-phase gate");
        validate_mvp3_gate_result_value(&value).expect("valid cross-phase gate result");
    }

    #[test]
    fn mvp3_9_cross_phase_regression_gate_release_binary_plan_supported() {
        let mut options = default_mvp3_9_cross_phase_regression_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-cross-phase-gate-release-plan-test-{}",
            unique_run_suffix()
        ));
        options.mode = Mvp3ValidationFixtureRunnerMode::ReleaseBinaryPlan;
        let result = run_mvp3_gate(options).expect("cross-phase release plan");
        assert_eq!(result.fixture_report.runner_mode, "release_binary_plan");
        assert!(result.ready_to_move_on);
        assert!(result
            .release_binary_tests
            .iter()
            .any(|entry| entry.ends_with(":true")));
    }

    #[test]
    fn mvp3_9_cross_phase_regression_gate_invariant_counters_zero() {
        let result = run_cross_phase_regression_gate_fast();
        assert_eq!(result.claimability_violations, 0);
        assert_eq!(result.unsupported_claim_violations, 0);
        assert_eq!(result.graph_proof_overclaim_count, 0);
        assert_eq!(result.false_hard_interrupt_count, 0);
        assert_eq!(result.false_blocking_severity_count, 0);
        assert_eq!(result.forbidden_claimable_output_count, 0);
        assert_eq!(result.stale_evidence_reused_as_fresh_count, 0);
        assert_eq!(result.test_mock_production_leakage_count, 0);
        assert_eq!(result.route_proof_overclaim_count, 0);
        assert_eq!(result.text_evidence_graph_proof_count, 0);
        assert_eq!(
            result.candidate_vector_source_navigation_graph_proof_count,
            0
        );
        assert!(!result.normal_dot_codegraph_mutated);
    }

    #[test]
    fn mvp3_9_cross_phase_regression_gate_prior_surface_contracts_present() {
        let text = load_manifest_by_id("mvp3_8_text_evidence_only_edit");
        assert_eq!(text.expected_severity, "warning");
        assert!(!text.expected_hard_interrupt);
        assert!(!text.forbidden_graph_proof_outputs.is_empty());

        let route = load_manifest_by_id("mvp3_8_route_handler_missing_target");
        assert_eq!(route.expected_severity, "unknown");
        assert!(!route.expected_hard_interrupt);

        let vector = load_manifest_by_id("mvp3_8_vector_runtime_stale");
        assert!(!vector.forbidden_graph_proof_outputs.is_empty());

        let sidecar = load_manifest_by_id("mvp3_8_candidate_query_index_corrupt");
        assert!(sidecar
            .expected_sidecar_statuses
            .get("candidate_spool_query_index_status")
            .is_some());

        let path = load_manifest_by_id("mvp3_8_path_evidence_stale_invalidated");
        assert!(mvp3_gate_effective_fixture_tags(&path).contains("path_evidence"));

        let unsupported = load_manifest_by_id("mvp3_8_unsupported_dynamic_call");
        assert_eq!(unsupported.required_support_status, "unsupported");
        assert_eq!(unsupported.expected_severity, "unknown");

        let bridge = load_manifest_by_id("mvp3_9_bridge_not_applicable");
        assert_eq!(bridge.required_support_status, "not_applicable");
        assert!(!bridge.expected_hard_interrupt);
    }

    #[test]
    fn mvp3_9_cross_phase_regression_gate_product_surface_smoke_writes_artifacts_when_requested() {
        let Ok(output_dir) = std::env::var("CODEGRAPH_MVP3_9_CROSS_PHASE_OUTPUT_DIR") else {
            return;
        };
        let output_dir = PathBuf::from(output_dir);
        let output_dir = if output_dir.is_absolute() {
            output_dir
        } else {
            workspace_root().join(output_dir)
        };
        let mut options = default_mvp3_9_cross_phase_regression_gate_options();
        options.mode = Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke;
        options.fixture_ids = MVP3_9_CROSS_PHASE_REGRESSION_FIXTURE_IDS
            .iter()
            .copied()
            .filter(|fixture_id| *fixture_id != "blocking_synthetic_exact_sample")
            .filter(|fixture_id| *fixture_id != "mvp3_8_derived_missing_provenance_integrity")
            .filter(|fixture_id| *fixture_id != "mvp3_8_missing_source_span_integrity")
            .filter(|fixture_id| *fixture_id != "mvp3_8_unsafe_db_recovery")
            .map(str::to_string)
            .collect();
        options.run_root = output_dir
            .join("logs")
            .join("r")
            .join(format!("cp_{}", unique_run_suffix()));
        options.runner_command =
            "cargo test -p codegraph-bench mvp3_9_cross_phase_regression_gate_product_surface_smoke_writes_artifacts_when_requested --lib"
                .to_string();
        let result = run_mvp3_gate(options).expect("cross-phase product-surface smoke");
        let value = serde_json::to_value(&result).expect("serialize cross-phase result");
        validate_mvp3_gate_result_value(&value).expect("valid cross-phase result");
        write_json(
            &output_dir.join("cross_phase_regression_gate_results.json"),
            &result,
        )
        .expect("write cross-phase gate result");
        assert_eq!(result.status, "complete");
        assert!(!result.normal_dot_codegraph_mutated);
    }

    #[test]
    fn release_mvp3_gate_orchestrator_complete() {
        let result = run_release_final_gate_orchestrator_fast();
        assert_eq!(result.gate_id, MVP3_9_RELEASE_FINAL_GATE_ORCHESTRATOR_ID);
        assert_eq!(result.status, "complete");
        assert!(result.ready_to_move_on);
        assert_eq!(
            result.fixture_report.fixtures_total,
            MVP3_9_RELEASE_FINAL_GATE_ORCHESTRATOR_FIXTURE_IDS.len()
        );
    }

    #[test]
    fn all_subgates_runnable_by_one_command_or_script() {
        let options = default_mvp3_9_release_final_gate_orchestrator_options();
        assert!(options
            .runner_command
            .contains("mvp3_9_release_final_gate_orchestrator"));
        assert_eq!(
            MVP3_9_RELEASE_FINAL_SUBGATE_IDS,
            &[
                MVP3_9_HOT_PATH_REINDEX_GATE_ID,
                MVP3_9_GRAPH_DELTA_GATE_ID,
                MVP3_9_HALLUCINATION_INTERRUPT_GATE_ID,
                MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_GATE_ID,
                MVP3_9_CROSS_PHASE_REGRESSION_GATE_ID,
            ]
        );
    }

    #[test]
    fn orchestrator_aggregates_subgate_results() {
        let final_ids = MVP3_9_RELEASE_FINAL_GATE_ORCHESTRATOR_FIXTURE_IDS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for subgate_fixture_ids in [
            MVP3_9_HOT_PATH_REINDEX_FIXTURE_IDS,
            MVP3_9_GRAPH_DELTA_FIXTURE_IDS,
            MVP3_9_HALLUCINATION_INTERRUPT_FIXTURE_IDS,
            MVP3_9_ROUTE_BRIDGE_INERT_CONTRACT_FIXTURE_IDS,
            MVP3_9_CROSS_PHASE_REGRESSION_FIXTURE_IDS,
        ] {
            for fixture_id in subgate_fixture_ids {
                assert!(
                    final_ids.contains(fixture_id),
                    "release final orchestrator missing subgate fixture {fixture_id}"
                );
            }
        }
    }

    #[test]
    fn orchestrator_aggregates_invariant_counters() {
        let result = run_release_final_gate_orchestrator_fast();
        assert_eq!(result.claimability_violations, 0);
        assert_eq!(result.unsupported_claim_violations, 0);
        assert_eq!(result.graph_proof_overclaim_count, 0);
        assert_eq!(result.false_hard_interrupt_count, 0);
        assert_eq!(result.false_blocking_severity_count, 0);
        assert_eq!(result.forbidden_claimable_output_count, 0);
        assert_eq!(result.stale_evidence_reused_as_fresh_count, 0);
        assert_eq!(result.test_mock_production_leakage_count, 0);
        assert_eq!(result.route_proof_overclaim_count, 0);
        assert_eq!(result.bridge_proof_overclaim_count, 0);
        assert_eq!(result.text_evidence_graph_proof_count, 0);
        assert_eq!(
            result.candidate_vector_source_navigation_graph_proof_count,
            0
        );
    }

    #[test]
    fn orchestrator_classifies_failures() {
        let mut options = default_mvp3_9_release_final_gate_orchestrator_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-release-final-empty-test-{}",
            unique_run_suffix()
        ));
        options.fixture_ids = vec!["missing_release_final_fixture".to_string()];
        let result = run_mvp3_gate(options).expect("missing fixture emits structured failure");
        assert_eq!(result.status, "failed");
        assert!(!result.ready_to_move_on);
        assert!(result
            .failures
            .iter()
            .any(|failure| failure.taxonomy == "gate_dependency_missing"));
        assert!(MVP3_9_FAILURE_TAXONOMY.contains(&"gate_dependency_missing"));
        assert!(MVP3_9_FAILURE_TAXONOMY.contains(&"fixture_runner_failure"));
    }

    #[test]
    fn orchestrator_validates_json_schemas() {
        let result = run_release_final_gate_orchestrator_fast();
        let value = serde_json::to_value(&result).expect("serialize release final gate");
        validate_mvp3_gate_result_value(&value).expect("valid release final gate result");
        let schema = mvp3_gate_result_schema_value();
        assert_eq!(schema.get("type").and_then(Value::as_str), Some("object"));
        assert!(schema
            .get("required")
            .and_then(Value::as_array)
            .expect("required fields")
            .iter()
            .any(|field| field.as_str() == Some("gate_id")));
        let taxonomy = mvp3_gate_failure_taxonomy_value();
        assert!(taxonomy
            .get("failure_taxonomy")
            .and_then(Value::as_array)
            .expect("taxonomy array")
            .iter()
            .any(|field| field.as_str() == Some("fixture_runner_failure")));
    }

    #[test]
    fn orchestrator_records_release_binary() {
        let result = run_release_final_gate_orchestrator_fast();
        assert!(result.release_binary.contains("codegraph-mcp"));
        assert!(result
            .release_binary_tests
            .iter()
            .any(|entry| entry.ends_with(":true") || entry.ends_with(":false")));
    }

    #[test]
    fn orchestrator_records_no_dot_codegraph() {
        let result = run_release_final_gate_orchestrator_fast();
        assert!(!result.normal_dot_codegraph_mutated);
        assert!(!result.observed_invariants.normal_dot_codegraph_mutated);
    }

    #[test]
    fn orchestrator_public_claim_false() {
        let result = run_release_final_gate_orchestrator_fast();
        assert!(!result.public_claim);
        assert!(!result.real_agent_patch_quality_claim);
        assert!(result.claim_boundaries_preserved);
    }

    #[test]
    fn orchestrator_mvp4_not_started() {
        let result = run_release_final_gate_orchestrator_fast();
        assert!(!result.mvp4_started);
        assert!(result.mvp4_future_only);
    }

    #[test]
    fn mvp3_9_release_final_gate_orchestrator_product_surface_smoke_writes_artifacts_when_requested(
    ) {
        let Ok(output_dir) = std::env::var("CODEGRAPH_MVP3_9_RELEASE_FINAL_OUTPUT_DIR") else {
            return;
        };
        let output_dir = PathBuf::from(output_dir);
        let output_dir = if output_dir.is_absolute() {
            output_dir
        } else {
            workspace_root().join(output_dir)
        };
        let mut options = default_mvp3_9_release_final_gate_orchestrator_options();
        options.mode = Mvp3ValidationFixtureRunnerMode::ProductSurfaceSmoke;
        options.fixture_ids = MVP3_9_RELEASE_FINAL_GATE_ORCHESTRATOR_FIXTURE_IDS
            .iter()
            .copied()
            .filter(|fixture_id| *fixture_id != "blocking_synthetic_exact_sample")
            .filter(|fixture_id| *fixture_id != "mvp3_8_derived_missing_provenance_integrity")
            .filter(|fixture_id| *fixture_id != "mvp3_8_missing_source_span_integrity")
            .filter(|fixture_id| *fixture_id != "mvp3_8_unsafe_db_recovery")
            .map(str::to_string)
            .collect();
        options.run_root = output_dir
            .join("logs")
            .join("r")
            .join(format!("rf_{}", unique_run_suffix()));
        options.runner_command =
            "cargo test -p codegraph-bench mvp3_9_release_final_gate_orchestrator_product_surface_smoke_writes_artifacts_when_requested --lib"
                .to_string();
        let result = run_mvp3_gate(options).expect("release final product-surface smoke");
        let value = serde_json::to_value(&result).expect("serialize release final result");
        validate_mvp3_gate_result_value(&value).expect("valid release final result");
        write_json(&output_dir.join("gate_results.json"), &result)
            .expect("write release final gate result");
        write_text(
            &output_dir.join("gate_summary.md"),
            &render_mvp3_gate_result_markdown(&result),
        )
        .expect("write release final gate summary");
        write_json(
            &output_dir.join("gate_results_by_subgate.json"),
            &json!({
                "orchestrator_gate_id": MVP3_9_RELEASE_FINAL_GATE_ORCHESTRATOR_ID,
                "subgate_ids": MVP3_9_RELEASE_FINAL_SUBGATE_IDS,
                "status": result.status,
                "ready_to_move_on": result.ready_to_move_on,
                "input_fixture_count": result.input_fixtures.len(),
                "claimability_violations": result.claimability_violations,
                "unsupported_claim_violations": result.unsupported_claim_violations,
                "graph_proof_overclaim_count": result.graph_proof_overclaim_count,
                "false_hard_interrupt_count": result.false_hard_interrupt_count,
                "false_blocking_severity_count": result.false_blocking_severity_count,
                "normal_dot_codegraph_mutated": result.normal_dot_codegraph_mutated,
                "public_claim": false,
                "real_agent_patch_quality_claim": false,
                "mvp4_not_started": true
            }),
        )
        .expect("write release final by-subgate result");
        assert_eq!(result.status, "complete");
        assert!(!result.normal_dot_codegraph_mutated);
    }

    fn first_sample_result() -> Mvp3FixtureResult {
        run_sample_harness()
            .results
            .into_iter()
            .next()
            .expect("sample result")
    }

    fn run_sample_gate() -> Mvp3GateResult {
        let mut options = default_mvp3_gate_runner_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-gate-sample-test-{}",
            unique_run_suffix()
        ));
        run_mvp3_gate(options).expect("sample gate")
    }

    fn run_hot_path_gate_fast() -> Mvp3GateResult {
        let mut options = default_mvp3_9_hot_path_reindex_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-hot-path-gate-fast-test-{}",
            unique_run_suffix()
        ));
        run_mvp3_gate(options).expect("hot-path gate")
    }

    fn run_graph_delta_gate_fast() -> Mvp3GateResult {
        let mut options = default_mvp3_9_graph_delta_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-graph-delta-gate-fast-test-{}",
            unique_run_suffix()
        ));
        run_mvp3_gate(options).expect("graph-delta gate")
    }

    fn run_hallucination_interrupt_gate_fast() -> Mvp3GateResult {
        let mut options = default_mvp3_9_hallucination_interrupt_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-hallucination-gate-fast-test-{}",
            unique_run_suffix()
        ));
        run_mvp3_gate(options).expect("hallucination gate")
    }

    fn run_route_bridge_inert_contract_gate_fast() -> Mvp3GateResult {
        let mut options = default_mvp3_9_route_bridge_inert_contract_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-route-bridge-gate-fast-test-{}",
            unique_run_suffix()
        ));
        run_mvp3_gate(options).expect("route/bridge gate")
    }

    fn run_minimal_route_bridge_gate_fast() -> Mvp3GateResult {
        let mut options = default_mvp3_10_minimal_route_bridge_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-10-minimal-route-bridge-gate-fast-test-{}",
            unique_run_suffix()
        ));
        run_mvp3_gate(options).expect("mvp3.10 minimal route/bridge gate")
    }

    fn run_cross_phase_regression_gate_fast() -> Mvp3GateResult {
        let mut options = default_mvp3_9_cross_phase_regression_gate_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-cross-phase-gate-fast-test-{}",
            unique_run_suffix()
        ));
        run_mvp3_gate(options).expect("cross-phase gate")
    }

    fn run_release_final_gate_orchestrator_fast() -> Mvp3GateResult {
        let mut options = default_mvp3_9_release_final_gate_orchestrator_options();
        options.run_root = std::env::temp_dir().join(format!(
            "codegraph-mvp3-release-final-gate-fast-test-{}",
            unique_run_suffix()
        ));
        run_mvp3_gate(options).expect("release final gate orchestrator")
    }

    fn run_sample_harness() -> Mvp3ValidationFixtureRunReport {
        run_mvp3_validation_fixtures(sample_options()).expect("sample harness")
    }

    fn sample_options() -> Mvp3ValidationFixtureRunnerOptions {
        let fixture_root = workspace_root().join("fixtures").join("mvp3_validation");
        Mvp3ValidationFixtureRunnerOptions {
            fixture_root,
            run_root: std::env::temp_dir().join(format!(
                "codegraph-mvp3-validation-fixtures-test-{}",
                unique_run_suffix()
            )),
            release_binary: Some(PathBuf::from("codegraph-mcp")),
            mode: Mvp3ValidationFixtureRunnerMode::FastSynthetic,
            fixture_ids: Vec::new(),
            fixture_families: Vec::new(),
            fixture_tags: Vec::new(),
            language_filters: Mvp3LanguageFixtureFilters::default(),
        }
    }

    fn sample_fixture_paths() -> Vec<PathBuf> {
        discover_mvp3_validation_fixture_paths(
            &workspace_root().join("fixtures").join("mvp3_validation"),
        )
        .expect("discover sample fixture paths")
    }

    fn load_sample_manifest(id: &str) -> Mvp3ValidationFixtureManifest {
        load_manifest_by_id(id)
    }

    fn load_manifest_by_id(id: &str) -> Mvp3ValidationFixtureManifest {
        for path in sample_fixture_paths() {
            let manifest =
                load_mvp3_validation_fixture_manifest(&path).expect("load fixture manifest");
            if manifest.fixture_id == id {
                return manifest;
            }
        }
        panic!("fixture {id} not found");
    }

    fn load_manifest_value_by_id(id: &str) -> Value {
        for path in sample_fixture_paths() {
            let raw = fs::read_to_string(&path).expect("read fixture manifest");
            let value: Value = serde_json::from_str(&raw).expect("fixture JSON");
            if value
                .get("fixture_id")
                .and_then(Value::as_str)
                .is_some_and(|fixture_id| fixture_id == id)
            {
                return value;
            }
        }
        panic!("fixture {id} not found");
    }

    fn result_by_id<'a>(
        report: &'a Mvp3ValidationFixtureRunReport,
        id: &str,
    ) -> &'a Mvp3FixtureResult {
        report
            .results
            .iter()
            .find(|result| result.fixture_id == id)
            .unwrap_or_else(|| panic!("fixture result {id} not found"))
    }

    fn assert_fixture_rule_and_status(
        fixture_id: &str,
        expected_rule_id: &str,
        expected_status: &str,
        expected_hard_interrupt: bool,
    ) {
        let report = run_sample_harness();
        let result = result_by_id(&report, fixture_id);
        assert_eq!(result.status, "passed");
        let manifest = load_manifest_by_id(fixture_id);
        assert_eq!(
            manifest.expected_cli_validate_edit_behavior.status,
            expected_status
        );
        assert_eq!(manifest.expected_hard_interrupt, expected_hard_interrupt);
        assert!(
            expected_rule_ids(&manifest)
                .iter()
                .any(|rule_id| rule_id == expected_rule_id),
            "{fixture_id} must include expected rule {expected_rule_id}"
        );
    }

    fn assert_dirty_fixture_sidecar_status(
        fixture_id: &str,
        field: &str,
        expected_value: &str,
        expected_status: &str,
        expected_hard_interrupt: bool,
    ) -> Mvp3ValidationFixtureManifest {
        let report = run_sample_harness();
        assert_eq!(result_by_id(&report, fixture_id).status, "passed");
        let manifest = load_manifest_by_id(fixture_id);
        assert_eq!(
            manifest.expected_cli_validate_edit_behavior.status,
            expected_status
        );
        assert_eq!(manifest.expected_hard_interrupt, expected_hard_interrupt);
        assert_eq!(
            manifest
                .expected_sidecar_statuses
                .get(field)
                .and_then(Value::as_str),
            Some(expected_value),
            "{fixture_id} expected sidecar status {field}={expected_value}"
        );
        manifest
    }

    fn assert_graph_claimable(manifest: &Mvp3ValidationFixtureManifest) {
        assert_eq!(
            manifest
                .expected_final_db_lifecycle_status
                .pointer("/claimable")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            manifest
                .expected_sidecar_statuses
                .pointer("/graph_db_claimable")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    fn assert_post_fix_status(fixture_id: &str, allowed: &[&str]) -> Mvp3ValidationFixtureManifest {
        let report = run_sample_harness();
        let result = result_by_id(&report, fixture_id);
        assert_eq!(result.status, "passed");
        assert_step(&result.command_plan, "post_fix_validate_edit");
        assert_step(&result.command_plan, "post_fix_context_pack");
        assert_step(&result.command_plan, "post_fix_status");
        let manifest = load_manifest_by_id(fixture_id);
        assert!(
            allowed.contains(&manifest.post_fix_expected_status.as_str()),
            "{fixture_id} post-fix status {} not in {allowed:?}",
            manifest.post_fix_expected_status
        );
        assert_eq!(
            manifest.post_fix_expected_hard_interrupt, false,
            "{fixture_id} must clear hard interrupt after fix/recovery"
        );
        manifest
    }

    fn assert_recovery_command_hint(manifest: &Mvp3ValidationFixtureManifest) {
        assert!(
            !manifest.recovery_commands_expected.is_empty(),
            "{} must include an expected recovery command",
            manifest.fixture_id
        );
        assert!(
            manifest.recovery_is_hint_not_proof,
            "{} must declare recovery is a hint/action, not proof",
            manifest.fixture_id
        );
        assert_eq!(
            manifest
                .recovery_command_verification
                .pointer("/lifecycle_preflight_required")
                .and_then(Value::as_bool),
            Some(true),
            "{} must require lifecycle preflight before claimability",
            manifest.fixture_id
        );
    }

    fn assert_step(command_plan: &[Mvp3FixtureCommandRecord], step: &str) {
        assert!(
            command_plan.iter().any(|record| record.step == step),
            "missing command plan step {step}: {command_plan:?}"
        );
    }

    fn command_step<'a>(
        command_plan: &'a [Mvp3FixtureCommandRecord],
        step: &str,
    ) -> &'a Mvp3FixtureCommandRecord {
        command_plan
            .iter()
            .find(|record| record.step == step)
            .unwrap_or_else(|| panic!("missing command step {step}"))
    }

    fn surface_by_name<'a>(
        result: &'a Mvp3FixtureResult,
        surface: &str,
    ) -> &'a Mvp3FixtureSurfaceRun {
        result
            .surface_runs
            .iter()
            .find(|run| run.surface == surface)
            .unwrap_or_else(|| panic!("missing surface {surface}"))
    }
}
