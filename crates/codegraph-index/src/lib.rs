//! Shared compact repository indexer for CodeGraph.
//!
//! This crate is the single indexing implementation used by both the CLI and
//! MCP server. It preserves the compact path: streaming batches, duplicate
//! source-content dedupe, source-only ignores, file-only duplicate records, and
//! no default full source/snippet/source-span FTS writes.

#![forbid(unsafe_code)]
#![recursion_limit = "256"]

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    str::FromStr,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use codegraph_core::{
    classify_entity_source_role, normalize_repo_relative_path as normalize_graph_path,
    stable_edge_id, stable_entity_id_for_kind, Edge, EdgeClass, EdgeContext, Entity, EntityKind,
    EvidenceRole, Exactness, FileRecord, Metadata, PathEvidence, RelationKind, RepoIndexState,
    RetrievalCandidate, RetrievalCandidateLifecycleBinding, RetrievalCandidateLifecycleStatus,
    RetrievalCandidateSource, RetrievalProofStatus, RetrievalVerificationStatus, SourceSpan,
    VectorEmbeddingSource,
};
use codegraph_parser::{
    content_hash, detect_language, extract_entities_and_relations, BasicExtraction, LanguageParser,
    TreeSitterParser,
};
use codegraph_query::{
    is_proof_path_relation, ExactGraphQueryEngine, GraphPath, TraversalDirection, TraversalStep,
};
use codegraph_store::{
    classify_sqlite_access_problem, inspect_db_preflight, DbPassport, DbPreflightReport,
    ExpectedDbPassport, GraphStore, SqliteGraphStore, StoreError, DB_PASSPORT_VERSION,
    SCHEMA_VERSION,
};
use codegraph_store::{reset_sqlite_profile, take_sqlite_profile};
use codegraph_vector::{
    BinarySignature, BinaryVectorIndex, EmbeddingProvider, EmbeddingProviderMetadata,
    InMemoryBinaryVectorIndex, TestEmbeddingVector,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub mod scope;
use scope::{IndexScope, IndexScopeRuntimeReport, ScopeAction, ScopePathKind};
pub use scope::{
    IndexScopeOptions, INCLUDE_SEMANTICS_DEFAULT_SCOPE_PLUS_OVERRIDES,
    SCOPE_POLICY_KIND_DEFAULT_WITH_OVERRIDES, SCOPE_TRUTH_STATUS_OVERRIDE_ONLY,
};

pub const UNBOUNDED_STORE_READ_LIMIT: usize = 1_000_000;
const DERIVED_MUTATION_CLOSURE_MAX_OUTPUT_EDGES: usize = 100_000;
const DERIVED_MUTATION_CLOSURE_MAX_WRITES_PER_CALLEE: usize = 64;
const DERIVED_DATAFLOW_CLOSURE_MAX_OUTPUT_EDGES: usize = 100_000;
const DERIVED_DATAFLOW_CLOSURE_MAX_HOPS_PER_NODE: usize = 64;
pub const DEFAULT_INDEX_BATCH_MAX_FILES: usize = 128;
pub const DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
pub const DEFAULT_GRAPH_OUTPUT_MAX_LOCAL_FACTS_PER_FILE: usize = 20_000;
pub const DEFAULT_GRAPH_OUTPUT_MAX_RELATION_FANOUT_PER_FILE: usize = 4_096;
pub const DEFAULT_GRAPH_OUTPUT_MAX_DERIVED_EDGES_PER_FILE: usize = 20_000;
pub const DEFAULT_GRAPH_OUTPUT_MAX_SOURCE_SPANS_PER_FILE: usize = 20_000;
pub const DEFAULT_GRAPH_OUTPUT_MAX_REDUCER_EDGES_PER_STAGE: usize = 100_000;
const GRAPH_OUTPUT_BUDGET_REPORTED_HIT_LIMIT: usize = 16;
pub const DEFAULT_STORAGE_POLICY: &str = "proof:compact-proof-graph";
pub const FILE_LIFECYCLE_STATE_KEY: &str = "file_lifecycle_state";
pub const FILE_LIFECYCLE_STATE_CURRENT: &str = "current";
pub const FILE_LIFECYCLE_POLICY_KEY: &str = "file_lifecycle_policy";
pub const FILE_LIFECYCLE_POLICY_CURRENT_ONLY: &str = "current_only";
pub const FILE_HISTORICAL_VISIBILITY_KEY: &str = "historical_visibility";
pub const FILE_HISTORICAL_VISIBILITY_HIDDEN: &str = "not_visible_as_current";
pub const FILE_STALE_CLEANUP_KEY: &str = "stale_cleanup";
pub const FILE_STALE_CLEANUP_DELETE_BEFORE_INSERT: &str = "delete_before_insert";
pub const DEFAULT_STORED_PATH_EVIDENCE_MAX_ROWS: usize = 4_096;
const DEFAULT_STORED_PATH_EVIDENCE_SCAN_MULTIPLIER: usize = 16;
const TEXT_EVIDENCE_MAX_READ_BYTES_PER_FILE: u64 = 1024 * 1024;
const TEXT_EVIDENCE_MAX_FTS_BYTES_PER_FILE: usize = 64 * 1024;
const TEXT_EVIDENCE_MAX_SNIPPETS_PER_FILE: usize = 24;
const TEXT_EVIDENCE_MAX_SNIPPET_BYTES: usize = 512;
const TEXT_EVIDENCE_MAX_TOKENS_PER_FILE: usize = 96;
const TEXT_EVIDENCE_KIND: &str = "text_evidence";
const TEXT_EVIDENCE_PROOF_STATUS: &str = "not_graph_proof";
const WRITE_PATH_CHAOS_FAILPOINT_ENV: &str = "CODEGRAPH_WRITE_PATH_FAILPOINT";
pub const VECTOR_EMBEDDING_CHUNK_EXTRACTION_VERSION: &str = "vector_embedding_chunk_v1";
pub const VECTOR_EMBEDDING_CHUNK_MAX_TEXT_BYTES: usize = 1024;
pub const VECTOR_CHUNK_INDEX_METADATA_VERSION: &str = "vector_chunk_index_metadata_v1";
pub const CANDIDATE_SPOOL_METADATA_VERSION: &str = "candidate_spool_v1";
pub const CANDIDATE_SPOOL_RECORD_MODEL_VERSION: &str = "candidate_spool_packet_v1";
pub const CANDIDATE_SPOOL_QUERY_INDEX_VERSION: &str = "candidate_spool_query_index_v1";
pub const VECTOR_CHUNK_INDEX_DEFAULT_MAX_CHUNKS: usize = 4_096;
pub const CANDIDATE_SPOOL_DEFAULT_MAX_BYTES: usize = 8 * 1024 * 1024;
pub const CANDIDATE_SPOOL_DEFAULT_MAX_RECORDS: usize = 12_000;
pub const CANDIDATE_SPOOL_DEFAULT_PER_FILE_MAX_RECORDS: usize = 12;
pub const CANDIDATE_SPOOL_DEFAULT_PER_DIR_SOFT_CAP: usize = 1_500;
pub const CANDIDATE_SPOOL_DEFAULT_MAX_SNIPPET_BYTES: usize = 384;
pub const CANDIDATE_SPOOL_DEFAULT_MAX_SNIPPETS_PER_FILE_PACKET: usize = 3;
pub const CANDIDATE_SPOOL_DEFAULT_MAX_SYMBOLS_PER_FILE_PACKET: usize = 8;
const CANDIDATE_SPOOL_QUERY_INDEX_SAFE_REBUILD_MAX_BYTES: u64 = 16 * 1024 * 1024;
const CANDIDATE_SPOOL_QUERY_INDEX_SAFE_REBUILD_MAX_RECORDS: usize =
    CANDIDATE_SPOOL_DEFAULT_MAX_RECORDS;
const RTDS_CLOSURE_FAILPOINT_ENV: &str = "CODEGRAPH_RTDS_CLOSURE_FAILPOINT";
const DEFAULT_RTDS_CLOSURE_MAX_DIRTY_FILES: usize = 64;
const DEFAULT_RTDS_CLOSURE_MAX_EDGES_INSPECTED: usize = 4_096;
const DEFAULT_RTDS_CLOSURE_MAX_RELATION_CLASSES: usize = 6;
const DEFAULT_RTDS_CLOSURE_MAX_WALL_MS: u64 = 250;
const DEFAULT_RTDS_CLOSURE_MAX_SOURCE_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_RTDS_CLOSURE_MAX_DB_ROWS_HYDRATED: usize = 8_192;
const DEFAULT_RTDS_CLOSURE_MAX_PER_RELATION: usize = 1_024;

thread_local! {
    static WRITE_PATH_CHAOS_FAILPOINT_OVERRIDE: RefCell<Option<String>> = RefCell::new(None);
    static RTDS_CLOSURE_FAILPOINT_OVERRIDE: RefCell<Option<String>> = RefCell::new(None);
    static RTDS_CLOSURE_BUDGET_OVERRIDE: RefCell<Option<RtdsDependencyClosureBudget>> = RefCell::new(None);
}

#[derive(Debug)]
pub enum IndexError {
    RepoNotFound(PathBuf),
    Io(std::io::Error),
    Store(codegraph_store::StoreError),
    Parse(codegraph_parser::ParseError),
    PathStrip { path: PathBuf, root: PathBuf },
    Message(String),
}

impl fmt::Display for IndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RepoNotFound(path) => write!(
                formatter,
                "repository path does not exist: {}",
                path.display()
            ),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Store(error) => write!(formatter, "store error: {error}"),
            Self::Parse(error) => write!(formatter, "parse error: {error}"),
            Self::PathStrip { path, root } => write!(
                formatter,
                "could not make {} relative to {}",
                path.display(),
                root.display()
            ),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl Error for IndexError {}

impl From<std::io::Error> for IndexError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<codegraph_store::StoreError> for IndexError {
    fn from(error: codegraph_store::StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<codegraph_parser::ParseError> for IndexError {
    fn from(error: codegraph_parser::ParseError) -> Self {
        Self::Parse(error)
    }
}

fn write_path_chaos_failpoint_matches(raw: &str, name: &str) -> bool {
    raw.split(',')
        .map(str::trim)
        .any(|candidate| candidate == name)
}

fn write_path_chaos_failpoint_enabled(name: &str) -> bool {
    if WRITE_PATH_CHAOS_FAILPOINT_OVERRIDE.with(|override_cell| {
        override_cell
            .borrow()
            .as_deref()
            .is_some_and(|raw| write_path_chaos_failpoint_matches(raw, name))
    }) {
        return true;
    }
    std::env::var(WRITE_PATH_CHAOS_FAILPOINT_ENV)
        .ok()
        .is_some_and(|raw| write_path_chaos_failpoint_matches(&raw, name))
}

fn write_path_chaos_failpoint(name: &str) -> Result<(), IndexError> {
    if write_path_chaos_failpoint_enabled(name) {
        Err(IndexError::Message(format!("chaos_failpoint:{name}")))
    } else {
        Ok(())
    }
}

fn write_path_chaos_store_failpoint(name: &str) -> Result<(), StoreError> {
    if write_path_chaos_failpoint_enabled(name) {
        Err(StoreError::Message(format!("chaos_failpoint:{name}")))
    } else {
        Ok(())
    }
}

fn env_usize_or(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_u64_or(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn rtds_closure_failpoint_enabled(name: &str) -> bool {
    if RTDS_CLOSURE_FAILPOINT_OVERRIDE.with(|override_cell| {
        override_cell
            .borrow()
            .as_deref()
            .is_some_and(|raw| raw == name)
    }) {
        return true;
    }
    std::env::var(RTDS_CLOSURE_FAILPOINT_ENV).ok().as_deref() == Some(name)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IndexSummary {
    pub repo_root: String,
    pub db_path: String,
    pub db_lifecycle: Option<DbLifecycleEvidence>,
    pub build_mode: String,
    pub files_seen: usize,
    pub files_walked: usize,
    pub files_metadata_unchanged: usize,
    pub files_read: usize,
    pub files_hashed: usize,
    pub files_parsed: usize,
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub files_deleted: usize,
    pub files_renamed: usize,
    pub parse_errors: usize,
    pub syntax_errors: usize,
    pub entities: usize,
    pub edges: usize,
    pub duplicate_edges_upserted: usize,
    pub batches_total: usize,
    pub batches_completed: usize,
    pub batch_max_files: usize,
    pub batch_max_source_bytes: usize,
    pub stale_files_deleted: usize,
    pub failed_files_deleted: usize,
    pub storage_policy: String,
    pub issue_counts: BTreeMap<String, usize>,
    pub issues: Vec<IndexIssue>,
    pub graph_output_budgets: GraphOutputBudgetSummary,
    pub scope: Option<IndexScopeRuntimeReport>,
    pub candidate_spool: Option<CandidateSpoolSummary>,
    pub profile: Option<IndexProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphOutputBudgets {
    pub max_local_facts_per_file: usize,
    pub max_relation_fanout_per_file: usize,
    pub max_derived_edges_per_file: usize,
    pub max_source_spans_per_file: usize,
    pub max_reducer_edges_per_stage: usize,
}

impl Default for GraphOutputBudgets {
    fn default() -> Self {
        Self {
            max_local_facts_per_file: DEFAULT_GRAPH_OUTPUT_MAX_LOCAL_FACTS_PER_FILE,
            max_relation_fanout_per_file: DEFAULT_GRAPH_OUTPUT_MAX_RELATION_FANOUT_PER_FILE,
            max_derived_edges_per_file: DEFAULT_GRAPH_OUTPUT_MAX_DERIVED_EDGES_PER_FILE,
            max_source_spans_per_file: DEFAULT_GRAPH_OUTPUT_MAX_SOURCE_SPANS_PER_FILE,
            max_reducer_edges_per_stage: DEFAULT_GRAPH_OUTPUT_MAX_REDUCER_EDGES_PER_STAGE,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphOutputBudgetHit {
    pub repo_relative_path: String,
    pub stage: String,
    pub kind: String,
    pub before: usize,
    pub after: usize,
    pub budget: usize,
    pub omitted: usize,
    pub claimability_label: String,
}

impl GraphOutputBudgetHit {
    fn message(&self) -> String {
        format!(
            "{} hit {} budget at stage {}; omitted {} fact(s), before={}, after={}, budget={}",
            self.repo_relative_path,
            self.kind,
            self.stage,
            self.omitted,
            self.before,
            self.after,
            self.budget
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphOutputBudgetSummary {
    pub configured: GraphOutputBudgets,
    pub source_batching_policy: String,
    pub worker_dispatch_source_clone_policy: String,
    pub files_degraded: usize,
    pub local_fact_budget_hits: usize,
    pub relation_fanout_budget_hits: usize,
    pub derived_edge_budget_hits: usize,
    pub source_span_budget_hits: usize,
    pub reducer_edge_budget_hits: usize,
    pub omitted_local_facts: usize,
    pub omitted_relation_fanout_edges: usize,
    pub omitted_derived_edges: usize,
    pub omitted_source_span_facts: usize,
    pub omitted_reducer_edges: usize,
    pub claimability_label: String,
    pub warnings: Vec<String>,
    pub degraded_files: Vec<GraphOutputBudgetHit>,
}

impl GraphOutputBudgetSummary {
    pub fn new(configured: GraphOutputBudgets) -> Self {
        Self {
            configured,
            source_batching_policy: format!(
                "bounded_batches:max_files={DEFAULT_INDEX_BATCH_MAX_FILES};max_source_bytes={DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES}"
            ),
            worker_dispatch_source_clone_policy:
                "pending source buffers are moved into worker chunks without cloning".to_string(),
            files_degraded: 0,
            local_fact_budget_hits: 0,
            relation_fanout_budget_hits: 0,
            derived_edge_budget_hits: 0,
            source_span_budget_hits: 0,
            reducer_edge_budget_hits: 0,
            omitted_local_facts: 0,
            omitted_relation_fanout_edges: 0,
            omitted_derived_edges: 0,
            omitted_source_span_facts: 0,
            omitted_reducer_edges: 0,
            claimability_label: "full_graph_output_with_no_budget_degradation".to_string(),
            warnings: Vec::new(),
            degraded_files: Vec::new(),
        }
    }

    fn record_hit(&mut self, hit: GraphOutputBudgetHit) {
        match hit.kind.as_str() {
            "local_facts_per_file" => {
                self.local_fact_budget_hits += 1;
                self.omitted_local_facts += hit.omitted;
            }
            "relation_fanout_per_file" => {
                self.relation_fanout_budget_hits += 1;
                self.omitted_relation_fanout_edges += hit.omitted;
            }
            "derived_edges_per_file" => {
                self.derived_edge_budget_hits += 1;
                self.omitted_derived_edges += hit.omitted;
            }
            "source_spans_per_file" => {
                self.source_span_budget_hits += 1;
                self.omitted_source_span_facts += hit.omitted;
            }
            "reducer_edges_per_stage" => {
                self.reducer_edge_budget_hits += 1;
                self.omitted_reducer_edges += hit.omitted;
            }
            _ => {}
        }
        if !self
            .degraded_files
            .iter()
            .any(|existing| existing.repo_relative_path == hit.repo_relative_path)
        {
            self.files_degraded += 1;
        }
        self.claimability_label =
            "graph_output_degraded_budget_hit_some_file_facts_nonclaimable".to_string();
        let warning = hit.message();
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
        if self.degraded_files.len() < GRAPH_OUTPUT_BUDGET_REPORTED_HIT_LIMIT {
            self.degraded_files.push(hit);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateSpoolSummary {
    pub status: String,
    pub candidate_spool_status: String,
    pub candidate_spool_path: String,
    pub artifact_kind: String,
    pub artifact_format: String,
    pub lifecycle: String,
    pub spooled_total_chunks: usize,
    pub spooled_by_source_kind: BTreeMap<String, usize>,
    pub spooled_by_chunk_kind: BTreeMap<String, usize>,
    pub spooled_bytes: u64,
    pub spooled_indexing_phase: String,
    #[serde(default)]
    pub candidate_spool_policy: String,
    #[serde(default)]
    pub candidate_spool_required: bool,
    #[serde(default)]
    pub candidate_spool_disabled_reason: Option<String>,
    #[serde(default)]
    pub candidate_spool_warning: Option<String>,
    #[serde(default)]
    pub artifact_budget_remaining_bytes: Option<u64>,
    #[serde(default)]
    pub artifact_budget_decision: Option<String>,
    #[serde(default)]
    pub record_model: String,
    #[serde(default)]
    pub generated_total_chunks: usize,
    #[serde(default)]
    pub normalized_total_chunks: usize,
    #[serde(default)]
    pub deduped_total_chunks: usize,
    #[serde(default)]
    pub selected_total_chunks: usize,
    #[serde(default)]
    pub persisted_total_chunks: usize,
    #[serde(default)]
    pub omitted_by_cap: usize,
    #[serde(default)]
    pub omitted_by_budget: usize,
    #[serde(default)]
    pub omitted_by_dedup: usize,
    #[serde(default)]
    pub omitted_low_signal: usize,
    #[serde(default)]
    pub omitted_by_file_limit: usize,
    #[serde(default)]
    pub omitted_by_dir_limit: usize,
    #[serde(default)]
    pub omitted_by_kind_limit: usize,
    #[serde(default)]
    pub candidate_spool_truncated: bool,
    #[serde(default)]
    pub candidate_spool_partial: bool,
    #[serde(default)]
    pub candidate_spool_budget_bytes: u64,
    #[serde(default)]
    pub query_index_status: String,
    #[serde(default)]
    pub query_index_kind: String,
    #[serde(default)]
    pub query_index_path: Option<String>,
    #[serde(default)]
    pub query_index_bytes: u64,
    #[serde(default)]
    pub query_index_record_count: usize,
    #[serde(default)]
    pub query_index_version: String,
    #[serde(default)]
    pub query_index_bound_manifest_hash: Option<String>,
    pub candidate_only: bool,
    pub graph_proof: bool,
    pub incomplete: bool,
    pub db_passport_hash: Option<String>,
    pub db_passport_snapshot: Option<DbPassport>,
    pub scope_hash: Option<String>,
    pub repo_hash: Option<String>,
    pub reason: Option<String>,
    #[serde(skip)]
    pub caps: CandidateSpoolCaps,
    #[serde(skip)]
    pub selected_candidate_ids: BTreeSet<String>,
    #[serde(skip)]
    pub selected_file_counts: BTreeMap<String, usize>,
    #[serde(skip)]
    pub selected_dir_counts: BTreeMap<String, usize>,
    #[serde(skip)]
    pub selected_kind_counts: BTreeMap<String, usize>,
    #[serde(skip)]
    pub selected_source_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateSpoolPolicy {
    Off,
    Bounded,
    Audit,
}

impl Default for CandidateSpoolPolicy {
    fn default() -> Self {
        Self::Bounded
    }
}

impl CandidateSpoolPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Bounded => "bounded",
            Self::Audit => "audit",
        }
    }
}

impl FromStr for CandidateSpoolPolicy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "disabled" => Ok(Self::Off),
            "bounded" | "default" => Ok(Self::Bounded),
            "audit" | "diagnostic" => Ok(Self::Audit),
            other => Err(format!(
                "invalid candidate spool policy {other}; expected off, bounded, or audit"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateSpoolCaps {
    pub global_max_bytes: usize,
    pub global_max_records: usize,
    pub per_file_max_records: usize,
    pub per_top_level_dir_soft_cap: usize,
    pub max_snippet_bytes: usize,
    pub max_snippets_per_file_packet: usize,
    pub max_symbols_per_file_packet: usize,
    pub per_candidate_kind_cap: BTreeMap<String, usize>,
    pub per_source_kind_cap: BTreeMap<String, usize>,
}

impl Default for CandidateSpoolCaps {
    fn default() -> Self {
        let mut per_candidate_kind_cap = BTreeMap::new();
        per_candidate_kind_cap.insert("file_path_title".to_string(), 4_096);
        per_candidate_kind_cap.insert("text_evidence_snippet".to_string(), 4_096);
        per_candidate_kind_cap.insert("symbol_signature".to_string(), 6_000);
        per_candidate_kind_cap.insert("import_export_source_navigation".to_string(), 3_000);
        per_candidate_kind_cap.insert("parser_local_reference".to_string(), 3_000);
        per_candidate_kind_cap.insert("relation_neighborhood_candidate".to_string(), 1_000);
        per_candidate_kind_cap.insert("doc_comment_or_title".to_string(), 3_000);
        per_candidate_kind_cap.insert("test_or_mock_candidate".to_string(), 1_500);
        per_candidate_kind_cap.insert("unknown".to_string(), 500);

        let mut per_source_kind_cap = BTreeMap::new();
        per_source_kind_cap.insert("metadata".to_string(), 3_000);
        per_source_kind_cap.insert("text_evidence".to_string(), 4_800);
        per_source_kind_cap.insert("graph_entity".to_string(), 6_000);
        per_source_kind_cap.insert("unknown".to_string(), 600);

        Self {
            global_max_bytes: CANDIDATE_SPOOL_DEFAULT_MAX_BYTES,
            global_max_records: CANDIDATE_SPOOL_DEFAULT_MAX_RECORDS,
            per_file_max_records: CANDIDATE_SPOOL_DEFAULT_PER_FILE_MAX_RECORDS,
            per_top_level_dir_soft_cap: CANDIDATE_SPOOL_DEFAULT_PER_DIR_SOFT_CAP,
            max_snippet_bytes: CANDIDATE_SPOOL_DEFAULT_MAX_SNIPPET_BYTES,
            max_snippets_per_file_packet: CANDIDATE_SPOOL_DEFAULT_MAX_SNIPPETS_PER_FILE_PACKET,
            max_symbols_per_file_packet: CANDIDATE_SPOOL_DEFAULT_MAX_SYMBOLS_PER_FILE_PACKET,
            per_candidate_kind_cap,
            per_source_kind_cap,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CandidateSpoolIndexLoad {
    pub path: PathBuf,
    pub query_index_path: PathBuf,
    pub metadata: Value,
    pub stale: bool,
    pub reason: Option<String>,
    pub query_index_status: String,
    pub query_index_kind: String,
    pub query_index_bytes: u64,
    pub query_index_record_count: usize,
    pub query_index_version: String,
    pub query_index_bound_manifest_hash: Option<String>,
    pub query_index_source_binding_count: usize,
}

#[derive(Debug, Clone)]
pub struct CandidateSpoolIndexQueryResult {
    pub load: CandidateSpoolIndexLoad,
    pub chunks: Vec<Value>,
    pub omitted_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DbLifecyclePolicy {
    SafeAuto,
    FreshRebuild,
    IncrementalRequired,
    FailOnDbProblem,
    DiagnosticStaleReuse,
}

impl Default for DbLifecyclePolicy {
    fn default() -> Self {
        Self::SafeAuto
    }
}

impl DbLifecyclePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SafeAuto => "safe-auto",
            Self::FreshRebuild => "fresh-rebuild",
            Self::IncrementalRequired => "incremental",
            Self::FailOnDbProblem => "fail-on-db-problem",
            Self::DiagnosticStaleReuse => "diagnostic-stale-reuse",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DbLifecycleOptions {
    pub policy: DbLifecyclePolicy,
    pub explicit_db_path: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DbLifecyclePreflight {
    pub safe: bool,
    pub db_problem_kind: Option<String>,
    pub path_access_status: String,
    pub path_access_error: Option<String>,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub exact_db_path_checked: String,
    pub repo_root_expected: String,
    pub db_path_outside_workspace: bool,
    pub outside_workspace_note: Option<String>,
    pub repo_root_status: String,
    pub schema_status: String,
    pub storage_mode_status: String,
    pub scope_status: String,
    pub scope_source: String,
    pub passport_scope_hash: Option<String>,
    pub explicit_scope_hash: Option<String>,
    pub scope_mismatch: Option<ScopeMismatchDetails>,
    pub passport_scope_policy: Option<IndexScopeOptions>,
    pub explicit_scope_policy: Option<IndexScopeOptions>,
    pub effective_scope_policy: Option<IndexScopeOptions>,
    pub db_health: DbPreflightReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DbLifecycleOperationKind {
    NormalRead,
    DiagnosticRead,
    WriteUpdate,
    ImportReplace,
    ImportMerge,
    BenchmarkInspection,
    BenchmarkSetup,
}

impl DbLifecycleOperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NormalRead => "normal_read",
            Self::DiagnosticRead => "diagnostic_read",
            Self::WriteUpdate => "write_update",
            Self::ImportReplace => "import_replace",
            Self::ImportMerge => "import_merge",
            Self::BenchmarkInspection => "benchmark_inspection",
            Self::BenchmarkSetup => "benchmark_setup",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbLifecycleSurfacePreflightRequest {
    pub repo_root: PathBuf,
    pub db_path: PathBuf,
    pub surface_name: String,
    pub operation_kind: DbLifecycleOperationKind,
    pub allow_stale_read: bool,
    pub allow_foreign_repo: bool,
    pub required_storage_mode: Option<StorageMode>,
    pub expected_scope: Option<IndexScopeOptions>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DbLifecycleSurfacePreflight {
    pub surface_name: String,
    pub operation_kind: DbLifecycleOperationKind,
    pub safe_to_read: bool,
    pub safe_to_write: bool,
    pub claimable: bool,
    pub diagnostic_only: bool,
    pub db_problem_kind: Option<String>,
    pub path_access_status: String,
    pub path_access_error: Option<String>,
    pub passport_status: String,
    pub repo_match: bool,
    pub scope_match: bool,
    pub schema_status: String,
    pub storage_mode_match: bool,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub exact_db_path_checked: String,
    pub repo_root_expected: String,
    pub db_path_outside_workspace: bool,
    pub outside_workspace_note: Option<String>,
    pub repo_root_observed: Option<String>,
    pub artifact_freshness: Option<String>,
    pub scope_source: String,
    pub passport_scope_hash: Option<String>,
    pub explicit_scope_hash: Option<String>,
    pub lifecycle_preflight: DbLifecyclePreflight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopeMismatchDetails {
    pub message: String,
    pub expected_scope_source: String,
    pub expected_scope_hash: Option<String>,
    pub observed_scope_source: String,
    pub observed_scope_hash: Option<String>,
    pub passport_scope_policy: Option<IndexScopeOptions>,
    pub explicit_scope_policy: Option<IndexScopeOptions>,
}

const EXTERNAL_PROFILE_DB_NOTE: &str =
    "This profile DB is outside the workspace; grant access or choose a workspace-local DB.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DbLifecycleEvidence {
    pub mode: String,
    pub decision: String,
    pub reasons: Vec<String>,
    pub passport_status: String,
    pub old_db_used: bool,
    pub old_db_replaced: bool,
    pub fresh_temp_db_path: Option<String>,
    pub claimable: bool,
    pub explicit_db_path: bool,
    pub preflight: Option<DbPreflightReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexIssue {
    pub repo_relative_path: String,
    pub kind: String,
    pub message: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IndexProfile {
    pub file_discovery_ms: u128,
    pub parse_ms: u128,
    pub extraction_ms: u128,
    pub semantic_resolver_ms: u128,
    pub db_write_ms: u128,
    pub fts_search_index_ms: u128,
    pub vector_signature_ms: u128,
    pub total_wall_ms: u128,
    pub files_per_sec: f64,
    pub entities_per_sec: f64,
    pub edges_per_sec: f64,
    pub memory_bytes: Option<u64>,
    pub memory_measured: bool,
    pub memory_status: String,
    pub memory_measurement_kind: String,
    pub db_write_measurement: String,
    pub fts_search_index_measurement: String,
    pub worker_count: usize,
    pub skipped_unchanged_files: usize,
    pub spans: Vec<PhaseTiming>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseTiming {
    pub name: String,
    pub elapsed_ms: f64,
    pub count: u64,
    pub items: u64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageMode {
    Proof,
    Audit,
    Debug,
}

impl Default for StorageMode {
    fn default() -> Self {
        Self::Proof
    }
}

impl StorageMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proof => "proof",
            Self::Audit => "audit",
            Self::Debug => "debug",
        }
    }

    pub fn storage_policy(self) -> &'static str {
        match self {
            Self::Proof => "proof:compact-proof-graph",
            Self::Audit => "audit:compact-proof-plus-diagnostic-sidecars",
            Self::Debug => "debug:proof-plus-full-debug-sidecars",
        }
    }

    fn preserves_heuristic_sidecars(self) -> bool {
        matches!(self, Self::Audit | Self::Debug)
    }
}

impl std::str::FromStr for StorageMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "proof" => Ok(Self::Proof),
            "audit" => Ok(Self::Audit),
            "debug" => Ok(Self::Debug),
            other => Err(format!(
                "invalid storage mode {other}; expected proof, audit, or debug"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexBuildMode {
    ProofBuildOnly,
    ProofBuildPlusValidation,
}

impl Default for IndexBuildMode {
    fn default() -> Self {
        Self::ProofBuildOnly
    }
}

impl IndexBuildMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProofBuildOnly => "proof-build-only",
            Self::ProofBuildPlusValidation => "proof-build-plus-validation",
        }
    }

    fn post_index_check(self) -> PostIndexCheck {
        match self {
            Self::ProofBuildOnly => PostIndexCheck::Quick,
            Self::ProofBuildPlusValidation => PostIndexCheck::Full,
        }
    }
}

impl std::str::FromStr for IndexBuildMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "proof-build-only" | "proof" | "fast" => Ok(Self::ProofBuildOnly),
            "proof-build-plus-validation" | "validation" | "validated" => {
                Ok(Self::ProofBuildPlusValidation)
            }
            other => Err(format!(
                "invalid build mode {other}; expected proof-build-only or proof-build-plus-validation"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexOptions {
    pub profile: bool,
    pub json: bool,
    pub worker_count: Option<usize>,
    pub storage_mode: StorageMode,
    pub build_mode: IndexBuildMode,
    pub scope: IndexScopeOptions,
    pub graph_output_budgets: GraphOutputBudgets,
    pub db_lifecycle: DbLifecycleOptions,
    pub candidate_spool_path: Option<PathBuf>,
    pub candidate_spool_policy: CandidateSpoolPolicy,
    pub candidate_spool_required: bool,
    pub candidate_spool_query_index: bool,
    pub candidate_spool_caps: CandidateSpoolCaps,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            profile: false,
            json: false,
            worker_count: None,
            storage_mode: StorageMode::default(),
            build_mode: IndexBuildMode::default(),
            scope: IndexScopeOptions::default(),
            graph_output_budgets: GraphOutputBudgets::default(),
            db_lifecycle: DbLifecycleOptions::default(),
            candidate_spool_path: None,
            candidate_spool_policy: CandidateSpoolPolicy::default(),
            candidate_spool_required: false,
            candidate_spool_query_index: true,
            candidate_spool_caps: CandidateSpoolCaps::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingIndexFile {
    pub repo_relative_path: String,
    pub source: String,
    pub file_hash: String,
    pub language: Option<String>,
    pub size_bytes: u64,
    pub modified_unix_nanos: Option<String>,
    pub needs_delete: bool,
    pub duplicate_of: Option<String>,
    pub template_required: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PendingIndexBatch {
    pub files: Vec<PendingIndexFile>,
    pub source_bytes: usize,
}

const RUBI_GRAPH_EXTRACTION_SKIP_BYTES: usize = 8 * 1024;
const LARGE_TEST_OR_GENERATED_GRAPH_EXTRACTION_SKIP_BYTES: usize = 64 * 1024;
const LARGE_GENERATED_OR_TEST_GRAPH_EXTRACTION_SKIP_BYTES: usize = 384 * 1024;

#[derive(Debug)]
struct HashedIndexCandidate {
    repo_relative_path: String,
    source: String,
    file_hash: String,
    language: Option<String>,
    size_bytes: u64,
    modified_unix_nanos: Option<String>,
    needs_delete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextEvidenceCandidate {
    file_path: PathBuf,
    repo_relative_path: String,
    kind: TextEvidenceFileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextEvidenceFileKind {
    MakefileFragment,
    Kconfig,
    Asciidoc,
    Markdown,
    ShellScript,
    PythonSupportScript,
    TextLikeSupportScript,
    BuildrootPackageMetadata,
    FixtureManifest,
}

impl TextEvidenceFileKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::MakefileFragment => "makefile_fragment",
            Self::Kconfig => "kconfig",
            Self::Asciidoc => "asciidoc",
            Self::Markdown => "markdown",
            Self::ShellScript => "shell_script",
            Self::PythonSupportScript => "python_support_script",
            Self::TextLikeSupportScript => "text_like_support_script",
            Self::BuildrootPackageMetadata => "buildroot_package_metadata",
            Self::FixtureManifest => "fixture_manifest",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::MakefileFragment => "Makefile fragment",
            Self::Kconfig => "Kconfig",
            Self::Asciidoc => "AsciiDoc",
            Self::Markdown => "Markdown",
            Self::ShellScript => "shell script",
            Self::PythonSupportScript => "Python support script",
            Self::TextLikeSupportScript => "text-like support script",
            Self::BuildrootPackageMetadata => "Buildroot package metadata",
            Self::FixtureManifest => "fixture manifest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextEvidenceSnippet {
    id: String,
    span: SourceSpan,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextEvidenceIndex {
    fts_body: String,
    snippets: Vec<TextEvidenceSnippet>,
    tokens: Vec<String>,
    indexed_bytes: usize,
    total_bytes: usize,
    omitted_bytes: usize,
    indexed_lines: usize,
    total_lines: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorEmbeddingChunkSourceKind {
    GraphEntity,
    TextEvidence,
    Metadata,
}

impl VectorEmbeddingChunkSourceKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::GraphEntity => "graph_entity",
            Self::TextEvidence => "text_evidence",
            Self::Metadata => "metadata",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorEmbeddingChunkKind {
    Function,
    Method,
    Type,
    ModuleFile,
    Signature,
    DocComment,
    SourceSnippet,
    Snippet,
    FilePathTitle,
    QName,
    RelationNeighborhood,
    SourceRole,
}

impl VectorEmbeddingChunkKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Type => "type",
            Self::ModuleFile => "module_file",
            Self::Signature => "signature",
            Self::DocComment => "doc_comment",
            Self::SourceSnippet => "source_snippet",
            Self::Snippet => "snippet",
            Self::FilePathTitle => "file_path_title",
            Self::QName => "qname",
            Self::RelationNeighborhood => "relation_neighborhood",
            Self::SourceRole => "source_role",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorEmbeddingChunk {
    pub chunk_id: String,
    pub chunk_kind: VectorEmbeddingChunkKind,
    pub source_kind: VectorEmbeddingChunkSourceKind,
    pub file_id: String,
    pub path: String,
    pub entity_id: Option<String>,
    pub source_span: Option<SourceSpan>,
    pub source_role: String,
    pub evidence_role: String,
    pub proof_status: String,
    pub graph_proof: bool,
    pub claimable_for_graph: bool,
    pub text: String,
    pub token_count: usize,
    pub byte_count: usize,
    pub language: Option<String>,
    pub file_kind: Option<String>,
    pub lifecycle_binding: Option<RetrievalCandidateLifecycleBinding>,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file_content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file_size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file_modified_unix_nanos: Option<String>,
    pub extraction_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_bucket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_level_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_stage: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorChunkIndexProviderSnapshot {
    pub provider_id: String,
    pub model_id: String,
    pub dimension: usize,
    pub normalization: String,
    pub provider_version: String,
    pub privacy_mode: String,
}

impl VectorChunkIndexProviderSnapshot {
    pub fn from_provider(metadata: &EmbeddingProviderMetadata) -> Self {
        Self {
            provider_id: metadata.provider_id.clone(),
            model_id: metadata.model_id.clone(),
            dimension: metadata.dimension,
            normalization: metadata.normalization.clone(),
            provider_version: metadata.version.clone(),
            privacy_mode: metadata.privacy_mode.as_str().to_string(),
        }
    }

    pub fn incompatibility_reason(&self, metadata: &EmbeddingProviderMetadata) -> Option<String> {
        if self.provider_id != metadata.provider_id {
            return Some("provider_id changed".to_string());
        }
        if self.model_id != metadata.model_id {
            return Some("model_id changed".to_string());
        }
        if self.dimension != metadata.dimension {
            return Some("dimension changed".to_string());
        }
        if self.normalization != metadata.normalization {
            return Some("normalization changed".to_string());
        }
        if self.provider_version != metadata.version {
            return Some("provider_version changed".to_string());
        }
        if self.privacy_mode != metadata.privacy_mode.as_str() {
            return Some("privacy_mode changed".to_string());
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorChunkIndexPassportSnapshot {
    pub passport_fingerprint: String,
    pub passport_version: u32,
    pub codegraph_schema_version: u32,
    pub storage_mode: String,
    pub index_scope_policy_hash: String,
    pub canonical_repo_root: String,
    #[serde(default)]
    pub repo_head: Option<String>,
}

impl VectorChunkIndexPassportSnapshot {
    pub fn from_passport(passport: &DbPassport) -> Self {
        Self {
            passport_fingerprint: db_passport_fingerprint(passport),
            passport_version: passport.passport_version,
            codegraph_schema_version: passport.codegraph_schema_version,
            storage_mode: passport.storage_mode.clone(),
            index_scope_policy_hash: passport.index_scope_policy_hash.clone(),
            canonical_repo_root: passport.canonical_repo_root.clone(),
            repo_head: passport.repo_head.clone(),
        }
    }

    pub fn incompatibility_reason(&self, passport: &DbPassport) -> Option<String> {
        if self.passport_version != passport.passport_version {
            return Some("db_passport_version changed".to_string());
        }
        if self.codegraph_schema_version != passport.codegraph_schema_version {
            return Some("codegraph_schema_version changed".to_string());
        }
        if self.storage_mode != passport.storage_mode {
            return Some("storage_mode changed".to_string());
        }
        if self.index_scope_policy_hash != passport.index_scope_policy_hash {
            return Some("index_scope_policy_hash changed".to_string());
        }
        if self.canonical_repo_root != passport.canonical_repo_root {
            return Some("canonical_repo_root changed".to_string());
        }
        if self.repo_head != passport.repo_head {
            return Some("repo_head changed".to_string());
        }
        if self.passport_fingerprint != db_passport_fingerprint(passport) {
            return Some("db_passport changed".to_string());
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorChunkIndexBuildOptions {
    pub max_chunks: usize,
    pub source_scope: String,
    pub extraction_version: String,
}

impl VectorChunkIndexBuildOptions {
    pub fn new(source_scope: impl Into<String>) -> Self {
        Self {
            max_chunks: VECTOR_CHUNK_INDEX_DEFAULT_MAX_CHUNKS,
            source_scope: source_scope.into(),
            extraction_version: VECTOR_EMBEDDING_CHUNK_EXTRACTION_VERSION.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorChunkIndexMetadata {
    pub metadata_version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artifact_kind: String,
    pub provider: VectorChunkIndexProviderSnapshot,
    pub passport: VectorChunkIndexPassportSnapshot,
    pub source_scope: String,
    pub extraction_version: String,
    pub created_at_unix_ms: u64,
    pub max_chunks: usize,
    pub chunk_count: usize,
    pub omitted_chunks: usize,
    #[serde(default)]
    pub generated_total_chunks: usize,
    #[serde(default)]
    pub generated_text_evidence_chunks: usize,
    #[serde(default)]
    pub generated_graph_entity_chunks: usize,
    #[serde(default)]
    pub generated_file_path_title_chunks: usize,
    #[serde(default)]
    pub generated_metadata_chunks: usize,
    #[serde(default)]
    pub selected_total_chunks: usize,
    #[serde(default)]
    pub selected_text_evidence_chunks: usize,
    #[serde(default)]
    pub selected_graph_entity_chunks: usize,
    #[serde(default)]
    pub selected_file_path_title_chunks: usize,
    #[serde(default)]
    pub selected_metadata_chunks: usize,
    #[serde(default)]
    pub persisted_total_chunks: usize,
    #[serde(default)]
    pub persisted_text_evidence_chunks: usize,
    #[serde(default)]
    pub persisted_graph_entity_chunks: usize,
    #[serde(default)]
    pub persisted_file_path_title_chunks: usize,
    #[serde(default)]
    pub persisted_metadata_chunks: usize,
    #[serde(default)]
    pub chunk_cap: usize,
    #[serde(default)]
    pub chunk_cap_applied: bool,
    #[serde(default)]
    pub chunk_selection_strategy: String,
    #[serde(default)]
    pub input_order_cap: bool,
    #[serde(default)]
    pub persisted_chunks_by_top_level_dir: BTreeMap<String, usize>,
    #[serde(default)]
    pub persisted_chunks_by_file_kind: BTreeMap<String, usize>,
    #[serde(default)]
    pub persisted_chunks_by_source_kind: BTreeMap<String, usize>,
    #[serde(default)]
    pub omitted_by_cap: usize,
    #[serde(default)]
    pub omitted_by_bucket_limit: usize,
    #[serde(default)]
    pub omitted_low_signal: usize,
    #[serde(default)]
    pub per_file_cap: usize,
    #[serde(default)]
    pub per_directory_soft_cap: usize,
    pub indexed_text_bytes: usize,
    pub estimated_vector_bytes_per_chunk: usize,
    #[serde(default)]
    pub estimated_f32_payload_bytes: usize,
    #[serde(default)]
    pub estimated_f32_payload_dim: usize,
    #[serde(default)]
    pub estimated_f32_payload_count: usize,
    pub estimated_vector_bytes: usize,
    #[serde(default)]
    pub estimated_vector_bytes_deprecated_alias_for: String,
    #[serde(default)]
    pub index_artifact_format: String,
    #[serde(default)]
    pub stores_chunk_text: bool,
    #[serde(default)]
    pub stores_chunk_metadata: bool,
    #[serde(default)]
    pub stores_full_source_body: bool,
    #[serde(default)]
    pub vector_payload_compression: String,
}

impl VectorChunkIndexMetadata {
    pub fn incompatibility_reason(
        &self,
        provider: &EmbeddingProviderMetadata,
        passport: &DbPassport,
        options: &VectorChunkIndexBuildOptions,
    ) -> Option<String> {
        if let Some(reason) = self.provider.incompatibility_reason(provider) {
            return Some(reason);
        }
        if let Some(reason) = self.passport.incompatibility_reason(passport) {
            return Some(reason);
        }
        if self.source_scope != options.source_scope {
            return Some("source_scope changed".to_string());
        }
        if self.extraction_version != options.extraction_version {
            return Some("extraction_version changed".to_string());
        }
        None
    }

    pub fn is_compatible_with(
        &self,
        provider: &EmbeddingProviderMetadata,
        passport: &DbPassport,
        options: &VectorChunkIndexBuildOptions,
    ) -> bool {
        self.incompatibility_reason(provider, passport, options)
            .is_none()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VectorChunkIndexEntry {
    pub chunk: VectorEmbeddingChunk,
    embedding: TestEmbeddingVector,
}

impl VectorChunkIndexEntry {
    pub fn embedding(&self) -> &TestEmbeddingVector {
        &self.embedding
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorChunkSearchHit {
    pub chunk: VectorEmbeddingChunk,
    pub score: f32,
    pub rank: usize,
    pub graph_proof: bool,
    pub proof_status: String,
    pub evidence_role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorChunkIndexUpdateSummary {
    pub path: String,
    pub removed_chunks: usize,
    pub inserted_chunks: usize,
    pub omitted_chunks: usize,
    pub chunk_count: usize,
    pub indexed_text_bytes: usize,
    pub estimated_f32_payload_bytes: usize,
    pub estimated_vector_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorChunkIndexBuildTimings {
    #[serde(default)]
    pub total_ms: f64,
    #[serde(default)]
    pub open_store_ms: f64,
    #[serde(default)]
    pub repo_resolution_ms: f64,
    #[serde(default)]
    pub load_passport_ms: f64,
    #[serde(default)]
    pub list_files_ms: f64,
    #[serde(default)]
    pub list_entities_ms: f64,
    #[serde(default)]
    pub filter_group_entities_ms: f64,
    #[serde(default)]
    pub chunk_generation_ms: f64,
    #[serde(default)]
    pub file_source_read_ms: f64,
    #[serde(default)]
    pub file_path_title_chunk_ms: f64,
    #[serde(default)]
    pub text_evidence_chunk_ms: f64,
    #[serde(default)]
    pub graph_entity_chunk_ms: f64,
    #[serde(default)]
    pub in_memory_index_ms: f64,
    #[serde(default)]
    pub input_collect_ms: f64,
    #[serde(default)]
    pub generated_count_ms: f64,
    #[serde(default)]
    pub selection_ms: f64,
    #[serde(default)]
    pub selected_count_ms: f64,
    #[serde(default)]
    pub embedding_ms: f64,
    #[serde(default)]
    pub metadata_build_ms: f64,
    #[serde(default)]
    pub write_json_ms: f64,
    #[serde(default)]
    pub artifact_metadata_ms: f64,
}

impl Default for VectorChunkIndexBuildTimings {
    fn default() -> Self {
        Self {
            total_ms: 0.0,
            open_store_ms: 0.0,
            repo_resolution_ms: 0.0,
            load_passport_ms: 0.0,
            list_files_ms: 0.0,
            list_entities_ms: 0.0,
            filter_group_entities_ms: 0.0,
            chunk_generation_ms: 0.0,
            file_source_read_ms: 0.0,
            file_path_title_chunk_ms: 0.0,
            text_evidence_chunk_ms: 0.0,
            graph_entity_chunk_ms: 0.0,
            in_memory_index_ms: 0.0,
            input_collect_ms: 0.0,
            generated_count_ms: 0.0,
            selection_ms: 0.0,
            selected_count_ms: 0.0,
            embedding_ms: 0.0,
            metadata_build_ms: 0.0,
            write_json_ms: 0.0,
            artifact_metadata_ms: 0.0,
        }
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct InMemoryVectorChunkIndex {
    metadata: VectorChunkIndexMetadata,
    entries: BTreeMap<String, VectorChunkIndexEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedVectorChunkIndex {
    pub metadata: VectorChunkIndexMetadata,
    pub chunks: Vec<VectorEmbeddingChunk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorChunkArtifactFormat {
    CompactJson,
    PrettyJson,
}

impl VectorChunkArtifactFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CompactJson => "compact_json",
            Self::PrettyJson => "pretty_json",
        }
    }

    const fn pretty(self) -> bool {
        matches!(self, Self::PrettyJson)
    }
}

impl Default for VectorChunkArtifactFormat {
    fn default() -> Self {
        Self::CompactJson
    }
}

impl FromStr for VectorChunkArtifactFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().replace('-', "_").as_str() {
            "compact_json" | "compact" | "runtime" => Ok(Self::CompactJson),
            "pretty_json" | "pretty" | "legacy_pretty_json" => Ok(Self::PrettyJson),
            "jsonl" | "binary" => Err(format!(
                "vector artifact format {value} is reserved but not implemented; use compact_json or pretty_json"
            )),
            other => Err(format!(
                "unknown vector artifact format: {other}; expected compact_json or pretty_json"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorChunkIndexArtifactOptions {
    pub runtime_format: VectorChunkArtifactFormat,
    pub audit_artifact_path: Option<PathBuf>,
}

impl Default for VectorChunkIndexArtifactOptions {
    fn default() -> Self {
        Self {
            runtime_format: VectorChunkArtifactFormat::CompactJson,
            audit_artifact_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorChunkSourceBindingValidation {
    pub status: String,
    pub checked_files: usize,
    pub checked_chunks: usize,
    pub unbound_chunks: usize,
    pub stale_reasons: Vec<String>,
}

impl VectorChunkSourceBindingValidation {
    pub fn is_valid(&self) -> bool {
        self.stale_reasons.is_empty()
    }
}

impl InMemoryVectorChunkIndex {
    pub fn metadata(&self) -> &VectorChunkIndexMetadata {
        &self.metadata
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> impl Iterator<Item = &VectorChunkIndexEntry> {
        self.entries.values()
    }

    pub fn remove_chunks_for_path(
        &mut self,
        repo_relative_path: &str,
    ) -> VectorChunkIndexUpdateSummary {
        let normalized_path = normalize_graph_path(repo_relative_path);
        let before = self.entries.len();
        self.entries.retain(|_, entry| {
            entry.chunk.path != normalized_path && entry.chunk.file_id != normalized_path
        });
        let removed_chunks = before.saturating_sub(self.entries.len());
        self.refresh_size_metadata();
        VectorChunkIndexUpdateSummary {
            path: normalized_path,
            removed_chunks,
            inserted_chunks: 0,
            omitted_chunks: 0,
            chunk_count: self.metadata.chunk_count,
            indexed_text_bytes: self.metadata.indexed_text_bytes,
            estimated_f32_payload_bytes: self.metadata.estimated_f32_payload_bytes,
            estimated_vector_bytes: self.metadata.estimated_vector_bytes,
        }
    }

    pub fn replace_chunks_for_path<P, I>(
        &mut self,
        repo_relative_path: &str,
        chunks: I,
        provider: &P,
    ) -> Result<VectorChunkIndexUpdateSummary, IndexError>
    where
        P: EmbeddingProvider,
        I: IntoIterator<Item = VectorEmbeddingChunk>,
    {
        if let Some(reason) = self
            .metadata
            .provider
            .incompatibility_reason(provider.metadata())
        {
            return Err(IndexError::Message(format!(
                "vector chunk index provider metadata is incompatible with update provider: {reason}"
            )));
        }

        let normalized_path = normalize_graph_path(repo_relative_path);
        let removed_chunks = self.remove_chunks_for_path(&normalized_path).removed_chunks;
        let mut inserted_chunks = 0usize;
        let mut omitted_chunks = 0usize;

        for chunk in chunks {
            if chunk.path != normalized_path && chunk.file_id != normalized_path {
                omitted_chunks += 1;
                continue;
            }
            if self.entries.contains_key(&chunk.chunk_id) {
                continue;
            }
            if self.entries.len() >= self.metadata.max_chunks {
                omitted_chunks += 1;
                continue;
            }
            let embedding = provider.embed(&chunk.text).map_err(|error| {
                IndexError::Message(format!(
                    "vector chunk update embedding failed for {}: {error}",
                    chunk.chunk_id
                ))
            })?;
            self.entries.insert(
                chunk.chunk_id.clone(),
                VectorChunkIndexEntry { chunk, embedding },
            );
            inserted_chunks += 1;
        }

        self.metadata.omitted_chunks = self.metadata.omitted_chunks.saturating_add(omitted_chunks);
        self.refresh_size_metadata();
        Ok(VectorChunkIndexUpdateSummary {
            path: normalized_path,
            removed_chunks,
            inserted_chunks,
            omitted_chunks,
            chunk_count: self.metadata.chunk_count,
            indexed_text_bytes: self.metadata.indexed_text_bytes,
            estimated_f32_payload_bytes: self.metadata.estimated_f32_payload_bytes,
            estimated_vector_bytes: self.metadata.estimated_vector_bytes,
        })
    }

    pub fn incompatibility_reason<P: EmbeddingProvider>(
        &self,
        provider: &P,
        passport: &DbPassport,
        options: &VectorChunkIndexBuildOptions,
    ) -> Option<String> {
        self.metadata
            .incompatibility_reason(provider.metadata(), passport, options)
    }

    pub fn search<P: EmbeddingProvider>(
        &self,
        provider: &P,
        query_text: &str,
        top_k: usize,
    ) -> Result<Vec<VectorChunkSearchHit>, IndexError> {
        if top_k == 0 {
            return Ok(Vec::new());
        }
        if let Some(reason) = self
            .metadata
            .provider
            .incompatibility_reason(provider.metadata())
        {
            return Err(IndexError::Message(format!(
                "vector chunk index provider metadata is incompatible with query provider: {reason}"
            )));
        }
        let query = provider.embed(query_text).map_err(|error| {
            IndexError::Message(format!("vector chunk query embedding failed: {error}"))
        })?;
        let mut hits = self
            .entries
            .values()
            .map(|entry| {
                vector_embedding_dot_product(&query, entry.embedding()).map(|score| {
                    VectorChunkSearchHit {
                        chunk: entry.chunk.clone(),
                        score,
                        rank: 0,
                        graph_proof: entry.chunk.graph_proof,
                        proof_status: entry.chunk.proof_status.clone(),
                        evidence_role: entry.chunk.evidence_role.clone(),
                    }
                })
            })
            .collect::<Result<Vec<_>, IndexError>>()?;
        hits.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.chunk.chunk_id.cmp(&right.chunk.chunk_id))
        });
        hits.truncate(top_k);
        for (index, hit) in hits.iter_mut().enumerate() {
            hit.rank = index + 1;
        }
        Ok(hits)
    }

    fn refresh_size_metadata(&mut self) {
        self.metadata.chunk_count = self.entries.len();
        let selected_counts =
            VectorChunkKindCounts::from_chunks(self.entries.values().map(|entry| &entry.chunk));
        self.metadata.selected_total_chunks = selected_counts.total;
        self.metadata.selected_text_evidence_chunks = selected_counts.text_evidence;
        self.metadata.selected_graph_entity_chunks = selected_counts.graph_entity;
        self.metadata.selected_file_path_title_chunks = selected_counts.file_path_title;
        self.metadata.selected_metadata_chunks = selected_counts.metadata;
        self.metadata.persisted_total_chunks = selected_counts.total;
        self.metadata.persisted_text_evidence_chunks = selected_counts.text_evidence;
        self.metadata.persisted_graph_entity_chunks = selected_counts.graph_entity;
        self.metadata.persisted_file_path_title_chunks = selected_counts.file_path_title;
        self.metadata.persisted_metadata_chunks = selected_counts.metadata;
        self.metadata.persisted_chunks_by_top_level_dir =
            count_chunks_by_top_level_dir(self.entries.values().map(|entry| &entry.chunk));
        self.metadata.persisted_chunks_by_file_kind =
            count_chunks_by_file_kind(self.entries.values().map(|entry| &entry.chunk));
        self.metadata.persisted_chunks_by_source_kind =
            count_chunks_by_source_kind(self.entries.values().map(|entry| &entry.chunk));
        self.metadata.chunk_cap = self.metadata.max_chunks;
        self.metadata.chunk_cap_applied = self.metadata.omitted_chunks > 0
            || self.metadata.generated_total_chunks > self.metadata.persisted_total_chunks;
        self.metadata.indexed_text_bytes = self
            .entries
            .values()
            .map(|entry| entry.chunk.byte_count)
            .sum();
        self.metadata.estimated_f32_payload_count = self.metadata.chunk_count;
        self.metadata.estimated_f32_payload_dim = self.metadata.provider.dimension;
        self.metadata.estimated_f32_payload_bytes = self
            .metadata
            .chunk_count
            .saturating_mul(self.metadata.estimated_vector_bytes_per_chunk);
        self.metadata.estimated_vector_bytes = self.metadata.estimated_f32_payload_bytes;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum GlobalEntityWriteMode {
    UpsertIndexed,
    InsertRecordIfMissing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct GlobalEntityFact {
    entity: Entity,
    write_mode: GlobalEntityWriteMode,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct GlobalFactReductionPlan {
    entities: Vec<GlobalEntityFact>,
    edges: Vec<Edge>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct GlobalFactApplySummary {
    entities_inserted: usize,
    edges_inserted: usize,
    edges_upserted_existing: usize,
}

impl GlobalFactReductionPlan {
    fn push_entity(&mut self, entity: Entity, write_mode: GlobalEntityWriteMode) {
        if let Some(existing) = self
            .entities
            .iter_mut()
            .find(|fact| fact.entity.id == entity.id)
        {
            if matches!(write_mode, GlobalEntityWriteMode::UpsertIndexed) {
                existing.write_mode = GlobalEntityWriteMode::UpsertIndexed;
                existing.entity = entity;
            }
            return;
        }
        self.entities.push(GlobalEntityFact { entity, write_mode });
    }

    fn push_edge(&mut self, edge: Edge) {
        if self.edges.iter().any(|existing| existing.id == edge.id) {
            return;
        }
        self.edges.push(edge);
    }

    fn sort(&mut self) {
        self.entities
            .sort_by(|left, right| left.entity.id.cmp(&right.entity.id));
        self.edges.sort_by(|left, right| left.id.cmp(&right.id));
    }

    fn retain_paths(&mut self, repo_relative_paths: &BTreeSet<String>) {
        if repo_relative_paths.is_empty() {
            self.entities.clear();
            self.edges.clear();
            return;
        }
        self.entities.retain(|fact| {
            repo_relative_paths.contains(&normalize_graph_path(&fact.entity.repo_relative_path))
        });
        self.edges.retain(|edge| {
            repo_relative_paths
                .contains(&normalize_graph_path(&edge.source_span.repo_relative_path))
        });
    }

    fn apply_reducer_edge_budget(
        &mut self,
        stage: &str,
        budget: usize,
    ) -> Option<GraphOutputBudgetHit> {
        let before = self.edges.len();
        if before <= budget {
            return None;
        }
        self.edges.truncate(budget);
        Some(graph_budget_hit(
            "<global>",
            stage,
            "reducer_edges_per_stage",
            before,
            self.edges.len(),
            budget,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticImportSpec {
    importer_path: String,
    imported_name: String,
    local_name: String,
    module_specifier: String,
    kind: StaticImportKind,
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticReexportSpec {
    exporter_path: String,
    imported_name: String,
    exported_name: String,
    module_specifier: String,
    kind: StaticImportKind,
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticDynamicImportSpec {
    specifier: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticImportKind {
    Named,
    Default,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalFactSymbol {
    pub id: String,
    pub kind: EntityKind,
    pub name: String,
    pub qualified_name: String,
    pub source_span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalFactRelation {
    pub edge_id: String,
    pub relation: RelationKind,
    pub head_id: String,
    pub tail_id: String,
    pub source_span: SourceSpan,
    pub exactness: Exactness,
    pub derived: bool,
    pub extractor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalFactReference {
    pub reference_id: String,
    pub name: String,
    pub relation: RelationKind,
    pub source_span: SourceSpan,
    pub exactness: Exactness,
    pub extractor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalFactBundle {
    pub repo_relative_path: String,
    pub file_hash: String,
    pub language: Option<String>,
    pub source: String,
    pub needs_delete: bool,
    pub duplicate_of: Option<String>,
    pub template_required: bool,
    pub declarations: Vec<LocalFactSymbol>,
    pub imports: Vec<LocalFactRelation>,
    pub exports: Vec<LocalFactRelation>,
    pub local_callsites: Vec<LocalFactRelation>,
    pub local_reads_writes: Vec<LocalFactRelation>,
    pub unresolved_references: Vec<LocalFactReference>,
    pub source_spans: Vec<SourceSpan>,
    pub extraction_warnings: Vec<String>,
    pub extraction: BasicExtraction,
}

pub type IndexedFileOutput = LocalFactBundle;

impl LocalFactBundle {
    fn new(
        repo_relative_path: String,
        source: String,
        needs_delete: bool,
        duplicate_of: Option<String>,
        template_required: bool,
        extraction: BasicExtraction,
    ) -> Self {
        let mut declarations = extraction
            .entities
            .iter()
            .map(|entity| LocalFactSymbol {
                id: entity.id.clone(),
                kind: entity.kind,
                name: entity.name.clone(),
                qualified_name: entity.qualified_name.clone(),
                source_span: entity.source_span.clone(),
            })
            .collect::<Vec<_>>();
        declarations.sort_by(|left, right| left.id.cmp(&right.id));

        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let mut local_callsites = Vec::new();
        let mut local_reads_writes = Vec::new();
        let mut unresolved_references = Vec::new();
        for edge in &extraction.edges {
            let fact = local_fact_relation(edge);
            match edge.relation {
                RelationKind::Imports | RelationKind::AliasedBy | RelationKind::AliasOf => {
                    imports.push(fact)
                }
                RelationKind::Exports | RelationKind::Reexports => exports.push(fact),
                RelationKind::Calls
                | RelationKind::CalledBy
                | RelationKind::Callee
                | RelationKind::Argument0
                | RelationKind::Argument1
                | RelationKind::ArgumentN
                | RelationKind::ReturnsTo => local_callsites.push(fact),
                RelationKind::Reads
                | RelationKind::Writes
                | RelationKind::Mutates
                | RelationKind::MutatedBy
                | RelationKind::FlowsTo
                | RelationKind::ReachingDef
                | RelationKind::AssignedFrom
                | RelationKind::ControlDependsOn
                | RelationKind::DataDependsOn => local_reads_writes.push(fact),
                _ => {}
            }
            if let Some(reference) = unresolved_reference_for_edge(edge) {
                unresolved_references.push(reference);
            }
        }
        imports.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));
        exports.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));
        local_callsites.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));
        local_reads_writes.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));
        unresolved_references.sort_by(|left, right| {
            left.source_span
                .to_string()
                .cmp(&right.source_span.to_string())
                .then_with(|| left.reference_id.cmp(&right.reference_id))
                .then_with(|| left.relation.to_string().cmp(&right.relation.to_string()))
        });

        let mut source_spans = Vec::new();
        for entity in &extraction.entities {
            if let Some(span) = &entity.source_span {
                push_unique_span(&mut source_spans, span.clone());
            }
        }
        for edge in &extraction.edges {
            push_unique_span(&mut source_spans, edge.source_span.clone());
        }
        source_spans.sort_by_key(|span| span.to_string());

        let mut extraction_warnings = Vec::new();
        if let Some(canonical) = &duplicate_of {
            extraction_warnings.push(format!(
                "duplicate source content; local graph facts are owned by {canonical}"
            ));
        }

        Self {
            repo_relative_path,
            file_hash: extraction.file.file_hash.clone(),
            language: extraction.file.language.clone(),
            source,
            needs_delete,
            duplicate_of,
            template_required,
            declarations,
            imports,
            exports,
            local_callsites,
            local_reads_writes,
            unresolved_references,
            source_spans,
            extraction_warnings,
            extraction,
        }
    }
}

fn apply_graph_output_budgets_to_extraction(
    repo_relative_path: &str,
    extraction: &mut BasicExtraction,
    budgets: &GraphOutputBudgets,
) -> Vec<GraphOutputBudgetHit> {
    let mut hits = Vec::new();
    apply_relation_fanout_budget(repo_relative_path, extraction, budgets, &mut hits);
    apply_derived_edge_budget(repo_relative_path, extraction, budgets, &mut hits);
    apply_local_fact_budget(repo_relative_path, extraction, budgets, &mut hits);
    apply_source_span_budget(repo_relative_path, extraction, budgets, &mut hits);
    if !hits.is_empty() {
        annotate_graph_output_budget_hits(&mut extraction.file.metadata, budgets, &hits);
    }
    hits
}

fn graph_budget_hit(
    repo_relative_path: &str,
    stage: &str,
    kind: &str,
    before: usize,
    after: usize,
    budget: usize,
) -> GraphOutputBudgetHit {
    GraphOutputBudgetHit {
        repo_relative_path: normalize_graph_path(repo_relative_path),
        stage: stage.to_string(),
        kind: kind.to_string(),
        before,
        after,
        budget,
        omitted: before.saturating_sub(after),
        claimability_label: "degraded_file_nonclaimable_for_omitted_facts".to_string(),
    }
}

fn apply_relation_fanout_budget(
    repo_relative_path: &str,
    extraction: &mut BasicExtraction,
    budgets: &GraphOutputBudgets,
    hits: &mut Vec<GraphOutputBudgetHit>,
) {
    let budget = budgets.max_relation_fanout_per_file;
    if budget == 0 || extraction.edges.is_empty() {
        return;
    }
    let before = extraction.edges.len();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut retained = Vec::with_capacity(extraction.edges.len().min(budget));
    for edge in std::mem::take(&mut extraction.edges) {
        let key = format!("{}|{}", edge.head_id, edge.relation);
        let count = counts.entry(key).or_default();
        if *count < budget {
            *count += 1;
            retained.push(edge);
        }
    }
    let after = retained.len();
    extraction.edges = retained;
    if after < before {
        hits.push(graph_budget_hit(
            repo_relative_path,
            "local_extraction",
            "relation_fanout_per_file",
            before,
            after,
            budget,
        ));
    }
}

fn apply_derived_edge_budget(
    repo_relative_path: &str,
    extraction: &mut BasicExtraction,
    budgets: &GraphOutputBudgets,
    hits: &mut Vec<GraphOutputBudgetHit>,
) {
    let budget = budgets.max_derived_edges_per_file;
    let before = extraction.edges.iter().filter(|edge| edge.derived).count();
    if before <= budget {
        return;
    }
    let mut retained_derived = 0usize;
    extraction.edges.retain(|edge| {
        if !edge.derived {
            return true;
        }
        if retained_derived < budget {
            retained_derived += 1;
            true
        } else {
            false
        }
    });
    hits.push(graph_budget_hit(
        repo_relative_path,
        "local_extraction",
        "derived_edges_per_file",
        before,
        retained_derived,
        budget,
    ));
}

fn apply_local_fact_budget(
    repo_relative_path: &str,
    extraction: &mut BasicExtraction,
    budgets: &GraphOutputBudgets,
    hits: &mut Vec<GraphOutputBudgetHit>,
) {
    let budget = budgets.max_local_facts_per_file;
    let before = extraction.entities.len() + extraction.edges.len();
    if before <= budget {
        return;
    }
    if extraction.entities.len() >= budget {
        extraction.entities.truncate(budget);
        extraction.edges.clear();
    } else {
        let allowed_edges = budget.saturating_sub(extraction.entities.len());
        extraction.edges.truncate(allowed_edges);
    }
    retain_edges_with_present_entities(extraction);
    let after = extraction.entities.len() + extraction.edges.len();
    hits.push(graph_budget_hit(
        repo_relative_path,
        "local_extraction",
        "local_facts_per_file",
        before,
        after,
        budget,
    ));
}

fn apply_source_span_budget(
    repo_relative_path: &str,
    extraction: &mut BasicExtraction,
    budgets: &GraphOutputBudgets,
    hits: &mut Vec<GraphOutputBudgetHit>,
) {
    let budget = budgets.max_source_spans_per_file;
    let before = extraction_source_span_count(extraction);
    if before <= budget {
        return;
    }
    let mut seen = BTreeSet::<String>::new();
    extraction.entities.retain(|entity| {
        let Some(span) = &entity.source_span else {
            return true;
        };
        let key = span.to_string();
        if seen.contains(&key) || seen.len() < budget {
            seen.insert(key);
            true
        } else {
            false
        }
    });
    retain_edges_with_present_entities(extraction);
    extraction.edges.retain(|edge| {
        let key = edge.source_span.to_string();
        if seen.contains(&key) || seen.len() < budget {
            seen.insert(key);
            true
        } else {
            false
        }
    });
    retain_edges_with_present_entities(extraction);
    let after = extraction_source_span_count(extraction);
    hits.push(graph_budget_hit(
        repo_relative_path,
        "local_extraction",
        "source_spans_per_file",
        before,
        after,
        budget,
    ));
}

fn retain_edges_with_present_entities(extraction: &mut BasicExtraction) {
    let entity_ids = extraction
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<BTreeSet<_>>();
    extraction.edges.retain(|edge| {
        entity_ids.contains(edge.head_id.as_str()) && entity_ids.contains(edge.tail_id.as_str())
    });
}

fn extraction_source_span_count(extraction: &BasicExtraction) -> usize {
    let mut spans = BTreeSet::<String>::new();
    for entity in &extraction.entities {
        if let Some(span) = &entity.source_span {
            spans.insert(span.to_string());
        }
    }
    for edge in &extraction.edges {
        spans.insert(edge.source_span.to_string());
    }
    spans.len()
}

fn annotate_graph_output_budget_hits(
    metadata: &mut Metadata,
    budgets: &GraphOutputBudgets,
    hits: &[GraphOutputBudgetHit],
) {
    metadata.insert("graph_output_budget_hit".to_string(), json!(true));
    metadata.insert(
        "claim_state".to_string(),
        json!("graph_output_degraded_budget_hit"),
    );
    metadata.insert(
        "graph_output_claimability".to_string(),
        json!("degraded_file_nonclaimable_for_omitted_facts"),
    );
    metadata.insert("graph_relation_claims".to_string(), json!("partial"));
    metadata.insert(
        "graph_output_budget_policy".to_string(),
        serde_json::to_value(budgets).unwrap_or(Value::Null),
    );
    metadata.insert(
        "graph_output_budget_hits".to_string(),
        serde_json::to_value(hits).unwrap_or(Value::Null),
    );
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PreliminarySymbolTable {
    pub by_id: BTreeMap<String, LocalFactSymbol>,
    pub by_file: BTreeMap<String, Vec<String>>,
    pub by_qualified_name: BTreeMap<String, Vec<String>>,
}

impl PreliminarySymbolTable {
    fn insert_declaration(
        &mut self,
        repo_relative_path: &str,
        symbol: LocalFactSymbol,
    ) -> Option<String> {
        let existing_conflict = self
            .by_id
            .get(&symbol.id)
            .filter(|existing| *existing != &symbol)
            .map(|_| symbol.id.clone());
        self.by_id
            .entry(symbol.id.clone())
            .or_insert_with(|| symbol.clone());
        push_unique_sorted(
            self.by_file
                .entry(repo_relative_path.to_string())
                .or_default(),
            symbol.id.clone(),
        );
        push_unique_sorted(
            self.by_qualified_name
                .entry(symbol.qualified_name.clone())
                .or_default(),
            symbol.id.clone(),
        );
        existing_conflict
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReducedIndexPlan {
    pub bundles: Vec<LocalFactBundle>,
    pub symbol_table: PreliminarySymbolTable,
    global_facts: GlobalFactReductionPlan,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseExtractStat {
    pub repo_relative_path: String,
    pub parse_ms: u128,
    pub extraction_ms: u128,
    pub bundle_ms: u128,
    pub parse_error: bool,
    pub syntax_error: bool,
    pub skipped: bool,
    pub message: Option<String>,
    pub graph_output_budget_hits: Vec<GraphOutputBudgetHit>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IncrementalIndexSummary {
    pub status: String,
    pub repo_root: String,
    pub db_path: String,
    pub changed_files: Vec<String>,
    pub dependency_closure: RtdsDependencyClosureSummary,
    pub files_seen: usize,
    pub files_walked: usize,
    pub files_metadata_unchanged: usize,
    pub files_read: usize,
    pub files_hashed: usize,
    pub files_parsed: usize,
    pub files_indexed: usize,
    pub files_deleted: usize,
    pub files_renamed: usize,
    pub files_skipped: usize,
    pub files_ignored: usize,
    pub parse_errors: usize,
    pub syntax_errors: usize,
    pub entities: usize,
    pub edges: usize,
    pub duplicate_edges_upserted: usize,
    pub binary_signatures_updated: usize,
    pub adjacency_edges: usize,
    pub deleted_fact_files: usize,
    pub dirty_path_evidence_count: usize,
    pub ignored_paths_seen: usize,
    pub ignored_paths_with_existing_facts: usize,
    pub stale_facts_deleted_for_ignored_paths: usize,
    pub deleted_file_facts_removed: usize,
    pub path_cleanup_reasons: BTreeMap<String, Vec<String>>,
    pub global_hash_check_ran: bool,
    pub storage_audit_ran: bool,
    pub integrity_check_ran: bool,
    pub profile: Option<IndexProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RtdsDependencyClosureBudget {
    pub max_dirty_files: usize,
    pub max_edges_inspected: usize,
    pub max_relation_classes: usize,
    pub max_wall_ms: u64,
    pub max_source_bytes: u64,
    pub max_db_rows_hydrated: usize,
    pub max_per_relation: usize,
}

impl Default for RtdsDependencyClosureBudget {
    fn default() -> Self {
        if let Some(budget) =
            RTDS_CLOSURE_BUDGET_OVERRIDE.with(|override_cell| override_cell.borrow().clone())
        {
            return budget;
        }
        Self {
            max_dirty_files: env_usize_or(
                "CODEGRAPH_RTDS_CLOSURE_MAX_DIRTY_FILES",
                DEFAULT_RTDS_CLOSURE_MAX_DIRTY_FILES,
            ),
            max_edges_inspected: env_usize_or(
                "CODEGRAPH_RTDS_CLOSURE_MAX_EDGES_INSPECTED",
                DEFAULT_RTDS_CLOSURE_MAX_EDGES_INSPECTED,
            ),
            max_relation_classes: env_usize_or(
                "CODEGRAPH_RTDS_CLOSURE_MAX_RELATION_CLASSES",
                DEFAULT_RTDS_CLOSURE_MAX_RELATION_CLASSES,
            ),
            max_wall_ms: env_u64_or(
                "CODEGRAPH_RTDS_CLOSURE_MAX_WALL_MS",
                DEFAULT_RTDS_CLOSURE_MAX_WALL_MS,
            ),
            max_source_bytes: env_u64_or(
                "CODEGRAPH_RTDS_CLOSURE_MAX_SOURCE_BYTES",
                DEFAULT_RTDS_CLOSURE_MAX_SOURCE_BYTES,
            ),
            max_db_rows_hydrated: env_usize_or(
                "CODEGRAPH_RTDS_CLOSURE_MAX_DB_ROWS_HYDRATED",
                DEFAULT_RTDS_CLOSURE_MAX_DB_ROWS_HYDRATED,
            ),
            max_per_relation: env_usize_or(
                "CODEGRAPH_RTDS_CLOSURE_MAX_PER_RELATION",
                DEFAULT_RTDS_CLOSURE_MAX_PER_RELATION,
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RtdsDependencyClosureSummary {
    pub status: String,
    pub scope_version: String,
    pub requested_changed_files: Vec<String>,
    pub closure_files_considered: Vec<String>,
    pub closure_files_updated: Vec<String>,
    pub closure_edges_inspected: usize,
    pub closure_relation_classes: Vec<String>,
    pub closure_budget_hit: bool,
    pub degraded_relation_classes: Vec<String>,
    pub skipped_relation_classes: Vec<String>,
    pub closure_unknowns: Vec<String>,
    pub budgets: RtdsDependencyClosureBudget,
    pub full_repo_fallback_avoided: bool,
    pub fallback_avoided_reason: String,
    pub manual_full_index_recommendation: Option<String>,
}

impl Default for RtdsDependencyClosureSummary {
    fn default() -> Self {
        Self {
            status: "not_run".to_string(),
            scope_version: "rtds_dependency_closure_v1".to_string(),
            requested_changed_files: Vec::new(),
            closure_files_considered: Vec::new(),
            closure_files_updated: Vec::new(),
            closure_edges_inspected: 0,
            closure_relation_classes: Vec::new(),
            closure_budget_hit: false,
            degraded_relation_classes: Vec::new(),
            skipped_relation_classes: Vec::new(),
            closure_unknowns: Vec::new(),
            budgets: RtdsDependencyClosureBudget::default(),
            full_repo_fallback_avoided: true,
            fallback_avoided_reason: "bounded_dependency_closure_v1_does_not_silently_full_reindex"
                .to_string(),
            manual_full_index_recommendation: None,
        }
    }
}

#[derive(Debug)]
pub struct IncrementalIndexCache {
    binary_index: InMemoryBinaryVectorIndex,
    signature_words: BTreeMap<String, Vec<u64>>,
    adjacency: ExactGraphQueryEngine,
}

impl IncrementalIndexCache {
    pub fn new(dimensions: usize) -> Result<Self, IndexError> {
        let binary_index = InMemoryBinaryVectorIndex::new(dimensions).map_err(|error| {
            IndexError::Message(format!("binary vector cache init failed: {error}"))
        })?;
        Ok(Self {
            binary_index,
            signature_words: BTreeMap::new(),
            adjacency: ExactGraphQueryEngine::new(Vec::new()),
        })
    }

    pub fn has_cached_facts(&self) -> bool {
        !self.signature_words.is_empty() || self.adjacency_edge_count() > 0
    }

    pub fn refresh_from_store(&mut self, store: &SqliteGraphStore) -> Result<(), IndexError> {
        let entities = store.list_entities(UNBOUNDED_STORE_READ_LIMIT)?;
        let mut binary_index = InMemoryBinaryVectorIndex::new(self.binary_index.dimensions())
            .map_err(|error| {
                IndexError::Message(format!("binary vector cache refresh failed: {error}"))
            })?;
        let mut signature_words = BTreeMap::new();
        for entity in entities {
            let signature = entity_binary_signature(&entity, binary_index.dimensions())?;
            signature_words.insert(entity.id.clone(), signature.words().to_vec());
            binary_index
                .upsert_signature(entity.id, signature)
                .map_err(|error| {
                    IndexError::Message(format!("binary signature update failed: {error}"))
                })?;
        }

        let edges = store.list_edges(UNBOUNDED_STORE_READ_LIMIT)?;
        self.adjacency = ExactGraphQueryEngine::new(edges);
        self.binary_index = binary_index;
        self.signature_words = signature_words;
        Ok(())
    }

    pub fn refresh_from_changed_facts(
        &mut self,
        removed_entity_ids: &[String],
        entities: &[Entity],
        edges: &[Edge],
    ) -> Result<(), IndexError> {
        for entity_id in removed_entity_ids {
            self.signature_words.remove(entity_id);
            self.binary_index.remove_signature(entity_id);
        }
        for entity in entities {
            let signature = entity_binary_signature(entity, self.binary_index.dimensions())?;
            self.signature_words
                .insert(entity.id.clone(), signature.words().to_vec());
            self.binary_index
                .upsert_signature(entity.id.clone(), signature)
                .map_err(|error| {
                    IndexError::Message(format!("binary signature update failed: {error}"))
                })?;
        }
        self.adjacency = ExactGraphQueryEngine::new(edges.to_vec());
        Ok(())
    }

    pub fn signature_words(&self, entity_id: &str) -> Option<&[u64]> {
        self.signature_words
            .get(entity_id)
            .map(std::vec::Vec::as_slice)
    }

    pub fn adjacency_edge_count(&self) -> usize {
        self.adjacency.edge_count()
    }
}

pub fn index_repo(repo_path: &Path) -> Result<IndexSummary, IndexError> {
    index_repo_with_options(repo_path, IndexOptions::default())
}

pub fn index_repo_with_options(
    repo_path: &Path,
    options: IndexOptions,
) -> Result<IndexSummary, IndexError> {
    let repo_root = resolve_repo_root_for_index(repo_path)?;
    let db_path = default_db_path(&repo_root);
    index_repo_to_db_with_options(&repo_root, &db_path, options)
}

pub fn index_repo_to_db(repo_path: &Path, db_path: &Path) -> Result<IndexSummary, IndexError> {
    index_repo_to_db_with_options(repo_path, db_path, IndexOptions::default())
}

pub fn index_repo_to_db_with_options(
    repo_path: &Path,
    db_path: &Path,
    options: IndexOptions,
) -> Result<IndexSummary, IndexError> {
    let repo_root = resolve_repo_root_for_index(repo_path)?;
    let db_path = normalize_db_path(&repo_root, db_path);
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let expected_passport = expected_db_passport(&repo_root, &options)?;
    let preflight = inspect_db_preflight(&db_path, SCHEMA_VERSION, &expected_passport);
    let decision = decide_db_lifecycle(&db_path, &options, &preflight);
    match decision.action {
        DbLifecycleAction::FreshRebuild => {
            let temp_db_path = atomic_temp_db_path(&db_path);
            let mut summary = index_repo_to_atomic_cold_db(
                &repo_root,
                &db_path,
                options,
                Some(temp_db_path.clone()),
            )?;
            summary.db_lifecycle = Some(decision.into_evidence(Some(temp_db_path), true));
            Ok(summary)
        }
        DbLifecycleAction::IncrementalReuse { claimable } => {
            let mut summary = index_repo_to_existing_db_with_options(
                &repo_root,
                &db_path,
                options,
                PostIndexCheck::Quick,
                BulkIndexLoadDurability::VisibleDb,
            )?;
            summary.db_lifecycle = Some(decision.into_evidence(None, claimable));
            Ok(summary)
        }
        DbLifecycleAction::Fail => Err(IndexError::Message(format!(
            "DB lifecycle preflight failed for {}: {}",
            db_path.display(),
            decision.reasons.join("; ")
        ))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PostIndexCheck {
    None,
    Full,
    Quick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BulkIndexLoadDurability {
    VisibleDb,
    HiddenAtomicColdTemp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbLifecycleAction {
    FreshRebuild,
    IncrementalReuse { claimable: bool },
    Fail,
}

#[derive(Debug, Clone, PartialEq)]
struct DbLifecycleDecision {
    mode: String,
    action: DbLifecycleAction,
    decision: String,
    reasons: Vec<String>,
    passport_status: String,
    old_db_used: bool,
    old_db_replaced: bool,
    explicit_db_path: bool,
    preflight: Option<DbPreflightReport>,
}

impl DbLifecycleDecision {
    fn into_evidence(
        self,
        fresh_temp_db_path: Option<PathBuf>,
        claimable: bool,
    ) -> DbLifecycleEvidence {
        DbLifecycleEvidence {
            mode: self.mode,
            decision: self.decision,
            reasons: self.reasons,
            passport_status: self.passport_status,
            old_db_used: self.old_db_used,
            old_db_replaced: self.old_db_replaced,
            fresh_temp_db_path: fresh_temp_db_path.map(|path| path_string(&path)),
            claimable,
            explicit_db_path: self.explicit_db_path,
            preflight: self.preflight,
        }
    }
}

fn decide_db_lifecycle(
    db_path: &Path,
    options: &IndexOptions,
    preflight: &DbPreflightReport,
) -> DbLifecycleDecision {
    let policy = options.db_lifecycle.policy;
    let explicit = options.db_lifecycle.explicit_db_path;
    let mode = policy.as_str().to_string();
    let db_exists = db_path.exists();
    let mut reasons = preflight.reasons.clone();
    if reasons.is_empty() && preflight.valid {
        reasons.push("passport and read-only preflight are valid".to_string());
    }

    if policy == DbLifecyclePolicy::FreshRebuild {
        return DbLifecycleDecision {
            mode,
            action: DbLifecycleAction::FreshRebuild,
            decision: "fresh_rebuild".to_string(),
            reasons: vec!["--fresh/--rebuild requested atomic fresh rebuild".to_string()],
            passport_status: preflight.passport_status.clone(),
            old_db_used: false,
            old_db_replaced: db_exists,
            explicit_db_path: explicit,
            preflight: Some(preflight.clone()),
        };
    }

    if preflight.passport_status == "locked" {
        return DbLifecycleDecision {
            mode,
            action: DbLifecycleAction::Fail,
            decision: "failed".to_string(),
            reasons,
            passport_status: preflight.passport_status.clone(),
            old_db_used: false,
            old_db_replaced: false,
            explicit_db_path: explicit,
            preflight: Some(preflight.clone()),
        };
    }

    if !db_exists && policy != DbLifecyclePolicy::IncrementalRequired {
        return DbLifecycleDecision {
            mode,
            action: DbLifecycleAction::FreshRebuild,
            decision: "fresh_rebuild".to_string(),
            reasons,
            passport_status: preflight.passport_status.clone(),
            old_db_used: false,
            old_db_replaced: false,
            explicit_db_path: explicit,
            preflight: Some(preflight.clone()),
        };
    }

    if preflight.valid {
        return DbLifecycleDecision {
            mode,
            action: DbLifecycleAction::IncrementalReuse { claimable: true },
            decision: "incremental_reuse".to_string(),
            reasons,
            passport_status: preflight.passport_status.clone(),
            old_db_used: true,
            old_db_replaced: false,
            explicit_db_path: explicit,
            preflight: Some(preflight.clone()),
        };
    }

    if policy == DbLifecyclePolicy::DiagnosticStaleReuse {
        return DbLifecycleDecision {
            mode,
            action: DbLifecycleAction::IncrementalReuse { claimable: false },
            decision: "diagnostic_stale_reuse".to_string(),
            reasons,
            passport_status: preflight.passport_status.clone(),
            old_db_used: true,
            old_db_replaced: false,
            explicit_db_path: explicit,
            preflight: Some(preflight.clone()),
        };
    }

    if policy == DbLifecyclePolicy::IncrementalRequired
        || policy == DbLifecyclePolicy::FailOnDbProblem
        || explicit
    {
        return DbLifecycleDecision {
            mode,
            action: DbLifecycleAction::Fail,
            decision: "failed".to_string(),
            reasons,
            passport_status: preflight.passport_status.clone(),
            old_db_used: false,
            old_db_replaced: false,
            explicit_db_path: explicit,
            preflight: Some(preflight.clone()),
        };
    }

    DbLifecycleDecision {
        mode,
        action: DbLifecycleAction::FreshRebuild,
        decision: "fresh_rebuild".to_string(),
        reasons,
        passport_status: preflight.passport_status.clone(),
        old_db_used: false,
        old_db_replaced: db_exists,
        explicit_db_path: explicit,
        preflight: Some(preflight.clone()),
    }
}

fn index_repo_to_existing_db_with_options(
    repo_root: &Path,
    db_path: &Path,
    options: IndexOptions,
    post_check: PostIndexCheck,
    bulk_durability: BulkIndexLoadDurability,
) -> Result<IndexSummary, IndexError> {
    let total_start = Instant::now();
    reset_sqlite_profile();
    let mut phase_profile = IndexPhaseRecorder::default();
    let open_start = Instant::now();
    let store = SqliteGraphStore::open(&db_path)?;
    phase_profile.add_duration("open_store", open_start.elapsed(), 1, 0);
    let mut summary = IndexSummary {
        repo_root: path_string(&repo_root),
        db_path: path_string(&db_path),
        db_lifecycle: None,
        build_mode: options.build_mode.as_str().to_string(),
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
        storage_policy: options.storage_mode.storage_policy().to_string(),
        issue_counts: BTreeMap::new(),
        issues: Vec::new(),
        graph_output_budgets: GraphOutputBudgetSummary::new(options.graph_output_budgets.clone()),
        scope: None,
        candidate_spool: None,
        profile: None,
    };

    let discovery_start = Instant::now();
    let scoped_files = collect_repo_files_with_scope(&repo_root, &options.scope)?;
    let files = scoped_files.files;
    summary.scope = Some(scoped_files.scope_report);
    initialize_candidate_spool(&mut summary, repo_root, db_path, &options)?;
    let file_discovery_ms = discovery_start.elapsed().as_millis();
    phase_profile.add_ms("file_walk", file_discovery_ms as f64, 1, files.len() as u64);
    let indexed_at = unix_time_ms();
    let mut source_candidates = Vec::new();
    let mut text_evidence_candidates = Vec::new();
    let mut skipped_unchanged_files = 0usize;
    for file_path in files {
        summary.files_seen += 1;
        summary.files_walked += 1;
        let repo_relative_path = repo_relative_path(&repo_root, &file_path)?;
        if detect_language(&file_path).is_some() {
            source_candidates.push((file_path, repo_relative_path));
            continue;
        }
        if let Some(kind) = classify_scoped_text_evidence_path(&repo_relative_path) {
            text_evidence_candidates.push(TextEvidenceCandidate {
                file_path,
                repo_relative_path,
                kind,
            });
        } else {
            summary.files_skipped += 1;
        }
    }

    let current_repo_paths = source_candidates
        .iter()
        .map(|(_, repo_relative_path)| repo_relative_path.clone())
        .chain(
            text_evidence_candidates
                .iter()
                .map(|candidate| candidate.repo_relative_path.clone()),
        )
        .collect::<BTreeSet<_>>();
    let metadata_start = Instant::now();
    let existing_files = store.list_files(UNBOUNDED_STORE_READ_LIMIT)?;
    let mut manifest_diff = ManifestDiffEngine::new(existing_files, &current_repo_paths);
    phase_profile.add_duration(
        "metadata_diff",
        metadata_start.elapsed(),
        1,
        source_candidates.len() as u64,
    );
    let mut db_write_ms = 0u128;

    emit_index_progress(
        &options,
        json!({
            "event": "index_started",
            "repo_root": summary.repo_root.clone(),
            "db_path": summary.db_path.clone(),
            "files_seen": summary.files_seen,
            "source_candidates": source_candidates.len(),
            "text_evidence_candidates": text_evidence_candidates.len(),
            "stale_candidates": manifest_diff.stale_cleanup_paths().len(),
            "batch_max_files": DEFAULT_INDEX_BATCH_MAX_FILES,
            "batch_max_source_bytes": DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES,
        }),
    );

    let mut parse_ms = 0u128;
    let mut extraction_ms = 0u128;
    let mut max_worker_count = 1usize;
    let mut bulk_index_load_started = false;
    let mut hashed_candidates = Vec::<HashedIndexCandidate>::new();
    if !text_evidence_candidates.is_empty() {
        let text_evidence_transaction_start = Instant::now();
        store.begin_write_transaction()?;
        let text_evidence_result = (|| -> Result<(), IndexError> {
            for candidate in text_evidence_candidates {
                let metadata_start = Instant::now();
                let file_metadata = fs::metadata(&candidate.file_path)?;
                let size_bytes = file_metadata.len();
                let existing_file = manifest_diff
                    .existing_file(&candidate.repo_relative_path)
                    .cloned();
                let existing_is_text_evidence = existing_file.as_ref().is_some_and(|record| {
                    file_record_is_text_evidence_kind(record, candidate.kind)
                });
                if existing_is_text_evidence
                    && manifest_diff.classify_file(
                        &candidate.repo_relative_path,
                        size_bytes,
                        &file_metadata,
                    ) == ManifestFileDecision::MetadataUnchanged
                {
                    phase_profile.add_duration("metadata_diff", metadata_start.elapsed(), 1, 1);
                    summary.files_skipped += 1;
                    summary.files_metadata_unchanged += 1;
                    skipped_unchanged_files += 1;
                    continue;
                }
                phase_profile.add_duration("metadata_diff", metadata_start.elapsed(), 1, 1);

                if size_bytes > TEXT_EVIDENCE_MAX_READ_BYTES_PER_FILE {
                    summary.files_skipped += 1;
                    if existing_file.is_some() {
                        let delete_start = Instant::now();
                        store.delete_facts_for_file(&candidate.repo_relative_path)?;
                        db_write_ms += delete_start.elapsed().as_millis();
                        summary.files_deleted += 1;
                    }
                    record_index_issue(
                        &mut summary,
                        &options,
                        candidate.repo_relative_path,
                        "text_evidence_too_large",
                        format!(
                            "scoped non-parser text candidate is {} bytes, above {} byte Stage 0 cap",
                            size_bytes, TEXT_EVIDENCE_MAX_READ_BYTES_PER_FILE
                        ),
                        "skipped_text_evidence",
                    );
                    continue;
                }

                let read_start = Instant::now();
                let source = match fs::read_to_string(&candidate.file_path) {
                    Ok(source) => source,
                    Err(error) => {
                        phase_profile.add_duration("file_read", read_start.elapsed(), 1, 1);
                        summary.files_skipped += 1;
                        summary.failed_files_deleted += 1;
                        if existing_file.is_some() {
                            summary.files_deleted += 1;
                        }
                        let delete_start = Instant::now();
                        store.delete_facts_for_file(&candidate.repo_relative_path)?;
                        db_write_ms += delete_start.elapsed().as_millis();
                        record_index_issue(
                            &mut summary,
                            &options,
                            candidate.repo_relative_path,
                            "text_evidence_read_error",
                            error.to_string(),
                            "skipped_and_deleted_old_facts",
                        );
                        continue;
                    }
                };
                phase_profile.add_duration(
                    "file_read",
                    read_start.elapsed(),
                    1,
                    source.len() as u64,
                );
                summary.files_read += 1;

                if !looks_like_text_evidence_source(&source) {
                    summary.files_skipped += 1;
                    if existing_file.is_some() {
                        let delete_start = Instant::now();
                        store.delete_facts_for_file(&candidate.repo_relative_path)?;
                        db_write_ms += delete_start.elapsed().as_millis();
                        summary.files_deleted += 1;
                    }
                    record_index_issue(
                        &mut summary,
                        &options,
                        candidate.repo_relative_path,
                        "text_evidence_not_text_like",
                        "scoped non-parser candidate was not text-like".to_string(),
                        "skipped_text_evidence",
                    );
                    continue;
                }

                let hash_start = Instant::now();
                let hash = content_hash(&source);
                phase_profile.add_duration(
                    "file_hash",
                    hash_start.elapsed(),
                    1,
                    source.len() as u64,
                );
                summary.files_hashed += 1;
                append_candidate_spool_chunks(
                    &mut summary,
                    candidate_spool_chunks_for_text_evidence(
                        &candidate.repo_relative_path,
                        &source,
                        &hash,
                        candidate.kind.as_str(),
                        size_bytes,
                        modified_unix_nanos(&file_metadata).as_deref(),
                    )?,
                )?;

                if let Some(record) = existing_file
                    .as_ref()
                    .filter(|record| record.file_hash == hash && existing_is_text_evidence)
                {
                    let refresh_start = Instant::now();
                    let metadata = text_evidence_file_metadata(
                        modified_unix_nanos(&file_metadata),
                        candidate.kind,
                        &build_text_evidence_index(&candidate.repo_relative_path, &source),
                    );
                    store.upsert_file(&FileRecord {
                        repo_relative_path: candidate.repo_relative_path.clone(),
                        file_hash: record.file_hash.clone(),
                        language: None,
                        size_bytes,
                        indexed_at_unix_ms: Some(indexed_at),
                        metadata,
                    })?;
                    db_write_ms += refresh_start.elapsed().as_millis();
                    phase_profile.add_duration(
                        "file_manifest_refresh",
                        refresh_start.elapsed(),
                        1,
                        1,
                    );
                    summary.files_skipped += 1;
                    skipped_unchanged_files += 1;
                    continue;
                }

                if existing_file.is_none() {
                    summary.files_renamed += manifest_diff.record_rename_matches(
                        &repo_root,
                        &candidate.repo_relative_path,
                        &hash,
                    );
                }

                let evidence_index =
                    build_text_evidence_index(&candidate.repo_relative_path, &source);
                let write_start = Instant::now();
                persist_text_evidence_to_writer(
                    &store,
                    &candidate.repo_relative_path,
                    &hash,
                    candidate.kind,
                    &evidence_index,
                    size_bytes,
                    indexed_at,
                    modified_unix_nanos(&file_metadata),
                    existing_file.is_some(),
                )?;
                db_write_ms += write_start.elapsed().as_millis();
                phase_profile.add_duration("text_evidence_upsert", write_start.elapsed(), 1, 1);
                summary.files_indexed += 1;
            }
            Ok(())
        })();
        match text_evidence_result {
            Ok(()) => {
                store.commit_write_transaction()?;
                phase_profile.add_duration(
                    "text_evidence_transaction",
                    text_evidence_transaction_start.elapsed(),
                    1,
                    summary.files_indexed as u64,
                );
            }
            Err(error) => {
                let _ = store.rollback_write_transaction();
                return Err(error);
            }
        }
    }

    for (file_path, repo_relative_path) in source_candidates {
        let metadata_start = Instant::now();
        let file_metadata = fs::metadata(&file_path)?;
        let size_bytes = file_metadata.len();
        let existing_file = manifest_diff.existing_file(&repo_relative_path).cloned();
        if manifest_diff.classify_file(&repo_relative_path, size_bytes, &file_metadata)
            == ManifestFileDecision::MetadataUnchanged
        {
            phase_profile.add_duration("metadata_diff", metadata_start.elapsed(), 1, 1);
            summary.files_skipped += 1;
            summary.files_metadata_unchanged += 1;
            skipped_unchanged_files += 1;
            continue;
        }
        phase_profile.add_duration("metadata_diff", metadata_start.elapsed(), 1, 1);
        let read_start = Instant::now();
        let source = match fs::read_to_string(&file_path) {
            Ok(source) => source,
            Err(error) => {
                phase_profile.add_duration("file_read", read_start.elapsed(), 1, 1);
                summary.files_skipped += 1;
                summary.failed_files_deleted += 1;
                if existing_file.is_some() {
                    summary.files_deleted += 1;
                }
                let delete_start = Instant::now();
                store.transaction(|tx| tx.delete_facts_for_file(&repo_relative_path))?;
                db_write_ms += delete_start.elapsed().as_millis();
                record_index_issue(
                    &mut summary,
                    &options,
                    repo_relative_path,
                    "read_error",
                    error.to_string(),
                    "skipped_and_deleted_old_facts",
                );
                continue;
            }
        };
        phase_profile.add_duration("file_read", read_start.elapsed(), 1, source.len() as u64);
        summary.files_read += 1;
        let hash_start = Instant::now();
        let hash = content_hash(&source);
        phase_profile.add_duration("file_hash", hash_start.elapsed(), 1, source.len() as u64);
        summary.files_hashed += 1;
        let language = detect_language(&file_path).map(|language| language.as_str().to_string());
        let needs_delete = existing_file.is_some();
        if let Some(record) = existing_file
            .as_ref()
            .filter(|record| record.file_hash == hash)
        {
            let refresh_start = Instant::now();
            store.transaction(|tx| {
                tx.upsert_file(&FileRecord {
                    repo_relative_path: repo_relative_path.clone(),
                    file_hash: record.file_hash.clone(),
                    language,
                    size_bytes,
                    indexed_at_unix_ms: Some(indexed_at),
                    metadata: file_manifest_metadata(modified_unix_nanos(&file_metadata)),
                })
            })?;
            db_write_ms += refresh_start.elapsed().as_millis();
            phase_profile.add_duration("file_manifest_refresh", refresh_start.elapsed(), 1, 1);
            summary.files_skipped += 1;
            skipped_unchanged_files += 1;
            continue;
        }
        if existing_file.is_none() {
            summary.files_renamed +=
                manifest_diff.record_rename_matches(&repo_root, &repo_relative_path, &hash);
        }

        hashed_candidates.push(HashedIndexCandidate {
            repo_relative_path,
            source,
            file_hash: hash,
            language,
            size_bytes,
            modified_unix_nanos: modified_unix_nanos(&file_metadata),
            needs_delete,
        });
    }

    let dedupe_start = Instant::now();
    let mut content_template_paths = BTreeMap::<(String, Option<String>), String>::new();
    let mut duplicate_of_by_path = BTreeMap::<String, String>::new();
    let mut template_required_paths = BTreeSet::<String>::new();
    for candidate in &hashed_candidates {
        let template_key = (candidate.file_hash.clone(), candidate.language.clone());
        let duplicate_of = content_template_paths.get(&template_key).cloned();
        if let Some(canonical_path) = duplicate_of {
            duplicate_of_by_path
                .insert(candidate.repo_relative_path.clone(), canonical_path.clone());
            template_required_paths.insert(canonical_path);
        } else {
            content_template_paths.insert(template_key, candidate.repo_relative_path.clone());
        }
    }
    phase_profile.add_duration(
        "content_template_dedupe",
        dedupe_start.elapsed(),
        1,
        hashed_candidates.len() as u64,
    );

    let mut batch = PendingIndexBatch::default();
    for candidate in hashed_candidates {
        let source_bytes = candidate.source.len();
        let duplicate_of = duplicate_of_by_path
            .get(&candidate.repo_relative_path)
            .cloned();

        if duplicate_of.is_none() {
            summary.files_parsed += 1;
        }

        if should_start_new_index_batch(
            batch.files.len(),
            batch.source_bytes,
            source_bytes,
            DEFAULT_INDEX_BATCH_MAX_FILES,
            DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES,
        ) {
            let batch_profile = process_and_commit_index_batch(
                &store,
                &mut summary,
                &options,
                bulk_durability,
                std::mem::take(&mut batch),
                indexed_at,
                &mut bulk_index_load_started,
                &mut db_write_ms,
                &mut phase_profile,
            )?;
            parse_ms += batch_profile.parse_ms;
            extraction_ms += batch_profile.extraction_ms;
            db_write_ms += batch_profile.db_write_ms;
            max_worker_count = max_worker_count.max(batch_profile.worker_count);
        }

        batch.source_bytes += source_bytes;
        batch.files.push(PendingIndexFile {
            template_required: template_required_paths.contains(&candidate.repo_relative_path),
            repo_relative_path: candidate.repo_relative_path,
            source: candidate.source,
            file_hash: candidate.file_hash,
            language: candidate.language,
            size_bytes: candidate.size_bytes,
            modified_unix_nanos: candidate.modified_unix_nanos,
            needs_delete: candidate.needs_delete,
            duplicate_of,
        });

        if batch.files.len() >= DEFAULT_INDEX_BATCH_MAX_FILES
            || batch.source_bytes >= DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES
        {
            let batch_profile = process_and_commit_index_batch(
                &store,
                &mut summary,
                &options,
                bulk_durability,
                std::mem::take(&mut batch),
                indexed_at,
                &mut bulk_index_load_started,
                &mut db_write_ms,
                &mut phase_profile,
            )?;
            parse_ms += batch_profile.parse_ms;
            extraction_ms += batch_profile.extraction_ms;
            db_write_ms += batch_profile.db_write_ms;
            max_worker_count = max_worker_count.max(batch_profile.worker_count);
        }
    }

    if !batch.files.is_empty() {
        let batch_profile = process_and_commit_index_batch(
            &store,
            &mut summary,
            &options,
            bulk_durability,
            batch,
            indexed_at,
            &mut bulk_index_load_started,
            &mut db_write_ms,
            &mut phase_profile,
        )?;
        parse_ms += batch_profile.parse_ms;
        extraction_ms += batch_profile.extraction_ms;
        db_write_ms += batch_profile.db_write_ms;
        max_worker_count = max_worker_count.max(batch_profile.worker_count);
    }

    let stale_cleanup_paths = manifest_diff.stale_cleanup_paths();
    let post_local_start = Instant::now();
    let (
        stale_deleted,
        import_resolution,
        security_resolution,
        test_resolution,
        derived_resolution,
    ) = if !bulk_index_load_started && stale_cleanup_paths.is_empty() {
        store.transaction(|tx| {
            upsert_index_state_to_writer(tx, repo_root, indexed_at)?;
            tx.quick_integrity_gate()?;
            Ok(())
        })?;
        (
            0,
            GlobalFactApplySummary::default(),
            GlobalFactApplySummary::default(),
            GlobalFactApplySummary::default(),
            GlobalFactApplySummary::default(),
        )
    } else if bulk_index_load_started {
        let post_local_result = (|| -> Result<_, IndexError> {
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "stale_cleanup");
            let stale_deleted =
                delete_indexed_files_by_path_to_writer(&store, &stale_cleanup_paths)?;
            phase_profile.add_duration(
                "post_local_stale_cleanup",
                stage_start.elapsed(),
                1,
                stale_deleted as u64,
            );
            emit_post_local_stage_completed(
                &options,
                "stale_cleanup",
                stage_start.elapsed(),
                stale_deleted as u64,
            );
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "global_resolver_workspace_load");
            let resolver_workspace = GlobalResolverWorkspace::load(repo_root, &store)?;
            phase_profile.add_duration(
                "global_resolver_workspace_load",
                stage_start.elapsed(),
                1,
                resolver_workspace.resolver_paths.len() as u64,
            );
            emit_post_local_stage_completed(
                &options,
                "global_resolver_workspace_load",
                stage_start.elapsed(),
                resolver_workspace.resolver_paths.len() as u64,
            );
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "reduce_static_import_edges");
            let mut import_plan =
                reduce_static_import_edges_from_workspace(repo_root, &resolver_workspace)?;
            import_plan.sort();
            if let Some(hit) = import_plan.apply_reducer_edge_budget(
                "reduce_static_import_edges",
                options.graph_output_budgets.max_reducer_edges_per_stage,
            ) {
                record_graph_output_budget_hit(&mut summary, &options, hit);
            }
            phase_profile.add_duration(
                "reduce_static_import_edges",
                stage_start.elapsed(),
                1,
                import_plan.edges.len() as u64,
            );
            emit_post_local_stage_completed(
                &options,
                "reduce_static_import_edges",
                stage_start.elapsed(),
                import_plan.edges.len() as u64,
            );
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "reduce_security_edges");
            let mut security_plan =
                reduce_security_edges_from_workspace(repo_root, &resolver_workspace)?;
            security_plan.sort();
            if let Some(hit) = security_plan.apply_reducer_edge_budget(
                "reduce_security_edges",
                options.graph_output_budgets.max_reducer_edges_per_stage,
            ) {
                record_graph_output_budget_hit(&mut summary, &options, hit);
            }
            phase_profile.add_duration(
                "reduce_security_edges",
                stage_start.elapsed(),
                1,
                security_plan.edges.len() as u64,
            );
            emit_post_local_stage_completed(
                &options,
                "reduce_security_edges",
                stage_start.elapsed(),
                security_plan.edges.len() as u64,
            );
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "reduce_test_edges");
            let mut test_plan = reduce_test_edges_from_workspace(repo_root, &resolver_workspace)?;
            test_plan.sort();
            if let Some(hit) = test_plan.apply_reducer_edge_budget(
                "reduce_test_edges",
                options.graph_output_budgets.max_reducer_edges_per_stage,
            ) {
                record_graph_output_budget_hit(&mut summary, &options, hit);
            }
            phase_profile.add_duration(
                "reduce_test_edges",
                stage_start.elapsed(),
                1,
                test_plan.edges.len() as u64,
            );
            emit_post_local_stage_completed(
                &options,
                "reduce_test_edges",
                stage_start.elapsed(),
                test_plan.edges.len() as u64,
            );
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "apply_import_edges");
            let import_resolution = apply_global_fact_reduction_plan_to_writer(
                &store,
                &import_plan,
                &options,
                &mut phase_profile,
            )?;
            phase_profile.add_duration(
                "apply_import_edges",
                stage_start.elapsed(),
                1,
                import_resolution.edges_inserted as u64,
            );
            emit_post_local_stage_completed(
                &options,
                "apply_import_edges",
                stage_start.elapsed(),
                import_resolution.edges_inserted as u64,
            );
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "apply_security_edges");
            let security_resolution = apply_global_fact_reduction_plan_to_writer(
                &store,
                &security_plan,
                &options,
                &mut phase_profile,
            )?;
            phase_profile.add_duration(
                "apply_security_edges",
                stage_start.elapsed(),
                1,
                security_resolution.edges_inserted as u64,
            );
            emit_post_local_stage_completed(
                &options,
                "apply_security_edges",
                stage_start.elapsed(),
                security_resolution.edges_inserted as u64,
            );
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "apply_test_edges");
            let test_resolution = apply_global_fact_reduction_plan_to_writer(
                &store,
                &test_plan,
                &options,
                &mut phase_profile,
            )?;
            phase_profile.add_duration(
                "apply_test_edges",
                stage_start.elapsed(),
                1,
                test_resolution.edges_inserted as u64,
            );
            emit_post_local_stage_completed(
                &options,
                "apply_test_edges",
                stage_start.elapsed(),
                test_resolution.edges_inserted as u64,
            );
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "reduce_derived_mutation_edges");
            let mut derived_plan = reduce_derived_mutation_edges_from_store(&store)?;
            derived_plan.sort();
            if let Some(hit) = derived_plan.apply_reducer_edge_budget(
                "reduce_derived_mutation_edges",
                options.graph_output_budgets.max_reducer_edges_per_stage,
            ) {
                record_graph_output_budget_hit(&mut summary, &options, hit);
            }
            phase_profile.add_duration(
                "reduce_derived_mutation_edges",
                stage_start.elapsed(),
                1,
                derived_plan.edges.len() as u64,
            );
            emit_post_local_stage_completed(
                &options,
                "reduce_derived_mutation_edges",
                stage_start.elapsed(),
                derived_plan.edges.len() as u64,
            );
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "apply_derived_edges");
            let derived_resolution = apply_global_fact_reduction_plan_to_writer(
                &store,
                &derived_plan,
                &options,
                &mut phase_profile,
            )?;
            phase_profile.add_duration(
                "apply_derived_edges",
                stage_start.elapsed(),
                1,
                derived_resolution.edges_inserted as u64,
            );
            emit_post_local_stage_completed(
                &options,
                "apply_derived_edges",
                stage_start.elapsed(),
                derived_resolution.edges_inserted as u64,
            );
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "refresh_path_evidence");
            refresh_stored_path_evidence_to_writer(
                &store,
                DEFAULT_STORED_PATH_EVIDENCE_MAX_ROWS,
                &mut phase_profile,
            )?;
            phase_profile.add_duration("refresh_path_evidence", stage_start.elapsed(), 1, 0);
            emit_post_local_stage_completed(
                &options,
                "refresh_path_evidence",
                stage_start.elapsed(),
                0,
            );
            let stage_start = Instant::now();
            emit_post_local_stage_started(&options, "upsert_index_state");
            upsert_index_state_to_writer(&store, repo_root, indexed_at)?;
            phase_profile.add_duration("upsert_index_state", stage_start.elapsed(), 1, 0);
            emit_post_local_stage_completed(
                &options,
                "upsert_index_state",
                stage_start.elapsed(),
                0,
            );
            if !(bulk_durability == BulkIndexLoadDurability::HiddenAtomicColdTemp
                && options.build_mode == IndexBuildMode::ProofBuildOnly)
            {
                store.quick_integrity_gate()?;
            }
            Ok((
                stale_deleted,
                import_resolution,
                security_resolution,
                test_resolution,
                derived_resolution,
            ))
        })();
        match post_local_result {
            Ok(result) => {
                emit_index_progress(
                    &options,
                    json!({
                        "event": "transaction_commit_started",
                        "durability_status": "commit_starting",
                        "visible_db_mutation_claim": if bulk_durability == BulkIndexLoadDurability::HiddenAtomicColdTemp {
                            "hidden_temp_db_commit_not_visible_until_publish"
                        } else {
                            "visible_db_transaction_commit_starting"
                        },
                    }),
                );
                let commit_start = Instant::now();
                if let Err(error) = store.commit_bulk_index_transaction() {
                    let _ = store.rollback_bulk_index_transaction();
                    return Err(error.into());
                }
                phase_profile.add_duration("transaction_commit", commit_start.elapsed(), 1, 0);
                db_write_ms += commit_start.elapsed().as_millis();
                emit_index_progress(
                    &options,
                    json!({
                        "event": "transaction_commit_completed",
                        "elapsed_ms": commit_start.elapsed().as_millis(),
                        "durability_status": if bulk_durability == BulkIndexLoadDurability::HiddenAtomicColdTemp {
                            "committed_to_hidden_temp_db"
                        } else {
                            "committed_to_visible_db"
                        },
                        "visible_db_mutation_claim": if bulk_durability == BulkIndexLoadDurability::HiddenAtomicColdTemp {
                            "not_visible_until_atomic_publish"
                        } else {
                            "visible_db_updated_by_committed_transaction"
                        },
                    }),
                );
                result
            }
            Err(error) => {
                let _ = store.rollback_bulk_index_transaction();
                return Err(error);
            }
        }
    } else {
        let stale_deleted = delete_indexed_files_by_path(&store, &stale_cleanup_paths)?;
        let mut import_plan = reduce_static_import_edges_from_store(repo_root, &store)?;
        import_plan.sort();
        if let Some(hit) = import_plan.apply_reducer_edge_budget(
            "reduce_static_import_edges",
            options.graph_output_budgets.max_reducer_edges_per_stage,
        ) {
            record_graph_output_budget_hit(&mut summary, &options, hit);
        }
        let mut security_plan = reduce_security_edges_from_store(repo_root, &store)?;
        security_plan.sort();
        if let Some(hit) = security_plan.apply_reducer_edge_budget(
            "reduce_security_edges",
            options.graph_output_budgets.max_reducer_edges_per_stage,
        ) {
            record_graph_output_budget_hit(&mut summary, &options, hit);
        }
        let mut test_plan = reduce_test_edges_from_store(repo_root, &store)?;
        test_plan.sort();
        if let Some(hit) = test_plan.apply_reducer_edge_budget(
            "reduce_test_edges",
            options.graph_output_budgets.max_reducer_edges_per_stage,
        ) {
            record_graph_output_budget_hit(&mut summary, &options, hit);
        }
        emit_index_progress(
            &options,
            json!({
                "event": "transaction_commit_started",
                "durability_status": "commit_starting",
                "visible_db_mutation_claim": "visible_db_transaction_commit_starting",
            }),
        );
        let transaction_start = Instant::now();
        let (import_resolution, security_resolution, test_resolution, derived_resolution) =
            store.transaction(|tx| {
                let import_resolution = apply_global_fact_reduction_plan_to_writer(
                    tx,
                    &import_plan,
                    &options,
                    &mut phase_profile,
                )?;
                let security_resolution = apply_global_fact_reduction_plan_to_writer(
                    tx,
                    &security_plan,
                    &options,
                    &mut phase_profile,
                )?;
                let test_resolution = apply_global_fact_reduction_plan_to_writer(
                    tx,
                    &test_plan,
                    &options,
                    &mut phase_profile,
                )?;
                let mut derived_plan = reduce_derived_mutation_edges_from_store(tx)
                    .map_err(index_error_as_store_error)?;
                derived_plan.sort();
                if let Some(hit) = derived_plan.apply_reducer_edge_budget(
                    "reduce_derived_mutation_edges",
                    options.graph_output_budgets.max_reducer_edges_per_stage,
                ) {
                    record_graph_output_budget_hit(&mut summary, &options, hit);
                }
                let derived_resolution = apply_global_fact_reduction_plan_to_writer(
                    tx,
                    &derived_plan,
                    &options,
                    &mut phase_profile,
                )?;
                refresh_stored_path_evidence_to_writer(
                    tx,
                    DEFAULT_STORED_PATH_EVIDENCE_MAX_ROWS,
                    &mut phase_profile,
                )?;
                upsert_index_state_to_writer(tx, repo_root, indexed_at)?;
                tx.quick_integrity_gate()?;
                Ok((
                    import_resolution,
                    security_resolution,
                    test_resolution,
                    derived_resolution,
                ))
            })?;
        emit_index_progress(
            &options,
            json!({
                "event": "transaction_commit_completed",
                "elapsed_ms": transaction_start.elapsed().as_millis(),
                "durability_status": "committed_to_visible_db",
                "visible_db_mutation_claim": "visible_db_updated_by_committed_transaction",
            }),
        );
        (
            stale_deleted,
            import_resolution,
            security_resolution,
            test_resolution,
            derived_resolution,
        )
    };
    db_write_ms += post_local_start.elapsed().as_millis();
    summary.stale_files_deleted = stale_deleted;
    summary.files_deleted += summary.stale_files_deleted;
    summary.entities += import_resolution.entities_inserted;
    summary.edges += import_resolution.edges_inserted;
    summary.duplicate_edges_upserted += import_resolution.edges_upserted_existing;
    summary.entities += security_resolution.entities_inserted;
    summary.edges += security_resolution.edges_inserted;
    summary.duplicate_edges_upserted += security_resolution.edges_upserted_existing;
    summary.entities += test_resolution.entities_inserted;
    summary.edges += test_resolution.edges_inserted;
    summary.duplicate_edges_upserted += test_resolution.edges_upserted_existing;
    summary.entities += derived_resolution.entities_inserted;
    summary.edges += derived_resolution.edges_inserted;
    summary.duplicate_edges_upserted += derived_resolution.edges_upserted_existing;

    if bulk_index_load_started {
        let index_finish_start = Instant::now();
        emit_index_progress(
            &options,
            json!({
                "event": "fts_build_started",
                "stage": "finish_bulk_index_load",
                "durability_status": "post_commit_index_build_started",
            }),
        );
        // Recreate default indexes after all local and global facts are visible.
        // Production proof-build-only keeps publish-time maintenance light; the
        // validation build mode retains ANALYZE/checkpoint-heavy verification.
        match options.build_mode {
            IndexBuildMode::ProofBuildOnly => store.finish_bulk_index_load_fast()?,
            IndexBuildMode::ProofBuildPlusValidation => store.finish_bulk_index_load()?,
        }
        db_write_ms += index_finish_start.elapsed().as_millis();
        phase_profile.add_duration("index_creation", index_finish_start.elapsed(), 1, 0);
        phase_profile.add_duration("fts_build", index_finish_start.elapsed(), 1, 0);
        emit_index_progress(
            &options,
            json!({
                "event": "fts_built",
                "elapsed_ms": index_finish_start.elapsed().as_millis(),
                "durability_status": if bulk_durability == BulkIndexLoadDurability::HiddenAtomicColdTemp {
                    "built_in_hidden_temp_db"
                } else {
                    "built_in_visible_db"
                },
            }),
        );

        let reconciliation_start = Instant::now();
        summary.entities = usize::try_from(store.count_entities()?).unwrap_or(usize::MAX);
        summary.edges = usize::try_from(store.count_edges()?).unwrap_or(usize::MAX);
        phase_profile.add_duration(
            "summary_count_reconciliation",
            reconciliation_start.elapsed(),
            1,
            (summary.entities + summary.edges) as u64,
        );
    }

    phase_profile.extend_sqlite_profile();
    if options.profile {
        let legacy_db_write_bucket_ms = db_write_ms;
        phase_profile.add_ms(
            "legacy_db_write_aggregate",
            legacy_db_write_bucket_ms as f64,
            1,
            0,
        );
        let total_wall_ms = total_start.elapsed().as_millis();
        let measured_db_write_ms = phase_profile.sum_spans_ms_u128(DB_WRITE_PROFILE_SPANS);
        let measured_fts_search_index_ms = phase_profile.span_ms_u128("fts_build");
        let memory_bytes = current_process_memory_bytes();
        summary.profile = Some(IndexProfile {
            file_discovery_ms,
            parse_ms,
            extraction_ms,
            semantic_resolver_ms: phase_profile.span_ms_u128("reducer"),
            db_write_ms: measured_db_write_ms,
            fts_search_index_ms: measured_fts_search_index_ms,
            vector_signature_ms: 0,
            total_wall_ms,
            files_per_sec: rate_per_second(summary.files_indexed, total_wall_ms),
            entities_per_sec: rate_per_second(summary.entities, total_wall_ms),
            edges_per_sec: rate_per_second(summary.edges, total_wall_ms),
            memory_bytes,
            memory_measured: memory_bytes.is_some(),
            memory_status: if memory_bytes.is_some() {
                "measured".to_string()
            } else {
                "unknown".to_string()
            },
            memory_measurement_kind: if memory_bytes.is_some() {
                "process_snapshot_not_peak".to_string()
            } else {
                "not_measured".to_string()
            },
            db_write_measurement: "measured_sql_write_aggregate".to_string(),
            fts_search_index_measurement: if measured_fts_search_index_ms > 0 {
                "measured_fts_build_span".to_string()
            } else {
                "unknown_or_not_run".to_string()
            },
            worker_count: max_worker_count,
            skipped_unchanged_files,
            spans: phase_profile.clone().into_spans(),
        });
    }

    let checkpoint_start = Instant::now();
    store.wal_checkpoint_truncate()?;
    phase_profile.add_duration("wal_checkpoint", checkpoint_start.elapsed(), 1, 0);
    let post_check_start = Instant::now();
    run_post_index_check(&store, post_check)?;
    if post_check != PostIndexCheck::None {
        phase_profile.add_duration(
            post_index_check_span_name(post_check),
            post_check_start.elapsed(),
            1,
            0,
        );
    }

    let passport_start = Instant::now();
    let passport = build_db_passport(&store, repo_root, &options, indexed_at, &summary, "ok")?;
    store.upsert_db_passport(&passport)?;
    phase_profile.add_duration("upsert_db_passport", passport_start.elapsed(), 1, 1);
    if bulk_durability == BulkIndexLoadDurability::HiddenAtomicColdTemp {
        mark_candidate_spool_final(&mut summary, repo_root, db_path, &options)?;
    } else {
        mark_candidate_spool_superseded(&mut summary, repo_root, db_path, &options, &passport)?;
    }

    if options.profile {
        if let Some(profile) = &mut summary.profile {
            profile.spans = phase_profile.into_spans();
        }
    }

    emit_index_progress(
        &options,
        json!({
            "event": "index_completed",
            "repo_root": summary.repo_root.clone(),
            "db_path": summary.db_path.clone(),
            "batches_completed": summary.batches_completed,
            "files_indexed": summary.files_indexed,
            "files_skipped": summary.files_skipped,
            "parse_errors": summary.parse_errors,
            "syntax_errors": summary.syntax_errors,
            "issues": summary.issues.len(),
        }),
    );

    Ok(summary)
}

fn index_repo_to_atomic_cold_db(
    repo_root: &Path,
    final_db_path: &Path,
    options: IndexOptions,
    temp_db_path_override: Option<PathBuf>,
) -> Result<IndexSummary, IndexError> {
    let atomic_start = Instant::now();
    let temp_db_path = temp_db_path_override.unwrap_or_else(|| atomic_temp_db_path(final_db_path));
    remove_sqlite_file_family(&temp_db_path)?;
    write_path_chaos_failpoint("cold_before_db_write")?;
    let build_mode = options.build_mode;
    let publish_check = options.build_mode.post_index_check();
    let spool_options = options.clone();
    let result = index_repo_to_existing_db_with_options(
        repo_root,
        &temp_db_path,
        options,
        PostIndexCheck::None,
        BulkIndexLoadDurability::HiddenAtomicColdTemp,
    );
    match result {
        Ok(mut summary) => {
            let mut published = false;
            let finalize_result = (|| -> Result<(), IndexError> {
                write_path_chaos_failpoint("cold_after_temp_db_write_before_validation")?;
                let temp_finalize_start = Instant::now();
                emit_index_progress(
                    &spool_options,
                    json!({
                        "event": "temp_db_validation_started",
                        "temp_db_path": path_string(&temp_db_path),
                        "final_db_path": path_string(final_db_path),
                        "durability_status": "hidden_temp_db_committed_not_published",
                        "visible_db_mutation_claim": "old_good_db_still_visible_until_publish",
                    }),
                );
                {
                    let temp_store = SqliteGraphStore::open(&temp_db_path)?;
                    let checkpoint_start = Instant::now();
                    temp_store.wal_checkpoint_truncate()?;
                    add_profile_span_to_summary(
                        &mut summary,
                        "wal_checkpoint",
                        checkpoint_start.elapsed(),
                        1,
                        0,
                        "atomic cold temp DB checkpoint before final integrity gate",
                    );
                    let publish_check_start = Instant::now();
                    run_post_index_check(&temp_store, publish_check)?;
                    add_profile_span_to_summary(
                        &mut summary,
                        post_index_check_span_name(publish_check),
                        publish_check_start.elapsed(),
                        1,
                        0,
                        match publish_check {
                            PostIndexCheck::Full => {
                                "atomic cold temp DB full integrity gate before replacement"
                            }
                            PostIndexCheck::Quick => {
                                "atomic cold temp DB quick gate before replacement"
                            }
                            PostIndexCheck::None => "atomic cold temp DB publish gate skipped",
                        },
                    );
                }
                add_profile_span_to_summary(
                    &mut summary,
                    "atomic_temp_db_finalize",
                    temp_finalize_start.elapsed(),
                    1,
                    0,
                    "checkpoint and integrity gate on hidden temp DB before visible replacement",
                );
                emit_index_progress(
                    &spool_options,
                    json!({
                        "event": "temp_db_validation_completed",
                        "elapsed_ms": temp_finalize_start.elapsed().as_millis(),
                        "temp_db_path": path_string(&temp_db_path),
                        "durability_status": "hidden_temp_db_validated_not_published",
                        "visible_db_mutation_claim": "old_good_db_still_visible_until_publish",
                    }),
                );
                write_path_chaos_failpoint("cold_after_validation_before_publish")?;
                emit_index_progress(
                    &spool_options,
                    json!({
                        "event": "publish_started",
                        "temp_db_path": path_string(&temp_db_path),
                        "final_db_path": path_string(final_db_path),
                        "durability_status": "atomic_publish_starting",
                        "visible_db_mutation_claim": "old_good_db_visible_until_rename_succeeds",
                    }),
                );
                let replace_start = Instant::now();
                publish_atomic_sqlite_db(&temp_db_path, final_db_path)?;
                published = true;
                add_profile_span_to_summary(
                    &mut summary,
                    "atomic_db_replace",
                    replace_start.elapsed(),
                    1,
                    0,
                    "swap validated temp DB into place with old DB rollback on publish failure",
                );
                add_profile_span_to_summary(
                    &mut summary,
                    "artifact_publish_rename",
                    replace_start.elapsed(),
                    1,
                    0,
                    "production artifact publish is the atomic DB replace step",
                );
                emit_index_progress(
                    &spool_options,
                    json!({
                        "event": "temp_db_published",
                        "elapsed_ms": replace_start.elapsed().as_millis(),
                        "final_db_path": path_string(final_db_path),
                        "durability_status": "published_to_visible_db",
                        "visible_db_updated": true,
                        "visible_db_mutation_claim": "visible_db_updated_after_atomic_publish",
                    }),
                );
                emit_index_progress(
                    &spool_options,
                    json!({
                        "event": "visible_db_updated",
                        "final_db_path": path_string(final_db_path),
                        "durability_status": "published_to_visible_db",
                        "claimability": "pending_final_status_check",
                    }),
                );
                write_path_chaos_failpoint("cold_after_publish_before_final_status")?;
                let final_finalize_start = Instant::now();
                {
                    let final_store = SqliteGraphStore::open(final_db_path)?;
                    if build_mode == IndexBuildMode::ProofBuildOnly
                        && publish_check == PostIndexCheck::Quick
                    {
                        add_profile_span_to_summary(
                            &mut summary,
                            "post_index_check_skipped",
                            Duration::ZERO,
                            1,
                            0,
                            "final proof-build-only quick check skipped after atomic rename; hidden temp DB already passed quick_check before visible replacement",
                        );
                    } else {
                        let publish_check_start = Instant::now();
                        run_post_index_check(&final_store, publish_check)?;
                        add_profile_span_to_summary(
                            &mut summary,
                            post_index_check_span_name(publish_check),
                            publish_check_start.elapsed(),
                            1,
                            0,
                            match publish_check {
                                PostIndexCheck::Full => {
                                    "atomic cold final DB full integrity gate after replacement"
                                }
                                PostIndexCheck::Quick => {
                                    "atomic cold final DB quick gate after replacement"
                                }
                                PostIndexCheck::None => "atomic cold final DB publish gate skipped",
                            },
                        );
                    }
                    let checkpoint_start = Instant::now();
                    final_store.wal_checkpoint_truncate()?;
                    add_profile_span_to_summary(
                        &mut summary,
                        "wal_checkpoint",
                        checkpoint_start.elapsed(),
                        1,
                        0,
                        "atomic cold final DB checkpoint after replacement",
                    );
                    if let Some(passport) = final_store.get_db_passport()? {
                        mark_candidate_spool_superseded(
                            &mut summary,
                            repo_root,
                            final_db_path,
                            &spool_options,
                            &passport,
                        )?;
                    }
                }
                add_profile_span_to_summary(
                    &mut summary,
                    "atomic_final_db_validate",
                    final_finalize_start.elapsed(),
                    1,
                    0,
                    "open, configured publish gate, and checkpoint after visible replacement",
                );
                Ok(())
            })();
            if let Err(error) = finalize_result {
                if !published {
                    let _ = remove_sqlite_file_family(&temp_db_path);
                }
                return Err(error);
            }
            summary.db_path = path_string(final_db_path);
            if let Some(profile) = &mut summary.profile {
                profile.total_wall_ms = atomic_start.elapsed().as_millis();
            }
            Ok(summary)
        }
        Err(error) => {
            let _ = remove_sqlite_file_family(&temp_db_path);
            Err(error)
        }
    }
}

fn run_post_index_check(store: &SqliteGraphStore, check: PostIndexCheck) -> Result<(), IndexError> {
    match check {
        PostIndexCheck::None => {}
        PostIndexCheck::Full => store.full_integrity_gate()?,
        PostIndexCheck::Quick => store.quick_integrity_gate()?,
    }
    Ok(())
}

fn post_index_check_span_name(check: PostIndexCheck) -> &'static str {
    match check {
        PostIndexCheck::None => "post_index_check_skipped",
        PostIndexCheck::Full => "integrity_check",
        PostIndexCheck::Quick => "quick_check",
    }
}

fn atomic_temp_db_path(final_db_path: &Path) -> PathBuf {
    let parent = final_db_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = final_db_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("codegraph.sqlite");
    parent.join(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        unix_time_ms()
    ))
}

fn atomic_backup_db_path(final_db_path: &Path) -> PathBuf {
    let parent = final_db_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = final_db_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("codegraph.sqlite");
    parent.join(format!(
        ".{file_name}.backup-{}-{}",
        std::process::id(),
        unix_time_ms()
    ))
}

fn publish_atomic_sqlite_db(temp_db_path: &Path, final_db_path: &Path) -> Result<(), IndexError> {
    let backup_db_path = atomic_backup_db_path(final_db_path);
    let had_old_db = final_db_path.exists();
    if write_path_chaos_failpoint_enabled("cold_permission_denied_publish_dir") {
        return Err(IndexError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "chaos_failpoint:cold_permission_denied_publish_dir",
        )));
    }
    if had_old_db {
        if let Err(error) = rename_sqlite_file_family(final_db_path, &backup_db_path) {
            let _ = rename_sqlite_file_family(&backup_db_path, final_db_path);
            return Err(error);
        }
    } else {
        remove_sqlite_sidecars(final_db_path)?;
    }
    if let Err(error) = write_path_chaos_failpoint("cold_during_publish") {
        if had_old_db {
            let _ = rename_sqlite_file_family(&backup_db_path, final_db_path);
        }
        return Err(error);
    }

    match fs::rename(temp_db_path, final_db_path) {
        Ok(()) => {
            remove_sqlite_sidecars(temp_db_path)?;
            if had_old_db {
                remove_sqlite_file_family(&backup_db_path)?;
            }
            Ok(())
        }
        Err(error) => {
            let _ = remove_sqlite_file_family(final_db_path);
            if had_old_db {
                let _ = rename_sqlite_file_family(&backup_db_path, final_db_path);
            }
            Err(IndexError::Io(error))
        }
    }
}

fn rename_sqlite_file_family(from: &Path, to: &Path) -> Result<(), IndexError> {
    rename_file_if_exists(from, to)?;
    rename_file_if_exists(
        &sqlite_sidecar_path(from, "wal"),
        &sqlite_sidecar_path(to, "wal"),
    )?;
    rename_file_if_exists(
        &sqlite_sidecar_path(from, "shm"),
        &sqlite_sidecar_path(to, "shm"),
    )?;
    Ok(())
}

fn remove_sqlite_file_family(path: &Path) -> Result<(), IndexError> {
    remove_file_if_exists(path)?;
    remove_sqlite_sidecars(path)
}

fn remove_sqlite_sidecars(path: &Path) -> Result<(), IndexError> {
    remove_file_if_exists(&sqlite_sidecar_path(path, "wal"))?;
    remove_file_if_exists(&sqlite_sidecar_path(path, "shm"))?;
    Ok(())
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}-{suffix}", path.display()))
}

fn remove_file_if_exists(path: &Path) -> Result<(), IndexError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(IndexError::Io(error)),
    }
}

fn rename_file_if_exists(from: &Path, to: &Path) -> Result<(), IndexError> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(IndexError::Io(error)),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct IndexBatchProfile {
    parse_ms: u128,
    extraction_ms: u128,
    bundle_ms: u128,
    db_write_ms: u128,
    worker_count: usize,
}

#[derive(Debug, Clone, Default)]
struct IndexPhaseRecorder {
    spans: BTreeMap<String, PhaseTiming>,
}

impl IndexPhaseRecorder {
    fn add_duration(
        &mut self,
        name: impl Into<String>,
        elapsed: std::time::Duration,
        count: u64,
        items: u64,
    ) {
        self.add_ms(name, elapsed.as_secs_f64() * 1_000.0, count, items);
    }

    fn add_ms(&mut self, name: impl Into<String>, elapsed_ms: f64, count: u64, items: u64) {
        let name = name.into();
        let entry = self
            .spans
            .entry(name.clone())
            .or_insert_with(|| PhaseTiming {
                name,
                elapsed_ms: 0.0,
                count: 0,
                items: 0,
                notes: Vec::new(),
            });
        entry.elapsed_ms += elapsed_ms;
        entry.count = entry.count.saturating_add(count);
        entry.items = entry.items.saturating_add(items);
    }

    fn add_note(&mut self, name: impl Into<String>, note: impl Into<String>) {
        let name = name.into();
        let entry = self
            .spans
            .entry(name.clone())
            .or_insert_with(|| PhaseTiming {
                name,
                elapsed_ms: 0.0,
                count: 0,
                items: 0,
                notes: Vec::new(),
            });
        let note = note.into();
        if !entry.notes.iter().any(|existing| existing == &note) {
            entry.notes.push(note);
        }
    }

    fn span_ms_u128(&self, name: &str) -> u128 {
        self.spans
            .get(name)
            .map(|span| span.elapsed_ms as u128)
            .unwrap_or(0)
    }

    fn sum_spans_ms_u128(&self, names: &[&str]) -> u128 {
        names
            .iter()
            .map(|name| self.span_ms_u128(name))
            .sum::<u128>()
    }

    fn extend_sqlite_profile(&mut self) {
        for span in take_sqlite_profile() {
            match span.name.as_str() {
                "open_connection" => self.add_ms("db_open_init", span.elapsed_ms, span.count, 0),
                "configure_pragmas" => self.add_ms("pragma_setup", span.elapsed_ms, span.count, 0),
                "migrate_schema" => {
                    self.add_ms("table_creation", span.elapsed_ms, span.count, 0);
                    self.add_ms("schema_migration", span.elapsed_ms, span.count, 0);
                }
                "entity_insert_sql" => {
                    self.add_ms(
                        "proof_entity_insert",
                        span.elapsed_ms,
                        span.count,
                        span.count,
                    );
                }
                "edge_insert_sql" => {
                    self.add_ms("proof_edge_insert", span.elapsed_ms, span.count, span.count);
                }
                "source_span_insert_sql" => {
                    self.add_ms(
                        "source_span_insert",
                        span.elapsed_ms,
                        span.count,
                        span.count,
                    );
                }
                "file_manifest_upsert_sql" => {
                    self.add_ms(
                        "file_template_entity_mapping_inserts",
                        span.elapsed_ms,
                        span.count,
                        span.count,
                    );
                }
                "path_evidence_upsert_sql" => {
                    self.add_ms(
                        "path_evidence_insert",
                        span.elapsed_ms,
                        span.count,
                        span.count,
                    );
                }
                "template_entity_insert_sql" => {
                    self.add_ms(
                        "template_entities_insert",
                        span.elapsed_ms,
                        span.count,
                        span.count,
                    );
                }
                "template_edge_insert_sql" => {
                    self.add_ms(
                        "template_edges_insert",
                        span.elapsed_ms,
                        span.count,
                        span.count,
                    );
                }
                _ => {}
            }
            if span.name == "dictionary_lookup_insert" {
                self.add_ms(
                    "dictionary_lookup_insert",
                    span.elapsed_ms,
                    span.count,
                    span.count,
                );
                self.add_ms("symbol_interning", span.elapsed_ms, span.count, span.count);
                self.add_note(
                    "symbol_interning",
                    "Symbol interning is measured through shared dictionary lookup/insert calls.",
                );
            }
            self.add_ms(
                format!("sqlite.{}", span.name),
                span.elapsed_ms,
                span.count,
                span.count,
            );
        }
    }

    fn into_spans(mut self) -> Vec<PhaseTiming> {
        for required in REQUIRED_PROFILE_SPANS {
            self.spans
                .entry((*required).to_string())
                .or_insert_with(|| PhaseTiming {
                    name: (*required).to_string(),
                    elapsed_ms: 0.0,
                    count: 0,
                    items: 0,
                    notes: Vec::new(),
                });
        }
        let mut spans = self.spans.into_values().collect::<Vec<_>>();
        spans.sort_by(|left, right| left.name.cmp(&right.name));
        spans
    }
}

fn add_profile_span_to_summary(
    summary: &mut IndexSummary,
    name: &str,
    elapsed: std::time::Duration,
    count: u64,
    items: u64,
    note: &str,
) {
    let Some(profile) = &mut summary.profile else {
        return;
    };
    let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
    if let Some(span) = profile.spans.iter_mut().find(|span| span.name == name) {
        span.elapsed_ms += elapsed_ms;
        span.count = span.count.saturating_add(count);
        span.items = span.items.saturating_add(items);
        if !note.is_empty() && !span.notes.iter().any(|existing| existing == note) {
            span.notes.push(note.to_string());
        }
        return;
    }
    profile.spans.push(PhaseTiming {
        name: name.to_string(),
        elapsed_ms,
        count,
        items,
        notes: if note.is_empty() {
            Vec::new()
        } else {
            vec![note.to_string()]
        },
    });
    profile
        .spans
        .sort_by(|left, right| left.name.cmp(&right.name));
}

const REQUIRED_PROFILE_SPANS: &[&str] = &[
    "repo_file_discovery",
    "manifest_load",
    "open_store",
    "db_open_init",
    "pragma_setup",
    "table_creation",
    "schema_migration",
    "file_walk",
    "metadata_diff",
    "file_read",
    "file_hash",
    "parse",
    "parse_extract_workers_wall",
    "parse_extract",
    "extract_entities_and_relations",
    "local_fact_bundle_creation",
    "content_template_dedupe",
    "reducer",
    "symbol_interning",
    "qname_prefix_interning",
    "qualified_name_interning",
    "dictionary_lookup_insert",
    "dictionary_batch_preparation",
    "template_entities_preparation",
    "template_edges_preparation",
    "proof_entities_preparation",
    "proof_edges_preparation",
    "source_spans_preparation",
    "path_evidence_preparation",
    "entity_insert",
    "proof_entity_insert",
    "edge_insert",
    "proof_edge_insert",
    "source_span_insert",
    "path_evidence_generation",
    "path_evidence_insert",
    "path_evidence_edges_insert",
    "path_evidence_symbols_insert",
    "path_evidence_tests_insert",
    "file_template_entity_mapping_inserts",
    "template_entities_insert",
    "template_edges_insert",
    "symbol_dict_insert",
    "qname_prefix_dict_insert",
    "qualified_name_dict_insert",
    "fts_build",
    "index_creation",
    "index_creation_by_name",
    "transaction_commit",
    "summary_count_reconciliation",
    "graph_fact_hash",
    "quick_check",
    "post_index_check_skipped",
    "integrity_check",
    "vacuum",
    "analyze",
    "storage_audit",
    "sql_query_execution",
    "snippet_loading",
    "json_serialization",
    "markdown_report_generation",
    "wal_checkpoint",
    "stale_fact_delete",
    "cache_refresh",
    "atomic_temp_db_finalize",
    "atomic_db_replace",
    "artifact_publish_rename",
    "atomic_final_db_validate",
    "storage_audit_skipped",
    "relation_sampler_skipped",
    "path_evidence_sampler_skipped",
    "cgc_comparison_skipped",
];

const DB_WRITE_PROFILE_SPANS: &[&str] = &[
    "stale_fact_delete",
    "file_manifest_upsert",
    "text_evidence_upsert",
    "entity_insert",
    "proof_entity_insert",
    "edge_insert",
    "proof_edge_insert",
    "source_span_insert",
    "path_evidence_insert",
    "path_evidence_edges_insert",
    "path_evidence_symbols_insert",
    "path_evidence_tests_insert",
    "file_template_entity_mapping_inserts",
    "template_entities_insert",
    "template_edges_insert",
    "symbol_dict_insert",
    "qname_prefix_dict_insert",
    "qualified_name_dict_insert",
    "dictionary_lookup_insert",
    "content_template_upsert",
    "upsert_index_state",
    "upsert_db_passport",
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PersistedBatchSummary {
    files: usize,
    entities: usize,
    edges: usize,
    duplicate_edges_upserted: usize,
}

fn local_fact_relation(edge: &Edge) -> LocalFactRelation {
    LocalFactRelation {
        edge_id: edge.id.clone(),
        relation: edge.relation,
        head_id: edge.head_id.clone(),
        tail_id: edge.tail_id.clone(),
        source_span: edge.source_span.clone(),
        exactness: edge.exactness,
        derived: edge.derived,
        extractor: edge.extractor.clone(),
    }
}

fn unresolved_reference_for_edge(edge: &Edge) -> Option<LocalFactReference> {
    let unresolved_tail = edge.tail_id.contains("static_reference:")
        || edge.tail_id.contains("dynamic_import:")
        || edge.tail_id.contains("unresolved");
    let unresolved_head = edge.head_id.contains("static_reference:")
        || edge.head_id.contains("dynamic_import:")
        || edge.head_id.contains("unresolved");
    if edge.exactness != Exactness::StaticHeuristic && !unresolved_tail && !unresolved_head {
        return None;
    }
    let reference_id = if unresolved_tail {
        edge.tail_id.clone()
    } else if unresolved_head {
        edge.head_id.clone()
    } else {
        format!("{}:{}:{}", edge.head_id, edge.relation, edge.tail_id)
    };
    Some(LocalFactReference {
        name: reference_display_name(&reference_id),
        reference_id,
        relation: edge.relation,
        source_span: edge.source_span.clone(),
        exactness: edge.exactness,
        extractor: edge.extractor.clone(),
    })
}

fn reference_display_name(reference_id: &str) -> String {
    reference_id
        .rsplit(|character| character == '/' || character == ':')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(reference_id)
        .to_string()
}

fn push_unique_sorted(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
        values.sort();
    }
}

fn push_unique_span(spans: &mut Vec<SourceSpan>, span: SourceSpan) {
    if !spans.iter().any(|existing| existing == &span) {
        spans.push(span);
    }
}

pub fn graph_fact_hash(entities: &[Entity], edges: &[Edge]) -> String {
    let mut facts = Vec::new();
    for entity in entities {
        facts.push(canonical_entity_fact_line(entity));
    }
    for edge in edges {
        facts.push(canonical_edge_fact_line(edge));
    }
    facts.sort();
    content_hash(&facts.join("\n"))
}

fn canonical_entity_fact_line(entity: &Entity) -> String {
    format!(
        "entity|{}|{}|{}|{}|{}|{}|{}|{}",
        entity.id,
        entity.kind,
        entity.name,
        entity.qualified_name,
        normalize_graph_path(&entity.repo_relative_path),
        entity
            .source_span
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "none".to_string()),
        entity.created_from,
        entity.file_hash.as_deref().unwrap_or("")
    )
}

fn canonical_edge_fact_line(edge: &Edge) -> String {
    let mut provenance = edge.provenance_edges.clone();
    provenance.sort();
    format!(
        "edge|{}|{}|{}|{}|{}|{}|{:.6}|{}|{}|{}|{}|{}|{}",
        edge.id,
        edge.head_id,
        edge.relation,
        edge.tail_id,
        edge.source_span,
        edge.exactness,
        edge.confidence,
        edge.edge_class,
        edge.context,
        edge.derived,
        provenance.join(","),
        edge.extractor,
        edge.file_hash.as_deref().unwrap_or("")
    )
}

pub fn reduce_local_fact_bundles(mut bundles: Vec<LocalFactBundle>) -> ReducedIndexPlan {
    bundles.sort_by(|left, right| left.repo_relative_path.cmp(&right.repo_relative_path));
    let mut warnings = Vec::new();
    let mut symbol_table = PreliminarySymbolTable::default();
    let mut seen_entities = BTreeMap::<String, Entity>::new();
    let mut seen_edges = BTreeMap::<String, Edge>::new();

    for bundle in &mut bundles {
        bundle
            .declarations
            .sort_by(|left, right| left.id.cmp(&right.id));
        for symbol in &bundle.declarations {
            if let Some(conflict_id) =
                symbol_table.insert_declaration(&bundle.repo_relative_path, symbol.clone())
            {
                warnings.push(format!(
                    "{} conflicts with previously reduced symbol id {}",
                    bundle.repo_relative_path, conflict_id
                ));
            }
        }

        bundle
            .extraction
            .entities
            .sort_by(|left, right| left.id.cmp(&right.id));
        let mut entities_by_id = BTreeMap::<String, Entity>::new();
        for entity in std::mem::take(&mut bundle.extraction.entities) {
            if let Some(existing) = entities_by_id.get(&entity.id) {
                if existing != &entity {
                    warnings.push(format!(
                        "{} has conflicting local entity id {}",
                        bundle.repo_relative_path, entity.id
                    ));
                }
                continue;
            }
            if let Some(existing) = seen_entities.get(&entity.id) {
                if existing != &entity {
                    warnings.push(format!(
                        "{} conflicts with previously reduced entity id {}",
                        bundle.repo_relative_path, entity.id
                    ));
                }
            } else {
                seen_entities.insert(entity.id.clone(), entity.clone());
            }
            entities_by_id.insert(entity.id.clone(), entity);
        }
        bundle.extraction.entities = entities_by_id.into_values().collect();

        bundle
            .extraction
            .edges
            .sort_by(|left, right| left.id.cmp(&right.id));
        let mut edges_by_id = BTreeMap::<String, Edge>::new();
        for edge in std::mem::take(&mut bundle.extraction.edges) {
            if let Some(existing) = edges_by_id.get(&edge.id) {
                if existing != &edge {
                    warnings.push(format!(
                        "{} has conflicting local edge id {}",
                        bundle.repo_relative_path, edge.id
                    ));
                }
                continue;
            }
            if let Some(existing) = seen_edges.get(&edge.id) {
                if existing != &edge {
                    warnings.push(format!(
                        "{} conflicts with previously reduced edge id {}",
                        bundle.repo_relative_path, edge.id
                    ));
                }
            } else {
                seen_edges.insert(edge.id.clone(), edge.clone());
            }
            edges_by_id.insert(edge.id.clone(), edge);
        }
        bundle.extraction.edges = edges_by_id.into_values().collect();
    }

    let global_facts = reduce_static_import_edges_from_bundles(&bundles);

    ReducedIndexPlan {
        bundles,
        symbol_table,
        global_facts,
        warnings,
    }
}

fn reduce_static_import_edges_from_bundles(bundles: &[LocalFactBundle]) -> GlobalFactReductionPlan {
    let mut entities_by_file = BTreeMap::<String, Vec<Entity>>::new();
    let mut indexed_paths = BTreeSet::<String>::new();
    let mut file_hashes = BTreeMap::<String, String>::new();
    let mut sources = BTreeMap::<String, String>::new();
    let mut languages = BTreeMap::<String, Option<String>>::new();

    for bundle in bundles {
        let repo_relative_path = normalize_graph_path(&bundle.repo_relative_path);
        indexed_paths.insert(repo_relative_path.clone());
        file_hashes.insert(repo_relative_path.clone(), bundle.file_hash.clone());
        sources.insert(repo_relative_path.clone(), bundle.source.clone());
        languages.insert(repo_relative_path.clone(), bundle.language.clone());
        let mut entities = bundle.extraction.entities.clone();
        entities.sort_by(|left, right| left.id.cmp(&right.id));
        entities_by_file.insert(repo_relative_path, entities);
    }

    let mut plan = GlobalFactReductionPlan::default();
    for bundle in bundles {
        let importer_path = normalize_graph_path(&bundle.repo_relative_path);
        if !languages
            .get(&importer_path)
            .and_then(Option::as_deref)
            .is_some_and(|language| language == "typescript" || language == "javascript")
        {
            continue;
        }
        let Some(source) = sources.get(&importer_path) else {
            continue;
        };
        let file_hash = file_hashes
            .get(&importer_path)
            .map(String::as_str)
            .unwrap_or("");

        for spec in parse_static_imports(&importer_path, source) {
            let Some(target_path) =
                resolve_local_module_path(&importer_path, &spec.module_specifier, &indexed_paths)
            else {
                continue;
            };
            let Some(target) = resolve_import_target_from_bundles(
                &entities_by_file,
                &indexed_paths,
                &sources,
                &target_path,
                &spec.imported_name,
                spec.kind,
            ) else {
                continue;
            };
            let import_entity = import_alias_entity(&spec, file_hash);
            plan.push_entity(import_entity.clone(), GlobalEntityWriteMode::UpsertIndexed);

            plan.push_edge(resolved_import_edge(
                &target.id,
                RelationKind::AliasedBy,
                &import_entity.id,
                &spec.span,
                file_hash,
                "static_named_import_alias",
            ));

            if let Some(file_entity) = file_entity_for_path(&entities_by_file, &importer_path) {
                plan.push_edge(resolved_import_edge(
                    &file_entity.id,
                    RelationKind::Imports,
                    &target.id,
                    &spec.span,
                    file_hash,
                    "static_import_target",
                ));
            }

            for call_span in call_spans_for_local_name(source, &importer_path, &spec.local_name) {
                if local_declaration_shadows_import(
                    &entities_by_file,
                    &importer_path,
                    &spec.local_name,
                    &spec.span,
                    &call_span,
                ) {
                    continue;
                }
                let Some(scope) =
                    containing_executable(&entities_by_file, &importer_path, &call_span)
                else {
                    continue;
                };
                plan.push_edge(resolved_import_edge(
                    &scope.id,
                    RelationKind::Calls,
                    &target.id,
                    &call_span,
                    file_hash,
                    "static_import_call_target",
                ));
            }
        }

        for spec in parse_dynamic_imports(&importer_path, source) {
            let Some(scope) = containing_executable(&entities_by_file, &importer_path, &spec.span)
            else {
                continue;
            };
            let import_entity = dynamic_import_entity(&importer_path, &spec, file_hash);
            plan.push_entity(
                import_entity.clone(),
                GlobalEntityWriteMode::InsertRecordIfMissing,
            );
            plan.push_edge(unresolved_dynamic_import_edge(
                &scope.id,
                &import_entity.id,
                &spec.span,
                file_hash,
            ));
        }
    }
    plan.sort();
    plan
}

fn resolve_import_target_from_bundles(
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    indexed_paths: &BTreeSet<String>,
    sources: &BTreeMap<String, String>,
    target_path: &str,
    imported_name: &str,
    kind: StaticImportKind,
) -> Option<Entity> {
    resolve_import_target_from_bundles_with_depth(
        entities_by_file,
        indexed_paths,
        sources,
        target_path,
        imported_name,
        kind,
        0,
    )
}

fn resolve_import_target_from_bundles_with_depth(
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    indexed_paths: &BTreeSet<String>,
    sources: &BTreeMap<String, String>,
    target_path: &str,
    imported_name: &str,
    kind: StaticImportKind,
    depth: usize,
) -> Option<Entity> {
    if depth > 8 {
        return None;
    }
    if kind == StaticImportKind::Default {
        if let Some(target) = resolve_default_import_target(entities_by_file, target_path) {
            return Some(target);
        }
    } else if let Some(target) =
        resolve_named_import_target(entities_by_file, target_path, imported_name)
    {
        return Some(target);
    }

    let source = sources.get(target_path)?;
    for spec in parse_static_reexports(target_path, source) {
        if spec.exported_name != imported_name {
            continue;
        }
        let Some(reexport_target_path) =
            resolve_local_module_path(target_path, &spec.module_specifier, indexed_paths)
        else {
            continue;
        };
        if let Some(target) = resolve_import_target_from_bundles_with_depth(
            entities_by_file,
            indexed_paths,
            sources,
            &reexport_target_path,
            &spec.imported_name,
            spec.kind,
            depth + 1,
        ) {
            return Some(target);
        }
    }

    None
}

fn ensure_bulk_index_load(
    store: &SqliteGraphStore,
    options: &IndexOptions,
    bulk_durability: BulkIndexLoadDurability,
    started: &mut bool,
    db_write_ms: &mut u128,
    profile: &mut IndexPhaseRecorder,
) -> Result<(), IndexError> {
    if *started {
        return Ok(());
    }
    let start = Instant::now();
    if bulk_durability == BulkIndexLoadDurability::HiddenAtomicColdTemp
        && options.build_mode == IndexBuildMode::ProofBuildOnly
    {
        store.begin_atomic_cold_bulk_index_load()?;
        profile.add_duration("atomic_cold_bulk_pragmas", start.elapsed(), 1, 0);
    } else {
        store.begin_bulk_index_load()?;
    }
    store.begin_bulk_index_transaction()?;
    store.drop_bulk_index_lookup_indexes()?;
    *db_write_ms += start.elapsed().as_millis();
    profile.add_duration("index_creation", start.elapsed(), 1, 0);
    *started = true;
    Ok(())
}

fn commit_bulk_index_batch(
    _store: &SqliteGraphStore,
    db_write_ms: &mut u128,
) -> Result<(), IndexError> {
    let start = Instant::now();
    *db_write_ms += start.elapsed().as_millis();
    Ok(())
}

fn process_and_commit_index_batch(
    store: &SqliteGraphStore,
    summary: &mut IndexSummary,
    options: &IndexOptions,
    bulk_durability: BulkIndexLoadDurability,
    batch: PendingIndexBatch,
    indexed_at: u64,
    bulk_index_load_started: &mut bool,
    db_write_ms: &mut u128,
    profile: &mut IndexPhaseRecorder,
) -> Result<IndexBatchProfile, IndexError> {
    ensure_bulk_index_load(
        store,
        options,
        bulk_durability,
        bulk_index_load_started,
        db_write_ms,
        profile,
    )?;
    match process_index_batch(store, summary, options, batch, indexed_at, profile) {
        Ok(batch_profile) => {
            if let Err(error) = commit_bulk_index_batch(store, db_write_ms) {
                let _ = store.rollback_bulk_index_transaction();
                return Err(error);
            }
            Ok(batch_profile)
        }
        Err(error) => {
            let _ = store.rollback_bulk_index_transaction();
            Err(error)
        }
    }
}

fn delete_indexed_files_by_path(
    store: &SqliteGraphStore,
    repo_relative_paths: &[String],
) -> Result<usize, IndexError> {
    if repo_relative_paths.is_empty() {
        return Ok(0);
    }
    Ok(store.transaction(|tx| delete_indexed_files_by_path_to_writer(tx, repo_relative_paths))?)
}

fn delete_indexed_files_by_path_to_writer(
    writer: &SqliteGraphStore,
    repo_relative_paths: &[String],
) -> Result<usize, StoreError> {
    for repo_relative_path in repo_relative_paths {
        writer.delete_facts_for_file(repo_relative_path)?;
    }
    Ok(repo_relative_paths.len())
}

fn upsert_index_state_to_writer(
    writer: &SqliteGraphStore,
    repo_root: &Path,
    indexed_at: u64,
) -> Result<(), StoreError> {
    let state = RepoIndexState {
        repo_id: format!("repo://{}", repo_root.display()),
        repo_root: path_string(repo_root),
        repo_commit: None,
        schema_version: writer.schema_version()?,
        indexed_at_unix_ms: Some(indexed_at),
        files_indexed: writer.count_files()?,
        entity_count: writer.count_entities()?,
        edge_count: writer.count_edges()?,
        metadata: Default::default(),
    };
    writer.upsert_repo_index_state(&state)
}

fn expected_db_passport(
    repo_root: &Path,
    options: &IndexOptions,
) -> Result<ExpectedDbPassport, IndexError> {
    Ok(ExpectedDbPassport {
        canonical_repo_root: canonical_repo_root_string(repo_root)?,
        storage_mode: options.storage_mode.as_str().to_string(),
        index_scope_policy_hash: scope_policy_hash(&options.scope)?,
        git_remote: git_remote(repo_root),
        worktree_root: git_worktree_root(repo_root).or_else(|| Some(path_string(repo_root))),
        repo_head: git_head(repo_root),
    })
}

pub fn inspect_repo_db_passport(
    repo_root: &Path,
    db_path: &Path,
    options: &IndexOptions,
) -> Result<DbPreflightReport, IndexError> {
    let repo_root = resolve_repo_root_for_index(repo_root)?;
    let db_path = normalize_db_path(&repo_root, db_path);
    let expected = expected_db_passport(&repo_root, options)?;
    Ok(inspect_db_preflight(&db_path, SCHEMA_VERSION, &expected))
}

pub fn inspect_db_lifecycle_preflight(
    repo_root: &Path,
    db_path: &Path,
    explicit_scope_policy: Option<IndexScopeOptions>,
) -> Result<DbLifecyclePreflight, IndexError> {
    let repo_root = resolve_repo_root_for_index(repo_root)?;
    let db_path = normalize_db_path(&repo_root, db_path);
    let initial = inspect_repo_db_passport(&repo_root, &db_path, &IndexOptions::default())?;
    let Some(passport) = initial.passport.as_ref() else {
        return Ok(db_lifecycle_preflight_from_report(
            &repo_root,
            initial,
            Vec::new(),
            Vec::new(),
            "missing".to_string(),
            None,
            None,
            None,
            None,
            explicit_scope_policy,
            None,
        ));
    };

    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    let mut options = IndexOptions::default();
    let passport_scope_hash = Some(passport.index_scope_policy_hash.clone());
    match passport.storage_mode.parse::<StorageMode>() {
        Ok(storage_mode) => options.storage_mode = storage_mode,
        Err(error) => blockers.push(format!("invalid passport storage_mode: {error}")),
    }

    let passport_scope_policy = match parse_passport_scope_policy(passport) {
        Ok(scope_policy) => scope_policy,
        Err(error) => {
            blockers.push(error);
            None
        }
    };
    let mut scope_source = if explicit_scope_policy.is_some() {
        "explicit".to_string()
    } else {
        "passport".to_string()
    };
    let explicit_scope_hash = explicit_scope_policy
        .as_ref()
        .map(scope_policy_hash)
        .transpose()?;
    let effective_scope_policy = if let Some(explicit) = explicit_scope_policy.clone() {
        options.scope = explicit.clone();
        Some(explicit)
    } else if let Some(scope_policy) = passport_scope_policy.clone() {
        options.scope = scope_policy.clone();
        Some(scope_policy)
    } else {
        let default_scope = IndexScopeOptions::default();
        let default_scope_hash = scope_policy_hash(&default_scope)?;
        if passport.index_scope_policy_hash == default_scope_hash {
            warnings.push(
                "passport scope_policy_json is missing; using default-scope compatibility"
                    .to_string(),
            );
            scope_source = "compat_default".to_string();
            options.scope = default_scope.clone();
            Some(default_scope)
        } else {
            blockers.push(
                "passport scope_policy_json is missing or unreadable and hash is not default; rebuild required"
                    .to_string(),
            );
            None
        }
    };

    let report = inspect_repo_db_passport(&repo_root, &db_path, &options)?;
    let scope_mismatch = if explicit_scope_policy.is_some()
        && report
            .reasons
            .iter()
            .any(|reason| reason.contains("index scope policy hash mismatch"))
    {
        Some(ScopeMismatchDetails {
            message: "DB passport scope does not match explicit requested scope".to_string(),
            expected_scope_source: "passport".to_string(),
            expected_scope_hash: passport_scope_hash.clone(),
            observed_scope_source: "explicit".to_string(),
            observed_scope_hash: explicit_scope_hash.clone(),
            passport_scope_policy: passport_scope_policy.clone(),
            explicit_scope_policy: explicit_scope_policy.clone(),
        })
    } else {
        None
    };
    if let Some(mismatch) = scope_mismatch.as_ref() {
        blockers.push(format!(
            "{}: expected passport_scope_hash={}, observed explicit_scope_hash={}",
            mismatch.message,
            mismatch.expected_scope_hash.as_deref().unwrap_or("unknown"),
            mismatch.observed_scope_hash.as_deref().unwrap_or("unknown")
        ));
    }

    Ok(db_lifecycle_preflight_from_report(
        &repo_root,
        report,
        blockers,
        warnings,
        scope_source,
        passport_scope_hash,
        explicit_scope_hash,
        scope_mismatch,
        passport_scope_policy,
        explicit_scope_policy,
        effective_scope_policy,
    ))
}

pub fn require_db_lifecycle_preflight(
    repo_root: &Path,
    db_path: &Path,
    explicit_scope_policy: Option<IndexScopeOptions>,
) -> Result<DbLifecyclePreflight, IndexError> {
    let preflight = inspect_db_lifecycle_preflight(repo_root, db_path, explicit_scope_policy)?;
    if preflight.safe {
        Ok(preflight)
    } else {
        Err(IndexError::Message(format!(
            "CodeGraph DB is not safe to reuse at {}: {}",
            db_path.display(),
            preflight.blockers.join("; ")
        )))
    }
}

pub fn inspect_db_lifecycle_surface_preflight(
    request: DbLifecycleSurfacePreflightRequest,
) -> Result<DbLifecycleSurfacePreflight, IndexError> {
    let repo_root = resolve_repo_root_for_index(&request.repo_root)?;
    let exact_db_path = normalize_db_path(&repo_root, &request.db_path);
    let lifecycle =
        inspect_db_lifecycle_preflight(&repo_root, &exact_db_path, request.expected_scope.clone())?;
    let mut blockers = lifecycle.blockers.clone();
    let mut warnings = lifecycle.warnings.clone();

    let mut storage_mode_match = lifecycle.storage_mode_status == "ok";
    if let Some(required_storage_mode) = request.required_storage_mode {
        let observed = lifecycle
            .db_health
            .passport
            .as_ref()
            .map(|passport| passport.storage_mode.as_str());
        if observed != Some(required_storage_mode.as_str()) {
            storage_mode_match = false;
            blockers.push(format!(
                "storage mode requirement mismatch: expected {}, observed {}",
                required_storage_mode.as_str(),
                observed.unwrap_or("unknown")
            ));
        }
    }

    let base_safe = lifecycle.safe && storage_mode_match;
    let db_missing = lifecycle
        .db_health
        .reasons
        .iter()
        .any(|reason| reason_is_db_missing(reason));
    let read_open_compatible = !db_missing
        && lifecycle.db_health.passport_status != "corrupt"
        && (lifecycle.schema_status == "ok"
            || lifecycle.db_health.schema_version == Some(SCHEMA_VERSION));

    let (safe_to_read, safe_to_write, claimable, diagnostic_only, blockers) = match request
        .operation_kind
    {
        DbLifecycleOperationKind::NormalRead => (base_safe, false, base_safe, false, blockers),
        DbLifecycleOperationKind::DiagnosticRead => {
            let blockers = diagnostic_blockers_for_surface(
                blockers,
                &mut warnings,
                request.allow_stale_read,
                request.allow_foreign_repo,
            );
            let safe_to_read = read_open_compatible && blockers.is_empty();
            (safe_to_read, false, false, true, blockers)
        }
        DbLifecycleOperationKind::WriteUpdate => {
            if request.allow_stale_read || request.allow_foreign_repo {
                warnings
                    .push("write_update ignores stale/foreign diagnostic allowances".to_string());
            }
            (base_safe, base_safe, base_safe, false, blockers)
        }
        DbLifecycleOperationKind::ImportReplace => {
            let safe_to_write = db_missing || base_safe;
            let claimable = base_safe;
            (false, safe_to_write, claimable, false, blockers)
        }
        DbLifecycleOperationKind::ImportMerge => (base_safe, base_safe, base_safe, false, blockers),
        DbLifecycleOperationKind::BenchmarkInspection => {
            let blockers = if base_safe {
                blockers
            } else {
                diagnostic_blockers_for_surface(
                    blockers,
                    &mut warnings,
                    request.allow_stale_read,
                    request.allow_foreign_repo,
                )
            };
            let safe_to_read = if base_safe {
                true
            } else {
                read_open_compatible && blockers.is_empty()
            };
            let diagnostic_only = !base_safe;
            let claimable = base_safe;
            (safe_to_read, false, claimable, diagnostic_only, blockers)
        }
        DbLifecycleOperationKind::BenchmarkSetup => {
            let safe_to_write = db_missing || base_safe;
            (false, safe_to_write, base_safe, !base_safe, blockers)
        }
    };

    let mut blockers = blockers;
    blockers.sort();
    blockers.dedup();
    warnings.sort();
    warnings.dedup();

    Ok(DbLifecycleSurfacePreflight {
        surface_name: request.surface_name,
        operation_kind: request.operation_kind,
        safe_to_read,
        safe_to_write,
        claimable,
        diagnostic_only,
        db_problem_kind: lifecycle.db_problem_kind.clone(),
        path_access_status: lifecycle.path_access_status.clone(),
        path_access_error: lifecycle.path_access_error.clone(),
        passport_status: lifecycle.db_health.passport_status.clone(),
        repo_match: lifecycle.repo_root_status == "ok" && lifecycle.db_health.passport.is_some(),
        scope_match: matches!(lifecycle.scope_status.as_str(), "ok" | "compat_default"),
        schema_status: lifecycle.schema_status.clone(),
        storage_mode_match,
        blockers,
        warnings,
        exact_db_path_checked: lifecycle.db_health.db_path.clone(),
        repo_root_expected: path_string(&repo_root),
        db_path_outside_workspace: lifecycle.db_path_outside_workspace,
        outside_workspace_note: lifecycle.outside_workspace_note.clone(),
        repo_root_observed: lifecycle
            .db_health
            .passport
            .as_ref()
            .map(|passport| passport.canonical_repo_root.clone()),
        artifact_freshness: db_artifact_freshness(&lifecycle),
        scope_source: lifecycle.scope_source.clone(),
        passport_scope_hash: lifecycle.passport_scope_hash.clone(),
        explicit_scope_hash: lifecycle.explicit_scope_hash.clone(),
        lifecycle_preflight: lifecycle,
    })
}

fn diagnostic_blockers_for_surface(
    blockers: Vec<String>,
    warnings: &mut Vec<String>,
    allow_stale_read: bool,
    allow_foreign_repo: bool,
) -> Vec<String> {
    blockers
        .into_iter()
        .filter_map(|blocker| {
            if allow_foreign_repo && reason_is_repo_identity_mismatch(&blocker) {
                warnings.push(format!(
                    "diagnostic_read allowed foreign-repo blocker; output is non-claimable: {blocker}"
                ));
                None
            } else if allow_stale_read && reason_is_stale_or_missing_passport(&blocker) {
                warnings.push(format!(
                    "diagnostic_read allowed stale/passport blocker; output is non-claimable: {blocker}"
                ));
                None
            } else {
                Some(blocker)
            }
        })
        .collect()
}

fn reason_is_repo_identity_mismatch(reason: &str) -> bool {
    reason.contains("repo root mismatch") || reason.contains("git remote mismatch")
}

fn reason_is_stale_or_missing_passport(reason: &str) -> bool {
    reason.contains("previous run did not complete")
        || reason.contains("previous integrity gate was not ok")
        || reason.contains("repo head mismatch")
        || reason.contains("codegraph_db_passport table is missing")
        || reason.contains("codegraph_db_passport row is missing")
        || reason.contains("passport scope_policy_json is missing")
}

fn reason_is_db_missing(reason: &str) -> bool {
    reason.contains("main DB does not exist")
}

fn db_artifact_freshness(preflight: &DbLifecyclePreflight) -> Option<String> {
    if preflight
        .db_health
        .reasons
        .iter()
        .any(|reason| reason_is_db_missing(reason))
    {
        return Some("missing".to_string());
    }
    let Some(passport) = preflight.db_health.passport.as_ref() else {
        return Some("passport_missing".to_string());
    };
    if passport.last_run_status != "completed" {
        return Some(format!("incomplete:{}", passport.last_run_status));
    }
    if passport.integrity_gate_result != "ok" {
        return Some(format!(
            "integrity_not_ok:{}",
            passport.integrity_gate_result
        ));
    }
    if preflight
        .db_health
        .reasons
        .iter()
        .any(|reason| reason.contains("repo head mismatch"))
    {
        return Some("repo_head_mismatch".to_string());
    }
    Some("fresh".to_string())
}

fn parse_passport_scope_policy(passport: &DbPassport) -> Result<Option<IndexScopeOptions>, String> {
    let raw = passport.scope_policy_json.trim();
    if raw.is_empty() || raw == "null" {
        return Ok(None);
    }
    serde_json::from_str::<IndexScopeOptions>(raw)
        .map(Some)
        .map_err(|error| format!("passport scope_policy_json is unreadable: {error}"))
}

fn db_lifecycle_preflight_from_report(
    repo_root: &Path,
    report: DbPreflightReport,
    extra_blockers: Vec<String>,
    warnings: Vec<String>,
    scope_source: String,
    passport_scope_hash: Option<String>,
    explicit_scope_hash: Option<String>,
    scope_mismatch: Option<ScopeMismatchDetails>,
    passport_scope_policy: Option<IndexScopeOptions>,
    explicit_scope_policy: Option<IndexScopeOptions>,
    effective_scope_policy: Option<IndexScopeOptions>,
) -> DbLifecyclePreflight {
    let mut blockers = if report.valid {
        Vec::new()
    } else {
        report.reasons.clone()
    };
    blockers.extend(extra_blockers);
    blockers.sort();
    blockers.dedup();
    let safe = report.valid && blockers.is_empty();
    let scope_status = if blockers
        .iter()
        .any(|reason| reason.contains("scope mismatch") || reason.contains("scope policy"))
        || report
            .reasons
            .iter()
            .any(|reason| reason.contains("index scope policy hash mismatch"))
    {
        "mismatched"
    } else if scope_source == "missing" {
        "missing"
    } else if scope_source == "compat_default" {
        "compat_default"
    } else if report.passport.is_some() {
        "ok"
    } else {
        "unknown"
    };
    let exact_db_path_checked = report.db_path.clone();
    let db_path = PathBuf::from(&exact_db_path_checked);
    let db_path_outside_workspace = db_path_is_outside_workspace(&db_path, repo_root);
    let outside_workspace_note =
        db_path_outside_workspace.then(|| EXTERNAL_PROFILE_DB_NOTE.to_string());
    let db_problem_kind = lifecycle_db_problem_kind(&report, &blockers, scope_status);
    DbLifecyclePreflight {
        safe,
        db_problem_kind,
        path_access_status: report.path_access_status.clone(),
        path_access_error: report.path_access_error.clone(),
        blockers,
        warnings,
        exact_db_path_checked,
        repo_root_expected: path_string(repo_root),
        db_path_outside_workspace,
        outside_workspace_note,
        repo_root_status: preflight_field_status(&report, "repo root mismatch"),
        schema_status: preflight_field_status_any(
            &report,
            &["schema version mismatch", "passport schema mismatch"],
        ),
        storage_mode_status: preflight_field_status(&report, "storage mode mismatch"),
        scope_status: scope_status.to_string(),
        scope_source,
        passport_scope_hash,
        explicit_scope_hash,
        scope_mismatch,
        passport_scope_policy,
        explicit_scope_policy,
        effective_scope_policy,
        db_health: report,
    }
}

fn preflight_field_status(report: &DbPreflightReport, needle: &str) -> String {
    preflight_field_status_any(report, &[needle])
}

fn preflight_field_status_any(report: &DbPreflightReport, needles: &[&str]) -> String {
    if report.passport.is_none() && !report.valid {
        return "unknown".to_string();
    }
    if report
        .reasons
        .iter()
        .any(|reason| needles.iter().any(|needle| reason.contains(needle)))
    {
        "mismatched".to_string()
    } else {
        "ok".to_string()
    }
}

fn lifecycle_db_problem_kind(
    report: &DbPreflightReport,
    blockers: &[String],
    scope_status: &str,
) -> Option<String> {
    if matches!(
        report.db_problem_kind.as_deref(),
        Some(
            "db_missing"
                | "permission_denied"
                | "filesystem_inaccessible"
                | "db_locked"
                | "sqlite_corrupt"
        )
    ) {
        return report.db_problem_kind.clone();
    }
    if blockers.iter().any(|blocker| {
        blocker.contains("repo root mismatch") || blocker.contains("git remote mismatch")
    }) {
        Some("repo_root_mismatch".to_string())
    } else if blockers
        .iter()
        .any(|blocker| blocker.contains("repo head mismatch"))
    {
        Some("repo_head_mismatch".to_string())
    } else if scope_status == "mismatched" {
        Some("scope_mismatch".to_string())
    } else if blockers.iter().any(|blocker| {
        blocker.contains("schema version mismatch") || blocker.contains("passport schema mismatch")
    }) {
        Some("schema_mismatch".to_string())
    } else if blockers
        .iter()
        .any(|blocker| blocker.contains("storage mode mismatch"))
    {
        Some("storage_mismatch".to_string())
    } else {
        report.db_problem_kind.clone()
    }
}

fn db_path_is_outside_workspace(db_path: &Path, workspace_root: &Path) -> bool {
    let absolute_db_path = if db_path.is_absolute() {
        db_path.to_path_buf()
    } else {
        workspace_root.join(db_path)
    };
    let normalized_db_path =
        normalize_lexical_path(&canonicalize_existing_prefix(&absolute_db_path));
    let normalized_workspace_root =
        normalize_lexical_path(&canonicalize_existing_prefix(workspace_root));
    !path_starts_with_workspace(&normalized_db_path, &normalized_workspace_root)
}

fn canonicalize_existing_prefix(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }
    let mut current = path.to_path_buf();
    let mut suffix = Vec::<std::ffi::OsString>::new();
    while let Some(file_name) = current.file_name().map(|value| value.to_os_string()) {
        suffix.push(file_name);
        if !current.pop() {
            return path.to_path_buf();
        }
        if let Ok(mut canonical) = fs::canonicalize(&current) {
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return canonical;
        }
    }
    path.to_path_buf()
}

#[cfg(windows)]
fn path_starts_with_workspace(path: &Path, workspace_root: &Path) -> bool {
    let path = path.display().to_string().to_ascii_lowercase();
    let workspace_root = workspace_root.display().to_string().to_ascii_lowercase();
    path == workspace_root
        || path
            .strip_prefix(&workspace_root)
            .is_some_and(|rest| rest.starts_with('\\') || rest.starts_with('/'))
}

#[cfg(not(windows))]
fn path_starts_with_workspace(path: &Path, workspace_root: &Path) -> bool {
    path.starts_with(workspace_root)
}

pub fn require_reusable_db_passport(
    repo_root: &Path,
    db_path: &Path,
    options: &IndexOptions,
) -> Result<DbPreflightReport, IndexError> {
    let report = inspect_repo_db_passport(repo_root, db_path, options)?;
    if report.valid {
        Ok(report)
    } else {
        Err(IndexError::Message(format!(
            "CodeGraph DB is not safe to reuse at {}: {}",
            db_path.display(),
            report.reasons.join("; ")
        )))
    }
}

fn build_db_passport(
    store: &SqliteGraphStore,
    repo_root: &Path,
    options: &IndexOptions,
    indexed_at: u64,
    summary: &IndexSummary,
    integrity_gate_result: &str,
) -> Result<DbPassport, IndexError> {
    let now = unix_time_ms();
    let existing_created_at = store
        .get_db_passport()
        .ok()
        .flatten()
        .map(|passport| passport.created_at_unix_ms)
        .unwrap_or(now);
    Ok(DbPassport {
        passport_version: DB_PASSPORT_VERSION,
        codegraph_schema_version: SCHEMA_VERSION,
        storage_mode: options.storage_mode.as_str().to_string(),
        index_scope_policy_hash: scope_policy_hash(&options.scope)?,
        scope_policy_json: scope_policy_json(&options.scope)?,
        canonical_repo_root: canonical_repo_root_string(repo_root)?,
        git_remote: git_remote(repo_root),
        worktree_root: git_worktree_root(repo_root).or_else(|| Some(path_string(repo_root))),
        repo_head: git_head(repo_root),
        source_discovery_policy_version: source_discovery_policy_version().to_string(),
        codegraph_build_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        last_successful_index_timestamp: Some(indexed_at),
        last_completed_run_id: Some(format!("index-{indexed_at}-{}", std::process::id())),
        last_run_status: "completed".to_string(),
        integrity_gate_result: integrity_gate_result.to_string(),
        files_seen: summary.files_seen as u64,
        files_indexed: summary.files_indexed as u64,
        created_at_unix_ms: existing_created_at,
        updated_at_unix_ms: now,
    })
}

fn build_incremental_db_passport(
    repo_root: &Path,
    scope: &IndexScopeOptions,
    storage_mode: StorageMode,
    indexed_at: u64,
    files_seen: u64,
    files_indexed: u64,
    existing_passport: Option<&DbPassport>,
) -> Result<DbPassport, IndexError> {
    let now = unix_time_ms();
    Ok(DbPassport {
        passport_version: DB_PASSPORT_VERSION,
        codegraph_schema_version: SCHEMA_VERSION,
        storage_mode: storage_mode.as_str().to_string(),
        index_scope_policy_hash: scope_policy_hash(scope)?,
        scope_policy_json: scope_policy_json(scope)?,
        canonical_repo_root: canonical_repo_root_string(repo_root)?,
        git_remote: git_remote(repo_root),
        worktree_root: git_worktree_root(repo_root).or_else(|| Some(path_string(repo_root))),
        repo_head: git_head(repo_root),
        source_discovery_policy_version: source_discovery_policy_version().to_string(),
        codegraph_build_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        last_successful_index_timestamp: Some(indexed_at),
        last_completed_run_id: Some(format!("incremental-{indexed_at}-{}", std::process::id())),
        last_run_status: "completed".to_string(),
        integrity_gate_result: "ok".to_string(),
        files_seen,
        files_indexed,
        created_at_unix_ms: existing_passport
            .map(|passport| passport.created_at_unix_ms)
            .unwrap_or(now),
        updated_at_unix_ms: now,
    })
}

fn source_discovery_policy_version() -> &'static str {
    "scope-policy-v1"
}

fn scope_policy_json(options: &IndexScopeOptions) -> Result<String, IndexError> {
    serde_json::to_string(options).map_err(|error| IndexError::Message(error.to_string()))
}

pub fn scope_policy_hash(options: &IndexScopeOptions) -> Result<String, IndexError> {
    Ok(stable_hex_hash(scope_policy_json(options)?.as_bytes()))
}

fn stable_hex_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn sqlite_index_error(context: &str, error: rusqlite::Error) -> IndexError {
    IndexError::Message(format!("{context}: {error}"))
}

pub fn candidate_spool_query_index_path(spool_path: &Path) -> PathBuf {
    let Some(file_name) = spool_path.file_name().and_then(|name| name.to_str()) else {
        return spool_path.with_extension("query.sqlite");
    };
    spool_path.with_file_name(format!("{file_name}.query.sqlite"))
}

fn canonical_repo_root_string(repo_root: &Path) -> Result<String, IndexError> {
    Ok(path_string(&fs::canonicalize(repo_root)?))
}

fn git_remote(repo_root: &Path) -> Option<String> {
    git_output(repo_root, &["config", "--get", "remote.origin.url"])
}

fn git_worktree_root(repo_root: &Path) -> Option<String> {
    git_output(repo_root, &["rev-parse", "--show-toplevel"])
}

fn git_head(repo_root: &Path) -> Option<String> {
    git_output(repo_root, &["rev-parse", "HEAD"])
}

fn git_output(repo_root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn process_index_batch(
    store: &SqliteGraphStore,
    summary: &mut IndexSummary,
    options: &IndexOptions,
    batch: PendingIndexBatch,
    indexed_at: u64,
    profile: &mut IndexPhaseRecorder,
) -> Result<IndexBatchProfile, IndexError> {
    if batch.files.is_empty() {
        return Ok(IndexBatchProfile::default());
    }

    let batch_index = summary.batches_total + 1;
    summary.batches_total += 1;
    emit_index_progress(
        options,
        json!({
            "event": "index_batch_started",
            "batch_index": batch_index,
            "files": batch.files.len(),
            "source_bytes": batch.source_bytes,
        }),
    );

    let worker_count = effective_worker_count(options, batch.files.len());
    let parse_extract_start = Instant::now();
    let (local_bundles, stats) = parse_extract_pending_files_with_progress(
        batch.files,
        worker_count,
        options.profile && options.json,
        &options.graph_output_budgets,
    )?;
    let files_parsed_count = stats
        .iter()
        .filter(|stat| !stat.parse_error && !stat.skipped)
        .count();
    emit_index_progress(
        options,
        json!({
            "event": "files_parsed",
            "batch_index": batch_index,
            "files_parsed": files_parsed_count,
            "parse_errors": stats.iter().filter(|stat| stat.parse_error).count(),
            "syntax_errors": stats.iter().filter(|stat| stat.syntax_error).count(),
        }),
    );
    let mut spool_chunks = Vec::new();
    for bundle in &local_bundles {
        spool_chunks.extend(candidate_spool_chunks_for_local_bundle(bundle)?);
    }
    append_candidate_spool_chunks(summary, spool_chunks)?;
    write_path_chaos_failpoint("candidate_spool_after_batch_before_db_write")?;
    profile.add_duration(
        "parse_extract_workers_wall",
        parse_extract_start.elapsed(),
        1,
        stats.len() as u64,
    );
    let reduce_start = Instant::now();
    let reduced_plan = reduce_local_fact_bundles(local_bundles);
    let bundles_reduced = reduced_plan.bundles.len();
    let edges_staged = reduced_plan
        .bundles
        .iter()
        .map(|bundle| bundle.extraction.edges.len())
        .sum::<usize>()
        + reduced_plan.global_facts.edges.len();
    profile.add_duration("reducer", reduce_start.elapsed(), 1, bundles_reduced as u64);
    emit_index_progress(
        options,
        json!({
            "event": "bundles_reduced",
            "batch_index": batch_index,
            "bundles_reduced": bundles_reduced,
            "edges_staged": edges_staged,
            "durability_status": "staged_in_memory_not_durably_committed",
        }),
    );
    let parse_ms = stats.iter().map(|stat| stat.parse_ms).sum::<u128>();
    let extraction_ms = stats.iter().map(|stat| stat.extraction_ms).sum::<u128>();
    let bundle_ms = stats.iter().map(|stat| stat.bundle_ms).sum::<u128>();
    profile.add_ms(
        "parse",
        parse_ms as f64,
        stats.len() as u64,
        stats.len() as u64,
    );
    profile.add_ms(
        "local_fact_bundle_creation",
        bundle_ms as f64,
        stats.len() as u64,
        stats.len() as u64,
    );
    profile.add_ms(
        "extract_entities_and_relations",
        extraction_ms as f64,
        stats.len() as u64,
        stats.len() as u64,
    );
    let failed_paths = stats
        .iter()
        .filter(|stat| stat.parse_error || stat.skipped)
        .map(|stat| stat.repo_relative_path.clone())
        .collect::<Vec<_>>();

    for stat in &stats {
        for hit in &stat.graph_output_budget_hits {
            record_graph_output_budget_hit(summary, options, hit.clone());
        }

        if stat.parse_error {
            summary.parse_errors += 1;
            summary.files_skipped += 1;
            summary.failed_files_deleted += 1;
            record_index_issue(
                summary,
                options,
                stat.repo_relative_path.clone(),
                "parse_error",
                stat.message
                    .clone()
                    .unwrap_or_else(|| "parser failed".to_string()),
                "skipped_and_deleted_old_facts",
            );
        } else if stat.skipped {
            summary.files_skipped += 1;
            summary.failed_files_deleted += 1;
            record_index_issue(
                summary,
                options,
                stat.repo_relative_path.clone(),
                "parser_skipped",
                stat.message
                    .clone()
                    .unwrap_or_else(|| "parser returned no parse tree".to_string()),
                "skipped_and_deleted_old_facts",
            );
        }

        if stat.syntax_error {
            summary.syntax_errors += 1;
            record_index_issue(
                summary,
                options,
                stat.repo_relative_path.clone(),
                "syntax_error",
                "tree-sitter reported syntax diagnostics".to_string(),
                "indexed_with_syntax_errors",
            );
        }
    }

    let db_start = Instant::now();
    for failed in &failed_paths {
        store.delete_facts_for_file(failed)?;
    }
    let persisted = persist_reduced_index_plan(store, reduced_plan, indexed_at, options, profile)?;
    let db_write_ms = db_start.elapsed().as_millis();

    summary.files_indexed += persisted.files;
    summary.entities += persisted.entities;
    summary.edges += persisted.edges;
    summary.duplicate_edges_upserted += persisted.duplicate_edges_upserted;
    summary.batches_completed += 1;
    emit_index_progress(
        options,
        index_batch_processed_progress_event(
            batch_index,
            &persisted,
            stats.iter().filter(|stat| stat.parse_error).count(),
            stats.iter().filter(|stat| stat.syntax_error).count(),
            db_write_ms,
            parse_ms,
            extraction_ms,
            bundle_ms,
            worker_count,
            files_parsed_count,
            bundles_reduced,
            edges_staged,
        ),
    );

    Ok(IndexBatchProfile {
        parse_ms,
        extraction_ms,
        bundle_ms,
        db_write_ms,
        worker_count,
    })
}

fn index_batch_processed_progress_event(
    batch_index: usize,
    persisted: &PersistedBatchSummary,
    parse_errors: usize,
    syntax_errors: usize,
    db_write_ms: u128,
    parse_ms: u128,
    extraction_ms: u128,
    bundle_ms: u128,
    worker_count: usize,
    files_parsed: usize,
    bundles_reduced: usize,
    edges_staged: usize,
) -> Value {
    json!({
        "event": "index_batch_processed",
        "batch_progress_status": "processed_not_durably_committed",
        "durability_status": "staged_in_open_transaction",
        "visible_db_mutation_claim": "not_claimed_until_commit_or_publish",
        "batch_index": batch_index,
        "files_parsed": files_parsed,
        "bundles_reduced": bundles_reduced,
        "edges_staged": edges_staged,
        "edges_inserted": persisted.edges,
        "files_indexed": persisted.files,
        "entities": persisted.entities,
        "edges": persisted.edges,
        "duplicate_edges_upserted": persisted.duplicate_edges_upserted,
        "parse_errors": parse_errors,
        "syntax_errors": syntax_errors,
        "db_write_ms": db_write_ms,
        "parse_ms": parse_ms,
        "extraction_ms": extraction_ms,
        "bundle_ms": bundle_ms,
        "worker_count": worker_count,
    })
}

fn persist_reduced_index_plan(
    store: &SqliteGraphStore,
    plan: ReducedIndexPlan,
    indexed_at: u64,
    options: &IndexOptions,
    profile: &mut IndexPhaseRecorder,
) -> Result<PersistedBatchSummary, StoreError> {
    let mut summary =
        persist_local_fact_bundles(store, plan.bundles, indexed_at, options, profile)?;
    let global_summary =
        persist_global_fact_reduction_plan(store, plan.global_facts, options, profile)?;
    summary.entities += global_summary.entities_inserted;
    summary.edges += global_summary.edges_inserted;
    summary.duplicate_edges_upserted += global_summary.edges_upserted_existing;
    Ok(summary)
}

fn persist_global_fact_reduction_plan(
    store: &SqliteGraphStore,
    mut plan: GlobalFactReductionPlan,
    options: &IndexOptions,
    profile: &mut IndexPhaseRecorder,
) -> Result<GlobalFactApplySummary, StoreError> {
    plan.sort();
    apply_global_fact_reduction_plan_to_writer(store, &plan, options, profile)
}

fn apply_global_fact_reduction_plan_to_writer(
    writer: &SqliteGraphStore,
    plan: &GlobalFactReductionPlan,
    options: &IndexOptions,
    profile: &mut IndexPhaseRecorder,
) -> Result<GlobalFactApplySummary, StoreError> {
    let mut summary = GlobalFactApplySummary::default();
    for fact in &plan.entities {
        if should_route_static_reference_entity(&fact.entity)
            || should_route_unresolved_entity(&fact.entity)
        {
            if options.storage_mode.preserves_heuristic_sidecars() {
                writer.insert_static_reference_after_file_delete(&fact.entity)?;
            }
            continue;
        }
        let lookup_start = Instant::now();
        let existed = writer.physical_entity_exists(&fact.entity.id)?;
        profile.add_duration("sql_query_execution", lookup_start.elapsed(), 1, 1);
        let entity_start = Instant::now();
        match fact.write_mode {
            GlobalEntityWriteMode::UpsertIndexed => writer.upsert_entity(&fact.entity)?,
            GlobalEntityWriteMode::InsertRecordIfMissing if !existed => {
                writer.insert_entity_record_after_file_delete(&fact.entity)?
            }
            GlobalEntityWriteMode::InsertRecordIfMissing => {}
        }
        profile.add_duration("entity_insert", entity_start.elapsed(), 1, 1);
        if !existed {
            summary.entities_inserted += 1;
        }
    }

    let fast_fresh_proof_insert = options.build_mode == IndexBuildMode::ProofBuildOnly;
    let mut seen_edge_ids = BTreeSet::<&str>::new();
    for edge in &plan.edges {
        if should_route_heuristic_edge(edge) {
            if options.storage_mode.preserves_heuristic_sidecars() {
                writer.insert_heuristic_edge_after_file_delete(edge)?;
            }
            continue;
        }
        if fast_fresh_proof_insert {
            if seen_edge_ids.insert(edge.id.as_str()) {
                summary.edges_inserted += 1;
            } else {
                summary.edges_upserted_existing += 1;
            }
            let edge_start = Instant::now();
            writer.insert_edge_after_file_delete(edge)?;
            profile.add_duration("edge_insert", edge_start.elapsed(), 1, 1);
        } else {
            let lookup_start = Instant::now();
            if !writer.stored_edge_exists(&edge.id)? {
                summary.edges_inserted += 1;
            } else {
                summary.edges_upserted_existing += 1;
            }
            profile.add_duration("sql_query_execution", lookup_start.elapsed(), 1, 1);
            let edge_start = Instant::now();
            writer.upsert_edge(edge)?;
            profile.add_duration("edge_insert", edge_start.elapsed(), 1, 1);
        }
    }
    Ok(summary)
}

fn refresh_stored_path_evidence_to_writer(
    writer: &SqliteGraphStore,
    max_rows: usize,
    profile: &mut IndexPhaseRecorder,
) -> Result<usize, StoreError> {
    if max_rows == 0 {
        return Ok(0);
    }

    let list_start = Instant::now();
    let candidate_limit = max_rows
        .saturating_mul(DEFAULT_STORED_PATH_EVIDENCE_SCAN_MULTIPLIER)
        .max(max_rows);
    let mut edges = writer.list_edges(candidate_limit)?;
    profile.add_duration(
        "sql_query_execution",
        list_start.elapsed(),
        1,
        edges.len() as u64,
    );
    let generation_start = Instant::now();
    edges.retain(should_persist_stored_path_evidence_for_edge);
    edges.sort_by(|left, right| {
        stored_path_evidence_edge_priority(left)
            .cmp(&stored_path_evidence_edge_priority(right))
            .then_with(|| left.id.cmp(&right.id))
    });
    edges.truncate(max_rows);

    let engine = ExactGraphQueryEngine::new(Vec::new());
    let mut persisted = 0usize;
    let mut seen = BTreeSet::<String>::new();
    for edge in edges {
        let evidence = stored_path_evidence_for_edge(&engine, edge);
        if seen.insert(evidence.id.clone()) {
            let upsert_start = Instant::now();
            writer.upsert_path_evidence(&evidence)?;
            profile.add_duration("path_evidence_insert", upsert_start.elapsed(), 1, 1);
            persisted += 1;
        }
    }
    profile.add_duration(
        "path_evidence_generation",
        generation_start.elapsed(),
        1,
        persisted as u64,
    );
    Ok(persisted)
}

fn refresh_stored_path_evidence_for_edges_to_writer(
    writer: &SqliteGraphStore,
    mut edges: Vec<Edge>,
    max_rows: usize,
    profile: &mut IndexPhaseRecorder,
) -> Result<usize, StoreError> {
    if max_rows == 0 || edges.is_empty() {
        return Ok(0);
    }

    let generation_start = Instant::now();
    edges.retain(should_persist_stored_path_evidence_for_edge);
    edges.sort_by(|left, right| {
        stored_path_evidence_edge_priority(left)
            .cmp(&stored_path_evidence_edge_priority(right))
            .then_with(|| left.id.cmp(&right.id))
    });
    edges.dedup_by(|left, right| left.id == right.id);
    edges.truncate(max_rows);

    let engine = ExactGraphQueryEngine::new(Vec::new());
    let mut persisted = 0usize;
    let mut seen = BTreeSet::<String>::new();
    for edge in edges {
        let evidence = stored_path_evidence_for_edge(&engine, edge);
        if seen.insert(evidence.id.clone()) {
            let upsert_start = Instant::now();
            writer.upsert_path_evidence(&evidence)?;
            profile.add_duration("path_evidence_insert", upsert_start.elapsed(), 1, 1);
            persisted += 1;
        }
    }
    profile.add_duration(
        "path_evidence_generation",
        generation_start.elapsed(),
        1,
        persisted as u64,
    );
    Ok(persisted)
}

fn should_persist_stored_path_evidence_for_edge(edge: &Edge) -> bool {
    is_proof_path_relation(edge.relation)
        || matches!(
            edge.relation,
            RelationKind::Imports
                | RelationKind::Exports
                | RelationKind::Reexports
                | RelationKind::AliasOf
                | RelationKind::AliasedBy
                | RelationKind::MayMutate
        )
        || edge.derived
        || !edge.provenance_edges.is_empty()
}

fn stored_path_evidence_edge_priority(edge: &Edge) -> (u8, u8, u8, String) {
    let exact_priority = if edge_exactness_is_path_evidence_grade(edge.exactness) {
        0
    } else {
        1
    };
    let provenance_priority = if edge.derived || !edge.provenance_edges.is_empty() {
        0
    } else {
        1
    };
    let proof_priority = if is_proof_path_relation(edge.relation) {
        0
    } else {
        1
    };
    (
        exact_priority,
        provenance_priority,
        proof_priority,
        edge.source_span.repo_relative_path.clone(),
    )
}

fn edge_exactness_is_path_evidence_grade(exactness: Exactness) -> bool {
    matches!(
        exactness,
        Exactness::Exact
            | Exactness::CompilerVerified
            | Exactness::LspVerified
            | Exactness::ParserVerified
            | Exactness::DerivedFromVerifiedEdges
    )
}

fn stored_path_evidence_for_edge(engine: &ExactGraphQueryEngine, edge: Edge) -> PathEvidence {
    let path = graph_path_for_single_edge(edge);
    let mut evidence = engine.path_evidence(&path);
    evidence.metadata.insert(
        "task_or_query".to_string(),
        json!("index://stored-path-evidence"),
    );
    evidence.metadata.insert(
        "path_storage_policy".to_string(),
        json!("bounded_deterministic_single_edge_paths"),
    );
    evidence.metadata.insert(
        "source_span_validation".to_string(),
        json!("not_validated_at_index_time"),
    );
    evidence
}

fn graph_path_for_single_edge(edge: Edge) -> GraphPath {
    let source = edge.head_id.clone();
    let target = edge.tail_id.clone();
    let cost = if is_proof_path_relation(edge.relation) {
        0.75
    } else {
        1.0
    };
    let uncertainty = stored_path_evidence_uncertainty(&edge);
    GraphPath {
        source: source.clone(),
        target: target.clone(),
        steps: vec![TraversalStep {
            edge,
            direction: TraversalDirection::Forward,
            from: source,
            to: target,
        }],
        cost,
        uncertainty,
    }
}

fn stored_path_evidence_uncertainty(edge: &Edge) -> f64 {
    let confidence_penalty = (1.0 - edge.confidence).clamp(0.0, 1.0);
    let exactness_penalty = match edge.exactness {
        Exactness::Exact | Exactness::CompilerVerified | Exactness::LspVerified => 0.0,
        Exactness::ParserVerified => 0.05,
        Exactness::StaticHeuristic => 0.35,
        Exactness::DynamicTrace => 0.10,
        Exactness::Inferred => 0.50,
        Exactness::DerivedFromVerifiedEdges => 0.15,
    };
    confidence_penalty + exactness_penalty
}

fn persist_local_fact_bundles(
    store: &SqliteGraphStore,
    indexed_files: Vec<LocalFactBundle>,
    indexed_at: u64,
    options: &IndexOptions,
    profile: &mut IndexPhaseRecorder,
) -> Result<PersistedBatchSummary, StoreError> {
    let mut summary = PersistedBatchSummary::default();
    for indexed in &indexed_files {
        if indexed.needs_delete {
            let delete_start = Instant::now();
            store.delete_facts_for_file(&indexed.repo_relative_path)?;
            profile.add_duration("stale_fact_delete", delete_start.elapsed(), 1, 1);
        }
    }
    for indexed in indexed_files {
        let mut extraction = indexed.extraction;
        extraction.file.indexed_at_unix_ms = Some(indexed_at);
        let mut snippets = None;
        let file_start = Instant::now();
        write_path_chaos_store_failpoint("cold_during_db_write")?;
        write_path_chaos_store_failpoint("cold_disk_full_simulated")?;
        store.upsert_file(&extraction.file)?;
        profile.add_duration("file_manifest_upsert", file_start.elapsed(), 1, 1);

        if indexed.duplicate_of.is_some() {
            summary.files += 1;
            continue;
        }

        if options.storage_mode.preserves_heuristic_sidecars() {
            persist_debug_sidecars(
                store,
                &indexed.repo_relative_path,
                Some(&indexed.file_hash),
                &extraction.entities,
                &extraction.edges,
                &indexed.unresolved_references,
                &indexed.extraction_warnings,
                profile,
            )?;
        }

        let persisted_entity_ids = persisted_entity_ids(&extraction.entities, options.storage_mode);
        if indexed.template_required {
            let preparation_start = Instant::now();
            let template_entities = extraction
                .entities
                .iter()
                .filter(|entity| should_persist_entity(entity, options.storage_mode))
                .cloned()
                .collect::<Vec<_>>();
            profile.add_duration(
                "template_entities_preparation",
                preparation_start.elapsed(),
                1,
                template_entities.len() as u64,
            );
            let preparation_start = Instant::now();
            let template_edges = extraction
                .edges
                .iter()
                .filter(|edge| should_persist_edge(edge, &persisted_entity_ids))
                .filter(|edge| should_store_template_edge_row(edge))
                .cloned()
                .collect::<Vec<_>>();
            profile.add_duration(
                "template_edges_preparation",
                preparation_start.elapsed(),
                1,
                template_edges.len() as u64,
            );
            let template_start = Instant::now();
            store.upsert_content_template_extraction(
                &extraction.file,
                &template_entities,
                &template_edges,
            )?;
            profile.add_duration(
                "content_template_upsert",
                template_start.elapsed(),
                1,
                (template_entities.len() + template_edges.len()) as u64,
            );
            summary.files += 1;
            summary.entities += template_entities.len();
            summary.edges += template_edges.len();
            continue;
        }
        let mut entity_count = 0usize;
        let mut edge_count = 0usize;

        let preparation_start = Instant::now();
        let proof_entities = extraction
            .entities
            .iter()
            .filter(|entity| should_persist_entity(entity, options.storage_mode))
            .collect::<Vec<_>>();
        profile.add_duration(
            "proof_entities_preparation",
            preparation_start.elapsed(),
            1,
            proof_entities.len() as u64,
        );
        for entity in proof_entities {
            let entity_start = Instant::now();
            if should_index_entity_text(entity) {
                store.insert_entity_after_file_delete(entity)?;
            } else {
                store.insert_entity_record_after_file_delete(entity)?;
            }
            profile.add_duration("entity_insert", entity_start.elapsed(), 1, 1);
            if let Some(span) = &entity.source_span {
                if should_index_entity_snippet(entity) {
                    let snippet_start = Instant::now();
                    let snippets =
                        snippets.get_or_insert_with(|| SourceSnippetCache::new(&indexed.source));
                    store.insert_snippet_text_after_file_delete(
                        &entity.id,
                        span,
                        &snippets.snippet(span),
                    )?;
                    profile.add_duration("snippet_loading", snippet_start.elapsed(), 1, 1);
                }
            }
            entity_count += 1;
        }

        let preparation_start = Instant::now();
        let proof_edges = extraction
            .edges
            .iter()
            .filter(|edge| should_persist_edge(edge, &persisted_entity_ids))
            .collect::<Vec<_>>();
        profile.add_duration(
            "proof_edges_preparation",
            preparation_start.elapsed(),
            1,
            proof_edges.len() as u64,
        );
        for edge in proof_edges {
            if should_store_edge_row(edge) {
                let edge_start = Instant::now();
                store.insert_edge_after_file_delete(edge)?;
                profile.add_duration("edge_insert", edge_start.elapsed(), 1, 1);
            }
            edge_count += 1;
        }

        summary.files += 1;
        summary.entities += entity_count;
        summary.edges += edge_count;
    }
    Ok(summary)
}

fn persist_debug_sidecars(
    store: &SqliteGraphStore,
    repo_relative_path: &str,
    file_hash: Option<&String>,
    entities: &[Entity],
    edges: &[Edge],
    unresolved_references: &[LocalFactReference],
    extraction_warnings: &[String],
    profile: &mut IndexPhaseRecorder,
) -> Result<(), StoreError> {
    let start = Instant::now();
    let mut rows = 0u64;
    for entity in entities.iter().filter(|entity| {
        should_route_static_reference_entity(entity) || should_route_unresolved_entity(entity)
    }) {
        store.insert_static_reference_after_file_delete(entity)?;
        rows += 1;
    }
    for edge in edges
        .iter()
        .filter(|edge| should_route_heuristic_edge(edge))
    {
        store.insert_heuristic_edge_after_file_delete(edge)?;
        rows += 1;
    }
    for reference in unresolved_references {
        let metadata = json!({
            "fact_class": "unresolved_reference",
            "storage_mode": "audit_debug_sidecar",
            "repo_relative_path": normalize_graph_path(repo_relative_path),
        });
        store.insert_unresolved_reference_after_file_delete(
            &reference.reference_id,
            &reference.name,
            reference.relation,
            &reference.source_span,
            file_hash.map(String::as_str),
            reference.exactness,
            &reference.extractor,
            &metadata,
        )?;
        rows += 1;
    }
    for warning in extraction_warnings {
        let metadata = json!({
            "fact_class": "extraction_warning",
            "storage_mode": "audit_debug_sidecar",
        });
        store.insert_extraction_warning_after_file_delete(
            repo_relative_path,
            file_hash.map(String::as_str),
            warning,
            &metadata,
        )?;
        rows += 1;
    }
    profile.add_duration("debug_sidecar_insert", start.elapsed(), 1, rows);
    Ok(())
}

fn record_index_issue(
    summary: &mut IndexSummary,
    options: &IndexOptions,
    repo_relative_path: String,
    kind: &str,
    message: String,
    action: &str,
) {
    *summary.issue_counts.entry(kind.to_string()).or_insert(0) += 1;
    let issue = IndexIssue {
        repo_relative_path,
        kind: kind.to_string(),
        message,
        action: action.to_string(),
    };
    emit_index_progress(
        options,
        json!({
            "event": "index_issue",
            "repo_relative_path": issue.repo_relative_path.clone(),
            "kind": issue.kind.clone(),
            "message": issue.message.clone(),
            "action": issue.action.clone(),
        }),
    );
    summary.issues.push(issue);
}

fn record_graph_output_budget_hit(
    summary: &mut IndexSummary,
    options: &IndexOptions,
    hit: GraphOutputBudgetHit,
) {
    summary.graph_output_budgets.record_hit(hit.clone());
    record_index_issue(
        summary,
        options,
        hit.repo_relative_path.clone(),
        "graph_output_budget_hit",
        hit.message(),
        "indexed_with_degraded_graph_output_budget",
    );
}

fn emit_index_progress(options: &IndexOptions, event: Value) {
    if options.profile && options.json {
        eprintln!("{event}");
    }
}

fn emit_post_local_stage_started(options: &IndexOptions, stage: &str) {
    emit_index_progress(
        options,
        json!({
            "event": "post_local_stage_started",
            "stage": stage,
        }),
    );
}

fn emit_post_local_stage_completed(
    options: &IndexOptions,
    stage: &str,
    elapsed: Duration,
    items: u64,
) {
    emit_index_progress(
        options,
        json!({
            "elapsed_ms": elapsed.as_millis(),
            "event": "post_local_stage_completed",
            "items": items,
            "stage": stage,
        }),
    );
}

pub fn should_start_new_index_batch(
    current_files: usize,
    current_source_bytes: usize,
    next_source_bytes: usize,
    max_files: usize,
    max_source_bytes: usize,
) -> bool {
    current_files > 0
        && (current_files >= max_files
            || current_source_bytes.saturating_add(next_source_bytes) > max_source_bytes)
}

fn deterministic_worker_count(file_count: usize) -> usize {
    if file_count <= 1 {
        return 1;
    }
    thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .clamp(1, file_count.min(16))
}

fn effective_worker_count(options: &IndexOptions, file_count: usize) -> usize {
    options
        .worker_count
        .map(|requested| requested.max(1).min(file_count.max(1)))
        .unwrap_or_else(|| deterministic_worker_count(file_count))
}

pub fn parse_extract_pending_files(
    pending: Vec<PendingIndexFile>,
    worker_count: usize,
) -> Result<(Vec<IndexedFileOutput>, Vec<ParseExtractStat>), IndexError> {
    parse_extract_pending_files_with_progress(
        pending,
        worker_count,
        false,
        &GraphOutputBudgets::default(),
    )
}

fn parse_extract_pending_files_with_progress(
    mut pending: Vec<PendingIndexFile>,
    worker_count: usize,
    emit_profile_json: bool,
    graph_output_budgets: &GraphOutputBudgets,
) -> Result<(Vec<IndexedFileOutput>, Vec<ParseExtractStat>), IndexError> {
    pending.sort_by(|left, right| left.repo_relative_path.cmp(&right.repo_relative_path));
    if pending.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let worker_count = worker_count.max(1).min(pending.len());
    let chunk_size = pending.len().div_ceil(worker_count);
    let mut handles = Vec::new();
    let mut worker_chunks = Vec::new();
    for _ in 0..worker_count {
        worker_chunks.push(Vec::new());
    }
    for (index, file) in pending.into_iter().enumerate() {
        let worker_index = index / chunk_size;
        worker_chunks[worker_index].push(file);
    }
    for work in worker_chunks.into_iter().filter(|chunk| !chunk.is_empty()) {
        let graph_output_budgets = graph_output_budgets.clone();
        handles.push(thread::spawn(move || {
            let parser = TreeSitterParser;
            let mut outputs = Vec::new();
            let mut stats = Vec::new();
            for file in work {
                if emit_profile_json {
                    eprintln!(
                        "{}",
                        json!({
                            "event": "file_extract_started",
                            "repo_relative_path": &file.repo_relative_path,
                            "source_bytes": file.source.len(),
                            "duplicate": file.duplicate_of.is_some(),
                        })
                    );
                }
                if file.duplicate_of.is_some() {
                    let repo_relative_path = file.repo_relative_path.clone();
                    let duplicate_of = file.duplicate_of.clone();
                    let extraction = BasicExtraction {
                        file: FileRecord {
                            repo_relative_path: file.repo_relative_path.clone(),
                            file_hash: file.file_hash,
                            language: file.language,
                            size_bytes: file.size_bytes,
                            indexed_at_unix_ms: None,
                            metadata: file_manifest_metadata(file.modified_unix_nanos.clone()),
                        },
                        entities: Vec::new(),
                        edges: Vec::new(),
                    };
                    let bundle_start = Instant::now();
                    outputs.push(LocalFactBundle::new(
                        repo_relative_path,
                        String::new(),
                        file.needs_delete,
                        duplicate_of,
                        file.template_required,
                        extraction,
                    ));
                    let bundle_ms = bundle_start.elapsed().as_millis();
                    if emit_profile_json {
                        eprintln!(
                            "{}",
                            json!({
                                "event": "file_extract_completed",
                                "repo_relative_path": &file.repo_relative_path,
                                "status": "duplicate_template",
                                "parse_ms": 0,
                                "extraction_ms": 0,
                                "bundle_ms": bundle_ms,
                                "entities": 0,
                                "edges": 0,
                            })
                        );
                    }
                    stats.push(ParseExtractStat {
                        repo_relative_path: file.repo_relative_path,
                        parse_ms: 0,
                        extraction_ms: 0,
                        bundle_ms,
                        parse_error: false,
                        syntax_error: false,
                        skipped: false,
                        message: None,
                        graph_output_budget_hits: Vec::new(),
                    });
                    continue;
                }
                if should_skip_graph_extraction_for_large_generated_or_test_source(
                    &file.repo_relative_path,
                    file.source.len(),
                ) {
                    let mut metadata = file_manifest_metadata(file.modified_unix_nanos.clone());
                    metadata.insert(
                        "parser_status".to_string(),
                        "graph_extraction_skipped_budget".into(),
                    );
                    metadata.insert("claim_state".to_string(), "source_navigation_only".into());
                    metadata.insert("graph_extraction_skipped".to_string(), true.into());
                    metadata.insert(
                        "graph_extraction_skip_reason".to_string(),
                        "large_generated_or_test_source_budget".into(),
                    );
                    metadata.insert("graph_relation_claims".to_string(), json!([]));
                    let extraction = BasicExtraction {
                        file: FileRecord {
                            repo_relative_path: file.repo_relative_path.clone(),
                            file_hash: file.file_hash.clone(),
                            language: file.language.clone(),
                            size_bytes: file.size_bytes,
                            indexed_at_unix_ms: None,
                            metadata,
                        },
                        entities: Vec::new(),
                        edges: Vec::new(),
                    };
                    let bundle_start = Instant::now();
                    outputs.push(LocalFactBundle::new(
                        file.repo_relative_path.clone(),
                        file.source,
                        file.needs_delete,
                        None,
                        file.template_required,
                        extraction,
                    ));
                    let bundle_ms = bundle_start.elapsed().as_millis();
                    if emit_profile_json {
                        eprintln!(
                            "{}",
                            json!({
                                "event": "file_extract_completed",
                                "repo_relative_path": &file.repo_relative_path,
                                "status": "graph_extraction_skipped_budget",
                                "reason": "large_generated_or_test_source_budget",
                                "parse_ms": 0,
                                "extraction_ms": 0,
                                "bundle_ms": bundle_ms,
                                "entities": 0,
                                "edges": 0,
                            })
                        );
                    }
                    stats.push(ParseExtractStat {
                        repo_relative_path: file.repo_relative_path,
                        parse_ms: 0,
                        extraction_ms: 0,
                        bundle_ms,
                        parse_error: false,
                        syntax_error: false,
                        skipped: false,
                        message: Some("graph_extraction_skipped_budget".to_string()),
                        graph_output_budget_hits: Vec::new(),
                    });
                    continue;
                }
                let parse_start = Instant::now();
                let parsed = parser.parse(&file.repo_relative_path, &file.source);
                let parse_ms = parse_start.elapsed().as_millis();
                match parsed {
                    Ok(Some(parsed)) => {
                        let syntax_error = parsed.has_syntax_errors();
                        let extraction_start = Instant::now();
                        let mut extraction = extract_entities_and_relations(&parsed, &file.source);
                        extraction.file.size_bytes = file.size_bytes;
                        extraction.file.metadata = file_manifest_metadata_with_parser_status(
                            file.modified_unix_nanos.clone(),
                            extraction.file.metadata.clone(),
                        );
                        let graph_output_budget_hits = apply_graph_output_budgets_to_extraction(
                            &file.repo_relative_path,
                            &mut extraction,
                            &graph_output_budgets,
                        );
                        let extraction_ms = extraction_start.elapsed().as_millis();
                        let extracted_repo_relative_path = parsed.repo_relative_path.clone();
                        let entity_count = extraction.entities.len();
                        let edge_count = extraction.edges.len();
                        let bundle_start = Instant::now();
                        outputs.push(LocalFactBundle::new(
                            file.repo_relative_path,
                            file.source,
                            file.needs_delete,
                            None,
                            file.template_required,
                            extraction,
                        ));
                        let bundle_ms = bundle_start.elapsed().as_millis();
                        if emit_profile_json {
                            eprintln!(
                                "{}",
                                json!({
                                    "event": "file_extract_completed",
                                    "repo_relative_path": &extracted_repo_relative_path,
                                    "status": "ok",
                                    "parse_ms": parse_ms,
                                    "extraction_ms": extraction_ms,
                                    "bundle_ms": bundle_ms,
                                    "entities": entity_count,
                                    "edges": edge_count,
                                    "syntax_error": syntax_error,
                                })
                            );
                        }
                        stats.push(ParseExtractStat {
                            repo_relative_path: extracted_repo_relative_path,
                            parse_ms,
                            extraction_ms,
                            bundle_ms,
                            parse_error: false,
                            syntax_error,
                            skipped: false,
                            message: None,
                            graph_output_budget_hits,
                        });
                    }
                    Ok(None) => {
                        if emit_profile_json {
                            eprintln!(
                                "{}",
                                json!({
                                    "event": "file_extract_completed",
                                    "repo_relative_path": &file.repo_relative_path,
                                    "status": "skipped",
                                    "parse_ms": parse_ms,
                                    "extraction_ms": 0,
                                    "bundle_ms": 0,
                                    "entities": 0,
                                    "edges": 0,
                                    "message": "unsupported language after detection",
                                })
                            );
                        }
                        stats.push(ParseExtractStat {
                            repo_relative_path: file.repo_relative_path,
                            parse_ms,
                            extraction_ms: 0,
                            bundle_ms: 0,
                            parse_error: false,
                            syntax_error: false,
                            skipped: true,
                            message: Some("unsupported language after detection".to_string()),
                            graph_output_budget_hits: Vec::new(),
                        });
                    }
                    Err(error) => {
                        let language = file.language.clone().or_else(|| {
                            detect_language(&file.repo_relative_path)
                                .map(|language| language.as_str().to_string())
                        });
                        let error_message = error.to_string();
                        let extraction = BasicExtraction {
                            file: FileRecord {
                                repo_relative_path: file.repo_relative_path.clone(),
                                file_hash: file.file_hash.clone(),
                                language: language.clone(),
                                size_bytes: file.size_bytes,
                                indexed_at_unix_ms: None,
                                metadata: parser_error_file_metadata(
                                    file.modified_unix_nanos.clone(),
                                    language.as_deref(),
                                    &error_message,
                                ),
                            },
                            entities: Vec::new(),
                            edges: Vec::new(),
                        };
                        let bundle_start = Instant::now();
                        outputs.push(LocalFactBundle::new(
                            file.repo_relative_path.clone(),
                            file.source,
                            file.needs_delete,
                            None,
                            file.template_required,
                            extraction,
                        ));
                        let bundle_ms = bundle_start.elapsed().as_millis();
                        if emit_profile_json {
                            eprintln!(
                                "{}",
                                json!({
                                    "event": "file_extract_completed",
                                    "repo_relative_path": &file.repo_relative_path,
                                    "status": "parse_error",
                                    "language": &language,
                                    "parse_ms": parse_ms,
                                    "extraction_ms": 0,
                                    "bundle_ms": bundle_ms,
                                    "entities": 0,
                                    "edges": 0,
                                    "message": &error_message,
                                })
                            );
                        }
                        stats.push(ParseExtractStat {
                            repo_relative_path: file.repo_relative_path,
                            parse_ms,
                            extraction_ms: 0,
                            bundle_ms,
                            parse_error: true,
                            syntax_error: false,
                            skipped: false,
                            message: Some(error_message),
                            graph_output_budget_hits: Vec::new(),
                        });
                    }
                }
            }
            (outputs, stats)
        }));
    }

    let mut outputs = Vec::new();
    let mut stats = Vec::new();
    for handle in handles {
        let (mut worker_outputs, mut worker_stats) = handle.join().map_err(|_| {
            IndexError::Message("parallel parse/extract worker panicked".to_string())
        })?;
        outputs.append(&mut worker_outputs);
        stats.append(&mut worker_stats);
    }
    outputs.sort_by(|left, right| left.repo_relative_path.cmp(&right.repo_relative_path));
    Ok((outputs, stats))
}

fn rate_per_second(count: usize, elapsed_ms: u128) -> f64 {
    if elapsed_ms == 0 {
        return count as f64;
    }
    (count as f64) / (elapsed_ms as f64 / 1_000.0)
}

fn current_process_memory_bytes() -> Option<u64> {
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexedPathFacts {
    has_file_record: bool,
    entity_ids: Vec<String>,
}

impl IndexedPathFacts {
    fn has_existing_facts(&self) -> bool {
        self.has_file_record || !self.entity_ids.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathCleanupReason {
    Deleted,
    NowIgnored,
    ScopeChanged,
    Replaced,
}

impl PathCleanupReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Deleted => "deleted",
            Self::NowIgnored => "now_ignored",
            Self::ScopeChanged => "scope_changed",
            Self::Replaced => "replaced",
        }
    }
}

fn path_has_indexed_facts(
    store: &SqliteGraphStore,
    repo_relative_path: &str,
) -> Result<IndexedPathFacts, IndexError> {
    let has_file_record = store.get_file(repo_relative_path)?.is_some();
    let entity_ids = store
        .list_entities_by_file(repo_relative_path)?
        .into_iter()
        .map(|entity| entity.id)
        .collect::<Vec<_>>();
    Ok(IndexedPathFacts {
        has_file_record,
        entity_ids,
    })
}

fn record_path_cleanup_reason(
    summary: &mut IncrementalIndexSummary,
    repo_relative_path: &str,
    reason: PathCleanupReason,
) {
    let entry = summary
        .path_cleanup_reasons
        .entry(normalize_graph_path(repo_relative_path))
        .or_default();
    let reason = reason.as_str().to_string();
    if !entry.contains(&reason) {
        entry.push(reason);
    }
}

#[allow(clippy::too_many_arguments)]
fn cleanup_facts_for_path(
    store: &SqliteGraphStore,
    cache: &IncrementalIndexCache,
    repo_relative_path: &str,
    reason: PathCleanupReason,
    summary: &mut IncrementalIndexSummary,
    changed_fact_paths: &mut BTreeSet<String>,
    removed_cache_entity_ids: &mut Vec<String>,
    changed_static_resolver_inputs: &mut bool,
    phase_profile: &mut IndexPhaseRecorder,
) -> Result<bool, IndexError> {
    let facts = path_has_indexed_facts(store, repo_relative_path)?;
    if !facts.has_existing_facts() {
        return Ok(false);
    }

    if path_has_static_resolver_language(repo_relative_path) {
        *changed_static_resolver_inputs = true;
    }
    if cache.has_cached_facts() {
        let cache_lookup_start = Instant::now();
        removed_cache_entity_ids.extend(facts.entity_ids);
        phase_profile.add_duration(
            "cache_old_fact_lookup",
            cache_lookup_start.elapsed(),
            1,
            removed_cache_entity_ids.len() as u64,
        );
    }
    let delete_start = Instant::now();
    store.delete_facts_for_file(repo_relative_path)?;
    phase_profile.add_duration("stale_fact_delete", delete_start.elapsed(), 1, 1);
    changed_fact_paths.insert(normalize_graph_path(repo_relative_path));
    summary.deleted_fact_files += 1;
    record_path_cleanup_reason(summary, repo_relative_path, reason);
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn cleanup_missing_indexed_paths_to_writer(
    store: &SqliteGraphStore,
    cache: &IncrementalIndexCache,
    repo_root: &Path,
    changed_paths: &BTreeSet<String>,
    summary: &mut IncrementalIndexSummary,
    changed_fact_paths: &mut BTreeSet<String>,
    removed_cache_entity_ids: &mut Vec<String>,
    changed_static_resolver_inputs: &mut bool,
    phase_profile: &mut IndexPhaseRecorder,
) -> Result<usize, IndexError> {
    let mut cleaned = 0usize;
    for file in store.list_files(UNBOUNDED_STORE_READ_LIMIT)? {
        let repo_relative_path = normalize_graph_path(&file.repo_relative_path);
        if changed_paths.contains(&repo_relative_path) {
            continue;
        }
        if repo_root.join(&repo_relative_path).exists() {
            continue;
        }
        if cleanup_facts_for_path(
            store,
            cache,
            &repo_relative_path,
            PathCleanupReason::Deleted,
            summary,
            changed_fact_paths,
            removed_cache_entity_ids,
            changed_static_resolver_inputs,
            phase_profile,
        )? {
            summary.deleted_file_facts_removed += 1;
            summary.files_deleted += 1;
            cleaned += 1;
        }
    }
    Ok(cleaned)
}

pub fn update_changed_files(
    repo_path: &Path,
    changed_paths: &[PathBuf],
) -> Result<IncrementalIndexSummary, IndexError> {
    let repo_root = resolve_repo_root_for_index(repo_path)?;
    let db_path = default_db_path(&repo_root);
    update_changed_files_to_db(&repo_root, changed_paths, &db_path)
}

pub fn update_changed_files_to_db(
    repo_path: &Path,
    changed_paths: &[PathBuf],
    db_path: &Path,
) -> Result<IncrementalIndexSummary, IndexError> {
    let mut cache = IncrementalIndexCache::new(256)?;
    update_changed_files_with_cache_to_db(repo_path, changed_paths, db_path, &mut cache)
}

pub fn update_changed_files_with_cache(
    repo_path: &Path,
    changed_paths: &[PathBuf],
    cache: &mut IncrementalIndexCache,
) -> Result<IncrementalIndexSummary, IndexError> {
    let repo_root = resolve_repo_root_for_index(repo_path)?;
    let db_path = default_db_path(&repo_root);
    update_changed_files_with_cache_to_db(&repo_root, changed_paths, &db_path, cache)
}

pub fn update_changed_files_with_cache_to_db(
    repo_path: &Path,
    changed_paths: &[PathBuf],
    db_path: &Path,
    cache: &mut IncrementalIndexCache,
) -> Result<IncrementalIndexSummary, IndexError> {
    let total_start = Instant::now();
    reset_sqlite_profile();
    let mut phase_profile = IndexPhaseRecorder::default();
    let repo_root = resolve_repo_root_for_index(repo_path)?;
    let db_path = normalize_db_path(&repo_root, db_path);
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let db_preflight = require_db_lifecycle_preflight(&repo_root, &db_path, None)?;
    let update_scope = db_preflight
        .effective_scope_policy
        .clone()
        .unwrap_or_else(IndexScopeOptions::default);
    let storage_mode = db_preflight
        .db_health
        .passport
        .as_ref()
        .and_then(|passport| passport.storage_mode.parse::<StorageMode>().ok())
        .unwrap_or_default();
    let previous_passport = db_preflight.db_health.passport.clone();
    let open_start = Instant::now();
    let store = SqliteGraphStore::open(&db_path)?;
    phase_profile.add_duration("open_store", open_start.elapsed(), 1, 0);
    let parser = TreeSitterParser;
    let indexed_at = unix_time_ms();
    let mut summary = IncrementalIndexSummary {
        status: "updated".to_string(),
        repo_root: path_string(&repo_root),
        db_path: path_string(&db_path),
        changed_files: Vec::new(),
        dependency_closure: RtdsDependencyClosureSummary::default(),
        files_seen: 0,
        files_walked: 0,
        files_metadata_unchanged: 0,
        files_read: 0,
        files_hashed: 0,
        files_parsed: 0,
        files_indexed: 0,
        files_deleted: 0,
        files_renamed: 0,
        files_skipped: 0,
        files_ignored: 0,
        parse_errors: 0,
        syntax_errors: 0,
        entities: 0,
        edges: 0,
        duplicate_edges_upserted: 0,
        binary_signatures_updated: 0,
        adjacency_edges: 0,
        deleted_fact_files: 0,
        dirty_path_evidence_count: 0,
        ignored_paths_seen: 0,
        ignored_paths_with_existing_facts: 0,
        stale_facts_deleted_for_ignored_paths: 0,
        deleted_file_facts_removed: 0,
        path_cleanup_reasons: BTreeMap::new(),
        global_hash_check_ran: false,
        storage_audit_ran: false,
        integrity_check_ran: false,
        profile: None,
    };

    let metadata_start = Instant::now();
    let requested_normalized =
        normalize_changed_paths_for_update(&repo_root, changed_paths, &store)?;
    let mut dependency_closure =
        rtds_dependency_closure_for_changed_paths(&repo_root, &store, &requested_normalized)?;
    let mut expanded_changed_paths = changed_paths.to_vec();
    for repo_relative_path in &dependency_closure.closure_files_considered {
        if !dependency_closure
            .requested_changed_files
            .iter()
            .any(|requested| {
                platform_path_identity_key(requested)
                    == platform_path_identity_key(repo_relative_path)
            })
        {
            expanded_changed_paths.push(repo_root.join(repo_relative_path));
        }
    }
    let normalized =
        normalize_changed_paths_for_update(&repo_root, &expanded_changed_paths, &store)?;
    summary.changed_files = normalized
        .iter()
        .map(|(_, repo_relative_path)| repo_relative_path.clone())
        .collect();
    dependency_closure.closure_files_updated = summary.changed_files.clone();
    summary.dependency_closure = dependency_closure;
    phase_profile.add_ms("file_walk", 0.0, 1, normalized.len() as u64);
    phase_profile.add_duration(
        "metadata_diff",
        metadata_start.elapsed(),
        1,
        normalized.len() as u64,
    );

    let mut changed_fact_paths = BTreeSet::<String>::new();
    let mut removed_cache_entity_ids = Vec::<String>::new();
    let mut changed_cache_entities = Vec::<Entity>::new();
    let mut changed_cache_edges = Vec::<Edge>::new();
    let mut dirty_path_evidence_edges = Vec::<Edge>::new();
    let mut changed_static_resolver_inputs = false;
    let transaction_begin_start = Instant::now();
    store.begin_write_transaction()?;
    phase_profile.add_duration("transaction_begin", transaction_begin_start.elapsed(), 1, 0);
    let transaction_result = (|| -> Result<_, IndexError> {
        let tx = &store;
        write_path_chaos_failpoint("incremental_before_stale_cleanup")?;
        for (file_path, repo_relative_path) in &normalized {
            summary.files_seen += 1;
            summary.files_walked += 1;
            let existing_facts = path_has_indexed_facts(tx, repo_relative_path)?;
            if !file_path.exists() || !file_path.is_file() {
                if existing_facts.has_existing_facts()
                    && cleanup_facts_for_path(
                        tx,
                        cache,
                        repo_relative_path,
                        PathCleanupReason::Deleted,
                        &mut summary,
                        &mut changed_fact_paths,
                        &mut removed_cache_entity_ids,
                        &mut changed_static_resolver_inputs,
                        &mut phase_profile,
                    )?
                {
                    summary.deleted_file_facts_removed += 1;
                    changed_fact_paths.insert(normalize_graph_path(repo_relative_path));
                    summary.files_deleted += 1;
                }
                continue;
            }

            if should_ignore_path_with_scope(&repo_root, file_path, &update_scope) {
                summary.ignored_paths_seen += 1;
                if existing_facts.has_existing_facts() {
                    summary.ignored_paths_with_existing_facts += 1;
                    if cleanup_facts_for_path(
                        tx,
                        cache,
                        repo_relative_path,
                        PathCleanupReason::NowIgnored,
                        &mut summary,
                        &mut changed_fact_paths,
                        &mut removed_cache_entity_ids,
                        &mut changed_static_resolver_inputs,
                        &mut phase_profile,
                    )? {
                        record_path_cleanup_reason(
                            &mut summary,
                            repo_relative_path,
                            PathCleanupReason::ScopeChanged,
                        );
                        summary.stale_facts_deleted_for_ignored_paths += 1;
                        summary.files_deleted += 1;
                    }
                }
                summary.files_ignored += 1;
                continue;
            }

            let Some(language) = detect_language(file_path) else {
                let Some(kind) = classify_scoped_text_evidence_path(repo_relative_path) else {
                    summary.files_skipped += 1;
                    continue;
                };

                let metadata_start = Instant::now();
                let file_metadata = fs::metadata(file_path)?;
                let size_bytes = file_metadata.len();
                let existing_file = tx.get_file(repo_relative_path)?;
                let existing_is_text_evidence = existing_file
                    .as_ref()
                    .is_some_and(|record| file_record_is_text_evidence_kind(record, kind));
                if existing_file.as_ref().is_some_and(|record| {
                    existing_is_text_evidence
                        && manifest_metadata_matches(record, size_bytes, &file_metadata)
                }) {
                    phase_profile.add_duration("metadata_diff", metadata_start.elapsed(), 1, 1);
                    summary.files_skipped += 1;
                    summary.files_metadata_unchanged += 1;
                    continue;
                }
                phase_profile.add_duration("metadata_diff", metadata_start.elapsed(), 1, 1);

                if size_bytes > TEXT_EVIDENCE_MAX_READ_BYTES_PER_FILE {
                    if existing_facts.has_existing_facts()
                        && cleanup_facts_for_path(
                            tx,
                            cache,
                            repo_relative_path,
                            PathCleanupReason::Replaced,
                            &mut summary,
                            &mut changed_fact_paths,
                            &mut removed_cache_entity_ids,
                            &mut changed_static_resolver_inputs,
                            &mut phase_profile,
                        )?
                    {
                        summary.files_deleted += 1;
                    }
                    summary.files_skipped += 1;
                    continue;
                }

                let read_start = Instant::now();
                let source = fs::read_to_string(file_path)?;
                phase_profile.add_duration(
                    "file_read",
                    read_start.elapsed(),
                    1,
                    source.len() as u64,
                );
                summary.files_read += 1;

                if !looks_like_text_evidence_source(&source) {
                    if existing_facts.has_existing_facts()
                        && cleanup_facts_for_path(
                            tx,
                            cache,
                            repo_relative_path,
                            PathCleanupReason::Replaced,
                            &mut summary,
                            &mut changed_fact_paths,
                            &mut removed_cache_entity_ids,
                            &mut changed_static_resolver_inputs,
                            &mut phase_profile,
                        )?
                    {
                        summary.files_deleted += 1;
                    }
                    summary.files_skipped += 1;
                    continue;
                }

                let hash_start = Instant::now();
                let hash = content_hash(&source);
                phase_profile.add_duration(
                    "file_hash",
                    hash_start.elapsed(),
                    1,
                    source.len() as u64,
                );
                summary.files_hashed += 1;

                if let Some(record) = existing_file
                    .as_ref()
                    .filter(|record| record.file_hash == hash && existing_is_text_evidence)
                {
                    let evidence = build_text_evidence_index(repo_relative_path, &source);
                    tx.upsert_file(&FileRecord {
                        repo_relative_path: repo_relative_path.clone(),
                        file_hash: record.file_hash.clone(),
                        language: None,
                        size_bytes,
                        indexed_at_unix_ms: Some(indexed_at),
                        metadata: text_evidence_file_metadata(
                            modified_unix_nanos(&file_metadata),
                            kind,
                            &evidence,
                        ),
                    })?;
                    summary.files_skipped += 1;
                    continue;
                }

                let rename_detection_start = Instant::now();
                let renamed_stale = if existing_file.is_none() {
                    delete_missing_files_with_hash(tx, &repo_root, repo_relative_path, &hash)?
                } else {
                    0
                };
                phase_profile.add_duration(
                    "rename_detection",
                    rename_detection_start.elapsed(),
                    1,
                    renamed_stale as u64,
                );
                summary.files_deleted += renamed_stale;
                summary.files_renamed += renamed_stale;
                summary.deleted_fact_files += renamed_stale;

                if existing_facts.has_existing_facts() {
                    cleanup_facts_for_path(
                        tx,
                        cache,
                        repo_relative_path,
                        PathCleanupReason::Replaced,
                        &mut summary,
                        &mut changed_fact_paths,
                        &mut removed_cache_entity_ids,
                        &mut changed_static_resolver_inputs,
                        &mut phase_profile,
                    )?;
                }

                let evidence = build_text_evidence_index(repo_relative_path, &source);
                let write_start = Instant::now();
                persist_text_evidence_to_writer(
                    tx,
                    repo_relative_path,
                    &hash,
                    kind,
                    &evidence,
                    size_bytes,
                    indexed_at,
                    modified_unix_nanos(&file_metadata),
                    false,
                )?;
                phase_profile.add_duration("text_evidence_upsert", write_start.elapsed(), 1, 1);
                summary.files_indexed += 1;
                changed_fact_paths.insert(normalize_graph_path(repo_relative_path));
                continue;
            };
            if language_supports_static_resolver(language.as_str()) {
                changed_static_resolver_inputs = true;
            }

            let metadata_start = Instant::now();
            let file_metadata =
                fs::metadata(file_path).map_err(|error| StoreError::Message(error.to_string()))?;
            let size_bytes = file_metadata.len();
            let existing_file = tx.get_file(repo_relative_path)?;
            if existing_file
                .as_ref()
                .is_some_and(|record| manifest_metadata_matches(record, size_bytes, &file_metadata))
            {
                phase_profile.add_duration("metadata_diff", metadata_start.elapsed(), 1, 1);
                summary.files_skipped += 1;
                summary.files_metadata_unchanged += 1;
                continue;
            }
            phase_profile.add_duration("metadata_diff", metadata_start.elapsed(), 1, 1);

            let read_start = Instant::now();
            let source = fs::read_to_string(file_path)
                .map_err(|error| StoreError::Message(error.to_string()))?;
            phase_profile.add_duration("file_read", read_start.elapsed(), 1, source.len() as u64);
            summary.files_read += 1;
            let hash_start = Instant::now();
            let hash = content_hash(&source);
            phase_profile.add_duration("file_hash", hash_start.elapsed(), 1, source.len() as u64);
            summary.files_hashed += 1;
            if let Some(record) = existing_file
                .as_ref()
                .filter(|record| record.file_hash == hash)
            {
                tx.upsert_file(&FileRecord {
                    repo_relative_path: repo_relative_path.clone(),
                    file_hash: record.file_hash.clone(),
                    language: Some(language.as_str().to_string()),
                    size_bytes,
                    indexed_at_unix_ms: Some(indexed_at),
                    metadata: file_manifest_metadata(modified_unix_nanos(&file_metadata)),
                })?;
                summary.files_skipped += 1;
                continue;
            }
            let rename_detection_start = Instant::now();
            let renamed_stale = if existing_file.is_none() {
                delete_missing_files_with_hash(tx, &repo_root, repo_relative_path, &hash)?
            } else {
                0
            };
            phase_profile.add_duration(
                "rename_detection",
                rename_detection_start.elapsed(),
                1,
                renamed_stale as u64,
            );
            summary.files_deleted += renamed_stale;
            summary.files_renamed += renamed_stale;
            summary.deleted_fact_files += renamed_stale;
            changed_fact_paths.insert(normalize_graph_path(repo_relative_path));
            cleanup_facts_for_path(
                tx,
                cache,
                repo_relative_path,
                PathCleanupReason::Replaced,
                &mut summary,
                &mut changed_fact_paths,
                &mut removed_cache_entity_ids,
                &mut changed_static_resolver_inputs,
                &mut phase_profile,
            )?;
            write_path_chaos_failpoint("incremental_after_stale_cleanup_before_insert")?;
            summary.files_parsed += 1;
            let parse_start = Instant::now();
            let parsed = match parser.parse(repo_relative_path, &source) {
                Ok(Some(parsed)) => parsed,
                Ok(None) => {
                    summary.files_skipped += 1;
                    continue;
                }
                Err(error) => {
                    summary.parse_errors += 1;
                    let error_message = error.to_string();
                    tx.upsert_file(&FileRecord {
                        repo_relative_path: repo_relative_path.clone(),
                        file_hash: hash.clone(),
                        language: Some(language.as_str().to_string()),
                        size_bytes,
                        indexed_at_unix_ms: Some(indexed_at),
                        metadata: parser_error_file_metadata(
                            modified_unix_nanos(&file_metadata),
                            Some(language.as_str()),
                            &error_message,
                        ),
                    })?;
                    continue;
                }
            };
            phase_profile.add_duration("parse", parse_start.elapsed(), 1, 1);
            if parsed.has_syntax_errors() {
                summary.syntax_errors += 1;
            }

            let extraction_start = Instant::now();
            let mut extraction = extract_entities_and_relations(&parsed, &source);
            phase_profile.add_duration(
                "extract_entities_and_relations",
                extraction_start.elapsed(),
                1,
                1,
            );
            extraction.file.size_bytes = size_bytes;
            extraction.file.indexed_at_unix_ms = Some(indexed_at);
            extraction.file.metadata = file_manifest_metadata_with_parser_status(
                modified_unix_nanos(&file_metadata),
                extraction.file.metadata.clone(),
            );
            let mut snippets = None;
            let file_start = Instant::now();
            tx.upsert_file(&extraction.file)?;
            phase_profile.add_duration("file_manifest_upsert", file_start.elapsed(), 1, 1);
            let source_text_start = Instant::now();
            tx.insert_file_text_after_file_delete(repo_relative_path, &source)?;
            phase_profile.add_duration(
                "source_text_evidence_upsert",
                source_text_start.elapsed(),
                1,
                source.len() as u64,
            );

            let persisted_entity_ids = persisted_entity_ids(&extraction.entities, storage_mode);
            let mut entity_count = 0usize;
            let mut edge_count = 0usize;

            for entity in extraction
                .entities
                .iter()
                .filter(|entity| should_persist_entity(entity, storage_mode))
            {
                let entity_start = Instant::now();
                write_path_chaos_failpoint("incremental_during_entity_insert")?;
                if should_index_entity_text(entity) {
                    tx.insert_entity_after_file_delete(entity)?;
                } else {
                    tx.insert_entity_record_after_file_delete(entity)?;
                }
                phase_profile.add_duration("entity_insert", entity_start.elapsed(), 1, 1);
                if let Some(span) = &entity.source_span {
                    if should_index_entity_snippet(entity) {
                        let snippet_start = Instant::now();
                        let snippets =
                            snippets.get_or_insert_with(|| SourceSnippetCache::new(&source));
                        tx.insert_snippet_text_after_file_delete(
                            &entity.id,
                            span,
                            &snippets.snippet(span),
                        )?;
                        phase_profile.add_duration(
                            "snippet_loading",
                            snippet_start.elapsed(),
                            1,
                            1,
                        );
                    }
                }
                changed_cache_entities.push(entity.clone());
                entity_count += 1;
            }

            for edge in extraction
                .edges
                .iter()
                .filter(|edge| should_persist_edge(edge, &persisted_entity_ids))
            {
                if should_store_edge_row(edge) {
                    let edge_start = Instant::now();
                    write_path_chaos_failpoint("incremental_during_edge_insert")?;
                    tx.insert_edge_after_file_delete(edge)?;
                    phase_profile.add_duration("edge_insert", edge_start.elapsed(), 1, 1);
                    changed_cache_edges.push(edge.clone());
                    if should_persist_stored_path_evidence_for_edge(edge) {
                        dirty_path_evidence_edges.push(edge.clone());
                    }
                }
                edge_count += 1;
            }

            summary.files_indexed += 1;
            summary.entities += entity_count;
            summary.edges += edge_count;
            summary.binary_signatures_updated += entity_count;
        }

        let stale_missing_scan_start = Instant::now();
        let changed_path_set = normalized
            .iter()
            .map(|(_, repo_relative_path)| normalize_graph_path(repo_relative_path))
            .collect::<BTreeSet<_>>();
        let stale_missing_deleted = cleanup_missing_indexed_paths_to_writer(
            tx,
            cache,
            &repo_root,
            &changed_path_set,
            &mut summary,
            &mut changed_fact_paths,
            &mut removed_cache_entity_ids,
            &mut changed_static_resolver_inputs,
            &mut phase_profile,
        )?;
        phase_profile.add_duration(
            "stale_missing_manifest_scan",
            stale_missing_scan_start.elapsed(),
            1,
            stale_missing_deleted as u64,
        );

        let (import_resolution, security_resolution, test_resolution, derived_resolution) =
            if changed_fact_paths.is_empty() || !changed_static_resolver_inputs {
                if !changed_fact_paths.is_empty() {
                    phase_profile.add_ms(
                        "resolver_impact_skip",
                        0.0,
                        1,
                        changed_fact_paths.len() as u64,
                    );
                }
                (
                    GlobalFactApplySummary::default(),
                    GlobalFactApplySummary::default(),
                    GlobalFactApplySummary::default(),
                    GlobalFactApplySummary::default(),
                )
            } else {
                let impact_start = Instant::now();
                let resolver_impact_paths =
                    resolver_impact_paths_from_store(&repo_root, tx, &changed_fact_paths)
                        .map_err(index_error_as_store_error)?;
                phase_profile.add_duration(
                    "resolver_impact_analysis",
                    impact_start.elapsed(),
                    1,
                    resolver_impact_paths.len() as u64,
                );
                let static_source_start = Instant::now();
                let has_static_sources =
                    resolver_impact_paths_have_static_sources(tx, &resolver_impact_paths)
                        .map_err(index_error_as_store_error)?;
                phase_profile.add_duration(
                    "resolver_static_source_check",
                    static_source_start.elapsed(),
                    1,
                    resolver_impact_paths.len() as u64,
                );
                if has_static_sources {
                    let mut import_plan = reduce_static_import_edges_from_store(&repo_root, tx)
                        .map_err(index_error_as_store_error)?;
                    import_plan.retain_paths(&resolver_impact_paths);
                    import_plan.sort();
                    changed_cache_entities
                        .extend(import_plan.entities.iter().map(|fact| fact.entity.clone()));
                    changed_cache_edges.extend(import_plan.edges.iter().cloned());
                    dirty_path_evidence_edges.extend(
                        import_plan
                            .edges
                            .iter()
                            .filter(|edge| should_persist_stored_path_evidence_for_edge(edge))
                            .cloned(),
                    );
                    let import_resolution = apply_global_fact_reduction_plan_to_writer(
                        tx,
                        &import_plan,
                        &IndexOptions::default(),
                        &mut phase_profile,
                    )?;

                    let mut security_plan = reduce_security_edges_from_store(&repo_root, tx)
                        .map_err(index_error_as_store_error)?;
                    security_plan.retain_paths(&resolver_impact_paths);
                    security_plan.sort();
                    changed_cache_entities.extend(
                        security_plan
                            .entities
                            .iter()
                            .map(|fact| fact.entity.clone()),
                    );
                    changed_cache_edges.extend(security_plan.edges.iter().cloned());
                    dirty_path_evidence_edges.extend(
                        security_plan
                            .edges
                            .iter()
                            .filter(|edge| should_persist_stored_path_evidence_for_edge(edge))
                            .cloned(),
                    );
                    let security_resolution = apply_global_fact_reduction_plan_to_writer(
                        tx,
                        &security_plan,
                        &IndexOptions::default(),
                        &mut phase_profile,
                    )?;

                    let mut test_plan = reduce_test_edges_from_store(&repo_root, tx)
                        .map_err(index_error_as_store_error)?;
                    test_plan.retain_paths(&resolver_impact_paths);
                    test_plan.sort();
                    changed_cache_entities
                        .extend(test_plan.entities.iter().map(|fact| fact.entity.clone()));
                    changed_cache_edges.extend(test_plan.edges.iter().cloned());
                    dirty_path_evidence_edges.extend(
                        test_plan
                            .edges
                            .iter()
                            .filter(|edge| should_persist_stored_path_evidence_for_edge(edge))
                            .cloned(),
                    );
                    let test_resolution = apply_global_fact_reduction_plan_to_writer(
                        tx,
                        &test_plan,
                        &IndexOptions::default(),
                        &mut phase_profile,
                    )?;
                    let mut derived_plan = reduce_derived_mutation_edges_from_store(tx)
                        .map_err(index_error_as_store_error)?;
                    derived_plan.retain_paths(&resolver_impact_paths);
                    derived_plan.sort();
                    changed_cache_entities
                        .extend(derived_plan.entities.iter().map(|fact| fact.entity.clone()));
                    changed_cache_edges.extend(derived_plan.edges.iter().cloned());
                    dirty_path_evidence_edges.extend(
                        derived_plan
                            .edges
                            .iter()
                            .filter(|edge| should_persist_stored_path_evidence_for_edge(edge))
                            .cloned(),
                    );
                    let derived_resolution = apply_global_fact_reduction_plan_to_writer(
                        tx,
                        &derived_plan,
                        &IndexOptions::default(),
                        &mut phase_profile,
                    )?;
                    (
                        import_resolution,
                        security_resolution,
                        test_resolution,
                        derived_resolution,
                    )
                } else {
                    (
                        GlobalFactApplySummary::default(),
                        GlobalFactApplySummary::default(),
                        GlobalFactApplySummary::default(),
                        GlobalFactApplySummary::default(),
                    )
                }
            };

        if !changed_fact_paths.is_empty() {
            write_path_chaos_failpoint("incremental_before_path_evidence_refresh")?;
            summary.dirty_path_evidence_count = refresh_stored_path_evidence_for_edges_to_writer(
                tx,
                dirty_path_evidence_edges.clone(),
                DEFAULT_STORED_PATH_EVIDENCE_MAX_ROWS,
                &mut phase_profile,
            )?;
            let digest_start = Instant::now();
            for repo_relative_path in &changed_fact_paths {
                tx.update_incremental_graph_digest_for_file(repo_relative_path, Some(indexed_at))?;
            }
            phase_profile.add_duration(
                "graph_fact_hash",
                digest_start.elapsed(),
                changed_fact_paths.len() as u64,
                changed_fact_paths.len() as u64,
            );
        }

        let repo_id = format!("repo://{}", repo_root.display());
        let repo_state_read_start = Instant::now();
        let previous_state = tx.get_repo_index_state(&repo_id)?;
        phase_profile.add_duration(
            "repo_index_state_read",
            repo_state_read_start.elapsed(),
            1,
            if previous_state.is_some() { 1 } else { 0 },
        );
        let repo_state_count_start = Instant::now();
        let (files_indexed, entity_count, edge_count, count_mode) =
            if let Some(previous) = previous_state {
                (
                    previous
                        .files_indexed
                        .saturating_add(summary.files_indexed as u64)
                        .saturating_sub(summary.deleted_fact_files as u64),
                    previous.entity_count,
                    previous.edge_count,
                    "preserved_previous_counts_fast_update",
                )
            } else {
                (
                    tx.count_files()?,
                    tx.count_entities()?,
                    tx.count_edges()?,
                    "global_counts_fallback",
                )
            };
        phase_profile.add_duration(
            if count_mode == "global_counts_fallback" {
                "repo_state_global_count"
            } else {
                "repo_state_count_fast_path"
            },
            repo_state_count_start.elapsed(),
            1,
            0,
        );
        let mut state_metadata = Metadata::default();
        state_metadata.insert(
            "count_mode".to_string(),
            Value::from(count_mode.to_string()),
        );
        state_metadata.insert("update_mode".to_string(), Value::from("fast_path"));
        state_metadata.insert(
            "changed_file_count".to_string(),
            Value::from(summary.changed_files.len() as u64),
        );
        let state = RepoIndexState {
            repo_id,
            repo_root: path_string(&repo_root),
            repo_commit: None,
            schema_version: tx.schema_version()?,
            indexed_at_unix_ms: Some(indexed_at),
            files_indexed,
            entity_count,
            edge_count,
            metadata: state_metadata,
        };
        tx.upsert_repo_index_state(&state)?;
        if !changed_fact_paths.is_empty()
            || summary.files_indexed > 0
            || summary.files_deleted > 0
            || summary.files_renamed > 0
            || summary.stale_facts_deleted_for_ignored_paths > 0
        {
            let files_count = tx.count_files()?;
            let passport = build_incremental_db_passport(
                &repo_root,
                &update_scope,
                storage_mode,
                indexed_at,
                files_count,
                files_count,
                previous_passport.as_ref(),
            )?;
            tx.upsert_db_passport(&passport)?;
        }
        write_path_chaos_failpoint("incremental_after_insert_before_commit")?;

        Ok((
            import_resolution,
            security_resolution,
            test_resolution,
            derived_resolution,
        ))
    })();
    let (import_resolution, security_resolution, test_resolution, derived_resolution) =
        match transaction_result {
            Ok(result) => {
                let commit_start = Instant::now();
                if let Err(error) = write_path_chaos_failpoint("incremental_before_commit") {
                    let _ = store.rollback_write_transaction();
                    return Err(error);
                }
                if let Err(error) = store.commit_write_transaction() {
                    let _ = store.rollback_write_transaction();
                    return Err(error.into());
                }
                phase_profile.add_duration("transaction_commit", commit_start.elapsed(), 1, 0);
                result
            }
            Err(error) => {
                let rollback_start = Instant::now();
                let _ = store.rollback_write_transaction();
                phase_profile.add_duration("transaction_rollback", rollback_start.elapsed(), 1, 0);
                return Err(error);
            }
        };

    summary.entities += import_resolution.entities_inserted;
    summary.edges += import_resolution.edges_inserted;
    summary.duplicate_edges_upserted += import_resolution.edges_upserted_existing;
    summary.entities += security_resolution.entities_inserted;
    summary.edges += security_resolution.edges_inserted;
    summary.duplicate_edges_upserted += security_resolution.edges_upserted_existing;
    summary.entities += test_resolution.entities_inserted;
    summary.edges += test_resolution.edges_inserted;
    summary.duplicate_edges_upserted += test_resolution.edges_upserted_existing;
    summary.entities += derived_resolution.entities_inserted;
    summary.edges += derived_resolution.edges_inserted;
    summary.duplicate_edges_upserted += derived_resolution.edges_upserted_existing;

    phase_profile.extend_sqlite_profile();
    let checkpoint_start = Instant::now();
    store.wal_checkpoint_truncate()?;
    phase_profile.add_duration("wal_checkpoint", checkpoint_start.elapsed(), 1, 0);
    let cache_start = Instant::now();
    cache.refresh_from_changed_facts(
        &removed_cache_entity_ids,
        &changed_cache_entities,
        &changed_cache_edges,
    )?;
    phase_profile.add_duration("cache_refresh", cache_start.elapsed(), 1, 0);
    summary.adjacency_edges = cache.adjacency_edge_count();
    let total_wall_ms = total_start.elapsed().as_millis();
    let measured_db_write_ms = phase_profile.sum_spans_ms_u128(DB_WRITE_PROFILE_SPANS);
    let measured_fts_search_index_ms = phase_profile.span_ms_u128("fts_build");
    let memory_bytes = current_process_memory_bytes();
    summary.profile = Some(IndexProfile {
        file_discovery_ms: 0,
        parse_ms: phase_profile.span_ms_u128("parse"),
        extraction_ms: phase_profile.span_ms_u128("extract_entities_and_relations"),
        semantic_resolver_ms: phase_profile.span_ms_u128("reducer"),
        db_write_ms: measured_db_write_ms,
        fts_search_index_ms: measured_fts_search_index_ms,
        vector_signature_ms: 0,
        total_wall_ms,
        files_per_sec: rate_per_second(summary.files_indexed, total_wall_ms),
        entities_per_sec: rate_per_second(summary.entities, total_wall_ms),
        edges_per_sec: rate_per_second(summary.edges, total_wall_ms),
        memory_bytes,
        memory_measured: memory_bytes.is_some(),
        memory_status: if memory_bytes.is_some() {
            "measured".to_string()
        } else {
            "unknown".to_string()
        },
        memory_measurement_kind: if memory_bytes.is_some() {
            "process_snapshot_not_peak".to_string()
        } else {
            "not_measured".to_string()
        },
        db_write_measurement: "measured_sql_write_aggregate".to_string(),
        fts_search_index_measurement: if measured_fts_search_index_ms > 0 {
            "measured_fts_build_span".to_string()
        } else {
            "unknown_or_not_run".to_string()
        },
        worker_count: 1,
        skipped_unchanged_files: summary.files_metadata_unchanged,
        spans: phase_profile.into_spans(),
    });
    Ok(summary)
}

fn rtds_dependency_closure_for_changed_paths(
    repo_root: &Path,
    store: &SqliteGraphStore,
    requested: &[(PathBuf, String)],
) -> Result<RtdsDependencyClosureSummary, IndexError> {
    if rtds_closure_failpoint_enabled("before_update") {
        return Err(IndexError::Message(
            "rtds_dependency_closure_failpoint:before_update".to_string(),
        ));
    }

    let start = Instant::now();
    let budgets = RtdsDependencyClosureBudget::default();
    let mut summary = RtdsDependencyClosureSummary {
        status: "ready".to_string(),
        requested_changed_files: requested
            .iter()
            .map(|(_, path)| normalize_graph_path(path))
            .collect(),
        closure_files_considered: requested
            .iter()
            .map(|(_, path)| normalize_graph_path(path))
            .collect(),
        budgets,
        ..RtdsDependencyClosureSummary::default()
    };
    sort_dedup_strings(&mut summary.requested_changed_files);
    sort_dedup_strings(&mut summary.closure_files_considered);

    let requested_path_set = summary
        .requested_changed_files
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut closure_path_set = requested_path_set.clone();
    let mut changed_entity_ids = BTreeSet::<String>::new();
    let mut rows_hydrated = 0usize;
    let mut source_bytes_considered = 0u64;

    for (_, repo_relative_path) in requested {
        let repo_relative_path = normalize_graph_path(repo_relative_path);
        if rtds_path_is_text_evidence_or_non_graph(store, &repo_relative_path)? {
            summary
                .skipped_relation_classes
                .push("text_evidence:not_graph_dependency".to_string());
        }

        let entities = store.list_entities_by_file(&repo_relative_path)?;
        rows_hydrated = rows_hydrated.saturating_add(entities.len());
        if rows_hydrated > summary.budgets.max_db_rows_hydrated {
            rtds_mark_closure_budget_hit(&mut summary, "db_rows_hydrated", "changed_file_entities");
            break;
        }
        for entity in entities {
            changed_entity_ids.insert(entity.id);
        }
    }

    if changed_entity_ids.is_empty() {
        rtds_finalize_dependency_closure_status(&mut summary);
        return Ok(summary);
    }

    let edge_count = store.count_edges()? as usize;
    if edge_count
        > summary
            .budgets
            .max_db_rows_hydrated
            .saturating_sub(rows_hydrated)
    {
        rtds_mark_closure_budget_hit(&mut summary, "db_rows_hydrated", "edge_scan");
    }
    let edge_limit = summary
        .budgets
        .max_edges_inspected
        .min(
            summary
                .budgets
                .max_db_rows_hydrated
                .saturating_sub(rows_hydrated),
        )
        .max(1);
    let edges = store.list_edges(edge_limit)?;
    let mut relation_counts = BTreeMap::<String, usize>::new();
    let mut relation_classes = BTreeSet::<String>::new();

    for edge in edges {
        if summary.closure_edges_inspected >= summary.budgets.max_edges_inspected {
            rtds_mark_closure_budget_hit(&mut summary, "edges_inspected", "edge_scan");
            break;
        }
        if start.elapsed() > Duration::from_millis(summary.budgets.max_wall_ms) {
            rtds_mark_closure_budget_hit(&mut summary, "wall_time", "edge_scan");
            break;
        }

        let head_changed = changed_entity_ids.contains(&edge.head_id);
        let tail_changed = changed_entity_ids.contains(&edge.tail_id);
        if !head_changed && !tail_changed {
            continue;
        }

        summary.closure_edges_inspected += 1;
        let Some(relation_class) =
            rtds_dependency_closure_relation_class(&edge, head_changed, tail_changed)
        else {
            if rtds_relation_is_unknown_for_dependency_closure(edge.relation) {
                summary
                    .closure_unknowns
                    .push(format!("unsupported_relation_class:{}", edge.relation));
                summary
                    .skipped_relation_classes
                    .push(edge.relation.to_string());
            }
            continue;
        };

        let relation_count = relation_counts
            .entry(relation_class.to_string())
            .and_modify(|count| *count += 1)
            .or_insert(1);
        if *relation_count > summary.budgets.max_per_relation {
            rtds_mark_closure_budget_hit(&mut summary, relation_class, "per_relation_limit");
            continue;
        }
        if !relation_classes.contains(relation_class)
            && relation_classes.len() >= summary.budgets.max_relation_classes
        {
            rtds_mark_closure_budget_hit(&mut summary, relation_class, "relation_class_limit");
            continue;
        }
        relation_classes.insert(relation_class.to_string());

        let dependent_path = normalize_graph_path(&edge.source_span.repo_relative_path);
        if requested_path_set.contains(&dependent_path) {
            continue;
        }
        if !repo_root.join(&dependent_path).exists() {
            summary
                .closure_unknowns
                .push(format!("dependent_path_missing:{dependent_path}"));
            continue;
        }
        if closure_path_set.contains(&dependent_path) {
            continue;
        }
        if closure_path_set.len() >= summary.budgets.max_dirty_files {
            rtds_mark_closure_budget_hit(&mut summary, relation_class, "dirty_file_limit");
            summary
                .closure_unknowns
                .push(format!("closure_file_omitted_over_budget:{dependent_path}"));
            continue;
        }
        let source_bytes = store
            .get_file(&dependent_path)?
            .map(|record| record.size_bytes)
            .or_else(|| {
                fs::metadata(repo_root.join(&dependent_path))
                    .ok()
                    .map(|metadata| metadata.len())
            })
            .unwrap_or(0);
        if source_bytes_considered.saturating_add(source_bytes) > summary.budgets.max_source_bytes {
            rtds_mark_closure_budget_hit(&mut summary, relation_class, "source_bytes_limit");
            summary.closure_unknowns.push(format!(
                "closure_file_omitted_source_bytes:{dependent_path}"
            ));
            continue;
        }
        source_bytes_considered = source_bytes_considered.saturating_add(source_bytes);
        closure_path_set.insert(dependent_path);
    }

    summary.closure_files_considered = closure_path_set.into_iter().collect();
    summary.closure_relation_classes = relation_classes.into_iter().collect();
    sort_dedup_strings(&mut summary.degraded_relation_classes);
    sort_dedup_strings(&mut summary.skipped_relation_classes);
    sort_dedup_strings(&mut summary.closure_unknowns);
    rtds_finalize_dependency_closure_status(&mut summary);
    Ok(summary)
}

fn rtds_dependency_closure_relation_class(
    edge: &Edge,
    head_changed: bool,
    tail_changed: bool,
) -> Option<&'static str> {
    if edge.derived || !rtds_edge_exactness_is_safe_for_dependency_closure(edge.exactness) {
        return None;
    }

    match edge.relation {
        RelationKind::Imports | RelationKind::Reexports => {
            tail_changed.then_some("direct_static_importer")
        }
        RelationKind::AliasedBy => (head_changed || tail_changed).then_some("direct_import_alias"),
        RelationKind::Calls => tail_changed.then_some("deleted_or_changed_callable_reference"),
        RelationKind::Reads | RelationKind::Writes => {
            tail_changed.then_some("direct_symbol_reference")
        }
        _ => None,
    }
}

fn rtds_edge_exactness_is_safe_for_dependency_closure(exactness: Exactness) -> bool {
    matches!(
        exactness,
        Exactness::Exact
            | Exactness::CompilerVerified
            | Exactness::LspVerified
            | Exactness::ParserVerified
    )
}

fn rtds_relation_is_unknown_for_dependency_closure(relation: RelationKind) -> bool {
    is_proof_path_relation(relation)
        && !matches!(
            relation,
            RelationKind::Contains
                | RelationKind::DefinedIn
                | RelationKind::Defines
                | RelationKind::Declares
                | RelationKind::Callee
                | RelationKind::Argument0
                | RelationKind::Argument1
                | RelationKind::ArgumentN
                | RelationKind::ReturnsTo
        )
}

fn rtds_path_is_text_evidence_or_non_graph(
    store: &SqliteGraphStore,
    repo_relative_path: &str,
) -> Result<bool, IndexError> {
    if let Some(kind) = classify_scoped_text_evidence_path(repo_relative_path) {
        if store
            .get_file(repo_relative_path)?
            .as_ref()
            .is_some_and(|record| file_record_is_text_evidence_kind(record, kind))
            || detect_language(Path::new(repo_relative_path)).is_none()
        {
            return Ok(true);
        }
    }
    Ok(store.get_file(repo_relative_path)?.is_some_and(|record| {
        record.language.is_none()
            && record.metadata.get("evidence_kind").and_then(Value::as_str)
                == Some(TEXT_EVIDENCE_KIND)
    }))
}

fn rtds_mark_closure_budget_hit(
    summary: &mut RtdsDependencyClosureSummary,
    relation_class: &str,
    reason: &str,
) {
    summary.closure_budget_hit = true;
    summary
        .degraded_relation_classes
        .push(relation_class.to_string());
    summary
        .closure_unknowns
        .push(format!("closure_budget_hit:{relation_class}:{reason}"));
    summary.manual_full_index_recommendation = Some(
        "run agent-use index --fresh for full dependency freshness if this dirty closure matters"
            .to_string(),
    );
}

fn rtds_finalize_dependency_closure_status(summary: &mut RtdsDependencyClosureSummary) {
    sort_dedup_strings(&mut summary.requested_changed_files);
    sort_dedup_strings(&mut summary.closure_files_considered);
    sort_dedup_strings(&mut summary.closure_relation_classes);
    sort_dedup_strings(&mut summary.degraded_relation_classes);
    sort_dedup_strings(&mut summary.skipped_relation_classes);
    sort_dedup_strings(&mut summary.closure_unknowns);
    summary.status = if summary.closure_budget_hit || !summary.closure_unknowns.is_empty() {
        "degraded".to_string()
    } else if summary.closure_files_considered.len() > summary.requested_changed_files.len() {
        "ready_with_dependency_closure".to_string()
    } else {
        "ready".to_string()
    };
    if summary.closure_budget_hit && summary.manual_full_index_recommendation.is_none() {
        summary.manual_full_index_recommendation = Some(
            "run agent-use index --fresh for full dependency freshness if this dirty closure matters"
                .to_string(),
        );
    }
}

fn sort_dedup_strings(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

fn resolver_impact_paths_from_store(
    repo_root: &Path,
    store: &SqliteGraphStore,
    changed_paths: &BTreeSet<String>,
) -> Result<BTreeSet<String>, IndexError> {
    let files = store.list_files(UNBOUNDED_STORE_READ_LIMIT)?;
    let indexed_paths = files
        .iter()
        .map(|file| normalize_graph_path(&file.repo_relative_path))
        .collect::<BTreeSet<_>>();
    let mut sources = BTreeMap::<String, String>::new();
    for file in &files {
        if !file
            .language
            .as_deref()
            .is_some_and(|language| language == "typescript" || language == "javascript")
        {
            continue;
        }
        let repo_relative_path = normalize_graph_path(&file.repo_relative_path);
        let source_path = repo_root.join(&repo_relative_path);
        if !source_path.exists() {
            continue;
        }
        sources.insert(repo_relative_path, fs::read_to_string(source_path)?);
    }

    let mut impacted = changed_paths.clone();
    loop {
        let previous_len = impacted.len();
        for (repo_relative_path, source) in &sources {
            if static_dependency_targets_any(repo_relative_path, source, &indexed_paths, &impacted)
            {
                impacted.insert(repo_relative_path.clone());
            }
        }
        if impacted.len() == previous_len {
            break;
        }
    }
    Ok(impacted)
}

fn path_has_static_resolver_language(repo_relative_path: &str) -> bool {
    matches!(
        Path::new(repo_relative_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase()),
        Some(extension)
            if matches!(
                extension.as_str(),
                "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs"
            )
    )
}

fn language_supports_static_resolver(language: &str) -> bool {
    matches!(language, "typescript" | "javascript")
}

fn static_dependency_targets_any(
    repo_relative_path: &str,
    source: &str,
    indexed_paths: &BTreeSet<String>,
    target_paths: &BTreeSet<String>,
) -> bool {
    parse_static_imports(repo_relative_path, source)
        .iter()
        .any(|spec| {
            module_specifier_targets_any(
                repo_relative_path,
                &spec.module_specifier,
                indexed_paths,
                target_paths,
            )
        })
        || parse_static_reexports(repo_relative_path, source)
            .iter()
            .any(|spec| {
                module_specifier_targets_any(
                    repo_relative_path,
                    &spec.module_specifier,
                    indexed_paths,
                    target_paths,
                )
            })
}

fn module_specifier_targets_any(
    importer_path: &str,
    module_specifier: &str,
    indexed_paths: &BTreeSet<String>,
    target_paths: &BTreeSet<String>,
) -> bool {
    if !module_specifier.starts_with('.') {
        return false;
    }
    local_module_path_candidates(importer_path, module_specifier)
        .iter()
        .any(|candidate| target_paths.contains(candidate))
        || resolve_local_module_path(importer_path, module_specifier, indexed_paths)
            .is_some_and(|target| target_paths.contains(&target))
}

fn resolver_impact_paths_have_static_sources(
    store: &SqliteGraphStore,
    impacted_paths: &BTreeSet<String>,
) -> Result<bool, IndexError> {
    for file in store.list_files(UNBOUNDED_STORE_READ_LIMIT)? {
        if impacted_paths.contains(&normalize_graph_path(&file.repo_relative_path))
            && file
                .language
                .as_deref()
                .is_some_and(|language| language == "typescript" || language == "javascript")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Debug, Clone, Default)]
struct GlobalResolverWorkspace {
    resolver_paths: Vec<String>,
    test_paths: Vec<String>,
    entities_by_file: BTreeMap<String, Vec<Entity>>,
    indexed_paths: BTreeSet<String>,
    file_hashes: BTreeMap<String, String>,
    sources: BTreeMap<String, String>,
    has_test_case_entity: bool,
}

impl GlobalResolverWorkspace {
    fn load(repo_root: &Path, store: &SqliteGraphStore) -> Result<Self, IndexError> {
        let files = store.list_files(UNBOUNDED_STORE_READ_LIMIT)?;
        let indexed_paths = files
            .iter()
            .map(|file| normalize_graph_path(&file.repo_relative_path))
            .collect::<BTreeSet<_>>();
        let mut resolver_paths = Vec::new();
        let mut test_paths = Vec::new();
        let mut file_hashes = BTreeMap::new();
        for file in &files {
            let repo_relative_path = normalize_graph_path(&file.repo_relative_path);
            file_hashes.insert(repo_relative_path.clone(), file.file_hash.clone());
            if file
                .language
                .as_deref()
                .is_some_and(language_supports_static_resolver)
            {
                if is_test_file_path_for_index(&repo_relative_path) {
                    test_paths.push(repo_relative_path.clone());
                }
                resolver_paths.push(repo_relative_path);
            }
        }

        if resolver_paths.is_empty() {
            return Ok(Self {
                indexed_paths,
                file_hashes,
                ..Self::default()
            });
        }

        let mut sources = BTreeMap::new();
        for repo_relative_path in &resolver_paths {
            let source_path = repo_root.join(repo_relative_path);
            if source_path.exists() {
                sources.insert(
                    repo_relative_path.clone(),
                    fs::read_to_string(&source_path)?,
                );
            }
        }

        let mut entities_by_file = BTreeMap::<String, Vec<Entity>>::new();
        let mut has_test_case_entity = false;
        for entity in store.list_entities(UNBOUNDED_STORE_READ_LIMIT)? {
            if entity.kind == EntityKind::TestCase {
                has_test_case_entity = true;
            }
            entities_by_file
                .entry(normalize_graph_path(&entity.repo_relative_path))
                .or_default()
                .push(entity);
        }

        Ok(Self {
            resolver_paths,
            test_paths,
            entities_by_file,
            indexed_paths,
            file_hashes,
            sources,
            has_test_case_entity,
        })
    }

    fn file_hash(&self, repo_relative_path: &str) -> &str {
        self.file_hashes
            .get(repo_relative_path)
            .map(String::as_str)
            .unwrap_or("")
    }
}

fn reduce_static_import_edges_from_store(
    repo_root: &Path,
    store: &SqliteGraphStore,
) -> Result<GlobalFactReductionPlan, IndexError> {
    let workspace = GlobalResolverWorkspace::load(repo_root, store)?;
    reduce_static_import_edges_from_workspace(repo_root, &workspace)
}

fn reduce_static_import_edges_from_workspace(
    repo_root: &Path,
    workspace: &GlobalResolverWorkspace,
) -> Result<GlobalFactReductionPlan, IndexError> {
    if workspace.resolver_paths.is_empty() {
        return Ok(GlobalFactReductionPlan::default());
    }
    let mut plan = GlobalFactReductionPlan::default();
    for importer_path in &workspace.resolver_paths {
        let Some(source) = workspace.sources.get(importer_path) else {
            continue;
        };
        let file_hash = workspace.file_hash(importer_path);
        for spec in parse_static_imports(importer_path, source) {
            let Some(target_path) = resolve_local_module_path(
                importer_path,
                &spec.module_specifier,
                &workspace.indexed_paths,
            ) else {
                continue;
            };
            let Some(target) = resolve_import_target_cached(
                repo_root,
                &workspace.entities_by_file,
                &workspace.indexed_paths,
                Some(&workspace.sources),
                &target_path,
                &spec.imported_name,
                spec.kind,
            )?
            else {
                continue;
            };
            let import_entity = import_alias_entity(&spec, file_hash);
            plan.push_entity(import_entity.clone(), GlobalEntityWriteMode::UpsertIndexed);

            plan.push_edge(resolved_import_edge(
                &target.id,
                RelationKind::AliasedBy,
                &import_entity.id,
                &spec.span,
                file_hash,
                "static_named_import_alias",
            ));

            if let Some(file_entity) =
                file_entity_for_path(&workspace.entities_by_file, importer_path)
            {
                plan.push_edge(resolved_import_edge(
                    &file_entity.id,
                    RelationKind::Imports,
                    &target.id,
                    &spec.span,
                    file_hash,
                    "static_import_target",
                ));
            }

            for call_span in call_spans_for_local_name(source, importer_path, &spec.local_name) {
                if local_declaration_shadows_import(
                    &workspace.entities_by_file,
                    importer_path,
                    &spec.local_name,
                    &spec.span,
                    &call_span,
                ) {
                    continue;
                }
                let Some(scope) =
                    containing_executable(&workspace.entities_by_file, importer_path, &call_span)
                else {
                    continue;
                };
                plan.push_edge(resolved_import_edge(
                    &scope.id,
                    RelationKind::Calls,
                    &target.id,
                    &call_span,
                    file_hash,
                    "static_import_call_target",
                ));
            }
        }
        for spec in parse_dynamic_imports(importer_path, source) {
            let Some(scope) =
                containing_executable(&workspace.entities_by_file, importer_path, &spec.span)
            else {
                continue;
            };
            let import_entity = dynamic_import_entity(importer_path, &spec, file_hash);
            plan.push_entity(
                import_entity.clone(),
                GlobalEntityWriteMode::InsertRecordIfMissing,
            );
            plan.push_edge(unresolved_dynamic_import_edge(
                &scope.id,
                &import_entity.id,
                &spec.span,
                file_hash,
            ));
        }
    }
    plan.sort();
    Ok(plan)
}

fn reduce_security_edges_from_store(
    repo_root: &Path,
    store: &SqliteGraphStore,
) -> Result<GlobalFactReductionPlan, IndexError> {
    let workspace = GlobalResolverWorkspace::load(repo_root, store)?;
    reduce_security_edges_from_workspace(repo_root, &workspace)
}

fn reduce_security_edges_from_workspace(
    repo_root: &Path,
    workspace: &GlobalResolverWorkspace,
) -> Result<GlobalFactReductionPlan, IndexError> {
    if workspace.sources.is_empty() {
        return Ok(GlobalFactReductionPlan::default());
    }

    let mut role_targets = BTreeMap::<String, Entity>::new();
    let mut middleware_targets = BTreeMap::<String, Entity>::new();
    let mut security_entities = BTreeMap::<String, Entity>::new();
    let mut sanitizer_targets = BTreeMap::<String, Entity>::new();
    for (repo_relative_path, source) in &workspace.sources {
        let file_hash = workspace.file_hash(repo_relative_path);
        for entity in workspace
            .entities_by_file
            .get(repo_relative_path)
            .into_iter()
            .flatten()
            .filter(|entity| matches!(entity.kind, EntityKind::Function | EntityKind::Method))
        {
            if let Some(check) = direct_role_check_for_function(source, repo_relative_path, entity)
            {
                let role_entity = role_entity_for(
                    repo_relative_path,
                    &check.role,
                    &check.role_span,
                    file_hash,
                    "direct_role_literal_call",
                );
                role_targets.insert(entity.id.clone(), role_entity.clone());
                security_entities.insert(role_entity.id.clone(), role_entity);
                if is_role_middleware_helper_name(&entity.name) {
                    let middleware =
                        middleware_entity_for(entity, &check.call_span, file_hash, "role_helper");
                    middleware_targets.insert(entity.id.clone(), middleware.clone());
                    security_entities.insert(middleware.id.clone(), middleware);
                }
            } else if let Some((role, role_span)) = role_literal_for_function(source, entity) {
                let role_entity = role_entity_for(
                    repo_relative_path,
                    &role,
                    &role_span,
                    file_hash,
                    "role_helper_body_literal",
                );
                role_targets.insert(entity.id.clone(), role_entity.clone());
                security_entities.insert(role_entity.id.clone(), role_entity);
                if is_role_middleware_helper_name(&entity.name) {
                    let middleware =
                        middleware_entity_for(entity, &role_span, file_hash, "role_helper");
                    middleware_targets.insert(entity.id.clone(), middleware.clone());
                    security_entities.insert(middleware.id.clone(), middleware);
                }
            }
            if looks_like_sanitizer_name(&entity.name) {
                let span = entity
                    .source_span
                    .clone()
                    .unwrap_or_else(|| SourceSpan::new(repo_relative_path, 1, 1));
                let sanitizer_entity = sanitizer_entity_for(entity, &span, file_hash);
                sanitizer_targets.insert(entity.id.clone(), sanitizer_entity.clone());
                security_entities.insert(sanitizer_entity.id.clone(), sanitizer_entity);
            }
        }
    }

    let mut plan = GlobalFactReductionPlan::default();
    for entity in security_entities.values() {
        plan.push_entity(entity.clone(), GlobalEntityWriteMode::InsertRecordIfMissing);
    }

    for (repo_relative_path, source) in &workspace.sources {
        let file_hash = workspace.file_hash(repo_relative_path);

        for check in direct_role_check_calls(source, repo_relative_path) {
            let Some(scope) = containing_executable(
                &workspace.entities_by_file,
                repo_relative_path,
                &check.call_span,
            ) else {
                continue;
            };
            let role_entity = role_entity_for(
                repo_relative_path,
                &check.role,
                &check.role_span,
                file_hash,
                "direct_role_literal_call",
            );
            plan.push_entity(
                role_entity.clone(),
                GlobalEntityWriteMode::InsertRecordIfMissing,
            );
            let head_id = middleware_targets
                .get(&scope.id)
                .map(|middleware| middleware.id.as_str())
                .unwrap_or(scope.id.as_str());
            plan.push_edge(resolved_security_edge(
                head_id,
                RelationKind::ChecksRole,
                &role_entity.id,
                &check.call_span,
                file_hash,
                "direct_role_literal_call",
            ));
        }

        for spec in parse_static_imports(repo_relative_path, source) {
            let Some(target_path) = resolve_local_module_path(
                repo_relative_path,
                &spec.module_specifier,
                &workspace.indexed_paths,
            ) else {
                continue;
            };
            let Some(target) = resolve_import_target_cached(
                repo_root,
                &workspace.entities_by_file,
                &workspace.indexed_paths,
                Some(&workspace.sources),
                &target_path,
                &spec.imported_name,
                spec.kind,
            )?
            else {
                continue;
            };

            if let Some(role_entity) = role_targets.get(&target.id) {
                for call in
                    call_records_for_local_name(source, repo_relative_path, &spec.local_name)
                {
                    if local_declaration_shadows_import(
                        &workspace.entities_by_file,
                        repo_relative_path,
                        &spec.local_name,
                        &spec.span,
                        &call.span,
                    ) {
                        continue;
                    }
                    let Some(scope) = containing_executable(
                        &workspace.entities_by_file,
                        repo_relative_path,
                        &call.span,
                    ) else {
                        continue;
                    };
                    plan.push_edge(resolved_security_edge(
                        &scope.id,
                        RelationKind::ChecksRole,
                        &role_entity.id,
                        &call.span,
                        file_hash,
                        "imported_role_helper_call",
                    ));
                }
            }

            if let Some(sanitizer_entity) = sanitizer_targets.get(&target.id) {
                let assignments = local_security_assignments(source, repo_relative_path);
                for call in
                    call_records_for_local_name(source, repo_relative_path, &spec.local_name)
                {
                    if local_declaration_shadows_import(
                        &workspace.entities_by_file,
                        repo_relative_path,
                        &spec.local_name,
                        &spec.span,
                        &call.span,
                    ) {
                        continue;
                    }
                    let Some((argument, argument_span)) = single_argument(&call) else {
                        continue;
                    };
                    let sanitized_entity = if looks_like_property_access(&argument) {
                        property_entity_for(
                            repo_relative_path,
                            &argument,
                            &argument_span,
                            file_hash,
                            "sanitizer_argument_property",
                        )
                    } else if let Some(root_assignment) =
                        root_local_assignment(&assignments, &argument)
                    {
                        local_variable_entity_for_assignment(
                            &workspace.entities_by_file,
                            repo_relative_path,
                            root_assignment,
                            file_hash,
                        )
                    } else {
                        continue;
                    };
                    plan.push_entity(
                        sanitized_entity.clone(),
                        GlobalEntityWriteMode::InsertRecordIfMissing,
                    );
                    plan.push_edge(resolved_security_edge(
                        &sanitizer_entity.id,
                        RelationKind::Sanitizes,
                        &sanitized_entity.id,
                        &call.span,
                        file_hash,
                        "direct_sanitizer_call_argument",
                    ));
                }
            }
        }

        let import_targets = resolved_import_targets_for_source_cached(
            repo_root,
            &workspace.entities_by_file,
            &workspace.indexed_paths,
            Some(&workspace.sources),
            repo_relative_path,
            source,
        )?;
        for route in route_exposure_specs(source, repo_relative_path) {
            let route_entity = route_entity_for(repo_relative_path, &route, file_hash);
            let endpoint_entity = endpoint_entity_for(repo_relative_path, &route, file_hash);
            plan.push_entity(
                route_entity.clone(),
                GlobalEntityWriteMode::InsertRecordIfMissing,
            );
            plan.push_entity(
                endpoint_entity.clone(),
                GlobalEntityWriteMode::InsertRecordIfMissing,
            );
            plan.push_edge(resolved_security_edge(
                &route_entity.id,
                RelationKind::Exposes,
                &endpoint_entity.id,
                &route.span,
                file_hash,
                "route_factory_literal",
            ));
            if let Some(guard_name) = route.guard_name.as_deref() {
                if let Some(target) = import_targets.get(guard_name).or_else(|| {
                    workspace
                        .entities_by_file
                        .get(repo_relative_path)
                        .and_then(|entities| {
                            entities.iter().find(|entity| {
                                matches!(entity.kind, EntityKind::Function | EntityKind::Method)
                                    && entity.name == guard_name
                            })
                        })
                }) {
                    if let Some(middleware) = middleware_targets.get(&target.id) {
                        plan.push_edge(resolved_security_edge(
                            &route_entity.id,
                            RelationKind::Authorizes,
                            &middleware.id,
                            &route.span,
                            file_hash,
                            "route_factory_guard_argument",
                        ));
                    }
                }
            }
        }

        for flow in local_property_flows(
            source,
            repo_relative_path,
            &workspace.entities_by_file,
            file_hash,
        ) {
            for entity in [&flow.head, &flow.tail] {
                plan.push_entity(entity.clone(), GlobalEntityWriteMode::InsertRecordIfMissing);
            }
            plan.push_edge(resolved_security_edge(
                &flow.head.id,
                RelationKind::FlowsTo,
                &flow.tail.id,
                &flow.span,
                file_hash,
                "local_property_binding_to_call_argument",
            ));
        }
        for flow in local_variable_call_flows(
            source,
            repo_relative_path,
            &workspace.entities_by_file,
            file_hash,
        ) {
            for entity in [&flow.head, &flow.tail] {
                plan.push_entity(entity.clone(), GlobalEntityWriteMode::InsertRecordIfMissing);
            }
            plan.push_edge(resolved_security_edge(
                &flow.head.id,
                RelationKind::FlowsTo,
                &flow.tail.id,
                &flow.span,
                file_hash,
                "local_variable_origin_to_call_target",
            ));
        }
    }
    plan.sort();
    Ok(plan)
}

fn reduce_test_edges_from_store(
    repo_root: &Path,
    store: &SqliteGraphStore,
) -> Result<GlobalFactReductionPlan, IndexError> {
    let workspace = GlobalResolverWorkspace::load(repo_root, store)?;
    reduce_test_edges_from_workspace(repo_root, &workspace)
}

fn reduce_test_edges_from_workspace(
    repo_root: &Path,
    workspace: &GlobalResolverWorkspace,
) -> Result<GlobalFactReductionPlan, IndexError> {
    if workspace.test_paths.is_empty() || !workspace.has_test_case_entity {
        return Ok(GlobalFactReductionPlan::default());
    }
    let mut maybe_has_test_relation = false;
    for repo_relative_path in &workspace.test_paths {
        let Some(source) = workspace.sources.get(repo_relative_path) else {
            continue;
        };
        if source_may_have_test_relation(&source) {
            maybe_has_test_relation = true;
            break;
        }
    }
    if !maybe_has_test_relation {
        return Ok(GlobalFactReductionPlan::default());
    }

    let mut plan = GlobalFactReductionPlan::default();
    for repo_relative_path in &workspace.test_paths {
        let Some(source) = workspace.sources.get(repo_relative_path) else {
            continue;
        };
        let file_hash = workspace.file_hash(repo_relative_path);
        let test_cases = workspace
            .entities_by_file
            .get(repo_relative_path)
            .into_iter()
            .flatten()
            .filter(|entity| entity.kind == EntityKind::TestCase)
            .cloned()
            .collect::<Vec<_>>();
        let import_targets = resolved_import_targets_for_source_cached(
            repo_root,
            &workspace.entities_by_file,
            &workspace.indexed_paths,
            Some(&workspace.sources),
            repo_relative_path,
            source,
        )?;
        let import_spans_by_local = parse_static_imports(repo_relative_path, source)
            .into_iter()
            .map(|spec| (spec.local_name, spec.span))
            .collect::<BTreeMap<_, _>>();

        for mock in parse_static_mock_specs(repo_relative_path, source) {
            let Some(target_path) = resolve_local_module_path(
                repo_relative_path,
                &mock.module_specifier,
                &workspace.indexed_paths,
            ) else {
                continue;
            };
            let Some(target) = resolve_named_import_target(
                &workspace.entities_by_file,
                &target_path,
                &mock.exported_name,
            ) else {
                continue;
            };
            let Some(test_case) = first_test_case(&test_cases) else {
                continue;
            };
            let mock_entity = mock_entity_for(
                &repo_relative_path,
                &mock.exported_name,
                &mock.span,
                file_hash,
            );
            plan.push_entity(
                mock_entity.clone(),
                GlobalEntityWriteMode::InsertRecordIfMissing,
            );
            plan.push_edge(resolved_test_edge(
                &test_case.id,
                RelationKind::Mocks,
                &target.id,
                &mock.span,
                file_hash,
                "static_test_mock_module_factory",
                "test",
            ));
            plan.push_edge(resolved_test_edge(
                &mock_entity.id,
                RelationKind::Stubs,
                &target.id,
                &mock.span,
                file_hash,
                "static_test_mock_module_factory",
                "mock",
            ));
        }

        for assertion in parse_assertion_specs(repo_relative_path, source, &import_targets) {
            let Some(test_case) = containing_test_case(&test_cases, &assertion.span)
                .or_else(|| first_test_case(&test_cases))
            else {
                continue;
            };
            plan.push_edge(resolved_test_edge(
                &test_case.id,
                RelationKind::Asserts,
                &assertion.target.id,
                &assertion.span,
                file_hash,
                "static_test_assertion_import_target",
                "test",
            ));
            plan.push_edge(resolved_test_edge(
                &test_case.id,
                RelationKind::Tests,
                &assertion.target.id,
                &assertion.span,
                file_hash,
                "static_test_assertion_import_target",
                "test",
            ));
        }

        let mut direct_test_edges = BTreeSet::new();
        for test_case in &test_cases {
            let Some(test_span) = test_case.source_span.as_ref() else {
                continue;
            };
            for (local_name, target) in &import_targets {
                for call_span in call_spans_for_local_name(source, repo_relative_path, local_name) {
                    let Some(import_span) = import_spans_by_local.get(local_name) else {
                        continue;
                    };
                    if !span_contains(test_span, &call_span)
                        || local_declaration_shadows_import(
                            &workspace.entities_by_file,
                            repo_relative_path,
                            local_name,
                            import_span,
                            &call_span,
                        )
                    {
                        continue;
                    }
                    if direct_test_edges.insert((
                        test_case.id.clone(),
                        target.id.clone(),
                        call_span.to_string(),
                    )) {
                        plan.push_edge(resolved_test_edge(
                            &test_case.id,
                            RelationKind::Tests,
                            &target.id,
                            &call_span,
                            file_hash,
                            "static_test_direct_import_call",
                            "test",
                        ));
                    }
                }
            }
        }
    }
    plan.sort();
    Ok(plan)
}

fn reduce_derived_mutation_edges_from_store(
    store: &SqliteGraphStore,
) -> Result<GlobalFactReductionPlan, IndexError> {
    let calls =
        store.list_stored_edges_by_relation(RelationKind::Calls, UNBOUNDED_STORE_READ_LIMIT)?;
    let mut writes =
        store.list_stored_edges_by_relation(RelationKind::Writes, UNBOUNDED_STORE_READ_LIMIT)?;
    writes.extend(
        store.list_stored_edges_by_relation(RelationKind::Mutates, UNBOUNDED_STORE_READ_LIMIT)?,
    );
    let mut writes_by_head = BTreeMap::<&str, Vec<&Edge>>::new();
    for edge in writes.iter().filter(|edge| !edge.derived) {
        writes_by_head
            .entry(edge.head_id.as_str())
            .or_default()
            .push(edge);
    }

    let mut plan = GlobalFactReductionPlan::default();
    let mut derived_edge_ids = BTreeSet::<String>::new();
    let mut mutation_edges = 0_usize;
    'mutation_calls: for call in calls.iter().filter(|edge| !edge.derived) {
        let Some(writes_for_callee) = writes_by_head.get(call.tail_id.as_str()) else {
            continue;
        };
        for write in writes_for_callee
            .iter()
            .take(DERIVED_MUTATION_CLOSURE_MAX_WRITES_PER_CALLEE)
        {
            if mutation_edges >= DERIVED_MUTATION_CLOSURE_MAX_OUTPUT_EDGES {
                break 'mutation_calls;
            }
            if push_unique_derived_edge(
                &mut plan,
                &mut derived_edge_ids,
                derived_mutation_edge(call, write),
            ) {
                mutation_edges += 1;
            }
        }
    }
    let flows =
        store.list_stored_edges_by_relation(RelationKind::FlowsTo, UNBOUNDED_STORE_READ_LIMIT)?;
    let base_flows = flows
        .iter()
        .filter(|edge| !edge.derived)
        .collect::<Vec<_>>();
    let mut flows_by_head = BTreeMap::<&str, Vec<&Edge>>::new();
    for edge in &base_flows {
        flows_by_head
            .entry(edge.head_id.as_str())
            .or_default()
            .push(*edge);
    }
    let mut dataflow_edges = 0_usize;
    'dataflow_sources: for first in &base_flows {
        let Some(second_hops) = flows_by_head.get(first.tail_id.as_str()) else {
            continue;
        };
        for second in second_hops
            .iter()
            .take(DERIVED_DATAFLOW_CLOSURE_MAX_HOPS_PER_NODE)
        {
            if dataflow_edges >= DERIVED_DATAFLOW_CLOSURE_MAX_OUTPUT_EDGES {
                break 'dataflow_sources;
            }
            if !chainable_dataflow_edges(first, second)
                || first.head_id == second.tail_id
                || first.id == second.id
            {
                continue;
            }
            if push_unique_derived_edge(
                &mut plan,
                &mut derived_edge_ids,
                derived_dataflow_edge(&[*first, *second]),
            ) {
                dataflow_edges += 1;
            }

            let Some(third_hops) = flows_by_head.get(second.tail_id.as_str()) else {
                continue;
            };
            for third in third_hops
                .iter()
                .take(DERIVED_DATAFLOW_CLOSURE_MAX_HOPS_PER_NODE)
            {
                if dataflow_edges >= DERIVED_DATAFLOW_CLOSURE_MAX_OUTPUT_EDGES {
                    break 'dataflow_sources;
                }
                if !chainable_dataflow_edges(second, third)
                    || first.id == third.id
                    || second.id == third.id
                    || first.head_id == third.tail_id
                    || second.head_id == third.tail_id
                {
                    continue;
                }
                if push_unique_derived_edge(
                    &mut plan,
                    &mut derived_edge_ids,
                    derived_dataflow_edge(&[*first, *second, *third]),
                ) {
                    dataflow_edges += 1;
                }
            }
        }
    }
    plan.sort();
    Ok(plan)
}

fn push_unique_derived_edge(
    plan: &mut GlobalFactReductionPlan,
    derived_edge_ids: &mut BTreeSet<String>,
    edge: Edge,
) -> bool {
    if !derived_edge_ids.insert(edge.id.clone()) {
        return false;
    }
    plan.edges.push(edge);
    true
}

fn derived_mutation_edge(call: &Edge, write: &Edge) -> Edge {
    let provenance_edges = vec![call.id.clone(), write.id.clone()];
    let exactness = derived_exactness_for_edges([call, write]);
    let context = derived_context_for_edges([call, write]);
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "derived_from_base_path".into());
    metadata.insert(
        "resolver".to_string(),
        "calls_then_writes_mutation_closure".into(),
    );
    metadata.insert("phase".to_string(), "32".into());
    metadata.insert("context".to_string(), context.as_str().into());
    metadata.insert(
        "source_relation".to_string(),
        call.relation.to_string().into(),
    );
    metadata.insert(
        "sink_relation".to_string(),
        write.relation.to_string().into(),
    );
    metadata.insert("provenance_kind".to_string(), "CALLS->WRITES".into());

    Edge {
        id: stable_edge_id(
            &call.head_id,
            RelationKind::MayMutate,
            &write.tail_id,
            &call.source_span,
        ),
        head_id: call.head_id.clone(),
        relation: RelationKind::MayMutate,
        tail_id: write.tail_id.clone(),
        source_span: call.source_span.clone(),
        repo_commit: call
            .repo_commit
            .clone()
            .or_else(|| write.repo_commit.clone()),
        file_hash: call.file_hash.clone().or_else(|| write.file_hash.clone()),
        extractor: "codegraph-index-derived-closure".to_string(),
        confidence: call.confidence.min(write.confidence),
        exactness,
        edge_class: EdgeClass::Derived,
        context,
        derived: true,
        provenance_edges,
        metadata,
    }
}

fn chainable_dataflow_edges(first: &Edge, second: &Edge) -> bool {
    first.relation == RelationKind::FlowsTo
        && second.relation == RelationKind::FlowsTo
        && !first.derived
        && !second.derived
        && first.tail_id == second.head_id
        && first.source_span.repo_relative_path == second.source_span.repo_relative_path
}

fn derived_dataflow_edge(path: &[&Edge]) -> Edge {
    debug_assert!(path.len() >= 2);
    let first = path[0];
    let last = path[path.len() - 1];
    let provenance_edges = path.iter().map(|edge| edge.id.clone()).collect::<Vec<_>>();
    let exactness = derived_exactness_for_edges(path.iter().copied());
    let context = derived_context_for_edges(path.iter().copied());
    let confidence = path
        .iter()
        .fold(1.0_f64, |minimum, edge| minimum.min(edge.confidence));
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "derived_from_base_path".into());
    metadata.insert("resolver".to_string(), "flows_to_local_closure".into());
    metadata.insert("phase".to_string(), "language_tier3".into());
    metadata.insert("context".to_string(), context.as_str().into());
    metadata.insert("provenance_kind".to_string(), "FLOWS_TO+".into());
    metadata.insert("claim_state".to_string(), "derived_with_provenance".into());
    metadata.insert("hop_count".to_string(), path.len().into());

    Edge {
        id: stable_edge_id(
            &first.head_id,
            RelationKind::FlowsTo,
            &last.tail_id,
            &last.source_span,
        ),
        head_id: first.head_id.clone(),
        relation: RelationKind::FlowsTo,
        tail_id: last.tail_id.clone(),
        source_span: last.source_span.clone(),
        repo_commit: first
            .repo_commit
            .clone()
            .or_else(|| last.repo_commit.clone()),
        file_hash: first.file_hash.clone().or_else(|| last.file_hash.clone()),
        extractor: "codegraph-index-derived-dataflow".to_string(),
        confidence,
        exactness,
        edge_class: EdgeClass::Derived,
        context,
        derived: true,
        provenance_edges,
        metadata,
    }
}

fn derived_exactness_for_edges<'a>(edges: impl IntoIterator<Item = &'a Edge>) -> Exactness {
    if edges.into_iter().all(|edge| {
        matches!(
            edge.exactness,
            Exactness::Exact
                | Exactness::CompilerVerified
                | Exactness::LspVerified
                | Exactness::ParserVerified
                | Exactness::DerivedFromVerifiedEdges
        )
    }) {
        Exactness::DerivedFromVerifiedEdges
    } else {
        Exactness::Inferred
    }
}

fn derived_context_for_edges<'a>(edges: impl IntoIterator<Item = &'a Edge>) -> EdgeContext {
    let mut has_production = false;
    let mut has_test = false;
    let mut has_mock = false;
    let mut has_unknown = false;
    let mut has_mixed = false;
    for edge in edges {
        match edge.context {
            EdgeContext::Production => has_production = true,
            EdgeContext::Test => has_test = true,
            EdgeContext::Mock => has_mock = true,
            EdgeContext::Mixed => has_mixed = true,
            EdgeContext::Unknown => has_unknown = true,
        }
    }
    let distinct = [has_production, has_test, has_mock, has_unknown]
        .into_iter()
        .filter(|present| *present)
        .count();
    if has_mixed || distinct > 1 {
        EdgeContext::Mixed
    } else if has_mock {
        EdgeContext::Mock
    } else if has_test {
        EdgeContext::Test
    } else if has_production {
        EdgeContext::Production
    } else {
        EdgeContext::Unknown
    }
}

fn parse_static_imports(repo_relative_path: &str, source: &str) -> Vec<StaticImportSpec> {
    let mut imports = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let line = strip_leading_utf8_bom(line);
        let trimmed = line.trim();
        if !trimmed.starts_with("import ") || !trimmed.contains(" from ") {
            continue;
        }
        let Some(module_specifier) = import_module_specifier(line) else {
            continue;
        };
        let span = SourceSpan::with_columns(
            repo_relative_path,
            line_index as u32 + 1,
            1,
            line_index as u32 + 1,
            line.chars().count() as u32 + 1,
        );

        if let Some(default_name) = default_import_name(line) {
            if looks_like_identifier(&default_name) {
                imports.push(StaticImportSpec {
                    importer_path: repo_relative_path.to_string(),
                    imported_name: "default".to_string(),
                    local_name: default_name,
                    module_specifier: module_specifier.clone(),
                    kind: StaticImportKind::Default,
                    span: span.clone(),
                });
            }
        }

        if let Some(open) = line.find('{') {
            if let Some(close_offset) = line[open + 1..].find('}') {
                let close = open + 1 + close_offset;
                for item in line[open + 1..close].split(',') {
                    let item = item.trim();
                    if item.is_empty() {
                        continue;
                    }
                    let (imported_name, local_name) =
                        if let Some((imported, local)) = split_import_alias(item) {
                            (imported, local)
                        } else {
                            (item, item)
                        };
                    if !looks_like_identifier(imported_name) || !looks_like_identifier(local_name) {
                        continue;
                    }
                    imports.push(StaticImportSpec {
                        importer_path: repo_relative_path.to_string(),
                        imported_name: imported_name.to_string(),
                        local_name: local_name.to_string(),
                        module_specifier: module_specifier.clone(),
                        kind: StaticImportKind::Named,
                        span: span.clone(),
                    });
                }
            }
        }
    }
    imports
}

fn parse_static_reexports(repo_relative_path: &str, source: &str) -> Vec<StaticReexportSpec> {
    let mut exports = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let line = strip_leading_utf8_bom(line);
        let trimmed = line.trim();
        if !trimmed.starts_with("export ") || !trimmed.contains(" from ") {
            continue;
        }
        let Some(module_specifier) = import_module_specifier(line) else {
            continue;
        };
        let Some(open) = line.find('{') else {
            continue;
        };
        let Some(close_offset) = line[open + 1..].find('}') else {
            continue;
        };
        let close = open + 1 + close_offset;
        let span = SourceSpan::with_columns(
            repo_relative_path,
            line_index as u32 + 1,
            1,
            line_index as u32 + 1,
            line.chars().count() as u32 + 1,
        );
        for item in line[open + 1..close].split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let (imported_name, exported_name) =
                if let Some((imported, exported)) = split_import_alias(item) {
                    (imported, exported)
                } else {
                    (item, item)
                };
            if !looks_like_identifier(imported_name) || !looks_like_identifier(exported_name) {
                continue;
            }
            exports.push(StaticReexportSpec {
                exporter_path: repo_relative_path.to_string(),
                imported_name: imported_name.to_string(),
                exported_name: exported_name.to_string(),
                module_specifier: module_specifier.clone(),
                kind: if imported_name == "default" {
                    StaticImportKind::Default
                } else {
                    StaticImportKind::Named
                },
                span: span.clone(),
            });
        }
    }
    exports
}

fn parse_dynamic_imports(repo_relative_path: &str, source: &str) -> Vec<StaticDynamicImportSpec> {
    let mut imports = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let line = strip_leading_utf8_bom(line);
        let mut search_start = 0usize;
        while let Some(offset) = line[search_start..].find("import(") {
            let start = search_start + offset;
            if !identifier_boundary_before(line, start) {
                search_start = start + "import(".len();
                continue;
            }
            let open_paren = start + "import".len();
            let Some(close_paren) = matching_close_paren(line, open_paren) else {
                search_start = start + "import(".len();
                continue;
            };
            let raw_specifier = &line[open_paren + 1..close_paren];
            let specifier = canonical_dynamic_import_specifier(raw_specifier);
            if specifier.is_empty() {
                search_start = close_paren + 1;
                continue;
            }
            imports.push(StaticDynamicImportSpec {
                specifier,
                span: SourceSpan::with_columns(
                    repo_relative_path,
                    line_index as u32 + 1,
                    line[..start].chars().count() as u32 + 1,
                    line_index as u32 + 1,
                    line[..close_paren + 1].chars().count() as u32 + 1,
                ),
            });
            search_start = close_paren + 1;
        }
    }
    imports
}

fn strip_leading_utf8_bom(line: &str) -> &str {
    line.strip_prefix('\u{feff}').unwrap_or(line)
}

fn matching_close_paren(line: &str, open_paren: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in line
        .char_indices()
        .skip_while(|(index, _)| *index < open_paren)
    {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }
        if matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
            continue;
        }
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn split_top_level_args(args: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in args.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }
        if matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
            continue;
        }
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                values.push(args[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    if start <= args.len() {
        let value = args[start..].trim();
        if !value.is_empty() {
            values.push(value.to_string());
        }
    }
    values
}

fn canonical_dynamic_import_specifier(raw: &str) -> String {
    raw.split('+')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.trim_matches(|ch| matches!(ch, '"' | '\'' | '`'))
                .trim()
                .to_string()
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("+")
}

#[allow(dead_code)]
fn resolved_import_targets_for_source(
    repo_root: &Path,
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    indexed_paths: &BTreeSet<String>,
    repo_relative_path: &str,
    source: &str,
) -> Result<BTreeMap<String, Entity>, IndexError> {
    resolved_import_targets_for_source_cached(
        repo_root,
        entities_by_file,
        indexed_paths,
        None,
        repo_relative_path,
        source,
    )
}

fn resolved_import_targets_for_source_cached(
    repo_root: &Path,
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    indexed_paths: &BTreeSet<String>,
    source_cache: Option<&BTreeMap<String, String>>,
    repo_relative_path: &str,
    source: &str,
) -> Result<BTreeMap<String, Entity>, IndexError> {
    let mut targets = BTreeMap::new();
    for spec in parse_static_imports(repo_relative_path, source) {
        let Some(target_path) =
            resolve_local_module_path(repo_relative_path, &spec.module_specifier, indexed_paths)
        else {
            continue;
        };
        if let Some(target) = resolve_import_target_cached(
            repo_root,
            entities_by_file,
            indexed_paths,
            source_cache,
            &target_path,
            &spec.imported_name,
            spec.kind,
        )? {
            targets.insert(spec.local_name, target);
        }
    }
    Ok(targets)
}

fn parse_static_mock_specs(repo_relative_path: &str, source: &str) -> Vec<StaticMockSpec> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut specs = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if !(line.contains("vi.mock(") || line.contains("jest.mock(")) {
            index += 1;
            continue;
        }
        let Some(module_specifier) = first_string_in_text(line) else {
            index += 1;
            continue;
        };
        let start_line = index + 1;
        let mut end_index = index;
        while end_index + 1 < lines.len() && !lines[end_index].contains("));") {
            end_index += 1;
        }
        let end_line = end_index + 1;
        let span = SourceSpan::with_columns(
            repo_relative_path,
            start_line as u32,
            1,
            end_line as u32,
            lines[end_index].chars().count() as u32 + 1,
        );
        for property_line in &lines[index..=end_index] {
            let trimmed = property_line.trim();
            let Some((candidate, _)) = trimmed.split_once(':') else {
                continue;
            };
            let exported_name = candidate.trim();
            if looks_like_identifier(exported_name) {
                specs.push(StaticMockSpec {
                    module_specifier: module_specifier.clone(),
                    exported_name: exported_name.to_string(),
                    span: span.clone(),
                });
            }
        }
        index = end_index + 1;
    }
    specs
}

fn parse_assertion_specs(
    repo_relative_path: &str,
    source: &str,
    import_targets: &BTreeMap<String, Entity>,
) -> Vec<AssertionSpec> {
    let mut assertions = Vec::new();
    for statement in assertion_statement_spans(repo_relative_path, source) {
        for (local_name, target) in import_targets {
            if line_contains_call(&statement.text, local_name) {
                assertions.push(AssertionSpec {
                    target: target.clone(),
                    span: statement.span.clone(),
                });
            }
        }
    }
    assertions
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssertionStatementSpan {
    text: String,
    span: SourceSpan,
}

fn assertion_statement_spans(
    repo_relative_path: &str,
    source: &str,
) -> Vec<AssertionStatementSpan> {
    let mut statements = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        if !line.contains("expect(") && !line.contains("assert") {
            continue;
        }
        let mut statement_start = 0usize;
        let mut quote = None;
        let mut escaped = false;
        for (index, ch) in line
            .char_indices()
            .chain(std::iter::once((line.len(), ';')))
        {
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == active_quote {
                    quote = None;
                }
                continue;
            }
            if matches!(ch, '"' | '\'' | '`') {
                quote = Some(ch);
                continue;
            }
            if ch != ';' {
                continue;
            }
            let raw = &line[statement_start..index];
            if let Some((start, end)) = trimmed_byte_range(line, statement_start, raw) {
                let text = line[start..end].to_string();
                if looks_like_assertion_statement(&text) {
                    statements.push(AssertionStatementSpan {
                        span: source_span_for_byte_range(
                            repo_relative_path,
                            line,
                            line_index,
                            start,
                            end,
                        ),
                        text,
                    });
                }
            }
            statement_start = index.saturating_add(ch.len_utf8());
        }
    }
    statements
}

fn trimmed_byte_range(
    line: &str,
    statement_start: usize,
    raw_statement: &str,
) -> Option<(usize, usize)> {
    let start_offset = raw_statement
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(index, _)| index)?;
    let end_offset = raw_statement
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(index, ch)| index + ch.len_utf8())?;
    let start = statement_start + start_offset;
    let end = statement_start + end_offset;
    (start < end && end <= line.len()).then_some((start, end))
}

fn looks_like_assertion_statement(statement: &str) -> bool {
    let lower = statement.to_ascii_lowercase();
    lower.contains("expect(")
        || lower.contains("assert(")
        || lower.contains("assert.")
        || lower.contains(".should")
}

fn first_string_in_text(text: &str) -> Option<String> {
    let quote_index = text.find('"').or_else(|| text.find('\''))?;
    let quote = text[quote_index..].chars().next()?;
    let rest = &text[quote_index + quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn line_contains_call(line: &str, local_name: &str) -> bool {
    let mut search_start = 0usize;
    while let Some(offset) = line[search_start..].find(local_name) {
        let start = search_start + offset;
        let after_name = start + local_name.len();
        if identifier_boundary_before(line, start) && identifier_boundary_after(line, after_name) {
            let rest = &line[after_name..];
            let whitespace = rest
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
            if rest[whitespace..].starts_with('(') {
                return true;
            }
        }
        search_start = after_name;
    }
    false
}

fn default_import_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let after_import = trimmed.strip_prefix("import ")?.trim_start();
    if after_import.starts_with('{') || after_import.starts_with('*') {
        return None;
    }
    let end = after_import
        .find(',')
        .or_else(|| after_import.find(" from "))?;
    let candidate = after_import[..end].trim();
    if candidate.is_empty() || candidate.chars().any(char::is_whitespace) {
        return None;
    }
    Some(candidate.to_string())
}

fn import_module_specifier(line: &str) -> Option<String> {
    let from_index = line.find(" from ")?;
    let after_from = line[from_index + " from ".len()..].trim_start();
    let quote = after_from.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let end = after_from[1..].find(quote)?;
    Some(after_from[1..1 + end].to_string())
}

fn split_import_alias(item: &str) -> Option<(&str, &str)> {
    for separator in [" as ", " AS "] {
        if let Some((imported, local)) = item.split_once(separator) {
            return Some((imported.trim(), local.trim()));
        }
    }
    None
}

fn resolve_local_module_path(
    importer_path: &str,
    module_specifier: &str,
    indexed_paths: &BTreeSet<String>,
) -> Option<String> {
    if !module_specifier.starts_with('.') {
        return None;
    }
    local_module_path_candidates(importer_path, module_specifier)
        .into_iter()
        .find(|path| indexed_paths.contains(path))
}

fn local_module_path_candidates(importer_path: &str, module_specifier: &str) -> Vec<String> {
    let importer_parent = Path::new(importer_path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let raw = normalize_resolved_module_path(&importer_parent.join(module_specifier));
    if Path::new(&raw).extension().is_some() {
        vec![raw]
    } else {
        ["ts", "tsx", "js", "jsx"]
            .into_iter()
            .map(|extension| format!("{raw}.{extension}"))
            .chain(
                ["ts", "tsx", "js", "jsx"]
                    .into_iter()
                    .map(|extension| format!("{raw}/index.{extension}")),
            )
            .collect::<Vec<_>>()
    }
    .into_iter()
    .map(|candidate| normalize_graph_path(&candidate))
    .collect()
}

fn normalize_resolved_module_path(path: &Path) -> String {
    let normalized = normalize_graph_path(path.to_string_lossy());
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if parts.last().is_some_and(|previous| *previous != "..") {
                parts.pop();
            } else {
                parts.push(part);
            }
            continue;
        }
        parts.push(part);
    }
    parts.join("/")
}

#[allow(dead_code)]
fn resolve_import_target(
    repo_root: &Path,
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    indexed_paths: &BTreeSet<String>,
    target_path: &str,
    imported_name: &str,
    kind: StaticImportKind,
) -> Result<Option<Entity>, IndexError> {
    resolve_import_target_cached(
        repo_root,
        entities_by_file,
        indexed_paths,
        None,
        target_path,
        imported_name,
        kind,
    )
}

fn resolve_import_target_cached(
    repo_root: &Path,
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    indexed_paths: &BTreeSet<String>,
    source_cache: Option<&BTreeMap<String, String>>,
    target_path: &str,
    imported_name: &str,
    kind: StaticImportKind,
) -> Result<Option<Entity>, IndexError> {
    resolve_import_target_with_depth_cached(
        repo_root,
        entities_by_file,
        indexed_paths,
        source_cache,
        target_path,
        imported_name,
        kind,
        0,
    )
}

fn resolve_import_target_with_depth_cached(
    repo_root: &Path,
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    indexed_paths: &BTreeSet<String>,
    source_cache: Option<&BTreeMap<String, String>>,
    target_path: &str,
    imported_name: &str,
    kind: StaticImportKind,
    depth: usize,
) -> Result<Option<Entity>, IndexError> {
    if depth > 8 {
        return Ok(None);
    }
    if kind == StaticImportKind::Default {
        if let Some(target) = resolve_default_import_target(entities_by_file, target_path) {
            return Ok(Some(target));
        }
    } else if let Some(target) =
        resolve_named_import_target(entities_by_file, target_path, imported_name)
    {
        return Ok(Some(target));
    }

    let source_path = repo_root.join(target_path);
    if !source_path.exists() {
        return Ok(None);
    }
    let source_holder;
    let source = match source_cache.and_then(|cache| cache.get(target_path)) {
        Some(source) => source.as_str(),
        None => {
            source_holder = fs::read_to_string(&source_path)?;
            source_holder.as_str()
        }
    };
    for spec in parse_static_reexports(target_path, source) {
        if spec.exported_name != imported_name {
            continue;
        }
        let Some(reexport_target_path) =
            resolve_local_module_path(target_path, &spec.module_specifier, indexed_paths)
        else {
            continue;
        };
        if let Some(target) = resolve_import_target_with_depth_cached(
            repo_root,
            entities_by_file,
            indexed_paths,
            source_cache,
            &reexport_target_path,
            &spec.imported_name,
            spec.kind,
            depth + 1,
        )? {
            return Ok(Some(target));
        }
    }

    Ok(None)
}

fn resolve_named_import_target(
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    target_path: &str,
    imported_name: &str,
) -> Option<Entity> {
    entities_by_file.get(target_path).and_then(|entities| {
        entities
            .iter()
            .find(|entity| {
                matches!(
                    entity.kind,
                    EntityKind::Function
                        | EntityKind::Method
                        | EntityKind::Class
                        | EntityKind::LocalVariable
                        | EntityKind::GlobalVariable
                        | EntityKind::Table
                ) && entity.name == imported_name
            })
            .cloned()
    })
}

fn resolve_default_import_target(
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    target_path: &str,
) -> Option<Entity> {
    entities_by_file.get(target_path).and_then(|entities| {
        if let Some(entity) = entities.iter().find(|entity| {
            matches!(
                entity.kind,
                EntityKind::Function
                    | EntityKind::Method
                    | EntityKind::Class
                    | EntityKind::LocalVariable
                    | EntityKind::GlobalVariable
            ) && entity.name == "default"
        }) {
            return Some(entity.clone());
        }
        let default_exports = entities
            .iter()
            .filter(|entity| {
                entity.kind == EntityKind::Export
                    && entity.name.to_ascii_lowercase().contains("export default")
            })
            .collect::<Vec<_>>();
        for export in default_exports {
            if let Some(entity) = entities.iter().find(|entity| {
                matches!(
                    entity.kind,
                    EntityKind::Function
                        | EntityKind::Method
                        | EntityKind::Class
                        | EntityKind::LocalVariable
                        | EntityKind::GlobalVariable
                ) && export.name.contains(&entity.name)
            }) {
                return Some(entity.clone());
            }
        }
        None
    })
}

fn file_entity_for_path(
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    repo_relative_path: &str,
) -> Option<Entity> {
    entities_by_file
        .get(repo_relative_path)
        .and_then(|entities| {
            entities
                .iter()
                .find(|entity| entity.kind == EntityKind::File)
                .cloned()
        })
}

fn import_alias_entity(spec: &StaticImportSpec, file_hash: &str) -> Entity {
    let qualified_name = format!(
        "{}.import:{}",
        module_name_for_index_path(&spec.importer_path),
        spec.local_name
    );
    let signature = format!(
        "{qualified_name}@{}:{}-{}:{}",
        spec.span.start_line,
        spec.span.start_column.unwrap_or(1),
        spec.span.end_line,
        spec.span.end_column.unwrap_or(1)
    );
    let mut metadata = Metadata::new();
    metadata.insert(
        "module_specifier".to_string(),
        spec.module_specifier.clone().into(),
    );
    metadata.insert(
        "imported_name".to_string(),
        spec.imported_name.clone().into(),
    );
    metadata.insert("local_name".to_string(), spec.local_name.clone().into());
    metadata.insert(
        "import_kind".to_string(),
        match spec.kind {
            StaticImportKind::Named => "named",
            StaticImportKind::Default => "default",
        }
        .into(),
    );
    metadata.insert("resolution".to_string(), "resolved_static_import".into());
    metadata.insert("claim_state".to_string(), "exact".into());
    metadata.insert("syntax_claim_state".to_string(), "exact".into());
    metadata.insert("target_resolution_claim_state".to_string(), "exact".into());
    metadata.insert(
        "proof_basis".to_string(),
        "local module specifier resolved to indexed file and declaration name".into(),
    );
    metadata.insert("phase".to_string(), "14".into());
    Entity {
        id: stable_entity_id_for_kind(
            &spec.importer_path,
            EntityKind::Import,
            &qualified_name,
            Some(&signature),
        ),
        kind: EntityKind::Import,
        name: spec.local_name.clone(),
        qualified_name,
        repo_relative_path: spec.importer_path.clone(),
        source_span: Some(spec.span.clone()),
        content_hash: None,
        file_hash: Some(file_hash.to_string()),
        created_from: "codegraph-index-static-import-resolver".to_string(),
        confidence: 1.0,
        metadata,
    }
}

fn dynamic_import_entity(
    repo_relative_path: &str,
    spec: &StaticDynamicImportSpec,
    file_hash: &str,
) -> Entity {
    let qualified_name = format!("dynamic_import:{}", spec.specifier);
    let signature = format!(
        "{qualified_name}@{}:{}-{}:{}",
        spec.span.start_line,
        spec.span.start_column.unwrap_or(1),
        spec.span.end_line,
        spec.span.end_column.unwrap_or(1)
    );
    let mut metadata = Metadata::new();
    metadata.insert("specifier".to_string(), spec.specifier.clone().into());
    metadata.insert("import_kind".to_string(), "dynamic".into());
    metadata.insert("resolution".to_string(), "unresolved_dynamic_import".into());
    metadata.insert("claim_state".to_string(), "heuristic".into());
    metadata.insert("syntax_claim_state".to_string(), "heuristic".into());
    metadata.insert(
        "target_resolution_claim_state".to_string(),
        "unresolved".into(),
    );
    metadata.insert("context".to_string(), "unknown".into());
    metadata.insert("phase".to_string(), "14".into());
    Entity {
        id: stable_entity_id_for_kind(
            repo_relative_path,
            EntityKind::Import,
            &qualified_name,
            Some(&signature),
        ),
        kind: EntityKind::Import,
        name: qualified_name.clone(),
        qualified_name,
        repo_relative_path: normalize_graph_path(repo_relative_path),
        source_span: Some(spec.span.clone()),
        content_hash: None,
        file_hash: Some(file_hash.to_string()),
        created_from: "codegraph-index-dynamic-import-resolver".to_string(),
        confidence: 0.55,
        metadata,
    }
}

fn resolved_import_edge(
    head_id: &str,
    relation: RelationKind,
    tail_id: &str,
    span: &SourceSpan,
    file_hash: &str,
    reason: &str,
) -> Edge {
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved_static_import".into());
    metadata.insert("resolver".to_string(), reason.into());
    metadata.insert("claim_state".to_string(), "exact".into());
    metadata.insert("syntax_claim_state".to_string(), "exact".into());
    metadata.insert("target_resolution_claim_state".to_string(), "exact".into());
    metadata.insert(
        "proof_basis".to_string(),
        "local module specifier resolved to indexed file and declaration name".into(),
    );
    metadata.insert("phase".to_string(), "14".into());
    Edge {
        id: stable_edge_id(head_id, relation, tail_id, span),
        head_id: head_id.to_string(),
        relation,
        tail_id: tail_id.to_string(),
        source_span: span.clone(),
        repo_commit: None,
        file_hash: Some(file_hash.to_string()),
        extractor: "codegraph-index-static-import-resolver".to_string(),
        confidence: 1.0,
        exactness: Exactness::ParserVerified,
        edge_class: EdgeClass::BaseExact,
        context: EdgeContext::Production,
        derived: false,
        provenance_edges: Vec::new(),
        metadata,
    }
}

fn unresolved_dynamic_import_edge(
    head_id: &str,
    tail_id: &str,
    span: &SourceSpan,
    file_hash: &str,
) -> Edge {
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "unresolved_dynamic_import".into());
    metadata.insert("resolver".to_string(), "dynamic_import_unresolved".into());
    metadata.insert("claim_state".to_string(), "heuristic".into());
    metadata.insert("syntax_claim_state".to_string(), "heuristic".into());
    metadata.insert(
        "target_resolution_claim_state".to_string(),
        "unresolved".into(),
    );
    metadata.insert("context".to_string(), "unknown".into());
    metadata.insert("phase".to_string(), "14".into());
    Edge {
        id: stable_edge_id(head_id, RelationKind::Imports, tail_id, span),
        head_id: head_id.to_string(),
        relation: RelationKind::Imports,
        tail_id: tail_id.to_string(),
        source_span: span.clone(),
        repo_commit: None,
        file_hash: Some(file_hash.to_string()),
        extractor: "codegraph-index-dynamic-import-resolver".to_string(),
        confidence: 0.55,
        exactness: Exactness::StaticHeuristic,
        edge_class: EdgeClass::BaseHeuristic,
        context: EdgeContext::Unknown,
        derived: false,
        provenance_edges: Vec::new(),
        metadata,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimpleCallRecord {
    span: SourceSpan,
    line_index: usize,
    line: String,
    args_start_byte: usize,
    args: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticMockSpec {
    module_specifier: String,
    exported_name: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
struct AssertionSpec {
    target: Entity,
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DirectRoleCheckCall {
    role: String,
    role_span: SourceSpan,
    call_span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteExposureSpec {
    route_name: String,
    method: String,
    path: String,
    guard_name: Option<String>,
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalAssignment {
    local_name: String,
    local_span: SourceSpan,
    source_expr: String,
    source_span: SourceSpan,
    source_local: Option<String>,
    source_property: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct LocalSecurityFlow {
    head: Entity,
    tail: Entity,
    span: SourceSpan,
}

fn resolved_security_edge(
    head_id: &str,
    relation: RelationKind,
    tail_id: &str,
    span: &SourceSpan,
    file_hash: &str,
    reason: &str,
) -> Edge {
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved".into());
    metadata.insert("resolver".to_string(), reason.into());
    metadata.insert("phase".to_string(), "16".into());
    metadata.insert("context".to_string(), "production".into());
    Edge {
        id: stable_edge_id(head_id, relation, tail_id, span),
        head_id: head_id.to_string(),
        relation,
        tail_id: tail_id.to_string(),
        source_span: span.clone(),
        repo_commit: None,
        file_hash: non_empty_file_hash(file_hash),
        extractor: "codegraph-index-security-resolver".to_string(),
        confidence: 1.0,
        exactness: Exactness::ParserVerified,
        edge_class: EdgeClass::BaseExact,
        context: EdgeContext::Production,
        derived: false,
        provenance_edges: Vec::new(),
        metadata,
    }
}

fn resolved_test_edge(
    head_id: &str,
    relation: RelationKind,
    tail_id: &str,
    span: &SourceSpan,
    file_hash: &str,
    reason: &str,
    context: &str,
) -> Edge {
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved".into());
    metadata.insert("resolver".to_string(), reason.into());
    metadata.insert("phase".to_string(), "31".into());
    metadata.insert("context".to_string(), context.into());
    let evidence_role = if context.eq_ignore_ascii_case("mock")
        || context.eq_ignore_ascii_case("stub")
        || matches!(relation, RelationKind::Mocks | RelationKind::Stubs)
    {
        "mock"
    } else {
        "test"
    };
    metadata.insert("source_role".to_string(), evidence_role.into());
    metadata.insert("evidence_role".to_string(), evidence_role.into());
    metadata.insert(
        "classification_source".to_string(),
        "test_relation_resolver".into(),
    );
    metadata.insert(
        "classification_reason".to_string(),
        if evidence_role == "mock" {
            "resolved mock/stub test relation".into()
        } else {
            "resolved test/assertion relation".into()
        },
    );
    Edge {
        id: stable_edge_id(head_id, relation, tail_id, span),
        head_id: head_id.to_string(),
        relation,
        tail_id: tail_id.to_string(),
        source_span: span.clone(),
        repo_commit: None,
        file_hash: non_empty_file_hash(file_hash),
        extractor: "codegraph-index-test-resolver".to_string(),
        confidence: 1.0,
        exactness: Exactness::ParserVerified,
        edge_class: if evidence_role == "mock" {
            EdgeClass::Mock
        } else {
            EdgeClass::Test
        },
        context: if evidence_role == "mock" {
            EdgeContext::Mock
        } else {
            EdgeContext::Test
        },
        derived: false,
        provenance_edges: Vec::new(),
        metadata,
    }
}

fn mock_entity_for(
    repo_relative_path: &str,
    exported_name: &str,
    span: &SourceSpan,
    file_hash: &str,
) -> Entity {
    let name = format!("{exported_name}Mock");
    let qualified_name = format!(
        "{}.{}",
        module_name_for_index_path(repo_relative_path),
        name
    );
    let signature = format!(
        "{qualified_name}@{}:{}-{}:{}",
        span.start_line,
        span.start_column.unwrap_or(1),
        span.end_line,
        span.end_column.unwrap_or(1)
    );
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved".into());
    metadata.insert(
        "resolver".to_string(),
        "static_test_mock_module_factory".into(),
    );
    metadata.insert("phase".to_string(), "31".into());
    metadata.insert("context".to_string(), "mock".into());
    metadata.insert("source_role".to_string(), "mock".into());
    metadata.insert("evidence_role".to_string(), "mock".into());
    metadata.insert(
        "source_role_reason".to_string(),
        "static test mock module factory".into(),
    );
    metadata.insert("source_role_source".to_string(), "test_resolver".into());
    metadata.insert("mocked_export".to_string(), exported_name.into());
    Entity {
        id: stable_entity_id_for_kind(
            repo_relative_path,
            EntityKind::Mock,
            &qualified_name,
            Some(&signature),
        ),
        kind: EntityKind::Mock,
        name,
        qualified_name,
        repo_relative_path: normalize_graph_path(repo_relative_path),
        source_span: Some(span.clone()),
        content_hash: None,
        file_hash: non_empty_file_hash(file_hash),
        created_from: "codegraph-index-test-resolver".to_string(),
        confidence: 1.0,
        metadata,
    }
}

fn role_entity_for(
    repo_relative_path: &str,
    role: &str,
    span: &SourceSpan,
    file_hash: &str,
    reason: &str,
) -> Entity {
    let qualified_name = format!("role:{role}");
    let signature = format!(
        "{qualified_name}@{}:{}-{}:{}",
        span.start_line,
        span.start_column.unwrap_or(1),
        span.end_line,
        span.end_column.unwrap_or(1)
    );
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved".into());
    metadata.insert("resolver".to_string(), reason.into());
    metadata.insert("phase".to_string(), "16".into());
    metadata.insert("context".to_string(), "production".into());
    Entity {
        id: stable_entity_id_for_kind(
            repo_relative_path,
            EntityKind::Role,
            &qualified_name,
            Some(&signature),
        ),
        kind: EntityKind::Role,
        name: role.to_string(),
        qualified_name,
        repo_relative_path: normalize_graph_path(repo_relative_path),
        source_span: Some(span.clone()),
        content_hash: None,
        file_hash: non_empty_file_hash(file_hash),
        created_from: "codegraph-index-security-resolver".to_string(),
        confidence: 1.0,
        metadata,
    }
}

fn middleware_entity_for(
    function: &Entity,
    span: &SourceSpan,
    file_hash: &str,
    reason: &str,
) -> Entity {
    let qualified_name = function.qualified_name.clone();
    let signature = format!(
        "{qualified_name}@{}:{}-{}:{}",
        span.start_line,
        span.start_column.unwrap_or(1),
        span.end_line,
        span.end_column.unwrap_or(1)
    );
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved".into());
    metadata.insert("resolver".to_string(), reason.into());
    metadata.insert("phase".to_string(), "31".into());
    metadata.insert("context".to_string(), "production".into());
    metadata.insert("source_function_id".to_string(), function.id.clone().into());
    Entity {
        id: stable_entity_id_for_kind(
            &function.repo_relative_path,
            EntityKind::Middleware,
            &qualified_name,
            Some(&signature),
        ),
        kind: EntityKind::Middleware,
        name: function.name.clone(),
        qualified_name,
        repo_relative_path: normalize_graph_path(&function.repo_relative_path),
        source_span: function.source_span.clone().or_else(|| Some(span.clone())),
        content_hash: None,
        file_hash: non_empty_file_hash(file_hash),
        created_from: "codegraph-index-security-resolver".to_string(),
        confidence: 1.0,
        metadata,
    }
}

fn route_entity_for(
    repo_relative_path: &str,
    route: &RouteExposureSpec,
    file_hash: &str,
) -> Entity {
    let qualified_name = format!(
        "{}.{}",
        module_name_for_index_path(repo_relative_path),
        route.route_name
    );
    let signature = format!(
        "{qualified_name}@{}:{}-{}:{}",
        route.span.start_line,
        route.span.start_column.unwrap_or(1),
        route.span.end_line,
        route.span.end_column.unwrap_or(1)
    );
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved".into());
    metadata.insert("resolver".to_string(), "route_factory_literal".into());
    metadata.insert("phase".to_string(), "31".into());
    metadata.insert("context".to_string(), "production".into());
    metadata.insert("method".to_string(), route.method.clone().into());
    metadata.insert("path".to_string(), route.path.clone().into());
    Entity {
        id: stable_entity_id_for_kind(
            repo_relative_path,
            EntityKind::Route,
            &qualified_name,
            Some(&signature),
        ),
        kind: EntityKind::Route,
        name: route.route_name.clone(),
        qualified_name,
        repo_relative_path: normalize_graph_path(repo_relative_path),
        source_span: Some(route.span.clone()),
        content_hash: None,
        file_hash: non_empty_file_hash(file_hash),
        created_from: "codegraph-index-security-resolver".to_string(),
        confidence: 1.0,
        metadata,
    }
}

fn endpoint_entity_for(
    repo_relative_path: &str,
    route: &RouteExposureSpec,
    file_hash: &str,
) -> Entity {
    let name = format!("{} {}", route.method, route.path);
    let qualified_name = format!("endpoint:{name}");
    let signature = format!(
        "{qualified_name}@{}:{}-{}:{}",
        route.span.start_line,
        route.span.start_column.unwrap_or(1),
        route.span.end_line,
        route.span.end_column.unwrap_or(1)
    );
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved".into());
    metadata.insert("resolver".to_string(), "route_factory_literal".into());
    metadata.insert("phase".to_string(), "31".into());
    metadata.insert("context".to_string(), "production".into());
    metadata.insert("method".to_string(), route.method.clone().into());
    metadata.insert("path".to_string(), route.path.clone().into());
    Entity {
        id: stable_entity_id_for_kind(
            repo_relative_path,
            EntityKind::Endpoint,
            &qualified_name,
            Some(&signature),
        ),
        kind: EntityKind::Endpoint,
        name,
        qualified_name,
        repo_relative_path: normalize_graph_path(repo_relative_path),
        source_span: Some(route.span.clone()),
        content_hash: None,
        file_hash: non_empty_file_hash(file_hash),
        created_from: "codegraph-index-security-resolver".to_string(),
        confidence: 1.0,
        metadata,
    }
}

fn sanitizer_entity_for(function: &Entity, span: &SourceSpan, file_hash: &str) -> Entity {
    let qualified_name = format!(
        "{}.{}",
        module_name_for_index_path(&function.repo_relative_path),
        function.name
    );
    let signature = format!(
        "{qualified_name}@{}:{}-{}:{}",
        span.start_line,
        span.start_column.unwrap_or(1),
        span.end_line,
        span.end_column.unwrap_or(1)
    );
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved".into());
    metadata.insert(
        "resolver".to_string(),
        "sanitizer_function_declaration".into(),
    );
    metadata.insert("phase".to_string(), "16".into());
    metadata.insert("context".to_string(), "production".into());
    metadata.insert("source_function_id".to_string(), function.id.clone().into());
    Entity {
        id: stable_entity_id_for_kind(
            &function.repo_relative_path,
            EntityKind::Sanitizer,
            &qualified_name,
            Some(&signature),
        ),
        kind: EntityKind::Sanitizer,
        name: function.name.clone(),
        qualified_name,
        repo_relative_path: normalize_graph_path(&function.repo_relative_path),
        source_span: Some(span.clone()),
        content_hash: None,
        file_hash: non_empty_file_hash(file_hash),
        created_from: "codegraph-index-security-resolver".to_string(),
        confidence: 1.0,
        metadata,
    }
}

fn property_entity_for(
    repo_relative_path: &str,
    name: &str,
    span: &SourceSpan,
    file_hash: &str,
    reason: &str,
) -> Entity {
    let qualified_name = format!(
        "{}.property:{name}",
        module_name_for_index_path(repo_relative_path)
    );
    let signature = format!(
        "{qualified_name}@{}:{}-{}:{}",
        span.start_line,
        span.start_column.unwrap_or(1),
        span.end_line,
        span.end_column.unwrap_or(1)
    );
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved".into());
    metadata.insert("resolver".to_string(), reason.into());
    metadata.insert("phase".to_string(), "16".into());
    metadata.insert("context".to_string(), "production".into());
    Entity {
        id: stable_entity_id_for_kind(
            repo_relative_path,
            EntityKind::Property,
            &qualified_name,
            Some(&signature),
        ),
        kind: EntityKind::Property,
        name: name.to_string(),
        qualified_name,
        repo_relative_path: normalize_graph_path(repo_relative_path),
        source_span: Some(span.clone()),
        content_hash: None,
        file_hash: non_empty_file_hash(file_hash),
        created_from: "codegraph-index-security-resolver".to_string(),
        confidence: 1.0,
        metadata,
    }
}

fn local_variable_entity_for_assignment(
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    repo_relative_path: &str,
    assignment: &LocalAssignment,
    file_hash: &str,
) -> Entity {
    if let Some(entity) = entities_by_file
        .get(repo_relative_path)
        .and_then(|entities| {
            entities
                .iter()
                .find(|entity| {
                    entity.kind == EntityKind::LocalVariable
                        && entity.name == assignment.local_name
                        && entity.source_span.as_ref().is_some_and(|span| {
                            span.start_line <= assignment.local_span.start_line
                                && span.end_line >= assignment.local_span.end_line
                        })
                })
                .cloned()
        })
    {
        return entity;
    }

    let scope = containing_executable(entities_by_file, repo_relative_path, &assignment.local_span);
    let qualified_name = scope
        .as_ref()
        .map(|scope| format!("{}.{}", scope.qualified_name, assignment.local_name))
        .unwrap_or_else(|| {
            format!(
                "{}.{}",
                module_name_for_index_path(repo_relative_path),
                assignment.local_name
            )
        });
    let signature = format!(
        "{qualified_name}@{}:{}-{}:{}",
        assignment.local_span.start_line,
        assignment.local_span.start_column.unwrap_or(1),
        assignment.local_span.end_line,
        assignment.local_span.end_column.unwrap_or(1)
    );
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved".into());
    metadata.insert(
        "resolver".to_string(),
        "local_variable_assignment_source".into(),
    );
    metadata.insert("phase".to_string(), "31".into());
    metadata.insert("context".to_string(), "production".into());
    Entity {
        id: stable_entity_id_for_kind(
            repo_relative_path,
            EntityKind::LocalVariable,
            &qualified_name,
            Some(&signature),
        ),
        kind: EntityKind::LocalVariable,
        name: assignment.local_name.clone(),
        qualified_name,
        repo_relative_path: normalize_graph_path(repo_relative_path),
        source_span: Some(assignment.local_span.clone()),
        content_hash: None,
        file_hash: non_empty_file_hash(file_hash),
        created_from: "codegraph-index-security-resolver".to_string(),
        confidence: 1.0,
        metadata,
    }
}

fn parameter_projection_entity_for(
    repo_relative_path: &str,
    function_name: &str,
    parameter_name: &str,
    ordinal: usize,
    span: &SourceSpan,
    file_hash: &str,
) -> Entity {
    let display_name = format!("{function_name}.{parameter_name}");
    let qualified_name = format!(
        "{}.{display_name}",
        module_name_for_index_path(repo_relative_path)
    );
    let signature = format!(
        "{qualified_name}@ordinal:{ordinal}@{}:{}-{}:{}",
        span.start_line,
        span.start_column.unwrap_or(1),
        span.end_line,
        span.end_column.unwrap_or(1)
    );
    let mut metadata = Metadata::new();
    metadata.insert("resolution".to_string(), "resolved".into());
    metadata.insert(
        "resolver".to_string(),
        "function_parameter_projection".into(),
    );
    metadata.insert("phase".to_string(), "16".into());
    metadata.insert("context".to_string(), "production".into());
    metadata.insert("ordinal".to_string(), json!(ordinal));
    Entity {
        id: stable_entity_id_for_kind(
            repo_relative_path,
            EntityKind::Parameter,
            &qualified_name,
            Some(&signature),
        ),
        kind: EntityKind::Parameter,
        name: display_name,
        qualified_name,
        repo_relative_path: normalize_graph_path(repo_relative_path),
        source_span: Some(span.clone()),
        content_hash: None,
        file_hash: non_empty_file_hash(file_hash),
        created_from: "codegraph-index-security-resolver".to_string(),
        confidence: 1.0,
        metadata,
    }
}

fn non_empty_file_hash(file_hash: &str) -> Option<String> {
    (!file_hash.is_empty()).then(|| file_hash.to_string())
}

fn role_literal_for_function(source: &str, entity: &Entity) -> Option<(String, SourceSpan)> {
    let span = entity.source_span.as_ref()?;
    let lines = source.lines().collect::<Vec<_>>();
    let start = span.start_line.saturating_sub(1) as usize;
    let end = span.end_line.max(span.start_line) as usize;
    for (line_index, line) in lines
        .iter()
        .enumerate()
        .take(end.min(lines.len()))
        .skip(start)
    {
        let lower = line.to_ascii_lowercase();
        let checks_role_property =
            lower.contains(".role") || lower.contains("[\"role\"]") || lower.contains("['role']");
        let checks_literal =
            line.contains("===") || line.contains("==") || line.contains("includes(");
        if checks_role_property && checks_literal {
            if let Some((value, literal_start, literal_end)) = first_string_literal_in_text(line) {
                return Some((
                    value,
                    source_span_for_byte_range(
                        &entity.repo_relative_path,
                        line,
                        line_index,
                        literal_start,
                        literal_end,
                    ),
                ));
            }
        }
    }
    None
}

fn direct_role_check_for_function(
    source: &str,
    repo_relative_path: &str,
    entity: &Entity,
) -> Option<DirectRoleCheckCall> {
    let span = entity.source_span.as_ref()?;
    direct_role_check_calls(source, repo_relative_path)
        .into_iter()
        .find(|check| span_contains(span, &check.call_span))
}

fn is_role_middleware_helper_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("require")
        || lower.ends_with("middleware")
        || lower.contains("guard")
        || lower.contains("authorize")
        || lower.contains("auth")
}

fn direct_role_check_calls(source: &str, repo_relative_path: &str) -> Vec<DirectRoleCheckCall> {
    ["requireRole", "checkRole"]
        .into_iter()
        .flat_map(|name| call_records_for_local_name(source, repo_relative_path, name))
        .filter_map(|call| {
            let (role, literal_start, literal_end) = first_string_literal_in_text(&call.args)?;
            let start = call.args_start_byte + literal_start;
            let end = call.args_start_byte + literal_end;
            let line = source.lines().nth(call.line_index)?;
            Some(DirectRoleCheckCall {
                role,
                role_span: source_span_for_byte_range(
                    repo_relative_path,
                    line,
                    call.line_index,
                    start,
                    end,
                ),
                call_span: call.span,
            })
        })
        .collect()
}

fn route_exposure_specs(source: &str, repo_relative_path: &str) -> Vec<RouteExposureSpec> {
    let mut specs = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let mut search_start = 0usize;
        while let Some(offset) = line[search_start..].find("route") {
            let start = search_start + offset;
            let after_name = start + "route".len();
            if !identifier_boundary_before(line, start)
                || !identifier_boundary_after(line, after_name)
                || !is_code_byte_position(line, start)
            {
                search_start = after_name;
                continue;
            }
            let rest = &line[after_name..];
            let whitespace = rest
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
            let open_paren = after_name + whitespace;
            if !line[open_paren..].starts_with('(') {
                search_start = after_name;
                continue;
            }
            let Some(close_paren) = matching_close_paren(line, open_paren) else {
                search_start = open_paren + 1;
                continue;
            };
            let args = split_top_level_args(&line[open_paren + 1..close_paren]);
            if args.len() < 2 {
                search_start = close_paren + 1;
                continue;
            }
            let Some(method) = strip_string_literal(args[0].trim()) else {
                search_start = close_paren + 1;
                continue;
            };
            if !is_http_method(&method) {
                search_start = close_paren + 1;
                continue;
            }
            let Some(path) = strip_string_literal(args[1].trim()) else {
                search_start = close_paren + 1;
                continue;
            };
            if !path.starts_with('/') {
                search_start = close_paren + 1;
                continue;
            }
            let route_name = route_assignment_name(line, start)
                .unwrap_or_else(|| format!("{} {}", method.to_ascii_uppercase(), path));
            let guard_name = args
                .get(2)
                .map(|value| value.trim())
                .filter(|value| looks_like_identifier(value))
                .map(ToString::to_string);
            specs.push(RouteExposureSpec {
                route_name,
                method: method.to_ascii_uppercase(),
                path,
                guard_name,
                span: SourceSpan::with_columns(
                    repo_relative_path,
                    line_index as u32 + 1,
                    line[..start].chars().count() as u32 + 1,
                    line_index as u32 + 1,
                    line[..close_paren + 1].chars().count() as u32 + 1,
                ),
            });
            search_start = close_paren + 1;
        }
    }
    specs
}

fn route_assignment_name(line: &str, route_start: usize) -> Option<String> {
    let before = &line[..route_start];
    let equal = before.rfind('=')?;
    let left = before[..equal].trim();
    let name = left
        .strip_prefix("export const ")
        .or_else(|| left.strip_prefix("const "))
        .or_else(|| left.strip_prefix("let "))
        .or_else(|| left.strip_prefix("var "))?
        .trim();
    if looks_like_identifier(name) {
        Some(name.to_string())
    } else {
        None
    }
}

fn is_http_method(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS" | "HEAD" | "ALL"
    )
}

fn looks_like_sanitizer_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    normalized.contains("sanitize")
        || normalized.starts_with("escape")
        || normalized.contains("cleanhtml")
        || normalized == "xss"
}

fn local_property_flows(
    source: &str,
    repo_relative_path: &str,
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    file_hash: &str,
) -> Vec<LocalSecurityFlow> {
    let assignments = local_property_assignments(source, repo_relative_path);
    if assignments.is_empty() {
        return Vec::new();
    }
    let parameters = function_parameter_projection_entities(
        source,
        repo_relative_path,
        entities_by_file,
        file_hash,
    );
    let mut flows = Vec::new();
    for (function_name, params) in parameters {
        for call in call_records_for_local_name(source, repo_relative_path, &function_name) {
            let Some((argument, argument_span)) = single_argument(&call) else {
                continue;
            };
            let Some((source_expr, source_span)) = assignments.get(&argument) else {
                continue;
            };
            let Some(parameter) = params.first() else {
                continue;
            };
            let head = property_entity_for(
                repo_relative_path,
                source_expr,
                source_span,
                file_hash,
                "local_property_assignment_source",
            );
            flows.push(LocalSecurityFlow {
                head,
                tail: parameter.clone(),
                span: argument_span,
            });
        }
    }
    flows
}

fn local_variable_call_flows(
    source: &str,
    repo_relative_path: &str,
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    file_hash: &str,
) -> Vec<LocalSecurityFlow> {
    let assignments = local_security_assignments(source, repo_relative_path);
    if assignments.is_empty() {
        return Vec::new();
    }
    let mut flows = Vec::new();
    for target in entities_by_file
        .get(repo_relative_path)
        .into_iter()
        .flatten()
        .filter(|entity| matches!(entity.kind, EntityKind::Function | EntityKind::Method))
    {
        for call in call_records_for_local_name(source, repo_relative_path, &target.name) {
            let Some((argument, _argument_span)) = single_argument(&call) else {
                continue;
            };
            let Some(root_assignment) = root_local_assignment(&assignments, &argument) else {
                continue;
            };
            let head = local_variable_entity_for_assignment(
                entities_by_file,
                repo_relative_path,
                root_assignment,
                file_hash,
            );
            flows.push(LocalSecurityFlow {
                head,
                tail: target.clone(),
                span: call.span,
            });
        }
    }
    flows
}

fn local_property_assignments(
    source: &str,
    repo_relative_path: &str,
) -> BTreeMap<String, (String, SourceSpan)> {
    local_security_assignments(source, repo_relative_path)
        .into_iter()
        .filter_map(|(local_name, assignment)| {
            assignment
                .source_property
                .map(|property| (local_name, (property, assignment.source_span)))
        })
        .collect()
}

fn local_security_assignments(
    source: &str,
    repo_relative_path: &str,
) -> BTreeMap<String, LocalAssignment> {
    let mut assignments = BTreeMap::new();
    for (line_index, line) in source.lines().enumerate() {
        let Some(keyword_start) = line.find(|ch: char| !ch.is_whitespace()) else {
            continue;
        };
        if !is_code_byte_position(line, keyword_start) {
            continue;
        }
        let rest = &line[keyword_start..];
        let Some(after_keyword) = rest
            .strip_prefix("const ")
            .or_else(|| rest.strip_prefix("let "))
            .or_else(|| rest.strip_prefix("var "))
        else {
            continue;
        };
        let Some(equal_offset) = after_keyword.find('=') else {
            continue;
        };
        let left = after_keyword[..equal_offset].trim();
        let local_name = left
            .split([':', ' ', '\t'])
            .next()
            .map(str::trim)
            .unwrap_or_default();
        if !looks_like_identifier(local_name) {
            continue;
        }
        let Some(local_offset_in_left) = left.find(local_name) else {
            continue;
        };
        let local_start = keyword_start
            + rest
                .find(after_keyword)
                .expect("after_keyword came from rest prefix strip")
            + local_offset_in_left;
        let local_span = source_span_for_byte_range(
            repo_relative_path,
            line,
            line_index,
            local_start,
            local_start + local_name.len(),
        );
        let right_raw = &after_keyword[equal_offset + 1..];
        let expression = right_raw
            .split(';')
            .next()
            .map(str::trim)
            .unwrap_or_default();
        let Some(expression_offset) = line.find(expression) else {
            continue;
        };
        let span = source_span_for_byte_range(
            repo_relative_path,
            line,
            line_index,
            expression_offset,
            expression_offset + expression.len(),
        );
        let source_property =
            looks_like_property_access(expression).then(|| expression.to_string());
        let source_local = source_local_from_expression(expression);
        if source_property.is_none() && source_local.is_none() {
            continue;
        }
        assignments.insert(
            local_name.to_string(),
            LocalAssignment {
                local_name: local_name.to_string(),
                local_span,
                source_expr: expression.to_string(),
                source_span: span,
                source_local,
                source_property,
            },
        );
    }
    assignments
}

fn source_local_from_expression(expression: &str) -> Option<String> {
    let trimmed = expression.trim();
    if looks_like_identifier(trimmed) {
        return Some(trimmed.to_string());
    }
    let candidate = trimmed.split_once('.')?.0.trim();
    looks_like_identifier(candidate).then(|| candidate.to_string())
}

fn root_local_assignment<'a>(
    assignments: &'a BTreeMap<String, LocalAssignment>,
    local_name: &str,
) -> Option<&'a LocalAssignment> {
    let mut current = local_name;
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(current.to_string()) {
            return None;
        }
        let assignment = assignments.get(current)?;
        let Some(next) = assignment.source_local.as_deref() else {
            return Some(assignment);
        };
        if !assignments.contains_key(next) {
            return Some(assignment);
        }
        current = next;
    }
}

fn function_parameter_projection_entities(
    source: &str,
    repo_relative_path: &str,
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    file_hash: &str,
) -> BTreeMap<String, Vec<Entity>> {
    let mut parameters = BTreeMap::<String, Vec<Entity>>::new();
    for function in entities_by_file
        .get(repo_relative_path)
        .into_iter()
        .flatten()
        .filter(|entity| matches!(entity.kind, EntityKind::Function | EntityKind::Method))
    {
        for (ordinal, name, span) in parameter_declarations_for_function(source, function) {
            let entity = parameter_projection_entity_for(
                repo_relative_path,
                &function.name,
                &name,
                ordinal,
                &span,
                file_hash,
            );
            parameters
                .entry(function.name.clone())
                .or_default()
                .push(entity);
        }
    }
    parameters
}

fn parameter_declarations_for_function(
    source: &str,
    function: &Entity,
) -> Vec<(usize, String, SourceSpan)> {
    let Some(span) = function.source_span.as_ref() else {
        return Vec::new();
    };
    let lines = source.lines().collect::<Vec<_>>();
    let start = span.start_line.saturating_sub(1) as usize;
    let end = span.end_line.max(span.start_line) as usize;
    for (line_index, line) in lines
        .iter()
        .enumerate()
        .take(end.min(lines.len()))
        .skip(start)
    {
        let Some(name_start) = line.find(&function.name) else {
            continue;
        };
        if !identifier_boundary_before(line, name_start)
            || !identifier_boundary_after(line, name_start + function.name.len())
        {
            continue;
        }
        let after_name = name_start + function.name.len();
        let Some(open_offset) = line[after_name..].find('(') else {
            continue;
        };
        let open = after_name + open_offset;
        let Some(close_offset) = line[open..].find(')') else {
            continue;
        };
        let close = open + close_offset;
        let parameter_text = &line[open + 1..close];
        let mut declarations = Vec::new();
        let mut search_start = 0usize;
        for (ordinal, raw) in parameter_text.split(',').enumerate() {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                search_start += raw.len() + 1;
                continue;
            }
            let name = trimmed
                .split([':', '?', '=', ' ', '\t'])
                .next()
                .map(str::trim)
                .unwrap_or_default();
            if !looks_like_identifier(name) {
                search_start += raw.len() + 1;
                continue;
            }
            let Some(raw_offset) = parameter_text[search_start..].find(trimmed) else {
                search_start += raw.len() + 1;
                continue;
            };
            let trimmed_start = search_start + raw_offset;
            let Some(name_offset) = trimmed.find(name) else {
                search_start += raw.len() + 1;
                continue;
            };
            let start_byte = open + 1 + trimmed_start + name_offset;
            let end_byte = start_byte + name.len();
            declarations.push((
                ordinal,
                name.to_string(),
                source_span_for_byte_range(
                    &function.repo_relative_path,
                    line,
                    line_index,
                    start_byte,
                    end_byte,
                ),
            ));
            search_start += raw.len() + 1;
        }
        return declarations;
    }
    Vec::new()
}

fn first_string_literal_in_text(text: &str) -> Option<(String, usize, usize)> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let quote = bytes[index];
        if quote != b'"' && quote != b'\'' {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        let mut escaped = false;
        while end < bytes.len() {
            let byte = bytes[end];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == quote {
                let value = &text[index + 1..end];
                if !value.trim().is_empty() {
                    return Some((value.to_string(), index + 1, end));
                }
                break;
            }
            end += 1;
        }
        index = end.saturating_add(1);
    }
    None
}

fn strip_string_literal(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let first = trimmed.chars().next()?;
    let last = trimmed.chars().last()?;
    if matches!(first, '"' | '\'' | '`') && first == last && trimmed.len() >= 2 {
        Some(trimmed[1..trimmed.len() - 1].to_string())
    } else {
        None
    }
}

fn single_argument(call: &SimpleCallRecord) -> Option<(String, SourceSpan)> {
    if call.args.contains(',') {
        return None;
    }
    let argument = call.args.trim();
    if !looks_like_identifier(argument) && !looks_like_property_access(argument) {
        return None;
    }
    let local_offset = call.args.find(argument)?;
    let start = call.args_start_byte + local_offset;
    let end = start + argument.len();
    Some((
        argument.to_string(),
        source_span_for_byte_range(
            &call.span.repo_relative_path,
            &call.line,
            call.line_index,
            start,
            end,
        ),
    ))
}

fn looks_like_property_access(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    looks_like_identifier(first)
        && parts.clone().next().is_some()
        && parts.all(looks_like_identifier)
}

fn source_span_for_byte_range(
    repo_relative_path: &str,
    line: &str,
    line_index: usize,
    start: usize,
    end: usize,
) -> SourceSpan {
    SourceSpan::with_columns(
        repo_relative_path,
        line_index as u32 + 1,
        line[..start].chars().count() as u32 + 1,
        line_index as u32 + 1,
        line[..end].chars().count() as u32 + 1,
    )
}

fn local_declaration_shadows_import(
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    repo_relative_path: &str,
    local_name: &str,
    import_span: &SourceSpan,
    call_span: &SourceSpan,
) -> bool {
    let call_scope = containing_executable(entities_by_file, repo_relative_path, call_span);
    entities_by_file
        .get(repo_relative_path)
        .is_some_and(|entities| {
            entities.iter().any(|entity| {
                let Some(span) = entity.source_span.as_ref() else {
                    return false;
                };
                entity.name == local_name
                    && matches!(
                        entity.kind,
                        EntityKind::Function
                            | EntityKind::Method
                            | EntityKind::Constructor
                            | EntityKind::Class
                            | EntityKind::LocalVariable
                            | EntityKind::GlobalVariable
                            | EntityKind::Parameter
                    )
                    && entity.created_from != "codegraph-index-static-import-resolver"
                    && entity.created_from != "tree-sitter-static-heuristic"
                    && !entity.qualified_name.starts_with("static_reference:")
                    && span.start_line > import_span.end_line
                    && (span_contains(span, call_span)
                        || call_scope.as_ref().is_some_and(|scope| {
                            scope.source_span.as_ref().is_some_and(|scope_span| {
                                span_contains(scope_span, span)
                                    && span_contains(scope_span, call_span)
                            })
                        }))
            })
        })
}

fn span_contains(outer: &SourceSpan, inner: &SourceSpan) -> bool {
    normalize_graph_path(&outer.repo_relative_path)
        == normalize_graph_path(&inner.repo_relative_path)
        && outer.start_line <= inner.start_line
        && outer.end_line >= inner.end_line
}

fn call_spans_for_local_name(
    source: &str,
    repo_relative_path: &str,
    local_name: &str,
) -> Vec<SourceSpan> {
    call_records_for_local_name(source, repo_relative_path, local_name)
        .into_iter()
        .map(|record| record.span)
        .collect()
}

fn call_records_for_local_name(
    source: &str,
    repo_relative_path: &str,
    local_name: &str,
) -> Vec<SimpleCallRecord> {
    let mut records = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let mut search_start = 0usize;
        while let Some(offset) = line[search_start..].find(local_name) {
            let start = search_start + offset;
            let after_name = start + local_name.len();
            if !identifier_boundary_before(line, start)
                || !identifier_boundary_after(line, after_name)
                || !is_code_byte_position(line, start)
            {
                search_start = after_name;
                continue;
            }
            let rest = &line[after_name..];
            let whitespace = rest
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
            let open_paren = after_name + whitespace;
            if !line[open_paren..].starts_with('(') {
                search_start = after_name;
                continue;
            }
            let Some(close_paren) = matching_close_paren(line, open_paren) else {
                search_start = open_paren + 1;
                continue;
            };
            let end = close_paren + 1;
            records.push(SimpleCallRecord {
                span: SourceSpan::with_columns(
                    repo_relative_path,
                    line_index as u32 + 1,
                    line[..start].chars().count() as u32 + 1,
                    line_index as u32 + 1,
                    line[..end].chars().count() as u32 + 1,
                ),
                line_index,
                line: line.to_string(),
                args_start_byte: open_paren + 1,
                args: line[open_paren + 1..close_paren].to_string(),
            });
            search_start = end;
        }
    }
    records
}

fn is_code_byte_position(line: &str, byte_index: usize) -> bool {
    let mut quote = None;
    let mut escaped = false;
    let mut previous = '\0';
    for (index, ch) in line.char_indices() {
        if index >= byte_index {
            break;
        }
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            previous = ch;
            continue;
        }
        if matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
            previous = ch;
            continue;
        }
        if previous == '/' && ch == '/' {
            return false;
        }
        previous = ch;
    }
    quote.is_none()
}

fn containing_executable(
    entities_by_file: &BTreeMap<String, Vec<Entity>>,
    repo_relative_path: &str,
    span: &SourceSpan,
) -> Option<Entity> {
    entities_by_file
        .get(repo_relative_path)
        .and_then(|entities| {
            entities
                .iter()
                .filter(|entity| {
                    matches!(
                        entity.kind,
                        EntityKind::Function | EntityKind::Method | EntityKind::Constructor
                    ) && entity.created_from != "tree-sitter-static-heuristic"
                        && !entity.qualified_name.starts_with("static_reference:")
                })
                .filter(|entity| {
                    entity.source_span.as_ref().is_some_and(|entity_span| {
                        entity_span.start_line <= span.start_line
                            && entity_span.end_line >= span.end_line
                    })
                })
                .min_by_key(|entity| {
                    entity
                        .source_span
                        .as_ref()
                        .map(|entity_span| {
                            entity_span.end_line.saturating_sub(entity_span.start_line)
                        })
                        .unwrap_or(u32::MAX)
                })
                .cloned()
        })
}

fn first_test_case(test_cases: &[Entity]) -> Option<Entity> {
    test_cases
        .iter()
        .min_by_key(|entity| {
            entity
                .source_span
                .as_ref()
                .map(|span| (span.start_line, span.start_column.unwrap_or(1)))
                .unwrap_or((u32::MAX, u32::MAX))
        })
        .cloned()
}

fn containing_test_case(test_cases: &[Entity], span: &SourceSpan) -> Option<Entity> {
    test_cases
        .iter()
        .filter(|entity| {
            entity
                .source_span
                .as_ref()
                .is_some_and(|candidate| span_contains(candidate, span))
        })
        .min_by_key(|entity| {
            entity
                .source_span
                .as_ref()
                .map(|candidate| candidate.end_line.saturating_sub(candidate.start_line))
                .unwrap_or(u32::MAX)
        })
        .cloned()
}

fn is_test_file_path_for_index(path: &str) -> bool {
    let normalized = normalize_graph_path(path).to_ascii_lowercase();
    let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);
    normalized.contains("/tests/")
        || normalized.contains("/test/")
        || normalized.contains("/spec/")
        || normalized.ends_with(".test.ts")
        || normalized.ends_with(".test.tsx")
        || normalized.ends_with(".test.js")
        || normalized.ends_with(".test.jsx")
        || normalized.ends_with(".spec.ts")
        || normalized.ends_with(".spec.tsx")
        || normalized.ends_with(".spec.js")
        || normalized.ends_with(".spec.jsx")
        || normalized.ends_with("_test.go")
        || normalized.ends_with("_test.py")
        || normalized.ends_with("_test.rb")
        || normalized.ends_with("_spec.rb")
        || normalized.ends_with("_test.php")
        || normalized.ends_with("_spec.php")
        || (file_name.starts_with("test_") && file_name.ends_with(".py"))
        || (file_name.starts_with("test_") && file_name.ends_with(".rb"))
        || (file_name.starts_with("test_") && file_name.ends_with(".php"))
        || file_name.ends_with("test.java")
        || file_name.ends_with("tests.java")
        || file_name.ends_with("spec.java")
        || file_name.ends_with("test.cs")
        || file_name.ends_with("tests.cs")
        || file_name.ends_with("spec.cs")
        || file_name.ends_with("test.php")
        || file_name.ends_with("testcase.php")
        || file_name.ends_with("test.rb")
        || file_name.ends_with("spec.rb")
}

fn source_may_have_test_relation(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.contains("expect(")
        || lower.contains("describe(")
        || lower.contains(" it(")
        || lower.trim_start().starts_with("it(")
        || lower.contains("\nit(")
        || lower.contains(" test(")
        || lower.trim_start().starts_with("test(")
        || lower.contains("\ntest(")
        || lower.contains("assert")
        || lower.contains(".mock(")
        || lower.contains("jest.mock")
        || lower.contains("vi.mock")
        || lower.contains("stub")
}

fn identifier_boundary_before(line: &str, start: usize) -> bool {
    line[..start]
        .chars()
        .last()
        .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
}

fn identifier_boundary_after(line: &str, end: usize) -> bool {
    line[end..]
        .chars()
        .next()
        .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
}

fn looks_like_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$'))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestFileDecision {
    MetadataUnchanged,
    ChangedOrUnknown,
}

#[derive(Debug, Clone)]
struct ManifestDiffEngine {
    existing_by_path: BTreeMap<String, FileRecord>,
    stale_missing_by_path: BTreeMap<String, FileRecord>,
    renamed_old_paths: BTreeSet<String>,
}

impl ManifestDiffEngine {
    fn new(existing_files: Vec<FileRecord>, current_repo_paths: &BTreeSet<String>) -> Self {
        let mut existing_by_path = BTreeMap::new();
        let mut stale_missing_by_path = BTreeMap::new();
        for file in existing_files {
            let normalized_path = normalize_graph_path(&file.repo_relative_path);
            if current_repo_paths.contains(&normalized_path) {
                existing_by_path.insert(normalized_path, file);
            } else {
                stale_missing_by_path.insert(normalized_path, file);
            }
        }
        Self {
            existing_by_path,
            stale_missing_by_path,
            renamed_old_paths: BTreeSet::new(),
        }
    }

    fn existing_file(&self, repo_relative_path: &str) -> Option<&FileRecord> {
        self.existing_by_path
            .get(&normalize_graph_path(repo_relative_path))
    }

    fn classify_file(
        &self,
        repo_relative_path: &str,
        size_bytes: u64,
        metadata: &fs::Metadata,
    ) -> ManifestFileDecision {
        match self.existing_file(repo_relative_path) {
            Some(record) if manifest_metadata_matches(record, size_bytes, metadata) => {
                ManifestFileDecision::MetadataUnchanged
            }
            _ => ManifestFileDecision::ChangedOrUnknown,
        }
    }

    fn record_rename_matches(&mut self, repo_root: &Path, current_path: &str, hash: &str) -> usize {
        let current_path = normalize_graph_path(current_path);
        let matches = self
            .stale_missing_by_path
            .iter()
            .filter_map(|(path, file)| {
                if path == &current_path || file.file_hash != hash || repo_root.join(path).exists()
                {
                    None
                } else {
                    Some(path.clone())
                }
            })
            .collect::<Vec<_>>();
        let count = matches.len();
        for path in matches {
            self.renamed_old_paths.insert(path);
        }
        count
    }

    fn stale_cleanup_paths(&self) -> Vec<String> {
        self.stale_missing_by_path.keys().cloned().collect()
    }
}

fn manifest_metadata_matches(
    record: &FileRecord,
    size_bytes: u64,
    metadata: &fs::Metadata,
) -> bool {
    record.size_bytes == size_bytes
        && modified_unix_nanos(metadata).is_some_and(|modified| {
            record
                .metadata
                .get("modified_unix_nanos")
                .and_then(serde_json::Value::as_str)
                == Some(modified.as_str())
        })
        && record
            .metadata
            .get(FILE_LIFECYCLE_STATE_KEY)
            .and_then(serde_json::Value::as_str)
            == Some(FILE_LIFECYCLE_STATE_CURRENT)
        && record
            .metadata
            .get(FILE_LIFECYCLE_POLICY_KEY)
            .and_then(serde_json::Value::as_str)
            == Some(FILE_LIFECYCLE_POLICY_CURRENT_ONLY)
}

fn modified_unix_nanos(metadata: &fs::Metadata) -> Option<String> {
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(duration.as_nanos().to_string())
}

fn file_manifest_metadata(modified_unix_nanos: Option<String>) -> Metadata {
    let mut metadata = Metadata::new();
    if let Some(modified_unix_nanos) = modified_unix_nanos {
        metadata.insert(
            "modified_unix_nanos".to_string(),
            modified_unix_nanos.into(),
        );
    }
    metadata.insert("manifest_diff".to_string(), "metadata_first".into());
    metadata.insert(
        FILE_LIFECYCLE_STATE_KEY.to_string(),
        FILE_LIFECYCLE_STATE_CURRENT.into(),
    );
    metadata.insert(
        FILE_LIFECYCLE_POLICY_KEY.to_string(),
        FILE_LIFECYCLE_POLICY_CURRENT_ONLY.into(),
    );
    metadata.insert(
        FILE_HISTORICAL_VISIBILITY_KEY.to_string(),
        FILE_HISTORICAL_VISIBILITY_HIDDEN.into(),
    );
    metadata.insert(
        FILE_STALE_CLEANUP_KEY.to_string(),
        FILE_STALE_CLEANUP_DELETE_BEFORE_INSERT.into(),
    );
    metadata
}

fn file_manifest_metadata_with_parser_status(
    modified_unix_nanos: Option<String>,
    parser_metadata: Metadata,
) -> Metadata {
    let mut metadata = file_manifest_metadata(modified_unix_nanos);
    metadata.extend(parser_metadata);
    metadata
}

fn parser_error_file_metadata(
    modified_unix_nanos: Option<String>,
    language: Option<&str>,
    error_message: &str,
) -> Metadata {
    let mut metadata = file_manifest_metadata(modified_unix_nanos);
    metadata.insert("parser_status".to_string(), "parser_error".into());
    metadata.insert("claim_state".to_string(), "unsupported".into());
    metadata.insert(
        "unsupported_behavior_label".to_string(),
        "parser_invocation_failed".into(),
    );
    metadata.insert("parser_error".to_string(), true.into());
    metadata.insert("parser_error_message".to_string(), error_message.into());
    metadata.insert("graph_relation_claims".to_string(), json!([]));
    if let Some(language) = language {
        metadata.insert("parser_frontend".to_string(), language.into());
    }
    metadata
}

fn classify_scoped_text_evidence_path(repo_relative_path: &str) -> Option<TextEvidenceFileKind> {
    let normalized = normalize_graph_path(repo_relative_path);
    let lower = normalized.to_ascii_lowercase();
    if is_text_evidence_hard_excluded_path(&lower) {
        return None;
    }

    let file_name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    if file_name == "expected_text_evidence.json" {
        return Some(TextEvidenceFileKind::FixtureManifest);
    }
    if lower.ends_with(".mk") {
        return Some(TextEvidenceFileKind::MakefileFragment);
    }
    if file_name == "config.in"
        || file_name == "kconfig"
        || file_name.starts_with("kconfig.")
        || lower.contains("/kconfig/")
    {
        return Some(TextEvidenceFileKind::Kconfig);
    }
    if lower.ends_with(".adoc") || lower.ends_with(".asciidoc") {
        return Some(TextEvidenceFileKind::Asciidoc);
    }
    if lower.ends_with(".md") || lower.ends_with(".markdown") {
        return Some(TextEvidenceFileKind::Markdown);
    }
    if lower.ends_with(".sh")
        || lower.ends_with(".bash")
        || lower.ends_with(".zsh")
        || lower.ends_with(".fish")
    {
        return Some(TextEvidenceFileKind::ShellScript);
    }
    if (path_starts_with_normalized(&lower, "support/scripts")
        || path_starts_with_normalized(&lower, "support/download"))
        && lower.ends_with(".py")
    {
        return Some(TextEvidenceFileKind::PythonSupportScript);
    }
    if path_starts_with_normalized(&lower, "package")
        && (lower.ends_with(".hash") || lower.ends_with(".mk") || file_name == "config.in")
    {
        return Some(TextEvidenceFileKind::BuildrootPackageMetadata);
    }
    if !file_name.contains('.')
        && (path_starts_with_normalized(&lower, "support/scripts")
            || path_starts_with_normalized(&lower, "support/download"))
    {
        return Some(TextEvidenceFileKind::TextLikeSupportScript);
    }

    None
}

fn is_text_evidence_hard_excluded_path(lower: &str) -> bool {
    if lower == "reports/final" || path_starts_with_normalized(lower, "reports/final") {
        return true;
    }
    if path_starts_with_normalized(lower, "reports/audit/artifacts") {
        return true;
    }
    if lower.ends_with(".sqlite")
        || lower.ends_with(".sqlite3")
        || lower.ends_with(".db")
        || lower.ends_with(".db-wal")
        || lower.ends_with(".db-shm")
        || lower.ends_with(".sqlite-wal")
        || lower.ends_with(".sqlite-shm")
        || lower.ends_with(".sqlite3-wal")
        || lower.ends_with(".sqlite3-shm")
        || lower.ends_with(".log")
    {
        return true;
    }
    lower.split('/').any(|component| {
        matches!(
            component,
            ".git"
                | "target"
                | "node_modules"
                | ".venv"
                | "venv"
                | "env"
                | "__pycache__"
                | ".pytest_cache"
                | ".mypy_cache"
                | ".ruff_cache"
                | ".tox"
                | ".cache"
                | "coverage"
                | ".codegraph"
        )
    })
}

fn path_starts_with_normalized(path: &str, prefix: &str) -> bool {
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

fn looks_like_text_evidence_source(source: &str) -> bool {
    if source.contains('\0') {
        return false;
    }
    let mut chars = 0usize;
    let mut suspicious_controls = 0usize;
    for ch in source.chars().take(8192) {
        chars += 1;
        if ch.is_control() && !matches!(ch, '\n' | '\r' | '\t') {
            suspicious_controls += 1;
        }
    }
    chars == 0 || suspicious_controls.saturating_mul(100) <= chars.saturating_mul(2)
}

fn build_text_evidence_index(repo_relative_path: &str, source: &str) -> TextEvidenceIndex {
    let fts_body = bounded_text_prefix(source, TEXT_EVIDENCE_MAX_FTS_BYTES_PER_FILE).to_string();
    let indexed_bytes = fts_body.len();
    let total_bytes = source.len();
    let omitted_bytes = total_bytes.saturating_sub(indexed_bytes);
    let total_lines = source.lines().count().max(1);
    let indexed_lines = fts_body.lines().count().max(1);
    let tokens = extract_text_evidence_tokens(&fts_body);
    let snippets = build_text_evidence_snippets(repo_relative_path, &fts_body);

    TextEvidenceIndex {
        fts_body,
        snippets,
        tokens,
        indexed_bytes,
        total_bytes,
        omitted_bytes,
        indexed_lines,
        total_lines,
    }
}

fn bounded_text_prefix(source: &str, max_bytes: usize) -> &str {
    if source.len() <= max_bytes {
        return source;
    }
    let mut end = max_bytes;
    while end > 0 && !source.is_char_boundary(end) {
        end -= 1;
    }
    &source[..end]
}

fn extract_text_evidence_tokens(source: &str) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    for raw in
        source.split(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')))
    {
        let token = raw.trim_matches(|ch: char| matches!(ch, '-' | '.'));
        if token.len() < 2 || !token.chars().any(|ch| ch.is_ascii_alphabetic()) {
            continue;
        }
        tokens.insert(token.to_string());
        if tokens.len() >= TEXT_EVIDENCE_MAX_TOKENS_PER_FILE {
            break;
        }
    }
    tokens.into_iter().collect()
}

fn build_text_evidence_snippets(
    repo_relative_path: &str,
    source: &str,
) -> Vec<TextEvidenceSnippet> {
    let mut selected = BTreeMap::<u32, String>::new();
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_text_evidence_interesting_line(trimmed) {
            selected.insert((line_index + 1) as u32, bounded_snippet_text(trimmed));
        }
        if selected.len() >= TEXT_EVIDENCE_MAX_SNIPPETS_PER_FILE {
            break;
        }
    }
    if selected.len() < TEXT_EVIDENCE_MAX_SNIPPETS_PER_FILE {
        for (line_index, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            selected
                .entry((line_index + 1) as u32)
                .or_insert_with(|| bounded_snippet_text(trimmed));
            if selected.len() >= TEXT_EVIDENCE_MAX_SNIPPETS_PER_FILE {
                break;
            }
        }
    }

    selected
        .into_iter()
        .map(|(line, text)| TextEvidenceSnippet {
            id: format!("text_evidence:{repo_relative_path}:{line}"),
            span: SourceSpan::new(repo_relative_path, line, line),
            text,
        })
        .collect()
}

fn bounded_snippet_text(line: &str) -> String {
    bounded_text_prefix(line, TEXT_EVIDENCE_MAX_SNIPPET_BYTES).to_string()
}

fn is_text_evidence_interesting_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    line.contains("BR2_PACKAGE_")
        || line.contains("generic-package")
        || line.contains("host-generic-package")
        || line.contains("Config.in")
        || line.contains("_VERSION")
        || line.contains("_SITE")
        || line.contains("_LICENSE")
        || line.contains("_DEPENDENCIES")
        || lower.contains("depends on")
        || lower.contains("select ")
        || lower.contains("package infrastructure")
        || lower.contains("source url")
}

fn text_evidence_file_metadata(
    modified_unix_nanos: Option<String>,
    kind: TextEvidenceFileKind,
    evidence: &TextEvidenceIndex,
) -> Metadata {
    let mut metadata = file_manifest_metadata(modified_unix_nanos);
    metadata.insert("evidence_kind".to_string(), TEXT_EVIDENCE_KIND.into());
    metadata.insert("evidence_role".to_string(), TEXT_EVIDENCE_KIND.into());
    metadata.insert(
        "proof_status".to_string(),
        TEXT_EVIDENCE_PROOF_STATUS.into(),
    );
    metadata.insert("graph_proof".to_string(), false.into());
    metadata.insert("source_file_kind".to_string(), kind.as_str().into());
    metadata.insert("source_file_label".to_string(), kind.title().into());
    metadata.insert(
        "text_evidence".to_string(),
        json!({
            "bounded": true,
            "max_read_bytes_per_file": TEXT_EVIDENCE_MAX_READ_BYTES_PER_FILE,
            "max_fts_bytes_per_file": TEXT_EVIDENCE_MAX_FTS_BYTES_PER_FILE,
            "max_snippets_per_file": TEXT_EVIDENCE_MAX_SNIPPETS_PER_FILE,
            "max_tokens_per_file": TEXT_EVIDENCE_MAX_TOKENS_PER_FILE,
            "indexed_bytes": evidence.indexed_bytes,
            "total_bytes": evidence.total_bytes,
            "omitted_bytes": evidence.omitted_bytes,
            "indexed_lines": evidence.indexed_lines,
            "total_lines": evidence.total_lines,
            "omitted_lines": evidence.total_lines.saturating_sub(evidence.indexed_lines),
            "snippet_count": evidence.snippets.len(),
            "tokens": evidence.tokens.clone(),
            "claimable_as": ["source_text_existence"],
            "not_claimable_as": [
                "typed_graph_relation",
                "CALLS",
                "READS",
                "WRITES",
                "FLOWS_TO",
                "MUTATES",
                "TESTS",
                "ASSERTS"
            ],
            "graph_relation_claims": []
        }),
    );
    metadata
}

fn file_record_is_text_evidence_kind(
    record: &FileRecord,
    expected_kind: TextEvidenceFileKind,
) -> bool {
    record.language.is_none()
        && record.metadata.get("evidence_kind").and_then(Value::as_str) == Some(TEXT_EVIDENCE_KIND)
        && record.metadata.get("proof_status").and_then(Value::as_str)
            == Some(TEXT_EVIDENCE_PROOF_STATUS)
        && record
            .metadata
            .get("source_file_kind")
            .and_then(Value::as_str)
            == Some(expected_kind.as_str())
}

#[allow(clippy::too_many_arguments)]
fn persist_text_evidence_to_writer(
    writer: &SqliteGraphStore,
    repo_relative_path: &str,
    file_hash: &str,
    kind: TextEvidenceFileKind,
    evidence: &TextEvidenceIndex,
    size_bytes: u64,
    indexed_at: u64,
    modified_unix_nanos: Option<String>,
    needs_delete: bool,
) -> Result<(), StoreError> {
    if needs_delete {
        writer.delete_facts_for_file(repo_relative_path)?;
    }
    writer.upsert_file(&FileRecord {
        repo_relative_path: repo_relative_path.to_string(),
        file_hash: file_hash.to_string(),
        language: None,
        size_bytes,
        indexed_at_unix_ms: Some(indexed_at),
        metadata: text_evidence_file_metadata(modified_unix_nanos, kind, evidence),
    })?;
    writer.insert_file_text(repo_relative_path, &evidence.fts_body)?;
    for snippet in &evidence.snippets {
        writer.insert_snippet_text(&snippet.id, &snippet.span, &snippet.text)?;
    }
    Ok(())
}

fn delete_missing_files_with_hash(
    store: &SqliteGraphStore,
    repo_root: &Path,
    current_path: &str,
    hash: &str,
) -> Result<usize, StoreError> {
    let mut deleted = 0usize;
    for file in store.list_files(UNBOUNDED_STORE_READ_LIMIT)? {
        let path = normalize_graph_path(&file.repo_relative_path);
        if path == current_path || file.file_hash != hash {
            continue;
        }
        if repo_root.join(&path).exists() {
            continue;
        }
        store.delete_facts_for_file(&path)?;
        deleted += 1;
    }
    Ok(deleted)
}

fn index_error_as_store_error(error: IndexError) -> StoreError {
    match error {
        IndexError::Store(error) => error,
        other => StoreError::Message(other.to_string()),
    }
}

fn module_name_for_index_path(path: &str) -> String {
    let normalized = normalize_graph_path(path);
    normalized
        .rsplit_once('.')
        .map(|(without_ext, _)| without_ext)
        .unwrap_or(&normalized)
        .replace('/', "::")
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScopedRepoFiles {
    pub files: Vec<PathBuf>,
    pub scope_report: IndexScopeRuntimeReport,
}

pub fn collect_repo_files(root: &Path) -> Result<Vec<PathBuf>, IndexError> {
    Ok(collect_repo_files_with_scope(root, &IndexScopeOptions::default())?.files)
}

pub fn collect_repo_files_with_scope(
    root: &Path,
    options: &IndexScopeOptions,
) -> Result<ScopedRepoFiles, IndexError> {
    let scope = IndexScope::for_repo(root, options.clone());
    let mut files = Vec::new();
    let mut scope_report = IndexScopeRuntimeReport::new(options);
    collect_repo_files_inner(root, root, &scope, &mut scope_report, &mut files)?;
    files.sort();
    Ok(ScopedRepoFiles {
        files,
        scope_report,
    })
}

fn collect_repo_files_inner(
    root: &Path,
    path: &Path,
    scope: &IndexScope,
    scope_report: &mut IndexScopeRuntimeReport,
    files: &mut Vec<PathBuf>,
) -> Result<(), IndexError> {
    if path.is_dir() {
        if path != root {
            let relative = path.strip_prefix(root).unwrap_or(path);
            let relative = relative.to_string_lossy().replace('\\', "/");
            let decision = scope.evaluate_repo_path(&relative, ScopePathKind::Directory);
            emit_scope_decision(scope.options(), &decision);
            let excluded = decision.excluded();
            scope_report.record(&decision);
            if excluded {
                let include_descendant = scope::could_include_descendant_decision(
                    &relative,
                    &scope.options().include_patterns,
                );
                let pruned = !include_descendant.could_include_descendant;
                let reason = if pruned {
                    format!("excluded_directory_pruned: {}", include_descendant.reason)
                } else {
                    format!(
                        "excluded_directory_descended: {}",
                        include_descendant.reason
                    )
                };
                scope_report.record_directory_prune(scope::IndexScopeDirectoryPruneDecision {
                    directory: relative.clone(),
                    excluded_by: decision.matched_rule.clone(),
                    include_patterns_present: scope.options().has_include_patterns(),
                    could_include_descendant: include_descendant.could_include_descendant,
                    pruned,
                    reason,
                });
                if pruned {
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
            collect_repo_files_inner(root, &entry.path(), scope, scope_report, files)?;
        }
    } else if path.is_file() {
        let relative = path.strip_prefix(root).unwrap_or(path);
        let relative = relative.to_string_lossy().replace('\\', "/");
        let decision = scope.evaluate_repo_path(relative, ScopePathKind::File);
        emit_scope_decision(scope.options(), &decision);
        scope_report.record(&decision);
        if !decision.excluded() && !should_skip_file_with_scope(path, &decision, scope.options()) {
            files.push(path.to_path_buf());
        }
    }

    Ok(())
}

pub fn should_ignore_path(root: &Path, path: &Path) -> bool {
    should_ignore_path_with_scope(root, path, &IndexScopeOptions::default())
}

pub fn should_ignore_path_with_scope(
    root: &Path,
    path: &Path,
    options: &IndexScopeOptions,
) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative = relative.to_string_lossy().replace('\\', "/");
    let path_kind = if path.is_dir() {
        ScopePathKind::Directory
    } else {
        ScopePathKind::File
    };
    let scope = IndexScope::for_repo(root, options.clone());
    let decision = scope.evaluate_repo_path(relative, path_kind);
    decision.excluded() || should_skip_file_with_scope(path, &decision, options)
}

fn should_skip_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".min.js")
                || name.ends_with(".bundle.js")
                || name.ends_with(".map")
                || name.ends_with(".lock")
        })
}

fn should_skip_file_with_scope(
    path: &Path,
    decision: &scope::IndexScopeDecision,
    options: &IndexScopeOptions,
) -> bool {
    if decision.path_kind != ScopePathKind::File {
        return false;
    }
    if options.no_default_excludes || decision.rule_kind == scope::ScopeRuleKind::ExplicitInclude {
        return false;
    }
    should_skip_file(path)
}

fn should_skip_graph_extraction_for_large_generated_or_test_source(
    repo_relative_path: &str,
    source_bytes: usize,
) -> bool {
    let normalized = repo_relative_path.replace('\\', "/").to_ascii_lowercase();
    let is_rubi_symbolic_source =
        normalized.contains("/rubi/") || normalized.contains("/rubi_tests/");
    let is_test_or_generated_source = normalized.contains("/generated/")
        || normalized.contains("/fixtures/")
        || normalized.contains("/tests/")
        || normalized.ends_with("_test.py")
        || normalized.ends_with("_test.go")
        || normalized.ends_with(".spec.ts")
        || normalized.ends_with(".spec.tsx")
        || normalized.ends_with(".test.ts")
        || normalized.ends_with(".test.tsx");

    (is_rubi_symbolic_source && source_bytes >= RUBI_GRAPH_EXTRACTION_SKIP_BYTES)
        || (is_test_or_generated_source
            && source_bytes >= LARGE_TEST_OR_GENERATED_GRAPH_EXTRACTION_SKIP_BYTES)
        || ((is_rubi_symbolic_source || is_test_or_generated_source)
            && source_bytes >= LARGE_GENERATED_OR_TEST_GRAPH_EXTRACTION_SKIP_BYTES)
}

fn emit_scope_decision(options: &IndexScopeOptions, decision: &scope::IndexScopeDecision) {
    if !options.has_print_or_explain() {
        return;
    }
    let should_print = match decision.action {
        ScopeAction::WouldExclude => options.print_excluded || options.explain_scope,
        ScopeAction::WouldInclude | ScopeAction::WouldIncludeWithWarning => {
            options.print_included || options.explain_scope
        }
    };
    if should_print {
        eprintln!(
            "scope\t{:?}\t{}\t{:?}\t{}",
            decision.action,
            decision.normalized_path,
            decision.rule_kind,
            decision.matched_rule.as_deref().unwrap_or("none")
        );
    }
    if options.explain_scope && decision.warned() {
        eprintln!(
            "scope-warning\t{}\t{:?}",
            decision.normalized_path, decision.warnings
        );
    }
}

pub fn repo_relative_path(root: &Path, path: &Path) -> Result<String, IndexError> {
    let relative = path.strip_prefix(root).map_err(|_| IndexError::PathStrip {
        path: path.to_path_buf(),
        root: root.to_path_buf(),
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

pub fn normalize_changed_path(root: &Path, path: &Path) -> Result<(PathBuf, String), IndexError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let normalized = normalize_lexical_path(&absolute);
    let normalized = resolve_existing_changed_path_case(root, &normalized);
    let repo_relative_path = repo_relative_path_for_changed_path(root, &normalized)?;
    Ok((normalized, repo_relative_path))
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn normalize_changed_paths_for_update(
    root: &Path,
    changed_paths: &[PathBuf],
    store: &SqliteGraphStore,
) -> Result<Vec<(PathBuf, String)>, IndexError> {
    let known_paths = if cfg!(windows) {
        store
            .list_files(UNBOUNDED_STORE_READ_LIMIT)?
            .into_iter()
            .map(|file| {
                (
                    platform_path_identity_key(&file.repo_relative_path),
                    normalize_graph_path(file.repo_relative_path),
                )
            })
            .collect::<BTreeMap<_, _>>()
    } else {
        BTreeMap::new()
    };
    let mut normalized = changed_paths
        .iter()
        .map(|path| {
            let (mut absolute, mut repo_relative_path) = normalize_changed_path(root, path)?;
            if cfg!(windows) {
                if let Some(known_path) =
                    known_paths.get(&platform_path_identity_key(&repo_relative_path))
                {
                    repo_relative_path = known_path.clone();
                    absolute = root.join(known_path);
                }
            }
            Ok((absolute, repo_relative_path))
        })
        .collect::<Result<Vec<_>, IndexError>>()?;
    normalized.sort_by(|left, right| {
        platform_path_identity_key(&left.1).cmp(&platform_path_identity_key(&right.1))
    });
    normalized.dedup_by(|left, right| {
        platform_path_identity_key(&left.1) == platform_path_identity_key(&right.1)
    });
    Ok(normalized)
}

fn repo_relative_path_for_changed_path(root: &Path, path: &Path) -> Result<String, IndexError> {
    if let Ok(relative) = path.strip_prefix(root) {
        return Ok(path_to_repo_relative_string(relative));
    }
    if cfg!(windows) {
        if let Some(relative) = strip_prefix_case_insensitive(root, path) {
            return Ok(path_to_repo_relative_string(&relative));
        }
    }
    Err(IndexError::PathStrip {
        path: path.to_path_buf(),
        root: root.to_path_buf(),
    })
}

fn path_to_repo_relative_string(path: &Path) -> String {
    normalize_graph_path(path.to_string_lossy().replace('\\', "/"))
}

fn strip_prefix_case_insensitive(root: &Path, path: &Path) -> Option<PathBuf> {
    let root_components = comparable_path_components(root);
    let path_components = comparable_path_components(path);
    if path_components.len() < root_components.len() {
        return None;
    }
    if !root_components
        .iter()
        .zip(path_components.iter())
        .all(|(left, right)| left.eq_ignore_ascii_case(right))
    {
        return None;
    }
    let mut relative = PathBuf::new();
    for component in path.components().skip(root_components.len()) {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

fn comparable_path_components(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().replace('\\', "/"))
        .collect()
}

fn resolve_existing_changed_path_case(root: &Path, path: &Path) -> PathBuf {
    if !cfg!(windows) || !path.exists() {
        return path.to_path_buf();
    }
    let Ok(repo_relative_path) = repo_relative_path_for_changed_path(root, path) else {
        return path.to_path_buf();
    };
    let mut cursor = root.to_path_buf();
    let mut resolved = root.to_path_buf();
    for component in repo_relative_path
        .split('/')
        .filter(|part| !part.is_empty())
    {
        let actual_name = fs::read_dir(&cursor).ok().and_then(|entries| {
            entries.filter_map(Result::ok).find_map(|entry| {
                let name = entry.file_name();
                name.to_str()
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(component))
                    .then_some(name)
            })
        });
        match actual_name {
            Some(name) => {
                resolved.push(name);
                cursor = resolved.clone();
            }
            None => {
                resolved.push(component);
                cursor = resolved.clone();
            }
        }
    }
    resolved
}

fn platform_path_identity_key(path: &str) -> String {
    let normalized = normalize_graph_path(path);
    if cfg!(windows) {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}

fn entity_binary_signature(
    entity: &Entity,
    dimensions: usize,
) -> Result<BinarySignature, IndexError> {
    let text = format!(
        "{} {} {} {} {} {} {}",
        entity.kind,
        entity.name,
        entity.qualified_name,
        entity.repo_relative_path,
        entity.created_from,
        entity.content_hash.as_deref().unwrap_or(""),
        entity.file_hash.as_deref().unwrap_or("")
    );
    BinarySignature::from_text(&text, dimensions).map_err(|error| {
        IndexError::Message(format!("binary signature generation failed: {error}"))
    })
}

pub fn extract_graph_entity_embedding_chunks(
    entity: &Entity,
    source: Option<&str>,
    language: Option<&str>,
    lifecycle_binding: Option<RetrievalCandidateLifecycleBinding>,
) -> Vec<VectorEmbeddingChunk> {
    let source_role = classify_entity_source_role(entity)
        .role
        .as_str()
        .to_string();
    let chunk_kind = vector_chunk_kind_for_entity(entity.kind);
    let signature_text = bounded_vector_chunk_text(&format!(
        "{} {} {} file {}",
        entity.kind, entity.qualified_name, entity.name, entity.repo_relative_path
    ));
    let mut chunks = vec![build_vector_embedding_chunk(VectorChunkBuildInput {
        source_kind: VectorEmbeddingChunkSourceKind::GraphEntity,
        chunk_kind,
        path: &entity.repo_relative_path,
        entity_id: Some(&entity.id),
        source_span: entity.source_span.clone(),
        source_role: &source_role,
        evidence_role: &source_role,
        proof_status: "candidate_only",
        graph_proof: false,
        claimable_for_graph: false,
        text: signature_text,
        language: language.map(str::to_string),
        file_kind: None,
        lifecycle_binding: lifecycle_binding.clone(),
    })];

    if let (Some(source), Some(span)) = (source, entity.source_span.as_ref()) {
        if let Some(text) = source_span_text(source, span) {
            chunks.push(build_vector_embedding_chunk(VectorChunkBuildInput {
                source_kind: VectorEmbeddingChunkSourceKind::GraphEntity,
                chunk_kind: VectorEmbeddingChunkKind::SourceSnippet,
                path: &entity.repo_relative_path,
                entity_id: Some(&entity.id),
                source_span: Some(span.clone()),
                source_role: &source_role,
                evidence_role: &source_role,
                proof_status: "candidate_only",
                graph_proof: false,
                claimable_for_graph: false,
                text,
                language: language.map(str::to_string),
                file_kind: None,
                lifecycle_binding: lifecycle_binding.clone(),
            }));
        }
        if let Some((comment_span, comment)) = leading_doc_comment(source, span) {
            chunks.push(build_vector_embedding_chunk(VectorChunkBuildInput {
                source_kind: VectorEmbeddingChunkSourceKind::GraphEntity,
                chunk_kind: VectorEmbeddingChunkKind::DocComment,
                path: &entity.repo_relative_path,
                entity_id: Some(&entity.id),
                source_span: Some(comment_span),
                source_role: &source_role,
                evidence_role: &source_role,
                proof_status: "candidate_only",
                graph_proof: false,
                claimable_for_graph: false,
                text: comment,
                language: language.map(str::to_string),
                file_kind: None,
                lifecycle_binding,
            }));
        }
    }

    chunks
}

pub fn extract_text_evidence_embedding_chunks_for_path(
    repo_relative_path: &str,
    source: &str,
    lifecycle_binding: Option<RetrievalCandidateLifecycleBinding>,
) -> Vec<VectorEmbeddingChunk> {
    let Some(kind) = classify_scoped_text_evidence_path(repo_relative_path) else {
        return Vec::new();
    };
    if !looks_like_text_evidence_source(source) {
        return Vec::new();
    }

    let evidence = build_text_evidence_index(repo_relative_path, source);
    evidence
        .snippets
        .iter()
        .map(|snippet| {
            build_vector_embedding_chunk(VectorChunkBuildInput {
                source_kind: VectorEmbeddingChunkSourceKind::TextEvidence,
                chunk_kind: VectorEmbeddingChunkKind::Snippet,
                path: repo_relative_path,
                entity_id: None,
                source_span: Some(snippet.span.clone()),
                source_role: TEXT_EVIDENCE_KIND,
                evidence_role: TEXT_EVIDENCE_KIND,
                proof_status: TEXT_EVIDENCE_PROOF_STATUS,
                graph_proof: false,
                claimable_for_graph: false,
                text: bounded_vector_chunk_text(&snippet.text),
                language: None,
                file_kind: Some(kind.as_str().to_string()),
                lifecycle_binding: lifecycle_binding.clone(),
            })
        })
        .collect()
}

pub fn extract_file_path_title_embedding_chunk_for_path(
    repo_relative_path: &str,
    evidence_role: &str,
    file_kind: Option<&str>,
    lifecycle_binding: Option<RetrievalCandidateLifecycleBinding>,
) -> VectorEmbeddingChunk {
    let normalized_path = normalize_graph_path(repo_relative_path);
    let file_name = normalized_path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(normalized_path.as_str());
    let title = file_name.split('.').next().unwrap_or(file_name);
    let proof_status = if evidence_role == TEXT_EVIDENCE_KIND {
        TEXT_EVIDENCE_PROOF_STATUS
    } else {
        "candidate_only"
    };
    build_vector_embedding_chunk(VectorChunkBuildInput {
        source_kind: VectorEmbeddingChunkSourceKind::Metadata,
        chunk_kind: VectorEmbeddingChunkKind::FilePathTitle,
        path: &normalized_path,
        entity_id: None,
        source_span: None,
        source_role: "file_path_title",
        evidence_role,
        proof_status,
        graph_proof: false,
        claimable_for_graph: false,
        text: bounded_vector_chunk_text(&format!(
            "file path {normalized_path} title {title} kind {}",
            file_kind.unwrap_or("unknown")
        )),
        language: None,
        file_kind: file_kind.map(str::to_string),
        lifecycle_binding,
    })
}

pub fn build_in_memory_vector_chunk_index<P, I>(
    chunks: I,
    provider: &P,
    passport: &DbPassport,
    options: VectorChunkIndexBuildOptions,
) -> Result<InMemoryVectorChunkIndex, IndexError>
where
    P: EmbeddingProvider,
    I: IntoIterator<Item = VectorEmbeddingChunk>,
{
    build_in_memory_vector_chunk_index_with_timings(chunks, provider, passport, options)
        .map(|(index, _timings)| index)
}

fn build_in_memory_vector_chunk_index_with_timings<P, I>(
    chunks: I,
    provider: &P,
    passport: &DbPassport,
    options: VectorChunkIndexBuildOptions,
) -> Result<(InMemoryVectorChunkIndex, VectorChunkIndexBuildTimings), IndexError>
where
    P: EmbeddingProvider,
    I: IntoIterator<Item = VectorEmbeddingChunk>,
{
    let total_start = Instant::now();
    let collect_start = Instant::now();
    let generated_chunks = chunks.into_iter().collect::<Vec<_>>();
    let input_collect_ms = duration_ms(collect_start.elapsed());
    let count_start = Instant::now();
    let generated_counts = VectorChunkKindCounts::from_chunks(&generated_chunks);
    let generated_count_ms = duration_ms(count_start.elapsed());
    let selection_start = Instant::now();
    let selection = select_vector_chunks_for_persistence(generated_chunks, options.max_chunks);
    let selection_ms = duration_ms(selection_start.elapsed());
    let selected_count_start = Instant::now();
    let selected_counts = VectorChunkKindCounts::from_chunks(&selection.selected);
    let persisted_chunks_by_top_level_dir = count_chunks_by_top_level_dir(&selection.selected);
    let persisted_chunks_by_file_kind = count_chunks_by_file_kind(&selection.selected);
    let persisted_chunks_by_source_kind = count_chunks_by_source_kind(&selection.selected);
    let selected_count_ms = duration_ms(selected_count_start.elapsed());
    let mut entries = BTreeMap::new();
    let mut indexed_text_bytes = 0usize;

    let embedding_start = Instant::now();
    for chunk in selection.selected {
        let embedding = provider.embed(&chunk.text).map_err(|error| {
            IndexError::Message(format!(
                "vector chunk embedding failed for {}: {error}",
                chunk.chunk_id
            ))
        })?;
        indexed_text_bytes = indexed_text_bytes.saturating_add(chunk.byte_count);
        entries.insert(
            chunk.chunk_id.clone(),
            VectorChunkIndexEntry { chunk, embedding },
        );
    }
    let embedding_ms = duration_ms(embedding_start.elapsed());

    let metadata_start = Instant::now();
    let chunk_count = entries.len();
    let estimated_vector_bytes_per_chunk = provider
        .metadata()
        .dimension
        .saturating_mul(std::mem::size_of::<f32>());
    let estimated_f32_payload_bytes = chunk_count.saturating_mul(estimated_vector_bytes_per_chunk);
    let metadata = VectorChunkIndexMetadata {
        metadata_version: VECTOR_CHUNK_INDEX_METADATA_VERSION.to_string(),
        artifact_kind: String::new(),
        provider: VectorChunkIndexProviderSnapshot::from_provider(provider.metadata()),
        passport: VectorChunkIndexPassportSnapshot::from_passport(passport),
        source_scope: options.source_scope,
        extraction_version: options.extraction_version,
        created_at_unix_ms: unix_time_ms(),
        max_chunks: options.max_chunks,
        chunk_count,
        omitted_chunks: selection
            .omitted_by_cap
            .saturating_add(selection.omitted_low_signal),
        generated_total_chunks: generated_counts.total,
        generated_text_evidence_chunks: generated_counts.text_evidence,
        generated_graph_entity_chunks: generated_counts.graph_entity,
        generated_file_path_title_chunks: generated_counts.file_path_title,
        generated_metadata_chunks: generated_counts.metadata,
        selected_total_chunks: selected_counts.total,
        selected_text_evidence_chunks: selected_counts.text_evidence,
        selected_graph_entity_chunks: selected_counts.graph_entity,
        selected_file_path_title_chunks: selected_counts.file_path_title,
        selected_metadata_chunks: selected_counts.metadata,
        persisted_total_chunks: selected_counts.total,
        persisted_text_evidence_chunks: selected_counts.text_evidence,
        persisted_graph_entity_chunks: selected_counts.graph_entity,
        persisted_file_path_title_chunks: selected_counts.file_path_title,
        persisted_metadata_chunks: selected_counts.metadata,
        chunk_cap: options.max_chunks,
        chunk_cap_applied: selection.omitted_by_cap > 0 || selection.omitted_low_signal > 0,
        chunk_selection_strategy: "diversity_ranked_v1".to_string(),
        input_order_cap: false,
        persisted_chunks_by_top_level_dir,
        persisted_chunks_by_file_kind,
        persisted_chunks_by_source_kind,
        omitted_by_cap: selection.omitted_by_cap,
        omitted_by_bucket_limit: selection.omitted_by_bucket_limit,
        omitted_low_signal: selection.omitted_low_signal,
        per_file_cap: selection.per_file_cap,
        per_directory_soft_cap: selection.per_directory_soft_cap,
        indexed_text_bytes,
        estimated_vector_bytes_per_chunk,
        estimated_f32_payload_bytes,
        estimated_f32_payload_dim: provider.metadata().dimension,
        estimated_f32_payload_count: chunk_count,
        estimated_vector_bytes: estimated_f32_payload_bytes,
        estimated_vector_bytes_deprecated_alias_for: "estimated_f32_payload_bytes".to_string(),
        index_artifact_format: "pretty_json".to_string(),
        stores_chunk_text: true,
        stores_chunk_metadata: true,
        stores_full_source_body: false,
        vector_payload_compression: "none".to_string(),
    };
    let metadata_build_ms = duration_ms(metadata_start.elapsed());

    let mut timings = VectorChunkIndexBuildTimings {
        total_ms: duration_ms(total_start.elapsed()),
        input_collect_ms,
        generated_count_ms,
        selection_ms,
        selected_count_ms,
        embedding_ms,
        metadata_build_ms,
        ..VectorChunkIndexBuildTimings::default()
    };
    timings.in_memory_index_ms = timings.total_ms;

    Ok((InMemoryVectorChunkIndex { metadata, entries }, timings))
}

pub fn persisted_vector_chunk_index_from_index(
    index: &InMemoryVectorChunkIndex,
) -> PersistedVectorChunkIndex {
    PersistedVectorChunkIndex {
        metadata: index.metadata().clone(),
        chunks: index
            .entries()
            .map(|entry| entry.chunk.clone())
            .collect::<Vec<_>>(),
    }
}

fn vector_chunk_modified_unix_nanos(file: &FileRecord) -> Option<String> {
    file.metadata
        .get("modified_unix_nanos")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn bind_vector_chunk_to_source_file(
    mut chunk: VectorEmbeddingChunk,
    file: &FileRecord,
) -> VectorEmbeddingChunk {
    chunk.source_file_content_hash = Some(file.file_hash.clone());
    chunk.source_file_size_bytes = Some(file.size_bytes);
    chunk.source_file_modified_unix_nanos = vector_chunk_modified_unix_nanos(file);
    chunk
}

fn bind_vector_chunks_to_source_file<I>(chunks: I, file: &FileRecord) -> Vec<VectorEmbeddingChunk>
where
    I: IntoIterator<Item = VectorEmbeddingChunk>,
{
    chunks
        .into_iter()
        .map(|chunk| bind_vector_chunk_to_source_file(chunk, file))
        .collect()
}

pub fn validate_vector_chunk_source_bindings(
    repo_root: &Path,
    index: &InMemoryVectorChunkIndex,
) -> Result<VectorChunkSourceBindingValidation, IndexError> {
    let repo_root = fs::canonicalize(repo_root)?;
    let mut by_path = BTreeMap::<String, (Option<String>, Option<u64>, usize)>::new();
    let mut unbound_chunks = 0usize;

    for entry in index.entries() {
        if entry.chunk.source_file_content_hash.is_none()
            && entry.chunk.source_file_size_bytes.is_none()
        {
            unbound_chunks += 1;
            continue;
        }
        let binding = by_path.entry(entry.chunk.path.clone()).or_insert((
            entry.chunk.source_file_content_hash.clone(),
            entry.chunk.source_file_size_bytes,
            0,
        ));
        binding.2 += 1;
    }

    let checked_files = by_path.len();
    let mut stale_reasons = Vec::new();
    let mut checked_chunks = 0usize;
    for (repo_relative_path, (expected_hash, expected_size, chunk_count)) in by_path {
        checked_chunks += chunk_count;
        let source_path = repo_root.join(&repo_relative_path);
        let metadata = match fs::metadata(&source_path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                stale_reasons.push(format!("not_file:{repo_relative_path}"));
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                stale_reasons.push(format!("deleted_file:{repo_relative_path}"));
                continue;
            }
            Err(error) => {
                stale_reasons.push(format!("metadata_failed:{repo_relative_path}:{error}"));
                continue;
            }
        };
        if let Some(expected_size) = expected_size {
            if metadata.len() != expected_size {
                stale_reasons.push(format!(
                    "changed_file_size:{repo_relative_path}:expected={expected_size}:actual={}",
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
                        stale_reasons.push(format!("changed_file_hash:{repo_relative_path}"));
                    }
                }
                Err(error) => {
                    stale_reasons.push(format!("source_read_failed:{repo_relative_path}:{error}"))
                }
            }
        }
    }

    stale_reasons.sort();
    stale_reasons.dedup();
    Ok(VectorChunkSourceBindingValidation {
        status: if stale_reasons.is_empty() {
            "valid".to_string()
        } else {
            "stale".to_string()
        },
        checked_files,
        checked_chunks,
        unbound_chunks,
        stale_reasons,
    })
}

pub fn write_vector_chunk_index_json(
    path: &Path,
    index: &InMemoryVectorChunkIndex,
) -> Result<(), IndexError> {
    write_vector_chunk_runtime_sidecar_json(path, index, VectorChunkArtifactFormat::CompactJson)
        .map(|_| ())
}

pub fn write_vector_chunk_runtime_sidecar_json(
    path: &Path,
    index: &InMemoryVectorChunkIndex,
    format: VectorChunkArtifactFormat,
) -> Result<u64, IndexError> {
    write_vector_chunk_artifact_json(path, index, "vector_runtime_sidecar", format, false)
}

pub fn write_vector_chunk_audit_artifact_json(
    path: &Path,
    index: &InMemoryVectorChunkIndex,
) -> Result<u64, IndexError> {
    write_vector_chunk_artifact_json(
        path,
        index,
        "audit_artifact",
        VectorChunkArtifactFormat::PrettyJson,
        true,
    )
}

fn write_vector_chunk_artifact_json(
    path: &Path,
    index: &InMemoryVectorChunkIndex,
    artifact_kind: &str,
    format: VectorChunkArtifactFormat,
    include_verbose_metadata: bool,
) -> Result<u64, IndexError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let value =
        vector_chunk_artifact_value(index, artifact_kind, format, include_verbose_metadata)?;
    let bytes = if format.pretty() {
        serde_json::to_vec_pretty(&value)
    } else {
        serde_json::to_vec(&value)
    }
    .map_err(|error| {
        IndexError::Message(format!(
            "failed to serialize vector chunk artifact: {error}"
        ))
    })?;
    let byte_len = bytes.len() as u64;
    write_vector_chunk_artifact_bytes_atomically(path, &bytes, artifact_kind)?;
    Ok(byte_len)
}

fn write_vector_chunk_artifact_bytes_atomically(
    path: &Path,
    bytes: &[u8],
    artifact_kind: &str,
) -> Result<(), IndexError> {
    let temp_path = atomic_temp_artifact_path(path);
    if let Some(parent) = temp_path.parent() {
        fs::create_dir_all(parent)?;
    }
    {
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    serde_json::from_slice::<Value>(bytes).map_err(|error| {
        let _ = remove_file_if_exists(&temp_path);
        IndexError::Message(format!(
            "serialized vector chunk artifact did not validate as JSON before publish: {error}"
        ))
    })?;

    for failpoint in [
        vector_artifact_failpoint_name(artifact_kind, "after_temp_write_before_publish"),
        "vector_artifact_after_temp_write_before_publish",
    ] {
        if let Err(error) = write_path_chaos_failpoint(failpoint) {
            let _ = remove_file_if_exists(&temp_path);
            return Err(error);
        }
    }

    match publish_atomic_regular_file(
        &temp_path,
        path,
        vector_artifact_failpoint_name(artifact_kind, "during_publish"),
    ) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = remove_file_if_exists(&temp_path);
            Err(error)
        }
    }
}

fn vector_artifact_failpoint_name(artifact_kind: &str, suffix: &str) -> &'static str {
    match (artifact_kind, suffix) {
        ("audit_artifact", "after_temp_write_before_publish") => {
            "vector_audit_after_temp_write_before_publish"
        }
        ("audit_artifact", "during_publish") => "vector_audit_during_publish",
        (_, "after_temp_write_before_publish") => "vector_runtime_after_temp_write_before_publish",
        (_, "during_publish") => "vector_runtime_during_publish",
        _ => "vector_artifact_unknown_failpoint",
    }
}

fn atomic_temp_artifact_path(final_path: &Path) -> PathBuf {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = final_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("codegraph-vector-artifact.json");
    parent.join(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        unix_time_ms()
    ))
}

fn atomic_backup_artifact_path(final_path: &Path) -> PathBuf {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = final_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("codegraph-vector-artifact.json");
    parent.join(format!(
        ".{file_name}.backup-{}-{}",
        std::process::id(),
        unix_time_ms()
    ))
}

fn publish_atomic_regular_file(
    temp_path: &Path,
    final_path: &Path,
    during_publish_failpoint: &str,
) -> Result<(), IndexError> {
    let backup_path = atomic_backup_artifact_path(final_path);
    let had_old_file = final_path.exists();
    if had_old_file {
        if let Err(error) = fs::rename(final_path, &backup_path) {
            return Err(IndexError::Io(error));
        }
    }
    if let Err(error) = write_path_chaos_failpoint(during_publish_failpoint) {
        if had_old_file {
            let _ = fs::rename(&backup_path, final_path);
        }
        return Err(error);
    }
    match fs::rename(temp_path, final_path) {
        Ok(()) => {
            if had_old_file {
                remove_file_if_exists(&backup_path)?;
            }
            Ok(())
        }
        Err(error) => {
            let _ = remove_file_if_exists(final_path);
            if had_old_file {
                let _ = fs::rename(&backup_path, final_path);
            }
            Err(IndexError::Io(error))
        }
    }
}

fn vector_chunk_artifact_value(
    index: &InMemoryVectorChunkIndex,
    artifact_kind: &str,
    format: VectorChunkArtifactFormat,
    include_verbose_metadata: bool,
) -> Result<Value, IndexError> {
    let mut metadata = serde_json::to_value(index.metadata()).map_err(|error| {
        IndexError::Message(format!(
            "failed to serialize vector chunk metadata: {error}"
        ))
    })?;
    let chunk_count = index.len();
    if let Some(object) = metadata.as_object_mut() {
        object.insert("artifact_kind".to_string(), json!(artifact_kind));
        object.insert("index_artifact_format".to_string(), json!(format.as_str()));
        object.insert("vector_payload_compression".to_string(), json!("none"));
        object.insert("stores_chunk_text".to_string(), json!(true));
        object.insert("stores_chunk_metadata".to_string(), json!(true));
        object.insert("stores_full_source_body".to_string(), json!(false));
        object.insert("stores_embedding_vectors".to_string(), json!(false));
        object.insert(
            "embedding_reconstruction".to_string(),
            json!("stored bounded chunk.text is the deterministic embedding input"),
        );
        object.insert("candidate_only".to_string(), json!(true));
        object.insert("graph_proof".to_string(), json!(false));
        object.insert("claimable_for_graph".to_string(), json!(false));
        object.insert(
            "requires_graph_verification".to_string(),
            json!("entity_id vector hits require graph verification before proof"),
        );
        object.insert(
            "runtime_total_chunks".to_string(),
            json!(if artifact_kind == "vector_runtime_sidecar" {
                chunk_count
            } else {
                0
            }),
        );
        object.insert(
            "audit_total_chunks".to_string(),
            json!(if artifact_kind == "audit_artifact" {
                chunk_count
            } else {
                0
            }),
        );
        object.insert("runtime_selected_chunks".to_string(), json!(chunk_count));
        object.insert(
            "diagnostic_only".to_string(),
            json!(artifact_kind == "audit_artifact"),
        );
        object.insert(
            "verbose_selection_metadata".to_string(),
            json!(include_verbose_metadata),
        );
        object.insert("public_claim".to_string(), json!(false));
    }

    let chunks = index
        .entries()
        .map(|entry| vector_chunk_artifact_chunk_value(&entry.chunk, include_verbose_metadata))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "metadata": metadata,
        "chunks": chunks,
    }))
}

fn vector_chunk_artifact_chunk_value(
    chunk: &VectorEmbeddingChunk,
    include_verbose_metadata: bool,
) -> Result<Value, IndexError> {
    let mut chunk = chunk.clone();
    if !include_verbose_metadata {
        chunk.selection_reason = None;
        chunk.top_level_dir = None;
        chunk.cap_stage = None;
    }
    let requires_graph_verification = chunk.entity_id.is_some()
        || chunk.source_kind == VectorEmbeddingChunkSourceKind::GraphEntity;
    let mut value = serde_json::to_value(&chunk).map_err(|error| {
        IndexError::Message(format!("failed to serialize vector chunk: {error}"))
    })?;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "requires_graph_verification".to_string(),
            json!(requires_graph_verification),
        );
        object.insert(
            "candidate_only".to_string(),
            json!(!chunk.graph_proof && !chunk.claimable_for_graph),
        );
    }
    Ok(value)
}

pub fn read_vector_chunk_index_json(path: &Path) -> Result<PersistedVectorChunkIndex, IndexError> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        IndexError::Message(format!(
            "failed to parse vector chunk index {}: {error}",
            path.display()
        ))
    })
}

pub fn load_vector_chunk_index_json<P: EmbeddingProvider>(
    path: &Path,
    provider: &P,
    passport: &DbPassport,
    options: VectorChunkIndexBuildOptions,
) -> Result<InMemoryVectorChunkIndex, IndexError> {
    let persisted = read_vector_chunk_index_json(path)?;
    if persisted.metadata.metadata_version != VECTOR_CHUNK_INDEX_METADATA_VERSION {
        return Err(IndexError::Message(format!(
            "metadata_version changed: expected {}, got {}",
            VECTOR_CHUNK_INDEX_METADATA_VERSION, persisted.metadata.metadata_version
        )));
    }
    match persisted.metadata.artifact_kind.as_str() {
        "" | "vector_runtime_sidecar" | "legacy_pretty_json_vector_artifact" => {}
        "audit_artifact" => {
            return Err(IndexError::Message(
                "vector audit artifact is diagnostic_only and cannot be loaded as the runtime vector sidecar"
                    .to_string(),
            ))
        }
        other => {
            return Err(IndexError::Message(format!(
                "unsupported vector artifact kind for runtime load: {other}"
            )))
        }
    }
    if let Some(reason) =
        persisted
            .metadata
            .incompatibility_reason(provider.metadata(), passport, &options)
    {
        return Err(IndexError::Message(reason));
    }
    if persisted.metadata.chunk_count != persisted.chunks.len() {
        return Err(IndexError::Message(format!(
            "chunk_count mismatch: metadata says {}, file has {} chunks",
            persisted.metadata.chunk_count,
            persisted.chunks.len()
        )));
    }
    let persisted_metadata = persisted.metadata.clone();
    let mut index =
        build_in_memory_vector_chunk_index(persisted.chunks, provider, passport, options)?;
    index.metadata = persisted_metadata;
    Ok(index)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VectorChunkIndexBuildSummary {
    pub status: String,
    pub index_path: String,
    pub runtime_sidecar_path: String,
    pub build_timings: VectorChunkIndexBuildTimings,
    pub chunk_count: usize,
    pub omitted_chunks: usize,
    pub generated_total_chunks: usize,
    pub generated_text_evidence_chunks: usize,
    pub generated_graph_entity_chunks: usize,
    pub generated_file_path_title_chunks: usize,
    pub generated_metadata_chunks: usize,
    pub selected_total_chunks: usize,
    pub selected_text_evidence_chunks: usize,
    pub selected_graph_entity_chunks: usize,
    pub selected_file_path_title_chunks: usize,
    pub selected_metadata_chunks: usize,
    pub persisted_total_chunks: usize,
    pub persisted_text_evidence_chunks: usize,
    pub persisted_graph_entity_chunks: usize,
    pub persisted_file_path_title_chunks: usize,
    pub persisted_metadata_chunks: usize,
    pub chunk_cap: usize,
    pub chunk_cap_applied: bool,
    pub chunk_selection_strategy: String,
    pub input_order_cap: bool,
    pub persisted_chunks_by_top_level_dir: BTreeMap<String, usize>,
    pub persisted_chunks_by_file_kind: BTreeMap<String, usize>,
    pub persisted_chunks_by_source_kind: BTreeMap<String, usize>,
    pub omitted_by_cap: usize,
    pub omitted_by_bucket_limit: usize,
    pub omitted_low_signal: usize,
    pub per_file_cap: usize,
    pub per_directory_soft_cap: usize,
    pub indexed_text_bytes: usize,
    pub actual_index_file_bytes: u64,
    pub runtime_sidecar_bytes: u64,
    pub audit_artifact_path: Option<String>,
    pub audit_artifact_bytes: Option<u64>,
    pub pretty_json_overhead: Option<i64>,
    pub index_artifact_format: String,
    pub estimated_f32_payload_bytes: usize,
    pub estimated_f32_payload_dim: usize,
    pub estimated_f32_payload_count: usize,
    pub index_file_to_f32_payload_ratio: f64,
    pub stores_chunk_text: bool,
    pub stores_chunk_metadata: bool,
    pub stores_full_source_body: bool,
    pub vector_payload_compression: String,
    pub estimated_vector_bytes: usize,
    pub estimated_vector_bytes_deprecated_alias_for: String,
    pub graph_entity_chunks: usize,
    pub text_evidence_chunks: usize,
    pub file_path_title_chunks: usize,
    pub runtime_total_chunks: usize,
    pub audit_total_chunks: usize,
    pub runtime_selected_chunks: usize,
    pub audit_chunks: usize,
    pub provider_id: String,
    pub model_id: String,
    pub dimension: usize,
    pub source_scope: String,
    pub extraction_version: String,
    pub graph_proof: bool,
    pub proof_status: String,
}

pub fn build_vector_chunk_index_json_for_repo<P: EmbeddingProvider>(
    repo_root: &Path,
    db_path: &Path,
    index_path: &Path,
    provider: &P,
    options: VectorChunkIndexBuildOptions,
) -> Result<VectorChunkIndexBuildSummary, IndexError> {
    build_vector_chunk_index_artifacts_for_repo(
        repo_root,
        db_path,
        index_path,
        provider,
        options,
        VectorChunkIndexArtifactOptions::default(),
    )
}

pub fn build_vector_chunk_index_artifacts_for_repo<P: EmbeddingProvider>(
    repo_root: &Path,
    db_path: &Path,
    index_path: &Path,
    provider: &P,
    options: VectorChunkIndexBuildOptions,
    artifact_options: VectorChunkIndexArtifactOptions,
) -> Result<VectorChunkIndexBuildSummary, IndexError> {
    let total_start = Instant::now();
    let mut timings = VectorChunkIndexBuildTimings::default();
    let repo_resolution_start = Instant::now();
    let repo_root = resolve_repo_root_for_index(repo_root)?;
    timings.repo_resolution_ms = duration_ms(repo_resolution_start.elapsed());
    let db_path = normalize_db_path(&repo_root, db_path);
    let open_store_start = Instant::now();
    let store = SqliteGraphStore::open_read_only(&db_path)?;
    timings.open_store_ms = duration_ms(open_store_start.elapsed());
    let load_passport_start = Instant::now();
    let passport = store
        .get_db_passport()?
        .ok_or_else(|| IndexError::Message("db passport missing".to_string()))?;
    timings.load_passport_ms = duration_ms(load_passport_start.elapsed());
    let lifecycle_binding = RetrievalCandidateLifecycleBinding {
        status: RetrievalCandidateLifecycleStatus::Fresh,
        db_passport_fingerprint: Some(db_passport_fingerprint(&passport)),
        repo_head: passport.repo_head.clone(),
        scope_policy_hash: Some(passport.index_scope_policy_hash.clone()),
        embedding_model_id: Some(provider.metadata().model_id.clone()),
        embedding_profile: Some(provider.metadata().version.clone()),
        stale_reason: None,
    };
    let list_files_start = Instant::now();
    let files = store.list_files(UNBOUNDED_STORE_READ_LIMIT)?;
    timings.list_files_ms = duration_ms(list_files_start.elapsed());
    let mut entities_by_file = BTreeMap::<String, Vec<Entity>>::new();
    let list_entities_start = Instant::now();
    let entities = store.list_entities(UNBOUNDED_STORE_READ_LIMIT)?;
    timings.list_entities_ms = duration_ms(list_entities_start.elapsed());
    let filter_group_start = Instant::now();
    for entity in entities
        .into_iter()
        .filter(should_generate_vector_chunks_for_entity)
    {
        entities_by_file
            .entry(normalize_graph_path(&entity.repo_relative_path))
            .or_default()
            .push(entity);
    }
    timings.filter_group_entities_ms = duration_ms(filter_group_start.elapsed());
    let mut chunks = Vec::new();
    let chunk_generation_start = Instant::now();

    for file in files {
        let repo_relative_path = normalize_graph_path(&file.repo_relative_path);
        let source_path = repo_root.join(&repo_relative_path);
        let source_read_start = Instant::now();
        let source = fs::read_to_string(&source_path).ok();
        timings.file_source_read_ms += duration_ms(source_read_start.elapsed());
        let is_text_evidence = file.metadata.get("evidence_kind").and_then(Value::as_str)
            == Some(TEXT_EVIDENCE_KIND)
            && file.metadata.get("proof_status").and_then(Value::as_str)
                == Some(TEXT_EVIDENCE_PROOF_STATUS);
        let file_kind = file
            .metadata
            .get("source_file_kind")
            .and_then(Value::as_str)
            .or(file.language.as_deref());
        let evidence_role = if is_text_evidence {
            TEXT_EVIDENCE_KIND
        } else {
            "production"
        };

        let file_path_chunk_start = Instant::now();
        chunks.push(bind_vector_chunk_to_source_file(
            extract_file_path_title_embedding_chunk_for_path(
                &repo_relative_path,
                evidence_role,
                file_kind,
                Some(lifecycle_binding.clone()),
            ),
            &file,
        ));
        timings.file_path_title_chunk_ms += duration_ms(file_path_chunk_start.elapsed());

        if is_text_evidence {
            if let Some(source) = source.as_deref() {
                let text_evidence_start = Instant::now();
                chunks.extend(bind_vector_chunks_to_source_file(
                    extract_text_evidence_embedding_chunks_for_path(
                        &repo_relative_path,
                        source,
                        Some(lifecycle_binding.clone()),
                    ),
                    &file,
                ));
                timings.text_evidence_chunk_ms += duration_ms(text_evidence_start.elapsed());
            }
            continue;
        }

        for entity in entities_by_file
            .remove(&repo_relative_path)
            .unwrap_or_default()
        {
            let graph_entity_start = Instant::now();
            chunks.extend(bind_vector_chunks_to_source_file(
                extract_graph_entity_embedding_chunks(
                    &entity,
                    source.as_deref(),
                    file.language.as_deref(),
                    Some(lifecycle_binding.clone()),
                ),
                &file,
            ));
            timings.graph_entity_chunk_ms += duration_ms(graph_entity_start.elapsed());
        }
    }
    timings.chunk_generation_ms = duration_ms(chunk_generation_start.elapsed());

    let graph_entity_chunks = chunks
        .iter()
        .filter(|chunk| chunk.source_kind == VectorEmbeddingChunkSourceKind::GraphEntity)
        .count();
    let text_evidence_chunks = chunks
        .iter()
        .filter(|chunk| chunk.source_kind == VectorEmbeddingChunkSourceKind::TextEvidence)
        .count();
    let file_path_title_chunks = chunks
        .iter()
        .filter(|chunk| chunk.chunk_kind == VectorEmbeddingChunkKind::FilePathTitle)
        .count();
    let in_memory_start = Instant::now();
    let (index, in_memory_timings) = build_in_memory_vector_chunk_index_with_timings(
        chunks,
        provider,
        &passport,
        options.clone(),
    )?;
    timings.in_memory_index_ms = duration_ms(in_memory_start.elapsed());
    timings.input_collect_ms = in_memory_timings.input_collect_ms;
    timings.generated_count_ms = in_memory_timings.generated_count_ms;
    timings.selection_ms = in_memory_timings.selection_ms;
    timings.selected_count_ms = in_memory_timings.selected_count_ms;
    timings.embedding_ms = in_memory_timings.embedding_ms;
    timings.metadata_build_ms = in_memory_timings.metadata_build_ms;
    let write_json_start = Instant::now();
    let runtime_sidecar_bytes = write_vector_chunk_runtime_sidecar_json(
        index_path,
        &index,
        artifact_options.runtime_format,
    )?;
    let audit_artifact_bytes =
        if let Some(audit_path) = artifact_options.audit_artifact_path.as_ref() {
            Some(write_vector_chunk_audit_artifact_json(audit_path, &index)?)
        } else {
            None
        };
    timings.write_json_ms = duration_ms(write_json_start.elapsed());
    let artifact_metadata_start = Instant::now();
    let actual_index_file_bytes = fs::metadata(index_path)
        .map(|metadata| metadata.len())
        .unwrap_or(runtime_sidecar_bytes);
    timings.artifact_metadata_ms = duration_ms(artifact_metadata_start.elapsed());
    timings.total_ms = duration_ms(total_start.elapsed());
    let estimated_f32_payload_bytes = index.metadata().estimated_f32_payload_bytes;
    let index_file_to_f32_payload_ratio = if estimated_f32_payload_bytes == 0 {
        0.0
    } else {
        actual_index_file_bytes as f64 / estimated_f32_payload_bytes as f64
    };

    Ok(VectorChunkIndexBuildSummary {
        status: "ok".to_string(),
        index_path: index_path.to_string_lossy().to_string(),
        runtime_sidecar_path: index_path.to_string_lossy().to_string(),
        build_timings: timings,
        chunk_count: index.metadata().chunk_count,
        omitted_chunks: index.metadata().omitted_chunks,
        generated_total_chunks: index.metadata().generated_total_chunks,
        generated_text_evidence_chunks: index.metadata().generated_text_evidence_chunks,
        generated_graph_entity_chunks: index.metadata().generated_graph_entity_chunks,
        generated_file_path_title_chunks: index.metadata().generated_file_path_title_chunks,
        generated_metadata_chunks: index.metadata().generated_metadata_chunks,
        selected_total_chunks: index.metadata().selected_total_chunks,
        selected_text_evidence_chunks: index.metadata().selected_text_evidence_chunks,
        selected_graph_entity_chunks: index.metadata().selected_graph_entity_chunks,
        selected_file_path_title_chunks: index.metadata().selected_file_path_title_chunks,
        selected_metadata_chunks: index.metadata().selected_metadata_chunks,
        persisted_total_chunks: index.metadata().persisted_total_chunks,
        persisted_text_evidence_chunks: index.metadata().persisted_text_evidence_chunks,
        persisted_graph_entity_chunks: index.metadata().persisted_graph_entity_chunks,
        persisted_file_path_title_chunks: index.metadata().persisted_file_path_title_chunks,
        persisted_metadata_chunks: index.metadata().persisted_metadata_chunks,
        chunk_cap: index.metadata().chunk_cap,
        chunk_cap_applied: index.metadata().chunk_cap_applied,
        chunk_selection_strategy: index.metadata().chunk_selection_strategy.clone(),
        input_order_cap: index.metadata().input_order_cap,
        persisted_chunks_by_top_level_dir: index
            .metadata()
            .persisted_chunks_by_top_level_dir
            .clone(),
        persisted_chunks_by_file_kind: index.metadata().persisted_chunks_by_file_kind.clone(),
        persisted_chunks_by_source_kind: index.metadata().persisted_chunks_by_source_kind.clone(),
        omitted_by_cap: index.metadata().omitted_by_cap,
        omitted_by_bucket_limit: index.metadata().omitted_by_bucket_limit,
        omitted_low_signal: index.metadata().omitted_low_signal,
        per_file_cap: index.metadata().per_file_cap,
        per_directory_soft_cap: index.metadata().per_directory_soft_cap,
        indexed_text_bytes: index.metadata().indexed_text_bytes,
        actual_index_file_bytes,
        runtime_sidecar_bytes,
        audit_artifact_path: artifact_options
            .audit_artifact_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        audit_artifact_bytes,
        pretty_json_overhead: audit_artifact_bytes
            .map(|bytes| bytes as i64 - runtime_sidecar_bytes as i64),
        index_artifact_format: artifact_options.runtime_format.as_str().to_string(),
        estimated_f32_payload_bytes,
        estimated_f32_payload_dim: index.metadata().estimated_f32_payload_dim,
        estimated_f32_payload_count: index.metadata().estimated_f32_payload_count,
        index_file_to_f32_payload_ratio,
        stores_chunk_text: index.metadata().stores_chunk_text,
        stores_chunk_metadata: index.metadata().stores_chunk_metadata,
        stores_full_source_body: index.metadata().stores_full_source_body,
        vector_payload_compression: index.metadata().vector_payload_compression.clone(),
        estimated_vector_bytes: index.metadata().estimated_vector_bytes,
        estimated_vector_bytes_deprecated_alias_for: index
            .metadata()
            .estimated_vector_bytes_deprecated_alias_for
            .clone(),
        graph_entity_chunks,
        text_evidence_chunks,
        file_path_title_chunks,
        runtime_total_chunks: index.metadata().selected_total_chunks,
        audit_total_chunks: audit_artifact_bytes
            .map(|_| index.metadata().selected_total_chunks)
            .unwrap_or(0),
        runtime_selected_chunks: index.metadata().selected_total_chunks,
        audit_chunks: audit_artifact_bytes
            .map(|_| index.metadata().selected_total_chunks)
            .unwrap_or(0),
        provider_id: index.metadata().provider.provider_id.clone(),
        model_id: index.metadata().provider.model_id.clone(),
        dimension: index.metadata().provider.dimension,
        source_scope: options.source_scope,
        extraction_version: options.extraction_version,
        graph_proof: false,
        proof_status: "candidate_only".to_string(),
    })
}

pub fn vector_chunk_search_hit_to_retrieval_candidate(
    hit: &VectorChunkSearchHit,
    provider: &EmbeddingProviderMetadata,
    matched_query_text: &str,
    lifecycle_binding: Option<RetrievalCandidateLifecycleBinding>,
) -> RetrievalCandidate {
    let chunk = &hit.chunk;
    let mut candidate = RetrievalCandidate::new(
        format!("vector://{}", chunk.chunk_id),
        RetrievalCandidateSource::VectorSemantic,
        "vector semantic chunk candidate; not graph proof",
    );
    candidate.embedding_source = Some(vector_embedding_source_for_chunk(chunk));
    candidate.file_id = Some(chunk.file_id.clone());
    candidate.path = Some(chunk.path.clone());
    candidate.entity_id = chunk.entity_id.clone();
    candidate.span = chunk.source_span.clone();
    if candidate.span.is_none() {
        candidate.source_span_missing_reason =
            Some("vector chunk did not carry a source span".to_string());
    }
    candidate.matched_query_text = Some(matched_query_text.to_string());
    candidate.evidence_role = evidence_role_for_vector_chunk(chunk);
    candidate.proof_status = if chunk.source_kind == VectorEmbeddingChunkSourceKind::TextEvidence {
        RetrievalProofStatus::NotGraphProof
    } else {
        RetrievalProofStatus::CandidateOnly
    };
    candidate.graph_proof = false;
    candidate.claimable = chunk.source_kind == VectorEmbeddingChunkSourceKind::TextEvidence;
    candidate.claimable_for_text =
        Some(chunk.source_kind == VectorEmbeddingChunkSourceKind::TextEvidence);
    candidate.claimable_for_graph = Some(false);
    candidate.score = Some(f64::from(hit.score));
    candidate.rank = Some(hit.rank);
    candidate.embedding_model_id = Some(provider.model_id.clone());
    candidate.embedding_dim = Some(provider.dimension);
    candidate.embedding_profile = Some(provider.version.clone());
    candidate.chunk_id = Some(chunk.chunk_id.clone());
    candidate.chunk_kind = Some(chunk.chunk_kind.as_str().to_string());
    candidate.requires_graph_verification = chunk.entity_id.is_some();
    candidate.verification_status = if chunk.entity_id.is_some() {
        RetrievalVerificationStatus::NeedsGraphVerification
    } else {
        RetrievalVerificationStatus::NotGraphProof
    };
    candidate.lifecycle_binding = lifecycle_binding.or_else(|| {
        Some(RetrievalCandidateLifecycleBinding {
            status: RetrievalCandidateLifecycleStatus::Fresh,
            db_passport_fingerprint: chunk
                .lifecycle_binding
                .as_ref()
                .and_then(|binding| binding.db_passport_fingerprint.clone()),
            repo_head: chunk
                .lifecycle_binding
                .as_ref()
                .and_then(|binding| binding.repo_head.clone()),
            scope_policy_hash: chunk
                .lifecycle_binding
                .as_ref()
                .and_then(|binding| binding.scope_policy_hash.clone()),
            embedding_model_id: Some(provider.model_id.clone()),
            embedding_profile: Some(provider.version.clone()),
            stale_reason: None,
        })
    });
    candidate.metadata.insert(
        "provider_id".to_string(),
        json!(provider.provider_id.clone()),
    );
    candidate
        .metadata
        .insert("chunk_text".to_string(), json!(chunk.text));
    candidate.metadata.insert(
        "evidence_role_raw".to_string(),
        json!(chunk.evidence_role.clone()),
    );
    candidate.metadata.insert(
        "proof_status_raw".to_string(),
        json!(chunk.proof_status.clone()),
    );
    candidate
}

fn vector_embedding_source_for_chunk(chunk: &VectorEmbeddingChunk) -> VectorEmbeddingSource {
    match (chunk.source_kind, chunk.chunk_kind) {
        (VectorEmbeddingChunkSourceKind::TextEvidence, _) => VectorEmbeddingSource::TextEvidence,
        (_, VectorEmbeddingChunkKind::FilePathTitle) => VectorEmbeddingSource::FilePathTitle,
        (_, VectorEmbeddingChunkKind::DocComment) => VectorEmbeddingSource::DocComment,
        (_, VectorEmbeddingChunkKind::SourceSnippet | VectorEmbeddingChunkKind::Snippet) => {
            VectorEmbeddingSource::Snippet
        }
        (_, VectorEmbeddingChunkKind::Signature) => VectorEmbeddingSource::Signature,
        (VectorEmbeddingChunkSourceKind::GraphEntity, _) => VectorEmbeddingSource::GraphEntity,
        _ => VectorEmbeddingSource::Unknown,
    }
}

fn evidence_role_for_vector_chunk(chunk: &VectorEmbeddingChunk) -> EvidenceRole {
    match chunk.evidence_role.as_str() {
        "production" => EvidenceRole::Production,
        "test" => EvidenceRole::Test,
        "mock" => EvidenceRole::Mock,
        "mixed" => EvidenceRole::Mixed,
        _ => EvidenceRole::Unknown,
    }
}

fn db_passport_fingerprint(passport: &DbPassport) -> String {
    let payload = serde_json::to_string(passport).unwrap_or_else(|_| {
        format!(
            "{}:{}:{}:{}:{}",
            passport.passport_version,
            passport.codegraph_schema_version,
            passport.storage_mode,
            passport.index_scope_policy_hash,
            passport.canonical_repo_root
        )
    });
    content_hash(&payload)
}

fn vector_embedding_dot_product(
    left: &TestEmbeddingVector,
    right: &TestEmbeddingVector,
) -> Result<f32, IndexError> {
    if left.dimensions() != right.dimensions() {
        return Err(IndexError::Message(format!(
            "vector chunk embedding dimension mismatch: expected {}, got {}",
            left.dimensions(),
            right.dimensions()
        )));
    }
    Ok(left
        .values()
        .iter()
        .zip(right.values())
        .map(|(left, right)| left * right)
        .sum())
}

struct VectorChunkBuildInput<'a> {
    source_kind: VectorEmbeddingChunkSourceKind,
    chunk_kind: VectorEmbeddingChunkKind,
    path: &'a str,
    entity_id: Option<&'a str>,
    source_span: Option<SourceSpan>,
    source_role: &'a str,
    evidence_role: &'a str,
    proof_status: &'a str,
    graph_proof: bool,
    claimable_for_graph: bool,
    text: String,
    language: Option<String>,
    file_kind: Option<String>,
    lifecycle_binding: Option<RetrievalCandidateLifecycleBinding>,
}

fn build_vector_embedding_chunk(input: VectorChunkBuildInput<'_>) -> VectorEmbeddingChunk {
    let normalized_path = normalize_graph_path(input.path);
    let content_hash = content_hash(&input.text);
    let chunk_id = stable_vector_chunk_id(
        input.source_kind,
        input.chunk_kind,
        &normalized_path,
        input.entity_id,
        input.source_span.as_ref(),
        &content_hash,
    );
    let token_count = vector_chunk_token_count(&input.text);
    let byte_count = input.text.len();
    let top_level_dir = vector_chunk_top_level_dir(&normalized_path);
    VectorEmbeddingChunk {
        chunk_id,
        chunk_kind: input.chunk_kind,
        source_kind: input.source_kind,
        file_id: normalized_path.clone(),
        path: normalized_path,
        entity_id: input.entity_id.map(str::to_string),
        source_span: input.source_span,
        source_role: input.source_role.to_string(),
        evidence_role: input.evidence_role.to_string(),
        proof_status: input.proof_status.to_string(),
        graph_proof: input.graph_proof,
        claimable_for_graph: input.claimable_for_graph,
        text: input.text,
        token_count,
        byte_count,
        language: input.language,
        file_kind: input.file_kind,
        lifecycle_binding: input.lifecycle_binding,
        content_hash,
        source_file_content_hash: None,
        source_file_size_bytes: None,
        source_file_modified_unix_nanos: None,
        extraction_version: VECTOR_EMBEDDING_CHUNK_EXTRACTION_VERSION.to_string(),
        selection_score: None,
        selection_bucket: None,
        selection_reason: None,
        top_level_dir: Some(top_level_dir),
        cap_stage: None,
    }
}

fn candidate_spool_scope_hash(options: &IndexOptions) -> Result<String, IndexError> {
    scope_policy_hash(&options.scope)
}

fn candidate_spool_repo_hash(repo_root: &Path) -> Result<String, IndexError> {
    Ok(stable_hex_hash(
        canonical_repo_root_string(repo_root)?.as_bytes(),
    ))
}

fn candidate_spool_query_index_binding_hash(spool: &CandidateSpoolSummary) -> String {
    let material = json!({
        "metadata_version": CANDIDATE_SPOOL_METADATA_VERSION,
        "record_model": spool.record_model,
        "repo_hash": spool.repo_hash,
        "scope_hash": spool.scope_hash,
        "db_passport_hash": spool.db_passport_hash,
        "candidate_spool_status": spool.candidate_spool_status,
        "lifecycle": spool.lifecycle,
        "generated_total_chunks": spool.generated_total_chunks,
        "selected_total_chunks": spool.selected_total_chunks,
        "persisted_total_chunks": spool.persisted_total_chunks,
        "spooled_total_chunks": spool.spooled_total_chunks,
        "candidate_spool_truncated": spool.candidate_spool_truncated,
        "candidate_spool_partial": spool.candidate_spool_partial,
    });
    stable_hex_hash(material.to_string().as_bytes())
}

fn candidate_spool_query_index_create_schema(connection: &Connection) -> Result<(), IndexError> {
    connection
        .execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            CREATE TABLE IF NOT EXISTS candidate_spool_metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS candidate_spool_records (
                chunk_id TEXT PRIMARY KEY,
                candidate_kind TEXT NOT NULL,
                packet_kind TEXT,
                chunk_kind TEXT,
                source_kind TEXT,
                path TEXT,
                normalized_path TEXT,
                filename TEXT,
                top_level_dir TEXT,
                file_kind TEXT,
                source_role TEXT,
                evidence_role TEXT,
                symbol_name TEXT,
                normalized_symbol TEXT,
                source_span_json TEXT,
                entity_id TEXT,
                text_preview TEXT,
                selection_score REAL,
                selection_bucket TEXT,
                source_file_content_hash TEXT,
                source_file_size_bytes INTEGER,
                chunk_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS candidate_spool_terms (
                term TEXT NOT NULL,
                chunk_id TEXT NOT NULL,
                field TEXT NOT NULL,
                weight INTEGER NOT NULL,
                PRIMARY KEY (term, chunk_id, field)
            );
            CREATE TABLE IF NOT EXISTS candidate_spool_source_bindings (
                path TEXT PRIMARY KEY,
                source_file_content_hash TEXT,
                source_file_size_bytes INTEGER,
                record_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_candidate_spool_path
                ON candidate_spool_records(normalized_path);
            CREATE INDEX IF NOT EXISTS idx_candidate_spool_filename
                ON candidate_spool_records(filename);
            CREATE INDEX IF NOT EXISTS idx_candidate_spool_symbol
                ON candidate_spool_records(normalized_symbol);
            CREATE INDEX IF NOT EXISTS idx_candidate_spool_kind
                ON candidate_spool_records(candidate_kind);
            CREATE INDEX IF NOT EXISTS idx_candidate_spool_source_kind
                ON candidate_spool_records(source_kind);
            CREATE INDEX IF NOT EXISTS idx_candidate_spool_dir
                ON candidate_spool_records(top_level_dir);
            CREATE INDEX IF NOT EXISTS idx_candidate_spool_file_kind
                ON candidate_spool_records(file_kind);
            CREATE INDEX IF NOT EXISTS idx_candidate_spool_role
                ON candidate_spool_records(source_role, evidence_role);
            CREATE INDEX IF NOT EXISTS idx_candidate_spool_terms_term
                ON candidate_spool_terms(term);
            CREATE INDEX IF NOT EXISTS idx_candidate_spool_terms_chunk
                ON candidate_spool_terms(chunk_id);
            CREATE INDEX IF NOT EXISTS idx_candidate_spool_source_bindings_hash
                ON candidate_spool_source_bindings(source_file_content_hash);
            ",
        )
        .map_err(|error| sqlite_index_error("candidate_spool_query_index_schema_failed", error))?;
    Ok(())
}

fn candidate_spool_query_index_set_metadata(
    connection: &Connection,
    key: &str,
    value: impl ToString,
) -> Result<(), IndexError> {
    connection
        .execute(
            "INSERT OR REPLACE INTO candidate_spool_metadata(key, value) VALUES (?1, ?2)",
            params![key, value.to_string()],
        )
        .map_err(|error| {
            sqlite_index_error("candidate_spool_query_index_metadata_write_failed", error)
        })?;
    Ok(())
}

fn candidate_spool_query_index_metadata_value(
    connection: &Connection,
    key: &str,
) -> Result<Option<String>, IndexError> {
    let mut statement = connection
        .prepare("SELECT value FROM candidate_spool_metadata WHERE key = ?1")
        .map_err(|error| {
            sqlite_index_error("candidate_spool_query_index_metadata_read_failed", error)
        })?;
    let mut rows = statement.query(params![key]).map_err(|error| {
        sqlite_index_error("candidate_spool_query_index_metadata_read_failed", error)
    })?;
    match rows.next().map_err(|error| {
        sqlite_index_error("candidate_spool_query_index_metadata_read_failed", error)
    })? {
        Some(row) => row.get::<_, String>(0).map(Some).map_err(|error| {
            sqlite_index_error("candidate_spool_query_index_metadata_read_failed", error)
        }),
        None => Ok(None),
    }
}

fn candidate_spool_query_index_record_count(connection: &Connection) -> Result<usize, IndexError> {
    let count = connection
        .query_row("SELECT COUNT(*) FROM candidate_spool_records", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|error| sqlite_index_error("candidate_spool_query_index_count_failed", error))?;
    Ok(count.max(0) as usize)
}

fn candidate_spool_query_index_source_binding_count(
    connection: &Connection,
) -> Result<usize, IndexError> {
    let count = connection
        .query_row(
            "SELECT COUNT(*) FROM candidate_spool_source_bindings",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            sqlite_index_error(
                "candidate_spool_query_index_source_binding_count_failed",
                error,
            )
        })?;
    Ok(count.max(0) as usize)
}

fn candidate_spool_query_index_source_binding_stale_reasons(
    connection: &Connection,
    repo_root: &Path,
) -> Result<Vec<String>, IndexError> {
    let mut statement = connection
        .prepare(
            "SELECT path, source_file_content_hash, source_file_size_bytes
             FROM candidate_spool_source_bindings
             ORDER BY path",
        )
        .map_err(|error| {
            sqlite_index_error(
                "candidate_spool_query_index_source_bindings_prepare_failed",
                error,
            )
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<u64>>(2)?,
            ))
        })
        .map_err(|error| {
            sqlite_index_error(
                "candidate_spool_query_index_source_bindings_read_failed",
                error,
            )
        })?;
    let mut stale_reasons = Vec::new();
    for row in rows {
        let (path, expected_hash, expected_size) = row.map_err(|error| {
            sqlite_index_error(
                "candidate_spool_query_index_source_binding_row_failed",
                error,
            )
        })?;
        let source_path = repo_root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !source_path.exists() {
            stale_reasons.push(format!("deleted_file: {path}"));
            continue;
        }
        if let Some(expected_size) = expected_size {
            let actual_size = fs::metadata(&source_path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if actual_size != expected_size {
                stale_reasons.push(format!(
                    "changed_file_size: {path} expected={expected_size} actual={actual_size}"
                ));
                continue;
            }
        }
        if let Some(expected_hash) = expected_hash {
            match fs::read_to_string(&source_path) {
                Ok(source) => {
                    let actual_hash = content_hash(&source);
                    if actual_hash != expected_hash {
                        stale_reasons.push(format!("changed_file_hash: {path}"));
                    }
                }
                Err(error) => stale_reasons.push(format!("read_failed: {path}: {error}")),
            }
        }
    }
    stale_reasons.sort();
    stale_reasons.dedup();
    Ok(stale_reasons)
}

fn refresh_candidate_spool_query_index_summary(
    spool: &mut CandidateSpoolSummary,
) -> Result<(), IndexError> {
    if spool.query_index_status == "disabled" {
        return Ok(());
    }
    let spool_path = PathBuf::from(&spool.candidate_spool_path);
    let index_path = candidate_spool_query_index_path(&spool_path);
    spool.query_index_path = Some(path_string(&index_path));
    spool.query_index_kind = "sqlite".to_string();
    spool.query_index_version = CANDIDATE_SPOOL_QUERY_INDEX_VERSION.to_string();
    spool.query_index_bound_manifest_hash = Some(candidate_spool_query_index_binding_hash(spool));
    if !index_path.exists() {
        spool.query_index_status = "index_missing".to_string();
        spool.query_index_bytes = 0;
        spool.query_index_record_count = 0;
        return Ok(());
    }
    let connection = Connection::open(&index_path)
        .map_err(|error| sqlite_index_error("candidate_spool_query_index_open_failed", error))?;
    candidate_spool_query_index_create_schema(&connection)?;
    let count = candidate_spool_query_index_record_count(&connection)?;
    let source_binding_count = candidate_spool_query_index_source_binding_count(&connection)?;
    spool.query_index_status = "ready".to_string();
    spool.query_index_bytes = fs::metadata(&index_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    spool.query_index_record_count = count;
    candidate_spool_query_index_set_metadata(
        &connection,
        "query_index_version",
        CANDIDATE_SPOOL_QUERY_INDEX_VERSION,
    )?;
    candidate_spool_query_index_set_metadata(&connection, "query_index_status", "ready")?;
    candidate_spool_query_index_set_metadata(&connection, "query_index_kind", "sqlite")?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "query_index_record_count",
        count.to_string(),
    )?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "query_index_source_binding_count",
        source_binding_count.to_string(),
    )?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "query_index_bound_manifest_hash",
        spool
            .query_index_bound_manifest_hash
            .as_deref()
            .unwrap_or_default(),
    )?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "candidate_spool_status",
        &spool.candidate_spool_status,
    )?;
    candidate_spool_query_index_set_metadata(&connection, "lifecycle", &spool.lifecycle)?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "record_model",
        if spool.record_model.is_empty() {
            CANDIDATE_SPOOL_RECORD_MODEL_VERSION
        } else {
            &spool.record_model
        },
    )?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "spooled_total_chunks",
        spool.spooled_total_chunks.to_string(),
    )?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "persisted_total_chunks",
        spool.persisted_total_chunks.to_string(),
    )?;
    if let Some(repo_hash) = spool.repo_hash.as_deref() {
        candidate_spool_query_index_set_metadata(&connection, "repo_hash", repo_hash)?;
    }
    if let Some(scope_hash) = spool.scope_hash.as_deref() {
        candidate_spool_query_index_set_metadata(&connection, "scope_hash", scope_hash)?;
    }
    if let Some(passport_hash) = spool.db_passport_hash.as_deref() {
        candidate_spool_query_index_set_metadata(&connection, "db_passport_hash", passport_hash)?;
    }
    Ok(())
}

fn initialize_candidate_spool_query_index(
    summary: &mut IndexSummary,
    repo_root: &Path,
    db_path: &Path,
) -> Result<(), IndexError> {
    let Some(spool) = summary.candidate_spool.as_mut() else {
        return Ok(());
    };
    let spool_path = PathBuf::from(&spool.candidate_spool_path);
    let index_path = candidate_spool_query_index_path(&spool_path);
    if let Some(parent) = index_path.parent() {
        fs::create_dir_all(parent)?;
    }
    remove_file_if_exists(&index_path)?;
    let connection = Connection::open(&index_path)
        .map_err(|error| sqlite_index_error("candidate_spool_query_index_open_failed", error))?;
    candidate_spool_query_index_create_schema(&connection)?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "metadata_version",
        CANDIDATE_SPOOL_METADATA_VERSION,
    )?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "query_index_version",
        CANDIDATE_SPOOL_QUERY_INDEX_VERSION,
    )?;
    candidate_spool_query_index_set_metadata(&connection, "query_index_status", "ready")?;
    candidate_spool_query_index_set_metadata(&connection, "query_index_kind", "sqlite")?;
    candidate_spool_query_index_set_metadata(&connection, "artifact_kind", "candidate_spool")?;
    candidate_spool_query_index_set_metadata(&connection, "spool_path", path_string(&spool_path))?;
    candidate_spool_query_index_set_metadata(&connection, "repo_root", path_string(repo_root))?;
    candidate_spool_query_index_set_metadata(&connection, "db_path", path_string(db_path))?;
    drop(connection);
    refresh_candidate_spool_query_index_summary(spool)?;
    Ok(())
}

fn initialize_candidate_spool(
    summary: &mut IndexSummary,
    repo_root: &Path,
    db_path: &Path,
    options: &IndexOptions,
) -> Result<(), IndexError> {
    if options.candidate_spool_policy == CandidateSpoolPolicy::Off {
        summary.candidate_spool = None;
        return Ok(());
    }
    let Some(path) = options.candidate_spool_path.as_ref() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    remove_file_if_exists(path)?;
    let scope_hash = Some(candidate_spool_scope_hash(options)?);
    let repo_hash = Some(candidate_spool_repo_hash(repo_root)?);
    summary.candidate_spool = Some(CandidateSpoolSummary {
        status: "building".to_string(),
        candidate_spool_status: "building".to_string(),
        candidate_spool_path: path_string(path),
        artifact_kind: "candidate_spool".to_string(),
        artifact_format: "jsonl".to_string(),
        lifecycle: "indexing_in_progress".to_string(),
        spooled_total_chunks: 0,
        spooled_by_source_kind: BTreeMap::new(),
        spooled_by_chunk_kind: BTreeMap::new(),
        spooled_bytes: 0,
        spooled_indexing_phase: "pre_graph_build".to_string(),
        candidate_spool_policy: options.candidate_spool_policy.as_str().to_string(),
        candidate_spool_required: options.candidate_spool_required,
        candidate_spool_disabled_reason: None,
        candidate_spool_warning: None,
        artifact_budget_remaining_bytes: None,
        artifact_budget_decision: None,
        record_model: CANDIDATE_SPOOL_RECORD_MODEL_VERSION.to_string(),
        generated_total_chunks: 0,
        normalized_total_chunks: 0,
        deduped_total_chunks: 0,
        selected_total_chunks: 0,
        persisted_total_chunks: 0,
        omitted_by_cap: 0,
        omitted_by_budget: 0,
        omitted_by_dedup: 0,
        omitted_low_signal: 0,
        omitted_by_file_limit: 0,
        omitted_by_dir_limit: 0,
        omitted_by_kind_limit: 0,
        candidate_spool_truncated: false,
        candidate_spool_partial: false,
        candidate_spool_budget_bytes: options.candidate_spool_caps.global_max_bytes as u64,
        query_index_status: "not_started".to_string(),
        query_index_kind: "none".to_string(),
        query_index_path: None,
        query_index_bytes: 0,
        query_index_record_count: 0,
        query_index_version: String::new(),
        query_index_bound_manifest_hash: None,
        candidate_only: true,
        graph_proof: false,
        incomplete: true,
        db_passport_hash: None,
        db_passport_snapshot: None,
        scope_hash,
        repo_hash,
        reason: Some("spool initialized before graph DB proof publish".to_string()),
        caps: options.candidate_spool_caps.clone(),
        selected_candidate_ids: BTreeSet::new(),
        selected_file_counts: BTreeMap::new(),
        selected_dir_counts: BTreeMap::new(),
        selected_kind_counts: BTreeMap::new(),
        selected_source_counts: BTreeMap::new(),
    });
    if options.candidate_spool_query_index {
        initialize_candidate_spool_query_index(summary, repo_root, db_path)?;
    } else if let Some(spool) = summary.candidate_spool.as_mut() {
        spool.query_index_status = "disabled".to_string();
        spool.query_index_kind = "none".to_string();
        spool.query_index_version = String::new();
        spool.query_index_path = Some(path_string(&candidate_spool_query_index_path(path)));
        spool.reason = Some(
            "candidate spool query index disabled by --candidate-spool-query-index no".to_string(),
        );
    }
    rewrite_candidate_spool_jsonl(summary, repo_root, db_path, options)?;
    Ok(())
}

fn candidate_spool_manifest(
    summary: &IndexSummary,
    repo_root: &Path,
    db_path: &Path,
    options: &IndexOptions,
) -> Result<Value, IndexError> {
    let Some(spool) = summary.candidate_spool.as_ref() else {
        return Err(IndexError::Message(
            "candidate spool manifest requested before initialization".to_string(),
        ));
    };
    let passport_snapshot = spool.db_passport_snapshot.as_ref().map(|passport| {
        let mut value = serde_json::to_value(passport).unwrap_or_else(|_| json!({}));
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "passport_fingerprint".to_string(),
                json!(db_passport_fingerprint(passport)),
            );
        }
        value
    });
    let spool_count_value = if spool.incomplete && spool.spooled_total_chunks == 0 {
        Value::Null
    } else {
        json!(spool.spooled_total_chunks)
    };
    Ok(json!({
        "metadata_version": CANDIDATE_SPOOL_METADATA_VERSION,
        "artifact_kind": "candidate_spool",
        "artifact_format": "jsonl",
        "record_model": spool.record_model.clone(),
        "source_scope": "fast_candidate_spool",
        "extraction_version": VECTOR_EMBEDDING_CHUNK_EXTRACTION_VERSION,
        "created_by": "codegraph-mcp index --candidate-spool",
        "repo_root": path_string(repo_root),
        "repo_hash": spool.repo_hash.clone(),
        "db_path": path_string(db_path),
        "db_passport_hash": spool.db_passport_hash.clone(),
        "passport": passport_snapshot,
        "scope_hash": spool.scope_hash.clone(),
        "lifecycle": spool.lifecycle.clone(),
        "status": spool.status.clone(),
        "candidate_spool_status": spool.candidate_spool_status.clone(),
        "candidate_spool_policy": spool.candidate_spool_policy.clone(),
        "candidate_spool_required": spool.candidate_spool_required,
        "candidate_spool_disabled_reason": spool.candidate_spool_disabled_reason.clone(),
        "candidate_spool_warning": spool.candidate_spool_warning.clone(),
        "candidate_spool_truncated": spool.candidate_spool_truncated,
        "candidate_spool_partial": spool.candidate_spool_partial,
        "candidate_spool_budget_bytes": spool.candidate_spool_budget_bytes,
        "candidate_spool_written_bytes": spool.spooled_bytes,
        "artifact_budget_remaining_bytes": spool.artifact_budget_remaining_bytes,
        "artifact_budget_decision": spool.artifact_budget_decision.clone(),
        "candidate_spool_record_count": spool.persisted_total_chunks,
        "candidate_spool_selected_count": spool.selected_total_chunks,
        "candidate_spool_generated_count": spool.generated_total_chunks,
        "incomplete": spool.incomplete,
        "candidate_only": true,
        "graph_proof": false,
        "claimable_for_graph": false,
        "creates_graph_relations": false,
        "can_answer_graph_proof": false,
        "requires_graph_verification": true,
        "spooled_total_chunks": spool_count_value.clone(),
        "generated_total_chunks": spool.generated_total_chunks,
        "normalized_total_chunks": spool.normalized_total_chunks,
        "deduped_total_chunks": spool.deduped_total_chunks,
        "selected_total_chunks": spool.selected_total_chunks,
        "persisted_total_chunks": spool.persisted_total_chunks,
        "spooled_by_source_kind": spool.spooled_by_source_kind.clone(),
        "spooled_by_chunk_kind": spool.spooled_by_chunk_kind.clone(),
        "spooled_indexing_phase": spool.spooled_indexing_phase.clone(),
        "spooled_bytes": spool.spooled_bytes,
        "index_artifact_format": "jsonl",
        "query_index_status": if spool.query_index_status.is_empty() { "not_started" } else { spool.query_index_status.as_str() },
        "query_index_kind": if spool.query_index_kind.is_empty() { "none" } else { spool.query_index_kind.as_str() },
        "query_index_path": spool.query_index_path.clone(),
        "query_index_bytes": spool.query_index_bytes,
        "query_index_record_count": spool.query_index_record_count,
        "query_index_version": if spool.query_index_version.is_empty() { Value::Null } else { json!(spool.query_index_version.clone()) },
        "query_index_bound_manifest_hash": spool.query_index_bound_manifest_hash.clone(),
        "vector_payload_compression": "none",
        "stores_embedding_vectors": false,
        "stores_chunk_text": true,
        "stores_chunk_metadata": true,
        "stores_full_source_body": false,
        "chunk_selection_strategy": "candidate_spool_bounded_packet_selector_v1",
        "input_order_cap": false,
        "omitted_by_cap": spool.omitted_by_cap,
        "omitted_by_budget": spool.omitted_by_budget,
        "omitted_low_signal": spool.omitted_low_signal,
        "omitted_by_bucket_limit": spool.omitted_by_dir_limit + spool.omitted_by_file_limit + spool.omitted_by_kind_limit,
        "omitted_by_dedup": spool.omitted_by_dedup,
        "omitted_by_file_limit": spool.omitted_by_file_limit,
        "omitted_by_dir_limit": spool.omitted_by_dir_limit,
        "omitted_by_kind_limit": spool.omitted_by_kind_limit,
        "caps": {
            "global_max_bytes": options.candidate_spool_caps.global_max_bytes,
            "global_max_records": options.candidate_spool_caps.global_max_records,
            "per_file_max_records": options.candidate_spool_caps.per_file_max_records,
            "per_top_level_dir_soft_cap": options.candidate_spool_caps.per_top_level_dir_soft_cap,
            "max_snippet_bytes": options.candidate_spool_caps.max_snippet_bytes,
            "max_snippets_per_file_packet": options.candidate_spool_caps.max_snippets_per_file_packet,
            "max_symbols_per_file_packet": options.candidate_spool_caps.max_symbols_per_file_packet,
            "per_candidate_kind_cap": options.candidate_spool_caps.per_candidate_kind_cap.clone(),
            "per_source_kind_cap": options.candidate_spool_caps.per_source_kind_cap.clone()
        },
        "provider": {
            "provider_id": null,
            "model_id": null,
            "dimension": 0,
            "note": "candidate spool stores text candidates only; no embeddings are stored"
        },
        "lifecycle_binding": {
            "status": "unknown",
            "db_passport_fingerprint": spool.db_passport_hash.clone(),
            "scope_policy_hash": spool.scope_hash.clone(),
            "stale_reason": spool.reason.clone()
        },
        "contract": {
            "file_path_title": "candidate_only",
            "symbol_signature": "candidate_only",
            "text_evidence": "source_text_existence_only",
            "imports_exports": "source_navigation_only",
            "graph_relations": "never_created_by_spool"
        },
        "index_options": {
            "storage_mode": options.storage_mode.as_str(),
            "build_mode": options.build_mode.as_str(),
            "candidate_spool_query_index": options.candidate_spool_query_index
        }
    }))
}

fn rewrite_candidate_spool_jsonl(
    summary: &mut IndexSummary,
    repo_root: &Path,
    db_path: &Path,
    options: &IndexOptions,
) -> Result<(), IndexError> {
    let Some(spool) = summary.candidate_spool.as_ref() else {
        return Ok(());
    };
    let path = PathBuf::from(&spool.candidate_spool_path);
    let mut chunk_lines = Vec::new();
    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        for (index, line) in existing.lines().enumerate() {
            if index == 0 {
                continue;
            }
            if !line.trim().is_empty() {
                chunk_lines.push(line.to_string());
            }
        }
    }
    let manifest = candidate_spool_manifest(summary, repo_root, db_path, options)?;
    let tmp_path = PathBuf::from(format!("{}.tmp", path.display()));
    if let Some(parent) = tmp_path.parent() {
        fs::create_dir_all(parent)?;
    }
    {
        let mut file = fs::File::create(&tmp_path)?;
        serde_json::to_writer(&mut file, &json!({ "metadata": manifest }))
            .map_err(|error| IndexError::Message(error.to_string()))?;
        file.write_all(b"\n")?;
        for line in chunk_lines {
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")?;
        }
    }
    remove_file_if_exists(&path)?;
    fs::rename(tmp_path, &path)?;
    if let Some(spool) = summary.candidate_spool.as_mut() {
        spool.spooled_bytes = fs::metadata(&path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct CandidateSpoolInput {
    stable_id: String,
    candidate_kind: String,
    source_kind: String,
    path: String,
    entity_id: Option<String>,
    source_span: Value,
    source_span_key: String,
    symbol_name: String,
    source_role: String,
    evidence_role: String,
    file_kind: String,
    top_level_dir: String,
    text: String,
    source_file_content_hash: Option<String>,
    source_file_size_bytes: Option<u64>,
    source_file_modified_unix_nanos: Option<String>,
    score: f64,
}

#[derive(Debug, Clone)]
struct CandidateSpoolPacket {
    packet_id: String,
    candidate_kind: String,
    source_kind: String,
    path: String,
    top_level_dir: String,
    source_span_key: String,
    symbol_name: String,
    score: f64,
    selection_bucket: String,
    input_count: usize,
    value: Value,
    serialized_bytes: usize,
}

#[derive(Debug, Clone, Default)]
struct CandidateSpoolSelectionResult {
    generated_count: usize,
    normalized_count: usize,
    deduped_count: usize,
    selected_count: usize,
    persisted_count: usize,
    omitted_by_cap: usize,
    omitted_by_budget: usize,
    omitted_by_dedup: usize,
    omitted_low_signal: usize,
    omitted_by_file_limit: usize,
    omitted_by_dir_limit: usize,
    omitted_by_kind_limit: usize,
    selected: Vec<CandidateSpoolPacket>,
}

#[derive(Debug, Clone, Default)]
struct CandidateSpoolFileGroup {
    path_title: Vec<CandidateSpoolInput>,
    snippets: Vec<CandidateSpoolInput>,
    symbols: Vec<CandidateSpoolInput>,
    navigation: Vec<CandidateSpoolInput>,
    other: Vec<CandidateSpoolInput>,
}

fn select_candidate_spool_packets(
    chunks: Vec<Value>,
    caps: &CandidateSpoolCaps,
    spool: &CandidateSpoolSummary,
) -> CandidateSpoolSelectionResult {
    let mut result = CandidateSpoolSelectionResult {
        generated_count: chunks.len(),
        ..CandidateSpoolSelectionResult::default()
    };
    if chunks.is_empty()
        || caps.global_max_records == 0
        || caps.global_max_bytes == 0
        || spool.persisted_total_chunks >= caps.global_max_records
    {
        result.omitted_by_cap = chunks.len();
        return result;
    }

    let mut normalized = Vec::new();
    for chunk in chunks {
        match normalize_candidate_spool_input(&chunk, caps) {
            Some(input) => normalized.push(input),
            None => result.omitted_low_signal += 1,
        }
    }
    result.normalized_count = normalized.len();

    let mut deduped = BTreeMap::new();
    for input in normalized {
        deduped.entry(input.stable_id.clone()).or_insert(input);
    }
    result.deduped_count = deduped.len();
    result.omitted_by_dedup = result.normalized_count.saturating_sub(result.deduped_count);

    let mut groups: BTreeMap<String, CandidateSpoolFileGroup> = BTreeMap::new();
    for input in deduped.into_values() {
        let group = groups.entry(input.path.clone()).or_default();
        match input.candidate_kind.as_str() {
            "file_path_title" => group.path_title.push(input),
            "text_evidence_snippet" => group.snippets.push(input),
            "symbol_signature" | "doc_comment_or_title" | "test_or_mock_candidate" => {
                group.symbols.push(input)
            }
            "import_export_source_navigation"
            | "parser_local_reference"
            | "relation_neighborhood_candidate" => group.navigation.push(input),
            _ => group.other.push(input),
        }
    }

    let mut packets = Vec::new();
    for (path, mut group) in groups {
        sort_candidate_spool_inputs(&mut group.path_title);
        sort_candidate_spool_inputs(&mut group.snippets);
        sort_candidate_spool_inputs(&mut group.symbols);
        sort_candidate_spool_inputs(&mut group.navigation);
        sort_candidate_spool_inputs(&mut group.other);

        if let Some(packet) = candidate_spool_file_packet(&path, &group, caps) {
            packets.push(packet);
        }
        if let Some(packet) = candidate_spool_text_packet(&path, &group, caps) {
            packets.push(packet);
        }
        if let Some(packet) = candidate_spool_navigation_packet(&path, &group, caps) {
            packets.push(packet);
        }
        for input in group
            .symbols
            .iter()
            .take(caps.max_symbols_per_file_packet.max(1))
        {
            packets.push(candidate_spool_symbol_packet(input, caps));
        }
    }

    packets.sort_by(candidate_spool_packet_cmp);
    let mut selected_ids = spool.selected_candidate_ids.clone();
    let mut file_counts = spool.selected_file_counts.clone();
    let mut dir_counts = spool.selected_dir_counts.clone();
    let mut kind_counts = spool.selected_kind_counts.clone();
    let mut source_counts = spool.selected_source_counts.clone();
    let mut used_bytes = spool.spooled_bytes as usize;
    let mut omitted_packet_ids = BTreeSet::new();

    for kind in candidate_spool_kind_order() {
        if result.selected.len() + spool.persisted_total_chunks >= caps.global_max_records {
            break;
        }
        let Some(packet) = packets
            .iter()
            .find(|packet| {
                packet.candidate_kind == kind && !selected_ids.contains(&packet.packet_id)
            })
            .cloned()
        else {
            continue;
        };
        try_select_candidate_spool_packet(
            packet,
            caps,
            &mut result,
            &mut selected_ids,
            &mut file_counts,
            &mut dir_counts,
            &mut kind_counts,
            &mut source_counts,
            &mut used_bytes,
            &mut omitted_packet_ids,
        );
    }

    for packet in packets {
        if result.selected.len() + spool.persisted_total_chunks >= caps.global_max_records {
            if !selected_ids.contains(&packet.packet_id)
                && omitted_packet_ids.insert(packet.packet_id)
            {
                result.omitted_by_cap += packet.input_count.max(1);
            }
            continue;
        }
        try_select_candidate_spool_packet(
            packet,
            caps,
            &mut result,
            &mut selected_ids,
            &mut file_counts,
            &mut dir_counts,
            &mut kind_counts,
            &mut source_counts,
            &mut used_bytes,
            &mut omitted_packet_ids,
        );
    }

    result.selected.sort_by(|left, right| {
        candidate_spool_packet_output_key(left).cmp(&candidate_spool_packet_output_key(right))
    });
    result.selected_count = result.selected.len();
    result.persisted_count = result.selected.len();
    result
}

#[allow(clippy::too_many_arguments)]
fn try_select_candidate_spool_packet(
    packet: CandidateSpoolPacket,
    caps: &CandidateSpoolCaps,
    result: &mut CandidateSpoolSelectionResult,
    selected_ids: &mut BTreeSet<String>,
    file_counts: &mut BTreeMap<String, usize>,
    dir_counts: &mut BTreeMap<String, usize>,
    kind_counts: &mut BTreeMap<String, usize>,
    source_counts: &mut BTreeMap<String, usize>,
    used_bytes: &mut usize,
    omitted_packet_ids: &mut BTreeSet<String>,
) {
    if selected_ids.contains(&packet.packet_id) || omitted_packet_ids.contains(&packet.packet_id) {
        return;
    }
    let input_count = packet.input_count.max(1);
    let kind_cap = caps
        .per_candidate_kind_cap
        .get(&packet.candidate_kind)
        .copied()
        .unwrap_or(caps.global_max_records);
    let source_cap = caps
        .per_source_kind_cap
        .get(&packet.source_kind)
        .copied()
        .unwrap_or(caps.global_max_records);
    if kind_counts
        .get(&packet.candidate_kind)
        .copied()
        .unwrap_or(0)
        >= kind_cap
    {
        omitted_packet_ids.insert(packet.packet_id);
        result.omitted_by_kind_limit += input_count;
        return;
    }
    if source_counts.get(&packet.source_kind).copied().unwrap_or(0) >= source_cap {
        omitted_packet_ids.insert(packet.packet_id);
        result.omitted_by_kind_limit += input_count;
        return;
    }
    if file_counts.get(&packet.path).copied().unwrap_or(0) >= caps.per_file_max_records {
        omitted_packet_ids.insert(packet.packet_id);
        result.omitted_by_file_limit += input_count;
        return;
    }
    if dir_counts.get(&packet.top_level_dir).copied().unwrap_or(0)
        >= caps.per_top_level_dir_soft_cap
    {
        omitted_packet_ids.insert(packet.packet_id);
        result.omitted_by_dir_limit += input_count;
        return;
    }
    if used_bytes.saturating_add(packet.serialized_bytes) > caps.global_max_bytes {
        omitted_packet_ids.insert(packet.packet_id);
        result.omitted_by_budget += input_count;
        return;
    }
    selected_ids.insert(packet.packet_id.clone());
    *file_counts.entry(packet.path.clone()).or_default() += 1;
    *dir_counts.entry(packet.top_level_dir.clone()).or_default() += 1;
    *kind_counts
        .entry(packet.candidate_kind.clone())
        .or_default() += 1;
    *source_counts.entry(packet.source_kind.clone()).or_default() += 1;
    *used_bytes += packet.serialized_bytes;
    result.selected.push(packet);
}

fn normalize_candidate_spool_input(
    chunk: &Value,
    caps: &CandidateSpoolCaps,
) -> Option<CandidateSpoolInput> {
    let path = chunk
        .get("path")
        .and_then(Value::as_str)?
        .replace('\\', "/");
    let source_kind = chunk
        .get("source_kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let chunk_kind = chunk
        .get("chunk_kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let selection_bucket = chunk
        .get("selection_bucket")
        .and_then(Value::as_str)
        .unwrap_or("");
    let source_role = chunk
        .get("source_role")
        .and_then(Value::as_str)
        .unwrap_or("source_navigation")
        .to_string();
    let evidence_role = chunk
        .get("evidence_role")
        .and_then(Value::as_str)
        .unwrap_or("source_navigation")
        .to_string();
    let candidate_kind =
        candidate_spool_candidate_kind(&chunk_kind, &source_kind, selection_bucket, &source_role);
    let raw_text = chunk
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or(&path)
        .trim();
    let text = bounded_text_prefix(raw_text, caps.max_snippet_bytes.max(32)).to_string();
    if candidate_kind != "file_path_title" && candidate_spool_low_signal_text(&text) {
        return None;
    }
    let source_span = chunk.get("source_span").cloned().unwrap_or(Value::Null);
    let source_span_key = candidate_spool_source_span_key(&source_span);
    let entity_id = chunk
        .get("entity_id")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let file_kind = chunk
        .get("file_kind")
        .and_then(Value::as_str)
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| candidate_spool_file_kind_from_path(&path));
    let top_level_dir = vector_chunk_top_level_dir(&path);
    let symbol_name = candidate_spool_symbol_name(chunk, &text);
    let stable_material = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}",
        candidate_kind,
        source_kind,
        path,
        entity_id.as_deref().unwrap_or("-"),
        source_span_key,
        symbol_name,
        stable_hex_hash(text.as_bytes())
    );
    let score = candidate_spool_input_score(
        &candidate_kind,
        &source_kind,
        &chunk_kind,
        &path,
        &file_kind,
        &source_span,
        &source_role,
        &text,
    );
    Some(CandidateSpoolInput {
        stable_id: format!(
            "candidate-spool:input:{}",
            stable_hex_hash(stable_material.as_bytes())
        ),
        candidate_kind,
        source_kind,
        path,
        entity_id,
        source_span,
        source_span_key,
        symbol_name,
        source_role,
        evidence_role,
        file_kind,
        top_level_dir,
        text,
        source_file_content_hash: chunk
            .get("source_file_content_hash")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        source_file_size_bytes: chunk.get("source_file_size_bytes").and_then(Value::as_u64),
        source_file_modified_unix_nanos: chunk
            .get("source_file_modified_unix_nanos")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        score,
    })
}

fn candidate_spool_file_packet(
    path: &str,
    group: &CandidateSpoolFileGroup,
    caps: &CandidateSpoolCaps,
) -> Option<CandidateSpoolPacket> {
    let representative = group
        .path_title
        .first()
        .or_else(|| group.snippets.first())
        .or_else(|| group.symbols.first())
        .or_else(|| group.navigation.first())
        .or_else(|| group.other.first())?;
    let snippets = group
        .snippets
        .iter()
        .take(caps.max_snippets_per_file_packet)
        .collect::<Vec<_>>();
    let symbols = group
        .symbols
        .iter()
        .take(caps.max_symbols_per_file_packet)
        .collect::<Vec<_>>();
    let mut text_parts = vec![format!("file {path}")];
    if !symbols.is_empty() {
        text_parts.push(format!(
            "symbols {}",
            symbols
                .iter()
                .map(|input| input.symbol_name.as_str())
                .filter(|name| !name.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    if !snippets.is_empty() {
        text_parts.push(format!(
            "snippets {}",
            snippets
                .iter()
                .map(|input| input.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    let text = bounded_text_prefix(
        &text_parts.join("\n"),
        caps.max_snippet_bytes.saturating_mul(4),
    )
    .to_string();
    candidate_spool_packet_from_parts(
        "file_candidate_packet",
        "file_path_title",
        "file_path_title",
        "metadata",
        representative,
        text,
        json!(snippets
            .iter()
            .map(|input| candidate_spool_snippet_json(input))
            .collect::<Vec<_>>()),
        json!(symbols
            .iter()
            .map(|input| candidate_spool_symbol_json(input))
            .collect::<Vec<_>>()),
        Value::Null,
    )
}

fn candidate_spool_text_packet(
    _path: &str,
    group: &CandidateSpoolFileGroup,
    caps: &CandidateSpoolCaps,
) -> Option<CandidateSpoolPacket> {
    let representative = group.snippets.first()?;
    let snippets = group
        .snippets
        .iter()
        .take(caps.max_snippets_per_file_packet)
        .collect::<Vec<_>>();
    let text = bounded_text_prefix(
        &snippets
            .iter()
            .map(|input| input.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        caps.max_snippet_bytes
            .saturating_mul(caps.max_snippets_per_file_packet.max(1)),
    )
    .to_string();
    candidate_spool_packet_from_parts(
        "text_evidence_candidate_packet",
        "text_evidence_snippet",
        "snippet",
        "text_evidence",
        representative,
        text,
        json!(snippets
            .iter()
            .map(|input| candidate_spool_snippet_json(input))
            .collect::<Vec<_>>()),
        Value::Null,
        Value::Null,
    )
}

fn candidate_spool_navigation_packet(
    _path: &str,
    group: &CandidateSpoolFileGroup,
    caps: &CandidateSpoolCaps,
) -> Option<CandidateSpoolPacket> {
    let representative = group.navigation.first()?;
    let navigation = group
        .navigation
        .iter()
        .take(caps.max_symbols_per_file_packet)
        .collect::<Vec<_>>();
    let text = bounded_text_prefix(
        &navigation
            .iter()
            .map(|input| input.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        caps.max_snippet_bytes.saturating_mul(2),
    )
    .to_string();
    candidate_spool_packet_from_parts(
        "source_navigation_candidate_packet",
        "import_export_source_navigation",
        "relation_neighborhood",
        "metadata",
        representative,
        text,
        Value::Null,
        Value::Null,
        json!(navigation
            .iter()
            .map(|input| candidate_spool_navigation_json(input))
            .collect::<Vec<_>>()),
    )
}

fn candidate_spool_symbol_packet(
    input: &CandidateSpoolInput,
    caps: &CandidateSpoolCaps,
) -> CandidateSpoolPacket {
    candidate_spool_packet_from_parts(
        "symbol_candidate_packet",
        "symbol_signature",
        "signature",
        "graph_entity",
        input,
        bounded_text_prefix(&input.text, caps.max_snippet_bytes).to_string(),
        Value::Null,
        json!([candidate_spool_symbol_json(input)]),
        Value::Null,
    )
    .expect("symbol packet from input")
}

#[allow(clippy::too_many_arguments)]
fn candidate_spool_packet_from_parts(
    packet_kind: &str,
    candidate_kind: &str,
    chunk_kind: &str,
    source_kind: &str,
    representative: &CandidateSpoolInput,
    text: String,
    snippets: Value,
    symbols: Value,
    source_navigation: Value,
) -> Option<CandidateSpoolPacket> {
    let packet_material = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}",
        packet_kind,
        candidate_kind,
        representative.path,
        representative.source_span_key,
        representative.symbol_name,
        representative
            .source_file_content_hash
            .as_deref()
            .unwrap_or("-"),
        stable_hex_hash(text.as_bytes())
    );
    let packet_id = format!(
        "candidate-spool:packet:v1:{}:{}",
        packet_kind,
        stable_hex_hash(packet_material.as_bytes())
    );
    let selection_bucket = candidate_kind.to_string();
    let source_span = representative.source_span.clone();
    let entity_id = representative.entity_id.clone();
    let text_bytes = text.len();
    let value = json!({
        "chunk_id": packet_id.clone(),
        "packet_kind": packet_kind,
        "candidate_kind": candidate_kind,
        "chunk_kind": chunk_kind,
        "source_kind": source_kind,
        "path": representative.path.clone(),
        "entity_id": entity_id.clone(),
        "source_span": source_span,
        "source_role": representative.source_role.clone(),
        "evidence_role": representative.evidence_role.clone(),
        "proof_status": "candidate_only",
        "graph_proof": false,
        "claimable_for_graph": false,
        "candidate_only": true,
        "requires_graph_verification": true,
        "creates_graph_relations": false,
        "can_answer_graph_proof": false,
        "text": text,
        "text_bytes": text_bytes,
        "text_preview_truncated": text_bytes >= CANDIDATE_SPOOL_DEFAULT_MAX_SNIPPET_BYTES,
        "file_kind": representative.file_kind.clone(),
        "top_level_dir": representative.top_level_dir.clone(),
        "selection_score": representative.score,
        "selection_bucket": selection_bucket.clone(),
        "lifecycle": "partial_spool",
        "candidate_spool_lifecycle": "partial_spool",
        "incomplete": true,
        "source_file_content_hash": representative.source_file_content_hash.clone(),
        "source_file_size_bytes": representative.source_file_size_bytes,
        "source_file_modified_unix_nanos": representative.source_file_modified_unix_nanos.clone(),
        "snippets": snippets,
        "symbols": symbols,
        "source_navigation": source_navigation
    });
    let serialized_bytes = serde_json::to_vec(&value).ok()?.len() + 1;
    Some(CandidateSpoolPacket {
        packet_id: value
            .get("chunk_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        candidate_kind: candidate_kind.to_string(),
        source_kind: source_kind.to_string(),
        path: representative.path.clone(),
        top_level_dir: representative.top_level_dir.clone(),
        source_span_key: representative.source_span_key.clone(),
        symbol_name: representative.symbol_name.clone(),
        score: representative.score,
        selection_bucket,
        input_count: 1,
        value,
        serialized_bytes,
    })
}

fn sort_candidate_spool_inputs(inputs: &mut [CandidateSpoolInput]) {
    inputs.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| candidate_spool_input_key(left).cmp(&candidate_spool_input_key(right)))
    });
}

fn candidate_spool_input_key(input: &CandidateSpoolInput) -> String {
    format!(
        "{:02}|{}|{}|{}|{}|{}",
        candidate_spool_kind_rank(&input.candidate_kind),
        input.source_kind,
        input.path,
        input.source_span_key,
        input.symbol_name,
        input.stable_id
    )
}

fn candidate_spool_packet_cmp(
    left: &CandidateSpoolPacket,
    right: &CandidateSpoolPacket,
) -> std::cmp::Ordering {
    right.score.total_cmp(&left.score).then_with(|| {
        candidate_spool_packet_output_key(left).cmp(&candidate_spool_packet_output_key(right))
    })
}

fn candidate_spool_packet_output_key(packet: &CandidateSpoolPacket) -> String {
    format!(
        "{:02}|{}|{}|{}|{}|{}|{}",
        candidate_spool_kind_rank(&packet.candidate_kind),
        packet.source_kind,
        packet.path,
        packet.source_span_key,
        packet.symbol_name,
        packet.selection_bucket,
        packet.packet_id
    )
}

fn candidate_spool_kind_order() -> [&'static str; 9] {
    [
        "file_path_title",
        "symbol_signature",
        "text_evidence_snippet",
        "import_export_source_navigation",
        "parser_local_reference",
        "relation_neighborhood_candidate",
        "doc_comment_or_title",
        "test_or_mock_candidate",
        "unknown",
    ]
}

fn candidate_spool_kind_rank(kind: &str) -> usize {
    candidate_spool_kind_order()
        .iter()
        .position(|candidate| *candidate == kind)
        .unwrap_or(usize::MAX)
}

fn candidate_spool_candidate_kind(
    chunk_kind: &str,
    source_kind: &str,
    selection_bucket: &str,
    source_role: &str,
) -> String {
    let bucket = selection_bucket.to_ascii_lowercase();
    let role = source_role.to_ascii_lowercase();
    match chunk_kind {
        "file_path_title" => "file_path_title",
        "snippet" | "source_snippet" if source_kind == "text_evidence" => "text_evidence_snippet",
        "function" | "method" | "type" | "module_file" | "signature" | "qname" => {
            "symbol_signature"
        }
        "doc_comment" | "source_role" => "doc_comment_or_title",
        "relation_neighborhood" if bucket.contains("import") || bucket.contains("export") => {
            "import_export_source_navigation"
        }
        "relation_neighborhood" if role.contains("source_navigation") => {
            "relation_neighborhood_candidate"
        }
        "relation_neighborhood" => "parser_local_reference",
        _ if bucket.contains("test") || bucket.contains("mock") => "test_or_mock_candidate",
        _ if source_kind == "text_evidence" => "text_evidence_snippet",
        _ => "unknown",
    }
    .to_string()
}

fn candidate_spool_low_signal_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.len() < 4 || !trimmed.chars().any(|ch| ch.is_ascii_alphanumeric()) {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "none" | "null" | "true" | "false" | "todo" | "fixme"
    )
}

fn candidate_spool_file_kind_from_path(path: &str) -> String {
    let pseudo = VectorEmbeddingChunk {
        chunk_id: String::new(),
        chunk_kind: VectorEmbeddingChunkKind::FilePathTitle,
        source_kind: VectorEmbeddingChunkSourceKind::Metadata,
        file_id: path.to_string(),
        path: path.to_string(),
        entity_id: None,
        source_span: None,
        source_role: String::new(),
        evidence_role: String::new(),
        proof_status: "candidate_only".to_string(),
        graph_proof: false,
        claimable_for_graph: false,
        text: String::new(),
        token_count: 0,
        byte_count: 0,
        language: None,
        file_kind: None,
        lifecycle_binding: None,
        content_hash: String::new(),
        source_file_content_hash: None,
        source_file_size_bytes: None,
        source_file_modified_unix_nanos: None,
        extraction_version: VECTOR_EMBEDDING_CHUNK_EXTRACTION_VERSION.to_string(),
        selection_score: None,
        selection_bucket: None,
        selection_reason: None,
        top_level_dir: None,
        cap_stage: None,
    };
    vector_chunk_file_kind_label(&pseudo)
}

fn candidate_spool_source_span_key(span: &Value) -> String {
    if span.is_null() {
        return "no-span".to_string();
    }
    let start_line = span
        .get("start_line")
        .and_then(Value::as_u64)
        .or_else(|| span.get("line").and_then(Value::as_u64))
        .unwrap_or(0);
    let start_col = span
        .get("start_col")
        .and_then(Value::as_u64)
        .or_else(|| span.get("column").and_then(Value::as_u64))
        .unwrap_or(0);
    let end_line = span
        .get("end_line")
        .and_then(Value::as_u64)
        .unwrap_or(start_line);
    let end_col = span
        .get("end_col")
        .and_then(Value::as_u64)
        .unwrap_or(start_col);
    format!("{start_line}:{start_col}-{end_line}:{end_col}")
}

fn candidate_spool_symbol_name(chunk: &Value, text: &str) -> String {
    if let Some(token) = text
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
        .find(|part| {
            part.len() >= 3
                && part.chars().any(|ch| ch.is_ascii_alphabetic())
                && (part.contains('_')
                    || part
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_lowercase()))
                && !matches!(*part, "Function" | "Method" | "Type" | "file")
        })
    {
        return token.to_string();
    }
    if let Some(entity_id) = chunk.get("entity_id").and_then(Value::as_str) {
        let display = reference_display_name(entity_id);
        if !display.trim().is_empty() {
            return display;
        }
    }
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
        .find(|part| part.chars().any(|ch| ch.is_ascii_alphabetic()))
        .unwrap_or("")
        .to_string()
}

fn candidate_spool_input_score(
    candidate_kind: &str,
    source_kind: &str,
    chunk_kind: &str,
    path: &str,
    file_kind: &str,
    source_span: &Value,
    source_role: &str,
    text: &str,
) -> f64 {
    let mut score = match candidate_kind {
        "file_path_title" => 700.0,
        "symbol_signature" => 650.0,
        "text_evidence_snippet" => 610.0,
        "import_export_source_navigation" => 540.0,
        "parser_local_reference" => 500.0,
        "relation_neighborhood_candidate" => 460.0,
        "doc_comment_or_title" => 430.0,
        "test_or_mock_candidate" => 390.0,
        _ => 200.0,
    };
    score += match source_kind {
        "graph_entity" => 90.0,
        "text_evidence" => 80.0,
        "metadata" => 60.0,
        _ => 10.0,
    };
    score += match chunk_kind {
        "function" | "method" => 45.0,
        "type" | "module_file" | "signature" => 35.0,
        "snippet" | "source_snippet" => 25.0,
        "relation_neighborhood" => 15.0,
        _ => 5.0,
    };
    if !source_span.is_null() {
        score += 35.0;
    }
    score += vector_path_priority(path);
    score += match file_kind {
        "mk" | "makefile" | "buildroot_package_metadata" => 35.0,
        "kconfig" | "config" => 34.0,
        "adoc" | "markdown" | "md" => 28.0,
        "support_script" | "shell" | "no_extension_text" => 30.0,
        "source" | "rust" | "python" | "go" | "c" | "cpp" | "typescript" | "javascript" => 24.0,
        _ => 6.0,
    };
    let lower_path = path.to_ascii_lowercase();
    if lower_path.contains("implementation_trace") || text.contains("implementation_trace") {
        score += 30.0;
    }
    if lower_path.contains("test")
        || lower_path.contains("mock")
        || text.to_ascii_lowercase().contains("mock")
    {
        score += 12.0;
    }
    if source_role.contains("source_navigation") {
        score += 10.0;
    }
    score + candidate_spool_rare_token_density(text)
}

fn candidate_spool_rare_token_density(text: &str) -> f64 {
    let mut tokens = BTreeSet::new();
    let mut total = 0usize;
    for token in text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')) {
        if token.len() < 3 {
            continue;
        }
        total += 1;
        if token.len() >= 8 || token.chars().any(|ch| ch == '_') {
            tokens.insert(token.to_ascii_lowercase());
        }
    }
    if total == 0 {
        return 0.0;
    }
    ((tokens.len() as f64 / total as f64) * 40.0).min(40.0)
}

fn candidate_spool_snippet_json(input: &CandidateSpoolInput) -> Value {
    json!({
        "source_span": input.source_span.clone(),
        "text": input.text.clone(),
        "text_bytes": input.text.len()
    })
}

fn candidate_spool_symbol_json(input: &CandidateSpoolInput) -> Value {
    json!({
        "symbol": input.symbol_name.clone(),
        "signature": input.text.clone(),
        "source_span": input.source_span.clone(),
        "entity_id": input.entity_id.clone()
    })
}

fn candidate_spool_navigation_json(input: &CandidateSpoolInput) -> Value {
    json!({
        "text": input.text.clone(),
        "source_span": input.source_span.clone(),
        "entity_id": input.entity_id.clone(),
        "proof_status": "candidate_only",
        "source_navigation_evidence": true,
        "graph_proof": false
    })
}

fn candidate_spool_index_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn candidate_spool_index_normalize(value: &str) -> String {
    value.replace('\\', "/").trim().to_ascii_lowercase()
}

fn candidate_spool_index_filename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase()
}

fn candidate_spool_index_symbol_name(value: &Value) -> String {
    if let Some(entity_id) = value.get("entity_id").and_then(Value::as_str) {
        if let Some(tail) = entity_id
            .rsplit([':', '/', '#', '.'])
            .find(|part| !part.trim().is_empty())
        {
            return tail.trim().to_string();
        }
    }
    if let Some(symbols) = value.get("symbols").and_then(Value::as_array) {
        for symbol in symbols {
            let text = symbol.get("text").and_then(Value::as_str).unwrap_or("");
            if let Some(name) = candidate_spool_symbol_name_from_text(text) {
                return name;
            }
        }
    }
    candidate_spool_symbol_name_from_text(
        value
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
    .unwrap_or_default()
}

fn candidate_spool_symbol_name_from_text(text: &str) -> Option<String> {
    let mut previous = "";
    for token in text
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        .filter(|token| !token.is_empty())
    {
        if matches!(
            previous,
            "fn" | "function" | "def" | "class" | "struct" | "enum"
        ) && token
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        {
            return Some(token.trim_matches('.').to_string());
        }
        previous = token;
    }
    None
}

fn candidate_spool_index_terms_from_text(text: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    let normalized = candidate_spool_index_normalize(text);
    if normalized.len() >= 2 && normalized.len() <= 256 {
        terms.insert(normalized.clone());
    }
    for term in normalized
        .split(|ch: char| {
            !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '/' || ch == '.')
        })
        .filter(|term| term.len() >= 2)
    {
        terms.insert(term.trim_matches(['/', '.']).to_string());
        for segment in term
            .split(['/', '.', '-'])
            .filter(|segment| segment.len() >= 2)
        {
            terms.insert(segment.to_string());
        }
    }
    terms.retain(|term| term.len() >= 2 && term.len() <= 128);
    terms
}

fn candidate_spool_index_collect_terms(value: &Value) -> BTreeMap<(String, String), i64> {
    let mut terms: BTreeMap<(String, String), i64> = BTreeMap::new();
    let mut add = |field: &str, text: &str, weight: i64| {
        for term in candidate_spool_index_terms_from_text(text) {
            terms
                .entry((term, field.to_string()))
                .and_modify(|current| *current = (*current).max(weight))
                .or_insert(weight);
        }
    };
    let path = candidate_spool_index_string(value, "path");
    add("path", &path, 50);
    add("filename", &candidate_spool_index_filename(&path), 60);
    add(
        "candidate_kind",
        &candidate_spool_index_string(value, "candidate_kind"),
        35,
    );
    add(
        "chunk_kind",
        &candidate_spool_index_string(value, "chunk_kind"),
        25,
    );
    add(
        "source_kind",
        &candidate_spool_index_string(value, "source_kind"),
        25,
    );
    add(
        "source_role",
        &candidate_spool_index_string(value, "source_role"),
        20,
    );
    add(
        "evidence_role",
        &candidate_spool_index_string(value, "evidence_role"),
        20,
    );
    add(
        "file_kind",
        &candidate_spool_index_string(value, "file_kind"),
        20,
    );
    add(
        "top_level_dir",
        &candidate_spool_index_string(value, "top_level_dir"),
        20,
    );
    add(
        "entity_id",
        &candidate_spool_index_string(value, "entity_id"),
        35,
    );
    add(
        "text",
        value.get("text").and_then(Value::as_str).unwrap_or(""),
        10,
    );
    add("symbol", &candidate_spool_index_symbol_name(value), 70);
    for key in ["snippets", "symbols", "source_navigation"] {
        if let Some(items) = value.get(key).and_then(Value::as_array) {
            for item in items {
                add(
                    key,
                    item.get("text").and_then(Value::as_str).unwrap_or(""),
                    12,
                );
                add(
                    key,
                    item.get("entity_id").and_then(Value::as_str).unwrap_or(""),
                    25,
                );
            }
        }
    }
    terms
}

fn candidate_spool_query_index_insert_value(
    connection: &Connection,
    value: &Value,
) -> Result<(), IndexError> {
    let chunk_id = value
        .get("chunk_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            IndexError::Message("candidate spool packet missing chunk_id".to_string())
        })?;
    let path = candidate_spool_index_string(value, "path");
    let normalized_path = candidate_spool_index_normalize(&path);
    let filename = candidate_spool_index_filename(&path);
    let symbol_name = candidate_spool_index_symbol_name(value);
    let normalized_symbol = candidate_spool_index_normalize(&symbol_name);
    let source_span_json = serde_json::to_string(value.get("source_span").unwrap_or(&Value::Null))
        .map_err(|error| IndexError::Message(error.to_string()))?;
    let chunk_json =
        serde_json::to_string(value).map_err(|error| IndexError::Message(error.to_string()))?;
    let source_file_size_bytes = value
        .get("source_file_size_bytes")
        .and_then(Value::as_u64)
        .and_then(|value| i64::try_from(value).ok());
    let source_file_content_hash = value
        .get("source_file_content_hash")
        .and_then(Value::as_str);
    connection
        .execute(
            "DELETE FROM candidate_spool_terms WHERE chunk_id = ?1",
            params![chunk_id],
        )
        .map_err(|error| {
            sqlite_index_error("candidate_spool_query_index_delete_terms_failed", error)
        })?;
    connection
        .execute(
            "INSERT OR REPLACE INTO candidate_spool_records(
                chunk_id, candidate_kind, packet_kind, chunk_kind, source_kind, path,
                normalized_path, filename, top_level_dir, file_kind, source_role, evidence_role,
                symbol_name, normalized_symbol, source_span_json, entity_id, text_preview,
                selection_score, selection_bucket, source_file_content_hash,
                source_file_size_bytes, chunk_json
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                ?16, ?17, ?18, ?19, ?20, ?21, ?22
             )",
            params![
                chunk_id,
                candidate_spool_index_string(value, "candidate_kind"),
                candidate_spool_index_string(value, "packet_kind"),
                candidate_spool_index_string(value, "chunk_kind"),
                candidate_spool_index_string(value, "source_kind"),
                path,
                normalized_path,
                filename,
                candidate_spool_index_string(value, "top_level_dir"),
                candidate_spool_index_string(value, "file_kind"),
                candidate_spool_index_string(value, "source_role"),
                candidate_spool_index_string(value, "evidence_role"),
                symbol_name,
                normalized_symbol,
                source_span_json,
                value.get("entity_id").and_then(Value::as_str),
                value.get("text").and_then(Value::as_str).unwrap_or(""),
                value
                    .get("selection_score")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                candidate_spool_index_string(value, "selection_bucket"),
                value
                    .get("source_file_content_hash")
                    .and_then(Value::as_str),
                source_file_size_bytes,
                chunk_json
            ],
        )
        .map_err(|error| sqlite_index_error("candidate_spool_query_index_insert_failed", error))?;
    if !path.is_empty() {
        connection
            .execute(
                "INSERT OR IGNORE INTO candidate_spool_source_bindings(
                    path, source_file_content_hash, source_file_size_bytes, record_count
                 ) VALUES (?1, ?2, ?3, 0)",
                params![path, source_file_content_hash, source_file_size_bytes],
            )
            .map_err(|error| {
                sqlite_index_error(
                    "candidate_spool_query_index_source_binding_insert_failed",
                    error,
                )
            })?;
        connection
            .execute(
                "UPDATE candidate_spool_source_bindings
                 SET source_file_content_hash = ?2,
                     source_file_size_bytes = ?3,
                     record_count = record_count + 1
                 WHERE path = ?1",
                params![path, source_file_content_hash, source_file_size_bytes],
            )
            .map_err(|error| {
                sqlite_index_error(
                    "candidate_spool_query_index_source_binding_update_failed",
                    error,
                )
            })?;
    }
    for ((term, field), weight) in candidate_spool_index_collect_terms(value) {
        connection
            .execute(
                "INSERT OR REPLACE INTO candidate_spool_terms(term, chunk_id, field, weight)
                 VALUES (?1, ?2, ?3, ?4)",
                params![term, chunk_id, field, weight],
            )
            .map_err(|error| {
                sqlite_index_error("candidate_spool_query_index_term_failed", error)
            })?;
    }
    Ok(())
}

fn append_candidate_spool_query_index_packets(
    spool: &mut CandidateSpoolSummary,
    packets: &[CandidateSpoolPacket],
) -> Result<(), IndexError> {
    let spool_path = PathBuf::from(&spool.candidate_spool_path);
    let index_path = candidate_spool_query_index_path(&spool_path);
    if let Some(parent) = index_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let connection = Connection::open(&index_path)
        .map_err(|error| sqlite_index_error("candidate_spool_query_index_open_failed", error))?;
    candidate_spool_query_index_create_schema(&connection)?;
    for packet in packets {
        candidate_spool_query_index_insert_value(&connection, &packet.value)?;
    }
    refresh_candidate_spool_query_index_summary(spool)?;
    Ok(())
}

fn append_candidate_spool_chunks(
    summary: &mut IndexSummary,
    chunks: Vec<Value>,
) -> Result<(), IndexError> {
    if chunks.is_empty() {
        return Ok(());
    }
    let Some(spool) = summary.candidate_spool.as_mut() else {
        return Ok(());
    };
    let caps = spool.caps.clone();
    let selection = select_candidate_spool_packets(chunks, &caps, spool);
    let path = PathBuf::from(&spool.candidate_spool_path);
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    spool.generated_total_chunks += selection.generated_count;
    spool.normalized_total_chunks += selection.normalized_count;
    spool.deduped_total_chunks += selection.deduped_count;
    spool.selected_total_chunks += selection.selected_count;
    spool.persisted_total_chunks += selection.persisted_count;
    spool.omitted_by_cap += selection.omitted_by_cap;
    spool.omitted_by_budget += selection.omitted_by_budget;
    spool.omitted_by_dedup += selection.omitted_by_dedup;
    spool.omitted_low_signal += selection.omitted_low_signal;
    spool.omitted_by_file_limit += selection.omitted_by_file_limit;
    spool.omitted_by_dir_limit += selection.omitted_by_dir_limit;
    spool.omitted_by_kind_limit += selection.omitted_by_kind_limit;
    for packet in &selection.selected {
        spool
            .selected_candidate_ids
            .insert(packet.packet_id.clone());
        *spool
            .selected_file_counts
            .entry(packet.path.clone())
            .or_default() += 1;
        *spool
            .selected_dir_counts
            .entry(packet.top_level_dir.clone())
            .or_default() += 1;
        *spool
            .selected_kind_counts
            .entry(packet.candidate_kind.clone())
            .or_default() += 1;
        *spool
            .selected_source_counts
            .entry(packet.source_kind.clone())
            .or_default() += 1;
        if let Some(source_kind) = packet.value.get("source_kind").and_then(Value::as_str) {
            *spool
                .spooled_by_source_kind
                .entry(source_kind.to_string())
                .or_default() += 1;
        }
        if let Some(chunk_kind) = packet.value.get("chunk_kind").and_then(Value::as_str) {
            *spool
                .spooled_by_chunk_kind
                .entry(chunk_kind.to_string())
                .or_default() += 1;
        }
        serde_json::to_writer(&mut file, &packet.value)
            .map_err(|error| IndexError::Message(error.to_string()))?;
        file.write_all(b"\n")?;
        spool.spooled_total_chunks += 1;
    }
    spool.status = "partial_ready".to_string();
    spool.candidate_spool_status = "partial_ready".to_string();
    spool.lifecycle = "partial_spool".to_string();
    spool.incomplete = true;
    spool.reason =
        Some("candidate spool contains local evidence while indexing is in progress".to_string());
    spool.spooled_bytes = fs::metadata(&path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    spool.candidate_spool_budget_bytes = caps.global_max_bytes as u64;
    spool.candidate_spool_truncated = spool.omitted_by_budget > 0
        || spool.omitted_by_cap > 0
        || spool.omitted_by_file_limit > 0
        || spool.omitted_by_dir_limit > 0
        || spool.omitted_by_kind_limit > 0
        || spool.spooled_bytes as usize >= caps.global_max_bytes;
    spool.candidate_spool_partial = spool.candidate_spool_truncated || spool.incomplete;
    if spool.query_index_status != "disabled" {
        append_candidate_spool_query_index_packets(spool, &selection.selected)?;
    }
    Ok(())
}

fn mark_candidate_spool_final(
    summary: &mut IndexSummary,
    repo_root: &Path,
    db_path: &Path,
    options: &IndexOptions,
) -> Result<(), IndexError> {
    if let Some(spool) = summary.candidate_spool.as_mut() {
        let ready_status = if spool.candidate_spool_truncated {
            "truncated_ready"
        } else {
            "bounded_ready"
        };
        spool.status = ready_status.to_string();
        spool.candidate_spool_status = ready_status.to_string();
        spool.lifecycle = "final_spool".to_string();
        spool.incomplete = false;
        spool.candidate_spool_partial = spool.candidate_spool_truncated;
        spool.reason = Some("all deterministic local candidate evidence for this index run was written; graph DB proof may still be unpublished".to_string());
        refresh_candidate_spool_query_index_summary(spool)?;
    }
    rewrite_candidate_spool_jsonl(summary, repo_root, db_path, options)
}

fn mark_candidate_spool_superseded(
    summary: &mut IndexSummary,
    repo_root: &Path,
    db_path: &Path,
    options: &IndexOptions,
    passport: &DbPassport,
) -> Result<(), IndexError> {
    if let Some(spool) = summary.candidate_spool.as_mut() {
        spool.status = "superseded_by_graph_db".to_string();
        spool.candidate_spool_status = "superseded_by_graph_db".to_string();
        spool.lifecycle = "superseded_by_graph_db".to_string();
        spool.incomplete = false;
        spool.candidate_spool_partial = spool.candidate_spool_truncated;
        spool.db_passport_hash = Some(db_passport_fingerprint(passport));
        spool.db_passport_snapshot = Some(passport.clone());
        spool.reason = Some("final graph DB passport is available; normal graph-proof surfaces must use DB verification, not the spool".to_string());
        refresh_candidate_spool_query_index_summary(spool)?;
    }
    rewrite_candidate_spool_jsonl(summary, repo_root, db_path, options)
}

fn candidate_spool_paths_equivalent(left: &str, right: &str) -> bool {
    let left = candidate_spool_normalize_path_identity(left);
    let right = candidate_spool_normalize_path_identity(right);
    left == right || windows_path_identity_strings_equivalent(&left, &right)
}

fn candidate_spool_normalize_path_identity(value: &str) -> String {
    let mut normalized = value.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("//?/UNC/") {
        normalized = format!("//{rest}");
    } else if let Some(rest) = normalized.strip_prefix("//?/") {
        normalized = rest.to_string();
    }
    normalized.trim_end_matches('/').to_ascii_lowercase()
}

#[cfg(windows)]
fn windows_path_identity_strings_equivalent(left: &str, right: &str) -> bool {
    let left_parts = left
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let right_parts = right
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    left_parts.len() == right_parts.len()
        && left_parts
            .iter()
            .zip(right_parts.iter())
            .all(|(left, right)| windows_path_component_equivalent(left, right))
}

#[cfg(not(windows))]
fn windows_path_identity_strings_equivalent(_left: &str, _right: &str) -> bool {
    false
}

#[cfg(windows)]
fn windows_path_component_equivalent(left: &str, right: &str) -> bool {
    left == right
        || windows_short_alias_component_matches(left, right)
        || windows_short_alias_component_matches(right, left)
}

#[cfg(windows)]
fn windows_short_alias_component_matches(short: &str, long: &str) -> bool {
    let Some((prefix, suffix)) = short.split_once('~') else {
        return false;
    };
    if prefix.len() < 3 {
        return false;
    }
    let digit_count = suffix.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digit_count == 0 {
        return false;
    }
    let suffix_tail = &suffix[digit_count..];
    if !suffix_tail.is_empty() && !suffix_tail.starts_with('.') {
        return false;
    }
    let long_compact = long
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    long_compact.starts_with(prefix)
}

fn read_candidate_spool_manifest(spool_path: &Path) -> Result<Value, IndexError> {
    if !spool_path.exists() {
        return Err(IndexError::Message(format!(
            "candidate_spool_missing: {} does not exist",
            spool_path.display()
        )));
    }
    let file = fs::File::open(spool_path)?;
    let reader = BufReader::new(file);
    for (line_index, line) in reader.lines().take(8).enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|error| {
            IndexError::Message(format!(
                "candidate_spool_corrupt line {}: {error}",
                line_index + 1
            ))
        })?;
        let metadata = value
            .get("metadata")
            .cloned()
            .unwrap_or_else(|| value.clone());
        if metadata.get("artifact_kind").is_some() {
            return Ok(metadata);
        }
    }
    Err(IndexError::Message(
        "candidate_spool_corrupt: missing metadata header".to_string(),
    ))
}

fn candidate_spool_manifest_count(metadata: &Value) -> usize {
    [
        "persisted_total_chunks",
        "candidate_spool_record_count",
        "spooled_total_chunks",
        "selected_total_chunks",
    ]
    .iter()
    .find_map(|key| metadata.get(*key).and_then(Value::as_u64))
    .unwrap_or(0) as usize
}

fn candidate_spool_index_load_from_parts(
    spool_path: &Path,
    index_path: &Path,
    metadata: Value,
    status: &str,
    reason: Option<String>,
    stale: bool,
    record_count: usize,
    source_binding_count: usize,
) -> CandidateSpoolIndexLoad {
    let mut metadata = metadata;
    if let Some(object) = metadata.as_object_mut() {
        object.insert("query_index_status".to_string(), json!(status));
        object.insert("query_index_kind".to_string(), json!("sqlite"));
        object.insert(
            "query_index_path".to_string(),
            json!(path_string(index_path)),
        );
        object.insert(
            "query_index_version".to_string(),
            json!(CANDIDATE_SPOOL_QUERY_INDEX_VERSION),
        );
        object.insert("query_index_record_count".to_string(), json!(record_count));
        object.insert(
            "query_index_source_binding_count".to_string(),
            json!(source_binding_count),
        );
        object.insert(
            "query_index_bytes".to_string(),
            json!(fs::metadata(index_path)
                .map(|metadata| metadata.len())
                .unwrap_or(0)),
        );
    }
    CandidateSpoolIndexLoad {
        path: spool_path.to_path_buf(),
        query_index_path: index_path.to_path_buf(),
        metadata,
        stale,
        reason,
        query_index_status: status.to_string(),
        query_index_kind: "sqlite".to_string(),
        query_index_bytes: fs::metadata(index_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0),
        query_index_record_count: record_count,
        query_index_version: CANDIDATE_SPOOL_QUERY_INDEX_VERSION.to_string(),
        query_index_bound_manifest_hash: None,
        query_index_source_binding_count: source_binding_count,
    }
}

fn candidate_spool_query_index_open_failure_status(
    error: &rusqlite::Error,
) -> (&'static str, String) {
    let message = error.to_string();
    let status =
        match classify_sqlite_access_problem(&message).map(|problem| problem.db_problem_kind) {
            Some("sqlite_corrupt") => "corrupt",
            Some("permission_denied") => "permission_denied",
            Some("filesystem_inaccessible") => "filesystem_inaccessible",
            Some(_) | None => "sidecar_unavailable",
        };
    (
        status,
        format!("candidate_spool_query_index_open_failed: {status}: {message}"),
    )
}

pub fn candidate_spool_index_status_for_repo(
    repo_root: &Path,
    spool_path: &Path,
    allow_stale: bool,
) -> Result<CandidateSpoolIndexLoad, IndexError> {
    let mut metadata = read_candidate_spool_manifest(spool_path)?;
    let kind = metadata
        .get("artifact_kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if kind != "candidate_spool" {
        return Err(IndexError::Message(format!(
            "candidate_spool_wrong_kind: expected candidate_spool, got {kind}"
        )));
    }
    let index_path = candidate_spool_query_index_path(spool_path);
    let mut stale_reasons = Vec::new();
    if let Some(recorded_repo) = metadata.get("repo_root").and_then(Value::as_str) {
        let current_repo = path_string(repo_root);
        if !candidate_spool_paths_equivalent(recorded_repo, &current_repo) {
            stale_reasons.push(format!(
                "foreign_repo: artifact repo_root={recorded_repo}, current_repo_root={current_repo}"
            ));
        }
    }
    if !index_path.exists() {
        let stale = !stale_reasons.is_empty();
        let reason = if stale {
            Some(stale_reasons.join("; "))
        } else {
            Some("candidate spool query index is missing".to_string())
        };
        if stale && !allow_stale {
            return Err(IndexError::Message(format!(
                "candidate_spool_stale: {}",
                reason.clone().unwrap_or_default()
            )));
        }
        return Ok(candidate_spool_index_load_from_parts(
            spool_path,
            &index_path,
            metadata,
            "index_missing",
            reason,
            stale,
            0,
            0,
        ));
    }
    let connection = match Connection::open(&index_path) {
        Ok(connection) => connection,
        Err(error) => {
            let (status, reason) = candidate_spool_query_index_open_failure_status(&error);
            return Ok(candidate_spool_index_load_from_parts(
                spool_path,
                &index_path,
                metadata,
                status,
                Some(reason),
                true,
                0,
                0,
            ));
        }
    };
    if let Err(error) = candidate_spool_query_index_create_schema(&connection) {
        return Ok(candidate_spool_index_load_from_parts(
            spool_path,
            &index_path,
            metadata,
            "corrupt",
            Some(error.to_string()),
            true,
            0,
            0,
        ));
    }
    let record_count = candidate_spool_query_index_record_count(&connection)?;
    let source_binding_count = candidate_spool_query_index_source_binding_count(&connection)?;
    let manifest_count = candidate_spool_manifest_count(&metadata);
    if manifest_count > 0 && record_count != manifest_count {
        stale_reasons.push(format!(
            "query_index_record_count_mismatch: manifest={manifest_count} index={record_count}"
        ));
    }
    if let Some(index_version) =
        candidate_spool_query_index_metadata_value(&connection, "query_index_version")?
    {
        if index_version != CANDIDATE_SPOOL_QUERY_INDEX_VERSION {
            stale_reasons.push(format!(
                "query_index_version_mismatch: expected={} actual={index_version}",
                CANDIDATE_SPOOL_QUERY_INDEX_VERSION
            ));
        }
    } else {
        return Ok(candidate_spool_index_load_from_parts(
            spool_path,
            &index_path,
            metadata,
            "corrupt",
            Some("candidate_spool_query_index_missing_version".to_string()),
            true,
            record_count,
            source_binding_count,
        ));
    }
    if let Some(index_spool_path) =
        candidate_spool_query_index_metadata_value(&connection, "spool_path")?
    {
        if !candidate_spool_paths_equivalent(&index_spool_path, &path_string(spool_path)) {
            stale_reasons.push(format!(
                "query_index_spool_path_mismatch: index={index_spool_path} spool={}",
                path_string(spool_path)
            ));
        }
    }
    let binding_hash =
        candidate_spool_query_index_metadata_value(&connection, "query_index_bound_manifest_hash")?;
    let manifest_status = metadata
        .get("candidate_spool_status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let manifest_incomplete = metadata
        .get("incomplete")
        .and_then(Value::as_bool)
        .unwrap_or(matches!(manifest_status, "building" | "partial_ready"));
    if !manifest_incomplete && !matches!(manifest_status, "building" | "partial_ready") {
        if let (Some(index_binding_hash), Some(manifest_binding_hash)) = (
            binding_hash.as_deref(),
            metadata
                .get("query_index_bound_manifest_hash")
                .and_then(Value::as_str),
        ) {
            if index_binding_hash != manifest_binding_hash {
                stale_reasons.push(format!(
                    "query_index_manifest_binding_mismatch: manifest={manifest_binding_hash} index={index_binding_hash}"
                ));
            }
        }
    }
    if let Some(index_spool_status) =
        candidate_spool_query_index_metadata_value(&connection, "candidate_spool_status")?
    {
        let current_spool_status = metadata
            .get("candidate_spool_status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if current_spool_status == "building"
            && matches!(
                index_spool_status.as_str(),
                "partial_ready" | "bounded_ready" | "truncated_ready"
            )
        {
            if let Some(object) = metadata.as_object_mut() {
                object.insert(
                    "candidate_spool_status".to_string(),
                    json!(index_spool_status.clone()),
                );
                object.insert("status".to_string(), json!(index_spool_status.clone()));
                if index_spool_status == "partial_ready" {
                    object.insert("lifecycle".to_string(), json!("partial_spool"));
                    object.insert("incomplete".to_string(), json!(true));
                }
            }
        }
    }
    stale_reasons.extend(candidate_spool_query_index_source_binding_stale_reasons(
        &connection,
        repo_root,
    )?);
    stale_reasons.sort();
    stale_reasons.dedup();
    let stale = !stale_reasons.is_empty();
    if stale && !allow_stale {
        return Err(IndexError::Message(format!(
            "candidate_spool_stale: {}",
            stale_reasons.join("; ")
        )));
    }
    let status = if stale { "stale" } else { "ready" };
    let mut load = candidate_spool_index_load_from_parts(
        spool_path,
        &index_path,
        metadata,
        status,
        stale.then(|| stale_reasons.join("; ")),
        stale,
        record_count,
        source_binding_count,
    );
    load.query_index_bound_manifest_hash = binding_hash;
    Ok(load)
}

fn candidate_spool_jsonl_safe_to_rebuild(metadata: &Value, spool_path: &Path) -> bool {
    let bytes = fs::metadata(spool_path)
        .map(|metadata| metadata.len())
        .unwrap_or(u64::MAX);
    let count = candidate_spool_manifest_count(metadata);
    let record_model = metadata
        .get("record_model")
        .and_then(Value::as_str)
        .unwrap_or("");
    record_model == CANDIDATE_SPOOL_RECORD_MODEL_VERSION
        || (bytes <= CANDIDATE_SPOOL_QUERY_INDEX_SAFE_REBUILD_MAX_BYTES
            && count <= CANDIDATE_SPOOL_QUERY_INDEX_SAFE_REBUILD_MAX_RECORDS)
}

pub fn rebuild_candidate_spool_query_index_for_repo(
    repo_root: &Path,
    spool_path: &Path,
) -> Result<PathBuf, IndexError> {
    let metadata = read_candidate_spool_manifest(spool_path)?;
    if metadata
        .get("artifact_kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        != "candidate_spool"
    {
        return Err(IndexError::Message(
            "candidate_spool_wrong_kind: expected candidate_spool".to_string(),
        ));
    }
    if !candidate_spool_jsonl_safe_to_rebuild(&metadata, spool_path) {
        return Err(IndexError::Message(
            "candidate_spool_query_index_missing: legacy or oversized candidate spool requires explicit audit inspection, not hot-path scanning".to_string(),
        ));
    }
    let index_path = candidate_spool_query_index_path(spool_path);
    remove_file_if_exists(&index_path)?;
    let connection = Connection::open(&index_path)
        .map_err(|error| sqlite_index_error("candidate_spool_query_index_open_failed", error))?;
    candidate_spool_query_index_create_schema(&connection)?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "metadata_version",
        CANDIDATE_SPOOL_METADATA_VERSION,
    )?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "query_index_version",
        CANDIDATE_SPOOL_QUERY_INDEX_VERSION,
    )?;
    candidate_spool_query_index_set_metadata(&connection, "query_index_status", "ready")?;
    candidate_spool_query_index_set_metadata(&connection, "query_index_kind", "sqlite")?;
    candidate_spool_query_index_set_metadata(&connection, "artifact_kind", "candidate_spool")?;
    candidate_spool_query_index_set_metadata(&connection, "spool_path", path_string(spool_path))?;
    candidate_spool_query_index_set_metadata(&connection, "repo_root", path_string(repo_root))?;
    if let Some(status) = metadata
        .get("candidate_spool_status")
        .and_then(Value::as_str)
    {
        candidate_spool_query_index_set_metadata(&connection, "candidate_spool_status", status)?;
    }
    if let Some(record_model) = metadata.get("record_model").and_then(Value::as_str) {
        candidate_spool_query_index_set_metadata(&connection, "record_model", record_model)?;
    }
    let file = fs::File::open(spool_path)?;
    let reader = BufReader::new(file);
    for (line_index, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|error| {
            IndexError::Message(format!(
                "candidate_spool_corrupt line {}: {error}",
                line_index + 1
            ))
        })?;
        if value.get("chunk_id").is_some() || value.get("text").is_some() {
            candidate_spool_query_index_insert_value(&connection, &value)?;
        }
    }
    let count = candidate_spool_query_index_record_count(&connection)?;
    let source_binding_count = candidate_spool_query_index_source_binding_count(&connection)?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "query_index_record_count",
        count.to_string(),
    )?;
    candidate_spool_query_index_set_metadata(
        &connection,
        "query_index_source_binding_count",
        source_binding_count.to_string(),
    )?;
    Ok(index_path)
}

fn candidate_spool_sql_mode_filter(subcommand: &str) -> &'static str {
    match subcommand {
        "files" => "(candidate_kind = 'file_path_title' OR chunk_kind = 'file_path_title')",
        "symbols" => "(source_kind = 'graph_entity' OR candidate_kind IN ('symbol_signature', 'import_export_source_navigation', 'parser_local_reference', 'relation_neighborhood_candidate', 'doc_comment_or_title', 'test_or_mock_candidate'))",
        "text" => "(source_kind = 'text_evidence' OR candidate_kind = 'text_evidence_snippet' OR chunk_kind = 'snippet')",
        _ => "1 = 1",
    }
}

fn candidate_spool_query_terms(query: &str) -> BTreeSet<String> {
    let mut terms = candidate_spool_index_terms_from_text(query);
    let filename = candidate_spool_index_filename(query);
    if filename.len() >= 2 {
        terms.insert(filename);
    }
    terms
}

fn candidate_spool_query_add_ids(
    connection: &Connection,
    scores: &mut BTreeMap<String, i64>,
    sql: &str,
    parameter: &str,
    base_score: i64,
    limit: usize,
) -> Result<(), IndexError> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| sqlite_index_error("candidate_spool_query_prepare_failed", error))?;
    let rows = statement
        .query_map(params![parameter, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1).unwrap_or(0)))
        })
        .map_err(|error| sqlite_index_error("candidate_spool_query_failed", error))?;
    for row in rows {
        let (chunk_id, score) =
            row.map_err(|error| sqlite_index_error("candidate_spool_query_row_failed", error))?;
        *scores.entry(chunk_id).or_default() += base_score + score;
    }
    Ok(())
}

fn candidate_spool_query_exact_ids(
    connection: &Connection,
    scores: &mut BTreeMap<String, i64>,
    subcommand: &str,
    query: &str,
    limit: usize,
) -> Result<(), IndexError> {
    let normalized = candidate_spool_index_normalize(query);
    let filename = candidate_spool_index_filename(query);
    let mode = candidate_spool_sql_mode_filter(subcommand);
    let exact_limit = limit.saturating_mul(8).max(limit).max(1);
    let sql = format!(
        "SELECT chunk_id, 0 FROM candidate_spool_records WHERE {mode} AND normalized_path = ?1 ORDER BY selection_score DESC, chunk_id LIMIT ?2"
    );
    candidate_spool_query_add_ids(connection, scores, &sql, &normalized, 1_000, exact_limit)?;
    let sql = format!(
        "SELECT chunk_id, 0 FROM candidate_spool_records WHERE {mode} AND filename = ?1 ORDER BY selection_score DESC, chunk_id LIMIT ?2"
    );
    candidate_spool_query_add_ids(connection, scores, &sql, &filename, 700, exact_limit)?;
    if subcommand == "symbols" {
        let sql = format!(
            "SELECT chunk_id, 0 FROM candidate_spool_records WHERE {mode} AND normalized_symbol = ?1 ORDER BY selection_score DESC, chunk_id LIMIT ?2"
        );
        candidate_spool_query_add_ids(connection, scores, &sql, &normalized, 900, exact_limit)?;
    }
    if subcommand == "files" {
        let sql = format!(
            "SELECT chunk_id, 0 FROM candidate_spool_records WHERE {mode} AND (top_level_dir = ?1 OR file_kind = ?1) ORDER BY selection_score DESC, chunk_id LIMIT ?2"
        );
        candidate_spool_query_add_ids(connection, scores, &sql, &normalized, 400, exact_limit)?;
    }
    Ok(())
}

fn candidate_spool_query_term_ids(
    connection: &Connection,
    scores: &mut BTreeMap<String, i64>,
    subcommand: &str,
    terms: &BTreeSet<String>,
    limit: usize,
) -> Result<(), IndexError> {
    let mode = candidate_spool_sql_mode_filter(subcommand);
    let term_limit = limit.saturating_mul(12).max(limit).max(1);
    let sql = format!(
        "SELECT records.chunk_id, SUM(terms.weight) AS score
         FROM candidate_spool_terms terms
         JOIN candidate_spool_records records ON records.chunk_id = terms.chunk_id
         WHERE {mode} AND terms.term = ?1
         GROUP BY records.chunk_id
         ORDER BY score DESC, records.chunk_id
         LIMIT ?2"
    );
    for term in terms {
        candidate_spool_query_add_ids(connection, scores, &sql, term, 0, term_limit)?;
    }
    Ok(())
}

fn candidate_spool_query_top_ids(
    connection: &Connection,
    scores: &mut BTreeMap<String, i64>,
    subcommand: &str,
    limit: usize,
) -> Result<(), IndexError> {
    let mode = candidate_spool_sql_mode_filter(subcommand);
    let sql = format!(
        "SELECT chunk_id, 0 FROM candidate_spool_records WHERE {mode} ORDER BY selection_score DESC, chunk_id LIMIT ?1"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| sqlite_index_error("candidate_spool_query_prepare_failed", error))?;
    let rows = statement
        .query_map(params![limit as i64], |row| row.get::<_, String>(0))
        .map_err(|error| sqlite_index_error("candidate_spool_query_failed", error))?;
    for row in rows {
        let chunk_id =
            row.map_err(|error| sqlite_index_error("candidate_spool_query_row_failed", error))?;
        *scores.entry(chunk_id).or_default() += 1;
    }
    Ok(())
}

fn candidate_spool_query_load_chunk(
    connection: &Connection,
    chunk_id: &str,
) -> Result<Value, IndexError> {
    let chunk_json = connection
        .query_row(
            "SELECT chunk_json FROM candidate_spool_records WHERE chunk_id = ?1",
            params![chunk_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| sqlite_index_error("candidate_spool_query_load_chunk_failed", error))?;
    serde_json::from_str(&chunk_json).map_err(|error| IndexError::Message(error.to_string()))
}

fn candidate_spool_validate_query_hits(
    repo_root: &Path,
    chunks: &mut [Value],
    allow_stale: bool,
) -> Result<Option<String>, IndexError> {
    let mut stale_reasons = Vec::new();
    for chunk in chunks.iter_mut() {
        let Some(path) = chunk.get("path").and_then(Value::as_str) else {
            continue;
        };
        let source_path = repo_root.join(path);
        if !source_path.exists() {
            stale_reasons.push(format!("deleted_file: {path}"));
            continue;
        }
        if let Some(expected_size) = chunk.get("source_file_size_bytes").and_then(Value::as_u64) {
            let actual_size = fs::metadata(&source_path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if actual_size != expected_size {
                stale_reasons.push(format!(
                    "changed_file_size: {path} expected={expected_size} actual={actual_size}"
                ));
                continue;
            }
        }
        if let Some(expected_hash) = chunk
            .get("source_file_content_hash")
            .and_then(Value::as_str)
        {
            match fs::read_to_string(&source_path) {
                Ok(source) => {
                    let actual_hash = content_hash(&source);
                    if actual_hash != expected_hash {
                        stale_reasons.push(format!("changed_file_hash: {path}"));
                    }
                }
                Err(error) => stale_reasons.push(format!("read_failed: {path}: {error}")),
            }
        }
    }
    stale_reasons.sort();
    stale_reasons.dedup();
    if stale_reasons.is_empty() {
        return Ok(None);
    }
    let reason = stale_reasons.join("; ");
    if !allow_stale {
        return Err(IndexError::Message(format!(
            "candidate_spool_stale: {reason}"
        )));
    }
    for chunk in chunks {
        if let Some(object) = chunk.as_object_mut() {
            object.insert("stale".to_string(), json!(true));
            object.insert("stale_reason".to_string(), json!(reason.clone()));
        }
    }
    Ok(Some(reason))
}

pub fn query_candidate_spool_index_for_repo(
    repo_root: &Path,
    spool_path: &Path,
    subcommand: &str,
    query: &str,
    limit: usize,
    allow_stale: bool,
) -> Result<CandidateSpoolIndexQueryResult, IndexError> {
    if !matches!(
        subcommand,
        "files" | "text" | "symbols" | "all" | "candidates"
    ) {
        return Err(IndexError::Message(format!(
            "candidate spool early mode only supports query files/text/symbols/all, got {subcommand}"
        )));
    }
    let mut load = candidate_spool_index_status_for_repo(repo_root, spool_path, allow_stale)?;
    if load.query_index_status == "index_missing" {
        rebuild_candidate_spool_query_index_for_repo(repo_root, spool_path)?;
        load = candidate_spool_index_status_for_repo(repo_root, spool_path, allow_stale)?;
    }
    if load.query_index_status != "ready" && !(allow_stale && load.query_index_status == "stale") {
        return Err(IndexError::Message(format!(
            "candidate_spool_query_index_unavailable: status={} reason={}",
            load.query_index_status,
            load.reason.clone().unwrap_or_default()
        )));
    }
    let connection = Connection::open(&load.query_index_path)
        .map_err(|error| sqlite_index_error("candidate_spool_query_index_open_failed", error))?;
    candidate_spool_query_index_create_schema(&connection)?;
    let terms = candidate_spool_query_terms(query);
    let mut scores = BTreeMap::new();
    candidate_spool_query_exact_ids(&connection, &mut scores, subcommand, query, limit)?;
    candidate_spool_query_term_ids(&connection, &mut scores, subcommand, &terms, limit)?;
    if scores.is_empty() && terms.is_empty() {
        candidate_spool_query_top_ids(&connection, &mut scores, subcommand, limit)?;
    }
    let mut scored = scores.into_iter().collect::<Vec<_>>();
    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let mut chunks = Vec::new();
    for (chunk_id, _score) in scored.into_iter().take(limit) {
        let mut chunk = candidate_spool_query_load_chunk(&connection, &chunk_id)?;
        if let Some(object) = chunk.as_object_mut() {
            object.insert("query_index_kind".to_string(), json!("sqlite"));
            object.insert(
                "query_index_status".to_string(),
                json!(load.query_index_status.clone()),
            );
            object.insert("candidate_only".to_string(), json!(true));
            object.insert("graph_proof".to_string(), json!(false));
            object.insert("claimable_for_graph".to_string(), json!(false));
        }
        chunks.push(chunk);
    }
    let stale_reason = candidate_spool_validate_query_hits(repo_root, &mut chunks, allow_stale)?;
    if stale_reason.is_some() {
        load.stale = true;
        load.reason = stale_reason;
    }
    let omitted_count = load.query_index_record_count.saturating_sub(chunks.len());
    Ok(CandidateSpoolIndexQueryResult {
        load,
        chunks,
        omitted_count,
    })
}

fn candidate_spool_lifecycle_binding() -> RetrievalCandidateLifecycleBinding {
    RetrievalCandidateLifecycleBinding {
        status: RetrievalCandidateLifecycleStatus::Unknown,
        db_passport_fingerprint: None,
        repo_head: None,
        scope_policy_hash: None,
        embedding_model_id: None,
        embedding_profile: None,
        stale_reason: Some(
            "candidate spool is not graph proof and requires graph verification".to_string(),
        ),
    }
}

fn candidate_spool_chunk_value(
    mut chunk: VectorEmbeddingChunk,
    source_file_content_hash: &str,
    source_file_size_bytes: u64,
    source_file_modified_unix_nanos: Option<&str>,
    lifecycle: &str,
    indexing_phase: &str,
    selection_bucket: &str,
    selection_reason: &str,
) -> Result<Value, IndexError> {
    chunk.graph_proof = false;
    chunk.claimable_for_graph = false;
    if chunk.proof_status.trim().is_empty() {
        chunk.proof_status = "candidate_only".to_string();
    }
    if chunk.lifecycle_binding.is_none() {
        chunk.lifecycle_binding = Some(candidate_spool_lifecycle_binding());
    }
    chunk.selection_score.get_or_insert(0.0);
    chunk
        .selection_bucket
        .get_or_insert(selection_bucket.to_string());
    chunk
        .selection_reason
        .get_or_insert(selection_reason.to_string());
    let mut value = serde_json::to_value(chunk).map_err(|error| {
        IndexError::Message(format!("candidate spool chunk serialize failed: {error}"))
    })?;
    if let Some(object) = value.as_object_mut() {
        object.insert("artifact_kind".to_string(), json!("candidate_spool"));
        object.insert("candidate_only".to_string(), json!(true));
        object.insert("graph_proof".to_string(), json!(false));
        object.insert("claimable_for_graph".to_string(), json!(false));
        object.insert("requires_graph_verification".to_string(), json!(true));
        object.insert("creates_graph_relations".to_string(), json!(false));
        object.insert("can_answer_graph_proof".to_string(), json!(false));
        object.insert("lifecycle".to_string(), json!(lifecycle));
        object.insert("candidate_spool_lifecycle".to_string(), json!(lifecycle));
        object.insert(
            "incomplete".to_string(),
            json!(lifecycle != "final_spool" && lifecycle != "superseded_by_graph_db"),
        );
        object.insert("spooled_indexing_phase".to_string(), json!(indexing_phase));
        object.insert(
            "source_file_content_hash".to_string(),
            json!(source_file_content_hash),
        );
        object.insert(
            "source_file_size_bytes".to_string(),
            json!(source_file_size_bytes),
        );
        object.insert(
            "source_file_modified_unix_nanos".to_string(),
            source_file_modified_unix_nanos
                .map(|value| json!(value))
                .unwrap_or(Value::Null),
        );
        object.insert("db_passport_hash".to_string(), Value::Null);
        object.insert("selection_score".to_string(), json!(0.0));
        object.insert("selection_bucket".to_string(), json!(selection_bucket));
        object.insert("selection_reason".to_string(), json!(selection_reason));
    }
    Ok(value)
}

fn candidate_spool_chunks_for_text_evidence(
    repo_relative_path: &str,
    source: &str,
    file_hash: &str,
    file_kind: &str,
    size_bytes: u64,
    modified_unix_nanos: Option<&str>,
) -> Result<Vec<Value>, IndexError> {
    let lifecycle = candidate_spool_lifecycle_binding();
    let mut chunks = Vec::new();
    chunks.push(candidate_spool_chunk_value(
        extract_file_path_title_embedding_chunk_for_path(
            repo_relative_path,
            TEXT_EVIDENCE_KIND,
            Some(file_kind),
            Some(lifecycle.clone()),
        ),
        file_hash,
        size_bytes,
        modified_unix_nanos,
        "partial_spool",
        "stage0_text_evidence",
        "file_path_title",
        "Stage 0 text evidence path/title candidate; source text existence only, not graph proof",
    )?);
    for chunk in extract_text_evidence_embedding_chunks_for_path(
        repo_relative_path,
        source,
        Some(lifecycle.clone()),
    ) {
        chunks.push(candidate_spool_chunk_value(
            chunk,
            file_hash,
            size_bytes,
            modified_unix_nanos,
            "partial_spool",
            "stage0_text_evidence",
            "text_evidence",
            "Stage 0 text evidence snippet; proves source text existence only, not graph proof",
        )?);
    }
    Ok(chunks)
}

fn candidate_spool_chunks_for_local_bundle(
    bundle: &LocalFactBundle,
) -> Result<Vec<Value>, IndexError> {
    let lifecycle = candidate_spool_lifecycle_binding();
    let modified_unix_nanos = bundle
        .extraction
        .file
        .metadata
        .get("modified_unix_nanos")
        .and_then(Value::as_str);
    let size_bytes = bundle.extraction.file.size_bytes;
    let file_kind = bundle.language.as_deref();
    let mut chunks = Vec::new();
    chunks.push(candidate_spool_chunk_value(
        extract_file_path_title_embedding_chunk_for_path(
            &bundle.repo_relative_path,
            "source_navigation",
            file_kind,
            Some(lifecycle.clone()),
        ),
        &bundle.file_hash,
        size_bytes,
        modified_unix_nanos,
        "partial_spool",
        "parser_local_evidence",
        "file_path_title",
        "File path/title candidate emitted from local parser input; not graph proof",
    )?);
    for entity in bundle
        .extraction
        .entities
        .iter()
        .filter(|entity| should_generate_vector_chunks_for_entity(entity))
    {
        for chunk in extract_graph_entity_embedding_chunks(
            entity,
            Some(&bundle.source),
            bundle.language.as_deref(),
            Some(lifecycle.clone()),
        ) {
            if matches!(entity.kind, EntityKind::File | EntityKind::Module)
                && chunk.chunk_kind == VectorEmbeddingChunkKind::SourceSnippet
            {
                continue;
            }
            chunks.push(candidate_spool_chunk_value(
                chunk,
                &bundle.file_hash,
                size_bytes,
                modified_unix_nanos,
                "partial_spool",
                "parser_local_evidence",
                "symbol_signature",
                "Parser-local declared symbol candidate; graph verification required before proof",
            )?);
        }
    }
    for relation in &bundle.imports {
        chunks.push(candidate_spool_relation_candidate_chunk(
            bundle,
            relation,
            "import_signature",
            "Parser-local import/export source-navigation candidate; no resolved cross-file graph proof",
        )?);
    }
    for relation in &bundle.exports {
        chunks.push(candidate_spool_relation_candidate_chunk(
            bundle,
            relation,
            "export_signature",
            "Parser-local import/export source-navigation candidate; no resolved cross-file graph proof",
        )?);
    }
    Ok(chunks)
}

fn candidate_spool_relation_candidate_chunk(
    bundle: &LocalFactBundle,
    relation: &LocalFactRelation,
    selection_bucket: &str,
    selection_reason: &str,
) -> Result<Value, IndexError> {
    let display = reference_display_name(&relation.tail_id);
    let text = bounded_vector_chunk_text(&format!(
        "{} {} in {} near {}",
        relation.relation, display, bundle.repo_relative_path, relation.source_span
    ));
    let chunk = build_vector_embedding_chunk(VectorChunkBuildInput {
        source_kind: VectorEmbeddingChunkSourceKind::Metadata,
        chunk_kind: VectorEmbeddingChunkKind::RelationNeighborhood,
        path: &bundle.repo_relative_path,
        entity_id: Some(&relation.head_id),
        source_span: Some(relation.source_span.clone()),
        source_role: "source_navigation",
        evidence_role: "source_navigation",
        proof_status: "candidate_only",
        graph_proof: false,
        claimable_for_graph: false,
        text,
        language: bundle.language.clone(),
        file_kind: bundle.language.clone(),
        lifecycle_binding: Some(candidate_spool_lifecycle_binding()),
    });
    let modified_unix_nanos = bundle
        .extraction
        .file
        .metadata
        .get("modified_unix_nanos")
        .and_then(Value::as_str);
    candidate_spool_chunk_value(
        chunk,
        &bundle.file_hash,
        bundle.extraction.file.size_bytes,
        modified_unix_nanos,
        "partial_spool",
        "parser_local_import_export_evidence",
        selection_bucket,
        selection_reason,
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct VectorChunkKindCounts {
    total: usize,
    text_evidence: usize,
    graph_entity: usize,
    file_path_title: usize,
    metadata: usize,
}

impl VectorChunkKindCounts {
    fn from_chunks<'a>(chunks: impl IntoIterator<Item = &'a VectorEmbeddingChunk>) -> Self {
        let mut counts = Self::default();
        for chunk in chunks {
            counts.total += 1;
            match chunk.source_kind {
                VectorEmbeddingChunkSourceKind::GraphEntity => counts.graph_entity += 1,
                VectorEmbeddingChunkSourceKind::TextEvidence => counts.text_evidence += 1,
                VectorEmbeddingChunkSourceKind::Metadata => counts.metadata += 1,
            }
            if chunk.chunk_kind == VectorEmbeddingChunkKind::FilePathTitle {
                counts.file_path_title += 1;
            }
        }
        counts
    }
}

#[derive(Debug, Clone)]
struct RankedVectorChunk {
    chunk: VectorEmbeddingChunk,
    score: f64,
    bucket: String,
    reason: String,
}

#[derive(Debug, Clone)]
struct VectorChunkSelectionResult {
    selected: Vec<VectorEmbeddingChunk>,
    omitted_by_cap: usize,
    omitted_by_bucket_limit: usize,
    omitted_low_signal: usize,
    per_file_cap: usize,
    per_directory_soft_cap: usize,
}

fn select_vector_chunks_for_persistence(
    chunks: Vec<VectorEmbeddingChunk>,
    max_chunks: usize,
) -> VectorChunkSelectionResult {
    if max_chunks == 0 {
        return VectorChunkSelectionResult {
            selected: Vec::new(),
            omitted_by_cap: chunks.len(),
            omitted_by_bucket_limit: 0,
            omitted_low_signal: 0,
            per_file_cap: 0,
            per_directory_soft_cap: 0,
        };
    }

    let mut deduped = BTreeMap::new();
    for chunk in chunks {
        deduped.entry(chunk.chunk_id.clone()).or_insert(chunk);
    }

    let mut omitted_low_signal = 0usize;
    let mut ranked = Vec::new();
    for chunk in deduped.into_values() {
        if vector_chunk_is_low_signal(&chunk) {
            omitted_low_signal += 1;
            continue;
        }
        let score = vector_chunk_selection_score(&chunk);
        ranked.push(RankedVectorChunk {
            bucket: vector_chunk_selection_bucket(&chunk),
            reason: vector_chunk_selection_reason(&chunk, score),
            chunk,
            score,
        });
    }
    ranked.sort_by(vector_ranked_chunk_cmp);

    let top_dirs = ranked
        .iter()
        .map(|ranked| vector_chunk_top_level_dir(&ranked.chunk.path))
        .collect::<BTreeSet<_>>();
    let per_file_cap = max_chunks.min(24).max(1);
    let per_directory_soft_cap = if top_dirs.is_empty() {
        max_chunks
    } else {
        ((max_chunks + top_dirs.len() - 1) / top_dirs.len())
            .saturating_mul(2)
            .min(max_chunks)
            .max(1)
    };

    let mut selected_ids = BTreeSet::new();
    let mut selected = Vec::new();
    let mut dir_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut file_counts: BTreeMap<String, usize> = BTreeMap::new();

    for source_kind in [
        VectorEmbeddingChunkSourceKind::GraphEntity,
        VectorEmbeddingChunkSourceKind::TextEvidence,
        VectorEmbeddingChunkSourceKind::Metadata,
    ] {
        if selected.len() >= max_chunks {
            break;
        }
        if let Some(index) = ranked.iter().position(|candidate| {
            candidate.chunk.source_kind == source_kind
                && !selected_ids.contains(&candidate.chunk.chunk_id)
        }) {
            select_ranked_vector_chunk(
                &ranked[index],
                "source_kind_minimum",
                &mut selected,
                &mut selected_ids,
                &mut dir_counts,
                &mut file_counts,
            );
        }
    }

    let mut seen_dirs = BTreeSet::new();
    for candidate in &ranked {
        if selected.len() >= max_chunks || seen_dirs.len() >= max_chunks {
            break;
        }
        let dir = vector_chunk_top_level_dir(&candidate.chunk.path);
        if seen_dirs.insert(dir) && !selected_ids.contains(&candidate.chunk.chunk_id) {
            select_ranked_vector_chunk(
                candidate,
                "top_level_dir_diversity",
                &mut selected,
                &mut selected_ids,
                &mut dir_counts,
                &mut file_counts,
            );
        }
    }

    let mut seen_file_kinds = BTreeSet::new();
    for candidate in &ranked {
        if selected.len() >= max_chunks || seen_file_kinds.len() >= max_chunks {
            break;
        }
        let file_kind = vector_chunk_file_kind_label(&candidate.chunk);
        if seen_file_kinds.insert(file_kind) && !selected_ids.contains(&candidate.chunk.chunk_id) {
            select_ranked_vector_chunk(
                candidate,
                "file_kind_diversity",
                &mut selected,
                &mut selected_ids,
                &mut dir_counts,
                &mut file_counts,
            );
        }
    }

    let mut omitted_by_bucket_limit = 0usize;
    for candidate in &ranked {
        if selected.len() >= max_chunks {
            break;
        }
        if selected_ids.contains(&candidate.chunk.chunk_id) {
            continue;
        }
        let dir = vector_chunk_top_level_dir(&candidate.chunk.path);
        let file = candidate.chunk.path.clone();
        if dir_counts.get(&dir).copied().unwrap_or(0) >= per_directory_soft_cap
            || file_counts.get(&file).copied().unwrap_or(0) >= per_file_cap
        {
            omitted_by_bucket_limit += 1;
            continue;
        }
        select_ranked_vector_chunk(
            candidate,
            "diversity_rank_fill",
            &mut selected,
            &mut selected_ids,
            &mut dir_counts,
            &mut file_counts,
        );
    }

    for candidate in &ranked {
        if selected.len() >= max_chunks {
            break;
        }
        if selected_ids.contains(&candidate.chunk.chunk_id) {
            continue;
        }
        select_ranked_vector_chunk(
            candidate,
            "relaxed_cap_fill",
            &mut selected,
            &mut selected_ids,
            &mut dir_counts,
            &mut file_counts,
        );
    }

    selected.sort_by(|left, right| left.chunk_id.cmp(&right.chunk_id));
    let omitted_by_cap = ranked.len().saturating_sub(selected.len());
    VectorChunkSelectionResult {
        selected,
        omitted_by_cap,
        omitted_by_bucket_limit,
        omitted_low_signal,
        per_file_cap,
        per_directory_soft_cap,
    }
}

fn select_ranked_vector_chunk(
    ranked: &RankedVectorChunk,
    cap_stage: &str,
    selected: &mut Vec<VectorEmbeddingChunk>,
    selected_ids: &mut BTreeSet<String>,
    dir_counts: &mut BTreeMap<String, usize>,
    file_counts: &mut BTreeMap<String, usize>,
) {
    if !selected_ids.insert(ranked.chunk.chunk_id.clone()) {
        return;
    }
    let mut chunk = ranked.chunk.clone();
    let dir = vector_chunk_top_level_dir(&chunk.path);
    chunk.selection_score = Some(ranked.score);
    chunk.selection_bucket = Some(ranked.bucket.clone());
    chunk.selection_reason = Some(ranked.reason.clone());
    chunk.top_level_dir = Some(dir.clone());
    chunk.cap_stage = Some(cap_stage.to_string());
    *dir_counts.entry(dir).or_default() += 1;
    *file_counts.entry(chunk.path.clone()).or_default() += 1;
    selected.push(chunk);
}

fn vector_ranked_chunk_cmp(
    left: &RankedVectorChunk,
    right: &RankedVectorChunk,
) -> std::cmp::Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.chunk.chunk_id.cmp(&right.chunk.chunk_id))
}

fn vector_chunk_is_low_signal(chunk: &VectorEmbeddingChunk) -> bool {
    let text = chunk.text.trim();
    text.len() < 4 || !text.chars().any(|ch| ch.is_ascii_alphanumeric())
}

fn vector_chunk_selection_score(chunk: &VectorEmbeddingChunk) -> f64 {
    let mut score = 0.0;
    score += match chunk.source_kind {
        VectorEmbeddingChunkSourceKind::GraphEntity => 320.0,
        VectorEmbeddingChunkSourceKind::TextEvidence => 280.0,
        VectorEmbeddingChunkSourceKind::Metadata => 220.0,
    };
    score += match chunk.chunk_kind {
        VectorEmbeddingChunkKind::Function | VectorEmbeddingChunkKind::Method => 90.0,
        VectorEmbeddingChunkKind::Type | VectorEmbeddingChunkKind::ModuleFile => 75.0,
        VectorEmbeddingChunkKind::Signature => 70.0,
        VectorEmbeddingChunkKind::FilePathTitle => 65.0,
        VectorEmbeddingChunkKind::SourceSnippet | VectorEmbeddingChunkKind::Snippet => 55.0,
        VectorEmbeddingChunkKind::DocComment => 45.0,
        VectorEmbeddingChunkKind::RelationNeighborhood => 40.0,
        VectorEmbeddingChunkKind::QName | VectorEmbeddingChunkKind::SourceRole => 30.0,
    };
    if chunk.source_span.is_some() {
        score += 35.0;
    }
    score += vector_file_kind_priority(chunk);
    score += vector_path_priority(&chunk.path);
    let token_count = chunk.token_count;
    if (2..=80).contains(&token_count) {
        score += 20.0;
    }
    if chunk.byte_count <= VECTOR_EMBEDDING_CHUNK_MAX_TEXT_BYTES / 2 {
        score += 10.0;
    }
    score
}

fn vector_chunk_selection_bucket(chunk: &VectorEmbeddingChunk) -> String {
    format!(
        "{}:{}:{}",
        chunk.source_kind.as_str(),
        vector_chunk_top_level_dir(&chunk.path),
        vector_chunk_file_kind_label(chunk)
    )
}

fn vector_chunk_selection_reason(chunk: &VectorEmbeddingChunk, score: f64) -> String {
    let mut reasons = Vec::new();
    reasons.push(format!("source_kind={}", chunk.source_kind.as_str()));
    reasons.push(format!("chunk_kind={}", chunk.chunk_kind.as_str()));
    reasons.push(format!(
        "top_level_dir={}",
        vector_chunk_top_level_dir(&chunk.path)
    ));
    reasons.push(format!("file_kind={}", vector_chunk_file_kind_label(chunk)));
    if chunk.source_span.is_some() {
        reasons.push("source_span".to_string());
    }
    if vector_path_priority(&chunk.path) > 0.0 {
        reasons.push("path_role_hint".to_string());
    }
    reasons.push(format!("score={score:.3}"));
    reasons.join("; ")
}

fn vector_file_kind_priority(chunk: &VectorEmbeddingChunk) -> f64 {
    let path = chunk.path.to_ascii_lowercase();
    let kind = vector_chunk_file_kind_label(chunk);
    match kind.as_str() {
        "source" | "rust" | "typescript" | "javascript" | "python" | "go" | "c" | "cpp" => 55.0,
        "makefile" | "mk" | "buildroot_package_metadata" => 55.0,
        "kconfig" | "config" => 55.0,
        "shell" | "support_script" | "no_extension_text" => 50.0,
        "adoc" | "markdown" | "md" => 42.0,
        "test" => 35.0,
        _ if path.ends_with(".mk") => 55.0,
        _ if path.ends_with("config.in") || path.ends_with("kconfig") => 55.0,
        _ if path.ends_with(".adoc") || path.ends_with(".md") => 42.0,
        _ if path.contains("/support/") || path.starts_with("support/") => 50.0,
        _ => 20.0,
    }
}

fn vector_path_priority(path: &str) -> f64 {
    let path = normalize_graph_path(path).to_ascii_lowercase();
    let mut score = 0.0;
    if path.starts_with("package/") {
        score += 35.0;
    }
    if path.starts_with("support/") {
        score += 32.0;
    }
    if path.starts_with("docs/") {
        score += 24.0;
    }
    if path.starts_with("configs/") {
        score += 22.0;
    }
    if path.starts_with("src/") || path.starts_with("crates/") {
        score += 28.0;
    }
    for needle in [
        "config.in",
        "kconfig",
        ".mk",
        "pkg-generic",
        "pkg-download",
        "download",
        "wrapper",
        "package",
    ] {
        if path.contains(needle) {
            score += 12.0;
        }
    }
    score
}

fn vector_chunk_top_level_dir(path: &str) -> String {
    let normalized = normalize_graph_path(path);
    if !normalized.contains('/') {
        return ".".to_string();
    }
    normalized
        .split('/')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(".")
        .to_string()
}

fn vector_chunk_file_kind_label(chunk: &VectorEmbeddingChunk) -> String {
    if let Some(kind) = chunk.file_kind.as_deref().filter(|kind| !kind.is_empty()) {
        return kind.to_ascii_lowercase();
    }
    let path = chunk.path.to_ascii_lowercase();
    if path.ends_with(".mk") {
        "mk".to_string()
    } else if path.ends_with("config.in") || path.ends_with("kconfig") {
        "kconfig".to_string()
    } else if path.ends_with(".adoc") {
        "adoc".to_string()
    } else if path.ends_with(".md") {
        "markdown".to_string()
    } else if path.ends_with(".rs")
        || path.ends_with(".ts")
        || path.ends_with(".tsx")
        || path.ends_with(".js")
        || path.ends_with(".py")
        || path.ends_with(".go")
        || path.ends_with(".c")
        || path.ends_with(".cpp")
        || path.ends_with(".h")
    {
        "source".to_string()
    } else if path.starts_with("support/") && !path.rsplit('/').next().unwrap_or("").contains('.') {
        "no_extension_text".to_string()
    } else {
        "unknown".to_string()
    }
}

fn count_chunks_by_top_level_dir<'a>(
    chunks: impl IntoIterator<Item = &'a VectorEmbeddingChunk>,
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for chunk in chunks {
        *counts
            .entry(vector_chunk_top_level_dir(&chunk.path))
            .or_default() += 1;
    }
    counts
}

fn count_chunks_by_file_kind<'a>(
    chunks: impl IntoIterator<Item = &'a VectorEmbeddingChunk>,
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for chunk in chunks {
        *counts
            .entry(vector_chunk_file_kind_label(chunk))
            .or_default() += 1;
    }
    counts
}

fn count_chunks_by_source_kind<'a>(
    chunks: impl IntoIterator<Item = &'a VectorEmbeddingChunk>,
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for chunk in chunks {
        *counts
            .entry(chunk.source_kind.as_str().to_string())
            .or_default() += 1;
    }
    counts
}

fn vector_chunk_kind_for_entity(kind: EntityKind) -> VectorEmbeddingChunkKind {
    match kind {
        EntityKind::Function => VectorEmbeddingChunkKind::Function,
        EntityKind::Method | EntityKind::Constructor => VectorEmbeddingChunkKind::Method,
        EntityKind::Class
        | EntityKind::Interface
        | EntityKind::Trait
        | EntityKind::Enum
        | EntityKind::Type
        | EntityKind::GenericType => VectorEmbeddingChunkKind::Type,
        EntityKind::Module | EntityKind::File => VectorEmbeddingChunkKind::ModuleFile,
        _ => VectorEmbeddingChunkKind::Signature,
    }
}

fn stable_vector_chunk_id(
    source_kind: VectorEmbeddingChunkSourceKind,
    chunk_kind: VectorEmbeddingChunkKind,
    path: &str,
    entity_id: Option<&str>,
    source_span: Option<&SourceSpan>,
    content_hash: &str,
) -> String {
    let entity = entity_id.unwrap_or("-");
    let span = source_span
        .map(ToString::to_string)
        .unwrap_or_else(|| "no-span".to_string());
    format!(
        "vector-chunk:{}:{}:{}:{}:{}:{}",
        source_kind.as_str(),
        chunk_kind.as_str(),
        path,
        entity,
        span,
        content_hash
    )
}

fn bounded_vector_chunk_text(text: &str) -> String {
    bounded_text_prefix(text.trim(), VECTOR_EMBEDDING_CHUNK_MAX_TEXT_BYTES).to_string()
}

fn source_span_text(source: &str, span: &SourceSpan) -> Option<String> {
    let start = span.start_line.max(1);
    let end = span.end_line.max(start);
    let selected = source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line_number = (index + 1) as u32;
            (line_number >= start && line_number <= end).then_some(line)
        })
        .collect::<Vec<_>>();
    let text = selected.join("\n");
    (!text.trim().is_empty()).then(|| bounded_vector_chunk_text(&text))
}

fn leading_doc_comment(source: &str, span: &SourceSpan) -> Option<(SourceSpan, String)> {
    let lines = source.lines().collect::<Vec<_>>();
    let start_index = span.start_line.saturating_sub(1) as usize;
    if start_index == 0 || start_index > lines.len() {
        return None;
    }

    let mut selected = Vec::<(u32, String)>::new();
    for index in (0..start_index).rev() {
        let trimmed = lines[index].trim();
        if trimmed.is_empty() {
            if selected.is_empty() {
                continue;
            }
            break;
        }
        if !is_doc_comment_line(trimmed) {
            break;
        }
        selected.push(((index + 1) as u32, trimmed.to_string()));
    }
    selected.reverse();
    let first_line = selected.first().map(|(line, _)| *line)?;
    let last_line = selected.last().map(|(line, _)| *line)?;
    let text = selected
        .into_iter()
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n");
    Some((
        SourceSpan::new(&span.repo_relative_path, first_line, last_line),
        bounded_vector_chunk_text(&text),
    ))
}

fn is_doc_comment_line(trimmed: &str) -> bool {
    trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || trimmed.starts_with("/**")
        || trimmed.starts_with('*')
        || trimmed.starts_with('#')
}

fn vector_chunk_token_count(text: &str) -> usize {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|token| !token.is_empty())
        .count()
}

pub fn default_db_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".codegraph").join("codegraph.sqlite")
}

fn normalize_db_path(repo_root: &Path, db_path: &Path) -> PathBuf {
    if db_path.is_absolute() {
        db_path.to_path_buf()
    } else {
        repo_root.join(db_path)
    }
}

fn resolve_repo_root_for_index(repo_path: &Path) -> Result<PathBuf, IndexError> {
    if !repo_path.exists() {
        return Err(IndexError::RepoNotFound(repo_path.to_path_buf()));
    }
    fs::canonicalize(repo_path).map_err(IndexError::from)
}

fn persisted_entity_ids(entities: &[Entity], storage_mode: StorageMode) -> BTreeSet<String> {
    entities
        .iter()
        .filter(|entity| should_persist_entity(entity, storage_mode))
        .map(|entity| entity.id.clone())
        .collect()
}

fn should_persist_entity(entity: &Entity, storage_mode: StorageMode) -> bool {
    !should_route_static_reference_entity(entity)
        && !should_route_unresolved_entity(entity)
        && !should_route_local_micro_entity_to_non_proof_storage(entity, storage_mode)
}

fn should_route_local_micro_entity_to_non_proof_storage(
    entity: &Entity,
    storage_mode: StorageMode,
) -> bool {
    matches!(storage_mode, StorageMode::Proof)
        && matches!(entity.kind, EntityKind::Expression | EntityKind::CallSite)
}

fn should_generate_vector_chunks_for_entity(entity: &Entity) -> bool {
    !matches!(
        entity.kind,
        EntityKind::Expression
            | EntityKind::Assignment
            | EntityKind::CallSite
            | EntityKind::Parameter
            | EntityKind::LocalVariable
            | EntityKind::ReturnSite
            | EntityKind::Import
            | EntityKind::Export
            | EntityKind::Assertion
    )
}

fn should_persist_edge(edge: &Edge, persisted_entity_ids: &BTreeSet<String>) -> bool {
    if should_route_heuristic_edge(edge) {
        return false;
    }
    if is_compact_callsite_relation(edge.relation) {
        return true;
    }
    persisted_entity_ids.contains(&edge.head_id) && persisted_entity_ids.contains(&edge.tail_id)
}

fn is_compact_callsite_relation(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Callee
            | RelationKind::Argument0
            | RelationKind::Argument1
            | RelationKind::ArgumentN
    )
}

fn should_store_edge_row(edge: &Edge) -> bool {
    let _ = edge;
    true
}

fn should_store_template_edge_row(edge: &Edge) -> bool {
    !matches!(
        edge.relation,
        RelationKind::Contains
            | RelationKind::DefinedIn
            | RelationKind::Declares
            | RelationKind::Callee
            | RelationKind::Argument0
            | RelationKind::Argument1
            | RelationKind::ArgumentN
            | RelationKind::ReturnsTo
    )
}

fn should_route_static_reference_entity(entity: &Entity) -> bool {
    entity.qualified_name.starts_with("static_reference:")
        || entity.name == "unknown_callee"
        || entity
            .created_from
            .to_ascii_lowercase()
            .contains("static-heuristic")
}

fn should_route_unresolved_entity(entity: &Entity) -> bool {
    entity.qualified_name.starts_with("dynamic_import:")
        || entity
            .metadata
            .get("resolution")
            .and_then(Value::as_str)
            .is_some_and(|resolution| resolution.to_ascii_lowercase().contains("unresolved"))
        || entity
            .metadata
            .get("heuristic")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn should_route_heuristic_edge(edge: &Edge) -> bool {
    matches!(
        edge.exactness,
        Exactness::StaticHeuristic | Exactness::Inferred
    ) || matches!(edge.edge_class, EdgeClass::BaseHeuristic)
        || edge.head_id.contains("static_reference:")
        || edge.tail_id.contains("static_reference:")
        || edge.head_id.contains("dynamic_import:")
        || edge.tail_id.contains("dynamic_import:")
        || edge.head_id.contains("unresolved")
        || edge.tail_id.contains("unresolved")
        || edge
            .metadata
            .get("resolution")
            .and_then(Value::as_str)
            .is_some_and(|resolution| resolution.to_ascii_lowercase().contains("unresolved"))
        || edge
            .metadata
            .get("heuristic")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || edge
            .metadata
            .get("resolved")
            .and_then(Value::as_bool)
            .is_some_and(|resolved| !resolved)
}

fn should_index_entity_text(entity: &Entity) -> bool {
    let _ = entity;
    false
}

fn should_index_entity_snippet(entity: &Entity) -> bool {
    let _ = entity;
    false
}

struct SourceSnippetCache<'a> {
    lines: Vec<&'a str>,
}

impl<'a> SourceSnippetCache<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            lines: source.lines().collect(),
        }
    }

    fn snippet(&self, span: &SourceSpan) -> String {
        let start = span.start_line.saturating_sub(1) as usize;
        let end = span.end_line.max(span.start_line) as usize;
        self.lines
            .iter()
            .skip(start)
            .take(end.saturating_sub(start).min(8))
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn unix_time_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(u128::from(u64::MAX)) as u64,
        Err(_) => 0,
    }
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use std::{process, time::Duration};

    use codegraph_core::{EdgeClass, EdgeContext, EntityKind, Exactness, RelationKind};
    use codegraph_query::{
        RetrievalFunnel, RetrievalFunnelConfig, RetrievalFunnelRequest, VectorCandidateBranchStatus,
    };
    use codegraph_store::{EntityFeatureRow, RoutingPacketHandleRow, TextSearchKind};
    use codegraph_vector::DeterministicTestEmbeddingProvider;

    use super::*;

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "codegraph-index-test-{}-{name}-{}",
            process::id(),
            unix_time_ms()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale temp");
        }
        fs::create_dir_all(root.join("src")).expect("create src");
        root
    }

    #[test]
    fn batch_progress_event_uses_processed_not_durable_completed_language() {
        let persisted = PersistedBatchSummary {
            files: 1,
            entities: 2,
            edges: 3,
            duplicate_edges_upserted: 0,
        };
        let event =
            index_batch_processed_progress_event(0, &persisted, 0, 0, 5, 3, 2, 1, 1, 1, 1, 3);

        assert_eq!(event["event"].as_str(), Some("index_batch_processed"));
        assert_eq!(
            event["batch_progress_status"].as_str(),
            Some("processed_not_durably_committed")
        );
        assert_eq!(
            event["visible_db_mutation_claim"].as_str(),
            Some("not_claimed_until_commit_or_publish")
        );
        assert_eq!(event["edges_staged"].as_u64(), Some(3));
        assert_eq!(event["edges_inserted"].as_u64(), Some(3));
    }

    #[test]
    fn graph_output_budget_hits_label_degraded_file_claimability() {
        let entities = (0..6)
            .map(|index| {
                test_entity(
                    "src/fanout.ts",
                    EntityKind::Function,
                    &format!("target{index}"),
                )
            })
            .collect::<Vec<_>>();
        let mut edges = Vec::new();
        for index in 1..6 {
            edges.push(graph_budget_test_edge(
                &entities[0].id,
                RelationKind::Calls,
                &entities[index].id,
                index + 1,
            ));
        }
        let mut extraction = BasicExtraction {
            file: graph_budget_file_record("src/fanout.ts"),
            entities,
            edges,
        };
        let budgets = GraphOutputBudgets {
            max_local_facts_per_file: 4,
            max_relation_fanout_per_file: 2,
            max_derived_edges_per_file: 100,
            max_source_spans_per_file: 100,
            max_reducer_edges_per_stage: 100,
        };

        let hits =
            apply_graph_output_budgets_to_extraction("src/fanout.ts", &mut extraction, &budgets);

        assert!(
            hits.iter()
                .any(|hit| hit.kind == "relation_fanout_per_file" && hit.omitted == 3),
            "high fanout must be reported, not silently dropped: {hits:#?}"
        );
        assert!(
            hits.iter()
                .any(|hit| hit.kind == "local_facts_per_file" && hit.after <= 4),
            "local fact cap must be reported, not silently dropped: {hits:#?}"
        );
        assert!(
            extraction.entities.len() + extraction.edges.len() <= 4,
            "local graph facts should be capped before persistence"
        );
        assert_eq!(
            extraction.file.metadata["graph_output_budget_hit"].as_bool(),
            Some(true)
        );
        assert_eq!(
            extraction.file.metadata["graph_output_claimability"].as_str(),
            Some("degraded_file_nonclaimable_for_omitted_facts")
        );
        assert_eq!(
            extraction.file.metadata["graph_relation_claims"].as_str(),
            Some("partial")
        );
    }

    #[test]
    fn reducer_edge_budget_truncates_and_reports_nonclaimable_omissions() {
        let mut plan = GlobalFactReductionPlan::default();
        for index in 0..5 {
            plan.push_edge(graph_budget_test_edge(
                "src/fanout.ts:head",
                RelationKind::Calls,
                &format!("src/fanout.ts:tail{index}"),
                index + 1,
            ));
        }

        let hit = plan
            .apply_reducer_edge_budget("reduce_test_edges", 2)
            .expect("reducer budget hit");

        assert_eq!(plan.edges.len(), 2);
        assert_eq!(hit.kind, "reducer_edges_per_stage");
        assert_eq!(hit.omitted, 3);
        assert_eq!(
            hit.claimability_label,
            "degraded_file_nonclaimable_for_omitted_facts"
        );
    }

    #[test]
    fn proof_storage_routes_micro_entities_out_of_main_rows() {
        let expression = test_entity("src/main.py", EntityKind::Expression, "expr");
        let callsite = test_entity("src/main.py", EntityKind::CallSite, "call");
        let function = test_entity("src/main.py", EntityKind::Function, "run");

        assert!(!should_persist_entity(&expression, StorageMode::Proof));
        assert!(!should_persist_entity(&callsite, StorageMode::Proof));
        assert!(should_persist_entity(&function, StorageMode::Proof));

        assert!(should_persist_entity(&expression, StorageMode::Debug));
        assert!(should_persist_entity(&callsite, StorageMode::Audit));
    }

    #[test]
    fn proof_storage_keeps_compact_callsite_edges_without_callsite_entity_rows() {
        let callsite = test_entity("src/main.py", EntityKind::CallSite, "call");
        let callee = test_entity("src/main.py", EntityKind::Function, "run");
        let mut persisted_entity_ids = BTreeSet::new();
        persisted_entity_ids.insert(callee.id.clone());

        let edge = Edge {
            id: "edge:callsite:callee".to_string(),
            head_id: callsite.id,
            relation: RelationKind::Callee,
            tail_id: callee.id,
            source_span: SourceSpan::with_columns("src/main.py", 1, 1, 1, 10),
            repo_commit: None,
            file_hash: Some("hash".to_string()),
            extractor: "test".to_string(),
            confidence: 1.0,
            exactness: Exactness::ParserVerified,
            edge_class: EdgeClass::ReifiedCallsite,
            context: EdgeContext::Production,
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Metadata::default(),
        };

        assert!(should_persist_edge(&edge, &persisted_entity_ids));
    }

    #[test]
    fn vector_chunk_generation_skips_low_level_local_entities() {
        for kind in [
            EntityKind::Expression,
            EntityKind::Assignment,
            EntityKind::CallSite,
            EntityKind::Parameter,
            EntityKind::LocalVariable,
            EntityKind::ReturnSite,
            EntityKind::Import,
            EntityKind::Export,
            EntityKind::Assertion,
        ] {
            assert!(
                !should_generate_vector_chunks_for_entity(&test_entity(
                    "src/main.py",
                    kind,
                    "local"
                )),
                "{kind} should not produce release vector chunks"
            );
        }

        assert!(should_generate_vector_chunks_for_entity(&test_entity(
            "src/main.py",
            EntityKind::Function,
            "run"
        )));
        assert!(should_generate_vector_chunks_for_entity(&test_entity(
            "src/main.py",
            EntityKind::Class,
            "Runner"
        )));
    }

    fn write_test_file(root: &Path, relative: &str, source: &str) {
        let path = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, source).expect("write test file");
    }

    fn read_jsonl_values(path: &Path) -> Vec<Value> {
        fs::read_to_string(path)
            .expect("read jsonl")
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("jsonl value"))
            .collect()
    }

    fn candidate_spool_fixture_repo(name: &str) -> PathBuf {
        let repo = temp_repo(name);
        write_test_file(
            &repo,
            "src/lib.rs",
            r#"
pub fn spool_target(value: i32) -> i32 {
    value + 1
}

pub use spool_target as exported_spool_target;
"#,
        );
        write_test_file(
            &repo,
            "docs/README.md",
            "Stage 0 text evidence fixture for spool_target source navigation.\n",
        );
        repo
    }

    #[cfg(windows)]
    #[test]
    fn candidate_spool_repo_binding_accepts_windows_short_home_alias() {
        assert!(candidate_spool_paths_equivalent(
            r"\\?\C:\Users\runneradmin\AppData\Local\Temp\codegraph-cli-unit-1",
            r"C:\Users\RUNNER~1\AppData\Local\Temp\codegraph-cli-unit-1"
        ));
        assert!(!candidate_spool_paths_equivalent(
            r"\\?\C:\Users\runneradmin\AppData\Local\Temp\codegraph-cli-unit-1",
            r"C:\Users\RUNNER~1\AppData\Local\Temp\codegraph-cli-unit-2"
        ));
    }

    #[test]
    fn candidate_spool_failpoint_writes_partial_before_final_db_publish() {
        let repo = candidate_spool_fixture_repo("candidate-spool-partial");
        let db = repo.join("artifacts").join("codegraph.sqlite");
        let spool = repo.join("artifacts").join("candidate-spool.jsonl");
        let mut options = fresh_rebuild_options();
        options.candidate_spool_path = Some(spool.clone());
        let result =
            with_write_path_chaos_failpoint("candidate_spool_after_batch_before_db_write", || {
                index_repo_to_db_with_options(&repo, &db, options)
            });
        assert!(result
            .expect_err("failpoint should abort before final DB publish")
            .to_string()
            .contains("candidate_spool_after_batch_before_db_write"));
        assert!(
            !db.exists(),
            "atomic cold failure must not publish the final DB"
        );
        let values = read_jsonl_values(&spool);
        let chunks = values
            .iter()
            .filter(|value| value.get("chunk_id").is_some())
            .collect::<Vec<_>>();
        assert!(
            chunks
                .iter()
                .any(|chunk| chunk.get("chunk_kind").and_then(Value::as_str)
                    == Some("file_path_title")),
            "partial spool should contain path/title candidates"
        );
        assert!(
            chunks.iter().any(|chunk| {
                chunk.get("selection_bucket").and_then(Value::as_str) == Some("symbol_signature")
                    && chunk
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .contains("spool_target")
            }),
            "partial spool should contain parser-local symbol signature candidates"
        );
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.get("graph_proof").and_then(Value::as_bool) == Some(false)),
            "spool chunks must never be graph proof"
        );
    }

    #[test]
    fn candidate_spool_final_artifact_is_candidate_only_and_superseded_by_db() {
        let repo = candidate_spool_fixture_repo("candidate-spool-final");
        let db = repo.join("artifacts").join("codegraph.sqlite");
        let spool = repo.join("artifacts").join("candidate-spool.jsonl");
        let mut options = fresh_rebuild_options();
        options.candidate_spool_path = Some(spool.clone());
        let summary = index_repo_to_db_with_options(&repo, &db, options).expect("index fixture");
        let spool_summary = summary.candidate_spool.expect("candidate spool summary");
        assert_eq!(
            spool_summary.candidate_spool_status,
            "superseded_by_graph_db"
        );
        assert!(!spool_summary.incomplete);
        assert!(spool_summary.db_passport_hash.is_some());
        let values = read_jsonl_values(&spool);
        let metadata = values[0].get("metadata").expect("manifest metadata");
        assert_eq!(
            metadata
                .get("candidate_spool_status")
                .and_then(Value::as_str),
            Some("superseded_by_graph_db")
        );
        assert_eq!(
            metadata.get("candidate_only").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            metadata.get("graph_proof").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            metadata
                .get("creates_graph_relations")
                .and_then(Value::as_bool),
            Some(false)
        );
        let chunks = values
            .iter()
            .filter(|value| value.get("chunk_id").is_some())
            .collect::<Vec<_>>();
        assert!(chunks.iter().any(|chunk| {
            chunk.get("source_kind").and_then(Value::as_str) == Some("text_evidence")
        }));
        assert!(chunks.iter().all(|chunk| {
            chunk.get("proof_status").and_then(Value::as_str) == Some("candidate_only")
                || chunk.get("proof_status").and_then(Value::as_str) == Some("not_graph_proof")
        }));
        assert!(chunks.iter().all(|chunk| {
            chunk.get("claimable_for_graph").and_then(Value::as_bool) == Some(false)
        }));
    }

    #[test]
    fn candidate_spool_query_index_is_written_and_queryable() {
        let repo = candidate_spool_fixture_repo("candidate-spool-query-index");
        let db = repo.join("artifacts").join("codegraph.sqlite");
        let spool = repo.join("artifacts").join("candidate-spool.jsonl");
        let mut options = fresh_rebuild_options();
        options.candidate_spool_path = Some(spool.clone());
        let summary = index_repo_to_db_with_options(&repo, &db, options).expect("index with spool");
        let spool_summary = summary.candidate_spool.expect("candidate spool summary");
        let index_path = candidate_spool_query_index_path(&spool);
        assert!(index_path.exists(), "query index should be written");
        assert_eq!(spool_summary.query_index_status, "ready");
        assert_eq!(spool_summary.query_index_kind, "sqlite");
        assert!(spool_summary.query_index_bytes > 0);
        assert_eq!(
            spool_summary.query_index_record_count,
            spool_summary.persisted_total_chunks
        );

        let status =
            candidate_spool_index_status_for_repo(&repo, &spool, false).expect("index status");
        assert_eq!(status.query_index_status, "ready");
        assert_eq!(
            status.query_index_record_count,
            spool_summary.persisted_total_chunks
        );

        let file_hits =
            query_candidate_spool_index_for_repo(&repo, &spool, "files", "src/lib.rs", 8, false)
                .expect("file query");
        assert!(
            file_hits
                .chunks
                .iter()
                .any(|chunk| chunk.get("path").and_then(Value::as_str) == Some("src/lib.rs")),
            "file/path lookup should use indexed records"
        );
        let symbol_hits = query_candidate_spool_index_for_repo(
            &repo,
            &spool,
            "symbols",
            "spool_target",
            8,
            false,
        )
        .expect("symbol query");
        assert!(
            symbol_hits.chunks.iter().any(|chunk| {
                chunk
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .contains("spool_target")
            }),
            "symbol lookup should use indexed terms"
        );
        let text_hits = query_candidate_spool_index_for_repo(
            &repo,
            &spool,
            "text",
            "Stage 0 text evidence",
            8,
            false,
        )
        .expect("text query");
        assert!(
            text_hits
                .chunks
                .iter()
                .all(|chunk| chunk.get("graph_proof").and_then(Value::as_bool) == Some(false)),
            "indexed text candidates remain non-proof"
        );
    }

    #[test]
    fn candidate_spool_query_index_lifecycle_detects_changed_deleted_and_renamed_sources() {
        let repo = candidate_spool_fixture_repo("candidate-spool-lifecycle");
        let db = repo.join("artifacts").join("codegraph.sqlite");
        let spool = repo.join("artifacts").join("candidate-spool.jsonl");
        let mut options = fresh_rebuild_options();
        options.candidate_spool_path = Some(spool.clone());
        index_repo_to_db_with_options(&repo, &db, options.clone()).expect("index fixture");

        let ready =
            candidate_spool_index_status_for_repo(&repo, &spool, false).expect("ready spool index");
        assert_eq!(ready.query_index_status, "ready");
        assert!(
            ready.query_index_source_binding_count > 0,
            "query index should keep bounded source bindings for lifecycle checks"
        );

        write_test_file(&repo, "src/lib.rs", "pub fn changed_spool_target() {}\n");
        let changed = candidate_spool_index_status_for_repo(&repo, &spool, true)
            .expect("diagnostic stale status");
        assert!(changed.stale);
        assert_eq!(changed.query_index_status, "stale");
        assert!(changed
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("changed_file"));
        let changed_error = candidate_spool_index_status_for_repo(&repo, &spool, false)
            .expect_err("normal status should reject stale source binding");
        assert!(changed_error.to_string().contains("candidate_spool_stale"));

        fs::remove_file(repo.join("src").join("lib.rs")).expect("delete source");
        let deleted = candidate_spool_index_status_for_repo(&repo, &spool, true)
            .expect("diagnostic deleted status");
        assert!(deleted
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("deleted_file"));

        write_test_file(
            &repo,
            "src/main.rs",
            r#"
pub fn spool_target(value: i32) -> i32 {
    value + 2
}
"#,
        );
        let mut reindex_options = fresh_rebuild_options();
        reindex_options.candidate_spool_path = Some(spool.clone());
        index_repo_to_db_with_options(&repo, &db, reindex_options).expect("reindex renamed file");
        let old_path_hits =
            query_candidate_spool_index_for_repo(&repo, &spool, "files", "src/lib.rs", 8, false)
                .expect("query old path after reindex");
        assert!(
            old_path_hits
                .chunks
                .iter()
                .all(|chunk| chunk.get("path").and_then(Value::as_str) != Some("src/lib.rs")),
            "old renamed path must not remain in regenerated spool"
        );
        let new_path_hits =
            query_candidate_spool_index_for_repo(&repo, &spool, "files", "src/main.rs", 8, false)
                .expect("query new path after reindex");
        assert!(
            !new_path_hits.chunks.is_empty(),
            "new renamed path should be present after regenerated spool"
        );
        assert!(new_path_hits
            .chunks
            .iter()
            .all(|chunk| chunk.get("graph_proof").and_then(Value::as_bool) == Some(false)));
    }

    #[test]
    fn candidate_spool_query_index_open_failure_is_not_corrupt_without_corruption_proof() {
        let repo = candidate_spool_fixture_repo("candidate-spool-query-index-access");
        let db = repo.join("artifacts").join("codegraph.sqlite");
        let spool = repo.join("artifacts").join("candidate-spool.jsonl");
        let mut options = fresh_rebuild_options();
        options.candidate_spool_path = Some(spool.clone());
        index_repo_to_db_with_options(&repo, &db, options).expect("index with spool");

        let index_path = candidate_spool_query_index_path(&spool);
        fs::remove_file(&index_path).expect("remove query index");
        fs::create_dir(&index_path).expect("replace query index with directory");

        let status = candidate_spool_index_status_for_repo(&repo, &spool, true)
            .expect("diagnostic sidecar status");
        assert_ne!(status.query_index_status, "corrupt");
        assert!(matches!(
            status.query_index_status.as_str(),
            "filesystem_inaccessible" | "permission_denied" | "sidecar_unavailable"
        ));
        assert_eq!(status.stale, true);
        assert!(status
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("candidate_spool_query_index_open_failed"));
    }

    #[test]
    fn candidate_spool_query_index_supports_role_and_directory_terms() {
        let repo = candidate_spool_fixture_repo("candidate-spool-query-index-roles");
        let db = repo.join("artifacts").join("codegraph.sqlite");
        let spool = repo.join("artifacts").join("candidate-spool.jsonl");
        let mut options = fresh_rebuild_options();
        options.candidate_spool_path = Some(spool.clone());
        index_repo_to_db_with_options(&repo, &db, options).expect("index with spool");

        let role_hits = query_candidate_spool_index_for_repo(
            &repo,
            &spool,
            "all",
            "symbol_signature",
            16,
            false,
        )
        .expect("role query");
        assert!(
            role_hits.chunks.iter().any(|chunk| {
                chunk.get("candidate_kind").and_then(Value::as_str) == Some("symbol_signature")
            }),
            "candidate_kind lookup should be indexed"
        );

        let dir_hits =
            query_candidate_spool_index_for_repo(&repo, &spool, "all", "docs", 16, false)
                .expect("directory query");
        assert!(
            dir_hits.chunks.iter().any(|chunk| {
                chunk.get("top_level_dir").and_then(Value::as_str) == Some("docs")
            }),
            "top-level directory lookup should be indexed"
        );
    }

    #[test]
    fn candidate_spool_persistence_is_bounded_and_aggregated_before_write() {
        let repo = temp_repo("candidate-spool-bounded-persisted");
        for file_index in 0..20usize {
            let mut source = String::new();
            for symbol_index in 0..30usize {
                source.push_str(&format!(
                    "pub fn bounded_symbol_{file_index}_{symbol_index}(value: i32) -> i32 {{ value + {symbol_index} }}\n"
                ));
            }
            write_test_file(&repo, &format!("src/module_{file_index}.rs"), &source);
        }
        let db = repo.join("artifacts").join("codegraph.sqlite");
        let spool = repo.join("artifacts").join("candidate-spool.jsonl");
        let mut options = fresh_rebuild_options();
        options.candidate_spool_path = Some(spool.clone());
        options.candidate_spool_caps.global_max_records = 24;
        options.candidate_spool_caps.per_file_max_records = 3;
        options.candidate_spool_caps.global_max_bytes = 128 * 1024;
        let summary = index_repo_to_db_with_options(&repo, &db, options).expect("index fixture");
        let spool_summary = summary.candidate_spool.expect("candidate spool summary");
        assert!(spool_summary.generated_total_chunks > spool_summary.persisted_total_chunks);
        assert!(spool_summary.persisted_total_chunks <= 24);
        assert!(
            spool_summary.omitted_by_cap
                + spool_summary.omitted_by_file_limit
                + spool_summary.omitted_by_dir_limit
                + spool_summary.omitted_by_kind_limit
                + spool_summary.omitted_by_budget
                > 0
        );
        let values = read_jsonl_values(&spool);
        let metadata = values[0].get("metadata").expect("manifest metadata");
        assert_eq!(
            metadata.get("record_model").and_then(Value::as_str),
            Some(CANDIDATE_SPOOL_RECORD_MODEL_VERSION)
        );
        assert_eq!(
            metadata
                .get("chunk_selection_strategy")
                .and_then(Value::as_str),
            Some("candidate_spool_bounded_packet_selector_v1")
        );
        let chunks = values
            .iter()
            .filter(|value| value.get("chunk_id").is_some())
            .collect::<Vec<_>>();
        assert_eq!(chunks.len(), spool_summary.persisted_total_chunks);
        assert!(chunks.len() <= 24);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.get("packet_kind").is_some()));
        assert!(chunks
            .iter()
            .all(|chunk| chunk.get("selection_reason").is_none()));
        assert!(chunks.iter().all(|chunk| {
            chunk.get("graph_proof").and_then(Value::as_bool) == Some(false)
                && chunk.get("claimable_for_graph").and_then(Value::as_bool) == Some(false)
                && chunk
                    .get("creates_graph_relations")
                    .and_then(Value::as_bool)
                    == Some(false)
        }));
    }

    fn selector_test_spool_summary(caps: CandidateSpoolCaps) -> CandidateSpoolSummary {
        CandidateSpoolSummary {
            status: "building".to_string(),
            candidate_spool_status: "building".to_string(),
            candidate_spool_path: "candidate-spool.jsonl".to_string(),
            artifact_kind: "candidate_spool".to_string(),
            artifact_format: "jsonl".to_string(),
            lifecycle: "indexing_in_progress".to_string(),
            spooled_total_chunks: 0,
            spooled_by_source_kind: BTreeMap::new(),
            spooled_by_chunk_kind: BTreeMap::new(),
            spooled_bytes: 0,
            spooled_indexing_phase: "test".to_string(),
            candidate_spool_policy: "bounded".to_string(),
            candidate_spool_required: false,
            candidate_spool_disabled_reason: None,
            candidate_spool_warning: None,
            artifact_budget_remaining_bytes: None,
            artifact_budget_decision: None,
            record_model: CANDIDATE_SPOOL_RECORD_MODEL_VERSION.to_string(),
            generated_total_chunks: 0,
            normalized_total_chunks: 0,
            deduped_total_chunks: 0,
            selected_total_chunks: 0,
            persisted_total_chunks: 0,
            omitted_by_cap: 0,
            omitted_by_budget: 0,
            omitted_by_dedup: 0,
            omitted_low_signal: 0,
            omitted_by_file_limit: 0,
            omitted_by_dir_limit: 0,
            omitted_by_kind_limit: 0,
            candidate_spool_truncated: false,
            candidate_spool_partial: false,
            candidate_spool_budget_bytes: caps.global_max_bytes as u64,
            query_index_status: "not_started".to_string(),
            query_index_kind: "none".to_string(),
            query_index_path: None,
            query_index_bytes: 0,
            query_index_record_count: 0,
            query_index_version: String::new(),
            query_index_bound_manifest_hash: None,
            candidate_only: true,
            graph_proof: false,
            incomplete: true,
            db_passport_hash: None,
            db_passport_snapshot: None,
            scope_hash: None,
            repo_hash: None,
            reason: None,
            caps,
            selected_candidate_ids: BTreeSet::new(),
            selected_file_counts: BTreeMap::new(),
            selected_dir_counts: BTreeMap::new(),
            selected_kind_counts: BTreeMap::new(),
            selected_source_counts: BTreeMap::new(),
        }
    }

    fn selector_test_chunk(
        id: usize,
        path: &str,
        chunk_kind: &str,
        source_kind: &str,
        text: &str,
        bucket: &str,
    ) -> Value {
        json!({
            "chunk_id": format!("test-chunk-{id}"),
            "chunk_kind": chunk_kind,
            "source_kind": source_kind,
            "path": path,
            "entity_id": if source_kind == "graph_entity" { json!(format!("entity:{}:{id}", path)) } else { Value::Null },
            "source_span": {
                "start_line": (id % 200) + 1,
                "start_col": 1,
                "end_line": (id % 200) + 1,
                "end_col": 20
            },
            "source_role": "source_navigation",
            "evidence_role": "source_navigation",
            "proof_status": "candidate_only",
            "graph_proof": false,
            "claimable_for_graph": false,
            "text": text,
            "file_kind": candidate_spool_file_kind_from_path(path),
            "source_file_content_hash": format!("hash-{path}"),
            "source_file_size_bytes": 128_u64,
            "source_file_modified_unix_nanos": "1",
            "selection_bucket": bucket
        })
    }

    fn selected_packet_values(result: &CandidateSpoolSelectionResult) -> Vec<&Value> {
        result.selected.iter().map(|packet| &packet.value).collect()
    }

    #[test]
    fn candidate_spool_selector_is_stable_deduped_and_capped() {
        let mut caps = CandidateSpoolCaps::default();
        caps.global_max_records = 6;
        caps.per_file_max_records = 2;
        caps.per_top_level_dir_soft_cap = 4;
        caps.max_snippet_bytes = 64;
        caps.per_candidate_kind_cap
            .insert("symbol_signature".to_string(), 3);
        let spool = selector_test_spool_summary(caps.clone());
        let mut chunks = Vec::new();
        for index in 0..12 {
            chunks.push(selector_test_chunk(
                index,
                "src/lib.rs",
                "function",
                "graph_entity",
                &format!("pub fn selected_symbol_{index}() -> usize"),
                "symbol_signature",
            ));
        }
        chunks.push(chunks[0].clone());
        chunks.push(selector_test_chunk(
            100,
            "docs/README.md",
            "snippet",
            "text_evidence",
            "Important Stage 0 text evidence about selected_symbol_0",
            "text_evidence",
        ));
        let first = select_candidate_spool_packets(chunks.clone(), &caps, &spool);
        let second = select_candidate_spool_packets(chunks, &caps, &spool);
        let first_ids = first
            .selected
            .iter()
            .map(|packet| packet.packet_id.clone())
            .collect::<Vec<_>>();
        let second_ids = second
            .selected
            .iter()
            .map(|packet| packet.packet_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(first_ids, second_ids, "selection order must be stable");
        assert!(first.omitted_by_dedup > 0, "identical input should dedupe");
        assert!(
            first.selected.len() <= caps.global_max_records,
            "global record cap should apply"
        );
        let src_count = first
            .selected
            .iter()
            .filter(|packet| packet.path == "src/lib.rs")
            .count();
        assert!(
            src_count <= caps.per_file_max_records,
            "per-file cap should apply"
        );
        assert!(
            first.omitted_by_file_limit > 0 || first.omitted_by_cap > 0,
            "selector should account for capped records"
        );
        assert_eq!(
            first.generated_count,
            first.normalized_count + first.omitted_low_signal
        );
        assert_eq!(
            first.normalized_count,
            first.deduped_count + first.omitted_by_dedup
        );
    }

    #[test]
    fn candidate_spool_selector_applies_byte_and_snippet_caps_without_verbose_reasons() {
        let mut caps = CandidateSpoolCaps::default();
        caps.global_max_bytes = 1_800;
        caps.global_max_records = 64;
        caps.max_snippet_bytes = 48;
        caps.max_snippets_per_file_packet = 2;
        let spool = selector_test_spool_summary(caps.clone());
        let long_text = "very_long_identifier_token ".repeat(80);
        let chunks = (0..24)
            .map(|index| {
                selector_test_chunk(
                    index,
                    "docs/manual/adding-packages-generic.adoc",
                    "snippet",
                    "text_evidence",
                    &long_text,
                    "text_evidence",
                )
            })
            .collect::<Vec<_>>();
        let result = select_candidate_spool_packets(chunks, &caps, &spool);
        let written_bytes = result
            .selected
            .iter()
            .map(|packet| packet.serialized_bytes)
            .sum::<usize>();
        assert!(written_bytes <= caps.global_max_bytes);
        assert!(
            result.omitted_by_budget > 0 || result.deduped_count > result.selected_count,
            "tight byte budget or aggregation should reduce persisted records"
        );
        for value in selected_packet_values(&result) {
            assert!(
                value.get("selection_reason").is_none(),
                "bounded spool must omit verbose selection reasons"
            );
            if let Some(snippets) = value.get("snippets").and_then(Value::as_array) {
                assert!(snippets.len() <= caps.max_snippets_per_file_packet);
                for snippet in snippets {
                    assert!(
                        snippet
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .len()
                            <= caps.max_snippet_bytes
                    );
                }
            }
        }
    }

    #[test]
    fn candidate_spool_selector_large_synthetic_firehose_stays_bounded() {
        let mut caps = CandidateSpoolCaps::default();
        caps.global_max_records = 2_048;
        caps.global_max_bytes = 2 * 1024 * 1024;
        caps.per_file_max_records = 8;
        caps.per_top_level_dir_soft_cap = 512;
        let spool = selector_test_spool_summary(caps.clone());
        let mut chunks = Vec::with_capacity(200_000);
        for index in 0..200_000usize {
            let file = index % 1_000;
            let path = format!("package/pkg{}/module{}.py", file % 50, file);
            let (chunk_kind, source_kind, bucket, text) = match index % 4 {
                0 => (
                    "file_path_title",
                    "metadata",
                    "file_path_title",
                    path.clone(),
                ),
                1 => (
                    "function",
                    "graph_entity",
                    "symbol_signature",
                    format!("def synthetic_symbol_{index}(value): return value"),
                ),
                2 => (
                    "snippet",
                    "text_evidence",
                    "text_evidence",
                    format!("synthetic evidence token_{index} package configuration"),
                ),
                _ => (
                    "relation_neighborhood",
                    "metadata",
                    "import_signature",
                    format!("import helper_{index} from local source navigation"),
                ),
            };
            chunks.push(selector_test_chunk(
                index,
                &path,
                chunk_kind,
                source_kind,
                &text,
                bucket,
            ));
        }
        let result = select_candidate_spool_packets(chunks, &caps, &spool);
        let written_bytes = result
            .selected
            .iter()
            .map(|packet| packet.serialized_bytes)
            .sum::<usize>();
        assert!(result.selected.len() <= caps.global_max_records);
        assert!(written_bytes <= caps.global_max_bytes);
        let kinds = result
            .selected
            .iter()
            .map(|packet| packet.candidate_kind.as_str())
            .collect::<BTreeSet<_>>();
        assert!(kinds.contains("file_path_title"));
        assert!(kinds.contains("symbol_signature"));
        assert!(kinds.contains("text_evidence_snippet"));
        assert!(kinds.contains("import_export_source_navigation"));
        assert!(
            result.selected.len() < result.generated_count / 20,
            "aggregation and caps should materially reduce firehose records"
        );
    }

    #[test]
    fn candidate_spool_query_index_large_synthetic_queries_are_bounded() {
        let repo = temp_repo("candidate-spool-large-query-index");
        let spool = repo.join("candidate-spool.jsonl");
        let mut caps = CandidateSpoolCaps::default();
        caps.global_max_records = 4_096;
        caps.global_max_bytes = 4 * 1024 * 1024;
        caps.per_file_max_records = 6;
        caps.per_top_level_dir_soft_cap = 768;
        let spool_summary = selector_test_spool_summary(caps.clone());
        let mut chunks = Vec::with_capacity(200_000);
        for index in 0..200_000usize {
            let file = index % 1_200;
            let path = format!("package/pkg{}/module{}.py", file % 80, file);
            let (chunk_kind, source_kind, bucket, text) = match index % 4 {
                0 => (
                    "file_path_title",
                    "metadata",
                    "file_path_title",
                    path.clone(),
                ),
                1 => (
                    "function",
                    "graph_entity",
                    "symbol_signature",
                    format!("def indexed_symbol_{index}(value): return value"),
                ),
                2 => (
                    "snippet",
                    "text_evidence",
                    "text_evidence",
                    format!("indexed evidence token_{index} vector accounting term"),
                ),
                _ => (
                    "relation_neighborhood",
                    "metadata",
                    "import_signature",
                    format!("implementation_trace import helper_{index} local source navigation"),
                ),
            };
            chunks.push(selector_test_chunk(
                index,
                &path,
                chunk_kind,
                source_kind,
                &text,
                bucket,
            ));
        }
        let selection = select_candidate_spool_packets(chunks, &caps, &spool_summary);
        assert!(selection.selected.len() <= caps.global_max_records);
        let written_bytes = selection
            .selected
            .iter()
            .map(|packet| packet.serialized_bytes)
            .sum::<usize>();
        let mut lines = Vec::new();
        lines.push(
            serde_json::to_string(&json!({
                "metadata": {
                    "metadata_version": CANDIDATE_SPOOL_METADATA_VERSION,
                    "artifact_kind": "candidate_spool",
                    "artifact_format": "jsonl",
                    "record_model": CANDIDATE_SPOOL_RECORD_MODEL_VERSION,
                    "repo_root": path_string(&repo),
                    "candidate_spool_status": "partial_ready",
                    "lifecycle": "partial_spool",
                    "incomplete": true,
                    "candidate_only": true,
                    "graph_proof": false,
                    "claimable_for_graph": false,
                    "persisted_total_chunks": selection.selected.len(),
                    "spooled_total_chunks": selection.selected.len()
                }
            }))
            .expect("manifest json"),
        );
        for packet in &selection.selected {
            let mut value = packet.value.clone();
            if let Some(object) = value.as_object_mut() {
                object.remove("source_file_content_hash");
                object.remove("source_file_size_bytes");
                object.remove("source_file_modified_unix_nanos");
            }
            lines.push(serde_json::to_string(&value).expect("packet json"));
        }
        fs::write(&spool, format!("{}\n", lines.join("\n"))).expect("write synthetic spool");
        rebuild_candidate_spool_query_index_for_repo(&repo, &spool).expect("rebuild query index");

        let mut status_durations = Vec::new();
        for _ in 0..12 {
            let started = Instant::now();
            let status =
                candidate_spool_index_status_for_repo(&repo, &spool, true).expect("indexed status");
            assert_eq!(status.query_index_record_count, selection.selected.len());
            status_durations.push(started.elapsed());
        }
        status_durations.sort();
        let status_p95 = status_durations[status_durations.len() * 95 / 100];
        assert!(
            status_p95 < Duration::from_millis(1_000),
            "synthetic indexed spool status p95 was {status_p95:?}"
        );

        let mut query_latency_ms = BTreeMap::new();
        for (subcommand, query) in [
            ("files", "package"),
            ("symbols", "return"),
            ("text", "vector accounting"),
        ] {
            let mut durations = Vec::new();
            for _ in 0..12 {
                let started = Instant::now();
                let result = query_candidate_spool_index_for_repo(
                    &repo, &spool, subcommand, query, 20, true,
                )
                .expect("indexed query");
                assert!(
                    !result.chunks.is_empty(),
                    "{subcommand} query should find bounded indexed candidates"
                );
                durations.push(started.elapsed());
            }
            durations.sort();
            let p95 = durations[durations.len() * 95 / 100];
            assert!(
                p95 < Duration::from_millis(500),
                "synthetic indexed spool {subcommand} query p95 was {p95:?}"
            );
            query_latency_ms.insert(subcommand, p95.as_secs_f64() * 1000.0);
        }
        let query_index_path = candidate_spool_query_index_path(&spool);
        let query_index_bytes = fs::metadata(&query_index_path)
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        println!(
            "candidate_spool_large_synthetic_metrics={}",
            json!({
                "generated_candidate_inputs": selection.generated_count,
                "selected_candidate_packets": selection.selected_count,
                "persisted_candidate_packets": selection.selected.len(),
                "spool_payload_bytes": written_bytes,
                "query_index_bytes": query_index_bytes,
                "omitted_by_budget": selection.omitted_by_budget,
                "omitted_by_cap": selection.omitted_by_cap,
                "omitted_by_dedup": selection.omitted_by_dedup,
                "omitted_by_file_limit": selection.omitted_by_file_limit,
                "omitted_by_dir_limit": selection.omitted_by_dir_limit,
                "omitted_by_kind_limit": selection.omitted_by_kind_limit,
                "path_query_p95_ms": query_latency_ms.get("files").copied().unwrap_or_default(),
                "symbol_query_p95_ms": query_latency_ms.get("symbols").copied().unwrap_or_default(),
                "text_query_p95_ms": query_latency_ms.get("text").copied().unwrap_or_default(),
                "status_p95_ms": status_p95.as_secs_f64() * 1000.0,
                "candidate_only": true,
                "graph_proof": false
            })
        );
    }

    #[test]
    fn candidate_spool_selector_sympy_like_distribution_preserves_diversity() {
        let mut caps = CandidateSpoolCaps::default();
        caps.global_max_records = 800;
        caps.global_max_bytes = 1024 * 1024;
        caps.per_file_max_records = 6;
        caps.per_top_level_dir_soft_cap = 120;
        let spool = selector_test_spool_summary(caps.clone());
        let mut chunks = Vec::new();
        for dir in 0..20usize {
            for file in 0..20usize {
                let path = format!("sympy/area_{dir}/module_{file}.py");
                chunks.push(selector_test_chunk(
                    dir * 10_000 + file,
                    &path,
                    "file_path_title",
                    "metadata",
                    &path,
                    "file_path_title",
                ));
                for symbol in 0..25usize {
                    chunks.push(selector_test_chunk(
                        dir * 10_000 + file * 100 + symbol,
                        &path,
                        "function",
                        "graph_entity",
                        &format!("def sympy_symbol_{dir}_{file}_{symbol}(expr): return expr"),
                        "symbol_signature",
                    ));
                }
            }
        }
        let result = select_candidate_spool_packets(chunks, &caps, &spool);
        assert!(result.selected.len() <= caps.global_max_records);
        let mut by_file = BTreeMap::new();
        let mut by_dir = BTreeMap::new();
        for packet in &result.selected {
            *by_file.entry(packet.path.clone()).or_insert(0usize) += 1;
            *by_dir.entry(packet.top_level_dir.clone()).or_insert(0usize) += 1;
        }
        assert!(by_file
            .values()
            .all(|count| *count <= caps.per_file_max_records));
        assert!(by_dir
            .values()
            .all(|count| *count <= caps.per_top_level_dir_soft_cap));
    }

    #[test]
    fn candidate_spool_selector_buildroot_roles_survive() {
        let mut caps = CandidateSpoolCaps::default();
        caps.global_max_records = 64;
        caps.global_max_bytes = 128 * 1024;
        let spool = selector_test_spool_summary(caps.clone());
        let chunks = vec![
            selector_test_chunk(
                1,
                "package/foo/foo.mk",
                "file_path_title",
                "metadata",
                "package/foo/foo.mk generic-package download site",
                "file_path_title",
            ),
            selector_test_chunk(
                2,
                "package/Config.in",
                "snippet",
                "text_evidence",
                "source package/foo/Config.in menu entry",
                "text_evidence",
            ),
            selector_test_chunk(
                3,
                "docs/manual/adding-packages-generic.adoc",
                "snippet",
                "text_evidence",
                "generic-package documentation for package metadata",
                "text_evidence",
            ),
            selector_test_chunk(
                4,
                "support/download/dl-wrapper",
                "relation_neighborhood",
                "metadata",
                "support download wrapper helper source navigation",
                "import_signature",
            ),
            selector_test_chunk(
                5,
                "package/foo/src/foo.c",
                "function",
                "graph_entity",
                "int foo_main(void) { return 0; }",
                "symbol_signature",
            ),
        ];
        let result = select_candidate_spool_packets(chunks, &caps, &spool);
        let joined = result
            .selected
            .iter()
            .map(|packet| {
                format!(
                    "{} {}",
                    packet.path,
                    packet
                        .value
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        for needle in [
            "package/foo/foo.mk",
            "package/Config.in",
            "adding-packages-generic.adoc",
            "support/download/dl-wrapper",
            "foo_main",
        ] {
            assert!(joined.contains(needle), "missing Buildroot role: {needle}");
        }
    }

    #[test]
    fn candidate_spool_selector_outputs_candidate_only_packets() {
        let caps = CandidateSpoolCaps::default();
        let spool = selector_test_spool_summary(caps.clone());
        let chunks = vec![
            selector_test_chunk(
                1,
                "src/lib.rs",
                "function",
                "graph_entity",
                "pub fn claim_boundary() {}",
                "symbol_signature",
            ),
            selector_test_chunk(
                2,
                "src/lib.rs",
                "relation_neighborhood",
                "metadata",
                "CALLS helper in src/lib.rs near 1:1",
                "import_signature",
            ),
        ];
        let result = select_candidate_spool_packets(chunks, &caps, &spool);
        assert!(!result.selected.is_empty());
        for value in selected_packet_values(&result) {
            assert_eq!(
                value.get("proof_status").and_then(Value::as_str),
                Some("candidate_only")
            );
            assert_eq!(
                value.get("graph_proof").and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(
                value.get("claimable_for_graph").and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(
                value
                    .get("creates_graph_relations")
                    .and_then(Value::as_bool),
                Some(false)
            );
            assert!(
                !value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .contains(&"full source body ".repeat(20)),
                "bounded spool should not store full source bodies"
            );
        }
    }

    fn init_git_repo_for_identity_test(root: &Path) -> String {
        let init = Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("init")
            .output()
            .expect("run git init");
        assert!(
            init.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        for (key, value) in [
            ("user.email", "codegraph-tests@example.invalid"),
            ("user.name", "CodeGraph Tests"),
        ] {
            let output = Command::new("git")
                .arg("-C")
                .arg(root)
                .args(["config", key, value])
                .output()
                .expect("run git config");
            assert!(
                output.status.success(),
                "git config {key} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let add = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["add", "."])
            .output()
            .expect("run git add");
        assert!(
            add.status.success(),
            "git add failed: {}",
            String::from_utf8_lossy(&add.stderr)
        );
        let commit = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["commit", "-m", "identity-test"])
            .output()
            .expect("run git commit");
        assert!(
            commit.status.success(),
            "git commit failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        );
        git_head(root).expect("git repo head")
    }

    struct WritePathChaosFailpointGuard;

    impl Drop for WritePathChaosFailpointGuard {
        fn drop(&mut self) {
            WRITE_PATH_CHAOS_FAILPOINT_OVERRIDE.with(|override_cell| {
                *override_cell.borrow_mut() = None;
            });
        }
    }

    fn with_write_path_chaos_failpoint<T>(failpoint: &str, action: impl FnOnce() -> T) -> T {
        WRITE_PATH_CHAOS_FAILPOINT_OVERRIDE.with(|override_cell| {
            *override_cell.borrow_mut() = Some(failpoint.to_string());
        });
        let _guard = WritePathChaosFailpointGuard;
        action()
    }

    struct RtdsClosureOverrideGuard;

    impl Drop for RtdsClosureOverrideGuard {
        fn drop(&mut self) {
            RTDS_CLOSURE_FAILPOINT_OVERRIDE.with(|override_cell| {
                *override_cell.borrow_mut() = None;
            });
            RTDS_CLOSURE_BUDGET_OVERRIDE.with(|override_cell| {
                *override_cell.borrow_mut() = None;
            });
        }
    }

    fn with_rtds_closure_budget<T>(
        budget: RtdsDependencyClosureBudget,
        action: impl FnOnce() -> T,
    ) -> T {
        RTDS_CLOSURE_BUDGET_OVERRIDE.with(|override_cell| {
            *override_cell.borrow_mut() = Some(budget);
        });
        let _guard = RtdsClosureOverrideGuard;
        action()
    }

    fn with_rtds_closure_failpoint<T>(failpoint: &str, action: impl FnOnce() -> T) -> T {
        RTDS_CLOSURE_FAILPOINT_OVERRIDE.with(|override_cell| {
            *override_cell.borrow_mut() = Some(failpoint.to_string());
        });
        let _guard = RtdsClosureOverrideGuard;
        action()
    }

    fn fresh_rebuild_options() -> IndexOptions {
        let mut options = IndexOptions::default();
        options.db_lifecycle.explicit_db_path = true;
        options.db_lifecycle.policy = DbLifecyclePolicy::FreshRebuild;
        options
    }

    fn vector_test_passport(scope_hash: &str) -> DbPassport {
        DbPassport {
            passport_version: DB_PASSPORT_VERSION,
            codegraph_schema_version: SCHEMA_VERSION,
            storage_mode: DEFAULT_STORAGE_POLICY.to_string(),
            index_scope_policy_hash: scope_hash.to_string(),
            scope_policy_json: format!("{{\"scope\":\"{scope_hash}\"}}"),
            canonical_repo_root: "C:/tmp/codegraph-vector-index-test".to_string(),
            git_remote: None,
            worktree_root: None,
            repo_head: Some("test-head".to_string()),
            source_discovery_policy_version: "test-policy-v1".to_string(),
            codegraph_build_version: Some("test-build".to_string()),
            last_successful_index_timestamp: Some(1),
            last_completed_run_id: Some("test-run".to_string()),
            last_run_status: "completed".to_string(),
            integrity_gate_result: "passed".to_string(),
            files_seen: 3,
            files_indexed: 3,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        }
    }

    fn collected_rel_paths(root: &Path, options: &IndexScopeOptions) -> BTreeSet<String> {
        collect_repo_files_with_scope(root, options)
            .expect("collect files")
            .files
            .into_iter()
            .map(|path| repo_relative_path(root, &path).expect("relative path"))
            .collect()
    }

    fn directory_prune_decision<'a>(
        report: &'a IndexScopeRuntimeReport,
        directory: &str,
    ) -> &'a scope::IndexScopeDirectoryPruneDecision {
        report
            .directory_prune_decisions
            .iter()
            .find(|decision| decision.directory == directory)
            .unwrap_or_else(|| panic!("missing directory prune decision for {directory}"))
    }

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf()
    }

    fn generated_junk_fixture() -> PathBuf {
        workspace_root()
            .join("fixtures")
            .join("index_scope")
            .join("generated_junk_repo")
    }

    fn indexed_file_paths(db: &Path) -> BTreeSet<String> {
        let store = SqliteGraphStore::open(db).expect("open indexed fixture DB");
        store
            .list_files(UNBOUNDED_STORE_READ_LIMIT)
            .expect("list indexed files")
            .into_iter()
            .map(|record| normalize_graph_path(&record.repo_relative_path))
            .collect()
    }

    fn expected_generated_junk_source_paths() -> BTreeSet<String> {
        BTreeSet::from([
            "docs/example.ts".to_string(),
            "examples/demo.ts".to_string(),
            "fixtures/local_fixture.ts".to_string(),
            "src/main.ts".to_string(),
            "tests/scope.test.ts".to_string(),
        ])
    }

    #[test]
    fn db_lifecycle_scope_policy_hash_is_deterministic() {
        let default_hash = scope_policy_hash(&IndexScopeOptions::default()).expect("hash");
        assert_eq!(
            default_hash,
            scope_policy_hash(&IndexScopeOptions::default()).expect("hash")
        );

        let changed_hash = scope_policy_hash(&IndexScopeOptions {
            exclude_patterns: vec!["generated/**".to_string()],
            ..IndexScopeOptions::default()
        })
        .expect("changed hash");
        assert_ne!(default_hash, changed_hash);
    }

    #[test]
    fn passport_non_default_scope_read_preflight_uses_stored_scope_policy() {
        let repo = temp_repo("passport-non-default-scope-read");
        write_test_file(&repo, ".gitignore", "ignored.ts\n");
        write_test_file(
            &repo,
            "ignored.ts",
            "export function ignored_scope_symbol() { return 1; }\n",
        );
        let db = repo.join(".codegraph").join("codegraph.sqlite");
        let mut options = IndexOptions::default();
        options.scope.include_ignored = true;

        index_repo_to_db_with_options(&repo, &db, options.clone())
            .expect("index with non-default include-ignored scope");

        let stored_scope_preflight =
            inspect_repo_db_passport(&repo, &db, &options).expect("stored-scope preflight");
        assert!(stored_scope_preflight.valid, "{stored_scope_preflight:?}");
        let passport = stored_scope_preflight.passport.expect("passport");
        assert_eq!(
            passport.index_scope_policy_hash,
            scope_policy_hash(&options.scope).expect("non-default scope hash")
        );
        assert_ne!(
            passport.index_scope_policy_hash,
            scope_policy_hash(&IndexScopeOptions::default()).expect("default scope hash")
        );

        let read_preflight =
            inspect_db_lifecycle_preflight(&repo, &db, None).expect("read preflight");
        assert!(
            read_preflight.safe,
            "read paths should derive scope_source=passport instead of rejecting a safe DB with the wrong default-scope question: {read_preflight:?}"
        );
        assert_eq!(read_preflight.scope_source, "passport");

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn db_lifecycle_explicit_incompatible_scope_reports_scope_mismatch() {
        let repo = temp_repo("passport-incompatible-scope");
        write_test_file(&repo, ".gitignore", "ignored.ts\n");
        write_test_file(
            &repo,
            "ignored.ts",
            "export function included_by_scope() { return 1; }\n",
        );
        let db = repo.join(".codegraph").join("codegraph.sqlite");
        let mut stored_options = IndexOptions::default();
        stored_options.scope.include_ignored = true;
        index_repo_to_db_with_options(&repo, &db, stored_options)
            .expect("index with non-default scope");

        let incompatible_scope = IndexScopeOptions {
            exclude_patterns: vec!["ignored.ts".to_string()],
            ..IndexScopeOptions::default()
        };
        let preflight = inspect_db_lifecycle_preflight(&repo, &db, Some(incompatible_scope))
            .expect("incompatible lifecycle preflight");

        assert!(!preflight.safe, "{preflight:?}");
        assert_eq!(preflight.scope_status, "mismatched");
        let mismatch = preflight.scope_mismatch.expect("scope mismatch details");
        assert_eq!(
            mismatch.message,
            "DB passport scope does not match explicit requested scope"
        );
        assert!(mismatch.expected_scope_hash.is_some(), "{mismatch:?}");
        assert!(mismatch.observed_scope_hash.is_some(), "{mismatch:?}");
        assert_ne!(mismatch.expected_scope_hash, mismatch.observed_scope_hash);

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    fn surface_preflight_request(
        repo_root: &Path,
        db_path: &Path,
        operation_kind: DbLifecycleOperationKind,
    ) -> DbLifecycleSurfacePreflightRequest {
        DbLifecycleSurfacePreflightRequest {
            repo_root: repo_root.to_path_buf(),
            db_path: db_path.to_path_buf(),
            surface_name: "test.surface".to_string(),
            operation_kind,
            allow_stale_read: false,
            allow_foreign_repo: false,
            required_storage_mode: Some(StorageMode::Proof),
            expected_scope: None,
        }
    }

    #[test]
    fn lifecycle_surface_preflight_valid_db_normal_read_passes_exact_path() {
        let repo = temp_repo("surface-valid-normal-read");
        write_test_file(
            &repo,
            "src/main.ts",
            "export function exact_path_checked() { return 1; }\n",
        );
        let db = repo.join("custom-artifact.sqlite");
        index_repo_to_db_with_options(&repo, &db, IndexOptions::default())
            .expect("index custom artifact");

        let preflight = inspect_db_lifecycle_surface_preflight(surface_preflight_request(
            &repo,
            &db,
            DbLifecycleOperationKind::NormalRead,
        ))
        .expect("surface preflight");

        assert!(preflight.safe_to_read, "{preflight:?}");
        assert!(!preflight.safe_to_write, "{preflight:?}");
        assert!(preflight.claimable, "{preflight:?}");
        assert!(!preflight.diagnostic_only, "{preflight:?}");
        assert_eq!(preflight.passport_status, "valid");
        assert!(preflight.repo_match, "{preflight:?}");
        assert!(preflight.scope_match, "{preflight:?}");
        assert_eq!(preflight.schema_status, "ok");
        assert!(preflight.storage_mode_match, "{preflight:?}");
        assert_eq!(preflight.exact_db_path_checked, db.display().to_string());
        assert_eq!(preflight.artifact_freshness.as_deref(), Some("fresh"));

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn lifecycle_surface_preflight_missing_passport_normal_read_fails() {
        let repo = temp_repo("surface-missing-passport");
        let db = repo.join("missing-passport.sqlite");
        drop(SqliteGraphStore::open(&db).expect("create schema without passport"));

        let preflight = inspect_db_lifecycle_surface_preflight(surface_preflight_request(
            &repo,
            &db,
            DbLifecycleOperationKind::NormalRead,
        ))
        .expect("surface preflight");

        assert!(!preflight.safe_to_read, "{preflight:?}");
        assert!(!preflight.safe_to_write, "{preflight:?}");
        assert!(!preflight.claimable, "{preflight:?}");
        assert_eq!(preflight.passport_status, "missing");
        assert_eq!(
            preflight.db_problem_kind.as_deref(),
            Some("passport_missing")
        );
        assert_eq!(preflight.path_access_status, "ok");
        assert!(!preflight.repo_match, "{preflight:?}");
        assert_eq!(
            preflight.artifact_freshness.as_deref(),
            Some("passport_missing")
        );
        assert!(
            preflight
                .blockers
                .iter()
                .any(|blocker| blocker.contains("codegraph_db_passport")),
            "{preflight:?}"
        );

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn lifecycle_surface_preflight_mismatched_repo_normal_read_fails_closed() {
        let repo_a = temp_repo("surface-repo-a");
        let repo_b = temp_repo("surface-repo-b");
        write_test_file(&repo_a, "src/a.ts", "export const repo_a = 1;\n");
        write_test_file(&repo_b, "src/b.ts", "export const repo_b = 1;\n");
        let db = repo_a.join("repo-a.sqlite");
        index_repo_to_db_with_options(&repo_a, &db, IndexOptions::default()).expect("index repo A");

        let mut request =
            surface_preflight_request(&repo_b, &db, DbLifecycleOperationKind::NormalRead);
        request.allow_foreign_repo = true;
        let preflight = inspect_db_lifecycle_surface_preflight(request).expect("surface preflight");

        assert!(!preflight.safe_to_read, "{preflight:?}");
        assert!(!preflight.claimable, "{preflight:?}");
        assert!(!preflight.repo_match, "{preflight:?}");
        assert_eq!(
            preflight.db_problem_kind.as_deref(),
            Some("repo_root_mismatch")
        );
        assert!(
            preflight
                .blockers
                .iter()
                .any(|blocker| blocker.contains("repo root mismatch")),
            "{preflight:?}"
        );

        fs::remove_dir_all(repo_a).expect("cleanup repo A");
        fs::remove_dir_all(repo_b).expect("cleanup repo B");
    }

    #[test]
    fn lifecycle_surface_preflight_reports_external_profile_db_path() {
        let repo = temp_repo("surface-external-profile-db");
        write_test_file(
            &repo,
            "src/main.ts",
            "export function external_profile_db() { return 1; }\n",
        );
        let external_db = std::env::temp_dir().join(format!(
            "codegraph-index-external-profile-{}-{}.sqlite",
            process::id(),
            unix_time_ms()
        ));
        index_repo_to_db_with_options(&repo, &external_db, IndexOptions::default())
            .expect("index external profile DB");

        let preflight = inspect_db_lifecycle_surface_preflight(surface_preflight_request(
            &repo,
            &external_db,
            DbLifecycleOperationKind::NormalRead,
        ))
        .expect("surface preflight");

        assert!(preflight.safe_to_read, "{preflight:?}");
        assert!(preflight.db_path_outside_workspace, "{preflight:?}");
        assert_eq!(
            preflight.outside_workspace_note.as_deref(),
            Some(EXTERNAL_PROFILE_DB_NOTE)
        );
        assert_eq!(preflight.path_access_status, "ok");

        let _ = fs::remove_file(&external_db);
        let _ = fs::remove_file(format!("{}-wal", external_db.display()));
        let _ = fs::remove_file(format!("{}-shm", external_db.display()));
        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn lifecycle_surface_preflight_mismatched_repo_diagnostic_read_is_nonclaimable() {
        let repo_a = temp_repo("surface-diagnostic-repo-a");
        let repo_b = temp_repo("surface-diagnostic-repo-b");
        write_test_file(&repo_a, "src/a.ts", "export const repo_a = 1;\n");
        write_test_file(&repo_b, "src/b.ts", "export const repo_b = 1;\n");
        let db = repo_a.join("repo-a.sqlite");
        index_repo_to_db_with_options(&repo_a, &db, IndexOptions::default()).expect("index repo A");

        let blocked = inspect_db_lifecycle_surface_preflight(surface_preflight_request(
            &repo_b,
            &db,
            DbLifecycleOperationKind::DiagnosticRead,
        ))
        .expect("blocked diagnostic preflight");
        assert!(!blocked.safe_to_read, "{blocked:?}");
        assert!(!blocked.claimable, "{blocked:?}");

        let mut request =
            surface_preflight_request(&repo_b, &db, DbLifecycleOperationKind::DiagnosticRead);
        request.allow_foreign_repo = true;
        let allowed =
            inspect_db_lifecycle_surface_preflight(request).expect("allowed diagnostic preflight");

        assert!(allowed.safe_to_read, "{allowed:?}");
        assert!(!allowed.safe_to_write, "{allowed:?}");
        assert!(allowed.diagnostic_only, "{allowed:?}");
        assert!(!allowed.claimable, "{allowed:?}");
        assert!(!allowed.repo_match, "{allowed:?}");
        assert!(allowed.blockers.is_empty(), "{allowed:?}");
        assert!(
            allowed
                .warnings
                .iter()
                .any(|warning| warning.contains("foreign-repo blocker")),
            "{allowed:?}"
        );

        fs::remove_dir_all(repo_a).expect("cleanup repo A");
        fs::remove_dir_all(repo_b).expect("cleanup repo B");
    }

    #[test]
    fn lifecycle_surface_preflight_repo_head_mismatch_blocks_claimable_reads() {
        let repo = temp_repo("surface-repo-head-mismatch");
        write_test_file(
            &repo,
            "src/main.ts",
            "export function current_head_symbol() { return 1; }\n",
        );
        let current_head = init_git_repo_for_identity_test(&repo);
        let db = repo.join("repo-head.sqlite");
        index_repo_to_db_with_options(&repo, &db, IndexOptions::default()).expect("index repo");
        {
            let store = SqliteGraphStore::open(&db).expect("open store");
            let mut passport = store
                .get_db_passport()
                .expect("read passport")
                .expect("passport");
            assert_eq!(passport.repo_head.as_deref(), Some(current_head.as_str()));
            passport.repo_head = Some("stale-head-for-test".to_string());
            store
                .upsert_db_passport(&passport)
                .expect("write stale head");
        }

        let normal = inspect_db_lifecycle_surface_preflight(surface_preflight_request(
            &repo,
            &db,
            DbLifecycleOperationKind::NormalRead,
        ))
        .expect("normal preflight");
        assert!(!normal.safe_to_read, "{normal:?}");
        assert!(!normal.claimable, "{normal:?}");
        assert_eq!(
            normal.db_problem_kind.as_deref(),
            Some("repo_head_mismatch")
        );
        assert_eq!(
            normal.artifact_freshness.as_deref(),
            Some("repo_head_mismatch")
        );
        assert!(normal
            .blockers
            .iter()
            .any(|blocker| blocker.contains("repo head mismatch")));

        let blocked_diagnostic = inspect_db_lifecycle_surface_preflight(surface_preflight_request(
            &repo,
            &db,
            DbLifecycleOperationKind::DiagnosticRead,
        ))
        .expect("blocked diagnostic");
        assert!(!blocked_diagnostic.safe_to_read, "{blocked_diagnostic:?}");
        assert!(!blocked_diagnostic.claimable, "{blocked_diagnostic:?}");

        let mut allowed =
            surface_preflight_request(&repo, &db, DbLifecycleOperationKind::DiagnosticRead);
        allowed.allow_stale_read = true;
        let allowed_diagnostic =
            inspect_db_lifecycle_surface_preflight(allowed).expect("allowed diagnostic");
        assert!(allowed_diagnostic.safe_to_read, "{allowed_diagnostic:?}");
        assert!(allowed_diagnostic.diagnostic_only, "{allowed_diagnostic:?}");
        assert!(!allowed_diagnostic.claimable, "{allowed_diagnostic:?}");
        assert!(
            allowed_diagnostic.blockers.is_empty(),
            "{allowed_diagnostic:?}"
        );
        assert!(allowed_diagnostic
            .warnings
            .iter()
            .any(|warning| warning.contains("stale/passport blocker")));

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn lifecycle_surface_preflight_write_update_is_stricter_than_diagnostic_read() {
        let repo_a = temp_repo("surface-write-repo-a");
        let repo_b = temp_repo("surface-write-repo-b");
        write_test_file(&repo_a, "src/a.ts", "export const repo_a = 1;\n");
        write_test_file(&repo_b, "src/b.ts", "export const repo_b = 1;\n");
        let db = repo_a.join("repo-a.sqlite");
        index_repo_to_db_with_options(&repo_a, &db, IndexOptions::default()).expect("index repo A");

        let mut diagnostic =
            surface_preflight_request(&repo_b, &db, DbLifecycleOperationKind::DiagnosticRead);
        diagnostic.allow_foreign_repo = true;
        assert!(
            inspect_db_lifecycle_surface_preflight(diagnostic)
                .expect("diagnostic")
                .safe_to_read
        );

        let mut write =
            surface_preflight_request(&repo_b, &db, DbLifecycleOperationKind::WriteUpdate);
        write.allow_foreign_repo = true;
        write.allow_stale_read = true;
        let write_preflight =
            inspect_db_lifecycle_surface_preflight(write).expect("write preflight");

        assert!(!write_preflight.safe_to_write, "{write_preflight:?}");
        assert!(!write_preflight.safe_to_read, "{write_preflight:?}");
        assert!(!write_preflight.claimable, "{write_preflight:?}");
        assert!(
            write_preflight
                .blockers
                .iter()
                .any(|blocker| blocker.contains("repo root mismatch")),
            "{write_preflight:?}"
        );

        fs::remove_dir_all(repo_a).expect("cleanup repo A");
        fs::remove_dir_all(repo_b).expect("cleanup repo B");
    }

    #[test]
    fn db_lifecycle_fresh_index_writes_passport_and_warm_reuses() {
        let repo = temp_repo("passport-fresh-warm");
        write_test_file(
            &repo,
            "src/main.ts",
            "export function lifecycle_target() { return 1; }\n",
        );
        let db = repo.join(".codegraph").join("codegraph.sqlite");

        let cold =
            index_repo_to_db_with_options(&repo, &db, IndexOptions::default()).expect("cold index");
        let cold_lifecycle = cold.db_lifecycle.as_ref().expect("cold lifecycle");
        assert_eq!(cold_lifecycle.decision, "fresh_rebuild");
        assert!(cold_lifecycle.claimable);

        let preflight =
            inspect_repo_db_passport(&repo, &db, &IndexOptions::default()).expect("preflight");
        assert!(preflight.valid, "{preflight:?}");
        assert_eq!(preflight.passport_status, "valid");
        let passport = preflight.passport.expect("passport");
        assert_eq!(passport.last_run_status, "completed");
        assert_eq!(passport.integrity_gate_result, "ok");
        assert_eq!(passport.files_indexed, 1);

        let warm =
            index_repo_to_db_with_options(&repo, &db, IndexOptions::default()).expect("warm index");
        let warm_lifecycle = warm.db_lifecycle.as_ref().expect("warm lifecycle");
        assert_eq!(warm_lifecycle.decision, "incremental_reuse");
        assert!(warm_lifecycle.old_db_used);
        assert_eq!(warm.files_metadata_unchanged, 1);

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn db_lifecycle_explicit_corrupt_db_fails_unless_fresh() {
        let repo = temp_repo("passport-explicit-corrupt");
        write_test_file(&repo, "src/main.ts", "export const value = 1;\n");
        let db = repo.join("named-artifact.sqlite");
        fs::write(&db, "not sqlite").expect("write corrupt DB");

        let mut explicit = IndexOptions::default();
        explicit.db_lifecycle.explicit_db_path = true;
        let error = index_repo_to_db_with_options(&repo, &db, explicit)
            .expect_err("explicit corrupt DB should fail");
        assert!(
            error.to_string().contains("DB lifecycle preflight failed"),
            "{error}"
        );
        assert_eq!(
            fs::read_to_string(&db).expect("old corrupt DB preserved"),
            "not sqlite"
        );

        let mut fresh = IndexOptions::default();
        fresh.db_lifecycle.explicit_db_path = true;
        fresh.db_lifecycle.policy = DbLifecyclePolicy::FreshRebuild;
        let summary =
            index_repo_to_db_with_options(&repo, &db, fresh).expect("--fresh explicit DB rebuilds");
        assert_eq!(
            summary
                .db_lifecycle
                .as_ref()
                .map(|lifecycle| lifecycle.decision.as_str()),
            Some("fresh_rebuild")
        );
        assert!(
            inspect_repo_db_passport(&repo, &db, &IndexOptions::default())
                .expect("preflight")
                .valid
        );

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn db_lifecycle_repo_mismatch_default_rebuilds() {
        let repo_a = temp_repo("passport-repo-a");
        let repo_b = temp_repo("passport-repo-b");
        write_test_file(&repo_a, "src/a.ts", "export const repo_a = 1;\n");
        write_test_file(&repo_b, "src/b.ts", "export const repo_b = 2;\n");
        let shared_db = repo_a.join(".codegraph").join("codegraph.sqlite");

        index_repo_to_db_with_options(&repo_a, &shared_db, IndexOptions::default())
            .expect("index repo A");
        let summary_b = index_repo_to_db_with_options(&repo_b, &shared_db, IndexOptions::default())
            .expect("default repo mismatch rebuilds");
        let lifecycle = summary_b.db_lifecycle.as_ref().expect("lifecycle");
        assert_eq!(lifecycle.decision, "fresh_rebuild");
        assert!(lifecycle.old_db_replaced);
        assert!(
            lifecycle
                .reasons
                .iter()
                .any(|reason| reason.contains("repo root mismatch")),
            "{lifecycle:?}"
        );
        let preflight_b = inspect_repo_db_passport(&repo_b, &shared_db, &IndexOptions::default())
            .expect("repo B preflight");
        assert!(preflight_b.valid, "{preflight_b:?}");

        fs::remove_dir_all(repo_a).expect("cleanup repo A");
        fs::remove_dir_all(repo_b).expect("cleanup repo B");
    }

    #[test]
    fn generated_junk_fixture_default_scope_excludes_artifacts() {
        let repo = generated_junk_fixture();
        let files = collected_rel_paths(&repo, &IndexScopeOptions::default());

        for expected in expected_generated_junk_source_paths() {
            assert!(files.contains(&expected), "{expected} should stay included");
        }

        for excluded in [
            "target/debug/fake.rs",
            "node_modules/pkg/index.js",
            "dist/bundle.js",
            "build/generated.py",
            "reports/final/fake_report.rs",
            ".venv/lib/fake.py",
            "__pycache__/fake.py",
            ".cache/fake.js",
            "artifacts/fake.db",
        ] {
            assert!(
                !files.contains(excluded),
                "{excluded} must not be collected by default"
            );
        }
    }

    #[test]
    fn generated_junk_fixture_overrides_and_gitignore_are_explicit() {
        let repo = generated_junk_fixture();

        let included = collected_rel_paths(
            &repo,
            &IndexScopeOptions {
                include_patterns: vec!["target/debug/fake.rs".to_string()],
                ..IndexScopeOptions::default()
            },
        );
        assert!(included.contains("target/debug/fake.rs"));
        assert!(!included.contains("node_modules/pkg/index.js"));

        let no_default = collected_rel_paths(
            &repo,
            &IndexScopeOptions {
                no_default_excludes: true,
                ..IndexScopeOptions::default()
            },
        );
        for normally_excluded in [
            "target/debug/fake.rs",
            "node_modules/pkg/index.js",
            "dist/bundle.js",
            "build/generated.py",
            "reports/final/fake_report.rs",
            ".venv/lib/fake.py",
            "__pycache__/fake.py",
            ".cache/fake.js",
            "artifacts/fake.db",
        ] {
            assert!(
                no_default.contains(normally_excluded),
                "{normally_excluded} should be included with --no-default-excludes"
            );
        }

        let windows_decision = scope::IndexScope::new(IndexScopeOptions::default()).evaluate_path(
            r"target\debug\fake.rs",
            scope::ScopePathKind::File,
            false,
        );
        assert_eq!(windows_decision.normalized_path, "target/debug/fake.rs");
        assert_eq!(windows_decision.action, scope::ScopeAction::WouldExclude);
    }

    #[test]
    fn scope_include_pattern_does_not_descend_unrelated_excluded_directories() {
        let repo = temp_repo("scope-include-pruning");
        write_test_file(
            &repo,
            "src/keep.ts",
            "export function only_included_path() { return 1; }\n",
        );
        write_test_file(&repo, ".gitignore", "dist/\n");
        write_test_file(
            &repo,
            "target/debug/noisy.ts",
            "export function target_noise() { return 1; }\n",
        );
        write_test_file(
            &repo,
            "node_modules/pkg/noisy.ts",
            "export function node_noise() { return 1; }\n",
        );
        write_test_file(
            &repo,
            "dist/noisy.ts",
            "export function dist_noise() { return 1; }\n",
        );
        write_test_file(
            &repo,
            ".git/objects/noisy.ts",
            "export function git_noise() { return 1; }\n",
        );

        let scoped = collect_repo_files_with_scope(
            &repo,
            &IndexScopeOptions {
                include_patterns: vec!["src/keep.ts".to_string()],
                ..IndexScopeOptions::default()
            },
        )
        .expect("collect scoped files");
        let files = scoped
            .files
            .iter()
            .map(|path| repo_relative_path(&repo, path).expect("relative"))
            .collect::<BTreeSet<_>>();
        let source_files = files
            .iter()
            .filter(|path| path.ends_with(".ts"))
            .cloned()
            .collect::<BTreeSet<_>>();

        assert_eq!(source_files, BTreeSet::from(["src/keep.ts".to_string()]));
        for unrelated_excluded_file in [
            "target/debug/noisy.ts",
            "node_modules/pkg/noisy.ts",
            "dist/noisy.ts",
            ".git/objects/noisy.ts",
        ] {
            assert!(
                !scoped.scope_report.excluded_examples.iter().any(|decision| {
                    decision.path_kind == ScopePathKind::File
                        && decision.normalized_path == unrelated_excluded_file
                }),
                "{unrelated_excluded_file} should remain pruned, not merely excluded after traversal: {:?}",
                scoped.scope_report.excluded_examples
            );
        }
        for unrelated_excluded_dir in ["target", "node_modules", "dist", ".git"] {
            let decision = directory_prune_decision(&scoped.scope_report, unrelated_excluded_dir);
            assert!(decision.pruned, "{decision:?}");
            assert!(!decision.could_include_descendant, "{decision:?}");
            assert!(decision.include_patterns_present, "{decision:?}");
        }

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn scope_include_pattern_descends_only_needed_node_modules_subtree() {
        let repo = temp_repo("scope-include-node-modules");
        write_test_file(
            &repo,
            "src/main.ts",
            "export function app() { return 1; }\n",
        );
        write_test_file(
            &repo,
            "node_modules/pkg/file.js",
            "export function explicit_dependency() { return 1; }\n",
        );
        write_test_file(
            &repo,
            "node_modules/other/noisy.js",
            "export function unrelated_dependency() { return 1; }\n",
        );

        let scoped = collect_repo_files_with_scope(
            &repo,
            &IndexScopeOptions {
                include_patterns: vec!["node_modules/pkg/file.js".to_string()],
                ..IndexScopeOptions::default()
            },
        )
        .expect("collect scoped files");
        let files = scoped
            .files
            .iter()
            .map(|path| repo_relative_path(&repo, path).expect("relative"))
            .collect::<BTreeSet<_>>();

        assert!(files.contains("node_modules/pkg/file.js"));
        assert!(!files.contains("node_modules/other/noisy.js"));
        let node_modules = directory_prune_decision(&scoped.scope_report, "node_modules");
        assert!(!node_modules.pruned, "{node_modules:?}");
        assert!(node_modules.could_include_descendant, "{node_modules:?}");
        let pkg = directory_prune_decision(&scoped.scope_report, "node_modules/pkg");
        assert!(!pkg.pruned, "{pkg:?}");
        assert!(pkg.could_include_descendant, "{pkg:?}");
        let other = directory_prune_decision(&scoped.scope_report, "node_modules/other");
        assert!(other.pruned, "{other:?}");
        assert!(!other.could_include_descendant, "{other:?}");

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn scope_include_pattern_descends_soft_dist_only_when_pattern_can_match() {
        let repo = temp_repo("scope-include-dist");
        write_test_file(&repo, ".gitignore", "dist/\n");
        write_test_file(
            &repo,
            "dist/foo.js",
            "export function explicit_dist_file() { return 1; }\n",
        );
        write_test_file(
            &repo,
            "dist/other.js",
            "export function unrelated_dist_file() { return 1; }\n",
        );

        let unrelated = collect_repo_files_with_scope(
            &repo,
            &IndexScopeOptions {
                include_patterns: vec!["src/keep.ts".to_string()],
                ..IndexScopeOptions::default()
            },
        )
        .expect("collect unrelated include");
        let unrelated_dist = directory_prune_decision(&unrelated.scope_report, "dist");
        assert!(unrelated_dist.pruned, "{unrelated_dist:?}");
        assert!(
            !unrelated_dist.could_include_descendant,
            "{unrelated_dist:?}"
        );

        let scoped = collect_repo_files_with_scope(
            &repo,
            &IndexScopeOptions {
                include_patterns: vec!["dist/foo.js".to_string()],
                ..IndexScopeOptions::default()
            },
        )
        .expect("collect scoped dist include");
        let files = scoped
            .files
            .iter()
            .map(|path| repo_relative_path(&repo, path).expect("relative"))
            .collect::<BTreeSet<_>>();

        assert!(files.contains("dist/foo.js"));
        assert!(!files.contains("dist/other.js"));
        let dist = directory_prune_decision(&scoped.scope_report, "dist");
        assert!(!dist.pruned, "{dist:?}");
        assert!(dist.could_include_descendant, "{dist:?}");

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn scope_complex_glob_conservative_descend_is_audited() {
        let repo = temp_repo("scope-complex-glob");
        write_test_file(
            &repo,
            "target/debug/noisy.generated.ts",
            "export function generated() { return 1; }\n",
        );

        let scoped = collect_repo_files_with_scope(
            &repo,
            &IndexScopeOptions {
                include_patterns: vec!["**/*.generated.ts".to_string()],
                ..IndexScopeOptions::default()
            },
        )
        .expect("collect complex glob");
        let target = directory_prune_decision(&scoped.scope_report, "target");
        assert!(!target.pruned, "{target:?}");
        assert!(target.could_include_descendant, "{target:?}");
        assert!(
            target.reason.contains("complex_glob_conservative_descend"),
            "{target:?}"
        );

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn generated_junk_include_pruning_keeps_traversal_bounded() {
        let repo = generated_junk_fixture();
        let no_default = collect_repo_files_with_scope(
            &repo,
            &IndexScopeOptions {
                no_default_excludes: true,
                ..IndexScopeOptions::default()
            },
        )
        .expect("collect no-default fixture");
        let scoped = collect_repo_files_with_scope(
            &repo,
            &IndexScopeOptions {
                include_patterns: vec!["src/main.ts".to_string()],
                ..IndexScopeOptions::default()
            },
        )
        .expect("collect exact include fixture");

        assert!(
            scoped.scope_report.paths_evaluated < no_default.scope_report.paths_evaluated,
            "exact include should not globally disable pruning: scoped={} no_default={}",
            scoped.scope_report.paths_evaluated,
            no_default.scope_report.paths_evaluated
        );
        for excluded_dir in ["target", "node_modules", ".venv", "__pycache__", ".cache"] {
            let decision = directory_prune_decision(&scoped.scope_report, excluded_dir);
            assert!(decision.pruned, "{decision:?}");
            assert!(!decision.could_include_descendant, "{decision:?}");
        }
    }

    #[test]
    fn generated_junk_fixture_default_index_persists_only_allowed_sources() {
        let repo = generated_junk_fixture();
        let work = temp_repo("generated-junk-index-db");
        let db = work.join("generated-junk.sqlite");

        let summary = index_repo_to_db_with_options(
            &repo,
            &db,
            IndexOptions {
                worker_count: Some(1),
                ..IndexOptions::default()
            },
        )
        .expect("index generated junk fixture");
        let indexed = indexed_file_paths(&db);
        let expected = expected_generated_junk_source_paths();

        assert_eq!(indexed, expected);
        assert_eq!(summary.files_indexed, expected.len());
        assert_eq!(summary.files_parsed, expected.len());
        assert_eq!(summary.parse_errors, 0);
        assert_db_integrity(&db);

        fs::remove_dir_all(work).expect("cleanup generated junk DB workspace");
    }

    #[test]
    fn collect_repo_files_applies_safe_hard_excludes_by_default() {
        let repo = temp_repo("scope-default-hard-excludes");
        write_test_file(&repo, "src/main.ts", "export const app = 1;\n");
        write_test_file(
            &repo,
            "fixtures/basic/src/app.ts",
            "export const fixture = 1;\n",
        );
        write_test_file(&repo, "tests/main.test.ts", "export const test = 1;\n");
        write_test_file(&repo, "examples/demo.ts", "export const demo = 1;\n");
        write_test_file(&repo, "docs/example.ts", "export const docs = 1;\n");
        write_test_file(
            &repo,
            "target/debug/generated.ts",
            "export const target = 1;\n",
        );
        write_test_file(
            &repo,
            "node_modules/pkg/index.ts",
            "export const dependency = 1;\n",
        );
        write_test_file(
            &repo,
            "reports/diagnostic_lab/artifact.py",
            "artifact = 1\n",
        );
        write_test_file(
            &repo,
            "reports/handwritten/source.ts",
            "export const reportSource = 1;\n",
        );
        write_test_file(&repo, "src/cache.sqlite", "not source");

        let files = collected_rel_paths(&repo, &IndexScopeOptions::default());

        assert!(files.contains("src/main.ts"));
        assert!(files.contains("fixtures/basic/src/app.ts"));
        assert!(files.contains("tests/main.test.ts"));
        assert!(files.contains("examples/demo.ts"));
        assert!(files.contains("docs/example.ts"));
        assert!(files.contains("reports/handwritten/source.ts"));
        assert!(!files.contains("target/debug/generated.ts"));
        assert!(!files.contains("node_modules/pkg/index.ts"));
        assert!(!files.contains("reports/diagnostic_lab/artifact.py"));
        assert!(!files.contains("src/cache.sqlite"));
    }

    #[test]
    fn collect_repo_files_respects_gitignore_and_include_ignored_override() {
        let repo = temp_repo("scope-gitignore");
        fs::write(repo.join(".gitignore"), "dist/\n*.log\n").expect("write gitignore");
        write_test_file(&repo, "src/main.ts", "export const app = 1;\n");
        write_test_file(&repo, "dist/app.ts", "export const built = 1;\n");
        write_test_file(&repo, "logs/run.log", "log");

        let default_files = collected_rel_paths(&repo, &IndexScopeOptions::default());
        assert!(default_files.contains("src/main.ts"));
        assert!(!default_files.contains("dist/app.ts"));
        assert!(!default_files.contains("logs/run.log"));

        let include_ignored_files = collected_rel_paths(
            &repo,
            &IndexScopeOptions {
                include_ignored: true,
                ..IndexScopeOptions::default()
            },
        );
        assert!(include_ignored_files.contains("dist/app.ts"));
        assert!(
            !include_ignored_files.contains("logs/run.log"),
            "hard log suffix stays excluded even when gitignored paths are included"
        );
    }

    #[test]
    fn scope_overrides_can_include_or_exclude_explicit_paths() {
        let repo = temp_repo("scope-overrides");
        write_test_file(&repo, "src/main.ts", "export const app = 1;\n");
        write_test_file(
            &repo,
            "target/debug/generated.ts",
            "export const target = 1;\n",
        );

        let included = collected_rel_paths(
            &repo,
            &IndexScopeOptions {
                include_patterns: vec!["target/debug/generated.ts".to_string()],
                ..IndexScopeOptions::default()
            },
        );
        assert!(included.contains("target/debug/generated.ts"));

        let excluded = collected_rel_paths(
            &repo,
            &IndexScopeOptions {
                exclude_patterns: vec!["src/main.ts".to_string()],
                ..IndexScopeOptions::default()
            },
        );
        assert!(!excluded.contains("src/main.ts"));

        let no_default = collected_rel_paths(
            &repo,
            &IndexScopeOptions {
                no_default_excludes: true,
                ..IndexScopeOptions::default()
            },
        );
        assert!(no_default.contains("target/debug/generated.ts"));
    }

    #[test]
    fn collect_repo_files_include_patterns_are_overrides_not_restrictive() {
        let repo = temp_repo("scope-include-override-not-restrictive");
        write_test_file(&repo, "src/main.ts", "export const main = 1;\n");
        write_test_file(&repo, "src/other.ts", "export const other = 1;\n");
        write_test_file(
            &repo,
            "target/debug/generated.ts",
            "export const generated = 1;\n",
        );

        let files = collected_rel_paths(
            &repo,
            &IndexScopeOptions {
                include_patterns: vec!["target/debug/generated.ts".to_string()],
                ..IndexScopeOptions::default()
            },
        );

        assert!(files.contains("src/main.ts"));
        assert!(files.contains("src/other.ts"));
        assert!(files.contains("target/debug/generated.ts"));
    }

    fn assert_db_integrity(db: &Path) {
        let store = SqliteGraphStore::open(db).expect("open db");
        store.full_integrity_gate().expect("integrity gate");
    }

    fn assert_no_atomic_temp_dbs(db: &Path) {
        let Some(parent) = db.parent() else {
            return;
        };
        let file_name = db
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let leftovers = fs::read_dir(parent)
            .expect("read db parent")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with(&format!(".{file_name}.tmp-")))
            .collect::<Vec<_>>();
        assert!(
            leftovers.is_empty(),
            "leftover temp DB files: {leftovers:?}"
        );
    }

    fn assert_text_evidence_file(
        store: &SqliteGraphStore,
        repo_relative_path: &str,
        expected_kind: TextEvidenceFileKind,
    ) -> FileRecord {
        let file = store
            .get_file(repo_relative_path)
            .expect("file lookup")
            .unwrap_or_else(|| panic!("missing text evidence file {repo_relative_path}"));
        assert_eq!(file.language, None, "{repo_relative_path}");
        assert_eq!(
            file.metadata.get("evidence_kind").and_then(Value::as_str),
            Some(TEXT_EVIDENCE_KIND),
            "{repo_relative_path}"
        );
        assert_eq!(
            file.metadata.get("evidence_role").and_then(Value::as_str),
            Some(TEXT_EVIDENCE_KIND),
            "{repo_relative_path}"
        );
        assert_eq!(
            file.metadata.get("proof_status").and_then(Value::as_str),
            Some(TEXT_EVIDENCE_PROOF_STATUS),
            "{repo_relative_path}"
        );
        assert_eq!(
            file.metadata
                .get("source_file_kind")
                .and_then(Value::as_str),
            Some(expected_kind.as_str()),
            "{repo_relative_path}"
        );
        assert_eq!(
            file.metadata.get("graph_proof").and_then(Value::as_bool),
            Some(false),
            "{repo_relative_path}"
        );
        let text_metadata = file
            .metadata
            .get("text_evidence")
            .unwrap_or_else(|| panic!("missing text_evidence metadata for {repo_relative_path}"));
        assert_eq!(
            text_metadata.get("bounded").and_then(Value::as_bool),
            Some(true),
            "{repo_relative_path}"
        );
        assert!(
            text_metadata
                .get("tokens")
                .and_then(Value::as_array)
                .is_some_and(|tokens| tokens.len() <= TEXT_EVIDENCE_MAX_TOKENS_PER_FILE),
            "{repo_relative_path}"
        );
        assert!(
            text_metadata.get("source").is_none()
                && text_metadata.get("body").is_none()
                && text_metadata.get("full_text").is_none()
                && text_metadata.get("content").is_none(),
            "text evidence metadata must not store full file bodies for {repo_relative_path}"
        );
        file
    }

    fn entities_by_kind_and_name(
        store: &SqliteGraphStore,
        kind: EntityKind,
        name: &str,
    ) -> Vec<Entity> {
        store
            .find_entities_by_exact_symbol(name)
            .expect("find symbol")
            .into_iter()
            .filter(|entity| entity.kind == kind && entity.name == name)
            .collect()
    }

    fn entity_by_file_kind_and_name(
        store: &SqliteGraphStore,
        repo_relative_path: &str,
        kind: EntityKind,
        name: &str,
    ) -> Entity {
        store
            .list_entities_by_file(repo_relative_path)
            .expect("entities by file")
            .into_iter()
            .find(|entity| entity.kind == kind && entity.name == name)
            .unwrap_or_else(|| panic!("missing {kind} entity named {name} in {repo_relative_path}"))
    }

    fn test_entity(repo_relative_path: &str, kind: EntityKind, name: &str) -> Entity {
        Entity {
            id: format!("{repo_relative_path}:{name}"),
            kind,
            name: name.to_string(),
            qualified_name: format!("{repo_relative_path}.{name}"),
            repo_relative_path: repo_relative_path.to_string(),
            source_span: Some(SourceSpan::with_columns(repo_relative_path, 1, 1, 1, 10)),
            content_hash: None,
            file_hash: Some("hash".to_string()),
            created_from: "test".to_string(),
            confidence: 1.0,
            metadata: Metadata::default(),
        }
    }

    fn graph_budget_file_record(repo_relative_path: &str) -> FileRecord {
        FileRecord {
            repo_relative_path: repo_relative_path.to_string(),
            file_hash: "hash".to_string(),
            language: Some("typescript".to_string()),
            size_bytes: 128,
            indexed_at_unix_ms: None,
            metadata: Metadata::default(),
        }
    }

    fn graph_budget_test_edge(
        head_id: &str,
        relation: RelationKind,
        tail_id: &str,
        line: usize,
    ) -> Edge {
        let line = line as u32;
        let span = SourceSpan::with_columns("src/fanout.ts", line, 1, line, 10);
        Edge {
            id: stable_edge_id(head_id, relation, tail_id, &span),
            head_id: head_id.to_string(),
            relation,
            tail_id: tail_id.to_string(),
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
            metadata: Metadata::default(),
        }
    }

    #[test]
    fn mcp_and_cli_shared_index_counts_match_on_fixture() {
        let repo = temp_repo("equivalent");
        fs::write(
            repo.join("src").join("auth.ts"),
            "export function sanitize(input: string) { return input.trim(); }\nexport function login(req: any) { return sanitize(req.body.email); }\n",
        )
        .expect("write auth");

        let cli_db = repo.join("target").join("cli.sqlite");
        let mcp_db = repo.join("target").join("mcp.sqlite");
        let cli = index_repo_to_db(&repo, &cli_db).expect("cli index");
        let mcp = index_repo_to_db(&repo, &mcp_db).expect("mcp index");

        assert_eq!(cli.files_indexed, mcp.files_indexed);
        assert_eq!(cli.entities, mcp.entities);
        assert_eq!(cli.edges, mcp.edges);
        assert!(!repo.join(".codegraph").exists());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn call_records_use_matching_close_paren_for_nested_arguments() {
        let source = "export function run() { return target(format(input), other(\"a)b\")); }\n";
        let records = call_records_for_local_name(source, "src/main.ts", "target");

        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.span.start_line, 1);
        assert_eq!(record.span.start_column, Some(32));
        assert_eq!(record.span.end_column, Some(67));
        assert_eq!(record.args, "format(input), other(\"a)b\")");
    }

    #[test]
    fn parse_assertion_specs_splits_nearby_assertions_on_same_line() {
        let checkout = test_entity("src/checkout.ts", EntityKind::Function, "checkout");
        let charge_card = test_entity("src/service.ts", EntityKind::Function, "chargeCard");
        let import_targets = BTreeMap::from([
            ("checkout".to_string(), checkout.clone()),
            ("chargeCard".to_string(), charge_card.clone()),
        ]);
        let line = "  expect(checkout(5)).toBe(\"ok\"); expect(chargeCard(5)).toBe(\"charged\");";
        let source = format!("it(\"checks\", () => {{\n{line}\n}});\n");
        let assertions = parse_assertion_specs("tests/checkout.test.ts", &source, &import_targets);

        assert_eq!(assertions.len(), 2);
        let checkout_assertion = assertions
            .iter()
            .find(|assertion| assertion.target.id == checkout.id)
            .expect("checkout assertion");
        let charge_assertion = assertions
            .iter()
            .find(|assertion| assertion.target.id == charge_card.id)
            .expect("chargeCard assertion");
        let checkout_text = "expect(checkout(5)).toBe(\"ok\")";
        let charge_text = "expect(chargeCard(5)).toBe(\"charged\")";
        let charge_start = line.find(charge_text).expect("charge statement");

        assert_eq!(checkout_assertion.span.start_line, 2);
        assert_eq!(checkout_assertion.span.start_column, Some(3));
        assert_eq!(
            checkout_assertion.span.end_column,
            Some(3 + checkout_text.chars().count() as u32)
        );
        assert_eq!(charge_assertion.span.start_line, 2);
        assert_eq!(
            charge_assertion.span.start_column,
            Some(line[..charge_start].chars().count() as u32 + 1)
        );
        assert_eq!(
            charge_assertion.span.end_column,
            Some(
                line[..charge_start].chars().count() as u32
                    + 1
                    + charge_text.chars().count() as u32
            )
        );
    }

    #[test]
    fn parse_extract_worker_counts_produce_same_local_fact_bundles() {
        let pending = vec![
            pending_test_file(
                "src/b.ts",
                "export function beta(value: number) {\n  return value + 2;\n}\n",
            ),
            pending_test_file(
                "src/a.ts",
                "import { beta } from './b';\nexport function alpha(value: number) {\n  const next = beta(value);\n  return next;\n}\n",
            ),
            pending_test_file(
                "src/c.ts",
                "export class Worker {\n  run(value: number) {\n    return value + 1;\n  }\n}\n",
            ),
            pending_test_file(
                "src/d.ts",
                "export function delta(flag: boolean) {\n  if (flag) return 'yes';\n  return 'no';\n}\n",
            ),
        ];

        let (serial, serial_stats) =
            parse_extract_pending_files(pending.clone(), 1).expect("serial parse/extract");
        let (parallel, parallel_stats) =
            parse_extract_pending_files(pending, 4).expect("parallel parse/extract");

        assert_eq!(local_fact_bundle(&serial), local_fact_bundle(&parallel));
        assert_eq!(stat_shape(&serial_stats), stat_shape(&parallel_stats));
    }

    #[test]
    fn local_fact_bundle_exposes_worker_output_categories() {
        let pending = vec![pending_test_file(
            "src/a.ts",
            "function beta(value: number) {\n  return value + 1;\n}\n\
             export function alpha(value: number) {\n  return beta(value);\n}\n",
        )];

        let (bundles, stats) = parse_extract_pending_files(pending, 1).expect("parse/extract");
        assert_eq!(stats.len(), 1);
        let bundle = bundles.first().expect("bundle");

        assert_eq!(bundle.repo_relative_path, "src/a.ts");
        assert_eq!(bundle.file_hash, bundle.extraction.file.file_hash);
        assert_eq!(bundle.language.as_deref(), Some("typescript"));
        assert!(!bundle.declarations.is_empty());
        assert!(bundle
            .local_callsites
            .iter()
            .any(|edge| edge.relation == RelationKind::Calls));
        assert!(!bundle.source_spans.is_empty());
        assert!(bundle.extraction_warnings.is_empty());
    }

    #[test]
    fn tier0_local_fact_bundle_preserves_parser_status_for_syntax_recovery() {
        let pending = vec![pending_test_file(
            "src/broken.ts",
            "function safe() { return 1; }\nfunction broken() {\n  return target(\n}\n",
        )];

        let (bundles, stats) = parse_extract_pending_files(pending, 1).expect("parse/extract");
        let stat = stats.first().expect("stat");
        assert!(!stat.parse_error);
        assert!(stat.syntax_error);

        let bundle = bundles.first().expect("bundle");
        assert_eq!(
            bundle.source,
            "function safe() { return 1; }\nfunction broken() {\n  return target(\n}\n"
        );
        assert_eq!(
            bundle
                .extraction
                .file
                .metadata
                .get("parser_status")
                .and_then(Value::as_str),
            Some("syntax_errors_recovered")
        );
        assert_eq!(
            bundle
                .extraction
                .file
                .metadata
                .get("unsupported_behavior_label")
                .and_then(Value::as_str),
            Some("malformed_regions_not_trusted_for_exact_relations")
        );
        assert!(bundle
            .declarations
            .iter()
            .any(|symbol| symbol.name == "safe"));
        assert!(!bundle.local_callsites.iter().any(|edge| {
            edge.relation == RelationKind::Calls
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.start_line >= 3
        }));
    }

    #[test]
    fn tier0_parser_error_metadata_is_structured_unsupported_evidence() {
        let metadata = parser_error_file_metadata(
            Some("123".to_string()),
            Some("typescript"),
            "synthetic parser failure",
        );

        assert_eq!(
            metadata.get("parser_status").and_then(Value::as_str),
            Some("parser_error")
        );
        assert_eq!(
            metadata.get("claim_state").and_then(Value::as_str),
            Some("unsupported")
        );
        assert_eq!(
            metadata
                .get("unsupported_behavior_label")
                .and_then(Value::as_str),
            Some("parser_invocation_failed")
        );
        assert_eq!(
            metadata.get("parser_frontend").and_then(Value::as_str),
            Some("typescript")
        );
        assert!(metadata
            .get("graph_relation_claims")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty));
    }

    #[test]
    fn local_fact_bundle_serializes_round_trip() {
        let pending = vec![pending_test_file(
            "src/a.ts",
            "export function alpha(value: number) {\n  return value + 1;\n}\n",
        )];

        let (bundles, _) = parse_extract_pending_files(pending, 1).expect("parse/extract");
        let bundle = bundles.first().expect("bundle");
        let encoded = serde_json::to_string(bundle).expect("serialize bundle");
        let decoded: LocalFactBundle = serde_json::from_str(&encoded).expect("deserialize bundle");

        assert_eq!(bundle, &decoded);
    }

    #[test]
    fn local_fact_reducer_sorts_and_deduplicates_facts() {
        let pending = vec![pending_test_file(
            "src/a.ts",
            "export function alpha(value: number) {\n  return value + 1;\n}\n",
        )];
        let (mut bundles, _) = parse_extract_pending_files(pending, 1).expect("parse/extract");
        let bundle = bundles.first_mut().expect("bundle");
        let entity = bundle.extraction.entities.first().expect("entity").clone();
        bundle.extraction.entities.push(entity);

        let reduced = reduce_local_fact_bundles(bundles);
        let reduced_bundle = reduced.bundles.first().expect("reduced bundle");
        let entity_ids = reduced_bundle
            .extraction
            .entities
            .iter()
            .map(|entity| entity.id.clone())
            .collect::<Vec<_>>();
        let unique_entity_ids = entity_ids.iter().collect::<BTreeSet<_>>();

        assert!(reduced.warnings.is_empty());
        assert_eq!(entity_ids.len(), unique_entity_ids.len());
        assert!(!reduced.symbol_table.by_id.is_empty());
        assert!(reduced
            .symbol_table
            .by_file
            .contains_key(&"src/a.ts".to_string()));
    }

    #[test]
    fn local_fact_reducer_output_is_independent_of_bundle_order() {
        let pending = vec![
            pending_test_file(
                "src/b.ts",
                "export function beta(value: number) {\n  return value + 2;\n}\n",
            ),
            pending_test_file(
                "src/a.ts",
                "export function alpha(value: number) {\n  return value + 1;\n}\n",
            ),
        ];

        let (bundles, _) = parse_extract_pending_files(pending, 1).expect("parse/extract");
        let mut shuffled = bundles.clone();
        shuffled.reverse();

        let reduced = reduce_local_fact_bundles(bundles);
        let shuffled_reduced = reduce_local_fact_bundles(shuffled);
        let paths = reduced
            .bundles
            .iter()
            .map(|bundle| bundle.repo_relative_path.clone())
            .collect::<Vec<_>>();

        assert_eq!(paths, vec!["src/a.ts", "src/b.ts"]);
        assert_eq!(
            reducer_signature(&reduced),
            reducer_signature(&shuffled_reduced)
        );
    }

    #[test]
    fn full_index_persists_bounded_path_evidence_rows() {
        let repo = temp_repo("stored-path-evidence");
        fs::write(
            repo.join("src").join("service.ts"),
            "export function target() {\n  return 1;\n}\n",
        )
        .expect("write service");
        fs::write(
            repo.join("src").join("main.ts"),
            "import { target } from './service';\n\
             export function run() {\n  return target();\n}\n",
        )
        .expect("write main");
        let db = repo.join(".codegraph").join("graph.sqlite");

        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");

        assert!(
            store.count_path_evidence().expect("count path evidence") > 0,
            "index should persist stored PathEvidence rows for proof-relevant edges"
        );
    }

    #[test]
    fn reducer_resolves_static_import_alias_from_local_bundles() {
        let pending = vec![
            pending_test_file(
                "src/service.ts",
                "export function canonicalName() { return 'ok'; }\n",
            ),
            pending_test_file(
                "src/consumer.ts",
                "import { canonicalName as aliasName } from './service';\n\
                 export function run() { return aliasName(); }\n",
            ),
        ];
        let (bundles, _) = parse_extract_pending_files(pending, 2).expect("parse/extract");
        let reduced = reduce_local_fact_bundles(bundles);
        let imported = reduced
            .bundles
            .iter()
            .flat_map(|bundle| &bundle.extraction.entities)
            .find(|entity| {
                entity.repo_relative_path == "src/service.ts"
                    && entity.kind == EntityKind::Function
                    && entity.name == "canonicalName"
            })
            .expect("imported function");
        let run = reduced
            .bundles
            .iter()
            .flat_map(|bundle| &bundle.extraction.entities)
            .find(|entity| {
                entity.repo_relative_path == "src/consumer.ts"
                    && entity.kind == EntityKind::Function
                    && entity.name == "run"
            })
            .expect("run function");

        assert!(reduced.global_facts.edges.iter().any(|edge| {
            edge.relation == RelationKind::Calls
                && edge.head_id == run.id
                && edge.tail_id == imported.id
                && edge.exactness == Exactness::ParserVerified
                && edge
                    .metadata
                    .get("resolver")
                    .and_then(|value| value.as_str())
                    == Some("static_import_call_target")
        }));
        assert!(reduced.global_facts.edges.iter().any(|edge| {
            edge.relation == RelationKind::AliasedBy
                && edge.head_id == imported.id
                && edge.exactness == Exactness::ParserVerified
        }));
    }

    #[test]
    fn full_index_worker_count_determinism_preserves_graph_facts() {
        let repo = temp_repo("worker-db-determinism");
        fs::write(
            repo.join("src").join("a.ts"),
            "export function chooseUser() { return 'a'; }\n",
        )
        .expect("write a");
        fs::write(
            repo.join("src").join("b.ts"),
            "export function chooseUser() { return 'b'; }\n",
        )
        .expect("write b");
        fs::write(
            repo.join("src").join("main.ts"),
            "import { chooseUser as choose } from './a';\n\
             export function handler() {\n  return choose();\n}\n",
        )
        .expect("write main");
        fs::write(
            repo.join("src").join("util.ts"),
            "export function helper(value: number) {\n  return value + 1;\n}\n",
        )
        .expect("write util");

        let db_one = repo.join("target").join("workers_one.sqlite");
        let db_two = repo.join("target").join("workers_two.sqlite");
        let db_many = repo.join("target").join("workers_many.sqlite");
        let one = index_repo_to_db_with_options(
            &repo,
            &db_one,
            IndexOptions {
                profile: true,
                json: false,
                worker_count: Some(1),
                ..IndexOptions::default()
            },
        )
        .expect("index with one worker");
        let two = index_repo_to_db_with_options(
            &repo,
            &db_two,
            IndexOptions {
                profile: true,
                json: false,
                worker_count: Some(2),
                ..IndexOptions::default()
            },
        )
        .expect("index with two workers");
        let many = index_repo_to_db_with_options(
            &repo,
            &db_many,
            IndexOptions {
                profile: true,
                json: false,
                worker_count: Some(4),
                ..IndexOptions::default()
            },
        )
        .expect("index with many workers");

        assert_eq!(one.files_indexed, two.files_indexed);
        assert_eq!(one.files_indexed, many.files_indexed);
        assert_eq!(one.profile.as_ref().expect("profile").worker_count, 1);
        assert_eq!(two.profile.as_ref().expect("profile").worker_count, 2);
        assert_eq!(many.profile.as_ref().expect("profile").worker_count, 4);
        assert_eq!(semantic_graph_facts(&db_one), semantic_graph_facts(&db_two));
        assert_eq!(
            semantic_graph_facts(&db_one),
            semantic_graph_facts(&db_many)
        );
        assert_eq!(
            semantic_graph_fact_hash(&db_one),
            semantic_graph_fact_hash(&db_two)
        );
        assert_eq!(
            semantic_graph_fact_hash(&db_one),
            semantic_graph_fact_hash(&db_many)
        );
        assert_no_duplicate_edge_ids(&db_two);
        assert_no_duplicate_edge_ids(&db_many);

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn derived_mutation_closure_is_persisted_with_base_provenance() {
        let repo = temp_repo("derived-mutation-provenance");
        fs::write(
            repo.join("src").join("store.ts"),
            "export const ordersTable = \"orders\";\n\n\
             export function saveOrder(order: any) {\n  return ordersTable;\n}\n",
        )
        .expect("write store");
        fs::write(
            repo.join("src").join("service.ts"),
            "import { saveOrder, ordersTable } from './store';\n\n\
             export function submitOrder(order: any) {\n  return saveOrder(order);\n}\n",
        )
        .expect("write service");

        let db = repo.join("target").join("derived.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let submit_order = entity_by_file_kind_and_name(
            &store,
            "src/service.ts",
            EntityKind::Function,
            "submitOrder",
        );
        let save_order =
            entity_by_file_kind_and_name(&store, "src/store.ts", EntityKind::Function, "saveOrder");
        let orders_table =
            entity_by_file_kind_and_name(&store, "src/store.ts", EntityKind::Table, "ordersTable");
        let edges = store.list_edges(UNBOUNDED_STORE_READ_LIMIT).expect("edges");
        let base_call = edges
            .iter()
            .find(|edge| {
                edge.relation == RelationKind::Calls
                    && edge.head_id == submit_order.id
                    && edge.tail_id == save_order.id
                    && edge.source_span.repo_relative_path == "src/service.ts"
            })
            .expect("base CALLS submitOrder -> saveOrder");
        let base_write = edges
            .iter()
            .find(|edge| {
                edge.relation == RelationKind::Writes
                    && edge.head_id == save_order.id
                    && edge.tail_id == orders_table.id
                    && edge.source_span.repo_relative_path == "src/store.ts"
            })
            .expect("base WRITES saveOrder -> ordersTable");
        let derived = edges
            .iter()
            .find(|edge| {
                edge.relation == RelationKind::MayMutate
                    && edge.head_id == submit_order.id
                    && edge.tail_id == orders_table.id
            })
            .expect("derived MAY_MUTATE submitOrder -> ordersTable");

        assert!(derived.derived);
        assert_eq!(derived.edge_class, EdgeClass::Derived);
        assert_eq!(derived.exactness, Exactness::DerivedFromVerifiedEdges);
        assert_eq!(derived.context, EdgeContext::Production);
        assert_eq!(derived.source_span.repo_relative_path, "src/service.ts");
        assert_eq!(derived.source_span.start_line, 4);
        assert_eq!(derived.source_span.start_column, Some(10));
        assert_eq!(derived.source_span.end_column, Some(26));
        assert!(derived.provenance_edges.contains(&base_call.id));
        assert!(derived.provenance_edges.contains(&base_write.id));
        assert_db_integrity(&db);

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn heuristic_mutation_method_call_is_not_promoted_to_proof_may_mutate() {
        let repo = temp_repo("method-mutation-proof-boundary");
        fs::write(
            repo.join("src").join("mutation.ts"),
            concat!(
                "export function collect(items: string[], input: string) {\n",
                "  items.push(input);\n",
                "  return items;\n",
                "}\n",
            ),
        )
        .expect("write mutation");

        let db = repo.join("target").join("method-mutation-boundary.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let collect = entity_by_file_kind_and_name(
            &store,
            "src/mutation.ts",
            EntityKind::Function,
            "collect",
        );
        let items =
            entity_by_file_kind_and_name(&store, "src/mutation.ts", EntityKind::Parameter, "items");
        let edges = store.list_edges(UNBOUNDED_STORE_READ_LIMIT).expect("edges");

        assert!(edges.iter().all(|edge| {
            !(edge.head_id == collect.id
                && edge.tail_id == items.id
                && edge.relation == RelationKind::MayMutate)
        }));
        assert_db_integrity(&db);

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn derived_local_dataflow_closure_is_persisted_with_base_provenance() {
        let repo = temp_repo("derived-dataflow-provenance");
        fs::write(
            repo.join("src").join("flow.ts"),
            concat!(
                "export function sink(value: string) { return value; }\n",
                "export function run(input: string) {\n",
                "  const a = input;\n",
                "  const b = a;\n",
                "  return sink(b);\n",
                "}\n",
            ),
        )
        .expect("write flow");

        let db = repo.join("target").join("derived-flow.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let input =
            entity_by_file_kind_and_name(&store, "src/flow.ts", EntityKind::Parameter, "input");
        let a = entity_by_file_kind_and_name(&store, "src/flow.ts", EntityKind::LocalVariable, "a");
        let b = entity_by_file_kind_and_name(&store, "src/flow.ts", EntityKind::LocalVariable, "b");
        let value =
            entity_by_file_kind_and_name(&store, "src/flow.ts", EntityKind::Parameter, "value");
        let edges = store.list_edges(UNBOUNDED_STORE_READ_LIMIT).expect("edges");
        let input_to_a = edges
            .iter()
            .find(|edge| {
                edge.relation == RelationKind::FlowsTo
                    && edge.head_id == input.id
                    && edge.tail_id == a.id
                    && !edge.derived
            })
            .expect("base FLOWS_TO input -> a");
        let a_to_b = edges
            .iter()
            .find(|edge| {
                edge.relation == RelationKind::FlowsTo
                    && edge.head_id == a.id
                    && edge.tail_id == b.id
                    && !edge.derived
            })
            .expect("base FLOWS_TO a -> b");
        let b_to_value = edges
            .iter()
            .find(|edge| {
                edge.relation == RelationKind::FlowsTo
                    && edge.head_id == b.id
                    && edge.tail_id == value.id
                    && !edge.derived
            })
            .expect("base FLOWS_TO b -> sink.value");
        let derived = edges
            .iter()
            .find(|edge| {
                edge.relation == RelationKind::FlowsTo
                    && edge.head_id == input.id
                    && edge.tail_id == value.id
                    && edge.derived
            })
            .expect("derived FLOWS_TO input -> sink.value");

        assert_eq!(derived.edge_class, EdgeClass::Derived);
        assert_eq!(derived.exactness, Exactness::DerivedFromVerifiedEdges);
        assert_eq!(derived.context, EdgeContext::Production);
        assert_eq!(derived.source_span.repo_relative_path, "src/flow.ts");
        assert_eq!(derived.source_span.start_line, 5);
        assert_eq!(derived.provenance_edges.len(), 3);
        assert!(derived.provenance_edges.contains(&input_to_a.id));
        assert!(derived.provenance_edges.contains(&a_to_b.id));
        assert!(derived.provenance_edges.contains(&b_to_value.id));
        assert!(edges.iter().all(|edge| {
            edge.relation != RelationKind::FlowsTo
                || !edge.derived
                || !edge.provenance_edges.is_empty()
        }));
        assert_db_integrity(&db);

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn sanitizer_comments_and_strings_do_not_create_sanitizer_proof() {
        let repo = temp_repo("sanitizer-comments-strings");
        fs::write(
            repo.join("src").join("flow.ts"),
            concat!(
                "export function sanitizeHtml(input: string) { return input; }\n",
                "export function run(raw: string) {\n",
                "  const note = \"sanitizeHtml(raw)\";\n",
                "  // sanitizeHtml(raw)\n",
                "  return raw;\n",
                "}\n",
            ),
        )
        .expect("write flow");

        let db = repo.join("target").join("sanitizer-comments.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let sanitizer = entity_by_file_kind_and_name(
            &store,
            "src/flow.ts",
            EntityKind::Sanitizer,
            "sanitizeHtml",
        );
        let sanitizer_edges = store
            .find_edges_by_head_relation(&sanitizer.id, RelationKind::Sanitizes)
            .expect("sanitizer edges");

        assert!(
            sanitizer_edges.is_empty(),
            "comments and string literals must not create SANITIZES proof edges"
        );

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    fn pending_test_file(repo_relative_path: &str, source: &str) -> PendingIndexFile {
        PendingIndexFile {
            repo_relative_path: repo_relative_path.to_string(),
            source: source.to_string(),
            file_hash: content_hash(source),
            language: Some("typescript".to_string()),
            size_bytes: source.len() as u64,
            modified_unix_nanos: None,
            needs_delete: false,
            duplicate_of: None,
            template_required: false,
        }
    }

    fn semantic_graph_facts(db_path: &Path) -> BTreeSet<String> {
        let store = SqliteGraphStore::open(db_path).expect("store");
        let mut facts = BTreeSet::new();
        for file in store.list_files(UNBOUNDED_STORE_READ_LIMIT).expect("files") {
            facts.insert(format!(
                "file|{}|{}|{}|{}",
                file.repo_relative_path,
                file.file_hash,
                file.language.unwrap_or_default(),
                file.size_bytes
            ));
        }
        for entity in store
            .list_entities(UNBOUNDED_STORE_READ_LIMIT)
            .expect("entities")
        {
            facts.insert(format!(
                "entity|{}|{}|{}|{}|{}|{}|{}",
                entity.id,
                entity.kind,
                entity.name,
                entity.qualified_name,
                entity.repo_relative_path,
                entity
                    .source_span
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "none".to_string()),
                entity.created_from
            ));
        }
        for edge in store.list_edges(UNBOUNDED_STORE_READ_LIMIT).expect("edges") {
            facts.insert(format!(
                "edge|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                edge.id,
                edge.head_id,
                edge.relation,
                edge.tail_id,
                edge.source_span,
                edge.exactness,
                edge.derived,
                edge.extractor,
                edge.provenance_edges.join(",")
            ));
        }
        facts
    }

    fn semantic_graph_fact_hash(db_path: &Path) -> String {
        let store = SqliteGraphStore::open(db_path).expect("store");
        let entities = store
            .list_entities(UNBOUNDED_STORE_READ_LIMIT)
            .expect("entities");
        let edges = store.list_edges(UNBOUNDED_STORE_READ_LIMIT).expect("edges");
        graph_fact_hash(&entities, &edges)
    }

    fn assert_no_duplicate_edge_ids(db_path: &Path) {
        let store = SqliteGraphStore::open(db_path).expect("store");
        let edges = store.list_edges(UNBOUNDED_STORE_READ_LIMIT).expect("edges");
        let ids = edges.iter().map(|edge| edge.id.clone()).collect::<Vec<_>>();
        let unique_ids = ids.iter().collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), unique_ids.len());
    }

    #[test]
    fn graph_fact_hash_is_order_independent_and_semantic() {
        let pending = vec![pending_test_file(
            "src/a.ts",
            "function beta(value: number) {\n  return value + 1;\n}\n\
             export function alpha(value: number) {\n  return beta(value);\n}\n",
        )];
        let (bundles, _) = parse_extract_pending_files(pending, 1).expect("parse/extract");
        let extraction = &bundles.first().expect("bundle").extraction;
        let mut reversed_entities = extraction.entities.clone();
        let mut reversed_edges = extraction.edges.clone();
        reversed_entities.reverse();
        reversed_edges.reverse();

        assert_eq!(
            graph_fact_hash(&extraction.entities, &extraction.edges),
            graph_fact_hash(&reversed_entities, &reversed_edges)
        );

        let mut changed_edges = extraction.edges.clone();
        let changed = changed_edges
            .iter_mut()
            .find(|edge| edge.relation == RelationKind::Calls)
            .expect("call edge");
        changed.tail_id.push_str(":changed");

        assert_ne!(
            graph_fact_hash(&extraction.entities, &extraction.edges),
            graph_fact_hash(&extraction.entities, &changed_edges)
        );
    }

    fn reducer_signature(plan: &ReducedIndexPlan) -> BTreeSet<String> {
        let mut facts = local_fact_bundle(&plan.bundles);
        for (id, symbol) in &plan.symbol_table.by_id {
            facts.insert(format!(
                "symbol|{}|{}|{}|{}|{}",
                id,
                symbol.kind,
                symbol.name,
                symbol.qualified_name,
                symbol
                    .source_span
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "none".to_string())
            ));
        }
        for (path, ids) in &plan.symbol_table.by_file {
            facts.insert(format!("file_symbols|{}|{}", path, ids.join(",")));
        }
        for (qualified_name, ids) in &plan.symbol_table.by_qualified_name {
            facts.insert(format!(
                "qualified_symbols|{}|{}",
                qualified_name,
                ids.join(",")
            ));
        }
        for warning in &plan.warnings {
            facts.insert(format!("warning|{warning}"));
        }
        for fact in &plan.global_facts.entities {
            facts.insert(format!(
                "global_entity|{}|{:?}",
                canonical_entity_fact_line(&fact.entity),
                fact.write_mode
            ));
        }
        for edge in &plan.global_facts.edges {
            facts.insert(format!("global_edge|{}", canonical_edge_fact_line(edge)));
        }
        facts
    }

    fn local_fact_bundle(outputs: &[IndexedFileOutput]) -> BTreeSet<String> {
        let mut facts = BTreeSet::new();
        for output in outputs {
            facts.insert(format!(
                "file|{}|{}|{}",
                output.repo_relative_path,
                output.extraction.file.file_hash,
                output.extraction.file.size_bytes
            ));
            for entity in &output.extraction.entities {
                facts.insert(format!(
                    "entity|{}|{}|{}|{}|{}|{}",
                    entity.id,
                    entity.kind,
                    entity.name,
                    entity.qualified_name,
                    entity.repo_relative_path,
                    entity
                        .source_span
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "none".to_string())
                ));
            }
            for edge in &output.extraction.edges {
                facts.insert(format!(
                    "edge|{}|{}|{}|{}|{}|{}|{}|{}",
                    edge.id,
                    edge.head_id,
                    edge.relation,
                    edge.tail_id,
                    edge.source_span,
                    edge.exactness,
                    edge.derived,
                    edge.extractor
                ));
            }
        }
        facts
    }

    fn stat_shape(stats: &[ParseExtractStat]) -> BTreeSet<String> {
        stats
            .iter()
            .map(|stat| {
                format!(
                    "{}|parse_error={}|syntax_error={}|skipped={}|message={}",
                    stat.repo_relative_path,
                    stat.parse_error,
                    stat.syntax_error,
                    stat.skipped,
                    stat.message.as_deref().unwrap_or("")
                )
            })
            .collect()
    }

    #[test]
    fn manifest_diff_classifies_metadata_unchanged_and_same_hash_rename() {
        let repo = temp_repo("manifest-diff");
        let source = "export function moved() { return 'ok'; }\n";
        let current_path = repo.join("src").join("new_path.ts");
        fs::write(&current_path, source).expect("write current");
        let metadata = fs::metadata(&current_path).expect("metadata");
        let hash = content_hash(source);
        let current_record = FileRecord {
            repo_relative_path: "src/new_path.ts".to_string(),
            file_hash: hash.clone(),
            language: Some("typescript".to_string()),
            size_bytes: source.len() as u64,
            indexed_at_unix_ms: Some(1),
            metadata: file_manifest_metadata(modified_unix_nanos(&metadata)),
        };
        let stale_record = FileRecord {
            repo_relative_path: "src/old_path.ts".to_string(),
            file_hash: hash.clone(),
            language: Some("typescript".to_string()),
            size_bytes: source.len() as u64,
            indexed_at_unix_ms: Some(1),
            metadata: file_manifest_metadata(Some("older".to_string())),
        };
        let current_paths = BTreeSet::from(["src/new_path.ts".to_string()]);
        let mut diff = ManifestDiffEngine::new(vec![current_record, stale_record], &current_paths);

        assert_eq!(
            diff.classify_file("src/new_path.ts", source.len() as u64, &metadata),
            ManifestFileDecision::MetadataUnchanged
        );
        assert_eq!(
            diff.record_rename_matches(&repo, "src/new_path.ts", &hash),
            1
        );
        assert_eq!(diff.stale_cleanup_paths(), vec!["src/old_path.ts"]);

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn duplicate_source_content_keeps_separate_file_and_symbol_identity() {
        let repo = temp_repo("duplicates");
        let source = "export function login() { return 'ok'; }\n";
        fs::write(repo.join("src").join("a.ts"), source).expect("write a");
        fs::write(repo.join("src").join("b.ts"), source).expect("write b");

        let db = repo.join("target").join("compact.sqlite");
        let summary = index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");

        assert_eq!(summary.files_indexed, 2);
        assert_eq!(store.count_files().expect("files"), 2);
        let first_login =
            entity_by_file_kind_and_name(&store, "src/a.ts", EntityKind::Function, "login");
        let second_login =
            entity_by_file_kind_and_name(&store, "src/b.ts", EntityKind::Function, "login");
        assert_eq!(first_login.file_hash, second_login.file_hash);
        assert_ne!(first_login.id, second_login.id);
        assert_ne!(
            first_login.repo_relative_path,
            second_login.repo_relative_path
        );
        assert!(!repo.join(".codegraph").exists());
        drop(store);

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_same_function_name_in_two_files_has_distinct_entity_ids() {
        let repo = temp_repo("same-function-name");
        fs::write(
            repo.join("src").join("a.ts"),
            "export function shared() { return 'a'; }\n",
        )
        .expect("write a");
        fs::write(
            repo.join("src").join("b.ts"),
            "export function shared() { return 'b'; }\n",
        )
        .expect("write b");

        let db = repo.join("target").join("identity.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let shared = entities_by_kind_and_name(&store, EntityKind::Function, "shared");

        assert_eq!(shared.len(), 2, "expected one shared function per file");
        assert_ne!(shared[0].id, shared[1].id);
        assert_ne!(shared[0].repo_relative_path, shared[1].repo_relative_path);

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_class_method_and_standalone_function_do_not_collide() {
        let repo = temp_repo("method-vs-function");
        fs::write(
            repo.join("src").join("mixed.ts"),
            "export function run() { return 1; }\n\
             export class Worker { run() { return 2; } }\n",
        )
        .expect("write mixed");

        let db = repo.join("target").join("identity.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let function_run =
            entity_by_file_kind_and_name(&store, "src/mixed.ts", EntityKind::Function, "run");
        let method_run =
            entity_by_file_kind_and_name(&store, "src/mixed.ts", EntityKind::Method, "run");

        assert_ne!(function_run.id, method_run.id);
        assert!(function_run.qualified_name.ends_with(".run"));
        assert!(method_run.qualified_name.contains("Worker.run"));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_same_method_name_in_two_classes_has_distinct_entity_ids() {
        let repo = temp_repo("same-method-name");
        fs::write(
            repo.join("src").join("classes.ts"),
            "export class Alpha { run() { return 1; } }\n\
             export class Beta { run() { return 2; } }\n",
        )
        .expect("write classes");

        let db = repo.join("target").join("identity.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let run_methods = entities_by_kind_and_name(&store, EntityKind::Method, "run");

        assert_eq!(run_methods.len(), 2, "expected Alpha.run and Beta.run");
        assert_ne!(run_methods[0].id, run_methods[1].id);
        assert!(run_methods
            .iter()
            .any(|entity| entity.qualified_name.contains("Alpha.run")));
        assert!(run_methods
            .iter()
            .any(|entity| entity.qualified_name.contains("Beta.run")));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_default_export_and_named_export_are_distinct_syntax_entities() {
        let repo = temp_repo("default-named-export");
        fs::write(
            repo.join("src").join("exports.ts"),
            "export default function defaultHandler() { return namedHandler(); }\n\
             export function namedHandler() { return 1; }\n",
        )
        .expect("write exports");

        let db = repo.join("target").join("identity.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let entities = store
            .list_entities_by_file("src/exports.ts")
            .expect("entities");
        let default_handler = entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "defaultHandler")
            .expect("default function entity");
        let default_export = entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "default")
            .expect("canonical default export function entity");
        let named_handler = entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "namedHandler")
            .expect("named function entity");
        let export_entities = entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::Export)
            .collect::<Vec<_>>();

        assert_ne!(default_handler.id, named_handler.id);
        assert_ne!(default_export.id, default_handler.id);
        assert_ne!(default_export.id, named_handler.id);
        assert_eq!(default_export.qualified_name, "src::exports.default");
        assert!(export_entities
            .iter()
            .any(|entity| entity.name.contains("defaultHandler")));
        assert!(export_entities
            .iter()
            .any(|entity| entity.name.contains("namedHandler")));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_named_import_points_to_export_target() {
        let repo = temp_repo("named-import-target");
        fs::write(
            repo.join("src").join("service.ts"),
            "export function canonicalName() { return 'ok'; }\n",
        )
        .expect("write service");
        fs::write(
            repo.join("src").join("consumer.ts"),
            "import { canonicalName } from './service';\n\
             export function run() { return canonicalName(); }\n",
        )
        .expect("write consumer");

        let db = repo.join("target").join("resolver.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let canonical = entity_by_file_kind_and_name(
            &store,
            "src/service.ts",
            EntityKind::Function,
            "canonicalName",
        );
        let run =
            entity_by_file_kind_and_name(&store, "src/consumer.ts", EntityKind::Function, "run");
        let calls = store
            .find_edges_by_head_relation(&run.id, RelationKind::Calls)
            .expect("calls from run");

        assert!(calls.iter().any(|edge| {
            edge.tail_id == canonical.id
                && edge.exactness == Exactness::ParserVerified
                && edge
                    .metadata
                    .get("resolver")
                    .and_then(|value| value.as_str())
                    == Some("static_import_call_target")
        }));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_import_alias_points_to_export_target() {
        let repo = temp_repo("import-alias-target");
        fs::write(
            repo.join("src").join("service.ts"),
            "export function canonicalName() { return 'ok'; }\n",
        )
        .expect("write service");
        fs::write(
            repo.join("src").join("consumer.ts"),
            "import { canonicalName as aliasName } from './service';\n\
             export function run() { return aliasName(); }\n",
        )
        .expect("write consumer");

        let db = repo.join("target").join("identity.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let canonical = entity_by_file_kind_and_name(
            &store,
            "src/service.ts",
            EntityKind::Function,
            "canonicalName",
        );
        let run =
            entity_by_file_kind_and_name(&store, "src/consumer.ts", EntityKind::Function, "run");
        let calls = store
            .find_edges_by_head_relation(&run.id, RelationKind::Calls)
            .expect("calls from run");

        assert!(
            calls.iter().any(|edge| {
                edge.tail_id == canonical.id
                    && edge.exactness == Exactness::ParserVerified
                    && edge
                        .metadata
                        .get("resolver")
                        .and_then(|value| value.as_str())
                        == Some("static_import_call_target")
            }),
            "CALLS should target the exported canonicalName entity through aliasName"
        );
    }

    #[test]
    fn audit_same_name_only_imported_target_gets_exact_call() {
        let repo = temp_repo("same-name-imported-call");
        fs::write(
            repo.join("src").join("a.ts"),
            "export function chooseUser(id: string) { return `user:${id}`; }\n",
        )
        .expect("write a");
        fs::write(
            repo.join("src").join("b.ts"),
            "export function chooseUser(id: string) { return `billing:${id}`; }\n",
        )
        .expect("write b");
        let import_line = "import { chooseUser } from './a';";
        fs::write(
            repo.join("src").join("main.ts"),
            format!(
                "{import_line}\n\
                 export function handler(id: string) {{\n\
                   return chooseUser(id);\n\
                 }}\n"
            ),
        )
        .expect("write main");

        let db = repo.join("target").join("same-name.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let imported =
            entity_by_file_kind_and_name(&store, "src/a.ts", EntityKind::Function, "chooseUser");
        let distractor =
            entity_by_file_kind_and_name(&store, "src/b.ts", EntityKind::Function, "chooseUser");
        let handler =
            entity_by_file_kind_and_name(&store, "src/main.ts", EntityKind::Function, "handler");
        let file = store
            .list_entities_by_file("src/main.ts")
            .expect("main entities")
            .into_iter()
            .find(|entity| {
                entity.kind == EntityKind::File
                    && (entity.name == "src/main" || entity.name == "src::main")
            })
            .expect("main file entity");
        let calls = store
            .find_edges_by_head_relation(&handler.id, RelationKind::Calls)
            .expect("calls from handler");
        let imports = store
            .find_edges_by_head_relation(&file.id, RelationKind::Imports)
            .expect("imports from main file");

        assert!(calls.iter().any(|edge| {
            edge.tail_id == imported.id
                && edge.exactness == Exactness::ParserVerified
                && edge.confidence == 1.0
        }));
        assert!(
            calls.iter().all(|edge| edge.tail_id != distractor.id),
            "same-name unimported function must not receive a CALLS edge"
        );
        assert!(imports.iter().any(|edge| {
            edge.tail_id == imported.id
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.repo_relative_path == "src/main.ts"
                && edge.source_span.start_line == 1
                && edge.source_span.start_column == Some(1)
                && edge.source_span.end_line == 1
                && edge.source_span.end_column == Some(import_line.chars().count() as u32 + 1)
        }));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_default_import_resolves_to_default_export_when_supported() {
        let repo = temp_repo("default-import-call");
        fs::write(
            repo.join("src").join("service.ts"),
            "export default function canonicalTarget() { return 'ok'; }\n",
        )
        .expect("write service");
        fs::write(
            repo.join("src").join("consumer.ts"),
            "import activeTarget from './service';\n\
             export function run() { return activeTarget(); }\n",
        )
        .expect("write consumer");

        let db = repo.join("target").join("default-import.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let canonical =
            entity_by_file_kind_and_name(&store, "src/service.ts", EntityKind::Function, "default");
        let run =
            entity_by_file_kind_and_name(&store, "src/consumer.ts", EntityKind::Function, "run");
        let calls = store
            .find_edges_by_head_relation(&run.id, RelationKind::Calls)
            .expect("calls from run");

        assert!(calls.iter().any(|edge| {
            edge.tail_id == canonical.id
                && edge.exactness == Exactness::ParserVerified
                && edge
                    .metadata
                    .get("resolver")
                    .and_then(|value| value.as_str())
                    == Some("static_import_call_target")
        }));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_barrel_reexport_resolves_default_and_named_targets_distinctly() {
        let repo = temp_repo("barrel-reexport");
        fs::write(
            repo.join("src").join("defaultFeature.ts"),
            "export default function feature() { return 'default'; }\n",
        )
        .expect("write default feature");
        fs::write(
            repo.join("src").join("namedFeature.ts"),
            "export function feature() { return 'named'; }\n",
        )
        .expect("write named feature");
        fs::write(
            repo.join("src").join("index.ts"),
            "export { default as defaultFeature } from './defaultFeature';\n\
             export { feature as namedFeature } from './namedFeature';\n",
        )
        .expect("write barrel");
        fs::write(
            repo.join("src").join("use.ts"),
            "import { defaultFeature, namedFeature } from './index';\n\
             export function runDefault() { return defaultFeature(); }\n\
             export function runNamed() { return namedFeature(); }\n",
        )
        .expect("write use");

        let db = repo.join("target").join("barrel.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let default_target = entity_by_file_kind_and_name(
            &store,
            "src/defaultFeature.ts",
            EntityKind::Function,
            "default",
        );
        let named_target = entity_by_file_kind_and_name(
            &store,
            "src/namedFeature.ts",
            EntityKind::Function,
            "feature",
        );
        let run_default =
            entity_by_file_kind_and_name(&store, "src/use.ts", EntityKind::Function, "runDefault");
        let run_named =
            entity_by_file_kind_and_name(&store, "src/use.ts", EntityKind::Function, "runNamed");
        let default_calls = store
            .find_edges_by_head_relation(&run_default.id, RelationKind::Calls)
            .expect("runDefault calls");
        let named_calls = store
            .find_edges_by_head_relation(&run_named.id, RelationKind::Calls)
            .expect("runNamed calls");

        assert!(default_calls
            .iter()
            .any(|edge| edge.tail_id == default_target.id));
        assert!(default_calls
            .iter()
            .all(|edge| edge.tail_id != named_target.id));
        assert!(named_calls
            .iter()
            .any(|edge| edge.tail_id == named_target.id));
        assert!(named_calls
            .iter()
            .all(|edge| edge.tail_id != default_target.id));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_local_shadowing_prevents_imported_exact_call() {
        let repo = temp_repo("local-shadow-import");
        fs::write(
            repo.join("src").join("service.ts"),
            "export function target() { return 'imported'; }\n",
        )
        .expect("write service");
        fs::write(
            repo.join("src").join("consumer.ts"),
            "import { target } from './service';\n\
             export function run() {\n\
               function target() { return 'local'; }\n\
               return target();\n\
             }\n",
        )
        .expect("write consumer");

        let db = repo.join("target").join("shadow.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let imported =
            entity_by_file_kind_and_name(&store, "src/service.ts", EntityKind::Function, "target");
        let run =
            entity_by_file_kind_and_name(&store, "src/consumer.ts", EntityKind::Function, "run");
        let calls = store
            .find_edges_by_head_relation(&run.id, RelationKind::Calls)
            .expect("calls from run");

        assert!(
            calls.iter().all(|edge| edge.tail_id != imported.id),
            "import resolver must not add exact CALLS through a shadowed local binding"
        );
        assert!(calls.iter().any(|edge| {
            store
                .get_entity(&edge.tail_id)
                .expect("tail")
                .is_some_and(|entity| {
                    entity.repo_relative_path == "src/consumer.ts" && entity.name == "target"
                })
        }));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_dynamic_import_is_unresolved_heuristic_import_fact() {
        let repo = temp_repo("dynamic-import-unresolved");
        fs::create_dir_all(repo.join("src").join("plugins")).expect("create plugins");
        fs::write(
            repo.join("src").join("loader.ts"),
            concat!(
                "export async function loadPlugin(name: string) {\n",
                "  const mod = await import(\"./plugins/\" + name);\n",
                "  const literal = await import(\"./plugins/alpha\");\n",
                "  return mod.default();\n",
                "}\n",
            ),
        )
        .expect("write loader");
        fs::write(
            repo.join("src").join("plugins").join("alpha.ts"),
            "export default function alpha() { return 'alpha'; }\n",
        )
        .expect("write alpha");

        let proof_db = repo.join("target").join("dynamic-proof.sqlite");
        index_repo_to_db(&repo, &proof_db).expect("proof index");
        let proof_store = SqliteGraphStore::open(&proof_db).expect("proof store");
        assert!(proof_store
            .list_entities_by_file("src/loader.ts")
            .expect("proof loader entities")
            .into_iter()
            .all(|entity| !entity.qualified_name.starts_with("dynamic_import:")));
        let proof_load_plugin = entity_by_file_kind_and_name(
            &proof_store,
            "src/loader.ts",
            EntityKind::Function,
            "loadPlugin",
        );
        assert!(proof_store
            .find_edges_by_head_relation(&proof_load_plugin.id, RelationKind::Imports)
            .expect("proof imports")
            .into_iter()
            .all(|edge| edge.exactness != Exactness::StaticHeuristic));
        drop(proof_store);

        let db = repo.join("target").join("dynamic-audit.sqlite");
        index_repo_to_db_with_options(
            &repo,
            &db,
            IndexOptions {
                storage_mode: StorageMode::Audit,
                ..IndexOptions::default()
            },
        )
        .expect("audit index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let load_plugin = entity_by_file_kind_and_name(
            &store,
            "src/loader.ts",
            EntityKind::Function,
            "loadPlugin",
        );
        let dynamic_import = store
            .list_static_references(UNBOUNDED_STORE_READ_LIMIT)
            .expect("sidecar entities")
            .into_iter()
            .find(|entity| {
                entity.kind == EntityKind::Import
                    && entity.qualified_name == "dynamic_import:./plugins/+name"
            })
            .expect("dynamic import entity");
        let literal_dynamic_import = store
            .list_static_references(UNBOUNDED_STORE_READ_LIMIT)
            .expect("sidecar entities")
            .into_iter()
            .find(|entity| {
                entity.kind == EntityKind::Import
                    && entity.qualified_name == "dynamic_import:./plugins/alpha"
            })
            .expect("literal dynamic import entity");
        let alpha_default = entity_by_file_kind_and_name(
            &store,
            "src/plugins/alpha.ts",
            EntityKind::Function,
            "default",
        );
        let imports = store
            .list_heuristic_edges(UNBOUNDED_STORE_READ_LIMIT)
            .expect("heuristic imports");

        assert!(imports.iter().any(|edge| {
            edge.head_id == load_plugin.id
                && edge.tail_id == dynamic_import.id
                && edge.exactness == Exactness::StaticHeuristic
                && edge.confidence < 1.0
                && edge.source_span.repo_relative_path == "src/loader.ts"
                && edge.source_span.start_line == 2
                && edge.source_span.start_column == Some(21)
                && edge.source_span.end_column == Some(48)
                && edge
                    .metadata
                    .get("resolution")
                    .and_then(|value| value.as_str())
                    == Some("unresolved_dynamic_import")
        }));
        assert!(imports.iter().any(|edge| {
            edge.head_id == load_plugin.id
                && edge.tail_id == literal_dynamic_import.id
                && edge.exactness == Exactness::StaticHeuristic
                && edge.confidence < 1.0
                && edge.source_span.repo_relative_path == "src/loader.ts"
                && edge.source_span.start_line == 3
                && edge
                    .metadata
                    .get("resolution")
                    .and_then(|value| value.as_str())
                    == Some("unresolved_dynamic_import")
        }));
        assert!(imports.iter().all(|edge| {
            !(edge.tail_id == alpha_default.id && edge.exactness == Exactness::ParserVerified)
        }));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_duplicate_file_content_keeps_separate_file_identity() {
        let repo = temp_repo("duplicate-file-identity");
        let source = "export function duplicated() { return 'same'; }\n";
        fs::write(repo.join("src").join("first.ts"), source).expect("write first");
        fs::write(repo.join("src").join("second.ts"), source).expect("write second");

        let db = repo.join("target").join("identity.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let first = store
            .get_file("src/first.ts")
            .expect("first file")
            .expect("first");
        let second = store
            .get_file("src/second.ts")
            .expect("second file")
            .expect("second");

        assert_eq!(first.file_hash, second.file_hash);
        assert_ne!(first.repo_relative_path, second.repo_relative_path);
        assert_eq!(store.count_files().expect("files"), 2);

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn duplicate_content_uses_template_overlay_with_path_specific_imports() {
        let repo = temp_repo("content-template-overlay");
        fs::create_dir_all(repo.join("src").join("runtime")).expect("runtime dir");
        fs::create_dir_all(repo.join("src").join("upstream")).expect("upstream dir");
        let shared = "export function duplicated() { return 'same'; }\n";
        fs::write(repo.join("src").join("runtime").join("lib.ts"), shared).expect("runtime lib");
        fs::write(repo.join("src").join("upstream").join("lib.ts"), shared).expect("upstream lib");
        fs::write(
            repo.join("src").join("use-runtime.ts"),
            "import { duplicated } from './runtime/lib';\nexport function callRuntime() { return duplicated(); }\n",
        )
        .expect("runtime importer");
        fs::write(
            repo.join("src").join("use-upstream.ts"),
            "import { duplicated } from './upstream/lib';\nexport function callUpstream() { return duplicated(); }\n",
        )
        .expect("upstream importer");

        let db = repo.join("target").join("template.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");

        let runtime_entities = store
            .list_entities_by_file("src/runtime/lib.ts")
            .expect("runtime entities");
        let upstream_entities = store
            .list_entities_by_file("src/upstream/lib.ts")
            .expect("upstream entities");
        let runtime_duplicate = runtime_entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "duplicated")
            .expect("runtime duplicated");
        let upstream_duplicate = upstream_entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "duplicated")
            .expect("upstream duplicated");

        assert_eq!(runtime_duplicate.file_hash, upstream_duplicate.file_hash);
        assert_ne!(runtime_duplicate.id, upstream_duplicate.id);
        assert_ne!(
            runtime_duplicate.repo_relative_path,
            upstream_duplicate.repo_relative_path
        );
        assert!(
            store
                .content_template_entity_count_for_file("src/runtime/lib.ts")
                .expect("runtime template count")
                > 0
        );
        assert!(
            store
                .content_template_entity_count_for_file("src/upstream/lib.ts")
                .expect("upstream template count")
                > 0
        );

        let runtime_caller = entity_by_file_kind_and_name(
            &store,
            "src/use-runtime.ts",
            EntityKind::Function,
            "callRuntime",
        );
        let upstream_caller = entity_by_file_kind_and_name(
            &store,
            "src/use-upstream.ts",
            EntityKind::Function,
            "callUpstream",
        );
        let runtime_calls = store
            .find_edges_by_head_relation(&runtime_caller.id, RelationKind::Calls)
            .expect("runtime calls");
        let upstream_calls = store
            .find_edges_by_head_relation(&upstream_caller.id, RelationKind::Calls)
            .expect("upstream calls");

        assert!(runtime_calls.iter().any(|edge| {
            edge.tail_id == runtime_duplicate.id && edge.exactness == Exactness::ParserVerified
        }));
        assert!(upstream_calls.iter().any(|edge| {
            edge.tail_id == upstream_duplicate.id && edge.exactness == Exactness::ParserVerified
        }));
        assert!(runtime_calls
            .iter()
            .all(|edge| edge.tail_id != upstream_duplicate.id));
        assert!(upstream_calls
            .iter()
            .all(|edge| edge.tail_id != runtime_duplicate.id));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    #[ignore = "audit gap: same-content rename detection and old/new identity mapping are not implemented yet"]
    fn audit_rename_same_content_preserves_semantic_identity_or_records_mapping() {
        let repo = temp_repo("rename-identity");
        fs::write(
            repo.join("src").join("old_name.ts"),
            "export function stableName() { return 'ok'; }\n",
        )
        .expect("write old");

        let db = repo.join("target").join("identity.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let old_entity = entity_by_file_kind_and_name(
            &store,
            "src/old_name.ts",
            EntityKind::Function,
            "stableName",
        );
        drop(store);

        fs::rename(
            repo.join("src").join("old_name.ts"),
            repo.join("src").join("new_name.ts"),
        )
        .expect("rename");
        index_repo_to_db(&repo, &db).expect("reindex after rename");
        let store = SqliteGraphStore::open(&db).expect("store");
        let new_entity = entity_by_file_kind_and_name(
            &store,
            "src/new_name.ts",
            EntityKind::Function,
            "stableName",
        );

        assert_eq!(
            new_entity.id, old_entity.id,
            "same-content rename should either preserve semantic identity or expose an old/new mapping"
        );
    }

    #[test]
    fn audit_deleted_file_removes_stale_entities_and_edges() {
        let repo = temp_repo("delete-stale");
        let file = repo.join("src").join("auth.ts");
        fs::write(
            &file,
            "function sanitize(value: string) { return value.trim(); }\n\
             export function login(input: string) { return sanitize(input); }\n",
        )
        .expect("write auth");

        let db = repo.join("target").join("identity.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        fs::remove_file(&file).expect("delete auth");

        let summary = update_changed_files_to_db(&repo, &[PathBuf::from("src/auth.ts")], &db)
            .expect("delete update");
        let store = SqliteGraphStore::open(&db).expect("store");
        let edges = store.list_edges(UNBOUNDED_STORE_READ_LIMIT).expect("edges");

        assert_eq!(summary.files_deleted, 1);
        assert_eq!(summary.deleted_file_facts_removed, 1);
        assert_eq!(
            summary
                .path_cleanup_reasons
                .get("src/auth.ts")
                .cloned()
                .unwrap_or_default(),
            vec!["deleted".to_string()]
        );
        assert!(store.get_file("src/auth.ts").expect("file").is_none());
        assert!(store
            .list_entities_by_file("src/auth.ts")
            .expect("entities")
            .is_empty());
        assert!(edges
            .iter()
            .all(|edge| edge.source_span.repo_relative_path != "src/auth.ts"));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn update_newly_ignored_file_deletes_stale_facts_before_ignore_skip() {
        let repo = temp_repo("newly-ignored-stale");
        write_test_file(
            &repo,
            "generated/now_ignored.ts",
            "export function stale_generated_symbol() { return 1; }\n",
        );
        let db = repo.join("target").join("newly-ignored.sqlite");
        index_repo_to_db(&repo, &db).expect("initial index");
        let store = SqliteGraphStore::open(&db).expect("store");
        assert!(store
            .get_file("generated/now_ignored.ts")
            .expect("file lookup")
            .is_some());
        assert!(!store
            .list_entities_by_file("generated/now_ignored.ts")
            .expect("entities")
            .is_empty());
        drop(store);

        write_test_file(&repo, ".gitignore", "generated/\n");
        let summary =
            update_changed_files_to_db(&repo, &[PathBuf::from("generated/now_ignored.ts")], &db)
                .expect("update newly ignored file");
        let store = SqliteGraphStore::open(&db).expect("store");

        assert_eq!(
            summary.files_deleted, 1,
            "existing facts must be deleted before the ignored-path skip is counted"
        );
        assert_eq!(summary.ignored_paths_seen, 1);
        assert_eq!(summary.ignored_paths_with_existing_facts, 1);
        assert_eq!(summary.stale_facts_deleted_for_ignored_paths, 1);
        assert_eq!(
            summary
                .path_cleanup_reasons
                .get("generated/now_ignored.ts")
                .cloned()
                .unwrap_or_default(),
            vec!["now_ignored".to_string(), "scope_changed".to_string()]
        );
        assert!(store
            .get_file("generated/now_ignored.ts")
            .expect("file lookup")
            .is_none());
        assert!(store
            .list_entities_by_file("generated/now_ignored.ts")
            .expect("entities")
            .is_empty());

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn update_ignored_path_without_existing_facts_is_skipped_without_cleanup() {
        let repo = temp_repo("ignored-no-existing-facts");
        write_test_file(&repo, ".gitignore", "generated/\n");
        write_test_file(
            &repo,
            "src/visible.ts",
            "export function visible_symbol() { return 1; }\n",
        );
        write_test_file(
            &repo,
            "generated/new_ignored.ts",
            "export function never_indexed_ignored_symbol() { return 1; }\n",
        );
        let db = repo.join("target").join("ignored-no-facts.sqlite");
        index_repo_to_db(&repo, &db).expect("initial index");

        let summary =
            update_changed_files_to_db(&repo, &[PathBuf::from("generated/new_ignored.ts")], &db)
                .expect("update ignored path without facts");

        assert_eq!(summary.ignored_paths_seen, 1);
        assert_eq!(summary.ignored_paths_with_existing_facts, 0);
        assert_eq!(summary.stale_facts_deleted_for_ignored_paths, 0);
        assert_eq!(summary.deleted_fact_files, 0);
        assert!(summary.path_cleanup_reasons.is_empty());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn update_uses_non_default_passport_scope_for_ignored_path() {
        let repo = temp_repo("update-passport-scope");
        write_test_file(&repo, ".gitignore", "ignored.ts\n");
        write_test_file(
            &repo,
            "ignored.ts",
            "export function ignored_scope_symbol() { return 1; }\n",
        );
        let db = repo.join("target").join("passport-scope-update.sqlite");
        let mut options = IndexOptions::default();
        options.scope.include_ignored = true;
        index_repo_to_db_with_options(&repo, &db, options).expect("index include-ignored");

        write_test_file(
            &repo,
            "ignored.ts",
            "export function ignored_scope_symbol_v2() { return 2; }\n",
        );
        let summary = update_changed_files_to_db(&repo, &[PathBuf::from("ignored.ts")], &db)
            .expect("update uses passport scope");
        let store = SqliteGraphStore::open(&db).expect("store");

        assert_eq!(summary.files_ignored, 0);
        assert_eq!(summary.ignored_paths_seen, 0);
        assert_eq!(summary.files_indexed, 1);
        assert!(!store
            .find_entities_by_exact_symbol("ignored_scope_symbol_v2")
            .expect("new symbol")
            .is_empty());
        assert!(store
            .find_entities_by_exact_symbol("ignored_scope_symbol")
            .expect("old symbol")
            .is_empty());

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn update_rejects_repo_root_mismatch_before_cleanup() {
        let repo_a = temp_repo("update-root-a");
        let repo_b = temp_repo("update-root-b");
        write_test_file(
            &repo_a,
            "src/auth.ts",
            "export function login() { return 1; }\n",
        );
        write_test_file(
            &repo_b,
            "src/auth.ts",
            "export function login() { return 2; }\n",
        );
        let db = repo_a.join("target").join("root-mismatch.sqlite");
        index_repo_to_db(&repo_a, &db).expect("index repo A");

        let error = update_changed_files_to_db(&repo_b, &[PathBuf::from("src/auth.ts")], &db)
            .expect_err("repo B must not update repo A DB");
        assert!(
            error.to_string().contains("repo root mismatch"),
            "error={error}"
        );

        fs::remove_dir_all(repo_a).expect("cleanup repo A");
        fs::remove_dir_all(repo_b).expect("cleanup repo B");
    }

    #[test]
    fn audit_test_mock_symbol_does_not_overwrite_production_symbol() {
        let repo = temp_repo("test-mock-symbol");
        fs::write(
            repo.join("src").join("auth.ts"),
            "export function login() { return 'prod'; }\n",
        )
        .expect("write auth");
        fs::write(
            repo.join("src").join("auth.test.ts"),
            "import { login as realLogin } from './auth';\n\
             const login = vi.fn();\n\
             export function exercise() { return realLogin(); }\n",
        )
        .expect("write test");

        let db = repo.join("target").join("identity.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let production =
            entity_by_file_kind_and_name(&store, "src/auth.ts", EntityKind::Function, "login");
        let test_login = entity_by_file_kind_and_name(
            &store,
            "src/auth.test.ts",
            EntityKind::LocalVariable,
            "login",
        );

        assert_ne!(production.id, test_login.id);
        assert_eq!(production.repo_relative_path, "src/auth.ts");
        assert_eq!(test_login.repo_relative_path, "src/auth.test.ts");

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_static_test_mock_links_to_production_without_overwrite() {
        let repo = temp_repo("test-mock-production-target");
        fs::create_dir_all(repo.join("tests")).expect("create tests");
        fs::write(
            repo.join("src").join("checkout.ts"),
            "import { chargeCard } from './service';\n\
             export function checkout(total: number) { return chargeCard(total); }\n",
        )
        .expect("write checkout");
        fs::write(
            repo.join("src").join("service.ts"),
            "export function chargeCard(total: number) { return `charged:${total}`; }\n",
        )
        .expect("write service");
        fs::write(
            repo.join("tests").join("checkout.test.ts"),
            "import { expect, it, vi } from 'vitest';\n\
             import { checkout } from '../src/checkout';\n\
             import { chargeCard } from '../src/service';\n\
             vi.mock('../src/service', () => ({\n\
               chargeCard: vi.fn(() => 'mocked')\n\
             }));\n\
             it('uses a test double for chargeCard', () => {\n\
               expect(checkout(5)).toBe('mocked');\n\
             });\n",
        )
        .expect("write test");

        let db = repo.join("target").join("mock.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let production = entity_by_file_kind_and_name(
            &store,
            "src/service.ts",
            EntityKind::Function,
            "chargeCard",
        );
        let checkout = entity_by_file_kind_and_name(
            &store,
            "src/checkout.ts",
            EntityKind::Function,
            "checkout",
        );
        let test_case = entity_by_file_kind_and_name(
            &store,
            "tests/checkout.test.ts",
            EntityKind::TestCase,
            "uses a test double for chargeCard",
        );
        let mock = entity_by_file_kind_and_name(
            &store,
            "tests/checkout.test.ts",
            EntityKind::Mock,
            "chargeCardMock",
        );
        let mock_edges = store
            .find_edges_by_head_relation(&test_case.id, RelationKind::Mocks)
            .expect("mock edges");
        let stub_edges = store
            .find_edges_by_head_relation(&mock.id, RelationKind::Stubs)
            .expect("stub edges");
        let assert_edges = store
            .find_edges_by_head_relation(&test_case.id, RelationKind::Asserts)
            .expect("assert edges");
        let test_edges = store
            .find_edges_by_head_relation(&test_case.id, RelationKind::Tests)
            .expect("test edges");

        assert_ne!(production.id, mock.id);
        assert!(mock_edges.iter().any(|edge| {
            edge.tail_id == production.id
                && matches!(edge.edge_class, EdgeClass::Test | EdgeClass::Mock)
                && matches!(edge.context, EdgeContext::Test | EdgeContext::Mock)
        }));
        assert!(stub_edges.iter().any(|edge| {
            edge.tail_id == production.id
                && edge.edge_class == EdgeClass::Mock
                && edge.context == EdgeContext::Mock
        }));
        assert!(assert_edges.iter().any(|edge| {
            edge.tail_id == checkout.id
                && edge.edge_class == EdgeClass::Test
                && edge.context == EdgeContext::Test
                && edge.source_span.repo_relative_path == "tests/checkout.test.ts"
        }));
        assert!(test_edges.iter().any(|edge| {
            edge.tail_id == checkout.id
                && edge.edge_class == EdgeClass::Test
                && edge.context == EdgeContext::Test
        }));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn cold_index_writes_atomic_integrity_clean_db() {
        let repo = temp_repo("cold-integrity");
        fs::write(
            repo.join("src").join("auth.ts"),
            "export function login() { return 'ok'; }\n",
        )
        .expect("write auth");

        let db = repo.join("target").join("cold.sqlite");
        assert!(!db.exists());
        let summary = index_repo_to_db(&repo, &db).expect("cold index");

        assert_eq!(summary.files_indexed, 1);
        assert!(db.exists());
        assert_db_integrity(&db);
        assert_no_atomic_temp_dbs(&db);

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn proof_build_only_cold_index_uses_quick_publish_checks_and_is_queryable() {
        let repo = temp_repo("proof-build-only");
        fs::write(
            repo.join("src").join("auth.ts"),
            "export function login() { return 'ok'; }\n",
        )
        .expect("write auth");

        let db = repo.join("target").join("proof-only.sqlite");
        let summary = index_repo_to_db_with_options(
            &repo,
            &db,
            IndexOptions {
                profile: true,
                build_mode: IndexBuildMode::ProofBuildOnly,
                ..IndexOptions::default()
            },
        )
        .expect("proof build only index");

        assert_eq!(summary.build_mode, "proof-build-only");
        let profile = summary.profile.as_ref().expect("profile");
        assert!(
            profile.spans.iter().any(|span| span.name == "quick_check"),
            "proof-build-only should publish with quick checks"
        );
        assert!(
            profile
                .spans
                .iter()
                .any(|span| span.name == "atomic_cold_bulk_pragmas"),
            "hidden cold temp builds should use the proof-build-only bulk-load profile"
        );
        assert!(
            !profile
                .spans
                .iter()
                .any(|span| span.name == "integrity_check" && span.count > 0),
            "proof-build-only should not run full integrity_check in the fast path"
        );
        let store = SqliteGraphStore::open(&db).expect("store");
        let entities = store
            .list_entities(UNBOUNDED_STORE_READ_LIMIT)
            .expect("entities");
        assert!(
            entities.iter().any(|entity| entity.name == "login"),
            "proof-build-only DB should be queryable"
        );
        store.quick_integrity_gate().expect("quick integrity");
        drop(store);

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn cold_index_persists_buildroot_text_evidence_without_graph_proof_pollution() {
        let repo = temp_repo("buildroot-text-evidence");
        write_test_file(
            &repo,
            "package/foo/foo.mk",
            "################################################################################\n\
             # foo\n\
             ################################################################################\n\n\
             FOO_VERSION = 1.2.3\n\
             FOO_SITE = https://example.com/foo\n\
             FOO_LICENSE = MIT\n\
             FOO_DEPENDENCIES = bar host-baz\n\n\
             $(eval $(generic-package))\n",
        );
        write_test_file(
            &repo,
            "package/foo/Config.in",
            "config BR2_PACKAGE_FOO\n\
             \tbool \"foo\"\n\
             \tdepends on BR2_USE_MMU\n\
             \tselect BR2_PACKAGE_BAR\n",
        );
        write_test_file(
            &repo,
            "docs/manual/adding-packages.adoc",
            "= Adding packages\n\n\
             Buildroot package infrastructure is documented here.\n\n\
             A package using generic-package normally has a Config.in entry.\n",
        );
        write_test_file(
            &repo,
            "support/scripts/pkg-stats",
            "#!/bin/sh\n\
             echo \"pkg-stats package infrastructure\"\n\
             echo \"generic-package Config.in BR2_PACKAGE_FOO\"\n",
        );
        write_test_file(
            &repo,
            "README.md",
            "# Buildroot Text Evidence Mini Fixture\n\n\
             Stage 0 text evidence fixture, not graph proof.\n",
        );
        write_test_file(
            &repo,
            "src/download.c",
            "int download_archive(const char *url) {\n\
             \treturn url != 0;\n\
             }\n",
        );

        let db = repo.join("target").join("buildroot-text.sqlite");
        let summary = index_repo_to_db_with_options(
            &repo,
            &db,
            IndexOptions {
                profile: true,
                ..IndexOptions::default()
            },
        )
        .expect("index buildroot mini fixture");
        assert_eq!(summary.files_indexed, 6);
        assert_eq!(summary.files_parsed, 1, "only the C file should be parsed");

        let store = SqliteGraphStore::open(&db).expect("store");
        assert_text_evidence_file(
            &store,
            "package/foo/foo.mk",
            TextEvidenceFileKind::MakefileFragment,
        );
        assert_text_evidence_file(
            &store,
            "package/foo/Config.in",
            TextEvidenceFileKind::Kconfig,
        );
        assert_text_evidence_file(
            &store,
            "docs/manual/adding-packages.adoc",
            TextEvidenceFileKind::Asciidoc,
        );
        assert_text_evidence_file(
            &store,
            "support/scripts/pkg-stats",
            TextEvidenceFileKind::TextLikeSupportScript,
        );
        assert_text_evidence_file(&store, "README.md", TextEvidenceFileKind::Markdown);

        let c_file = store
            .get_file("src/download.c")
            .expect("c file lookup")
            .expect("c file");
        assert_eq!(c_file.language.as_deref(), Some("c"));
        assert!(store
            .list_entities_by_file("src/download.c")
            .expect("c entities")
            .iter()
            .any(|entity| entity.name == "download_archive"));

        let generic_hits = store
            .search_text("generic-package", 20)
            .expect("generic-package search");
        assert!(generic_hits
            .iter()
            .any(|hit| hit.repo_relative_path == "package/foo/foo.mk"));
        assert!(generic_hits
            .iter()
            .any(|hit| hit.repo_relative_path == "docs/manual/adding-packages.adoc"));
        assert!(generic_hits.iter().any(|hit| {
            hit.kind == TextSearchKind::Snippet && hit.repo_relative_path == "package/foo/foo.mk"
        }));

        let config_hits = store
            .search_text("BR2_PACKAGE_FOO", 20)
            .expect("BR2 search");
        assert!(config_hits
            .iter()
            .any(|hit| hit.repo_relative_path == "package/foo/Config.in"));

        let text_paths = BTreeSet::from([
            "package/foo/foo.mk",
            "package/foo/Config.in",
            "docs/manual/adding-packages.adoc",
            "support/scripts/pkg-stats",
            "README.md",
        ]);
        for edge in store.list_edges(UNBOUNDED_STORE_READ_LIMIT).expect("edges") {
            assert!(
                !text_paths.contains(edge.source_span.repo_relative_path.as_str()),
                "text evidence file produced graph edge: {:?}",
                edge
            );
        }

        assert_db_integrity(&db);
        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn vector_embedding_function_chunk_has_span_entity_and_no_graph_proof() {
        let source = "/// Auth login entry point\nexport function login() {\n  return true;\n}\n";
        let mut metadata = Metadata::default();
        metadata.insert("evidence_role".to_string(), serde_json::json!("production"));
        let entity = Entity {
            id: "entity://src/auth.ts/login".to_string(),
            kind: EntityKind::Function,
            name: "login".to_string(),
            qualified_name: "auth::login".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(SourceSpan::new("src/auth.ts", 2, 4)),
            content_hash: Some(content_hash("auth::login")),
            file_hash: Some(content_hash(source)),
            created_from: "parser:test".to_string(),
            confidence: 1.0,
            metadata,
        };

        let chunks =
            extract_graph_entity_embedding_chunks(&entity, Some(source), Some("typescript"), None);
        let function_chunk = chunks
            .iter()
            .find(|chunk| chunk.chunk_kind == VectorEmbeddingChunkKind::Function)
            .expect("function chunk");
        assert_eq!(
            function_chunk.source_kind,
            VectorEmbeddingChunkSourceKind::GraphEntity
        );
        assert_eq!(
            function_chunk.entity_id.as_deref(),
            Some("entity://src/auth.ts/login")
        );
        assert_eq!(
            function_chunk
                .source_span
                .as_ref()
                .map(|span| span.repo_relative_path.as_str()),
            Some("src/auth.ts")
        );
        assert_eq!(function_chunk.evidence_role, "production");
        assert_eq!(function_chunk.proof_status, "candidate_only");
        assert!(!function_chunk.graph_proof);
        assert!(!function_chunk.claimable_for_graph);
        assert!(function_chunk.byte_count <= VECTOR_EMBEDDING_CHUNK_MAX_TEXT_BYTES);
        assert!(chunks
            .iter()
            .any(|chunk| chunk.chunk_kind == VectorEmbeddingChunkKind::DocComment));
    }

    #[test]
    fn vector_embedding_text_evidence_chunks_preserve_spans_and_labels() {
        let chunks = extract_text_evidence_embedding_chunks_for_path(
            "package/foo/Config.in",
            "config BR2_PACKAGE_FOO\n\tbool \"foo\"\n\tdepends on BR2_USE_MMU\n",
            None,
        );
        let config_chunk = chunks
            .iter()
            .find(|chunk| chunk.text.contains("BR2_PACKAGE_FOO"))
            .expect("BR2_PACKAGE_FOO chunk");
        assert_eq!(
            config_chunk.source_kind,
            VectorEmbeddingChunkSourceKind::TextEvidence
        );
        assert_eq!(config_chunk.chunk_kind, VectorEmbeddingChunkKind::Snippet);
        assert_eq!(config_chunk.evidence_role, "text_evidence");
        assert_eq!(config_chunk.source_role, "text_evidence");
        assert_eq!(config_chunk.proof_status, "not_graph_proof");
        assert!(!config_chunk.graph_proof);
        assert!(!config_chunk.claimable_for_graph);
        assert_eq!(config_chunk.file_kind.as_deref(), Some("kconfig"));
        assert!(config_chunk.source_span.is_some());
    }

    #[test]
    fn vector_embedding_makefile_chunk_includes_generic_package_token() {
        let chunks = extract_text_evidence_embedding_chunks_for_path(
            "package/foo/foo.mk",
            "FOO_VERSION = 1.2.3\nFOO_LICENSE = MIT\n$(eval $(generic-package))\n",
            None,
        );
        assert!(chunks
            .iter()
            .any(|chunk| chunk.text.contains("generic-package")));
        assert!(chunks.iter().all(|chunk| {
            chunk.evidence_role == "text_evidence"
                && chunk.proof_status == "not_graph_proof"
                && !chunk.graph_proof
        }));
    }

    #[test]
    fn vector_embedding_no_extension_support_script_chunks_when_scoped_and_text_like() {
        let chunks = extract_text_evidence_embedding_chunks_for_path(
            "support/scripts/pkg-stats",
            "#!/bin/sh\necho \"generic-package Config.in BR2_PACKAGE_FOO\"\n",
            None,
        );
        let script_chunk = chunks
            .iter()
            .find(|chunk| chunk.text.contains("generic-package"))
            .expect("no-extension support script chunk");
        assert_eq!(
            script_chunk.file_kind.as_deref(),
            Some("text_like_support_script")
        );
        assert!(script_chunk.source_span.is_some());
    }

    #[test]
    fn vector_embedding_chunk_ids_are_stable_and_chunk_text_is_bounded() {
        let long_line = format!("{} {}", "BR2_PACKAGE_FOO", "x".repeat(4096));
        let source = format!("{long_line}\n$(eval $(generic-package))\n");
        let first =
            extract_text_evidence_embedding_chunks_for_path("package/foo/Config.in", &source, None);
        let second =
            extract_text_evidence_embedding_chunks_for_path("package/foo/Config.in", &source, None);
        let first_ids = first
            .iter()
            .map(|chunk| chunk.chunk_id.clone())
            .collect::<Vec<_>>();
        let second_ids = second
            .iter()
            .map(|chunk| chunk.chunk_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(first_ids, second_ids);
        assert!(first
            .iter()
            .all(|chunk| chunk.byte_count <= VECTOR_EMBEDDING_CHUNK_MAX_TEXT_BYTES));
    }

    #[test]
    fn vector_chunk_index_buildroot_mini_text_evidence_is_indexed_and_searchable() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-buildroot-mini");
        let options = VectorChunkIndexBuildOptions {
            max_chunks: 16,
            source_scope: "scope-buildroot-mini".to_string(),
            extraction_version: VECTOR_EMBEDDING_CHUNK_EXTRACTION_VERSION.to_string(),
        };
        let mut chunks = Vec::new();
        chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "package/foo/Config.in",
            "config BR2_PACKAGE_FOO\n\tbool \"foo\"\n\tdepends on BR2_USE_MMU\n",
            None,
        ));
        chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "package/foo/foo.mk",
            "FOO_VERSION = 1.2.3\nFOO_LICENSE = MIT\n$(eval $(generic-package))\n",
            None,
        ));
        chunks.push(extract_file_path_title_embedding_chunk_for_path(
            "package/foo/Config.in",
            TEXT_EVIDENCE_KIND,
            Some("kconfig"),
            None,
        ));

        let index = build_in_memory_vector_chunk_index(
            chunks.clone(),
            &provider,
            &passport,
            options.clone(),
        )
        .expect("build vector chunk index");

        assert!(index.len() >= 3, "indexed chunks: {}", index.len());
        assert!(index
            .entries()
            .any(|entry| entry.chunk.source_kind == VectorEmbeddingChunkSourceKind::TextEvidence));
        assert!(index
            .entries()
            .any(|entry| entry.chunk.chunk_kind == VectorEmbeddingChunkKind::FilePathTitle));
        assert_eq!(
            index.metadata().provider.model_id,
            provider.metadata().model_id
        );
        assert_eq!(index.metadata().provider.dimension, 64);
        assert_eq!(
            index.metadata().passport.index_scope_policy_hash,
            "scope-buildroot-mini"
        );
        assert_eq!(
            index.metadata().extraction_version,
            VECTOR_EMBEDDING_CHUNK_EXTRACTION_VERSION
        );
        assert!(index.metadata().created_at_unix_ms > 0);
        assert_eq!(index.metadata().estimated_vector_bytes_per_chunk, 64 * 4);
        assert_eq!(
            index.metadata().estimated_vector_bytes,
            index.len() * index.metadata().estimated_vector_bytes_per_chunk
        );
        assert_eq!(
            index.metadata().estimated_f32_payload_bytes,
            index.metadata().estimated_vector_bytes
        );
        assert_eq!(index.metadata().estimated_f32_payload_dim, 64);
        assert_eq!(index.metadata().estimated_f32_payload_count, index.len());
        assert_eq!(
            index.metadata().estimated_vector_bytes_deprecated_alias_for,
            "estimated_f32_payload_bytes"
        );
        assert_eq!(index.metadata().index_artifact_format, "pretty_json");
        assert_eq!(index.metadata().vector_payload_compression, "none");
        assert!(index.metadata().stores_chunk_text);
        assert!(index.metadata().stores_chunk_metadata);
        assert!(!index.metadata().stores_full_source_body);
        assert!(index.metadata().generated_total_chunks >= index.len());
        assert!(index.metadata().generated_text_evidence_chunks >= 1);
        assert_eq!(index.metadata().generated_file_path_title_chunks, 1);
        assert_eq!(index.metadata().generated_metadata_chunks, 1);
        assert_eq!(
            index.metadata().generated_total_chunks,
            index.metadata().generated_text_evidence_chunks
                + index.metadata().generated_graph_entity_chunks
                + index.metadata().generated_metadata_chunks
        );
        assert_eq!(index.metadata().selected_total_chunks, index.len());
        assert_eq!(index.metadata().persisted_total_chunks, index.len());
        assert_eq!(index.metadata().chunk_cap, options.max_chunks);
        assert_eq!(
            index.metadata().chunk_selection_strategy,
            "diversity_ranked_v1"
        );
        assert!(!index.metadata().input_order_cap);
        assert!(index
            .entries()
            .all(|entry| entry.chunk.selection_score.is_some()
                && entry.chunk.selection_bucket.is_some()
                && entry.chunk.selection_reason.is_some()
                && entry.chunk.top_level_dir.is_some()
                && entry.chunk.cap_stage.is_some()));
        assert!(index.metadata().estimated_vector_bytes <= index.metadata().max_chunks * 64 * 4);

        let hits = index
            .search(
                &provider,
                "BR2_PACKAGE_FOO generic-package package config",
                5,
            )
            .expect("search top-k");
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|hit| !hit.graph_proof));
        assert!(hits
            .iter()
            .any(|hit| hit.evidence_role == TEXT_EVIDENCE_KIND));

        let mut bounded_options = options;
        bounded_options.max_chunks = 2;
        let bounded_index =
            build_in_memory_vector_chunk_index(chunks, &provider, &passport, bounded_options)
                .expect("bounded index");
        assert_eq!(bounded_index.len(), 2);
        assert!(bounded_index.metadata().omitted_chunks > 0);
        assert!(bounded_index.metadata().chunk_cap_applied);
        assert!(bounded_index.metadata().generated_total_chunks > bounded_index.len());
        assert_eq!(bounded_index.metadata().persisted_total_chunks, 2);
    }

    #[test]
    fn vector_chunk_selection_diversity_prevents_input_order_starvation() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-selection-starvation");
        let mut options = VectorChunkIndexBuildOptions::new("scope-selection-starvation");
        options.max_chunks = 8;
        let mut chunks = Vec::new();
        for idx in 0..20 {
            chunks.push(extract_file_path_title_embedding_chunk_for_path(
                &format!("docs/board/boot/topic-{idx}.adoc"),
                TEXT_EVIDENCE_KIND,
                Some("adoc"),
                None,
            ));
        }
        chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "package/foo/foo.mk",
            "FOO_VERSION = 1.2.3\nFOO_LICENSE = MIT\n$(eval $(generic-package))\n",
            None,
        ));
        chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "support/scripts/pkg-stats",
            "#!/bin/sh\nprintf 'package support statistics generic-package'\n",
            None,
        ));

        let index = build_in_memory_vector_chunk_index(chunks, &provider, &passport, options)
            .expect("diversity selected index");
        let selected_paths = index
            .entries()
            .map(|entry| entry.chunk.path.as_str())
            .collect::<BTreeSet<_>>();

        assert!(selected_paths.contains("package/foo/foo.mk"));
        assert!(selected_paths.contains("support/scripts/pkg-stats"));
        assert_eq!(
            index.metadata().chunk_selection_strategy,
            "diversity_ranked_v1"
        );
        assert!(!index.metadata().input_order_cap);
        assert!(index.metadata().omitted_by_cap > 0);
        assert!(index
            .metadata()
            .persisted_chunks_by_top_level_dir
            .contains_key("package"));
        assert!(index
            .metadata()
            .persisted_chunks_by_top_level_dir
            .contains_key("support"));
    }

    #[test]
    fn vector_chunk_selection_preserves_source_kind_diversity_under_cap() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-selection-source-kind");
        let mut options = VectorChunkIndexBuildOptions::new("scope-selection-source-kind");
        options.max_chunks = 3;
        let source = "/// login token creation\nexport function loginUser() { return true; }\n";
        let entity = Entity {
            id: "entity://src/auth.ts/loginUser".to_string(),
            kind: EntityKind::Function,
            name: "loginUser".to_string(),
            qualified_name: "auth::loginUser".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(SourceSpan::new("src/auth.ts", 2, 2)),
            content_hash: Some(content_hash("auth::loginUser")),
            file_hash: Some(content_hash(source)),
            created_from: "parser:test".to_string(),
            confidence: 1.0,
            metadata: Metadata::default(),
        };
        let mut chunks =
            extract_graph_entity_embedding_chunks(&entity, Some(source), Some("typescript"), None);
        chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "package/foo/Config.in",
            "config BR2_PACKAGE_FOO\n\tbool \"foo\"\n",
            None,
        ));
        chunks.push(extract_file_path_title_embedding_chunk_for_path(
            "docs/manual/adding-packages.adoc",
            TEXT_EVIDENCE_KIND,
            Some("adoc"),
            None,
        ));

        let index = build_in_memory_vector_chunk_index(chunks, &provider, &passport, options)
            .expect("source-kind diversity index");
        let source_counts = &index.metadata().persisted_chunks_by_source_kind;

        assert!(source_counts.get("graph_entity").copied().unwrap_or(0) >= 1);
        assert!(source_counts.get("text_evidence").copied().unwrap_or(0) >= 1);
        assert!(source_counts.get("metadata").copied().unwrap_or(0) >= 1);
    }

    #[test]
    fn vector_chunk_selection_buildroot_mini_keeps_package_docs_support_and_source() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-selection-buildroot-mini");
        let mut options = VectorChunkIndexBuildOptions::new("scope-selection-buildroot-mini");
        options.max_chunks = 8;
        let mut chunks = Vec::new();
        chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "docs/manual/adding-packages.adoc",
            "package infrastructure documentation generic-package Config.in support scripts\n",
            None,
        ));
        chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "package/foo/foo.mk",
            "FOO_VERSION = 1.2.3\nFOO_LICENSE = MIT\n$(eval $(generic-package))\n",
            None,
        ));
        chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "package/foo/Config.in",
            "config BR2_PACKAGE_FOO\n\tbool \"foo package\"\n",
            None,
        ));
        chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "support/scripts/pkg-stats",
            "#!/bin/sh\nprintf 'support script package statistics'\n",
            None,
        ));
        let source = "int download_package(void) { return 0; }\n";
        let entity = Entity {
            id: "entity://src/download.c/download_package".to_string(),
            kind: EntityKind::Function,
            name: "download_package".to_string(),
            qualified_name: "download_package".to_string(),
            repo_relative_path: "src/download.c".to_string(),
            source_span: Some(SourceSpan::new("src/download.c", 1, 1)),
            content_hash: Some(content_hash("download_package")),
            file_hash: Some(content_hash(source)),
            created_from: "parser:test".to_string(),
            confidence: 1.0,
            metadata: Metadata::default(),
        };
        chunks.extend(extract_graph_entity_embedding_chunks(
            &entity,
            Some(source),
            Some("c"),
            None,
        ));
        for idx in 0..20 {
            chunks.push(extract_file_path_title_embedding_chunk_for_path(
                &format!("board/vendor/board-{idx}.adoc"),
                TEXT_EVIDENCE_KIND,
                Some("adoc"),
                None,
            ));
        }

        let index = build_in_memory_vector_chunk_index(chunks, &provider, &passport, options)
            .expect("buildroot diversity index");
        let selected_paths = index
            .entries()
            .map(|entry| entry.chunk.path.as_str())
            .collect::<BTreeSet<_>>();

        assert!(selected_paths.contains("package/foo/foo.mk"));
        assert!(selected_paths.contains("package/foo/Config.in"));
        assert!(selected_paths.contains("docs/manual/adding-packages.adoc"));
        assert!(selected_paths.contains("support/scripts/pkg-stats"));
        assert!(selected_paths.contains("src/download.c"));
        assert!(index.entries().any(|entry| entry.chunk.entity_id.as_deref()
            == Some("entity://src/download.c/download_package")));
    }

    #[test]
    fn vector_chunk_selection_is_deterministic_and_cap_counts_reconcile() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-selection-deterministic");
        let mut options = VectorChunkIndexBuildOptions::new("scope-selection-deterministic");
        options.max_chunks = 6;
        let mut chunks = Vec::new();
        for idx in 0..18 {
            let path = match idx % 3 {
                0 => format!("docs/manual/topic-{idx}.adoc"),
                1 => format!("package/pkg{idx}/pkg{idx}.mk"),
                _ => format!("support/scripts/tool-{idx}"),
            };
            chunks.extend(extract_text_evidence_embedding_chunks_for_path(
                &path,
                &format!("chunk {idx} package support docs generic-package BR2_PACKAGE_{idx}\n"),
                None,
            ));
        }

        let first = build_in_memory_vector_chunk_index(
            chunks.clone(),
            &provider,
            &passport,
            options.clone(),
        )
        .expect("first selection");
        let second = build_in_memory_vector_chunk_index(chunks, &provider, &passport, options)
            .expect("second selection");
        let first_ids = first
            .entries()
            .map(|entry| entry.chunk.chunk_id.clone())
            .collect::<Vec<_>>();
        let second_ids = second
            .entries()
            .map(|entry| entry.chunk.chunk_id.clone())
            .collect::<Vec<_>>();

        assert_eq!(first_ids, second_ids);
        assert!(first.metadata().persisted_total_chunks <= first.metadata().chunk_cap);
        assert_eq!(
            first.metadata().generated_total_chunks,
            first.metadata().persisted_total_chunks
                + first.metadata().omitted_by_cap
                + first.metadata().omitted_low_signal
        );
        for entry in first.entries() {
            let path_count = first
                .entries()
                .filter(|other| other.chunk.path == entry.chunk.path)
                .count();
            assert!(path_count <= first.metadata().per_file_cap);
        }
        assert!(first.entries().all(|entry| !entry.chunk.graph_proof));
    }

    #[test]
    fn vector_chunk_index_function_chunks_are_indexed_without_graph_proof() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-functions");
        let options = VectorChunkIndexBuildOptions::new("scope-functions");
        let source =
            "/// Auth login entry point\nexport function loginUser() {\n  return true;\n}\n";
        let entity = Entity {
            id: "entity://src/auth.ts/loginUser".to_string(),
            kind: EntityKind::Function,
            name: "loginUser".to_string(),
            qualified_name: "auth::loginUser".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(SourceSpan::new("src/auth.ts", 2, 4)),
            content_hash: Some(content_hash("auth::loginUser")),
            file_hash: Some(content_hash(source)),
            created_from: "parser:test".to_string(),
            confidence: 1.0,
            metadata: Metadata::default(),
        };
        let chunks =
            extract_graph_entity_embedding_chunks(&entity, Some(source), Some("typescript"), None);
        let index = build_in_memory_vector_chunk_index(chunks, &provider, &passport, options)
            .expect("build function vector index");
        assert!(index.entries().any(
            |entry| entry.chunk.entity_id.as_deref() == Some("entity://src/auth.ts/loginUser")
        ));
        assert!(index
            .entries()
            .all(|entry| !entry.chunk.graph_proof && !entry.chunk.claimable_for_graph));

        let hits = index
            .search(&provider, "auth login entry point function", 3)
            .expect("function top-k");
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|hit| {
            hit.chunk.entity_id.as_deref() == Some("entity://src/auth.ts/loginUser")
        }));
        assert!(hits.iter().all(|hit| {
            !hit.graph_proof
                && hit.proof_status == "candidate_only"
                && !hit.chunk.claimable_for_graph
        }));
    }

    #[test]
    fn vector_chunk_index_metadata_invalidates_provider_passport_scope_and_extraction_changes() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-a");
        let options = VectorChunkIndexBuildOptions::new("scope-a");
        let chunks = extract_text_evidence_embedding_chunks_for_path(
            "package/foo/Config.in",
            "config BR2_PACKAGE_FOO\n\tbool \"foo\"\n",
            None,
        );
        let index =
            build_in_memory_vector_chunk_index(chunks, &provider, &passport, options.clone())
                .expect("index");
        assert!(index
            .metadata()
            .is_compatible_with(provider.metadata(), &passport, &options));

        let mut changed_provider = provider.metadata().clone();
        changed_provider.provider_id = "codegraph-deterministic-test-other".to_string();
        assert_eq!(
            index
                .metadata()
                .incompatibility_reason(&changed_provider, &passport, &options),
            Some("provider_id changed".to_string())
        );

        let mut changed_model = provider.metadata().clone();
        changed_model.model_id = "codegraph-deterministic-token-projection-v2".to_string();
        assert_eq!(
            index
                .metadata()
                .incompatibility_reason(&changed_model, &passport, &options),
            Some("model_id changed".to_string())
        );

        let different_provider =
            DeterministicTestEmbeddingProvider::for_tests(32).expect("other provider");
        assert_eq!(
            index.metadata().incompatibility_reason(
                different_provider.metadata(),
                &passport,
                &options
            ),
            Some("dimension changed".to_string())
        );

        let mut changed_passport = passport.clone();
        changed_passport.repo_head = Some("new-head".to_string());
        assert_eq!(
            index.metadata().incompatibility_reason(
                provider.metadata(),
                &changed_passport,
                &options
            ),
            Some("repo_head changed".to_string())
        );

        let mut changed_scope_passport = passport.clone();
        changed_scope_passport.index_scope_policy_hash = "scope-b".to_string();
        assert_eq!(
            index.metadata().incompatibility_reason(
                provider.metadata(),
                &changed_scope_passport,
                &options
            ),
            Some("index_scope_policy_hash changed".to_string())
        );

        let changed_scope_options = VectorChunkIndexBuildOptions::new("scope-b");
        assert_eq!(
            index.metadata().incompatibility_reason(
                provider.metadata(),
                &passport,
                &changed_scope_options
            ),
            Some("source_scope changed".to_string())
        );

        let mut changed_extraction = options;
        changed_extraction.extraction_version = "vector_embedding_chunk_v2".to_string();
        assert_eq!(
            index.metadata().incompatibility_reason(
                provider.metadata(),
                &passport,
                &changed_extraction
            ),
            Some("extraction_version changed".to_string())
        );
    }

    #[test]
    fn vector_chunk_index_json_round_trip_loads_and_rejects_stale_passport() {
        let repo = temp_repo("vector-json-round-trip");
        let index_path = repo.join("artifacts").join("codegraph-vector-chunks.json");
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-json");
        let options = VectorChunkIndexBuildOptions::new("scope-json");
        let chunks = vec![extract_file_path_title_embedding_chunk_for_path(
            "package/foo/foo.mk",
            TEXT_EVIDENCE_KIND,
            Some("buildroot_package_metadata"),
            None,
        )];
        let index =
            build_in_memory_vector_chunk_index(chunks, &provider, &passport, options.clone())
                .expect("build vector chunk index");

        write_vector_chunk_index_json(&index_path, &index).expect("write vector index json");
        let runtime_text = fs::read_to_string(&index_path).expect("runtime sidecar json");
        assert!(runtime_text.contains("\"artifact_kind\":\"vector_runtime_sidecar\""));
        assert!(runtime_text.contains("\"index_artifact_format\":\"compact_json\""));
        assert!(!runtime_text.contains("selection_reason"));
        let loaded = load_vector_chunk_index_json(&index_path, &provider, &passport, options)
            .expect("load vector index json");
        assert_eq!(loaded.metadata().index_artifact_format, "compact_json");
        assert_eq!(loaded.metadata().artifact_kind, "vector_runtime_sidecar");
        let hits = loaded
            .search(&provider, "foo package metadata", 3)
            .expect("search loaded vector index");
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|hit| !hit.graph_proof));

        let stale_passport = vector_test_passport("scope-json-stale");
        let stale = load_vector_chunk_index_json(
            &index_path,
            &provider,
            &stale_passport,
            VectorChunkIndexBuildOptions::new("scope-json"),
        )
        .expect_err("stale passport must be rejected");
        assert!(stale
            .to_string()
            .contains("index_scope_policy_hash changed"));

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn vector_runtime_sidecar_atomic_failpoints_preserve_old_artifact() {
        let repo = temp_repo("vector-runtime-atomic-failpoints");
        let index_path = repo.join("artifacts").join("runtime.json");
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-runtime-atomic");
        let options = VectorChunkIndexBuildOptions::new("scope-runtime-atomic");
        let old_index = build_in_memory_vector_chunk_index(
            vec![extract_file_path_title_embedding_chunk_for_path(
                "src/old.rs",
                "production",
                Some("rust"),
                None,
            )],
            &provider,
            &passport,
            options.clone(),
        )
        .expect("old vector index");
        write_vector_chunk_runtime_sidecar_json(
            &index_path,
            &old_index,
            VectorChunkArtifactFormat::CompactJson,
        )
        .expect("write old runtime");
        let old_bytes = fs::read(&index_path).expect("old runtime bytes");

        let new_index = build_in_memory_vector_chunk_index(
            vec![extract_file_path_title_embedding_chunk_for_path(
                "src/new.rs",
                "production",
                Some("rust"),
                None,
            )],
            &provider,
            &passport,
            options,
        )
        .expect("new vector index");

        for failpoint in [
            "vector_runtime_after_temp_write_before_publish",
            "vector_runtime_during_publish",
        ] {
            let error = with_write_path_chaos_failpoint(failpoint, || {
                write_vector_chunk_runtime_sidecar_json(
                    &index_path,
                    &new_index,
                    VectorChunkArtifactFormat::CompactJson,
                )
            })
            .expect_err("runtime sidecar publish failpoint should fail");
            assert!(
                error.to_string().contains(failpoint),
                "failpoint={failpoint} error={error}"
            );
            assert_eq!(
                fs::read(&index_path).expect("preserved runtime bytes"),
                old_bytes,
                "old runtime sidecar changed after {failpoint}"
            );
            let leftovers = fs::read_dir(index_path.parent().expect("artifact parent"))
                .expect("read artifact parent")
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .filter(|name| name.contains(".tmp-") || name.contains(".backup-"))
                .collect::<Vec<_>>();
            assert!(
                leftovers.is_empty(),
                "partial runtime artifact leftovers after {failpoint}: {leftovers:?}"
            );
        }

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn vector_audit_artifact_atomic_failpoint_preserves_old_artifact() {
        let repo = temp_repo("vector-audit-atomic-failpoints");
        let audit_path = repo.join("artifacts").join("audit.json");
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-audit-atomic");
        let options = VectorChunkIndexBuildOptions::new("scope-audit-atomic");
        let old_index = build_in_memory_vector_chunk_index(
            vec![extract_file_path_title_embedding_chunk_for_path(
                "README.md",
                "production",
                Some("markdown"),
                None,
            )],
            &provider,
            &passport,
            options.clone(),
        )
        .expect("old audit index");
        write_vector_chunk_audit_artifact_json(&audit_path, &old_index).expect("write old audit");
        let old_bytes = fs::read(&audit_path).expect("old audit bytes");

        let new_index = build_in_memory_vector_chunk_index(
            vec![extract_file_path_title_embedding_chunk_for_path(
                "docs/README.md",
                "production",
                Some("markdown"),
                None,
            )],
            &provider,
            &passport,
            options,
        )
        .expect("new audit index");

        let error = with_write_path_chaos_failpoint("vector_audit_during_publish", || {
            write_vector_chunk_audit_artifact_json(&audit_path, &new_index)
        })
        .expect_err("audit artifact publish failpoint should fail");
        assert!(error.to_string().contains("vector_audit_during_publish"));
        assert_eq!(
            fs::read(&audit_path).expect("preserved audit bytes"),
            old_bytes
        );

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn legacy_pretty_vector_artifact_still_loads() {
        let repo = temp_repo("vector-legacy-pretty-load");
        let index_path = repo.join("artifacts").join("legacy-vector.json");
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-legacy");
        let options = VectorChunkIndexBuildOptions::new("scope-legacy");
        let chunks = vec![extract_file_path_title_embedding_chunk_for_path(
            "README.md",
            "production",
            Some("markdown"),
            None,
        )];
        let index =
            build_in_memory_vector_chunk_index(chunks, &provider, &passport, options.clone())
                .expect("build vector chunk index");
        let persisted = persisted_vector_chunk_index_from_index(&index);
        fs::create_dir_all(index_path.parent().expect("parent")).expect("artifact dir");
        fs::write(
            &index_path,
            serde_json::to_vec_pretty(&persisted).expect("legacy pretty json"),
        )
        .expect("write legacy pretty");

        let loaded = load_vector_chunk_index_json(&index_path, &provider, &passport, options)
            .expect("legacy pretty JSON vector artifact should still load");
        assert_eq!(loaded.metadata().index_artifact_format, "pretty_json");
        assert!(!loaded
            .search(&provider, "README", 3)
            .expect("search")
            .is_empty());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_vector_artifact_is_not_runtime_loadable() {
        let repo = temp_repo("vector-audit-not-runtime");
        let audit_path = repo.join("artifacts").join("audit-vector.json");
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-audit-runtime");
        let options = VectorChunkIndexBuildOptions::new("scope-audit-runtime");
        let chunks = vec![extract_file_path_title_embedding_chunk_for_path(
            "README.md",
            "production",
            Some("markdown"),
            None,
        )];
        let index =
            build_in_memory_vector_chunk_index(chunks, &provider, &passport, options.clone())
                .expect("build vector chunk index");
        write_vector_chunk_audit_artifact_json(&audit_path, &index).expect("write audit artifact");

        let error = load_vector_chunk_index_json(&audit_path, &provider, &passport, options)
            .expect_err("audit artifact must not be runtime-loadable");
        assert!(error.to_string().contains("diagnostic_only"));

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn stale_audit_vector_artifact_stays_diagnostic_only() {
        let repo = temp_repo("vector-audit-stale-diagnostic");
        let audit_path = repo.join("artifacts").join("audit-vector.json");
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-audit-fresh");
        let options = VectorChunkIndexBuildOptions::new("scope-audit-fresh");
        let chunks = vec![extract_file_path_title_embedding_chunk_for_path(
            "README.md",
            "production",
            Some("markdown"),
            None,
        )];
        let index = build_in_memory_vector_chunk_index(chunks, &provider, &passport, options)
            .expect("build vector chunk index");
        write_vector_chunk_audit_artifact_json(&audit_path, &index).expect("write audit artifact");

        let stale_passport = vector_test_passport("scope-audit-stale");
        let stale_options = VectorChunkIndexBuildOptions::new("scope-audit-stale");
        let error =
            load_vector_chunk_index_json(&audit_path, &provider, &stale_passport, stale_options)
                .expect_err("stale audit artifact must stay diagnostic-only");
        assert!(error.to_string().contains("diagnostic_only"));

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn vector_chunk_index_update_replaces_changed_text_evidence_chunks() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-update-text");
        let options = VectorChunkIndexBuildOptions::new("scope-update-text");
        let old_chunks = extract_text_evidence_embedding_chunks_for_path(
            "package/foo/foo.mk",
            "FOO_VERSION = 1.0\n$(eval $(generic-package))\n",
            None,
        );
        let old_ids = old_chunks
            .iter()
            .map(|chunk| chunk.chunk_id.clone())
            .collect::<BTreeSet<_>>();
        let mut index =
            build_in_memory_vector_chunk_index(old_chunks, &provider, &passport, options)
                .expect("build old text evidence index");

        let new_chunks = extract_text_evidence_embedding_chunks_for_path(
            "package/foo/foo.mk",
            "FOO_VERSION = 2.0\nFOO_LICENSE = MIT\n$(eval $(host-generic-package))\n",
            None,
        );
        let new_ids = new_chunks
            .iter()
            .map(|chunk| chunk.chunk_id.clone())
            .collect::<BTreeSet<_>>();
        let summary = index
            .replace_chunks_for_path("package/foo/foo.mk", new_chunks, &provider)
            .expect("replace changed text evidence chunks");

        assert!(summary.removed_chunks > 0);
        assert!(summary.inserted_chunks > 0);
        assert!(old_ids
            .iter()
            .all(|old_id| { !index.entries().any(|entry| entry.chunk.chunk_id == *old_id) }));
        assert!(new_ids
            .iter()
            .all(|new_id| { index.entries().any(|entry| entry.chunk.chunk_id == *new_id) }));
        assert!(index
            .entries()
            .all(|entry| entry.chunk.evidence_role == TEXT_EVIDENCE_KIND));
        assert_eq!(
            summary.estimated_vector_bytes,
            index.metadata().estimated_vector_bytes
        );
    }

    #[test]
    fn vector_chunk_index_update_replaces_changed_graph_entity_chunks() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-update-graph");
        let options = VectorChunkIndexBuildOptions::new("scope-update-graph");
        let old_source = "/// Login user\nexport function loginUser() {\n  return true;\n}\n";
        let old_entity = Entity {
            id: "entity://src/auth.ts/loginUser".to_string(),
            kind: EntityKind::Function,
            name: "loginUser".to_string(),
            qualified_name: "auth::loginUser".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(SourceSpan::new("src/auth.ts", 2, 4)),
            content_hash: Some(content_hash("auth::loginUser")),
            file_hash: Some(content_hash(old_source)),
            created_from: "parser:test".to_string(),
            confidence: 1.0,
            metadata: Metadata::default(),
        };
        let old_chunks = extract_graph_entity_embedding_chunks(
            &old_entity,
            Some(old_source),
            Some("typescript"),
            None,
        );
        let old_ids = old_chunks
            .iter()
            .map(|chunk| chunk.chunk_id.clone())
            .collect::<BTreeSet<_>>();
        let mut index =
            build_in_memory_vector_chunk_index(old_chunks, &provider, &passport, options)
                .expect("build old graph entity index");

        let new_source =
            "/// Login user with token refresh\nexport function loginUserToken() {\n  return true;\n}\n";
        let new_entity = Entity {
            id: "entity://src/auth.ts/loginUserToken".to_string(),
            kind: EntityKind::Function,
            name: "loginUserToken".to_string(),
            qualified_name: "auth::loginUserToken".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(SourceSpan::new("src/auth.ts", 2, 4)),
            content_hash: Some(content_hash("auth::loginUserToken")),
            file_hash: Some(content_hash(new_source)),
            created_from: "parser:test".to_string(),
            confidence: 1.0,
            metadata: Metadata::default(),
        };
        let new_chunks = extract_graph_entity_embedding_chunks(
            &new_entity,
            Some(new_source),
            Some("typescript"),
            None,
        );
        let new_ids = new_chunks
            .iter()
            .map(|chunk| chunk.chunk_id.clone())
            .collect::<BTreeSet<_>>();
        let summary = index
            .replace_chunks_for_path("src/auth.ts", new_chunks, &provider)
            .expect("replace changed graph entity chunks");

        assert_eq!(summary.removed_chunks, old_ids.len());
        assert_eq!(summary.inserted_chunks, new_ids.len());
        assert!(old_ids
            .iter()
            .all(|old_id| { !index.entries().any(|entry| entry.chunk.chunk_id == *old_id) }));
        assert!(new_ids
            .iter()
            .all(|new_id| { index.entries().any(|entry| entry.chunk.chunk_id == *new_id) }));
        assert!(index.entries().all(|entry| {
            entry.chunk.proof_status == "candidate_only" && !entry.chunk.graph_proof
        }));
    }

    #[test]
    fn vector_chunk_index_update_removes_deleted_file_chunks() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-delete");
        let options = VectorChunkIndexBuildOptions::new("scope-delete");
        let mut chunks = Vec::new();
        chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "package/foo/foo.mk",
            "FOO_VERSION = 1.0\n$(eval $(generic-package))\n",
            None,
        ));
        chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "package/bar/bar.mk",
            "BAR_VERSION = 1.0\n$(eval $(generic-package))\n",
            None,
        ));
        let mut index = build_in_memory_vector_chunk_index(chunks, &provider, &passport, options)
            .expect("build two-file index");
        let before = index.len();

        let summary = index.remove_chunks_for_path("package/foo/foo.mk");

        assert!(summary.removed_chunks > 0);
        assert!(index.len() < before);
        assert!(index
            .entries()
            .all(|entry| entry.chunk.path != "package/foo/foo.mk"));
        assert!(index
            .entries()
            .any(|entry| entry.chunk.path == "package/bar/bar.mk"));
    }

    #[test]
    fn vector_chunk_source_bindings_reject_changed_and_deleted_files() {
        let repo = temp_repo("vector-source-binding-stale");
        let source = "export function login() { return 'ok'; }\n";
        write_test_file(&repo, "src/auth.ts", source);
        let file = FileRecord {
            repo_relative_path: "src/auth.ts".to_string(),
            file_hash: content_hash(source),
            language: Some("typescript".to_string()),
            size_bytes: source.len() as u64,
            indexed_at_unix_ms: Some(unix_time_ms()),
            metadata: Metadata::default(),
        };
        let entity = Entity {
            id: "entity://src/auth.ts/login".to_string(),
            kind: EntityKind::Function,
            name: "login".to_string(),
            qualified_name: "auth::login".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(SourceSpan::new("src/auth.ts", 1, 1)),
            content_hash: Some(content_hash("auth::login")),
            file_hash: Some(content_hash(source)),
            created_from: "parser:test".to_string(),
            confidence: 1.0,
            metadata: Metadata::default(),
        };
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-source-binding");
        let chunks = bind_vector_chunks_to_source_file(
            extract_graph_entity_embedding_chunks(&entity, Some(source), Some("typescript"), None),
            &file,
        );
        let index = build_in_memory_vector_chunk_index(
            chunks,
            &provider,
            &passport,
            VectorChunkIndexBuildOptions::new("scope-source-binding"),
        )
        .expect("build bound vector index");

        let valid = validate_vector_chunk_source_bindings(&repo, &index)
            .expect("valid source binding check");
        assert!(valid.is_valid(), "{valid:?}");
        assert_eq!(valid.checked_files, 1);

        write_test_file(
            &repo,
            "src/auth.ts",
            "export function loginChanged() { return 'changed'; }\n",
        );
        let changed = validate_vector_chunk_source_bindings(&repo, &index)
            .expect("changed source binding check");
        assert_eq!(changed.status, "stale");
        assert!(changed.stale_reasons.iter().any(|reason| {
            reason.contains("changed_file_size") || reason.contains("changed_file_hash")
        }));

        fs::remove_file(repo.join("src").join("auth.ts")).expect("delete source file");
        let deleted = validate_vector_chunk_source_bindings(&repo, &index)
            .expect("deleted source binding check");
        assert_eq!(deleted.status, "stale");
        assert!(deleted
            .stale_reasons
            .iter()
            .any(|reason| reason.contains("deleted_file:src/auth.ts")));

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn incremental_update_refreshes_passport_and_invalidates_old_runtime_sidecar() {
        let repo = temp_repo("vector-incremental-passport-stale");
        write_test_file(
            &repo,
            "src/auth.ts",
            "export function oldLogin() { return 'old'; }\n",
        );
        let db = repo.join("target").join("codegraph.sqlite");
        let runtime = repo.join("target").join("runtime.json");
        index_repo_to_db(&repo, &db).expect("initial index");
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let vector_options =
            VectorChunkIndexBuildOptions::new("context-pack-release-vector-candidates");
        build_vector_chunk_index_artifacts_for_repo(
            &repo,
            &db,
            &runtime,
            &provider,
            vector_options.clone(),
            VectorChunkIndexArtifactOptions::default(),
        )
        .expect("build runtime sidecar");
        let store = SqliteGraphStore::open(&db).expect("store");
        let before_passport = store
            .get_db_passport()
            .expect("read before passport")
            .expect("before passport");
        load_vector_chunk_index_json(
            &runtime,
            &provider,
            &before_passport,
            vector_options.clone(),
        )
        .expect("runtime sidecar loads before update");
        drop(store);

        write_test_file(
            &repo,
            "src/auth.ts",
            "export function newLogin() { return 'new'; }\n",
        );
        let update = update_changed_files_to_db(&repo, &[PathBuf::from("src/auth.ts")], &db)
            .expect("incremental update");
        assert_eq!(update.files_indexed, 1);
        let store = SqliteGraphStore::open(&db).expect("store after update");
        let after_passport = store
            .get_db_passport()
            .expect("read after passport")
            .expect("after passport");
        assert_ne!(
            db_passport_fingerprint(&before_passport),
            db_passport_fingerprint(&after_passport),
            "incremental DB passport fingerprint must change after graph facts change"
        );
        let stale =
            load_vector_chunk_index_json(&runtime, &provider, &after_passport, vector_options)
                .expect_err("old runtime sidecar must be stale after incremental update");
        assert!(
            stale.to_string().contains("db_passport changed")
                || stale.to_string().contains("repo_head changed"),
            "{stale}"
        );

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn vector_chunk_index_metadata_does_not_store_full_source_bodies() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-no-full-source");
        let options = VectorChunkIndexBuildOptions::new("scope-no-full-source");
        let tail_marker = "FULL_SOURCE_TAIL_MARKER_SHOULD_NOT_APPEAR_IN_VECTOR_METADATA";
        let source = format!(
            "/// Auth login\nexport function login() {{\n  return true;\n}}\n{}\n{}\n",
            "x".repeat(VECTOR_EMBEDDING_CHUNK_MAX_TEXT_BYTES * 4),
            tail_marker
        );
        let entity = Entity {
            id: "entity://src/auth.ts/login".to_string(),
            kind: EntityKind::Function,
            name: "login".to_string(),
            qualified_name: "auth::login".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(SourceSpan::new("src/auth.ts", 2, 4)),
            content_hash: Some(content_hash("auth::login")),
            file_hash: Some(content_hash(&source)),
            created_from: "parser:test".to_string(),
            confidence: 1.0,
            metadata: Metadata::default(),
        };
        let chunks =
            extract_graph_entity_embedding_chunks(&entity, Some(&source), Some("typescript"), None);
        let index = build_in_memory_vector_chunk_index(chunks, &provider, &passport, options)
            .expect("build vector index");
        let metadata_json =
            serde_json::to_string(index.metadata()).expect("serialize vector metadata");

        assert!(!metadata_json.contains(tail_marker));
        assert!(!metadata_json.contains("return true"));
        assert!(index
            .entries()
            .all(|entry| entry.chunk.byte_count <= VECTOR_EMBEDDING_CHUNK_MAX_TEXT_BYTES));
        assert!(index
            .entries()
            .all(|entry| !entry.chunk.text.contains(tail_marker)));
        assert_eq!(index.metadata().estimated_vector_bytes_per_chunk, 64 * 4);
        assert_eq!(
            index.metadata().estimated_vector_bytes,
            index.len() * index.metadata().estimated_vector_bytes_per_chunk
        );
        assert_eq!(
            index.metadata().estimated_f32_payload_bytes,
            index.metadata().estimated_vector_bytes
        );
        assert_eq!(index.metadata().index_artifact_format, "pretty_json");
        assert_eq!(index.metadata().vector_payload_compression, "none");
        assert!(!index.metadata().stores_full_source_body);
    }

    #[test]
    fn vector_chunk_index_candidates_feed_retrieval_text_evidence_fallback() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-buildroot-retrieval");
        let chunks = extract_text_evidence_embedding_chunks_for_path(
            "package/foo/Config.in",
            "config BR2_PACKAGE_FOO\n\tbool \"foo\"\n\tdepends on BR2_USE_MMU\n",
            None,
        );
        let index = build_in_memory_vector_chunk_index(
            chunks,
            &provider,
            &passport,
            VectorChunkIndexBuildOptions::new("scope-buildroot-retrieval"),
        )
        .expect("vector index");
        let hits = index
            .search(
                &provider,
                "Which Buildroot option enables the foo package?",
                8,
            )
            .expect("vector search");
        let candidates = hits
            .iter()
            .map(|hit| {
                vector_chunk_search_hit_to_retrieval_candidate(
                    hit,
                    provider.metadata(),
                    "Which Buildroot option enables the foo package?",
                    None,
                )
            })
            .collect::<Vec<_>>();
        assert!(candidates.iter().any(|candidate| {
            candidate.candidate_source == RetrievalCandidateSource::VectorSemantic
                && !candidate.graph_proof
                && candidate
                    .metadata
                    .get("evidence_role_raw")
                    .and_then(Value::as_str)
                    == Some("text_evidence")
        }));

        let funnel = RetrievalFunnel::new(
            Vec::new(),
            Vec::new(),
            RetrievalFunnelConfig {
                vector_candidate_top_k: 8,
                ..RetrievalFunnelConfig::default()
            },
        )
        .expect("funnel");
        let result = funnel
            .run(
                RetrievalFunnelRequest::new(
                    "Which Buildroot option enables the foo package?",
                    "planning",
                    1_000,
                )
                .enable_vector_candidates(true)
                .vector_branch_status(VectorCandidateBranchStatus::Ready)
                .vector_candidates(candidates),
            )
            .expect("retrieval");

        assert!(result.packet.verified_paths.is_empty());
        assert!(result
            .packet
            .snippets
            .iter()
            .any(|snippet| snippet.file == "package/foo/Config.in"
                && snippet.text.contains("BR2_PACKAGE_FOO")));
        assert_eq!(
            result
                .packet
                .metadata
                .get("proof_status")
                .and_then(Value::as_str),
            Some("no_proof_path_found")
        );
    }

    #[test]
    fn vector_chunk_index_candidates_feed_retrieval_graph_verification() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-graph-retrieval");
        let source = "/// login token creation\nexport function loginUser() {\n  return true;\n}\n";
        let entity = Entity {
            id: "AuthService.login".to_string(),
            kind: EntityKind::Function,
            name: "loginUser".to_string(),
            qualified_name: "AuthService.login".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(SourceSpan::new("src/auth.ts", 2, 4)),
            content_hash: Some(content_hash("AuthService.login")),
            file_hash: Some(content_hash(source)),
            created_from: "parser:test".to_string(),
            confidence: 1.0,
            metadata: Metadata::default(),
        };
        let chunks =
            extract_graph_entity_embedding_chunks(&entity, Some(source), Some("typescript"), None);
        let index = build_in_memory_vector_chunk_index(
            chunks,
            &provider,
            &passport,
            VectorChunkIndexBuildOptions::new("scope-graph-retrieval"),
        )
        .expect("vector index");
        let hits = index
            .search(&provider, "where is login token creation implemented", 8)
            .expect("vector search");
        let candidates = hits
            .iter()
            .map(|hit| {
                vector_chunk_search_hit_to_retrieval_candidate(
                    hit,
                    provider.metadata(),
                    "where is login token creation implemented",
                    None,
                )
            })
            .collect::<Vec<_>>();
        assert!(candidates.iter().any(|candidate| {
            candidate.entity_id.as_deref() == Some("AuthService.login")
                && candidate.requires_graph_verification
                && !candidate.graph_proof
        }));

        let span = SourceSpan::new("src/auth.ts", 3, 3);
        let edge = resolved_import_edge(
            "AuthService.login",
            RelationKind::Calls,
            "TokenStore.create",
            &span,
            &content_hash(source),
            "test_vector_retrieval",
        );
        let funnel = RetrievalFunnel::new(
            vec![edge],
            Vec::new(),
            RetrievalFunnelConfig {
                vector_candidate_top_k: 8,
                ..RetrievalFunnelConfig::default()
            },
        )
        .expect("funnel");
        let result = funnel
            .run(
                RetrievalFunnelRequest::new(
                    "where is login token creation implemented",
                    "impact",
                    1_000,
                )
                .enable_vector_candidates(true)
                .vector_branch_status(VectorCandidateBranchStatus::Ready)
                .vector_candidates(candidates),
            )
            .expect("retrieval");

        assert!(result.packet.verified_paths.iter().any(|path| {
            path.source == "AuthService.login" && path.target == "TokenStore.create"
        }));
        assert!(result
            .vector_candidates
            .iter()
            .all(|candidate| !candidate.graph_proof));
    }

    #[test]
    fn vector_recall_fixture_gate_passes_with_deterministic_provider() {
        let provider = DeterministicTestEmbeddingProvider::for_tests(64).expect("provider");
        let passport = vector_test_passport("scope-vector-recall-fixture-gate");
        let mut text_chunks = Vec::new();
        text_chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "package/foo/foo.mk",
            "FOO_VERSION = 1.2.3\nFOO_LICENSE = MIT\n$(eval $(generic-package))\n",
            None,
        ));
        text_chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "package/foo/Config.in",
            "config BR2_PACKAGE_FOO\n\tbool \"foo package\"\n\tdepends on BR2_USE_MMU\n",
            None,
        ));
        text_chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "docs/manual/adding-packages.adoc",
            "The package infrastructure uses generic-package helpers and Config.in entries.\n",
            None,
        ));
        text_chunks.extend(extract_text_evidence_embedding_chunks_for_path(
            "docs/manual/unrelated.adoc",
            "This chapter describes release notes and unrelated board setup.\n",
            None,
        ));

        let config_prompt =
            "How is package metadata declaring license and generic package build rules?";
        let config_candidates = vector_candidates_from_chunks_for_gate(
            text_chunks.clone(),
            &provider,
            &passport,
            "scope-vector-recall-fixture-gate",
            config_prompt,
            5,
        );
        let config_recall_at_5 = recall_file_at_k(&config_candidates, "package/foo/foo.mk", 5);
        assert_eq!(config_recall_at_5, 1.0, "{config_candidates:?}");

        let docs_prompt =
            "Where does package infrastructure explain generic package helpers and Config.in?";
        let docs_candidates = vector_candidates_from_chunks_for_gate(
            text_chunks.clone(),
            &provider,
            &passport,
            "scope-vector-recall-fixture-gate",
            docs_prompt,
            5,
        );
        let docs_recall_at_5 =
            recall_file_at_k(&docs_candidates, "docs/manual/adding-packages.adoc", 5);
        assert_eq!(docs_recall_at_5, 1.0, "{docs_candidates:?}");

        let function_source = "/// Persists the login token for an authenticated user\nexport function loginUserToken() {\n  return tokenStore.save();\n}\n";
        let entity = Entity {
            id: "AuthService.loginUserToken".to_string(),
            kind: EntityKind::Function,
            name: "loginUserToken".to_string(),
            qualified_name: "AuthService.loginUserToken".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(SourceSpan::new("src/auth.ts", 2, 4)),
            content_hash: Some(content_hash("AuthService.loginUserToken")),
            file_hash: Some(content_hash(function_source)),
            created_from: "parser:test".to_string(),
            confidence: 1.0,
            metadata: Metadata::default(),
        };
        let graph_candidates = vector_candidates_from_chunks_for_gate(
            extract_graph_entity_embedding_chunks(
                &entity,
                Some(function_source),
                Some("typescript"),
                None,
            ),
            &provider,
            &passport,
            "scope-vector-recall-fixture-gate",
            "Where is the authenticated user login token persisted?",
            5,
        );
        let entity_recall_at_5 =
            recall_entity_at_k(&graph_candidates, "AuthService.loginUserToken", 5);
        assert_eq!(entity_recall_at_5, 1.0, "{graph_candidates:?}");
        assert!(graph_candidates
            .iter()
            .filter(|candidate| {
                candidate.entity_id.as_deref() == Some("AuthService.loginUserToken")
            })
            .all(|candidate| candidate.requires_graph_verification && !candidate.graph_proof));

        let no_proof_funnel = RetrievalFunnel::new(
            Vec::new(),
            Vec::new(),
            RetrievalFunnelConfig {
                vector_candidate_top_k: 5,
                ..RetrievalFunnelConfig::default()
            },
        )
        .expect("no-proof funnel");
        let no_proof_result = no_proof_funnel
            .run(
                RetrievalFunnelRequest::new(config_prompt, "planning", 1_000)
                    .enable_vector_candidates(true)
                    .vector_branch_status(VectorCandidateBranchStatus::Ready)
                    .vector_candidates(config_candidates.clone()),
            )
            .expect("no-proof vector retrieval");
        assert!(no_proof_result.packet.verified_paths.is_empty());
        assert_eq!(
            no_proof_result
                .packet
                .metadata
                .get("proof_status")
                .and_then(Value::as_str),
            Some("no_proof_path_found")
        );
        assert_eq!(
            no_proof_result
                .packet
                .metadata
                .get("graph_proof")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(no_proof_result
            .packet
            .snippets
            .iter()
            .any(|snippet| snippet.file == "package/foo/foo.mk"));

        let mut noisy_candidates = Vec::new();
        for index in 0..24 {
            noisy_candidates.push(vector_noise_candidate_for_gate(index));
        }
        let exact_funnel = RetrievalFunnel::new(
            vec![resolved_import_edge(
                "Exact.seed",
                RelationKind::Calls,
                "verified-target",
                &SourceSpan::new("src/exact.ts", 1, 1),
                "hash",
                "vector_recall_fixture_gate",
            )],
            Vec::new(),
            RetrievalFunnelConfig {
                stage1_top_k: 1,
                stage2_top_n: 1,
                vector_candidate_top_k: 1,
                ..RetrievalFunnelConfig::default()
            },
        )
        .expect("exact seed funnel");
        let exact_result = exact_funnel
            .run(
                RetrievalFunnelRequest::new(
                    "semantic package metadata generic package noise",
                    "impact",
                    1_000,
                )
                .exact_seeds(vec!["Exact.seed".to_string()])
                .enable_vector_candidates(true)
                .vector_branch_status(VectorCandidateBranchStatus::Ready)
                .vector_candidates(noisy_candidates),
            )
            .expect("exact seed protected retrieval");
        let exact_seed_preserved = exact_result
            .rerank_scores
            .first()
            .is_some_and(|score| score.id == "Exact.seed" && score.exact_seed)
            && exact_result
                .packet
                .verified_paths
                .iter()
                .any(|path| path.source == "Exact.seed");
        assert!(exact_seed_preserved);

        let all_vector_candidates = config_candidates
            .iter()
            .chain(docs_candidates.iter())
            .chain(graph_candidates.iter())
            .chain(no_proof_result.vector_candidates.iter())
            .cloned()
            .collect::<Vec<_>>();
        assert!(all_vector_candidates
            .iter()
            .all(|candidate| !candidate.graph_proof));

        let config_output_size = serde_json::to_vec(&no_proof_result.packet)
            .expect("serialize no-proof packet")
            .len();
        let exact_output_size = serde_json::to_vec(&exact_result.packet)
            .expect("serialize exact packet")
            .len();
        let metrics = serde_json::json!({
            "gate": "vector_recall_fixture_gate",
            "status": "passed",
            "provider": {
                "provider_id": provider.metadata().provider_id,
                "model_id": provider.metadata().model_id,
                "dimension": provider.metadata().dimension,
                "production_semantic_quality": provider.metadata().production_semantic_quality,
                "source_leaves_machine": provider.metadata().source_leaves_machine
            },
            "recall_at_k": {
                "expected_files": {
                    "package/foo/foo.mk": config_recall_at_5,
                    "docs/manual/adding-packages.adoc": docs_recall_at_5
                },
                "expected_entities": {
                    "AuthService.loginUserToken": entity_recall_at_5
                },
                "k": 5
            },
            "candidate_source_counts": candidate_source_counts(&all_vector_candidates),
            "proof_status_counts": proof_status_counts(&all_vector_candidates),
            "graph_proof_false_until_verified": all_vector_candidates.iter().all(|candidate| !candidate.graph_proof),
            "no_proof_fallback": {
                "proof_status": no_proof_result.packet.metadata.get("proof_status").and_then(Value::as_str),
                "graph_proof": no_proof_result.packet.metadata.get("graph_proof").and_then(Value::as_bool),
                "fallback_file_found": no_proof_result.packet.snippets.iter().any(|snippet| snippet.file == "package/foo/foo.mk")
            },
            "exact_seed": {
                "preserved": exact_seed_preserved,
                "rerank_top_id": exact_result.rerank_scores.first().map(|score| score.id.as_str()),
                "vector_cap": 1
            },
            "output_size_bytes": {
                "no_proof_packet": config_output_size,
                "exact_seed_packet": exact_output_size
            }
        });
        println!(
            "VECTOR_RECALL_FIXTURE_GATE_JSON={}",
            serde_json::to_string(&metrics).expect("metrics json")
        );
    }

    fn vector_candidates_from_chunks_for_gate(
        chunks: Vec<VectorEmbeddingChunk>,
        provider: &DeterministicTestEmbeddingProvider,
        passport: &DbPassport,
        source_scope: &str,
        prompt: &str,
        top_k: usize,
    ) -> Vec<RetrievalCandidate> {
        let index = build_in_memory_vector_chunk_index(
            chunks,
            provider,
            passport,
            VectorChunkIndexBuildOptions {
                max_chunks: 64,
                source_scope: source_scope.to_string(),
                extraction_version: VECTOR_EMBEDDING_CHUNK_EXTRACTION_VERSION.to_string(),
            },
        )
        .expect("fixture vector index");
        index
            .search(provider, prompt, top_k)
            .expect("fixture vector search")
            .iter()
            .map(|hit| {
                vector_chunk_search_hit_to_retrieval_candidate(
                    hit,
                    provider.metadata(),
                    prompt,
                    None,
                )
            })
            .collect()
    }

    fn recall_file_at_k(candidates: &[RetrievalCandidate], expected_path: &str, k: usize) -> f64 {
        if candidates
            .iter()
            .take(k)
            .any(|candidate| candidate.path.as_deref() == Some(expected_path))
        {
            1.0
        } else {
            0.0
        }
    }

    fn recall_entity_at_k(candidates: &[RetrievalCandidate], expected_id: &str, k: usize) -> f64 {
        if candidates
            .iter()
            .take(k)
            .any(|candidate| candidate.entity_id.as_deref() == Some(expected_id))
        {
            1.0
        } else {
            0.0
        }
    }

    fn vector_noise_candidate_for_gate(index: usize) -> RetrievalCandidate {
        let mut candidate = RetrievalCandidate::new(
            format!("vector://noise/{index}"),
            RetrievalCandidateSource::VectorSemantic,
            "noisy vector candidate for exact seed protection fixture",
        );
        candidate.embedding_source = Some(VectorEmbeddingSource::TextEvidence);
        candidate.path = Some(format!("docs/noise-{index}.adoc"));
        candidate.span = Some(SourceSpan::new(format!("docs/noise-{index}.adoc"), 1, 1));
        candidate.proof_status = RetrievalProofStatus::NotGraphProof;
        candidate.graph_proof = false;
        candidate.claimable = true;
        candidate.claimable_for_text = Some(true);
        candidate.claimable_for_graph = Some(false);
        candidate.score = Some(1.0 - (index as f64 * 0.001));
        candidate.chunk_id = Some(format!("vector-noise-chunk-{index}"));
        candidate.chunk_kind = Some("snippet".to_string());
        candidate.requires_graph_verification = false;
        candidate.verification_status = RetrievalVerificationStatus::NotGraphProof;
        candidate.metadata.insert(
            "chunk_text".to_string(),
            serde_json::json!("semantic package metadata generic package noise"),
        );
        candidate
    }

    fn candidate_source_counts(candidates: &[RetrievalCandidate]) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for candidate in candidates {
            let source = serde_json::to_value(candidate.candidate_source)
                .expect("source json")
                .as_str()
                .expect("source string")
                .to_string();
            *counts.entry(source).or_insert(0) += 1;
        }
        counts
    }

    fn proof_status_counts(candidates: &[RetrievalCandidate]) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for candidate in candidates {
            let status = serde_json::to_value(candidate.proof_status)
                .expect("proof json")
                .as_str()
                .expect("proof string")
                .to_string();
            *counts.entry(status).or_insert(0) += 1;
        }
        counts
    }

    #[test]
    fn text_evidence_respects_hard_excludes_and_artifact_suffixes() {
        let repo = temp_repo("text-evidence-negative-scope");
        write_test_file(
            &repo,
            "package/foo/foo.mk",
            "FOO_VERSION = 1\n$(eval $(generic-package))\n",
        );
        write_test_file(
            &repo,
            "target/generated.mk",
            "SHOULD_NOT_INDEX = target\n$(eval $(generic-package))\n",
        );
        write_test_file(
            &repo,
            "node_modules/pkg/Config.in",
            "config SHOULD_NOT_INDEX_NODE_MODULES\n",
        );
        write_test_file(
            &repo,
            "reports/final/generated.md",
            "SHOULD_NOT_INDEX_REPORT_FINAL\n",
        );
        write_test_file(
            &repo,
            "reports/audit/artifacts/run/generated.md",
            "SHOULD_NOT_INDEX_REPORT_ARTIFACT\n",
        );
        write_test_file(&repo, "src/cache.db", "SHOULD_NOT_INDEX_DB\n");
        write_test_file(&repo, "src/cache.sqlite", "SHOULD_NOT_INDEX_SQLITE\n");
        write_test_file(&repo, "logs/run.log", "SHOULD_NOT_INDEX_LOG\n");

        let db = repo.join("target").join("text-negative.sqlite");
        index_repo_to_db(&repo, &db).expect("index fixture");
        let store = SqliteGraphStore::open(&db).expect("store");
        assert_text_evidence_file(
            &store,
            "package/foo/foo.mk",
            TextEvidenceFileKind::MakefileFragment,
        );
        for path in [
            "target/generated.mk",
            "node_modules/pkg/Config.in",
            "reports/final/generated.md",
            "reports/audit/artifacts/run/generated.md",
            "src/cache.db",
            "src/cache.sqlite",
            "logs/run.log",
        ] {
            assert!(
                store.get_file(path).expect("file lookup").is_none(),
                "{path}"
            );
        }
        assert!(store
            .search_text("SHOULD_NOT_INDEX", 20)
            .expect("negative search")
            .is_empty());

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn incremental_update_replaces_text_evidence_fts_rows_without_parsing() {
        let repo = temp_repo("text-evidence-incremental");
        write_test_file(
            &repo,
            "package/foo/foo.mk",
            "FOO_VERSION = 1\n$(eval $(generic-package))\n",
        );
        let db = repo.join("target").join("text-incremental.sqlite");
        index_repo_to_db(&repo, &db).expect("initial index");

        write_test_file(
            &repo,
            "package/foo/foo.mk",
            "FOO_VERSION = 2\nFOO_LICENSE = MIT\n$(eval $(generic-package))\n",
        );
        let summary =
            update_changed_files_to_db(&repo, &[PathBuf::from("package/foo/foo.mk")], &db)
                .expect("incremental text update");
        assert_eq!(summary.files_indexed, 1);
        assert_eq!(summary.files_parsed, 0);

        let store = SqliteGraphStore::open(&db).expect("store");
        let license_hits = store
            .search_text("FOO_LICENSE", 20)
            .expect("license search");
        assert!(license_hits
            .iter()
            .any(|hit| hit.repo_relative_path == "package/foo/foo.mk"));
        assert!(store
            .list_entities_by_file("package/foo/foo.mk")
            .expect("text entities")
            .is_empty());

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn proof_build_plus_validation_runs_full_integrity_gate() {
        let repo = temp_repo("proof-build-validated");
        fs::write(
            repo.join("src").join("auth.ts"),
            "export function login() { return 'ok'; }\n",
        )
        .expect("write auth");

        let db = repo.join("target").join("validated.sqlite");
        let summary = index_repo_to_db_with_options(
            &repo,
            &db,
            IndexOptions {
                profile: true,
                build_mode: IndexBuildMode::ProofBuildPlusValidation,
                ..IndexOptions::default()
            },
        )
        .expect("validated proof build");

        assert_eq!(summary.build_mode, "proof-build-plus-validation");
        let profile = summary.profile.as_ref().expect("profile");
        assert!(
            profile
                .spans
                .iter()
                .any(|span| span.name == "integrity_check"),
            "validation build should run full integrity checks"
        );
        assert_db_integrity(&db);

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_repeat_index_skips_unchanged_file() {
        let repo = temp_repo("unchanged-skip");
        fs::write(
            repo.join("src").join("auth.ts"),
            "export function login() { return 'ok'; }\n",
        )
        .expect("write auth");

        let db = repo.join("target").join("manifest.sqlite");
        let initial = index_repo_to_db(&repo, &db).expect("initial index");
        let repeat = index_repo_to_db_with_options(
            &repo,
            &db,
            IndexOptions {
                profile: true,
                json: false,
                ..IndexOptions::default()
            },
        )
        .expect("repeat index");

        assert_eq!(initial.files_indexed, 1);
        assert_eq!(repeat.files_indexed, 0);
        assert_eq!(repeat.files_skipped, 1);
        assert_eq!(repeat.files_walked, 1);
        assert_eq!(repeat.files_metadata_unchanged, 1);
        assert_eq!(repeat.files_read, 0);
        assert_eq!(repeat.files_hashed, 0);
        assert_eq!(repeat.files_parsed, 0);
        assert_eq!(
            repeat
                .profile
                .as_ref()
                .expect("profile")
                .skipped_unchanged_files,
            1
        );
        let store = SqliteGraphStore::open(&db).expect("store");
        let file = store
            .get_file("src/auth.ts")
            .expect("file lookup")
            .expect("file");
        assert_eq!(
            file.metadata
                .get(FILE_LIFECYCLE_STATE_KEY)
                .and_then(Value::as_str),
            Some(FILE_LIFECYCLE_STATE_CURRENT)
        );
        assert_eq!(
            file.metadata
                .get(FILE_LIFECYCLE_POLICY_KEY)
                .and_then(Value::as_str),
            Some(FILE_LIFECYCLE_POLICY_CURRENT_ONLY)
        );
        store.quick_integrity_gate().expect("repeat quick check");

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_hash_unchanged_skips_parse_after_metadata_change() {
        let repo = temp_repo("hash-unchanged-skip");
        let source = "export function login() { return 'ok'; }\n";
        let file = repo.join("src").join("auth.ts");
        fs::write(&file, source).expect("write auth");

        let db = repo.join("target").join("manifest.sqlite");
        index_repo_to_db(&repo, &db).expect("initial index");
        std::thread::sleep(Duration::from_millis(10));
        fs::write(&file, source).expect("rewrite same auth");

        let repeat = index_repo_to_db_with_options(
            &repo,
            &db,
            IndexOptions {
                profile: true,
                json: false,
                ..IndexOptions::default()
            },
        )
        .expect("repeat index after metadata-only change");

        assert_eq!(repeat.files_indexed, 0);
        assert_eq!(repeat.files_skipped, 1);
        assert_eq!(repeat.files_metadata_unchanged, 0);
        assert_eq!(repeat.files_read, 1);
        assert_eq!(repeat.files_hashed, 1);
        assert_eq!(repeat.files_parsed, 0);
        assert_db_integrity(&db);

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_changed_file_is_reparsed_and_old_symbols_are_removed() {
        let repo = temp_repo("changed-reparse");
        let file = repo.join("src").join("auth.ts");
        fs::write(&file, "export function login() { return 'old'; }\n").expect("write old");

        let db = repo.join("target").join("manifest.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        fs::write(&file, "export function register() { return 'new'; }\n").expect("write new");

        let summary = update_changed_files_to_db(&repo, &[PathBuf::from("src/auth.ts")], &db)
            .expect("update");
        let store = SqliteGraphStore::open(&db).expect("store");
        let login = entities_by_kind_and_name(&store, EntityKind::Function, "login");
        let register = entities_by_kind_and_name(&store, EntityKind::Function, "register");

        assert_eq!(summary.files_indexed, 1);
        assert_eq!(summary.files_read, 1);
        assert_eq!(summary.files_hashed, 1);
        assert_eq!(summary.files_parsed, 1);
        assert!(!summary.global_hash_check_ran);
        assert!(!summary.integrity_check_ran);
        assert!(summary.dirty_path_evidence_count <= summary.edges);
        let profile = summary.profile.as_ref().expect("update profile");
        let cache_refresh = profile
            .spans
            .iter()
            .find(|span| span.name == "cache_refresh")
            .expect("cache refresh span");
        assert_eq!(cache_refresh.items, 0);
        assert!(login.is_empty());
        assert_eq!(register.len(), 1);
        store.quick_integrity_gate().expect("update quick check");

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn failed_update_transaction_rolls_back_and_leaves_db_valid() {
        let repo = temp_repo("failed-update-rollback");
        fs::write(
            repo.join("src").join("auth.ts"),
            "export function login() { return 'ok'; }\n",
        )
        .expect("write auth");

        let db = repo.join("target").join("rollback.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let before = store.count_entities().expect("before entity count");
        let result: Result<(), StoreError> = store.transaction(|tx| {
            tx.delete_facts_for_file("src/auth.ts")?;
            tx.quick_integrity_gate()?;
            Err(StoreError::Message("simulated update failure".to_string()))
        });

        assert!(
            matches!(result, Err(StoreError::Message(message)) if message.contains("simulated"))
        );
        assert_eq!(store.count_entities().expect("after entity count"), before);
        store.full_integrity_gate().expect("rollback integrity");

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn write_path_cold_publish_failpoints_preserve_old_claimable_db() {
        let repo = temp_repo("write-path-cold-publish-failpoints");
        write_test_file(
            &repo,
            "src/auth.ts",
            "export function oldLogin() { return 'old'; }\n",
        );
        let db = repo.join("target").join("cold.sqlite");
        index_repo_to_db(&repo, &db).expect("initial index");
        let before_hash = semantic_graph_fact_hash(&db);

        write_test_file(
            &repo,
            "src/auth.ts",
            "export function newLogin() { return 'new'; }\n",
        );

        for failpoint in [
            "cold_before_db_write",
            "cold_during_db_write",
            "cold_disk_full_simulated",
            "cold_after_temp_db_write_before_validation",
            "cold_after_validation_before_publish",
            "cold_during_publish",
            "cold_permission_denied_publish_dir",
        ] {
            let error = with_write_path_chaos_failpoint(failpoint, || {
                index_repo_to_db_with_options(&repo, &db, fresh_rebuild_options())
            })
            .expect_err("cold publish failpoint must fail");
            assert!(
                error.to_string().contains(failpoint),
                "failpoint={failpoint} error={error}"
            );

            let store = SqliteGraphStore::open(&db).expect("open preserved DB");
            store
                .full_integrity_gate()
                .expect("preserved DB remains valid");
            assert_eq!(
                semantic_graph_fact_hash(&db),
                before_hash,
                "old graph facts changed after {failpoint}"
            );
            assert_eq!(
                entities_by_kind_and_name(&store, EntityKind::Function, "oldLogin").len(),
                1,
                "old fact missing after {failpoint}"
            );
            assert!(
                entities_by_kind_and_name(&store, EntityKind::Function, "newLogin").is_empty(),
                "new fact leaked after {failpoint}"
            );
            drop(store);

            let preflight =
                inspect_repo_db_passport(&repo, &db, &IndexOptions::default()).expect("preflight");
            assert!(
                preflight.valid,
                "old DB became non-claimable after {failpoint}: {preflight:?}"
            );
            assert_no_atomic_temp_dbs(&db);
        }

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn write_path_cold_after_publish_failure_leaves_complete_claimable_db() {
        let repo = temp_repo("write-path-cold-after-publish");
        write_test_file(
            &repo,
            "src/auth.ts",
            "export function oldLogin() { return 'old'; }\n",
        );
        let db = repo.join("target").join("cold.sqlite");
        index_repo_to_db(&repo, &db).expect("initial index");
        write_test_file(
            &repo,
            "src/auth.ts",
            "export function newLogin() { return 'new'; }\n",
        );

        let error =
            with_write_path_chaos_failpoint("cold_after_publish_before_final_status", || {
                index_repo_to_db_with_options(&repo, &db, fresh_rebuild_options())
            })
            .expect_err("post-publish failpoint must fail");
        assert!(error
            .to_string()
            .contains("cold_after_publish_before_final_status"));

        let store = SqliteGraphStore::open(&db).expect("open published DB");
        store
            .full_integrity_gate()
            .expect("published DB remains valid");
        assert_eq!(
            entities_by_kind_and_name(&store, EntityKind::Function, "newLogin").len(),
            1
        );
        assert!(entities_by_kind_and_name(&store, EntityKind::Function, "oldLogin").is_empty());
        drop(store);

        let preflight =
            inspect_repo_db_passport(&repo, &db, &IndexOptions::default()).expect("preflight");
        assert!(
            preflight.valid,
            "post-publish failpoint must not leave a partial artifact: {preflight:?}"
        );
        assert_no_atomic_temp_dbs(&db);

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn write_path_incremental_failpoints_roll_back_changed_file() {
        for failpoint in [
            "incremental_before_stale_cleanup",
            "incremental_after_stale_cleanup_before_insert",
            "incremental_during_entity_insert",
            "incremental_during_edge_insert",
            "incremental_before_path_evidence_refresh",
            "incremental_after_insert_before_commit",
            "incremental_before_commit",
        ] {
            let repo = temp_repo(&format!("write-path-incremental-{failpoint}"));
            write_test_file(
                &repo,
                "src/auth.ts",
                "export function helper() { return 'old'; }\n\
                 export function oldLogin() { return helper(); }\n",
            );
            let db = repo.join("target").join("incremental.sqlite");
            index_repo_to_db(&repo, &db).expect("initial index");
            let before_hash = semantic_graph_fact_hash(&db);

            write_test_file(
                &repo,
                "src/auth.ts",
                "export function helper() { return 'new'; }\n\
                 export function newLogin() { return helper(); }\n",
            );
            let error = with_write_path_chaos_failpoint(failpoint, || {
                update_changed_files_to_db(&repo, &[PathBuf::from("src/auth.ts")], &db)
            })
            .expect_err("incremental failpoint must fail");
            assert!(
                error.to_string().contains(failpoint),
                "failpoint={failpoint} error={error}"
            );

            let store = SqliteGraphStore::open(&db).expect("open rolled-back DB");
            store
                .full_integrity_gate()
                .expect("rolled-back DB remains valid");
            assert_eq!(
                semantic_graph_fact_hash(&db),
                before_hash,
                "graph facts changed after {failpoint}"
            );
            assert_eq!(
                entities_by_kind_and_name(&store, EntityKind::Function, "oldLogin").len(),
                1,
                "old symbol missing after {failpoint}"
            );
            assert!(
                entities_by_kind_and_name(&store, EntityKind::Function, "newLogin").is_empty(),
                "new symbol leaked after {failpoint}"
            );
            drop(store);
            fs::remove_dir_all(repo).expect("cleanup");
        }
    }

    #[test]
    fn write_path_incremental_success_prunes_dirty_sidecar_handles() {
        let repo = temp_repo("write-path-incremental-dirty-sidecars");
        write_test_file(
            &repo,
            "src/auth.ts",
            "export function oldLogin() { return 'old'; }\n",
        );
        let db = repo.join("target").join("sidecars.sqlite");
        index_repo_to_db(&repo, &db).expect("initial index");
        {
            let store = SqliteGraphStore::open(&db).expect("store");
            let old_login = entity_by_file_kind_and_name(
                &store,
                "src/auth.ts",
                EntityKind::Function,
                "oldLogin",
            );
            store
                .insert_entity_feature(&EntityFeatureRow {
                    entity_id: old_login.id.clone(),
                    feature_kind: "ast_shape".to_string(),
                    payload_version: 1,
                    compact_payload: "{\"shape\":\"old\"}".to_string(),
                    extraction_version: "test-sidecar-v1".to_string(),
                    source_span_id: Some(old_login.id.clone()),
                    claimability: "diagnostic_only".to_string(),
                })
                .expect("insert entity feature");
            store
                .insert_routing_packet_handle(&RoutingPacketHandleRow {
                    handle_id: "handle-old-auth".to_string(),
                    db_passport_hash: "passport-old".to_string(),
                    task_intent_hash: "intent-old".to_string(),
                    packet_kind: "routing_packet".to_string(),
                    evidence_refs_json: format!("[\"{}\"]", old_login.id),
                    expires_or_invalidates_on: "file_fact_cleanup".to_string(),
                    payload_version: 1,
                    claimability: "diagnostic_only".to_string(),
                })
                .expect("insert routing handle");
            let counts = store.sparse_sidecar_counts().expect("sidecar counts");
            assert_eq!(counts.get("entity_features").copied(), Some(1));
            assert_eq!(counts.get("routing_packet_handles").copied(), Some(1));
        }

        write_test_file(
            &repo,
            "src/auth.ts",
            "export function newLogin() { return 'new'; }\n",
        );
        let summary = update_changed_files_to_db(&repo, &[PathBuf::from("src/auth.ts")], &db)
            .expect("successful update after sidecar seed");
        assert_eq!(summary.files_indexed, 1);

        let store = SqliteGraphStore::open(&db).expect("store");
        assert!(entities_by_kind_and_name(&store, EntityKind::Function, "oldLogin").is_empty());
        assert_eq!(
            entities_by_kind_and_name(&store, EntityKind::Function, "newLogin").len(),
            1
        );
        let counts = store.sparse_sidecar_counts().expect("sidecar counts");
        assert_eq!(counts.get("entity_features").copied(), Some(0));
        assert_eq!(counts.get("routing_packet_handles").copied(), Some(0));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn write_path_incremental_sidecar_invalidation_failure_rolls_back_dirty_evidence() {
        let repo = temp_repo("write-path-incremental-sidecar-rollback");
        write_test_file(
            &repo,
            "src/auth.ts",
            "export function oldLogin() { return 'old'; }\n",
        );
        let db = repo.join("target").join("sidecars-rollback.sqlite");
        index_repo_to_db(&repo, &db).expect("initial index");
        {
            let store = SqliteGraphStore::open(&db).expect("store");
            let old_login = entity_by_file_kind_and_name(
                &store,
                "src/auth.ts",
                EntityKind::Function,
                "oldLogin",
            );
            store
                .insert_entity_feature(&EntityFeatureRow {
                    entity_id: old_login.id.clone(),
                    feature_kind: "ast_shape".to_string(),
                    payload_version: 1,
                    compact_payload: "{\"shape\":\"old\"}".to_string(),
                    extraction_version: "test-sidecar-v1".to_string(),
                    source_span_id: Some(old_login.id.clone()),
                    claimability: "diagnostic_only".to_string(),
                })
                .expect("insert entity feature");
            store
                .insert_routing_packet_handle(&RoutingPacketHandleRow {
                    handle_id: "handle-old-auth".to_string(),
                    db_passport_hash: "passport-old".to_string(),
                    task_intent_hash: "intent-old".to_string(),
                    packet_kind: "routing_packet".to_string(),
                    evidence_refs_json: format!("[\"{}\"]", old_login.id),
                    expires_or_invalidates_on: "file_fact_cleanup".to_string(),
                    payload_version: 1,
                    claimability: "diagnostic_only".to_string(),
                })
                .expect("insert routing handle");
        }

        write_test_file(
            &repo,
            "src/auth.ts",
            "export function newLogin() { return 'new'; }\n",
        );
        let error = with_write_path_chaos_failpoint(
            "incremental_after_stale_cleanup_before_insert",
            || update_changed_files_to_db(&repo, &[PathBuf::from("src/auth.ts")], &db),
        )
        .expect_err("sidecar invalidation phase failpoint must fail");
        assert!(error
            .to_string()
            .contains("incremental_after_stale_cleanup_before_insert"));

        let store = SqliteGraphStore::open(&db).expect("store");
        store
            .full_integrity_gate()
            .expect("rolled-back sidecar invalidation DB remains valid");
        assert_eq!(
            entities_by_kind_and_name(&store, EntityKind::Function, "oldLogin").len(),
            1
        );
        assert!(entities_by_kind_and_name(&store, EntityKind::Function, "newLogin").is_empty());
        let counts = store.sparse_sidecar_counts().expect("sidecar counts");
        assert_eq!(counts.get("entity_features").copied(), Some(1));
        assert_eq!(counts.get("routing_packet_handles").copied(), Some(1));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_full_reindex_after_rename_deletes_old_path_and_indexes_new_path() {
        let repo = temp_repo("rename-cleanup");
        let old_path = repo.join("src").join("old_path.ts");
        let new_path = repo.join("src").join("new_path.ts");
        fs::write(&old_path, "export function moved() { return 'ok'; }\n").expect("write old");

        let db = repo.join("target").join("manifest.sqlite");
        index_repo_to_db(&repo, &db).expect("index old");
        fs::rename(&old_path, &new_path).expect("rename");

        let summary = index_repo_to_db(&repo, &db).expect("reindex renamed");
        let store = SqliteGraphStore::open(&db).expect("store");

        assert_eq!(summary.stale_files_deleted, 1);
        assert_eq!(summary.files_deleted, 1);
        assert_eq!(summary.files_renamed, 1);
        assert!(store
            .get_file("src/old_path.ts")
            .expect("old file")
            .is_none());
        assert!(store
            .get_file("src/new_path.ts")
            .expect("new file")
            .is_some());
        assert!(store
            .list_entities_by_file("src/old_path.ts")
            .expect("old entities")
            .is_empty());
        assert_eq!(
            entities_by_kind_and_name(&store, EntityKind::Function, "moved").len(),
            1
        );

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_incremental_rename_prunes_old_path_and_retargets_import_call() {
        let repo = temp_repo("incremental-rename-cleanup");
        let old_path = repo.join("src").join("oldName.ts");
        let new_path = repo.join("src").join("newName.ts");
        let use_path = repo.join("src").join("use.ts");
        fs::write(&old_path, "export function doWork() { return 'old'; }\n").expect("write old");
        fs::write(
            &use_path,
            "import { doWork } from './oldName';\n\
             export function run() { return doWork(); }\n",
        )
        .expect("write use old");

        let db = repo.join("target").join("manifest.sqlite");
        index_repo_to_db(&repo, &db).expect("index old");
        fs::rename(&old_path, &new_path).expect("rename");
        fs::write(
            &use_path,
            "import { doWork } from './newName';\n\
             export function run() { return doWork(); }\n",
        )
        .expect("write use new");

        let summary = update_changed_files_to_db(
            &repo,
            &[
                PathBuf::from("src/oldName.ts"),
                PathBuf::from("src/newName.ts"),
                PathBuf::from("src/use.ts"),
            ],
            &db,
        )
        .expect("incremental rename update");
        let store = SqliteGraphStore::open(&db).expect("store");
        let moved =
            entity_by_file_kind_and_name(&store, "src/newName.ts", EntityKind::Function, "doWork");
        let run = entity_by_file_kind_and_name(&store, "src/use.ts", EntityKind::Function, "run");
        let calls = store
            .find_edges_by_head_relation(&run.id, RelationKind::Calls)
            .expect("calls from run");

        assert!(summary.files_deleted >= 1);
        assert!(summary.files_renamed >= 1);
        assert!(store
            .get_file("src/oldName.ts")
            .expect("old file")
            .is_none());
        assert!(store
            .list_entities_by_file("src/oldName.ts")
            .expect("old entities")
            .is_empty());
        assert!(calls.iter().any(|edge| edge.tail_id == moved.id));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_incremental_delete_prunes_stale_target_and_retargets_live_import() {
        let repo = temp_repo("incremental-delete-cleanup");
        let deleted_path = repo.join("src").join("deleted.ts");
        let live_path = repo.join("src").join("live.ts");
        let use_path = repo.join("src").join("use.ts");
        fs::write(
            &deleted_path,
            "export function deleted() { return 'old'; }\n",
        )
        .expect("write deleted");
        fs::write(
            &use_path,
            "import { deleted } from './deleted';\n\
             export function run() { return deleted(); }\n",
        )
        .expect("write use deleted");

        let db = repo.join("target").join("manifest.sqlite");
        index_repo_to_db(&repo, &db).expect("index deleted");
        fs::remove_file(&deleted_path).expect("delete old target");
        fs::write(&live_path, "export function live() { return 'live'; }\n").expect("write live");
        fs::write(
            &use_path,
            "import { live } from './live';\n\
             export function run() { return live(); }\n",
        )
        .expect("write use live");

        let summary = update_changed_files_to_db(
            &repo,
            &[
                PathBuf::from("src/deleted.ts"),
                PathBuf::from("src/live.ts"),
                PathBuf::from("src/use.ts"),
            ],
            &db,
        )
        .expect("incremental delete update");
        let store = SqliteGraphStore::open(&db).expect("store");
        let live =
            entity_by_file_kind_and_name(&store, "src/live.ts", EntityKind::Function, "live");
        let run = entity_by_file_kind_and_name(&store, "src/use.ts", EntityKind::Function, "run");
        let calls = store
            .find_edges_by_head_relation(&run.id, RelationKind::Calls)
            .expect("calls from run");

        assert!(summary.files_deleted >= 1);
        assert!(store
            .get_file("src/deleted.ts")
            .expect("deleted file")
            .is_none());
        assert!(store
            .list_entities_by_file("src/deleted.ts")
            .expect("deleted entities")
            .is_empty());
        assert!(calls.iter().any(|edge| edge.tail_id == live.id));
        assert!(calls.iter().all(|edge| {
            store
                .get_entity(&edge.tail_id)
                .expect("tail lookup")
                .is_some_and(|entity| entity.repo_relative_path != "src/deleted.ts")
        }));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_import_alias_change_updates_calls_target() {
        let repo = temp_repo("alias-change-target");
        fs::write(
            repo.join("src").join("service.ts"),
            "export function oldTarget() { return 'old'; }\n\
             export function newTarget() { return 'new'; }\n",
        )
        .expect("write service");
        let consumer = repo.join("src").join("consumer.ts");
        fs::write(
            &consumer,
            "import { oldTarget as target } from './service';\n\
             export function run() { return target(); }\n",
        )
        .expect("write consumer");

        let db = repo.join("target").join("manifest.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        fs::write(
            &consumer,
            "import { newTarget as target } from './service';\n\
             export function run() { return target(); }\n",
        )
        .expect("retarget consumer");
        update_changed_files_to_db(&repo, &[PathBuf::from("src/consumer.ts")], &db)
            .expect("update alias");

        let store = SqliteGraphStore::open(&db).expect("store");
        let new_target = entity_by_file_kind_and_name(
            &store,
            "src/service.ts",
            EntityKind::Function,
            "newTarget",
        );
        let run =
            entity_by_file_kind_and_name(&store, "src/consumer.ts", EntityKind::Function, "run");
        let calls = store
            .find_edges_by_head_relation(&run.id, RelationKind::Calls)
            .expect("calls from run");

        assert!(
            calls.iter().any(|edge| edge.tail_id == new_target.id),
            "CALLS should move to newTarget after the import alias changes"
        );
    }

    #[test]
    fn rtds_dependency_closure_changed_export_considers_direct_importer_without_full_reparse() {
        let repo = temp_repo("rtds-closure-export-importer");
        write_test_file(
            &repo,
            "src/service.ts",
            "export function oldTarget() { return 'old'; }\n",
        );
        write_test_file(
            &repo,
            "src/consumer.ts",
            "import { oldTarget } from './service';\n\
             export function run() { return oldTarget(); }\n",
        );
        let db = repo.join("target").join("closure.sqlite");
        index_repo_to_db(&repo, &db).expect("index");

        write_test_file(
            &repo,
            "src/service.ts",
            "export function newTarget() { return 'new'; }\n",
        );
        let summary = update_changed_files_to_db(&repo, &[PathBuf::from("src/service.ts")], &db)
            .expect("delta update");

        assert!(summary
            .dependency_closure
            .closure_files_considered
            .contains(&"src/consumer.ts".to_string()));
        assert!(summary
            .dependency_closure
            .closure_relation_classes
            .iter()
            .any(|class| class == "direct_static_importer"
                || class == "deleted_or_changed_callable_reference"));
        assert!(!summary.dependency_closure.closure_budget_hit);
        assert!(summary.dependency_closure.full_repo_fallback_avoided);
        assert_eq!(
            summary.files_parsed, 1,
            "unchanged importer is metadata-checked, not reparsed"
        );
        assert_eq!(summary.files_walked, 2);

        let store = SqliteGraphStore::open(&db).expect("store");
        assert!(entities_by_kind_and_name(&store, EntityKind::Function, "oldTarget").is_empty());
        assert_eq!(
            entities_by_kind_and_name(&store, EntityKind::Function, "newTarget").len(),
            1
        );
        drop(store);
        assert!(!repo.join(".codegraph").exists());
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn rtds_dependency_closure_handles_utf8_bom_static_imports() {
        let repo = temp_repo("rtds-closure-bom-import");
        write_test_file(
            &repo,
            "src/service.ts",
            "export function oldTarget() { return 'old'; }\n",
        );
        write_test_file(
            &repo,
            "src/consumer.ts",
            "\u{feff}import { oldTarget } from './service';\n\
             export function run() { return oldTarget(); }\n",
        );
        let db = repo.join("target").join("closure.sqlite");
        index_repo_to_db(&repo, &db).expect("index");

        write_test_file(
            &repo,
            "src/service.ts",
            "export function newTarget() { return 'new'; }\n",
        );
        let summary = update_changed_files_to_db(&repo, &[PathBuf::from("src/service.ts")], &db)
            .expect("delta update");

        assert!(summary
            .dependency_closure
            .closure_files_considered
            .contains(&"src/consumer.ts".to_string()));
        assert!(summary
            .dependency_closure
            .closure_relation_classes
            .contains(&"direct_static_importer".to_string()));

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn rtds_dependency_closure_changed_import_alias_reports_exact_alias_class() {
        let repo = temp_repo("rtds-closure-alias-change");
        write_test_file(
            &repo,
            "src/service.ts",
            "export function oldTarget() { return 'old'; }\n\
             export function newTarget() { return 'new'; }\n",
        );
        write_test_file(
            &repo,
            "src/consumer.ts",
            "import { oldTarget as target } from './service';\n\
             export function run() { return target(); }\n",
        );
        let db = repo.join("target").join("closure.sqlite");
        index_repo_to_db(&repo, &db).expect("index");

        write_test_file(
            &repo,
            "src/consumer.ts",
            "import { newTarget as target } from './service';\n\
             export function run() { return target(); }\n",
        );
        let summary = update_changed_files_to_db(&repo, &[PathBuf::from("src/consumer.ts")], &db)
            .expect("delta update");

        assert!(summary
            .dependency_closure
            .closure_relation_classes
            .contains(&"direct_import_alias".to_string()));
        assert!(summary
            .dependency_closure
            .closure_files_considered
            .contains(&"src/consumer.ts".to_string()));
        assert!(!summary.dependency_closure.closure_budget_hit);

        let store = SqliteGraphStore::open(&db).expect("store");
        let new_target = entity_by_file_kind_and_name(
            &store,
            "src/service.ts",
            EntityKind::Function,
            "newTarget",
        );
        let run =
            entity_by_file_kind_and_name(&store, "src/consumer.ts", EntityKind::Function, "run");
        let calls = store
            .find_edges_by_head_relation(&run.id, RelationKind::Calls)
            .expect("calls from run");
        assert!(calls.iter().any(|edge| edge.tail_id == new_target.id));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn rtds_dependency_closure_deleted_callable_considers_direct_reference() {
        let repo = temp_repo("rtds-closure-deleted-callable");
        write_test_file(
            &repo,
            "src/service.ts",
            "export function removedTarget() { return 'old'; }\n",
        );
        write_test_file(
            &repo,
            "src/consumer.ts",
            "import { removedTarget } from './service';\n\
             export function run() { return removedTarget(); }\n",
        );
        let db = repo.join("target").join("closure.sqlite");
        index_repo_to_db(&repo, &db).expect("index");

        fs::remove_file(repo.join("src").join("service.ts")).expect("delete service");
        let summary = update_changed_files_to_db(&repo, &[PathBuf::from("src/service.ts")], &db)
            .expect("delta update");

        assert!(summary
            .dependency_closure
            .closure_files_considered
            .contains(&"src/consumer.ts".to_string()));
        assert!(summary
            .dependency_closure
            .closure_relation_classes
            .contains(&"deleted_or_changed_callable_reference".to_string()));
        assert_eq!(summary.files_deleted, 1);
        assert_eq!(summary.files_walked, 2);
        assert_eq!(summary.files_parsed, 0);

        let store = SqliteGraphStore::open(&db).expect("store");
        assert!(store.get_file("src/service.ts").expect("file").is_none());
        assert!(
            entities_by_kind_and_name(&store, EntityKind::Function, "removedTarget").is_empty()
        );
        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn rtds_dependency_closure_over_budget_degrades_without_full_repo_fallback() {
        let repo = temp_repo("rtds-closure-over-budget");
        write_test_file(
            &repo,
            "src/service.ts",
            "export function target() { return 'old'; }\n",
        );
        for index in 0..4 {
            write_test_file(
                &repo,
                &format!("src/consumer{index}.ts"),
                "import { target } from './service';\n\
                 export function run() { return target(); }\n",
            );
        }
        let db = repo.join("target").join("closure.sqlite");
        index_repo_to_db(&repo, &db).expect("index");

        write_test_file(
            &repo,
            "src/service.ts",
            "export function target() { return 'new'; }\n",
        );
        let budget = RtdsDependencyClosureBudget {
            max_dirty_files: 1,
            max_edges_inspected: 64,
            max_relation_classes: 6,
            max_wall_ms: 250,
            max_source_bytes: 4 * 1024 * 1024,
            max_db_rows_hydrated: 512,
            max_per_relation: 64,
        };
        let summary = with_rtds_closure_budget(budget, || {
            update_changed_files_to_db(&repo, &[PathBuf::from("src/service.ts")], &db)
        })
        .expect("delta update");

        assert!(summary.dependency_closure.closure_budget_hit);
        assert_eq!(
            summary.dependency_closure.status, "degraded",
            "over-budget closure must be explicit"
        );
        assert_eq!(
            summary.dependency_closure.closure_files_considered,
            vec!["src/service.ts".to_string()]
        );
        assert_eq!(
            summary.files_walked, 1,
            "must not silently full-repo fallback"
        );
        assert!(summary
            .dependency_closure
            .manual_full_index_recommendation
            .as_deref()
            .is_some_and(|recommendation| recommendation.contains("agent-use index --fresh")));

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn rtds_dependency_closure_unsupported_relation_is_unknown_not_proof() {
        let repo = temp_repo("rtds-closure-unsupported-relation");
        write_test_file(
            &repo,
            "src/policy.ts",
            "export function policy() { return true; }\n",
        );
        write_test_file(
            &repo,
            "src/route.ts",
            "export function route() { return 'ok'; }\n",
        );
        let db = repo.join("target").join("closure.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        {
            let store = SqliteGraphStore::open(&db).expect("store");
            let policy = entity_by_file_kind_and_name(
                &store,
                "src/policy.ts",
                EntityKind::Function,
                "policy",
            );
            let route =
                entity_by_file_kind_and_name(&store, "src/route.ts", EntityKind::Function, "route");
            let span = SourceSpan::new("src/route.ts", 1, 1);
            store
                .upsert_edge(&Edge {
                    id: stable_edge_id(&route.id, RelationKind::Authorizes, &policy.id, &span),
                    head_id: route.id,
                    relation: RelationKind::Authorizes,
                    tail_id: policy.id,
                    source_span: span,
                    repo_commit: None,
                    file_hash: None,
                    extractor: "test".to_string(),
                    confidence: 1.0,
                    exactness: Exactness::ParserVerified,
                    edge_class: EdgeClass::BaseExact,
                    context: EdgeContext::Production,
                    derived: false,
                    provenance_edges: Vec::new(),
                    metadata: Metadata::new(),
                })
                .expect("insert unsupported proof relation");
        }

        write_test_file(
            &repo,
            "src/policy.ts",
            "export function policy() { return false; }\n",
        );
        let summary = update_changed_files_to_db(&repo, &[PathBuf::from("src/policy.ts")], &db)
            .expect("delta update");

        assert!(summary
            .dependency_closure
            .closure_unknowns
            .contains(&"unsupported_relation_class:AUTHORIZES".to_string()));
        assert!(summary
            .dependency_closure
            .skipped_relation_classes
            .contains(&"AUTHORIZES".to_string()));
        assert_eq!(summary.dependency_closure.status, "degraded");

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn rtds_dependency_closure_keeps_same_name_symbols_file_scoped() {
        let repo = temp_repo("rtds-closure-same-name");
        write_test_file(
            &repo,
            "src/a.ts",
            "export function chooseUser() { return 'a'; }\n",
        );
        write_test_file(
            &repo,
            "src/b.ts",
            "export function chooseUser() { return 'b'; }\n",
        );
        write_test_file(
            &repo,
            "src/consumer.ts",
            "import { chooseUser } from './a';\n\
             export function run() { return chooseUser(); }\n",
        );
        let db = repo.join("target").join("closure.sqlite");
        index_repo_to_db(&repo, &db).expect("index");

        write_test_file(
            &repo,
            "src/a.ts",
            "export function chooseUser() { return 'a2'; }\n",
        );
        let summary = update_changed_files_to_db(&repo, &[PathBuf::from("src/a.ts")], &db)
            .expect("delta update");

        assert!(summary
            .dependency_closure
            .closure_files_considered
            .contains(&"src/consumer.ts".to_string()));
        assert!(!summary
            .dependency_closure
            .closure_files_considered
            .contains(&"src/b.ts".to_string()));

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn rtds_dependency_closure_failpoint_preserves_old_good_db() {
        let repo = temp_repo("rtds-closure-failpoint-preserves-db");
        write_test_file(
            &repo,
            "src/service.ts",
            "export function oldTarget() { return 'old'; }\n",
        );
        let db = repo.join("target").join("closure.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let before_hash = semantic_graph_fact_hash(&db);

        write_test_file(
            &repo,
            "src/service.ts",
            "export function newTarget() { return 'new'; }\n",
        );
        let error = with_rtds_closure_failpoint("before_update", || {
            update_changed_files_to_db(&repo, &[PathBuf::from("src/service.ts")], &db)
        })
        .expect_err("closure failpoint must stop before update");
        assert!(
            error
                .to_string()
                .contains("rtds_dependency_closure_failpoint"),
            "error={error}"
        );

        let store = SqliteGraphStore::open(&db).expect("store");
        store.full_integrity_gate().expect("old DB valid");
        assert_eq!(semantic_graph_fact_hash(&db), before_hash);
        assert_eq!(
            entities_by_kind_and_name(&store, EntityKind::Function, "oldTarget").len(),
            1
        );
        assert!(entities_by_kind_and_name(&store, EntityKind::Function, "newTarget").is_empty());

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_admin_user_role_helpers_are_not_conflated() {
        let repo = temp_repo("admin-user-roles");
        fs::write(
            repo.join("src").join("auth.ts"),
            concat!(
                "export function requireAdmin(user: { role: string }) {\n",
                "  return checkRole(user, \"admin\");\n",
                "}\n",
                "\n",
                "export function requireUser(user: { role: string }) {\n",
                "  return checkRole(user, \"user\");\n",
                "}\n",
                "\n",
                "export function checkRole(user: { role: string }, role: string) {\n",
                "  return user.role === role;\n",
                "}\n",
            ),
        )
        .expect("write auth");
        fs::write(
            repo.join("src").join("routes.ts"),
            concat!(
                "import { requireAdmin, requireUser } from \"./auth\";\n",
                "\n",
                "export function adminRoute(req: { user: { role: string } }) {\n",
                "  if (!requireAdmin(req.user)) throw new Error(\"forbidden\");\n",
                "  return \"admin\";\n",
                "}\n",
                "\n",
                "export function userRoute(req: { user: { role: string } }) {\n",
                "  if (!requireUser(req.user)) throw new Error(\"forbidden\");\n",
                "  return \"user\";\n",
                "}\n",
            ),
        )
        .expect("write routes");

        let db = repo.join("target").join("security.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let admin = entity_by_file_kind_and_name(&store, "src/auth.ts", EntityKind::Role, "admin");
        let user = entity_by_file_kind_and_name(&store, "src/auth.ts", EntityKind::Role, "user");
        let admin_middleware = entity_by_file_kind_and_name(
            &store,
            "src/auth.ts",
            EntityKind::Middleware,
            "requireAdmin",
        );
        let user_middleware = entity_by_file_kind_and_name(
            &store,
            "src/auth.ts",
            EntityKind::Middleware,
            "requireUser",
        );
        let admin_route = entity_by_file_kind_and_name(
            &store,
            "src/routes.ts",
            EntityKind::Function,
            "adminRoute",
        );
        let user_route = entity_by_file_kind_and_name(
            &store,
            "src/routes.ts",
            EntityKind::Function,
            "userRoute",
        );
        let admin_edges = store
            .find_edges_by_head_relation(&admin_route.id, RelationKind::ChecksRole)
            .expect("admin role edges");
        let user_edges = store
            .find_edges_by_head_relation(&user_route.id, RelationKind::ChecksRole)
            .expect("user role edges");
        let admin_helper_edges = store
            .find_edges_by_head_relation(&admin_middleware.id, RelationKind::ChecksRole)
            .expect("admin helper role edges");
        let user_helper_edges = store
            .find_edges_by_head_relation(&user_middleware.id, RelationKind::ChecksRole)
            .expect("user helper role edges");

        assert!(admin_edges.iter().any(|edge| {
            edge.tail_id == admin.id
                && edge.exactness == Exactness::ParserVerified
                && edge.confidence == 1.0
                && edge.source_span.repo_relative_path == "src/routes.ts"
                && edge.source_span.start_line == 4
                && edge.source_span.start_column == Some(8)
                && edge.source_span.end_column == Some(30)
        }));
        assert!(user_edges.iter().any(|edge| {
            edge.tail_id == user.id
                && edge.exactness == Exactness::ParserVerified
                && edge.confidence == 1.0
                && edge.source_span.repo_relative_path == "src/routes.ts"
                && edge.source_span.start_line == 9
                && edge.source_span.start_column == Some(8)
                && edge.source_span.end_column == Some(29)
        }));
        assert!(
            admin_edges.iter().all(|edge| edge.tail_id != user.id),
            "adminRoute must not be linked to the user role"
        );
        assert!(
            user_edges.iter().all(|edge| edge.tail_id != admin.id),
            "userRoute must not be linked to the admin role"
        );
        assert!(admin_helper_edges.iter().any(|edge| {
            edge.tail_id == admin.id
                && edge.exactness == Exactness::ParserVerified
                && edge.context == EdgeContext::Production
                && edge.source_span.repo_relative_path == "src/auth.ts"
                && edge.source_span.start_line == 2
                && edge.source_span.start_column == Some(10)
        }));
        assert!(user_helper_edges.iter().any(|edge| {
            edge.tail_id == user.id
                && edge.exactness == Exactness::ParserVerified
                && edge.context == EdgeContext::Production
                && edge.source_span.repo_relative_path == "src/auth.ts"
                && edge.source_span.start_line == 6
                && edge.source_span.start_column == Some(10)
        }));
        assert_eq!(
            admin.qualified_name, "role:admin",
            "role selectors must not include nearby helper names or comments"
        );
        assert_eq!(user.qualified_name, "role:user");

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_role_check_ignores_comments_and_string_literals() {
        let repo = temp_repo("role-comment-string-guard");
        fs::write(
            repo.join("src").join("auth.ts"),
            concat!(
                "export function helper(user: any) {\n",
                "  // checkRole(user, \"admin\")\n",
                "  const text = \"checkRole(user, 'admin')\";\n",
                "  return text;\n",
                "}\n",
            ),
        )
        .expect("write auth");

        let db = repo.join("target").join("security.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let role_edges = store
            .list_edges(UNBOUNDED_STORE_READ_LIMIT)
            .expect("edges")
            .into_iter()
            .filter(|edge| edge.relation == RelationKind::ChecksRole)
            .collect::<Vec<_>>();

        assert!(
            role_edges
                .iter()
                .all(|edge| edge.exactness != Exactness::ParserVerified),
            "comment/string role mentions must not become exact CHECKS_ROLE facts"
        );

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_route_factory_exposes_endpoint_and_authorizes_guard() {
        let repo = temp_repo("route-factory-exposure");
        fs::write(
            repo.join("src").join("auth.ts"),
            concat!(
                "export function requireAdmin(user: any) {\n",
                "  return checkRole(user, \"admin\");\n",
                "}\n",
                "export function checkRole(user: any, role: string) {\n",
                "  return user.roles.includes(role);\n",
                "}\n",
            ),
        )
        .expect("write auth");
        fs::write(
            repo.join("src").join("routes.ts"),
            concat!(
                "import { requireAdmin } from './auth';\n",
                "export const adminRoute = route(\"GET\", \"/admin\", requireAdmin, adminPanel);\n",
                "function route(method: string, path: string, guard: Function, handler: Function) {\n",
                "  return { method, path, guard, handler };\n",
                "}\n",
                "function adminPanel() { return \"admin\"; }\n",
            ),
        )
        .expect("write routes");

        let db = repo.join("target").join("security.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let route =
            entity_by_file_kind_and_name(&store, "src/routes.ts", EntityKind::Route, "adminRoute");
        let endpoint = entity_by_file_kind_and_name(
            &store,
            "src/routes.ts",
            EntityKind::Endpoint,
            "GET /admin",
        );
        let middleware = entity_by_file_kind_and_name(
            &store,
            "src/auth.ts",
            EntityKind::Middleware,
            "requireAdmin",
        );
        let exposes = store
            .find_edges_by_head_relation(&route.id, RelationKind::Exposes)
            .expect("exposes");
        let authorizes = store
            .find_edges_by_head_relation(&route.id, RelationKind::Authorizes)
            .expect("authorizes");

        assert!(exposes.iter().any(|edge| {
            edge.tail_id == endpoint.id
                && edge.exactness == Exactness::ParserVerified
                && edge.context == EdgeContext::Production
                && edge.source_span.repo_relative_path == "src/routes.ts"
                && edge.source_span.start_line == 2
        }));
        assert!(authorizes.iter().any(|edge| {
            edge.tail_id == middleware.id
                && edge.exactness == Exactness::ParserVerified
                && edge.context == EdgeContext::Production
        }));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_unused_sanitizer_import_does_not_create_sanitized_flow() {
        let repo = temp_repo("unused-sanitizer");
        fs::write(
            repo.join("src").join("sanitize.ts"),
            concat!(
                "export function sanitizeEmail(input: string) {\n",
                "  return input.trim().toLowerCase();\n",
                "}\n",
            ),
        )
        .expect("write sanitizer");
        fs::write(
            repo.join("src").join("register.ts"),
            concat!(
                "import { sanitizeEmail } from \"./sanitize\";\n",
                "\n",
                "export function register(req: { body: { email: string } }) {\n",
                "  const email = req.body.email;\n",
                "  return saveUser(email);\n",
                "}\n",
                "\n",
                "function saveUser(email: string) {\n",
                "  return email;\n",
                "}\n",
            ),
        )
        .expect("write register");

        let db = repo.join("target").join("security.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let sanitizer = entity_by_file_kind_and_name(
            &store,
            "src/sanitize.ts",
            EntityKind::Sanitizer,
            "sanitizeEmail",
        );
        let property = entity_by_file_kind_and_name(
            &store,
            "src/register.ts",
            EntityKind::Property,
            "req.body.email",
        );
        let sink = entity_by_file_kind_and_name(
            &store,
            "src/register.ts",
            EntityKind::Parameter,
            "saveUser.email",
        );
        let flows = store
            .find_edges_by_head_relation(&property.id, RelationKind::FlowsTo)
            .expect("flows from email property");
        let sanitizer_edges = store
            .find_edges_by_head_relation(&sanitizer.id, RelationKind::Sanitizes)
            .expect("sanitizer edges");

        assert!(flows.iter().any(|edge| {
            edge.tail_id == sink.id
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.repo_relative_path == "src/register.ts"
                && edge.source_span.start_line == 5
                && edge.source_span.start_column == Some(19)
                && edge.source_span.end_column == Some(24)
        }));
        assert!(
            sanitizer_edges.is_empty(),
            "unused sanitizer import must not create a SANITIZES edge"
        );

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_sanitizer_call_on_value_flow_is_explicit() {
        let repo = temp_repo("sanitizer-on-flow");
        fs::write(
            repo.join("src").join("sanitize.ts"),
            concat!(
                "export function sanitizeEmail(input: string) {\n",
                "  return input.trim().toLowerCase();\n",
                "}\n",
            ),
        )
        .expect("write sanitizer");
        fs::write(
            repo.join("src").join("register.ts"),
            concat!(
                "import { sanitizeEmail } from \"./sanitize\";\n",
                "\n",
                "export function register(req: { body: { email: string } }) {\n",
                "  const email = sanitizeEmail(req.body.email);\n",
                "  return saveUser(email);\n",
                "}\n",
                "\n",
                "function saveUser(email: string) {\n",
                "  return email;\n",
                "}\n",
            ),
        )
        .expect("write register");

        let db = repo.join("target").join("security.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let sanitizer = entity_by_file_kind_and_name(
            &store,
            "src/sanitize.ts",
            EntityKind::Sanitizer,
            "sanitizeEmail",
        );
        let property = entity_by_file_kind_and_name(
            &store,
            "src/register.ts",
            EntityKind::Property,
            "req.body.email",
        );
        let sanitizer_edges = store
            .find_edges_by_head_relation(&sanitizer.id, RelationKind::Sanitizes)
            .expect("sanitizer edges");

        assert!(sanitizer_edges.iter().any(|edge| {
            edge.tail_id == property.id
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.repo_relative_path == "src/register.ts"
                && edge.source_span.start_line == 4
                && edge
                    .metadata
                    .get("resolver")
                    .and_then(|value| value.as_str())
                    == Some("direct_sanitizer_call_argument")
        }));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_raw_local_flow_to_write_sink_survives_simple_alias() {
        let repo = temp_repo("raw-local-flow");
        fs::write(
            repo.join("src").join("register.ts"),
            concat!(
                "export function saveComment(req: any) {\n",
                "  const raw = req.body.comment;\n",
                "  const normalized = raw.trim();\n",
                "  return writeComment(normalized);\n",
                "}\n",
                "export function writeComment(comment: string) {\n",
                "  return comment;\n",
                "}\n",
            ),
        )
        .expect("write register");

        let db = repo.join("target").join("security.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let raw = entity_by_file_kind_and_name(
            &store,
            "src/register.ts",
            EntityKind::LocalVariable,
            "raw",
        );
        let write_comment = entity_by_file_kind_and_name(
            &store,
            "src/register.ts",
            EntityKind::Function,
            "writeComment",
        );
        let flows = store
            .find_edges_by_head_relation(&raw.id, RelationKind::FlowsTo)
            .expect("flows from raw");

        assert!(flows.iter().any(|edge| {
            edge.tail_id == write_comment.id
                && edge.exactness == Exactness::ParserVerified
                && edge.context == EdgeContext::Production
                && edge.source_span.repo_relative_path == "src/register.ts"
                && edge.source_span.start_line == 4
                && edge.source_span.start_column == Some(10)
        }));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_sanitizer_call_on_local_variable_requires_proven_input_flow() {
        let repo = temp_repo("sanitizer-local-flow");
        fs::write(
            repo.join("src").join("sanitize.ts"),
            concat!(
                "export function sanitizeHtml(input: string) {\n",
                "  return input.replace(/</g, \"&lt;\");\n",
                "}\n",
            ),
        )
        .expect("write sanitizer");
        fs::write(
            repo.join("src").join("register.ts"),
            concat!(
                "import { sanitizeHtml } from './sanitize';\n",
                "export function saveComment(req: any) {\n",
                "  const raw = req.body.comment;\n",
                "  return writeComment(sanitizeHtml(raw));\n",
                "}\n",
                "function writeComment(comment: string) {\n",
                "  return comment;\n",
                "}\n",
            ),
        )
        .expect("write register");

        let db = repo.join("target").join("security.sqlite");
        index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let sanitizer = entity_by_file_kind_and_name(
            &store,
            "src/sanitize.ts",
            EntityKind::Sanitizer,
            "sanitizeHtml",
        );
        let raw = entity_by_file_kind_and_name(
            &store,
            "src/register.ts",
            EntityKind::LocalVariable,
            "raw",
        );
        let sanitizer_edges = store
            .find_edges_by_head_relation(&sanitizer.id, RelationKind::Sanitizes)
            .expect("sanitizer edges");

        assert!(sanitizer_edges.iter().any(|edge| {
            edge.tail_id == raw.id
                && edge.exactness == Exactness::ParserVerified
                && edge.context == EdgeContext::Production
                && edge.source_span.repo_relative_path == "src/register.ts"
                && edge.source_span.start_line == 4
        }));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn audit_unsupported_authorize_pattern_remains_heuristic() {
        let repo = temp_repo("unsupported-authorize");
        fs::write(
            repo.join("src").join("route.ts"),
            "export function route(req: any) {\n\
             return authorize(req.user);\n\
             }\n",
        )
        .expect("write route");

        let db = repo.join("target").join("security.sqlite");
        index_repo_to_db_with_options(
            &repo,
            &db,
            IndexOptions {
                storage_mode: StorageMode::Audit,
                ..IndexOptions::default()
            },
        )
        .expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let proof_edges = store.list_edges(UNBOUNDED_STORE_READ_LIMIT).expect("edges");
        assert!(
            proof_edges
                .iter()
                .all(|edge| edge.relation != RelationKind::Authorizes),
            "unsupported authorize calls must not be proof edges"
        );
        let edges = store
            .list_heuristic_edges(UNBOUNDED_STORE_READ_LIMIT)
            .expect("heuristic sidecar edges");
        let auth_edges = edges
            .iter()
            .filter(|edge| edge.relation == RelationKind::Authorizes)
            .collect::<Vec<_>>();

        assert!(
            !auth_edges.is_empty(),
            "parser should still surface unsupported authorize calls as heuristic evidence"
        );
        assert!(auth_edges
            .iter()
            .all(|edge| edge.exactness == Exactness::StaticHeuristic));

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn compact_default_storage_preserves_mvp_relations_and_spans() {
        let repo = temp_repo("full-mvp");
        fs::write(
            repo.join("src").join("audit.ts"),
            "export function auditLogin(user: any) { return user.id; }\n",
        )
        .expect("write audit");
        fs::write(
            repo.join("src").join("auth.ts"),
            "import { auditLogin } from './audit';\n\
             export function sanitize(input: string) { return input.trim(); }\n\
             export function saveUser(email: string) { return email; }\n\
             export function login(req: any) {\n\
             const email = sanitize(req.body.email);\n\
             saveUser(email);\n\
             auditLogin(req.user);\n\
             return email;\n\
             }\n",
        )
        .expect("write auth");

        let db = repo.join("target").join("full-mvp.sqlite");
        let summary = index_repo_to_db(&repo, &db).expect("index");
        let store = SqliteGraphStore::open(&db).expect("store");
        let relation_counts = store.relation_counts().expect("relation counts");
        let source_spans = store.count_source_spans().expect("source spans");

        assert_eq!(summary.storage_policy, DEFAULT_STORAGE_POLICY);
        assert_eq!(
            u64::try_from(summary.entities).ok(),
            Some(store.count_entities().expect("entities"))
        );
        assert_eq!(
            u64::try_from(summary.edges).ok(),
            Some(store.count_edges().expect("edges"))
        );
        assert!(source_spans >= store.count_entities().expect("entities"));
        for relation in [
            RelationKind::Contains,
            RelationKind::DefinedIn,
            RelationKind::Imports,
            RelationKind::Exports,
            RelationKind::Calls,
            RelationKind::Callee,
            RelationKind::Argument0,
            RelationKind::ReturnsTo,
            RelationKind::FlowsTo,
        ] {
            let key = relation.to_string();
            assert!(
                relation_counts.get(&key).copied().unwrap_or_default() > 0,
                "missing compact persisted relation {key}; counts={relation_counts:?}"
            );
        }

        let edges = store.list_edges(UNBOUNDED_STORE_READ_LIMIT).expect("edges");
        assert!(edges
            .iter()
            .all(|edge| !edge.source_span.repo_relative_path.is_empty()));
        assert!(edges.iter().all(|edge| !edge.extractor.is_empty()));
        for index in [
            "idx_entities_path",
            "idx_entities_name",
            "idx_entities_qname",
            "idx_edges_head_relation",
            "idx_edges_tail_relation",
            "idx_edges_span_path",
            "idx_source_spans_path",
        ] {
            assert!(
                store.index_exists(index).expect("index exists check"),
                "missing default lookup index {index}"
            );
        }

        drop(store);
        fs::remove_dir_all(repo).expect("cleanup");
    }
}
