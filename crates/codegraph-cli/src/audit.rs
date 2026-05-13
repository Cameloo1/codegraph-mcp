use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const AUDIT_SCHEMA_VERSION: u32 = 1;
const LABEL_SCHEMA_VERSION: u32 = 1;
const DEFAULT_SAMPLE_LIMIT: usize = 100;
const DEFAULT_PATH_SAMPLE_LIMIT: usize = 20;
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
            "Usage: codegraph-mcp audit <storage|schema-check|storage-experiments|sample-edges|sample-paths|relation-counts|label-samples|summarize-labels> [ARGS]".to_string(),
        );
    };

    match subcommand {
        "storage" | "storage-forensics" => run_storage_command(&args[1..]),
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
    let connection = open_read_only(db_path)?;
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
    let connection = open_read_only(db_path)?;
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

    Ok(SchemaValidationReport {
        schema_version: AUDIT_SCHEMA_VERSION,
        db_path: path_string(db_path),
        status: if failures.is_empty() {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
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
    enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "open_db")?;

    let started = Instant::now();
    let repo_roots = repo_roots(&connection)?;
    timing.repo_roots_ms = elapsed_ms(started);
    enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "repo_roots")?;

    let started = Instant::now();
    let stored_path_count = row_count(&connection, "path_evidence").unwrap_or(0);
    timing.count_ms = elapsed_ms(started);
    enforce_path_sample_deadline(total_start, deadline, options.timeout_ms, "row_count")?;

    let started = Instant::now();
    let mut explain_plans = path_sampler_query_plans(&connection, options)?;
    timing.explain_ms = elapsed_ms(started);

    let started = Instant::now();
    let index_status = path_evidence_index_status(&connection)?;
    timing.index_check_ms = elapsed_ms(started);

    let stored_result = if stored_path_count > 0 {
        stored_path_samples(
            &connection,
            &repo_roots,
            options,
            total_start,
            deadline,
            &mut timing,
        )?
    } else {
        StoredPathSampleResult::default()
    };
    let mut samples = stored_result.samples;
    let stored_samples = samples.len();
    let fallback_allowed = options.mode.allows_generated_fallback();
    if fallback_allowed && samples.len() < options.limit {
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
        notes: vec![
            "Classification fields are intentionally blank in markdown for human review."
                .to_string(),
            "PathEvidence sampling is bounded: candidate path IDs are selected first, details are batch-loaded only for those IDs, and snippets are loaded only for sampled spans when requested.".to_string(),
            format!(
                "Mode `{}` {} generated fallback paths.",
                options.mode.as_str(),
                if fallback_allowed { "allows" } else { "disables" }
            ),
            format!(
                "Path edge materialization load cap: {} rows; truncated: {}.",
                options.max_edge_load, stored_result.edge_load_truncated
            ),
            format!("Stored PathEvidence samples used before fallback: {stored_samples}."),
        ],
    })
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
        let present = sqlite_master_exists(connection, "index", index_name).unwrap_or(false)
            || sqlite_master_exists(connection, "table", object).unwrap_or(false);
        let columns = if sqlite_master_exists(connection, "index", index_name).unwrap_or(false) {
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
            satisfied_by: if present {
                Some(index_name.to_string())
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
    if !db_path.exists() {
        return Err(format!("database does not exist: {}", db_path.display()));
    }
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("failed to open {} read-only: {error}", db_path.display()))
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
        "- DBSTAT available: `{}`\n- Database bytes: `{}`\n- WAL bytes: `{}`\n- SHM bytes: `{}`\n- File family bytes: `{}`\n- Page size: `{}`\n- Page count: `{}`\n- Freelist count: `{}`\n\n",
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
    output.push_str(&format!("- Failure count: `{}`\n\n", report.failures.len()));

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
        "Database: `{}`\n\nMode: `{}`\n\nLimit: `{}`\n\nSeed: `{}`\n\nMax edge load: `{}`\n\nTimeout ms: `{}`\n\nStored PathEvidence rows: `{}`\n\nCandidate path IDs: `{}`\n\nLoaded materialized path edges: `{}`\n\nEdge load truncated: `{}`\n\nGenerated fallback samples: `{}`\n\n",
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

        let report = inspect_storage(&db).expect("storage inspection");
        let value = serde_json::to_value(&report).expect("storage JSON");

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
