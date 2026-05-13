//! Retrieval-stage ablation benchmark over graph-truth fixtures.
//!
//! This is measurement scaffolding only. It keeps Stage 0, Stage 1, Stage 2,
//! exact graph verification, and final context-packet work separately visible.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::Instant,
};

use codegraph_core::{ContextPacket, Edge, Entity, EntityKind, RelationKind};
use codegraph_index::{index_repo_to_db, UNBOUNDED_STORE_READ_LIMIT};
use codegraph_query::{
    extract_prompt_seeds, ExactGraphQueryEngine, GraphPath, QueryLimits, RetrievalDocument,
    RetrievalFunnel, RetrievalFunnelConfig, RetrievalFunnelRequest,
};
use codegraph_store::{GraphStore, SqliteGraphStore};
use codegraph_vector::{
    BinarySignature, BinaryVectorIndex, CompressedVectorReranker, DeterministicCompressedReranker,
    InMemoryBinaryVectorIndex, RerankCandidate, RerankConfig, RerankQuery,
};
use serde::{Deserialize, Serialize};

use crate::{unique_output_path, BenchResult, BenchmarkError, MetricScore};

const DEFAULT_TOP_K: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetrievalAblationOptions {
    pub cases: PathBuf,
    pub fixture_root: PathBuf,
    pub out_json: PathBuf,
    pub out_md: PathBuf,
    pub top_k: usize,
    pub modes: Vec<RetrievalAblationMode>,
}

pub fn default_retrieval_ablation_options() -> RetrievalAblationOptions {
    RetrievalAblationOptions {
        cases: PathBuf::from("benchmarks")
            .join("graph_truth")
            .join("fixtures"),
        fixture_root: PathBuf::from("."),
        out_json: PathBuf::from("reports")
            .join("audit")
            .join("artifacts")
            .join("10_stage_ablation.json"),
        out_md: PathBuf::from("reports")
            .join("audit")
            .join("artifacts")
            .join("10_stage_ablation.md"),
        top_k: DEFAULT_TOP_K,
        modes: Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalAblationMode {
    Stage0ExactOnly,
    Stage1BinaryOnly,
    Stage2Int8PqOnly,
    Stage0PlusStage1,
    Stage0PlusStage1PlusStage2,
    GraphVerificationOnly,
    FullContextPacket,
}

impl RetrievalAblationMode {
    pub const ALL: [Self; 7] = [
        Self::Stage0ExactOnly,
        Self::Stage1BinaryOnly,
        Self::Stage2Int8PqOnly,
        Self::Stage0PlusStage1,
        Self::Stage0PlusStage1PlusStage2,
        Self::GraphVerificationOnly,
        Self::FullContextPacket,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stage0ExactOnly => "stage0_exact_only",
            Self::Stage1BinaryOnly => "stage1_binary_only",
            Self::Stage2Int8PqOnly => "stage2_int8_pq_only",
            Self::Stage0PlusStage1 => "stage0_plus_stage1",
            Self::Stage0PlusStage1PlusStage2 => "stage0_plus_stage1_plus_stage2",
            Self::GraphVerificationOnly => "graph_verification_only",
            Self::FullContextPacket => "full_context_packet",
        }
    }

    pub const fn uses_stage0(self) -> bool {
        matches!(
            self,
            Self::Stage0ExactOnly
                | Self::Stage0PlusStage1
                | Self::Stage0PlusStage1PlusStage2
                | Self::GraphVerificationOnly
                | Self::FullContextPacket
        )
    }

    pub const fn uses_stage1(self) -> bool {
        matches!(
            self,
            Self::Stage1BinaryOnly
                | Self::Stage0PlusStage1
                | Self::Stage0PlusStage1PlusStage2
                | Self::FullContextPacket
        )
    }

    pub const fn uses_stage2(self) -> bool {
        matches!(
            self,
            Self::Stage2Int8PqOnly | Self::Stage0PlusStage1PlusStage2 | Self::FullContextPacket
        )
    }

    pub const fn uses_graph_verification(self) -> bool {
        matches!(self, Self::GraphVerificationOnly | Self::FullContextPacket)
    }

    pub const fn uses_context_packet(self) -> bool {
        matches!(self, Self::FullContextPacket)
    }
}

impl FromStr for RetrievalAblationMode {
    type Err = BenchmarkError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().replace('-', "_").to_ascii_lowercase().as_str() {
            "stage0_exact_only" | "stage0" | "exact_only" => Ok(Self::Stage0ExactOnly),
            "stage1_binary_only" | "stage1" | "binary_only" => Ok(Self::Stage1BinaryOnly),
            "stage2_int8_pq_only" | "stage2" | "int8_pq_only" | "pq_only" => {
                Ok(Self::Stage2Int8PqOnly)
            }
            "stage0_plus_stage1" | "stage0_stage1" => Ok(Self::Stage0PlusStage1),
            "stage0_plus_stage1_plus_stage2" | "stage0_stage1_stage2" | "vector_funnel" => {
                Ok(Self::Stage0PlusStage1PlusStage2)
            }
            "graph_verification_only" | "graph_only" | "stage3" => Ok(Self::GraphVerificationOnly),
            "full_context_packet" | "context_packet" | "full" | "stage4" => {
                Ok(Self::FullContextPacket)
            }
            other => Err(BenchmarkError::Validation(format!(
                "unknown retrieval-ablation mode: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalAblationReport {
    pub schema_version: u32,
    pub benchmark: String,
    pub source_of_truth: String,
    pub status: String,
    pub cases_path: String,
    pub fixture_root: String,
    pub top_k: usize,
    pub modes: Vec<RetrievalAblationModeSummary>,
    pub case_results: Vec<RetrievalAblationCaseResult>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalAblationModeSummary {
    pub mode: String,
    pub cases: usize,
    pub file_recall_at_k_mean: f64,
    pub symbol_recall_at_k_mean: f64,
    pub path_recall_at_k_mean: f64,
    pub relation_precision_mean: Option<f64>,
    pub relation_recall_mean: Option<f64>,
    pub relation_f1_mean: Option<f64>,
    pub false_positive_rate_mean: f64,
    pub forbidden_edge_violations: usize,
    pub forbidden_path_violations: usize,
    pub query_latency_p50_ms: u64,
    pub query_latency_p95_ms: u64,
    pub memory_bytes_estimate_mean: u64,
    pub index_size_bytes_mean: u64,
    pub stage0_candidate_count_mean: f64,
    pub stage1_candidate_count_mean: f64,
    pub stage2_candidate_count_mean: f64,
    pub graph_verified_candidate_count_mean: f64,
    pub context_packet_symbol_count_mean: f64,
    pub proof_grade_path_success_claimed: bool,
    pub stage0_exact_contribution_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalAblationCaseResult {
    pub case_id: String,
    pub description: String,
    pub repo_fixture_path: String,
    pub files_indexed: u64,
    pub entities_indexed: u64,
    pub edges_indexed: u64,
    pub index_size_bytes: u64,
    pub modes: Vec<RetrievalAblationModeResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalAblationModeResult {
    pub mode: String,
    pub file_recall_at_k: f64,
    pub symbol_recall_at_k: f64,
    pub path_recall_at_k: f64,
    pub relation_f1: Option<MetricScore>,
    pub false_positive_rate: f64,
    pub false_positive_count: usize,
    pub forbidden_edge_violations: usize,
    pub forbidden_path_violations: usize,
    pub query_latency_ms: u64,
    pub memory_bytes_estimate: u64,
    pub index_size_bytes: u64,
    pub candidate_counts: AblationCandidateCounts,
    pub stage0_exact_file_recall_at_k: Option<f64>,
    pub stage0_exact_symbol_recall_at_k: Option<f64>,
    pub proof_grade_path_success_claimed: bool,
    pub retrieved_files: Vec<String>,
    pub retrieved_symbols: Vec<String>,
    pub retrieved_paths: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AblationCandidateCounts {
    pub corpus_documents: usize,
    pub stage0_candidates: usize,
    pub stage0_exact_seed_candidates: usize,
    pub stage1_candidates: usize,
    pub stage2_candidates: usize,
    pub graph_verification_inputs: usize,
    pub graph_verified_candidates: usize,
    pub graph_verified_paths: usize,
    pub context_packet_symbols: usize,
    pub context_packet_paths: usize,
}

#[derive(Debug)]
struct IndexedAblationRepo {
    repo_root: PathBuf,
    entities: Vec<Entity>,
    edges: Vec<Edge>,
    sources: BTreeMap<String, String>,
    documents: Vec<RetrievalDocument>,
    index_size_bytes: u64,
}

#[derive(Debug, Clone)]
struct Stage0Output {
    candidates: Vec<String>,
    exact_seed_candidates: Vec<String>,
    candidate_documents: Vec<RetrievalDocument>,
}

#[derive(Debug, Clone)]
struct ModeObservation {
    candidate_ids: Vec<String>,
    stage0: Option<Stage0Output>,
    stage1_ids: Vec<String>,
    stage2_ids: Vec<String>,
    graph_inputs: Vec<String>,
    graph_paths: Vec<GraphPath>,
    context_packet: Option<ContextPacket>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct AblationCase {
    case_id: String,
    description: String,
    repo_fixture_path: String,
    task_prompt: String,
    #[serde(default)]
    expected_entities: Vec<EntityExpectation>,
    #[serde(default)]
    expected_edges: Vec<EdgeExpectation>,
    #[serde(default)]
    forbidden_edges: Vec<EdgeExpectation>,
    #[serde(default)]
    expected_paths: Vec<PathExpectation>,
    #[serde(default)]
    forbidden_paths: Vec<PathExpectation>,
    #[serde(default)]
    expected_context_symbols: Vec<ContextSymbolExpectation>,
    #[serde(default)]
    forbidden_context_symbols: Vec<ContextSymbolExpectation>,
    #[serde(default)]
    expected_tests: Vec<TestExpectation>,
    #[serde(default)]
    forbidden_tests: Vec<TestExpectation>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct EntityExpectation {
    selector: EntityRef,
    #[serde(default)]
    source_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct EdgeExpectation {
    head: EntityRef,
    relation: RelationKind,
    tail: EntityRef,
    source_file: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct PathExpectation {
    #[serde(default)]
    path_id: Option<String>,
    source: EntityRef,
    target: EntityRef,
    ordered_edges: Vec<EdgeExpectation>,
    relation_sequence: Vec<RelationKind>,
    max_length: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct ContextSymbolExpectation {
    symbol: EntityRef,
    #[serde(default)]
    source_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct TestExpectation {
    name: String,
    #[serde(default)]
    source_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct EntityRef {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    qualified_name: Option<String>,
    #[serde(default)]
    kind: Option<EntityKind>,
    #[serde(default)]
    source_file: Option<String>,
}

pub fn write_retrieval_ablation_report(
    options: RetrievalAblationOptions,
) -> BenchResult<RetrievalAblationReport> {
    let report = run_retrieval_ablation(&options)?;
    write_json(&options.out_json, &report)?;
    write_text(
        &options.out_md,
        &render_retrieval_ablation_markdown(&report),
    )?;
    Ok(report)
}

pub fn run_retrieval_ablation(
    options: &RetrievalAblationOptions,
) -> BenchResult<RetrievalAblationReport> {
    if options.top_k == 0 {
        return Err(BenchmarkError::Validation(
            "retrieval-ablation --top-k must be > 0".to_string(),
        ));
    }
    let modes = selected_retrieval_ablation_modes(options);
    let case_paths = discover_case_paths(&options.cases)?;
    if case_paths.is_empty() {
        return Err(BenchmarkError::Validation(format!(
            "no graph truth cases found under {}",
            options.cases.display()
        )));
    }

    let mut case_results = Vec::new();
    for case_path in case_paths {
        let case = load_case(&case_path)?;
        case_results.push(run_case(options, &modes, &case_path, case)?);
    }

    let summaries = summarize_modes(&modes, &case_results);
    Ok(RetrievalAblationReport {
        schema_version: 1,
        benchmark: "retrieval_ablation".to_string(),
        source_of_truth: "MVP.md".to_string(),
        status: "benchmarked".to_string(),
        cases_path: path_string(&options.cases),
        fixture_root: path_string(&options.fixture_root),
        top_k: options.top_k,
        modes: summaries,
        case_results,
        notes: vec![
            "Stage 0 exact/FTS contribution is reported separately and is not hidden inside Stage 1 or Stage 2.".to_string(),
            "Vector-only modes report path recall as 0 and cannot claim proof-grade path success without exact graph verification.".to_string(),
            "This benchmark runs on graph-truth fixtures and does not change extractor behavior or benchmark scoring.".to_string(),
        ],
    })
}

pub fn selected_retrieval_ablation_modes(
    options: &RetrievalAblationOptions,
) -> Vec<RetrievalAblationMode> {
    if options.modes.is_empty() {
        RetrievalAblationMode::ALL.to_vec()
    } else {
        let mut modes = options.modes.clone();
        modes.sort();
        modes.dedup();
        modes
    }
}

pub fn render_retrieval_ablation_markdown(report: &RetrievalAblationReport) -> String {
    let mut output = String::new();
    output.push_str("# Retrieval Stage Ablation\n\n");
    output.push_str("Source of truth: `MVP.md`.\n\n");
    output.push_str(&format!(
        "Cases: {}. Top-k: {}.\n\n",
        report.case_results.len(),
        report.top_k
    ));
    output.push_str("## Mode Summary\n\n");
    output.push_str("| Mode | File R@k | Symbol R@k | Path R@k | Relation F1 | FP Rate | Forbidden | p50 ms | p95 ms | Stage0 | Stage1 | Stage2 | Verified |\n");
    output.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for mode in &report.modes {
        output.push_str(&format!(
            "| `{}` | {:.3} | {:.3} | {:.3} | {} | {:.3} | {} | {} | {} | {:.1} | {:.1} | {:.1} | {:.1} |\n",
            mode.mode,
            mode.file_recall_at_k_mean,
            mode.symbol_recall_at_k_mean,
            mode.path_recall_at_k_mean,
            optional_ratio(mode.relation_f1_mean),
            mode.false_positive_rate_mean,
            mode.forbidden_edge_violations + mode.forbidden_path_violations,
            mode.query_latency_p50_ms,
            mode.query_latency_p95_ms,
            mode.stage0_candidate_count_mean,
            mode.stage1_candidate_count_mean,
            mode.stage2_candidate_count_mean,
            mode.graph_verified_candidate_count_mean
        ));
    }

    output.push_str("\n## Proof Policy\n\n");
    for mode in &report.modes {
        output.push_str(&format!(
            "- `{}`: proof-grade path success claimed = `{}`; Stage 0 exact contribution visible = `{}`.\n",
            mode.mode, mode.proof_grade_path_success_claimed, mode.stage0_exact_contribution_visible
        ));
    }

    output.push_str("\n## Case Results\n\n");
    output.push_str("| Case | Mode | File R@k | Symbol R@k | Path R@k | Stage0 | Stage1 | Stage2 | Verified Paths | Notes |\n");
    output.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n");
    for case in &report.case_results {
        for mode in &case.modes {
            output.push_str(&format!(
                "| `{}` | `{}` | {:.3} | {:.3} | {:.3} | {} | {} | {} | {} | {} |\n",
                case.case_id,
                mode.mode,
                mode.file_recall_at_k,
                mode.symbol_recall_at_k,
                mode.path_recall_at_k,
                mode.candidate_counts.stage0_candidates,
                mode.candidate_counts.stage1_candidates,
                mode.candidate_counts.stage2_candidates,
                mode.candidate_counts.graph_verified_paths,
                mode.notes.join("; ")
            ));
        }
    }

    output.push_str("\n## Notes\n\n");
    for note in &report.notes {
        output.push_str(&format!("- {note}\n"));
    }
    output
}

fn run_case(
    options: &RetrievalAblationOptions,
    modes: &[RetrievalAblationMode],
    case_path: &Path,
    case: AblationCase,
) -> BenchResult<RetrievalAblationCaseResult> {
    let repo_root = resolve_fixture_repo(options, case_path, &case)?;
    let db_path = unique_output_path(
        &format!("codegraph-retrieval-ablation-{}", safe_id(&case.case_id)),
        "sqlite",
    );
    let index_summary = index_repo_to_db(&repo_root, &db_path)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let store = SqliteGraphStore::open(&db_path)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let edges = store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let sources = load_sources(&repo_root, &store)?;
    let documents = build_documents(&sources, &entities);
    let index_size_bytes = db_family_size_bytes(&db_path);
    let repo = IndexedAblationRepo {
        repo_root,
        entities,
        edges,
        sources,
        documents,
        index_size_bytes,
    };
    let mut results = Vec::new();
    for mode in modes {
        results.push(run_mode(&case, &repo, *mode, options.top_k)?);
    }
    cleanup_db_family(&db_path);
    Ok(RetrievalAblationCaseResult {
        case_id: case.case_id,
        description: case.description,
        repo_fixture_path: path_string(&repo.repo_root),
        files_indexed: index_summary.files_indexed as u64,
        entities_indexed: index_summary.entities as u64,
        edges_indexed: index_summary.edges as u64,
        index_size_bytes,
        modes: results,
    })
}

fn run_mode(
    case: &AblationCase,
    repo: &IndexedAblationRepo,
    mode: RetrievalAblationMode,
    top_k: usize,
) -> BenchResult<RetrievalAblationModeResult> {
    let expected = ExpectedSets::from_case(case);
    let started = Instant::now();
    let observation = observe_mode(case, repo, mode, top_k)?;
    let elapsed_ms = started.elapsed().as_millis().max(1) as u64;
    let retrieved = retrieved_sets(repo, &observation);
    let stage0_retrieved = observation
        .stage0
        .as_ref()
        .map(|stage0| retrieved_sets_for_ids(repo, &stage0.candidates, &[]));
    let relation_f1 = if mode.uses_graph_verification() {
        Some(relation_score(case, repo, &observation.graph_paths))
    } else {
        None
    };
    let forbidden_edge_violations = if mode.uses_graph_verification() {
        count_forbidden_edges(case, repo, &observation.graph_paths)
    } else {
        0
    };
    let forbidden_path_violations = if mode.uses_graph_verification() {
        count_forbidden_paths(case, repo, &observation.graph_paths)
    } else {
        0
    };
    let forbidden_context_hits = count_forbidden_context_symbols(case, &retrieved.symbols)
        + count_forbidden_tests(case, &retrieved.tests);
    let false_positive_count =
        forbidden_edge_violations + forbidden_path_violations + forbidden_context_hits;
    let denominator = retrieved.symbols.len() + retrieved.files.len() + retrieved.paths.len();
    let false_positive_rate = divide_usize(false_positive_count, denominator.max(1));
    let path_recall = if mode.uses_graph_verification() {
        path_recall_at_k(case, repo, &observation.graph_paths, top_k)
    } else {
        0.0
    };
    let proof_grade_path_success_claimed = mode.uses_graph_verification()
        && path_recall > 0.0
        && relation_f1.as_ref().is_some_and(|score| score.f1 > 0.0)
        && observation
            .graph_paths
            .iter()
            .take(top_k)
            .any(is_proof_grade_path);
    let candidate_counts = AblationCandidateCounts {
        corpus_documents: repo.documents.len(),
        stage0_candidates: observation
            .stage0
            .as_ref()
            .map(|stage0| stage0.candidates.len())
            .unwrap_or_default(),
        stage0_exact_seed_candidates: observation
            .stage0
            .as_ref()
            .map(|stage0| stage0.exact_seed_candidates.len())
            .unwrap_or_default(),
        stage1_candidates: observation.stage1_ids.len(),
        stage2_candidates: observation.stage2_ids.len(),
        graph_verification_inputs: observation.graph_inputs.len(),
        graph_verified_candidates: observation
            .graph_paths
            .iter()
            .map(|path| path.source.clone())
            .collect::<BTreeSet<_>>()
            .len(),
        graph_verified_paths: observation.graph_paths.len(),
        context_packet_symbols: observation
            .context_packet
            .as_ref()
            .map(|packet| packet.symbols.len())
            .unwrap_or_default(),
        context_packet_paths: observation
            .context_packet
            .as_ref()
            .map(|packet| packet.verified_paths.len())
            .unwrap_or_default(),
    };
    Ok(RetrievalAblationModeResult {
        mode: mode.as_str().to_string(),
        file_recall_at_k: recall_match_at_k(&expected.files, &retrieved.files, top_k, file_matches),
        symbol_recall_at_k: recall_match_at_k(
            &expected.symbols,
            &retrieved.symbols,
            top_k,
            symbol_matches,
        ),
        path_recall_at_k: path_recall,
        relation_f1,
        false_positive_rate,
        false_positive_count,
        forbidden_edge_violations,
        forbidden_path_violations,
        query_latency_ms: elapsed_ms,
        memory_bytes_estimate: estimate_mode_memory_bytes(mode, repo, &candidate_counts),
        index_size_bytes: repo.index_size_bytes,
        candidate_counts,
        stage0_exact_file_recall_at_k: stage0_retrieved
            .as_ref()
            .map(|sets| recall_match_at_k(&expected.files, &sets.files, top_k, file_matches)),
        stage0_exact_symbol_recall_at_k: stage0_retrieved
            .as_ref()
            .map(|sets| recall_match_at_k(&expected.symbols, &sets.symbols, top_k, symbol_matches)),
        proof_grade_path_success_claimed,
        retrieved_files: retrieved.files.into_iter().take(top_k).collect(),
        retrieved_symbols: retrieved.symbols.into_iter().take(top_k).collect(),
        retrieved_paths: retrieved.paths.into_iter().take(top_k).collect(),
        notes: observation.notes,
    })
}

fn observe_mode(
    case: &AblationCase,
    repo: &IndexedAblationRepo,
    mode: RetrievalAblationMode,
    top_k: usize,
) -> BenchResult<ModeObservation> {
    match mode {
        RetrievalAblationMode::Stage0ExactOnly => {
            let stage0 = stage0_candidates(case, repo, top_k);
            Ok(ModeObservation {
                candidate_ids: stage0.candidates.clone(),
                stage0: Some(stage0),
                stage1_ids: Vec::new(),
                stage2_ids: Vec::new(),
                graph_inputs: Vec::new(),
                graph_paths: Vec::new(),
                context_packet: None,
                notes: vec![
                    "Stage 0 exact/BM25/FTS candidates only; no vector or graph proof.".to_string(),
                ],
            })
        }
        RetrievalAblationMode::Stage1BinaryOnly => {
            let stage1 = stage1_binary(repo, &case.task_prompt, &[], top_k)?;
            Ok(ModeObservation {
                candidate_ids: stage1.clone(),
                stage0: None,
                stage1_ids: stage1,
                stage2_ids: Vec::new(),
                graph_inputs: Vec::new(),
                graph_paths: Vec::new(),
                context_packet: None,
                notes: vec!["Binary-only mode uses no Stage 0 exact-seed bypass and cannot claim proof paths.".to_string()],
            })
        }
        RetrievalAblationMode::Stage2Int8PqOnly => {
            let stage2 = stage2_rerank(repo, &case.task_prompt, &repo.documents, top_k)?;
            Ok(ModeObservation {
                candidate_ids: stage2.clone(),
                stage0: None,
                stage1_ids: Vec::new(),
                stage2_ids: stage2,
                graph_inputs: Vec::new(),
                graph_paths: Vec::new(),
                context_packet: None,
                notes: vec!["Stage 2-only mode uses the deterministic int8/Matryoshka rerank interface and cannot claim proof paths.".to_string()],
            })
        }
        RetrievalAblationMode::Stage0PlusStage1 => {
            let stage0 = stage0_candidates(case, repo, top_k);
            let stage1 = stage1_binary(
                repo,
                &case.task_prompt,
                &stage0.exact_seed_candidates,
                top_k,
            )?;
            Ok(ModeObservation {
                candidate_ids: stage1.clone(),
                stage0: Some(stage0),
                stage1_ids: stage1,
                stage2_ids: Vec::new(),
                graph_inputs: Vec::new(),
                graph_paths: Vec::new(),
                context_packet: None,
                notes: vec!["Stage 0 exact-seed contribution is reported separately from Stage 1 candidates.".to_string()],
            })
        }
        RetrievalAblationMode::Stage0PlusStage1PlusStage2 => {
            let stage0 = stage0_candidates(case, repo, top_k);
            let stage1 = stage1_binary(
                repo,
                &case.task_prompt,
                &stage0.exact_seed_candidates,
                top_k * 2,
            )?;
            let docs = documents_for_ids(repo, &stage1);
            let stage2 = stage2_rerank(repo, &case.task_prompt, &docs, top_k)?;
            Ok(ModeObservation {
                candidate_ids: stage2.clone(),
                stage0: Some(stage0),
                stage1_ids: stage1,
                stage2_ids: stage2,
                graph_inputs: Vec::new(),
                graph_paths: Vec::new(),
                context_packet: None,
                notes: vec!["Vector funnel stops before graph verification; proof path recall is forced to 0.".to_string()],
            })
        }
        RetrievalAblationMode::GraphVerificationOnly => {
            let stage0 = stage0_candidates(case, repo, top_k);
            let graph_inputs = stage0
                .candidates
                .iter()
                .filter(|id| entity_by_id(repo, id).is_some())
                .cloned()
                .collect::<Vec<_>>();
            let graph_paths = graph_paths_for_seeds(repo, &graph_inputs);
            Ok(ModeObservation {
                candidate_ids: graph_inputs.clone(),
                stage0: Some(stage0),
                stage1_ids: Vec::new(),
                stage2_ids: Vec::new(),
                graph_inputs,
                graph_paths,
                context_packet: None,
                notes: vec![
                    "Exact graph verification is measured without Stage 1/2 vector reranking."
                        .to_string(),
                ],
            })
        }
        RetrievalAblationMode::FullContextPacket => {
            let stage0 = stage0_candidates(case, repo, top_k);
            let config = RetrievalFunnelConfig {
                stage1_top_k: top_k * 2,
                stage2_top_n: top_k,
                query_limits: query_limits(),
                ..RetrievalFunnelConfig::default()
            };
            let funnel = RetrievalFunnel::new(repo.edges.clone(), Vec::new(), config)
                .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
            let output = funnel
                .run(
                    RetrievalFunnelRequest::new(&case.task_prompt, "retrieval_ablation", 2_000)
                        .exact_seeds(stage0.exact_seed_candidates.clone())
                        .stage0_candidates(stage0.candidate_documents.clone())
                        .sources(repo.sources.clone()),
                )
                .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
            let stage1_ids = trace_kept(&output.trace, "stage1_binary_sieve");
            let stage2_ids = trace_kept(&output.trace, "stage2_compressed_rerank");
            let graph_inputs = stage2_ids.clone();
            let graph_paths = graph_paths_from_packet(repo, &output.packet);
            Ok(ModeObservation {
                candidate_ids: output.packet.symbols.clone(),
                stage0: Some(stage0),
                stage1_ids,
                stage2_ids,
                graph_inputs,
                graph_paths,
                context_packet: Some(output.packet),
                notes: vec!["Full funnel uses Stage 0 candidates, binary sieve, compressed rerank, exact verification, and context packet construction.".to_string()],
            })
        }
    }
}

fn stage0_candidates(
    case: &AblationCase,
    repo: &IndexedAblationRepo,
    top_k: usize,
) -> Stage0Output {
    let mut exact_candidates = BTreeSet::<String>::new();
    let mut scored_candidates = BTreeMap::<String, f64>::new();
    for seed in extract_prompt_seeds(&case.task_prompt) {
        if let Some(value) = seed.exact_value() {
            for id in resolve_seed_value(repo, &value) {
                exact_candidates.insert(id.clone());
                scored_candidates.insert(id, 1.0);
            }
        }
    }
    for (id, score) in bm25_like_candidates(repo, &case.task_prompt, top_k * 2) {
        scored_candidates
            .entry(id)
            .and_modify(|current| *current = current.max(score))
            .or_insert(score);
    }
    let mut ranked = scored_candidates.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    let candidates = ranked
        .into_iter()
        .take(top_k)
        .map(|(id, _score)| id)
        .collect::<Vec<_>>();
    let candidate_documents = documents_for_ids(repo, &candidates);
    Stage0Output {
        candidates,
        exact_seed_candidates: exact_candidates.into_iter().collect(),
        candidate_documents,
    }
}

fn stage1_binary(
    repo: &IndexedAblationRepo,
    prompt: &str,
    exact_seed_ids: &[String],
    top_k: usize,
) -> BenchResult<Vec<String>> {
    let mut index = InMemoryBinaryVectorIndex::new(128)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    for document in &repo.documents {
        index
            .upsert_text(&document.id, &document.text)
            .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    }
    let query = BinarySignature::from_text(prompt, 128)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    let candidates = index
        .search_with_exact_seeds(&query, top_k, exact_seed_ids)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    Ok(candidates
        .into_iter()
        .map(|candidate| candidate.id)
        .collect())
}

fn stage2_rerank(
    repo: &IndexedAblationRepo,
    prompt: &str,
    documents: &[RetrievalDocument],
    top_k: usize,
) -> BenchResult<Vec<String>> {
    let reranker = DeterministicCompressedReranker::new(RerankConfig::default());
    let candidates = documents
        .iter()
        .map(|document| {
            let mut candidate = RerankCandidate::new(document.id.clone(), document.text.clone())
                .stage0_score(document.stage0_score);
            if resolve_seed_value(repo, &document.id).contains(&document.id) {
                candidate = candidate.exact_seed(false);
            }
            candidate
        })
        .collect::<Vec<_>>();
    let scores = reranker
        .rerank(&RerankQuery::new(prompt), &candidates, top_k)
        .map_err(|error| BenchmarkError::Vector(error.to_string()))?;
    Ok(scores.into_iter().map(|score| score.id).collect())
}

fn graph_paths_for_seeds(repo: &IndexedAblationRepo, seeds: &[String]) -> Vec<GraphPath> {
    let engine = ExactGraphQueryEngine::new(repo.edges.clone());
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
    unique_paths(paths)
}

fn graph_paths_from_packet(repo: &IndexedAblationRepo, packet: &ContextPacket) -> Vec<GraphPath> {
    let seeds = packet
        .symbols
        .iter()
        .filter(|id| entity_by_id(repo, id).is_some())
        .cloned()
        .collect::<Vec<_>>();
    graph_paths_for_seeds(repo, &seeds)
}

fn retrieved_sets(repo: &IndexedAblationRepo, observation: &ModeObservation) -> RetrievedSets {
    let mut sets =
        retrieved_sets_for_ids(repo, &observation.candidate_ids, &observation.graph_paths);
    if let Some(packet) = &observation.context_packet {
        for symbol in &packet.symbols {
            push_unique(&mut sets.symbols, symbol.clone());
            if let Some(entity) = entity_by_id(repo, symbol) {
                push_unique(&mut sets.files, normalize_path(&entity.repo_relative_path));
            }
        }
        for snippet in &packet.snippets {
            push_unique(&mut sets.files, normalize_path(&snippet.file));
        }
        for test in &packet.recommended_tests {
            push_unique(&mut sets.tests, test.clone());
        }
        for path in &packet.verified_paths {
            push_unique(
                &mut sets.paths,
                format!(
                    "{}->{}/{}",
                    path.source,
                    path.target,
                    path.metapath
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(">")
                ),
            );
        }
    }
    sets
}

fn retrieved_sets_for_ids(
    repo: &IndexedAblationRepo,
    ids: &[String],
    paths: &[GraphPath],
) -> RetrievedSets {
    let mut sets = RetrievedSets::default();
    for id in ids {
        if repo.sources.contains_key(&normalize_path(id)) {
            push_unique(&mut sets.files, normalize_path(id));
        }
        if let Some(entity) = entity_by_id(repo, id) {
            push_unique(&mut sets.symbols, entity.id.clone());
            push_unique(&mut sets.symbols, entity.qualified_name.clone());
            push_unique(&mut sets.symbols, entity.name.clone());
            push_unique(&mut sets.files, normalize_path(&entity.repo_relative_path));
            if matches!(
                entity.kind,
                EntityKind::TestFile | EntityKind::TestSuite | EntityKind::TestCase
            ) {
                push_unique(&mut sets.tests, entity.qualified_name.clone());
            }
        } else {
            push_unique(&mut sets.symbols, id.clone());
        }
    }
    for path in paths {
        push_unique(&mut sets.symbols, path.source.clone());
        push_unique(&mut sets.symbols, path.target.clone());
        push_unique(
            &mut sets.paths,
            format!(
                "{}->{}/{}",
                path.source,
                path.target,
                path.relations()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(">")
            ),
        );
        for step in &path.steps {
            push_unique(&mut sets.symbols, step.from.clone());
            push_unique(&mut sets.symbols, step.to.clone());
            if let Some(entity) =
                entity_by_id(repo, &step.from).or_else(|| entity_by_id(repo, &step.to))
            {
                push_unique(&mut sets.files, normalize_path(&entity.repo_relative_path));
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
                push_unique(&mut sets.tests, step.to.clone());
            }
        }
    }
    sets
}

#[derive(Default)]
struct RetrievedSets {
    files: Vec<String>,
    symbols: Vec<String>,
    paths: Vec<String>,
    tests: Vec<String>,
}

struct ExpectedSets {
    files: Vec<String>,
    symbols: Vec<String>,
}

impl ExpectedSets {
    fn from_case(case: &AblationCase) -> Self {
        let mut files = Vec::new();
        let mut symbols = Vec::new();
        for entity in &case.expected_entities {
            collect_entity_ref(&mut symbols, &entity.selector);
            if let Some(file) = entity
                .source_file
                .as_deref()
                .or(entity.selector.source_file.as_deref())
            {
                push_unique(&mut files, normalize_path(file));
            }
        }
        for edge in &case.expected_edges {
            collect_edge_expectation(&mut files, &mut symbols, edge);
        }
        for path in &case.expected_paths {
            collect_entity_ref(&mut symbols, &path.source);
            collect_entity_ref(&mut symbols, &path.target);
            for edge in &path.ordered_edges {
                collect_edge_expectation(&mut files, &mut symbols, edge);
            }
        }
        for context in &case.expected_context_symbols {
            collect_entity_ref(&mut symbols, &context.symbol);
            if let Some(file) = context
                .source_file
                .as_deref()
                .or(context.symbol.source_file.as_deref())
            {
                push_unique(&mut files, normalize_path(file));
            }
        }
        for test in &case.expected_tests {
            push_unique(&mut symbols, test.name.clone());
            if let Some(file) = &test.source_file {
                push_unique(&mut files, normalize_path(file));
            }
        }
        Self { files, symbols }
    }
}

fn collect_edge_expectation(
    files: &mut Vec<String>,
    symbols: &mut Vec<String>,
    edge: &EdgeExpectation,
) {
    collect_entity_ref(symbols, &edge.head);
    collect_entity_ref(symbols, &edge.tail);
    push_unique(files, normalize_path(&edge.source_file));
    if let Some(file) = edge.head.source_file.as_deref() {
        push_unique(files, normalize_path(file));
    }
    if let Some(file) = edge.tail.source_file.as_deref() {
        push_unique(files, normalize_path(file));
    }
}

fn collect_entity_ref(symbols: &mut Vec<String>, reference: &EntityRef) {
    for value in [
        reference.id.as_deref(),
        reference.qualified_name.as_deref(),
        reference.name.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        push_unique(symbols, value.to_string());
    }
}

fn relation_score(
    case: &AblationCase,
    repo: &IndexedAblationRepo,
    paths: &[GraphPath],
) -> MetricScore {
    let mut matched_expected = 0usize;
    for expected in &case.expected_edges {
        if paths.iter().any(|path| {
            path.steps
                .iter()
                .any(|step| edge_matches_expectation(repo, &step.edge, expected))
        }) {
            matched_expected += 1;
        }
    }
    let forbidden = count_forbidden_edges(case, repo, paths);
    let precision = divide_usize(matched_expected, matched_expected + forbidden);
    let recall = divide_usize(matched_expected, case.expected_edges.len());
    let f1 = if precision + recall <= f64::EPSILON {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    MetricScore {
        precision,
        recall,
        f1,
    }
}

fn count_forbidden_edges(
    case: &AblationCase,
    repo: &IndexedAblationRepo,
    paths: &[GraphPath],
) -> usize {
    case.forbidden_edges
        .iter()
        .filter(|forbidden| {
            paths.iter().any(|path| {
                path.steps
                    .iter()
                    .any(|step| edge_matches_expectation(repo, &step.edge, forbidden))
            })
        })
        .count()
}

fn count_forbidden_paths(
    case: &AblationCase,
    repo: &IndexedAblationRepo,
    paths: &[GraphPath],
) -> usize {
    case.forbidden_paths
        .iter()
        .filter(|forbidden| path_expectation_matches(repo, paths, forbidden))
        .count()
}

fn count_forbidden_context_symbols(case: &AblationCase, symbols: &[String]) -> usize {
    case.forbidden_context_symbols
        .iter()
        .filter(|forbidden| {
            let labels = entity_ref_labels(&forbidden.symbol);
            labels.iter().any(|expected| {
                symbols
                    .iter()
                    .any(|observed| symbol_matches(expected, observed))
            })
        })
        .count()
}

fn count_forbidden_tests(case: &AblationCase, tests: &[String]) -> usize {
    case.forbidden_tests
        .iter()
        .filter(|forbidden| {
            tests
                .iter()
                .any(|test| symbol_matches(&forbidden.name, test))
        })
        .count()
}

fn path_recall_at_k(
    case: &AblationCase,
    repo: &IndexedAblationRepo,
    paths: &[GraphPath],
    top_k: usize,
) -> f64 {
    if case.expected_paths.is_empty() {
        return 1.0;
    }
    let top_paths = paths.iter().take(top_k).cloned().collect::<Vec<_>>();
    let hits = case
        .expected_paths
        .iter()
        .filter(|expected| path_expectation_matches(repo, &top_paths, expected))
        .count();
    divide_usize(hits, case.expected_paths.len())
}

fn path_expectation_matches(
    repo: &IndexedAblationRepo,
    paths: &[GraphPath],
    expected: &PathExpectation,
) -> bool {
    paths.iter().any(|path| {
        if expected.ordered_edges.len() > expected.max_length {
            return false;
        }
        if expected.relation_sequence != path.relations() {
            return false;
        }
        if !entity_id_or_label_matches(repo, &path.source, &expected.source)
            || !entity_id_or_label_matches(repo, &path.target, &expected.target)
        {
            return false;
        }
        expected.ordered_edges.iter().all(|expected_edge| {
            path.steps
                .iter()
                .any(|step| edge_matches_expectation(repo, &step.edge, expected_edge))
        })
    })
}

fn edge_matches_expectation(
    repo: &IndexedAblationRepo,
    edge: &Edge,
    expected: &EdgeExpectation,
) -> bool {
    if edge.relation != expected.relation {
        return false;
    }
    if normalize_path(&edge.source_span.repo_relative_path) != normalize_path(&expected.source_file)
    {
        return false;
    }
    entity_id_or_label_matches(repo, &edge.head_id, &expected.head)
        && entity_id_or_label_matches(repo, &edge.tail_id, &expected.tail)
}

fn entity_id_or_label_matches(repo: &IndexedAblationRepo, id: &str, expected: &EntityRef) -> bool {
    if let Some(entity) = entity_by_id(repo, id) {
        entity_matches_ref(entity, expected)
    } else {
        entity_ref_labels(expected)
            .iter()
            .any(|label| symbol_matches(label, id))
    }
}

fn entity_matches_ref(entity: &Entity, expected: &EntityRef) -> bool {
    if let Some(id) = &expected.id {
        return &entity.id == id;
    }
    if let Some(kind) = expected.kind {
        if entity.kind != kind {
            return false;
        }
    }
    if let Some(source_file) = &expected.source_file {
        if normalize_path(&entity.repo_relative_path) != normalize_path(source_file) {
            return false;
        }
    }
    entity_ref_labels(expected).iter().any(|label| {
        symbol_matches(label, &entity.id)
            || symbol_matches(label, &entity.name)
            || symbol_matches(label, &entity.qualified_name)
    })
}

fn is_proof_grade_path(path: &GraphPath) -> bool {
    !path.steps.is_empty()
        && path.steps.iter().all(|step| {
            step.edge.confidence >= 1.0
                && !step.edge.derived
                && !step.edge.source_span.repo_relative_path.trim().is_empty()
        })
}

fn build_documents(
    sources: &BTreeMap<String, String>,
    entities: &[Entity],
) -> Vec<RetrievalDocument> {
    let mut documents = Vec::new();
    for (path, source) in sources {
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
        documents.push(document);
    }
    documents
}

fn bm25_like_candidates(
    repo: &IndexedAblationRepo,
    prompt: &str,
    limit: usize,
) -> Vec<(String, f64)> {
    let query_tokens = tokens(prompt);
    if query_tokens.is_empty() {
        return Vec::new();
    }
    let mut scored = repo
        .documents
        .iter()
        .filter_map(|document| {
            let doc_tokens = tokens(&document.text);
            let overlap = query_tokens.intersection(&doc_tokens).count();
            if overlap == 0 {
                None
            } else {
                Some((
                    document.id.clone(),
                    (overlap as f64 / query_tokens.len() as f64) + document.stage0_score,
                ))
            }
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    scored.truncate(limit);
    scored
}

fn resolve_seed_value(repo: &IndexedAblationRepo, seed: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let normalized_seed = normalize_path(seed);
    if repo.sources.contains_key(&normalized_seed) {
        push_unique(&mut ids, normalized_seed.clone());
    }
    for document in &repo.documents {
        if symbol_matches(seed, &document.id)
            || document
                .metadata
                .get("file")
                .is_some_and(|file| file_matches(seed, file))
        {
            push_unique(&mut ids, document.id.clone());
        }
    }
    for entity in &repo.entities {
        if entity_matches_seed(entity, seed) {
            push_unique(&mut ids, entity.id.clone());
        }
    }
    ids
}

fn entity_matches_seed(entity: &Entity, seed: &str) -> bool {
    symbol_matches(seed, &entity.id)
        || symbol_matches(seed, &entity.name)
        || symbol_matches(seed, &entity.qualified_name)
        || file_matches(seed, &entity.repo_relative_path)
}

fn documents_for_ids(repo: &IndexedAblationRepo, ids: &[String]) -> Vec<RetrievalDocument> {
    ids.iter()
        .filter_map(|id| {
            repo.documents
                .iter()
                .find(|document| &document.id == id)
                .cloned()
                .or_else(|| Some(RetrievalDocument::new(id, id).stage0_score(1.0)))
        })
        .collect()
}

fn trace_kept(trace: &[codegraph_query::RetrievalTraceStage], stage: &str) -> Vec<String> {
    trace
        .iter()
        .find(|entry| entry.stage == stage)
        .map(|entry| entry.kept.clone())
        .unwrap_or_default()
}

fn summarize_modes(
    modes: &[RetrievalAblationMode],
    cases: &[RetrievalAblationCaseResult],
) -> Vec<RetrievalAblationModeSummary> {
    modes
        .iter()
        .filter_map(|mode| {
            let mode_name = mode.as_str();
            let results = cases
                .iter()
                .flat_map(|case| case.modes.iter())
                .filter(|result| result.mode == mode_name)
                .collect::<Vec<_>>();
            if results.is_empty() {
                return None;
            }
            let relation_scores = results
                .iter()
                .filter_map(|result| result.relation_f1.as_ref())
                .collect::<Vec<_>>();
            Some(RetrievalAblationModeSummary {
                mode: mode_name.to_string(),
                cases: results.len(),
                file_recall_at_k_mean: mean(results.iter().map(|result| result.file_recall_at_k)),
                symbol_recall_at_k_mean: mean(
                    results.iter().map(|result| result.symbol_recall_at_k),
                ),
                path_recall_at_k_mean: mean(results.iter().map(|result| result.path_recall_at_k)),
                relation_precision_mean: mean_option(
                    relation_scores.iter().map(|score| score.precision),
                ),
                relation_recall_mean: mean_option(relation_scores.iter().map(|score| score.recall)),
                relation_f1_mean: mean_option(relation_scores.iter().map(|score| score.f1)),
                false_positive_rate_mean: mean(
                    results.iter().map(|result| result.false_positive_rate),
                ),
                forbidden_edge_violations: results
                    .iter()
                    .map(|result| result.forbidden_edge_violations)
                    .sum(),
                forbidden_path_violations: results
                    .iter()
                    .map(|result| result.forbidden_path_violations)
                    .sum(),
                query_latency_p50_ms: percentile(
                    results
                        .iter()
                        .map(|result| result.query_latency_ms)
                        .collect(),
                    0.50,
                ),
                query_latency_p95_ms: percentile(
                    results
                        .iter()
                        .map(|result| result.query_latency_ms)
                        .collect(),
                    0.95,
                ),
                memory_bytes_estimate_mean: mean_u64(
                    results.iter().map(|result| result.memory_bytes_estimate),
                ),
                index_size_bytes_mean: mean_u64(
                    results.iter().map(|result| result.index_size_bytes),
                ),
                stage0_candidate_count_mean: mean(
                    results
                        .iter()
                        .map(|result| result.candidate_counts.stage0_candidates as f64),
                ),
                stage1_candidate_count_mean: mean(
                    results
                        .iter()
                        .map(|result| result.candidate_counts.stage1_candidates as f64),
                ),
                stage2_candidate_count_mean: mean(
                    results
                        .iter()
                        .map(|result| result.candidate_counts.stage2_candidates as f64),
                ),
                graph_verified_candidate_count_mean: mean(
                    results
                        .iter()
                        .map(|result| result.candidate_counts.graph_verified_candidates as f64),
                ),
                context_packet_symbol_count_mean: mean(
                    results
                        .iter()
                        .map(|result| result.candidate_counts.context_packet_symbols as f64),
                ),
                proof_grade_path_success_claimed: results
                    .iter()
                    .any(|result| result.proof_grade_path_success_claimed),
                stage0_exact_contribution_visible: mode.uses_stage0()
                    && results.iter().any(|result| {
                        result.stage0_exact_file_recall_at_k.is_some()
                            || result.stage0_exact_symbol_recall_at_k.is_some()
                    }),
            })
        })
        .collect()
}

fn estimate_mode_memory_bytes(
    mode: RetrievalAblationMode,
    repo: &IndexedAblationRepo,
    counts: &AblationCandidateCounts,
) -> u64 {
    let source_bytes = repo.sources.values().map(String::len).sum::<usize>() as u64;
    let graph_bytes = (repo.entities.len() as u64 * 256) + (repo.edges.len() as u64 * 384);
    let binary_bytes = counts.stage1_candidates as u64 * 128 / 8;
    let int8_bytes = counts.stage2_candidates as u64 * 128;
    match mode {
        RetrievalAblationMode::Stage0ExactOnly => source_bytes,
        RetrievalAblationMode::Stage1BinaryOnly | RetrievalAblationMode::Stage0PlusStage1 => {
            source_bytes + binary_bytes
        }
        RetrievalAblationMode::Stage2Int8PqOnly
        | RetrievalAblationMode::Stage0PlusStage1PlusStage2 => {
            source_bytes + binary_bytes + int8_bytes
        }
        RetrievalAblationMode::GraphVerificationOnly => source_bytes + graph_bytes,
        RetrievalAblationMode::FullContextPacket => {
            source_bytes + graph_bytes + binary_bytes + int8_bytes
        }
    }
}

fn load_case(path: &Path) -> BenchResult<AblationCase> {
    let raw = fs::read_to_string(path)?;
    serde_json::from_str(&raw).map_err(|error| {
        BenchmarkError::Parse(format!(
            "failed to parse graph truth case {}: {error}",
            path.display()
        ))
    })
}

fn discover_case_paths(path: &Path) -> BenchResult<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.exists() {
        return Err(BenchmarkError::Io(format!(
            "cases path does not exist: {}",
            path.display()
        )));
    }
    let mut cases = Vec::new();
    discover_case_paths_recursive(path, &mut cases)?;
    cases.sort();
    Ok(cases)
}

fn discover_case_paths_recursive(path: &Path, cases: &mut Vec<PathBuf>) -> BenchResult<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            discover_case_paths_recursive(&child, cases)?;
        } else if child.file_name().and_then(|name| name.to_str()) == Some("graph_truth_case.json")
        {
            cases.push(child);
        }
    }
    Ok(())
}

fn resolve_fixture_repo(
    options: &RetrievalAblationOptions,
    case_path: &Path,
    case: &AblationCase,
) -> BenchResult<PathBuf> {
    let declared = PathBuf::from(&case.repo_fixture_path);
    if declared.is_absolute() && declared.exists() {
        return Ok(declared);
    }
    let candidates = [
        options.fixture_root.join(&declared),
        PathBuf::from(&case.repo_fixture_path),
        case_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("repo"),
        options.fixture_root.join(&case.case_id).join("repo"),
    ];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| {
            BenchmarkError::Validation(format!(
                "fixture repo for case {} not found from {}",
                case.case_id, case.repo_fixture_path
            ))
        })
}

fn load_sources(
    repo_root: &Path,
    store: &SqliteGraphStore,
) -> BenchResult<BTreeMap<String, String>> {
    let mut sources = BTreeMap::new();
    for file in store
        .list_files(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?
    {
        let path = repo_root.join(&file.repo_relative_path);
        if path.exists() {
            sources.insert(
                normalize_path(&file.repo_relative_path),
                fs::read_to_string(path)?,
            );
        }
    }
    Ok(sources)
}

fn db_family_size_bytes(path: &Path) -> u64 {
    [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ]
    .into_iter()
    .filter_map(|path| fs::metadata(path).ok().map(|metadata| metadata.len()))
    .sum()
}

fn cleanup_db_family(db_path: &Path) {
    for path in [
        db_path.to_path_buf(),
        PathBuf::from(format!("{}-wal", db_path.display())),
        PathBuf::from(format!("{}-shm", db_path.display())),
    ] {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
}

fn entity_by_id<'a>(repo: &'a IndexedAblationRepo, id: &str) -> Option<&'a Entity> {
    repo.entities.iter().find(|entity| entity.id == id)
}

fn query_limits() -> QueryLimits {
    QueryLimits {
        max_depth: 6,
        max_paths: 24,
        max_edges_visited: 2_048,
    }
}

fn unique_paths(paths: Vec<GraphPath>) -> Vec<GraphPath> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for path in paths {
        let key = format!(
            "{}->{}:{}",
            path.source,
            path.target,
            path.edge_ids().join(",")
        );
        if seen.insert(key) {
            unique.push(path);
        }
    }
    unique
}

fn recall_match_at_k<F>(expected: &[String], observed: &[String], k: usize, matches: F) -> f64
where
    F: Fn(&str, &str) -> bool,
{
    if expected.is_empty() {
        return 1.0;
    }
    let hits = expected
        .iter()
        .filter(|expected| {
            observed
                .iter()
                .take(k)
                .any(|observed| matches(expected, observed))
        })
        .count();
    divide_usize(hits, expected.len())
}

fn entity_ref_labels(reference: &EntityRef) -> Vec<String> {
    [
        reference.id.clone(),
        reference.qualified_name.clone(),
        reference.name.clone(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn file_matches(expected: &str, observed: &str) -> bool {
    normalize_path(expected) == normalize_path(observed)
}

fn symbol_matches(expected: &str, observed: &str) -> bool {
    let expected = canonical_symbol(expected);
    let observed = canonical_symbol(observed);
    !expected.is_empty()
        && !observed.is_empty()
        && (expected == observed || observed.contains(&expected) || expected.contains(&observed))
}

fn tokens(input: &str) -> BTreeSet<String> {
    input
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        .filter(|token| token.len() >= 2)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.trim().is_empty() && !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn mean_option(values: impl Iterator<Item = f64>) -> Option<f64> {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn mean_u64(values: impl Iterator<Item = u64>) -> u64 {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() {
        0
    } else {
        values.iter().sum::<u64>() / values.len() as u64
    }
}

fn percentile(mut values: Vec<u64>, percentile: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let index = ((values.len() - 1) as f64 * percentile).ceil() as usize;
    values[index.min(values.len() - 1)]
}

fn divide_usize(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

fn canonical_symbol(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn safe_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn optional_ratio(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn write_json(path: &Path, value: &impl Serialize) -> BenchResult<()> {
    write_text(
        path,
        &serde_json::to_string_pretty(value)
            .map_err(|error| BenchmarkError::Parse(error.to_string()))?,
    )
}

fn write_text(path: &Path, contents: &str) -> BenchResult<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_ablation_mode_selection_filters_and_deduplicates_modes() {
        let mut options = default_retrieval_ablation_options();
        options.modes = vec![
            RetrievalAblationMode::FullContextPacket,
            RetrievalAblationMode::Stage0ExactOnly,
            RetrievalAblationMode::FullContextPacket,
        ];

        assert_eq!(
            selected_retrieval_ablation_modes(&options),
            vec![
                RetrievalAblationMode::Stage0ExactOnly,
                RetrievalAblationMode::FullContextPacket
            ]
        );
    }

    #[test]
    fn retrieval_ablation_vector_only_modes_cannot_claim_proof_paths() {
        for mode in [
            RetrievalAblationMode::Stage1BinaryOnly,
            RetrievalAblationMode::Stage2Int8PqOnly,
            RetrievalAblationMode::Stage0PlusStage1PlusStage2,
        ] {
            assert!(!mode.uses_graph_verification());
            assert!(!mode.uses_context_packet());
        }
        assert!(RetrievalAblationMode::FullContextPacket.uses_graph_verification());
        assert!(RetrievalAblationMode::FullContextPacket.uses_context_packet());
    }
}
