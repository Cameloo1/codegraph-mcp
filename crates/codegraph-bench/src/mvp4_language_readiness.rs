//! Executable, production-path MVP4 all-language readiness gate.
//!
//! Manifest validation is deliberately insufficient for readiness. The runner
//! creates one disposable Git repository per registered language, invokes the
//! production `codegraph-mcp` binary with an external agent-use data root,
//! inspects the resulting SQLite rows read-only, and requires the CLI packet
//! query to agree with persisted evidence. Unsupported or incomplete language
//! rows are reported as explicit `unmet` results, never as passes.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use codegraph_core::MicroEdgeKind;
use codegraph_store::SqliteGraphStore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{BenchResult, BenchmarkError};

pub const MVP4_LANGUAGE_READINESS_SCHEMA_VERSION: u32 = 3;
pub const MVP4_LANGUAGE_READINESS_MANIFEST_FILE: &str = "manifest.json";
pub const MVP4_LANGUAGE_READINESS_GATE: &str = "mvp4_3l_all_language_static_flow_readiness";
pub const MVP4_LANGUAGE_READINESS_CONTRACT_ID: &str =
    "mvp4_3l_all_language_static_flow_readiness_v3";

const HISTORICAL_BASELINE_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
const HISTORICAL_BASELINE_SNAPSHOT_ID: &str = "pre_mvp4_3l_implementation_v1";
const CURRENT_READINESS_AUTHORITY: &str = "release_runner_production_execution";

const REQUIRED_LANGUAGE_IDS: [&str; 13] = [
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
];

const REQUIRED_DIMENSIONS: [&str; 5] = [
    "frontend_correctness",
    "project_and_symbol_resolution",
    "compression_required_micro_facts",
    "packet_and_product_integration",
    "executable_evidence",
];

const REQUIRED_SEMANTIC_FIXTURE_CLASSES: [&str; 6] = [
    "core_binding_flow",
    "branch_return_paths",
    "mutation",
    "direct_call_resolution",
    "assert_check_sanitize",
    "dynamic_boundary_negatives",
];

const REQUIRED_CAPABILITY_FLAGS: [&str; 6] = [
    "syntax_exact",
    "span_exact",
    "local_binding_resolved",
    "read_write_extracted",
    "local_dataflow_derived",
    "local_flow_packet_supported",
];

const REQUIRED_MICRO_NODE_KINDS: [&str; 13] = [
    "function_frame",
    "parameter",
    "local_binding",
    "property_access",
    "call_site",
    "return_site",
    "assignment_site",
    "value_use",
    "mutation_site",
    "condition_site",
    "branch_arm",
    "test_assertion",
    "sanitizer_call",
];

const REQUIRED_MICRO_EDGE_KINDS: [&str; 11] = [
    "LOCAL_READS",
    "LOCAL_WRITES",
    "LOCAL_FLOWS_TO",
    "LOCAL_RETURNS_TO",
    "LOCAL_CALLS",
    "LOCAL_MUTATES",
    "LOCAL_CHECKS",
    "LOCAL_GUARDS",
    "LOCAL_BRANCHES_TO",
    "LOCAL_SANITIZES",
    "LOCAL_ASSERTS",
];

const FIXTURE_SHAPE_MARKER: &str = "@codegraph-fixture";
const SANITIZER_MARKER: &str = "@codegraph-sanitizer";
const ASSERTION_MARKER: &str = "@codegraph-assertion";
const SAME_FILE_INTRAPROCEDURAL_SCOPE: &str = "same_file_intraprocedural";
const REQUIRED_COMPATIBILITY_TIER: u8 = 5;
const MAX_COMMAND_STDIN_BYTES: usize = 64 * 1024;
const REQUIRED_MCP_TOOLS: [&str; 4] = [
    "codegraph.status",
    "codegraph.query_local_flow_packets",
    "codegraph.context_pack",
    "codegraph.open_local_flow_packet",
];

const SCOPED_CAPABILITY_FLAGS: [&str; 4] = [
    "local_binding_resolved",
    "read_write_extracted",
    "local_dataflow_derived",
    "local_flow_packet_supported",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4LanguageReadinessManifest {
    pub schema_version: u32,
    pub contract_id: String,
    pub production_extraction_required_by_current_tests: bool,
    pub manifest_validation_is_readiness: bool,
    pub historical_baseline: Mvp4HistoricalBaselineSnapshot,
    pub release_runner: Mvp4ReleaseRunnerContract,
    pub required_dimensions: Vec<String>,
    pub semantic_fixture_classes: Vec<String>,
    pub shared_requirements: Mvp4SharedLanguageRequirements,
    pub languages: Vec<Mvp4LanguageContractRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4HistoricalBaselineSnapshot {
    pub snapshot_schema_version: u32,
    pub snapshot_id: String,
    pub scope: String,
    pub immutable: bool,
    pub captured_before_implementation: bool,
    pub authoritative_for_current_readiness: bool,
    pub current_readiness_authority: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4ReleaseRunnerContract {
    pub executes_now: bool,
    pub production_binary_required: bool,
    pub external_profile_required: bool,
    pub external_trace_root_required: bool,
    pub repo_local_dot_codegraph_forbidden: bool,
    pub repo_local_trace_and_target_forbidden: bool,
    pub bounded_stdin_audit_required: bool,
    pub all_product_outputs_negative_scan_required: bool,
    pub index_command: String,
    pub status_command: String,
    pub doctor_command: String,
    pub packet_query_command: String,
    pub packet_open_command: String,
    pub context_pack_command: String,
    pub validate_edit_command: String,
    pub mcp_serve_command: String,
    pub required_mcp_tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4SharedLanguageRequirements {
    pub capabilities: Vec<Mvp4CapabilityRequirement>,
    pub micro_node_kinds: Vec<String>,
    pub micro_edge_kinds: Vec<String>,
    pub packet: Mvp4PacketRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4CapabilityRequirement {
    pub flag: String,
    pub accepted_statuses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4PacketRequirement {
    pub source_role: String,
    pub encoding: String,
    pub accepted_proof_strengths: Vec<String>,
    pub require_claimable: bool,
    pub require_source_spans: bool,
    pub require_zero_omissions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4LanguageContractRow {
    pub language_id: String,
    pub display_name: String,
    pub file_extensions: Vec<String>,
    pub primary_source_path: String,
    pub supported_static_scope: String,
    pub resolver_requirement: String,
    pub accepted_project_resolver_statuses: Vec<String>,
    pub dynamic_boundaries: Vec<String>,
    pub historical_baseline_status: String,
    pub historical_baseline_unmet_reason: String,
    pub required_for_mvp4_ready: bool,
    pub files: Vec<Mvp4ReadinessFixtureFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4ReadinessFixtureFile {
    pub path: String,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4ManifestReadinessAssessment {
    pub valid: bool,
    pub status: String,
    pub ready_to_enter_mvp4_4: bool,
    pub production_execution_required: bool,
    pub language_rows: usize,
}

#[derive(Debug, Clone)]
pub struct Mvp4LanguageReadinessRunnerOptions {
    pub manifest_path: PathBuf,
    pub run_root: PathBuf,
    pub release_binary: PathBuf,
    pub command_timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mvp4LanguageReadinessVerdict {
    Ready,
    Unmet,
    RunnerError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4CommandEvidence {
    pub step: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub environment: BTreeMap<String, String>,
    pub stdout_log: String,
    pub stderr_log: String,
    pub stdin_supplied: bool,
    pub stdin_bytes: usize,
    pub stdin_audit_log: Option<String>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub success: bool,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4ProductSurfaceEvidence {
    pub cli_status_packet_status: String,
    pub cli_status_packet_rows: usize,
    pub cli_status_matches_persistence: bool,
    pub doctor_packet_status: String,
    pub doctor_packet_rows: usize,
    pub doctor_matches_persistence: bool,
    pub cli_filtered_query_count: usize,
    pub cli_filtered_query_exact: bool,
    pub cli_packet_id: Option<String>,
    pub cli_context_handle_id: Option<String>,
    pub cli_context_handle_matches_query: bool,
    pub cli_packet_body_resolved: bool,
    pub validate_edit_status: String,
    pub validate_edit_packet_delta_safe: bool,
    pub validate_edit_packet_integrity_status: String,
    pub validate_edit_packet_integrity_blocking_count: Option<usize>,
    pub validate_edit_blocking_integrity_absent: bool,
    pub mcp_initialize_succeeded: bool,
    pub mcp_status_packet_status: String,
    pub mcp_status_packet_rows: usize,
    pub mcp_status_matches_persistence: bool,
    pub mcp_filtered_query_count: usize,
    pub mcp_filtered_query_exact: bool,
    pub mcp_selected_packet_id: Option<String>,
    pub mcp_query_context_intersection_found: bool,
    pub mcp_context_handle_id: Option<String>,
    pub mcp_context_handle_matches_query: bool,
    pub mcp_expansion_tool: Option<String>,
    pub mcp_expansion_descriptor_concrete: bool,
    pub mcp_open_packet_body_included: bool,
    pub mcp_open_ordered_steps_absent_by_default: bool,
    pub mcp_open_packet_id_matches_descriptor: bool,
    pub external_db_path: Option<String>,
    pub external_trace_root: Option<String>,
    pub trace_root_external: bool,
    pub repo_local_trace_and_target_absent: bool,
    pub all_cli_mcp_outputs_phantom_free: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4CapabilityEvidence {
    pub status: String,
    pub scope: String,
    pub proof_boundary: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4LanguageReadinessResult {
    pub language_id: String,
    pub display_name: String,
    pub verdict: Mvp4LanguageReadinessVerdict,
    pub historical_baseline_status: String,
    pub historical_baseline_unmet_reason: String,
    pub production_extraction_executed: bool,
    pub observed_support_tier: String,
    pub observed_compatibility_tier: u8,
    pub capability_statuses: BTreeMap<String, String>,
    pub capability_evidence: BTreeMap<String, Mvp4CapabilityEvidence>,
    pub catalog_project_resolver_status: String,
    pub project_resolver_status: String,
    pub project_resolver_scope: String,
    pub observed_micro_node_kinds: Vec<String>,
    pub observed_micro_edge_kinds: Vec<String>,
    pub persisted_packet_count: usize,
    pub query_result_count: usize,
    pub profile_db: Option<String>,
    pub profile_external: bool,
    pub lifecycle_claimable: bool,
    pub repo_local_dot_codegraph_absent: bool,
    pub dynamic_boundary_negative_clean: bool,
    pub product_surfaces: Mvp4ProductSurfaceEvidence,
    pub unmet_reasons: Vec<String>,
    pub commands: Vec<Mvp4CommandEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4LanguageReadinessReport {
    pub schema_version: u32,
    pub gate: String,
    pub contract_id: String,
    pub historical_baseline: Mvp4HistoricalBaselineSnapshot,
    pub status: String,
    pub ready_to_enter_mvp4_4: bool,
    pub manifest_validation_only: bool,
    pub production_extraction_required: bool,
    pub production_extraction_executed_for_all_rows: bool,
    pub external_profile_enforced: bool,
    pub repo_local_dot_codegraph_absent_for_all_rows: bool,
    pub language_rows: usize,
    pub ready_rows: usize,
    pub unmet_rows: usize,
    pub runner_error_rows: usize,
    pub manifest_path: String,
    pub run_root: String,
    pub release_binary: String,
    pub language_catalog_command: Mvp4CommandEvidence,
    pub results: Vec<Mvp4LanguageReadinessResult>,
}

#[derive(Debug, Clone, Default)]
struct LanguageCatalogRow {
    support_tier: String,
    compatibility_tier: u8,
    capabilities: BTreeMap<String, String>,
    capability_scopes: BTreeMap<String, String>,
    scoped_readiness: BTreeMap<String, Vec<CatalogScopedReadiness>>,
    project_resolver_status: String,
}

#[derive(Debug, Clone, Default)]
struct CatalogScopedReadiness {
    status: String,
    scope: String,
    proof_boundary: String,
}

#[derive(Debug, Clone)]
struct CommandSpec {
    step: String,
    program: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    environment: BTreeMap<String, String>,
    stdin: Option<String>,
}

#[derive(Debug)]
struct CommandRun {
    evidence: Mvp4CommandEvidence,
    stdout: String,
}

#[derive(Debug, Clone)]
struct ObservedLanguageEvidence {
    support_tier: String,
    compatibility_tier: u8,
    capability_statuses: BTreeMap<String, String>,
    capability_evidence: BTreeMap<String, Mvp4CapabilityEvidence>,
    catalog_project_resolver_status: String,
    project_resolver_status: String,
    project_resolver_scope: String,
    micro_node_kinds: BTreeSet<String>,
    micro_edge_kinds: BTreeSet<String>,
    persisted_packet_count: usize,
    packet_contract_satisfied: bool,
    query_result_count: usize,
    query_language_matches: bool,
    profile_db: Option<String>,
    profile_external: bool,
    lifecycle_claimable: bool,
    repo_local_dot_codegraph_absent: bool,
    dynamic_boundary_negative_clean: bool,
    product_surfaces: Mvp4ProductSurfaceEvidence,
    production_extraction_executed: bool,
}

pub fn default_mvp4_language_readiness_runner_options(
    release_binary: impl Into<PathBuf>,
) -> Mvp4LanguageReadinessRunnerOptions {
    let workspace = workspace_root();
    Mvp4LanguageReadinessRunnerOptions {
        manifest_path: workspace
            .join("fixtures")
            .join("mvp4_micro_flow_oracles")
            .join(MVP4_LANGUAGE_READINESS_MANIFEST_FILE),
        run_root: std::env::temp_dir().join(format!(
            "codegraph-mvp4-language-readiness-{}",
            unique_suffix()
        )),
        release_binary: release_binary.into(),
        command_timeout: Duration::from_secs(120),
    }
}

pub fn load_mvp4_language_readiness_manifest(
    path: &Path,
) -> BenchResult<Mvp4LanguageReadinessManifest> {
    let raw = fs::read_to_string(path).map_err(|error| {
        BenchmarkError::Io(format!(
            "read MVP4 language manifest {}: {error}",
            path.display()
        ))
    })?;
    let manifest =
        serde_json::from_str::<Mvp4LanguageReadinessManifest>(&raw).map_err(|error| {
            BenchmarkError::Parse(format!(
                "parse MVP4 language manifest {}: {error}",
                path.display()
            ))
        })?;
    validate_mvp4_language_readiness_manifest(&manifest)?;
    Ok(manifest)
}

pub fn validate_mvp4_language_readiness_manifest(
    manifest: &Mvp4LanguageReadinessManifest,
) -> BenchResult<()> {
    if manifest.schema_version != MVP4_LANGUAGE_READINESS_SCHEMA_VERSION {
        return validation_error(format!(
            "language readiness schema must be {}, got {}",
            MVP4_LANGUAGE_READINESS_SCHEMA_VERSION, manifest.schema_version
        ));
    }
    if manifest.contract_id != MVP4_LANGUAGE_READINESS_CONTRACT_ID {
        return validation_error(format!(
            "contract_id must be {MVP4_LANGUAGE_READINESS_CONTRACT_ID}"
        ));
    }
    if !manifest.production_extraction_required_by_current_tests {
        return validation_error("production extraction must be required by the current gate");
    }
    if manifest.manifest_validation_is_readiness {
        return validation_error("manifest validation must never be treated as readiness");
    }
    let historical_baseline = &manifest.historical_baseline;
    if historical_baseline.snapshot_schema_version != HISTORICAL_BASELINE_SNAPSHOT_SCHEMA_VERSION
        || historical_baseline.snapshot_id != HISTORICAL_BASELINE_SNAPSHOT_ID
        || historical_baseline.scope.trim().is_empty()
        || !historical_baseline.immutable
        || !historical_baseline.captured_before_implementation
        || historical_baseline.authoritative_for_current_readiness
        || historical_baseline.current_readiness_authority != CURRENT_READINESS_AUTHORITY
    {
        return validation_error(
            "historical baseline must be the immutable, versioned pre-implementation snapshot and must defer current readiness to production release-runner execution",
        );
    }
    if !manifest.release_runner.executes_now
        || !manifest.release_runner.production_binary_required
        || !manifest.release_runner.external_profile_required
        || !manifest.release_runner.external_trace_root_required
        || !manifest.release_runner.repo_local_dot_codegraph_forbidden
        || !manifest
            .release_runner
            .repo_local_trace_and_target_forbidden
        || !manifest.release_runner.bounded_stdin_audit_required
        || !manifest
            .release_runner
            .all_product_outputs_negative_scan_required
    {
        return validation_error(
            "release runner must execute the complete product matrix with external profile/trace roots, bounded stdin audit, and no repo-local generated state",
        );
    }
    require_exact_strings(
        "release runner MCP tools",
        &manifest.release_runner.required_mcp_tools,
        &REQUIRED_MCP_TOOLS,
    )?;
    for (label, command) in [
        ("index", &manifest.release_runner.index_command),
        ("status", &manifest.release_runner.status_command),
        ("doctor", &manifest.release_runner.doctor_command),
        (
            "packet query",
            &manifest.release_runner.packet_query_command,
        ),
        ("packet open", &manifest.release_runner.packet_open_command),
        (
            "context pack",
            &manifest.release_runner.context_pack_command,
        ),
        (
            "validate edit",
            &manifest.release_runner.validate_edit_command,
        ),
        ("MCP serve", &manifest.release_runner.mcp_serve_command),
    ] {
        if command.trim().is_empty() {
            return validation_error(format!("release runner {label} command must not be empty"));
        }
    }

    require_exact_strings(
        "required_dimensions",
        &manifest.required_dimensions,
        &REQUIRED_DIMENSIONS,
    )?;
    require_exact_strings(
        "semantic_fixture_classes",
        &manifest.semantic_fixture_classes,
        &REQUIRED_SEMANTIC_FIXTURE_CLASSES,
    )?;
    let capability_flags = manifest
        .shared_requirements
        .capabilities
        .iter()
        .map(|requirement| requirement.flag.clone())
        .collect::<Vec<_>>();
    require_exact_strings(
        "shared capability flags",
        &capability_flags,
        &REQUIRED_CAPABILITY_FLAGS,
    )?;
    for requirement in &manifest.shared_requirements.capabilities {
        if requirement.accepted_statuses.is_empty() {
            return validation_error(format!(
                "capability {} must declare accepted statuses",
                requirement.flag
            ));
        }
    }
    require_exact_strings(
        "required micro-node kinds",
        &manifest.shared_requirements.micro_node_kinds,
        &REQUIRED_MICRO_NODE_KINDS,
    )?;
    require_exact_strings(
        "required micro-edge kinds",
        &manifest.shared_requirements.micro_edge_kinds,
        &REQUIRED_MICRO_EDGE_KINDS,
    )?;
    let packet = &manifest.shared_requirements.packet;
    if packet.source_role != "production"
        || packet.encoding != "dict_v1"
        || packet.accepted_proof_strengths.is_empty()
        || !packet.require_claimable
        || !packet.require_source_spans
        || !packet.require_zero_omissions
    {
        return validation_error(
            "packet requirement must enforce production dict_v1, claimability, spans, proof strength, and zero omissions",
        );
    }

    if manifest.languages.len() != REQUIRED_LANGUAGE_IDS.len() {
        return validation_error(format!(
            "language manifest must contain exactly {} rows, got {}",
            REQUIRED_LANGUAGE_IDS.len(),
            manifest.languages.len()
        ));
    }
    let language_ids = manifest
        .languages
        .iter()
        .map(|row| row.language_id.clone())
        .collect::<Vec<_>>();
    require_exact_strings("language rows", &language_ids, &REQUIRED_LANGUAGE_IDS)?;
    for row in &manifest.languages {
        validate_language_row(row)?;
    }
    Ok(())
}

pub fn manifest_readiness_assessment(
    manifest: &Mvp4LanguageReadinessManifest,
) -> BenchResult<Mvp4ManifestReadinessAssessment> {
    validate_mvp4_language_readiness_manifest(manifest)?;
    Ok(Mvp4ManifestReadinessAssessment {
        valid: true,
        status: "manifest_validated_execution_required".to_string(),
        ready_to_enter_mvp4_4: false,
        production_execution_required: true,
        language_rows: manifest.languages.len(),
    })
}

pub fn run_mvp4_language_readiness(
    options: &Mvp4LanguageReadinessRunnerOptions,
) -> BenchResult<Mvp4LanguageReadinessReport> {
    validate_runner_options(options)?;
    let manifest = load_mvp4_language_readiness_manifest(&options.manifest_path)?;
    prepare_empty_run_root(&options.run_root)?;
    let logs_root = options.run_root.join("logs");
    let repos_root = options.run_root.join("repos");
    let profile_root = options.run_root.join("agent-use-data");
    fs::create_dir_all(&logs_root)?;
    fs::create_dir_all(&repos_root)?;
    fs::create_dir_all(&profile_root)?;

    let mut catalog_commands = Vec::new();
    let catalog_json = run_json_stage(
        CommandSpec {
            step: "languages".to_string(),
            program: options.release_binary.clone(),
            args: vec!["languages".to_string(), "--json".to_string()],
            cwd: workspace_root(),
            environment: BTreeMap::new(),
            stdin: None,
        },
        &logs_root.join("catalog"),
        options.command_timeout,
        &mut catalog_commands,
    )
    .map_err(BenchmarkError::Validation)?;
    let catalog = parse_language_catalog(&catalog_json)?;
    let language_catalog_command = catalog_commands.pop().ok_or_else(|| {
        BenchmarkError::Validation("languages command evidence missing".to_string())
    })?;

    let mut results = Vec::with_capacity(manifest.languages.len());
    for row in &manifest.languages {
        let catalog_row = catalog.get(&row.language_id).cloned().unwrap_or_default();
        let result = execute_language_row(
            options,
            &manifest,
            row,
            &catalog_row,
            &repos_root,
            &profile_root,
            &logs_root,
        )?;
        results.push(result);
    }

    let ready_rows = results
        .iter()
        .filter(|result| result.verdict == Mvp4LanguageReadinessVerdict::Ready)
        .count();
    let unmet_rows = results
        .iter()
        .filter(|result| result.verdict == Mvp4LanguageReadinessVerdict::Unmet)
        .count();
    let runner_error_rows = results
        .iter()
        .filter(|result| result.verdict == Mvp4LanguageReadinessVerdict::RunnerError)
        .count();
    let production_extraction_executed_for_all_rows = results
        .iter()
        .all(|result| result.production_extraction_executed);
    let repo_local_dot_codegraph_absent_for_all_rows = results
        .iter()
        .all(|result| result.repo_local_dot_codegraph_absent);
    let external_profile_enforced = results.iter().all(|result| result.profile_external);
    let ready_to_enter_mvp4_4 = ready_rows == REQUIRED_LANGUAGE_IDS.len()
        && unmet_rows == 0
        && runner_error_rows == 0
        && production_extraction_executed_for_all_rows
        && repo_local_dot_codegraph_absent_for_all_rows
        && external_profile_enforced;
    let status = if runner_error_rows > 0 {
        "runner_error"
    } else if ready_to_enter_mvp4_4 {
        "ready"
    } else {
        "unmet"
    };
    let report = Mvp4LanguageReadinessReport {
        schema_version: MVP4_LANGUAGE_READINESS_SCHEMA_VERSION,
        gate: MVP4_LANGUAGE_READINESS_GATE.to_string(),
        contract_id: manifest.contract_id,
        historical_baseline: manifest.historical_baseline,
        status: status.to_string(),
        ready_to_enter_mvp4_4,
        manifest_validation_only: false,
        production_extraction_required: true,
        production_extraction_executed_for_all_rows,
        external_profile_enforced,
        repo_local_dot_codegraph_absent_for_all_rows,
        language_rows: results.len(),
        ready_rows,
        unmet_rows,
        runner_error_rows,
        manifest_path: path_string(&options.manifest_path),
        run_root: path_string(&options.run_root),
        release_binary: path_string(&options.release_binary),
        language_catalog_command,
        results,
    };
    fs::write(
        options.run_root.join("mvp4-language-readiness-report.json"),
        serde_json::to_vec_pretty(&report)
            .map_err(|error| BenchmarkError::Parse(error.to_string()))?,
    )?;
    Ok(report)
}

fn execute_language_row(
    options: &Mvp4LanguageReadinessRunnerOptions,
    manifest: &Mvp4LanguageReadinessManifest,
    row: &Mvp4LanguageContractRow,
    catalog: &LanguageCatalogRow,
    repos_root: &Path,
    profile_root: &Path,
    logs_root: &Path,
) -> BenchResult<Mvp4LanguageReadinessResult> {
    let repo_root = repos_root.join(&row.language_id);
    let row_logs = logs_root.join(&row.language_id);
    fs::create_dir_all(&repo_root)?;
    fs::create_dir_all(&row_logs)?;
    for fixture_file in &row.files {
        let destination = repo_root.join(&fixture_file.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, &fixture_file.contents)?;
    }

    let mut commands = Vec::new();
    for (step, args) in [
        ("git_init", vec!["init", "--quiet"]),
        ("git_add", vec!["add", "--all"]),
        (
            "git_commit",
            vec![
                "-c",
                "user.name=CodeGraph Readiness",
                "-c",
                "user.email=readiness@codegraph.invalid",
                "commit",
                "--quiet",
                "-m",
                "readiness fixture",
            ],
        ),
    ] {
        let spec = CommandSpec {
            step: step.to_string(),
            program: PathBuf::from("git"),
            args: args.into_iter().map(str::to_string).collect(),
            cwd: repo_root.clone(),
            environment: BTreeMap::new(),
            stdin: None,
        };
        if let Err(reason) =
            run_plain_stage(spec, &row_logs, options.command_timeout, &mut commands)
        {
            return Ok(runner_error_result(row, catalog, commands, reason));
        }
    }

    let trace_root = options.run_root.join("traces").join(&row.language_id);
    fs::create_dir_all(&trace_root)?;
    let mut environment = BTreeMap::from([
        (
            "CODEGRAPH_AGENT_USE_DATA_ROOT".to_string(),
            path_string(profile_root),
        ),
        ("CODEGRAPH_TRACE_ROOT".to_string(), path_string(&trace_root)),
    ]);
    let profile_config_json = match run_json_stage(
        CommandSpec {
            step: "agent_use_mcp_config".to_string(),
            program: options.release_binary.clone(),
            args: vec![
                "agent-use".to_string(),
                "mcp-config".to_string(),
                "--repo".to_string(),
                path_string(&repo_root),
                "--json".to_string(),
            ],
            cwd: repo_root.clone(),
            environment: environment.clone(),
            stdin: None,
        },
        &row_logs,
        options.command_timeout,
        &mut commands,
    ) {
        Ok(value) => value,
        Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
    };
    let Some(configured_db) =
        find_string_for_keys(&profile_config_json, &["resolved_db", "db_path", "db"])
    else {
        return Ok(runner_error_result(
            row,
            catalog,
            commands,
            "agent-use mcp-config did not expose the production profile DB path".to_string(),
        ));
    };
    environment.insert("CODEGRAPH_DB_PATH".to_string(), configured_db.clone());
    let index_json = match run_json_stage(
        CommandSpec {
            step: "agent_use_index".to_string(),
            program: options.release_binary.clone(),
            args: vec![
                "agent-use".to_string(),
                "index".to_string(),
                "--repo".to_string(),
                path_string(&repo_root),
                "--fresh".to_string(),
                "--json".to_string(),
            ],
            cwd: repo_root.clone(),
            environment: environment.clone(),
            stdin: None,
        },
        &row_logs,
        options.command_timeout,
        &mut commands,
    ) {
        Ok(value) => value,
        Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
    };
    let status_json = match run_json_stage(
        CommandSpec {
            step: "agent_use_status".to_string(),
            program: options.release_binary.clone(),
            args: vec![
                "agent-use".to_string(),
                "status".to_string(),
                "--repo".to_string(),
                path_string(&repo_root),
                "--json".to_string(),
            ],
            cwd: repo_root.clone(),
            environment: environment.clone(),
            stdin: None,
        },
        &row_logs,
        options.command_timeout,
        &mut commands,
    ) {
        Ok(value) => value,
        Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
    };
    let query_json = match run_json_stage(
        CommandSpec {
            step: "agent_use_query_local_flow".to_string(),
            program: options.release_binary.clone(),
            args: vec![
                "agent-use".to_string(),
                "query".to_string(),
                "local-flow".to_string(),
                "--repo".to_string(),
                path_string(&repo_root),
                "--file".to_string(),
                row.primary_source_path.clone(),
                "--language".to_string(),
                row.language_id.clone(),
                "--source-role".to_string(),
                "production".to_string(),
                "--limit".to_string(),
                "100".to_string(),
                "--agent-json".to_string(),
            ],
            cwd: repo_root.clone(),
            environment: environment.clone(),
            stdin: None,
        },
        &row_logs,
        options.command_timeout,
        &mut commands,
    ) {
        Ok(value) => value,
        Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
    };

    let Some((packet_id_value, function_entity_id_value)) =
        selected_cli_packet_identity(&query_json)
    else {
        return Ok(runner_error_result(
            row,
            catalog,
            commands,
            "filtered production local-flow query did not expose a packet id and function entity id"
                .to_string(),
        ));
    };
    let packet_id = Some(packet_id_value.clone());
    let doctor_json = match run_json_stage(
        CommandSpec {
            step: "doctor".to_string(),
            program: options.release_binary.clone(),
            args: vec![
                "doctor".to_string(),
                path_string(&repo_root),
                "--json".to_string(),
            ],
            cwd: repo_root.clone(),
            environment: environment.clone(),
            stdin: None,
        },
        &row_logs,
        options.command_timeout,
        &mut commands,
    ) {
        Ok(value) => value,
        Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
    };
    let packet_open_json = match run_json_stage(
        CommandSpec {
            step: "agent_use_open_local_flow_packet".to_string(),
            program: options.release_binary.clone(),
            args: vec![
                "agent-use".to_string(),
                "query".to_string(),
                "local-flow".to_string(),
                "--repo".to_string(),
                path_string(&repo_root),
                "--packet-id".to_string(),
                packet_id_value.clone(),
                "--include-packet-body".to_string(),
                "--explain".to_string(),
                "--agent-json".to_string(),
            ],
            cwd: repo_root.clone(),
            environment: environment.clone(),
            stdin: None,
        },
        &row_logs,
        options.command_timeout,
        &mut commands,
    ) {
        Ok(value) => value,
        Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
    };
    let context_json = match run_json_stage(
        CommandSpec {
            step: "agent_use_context_pack".to_string(),
            program: options.release_binary.clone(),
            args: vec![
                "agent-use".to_string(),
                "context-pack".to_string(),
                "--repo".to_string(),
                path_string(&repo_root),
                "--task".to_string(),
                format!("inspect static local flow in {}", row.primary_source_path),
                "--seed".to_string(),
                function_entity_id_value,
                "--agent-json".to_string(),
            ],
            cwd: repo_root.clone(),
            environment: environment.clone(),
            stdin: None,
        },
        &row_logs,
        options.command_timeout,
        &mut commands,
    ) {
        Ok(value) => value,
        Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
    };
    let validate_edit_json = match run_json_stage(
        CommandSpec {
            step: "agent_use_validate_edit".to_string(),
            program: options.release_binary.clone(),
            args: vec![
                "agent-use".to_string(),
                "validate-edit".to_string(),
                "--repo".to_string(),
                path_string(&repo_root),
                "--changed".to_string(),
                row.primary_source_path.clone(),
                "--explain".to_string(),
                "--agent-json".to_string(),
            ],
            cwd: repo_root.clone(),
            environment: environment.clone(),
            stdin: None,
        },
        &row_logs,
        options.command_timeout,
        &mut commands,
    ) {
        Ok(value) => value,
        Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
    };

    let profile_db = find_string_for_keys(&status_json, &["resolved_db", "db_path", "db"])
        .or_else(|| find_string_for_keys(&index_json, &["resolved_db", "db_path", "db"]))
        .or_else(|| Some(configured_db));
    let Some(profile_db_value) = profile_db.clone() else {
        return Ok(runner_error_result(
            row,
            catalog,
            commands,
            "production index/status output did not expose the resolved DB path".to_string(),
        ));
    };
    let profile_db_path = PathBuf::from(&profile_db_value);
    let profile_external = profile_is_external(&profile_db_path, profile_root, &repo_root);
    let repo_local_dot_codegraph_absent = !repo_root.join(".codegraph").exists();
    let lifecycle_claimable = status_json
        .get("claimable")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mcp_first_payload = match mcp_session_payload(&[
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize"
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "codegraph.status",
                "arguments": {
                    "repo": path_string(&repo_root),
                    "db_path": profile_db_value.clone()
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "codegraph.query_local_flow_packets",
                "arguments": {
                    "repo": path_string(&repo_root),
                    "db_path": profile_db_value.clone(),
                    "file": row.primary_source_path.clone(),
                    "language": row.language_id.clone(),
                    "source_role": "production",
                    "limit": 100
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "codegraph.context_pack",
                "arguments": {
                    "repo": path_string(&repo_root),
                    "db_path": profile_db_value.clone(),
                    "task": format!("inspect static local flow in {}", row.primary_source_path),
                    "seeds": [row.primary_source_path.clone()],
                    "limit": 8,
                    "response_mode": "compact"
                }
            }
        }),
    ]) {
        Ok(payload) => payload,
        Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
    };
    let mcp_first_responses = match run_json_lines_stage(
        CommandSpec {
            step: "mcp_initialize_status_query_context".to_string(),
            program: options.release_binary.clone(),
            args: vec!["serve-mcp".to_string()],
            cwd: repo_root.clone(),
            environment: environment.clone(),
            stdin: Some(mcp_first_payload),
        },
        &row_logs,
        options.command_timeout,
        &mut commands,
    ) {
        Ok(responses) => responses,
        Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
    };
    let mcp_initialize_succeeded =
        mcp_response_for_id(&mcp_first_responses, 1).is_ok_and(|response| {
            response.get("error").is_none()
                && response
                    .pointer("/result/serverInfo/name")
                    .and_then(Value::as_str)
                    == Some("codegraph-mcp")
        });
    let mcp_status_json =
        match mcp_response_for_id(&mcp_first_responses, 2).and_then(mcp_structured_content) {
            Ok(value) => value.clone(),
            Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
        };
    let mcp_query_json =
        match mcp_response_for_id(&mcp_first_responses, 3).and_then(mcp_structured_content) {
            Ok(value) => value.clone(),
            Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
        };
    let mcp_context_json =
        match mcp_response_for_id(&mcp_first_responses, 4).and_then(mcp_structured_content) {
            Ok(value) => value.clone(),
            Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
        };
    let mcp_selection = select_mcp_query_context_packet(&mcp_query_json, &mcp_context_json);
    let mcp_selected_packet_id = mcp_selection
        .as_ref()
        .map(|selection| selection.packet_id.clone());
    let mcp_context_handle = mcp_selection
        .as_ref()
        .map(|selection| &selection.context_handle);
    let mcp_expansion = mcp_context_handle.and_then(|handle| handle.get("mcp_expansion"));
    let mcp_expansion_tool = mcp_expansion
        .and_then(|expansion| expansion.get("tool"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let mcp_expansion_arguments = mcp_expansion
        .and_then(|expansion| expansion.get("arguments"))
        .cloned();
    let mcp_expansion_descriptor_concrete = mcp_selected_packet_id
        .as_deref()
        .zip(mcp_expansion)
        .is_some_and(|(selected_packet_id, expansion)| {
            mcp_expansion_descriptor_is_concrete(
                expansion,
                &repo_root,
                &profile_db_path,
                selected_packet_id,
            )
        });
    let mcp_open_json = if mcp_expansion_descriptor_concrete {
        let (Some(mcp_expansion_tool_value), Some(mcp_expansion_arguments_value)) =
            (mcp_expansion_tool.clone(), mcp_expansion_arguments.clone())
        else {
            return Ok(runner_error_result(
                row,
                catalog,
                commands,
                "validated MCP packet-open descriptor became unavailable".to_string(),
            ));
        };
        let mcp_open_payload = match mcp_session_payload(&[
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize"
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": mcp_expansion_tool_value,
                    "arguments": mcp_expansion_arguments_value
                }
            }),
        ]) {
            Ok(payload) => payload,
            Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
        };
        let mcp_open_responses = match run_json_lines_stage(
            CommandSpec {
                step: "mcp_initialize_open_local_flow_packet".to_string(),
                program: options.release_binary.clone(),
                args: vec!["serve-mcp".to_string()],
                cwd: repo_root.clone(),
                environment: environment.clone(),
                stdin: Some(mcp_open_payload),
            },
            &row_logs,
            options.command_timeout,
            &mut commands,
        ) {
            Ok(responses) => responses,
            Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
        };
        match mcp_response_for_id(&mcp_open_responses, 2).and_then(mcp_structured_content) {
            Ok(value) => value.clone(),
            Err(reason) => return Ok(runner_error_result(row, catalog, commands, reason)),
        }
    } else {
        Value::Null
    };

    let store = match SqliteGraphStore::open_read_only(&profile_db_path) {
        Ok(store) => store,
        Err(error) => {
            return Ok(runner_error_result(
                row,
                catalog,
                commands,
                format!("open production profile DB read-only: {error}"),
            ));
        }
    };
    let micro_nodes = store
        .ast_micro_nodes_for_file(&row.primary_source_path)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let micro_edges = store
        .ast_micro_edges_for_file(&row.primary_source_path)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let packets = store
        .local_flow_packets_for_file(&row.primary_source_path)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let micro_node_kinds = micro_nodes
        .iter()
        .filter(|node| {
            node.language == row.language_id
                && node.source_role == "production"
                && !node.source_span_id.trim().is_empty()
                && exact_or_provenanced(&node.exactness, node.provenance_id.as_deref())
                && claimability_is_claimable(&node.claimability)
        })
        .map(|node| node.micro_kind.clone())
        .collect::<BTreeSet<_>>();
    let micro_edge_kinds = micro_edges
        .iter()
        .filter(|edge| {
            edge.language == row.language_id
                && edge.source_role == "production"
                && edge.source_span_id.is_some()
                && exact_or_provenanced(&edge.exactness, edge.provenance_id.as_deref())
                && claimability_is_claimable(&edge.claimability)
        })
        .filter_map(|edge| micro_edge_contract_kind_from_storage(&edge.relation_kind))
        .collect::<BTreeSet<_>>();
    let packet_requirement = &manifest.shared_requirements.packet;
    let production_packets = packets
        .iter()
        .filter(|packet| {
            packet.language == row.language_id
                && packet.source_role == packet_requirement.source_role
        })
        .collect::<Vec<_>>();
    let packet_contract_satisfied = production_packets.iter().any(|packet| {
        packet.language == row.language_id
            && packet.source_role == packet_requirement.source_role
            && packet.encoding == packet_requirement.encoding
            && packet_requirement
                .accepted_proof_strengths
                .iter()
                .any(|strength| strength == &packet.proof_strength)
            && (!packet_requirement.require_claimable
                || claimability_is_claimable(&packet.claimability))
            && (!packet_requirement.require_source_spans || packet.primary_source_span_id.is_some())
            && (!packet_requirement.require_zero_omissions || packet.omitted_count == 0)
    });
    let all_cli_mcp_outputs_phantom_free = command_logs_are_phantom_free(&commands);
    let dynamic_boundary_negative_clean = micro_nodes.iter().all(|node| {
        node.symbol
            .as_deref()
            .is_none_or(|symbol| !symbol.contains("phantom_dynamic_target"))
    }) && packets
        .iter()
        .all(|packet| !packet.packet_body.contains("phantom_dynamic_target"))
        && all_cli_mcp_outputs_phantom_free;
    let query_result_count = local_flow_query_total_result_count(&query_json);
    let query_results = query_json
        .get("results")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let query_language_matches = query_results
        .iter()
        .all(|result| result.get("language").and_then(Value::as_str) == Some(&row.language_id));
    let cli_filtered_query_exact = local_flow_filtered_query_is_exact(
        &query_json,
        production_packets.len(),
        &row.primary_source_path,
        &row.language_id,
        "production",
    );
    let cli_context_handle = context_json
        .get("micro_flow_handles")
        .and_then(Value::as_array)
        .and_then(|handles| {
            handles.iter().find(|handle| {
                handle.get("packet_id").and_then(Value::as_str) == Some(packet_id_value.as_str())
            })
        });
    let cli_status_packet_status = status_json
        .get("mvp4_local_flow_packet_status")
        .and_then(Value::as_str)
        .unwrap_or("missing")
        .to_string();
    let cli_status_packet_rows = status_json
        .get("mvp4_local_flow_packet_rows")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let doctor_packet_status = doctor_json
        .get("mvp4_local_flow_packet_status")
        .and_then(Value::as_str)
        .unwrap_or("missing")
        .to_string();
    let doctor_packet_rows = doctor_json
        .get("mvp4_local_flow_packet_rows")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let cli_packet_body_resolved = packet_open_json
        .get("results")
        .and_then(Value::as_array)
        .is_some_and(|results| {
            results.iter().any(|result| {
                result.get("packet_id").and_then(Value::as_str) == Some(packet_id_value.as_str())
                    && result.get("packet_body_included").and_then(Value::as_bool) == Some(true)
                    && result
                        .get("packet_body")
                        .is_some_and(|body| !body.is_null())
            })
        });
    let validate_edit_status = validate_edit_json
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("missing")
        .to_string();
    let validate_edit_packet_delta_safe = validate_edit_json
        .pointer("/micro_flow_packet_delta/normal_packet_delta_not_error")
        .and_then(Value::as_bool)
        == Some(true);
    let (validate_edit_packet_integrity, validate_edit_packet_integrity_blocking_count) =
        validate_edit_packet_integrity_evidence(&validate_edit_json);
    let validate_edit_packet_integrity_status = validate_edit_packet_integrity.as_str().to_string();
    let validate_edit_blocking_integrity_absent =
        validate_edit_packet_integrity == ValidateEditPacketIntegrity::Clear;
    let mcp_status_packet_status = mcp_status_json
        .get("mvp4_local_flow_packet_status")
        .and_then(Value::as_str)
        .unwrap_or("missing")
        .to_string();
    let mcp_status_packet_rows = mcp_status_json
        .get("mvp4_local_flow_packet_rows")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let mcp_query_results = mcp_query_json
        .get("results")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mcp_filtered_query_count = mcp_query_json
        .get("result_count")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let mcp_filtered_query_exact = mcp_filtered_query_count == production_packets.len()
        && mcp_query_results.len() == production_packets.len()
        && mcp_query_results.iter().all(|result| {
            result.get("file").and_then(Value::as_str) == Some(&row.primary_source_path)
                && result.get("language").and_then(Value::as_str) == Some(&row.language_id)
                && result.get("source_role").and_then(Value::as_str) == Some("production")
        });
    let mcp_open_packet_body_included = mcp_open_json
        .get("packet_body_included")
        .and_then(Value::as_bool)
        == Some(true)
        && mcp_open_json
            .pointer("/packet/packet_body")
            .is_some_and(|body| !body.is_null());
    let mcp_open_ordered_steps_absent_by_default = mcp_open_json
        .get("ordered_steps_included")
        .and_then(Value::as_bool)
        == Some(false)
        && mcp_open_json.pointer("/packet/ordered_steps").is_none()
        && mcp_open_json
            .pointer("/packet/ordered_steps_inline")
            .and_then(Value::as_bool)
            == Some(false);
    let mcp_open_packet_id_matches_descriptor =
        mcp_open_response_matches_descriptor(&mcp_open_json, mcp_expansion);
    let trace_root_external =
        path_is_within(&trace_root, &options.run_root) && !path_is_within(&trace_root, &repo_root);
    let repo_local_trace_and_target_absent = !repo_root.join("target").exists();
    let product_surfaces = Mvp4ProductSurfaceEvidence {
        cli_status_packet_status,
        cli_status_packet_rows,
        cli_status_matches_persistence: cli_status_packet_rows == production_packets.len(),
        doctor_packet_status,
        doctor_packet_rows,
        doctor_matches_persistence: doctor_packet_rows == production_packets.len(),
        cli_filtered_query_count: query_result_count,
        cli_filtered_query_exact,
        cli_packet_id: packet_id,
        cli_context_handle_id: cli_context_handle
            .and_then(|handle| handle.get("handle_id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        cli_context_handle_matches_query: cli_context_handle.is_some(),
        cli_packet_body_resolved,
        validate_edit_status,
        validate_edit_packet_delta_safe,
        validate_edit_packet_integrity_status,
        validate_edit_packet_integrity_blocking_count,
        validate_edit_blocking_integrity_absent,
        mcp_initialize_succeeded,
        mcp_status_packet_status,
        mcp_status_packet_rows,
        mcp_status_matches_persistence: mcp_status_packet_rows == production_packets.len(),
        mcp_filtered_query_count,
        mcp_filtered_query_exact,
        mcp_selected_packet_id,
        mcp_query_context_intersection_found: mcp_selection.is_some(),
        mcp_context_handle_id: mcp_context_handle
            .and_then(|handle| handle.get("expansion_handle"))
            .and_then(Value::as_str)
            .map(str::to_string),
        mcp_context_handle_matches_query: mcp_context_handle.is_some(),
        mcp_expansion_tool,
        mcp_expansion_descriptor_concrete,
        mcp_open_packet_body_included,
        mcp_open_ordered_steps_absent_by_default,
        mcp_open_packet_id_matches_descriptor,
        external_db_path: profile_db.clone(),
        external_trace_root: Some(path_string(&trace_root)),
        trace_root_external,
        repo_local_trace_and_target_absent,
        all_cli_mcp_outputs_phantom_free,
    };
    let observed = ObservedLanguageEvidence {
        support_tier: catalog.support_tier.clone(),
        compatibility_tier: catalog.compatibility_tier,
        capability_statuses: catalog.capabilities.clone(),
        capability_evidence: manifest
            .shared_requirements
            .capabilities
            .iter()
            .filter_map(|requirement| {
                select_capability_evidence(catalog, requirement)
                    .map(|evidence| (requirement.flag.clone(), evidence))
            })
            .collect(),
        catalog_project_resolver_status: catalog.project_resolver_status.clone(),
        project_resolver_status: "not_applicable".to_string(),
        project_resolver_scope: SAME_FILE_INTRAPROCEDURAL_SCOPE.to_string(),
        micro_node_kinds,
        micro_edge_kinds,
        persisted_packet_count: production_packets.len(),
        packet_contract_satisfied,
        query_result_count,
        query_language_matches,
        profile_db: profile_db,
        profile_external,
        lifecycle_claimable,
        repo_local_dot_codegraph_absent,
        dynamic_boundary_negative_clean,
        product_surfaces,
        production_extraction_executed: true,
    };
    Ok(assess_language_row(manifest, row, observed, commands))
}

fn assess_language_row(
    manifest: &Mvp4LanguageReadinessManifest,
    row: &Mvp4LanguageContractRow,
    observed: ObservedLanguageEvidence,
    commands: Vec<Mvp4CommandEvidence>,
) -> Mvp4LanguageReadinessResult {
    let mut unmet_reasons = Vec::new();
    if observed.compatibility_tier != REQUIRED_COMPATIBILITY_TIER {
        unmet_reasons.push(format!(
            "compatibility_tier:{}:requires_{}",
            observed.compatibility_tier, REQUIRED_COMPATIBILITY_TIER
        ));
    }
    for requirement in &manifest.shared_requirements.capabilities {
        match observed.capability_evidence.get(&requirement.flag) {
            Some(evidence)
                if requirement
                    .accepted_statuses
                    .iter()
                    .any(|accepted| accepted == &evidence.status)
                    && !evidence.scope.trim().is_empty()
                    && !evidence.proof_boundary.trim().is_empty() => {}
            _ => {
                let broad_status = observed
                    .capability_statuses
                    .get(&requirement.flag)
                    .map(String::as_str)
                    .unwrap_or("missing");
                unmet_reasons.push(format!(
                    "capability:{}:{}:no_accepted_scoped_evidence",
                    requirement.flag, broad_status
                ));
            }
        }
    }
    if !row
        .accepted_project_resolver_statuses
        .iter()
        .any(|accepted| accepted == &observed.project_resolver_status)
    {
        unmet_reasons.push(format!(
            "project_resolver:{}",
            if observed.project_resolver_status.is_empty() {
                "missing"
            } else {
                &observed.project_resolver_status
            }
        ));
    }
    for required in &manifest.shared_requirements.micro_node_kinds {
        if !observed.micro_node_kinds.contains(required) {
            unmet_reasons.push(format!("missing_micro_node:{required}"));
        }
    }
    for required in &manifest.shared_requirements.micro_edge_kinds {
        if !observed.micro_edge_kinds.contains(required) {
            unmet_reasons.push(format!("missing_micro_edge:{required}"));
        }
    }
    if !observed.packet_contract_satisfied {
        unmet_reasons.push("packet_contract_unmet".to_string());
    }
    if observed.query_result_count == 0 {
        unmet_reasons.push("production_query_returned_no_packets".to_string());
    }
    if observed.query_result_count != observed.persisted_packet_count {
        unmet_reasons.push(format!(
            "query_store_packet_count_mismatch:{}:{}",
            observed.query_result_count, observed.persisted_packet_count
        ));
    }
    if !observed.query_language_matches {
        unmet_reasons.push("production_query_language_mismatch".to_string());
    }
    if !observed.lifecycle_claimable {
        unmet_reasons.push("production_profile_nonclaimable".to_string());
    }
    if !observed.profile_external {
        unmet_reasons.push("profile_not_external".to_string());
    }
    if !observed.repo_local_dot_codegraph_absent {
        unmet_reasons.push("repo_local_dot_codegraph_created".to_string());
    }
    if !observed.dynamic_boundary_negative_clean {
        unmet_reasons.push("dynamic_boundary_negative_overclaimed".to_string());
    }
    let surfaces = &observed.product_surfaces;
    if surfaces.cli_status_packet_status != "ready"
        || !surfaces.cli_status_matches_persistence
        || surfaces.cli_status_packet_rows != observed.persisted_packet_count
    {
        unmet_reasons.push("cli_status_packet_surface_mismatch".to_string());
    }
    if surfaces.doctor_packet_status != "ready"
        || !surfaces.doctor_matches_persistence
        || surfaces.doctor_packet_rows != observed.persisted_packet_count
    {
        unmet_reasons.push("doctor_packet_surface_mismatch".to_string());
    }
    if surfaces.cli_filtered_query_count != observed.persisted_packet_count
        || !surfaces.cli_filtered_query_exact
    {
        unmet_reasons.push("cli_filtered_packet_query_mismatch".to_string());
    }
    if surfaces.cli_packet_id.is_none() {
        unmet_reasons.push("cli_packet_id_missing".to_string());
    }
    if surfaces.cli_context_handle_id.is_none() || !surfaces.cli_context_handle_matches_query {
        unmet_reasons.push("cli_context_packet_handle_unresolved".to_string());
    }
    if !surfaces.cli_packet_body_resolved {
        unmet_reasons.push("cli_packet_body_open_failed".to_string());
    }
    if surfaces.validate_edit_status.is_empty()
        || matches!(
            surfaces.validate_edit_status.as_str(),
            "missing" | "error" | "runner_error"
        )
    {
        unmet_reasons.push("validate_edit_non_error_status_unmet".to_string());
    }
    if !surfaces.validate_edit_packet_delta_safe {
        unmet_reasons.push("validate_edit_packet_delta_not_safe".to_string());
    }
    if surfaces.validate_edit_packet_integrity_status == "blocking" {
        unmet_reasons.push("validate_edit_blocking_packet_integrity_present".to_string());
    } else if surfaces.validate_edit_packet_integrity_status != "clear"
        || surfaces.validate_edit_packet_integrity_blocking_count != Some(0)
        || !surfaces.validate_edit_blocking_integrity_absent
    {
        unmet_reasons.push("validate_edit_packet_integrity_unavailable".to_string());
    }
    if !surfaces.mcp_initialize_succeeded {
        unmet_reasons.push("mcp_initialize_failed".to_string());
    }
    if surfaces.mcp_status_packet_status != "ready"
        || !surfaces.mcp_status_matches_persistence
        || surfaces.mcp_status_packet_rows != observed.persisted_packet_count
    {
        unmet_reasons.push("mcp_status_packet_surface_mismatch".to_string());
    }
    if surfaces.mcp_filtered_query_count != observed.persisted_packet_count
        || !surfaces.mcp_filtered_query_exact
    {
        unmet_reasons.push("mcp_filtered_packet_query_mismatch".to_string());
    }
    if !surfaces.mcp_query_context_intersection_found || surfaces.mcp_selected_packet_id.is_none() {
        unmet_reasons.push("mcp_query_context_packet_intersection_missing".to_string());
    } else {
        if surfaces.mcp_context_handle_id.is_none() || !surfaces.mcp_context_handle_matches_query {
            unmet_reasons.push("mcp_context_packet_handle_unresolved".to_string());
        }
        if surfaces.mcp_expansion_tool.as_deref() != Some("codegraph.open_local_flow_packet")
            || !surfaces.mcp_expansion_descriptor_concrete
        {
            unmet_reasons.push("mcp_packet_open_descriptor_not_concrete".to_string());
        } else {
            if !surfaces.mcp_open_packet_body_included {
                unmet_reasons.push("mcp_packet_body_open_failed".to_string());
            }
            if !surfaces.mcp_open_ordered_steps_absent_by_default {
                unmet_reasons.push("mcp_packet_open_default_steps_not_bounded".to_string());
            }
            if !surfaces.mcp_open_packet_id_matches_descriptor {
                unmet_reasons.push("mcp_packet_open_identity_mismatch".to_string());
            }
        }
    }
    if surfaces.external_db_path.is_none() {
        unmet_reasons.push("external_db_path_missing".to_string());
    }
    if surfaces.external_trace_root.is_none() || !surfaces.trace_root_external {
        unmet_reasons.push("external_trace_root_unmet".to_string());
    }
    if !surfaces.repo_local_trace_and_target_absent {
        unmet_reasons.push("repo_local_trace_or_target_created".to_string());
    }
    if !surfaces.all_cli_mcp_outputs_phantom_free {
        unmet_reasons.push("product_output_dynamic_negative_overclaimed".to_string());
    }
    if !observed.production_extraction_executed {
        unmet_reasons.push("production_extraction_not_executed".to_string());
    }
    let safety_failure = !observed.profile_external
        || !observed.repo_local_dot_codegraph_absent
        || !surfaces.trace_root_external
        || !surfaces.repo_local_trace_and_target_absent;
    let verdict = if safety_failure {
        Mvp4LanguageReadinessVerdict::RunnerError
    } else if unmet_reasons.is_empty() {
        Mvp4LanguageReadinessVerdict::Ready
    } else {
        Mvp4LanguageReadinessVerdict::Unmet
    };
    Mvp4LanguageReadinessResult {
        language_id: row.language_id.clone(),
        display_name: row.display_name.clone(),
        verdict,
        historical_baseline_status: row.historical_baseline_status.clone(),
        historical_baseline_unmet_reason: row.historical_baseline_unmet_reason.clone(),
        production_extraction_executed: observed.production_extraction_executed,
        observed_support_tier: observed.support_tier,
        observed_compatibility_tier: observed.compatibility_tier,
        capability_statuses: observed.capability_statuses,
        capability_evidence: observed.capability_evidence,
        catalog_project_resolver_status: observed.catalog_project_resolver_status,
        project_resolver_status: observed.project_resolver_status,
        project_resolver_scope: observed.project_resolver_scope,
        observed_micro_node_kinds: observed.micro_node_kinds.into_iter().collect(),
        observed_micro_edge_kinds: observed.micro_edge_kinds.into_iter().collect(),
        persisted_packet_count: observed.persisted_packet_count,
        query_result_count: observed.query_result_count,
        profile_db: observed.profile_db,
        profile_external: observed.profile_external,
        lifecycle_claimable: observed.lifecycle_claimable,
        repo_local_dot_codegraph_absent: observed.repo_local_dot_codegraph_absent,
        dynamic_boundary_negative_clean: observed.dynamic_boundary_negative_clean,
        product_surfaces: observed.product_surfaces,
        unmet_reasons,
        commands,
    }
}

fn runner_error_result(
    row: &Mvp4LanguageContractRow,
    catalog: &LanguageCatalogRow,
    commands: Vec<Mvp4CommandEvidence>,
    reason: String,
) -> Mvp4LanguageReadinessResult {
    Mvp4LanguageReadinessResult {
        language_id: row.language_id.clone(),
        display_name: row.display_name.clone(),
        verdict: Mvp4LanguageReadinessVerdict::RunnerError,
        historical_baseline_status: row.historical_baseline_status.clone(),
        historical_baseline_unmet_reason: row.historical_baseline_unmet_reason.clone(),
        production_extraction_executed: commands
            .iter()
            .any(|command| command.step == "agent_use_index" && command.success),
        observed_support_tier: catalog.support_tier.clone(),
        observed_compatibility_tier: catalog.compatibility_tier,
        capability_statuses: catalog.capabilities.clone(),
        capability_evidence: BTreeMap::new(),
        catalog_project_resolver_status: catalog.project_resolver_status.clone(),
        project_resolver_status: "not_applicable".to_string(),
        project_resolver_scope: SAME_FILE_INTRAPROCEDURAL_SCOPE.to_string(),
        observed_micro_node_kinds: Vec::new(),
        observed_micro_edge_kinds: Vec::new(),
        persisted_packet_count: 0,
        query_result_count: 0,
        profile_db: None,
        profile_external: false,
        lifecycle_claimable: false,
        repo_local_dot_codegraph_absent: false,
        dynamic_boundary_negative_clean: false,
        product_surfaces: Mvp4ProductSurfaceEvidence {
            cli_status_packet_status: "runner_error".to_string(),
            doctor_packet_status: "runner_error".to_string(),
            validate_edit_status: "runner_error".to_string(),
            mcp_status_packet_status: "runner_error".to_string(),
            ..Mvp4ProductSurfaceEvidence::default()
        },
        unmet_reasons: vec![format!("runner_error:{reason}")],
        commands,
    }
}

fn validate_language_row(row: &Mvp4LanguageContractRow) -> BenchResult<()> {
    if row.language_id.trim().is_empty()
        || row.display_name.trim().is_empty()
        || row.primary_source_path.trim().is_empty()
        || row.supported_static_scope.trim().is_empty()
        || row.resolver_requirement.trim().is_empty()
    {
        return validation_error("language identity, scope, path, and resolver must not be empty");
    }
    if !row.required_for_mvp4_ready {
        return validation_error(format!(
            "{} must be required for MVP4 readiness",
            row.language_id
        ));
    }
    if row.historical_baseline_status != "unmet" {
        return validation_error(format!(
            "{} historical_baseline_status must remain unmet for the immutable pre-implementation snapshot",
            row.language_id
        ));
    }
    if row.historical_baseline_unmet_reason.trim().is_empty() {
        return validation_error(format!(
            "{} historical unmet baseline requires an explicit reason",
            row.language_id
        ));
    }
    if row.file_extensions.is_empty()
        || row.accepted_project_resolver_statuses.is_empty()
        || row.dynamic_boundaries.is_empty()
        || row.files.is_empty()
    {
        return validation_error(format!(
            "{} must declare extensions, accepted resolver statuses, dynamic boundaries, and fixture files",
            row.language_id
        ));
    }
    let allowed_resolver_statuses = BTreeSet::from(["exact", "candidate_set", "not_applicable"]);
    if row
        .accepted_project_resolver_statuses
        .iter()
        .any(|status| !allowed_resolver_statuses.contains(status.as_str()))
    {
        return validation_error(format!(
            "{} declares an optimistic or invalid resolver status; only exact, candidate_set, or explicitly scoped not_applicable are accepted",
            row.language_id
        ));
    }
    if !row
        .accepted_project_resolver_statuses
        .iter()
        .any(|status| status == "not_applicable")
        || !row
            .resolver_requirement
            .contains("same_file_intraprocedural")
    {
        return validation_error(format!(
            "{} must accept not_applicable for its explicit same_file_intraprocedural fixture scope",
            row.language_id
        ));
    }
    let mut paths = BTreeSet::new();
    for file in &row.files {
        validate_relative_path(&file.path, &row.language_id)?;
        if file.contents.trim().is_empty() || !paths.insert(file.path.as_str()) {
            return validation_error(format!(
                "{} fixture files must be non-empty and unique",
                row.language_id
            ));
        }
    }
    if !paths.contains(row.primary_source_path.as_str()) {
        return validation_error(format!(
            "{} primary source path is absent from files",
            row.language_id
        ));
    }
    let extension = Path::new(&row.primary_source_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !row.file_extensions.iter().any(|value| value == extension) {
        return validation_error(format!(
            "{} primary source extension {extension} is not declared",
            row.language_id
        ));
    }
    require_exact_strings(
        &format!("{} semantic fixture coverage", row.language_id),
        &detect_semantic_fixture_classes(row),
        &REQUIRED_SEMANTIC_FIXTURE_CLASSES,
    )?;
    Ok(())
}

fn detect_semantic_fixture_classes(row: &Mvp4LanguageContractRow) -> Vec<String> {
    let source = row
        .files
        .iter()
        .map(|file| file.contents.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if !source.contains(FIXTURE_SHAPE_MARKER)
        || !source.contains(SANITIZER_MARKER)
        || !source.contains(ASSERTION_MARKER)
        || !marker_precedes_helper(&source, SANITIZER_MARKER, "sanitize(")
        || !marker_precedes_helper(&source, ASSERTION_MARKER, "assert_ready(")
        || source.matches("sanitize(").count() < 2
        || source.matches("record(").count() < 2
        || source.matches("assert_ready(").count() < 2
        || [
            ".sanitize(",
            "->sanitize(",
            "::sanitize(",
            ".record(",
            "->record(",
            "::record(",
        ]
        .iter()
        .any(|qualified| source.contains(qualified))
    {
        return Vec::new();
    }
    REQUIRED_SEMANTIC_FIXTURE_CLASSES
        .iter()
        .filter(|class| source.contains(**class))
        .map(|class| class.to_string())
        .collect()
}

fn marker_precedes_helper(source: &str, marker: &str, helper: &str) -> bool {
    let lines = source.lines().collect::<Vec<_>>();
    lines.iter().enumerate().any(|(index, line)| {
        line.contains(marker)
            && lines
                .iter()
                .skip(index + 1)
                .find(|candidate| !candidate.trim().is_empty())
                .is_some_and(|candidate| candidate.contains(helper))
    })
}

fn exact_or_provenanced(exactness: &str, provenance_id: Option<&str>) -> bool {
    exactness == "exact"
        || (exactness == "derived_with_provenance"
            && provenance_id.is_some_and(|value| !value.trim().is_empty()))
}

fn micro_edge_contract_kind_from_storage(raw: &str) -> Option<String> {
    MicroEdgeKind::from_storage_str(raw).map(|kind| kind.as_str().to_ascii_uppercase())
}

fn claimability_is_claimable(raw: &str) -> bool {
    let raw = raw.trim();
    if raw == "claimable" || raw.starts_with("claimable_") {
        return true;
    }
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|value| value.get("claimable").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn local_flow_query_total_result_count(query_json: &Value) -> usize {
    let count = |key| {
        query_json
            .get(key)
            .and_then(Value::as_u64)
            .map(|value| usize::try_from(value).unwrap_or(usize::MAX))
            .unwrap_or_default()
    };
    count("result_count").saturating_add(count("result_omitted_count"))
}

fn selected_cli_packet_identity(query_json: &Value) -> Option<(String, String)> {
    query_json
        .get("results")
        .and_then(Value::as_array)?
        .iter()
        .find_map(|result| {
            let packet_id = result
                .get("packet_id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())?;
            let function_entity_id = result
                .get("function_entity_id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())?;
            Some((packet_id.to_string(), function_entity_id.to_string()))
        })
}

fn local_flow_filtered_query_is_exact(
    query_json: &Value,
    expected_total: usize,
    expected_file: &str,
    expected_language: &str,
    expected_source_role: &str,
) -> bool {
    let Some(results) = query_json.get("results").and_then(Value::as_array) else {
        return false;
    };
    let visible_count = query_json
        .get("result_count")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok());
    local_flow_query_total_result_count(query_json) == expected_total
        && visible_count == Some(results.len())
        && !results.is_empty()
        && query_json.pointer("/query/file").and_then(Value::as_str) == Some(expected_file)
        && query_json
            .pointer("/query/language")
            .and_then(Value::as_str)
            == Some(expected_language)
        && query_json
            .pointer("/query/source_role")
            .and_then(Value::as_str)
            == Some(expected_source_role)
        && results.iter().all(|result| {
            result.get("file").and_then(Value::as_str) == Some(expected_file)
                && result.get("language").and_then(Value::as_str) == Some(expected_language)
                && result.get("source_role").and_then(Value::as_str) == Some(expected_source_role)
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidateEditPacketIntegrity {
    Clear,
    Blocking,
    Unavailable,
}

impl ValidateEditPacketIntegrity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Blocking => "blocking",
            Self::Unavailable => "unavailable",
        }
    }
}

fn validate_edit_packet_integrity_evidence(
    validate_edit_json: &Value,
) -> (ValidateEditPacketIntegrity, Option<usize>) {
    let blocking_count = validate_edit_json
        .pointer("/micro_flow_packet_integrity/blocking_count")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok());
    match blocking_count {
        Some(0) => (ValidateEditPacketIntegrity::Clear, Some(0)),
        Some(count) => (ValidateEditPacketIntegrity::Blocking, Some(count)),
        None => (ValidateEditPacketIntegrity::Unavailable, None),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct McpPacketSelection {
    packet_id: String,
    context_handle: Value,
}

fn select_mcp_query_context_packet(
    query_json: &Value,
    context_json: &Value,
) -> Option<McpPacketSelection> {
    let query_results = query_json.get("results").and_then(Value::as_array)?;
    let context_handles = context_json
        .get("local_flow_packet_handles")
        .and_then(Value::as_array)?;
    query_results.iter().find_map(|result| {
        let packet_id = result
            .get("packet_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())?;
        let context_handle = context_handles
            .iter()
            .find(|handle| handle.get("packet_id").and_then(Value::as_str) == Some(packet_id))?;
        Some(McpPacketSelection {
            packet_id: packet_id.to_string(),
            context_handle: context_handle.clone(),
        })
    })
}

fn existing_paths_equivalent(left: &Path, right: &Path) -> bool {
    let (Ok(left), Ok(right)) = (fs::canonicalize(left), fs::canonicalize(right)) else {
        return false;
    };
    #[cfg(windows)]
    {
        windows_path_identity(&left) == windows_path_identity(&right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

#[cfg(windows)]
fn windows_path_identity(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('/', "\\");
    let normalized = if let Some(rest) = normalized.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else if let Some(rest) = normalized.strip_prefix("\\\\?\\") {
        rest.to_string()
    } else {
        normalized
    };
    normalized.to_ascii_lowercase()
}

fn mcp_expansion_descriptor_is_concrete(
    expansion: &Value,
    expected_repo: &Path,
    expected_db_path: &Path,
    expected_packet_id: &str,
) -> bool {
    let Some(arguments) = expansion.get("arguments") else {
        return false;
    };
    let (Some(repo), Some(db_path)) = (
        arguments.get("repo").and_then(Value::as_str),
        arguments.get("db_path").and_then(Value::as_str),
    ) else {
        return false;
    };
    expansion.get("tool").and_then(Value::as_str) == Some("codegraph.open_local_flow_packet")
        && existing_paths_equivalent(Path::new(repo), expected_repo)
        && existing_paths_equivalent(Path::new(db_path), expected_db_path)
        && arguments.get("packet_id").and_then(Value::as_str) == Some(expected_packet_id)
        && arguments
            .get("include_ordered_steps")
            .and_then(Value::as_bool)
            == Some(false)
}

fn mcp_open_response_matches_descriptor(open_json: &Value, expansion: Option<&Value>) -> bool {
    let descriptor_packet_id = expansion
        .and_then(|value| value.pointer("/arguments/packet_id"))
        .and_then(Value::as_str);
    descriptor_packet_id.is_some()
        && open_json
            .pointer("/packet/packet_id")
            .and_then(Value::as_str)
            == descriptor_packet_id
}

fn validate_runner_options(options: &Mvp4LanguageReadinessRunnerOptions) -> BenchResult<()> {
    if !options.release_binary.is_file() {
        return validation_error(format!(
            "production release binary is missing: {}",
            options.release_binary.display()
        ));
    }
    if !options.manifest_path.is_file() {
        return validation_error(format!(
            "language readiness manifest is missing: {}",
            options.manifest_path.display()
        ));
    }
    if options.command_timeout.is_zero() {
        return validation_error("command timeout must be positive");
    }
    if path_is_within(&options.run_root, &workspace_root()) {
        return validation_error(format!(
            "run root must be outside the CodeGraph workspace: {}",
            options.run_root.display()
        ));
    }
    Ok(())
}

fn prepare_empty_run_root(path: &Path) -> BenchResult<()> {
    if path.exists() {
        let mut entries = fs::read_dir(path)?;
        if entries.next().transpose()?.is_some() {
            return validation_error(format!(
                "run root must be new or empty; refusing to overwrite {}",
                path.display()
            ));
        }
    }
    fs::create_dir_all(path)?;
    Ok(())
}

fn run_plain_stage(
    spec: CommandSpec,
    logs_dir: &Path,
    timeout: Duration,
    commands: &mut Vec<Mvp4CommandEvidence>,
) -> Result<(), String> {
    let run = run_recorded_command(spec, logs_dir, timeout).map_err(|error| error.to_string())?;
    let success = run.evidence.success;
    let step = run.evidence.step.clone();
    commands.push(run.evidence);
    if success {
        Ok(())
    } else {
        Err(format!("{step} failed; inspect command logs"))
    }
}

fn run_json_stage(
    spec: CommandSpec,
    logs_dir: &Path,
    timeout: Duration,
    commands: &mut Vec<Mvp4CommandEvidence>,
) -> Result<Value, String> {
    let run = run_recorded_command(spec, logs_dir, timeout).map_err(|error| error.to_string())?;
    let success = run.evidence.success;
    let step = run.evidence.step.clone();
    commands.push(run.evidence);
    if !success {
        return Err(format!("{step} failed; inspect command logs"));
    }
    serde_json::from_str(&run.stdout)
        .map_err(|error| format!("{step} stdout was not JSON: {error}"))
}

fn run_json_lines_stage(
    spec: CommandSpec,
    logs_dir: &Path,
    timeout: Duration,
    commands: &mut Vec<Mvp4CommandEvidence>,
) -> Result<Vec<Value>, String> {
    let run = run_recorded_command(spec, logs_dir, timeout).map_err(|error| error.to_string())?;
    let success = run.evidence.success;
    let step = run.evidence.step.clone();
    commands.push(run.evidence);
    if !success {
        return Err(format!("{step} failed; inspect command logs"));
    }
    let responses = run
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .map_err(|error| format!("{step} stdout contained invalid JSON: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if responses.is_empty() {
        return Err(format!("{step} produced no JSON responses"));
    }
    Ok(responses)
}

fn mcp_session_payload(requests: &[Value]) -> Result<String, String> {
    let mut payload = String::new();
    for request in requests {
        payload.push_str(
            &serde_json::to_string(request)
                .map_err(|error| format!("encode MCP JSON-RPC request: {error}"))?,
        );
        payload.push('\n');
    }
    validate_stdin_payload(&payload).map_err(|error| error.to_string())?;
    Ok(payload)
}

fn mcp_response_for_id<'a>(responses: &'a [Value], id: u64) -> Result<&'a Value, String> {
    responses
        .iter()
        .find(|response| response.get("id").and_then(Value::as_u64) == Some(id))
        .ok_or_else(|| format!("MCP session did not return JSON-RPC id {id}"))
}

fn mcp_structured_content(response: &Value) -> Result<&Value, String> {
    if response.get("error").is_some() {
        return Err(format!("MCP JSON-RPC error: {}", response["error"]));
    }
    let result = response
        .get("result")
        .ok_or_else(|| "MCP tool response did not contain result".to_string())?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(format!("MCP tool returned isError=true: {result}"));
    }
    result
        .get("structuredContent")
        .ok_or_else(|| "MCP tool response did not contain structuredContent".to_string())
}

fn command_logs_are_phantom_free(commands: &[Mvp4CommandEvidence]) -> bool {
    commands.iter().all(|command| {
        [&command.stdout_log, &command.stderr_log]
            .into_iter()
            .all(|path| {
                fs::read_to_string(path)
                    .is_ok_and(|contents| !contents.contains("phantom_dynamic_target"))
            })
    })
}

fn run_recorded_command(
    spec: CommandSpec,
    logs_dir: &Path,
    timeout: Duration,
) -> BenchResult<CommandRun> {
    fs::create_dir_all(logs_dir)?;
    let stdin_bytes = spec.stdin.as_ref().map_or(0, |payload| payload.len());
    let stdin_audit_log = if let Some(payload) = spec.stdin.as_deref() {
        validate_stdin_payload(payload)?;
        if path_is_within(logs_dir, &spec.cwd) {
            return validation_error(format!(
                "stdin audit logs for {} must stay outside the disposable repository",
                spec.step
            ));
        }
        let path = logs_dir.join(format!("{}.stdin.log", spec.step));
        fs::write(&path, payload)?;
        Some(path)
    } else {
        None
    };
    let stdout_log = logs_dir.join(format!("{}.stdout.log", spec.step));
    let stderr_log = logs_dir.join(format!("{}.stderr.log", spec.step));
    let stdout_file = File::create(&stdout_log)?;
    let stderr_file = File::create(&stderr_log)?;
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .envs(&spec.environment)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    if spec.stdin.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let started = Instant::now();
    let mut child = command.spawn().map_err(|error| {
        BenchmarkError::Io(format!(
            "spawn {} for {}: {error}",
            spec.program.display(),
            spec.step
        ))
    })?;
    if let Some(payload) = spec.stdin.as_deref() {
        let write_result = child
            .stdin
            .take()
            .ok_or_else(|| BenchmarkError::Io(format!("stdin pipe unavailable for {}", spec.step)))?
            .write_all(payload.as_bytes());
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BenchmarkError::Io(format!(
                "write bounded stdin for {}: {error}",
                spec.step
            )));
        }
    }
    let (status, timed_out) = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| BenchmarkError::Io(format!("wait for {}: {error}", spec.step)))?
        {
            break (status, false);
        }
        if started.elapsed() >= timeout {
            child.kill().map_err(|error| {
                BenchmarkError::Io(format!("kill timed-out {}: {error}", spec.step))
            })?;
            let status = child.wait().map_err(|error| {
                BenchmarkError::Io(format!("reap timed-out {}: {error}", spec.step))
            })?;
            break (status, true);
        }
        thread::sleep(Duration::from_millis(20));
    };
    let stdout = fs::read_to_string(&stdout_log)?;
    let evidence = Mvp4CommandEvidence {
        step: spec.step,
        program: path_string(&spec.program),
        args: auditable_args(&spec.args),
        cwd: path_string(&spec.cwd),
        environment: auditable_environment(&spec.environment),
        stdout_log: path_string(&stdout_log),
        stderr_log: path_string(&stderr_log),
        stdin_supplied: spec.stdin.is_some(),
        stdin_bytes,
        stdin_audit_log: stdin_audit_log.as_deref().map(path_string),
        exit_code: status.code(),
        timed_out,
        success: status.success() && !timed_out,
        duration_ms: started.elapsed().as_millis(),
    };
    Ok(CommandRun { evidence, stdout })
}

fn validate_stdin_payload(payload: &str) -> BenchResult<()> {
    if payload.len() > MAX_COMMAND_STDIN_BYTES {
        return validation_error(format!(
            "command stdin exceeds bounded audit limit of {MAX_COMMAND_STDIN_BYTES} bytes"
        ));
    }
    for line in payload.lines().filter(|line| !line.trim().is_empty()) {
        let value = serde_json::from_str::<Value>(line).map_err(|error| {
            BenchmarkError::Validation(format!(
                "audited command stdin must be newline-delimited JSON: {error}"
            ))
        })?;
        if contains_sensitive_json_key(&value) {
            return validation_error(
                "audited command stdin contains a secret-bearing field and was not written",
            );
        }
    }
    Ok(())
}

fn contains_sensitive_json_key(value: &Value) -> bool {
    match value {
        Value::Object(object) => object
            .iter()
            .any(|(key, child)| sensitive_name(key) || contains_sensitive_json_key(child)),
        Value::Array(values) => values.iter().any(contains_sensitive_json_key),
        _ => false,
    }
}

fn sensitive_name(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase().replace('-', "_");
    [
        "authorization",
        "cookie",
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "private_key",
    ]
    .iter()
    .any(|sensitive| normalized == *sensitive || normalized.ends_with(&format!("_{sensitive}")))
}

fn auditable_environment(environment: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    environment
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                if sensitive_name(key) {
                    "<redacted>".to_string()
                } else {
                    value.clone()
                },
            )
        })
        .collect()
}

fn auditable_args(args: &[String]) -> Vec<String> {
    let mut redact_next = false;
    args.iter()
        .map(|arg| {
            if redact_next {
                redact_next = false;
                return "<redacted>".to_string();
            }
            if arg.starts_with("--") && sensitive_name(arg.trim_start_matches('-')) {
                redact_next = true;
            }
            arg.clone()
        })
        .collect()
}

fn parse_language_catalog(value: &Value) -> BenchResult<BTreeMap<String, LanguageCatalogRow>> {
    let frontends = value
        .get("frontends")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            BenchmarkError::Parse("languages JSON has no frontends array".to_string())
        })?;
    let mut catalog = BTreeMap::new();
    let mut observed_ids = BTreeSet::new();
    for frontend in frontends {
        let Some(language_id) = frontend.get("language_id").and_then(Value::as_str) else {
            return validation_error("languages JSON contains a frontend without language_id");
        };
        if !observed_ids.insert(language_id.to_string()) {
            return validation_error(format!(
                "languages JSON contains duplicate frontend {language_id}"
            ));
        }
        let capabilities = frontend
            .get("capabilities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|capability| {
                Some((
                    capability.get("flag")?.as_str()?.to_string(),
                    capability.get("status")?.as_str()?.to_string(),
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let capability_scopes = frontend
            .get("capabilities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|capability| {
                Some((
                    capability.get("flag")?.as_str()?.to_string(),
                    capability.get("scope")?.as_str()?.to_string(),
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let mut scoped_readiness = BTreeMap::<String, Vec<CatalogScopedReadiness>>::new();
        for readiness in frontend
            .get("scoped_readiness")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(flag), Some(status), Some(scope), Some(proof_boundary)) = (
                readiness.get("flag").and_then(Value::as_str),
                readiness.get("status").and_then(Value::as_str),
                readiness.get("scope").and_then(Value::as_str),
                readiness.get("proof_boundary").and_then(Value::as_str),
            ) else {
                continue;
            };
            scoped_readiness
                .entry(flag.to_string())
                .or_default()
                .push(CatalogScopedReadiness {
                    status: status.to_string(),
                    scope: scope.to_string(),
                    proof_boundary: proof_boundary.to_string(),
                });
        }
        let support_tier = frontend
            .get("support_tier")
            .and_then(Value::as_str)
            .unwrap_or("missing")
            .to_string();
        let project_resolver_status = frontend
            .get("project_resolver_status")
            .and_then(Value::as_str)
            .unwrap_or("missing")
            .to_string();
        catalog.insert(
            language_id.to_string(),
            LanguageCatalogRow {
                compatibility_tier: support_tier_number(&support_tier),
                support_tier,
                capabilities,
                capability_scopes,
                scoped_readiness,
                project_resolver_status,
            },
        );
    }
    let ids = catalog.keys().cloned().collect::<Vec<_>>();
    require_exact_strings("release language catalog", &ids, &REQUIRED_LANGUAGE_IDS)?;
    if catalog.len() != REQUIRED_LANGUAGE_IDS.len()
        || frontends.len() != REQUIRED_LANGUAGE_IDS.len()
    {
        return validation_error(format!(
            "release language catalog must contain exactly {} unique frontends",
            REQUIRED_LANGUAGE_IDS.len()
        ));
    }
    Ok(catalog)
}

fn support_tier_number(value: &str) -> u8 {
    match value {
        "tier0_file_discovery" => 0,
        "tier1_syntax_entities" => 1,
        "tier2_imports_exports_packages" => 2,
        "tier3_calls_caller_callee" => 3,
        "tier4_compiler_or_lsp_verified" => 4,
        "tier5_dataflow_security_test_impact" => 5,
        _ => 0,
    }
}

fn select_capability_evidence(
    catalog: &LanguageCatalogRow,
    requirement: &Mvp4CapabilityRequirement,
) -> Option<Mvp4CapabilityEvidence> {
    if SCOPED_CAPABILITY_FLAGS.contains(&requirement.flag.as_str()) {
        return catalog
            .scoped_readiness
            .get(&requirement.flag)?
            .iter()
            .find(|readiness| {
                readiness.scope == SAME_FILE_INTRAPROCEDURAL_SCOPE
                    && requirement
                        .accepted_statuses
                        .iter()
                        .any(|accepted| accepted == &readiness.status)
                    && !readiness.proof_boundary.trim().is_empty()
            })
            .map(|readiness| Mvp4CapabilityEvidence {
                status: readiness.status.clone(),
                scope: readiness.scope.clone(),
                proof_boundary: readiness.proof_boundary.clone(),
                source: "scoped_readiness".to_string(),
            });
    }

    let status = catalog.capabilities.get(&requirement.flag)?;
    if !requirement
        .accepted_statuses
        .iter()
        .any(|accepted| accepted == status)
    {
        return None;
    }
    Some(Mvp4CapabilityEvidence {
        status: status.clone(),
        scope: catalog
            .capability_scopes
            .get(&requirement.flag)
            .cloned()
            .unwrap_or_else(|| "language_frontend".to_string()),
        proof_boundary:
            "broad language frontend capability metadata; persisted runtime facts remain required"
                .to_string(),
        source: "capabilities".to_string(),
    })
}

fn find_string_for_keys(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(value) = object.get(*key).and_then(Value::as_str) {
                    return Some(value.to_string());
                }
            }
            object
                .values()
                .find_map(|child| find_string_for_keys(child, keys))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|child| find_string_for_keys(child, keys)),
        _ => None,
    }
}

fn require_exact_strings<const N: usize>(
    label: &str,
    actual: &[String],
    required: &[&str; N],
) -> BenchResult<()> {
    let actual = actual.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let required = required.iter().copied().collect::<BTreeSet<_>>();
    if actual != required {
        return validation_error(format!(
            "{label} must be exactly {:?}; observed {:?}",
            required, actual
        ));
    }
    Ok(())
}

fn validate_relative_path(path: &str, language_id: &str) -> BenchResult<()> {
    let value = Path::new(path);
    if value.is_absolute()
        || value.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return validation_error(format!(
            "{language_id} fixture path must be repository-relative: {path}"
        ));
    }
    Ok(())
}

fn profile_is_external(db_path: &Path, profile_root: &Path, repo_root: &Path) -> bool {
    path_is_within(db_path, profile_root) && !path_is_within(db_path, repo_root)
}

fn path_is_within(child: &Path, parent: &Path) -> bool {
    normalized_absolute(child).starts_with(normalized_absolute(parent))
}

fn normalized_absolute(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn unique_suffix() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{}-{millis}", std::process::id())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn validation_error<T>(message: impl Into<String>) -> BenchResult<T> {
    Err(BenchmarkError::Validation(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_path() -> PathBuf {
        workspace_root()
            .join("fixtures")
            .join("mvp4_micro_flow_oracles")
            .join(MVP4_LANGUAGE_READINESS_MANIFEST_FILE)
    }

    fn ready_product_surface_evidence(packet_count: usize) -> Mvp4ProductSurfaceEvidence {
        Mvp4ProductSurfaceEvidence {
            cli_status_packet_status: "ready".to_string(),
            cli_status_packet_rows: packet_count,
            cli_status_matches_persistence: true,
            doctor_packet_status: "ready".to_string(),
            doctor_packet_rows: packet_count,
            doctor_matches_persistence: true,
            cli_filtered_query_count: packet_count,
            cli_filtered_query_exact: true,
            cli_packet_id: Some("packet-1".to_string()),
            cli_context_handle_id: Some("micro-flow-handle:packet-1".to_string()),
            cli_context_handle_matches_query: true,
            cli_packet_body_resolved: true,
            validate_edit_status: "ok".to_string(),
            validate_edit_packet_delta_safe: true,
            validate_edit_packet_integrity_status: "clear".to_string(),
            validate_edit_packet_integrity_blocking_count: Some(0),
            validate_edit_blocking_integrity_absent: true,
            mcp_initialize_succeeded: true,
            mcp_status_packet_status: "ready".to_string(),
            mcp_status_packet_rows: packet_count,
            mcp_status_matches_persistence: true,
            mcp_filtered_query_count: packet_count,
            mcp_filtered_query_exact: true,
            mcp_selected_packet_id: Some("packet-1".to_string()),
            mcp_query_context_intersection_found: true,
            mcp_context_handle_id: Some("audit.local_flow_packets.packet:packet-1".to_string()),
            mcp_context_handle_matches_query: true,
            mcp_expansion_tool: Some("codegraph.open_local_flow_packet".to_string()),
            mcp_expansion_descriptor_concrete: true,
            mcp_open_packet_body_included: true,
            mcp_open_ordered_steps_absent_by_default: true,
            mcp_open_packet_id_matches_descriptor: true,
            external_db_path: Some("external/profile.sqlite".to_string()),
            external_trace_root: Some("external/traces".to_string()),
            trace_root_external: true,
            repo_local_trace_and_target_absent: true,
            all_cli_mcp_outputs_phantom_free: true,
        }
    }

    #[test]
    fn language_manifest_has_exactly_thirteen_required_rows() {
        let manifest = load_mvp4_language_readiness_manifest(&manifest_path()).expect("manifest");
        assert_eq!(manifest.languages.len(), 13);
        assert_eq!(manifest.contract_id, MVP4_LANGUAGE_READINESS_CONTRACT_ID);
        assert!(manifest.production_extraction_required_by_current_tests);
        assert!(!manifest.manifest_validation_is_readiness);
        assert!(manifest.release_runner.executes_now);
        assert_eq!(
            manifest.historical_baseline.snapshot_schema_version,
            HISTORICAL_BASELINE_SNAPSHOT_SCHEMA_VERSION
        );
        assert_eq!(
            manifest.historical_baseline.snapshot_id,
            HISTORICAL_BASELINE_SNAPSHOT_ID
        );
        assert!(manifest.historical_baseline.immutable);
        assert!(manifest.historical_baseline.captured_before_implementation);
        assert!(
            !manifest
                .historical_baseline
                .authoritative_for_current_readiness
        );
        assert_eq!(
            manifest.historical_baseline.current_readiness_authority,
            CURRENT_READINESS_AUTHORITY
        );
        assert!(manifest
            .languages
            .iter()
            .all(|row| row.required_for_mvp4_ready));
        assert!(manifest
            .languages
            .iter()
            .all(|row| row.historical_baseline_status == "unmet"
                && !row.historical_baseline_unmet_reason.is_empty()));

        let python = manifest
            .languages
            .iter()
            .find(|row| row.language_id == "python")
            .expect("python row");
        assert_eq!(python.file_extensions, vec!["py"]);
    }

    #[test]
    fn typescript_readiness_row_is_explicitly_mts_representative() {
        let manifest = load_mvp4_language_readiness_manifest(&manifest_path()).expect("manifest");
        let row = manifest
            .languages
            .iter()
            .find(|row| row.language_id == "typescript")
            .expect("typescript row");
        assert_eq!(
            row.file_extensions
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["ts", "mts", "cts"]
        );
        assert_eq!(row.primary_source_path, "src/readiness.mts");
        assert_eq!(
            row.files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["src/readiness.mts"]
        );
        for boundary in [
            ".mts/.cts ParserFactsV1",
            "ordinary .ts",
            "bounded legacy v1",
            ".d.ts remains inactive",
        ] {
            assert!(row.supported_static_scope.contains(boundary), "{boundary}");
        }
        assert!(row
            .historical_baseline_unmet_reason
            .contains("was bound to ordinary .ts legacy v1"));
    }

    #[test]
    fn historical_baseline_can_never_be_current_readiness_authority() {
        let mut manifest =
            load_mvp4_language_readiness_manifest(&manifest_path()).expect("manifest");
        manifest
            .historical_baseline
            .authoritative_for_current_readiness = true;
        let error = validate_mvp4_language_readiness_manifest(&manifest)
            .expect_err("historical snapshot cannot become readiness truth");
        assert!(error
            .to_string()
            .contains("historical baseline must be the immutable"));
    }

    #[test]
    fn manifest_validation_can_never_claim_readiness() {
        let manifest = load_mvp4_language_readiness_manifest(&manifest_path()).expect("manifest");
        let assessment = manifest_readiness_assessment(&manifest).expect("assessment");
        assert!(assessment.valid);
        assert!(assessment.production_execution_required);
        assert!(!assessment.ready_to_enter_mvp4_4);
        assert_eq!(assessment.status, "manifest_validated_execution_required");
    }

    #[test]
    fn unsupported_language_is_explicit_unmet_not_ready() {
        let manifest = load_mvp4_language_readiness_manifest(&manifest_path()).expect("manifest");
        let row = manifest
            .languages
            .iter()
            .find(|row| row.language_id == "python")
            .expect("python row");
        let capabilities = REQUIRED_CAPABILITY_FLAGS
            .iter()
            .map(|flag| (flag.to_string(), "not_implemented".to_string()))
            .collect();
        let result = assess_language_row(
            &manifest,
            row,
            ObservedLanguageEvidence {
                support_tier: "tier1_syntax_entities".to_string(),
                compatibility_tier: 1,
                capability_statuses: capabilities,
                capability_evidence: BTreeMap::new(),
                catalog_project_resolver_status: "unsupported".to_string(),
                project_resolver_status: "unsupported".to_string(),
                project_resolver_scope: SAME_FILE_INTRAPROCEDURAL_SCOPE.to_string(),
                micro_node_kinds: BTreeSet::new(),
                micro_edge_kinds: BTreeSet::new(),
                persisted_packet_count: 0,
                packet_contract_satisfied: false,
                query_result_count: 0,
                query_language_matches: true,
                profile_db: Some("external/profile.sqlite".to_string()),
                profile_external: true,
                lifecycle_claimable: true,
                repo_local_dot_codegraph_absent: true,
                dynamic_boundary_negative_clean: true,
                product_surfaces: Mvp4ProductSurfaceEvidence {
                    external_db_path: Some("external/profile.sqlite".to_string()),
                    external_trace_root: Some("external/traces".to_string()),
                    trace_root_external: true,
                    repo_local_trace_and_target_absent: true,
                    ..Mvp4ProductSurfaceEvidence::default()
                },
                production_extraction_executed: true,
            },
            Vec::new(),
        );
        assert_eq!(result.verdict, Mvp4LanguageReadinessVerdict::Unmet);
        assert!(result
            .unmet_reasons
            .iter()
            .any(|reason| reason
                == "capability:local_flow_packet_supported:not_implemented:no_accepted_scoped_evidence"));
        assert!(result
            .unmet_reasons
            .iter()
            .any(|reason| reason == "compatibility_tier:1:requires_5"));
        assert!(result
            .unmet_reasons
            .iter()
            .any(|reason| reason == "production_query_returned_no_packets"));
        assert!(result
            .unmet_reasons
            .iter()
            .any(|reason| reason == "cli_status_packet_surface_mismatch"));
    }

    #[test]
    fn complete_production_evidence_can_satisfy_a_row() {
        let manifest = load_mvp4_language_readiness_manifest(&manifest_path()).expect("manifest");
        let row = manifest
            .languages
            .iter()
            .find(|row| row.language_id == "typescript")
            .expect("typescript row");
        let capabilities = manifest
            .shared_requirements
            .capabilities
            .iter()
            .map(|requirement| {
                (
                    requirement.flag.clone(),
                    requirement.accepted_statuses[0].clone(),
                )
            })
            .collect();
        let capability_evidence = manifest
            .shared_requirements
            .capabilities
            .iter()
            .map(|requirement| {
                let scoped = SCOPED_CAPABILITY_FLAGS.contains(&requirement.flag.as_str());
                (
                    requirement.flag.clone(),
                    Mvp4CapabilityEvidence {
                        status: requirement.accepted_statuses[0].clone(),
                        scope: if scoped {
                            SAME_FILE_INTRAPROCEDURAL_SCOPE.to_string()
                        } else {
                            "language_frontend".to_string()
                        },
                        proof_boundary: "unit-test evidence boundary".to_string(),
                        source: if scoped {
                            "scoped_readiness".to_string()
                        } else {
                            "capabilities".to_string()
                        },
                    },
                )
            })
            .collect();
        let result = assess_language_row(
            &manifest,
            row,
            ObservedLanguageEvidence {
                support_tier: "tier5_dataflow_security_test_impact".to_string(),
                compatibility_tier: 5,
                capability_statuses: capabilities,
                capability_evidence,
                catalog_project_resolver_status: "unsupported".to_string(),
                project_resolver_status: "not_applicable".to_string(),
                project_resolver_scope: SAME_FILE_INTRAPROCEDURAL_SCOPE.to_string(),
                micro_node_kinds: manifest
                    .shared_requirements
                    .micro_node_kinds
                    .iter()
                    .cloned()
                    .collect(),
                micro_edge_kinds: manifest
                    .shared_requirements
                    .micro_edge_kinds
                    .iter()
                    .cloned()
                    .collect(),
                persisted_packet_count: 1,
                packet_contract_satisfied: true,
                query_result_count: 1,
                query_language_matches: true,
                profile_db: Some("external/profile.sqlite".to_string()),
                profile_external: true,
                lifecycle_claimable: true,
                repo_local_dot_codegraph_absent: true,
                dynamic_boundary_negative_clean: true,
                product_surfaces: ready_product_surface_evidence(1),
                production_extraction_executed: true,
            },
            Vec::new(),
        );
        assert_eq!(result.verdict, Mvp4LanguageReadinessVerdict::Ready);
        assert!(result.unmet_reasons.is_empty());
    }

    #[test]
    fn profile_must_be_outside_the_fixture_repo() {
        let root = std::env::temp_dir().join(format!("mvp4-profile-safety-{}", unique_suffix()));
        let repo = root.join("repo");
        let external = root.join("profiles").join("production.sqlite");
        let internal = repo.join(".codegraph").join("production.sqlite");
        assert!(profile_is_external(
            &external,
            &root.join("profiles"),
            &repo
        ));
        assert!(!profile_is_external(
            &internal,
            &repo.join(".codegraph"),
            &repo
        ));
    }

    #[test]
    fn mcp_stdin_payload_is_bounded_json_lines_and_rejects_secret_fields() {
        let payload = mcp_session_payload(&[
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {"name": "codegraph.status", "arguments": {"repo": "repo"}}
            }),
        ])
        .expect("bounded MCP payload");
        assert_eq!(payload.lines().count(), 2);
        assert!(payload.len() < MAX_COMMAND_STDIN_BYTES);
        assert!(
            validate_stdin_payload(&format!("{}\n", "x".repeat(MAX_COMMAND_STDIN_BYTES))).is_err()
        );
        assert!(mcp_session_payload(&[json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"arguments": {"api_key": "must-not-be-logged"}}
        })])
        .is_err());
    }

    #[test]
    fn command_audit_redacts_secret_environment_and_argument_values() {
        let environment = BTreeMap::from([
            (
                "CODEGRAPH_TRACE_ROOT".to_string(),
                "external/traces".to_string(),
            ),
            ("CODEGRAPH_API_KEY".to_string(), "secret-value".to_string()),
        ]);
        let audited_environment = auditable_environment(&environment);
        assert_eq!(
            audited_environment
                .get("CODEGRAPH_API_KEY")
                .map(String::as_str),
            Some("<redacted>")
        );
        assert_eq!(
            audited_environment
                .get("CODEGRAPH_TRACE_ROOT")
                .map(String::as_str),
            Some("external/traces")
        );
        assert_eq!(
            auditable_args(&[
                "--api-key".to_string(),
                "secret-value".to_string(),
                "--limit".to_string(),
                "10".to_string(),
            ]),
            vec!["--api-key", "<redacted>", "--limit", "10"]
        );
    }

    #[test]
    fn mcp_response_parser_requires_non_error_structured_content() {
        let responses = vec![json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": {
                "isError": false,
                "structuredContent": {"status": "ok"}
            }
        })];
        let response = mcp_response_for_id(&responses, 7).expect("response id");
        assert_eq!(
            mcp_structured_content(response)
                .expect("structured content")
                .get("status")
                .and_then(Value::as_str),
            Some("ok")
        );
        assert!(mcp_response_for_id(&responses, 8).is_err());
        assert!(mcp_structured_content(&json!({
            "result": {"isError": true, "structuredContent": {"status": "error"}}
        }))
        .is_err());
    }

    #[test]
    fn claimability_parser_rejects_nonclaimable_text_and_false_json() {
        assert!(claimability_is_claimable("claimable"));
        assert!(claimability_is_claimable("claimable_current"));
        assert!(claimability_is_claimable(
            " claimable_source_spanned_parser_facts_v1_same_file_intraprocedural "
        ));
        assert!(claimability_is_claimable(
            "claimable_dict_v1_local_micro_flow_packet"
        ));
        assert!(claimability_is_claimable(
            r#"{"claimable":true,"current":true}"#
        ));
        assert!(!claimability_is_claimable("non_claimable"));
        assert!(!claimability_is_claimable(
            r#"{"claimable":false,"reason":"stale"}"#
        ));
    }

    #[test]
    fn stored_micro_edge_kinds_normalize_to_canonical_contract_names() {
        for &kind in MicroEdgeKind::ALL {
            let lowercase = kind.as_str();
            let uppercase = lowercase.to_ascii_uppercase();
            let padded = format!("  {lowercase}  ");
            assert_eq!(
                micro_edge_contract_kind_from_storage(lowercase).as_deref(),
                Some(uppercase.as_str())
            );
            assert_eq!(
                micro_edge_contract_kind_from_storage(&uppercase).as_deref(),
                Some(uppercase.as_str())
            );
            assert_eq!(
                micro_edge_contract_kind_from_storage(&padded).as_deref(),
                Some(uppercase.as_str())
            );
        }
        assert_eq!(
            micro_edge_contract_kind_from_storage("local_unknown_relation"),
            None
        );
    }

    #[test]
    fn compact_query_total_includes_visible_and_omitted_results() {
        assert_eq!(
            local_flow_query_total_result_count(&serde_json::json!({
                "result_count": 1,
                "result_omitted_count": 4,
            })),
            5
        );
        assert_eq!(
            local_flow_query_total_result_count(&serde_json::json!({
                "result_count": 3,
            })),
            3
        );
        assert_eq!(
            local_flow_query_total_result_count(&serde_json::json!({
                "result_count": u64::MAX,
                "result_omitted_count": 1,
            })),
            usize::MAX
        );
    }

    fn compact_query_fixture(omitted_count: usize) -> Value {
        json!({
            "query": {
                "file": "src/readiness.ts",
                "language": "typescript",
                "source_role": "production"
            },
            "result_count": 1,
            "result_omitted_count": omitted_count,
            "results": [{
                "packet_id": "packet-1",
                "function_entity_id": "function-1",
                "file": "src/readiness.ts",
                "language": "typescript",
                "source_role": "production"
            }]
        })
    }

    #[test]
    fn compact_filtered_query_accepts_visible_plus_omitted_totals() {
        for (omitted_count, expected_total) in [(4, 5), (5, 6)] {
            assert!(local_flow_filtered_query_is_exact(
                &compact_query_fixture(omitted_count),
                expected_total,
                "src/readiness.ts",
                "typescript",
                "production",
            ));
        }
    }

    #[test]
    fn compact_filtered_query_rejects_wrong_totals_echoes_rows_and_empty_visibility() {
        let mut wrong_total = compact_query_fixture(4);
        wrong_total["result_omitted_count"] = json!(3);
        assert!(!local_flow_filtered_query_is_exact(
            &wrong_total,
            5,
            "src/readiness.ts",
            "typescript",
            "production",
        ));

        let mut wrong_echo = compact_query_fixture(4);
        wrong_echo["query"]["language"] = json!("javascript");
        assert!(!local_flow_filtered_query_is_exact(
            &wrong_echo,
            5,
            "src/readiness.ts",
            "typescript",
            "production",
        ));

        let mut wrong_row = compact_query_fixture(4);
        wrong_row["results"][0]["file"] = json!("src/other.ts");
        assert!(!local_flow_filtered_query_is_exact(
            &wrong_row,
            5,
            "src/readiness.ts",
            "typescript",
            "production",
        ));

        let mut no_visible_rows = compact_query_fixture(4);
        no_visible_rows["result_count"] = json!(0);
        no_visible_rows["result_omitted_count"] = json!(5);
        no_visible_rows["results"] = json!([]);
        assert!(!local_flow_filtered_query_is_exact(
            &no_visible_rows,
            5,
            "src/readiness.ts",
            "typescript",
            "production",
        ));
    }

    #[test]
    fn cli_packet_selection_requires_a_function_entity_seed() {
        assert_eq!(
            selected_cli_packet_identity(&compact_query_fixture(4)),
            Some(("packet-1".to_string(), "function-1".to_string()))
        );
        let mut missing_function = compact_query_fixture(4);
        missing_function["results"][0]
            .as_object_mut()
            .expect("result object")
            .remove("function_entity_id");
        assert_eq!(selected_cli_packet_identity(&missing_function), None);
    }

    #[test]
    fn validate_edit_integrity_is_clear_blocking_or_unavailable() {
        assert_eq!(
            validate_edit_packet_integrity_evidence(&json!({
                "micro_flow_packet_integrity": {"blocking_count": 0}
            })),
            (ValidateEditPacketIntegrity::Clear, Some(0))
        );
        assert_eq!(
            validate_edit_packet_integrity_evidence(&json!({
                "micro_flow_packet_integrity": {"blocking_count": 3}
            })),
            (ValidateEditPacketIntegrity::Blocking, Some(3))
        );
        assert_eq!(
            validate_edit_packet_integrity_evidence(&json!({})),
            (ValidateEditPacketIntegrity::Unavailable, None)
        );
    }

    #[test]
    fn mcp_packet_selection_uses_query_order_across_reordered_context_handles() {
        let query = json!({"results": [{"packet_id": "packet-a"}, {"packet_id": "packet-b"}]});
        let context = json!({
            "local_flow_packet_handles": [
                {"packet_id": "packet-b", "expansion_handle": "handle-b"},
                {"packet_id": "packet-a", "expansion_handle": "handle-a"}
            ]
        });
        let selection =
            select_mcp_query_context_packet(&query, &context).expect("packet intersection");
        assert_eq!(selection.packet_id, "packet-a");
        assert_eq!(
            selection
                .context_handle
                .get("expansion_handle")
                .and_then(Value::as_str),
            Some("handle-a")
        );
    }

    #[test]
    fn mcp_packet_selection_finds_later_intersection_and_reports_none_without_one() {
        let query = json!({"results": [
            {"packet_id": "packet-missing"},
            {"packet_id": "packet-shared"}
        ]});
        let context = json!({
            "local_flow_packet_handles": [{"packet_id": "packet-shared"}]
        });
        assert_eq!(
            select_mcp_query_context_packet(&query, &context)
                .expect("later intersection")
                .packet_id,
            "packet-shared"
        );
        assert!(select_mcp_query_context_packet(
            &query,
            &json!({"local_flow_packet_handles": [{"packet_id": "packet-other"}]})
        )
        .is_none());
    }

    #[test]
    fn existing_path_equivalence_requires_two_paths_to_the_same_existing_target() {
        let root = std::env::temp_dir().join(format!("mvp4-path-identity-{}", unique_suffix()));
        let repo = root.join("repo");
        let db = root.join("profile.sqlite");
        fs::create_dir_all(&repo).expect("repo directory");
        fs::write(&db, b"sqlite").expect("db fixture");
        assert!(existing_paths_equivalent(&repo, &repo));
        assert!(!existing_paths_equivalent(&repo, &db));
        assert!(!existing_paths_equivalent(&repo, &root.join("missing")));
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn windows_existing_path_equivalence_accepts_verbatim_case_and_separator_variants() {
        let root = std::env::temp_dir().join(format!("mvp4-win-path-{}", unique_suffix()));
        let existing = root.join("MixedCaseRepo");
        fs::create_dir_all(&existing).expect("existing directory");
        let canonical = fs::canonicalize(&existing).expect("verbatim canonical path");
        let canonical_text = path_string(&canonical);
        let standard = canonical_text
            .strip_prefix("\\\\?\\")
            .unwrap_or(&canonical_text);
        let case_and_separator_variant = standard.replace('\\', "/").to_ascii_uppercase();
        assert!(existing_paths_equivalent(
            &canonical,
            Path::new(&case_and_separator_variant)
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn mcp_descriptor_requires_exact_packet_and_open_response_identity() {
        let root = std::env::temp_dir().join(format!("mvp4-mcp-descriptor-{}", unique_suffix()));
        let repo = root.join("repo");
        let db = root.join("profile.sqlite");
        fs::create_dir_all(&repo).expect("repo directory");
        fs::write(&db, b"sqlite").expect("db fixture");
        let expansion = json!({
            "tool": "codegraph.open_local_flow_packet",
            "arguments": {
                "repo": path_string(&fs::canonicalize(&repo).expect("canonical repo")),
                "db_path": path_string(&db),
                "packet_id": "packet-1",
                "include_ordered_steps": false
            }
        });
        assert!(mcp_expansion_descriptor_is_concrete(
            &expansion, &repo, &db, "packet-1"
        ));
        assert!(mcp_open_response_matches_descriptor(
            &json!({"packet": {"packet_id": "packet-1"}}),
            Some(&expansion)
        ));
        assert!(!mcp_open_response_matches_descriptor(
            &json!({"packet": {"packet_id": "packet-2"}}),
            Some(&expansion)
        ));

        let mut wrong_packet = expansion.clone();
        wrong_packet["arguments"]["packet_id"] = json!("packet-2");
        assert!(!mcp_expansion_descriptor_is_concrete(
            &wrong_packet,
            &repo,
            &db,
            "packet-1"
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scoped_capability_selection_does_not_promote_broad_compiler_requirement() {
        let catalog = LanguageCatalogRow {
            support_tier: "tier5_dataflow_security_test_impact".to_string(),
            compatibility_tier: 5,
            capabilities: BTreeMap::from([(
                "local_binding_resolved".to_string(),
                "requires_compiler".to_string(),
            )]),
            capability_scopes: BTreeMap::from([(
                "local_binding_resolved".to_string(),
                "language_frontend".to_string(),
            )]),
            scoped_readiness: BTreeMap::from([(
                "local_binding_resolved".to_string(),
                vec![CatalogScopedReadiness {
                    status: "supported_exact".to_string(),
                    scope: SAME_FILE_INTRAPROCEDURAL_SCOPE.to_string(),
                    proof_boundary: "same-file lexical binding only".to_string(),
                }],
            )]),
            project_resolver_status: "unsupported".to_string(),
        };
        let requirement = Mvp4CapabilityRequirement {
            flag: "local_binding_resolved".to_string(),
            accepted_statuses: vec!["supported_exact".to_string()],
        };
        let evidence = select_capability_evidence(&catalog, &requirement).expect("scoped evidence");
        assert_eq!(evidence.status, "supported_exact");
        assert_eq!(evidence.scope, SAME_FILE_INTRAPROCEDURAL_SCOPE);
        assert_eq!(evidence.source, "scoped_readiness");
        assert_eq!(
            catalog.capabilities["local_binding_resolved"],
            "requires_compiler"
        );
    }

    #[test]
    fn every_language_fixture_covers_all_six_executable_semantic_classes() {
        let manifest = load_mvp4_language_readiness_manifest(&manifest_path()).expect("manifest");
        for row in &manifest.languages {
            assert_eq!(
                detect_semantic_fixture_classes(row)
                    .into_iter()
                    .collect::<BTreeSet<_>>(),
                REQUIRED_SEMANTIC_FIXTURE_CLASSES
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<BTreeSet<_>>(),
                "{} semantic fixture coverage",
                row.language_id
            );
        }
    }

    #[test]
    fn c_and_cpp_assertion_helpers_are_self_contained() {
        let manifest = load_mvp4_language_readiness_manifest(&manifest_path()).expect("manifest");
        for (language_id, include) in [("c", "#include <assert.h>"), ("cpp", "#include <cassert>")]
        {
            let row = manifest
                .languages
                .iter()
                .find(|row| row.language_id == language_id)
                .unwrap_or_else(|| panic!("{language_id} row"));
            let source = row
                .files
                .iter()
                .map(|file| file.contents.as_str())
                .collect::<Vec<_>>()
                .join("\n");

            assert!(
                source.starts_with(include),
                "{language_id} ordinary include"
            );
            assert!(source.contains(
                "// @codegraph-assertion\nstatic int assert_ready(int value) { return value; }"
            ));
            assert_eq!(source.matches("assert_ready(").count(), 2, "{language_id}");
            assert!(!source.contains("assert("), "{language_id} macro call");
        }
    }

    #[test]
    fn ruby_and_php_readiness_rows_use_static_fixture_shapes() {
        let manifest = load_mvp4_language_readiness_manifest(&manifest_path()).expect("manifest");
        let source_for = |language_id: &str| {
            manifest
                .languages
                .iter()
                .find(|row| row.language_id == language_id)
                .unwrap_or_else(|| panic!("{language_id} row"))
                .files
                .iter()
                .map(|file| file.contents.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };

        let ruby = source_for("ruby");
        assert!(ruby.contains("def property_probe(input)\n  input[:value]\nend"));
        assert!(!ruby.contains("input.value"));
        assert!(
            ruby.contains("# @codegraph-assertion\ndef assert_ready(value)\n  return value\nend")
        );
        assert!(!ruby.contains("raise 'assert'"));
        assert!(ruby.contains("if !result\n    return result\n  end"));
        assert!(!ruby.contains(".nil?"));
        assert_eq!(ruby.matches("assert_ready(").count(), 2);

        let php = source_for("php");
        assert!(php
            .contains("// @codegraph-assertion\nfunction assert_ready($value) { return $value; }"));
        assert!(!php.contains("assert("));
        assert_eq!(php.matches("assert_ready(").count(), 2);
    }

    #[test]
    fn run_root_inside_workspace_is_rejected() {
        let options = Mvp4LanguageReadinessRunnerOptions {
            manifest_path: manifest_path(),
            run_root: workspace_root()
                .join("target")
                .join("forbidden-readiness-run"),
            release_binary: std::env::current_exe().expect("test executable"),
            command_timeout: Duration::from_secs(1),
        };
        let error = validate_runner_options(&options).expect_err("workspace-local run rejected");
        assert!(error
            .to_string()
            .contains("outside the CodeGraph workspace"));
    }
}
