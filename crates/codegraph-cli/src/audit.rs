use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use codegraph_index::{
    candidate_spool_query_index_path, index_repo_to_db_with_options,
    scope::{self, IndexScope, IndexScopeDecision, ScopeAction, ScopePathKind, ScopeRuleKind},
    IndexBuildMode, IndexOptions, IndexScopeOptions, StorageMode,
};
use codegraph_parser::{content_hash, detect_language};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::storage_budget;

const AUDIT_SCHEMA_VERSION: u32 = 1;
const LABEL_SCHEMA_VERSION: u32 = 1;
const STORAGE_MICRO_SCHEMA_VERSION: u32 = 1;
const VECTOR_CHUNK_INSPECTOR_SCHEMA_VERSION: u32 = 1;
const DEFAULT_VECTOR_CHUNKS_FILE_NAME: &str = "codegraph-vector-chunks.json";
const DEFAULT_SAMPLE_LIMIT: usize = 100;
const DEFAULT_PATH_SAMPLE_LIMIT: usize = 20;
const DEFAULT_VECTOR_CHUNK_SAMPLE_LIMIT: usize = 20;
const VECTOR_CHUNK_TEXT_PREVIEW_CHARS: usize = 240;
const DEFAULT_SAMPLE_SEED: u64 = 1;
const DEFAULT_PATH_SAMPLE_MAX_EDGE_LOAD: usize = 512;
const DEFAULT_PATH_SAMPLE_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_LABELS_JSON: &str = "reports/audit/artifacts/manual_relation_labels.json";
const DEFAULT_LABELS_MARKDOWN: &str = "reports/audit/artifacts/manual_relation_labels.md";
const DEFAULT_LABEL_SUMMARY_JSON: &str = "reports/audit/manual_relation_labeling_summary.json";
const DEFAULT_LABEL_SUMMARY_MARKDOWN: &str = "reports/audit/manual_relation_labeling_summary.md";
const EDGE_INDEXES: &[(&str, &str)] = &[
    (
        "idx_edges_head_relation",
        "CREATE INDEX IF NOT EXISTS idx_edges_head_relation ON edges(head_id_key, relation_id)",
    ),
    (
        "idx_edges_tail_relation",
        "CREATE INDEX IF NOT EXISTS idx_edges_tail_relation ON edges(tail_id_key, relation_id)",
    ),
    (
        "idx_edges_span_path",
        "CREATE INDEX IF NOT EXISTS idx_edges_span_path ON edges(span_path_id)",
    ),
];
const SECONDARY_INDEXES: &[(&str, &str)] = &[
    (
        "idx_entities_path",
        "CREATE INDEX IF NOT EXISTS idx_entities_path ON entities(path_id)",
    ),
    (
        "idx_entities_name",
        "CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name_id)",
    ),
    (
        "idx_entities_qname",
        "CREATE INDEX IF NOT EXISTS idx_entities_qname ON entities(qualified_name_id)",
    ),
    (
        "idx_edges_head_relation",
        "CREATE INDEX IF NOT EXISTS idx_edges_head_relation ON edges(head_id_key, relation_id)",
    ),
    (
        "idx_edges_tail_relation",
        "CREATE INDEX IF NOT EXISTS idx_edges_tail_relation ON edges(tail_id_key, relation_id)",
    ),
    (
        "idx_edges_span_path",
        "CREATE INDEX IF NOT EXISTS idx_edges_span_path ON edges(span_path_id)",
    ),
    (
        "idx_source_spans_path",
        "CREATE INDEX IF NOT EXISTS idx_source_spans_path ON source_spans(path_id)",
    ),
    (
        "idx_retrieval_traces_created",
        "CREATE INDEX IF NOT EXISTS idx_retrieval_traces_created ON retrieval_traces(created_at_unix_ms)",
    ),
];
const BROAD_UNUSED_INDEXES: &[&str] = &[
    "idx_edges_span_path",
    "idx_source_spans_path",
    "idx_retrieval_traces_created",
];
const SAMPLE_CLASSIFICATIONS: &[&str] = &[
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
];
const TEST_RELATIONS: &[&str] = &["TESTS", "ASSERTS", "COVERS", "FIXTURES_FOR"];
const MOCK_RELATIONS: &[&str] = &["MOCKS", "STUBS"];

pub fn run_audit_command(args: &[String]) -> Result<Value, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Err(
            "Usage: codegraph-mcp audit <index-scope|vector-chunks|storage|storage-micro|schema-check|storage-experiments|sample-edges|sample-paths|relation-counts|label-samples|summarize-labels> [ARGS]".to_string(),
        );
    };

    match subcommand {
        "index-scope" | "index_scope" | "scope" => run_index_scope_command(&args[1..]),
        "vector-chunks" | "vector_chunks" | "vectors" | "vector-artifact" => {
            run_vector_chunks_command(&args[1..])
        }
        "storage" | "storage-forensics" => run_storage_command(&args[1..]),
        "storage-micro" | "storage_micro" => run_storage_micro_command(&args[1..]),
        "schema-check" | "schema" | "validate-schema" => run_schema_check_command(&args[1..]),
        "storage-experiments" | "storage-experiment" => run_storage_experiments_command(&args[1..]),
        "sample-edges" | "edge-sample" => run_sample_edges_command(&args[1..]),
        "sample-paths" | "path-sample" => run_sample_paths_command(&args[1..]),
        "relation-counts" | "relations" => run_relation_counts_command(&args[1..]),
        "label-samples" | "labels" => run_label_samples_command(&args[1..]),
        "summarize-labels" | "label-summary" => run_summarize_labels_command(&args[1..]),
        other => Err(format!("unknown audit subcommand: {other}")),
    }
}

#[derive(Debug, Clone)]
struct StorageOptions {
    db_path: PathBuf,
    json_path: Option<PathBuf>,
    markdown_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct VectorChunksOptions {
    artifact_path: Option<PathBuf>,
    runtime_sidecar_path: Option<PathBuf>,
    audit_artifact_path: Option<PathBuf>,
    db_path: Option<PathBuf>,
    repo: Option<PathBuf>,
    json_path: Option<PathBuf>,
    markdown_path: Option<PathBuf>,
    sample_limit: usize,
}

#[derive(Debug, Clone, Serialize)]
struct VectorInspectorDbPassport {
    passport_version: u32,
    codegraph_schema_version: u32,
    storage_mode: String,
    index_scope_policy_hash: String,
    scope_policy_json: String,
    canonical_repo_root: String,
    git_remote: Option<String>,
    worktree_root: Option<String>,
    repo_head: Option<String>,
    source_discovery_policy_version: String,
    codegraph_build_version: Option<String>,
    last_successful_index_timestamp: Option<u64>,
    last_completed_run_id: Option<String>,
    last_run_status: String,
    integrity_gate_result: String,
    files_seen: u64,
    files_indexed: u64,
    created_at_unix_ms: u64,
    updated_at_unix_ms: u64,
}

#[derive(Debug, Clone)]
struct IndexScopeAuditOptions {
    repo: PathBuf,
    json_path: Option<PathBuf>,
    markdown_path: Option<PathBuf>,
    scope: IndexScopeOptions,
    include_included_examples: bool,
    include_excluded_examples: bool,
    example_limit: usize,
}

#[derive(Debug, Clone)]
struct StorageMicroOptions {
    out_dir: PathBuf,
    cases: BTreeSet<StorageMicroCaseKind>,
    batch_sizes: Vec<usize>,
    keep_artifacts: bool,
    json_path: Option<PathBuf>,
    markdown_path: Option<PathBuf>,
    no_context_pack: bool,
    respect_gitignore: bool,
    storage_budget: storage_budget::StorageBudgetOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum StorageMicroCaseKind {
    Simple,
    Expression,
    InlineTests,
    Duplicates,
    ExcludedJunk,
}

#[derive(Debug, Clone)]
struct StorageMicroCaseSpec {
    name: String,
    kind: StorageMicroCaseKind,
    file_count: usize,
}

#[derive(Debug, Clone)]
struct SchemaCheckOptions {
    db_path: PathBuf,
    json_path: Option<PathBuf>,
    markdown_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct SampleEdgesOptions {
    db_path: PathBuf,
    relation: Option<String>,
    limit: usize,
    seed: u64,
    json_path: Option<PathBuf>,
    markdown_path: Option<PathBuf>,
    include_snippets: bool,
}

#[derive(Debug, Clone)]
struct SamplePathsOptions {
    db_path: PathBuf,
    limit: usize,
    seed: u64,
    json_path: Option<PathBuf>,
    markdown_path: Option<PathBuf>,
    include_snippets: bool,
    max_edge_load: usize,
    timeout_ms: u64,
    mode: PathSampleMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PathSampleMode {
    Proof,
    Audit,
    Debug,
}

impl Default for PathSampleMode {
    fn default() -> Self {
        Self::Proof
    }
}

impl PathSampleMode {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw.to_ascii_lowercase().as_str() {
            "proof" => Ok(Self::Proof),
            "audit" => Ok(Self::Audit),
            "debug" => Ok(Self::Debug),
            other => Err(format!(
                "invalid --mode value: {other}; expected proof, audit, or debug"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Proof => "proof",
            Self::Audit => "audit",
            Self::Debug => "debug",
        }
    }

    fn allows_generated_fallback(self) -> bool {
        !matches!(self, Self::Proof)
    }
}

#[derive(Debug, Clone)]
struct RelationCountsOptions {
    db_path: PathBuf,
    json_path: Option<PathBuf>,
    markdown_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct LabelSamplesOptions {
    edge_json_paths: Vec<PathBuf>,
    edge_markdown_paths: Vec<PathBuf>,
    path_json_paths: Vec<PathBuf>,
    path_markdown_paths: Vec<PathBuf>,
    json_path: Option<PathBuf>,
    markdown_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct SummarizeLabelsOptions {
    label_paths: Vec<PathBuf>,
    label_dir: Option<PathBuf>,
    json_path: Option<PathBuf>,
    markdown_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct StorageExperimentOptions {
    db_path: PathBuf,
    workdir: PathBuf,
    json_path: Option<PathBuf>,
    markdown_path: Option<PathBuf>,
    keep_copies: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileFamilySize {
    database_bytes: u64,
    wal_bytes: u64,
    shm_bytes: u64,
    total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SidecarSnapshot {
    kind: String,
    path: String,
    size: u64,
    mtime_unix_ms: Option<u64>,
    hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DbFileSnapshot {
    main_db_size: u64,
    main_db_mtime_unix_ms: Option<u64>,
    main_db_hash: Option<String>,
    sidecars: Vec<SidecarSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReadOnlyInspectionAudit {
    main_db_size_before: u64,
    main_db_size_after: u64,
    main_db_mtime_before: Option<u64>,
    main_db_mtime_after: Option<u64>,
    main_db_hash_before: Option<String>,
    main_db_hash_after: Option<String>,
    main_db_hash_algorithm: String,
    sidecars_before: Vec<SidecarSnapshot>,
    sidecars_after: Vec<SidecarSnapshot>,
    sidecar_status: String,
    artifact_mutated_during_inspection: bool,
    sidecar_only_change: bool,
    read_only_mode_used: String,
    immutable_mode_used: bool,
    immutable_mode_reason: String,
}

struct ReadOnlyConnection {
    connection: Connection,
    read_only_mode_used: String,
    immutable_mode_used: bool,
    immutable_mode_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PageMetrics {
    page_size_bytes: u64,
    page_count: u64,
    freelist_count: u64,
    live_page_bytes: u64,
    free_page_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageObjectSize {
    name: String,
    object_type: String,
    row_count: Option<u64>,
    pages: u64,
    total_bytes: u64,
    payload_bytes: u64,
    unused_bytes: u64,
    percent_of_database_file: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DictionaryMetric {
    table: String,
    row_count: u64,
    value_bytes: u64,
    unique_index_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QualifiedNameMetric {
    row_count: u64,
    full_value_bytes: u64,
    prefix_value_bytes: u64,
    suffix_value_bytes: u64,
    unique_index_bytes: u64,
    stores_full_qualified_name_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageInspection {
    schema_version: u32,
    db_path: String,
    inspection_read_only: bool,
    main_db_size_before: u64,
    main_db_size_after: u64,
    main_db_mtime_before: Option<u64>,
    main_db_mtime_after: Option<u64>,
    main_db_hash_before: Option<String>,
    main_db_hash_after: Option<String>,
    main_db_hash_algorithm: String,
    sidecars_before: Vec<SidecarSnapshot>,
    sidecars_after: Vec<SidecarSnapshot>,
    sidecar_status: String,
    artifact_mutated_during_inspection: bool,
    sidecar_only_change: bool,
    read_only_mode_used: String,
    immutable_mode_used: bool,
    immutable_mode_reason: String,
    dbstat_available: bool,
    integrity_check: Value,
    file_family: FileFamilySize,
    page_metrics: PageMetrics,
    objects: Vec<StorageObjectSize>,
    categories: StorageCategoryBreakdown,
    table_row_metrics: Vec<TableRowMetric>,
    aggregate_metrics: AggregateStorageMetrics,
    dictionary_metrics: Vec<DictionaryMetric>,
    qualified_name_metric: Option<QualifiedNameMetric>,
    fts_storage: Option<FtsStorageMetric>,
    edge_fact_mix: Option<EdgeFactMix>,
    index_usage: Vec<IndexUsageReport>,
    core_query_plans: Vec<CoreQueryPlanReport>,
    vacuum_analyze_measurement: Value,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SchemaValidationReport {
    schema_version: u32,
    db_path: String,
    status: String,
    inspection_read_only: bool,
    main_db_size_before: u64,
    main_db_size_after: u64,
    main_db_mtime_before: Option<u64>,
    main_db_mtime_after: Option<u64>,
    main_db_hash_before: Option<String>,
    main_db_hash_after: Option<String>,
    main_db_hash_algorithm: String,
    sidecars_before: Vec<SidecarSnapshot>,
    sidecars_after: Vec<SidecarSnapshot>,
    sidecar_status: String,
    artifact_mutated_during_inspection: bool,
    sidecar_only_change: bool,
    read_only_mode_used: String,
    immutable_mode_used: bool,
    immutable_mode_reason: String,
    user_version: u32,
    expected_columns: Vec<SchemaColumnCheck>,
    views: Vec<SchemaCompileCheck>,
    default_query_sql: Vec<SchemaCompileCheck>,
    failures: Vec<String>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SchemaColumnCheck {
    table: String,
    column: String,
    present: bool,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SchemaCompileCheck {
    name: String,
    sql: String,
    status: String,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditEndpoint {
    id: String,
    name: Option<String>,
    qualified_name: Option<String>,
    kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditSourceSpan {
    repo_relative_path: String,
    start_line: u32,
    start_column: Option<u32>,
    end_line: u32,
    end_column: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceSnippet {
    path: String,
    start_line: u32,
    end_line: u32,
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EdgeSample {
    ordinal: usize,
    edge_id: String,
    head: AuditEndpoint,
    relation: String,
    tail: AuditEndpoint,
    source_span: AuditSourceSpan,
    relation_direction: String,
    exactness: String,
    confidence: f64,
    repo_commit: Option<String>,
    file_hash: Option<String>,
    derived: bool,
    extractor: String,
    fact_classification: String,
    production_test_mock_context: String,
    provenance_edges: Vec<String>,
    metadata: Value,
    span_loaded: bool,
    span_load_error: Option<String>,
    source_snippet: Option<SourceSnippet>,
    missing_metadata: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EdgeSampleReport {
    schema_version: u32,
    db_path: String,
    relation_filter: Option<String>,
    limit: usize,
    seed: u64,
    include_snippets: bool,
    manual_classification_options: Vec<String>,
    samples: Vec<EdgeSample>,
    notes: Vec<String>,
}

#[derive(Debug, Clone)]
struct RawEdgeSample {
    edge_id: String,
    head: AuditEndpoint,
    relation: String,
    tail: AuditEndpoint,
    source_span: AuditSourceSpan,
    exactness: String,
    confidence: f64,
    repo_commit: Option<String>,
    file_hash: Option<String>,
    derived: bool,
    extractor: String,
    provenance_json: String,
    metadata_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PathEdgeSample {
    edge_id: Option<String>,
    head: AuditEndpoint,
    relation: String,
    tail: AuditEndpoint,
    source_span: Option<AuditSourceSpan>,
    relation_direction: String,
    exactness: Option<String>,
    confidence: Option<f64>,
    derived: Option<bool>,
    fact_classification: String,
    production_test_mock_context: String,
    provenance_edges: Vec<String>,
    missing_metadata: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PathEvidenceSample {
    ordinal: usize,
    path_id: String,
    generated_by_audit: bool,
    task_or_query: Option<String>,
    summary: Option<String>,
    source: AuditEndpoint,
    target: AuditEndpoint,
    relation_sequence: Vec<String>,
    edge_list: Vec<PathEdgeSample>,
    source_spans: Vec<AuditSourceSpan>,
    source_snippets: Vec<SourceSnippet>,
    exactness: String,
    confidence: f64,
    derived_provenance_label: String,
    production_test_mock_context: String,
    metadata: Value,
    missing_metadata: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PathEvidenceSampleReport {
    schema_version: u32,
    db_path: String,
    limit: usize,
    seed: u64,
    include_snippets: bool,
    #[serde(default)]
    mode: PathSampleMode,
    #[serde(default)]
    max_edge_load: usize,
    #[serde(default)]
    timeout_ms: u64,
    stored_path_count: u64,
    #[serde(default)]
    candidate_path_count: usize,
    #[serde(default)]
    loaded_path_edge_count: usize,
    #[serde(default)]
    edge_load_truncated: bool,
    #[serde(default)]
    path_evidence_truncated: bool,
    #[serde(default)]
    path_evidence_omitted_count: usize,
    #[serde(default)]
    hydration_budget_exhausted: bool,
    #[serde(default)]
    source_snippet_omitted_count: usize,
    #[serde(default)]
    partial_diagnostic: bool,
    #[serde(default)]
    timeout_ms_exhausted: bool,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    fallback_allowed: bool,
    generated_path_count: usize,
    #[serde(default)]
    generated_fallback_used: bool,
    #[serde(default)]
    timing: PathSamplerTiming,
    #[serde(default)]
    explain_query_plan: Vec<PathSamplerQueryPlan>,
    #[serde(default)]
    index_status: Vec<PathEvidenceIndexStatus>,
    manual_classification_options: Vec<String>,
    samples: Vec<PathEvidenceSample>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PathSamplerTiming {
    total_ms: u64,
    open_db_ms: u64,
    repo_roots_ms: u64,
    count_ms: u64,
    candidate_select_ms: u64,
    path_rows_load_ms: u64,
    path_edges_load_ms: u64,
    endpoint_load_ms: u64,
    snippet_load_ms: u64,
    sample_build_ms: u64,
    explain_ms: u64,
    index_check_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PathSamplerQueryPlan {
    name: String,
    sql: String,
    explain_query_plan: Vec<QueryPlanRow>,
    query_plan_analysis: QueryPlanAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PathEvidenceIndexStatus {
    object: String,
    required_shape: String,
    present: bool,
    satisfied_by: Option<String>,
    columns: Vec<String>,
    notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ManualLabelSet {
    #[serde(default)]
    true_positive: bool,
    #[serde(default)]
    false_positive: bool,
    #[serde(default)]
    wrong_direction: bool,
    #[serde(default)]
    wrong_target: bool,
    #[serde(default)]
    wrong_span: bool,
    #[serde(default)]
    stale: bool,
    #[serde(default)]
    duplicate: bool,
    #[serde(default)]
    unresolved_mislabeled_exact: bool,
    #[serde(default)]
    test_mock_leaked: bool,
    #[serde(default)]
    derived_missing_provenance: bool,
    #[serde(default)]
    unsure: bool,
    #[serde(default)]
    unsupported: bool,
    #[serde(default)]
    false_positive_cause: Option<String>,
    #[serde(default)]
    wrong_span_cause: Option<String>,
    #[serde(default)]
    unsupported_pattern: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LabeledSample {
    sample_type: String,
    source_json: String,
    source_markdown: Option<String>,
    ordinal: usize,
    sample_id: String,
    relation: String,
    relation_sequence: Vec<String>,
    edge_ids: Vec<String>,
    exactness: Option<String>,
    confidence: Option<f64>,
    source_span_count: usize,
    span_loaded: Option<bool>,
    fact_classification: Option<String>,
    production_test_mock_context: Option<String>,
    labels: ManualLabelSet,
    labeled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaxonomyCount {
    category: String,
    count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrecisionEstimate {
    eligible_samples: u64,
    true_positive: u64,
    false_positive: u64,
    wrong_span: u64,
    precision: Option<f64>,
    recall: Option<f64>,
    recall_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RelationPrecisionSummary {
    relation: String,
    labeled_samples: u64,
    unlabeled_samples: u64,
    unsupported_samples: u64,
    unsure_samples: u64,
    true_positive: u64,
    false_positive: u64,
    wrong_span: u64,
    precision: Option<f64>,
    recall: Option<f64>,
    recall_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LabelSummary {
    total_samples: u64,
    edge_samples: u64,
    path_samples: u64,
    labeled_samples: u64,
    unlabeled_samples: u64,
    unsupported_samples: u64,
    unsure_samples: u64,
    relation_precision: Vec<RelationPrecisionSummary>,
    source_span_precision: PrecisionEstimate,
    false_positive_taxonomy: Vec<TaxonomyCount>,
    wrong_span_taxonomy: Vec<TaxonomyCount>,
    unsupported_pattern_taxonomy: Vec<TaxonomyCount>,
    recall_estimate_status: String,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LabelSamplesReport {
    schema_version: u32,
    label_schema_version: u32,
    generated_at_unix_ms: u128,
    edge_inputs: Vec<String>,
    path_inputs: Vec<String>,
    markdown_inputs: Vec<String>,
    manual_classification_options: Vec<String>,
    samples: Vec<LabeledSample>,
    summary: LabelSummary,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ContextBreakdown {
    test_or_fixture_inferred: u64,
    mock_or_stub_inferred: u64,
    unknown_not_first_class: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TypeCount {
    entity_type: String,
    count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RelationCountRow {
    relation: String,
    edge_count: u64,
    source_span_count: u64,
    missing_source_span_rows: u64,
    source_span_row_status: String,
    duplicate_edge_count: u64,
    duplicate_edge_count_status: String,
    derived_count: u64,
    exactness_counts: BTreeMap<String, u64>,
    exactness_count_status: String,
    context_breakdown: ContextBreakdown,
    top_head_entity_types: Vec<TypeCount>,
    top_tail_entity_types: Vec<TypeCount>,
    top_entity_type_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RelationCountsReport {
    schema_version: u32,
    db_path: String,
    relation_count: usize,
    total_edges: u64,
    relations: Vec<RelationCountRow>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageCategoryBreakdown {
    dictionary_table_bytes: u64,
    unique_text_index_bytes: u64,
    edge_index_bytes: u64,
    fts_bytes: u64,
    source_span_bytes: u64,
    snippet_like_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TableRowMetric {
    table: String,
    row_count: u64,
    total_bytes: u64,
    payload_bytes: u64,
    average_total_bytes_per_row: f64,
    average_payload_bytes_per_row: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AggregateStorageMetrics {
    table_count: usize,
    index_count: usize,
    total_rows_observed: u64,
    edge_count: u64,
    proof_edge_count: u64,
    structural_record_count: u64,
    callsite_record_count: u64,
    callsite_arg_record_count: u64,
    semantic_edge_count: u64,
    edge_table_bytes: u64,
    edge_index_bytes: u64,
    structural_table_bytes: u64,
    callsite_table_bytes: u64,
    callsite_arg_table_bytes: u64,
    average_database_bytes_per_edge: f64,
    average_database_bytes_per_semantic_edge: f64,
    average_edge_table_bytes_per_edge: f64,
    average_edge_table_plus_index_bytes_per_edge: f64,
    source_span_count: u64,
    source_span_table_bytes: u64,
    average_source_span_bytes_per_row: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FtsStorageMetric {
    total_bytes: u64,
    row_count: u64,
    payload_bytes: u64,
    kind_counts: BTreeMap<String, u64>,
    stores_source_snippets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EdgeFactMix {
    total_edges: u64,
    derived_edges: u64,
    exactness_counts: BTreeMap<String, u64>,
    edge_class_counts: BTreeMap<String, u64>,
    context_counts: BTreeMap<String, u64>,
    heuristic_or_unknown_edges: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryPlanRow {
    id: i64,
    parent: i64,
    detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryPlanAnalysis {
    uses_indexes: bool,
    indexes_used: Vec<String>,
    full_scans: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryLatencyMeasurement {
    name: String,
    sql: String,
    elapsed_ms: u64,
    rows_observed: u64,
    status: String,
    error: Option<String>,
    explain_query_plan: Vec<QueryPlanRow>,
    query_plan_analysis: QueryPlanAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoreQueryPlanReport {
    name: String,
    default_workflow: String,
    sql: String,
    status: String,
    error: Option<String>,
    explain_query_plan: Vec<QueryPlanRow>,
    query_plan_analysis: QueryPlanAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexUsageReport {
    name: String,
    table: String,
    columns: Vec<String>,
    unique: bool,
    origin: String,
    partial: bool,
    total_bytes: u64,
    sql: Option<String>,
    default_query_usage: Vec<String>,
    used_by_core_query_plans: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageExperimentCheckpoint {
    name: String,
    mutation_elapsed_ms: u64,
    integrity_check: Value,
    file_family: FileFamilySize,
    page_metrics: PageMetrics,
    categories: StorageCategoryBreakdown,
    top_objects: Vec<StorageObjectSize>,
    dictionary_metrics: Vec<DictionaryMetric>,
    qualified_name_metric: Option<QualifiedNameMetric>,
    query_latencies: Vec<QueryLatencyMeasurement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryRegression {
    query: String,
    checkpoint: String,
    baseline_ms: u64,
    after_ms: u64,
    degraded: bool,
    ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryLatencyDelta {
    query: String,
    before_ms: u64,
    after_ms: u64,
    delta_ms: i64,
    before_status: String,
    after_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageExperimentSummary {
    db_size_before_bytes: u64,
    db_size_after_bytes: u64,
    size_delta_bytes: i64,
    size_delta_percent: f64,
    core_query_latency_before_after: Vec<QueryLatencyDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageExperimentRecommendation {
    recommended: bool,
    decision: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageExperiment {
    name: String,
    copied_db_path: String,
    copy_removed: bool,
    mutations: Vec<String>,
    summary: StorageExperimentSummary,
    checkpoints: Vec<StorageExperimentCheckpoint>,
    degraded_queries: Vec<QueryRegression>,
    graph_truth: Value,
    context_packet: Value,
    recommendation: StorageExperimentRecommendation,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageExperimentReport {
    schema_version: u32,
    original_db_path: String,
    run_dir: String,
    original_file_family: FileFamilySize,
    experiments: Vec<StorageExperiment>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct ScopeDirStats {
    count: u64,
    bytes: u64,
}

#[derive(Debug, Clone, Default)]
struct ScopeVisibilityStats {
    visible: bool,
    included: u64,
    graph_parsed: u64,
    text_evidence_candidates: u64,
    examples: Vec<String>,
}

#[derive(Debug, Clone)]
struct ScopeAuditAggregate {
    paths_walked: u64,
    files_considered: u64,
    files_would_be_parsed: u64,
    files_text_evidence_candidates: u64,
    files_skipped: u64,
    bytes_considered: u64,
    bytes_included: u64,
    bytes_excluded_files: u64,
    directory_pruned_count: u64,
    hard_excluded_directory_count: u64,
    soft_excluded_path_count: u64,
    warning_count: u64,
    action_counts: BTreeMap<String, u64>,
    rule_counts: BTreeMap<String, u64>,
    language_counts: BTreeMap<String, u64>,
    included_bytes_by_top_level: BTreeMap<String, ScopeDirStats>,
    excluded_bytes_by_top_level: BTreeMap<String, ScopeDirStats>,
    included_files_by_directory: BTreeMap<String, ScopeDirStats>,
    excluded_files_by_directory: BTreeMap<String, ScopeDirStats>,
    extensions: BTreeMap<String, ScopeDirStats>,
    suspicious_included_dirs: BTreeMap<String, ScopeDirStats>,
    hard_excluded_directory_hits: Vec<Value>,
    soft_excluded_warnings: Vec<Value>,
    warning_examples: Vec<Value>,
    included_examples: Vec<Value>,
    excluded_examples: Vec<Value>,
    makefile: ScopeVisibilityStats,
    kconfig: ScopeVisibilityStats,
    docs: ScopeVisibilityStats,
    configs: ScopeVisibilityStats,
}

impl Default for ScopeAuditAggregate {
    fn default() -> Self {
        Self {
            paths_walked: 0,
            files_considered: 0,
            files_would_be_parsed: 0,
            files_text_evidence_candidates: 0,
            files_skipped: 0,
            bytes_considered: 0,
            bytes_included: 0,
            bytes_excluded_files: 0,
            directory_pruned_count: 0,
            hard_excluded_directory_count: 0,
            soft_excluded_path_count: 0,
            warning_count: 0,
            action_counts: BTreeMap::new(),
            rule_counts: BTreeMap::new(),
            language_counts: BTreeMap::new(),
            included_bytes_by_top_level: BTreeMap::new(),
            excluded_bytes_by_top_level: BTreeMap::new(),
            included_files_by_directory: BTreeMap::new(),
            excluded_files_by_directory: BTreeMap::new(),
            extensions: BTreeMap::new(),
            suspicious_included_dirs: BTreeMap::new(),
            hard_excluded_directory_hits: Vec::new(),
            soft_excluded_warnings: Vec::new(),
            warning_examples: Vec::new(),
            included_examples: Vec::new(),
            excluded_examples: Vec::new(),
            makefile: ScopeVisibilityStats::default(),
            kconfig: ScopeVisibilityStats::default(),
            docs: ScopeVisibilityStats::default(),
            configs: ScopeVisibilityStats::default(),
        }
    }
}

fn run_index_scope_command(args: &[String]) -> Result<Value, String> {
    let options = parse_index_scope_options(args)?;
    let started = Instant::now();
    let repo_root = fs::canonicalize(&options.repo).map_err(|error| {
        format!(
            "repository path does not exist or cannot be resolved: {} ({error})",
            options.repo.display()
        )
    })?;
    if !repo_root.is_dir() {
        return Err(format!(
            "repository path is not a directory: {}",
            repo_root.display()
        ));
    }

    let normal_codegraph_before = repo_root.join(".codegraph").exists();
    let scope = IndexScope::for_repo(&repo_root, options.scope.clone());
    let mut aggregate = ScopeAuditAggregate::default();
    walk_index_scope(&repo_root, &repo_root, &scope, &options, &mut aggregate)?;
    let normal_codegraph_after = repo_root.join(".codegraph").exists();

    let duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    let files_included = aggregate.files_would_be_parsed + aggregate.files_text_evidence_candidates;
    let report = json!({
        "schema_version": 1,
        "status": "ok",
        "audit": "index_scope",
        "command": "codegraph-mcp audit index-scope",
        "repo_root": path_string(&repo_root),
        "generated_at_unix_ms": now_unix_ms(),
        "duration_ms": round3(duration_ms),
        "dry_run": true,
        "db_created": false,
        "db_path": Value::Null,
        "normal_codegraph_db_before": normal_codegraph_before,
        "normal_codegraph_db_after": normal_codegraph_after,
        "normal_codegraph_db_created": !normal_codegraph_before && normal_codegraph_after,
        "options": {
            "default_excludes_enabled": !options.scope.no_default_excludes,
            "include_ignored": options.scope.include_ignored,
            "no_default_excludes": options.scope.no_default_excludes,
            "respect_gitignore": options.scope.respect_gitignore,
            "include_patterns": options.scope.include_patterns,
            "exclude_patterns": options.scope.exclude_patterns,
            "examples_requested": options.include_included_examples || options.include_excluded_examples,
            "example_limit": options.example_limit,
        },
        "counts": {
            "total_paths_walked": aggregate.paths_walked,
            "paths_walked": aggregate.paths_walked,
            "files_considered": aggregate.files_considered,
            "files_included": files_included,
            "files_that_would_be_parsed": aggregate.files_would_be_parsed,
            "files_would_be_parsed": aggregate.files_would_be_parsed,
            "files_text_evidence_candidates": aggregate.files_text_evidence_candidates,
            "files_skipped": aggregate.files_skipped,
            "directory_pruned_count": aggregate.directory_pruned_count,
            "hard_excluded_directory_hits": aggregate.hard_excluded_directory_count,
            "soft_excluded_paths": aggregate.soft_excluded_path_count,
            "warnings": aggregate.warning_count,
            "bytes_considered": aggregate.bytes_considered,
            "bytes_included": aggregate.bytes_included,
            "bytes_excluded_files": aggregate.bytes_excluded_files,
        },
        "action_counts": aggregate.action_counts,
        "rule_counts": aggregate.rule_counts,
        "parsed_language_counts": aggregate.language_counts,
        "bytes_by_top_level_directory": stats_map_json(&aggregate.included_bytes_by_top_level, 50),
        "included_bytes_by_top_level_directory": stats_map_json(&aggregate.included_bytes_by_top_level, 50),
        "excluded_file_bytes_by_top_level_directory": stats_map_json(&aggregate.excluded_bytes_by_top_level, 50),
        "extensions_by_count_and_bytes": stats_map_json(&aggregate.extensions, 100),
        "top_included_directories": stats_map_json(&aggregate.included_files_by_directory, 25),
        "top_excluded_directories": stats_map_json(&aggregate.excluded_files_by_directory, 25),
        "suspicious_included_directories": stats_map_json(&aggregate.suspicious_included_dirs, 25),
        "hard_excluded_directory_hits": aggregate.hard_excluded_directory_hits,
        "soft_excluded_warnings": aggregate.soft_excluded_warnings,
        "warning_examples": aggregate.warning_examples,
        "included_examples": if options.include_included_examples { aggregate.included_examples } else { Vec::new() },
        "excluded_examples": if options.include_excluded_examples { aggregate.excluded_examples } else { Vec::new() },
        "visibility": {
            "makefile": visibility_json(&aggregate.makefile),
            "kconfig": visibility_json(&aggregate.kconfig),
            "docs": visibility_json(&aggregate.docs),
            "configs": visibility_json(&aggregate.configs),
            "makefile_kconfig_visible_as_text_evidence": (aggregate.makefile.text_evidence_candidates + aggregate.kconfig.text_evidence_candidates) > 0,
            "docs_configs_text_evidence_only_where_not_graph_parsed": true,
        },
        "claim_boundaries": {
            "diagnostic_only": true,
            "public_benchmark_claim": false,
            "codegraph_superiority_claim": false,
            "final_intended_performance_pass_claim": false,
            "full_index_run": false,
            "source_parsed": false,
        },
        "output_contract": {
            "concise_for_agent_use": true,
            "examples_omitted_unless_requested": !(options.include_included_examples || options.include_excluded_examples),
            "no_db_required": true,
            "no_indexing_performed": true,
        },
        "json_output": options.json_path.as_ref().map(path_string),
        "markdown_output": options.markdown_path.as_ref().map(path_string),
    });
    let markdown = render_index_scope_markdown(&report);
    write_optional_outputs(
        &report,
        &markdown,
        &options.json_path,
        &options.markdown_path,
    )?;
    Ok(index_scope_stdout_summary(&report))
}

fn walk_index_scope(
    root: &Path,
    path: &Path,
    scope: &IndexScope,
    options: &IndexScopeAuditOptions,
    aggregate: &mut ScopeAuditAggregate,
) -> Result<(), String> {
    aggregate.paths_walked += 1;
    if path.is_dir() {
        if path != root {
            let relative = relative_scope_path(root, path);
            let decision = scope.evaluate_repo_path(&relative, ScopePathKind::Directory);
            record_scope_decision(aggregate, &decision, 0, None, options);
            if decision.excluded() {
                let include_descendant = scope::could_include_descendant_decision(
                    &relative,
                    &scope.options().include_patterns,
                );
                let pruned = !include_descendant.could_include_descendant;
                if pruned {
                    aggregate.directory_pruned_count += 1;
                    return Ok(());
                }
            }
        }

        let mut entries = match fs::read_dir(path) {
            Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
            Err(_) => return Ok(()),
        };
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            walk_index_scope(root, &entry.path(), scope, options, aggregate)?;
        }
        return Ok(());
    }

    if path.is_file() {
        let relative = relative_scope_path(root, path);
        let decision = scope.evaluate_repo_path(&relative, ScopePathKind::File);
        let bytes = fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let language = detect_language(path).map(|language| language.as_str().to_string());
        aggregate.files_considered += 1;
        aggregate.bytes_considered += bytes;
        record_scope_decision(aggregate, &decision, bytes, language.as_deref(), options);
        if decision.excluded() {
            aggregate.files_skipped += 1;
            aggregate.bytes_excluded_files += bytes;
            increment_stats(
                &mut aggregate.excluded_bytes_by_top_level,
                top_level_component(&relative),
                bytes,
            );
            increment_stats(
                &mut aggregate.excluded_files_by_directory,
                parent_directory(&relative),
                bytes,
            );
        } else if let Some(language) = language {
            aggregate.files_would_be_parsed += 1;
            aggregate.bytes_included += bytes;
            *aggregate.language_counts.entry(language).or_default() += 1;
            increment_included_file_stats(aggregate, &relative, bytes);
            record_visibility(aggregate, &relative, true);
        } else {
            aggregate.files_text_evidence_candidates += 1;
            aggregate.bytes_included += bytes;
            increment_included_file_stats(aggregate, &relative, bytes);
            record_visibility(aggregate, &relative, false);
        }
    }

    Ok(())
}

fn record_scope_decision(
    aggregate: &mut ScopeAuditAggregate,
    decision: &IndexScopeDecision,
    bytes: u64,
    language: Option<&str>,
    options: &IndexScopeAuditOptions,
) {
    *aggregate
        .action_counts
        .entry(format!("{:?}", decision.action))
        .or_default() += 1;
    *aggregate
        .rule_counts
        .entry(format!("{:?}", decision.rule_kind))
        .or_default() += 1;

    let decision_value = scope_decision_json(decision, bytes, language);
    if decision.path_kind == ScopePathKind::Directory
        && decision.action == ScopeAction::WouldExclude
        && decision.rule_kind == ScopeRuleKind::HardExclude
    {
        aggregate.hard_excluded_directory_count += 1;
        push_limited_value(
            &mut aggregate.hard_excluded_directory_hits,
            decision_value.clone(),
            options.example_limit,
        );
    }
    if decision.rule_kind == ScopeRuleKind::SoftExclude {
        aggregate.soft_excluded_path_count += 1;
        push_limited_value(
            &mut aggregate.soft_excluded_warnings,
            decision_value.clone(),
            options.example_limit,
        );
    }
    if decision.warned() {
        aggregate.warning_count += 1;
        push_limited_value(
            &mut aggregate.warning_examples,
            decision_value.clone(),
            options.example_limit,
        );
    }
    if decision.path_kind == ScopePathKind::File {
        if decision.excluded() {
            push_limited_value(
                &mut aggregate.excluded_examples,
                decision_value,
                options.example_limit,
            );
        } else {
            push_limited_value(
                &mut aggregate.included_examples,
                decision_value,
                options.example_limit,
            );
        }
    }
}

fn increment_included_file_stats(aggregate: &mut ScopeAuditAggregate, relative: &str, bytes: u64) {
    increment_stats(
        &mut aggregate.included_bytes_by_top_level,
        top_level_component(relative),
        bytes,
    );
    increment_stats(
        &mut aggregate.included_files_by_directory,
        parent_directory(relative),
        bytes,
    );
    increment_stats(&mut aggregate.extensions, extension_key(relative), bytes);
    for component in suspicious_components(relative) {
        increment_stats(&mut aggregate.suspicious_included_dirs, component, bytes);
    }
}

fn increment_stats(map: &mut BTreeMap<String, ScopeDirStats>, key: String, bytes: u64) {
    let entry = map.entry(key).or_default();
    entry.count += 1;
    entry.bytes += bytes;
}

fn record_visibility(aggregate: &mut ScopeAuditAggregate, relative: &str, graph_parsed: bool) {
    let file_name = Path::new(relative)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(relative);
    let is_makefile = file_name == "Makefile" || file_name.ends_with(".mk");
    let is_kconfig = file_name == "Kconfig"
        || file_name.starts_with("Kconfig.")
        || file_name.starts_with("Config.in");
    let is_docs = relative == "docs" || relative.starts_with("docs/");
    let is_configs = relative == "configs" || relative.starts_with("configs/");

    if is_makefile {
        update_visibility(&mut aggregate.makefile, relative, graph_parsed);
    }
    if is_kconfig {
        update_visibility(&mut aggregate.kconfig, relative, graph_parsed);
    }
    if is_docs {
        update_visibility(&mut aggregate.docs, relative, graph_parsed);
    }
    if is_configs {
        update_visibility(&mut aggregate.configs, relative, graph_parsed);
    }
}

fn update_visibility(stats: &mut ScopeVisibilityStats, relative: &str, graph_parsed: bool) {
    stats.visible = true;
    stats.included += 1;
    if graph_parsed {
        stats.graph_parsed += 1;
    } else {
        stats.text_evidence_candidates += 1;
    }
    if stats.examples.len() < 20 {
        stats.examples.push(relative.to_string());
    }
}

fn scope_decision_json(decision: &IndexScopeDecision, bytes: u64, language: Option<&str>) -> Value {
    json!({
        "path": decision.normalized_path,
        "path_kind": format!("{:?}", decision.path_kind),
        "action": format!("{:?}", decision.action),
        "rule_kind": format!("{:?}", decision.rule_kind),
        "matched_rule": decision.matched_rule,
        "reason": decision.reason,
        "classification": format!("{:?}", decision.classification),
        "gitignored": decision.gitignored,
        "warnings": decision.warnings.iter().map(|warning| format!("{:?}", warning)).collect::<Vec<_>>(),
        "bytes": bytes,
        "would_be_graph_parsed": language.is_some(),
        "language": language,
        "evidence_type": if language.is_some() { "graph_parse_candidate" } else { "text_evidence_candidate" },
    })
}

fn stats_map_json(map: &BTreeMap<String, ScopeDirStats>, limit: usize) -> Vec<Value> {
    let mut rows = map
        .iter()
        .map(|(key, stats)| (key, stats.count, stats.bytes))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .2
            .cmp(&left.2)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.0.cmp(right.0))
    });
    rows.into_iter()
        .take(limit)
        .map(|(key, count, bytes)| {
            json!({
                "path": key,
                "count": count,
                "bytes": bytes,
            })
        })
        .collect()
}

fn visibility_json(stats: &ScopeVisibilityStats) -> Value {
    json!({
        "visible": stats.visible,
        "included": stats.included,
        "graph_parsed": stats.graph_parsed,
        "text_evidence_candidates": stats.text_evidence_candidates,
        "examples": stats.examples,
    })
}

fn render_index_scope_markdown(report: &Value) -> String {
    let counts = &report["counts"];
    let visibility = &report["visibility"];
    format!(
        "# Index Scope Dry Run\n\n\
Diagnostic-only release CLI dry run. No index DB is created and no source parse/extraction is performed.\n\n\
## Summary\n\n\
- Status: `{}`\n\
- Repo: `{}`\n\
- Files considered: `{}`\n\
- Files that would be parsed: `{}`\n\
- Text evidence candidates: `{}`\n\
- Files skipped: `{}`\n\
- Directory prunes: `{}`\n\
- Warnings: `{}`\n\
- Normal `.codegraph` created: `{}`\n\n\
## Visibility\n\n\
- Makefile visible: `{}`; text evidence candidates: `{}`; graph parsed: `{}`\n\
- Kconfig visible: `{}`; text evidence candidates: `{}`; graph parsed: `{}`\n\
- Docs visible: `{}`; text evidence candidates: `{}`; graph parsed: `{}`\n\
- Configs visible: `{}`; text evidence candidates: `{}`; graph parsed: `{}`\n\n\
## Claim Boundaries\n\n\
This report is diagnostic-only local evidence. It is not a public benchmark, not a performance claim, and not an index result.\n",
        report["status"].as_str().unwrap_or("unknown"),
        report["repo_root"].as_str().unwrap_or("unknown"),
        counts["files_considered"].as_u64().unwrap_or_default(),
        counts["files_that_would_be_parsed"].as_u64().unwrap_or_default(),
        counts["files_text_evidence_candidates"].as_u64().unwrap_or_default(),
        counts["files_skipped"].as_u64().unwrap_or_default(),
        counts["directory_pruned_count"].as_u64().unwrap_or_default(),
        counts["warnings"].as_u64().unwrap_or_default(),
        report["normal_codegraph_db_created"].as_bool().unwrap_or(false),
        visibility["makefile"]["visible"].as_bool().unwrap_or(false),
        visibility["makefile"]["text_evidence_candidates"].as_u64().unwrap_or_default(),
        visibility["makefile"]["graph_parsed"].as_u64().unwrap_or_default(),
        visibility["kconfig"]["visible"].as_bool().unwrap_or(false),
        visibility["kconfig"]["text_evidence_candidates"].as_u64().unwrap_or_default(),
        visibility["kconfig"]["graph_parsed"].as_u64().unwrap_or_default(),
        visibility["docs"]["visible"].as_bool().unwrap_or(false),
        visibility["docs"]["text_evidence_candidates"].as_u64().unwrap_or_default(),
        visibility["docs"]["graph_parsed"].as_u64().unwrap_or_default(),
        visibility["configs"]["visible"].as_bool().unwrap_or(false),
        visibility["configs"]["text_evidence_candidates"].as_u64().unwrap_or_default(),
        visibility["configs"]["graph_parsed"].as_u64().unwrap_or_default(),
    )
}

fn index_scope_stdout_summary(report: &Value) -> Value {
    let hard_hits = report["hard_excluded_directory_hits"]
        .as_array()
        .map(|items| items.iter().take(8).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let soft_warnings = report["soft_excluded_warnings"]
        .as_array()
        .map(|items| items.iter().take(8).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    json!({
        "schema_version": report["schema_version"].clone(),
        "status": report["status"].clone(),
        "audit": report["audit"].clone(),
        "command": report["command"].clone(),
        "repo_root": report["repo_root"].clone(),
        "dry_run": report["dry_run"].clone(),
        "db_created": report["db_created"].clone(),
        "normal_codegraph_db_created": report["normal_codegraph_db_created"].clone(),
        "counts": report["counts"].clone(),
        "visibility": compact_visibility_summary(&report["visibility"]),
        "hard_excluded_directory_hits": hard_hits,
        "soft_excluded_warnings": soft_warnings,
        "suspicious_included_directories_count": report["suspicious_included_directories"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
        "claim_boundaries": report["claim_boundaries"].clone(),
        "output_contract": report["output_contract"].clone(),
        "json_output": report["json_output"].clone(),
        "markdown_output": report["markdown_output"].clone(),
    })
}

fn compact_visibility_summary(visibility: &Value) -> Value {
    json!({
        "makefile": compact_visibility_entry(&visibility["makefile"]),
        "kconfig": compact_visibility_entry(&visibility["kconfig"]),
        "docs": compact_visibility_entry(&visibility["docs"]),
        "configs": compact_visibility_entry(&visibility["configs"]),
        "makefile_kconfig_visible_as_text_evidence": visibility["makefile_kconfig_visible_as_text_evidence"].clone(),
        "docs_configs_text_evidence_only_where_not_graph_parsed": visibility["docs_configs_text_evidence_only_where_not_graph_parsed"].clone(),
    })
}

fn compact_visibility_entry(entry: &Value) -> Value {
    json!({
        "visible": entry["visible"].clone(),
        "included": entry["included"].clone(),
        "graph_parsed": entry["graph_parsed"].clone(),
        "text_evidence_candidates": entry["text_evidence_candidates"].clone(),
        "examples_count": entry["examples"].as_array().map(Vec::len).unwrap_or_default(),
        "first_example": entry["examples"].as_array().and_then(|items| items.first()).cloned(),
    })
}

fn run_vector_chunks_command(args: &[String]) -> Result<Value, String> {
    let options = parse_vector_chunks_options(args)?;
    let resolved_db_path = resolve_vector_chunks_db_path(&options);
    let artifact_path = resolve_vector_chunks_artifact_path(&options, resolved_db_path.as_deref())?;
    let report =
        inspect_vector_chunks_artifact(&options, &artifact_path, resolved_db_path.as_deref());
    let markdown = render_vector_chunks_markdown(&report);
    write_optional_outputs(
        &report,
        &markdown,
        &options.json_path,
        &options.markdown_path,
    )?;
    Ok(report)
}

fn inspect_vector_chunks_artifact(
    options: &VectorChunksOptions,
    artifact_path: &Path,
    db_path: Option<&Path>,
) -> Value {
    let started = Instant::now();
    let artifact_exists = artifact_path.exists();
    let artifact_hash_before = stable_file_hash(artifact_path);
    let artifact_mtime_before = fs::metadata(artifact_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_unix_ms);
    let artifact_bytes = fs::metadata(artifact_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);

    if !artifact_exists {
        return vector_chunks_error_report(
            options,
            artifact_path,
            db_path,
            "missing",
            "artifact_missing",
            &format!("artifact does not exist: {}", artifact_path.display()),
            artifact_hash_before,
            artifact_mtime_before,
            artifact_bytes,
            started,
        );
    }

    let raw = match fs::read(artifact_path) {
        Ok(raw) => raw,
        Err(error) => {
            return vector_chunks_error_report(
                options,
                artifact_path,
                db_path,
                "corrupt",
                "artifact_read_failed",
                &format!("failed to read artifact: {error}"),
                artifact_hash_before,
                artifact_mtime_before,
                artifact_bytes,
                started,
            )
        }
    };

    let compression_status = detect_vector_artifact_compression(&raw);
    let mut artifact_format = detect_vector_artifact_format(artifact_path, &raw);
    if artifact_format == "binary" {
        return vector_chunks_error_report(
            options,
            artifact_path,
            db_path,
            "corrupt",
            "unsupported_binary_or_compressed_artifact",
            "artifact is not UTF-8 JSON/JSONL; binary vector sidecar parsing is not implemented by this inspector",
            artifact_hash_before,
            artifact_mtime_before,
            artifact_bytes,
            started,
        );
    }

    let text = match std::str::from_utf8(&raw) {
        Ok(text) => text,
        Err(error) => {
            return vector_chunks_error_report(
                options,
                artifact_path,
                db_path,
                "corrupt",
                "artifact_utf8_decode_failed",
                &format!("artifact is not UTF-8 JSON/JSONL: {error}"),
                artifact_hash_before,
                artifact_mtime_before,
                artifact_bytes,
                started,
            )
        }
    };

    let parsed = match parse_vector_artifact_text(text, &artifact_format) {
        Ok(parsed) => parsed,
        Err(error) => {
            return vector_chunks_error_report(
                options,
                artifact_path,
                db_path,
                "corrupt",
                "artifact_parse_failed",
                &error,
                artifact_hash_before,
                artifact_mtime_before,
                artifact_bytes,
                started,
            )
        }
    };
    artifact_format = parsed.artifact_format.clone();

    let artifact_kind =
        classify_vector_artifact_kind(&parsed.root, &parsed.metadata, &parsed.chunks);
    let chunk_counts = vector_chunk_counts(
        &artifact_kind,
        &parsed.metadata,
        &parsed.root,
        &parsed.chunks,
    );
    let composition = vector_chunk_composition(&parsed.chunks);
    let mut byte_accounting = vector_chunk_byte_accounting(
        &parsed.root,
        &parsed.metadata,
        &parsed.chunks,
        artifact_bytes,
        text.len() as u64,
    );
    annotate_vector_artifact_byte_accounting(&mut byte_accounting, &artifact_kind, artifact_bytes);
    let provider_lifecycle = vector_provider_lifecycle(
        options.repo.as_deref(),
        db_path,
        &parsed.root,
        &parsed.metadata,
        &parsed.chunks,
        &artifact_hash_before,
    );
    let related_artifacts = vector_related_artifacts(options, artifact_path, &artifact_kind);
    let query_index = vector_query_index_metadata(&parsed.metadata, &parsed.root, artifact_path);
    let sample_chunks = vector_chunk_samples(&parsed.chunks, options.sample_limit);
    let safety = vector_safety_conclusions(&parsed.root, &parsed.metadata, &parsed.chunks);
    let artifact_hash_after = stable_file_hash(artifact_path);
    let artifact_mtime_after = fs::metadata(artifact_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_unix_ms);

    json!({
        "schema_version": VECTOR_CHUNK_INSPECTOR_SCHEMA_VERSION,
        "status": "ok",
        "audit": "vector_chunks",
        "command": "codegraph-mcp audit vector-chunks",
        "generated_at_unix_ms": now_unix_ms(),
        "duration_ms": round3(started.elapsed().as_secs_f64() * 1000.0),
        "options": {
            "artifact": path_string(artifact_path),
            "runtime_sidecar": options.runtime_sidecar_path.as_ref().map(path_string),
            "audit_artifact": options.audit_artifact_path.as_ref().map(path_string),
            "db": db_path.map(path_string),
            "repo": options.repo.as_ref().map(path_string),
            "sample": options.sample_limit,
            "json_output": options.json_path.as_ref().map(path_string),
            "markdown_output": options.markdown_path.as_ref().map(path_string),
        },
        "artifact_identity": {
            "artifact_path": path_string(artifact_path),
            "artifact_exists": true,
            "artifact_kind": artifact_kind,
            "artifact_format": artifact_format,
            "artifact_bytes": artifact_bytes,
            "pretty_json": parsed.artifact_format == "pretty_json",
            "compact_json": parsed.artifact_format == "compact_json",
            "jsonl": parsed.artifact_format == "jsonl",
            "binary": false,
            "compression_status": compression_status,
            "vector_payload_compression": vector_string(&parsed.metadata, &parsed.root, &[&["vector_payload_compression"]])
                .unwrap_or_else(|| compression_status.to_string()),
        },
        "chunk_counts": chunk_counts,
        "chunk_composition": composition,
        "byte_accounting": byte_accounting,
        "query_index": query_index,
        "related_artifacts": related_artifacts,
        "passport_lifecycle_provider": provider_lifecycle,
        "sample_chunks": sample_chunks,
        "safety_conclusions": safety,
        "mutation_check": {
            "artifact_hash_algorithm": "fnv1a64",
            "artifact_hash_before": artifact_hash_before,
            "artifact_hash_after": artifact_hash_after,
            "artifact_mtime_before": artifact_mtime_before,
            "artifact_mtime_after": artifact_mtime_after,
            "artifact_mutated_during_inspection": artifact_hash_before != artifact_hash_after || artifact_mtime_before != artifact_mtime_after,
            "db": db_path.map(|path| vector_db_mutation_status(path)),
        },
        "claim_boundaries": {
            "candidate_only": true,
            "diagnostic_only": true,
            "public_claim": false,
            "can_answer_graph_proof": false,
            "creates_graph_relations": false,
            "graph_verification_required_for_entity_hits": true,
            "full_split_implemented": matches!(artifact_kind.as_str(), "vector_runtime_sidecar" | "audit_artifact"),
        },
    })
}

fn vector_chunks_error_report(
    options: &VectorChunksOptions,
    artifact_path: &Path,
    db_path: Option<&Path>,
    validity_status: &str,
    error_kind: &str,
    message: &str,
    artifact_hash_before: Option<String>,
    artifact_mtime_before: Option<u64>,
    artifact_bytes: u64,
    started: Instant,
) -> Value {
    let artifact_hash_after = stable_file_hash(artifact_path);
    let artifact_mtime_after = fs::metadata(artifact_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_unix_ms);
    let (artifact_format, compression_status, binary) = if artifact_path.exists() {
        match fs::read(artifact_path) {
            Ok(raw) => {
                let format = detect_vector_artifact_format(artifact_path, &raw);
                let binary = format == "binary";
                (
                    format,
                    detect_vector_artifact_compression(&raw).to_string(),
                    binary,
                )
            }
            Err(_) => ("unknown".to_string(), "unknown".to_string(), false),
        }
    } else {
        ("missing".to_string(), "unknown".to_string(), false)
    };
    json!({
        "schema_version": VECTOR_CHUNK_INSPECTOR_SCHEMA_VERSION,
        "status": "error",
        "audit": "vector_chunks",
        "command": "codegraph-mcp audit vector-chunks",
        "generated_at_unix_ms": now_unix_ms(),
        "duration_ms": round3(started.elapsed().as_secs_f64() * 1000.0),
        "error": {
            "kind": error_kind,
            "message": message,
        },
        "options": {
            "artifact": path_string(artifact_path),
            "runtime_sidecar": options.runtime_sidecar_path.as_ref().map(path_string),
            "audit_artifact": options.audit_artifact_path.as_ref().map(path_string),
            "db": db_path.map(path_string),
            "repo": options.repo.as_ref().map(path_string),
            "sample": options.sample_limit,
            "json_output": options.json_path.as_ref().map(path_string),
            "markdown_output": options.markdown_path.as_ref().map(path_string),
        },
        "artifact_identity": {
            "artifact_path": path_string(artifact_path),
            "artifact_exists": artifact_path.exists(),
            "artifact_kind": "unknown",
            "artifact_format": artifact_format,
            "artifact_bytes": artifact_bytes,
            "pretty_json": false,
            "compact_json": false,
            "jsonl": false,
            "binary": binary,
            "compression_status": compression_status.clone(),
            "vector_payload_compression": compression_status,
        },
        "chunk_counts": empty_vector_chunk_counts(),
        "chunk_composition": empty_vector_chunk_composition(),
        "byte_accounting": {
            "actual_index_file_bytes": artifact_bytes,
            "estimated_f32_payload_bytes": 0,
            "indexed_chunk_text_bytes": 0,
            "metadata_estimated_bytes": 0,
            "selection_reason_bytes": 0,
            "repeated_field_overhead_estimate": 0,
            "artifact_to_payload_ratio": Value::Null,
            "chunk_text_share_percent": 0.0,
            "metadata_share_percent": 0.0,
            "audit_overhead_share_percent": 0.0,
            "byte_accounting_method": "error_path_no_chunks_loaded",
        },
        "passport_lifecycle_provider": {
            "db_passport_hash": Value::Null,
            "repo_hash": Value::Null,
            "scope_hash": Value::Null,
            "provider_name": Value::Null,
            "model": Value::Null,
            "dims": Value::Null,
            "extraction_version": Value::Null,
            "validity_status": validity_status,
            "reason": message,
            "db_binding": db_path.map(|path| vector_db_binding_without_artifact(path)),
        },
        "sample_chunks": [],
        "safety_conclusions": {
            "stores_embedding_vectors": false,
            "stores_chunk_text": false,
            "stores_chunk_metadata": false,
            "stores_full_source_body": false,
            "creates_graph_relations": false,
            "can_answer_graph_proof": false,
            "candidate_only": true,
            "deterministic_embeddings_regenerated_from_chunk_text": false,
            "embedding_reconstruction": "not_available_error_path",
        },
        "mutation_check": {
            "artifact_hash_algorithm": "fnv1a64",
            "artifact_hash_before": artifact_hash_before,
            "artifact_hash_after": artifact_hash_after,
            "artifact_mtime_before": artifact_mtime_before,
            "artifact_mtime_after": artifact_mtime_after,
            "artifact_mutated_during_inspection": artifact_hash_before != artifact_hash_after || artifact_mtime_before != artifact_mtime_after,
            "db": db_path.map(|path| vector_db_mutation_status(path)),
        },
        "claim_boundaries": {
            "candidate_only": true,
            "diagnostic_only": true,
            "public_claim": false,
            "can_answer_graph_proof": false,
            "creates_graph_relations": false,
            "graph_verification_required_for_entity_hits": true,
            "full_split_implemented": false,
        },
    })
}

#[derive(Debug, Clone)]
struct ParsedVectorArtifact {
    root: Value,
    metadata: Value,
    chunks: Vec<Value>,
    artifact_format: String,
}

fn parse_vector_artifact_text(
    text: &str,
    detected_format: &str,
) -> Result<ParsedVectorArtifact, String> {
    if detected_format == "jsonl" {
        return parse_vector_jsonl_artifact(text);
    }
    let root: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let artifact_format = if detected_format == "unknown" {
        if text.lines().count() > 1 {
            "pretty_json".to_string()
        } else {
            "compact_json".to_string()
        }
    } else {
        detected_format.to_string()
    };
    let metadata = root
        .get("metadata")
        .or_else(|| root.get("manifest"))
        .or_else(|| root.get("artifact_metadata"))
        .cloned()
        .unwrap_or_else(|| {
            if root.is_object() {
                root.clone()
            } else {
                Value::Null
            }
        });
    let chunks = if let Some(array) = root.get("chunks").and_then(Value::as_array) {
        array.clone()
    } else if let Some(array) = root.get("runtime_chunks").and_then(Value::as_array) {
        array.clone()
    } else if let Some(array) = root.get("audit_chunks").and_then(Value::as_array) {
        array.clone()
    } else if let Some(array) = root.as_array() {
        array.clone()
    } else {
        Vec::new()
    };
    Ok(ParsedVectorArtifact {
        root,
        metadata,
        chunks,
        artifact_format,
    })
}

fn parse_vector_jsonl_artifact(text: &str) -> Result<ParsedVectorArtifact, String> {
    let mut chunks = Vec::new();
    let mut metadata = Value::Null;
    let mut manifest_lines = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|error| format!("invalid JSONL line {}: {error}", line_index + 1))?;
        if value.get("chunk_id").is_some() || value.get("text").is_some() {
            chunks.push(value);
        } else {
            if metadata.is_null() {
                metadata = value
                    .get("metadata")
                    .or_else(|| value.get("manifest"))
                    .cloned()
                    .unwrap_or_else(|| value.clone());
            }
            manifest_lines.push(value);
        }
    }
    let root = json!({
        "artifact_format": "jsonl",
        "metadata": metadata,
        "manifest_lines": manifest_lines,
        "chunks": chunks,
    });
    let chunks = root["chunks"].as_array().cloned().unwrap_or_default();
    Ok(ParsedVectorArtifact {
        root,
        metadata,
        chunks,
        artifact_format: "jsonl".to_string(),
    })
}

fn detect_vector_artifact_format(path: &Path, raw: &[u8]) -> String {
    if std::str::from_utf8(raw).is_err() {
        return "binary".to_string();
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let text = std::str::from_utf8(raw).unwrap_or_default();
    let trimmed = text.trim_start();
    if extension == "jsonl" {
        return "jsonl".to_string();
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if text.lines().count() > 1 && (text.contains("\n  \"") || text.contains("\n    \"")) {
            "pretty_json".to_string()
        } else {
            "compact_json".to_string()
        }
    } else if !trimmed.is_empty() {
        "jsonl".to_string()
    } else {
        "unknown".to_string()
    }
}

fn detect_vector_artifact_compression(raw: &[u8]) -> &'static str {
    if raw.starts_with(&[0x1f, 0x8b]) {
        "gzip"
    } else if raw.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        "zstd"
    } else {
        "none"
    }
}

fn classify_vector_artifact_kind(root: &Value, metadata: &Value, chunks: &[Value]) -> String {
    for value in [
        vector_string(metadata, root, &[&["artifact_kind"]]),
        vector_string(metadata, root, &[&["kind"]]),
        vector_string(metadata, root, &[&["metadata", "artifact_kind"]]),
    ]
    .into_iter()
    .flatten()
    {
        match normalize_vector_artifact_kind(&value).as_deref() {
            Some(kind) => return kind.to_string(),
            None => {}
        }
    }
    if vector_bool(metadata, root, &[&["diagnostic_only"]]).unwrap_or(false) {
        return "audit_artifact".to_string();
    }
    if vector_string(metadata, root, &[&["metadata_version"]]).as_deref()
        == Some("vector_chunk_index_metadata_v1")
        && vector_string(metadata, root, &[&["index_artifact_format"]]).as_deref()
            == Some("pretty_json")
        && !chunks.is_empty()
    {
        return "legacy_pretty_json_vector_artifact".to_string();
    }
    let scope = vector_string(metadata, root, &[&["source_scope"]]).unwrap_or_default();
    let scope_lower = scope.to_ascii_lowercase();
    if scope_lower.contains("spool") {
        "candidate_spool".to_string()
    } else if scope_lower.contains("runtime") {
        "vector_runtime_sidecar".to_string()
    } else {
        "unknown".to_string()
    }
}

fn normalize_vector_artifact_kind(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().replace('-', "_").as_str() {
        "candidate_spool" | "spool" | "fast_candidate_spool" => Some("candidate_spool"),
        "vector_runtime_sidecar" | "runtime_sidecar" | "runtime" => Some("vector_runtime_sidecar"),
        "audit_artifact" | "audit" | "diagnostic_artifact" => Some("audit_artifact"),
        "legacy_pretty_json_vector_artifact" | "legacy_vector_artifact" => {
            Some("legacy_pretty_json_vector_artifact")
        }
        "unknown" => Some("unknown"),
        _ => None,
    }
}

fn vector_chunk_counts(
    artifact_kind: &str,
    metadata: &Value,
    root: &Value,
    chunks: &[Value],
) -> Value {
    let chunk_len = chunks.len() as u64;
    let generated = vector_u64(metadata, root, &[&["generated_total_chunks"]]).unwrap_or(chunk_len);
    let selected = vector_u64(metadata, root, &[&["selected_total_chunks"]]).unwrap_or(chunk_len);
    let persisted = vector_u64(
        metadata,
        root,
        &[&["persisted_total_chunks"], &["chunk_count"]],
    )
    .unwrap_or(chunk_len);
    let spooled = vector_u64(metadata, root, &[&["spooled_total_chunks"]]).unwrap_or_else(|| {
        if artifact_kind == "candidate_spool" {
            chunk_len
        } else {
            0
        }
    });
    let runtime = vector_u64(metadata, root, &[&["runtime_total_chunks"]]).unwrap_or_else(|| {
        if artifact_kind == "vector_runtime_sidecar" {
            chunk_len
        } else {
            0
        }
    });
    let audit = vector_u64(metadata, root, &[&["audit_total_chunks"]]).unwrap_or_else(|| {
        if artifact_kind == "audit_artifact" {
            chunk_len
        } else {
            0
        }
    });
    json!({
        "generated_total_chunks": generated,
        "spooled_total_chunks": spooled,
        "selected_total_chunks": selected,
        "persisted_total_chunks": persisted,
        "runtime_total_chunks": runtime,
        "audit_total_chunks": audit,
        "omitted_by_cap": vector_u64(metadata, root, &[&["omitted_by_cap"], &["omitted_chunks"]]).unwrap_or(0),
        "omitted_low_signal": vector_u64(metadata, root, &[&["omitted_low_signal"]]).unwrap_or(0),
        "omitted_by_bucket_limit": vector_u64(metadata, root, &[&["omitted_by_bucket_limit"]]).unwrap_or(0),
        "omitted_by_dedup": vector_u64(metadata, root, &[&["omitted_by_dedup"]]).unwrap_or(0),
    })
}

fn empty_vector_chunk_counts() -> Value {
    json!({
        "generated_total_chunks": 0,
        "spooled_total_chunks": 0,
        "selected_total_chunks": 0,
        "persisted_total_chunks": 0,
        "runtime_total_chunks": 0,
        "audit_total_chunks": 0,
        "omitted_by_cap": 0,
        "omitted_low_signal": 0,
        "omitted_by_bucket_limit": 0,
        "omitted_by_dedup": 0,
    })
}

fn vector_chunk_composition(chunks: &[Value]) -> Value {
    let mut by_source_kind = BTreeMap::new();
    let mut by_chunk_kind = BTreeMap::new();
    let mut by_file_kind = BTreeMap::new();
    let mut by_top_level_dir = BTreeMap::new();
    let mut by_proof_status = BTreeMap::new();
    let mut by_graph_proof = BTreeMap::new();
    let mut by_claimable = BTreeMap::new();
    let mut by_requires_graph_verification = BTreeMap::new();
    let mut by_selection_bucket = BTreeMap::new();

    for chunk in chunks {
        let source_kind = chunk_str(chunk, "source_kind").unwrap_or("unknown");
        let chunk_kind = chunk_str(chunk, "chunk_kind").unwrap_or("unknown");
        let path = chunk_str(chunk, "path").unwrap_or("");
        let file_kind = chunk_str(chunk, "file_kind")
            .map(str::to_string)
            .unwrap_or_else(|| infer_vector_file_kind(path));
        let top_level = chunk_str(chunk, "top_level_dir")
            .map(str::to_string)
            .unwrap_or_else(|| top_level_component(path));
        let proof_status = chunk_str(chunk, "proof_status").unwrap_or("unknown");
        let graph_proof = chunk_bool(chunk, "graph_proof")
            .unwrap_or(false)
            .to_string();
        let claimable = chunk_bool(chunk, "claimable_for_graph")
            .unwrap_or(false)
            .to_string();
        let requires_graph_verification = chunk_requires_graph_verification(chunk).to_string();
        let selection_bucket = chunk_str(chunk, "selection_bucket").unwrap_or("unknown");

        increment_count(&mut by_source_kind, source_kind);
        increment_count(&mut by_chunk_kind, chunk_kind);
        increment_count(&mut by_file_kind, &file_kind);
        increment_count(&mut by_top_level_dir, &top_level);
        increment_count(&mut by_proof_status, proof_status);
        increment_count(&mut by_graph_proof, &graph_proof);
        increment_count(&mut by_claimable, &claimable);
        increment_count(
            &mut by_requires_graph_verification,
            &requires_graph_verification,
        );
        increment_count(&mut by_selection_bucket, selection_bucket);
    }

    json!({
        "by_source_kind": by_source_kind,
        "by_chunk_kind": by_chunk_kind,
        "by_file_kind": by_file_kind,
        "by_top_level_dir": by_top_level_dir,
        "by_proof_status": by_proof_status,
        "by_graph_proof": by_graph_proof,
        "by_claimable_for_graph": by_claimable,
        "by_requires_graph_verification": by_requires_graph_verification,
        "by_selection_bucket": by_selection_bucket,
    })
}

fn empty_vector_chunk_composition() -> Value {
    json!({
        "by_source_kind": {},
        "by_chunk_kind": {},
        "by_file_kind": {},
        "by_top_level_dir": {},
        "by_proof_status": {},
        "by_graph_proof": {},
        "by_claimable_for_graph": {},
        "by_requires_graph_verification": {},
        "by_selection_bucket": {},
    })
}

fn vector_query_index_metadata(metadata: &Value, root: &Value, artifact_path: &Path) -> Value {
    let query_index_path = vector_string(metadata, root, &[&["query_index_path"]])
        .map(PathBuf::from)
        .unwrap_or_else(|| candidate_spool_query_index_path(artifact_path));
    let query_index_exists = query_index_path.exists();
    let query_index_bytes = fs::metadata(&query_index_path)
        .map(|metadata| metadata.len())
        .ok()
        .or_else(|| vector_u64(metadata, root, &[&["query_index_bytes"]]));
    json!({
        "query_index_status": vector_string(metadata, root, &[&["query_index_status"]])
            .unwrap_or_else(|| if query_index_exists { "present_unvalidated".to_string() } else { "missing".to_string() }),
        "query_index_kind": vector_string(metadata, root, &[&["query_index_kind"]])
            .unwrap_or_else(|| if query_index_exists { "sqlite".to_string() } else { "none".to_string() }),
        "query_index_path": path_string(&query_index_path),
        "query_index_exists": query_index_exists,
        "query_index_bytes": query_index_bytes,
        "query_index_record_count": vector_u64(metadata, root, &[&["query_index_record_count"]]),
        "query_index_version": vector_string(metadata, root, &[&["query_index_version"]]),
        "query_index_bound_manifest_hash": vector_string(metadata, root, &[&["query_index_bound_manifest_hash"]]),
        "hot_path_contract": "normal candidate-spool query/status/context-pack uses this indexed sidecar, not a full JSONL scan",
    })
}

fn vector_chunk_byte_accounting(
    root: &Value,
    metadata: &Value,
    chunks: &[Value],
    artifact_bytes: u64,
    parsed_text_bytes: u64,
) -> Value {
    let indexed_chunk_text_bytes = chunks
        .iter()
        .filter_map(|chunk| chunk.get("text").and_then(Value::as_str))
        .map(|text| text.as_bytes().len() as u64)
        .sum::<u64>();
    let selection_reason_bytes = chunks
        .iter()
        .filter_map(|chunk| chunk.get("selection_reason").and_then(Value::as_str))
        .map(|text| text.as_bytes().len() as u64)
        .sum::<u64>();
    let repeated_field_overhead = repeated_field_overhead_estimate(chunks);
    let estimated_f32_payload_bytes =
        vector_u64(metadata, root, &[&["estimated_f32_payload_bytes"]]).unwrap_or_else(|| {
            let dims = vector_u64(
                metadata,
                root,
                &[&["provider", "dimension"], &["dims"], &["dim"]],
            )
            .unwrap_or(0);
            dims.saturating_mul(4).saturating_mul(chunks.len() as u64)
        });
    let metadata_estimated_bytes = artifact_bytes.saturating_sub(indexed_chunk_text_bytes);
    let audit_overhead_bytes =
        selection_reason_bytes.saturating_add(selection_field_overhead(chunks));
    json!({
        "actual_index_file_bytes": artifact_bytes,
        "parsed_artifact_text_bytes": parsed_text_bytes,
        "estimated_f32_payload_bytes": estimated_f32_payload_bytes,
        "indexed_chunk_text_bytes": indexed_chunk_text_bytes,
        "metadata_estimated_bytes": metadata_estimated_bytes,
        "selection_reason_bytes": selection_reason_bytes,
        "repeated_field_overhead_estimate": repeated_field_overhead,
        "artifact_to_payload_ratio": if estimated_f32_payload_bytes == 0 {
            Value::Null
        } else {
            json!(round3(artifact_bytes as f64 / estimated_f32_payload_bytes as f64))
        },
        "chunk_text_share_percent": percent(indexed_chunk_text_bytes, artifact_bytes),
        "metadata_share_percent": percent(metadata_estimated_bytes, artifact_bytes),
        "audit_overhead_share_percent": percent(audit_overhead_bytes, artifact_bytes),
        "byte_accounting_method": "read_only_artifact_bytes_minus_stored_chunk_text; metadata and repeated field overhead are estimates",
    })
}

fn annotate_vector_artifact_byte_accounting(
    byte_accounting: &mut Value,
    artifact_kind: &str,
    artifact_bytes: u64,
) {
    if let Some(object) = byte_accounting.as_object_mut() {
        object.insert(
            "runtime_sidecar_bytes".to_string(),
            if artifact_kind == "vector_runtime_sidecar" {
                json!(artifact_bytes)
            } else {
                Value::Null
            },
        );
        object.insert(
            "audit_artifact_bytes".to_string(),
            if artifact_kind == "audit_artifact" {
                json!(artifact_bytes)
            } else {
                Value::Null
            },
        );
        object.insert("pretty_json_overhead".to_string(), Value::Null);
    }
}

fn vector_related_artifacts(
    options: &VectorChunksOptions,
    inspected_artifact_path: &Path,
    inspected_artifact_kind: &str,
) -> Value {
    let runtime_path = options.runtime_sidecar_path.as_ref().cloned().or_else(|| {
        (inspected_artifact_kind == "vector_runtime_sidecar")
            .then(|| inspected_artifact_path.to_path_buf())
    });
    let audit_path = options.audit_artifact_path.as_ref().cloned().or_else(|| {
        (inspected_artifact_kind == "audit_artifact").then(|| inspected_artifact_path.to_path_buf())
    });
    let runtime = runtime_path
        .as_ref()
        .map(|path| vector_artifact_brief(path))
        .unwrap_or_else(|| {
            json!({
                "path": Value::Null,
                "exists": false,
                "artifact_kind": Value::Null,
                "artifact_format": Value::Null,
                "bytes": Value::Null,
                "chunk_count": Value::Null,
            })
        });
    let audit = audit_path
        .as_ref()
        .map(|path| vector_artifact_brief(path))
        .unwrap_or_else(|| {
            json!({
                "path": Value::Null,
                "exists": false,
                "artifact_kind": Value::Null,
                "artifact_format": Value::Null,
                "bytes": Value::Null,
                "chunk_count": Value::Null,
            })
        });
    let runtime_bytes = runtime.get("bytes").and_then(Value::as_u64);
    let audit_bytes = audit.get("bytes").and_then(Value::as_u64);
    let overhead_reduction_bytes = match (audit_bytes, runtime_bytes) {
        (Some(audit), Some(runtime)) if audit >= runtime => Some(audit - runtime),
        _ => None,
    };
    json!({
        "runtime_will_use": if runtime.get("artifact_kind").and_then(Value::as_str) == Some("vector_runtime_sidecar") {
            runtime.get("path").cloned().unwrap_or(Value::Null)
        } else if inspected_artifact_kind == "legacy_pretty_json_vector_artifact" {
            json!(path_string(inspected_artifact_path))
        } else {
            Value::Null
        },
        "runtime_sidecar": runtime,
        "audit_artifact": audit,
        "audit_required_for_runtime": false,
        "runtime_prefers_compact_sidecar": true,
        "overhead_reduction_bytes": overhead_reduction_bytes.map(Value::from).unwrap_or(Value::Null),
        "overhead_reduction_percent": match (overhead_reduction_bytes, audit_bytes) {
            (Some(saved), Some(audit)) if audit > 0 => json!(percent(saved, audit)),
            _ => Value::Null,
        },
    })
}

fn vector_artifact_brief(path: &Path) -> Value {
    let exists = path.exists();
    let bytes = fs::metadata(path).map(|metadata| metadata.len()).ok();
    if !exists {
        return json!({
            "path": path_string(path),
            "exists": false,
            "artifact_kind": "missing",
            "artifact_format": "missing",
            "bytes": Value::Null,
            "chunk_count": Value::Null,
        });
    }
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) => {
            return json!({
                "path": path_string(path),
                "exists": true,
                "artifact_kind": "unknown",
                "artifact_format": "unknown",
                "bytes": bytes,
                "chunk_count": Value::Null,
                "error": format!("read failed: {error}"),
            })
        }
    };
    let format = detect_vector_artifact_format(path, &raw);
    let text = match std::str::from_utf8(&raw) {
        Ok(text) => text,
        Err(error) => {
            return json!({
                "path": path_string(path),
                "exists": true,
                "artifact_kind": "unknown",
                "artifact_format": format,
                "bytes": bytes,
                "chunk_count": Value::Null,
                "error": format!("utf8 decode failed: {error}"),
            })
        }
    };
    match parse_vector_artifact_text(text, &format) {
        Ok(parsed) => {
            let kind =
                classify_vector_artifact_kind(&parsed.root, &parsed.metadata, &parsed.chunks);
            json!({
                "path": path_string(path),
                "exists": true,
                "artifact_kind": kind,
                "artifact_format": parsed.artifact_format,
                "bytes": bytes,
                "chunk_count": parsed.chunks.len(),
                "selection_reason_bytes": parsed.chunks.iter()
                    .filter_map(|chunk| chunk.get("selection_reason").and_then(Value::as_str))
                    .map(|text| text.as_bytes().len() as u64)
                    .sum::<u64>(),
            })
        }
        Err(error) => json!({
            "path": path_string(path),
            "exists": true,
            "artifact_kind": "unknown",
            "artifact_format": format,
            "bytes": bytes,
            "chunk_count": Value::Null,
            "error": error,
        }),
    }
}

fn repeated_field_overhead_estimate(chunks: &[Value]) -> u64 {
    chunks
        .iter()
        .filter_map(Value::as_object)
        .flat_map(|object| object.keys())
        .map(|key| key.as_bytes().len() as u64 + 3)
        .sum()
}

fn selection_field_overhead(chunks: &[Value]) -> u64 {
    chunks
        .iter()
        .filter_map(Value::as_object)
        .map(|object| {
            [
                "selection_score",
                "selection_bucket",
                "selection_reason",
                "cap_stage",
            ]
            .iter()
            .filter(|field| object.contains_key(**field))
            .map(|field| field.len() as u64 + 3)
            .sum::<u64>()
        })
        .sum()
}

fn vector_provider_lifecycle(
    repo_path: Option<&Path>,
    db_path: Option<&Path>,
    root: &Value,
    metadata: &Value,
    chunks: &[Value],
    artifact_hash_before: &Option<String>,
) -> Value {
    let provider = metadata
        .get("provider")
        .or_else(|| root.get("provider"))
        .unwrap_or(&Value::Null);
    let passport = metadata
        .get("passport")
        .or_else(|| root.get("passport"))
        .or_else(|| root.get("db_passport"))
        .unwrap_or(&Value::Null);
    let db_binding = db_path
        .map(|path| vector_db_binding(path, passport))
        .unwrap_or_else(|| {
            json!({
                "status": "missing",
                "reason": "no --db path supplied; DB passport compatibility was not checked",
            })
        });
    let validity_status = db_binding["status"].as_str().unwrap_or("missing");
    let reason = db_binding["reason"].as_str().unwrap_or("unknown");
    let source_binding = vector_source_binding_status(repo_path, chunks);
    let source_status = source_binding
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("missing");
    let source_reason = source_binding
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("source bindings were not checked");
    let combined_status = if matches!(source_status, "stale" | "foreign" | "corrupt") {
        source_status
    } else {
        validity_status
    };
    let combined_reason = if matches!(source_status, "stale" | "foreign" | "corrupt") {
        source_reason
    } else {
        reason
    };
    json!({
        "db_passport_hash": passport.get("passport_fingerprint")
            .and_then(Value::as_str)
            .map(str::to_string),
        "repo_hash": passport.get("repo_head")
            .and_then(Value::as_str)
            .map(str::to_string),
        "scope_hash": passport.get("index_scope_policy_hash")
            .or_else(|| metadata.get("scope_hash"))
            .and_then(Value::as_str)
            .map(str::to_string),
        "provider_name": provider.get("provider_id")
            .or_else(|| provider.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        "model": provider.get("model_id")
            .or_else(|| provider.get("model"))
            .and_then(Value::as_str)
            .map(str::to_string),
        "dims": provider.get("dimension")
            .or_else(|| provider.get("dims"))
            .or_else(|| provider.get("dim"))
            .and_then(Value::as_u64),
        "extraction_version": metadata.get("extraction_version")
            .or_else(|| root.get("extraction_version"))
            .and_then(Value::as_str)
            .map(str::to_string),
        "source_scope": metadata.get("source_scope")
            .or_else(|| root.get("source_scope"))
            .and_then(Value::as_str)
            .map(str::to_string),
        "artifact_hash": artifact_hash_before,
        "validity_status": combined_status,
        "reason": combined_reason,
        "stale_valid_foreign_missing_corrupt": combined_status,
        "db_binding": db_binding,
        "source_binding": source_binding,
    })
}

fn vector_source_binding_status(repo_path: Option<&Path>, chunks: &[Value]) -> Value {
    let Some(repo_path) = repo_path else {
        return json!({
            "status": "missing",
            "reason": "no --repo path supplied; source file bindings were not checked",
            "checked_files": 0,
            "checked_chunks": 0,
            "unbound_chunks": chunks.len(),
            "stale_reasons": [],
        });
    };
    let repo_root = match fs::canonicalize(repo_path) {
        Ok(path) => path,
        Err(error) => {
            return json!({
                "status": "foreign",
                "reason": format!("repo path could not be canonicalized: {error}"),
                "checked_files": 0,
                "checked_chunks": 0,
                "unbound_chunks": chunks.len(),
                "stale_reasons": [format!("repo_path_failed:{error}")],
            });
        }
    };
    let mut by_path = BTreeMap::<String, (Option<String>, Option<u64>, usize)>::new();
    let mut unbound_chunks = 0usize;
    for chunk in chunks {
        let Some(path) = chunk.get("path").and_then(Value::as_str) else {
            unbound_chunks += 1;
            continue;
        };
        let expected_hash = chunk
            .get("source_file_content_hash")
            .and_then(Value::as_str)
            .map(str::to_string);
        let expected_size = chunk.get("source_file_size_bytes").and_then(Value::as_u64);
        if expected_hash.is_none() && expected_size.is_none() {
            unbound_chunks += 1;
            continue;
        }
        let binding =
            by_path
                .entry(path.replace('\\', "/"))
                .or_insert((expected_hash, expected_size, 0));
        binding.2 += 1;
    }
    let checked_files = by_path.len();
    let mut checked_chunks = 0usize;
    let mut stale_reasons = Vec::<String>::new();
    for (path, (expected_hash, expected_size, chunk_count)) in by_path {
        checked_chunks += chunk_count;
        let source_path = repo_root.join(&path);
        let metadata = match fs::metadata(&source_path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                stale_reasons.push(format!("not_file:{path}"));
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                stale_reasons.push(format!("deleted_file:{path}"));
                continue;
            }
            Err(error) => {
                stale_reasons.push(format!("metadata_failed:{path}:{error}"));
                continue;
            }
        };
        if let Some(expected_size) = expected_size {
            if metadata.len() != expected_size {
                stale_reasons.push(format!(
                    "changed_file_size:{path}:expected={expected_size}:actual={}",
                    metadata.len()
                ));
                continue;
            }
        }
        if let Some(expected_hash) = expected_hash.as_deref() {
            match fs::read_to_string(&source_path) {
                Ok(source) => {
                    let actual_hash = content_hash(&source);
                    if actual_hash != expected_hash {
                        stale_reasons.push(format!("changed_file_hash:{path}"));
                    }
                }
                Err(error) => stale_reasons.push(format!("source_read_failed:{path}:{error}")),
            }
        }
    }
    stale_reasons.sort();
    stale_reasons.dedup();
    json!({
        "status": if stale_reasons.is_empty() { "valid" } else { "stale" },
        "reason": if stale_reasons.is_empty() {
            "source file bindings match current repo files".to_string()
        } else {
            format!("source file bindings stale: {}", stale_reasons.join("; "))
        },
        "checked_files": checked_files,
        "checked_chunks": checked_chunks,
        "unbound_chunks": unbound_chunks,
        "stale_reasons": stale_reasons,
    })
}

fn vector_db_binding_without_artifact(db_path: &Path) -> Value {
    vector_db_binding(db_path, &Value::Null)
}

fn vector_db_binding(db_path: &Path, artifact_passport: &Value) -> Value {
    let before = db_file_snapshot(db_path);
    if !db_path.exists() {
        return json!({
            "db_path": path_string(db_path),
            "db_exists": false,
            "status": "missing",
            "reason": "DB path does not exist",
            "db_hash_before": before.main_db_hash,
            "db_hash_after": stable_file_hash(db_path),
            "db_mutated_during_inspection": false,
        });
    }
    let read_only = match open_read_only_with_snapshot(db_path, &before) {
        Ok(read_only) => read_only,
        Err(error) => {
            let after = db_file_snapshot(db_path);
            return json!({
                "db_path": path_string(db_path),
                "db_exists": true,
                "status": "corrupt",
                "reason": format!("failed to open DB read-only: {error}"),
                "db_hash_before": before.main_db_hash,
                "db_hash_after": after.main_db_hash,
                "db_mutated_during_inspection": before.main_db_hash != after.main_db_hash || before.main_db_mtime_unix_ms != after.main_db_mtime_unix_ms,
            });
        }
    };
    let passport = match read_vector_inspector_db_passport(&read_only.connection) {
        Ok(Some(passport)) => passport,
        Ok(None) => {
            let after = db_file_snapshot(db_path);
            return json!({
                "db_path": path_string(db_path),
                "db_exists": true,
                "status": "missing",
                "reason": "codegraph_db_passport row is missing",
                "read_only_mode_used": read_only.read_only_mode_used,
                "immutable_mode_used": read_only.immutable_mode_used,
                "db_hash_before": before.main_db_hash,
                "db_hash_after": after.main_db_hash,
                "db_mutated_during_inspection": before.main_db_hash != after.main_db_hash || before.main_db_mtime_unix_ms != after.main_db_mtime_unix_ms,
            });
        }
        Err(error) => {
            let after = db_file_snapshot(db_path);
            return json!({
                "db_path": path_string(db_path),
                "db_exists": true,
                "status": "corrupt",
                "reason": error,
                "read_only_mode_used": read_only.read_only_mode_used,
                "immutable_mode_used": read_only.immutable_mode_used,
                "db_hash_before": before.main_db_hash,
                "db_hash_after": after.main_db_hash,
                "db_mutated_during_inspection": before.main_db_hash != after.main_db_hash || before.main_db_mtime_unix_ms != after.main_db_mtime_unix_ms,
            });
        }
    };
    let passport_json = serde_json::to_string(&passport).unwrap_or_default();
    let db_passport_hash = content_hash(&passport_json);
    let artifact_passport_hash = artifact_passport
        .get("passport_fingerprint")
        .and_then(Value::as_str);
    let artifact_repo_root = artifact_passport
        .get("canonical_repo_root")
        .and_then(Value::as_str);
    let artifact_scope_hash = artifact_passport
        .get("index_scope_policy_hash")
        .and_then(Value::as_str);
    let artifact_repo_head = artifact_passport.get("repo_head").and_then(Value::as_str);
    let (status, reason) = if artifact_passport.is_null() {
        (
            "missing",
            "artifact does not include DB passport binding".to_string(),
        )
    } else if artifact_passport_hash == Some(db_passport_hash.as_str()) {
        ("valid", "artifact DB passport hash matches DB".to_string())
    } else if artifact_repo_root.is_some_and(|value| value != passport.canonical_repo_root) {
        (
            "foreign",
            "artifact canonical_repo_root differs from DB".to_string(),
        )
    } else if artifact_scope_hash.is_some_and(|value| value != passport.index_scope_policy_hash) {
        ("stale", "artifact scope hash differs from DB".to_string())
    } else if artifact_repo_head != passport.repo_head.as_deref() {
        ("stale", "artifact repo_head differs from DB".to_string())
    } else {
        (
            "stale",
            "artifact DB passport hash differs from DB".to_string(),
        )
    };
    let after = db_file_snapshot(db_path);
    json!({
        "db_path": path_string(db_path),
        "db_exists": true,
        "status": status,
        "reason": reason,
        "db_passport_hash": db_passport_hash,
        "artifact_passport_hash": artifact_passport_hash,
        "db_passport_hash_matches_artifact": artifact_passport_hash == Some(db_passport_hash.as_str()),
        "canonical_repo_root": passport.canonical_repo_root,
        "repo_head": passport.repo_head,
        "scope_hash": passport.index_scope_policy_hash,
        "storage_mode": passport.storage_mode,
        "schema_version": passport.codegraph_schema_version,
        "read_only_mode_used": read_only.read_only_mode_used,
        "immutable_mode_used": read_only.immutable_mode_used,
        "db_hash_before": before.main_db_hash,
        "db_hash_after": after.main_db_hash,
        "db_mtime_before": before.main_db_mtime_unix_ms,
        "db_mtime_after": after.main_db_mtime_unix_ms,
        "db_mutated_during_inspection": before.main_db_hash != after.main_db_hash || before.main_db_mtime_unix_ms != after.main_db_mtime_unix_ms,
    })
}

fn vector_db_mutation_status(db_path: &Path) -> Value {
    let snapshot = db_file_snapshot(db_path);
    json!({
        "db_path": path_string(db_path),
        "db_exists": db_path.exists(),
        "hash_algorithm": "fnv1a64",
        "main_db_size": snapshot.main_db_size,
        "main_db_hash": snapshot.main_db_hash,
        "main_db_mtime_unix_ms": snapshot.main_db_mtime_unix_ms,
        "sidecars": snapshot.sidecars,
    })
}

fn read_vector_inspector_db_passport(
    connection: &Connection,
) -> Result<Option<VectorInspectorDbPassport>, String> {
    if !table_exists(connection, "codegraph_db_passport")? {
        return Ok(None);
    }
    connection
        .query_row(
            "SELECT passport_version, codegraph_schema_version, storage_mode, index_scope_policy_hash, scope_policy_json, canonical_repo_root, git_remote, worktree_root, repo_head, source_discovery_policy_version, codegraph_build_version, last_successful_index_timestamp, last_completed_run_id, last_run_status, integrity_gate_result, files_seen, files_indexed, created_at_unix_ms, updated_at_unix_ms FROM codegraph_db_passport WHERE id = 1",
            [],
            |row| {
                Ok(VectorInspectorDbPassport {
                    passport_version: row.get::<_, u32>(0)?,
                    codegraph_schema_version: row.get::<_, u32>(1)?,
                    storage_mode: row.get(2)?,
                    index_scope_policy_hash: row.get(3)?,
                    scope_policy_json: row.get(4)?,
                    canonical_repo_root: row.get(5)?,
                    git_remote: row.get(6)?,
                    worktree_root: row.get(7)?,
                    repo_head: row.get(8)?,
                    source_discovery_policy_version: row.get(9)?,
                    codegraph_build_version: row.get(10)?,
                    last_successful_index_timestamp: row.get::<_, Option<i64>>(11)?.map(|value| value.max(0) as u64),
                    last_completed_run_id: row.get(12)?,
                    last_run_status: row.get(13)?,
                    integrity_gate_result: row.get(14)?,
                    files_seen: row.get::<_, i64>(15)?.max(0) as u64,
                    files_indexed: row.get::<_, i64>(16)?.max(0) as u64,
                    created_at_unix_ms: row.get::<_, i64>(17)?.max(0) as u64,
                    updated_at_unix_ms: row.get::<_, i64>(18)?.max(0) as u64,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn vector_chunk_samples(chunks: &[Value], sample_limit: usize) -> Vec<Value> {
    chunks
        .iter()
        .take(sample_limit)
        .map(|chunk| {
            let text = chunk
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let (preview, truncated) = truncate_preview(text, VECTOR_CHUNK_TEXT_PREVIEW_CHARS);
            json!({
                "chunk_id": chunk_string_value(chunk, "chunk_id"),
                "chunk_kind": chunk_string_value(chunk, "chunk_kind"),
                "source_kind": chunk_string_value(chunk, "source_kind"),
                "path": chunk_string_value(chunk, "path"),
                "source_span": chunk.get("source_span").cloned().unwrap_or(Value::Null),
                "entity_id": chunk.get("entity_id").cloned().unwrap_or(Value::Null),
                "proof_status": chunk_string_value(chunk, "proof_status"),
                "graph_proof": chunk_bool(chunk, "graph_proof").unwrap_or(false),
                "claimable_for_graph": chunk_bool(chunk, "claimable_for_graph").unwrap_or(false),
                "requires_graph_verification": chunk_requires_graph_verification(chunk),
                "text_preview": preview,
                "text_preview_truncated": truncated,
                "text_bytes": text.as_bytes().len(),
                "selection_score": chunk.get("selection_score").cloned().unwrap_or(Value::Null),
                "selection_bucket": chunk.get("selection_bucket").cloned().unwrap_or(Value::Null),
                "selection_reason": chunk.get("selection_reason").cloned().unwrap_or(Value::Null),
                "source_file_content_hash": chunk.get("source_file_content_hash").cloned().unwrap_or(Value::Null),
                "source_file_size_bytes": chunk.get("source_file_size_bytes").cloned().unwrap_or(Value::Null),
                "source_file_modified_unix_nanos": chunk.get("source_file_modified_unix_nanos").cloned().unwrap_or(Value::Null),
                "lifecycle_binding": chunk.get("lifecycle_binding")
                    .or_else(|| chunk.get("lifecycle"))
                    .cloned()
                    .unwrap_or(Value::Null),
            })
        })
        .collect()
}

fn vector_safety_conclusions(root: &Value, metadata: &Value, chunks: &[Value]) -> Value {
    let stores_embedding_vectors = root.get("vectors").is_some()
        || root.get("embeddings").is_some()
        || chunks.iter().any(chunk_has_embedding_vector);
    let stores_chunk_text = vector_bool(metadata, root, &[&["stores_chunk_text"]])
        .unwrap_or_else(|| chunks.iter().any(|chunk| chunk.get("text").is_some()));
    let stores_chunk_metadata = vector_bool(metadata, root, &[&["stores_chunk_metadata"]])
        .unwrap_or_else(|| !chunks.is_empty() || metadata.is_object());
    let stores_full_source_body =
        vector_bool(metadata, root, &[&["stores_full_source_body"]]).unwrap_or(false);
    let graph_proof_true = chunks
        .iter()
        .filter(|chunk| chunk_bool(chunk, "graph_proof").unwrap_or(false))
        .count();
    let claimable_true = chunks
        .iter()
        .filter(|chunk| chunk_bool(chunk, "claimable_for_graph").unwrap_or(false))
        .count();
    json!({
        "stores_embedding_vectors": stores_embedding_vectors,
        "stores_chunk_text": stores_chunk_text,
        "stores_chunk_metadata": stores_chunk_metadata,
        "stores_full_source_body": stores_full_source_body,
        "creates_graph_relations": false,
        "can_answer_graph_proof": false,
        "candidate_only": graph_proof_true == 0 && claimable_true == 0,
        "graph_proof_true_chunks": graph_proof_true,
        "claimable_for_graph_true_chunks": claimable_true,
        "deterministic_embeddings_regenerated_from_chunk_text": !stores_embedding_vectors && stores_chunk_text,
        "embedding_reconstruction": if !stores_embedding_vectors && stores_chunk_text {
            "stored chunk.text is the deterministic embedding input; vectors are regenerated at runtime"
        } else if stores_embedding_vectors {
            "artifact stores embedding/vector arrays"
        } else {
            "embedding input unavailable in artifact"
        },
        "full_source_body_detection_method": "explicit metadata flag only; samples expose truncated text_preview, never full chunk text",
    })
}

fn chunk_has_embedding_vector(chunk: &Value) -> bool {
    ["embedding", "vector", "embedding_vector"]
        .iter()
        .any(|field| chunk.get(*field).and_then(Value::as_array).is_some())
}

fn render_vector_chunks_markdown(report: &Value) -> String {
    let identity = &report["artifact_identity"];
    let counts = &report["chunk_counts"];
    let bytes = &report["byte_accounting"];
    let query_index = &report["query_index"];
    let provider = &report["passport_lifecycle_provider"];
    let safety = &report["safety_conclusions"];
    let mut output = String::new();
    output.push_str("# Vector Chunk Artifact Inspector\n\n");
    output.push_str("Diagnostic-only read-only artifact inspection. This report does not create graph proof; runtime sidecars remain candidate-only and audit artifacts remain diagnostic-only.\n\n");
    output.push_str("## Artifact Identity\n\n");
    output.push_str("| Field | Value |\n| --- | --- |\n");
    for (field, value) in [
        ("status", report["status"].clone()),
        ("artifact_path", identity["artifact_path"].clone()),
        ("artifact_exists", identity["artifact_exists"].clone()),
        ("artifact_kind", identity["artifact_kind"].clone()),
        ("artifact_format", identity["artifact_format"].clone()),
        ("artifact_bytes", identity["artifact_bytes"].clone()),
        ("compression_status", identity["compression_status"].clone()),
        ("provider", provider["provider_name"].clone()),
        ("model", provider["model"].clone()),
        ("dims", provider["dims"].clone()),
        ("validity_status", provider["validity_status"].clone()),
        ("reason", provider["reason"].clone()),
    ] {
        output.push_str(&format!("| `{}` | `{}` |\n", field, markdown_value(&value)));
    }
    output.push_str("\n## Chunk Counts\n\n");
    output.push_str("| Count | Value |\n| --- | ---: |\n");
    for field in [
        "generated_total_chunks",
        "spooled_total_chunks",
        "selected_total_chunks",
        "persisted_total_chunks",
        "runtime_total_chunks",
        "audit_total_chunks",
        "omitted_by_cap",
        "omitted_low_signal",
        "omitted_by_bucket_limit",
        "omitted_by_dedup",
    ] {
        output.push_str(&format!(
            "| `{}` | {} |\n",
            field,
            markdown_value(&counts[field])
        ));
    }
    output.push_str("\n## Byte Accounting\n\n");
    output.push_str("| Metric | Value |\n| --- | ---: |\n");
    for field in [
        "actual_index_file_bytes",
        "runtime_sidecar_bytes",
        "audit_artifact_bytes",
        "pretty_json_overhead",
        "estimated_f32_payload_bytes",
        "indexed_chunk_text_bytes",
        "metadata_estimated_bytes",
        "selection_reason_bytes",
        "repeated_field_overhead_estimate",
        "artifact_to_payload_ratio",
        "chunk_text_share_percent",
        "metadata_share_percent",
        "audit_overhead_share_percent",
    ] {
        output.push_str(&format!(
            "| `{}` | {} |\n",
            field,
            markdown_value(&bytes[field])
        ));
    }
    output.push_str("\n## Query Index\n\n");
    output.push_str("| Field | Value |\n| --- | --- |\n");
    for field in [
        "query_index_status",
        "query_index_kind",
        "query_index_path",
        "query_index_exists",
        "query_index_bytes",
        "query_index_record_count",
        "query_index_version",
        "query_index_bound_manifest_hash",
    ] {
        output.push_str(&format!(
            "| `{}` | `{}` |\n",
            field,
            markdown_value(&query_index[field])
        ));
    }
    output.push_str("\n## Safety Conclusions\n\n");
    output.push_str("| Field | Value |\n| --- | --- |\n");
    for field in [
        "stores_embedding_vectors",
        "stores_chunk_text",
        "stores_chunk_metadata",
        "stores_full_source_body",
        "creates_graph_relations",
        "can_answer_graph_proof",
        "candidate_only",
        "deterministic_embeddings_regenerated_from_chunk_text",
    ] {
        output.push_str(&format!(
            "| `{}` | `{}` |\n",
            field,
            markdown_value(&safety[field])
        ));
    }
    output.push_str("\n## Sample Chunks\n\n");
    output.push_str("| # | chunk_id | kind | source | path | proof_status | text_bytes | truncated | preview |\n");
    output.push_str("| ---: | --- | --- | --- | --- | --- | ---: | --- | --- |\n");
    if let Some(samples) = report["sample_chunks"].as_array() {
        for (index, sample) in samples.iter().enumerate() {
            output.push_str(&format!(
                "| {} | `{}` | `{}` | `{}` | `{}` | `{}` | {} | `{}` | {} |\n",
                index + 1,
                markdown_escape(sample["chunk_id"].as_str().unwrap_or("unknown")),
                markdown_escape(sample["chunk_kind"].as_str().unwrap_or("unknown")),
                markdown_escape(sample["source_kind"].as_str().unwrap_or("unknown")),
                markdown_escape(sample["path"].as_str().unwrap_or("unknown")),
                markdown_escape(sample["proof_status"].as_str().unwrap_or("unknown")),
                sample["text_bytes"].as_u64().unwrap_or(0),
                sample["text_preview_truncated"].as_bool().unwrap_or(false),
                markdown_escape(sample["text_preview"].as_str().unwrap_or_default()),
            ));
        }
    }
    output.push_str("\n## Claim Boundary\n\n");
    output.push_str("Vector chunks are candidate-only. Entity-linked vector hits require graph/source verification before any graph-proof claim.\n");
    output
}

fn markdown_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::String(value) => markdown_escape(value),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| "unknown".to_string()),
    }
}

fn markdown_escape(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn push_limited_value(items: &mut Vec<Value>, value: Value, limit: usize) {
    if items.len() < limit {
        items.push(value);
    }
}

fn relative_scope_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn top_level_component(relative: &str) -> String {
    relative
        .split('/')
        .find(|component| !component.is_empty())
        .unwrap_or("(repo_root)")
        .to_string()
}

fn parent_directory(relative: &str) -> String {
    relative
        .rsplit_once('/')
        .map(|(parent, _)| {
            if parent.is_empty() {
                "(repo_root)".to_string()
            } else {
                parent.to_string()
            }
        })
        .unwrap_or_else(|| "(repo_root)".to_string())
}

fn extension_key(relative: &str) -> String {
    let file_name = Path::new(relative)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(relative);
    if file_name == "Makefile" {
        return "Makefile".to_string();
    }
    if file_name == "Kconfig" || file_name.starts_with("Kconfig.") {
        return "Kconfig".to_string();
    }
    if file_name.starts_with("Config.in") {
        return "Config.in".to_string();
    }
    Path::new(relative)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{}", extension.to_ascii_lowercase()))
        .unwrap_or_else(|| "(none)".to_string())
}

fn suspicious_components(relative: &str) -> Vec<String> {
    const SUSPICIOUS: &[&str] = &[
        "output",
        "dl",
        "build",
        "dist",
        "generated",
        "out",
        "vendor",
        "third_party",
        "reports",
        "target",
        "node_modules",
        ".git",
        ".codegraph",
    ];
    let mut observed = Vec::new();
    for component in relative.split('/') {
        if SUSPICIOUS.contains(&component) {
            observed.push(component.to_string());
        }
    }
    observed
}

fn run_storage_command(args: &[String]) -> Result<Value, String> {
    let options = parse_storage_options(args)?;
    let report = inspect_storage(&options.db_path)?;
    let markdown = render_storage_markdown(&report);
    write_optional_outputs(
        &report,
        &markdown,
        &options.json_path,
        &options.markdown_path,
    )?;
    Ok(json!({
        "status": "ok",
        "audit": "storage",
        "db_path": report.db_path,
        "dbstat_available": report.dbstat_available,
        "database_bytes": report.file_family.database_bytes,
        "file_family_bytes": report.file_family.total_bytes,
        "object_count": report.objects.len(),
        "edge_count": report.aggregate_metrics.edge_count,
        "average_database_bytes_per_edge": report.aggregate_metrics.average_database_bytes_per_edge,
        "index_count": report.index_usage.len(),
        "core_query_plan_count": report.core_query_plans.len(),
        "json_output": options.json_path.as_ref().map(path_string),
        "markdown_output": options.markdown_path.as_ref().map(path_string),
    }))
}

fn run_storage_micro_command(args: &[String]) -> Result<Value, String> {
    let options = parse_storage_micro_options(args)?;
    let report = run_storage_micro(&options)?;
    let markdown = render_storage_micro_markdown(&report);
    let json_path = options
        .json_path
        .clone()
        .unwrap_or_else(|| PathBuf::from(report["reports"]["json"].as_str().unwrap_or_default()));
    let markdown_path = options.markdown_path.clone().unwrap_or_else(|| {
        PathBuf::from(report["reports"]["markdown"].as_str().unwrap_or_default())
    });
    write_json(&json_path, &report)?;
    write_text(&markdown_path, &markdown)?;
    Ok(json!({
        "status": "ok",
        "audit": "storage_micro",
        "command_namespace": "audit",
        "run_dir": report["run_dir"],
        "json_output": path_string(&json_path),
        "markdown_output": path_string(&markdown_path),
        "case_count": report["cases"].as_array().map(Vec::len).unwrap_or(0),
        "artifacts_kept": report["artifacts_kept"],
        "normal_codegraph_db_created": report["safety"]["normal_codegraph_db_created"],
        "dbstat_available_all_cases": report["dbstat_available_all_cases"],
        "context_pack_ran": report["context_pack_checks"]["ran"],
        "storage_budget": report["storage_budget"].clone(),
    }))
}

fn run_schema_check_command(args: &[String]) -> Result<Value, String> {
    let options = parse_schema_check_options(args)?;
    let report = validate_schema(&options.db_path)?;
    let markdown = render_schema_validation_markdown(&report);
    write_optional_outputs(
        &report,
        &markdown,
        &options.json_path,
        &options.markdown_path,
    )?;
    Ok(json!({
        "status": report.status,
        "audit": "schema_check",
        "db_path": report.db_path,
        "user_version": report.user_version,
        "failure_count": report.failures.len(),
        "json_output": options.json_path.as_ref().map(path_string),
        "markdown_output": options.markdown_path.as_ref().map(path_string),
    }))
}

fn run_storage_experiments_command(args: &[String]) -> Result<Value, String> {
    let options = parse_storage_experiment_options(args)?;
    let report = run_storage_experiments(&options)?;
    let markdown = render_storage_experiments_markdown(&report);
    write_optional_outputs(
        &report,
        &markdown,
        &options.json_path,
        &options.markdown_path,
    )?;
    Ok(json!({
        "status": "ok",
        "audit": "storage_experiments",
        "original_db_path": report.original_db_path,
        "run_dir": report.run_dir,
        "experiment_count": report.experiments.len(),
        "json_output": options.json_path.as_ref().map(path_string),
        "markdown_output": options.markdown_path.as_ref().map(path_string),
    }))
}

fn run_sample_edges_command(args: &[String]) -> Result<Value, String> {
    let options = parse_sample_edges_options(args)?;
    let report = sample_edges(&options)?;
    let markdown = render_edge_samples_markdown(&report);
    write_optional_outputs(
        &report,
        &markdown,
        &options.json_path,
        &options.markdown_path,
    )?;
    Ok(json!({
        "status": "ok",
        "audit": "sample_edges",
        "db_path": report.db_path,
        "relation_filter": report.relation_filter,
        "seed": report.seed,
        "limit": report.limit,
        "sample_count": report.samples.len(),
        "json_output": options.json_path.as_ref().map(path_string),
        "markdown_output": options.markdown_path.as_ref().map(path_string),
    }))
}

fn run_sample_paths_command(args: &[String]) -> Result<Value, String> {
    let options = parse_sample_paths_options(args)?;
    let report = sample_paths(&options)?;
    let markdown = render_path_samples_markdown(&report);
    write_optional_outputs(
        &report,
        &markdown,
        &options.json_path,
        &options.markdown_path,
    )?;
    Ok(json!({
        "status": "ok",
        "audit": "sample_paths",
        "db_path": report.db_path,
        "seed": report.seed,
        "limit": report.limit,
        "mode": report.mode.as_str(),
        "max_edge_load": report.max_edge_load,
        "timeout_ms": report.timeout_ms,
        "stored_path_count": report.stored_path_count,
        "candidate_path_count": report.candidate_path_count,
        "loaded_path_edge_count": report.loaded_path_edge_count,
        "generated_path_count": report.generated_path_count,
        "generated_fallback_used": report.generated_fallback_used,
        "elapsed_ms": report.timing.total_ms,
        "sample_count": report.samples.len(),
        "json_output": options.json_path.as_ref().map(path_string),
        "markdown_output": options.markdown_path.as_ref().map(path_string),
    }))
}

fn run_relation_counts_command(args: &[String]) -> Result<Value, String> {
    let options = parse_relation_counts_options(args)?;
    let report = relation_counts(&options.db_path)?;
    let markdown = render_relation_counts_markdown(&report);
    write_optional_outputs(
        &report,
        &markdown,
        &options.json_path,
        &options.markdown_path,
    )?;
    Ok(json!({
        "status": "ok",
        "audit": "relation_counts",
        "db_path": report.db_path,
        "relation_count": report.relation_count,
        "total_edges": report.total_edges,
        "json_output": options.json_path.as_ref().map(path_string),
        "markdown_output": options.markdown_path.as_ref().map(path_string),
    }))
}

fn run_label_samples_command(args: &[String]) -> Result<Value, String> {
    let options = parse_label_samples_options(args)?;
    let report = label_samples(&options)?;
    let markdown = render_label_samples_markdown(&report);
    write_optional_outputs(
        &report,
        &markdown,
        &options.json_path,
        &options.markdown_path,
    )?;
    Ok(json!({
        "status": "ok",
        "audit": "label_samples",
        "sample_count": report.samples.len(),
        "labeled_samples": report.summary.labeled_samples,
        "unlabeled_samples": report.summary.unlabeled_samples,
        "json_output": options.json_path.as_ref().map(path_string),
        "markdown_output": options.markdown_path.as_ref().map(path_string),
    }))
}

fn run_summarize_labels_command(args: &[String]) -> Result<Value, String> {
    let options = parse_summarize_labels_options(args)?;
    let report = summarize_label_inputs(&options)?;
    let markdown = render_label_samples_markdown(&report);
    write_optional_outputs(
        &report,
        &markdown,
        &options.json_path,
        &options.markdown_path,
    )?;
    Ok(json!({
        "status": "ok",
        "audit": "summarize_labels",
        "sample_count": report.samples.len(),
        "labeled_samples": report.summary.labeled_samples,
        "unlabeled_samples": report.summary.unlabeled_samples,
        "json_output": options.json_path.as_ref().map(path_string),
        "markdown_output": options.markdown_path.as_ref().map(path_string),
    }))
}

fn parse_index_scope_options(args: &[String]) -> Result<IndexScopeAuditOptions, String> {
    let mut repo = None;
    let mut json_path = None;
    let mut markdown_path = None;
    let mut scope = IndexScopeOptions::default();
    let mut include_included_examples = false;
    let mut include_excluded_examples = false;
    let mut example_limit = 25usize;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                if args
                    .get(index + 1)
                    .is_some_and(|value| !value.starts_with("--"))
                {
                    json_path = Some(take_path(args, &mut index, "--json")?);
                }
            }
            "--markdown" | "--md" => {
                markdown_path = Some(take_path(args, &mut index, "--markdown")?);
            }
            "--include-ignored" | "--include_ignored" => scope.include_ignored = true,
            "--no-default-excludes" | "--no_default_excludes" => scope.no_default_excludes = true,
            "--respect-gitignore" | "--respect_gitignore" => {
                let raw = take_value(args, &mut index, "--respect-gitignore")?;
                scope.respect_gitignore = parse_bool_value(&raw, "--respect-gitignore")?;
            }
            "--include" => {
                let raw = take_value(args, &mut index, "--include")?;
                scope.include_patterns.push(raw);
            }
            "--exclude" => {
                let raw = take_value(args, &mut index, "--exclude")?;
                scope.exclude_patterns.push(raw);
            }
            "--print-included" | "--print_included" => include_included_examples = true,
            "--print-excluded" | "--print_excluded" => include_excluded_examples = true,
            "--explain-scope" | "--explain_scope" => {
                include_included_examples = true;
                include_excluded_examples = true;
            }
            "--limit-examples" | "--limit_examples" => {
                let raw = take_value(args, &mut index, "--limit-examples")?;
                example_limit = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit-examples value: {raw}"))?;
            }
            "--help" | "-h" => return Err(index_scope_usage()),
            value if value.starts_with('-') => {
                return Err(format!("unknown audit index-scope option: {value}"));
            }
            value => {
                if repo.is_some() {
                    return Err(format!("unexpected audit index-scope argument: {value}"));
                }
                repo = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    let Some(repo) = repo else {
        return Err(index_scope_usage());
    };
    Ok(IndexScopeAuditOptions {
        repo,
        json_path,
        markdown_path,
        scope,
        include_included_examples,
        include_excluded_examples,
        example_limit,
    })
}

fn index_scope_usage() -> String {
    "Usage: codegraph-mcp audit index-scope <repo> [--json [path]] [--markdown <path>] [--include-ignored] [--include <pattern>] [--exclude <pattern>] [--no-default-excludes] [--respect-gitignore true|false] [--explain-scope] [--print-included] [--print-excluded] [--limit-examples <n>]".to_string()
}

fn parse_vector_chunks_options(args: &[String]) -> Result<VectorChunksOptions, String> {
    let mut options = VectorChunksOptions {
        artifact_path: None,
        runtime_sidecar_path: None,
        audit_artifact_path: None,
        db_path: None,
        repo: None,
        json_path: None,
        markdown_path: None,
        sample_limit: DEFAULT_VECTOR_CHUNK_SAMPLE_LIMIT,
    };
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--artifact" | "--vector-artifact" | "--vector_artifact" | "--vectors"
            | "--vector-index" | "--vector_index" => {
                options.artifact_path = Some(take_path(args, &mut index, "--artifact")?)
            }
            "--runtime-sidecar"
            | "--runtime_sidecar"
            | "--vector-runtime-sidecar"
            | "--vector_runtime_sidecar" => {
                options.runtime_sidecar_path =
                    Some(take_path(args, &mut index, "--runtime-sidecar")?)
            }
            "--audit-artifact"
            | "--audit_artifact"
            | "--vector-audit-artifact"
            | "--vector_audit_artifact" => {
                options.audit_artifact_path = Some(take_path(args, &mut index, "--audit-artifact")?)
            }
            "--db" => options.db_path = Some(take_path(args, &mut index, "--db")?),
            "--repo" => options.repo = Some(take_path(args, &mut index, "--repo")?),
            "--json" => {
                if args
                    .get(index + 1)
                    .is_some_and(|value| !value.starts_with("--"))
                {
                    options.json_path = Some(take_path(args, &mut index, "--json")?);
                }
            }
            "--markdown" | "--md" => {
                options.markdown_path = Some(take_path(args, &mut index, "--markdown")?)
            }
            "--sample" | "--limit" => {
                let raw = take_value(args, &mut index, "--sample")?;
                options.sample_limit = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --sample value: {raw}"))?;
            }
            "--help" | "-h" => return Err(vector_chunks_usage()),
            value => return Err(format!("unknown audit vector-chunks option: {value}")),
        }
        index += 1;
    }
    if options.artifact_path.is_none()
        && options.runtime_sidecar_path.is_none()
        && options.audit_artifact_path.is_none()
        && options.db_path.is_none()
        && options.repo.is_none()
    {
        return Err(vector_chunks_usage());
    }
    Ok(options)
}

fn vector_chunks_usage() -> String {
    "Usage: codegraph-mcp audit vector-chunks --artifact <path> [--db <path>] [--repo <path>] [--json [path]] [--markdown <path>] [--sample <n>]\n  codegraph-mcp audit vector-chunks --runtime-sidecar <path> [--audit-artifact <path>] [--db <path>] [--json [path]] [--sample <n>]\n  codegraph-mcp audit vector-chunks --db <path> [--vectors <path>] [--json [path]] [--sample <n>]".to_string()
}

fn resolve_vector_chunks_db_path(options: &VectorChunksOptions) -> Option<PathBuf> {
    options.db_path.clone().or_else(|| {
        options
            .repo
            .as_ref()
            .map(|repo| repo.join(".codegraph").join("codegraph.sqlite"))
    })
}

fn resolve_vector_chunks_artifact_path(
    options: &VectorChunksOptions,
    db_path: Option<&Path>,
) -> Result<PathBuf, String> {
    if let Some(path) = &options.artifact_path {
        return Ok(path.clone());
    }
    if let Some(path) = &options.runtime_sidecar_path {
        return Ok(path.clone());
    }
    if let Some(path) = &options.audit_artifact_path {
        return Ok(path.clone());
    }
    if let Some(db_path) = db_path {
        return Ok(db_path
            .parent()
            .map(|parent| parent.join(DEFAULT_VECTOR_CHUNKS_FILE_NAME))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_VECTOR_CHUNKS_FILE_NAME)));
    }
    Err(vector_chunks_usage())
}

fn parse_storage_options(args: &[String]) -> Result<StorageOptions, String> {
    let mut db_path = default_audit_db_path();
    let mut json_path = None;
    let mut markdown_path = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--db" => db_path = take_path(args, &mut index, "--db")?,
            "--json" => json_path = Some(take_path(args, &mut index, "--json")?),
            "--markdown" | "--md" => {
                markdown_path = Some(take_path(args, &mut index, "--markdown")?)
            }
            value => return Err(format!("unknown audit storage option: {value}")),
        }
        index += 1;
    }
    Ok(StorageOptions {
        db_path,
        json_path,
        markdown_path,
    })
}

fn parse_storage_micro_options(args: &[String]) -> Result<StorageMicroOptions, String> {
    let mut out_dir = None;
    let mut cases = storage_micro_all_cases();
    let mut batch_sizes = vec![1, 10, 100];
    let mut keep_artifacts = false;
    let mut json_path = None;
    let mut markdown_path = None;
    let mut no_context_pack = false;
    let mut respect_gitignore = true;
    let mut storage_budget = storage_budget::StorageBudgetOptions::fixture_smoke();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "--output-dir" => out_dir = Some(take_path(args, &mut index, "--out")?),
            "--cases" => {
                let raw = take_value(args, &mut index, "--cases")?;
                cases = parse_storage_micro_cases(&raw)?;
            }
            "--n" | "--batch-sizes" | "--batch_sizes" => {
                let raw = take_value(args, &mut index, "--batch-sizes")?;
                batch_sizes = parse_storage_micro_batch_sizes(&raw)?;
            }
            "--keep-artifacts" | "--keep_artifacts" => keep_artifacts = true,
            "--json" => {
                if args
                    .get(index + 1)
                    .is_some_and(|value| !value.starts_with("--"))
                {
                    json_path = Some(take_path(args, &mut index, "--json")?);
                }
            }
            "--markdown" | "--md" => {
                if args
                    .get(index + 1)
                    .is_some_and(|value| !value.starts_with("--"))
                {
                    markdown_path = Some(take_path(args, &mut index, "--markdown")?);
                }
            }
            "--no-context-pack" | "--no_context_pack" => no_context_pack = true,
            "--respect-gitignore" | "--respect_gitignore" => {
                let raw = take_value(args, &mut index, "--respect-gitignore")?;
                respect_gitignore = parse_bool_value(&raw, "--respect-gitignore")?;
            }
            "--help" | "-h" => {
                return Err(storage_micro_usage());
            }
            value => {
                if storage_budget::parse_storage_budget_flag(args, &mut index, &mut storage_budget)?
                {
                    index += 1;
                    continue;
                }
                return Err(format!("unknown audit storage-micro option: {value}"));
            }
        }
        index += 1;
    }
    let Some(out_dir) = out_dir else {
        return Err(storage_micro_usage());
    };
    Ok(StorageMicroOptions {
        out_dir,
        cases,
        batch_sizes,
        keep_artifacts,
        json_path,
        markdown_path,
        no_context_pack,
        respect_gitignore,
        storage_budget,
    })
}

fn storage_micro_usage() -> String {
    "Usage: codegraph-mcp audit storage-micro --out <dir> [--cases simple,expression,inline-tests,duplicates,excluded-junk,all] [--batch-sizes 1,10,100] [--keep-artifacts] [--json [path]] [--markdown [path]] [--no-context-pack] [--respect-gitignore true|false] [--max-db-mib <n>] [--max-artifacts-mib <n>] [--min-free-disk-gib <n>] [--extended] [--stress-corpus buildroot|linux]".to_string()
}

fn storage_micro_all_cases() -> BTreeSet<StorageMicroCaseKind> {
    BTreeSet::from([
        StorageMicroCaseKind::Simple,
        StorageMicroCaseKind::Expression,
        StorageMicroCaseKind::InlineTests,
        StorageMicroCaseKind::Duplicates,
        StorageMicroCaseKind::ExcludedJunk,
    ])
}

fn parse_storage_micro_cases(raw: &str) -> Result<BTreeSet<StorageMicroCaseKind>, String> {
    let mut cases = BTreeSet::new();
    for part in raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "all" => cases.extend(storage_micro_all_cases()),
            "simple" => {
                cases.insert(StorageMicroCaseKind::Simple);
            }
            "expression" | "expressions" | "expression-heavy" => {
                cases.insert(StorageMicroCaseKind::Expression);
            }
            "inline-tests" | "inline_tests" | "tests" => {
                cases.insert(StorageMicroCaseKind::InlineTests);
            }
            "duplicates" | "duplicate" => {
                cases.insert(StorageMicroCaseKind::Duplicates);
            }
            "excluded-junk" | "excluded_junk" | "junk" => {
                cases.insert(StorageMicroCaseKind::ExcludedJunk);
            }
            other => {
                return Err(format!(
                    "invalid --cases value: {other}; expected simple, expression, inline-tests, duplicates, excluded-junk, or all"
                ))
            }
        }
    }
    if cases.is_empty() {
        return Err("--cases must select at least one case".to_string());
    }
    Ok(cases)
}

fn parse_storage_micro_batch_sizes(raw: &str) -> Result<Vec<usize>, String> {
    let mut sizes = Vec::new();
    for part in raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let size = part
            .parse::<usize>()
            .map_err(|_| format!("invalid --batch-sizes value: {part}"))?;
        if size == 0 {
            return Err("--batch-sizes values must be greater than zero".to_string());
        }
        sizes.push(size);
    }
    sizes.sort_unstable();
    sizes.dedup();
    if sizes.is_empty() {
        return Err("--batch-sizes must contain at least one value".to_string());
    }
    Ok(sizes)
}

fn parse_bool_value(raw: &str, flag: &str) -> Result<bool, String> {
    match raw.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("{flag} requires true or false")),
    }
}

fn parse_schema_check_options(args: &[String]) -> Result<SchemaCheckOptions, String> {
    let mut db_path = default_audit_db_path();
    let mut json_path = None;
    let mut markdown_path = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--db" => db_path = take_path(args, &mut index, "--db")?,
            "--json" => json_path = Some(take_path(args, &mut index, "--json")?),
            "--markdown" | "--md" => {
                markdown_path = Some(take_path(args, &mut index, "--markdown")?)
            }
            value => return Err(format!("unknown audit schema-check option: {value}")),
        }
        index += 1;
    }
    Ok(SchemaCheckOptions {
        db_path,
        json_path,
        markdown_path,
    })
}

fn parse_storage_experiment_options(args: &[String]) -> Result<StorageExperimentOptions, String> {
    let mut options = StorageExperimentOptions {
        db_path: default_audit_db_path(),
        workdir: PathBuf::from("reports")
            .join("audit")
            .join("artifacts")
            .join("storage_experiments"),
        json_path: None,
        markdown_path: None,
        keep_copies: false,
    };
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--db" => options.db_path = take_path(args, &mut index, "--db")?,
            "--workdir" | "--copy-dir" => {
                options.workdir = take_path(args, &mut index, "--workdir")?
            }
            "--json" => options.json_path = Some(take_path(args, &mut index, "--json")?),
            "--markdown" | "--md" => {
                options.markdown_path = Some(take_path(args, &mut index, "--markdown")?)
            }
            "--keep-copies" => options.keep_copies = true,
            value => return Err(format!("unknown audit storage-experiments option: {value}")),
        }
        index += 1;
    }
    Ok(options)
}

fn parse_sample_edges_options(args: &[String]) -> Result<SampleEdgesOptions, String> {
    let mut options = SampleEdgesOptions {
        db_path: default_audit_db_path(),
        relation: None,
        limit: DEFAULT_SAMPLE_LIMIT,
        seed: DEFAULT_SAMPLE_SEED,
        json_path: None,
        markdown_path: None,
        include_snippets: false,
    };
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--db" => options.db_path = take_path(args, &mut index, "--db")?,
            "--relation" => {
                options.relation = Some(take_value(args, &mut index, "--relation")?.to_uppercase())
            }
            "--limit" => {
                let raw = take_value(args, &mut index, "--limit")?;
                options.limit = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit value: {raw}"))?;
            }
            "--seed" => {
                let raw = take_value(args, &mut index, "--seed")?;
                options.seed = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --seed value: {raw}"))?;
            }
            "--json" => options.json_path = Some(take_path(args, &mut index, "--json")?),
            "--markdown" | "--md" => {
                options.markdown_path = Some(take_path(args, &mut index, "--markdown")?)
            }
            "--include-snippets" => options.include_snippets = true,
            value => return Err(format!("unknown audit sample-edges option: {value}")),
        }
        index += 1;
    }
    if options.limit == 0 {
        return Err("--limit must be greater than zero".to_string());
    }
    Ok(options)
}

fn parse_sample_paths_options(args: &[String]) -> Result<SamplePathsOptions, String> {
    let mut options = SamplePathsOptions {
        db_path: default_audit_db_path(),
        limit: DEFAULT_PATH_SAMPLE_LIMIT,
        seed: DEFAULT_SAMPLE_SEED,
        json_path: None,
        markdown_path: None,
        include_snippets: false,
        max_edge_load: DEFAULT_PATH_SAMPLE_MAX_EDGE_LOAD,
        timeout_ms: DEFAULT_PATH_SAMPLE_TIMEOUT_MS,
        mode: PathSampleMode::Proof,
    };
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--db" => options.db_path = take_path(args, &mut index, "--db")?,
            "--limit" => {
                let raw = take_value(args, &mut index, "--limit")?;
                options.limit = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit value: {raw}"))?;
            }
            "--seed" => {
                let raw = take_value(args, &mut index, "--seed")?;
                options.seed = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --seed value: {raw}"))?;
            }
            "--json" => options.json_path = Some(take_path(args, &mut index, "--json")?),
            "--markdown" | "--md" => {
                options.markdown_path = Some(take_path(args, &mut index, "--markdown")?)
            }
            "--include-snippets" => options.include_snippets = true,
            "--max-edge-load" => {
                let raw = take_value(args, &mut index, "--max-edge-load")?;
                options.max_edge_load = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --max-edge-load value: {raw}"))?;
            }
            "--timeout-ms" => {
                let raw = take_value(args, &mut index, "--timeout-ms")?;
                options.timeout_ms = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}"))?;
            }
            "--mode" => {
                let raw = take_value(args, &mut index, "--mode")?;
                options.mode = PathSampleMode::parse(&raw)?;
            }
            value => return Err(format!("unknown audit sample-paths option: {value}")),
        }
        index += 1;
    }
    if options.limit == 0 {
        return Err("--limit must be greater than zero".to_string());
    }
    if options.max_edge_load == 0 {
        return Err("--max-edge-load must be greater than zero".to_string());
    }
    if options.timeout_ms == 0 {
        return Err("--timeout-ms must be greater than zero".to_string());
    }
    Ok(options)
}

fn parse_relation_counts_options(args: &[String]) -> Result<RelationCountsOptions, String> {
    let mut options = RelationCountsOptions {
        db_path: default_audit_db_path(),
        json_path: None,
        markdown_path: None,
    };
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--db" => options.db_path = take_path(args, &mut index, "--db")?,
            "--json" => options.json_path = Some(take_path(args, &mut index, "--json")?),
            "--markdown" | "--md" => {
                options.markdown_path = Some(take_path(args, &mut index, "--markdown")?)
            }
            value => return Err(format!("unknown audit relation-counts option: {value}")),
        }
        index += 1;
    }
    Ok(options)
}

fn parse_label_samples_options(args: &[String]) -> Result<LabelSamplesOptions, String> {
    let mut options = LabelSamplesOptions {
        edge_json_paths: Vec::new(),
        edge_markdown_paths: Vec::new(),
        path_json_paths: Vec::new(),
        path_markdown_paths: Vec::new(),
        json_path: Some(PathBuf::from(DEFAULT_LABELS_JSON)),
        markdown_path: Some(PathBuf::from(DEFAULT_LABELS_MARKDOWN)),
    };
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--edges-json" | "--edge-json" => {
                options
                    .edge_json_paths
                    .push(take_path(args, &mut index, "--edges-json")?)
            }
            "--edges-md" | "--edge-md" | "--edges-markdown" | "--edge-markdown" => options
                .edge_markdown_paths
                .push(take_path(args, &mut index, "--edges-md")?),
            "--paths-json" | "--path-json" => {
                options
                    .path_json_paths
                    .push(take_path(args, &mut index, "--paths-json")?)
            }
            "--paths-md" | "--path-md" | "--paths-markdown" | "--path-markdown" => options
                .path_markdown_paths
                .push(take_path(args, &mut index, "--paths-md")?),
            "--json" => options.json_path = Some(take_path(args, &mut index, "--json")?),
            "--markdown" | "--md" => {
                options.markdown_path = Some(take_path(args, &mut index, "--markdown")?)
            }
            "--no-json" => options.json_path = None,
            "--no-markdown" => options.markdown_path = None,
            value => return Err(format!("unknown audit label-samples option: {value}")),
        }
        index += 1;
    }
    if options.edge_json_paths.is_empty() && options.path_json_paths.is_empty() {
        return Err(
            "audit label-samples requires at least one --edges-json or --paths-json input"
                .to_string(),
        );
    }
    Ok(options)
}

fn parse_summarize_labels_options(args: &[String]) -> Result<SummarizeLabelsOptions, String> {
    let mut options = SummarizeLabelsOptions {
        label_paths: Vec::new(),
        label_dir: None,
        json_path: Some(PathBuf::from(DEFAULT_LABEL_SUMMARY_JSON)),
        markdown_path: Some(PathBuf::from(DEFAULT_LABEL_SUMMARY_MARKDOWN)),
    };
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--labels" | "--label-json" => options
                .label_paths
                .push(take_path(args, &mut index, "--labels")?),
            "--dir" | "--labels-dir" => {
                options.label_dir = Some(take_path(args, &mut index, "--dir")?)
            }
            "--json" => options.json_path = Some(take_path(args, &mut index, "--json")?),
            "--markdown" | "--md" => {
                options.markdown_path = Some(take_path(args, &mut index, "--markdown")?)
            }
            "--no-json" => options.json_path = None,
            "--no-markdown" => options.markdown_path = None,
            value => return Err(format!("unknown audit summarize-labels option: {value}")),
        }
        index += 1;
    }
    if options.label_paths.is_empty() && options.label_dir.is_none() {
        options.label_paths.push(PathBuf::from(DEFAULT_LABELS_JSON));
    }
    Ok(options)
}

fn inspect_storage(db_path: &Path) -> Result<StorageInspection, String> {
    let before = db_file_snapshot(db_path);
    let read_only = open_read_only_with_snapshot(db_path, &before)?;
    let read_only_mode_used = read_only.read_only_mode_used.clone();
    let immutable_mode_used = read_only.immutable_mode_used;
    let immutable_mode_reason = read_only.immutable_mode_reason.clone();
    let (
        file_family,
        integrity_check,
        page_metrics,
        dbstat_available,
        objects,
        categories,
        table_row_metrics,
        aggregate_metrics,
        dictionary_metrics,
        qualified_name_metric,
        fts_storage,
        edge_fact_mix,
        index_usage,
        core_query_plans,
    ) = {
        let connection = read_only.connection;
        let file_family = file_family_size(db_path);
        let integrity_check = sqlite_integrity_check(&connection);
        let page_metrics = page_metrics(&connection)?;
        let object_types = sqlite_object_types(&connection)?;
        let (dbstat_available, mut objects) =
            dbstat_objects(&connection, &object_types, file_family.database_bytes)?;
        if !dbstat_available {
            objects = fallback_objects(&connection, &object_types, file_family.database_bytes)?;
        }
        let object_bytes = objects
            .iter()
            .map(|object| (object.name.clone(), object.total_bytes))
            .collect::<HashMap<_, _>>();
        let categories = storage_category_breakdown(&objects);
        let table_row_metrics = table_row_metrics(&objects);
        let aggregate_metrics = aggregate_storage_metrics(&objects, &file_family);
        let dictionary_metrics = dictionary_metrics(&connection, &object_bytes)?;
        let qualified_name_metric = qualified_name_metric(&connection, &object_bytes)?;
        let fts_storage = fts_storage_metric(&connection, &object_bytes)?;
        let edge_fact_mix = edge_fact_mix(&connection)?;
        let core_query_plans = core_query_plan_reports(&connection);
        let index_usage = index_usage_report(&connection, &object_bytes, &core_query_plans)?;
        (
            file_family,
            integrity_check,
            page_metrics,
            dbstat_available,
            objects,
            categories,
            table_row_metrics,
            aggregate_metrics,
            dictionary_metrics,
            qualified_name_metric,
            fts_storage,
            edge_fact_mix,
            index_usage,
            core_query_plans,
        )
    };
    let after = db_file_snapshot(db_path);
    let inspection = read_only_inspection_audit(
        before,
        after,
        read_only_mode_used,
        immutable_mode_used,
        immutable_mode_reason,
    );
    let mut notes = vec![
        "Read-only audit: no VACUUM, ANALYZE, index drop, or storage rewrite was applied."
            .to_string(),
        "dbstat byte totals include SQLite b-tree pages and FTS shadow objects when available."
            .to_string(),
    ];
    if !dbstat_available {
        notes.push(
            "SQLite dbstat was unavailable; object byte totals use row counts only.".to_string(),
        );
    }
    Ok(StorageInspection {
        schema_version: AUDIT_SCHEMA_VERSION,
        db_path: path_string(db_path),
        inspection_read_only: true,
        main_db_size_before: inspection.main_db_size_before,
        main_db_size_after: inspection.main_db_size_after,
        main_db_mtime_before: inspection.main_db_mtime_before,
        main_db_mtime_after: inspection.main_db_mtime_after,
        main_db_hash_before: inspection.main_db_hash_before,
        main_db_hash_after: inspection.main_db_hash_after,
        main_db_hash_algorithm: inspection.main_db_hash_algorithm,
        sidecars_before: inspection.sidecars_before,
        sidecars_after: inspection.sidecars_after,
        sidecar_status: inspection.sidecar_status,
        artifact_mutated_during_inspection: inspection.artifact_mutated_during_inspection,
        sidecar_only_change: inspection.sidecar_only_change,
        read_only_mode_used: inspection.read_only_mode_used,
        immutable_mode_used: inspection.immutable_mode_used,
        immutable_mode_reason: inspection.immutable_mode_reason,
        dbstat_available,
        integrity_check,
        file_family,
        page_metrics,
        objects,
        categories,
        table_row_metrics,
        aggregate_metrics,
        dictionary_metrics,
        qualified_name_metric,
        fts_storage,
        edge_fact_mix,
        index_usage,
        core_query_plans,
        vacuum_analyze_measurement: json!({
            "vacuum_run": false,
            "analyze_run": false,
            "reason": "This phase is audit-only; measure VACUUM/ANALYZE on a copied DB before changing production artifacts."
        }),
        notes,
    })
}

fn validate_schema(db_path: &Path) -> Result<SchemaValidationReport, String> {
    let before = db_file_snapshot(db_path);
    let read_only = open_read_only_with_snapshot(db_path, &before)?;
    let read_only_mode_used = read_only.read_only_mode_used.clone();
    let immutable_mode_used = read_only.immutable_mode_used;
    let immutable_mode_reason = read_only.immutable_mode_reason.clone();
    let connection = read_only.connection;
    let user_version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .map_err(|error| error.to_string())?;

    let expected_columns = [
        ("files", "file_id"),
        ("files", "path_id"),
        ("files", "content_hash"),
        ("files", "content_template_id"),
        ("source_content_template", "content_template_id"),
        ("source_content_template", "content_hash"),
        ("source_spans", "id_key"),
        ("source_spans", "path_id"),
        ("entities", "id_key"),
        ("entities", "file_id"),
        ("entities", "qualified_name_id"),
        ("edges", "id_key"),
        ("edges", "file_id"),
        ("edges", "relation_id"),
        ("file_fts_rows", "file_id"),
        ("path_evidence", "id"),
    ]
    .into_iter()
    .map(|(table, column)| {
        let present = audit_table_has_column(&connection, table, column).unwrap_or(false);
        SchemaColumnCheck {
            table: table.to_string(),
            column: column.to_string(),
            present,
            status: if present { "pass" } else { "fail" }.to_string(),
        }
    })
    .collect::<Vec<_>>();

    let views = [
        "file_instance",
        "qualified_name_lookup",
        "qualified_name_debug",
        "object_id_lookup",
        "object_id_debug",
        "edges_compat",
    ]
    .into_iter()
    .map(|view| compile_schema_sql(&connection, view, &format!("SELECT * FROM {view} LIMIT 0")))
    .collect::<Vec<_>>();

    let default_query_sql = [
        (
            "symbol_query_exact_name",
            r#"
            SELECT e.id_key
            FROM entities e
            JOIN object_id_lookup oid ON oid.id = e.id_key
            JOIN symbol_dict name ON name.id = e.name_id
            JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
            JOIN path_dict path ON path.id = e.path_id
            LEFT JOIN files file ON file.file_id = COALESCE(e.file_id, e.path_id)
            WHERE oid.value = ?1 OR name.value = ?1 OR qname.value = ?1
            ORDER BY qname.value, oid.value
            LIMIT ?2
            "#,
        ),
        (
            "text_query_fts",
            r#"
            SELECT kind, id, repo_relative_path, line, title, body,
                   bm25(stage0_fts) AS rank
            FROM stage0_fts
            WHERE stage0_fts MATCH ?1
            ORDER BY rank, kind, id
            LIMIT ?2
            "#,
        ),
        (
            "bounded_relation_query",
            r#"
            SELECT e.id_key, head.value AS head_id, relation.value AS relation, tail.value AS tail_id
            FROM edges_compat e
            JOIN relation_kind_dict relation ON relation.id = e.relation_id
            JOIN object_id_lookup head ON head.id = e.head_id_key
            JOIN object_id_lookup tail ON tail.id = e.tail_id_key
            WHERE relation.value = ?1
            ORDER BY e.id_key
            LIMIT ?2
            "#,
        ),
        (
            "context_pack_path_evidence_lookup",
            r#"
            SELECT lookup.path_id
            FROM path_evidence_lookup lookup
            WHERE lookup.source_id = ?1 OR lookup.target_id = ?1
            ORDER BY lookup.confidence DESC, lookup.path_id
            LIMIT ?2
            "#,
        ),
        (
            "source_span_batch_load",
            r#"
            SELECT span.id_key, path.value AS repo_relative_path,
                   span.start_line, span.start_column, span.end_line, span.end_column
            FROM source_spans span
            JOIN path_dict path ON path.id = span.path_id
            ORDER BY span.id_key
            LIMIT ?1
            "#,
        ),
    ]
    .into_iter()
    .map(|(name, sql)| compile_schema_sql(&connection, name, sql))
    .collect::<Vec<_>>();

    let mut failures = Vec::new();
    for check in &expected_columns {
        if !check.present {
            failures.push(format!("missing column {}.{}", check.table, check.column));
        }
    }
    for check in views.iter().chain(default_query_sql.iter()) {
        if check.status != "pass" {
            failures.push(format!(
                "{} failed to compile: {}",
                check.name,
                check.error.as_deref().unwrap_or("unknown error")
            ));
        }
    }
    drop(connection);
    let after = db_file_snapshot(db_path);
    let inspection = read_only_inspection_audit(
        before,
        after,
        read_only_mode_used,
        immutable_mode_used,
        immutable_mode_reason,
    );

    Ok(SchemaValidationReport {
        schema_version: AUDIT_SCHEMA_VERSION,
        db_path: path_string(db_path),
        status: if failures.is_empty() {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
        inspection_read_only: true,
        main_db_size_before: inspection.main_db_size_before,
        main_db_size_after: inspection.main_db_size_after,
        main_db_mtime_before: inspection.main_db_mtime_before,
        main_db_mtime_after: inspection.main_db_mtime_after,
        main_db_hash_before: inspection.main_db_hash_before,
        main_db_hash_after: inspection.main_db_hash_after,
        main_db_hash_algorithm: inspection.main_db_hash_algorithm,
        sidecars_before: inspection.sidecars_before,
        sidecars_after: inspection.sidecars_after,
        sidecar_status: inspection.sidecar_status,
        artifact_mutated_during_inspection: inspection.artifact_mutated_during_inspection,
        sidecar_only_change: inspection.sidecar_only_change,
        read_only_mode_used: inspection.read_only_mode_used,
        immutable_mode_used: inspection.immutable_mode_used,
        immutable_mode_reason: inspection.immutable_mode_reason,
        user_version,
        expected_columns,
        views,
        default_query_sql,
        failures,
        notes: vec![
            "Read-only schema check: validates compact-proof compatibility views and representative default query SQL without mutating the DB.".to_string(),
            "This check is intentionally separate from storage optimization; it catches stale view/query compatibility regressions.".to_string(),
        ],
    })
}

fn audit_table_has_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?;
    for row in rows {
        if row.map_err(|error| error.to_string())? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn compile_schema_sql(connection: &Connection, name: &str, sql: &str) -> SchemaCompileCheck {
    match connection.prepare(sql) {
        Ok(_) => SchemaCompileCheck {
            name: name.to_string(),
            sql: normalize_sql_for_report(sql),
            status: "pass".to_string(),
            error: None,
        },
        Err(error) => SchemaCompileCheck {
            name: name.to_string(),
            sql: normalize_sql_for_report(sql),
            status: "fail".to_string(),
            error: Some(error.to_string()),
        },
    }
}

fn normalize_sql_for_report(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn run_storage_experiments(
    options: &StorageExperimentOptions,
) -> Result<StorageExperimentReport, String> {
    if !options.db_path.exists() {
        return Err(format!(
            "database does not exist: {}",
            options.db_path.display()
        ));
    }
    fs::create_dir_all(&options.workdir).map_err(|error| error.to_string())?;
    let run_dir = options.workdir.join(format!("run-{}", unix_time_ms()));
    fs::create_dir_all(&run_dir).map_err(|error| error.to_string())?;

    let experiments = vec![
        run_vacuum_analyze_experiment(options, &run_dir)?,
        run_drop_recreate_edge_indexes_experiment(options, &run_dir)?,
        run_drop_broad_unused_indexes_experiment(options, &run_dir)?,
        run_replace_broad_with_partial_index_experiment(options, &run_dir)?,
        run_compact_qualified_name_simulation_experiment(options, &run_dir)?,
        run_exact_base_partition_simulation_experiment(options, &run_dir)?,
        run_disable_fts_snippets_experiment(options, &run_dir)?,
        run_bulk_load_secondary_indexes_experiment(options, &run_dir)?,
    ];

    Ok(StorageExperimentReport {
        schema_version: AUDIT_SCHEMA_VERSION,
        original_db_path: path_string(&options.db_path),
        run_dir: path_string(&run_dir),
        original_file_family: file_family_size(&options.db_path),
        experiments,
        notes: vec![
            "Every experiment is run against a copied SQLite DB; the original path is opened read-only or copied from disk only.".to_string(),
            "Index-removal experiments are measurement-only and must not be translated into production schema changes without graph-truth and query-plan review.".to_string(),
            "Graph Truth is marked not applicable here because graph-truth cases reindex fixture repositories instead of consuming an already-copied benchmark DB.".to_string(),
        ],
    })
}

fn run_vacuum_analyze_experiment(
    options: &StorageExperimentOptions,
    run_dir: &Path,
) -> Result<StorageExperiment, String> {
    let copy_path = copy_database_for_experiment(&options.db_path, run_dir, "vacuum_analyze")?;
    let mut checkpoints = Vec::new();
    checkpoints.push(storage_experiment_checkpoint(&copy_path, "before", 0)?);

    let analyze_elapsed = mutate_copy(&copy_path, "ANALYZE; PRAGMA optimize;")?;
    checkpoints.push(storage_experiment_checkpoint(
        &copy_path,
        "after_analyze",
        analyze_elapsed,
    )?);

    let vacuum_elapsed = mutate_copy(&copy_path, "VACUUM;")?;
    checkpoints.push(storage_experiment_checkpoint(
        &copy_path,
        "after_vacuum",
        vacuum_elapsed,
    )?);

    finish_storage_experiment(
        options,
        "vacuum_analyze",
        copy_path,
        vec![
            "ANALYZE".to_string(),
            "PRAGMA optimize".to_string(),
            "VACUUM".to_string(),
        ],
        checkpoints,
        vec![
            "Maintenance-only experiment; graph semantics should be unchanged, but production adoption still needs the normal semantic gate.".to_string(),
        ],
    )
}

fn run_drop_recreate_edge_indexes_experiment(
    options: &StorageExperimentOptions,
    run_dir: &Path,
) -> Result<StorageExperiment, String> {
    let copy_path =
        copy_database_for_experiment(&options.db_path, run_dir, "drop_recreate_edge_indexes")?;
    let mut checkpoints = Vec::new();
    checkpoints.push(storage_experiment_checkpoint(&copy_path, "before", 0)?);

    let drop_sql = EDGE_INDEXES
        .iter()
        .map(|(name, _)| format!("DROP INDEX IF EXISTS {};", quote_ident(name)))
        .collect::<Vec<_>>()
        .join("\n");
    let drop_elapsed = mutate_copy(&copy_path, &format!("{drop_sql}\nVACUUM;"))?;
    checkpoints.push(storage_experiment_checkpoint(
        &copy_path,
        "after_drop_edge_indexes",
        drop_elapsed,
    )?);

    let create_sql = EDGE_INDEXES
        .iter()
        .map(|(_, sql)| format!("{sql};"))
        .collect::<Vec<_>>()
        .join("\n");
    let recreate_elapsed = mutate_copy(&copy_path, &format!("{create_sql}\nANALYZE;"))?;
    checkpoints.push(storage_experiment_checkpoint(
        &copy_path,
        "after_recreate_edge_indexes",
        recreate_elapsed,
    )?);

    finish_storage_experiment(
        options,
        "drop_recreate_edge_indexes",
        copy_path,
        vec![
            "DROP INDEX idx_edges_head_relation".to_string(),
            "DROP INDEX idx_edges_tail_relation".to_string(),
            "DROP INDEX idx_edges_span_path".to_string(),
            "VACUUM".to_string(),
            "recreate edge indexes".to_string(),
            "ANALYZE".to_string(),
        ],
        checkpoints,
        vec![
            "Measures index rebuild cost and final size after restoring the same edge indexes.".to_string(),
            "Temporary checkpoint after dropping indexes is expected to change query plans; recommendation is based on the final restored checkpoint.".to_string(),
        ],
    )
}

fn run_drop_broad_unused_indexes_experiment(
    options: &StorageExperimentOptions,
    run_dir: &Path,
) -> Result<StorageExperiment, String> {
    let copy_path =
        copy_database_for_experiment(&options.db_path, run_dir, "drop_broad_unused_indexes")?;
    let mut checkpoints = Vec::new();
    checkpoints.push(storage_experiment_checkpoint(&copy_path, "before", 0)?);
    let existing = existing_index_names(&copy_path, BROAD_UNUSED_INDEXES)?;
    let mut notes = vec![
        "Drops broad indexes called out by storage forensics as unused or weakly justified by default workflows.".to_string(),
        "This is a copied-DB measurement only; source-span and retrieval-trace workflows need explicit validation before any schema change.".to_string(),
    ];
    if existing.is_empty() {
        checkpoints.push(storage_experiment_checkpoint(
            &copy_path,
            "after_no_matching_indexes",
            0,
        )?);
        notes.push("No matching broad indexes existed in this DB.".to_string());
    } else {
        let drop_sql = existing
            .iter()
            .map(|name| format!("DROP INDEX IF EXISTS {};", quote_ident(name)))
            .collect::<Vec<_>>()
            .join("\n");
        let elapsed = mutate_copy(&copy_path, &format!("{drop_sql}\nVACUUM;\nANALYZE;"))?;
        checkpoints.push(storage_experiment_checkpoint(
            &copy_path,
            "after_drop_broad_indexes",
            elapsed,
        )?);
    }

    finish_storage_experiment(
        options,
        "drop_broad_unused_indexes",
        copy_path,
        existing
            .iter()
            .map(|name| format!("DROP INDEX {name}"))
            .chain(["VACUUM".to_string(), "ANALYZE".to_string()])
            .collect(),
        checkpoints,
        notes,
    )
}

fn run_replace_broad_with_partial_index_experiment(
    options: &StorageExperimentOptions,
    run_dir: &Path,
) -> Result<StorageExperiment, String> {
    let copy_path = copy_database_for_experiment(
        &options.db_path,
        run_dir,
        "replace_broad_with_partial_index",
    )?;
    let mut checkpoints = Vec::new();
    checkpoints.push(storage_experiment_checkpoint(&copy_path, "before", 0)?);
    let mut notes = vec![
        "Replaces the broad span-path edge index with a CALLS-focused partial index when the CALLS relation id is present.".to_string(),
        "The partial index is experimental; it is only useful if EXPLAIN shows default relation/unresolved-call queries stop scanning the whole edge table.".to_string(),
    ];
    let calls_id = dict_id_in_db(&copy_path, "relation_kind_dict", "CALLS")?;
    let elapsed = if let Some(calls_id) = calls_id {
        let heuristic_ids = dict_ids_in_db(
            &copy_path,
            "exactness_dict",
            &[
                "static_heuristic",
                "inferred",
                "derived_from_verified_edges",
            ],
        )?;
        let exactness_clause = if heuristic_ids.is_empty() {
            notes.push(
                "No heuristic exactness ids were present; partial index covers all CALLS edges."
                    .to_string(),
            );
            String::new()
        } else {
            format!(
                " AND exactness_id IN ({})",
                heuristic_ids
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let sql = format!(
            "DROP INDEX IF EXISTS idx_edges_span_path;\n\
             DROP INDEX IF EXISTS idx_edges_calls_partial_heuristic;\n\
             CREATE INDEX IF NOT EXISTS idx_edges_calls_partial_heuristic \
             ON edges(relation_id, exactness_id, id_key) \
             WHERE relation_id = {calls_id}{exactness_clause};\n\
             ANALYZE;\nVACUUM;"
        );
        mutate_copy(&copy_path, &sql)?
    } else {
        notes.push("CALLS relation id was absent; no partial index could be created.".to_string());
        0
    };
    checkpoints.push(storage_experiment_checkpoint(
        &copy_path,
        "after_partial_calls_index",
        elapsed,
    )?);

    finish_storage_experiment(
        options,
        "replace_broad_with_partial_index",
        copy_path,
        vec![
            "DROP INDEX idx_edges_span_path".to_string(),
            "CREATE PARTIAL INDEX idx_edges_calls_partial_heuristic".to_string(),
            "ANALYZE".to_string(),
            "VACUUM".to_string(),
        ],
        checkpoints,
        notes,
    )
}

fn run_compact_qualified_name_simulation_experiment(
    options: &StorageExperimentOptions,
    run_dir: &Path,
) -> Result<StorageExperiment, String> {
    let copy_path = copy_database_for_experiment(
        &options.db_path,
        run_dir,
        "simulate_compact_qualified_names",
    )?;
    let mut checkpoints = Vec::new();
    checkpoints.push(storage_experiment_checkpoint(&copy_path, "before", 0)?);
    let mut notes = vec![
        "Simulates replacing full qualified-name text with compact tuple text on a copied DB.".to_string(),
        "This intentionally breaks human-readable qualified-name query output and is not a production schema change.".to_string(),
    ];
    let elapsed = if table_exists_in_db(&copy_path, "qualified_name_dict")? {
        let pair_collision_count = qname_prefix_suffix_collision_count(&copy_path)?;
        let replacement = if pair_collision_count == 0 {
            "'q:' || prefix_id || ':' || suffix_id"
        } else {
            notes.push(format!(
                "{pair_collision_count} prefix/suffix groups collided; simulation includes id to preserve uniqueness."
            ));
            "'q:' || prefix_id || ':' || suffix_id || ':' || id"
        };
        let sql =
            format!("UPDATE qualified_name_dict SET value = {replacement};\nVACUUM;\nANALYZE;");
        mutate_copy(&copy_path, &sql)?
    } else {
        notes.push(
            "qualified_name_dict was absent; no compact-name simulation was run.".to_string(),
        );
        0
    };
    checkpoints.push(storage_experiment_checkpoint(
        &copy_path,
        "after_compact_qname_simulation",
        elapsed,
    )?);

    finish_storage_experiment(
        options,
        "simulate_compact_qualified_names",
        copy_path,
        vec![
            "UPDATE qualified_name_dict.value to compact tuple surrogate".to_string(),
            "VACUUM".to_string(),
            "ANALYZE".to_string(),
        ],
        checkpoints,
        notes,
    )
}

fn run_exact_base_partition_simulation_experiment(
    options: &StorageExperimentOptions,
    run_dir: &Path,
) -> Result<StorageExperiment, String> {
    let copy_path =
        copy_database_for_experiment(&options.db_path, run_dir, "simulate_exact_base_partition")?;
    let mut checkpoints = Vec::new();
    checkpoints.push(storage_experiment_checkpoint(&copy_path, "before", 0)?);
    let mut notes = vec![
        "Simulates separating proof-grade base graph rows from heuristic/debug/test rows by moving non-proof edges into a side table.".to_string(),
        "The simulation changes graph query answers, so it is a storage-forensics candidate only and cannot be recommended without reducer/query-layer changes.".to_string(),
    ];
    let mut joins = Vec::new();
    let mut predicates = Vec::new();
    if table_column_exists_in_db(&copy_path, "edges", "derived")? {
        predicates.push("e.derived != 0".to_string());
    } else {
        notes.push("edges.derived is absent; derived-edge partitioning was skipped.".to_string());
    }
    if table_exists_in_db(&copy_path, "exactness_dict")?
        && table_column_exists_in_db(&copy_path, "edges", "exactness_id")?
    {
        joins.push(
            "LEFT JOIN exactness_dict exactness ON exactness.id = e.exactness_id".to_string(),
        );
        predicates.push(
            "COALESCE(exactness.value, '') NOT IN ('exact', 'compiler_verified', 'lsp_verified', 'parser_verified')"
                .to_string(),
        );
    } else {
        notes.push(
            "exactness metadata is absent; exact/heuristic partitioning was skipped.".to_string(),
        );
    }
    if table_exists_in_db(&copy_path, "edge_class_dict")?
        && table_column_exists_in_db(&copy_path, "edges", "edge_class_id")?
    {
        joins.push(
            "LEFT JOIN edge_class_dict edge_class ON edge_class.id = e.edge_class_id".to_string(),
        );
        predicates.push(
            "COALESCE(edge_class.value, '') IN ('base_heuristic', 'derived', 'test', 'mock', 'mixed', 'unknown')"
                .to_string(),
        );
    } else {
        notes.push(
            "edge class metadata is absent; class-based partitioning was skipped.".to_string(),
        );
    }
    if table_exists_in_db(&copy_path, "edge_context_dict")?
        && table_column_exists_in_db(&copy_path, "edges", "context_id")?
    {
        joins.push("LEFT JOIN edge_context_dict context ON context.id = e.context_id".to_string());
        predicates.push("COALESCE(context.value, '') IN ('test', 'mock', 'mixed')".to_string());
    } else {
        notes.push(
            "edge context metadata is absent; test/mock partitioning was skipped.".to_string(),
        );
    }
    let elapsed = if predicates.is_empty() {
        notes.push(
            "No supported partition predicates were available; simulation was a no-op.".to_string(),
        );
        0
    } else {
        let sql = format!(
            "DROP TABLE IF EXISTS edges_non_proof_sim;\n\
             CREATE TABLE edges_non_proof_sim AS\n\
             SELECT e.*\n\
             FROM edges e\n\
             {}\n\
             WHERE {};\n\
             DELETE FROM edges WHERE id_key IN (SELECT id_key FROM edges_non_proof_sim);\n\
             VACUUM;\n\
             ANALYZE;",
            joins.join("\n"),
            predicates.join("\n OR ")
        );
        mutate_copy(&copy_path, &sql)?
    };
    checkpoints.push(storage_experiment_checkpoint(
        &copy_path,
        "after_exact_base_partition_simulation",
        elapsed,
    )?);

    finish_storage_experiment(
        options,
        "simulate_exact_base_partition",
        copy_path,
        vec![
            "CREATE TABLE edges_non_proof_sim AS non-proof edges".to_string(),
            "DELETE non-proof rows from edges".to_string(),
            "VACUUM".to_string(),
            "ANALYZE".to_string(),
        ],
        checkpoints,
        notes,
    )
}

fn run_disable_fts_snippets_experiment(
    options: &StorageExperimentOptions,
    run_dir: &Path,
) -> Result<StorageExperiment, String> {
    let copy_path =
        copy_database_for_experiment(&options.db_path, run_dir, "disable_fts_snippets_simulation")?;
    let mut checkpoints = Vec::new();
    checkpoints.push(storage_experiment_checkpoint(&copy_path, "before", 0)?);
    let mut notes = vec![
        "Simulates compact mode with no FTS/snippet payload in SQLite by clearing stage0_fts on a copy.".to_string(),
        "This is expected to break text-query workflows unless a replacement text index exists outside SQLite.".to_string(),
    ];
    let elapsed = if table_exists_in_db(&copy_path, "stage0_fts")? {
        mutate_copy(&copy_path, "DELETE FROM stage0_fts;\nVACUUM;\nANALYZE;")?
    } else {
        notes.push("stage0_fts was absent; no compact-mode FTS mutation was run.".to_string());
        0
    };
    checkpoints.push(storage_experiment_checkpoint(
        &copy_path,
        "after_disable_fts_snippets_simulation",
        elapsed,
    )?);

    finish_storage_experiment(
        options,
        "disable_fts_snippets_simulation",
        copy_path,
        vec![
            "DELETE FROM stage0_fts".to_string(),
            "VACUUM".to_string(),
            "ANALYZE".to_string(),
        ],
        checkpoints,
        notes,
    )
}

fn run_bulk_load_secondary_indexes_experiment(
    options: &StorageExperimentOptions,
    run_dir: &Path,
) -> Result<StorageExperiment, String> {
    let copy_path =
        copy_database_for_experiment(&options.db_path, run_dir, "bulk_load_secondary_indexes")?;
    let mut checkpoints = Vec::new();
    checkpoints.push(storage_experiment_checkpoint(&copy_path, "before", 0)?);
    let drop_sql = SECONDARY_INDEXES
        .iter()
        .map(|(name, _)| format!("DROP INDEX IF EXISTS {};", quote_ident(name)))
        .collect::<Vec<_>>()
        .join("\n");
    let drop_elapsed = mutate_copy(&copy_path, &format!("{drop_sql}\nVACUUM;"))?;
    checkpoints.push(storage_experiment_checkpoint(
        &copy_path,
        "after_drop_secondary_indexes",
        drop_elapsed,
    )?);
    let create_sql = SECONDARY_INDEXES
        .iter()
        .map(|(_, sql)| format!("{sql};"))
        .collect::<Vec<_>>()
        .join("\n");
    let recreate_elapsed = mutate_copy(&copy_path, &format!("{create_sql}\nANALYZE;"))?;
    checkpoints.push(storage_experiment_checkpoint(
        &copy_path,
        "after_recreate_secondary_indexes",
        recreate_elapsed,
    )?);

    finish_storage_experiment(
        options,
        "bulk_load_secondary_indexes",
        copy_path,
        vec![
            "DROP all secondary non-auto indexes".to_string(),
            "VACUUM".to_string(),
            "recreate secondary indexes".to_string(),
            "ANALYZE".to_string(),
        ],
        checkpoints,
        vec![
            "Simulates the final storage shape after bulk loading data first and recreating secondary indexes after insertion.".to_string(),
            "This does not measure insertion throughput directly; it measures final size/query impact after the rebuild.".to_string(),
        ],
    )
}

fn finish_storage_experiment(
    options: &StorageExperimentOptions,
    name: &str,
    copy_path: PathBuf,
    mutations: Vec<String>,
    checkpoints: Vec<StorageExperimentCheckpoint>,
    notes: Vec<String>,
) -> Result<StorageExperiment, String> {
    let degraded_queries = storage_query_regressions(&checkpoints);
    let summary = storage_experiment_summary(&checkpoints);
    let context_packet = context_packet_status(&checkpoints);
    let recommendation =
        storage_experiment_recommendation(name, &checkpoints, &context_packet, &notes);
    let mut copy_removed = false;
    if !options.keep_copies {
        if let Some(parent) = copy_path.parent() {
            fs::remove_dir_all(parent).map_err(|error| error.to_string())?;
            copy_removed = true;
        }
    }
    Ok(StorageExperiment {
        name: name.to_string(),
        copied_db_path: path_string(copy_path),
        copy_removed,
        mutations,
        summary,
        checkpoints,
        degraded_queries,
        graph_truth: graph_truth_not_applicable(),
        context_packet,
        recommendation,
        notes,
    })
}

fn storage_experiment_checkpoint(
    db_path: &Path,
    name: &str,
    mutation_elapsed_ms: u64,
) -> Result<StorageExperimentCheckpoint, String> {
    let inspection = inspect_storage(db_path)?;
    let connection = open_read_only(db_path)?;
    Ok(StorageExperimentCheckpoint {
        name: name.to_string(),
        mutation_elapsed_ms,
        integrity_check: inspection.integrity_check,
        file_family: inspection.file_family,
        page_metrics: inspection.page_metrics,
        categories: storage_category_breakdown(&inspection.objects),
        top_objects: inspection.objects.into_iter().take(40).collect(),
        dictionary_metrics: inspection.dictionary_metrics,
        qualified_name_metric: inspection.qualified_name_metric,
        query_latencies: measure_standard_queries(&connection),
    })
}

fn storage_category_breakdown(objects: &[StorageObjectSize]) -> StorageCategoryBreakdown {
    let dictionary_tables = [
        "qualified_name_dict",
        "object_id_dict",
        "symbol_dict",
        "qname_prefix_dict",
        "path_dict",
    ];
    let unique_text_indexes = [
        "sqlite_autoindex_qualified_name_dict_1",
        "sqlite_autoindex_object_id_dict_1",
        "sqlite_autoindex_symbol_dict_1",
        "sqlite_autoindex_qname_prefix_dict_1",
        "sqlite_autoindex_path_dict_1",
    ];
    let mut categories = StorageCategoryBreakdown {
        dictionary_table_bytes: 0,
        unique_text_index_bytes: 0,
        edge_index_bytes: 0,
        fts_bytes: 0,
        source_span_bytes: 0,
        snippet_like_bytes: 0,
    };
    for object in objects {
        if dictionary_tables.contains(&object.name.as_str()) {
            categories.dictionary_table_bytes += object.total_bytes;
        }
        if unique_text_indexes.contains(&object.name.as_str()) {
            categories.unique_text_index_bytes += object.total_bytes;
        }
        if object.name.starts_with("idx_edges_") {
            categories.edge_index_bytes += object.total_bytes;
        }
        if object.name.starts_with("stage0_fts") {
            categories.fts_bytes += object.total_bytes;
        }
        if object.name.starts_with("source_spans") || object.name == "idx_source_spans_path" {
            categories.source_span_bytes += object.total_bytes;
        }
        if object.name.to_ascii_lowercase().contains("snippet") {
            categories.snippet_like_bytes += object.total_bytes;
        }
    }
    categories
}

fn table_row_metrics(objects: &[StorageObjectSize]) -> Vec<TableRowMetric> {
    let mut metrics = objects
        .iter()
        .filter(|object| object.object_type == "table")
        .filter_map(|object| {
            let row_count = object.row_count?;
            Some(TableRowMetric {
                table: object.name.clone(),
                row_count,
                total_bytes: object.total_bytes,
                payload_bytes: object.payload_bytes,
                average_total_bytes_per_row: average_bytes(object.total_bytes, row_count),
                average_payload_bytes_per_row: average_bytes(object.payload_bytes, row_count),
            })
        })
        .collect::<Vec<_>>();
    metrics.sort_by(|left, right| {
        right
            .total_bytes
            .cmp(&left.total_bytes)
            .then_with(|| left.table.cmp(&right.table))
    });
    metrics
}

fn aggregate_storage_metrics(
    objects: &[StorageObjectSize],
    file_family: &FileFamilySize,
) -> AggregateStorageMetrics {
    let categories = storage_category_breakdown(objects);
    let table_count = objects
        .iter()
        .filter(|object| object.object_type == "table")
        .count();
    let index_count = objects
        .iter()
        .filter(|object| object.object_type == "index" || object.object_type == "autoindex")
        .count();
    let total_rows_observed = objects
        .iter()
        .filter(|object| object.object_type == "table")
        .filter_map(|object| object.row_count)
        .sum();
    let edge = objects.iter().find(|object| object.name == "edges");
    let edge_count = edge.and_then(|object| object.row_count).unwrap_or(0);
    let structural = objects
        .iter()
        .find(|object| object.name == "structural_relations");
    let callsites = objects.iter().find(|object| object.name == "callsites");
    let callsite_args = objects.iter().find(|object| object.name == "callsite_args");
    let structural_record_count = structural.and_then(|object| object.row_count).unwrap_or(0);
    let callsite_record_count = callsites.and_then(|object| object.row_count).unwrap_or(0);
    let callsite_arg_record_count = callsite_args
        .and_then(|object| object.row_count)
        .unwrap_or(0);
    let semantic_edge_count =
        edge_count + structural_record_count + callsite_record_count + callsite_arg_record_count;
    let edge_table_bytes = edge.map(|object| object.total_bytes).unwrap_or(0);
    let structural_table_bytes = structural.map(|object| object.total_bytes).unwrap_or(0);
    let callsite_table_bytes = callsites.map(|object| object.total_bytes).unwrap_or(0);
    let callsite_arg_table_bytes = callsite_args.map(|object| object.total_bytes).unwrap_or(0);
    let source_spans = objects.iter().find(|object| object.name == "source_spans");
    let source_span_count = source_spans
        .and_then(|object| object.row_count)
        .unwrap_or(0);
    let source_span_table_bytes = source_spans.map(|object| object.total_bytes).unwrap_or(0);
    AggregateStorageMetrics {
        table_count,
        index_count,
        total_rows_observed,
        edge_count,
        proof_edge_count: edge_count,
        structural_record_count,
        callsite_record_count,
        callsite_arg_record_count,
        semantic_edge_count,
        edge_table_bytes,
        edge_index_bytes: categories.edge_index_bytes,
        structural_table_bytes,
        callsite_table_bytes,
        callsite_arg_table_bytes,
        average_database_bytes_per_edge: average_bytes(file_family.total_bytes, edge_count),
        average_database_bytes_per_semantic_edge: average_bytes(
            file_family.total_bytes,
            semantic_edge_count,
        ),
        average_edge_table_bytes_per_edge: average_bytes(edge_table_bytes, edge_count),
        average_edge_table_plus_index_bytes_per_edge: average_bytes(
            edge_table_bytes + categories.edge_index_bytes,
            edge_count,
        ),
        source_span_count,
        source_span_table_bytes,
        average_source_span_bytes_per_row: average_bytes(
            source_span_table_bytes,
            source_span_count,
        ),
    }
}

fn average_bytes(bytes: u64, rows: u64) -> f64 {
    if rows == 0 {
        0.0
    } else {
        ((bytes as f64 / rows as f64) * 100.0).round() / 100.0
    }
}

fn storage_query_regressions(checkpoints: &[StorageExperimentCheckpoint]) -> Vec<QueryRegression> {
    let Some(baseline) = checkpoints.first() else {
        return Vec::new();
    };
    let baseline_by_name = baseline
        .query_latencies
        .iter()
        .map(|query| (query.name.as_str(), query.elapsed_ms))
        .collect::<BTreeMap<_, _>>();
    let mut regressions = Vec::new();
    for checkpoint in checkpoints.iter().skip(1) {
        for query in &checkpoint.query_latencies {
            let Some(baseline_ms) = baseline_by_name.get(query.name.as_str()).copied() else {
                continue;
            };
            let ratio = if baseline_ms == 0 {
                if query.elapsed_ms == 0 {
                    1.0
                } else {
                    query.elapsed_ms as f64
                }
            } else {
                query.elapsed_ms as f64 / baseline_ms as f64
            };
            let degraded =
                query.status != "ok" || (query.elapsed_ms > baseline_ms + 1 && ratio > 1.25);
            regressions.push(QueryRegression {
                query: query.name.clone(),
                checkpoint: checkpoint.name.clone(),
                baseline_ms,
                after_ms: query.elapsed_ms,
                degraded,
                ratio: (ratio * 100.0).round() / 100.0,
            });
        }
    }
    regressions
}

fn storage_experiment_summary(
    checkpoints: &[StorageExperimentCheckpoint],
) -> StorageExperimentSummary {
    let before = checkpoints
        .first()
        .map(|checkpoint| checkpoint.file_family.total_bytes)
        .unwrap_or(0);
    let after = checkpoints
        .last()
        .map(|checkpoint| checkpoint.file_family.total_bytes)
        .unwrap_or(before);
    let delta = after as i64 - before as i64;
    let percent = if before == 0 {
        0.0
    } else {
        ((delta as f64 / before as f64) * 10_000.0).round() / 100.0
    };
    StorageExperimentSummary {
        db_size_before_bytes: before,
        db_size_after_bytes: after,
        size_delta_bytes: delta,
        size_delta_percent: percent,
        core_query_latency_before_after: query_latency_deltas(checkpoints),
    }
}

fn query_latency_deltas(checkpoints: &[StorageExperimentCheckpoint]) -> Vec<QueryLatencyDelta> {
    let Some(before) = checkpoints.first() else {
        return Vec::new();
    };
    let Some(after) = checkpoints.last() else {
        return Vec::new();
    };
    let before_by_name = before
        .query_latencies
        .iter()
        .map(|query| (query.name.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    after
        .query_latencies
        .iter()
        .filter_map(|query| {
            let before = before_by_name.get(query.name.as_str())?;
            Some(QueryLatencyDelta {
                query: query.name.clone(),
                before_ms: before.elapsed_ms,
                after_ms: query.elapsed_ms,
                delta_ms: query.elapsed_ms as i64 - before.elapsed_ms as i64,
                before_status: before.status.clone(),
                after_status: query.status.clone(),
            })
        })
        .collect()
}

fn context_packet_status(checkpoints: &[StorageExperimentCheckpoint]) -> Value {
    let Some(before) = checkpoints.first() else {
        return json!({
            "status": "not_run",
            "query": "context_pack_outbound",
            "reason": "No experiment checkpoints were captured."
        });
    };
    let Some(after) = checkpoints.last() else {
        return json!({
            "status": "not_run",
            "query": "context_pack_outbound",
            "reason": "No final experiment checkpoint was captured."
        });
    };
    let before_query = before
        .query_latencies
        .iter()
        .find(|query| query.name == "context_pack_outbound")
        .or_else(|| {
            before
                .query_latencies
                .iter()
                .find(|query| query.name == "edge_head_relation_lookup")
        });
    let after_query = after
        .query_latencies
        .iter()
        .find(|query| query.name == "context_pack_outbound")
        .or_else(|| {
            after
                .query_latencies
                .iter()
                .find(|query| query.name == "edge_head_relation_lookup")
        });
    match (before_query, after_query) {
        (Some(before_query), Some(after_query)) => json!({
            "status": if after_query.status == "ok" { "queried" } else { "failed" },
            "query": after_query.name,
            "before_ms": before_query.elapsed_ms,
            "after_ms": after_query.elapsed_ms,
            "before_rows_observed": before_query.rows_observed,
            "after_rows_observed": after_query.rows_observed,
            "after_error": after_query.error,
            "correctness_claim": false,
            "reason": "A context-pack-shaped expansion query was measured on the copied DB, but Graph Truth was not rerun against this DB artifact."
        }),
        _ => json!({
            "status": "not_run",
            "query": "context_pack_outbound",
            "correctness_claim": false,
            "reason": "No context-pack-shaped query measurement was available for this schema."
        }),
    }
}

fn storage_experiment_recommendation(
    name: &str,
    checkpoints: &[StorageExperimentCheckpoint],
    context_packet: &Value,
    notes: &[String],
) -> StorageExperimentRecommendation {
    let integrity_failed = checkpoints
        .iter()
        .any(|checkpoint| !integrity_status_ok(&checkpoint.integrity_check));
    let final_regressions = final_query_regressions(checkpoints);
    let context_failed = context_packet
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status == "failed");
    let schema_breaking = matches!(
        name,
        "simulate_compact_qualified_names"
            | "simulate_exact_base_partition"
            | "disable_fts_snippets_simulation"
            | "replace_broad_with_partial_index"
            | "drop_broad_unused_indexes"
    );
    let (recommended, decision, reason) = if integrity_failed {
        (
            false,
            "not_recommended".to_string(),
            "At least one copied-DB checkpoint failed PRAGMA integrity_check.".to_string(),
        )
    } else if !final_regressions.is_empty() || context_failed {
        (
            false,
            "not_recommended".to_string(),
            "Final checkpoint degraded or failed a core/context query.".to_string(),
        )
    } else if schema_breaking {
        (
            false,
            "not_recommended_for_production_yet".to_string(),
            "The copied-DB result changes schema/query semantics and needs a reducer/query-layer design plus Graph Truth before adoption.".to_string(),
        )
    } else {
        (
            true,
            "recommended_for_next_safe_trial".to_string(),
            "Final copied-DB checkpoint preserved measured core/context query behavior; run the semantic gate before applying this to production artifacts.".to_string(),
        )
    };
    let reason = if notes.iter().any(|note| note.contains("No matching")) {
        format!("{reason} This DB did not contain all candidate structures.")
    } else {
        reason
    };
    StorageExperimentRecommendation {
        recommended,
        decision,
        reason,
    }
}

fn final_query_regressions(checkpoints: &[StorageExperimentCheckpoint]) -> Vec<QueryRegression> {
    let Some(baseline) = checkpoints.first() else {
        return Vec::new();
    };
    let Some(final_checkpoint) = checkpoints.last() else {
        return Vec::new();
    };
    if baseline.name == final_checkpoint.name {
        return Vec::new();
    }
    let baseline_by_name = baseline
        .query_latencies
        .iter()
        .map(|query| (query.name.as_str(), query.elapsed_ms))
        .collect::<BTreeMap<_, _>>();
    final_checkpoint
        .query_latencies
        .iter()
        .filter_map(|query| {
            let baseline_ms = baseline_by_name.get(query.name.as_str()).copied()?;
            let ratio = if baseline_ms == 0 {
                if query.elapsed_ms == 0 {
                    1.0
                } else {
                    query.elapsed_ms as f64
                }
            } else {
                query.elapsed_ms as f64 / baseline_ms as f64
            };
            let degraded =
                query.status != "ok" || (query.elapsed_ms > baseline_ms + 1 && ratio > 1.25);
            degraded.then(|| QueryRegression {
                query: query.name.clone(),
                checkpoint: final_checkpoint.name.clone(),
                baseline_ms,
                after_ms: query.elapsed_ms,
                degraded,
                ratio: (ratio * 100.0).round() / 100.0,
            })
        })
        .collect()
}

fn integrity_status_ok(value: &Value) -> bool {
    value
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status == "ok")
}

fn graph_truth_not_applicable() -> Value {
    json!({
        "status": "not_run",
        "applicable": false,
        "reason": "Storage experiments mutate copied DB artifacts; graph-truth fixtures reindex source fixtures and do not consume this copied DB directly."
    })
}

fn copy_database_for_experiment(
    original_db: &Path,
    run_dir: &Path,
    experiment_name: &str,
) -> Result<PathBuf, String> {
    let experiment_dir = run_dir.join(experiment_name);
    fs::create_dir_all(&experiment_dir).map_err(|error| error.to_string())?;
    let copy_path = experiment_dir.join("codegraph.sqlite");
    copy_database_family(original_db, &copy_path)?;
    Ok(copy_path)
}

fn copy_database_family(original_db: &Path, copy_path: &Path) -> Result<(), String> {
    ensure_experiment_copy_path(original_db, copy_path)?;
    if let Some(parent) = copy_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    for path in [
        copy_path.to_path_buf(),
        sqlite_sidecar_path(copy_path, "-wal"),
        sqlite_sidecar_path(copy_path, "-shm"),
    ] {
        if path.exists() {
            fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
    }
    fs::copy(original_db, copy_path).map_err(|error| error.to_string())?;
    for suffix in ["-wal", "-shm"] {
        let source = sqlite_sidecar_path(original_db, suffix);
        if source.exists() {
            let target = sqlite_sidecar_path(copy_path, suffix);
            fs::copy(&source, &target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn ensure_experiment_copy_path(original_db: &Path, copy_path: &Path) -> Result<(), String> {
    if absolute_path(original_db)? == absolute_path(copy_path)? {
        return Err(format!(
            "refusing to run storage experiment on the original DB path: {}",
            original_db.display()
        ));
    }
    Ok(())
}

fn existing_index_names(db_path: &Path, candidates: &[&str]) -> Result<Vec<String>, String> {
    let connection = open_read_only(db_path)?;
    let mut existing = Vec::new();
    for name in candidates {
        if sqlite_master_exists(&connection, "index", name)? {
            existing.push((*name).to_string());
        }
    }
    Ok(existing)
}

fn table_exists_in_db(db_path: &Path, table: &str) -> Result<bool, String> {
    let connection = open_read_only(db_path)?;
    table_exists(&connection, table)
}

fn table_column_exists_in_db(db_path: &Path, table: &str, column: &str) -> Result<bool, String> {
    let connection = open_read_only(db_path)?;
    table_column_exists(&connection, table, column)
}

fn table_column_exists(connection: &Connection, table: &str, column: &str) -> Result<bool, String> {
    if !table_exists(connection, table)? {
        return Ok(false);
    }
    let pragma = format!("PRAGMA table_info({})", quote_ident(table));
    let mut statement = connection
        .prepare(&pragma)
        .map_err(|error| error.to_string())?;
    let mapped = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?;
    for row in mapped {
        if row.map_err(|error| error.to_string())? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn dict_id_in_db(db_path: &Path, table: &str, value: &str) -> Result<Option<i64>, String> {
    let connection = open_read_only(db_path)?;
    if !table_exists(&connection, table)? {
        return Ok(None);
    }
    let sql = format!(
        "SELECT id FROM {} WHERE value = ?1 LIMIT 1",
        quote_ident(table)
    );
    connection
        .query_row(&sql, [value], |row| row.get::<_, i64>(0))
        .optional()
        .map_err(|error| error.to_string())
}

fn dict_ids_in_db(db_path: &Path, table: &str, values: &[&str]) -> Result<Vec<i64>, String> {
    let mut ids = Vec::new();
    for value in values {
        if let Some(id) = dict_id_in_db(db_path, table, value)? {
            ids.push(id);
        }
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

fn qname_prefix_suffix_collision_count(db_path: &Path) -> Result<u64, String> {
    let connection = open_read_only(db_path)?;
    if !table_exists(&connection, "qualified_name_dict")? {
        return Ok(0);
    }
    connection
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM (
                SELECT prefix_id, suffix_id, COUNT(*) AS count
                FROM qualified_name_dict
                GROUP BY prefix_id, suffix_id
                HAVING count > 1
            )
            "#,
            [],
            |row| row.get::<_, u64>(0),
        )
        .map_err(|error| error.to_string())
}

fn sqlite_master_exists(
    connection: &Connection,
    object_type: &str,
    name: &str,
) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2 LIMIT 1",
            params![object_type, name],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|error| error.to_string())
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return fs::canonicalize(path).map_err(|error| error.to_string());
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("path has no file name: {}", path.display()))?;
    let parent = if parent.exists() {
        fs::canonicalize(parent).map_err(|error| error.to_string())?
    } else if parent.is_absolute() {
        parent.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(parent)
    };
    Ok(parent.join(file_name))
}

fn sqlite_sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", db_path.display(), suffix))
}

fn mutate_copy(db_path: &Path, sql: &str) -> Result<u64, String> {
    let start = Instant::now();
    let connection = Connection::open(db_path)
        .map_err(|error| format!("failed to open copied DB {}: {error}", db_path.display()))?;
    connection
        .execute_batch(sql)
        .map_err(|error| format!("failed to mutate copied DB {}: {error}", db_path.display()))?;
    Ok(elapsed_ms(start))
}

fn measure_standard_queries(connection: &Connection) -> Vec<QueryLatencyMeasurement> {
    standard_query_specs()
        .into_iter()
        .map(|(name, sql)| measure_query(connection, name, sql))
        .collect()
}

fn standard_query_specs() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "entity_name_lookup",
            "SELECT id_key FROM entities WHERE name_id = (SELECT name_id FROM entities ORDER BY id_key LIMIT 1) LIMIT 64",
        ),
        (
            "entity_qname_lookup",
            "SELECT id_key FROM entities WHERE qualified_name_id = (SELECT qualified_name_id FROM entities ORDER BY id_key LIMIT 1) LIMIT 64",
        ),
        (
            "edge_head_relation_lookup",
            "SELECT id_key FROM edges WHERE head_id_key = (SELECT head_id_key FROM edges ORDER BY id_key LIMIT 1) AND relation_id = (SELECT relation_id FROM edges ORDER BY id_key LIMIT 1) LIMIT 64",
        ),
        (
            "edge_tail_relation_lookup",
            "SELECT id_key FROM edges WHERE tail_id_key = (SELECT tail_id_key FROM edges ORDER BY id_key LIMIT 1) AND relation_id = (SELECT relation_id FROM edges ORDER BY id_key LIMIT 1) LIMIT 64",
        ),
        (
            "edge_span_path_lookup",
            "SELECT id_key FROM edges WHERE span_path_id = (SELECT span_path_id FROM edges ORDER BY id_key LIMIT 1) LIMIT 64",
        ),
        (
            "relation_count_scan",
            "SELECT relation_id, COUNT(*) FROM edges GROUP BY relation_id LIMIT 64",
        ),
        (
            "text_query_fts",
            "SELECT rowid, kind, id, repo_relative_path, line, title, body, bm25(stage0_fts) AS rank FROM stage0_fts WHERE stage0_fts MATCH 'login' ORDER BY rank LIMIT 20",
        ),
        (
            "relation_query_calls",
            "SELECT e.id_key FROM edges e WHERE e.relation_id = (SELECT id FROM relation_kind_dict WHERE value = 'CALLS') ORDER BY e.id_key LIMIT 20",
        ),
        (
            "context_pack_outbound",
            "SELECT e.id_key FROM edges e WHERE e.head_id_key = (SELECT head_id_key FROM edges ORDER BY id_key LIMIT 1) AND e.relation_id = (SELECT relation_id FROM edges ORDER BY id_key LIMIT 1) ORDER BY e.id_key LIMIT 64",
        ),
        (
            "impact_inbound",
            "SELECT e.id_key FROM edges e WHERE e.tail_id_key = (SELECT tail_id_key FROM edges ORDER BY id_key LIMIT 1) AND e.relation_id = (SELECT relation_id FROM edges ORDER BY id_key LIMIT 1) ORDER BY e.id_key LIMIT 64",
        ),
        (
            "unresolved_calls_paginated",
            "SELECT e.id_key FROM edges_compat e JOIN exactness_dict exactness ON exactness.id = e.exactness_id JOIN entities tail ON tail.id_key = e.tail_id_key JOIN symbol_dict tail_name ON tail_name.id = tail.name_id JOIN qualified_name_lookup tail_qname ON tail_qname.id = tail.qualified_name_id JOIN extractor_dict tail_extractor ON tail_extractor.id = tail.created_from_id WHERE e.relation_id = (SELECT id FROM relation_kind_dict WHERE value = 'CALLS') AND (exactness.value = 'static_heuristic' OR (e.flags_bitset & 2) != 0 OR lower(COALESCE(tail.metadata_json, '')) LIKE '%unresolved%' OR lower(tail_extractor.value) LIKE '%heuristic%' OR lower(tail_name.value) LIKE '%unknown_callee%' OR tail_qname.value LIKE 'static_reference:%') ORDER BY e.id_key LIMIT 20 OFFSET 0",
        ),
    ]
}

fn measure_query(connection: &Connection, name: &str, sql: &str) -> QueryLatencyMeasurement {
    let explain_query_plan = explain_query_plan(connection, sql).unwrap_or_default();
    let start = Instant::now();
    let mut rows_observed = 0u64;
    let result = (|| -> Result<(), String> {
        let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
        let mut rows = statement.query([]).map_err(|error| error.to_string())?;
        while rows.next().map_err(|error| error.to_string())?.is_some() {
            rows_observed += 1;
        }
        Ok(())
    })();
    let elapsed_ms = elapsed_ms(start);
    let (status, error) = match result {
        Ok(()) => ("ok".to_string(), None),
        Err(error) => ("error".to_string(), Some(error)),
    };
    QueryLatencyMeasurement {
        name: name.to_string(),
        sql: sql.to_string(),
        elapsed_ms,
        rows_observed,
        status,
        error,
        query_plan_analysis: analyze_query_plan(&explain_query_plan),
        explain_query_plan,
    }
}

fn explain_query_plan(connection: &Connection, sql: &str) -> Result<Vec<QueryPlanRow>, String> {
    let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let mut statement = connection
        .prepare(&explain_sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(QueryPlanRow {
                id: row.get(0)?,
                parent: row.get(1)?,
                detail: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut plan = Vec::new();
    for row in rows {
        plan.push(row.map_err(|error| error.to_string())?);
    }
    Ok(plan)
}

fn analyze_query_plan(plan: &[QueryPlanRow]) -> QueryPlanAnalysis {
    let mut indexes = Vec::new();
    let mut full_scans = Vec::new();
    for row in plan {
        let lower = row.detail.to_ascii_lowercase();
        if lower.contains(" scan ") || lower.starts_with("scan ") {
            full_scans.push(row.detail.clone());
        }
        if let Some(index) = query_plan_index_name(&row.detail) {
            if !indexes.contains(&index) {
                indexes.push(index);
            }
        }
    }
    QueryPlanAnalysis {
        uses_indexes: !indexes.is_empty(),
        indexes_used: indexes,
        full_scans,
    }
}

fn query_plan_index_name(detail: &str) -> Option<String> {
    for marker in [
        "USING COVERING INDEX ",
        "USING INDEX ",
        "USING INTEGER PRIMARY KEY ",
        "USING PRIMARY KEY ",
    ] {
        if let Some((_, rest)) = detail.split_once(marker) {
            return rest
                .split_whitespace()
                .next()
                .map(|name| name.trim_matches(|ch| ch == '(' || ch == ')').to_string());
        }
    }
    None
}

fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn unix_time_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(u128::from(u64::MAX)) as u64,
        Err(_) => 0,
    }
}

fn sample_edges(options: &SampleEdgesOptions) -> Result<EdgeSampleReport, String> {
    let connection = open_read_only(&options.db_path)?;
    let repo_roots = repo_roots(&connection)?;
    let relation_filter = options.relation.as_deref();
    let relation_id_filter = if let Some(relation) = relation_filter {
        match lookup_relation_id(&connection, relation)? {
            Some(relation_id) => Some(relation_id),
            None => {
                return Ok(edge_sample_report(
                    options,
                    Vec::new(),
                    vec![
                        "Classification fields are intentionally blank in markdown for human review."
                            .to_string(),
                        format!("Relation filter `{relation}` is not present in relation_kind_dict."),
                    ],
                ));
            }
        }
    } else {
        None
    };

    let mut raw_rows = Vec::new();
    if let Some((min_edge_id, max_edge_id)) = edge_id_range(&connection)? {
        if options.limit > 0 {
            let width = (max_edge_id as i128 - min_edge_id as i128 + 1).max(1);
            let start_id = (min_edge_id as i128 + (options.seed as i128).rem_euclid(width)) as i64;
            raw_rows.extend(sample_edge_rows_from_range(
                &connection,
                relation_id_filter,
                start_id,
                false,
                options.limit,
            )?);
            if raw_rows.len() < options.limit {
                raw_rows.extend(sample_edge_rows_from_range(
                    &connection,
                    relation_id_filter,
                    start_id,
                    true,
                    options.limit - raw_rows.len(),
                )?);
            }
        }
    }

    let mut samples = Vec::new();
    for row in raw_rows {
        let provenance_edges =
            serde_json::from_str::<Vec<String>>(&row.provenance_json).unwrap_or_default();
        let metadata =
            serde_json::from_str::<Value>(&row.metadata_json).unwrap_or_else(|_| json!({}));
        let (source_snippet, span_loaded, span_load_error) = if options.include_snippets {
            match load_source_snippet(&repo_roots, &row.source_span) {
                Ok(snippet) => (Some(snippet), true, None),
                Err(error) => (None, false, Some(error)),
            }
        } else {
            (None, false, None)
        };
        let context = infer_context(
            &row.relation,
            &row.source_span.repo_relative_path,
            &row.head.id,
            &row.tail.id,
        );
        let fact_classification =
            classify_edge_fact(&row.relation, &row.exactness, row.derived, &context);
        let missing_metadata = missing_edge_metadata(
            &row.head,
            &row.tail,
            row.repo_commit.as_deref(),
            row.file_hash.as_deref(),
            row.derived,
            &provenance_edges,
            span_loaded,
            options.include_snippets,
            &metadata,
        );
        samples.push(EdgeSample {
            ordinal: samples.len() + 1,
            edge_id: row.edge_id,
            head: row.head,
            relation: row.relation,
            tail: row.tail,
            source_span: row.source_span,
            relation_direction: "head_to_tail".to_string(),
            exactness: row.exactness,
            confidence: row.confidence,
            repo_commit: row.repo_commit,
            file_hash: row.file_hash,
            derived: row.derived,
            extractor: row.extractor,
            fact_classification,
            production_test_mock_context: context,
            provenance_edges,
            metadata,
            span_loaded,
            span_load_error,
            source_snippet,
            missing_metadata,
        });
    }

    Ok(edge_sample_report(
        options,
        samples,
        vec![
            "Classification fields are intentionally blank in markdown for human review."
                .to_string(),
            "Sampling is deterministic for a seed and starts at a seeded edge primary-key range, wrapping once if needed.".to_string(),
            "production_test_mock_context is inferred from relation/path/id text because context is not first-class in the current edge schema.".to_string(),
        ],
    ))
}

fn edge_sample_report(
    options: &SampleEdgesOptions,
    samples: Vec<EdgeSample>,
    notes: Vec<String>,
) -> EdgeSampleReport {
    EdgeSampleReport {
        schema_version: AUDIT_SCHEMA_VERSION,
        db_path: path_string(&options.db_path),
        relation_filter: options.relation.clone(),
        limit: options.limit,
        seed: options.seed,
        include_snippets: options.include_snippets,
        manual_classification_options: SAMPLE_CLASSIFICATIONS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        samples,
        notes,
    }
}

fn lookup_relation_id(connection: &Connection, relation: &str) -> Result<Option<i64>, String> {
    connection
        .query_row(
            "SELECT id FROM relation_kind_dict WHERE value = ?1",
            [relation],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn edge_id_range(connection: &Connection) -> Result<Option<(i64, i64)>, String> {
    let range = connection
        .query_row("SELECT MIN(id_key), MAX(id_key) FROM edges", [], |row| {
            Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?))
        })
        .map_err(|error| error.to_string())?;
    Ok(match range {
        (Some(min_id), Some(max_id)) => Some((min_id, max_id)),
        _ => None,
    })
}

fn sample_edge_rows_from_range(
    connection: &Connection,
    relation_id_filter: Option<i64>,
    start_id: i64,
    before_start: bool,
    limit: usize,
) -> Result<Vec<RawEdgeSample>, String> {
    let operator = if before_start { "<" } else { ">=" };
    let sql = format!(
        r#"
        SELECT COALESCE(edge_id.value, 'edge-key:' || e.id_key) AS edge_id,
               head.value AS head_id,
               head_name.value AS head_name,
               head_qname.value AS head_qname,
               head_kind.value AS head_kind,
               relation.value AS relation,
               tail.value AS tail_id,
               tail_name.value AS tail_name,
               tail_qname.value AS tail_qname,
               tail_kind.value AS tail_kind,
               span_path.value AS span_repo_relative_path,
               e.start_line, e.start_column, e.end_line, e.end_column,
               extractor.value AS extractor,
               exactness.value AS exactness,
               e.confidence,
               e.repo_commit,
               file.content_hash AS file_hash,
               e.derived,
               e.provenance_edges_json,
               e.metadata_json
        FROM edges_compat e
        LEFT JOIN object_id_lookup edge_id ON edge_id.id = e.id_key
        JOIN object_id_lookup head ON head.id = e.head_id_key
        JOIN relation_kind_dict relation ON relation.id = e.relation_id
        JOIN object_id_lookup tail ON tail.id = e.tail_id_key
        JOIN path_dict span_path ON span_path.id = e.span_path_id
        LEFT JOIN files file ON file.file_id = e.file_id
        JOIN extractor_dict extractor ON extractor.id = e.extractor_id
        JOIN exactness_dict exactness ON exactness.id = e.exactness_id
        LEFT JOIN entities head_entity ON head_entity.id_key = e.head_id_key
        LEFT JOIN symbol_dict head_name ON head_name.id = head_entity.name_id
        LEFT JOIN qualified_name_lookup head_qname ON head_qname.id = head_entity.qualified_name_id
        LEFT JOIN entity_kind_dict head_kind ON head_kind.id = head_entity.kind_id
        LEFT JOIN entities tail_entity ON tail_entity.id_key = e.tail_id_key
        LEFT JOIN symbol_dict tail_name ON tail_name.id = tail_entity.name_id
        LEFT JOIN qualified_name_lookup tail_qname ON tail_qname.id = tail_entity.qualified_name_id
        LEFT JOIN entity_kind_dict tail_kind ON tail_kind.id = tail_entity.kind_id
        WHERE e.id_key {operator} ?1
          AND (?2 IS NULL OR e.relation_id = ?2)
        ORDER BY e.id_key
        LIMIT ?3
        "#
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let mapped = statement
        .query_map(
            params![start_id, relation_id_filter, limit as i64],
            raw_edge_sample_from_row,
        )
        .map_err(|error| error.to_string())?;
    let mut rows = Vec::new();
    for row in mapped {
        rows.push(row.map_err(|error| error.to_string())?);
    }
    Ok(rows)
}

fn raw_edge_sample_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawEdgeSample> {
    let span = AuditSourceSpan {
        repo_relative_path: row.get("span_repo_relative_path")?,
        start_line: row.get::<_, i64>("start_line")?.max(0) as u32,
        start_column: row
            .get::<_, Option<i64>>("start_column")?
            .map(|value| value as u32),
        end_line: row.get::<_, i64>("end_line")?.max(0) as u32,
        end_column: row
            .get::<_, Option<i64>>("end_column")?
            .map(|value| value as u32),
    };
    Ok(RawEdgeSample {
        edge_id: row.get("edge_id")?,
        head: AuditEndpoint {
            id: row.get("head_id")?,
            name: row.get("head_name")?,
            qualified_name: row.get("head_qname")?,
            kind: row.get("head_kind")?,
        },
        relation: row.get("relation")?,
        tail: AuditEndpoint {
            id: row.get("tail_id")?,
            name: row.get("tail_name")?,
            qualified_name: row.get("tail_qname")?,
            kind: row.get("tail_kind")?,
        },
        source_span: span,
        exactness: row.get("exactness")?,
        confidence: row.get("confidence")?,
        repo_commit: row.get("repo_commit")?,
        file_hash: row.get("file_hash")?,
        derived: row.get::<_, i64>("derived")? != 0,
        extractor: row.get("extractor")?,
        provenance_json: row.get("provenance_edges_json")?,
        metadata_json: row.get("metadata_json")?,
    })
}

fn sample_paths(options: &SamplePathsOptions) -> Result<PathEvidenceSampleReport, String> {
    let total_start = Instant::now();
    let deadline = total_start
        .checked_add(Duration::from_millis(options.timeout_ms))
        .unwrap_or(total_start);
    let mut timing = PathSamplerTiming::default();

    let started = Instant::now();
    let connection = open_read_only(&options.db_path)?;
    timing.open_db_ms = elapsed_ms(started);
    if let Err(error) =
        enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "open_db")
    {
        return Ok(partial_path_sample_report(
            options,
            timing,
            0,
            Vec::new(),
            Vec::new(),
            error,
            total_start,
        ));
    }

    let started = Instant::now();
    let repo_roots = repo_roots(&connection)?;
    timing.repo_roots_ms = elapsed_ms(started);
    if let Err(error) =
        enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "repo_roots")
    {
        return Ok(partial_path_sample_report(
            options,
            timing,
            0,
            Vec::new(),
            Vec::new(),
            error,
            total_start,
        ));
    }

    let started = Instant::now();
    let stored_path_count = row_count(&connection, "path_evidence").unwrap_or(0);
    timing.count_ms = elapsed_ms(started);
    if let Err(error) =
        enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "row_count")
    {
        return Ok(partial_path_sample_report(
            options,
            timing,
            stored_path_count,
            Vec::new(),
            Vec::new(),
            error,
            total_start,
        ));
    }

    let started = Instant::now();
    let mut explain_plans = path_sampler_query_plans(&connection, options)?;
    timing.explain_ms = elapsed_ms(started);
    if let Err(error) =
        enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "explain")
    {
        return Ok(partial_path_sample_report(
            options,
            timing,
            stored_path_count,
            explain_plans,
            Vec::new(),
            error,
            total_start,
        ));
    }

    let started = Instant::now();
    let index_status = path_evidence_index_status(&connection)?;
    timing.index_check_ms = elapsed_ms(started);
    if let Err(error) =
        enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "index_check")
    {
        return Ok(partial_path_sample_report(
            options,
            timing,
            stored_path_count,
            explain_plans,
            index_status,
            error,
            total_start,
        ));
    }

    let mut partial_diagnostic = false;
    let mut timeout_ms_exhausted = false;
    let mut stop_reason = None;
    let mut notes = vec![
        "Classification fields are intentionally blank in markdown for human review.".to_string(),
        "PathEvidence sampling is bounded: candidate path IDs are selected first, details are batch-loaded only for those IDs, and snippets are loaded only for sampled spans when requested.".to_string(),
    ];

    let stored_result = if stored_path_count > 0 {
        match stored_path_samples(
            &connection,
            &repo_roots,
            options,
            total_start,
            deadline,
            &mut timing,
        ) {
            Ok(result) => result,
            Err(error) if is_path_sample_timeout_error(&error) => {
                partial_diagnostic = true;
                timeout_ms_exhausted = true;
                stop_reason = Some(error.clone());
                notes.push(error);
                StoredPathSampleResult::default()
            }
            Err(error) => return Err(error),
        }
    } else {
        StoredPathSampleResult::default()
    };
    let mut samples = stored_result.samples;
    let stored_samples = samples.len();
    let fallback_allowed = options.mode.allows_generated_fallback();
    if fallback_allowed && !timeout_ms_exhausted && samples.len() < options.limit {
        let generated = generated_path_samples(&connection, &repo_roots, options, samples.len())?;
        samples.extend(generated);
    }
    samples.truncate(options.limit);
    let generated_path_count = samples
        .iter()
        .filter(|sample| sample.generated_by_audit)
        .count();
    let generated_fallback_used = generated_path_count > 0;
    timing.total_ms = elapsed_ms(total_start);

    explain_plans.sort_by(|left, right| left.name.cmp(&right.name));
    let source_snippet_omitted_count = path_sample_source_snippet_omitted_count(&samples);
    let stored_path_count_usize = usize::try_from(stored_path_count).unwrap_or(usize::MAX);
    let path_evidence_omitted_count = stored_path_count_usize.saturating_sub(stored_samples);
    let path_evidence_truncated =
        stored_result.edge_load_truncated || path_evidence_omitted_count > 0 || partial_diagnostic;
    notes.push(format!(
        "Mode `{}` {} generated fallback paths.",
        options.mode.as_str(),
        if fallback_allowed {
            "allows"
        } else {
            "disables"
        }
    ));
    notes.push(format!(
        "Path edge materialization load cap: {} rows; truncated: {}.",
        options.max_edge_load, stored_result.edge_load_truncated
    ));
    notes.push(format!(
        "Stored PathEvidence samples used before fallback: {stored_samples}."
    ));
    notes.push(format!(
        "PathEvidence sampler row budget: limit={}, max_edge_load={}, timeout_ms={}.",
        options.limit, options.max_edge_load, options.timeout_ms
    ));

    Ok(PathEvidenceSampleReport {
        schema_version: AUDIT_SCHEMA_VERSION,
        db_path: path_string(&options.db_path),
        limit: options.limit,
        seed: options.seed,
        include_snippets: options.include_snippets,
        mode: options.mode,
        max_edge_load: options.max_edge_load,
        timeout_ms: options.timeout_ms,
        stored_path_count,
        candidate_path_count: stored_result.candidate_path_count,
        loaded_path_edge_count: stored_result.loaded_path_edge_count,
        edge_load_truncated: stored_result.edge_load_truncated,
        path_evidence_truncated,
        path_evidence_omitted_count,
        hydration_budget_exhausted: stored_result.edge_load_truncated,
        source_snippet_omitted_count,
        partial_diagnostic,
        timeout_ms_exhausted,
        stop_reason,
        fallback_allowed,
        generated_path_count,
        generated_fallback_used,
        timing,
        explain_query_plan: explain_plans,
        index_status,
        manual_classification_options: SAMPLE_CLASSIFICATIONS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        samples,
        notes,
    })
}

fn partial_path_sample_report(
    options: &SamplePathsOptions,
    mut timing: PathSamplerTiming,
    stored_path_count: u64,
    mut explain_query_plan: Vec<PathSamplerQueryPlan>,
    index_status: Vec<PathEvidenceIndexStatus>,
    stop_reason: String,
    total_start: Instant,
) -> PathEvidenceSampleReport {
    timing.total_ms = elapsed_ms(total_start);
    explain_query_plan.sort_by(|left, right| left.name.cmp(&right.name));
    PathEvidenceSampleReport {
        schema_version: AUDIT_SCHEMA_VERSION,
        db_path: path_string(&options.db_path),
        limit: options.limit,
        seed: options.seed,
        include_snippets: options.include_snippets,
        mode: options.mode,
        max_edge_load: options.max_edge_load,
        timeout_ms: options.timeout_ms,
        stored_path_count,
        candidate_path_count: 0,
        loaded_path_edge_count: 0,
        edge_load_truncated: false,
        path_evidence_truncated: true,
        path_evidence_omitted_count: usize::try_from(stored_path_count).unwrap_or(usize::MAX),
        hydration_budget_exhausted: true,
        source_snippet_omitted_count: 0,
        partial_diagnostic: true,
        timeout_ms_exhausted: true,
        stop_reason: Some(stop_reason.clone()),
        fallback_allowed: options.mode.allows_generated_fallback(),
        generated_path_count: 0,
        generated_fallback_used: false,
        timing,
        explain_query_plan,
        index_status,
        manual_classification_options: SAMPLE_CLASSIFICATIONS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        samples: Vec::new(),
        notes: vec![
            "Partial diagnostic report emitted because the sample-paths timeout budget was exhausted.".to_string(),
            stop_reason,
            format!(
                "PathEvidence sampler row budget: limit={}, max_edge_load={}, timeout_ms={}.",
                options.limit, options.max_edge_load, options.timeout_ms
            ),
        ],
    }
}

#[derive(Debug, Clone, Default)]
struct StoredPathSampleResult {
    samples: Vec<PathEvidenceSample>,
    candidate_path_count: usize,
    loaded_path_edge_count: usize,
    edge_load_truncated: bool,
}

fn stored_path_samples(
    connection: &Connection,
    repo_roots: &[PathBuf],
    options: &SamplePathsOptions,
    total_start: Instant,
    deadline: Instant,
    timing: &mut PathSamplerTiming,
) -> Result<StoredPathSampleResult, String> {
    let started = Instant::now();
    let path_ids = bounded_path_evidence_ids(connection, options.limit, options.seed)?;
    timing.candidate_select_ms = elapsed_ms(started);
    enforce_path_sample_deadline(
        total_start,
        deadline,
        options.timeout_ms,
        "candidate_select",
    )?;

    let started = Instant::now();
    let rows = load_stored_path_rows(connection, &path_ids)?;
    timing.path_rows_load_ms = elapsed_ms(started);
    enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "path_rows_load")?;

    let started = Instant::now();
    let loaded_edges = load_materialized_path_edges(connection, &path_ids, options.max_edge_load)?;
    let loaded_path_edge_count = loaded_edges.row_count;
    let edge_load_truncated = loaded_edges.truncated;
    timing.path_edges_load_ms = elapsed_ms(started);
    enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "path_edges_load")?;

    let mut entity_ids = BTreeSet::new();
    for row in &rows {
        entity_ids.insert(row.source.clone());
        entity_ids.insert(row.target.clone());
        for (head, _, tail) in parse_edge_triples(&row.edges_json) {
            entity_ids.insert(head);
            entity_ids.insert(tail);
        }
    }
    for edge_rows in loaded_edges.by_path.values() {
        for edge in edge_rows {
            entity_ids.insert(edge.head_id.clone());
            entity_ids.insert(edge.tail_id.clone());
        }
    }

    let started = Instant::now();
    let endpoints = batch_entity_endpoints(connection, &entity_ids)?;
    timing.endpoint_load_ms = elapsed_ms(started);
    enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "endpoint_load")?;

    let mut snippet_cache = BTreeMap::<String, Result<SourceSnippet, String>>::new();
    let mut samples = Vec::new();
    for row in rows {
        enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "sample_build")?;
        let sample_started = Instant::now();
        let edge_rows = loaded_edges
            .by_path
            .get(&row.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let sample = path_sample_from_stored_row(
            repo_roots,
            row,
            edge_rows,
            &endpoints,
            samples.len() + 1,
            options.include_snippets,
            &mut snippet_cache,
            timing,
        )?;
        timing.sample_build_ms = timing
            .sample_build_ms
            .saturating_add(elapsed_ms(sample_started));
        samples.push(sample);
        if samples.len() >= options.limit {
            break;
        }
    }

    Ok(StoredPathSampleResult {
        samples,
        candidate_path_count: path_ids.len(),
        loaded_path_edge_count,
        edge_load_truncated,
    })
}

#[derive(Debug, Clone)]
struct StoredPathRow {
    id: String,
    source: String,
    target: String,
    summary: Option<String>,
    metapath_json: String,
    edges_json: String,
    source_spans_json: String,
    exactness: String,
    confidence: f64,
    metadata_json: String,
}

#[derive(Debug, Clone)]
struct StoredPathEdgeRow {
    path_id: String,
    ordinal: usize,
    edge_id: String,
    head_id: String,
    relation: String,
    tail_id: String,
    source_span_path: Option<String>,
    exactness: Option<String>,
    confidence: Option<f64>,
    derived: Option<bool>,
    edge_class: Option<String>,
    context: Option<String>,
    provenance_edges_json: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct MaterializedPathEdges {
    by_path: BTreeMap<String, Vec<StoredPathEdgeRow>>,
    row_count: usize,
    truncated: bool,
}

fn enforce_path_sample_deadline(
    total_start: Instant,
    deadline: Instant,
    timeout_ms: u64,
    stage: &str,
) -> Result<(), String> {
    if Instant::now() > deadline {
        return Err(format!(
            "sample-paths timed out after >{timeout_ms}ms during {stage}; elapsed_ms={}",
            elapsed_ms(total_start)
        ));
    }
    Ok(())
}

fn is_path_sample_timeout_error(error: &str) -> bool {
    error.starts_with("sample-paths timed out")
}

fn path_sample_source_snippet_omitted_count(samples: &[PathEvidenceSample]) -> usize {
    samples
        .iter()
        .map(|sample| {
            sample
                .source_spans
                .len()
                .saturating_sub(sample.source_snippets.len())
        })
        .sum()
}

fn bounded_path_evidence_ids(
    connection: &Connection,
    limit: usize,
    seed: u64,
) -> Result<Vec<String>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    match path_evidence_rowid_range(connection)? {
        Some((min_rowid, max_rowid)) => {
            let width = (max_rowid as i128 - min_rowid as i128 + 1).max(1);
            let start_rowid = (min_rowid as i128 + (seed as i128).rem_euclid(width)) as i64;
            let mut ids =
                path_evidence_ids_from_rowid_range(connection, start_rowid, false, limit)?;
            if ids.len() < limit {
                ids.extend(path_evidence_ids_from_rowid_range(
                    connection,
                    start_rowid,
                    true,
                    limit - ids.len(),
                )?);
            }
            Ok(ids)
        }
        None => {
            let mut statement = connection
                .prepare("SELECT id FROM path_evidence ORDER BY id LIMIT ?1")
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([limit as i64], |row| row.get::<_, String>(0))
                .map_err(|error| error.to_string())?;
            collect_string_rows(rows)
        }
    }
}

fn path_evidence_rowid_range(connection: &Connection) -> Result<Option<(i64, i64)>, String> {
    let range = connection
        .query_row(
            "SELECT MIN(rowid), MAX(rowid) FROM path_evidence",
            [],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(match range {
        Some((Some(min_id), Some(max_id))) => Some((min_id, max_id)),
        _ => None,
    })
}

fn path_evidence_ids_from_rowid_range(
    connection: &Connection,
    start_rowid: i64,
    before_start: bool,
    limit: usize,
) -> Result<Vec<String>, String> {
    let operator = if before_start { "<" } else { ">=" };
    let sql =
        format!("SELECT id FROM path_evidence WHERE rowid {operator} ?1 ORDER BY rowid LIMIT ?2");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![start_rowid, limit as i64], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| error.to_string())?;
    collect_string_rows(rows)
}

fn load_stored_path_rows(
    connection: &Connection,
    path_ids: &[String],
) -> Result<Vec<StoredPathRow>, String> {
    if path_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = sql_placeholders(path_ids.len());
    let sql = format!(
        "
        SELECT id, source, target, summary, metapath_json, edges_json,
               source_spans_json, exactness, length, confidence, metadata_json
        FROM path_evidence
        WHERE id IN ({placeholders})
        "
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(path_ids.iter()), |row| {
            Ok(StoredPathRow {
                id: row.get("id")?,
                source: row.get("source")?,
                target: row.get("target")?,
                summary: row.get("summary")?,
                metapath_json: row.get("metapath_json")?,
                edges_json: row.get("edges_json")?,
                source_spans_json: row.get("source_spans_json")?,
                exactness: row.get("exactness")?,
                confidence: row.get("confidence")?,
                metadata_json: row.get("metadata_json")?,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut by_id = BTreeMap::new();
    for row in rows {
        let row = row.map_err(|error| error.to_string())?;
        by_id.insert(row.id.clone(), row);
    }
    Ok(path_ids
        .iter()
        .filter_map(|id| by_id.remove(id))
        .collect::<Vec<_>>())
}

fn load_materialized_path_edges(
    connection: &Connection,
    path_ids: &[String],
    max_edge_load: usize,
) -> Result<MaterializedPathEdges, String> {
    if path_ids.is_empty()
        || max_edge_load == 0
        || !table_exists(connection, "path_evidence_edges")?
    {
        return Ok(MaterializedPathEdges::default());
    }
    let placeholders = sql_placeholders(path_ids.len());
    let has_compact_metadata =
        audit_table_has_column(connection, "path_evidence_edges", "exactness")?
            && audit_table_has_column(connection, "path_evidence_edges", "confidence")?
            && audit_table_has_column(connection, "path_evidence_edges", "derived")?
            && audit_table_has_column(connection, "path_evidence_edges", "edge_class")?
            && audit_table_has_column(connection, "path_evidence_edges", "context")?
            && audit_table_has_column(connection, "path_evidence_edges", "provenance_edges_json")?;
    let compact_columns = if has_compact_metadata {
        "exactness, confidence, derived, edge_class, context, provenance_edges_json"
    } else {
        "NULL AS exactness, NULL AS confidence, NULL AS derived, NULL AS edge_class, NULL AS context, NULL AS provenance_edges_json"
    };
    let sql = format!(
        "
        SELECT path_id, ordinal, edge_id, head_id, relation, tail_id, source_span_path,
               {compact_columns}
        FROM path_evidence_edges
        WHERE path_id IN ({placeholders})
        ORDER BY path_id, ordinal
        LIMIT ?{}
        ",
        path_ids.len() + 1
    );
    let mut params = path_ids.iter().map(String::as_str).collect::<Vec<_>>();
    let limit_text = (max_edge_load + 1).to_string();
    params.push(limit_text.as_str());
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(params), |row| {
            Ok(StoredPathEdgeRow {
                path_id: row.get("path_id")?,
                ordinal: row.get::<_, i64>("ordinal")?.max(0) as usize,
                edge_id: row.get("edge_id")?,
                head_id: row.get("head_id")?,
                relation: row.get("relation")?,
                tail_id: row.get("tail_id")?,
                source_span_path: row.get("source_span_path")?,
                exactness: row.get("exactness")?,
                confidence: row.get("confidence")?,
                derived: row
                    .get::<_, Option<i64>>("derived")?
                    .map(|value| value != 0),
                edge_class: row.get("edge_class")?,
                context: row.get("context")?,
                provenance_edges_json: row.get("provenance_edges_json")?,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut by_path = BTreeMap::<String, Vec<StoredPathEdgeRow>>::new();
    let mut row_count = 0usize;
    let mut truncated = false;
    for row in rows {
        let row = row.map_err(|error| error.to_string())?;
        row_count += 1;
        if row_count > max_edge_load {
            truncated = true;
            break;
        }
        by_path.entry(row.path_id.clone()).or_default().push(row);
    }
    for rows in by_path.values_mut() {
        rows.sort_by_key(|row| row.ordinal);
    }
    Ok(MaterializedPathEdges {
        by_path,
        row_count: row_count.min(max_edge_load),
        truncated,
    })
}

fn batch_entity_endpoints(
    connection: &Connection,
    ids: &BTreeSet<String>,
) -> Result<BTreeMap<String, AuditEndpoint>, String> {
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut endpoints = BTreeMap::new();
    let mut hash_hexes = BTreeSet::new();
    let mut fallback_ids = BTreeSet::new();
    for id in ids {
        if let Some(hex) = repo_entity_hash_hex(id) {
            hash_hexes.insert(hex);
        } else {
            fallback_ids.insert(id.clone());
        }
    }

    if !hash_hexes.is_empty() {
        let blob_literals = hash_hexes
            .iter()
            .map(|hex| format!("X'{hex}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "
            SELECT lower(hex(e.entity_hash)) AS entity_hash_hex,
                   name.value AS name,
                   kind.value AS kind
            FROM entities e
            LEFT JOIN symbol_dict name ON name.id = e.name_id
            LEFT JOIN entity_kind_dict kind ON kind.id = e.kind_id
            WHERE e.entity_hash IN ({blob_literals})
            "
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                let hex: String = row.get("entity_hash_hex")?;
                Ok(AuditEndpoint {
                    id: format!("repo://e/{hex}"),
                    name: row.get("name")?,
                    qualified_name: None,
                    kind: row.get("kind")?,
                })
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            let endpoint = row.map_err(|error| error.to_string())?;
            endpoints.insert(endpoint.id.clone(), endpoint);
        }
    }

    if !fallback_ids.is_empty() {
        let placeholders = sql_placeholders(fallback_ids.len());
        let sql = format!(
            "
            SELECT object_id.value AS id,
                   name.value AS name,
                   qname.value AS qualified_name,
                   kind.value AS kind
            FROM object_id_lookup object_id
            LEFT JOIN entities entity ON entity.id_key = object_id.id
            LEFT JOIN symbol_dict name ON name.id = entity.name_id
            LEFT JOIN qualified_name_lookup qname ON qname.id = entity.qualified_name_id
            LEFT JOIN entity_kind_dict kind ON kind.id = entity.kind_id
            WHERE object_id.value IN ({placeholders})
            "
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(fallback_ids.iter()), |row| {
                Ok(AuditEndpoint {
                    id: row.get("id")?,
                    name: row.get("name")?,
                    qualified_name: row.get("qualified_name")?,
                    kind: row.get("kind")?,
                })
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            let endpoint = row.map_err(|error| error.to_string())?;
            endpoints.insert(endpoint.id.clone(), endpoint);
        }
    }
    Ok(endpoints)
}

fn repo_entity_hash_hex(id: &str) -> Option<String> {
    let hex = id.strip_prefix("repo://e/")?;
    if matches!(hex.len(), 32 | 64) && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(hex.to_ascii_lowercase())
    } else {
        None
    }
}

fn endpoint_from_map(endpoints: &BTreeMap<String, AuditEndpoint>, id: &str) -> AuditEndpoint {
    endpoints.get(id).cloned().unwrap_or_else(|| AuditEndpoint {
        id: id.to_string(),
        name: None,
        qualified_name: None,
        kind: None,
    })
}

fn path_edges_from_stored_metadata(
    row: &StoredPathRow,
    materialized_edges: &[StoredPathEdgeRow],
    source_spans: &[AuditSourceSpan],
    metadata: &Value,
    endpoints: &BTreeMap<String, AuditEndpoint>,
) -> Vec<PathEdgeSample> {
    let edge_labels = metadata
        .get("edge_labels")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let fallback_triples = parse_edge_triples(&row.edges_json);
    let mut edges = if materialized_edges.is_empty() {
        fallback_triples
            .iter()
            .enumerate()
            .map(|(ordinal, (head, relation, tail))| PathStoredEdgeLike {
                edge_id: None,
                head_id: head.clone(),
                relation: relation.clone(),
                tail_id: tail.clone(),
                ordinal,
                source_span_path: None,
                exactness: None,
                confidence: None,
                derived: None,
                edge_class: None,
                context: None,
                provenance_edges_json: None,
            })
            .collect::<Vec<_>>()
    } else {
        materialized_edges
            .iter()
            .map(|edge| PathStoredEdgeLike {
                edge_id: Some(edge.edge_id.clone()),
                head_id: edge.head_id.clone(),
                relation: edge.relation.clone(),
                tail_id: edge.tail_id.clone(),
                ordinal: edge.ordinal,
                source_span_path: edge.source_span_path.clone(),
                exactness: edge.exactness.clone(),
                confidence: edge.confidence,
                derived: edge.derived,
                edge_class: edge.edge_class.clone(),
                context: edge.context.clone(),
                provenance_edges_json: edge.provenance_edges_json.clone(),
            })
            .collect::<Vec<_>>()
    };
    edges.sort_by_key(|edge| edge.ordinal);
    edges
        .into_iter()
        .map(|edge| {
            let label = edge_label_for(&edge_labels, edge.ordinal, edge.edge_id.as_deref());
            let source_span = source_spans.get(edge.ordinal).cloned().or_else(|| {
                edge.source_span_path.as_ref().map(|path| AuditSourceSpan {
                    repo_relative_path: path.clone(),
                    start_line: 0,
                    start_column: None,
                    end_line: 0,
                    end_column: None,
                })
            });
            let exactness = edge
                .exactness
                .clone()
                .or_else(|| {
                    label
                        .and_then(|value| value.get("exactness"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .or_else(|| Some(row.exactness.clone()));
            let confidence = edge
                .confidence
                .or_else(|| {
                    label
                        .and_then(|value| value.get("confidence"))
                        .and_then(Value::as_f64)
                })
                .or(Some(row.confidence));
            let derived = edge.derived.or_else(|| {
                label
                    .and_then(|value| value.get("derived"))
                    .and_then(Value::as_bool)
            });
            let provenance_edges = edge
                .provenance_edges_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
                .or_else(|| {
                    label
                        .and_then(|value| value.get("provenance_edges"))
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect::<Vec<_>>()
                        })
                })
                .unwrap_or_default();
            let context = edge
                .context
                .clone()
                .or_else(|| {
                    label
                        .and_then(|value| value.get("context"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| {
                    infer_context(
                        &edge.relation,
                        source_span
                            .as_ref()
                            .map(|span| span.repo_relative_path.as_str())
                            .unwrap_or(""),
                        &edge.head_id,
                        &edge.tail_id,
                    )
                });
            let fact_classification = edge
                .edge_class
                .clone()
                .or_else(|| {
                    label
                        .and_then(|value| {
                            value
                                .get("fact_class")
                                .or_else(|| value.get("edge_class"))
                                .and_then(Value::as_str)
                        })
                        .map(str::to_string)
                })
                .unwrap_or_else(|| {
                    classify_edge_fact(
                        &edge.relation,
                        exactness.as_deref().unwrap_or("unknown"),
                        derived.unwrap_or(false),
                        &context,
                    )
                });
            let head = endpoint_from_map(endpoints, &edge.head_id);
            let tail = endpoint_from_map(endpoints, &edge.tail_id);
            let mut missing_metadata = Vec::new();
            if edge.edge_id.is_none() {
                missing_metadata.push("edge_id_missing_from_materialized_path".to_string());
            }
            if source_span.is_none() {
                missing_metadata.push("edge_source_span_missing".to_string());
            }
            if label.is_none() && edge.exactness.is_none() && edge.edge_class.is_none() {
                missing_metadata.push("edge_label_metadata_missing".to_string());
            }
            PathEdgeSample {
                edge_id: edge.edge_id,
                head,
                relation: edge.relation,
                tail,
                source_span,
                relation_direction: "head_to_tail".to_string(),
                exactness,
                confidence,
                derived,
                fact_classification,
                production_test_mock_context: context,
                provenance_edges,
                missing_metadata,
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
struct PathStoredEdgeLike {
    edge_id: Option<String>,
    head_id: String,
    relation: String,
    tail_id: String,
    ordinal: usize,
    source_span_path: Option<String>,
    exactness: Option<String>,
    confidence: Option<f64>,
    derived: Option<bool>,
    edge_class: Option<String>,
    context: Option<String>,
    provenance_edges_json: Option<String>,
}

fn edge_label_for<'a>(
    labels: &'a [Value],
    ordinal: usize,
    edge_id: Option<&str>,
) -> Option<&'a Value> {
    if let Some(edge_id) = edge_id {
        if let Some(label) = labels.iter().find(|value| {
            value
                .get("edge_id")
                .and_then(Value::as_str)
                .is_some_and(|candidate| candidate == edge_id)
        }) {
            return Some(label);
        }
    }
    labels.get(ordinal)
}

fn source_span_cache_key(span: &AuditSourceSpan) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        span.repo_relative_path,
        span.start_line,
        span.start_column.unwrap_or(0),
        span.end_line,
        span.end_column.unwrap_or(0)
    )
}

fn collect_string_rows<I>(rows: I) -> Result<Vec<String>, String>
where
    I: IntoIterator<Item = rusqlite::Result<String>>,
{
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|error| error.to_string())?);
    }
    Ok(out)
}

fn sql_placeholders(count: usize) -> String {
    (0..count).map(|_| "?").collect::<Vec<_>>().join(", ")
}

fn path_sampler_query_plans(
    connection: &Connection,
    options: &SamplePathsOptions,
) -> Result<Vec<PathSamplerQueryPlan>, String> {
    if !table_exists(connection, "path_evidence")? {
        return Ok(Vec::new());
    }
    let limit = options.limit.max(1);
    let edge_limit = options.max_edge_load.max(1);
    let queries = vec![
        (
            "candidate_path_ids_rowid",
            format!(
                "SELECT id FROM path_evidence WHERE rowid >= 1 ORDER BY rowid LIMIT {limit}"
            ),
        ),
        (
            "path_rows_by_id",
            "SELECT id, source, target, summary, metapath_json, edges_json, source_spans_json, exactness, length, confidence, metadata_json FROM path_evidence WHERE id IN ('path://sample')".to_string(),
        ),
        (
            "path_edges_by_path_id",
            format!(
                "SELECT path_id, ordinal, edge_id, head_id, relation, tail_id, source_span_path, exactness, confidence, derived, edge_class, context, provenance_edges_json FROM path_evidence_edges WHERE path_id IN ('path://sample') ORDER BY path_id, ordinal LIMIT {edge_limit}"
            ),
        ),
        (
            "endpoint_batch_lookup",
            "SELECT lower(hex(e.entity_hash)) AS entity_hash_hex, name.value AS name, kind.value AS kind FROM entities e LEFT JOIN symbol_dict name ON name.id = e.name_id LEFT JOIN entity_kind_dict kind ON kind.id = e.kind_id WHERE e.entity_hash IN (X'00000000000000000000000000000000')".to_string(),
        ),
    ];
    let mut plans = Vec::new();
    for (name, sql) in queries {
        let plan = match explain_query_plan(connection, &sql) {
            Ok(plan) => plan,
            Err(error) => vec![QueryPlanRow {
                id: -1,
                parent: -1,
                detail: format!("EXPLAIN failed: {error}"),
            }],
        };
        plans.push(PathSamplerQueryPlan {
            name: name.to_string(),
            sql,
            query_plan_analysis: analyze_query_plan(&plan),
            explain_query_plan: plan,
        });
    }
    Ok(plans)
}

fn path_evidence_index_status(
    connection: &Connection,
) -> Result<Vec<PathEvidenceIndexStatus>, String> {
    let checks = [
        (
            "path_evidence",
            "path_evidence(id primary key; logical path_id)",
            "sqlite_autoindex_path_evidence_1",
            vec!["id"],
            "Candidate path IDs are selected before detail loading; path_evidence.id is the logical path_id.",
        ),
        (
            "path_evidence_edges",
            "path_evidence_edges(path_id, ordinal)",
            "idx_path_evidence_edges_path_ordinal",
            vec!["path_id", "ordinal"],
            "Used to load ordered edge rows for already selected path IDs.",
        ),
        (
            "path_evidence_symbols",
            "path_evidence_symbols(path_id, entity_id)",
            "idx_path_evidence_symbols_path",
            vec!["path_id", "entity_id"],
            "Used by context packet lookup and available for path-centered sampling joins.",
        ),
        (
            "path_evidence_files",
            "path_evidence_files(file_id, path_id)",
            "idx_path_evidence_files_file",
            vec!["file_id", "path_id"],
            "Used for file-scoped invalidation and file-centered path audits.",
        ),
        (
            "path_evidence_tests",
            "path_evidence_tests(path_id, test_id)",
            "sqlite_autoindex_path_evidence_tests_1",
            vec!["path_id", "test_id", "relation"],
            "Primary key begins with path_id, test_id; this satisfies path-scoped test lookup.",
        ),
    ];
    let mut out = Vec::new();
    for (object, required_shape, index_name, expected_columns, notes) in checks {
        let index_present = sqlite_master_exists(connection, "index", index_name).unwrap_or(false);
        let table_present = sqlite_master_exists(connection, "table", object).unwrap_or(false);
        let primary_key_satisfier = match object {
            "path_evidence_edges" => Some("PRIMARY KEY(path_id, ordinal) WITHOUT ROWID"),
            "path_evidence_tests" => Some("PRIMARY KEY(path_id, test_id, relation) WITHOUT ROWID"),
            _ => None,
        };
        let present = index_present || (table_present && primary_key_satisfier.is_some());
        let columns = if index_present {
            index_columns(connection, index_name).unwrap_or_default()
        } else {
            expected_columns
                .iter()
                .map(|column| (*column).to_string())
                .collect()
        };
        out.push(PathEvidenceIndexStatus {
            object: object.to_string(),
            required_shape: required_shape.to_string(),
            present,
            satisfied_by: if index_present {
                Some(index_name.to_string())
            } else if present {
                primary_key_satisfier.map(str::to_string)
            } else {
                None
            },
            columns,
            notes: notes.to_string(),
        });
    }
    Ok(out)
}

fn path_sample_from_stored_row(
    repo_roots: &[PathBuf],
    row: StoredPathRow,
    materialized_edges: &[StoredPathEdgeRow],
    endpoints: &BTreeMap<String, AuditEndpoint>,
    ordinal: usize,
    include_snippets: bool,
    snippet_cache: &mut BTreeMap<String, Result<SourceSnippet, String>>,
    timing: &mut PathSamplerTiming,
) -> Result<PathEvidenceSample, String> {
    let metadata = serde_json::from_str::<Value>(&row.metadata_json).unwrap_or_else(|_| json!({}));
    let relation_sequence = parse_string_array(&row.metapath_json);
    let source_spans = parse_source_spans(&row.source_spans_json);
    let edge_list = path_edges_from_stored_metadata(
        &row,
        materialized_edges,
        &source_spans,
        &metadata,
        endpoints,
    );
    let snippet_started = Instant::now();
    let source_snippets = if include_snippets {
        source_spans
            .iter()
            .filter_map(|span| {
                let key = source_span_cache_key(span);
                let entry = snippet_cache
                    .entry(key)
                    .or_insert_with(|| load_source_snippet(repo_roots, span));
                entry.as_ref().ok().cloned()
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    timing.snippet_load_ms = timing
        .snippet_load_ms
        .saturating_add(elapsed_ms(snippet_started));
    let context = infer_path_context(&edge_list, &source_spans);
    let derived_label = derived_provenance_label(&edge_list);
    let source = endpoint_from_map(endpoints, &row.source);
    let target = endpoint_from_map(endpoints, &row.target);
    let missing_metadata = missing_path_metadata(
        &source,
        &target,
        &edge_list,
        &source_spans,
        source_snippets.len(),
        include_snippets,
        &metadata,
    );

    Ok(PathEvidenceSample {
        ordinal,
        path_id: row.id,
        generated_by_audit: false,
        task_or_query: task_or_query_from_metadata(&metadata),
        summary: row.summary,
        source,
        target,
        relation_sequence,
        edge_list,
        source_spans,
        source_snippets,
        exactness: row.exactness,
        confidence: row.confidence,
        derived_provenance_label: derived_label,
        production_test_mock_context: context,
        metadata,
        missing_metadata,
    })
}

fn generated_path_samples(
    _connection: &Connection,
    repo_roots: &[PathBuf],
    options: &SamplePathsOptions,
    ordinal_offset: usize,
) -> Result<Vec<PathEvidenceSample>, String> {
    let edge_options = SampleEdgesOptions {
        db_path: options.db_path.clone(),
        relation: None,
        limit: options.limit.saturating_sub(ordinal_offset),
        seed: options.seed,
        json_path: None,
        markdown_path: None,
        include_snippets: options.include_snippets,
    };
    let edge_report = sample_edges(&edge_options)?;
    let mut samples = Vec::new();
    for edge in edge_report.samples {
        let source_spans = vec![edge.source_span.clone()];
        let source_snippets = edge.source_snippet.clone().into_iter().collect::<Vec<_>>();
        let edge_step = PathEdgeSample {
            edge_id: Some(edge.edge_id.clone()),
            head: edge.head.clone(),
            relation: edge.relation.clone(),
            tail: edge.tail.clone(),
            source_span: Some(edge.source_span.clone()),
            relation_direction: edge.relation_direction.clone(),
            exactness: Some(edge.exactness.clone()),
            confidence: Some(edge.confidence),
            derived: Some(edge.derived),
            fact_classification: edge.fact_classification.clone(),
            production_test_mock_context: edge.production_test_mock_context.clone(),
            provenance_edges: edge.provenance_edges.clone(),
            missing_metadata: edge.missing_metadata.clone(),
        };
        let context = infer_path_context(std::slice::from_ref(&edge_step), &source_spans);
        let derived_label = derived_provenance_label(std::slice::from_ref(&edge_step));
        let mut missing_metadata = missing_path_metadata(
            &edge.head,
            &edge.tail,
            std::slice::from_ref(&edge_step),
            &source_spans,
            source_snippets.len(),
            options.include_snippets,
            &edge.metadata,
        );
        if options.include_snippets && source_snippets.is_empty() {
            let _ = repo_roots;
            missing_metadata.push("source_snippet_unavailable".to_string());
        }
        samples.push(PathEvidenceSample {
            ordinal: ordinal_offset + samples.len() + 1,
            path_id: format!("generated://audit/{}", edge.edge_id),
            generated_by_audit: true,
            task_or_query: None,
            summary: Some(format!(
                "Generated one-edge audit path: {} -{}-> {}",
                edge.head.id, edge.relation, edge.tail.id
            )),
            source: edge.head,
            target: edge.tail,
            relation_sequence: vec![edge.relation],
            edge_list: vec![edge_step],
            source_spans,
            source_snippets,
            exactness: edge.exactness,
            confidence: edge.confidence,
            derived_provenance_label: derived_label,
            production_test_mock_context: context,
            metadata: edge.metadata,
            missing_metadata,
        });
    }
    Ok(samples)
}

fn relation_counts(db_path: &Path) -> Result<RelationCountsReport, String> {
    let connection = open_read_only(db_path)?;
    let rows = relation_count_rows(&connection)?;
    let total_edges = rows.values().map(|row| row.edge_count).sum();
    let mut relations = rows.into_values().collect::<Vec<_>>();
    relations.sort_by(|left, right| {
        right
            .edge_count
            .cmp(&left.edge_count)
            .then_with(|| left.relation.cmp(&right.relation))
    });
    Ok(RelationCountsReport {
        schema_version: AUDIT_SCHEMA_VERSION,
        db_path: path_string(db_path),
        relation_count: relations.len(),
        total_edges,
        relations,
        notes: vec![
            "Fast relation-counts reads only the edge table plus relation dictionary; inline source span columns are mandatory in the current schema.".to_string(),
            "Fast relation-counts intentionally skips source_spans row joins, exactness grouping, duplicate grouping, and top entity-type grouping on large DBs; those fields are marked not_measured_fast_path.".to_string(),
            "Context breakdown is not measured in the fast relation-count summary because runtime context is not a first-class edge column; inspect sampled edges for inferred production/test/mock labels.".to_string(),
        ],
    })
}

fn label_samples(options: &LabelSamplesOptions) -> Result<LabelSamplesReport, String> {
    let mut samples = Vec::new();
    let mut markdown_inputs = Vec::new();

    for edge_json in &options.edge_json_paths {
        let markdown_path = matching_markdown_path(
            edge_json,
            &options.edge_markdown_paths,
            options.edge_json_paths.len(),
        );
        if let Some(path) = &markdown_path {
            markdown_inputs.push(path_string(path));
        }
        samples.extend(labeled_edge_samples_from_report(
            edge_json,
            markdown_path.as_deref(),
        )?);
    }

    for path_json in &options.path_json_paths {
        let markdown_path = matching_markdown_path(
            path_json,
            &options.path_markdown_paths,
            options.path_json_paths.len(),
        );
        if let Some(path) = &markdown_path {
            markdown_inputs.push(path_string(path));
        }
        samples.extend(labeled_path_samples_from_report(
            path_json,
            markdown_path.as_deref(),
        )?);
    }

    samples.sort_by(|left, right| {
        (
            left.sample_type.as_str(),
            left.source_json.as_str(),
            left.ordinal,
            left.sample_id.as_str(),
        )
            .cmp(&(
                right.sample_type.as_str(),
                right.source_json.as_str(),
                right.ordinal,
                right.sample_id.as_str(),
            ))
    });
    let summary = summarize_labeled_samples(&samples);
    let mut notes = vec![
        "Manual labels are read from edited sample markdown bullets or from sample JSON manual_labels/labels objects.".to_string(),
        "Blank labels remain unlabeled and are excluded from precision denominators.".to_string(),
        "Recall is unknown unless a separate gold false-negative denominator is supplied; sampled positives only estimate precision.".to_string(),
    ];
    if summary.labeled_samples == 0 {
        notes.push("No human labels were found in the supplied inputs.".to_string());
    }
    Ok(LabelSamplesReport {
        schema_version: AUDIT_SCHEMA_VERSION,
        label_schema_version: LABEL_SCHEMA_VERSION,
        generated_at_unix_ms: now_unix_ms(),
        edge_inputs: options.edge_json_paths.iter().map(path_string).collect(),
        path_inputs: options.path_json_paths.iter().map(path_string).collect(),
        markdown_inputs,
        manual_classification_options: SAMPLE_CLASSIFICATIONS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        samples,
        summary,
        notes,
    })
}

fn summarize_label_inputs(options: &SummarizeLabelsOptions) -> Result<LabelSamplesReport, String> {
    let mut label_paths = options.label_paths.clone();
    if let Some(dir) = &options.label_dir {
        for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("json") {
                label_paths.push(path);
            }
        }
    }
    label_paths.sort();
    label_paths.dedup();
    if label_paths.is_empty() {
        return Err("audit summarize-labels found no label JSON inputs".to_string());
    }

    let mut samples = Vec::new();
    let mut edge_inputs = Vec::new();
    let mut path_inputs = Vec::new();
    let mut markdown_inputs = Vec::new();
    let mut notes = Vec::new();
    for path in &label_paths {
        let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let report: LabelSamplesReport =
            serde_json::from_str(&text).map_err(|error| error.to_string())?;
        edge_inputs.extend(report.edge_inputs);
        path_inputs.extend(report.path_inputs);
        markdown_inputs.extend(report.markdown_inputs);
        notes.extend(report.notes);
        samples.extend(report.samples);
    }
    samples.sort_by(|left, right| {
        (
            left.sample_type.as_str(),
            left.source_json.as_str(),
            left.ordinal,
            left.sample_id.as_str(),
        )
            .cmp(&(
                right.sample_type.as_str(),
                right.source_json.as_str(),
                right.ordinal,
                right.sample_id.as_str(),
            ))
    });
    let summary = summarize_labeled_samples(&samples);
    notes.push(format!(
        "Aggregated {} labeled-sample file(s).",
        label_paths.len()
    ));
    notes.sort();
    notes.dedup();
    Ok(LabelSamplesReport {
        schema_version: AUDIT_SCHEMA_VERSION,
        label_schema_version: LABEL_SCHEMA_VERSION,
        generated_at_unix_ms: now_unix_ms(),
        edge_inputs,
        path_inputs,
        markdown_inputs,
        manual_classification_options: SAMPLE_CLASSIFICATIONS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        samples,
        summary,
        notes,
    })
}

fn labeled_edge_samples_from_report(
    json_path: &Path,
    markdown_path: Option<&Path>,
) -> Result<Vec<LabeledSample>, String> {
    let text = fs::read_to_string(json_path).map_err(|error| error.to_string())?;
    let raw: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let report: EdgeSampleReport =
        serde_json::from_value(raw.clone()).map_err(|error| error.to_string())?;
    let json_labels = json_sample_labels_by_ordinal(&raw);
    let markdown_labels = read_markdown_labels(markdown_path)?;
    let source_json = path_string(json_path);
    let source_markdown = markdown_path.map(path_string);
    let mut out = Vec::new();
    for sample in report.samples {
        let labels = choose_labels(
            json_labels.get(&sample.ordinal),
            markdown_labels.get(&sample.ordinal),
        );
        out.push(LabeledSample {
            sample_type: "edge".to_string(),
            source_json: source_json.clone(),
            source_markdown: source_markdown.clone(),
            ordinal: sample.ordinal,
            sample_id: sample.edge_id.clone(),
            relation: sample.relation.clone(),
            relation_sequence: vec![sample.relation.clone()],
            edge_ids: vec![sample.edge_id],
            exactness: Some(sample.exactness),
            confidence: Some(sample.confidence),
            source_span_count: 1,
            span_loaded: Some(sample.span_loaded),
            fact_classification: Some(sample.fact_classification),
            production_test_mock_context: Some(sample.production_test_mock_context),
            labeled: labels.has_any_signal(),
            labels,
        });
    }
    Ok(out)
}

fn labeled_path_samples_from_report(
    json_path: &Path,
    markdown_path: Option<&Path>,
) -> Result<Vec<LabeledSample>, String> {
    let text = fs::read_to_string(json_path).map_err(|error| error.to_string())?;
    let raw: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let report: PathEvidenceSampleReport =
        serde_json::from_value(raw.clone()).map_err(|error| error.to_string())?;
    let json_labels = json_sample_labels_by_ordinal(&raw);
    let markdown_labels = read_markdown_labels(markdown_path)?;
    let source_json = path_string(json_path);
    let source_markdown = markdown_path.map(path_string);
    let mut out = Vec::new();
    for sample in report.samples {
        let labels = choose_labels(
            json_labels.get(&sample.ordinal),
            markdown_labels.get(&sample.ordinal),
        );
        let relation_sequence = sample.relation_sequence;
        let edge_ids = sample
            .edge_list
            .iter()
            .filter_map(|edge| edge.edge_id.clone())
            .collect::<Vec<_>>();
        let span_loaded = if sample.source_spans.is_empty() {
            None
        } else {
            Some(!sample.source_snippets.is_empty())
        };
        out.push(LabeledSample {
            sample_type: "path".to_string(),
            source_json: source_json.clone(),
            source_markdown: source_markdown.clone(),
            ordinal: sample.ordinal,
            sample_id: sample.path_id,
            relation: "PathEvidence".to_string(),
            relation_sequence,
            edge_ids,
            exactness: Some(sample.exactness),
            confidence: Some(sample.confidence),
            source_span_count: sample.source_spans.len(),
            span_loaded,
            fact_classification: Some(sample.derived_provenance_label),
            production_test_mock_context: Some(sample.production_test_mock_context),
            labeled: labels.has_any_signal(),
            labels,
        });
    }
    Ok(out)
}

fn matching_markdown_path(
    json_path: &Path,
    explicit_paths: &[PathBuf],
    json_count: usize,
) -> Option<PathBuf> {
    let json_stem = json_path.file_stem().and_then(|value| value.to_str());
    for path in explicit_paths {
        if path.file_stem().and_then(|value| value.to_str()) == json_stem {
            return Some(path.clone());
        }
    }
    if json_count == 1 && explicit_paths.len() == 1 {
        return explicit_paths.first().cloned();
    }
    let sibling = json_path.with_extension("md");
    if sibling.exists() {
        return Some(sibling);
    }
    None
}

fn read_markdown_labels(
    markdown_path: Option<&Path>,
) -> Result<BTreeMap<usize, ManualLabelSet>, String> {
    let Some(path) = markdown_path else {
        return Ok(BTreeMap::new());
    };
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    Ok(parse_markdown_labels(&text))
}

fn parse_markdown_labels(text: &str) -> BTreeMap<usize, ManualLabelSet> {
    let mut labels = BTreeMap::<usize, ManualLabelSet>::new();
    let mut current = None;
    for line in text.lines() {
        if let Some(ordinal) = markdown_sample_ordinal(line) {
            current = Some(ordinal);
            labels.entry(ordinal).or_default();
            continue;
        }
        let Some(ordinal) = current else {
            continue;
        };
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("- ") else {
            continue;
        };
        let Some((key, value)) = rest.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_matches('`');
        let value = value.trim().trim_matches('`').trim();
        if let Some(label_set) = labels.get_mut(&ordinal) {
            label_set.apply_markdown_field(key, value);
        }
    }
    labels
}

fn markdown_sample_ordinal(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    if !trimmed.starts_with("## ") || !trimmed.contains("Sample") {
        return None;
    }
    trimmed
        .split_whitespace()
        .rev()
        .find_map(|token| token.trim_matches('#').parse::<usize>().ok())
}

fn json_sample_labels_by_ordinal(raw: &Value) -> BTreeMap<usize, ManualLabelSet> {
    let mut out = BTreeMap::new();
    let Some(samples) = raw.get("samples").and_then(Value::as_array) else {
        return out;
    };
    for sample in samples {
        let Some(ordinal) = sample.get("ordinal").and_then(Value::as_u64) else {
            continue;
        };
        let labels = manual_labels_from_json_sample(sample);
        if labels.has_any_signal() {
            out.insert(ordinal as usize, labels);
        }
    }
    out
}

fn manual_labels_from_json_sample(sample: &Value) -> ManualLabelSet {
    let mut labels = ManualLabelSet::default();
    labels.apply_json_fields(sample);
    if let Some(manual_labels) = sample.get("manual_labels") {
        labels.apply_json_fields(manual_labels);
    }
    if let Some(manual_labels) = sample.get("labels") {
        labels.apply_json_fields(manual_labels);
    }
    labels
}

fn choose_labels(
    json_labels: Option<&ManualLabelSet>,
    markdown_labels: Option<&ManualLabelSet>,
) -> ManualLabelSet {
    if let Some(markdown) = markdown_labels {
        if markdown.has_any_signal() {
            return markdown.clone();
        }
    }
    json_labels.cloned().unwrap_or_default()
}

fn summarize_labeled_samples(samples: &[LabeledSample]) -> LabelSummary {
    #[derive(Default)]
    struct RelationStats {
        labeled_samples: u64,
        unlabeled_samples: u64,
        unsupported_samples: u64,
        unsure_samples: u64,
        true_positive: u64,
        false_positive: u64,
        wrong_span: u64,
    }

    let mut relation_stats = BTreeMap::<String, RelationStats>::new();
    let mut false_positive_taxonomy = BTreeMap::<String, u64>::new();
    let mut wrong_span_taxonomy = BTreeMap::<String, u64>::new();
    let mut unsupported_pattern_taxonomy = BTreeMap::<String, u64>::new();
    let mut source_span = PrecisionEstimate {
        eligible_samples: 0,
        true_positive: 0,
        false_positive: 0,
        wrong_span: 0,
        precision: None,
        recall: None,
        recall_status: "unknown_no_gold_false_negative_denominator".to_string(),
    };
    let mut edge_samples = 0u64;
    let mut path_samples = 0u64;
    let mut labeled_samples = 0u64;
    let mut unlabeled_samples = 0u64;
    let mut unsupported_samples = 0u64;
    let mut unsure_samples = 0u64;

    for sample in samples {
        if sample.sample_type == "edge" {
            edge_samples += 1;
        } else if sample.sample_type == "path" {
            path_samples += 1;
        }
        let labels = &sample.labels;
        let stats = relation_stats.entry(sample.relation.clone()).or_default();
        if !sample.labeled {
            stats.unlabeled_samples += 1;
            unlabeled_samples += 1;
            continue;
        }
        stats.labeled_samples += 1;
        labeled_samples += 1;
        if labels.is_unsupported() {
            stats.unsupported_samples += 1;
            unsupported_samples += 1;
            let category = labels
                .unsupported_pattern
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("unsupported_unspecified");
            increment_taxonomy(&mut unsupported_pattern_taxonomy, category);
            continue;
        }
        if labels.unsure {
            stats.unsure_samples += 1;
            unsure_samples += 1;
            continue;
        }
        if labels.relation_is_false_positive() {
            stats.false_positive += 1;
            for category in labels.false_positive_categories() {
                increment_taxonomy(&mut false_positive_taxonomy, &category);
            }
        } else if labels.relation_is_true_positive() {
            stats.true_positive += 1;
        }
        if labels.wrong_span {
            stats.wrong_span += 1;
            let category = labels
                .wrong_span_cause
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("wrong_span_unspecified");
            increment_taxonomy(&mut wrong_span_taxonomy, category);
        }
        if sample.source_span_count > 0 || sample.span_loaded.is_some() {
            source_span.eligible_samples += 1;
            if labels.wrong_span {
                source_span.wrong_span += 1;
                source_span.false_positive += 1;
            } else {
                source_span.true_positive += 1;
            }
        }
    }

    source_span.precision = precision(
        source_span.true_positive,
        source_span.true_positive + source_span.false_positive,
    );

    let relation_precision = relation_stats
        .into_iter()
        .map(|(relation, stats)| {
            let denominator = stats.true_positive + stats.false_positive;
            RelationPrecisionSummary {
                relation,
                labeled_samples: stats.labeled_samples,
                unlabeled_samples: stats.unlabeled_samples,
                unsupported_samples: stats.unsupported_samples,
                unsure_samples: stats.unsure_samples,
                true_positive: stats.true_positive,
                false_positive: stats.false_positive,
                wrong_span: stats.wrong_span,
                precision: precision(stats.true_positive, denominator),
                recall: None,
                recall_status: "unknown_no_gold_false_negative_denominator".to_string(),
            }
        })
        .collect::<Vec<_>>();
    let mut notes = Vec::new();
    if labeled_samples == 0 {
        notes.push("No labeled samples found; precision and recall remain unknown.".to_string());
    }
    if unsupported_samples > 0 {
        notes.push(
            "Unsupported samples are excluded from wrong-case precision denominators.".to_string(),
        );
    }
    LabelSummary {
        total_samples: samples.len() as u64,
        edge_samples,
        path_samples,
        labeled_samples,
        unlabeled_samples,
        unsupported_samples,
        unsure_samples,
        relation_precision,
        source_span_precision: source_span,
        false_positive_taxonomy: taxonomy_vec(false_positive_taxonomy),
        wrong_span_taxonomy: taxonomy_vec(wrong_span_taxonomy),
        unsupported_pattern_taxonomy: taxonomy_vec(unsupported_pattern_taxonomy),
        recall_estimate_status: "unknown_no_gold_false_negative_denominator".to_string(),
        notes,
    }
}

fn increment_taxonomy(map: &mut BTreeMap<String, u64>, category: &str) {
    *map.entry(category.trim().to_string()).or_insert(0) += 1;
}

fn taxonomy_vec(map: BTreeMap<String, u64>) -> Vec<TaxonomyCount> {
    map.into_iter()
        .map(|(category, count)| TaxonomyCount { category, count })
        .collect()
}

fn precision(numerator: u64, denominator: u64) -> Option<f64> {
    if denominator == 0 {
        None
    } else {
        Some(((numerator as f64 / denominator as f64) * 10_000.0).round() / 10_000.0)
    }
}

impl ManualLabelSet {
    fn has_any_signal(&self) -> bool {
        self.true_positive
            || self.false_positive
            || self.wrong_direction
            || self.wrong_target
            || self.wrong_span
            || self.stale
            || self.duplicate
            || self.unresolved_mislabeled_exact
            || self.test_mock_leaked
            || self.derived_missing_provenance
            || self.unsure
            || self.is_unsupported()
            || self.false_positive_cause.as_deref().is_some_and(non_empty)
            || self.wrong_span_cause.as_deref().is_some_and(non_empty)
            || self.notes.as_deref().is_some_and(non_empty)
    }

    fn is_unsupported(&self) -> bool {
        self.unsupported || self.unsupported_pattern.as_deref().is_some_and(non_empty)
    }

    fn relation_is_false_positive(&self) -> bool {
        self.false_positive
            || self.wrong_direction
            || self.wrong_target
            || self.stale
            || self.duplicate
            || self.unresolved_mislabeled_exact
            || self.test_mock_leaked
            || self.derived_missing_provenance
    }

    fn relation_is_true_positive(&self) -> bool {
        self.true_positive && !self.relation_is_false_positive()
    }

    fn false_positive_categories(&self) -> Vec<String> {
        let mut categories = Vec::new();
        if self.false_positive {
            categories.push("false_positive".to_string());
        }
        if self.wrong_direction {
            categories.push("wrong_direction".to_string());
        }
        if self.wrong_target {
            categories.push("wrong_target".to_string());
        }
        if self.stale {
            categories.push("stale".to_string());
        }
        if self.duplicate {
            categories.push("duplicate".to_string());
        }
        if self.unresolved_mislabeled_exact {
            categories.push("unresolved_mislabeled_exact".to_string());
        }
        if self.test_mock_leaked {
            categories.push("test_mock_leaked".to_string());
        }
        if self.derived_missing_provenance {
            categories.push("derived_missing_provenance".to_string());
        }
        if let Some(cause) = self
            .false_positive_cause
            .as_deref()
            .filter(|value| non_empty(value))
        {
            categories.push(format!("cause:{cause}"));
        }
        categories
    }

    fn apply_markdown_field(&mut self, key: &str, value: &str) {
        let trimmed_key = key.trim();
        let checked_key = trimmed_key.to_ascii_lowercase().starts_with("[x]");
        let key = trimmed_key
            .trim_start_matches("[x] ")
            .trim_start_matches("[X] ")
            .trim_start_matches("[ ] ")
            .trim();
        if SAMPLE_CLASSIFICATIONS.contains(&key) {
            self.set_classification(key, checked_key || parse_boolish_str(value));
            return;
        }
        match key {
            "unsupported" => self.unsupported = parse_boolish_str(value),
            "false_positive_cause" => self.false_positive_cause = optional_label_string(value),
            "wrong_span_cause" => self.wrong_span_cause = optional_label_string(value),
            "unsupported_pattern" => self.unsupported_pattern = optional_label_string(value),
            "notes" => self.notes = optional_label_string(value),
            _ => {}
        }
    }

    fn apply_json_fields(&mut self, value: &Value) {
        for key in SAMPLE_CLASSIFICATIONS {
            if let Some(label) = value.get(*key).and_then(parse_boolish_json) {
                self.set_classification(key, label);
            }
        }
        if let Some(label) = value.get("unsupported").and_then(parse_boolish_json) {
            self.unsupported = label;
        }
        if let Some(label) = value.get("false_positive_cause").and_then(json_stringish) {
            self.false_positive_cause = optional_label_string(&label);
        }
        if let Some(label) = value.get("wrong_span_cause").and_then(json_stringish) {
            self.wrong_span_cause = optional_label_string(&label);
        }
        if let Some(label) = value.get("unsupported_pattern").and_then(json_stringish) {
            self.unsupported_pattern = optional_label_string(&label);
        }
        if let Some(label) = value.get("notes").and_then(json_stringish) {
            self.notes = optional_label_string(&label);
        }
    }

    fn set_classification(&mut self, key: &str, value: bool) {
        match key {
            "true_positive" => self.true_positive = value,
            "false_positive" => self.false_positive = value,
            "wrong_direction" => self.wrong_direction = value,
            "wrong_target" => self.wrong_target = value,
            "wrong_span" => self.wrong_span = value,
            "stale" => self.stale = value,
            "duplicate" => self.duplicate = value,
            "unresolved_mislabeled_exact" => self.unresolved_mislabeled_exact = value,
            "test_mock_leaked" => self.test_mock_leaked = value,
            "derived_missing_provenance" => self.derived_missing_provenance = value,
            "unsure" => self.unsure = value,
            _ => {}
        }
    }
}

fn parse_boolish_json(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::Number(number) => Some(number.as_i64().unwrap_or_default() != 0),
        Value::String(value) => Some(parse_boolish_str(value)),
        _ => None,
    }
}

fn parse_boolish_str(value: &str) -> bool {
    matches!(
        value.trim().trim_matches('`').to_ascii_lowercase().as_str(),
        "true" | "yes" | "y" | "1" | "x" | "[x]" | "checked" | "tp"
    )
}

fn json_stringish(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn optional_label_string(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_matches('`').trim();
    if non_empty(trimmed) {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn relation_count_rows(
    connection: &Connection,
) -> Result<BTreeMap<String, RelationCountRow>, String> {
    let mut fact_selects = vec!["SELECT relation_id, derived FROM edges".to_string()];
    if table_exists(connection, "structural_relations")? {
        fact_selects.push("SELECT relation_id, 0 AS derived FROM structural_relations".to_string());
    }
    if table_exists(connection, "callsites")? {
        fact_selects.push("SELECT relation_id, 0 AS derived FROM callsites".to_string());
    }
    if table_exists(connection, "callsite_args")? {
        fact_selects.push("SELECT relation_id, 0 AS derived FROM callsite_args".to_string());
    }
    if table_exists(connection, "entities")?
        && table_column_exists(connection, "entities", "parent_id")?
        && table_column_exists(connection, "entities", "structural_flags")?
    {
        fact_selects.push(
            "SELECT relation.id AS relation_id, 0 AS derived
             FROM entities e
             JOIN entities parent ON parent.id_key = e.parent_id
             JOIN relation_kind_dict relation ON relation.value = 'CONTAINS'
             WHERE e.parent_id IS NOT NULL
               AND e.span_path_id IS NOT NULL
               AND e.start_line IS NOT NULL
               AND e.end_line IS NOT NULL
               AND e.structural_flags IS NOT NULL
               AND (e.structural_flags & 2) != 0"
                .to_string(),
        );
        fact_selects.push(
            "SELECT relation.id AS relation_id, 0 AS derived
             FROM entities e
             JOIN entities parent ON parent.id_key = e.parent_id
             JOIN relation_kind_dict relation ON relation.value = 'DEFINED_IN'
             WHERE e.parent_id IS NOT NULL
               AND e.span_path_id IS NOT NULL
               AND e.start_line IS NOT NULL
               AND e.end_line IS NOT NULL
               AND e.structural_flags IS NOT NULL
               AND (e.structural_flags & 4) != 0"
                .to_string(),
        );
        fact_selects.push(
            "SELECT relation.id AS relation_id, 0 AS derived
             FROM entities e
             JOIN entities parent ON parent.id_key = e.parent_id
             JOIN relation_kind_dict relation ON relation.value = 'DECLARES'
             WHERE e.parent_id IS NOT NULL
               AND e.span_path_id IS NOT NULL
               AND e.start_line IS NOT NULL
               AND e.end_line IS NOT NULL
               AND (e.structural_flags & 1) != 0"
                .to_string(),
        );
    }
    let fact_union = fact_selects.join("\nUNION ALL\n");
    let sql = format!(
        r#"
            SELECT relation.value AS relation,
                   COUNT(*) AS edge_count,
                   SUM(CASE WHEN e.derived != 0 THEN 1 ELSE 0 END) AS derived_count
            FROM (
                {fact_union}
            ) e
            JOIN relation_kind_dict relation ON relation.id = e.relation_id
            GROUP BY e.relation_id, relation.value
            ORDER BY relation.value
            "#
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let mapped = statement
        .query_map([], |row| {
            let relation: String = row.get("relation")?;
            let edge_count = row.get::<_, u64>("edge_count")?;
            Ok(RelationCountRow {
                relation: relation.clone(),
                edge_count,
                source_span_count: edge_count,
                missing_source_span_rows: 0,
                source_span_row_status: "not_measured_fast_path_inline_spans_present".to_string(),
                duplicate_edge_count: 0,
                duplicate_edge_count_status: "not_measured_fast_path".to_string(),
                derived_count: row.get("derived_count")?,
                exactness_counts: BTreeMap::new(),
                exactness_count_status: "not_measured_fast_path".to_string(),
                context_breakdown: ContextBreakdown {
                    test_or_fixture_inferred: 0,
                    mock_or_stub_inferred: 0,
                    unknown_not_first_class: edge_count,
                },
                top_head_entity_types: Vec::new(),
                top_tail_entity_types: Vec::new(),
                top_entity_type_status: "not_measured_fast_path".to_string(),
            })
        })
        .map_err(|error| error.to_string())?;

    let mut rows = BTreeMap::new();
    for row in mapped {
        let row = row.map_err(|error| error.to_string())?;
        rows.insert(row.relation.clone(), row);
    }
    Ok(rows)
}

fn open_read_only(db_path: &Path) -> Result<Connection, String> {
    let before = db_file_snapshot(db_path);
    open_read_only_with_snapshot(db_path, &before).map(|read_only| read_only.connection)
}

fn open_read_only_with_snapshot(
    db_path: &Path,
    before: &DbFileSnapshot,
) -> Result<ReadOnlyConnection, String> {
    if !db_path.exists() {
        return Err(format!("database does not exist: {}", db_path.display()));
    }
    let rollback_journal_exists = sqlite_rollback_journal_path(db_path).exists();
    let immutable_mode_used = before.sidecars.is_empty() && !rollback_journal_exists;
    let immutable_mode_reason = if immutable_mode_used {
        "immutable=1 used because no WAL/SHM sidecars or rollback journal were present before inspection".to_string()
    } else if rollback_journal_exists {
        "immutable=1 not used because a rollback journal was present; strict mode=ro preserves lock visibility".to_string()
    } else {
        "immutable=1 not used because WAL/SHM sidecars were present; strict mode=ro preserves WAL visibility".to_string()
    };
    let uri = sqlite_read_only_uri(db_path, immutable_mode_used)?;
    let connection = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|error| format!("failed to open {} read-only: {error}", db_path.display()))?;
    connection
        .pragma_update(None, "query_only", true)
        .map_err(|error| format!("failed to mark {} query-only: {error}", db_path.display()))?;
    Ok(ReadOnlyConnection {
        connection,
        read_only_mode_used: if immutable_mode_used {
            "sqlite_uri_mode_ro_immutable_query_only".to_string()
        } else {
            "sqlite_uri_mode_ro_query_only".to_string()
        },
        immutable_mode_used,
        immutable_mode_reason,
    })
}

fn sqlite_read_only_uri(db_path: &Path, immutable: bool) -> Result<String, String> {
    let absolute = if db_path.is_absolute() {
        db_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(db_path)
    };
    let mut raw = absolute.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = raw.strip_prefix("//?/") {
        raw = stripped.to_string();
    }
    let encoded = percent_encode_sqlite_uri_path(&raw);
    let path = encoded.trim_start_matches('/');
    let immutable_param = if immutable { "&immutable=1" } else { "" };
    Ok(format!("file:///{path}?mode=ro{immutable_param}"))
}

fn sqlite_rollback_journal_path(db_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-journal", db_path.display()))
}

fn percent_encode_sqlite_uri_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        let keep =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':');
        if keep {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn db_file_snapshot(db_path: &Path) -> DbFileSnapshot {
    let main_metadata = fs::metadata(db_path).ok();
    DbFileSnapshot {
        main_db_size: main_metadata.as_ref().map(fs::Metadata::len).unwrap_or(0),
        main_db_mtime_unix_ms: main_metadata
            .and_then(|metadata| metadata.modified().ok())
            .map(system_time_unix_ms),
        main_db_hash: stable_file_hash(db_path),
        sidecars: sqlite_sidecar_snapshots(db_path),
    }
}

fn sqlite_sidecar_snapshots(db_path: &Path) -> Vec<SidecarSnapshot> {
    let mut sidecars = ["wal", "shm"]
        .into_iter()
        .filter_map(|kind| {
            let path = PathBuf::from(format!("{}-{kind}", db_path.display()));
            let metadata = fs::metadata(&path).ok()?;
            Some(SidecarSnapshot {
                kind: kind.to_string(),
                path: path_string(&path),
                size: metadata.len(),
                mtime_unix_ms: metadata.modified().ok().map(system_time_unix_ms),
                hash: stable_file_hash(&path),
            })
        })
        .collect::<Vec<_>>();
    sidecars.sort_by(|left, right| left.kind.cmp(&right.kind));
    sidecars
}

fn read_only_inspection_audit(
    before: DbFileSnapshot,
    after: DbFileSnapshot,
    read_only_mode_used: String,
    immutable_mode_used: bool,
    immutable_mode_reason: String,
) -> ReadOnlyInspectionAudit {
    let main_changed = before.main_db_size != after.main_db_size
        || before.main_db_mtime_unix_ms != after.main_db_mtime_unix_ms
        || before.main_db_hash != after.main_db_hash;
    let sidecars_changed = before.sidecars != after.sidecars;
    let sidecar_status = if main_changed {
        "main_db_changed"
    } else if sidecars_changed {
        "sidecar_only_change"
    } else if after.sidecars.is_empty() {
        "none"
    } else {
        "unchanged"
    }
    .to_string();
    ReadOnlyInspectionAudit {
        main_db_size_before: before.main_db_size,
        main_db_size_after: after.main_db_size,
        main_db_mtime_before: before.main_db_mtime_unix_ms,
        main_db_mtime_after: after.main_db_mtime_unix_ms,
        main_db_hash_before: before.main_db_hash,
        main_db_hash_after: after.main_db_hash,
        main_db_hash_algorithm: "fnv1a64".to_string(),
        sidecars_before: before.sidecars,
        sidecars_after: after.sidecars,
        sidecar_status,
        artifact_mutated_during_inspection: main_changed,
        sidecar_only_change: !main_changed && sidecars_changed,
        read_only_mode_used,
        immutable_mode_used,
        immutable_mode_reason,
    }
}

fn stable_file_hash(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hash = 0xcbf29ce484222325u64;
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    Some(format!("{hash:016x}"))
}

fn system_time_unix_ms(value: SystemTime) -> u64 {
    value
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn page_metrics(connection: &Connection) -> Result<PageMetrics, String> {
    let page_size = pragma_u64(connection, "page_size")?;
    let page_count = pragma_u64(connection, "page_count")?;
    let freelist_count = pragma_u64(connection, "freelist_count")?;
    Ok(PageMetrics {
        page_size_bytes: page_size,
        page_count,
        freelist_count,
        live_page_bytes: page_size.saturating_mul(page_count.saturating_sub(freelist_count)),
        free_page_bytes: page_size.saturating_mul(freelist_count),
    })
}

fn sqlite_integrity_check(connection: &Connection) -> Value {
    let result = (|| -> Result<Vec<String>, String> {
        let mut statement = connection
            .prepare("PRAGMA integrity_check(20)")
            .map_err(|error| error.to_string())?;
        let mapped = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        let mut rows = Vec::new();
        for row in mapped {
            rows.push(row.map_err(|error| error.to_string())?);
        }
        Ok(rows)
    })();
    match result {
        Ok(rows) => {
            let ok = rows.len() == 1 && rows.first().is_some_and(|value| value == "ok");
            json!({
                "status": if ok { "ok" } else { "failed" },
                "checked": true,
                "max_errors": 20,
                "messages": rows,
            })
        }
        Err(error) => json!({
            "status": "error",
            "checked": false,
            "max_errors": 20,
            "error": error,
        }),
    }
}

fn pragma_u64(connection: &Connection, name: &str) -> Result<u64, String> {
    let sql = format!("PRAGMA {name}");
    let value: i64 = connection
        .query_row(&sql, [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    Ok(value.max(0) as u64)
}

fn sqlite_object_types(connection: &Connection) -> Result<BTreeMap<String, String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT name, type FROM sqlite_master WHERE type IN ('table', 'index') ORDER BY name",
        )
        .map_err(|error| error.to_string())?;
    let mapped = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let mut types = BTreeMap::new();
    for row in mapped {
        let (name, object_type) = row.map_err(|error| error.to_string())?;
        types.insert(name, object_type);
    }
    Ok(types)
}

fn dbstat_objects(
    connection: &Connection,
    object_types: &BTreeMap<String, String>,
    database_bytes: u64,
) -> Result<(bool, Vec<StorageObjectSize>), String> {
    let mut statement = match connection.prepare(
        r#"
        SELECT name,
               COUNT(*) AS pages,
               COALESCE(SUM(pgsize), 0) AS total_bytes,
               COALESCE(SUM(payload), 0) AS payload_bytes,
               COALESCE(SUM(unused), 0) AS unused_bytes
        FROM dbstat
        GROUP BY name
        ORDER BY total_bytes DESC, name
        "#,
    ) {
        Ok(statement) => statement,
        Err(_) => return Ok((false, Vec::new())),
    };
    let mapped = statement
        .query_map([], |row| {
            let name: String = row.get("name")?;
            let total_bytes = row.get::<_, u64>("total_bytes")?;
            Ok(StorageObjectSize {
                object_type: object_type_for(&name, object_types),
                row_count: None,
                name,
                pages: row.get("pages")?,
                total_bytes,
                payload_bytes: row.get("payload_bytes")?,
                unused_bytes: row.get("unused_bytes")?,
                percent_of_database_file: percent(total_bytes, database_bytes),
            })
        })
        .map_err(|error| error.to_string())?;
    let mut objects = Vec::new();
    for row in mapped {
        let mut object = row.map_err(|error| error.to_string())?;
        if object.object_type == "table" {
            object.row_count = row_count(connection, &object.name).ok();
        }
        objects.push(object);
    }
    Ok((true, objects))
}

fn fallback_objects(
    connection: &Connection,
    object_types: &BTreeMap<String, String>,
    database_bytes: u64,
) -> Result<Vec<StorageObjectSize>, String> {
    let mut objects = Vec::new();
    for (name, object_type) in object_types {
        let row_count = if object_type == "table" {
            row_count(connection, name).ok()
        } else {
            None
        };
        objects.push(StorageObjectSize {
            name: name.clone(),
            object_type: object_type.clone(),
            row_count,
            pages: 0,
            total_bytes: 0,
            payload_bytes: 0,
            unused_bytes: 0,
            percent_of_database_file: percent(0, database_bytes),
        });
    }
    Ok(objects)
}

fn dictionary_metrics(
    connection: &Connection,
    object_bytes: &HashMap<String, u64>,
) -> Result<Vec<DictionaryMetric>, String> {
    let specs = [
        ("object_id_dict", "sqlite_autoindex_object_id_dict_1"),
        ("path_dict", "sqlite_autoindex_path_dict_1"),
        ("symbol_dict", "sqlite_autoindex_symbol_dict_1"),
        ("qname_prefix_dict", "sqlite_autoindex_qname_prefix_dict_1"),
        (
            "qualified_name_dict",
            "sqlite_autoindex_qualified_name_dict_1",
        ),
    ];
    let mut metrics = Vec::new();
    for (table, index) in specs {
        if !table_exists(connection, table)? {
            continue;
        }
        let sql = format!(
            "SELECT COUNT(*) AS count, COALESCE(SUM(length(value)), 0) AS bytes FROM {}",
            quote_ident(table)
        );
        let (row_count, value_bytes): (u64, u64) = connection
            .query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|error| error.to_string())?;
        metrics.push(DictionaryMetric {
            table: table.to_string(),
            row_count,
            value_bytes,
            unique_index_bytes: object_bytes.get(index).copied().unwrap_or(0),
        });
    }
    metrics.sort_by(|left, right| {
        (right.value_bytes + right.unique_index_bytes)
            .cmp(&(left.value_bytes + left.unique_index_bytes))
            .then_with(|| left.table.cmp(&right.table))
    });
    Ok(metrics)
}

fn qualified_name_metric(
    connection: &Connection,
    object_bytes: &HashMap<String, u64>,
) -> Result<Option<QualifiedNameMetric>, String> {
    if !table_exists(connection, "qualified_name_dict")? {
        return Ok(None);
    }
    let metric = connection
        .query_row(
            r#"
            SELECT COUNT(*) AS count,
                   COALESCE(SUM(length(q.value)), 0) AS full_value_bytes,
                   COALESCE(SUM(length(prefix.value)), 0) AS prefix_value_bytes,
                   COALESCE(SUM(length(suffix.value)), 0) AS suffix_value_bytes
            FROM qualified_name_dict q
            LEFT JOIN qname_prefix_dict prefix ON prefix.id = q.prefix_id
            LEFT JOIN symbol_dict suffix ON suffix.id = q.suffix_id
            "#,
            [],
            |row| {
                Ok(QualifiedNameMetric {
                    row_count: row.get("count")?,
                    full_value_bytes: row.get("full_value_bytes")?,
                    prefix_value_bytes: row.get("prefix_value_bytes")?,
                    suffix_value_bytes: row.get("suffix_value_bytes")?,
                    unique_index_bytes: object_bytes
                        .get("sqlite_autoindex_qualified_name_dict_1")
                        .copied()
                        .unwrap_or(0),
                    stores_full_qualified_name_text: row.get::<_, u64>("full_value_bytes")? > 0,
                })
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(Some(metric))
}

fn fts_storage_metric(
    connection: &Connection,
    object_bytes: &HashMap<String, u64>,
) -> Result<Option<FtsStorageMetric>, String> {
    if !table_exists(connection, "stage0_fts")? {
        return Ok(None);
    }
    let row_count = row_count(connection, "stage0_fts").unwrap_or(0);
    let payload_bytes = connection
        .query_row(
            "SELECT COALESCE(SUM(length(id) + length(repo_relative_path) + length(title) + length(body)), 0) FROM stage0_fts",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?
        .max(0) as u64;
    let mut kind_counts = BTreeMap::new();
    let mut statement = connection
        .prepare("SELECT kind, COUNT(*) FROM stage0_fts GROUP BY kind ORDER BY kind")
        .map_err(|error| error.to_string())?;
    let mapped = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })
        .map_err(|error| error.to_string())?;
    for row in mapped {
        let (kind, count) = row.map_err(|error| error.to_string())?;
        kind_counts.insert(kind, count);
    }
    let shadow_bytes = object_bytes
        .iter()
        .filter(|(name, _)| name.starts_with("stage0_fts"))
        .map(|(_, bytes)| *bytes)
        .sum();
    Ok(Some(FtsStorageMetric {
        total_bytes: shadow_bytes,
        row_count,
        payload_bytes,
        stores_source_snippets: kind_counts.get("snippet").copied().unwrap_or(0) > 0,
        kind_counts,
    }))
}

fn edge_fact_mix(connection: &Connection) -> Result<Option<EdgeFactMix>, String> {
    if !table_exists(connection, "edges")? {
        return Ok(None);
    }
    let total_edges = row_count(connection, "edges").unwrap_or(0);
    let derived_edges = connection
        .query_row("SELECT COUNT(*) FROM edges WHERE derived != 0", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|error| error.to_string())?
        .max(0) as u64;
    let exactness_counts =
        grouped_edge_counts(connection, "exactness_dict", "exactness_id", "exactness")?;
    let edge_class_counts =
        grouped_edge_counts(connection, "edge_class_dict", "edge_class_id", "edge class")?;
    let context_counts = grouped_edge_counts(
        connection,
        "edge_context_dict",
        "context_id",
        "edge context",
    )?;
    let heuristic_or_unknown_edges = exactness_counts
        .iter()
        .filter(|(label, _)| {
            let lower = label.to_ascii_lowercase();
            lower.contains("heuristic") || lower.contains("unresolved") || lower == "unknown"
        })
        .map(|(_, count)| *count)
        .sum::<u64>()
        + edge_class_counts
            .iter()
            .filter(|(label, _)| {
                let lower = label.to_ascii_lowercase();
                lower.contains("heuristic") || lower == "unknown"
            })
            .map(|(_, count)| *count)
            .sum::<u64>();
    Ok(Some(EdgeFactMix {
        total_edges,
        derived_edges,
        exactness_counts,
        edge_class_counts,
        context_counts,
        heuristic_or_unknown_edges,
    }))
}

fn grouped_edge_counts(
    connection: &Connection,
    dictionary_table: &str,
    edge_column: &str,
    label: &str,
) -> Result<BTreeMap<String, u64>, String> {
    if !table_exists(connection, dictionary_table)? {
        return Ok(BTreeMap::new());
    }
    let sql = format!(
        "SELECT d.value, COUNT(*) FROM edges e JOIN {} d ON d.id = e.{} GROUP BY d.value ORDER BY d.value",
        quote_ident(dictionary_table),
        quote_ident(edge_column)
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("failed to prepare {label} count: {error}"))?;
    let mapped = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let mut counts = BTreeMap::new();
    for row in mapped {
        let (value, count) = row.map_err(|error| error.to_string())?;
        counts.insert(value, count);
    }
    Ok(counts)
}

fn core_query_plan_reports(connection: &Connection) -> Vec<CoreQueryPlanReport> {
    core_query_specs()
        .into_iter()
        .map(|(name, workflow, sql)| {
            let result = explain_query_plan(connection, sql);
            match result {
                Ok(explain_query_plan) => CoreQueryPlanReport {
                    name: name.to_string(),
                    default_workflow: workflow.to_string(),
                    sql: sql.to_string(),
                    status: "ok".to_string(),
                    error: None,
                    query_plan_analysis: analyze_query_plan(&explain_query_plan),
                    explain_query_plan,
                },
                Err(error) => CoreQueryPlanReport {
                    name: name.to_string(),
                    default_workflow: workflow.to_string(),
                    sql: sql.to_string(),
                    status: "error".to_string(),
                    error: Some(error),
                    explain_query_plan: Vec::new(),
                    query_plan_analysis: QueryPlanAnalysis {
                        uses_indexes: false,
                        indexes_used: Vec::new(),
                        full_scans: Vec::new(),
                    },
                },
            }
        })
        .collect()
}

fn core_query_specs() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "symbol_query_exact_name",
            "query symbols / definitions / seed resolution",
            "SELECT e.id_key FROM entities e WHERE e.name_id = (SELECT name_id FROM entities ORDER BY id_key LIMIT 1) ORDER BY e.id_key LIMIT 20",
        ),
        (
            "text_query_fts",
            "query text / query files / symbol FTS fallback",
            "SELECT rowid, kind, id, repo_relative_path, line, title, body, bm25(stage0_fts) AS rank FROM stage0_fts WHERE stage0_fts MATCH 'login' ORDER BY rank LIMIT 20",
        ),
        (
            "relation_query_calls",
            "query relation/path samples by relation",
            "SELECT e.id_key FROM edges e WHERE e.relation_id = (SELECT id FROM relation_kind_dict WHERE value = 'CALLS') ORDER BY e.id_key LIMIT 20",
        ),
        (
            "context_pack_outbound",
            "context-pack proof path expansion from a seed",
            "SELECT e.id_key FROM edges e WHERE e.head_id_key = (SELECT head_id_key FROM edges ORDER BY id_key LIMIT 1) AND e.relation_id = (SELECT relation_id FROM edges ORDER BY id_key LIMIT 1) ORDER BY e.id_key LIMIT 64",
        ),
        (
            "impact_inbound",
            "impact/callers/test-impact reverse traversal",
            "SELECT e.id_key FROM edges e WHERE e.tail_id_key = (SELECT tail_id_key FROM edges ORDER BY id_key LIMIT 1) AND e.relation_id = (SELECT relation_id FROM edges ORDER BY id_key LIMIT 1) ORDER BY e.id_key LIMIT 64",
        ),
        (
            "unresolved_calls_paginated",
            "query unresolved-calls paginated",
            "SELECT e.id_key FROM edges_compat e JOIN exactness_dict exactness ON exactness.id = e.exactness_id JOIN entities tail ON tail.id_key = e.tail_id_key JOIN symbol_dict tail_name ON tail_name.id = tail.name_id JOIN qualified_name_lookup tail_qname ON tail_qname.id = tail.qualified_name_id JOIN extractor_dict tail_extractor ON tail_extractor.id = tail.created_from_id WHERE e.relation_id = (SELECT id FROM relation_kind_dict WHERE value = 'CALLS') AND (exactness.value = 'static_heuristic' OR (e.flags_bitset & 2) != 0 OR lower(COALESCE(tail.metadata_json, '')) LIKE '%unresolved%' OR lower(tail_extractor.value) LIKE '%heuristic%' OR lower(tail_name.value) LIKE '%unknown_callee%' OR tail_qname.value LIKE 'static_reference:%') ORDER BY e.id_key LIMIT 20 OFFSET 0",
        ),
    ]
}

fn index_usage_report(
    connection: &Connection,
    object_bytes: &HashMap<String, u64>,
    core_query_plans: &[CoreQueryPlanReport],
) -> Result<Vec<IndexUsageReport>, String> {
    let mut reports = Vec::new();
    let mut seen = Vec::<String>::new();
    for table in user_table_names(connection)? {
        let pragma = format!("PRAGMA index_list({})", quote_ident(&table));
        let mut statement = connection
            .prepare(&pragma)
            .map_err(|error| error.to_string())?;
        let mapped = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)? != 0,
                ))
            })
            .map_err(|error| error.to_string())?;
        for row in mapped {
            let (name, unique, origin, partial) = row.map_err(|error| error.to_string())?;
            seen.push(name.clone());
            reports.push(IndexUsageReport {
                columns: index_columns(connection, &name)?,
                sql: index_sql(connection, &name)?,
                total_bytes: object_bytes.get(&name).copied().unwrap_or(0),
                used_by_core_query_plans: core_queries_using_index(core_query_plans, &name),
                default_query_usage: default_index_usage(&name, &table),
                name,
                table: table.clone(),
                unique,
                origin,
                partial,
            });
        }
    }
    for (name, bytes) in object_bytes {
        if seen.iter().any(|seen_name| seen_name == name)
            || !(name.starts_with("sqlite_autoindex") || name.starts_with("idx_"))
        {
            continue;
        }
        reports.push(IndexUsageReport {
            name: name.clone(),
            table: "unknown_or_internal".to_string(),
            columns: Vec::new(),
            unique: name.starts_with("sqlite_autoindex"),
            origin: "dbstat_only".to_string(),
            partial: false,
            total_bytes: *bytes,
            sql: None,
            default_query_usage: default_index_usage(name, "unknown_or_internal"),
            used_by_core_query_plans: core_queries_using_index(core_query_plans, name),
        });
    }
    reports.sort_by(|left, right| {
        right
            .total_bytes
            .cmp(&left.total_bytes)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(reports)
}

fn user_table_names(connection: &Connection) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .map_err(|error| error.to_string())?;
    let mapped = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let mut tables = Vec::new();
    for row in mapped {
        tables.push(row.map_err(|error| error.to_string())?);
    }
    Ok(tables)
}

fn index_columns(connection: &Connection, index_name: &str) -> Result<Vec<String>, String> {
    let pragma = format!("PRAGMA index_info({})", quote_ident(index_name));
    let mut statement = connection
        .prepare(&pragma)
        .map_err(|error| error.to_string())?;
    let mapped = statement
        .query_map([], |row| row.get::<_, String>(2))
        .map_err(|error| error.to_string())?;
    let mut columns = Vec::new();
    for row in mapped {
        columns.push(row.map_err(|error| error.to_string())?);
    }
    Ok(columns)
}

fn index_sql(connection: &Connection, index_name: &str) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
            [index_name],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map(|value| value.flatten())
        .map_err(|error| error.to_string())
}

fn core_queries_using_index(
    core_query_plans: &[CoreQueryPlanReport],
    index_name: &str,
) -> Vec<String> {
    core_query_plans
        .iter()
        .filter(|query| {
            query
                .query_plan_analysis
                .indexes_used
                .iter()
                .any(|used| used == index_name)
                || query
                    .explain_query_plan
                    .iter()
                    .any(|row| row.detail.contains(index_name))
        })
        .map(|query| query.name.clone())
        .collect()
}

fn default_index_usage(index_name: &str, table: &str) -> Vec<String> {
    let usages: &[&str] = match index_name {
        "idx_entities_path" => &[
            "list_entities_by_file during symbol FTS fallback and file-scoped expansion",
            "stale cleanup and file lifecycle maintenance by path",
        ],
        "idx_entities_name" => &[
            "query symbols exact-name lookup",
            "definitions/callers/callees/context-pack/impact seed resolution",
        ],
        "idx_entities_qname" => &[
            "query symbols exact qualified-name lookup",
            "definitions/callers/callees/context-pack/impact seed resolution",
        ],
        "idx_object_id_dict_hash" => &[
            "compact object-id dictionary lookup by stable hash/length with exact string verification",
            "replaces the former full-text UNIQUE autoindex on object_id_dict.value",
        ],
        "idx_symbol_dict_hash" => &[
            "compact symbol dictionary lookup by stable hash/length with exact string verification",
            "supports exact-name resolution and qualified-name suffix reconstruction",
        ],
        "idx_qname_prefix_dict_hash" => &[
            "compact qualified-name prefix lookup by stable hash/length with exact string verification",
            "supports qualified-name interning without a full-prefix UNIQUE text index",
        ],
        "idx_qualified_name_parts" => &[
            "qualified-name lookup by prefix_id/suffix_id tuple",
            "replaces redundant full qualified-name text storage and UNIQUE text index",
        ],
        "idx_edges_head_relation" => &[
            "context-pack outbound proof expansion",
            "impact/callees/path traversal from a seed entity",
        ],
        "idx_edges_tail_relation" => &[
            "impact/callers/test-impact reverse traversal",
            "reverse proof expansion by target entity",
        ],
        "idx_edges_span_path" => &[
            "edge lookup by source-span file for audit/UI/source-span workflows",
            "not observed in the main symbol/text/context/impact query plans unless path-scoped edge lookup is requested",
        ],
        "idx_source_spans_path" => &[
            "source-span lookup and cleanup by file path",
            "not used for most proof edges because edge spans are inline in the compact edge table",
        ],
        "idx_retrieval_traces_created" => &[
            "trace/history recency lookup",
            "not part of default symbol/text/context/impact graph queries",
        ],
        _ => {
            if index_name.starts_with("sqlite_autoindex_") {
                match table {
                    "object_id_dict" => &[
                        "compact object id dictionary value lookup during writes and id resolution",
                        "entity/edge joins usually use the INTEGER primary key after lookup",
                    ],
                    "path_dict" => &[
                        "path dictionary lookup during indexing, file cleanup, and source-span resolution",
                    ],
                    "symbol_dict" => &[
                        "symbol dictionary lookup for exact symbol resolution and indexed writes",
                    ],
                    "qname_prefix_dict" => &[
                        "qualified-name prefix interning during indexing",
                        "not directly used by default read workflows",
                    ],
                    "qualified_name_dict" => &[
                        "qualified-name dictionary lookup for exact symbol resolution",
                        "also backs joins that expose qualified names in query output",
                    ],
                    "relation_kind_dict" => &[
                        "relation name lookup for relation filters such as CALLS and IMPORTS",
                    ],
                    "exactness_dict" => &[
                        "exactness lookup for unresolved-calls and proof/heuristic filtering",
                    ],
                    "edge_class_dict" => &[
                        "edge class lookup for context/audit/proof labeling",
                    ],
                    "edge_context_dict" => &[
                        "edge context lookup for production/test/mock labeling",
                    ],
                    "language_dict" => &["file language lookup for status and manifest reporting"],
                    "repo_index_state" => &["repo status lookup by repo id"],
                    "path_evidence" => &["stored PathEvidence lookup by id when persisted"],
                    "derived_edges" => &["stored derived-edge lookup by id when persisted"],
                    "retrieval_traces" => &["trace lookup by id"],
                    "bench_tasks" | "bench_runs" => &["benchmark artifact lookup by id"],
                    _ => &["automatic unique/primary-key constraint; usage requires case-by-case verification"],
                }
            } else {
                &["no mapped default workflow observed; verify with query plans before keeping"]
            }
        }
    };
    usages.iter().map(|usage| (*usage).to_string()).collect()
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
            [table],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|error| error.to_string())
}

fn row_count(connection: &Connection, table: &str) -> Result<u64, String> {
    let sql = format!("SELECT COUNT(*) FROM {}", quote_ident(table));
    let value: i64 = connection
        .query_row(&sql, [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    Ok(value.max(0) as u64)
}

fn object_type_for(name: &str, object_types: &BTreeMap<String, String>) -> String {
    if let Some(object_type) = object_types.get(name) {
        object_type.clone()
    } else if name.starts_with("sqlite_autoindex") {
        "autoindex".to_string()
    } else if name.starts_with("sqlite_schema") {
        "schema".to_string()
    } else {
        "internal".to_string()
    }
}

fn file_family_size(db_path: &Path) -> FileFamilySize {
    let database_bytes = file_size(db_path);
    let wal_bytes = file_size(&PathBuf::from(format!("{}-wal", db_path.display())));
    let shm_bytes = file_size(&PathBuf::from(format!("{}-shm", db_path.display())));
    FileFamilySize {
        database_bytes,
        wal_bytes,
        shm_bytes,
        total_bytes: database_bytes + wal_bytes + shm_bytes,
    }
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn repo_roots(connection: &Connection) -> Result<Vec<PathBuf>, String> {
    let mut roots = Vec::new();
    if table_exists(connection, "repo_index_state")? {
        let mut statement = connection
            .prepare(
                "SELECT repo_root FROM repo_index_state ORDER BY indexed_at_unix_ms DESC LIMIT 5",
            )
            .map_err(|error| error.to_string())?;
        let mapped = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        for row in mapped {
            let root = row.map_err(|error| error.to_string())?;
            if !root.is_empty() {
                roots.push(PathBuf::from(root));
            }
        }
    }
    roots.push(std::env::current_dir().map_err(|error| error.to_string())?);
    roots.sort();
    roots.dedup();
    Ok(roots)
}

fn load_source_snippet(
    repo_roots: &[PathBuf],
    span: &AuditSourceSpan,
) -> Result<SourceSnippet, String> {
    for root in repo_roots {
        let candidate = if Path::new(&span.repo_relative_path).is_absolute() {
            PathBuf::from(&span.repo_relative_path)
        } else {
            root.join(&span.repo_relative_path)
        };
        if !candidate.exists() {
            continue;
        }
        let source = fs::read_to_string(&candidate)
            .map_err(|error| format!("failed to read {}: {error}", candidate.display()))?;
        let lines = source.lines().collect::<Vec<_>>();
        if lines.is_empty() {
            return Ok(SourceSnippet {
                path: path_string(&candidate),
                start_line: 1,
                end_line: 1,
                text: String::new(),
            });
        }
        let start = span.start_line.saturating_sub(2).max(1);
        let end = (span.end_line + 2).min(lines.len() as u32).max(start);
        let text = (start..=end)
            .filter_map(|line| {
                lines
                    .get((line - 1) as usize)
                    .map(|text| format!("{line}: {text}"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Ok(SourceSnippet {
            path: path_string(&candidate),
            start_line: start,
            end_line: end,
            text,
        });
    }
    Err(format!(
        "source file could not be found for {} under {} root(s)",
        span.repo_relative_path,
        repo_roots.len()
    ))
}

fn infer_context(relation: &str, path: &str, head_id: &str, tail_id: &str) -> String {
    let lower_path = path.to_ascii_lowercase();
    let lower_head = head_id.to_ascii_lowercase();
    let lower_tail = tail_id.to_ascii_lowercase();
    if MOCK_RELATIONS.contains(&relation)
        || lower_head.contains("mock")
        || lower_head.contains("stub")
        || lower_tail.contains("mock")
        || lower_tail.contains("stub")
    {
        "mock_or_stub_inferred".to_string()
    } else if TEST_RELATIONS.contains(&relation)
        || lower_path.contains("test")
        || lower_path.contains("spec")
        || lower_head.contains("test")
        || lower_tail.contains("test")
    {
        "test_or_fixture_inferred".to_string()
    } else if lower_path.starts_with("src/")
        || lower_path.contains("/src/")
        || (!lower_path.contains("test") && !lower_path.contains("mock"))
    {
        "production_inferred".to_string()
    } else {
        "unknown_not_first_class".to_string()
    }
}

fn classify_edge_fact(relation: &str, exactness: &str, derived: bool, context: &str) -> String {
    if derived {
        "derived".to_string()
    } else if context == "test_or_fixture_inferred" || context == "mock_or_stub_inferred" {
        "test_or_mock".to_string()
    } else if MOCK_RELATIONS.contains(&relation) || TEST_RELATIONS.contains(&relation) {
        "test_or_mock".to_string()
    } else if matches!(
        exactness,
        "exact" | "compiler_verified" | "lsp_verified" | "parser_verified"
    ) {
        "base_exact".to_string()
    } else {
        "base_heuristic".to_string()
    }
}

fn missing_edge_metadata(
    head: &AuditEndpoint,
    tail: &AuditEndpoint,
    repo_commit: Option<&str>,
    file_hash: Option<&str>,
    derived: bool,
    provenance_edges: &[String],
    span_loaded: bool,
    snippets_requested: bool,
    metadata: &Value,
) -> Vec<String> {
    let mut missing = Vec::new();
    if head.name.is_none() {
        missing.push("head_name_missing".to_string());
    }
    if head.qualified_name.is_none() {
        missing.push("head_qualified_name_missing".to_string());
    }
    if tail.name.is_none() {
        missing.push("tail_name_missing".to_string());
    }
    if tail.qualified_name.is_none() {
        missing.push("tail_qualified_name_missing".to_string());
    }
    if repo_commit.is_none() {
        missing.push("repo_commit_missing".to_string());
    }
    if file_hash.is_none() {
        missing.push("file_hash_missing".to_string());
    }
    if derived && provenance_edges.is_empty() {
        missing.push("derived_provenance_missing".to_string());
    }
    if snippets_requested && !span_loaded {
        missing.push("source_snippet_unavailable".to_string());
    }
    if metadata.as_object().is_none_or(|object| object.is_empty()) {
        missing.push("metadata_empty".to_string());
    }
    missing
}

fn parse_string_array(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn parse_source_spans(raw: &str) -> Vec<AuditSourceSpan> {
    serde_json::from_str::<Vec<AuditSourceSpan>>(raw).unwrap_or_default()
}

fn parse_edge_triples(raw: &str) -> Vec<(String, String, String)> {
    let value = serde_json::from_str::<Value>(raw).unwrap_or(Value::Null);
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let triple = item.as_array()?;
            if triple.len() != 3 {
                return None;
            }
            Some((
                triple.first()?.as_str()?.to_string(),
                triple.get(1)?.as_str()?.to_string(),
                triple.get(2)?.as_str()?.to_string(),
            ))
        })
        .collect()
}

fn task_or_query_from_metadata(metadata: &Value) -> Option<String> {
    ["task", "query", "prompt", "task_or_query"]
        .iter()
        .find_map(|key| metadata.get(key).and_then(Value::as_str))
        .map(str::to_string)
}

fn infer_path_context(edges: &[PathEdgeSample], spans: &[AuditSourceSpan]) -> String {
    if edges
        .iter()
        .any(|edge| edge.production_test_mock_context == "mock_or_stub_inferred")
    {
        "mock_or_stub_inferred".to_string()
    } else if edges
        .iter()
        .any(|edge| edge.production_test_mock_context == "test_or_fixture_inferred")
        || spans.iter().any(|span| {
            let path = span.repo_relative_path.to_ascii_lowercase();
            path.contains("test") || path.contains("spec")
        })
    {
        "test_or_fixture_inferred".to_string()
    } else if edges
        .iter()
        .any(|edge| edge.production_test_mock_context == "production_inferred")
        || !spans.is_empty()
    {
        "production_inferred".to_string()
    } else {
        "unknown_not_first_class".to_string()
    }
}

fn derived_provenance_label(edges: &[PathEdgeSample]) -> String {
    if edges
        .iter()
        .any(|edge| edge.derived == Some(true) && edge.provenance_edges.is_empty())
    {
        "derived_missing_provenance".to_string()
    } else if edges.iter().any(|edge| edge.derived == Some(true)) {
        "derived_with_provenance".to_string()
    } else if edges
        .iter()
        .any(|edge| edge.exactness.as_deref() == Some("derived_from_verified_edges"))
    {
        "derived_exactness_without_derived_flag".to_string()
    } else {
        "base_or_heuristic_edges".to_string()
    }
}

fn missing_path_metadata(
    source: &AuditEndpoint,
    target: &AuditEndpoint,
    edges: &[PathEdgeSample],
    source_spans: &[AuditSourceSpan],
    source_snippet_count: usize,
    snippets_requested: bool,
    metadata: &Value,
) -> Vec<String> {
    let mut missing = Vec::new();
    if source.name.is_none() {
        missing.push("source_name_missing".to_string());
    }
    if target.name.is_none() {
        missing.push("target_name_missing".to_string());
    }
    if edges.is_empty() {
        missing.push("edge_list_missing".to_string());
    }
    if source_spans.is_empty() {
        missing.push("source_spans_missing".to_string());
    }
    if snippets_requested && source_snippet_count == 0 {
        missing.push("source_snippets_unavailable".to_string());
    }
    if metadata.as_object().is_none_or(|object| object.is_empty()) {
        missing.push("metadata_empty".to_string());
    }
    for edge in edges {
        missing.extend(edge.missing_metadata.iter().cloned());
    }
    missing.sort();
    missing.dedup();
    missing
}

fn render_storage_markdown(report: &StorageInspection) -> String {
    let mut output = String::new();
    output.push_str("# Storage Inspection\n\n");
    output.push_str(&format!("Database: `{}`\n\n", report.db_path));
    output.push_str(&format!(
        "- Inspection read-only: `{}`\n- Artifact mutated during inspection: `{}`\n- Sidecar-only change: `{}`\n- Sidecar status: `{}`\n- Read-only mode: `{}`\n- Immutable mode used: `{}`\n- Immutable mode reason: `{}`\n- Main DB size before: `{}`\n- Main DB size after: `{}`\n- Main DB mtime before: `{}`\n- Main DB mtime after: `{}`\n- Main DB hash before: `{}`\n- Main DB hash after: `{}`\n- DBSTAT available: `{}`\n- Database bytes: `{}`\n- WAL bytes: `{}`\n- SHM bytes: `{}`\n- File family bytes: `{}`\n- Page size: `{}`\n- Page count: `{}`\n- Freelist count: `{}`\n\n",
        report.inspection_read_only,
        report.artifact_mutated_during_inspection,
        report.sidecar_only_change,
        report.sidecar_status,
        report.read_only_mode_used,
        report.immutable_mode_used,
        report.immutable_mode_reason.replace('`', "'"),
        report.main_db_size_before,
        report.main_db_size_after,
        report
            .main_db_mtime_before
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        report
            .main_db_mtime_after
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        report.main_db_hash_before.as_deref().unwrap_or("unknown"),
        report.main_db_hash_after.as_deref().unwrap_or("unknown"),
        report.dbstat_available,
        report.file_family.database_bytes,
        report.file_family.wal_bytes,
        report.file_family.shm_bytes,
        report.file_family.total_bytes,
        report.page_metrics.page_size_bytes,
        report.page_metrics.page_count,
        report.page_metrics.freelist_count,
    ));
    output.push_str("## Integrity Check\n\n");
    output.push_str(&format!(
        "- Status: `{}`\n- Checked: `{}`\n- Max errors captured: `{}`\n\n",
        report
            .integrity_check
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        report
            .integrity_check
            .get("checked")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        report
            .integrity_check
            .get("max_errors")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    ));
    if let Some(messages) = report
        .integrity_check
        .get("messages")
        .and_then(Value::as_array)
    {
        for message in messages.iter().take(5).filter_map(Value::as_str) {
            output.push_str(&format!("- `{}`\n", message.replace('`', "'")));
        }
        output.push('\n');
    }
    output.push_str("## Aggregate Metrics\n\n");
    output.push_str(&format!(
        "- Tables: `{}`\n- Indexes: `{}`\n- Observed table rows: `{}`\n- Proof edge rows: `{}`\n- Structural relation rows: `{}`\n- Callsite rows: `{}`\n- Callsite argument rows: `{}`\n- Semantic edge/fact rows: `{}`\n- Average database bytes per proof edge: `{:.2}`\n- Average database bytes per semantic edge/fact: `{:.2}`\n- Average edge table bytes per proof edge: `{:.2}`\n- Average edge table plus edge-index bytes per proof edge: `{:.2}`\n- Source-span rows: `{}`\n- Average source-span table bytes per row: `{:.2}`\n\n",
        report.aggregate_metrics.table_count,
        report.aggregate_metrics.index_count,
        report.aggregate_metrics.total_rows_observed,
        report.aggregate_metrics.proof_edge_count,
        report.aggregate_metrics.structural_record_count,
        report.aggregate_metrics.callsite_record_count,
        report.aggregate_metrics.callsite_arg_record_count,
        report.aggregate_metrics.semantic_edge_count,
        report.aggregate_metrics.average_database_bytes_per_edge,
        report.aggregate_metrics.average_database_bytes_per_semantic_edge,
        report.aggregate_metrics.average_edge_table_bytes_per_edge,
        report.aggregate_metrics.average_edge_table_plus_index_bytes_per_edge,
        report.aggregate_metrics.source_span_count,
        report.aggregate_metrics.average_source_span_bytes_per_row,
    ));
    output.push_str("## Category Breakdown\n\n");
    output.push_str("| Category | Bytes |\n");
    output.push_str("| --- | ---: |\n");
    output.push_str(&format!(
        "| Dictionary tables | {} |\n| Dictionary unique indexes | {} |\n| Edge indexes | {} |\n| Source-span table/index | {} |\n| FTS/shadow tables | {} |\n| Snippet-like objects | {} |\n\n",
        report.categories.dictionary_table_bytes,
        report.categories.unique_text_index_bytes,
        report.categories.edge_index_bytes,
        report.categories.source_span_bytes,
        report.categories.fts_bytes,
        report.categories.snippet_like_bytes,
    ));
    output.push_str("## Table and Index Sizes\n\n");
    output.push_str("| Object | Type | Rows | Bytes | Payload | Unused | DB % |\n");
    output.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: |\n");
    for object in report.objects.iter().take(80) {
        output.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} | {:.2} |\n",
            object.name,
            object.object_type,
            object
                .row_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            object.total_bytes,
            object.payload_bytes,
            object.unused_bytes,
            object.percent_of_database_file,
        ));
    }
    output.push_str("\n## Table Row Averages\n\n");
    output.push_str("| Table | Rows | Bytes | Payload | Avg bytes/row | Avg payload/row |\n");
    output.push_str("| --- | ---: | ---: | ---: | ---: | ---: |\n");
    for metric in &report.table_row_metrics {
        output.push_str(&format!(
            "| `{}` | {} | {} | {} | {:.2} | {:.2} |\n",
            metric.table,
            metric.row_count,
            metric.total_bytes,
            metric.payload_bytes,
            metric.average_total_bytes_per_row,
            metric.average_payload_bytes_per_row,
        ));
    }
    output.push_str("\n## Dictionary Metrics\n\n");
    output.push_str("| Dictionary | Rows | Value bytes | Unique index bytes |\n");
    output.push_str("| --- | ---: | ---: | ---: |\n");
    for metric in &report.dictionary_metrics {
        output.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            metric.table, metric.row_count, metric.value_bytes, metric.unique_index_bytes
        ));
    }
    if let Some(metric) = &report.fts_storage {
        output.push_str("\n## FTS And Snippet Storage\n\n");
        output.push_str(&format!(
            "- FTS total bytes: `{}`\n- FTS rows: `{}`\n- FTS payload bytes: `{}`\n- Stores source snippets: `{}`\n\n",
            metric.total_bytes,
            metric.row_count,
            metric.payload_bytes,
            metric.stores_source_snippets,
        ));
        output.push_str("| Kind | Rows |\n| --- | ---: |\n");
        for (kind, count) in &metric.kind_counts {
            output.push_str(&format!("| `{kind}` | {count} |\n"));
        }
    }
    if let Some(metric) = &report.edge_fact_mix {
        output.push_str("\n## Edge Fact Mix\n\n");
        output.push_str(&format!(
            "- Total edges: `{}`\n- Derived edges: `{}`\n- Heuristic/unknown edge labels observed: `{}`\n\n",
            metric.total_edges, metric.derived_edges, metric.heuristic_or_unknown_edges
        ));
        output.push_str("### Exactness Counts\n\n");
        output.push_str("| Exactness | Edges |\n| --- | ---: |\n");
        for (label, count) in &metric.exactness_counts {
            output.push_str(&format!("| `{label}` | {count} |\n"));
        }
        output.push_str("\n### Edge Class Counts\n\n");
        output.push_str("| Edge class | Edges |\n| --- | ---: |\n");
        for (label, count) in &metric.edge_class_counts {
            output.push_str(&format!("| `{label}` | {count} |\n"));
        }
        output.push_str("\n### Edge Context Counts\n\n");
        output.push_str("| Context | Edges |\n| --- | ---: |\n");
        for (label, count) in &metric.context_counts {
            output.push_str(&format!("| `{label}` | {count} |\n"));
        }
    }
    if let Some(metric) = &report.qualified_name_metric {
        output.push_str("\n## Qualified Name Redundancy\n\n");
        output.push_str(&format!(
            "- Stores full qualified-name text: `{}`\n- Rows: `{}`\n- Full value bytes: `{}`\n- Prefix value bytes: `{}`\n- Suffix value bytes: `{}`\n- Unique index bytes: `{}`\n",
            metric.stores_full_qualified_name_text,
            metric.row_count,
            metric.full_value_bytes,
            metric.prefix_value_bytes,
            metric.suffix_value_bytes,
            metric.unique_index_bytes,
        ));
    }
    output.push_str("\n## Index Usage Report\n\n");
    output.push_str("| Index | Table | Columns | Bytes | Unique | Origin | Used by core plans | Default workflow usage |\n");
    output.push_str("| --- | --- | --- | ---: | --- | --- | --- | --- |\n");
    for index in &report.index_usage {
        output.push_str(&format!(
            "| `{}` | `{}` | {} | {} | `{}` | `{}` | {} | {} |\n",
            index.name,
            index.table,
            markdown_join(&index.columns),
            index.total_bytes,
            index.unique,
            index.origin,
            markdown_join(&index.used_by_core_query_plans),
            markdown_join(&index.default_query_usage),
        ));
    }
    output.push_str("\n## Core Query Plans\n\n");
    for query in &report.core_query_plans {
        output.push_str(&format!(
            "### `{}`\n\nDefault workflow: {}\n\nStatus: `{}`\n\nIndexes: {}\n\nFull scans: {}\n\n",
            query.name,
            query.default_workflow,
            query.status,
            markdown_join(&query.query_plan_analysis.indexes_used),
            markdown_join(&query.query_plan_analysis.full_scans),
        ));
        if let Some(error) = &query.error {
            output.push_str(&format!("Error: `{}`\n\n", error.replace('`', "'")));
        }
        output.push_str("| ID | Parent | Detail |\n| ---: | ---: | --- |\n");
        for row in &query.explain_query_plan {
            output.push_str(&format!(
                "| {} | {} | `{}` |\n",
                row.id,
                row.parent,
                row.detail.replace('`', "'")
            ));
        }
        output.push('\n');
    }
    output.push_str("\n## Notes\n\n");
    for note in &report.notes {
        output.push_str(&format!("- {note}\n"));
    }
    output
}

fn render_schema_validation_markdown(report: &SchemaValidationReport) -> String {
    let mut output = String::new();
    output.push_str("# Compact Proof Schema Check\n\n");
    output.push_str(&format!("- Status: `{}`\n", report.status));
    output.push_str(&format!("- Database: `{}`\n", report.db_path));
    output.push_str(&format!("- User version: `{}`\n", report.user_version));
    output.push_str(&format!(
        "- Failure count: `{}`\n- Inspection read-only: `{}`\n- Artifact mutated during inspection: `{}`\n- Sidecar-only change: `{}`\n- Sidecar status: `{}`\n- Read-only mode: `{}`\n- Immutable mode used: `{}`\n- Immutable mode reason: `{}`\n- Main DB size before: `{}`\n- Main DB size after: `{}`\n- Main DB hash before: `{}`\n- Main DB hash after: `{}`\n\n",
        report.failures.len(),
        report.inspection_read_only,
        report.artifact_mutated_during_inspection,
        report.sidecar_only_change,
        report.sidecar_status,
        report.read_only_mode_used,
        report.immutable_mode_used,
        report.immutable_mode_reason.replace('`', "'"),
        report.main_db_size_before,
        report.main_db_size_after,
        report.main_db_hash_before.as_deref().unwrap_or("unknown"),
        report.main_db_hash_after.as_deref().unwrap_or("unknown"),
    ));

    output.push_str("## Expected Columns\n\n");
    output.push_str("| Table | Column | Status |\n");
    output.push_str("| --- | --- | --- |\n");
    for check in &report.expected_columns {
        output.push_str(&format!(
            "| `{}` | `{}` | `{}` |\n",
            check.table, check.column, check.status
        ));
    }
    output.push('\n');

    output.push_str("## Views\n\n");
    output.push_str("| View | Status | Error |\n");
    output.push_str("| --- | --- | --- |\n");
    for check in &report.views {
        output.push_str(&format!(
            "| `{}` | `{}` | {} |\n",
            check.name,
            check.status,
            check
                .error
                .as_deref()
                .map(markdown_code)
                .unwrap_or_else(|| "`none`".to_string())
        ));
    }
    output.push('\n');

    output.push_str("## Default Query SQL\n\n");
    output.push_str("| Query | Status | Error |\n");
    output.push_str("| --- | --- | --- |\n");
    for check in &report.default_query_sql {
        output.push_str(&format!(
            "| `{}` | `{}` | {} |\n",
            check.name,
            check.status,
            check
                .error
                .as_deref()
                .map(markdown_code)
                .unwrap_or_else(|| "`none`".to_string())
        ));
    }
    output.push('\n');

    output.push_str("## Failures\n\n");
    if report.failures.is_empty() {
        output.push_str("- none\n");
    } else {
        for failure in &report.failures {
            output.push_str(&format!("- {}\n", failure));
        }
    }
    output.push('\n');

    output.push_str("## Notes\n\n");
    for note in &report.notes {
        output.push_str(&format!("- {}\n", note));
    }
    output
}

fn render_storage_experiments_markdown(report: &StorageExperimentReport) -> String {
    let mut output = String::new();
    output.push_str("# Storage Experiments\n\n");
    output.push_str(&format!(
        "Original DB: `{}`\n\nRun dir: `{}`\n\nOriginal file family bytes: `{}`\n\n",
        report.original_db_path, report.run_dir, report.original_file_family.total_bytes
    ));
    output.push_str("## Experiments\n\n");
    for experiment in &report.experiments {
        output.push_str(&format!(
            "### `{}`\n\nCopied DB: `{}`\n\nCopy removed: `{}`\n\nMutations: `{}`\n\n",
            experiment.name,
            experiment.copied_db_path,
            experiment.copy_removed,
            experiment.mutations.join("`, `")
        ));
        output.push_str(&format!(
            "Recommendation: `{}` (`recommended={}`)\n\nReason: {}\n\n",
            experiment.recommendation.decision,
            experiment.recommendation.recommended,
            experiment.recommendation.reason
        ));
        output.push_str("| DB bytes before | DB bytes after | Delta bytes | Delta percent | Graph Truth status | Context packet status |\n");
        output.push_str("| ---: | ---: | ---: | ---: | --- | --- |\n");
        output.push_str(&format!(
            "| {} | {} | {} | {:.2} | `{}` | `{}` |\n\n",
            experiment.summary.db_size_before_bytes,
            experiment.summary.db_size_after_bytes,
            experiment.summary.size_delta_bytes,
            experiment.summary.size_delta_percent,
            experiment
                .graph_truth
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            experiment
                .context_packet
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
        ));
        if !experiment.notes.is_empty() {
            output.push_str("Notes:\n\n");
            for note in &experiment.notes {
                output.push_str(&format!("- {note}\n"));
            }
            output.push('\n');
        }
        output.push_str("| Checkpoint | DB bytes | WAL bytes | Edge index bytes | Dict table bytes | Unique text index bytes | FTS bytes | Source span bytes |\n");
        output.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
        for checkpoint in &experiment.checkpoints {
            output.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
                checkpoint.name,
                checkpoint.file_family.database_bytes,
                checkpoint.file_family.wal_bytes,
                checkpoint.categories.edge_index_bytes,
                checkpoint.categories.dictionary_table_bytes,
                checkpoint.categories.unique_text_index_bytes,
                checkpoint.categories.fts_bytes,
                checkpoint.categories.source_span_bytes,
            ));
        }
        output.push_str("\n#### Core Query Delta\n\n");
        output.push_str(
            "| Query | Before ms | After ms | Delta ms | Before status | After status |\n",
        );
        output.push_str("| --- | ---: | ---: | ---: | --- | --- |\n");
        for delta in &experiment.summary.core_query_latency_before_after {
            output.push_str(&format!(
                "| `{}` | {} | {} | {} | `{}` | `{}` |\n",
                delta.query,
                delta.before_ms,
                delta.after_ms,
                delta.delta_ms,
                delta.before_status,
                delta.after_status,
            ));
        }
        output.push_str("\n#### Query Latencies\n\n");
        output.push_str("| Checkpoint | Query | ms | Rows | Status | Indexes | Full scans |\n");
        output.push_str("| --- | --- | ---: | ---: | --- | --- | --- |\n");
        for checkpoint in &experiment.checkpoints {
            for query in &checkpoint.query_latencies {
                output.push_str(&format!(
                    "| `{}` | `{}` | {} | {} | `{}` | {} | {} |\n",
                    checkpoint.name,
                    query.name,
                    query.elapsed_ms,
                    query.rows_observed,
                    query.status,
                    markdown_join(&query.query_plan_analysis.indexes_used),
                    markdown_join(&query.query_plan_analysis.full_scans),
                ));
            }
        }
        output.push_str("\n#### Degradation Flags\n\n");
        let degraded = experiment
            .degraded_queries
            .iter()
            .filter(|query| query.degraded)
            .collect::<Vec<_>>();
        if degraded.is_empty() {
            output.push_str("No query degradation crossed the audit threshold.\n\n");
        } else {
            output.push_str("| Query | Checkpoint | Before ms | After ms | Ratio |\n");
            output.push_str("| --- | --- | ---: | ---: | ---: |\n");
            for query in degraded {
                output.push_str(&format!(
                    "| `{}` | `{}` | {} | {} | {:.2} |\n",
                    query.query, query.checkpoint, query.baseline_ms, query.after_ms, query.ratio,
                ));
            }
            output.push('\n');
        }
    }
    output.push_str("## Notes\n\n");
    for note in &report.notes {
        output.push_str(&format!("- {note}\n"));
    }
    output
}

fn markdown_join(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .map(|value| format!("`{}`", value.replace('`', "'")))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn markdown_code(value: &str) -> String {
    format!("`{}`", value.replace('`', "'"))
}

fn render_edge_samples_markdown(report: &EdgeSampleReport) -> String {
    let mut output = String::new();
    output.push_str("# Edge Sample Audit\n\n");
    output.push_str(&format!(
        "Database: `{}`\n\nRelation filter: `{}`\n\nLimit: `{}`\n\nSeed: `{}`\n\n",
        report.db_path,
        report.relation_filter.as_deref().unwrap_or("all relations"),
        report.limit,
        report.seed,
    ));
    output.push_str("Allowed manual classifications: ");
    output.push_str(&report.manual_classification_options.join(", "));
    output.push_str("\n\n");
    for sample in &report.samples {
        output.push_str(&format!("## Sample {}\n\n", sample.ordinal));
        for classification in &report.manual_classification_options {
            output.push_str(&format!("- {classification}:\n"));
        }
        output.push_str(&format!(
            "- edge_id: `{}`\n- head: `{}`",
            sample.edge_id, sample.head.id
        ));
        if let Some(name) = &sample.head.name {
            output.push_str(&format!(" (`{name}`)"));
        }
        output.push_str(&format!(
            "\n- relation: `{}`\n- tail: `{}`",
            sample.relation, sample.tail.id
        ));
        if let Some(name) = &sample.tail.name {
            output.push_str(&format!(" (`{name}`)"));
        }
        output.push_str(&format!(
            "\n- source_span: `{}:{}-{}`
- relation_direction: `{}`
- exactness: `{}`
- confidence: `{}`
- repo_commit: `{}`
- file_hash: `{}`
- derived: `{}`
- extractor: `{}`
- fact_classification: `{}`
- context: `{}`
- provenance_edges: `{}`
- missing_metadata: `{}`
- span_loaded: `{}`
",
            sample.source_span.repo_relative_path,
            sample.source_span.start_line,
            sample.source_span.end_line,
            sample.relation_direction,
            sample.exactness,
            sample.confidence,
            sample.repo_commit.as_deref().unwrap_or("unknown"),
            sample.file_hash.as_deref().unwrap_or("unknown"),
            sample.derived,
            sample.extractor,
            sample.fact_classification,
            sample.production_test_mock_context,
            sample.provenance_edges.join(", "),
            sample.missing_metadata.join(", "),
            sample.span_loaded,
        ));
        if let Some(error) = &sample.span_load_error {
            output.push_str(&format!(
                "- span_load_error: `{}`\n",
                error.replace('`', "'")
            ));
        }
        if let Some(snippet) = &sample.source_snippet {
            output.push_str(&format!(
                "\n```text\n{}\n```\n",
                snippet.text.replace("```", "'''")
            ));
        }
        output.push('\n');
    }
    if report.samples.is_empty() {
        output.push_str("No edges matched this sample request.\n");
    }
    output
}

fn render_path_samples_markdown(report: &PathEvidenceSampleReport) -> String {
    let mut output = String::new();
    output.push_str("# PathEvidence Sample Audit\n\n");
    output.push_str(&format!(
        "Database: `{}`\n\nMode: `{}`\n\nLimit: `{}`\n\nSeed: `{}`\n\nMax edge load: `{}`\n\nTimeout ms: `{}`\n\nStored PathEvidence rows: `{}`\n\nCandidate path IDs: `{}`\n\nLoaded materialized path edges: `{}`\n\nEdge load truncated: `{}`\n\nPathEvidence truncated: `{}`\n\nPathEvidence omitted count: `{}`\n\nHydration budget exhausted: `{}`\n\nSource snippet omitted count: `{}`\n\nPartial diagnostic: `{}`\n\nTimeout exhausted: `{}`\n\nStop reason: `{}`\n\nGenerated fallback samples: `{}`\n\n",
        report.db_path,
        report.mode.as_str(),
        report.limit,
        report.seed,
        report.max_edge_load,
        report.timeout_ms,
        report.stored_path_count,
        report.candidate_path_count,
        report.loaded_path_edge_count,
        report.edge_load_truncated,
        report.path_evidence_truncated,
        report.path_evidence_omitted_count,
        report.hydration_budget_exhausted,
        report.source_snippet_omitted_count,
        report.partial_diagnostic,
        report.timeout_ms_exhausted,
        report.stop_reason.as_deref().unwrap_or("none"),
        report.generated_path_count,
    ));
    output.push_str("## Sampler Timing\n\n");
    output.push_str("| Stage | ms |\n");
    output.push_str("| --- | ---: |\n");
    for (stage, value) in [
        ("total", report.timing.total_ms),
        ("open_db", report.timing.open_db_ms),
        ("repo_roots", report.timing.repo_roots_ms),
        ("count", report.timing.count_ms),
        ("candidate_select", report.timing.candidate_select_ms),
        ("path_rows_load", report.timing.path_rows_load_ms),
        ("path_edges_load", report.timing.path_edges_load_ms),
        ("endpoint_load", report.timing.endpoint_load_ms),
        ("snippet_load", report.timing.snippet_load_ms),
        ("sample_build", report.timing.sample_build_ms),
        ("explain", report.timing.explain_ms),
        ("index_check", report.timing.index_check_ms),
    ] {
        output.push_str(&format!("| `{stage}` | {value} |\n"));
    }
    output.push_str("\n## Index Status\n\n");
    output.push_str("| Object | Required shape | Present | Satisfied by | Columns |\n");
    output.push_str("| --- | --- | --- | --- | --- |\n");
    for status in &report.index_status {
        output.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` |\n",
            status.object,
            status.required_shape,
            status.present,
            status.satisfied_by.as_deref().unwrap_or("missing"),
            status.columns.join(", "),
        ));
    }
    output.push_str("\n## Query Plans\n\n");
    for plan in &report.explain_query_plan {
        output.push_str(&format!("### `{}`\n\n", plan.name));
        output.push_str(&format!("```sql\n{}\n```\n\n", plan.sql));
        for row in &plan.explain_query_plan {
            output.push_str(&format!("- `{}`\n", row.detail.replace('`', "'")));
        }
        if !plan.query_plan_analysis.full_scans.is_empty() {
            output.push_str("- full_scans: `");
            output.push_str(&plan.query_plan_analysis.full_scans.join(" | "));
            output.push_str("`\n");
        }
        output.push('\n');
    }
    output.push_str("Allowed manual classifications: ");
    output.push_str(&report.manual_classification_options.join(", "));
    output.push_str("\n\n");
    for sample in &report.samples {
        output.push_str(&format!("## Path Sample {}\n\n", sample.ordinal));
        for classification in &report.manual_classification_options {
            output.push_str(&format!("- {classification}:\n"));
        }
        output.push_str(&format!(
            "- path_id: `{}`\n- generated_by_audit: `{}`\n- task_or_query: `{}`\n- source: `{}`\n- target: `{}`\n- relation_sequence: `{}`\n- exactness: `{}`\n- confidence: `{}`\n- derived_provenance_label: `{}`\n- context: `{}`\n- missing_metadata: `{}`\n",
            sample.path_id,
            sample.generated_by_audit,
            sample.task_or_query.as_deref().unwrap_or("unknown"),
            sample.source.id,
            sample.target.id,
            sample.relation_sequence.join(" -> "),
            sample.exactness,
            sample.confidence,
            sample.derived_provenance_label,
            sample.production_test_mock_context,
            sample.missing_metadata.join(", "),
        ));
        if let Some(summary) = &sample.summary {
            output.push_str(&format!("- summary: `{}`\n", summary.replace('`', "'")));
        }
        output.push_str("\n### Edge List\n\n");
        output.push_str("| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |\n");
        output.push_str("| --- | --- | --- | --- | ---: | --- | --- | --- | --- |\n");
        for edge in &sample.edge_list {
            output.push_str(&format!(
                "| `{}` | `{}` | `{}` | `{}` | {} | `{}` | `{}` | `{}` | `{}` |\n",
                edge.edge_id.as_deref().unwrap_or("unknown"),
                edge.relation,
                edge.relation_direction,
                edge.exactness.as_deref().unwrap_or("unknown"),
                edge.confidence
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                edge.derived
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                edge.production_test_mock_context,
                edge.fact_classification,
                edge.provenance_edges.join(", "),
            ));
        }
        output.push_str("\n### Source Spans\n\n");
        if sample.source_spans.is_empty() {
            output.push_str("- unknown\n");
        } else {
            for span in &sample.source_spans {
                output.push_str(&format!(
                    "- `{}:{}-{}`
",
                    span.repo_relative_path, span.start_line, span.end_line
                ));
            }
        }
        for snippet in &sample.source_snippets {
            output.push_str(&format!(
                "\n```text\n{}\n```\n",
                snippet.text.replace("```", "'''")
            ));
        }
        output.push('\n');
    }
    if report.samples.is_empty() {
        output.push_str(
            "No PathEvidence rows or generated fallback edges matched this sample request.\n",
        );
    }
    output.push_str("\n## Notes\n\n");
    for note in &report.notes {
        output.push_str(&format!("- {note}\n"));
    }
    output
}

fn render_label_samples_markdown(report: &LabelSamplesReport) -> String {
    let mut output = String::new();
    output.push_str("# Manual Relation Labeling Summary\n\n");
    output.push_str(&format!(
        "Generated at unix ms: `{}`\n\nTotal samples: `{}`\n\nLabeled samples: `{}`\n\nUnlabeled samples: `{}`\n\nUnsupported samples: `{}`\n\nRecall estimate: `{}`\n\n",
        report.generated_at_unix_ms,
        report.summary.total_samples,
        report.summary.labeled_samples,
        report.summary.unlabeled_samples,
        report.summary.unsupported_samples,
        report.summary.recall_estimate_status,
    ));
    output.push_str("## Workflow\n\n");
    output.push_str(
        "Edit sampled Markdown bullets with values like `yes`, `true`, or `x`, then run `codegraph-mcp audit label-samples` followed by `codegraph-mcp audit summarize-labels`. Unsupported patterns may use `- unsupported: yes` and `- unsupported_pattern: <pattern>`.\n\n",
    );
    output.push_str("## Inputs\n\n");
    output.push_str("### Edge JSON\n\n");
    append_path_list(&mut output, &report.edge_inputs);
    output.push_str("\n### PathEvidence JSON\n\n");
    append_path_list(&mut output, &report.path_inputs);
    output.push_str("\n### Label Markdown\n\n");
    append_path_list(&mut output, &report.markdown_inputs);

    output.push_str("\n## Precision By Relation\n\n");
    output.push_str("| Relation | Labeled | Unlabeled | Unsupported | Unsure | TP | FP | Wrong span | Precision | Recall |\n");
    output.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n");
    if report.summary.relation_precision.is_empty() {
        output.push_str("| unknown | 0 | 0 | 0 | 0 | 0 | 0 | 0 | unknown | unknown |\n");
    } else {
        for relation in &report.summary.relation_precision {
            output.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} | `{}` |\n",
                relation.relation,
                relation.labeled_samples,
                relation.unlabeled_samples,
                relation.unsupported_samples,
                relation.unsure_samples,
                relation.true_positive,
                relation.false_positive,
                relation.wrong_span,
                format_optional_f64(relation.precision),
                relation.recall_status,
            ));
        }
    }

    output.push_str("\n## Source-Span Precision\n\n");
    output.push_str("| Eligible | Correct span | Wrong span | Precision | Recall |\n");
    output.push_str("| ---: | ---: | ---: | ---: | --- |\n");
    output.push_str(&format!(
        "| {} | {} | {} | {} | `{}` |\n",
        report.summary.source_span_precision.eligible_samples,
        report.summary.source_span_precision.true_positive,
        report.summary.source_span_precision.wrong_span,
        format_optional_f64(report.summary.source_span_precision.precision),
        report.summary.source_span_precision.recall_status,
    ));

    output.push_str("\n## False-Positive Taxonomy\n\n");
    append_taxonomy_table(&mut output, &report.summary.false_positive_taxonomy);
    output.push_str("\n## Wrong-Span Taxonomy\n\n");
    append_taxonomy_table(&mut output, &report.summary.wrong_span_taxonomy);
    output.push_str("\n## Unsupported Pattern Taxonomy\n\n");
    append_taxonomy_table(&mut output, &report.summary.unsupported_pattern_taxonomy);

    output.push_str("\n## Unlabeled Samples\n\n");
    output.push_str("| Type | Relation | Ordinal | Sample ID | Source |\n");
    output.push_str("| --- | --- | ---: | --- | --- |\n");
    let mut unlabeled_count = 0usize;
    for sample in &report.samples {
        if !sample.labeled {
            unlabeled_count += 1;
            output.push_str(&format!(
                "| `{}` | `{}` | {} | `{}` | `{}` |\n",
                sample.sample_type,
                sample.relation,
                sample.ordinal,
                sample.sample_id.replace('`', "'"),
                sample.source_json,
            ));
        }
    }
    if unlabeled_count == 0 {
        output.push_str("| none | none | 0 | none | none |\n");
    }

    output.push_str("\n## Notes\n\n");
    for note in &report.notes {
        output.push_str(&format!("- {note}\n"));
    }
    for note in &report.summary.notes {
        output.push_str(&format!("- {note}\n"));
    }
    output
}

fn append_path_list(output: &mut String, paths: &[String]) {
    if paths.is_empty() {
        output.push_str("- none\n");
    } else {
        for path in paths {
            output.push_str(&format!("- `{}`\n", path.replace('`', "'")));
        }
    }
}

fn append_taxonomy_table(output: &mut String, taxonomy: &[TaxonomyCount]) {
    output.push_str("| Category | Count |\n");
    output.push_str("| --- | ---: |\n");
    if taxonomy.is_empty() {
        output.push_str("| none | 0 |\n");
    } else {
        for entry in taxonomy {
            output.push_str(&format!(
                "| `{}` | {} |\n",
                entry.category.replace('`', "'"),
                entry.count
            ));
        }
    }
}

fn format_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.4}"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn render_relation_counts_markdown(report: &RelationCountsReport) -> String {
    let mut output = String::new();
    output.push_str("# Relation Counts\n\n");
    output.push_str(&format!(
        "Database: `{}`\n\nTotal edges: `{}`\n\n",
        report.db_path, report.total_edges
    ));
    output.push_str("| Relation | Edges | Source spans | Missing span rows | Duplicates | Duplicate status | Derived | Top head types | Top tail types | Type status |\n");
    output.push_str("| --- | ---: | ---: | ---: | ---: | --- | ---: | --- | --- | --- |\n");
    for row in &report.relations {
        output.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | `{}` | {} | {} | {} | `{}` |\n",
            row.relation,
            row.edge_count,
            row.source_span_count,
            row.missing_source_span_rows,
            row.duplicate_edge_count,
            row.duplicate_edge_count_status,
            row.derived_count,
            render_type_counts(&row.top_head_entity_types),
            render_type_counts(&row.top_tail_entity_types),
            row.top_entity_type_status,
        ));
    }
    output.push_str("\n## Notes\n\n");
    for note in &report.notes {
        output.push_str(&format!("- {note}\n"));
    }
    output
}

fn render_type_counts(counts: &[TypeCount]) -> String {
    if counts.is_empty() {
        return "unknown".to_string();
    }
    counts
        .iter()
        .map(|count| format!("{}:{}", count.entity_type, count.count))
        .collect::<Vec<_>>()
        .join(", ")
}

fn run_storage_micro(options: &StorageMicroOptions) -> Result<Value, String> {
    let started = Instant::now();
    let output_root = absolutize_storage_micro_path(&options.out_dir)?;
    let preflight = storage_budget::storage_budget_preflight(
        &options.storage_budget,
        storage_budget::StorageBudgetContext {
            command: "codegraph-mcp audit storage-micro".to_string(),
            repo_root: std::env::current_dir().ok(),
            db_path: None,
            out_path: Some(output_root.clone()),
            explicit_db: false,
            explicit_out: true,
            diagnostic_only: true,
        },
    );
    if preflight.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp audit storage-micro",
                &preflight,
            ),
        ));
    }
    fs::create_dir_all(&output_root).map_err(|error| error.to_string())?;
    let run_id = storage_micro_run_id();
    let run_dir = output_root.join(format!("storage-micro-{run_id}"));
    fs::create_dir_all(&run_dir).map_err(|error| error.to_string())?;
    let fixtures_dir = run_dir.join("fixtures");
    let dbs_dir = run_dir.join("dbs");
    let reports_dir = run_dir.join("reports");
    let logs_dir = run_dir.join("logs");
    for directory in [&fixtures_dir, &dbs_dir, &reports_dir, &logs_dir] {
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }

    let json_path = options
        .json_path
        .clone()
        .unwrap_or_else(|| run_dir.join("storage-micro.json"));
    let markdown_path = options
        .markdown_path
        .clone()
        .unwrap_or_else(|| run_dir.join("storage-micro.md"));
    let mut cases = Vec::new();
    let baseline = StorageMicroCaseSpec {
        name: "schema_baseline_empty_repo".to_string(),
        kind: StorageMicroCaseKind::Simple,
        file_count: 0,
    };
    let baseline_case = run_storage_micro_case(
        &baseline,
        &fixtures_dir,
        &dbs_dir,
        &reports_dir,
        &logs_dir,
        options,
    )?;
    let baseline_bytes = baseline_case["db_absolute_bytes"].as_u64().unwrap_or(0);
    cases.push(baseline_case);

    for spec in storage_micro_case_specs(options) {
        let mut case = run_storage_micro_case(
            &spec,
            &fixtures_dir,
            &dbs_dir,
            &reports_dir,
            &logs_dir,
            options,
        )?;
        let db_bytes = case["db_absolute_bytes"].as_u64().unwrap_or(0);
        let source_bytes = case["source_bytes"].as_u64().unwrap_or(0);
        let delta = db_bytes.saturating_sub(baseline_bytes);
        case["db_delta_vs_schema_baseline"] = json!(delta);
        case["db_bytes_per_source_kb"] = json!(bytes_per_source_kb(db_bytes, source_bytes));
        case["db_delta_bytes_per_source_kb"] = json!(bytes_per_source_kb(delta, source_bytes));
        write_json(&reports_dir.join(format!("{}.json", spec.name)), &case)?;
        cases.push(case);
    }

    let cases = add_storage_micro_series_deltas(cases);
    for case in &cases {
        if let Some(name) = case["case"].as_str() {
            write_json(&reports_dir.join(format!("{name}.json")), case)?;
        }
    }

    let context_pack_checks = if options.no_context_pack {
        json!({
            "ran": false,
            "reason": "--no-context-pack was supplied"
        })
    } else {
        run_storage_micro_context_pack_checks(&cases, &logs_dir)?
    };
    let excluded_scope = storage_micro_excluded_scope_metrics(&cases);
    let duplicate_metrics = storage_micro_duplicate_metrics(&cases);
    let cumulative = storage_micro_cumulative_analysis(&cases);
    let normal_codegraph_db_created = cases.iter().any(|case| {
        case["normal_codegraph_db_exists_after_index"]
            .as_bool()
            .unwrap_or(false)
    });
    let dbstat_available_all_cases = cases
        .iter()
        .all(|case| case["dbstat_available"].as_bool().unwrap_or(false));
    let db_paths = cases
        .iter()
        .filter_map(|case| case["db_path"].as_str().map(PathBuf::from))
        .collect::<Vec<_>>();
    let storage_budget = storage_budget::storage_budget_postflight(
        preflight,
        &db_paths,
        &[
            fixtures_dir.clone(),
            dbs_dir.clone(),
            reports_dir.clone(),
            logs_dir.clone(),
        ],
    );
    let cleanup = if options.keep_artifacts {
        json!({
            "fixtures_removed": false,
            "dbs_removed": false,
            "reason": "--keep-artifacts was supplied"
        })
    } else {
        let fixtures_removed = remove_dir_if_exists(&fixtures_dir)?;
        let dbs_removed = remove_dir_if_exists(&dbs_dir)?;
        json!({
            "fixtures_removed": fixtures_removed,
            "dbs_removed": dbs_removed,
            "reason": "default cleanup removes generated fixture repos and DB files after reports are written"
        })
    };
    let manifest_path = run_dir.join("artifact_manifest.json");
    let report = json!({
        "schema_version": STORAGE_MICRO_SCHEMA_VERSION,
        "report": "storage_micro",
        "status": "ok",
        "diagnostic_only": true,
        "command_namespace": "audit",
        "command": "codegraph-mcp audit storage-micro",
        "run_id": run_id,
        "run_dir": path_string(&run_dir),
        "output_root": path_string(&output_root),
        "generated_at_unix_ms": current_unix_ms(),
        "duration_ms": elapsed_ms(started),
        "options": {
            "cases": storage_micro_case_names(&options.cases),
            "batch_sizes": options.batch_sizes,
            "keep_artifacts": options.keep_artifacts,
            "no_context_pack": options.no_context_pack,
            "respect_gitignore": options.respect_gitignore,
            "max_db_mib": options.storage_budget.max_db_mib,
            "max_artifacts_mib": options.storage_budget.max_artifacts_mib,
            "min_free_disk_gib": options.storage_budget.min_free_disk_gib,
            "extended": options.storage_budget.extended,
            "stress_corpus": options.storage_budget.stress_corpus_normalized(),
        },
        "reports": {
            "json": path_string(&json_path),
            "markdown": path_string(&markdown_path),
            "per_case_dir": path_string(&reports_dir),
            "logs_dir": path_string(&logs_dir),
            "artifact_manifest": path_string(&manifest_path),
        },
        "artifact_dirs": {
            "fixtures": path_string(&fixtures_dir),
            "dbs": path_string(&dbs_dir),
            "reports": path_string(&reports_dir),
            "logs": path_string(&logs_dir),
        },
        "artifacts_kept": options.keep_artifacts,
        "cleanup": cleanup,
        "cases": cases,
        "cumulative_analysis": cumulative,
        "duplicate_template_reuse": duplicate_metrics,
        "inline_test_classification": context_pack_checks.clone(),
        "context_pack_checks": context_pack_checks,
        "excluded_junk_scope": excluded_scope,
        "dbstat_available_all_cases": dbstat_available_all_cases,
        "storage_budget": storage_budget.to_json(),
        "safety": {
            "normal_codegraph_db_created": normal_codegraph_db_created,
            "normal_codegraph_db_mutation": normal_codegraph_db_created,
            "production_agent_use_db_touched": false,
            "db_paths_are_explicit_absolute": cases.iter().all(|case| case["db_path"].as_str().map(|value| Path::new(value).is_absolute()).unwrap_or(false)),
            "temp_db_root": path_string(&dbs_dir),
        },
        "claim_boundaries": [
            "This command is diagnostic-only.",
            "No final intended-performance pass is claimed.",
            "No CodeGraph vs CGC superiority claim is made.",
            "No CGC run is performed.",
            "No Autoresearch run is performed.",
            "No real-world recall claim is made.",
            "No precision claim is made for absent proof-mode relations."
        ],
    });
    write_json(&manifest_path, &storage_micro_manifest(&report))?;
    if storage_budget.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value(
                "codegraph-mcp audit storage-micro",
                &storage_budget,
            ),
        ));
    }
    Ok(report)
}

fn storage_micro_case_specs(options: &StorageMicroOptions) -> Vec<StorageMicroCaseSpec> {
    let mut specs = Vec::new();
    if options.cases.contains(&StorageMicroCaseKind::Simple) {
        for size in &options.batch_sizes {
            specs.push(StorageMicroCaseSpec {
                name: format!("n{size}_simple_2kb_files"),
                kind: StorageMicroCaseKind::Simple,
                file_count: *size,
            });
        }
    }
    if options.cases.contains(&StorageMicroCaseKind::Expression) {
        specs.push(StorageMicroCaseSpec {
            name: "n10_expression_heavy_2kb_files".to_string(),
            kind: StorageMicroCaseKind::Expression,
            file_count: 10,
        });
    }
    if options.cases.contains(&StorageMicroCaseKind::InlineTests) {
        specs.push(StorageMicroCaseSpec {
            name: "n10_inline_rust_tests_2kb_files".to_string(),
            kind: StorageMicroCaseKind::InlineTests,
            file_count: 10,
        });
    }
    if options.cases.contains(&StorageMicroCaseKind::Duplicates) {
        specs.push(StorageMicroCaseSpec {
            name: "n10_duplicate_same_content_files".to_string(),
            kind: StorageMicroCaseKind::Duplicates,
            file_count: 10,
        });
    }
    if options.cases.contains(&StorageMicroCaseKind::ExcludedJunk) {
        specs.push(StorageMicroCaseSpec {
            name: "excluded_junk_dirs_control".to_string(),
            kind: StorageMicroCaseKind::ExcludedJunk,
            file_count: 1,
        });
    }
    specs
}

fn run_storage_micro_case(
    spec: &StorageMicroCaseSpec,
    fixtures_dir: &Path,
    dbs_dir: &Path,
    reports_dir: &Path,
    logs_dir: &Path,
    options: &StorageMicroOptions,
) -> Result<Value, String> {
    let started = Instant::now();
    let repo = fixtures_dir.join(&spec.name);
    fs::create_dir_all(&repo).map_err(|error| error.to_string())?;
    let fixture = write_storage_micro_fixture(&repo, spec)?;
    let db_path = dbs_dir.join(format!("{}.sqlite", spec.name));
    remove_sqlite_family_if_exists(&db_path)?;
    let index_started = Instant::now();
    let summary = index_repo_to_db_with_options(
        &repo,
        &db_path,
        IndexOptions {
            profile: true,
            json: false,
            storage_mode: StorageMode::Proof,
            build_mode: IndexBuildMode::ProofBuildOnly,
            scope: IndexScopeOptions {
                respect_gitignore: options.respect_gitignore,
                ..IndexScopeOptions::default()
            },
            ..IndexOptions::default()
        },
    )
    .map_err(|error| error.to_string())?;
    let index_wall_ms = elapsed_ms(index_started);
    let storage_started = Instant::now();
    let storage = inspect_storage(&db_path)?;
    let storage_wall_ms = elapsed_ms(storage_started);
    let storage_json = reports_dir.join(format!("{}.storage_audit.json", spec.name));
    let storage_md = reports_dir.join(format!("{}.storage_audit.md", spec.name));
    write_json(&storage_json, &storage)?;
    write_text(&storage_md, &render_storage_markdown(&storage))?;
    let requested_counts = storage_micro_requested_counts(&db_path, &storage)?;
    let file_paths = storage_micro_file_paths(&db_path)?;
    let normal_db_after = repo.join(".codegraph").join("codegraph.sqlite").exists();
    let db_bytes = storage.file_family.database_bytes;
    let source_bytes = fixture["source_bytes"].as_u64().unwrap_or(0);
    let log = json!({
        "case": spec.name,
        "repo": path_string(&repo),
        "db": path_string(&db_path),
        "index_status": "ok",
        "index_wall_ms": index_wall_ms,
        "storage_audit_status": "ok",
        "storage_audit_wall_ms": storage_wall_ms,
        "total_wall_ms": elapsed_ms(started),
        "normal_codegraph_db_exists_after_index": normal_db_after,
    });
    write_json(&logs_dir.join(format!("{}.log.json", spec.name)), &log)?;
    let table_bytes = storage
        .objects
        .iter()
        .filter(|object| object.object_type == "table" || object.object_type == "schema")
        .map(|object| object.total_bytes)
        .sum::<u64>();
    let index_bytes = storage
        .objects
        .iter()
        .filter(|object| object.object_type == "index" || object.object_type == "autoindex")
        .map(|object| object.total_bytes)
        .sum::<u64>();
    let largest_tables = storage_micro_largest_objects(&storage, true);
    let largest_indexes = storage_micro_largest_objects(&storage, false);
    let read_only_inspection = json!({
        "main_db_size_before": storage.main_db_size_before,
        "main_db_size_after": storage.main_db_size_after,
        "main_db_mtime_before": storage.main_db_mtime_before,
        "main_db_mtime_after": storage.main_db_mtime_after,
        "main_db_hash_before": storage.main_db_hash_before,
        "main_db_hash_after": storage.main_db_hash_after,
        "main_db_hash_algorithm": storage.main_db_hash_algorithm,
        "sidecars_before": storage.sidecars_before,
        "sidecars_after": storage.sidecars_after,
        "sidecar_status": storage.sidecar_status,
        "artifact_mutated_during_inspection": storage.artifact_mutated_during_inspection,
        "sidecar_only_change": storage.sidecar_only_change,
        "read_only_mode_used": storage.read_only_mode_used,
        "immutable_mode_used": storage.immutable_mode_used,
        "immutable_mode_reason": storage.immutable_mode_reason,
    });
    let mut case = json!({
        "case": spec.name,
        "kind": storage_micro_case_kind_name(spec.kind),
        "file_count": spec.file_count,
        "repo_path": path_string(&repo),
        "db_path": path_string(&db_path),
        "db_path_is_absolute": db_path.is_absolute(),
        "source_bytes": source_bytes,
        "all_generated_source_bytes": fixture["all_generated_source_bytes"].clone(),
        "all_generated_code_files_approximately_2kb": fixture["all_generated_code_files_approximately_2kb"].clone(),
        "index_wall_ms": index_wall_ms,
        "storage_audit_wall_ms": storage_wall_ms,
        "db_absolute_bytes": db_bytes,
        "db_family_bytes": storage.file_family.total_bytes,
        "db_delta_vs_schema_baseline": 0,
        "db_delta_vs_previous_n_case": Value::Null,
        "db_bytes_per_source_kb": bytes_per_source_kb(db_bytes, source_bytes),
        "db_delta_bytes_per_source_kb": Value::Null,
        "dbstat_available": storage.dbstat_available,
        "table_bytes": table_bytes,
        "index_bytes": index_bytes,
        "storage_audit_json": path_string(&storage_json),
        "storage_audit_markdown": path_string(&storage_md),
        "case_log": path_string(&logs_dir.join(format!("{}.log.json", spec.name))),
        "normal_codegraph_db_exists_after_index": normal_db_after,
    });
    if let Some(object) = case.as_object_mut() {
        object.insert("source_files".to_string(), fixture["source_files"].clone());
        object.insert("junk_files".to_string(), fixture["junk_files"].clone());
        object.insert("index_summary".to_string(), json!(summary));
        object.insert("largest_tables".to_string(), largest_tables);
        object.insert("largest_indexes".to_string(), largest_indexes);
        object.insert("requested_row_counts".to_string(), requested_counts);
        object.insert(
            "all_table_counts".to_string(),
            json!(storage.table_row_metrics),
        );
        object.insert(
            "dictionary_metrics".to_string(),
            json!(storage.dictionary_metrics),
        );
        object.insert(
            "qualified_name_metric".to_string(),
            json!(storage.qualified_name_metric),
        );
        object.insert("file_paths".to_string(), json!(file_paths));
        object.insert("read_only_inspection".to_string(), read_only_inspection);
    }
    Ok(case)
}

fn write_storage_micro_fixture(repo: &Path, spec: &StorageMicroCaseSpec) -> Result<Value, String> {
    let mut source_files = Vec::new();
    let mut junk_files = Vec::new();
    match spec.kind {
        StorageMicroCaseKind::Simple if spec.file_count == 0 => {}
        StorageMicroCaseKind::Simple => {
            for index in 0..spec.file_count {
                let label = format!("simple_{index:03}");
                source_files.push(write_storage_micro_source(
                    repo,
                    &format!("src/{label}.rs"),
                    &storage_micro_simple_rust(&label),
                )?);
            }
        }
        StorageMicroCaseKind::Expression => {
            for index in 0..spec.file_count {
                let label = format!("expr_{index:03}");
                source_files.push(write_storage_micro_source(
                    repo,
                    &format!("src/{label}.rs"),
                    &storage_micro_expression_rust(&label),
                )?);
            }
        }
        StorageMicroCaseKind::InlineTests => {
            for index in 0..spec.file_count {
                source_files.push(write_storage_micro_source(
                    repo,
                    &format!("src/inline_prod_{index:03}.rs"),
                    &storage_micro_inline_test_rust(index),
                )?);
            }
        }
        StorageMicroCaseKind::Duplicates => {
            let source = storage_micro_duplicate_rust();
            for index in 0..spec.file_count {
                source_files.push(write_storage_micro_source(
                    repo,
                    &format!("src/duplicates/dup_{index:03}.rs"),
                    &source,
                )?);
            }
        }
        StorageMicroCaseKind::ExcludedJunk => {
            source_files.push(write_storage_micro_source(
                repo,
                "src/kept_control.rs",
                &storage_micro_simple_rust("kept_control"),
            )?);
            for (path, source) in [
                (
                    "target/generated.rs",
                    storage_micro_simple_rust("target_generated"),
                ),
                (
                    "node_modules/pkg/index.js",
                    storage_micro_js_fixture("ignored_node"),
                ),
                ("dist/bundle.rs", storage_micro_simple_rust("dist_bundle")),
                ("build/output.rs", storage_micro_simple_rust("build_output")),
                (
                    "reports/audit/generated.rs",
                    storage_micro_simple_rust("reports_generated"),
                ),
                (
                    ".venv/script.py",
                    storage_micro_python_fixture("ignored_python"),
                ),
                (
                    "__pycache__/cached.py",
                    storage_micro_python_fixture("cached_value"),
                ),
            ] {
                junk_files.push(write_storage_micro_source(repo, path, &source)?);
            }
        }
    }
    let source_bytes = source_files
        .iter()
        .map(|file| file["bytes"].as_u64().unwrap_or(0))
        .sum::<u64>();
    let junk_bytes = junk_files
        .iter()
        .map(|file| file["bytes"].as_u64().unwrap_or(0))
        .sum::<u64>();
    let all_files = source_files
        .iter()
        .chain(junk_files.iter())
        .collect::<Vec<_>>();
    Ok(json!({
        "source_files": source_files,
        "junk_files": junk_files,
        "source_bytes": source_bytes,
        "all_generated_source_bytes": source_bytes + junk_bytes,
        "all_generated_code_files_approximately_2kb": all_files.iter().all(|file| {
            let bytes = file["bytes"].as_u64().unwrap_or(0);
            (1536..=3072).contains(&bytes)
        }),
    }))
}

fn write_storage_micro_source(repo: &Path, relative: &str, source: &str) -> Result<Value, String> {
    let path = repo.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&path, source).map_err(|error| error.to_string())?;
    Ok(json!({
        "path": relative.replace('\\', "/"),
        "bytes": file_size(&path),
        "approximately_2kb": (1536..=3072).contains(&file_size(&path)),
    }))
}

fn storage_micro_simple_rust(label: &str) -> String {
    storage_micro_pad_rust(
        format!(
            "pub fn {label}_score(seed: i32) -> i32 {{\n    let base = seed.wrapping_mul(3);\n    base + 17\n}}\n\npub fn {label}_message(name: &str, seed: i32) -> String {{\n    let score = {label}_score(seed);\n    format!(\"{{name}}:{{score}}\")\n}}\n\npub fn {label}_entry() -> String {{\n    {label}_message(\"{label}\", 7)\n}}\n\npub fn {label}_threshold(seed: i32) -> bool {{\n    {label}_score(seed) > 30\n}}\n"
        ),
        label,
    )
}

fn storage_micro_expression_rust(label: &str) -> String {
    storage_micro_pad_rust(
        format!(
            "pub fn {label}_bucket(value: i32) -> &'static str {{\n    match value {{\n        v if v < 0 => \"negative\",\n        0 => \"zero\",\n        1..=20 => \"small\",\n        21..=100 => \"medium\",\n        _ => \"large\",\n    }}\n}}\n\npub fn {label}_score(input: &[i32]) -> i32 {{\n    input.iter().enumerate().map(|(idx, value)| if idx % 2 == 0 {{ value * 2 }} else {{ value - 3 }}).filter(|value| value % 3 != 0).fold(0, |acc, value| acc + value)\n}}\n\npub fn {label}_entry() -> String {{\n    let seed = [1, 3, 5, 8, 13, 21, 34];\n    let score = {label}_score(&seed);\n    format!(\"{{}}:{{}}\", {label}_bucket(score), score)\n}}\n"
        ),
        label,
    )
}

fn storage_micro_inline_test_rust(index: usize) -> String {
    let label = format!("inline_prod_{index:03}");
    storage_micro_pad_rust(
        format!(
            "pub fn {label}(name: &str) -> String {{\n    format!(\"hello {{name}}\")\n}}\n\npub fn inline_bridge_{index:03}() -> String {{\n    {label}(\"wasif\")\n}}\n\npub fn inline_upper_{index:03}(name: &str) -> String {{\n    inline_bridge_{index:03}().to_uppercase() + \":\" + name\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn {label}_works() {{\n        assert_eq!({label}(\"wasif\"), \"hello wasif\");\n    }}\n}}\n"
        ),
        &label,
    )
}

fn storage_micro_duplicate_rust() -> String {
    storage_micro_pad_rust(
        "pub fn duplicate_shared_score(seed: i32) -> i32 {\n    let doubled = seed.wrapping_mul(2);\n    doubled + 41\n}\n\npub fn duplicate_shared_message(name: &str) -> String {\n    let score = duplicate_shared_score(9);\n    format!(\"{name}:{score}\")\n}\n\npub fn duplicate_shared_entry() -> String {\n    duplicate_shared_message(\"duplicate\")\n}\n".to_string(),
        "duplicate_shared",
    )
}

fn storage_micro_js_fixture(label: &str) -> String {
    let mut source = format!("export function {label}(value) {{\n  return value + 1;\n}}\n");
    while source.len() < 2048 {
        let index = source.len();
        source.push_str(&format!(
            "export function {label}_{index}(value) {{\n  const shifted = value + {index};\n  return shifted * 2;\n}}\n"
        ));
    }
    source
}

fn storage_micro_python_fixture(label: &str) -> String {
    let mut source = format!("def {label}(value):\n    return value + 1\n\n");
    while source.len() < 2048 {
        let index = source.len();
        source.push_str(&format!(
            "def {label}_{index}(value):\n    shifted = value + {index}\n    return shifted * 2\n\n"
        ));
    }
    source
}

fn storage_micro_pad_rust(mut source: String, label: &str) -> String {
    let mut index = 0usize;
    while source.len() < 2048 {
        source.push_str(&format!(
            "\npub fn {label}_pad_{index}(value: i32) -> i32 {{\n    let shifted = value.wrapping_add({});\n    shifted ^ {}\n}}\n",
            index + 3,
            index + 11
        ));
        index += 1;
    }
    source
}

fn storage_micro_requested_counts(
    db_path: &Path,
    storage: &StorageInspection,
) -> Result<Value, String> {
    let connection = open_read_only(db_path)?;
    let object_types = sqlite_object_types(&connection)?;
    let object_bytes = storage
        .objects
        .iter()
        .map(|object| (object.name.clone(), object.total_bytes))
        .collect::<HashMap<_, _>>();
    let requested = [
        "files",
        "file_instance",
        "file_instances",
        "source_content_template",
        "template_entities",
        "template_edges",
        "file_entities",
        "file_edges",
        "file_source_spans",
        "source_spans",
        "path_evidence",
        "path_evidence_edges",
        "symbol_dict",
        "qname_prefix_dict",
        "qualified_name_dict",
        "entities",
        "edges",
        "proof_edges",
    ];
    let mut counts = serde_json::Map::new();
    for name in requested {
        let present = storage_micro_object_exists(&connection, name)?;
        let rows = if present {
            row_count(&connection, name).ok()
        } else {
            None
        };
        counts.insert(
            name.to_string(),
            json!({
                "present": present,
                "type": object_types.get(name),
                "rows": rows,
                "bytes": object_bytes.get(name).copied(),
            }),
        );
    }
    if counts
        .get("proof_edges")
        .and_then(|value| value["present"].as_bool())
        != Some(true)
    {
        if let Some(edges) = counts.get("edges").cloned() {
            counts.insert(
                "proof_edges".to_string(),
                json!({
                    "present": false,
                    "current_equivalent": "edges",
                    "equivalent_rows": edges["rows"],
                    "equivalent_bytes": edges["bytes"],
                }),
            );
        }
    }
    Ok(Value::Object(counts))
}

fn storage_micro_object_exists(connection: &Connection, name: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_schema WHERE name = ?1 AND type IN ('table', 'view') LIMIT 1",
            [name],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|error| error.to_string())
}

fn storage_micro_file_paths(db_path: &Path) -> Result<Vec<String>, String> {
    let connection = open_read_only(db_path)?;
    let attempts = [
        "SELECT repo_relative_path FROM file_instance ORDER BY repo_relative_path",
        "SELECT path_dict.value FROM files JOIN path_dict ON path_dict.id = files.path_id ORDER BY path_dict.value",
    ];
    for sql in attempts {
        if let Ok(mut statement) = connection.prepare(sql) {
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|error| error.to_string())?;
            let mut paths = Vec::new();
            for row in rows {
                paths.push(row.map_err(|error| error.to_string())?.replace('\\', "/"));
            }
            return Ok(paths);
        }
    }
    Ok(Vec::new())
}

fn storage_micro_largest_objects(storage: &StorageInspection, tables: bool) -> Value {
    let values = storage
        .objects
        .iter()
        .filter(|object| {
            if tables {
                object.object_type == "table" || object.object_type == "schema"
            } else {
                object.object_type == "index" || object.object_type == "autoindex"
            }
        })
        .take(10)
        .map(|object| {
            json!({
                "name": object.name,
                "object_type": object.object_type,
                "row_count": object.row_count,
                "bytes": object.total_bytes,
                "payload_bytes": object.payload_bytes,
                "unused_bytes": object.unused_bytes,
            })
        })
        .collect::<Vec<_>>();
    json!(values)
}

fn add_storage_micro_series_deltas(mut cases: Vec<Value>) -> Vec<Value> {
    let baseline = cases
        .iter()
        .find(|case| case["case"].as_str() == Some("schema_baseline_empty_repo"))
        .and_then(|case| case["db_absolute_bytes"].as_u64())
        .unwrap_or(0);
    let mut previous_simple: Option<(usize, u64)> = None;
    for case in &mut cases {
        let db_bytes = case["db_absolute_bytes"].as_u64().unwrap_or(0);
        let source_bytes = case["source_bytes"].as_u64().unwrap_or(0);
        let delta = db_bytes.saturating_sub(baseline);
        case["db_delta_vs_schema_baseline"] = json!(delta);
        case["db_bytes_per_source_kb"] = json!(bytes_per_source_kb(db_bytes, source_bytes));
        case["db_delta_bytes_per_source_kb"] = json!(bytes_per_source_kb(delta, source_bytes));
        if case["kind"].as_str() == Some("simple") && case["file_count"].as_u64().unwrap_or(0) > 0 {
            let file_count = case["file_count"].as_u64().unwrap_or(0) as usize;
            case["db_delta_vs_previous_n_case"] = if let Some((_, previous_bytes)) = previous_simple
            {
                json!(db_bytes.saturating_sub(previous_bytes))
            } else {
                Value::Null
            };
            previous_simple = Some((file_count, db_bytes));
        }
    }
    cases
}

fn storage_micro_cumulative_analysis(cases: &[Value]) -> Value {
    let mut simple = cases
        .iter()
        .filter(|case| case["kind"].as_str() == Some("simple"))
        .filter(|case| case["file_count"].as_u64().unwrap_or(0) > 0)
        .collect::<Vec<_>>();
    simple.sort_by_key(|case| case["file_count"].as_u64().unwrap_or(0));
    let mut slopes = Vec::new();
    for pair in simple.windows(2) {
        let left = pair[0];
        let right = pair[1];
        let left_count = left["file_count"].as_u64().unwrap_or(0);
        let right_count = right["file_count"].as_u64().unwrap_or(0);
        if right_count <= left_count {
            continue;
        }
        let file_delta = (right_count - left_count) as f64;
        let db_delta = right["db_absolute_bytes"].as_u64().unwrap_or(0) as f64
            - left["db_absolute_bytes"].as_u64().unwrap_or(0) as f64;
        let entity_delta = storage_micro_count(right, "entities") as f64
            - storage_micro_count(left, "entities") as f64;
        let edge_delta =
            storage_micro_count(right, "edges") as f64 - storage_micro_count(left, "edges") as f64;
        slopes.push(json!({
            "from_case": left["case"],
            "to_case": right["case"],
            "main_db_bytes_per_file": round3(db_delta / file_delta),
            "bytes_per_entity": if entity_delta > 0.0 { json!(round3(db_delta / entity_delta)) } else { Value::Null },
            "bytes_per_edge": if edge_delta > 0.0 { json!(round3(db_delta / edge_delta)) } else { Value::Null },
        }));
    }
    json!({
        "fixed_overhead_schema_baseline_bytes": cases.iter().find(|case| case["case"].as_str() == Some("schema_baseline_empty_repo")).and_then(|case| case["db_absolute_bytes"].as_u64()),
        "simple_series": simple,
        "slope_estimates": slopes,
    })
}

fn storage_micro_count(case: &Value, table: &str) -> u64 {
    case["requested_row_counts"][table]["rows"]
        .as_u64()
        .or_else(|| case["requested_row_counts"][table]["equivalent_rows"].as_u64())
        .unwrap_or(0)
}

fn storage_micro_duplicate_metrics(cases: &[Value]) -> Value {
    let Some(case) = cases
        .iter()
        .find(|case| case["case"].as_str() == Some("n10_duplicate_same_content_files"))
    else {
        return json!({"ran": false, "reason": "duplicates case was not selected"});
    };
    let files = storage_micro_count(case, "files");
    let templates = storage_micro_count(case, "source_content_template");
    json!({
        "ran": true,
        "files_rows": files,
        "source_content_template_rows": templates,
        "template_reuse_observed": files > templates && templates == 1,
    })
}

fn storage_micro_excluded_scope_metrics(cases: &[Value]) -> Value {
    let Some(case) = cases
        .iter()
        .find(|case| case["case"].as_str() == Some("excluded_junk_dirs_control"))
    else {
        return json!({"ran": false, "reason": "excluded-junk case was not selected"});
    };
    let paths = case["file_paths"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    let prefixes = [
        "target/",
        "node_modules/",
        "dist/",
        "build/",
        "reports/",
        ".venv/",
        "__pycache__/",
    ];
    let mut checks = serde_json::Map::new();
    for prefix in prefixes {
        let matches = paths
            .iter()
            .filter(|path| path.starts_with(prefix) || path.contains(&format!("/{prefix}")))
            .cloned()
            .collect::<Vec<_>>();
        checks.insert(
            prefix.trim_end_matches('/').to_string(),
            json!({
                "status": if matches.is_empty() { "excluded" } else { "included" },
                "matches": matches,
            }),
        );
    }
    json!({
        "ran": true,
        "file_paths_observed": paths,
        "all_junk_prefixes_absent": checks.values().all(|value| value["status"].as_str() == Some("excluded")),
        "dist_policy_observed": checks.get("dist").and_then(|value| value["status"].as_str()).unwrap_or("unknown"),
        "build_policy_observed": checks.get("build").and_then(|value| value["status"].as_str()).unwrap_or("unknown"),
        "prefix_checks": Value::Object(checks),
    })
}

fn run_storage_micro_context_pack_checks(
    cases: &[Value],
    logs_dir: &Path,
) -> Result<Value, String> {
    let Some(case) = cases
        .iter()
        .find(|case| case["case"].as_str() == Some("n10_inline_rust_tests_2kb_files"))
    else {
        return Ok(json!({"ran": false, "reason": "inline-tests case was not selected"}));
    };
    let repo = PathBuf::from(case["repo_path"].as_str().unwrap_or_default());
    let db = PathBuf::from(case["db_path"].as_str().unwrap_or_default());
    let production_args = vec![
        "--task".to_string(),
        "storage micro production context for inline_prod_000".to_string(),
        "--seed".to_string(),
        "inline_prod_000".to_string(),
        "--mode".to_string(),
        "production".to_string(),
        "--agent-json".to_string(),
        "--limit-paths".to_string(),
        "5".to_string(),
        "--limit-snippets".to_string(),
        "8".to_string(),
        "--max-output-bytes".to_string(),
        "25000".to_string(),
    ];
    let test_args = vec![
        "--task".to_string(),
        "storage micro test impact for inline_prod_000_works".to_string(),
        "--seed".to_string(),
        "inline_prod_000_works".to_string(),
        "--mode".to_string(),
        "test-impact".to_string(),
        "--agent-json".to_string(),
        "--limit-paths".to_string(),
        "5".to_string(),
        "--limit-snippets".to_string(),
        "8".to_string(),
        "--max-output-bytes".to_string(),
        "25000".to_string(),
    ];
    let production_started = Instant::now();
    let production = super::with_repo_db_context(&repo, &db, || {
        super::run_context_pack_command(&production_args)
    })?;
    let production_wall_ms = elapsed_ms(production_started);
    let test_started = Instant::now();
    let test_impact =
        super::with_repo_db_context(&repo, &db, || super::run_context_pack_command(&test_args))?;
    let test_wall_ms = elapsed_ms(test_started);
    write_json(
        &logs_dir.join("context_pack_inline_production.json"),
        &production,
    )?;
    write_json(
        &logs_dir.join("context_pack_inline_test_impact.json"),
        &test_impact,
    )?;
    let production_text = serde_json::to_string(&production).map_err(|error| error.to_string())?;
    let test_text = serde_json::to_string(&test_impact).map_err(|error| error.to_string())?;
    let production_roles = collect_json_strings_by_key(&production, "evidence_role");
    let test_roles = collect_json_strings_by_key(&test_impact, "evidence_role");
    Ok(json!({
        "ran": true,
        "production_wall_ms": production_wall_ms,
        "test_impact_wall_ms": test_wall_ms,
        "production_context_excludes_inline_tests": !production_text.contains("inline_prod_000_works") && !production_text.contains("#[test]") && !production_text.contains("cfg(test)"),
        "test_impact_surfaces_inline_tests": test_text.contains("inline_prod_000_works") && (test_text.contains("#[test]") || test_roles.iter().any(|role| role == "test" || role == "mixed")),
        "production_evidence_roles": production_roles,
        "test_impact_evidence_roles": test_roles,
        "fallback_sources": collect_json_strings_by_key(&test_impact, "fallback_source"),
        "recommended_tests": collect_recommended_test_strings(&test_impact),
        "production_log": path_string(&logs_dir.join("context_pack_inline_production.json")),
        "test_impact_log": path_string(&logs_dir.join("context_pack_inline_test_impact.json")),
    }))
}

fn collect_recommended_test_strings(value: &Value) -> Vec<String> {
    let mut tests = BTreeSet::new();
    for recommended in collect_json_values_by_key(value, "recommended_tests") {
        match recommended {
            Value::Array(items) => {
                for item in items {
                    if let Some(test) = item.as_str() {
                        tests.insert(test.to_string());
                    }
                }
            }
            Value::String(test) => {
                tests.insert(test);
            }
            _ => {}
        }
    }
    tests.into_iter().collect()
}

fn collect_json_strings_by_key(value: &Value, key: &str) -> Vec<String> {
    let mut values = BTreeSet::new();
    collect_json_strings_by_key_inner(value, key, &mut values);
    values.into_iter().collect()
}

fn collect_json_strings_by_key_inner(value: &Value, key: &str, values: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (current_key, current_value) in map {
                if current_key == key {
                    if let Some(text) = current_value.as_str() {
                        values.insert(text.to_string());
                    }
                }
                collect_json_strings_by_key_inner(current_value, key, values);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_json_strings_by_key_inner(item, key, values);
            }
        }
        _ => {}
    }
}

fn collect_json_values_by_key(value: &Value, key: &str) -> Vec<Value> {
    let mut values = Vec::new();
    collect_json_values_by_key_inner(value, key, &mut values);
    values
}

fn collect_json_values_by_key_inner(value: &Value, key: &str, values: &mut Vec<Value>) {
    match value {
        Value::Object(map) => {
            for (current_key, current_value) in map {
                if current_key == key {
                    values.push(current_value.clone());
                }
                collect_json_values_by_key_inner(current_value, key, values);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_json_values_by_key_inner(item, key, values);
            }
        }
        _ => {}
    }
}

fn storage_micro_manifest(report: &Value) -> Value {
    json!({
        "schema_version": STORAGE_MICRO_SCHEMA_VERSION,
        "report": "storage_micro_artifact_manifest",
        "run_id": report["run_id"],
        "run_dir": report["run_dir"],
        "reports": report["reports"],
        "artifact_dirs": report["artifact_dirs"],
        "artifacts_kept": report["artifacts_kept"],
        "cleanup": report["cleanup"],
        "storage_budget": report["storage_budget"],
        "case_count": report["cases"].as_array().map(Vec::len).unwrap_or(0),
        "claim_boundaries": report["claim_boundaries"],
    })
}

fn render_storage_micro_markdown(report: &Value) -> String {
    let mut output = String::new();
    output.push_str("# Storage Micro Diagnostic\n\n");
    output.push_str(&format!(
        "Run ID: `{}`\n\n",
        report["run_id"].as_str().unwrap_or("unknown")
    ));
    output.push_str("This is a diagnostic storage micro-lab. It uses isolated generated fixtures and explicit external DB paths under the requested output directory.\n\n");
    output.push_str("No final intended-performance pass, CGC superiority, real-world recall, or unsupported relation precision claim is made.\n\n");
    output.push_str("## Summary\n\n");
    output.push_str(&format!(
        "- Run dir: `{}`\n",
        report["run_dir"].as_str().unwrap_or("")
    ));
    output.push_str(&format!(
        "- Artifacts kept: `{}`\n",
        report["artifacts_kept"].as_bool().unwrap_or(false)
    ));
    output.push_str(&format!(
        "- Normal `.codegraph` DB created: `{}`\n",
        report["safety"]["normal_codegraph_db_created"]
            .as_bool()
            .unwrap_or(false)
    ));
    output.push_str(&format!(
        "- DBSTAT available all cases: `{}`\n\n",
        report["dbstat_available_all_cases"]
            .as_bool()
            .unwrap_or(false)
    ));
    output.push_str("## Storage Budget\n\n");
    output.push_str(&format!(
        "- Status: `{}`\n",
        report["storage_budget"]["budget_status"]
            .as_str()
            .unwrap_or("unknown")
    ));
    output.push_str(&format!(
        "- DB bytes: `{}` / max `{}` MiB\n",
        report["storage_budget"]["db_bytes"].as_u64().unwrap_or(0),
        display_json_number(&report["storage_budget"]["max_db_mib"])
    ));
    output.push_str(&format!(
        "- Artifact bytes: `{}` / max `{}` MiB\n",
        report["storage_budget"]["artifact_bytes"]
            .as_u64()
            .unwrap_or(0),
        display_json_number(&report["storage_budget"]["max_artifacts_mib"])
    ));
    output.push_str(&format!(
        "- Extended: `{}`; stress corpus: `{}`\n\n",
        report["storage_budget"]["extended"]
            .as_bool()
            .unwrap_or(false),
        report["storage_budget"]["stress_corpus"]
            .as_str()
            .unwrap_or("none")
    ));
    output.push_str("## Case Matrix\n\n");
    output.push_str("| Case | Files | Source bytes | DB bytes | Delta vs baseline | Bytes/source KB | files rows | templates | edges | path evidence |\n");
    output.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    if let Some(cases) = report["cases"].as_array() {
        for case in cases {
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                case["case"].as_str().unwrap_or("unknown"),
                case["file_count"].as_u64().unwrap_or(0),
                case["source_bytes"].as_u64().unwrap_or(0),
                case["db_absolute_bytes"].as_u64().unwrap_or(0),
                case["db_delta_vs_schema_baseline"].as_u64().unwrap_or(0),
                display_json_number(&case["db_bytes_per_source_kb"]),
                storage_micro_count(case, "files"),
                storage_micro_count(case, "source_content_template"),
                storage_micro_count(case, "edges"),
                storage_micro_count(case, "path_evidence"),
            ));
        }
    }
    output.push_str("\n## Slope Estimates\n\n");
    if let Some(slopes) = report["cumulative_analysis"]["slope_estimates"].as_array() {
        for slope in slopes {
            output.push_str(&format!(
                "- `{}` -> `{}`: `{}` main DB bytes/file; `{}` bytes/entity; `{}` bytes/edge\n",
                slope["from_case"].as_str().unwrap_or("unknown"),
                slope["to_case"].as_str().unwrap_or("unknown"),
                display_json_number(&slope["main_db_bytes_per_file"]),
                display_json_number(&slope["bytes_per_entity"]),
                display_json_number(&slope["bytes_per_edge"]),
            ));
        }
    }
    output.push_str("\n## Inline Test Classification\n\n");
    output.push_str(&format!(
        "- Context-pack ran: `{}`\n",
        report["context_pack_checks"]["ran"]
            .as_bool()
            .unwrap_or(false)
    ));
    output.push_str(&format!(
        "- Production excludes inline tests: `{}`\n",
        report["context_pack_checks"]["production_context_excludes_inline_tests"]
            .as_bool()
            .unwrap_or(false)
    ));
    output.push_str(&format!(
        "- Test-impact surfaces inline tests: `{}`\n",
        report["context_pack_checks"]["test_impact_surfaces_inline_tests"]
            .as_bool()
            .unwrap_or(false)
    ));
    output.push_str("\n## Excluded Junk Scope\n\n");
    output.push_str(&format!(
        "- Ran: `{}`\n",
        report["excluded_junk_scope"]["ran"]
            .as_bool()
            .unwrap_or(false)
    ));
    output.push_str(&format!(
        "- All junk prefixes absent: `{}`\n",
        report["excluded_junk_scope"]["all_junk_prefixes_absent"]
            .as_bool()
            .unwrap_or(false)
    ));
    output.push_str(&format!(
        "- `dist/` policy observed: `{}`\n",
        report["excluded_junk_scope"]["dist_policy_observed"]
            .as_str()
            .unwrap_or("unknown")
    ));
    output.push_str(&format!(
        "- `build/` policy observed: `{}`\n",
        report["excluded_junk_scope"]["build_policy_observed"]
            .as_str()
            .unwrap_or("unknown")
    ));
    output.push_str("\n## Claim Boundaries\n\n");
    if let Some(boundaries) = report["claim_boundaries"].as_array() {
        for boundary in boundaries {
            output.push_str(&format!("- {}\n", boundary.as_str().unwrap_or("")));
        }
    }
    output
}

fn display_json_number(value: &Value) -> String {
    if value.is_null() {
        "null".to_string()
    } else if let Some(number) = value.as_f64() {
        format!("{number:.3}")
    } else {
        value.to_string()
    }
}

fn bytes_per_source_kb(bytes: u64, source_bytes: u64) -> Value {
    if source_bytes == 0 {
        Value::Null
    } else {
        json!(round3(bytes as f64 / (source_bytes as f64 / 1024.0)))
    }
}

fn storage_micro_case_kind_name(kind: StorageMicroCaseKind) -> &'static str {
    match kind {
        StorageMicroCaseKind::Simple => "simple",
        StorageMicroCaseKind::Expression => "expression",
        StorageMicroCaseKind::InlineTests => "inline-tests",
        StorageMicroCaseKind::Duplicates => "duplicates",
        StorageMicroCaseKind::ExcludedJunk => "excluded-junk",
    }
}

fn storage_micro_case_names(cases: &BTreeSet<StorageMicroCaseKind>) -> Vec<&'static str> {
    cases
        .iter()
        .map(|case| storage_micro_case_kind_name(*case))
        .collect()
}

fn storage_micro_run_id() -> String {
    format!("{}-{}", current_unix_ms(), std::process::id())
}

fn current_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn absolutize_storage_micro_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path))
    }
}

fn remove_dir_if_exists(path: &Path) -> Result<bool, String> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|error| error.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn remove_sqlite_family_if_exists(db_path: &Path) -> Result<(), String> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let candidate = PathBuf::from(format!("{}{}", db_path.display(), suffix));
        if candidate.exists() {
            fs::remove_file(&candidate).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn write_optional_outputs<T: Serialize>(
    report: &T,
    markdown: &str,
    json_path: &Option<PathBuf>,
    markdown_path: &Option<PathBuf>,
) -> Result<(), String> {
    if let Some(path) = json_path {
        write_json(path, report)?;
    }
    if let Some(path) = markdown_path {
        write_text(path, markdown)?;
    }
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, format!("{json}\n")).map_err(|error| error.to_string())
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, text).map_err(|error| error.to_string())
}

fn default_audit_db_path() -> PathBuf {
    std::env::var_os("CODEGRAPH_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".codegraph").join("codegraph.sqlite"))
}

fn take_path(args: &[String], index: &mut usize, flag: &str) -> Result<PathBuf, String> {
    take_value(args, index, flag).map(PathBuf::from)
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn percent(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        ((part as f64 / total as f64) * 10_000.0).round() / 100.0
    }
}

fn path_string(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string().replace('\\', "/")
}

fn vector_string(metadata: &Value, root: &Value, paths: &[&[&str]]) -> Option<String> {
    paths
        .iter()
        .find_map(|path| value_at(metadata, path).or_else(|| value_at(root, path)))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn vector_bool(metadata: &Value, root: &Value, paths: &[&[&str]]) -> Option<bool> {
    paths
        .iter()
        .find_map(|path| value_at(metadata, path).or_else(|| value_at(root, path)))
        .and_then(Value::as_bool)
}

fn vector_u64(metadata: &Value, root: &Value, paths: &[&[&str]]) -> Option<u64> {
    paths
        .iter()
        .find_map(|path| value_at(metadata, path).or_else(|| value_at(root, path)))
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        })
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn chunk_str<'a>(chunk: &'a Value, field: &str) -> Option<&'a str> {
    chunk.get(field).and_then(Value::as_str)
}

fn chunk_bool(chunk: &Value, field: &str) -> Option<bool> {
    chunk.get(field).and_then(Value::as_bool)
}

fn chunk_string_value(chunk: &Value, field: &str) -> Value {
    chunk
        .get(field)
        .and_then(Value::as_str)
        .map(|value| Value::String(value.to_string()))
        .unwrap_or(Value::Null)
}

fn chunk_requires_graph_verification(chunk: &Value) -> bool {
    chunk_bool(chunk, "requires_graph_verification").unwrap_or_else(|| {
        let has_entity = chunk
            .get("entity_id")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty());
        let graph_entity = chunk_str(chunk, "source_kind") == Some("graph_entity");
        has_entity || graph_entity
    })
}

fn infer_vector_file_kind(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".rs")
        || lower.ends_with(".py")
        || lower.ends_with(".c")
        || lower.ends_with(".h")
        || lower.ends_with(".cc")
        || lower.ends_with(".cpp")
        || lower.ends_with(".js")
        || lower.ends_with(".ts")
        || lower.ends_with(".java")
        || lower.ends_with(".go")
    {
        "source".to_string()
    } else if lower.ends_with(".md") || lower.ends_with(".adoc") || lower.ends_with(".rst") {
        "documentation".to_string()
    } else if lower.ends_with(".mk") || lower.ends_with("makefile") {
        "makefile".to_string()
    } else if lower.ends_with("config.in") || lower.contains("/config.in") {
        "kconfig".to_string()
    } else if path.is_empty() {
        "unknown".to_string()
    } else {
        "other".to_string()
    }
}

fn increment_count(map: &mut BTreeMap<String, u64>, key: &str) {
    *map.entry(key.to_string()).or_insert(0) += 1;
}

fn truncate_preview(text: &str, max_chars: usize) -> (String, bool) {
    let mut preview = String::new();
    let mut truncated = false;
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            truncated = true;
            break;
        }
        preview.push(ch);
    }
    if truncated {
        preview.push_str("...");
    }
    (preview.replace('\n', "\\n"), truncated)
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::{
        Edge, EdgeClass, EdgeContext, Entity, EntityKind, Exactness, FileRecord, PathEvidence,
        RelationKind, SourceSpan,
    };
    use codegraph_store::{GraphStore, SqliteGraphStore};

    #[test]
    fn vector_chunk_inspector_legacy_pretty_json_fixture() {
        let root = temp_audit_dir("vector-legacy-pretty");
        let artifact = root.join("legacy.vector.json");
        write_json(&artifact, &legacy_vector_fixture("short legacy text")).expect("write fixture");

        let options = vector_test_options(Some(artifact.clone()));
        let report = inspect_vector_chunks_artifact(&options, &artifact, None);

        assert_vector_chunk_schema(&report);
        assert_eq!(report["status"].as_str(), Some("ok"));
        assert_eq!(
            report["artifact_identity"]["artifact_kind"].as_str(),
            Some("legacy_pretty_json_vector_artifact")
        );
        assert_eq!(
            report["artifact_identity"]["artifact_format"].as_str(),
            Some("pretty_json")
        );
        assert_eq!(
            report["safety_conclusions"]["stores_embedding_vectors"].as_bool(),
            Some(false)
        );
        assert_eq!(
            report["safety_conclusions"]["deterministic_embeddings_regenerated_from_chunk_text"]
                .as_bool(),
            Some(true)
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn vector_chunk_inspector_compact_runtime_fixture() {
        let root = temp_audit_dir("vector-runtime-compact");
        let artifact = root.join("runtime.vector.json");
        fs::write(
            &artifact,
            serde_json::to_string(&runtime_vector_fixture("compact runtime text"))
                .expect("compact json"),
        )
        .expect("write fixture");

        let options = vector_test_options(Some(artifact.clone()));
        let report = inspect_vector_chunks_artifact(&options, &artifact, None);

        assert_vector_chunk_schema(&report);
        assert_eq!(
            report["artifact_identity"]["artifact_kind"].as_str(),
            Some("vector_runtime_sidecar")
        );
        assert_eq!(
            report["artifact_identity"]["artifact_format"].as_str(),
            Some("compact_json")
        );
        assert_eq!(
            report["chunk_counts"]["runtime_total_chunks"].as_u64(),
            Some(2)
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn vector_chunk_inspector_runtime_and_audit_split_fixture() {
        let root = temp_audit_dir("vector-runtime-audit-split");
        let runtime = root.join("runtime.vector.json");
        let audit = root.join("audit.vector.json");
        fs::write(
            &runtime,
            serde_json::to_string(&runtime_vector_fixture("compact runtime text"))
                .expect("runtime json"),
        )
        .expect("write runtime");
        write_json(&audit, &audit_vector_fixture("verbose audit text")).expect("write audit");

        let options = VectorChunksOptions {
            artifact_path: None,
            runtime_sidecar_path: Some(runtime.clone()),
            audit_artifact_path: Some(audit.clone()),
            db_path: None,
            repo: None,
            json_path: None,
            markdown_path: None,
            sample_limit: 2,
        };
        let report = inspect_vector_chunks_artifact(&options, &runtime, None);

        assert_vector_chunk_schema(&report);
        assert_eq!(
            report["artifact_identity"]["artifact_kind"].as_str(),
            Some("vector_runtime_sidecar")
        );
        assert_eq!(
            report["related_artifacts"]["runtime_will_use"].as_str(),
            Some(path_string(&runtime).as_str())
        );
        assert_eq!(
            report["related_artifacts"]["audit_required_for_runtime"].as_bool(),
            Some(false)
        );
        assert!(
            report["related_artifacts"]["overhead_reduction_bytes"]
                .as_u64()
                .unwrap_or_default()
                > 0
        );
        assert_eq!(
            report["related_artifacts"]["audit_artifact"]["artifact_kind"].as_str(),
            Some("audit_artifact")
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn vector_chunk_inspector_candidate_spool_jsonl_fixture() {
        let root = temp_audit_dir("vector-spool-jsonl");
        let artifact = root.join("candidate-spool.jsonl");
        let query_index = candidate_spool_query_index_path(&artifact);
        fs::write(&query_index, "sqlite placeholder").expect("write placeholder query index");
        let mut metadata = vector_fixture_metadata(Some("candidate_spool"), "jsonl");
        if let Some(object) = metadata.as_object_mut() {
            object.insert("query_index_status".to_string(), json!("ready"));
            object.insert("query_index_kind".to_string(), json!("sqlite"));
            object.insert(
                "query_index_path".to_string(),
                json!(path_string(&query_index)),
            );
            object.insert("query_index_record_count".to_string(), json!(2));
            object.insert(
                "query_index_version".to_string(),
                json!("candidate_spool_query_index_v1"),
            );
        }
        let manifest = json!({
            "metadata": metadata,
        });
        let chunks = vector_fixture_chunks("spool snippet");
        let mut lines = vec![serde_json::to_string(&manifest).expect("manifest line")];
        lines.extend(
            chunks
                .iter()
                .map(|chunk| serde_json::to_string(chunk).expect("chunk line")),
        );
        fs::write(&artifact, format!("{}\n", lines.join("\n"))).expect("write jsonl");

        let options = vector_test_options(Some(artifact.clone()));
        let report = inspect_vector_chunks_artifact(&options, &artifact, None);

        assert_vector_chunk_schema(&report);
        assert_eq!(
            report["artifact_identity"]["artifact_kind"].as_str(),
            Some("candidate_spool")
        );
        assert_eq!(
            report["artifact_identity"]["artifact_format"].as_str(),
            Some("jsonl")
        );
        assert_eq!(
            report["chunk_counts"]["spooled_total_chunks"].as_u64(),
            Some(2)
        );
        assert_eq!(
            report["query_index"]["query_index_status"].as_str(),
            Some("ready")
        );
        assert_eq!(
            report["query_index"]["query_index_kind"].as_str(),
            Some("sqlite")
        );
        assert_eq!(
            report["query_index"]["query_index_record_count"].as_u64(),
            Some(2)
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn vector_chunk_inspector_corrupt_artifact_is_structured_error() {
        let root = temp_audit_dir("vector-corrupt");
        let artifact = root.join("corrupt.vector.json");
        fs::write(&artifact, "{ not valid json").expect("write corrupt");

        let options = vector_test_options(Some(artifact.clone()));
        let report = inspect_vector_chunks_artifact(&options, &artifact, None);

        assert_eq!(report["status"].as_str(), Some("error"));
        assert_eq!(
            report["error"]["kind"].as_str(),
            Some("artifact_parse_failed")
        );
        assert_eq!(
            report["passport_lifecycle_provider"]["validity_status"].as_str(),
            Some("corrupt")
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn vector_chunk_inspector_missing_artifact_is_structured_error() {
        let root = temp_audit_dir("vector-missing");
        let artifact = root.join("missing.vector.json");

        let options = vector_test_options(Some(artifact.clone()));
        let report = inspect_vector_chunks_artifact(&options, &artifact, None);

        assert_eq!(report["status"].as_str(), Some("error"));
        assert_eq!(report["error"]["kind"].as_str(), Some("artifact_missing"));
        assert_eq!(
            report["artifact_identity"]["artifact_exists"].as_bool(),
            Some(false)
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn vector_chunk_inspector_byte_accounting_reports_text_and_metadata() {
        let root = temp_audit_dir("vector-byte-accounting");
        let artifact = root.join("legacy.vector.json");
        write_json(&artifact, &legacy_vector_fixture("byte accounting text")).expect("write");

        let options = vector_test_options(Some(artifact.clone()));
        let report = inspect_vector_chunks_artifact(&options, &artifact, None);

        assert!(
            report["byte_accounting"]["actual_index_file_bytes"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(
            report["byte_accounting"]["indexed_chunk_text_bytes"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(
            report["byte_accounting"]["metadata_estimated_bytes"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(
            report["byte_accounting"]["repeated_field_overhead_estimate"]
                .as_u64()
                .unwrap()
                > 0
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn vector_chunk_inspector_never_outputs_full_source_body() {
        let root = temp_audit_dir("vector-no-full-body");
        let artifact = root.join("legacy.vector.json");
        let long_text = "FULL_SOURCE_BODY_MARKER ".repeat(80);
        write_json(&artifact, &legacy_vector_fixture(&long_text)).expect("write");

        let options = vector_test_options(Some(artifact.clone()));
        let report = inspect_vector_chunks_artifact(&options, &artifact, None);
        let encoded = serde_json::to_string(&report).expect("report json");

        assert!(!encoded.contains(&long_text));
        assert_eq!(
            report["sample_chunks"][0]["text_preview_truncated"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report["safety_conclusions"]["stores_full_source_body"].as_bool(),
            Some(false)
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn vector_chunk_inspector_does_not_mutate_artifact_or_db() {
        let root = temp_audit_dir("vector-no-mutation");
        let artifact = root.join("legacy.vector.json");
        let db = root.join("codegraph.sqlite");
        write_json(&artifact, &legacy_vector_fixture("mutation check text")).expect("write");
        create_storage_experiment_fixture_db(&db);
        remove_test_sidecars(&db);
        let artifact_hash_before = stable_file_hash(&artifact);
        let db_hash_before = stable_file_hash(&db);

        let options = VectorChunksOptions {
            artifact_path: Some(artifact.clone()),
            runtime_sidecar_path: None,
            audit_artifact_path: None,
            db_path: Some(db.clone()),
            repo: None,
            json_path: None,
            markdown_path: None,
            sample_limit: 2,
        };
        let report = inspect_vector_chunks_artifact(&options, &artifact, Some(&db));

        assert_eq!(artifact_hash_before, stable_file_hash(&artifact));
        assert_eq!(db_hash_before, stable_file_hash(&db));
        assert_eq!(
            report["mutation_check"]["artifact_mutated_during_inspection"].as_bool(),
            Some(false)
        );
        assert_eq!(
            report["passport_lifecycle_provider"]["db_binding"]["db_mutated_during_inspection"]
                .as_bool(),
            Some(false)
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn vector_chunk_inspector_reports_stale_source_binding_without_mutation() {
        let root = temp_audit_dir("vector-source-binding-stale");
        let repo = root.join("repo");
        fs::create_dir_all(repo.join("src")).expect("create src");
        fs::write(repo.join("src").join("lib.rs"), "pub fn live() {}\n").expect("write live");
        let artifact = root.join("runtime.vector.json");
        let mut fixture = runtime_vector_fixture("bound runtime text");
        if let Some(chunk) = fixture
            .get_mut("chunks")
            .and_then(Value::as_array_mut)
            .and_then(|chunks| chunks.first_mut())
            .and_then(Value::as_object_mut)
        {
            chunk.insert(
                "source_file_content_hash".to_string(),
                json!(content_hash("pub fn old() {}\n")),
            );
            chunk.insert("source_file_size_bytes".to_string(), json!(16));
        }
        fs::write(
            &artifact,
            serde_json::to_string(&fixture).expect("fixture json"),
        )
        .expect("write runtime fixture");
        let artifact_hash_before = stable_file_hash(&artifact);

        let options = VectorChunksOptions {
            artifact_path: Some(artifact.clone()),
            runtime_sidecar_path: None,
            audit_artifact_path: None,
            db_path: None,
            repo: Some(repo.clone()),
            json_path: None,
            markdown_path: None,
            sample_limit: 2,
        };
        let report = inspect_vector_chunks_artifact(&options, &artifact, None);

        assert_eq!(report["status"].as_str(), Some("ok"));
        assert_eq!(
            report["passport_lifecycle_provider"]["validity_status"].as_str(),
            Some("stale")
        );
        assert_eq!(
            report["passport_lifecycle_provider"]["source_binding"]["status"].as_str(),
            Some("stale")
        );
        assert_eq!(artifact_hash_before, stable_file_hash(&artifact));
        assert_eq!(
            report["mutation_check"]["artifact_mutated_during_inspection"].as_bool(),
            Some(false)
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn vector_chunk_inspector_json_schema_shape() {
        let root = temp_audit_dir("vector-schema-shape");
        let artifact = root.join("legacy.vector.json");
        write_json(&artifact, &legacy_vector_fixture("schema shape text")).expect("write");

        let options = vector_test_options(Some(artifact.clone()));
        let report = inspect_vector_chunks_artifact(&options, &artifact, None);

        assert_vector_chunk_schema(&report);
        serde_json::to_string(&report).expect("valid json serialization");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn vector_chunk_inspector_writes_markdown_report_when_requested() {
        let root = temp_audit_dir("vector-markdown");
        let artifact = root.join("legacy.vector.json");
        let json_out = root.join("report.json");
        let md_out = root.join("report.md");
        write_json(&artifact, &legacy_vector_fixture("markdown report text")).expect("write");

        let args = vec![
            "--artifact".to_string(),
            path_string(&artifact),
            "--json".to_string(),
            path_string(&json_out),
            "--markdown".to_string(),
            path_string(&md_out),
            "--sample".to_string(),
            "1".to_string(),
        ];
        let report = run_vector_chunks_command(&args).expect("run vector chunks");

        assert_vector_chunk_schema(&report);
        assert!(json_out.exists());
        assert!(md_out.exists());
        let markdown = fs::read_to_string(&md_out).expect("read markdown");
        assert!(markdown.contains("Vector Chunk Artifact Inspector"));
        assert!(markdown.contains("candidate-only"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn infer_context_keeps_unknown_when_context_is_not_first_class() {
        assert_eq!(
            infer_context("CALLS", "src/auth.ts", "login", "save"),
            "production_inferred"
        );
        assert_eq!(
            infer_context("MOCKS", "src/auth.test.ts", "mockAuth", "login"),
            "mock_or_stub_inferred"
        );
        assert_eq!(
            infer_context("ASSERTS", "src/auth.spec.ts", "authTest", "login"),
            "test_or_fixture_inferred"
        );
    }

    #[test]
    fn render_type_counts_is_stable() {
        let rendered = render_type_counts(&[
            TypeCount {
                entity_type: "Function".to_string(),
                count: 2,
            },
            TypeCount {
                entity_type: "Method".to_string(),
                count: 1,
            },
        ]);
        assert_eq!(rendered, "Function:2, Method:1");
    }

    #[test]
    fn manual_label_markdown_ingests_human_fields() {
        let labels = parse_markdown_labels(
            r#"
## Sample 1

- true_positive: yes
- wrong_span: x
- wrong_span_cause: callsite includes receiver
- notes: checked against source

## Path Sample 2

- unsupported: true
- unsupported_pattern: dynamic runtime dispatch
"#,
        );

        let first = labels.get(&1).expect("sample 1 labels");
        assert!(first.true_positive);
        assert!(first.wrong_span);
        assert_eq!(
            first.wrong_span_cause.as_deref(),
            Some("callsite includes receiver")
        );
        let second = labels.get(&2).expect("sample 2 labels");
        assert!(second.is_unsupported());
        assert_eq!(
            second.unsupported_pattern.as_deref(),
            Some("dynamic runtime dispatch")
        );
    }

    #[test]
    fn label_summary_reports_relation_and_source_span_precision() {
        let samples = vec![
            test_labeled_sample(
                "edge",
                "CALLS",
                ManualLabelSet {
                    true_positive: true,
                    ..ManualLabelSet::default()
                },
            ),
            test_labeled_sample(
                "edge",
                "CALLS",
                ManualLabelSet {
                    wrong_target: true,
                    false_positive_cause: Some("same-name collision".to_string()),
                    ..ManualLabelSet::default()
                },
            ),
            test_labeled_sample(
                "edge",
                "READS",
                ManualLabelSet {
                    true_positive: true,
                    wrong_span: true,
                    wrong_span_cause: Some("line too broad".to_string()),
                    ..ManualLabelSet::default()
                },
            ),
            test_labeled_sample(
                "path",
                "PathEvidence",
                ManualLabelSet {
                    unsupported: true,
                    unsupported_pattern: Some("generated fallback path".to_string()),
                    ..ManualLabelSet::default()
                },
            ),
            test_labeled_sample("edge", "WRITES", ManualLabelSet::default()),
        ];

        let summary = summarize_labeled_samples(&samples);
        let calls = summary
            .relation_precision
            .iter()
            .find(|row| row.relation == "CALLS")
            .expect("CALLS summary");
        assert_eq!(calls.labeled_samples, 2);
        assert_eq!(calls.true_positive, 1);
        assert_eq!(calls.false_positive, 1);
        assert_eq!(calls.precision, Some(0.5));
        assert_eq!(summary.source_span_precision.eligible_samples, 3);
        assert_eq!(summary.source_span_precision.wrong_span, 1);
        assert_eq!(summary.source_span_precision.precision, Some(0.6667));
        assert!(summary
            .false_positive_taxonomy
            .iter()
            .any(|entry| entry.category == "wrong_target" && entry.count == 1));
        assert!(summary
            .unsupported_pattern_taxonomy
            .iter()
            .any(|entry| entry.category == "generated fallback path" && entry.count == 1));
    }

    #[test]
    fn storage_experiment_refuses_original_db_as_copy_target() {
        let root = temp_audit_dir("storage-experiment-refusal");
        let db = root.join("codegraph.sqlite");
        create_storage_experiment_fixture_db(&db);
        let error = ensure_experiment_copy_path(&db, &db).expect_err("should reject original path");
        assert!(error.contains("refusing to run storage experiment"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn storage_experiment_result_json_has_expected_shape() {
        let root = temp_audit_dir("storage-experiment-format");
        let db = root.join("codegraph.sqlite");
        create_storage_experiment_fixture_db(&db);
        let options = StorageExperimentOptions {
            db_path: db,
            workdir: root.join("work"),
            json_path: None,
            markdown_path: None,
            keep_copies: false,
        };
        let report = run_storage_experiments(&options).expect("storage experiments");
        let value = serde_json::to_value(&report).expect("report JSON");
        assert_eq!(value["schema_version"].as_u64(), Some(1));
        assert!(value["experiments"].as_array().expect("experiments").len() >= 8);
        let first = &value["experiments"].as_array().expect("experiments")[0];
        assert!(first["copy_removed"].as_bool().is_some());
        assert!(first["summary"]["db_size_before_bytes"].is_number());
        assert!(first["summary"]["core_query_latency_before_after"].is_array());
        assert!(first["context_packet"]["status"].is_string());
        assert!(first["recommendation"]["recommended"].is_boolean());
        assert!(!first["checkpoints"]
            .as_array()
            .expect("checkpoints")
            .is_empty());
        assert!(first["checkpoints"][0]["query_latencies"]
            .as_array()
            .expect("query latencies")
            .iter()
            .any(|query| query["name"].as_str() == Some("edge_head_relation_lookup")));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn storage_experiments_do_not_mutate_original_db() {
        let root = temp_audit_dir("storage-experiment-original-safe");
        let db = root.join("codegraph.sqlite");
        create_storage_experiment_fixture_db(&db);
        let before_size = file_family_size(&db);
        let before = inspect_storage(&db).expect("before storage inspection");
        let options = StorageExperimentOptions {
            db_path: db.clone(),
            workdir: root.join("work"),
            json_path: None,
            markdown_path: None,
            keep_copies: false,
        };

        let _report = run_storage_experiments(&options).expect("storage experiments");
        let after_size = file_family_size(&db);
        let after = inspect_storage(&db).expect("after storage inspection");

        assert_eq!(before_size.database_bytes, after_size.database_bytes);
        assert_eq!(before_size.wal_bytes, after_size.wal_bytes);
        assert_eq!(
            before.aggregate_metrics.edge_count,
            after.aggregate_metrics.edge_count
        );
        assert_eq!(
            before.aggregate_metrics.source_span_count,
            after.aggregate_metrics.source_span_count
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn storage_inspection_reports_index_usage_and_core_query_plans() {
        let root = temp_audit_dir("storage-inspection-forensics");
        let db = root.join("codegraph.sqlite");
        create_storage_experiment_fixture_db(&db);
        remove_test_sidecars(&db);

        let report = inspect_storage(&db).expect("storage inspection");
        let value = serde_json::to_value(&report).expect("storage JSON");

        assert_eq!(
            value["artifact_mutated_during_inspection"].as_bool(),
            Some(false)
        );
        assert_eq!(value["sidecar_only_change"].as_bool(), Some(false));
        assert_eq!(value["sidecar_status"].as_str(), Some("none"));
        assert_eq!(value["immutable_mode_used"].as_bool(), Some(true));
        assert!(value["sidecars_before"]
            .as_array()
            .expect("sidecars before")
            .is_empty());
        assert!(value["sidecars_after"]
            .as_array()
            .expect("sidecars after")
            .is_empty());
        assert_eq!(value["integrity_check"]["status"].as_str(), Some("ok"));
        assert!(value["aggregate_metrics"]["average_database_bytes_per_edge"].is_number());
        assert!(value["table_row_metrics"]
            .as_array()
            .expect("table metrics")
            .iter()
            .any(|row| row["table"].as_str() == Some("edges")));
        assert!(value["index_usage"]
            .as_array()
            .expect("index usage")
            .iter()
            .any(
                |index| index["name"].as_str() == Some("idx_edges_head_relation")
                    && index["default_query_usage"].is_array()
            ));
        assert!(value["core_query_plans"]
            .as_array()
            .expect("core query plans")
            .iter()
            .any(
                |query| query["name"].as_str() == Some("unresolved_calls_paginated")
                    && query["explain_query_plan"].is_array()
            ));
        assert!(render_storage_markdown(&report).contains("Core Query Plans"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn schema_check_uses_immutable_read_only_without_creating_sidecars() {
        let root = temp_audit_dir("schema-check-read-only");
        let db = root.join("codegraph.sqlite");
        create_storage_experiment_fixture_db(&db);
        remove_test_sidecars(&db);
        let before = db_file_snapshot(&db);
        assert!(before.sidecars.is_empty());

        let report = validate_schema(&db).expect("schema check");
        let after = db_file_snapshot(&db);

        assert_eq!(report.artifact_mutated_during_inspection, false);
        assert_eq!(report.sidecar_only_change, false);
        assert_eq!(report.sidecar_status, "none");
        assert!(report.immutable_mode_used);
        assert_eq!(before.main_db_size, after.main_db_size);
        assert_eq!(before.main_db_mtime_unix_ms, after.main_db_mtime_unix_ms);
        assert_eq!(before.main_db_hash, after.main_db_hash);
        assert!(after.sidecars.is_empty());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn schema_check_does_not_migrate_old_schema_db() {
        let root = temp_audit_dir("schema-check-old-schema");
        let db = root.join("old.sqlite");
        {
            let connection = Connection::open(&db).expect("open old schema");
            connection
                .execute_batch(
                    "
                    PRAGMA user_version = 1;
                    CREATE TABLE legacy_only(id INTEGER PRIMARY KEY);
                    ",
                )
                .expect("create old schema");
        }
        remove_test_sidecars(&db);

        let report = validate_schema(&db).expect("schema check old db");
        let user_version_after = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open old db read-only")
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .expect("user version");

        assert_eq!(report.user_version, 1);
        assert_eq!(user_version_after, 1);
        assert_eq!(report.artifact_mutated_during_inspection, false);
        assert!(!report.failures.is_empty());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn inspection_audit_classifies_sidecar_only_changes() {
        let before = DbFileSnapshot {
            main_db_size: 10,
            main_db_mtime_unix_ms: Some(100),
            main_db_hash: Some("abc".to_string()),
            sidecars: Vec::new(),
        };
        let after = DbFileSnapshot {
            main_db_size: 10,
            main_db_mtime_unix_ms: Some(100),
            main_db_hash: Some("abc".to_string()),
            sidecars: vec![SidecarSnapshot {
                kind: "shm".to_string(),
                path: "db.sqlite-shm".to_string(),
                size: 32768,
                mtime_unix_ms: Some(101),
                hash: Some("def".to_string()),
            }],
        };

        let audit = read_only_inspection_audit(
            before,
            after,
            "sqlite_uri_mode_ro_query_only".to_string(),
            false,
            "immutable=1 not used because WAL/SHM sidecars were present".to_string(),
        );

        assert_eq!(audit.artifact_mutated_during_inspection, false);
        assert_eq!(audit.sidecar_only_change, true);
        assert_eq!(audit.sidecar_status, "sidecar_only_change");
    }

    #[test]
    fn sample_edges_is_deterministic_for_seed() {
        let root = temp_audit_dir("sample-deterministic");
        let db = root.join("codegraph.sqlite");
        create_storage_experiment_fixture_db(&db);
        let mut options = sample_edges_test_options(db.clone());
        options.seed = 42;

        let first = sample_edges(&options).expect("first sample");
        let second = sample_edges(&options).expect("second sample");

        let first_ids = first
            .samples
            .iter()
            .map(|sample| sample.edge_id.clone())
            .collect::<Vec<_>>();
        let second_ids = second
            .samples
            .iter()
            .map(|sample| sample.edge_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(first_ids, second_ids);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sample_edges_applies_relation_filter() {
        let root = temp_audit_dir("sample-relation-filter");
        let db = root.join("codegraph.sqlite");
        create_storage_experiment_fixture_db(&db);

        let mut calls = sample_edges_test_options(db.clone());
        calls.relation = Some("CALLS".to_string());
        let calls_report = sample_edges(&calls).expect("CALLS sample");
        assert_eq!(calls_report.samples.len(), 1);
        assert!(calls_report
            .samples
            .iter()
            .all(|sample| sample.relation == "CALLS"));

        let mut reads = sample_edges_test_options(db);
        reads.relation = Some("READS".to_string());
        let reads_report = sample_edges(&reads).expect("READS sample");
        assert!(reads_report.samples.is_empty());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sample_edges_report_serializes_to_valid_json() {
        let root = temp_audit_dir("sample-json");
        let db = root.join("codegraph.sqlite");
        create_storage_experiment_fixture_db(&db);

        let report = sample_edges(&sample_edges_test_options(db)).expect("sample");
        let value = serde_json::to_value(&report).expect("valid JSON value");

        assert_eq!(value["schema_version"].as_u64(), Some(1));
        assert_eq!(
            value["samples"][0]["relation_direction"].as_str(),
            Some("head_to_tail")
        );
        assert!(value["samples"][0]["missing_metadata"].is_array());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sample_edges_missing_snippet_does_not_crash() {
        let root = temp_audit_dir("sample-missing-snippet");
        let db = root.join("codegraph.sqlite");
        create_storage_experiment_fixture_db(&db);
        let mut options = sample_edges_test_options(db);
        options.include_snippets = true;

        let report = sample_edges(&options).expect("sample with missing snippet");

        assert_eq!(report.samples.len(), 1);
        assert_eq!(report.samples[0].span_loaded, false);
        assert!(report.samples[0].span_load_error.is_some());
        assert!(report.samples[0]
            .missing_metadata
            .iter()
            .any(|value| value == "source_snippet_unavailable"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sample_paths_parses_bounded_options() {
        let options = parse_sample_paths_options(&[
            "--db".to_string(),
            "fixture.sqlite".to_string(),
            "--limit".to_string(),
            "7".to_string(),
            "--seed".to_string(),
            "42".to_string(),
            "--max-edge-load".to_string(),
            "99".to_string(),
            "--timeout-ms".to_string(),
            "1234".to_string(),
            "--mode".to_string(),
            "debug".to_string(),
            "--include-snippets".to_string(),
        ])
        .expect("parse options");

        assert_eq!(options.limit, 7);
        assert_eq!(options.seed, 42);
        assert_eq!(options.max_edge_load, 99);
        assert_eq!(options.timeout_ms, 1234);
        assert_eq!(options.mode, PathSampleMode::Debug);
        assert!(options.include_snippets);
    }

    #[test]
    fn sample_paths_is_deterministic_for_seed_and_respects_limit() {
        let root = temp_audit_dir("path-sample-deterministic");
        let db = root.join("codegraph.sqlite");
        create_path_evidence_fixture_db(&db);
        let mut options = sample_paths_test_options(db);
        options.seed = 42;
        options.limit = 1;

        let first = sample_paths(&options).expect("first sample");
        let second = sample_paths(&options).expect("second sample");

        assert_eq!(first.samples.len(), 1);
        assert_eq!(second.samples.len(), 1);
        assert_eq!(first.samples[0].path_id, second.samples[0].path_id);
        assert_eq!(first.generated_path_count, 0);
        assert!(!first.generated_fallback_used);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sample_paths_read_only_does_not_mutate_main_db_or_create_sidecars() {
        let root = temp_audit_dir("path-sample-read-only");
        let db = root.join("codegraph.sqlite");
        create_path_evidence_fixture_db(&db);
        remove_test_sidecars(&db);
        let before = db_file_snapshot(&db);
        assert!(before.sidecars.is_empty());
        let mut options = sample_paths_test_options(db.clone());
        options.limit = 1;

        let report = sample_paths(&options).expect("sample paths");
        let after = db_file_snapshot(&db);

        assert_eq!(report.samples.len(), 1);
        assert_eq!(before.main_db_size, after.main_db_size);
        assert_eq!(before.main_db_mtime_unix_ms, after.main_db_mtime_unix_ms);
        assert_eq!(before.main_db_hash, after.main_db_hash);
        assert!(after.sidecars.is_empty());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sample_paths_proof_mode_does_not_generate_fallback_paths() {
        let root = temp_audit_dir("path-sample-proof-no-fallback");
        let db = root.join("codegraph.sqlite");
        create_storage_experiment_fixture_db(&db);
        let options = sample_paths_test_options(db);

        let report = sample_paths(&options).expect("sample paths");

        assert_eq!(report.samples.len(), 0);
        assert_eq!(report.generated_path_count, 0);
        assert!(!report.fallback_allowed);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sample_paths_outputs_materialized_metadata_and_query_plans() {
        let root = temp_audit_dir("path-sample-json");
        let db = root.join("codegraph.sqlite");
        create_path_evidence_fixture_db(&db);
        let options = sample_paths_test_options(db);

        let report = sample_paths(&options).expect("sample paths");
        let value = serde_json::to_value(&report).expect("valid JSON");

        assert_eq!(value["schema_version"].as_u64(), Some(1));
        assert_eq!(value["mode"].as_str(), Some("proof"));
        assert!(value["explain_query_plan"].as_array().expect("plans").len() >= 2);
        assert!(value["index_status"].as_array().expect("indexes").len() >= 5);
        assert_eq!(report.samples.len(), 1);
        assert_eq!(report.samples[0].source_spans.len(), 1);
        assert_eq!(report.samples[0].edge_list.len(), 1);
        assert_eq!(
            report.samples[0].edge_list[0].exactness.as_deref(),
            Some("parser_verified")
        );
        assert!(!report.path_evidence_truncated);
        assert_eq!(report.hydration_budget_exhausted, false);
        assert_eq!(report.source_snippet_omitted_count, 1);
        assert!(report
            .index_status
            .iter()
            .any(|index| index.object == "path_evidence_edges"
                && index.satisfied_by.as_deref()
                    == Some("PRIMARY KEY(path_id, ordinal) WITHOUT ROWID")));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sample_paths_limit_100_and_timeout_are_bounded_diagnostics() {
        let root = temp_audit_dir("path-sample-timeout");
        let db = root.join("codegraph.sqlite");
        create_path_evidence_fixture_db(&db);

        let mut options = sample_paths_test_options(db.clone());
        options.limit = 100;
        options.max_edge_load = 100;
        let hundred = sample_paths(&options).expect("sample 100");
        assert!(hundred.samples.len() <= 100);
        assert_eq!(hundred.limit, 100);
        assert_eq!(hundred.max_edge_load, 100);
        assert!(!hundred.timeout_ms_exhausted);

        let mut timeout_options = sample_paths_test_options(db);
        timeout_options.timeout_ms = 1;
        timeout_options.limit = 100;
        timeout_options.max_edge_load = 100;
        let timeout = sample_paths(&timeout_options).expect("partial timeout report");
        if timeout.timeout_ms_exhausted {
            assert!(timeout.partial_diagnostic);
            assert!(timeout.path_evidence_truncated);
            assert!(timeout.hydration_budget_exhausted);
            assert!(timeout
                .stop_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("timed out")));
        } else {
            assert!(timeout.samples.len() <= 100);
        }

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sample_paths_missing_snippet_does_not_crash() {
        let root = temp_audit_dir("path-sample-missing-snippet");
        let db = root.join("codegraph.sqlite");
        create_path_evidence_fixture_db(&db);
        let mut options = sample_paths_test_options(db);
        options.include_snippets = true;

        let report = sample_paths(&options).expect("sample paths");

        assert_eq!(report.samples.len(), 1);
        assert!(report.samples[0].source_snippets.is_empty());
        assert!(report.samples[0]
            .missing_metadata
            .iter()
            .any(|value| value == "source_snippets_unavailable"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    fn test_labeled_sample(
        sample_type: &str,
        relation: &str,
        labels: ManualLabelSet,
    ) -> LabeledSample {
        LabeledSample {
            sample_type: sample_type.to_string(),
            source_json: "sample.json".to_string(),
            source_markdown: Some("sample.md".to_string()),
            ordinal: 1,
            sample_id: format!("{sample_type}:{relation}"),
            relation: relation.to_string(),
            relation_sequence: vec![relation.to_string()],
            edge_ids: vec!["edge://sample".to_string()],
            exactness: Some("parser_verified".to_string()),
            confidence: Some(1.0),
            source_span_count: 1,
            span_loaded: Some(true),
            fact_classification: Some("base_exact".to_string()),
            production_test_mock_context: Some("production_inferred".to_string()),
            labeled: labels.has_any_signal(),
            labels,
        }
    }

    fn create_storage_experiment_fixture_db(path: &Path) {
        let store = SqliteGraphStore::open(path).expect("open store");
        let span = SourceSpan::with_columns("src/auth.ts", 1, 1, 1, 20);
        store
            .upsert_file(&FileRecord {
                repo_relative_path: "src/auth.ts".to_string(),
                file_hash: "hash".to_string(),
                language: Some("typescript".to_string()),
                size_bytes: 20,
                indexed_at_unix_ms: Some(1),
                metadata: Default::default(),
            })
            .expect("file");
        let head = Entity {
            id: "repo://fixture#login".to_string(),
            kind: EntityKind::Function,
            name: "login".to_string(),
            qualified_name: "src.auth.login".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(span.clone()),
            content_hash: None,
            file_hash: Some("hash".to_string()),
            created_from: "test".to_string(),
            confidence: 1.0,
            metadata: Default::default(),
        };
        let tail = Entity {
            id: "repo://fixture#saveUser".to_string(),
            kind: EntityKind::Function,
            name: "saveUser".to_string(),
            qualified_name: "src.auth.saveUser".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(span.clone()),
            content_hash: None,
            file_hash: Some("hash".to_string()),
            created_from: "test".to_string(),
            confidence: 1.0,
            metadata: Default::default(),
        };
        store.upsert_entity(&head).expect("head");
        store.upsert_entity(&tail).expect("tail");
        store
            .upsert_edge(&Edge {
                id: "edge://fixture-login-calls-saveUser".to_string(),
                head_id: head.id,
                relation: RelationKind::Calls,
                tail_id: tail.id,
                source_span: span,
                repo_commit: None,
                file_hash: Some("hash".to_string()),
                extractor: "test".to_string(),
                confidence: 1.0,
                exactness: Exactness::ParserVerified,
                edge_class: EdgeClass::BaseExact,
                context: EdgeContext::Production,
                derived: false,
                provenance_edges: Vec::new(),
                metadata: Default::default(),
            })
            .expect("edge");
    }

    fn remove_test_sidecars(db: &Path) {
        for suffix in ["wal", "shm"] {
            let _ = fs::remove_file(PathBuf::from(format!("{}-{suffix}", db.display())));
        }
    }

    fn create_path_evidence_fixture_db(path: &Path) {
        create_storage_experiment_fixture_db(path);
        let store = SqliteGraphStore::open(path).expect("open store");
        let span = SourceSpan::with_columns("src/auth.ts", 1, 1, 1, 20);
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "task_or_query".to_string(),
            json!("audit path sample fixture"),
        );
        metadata.insert(
            "ordered_edge_ids".to_string(),
            json!(["edge://fixture-login-calls-saveUser"]),
        );
        metadata.insert(
            "edge_labels".to_string(),
            json!([
                {
                    "edge_id": "edge://fixture-login-calls-saveUser",
                    "relation": "CALLS",
                    "exactness": "parser_verified",
                    "confidence": 1.0,
                    "extractor": "test",
                    "edge_class": "base_exact",
                    "context": "production_inferred",
                    "derived": false,
                    "provenance_edges": [],
                    "source_span": "src/auth.ts:1:1-1:20"
                }
            ]),
        );
        store
            .upsert_path_evidence(&PathEvidence {
                id: "path://fixture/login-calls-save-user".to_string(),
                summary: Some("login calls saveUser".to_string()),
                source: "repo://fixture#login".to_string(),
                target: "repo://fixture#saveUser".to_string(),
                metapath: vec![RelationKind::Calls],
                edges: vec![(
                    "repo://fixture#login".to_string(),
                    RelationKind::Calls,
                    "repo://fixture#saveUser".to_string(),
                )],
                source_spans: vec![span],
                exactness: Exactness::ParserVerified,
                length: 1,
                confidence: 1.0,
                metadata,
            })
            .expect("path evidence");
    }

    fn sample_edges_test_options(db_path: PathBuf) -> SampleEdgesOptions {
        SampleEdgesOptions {
            db_path,
            relation: None,
            limit: 10,
            seed: 1,
            json_path: None,
            markdown_path: None,
            include_snippets: false,
        }
    }

    fn sample_paths_test_options(db_path: PathBuf) -> SamplePathsOptions {
        SamplePathsOptions {
            db_path,
            limit: 20,
            seed: 1,
            json_path: None,
            markdown_path: None,
            include_snippets: false,
            max_edge_load: 64,
            timeout_ms: 10_000,
            mode: PathSampleMode::Proof,
        }
    }

    fn vector_test_options(artifact_path: Option<PathBuf>) -> VectorChunksOptions {
        VectorChunksOptions {
            artifact_path,
            runtime_sidecar_path: None,
            audit_artifact_path: None,
            db_path: None,
            repo: None,
            json_path: None,
            markdown_path: None,
            sample_limit: 2,
        }
    }

    fn legacy_vector_fixture(text: &str) -> Value {
        json!({
            "metadata": vector_fixture_metadata(None, "pretty_json"),
            "chunks": vector_fixture_chunks(text),
        })
    }

    fn runtime_vector_fixture(text: &str) -> Value {
        json!({
            "metadata": vector_fixture_metadata(Some("vector_runtime_sidecar"), "compact_json"),
            "chunks": vector_fixture_chunks(text),
        })
    }

    fn audit_vector_fixture(text: &str) -> Value {
        let mut metadata = vector_fixture_metadata(Some("audit_artifact"), "pretty_json");
        metadata["diagnostic_only"] = json!(true);
        metadata["runtime_total_chunks"] = json!(0);
        metadata["audit_total_chunks"] = json!(2);
        json!({
            "metadata": metadata,
            "chunks": vector_fixture_chunks(text),
        })
    }

    fn vector_fixture_metadata(artifact_kind: Option<&str>, format: &str) -> Value {
        let mut metadata = json!({
            "metadata_version": "vector_chunk_index_metadata_v1",
            "provider": {
                "provider_id": "codegraph-deterministic-test",
                "model_id": "codegraph-deterministic-token-projection-v1",
                "dimension": 64,
                "normalization": "l2",
                "provider_version": "deterministic-test-embedding-v1",
                "privacy_mode": "local_only"
            },
            "passport": {
                "passport_fingerprint": "fnv64:test-passport",
                "passport_version": 1,
                "codegraph_schema_version": 21,
                "storage_mode": "proof",
                "index_scope_policy_hash": "scope-test",
                "canonical_repo_root": "fixture/repo",
                "repo_head": "fixture-head"
            },
            "source_scope": "context-pack-release-vector-candidates",
            "extraction_version": "vector_embedding_chunk_v1",
            "max_chunks": 8,
            "chunk_count": 2,
            "generated_total_chunks": 3,
            "spooled_total_chunks": 2,
            "selected_total_chunks": 2,
            "persisted_total_chunks": 2,
            "runtime_total_chunks": 2,
            "audit_total_chunks": 0,
            "omitted_by_cap": 1,
            "omitted_low_signal": 0,
            "omitted_by_bucket_limit": 0,
            "omitted_by_dedup": 0,
            "indexed_text_bytes": 1,
            "estimated_f32_payload_bytes": 512,
            "index_artifact_format": format,
            "stores_chunk_text": true,
            "stores_chunk_metadata": true,
            "stores_full_source_body": false,
            "vector_payload_compression": "none",
        });
        if let Some(kind) = artifact_kind {
            metadata["artifact_kind"] = json!(kind);
        }
        metadata
    }

    fn vector_fixture_chunks(text: &str) -> Vec<Value> {
        vec![
            json!({
                "chunk_id": "chunk://fixture/src/lib.rs#1",
                "chunk_kind": "function",
                "source_kind": "graph_entity",
                "file_id": "file://src/lib.rs",
                "path": "src/lib.rs",
                "entity_id": "repo://e/abc",
                "source_span": {
                    "repo_relative_path": "src/lib.rs",
                    "start_line": 1,
                    "start_column": 1,
                    "end_line": 3,
                    "end_column": 1
                },
                "source_role": "implementation",
                "evidence_role": "production",
                "proof_status": "candidate_only",
                "graph_proof": false,
                "claimable_for_graph": false,
                "text": text,
                "token_count": 4,
                "byte_count": text.as_bytes().len(),
                "language": "rust",
                "file_kind": "source",
                "lifecycle_binding": {
                    "db_passport_fingerprint": "fnv64:test-passport",
                    "read_decision": "read_reuse",
                    "claimable": false
                },
                "content_hash": "fnv64:chunk",
                "extraction_version": "vector_embedding_chunk_v1",
                "selection_score": 512.0,
                "selection_bucket": "graph_entity",
                "selection_reason": "source_kind=graph_entity; score=512.000",
                "top_level_dir": "src",
                "cap_stage": "selected"
            }),
            json!({
                "chunk_id": "chunk://fixture/README.md#path",
                "chunk_kind": "file_path_title",
                "source_kind": "metadata",
                "file_id": "file://README.md",
                "path": "README.md",
                "entity_id": Value::Null,
                "source_span": Value::Null,
                "source_role": "metadata",
                "evidence_role": "unknown",
                "proof_status": "not_graph_proof",
                "graph_proof": false,
                "claimable_for_graph": false,
                "text": "file path README.md title Fixture",
                "token_count": 5,
                "byte_count": 33,
                "language": Value::Null,
                "file_kind": "documentation",
                "lifecycle_binding": {
                    "db_passport_fingerprint": "fnv64:test-passport",
                    "read_decision": "read_reuse",
                    "claimable": false
                },
                "content_hash": "fnv64:path",
                "extraction_version": "vector_embedding_chunk_v1",
                "selection_score": 360.0,
                "selection_bucket": "metadata",
                "selection_reason": "source_kind=metadata; score=360.000",
                "top_level_dir": "(repo_root)",
                "cap_stage": "selected"
            }),
        ]
    }

    fn assert_vector_chunk_schema(report: &Value) {
        assert!(report["schema_version"].is_number());
        assert_eq!(report["audit"].as_str(), Some("vector_chunks"));
        assert!(report["artifact_identity"]["artifact_path"].is_string());
        assert!(report["artifact_identity"]["artifact_kind"].is_string());
        assert!(report["chunk_counts"]["generated_total_chunks"].is_number());
        assert!(report["chunk_composition"]["by_source_kind"].is_object());
        assert!(report["byte_accounting"]["actual_index_file_bytes"].is_number());
        assert!(report["passport_lifecycle_provider"]["validity_status"].is_string());
        assert!(report["safety_conclusions"]["candidate_only"].is_boolean());
        assert!(report["sample_chunks"].is_array());
        if let Some(sample) = report["sample_chunks"]
            .as_array()
            .and_then(|items| items.first())
        {
            for field in [
                "chunk_id",
                "chunk_kind",
                "source_kind",
                "path",
                "proof_status",
                "text_preview",
                "text_bytes",
                "text_preview_truncated",
            ] {
                assert!(sample.get(field).is_some(), "missing sample field {field}");
            }
            assert!(sample.get("text").is_none());
        }
    }

    fn temp_audit_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codegraph-audit-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp audit dir");
        path
    }
}
