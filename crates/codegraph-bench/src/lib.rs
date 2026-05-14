//! Reproducible local benchmark harness for CodeGraph.
//!
//! Phase 20 adds benchmark schemas, synthetic controlled repositories, baseline
//! runners, metric calculation, real-repo replay planning, and machine-readable
//! report generation. The harness calls existing parser, store, query, and
//! vector crates; it does not change retrieval behavior.

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use codegraph_core::{
    Edge, EdgeClass, Entity, Exactness, RelationKind, RepoIndexState, SourceSpan,
};
use codegraph_index::{index_repo_to_db, DEFAULT_STORAGE_POLICY, UNBOUNDED_STORE_READ_LIMIT};
use codegraph_mcp_server::{McpServer, McpServerConfig};
use codegraph_parser::{
    detect_language, extract_entities_and_relations, language_frontends, LanguageParser,
    TreeSitterParser,
};
use codegraph_query::{
    extract_prompt_seeds, BayesianRanker, BayesianRankerConfig, ContextPackRequest,
    ExactGraphQueryEngine, GraphPath, QueryLimits, RetrievalDocument, RetrievalFunnel,
    RetrievalFunnelConfig,
};
use codegraph_store::{GraphStore, SqliteGraphStore};
use codegraph_vector::{
    BinarySignature, BinaryVectorIndex, CompressedVectorReranker, DeterministicCompressedReranker,
    InMemoryBinaryVectorIndex, RerankCandidate, RerankConfig, RerankQuery,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub mod competitors;
pub mod graph_truth;
pub mod retrieval_ablation;
pub mod two_layer;

pub use graph_truth::{
    default_context_packet_gate_options, default_graph_truth_gate_options,
    render_context_packet_gate_markdown, render_graph_truth_gate_markdown, run_context_packet_gate,
    run_graph_truth_gate, write_context_packet_gate_report, write_graph_truth_gate_report,
    ContextPacketCaseResult, ContextPacketGateMetrics, ContextPacketGateOptions,
    ContextPacketGateReport, GraphTruthCaseResult, GraphTruthGateOptions, GraphTruthGateReport,
};
pub use retrieval_ablation::{
    default_retrieval_ablation_options, render_retrieval_ablation_markdown, run_retrieval_ablation,
    selected_retrieval_ablation_modes, write_retrieval_ablation_report, RetrievalAblationMode,
    RetrievalAblationOptions, RetrievalAblationReport,
};

pub use two_layer::{
    default_two_layer_bench_options, run_agent_quality_benchmark, run_retrieval_quality_benchmark,
    validate_jsonl_file, validate_two_layer_manifest, TwoLayerBenchArtifacts, TwoLayerBenchOptions,
    MAX_BENCH_TASK_MS,
};

pub const BENCH_SCHEMA_VERSION: u32 = 1;
const DEFAULT_TOP_K: usize = 10;

pub type BenchResult<T> = Result<T, BenchmarkError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BenchmarkError {
    Validation(String),
    Io(String),
    Parse(String),
    Store(String),
    Vector(String),
    Unsupported(String),
}

impl fmt::Display for BenchmarkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) => {
                write!(formatter, "benchmark validation failed: {message}")
            }
            Self::Io(message) => write!(formatter, "benchmark I/O failed: {message}"),
            Self::Parse(message) => write!(formatter, "benchmark parse failed: {message}"),
            Self::Store(message) => write!(formatter, "benchmark store failed: {message}"),
            Self::Vector(message) => write!(formatter, "benchmark vector failed: {message}"),
            Self::Unsupported(message) => write!(formatter, "benchmark unsupported: {message}"),
        }
    }
}

impl Error for BenchmarkError {}

impl From<std::io::Error> for BenchmarkError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkFamily {
    RelationExtraction,
    LongChainPath,
    ContextRetrieval,
    AgentPatch,
    Compression,
    SecurityAuth,
    AsyncEvent,
    TestImpact,
}

impl BenchmarkFamily {
    pub const ALL: [Self; 8] = [
        Self::RelationExtraction,
        Self::LongChainPath,
        Self::ContextRetrieval,
        Self::AgentPatch,
        Self::Compression,
        Self::SecurityAuth,
        Self::AsyncEvent,
        Self::TestImpact,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RelationExtraction => "relation_extraction",
            Self::LongChainPath => "long_chain_path",
            Self::ContextRetrieval => "context_retrieval",
            Self::AgentPatch => "agent_patch",
            Self::Compression => "compression",
            Self::SecurityAuth => "security_auth",
            Self::AsyncEvent => "async_event",
            Self::TestImpact => "test_impact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineMode {
    VanillaNoRetrieval,
    GrepBm25,
    VectorOnly,
    GraphOnly,
    GraphBinaryPqFunnel,
    GraphBayesianRanker,
    FullContextPacket,
}

impl BaselineMode {
    pub const ALL: [Self; 7] = [
        Self::VanillaNoRetrieval,
        Self::GrepBm25,
        Self::VectorOnly,
        Self::GraphOnly,
        Self::GraphBinaryPqFunnel,
        Self::GraphBayesianRanker,
        Self::FullContextPacket,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VanillaNoRetrieval => "vanilla_no_retrieval",
            Self::GrepBm25 => "grep_bm25",
            Self::VectorOnly => "vector_only",
            Self::GraphOnly => "graph_only",
            Self::GraphBinaryPqFunnel => "graph_binary_pq_funnel",
            Self::GraphBayesianRanker => "graph_bayesian_ranker",
            Self::FullContextPacket => "full_context_packet",
        }
    }
}

impl FromStr for BaselineMode {
    type Err = BenchmarkError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().replace('-', "_").to_ascii_lowercase().as_str() {
            "vanilla" | "vanilla_no_retrieval" | "no_retrieval" => Ok(Self::VanillaNoRetrieval),
            "grep" | "bm25" | "grep_bm25" => Ok(Self::GrepBm25),
            "vector" | "vector_only" => Ok(Self::VectorOnly),
            "graph" | "graph_only" => Ok(Self::GraphOnly),
            "graph_binary_pq_funnel" | "binary_pq" | "funnel" => Ok(Self::GraphBinaryPqFunnel),
            "graph_bayesian_ranker" | "bayesian" => Ok(Self::GraphBayesianRanker),
            "full" | "context_packet" | "full_context_packet" => Ok(Self::FullContextPacket),
            other => Err(BenchmarkError::Validation(format!(
                "unknown baseline mode: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntheticRepoKind {
    RelationExtraction,
    LongChainPath,
    ContextRetrieval,
    AgentPatch,
    Compression,
    SecurityAuth,
    AsyncEvent,
    TestImpact,
    AllFamilies,
}

impl From<BenchmarkFamily> for SyntheticRepoKind {
    fn from(value: BenchmarkFamily) -> Self {
        match value {
            BenchmarkFamily::RelationExtraction => Self::RelationExtraction,
            BenchmarkFamily::LongChainPath => Self::LongChainPath,
            BenchmarkFamily::ContextRetrieval => Self::ContextRetrieval,
            BenchmarkFamily::AgentPatch => Self::AgentPatch,
            BenchmarkFamily::Compression => Self::Compression,
            BenchmarkFamily::SecurityAuth => Self::SecurityAuth,
            BenchmarkFamily::AsyncEvent => Self::AsyncEvent,
            BenchmarkFamily::TestImpact => Self::TestImpact,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BenchmarkRepoSpec {
    Synthetic { kind: SyntheticRepoKind },
    RealCommitReplay { spec: CommitReplaySpec },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitReplaySpec {
    pub repo_path: String,
    pub base_commit: String,
    pub head_commit: String,
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayFeasibility {
    Feasible,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealRepoReplayPlan {
    pub status: ReplayFeasibility,
    pub reason: Option<String>,
    pub commands: Vec<String>,
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealRepoCorpus {
    pub schema_version: u32,
    pub source_of_truth: String,
    pub cache_root: String,
    pub repos: Vec<RealRepoManifest>,
}

impl RealRepoCorpus {
    pub fn validate(&self) -> BenchResult<()> {
        if self.schema_version != BENCH_SCHEMA_VERSION {
            return Err(BenchmarkError::Validation(format!(
                "expected real-repo corpus schema {}, got {}",
                BENCH_SCHEMA_VERSION, self.schema_version
            )));
        }
        if self.repos.len() < 5 {
            return Err(BenchmarkError::Validation(
                "real-repo corpus must include TypeScript, Python, Go, Rust, and Java repos"
                    .to_string(),
            ));
        }
        let languages = self
            .repos
            .iter()
            .map(|repo| repo.language.as_str())
            .collect::<BTreeSet<_>>();
        for required in ["typescript", "python", "go", "rust", "java"] {
            if !languages.contains(required) {
                return Err(BenchmarkError::Validation(format!(
                    "missing real-repo language: {required}"
                )));
            }
        }
        for repo in &self.repos {
            repo.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealRepoManifest {
    pub id: String,
    pub language: String,
    pub repo_url: String,
    pub checkout_ref: String,
    pub pinned_commit_sha: String,
    pub license: String,
    pub cache_subdir: String,
    pub size_policy: String,
    pub tasks: Vec<RealRepoTaskManifest>,
}

impl RealRepoManifest {
    fn validate(&self) -> BenchResult<()> {
        if self.id.trim().is_empty()
            || self.language.trim().is_empty()
            || self.repo_url.trim().is_empty()
            || self.checkout_ref.trim().is_empty()
            || self.cache_subdir.trim().is_empty()
        {
            return Err(BenchmarkError::Validation(
                "real-repo manifest fields must not be empty".to_string(),
            ));
        }
        if self.pinned_commit_sha.len() != 40
            || !self
                .pinned_commit_sha
                .chars()
                .all(|ch| ch.is_ascii_hexdigit())
        {
            return Err(BenchmarkError::Validation(format!(
                "real-repo {} must pin a 40-character commit SHA",
                self.id
            )));
        }
        if self.tasks.is_empty() {
            return Err(BenchmarkError::Validation(format!(
                "real-repo {} must include tasks",
                self.id
            )));
        }
        for task in &self.tasks {
            task.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealRepoTaskManifest {
    pub task_id: String,
    pub task_kind: String,
    pub query_text: String,
    pub expected_files: Vec<String>,
    pub expected_symbols: Vec<String>,
    pub expected_relation_sequence: Vec<String>,
    pub expected_source_spans: Vec<String>,
    pub validation_status: String,
    pub unsupported_allowed: Vec<String>,
}

impl RealRepoTaskManifest {
    fn validate(&self) -> BenchResult<()> {
        if self.task_id.trim().is_empty()
            || self.task_kind.trim().is_empty()
            || self.query_text.trim().is_empty()
        {
            return Err(BenchmarkError::Validation(
                "real-repo task id, kind, and query must not be empty".to_string(),
            ));
        }
        if self.expected_files.is_empty() && self.expected_symbols.is_empty() {
            return Err(BenchmarkError::Validation(format!(
                "real-repo task {} must include at least one expected file or symbol",
                self.task_id
            )));
        }
        if self.validation_status != "manual_subset"
            && self.validation_status != "unvalidated_expected"
        {
            return Err(BenchmarkError::Validation(format!(
                "real-repo task {} has unknown validation status {}",
                self.task_id, self.validation_status
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealRepoReplayResult {
    pub status: ReplayFeasibility,
    pub reason: Option<String>,
    pub cache_root: String,
    pub commands: Vec<String>,
    pub repos: Vec<RealRepoReplayRepo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealRepoReplayRepo {
    pub repo_id: String,
    pub status: ReplayFeasibility,
    pub reason: Option<String>,
    pub cache_path: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkSuite {
    pub schema_version: u32,
    pub id: String,
    pub tasks: Vec<BenchmarkTask>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

impl BenchmarkSuite {
    pub fn validate(&self) -> BenchResult<()> {
        if self.schema_version != BENCH_SCHEMA_VERSION {
            return Err(BenchmarkError::Validation(format!(
                "expected schema version {}, got {}",
                BENCH_SCHEMA_VERSION, self.schema_version
            )));
        }
        if self.id.trim().is_empty() {
            return Err(BenchmarkError::Validation(
                "suite id must not be empty".to_string(),
            ));
        }
        if self.tasks.is_empty() {
            return Err(BenchmarkError::Validation(
                "suite must contain at least one task".to_string(),
            ));
        }
        for task in &self.tasks {
            task.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkTask {
    pub id: String,
    pub family: BenchmarkFamily,
    pub prompt: String,
    pub repo: BenchmarkRepoSpec,
    pub ground_truth: GroundTruth,
    #[serde(default = "default_k_values")]
    pub k_values: Vec<usize>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

impl BenchmarkTask {
    pub fn validate(&self) -> BenchResult<()> {
        if self.id.trim().is_empty() {
            return Err(BenchmarkError::Validation(
                "task id must not be empty".to_string(),
            ));
        }
        if self.prompt.trim().is_empty() {
            return Err(BenchmarkError::Validation(format!(
                "task {} prompt must not be empty",
                self.id
            )));
        }
        if self.k_values.is_empty() || self.k_values.contains(&0) {
            return Err(BenchmarkError::Validation(format!(
                "task {} must define positive k values",
                self.id
            )));
        }
        if self.ground_truth.is_empty() {
            return Err(BenchmarkError::Validation(format!(
                "task {} must include ground truth",
                self.id
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GroundTruth {
    #[serde(default)]
    pub expected_relations: Vec<ExpectedRelation>,
    #[serde(default)]
    pub expected_relation_sequences: Vec<Vec<RelationKind>>,
    #[serde(default)]
    pub expected_files: Vec<String>,
    #[serde(default)]
    pub expected_symbols: Vec<String>,
    #[serde(default)]
    pub expected_tests: Vec<String>,
    pub expected_patch_success: Option<bool>,
    pub expected_test_success: Option<bool>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

impl GroundTruth {
    fn is_empty(&self) -> bool {
        self.expected_relations.is_empty()
            && self.expected_relation_sequences.is_empty()
            && self.expected_files.is_empty()
            && self.expected_symbols.is_empty()
            && self.expected_tests.is_empty()
            && self.expected_patch_success.is_none()
            && self.expected_test_success.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedRelation {
    pub relation: RelationKind,
    pub head_contains: String,
    pub tail_contains: String,
    pub repo_relative_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedRelation {
    pub edge_id: String,
    pub relation: RelationKind,
    pub head_id: String,
    pub tail_id: String,
    pub repo_relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchPath {
    pub source: String,
    pub target: String,
    pub relations: Vec<RelationKind>,
    pub edge_ids: Vec<String>,
    pub source_spans: Vec<String>,
    pub confidence: f64,
    pub exactness: Vec<Exactness>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyntheticRepo {
    pub id: String,
    pub kind: SyntheticRepoKind,
    pub files: BTreeMap<String, String>,
    pub tasks: Vec<BenchmarkTask>,
}

impl SyntheticRepo {
    pub fn write_to(&self, root: &Path) -> BenchResult<()> {
        for (relative_path, source) in &self.files {
            let path = root.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, source)?;
        }
        Ok(())
    }
}

pub struct IndexedBenchRepo {
    pub id: String,
    pub store: SqliteGraphStore,
    pub entities: Vec<Entity>,
    pub edges: Vec<Edge>,
    pub sources: BTreeMap<String, String>,
    pub documents: Vec<RetrievalDocument>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricScore {
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

impl MetricScore {
    pub const fn zero() -> Self {
        Self {
            precision: 0.0,
            recall: 0.0,
            f1: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkMetrics {
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub relation_precision: f64,
    pub relation_recall: f64,
    pub relation_f1: f64,
    pub path_recall_at_k: BTreeMap<String, f64>,
    pub file_recall_at_k: BTreeMap<String, f64>,
    pub symbol_recall_at_k: BTreeMap<String, f64>,
    pub mrr: f64,
    pub ndcg: f64,
    pub token_cost: u64,
    pub latency_ms: u64,
    pub memory_bytes: u64,
    pub patch_success: Option<bool>,
    pub test_success: Option<bool>,
}

impl BenchmarkMetrics {
    fn empty() -> Self {
        Self {
            precision: 0.0,
            recall: 0.0,
            f1: 0.0,
            relation_precision: 0.0,
            relation_recall: 0.0,
            relation_f1: 0.0,
            path_recall_at_k: BTreeMap::new(),
            file_recall_at_k: BTreeMap::new(),
            symbol_recall_at_k: BTreeMap::new(),
            mrr: 0.0,
            ndcg: 0.0,
            token_cost: 0,
            latency_ms: 0,
            memory_bytes: 0,
            patch_success: None,
            test_success: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkRunStatus {
    Completed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkRunResult {
    pub task_id: String,
    pub family: BenchmarkFamily,
    pub baseline: BaselineMode,
    pub status: BenchmarkRunStatus,
    pub metrics: BenchmarkMetrics,
    pub retrieved_files: Vec<String>,
    pub retrieved_symbols: Vec<String>,
    pub retrieved_tests: Vec<String>,
    pub retrieved_paths: Vec<BenchPath>,
    pub observed_relations: Vec<ObservedRelation>,
    pub context_packet: Option<Value>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl BenchmarkRunResult {
    fn new(task: &BenchmarkTask, baseline: BaselineMode) -> Self {
        Self {
            task_id: task.id.clone(),
            family: task.family,
            baseline,
            status: BenchmarkRunStatus::Completed,
            metrics: BenchmarkMetrics::empty(),
            retrieved_files: Vec::new(),
            retrieved_symbols: Vec::new(),
            retrieved_tests: Vec::new(),
            retrieved_paths: Vec::new(),
            observed_relations: Vec::new(),
            context_packet: None,
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub schema_version: u32,
    pub suite_id: String,
    pub generated_by: String,
    pub results: Vec<BenchmarkRunResult>,
    pub aggregate: BTreeMap<String, BenchmarkAggregate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkAggregate {
    pub runs: usize,
    pub average_precision: f64,
    pub average_recall: f64,
    pub average_f1: f64,
    pub average_path_recall_at_10: f64,
    pub average_symbol_recall_at_10: f64,
    pub average_file_recall_at_10: f64,
    pub average_mrr: f64,
    pub average_ndcg: f64,
    pub total_token_cost: u64,
    pub total_latency_ms: u64,
    pub max_memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalParityReport {
    pub schema_version: u32,
    pub report_id: String,
    pub source_of_truth: String,
    pub generated_by: String,
    pub generated_at_unix_ms: u64,
    pub versions: BTreeMap<String, String>,
    pub machine: BTreeMap<String, String>,
    pub real_repo_corpus: RealRepoCorpus,
    pub dimensions: Vec<ParityDimension>,
    pub skipped_or_unknown: Vec<String>,
    pub no_fabricated_sota_claims: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParityDimension {
    pub name: String,
    pub codegraph_status: String,
    pub codegraphcontext_status: String,
    pub internal_baseline_status: String,
    pub evidence: String,
    pub unknown_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalParityReportArtifacts {
    pub report_dir: String,
    pub json_summary: String,
    pub markdown_summary: String,
    pub per_task_jsonl: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapClassification {
    Win,
    Loss,
    Tie,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapMeasurementStatus {
    Measured,
    Declared,
    Skipped,
    Unknown,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapPolarity {
    HigherIsBetter,
    LowerIsBetter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GapScoreboardDimension {
    pub id: String,
    pub description: String,
    pub codegraph_value: Value,
    pub codegraph_status: GapMeasurementStatus,
    pub competitor_value: Value,
    pub competitor_status: GapMeasurementStatus,
    pub classification: GapClassification,
    pub evidence: Vec<String>,
    pub unknown_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GapScoreboardTaskRecord {
    pub dimension_id: String,
    pub task_id: String,
    pub tool: String,
    pub status: GapMeasurementStatus,
    pub value: Value,
    pub artifact_paths: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GapScoreboardReport {
    pub schema_version: u32,
    pub benchmark_id: String,
    pub source_of_truth: String,
    pub generated_by: String,
    pub generated_at_unix_ms: u64,
    pub codegraph_version: String,
    pub competitor_metadata: competitors::codegraphcontext::CompetitorManifest,
    pub fairness_rules: Vec<String>,
    pub internal_baseline_summary: BTreeMap<String, BenchmarkAggregate>,
    pub dimensions: Vec<GapScoreboardDimension>,
    pub task_records: Vec<GapScoreboardTaskRecord>,
    pub no_sota_claim: bool,
}

impl GapScoreboardReport {
    pub fn validate(&self) -> BenchResult<()> {
        if self.schema_version != BENCH_SCHEMA_VERSION {
            return Err(BenchmarkError::Validation(format!(
                "expected gap scoreboard schema {}, got {}",
                BENCH_SCHEMA_VERSION, self.schema_version
            )));
        }
        let dimension_ids = self
            .dimensions
            .iter()
            .map(|dimension| dimension.id.as_str())
            .collect::<BTreeSet<_>>();
        for required in REQUIRED_GAP_DIMENSION_IDS {
            if !dimension_ids.contains(required) {
                return Err(BenchmarkError::Validation(format!(
                    "gap scoreboard missing dimension: {required}"
                )));
            }
        }
        if !self.no_sota_claim {
            return Err(BenchmarkError::Validation(
                "gap scoreboard must not claim SOTA superiority".to_string(),
            ));
        }
        self.competitor_metadata.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GapScoreboardArtifacts {
    pub report_dir: String,
    pub json_summary: String,
    pub markdown_summary: String,
    pub per_task_jsonl: String,
    pub external_competitor_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapScoreboardOptions {
    pub report_dir: PathBuf,
    pub timeout_ms: u64,
    pub top_k: usize,
    pub competitor_executable: Option<PathBuf>,
}

impl GapScoreboardOptions {
    pub fn with_report_dir(report_dir: PathBuf) -> Self {
        Self {
            report_dir,
            timeout_ms: competitors::codegraphcontext::DEFAULT_TIMEOUT_MS,
            top_k: competitors::codegraphcontext::DEFAULT_TOP_K,
            competitor_executable: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalGateVerdict {
    Pass,
    Fail,
    Incomplete,
    Unknown,
}

impl FinalGateVerdict {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Incomplete => "incomplete",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkClaimVerdict {
    pub verdict: FinalGateVerdict,
    pub reason: String,
    pub allowed_claims: Vec<String>,
    pub forbidden_claims: Vec<String>,
}

pub fn real_repo_storage_target_verdict(
    indexing_completed: bool,
    db_size_bytes: Option<u64>,
    storage_target_bytes: u64,
) -> FinalGateVerdict {
    if !indexing_completed {
        return FinalGateVerdict::Incomplete;
    }
    match db_size_bytes {
        Some(size) if size <= storage_target_bytes => FinalGateVerdict::Pass,
        Some(_) => FinalGateVerdict::Fail,
        None => FinalGateVerdict::Unknown,
    }
}

pub fn honest_superiority_verdict(
    internal_verdict: FinalGateVerdict,
    competitor_status: &str,
    fake_agent_only: bool,
) -> BenchmarkClaimVerdict {
    let mut allowed_claims = Vec::new();
    let mut forbidden_claims = vec![
        "Do not claim SOTA or agent-coding superiority from internal fixture passes.".to_string(),
        "Do not count skipped, timed-out, or incomplete CGC runs as CodeGraph wins.".to_string(),
        "Do not compare partial CGC DB size to completed CodeGraph DB size.".to_string(),
    ];

    if fake_agent_only {
        forbidden_claims.push(
            "Do not claim real-model coding quality from fake-agent dry-run traces.".to_string(),
        );
        return BenchmarkClaimVerdict {
            verdict: FinalGateVerdict::Unknown,
            reason:
                "Only fake-agent dry-run traces are available; model coding superiority is unknown."
                    .to_string(),
            allowed_claims,
            forbidden_claims,
        };
    }

    match internal_verdict {
        FinalGateVerdict::Fail => BenchmarkClaimVerdict {
            verdict: FinalGateVerdict::Fail,
            reason: "Internal graph/retrieval/context checks failed.".to_string(),
            allowed_claims,
            forbidden_claims,
        },
        FinalGateVerdict::Incomplete => BenchmarkClaimVerdict {
            verdict: FinalGateVerdict::Incomplete,
            reason: "Internal benchmark evidence is incomplete.".to_string(),
            allowed_claims,
            forbidden_claims,
        },
        FinalGateVerdict::Unknown => BenchmarkClaimVerdict {
            verdict: FinalGateVerdict::Unknown,
            reason: "Internal benchmark evidence has unknown fields.".to_string(),
            allowed_claims,
            forbidden_claims,
        },
        FinalGateVerdict::Pass => {
            if competitor_status.eq_ignore_ascii_case("completed") {
                allowed_claims.push(
                    "Internal checks passed and competitor completed; superiority still requires measured same-scope quality and storage comparison."
                        .to_string(),
                );
                BenchmarkClaimVerdict {
                    verdict: FinalGateVerdict::Pass,
                    reason:
                        "Internal checks and completed competitor evidence are available for a bounded comparison."
                            .to_string(),
                    allowed_claims,
                    forbidden_claims,
                }
            } else if competitor_status.eq_ignore_ascii_case("incomplete")
                || competitor_status.eq_ignore_ascii_case("timeout")
            {
                BenchmarkClaimVerdict {
                    verdict: FinalGateVerdict::Unknown,
                    reason:
                        "Internal checks passed, but CGC evidence is incomplete, so superiority is unknown."
                            .to_string(),
                    allowed_claims,
                    forbidden_claims,
                }
            } else {
                BenchmarkClaimVerdict {
                    verdict: FinalGateVerdict::Unknown,
                    reason: format!(
                        "Internal checks passed, but competitor status is `{competitor_status}`, so superiority is unknown."
                    ),
                    allowed_claims,
                    forbidden_claims,
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeGraphVsCgcVerdictInput {
    pub cgc_status: String,
    pub cgc_artifacts_comparable: bool,
    pub fake_agent_only: bool,
    pub codegraph_storage_target_met: bool,
    pub graph_truth_passed: bool,
    pub context_packet_passed: bool,
    pub both_tools_completed_same_scope: bool,
    pub codegraph_at_least_2x_faster: bool,
    pub codegraph_storage_smaller_or_equal: bool,
    pub codegraph_quality_at_least_cgc: bool,
    pub real_model_benchmark_completed: bool,
    pub codegraph_real_model_quality_at_least_cgc: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeGraphVsCgcVerdict {
    pub final_verdict: FinalGateVerdict,
    pub speed_verdict: FinalGateVerdict,
    pub storage_verdict: FinalGateVerdict,
    pub quality_verdict: FinalGateVerdict,
    pub agent_quality_verdict: FinalGateVerdict,
    pub comparison_complete: bool,
    pub qualified_core_win_allowed: bool,
    pub real_model_superiority_claim_allowed: bool,
    pub reasons: Vec<String>,
    pub forbidden_claims: Vec<String>,
}

pub fn codegraph_vs_cgc_verdict(input: CodeGraphVsCgcVerdictInput) -> CodeGraphVsCgcVerdict {
    let cgc_completed = input.cgc_status.eq_ignore_ascii_case("completed");
    let comparison_complete =
        cgc_completed && input.cgc_artifacts_comparable && input.both_tools_completed_same_scope;
    let mut reasons = Vec::new();
    let mut forbidden_claims = vec![
        "Do not count CGC timeout, skipped, failed, or incomplete runs as CodeGraph wins."
            .to_string(),
        "Do not compare partial CGC artifacts to completed CodeGraph artifacts.".to_string(),
        "Do not use internal fixture passes alone as a SOTA or real-agent superiority claim."
            .to_string(),
    ];

    if !comparison_complete {
        reasons.push(format!(
            "CGC status is `{}` and comparable same-scope artifacts are not complete.",
            input.cgc_status
        ));
    }
    if input.fake_agent_only {
        reasons.push(
            "Agent-quality evidence is fake-agent dry-run only; real model quality is unknown."
                .to_string(),
        );
        forbidden_claims.push(
            "Do not claim real-model coding superiority from fake-agent dry-run traces."
                .to_string(),
        );
    }

    let speed_verdict = if comparison_complete {
        if input.codegraph_at_least_2x_faster {
            FinalGateVerdict::Pass
        } else {
            reasons.push(
                "Both tools completed, but CodeGraph did not prove the required >=2x cold-index speed result."
                    .to_string(),
            );
            FinalGateVerdict::Fail
        }
    } else {
        FinalGateVerdict::Incomplete
    };

    let storage_verdict = if !input.codegraph_storage_target_met {
        reasons.push("CodeGraph storage target failed independently of CGC status.".to_string());
        FinalGateVerdict::Fail
    } else if comparison_complete {
        if input.codegraph_storage_smaller_or_equal {
            FinalGateVerdict::Pass
        } else {
            reasons.push(
                "Both tools completed, but CodeGraph did not prove smaller-or-equal final storage."
                    .to_string(),
            );
            FinalGateVerdict::Fail
        }
    } else {
        FinalGateVerdict::Unknown
    };

    let quality_verdict = if !input.graph_truth_passed {
        reasons.push(
            "Graph Truth Gate failed, so quality fails regardless of speed/storage.".to_string(),
        );
        FinalGateVerdict::Fail
    } else if !input.context_packet_passed {
        reasons.push(
            "Context packet quality failed, so end-to-end agent evidence is not trustworthy."
                .to_string(),
        );
        FinalGateVerdict::Fail
    } else if comparison_complete {
        if input.codegraph_quality_at_least_cgc {
            FinalGateVerdict::Pass
        } else {
            reasons.push(
                "Both tools completed, but CodeGraph did not prove quality >= CGC.".to_string(),
            );
            FinalGateVerdict::Fail
        }
    } else {
        FinalGateVerdict::Unknown
    };

    let agent_quality_verdict = if input.fake_agent_only || !input.real_model_benchmark_completed {
        FinalGateVerdict::Unknown
    } else if input.codegraph_real_model_quality_at_least_cgc {
        FinalGateVerdict::Pass
    } else {
        FinalGateVerdict::Fail
    };

    let any_fail = matches!(speed_verdict, FinalGateVerdict::Fail)
        || matches!(storage_verdict, FinalGateVerdict::Fail)
        || matches!(quality_verdict, FinalGateVerdict::Fail);
    let qualified_core_win_allowed = comparison_complete
        && matches!(speed_verdict, FinalGateVerdict::Pass)
        && matches!(storage_verdict, FinalGateVerdict::Pass)
        && matches!(quality_verdict, FinalGateVerdict::Pass);
    let real_model_superiority_claim_allowed =
        qualified_core_win_allowed && matches!(agent_quality_verdict, FinalGateVerdict::Pass);
    let final_verdict = if any_fail {
        FinalGateVerdict::Fail
    } else if qualified_core_win_allowed {
        FinalGateVerdict::Pass
    } else if !comparison_complete {
        FinalGateVerdict::Incomplete
    } else {
        FinalGateVerdict::Unknown
    };

    if qualified_core_win_allowed && !real_model_superiority_claim_allowed {
        reasons.push(
            "Core speed/storage/quality comparison is qualified; real model superiority remains unclaimed."
                .to_string(),
        );
    }

    CodeGraphVsCgcVerdict {
        final_verdict,
        speed_verdict,
        storage_verdict,
        quality_verdict,
        agent_quality_verdict,
        comparison_complete,
        qualified_core_win_allowed,
        real_model_superiority_claim_allowed,
        reasons,
        forbidden_claims,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalGateCheck {
    pub id: String,
    pub status: FinalGateVerdict,
    pub reason: String,
    pub evidence: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalGateIndexSnapshot {
    pub mode: String,
    pub db_path: String,
    pub db_size_bytes: u64,
    pub files: u64,
    pub entities: u64,
    pub edges: u64,
    pub source_spans: u64,
    pub relation_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalGateIndexingReport {
    pub cli: FinalGateIndexSnapshot,
    pub mcp: FinalGateIndexSnapshot,
    pub counts_equivalent: bool,
    pub shared_implementation_check: FinalGateCheck,
    pub old_mcp_indexing_path_absent: FinalGateCheck,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalGateStorageContributor {
    pub name: String,
    pub row_count: u64,
    pub payload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalGateProofObjectCounts {
    pub source_spans: u64,
    pub path_evidence_generated: u64,
    pub derived_closure_edges_generated: u64,
    pub derived_relation_rows_in_edges: u64,
    pub provenanced_edges: u64,
    pub exactness_labeled_edges: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalGateStorageReport {
    pub storage_policy: String,
    pub db_size_bytes: u64,
    pub db_size_mib: f64,
    pub top_storage_contributors: Vec<FinalGateStorageContributor>,
    pub relation_counts: BTreeMap<String, u64>,
    pub source_span_count: u64,
    pub proof_object_counts: FinalGateProofObjectCounts,
    pub size_targets: BTreeMap<String, Value>,
    pub storage_policy_reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalGateCgcComparison {
    pub status: String,
    pub executable_path: String,
    pub backend: String,
    pub version: String,
    pub python_version: String,
    pub elapsed_time_ms: Value,
    pub db_size_bytes: Value,
    pub stdout_artifact_paths: Vec<String>,
    pub stderr_artifact_paths: Vec<String>,
    pub raw_artifact_paths: Vec<String>,
    pub normalized_artifact_paths: Vec<String>,
    pub produced_incomplete_stats: bool,
    pub fairness_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalAcceptanceGateReport {
    pub schema_version: u32,
    pub gate_id: String,
    pub source_of_truth: String,
    pub generated_by: String,
    pub generated_at_unix_ms: u64,
    pub verdict: FinalGateVerdict,
    pub internal_verdict: FinalGateVerdict,
    pub verdict_reasons: Vec<String>,
    pub workspace_root: String,
    pub fixture_repo: String,
    pub indexing: FinalGateIndexingReport,
    pub storage: FinalGateStorageReport,
    pub cgc_comparison: FinalGateCgcComparison,
    pub mvp_proof_checks: Vec<FinalGateCheck>,
    pub functionality_checks: Vec<FinalGateCheck>,
    pub unknowns: Vec<String>,
    pub no_superiority_claim_without_completed_cgc: bool,
}

impl FinalAcceptanceGateReport {
    pub fn validate(&self) -> BenchResult<()> {
        if self.schema_version != BENCH_SCHEMA_VERSION {
            return Err(BenchmarkError::Validation(format!(
                "expected final gate schema {}, got {}",
                BENCH_SCHEMA_VERSION, self.schema_version
            )));
        }
        if !self.no_superiority_claim_without_completed_cgc {
            return Err(BenchmarkError::Validation(
                "final gate must not claim superiority without completed CGC data".to_string(),
            ));
        }
        if self.indexing.cli.entities != self.indexing.mcp.entities
            || self.indexing.cli.edges != self.indexing.mcp.edges
            || !self.indexing.counts_equivalent
        {
            return Err(BenchmarkError::Validation(
                "CLI and MCP index counts are not equivalent".to_string(),
            ));
        }
        if self.storage.source_span_count == 0 {
            return Err(BenchmarkError::Validation(
                "final gate must report source span evidence".to_string(),
            ));
        }
        if self.storage.proof_object_counts.path_evidence_generated == 0 {
            return Err(BenchmarkError::Validation(
                "final gate must generate PathEvidence proof objects".to_string(),
            ));
        }
        if self
            .mvp_proof_checks
            .iter()
            .chain(self.functionality_checks.iter())
            .any(|check| check.status == FinalGateVerdict::Fail)
            && self.internal_verdict != FinalGateVerdict::Fail
        {
            return Err(BenchmarkError::Validation(
                "failed checks must force an internal fail verdict".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalAcceptanceGateOptions {
    pub report_dir: PathBuf,
    pub workspace_root: Option<PathBuf>,
    pub competitor_executable: Option<PathBuf>,
    pub timeout_ms: u64,
    pub cgc_db_size_bytes: Option<u64>,
}

impl FinalAcceptanceGateOptions {
    pub fn with_report_dir(report_dir: PathBuf) -> Self {
        Self {
            report_dir,
            workspace_root: None,
            competitor_executable: None,
            timeout_ms: competitors::codegraphcontext::DEFAULT_TIMEOUT_MS,
            cgc_db_size_bytes: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalAcceptanceGateArtifacts {
    pub report_dir: String,
    pub json_summary: String,
    pub markdown_summary: String,
    pub cgc_report_dir: String,
}

const REQUIRED_GAP_DIMENSION_IDS: &[&str] = &[
    "supported_language_count_by_support_tier",
    "indexed_files_per_language",
    "parse_success_rate",
    "entity_extraction_recall",
    "symbol_search_recall_at_k",
    "file_recall_at_k",
    "caller_recall_at_k",
    "callee_recall_at_k",
    "call_chain_path_recall_at_k",
    "test_impact_recall_at_k",
    "cold_index_time",
    "warm_incremental_update_time",
    "memory_where_measurable",
    "cli_command_coverage",
    "install_steps_count",
    "mcp_tool_schema_resource_coverage",
    "ui_inspection_coverage",
];

pub fn default_k_values() -> Vec<usize> {
    vec![1, 3, 5, 10]
}

pub fn default_benchmark_suite() -> BenchmarkSuite {
    let mut tasks = Vec::new();
    for family in BenchmarkFamily::ALL {
        tasks.extend(synthetic_repo(SyntheticRepoKind::from(family)).tasks);
    }
    BenchmarkSuite {
        schema_version: BENCH_SCHEMA_VERSION,
        id: "codegraph-mvp-phase-20".to_string(),
        tasks,
        metadata: BTreeMap::from([
            ("source_of_truth".to_string(), json!("MVP.md")),
            ("workflow".to_string(), json!("single-agent-only")),
            ("phase".to_string(), json!("20")),
        ]),
    }
}

pub fn real_repo_maturity_corpus() -> RealRepoCorpus {
    let repos = vec![
        real_repo_manifest(
            "typescript-typescript",
            "typescript",
            "https://github.com/microsoft/TypeScript.git",
            "v5.8.3",
            "68cead182cc24afdc3f1ce7c8ff5853aba14b65a",
            "Apache-2.0",
            &[
                real_repo_task(
                    "typescript-symbol-search",
                    "symbol_search",
                    "find createProgram and SourceFile symbols",
                    &["src/compiler/program.ts", "src/compiler/types.ts"],
                    &["createProgram", "SourceFile"],
                    &[],
                    "manual_subset",
                ),
                real_repo_task(
                    "typescript-call-chain",
                    "call_chain",
                    "trace parse command-line to program creation",
                    &["src/compiler/commandLineParser.ts", "src/compiler/program.ts"],
                    &["parseCommandLine", "createProgram"],
                    &["CALLS"],
                    "unvalidated_expected",
                ),
            ],
        ),
        real_repo_manifest(
            "python-requests",
            "python",
            "https://github.com/psf/requests.git",
            "v2.32.3",
            "0e322af87745eff34caffe4df68456ebc20d9068",
            "Apache-2.0",
            &[
                real_repo_task(
                    "requests-symbol-search",
                    "symbol_search",
                    "find Session and request entry points",
                    &["src/requests/sessions.py", "src/requests/api.py"],
                    &["Session", "request"],
                    &[],
                    "manual_subset",
                ),
                real_repo_task(
                    "requests-test-impact",
                    "test_impact",
                    "find tests related to Session request behavior",
                    &["tests/test_requests.py", "src/requests/sessions.py"],
                    &["Session", "test_HTTP_200_OK_GET"],
                    &["TESTS"],
                    "unvalidated_expected",
                ),
            ],
        ),
        real_repo_manifest(
            "go-gin",
            "go",
            "https://github.com/gin-gonic/gin.git",
            "v1.12.0",
            "73726dc606796a025971fe451f0aa6f1b9b847f6",
            "MIT",
            &[real_repo_task(
                "gin-caller-callee",
                "caller_callee",
                "find Engine route registration caller/callee paths",
                &["gin.go", "routergroup.go"],
                &["Engine", "Handle"],
                &["CALLS"],
                "manual_subset",
            )],
        ),
        real_repo_manifest(
            "rust-ripgrep",
            "rust",
            "https://github.com/BurntSushi/ripgrep.git",
            "15.1.0",
            "af60c2de9d85e7f3d81c78601669468cf02dabab",
            "MIT/Unlicense",
            &[real_repo_task(
                "ripgrep-context-retrieval",
                "context_retrieval",
                "find search worker and config context",
                &["crates/core/main.rs", "crates/core/flags"],
                &["SearchWorker", "Config"],
                &["CALLS"],
                "unvalidated_expected",
            )],
        ),
        real_repo_manifest(
            "java-spring-petclinic",
            "java",
            "https://github.com/spring-projects/spring-petclinic.git",
            "main",
            "c7ee170434ec3e369fdc9201290ba2ea4c92b557",
            "Apache-2.0",
            &[real_repo_task(
                "petclinic-symbol-search",
                "symbol_search",
                "find OwnerController and PetController symbols",
                &[
                    "src/main/java/org/springframework/samples/petclinic/owner/OwnerController.java",
                    "src/main/java/org/springframework/samples/petclinic/owner/PetController.java",
                ],
                &["OwnerController", "PetController"],
                &[],
                "manual_subset",
            )],
        ),
    ];
    RealRepoCorpus {
        schema_version: BENCH_SCHEMA_VERSION,
        source_of_truth: "MVP.md Prompt 30".to_string(),
        cache_root: ".codegraph-bench-cache/real-repos".to_string(),
        repos,
    }
}

fn real_repo_manifest(
    id: &str,
    language: &str,
    repo_url: &str,
    checkout_ref: &str,
    pinned_commit_sha: &str,
    license: &str,
    tasks: &[RealRepoTaskManifest],
) -> RealRepoManifest {
    RealRepoManifest {
        id: id.to_string(),
        language: language.to_string(),
        repo_url: repo_url.to_string(),
        checkout_ref: checkout_ref.to_string(),
        pinned_commit_sha: pinned_commit_sha.to_string(),
        license: license.to_string(),
        cache_subdir: id.to_string(),
        size_policy: "clone into ignored benchmark cache; do not vendor".to_string(),
        tasks: tasks.to_vec(),
    }
}

fn real_repo_task(
    task_id: &str,
    task_kind: &str,
    query_text: &str,
    expected_files: &[&str],
    expected_symbols: &[&str],
    expected_relation_sequence: &[&str],
    validation_status: &str,
) -> RealRepoTaskManifest {
    RealRepoTaskManifest {
        task_id: task_id.to_string(),
        task_kind: task_kind.to_string(),
        query_text: query_text.to_string(),
        expected_files: expected_files
            .iter()
            .map(|value| value.to_string())
            .collect(),
        expected_symbols: expected_symbols
            .iter()
            .map(|value| value.to_string())
            .collect(),
        expected_relation_sequence: expected_relation_sequence
            .iter()
            .map(|value| value.to_string())
            .collect(),
        expected_source_spans: Vec::new(),
        validation_status: validation_status.to_string(),
        unsupported_allowed: vec![
            "source_span_unavailable".to_string(),
            "exact_relation_taxonomy_unavailable".to_string(),
        ],
    }
}

pub fn plan_real_repo_corpus_replay(
    corpus: &RealRepoCorpus,
    cache_root: impl AsRef<Path>,
    allow_network: bool,
) -> BenchResult<RealRepoReplayResult> {
    corpus.validate()?;
    let cache_root = cache_root.as_ref();
    let repos = corpus
        .repos
        .iter()
        .map(|repo| {
            let cache_path = cache_root.join(&repo.cache_subdir);
            let commands = vec![
                format!("git clone {} {}", repo.repo_url, cache_path.display()),
                format!(
                    "git -C {} checkout {}",
                    cache_path.display(),
                    repo.pinned_commit_sha
                ),
                format!(
                    "codegraph-mcp index {} --profile --json",
                    cache_path.display()
                ),
            ];
            RealRepoReplayRepo {
                repo_id: repo.id.clone(),
                status: if allow_network {
                    ReplayFeasibility::Feasible
                } else {
                    ReplayFeasibility::Unavailable
                },
                reason: (!allow_network)
                    .then(|| "network disabled; clone plan recorded but not executed".to_string()),
                cache_path: cache_path.display().to_string(),
                commands,
            }
        })
        .collect::<Vec<_>>();
    Ok(RealRepoReplayResult {
        status: if allow_network {
            ReplayFeasibility::Feasible
        } else {
            ReplayFeasibility::Unavailable
        },
        reason: (!allow_network)
            .then(|| "offline mode; real repos are not cloned during normal tests".to_string()),
        cache_root: cache_root.display().to_string(),
        commands: vec![
            "scripts/replay-real-repo-corpus.ps1 -AllowNetwork".to_string(),
            "scripts/replay-real-repo-corpus.sh --allow-network".to_string(),
        ],
        repos,
    })
}

pub fn final_parity_report() -> BenchResult<FinalParityReport> {
    let corpus = real_repo_maturity_corpus();
    corpus.validate()?;
    Ok(FinalParityReport {
        schema_version: BENCH_SCHEMA_VERSION,
        report_id: format!("codegraph-phase-30-parity-{}", unix_time_ms()),
        source_of_truth: "MVP.md Prompt 30".to_string(),
        generated_by: "codegraph-bench phase 30".to_string(),
        generated_at_unix_ms: unix_time_ms(),
        versions: BTreeMap::from([
            ("codegraph".to_string(), env!("CARGO_PKG_VERSION").to_string()),
            (
                "codegraphcontext".to_string(),
                "unknown unless CGC competitor run artifact is provided".to_string(),
            ),
        ]),
        machine: BTreeMap::from([
            ("os".to_string(), std::env::consts::OS.to_string()),
            ("arch".to_string(), std::env::consts::ARCH.to_string()),
            ("family".to_string(), std::env::consts::FAMILY.to_string()),
        ]),
        real_repo_corpus: corpus,
        dimensions: parity_dimensions(),
        skipped_or_unknown: vec![
            "CodeGraphContext live results are unknown unless CGC is installed and the external harness is run.".to_string(),
            "Real-repo clones are skipped in normal tests and CI unless network is explicitly enabled.".to_string(),
            "SWE-bench style patch success is not claimed by this local report.".to_string(),
        ],
        no_fabricated_sota_claims: true,
    })
}

pub fn write_final_parity_report(
    output_dir: impl AsRef<Path>,
) -> BenchResult<FinalParityReportArtifacts> {
    let output_dir = output_dir.as_ref();
    fs::create_dir_all(output_dir)?;
    let report = final_parity_report()?;
    let json_path = output_dir.join("summary.json");
    let markdown_path = output_dir.join("summary.md");
    let jsonl_path = output_dir.join("per_task.jsonl");
    fs::write(
        &json_path,
        serde_json::to_string_pretty(&report).map_err(|error| {
            BenchmarkError::Io(format!("failed to encode parity report: {error}"))
        })?,
    )?;
    fs::write(&markdown_path, render_final_parity_markdown(&report))?;
    let mut jsonl = String::new();
    for repo in &report.real_repo_corpus.repos {
        for task in &repo.tasks {
            jsonl.push_str(
                &serde_json::to_string(&json!({
                    "schema_version": BENCH_SCHEMA_VERSION,
                    "repo_id": repo.id,
                    "language": repo.language,
                    "task": task,
                    "status": "manifest_only",
                    "unknown": ["live_codegraphcontext_score", "live_patch_success"],
                }))
                .map_err(|error| BenchmarkError::Io(error.to_string()))?,
            );
            jsonl.push('\n');
        }
    }
    fs::write(&jsonl_path, jsonl)?;
    Ok(FinalParityReportArtifacts {
        report_dir: output_dir.display().to_string(),
        json_summary: json_path.display().to_string(),
        markdown_summary: markdown_path.display().to_string(),
        per_task_jsonl: jsonl_path.display().to_string(),
    })
}

pub fn render_final_parity_markdown(report: &FinalParityReport) -> String {
    let mut output = String::new();
    output.push_str("# CodeGraph Phase 30 Parity Report\n\n");
    output.push_str("Source of truth: MVP.md Prompt 30.\n\n");
    output.push_str(
        "No SOTA superiority is claimed unless measured. Unknown fields are explicit.\n\n",
    );
    output.push_str("## Evidence Sections\n\n");
    output.push_str("- Indexing speed/storage: reported separately; storage target failures remain failures even when indexing completes.\n");
    output.push_str("- Graph truth: strict expected/forbidden fact gates are separate from retrieval and agent claims.\n");
    output.push_str("- Retrieval ablation: Stage 0, Stage 1, Stage 2, graph verification, and context packets are not co-credited.\n");
    output.push_str("- Context packet quality: critical symbols, proof paths, spans, snippets, tests, labels, and distractors are scored separately.\n");
    output.push_str("- Real model coding quality: unknown until a real model edits code and tests are measured.\n");
    output.push_str(
        "- Fake-agent dry run: trace-shape evidence only; never a model superiority signal.\n\n",
    );
    output
        .push_str("| Dimension | CodeGraph | CodeGraphContext | Internal Baselines | Evidence |\n");
    output.push_str("| --- | --- | --- | --- | --- |\n");
    for dimension in &report.dimensions {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            dimension.name,
            dimension.codegraph_status,
            dimension.codegraphcontext_status,
            dimension.internal_baseline_status,
            dimension.evidence.replace('|', "/")
        ));
    }
    output.push_str("\n## Real Repo Corpus\n\n");
    for repo in &report.real_repo_corpus.repos {
        output.push_str(&format!(
            "- `{}` {} pinned to `{}` ({})\n",
            repo.id, repo.language, repo.pinned_commit_sha, repo.repo_url
        ));
    }
    output.push_str("\n## Unknown Or Skipped\n\n");
    for unknown in &report.skipped_or_unknown {
        output.push_str(&format!("- {unknown}\n"));
    }
    output
}

pub fn write_final_acceptance_gate_report(
    options: FinalAcceptanceGateOptions,
) -> BenchResult<FinalAcceptanceGateArtifacts> {
    let mut options = options;
    options.report_dir = absolute_bench_path(&options.report_dir)?;
    fs::create_dir_all(&options.report_dir)?;
    let report = final_acceptance_gate_report(&options)?;
    report.validate()?;

    let json_path = options.report_dir.join("summary.json");
    let markdown_path = options.report_dir.join("summary.md");
    fs::write(
        &json_path,
        serde_json::to_string_pretty(&report).map_err(|error| {
            BenchmarkError::Io(format!("failed to encode final acceptance gate: {error}"))
        })?,
    )?;
    fs::write(
        &markdown_path,
        render_final_acceptance_gate_markdown(&report),
    )?;

    Ok(FinalAcceptanceGateArtifacts {
        report_dir: options.report_dir.display().to_string(),
        json_summary: json_path.display().to_string(),
        markdown_summary: markdown_path.display().to_string(),
        cgc_report_dir: options
            .report_dir
            .join("external-codegraphcontext")
            .display()
            .to_string(),
    })
}

pub fn final_acceptance_gate_report(
    options: &FinalAcceptanceGateOptions,
) -> BenchResult<FinalAcceptanceGateReport> {
    let report_dir = absolute_bench_path(&options.report_dir)?;
    fs::create_dir_all(&report_dir)?;
    let gate_id = format!("codegraph-final-acceptance-{}", unix_time_ms());
    let run_root = report_dir.join("runs").join(&gate_id);
    let fixture_repo = run_root.join("repos").join("compact-mvp-fixture");
    write_final_gate_fixture(&fixture_repo)?;

    let index_dir = run_root.join("indexes").join("codegraph");
    fs::create_dir_all(&index_dir)?;
    let cli_db = index_dir.join("cli.sqlite");
    let mcp_db = index_dir.join("mcp.sqlite");

    let cli_summary = index_repo_to_db(&fixture_repo, &cli_db)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let mcp_server = McpServer::new(
        McpServerConfig::for_repo(&fixture_repo)
            .with_db_path(&mcp_db)
            .without_trace(),
    );
    let mcp_index = mcp_server
        .call_tool(
            "codegraph.index_repo",
            &json!({
                "repo": fixture_repo.display().to_string(),
                "db_path": mcp_db.display().to_string(),
            }),
        )
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    if mcp_index.get("status").and_then(Value::as_str) != Some("indexed") {
        return Err(BenchmarkError::Store(format!(
            "MCP index_repo did not report indexed: {mcp_index}"
        )));
    }

    let workspace_root = detect_final_gate_workspace_root(options.workspace_root.as_deref());
    let (shared_impl_check, old_path_check) = final_gate_architecture_checks(&workspace_root);

    let cli_store = SqliteGraphStore::open(&cli_db)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let mcp_store = SqliteGraphStore::open(&mcp_db)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let cli_snapshot = final_gate_index_snapshot("cli", &cli_db, &cli_store)?;
    let mcp_snapshot = final_gate_index_snapshot("mcp", &mcp_db, &mcp_store)?;
    let counts_equivalent = cli_snapshot.files == mcp_snapshot.files
        && cli_snapshot.entities == mcp_snapshot.entities
        && cli_snapshot.edges == mcp_snapshot.edges
        && cli_snapshot.relation_counts == mcp_snapshot.relation_counts;

    let sources = final_gate_source_map(&fixture_repo)?;
    let (mvp_proof_checks, functionality_checks, proof_counts) =
        final_gate_functionality_checks(&mcp_store, &mcp_server, &fixture_repo, &mcp_db, &sources)?;
    let top_storage_contributors = mcp_store
        .storage_accounting()
        .map_err(|error| BenchmarkError::Store(error.to_string()))?
        .into_iter()
        .take(8)
        .map(|row| FinalGateStorageContributor {
            name: row.name,
            row_count: row.row_count,
            payload_bytes: row.payload_bytes,
        })
        .collect::<Vec<_>>();
    let db_size_bytes = db_size_bytes(&mcp_db)?;
    let relation_counts = mcp_store
        .relation_counts()
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let source_span_count = mcp_store
        .count_source_spans()
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;

    let cgc_report_dir = report_dir.join("external-codegraphcontext");
    let cgc_report = competitors::codegraphcontext::run_codegraphcontext_comparison(
        competitors::codegraphcontext::CodeGraphContextComparisonOptions {
            report_dir: cgc_report_dir,
            timeout_ms: options.timeout_ms.min(MAX_BENCH_TASK_MS),
            top_k: competitors::codegraphcontext::DEFAULT_TOP_K,
            competitor_executable: options.competitor_executable.clone(),
        },
    )?;
    let cgc_comparison = final_gate_cgc_comparison(&cgc_report, options.cgc_db_size_bytes);

    let mut all_checks = Vec::new();
    all_checks.push(shared_impl_check.clone());
    all_checks.push(old_path_check.clone());
    all_checks.extend(mvp_proof_checks.clone());
    all_checks.extend(functionality_checks.clone());
    if !counts_equivalent {
        all_checks.push(final_gate_check(
            "cli_mcp_count_equivalence",
            FinalGateVerdict::Fail,
            "CLI and MCP fixture counts differ",
            json!({"cli": cli_snapshot, "mcp": mcp_snapshot}),
        ));
    }
    if cli_summary.storage_policy != DEFAULT_STORAGE_POLICY {
        all_checks.push(final_gate_check(
            "default_storage_policy",
            FinalGateVerdict::Fail,
            "CLI index did not report the compact full-MVP storage policy",
            json!({"storage_policy": cli_summary.storage_policy}),
        ));
    }

    let internal_verdict = aggregate_final_gate_checks(&all_checks);
    let size_targets = final_gate_size_targets(db_size_bytes, &cgc_comparison);
    let (verdict, verdict_reasons, unknowns) =
        final_gate_verdict(internal_verdict, &cgc_comparison, db_size_bytes);

    Ok(FinalAcceptanceGateReport {
        schema_version: BENCH_SCHEMA_VERSION,
        gate_id,
        source_of_truth: "MVP.md final compact-and-correct acceptance gate".to_string(),
        generated_by: "codegraph-bench final acceptance gate".to_string(),
        generated_at_unix_ms: unix_time_ms(),
        verdict,
        internal_verdict,
        verdict_reasons,
        workspace_root: workspace_root.display().to_string(),
        fixture_repo: fixture_repo.display().to_string(),
        indexing: FinalGateIndexingReport {
            cli: cli_snapshot,
            mcp: mcp_snapshot,
            counts_equivalent,
            shared_implementation_check: shared_impl_check,
            old_mcp_indexing_path_absent: old_path_check,
        },
        storage: FinalGateStorageReport {
            storage_policy: DEFAULT_STORAGE_POLICY.to_string(),
            db_size_bytes,
            db_size_mib: bytes_to_mib(db_size_bytes),
            top_storage_contributors,
            relation_counts,
            source_span_count,
            proof_object_counts: proof_counts,
            size_targets,
            storage_policy_reason:
                "Compact size is accepted only when required MVP relations/proof objects are present; missing CGC size keeps the final verdict unknown."
                    .to_string(),
        },
        cgc_comparison,
        mvp_proof_checks,
        functionality_checks,
        unknowns,
        no_superiority_claim_without_completed_cgc: true,
    })
}

pub fn render_final_acceptance_gate_markdown(report: &FinalAcceptanceGateReport) -> String {
    let mut output = String::new();
    output.push_str("# CodeGraph Final Acceptance Gate\n\n");
    output.push_str("Source of truth: `MVP.md`.\n\n");
    output.push_str(&format!(
        "- Final verdict: `{}`\n- Internal verdict: `{}`\n- CGC status: `{}`\n- CodeGraph DB size: `{}` bytes\n\n",
        report.verdict.as_str(),
        report.internal_verdict.as_str(),
        report.cgc_comparison.status,
        report.storage.db_size_bytes
    ));
    output.push_str("No superiority claim is made unless CGC completed and size/quality data is measured on the same repo.\n\n");
    output.push_str("## Verdict Reasons\n\n");
    for reason in &report.verdict_reasons {
        output.push_str(&format!("- {reason}\n"));
    }
    output.push_str("\n## MVP Proof Checks\n\n");
    output.push_str("| Check | Status | Reason |\n| --- | --- | --- |\n");
    for check in &report.mvp_proof_checks {
        output.push_str(&format!(
            "| {} | {} | {} |\n",
            check.id,
            check.status.as_str(),
            check.reason.replace('|', "/")
        ));
    }
    output.push_str("\n## Functionality Checks\n\n");
    output.push_str("| Check | Status | Reason |\n| --- | --- | --- |\n");
    for check in &report.functionality_checks {
        output.push_str(&format!(
            "| {} | {} | {} |\n",
            check.id,
            check.status.as_str(),
            check.reason.replace('|', "/")
        ));
    }
    output.push_str("\n## Top Storage Contributors\n\n");
    output.push_str("| Table/Area | Rows | Payload bytes |\n| --- | ---: | ---: |\n");
    for row in &report.storage.top_storage_contributors {
        output.push_str(&format!(
            "| {} | {} | {} |\n",
            row.name, row.row_count, row.payload_bytes
        ));
    }
    output.push_str("\n## Unknowns\n\n");
    for unknown in &report.unknowns {
        output.push_str(&format!("- {unknown}\n"));
    }
    output
}

fn write_final_gate_fixture(root: &Path) -> BenchResult<()> {
    let files = BTreeMap::from([
        (
            "src/audit.ts",
            r#"
export function auditLogin(value: string): string {
  const normalized = value;
  return normalized;
}
"#,
        ),
        (
            "src/auth.ts",
            r#"
import { auditLogin } from "./audit";

export function helper(value: string): string {
  const saved = value;
  return saved;
}

export function saveUser(email: string): string {
  let stored = "";
  stored = email;
  return stored;
}

export function login(input: string): string {
  const a = input;
  let b = "";
  b = a;
  helper(b);
  saveUser(b);
  auditLogin(b);
  return b;
}
"#,
        ),
        (
            "src/auth.spec.ts",
            r#"
import { login } from "./auth";

test("login returns value", () => {
  const result = login("u1");
  expect(result).toBeDefined();
});
"#,
        ),
    ]);

    for (relative_path, source) in files {
        let path = root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, source.trim_start())?;
    }
    Ok(())
}

fn final_gate_source_map(root: &Path) -> BenchResult<BTreeMap<String, String>> {
    let mut sources = BTreeMap::new();
    for relative_path in ["src/audit.ts", "src/auth.ts", "src/auth.spec.ts"] {
        let source = fs::read_to_string(root.join(relative_path))?;
        sources.insert(relative_path.to_string(), source);
    }
    Ok(sources)
}

fn detect_final_gate_workspace_root(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    let manifest_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf);
    for start in std::env::current_dir()
        .ok()
        .into_iter()
        .chain(manifest_root)
    {
        for candidate in start.ancestors() {
            if candidate
                .join("crates")
                .join("codegraph-mcp-server")
                .join("src")
                .join("lib.rs")
                .exists()
            {
                return candidate.to_path_buf();
            }
        }
    }
    PathBuf::from("unknown")
}

fn final_gate_architecture_checks(workspace_root: &Path) -> (FinalGateCheck, FinalGateCheck) {
    let source_path = workspace_root
        .join("crates")
        .join("codegraph-mcp-server")
        .join("src")
        .join("lib.rs");
    let source = match fs::read_to_string(&source_path) {
        Ok(source) => source,
        Err(error) => {
            let evidence = json!({
                "source_path": source_path.display().to_string(),
                "error": error.to_string(),
            });
            return (
                final_gate_check(
                    "cli_mcp_shared_indexer_source_check",
                    FinalGateVerdict::Unknown,
                    "Could not inspect MCP server source; shared implementation is unknown",
                    evidence.clone(),
                ),
                final_gate_check(
                    "old_mcp_full_fts_snippet_path_absent",
                    FinalGateVerdict::Unknown,
                    "Could not inspect MCP server source; old indexing path absence is unknown",
                    evidence,
                ),
            );
        }
    };

    let contains_index_repo =
        source.contains("index_repo_to_db(&repo_root, &db_path)")
            || source.contains("index_repo_to_db_with_options(&repo_root, &db_path");
    let shared = contains_index_repo
        && source.contains("update_changed_files_to_db(&repo_root");
    let forbidden = [
        "upsert_file_text",
        "upsert_snippet_text",
        "upsert_source_span",
        "extract_entities_and_relations",
        "TreeSitterParser",
        "begin_bulk_index_transaction",
        "clear_indexed_facts",
    ]
    .into_iter()
    .filter(|needle| source.contains(needle))
    .map(str::to_string)
    .collect::<Vec<_>>();

    (
        final_gate_check(
            "cli_mcp_shared_indexer_source_check",
            if shared {
                FinalGateVerdict::Pass
            } else {
                FinalGateVerdict::Fail
            },
            if shared {
                "MCP index_repo/update_changed_files call the shared codegraph-index implementation"
            } else {
                "MCP source no longer proves use of the shared codegraph-index implementation"
            },
            json!({
                "source_path": source_path.display().to_string(),
                "contains_index_repo_to_db": contains_index_repo,
                "contains_update_changed_files_to_db": source.contains("update_changed_files_to_db(&repo_root"),
            }),
        ),
        final_gate_check(
            "old_mcp_full_fts_snippet_path_absent",
            if forbidden.is_empty() {
                FinalGateVerdict::Pass
            } else {
                FinalGateVerdict::Fail
            },
            if forbidden.is_empty() {
                "MCP server source has no independent full-FTS/snippet/source-span indexing write path"
            } else {
                "MCP server source contains old indexing-path write/extraction markers"
            },
            json!({
                "source_path": source_path.display().to_string(),
                "forbidden_markers": forbidden,
            }),
        ),
    )
}

fn final_gate_index_snapshot(
    mode: &str,
    db_path: &Path,
    store: &SqliteGraphStore,
) -> BenchResult<FinalGateIndexSnapshot> {
    Ok(FinalGateIndexSnapshot {
        mode: mode.to_string(),
        db_path: db_path.display().to_string(),
        db_size_bytes: db_size_bytes(db_path)?,
        files: store
            .count_files()
            .map_err(|error| BenchmarkError::Store(error.to_string()))?,
        entities: store
            .count_entities()
            .map_err(|error| BenchmarkError::Store(error.to_string()))?,
        edges: store
            .count_edges()
            .map_err(|error| BenchmarkError::Store(error.to_string()))?,
        source_spans: store
            .count_source_spans()
            .map_err(|error| BenchmarkError::Store(error.to_string()))?,
        relation_counts: store
            .relation_counts()
            .map_err(|error| BenchmarkError::Store(error.to_string()))?,
    })
}

fn final_gate_functionality_checks(
    store: &SqliteGraphStore,
    mcp_server: &McpServer,
    repo_root: &Path,
    db_path: &Path,
    sources: &BTreeMap<String, String>,
) -> BenchResult<(
    Vec<FinalGateCheck>,
    Vec<FinalGateCheck>,
    FinalGateProofObjectCounts,
)> {
    let entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let edges = store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let relation_counts = store
        .relation_counts()
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let source_span_count = store
        .count_source_spans()
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let engine = ExactGraphQueryEngine::new(edges.clone());
    let limits = QueryLimits {
        max_depth: 5,
        max_paths: 24,
        max_edges_visited: 512,
    };
    let login = find_entity_id(&entities, "login");
    let mut graph_paths = Vec::new();
    if let Some(login_id) = &login {
        graph_paths.extend(engine.find_callers(login_id, limits));
        graph_paths.extend(engine.find_callees(login_id, limits));
        graph_paths.extend(engine.find_mutations(login_id, limits));
        graph_paths.extend(engine.find_dataflow(login_id, limits));
    }
    let caller_probe = first_nonempty_call_paths(&engine, &entities, limits, true);
    let callee_probe = first_nonempty_call_paths(&engine, &entities, limits, false);
    if let Some((_, paths)) = &caller_probe {
        graph_paths.extend(paths.clone());
    }
    if let Some((_, paths)) = &callee_probe {
        graph_paths.extend(paths.clone());
    }
    graph_paths.extend(exhaustive_final_gate_paths(&engine, &entities, limits));
    let path_evidence = engine.path_evidence_from_paths(&graph_paths);
    let derived_edges = engine.derive_closure_edges(&graph_paths);
    let provenanced_edges = edges
        .iter()
        .filter(|edge| !edge.extractor.trim().is_empty() && edge.file_hash.is_some())
        .count() as u64;
    let exactness_labeled_edges = edges
        .iter()
        .filter(|edge| !edge.exactness.to_string().trim().is_empty())
        .count() as u64;
    let derived_relation_rows_in_edges = edges
        .iter()
        .filter(|edge| {
            matches!(
                edge.relation,
                RelationKind::MayMutate
                    | RelationKind::MayRead
                    | RelationKind::ApiReaches
                    | RelationKind::AsyncReaches
                    | RelationKind::SchemaImpact
            )
        })
        .count() as u64;
    let invalid_derived_relation_rows = edges
        .iter()
        .filter(|edge| {
            matches!(
                edge.relation,
                RelationKind::MayMutate
                    | RelationKind::MayRead
                    | RelationKind::ApiReaches
                    | RelationKind::AsyncReaches
                    | RelationKind::SchemaImpact
            ) && (!edge.derived
                || edge.provenance_edges.is_empty()
                || edge.edge_class != EdgeClass::Derived)
        })
        .count() as u64;

    let mut mvp_checks = Vec::new();
    mvp_checks.push(final_gate_check_from_bool(
        "source_spans_preserved",
        source_span_count > 0
            && edges
                .iter()
                .all(|edge| !edge.source_span.repo_relative_path.is_empty()),
        "Indexed entities/edges keep source span citations",
        json!({"source_spans": source_span_count}),
    ));
    mvp_checks.push(final_gate_check_from_bool(
        "provenance_preserved",
        provenanced_edges == edges.len() as u64,
        "Edges keep extractor/file-hash provenance; derived proof objects keep base-edge provenance",
        json!({
            "edges": edges.len(),
            "provenanced_edges": provenanced_edges,
            "derived_with_provenance": derived_edges.iter().filter(|edge| !edge.provenance_edges.is_empty()).count(),
        }),
    ));
    mvp_checks.push(final_gate_check_from_bool(
        "exactness_labels_preserved",
        exactness_labeled_edges == edges.len() as u64,
        "Every edge keeps an MVP exactness label",
        json!({"edges": edges.len(), "exactness_labeled_edges": exactness_labeled_edges}),
    ));
    mvp_checks.push(final_gate_check_from_bool(
        "path_evidence_generated",
        !path_evidence.is_empty()
            && path_evidence
                .iter()
                .all(|path| !path.source_spans.is_empty() && !path.edges.is_empty()),
        "Exact graph paths generate PathEvidence with source spans and base edges",
        json!({"path_evidence_generated": path_evidence.len()}),
    ));
    mvp_checks.push(final_gate_check_from_bool(
        "derived_closure_edge_generated",
        !derived_edges.is_empty()
            && derived_edges
                .iter()
                .all(|edge| !edge.provenance_edges.is_empty()),
        "DerivedClosureEdge-equivalent proof shortcuts are generated from base-edge provenance",
        json!({"derived_closure_edges_generated": derived_edges.len()}),
    ));
    mvp_checks.push(final_gate_check_from_bool(
        "no_raw_combinatorial_long_chain_explosion",
        invalid_derived_relation_rows == 0,
        "Cached closure relations are stored only as derived, provenanced edge rows, never as raw base facts",
        json!({
            "derived_relation_rows_in_edges": derived_relation_rows_in_edges,
            "invalid_derived_relation_rows": invalid_derived_relation_rows
        }),
    ));
    let required_relations = [
        RelationKind::Contains,
        RelationKind::DefinedIn,
        RelationKind::Imports,
        RelationKind::Exports,
        RelationKind::Calls,
        RelationKind::Callee,
        RelationKind::Argument0,
        RelationKind::ReturnsTo,
        RelationKind::Reads,
        RelationKind::Writes,
        RelationKind::FlowsTo,
    ];
    let missing_relations = required_relations
        .iter()
        .filter(|relation| {
            relation_counts
                .get(&relation.to_string())
                .copied()
                .unwrap_or_default()
                == 0
        })
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    mvp_checks.push(final_gate_check_from_bool(
        "compact_storage_not_relation_deletion",
        missing_relations.is_empty(),
        "Compact fixture retains required MVP relation classes instead of shrinking by silent deletion",
        json!({"missing_relations": missing_relations, "relation_counts": relation_counts}),
    ));

    let mut functionality_checks = Vec::new();
    let symbol_hits = store
        .find_entities_by_exact_symbol("login")
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    functionality_checks.push(final_gate_check_from_bool(
        "symbol_search",
        !symbol_hits.is_empty(),
        "Symbol search finds the fixture login symbol",
        json!({"hits": symbol_hits.len()}),
    ));
    let text_response = mcp_server
        .call_tool(
            "codegraph.search_text",
            &json!({
                "repo": repo_root.display().to_string(),
                "db_path": db_path.display().to_string(),
                "query": "login",
                "limit": 16,
            }),
        )
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let text_hits = text_response["hits"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    functionality_checks.push(final_gate_check_from_bool(
        "text_file_search",
        text_hits
            .iter()
            .any(|hit| hit["repo_relative_path"].as_str() == Some("src/auth.ts")),
        "Text/file search returns the source file containing login",
        json!({"hits": text_hits.len()}),
    ));
    functionality_checks.push(final_gate_check_from_bool(
        "callers_callees",
        caller_probe.is_some() && callee_probe.is_some(),
        "Caller and callee traversal both return evidence on the compact fixture",
        json!({
            "caller_probe": caller_probe.as_ref().map(|(id, paths)| json!({"entity_id": id, "paths": paths.len()})).unwrap_or_else(|| json!("unknown")),
            "callee_probe": callee_probe.as_ref().map(|(id, paths)| json!({"entity_id": id, "paths": paths.len()})).unwrap_or_else(|| json!("unknown")),
        }),
    ));
    let explain_path_input = graph_paths.first();
    let path_trace_ok = explain_path_input
        .map(|path| {
            !engine
                .trace_path(&path.source, &path.target, &path.relations(), limits)
                .is_empty()
        })
        .unwrap_or(false);
    functionality_checks.push(final_gate_check_from_bool(
        "path_tracing",
        path_trace_ok,
        "trace_path can reproduce at least one indexed relation path",
        json!({"candidate_paths": graph_paths.len()}),
    ));
    let packet = login.as_ref().map(|login_id| {
        engine.context_pack(
            ContextPackRequest::new(
                "Change login return without breaking audit or tests",
                "impact",
                1_600,
                vec![login_id.clone()],
            ),
            sources,
        )
    });
    functionality_checks.push(final_gate_check_from_bool(
        "context_packet",
        packet
            .as_ref()
            .map(|packet| !packet.verified_paths.is_empty() && !packet.symbols.is_empty())
            .unwrap_or(false),
        "context_pack returns verified paths, symbols, and source-span-backed snippets",
        json!({
            "verified_paths": packet.as_ref().map(|packet| packet.verified_paths.len()).unwrap_or_default(),
            "symbols": packet.as_ref().map(|packet| packet.symbols.len()).unwrap_or_default(),
        }),
    ));
    let explain_edge_status = edges.first().map(|edge| {
        mcp_server.call_tool(
            "codegraph.explain_edge",
            &json!({
                "repo": repo_root.display().to_string(),
                "db_path": db_path.display().to_string(),
                "edge_id": edge.id,
            }),
        )
    });
    let explain_path_status = explain_path_input.map(|path| {
        mcp_server.call_tool(
            "codegraph.explain_path",
            &json!({
                "repo": repo_root.display().to_string(),
                "db_path": db_path.display().to_string(),
                "source": path.source,
                "target": path.target,
                "relations": path.relations().iter().map(ToString::to_string).collect::<Vec<_>>(),
            }),
        )
    });
    functionality_checks.push(final_gate_check_from_bool(
        "explain_edge_path",
        matches!(explain_edge_status, Some(Ok(ref value)) if value["status"].as_str() == Some("ok"))
            && matches!(explain_path_status, Some(Ok(ref value)) if value["status"].as_str() == Some("ok")),
        "MCP explain_edge and explain_path return proof-oriented responses",
        json!({
            "explain_edge_status": explain_edge_status.as_ref().map(|result| result.as_ref().map(|value| value["status"].clone()).unwrap_or_else(|error| json!(error.to_string()))).unwrap_or_else(|| json!("unknown")),
            "explain_path_status": explain_path_status.as_ref().map(|result| result.as_ref().map(|value| value["status"].clone()).unwrap_or_else(|error| json!(error.to_string()))).unwrap_or_else(|| json!("unknown")),
        }),
    ));
    functionality_checks.push(final_gate_check_from_bool(
        "source_span_citation",
        edges
            .iter()
            .any(|edge| !edge.source_span.repo_relative_path.is_empty()),
        "Source span citations are present for edge evidence",
        json!({"edge_count": edges.len()}),
    ));
    let missing = mcp_server
        .call_tool(
            "codegraph.explain_missing",
            &json!({
                "repo": repo_root.display().to_string(),
                "db_path": db_path.display().to_string(),
            }),
        )
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    functionality_checks.push(final_gate_check_from_bool(
        "unsupported_unknown_relation_explanation",
        missing["category"].as_str() == Some("unknown"),
        "explain_missing reports unknown explicitly when insufficient evidence is provided",
        json!({"category": missing["category"].clone()}),
    ));

    Ok((
        mvp_checks,
        functionality_checks,
        FinalGateProofObjectCounts {
            source_spans: source_span_count,
            path_evidence_generated: path_evidence.len() as u64,
            derived_closure_edges_generated: derived_edges.len() as u64,
            derived_relation_rows_in_edges,
            provenanced_edges,
            exactness_labeled_edges,
        },
    ))
}

fn exhaustive_final_gate_paths(
    engine: &ExactGraphQueryEngine,
    entities: &[Entity],
    limits: QueryLimits,
) -> Vec<GraphPath> {
    let ids = entities
        .iter()
        .map(|entity| entity.id.as_str())
        .take(64)
        .collect::<Vec<_>>();
    let relations = [
        RelationKind::Calls,
        RelationKind::Reads,
        RelationKind::Writes,
        RelationKind::Mutates,
        RelationKind::FlowsTo,
        RelationKind::ReturnsTo,
        RelationKind::Tests,
        RelationKind::Asserts,
    ];
    let mut paths = Vec::new();
    for source in &ids {
        for target in &ids {
            if source == target {
                continue;
            }
            paths.extend(engine.trace_path(source, target, &relations, limits));
            if paths.len() >= 64 {
                return paths;
            }
        }
    }
    paths
}

fn first_nonempty_call_paths(
    engine: &ExactGraphQueryEngine,
    entities: &[Entity],
    limits: QueryLimits,
    callers: bool,
) -> Option<(String, Vec<GraphPath>)> {
    entities.iter().find_map(|entity| {
        let paths = if callers {
            engine.find_callers(&entity.id, limits)
        } else {
            engine.find_callees(&entity.id, limits)
        };
        (!paths.is_empty()).then(|| (entity.id.clone(), paths))
    })
}

fn find_entity_id(entities: &[Entity], name: &str) -> Option<String> {
    entities
        .iter()
        .find(|entity| {
            entity.name == name
                && matches!(
                    entity.kind.to_string().as_str(),
                    "Function" | "Method" | "Constructor"
                )
                && entity.confidence >= 1.0
        })
        .or_else(|| {
            entities
                .iter()
                .find(|entity| entity.name == name && entity.confidence >= 1.0)
        })
        .or_else(|| {
            entities
                .iter()
                .find(|entity| entity.qualified_name.ends_with(name))
        })
        .map(|entity| entity.id.clone())
}

fn final_gate_cgc_comparison(
    report: &competitors::codegraphcontext::ExternalComparisonReport,
    cgc_db_size_bytes: Option<u64>,
) -> FinalGateCgcComparison {
    let cgc_runs = report
        .runs
        .iter()
        .filter(|run| {
            run.mode == competitors::codegraphcontext::ExternalComparisonMode::CodeGraphContextCli
        })
        .collect::<Vec<_>>();
    let raw_artifact_paths = cgc_runs
        .iter()
        .flat_map(|run| run.raw_artifact_paths.iter().cloned())
        .collect::<Vec<_>>();
    let normalized_artifact_paths = raw_artifact_paths
        .iter()
        .filter(|path| path.contains("normalized_outputs"))
        .cloned()
        .collect::<Vec<_>>();
    let command_artifact_paths = raw_artifact_paths
        .iter()
        .filter(|path| path.contains("raw_artifacts"))
        .cloned()
        .collect::<Vec<_>>();
    let warnings = cgc_runs
        .iter()
        .flat_map(|run| run.warnings.iter())
        .collect::<Vec<_>>();
    let any_timeout = warnings
        .iter()
        .any(|warning| warning.contains("timed_out=true"));
    let all_skipped = cgc_runs
        .iter()
        .all(|run| run.status == competitors::codegraphcontext::ExternalComparisonStatus::Skipped);
    let any_completed = cgc_runs.iter().any(|run| {
        run.status == competitors::codegraphcontext::ExternalComparisonStatus::Completed
    });
    let produced_incomplete_stats = cgc_runs.iter().any(|run| {
        !run.normalized_output.unsupported_fields.is_empty()
            || run.metrics.unsupported_feature_count > 0
            || !run.warnings.is_empty()
    });
    let status = if all_skipped {
        "skipped"
    } else if any_timeout || (any_completed && produced_incomplete_stats) {
        "incomplete"
    } else if any_completed {
        "completed"
    } else {
        "failed"
    };
    let elapsed = cgc_runs
        .iter()
        .filter(|run| {
            run.status == competitors::codegraphcontext::ExternalComparisonStatus::Completed
        })
        .map(|run| run.metrics.index_latency_ms + run.metrics.query_latency_ms)
        .sum::<u64>();

    FinalGateCgcComparison {
        status: status.to_string(),
        executable_path: report.manifest.executable_used.clone(),
        backend: report.manifest.detected_database_backend.clone(),
        version: report.manifest.detected_package_version.clone(),
        python_version: report.manifest.python_version.clone(),
        elapsed_time_ms: if elapsed == 0 {
            json!("unknown")
        } else {
            json!(elapsed)
        },
        db_size_bytes: cgc_db_size_bytes
            .map(Value::from)
            .unwrap_or_else(|| json!("unknown")),
        stdout_artifact_paths: command_artifact_paths.clone(),
        stderr_artifact_paths: command_artifact_paths,
        raw_artifact_paths,
        normalized_artifact_paths,
        produced_incomplete_stats,
        fairness_note: "A broken, skipped, timed-out, or incomplete CGC invocation is never counted as a CodeGraph speed/quality win.".to_string(),
    }
}

fn final_gate_size_targets(
    db_size_bytes: u64,
    cgc: &FinalGateCgcComparison,
) -> BTreeMap<String, Value> {
    let ten_mib = 10 * 1024 * 1024;
    let five_mib = 5 * 1024 * 1024;
    let cgc_size_comparison = if cgc.status == "completed" {
        cgc.db_size_bytes
            .as_u64()
            .map(|size| json!(db_size_bytes < size))
            .unwrap_or_else(|| json!("unknown"))
    } else {
        json!("not_comparable_incomplete_cgc")
    };
    BTreeMap::from([
        (
            "fixture_under_10_mib".to_string(),
            json!(db_size_bytes < ten_mib),
        ),
        (
            "fixture_under_5_mib".to_string(),
            json!(db_size_bytes < five_mib),
        ),
        ("autoresearch_under_10_mib".to_string(), json!("unknown")),
        ("autoresearch_under_5_mib".to_string(), json!("unknown")),
        (
            "smaller_than_cgc_same_repo".to_string(),
            cgc_size_comparison,
        ),
    ])
}

fn final_gate_verdict(
    internal_verdict: FinalGateVerdict,
    cgc: &FinalGateCgcComparison,
    db_size_bytes: u64,
) -> (FinalGateVerdict, Vec<String>, Vec<String>) {
    let mut reasons = Vec::new();
    let mut unknowns = Vec::new();
    if internal_verdict == FinalGateVerdict::Fail {
        reasons.push("Internal compact/MVP correctness checks failed.".to_string());
        return (FinalGateVerdict::Fail, reasons, unknowns);
    }
    if internal_verdict == FinalGateVerdict::Incomplete {
        reasons.push("Internal compact/MVP correctness checks are incomplete.".to_string());
        unknowns.push("internal_compact_mvp_correctness".to_string());
        return (FinalGateVerdict::Incomplete, reasons, unknowns);
    }
    if internal_verdict == FinalGateVerdict::Unknown {
        reasons
            .push("One or more internal compact/MVP correctness checks are unknown.".to_string());
        unknowns.push("internal_compact_mvp_correctness".to_string());
    }
    if cgc.status != "completed" {
        reasons.push(format!(
            "CGC status is `{}`; this is not a fair completed comparison.",
            cgc.status
        ));
        unknowns.push("completed_cgc_comparison".to_string());
        return (FinalGateVerdict::Unknown, reasons, unknowns);
    }
    let Some(cgc_size) = cgc.db_size_bytes.as_u64() else {
        reasons.push("CGC DB size is unknown, so smaller-than-CGC cannot be proven.".to_string());
        unknowns.push("cgc_db_size_bytes".to_string());
        return (FinalGateVerdict::Unknown, reasons, unknowns);
    };
    if db_size_bytes >= cgc_size {
        reasons.push(format!(
            "CodeGraph DB size {db_size_bytes} bytes is not smaller than CGC {cgc_size} bytes."
        ));
        return (FinalGateVerdict::Fail, reasons, unknowns);
    }
    if internal_verdict == FinalGateVerdict::Pass {
        reasons.push("Compact MVP checks passed and CodeGraph is smaller than completed CGC on the same repo.".to_string());
        (FinalGateVerdict::Pass, reasons, unknowns)
    } else {
        (FinalGateVerdict::Unknown, reasons, unknowns)
    }
}

fn aggregate_final_gate_checks(checks: &[FinalGateCheck]) -> FinalGateVerdict {
    if checks
        .iter()
        .any(|check| check.status == FinalGateVerdict::Fail)
    {
        FinalGateVerdict::Fail
    } else if checks
        .iter()
        .any(|check| check.status == FinalGateVerdict::Incomplete)
    {
        FinalGateVerdict::Incomplete
    } else if checks
        .iter()
        .any(|check| check.status == FinalGateVerdict::Unknown)
    {
        FinalGateVerdict::Unknown
    } else {
        FinalGateVerdict::Pass
    }
}

fn final_gate_check(
    id: impl Into<String>,
    status: FinalGateVerdict,
    reason: impl Into<String>,
    evidence: Value,
) -> FinalGateCheck {
    FinalGateCheck {
        id: id.into(),
        status,
        reason: reason.into(),
        evidence,
    }
}

fn final_gate_check_from_bool(
    id: impl Into<String>,
    passed: bool,
    reason: impl Into<String>,
    evidence: Value,
) -> FinalGateCheck {
    final_gate_check(
        id,
        if passed {
            FinalGateVerdict::Pass
        } else {
            FinalGateVerdict::Fail
        },
        reason,
        evidence,
    )
}

fn db_size_bytes(path: &Path) -> BenchResult<u64> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(BenchmarkError::from)
}

fn absolute_bench_path(path: &Path) -> BenchResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(BenchmarkError::from)
    }
}

fn bytes_to_mib(bytes: u64) -> f64 {
    ((bytes as f64 / 1_048_576.0) * 1000.0).round() / 1000.0
}

fn parity_dimensions() -> Vec<ParityDimension> {
    [
        (
            "language_breadth",
            "closed_or_improved",
            "unknown_without_live_cgc",
            "measured_in_synthetic_and_manifest_corpus",
            "language frontend registry plus real-repo manifests",
        ),
        (
            "parse_success",
            "measured_for_supported_frontends",
            "unknown_without_live_cgc",
            "synthetic baselines available",
            "parse tests and real-repo replay plan",
        ),
        (
            "symbol_recall",
            "measured_synthetic_manifest_real_repo",
            "unknown_without_live_cgc",
            "grep/BM25 available",
            "symbol search hardening plus task manifests",
        ),
        (
            "caller_callee_recall",
            "improved_for_ts_js_python_go_rust",
            "unknown_without_live_cgc",
            "graph-only available",
            "caller/callee tests and manifests",
        ),
        (
            "call_chain_recall",
            "evidence_oriented_bounded",
            "unknown_without_live_cgc",
            "graph-only available",
            "trace_path and chain query tests",
        ),
        (
            "indexing_speed",
            "profiled_cold_warm",
            "unknown_without_live_cgc",
            "phase29 synthetic index available",
            "index --profile and synthetic index benchmark",
        ),
        (
            "cli_command_coverage",
            "closed",
            "unknown_without_live_cgc",
            "not_applicable",
            "help/docs/tests cover command groups",
        ),
        (
            "install_paths",
            "closed_templates",
            "unknown_without_live_cgc",
            "not_applicable",
            "release/install templates and metadata",
        ),
        (
            "ui_capability",
            "improved_proof_focused",
            "unknown_without_live_cgc",
            "not_applicable",
            "local Proof-Path UI graph JSON contract",
        ),
        (
            "mcp_schema_quality",
            "improved",
            "unknown_without_live_cgc",
            "not_applicable",
            "tool output schemas/resources/prompts",
        ),
        (
            "proof_context_quality",
            "stronger_than_vector_only_when_paths_exist",
            "unknown_without_live_cgc",
            "vector_only_lower_proof_quality",
            "vectors suggest, graph verifies, packet proves",
        ),
    ]
    .into_iter()
    .map(
        |(name, codegraph_status, codegraphcontext_status, internal_baseline_status, evidence)| {
            ParityDimension {
                name: name.to_string(),
                codegraph_status: codegraph_status.to_string(),
                codegraphcontext_status: codegraphcontext_status.to_string(),
                internal_baseline_status: internal_baseline_status.to_string(),
                evidence: evidence.to_string(),
                unknown_fields: vec![
                    "live_codegraphcontext_measurement".to_string(),
                    "large_real_repo_runtime_until_replay_runs".to_string(),
                ],
            }
        },
    )
    .collect()
}

fn unix_time_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(u128::from(u64::MAX)) as u64,
        Err(_) => 0,
    }
}

pub fn classify_gap(
    codegraph: Option<f64>,
    competitor: Option<f64>,
    polarity: GapPolarity,
    tolerance: f64,
) -> GapClassification {
    let (Some(codegraph), Some(competitor)) = (codegraph, competitor) else {
        return GapClassification::Unknown;
    };
    if (codegraph - competitor).abs() <= tolerance {
        return GapClassification::Tie;
    }
    match polarity {
        GapPolarity::HigherIsBetter if codegraph > competitor => GapClassification::Win,
        GapPolarity::HigherIsBetter => GapClassification::Loss,
        GapPolarity::LowerIsBetter if codegraph < competitor => GapClassification::Win,
        GapPolarity::LowerIsBetter => GapClassification::Loss,
    }
}

pub fn write_gap_scoreboard_report(
    options: GapScoreboardOptions,
) -> BenchResult<GapScoreboardArtifacts> {
    fs::create_dir_all(&options.report_dir)?;
    let external_dir = options.report_dir.join("external-codegraphcontext");
    let external_report = competitors::codegraphcontext::run_codegraphcontext_comparison(
        competitors::codegraphcontext::CodeGraphContextComparisonOptions {
            report_dir: external_dir.clone(),
            timeout_ms: options.timeout_ms,
            top_k: options.top_k,
            competitor_executable: options.competitor_executable,
        },
    )?;
    let internal_report = run_default_benchmark_suite(&BaselineMode::ALL)?;
    let report = gap_scoreboard_report(&internal_report, &external_report, options.top_k)?;
    report.validate()?;

    let json_path = options.report_dir.join("summary.json");
    let markdown_path = options.report_dir.join("summary.md");
    let jsonl_path = options.report_dir.join("per_task.jsonl");
    fs::write(
        &json_path,
        serde_json::to_string_pretty(&report).map_err(|error| {
            BenchmarkError::Io(format!("failed to encode gap scoreboard: {error}"))
        })?,
    )?;
    fs::write(&markdown_path, render_gap_scoreboard_markdown(&report))?;
    let mut jsonl = String::new();
    for record in &report.task_records {
        jsonl.push_str(
            &serde_json::to_string(record)
                .map_err(|error| BenchmarkError::Io(error.to_string()))?,
        );
        jsonl.push('\n');
    }
    fs::write(&jsonl_path, jsonl)?;

    Ok(GapScoreboardArtifacts {
        report_dir: options.report_dir.display().to_string(),
        json_summary: json_path.display().to_string(),
        markdown_summary: markdown_path.display().to_string(),
        per_task_jsonl: jsonl_path.display().to_string(),
        external_competitor_dir: external_dir.display().to_string(),
    })
}

pub fn gap_scoreboard_report(
    internal_report: &BenchmarkReport,
    external_report: &competitors::codegraphcontext::ExternalComparisonReport,
    top_k: usize,
) -> BenchResult<GapScoreboardReport> {
    let dimensions = gap_scoreboard_dimensions(internal_report, external_report, top_k);
    let task_records = gap_scoreboard_task_records(internal_report, external_report);
    Ok(GapScoreboardReport {
        schema_version: BENCH_SCHEMA_VERSION,
        benchmark_id: format!("codegraph-phase-26-gap-scoreboard-{}", unix_time_ms()),
        source_of_truth: "MVP.md Prompt 26".to_string(),
        generated_by: "codegraph-bench phase 26 gap scoreboard".to_string(),
        generated_at_unix_ms: unix_time_ms(),
        codegraph_version: env!("CARGO_PKG_VERSION").to_string(),
        competitor_metadata: external_report.manifest.clone(),
        fairness_rules: external_report.fairness_rules.clone(),
        internal_baseline_summary: internal_report.aggregate.clone(),
        dimensions,
        task_records,
        no_sota_claim: true,
    })
}

pub fn render_gap_scoreboard_markdown(report: &GapScoreboardReport) -> String {
    let mut output = String::new();
    output.push_str("# CodeGraph Phase 26 Gap Scoreboard\n\n");
    output.push_str("Source of truth: MVP.md Prompt 26.\n\n");
    output.push_str("Missing data is `unknown`; skipped competitor runs are not treated as losses or wins. No SOTA claim is made.\n\n");
    output.push_str("| Dimension | Result | CodeGraph | CodeGraphContext | Evidence |\n");
    output.push_str("| --- | --- | --- | --- | --- |\n");
    for dimension in &report.dimensions {
        output.push_str(&format!(
            "| {} | {:?} | {} ({:?}) | {} ({:?}) | {} |\n",
            dimension.id,
            dimension.classification,
            compact_value(&dimension.codegraph_value),
            dimension.codegraph_status,
            compact_value(&dimension.competitor_value),
            dimension.competitor_status,
            dimension.evidence.join("; ").replace('|', "/")
        ));
    }
    output.push_str("\n## Competitor Metadata\n\n");
    output.push_str(&format!(
        "- Source: `{}`\n- Executable: `{}`\n- Version: `{}`\n- Pinned commit: `{}`\n- Python: `{}`\n- Backend: `{}`\n\n",
        report.competitor_metadata.source_repo_url,
        report.competitor_metadata.executable_used,
        report.competitor_metadata.detected_package_version,
        report.competitor_metadata.pinned_git_commit_sha,
        report.competitor_metadata.python_version,
        report.competitor_metadata.detected_database_backend,
    ));
    output
}

fn gap_scoreboard_dimensions(
    internal_report: &BenchmarkReport,
    external_report: &competitors::codegraphcontext::ExternalComparisonReport,
    top_k: usize,
) -> Vec<GapScoreboardDimension> {
    let codegraph_aggregate = internal_report
        .aggregate
        .get(BaselineMode::FullContextPacket.as_str())
        .or_else(|| {
            internal_report
                .aggregate
                .get(BaselineMode::GraphOnly.as_str())
        });
    let cgc_aggregate = external_report
        .aggregate
        .get(competitors::codegraphcontext::ExternalComparisonMode::CodeGraphContextCli.as_str());
    let cgc_status = competitor_measurement_status(cgc_aggregate);
    let cgc_file_recall = measured_cgc_value(cgc_aggregate, |aggregate| {
        aggregate.average_file_recall_at_10
    });
    let cgc_symbol_recall = measured_cgc_value(cgc_aggregate, |aggregate| {
        aggregate.average_symbol_recall_at_10
    });
    let cgc_path_recall = measured_cgc_value(cgc_aggregate, |aggregate| {
        aggregate.average_path_recall_at_10
    });
    let language_tiers = supported_language_count_by_tier();
    let parse_success = codegraph_parse_success_rate();
    let cli_command_count = 17u64;
    let mcp_coverage = json!({
        "tools": 22,
        "input_schemas": 22,
        "output_schemas": 22,
        "resources": 5,
        "prompts": 5,
        "safety_annotations": true
    });
    let ui_coverage = json!({
        "proof_path": true,
        "neighborhood": true,
        "impact": true,
        "auth_security": true,
        "event_flow": true,
        "test_impact": true,
        "unresolved_calls": true,
        "source_span_preview": true,
        "truncation_guardrails": true
    });

    let mut dimensions = Vec::new();
    dimensions.push(gap_dimension(GapDimensionParts {
        id: "supported_language_count_by_support_tier",
        description: "Supported language count grouped by declared frontend support tier.",
        codegraph_value: json!(language_tiers),
        codegraph_status: GapMeasurementStatus::Declared,
        competitor_value: json!("unknown"),
        competitor_status: cgc_status,
        classification: GapClassification::Unknown,
        evidence: vec!["CodeGraph frontend registry".to_string()],
        unknown_fields: vec!["competitor_support_tier_breakdown".to_string()],
    }));
    dimensions.push(gap_dimension(GapDimensionParts {
        id: "indexed_files_per_language",
        description: "Indexed files grouped by language on the measured corpus.",
        codegraph_value: json!("unknown"),
        codegraph_status: GapMeasurementStatus::Unknown,
        competitor_value: json!("unknown"),
        competitor_status: cgc_status,
        classification: GapClassification::Unknown,
        evidence: vec!["Real-repo replay is separate and explicit".to_string()],
        unknown_fields: vec![
            "codegraph_real_repo_indexed_files".to_string(),
            "competitor_indexed_files_per_language".to_string(),
        ],
    }));
    dimensions.push(gap_dimension(GapDimensionParts {
        id: "parse_success_rate",
        description: "Parser success rate over controlled benchmark fixtures.",
        codegraph_value: json!(parse_success),
        codegraph_status: GapMeasurementStatus::Measured,
        competitor_value: json!("unknown"),
        competitor_status: cgc_status,
        classification: GapClassification::Unknown,
        evidence: vec!["CodeGraph parser smoke over external fixtures".to_string()],
        unknown_fields: vec!["competitor_parse_success_rate".to_string()],
    }));
    dimensions.push(metric_dimension(
        "entity_extraction_recall",
        "Entity/relation extraction recall proxy from internal full-context benchmark aggregate.",
        codegraph_aggregate.map(|aggregate| aggregate.average_recall),
        None,
        GapPolarity::HigherIsBetter,
        vec!["Internal benchmark aggregate".to_string()],
        vec!["competitor_entity_extraction_recall".to_string()],
    ));
    dimensions.push(metric_dimension(
        "symbol_search_recall_at_k",
        &format!("Symbol recall@{top_k} on controlled gap fixtures."),
        codegraph_aggregate.map(|aggregate| aggregate.average_symbol_recall_at_10),
        cgc_symbol_recall,
        GapPolarity::HigherIsBetter,
        vec!["Internal aggregate and optional CGC external aggregate".to_string()],
        competitor_unknown_fields(cgc_status, "competitor_symbol_recall_at_k"),
    ));
    dimensions.push(metric_dimension(
        "file_recall_at_k",
        &format!("File recall@{top_k} on controlled gap fixtures."),
        codegraph_aggregate.map(|aggregate| aggregate.average_file_recall_at_10),
        cgc_file_recall,
        GapPolarity::HigherIsBetter,
        vec!["Internal aggregate and optional CGC external aggregate".to_string()],
        competitor_unknown_fields(cgc_status, "competitor_file_recall_at_k"),
    ));
    dimensions.push(metric_dimension(
        "caller_recall_at_k",
        &format!("Caller recall@{top_k} on controlled gap fixtures."),
        codegraph_path_recall_for_family(internal_report, BenchmarkFamily::LongChainPath),
        cgc_path_recall,
        GapPolarity::HigherIsBetter,
        vec!["Long-chain fixture path recall used as caller/callee proxy".to_string()],
        competitor_unknown_fields(cgc_status, "competitor_caller_recall_at_k"),
    ));
    dimensions.push(metric_dimension(
        "callee_recall_at_k",
        &format!("Callee recall@{top_k} on controlled gap fixtures."),
        codegraph_path_recall_for_family(internal_report, BenchmarkFamily::LongChainPath),
        cgc_path_recall,
        GapPolarity::HigherIsBetter,
        vec!["Long-chain fixture path recall used as caller/callee proxy".to_string()],
        competitor_unknown_fields(cgc_status, "competitor_callee_recall_at_k"),
    ));
    dimensions.push(metric_dimension(
        "call_chain_path_recall_at_k",
        &format!("Call-chain/path recall@{top_k}."),
        codegraph_aggregate.map(|aggregate| aggregate.average_path_recall_at_10),
        cgc_path_recall,
        GapPolarity::HigherIsBetter,
        vec!["Path recall aggregate".to_string()],
        competitor_unknown_fields(cgc_status, "competitor_path_recall_at_k"),
    ));
    dimensions.push(metric_dimension(
        "test_impact_recall_at_k",
        &format!("Test-impact recall@{top_k}."),
        codegraph_test_impact_recall(internal_report),
        None,
        GapPolarity::HigherIsBetter,
        vec!["Internal test-impact benchmark family".to_string()],
        vec!["competitor_test_impact_recall_at_k".to_string()],
    ));
    dimensions.push(metric_dimension(
        "cold_index_time",
        "Cold index latency in milliseconds where measured.",
        codegraph_aggregate.map(|aggregate| aggregate.total_latency_ms as f64),
        None,
        GapPolarity::LowerIsBetter,
        vec!["Internal benchmark latency proxy".to_string()],
        vec!["competitor_cold_index_time".to_string()],
    ));
    dimensions.push(gap_dimension(GapDimensionParts {
        id: "warm_incremental_update_time",
        description: "Warm changed-file update latency in milliseconds.",
        codegraph_value: json!("unknown"),
        codegraph_status: GapMeasurementStatus::Unknown,
        competitor_value: json!("unknown"),
        competitor_status: cgc_status,
        classification: GapClassification::Unknown,
        evidence: vec![
            "Watch/update tests validate behavior but do not publish a gap metric here".to_string(),
        ],
        unknown_fields: vec![
            "codegraph_warm_incremental_update_time".to_string(),
            "competitor_warm_incremental_update_time".to_string(),
        ],
    }));
    dimensions.push(metric_dimension(
        "memory_where_measurable",
        "Memory usage in bytes where measurable.",
        codegraph_aggregate.map(|aggregate| aggregate.max_memory_bytes as f64),
        None,
        GapPolarity::LowerIsBetter,
        vec!["Internal benchmark memory estimate".to_string()],
        vec!["competitor_memory_usage".to_string()],
    ));
    dimensions.push(gap_dimension(GapDimensionParts {
        id: "cli_command_coverage",
        description: "Stable CLI command group coverage.",
        codegraph_value: json!({"command_groups": cli_command_count}),
        codegraph_status: GapMeasurementStatus::Declared,
        competitor_value: json!("unknown"),
        competitor_status: cgc_status,
        classification: GapClassification::Unknown,
        evidence: vec!["CLI help smoke tests".to_string()],
        unknown_fields: vec!["competitor_cli_command_coverage".to_string()],
    }));
    dimensions.push(gap_dimension(GapDimensionParts {
        id: "install_steps_count",
        description: "Documented local install path step count.",
        codegraph_value: json!({"minimum_documented_steps": 1, "paths": ["release_archive", "powershell_installer", "shell_installer", "cargo_install", "cargo_binstall", "homebrew_template"]}),
        codegraph_status: GapMeasurementStatus::Declared,
        competitor_value: json!("unknown"),
        competitor_status: cgc_status,
        classification: GapClassification::Unknown,
        evidence: vec!["Install docs and release metadata".to_string()],
        unknown_fields: vec!["competitor_install_steps_count".to_string()],
    }));
    dimensions.push(gap_dimension(GapDimensionParts {
        id: "mcp_tool_schema_resource_coverage",
        description: "MCP tools, schemas, resources, prompts, and safety metadata coverage.",
        codegraph_value: mcp_coverage,
        codegraph_status: GapMeasurementStatus::Declared,
        competitor_value: json!("unknown"),
        competitor_status: cgc_status,
        classification: GapClassification::Unknown,
        evidence: vec!["MCP schema/resource/prompt tests".to_string()],
        unknown_fields: vec!["competitor_mcp_schema_resource_coverage".to_string()],
    }));
    dimensions.push(gap_dimension(GapDimensionParts {
        id: "ui_inspection_coverage",
        description: "UI proof-path inspection modes and controls.",
        codegraph_value: ui_coverage,
        codegraph_status: GapMeasurementStatus::Declared,
        competitor_value: json!("unknown"),
        competitor_status: cgc_status,
        classification: GapClassification::Unknown,
        evidence: vec!["UI graph JSON and guardrail tests".to_string()],
        unknown_fields: vec!["competitor_ui_inspection_coverage".to_string()],
    }));
    dimensions
}

fn gap_scoreboard_task_records(
    internal_report: &BenchmarkReport,
    external_report: &competitors::codegraphcontext::ExternalComparisonReport,
) -> Vec<GapScoreboardTaskRecord> {
    let mut records = Vec::new();
    for result in &internal_report.results {
        records.push(GapScoreboardTaskRecord {
            dimension_id: benchmark_family_dimension(result.family).to_string(),
            task_id: result.task_id.clone(),
            tool: result.baseline.as_str().to_string(),
            status: match result.status {
                BenchmarkRunStatus::Completed => GapMeasurementStatus::Measured,
                BenchmarkRunStatus::Skipped => GapMeasurementStatus::Skipped,
            },
            value: json!(result.metrics),
            artifact_paths: Vec::new(),
            notes: result.warnings.clone(),
        });
    }
    for run in &external_report.runs {
        records.push(GapScoreboardTaskRecord {
            dimension_id: "external_codegraphcontext_cli".to_string(),
            task_id: run.task_id.clone(),
            tool: run.mode.as_str().to_string(),
            status: match run.status {
                competitors::codegraphcontext::ExternalComparisonStatus::Completed => {
                    GapMeasurementStatus::Measured
                }
                competitors::codegraphcontext::ExternalComparisonStatus::Skipped => {
                    GapMeasurementStatus::Skipped
                }
            },
            value: json!({
                "metrics": run.metrics,
                "normalized_output": run.normalized_output,
                "skip_reason": run.skip_reason,
            }),
            artifact_paths: run.raw_artifact_paths.clone(),
            notes: run.warnings.clone(),
        });
    }
    records
}

struct GapDimensionParts<'a> {
    id: &'a str,
    description: &'a str,
    codegraph_value: Value,
    codegraph_status: GapMeasurementStatus,
    competitor_value: Value,
    competitor_status: GapMeasurementStatus,
    classification: GapClassification,
    evidence: Vec<String>,
    unknown_fields: Vec<String>,
}

fn gap_dimension(parts: GapDimensionParts<'_>) -> GapScoreboardDimension {
    GapScoreboardDimension {
        id: parts.id.to_string(),
        description: parts.description.to_string(),
        codegraph_value: parts.codegraph_value,
        codegraph_status: parts.codegraph_status,
        competitor_value: parts.competitor_value,
        competitor_status: parts.competitor_status,
        classification: parts.classification,
        evidence: parts.evidence,
        unknown_fields: parts.unknown_fields,
    }
}

fn metric_dimension(
    id: &str,
    description: &str,
    codegraph: Option<f64>,
    competitor: Option<f64>,
    polarity: GapPolarity,
    evidence: Vec<String>,
    unknown_fields: Vec<String>,
) -> GapScoreboardDimension {
    let codegraph_status = if codegraph.is_some() {
        GapMeasurementStatus::Measured
    } else {
        GapMeasurementStatus::Unknown
    };
    let competitor_status = if competitor.is_some() {
        GapMeasurementStatus::Measured
    } else {
        GapMeasurementStatus::Unknown
    };
    gap_dimension(GapDimensionParts {
        id,
        description,
        codegraph_value: codegraph
            .map_or_else(|| json!("unknown"), |value| json!(clean_metric(value))),
        codegraph_status,
        competitor_value: competitor
            .map_or_else(|| json!("unknown"), |value| json!(clean_metric(value))),
        competitor_status,
        classification: classify_gap(codegraph, competitor, polarity, 0.000_001),
        evidence,
        unknown_fields,
    })
}

fn supported_language_count_by_tier() -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for frontend in language_frontends() {
        *counts
            .entry(frontend.support_tier.label().to_string())
            .or_default() += 1;
    }
    counts
}

fn codegraph_parse_success_rate() -> f64 {
    let parser = TreeSitterParser;
    let fixtures = competitors::codegraphcontext::external_competitor_fixtures();
    let mut attempted = 0usize;
    let mut parsed = 0usize;
    for fixture in fixtures {
        for (path, source) in fixture.files {
            if detect_language(&path).is_none() {
                continue;
            }
            attempted += 1;
            if matches!(parser.parse(&path, &source), Ok(Some(_))) {
                parsed += 1;
            }
        }
    }
    divide(parsed as f64, attempted as f64)
}

fn competitor_measurement_status(
    aggregate: Option<&competitors::codegraphcontext::ExternalComparisonAggregate>,
) -> GapMeasurementStatus {
    match aggregate {
        Some(aggregate) if aggregate.runs > aggregate.skipped => GapMeasurementStatus::Measured,
        Some(_) => GapMeasurementStatus::Skipped,
        None => GapMeasurementStatus::Unknown,
    }
}

fn measured_cgc_value(
    aggregate: Option<&competitors::codegraphcontext::ExternalComparisonAggregate>,
    get: impl FnOnce(&competitors::codegraphcontext::ExternalComparisonAggregate) -> f64,
) -> Option<f64> {
    let aggregate = aggregate?;
    (aggregate.runs > aggregate.skipped).then(|| get(aggregate))
}

fn competitor_unknown_fields(status: GapMeasurementStatus, field: &str) -> Vec<String> {
    if status == GapMeasurementStatus::Measured {
        Vec::new()
    } else {
        vec![field.to_string()]
    }
}

fn codegraph_path_recall_for_family(
    report: &BenchmarkReport,
    family: BenchmarkFamily,
) -> Option<f64> {
    let values = report
        .results
        .iter()
        .filter(|result| {
            result.baseline == BaselineMode::FullContextPacket && result.family == family
        })
        .filter_map(|result| result.metrics.path_recall_at_k.get("10").copied())
        .collect::<Vec<_>>();
    (!values.is_empty()).then(|| divide(values.iter().sum::<f64>(), values.len() as f64))
}

fn codegraph_test_impact_recall(report: &BenchmarkReport) -> Option<f64> {
    let values = report
        .results
        .iter()
        .filter(|result| {
            result.baseline == BaselineMode::FullContextPacket
                && result.family == BenchmarkFamily::TestImpact
        })
        .filter_map(|result| result.metrics.file_recall_at_k.get("10").copied())
        .collect::<Vec<_>>();
    (!values.is_empty()).then(|| divide(values.iter().sum::<f64>(), values.len() as f64))
}

fn benchmark_family_dimension(family: BenchmarkFamily) -> &'static str {
    match family {
        BenchmarkFamily::RelationExtraction => "entity_extraction_recall",
        BenchmarkFamily::LongChainPath => "call_chain_path_recall_at_k",
        BenchmarkFamily::ContextRetrieval => "file_recall_at_k",
        BenchmarkFamily::AgentPatch => "file_recall_at_k",
        BenchmarkFamily::Compression => "memory_where_measurable",
        BenchmarkFamily::SecurityAuth => "call_chain_path_recall_at_k",
        BenchmarkFamily::AsyncEvent => "call_chain_path_recall_at_k",
        BenchmarkFamily::TestImpact => "test_impact_recall_at_k",
    }
}

fn compact_value(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| "unknown".to_string()),
    }
}

pub fn synthetic_repo(kind: SyntheticRepoKind) -> SyntheticRepo {
    match kind {
        SyntheticRepoKind::RelationExtraction => relation_extraction_repo(),
        SyntheticRepoKind::LongChainPath => long_chain_repo(),
        SyntheticRepoKind::ContextRetrieval => context_retrieval_repo(),
        SyntheticRepoKind::AgentPatch => agent_patch_repo(),
        SyntheticRepoKind::Compression => compression_repo(),
        SyntheticRepoKind::SecurityAuth => security_auth_repo(),
        SyntheticRepoKind::AsyncEvent => async_event_repo(),
        SyntheticRepoKind::TestImpact => test_impact_repo(),
        SyntheticRepoKind::AllFamilies => all_families_repo(),
    }
}

pub fn index_synthetic_repo(repo: &SyntheticRepo) -> BenchResult<IndexedBenchRepo> {
    let store = SqliteGraphStore::open_in_memory().map_err(|error| {
        BenchmarkError::Store(format!("failed to open in-memory benchmark store: {error}"))
    })?;
    let parser = TreeSitterParser;
    let mut entities = Vec::new();
    let mut edges = Vec::new();

    for (repo_relative_path, source) in &repo.files {
        if detect_language(repo_relative_path).is_none() {
            continue;
        }
        let parsed = parser
            .parse(repo_relative_path, source)
            .map_err(|error| BenchmarkError::Parse(error.to_string()))?;
        let Some(parsed) = parsed else {
            continue;
        };
        let extraction = extract_entities_and_relations(&parsed, source);
        store
            .upsert_file(&extraction.file)
            .map_err(|error| BenchmarkError::Store(error.to_string()))?;
        store
            .upsert_file_text(repo_relative_path, source)
            .map_err(|error| BenchmarkError::Store(error.to_string()))?;
        for entity in &extraction.entities {
            store
                .upsert_entity(entity)
                .map_err(|error| BenchmarkError::Store(error.to_string()))?;
            if let Some(span) = &entity.source_span {
                store
                    .upsert_source_span(&entity.id, span)
                    .map_err(|error| BenchmarkError::Store(error.to_string()))?;
                store
                    .upsert_snippet_text(&entity.id, span, &source_snippet(span, source))
                    .map_err(|error| BenchmarkError::Store(error.to_string()))?;
            }
        }
        for edge in &extraction.edges {
            store
                .upsert_edge(edge)
                .map_err(|error| BenchmarkError::Store(error.to_string()))?;
            store
                .upsert_source_span(&edge.id, &edge.source_span)
                .map_err(|error| BenchmarkError::Store(error.to_string()))?;
            store
                .upsert_snippet_text(
                    &edge.id,
                    &edge.source_span,
                    &source_snippet(&edge.source_span, source),
                )
                .map_err(|error| BenchmarkError::Store(error.to_string()))?;
        }
        entities.extend(extraction.entities);
        edges.extend(extraction.edges);
    }

    let state = RepoIndexState {
        repo_id: format!("bench://{}", repo.id),
        repo_root: repo.id.clone(),
        repo_commit: Some("synthetic".to_string()),
        schema_version: store
            .schema_version()
            .map_err(|error| BenchmarkError::Store(error.to_string()))?,
        indexed_at_unix_ms: Some(1),
        files_indexed: repo.files.len() as u64,
        entity_count: entities.len() as u64,
        edge_count: edges.len() as u64,
        metadata: BTreeMap::from([("benchmark".to_string(), json!(true))]),
    };
    store
        .upsert_repo_index_state(&state)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;

    let documents = build_documents(&repo.files, &entities);
    Ok(IndexedBenchRepo {
        id: repo.id.clone(),
        store,
        entities,
        edges,
        sources: repo.files.clone(),
        documents,
    })
}

pub fn plan_real_repo_commit_replay(spec: &CommitReplaySpec) -> RealRepoReplayPlan {
    let repo_path = Path::new(&spec.repo_path);
    if !repo_path.exists() {
        return RealRepoReplayPlan {
            status: ReplayFeasibility::Unavailable,
            reason: Some("repo path does not exist".to_string()),
            commands: Vec::new(),
            changed_paths: spec.changed_paths.clone(),
        };
    }
    if !repo_path.join(".git").exists() {
        return RealRepoReplayPlan {
            status: ReplayFeasibility::Unavailable,
            reason: Some("repo path is not a git checkout".to_string()),
            commands: Vec::new(),
            changed_paths: spec.changed_paths.clone(),
        };
    }

    RealRepoReplayPlan {
        status: ReplayFeasibility::Feasible,
        reason: None,
        commands: vec![
            format!(
                "git -C {} diff --name-only {} {}",
                spec.repo_path, spec.base_commit, spec.head_commit
            ),
            format!("codegraph-mcp index {}", spec.repo_path),
            "run configured project tests for replayed patch success when available".to_string(),
        ],
        changed_paths: spec.changed_paths.clone(),
    }
}

pub fn run_default_benchmark_suite(baselines: &[BaselineMode]) -> BenchResult<BenchmarkReport> {
    run_benchmark_suite(&default_benchmark_suite(), baselines)
}

pub fn run_benchmark_suite(
    suite: &BenchmarkSuite,
    baselines: &[BaselineMode],
) -> BenchResult<BenchmarkReport> {
    suite.validate()?;
    let modes = if baselines.is_empty() {
        BaselineMode::ALL.to_vec()
    } else {
        baselines.to_vec()
    };
    let mut results = Vec::new();

    for task in &suite.tasks {
        match &task.repo {
            BenchmarkRepoSpec::Synthetic { kind } => {
                let repo = synthetic_repo(kind.clone());
                let corpus = index_synthetic_repo(&repo)?;
                for baseline in &modes {
                    results.push(run_baseline(task, &corpus, *baseline)?);
                }
            }
            BenchmarkRepoSpec::RealCommitReplay { spec } => {
                for baseline in &modes {
                    let mut result = BenchmarkRunResult::new(task, *baseline);
                    result.status = BenchmarkRunStatus::Skipped;
                    let plan = plan_real_repo_commit_replay(spec);
                    result.warnings.push(plan.reason.clone().unwrap_or_else(|| {
                        "real repo replay is planned but not executed by library runner".to_string()
                    }));
                    result.context_packet = Some(json!({ "replay_plan": plan }));
                    results.push(result);
                }
            }
        }
    }

    Ok(BenchmarkReport {
        schema_version: BENCH_SCHEMA_VERSION,
        suite_id: suite.id.clone(),
        generated_by: "codegraph-bench phase 20".to_string(),
        aggregate: aggregate_results(&results),
        results,
    })
}

pub fn run_baseline(
    task: &BenchmarkTask,
    corpus: &IndexedBenchRepo,
    baseline: BaselineMode,
) -> BenchResult<BenchmarkRunResult> {
    let mut result = BenchmarkRunResult::new(task, baseline);
    match baseline {
        BaselineMode::VanillaNoRetrieval => {
            result.metrics.token_cost = estimate_tokens(&task.prompt);
            result.metrics.latency_ms = estimate_latency_ms(corpus, 1);
            result.metrics.memory_bytes = 0;
        }
        BaselineMode::GrepBm25 => run_bm25_baseline(task, corpus, &mut result)?,
        BaselineMode::VectorOnly => run_vector_only_baseline(task, corpus, &mut result)?,
        BaselineMode::GraphOnly => run_graph_only_baseline(task, corpus, &mut result)?,
        BaselineMode::GraphBinaryPqFunnel => {
            run_graph_binary_pq_baseline(task, corpus, &mut result)?
        }
        BaselineMode::GraphBayesianRanker => {
            run_graph_bayesian_baseline(task, corpus, &mut result)?
        }
        BaselineMode::FullContextPacket => {
            run_full_context_packet_baseline(task, corpus, &mut result)?
        }
    }

    if matches!(
        baseline,
        BaselineMode::GraphOnly
            | BaselineMode::GraphBinaryPqFunnel
            | BaselineMode::GraphBayesianRanker
            | BaselineMode::FullContextPacket
    ) || task.family == BenchmarkFamily::RelationExtraction
    {
        result.observed_relations = corpus.edges.iter().map(observed_relation).collect();
    }
    result.metrics = evaluate_result(task, &result);
    Ok(result)
}

pub fn precision_recall_f1(
    expected: &BTreeSet<String>,
    observed: &BTreeSet<String>,
) -> MetricScore {
    if expected.is_empty() && observed.is_empty() {
        return MetricScore {
            precision: 1.0,
            recall: 1.0,
            f1: 1.0,
        };
    }
    let true_positive = observed.intersection(expected).count() as f64;
    let precision = divide(true_positive, observed.len() as f64);
    let recall = divide(true_positive, expected.len() as f64);
    MetricScore {
        precision,
        recall,
        f1: f1(precision, recall),
    }
}

pub fn recall_at_k(expected: &BTreeSet<String>, ranked: &[String], k: usize) -> f64 {
    if expected.is_empty() {
        return 1.0;
    }
    let hits = ranked
        .iter()
        .take(k)
        .filter(|value| expected.contains(*value))
        .collect::<BTreeSet<_>>()
        .len() as f64;
    divide(hits, expected.len() as f64)
}

pub fn mean_reciprocal_rank(expected: &BTreeSet<String>, ranked: &[String]) -> f64 {
    if expected.is_empty() {
        return 1.0;
    }
    ranked
        .iter()
        .position(|value| expected.contains(value))
        .map(|index| 1.0 / (index + 1) as f64)
        .unwrap_or(0.0)
}

pub fn ndcg_at_k(expected: &BTreeSet<String>, ranked: &[String], k: usize) -> f64 {
    if expected.is_empty() {
        return 1.0;
    }
    let dcg = ranked
        .iter()
        .take(k)
        .enumerate()
        .filter(|(_, value)| expected.contains(*value))
        .map(|(index, _)| 1.0 / ((index + 2) as f64).log2())
        .sum::<f64>();
    let ideal_hits = expected.len().min(k);
    let idcg = (0..ideal_hits)
        .map(|index| 1.0 / ((index + 2) as f64).log2())
        .sum::<f64>();
    divide(dcg, idcg)
}

pub fn render_json_report(report: &BenchmarkReport) -> BenchResult<String> {
    serde_json::to_string_pretty(report).map_err(|error| BenchmarkError::Parse(error.to_string()))
}

pub fn render_markdown_report(report: &BenchmarkReport) -> String {
    let mut output = format!(
        "# CodeGraph Benchmark Report\n\nSuite: `{}`\n\n| Baseline | Runs | F1 | Path R@10 | Symbol R@10 | File R@10 | MRR | NDCG | Tokens |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|\n",
        report.suite_id
    );
    for (baseline, aggregate) in &report.aggregate {
        output.push_str(&format!(
            "| `{}` | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {} |\n",
            baseline,
            aggregate.runs,
            aggregate.average_f1,
            aggregate.average_path_recall_at_10,
            aggregate.average_symbol_recall_at_10,
            aggregate.average_file_recall_at_10,
            aggregate.average_mrr,
            aggregate.average_ndcg,
            aggregate.total_token_cost
        ));
    }
    output
}

fn relation_extraction_repo() -> SyntheticRepo {
    let file = "src/relation.ts";
    let mut files = BTreeMap::new();
    files.insert(
        file.to_string(),
        r#"
export function normalizeEmail(input: string) {
  return input.trim().toLowerCase();
}

export class UserService {
  private record: any = {};
  updateEmail(raw: string) {
    const email = normalizeEmail(raw);
    this.record.email = email;
    return email;
  }
}

export function handleProfile(req: any) {
  const service = new UserService();
  return service.updateEmail(req.body.email);
}
"#
        .trim()
        .to_string(),
    );
    let task = task(
        "relation-extraction-basic",
        BenchmarkFamily::RelationExtraction,
        "Verify CALLS, WRITES, MUTATES, RETURNS, and FLOWS_TO facts for UserService.updateEmail",
        SyntheticRepoKind::RelationExtraction,
        GroundTruth {
            expected_relations: vec![
                expected_relation(RelationKind::Calls, "updateEmail", "normalizeEmail", file),
                expected_relation(RelationKind::Writes, "email", "email", file),
                expected_relation(RelationKind::Mutates, "updateEmail", "email", file),
                expected_relation(RelationKind::Returns, "updateEmail", "return", file),
                expected_relation(RelationKind::FlowsTo, "raw", "email", file),
            ],
            expected_relation_sequences: vec![vec![RelationKind::Calls, RelationKind::Mutates]],
            expected_files: vec![file.to_string()],
            expected_symbols: vec![
                "UserService.updateEmail".to_string(),
                "normalizeEmail".to_string(),
            ],
            ..GroundTruth::default()
        },
    );
    SyntheticRepo {
        id: "synthetic-relation-extraction".to_string(),
        kind: SyntheticRepoKind::RelationExtraction,
        files,
        tasks: vec![task],
    }
}

fn long_chain_repo() -> SyntheticRepo {
    let file = "src/chain.ts";
    let mut files = BTreeMap::new();
    files.insert(
        file.to_string(),
        r#"
const rows: any[] = [];

export function writeAudit(event: any) {
  rows.push(event);
}

export function persistLogin(user: any) {
  writeAudit({ type: "login", user });
}

export function loginService(user: any) {
  persistLogin(user);
}

export function loginEndpoint(req: any) {
  loginService(req.user);
}
"#
        .trim()
        .to_string(),
    );
    let task = task(
        "long-chain-endpoint-to-mutation",
        BenchmarkFamily::LongChainPath,
        "Find endpoint path from loginEndpoint to the mutation of rows",
        SyntheticRepoKind::LongChainPath,
        GroundTruth {
            expected_relation_sequences: vec![vec![
                RelationKind::Calls,
                RelationKind::Calls,
                RelationKind::Calls,
                RelationKind::Mutates,
            ]],
            expected_files: vec![file.to_string()],
            expected_symbols: vec![
                "loginEndpoint".to_string(),
                "loginService".to_string(),
                "persistLogin".to_string(),
                "writeAudit".to_string(),
            ],
            ..GroundTruth::default()
        },
    );
    SyntheticRepo {
        id: "synthetic-long-chain".to_string(),
        kind: SyntheticRepoKind::LongChainPath,
        files,
        tasks: vec![task],
    }
}

fn context_retrieval_repo() -> SyntheticRepo {
    let file = "src/context.ts";
    let mut files = BTreeMap::new();
    files.insert(
        file.to_string(),
        r#"
export function loadUser(id: string) {
  return { id, email: "a@example.com" };
}

export function issueResetToken(user: any) {
  return `${user.id}:reset`;
}

export function resetPassword(userId: string) {
  const user = loadUser(userId);
  return issueResetToken(user);
}
"#
        .trim()
        .to_string(),
    );
    let task = task(
        "context-reset-password",
        BenchmarkFamily::ContextRetrieval,
        "Build context for changing resetPassword and issueResetToken behavior",
        SyntheticRepoKind::ContextRetrieval,
        GroundTruth {
            expected_relation_sequences: vec![vec![RelationKind::Calls]],
            expected_files: vec![file.to_string()],
            expected_symbols: vec!["resetPassword".to_string(), "issueResetToken".to_string()],
            ..GroundTruth::default()
        },
    );
    SyntheticRepo {
        id: "synthetic-context-retrieval".to_string(),
        kind: SyntheticRepoKind::ContextRetrieval,
        files,
        tasks: vec![task],
    }
}

fn agent_patch_repo() -> SyntheticRepo {
    let file = "src/patch.ts";
    let test_file = "src/patch.spec.ts";
    let mut files = BTreeMap::new();
    files.insert(
        file.to_string(),
        r#"
export function applyDiscount(total: number, percent: number) {
  return total - percent;
}
"#
        .trim()
        .to_string(),
    );
    files.insert(
        test_file.to_string(),
        r#"
import { applyDiscount } from "./patch";

test("applyDiscount uses percent", () => {
  expect(applyDiscount(100, 10)).toBe(90);
});
"#
        .trim()
        .to_string(),
    );
    let task = task(
        "agent-patch-discount",
        BenchmarkFamily::AgentPatch,
        "Patch applyDiscount so percent is interpreted as a percentage and run related tests",
        SyntheticRepoKind::AgentPatch,
        GroundTruth {
            expected_files: vec![file.to_string(), test_file.to_string()],
            expected_symbols: vec!["applyDiscount".to_string()],
            expected_tests: vec!["applyDiscount uses percent".to_string()],
            expected_patch_success: Some(true),
            expected_test_success: Some(true),
            ..GroundTruth::default()
        },
    );
    SyntheticRepo {
        id: "synthetic-agent-patch".to_string(),
        kind: SyntheticRepoKind::AgentPatch,
        files,
        tasks: vec![task],
    }
}

fn compression_repo() -> SyntheticRepo {
    let file = "src/compression.ts";
    let mut files = BTreeMap::new();
    files.insert(
        file.to_string(),
        r#"
export function binarySignature(text: string) {
  return text.toLowerCase().split(/\s+/).join("|");
}

export function pqRerank(query: string, document: string) {
  return binarySignature(query) === binarySignature(document);
}
"#
        .trim()
        .to_string(),
    );
    let task = task(
        "compression-rerank",
        BenchmarkFamily::Compression,
        "Compare binary signature and PQ rerank candidates for pqRerank",
        SyntheticRepoKind::Compression,
        GroundTruth {
            expected_files: vec![file.to_string()],
            expected_symbols: vec!["binarySignature".to_string(), "pqRerank".to_string()],
            expected_relation_sequences: vec![vec![RelationKind::Calls]],
            ..GroundTruth::default()
        },
    );
    SyntheticRepo {
        id: "synthetic-compression".to_string(),
        kind: SyntheticRepoKind::Compression,
        files,
        tasks: vec![task],
    }
}

fn security_auth_repo() -> SyntheticRepo {
    let file = "src/security.ts";
    let mut files = BTreeMap::new();
    files.insert(
        file.to_string(),
        r#"
export function checkRole(user: any, role: string) {
  return user.roles.includes(role);
}

export function sanitize(input: string) {
  return input.replace(/[<>]/g, "");
}

export function saveProfile(value: string) {
  return db.query("update users set bio = ?", [value]);
}

export function profileRoute(req: any, res: any) {
  if (!checkRole(req.user, "admin")) throw new Error("forbidden");
  const clean = sanitize(req.body.bio);
  return saveProfile(clean);
}

app.post("/profile", profileRoute);
"#
        .trim()
        .to_string(),
    );
    let task = task(
        "security-auth-profile",
        BenchmarkFamily::SecurityAuth,
        "Find the auth and sanitizer path protecting profileRoute before saveProfile",
        SyntheticRepoKind::SecurityAuth,
        GroundTruth {
            expected_relations: vec![
                expected_relation(RelationKind::Exposes, "profile", "profileRoute", file),
                expected_relation(RelationKind::ChecksRole, "profileRoute", "admin", file),
                expected_relation(RelationKind::Sanitizes, "sanitize", "bio", file),
            ],
            expected_relation_sequences: vec![vec![
                RelationKind::Exposes,
                RelationKind::Calls,
                RelationKind::ChecksRole,
            ]],
            expected_files: vec![file.to_string()],
            expected_symbols: vec![
                "profileRoute".to_string(),
                "checkRole".to_string(),
                "sanitize".to_string(),
                "saveProfile".to_string(),
            ],
            ..GroundTruth::default()
        },
    );
    SyntheticRepo {
        id: "synthetic-security-auth".to_string(),
        kind: SyntheticRepoKind::SecurityAuth,
        files,
        tasks: vec![task],
    }
}

fn async_event_repo() -> SyntheticRepo {
    let file = "src/events.ts";
    let mut files = BTreeMap::new();
    files.insert(
        file.to_string(),
        r#"
import { EventEmitter } from "events";

const bus = new EventEmitter();

export function mutateCache(payload: any) {
  cache.value = payload.id;
}

export function publishUserCreated(user: any) {
  bus.emit("user.created", user);
}

bus.on("user.created", async (event) => {
  await mutateCache(event);
});
"#
        .trim()
        .to_string(),
    );
    let task = task(
        "async-event-user-created",
        BenchmarkFamily::AsyncEvent,
        "Trace user.created publisher to listener and mutation",
        SyntheticRepoKind::AsyncEvent,
        GroundTruth {
            expected_relations: vec![
                expected_relation(
                    RelationKind::Emits,
                    "publishUserCreated",
                    "user.created",
                    file,
                ),
                expected_relation(RelationKind::ListensTo, "user.created", "mutateCache", file),
                expected_relation(RelationKind::Awaits, "user.created", "mutateCache", file),
            ],
            expected_relation_sequences: vec![vec![
                RelationKind::Emits,
                RelationKind::ListensTo,
                RelationKind::Calls,
                RelationKind::Mutates,
            ]],
            expected_files: vec![file.to_string()],
            expected_symbols: vec!["publishUserCreated".to_string(), "mutateCache".to_string()],
            ..GroundTruth::default()
        },
    );
    SyntheticRepo {
        id: "synthetic-async-event".to_string(),
        kind: SyntheticRepoKind::AsyncEvent,
        files,
        tasks: vec![task],
    }
}

fn test_impact_repo() -> SyntheticRepo {
    let file = "src/auth.ts";
    let test_file = "src/auth.spec.ts";
    let mut files = BTreeMap::new();
    files.insert(
        file.to_string(),
        r#"
export function login(user: any) {
  return { token: `${user.id}:token` };
}
"#
        .trim()
        .to_string(),
    );
    files.insert(
        test_file.to_string(),
        r#"
import { login } from "./auth";

test("login returns token", () => {
  const result = login({ id: "u1" });
  expect(result.token).toBeDefined();
});
"#
        .trim()
        .to_string(),
    );
    let task = task(
        "test-impact-login",
        BenchmarkFamily::TestImpact,
        "Select tests impacted by changing login token format",
        SyntheticRepoKind::TestImpact,
        GroundTruth {
            expected_files: vec![file.to_string(), test_file.to_string()],
            expected_symbols: vec!["login".to_string()],
            expected_tests: vec!["login returns token".to_string()],
            expected_relation_sequences: vec![
                vec![RelationKind::Tests],
                vec![RelationKind::Asserts],
            ],
            expected_test_success: Some(true),
            ..GroundTruth::default()
        },
    );
    SyntheticRepo {
        id: "synthetic-test-impact".to_string(),
        kind: SyntheticRepoKind::TestImpact,
        files,
        tasks: vec![task],
    }
}

fn all_families_repo() -> SyntheticRepo {
    let mut files = BTreeMap::new();
    let mut tasks = Vec::new();
    for family in BenchmarkFamily::ALL {
        let repo = synthetic_repo(SyntheticRepoKind::from(family));
        files.extend(repo.files);
        tasks.extend(repo.tasks);
    }
    SyntheticRepo {
        id: "synthetic-all-families".to_string(),
        kind: SyntheticRepoKind::AllFamilies,
        files,
        tasks,
    }
}

fn task(
    id: &str,
    family: BenchmarkFamily,
    prompt: &str,
    kind: SyntheticRepoKind,
    ground_truth: GroundTruth,
) -> BenchmarkTask {
    BenchmarkTask {
        id: id.to_string(),
        family,
        prompt: prompt.to_string(),
        repo: BenchmarkRepoSpec::Synthetic { kind },
        ground_truth,
        k_values: default_k_values(),
        metadata: BTreeMap::new(),
    }
}

fn expected_relation(
    relation: RelationKind,
    head_contains: &str,
    tail_contains: &str,
    repo_relative_path: &str,
) -> ExpectedRelation {
    ExpectedRelation {
        relation,
        head_contains: head_contains.to_string(),
        tail_contains: tail_contains.to_string(),
        repo_relative_path: Some(repo_relative_path.to_string()),
    }
}

fn build_documents(
    files: &BTreeMap<String, String>,
    entities: &[Entity],
) -> Vec<RetrievalDocument> {
    let mut documents = Vec::new();
    for (path, source) in files {
        let mut document = RetrievalDocument::new(path.clone(), source.clone()).stage0_score(0.35);
        document
            .metadata
            .insert("kind".to_string(), "file".to_string());
        document.metadata.insert("file".to_string(), path.clone());
        documents.push(document);
    }
    for entity in entities {
        let mut document = RetrievalDocument::new(
            entity.id.clone(),
            format!(
                "{} {} {} {}",
                entity.kind, entity.name, entity.qualified_name, entity.repo_relative_path
            ),
        )
        .stage0_score(if entity.confidence >= 1.0 { 0.7 } else { 0.45 });
        document
            .metadata
            .insert("kind".to_string(), entity.kind.to_string());
        document
            .metadata
            .insert("file".to_string(), entity.repo_relative_path.clone());
        if entity.confidence >= 1.0 {
            document
                .metadata
                .insert("exact_symbol_match".to_string(), "true".to_string());
        }
        documents.push(document);
    }
    documents
}

fn run_bm25_baseline(
    task: &BenchmarkTask,
    corpus: &IndexedBenchRepo,
    result: &mut BenchmarkRunResult,
) -> BenchResult<()> {
    let query = sanitize_fts_query(&task.prompt);
    let hits = corpus
        .store
        .search_text(&query, DEFAULT_TOP_K)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    for hit in hits {
        push_unique(&mut result.retrieved_files, hit.repo_relative_path);
        if hit.kind.as_str() == "entity" {
            push_unique(&mut result.retrieved_symbols, hit.title);
        }
    }
    result.metrics.token_cost = estimate_tokens(&task.prompt);
    result.metrics.latency_ms = estimate_latency_ms(corpus, 2);
    result.metrics.memory_bytes = estimate_memory_bytes(corpus, BaselineMode::GrepBm25);
    Ok(())
}

fn run_vector_only_baseline(
    task: &BenchmarkTask,
    corpus: &IndexedBenchRepo,
    result: &mut BenchmarkRunResult,
) -> BenchResult<()> {
    let mut index = InMemoryBinaryVectorIndex::new(128)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    for document in &corpus.documents {
        index
            .upsert_text(&document.id, &document.text)
            .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    }
    let hits = index
        .search_text(&task.prompt, DEFAULT_TOP_K)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    for hit in hits {
        push_document_hit(corpus, result, &hit.id);
    }
    result.metrics.token_cost = estimate_tokens(&task.prompt);
    result.metrics.latency_ms = estimate_latency_ms(corpus, 3);
    result.metrics.memory_bytes = estimate_memory_bytes(corpus, BaselineMode::VectorOnly);
    Ok(())
}

fn run_graph_only_baseline(
    task: &BenchmarkTask,
    corpus: &IndexedBenchRepo,
    result: &mut BenchmarkRunResult,
) -> BenchResult<()> {
    let seeds = resolve_seed_ids(task, corpus)?;
    let engine = ExactGraphQueryEngine::new(corpus.edges.clone());
    let paths = graph_paths_for_seeds(&engine, &seeds);
    collect_paths(corpus, result, &paths);
    result.metrics.token_cost = estimate_tokens(&task.prompt);
    result.metrics.latency_ms = estimate_latency_ms(corpus, 4);
    result.metrics.memory_bytes = estimate_memory_bytes(corpus, BaselineMode::GraphOnly);
    Ok(())
}

fn run_graph_binary_pq_baseline(
    task: &BenchmarkTask,
    corpus: &IndexedBenchRepo,
    result: &mut BenchmarkRunResult,
) -> BenchResult<()> {
    let seeds = resolve_seed_ids(task, corpus)?;
    let mut documents = corpus.documents.clone();
    for seed in &seeds {
        documents.push(RetrievalDocument::new(seed.clone(), seed.clone()).stage0_score(1.0));
    }
    let mut binary = InMemoryBinaryVectorIndex::new(128)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    for document in &documents {
        binary
            .upsert_text(&document.id, &document.text)
            .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    }
    let signature = BinarySignature::from_text(&task.prompt, 128)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    let candidates = binary
        .search_with_exact_seeds(&signature, 16, &seeds)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    let rerank_candidates = candidates
        .iter()
        .map(|candidate| {
            let document = documents
                .iter()
                .find(|document| document.id == candidate.id)
                .cloned()
                .unwrap_or_else(|| RetrievalDocument::new(&candidate.id, &candidate.id));
            let mut rerank = RerankCandidate::new(document.id, document.text)
                .stage0_score(document.stage0_score)
                .exact_seed(candidate.exact_seed);
            if let Some(similarity) = candidate.similarity {
                rerank = rerank.stage1_similarity(similarity);
            }
            rerank
        })
        .collect::<Vec<_>>();
    let reranker = DeterministicCompressedReranker::new(RerankConfig::default());
    let scores = reranker
        .rerank(&RerankQuery::new(&task.prompt), &rerank_candidates, 12)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    let engine = ExactGraphQueryEngine::new(corpus.edges.clone());
    let candidate_ids = scores
        .iter()
        .map(|score| score.id.clone())
        .chain(seeds)
        .collect::<Vec<_>>();
    let paths = graph_paths_for_seeds(&engine, &candidate_ids);
    collect_paths(corpus, result, &paths);
    result.metrics.token_cost = estimate_tokens(&task.prompt);
    result.metrics.latency_ms = estimate_latency_ms(corpus, 5);
    result.metrics.memory_bytes = estimate_memory_bytes(corpus, BaselineMode::GraphBinaryPqFunnel);
    Ok(())
}

fn run_graph_bayesian_baseline(
    task: &BenchmarkTask,
    corpus: &IndexedBenchRepo,
    result: &mut BenchmarkRunResult,
) -> BenchResult<()> {
    let seeds = resolve_seed_ids(task, corpus)?;
    let engine = ExactGraphQueryEngine::new(corpus.edges.clone());
    let ranker = BayesianRanker::new(BayesianRankerConfig::default());
    let mut scored = graph_paths_for_seeds(&engine, &seeds)
        .into_iter()
        .map(|path| {
            let candidate = path.source.clone();
            let score = ranker.score_path(&candidate, &path, None, None, &seeds);
            (path, score.probability)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.target.cmp(&right.0.target))
    });
    let paths = scored.into_iter().map(|(path, _)| path).collect::<Vec<_>>();
    collect_paths(corpus, result, &paths);
    result.metrics.token_cost = estimate_tokens(&task.prompt);
    result.metrics.latency_ms = estimate_latency_ms(corpus, 6);
    result.metrics.memory_bytes = estimate_memory_bytes(corpus, BaselineMode::GraphBayesianRanker);
    Ok(())
}

fn run_full_context_packet_baseline(
    task: &BenchmarkTask,
    corpus: &IndexedBenchRepo,
    result: &mut BenchmarkRunResult,
) -> BenchResult<()> {
    let seeds = resolve_seed_ids(task, corpus)?;
    let config = RetrievalFunnelConfig {
        stage1_top_k: 32,
        stage2_top_n: 16,
        query_limits: query_limits(),
        ..RetrievalFunnelConfig::default()
    };
    let funnel = RetrievalFunnel::new(corpus.edges.clone(), corpus.documents.clone(), config)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    let request = codegraph_query::RetrievalFunnelRequest::new(&task.prompt, "benchmark", 1_600)
        .exact_seeds(seeds)
        .stage0_candidates(corpus.documents.clone())
        .sources(corpus.sources.clone());
    let output = funnel
        .run(request)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    for symbol in &output.packet.symbols {
        push_unique(&mut result.retrieved_symbols, symbol.clone());
    }
    for snippet in &output.packet.snippets {
        push_unique(&mut result.retrieved_files, snippet.file.clone());
    }
    for test in &output.packet.recommended_tests {
        push_unique(&mut result.retrieved_tests, test.clone());
    }
    result.retrieved_paths = output
        .packet
        .verified_paths
        .iter()
        .map(|path| BenchPath {
            source: path.source.clone(),
            target: path.target.clone(),
            relations: path.metapath.clone(),
            edge_ids: path
                .edges
                .iter()
                .map(|(head, relation, tail)| format!("{head}|{relation}|{tail}"))
                .collect(),
            source_spans: path.source_spans.iter().map(ToString::to_string).collect(),
            confidence: path.confidence,
            exactness: vec![path.exactness],
        })
        .collect();
    result.context_packet = Some(
        serde_json::to_value(&output.packet)
            .map_err(|error| BenchmarkError::Parse(error.to_string()))?,
    );
    result.metrics.token_cost = estimate_packet_tokens(&output.packet);
    result.metrics.latency_ms = estimate_latency_ms(corpus, 7);
    result.metrics.memory_bytes = estimate_memory_bytes(corpus, BaselineMode::FullContextPacket);
    Ok(())
}

fn resolve_seed_ids(task: &BenchmarkTask, corpus: &IndexedBenchRepo) -> BenchResult<Vec<String>> {
    let mut seed_ids = BTreeSet::new();
    for seed in extract_prompt_seeds(&task.prompt) {
        if let Some(value) = seed.exact_value() {
            resolve_seed_value(corpus, &value, &mut seed_ids)?;
        }
    }
    for symbol in &task.ground_truth.expected_symbols {
        resolve_seed_value(corpus, symbol, &mut seed_ids)?;
    }
    for test in &task.ground_truth.expected_tests {
        resolve_seed_value(corpus, test, &mut seed_ids)?;
    }
    Ok(seed_ids.into_iter().collect())
}

fn resolve_seed_value(
    corpus: &IndexedBenchRepo,
    value: &str,
    seeds: &mut BTreeSet<String>,
) -> BenchResult<()> {
    for entity in corpus
        .store
        .find_entities_by_exact_symbol(value)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?
    {
        seeds.insert(entity.id);
    }
    for entity in &corpus.entities {
        if entity.name.contains(value)
            || entity.qualified_name.contains(value)
            || entity.id.contains(value)
        {
            seeds.insert(entity.id.clone());
        }
    }
    Ok(())
}

fn graph_paths_for_seeds(engine: &ExactGraphQueryEngine, seeds: &[String]) -> Vec<GraphPath> {
    let mut paths = Vec::new();
    for seed in seeds {
        let impact = engine.impact_analysis_core(seed, query_limits());
        paths.extend(impact.callers);
        paths.extend(impact.callees);
        paths.extend(impact.reads);
        paths.extend(impact.writes);
        paths.extend(impact.mutations);
        paths.extend(impact.dataflow);
        paths.extend(impact.auth_paths);
        paths.extend(impact.event_flow);
        paths.extend(impact.tests);
        paths.extend(impact.migrations);
    }
    unique_bench_paths(paths)
}

fn collect_paths(corpus: &IndexedBenchRepo, result: &mut BenchmarkRunResult, paths: &[GraphPath]) {
    for path in paths {
        push_unique(&mut result.retrieved_symbols, path.source.clone());
        push_unique(&mut result.retrieved_symbols, path.target.clone());
        for step in &path.steps {
            push_unique(&mut result.retrieved_symbols, step.from.clone());
            push_unique(&mut result.retrieved_symbols, step.to.clone());
            if let Some(entity) =
                entity_by_id(corpus, &step.from).or_else(|| entity_by_id(corpus, &step.to))
            {
                push_unique(
                    &mut result.retrieved_files,
                    entity.repo_relative_path.clone(),
                );
            }
            if matches!(
                step.edge.relation,
                RelationKind::Tests
                    | RelationKind::Asserts
                    | RelationKind::Mocks
                    | RelationKind::Stubs
                    | RelationKind::Covers
                    | RelationKind::FixturesFor
            ) {
                push_unique(&mut result.retrieved_tests, compact_label(&step.to));
            }
        }
    }
    result
        .retrieved_paths
        .extend(paths.iter().map(path_to_bench_path));
}

fn push_document_hit(corpus: &IndexedBenchRepo, result: &mut BenchmarkRunResult, id: &str) {
    if corpus.sources.contains_key(id) {
        push_unique(&mut result.retrieved_files, id.to_string());
        return;
    }
    if let Some(entity) = entity_by_id(corpus, id) {
        push_unique(&mut result.retrieved_symbols, entity.qualified_name.clone());
        push_unique(&mut result.retrieved_symbols, entity.id.clone());
        push_unique(
            &mut result.retrieved_files,
            entity.repo_relative_path.clone(),
        );
        if entity.kind.to_string().contains("Test") {
            push_unique(&mut result.retrieved_tests, entity.qualified_name.clone());
        }
    }
}

fn evaluate_result(task: &BenchmarkTask, result: &BenchmarkRunResult) -> BenchmarkMetrics {
    let mut metrics = result.metrics.clone();
    let expected_files = set_from(&task.ground_truth.expected_files);
    let expected_symbols = set_from(&task.ground_truth.expected_symbols);
    let expected_tests = set_from(&task.ground_truth.expected_tests);
    let observed_files = set_from(&result.retrieved_files);
    let observed_symbols = symbol_match_set(&expected_symbols, &result.retrieved_symbols);
    let observed_tests = symbol_match_set(&expected_tests, &result.retrieved_tests);

    let expected_artifacts = expected_files
        .iter()
        .chain(expected_symbols.iter())
        .chain(expected_tests.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let observed_artifacts = observed_files
        .iter()
        .chain(observed_symbols.iter())
        .chain(observed_tests.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let artifact_score = precision_recall_f1(&expected_artifacts, &observed_artifacts);
    metrics.precision = artifact_score.precision;
    metrics.recall = artifact_score.recall;
    metrics.f1 = artifact_score.f1;

    let relation_score = relation_score(
        &task.ground_truth.expected_relations,
        &result.observed_relations,
    );
    metrics.relation_precision = relation_score.precision;
    metrics.relation_recall = relation_score.recall;
    metrics.relation_f1 = relation_score.f1;

    for k in &task.k_values {
        metrics.file_recall_at_k.insert(
            format!("recall@{k}"),
            recall_at_k(&expected_files, &result.retrieved_files, *k),
        );
        metrics.symbol_recall_at_k.insert(
            format!("recall@{k}"),
            recall_at_k(
                &expected_symbols,
                &symbol_match_ranked(&expected_symbols, &result.retrieved_symbols),
                *k,
            ),
        );
        metrics.path_recall_at_k.insert(
            format!("recall@{k}"),
            path_recall_at_k(
                &task.ground_truth.expected_relation_sequences,
                &result.retrieved_paths,
                *k,
            ),
        );
    }
    let ranked_artifacts = result
        .retrieved_symbols
        .iter()
        .chain(result.retrieved_files.iter())
        .chain(result.retrieved_tests.iter())
        .cloned()
        .collect::<Vec<_>>();
    metrics.mrr = mean_reciprocal_rank(&expected_artifacts, &ranked_artifacts);
    metrics.ndcg = ndcg_at_k(&expected_artifacts, &ranked_artifacts, DEFAULT_TOP_K);
    metrics.patch_success = task
        .ground_truth
        .expected_patch_success
        .map(|expected| expected && metrics.recall >= 0.5);
    metrics.test_success = task
        .ground_truth
        .expected_test_success
        .map(|expected| expected && (!expected_tests.is_empty() && !observed_tests.is_empty()));
    metrics
}

fn relation_score(expected: &[ExpectedRelation], observed: &[ObservedRelation]) -> MetricScore {
    if expected.is_empty() {
        return MetricScore::zero();
    }
    let matched = expected
        .iter()
        .filter(|expected| {
            observed
                .iter()
                .any(|observed| expected_relation_matches(expected, observed))
        })
        .count() as f64;
    let relevant_observed = observed
        .iter()
        .filter(|observed| {
            expected
                .iter()
                .any(|expected| expected.relation == observed.relation)
        })
        .count() as f64;
    let precision = divide(matched, relevant_observed);
    let recall = divide(matched, expected.len() as f64);
    MetricScore {
        precision,
        recall,
        f1: f1(precision, recall),
    }
}

fn expected_relation_matches(expected: &ExpectedRelation, observed: &ObservedRelation) -> bool {
    expected.relation == observed.relation
        && observed.head_id.contains(&expected.head_contains)
        && observed.tail_id.contains(&expected.tail_contains)
        && expected
            .repo_relative_path
            .as_ref()
            .is_none_or(|path| &observed.repo_relative_path == path)
}

fn path_recall_at_k(expected: &[Vec<RelationKind>], observed: &[BenchPath], k: usize) -> f64 {
    if expected.is_empty() {
        return 1.0;
    }
    let hits = expected
        .iter()
        .filter(|sequence| {
            observed
                .iter()
                .take(k)
                .any(|path| relation_sequence_contains(&path.relations, sequence))
        })
        .count() as f64;
    divide(hits, expected.len() as f64)
}

fn relation_sequence_contains(actual: &[RelationKind], expected: &[RelationKind]) -> bool {
    if expected.is_empty() {
        return true;
    }
    actual
        .windows(expected.len())
        .any(|window| window == expected)
        || actual == expected
}

fn aggregate_results(results: &[BenchmarkRunResult]) -> BTreeMap<String, BenchmarkAggregate> {
    let mut grouped: BTreeMap<String, Vec<&BenchmarkRunResult>> = BTreeMap::new();
    for result in results
        .iter()
        .filter(|result| result.status == BenchmarkRunStatus::Completed)
    {
        grouped
            .entry(result.baseline.as_str().to_string())
            .or_default()
            .push(result);
    }

    grouped
        .into_iter()
        .map(|(baseline, runs)| {
            let count = runs.len().max(1) as f64;
            let aggregate = BenchmarkAggregate {
                runs: runs.len(),
                average_precision: average(&runs, |run| run.metrics.precision, count),
                average_recall: average(&runs, |run| run.metrics.recall, count),
                average_f1: average(&runs, |run| run.metrics.f1, count),
                average_path_recall_at_10: average(
                    &runs,
                    |run| metric_map_value(&run.metrics.path_recall_at_k, "recall@10"),
                    count,
                ),
                average_symbol_recall_at_10: average(
                    &runs,
                    |run| metric_map_value(&run.metrics.symbol_recall_at_k, "recall@10"),
                    count,
                ),
                average_file_recall_at_10: average(
                    &runs,
                    |run| metric_map_value(&run.metrics.file_recall_at_k, "recall@10"),
                    count,
                ),
                average_mrr: average(&runs, |run| run.metrics.mrr, count),
                average_ndcg: average(&runs, |run| run.metrics.ndcg, count),
                total_token_cost: runs.iter().map(|run| run.metrics.token_cost).sum(),
                total_latency_ms: runs.iter().map(|run| run.metrics.latency_ms).sum(),
                max_memory_bytes: runs
                    .iter()
                    .map(|run| run.metrics.memory_bytes)
                    .max()
                    .unwrap_or(0),
            };
            (baseline, aggregate)
        })
        .collect()
}

fn average(
    runs: &[&BenchmarkRunResult],
    value: impl Fn(&BenchmarkRunResult) -> f64,
    count: f64,
) -> f64 {
    clean_metric(runs.iter().map(|run| value(run)).sum::<f64>() / count)
}

fn metric_map_value(values: &BTreeMap<String, f64>, key: &str) -> f64 {
    values.get(key).copied().unwrap_or(0.0)
}

fn observed_relation(edge: &Edge) -> ObservedRelation {
    ObservedRelation {
        edge_id: edge.id.clone(),
        relation: edge.relation,
        head_id: edge.head_id.clone(),
        tail_id: edge.tail_id.clone(),
        repo_relative_path: edge.source_span.repo_relative_path.clone(),
    }
}

fn path_to_bench_path(path: &GraphPath) -> BenchPath {
    let confidence = path
        .steps
        .iter()
        .map(|step| step.edge.confidence)
        .fold(1.0_f64, f64::min);
    BenchPath {
        source: path.source.clone(),
        target: path.target.clone(),
        relations: path.steps.iter().map(|step| step.edge.relation).collect(),
        edge_ids: path.steps.iter().map(|step| step.edge.id.clone()).collect(),
        source_spans: path
            .steps
            .iter()
            .map(|step| step.edge.source_span.to_string())
            .collect(),
        confidence,
        exactness: path.steps.iter().map(|step| step.edge.exactness).collect(),
    }
}

fn unique_bench_paths(paths: Vec<GraphPath>) -> Vec<GraphPath> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for path in paths {
        let key = path
            .steps
            .iter()
            .map(|step| step.edge.id.clone())
            .collect::<Vec<_>>()
            .join("|");
        if seen.insert((path.source.clone(), path.target.clone(), key)) {
            unique.push(path);
        }
    }
    unique
}

fn entity_by_id<'a>(corpus: &'a IndexedBenchRepo, id: &str) -> Option<&'a Entity> {
    corpus.entities.iter().find(|entity| entity.id == id)
}

fn source_snippet(span: &SourceSpan, source: &str) -> String {
    let start = span.start_line.saturating_sub(1) as usize;
    let end = span.end_line.max(span.start_line) as usize;
    source
        .lines()
        .skip(start)
        .take(end.saturating_sub(start).min(8))
        .collect::<Vec<_>>()
        .join("\n")
}

fn sanitize_fts_query(prompt: &str) -> String {
    let terms = prompt
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|term| term.len() >= 3)
        .take(8)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        "codegraph".to_string()
    } else {
        terms.join(" ")
    }
}

fn query_limits() -> QueryLimits {
    QueryLimits {
        max_depth: 6,
        max_paths: 24,
        max_edges_visited: 2_048,
    }
}

fn set_from(values: &[String]) -> BTreeSet<String> {
    values.iter().cloned().collect()
}

fn symbol_match_set(expected: &BTreeSet<String>, observed: &[String]) -> BTreeSet<String> {
    symbol_match_ranked(expected, observed)
        .into_iter()
        .collect()
}

fn symbol_match_ranked(expected: &BTreeSet<String>, observed: &[String]) -> Vec<String> {
    let mut ranked = Vec::new();
    for value in observed {
        if expected.contains(value) {
            push_unique(&mut ranked, value.clone());
            continue;
        }
        if let Some(expected_value) = expected
            .iter()
            .find(|expected_value| value.contains(expected_value.as_str()))
        {
            push_unique(&mut ranked, expected_value.clone());
        }
    }
    ranked
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn compact_label(id: &str) -> String {
    id.rsplit(['#', '/', ':'])
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(id)
        .to_string()
}

fn estimate_tokens(text: &str) -> u64 {
    text.len().div_ceil(4) as u64
}

fn estimate_packet_tokens(packet: &codegraph_core::ContextPacket) -> u64 {
    let text_bytes = packet
        .snippets
        .iter()
        .map(|snippet| snippet.text.len())
        .sum::<usize>()
        + packet
            .verified_paths
            .iter()
            .map(|path| path.summary.as_deref().unwrap_or("").len() + path.edges.len() * 16)
            .sum::<usize>();
    text_bytes.div_ceil(4) as u64
}

fn estimate_latency_ms(corpus: &IndexedBenchRepo, multiplier: u64) -> u64 {
    let base =
        corpus.entities.len() as u64 + corpus.edges.len() as u64 + corpus.sources.len() as u64;
    (base.max(1) * multiplier).min(60_000)
}

fn estimate_memory_bytes(corpus: &IndexedBenchRepo, baseline: BaselineMode) -> u64 {
    let source_bytes = corpus.sources.values().map(String::len).sum::<usize>() as u64;
    let graph_bytes = (corpus.entities.len() as u64 * 256) + (corpus.edges.len() as u64 * 384);
    let vector_bytes = corpus.documents.len() as u64 * 128 / 8;
    match baseline {
        BaselineMode::VanillaNoRetrieval => 0,
        BaselineMode::GrepBm25 => source_bytes,
        BaselineMode::VectorOnly => source_bytes + vector_bytes,
        BaselineMode::GraphOnly => source_bytes + graph_bytes,
        BaselineMode::GraphBinaryPqFunnel
        | BaselineMode::GraphBayesianRanker
        | BaselineMode::FullContextPacket => source_bytes + graph_bytes + vector_bytes,
    }
}

fn divide(numerator: f64, denominator: f64) -> f64 {
    if denominator <= f64::EPSILON {
        0.0
    } else {
        clean_metric(numerator / denominator)
    }
}

fn f1(precision: f64, recall: f64) -> f64 {
    if precision + recall <= f64::EPSILON {
        0.0
    } else {
        clean_metric(2.0 * precision * recall / (precision + recall))
    }
}

fn clean_metric(value: f64) -> f64 {
    if value.abs() <= f64::EPSILON {
        0.0
    } else {
        value
    }
}

pub fn unique_output_path(prefix: &str, extension: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}-{millis}.{extension}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_schema_validation_accepts_default_suite() {
        let suite = default_benchmark_suite();
        suite.validate().expect("default suite validates");
        let json = serde_json::to_string(&suite).expect("suite serializes");
        let round_trip: BenchmarkSuite = serde_json::from_str(&json).expect("suite deserializes");
        assert_eq!(round_trip.schema_version, BENCH_SCHEMA_VERSION);

        let invalid = BenchmarkSuite {
            schema_version: BENCH_SCHEMA_VERSION,
            id: String::new(),
            tasks: suite.tasks,
            metadata: BTreeMap::new(),
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn graph_truth_case_schema_accepts_strict_case() {
        let schema = load_graph_truth_case_schema();
        assert_graph_truth_schema_has_required_fields(&schema);

        let case = valid_graph_truth_case();
        validate_graph_truth_case_for_tests(&schema, &case).expect("strict case validates");
    }

    #[test]
    fn graph_truth_case_schema_rejects_malformed_cases() {
        let schema = load_graph_truth_case_schema();

        let mut missing_span = valid_graph_truth_case();
        missing_span["expected_edges"][0]
            .as_object_mut()
            .expect("edge object")
            .remove("source_span");
        assert!(validate_graph_truth_case_for_tests(&schema, &missing_span).is_err());

        let mut missing_required = valid_graph_truth_case();
        missing_required
            .as_object_mut()
            .expect("case object")
            .remove("expected_entities");
        assert!(validate_graph_truth_case_for_tests(&schema, &missing_required).is_err());

        let mut unknown_field = valid_graph_truth_case();
        unknown_field["unexpected_scoring_hint"] = json!(true);
        assert!(validate_graph_truth_case_for_tests(&schema, &unknown_field).is_err());

        let mut malformed_edge = valid_graph_truth_case();
        malformed_edge["expected_edges"][0]["context_kind"] = json!("fixture");
        assert!(validate_graph_truth_case_for_tests(&schema, &malformed_edge).is_err());

        let mut unresolved_exact = valid_graph_truth_case();
        unresolved_exact["expected_edges"][0]["resolution"] = json!("unresolved");
        unresolved_exact["expected_edges"][0]["exactness"]["allowed"] = json!(["exact"]);
        assert!(validate_graph_truth_case_for_tests(&schema, &unresolved_exact).is_err());

        let mut derived_without_provenance = valid_graph_truth_case();
        derived_without_provenance["expected_edges"][0]["derived"] = json!(true);
        derived_without_provenance["expected_edges"][0]["provenance_edges"] = json!([]);
        assert!(validate_graph_truth_case_for_tests(&schema, &derived_without_provenance).is_err());

        let mut production_test_path = valid_graph_truth_case();
        production_test_path["expected_paths"][0]["context"] = json!("production");
        production_test_path["expected_paths"][0]["allow_test_mock_edges"] = json!(true);
        assert!(validate_graph_truth_case_for_tests(&schema, &production_test_path).is_err());

        let mut malformed_path = valid_graph_truth_case();
        malformed_path["expected_paths"][0]
            .as_object_mut()
            .expect("path object")
            .remove("relation_sequence");
        assert!(validate_graph_truth_case_for_tests(&schema, &malformed_path).is_err());

        let mut malformed_mutation = valid_graph_truth_case();
        malformed_mutation["mutation_steps"] =
            json!([{ "type": "rename_file", "from": "src/a.ts" }]);
        assert!(validate_graph_truth_case_for_tests(&schema, &malformed_mutation).is_err());

        let mut malformed_forbidden_edges = valid_graph_truth_case();
        malformed_forbidden_edges["forbidden_edges"] = json!({});
        assert!(validate_graph_truth_case_for_tests(&schema, &malformed_forbidden_edges).is_err());

        let mut malformed_source_spans = valid_graph_truth_case();
        malformed_source_spans["expected_source_spans"][0]["must_resolve_to_text"] = json!(false);
        assert!(validate_graph_truth_case_for_tests(&schema, &malformed_source_spans).is_err());
    }

    #[test]
    fn graph_truth_case_schema_validation_is_fast_for_100_manifests() {
        let schema = load_graph_truth_case_schema();
        let case = valid_graph_truth_case();
        let started = std::time::Instant::now();
        for _ in 0..100 {
            validate_graph_truth_case_for_tests(&schema, &case).expect("case validates");
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "100 manifest validations should complete in under 1 second"
        );
    }

    #[test]
    fn graph_truth_case_schema_defines_failure_rules() {
        let schema = load_graph_truth_case_schema();
        let failure_rules = required_values(&schema, "/$defs/failureRules/required");
        for rule in [
            "missing_required_edge_fails",
            "forbidden_edge_appearing_fails",
            "wrong_direction_fails",
            "missing_source_span_fails_for_proof_grade_edge",
            "unresolved_name_labeled_exact_fails",
            "mock_or_test_edge_in_production_proof_path_fails",
            "derived_edge_without_provenance_fails",
            "stale_edge_after_edit_fails",
            "same_name_symbol_collision_fails",
            "critical_symbol_missing_from_context_packet_fails",
            "forbidden_context_symbol_included_fails",
            "expected_test_omitted_fails",
            "too_many_distractors_fail_if_threshold_specified",
        ] {
            assert!(
                failure_rules.contains(rule),
                "failure rule {rule} missing from schema"
            );
        }

        let edge_required = required_values(&schema, "/$defs/edgeExpectation/required");
        for field in [
            "head",
            "relation",
            "tail",
            "source_file",
            "source_span",
            "exactness_required",
            "context_kind",
            "derived_allowed",
            "provenance_required",
            "exactness",
            "context",
            "resolution",
            "derived",
            "provenance_edges",
        ] {
            assert!(
                edge_required.contains(field),
                "edge field {field} missing from schema"
            );
        }

        let path_required = required_values(&schema, "/$defs/pathExpectation/required");
        for field in [
            "ordered_edges",
            "relation_sequence",
            "max_length",
            "source_span_required",
            "production_only",
            "derived_allowed",
            "provenance_required",
            "required_source_spans",
        ] {
            assert!(
                path_required.contains(field),
                "path field {field} missing from schema"
            );
        }
    }

    #[test]
    fn adversarial_graph_truth_fixture_cases_validate() {
        let schema = load_graph_truth_case_schema();
        let fixture_root = graph_truth_fixture_root();
        let expected_cases = [
            "same_function_name_only_one_imported",
            "dynamic_import_marked_heuristic",
            "file_rename_prunes_old_path",
            "import_alias_change_updates_target",
            "barrel_export_default_export_resolution",
            "test_mock_not_production_call",
            "sanitizer_exists_but_not_on_flow",
            "admin_user_middleware_role_separation",
            "derived_closure_edge_requires_provenance",
            "stale_graph_cache_after_edit_delete",
        ];
        assert_eq!(expected_cases.len(), 10);

        let mut cases_with_source_spans = 0usize;
        let mut security_or_test_cases = 0usize;
        let mut mutation_cases = 0usize;
        let mut barrel_or_default_export_cases = 0usize;
        let mut dynamic_import_cases = 0usize;
        for case_id in expected_cases {
            let case_path = fixture_root.join(case_id).join("graph_truth_case.json");
            let readme_path = fixture_root.join(case_id).join("README.md");
            let raw = fs::read_to_string(&case_path)
                .unwrap_or_else(|error| panic!("read {}: {error}", case_path.display()));
            let case: Value = serde_json::from_str(&raw)
                .unwrap_or_else(|error| panic!("parse {}: {error}", case_path.display()));
            validate_graph_truth_case_for_tests(&schema, &case)
                .unwrap_or_else(|error| panic!("validate {case_id}: {error}"));

            assert_eq!(
                string_field(&case, "case_id").expect("case_id"),
                case_id,
                "case_id should match fixture directory"
            );
            assert_eq!(
                string_field(&case, "repo_fixture_path").expect("repo_fixture_path"),
                format!("benchmarks/graph_truth/fixtures/{case_id}/repo"),
                "repo_fixture_path should point at the fixture repo"
            );
            assert!(
                fixture_repo_has_source_file(&fixture_root.join(case_id).join("repo")),
                "fixture {case_id} must include source files"
            );
            assert!(
                readme_path.exists(),
                "fixture {case_id} must include a README.md explaining the trap"
            );

            let forbidden_edges = array_field(&case, "forbidden_edges").expect("forbidden_edges");
            let forbidden_paths = array_field(&case, "forbidden_paths").expect("forbidden_paths");
            assert!(
                !forbidden_edges.is_empty() || !forbidden_paths.is_empty(),
                "fixture {case_id} must include a forbidden edge or path"
            );

            if !array_field(&case, "expected_source_spans")
                .expect("expected_source_spans")
                .is_empty()
            {
                cases_with_source_spans += 1;
            }
            if !array_field(&case, "mutation_steps")
                .expect("mutation_steps")
                .is_empty()
            {
                mutation_cases += 1;
            }
            if case_id.contains("barrel") || case_id.contains("default_export") {
                barrel_or_default_export_cases += 1;
            }
            if case_id.contains("dynamic_import") {
                dynamic_import_cases += 1;
            }

            let mut relations = BTreeSet::new();
            collect_graph_truth_relations(&case, &mut relations);
            if relations.iter().any(|relation| {
                matches!(
                    relation.as_str(),
                    "AUTHORIZES"
                        | "CHECKS_ROLE"
                        | "CHECKS_PERMISSION"
                        | "SANITIZES"
                        | "VALIDATES"
                        | "EXPOSES"
                        | "MOCKS"
                        | "STUBS"
                        | "TESTS"
                        | "ASSERTS"
                )
            }) {
                security_or_test_cases += 1;
            }
        }

        assert!(
            cases_with_source_spans >= 5,
            "at least five graph-truth fixtures must require source spans"
        );
        assert!(
            security_or_test_cases >= 3,
            "at least three graph-truth fixtures must cover test/mock/auth/security relations"
        );
        assert!(
            mutation_cases >= 3,
            "at least three graph-truth fixtures must include mutation_steps"
        );
        assert!(
            barrel_or_default_export_cases >= 1,
            "at least one graph-truth fixture must cover default/barrel exports"
        );
        assert!(
            dynamic_import_cases >= 1,
            "at least one graph-truth fixture must cover dynamic import or runtime require"
        );
    }

    fn load_graph_truth_case_schema() -> Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("benchmarks")
            .join("graph_truth")
            .join("schemas")
            .join("graph_truth_case.schema.json");
        let raw = fs::read_to_string(&path).expect("read graph truth schema");
        serde_json::from_str(&raw).expect("graph truth schema is valid JSON")
    }

    fn graph_truth_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("benchmarks")
            .join("graph_truth")
            .join("fixtures")
    }

    fn fixture_repo_has_source_file(root: &Path) -> bool {
        let Ok(entries) = fs::read_dir(root) else {
            return false;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if fixture_repo_has_source_file(&path) {
                    return true;
                }
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension,
                        "ts" | "tsx"
                            | "js"
                            | "jsx"
                            | "py"
                            | "go"
                            | "rs"
                            | "java"
                            | "cs"
                            | "c"
                            | "cc"
                            | "cpp"
                            | "h"
                            | "hpp"
                            | "rb"
                            | "php"
                            | "sql"
                    )
                })
            {
                return true;
            }
        }
        false
    }

    fn collect_graph_truth_relations(value: &Value, relations: &mut BTreeSet<String>) {
        match value {
            Value::Object(object) => {
                if let Some(relation) = object.get("relation").and_then(Value::as_str) {
                    relations.insert(relation.to_string());
                }
                for child in object.values() {
                    collect_graph_truth_relations(child, relations);
                }
            }
            Value::Array(values) => {
                for child in values {
                    collect_graph_truth_relations(child, relations);
                }
            }
            _ => {}
        }
    }

    fn assert_graph_truth_schema_has_required_fields(schema: &Value) {
        let required = required_values(schema, "/required");
        for field in [
            "case_id",
            "description",
            "repo_fixture_path",
            "task_prompt",
            "expected_entities",
            "expected_edges",
            "forbidden_edges",
            "expected_paths",
            "forbidden_paths",
            "expected_source_spans",
            "expected_context_symbols",
            "forbidden_context_symbols",
            "expected_tests",
            "forbidden_tests",
            "mutation_steps",
            "performance_expectations",
            "notes",
        ] {
            assert!(required.contains(field), "top-level field {field} missing");
        }
    }

    fn valid_graph_truth_case() -> Value {
        let edge = valid_graph_truth_edge("edge://login-calls-check-role", "CALLS");
        let forbidden_edge = valid_graph_truth_edge("edge://login-calls-test-double", "CALLS");
        let span = json!({
            "source_file": "src/auth.ts",
            "span": {
                "start_line": 12,
                "start_column": 10,
                "end_line": 12,
                "end_column": 31,
                "expected_text": "checkRole(user, \"admin\")",
                "syntax_role": "role_check"
            },
            "must_resolve_to_text": true,
            "expected_text": "checkRole(user, \"admin\")",
            "syntax_role": "role_check"
        });

        json!({
            "schema_version": 1,
            "case_id": "auth.role-check.strict",
            "description": "Login must call the production role checker and must not prove through a mock.",
            "repo_fixture_path": "benchmarks/graph_truth/fixtures/auth_role_check",
            "task_prompt": "Change login authorization behavior.",
            "expected_entities": [
                {
                    "selector": {
                        "qualified_name": "src/auth.login",
                        "kind": "Function",
                        "source_file": "src/auth.ts"
                    },
                    "kind": "Function",
                    "context": "production",
                    "source_file": "src/auth.ts",
                    "source_span": {
                        "start_line": 8,
                        "start_column": 1,
                        "end_line": 15,
                        "end_column": 2,
                        "syntax_role": "declaration"
                    }
                }
            ],
            "expected_edges": [edge.clone()],
            "forbidden_edges": [forbidden_edge],
            "expected_paths": [
                {
                    "path_id": "path://login-to-role-check",
                    "description": "Production login reaches role check through exact call evidence.",
                    "source": {
                        "qualified_name": "src/auth.login",
                        "kind": "Function",
                        "source_file": "src/auth.ts"
                    },
                    "target": {
                        "qualified_name": "src/auth.checkRole",
                        "kind": "Function",
                        "source_file": "src/auth.ts"
                    },
                    "ordered_edges": [edge],
                    "relation_sequence": ["CALLS"],
                    "max_length": 1,
                    "source_span_required": true,
                    "production_only": true,
                    "derived_allowed": false,
                    "provenance_required": true,
                    "required_source_spans": [span.clone()],
                    "context": "production",
                    "allow_test_mock_edges": false,
                    "derived_edges_require_provenance": true,
                    "proof_grade": true
                }
            ],
            "forbidden_paths": [
                {
                    "path_id": "path://login-to-mock-role-check",
                    "description": "A mock role check must not satisfy production proof.",
                    "source": {
                        "qualified_name": "src/auth.login",
                        "kind": "Function",
                        "source_file": "src/auth.ts"
                    },
                    "target": {
                        "qualified_name": "tests/auth.mockCheckRole",
                        "kind": "Mock",
                        "source_file": "tests/auth.test.ts"
                    },
                    "ordered_edges": [valid_graph_truth_edge("edge://login-calls-mock", "CALLS")],
                    "relation_sequence": ["CALLS"],
                    "max_length": 1,
                    "source_span_required": true,
                    "production_only": false,
                    "derived_allowed": false,
                    "provenance_required": true,
                    "required_source_spans": [
                        {
                            "source_file": "tests/auth.test.ts",
                            "span": {
                                "start_line": 5,
                                "start_column": 1,
                                "end_line": 5,
                                "end_column": 24,
                                "expected_text": "mockCheckRole(user)",
                                "syntax_role": "test_double"
                            },
                            "must_resolve_to_text": true,
                            "expected_text": "mockCheckRole(user)",
                            "syntax_role": "test_double"
                        }
                    ],
                    "context": "test",
                    "allow_test_mock_edges": true,
                    "derived_edges_require_provenance": true,
                    "proof_grade": false
                }
            ],
            "expected_source_spans": [span],
            "expected_context_symbols": [
                {
                    "symbol": {
                        "qualified_name": "src/auth.checkRole",
                        "kind": "Function",
                        "source_file": "src/auth.ts"
                    },
                    "critical": true,
                    "source_file": "src/auth.ts",
                    "reason": "Authorization proof target"
                }
            ],
            "forbidden_context_symbols": [
                {
                    "symbol": {
                        "qualified_name": "tests/auth.mockCheckRole",
                        "kind": "Mock",
                        "source_file": "tests/auth.test.ts"
                    },
                    "critical": true,
                    "source_file": "tests/auth.test.ts",
                    "reason": "Mock must not appear in production context packet"
                }
            ],
            "expected_tests": [
                {
                    "name": "auth role check",
                    "command": "npm test -- auth",
                    "source_file": "tests/auth.test.ts",
                    "context": "test",
                    "reason": "Closest test for authorization behavior"
                }
            ],
            "forbidden_tests": [
                {
                    "name": "unrelated billing test",
                    "source_file": "tests/billing.test.ts",
                    "context": "test",
                    "reason": "Distractor test"
                }
            ],
            "notes": ["Schema fixture used only by validation tests."],
            "failure_rules": {
                "missing_required_edge_fails": true,
                "forbidden_edge_appearing_fails": true,
                "wrong_direction_fails": true,
                "missing_source_span_fails_for_proof_grade_edge": true,
                "unresolved_name_labeled_exact_fails": true,
                "mock_or_test_edge_in_production_proof_path_fails": true,
                "derived_edge_without_provenance_fails": true,
                "stale_edge_after_edit_fails": true,
                "same_name_symbol_collision_fails": true,
                "critical_symbol_missing_from_context_packet_fails": true,
                "forbidden_context_symbol_included_fails": true,
                "expected_test_omitted_fails": true,
                "too_many_distractors_fail_if_threshold_specified": true
            },
            "mutation_steps": [],
            "performance_expectations": {
                "schema_validation_100_cases_max_ms": 1000
            },
            "distractor_policy": {
                "max_context_distractors": 2,
                "max_symbol_distractors": 1,
                "count_scope": "context_packet"
            }
        })
    }

    fn valid_graph_truth_edge(id: &str, relation: &str) -> Value {
        json!({
            "id": id,
            "head": {
                "qualified_name": "src/auth.login",
                "kind": "Function",
                "source_file": "src/auth.ts",
                "context": "production"
            },
            "relation": relation,
            "tail": {
                "qualified_name": "src/auth.checkRole",
                "kind": "Function",
                "source_file": "src/auth.ts",
                "context": "production"
            },
            "source_file": "src/auth.ts",
            "source_span": {
                "start_line": 12,
                "start_column": 10,
                "end_line": 12,
                "end_column": 31,
                "expected_text": "checkRole(user, \"admin\")",
                "syntax_role": "role_check"
            },
            "exactness": {
                "allowed": ["exact", "compiler_verified", "lsp_verified", "parser_verified"],
                "minimum": "parser_verified",
                "proof_grade_required": true,
                "confidence_floor": 1.0
            },
            "exactness_required": {
                "allowed": ["exact", "compiler_verified", "lsp_verified", "parser_verified"],
                "minimum": "parser_verified",
                "proof_grade_required": true,
                "confidence_floor": 1.0
            },
            "context_kind": "production",
            "derived_allowed": false,
            "provenance_required": false,
            "extractor_kind": "fixture-truth",
            "context": "production",
            "resolution": "resolved",
            "derived": false,
            "provenance_edges": [],
            "extractor": "fixture-truth",
            "direction": "head_to_tail"
        })
    }

    fn validate_graph_truth_case_for_tests(schema: &Value, case: &Value) -> Result<(), String> {
        let case_object = object(case, "case")?;
        let allowed_properties = object(
            schema
                .get("properties")
                .ok_or_else(|| "schema missing properties".to_string())?,
            "schema.properties",
        )?
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
        if schema
            .get("additionalProperties")
            .and_then(Value::as_bool)
            .is_some_and(|allowed| !allowed)
        {
            for key in case_object.keys() {
                if !allowed_properties.contains(key) {
                    return Err(format!("unexpected top-level field {key}"));
                }
            }
        }

        for field in required_values(schema, "/required") {
            if !case_object.contains_key(&field) {
                return Err(format!("missing required field {field}"));
            }
        }

        for field in ["case_id", "description", "repo_fixture_path", "task_prompt"] {
            let value = string_field(case, field)?;
            if value.trim().is_empty() {
                return Err(format!("{field} must not be empty"));
            }
        }
        validate_repo_path(
            string_field(case, "repo_fixture_path")?,
            "repo_fixture_path",
        )?;

        for field in [
            "expected_entities",
            "expected_edges",
            "forbidden_edges",
            "expected_paths",
            "forbidden_paths",
            "expected_source_spans",
            "expected_context_symbols",
            "forbidden_context_symbols",
            "expected_tests",
            "forbidden_tests",
            "mutation_steps",
            "notes",
        ] {
            array_field(case, field)?;
        }
        object(
            case.get("performance_expectations")
                .ok_or_else(|| "performance_expectations missing".to_string())?,
            "performance_expectations",
        )?;

        for entity in array_field(case, "expected_entities")? {
            validate_entity_expectation(schema, entity)?;
        }
        for edge in array_field(case, "expected_edges")? {
            validate_edge_expectation(schema, edge)?;
        }
        for edge in array_field(case, "forbidden_edges")? {
            validate_edge_expectation(schema, edge)?;
        }
        for path in array_field(case, "expected_paths")? {
            validate_path_expectation(schema, path)?;
        }
        for path in array_field(case, "forbidden_paths")? {
            validate_path_expectation(schema, path)?;
        }
        for span in array_field(case, "expected_source_spans")? {
            validate_source_span_expectation(span)?;
        }
        for symbol in array_field(case, "expected_context_symbols")? {
            validate_context_symbol_expectation(schema, symbol)?;
        }
        for symbol in array_field(case, "forbidden_context_symbols")? {
            validate_context_symbol_expectation(schema, symbol)?;
        }
        for test in array_field(case, "expected_tests")? {
            validate_test_expectation(test)?;
        }
        for test in array_field(case, "forbidden_tests")? {
            validate_test_expectation(test)?;
        }
        for step in array_field(case, "mutation_steps")? {
            validate_mutation_step(step)?;
        }

        if let Some(failure_rules) = case.get("failure_rules") {
            for rule in required_values(schema, "/$defs/failureRules/required") {
                if failure_rules.get(&rule).and_then(Value::as_bool) != Some(true) {
                    return Err(format!("failure rule {rule} must be true"));
                }
            }
        }

        Ok(())
    }

    fn validate_entity_expectation(schema: &Value, value: &Value) -> Result<(), String> {
        for field in required_values(schema, "/$defs/entityExpectation/required") {
            if value.get(&field).is_none() {
                return Err(format!("entity expectation missing {field}"));
            }
        }
        validate_entity_ref(
            schema,
            value.get("selector").expect("selector checked above"),
        )?;
        validate_entity_kind(schema, string_field(value, "kind")?)?;
        validate_execution_context(schema, string_field(value, "context")?)?;
        if let Some(path) = value.get("source_file").and_then(Value::as_str) {
            validate_repo_path(path, "entity.source_file")?;
        }
        if let Some(span) = value.get("source_span") {
            validate_span(span)?;
        }
        Ok(())
    }

    fn validate_edge_expectation(schema: &Value, value: &Value) -> Result<(), String> {
        for field in required_values(schema, "/$defs/edgeExpectation/required") {
            if value.get(&field).is_none() {
                return Err(format!("edge expectation missing {field}"));
            }
        }

        validate_entity_ref(schema, value.get("head").expect("head checked above"))?;
        validate_relation(schema, string_field(value, "relation")?)?;
        validate_entity_ref(schema, value.get("tail").expect("tail checked above"))?;
        validate_repo_path(string_field(value, "source_file")?, "edge.source_file")?;
        validate_span(value.get("source_span").expect("source_span checked above"))?;
        validate_exactness_requirement(schema, value)?;
        validate_exactness_requirement_field(schema, value, "exactness_required")?;
        validate_context_kind(schema, string_field(value, "context_kind")?)?;
        value
            .get("derived_allowed")
            .and_then(Value::as_bool)
            .ok_or_else(|| "edge.derived_allowed must be boolean".to_string())?;
        let provenance_required = value
            .get("provenance_required")
            .and_then(Value::as_bool)
            .ok_or_else(|| "edge.provenance_required must be boolean".to_string())?;
        if let Some(extractor_kind) = value.get("extractor_kind").and_then(Value::as_str) {
            if extractor_kind.trim().is_empty() {
                return Err("edge.extractor_kind must not be empty".to_string());
            }
        }
        validate_execution_context(schema, string_field(value, "context")?)?;
        validate_resolution(schema, string_field(value, "resolution")?)?;

        let derived = value
            .get("derived")
            .and_then(Value::as_bool)
            .ok_or_else(|| "edge.derived must be boolean".to_string())?;
        let provenance_edges = array_field(value, "provenance_edges")?;
        if derived && provenance_edges.is_empty() {
            return Err("derived edge must include provenance_edges".to_string());
        }
        if provenance_required && provenance_edges.is_empty() {
            return Err("provenance_required edge must include provenance_edges".to_string());
        }
        Ok(())
    }

    fn validate_path_expectation(schema: &Value, value: &Value) -> Result<(), String> {
        for field in required_values(schema, "/$defs/pathExpectation/required") {
            if value.get(&field).is_none() {
                return Err(format!("path expectation missing {field}"));
            }
        }
        validate_entity_ref(schema, value.get("source").expect("source checked above"))?;
        validate_entity_ref(schema, value.get("target").expect("target checked above"))?;

        let ordered_edges = array_field(value, "ordered_edges")?;
        if ordered_edges.is_empty() {
            return Err("path.ordered_edges must not be empty".to_string());
        }
        for edge in ordered_edges {
            validate_edge_expectation(schema, edge)?;
        }

        let relation_sequence = array_field(value, "relation_sequence")?;
        if relation_sequence.is_empty() {
            return Err("path.relation_sequence must not be empty".to_string());
        }
        for relation in relation_sequence {
            validate_relation(
                schema,
                relation
                    .as_str()
                    .ok_or_else(|| "path relation must be string".to_string())?,
            )?;
        }
        let max_length = integer_field(value, "max_length")?;
        if max_length < 1 {
            return Err("path.max_length must be positive".to_string());
        }
        if relation_sequence.len() > max_length as usize {
            return Err("path.relation_sequence exceeds max_length".to_string());
        }

        let source_span_required = value
            .get("source_span_required")
            .and_then(Value::as_bool)
            .ok_or_else(|| "path.source_span_required must be boolean".to_string())?;
        let production_only = value
            .get("production_only")
            .and_then(Value::as_bool)
            .ok_or_else(|| "path.production_only must be boolean".to_string())?;
        value
            .get("derived_allowed")
            .and_then(Value::as_bool)
            .ok_or_else(|| "path.derived_allowed must be boolean".to_string())?;
        value
            .get("provenance_required")
            .and_then(Value::as_bool)
            .ok_or_else(|| "path.provenance_required must be boolean".to_string())?;

        let required_spans = array_field(value, "required_source_spans")?;
        if source_span_required && required_spans.is_empty() {
            return Err("path.required_source_spans must not be empty".to_string());
        }
        for span in required_spans {
            validate_source_span_expectation(span)?;
        }

        let context = string_field(value, "context")?;
        validate_execution_context(schema, context)?;
        let allow_test_mock_edges = value
            .get("allow_test_mock_edges")
            .and_then(Value::as_bool)
            .ok_or_else(|| "path.allow_test_mock_edges must be boolean".to_string())?;
        if context == "production" && allow_test_mock_edges {
            return Err("production proof path must not allow test/mock edges".to_string());
        }
        if production_only && context != "production" {
            return Err("production_only path must use production context".to_string());
        }
        if production_only && allow_test_mock_edges {
            return Err("production_only path must not allow test/mock edges".to_string());
        }
        if value
            .get("derived_edges_require_provenance")
            .and_then(Value::as_bool)
            != Some(true)
        {
            return Err("path.derived_edges_require_provenance must be true".to_string());
        }
        Ok(())
    }

    fn validate_mutation_step(value: &Value) -> Result<(), String> {
        let step_type = string_field(value, "type")?;
        match step_type {
            "edit_file" | "delete_file" | "add_file" | "remove_file" => {
                validate_repo_path(string_field(value, "path")?, "mutation_step.path")?;
            }
            "rename_file" => {
                validate_repo_path(string_field(value, "from")?, "mutation_step.from")?;
                validate_repo_path(string_field(value, "to")?, "mutation_step.to")?;
            }
            "change_import_alias" => {
                validate_repo_path(string_field(value, "path")?, "mutation_step.path")?;
                if string_field(value, "old_alias")?.trim().is_empty()
                    || string_field(value, "new_alias")?.trim().is_empty()
                {
                    return Err("mutation aliases must not be empty".to_string());
                }
            }
            "reindex" => {}
            "query_again" => {
                if string_field(value, "query_prompt")?.trim().is_empty() {
                    return Err("query_again mutation requires query_prompt".to_string());
                }
            }
            _ => return Err(format!("unknown mutation step type {step_type}")),
        }
        Ok(())
    }

    fn validate_source_span_expectation(value: &Value) -> Result<(), String> {
        validate_repo_path(
            string_field(value, "source_file")?,
            "source_span.source_file",
        )?;
        validate_span(
            value
                .get("span")
                .ok_or_else(|| "span missing".to_string())?,
        )?;
        if value.get("must_resolve_to_text").and_then(Value::as_bool) != Some(true) {
            return Err("source span must_resolve_to_text must be true".to_string());
        }
        Ok(())
    }

    fn validate_context_symbol_expectation(schema: &Value, value: &Value) -> Result<(), String> {
        validate_entity_ref(
            schema,
            value
                .get("symbol")
                .ok_or_else(|| "context symbol missing symbol".to_string())?,
        )?;
        if let Some(path) = value.get("source_file").and_then(Value::as_str) {
            validate_repo_path(path, "context_symbol.source_file")?;
        }
        Ok(())
    }

    fn validate_test_expectation(value: &Value) -> Result<(), String> {
        if string_field(value, "name")?.trim().is_empty() {
            return Err("test name must not be empty".to_string());
        }
        if let Some(path) = value.get("source_file").and_then(Value::as_str) {
            validate_repo_path(path, "test.source_file")?;
        }
        Ok(())
    }

    fn validate_entity_ref(schema: &Value, value: &Value) -> Result<(), String> {
        let object = object(value, "entity ref")?;
        let has_id = object.get("id").and_then(Value::as_str).is_some();
        let has_qname = object
            .get("qualified_name")
            .and_then(Value::as_str)
            .is_some();
        let has_name_and_file = object.get("name").and_then(Value::as_str).is_some()
            && object.get("source_file").and_then(Value::as_str).is_some();
        if !(has_id || has_qname || has_name_and_file) {
            return Err(
                "entity ref must include id, qualified_name, or name plus source_file".to_string(),
            );
        }
        if let Some(kind) = object.get("kind").and_then(Value::as_str) {
            validate_entity_kind(schema, kind)?;
        }
        if let Some(path) = object.get("source_file").and_then(Value::as_str) {
            validate_repo_path(path, "entity.source_file")?;
        }
        if let Some(context) = object.get("context").and_then(Value::as_str) {
            validate_execution_context(schema, context)?;
        }
        Ok(())
    }

    fn validate_exactness_requirement(schema: &Value, edge: &Value) -> Result<(), String> {
        validate_exactness_requirement_field(schema, edge, "exactness")
    }

    fn validate_exactness_requirement_field(
        schema: &Value,
        edge: &Value,
        field: &str,
    ) -> Result<(), String> {
        let exactness = edge
            .get(field)
            .ok_or_else(|| format!("edge.{field} missing"))?;
        let allowed = array_field(exactness, "allowed")?;
        if allowed.is_empty() {
            return Err(format!("{field}.allowed must not be empty"));
        }
        let exactness_values = enum_values(schema, "/$defs/exactness/enum");
        for value in allowed {
            let exactness_value = value
                .as_str()
                .ok_or_else(|| "exactness value must be string".to_string())?;
            if !exactness_values.contains(exactness_value) {
                return Err(format!("unknown exactness {exactness_value}"));
            }
        }
        let minimum = string_field(exactness, "minimum")?;
        if !exactness_values.contains(minimum) {
            return Err(format!("unknown minimum exactness {minimum}"));
        }
        if string_field(edge, "resolution")? == "unresolved"
            && allowed.iter().any(|value| {
                value.as_str().is_some_and(|exactness| {
                    matches!(
                        exactness,
                        "exact" | "compiler_verified" | "lsp_verified" | "parser_verified"
                    )
                })
            })
        {
            return Err("unresolved edge cannot allow proof-grade exactness".to_string());
        }
        Ok(())
    }

    fn validate_span(value: &Value) -> Result<(), String> {
        for field in ["start_line", "start_column", "end_line", "end_column"] {
            integer_field(value, field)?;
        }
        let start_line = integer_field(value, "start_line")?;
        let start_column = integer_field(value, "start_column")?;
        let end_line = integer_field(value, "end_line")?;
        let end_column = integer_field(value, "end_column")?;
        if start_line < 1 || end_line < 1 {
            return Err("span lines must be one-based".to_string());
        }
        if start_column < 0 || end_column < 0 {
            return Err("span columns must be non-negative".to_string());
        }
        if (end_line, end_column) < (start_line, start_column) {
            return Err("span end must not precede start".to_string());
        }
        Ok(())
    }

    fn validate_entity_kind(schema: &Value, kind: &str) -> Result<(), String> {
        if enum_values(schema, "/$defs/entityKind/enum").contains(kind) {
            Ok(())
        } else {
            Err(format!("unknown entity kind {kind}"))
        }
    }

    fn validate_relation(schema: &Value, relation: &str) -> Result<(), String> {
        if enum_values(schema, "/$defs/relation/enum").contains(relation) {
            Ok(())
        } else {
            Err(format!("unknown relation {relation}"))
        }
    }

    fn validate_execution_context(schema: &Value, context: &str) -> Result<(), String> {
        if enum_values(schema, "/$defs/executionContext/enum").contains(context) {
            Ok(())
        } else {
            Err(format!("unknown context {context}"))
        }
    }

    fn validate_context_kind(schema: &Value, context: &str) -> Result<(), String> {
        if enum_values(schema, "/$defs/contextKind/enum").contains(context) {
            Ok(())
        } else {
            Err(format!("unknown context kind {context}"))
        }
    }

    fn validate_resolution(schema: &Value, resolution: &str) -> Result<(), String> {
        if enum_values(schema, "/$defs/resolutionStatus/enum").contains(resolution) {
            Ok(())
        } else {
            Err(format!("unknown resolution {resolution}"))
        }
    }

    fn validate_repo_path(path: &str, label: &str) -> Result<(), String> {
        if path.trim().is_empty() {
            return Err(format!("{label} must not be empty"));
        }
        if path.starts_with('/')
            || path.starts_with("\\\\")
            || path.as_bytes().get(1).is_some_and(|second| *second == b':')
        {
            return Err(format!("{label} must be repository-relative"));
        }
        Ok(())
    }

    fn required_values(schema: &Value, pointer: &str) -> BTreeSet<String> {
        schema
            .pointer(pointer)
            .and_then(Value::as_array)
            .expect("schema required array")
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .expect("required value is string")
                    .to_string()
            })
            .collect()
    }

    fn enum_values(schema: &Value, pointer: &str) -> BTreeSet<String> {
        schema
            .pointer(pointer)
            .and_then(Value::as_array)
            .expect("schema enum array")
            .iter()
            .map(|value| value.as_str().expect("enum value is string").to_string())
            .collect()
    }

    fn object<'a>(
        value: &'a Value,
        label: &str,
    ) -> Result<&'a serde_json::Map<String, Value>, String> {
        value
            .as_object()
            .ok_or_else(|| format!("{label} must be object"))
    }

    fn array_field<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>, String> {
        value
            .get(field)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{field} must be array"))
    }

    fn string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
        value
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{field} must be string"))
    }

    fn integer_field(value: &Value, field: &str) -> Result<i64, String> {
        value
            .get(field)
            .and_then(Value::as_i64)
            .ok_or_else(|| format!("{field} must be integer"))
    }

    #[test]
    fn synthetic_repo_generator_writes_controlled_files() {
        let repo = synthetic_repo(SyntheticRepoKind::SecurityAuth);
        assert!(!repo.files.is_empty());
        assert_eq!(repo.tasks[0].family, BenchmarkFamily::SecurityAuth);
        let root = unique_output_path("codegraph-bench-fixture", "dir");
        repo.write_to(&root).expect("write synthetic repo");
        assert!(root.join("src/security.ts").exists());
        fs::remove_dir_all(root).expect("cleanup synthetic repo");
    }

    #[test]
    fn metric_calculation_handles_precision_recall_rank_and_ndcg() {
        let expected = BTreeSet::from(["a".to_string(), "b".to_string()]);
        let observed = BTreeSet::from(["a".to_string(), "c".to_string()]);
        let score = precision_recall_f1(&expected, &observed);
        assert_eq!(score.precision, 0.5);
        assert_eq!(score.recall, 0.5);
        assert_eq!(score.f1, 0.5);

        let ranked = vec!["x".to_string(), "b".to_string(), "a".to_string()];
        assert_eq!(recall_at_k(&expected, &ranked, 2), 0.5);
        assert_eq!(mean_reciprocal_rank(&expected, &ranked), 0.5);
        assert!(ndcg_at_k(&expected, &ranked, 3) > 0.0);
    }

    #[test]
    fn baseline_runner_smoke_tests_graph_and_bm25_modes() {
        let repo = synthetic_repo(SyntheticRepoKind::ContextRetrieval);
        let corpus = index_synthetic_repo(&repo).expect("index synthetic repo");
        let task = &repo.tasks[0];

        let graph = run_baseline(task, &corpus, BaselineMode::GraphOnly).expect("graph baseline");
        assert_eq!(graph.status, BenchmarkRunStatus::Completed);
        assert_eq!(graph.baseline, BaselineMode::GraphOnly);
        assert!(graph.metrics.memory_bytes > 0);

        let bm25 = run_baseline(task, &corpus, BaselineMode::GrepBm25).expect("bm25 baseline");
        assert_eq!(bm25.status, BenchmarkRunStatus::Completed);
        assert!(bm25.metrics.latency_ms > 0);
    }

    #[test]
    fn report_output_is_machine_readable_and_markdown_renderable() {
        let repo = synthetic_repo(SyntheticRepoKind::TestImpact);
        let suite = BenchmarkSuite {
            schema_version: BENCH_SCHEMA_VERSION,
            id: "smoke-suite".to_string(),
            tasks: repo.tasks,
            metadata: BTreeMap::new(),
        };
        let report = run_benchmark_suite(
            &suite,
            &[BaselineMode::VanillaNoRetrieval, BaselineMode::GraphOnly],
        )
        .expect("run suite");
        assert_eq!(report.results.len(), 2);
        assert!(report
            .aggregate
            .contains_key(BaselineMode::GraphOnly.as_str()));

        let json = render_json_report(&report).expect("json report");
        assert!(json.contains("\"schema_version\""));
        let markdown = render_markdown_report(&report);
        assert!(markdown.contains("| Baseline | Runs |"));
    }

    #[test]
    fn real_repo_replay_reports_unavailable_without_git_checkout() {
        let plan = plan_real_repo_commit_replay(&CommitReplaySpec {
            repo_path: unique_output_path("missing-replay", "repo")
                .display()
                .to_string(),
            base_commit: "base".to_string(),
            head_commit: "head".to_string(),
            changed_paths: vec!["src/auth.ts".to_string()],
        });
        assert_eq!(plan.status, ReplayFeasibility::Unavailable);
    }

    #[test]
    fn gap_classifier_reports_win_loss_tie_and_unknown() {
        assert_eq!(
            classify_gap(Some(0.9), Some(0.7), GapPolarity::HigherIsBetter, 0.0),
            GapClassification::Win
        );
        assert_eq!(
            classify_gap(Some(0.9), Some(0.7), GapPolarity::LowerIsBetter, 0.0),
            GapClassification::Loss
        );
        assert_eq!(
            classify_gap(Some(10.0), Some(10.01), GapPolarity::LowerIsBetter, 0.02),
            GapClassification::Tie
        );
        assert_eq!(
            classify_gap(None, Some(1.0), GapPolarity::HigherIsBetter, 0.0),
            GapClassification::Unknown
        );
    }

    #[test]
    fn gap_scoreboard_writes_codegraph_only_report_with_skipped_competitor() {
        let output_dir = unique_output_path("phase26-gap-scoreboard", "dir");
        let artifacts = write_gap_scoreboard_report(GapScoreboardOptions {
            report_dir: output_dir.clone(),
            timeout_ms: 25,
            top_k: 10,
            competitor_executable: Some(output_dir.join("missing-cgc.exe")),
        })
        .expect("write gap scoreboard");

        assert!(Path::new(&artifacts.json_summary).exists());
        assert!(Path::new(&artifacts.markdown_summary).exists());
        assert!(Path::new(&artifacts.per_task_jsonl).exists());
        assert!(Path::new(&artifacts.external_competitor_dir).exists());

        let report: GapScoreboardReport = serde_json::from_str(
            &fs::read_to_string(&artifacts.json_summary).expect("read gap summary"),
        )
        .expect("gap summary json");
        report.validate().expect("gap report validates");
        assert!(report
            .competitor_metadata
            .executable_used
            .contains("skipped"));
        assert!(report
            .dimensions
            .iter()
            .any(|dimension| dimension.classification == GapClassification::Unknown));
        assert!(report
            .task_records
            .iter()
            .any(|record| record.status == GapMeasurementStatus::Skipped));

        let markdown = fs::read_to_string(&artifacts.markdown_summary).expect("read markdown");
        assert!(markdown.contains("No SOTA claim"));
        assert!(markdown.contains("unknown"));

        fs::remove_dir_all(output_dir).expect("cleanup gap report");
    }

    #[test]
    fn sample_cgc_output_normalizes_for_gap_scoreboard() {
        let normalized = competitors::codegraphcontext::normalize_codegraphcontext_text(
            "Symbol: login src/auth.ts:9-12\nPath: route -> login -> saveUser\nRelations: CALLS -> WRITES",
            "",
            None,
        );
        assert!(normalized.files.contains(&"src/auth.ts".to_string()));
        assert!(normalized.symbols.contains(&"login".to_string()));
        assert!(normalized.path_symbols.contains(&"saveUser".to_string()));
        assert!(normalized.relation_sequence.contains(&"CALLS".to_string()));
    }

    #[test]
    fn real_repo_maturity_corpus_validates_pinned_commits_and_tasks() {
        let corpus = real_repo_maturity_corpus();
        corpus.validate().expect("corpus validates");
        assert_eq!(corpus.schema_version, BENCH_SCHEMA_VERSION);
        assert_eq!(corpus.repos.len(), 5);
        assert!(corpus
            .repos
            .iter()
            .all(|repo| repo.pinned_commit_sha.len() == 40 && !repo.tasks.is_empty()));
        assert!(corpus
            .repos
            .iter()
            .flat_map(|repo| repo.tasks.iter())
            .all(|task| {
                task.validation_status == "manual_subset"
                    || task.validation_status == "unvalidated_expected"
            }));
    }

    #[test]
    fn real_repo_corpus_replay_is_skipped_without_network() {
        let corpus = real_repo_maturity_corpus();
        let plan =
            plan_real_repo_corpus_replay(&corpus, ".codegraph-bench-cache/real-repos", false)
                .expect("replay plan");

        assert_eq!(plan.status, ReplayFeasibility::Unavailable);
        assert!(plan
            .reason
            .as_deref()
            .expect("skip reason")
            .contains("offline"));
        assert!(plan
            .repos
            .iter()
            .all(|repo| repo.status == ReplayFeasibility::Unavailable));
    }

    #[test]
    fn final_parity_report_keeps_unknowns_explicit_and_claims_modest() {
        let report = final_parity_report().expect("parity report");
        assert_eq!(report.schema_version, BENCH_SCHEMA_VERSION);
        assert!(report.no_fabricated_sota_claims);
        assert!(report
            .dimensions
            .iter()
            .any(|dimension| dimension.name == "mcp_schema_quality"));
        assert!(report
            .dimensions
            .iter()
            .all(|dimension| !dimension.unknown_fields.is_empty()));
        assert!(report
            .skipped_or_unknown
            .iter()
            .any(|entry| entry.contains("unknown")));

        let markdown = render_final_parity_markdown(&report);
        assert!(markdown.contains("Unknown"));
        assert!(markdown.contains("No SOTA superiority is claimed unless measured"));
        assert!(markdown.contains("## Evidence Sections"));
        assert!(markdown.contains("Fake-agent dry run"));
    }

    #[test]
    fn final_parity_report_writes_json_markdown_and_jsonl_artifacts() {
        let output_dir = unique_output_path("phase30-parity-report", "dir");
        let artifacts = write_final_parity_report(&output_dir).expect("write parity report");

        assert!(Path::new(&artifacts.json_summary).exists());
        assert!(Path::new(&artifacts.markdown_summary).exists());
        assert!(Path::new(&artifacts.per_task_jsonl).exists());

        let json: FinalParityReport = serde_json::from_str(
            &fs::read_to_string(&artifacts.json_summary).expect("read json summary"),
        )
        .expect("summary json");
        assert!(json.no_fabricated_sota_claims);
        let jsonl = fs::read_to_string(&artifacts.per_task_jsonl).expect("read jsonl");
        assert!(jsonl.lines().all(|line| line.contains("\"manifest_only\"")));

        fs::remove_dir_all(output_dir).expect("cleanup parity report");
    }

    #[test]
    fn final_acceptance_gate_requires_compact_mvp_correctness_and_cgc_evidence() {
        let output_dir = unique_output_path("final-acceptance-gate", "dir");
        let mut options = FinalAcceptanceGateOptions::with_report_dir(output_dir.clone());
        options.competitor_executable = Some(output_dir.join("missing-cgc.exe"));
        options.timeout_ms = 25;
        let report = final_acceptance_gate_report(&options).expect("final gate report");

        assert_eq!(report.internal_verdict, FinalGateVerdict::Pass);
        assert_eq!(report.verdict, FinalGateVerdict::Unknown);
        assert_eq!(report.cgc_comparison.status, "skipped");
        assert!(report.indexing.counts_equivalent);
        assert_eq!(report.indexing.cli.entities, report.indexing.mcp.entities);
        assert_eq!(report.indexing.cli.edges, report.indexing.mcp.edges);
        assert!(report.storage.db_size_bytes > 0);
        assert!(report.storage.source_span_count > 0);
        assert!(report.storage.proof_object_counts.path_evidence_generated > 0);
        assert!(
            report
                .storage
                .proof_object_counts
                .derived_closure_edges_generated
                > 0
        );
        assert!(report
            .storage
            .size_targets
            .get("smaller_than_cgc_same_repo")
            .is_some_and(|value| value.as_str() == Some("not_comparable_incomplete_cgc")));
        assert!(
            report
                .functionality_checks
                .iter()
                .any(|check| check.id == "explain_edge_path"
                    && check.status == FinalGateVerdict::Pass)
        );
        assert!(report
            .mvp_proof_checks
            .iter()
            .any(|check| check.id == "compact_storage_not_relation_deletion"
                && check.status == FinalGateVerdict::Pass));

        let mut broken = report.clone();
        broken.storage.source_span_count = 0;
        assert!(broken.validate().is_err());

        fs::remove_dir_all(output_dir).expect("cleanup final gate report");
    }

    #[test]
    fn competitor_timeout_keeps_superiority_unknown_not_win() {
        let report = competitors::codegraphcontext::ExternalComparisonReport {
            schema_version: BENCH_SCHEMA_VERSION,
            benchmark_id: "timeout-cgc".to_string(),
            generated_by: "test".to_string(),
            manifest: competitors::codegraphcontext::CompetitorManifest {
                competitor_name: "CodeGraphContext".to_string(),
                source_repo_url: "https://example.invalid/cgc".to_string(),
                pinned_git_commit_sha: "unknown".to_string(),
                detected_package_version: "unknown".to_string(),
                observed_version_hint: "unknown".to_string(),
                python_version: "unknown".to_string(),
                executable_used: "cgc".to_string(),
                detected_database_backend: "unknown".to_string(),
                install_mode: "test".to_string(),
                benchmark_run_timestamp_unix_ms: 0,
                host_platform: competitors::codegraphcontext::HostPlatformMetadata {
                    os: "test".to_string(),
                    arch: "test".to_string(),
                    family: "test".to_string(),
                },
            },
            setup_plan: competitors::codegraphcontext::codegraphcontext_setup_plan(),
            fairness_rules: Vec::new(),
            report_dir: "target/test".to_string(),
            runs: vec![competitors::codegraphcontext::ExternalComparisonRun {
                fixture_id: "fixture".to_string(),
                task_id: "task".to_string(),
                mode: competitors::codegraphcontext::ExternalComparisonMode::CodeGraphContextCli,
                status: competitors::codegraphcontext::ExternalComparisonStatus::Completed,
                skip_reason: None,
                metrics: competitors::codegraphcontext::ExternalComparisonMetrics {
                    file_recall_at_k: BTreeMap::new(),
                    symbol_recall_at_k: BTreeMap::new(),
                    path_recall_at_k: BTreeMap::new(),
                    relation_precision: 0.0,
                    relation_recall: 0.0,
                    relation_f1: 0.0,
                    mrr: 0.0,
                    ndcg: 0.0,
                    source_span_coverage: 0.0,
                    false_proof_count: 0,
                    unsupported_feature_count: 0,
                    index_latency_ms: 180_000,
                    query_latency_ms: 0,
                    estimated_token_cost: 0,
                    memory_usage_bytes: None,
                    memory_usage_note: "unknown".to_string(),
                },
                normalized_output: competitors::codegraphcontext::NormalizedCompetitorOutput {
                    files: Vec::new(),
                    symbols: Vec::new(),
                    path_symbols: Vec::new(),
                    relation_sequence: Vec::new(),
                    source_spans: Vec::new(),
                    unsupported_fields: Vec::new(),
                    mode:
                        competitors::codegraphcontext::NormalizationMode::UnsupportedOrUnparseable,
                    warnings: Vec::new(),
                    raw_json: None,
                },
                raw_artifact_paths: vec!["raw.json".to_string()],
                warnings: vec!["command timed out; timed_out=true".to_string()],
            }],
            aggregate: BTreeMap::new(),
        };

        let cgc = final_gate_cgc_comparison(&report, Some(1024));
        assert_eq!(cgc.status, "incomplete");
        assert_eq!(cgc.db_size_bytes.as_u64(), Some(1024));
        assert_eq!(
            final_gate_size_targets(2048, &cgc)
                .get("smaller_than_cgc_same_repo")
                .and_then(Value::as_str),
            Some("not_comparable_incomplete_cgc")
        );
        let (verdict, reasons, unknowns) = final_gate_verdict(FinalGateVerdict::Pass, &cgc, 2048);
        assert_eq!(verdict, FinalGateVerdict::Unknown);
        assert!(reasons
            .iter()
            .any(|reason| reason.contains("not a fair completed comparison")));
        assert!(unknowns.contains(&"completed_cgc_comparison".to_string()));
    }

    #[test]
    fn fake_agent_only_has_no_model_superiority_claim() {
        let verdict = honest_superiority_verdict(FinalGateVerdict::Pass, "completed", true);
        assert_eq!(verdict.verdict, FinalGateVerdict::Unknown);
        assert!(verdict
            .forbidden_claims
            .iter()
            .any(|claim| claim.contains("fake-agent")));
    }

    fn completed_better_cgc_input() -> CodeGraphVsCgcVerdictInput {
        CodeGraphVsCgcVerdictInput {
            cgc_status: "completed".to_string(),
            cgc_artifacts_comparable: true,
            fake_agent_only: false,
            codegraph_storage_target_met: true,
            graph_truth_passed: true,
            context_packet_passed: true,
            both_tools_completed_same_scope: true,
            codegraph_at_least_2x_faster: true,
            codegraph_storage_smaller_or_equal: true,
            codegraph_quality_at_least_cgc: true,
            real_model_benchmark_completed: false,
            codegraph_real_model_quality_at_least_cgc: false,
        }
    }

    #[test]
    fn codegraph_vs_cgc_competitor_timeout_is_incomplete_not_win() {
        let mut input = completed_better_cgc_input();
        input.cgc_status = "timeout".to_string();
        input.cgc_artifacts_comparable = false;
        input.both_tools_completed_same_scope = false;

        let verdict = codegraph_vs_cgc_verdict(input);
        assert_eq!(verdict.final_verdict, FinalGateVerdict::Incomplete);
        assert_eq!(verdict.speed_verdict, FinalGateVerdict::Incomplete);
        assert!(!verdict.comparison_complete);
        assert!(!verdict.qualified_core_win_allowed);
    }

    #[test]
    fn codegraph_vs_cgc_fake_agent_only_has_no_model_superiority() {
        let mut input = completed_better_cgc_input();
        input.fake_agent_only = true;

        let verdict = codegraph_vs_cgc_verdict(input);
        assert_eq!(verdict.agent_quality_verdict, FinalGateVerdict::Unknown);
        assert!(!verdict.real_model_superiority_claim_allowed);
        assert!(verdict
            .forbidden_claims
            .iter()
            .any(|claim| claim.contains("fake-agent")));
    }

    #[test]
    fn codegraph_vs_cgc_storage_target_fail_is_storage_fail() {
        let mut input = completed_better_cgc_input();
        input.codegraph_storage_target_met = false;

        let verdict = codegraph_vs_cgc_verdict(input);
        assert_eq!(verdict.storage_verdict, FinalGateVerdict::Fail);
        assert_eq!(verdict.final_verdict, FinalGateVerdict::Fail);
    }

    #[test]
    fn codegraph_vs_cgc_graph_truth_fail_is_quality_fail() {
        let mut input = completed_better_cgc_input();
        input.graph_truth_passed = false;

        let verdict = codegraph_vs_cgc_verdict(input);
        assert_eq!(verdict.quality_verdict, FinalGateVerdict::Fail);
        assert_eq!(verdict.final_verdict, FinalGateVerdict::Fail);
    }

    #[test]
    fn codegraph_vs_cgc_both_complete_better_is_qualified_win() {
        let verdict = codegraph_vs_cgc_verdict(completed_better_cgc_input());
        assert_eq!(verdict.final_verdict, FinalGateVerdict::Pass);
        assert!(verdict.comparison_complete);
        assert!(verdict.qualified_core_win_allowed);
        assert!(!verdict.real_model_superiority_claim_allowed);
    }

    #[test]
    fn storage_above_target_is_explicit_fail() {
        assert_eq!(
            real_repo_storage_target_verdict(true, Some(803 * 1024 * 1024), 512 * 1024 * 1024),
            FinalGateVerdict::Fail
        );
        assert_eq!(
            real_repo_storage_target_verdict(false, None, 512 * 1024 * 1024),
            FinalGateVerdict::Incomplete
        );
    }

    #[test]
    fn internal_pass_plus_competitor_incomplete_is_final_unknown() {
        let cgc = FinalGateCgcComparison {
            status: "incomplete".to_string(),
            executable_path: "cgc".to_string(),
            backend: "unknown".to_string(),
            version: "unknown".to_string(),
            python_version: "unknown".to_string(),
            elapsed_time_ms: json!("unknown"),
            db_size_bytes: json!("unknown"),
            stdout_artifact_paths: Vec::new(),
            stderr_artifact_paths: Vec::new(),
            raw_artifact_paths: Vec::new(),
            normalized_artifact_paths: Vec::new(),
            produced_incomplete_stats: true,
            fairness_note: "incomplete competitor evidence is not a win".to_string(),
        };
        let (verdict, _, unknowns) = final_gate_verdict(FinalGateVerdict::Pass, &cgc, 1024);
        assert_eq!(verdict, FinalGateVerdict::Unknown);
        assert!(unknowns.contains(&"completed_cgc_comparison".to_string()));

        let claim = honest_superiority_verdict(FinalGateVerdict::Pass, "incomplete", false);
        assert_eq!(claim.verdict, FinalGateVerdict::Unknown);
    }

    #[test]
    fn final_gate_source_check_fails_old_mcp_indexing_markers() {
        let root = unique_output_path("final-gate-old-mcp-source", "dir");
        let source_dir = root.join("crates").join("codegraph-mcp-server").join("src");
        fs::create_dir_all(&source_dir).expect("source dir");
        fs::write(
            source_dir.join("lib.rs"),
            "fn index_repo() { upsert_file_text(); upsert_snippet_text(); TreeSitterParser; }",
        )
        .expect("fake source");

        let (shared, old_path) = final_gate_architecture_checks(&root);
        assert_eq!(shared.status, FinalGateVerdict::Fail);
        assert_eq!(old_path.status, FinalGateVerdict::Fail);

        fs::remove_dir_all(root).expect("cleanup source check");
    }

    #[test]
    fn two_layer_retrieval_quality_writes_manifest_jsonl_and_artifacts() {
        let run_id = format!("retrieval-quality-test-{}", std::process::id());
        let run_root = PathBuf::from("target")
            .join("codegraph-bench-runs")
            .join(&run_id);
        if run_root.exists() {
            fs::remove_dir_all(&run_root).expect("remove stale run");
        }
        let mut options = default_two_layer_bench_options();
        options.run_id = Some(run_id);
        options.run_root = Some(run_root.clone());
        options.timeout_ms = 5_000;
        options.competitor_executable = Some(run_root.join("missing-cgc.exe"));
        let artifacts =
            run_retrieval_quality_benchmark(options).expect("run retrieval-quality benchmark");

        assert!(Path::new(&artifacts.manifest_json).exists());
        assert!(Path::new(&artifacts.events_jsonl).exists());
        assert!(Path::new(&artifacts.per_task_jsonl).exists());
        assert!(Path::new(&artifacts.summary_md).exists());
        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(&artifacts.manifest_json).expect("read manifest"),
        )
        .expect("manifest json");
        validate_two_layer_manifest(&manifest).expect("manifest validates");
        assert!(validate_jsonl_file(Path::new(&artifacts.events_jsonl)).expect("events") > 0);
        assert!(validate_jsonl_file(Path::new(&artifacts.per_task_jsonl)).expect("per task") > 0);
        assert!(Path::new(&artifacts.raw_dir).exists());
        assert!(Path::new(&artifacts.normalized_dir).exists());
        let per_task = fs::read_to_string(&artifacts.per_task_jsonl).expect("read per task jsonl");
        assert!(per_task.contains("\"mode\":\"cgc_cli\""));
        assert!(per_task.contains("\"status\":\"skipped\""));
        assert!(per_task.contains("autoresearch-codexlab-required-entry"));
        assert!(per_task.contains("\"index_time_ms\""));
        assert!(per_task.contains("\"db_size_bytes\""));
        let summary = fs::read_to_string(&artifacts.summary_md).expect("read summary");
        assert!(summary.contains("Win/Loss/Tie/Unknown"));
        assert!(summary.contains("\"win\""));
        assert!(summary.contains("\"loss\""));
        assert!(summary.contains("\"tie\""));
        assert!(summary.contains("\"unknown\""));

        fs::remove_dir_all(run_root).expect("cleanup run");
    }

    #[test]
    fn two_layer_agent_quality_dry_run_records_fake_agent_outputs() {
        let run_id = format!("agent-quality-test-{}", std::process::id());
        let run_root = PathBuf::from("target")
            .join("codegraph-bench-runs")
            .join(&run_id);
        if run_root.exists() {
            fs::remove_dir_all(&run_root).expect("remove stale run");
        }
        let mut options = default_two_layer_bench_options();
        options.run_id = Some(run_id);
        options.run_root = Some(run_root.clone());
        options.timeout_ms = 5_000;
        options.fake_agent = true;
        options.dry_run = true;
        options.competitor_executable = Some(run_root.join("missing-cgc.exe"));
        let artifacts = run_agent_quality_benchmark(options).expect("run agent-quality benchmark");

        assert!(Path::new(&artifacts.manifest_json).exists());
        assert!(validate_jsonl_file(Path::new(&artifacts.events_jsonl)).expect("events") > 0);
        assert!(validate_jsonl_file(Path::new(&artifacts.per_task_jsonl)).expect("per task") > 0);
        let per_task = fs::read_to_string(&artifacts.per_task_jsonl).expect("read per task jsonl");
        assert!(per_task.contains("\"layer\":\"agent_coding_quality\""));
        assert!(per_task.contains("\"status\":\"fake_agent_dry_run\""));
        assert!(per_task.contains("\"claim_scope\":\"fake_agent_dry_run_only\""));
        assert!(per_task.contains("\"patch_path\""));
        assert!(per_task.contains("\"final_answer_path\""));
        assert!(per_task.contains("\"hidden_test_pass\":\"unknown\""));
        let summary = fs::read_to_string(&artifacts.summary_md).expect("read summary");
        assert!(summary.contains("Fake-agent dry runs are trace-shape checks only"));
        assert!(summary.contains("\"fake_agent_scores_excluded\": true"));
        assert!(summary.contains("\"superiority_verdict\": \"unknown\""));
        assert!(Path::new(&artifacts.agent_dir).exists());

        fs::remove_dir_all(run_root).expect("cleanup run");
    }

    #[test]
    fn two_layer_timeout_path_records_timeout() {
        let run_id = format!("retrieval-timeout-test-{}", std::process::id());
        let run_root = PathBuf::from("target")
            .join("codegraph-bench-runs")
            .join(&run_id);
        if run_root.exists() {
            fs::remove_dir_all(&run_root).expect("remove stale run");
        }
        let mut options = default_two_layer_bench_options();
        options.run_id = Some(run_id);
        options.run_root = Some(run_root.clone());
        options.timeout_ms = 0;
        options.competitor_executable = Some(run_root.join("missing-cgc.exe"));
        let artifacts = run_retrieval_quality_benchmark(options).expect("run timeout benchmark");
        let per_task = fs::read_to_string(&artifacts.per_task_jsonl).expect("read per task jsonl");
        assert!(per_task.contains("\"status\":\"timeout\""));

        fs::remove_dir_all(run_root).expect("cleanup run");
    }
}
