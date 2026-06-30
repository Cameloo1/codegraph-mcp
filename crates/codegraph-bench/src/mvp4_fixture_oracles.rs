//! MVP4 fixture/oracle manifest and runner scaffolding.
//!
//! This module validates durable MVP4 fixture oracles before production
//! extraction exists. It deliberately plans future parser/store/query stages but
//! never calls an MVP4 extractor or requires production micro facts.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{BenchResult, BenchmarkError};

pub const MVP4_FIXTURE_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const MVP4_FIXTURE_ROOT_MANIFEST_FILE: &str = "manifest.json";
pub const MVP4_FIXTURE_ORACLE_GATE: &str = "mvp4_fixture_oracle_harness_schema";

const REQUIRED_ROOT_FIELDS: &[&str] = &[
    "schema_version",
    "generated_at",
    "fixture_root",
    "selected_slice_id",
    "production_extraction_required_by_current_tests",
    "release_runner_plan",
    "fixtures",
];

const REQUIRED_FIXTURE_FIELDS: &[&str] = &[
    "fixture_id",
    "title",
    "description",
    "language",
    "feature_tier",
    "source_role",
    "required_support_status",
    "tags",
    "schema_version",
    "extraction_version_expectation",
    "initial_source",
    "expected_micro_nodes",
    "expected_micro_edges",
    "expected_packets",
    "mutation_update",
    "storage",
    "negative_oracles",
];

const OPTIONAL_FIXTURE_FIELDS: &[&str] = &[
    "forbidden_micro_edges",
    "expected_micro_edge_delta",
    "expected_validation",
    "expected_local_micro_flow_packets",
    "forbidden_local_micro_flow_packets",
    "expected_packet_delta",
    "expected_linter_packet_state",
];

const REQUIRED_INITIAL_SOURCE_FIELDS: &[&str] = &[
    "files",
    "function_scope_anchors",
    "expected_parser_state",
    "expected_unsupported_regions",
];

const REQUIRED_NODE_FIELDS: &[&str] = &[
    "node_kind",
    "semantic_name_or_literal",
    "scope_function",
    "source_span",
    "binding_status",
    "exactness",
    "source_role",
    "forbidden_duplicate_count",
];

const REQUIRED_EDGE_FIELDS: &[&str] = &[
    "relation_kind",
    "head_selector",
    "tail_selector",
    "source_span",
    "provenance",
    "exactness",
    "claimability",
    "forbidden_edges",
];

const OPTIONAL_EDGE_FIELDS: &[&str] = &["expected_count"];

const REQUIRED_FORBIDDEN_EDGE_FIELDS: &[&str] = &[
    "relation_kind",
    "head_selector",
    "tail_selector",
    "reason",
    "forbidden_head_tail_pairing",
    "forbidden_cross_function_link",
    "forbidden_cross_file_link",
    "forbidden_duplicate",
];

const REQUIRED_PACKET_BASE_FIELDS: &[&str] = &[
    "packet_kind",
    "proof_strength",
    "unknown_gaps",
    "omitted_truncation_expectation",
    "forbidden_global_interprocedural_claim",
];

const VERBOSE_PACKET_FIELDS: &[&str] = &["ordered_steps", "step_spans"];

const COMPACT_PACKET_FIELDS: &[&str] = &["encoding", "packet_body"];

const REQUIRED_PACKET_COMPRESSION_CONTRACT_FIELDS: &[&str] = &[
    "lossless_to_audit_ordered_steps",
    "source_spans_preserved",
    "provenance_preserved",
    "exactness_preserved",
    "source_roles_preserved",
    "branch_and_return_paths_preserved",
    "unknown_gaps_preserved",
    "full_source_bodies_allowed",
];

const REQUIRED_LOCAL_MICRO_FLOW_PACKET_FIELDS: &[&str] = &[
    "packet_id_selector",
    "function_selector",
    "language",
    "source_role",
    "proof_status",
    "proof_strength",
    "encoding",
    "packet_body",
    "expected_dict_entries",
    "expected_paths",
    "expected_steps",
    "expected_branch_ids",
    "expected_return_path_ids",
    "expected_gaps",
    "expected_unknowns_or_risks",
    "expected_omitted_count",
    "expected_truncation_reason",
    "expected_expansion_handle",
    "expected_audit_ordered_steps",
];

const REQUIRED_FORBIDDEN_LOCAL_MICRO_FLOW_PACKET_FIELDS: &[&str] = &[
    "reason",
    "forbidden_proof_strength",
    "forbidden_collapsed_branch",
    "forbidden_collapsed_return_path",
    "forbidden_collapsed_shadowed_binding",
    "forbidden_missing_span",
    "forbidden_missing_provenance",
    "forbidden_full_source_body",
    "forbidden_default_ordered_steps_inline",
];

const REQUIRED_PACKET_DELTA_FIELDS: &[&str] = &[
    "added",
    "removed",
    "changed",
    "stale",
    "truncated",
    "unavailable",
    "proof_strength_changed",
];

const REQUIRED_LINTER_PACKET_STATE_FIELDS: &[&str] = &[
    "packet_delta",
    "packet_integrity",
    "packet_layer_status",
    "severity",
    "hard_interrupt_available",
    "recommended_action_kind",
];

const REQUIRED_MUTATION_FIELDS: &[&str] = &[
    "changed_source",
    "expected_removed_facts",
    "expected_added_facts",
    "expected_packet_invalidation",
    "expected_stable_identities",
    "expected_rekeys",
    "post_fix_expectations",
];

const REQUIRED_EDGE_DELTA_FIELDS: &[&str] = &[
    "added",
    "removed",
    "changed",
    "omitted",
    "integrity_changes",
];

const REQUIRED_VALIDATION_FIELDS: &[&str] = &[
    "status",
    "classification",
    "severity",
    "rule_id",
    "hard_interrupt_available",
    "must_fix_before_continuing",
    "recommended_action_kind",
];

const REQUIRED_STORAGE_FIELDS: &[&str] = &[
    "max_expected_rows",
    "no_full_source_storage",
    "no_duplicate_payload",
    "projected_bytes_if_relevant",
];

const REQUIRED_NEGATIVE_ORACLE_FIELDS: &[&str] = &[
    "oracle_id",
    "description",
    "forbidden_claim",
    "expected_classification",
    "why_not_proof",
];

const REQUIRED_POSITIVE_FEATURES: &[&str] = &[
    "local_assignment_chain",
    "return_value_flow",
    "argument_to_call_flow",
    "property_flow",
    "mutation_method",
    "branch_guard",
    "sanitizer_on_actual_path",
    "literal_route_handler_structure",
    "auth_role_direct_call",
    "async_direct_callback",
    "test_assertion_flow",
    "multiple_returns",
    "early_return",
    "destructuring",
    "alias_assignment",
    "closure_capture",
    "same_name_local_shadowing",
    "nested_scopes",
    "partial_broken_code_recovery",
];

const REQUIRED_NEGATIVE_ORACLES: &[&str] = &[
    "comment_only_security_proof_forbidden",
    "string_proximity_sanitizer_proof_forbidden",
    "name_match_alone_not_binding_proof",
    "computed_import_not_exact",
    "reflection_not_exact",
    "dynamic_di_not_exact",
    "macro_hidden_call_not_exact_without_expansion",
    "inactive_c_cpp_preprocessor_branch_not_exact",
    "computed_callback_not_exact",
    "unindexed_cross_language_target_unknown",
    "full_source_body_storage_forbidden",
    "unbounded_micro_node_growth_forbidden",
    "duplicate_micro_facts_forbidden",
    "stale_micro_flow_reuse_forbidden",
    "candidate_vector_match_not_flow_proof",
];

const REQUIRED_MVP4_2_POSITIVE_TAGS: &[&str] = &[
    "simple_expression_return",
    "bare_return",
    "multiple_returns",
    "early_return",
    "if_else_returns",
    "try_return",
    "catch_return",
    "finally_return",
    "async_return",
    "generator_return",
    "class_method_return",
    "nested_function_return",
    "nested_method_callback_boundary",
    "same_shaped_returns",
    "same_function_name_separate_scopes",
    "zero_returns",
];

const REQUIRED_MVP4_2_NEGATIVE_ORACLES: &[&str] = &[
    "return_in_comment_not_edge",
    "return_in_string_literal_not_edge",
    "return_like_identifier_not_edge",
    "top_level_invalid_return_not_claimable",
    "nested_return_not_outer_function",
    "method_return_not_class_frame",
    "unsupported_function_form_no_edge",
    "parser_recovery_return_not_claimable",
    "tsx_js_jsx_declaration_not_applicable",
    "test_generated_mock_stub_not_claimable",
    "duplicate_ast_visitation_forbidden",
    "candidate_vector_text_not_edge_proof",
    "local_returns_to_not_flow_proof",
    "cap_omission_not_complete",
];

const REQUIRED_MVP4_2_INTEGRITY_RULES: &[&str] = &[
    "CG_MVP4_2_EDGE_HEAD_MISSING",
    "CG_MVP4_2_EDGE_TAIL_MISSING",
    "CG_MVP4_2_EDGE_HEAD_KIND_INVALID",
    "CG_MVP4_2_EDGE_TAIL_KIND_INVALID",
    "CG_MVP4_2_EDGE_CROSSES_FILE",
    "CG_MVP4_2_EDGE_CROSSES_FUNCTION",
    "CG_MVP4_2_EDGE_WRONG_ENCLOSING_FUNCTION",
    "CG_MVP4_2_EDGE_MISSING_SOURCE_SPAN",
    "CG_MVP4_2_EDGE_MISSING_PROVENANCE",
    "CG_MVP4_2_EDGE_EXTRACTION_VERSION_MISMATCH",
    "CG_MVP4_2_EDGE_STALE_AFTER_SOURCE_UPDATE",
    "CG_MVP4_2_EDGE_DUPLICATE_ROW",
];

const REQUIRED_MVP4_3_POSITIVE_PACKET_TAGS: &[&str] = &[
    "packet_param_assignment_return",
    "packet_param_assignment_read_return",
    "packet_local_binding_assignment_return",
    "packet_alias_assignment_chain",
    "packet_multiple_return_paths",
    "packet_early_return",
    "packet_if_else_branches",
    "packet_nested_function_separation",
    "packet_class_method_flow",
    "packet_shadowed_binding",
    "packet_same_name_separate_scopes",
    "packet_no_path_found",
    "packet_returns_to_only_no_flow_proof",
    "packet_local_flows_to_provenance",
    "packet_dict_v1_repeated_interning",
];

const REQUIRED_MVP4_3_NEGATIVE_PACKET_ORACLES: &[&str] = &[
    "dynamic_call_target_packet_gap",
    "member_global_unresolved_read_gap",
    "property_index_write_unsupported_gap",
    "destructuring_target_unsupported_gap",
    "parser_recovery_packet_downgrade",
    "test_generated_tsx_js_dts_excluded",
    "cap_truncated_packet_not_flow_proof",
    "packet_missing_provenance_integrity",
    "packet_missing_source_span_integrity",
    "packet_stale_edge_integrity",
    "packet_missing_edge_endpoint_integrity",
    "packet_missing_node_endpoint_integrity",
    "unsupported_language_no_packet_proof",
    "shadowed_binding_collapse_forbidden",
    "branch_collapse_forbidden",
    "return_path_collapse_forbidden",
    "unknown_gap_erasure_forbidden",
    "cap_omission_erasure_forbidden",
    "packet_full_source_body_forbidden",
    "packet_default_ordered_steps_inline_forbidden",
    "returns_to_only_flow_proof_forbidden",
    "incomplete_local_flows_to_flow_proof_forbidden",
];

const REQUIRED_MVP4_3_FUTURE_LANGUAGES: &[&str] = &[
    "javascript",
    "rust",
    "python",
    "go",
    "c_cpp",
    "java",
    "csharp",
    "ruby",
    "php",
];

const ALLOWED_VALIDATION_ACTIONS: &[&str] = &[
    "source_fix",
    "reindex_or_repair",
    "safe_to_continue",
    "unsupported_or_unknown",
];

const FIRST_SLICE_REQUIRED_NODE_KINDS: &[&str] = &[
    "FunctionFrame",
    "Parameter",
    "LocalBinding",
    "AssignmentSite",
    "ReturnSite",
    "CallSite",
    "PropertyAccess",
];

const ALLOWED_SUPPORT_STATUSES: &[&str] = &[
    "first_slice_required",
    "future_required",
    "negative_boundary",
    "unsupported_boundary",
    "release_runner_plan_only",
];

#[cfg(test)]
const ALLOWED_RUNNER_MODES: &[&str] = &[
    "manifest_validation",
    "release_binary_plan",
    "future_product_stages",
];

const ALLOWED_EXACTNESS: &[&str] = &[
    "exact",
    "derived_with_provenance",
    "heuristic",
    "unsupported",
    "unknown",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4FixtureManifest {
    pub schema_version: u32,
    pub generated_at: String,
    pub fixture_root: String,
    pub selected_slice_id: String,
    pub production_extraction_required_by_current_tests: bool,
    pub release_runner_plan: Value,
    pub fixtures: Vec<Mvp4FixtureOracle>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4FixtureOracle {
    pub fixture_id: String,
    pub title: String,
    pub description: String,
    pub language: String,
    pub feature_tier: String,
    pub source_role: String,
    pub required_support_status: String,
    pub tags: Vec<String>,
    pub schema_version: u32,
    pub extraction_version_expectation: String,
    pub initial_source: Mvp4InitialSource,
    pub expected_micro_nodes: Vec<Mvp4ExpectedMicroNode>,
    pub expected_micro_edges: Vec<Mvp4ExpectedMicroEdge>,
    pub expected_packets: Vec<Mvp4ExpectedPacket>,
    pub mutation_update: Mvp4MutationUpdate,
    pub storage: Mvp4StorageExpectation,
    pub negative_oracles: Vec<Mvp4NegativeOracle>,
    #[serde(default)]
    pub forbidden_micro_edges: Vec<Mvp4ForbiddenMicroEdge>,
    #[serde(default)]
    pub expected_micro_edge_delta: Mvp4ExpectedMicroEdgeDelta,
    #[serde(default)]
    pub expected_validation: Vec<Mvp4ExpectedValidation>,
    #[serde(default)]
    pub expected_local_micro_flow_packets: Vec<Mvp4ExpectedLocalMicroFlowPacket>,
    #[serde(default)]
    pub forbidden_local_micro_flow_packets: Vec<Mvp4ForbiddenLocalMicroFlowPacket>,
    #[serde(default)]
    pub expected_packet_delta: Mvp4ExpectedPacketDelta,
    #[serde(default)]
    pub expected_linter_packet_state: Vec<Mvp4ExpectedLinterPacketState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4InitialSource {
    pub files: Vec<Mvp4SourceFile>,
    pub function_scope_anchors: Vec<Value>,
    pub expected_parser_state: String,
    pub expected_unsupported_regions: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4SourceFile {
    pub path: String,
    pub contents: String,
    pub source_role: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4ExpectedMicroNode {
    pub node_kind: String,
    pub semantic_name_or_literal: String,
    pub scope_function: String,
    pub source_span: Value,
    pub binding_status: String,
    pub exactness: String,
    pub source_role: String,
    pub forbidden_duplicate_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4ExpectedMicroEdge {
    pub relation_kind: String,
    pub head_selector: Value,
    pub tail_selector: Value,
    pub source_span: Value,
    pub provenance: Value,
    pub exactness: String,
    pub claimability: String,
    pub forbidden_edges: Vec<Value>,
    #[serde(default)]
    pub expected_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4ForbiddenMicroEdge {
    pub relation_kind: String,
    pub head_selector: Value,
    pub tail_selector: Value,
    pub reason: String,
    pub forbidden_head_tail_pairing: bool,
    pub forbidden_cross_function_link: bool,
    pub forbidden_cross_file_link: bool,
    pub forbidden_duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Mvp4ExpectedMicroEdgeDelta {
    pub added: Vec<Value>,
    pub removed: Vec<Value>,
    pub changed: Vec<Value>,
    pub omitted: Vec<Value>,
    pub integrity_changes: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4ExpectedValidation {
    pub status: String,
    pub classification: String,
    pub severity: String,
    pub rule_id: String,
    pub hard_interrupt_available: bool,
    pub must_fix_before_continuing: bool,
    pub recommended_action_kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4ExpectedPacket {
    pub packet_kind: String,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub packet_body: Option<Value>,
    #[serde(default)]
    pub ordered_steps: Vec<Value>,
    #[serde(default)]
    pub step_spans: Vec<Value>,
    pub proof_strength: String,
    pub unknown_gaps: Vec<String>,
    pub omitted_truncation_expectation: Value,
    pub forbidden_global_interprocedural_claim: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4ExpectedLocalMicroFlowPacket {
    pub packet_id_selector: Value,
    pub function_selector: Value,
    pub language: String,
    pub source_role: String,
    pub proof_status: String,
    pub proof_strength: String,
    pub encoding: String,
    pub packet_body: Value,
    pub expected_dict_entries: Value,
    pub expected_paths: Vec<Value>,
    pub expected_steps: Vec<Value>,
    pub expected_branch_ids: Vec<String>,
    pub expected_return_path_ids: Vec<String>,
    pub expected_gaps: Vec<Value>,
    pub expected_unknowns_or_risks: Vec<String>,
    pub expected_omitted_count: u64,
    pub expected_truncation_reason: String,
    pub expected_expansion_handle: Value,
    pub expected_audit_ordered_steps: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4ForbiddenLocalMicroFlowPacket {
    pub reason: String,
    pub forbidden_proof_strength: String,
    pub forbidden_collapsed_branch: bool,
    pub forbidden_collapsed_return_path: bool,
    pub forbidden_collapsed_shadowed_binding: bool,
    pub forbidden_missing_span: bool,
    pub forbidden_missing_provenance: bool,
    pub forbidden_full_source_body: bool,
    pub forbidden_default_ordered_steps_inline: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Mvp4ExpectedPacketDelta {
    pub added: Vec<Value>,
    pub removed: Vec<Value>,
    pub changed: Vec<Value>,
    pub stale: Vec<Value>,
    pub truncated: Vec<Value>,
    pub unavailable: Vec<Value>,
    pub proof_strength_changed: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4ExpectedLinterPacketState {
    pub packet_delta: Value,
    pub packet_integrity: Value,
    pub packet_layer_status: String,
    pub severity: String,
    pub hard_interrupt_available: bool,
    pub recommended_action_kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4MutationUpdate {
    pub changed_source: Vec<Value>,
    pub expected_removed_facts: Vec<Value>,
    pub expected_added_facts: Vec<Value>,
    pub expected_packet_invalidation: Vec<Value>,
    pub expected_stable_identities: Vec<Value>,
    pub expected_rekeys: Vec<Value>,
    pub post_fix_expectations: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4StorageExpectation {
    pub max_expected_rows: Value,
    pub no_full_source_storage: bool,
    pub no_duplicate_payload: bool,
    pub projected_bytes_if_relevant: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mvp4NegativeOracle {
    pub oracle_id: String,
    pub description: String,
    pub forbidden_claim: String,
    pub expected_classification: String,
    pub why_not_proof: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mvp4FixtureRunnerMode {
    ManifestValidation,
    ReleaseBinaryPlan,
    FutureProductStages,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mvp4FixtureRunnerOptions {
    pub manifest_path: PathBuf,
    pub run_root: PathBuf,
    pub release_binary: Option<PathBuf>,
    pub mode: Mvp4FixtureRunnerMode,
    pub fixture_ids: Vec<String>,
    pub languages: Vec<String>,
    pub feature_tiers: Vec<String>,
    pub tags: Vec<String>,
    pub relation_kinds: Vec<String>,
    pub packet_kinds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4FixtureCommandRecord {
    pub step: String,
    pub command: Vec<String>,
    pub env: Vec<String>,
    pub stdout_log: String,
    pub stderr_log: String,
    pub status: String,
    pub execution_required_now: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4FixtureResult {
    pub fixture_id: String,
    pub language: String,
    pub feature_tier: String,
    pub required_support_status: String,
    pub status: String,
    pub checked_assertions: Vec<String>,
    pub failures: Vec<String>,
    pub expected_micro_edge_count: usize,
    pub forbidden_micro_edge_count: usize,
    pub expected_validation_count: usize,
    pub expected_local_micro_flow_packet_count: usize,
    pub forbidden_local_micro_flow_packet_count: usize,
    pub expected_linter_packet_state_count: usize,
    pub mutation_sequence_count: usize,
    pub proof_overclaim_expectation_count: usize,
    pub command_plan: Vec<Mvp4FixtureCommandRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4FixtureRunReport {
    pub schema_version: u32,
    pub gate: String,
    pub status: String,
    pub ready_to_move_on: bool,
    pub fixture_manifest: String,
    pub run_root: String,
    pub runner_mode: String,
    pub fixtures_total: usize,
    pub fixtures_passed: usize,
    pub fixtures_failed: usize,
    pub positive_fixture_count: usize,
    pub negative_fixture_count: usize,
    pub first_slice_fixture_coverage_complete: bool,
    pub production_extraction_required_by_current_tests: bool,
    pub release_runner_plan_ready: bool,
    pub language_fixture_coverage: BTreeMap<String, usize>,
    pub feature_tier_coverage: BTreeMap<String, usize>,
    pub relation_kind_coverage: BTreeMap<String, usize>,
    pub packet_kind_coverage: BTreeMap<String, usize>,
    pub expected_micro_edge_count: usize,
    pub forbidden_micro_edge_count: usize,
    pub expected_validation_count: usize,
    pub expected_local_micro_flow_packet_count: usize,
    pub forbidden_local_micro_flow_packet_count: usize,
    pub expected_linter_packet_state_count: usize,
    pub mutation_sequence_count: usize,
    pub proof_overclaim_expectation_count: usize,
    pub results: Vec<Mvp4FixtureResult>,
    pub normal_dot_codegraph_mutated: bool,
}

pub fn default_mvp4_fixture_runner_options() -> Mvp4FixtureRunnerOptions {
    let workspace = workspace_root();
    let run_id = format!("mvp4-fixture-oracle-harness-{}", unique_run_suffix());
    Mvp4FixtureRunnerOptions {
        manifest_path: workspace
            .join("fixtures")
            .join("mvp4_micro_flow_oracles")
            .join(MVP4_FIXTURE_ROOT_MANIFEST_FILE),
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
        mode: Mvp4FixtureRunnerMode::ManifestValidation,
        fixture_ids: Vec::new(),
        languages: Vec::new(),
        feature_tiers: Vec::new(),
        tags: Vec::new(),
        relation_kinds: Vec::new(),
        packet_kinds: Vec::new(),
    }
}

pub fn mvp4_fixture_manifest_schema_value() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://codegraph.local/schemas/mvp4_fixture_manifest.schema.json",
        "title": "CodeGraph MVP4 fixture oracle manifest",
        "description": "Dormant manifest contract for MVP4 AST Quantization and micro-flow oracle fixtures. Current tests validate manifests and oracle consistency only; production extraction is not required.",
        "type": "object",
        "additionalProperties": false,
        "required": REQUIRED_ROOT_FIELDS,
        "properties": {
            "schema_version": {"type": "integer", "const": MVP4_FIXTURE_MANIFEST_SCHEMA_VERSION},
            "generated_at": {"type": "string"},
            "fixture_root": {"type": "string"},
            "selected_slice_id": {"type": "string"},
            "production_extraction_required_by_current_tests": {"type": "boolean", "const": false},
            "release_runner_plan": {"type": "object"},
            "fixtures": {
                "type": "array",
                "minItems": 1,
                "items": {"$ref": "#/$defs/fixture"}
            }
        },
        "$defs": {
            "fixture": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_FIXTURE_FIELDS,
                "properties": {
                    "fixture_id": {"type": "string"},
                    "title": {"type": "string"},
                    "description": {"type": "string"},
                    "language": {"type": "string"},
                    "feature_tier": {"type": "string"},
                    "source_role": {"type": "string"},
                    "required_support_status": {"enum": ALLOWED_SUPPORT_STATUSES},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "schema_version": {"type": "integer", "const": MVP4_FIXTURE_MANIFEST_SCHEMA_VERSION},
                    "extraction_version_expectation": {"type": "string"},
                    "initial_source": {"$ref": "#/$defs/initial_source"},
                    "expected_micro_nodes": {"type": "array", "items": {"$ref": "#/$defs/micro_node"}},
                    "expected_micro_edges": {"type": "array", "items": {"$ref": "#/$defs/micro_edge"}},
                    "expected_packets": {"type": "array", "items": {"$ref": "#/$defs/packet"}},
                    "mutation_update": {"$ref": "#/$defs/mutation_update"},
                    "storage": {"$ref": "#/$defs/storage"},
                    "negative_oracles": {"type": "array", "items": {"$ref": "#/$defs/negative_oracle"}},
                    "forbidden_micro_edges": {"type": "array", "items": {"$ref": "#/$defs/forbidden_micro_edge"}},
                    "expected_micro_edge_delta": {"$ref": "#/$defs/micro_edge_delta"},
                    "expected_validation": {"type": "array", "items": {"$ref": "#/$defs/expected_validation"}},
                    "expected_local_micro_flow_packets": {"type": "array", "items": {"$ref": "#/$defs/local_micro_flow_packet"}},
                    "forbidden_local_micro_flow_packets": {"type": "array", "items": {"$ref": "#/$defs/forbidden_local_micro_flow_packet"}},
                    "expected_packet_delta": {"$ref": "#/$defs/packet_delta"},
                    "expected_linter_packet_state": {"type": "array", "items": {"$ref": "#/$defs/linter_packet_state"}}
                }
            },
            "initial_source": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_INITIAL_SOURCE_FIELDS,
                "properties": {
                    "files": {"type": "array", "items": {"$ref": "#/$defs/source_file"}},
                    "function_scope_anchors": {"type": "array"},
                    "expected_parser_state": {"type": "string"},
                    "expected_unsupported_regions": {"type": "array"}
                }
            },
            "source_file": {
                "type": "object",
                "additionalProperties": false,
                "required": ["path", "contents", "source_role"],
                "properties": {
                    "path": {"type": "string"},
                    "contents": {"type": "string"},
                    "source_role": {"type": "string"}
                }
            },
            "micro_node": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_NODE_FIELDS,
                "properties": {
                    "node_kind": {"type": "string"},
                    "semantic_name_or_literal": {"type": "string"},
                    "scope_function": {"type": "string"},
                    "source_span": {"type": "object"},
                    "binding_status": {"type": "string"},
                    "exactness": {"enum": ALLOWED_EXACTNESS},
                    "source_role": {"type": "string"},
                    "forbidden_duplicate_count": {"type": "integer", "minimum": 0}
                }
            },
            "micro_edge": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_EDGE_FIELDS,
                "properties": {
                    "relation_kind": {"type": "string"},
                    "head_selector": {"type": "object"},
                    "tail_selector": {"type": "object"},
                    "source_span": {"type": "object"},
                    "provenance": {"type": "object"},
                    "exactness": {"enum": ALLOWED_EXACTNESS},
                    "claimability": {"type": "string"},
                    "forbidden_edges": {"type": "array"},
                    "expected_count": {"type": "integer", "minimum": 0}
                }
            },
            "forbidden_micro_edge": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_FORBIDDEN_EDGE_FIELDS,
                "properties": {
                    "relation_kind": {"type": "string"},
                    "head_selector": {"type": "object"},
                    "tail_selector": {"type": "object"},
                    "reason": {"type": "string"},
                    "forbidden_head_tail_pairing": {"type": "boolean"},
                    "forbidden_cross_function_link": {"type": "boolean"},
                    "forbidden_cross_file_link": {"type": "boolean"},
                    "forbidden_duplicate": {"type": "boolean"}
                }
            },
            "packet": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_PACKET_BASE_FIELDS,
                "anyOf": [
                    {"required": VERBOSE_PACKET_FIELDS},
                    {"required": COMPACT_PACKET_FIELDS}
                ],
                "properties": {
                    "packet_kind": {"type": "string"},
                    "encoding": {"enum": ["dict_v1"]},
                    "packet_body": {"type": "object"},
                    "ordered_steps": {"type": "array"},
                    "step_spans": {"type": "array"},
                    "proof_strength": {"type": "string"},
                    "unknown_gaps": {"type": "array", "items": {"type": "string"}},
                    "omitted_truncation_expectation": {"type": "object"},
                    "forbidden_global_interprocedural_claim": {"type": "boolean", "const": true}
                }
            },
            "mutation_update": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_MUTATION_FIELDS,
                "properties": {
                    "changed_source": {"type": "array"},
                    "expected_removed_facts": {"type": "array"},
                    "expected_added_facts": {"type": "array"},
                    "expected_packet_invalidation": {"type": "array"},
                    "expected_stable_identities": {"type": "array"},
                    "expected_rekeys": {"type": "array"},
                    "post_fix_expectations": {"type": "object"}
                }
            },
            "local_micro_flow_packet": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_LOCAL_MICRO_FLOW_PACKET_FIELDS,
                "properties": {
                    "packet_id_selector": {"type": "object"},
                    "function_selector": {"type": "object"},
                    "language": {"type": "string"},
                    "source_role": {"type": "string"},
                    "proof_status": {"type": "string"},
                    "proof_strength": {"type": "string"},
                    "encoding": {"enum": ["dict_v1"]},
                    "packet_body": {"type": "object"},
                    "expected_dict_entries": {"type": "object"},
                    "expected_paths": {"type": "array"},
                    "expected_steps": {"type": "array"},
                    "expected_branch_ids": {"type": "array", "items": {"type": "string"}},
                    "expected_return_path_ids": {"type": "array", "items": {"type": "string"}},
                    "expected_gaps": {"type": "array"},
                    "expected_unknowns_or_risks": {"type": "array", "items": {"type": "string"}},
                    "expected_omitted_count": {"type": "integer", "minimum": 0},
                    "expected_truncation_reason": {"type": "string"},
                    "expected_expansion_handle": {"type": "object"},
                    "expected_audit_ordered_steps": {"type": "array"}
                }
            },
            "forbidden_local_micro_flow_packet": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_FORBIDDEN_LOCAL_MICRO_FLOW_PACKET_FIELDS,
                "properties": {
                    "reason": {"type": "string"},
                    "forbidden_proof_strength": {"type": "string"},
                    "forbidden_collapsed_branch": {"type": "boolean"},
                    "forbidden_collapsed_return_path": {"type": "boolean"},
                    "forbidden_collapsed_shadowed_binding": {"type": "boolean"},
                    "forbidden_missing_span": {"type": "boolean"},
                    "forbidden_missing_provenance": {"type": "boolean"},
                    "forbidden_full_source_body": {"type": "boolean"},
                    "forbidden_default_ordered_steps_inline": {"type": "boolean"}
                }
            },
            "packet_delta": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_PACKET_DELTA_FIELDS,
                "properties": {
                    "added": {"type": "array"},
                    "removed": {"type": "array"},
                    "changed": {"type": "array"},
                    "stale": {"type": "array"},
                    "truncated": {"type": "array"},
                    "unavailable": {"type": "array"},
                    "proof_strength_changed": {"type": "array"}
                }
            },
            "linter_packet_state": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_LINTER_PACKET_STATE_FIELDS,
                "properties": {
                    "packet_delta": {"type": "object"},
                    "packet_integrity": {"type": "object"},
                    "packet_layer_status": {"type": "string"},
                    "severity": {"type": "string"},
                    "hard_interrupt_available": {"type": "boolean"},
                    "recommended_action_kind": {"enum": ALLOWED_VALIDATION_ACTIONS}
                }
            },
            "micro_edge_delta": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_EDGE_DELTA_FIELDS,
                "properties": {
                    "added": {"type": "array"},
                    "removed": {"type": "array"},
                    "changed": {"type": "array"},
                    "omitted": {"type": "array"},
                    "integrity_changes": {"type": "array"}
                }
            },
            "storage": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_STORAGE_FIELDS,
                "properties": {
                    "max_expected_rows": {"type": "object"},
                    "no_full_source_storage": {"type": "boolean", "const": true},
                    "no_duplicate_payload": {"type": "boolean", "const": true},
                    "projected_bytes_if_relevant": {"type": "object"}
                }
            },
            "negative_oracle": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_NEGATIVE_ORACLE_FIELDS,
                "properties": {
                    "oracle_id": {"type": "string"},
                    "description": {"type": "string"},
                    "forbidden_claim": {"type": "string"},
                    "expected_classification": {"enum": ALLOWED_EXACTNESS},
                    "why_not_proof": {"type": "string"}
                }
            },
            "expected_validation": {
                "type": "object",
                "additionalProperties": false,
                "required": REQUIRED_VALIDATION_FIELDS,
                "properties": {
                    "status": {"type": "string"},
                    "classification": {"type": "string"},
                    "severity": {"type": "string"},
                    "rule_id": {"type": "string"},
                    "hard_interrupt_available": {"type": "boolean"},
                    "must_fix_before_continuing": {"type": "boolean"},
                    "recommended_action_kind": {"enum": ALLOWED_VALIDATION_ACTIONS}
                }
            }
        }
    })
}

pub fn load_mvp4_fixture_manifest(path: &Path) -> BenchResult<Mvp4FixtureManifest> {
    let raw = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&raw).map_err(|error| {
        BenchmarkError::Parse(format!(
            "failed to parse MVP4 fixture manifest {}: {error}",
            path.display()
        ))
    })?;
    validate_mvp4_fixture_manifest_value(&value)?;
    serde_json::from_value(value).map_err(|error| {
        BenchmarkError::Parse(format!(
            "failed to decode MVP4 fixture manifest {}: {error}",
            path.display()
        ))
    })
}

pub fn validate_mvp4_fixture_manifest_value(value: &Value) -> BenchResult<()> {
    let object = value.as_object().ok_or_else(|| {
        BenchmarkError::Validation("MVP4 fixture manifest must be an object".to_string())
    })?;
    validate_exact_fields("MVP4 root manifest", object, REQUIRED_ROOT_FIELDS)?;
    if value["schema_version"].as_u64() != Some(MVP4_FIXTURE_MANIFEST_SCHEMA_VERSION as u64) {
        return Err(BenchmarkError::Validation(format!(
            "schema_version must be {MVP4_FIXTURE_MANIFEST_SCHEMA_VERSION}"
        )));
    }
    if value["production_extraction_required_by_current_tests"].as_bool() != Some(false) {
        return Err(BenchmarkError::Validation(
            "current MVP4 fixture tests must not require production extraction".to_string(),
        ));
    }
    if value["release_runner_plan"].as_object().is_none() {
        return Err(BenchmarkError::Validation(
            "release_runner_plan must be an object".to_string(),
        ));
    }
    let fixtures = value["fixtures"].as_array().ok_or_else(|| {
        BenchmarkError::Validation("fixtures must be a non-empty array".to_string())
    })?;
    if fixtures.is_empty() {
        return Err(BenchmarkError::Validation(
            "fixtures must be a non-empty array".to_string(),
        ));
    }

    let mut fixture_ids = BTreeSet::new();
    let mut positive_features = BTreeSet::new();
    let mut negative_oracles = BTreeSet::new();
    let mut first_slice_node_kinds = BTreeSet::new();
    let mut mvp4_2_positive_tags = BTreeSet::new();
    let mut mvp4_2_negative_oracles = BTreeSet::new();
    let mut mvp4_2_integrity_rules = BTreeSet::new();
    let mut mvp4_2_validation_actions = BTreeSet::new();
    let mut mvp4_3_positive_tags = BTreeSet::new();
    let mut mvp4_3_negative_oracles = BTreeSet::new();
    let mut mvp4_3_linter_actions = BTreeSet::new();
    let mut mvp4_3_future_languages = BTreeSet::new();
    let mut update_fixture_count = 0usize;
    let mut mvp4_2_fixture_count = 0usize;
    let mut mvp4_2_mutation_fixture_count = 0usize;
    let mut mvp4_2_forbidden_flow_proof_expectation = false;
    let mut mvp4_3_fixture_count = 0usize;
    let mut mvp4_3_mutation_fixture_count = 0usize;
    let mut mvp4_3_dict_v1_oracle_count = 0usize;
    let mut mvp4_3_forbidden_flow_proof_expectation = false;

    for fixture in fixtures {
        validate_fixture_value(fixture)?;
        let fixture_object = fixture.as_object().unwrap();
        let fixture_id = fixture["fixture_id"].as_str().unwrap_or_default();
        if !fixture_ids.insert(fixture_id.to_string()) {
            return Err(BenchmarkError::Validation(format!(
                "duplicate fixture_id: {fixture_id}"
            )));
        }
        let tags = string_set(&fixture["tags"]);
        for feature in REQUIRED_POSITIVE_FEATURES {
            if tags.contains(*feature) {
                positive_features.insert((*feature).to_string());
            }
        }
        let is_mvp4_2 = tags.contains("mvp4_2");
        let is_mvp4_3 = tags.contains("mvp4_3");
        if is_mvp4_2 {
            mvp4_2_fixture_count += 1;
            for feature in REQUIRED_MVP4_2_POSITIVE_TAGS {
                if tags.contains(*feature) {
                    mvp4_2_positive_tags.insert((*feature).to_string());
                }
            }
            if fixture["expected_validation"]
                .as_array()
                .map(|items| items.is_empty())
                .unwrap_or(true)
            {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} MVP4.2 fixtures must encode linter/validation expectations"
                )));
            }
            if fixture["expected_micro_edges"]
                .as_array()
                .map(|items| items.is_empty())
                .unwrap_or(true)
                && fixture["forbidden_micro_edges"]
                    .as_array()
                    .map(|items| items.is_empty())
                    .unwrap_or(true)
                && edge_delta_is_empty(fixture.get("expected_micro_edge_delta"))
            {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} MVP4.2 fixtures must encode edge, forbidden-edge, or delta expectations"
                )));
            }
        }
        if is_mvp4_3 {
            mvp4_3_fixture_count += 1;
            for feature in REQUIRED_MVP4_3_POSITIVE_PACKET_TAGS {
                if tags.contains(*feature) {
                    mvp4_3_positive_tags.insert((*feature).to_string());
                }
            }
            if tags.contains("future_language_scaffold") {
                mvp4_3_future_languages
                    .insert(fixture["language"].as_str().unwrap_or_default().to_string());
            }
            let has_packet_oracle = fixture["expected_local_micro_flow_packets"]
                .as_array()
                .map(|items| !items.is_empty())
                .unwrap_or(false)
                || fixture["forbidden_local_micro_flow_packets"]
                    .as_array()
                    .map(|items| !items.is_empty())
                    .unwrap_or(false)
                || !packet_delta_is_empty(fixture.get("expected_packet_delta"))
                || fixture["expected_linter_packet_state"]
                    .as_array()
                    .map(|items| !items.is_empty())
                    .unwrap_or(false);
            if !has_packet_oracle {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} MVP4.3 fixtures must encode packet, forbidden-packet, delta, or linter packet expectations"
                )));
            }
            for packet in fixture["expected_local_micro_flow_packets"]
                .as_array()
                .unwrap_or(&Vec::new())
            {
                if packet["encoding"].as_str() == Some("dict_v1") {
                    mvp4_3_dict_v1_oracle_count += 1;
                }
                if packet["proof_strength"].as_str() == Some("flow_proof") {
                    let status = packet["proof_status"].as_str().unwrap_or_default();
                    if !matches!(status, "micro_flow_found" | "partial_micro_flow_found") {
                        return Err(BenchmarkError::Validation(format!(
                            "{fixture_id} flow_proof packet oracle must use a flow-capable proof_status"
                        )));
                    }
                }
            }
        }
        for oracle in fixture["negative_oracles"]
            .as_array()
            .unwrap_or(&Vec::new())
        {
            if let Some(oracle_id) = oracle["oracle_id"].as_str() {
                negative_oracles.insert(oracle_id.to_string());
                if REQUIRED_MVP4_2_NEGATIVE_ORACLES.contains(&oracle_id) {
                    mvp4_2_negative_oracles.insert(oracle_id.to_string());
                }
                if REQUIRED_MVP4_3_NEGATIVE_PACKET_ORACLES.contains(&oracle_id) {
                    mvp4_3_negative_oracles.insert(oracle_id.to_string());
                }
                let forbidden_claim = oracle["forbidden_claim"].as_str().unwrap_or_default();
                if forbidden_claim.contains("flow_proof")
                    || forbidden_claim.contains("returned-value")
                    || forbidden_claim.contains("returned value")
                {
                    mvp4_2_forbidden_flow_proof_expectation = true;
                }
                if forbidden_claim.contains("flow_proof")
                    || forbidden_claim.contains("collapsed branch")
                    || forbidden_claim.contains("collapsed return")
                    || forbidden_claim.contains("unknown gap")
                    || forbidden_claim.contains("cap omission")
                {
                    mvp4_3_forbidden_flow_proof_expectation = true;
                }
            }
        }
        for expected in fixture["expected_validation"]
            .as_array()
            .unwrap_or(&Vec::new())
        {
            if let Some(rule_id) = expected["rule_id"].as_str() {
                if REQUIRED_MVP4_2_INTEGRITY_RULES.contains(&rule_id) {
                    mvp4_2_integrity_rules.insert(rule_id.to_string());
                }
            }
            if let Some(action) = expected["recommended_action_kind"].as_str() {
                mvp4_2_validation_actions.insert(action.to_string());
            }
        }
        for expected in fixture["expected_linter_packet_state"]
            .as_array()
            .unwrap_or(&Vec::new())
        {
            if let Some(action) = expected["recommended_action_kind"].as_str() {
                mvp4_3_linter_actions.insert(action.to_string());
            }
        }
        if !fixture["mutation_update"]["changed_source"]
            .as_array()
            .unwrap_or(&Vec::new())
            .is_empty()
        {
            update_fixture_count += 1;
            if is_mvp4_2 {
                mvp4_2_mutation_fixture_count += 1;
            }
            if is_mvp4_3 {
                mvp4_3_mutation_fixture_count += 1;
            }
        }
        if is_mvp4_3 && !packet_delta_is_empty(fixture.get("expected_packet_delta")) {
            mvp4_3_mutation_fixture_count += 1;
        }
        if fixture["language"].as_str() == Some("typescript")
            && tags.contains("first_slice")
            && fixture["required_support_status"].as_str() == Some("first_slice_required")
        {
            for node in fixture["expected_micro_nodes"]
                .as_array()
                .unwrap_or(&Vec::new())
            {
                if let Some(kind) = node["node_kind"].as_str() {
                    first_slice_node_kinds.insert(kind.to_string());
                }
            }
        }
        if fixture_object
            .get("feature_tier")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} feature_tier must not be empty"
            )));
        }
    }

    for feature in REQUIRED_POSITIVE_FEATURES {
        if !positive_features.contains(*feature) {
            return Err(BenchmarkError::Validation(format!(
                "missing required positive fixture feature: {feature}"
            )));
        }
    }
    for oracle in REQUIRED_NEGATIVE_ORACLES {
        if !negative_oracles.contains(*oracle) {
            return Err(BenchmarkError::Validation(format!(
                "missing required negative oracle: {oracle}"
            )));
        }
    }
    for kind in FIRST_SLICE_REQUIRED_NODE_KINDS {
        if !first_slice_node_kinds.contains(*kind) {
            return Err(BenchmarkError::Validation(format!(
                "first slice missing expected micro-node kind: {kind}"
            )));
        }
    }
    if update_fixture_count == 0 {
        return Err(BenchmarkError::Validation(
            "at least one fixture must define mutation/update expectations".to_string(),
        ));
    }
    if mvp4_2_fixture_count == 0 {
        return Err(BenchmarkError::Validation(
            "MVP4.2 LOCAL_RETURNS_TO oracle fixtures must be present".to_string(),
        ));
    }
    for feature in REQUIRED_MVP4_2_POSITIVE_TAGS {
        if !mvp4_2_positive_tags.contains(*feature) {
            return Err(BenchmarkError::Validation(format!(
                "missing MVP4.2 LOCAL_RETURNS_TO positive oracle tag: {feature}"
            )));
        }
    }
    for oracle in REQUIRED_MVP4_2_NEGATIVE_ORACLES {
        if !mvp4_2_negative_oracles.contains(*oracle) {
            return Err(BenchmarkError::Validation(format!(
                "missing MVP4.2 LOCAL_RETURNS_TO negative oracle: {oracle}"
            )));
        }
    }
    for rule in REQUIRED_MVP4_2_INTEGRITY_RULES {
        if !mvp4_2_integrity_rules.contains(*rule) {
            return Err(BenchmarkError::Validation(format!(
                "missing MVP4.2 LOCAL_RETURNS_TO integrity rule oracle: {rule}"
            )));
        }
    }
    for action in ALLOWED_VALIDATION_ACTIONS {
        if !mvp4_2_validation_actions.contains(*action) {
            return Err(BenchmarkError::Validation(format!(
                "missing MVP4.2 validation action kind: {action}"
            )));
        }
    }
    if mvp4_2_mutation_fixture_count == 0 {
        return Err(BenchmarkError::Validation(
            "MVP4.2 fixtures must include mutation/update expectations".to_string(),
        ));
    }
    if !mvp4_2_forbidden_flow_proof_expectation {
        return Err(BenchmarkError::Validation(
            "MVP4.2 fixtures must forbid LOCAL_RETURNS_TO flow_proof overclaims".to_string(),
        ));
    }
    if mvp4_3_fixture_count == 0 {
        return Err(BenchmarkError::Validation(
            "MVP4.3 local micro-flow packet oracle fixtures must be present".to_string(),
        ));
    }
    for feature in REQUIRED_MVP4_3_POSITIVE_PACKET_TAGS {
        if !mvp4_3_positive_tags.contains(*feature) {
            return Err(BenchmarkError::Validation(format!(
                "missing MVP4.3 local micro-flow packet positive oracle tag: {feature}"
            )));
        }
    }
    for oracle in REQUIRED_MVP4_3_NEGATIVE_PACKET_ORACLES {
        if !mvp4_3_negative_oracles.contains(*oracle) {
            return Err(BenchmarkError::Validation(format!(
                "missing MVP4.3 local micro-flow packet negative oracle: {oracle}"
            )));
        }
    }
    for action in ALLOWED_VALIDATION_ACTIONS {
        if !mvp4_3_linter_actions.contains(*action) {
            return Err(BenchmarkError::Validation(format!(
                "missing MVP4.3 packet linter action kind: {action}"
            )));
        }
    }
    for language in REQUIRED_MVP4_3_FUTURE_LANGUAGES {
        if !mvp4_3_future_languages.contains(*language) {
            return Err(BenchmarkError::Validation(format!(
                "missing MVP4.3 future language scaffold: {language}"
            )));
        }
    }
    if mvp4_3_mutation_fixture_count == 0 {
        return Err(BenchmarkError::Validation(
            "MVP4.3 fixtures must include mutation/update packet expectations".to_string(),
        ));
    }
    if mvp4_3_dict_v1_oracle_count == 0 {
        return Err(BenchmarkError::Validation(
            "MVP4.3 fixtures must include dict_v1 packet oracles".to_string(),
        ));
    }
    if !mvp4_3_forbidden_flow_proof_expectation {
        return Err(BenchmarkError::Validation(
            "MVP4.3 fixtures must forbid incomplete or collapsed flow_proof overclaims".to_string(),
        ));
    }
    Ok(())
}

pub fn list_mvp4_fixture_oracles(path: &Path) -> BenchResult<Vec<Mvp4FixtureOracle>> {
    Ok(load_mvp4_fixture_manifest(path)?.fixtures)
}

pub fn run_mvp4_fixture_oracle_harness(
    options: Mvp4FixtureRunnerOptions,
) -> BenchResult<Mvp4FixtureRunReport> {
    let workspace = workspace_root();
    let dot_codegraph_before = dot_codegraph_state(&workspace);
    fs::create_dir_all(&options.run_root)?;
    let manifest = load_mvp4_fixture_manifest(&options.manifest_path)?;
    let mut results = Vec::new();
    for fixture in manifest
        .fixtures
        .iter()
        .filter(|fixture| selected(&options, fixture))
    {
        results.push(run_fixture(&options, fixture)?);
    }
    let fixtures_total = results.len();
    let fixtures_passed = results
        .iter()
        .filter(|result| result.status == "passed")
        .count();
    let fixtures_failed = fixtures_total.saturating_sub(fixtures_passed);
    let mut language_fixture_coverage = BTreeMap::new();
    let mut feature_tier_coverage = BTreeMap::new();
    let mut relation_kind_coverage = BTreeMap::new();
    let mut packet_kind_coverage = BTreeMap::new();
    let mut expected_micro_edge_count = 0usize;
    let mut forbidden_micro_edge_count = 0usize;
    let mut expected_validation_count = 0usize;
    let mut expected_local_micro_flow_packet_count = 0usize;
    let mut forbidden_local_micro_flow_packet_count = 0usize;
    let mut expected_linter_packet_state_count = 0usize;
    let mut mutation_sequence_count = 0usize;
    let mut proof_overclaim_expectation_count = 0usize;
    for fixture in &manifest.fixtures {
        *language_fixture_coverage
            .entry(fixture.language.clone())
            .or_insert(0) += 1;
        *feature_tier_coverage
            .entry(fixture.feature_tier.clone())
            .or_insert(0) += 1;
        for edge in &fixture.expected_micro_edges {
            *relation_kind_coverage
                .entry(edge.relation_kind.clone())
                .or_insert(0) += 1;
        }
        for edge in &fixture.forbidden_micro_edges {
            *relation_kind_coverage
                .entry(edge.relation_kind.clone())
                .or_insert(0) += 1;
        }
        for packet in &fixture.expected_packets {
            *packet_kind_coverage
                .entry(packet.packet_kind.clone())
                .or_insert(0) += 1;
        }
        for _packet in &fixture.expected_local_micro_flow_packets {
            *packet_kind_coverage
                .entry("local_micro_flow_packet".to_string())
                .or_insert(0) += 1;
        }
        expected_micro_edge_count += fixture.expected_micro_edges.len();
        forbidden_micro_edge_count += fixture.forbidden_micro_edges.len();
        expected_validation_count += fixture.expected_validation.len();
        expected_local_micro_flow_packet_count += fixture.expected_local_micro_flow_packets.len();
        forbidden_local_micro_flow_packet_count += fixture.forbidden_local_micro_flow_packets.len();
        expected_linter_packet_state_count += fixture.expected_linter_packet_state.len();
        if !fixture.mutation_update.changed_source.is_empty()
            || !fixture.expected_micro_edge_delta.added.is_empty()
            || !fixture.expected_micro_edge_delta.removed.is_empty()
            || !fixture.expected_micro_edge_delta.changed.is_empty()
            || !fixture.expected_micro_edge_delta.omitted.is_empty()
            || !fixture
                .expected_micro_edge_delta
                .integrity_changes
                .is_empty()
            || !fixture.expected_packet_delta.added.is_empty()
            || !fixture.expected_packet_delta.removed.is_empty()
            || !fixture.expected_packet_delta.changed.is_empty()
            || !fixture.expected_packet_delta.stale.is_empty()
            || !fixture.expected_packet_delta.truncated.is_empty()
            || !fixture.expected_packet_delta.unavailable.is_empty()
            || !fixture
                .expected_packet_delta
                .proof_strength_changed
                .is_empty()
        {
            mutation_sequence_count += 1;
        }
        proof_overclaim_expectation_count += fixture
            .negative_oracles
            .iter()
            .filter(|oracle| {
                let claim = oracle.forbidden_claim.as_str();
                claim.contains("flow_proof")
                    || claim.contains("mutation_proof")
                    || claim.contains("returned-value")
                    || claim.contains("returned value")
                    || claim.contains("runtime")
            })
            .count();
    }
    let positive_fixture_count = manifest
        .fixtures
        .iter()
        .filter(|fixture| !fixture.tags.iter().any(|tag| tag == "negative_oracle_only"))
        .count();
    let negative_fixture_count = manifest
        .fixtures
        .iter()
        .filter(|fixture| !fixture.negative_oracles.is_empty())
        .count();
    let first_slice_fixture_coverage_complete =
        selected_first_slice_node_kinds(&manifest) == required_first_slice_node_kinds();
    let normal_dot_codegraph_mutated = dot_codegraph_before != dot_codegraph_state(&workspace)
        || results
            .iter()
            .flat_map(|result| result.command_plan.iter())
            .any(|record| record.execution_required_now);

    Ok(Mvp4FixtureRunReport {
        schema_version: MVP4_FIXTURE_MANIFEST_SCHEMA_VERSION,
        gate: MVP4_FIXTURE_ORACLE_GATE.to_string(),
        status: if fixtures_failed == 0 {
            "complete".to_string()
        } else {
            "failed".to_string()
        },
        ready_to_move_on: fixtures_failed == 0,
        fixture_manifest: path_string(&options.manifest_path),
        run_root: path_string(&options.run_root),
        runner_mode: runner_mode_name(options.mode).to_string(),
        fixtures_total,
        fixtures_passed,
        fixtures_failed,
        positive_fixture_count,
        negative_fixture_count,
        first_slice_fixture_coverage_complete,
        production_extraction_required_by_current_tests: manifest
            .production_extraction_required_by_current_tests,
        release_runner_plan_ready: release_runner_plan_ready(&manifest),
        language_fixture_coverage,
        feature_tier_coverage,
        relation_kind_coverage,
        packet_kind_coverage,
        expected_micro_edge_count,
        forbidden_micro_edge_count,
        expected_validation_count,
        expected_local_micro_flow_packet_count,
        forbidden_local_micro_flow_packet_count,
        expected_linter_packet_state_count,
        mutation_sequence_count,
        proof_overclaim_expectation_count,
        results,
        normal_dot_codegraph_mutated,
    })
}

pub fn run_mvp4_fixture_oracle_by_id(
    mut options: Mvp4FixtureRunnerOptions,
    fixture_id: impl Into<String>,
) -> BenchResult<Mvp4FixtureRunReport> {
    options.fixture_ids = vec![fixture_id.into()];
    run_mvp4_fixture_oracle_harness(options)
}

pub fn run_mvp4_fixture_oracle_feature(
    mut options: Mvp4FixtureRunnerOptions,
    feature_tier: impl Into<String>,
) -> BenchResult<Mvp4FixtureRunReport> {
    options.feature_tiers = vec![feature_tier.into()];
    run_mvp4_fixture_oracle_harness(options)
}

fn validate_fixture_value(value: &Value) -> BenchResult<()> {
    let object = value
        .as_object()
        .ok_or_else(|| BenchmarkError::Validation("fixture entry must be an object".to_string()))?;
    validate_fields(
        "MVP4 fixture",
        object,
        REQUIRED_FIXTURE_FIELDS,
        OPTIONAL_FIXTURE_FIELDS,
    )?;
    let fixture_id = value["fixture_id"].as_str().unwrap_or_default();
    validate_fixture_id(fixture_id)?;
    validate_string_field(value, "title", fixture_id)?;
    validate_string_field(value, "description", fixture_id)?;
    validate_string_field(value, "language", fixture_id)?;
    validate_string_field(value, "feature_tier", fixture_id)?;
    validate_string_field(value, "source_role", fixture_id)?;
    validate_string_field(value, "extraction_version_expectation", fixture_id)?;
    if value["schema_version"].as_u64() != Some(MVP4_FIXTURE_MANIFEST_SCHEMA_VERSION as u64) {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} schema_version must be {MVP4_FIXTURE_MANIFEST_SCHEMA_VERSION}"
        )));
    }
    let support_status = value["required_support_status"]
        .as_str()
        .unwrap_or_default();
    if !ALLOWED_SUPPORT_STATUSES.contains(&support_status) {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} invalid support status: {support_status}"
        )));
    }
    if value["tags"]
        .as_array()
        .filter(|tags| !tags.is_empty())
        .is_none()
    {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} tags must be non-empty"
        )));
    }
    validate_initial_source(fixture_id, &value["initial_source"])?;
    validate_micro_nodes(fixture_id, &value["expected_micro_nodes"])?;
    validate_micro_edges(fixture_id, &value["expected_micro_edges"])?;
    validate_packets(fixture_id, &value["expected_packets"])?;
    validate_mutation_update(fixture_id, &value["mutation_update"])?;
    validate_storage(fixture_id, &value["storage"])?;
    validate_negative_oracles(fixture_id, &value["negative_oracles"])?;
    if let Some(forbidden_edges) = value.get("forbidden_micro_edges") {
        validate_forbidden_micro_edges(fixture_id, forbidden_edges)?;
    }
    if let Some(edge_delta) = value.get("expected_micro_edge_delta") {
        validate_micro_edge_delta(fixture_id, edge_delta)?;
    }
    if let Some(expected_validation) = value.get("expected_validation") {
        validate_expected_validation(fixture_id, expected_validation)?;
    }
    if let Some(expected_packets) = value.get("expected_local_micro_flow_packets") {
        validate_expected_local_micro_flow_packets(fixture_id, expected_packets)?;
    }
    if let Some(forbidden_packets) = value.get("forbidden_local_micro_flow_packets") {
        validate_forbidden_local_micro_flow_packets(fixture_id, forbidden_packets)?;
    }
    if let Some(packet_delta) = value.get("expected_packet_delta") {
        validate_packet_delta(fixture_id, packet_delta)?;
    }
    if let Some(linter_state) = value.get("expected_linter_packet_state") {
        validate_expected_linter_packet_state(fixture_id, linter_state)?;
    }
    Ok(())
}

fn validate_initial_source(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let object = value.as_object().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} initial_source must be object"))
    })?;
    validate_exact_fields(
        "MVP4 fixture initial_source",
        object,
        REQUIRED_INITIAL_SOURCE_FIELDS,
    )?;
    let files = value["files"].as_array().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} initial_source.files must be array"))
    })?;
    if files.is_empty() {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} must include at least one source file"
        )));
    }
    for file in files {
        let file_object = file.as_object().ok_or_else(|| {
            BenchmarkError::Validation(format!("{fixture_id} source file must be object"))
        })?;
        validate_exact_fields(
            "MVP4 fixture source file",
            file_object,
            &["path", "contents", "source_role"],
        )?;
        let path = file["path"].as_str().unwrap_or_default();
        validate_relative_path(path)?;
        validate_string_field(file, "contents", fixture_id)?;
        validate_string_field(file, "source_role", fixture_id)?;
    }
    if !value["function_scope_anchors"]
        .as_array()
        .map(|anchors| !anchors.is_empty())
        .unwrap_or(false)
    {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} must include function/scope anchors"
        )));
    }
    validate_string_field(value, "expected_parser_state", fixture_id)?;
    if !value["expected_unsupported_regions"].is_array() {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} expected_unsupported_regions must be array"
        )));
    }
    Ok(())
}

fn validate_micro_nodes(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let nodes = value.as_array().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} expected_micro_nodes must be array"))
    })?;
    for node in nodes {
        let object = node.as_object().ok_or_else(|| {
            BenchmarkError::Validation(format!("{fixture_id} micro-node must be object"))
        })?;
        validate_exact_fields("MVP4 micro-node", object, REQUIRED_NODE_FIELDS)?;
        for field in [
            "node_kind",
            "semantic_name_or_literal",
            "scope_function",
            "binding_status",
            "exactness",
            "source_role",
        ] {
            validate_string_field(node, field, fixture_id)?;
        }
        validate_exactness(fixture_id, node["exactness"].as_str().unwrap_or_default())?;
        validate_span(fixture_id, &node["source_span"])?;
        if node["forbidden_duplicate_count"].as_u64().is_none() {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} micro-node forbidden_duplicate_count must be integer"
            )));
        }
    }
    Ok(())
}

fn validate_micro_edges(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let edges = value.as_array().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} expected_micro_edges must be array"))
    })?;
    for edge in edges {
        let object = edge.as_object().ok_or_else(|| {
            BenchmarkError::Validation(format!("{fixture_id} micro-edge must be object"))
        })?;
        validate_fields(
            "MVP4 micro-edge",
            object,
            REQUIRED_EDGE_FIELDS,
            OPTIONAL_EDGE_FIELDS,
        )?;
        validate_string_field(edge, "relation_kind", fixture_id)?;
        validate_string_field(edge, "exactness", fixture_id)?;
        validate_string_field(edge, "claimability", fixture_id)?;
        validate_exactness(fixture_id, edge["exactness"].as_str().unwrap_or_default())?;
        validate_span(fixture_id, &edge["source_span"])?;
        for field in ["head_selector", "tail_selector", "provenance"] {
            if !edge[field].is_object() {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} micro-edge {field} must be object"
                )));
            }
        }
        if !edge["forbidden_edges"].is_array() {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} forbidden_edges must be array"
            )));
        }
        if edge.get("expected_count").is_some() && edge["expected_count"].as_u64().is_none() {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} micro-edge expected_count must be integer"
            )));
        }
    }
    Ok(())
}

fn validate_forbidden_micro_edges(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let edges = value.as_array().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} forbidden_micro_edges must be array"))
    })?;
    for edge in edges {
        let object = edge.as_object().ok_or_else(|| {
            BenchmarkError::Validation(format!("{fixture_id} forbidden micro-edge must be object"))
        })?;
        validate_exact_fields(
            "MVP4 forbidden micro-edge",
            object,
            REQUIRED_FORBIDDEN_EDGE_FIELDS,
        )?;
        for field in ["relation_kind", "reason"] {
            validate_string_field(edge, field, fixture_id)?;
        }
        for field in ["head_selector", "tail_selector"] {
            if !edge[field].is_object() {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} forbidden micro-edge {field} must be object"
                )));
            }
        }
        for field in [
            "forbidden_head_tail_pairing",
            "forbidden_cross_function_link",
            "forbidden_cross_file_link",
            "forbidden_duplicate",
        ] {
            if edge[field].as_bool().is_none() {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} forbidden micro-edge {field} must be boolean"
                )));
            }
        }
    }
    Ok(())
}

fn validate_micro_edge_delta(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let object = value.as_object().ok_or_else(|| {
        BenchmarkError::Validation(format!(
            "{fixture_id} expected_micro_edge_delta must be object"
        ))
    })?;
    validate_exact_fields("MVP4 micro-edge delta", object, REQUIRED_EDGE_DELTA_FIELDS)?;
    for field in REQUIRED_EDGE_DELTA_FIELDS {
        if !value[*field].is_array() {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} expected_micro_edge_delta {field} must be array"
            )));
        }
    }
    Ok(())
}

fn validate_expected_validation(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let expectations = value.as_array().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} expected_validation must be array"))
    })?;
    for expectation in expectations {
        let object = expectation.as_object().ok_or_else(|| {
            BenchmarkError::Validation(format!(
                "{fixture_id} validation expectation must be object"
            ))
        })?;
        validate_exact_fields(
            "MVP4 expected validation",
            object,
            REQUIRED_VALIDATION_FIELDS,
        )?;
        for field in [
            "status",
            "classification",
            "severity",
            "rule_id",
            "recommended_action_kind",
        ] {
            validate_string_field(expectation, field, fixture_id)?;
        }
        if !ALLOWED_VALIDATION_ACTIONS.contains(
            &expectation["recommended_action_kind"]
                .as_str()
                .unwrap_or_default(),
        ) {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} invalid recommended_action_kind {}",
                expectation["recommended_action_kind"]
                    .as_str()
                    .unwrap_or_default()
            )));
        }
        for field in ["hard_interrupt_available", "must_fix_before_continuing"] {
            if expectation[field].as_bool().is_none() {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} validation {field} must be boolean"
                )));
            }
        }
    }
    Ok(())
}

fn validate_packets(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let packets = value.as_array().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} expected_packets must be array"))
    })?;
    for packet in packets {
        let object = packet.as_object().ok_or_else(|| {
            BenchmarkError::Validation(format!("{fixture_id} packet must be object"))
        })?;
        validate_fields(
            "MVP4 packet",
            object,
            REQUIRED_PACKET_BASE_FIELDS,
            &["ordered_steps", "step_spans", "encoding", "packet_body"],
        )?;
        validate_string_field(packet, "packet_kind", fixture_id)?;
        validate_string_field(packet, "proof_strength", fixture_id)?;
        if !packet["unknown_gaps"].is_array() {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} packet unknown_gaps must be array"
            )));
        }
        let has_verbose = VERBOSE_PACKET_FIELDS
            .iter()
            .all(|field| object.contains_key(*field));
        let has_compact = COMPACT_PACKET_FIELDS
            .iter()
            .all(|field| object.contains_key(*field));
        if !has_verbose && !has_compact {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} packet must include either ordered_steps+step_spans or encoding+packet_body"
            )));
        }
        if has_verbose {
            for field in VERBOSE_PACKET_FIELDS {
                if !packet[*field].is_array() {
                    return Err(BenchmarkError::Validation(format!(
                        "{fixture_id} packet {field} must be array"
                    )));
                }
            }
        }
        if has_compact {
            if packet["encoding"].as_str() != Some("dict_v1") {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} compact packet encoding must be dict_v1"
                )));
            }
            validate_compact_packet_body(fixture_id, &packet["packet_body"])?;
        }
        if !packet["omitted_truncation_expectation"].is_object() {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} omitted_truncation_expectation must be object"
            )));
        }
        if packet["forbidden_global_interprocedural_claim"].as_bool() != Some(true) {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} packet must forbid global/interprocedural claims"
            )));
        }
    }
    Ok(())
}

fn validate_compact_packet_body(fixture_id: &str, packet_body: &Value) -> BenchResult<()> {
    let body = packet_body.as_object().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} packet_body must be object"))
    })?;
    validate_fields(
        "MVP4 compact packet_body",
        body,
        &["dictionary", "paths", "compression_contract"],
        &[],
    )?;
    if !packet_body["dictionary"].is_object() {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} packet_body.dictionary must be object"
        )));
    }
    if !packet_body["paths"].is_array() {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} packet_body.paths must be array"
        )));
    }
    let contract = packet_body["compression_contract"]
        .as_object()
        .ok_or_else(|| {
            BenchmarkError::Validation(format!(
                "{fixture_id} packet_body.compression_contract must be object"
            ))
        })?;
    validate_fields(
        "MVP4 packet compression_contract",
        contract,
        REQUIRED_PACKET_COMPRESSION_CONTRACT_FIELDS,
        &[
            "shadowed_binding_identity_preserved",
            "cap_omissions_preserved",
            "verbose_ordered_steps_default_inline",
        ],
    )?;
    for field in REQUIRED_PACKET_COMPRESSION_CONTRACT_FIELDS {
        let expected = *field != "full_source_bodies_allowed";
        if packet_body["compression_contract"][*field].as_bool() != Some(expected) {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} compression_contract {field} must be {expected}"
            )));
        }
    }
    Ok(())
}

fn validate_mutation_update(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let object = value.as_object().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} mutation_update must be object"))
    })?;
    validate_exact_fields("MVP4 mutation_update", object, REQUIRED_MUTATION_FIELDS)?;
    for field in [
        "changed_source",
        "expected_removed_facts",
        "expected_added_facts",
        "expected_packet_invalidation",
        "expected_stable_identities",
        "expected_rekeys",
    ] {
        if !value[field].is_array() {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} mutation_update {field} must be array"
            )));
        }
    }
    if !value["post_fix_expectations"].is_object() {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} post_fix_expectations must be object"
        )));
    }
    Ok(())
}

fn validate_expected_local_micro_flow_packets(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let packets = value.as_array().ok_or_else(|| {
        BenchmarkError::Validation(format!(
            "{fixture_id} expected_local_micro_flow_packets must be array"
        ))
    })?;
    for packet in packets {
        let object = packet.as_object().ok_or_else(|| {
            BenchmarkError::Validation(format!(
                "{fixture_id} local micro-flow packet oracle must be object"
            ))
        })?;
        validate_exact_fields(
            "MVP4.3 expected local micro-flow packet",
            object,
            REQUIRED_LOCAL_MICRO_FLOW_PACKET_FIELDS,
        )?;
        for field in [
            "language",
            "source_role",
            "proof_status",
            "proof_strength",
            "encoding",
            "expected_truncation_reason",
        ] {
            validate_string_field(packet, field, fixture_id)?;
        }
        if packet["encoding"].as_str() != Some("dict_v1") {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} expected local micro-flow packet encoding must be dict_v1"
            )));
        }
        for field in [
            "packet_id_selector",
            "function_selector",
            "expected_dict_entries",
            "expected_expansion_handle",
        ] {
            if !packet[field].is_object() {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} expected local micro-flow packet {field} must be object"
                )));
            }
        }
        validate_compact_packet_body(fixture_id, &packet["packet_body"])?;
        validate_mvp4_3_compression_contract(fixture_id, &packet["packet_body"])?;
        if packet["packet_body"].get("ordered_steps").is_some() {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} compact packet_body must not inline ordered_steps"
            )));
        }
        if value_contains_forbidden_source_body(packet) {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} packet oracle must not contain full source body fields"
            )));
        }
        for field in [
            "expected_paths",
            "expected_steps",
            "expected_branch_ids",
            "expected_return_path_ids",
            "expected_gaps",
            "expected_unknowns_or_risks",
            "expected_audit_ordered_steps",
        ] {
            if !packet[field].is_array() {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} expected local micro-flow packet {field} must be array"
                )));
            }
        }
        if packet["expected_omitted_count"].as_u64().is_none() {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} expected_omitted_count must be integer"
            )));
        }
    }
    Ok(())
}

fn validate_mvp4_3_compression_contract(fixture_id: &str, packet_body: &Value) -> BenchResult<()> {
    let contract = packet_body["compression_contract"]
        .as_object()
        .ok_or_else(|| {
            BenchmarkError::Validation(format!(
                "{fixture_id} packet_body.compression_contract must be object"
            ))
        })?;
    for field in [
        "shadowed_binding_identity_preserved",
        "cap_omissions_preserved",
    ] {
        if contract.get(field).and_then(Value::as_bool) != Some(true) {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} MVP4.3 compression_contract {field} must be true"
            )));
        }
    }
    if contract
        .get("verbose_ordered_steps_default_inline")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} MVP4.3 compression_contract verbose_ordered_steps_default_inline must be false"
        )));
    }
    Ok(())
}

fn validate_forbidden_local_micro_flow_packets(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let packets = value.as_array().ok_or_else(|| {
        BenchmarkError::Validation(format!(
            "{fixture_id} forbidden_local_micro_flow_packets must be array"
        ))
    })?;
    for packet in packets {
        let object = packet.as_object().ok_or_else(|| {
            BenchmarkError::Validation(format!(
                "{fixture_id} forbidden local micro-flow packet must be object"
            ))
        })?;
        validate_exact_fields(
            "MVP4.3 forbidden local micro-flow packet",
            object,
            REQUIRED_FORBIDDEN_LOCAL_MICRO_FLOW_PACKET_FIELDS,
        )?;
        for field in ["reason", "forbidden_proof_strength"] {
            validate_string_field(packet, field, fixture_id)?;
        }
        for field in [
            "forbidden_collapsed_branch",
            "forbidden_collapsed_return_path",
            "forbidden_collapsed_shadowed_binding",
            "forbidden_missing_span",
            "forbidden_missing_provenance",
            "forbidden_full_source_body",
            "forbidden_default_ordered_steps_inline",
        ] {
            if packet[field].as_bool().is_none() {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} forbidden local micro-flow packet {field} must be boolean"
                )));
            }
        }
    }
    Ok(())
}

fn validate_packet_delta(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let object = value.as_object().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} expected_packet_delta must be object"))
    })?;
    validate_exact_fields("MVP4.3 packet delta", object, REQUIRED_PACKET_DELTA_FIELDS)?;
    for field in REQUIRED_PACKET_DELTA_FIELDS {
        if !value[*field].is_array() {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} expected_packet_delta {field} must be array"
            )));
        }
    }
    Ok(())
}

fn validate_expected_linter_packet_state(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let states = value.as_array().ok_or_else(|| {
        BenchmarkError::Validation(format!(
            "{fixture_id} expected_linter_packet_state must be array"
        ))
    })?;
    for state in states {
        let object = state.as_object().ok_or_else(|| {
            BenchmarkError::Validation(format!("{fixture_id} packet linter state must be object"))
        })?;
        validate_exact_fields(
            "MVP4.3 linter packet state",
            object,
            REQUIRED_LINTER_PACKET_STATE_FIELDS,
        )?;
        for field in ["packet_layer_status", "severity", "recommended_action_kind"] {
            validate_string_field(state, field, fixture_id)?;
        }
        for field in ["packet_delta", "packet_integrity"] {
            if !state[field].is_object() {
                return Err(BenchmarkError::Validation(format!(
                    "{fixture_id} packet linter state {field} must be object"
                )));
            }
        }
        if !ALLOWED_VALIDATION_ACTIONS.contains(
            &state["recommended_action_kind"]
                .as_str()
                .unwrap_or_default(),
        ) {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} invalid packet linter recommended_action_kind {}",
                state["recommended_action_kind"]
                    .as_str()
                    .unwrap_or_default()
            )));
        }
        if state["hard_interrupt_available"].as_bool().is_none() {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} packet linter hard_interrupt_available must be boolean"
            )));
        }
    }
    Ok(())
}

fn validate_storage(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let object = value.as_object().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} storage must be object"))
    })?;
    validate_exact_fields("MVP4 storage", object, REQUIRED_STORAGE_FIELDS)?;
    if !value["max_expected_rows"].is_object() {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} max_expected_rows must be object"
        )));
    }
    if value["no_full_source_storage"].as_bool() != Some(true) {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} must forbid full-source storage"
        )));
    }
    if value["no_duplicate_payload"].as_bool() != Some(true) {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} must forbid duplicate payload storage"
        )));
    }
    if !value["projected_bytes_if_relevant"].is_object() {
        return Err(BenchmarkError::Validation(format!(
            "{fixture_id} projected_bytes_if_relevant must be object"
        )));
    }
    Ok(())
}

fn validate_negative_oracles(fixture_id: &str, value: &Value) -> BenchResult<()> {
    let oracles = value.as_array().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} negative_oracles must be array"))
    })?;
    for oracle in oracles {
        let object = oracle.as_object().ok_or_else(|| {
            BenchmarkError::Validation(format!("{fixture_id} negative oracle must be object"))
        })?;
        validate_exact_fields(
            "MVP4 negative oracle",
            object,
            REQUIRED_NEGATIVE_ORACLE_FIELDS,
        )?;
        for field in [
            "oracle_id",
            "description",
            "forbidden_claim",
            "expected_classification",
            "why_not_proof",
        ] {
            validate_string_field(oracle, field, fixture_id)?;
        }
        validate_exactness(
            fixture_id,
            oracle["expected_classification"]
                .as_str()
                .unwrap_or_default(),
        )?;
    }
    Ok(())
}

fn run_fixture(
    options: &Mvp4FixtureRunnerOptions,
    fixture: &Mvp4FixtureOracle,
) -> BenchResult<Mvp4FixtureResult> {
    let run_dir = options.run_root.join(&fixture.fixture_id);
    let logs_dir = run_dir.join("logs");
    fs::create_dir_all(&logs_dir)?;
    write_planned_logs(fixture, &logs_dir)?;
    let command_plan = build_command_plan(options, fixture, &logs_dir);
    let mut checked_assertions = Vec::new();
    let mut failures = Vec::new();
    checked_assertions.push("manifest_decodes".to_string());
    checked_assertions.push("initial_source_has_files".to_string());
    checked_assertions.push("negative_oracles_classified".to_string());
    checked_assertions.push("storage_forbids_full_source".to_string());
    checked_assertions.push("production_extraction_not_required".to_string());
    checked_assertions.push("micro_edge_oracles_validated".to_string());
    checked_assertions.push("forbidden_micro_edge_oracles_validated".to_string());
    checked_assertions.push("linter_expectations_validated".to_string());
    checked_assertions.push("mutation_delta_expectations_validated".to_string());
    checked_assertions.push("local_micro_flow_packet_oracles_validated".to_string());
    checked_assertions.push("forbidden_packet_oracles_validated".to_string());
    checked_assertions.push("packet_delta_expectations_validated".to_string());
    checked_assertions.push("packet_linter_expectations_validated".to_string());
    checked_assertions.push("proof_overclaim_expectations_counted".to_string());
    if !fixture.storage.no_full_source_storage {
        failures.push("storage does not forbid full source".to_string());
    }
    if !fixture.storage.no_duplicate_payload {
        failures.push("storage does not forbid duplicate payload".to_string());
    }
    if fixture
        .expected_packets
        .iter()
        .any(|packet| !packet.forbidden_global_interprocedural_claim)
    {
        failures.push("packet does not forbid global/interprocedural claim".to_string());
    }
    if fixture
        .expected_local_micro_flow_packets
        .iter()
        .any(|packet| packet.encoding != "dict_v1")
    {
        failures.push("local micro-flow packet oracle does not use dict_v1".to_string());
    }
    if options.mode == Mvp4FixtureRunnerMode::ManifestValidation
        && command_plan
            .iter()
            .any(|record| record.execution_required_now)
    {
        failures.push("manifest validation mode must not execute product stages".to_string());
    }
    let status = if failures.is_empty() {
        "passed".to_string()
    } else {
        "failed".to_string()
    };
    let result = Mvp4FixtureResult {
        fixture_id: fixture.fixture_id.clone(),
        language: fixture.language.clone(),
        feature_tier: fixture.feature_tier.clone(),
        required_support_status: fixture.required_support_status.clone(),
        status,
        checked_assertions,
        failures,
        expected_micro_edge_count: fixture.expected_micro_edges.len(),
        forbidden_micro_edge_count: fixture.forbidden_micro_edges.len(),
        expected_validation_count: fixture.expected_validation.len(),
        expected_local_micro_flow_packet_count: fixture.expected_local_micro_flow_packets.len(),
        forbidden_local_micro_flow_packet_count: fixture.forbidden_local_micro_flow_packets.len(),
        expected_linter_packet_state_count: fixture.expected_linter_packet_state.len(),
        mutation_sequence_count: if !fixture.mutation_update.changed_source.is_empty()
            || !fixture.expected_micro_edge_delta.added.is_empty()
            || !fixture.expected_micro_edge_delta.removed.is_empty()
            || !fixture.expected_micro_edge_delta.changed.is_empty()
            || !fixture.expected_micro_edge_delta.omitted.is_empty()
            || !fixture
                .expected_micro_edge_delta
                .integrity_changes
                .is_empty()
            || !fixture.expected_packet_delta.added.is_empty()
            || !fixture.expected_packet_delta.removed.is_empty()
            || !fixture.expected_packet_delta.changed.is_empty()
            || !fixture.expected_packet_delta.stale.is_empty()
            || !fixture.expected_packet_delta.truncated.is_empty()
            || !fixture.expected_packet_delta.unavailable.is_empty()
            || !fixture
                .expected_packet_delta
                .proof_strength_changed
                .is_empty()
        {
            1
        } else {
            0
        },
        proof_overclaim_expectation_count: fixture
            .negative_oracles
            .iter()
            .filter(|oracle| {
                let claim = oracle.forbidden_claim.as_str();
                claim.contains("flow_proof")
                    || claim.contains("mutation_proof")
                    || claim.contains("returned-value")
                    || claim.contains("returned value")
                    || claim.contains("runtime")
            })
            .count(),
        command_plan,
    };
    write_json(&run_dir.join("result.json"), &result)?;
    Ok(result)
}

fn build_command_plan(
    options: &Mvp4FixtureRunnerOptions,
    fixture: &Mvp4FixtureOracle,
    logs_dir: &Path,
) -> Vec<Mvp4FixtureCommandRecord> {
    let binary = options
        .release_binary
        .as_ref()
        .map(|path| path_string(path))
        .unwrap_or_else(|| "<release-binary-not-configured>".to_string());
    let mut records = vec![
        planned_record(
            logs_dir,
            "validate_manifest",
            vec![
                "cargo".to_string(),
                "test".to_string(),
                "-p".to_string(),
                "codegraph-bench".to_string(),
                "mvp4_fixture".to_string(),
                "--lib".to_string(),
            ],
            false,
        ),
        planned_record(
            logs_dir,
            "list_fixture",
            vec![
                "codegraph-bench".to_string(),
                "mvp4-fixture-list".to_string(),
                fixture.fixture_id.clone(),
            ],
            false,
        ),
    ];
    if options.mode != Mvp4FixtureRunnerMode::ManifestValidation {
        records.push(planned_record(
            logs_dir,
            "future_release_binary_stage",
            vec![
                binary,
                "agent-use".to_string(),
                "mvp4-fixture-oracle".to_string(),
                "--fixture-id".to_string(),
                fixture.fixture_id.clone(),
                "--agent-json".to_string(),
            ],
            false,
        ));
    }
    records
}

fn planned_record(
    logs_dir: &Path,
    step: &str,
    command: Vec<String>,
    execution_required_now: bool,
) -> Mvp4FixtureCommandRecord {
    Mvp4FixtureCommandRecord {
        step: step.to_string(),
        command,
        env: vec!["CODEGRAPH_AGENT_USE_DATA_ROOT=<external-profile-root>".to_string()],
        stdout_log: path_string(&logs_dir.join(format!("{step}.stdout.log"))),
        stderr_log: path_string(&logs_dir.join(format!("{step}.stderr.log"))),
        status: "planned_not_executed".to_string(),
        execution_required_now,
    }
}

fn write_planned_logs(fixture: &Mvp4FixtureOracle, logs_dir: &Path) -> BenchResult<()> {
    for step in [
        "validate_manifest",
        "list_fixture",
        "future_release_binary_stage",
    ] {
        fs::write(
            logs_dir.join(format!("{step}.stdout.log")),
            format!(
                "planned MVP4 fixture oracle step for {}; production extraction is not required\n",
                fixture.fixture_id
            ),
        )?;
        fs::write(logs_dir.join(format!("{step}.stderr.log")), "")?;
    }
    Ok(())
}

fn release_runner_plan_ready(manifest: &Mvp4FixtureManifest) -> bool {
    manifest
        .release_runner_plan
        .get("ready")
        .and_then(Value::as_bool)
        == Some(true)
        && manifest
            .release_runner_plan
            .get("requires_release_binary_later")
            .and_then(Value::as_bool)
            == Some(true)
        && manifest
            .release_runner_plan
            .get("executes_now")
            .and_then(Value::as_bool)
            == Some(false)
}

fn selected(options: &Mvp4FixtureRunnerOptions, fixture: &Mvp4FixtureOracle) -> bool {
    (options.fixture_ids.is_empty()
        || options
            .fixture_ids
            .iter()
            .any(|id| id == &fixture.fixture_id))
        && (options.languages.is_empty()
            || options
                .languages
                .iter()
                .any(|language| language == &fixture.language))
        && (options.feature_tiers.is_empty()
            || options
                .feature_tiers
                .iter()
                .any(|tier| tier == &fixture.feature_tier))
        && (options.tags.is_empty()
            || options
                .tags
                .iter()
                .any(|tag| fixture.tags.iter().any(|fixture_tag| fixture_tag == tag)))
        && (options.relation_kinds.is_empty()
            || options
                .relation_kinds
                .iter()
                .any(|relation| fixture_has_relation_kind(fixture, relation)))
        && (options.packet_kinds.is_empty()
            || options
                .packet_kinds
                .iter()
                .any(|packet_kind| fixture_has_packet_kind(fixture, packet_kind)))
}

fn fixture_has_relation_kind(fixture: &Mvp4FixtureOracle, relation_kind: &str) -> bool {
    fixture
        .expected_micro_edges
        .iter()
        .any(|edge| edge.relation_kind == relation_kind)
        || fixture
            .forbidden_micro_edges
            .iter()
            .any(|edge| edge.relation_kind == relation_kind)
        || edge_delta_mentions_relation(&fixture.expected_micro_edge_delta, relation_kind)
}

fn edge_delta_mentions_relation(delta: &Mvp4ExpectedMicroEdgeDelta, relation_kind: &str) -> bool {
    [
        &delta.added,
        &delta.removed,
        &delta.changed,
        &delta.omitted,
        &delta.integrity_changes,
    ]
    .iter()
    .any(|items| {
        items.iter().any(|item| {
            item.get("relation_kind")
                .and_then(Value::as_str)
                .is_some_and(|value| value == relation_kind)
        })
    })
}

fn fixture_has_packet_kind(fixture: &Mvp4FixtureOracle, packet_kind: &str) -> bool {
    fixture
        .expected_packets
        .iter()
        .any(|packet| packet.packet_kind == packet_kind)
        || (packet_kind == "local_micro_flow_packet"
            && !fixture.expected_local_micro_flow_packets.is_empty())
        || packet_delta_mentions_packet(&fixture.expected_packet_delta, packet_kind)
}

fn packet_delta_mentions_packet(delta: &Mvp4ExpectedPacketDelta, packet_kind: &str) -> bool {
    [
        &delta.added,
        &delta.removed,
        &delta.changed,
        &delta.stale,
        &delta.truncated,
        &delta.unavailable,
        &delta.proof_strength_changed,
    ]
    .iter()
    .any(|items| {
        items.iter().any(|item| {
            item.get("packet_kind")
                .and_then(Value::as_str)
                .is_some_and(|value| value == packet_kind)
        })
    })
}

fn selected_first_slice_node_kinds(manifest: &Mvp4FixtureManifest) -> BTreeSet<String> {
    manifest
        .fixtures
        .iter()
        .filter(|fixture| {
            fixture.language == "typescript"
                && fixture.required_support_status == "first_slice_required"
                && fixture.tags.iter().any(|tag| tag == "first_slice")
        })
        .flat_map(|fixture| fixture.expected_micro_nodes.iter())
        .map(|node| node.node_kind.clone())
        .collect()
}

fn required_first_slice_node_kinds() -> BTreeSet<String> {
    FIRST_SLICE_REQUIRED_NODE_KINDS
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .map(|array| {
            array
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn validate_exact_fields(
    label: &str,
    object: &serde_json::Map<String, Value>,
    required: &[&str],
) -> BenchResult<()> {
    validate_fields(label, object, required, &[])
}

fn validate_fields(
    label: &str,
    object: &serde_json::Map<String, Value>,
    required: &[&str],
    optional: &[&str],
) -> BenchResult<()> {
    let allowed = required
        .iter()
        .chain(optional.iter())
        .copied()
        .collect::<BTreeSet<_>>();
    for key in object.keys() {
        if !allowed.contains(key.as_str()) {
            return Err(BenchmarkError::Validation(format!(
                "{label} has unknown field: {key}"
            )));
        }
    }
    for field in required {
        if !object.contains_key(*field) {
            return Err(BenchmarkError::Validation(format!(
                "{label} missing required field: {field}"
            )));
        }
    }
    Ok(())
}

fn edge_delta_is_empty(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_object)
        .map(|object| {
            REQUIRED_EDGE_DELTA_FIELDS.iter().all(|field| {
                object
                    .get(*field)
                    .and_then(Value::as_array)
                    .map(|items| items.is_empty())
                    .unwrap_or(true)
            })
        })
        .unwrap_or(true)
}

fn packet_delta_is_empty(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_object)
        .map(|object| {
            REQUIRED_PACKET_DELTA_FIELDS.iter().all(|field| {
                object
                    .get(*field)
                    .and_then(Value::as_array)
                    .map(|items| items.is_empty())
                    .unwrap_or(true)
            })
        })
        .unwrap_or(true)
}

fn value_contains_forbidden_source_body(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, nested)| {
            matches!(
                key.as_str(),
                "full_source_body" | "source_body" | "source_text" | "full_source"
            ) || value_contains_forbidden_source_body(nested)
        }),
        Value::Array(items) => items.iter().any(value_contains_forbidden_source_body),
        _ => false,
    }
}

fn validate_string_field(value: &Value, field: &str, fixture_id: &str) -> BenchResult<()> {
    if value
        .get(field)
        .and_then(Value::as_str)
        .map(|text| !text.trim().is_empty())
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(BenchmarkError::Validation(format!(
            "{fixture_id} field {field} must be a non-empty string"
        )))
    }
}

fn validate_exactness(fixture_id: &str, exactness: &str) -> BenchResult<()> {
    if ALLOWED_EXACTNESS.contains(&exactness) {
        Ok(())
    } else {
        Err(BenchmarkError::Validation(format!(
            "{fixture_id} invalid exactness/classification: {exactness}"
        )))
    }
}

fn validate_span(fixture_id: &str, span: &Value) -> BenchResult<()> {
    let object = span.as_object().ok_or_else(|| {
        BenchmarkError::Validation(format!("{fixture_id} source_span must be object"))
    })?;
    for field in [
        "file",
        "start_line",
        "start_column",
        "end_line",
        "end_column",
    ] {
        if !object.contains_key(field) {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} source_span missing {field}"
            )));
        }
    }
    validate_relative_path(span["file"].as_str().unwrap_or_default())?;
    for field in ["start_line", "start_column", "end_line", "end_column"] {
        if span[field].as_u64().unwrap_or(0) == 0 {
            return Err(BenchmarkError::Validation(format!(
                "{fixture_id} source_span {field} must be positive"
            )));
        }
    }
    Ok(())
}

fn validate_fixture_id(id: &str) -> BenchResult<()> {
    if id.is_empty()
        || !id.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'
                || character == '-'
        })
    {
        return Err(BenchmarkError::Validation(format!(
            "fixture_id must be lowercase slug: {id}"
        )));
    }
    Ok(())
}

fn validate_relative_path(path: &str) -> BenchResult<()> {
    if path.trim().is_empty() {
        return Err(BenchmarkError::Validation(
            "fixture path must not be empty".to_string(),
        ));
    }
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(BenchmarkError::Validation(format!(
            "fixture path must be repo-relative and stay inside fixture root: {}",
            path.display()
        )));
    }
    Ok(())
}

fn runner_mode_name(mode: Mvp4FixtureRunnerMode) -> &'static str {
    match mode {
        Mvp4FixtureRunnerMode::ManifestValidation => "manifest_validation",
        Mvp4FixtureRunnerMode::ReleaseBinaryPlan => "release_binary_plan",
        Mvp4FixtureRunnerMode::FutureProductStages => "future_product_stages",
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> BenchResult<()> {
    let encoded = serde_json::to_string_pretty(value)
        .map_err(|error| BenchmarkError::Io(format!("failed to encode JSON: {error}")))?;
    fs::write(path, encoded)?;
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
    let metadata = fs::metadata(&path).ok()?;
    Some((metadata.is_dir(), metadata.len()))
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn unique_run_suffix() -> String {
    let sequence = RUN_COUNTER.fetch_add(1, Ordering::SeqCst);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{millis}-{sequence}")
}

static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvp4_fixture_manifest_schema_defined() {
        let schema = mvp4_fixture_manifest_schema_value();
        assert_eq!(
            schema["properties"]["schema_version"]["const"].as_u64(),
            Some(MVP4_FIXTURE_MANIFEST_SCHEMA_VERSION as u64)
        );
        assert_eq!(schema["additionalProperties"].as_bool(), Some(false));
        assert_eq!(
            schema["properties"]["production_extraction_required_by_current_tests"]["const"]
                .as_bool(),
            Some(false)
        );
        assert!(schema["$defs"]["fixture"]["properties"]
            .as_object()
            .expect("fixture properties")
            .contains_key("expected_validation"));
        assert!(schema["$defs"]["fixture"]["properties"]
            .as_object()
            .expect("fixture properties")
            .contains_key("expected_local_micro_flow_packets"));
        assert!(schema["$defs"]["fixture"]["properties"]
            .as_object()
            .expect("fixture properties")
            .contains_key("expected_linter_packet_state"));
        assert!(schema["$defs"]["micro_edge"]["properties"]
            .as_object()
            .expect("edge properties")
            .contains_key("expected_count"));
    }

    #[test]
    fn mvp4_fixture_manifest_validates() {
        let manifest_path = workspace_root()
            .join("fixtures")
            .join("mvp4_micro_flow_oracles")
            .join(MVP4_FIXTURE_ROOT_MANIFEST_FILE);
        let manifest = load_mvp4_fixture_manifest(&manifest_path).expect("MVP4 manifest");
        assert_eq!(
            manifest.schema_version,
            MVP4_FIXTURE_MANIFEST_SCHEMA_VERSION
        );
        assert!(!manifest.production_extraction_required_by_current_tests);
    }

    #[test]
    fn mvp4_fixture_runner_lists_and_runs_manifest_only() {
        let report =
            run_mvp4_fixture_oracle_harness(default_mvp4_fixture_runner_options()).expect("run");
        assert_eq!(report.status, "complete");
        assert!(report.fixtures_total >= REQUIRED_POSITIVE_FEATURES.len());
        assert_eq!(report.fixtures_failed, 0);
        assert!(report.positive_fixture_count >= REQUIRED_POSITIVE_FEATURES.len());
        assert!(report.negative_fixture_count >= REQUIRED_NEGATIVE_ORACLES.len());
        assert!(report.expected_micro_edge_count > 0);
        assert!(report.forbidden_micro_edge_count > 0);
        assert!(report.expected_validation_count > 0);
        assert!(report.expected_local_micro_flow_packet_count > 0);
        assert!(report.forbidden_local_micro_flow_packet_count > 0);
        assert!(report.expected_linter_packet_state_count > 0);
        assert!(report
            .packet_kind_coverage
            .contains_key("local_micro_flow_packet"));
        assert!(report.mutation_sequence_count > 0);
        assert!(report.proof_overclaim_expectation_count > 0);
        assert!(!report.production_extraction_required_by_current_tests);
        assert!(!report.normal_dot_codegraph_mutated);
    }

    #[test]
    fn mvp4_fixture_runner_filters_by_language_and_feature() {
        let mut options = default_mvp4_fixture_runner_options();
        options.languages = vec!["typescript".to_string()];
        options.feature_tiers = vec!["first_slice_micro_nodes".to_string()];
        let report = run_mvp4_fixture_oracle_harness(options).expect("filtered run");
        assert!(report.fixtures_total > 0);
        assert_eq!(report.fixtures_failed, 0);
        assert!(report
            .results
            .iter()
            .all(|result| result.language == "typescript"));
        assert!(report
            .results
            .iter()
            .all(|result| result.feature_tier == "first_slice_micro_nodes"));
    }

    #[test]
    fn mvp4_fixture_runner_filters_by_mvp4_2_relation_kind() {
        let mut options = default_mvp4_fixture_runner_options();
        options.tags = vec!["mvp4_2".to_string()];
        options.relation_kinds = vec!["LOCAL_RETURNS_TO".to_string()];
        let report = run_mvp4_fixture_oracle_harness(options).expect("mvp4.2 relation run");
        assert!(report.fixtures_total >= 4);
        assert_eq!(report.fixtures_failed, 0);
        assert!(
            report
                .relation_kind_coverage
                .get("LOCAL_RETURNS_TO")
                .copied()
                .unwrap_or_default()
                > 0
        );
        assert!(report.results.iter().all(|result| {
            result.expected_micro_edge_count > 0
                || result.forbidden_micro_edge_count > 0
                || result.mutation_sequence_count > 0
        }));
        assert!(report
            .results
            .iter()
            .all(|result| result.expected_validation_count > 0));
    }

    #[test]
    fn mvp4_fixture_runner_filters_by_mvp4_3_packet_kind() {
        let mut options = default_mvp4_fixture_runner_options();
        options.tags = vec!["mvp4_3".to_string()];
        options.packet_kinds = vec!["local_micro_flow_packet".to_string()];
        let report = run_mvp4_fixture_oracle_harness(options).expect("mvp4.3 packet run");
        assert!(report.fixtures_total >= 4);
        assert_eq!(report.fixtures_failed, 0);
        assert!(
            report
                .packet_kind_coverage
                .get("local_micro_flow_packet")
                .copied()
                .unwrap_or_default()
                > 0
        );
        assert!(report.results.iter().all(|result| {
            result.expected_local_micro_flow_packet_count > 0 || result.mutation_sequence_count > 0
        }));
    }

    #[test]
    fn mvp4_fixture_runner_can_run_one_fixture_by_id() {
        let report = run_mvp4_fixture_oracle_by_id(
            default_mvp4_fixture_runner_options(),
            "ts_first_slice_function_local_micro_nodes",
        )
        .expect("single fixture");
        assert_eq!(report.fixtures_total, 1);
        assert_eq!(report.fixtures_passed, 1);
        assert_eq!(
            report.results[0].fixture_id,
            "ts_first_slice_function_local_micro_nodes"
        );
    }

    #[test]
    fn mvp4_first_slice_node_coverage_complete() {
        let manifest = load_mvp4_fixture_manifest(
            &workspace_root()
                .join("fixtures")
                .join("mvp4_micro_flow_oracles")
                .join(MVP4_FIXTURE_ROOT_MANIFEST_FILE),
        )
        .expect("manifest");
        assert_eq!(
            selected_first_slice_node_kinds(&manifest),
            required_first_slice_node_kinds()
        );
    }

    #[test]
    fn mvp4_negative_oracle_ids_complete() {
        let manifest = load_mvp4_fixture_manifest(
            &workspace_root()
                .join("fixtures")
                .join("mvp4_micro_flow_oracles")
                .join(MVP4_FIXTURE_ROOT_MANIFEST_FILE),
        )
        .expect("manifest");
        let ids = manifest
            .fixtures
            .iter()
            .flat_map(|fixture| fixture.negative_oracles.iter())
            .map(|oracle| oracle.oracle_id.as_str())
            .collect::<BTreeSet<_>>();
        for oracle in REQUIRED_NEGATIVE_ORACLES {
            assert!(ids.contains(oracle), "missing negative oracle {oracle}");
        }
    }

    #[test]
    fn mvp4_2_local_returns_to_oracles_complete() {
        let manifest = load_mvp4_fixture_manifest(
            &workspace_root()
                .join("fixtures")
                .join("mvp4_micro_flow_oracles")
                .join(MVP4_FIXTURE_ROOT_MANIFEST_FILE),
        )
        .expect("manifest");
        let mvp4_2_fixtures = manifest
            .fixtures
            .iter()
            .filter(|fixture| fixture.tags.iter().any(|tag| tag == "mvp4_2"))
            .collect::<Vec<_>>();
        assert!(mvp4_2_fixtures.len() >= 5);

        let tags = mvp4_2_fixtures
            .iter()
            .flat_map(|fixture| fixture.tags.iter().map(String::as_str))
            .collect::<BTreeSet<_>>();
        for tag in REQUIRED_MVP4_2_POSITIVE_TAGS {
            assert!(tags.contains(tag), "missing MVP4.2 positive tag {tag}");
        }

        let negative_oracles = mvp4_2_fixtures
            .iter()
            .flat_map(|fixture| fixture.negative_oracles.iter())
            .map(|oracle| oracle.oracle_id.as_str())
            .collect::<BTreeSet<_>>();
        for oracle in REQUIRED_MVP4_2_NEGATIVE_ORACLES {
            assert!(
                negative_oracles.contains(oracle),
                "missing MVP4.2 negative oracle {oracle}"
            );
        }

        let validation_rules = mvp4_2_fixtures
            .iter()
            .flat_map(|fixture| fixture.expected_validation.iter())
            .map(|expectation| expectation.rule_id.as_str())
            .collect::<BTreeSet<_>>();
        for rule in REQUIRED_MVP4_2_INTEGRITY_RULES {
            assert!(
                validation_rules.contains(rule),
                "missing MVP4.2 integrity rule {rule}"
            );
        }

        let actions = mvp4_2_fixtures
            .iter()
            .flat_map(|fixture| fixture.expected_validation.iter())
            .map(|expectation| expectation.recommended_action_kind.as_str())
            .collect::<BTreeSet<_>>();
        for action in ALLOWED_VALIDATION_ACTIONS {
            assert!(actions.contains(action), "missing action {action}");
        }
        assert!(mvp4_2_fixtures.iter().any(|fixture| {
            !fixture.mutation_update.changed_source.is_empty()
                || !fixture.expected_micro_edge_delta.added.is_empty()
                || !fixture.expected_micro_edge_delta.removed.is_empty()
                || !fixture.expected_micro_edge_delta.changed.is_empty()
                || !fixture.expected_micro_edge_delta.omitted.is_empty()
                || !fixture
                    .expected_micro_edge_delta
                    .integrity_changes
                    .is_empty()
        }));
    }

    #[test]
    fn mvp4_3_local_micro_flow_packet_oracles_complete() {
        let manifest = load_mvp4_fixture_manifest(
            &workspace_root()
                .join("fixtures")
                .join("mvp4_micro_flow_oracles")
                .join(MVP4_FIXTURE_ROOT_MANIFEST_FILE),
        )
        .expect("manifest");
        let mvp4_3_fixtures = manifest
            .fixtures
            .iter()
            .filter(|fixture| fixture.tags.iter().any(|tag| tag == "mvp4_3"))
            .collect::<Vec<_>>();
        assert!(mvp4_3_fixtures.len() >= 8);

        let tags = mvp4_3_fixtures
            .iter()
            .flat_map(|fixture| fixture.tags.iter().map(String::as_str))
            .collect::<BTreeSet<_>>();
        for tag in REQUIRED_MVP4_3_POSITIVE_PACKET_TAGS {
            assert!(tags.contains(tag), "missing MVP4.3 positive tag {tag}");
        }

        let negative_oracles = mvp4_3_fixtures
            .iter()
            .flat_map(|fixture| fixture.negative_oracles.iter())
            .map(|oracle| oracle.oracle_id.as_str())
            .collect::<BTreeSet<_>>();
        for oracle in REQUIRED_MVP4_3_NEGATIVE_PACKET_ORACLES {
            assert!(
                negative_oracles.contains(oracle),
                "missing MVP4.3 negative oracle {oracle}"
            );
        }

        let future_languages = mvp4_3_fixtures
            .iter()
            .filter(|fixture| {
                fixture
                    .tags
                    .iter()
                    .any(|tag| tag == "future_language_scaffold")
            })
            .map(|fixture| fixture.language.as_str())
            .collect::<BTreeSet<_>>();
        for language in REQUIRED_MVP4_3_FUTURE_LANGUAGES {
            assert!(
                future_languages.contains(language),
                "missing future language scaffold {language}"
            );
        }

        assert!(mvp4_3_fixtures
            .iter()
            .flat_map(|fixture| fixture.expected_local_micro_flow_packets.iter())
            .all(|packet| packet.encoding == "dict_v1"));
        assert!(mvp4_3_fixtures
            .iter()
            .flat_map(|fixture| fixture.forbidden_local_micro_flow_packets.iter())
            .any(|packet| packet.forbidden_default_ordered_steps_inline));
        assert!(mvp4_3_fixtures.iter().any(|fixture| {
            !fixture.expected_packet_delta.added.is_empty()
                || !fixture.expected_packet_delta.removed.is_empty()
                || !fixture.expected_packet_delta.changed.is_empty()
                || !fixture.expected_packet_delta.stale.is_empty()
                || !fixture.expected_packet_delta.truncated.is_empty()
                || !fixture.expected_packet_delta.unavailable.is_empty()
                || !fixture
                    .expected_packet_delta
                    .proof_strength_changed
                    .is_empty()
        }));
    }

    #[test]
    fn mvp4_release_runner_plan_is_planned_not_executed() {
        let mut options = default_mvp4_fixture_runner_options();
        options.mode = Mvp4FixtureRunnerMode::ReleaseBinaryPlan;
        let report =
            run_mvp4_fixture_oracle_by_id(options, "ts_first_slice_function_local_micro_nodes")
                .expect("release plan");
        assert_eq!(report.runner_mode, "release_binary_plan");
        assert!(report.release_runner_plan_ready);
        assert!(report.results[0]
            .command_plan
            .iter()
            .any(|record| record.step == "future_release_binary_stage"));
        assert!(report.results[0]
            .command_plan
            .iter()
            .all(|record| !record.execution_required_now));
    }

    #[test]
    fn mvp4_allowed_runner_modes_are_documented() {
        for mode in ALLOWED_RUNNER_MODES {
            assert!([
                "manifest_validation",
                "release_binary_plan",
                "future_product_stages"
            ]
            .contains(mode));
        }
    }
}
