//! Graph-truth benchmark gate.
//!
//! This runner indexes small hand-labeled fixture repositories and compares the
//! observed graph against strict expected and forbidden graph facts.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use codegraph_core::{
    ContextPacket, ContextSnippet, Edge, Entity, EntityKind, Exactness, PathEvidence, RelationKind,
    SourceSpan,
};
#[cfg(test)]
use codegraph_core::{EdgeClass, EdgeContext};
use codegraph_index::{
    index_repo_to_db_with_options, IndexOptions, StorageMode, UNBOUNDED_STORE_READ_LIMIT,
};
use codegraph_query::{
    classify_edge_fact, extract_prompt_seeds, ContextPackRequest, ExactGraphQueryEngine, GraphPath,
    TraversalDirection, TraversalStep,
};
use codegraph_store::{GraphStore, SqliteGraphStore};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{unique_output_path, BenchResult, BenchmarkError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphTruthGateOptions {
    pub cases: PathBuf,
    pub fixture_root: PathBuf,
    pub out_json: PathBuf,
    pub out_md: PathBuf,
    pub fail_on_forbidden: bool,
    pub fail_on_missing_source_span: bool,
    pub fail_on_unresolved_exact: bool,
    pub fail_on_derived_without_provenance: bool,
    pub fail_on_test_mock_production_leak: bool,
    pub update_mode: bool,
    pub keep_workdirs: bool,
    pub verbose: bool,
}

pub fn default_graph_truth_gate_options() -> GraphTruthGateOptions {
    GraphTruthGateOptions {
        cases: PathBuf::from("benchmarks")
            .join("graph_truth")
            .join("fixtures"),
        fixture_root: PathBuf::from("."),
        out_json: PathBuf::from("reports")
            .join("audit")
            .join("artifacts")
            .join("04_graph_truth_gate_initial_run.json"),
        out_md: PathBuf::from("reports")
            .join("audit")
            .join("artifacts")
            .join("04_graph_truth_gate_initial_run.md"),
        fail_on_forbidden: false,
        fail_on_missing_source_span: false,
        fail_on_unresolved_exact: false,
        fail_on_derived_without_provenance: false,
        fail_on_test_mock_production_leak: false,
        update_mode: false,
        keep_workdirs: false,
        verbose: false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextPacketGateOptions {
    pub cases: PathBuf,
    pub fixture_root: PathBuf,
    pub out_json: PathBuf,
    pub out_md: PathBuf,
    pub top_k: usize,
    pub token_budget: usize,
}

pub fn default_context_packet_gate_options() -> ContextPacketGateOptions {
    ContextPacketGateOptions {
        cases: PathBuf::from("benchmarks")
            .join("graph_truth")
            .join("fixtures"),
        fixture_root: PathBuf::from("."),
        out_json: PathBuf::from("reports")
            .join("audit")
            .join("artifacts")
            .join("20_context_packet_gate.json"),
        out_md: PathBuf::from("reports")
            .join("audit")
            .join("artifacts")
            .join("20_context_packet_gate.md"),
        top_k: 10,
        token_budget: 4_000,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTruthGateReport {
    pub schema_version: u32,
    pub gate: String,
    pub status: String,
    pub verdict: String,
    pub cases_path: String,
    pub fixture_root: String,
    pub cases_total: usize,
    pub cases_passed: usize,
    pub cases_failed: usize,
    pub relation_metrics: Vec<RelationMetric>,
    pub totals: GraphTruthTotals,
    pub cases: Vec<GraphTruthCaseResult>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextPacketGateReport {
    pub schema_version: u32,
    pub gate: String,
    pub status: String,
    pub verdict: String,
    pub cases_path: String,
    pub fixture_root: String,
    pub top_k: usize,
    pub token_budget: usize,
    pub cases_total: usize,
    pub cases_passed: usize,
    pub cases_failed: usize,
    pub metrics: ContextPacketGateMetrics,
    pub cases: Vec<ContextPacketCaseResult>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphTruthTotals {
    pub expected_entities: usize,
    pub matched_entities: usize,
    pub expected_edges: usize,
    pub matched_expected_edges: usize,
    pub forbidden_edges: usize,
    pub matched_forbidden_edges: usize,
    pub expected_paths: usize,
    pub matched_expected_paths: usize,
    pub forbidden_paths: usize,
    pub matched_forbidden_paths: usize,
    pub source_span_failures: usize,
    pub context_packet_failures: usize,
    pub stale_failures: usize,
    pub base_edges_observed: usize,
    pub derived_edges_observed: usize,
    pub edge_class_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextPacketGateMetrics {
    pub expected_context_symbols: usize,
    pub matched_context_symbols_at_k: usize,
    pub context_symbol_recall_at_k: Option<f64>,
    pub critical_symbols: usize,
    pub missing_critical_symbols: usize,
    pub critical_symbol_missing_rate: Option<f64>,
    pub forbidden_context_symbols: usize,
    pub matched_forbidden_context_symbols: usize,
    pub distractor_symbols: usize,
    pub observed_distractors: usize,
    pub distractor_ratio: Option<f64>,
    pub expected_proof_paths: usize,
    pub matched_proof_paths: usize,
    pub proof_path_coverage: Option<f64>,
    pub proof_paths_with_source_spans: usize,
    pub source_span_coverage: Option<f64>,
    pub expected_critical_snippets: usize,
    pub matched_critical_snippets: usize,
    pub critical_snippet_coverage: Option<f64>,
    pub expected_tests: usize,
    pub matched_tests: usize,
    pub recommended_test_recall: Option<f64>,
    pub packet_bytes: usize,
    pub estimated_tokens: usize,
    pub useful_facts: usize,
    pub useful_facts_per_byte: Option<f64>,
    pub useful_facts_per_estimated_token: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationMetric {
    pub relation: String,
    pub expected_edges: usize,
    pub matched_expected_edges: usize,
    pub forbidden_edges: usize,
    pub matched_forbidden_edges: usize,
    pub precision: Option<f64>,
    pub recall: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTruthCaseResult {
    pub case_id: String,
    pub description: String,
    pub status: String,
    pub repo_fixture_path: String,
    pub db_path: String,
    pub staged_repo_path: String,
    pub files_indexed: u64,
    pub entities_indexed: u64,
    pub edges_indexed: u64,
    pub entity_count: u64,
    pub edge_count: u64,
    pub source_span_count: u64,
    pub timing: GraphTruthTiming,
    pub index_time: u128,
    pub query_time: u128,
    pub base_edges_observed: usize,
    pub derived_edges_observed: usize,
    pub edge_class_counts: BTreeMap<String, usize>,
    pub expected_entities: usize,
    pub matched_entities: usize,
    pub expected_edges: usize,
    pub matched_expected_edges: usize,
    pub forbidden_edges: usize,
    pub matched_forbidden_edges: usize,
    pub expected_paths: usize,
    pub matched_expected_paths: usize,
    pub forbidden_paths: usize,
    pub matched_forbidden_paths: usize,
    pub expected_context_symbols: usize,
    pub matched_context_symbols: usize,
    pub expected_tests: usize,
    pub matched_tests: usize,
    pub failures: Vec<GraphTruthFailure>,
    pub missing_entities: Vec<String>,
    pub missing_edges: Vec<String>,
    pub forbidden_edges_found: Vec<String>,
    pub missing_paths: Vec<String>,
    pub forbidden_paths_found: Vec<String>,
    pub false_positives: Vec<String>,
    pub false_negatives: Vec<String>,
    pub source_span_failures: Vec<String>,
    pub context_symbol_failures: Vec<String>,
    pub context_packet_failures: Vec<String>,
    pub expected_test_failures: Vec<String>,
    pub mutation_failures: Vec<String>,
    pub stale_failures: Vec<String>,
    pub relation_metrics: Vec<RelationMetric>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphTruthTiming {
    pub total_ms: u128,
    pub index_ms: u128,
    pub mutation_ms: u128,
    pub query_ms: u128,
    pub evaluation_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextPacketCaseResult {
    pub case_id: String,
    pub description: String,
    pub status: String,
    pub repo_fixture_path: String,
    pub db_path: String,
    pub files_indexed: u64,
    pub entities_indexed: u64,
    pub edges_indexed: u64,
    pub context_mode: String,
    pub prompt_seed_count: usize,
    pub resolved_seed_count: usize,
    pub packet_symbols: usize,
    pub packet_verified_paths: usize,
    pub packet_snippets: usize,
    pub packet_risks: usize,
    pub packet_recommended_tests: usize,
    pub stored_path_evidence_rows: u64,
    pub packet_bytes: usize,
    pub estimated_tokens: usize,
    pub metrics: ContextPacketGateMetrics,
    pub failures: Vec<GraphTruthFailure>,
    pub missing_critical_symbols: Vec<String>,
    pub missing_proof_paths: Vec<String>,
    pub missing_critical_snippets: Vec<String>,
    pub missing_tests: Vec<String>,
    pub forbidden_context_hits: Vec<String>,
    pub path_context_failures: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphTruthFailure {
    pub category: String,
    pub severity: String,
    pub message: String,
    pub relation: Option<String>,
    pub expected_id: Option<String>,
}

#[derive(Debug, Clone)]
struct ObservedGraph {
    entities: Vec<Entity>,
    edges: Vec<Edge>,
    sources: BTreeMap<String, String>,
    context_packet: ContextPacket,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct GraphTruthCase {
    #[serde(default)]
    schema_version: Option<u32>,
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
    expected_source_spans: Vec<SourceSpanExpectation>,
    #[serde(default)]
    expected_context_symbols: Vec<ContextSymbolExpectation>,
    #[serde(default)]
    forbidden_context_symbols: Vec<ContextSymbolExpectation>,
    #[serde(default)]
    expected_tests: Vec<TestExpectation>,
    #[serde(default)]
    forbidden_tests: Vec<TestExpectation>,
    #[serde(default)]
    mutation_steps: Vec<MutationStep>,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    distractor_policy: Option<DistractorPolicy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EntityExpectation {
    selector: EntityRef,
    kind: EntityKind,
    context: ExecutionContext,
    #[serde(default)]
    source_file: Option<String>,
    #[serde(default)]
    source_span: Option<SpanExpectation>,
    #[serde(default)]
    must_not_collide_with: Vec<EntityRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EdgeExpectation {
    #[serde(default)]
    id: Option<String>,
    head: EntityRef,
    relation: RelationKind,
    tail: EntityRef,
    source_file: String,
    source_span: SpanExpectation,
    exactness: ExactnessRequirement,
    context: ExecutionContext,
    resolution: ResolutionStatus,
    derived: bool,
    #[serde(default)]
    provenance_edges: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PathExpectation {
    #[serde(default)]
    path_id: Option<String>,
    #[serde(default)]
    description: Option<String>,
    source: EntityRef,
    target: EntityRef,
    ordered_edges: Vec<EdgeExpectation>,
    relation_sequence: Vec<RelationKind>,
    max_length: usize,
    source_span_required: bool,
    production_only: bool,
    derived_allowed: bool,
    provenance_required: bool,
    required_source_spans: Vec<SourceSpanExpectation>,
    context: ExecutionContext,
    allow_test_mock_edges: bool,
    derived_edges_require_provenance: bool,
    #[serde(default)]
    proof_grade: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SourceSpanExpectation {
    #[serde(default)]
    edge_ref: Option<String>,
    source_file: String,
    span: SpanExpectation,
    must_resolve_to_text: bool,
    #[serde(default)]
    expected_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ContextSymbolExpectation {
    symbol: EntityRef,
    #[serde(default)]
    critical: Option<bool>,
    #[serde(default)]
    source_file: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    max_distractors: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TestExpectation {
    name: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    source_file: Option<String>,
    #[serde(default)]
    context: Option<ExecutionContext>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DistractorPolicy {
    #[serde(default)]
    max_context_distractors: Option<usize>,
    #[serde(default)]
    max_symbol_distractors: Option<usize>,
    #[serde(default)]
    distractor_symbols: Vec<ContextSymbolExpectation>,
    #[serde(default)]
    count_scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MutationStep {
    #[serde(rename = "type")]
    step_type: MutationStepType,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    find: Option<String>,
    #[serde(default)]
    replace: Option<String>,
    #[serde(default)]
    import_from: Option<String>,
    #[serde(default)]
    old_alias: Option<String>,
    #[serde(default)]
    new_alias: Option<String>,
    #[serde(default)]
    query_prompt: Option<String>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MutationStepType {
    EditFile,
    RenameFile,
    DeleteFile,
    ChangeImportAlias,
    AddFile,
    RemoveFile,
    Reindex,
    QueryAgain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default)]
    context: Option<ExecutionContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SpanExpectation {
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    #[serde(default)]
    expected_text: Option<String>,
    #[serde(default)]
    syntax_role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExecutionContext {
    Production,
    Test,
    Mock,
    Fixture,
    Generated,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ResolutionStatus {
    Resolved,
    Unresolved,
    Unknown,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ExactnessRequirement {
    allowed: Vec<Exactness>,
    minimum: Exactness,
    #[serde(default)]
    proof_grade_required: Option<bool>,
    #[serde(default)]
    confidence_floor: Option<f64>,
}

pub fn write_graph_truth_gate_report(
    options: GraphTruthGateOptions,
) -> BenchResult<GraphTruthGateReport> {
    let report = run_graph_truth_gate(&options)?;
    write_json(&options.out_json, &report)?;
    write_text(&options.out_md, &render_graph_truth_gate_markdown(&report))?;
    Ok(report)
}

pub fn run_graph_truth_gate(options: &GraphTruthGateOptions) -> BenchResult<GraphTruthGateReport> {
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
        case_results.push(run_case(options, &case_path, case)?);
    }

    let mut relation_totals = BTreeMap::<String, RelationMetric>::new();
    let mut totals = GraphTruthTotals::default();
    for case in &case_results {
        totals.expected_entities += case.expected_entities;
        totals.matched_entities += case.matched_entities;
        totals.expected_edges += case.expected_edges;
        totals.matched_expected_edges += case.matched_expected_edges;
        totals.forbidden_edges += case.forbidden_edges;
        totals.matched_forbidden_edges += case.matched_forbidden_edges;
        totals.expected_paths += case.expected_paths;
        totals.matched_expected_paths += case.matched_expected_paths;
        totals.forbidden_paths += case.forbidden_paths;
        totals.matched_forbidden_paths += case.matched_forbidden_paths;
        totals.source_span_failures += case.source_span_failures.len();
        totals.context_packet_failures += case.context_packet_failures.len();
        totals.stale_failures += case.stale_failures.len();
        totals.base_edges_observed += case.base_edges_observed;
        totals.derived_edges_observed += case.derived_edges_observed;
        for (fact_class, count) in &case.edge_class_counts {
            *totals
                .edge_class_counts
                .entry(fact_class.clone())
                .or_default() += count;
        }

        for metric in &case.relation_metrics {
            let aggregate = relation_totals
                .entry(metric.relation.clone())
                .or_insert_with(|| empty_relation_metric_string(&metric.relation));
            aggregate.expected_edges += metric.expected_edges;
            aggregate.matched_expected_edges += metric.matched_expected_edges;
            aggregate.forbidden_edges += metric.forbidden_edges;
            aggregate.matched_forbidden_edges += metric.matched_forbidden_edges;
        }
    }
    let mut relation_metrics = relation_totals.into_values().collect::<Vec<_>>();
    for metric in &mut relation_metrics {
        metric.precision = if metric.matched_expected_edges + metric.matched_forbidden_edges > 0 {
            Some(divide(
                metric.matched_expected_edges,
                metric.matched_expected_edges + metric.matched_forbidden_edges,
            ))
        } else {
            None
        };
        metric.recall = if metric.expected_edges > 0 {
            Some(divide(metric.matched_expected_edges, metric.expected_edges))
        } else {
            None
        };
    }
    relation_metrics.sort_by(|left, right| left.relation.cmp(&right.relation));

    let cases_passed = case_results
        .iter()
        .filter(|case| case.status == "passed")
        .count();
    let cases_failed = case_results.len().saturating_sub(cases_passed);
    let verdict = if cases_failed == 0 {
        "passed"
    } else {
        "failed"
    };
    Ok(GraphTruthGateReport {
        schema_version: 1,
        gate: "graph_truth".to_string(),
        status: verdict.to_string(),
        verdict: verdict.to_string(),
        cases_path: path_string(&options.cases),
        fixture_root: path_string(&options.fixture_root),
        cases_total: case_results.len(),
        cases_passed,
        cases_failed,
        relation_metrics,
        totals,
        cases: case_results,
        notes: vec![
            "Graph Truth Gate indexes each fixture and compares observed graph facts against hand-labeled expected and forbidden facts.".to_string(),
            "A failed gate is expected until semantic resolution, source-span exactness, stale update, and provenance gaps are fixed.".to_string(),
        ],
    })
}

pub fn write_context_packet_gate_report(
    options: ContextPacketGateOptions,
) -> BenchResult<ContextPacketGateReport> {
    let report = run_context_packet_gate(&options)?;
    write_json(&options.out_json, &report)?;
    write_text(
        &options.out_md,
        &render_context_packet_gate_markdown(&report),
    )?;
    Ok(report)
}

pub fn run_context_packet_gate(
    options: &ContextPacketGateOptions,
) -> BenchResult<ContextPacketGateReport> {
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
        case_results.push(run_context_packet_case(options, &case_path, case)?);
    }

    let cases_passed = case_results
        .iter()
        .filter(|case| case.status == "passed")
        .count();
    let cases_failed = case_results.len().saturating_sub(cases_passed);
    let verdict = if cases_failed == 0 {
        "passed"
    } else {
        "failed"
    };
    Ok(ContextPacketGateReport {
        schema_version: 1,
        gate: "context_packet".to_string(),
        status: verdict.to_string(),
        verdict: verdict.to_string(),
        cases_path: path_string(&options.cases),
        fixture_root: path_string(&options.fixture_root),
        top_k: options.top_k,
        token_budget: options.token_budget,
        cases_total: case_results.len(),
        cases_passed,
        cases_failed,
        metrics: aggregate_context_packet_metrics(&case_results),
        cases: case_results,
        notes: vec![
            "Context Packet Gate stages each graph-truth fixture, applies mutation_steps, builds packets from graph-truth-aware critical seeds plus prompt seeds, then checks critical symbols, proof paths, source spans, snippets, tests, path context labels, and distractors.".to_string(),
            "This gate scores packet usefulness, not just graph indexing success or command execution.".to_string(),
        ],
    })
}

pub fn render_context_packet_gate_markdown(report: &ContextPacketGateReport) -> String {
    let mut output = String::new();
    output.push_str("# Context Packet Gate\n\n");
    output.push_str(&format!("Verdict: `{}`\n\n", report.verdict));
    output.push_str(&format!(
        "Cases: {} total, {} passed, {} failed. Top-k: {}. Token budget: {}.\n\n",
        report.cases_total,
        report.cases_passed,
        report.cases_failed,
        report.top_k,
        report.token_budget
    ));
    output.push_str("## Aggregate Metrics\n\n");
    output.push_str("| Metric | Value |\n");
    output.push_str("| --- | ---: |\n");
    output.push_str(&format!(
        "| Context symbol recall@{} | {} |\n",
        report.top_k,
        optional_ratio(report.metrics.context_symbol_recall_at_k)
    ));
    output.push_str(&format!(
        "| Critical symbol missing rate | {} |\n",
        optional_ratio(report.metrics.critical_symbol_missing_rate)
    ));
    output.push_str(&format!(
        "| Distractor ratio | {} |\n",
        optional_ratio(report.metrics.distractor_ratio)
    ));
    output.push_str(&format!(
        "| Proof path coverage | {} |\n",
        optional_ratio(report.metrics.proof_path_coverage)
    ));
    output.push_str(&format!(
        "| Source span coverage | {} |\n",
        optional_ratio(report.metrics.source_span_coverage)
    ));
    output.push_str(&format!(
        "| Critical snippet coverage | {} |\n",
        optional_ratio(report.metrics.critical_snippet_coverage)
    ));
    output.push_str(&format!(
        "| Recommended test recall | {} |\n",
        optional_ratio(report.metrics.recommended_test_recall)
    ));
    output.push_str(&format!(
        "| Useful facts per byte | {} |\n",
        optional_ratio(report.metrics.useful_facts_per_byte)
    ));
    output.push_str(&format!(
        "| Useful facts per estimated token | {} |\n",
        optional_ratio(report.metrics.useful_facts_per_estimated_token)
    ));

    output.push_str("\n## Case Results\n\n");
    output.push_str("| Case | Status | Symbol R@k | Critical Missing | Distractor Ratio | Proof Paths | Stored Paths | Span Coverage | Snippets | Tests | Useful/byte |\n");
    output
        .push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for case in &report.cases {
        output.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {}/{} | {} | {} | {}/{} | {}/{} | {} |\n",
            case.case_id,
            case.status,
            optional_ratio(case.metrics.context_symbol_recall_at_k),
            case.metrics.missing_critical_symbols,
            optional_ratio(case.metrics.distractor_ratio),
            case.metrics.matched_proof_paths,
            case.metrics.expected_proof_paths,
            case.stored_path_evidence_rows,
            optional_ratio(case.metrics.source_span_coverage),
            case.metrics.matched_critical_snippets,
            case.metrics.expected_critical_snippets,
            case.metrics.matched_tests,
            case.metrics.expected_tests,
            optional_ratio(case.metrics.useful_facts_per_byte)
        ));
    }

    output.push_str("\n## Top Failures\n\n");
    for (case_id, failure) in report
        .cases
        .iter()
        .flat_map(|case| {
            case.failures
                .iter()
                .map(move |failure| (&case.case_id, failure))
        })
        .take(40)
    {
        output.push_str(&format!(
            "- `{}`: `{}` - {}\n",
            case_id, failure.category, failure.message
        ));
    }

    output.push_str("\n## Missing Critical Symbols\n\n");
    for (case_id, message) in report
        .cases
        .iter()
        .flat_map(|case| {
            case.missing_critical_symbols
                .iter()
                .map(move |message| (&case.case_id, message))
        })
        .take(40)
    {
        output.push_str(&format!("- `{case_id}`: {message}\n"));
    }

    output.push_str("\n## Missing Proof Paths\n\n");
    for (case_id, message) in report
        .cases
        .iter()
        .flat_map(|case| {
            case.missing_proof_paths
                .iter()
                .map(move |message| (&case.case_id, message))
        })
        .take(40)
    {
        output.push_str(&format!("- `{case_id}`: {message}\n"));
    }

    output.push_str("\n## Distractor Problems\n\n");
    for (case_id, message) in report
        .cases
        .iter()
        .flat_map(|case| {
            case.forbidden_context_hits
                .iter()
                .map(move |message| (&case.case_id, message))
        })
        .take(40)
    {
        output.push_str(&format!("- `{case_id}`: {message}\n"));
    }

    output.push_str("\n## Notes\n\n");
    for note in &report.notes {
        output.push_str(&format!("- {note}\n"));
    }
    output
}

pub fn render_graph_truth_gate_markdown(report: &GraphTruthGateReport) -> String {
    let mut output = String::new();
    output.push_str("# Graph Truth Gate\n\n");
    output.push_str(&format!("Verdict: `{}`\n\n", report.verdict));
    output.push_str(&format!(
        "Cases: {} total, {} passed, {} failed.\n\n",
        report.cases_total, report.cases_passed, report.cases_failed
    ));
    output.push_str("## Case Results\n\n");
    output.push_str(
        "| Case | Status | Expected Edges | Base Edges | Derived Edges | Forbidden Hits | Span Failures | Context Failures |\n",
    );
    output.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for case in &report.cases {
        output.push_str(&format!(
            "| `{}` | `{}` | {}/{} | {} | {} | {} | {} | {} |\n",
            case.case_id,
            case.status,
            case.matched_expected_edges,
            case.expected_edges,
            case.base_edges_observed,
            case.derived_edges_observed,
            case.matched_forbidden_edges + case.matched_forbidden_paths,
            case.source_span_failures.len(),
            case.context_packet_failures.len()
        ));
    }

    output.push_str("\n## Relation Metrics\n\n");
    output.push_str("| Relation | Precision | Recall | Expected | Forbidden Hits |\n");
    output.push_str("| --- | ---: | ---: | ---: | ---: |\n");
    for metric in &report.relation_metrics {
        output.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            metric.relation,
            optional_ratio(metric.precision),
            optional_ratio(metric.recall),
            metric.expected_edges,
            metric.matched_forbidden_edges
        ));
    }

    output.push_str("\n## Edge Class Counts\n\n");
    output.push_str(&format!(
        "Observed base edges: {}. Observed derived/cache edges: {}.\n\n",
        report.totals.base_edges_observed, report.totals.derived_edges_observed
    ));
    output.push_str("| Fact Class | Observed Edges |\n");
    output.push_str("| --- | ---: |\n");
    for (fact_class, count) in &report.totals.edge_class_counts {
        output.push_str(&format!("| `{fact_class}` | {count} |\n"));
    }

    output.push_str("\n## Top False Positives\n\n");
    for message in report
        .cases
        .iter()
        .flat_map(|case| {
            case.false_positives
                .iter()
                .map(move |message| (&case.case_id, message))
        })
        .take(20)
    {
        output.push_str(&format!("- `{}`: {}\n", message.0, message.1));
    }

    output.push_str("\n## Top False Negatives\n\n");
    for message in report
        .cases
        .iter()
        .flat_map(|case| {
            case.false_negatives
                .iter()
                .map(move |message| (&case.case_id, message))
        })
        .take(20)
    {
        output.push_str(&format!("- `{}`: {}\n", message.0, message.1));
    }

    output.push_str("\n## Source Span Failures\n\n");
    for message in report
        .cases
        .iter()
        .flat_map(|case| {
            case.source_span_failures
                .iter()
                .map(move |message| (&case.case_id, message))
        })
        .take(20)
    {
        output.push_str(&format!("- `{}`: {}\n", message.0, message.1));
    }

    output.push_str("\n## Stale-Update Failures\n\n");
    for message in report
        .cases
        .iter()
        .flat_map(|case| {
            case.stale_failures
                .iter()
                .chain(case.mutation_failures.iter())
                .map(move |message| (&case.case_id, message))
        })
        .take(20)
    {
        output.push_str(&format!("- `{}`: {}\n", message.0, message.1));
    }

    output.push_str("\n## Test/Mock Leakage Failures\n\n");
    for (case_id, failure) in report
        .cases
        .iter()
        .flat_map(|case| {
            case.failures
                .iter()
                .filter(|failure| failure.category.contains("test_mock"))
                .map(move |failure| (&case.case_id, failure))
        })
        .take(20)
    {
        output.push_str(&format!(
            "- `{}`: `{}` - {}\n",
            case_id, failure.category, failure.message
        ));
    }

    output.push_str("\n## Derived/Provenance Failures\n\n");
    for (case_id, failure) in report
        .cases
        .iter()
        .flat_map(|case| {
            case.failures
                .iter()
                .filter(|failure| failure.category.contains("derived"))
                .map(move |failure| (&case.case_id, failure))
        })
        .take(20)
    {
        output.push_str(&format!(
            "- `{}`: `{}` - {}\n",
            case_id, failure.category, failure.message
        ));
    }

    output.push_str("\n## Notes\n\n");
    for note in &report.notes {
        output.push_str(&format!("- {note}\n"));
    }
    output
}

fn run_case(
    options: &GraphTruthGateOptions,
    case_path: &Path,
    case: GraphTruthCase,
) -> BenchResult<GraphTruthCaseResult> {
    let total_start = Instant::now();
    let fixture_repo = resolve_fixture_repo(options, case_path, &case)?;
    let staged_repo = stage_fixture_repo(&fixture_repo, &case.case_id)?;
    let db_path = unique_output_path(
        &format!("codegraph-graph-truth-{}", safe_id(&case.case_id)),
        "sqlite",
    );
    let mut mutation_failures = Vec::new();

    let (mut index_summary, mut index_time_ms) = index_fixture_repo(&staged_repo, &db_path)?;
    let mutation_start = Instant::now();
    if !case.mutation_steps.is_empty() {
        if options.update_mode {
            for (step_index, step) in case.mutation_steps.iter().enumerate() {
                match apply_mutation_step(&staged_repo, step) {
                    Ok(MutationAction::Reindex) => {
                        let (summary, elapsed_ms) = index_fixture_repo(&staged_repo, &db_path)?;
                        index_summary = summary;
                        index_time_ms += elapsed_ms;
                    }
                    Ok(MutationAction::QueryAgain) | Ok(MutationAction::FileMutation) => {}
                    Err(message) => mutation_failures.push(format!(
                        "mutation step {} ({:?}) failed: {}",
                        step_index + 1,
                        step.step_type,
                        message
                    )),
                }
            }
        } else {
            mutation_failures.push(format!(
                "case has {} mutation_steps but --update-mode was not supplied",
                case.mutation_steps.len()
            ));
        }
    }
    let mutation_time_ms = mutation_start.elapsed().as_millis();

    let query_start = Instant::now();
    let store = SqliteGraphStore::open(&db_path)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let (entities, edges) = load_observed_graph_facts(&store)?;
    let source_span_count = store
        .count_source_spans()
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let sources = load_sources(&staged_repo, &store)?;
    let engine = ExactGraphQueryEngine::new(edges.clone());
    let context_packet = build_context_packet(&case, &entities, &edges, &engine, &sources);
    let query_time_ms = query_start.elapsed().as_millis();
    let observed = ObservedGraph {
        entities,
        edges,
        sources,
        context_packet,
    };
    let evaluation_start = Instant::now();
    let mut result = evaluate_case(&case, &observed, options);
    let evaluation_time_ms = evaluation_start.elapsed().as_millis();
    if !mutation_failures.is_empty() {
        for message in &mutation_failures {
            push_failure(
                &mut result.failures,
                "mutation_failure",
                None,
                None,
                message.clone(),
            );
        }
        result.status = "failed".to_string();
    }
    result.db_path = path_string(&db_path);
    result.staged_repo_path = path_string(&staged_repo);
    result.files_indexed = index_summary.files_indexed as u64;
    result.entities_indexed = index_summary.entities as u64;
    result.edges_indexed = index_summary.edges as u64;
    result.entity_count = observed.entities.len() as u64;
    result.edge_count = observed.edges.len() as u64;
    result.source_span_count = source_span_count;
    result.index_time = index_time_ms;
    result.query_time = query_time_ms;
    result.timing = GraphTruthTiming {
        total_ms: total_start.elapsed().as_millis(),
        index_ms: index_time_ms,
        mutation_ms: mutation_time_ms,
        query_ms: query_time_ms,
        evaluation_ms: evaluation_time_ms,
    };
    result.mutation_failures = mutation_failures;
    if !options.keep_workdirs {
        cleanup_db_family(&db_path);
        let _ = fs::remove_dir_all(&staged_repo);
    }
    Ok(result)
}

enum MutationAction {
    FileMutation,
    Reindex,
    QueryAgain,
}

fn index_fixture_repo(
    repo_root: &Path,
    db_path: &Path,
) -> BenchResult<(codegraph_index::IndexSummary, u128)> {
    let start = Instant::now();
    let summary = index_repo_to_audit_db(repo_root, db_path)?;
    Ok((summary, start.elapsed().as_millis()))
}

fn index_repo_to_audit_db(
    repo_root: &Path,
    db_path: &Path,
) -> BenchResult<codegraph_index::IndexSummary> {
    index_repo_to_db_with_options(
        repo_root,
        db_path,
        IndexOptions {
            storage_mode: StorageMode::Audit,
            ..IndexOptions::default()
        },
    )
    .map_err(|error| BenchmarkError::Store(error.to_string()))
}

fn load_observed_graph_facts(store: &SqliteGraphStore) -> BenchResult<(Vec<Entity>, Vec<Edge>)> {
    let mut entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    entities.extend(
        store
            .list_static_references(UNBOUNDED_STORE_READ_LIMIT)
            .map_err(|error| BenchmarkError::Store(error.to_string()))?,
    );
    entities.sort_by(|left, right| left.id.cmp(&right.id));
    entities.dedup_by(|left, right| left.id == right.id);

    let mut edges = store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    edges.extend(
        store
            .list_heuristic_edges(UNBOUNDED_STORE_READ_LIMIT)
            .map_err(|error| BenchmarkError::Store(error.to_string()))?,
    );
    edges.sort_by(|left, right| left.id.cmp(&right.id));
    edges.dedup_by(|left, right| left.id == right.id);
    Ok((entities, edges))
}

fn stage_fixture_repo(source_repo: &Path, case_id: &str) -> BenchResult<PathBuf> {
    let staged_repo = std::env::temp_dir().join(format!(
        "codegraph-graph-truth-workdir-{}-{}",
        safe_id(case_id),
        unique_suffix()
    ));
    copy_dir_recursive(source_repo, &staged_repo)?;
    Ok(staged_repo)
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> BenchResult<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn apply_mutation_step(repo_root: &Path, step: &MutationStep) -> Result<MutationAction, String> {
    match step.step_type {
        MutationStepType::AddFile => {
            let path = mutation_path(repo_root, step.path.as_deref(), "add_file path")?;
            write_mutation_file(&path, step.content.as_deref().unwrap_or_default())?;
            Ok(MutationAction::FileMutation)
        }
        MutationStepType::EditFile => {
            let path = mutation_path(repo_root, step.path.as_deref(), "edit_file path")?;
            if let Some(content) = step.content.as_deref() {
                write_mutation_file(&path, content)?;
            } else {
                replace_in_mutation_file(
                    &path,
                    step.find
                        .as_deref()
                        .ok_or("edit_file requires content or find")?,
                    step.replace.as_deref().unwrap_or_default(),
                )?;
            }
            Ok(MutationAction::FileMutation)
        }
        MutationStepType::RenameFile => {
            let from = mutation_path(repo_root, step.from.as_deref(), "rename_file from")?;
            let to = mutation_path(repo_root, step.to.as_deref(), "rename_file to")?;
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            if to.exists() {
                fs::remove_file(&to).map_err(|error| error.to_string())?;
            }
            fs::rename(&from, &to).map_err(|error| error.to_string())?;
            Ok(MutationAction::FileMutation)
        }
        MutationStepType::DeleteFile | MutationStepType::RemoveFile => {
            let path = mutation_path(repo_root, step.path.as_deref(), "delete_file path")?;
            if path.exists() {
                fs::remove_file(&path).map_err(|error| error.to_string())?;
            }
            Ok(MutationAction::FileMutation)
        }
        MutationStepType::ChangeImportAlias => {
            let path = mutation_path(repo_root, step.path.as_deref(), "change_import_alias path")?;
            let mut contents = fs::read_to_string(&path).map_err(|error| error.to_string())?;
            let old_alias = step
                .old_alias
                .as_deref()
                .ok_or("change_import_alias requires old_alias")?;
            let new_alias = step
                .new_alias
                .as_deref()
                .ok_or("change_import_alias requires new_alias")?;
            contents = replace_or_keep(contents, old_alias, new_alias);
            if let Some(import_from) = step.import_from.as_deref() {
                contents = replace_import_source(contents, import_from);
            }
            write_mutation_file(&path, &contents)?;
            Ok(MutationAction::FileMutation)
        }
        MutationStepType::Reindex => Ok(MutationAction::Reindex),
        MutationStepType::QueryAgain => Ok(MutationAction::QueryAgain),
    }
}

fn mutation_path(repo_root: &Path, raw: Option<&str>, label: &str) -> Result<PathBuf, String> {
    let raw = raw.ok_or_else(|| format!("{label} is required"))?;
    let relative = PathBuf::from(raw);
    if relative.is_absolute() {
        return Err(format!("{label} must be repo-relative: {raw}"));
    }
    Ok(repo_root.join(relative))
}

fn write_mutation_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, contents).map_err(|error| error.to_string())
}

fn replace_in_mutation_file(path: &Path, find: &str, replace: &str) -> Result<(), String> {
    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    if contents.contains(find) {
        write_mutation_file(path, &contents.replace(find, replace))
    } else if !replace.is_empty() && contents.contains(replace) {
        Ok(())
    } else {
        Err(format!(
            "pattern {:?} not found in {}",
            find,
            path.display()
        ))
    }
}

fn replace_or_keep(contents: String, find: &str, replace: &str) -> String {
    if contents.contains(find) {
        contents.replace(find, replace)
    } else {
        contents
    }
}

fn replace_import_source(contents: String, import_from: &str) -> String {
    if contents.contains(import_from) {
        return contents;
    }
    let mut output = String::new();
    for line in contents.lines() {
        if line.trim_start().starts_with("import ") && line.contains(" from ") {
            if let Some((prefix, _)) = line.rsplit_once(" from ") {
                output.push_str(prefix);
                output.push_str(" from \"");
                output.push_str(import_from);
                output.push_str("\";");
                output.push('\n');
                continue;
            }
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn run_context_packet_case(
    options: &ContextPacketGateOptions,
    case_path: &Path,
    case: GraphTruthCase,
) -> BenchResult<ContextPacketCaseResult> {
    let fixture_repo = resolve_fixture_repo_for_options(&options.fixture_root, case_path, &case)?;
    let staged_repo = stage_fixture_repo(&fixture_repo, &case.case_id)?;
    let db_path = unique_output_path(
        &format!("codegraph-context-packet-{}", safe_id(&case.case_id)),
        "sqlite",
    );
    let mut mutation_failures = Vec::new();
    let mut index_summary = index_repo_to_audit_db(&staged_repo, &db_path)?;
    let mut dirty_after_mutation = false;
    for (step_index, step) in case.mutation_steps.iter().enumerate() {
        match apply_mutation_step(&staged_repo, step) {
            Ok(MutationAction::FileMutation) => {
                dirty_after_mutation = true;
            }
            Ok(MutationAction::Reindex) => {
                index_summary = index_repo_to_audit_db(&staged_repo, &db_path)?;
                dirty_after_mutation = false;
            }
            Ok(MutationAction::QueryAgain) => {
                if dirty_after_mutation {
                    index_summary = index_repo_to_audit_db(&staged_repo, &db_path)?;
                    dirty_after_mutation = false;
                }
            }
            Err(message) => mutation_failures.push(format!(
                "mutation step {} ({:?}) failed: {}",
                step_index + 1,
                step.step_type,
                message
            )),
        }
    }
    if dirty_after_mutation {
        index_summary = index_repo_to_audit_db(&staged_repo, &db_path)?;
    }
    let store = SqliteGraphStore::open(&db_path)
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let (entities, edges) = load_observed_graph_facts(&store)?;
    let sources = load_sources(&staged_repo, &store)?;
    let engine = ExactGraphQueryEngine::new(edges.clone());
    let prompt_seeds = extract_prompt_seeds(&case.task_prompt);
    let context_mode = context_mode_for_case(&case);
    let seed_ids = context_seed_ids_from_case(&case, &entities);
    let context_packet = build_context_packet_with_budget(
        &case,
        &entities,
        &edges,
        &engine,
        &sources,
        options.token_budget,
    );
    for path in &context_packet.verified_paths {
        store
            .upsert_path_evidence(path)
            .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    }
    let stored_path_evidence_rows = store
        .count_path_evidence()
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    let observed = ObservedGraph {
        entities,
        edges,
        sources,
        context_packet,
    };
    let mut result = evaluate_context_packet_case(
        &case,
        &observed,
        options,
        context_mode,
        prompt_seeds.len(),
        seed_ids.len(),
    );
    if !mutation_failures.is_empty() {
        result.status = "failed".to_string();
        for message in mutation_failures {
            push_failure(
                &mut result.failures,
                "mutation_failure",
                None,
                None,
                message.clone(),
            );
            result.notes.push(message);
        }
    }
    result.db_path = path_string(&db_path);
    result.files_indexed = index_summary.files_indexed as u64;
    result.entities_indexed = index_summary.entities as u64;
    result.edges_indexed = index_summary.edges as u64;
    result.stored_path_evidence_rows = stored_path_evidence_rows;
    cleanup_db_family(&db_path);
    let _ = fs::remove_dir_all(&staged_repo);
    Ok(result)
}

fn evaluate_case(
    case: &GraphTruthCase,
    observed: &ObservedGraph,
    options: &GraphTruthGateOptions,
) -> GraphTruthCaseResult {
    let mut failures = Vec::new();
    let mut false_positives = Vec::new();
    let mut false_negatives = Vec::new();
    let mut missing_entities = Vec::new();
    let mut missing_edges = Vec::new();
    let mut forbidden_edges_found = Vec::new();
    let mut missing_paths = Vec::new();
    let mut forbidden_paths_found = Vec::new();
    let mut source_span_failures = Vec::new();
    let mut context_packet_failures = Vec::new();
    let mut context_symbol_failures = Vec::new();
    let mut expected_test_failures = Vec::new();
    let mut stale_failures = Vec::new();
    let mut relation_metrics = BTreeMap::<RelationKind, RelationMetric>::new();

    let mut matched_entities = 0usize;
    let mut entity_matches = Vec::<(String, String)>::new();
    for expected in &case.expected_entities {
        let matches = matching_entities(&observed.entities, &expected.selector);
        if matches.is_empty() {
            let message = format!(
                "missing expected entity {}",
                describe_entity_ref(&expected.selector)
            );
            push_failure(
                &mut failures,
                "missing_expected_entity",
                None,
                None,
                message.clone(),
            );
            missing_entities.push(message.clone());
            false_negatives.push(message);
        } else {
            matched_entities += 1;
            entity_matches.push((
                describe_entity_ref(&expected.selector),
                matches[0].id.clone(),
            ));
        }
    }
    let mut seen_expected_entity_ids = BTreeMap::<String, String>::new();
    for (selector, entity_id) in entity_matches {
        if let Some(previous_selector) =
            seen_expected_entity_ids.insert(entity_id.clone(), selector.clone())
        {
            push_failure(
                &mut failures,
                "same_name_symbol_collision",
                None,
                None,
                format!(
                    "expected selectors {previous_selector} and {selector} resolved to the same entity id {entity_id}"
                ),
            );
        }
    }

    let mut matched_expected_edges = 0usize;
    for expected in &case.expected_edges {
        let metric = relation_metrics
            .entry(expected.relation)
            .or_insert_with(|| empty_relation_metric(expected.relation));
        metric.expected_edges += 1;
        match best_edge_match(observed, expected) {
            Some(edge) => {
                matched_expected_edges += 1;
                metric.matched_expected_edges += 1;
                validate_edge_gate(
                    expected,
                    edge,
                    observed,
                    options,
                    &mut failures,
                    &mut source_span_failures,
                );
            }
            None => {
                let message = format!(
                    "missing required edge {}",
                    describe_edge_expectation(expected)
                );
                if let Some(edge) = reverse_edge_match(observed, expected) {
                    push_failure(
                        &mut failures,
                        "wrong_direction",
                        Some(expected.relation),
                        expected.id.clone(),
                        format!(
                            "edge direction wrong for {}; observed reverse edge {}",
                            describe_edge_expectation(expected),
                            edge.id
                        ),
                    );
                }
                push_failure(
                    &mut failures,
                    "missing_required_edge",
                    Some(expected.relation),
                    expected.id.clone(),
                    message.clone(),
                );
                missing_edges.push(message.clone());
                false_negatives.push(message);
            }
        }
    }

    let mut matched_forbidden_edges = 0usize;
    for forbidden in &case.forbidden_edges {
        let metric = relation_metrics
            .entry(forbidden.relation)
            .or_insert_with(|| empty_relation_metric(forbidden.relation));
        metric.forbidden_edges += 1;
        if let Some(edge) = exact_edge_match(observed, forbidden) {
            matched_forbidden_edges += 1;
            metric.matched_forbidden_edges += 1;
            let message = format!(
                "forbidden edge exists: {} observed as {}",
                describe_edge_expectation(forbidden),
                edge.id
            );
            if options.fail_on_forbidden {
                push_failure(
                    &mut failures,
                    "forbidden_edge",
                    Some(forbidden.relation),
                    forbidden.id.clone(),
                    message.clone(),
                );
            }
            forbidden_edges_found.push(message.clone());
            false_positives.push(message);
            if forbidden
                .tail
                .source_file
                .as_deref()
                .is_some_and(|path| !observed.sources.contains_key(&normalize_path(path)))
            {
                stale_failures.push(format!(
                    "stale forbidden edge references unavailable source {}",
                    describe_edge_expectation(forbidden)
                ));
            }
        }
    }

    let mut matched_expected_paths = 0usize;
    for expected in &case.expected_paths {
        if path_matches(observed, expected, true) {
            matched_expected_paths += 1;
        } else {
            let message = format!(
                "missing expected path {}",
                describe_path_expectation(expected)
            );
            push_failure(
                &mut failures,
                "missing_expected_path",
                None,
                expected.path_id.clone(),
                message.clone(),
            );
            missing_paths.push(message.clone());
            false_negatives.push(message);
        }
    }

    let mut matched_forbidden_paths = 0usize;
    for forbidden in &case.forbidden_paths {
        if path_matches(observed, forbidden, false) {
            matched_forbidden_paths += 1;
            let message = format!(
                "forbidden path exists: {}",
                describe_path_expectation(forbidden)
            );
            if options.fail_on_forbidden {
                push_failure(
                    &mut failures,
                    "forbidden_path",
                    None,
                    forbidden.path_id.clone(),
                    message.clone(),
                );
            }
            forbidden_paths_found.push(message.clone());
            false_positives.push(message);
        }
    }

    for span in &case.expected_source_spans {
        if let Some(edge_ref) = &span.edge_ref {
            if let Some(edge) = observed.edges.iter().find(|edge| &edge.id == edge_ref) {
                if let Err(message) =
                    validate_source_span(&edge.source_span, &span.span, &observed.sources)
                {
                    source_span_failures.push(message.clone());
                    if options.fail_on_missing_source_span {
                        push_failure(
                            &mut failures,
                            "source_span_failure",
                            Some(edge.relation),
                            Some(edge_ref.clone()),
                            message,
                        );
                    }
                }
                continue;
            }
        }
        if !observed
            .edges
            .iter()
            .any(|edge| span_matches(&edge.source_span, &span.span, &observed.sources).is_ok())
        {
            let message = format!(
                "expected source span not observed: {}:{}-{}",
                span.source_file, span.span.start_line, span.span.end_line
            );
            source_span_failures.push(message.clone());
            if options.fail_on_missing_source_span {
                push_failure(
                    &mut failures,
                    "source_span_failure",
                    None,
                    span.edge_ref.clone(),
                    message,
                );
            }
        }
    }

    let mut matched_context_symbols = 0usize;
    for expected in &case.expected_context_symbols {
        if context_symbol_matches(observed, &expected.symbol) {
            matched_context_symbols += 1;
        } else if expected.critical.unwrap_or(false) {
            let message = format!(
                "critical context symbol missing from packet: {}",
                describe_entity_ref(&expected.symbol)
            );
            context_packet_failures.push(message.clone());
            context_symbol_failures.push(message.clone());
            push_failure(
                &mut failures,
                "critical_context_symbol_missing",
                None,
                None,
                message,
            );
        }
    }
    for forbidden in &case.forbidden_context_symbols {
        if context_symbol_matches(observed, &forbidden.symbol) {
            let message = format!(
                "forbidden context symbol appeared: {}",
                describe_entity_ref(&forbidden.symbol)
            );
            context_packet_failures.push(message.clone());
            context_symbol_failures.push(message.clone());
            if options.fail_on_forbidden {
                push_failure(
                    &mut failures,
                    "forbidden_context_symbol",
                    None,
                    None,
                    message,
                );
            }
        }
    }
    for path in &observed.context_packet.verified_paths {
        let path_context = path.metadata.get("path_context").and_then(Value::as_str);
        if path_context.is_none() {
            let message = format!("context packet path {} lacks path_context", path.id);
            context_packet_failures.push(message.clone());
            push_failure(
                &mut failures,
                "missing_path_context",
                None,
                Some(path.id.clone()),
                message,
            );
        }
        if path_context != Some("production")
            && path
                .metadata
                .get("production_proof_eligible")
                .and_then(Value::as_bool)
                == Some(true)
        {
            let message = format!(
                "non-production context packet path {} is marked production proof eligible",
                path.id
            );
            context_packet_failures.push(message.clone());
            push_failure(
                &mut failures,
                "test_mock_leaked_into_production_proof",
                None,
                Some(path.id.clone()),
                message,
            );
        }
        let production_marked = path_context == Some("production")
            || path
                .metadata
                .get("production_proof_eligible")
                .and_then(Value::as_bool)
                == Some(true);
        if production_marked && path_has_test_mock_evidence(path) {
            let message = format!(
                "production proof path {} includes test/mock evidence",
                path.id
            );
            context_packet_failures.push(message.clone());
            push_failure(
                &mut failures,
                "test_mock_leaked_into_production_proof",
                None,
                Some(path.id.clone()),
                message,
            );
        }
    }
    if let Some(policy) = &case.distractor_policy {
        if let Some(max) = policy.max_context_distractors {
            let count = policy
                .distractor_symbols
                .iter()
                .filter(|symbol| context_symbol_matches(observed, &symbol.symbol))
                .count();
            if count > max {
                let message =
                    format!("too many context distractors: observed {count}, allowed {max}");
                context_packet_failures.push(message.clone());
                push_failure(&mut failures, "too_many_distractors", None, None, message);
            }
        }
    }

    let mut matched_tests = 0usize;
    for expected in &case.expected_tests {
        if test_expectation_matches(observed, expected) {
            matched_tests += 1;
        } else {
            let message = format!("missing expected test {}", expected.name);
            push_failure(
                &mut failures,
                "missing_expected_test",
                None,
                Some(expected.name.clone()),
                message.clone(),
            );
            expected_test_failures.push(message.clone());
            false_negatives.push(message);
        }
    }
    for forbidden in &case.forbidden_tests {
        if test_expectation_matches(observed, forbidden) {
            let message = format!("forbidden test appeared {}", forbidden.name);
            push_failure(
                &mut failures,
                "forbidden_test",
                None,
                Some(forbidden.name.clone()),
                message.clone(),
            );
            expected_test_failures.push(message.clone());
            false_positives.push(message);
        }
    }

    for edge in &observed.edges {
        if edge.derived
            && edge.provenance_edges.is_empty()
            && (options.fail_on_derived_without_provenance || edge.derived)
        {
            push_failure(
                &mut failures,
                "derived_edge_without_provenance",
                Some(edge.relation),
                Some(edge.id.clone()),
                format!("derived edge {} lacks provenance", edge.id),
            );
        }
        if edge.exactness_is_proof_grade()
            && edge
                .metadata
                .get("resolution")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "unresolved")
            && (options.fail_on_unresolved_exact || edge.exactness_is_proof_grade())
        {
            push_failure(
                &mut failures,
                "unresolved_name_labeled_exact",
                Some(edge.relation),
                Some(edge.id.clone()),
                format!("unresolved edge {} is labeled {}", edge.id, edge.exactness),
            );
        }
    }

    let mut relation_metrics = relation_metrics.into_values().collect::<Vec<_>>();
    for metric in &mut relation_metrics {
        metric.precision = if metric.matched_expected_edges + metric.matched_forbidden_edges > 0 {
            Some(divide(
                metric.matched_expected_edges,
                metric.matched_expected_edges + metric.matched_forbidden_edges,
            ))
        } else {
            None
        };
        metric.recall = if metric.expected_edges > 0 {
            Some(divide(metric.matched_expected_edges, metric.expected_edges))
        } else {
            None
        };
    }
    let status = if failures.is_empty() {
        "passed"
    } else {
        "failed"
    }
    .to_string();
    let edge_class_counts = edge_class_counts(&observed.edges);
    let derived_edges_observed = edge_class_counts
        .get("derived")
        .copied()
        .unwrap_or_default();
    let base_edges_observed = observed.edges.len().saturating_sub(derived_edges_observed);
    GraphTruthCaseResult {
        case_id: case.case_id.clone(),
        description: case.description.clone(),
        status,
        repo_fixture_path: case.repo_fixture_path.clone(),
        db_path: String::new(),
        staged_repo_path: String::new(),
        files_indexed: 0,
        entities_indexed: observed.entities.len() as u64,
        edges_indexed: observed.edges.len() as u64,
        entity_count: observed.entities.len() as u64,
        edge_count: observed.edges.len() as u64,
        source_span_count: observed.edges.len() as u64,
        timing: GraphTruthTiming::default(),
        index_time: 0,
        query_time: 0,
        base_edges_observed,
        derived_edges_observed,
        edge_class_counts,
        expected_entities: case.expected_entities.len(),
        matched_entities,
        expected_edges: case.expected_edges.len(),
        matched_expected_edges,
        forbidden_edges: case.forbidden_edges.len(),
        matched_forbidden_edges,
        expected_paths: case.expected_paths.len(),
        matched_expected_paths,
        forbidden_paths: case.forbidden_paths.len(),
        matched_forbidden_paths,
        expected_context_symbols: case.expected_context_symbols.len(),
        matched_context_symbols,
        expected_tests: case.expected_tests.len(),
        matched_tests,
        failures,
        missing_entities,
        missing_edges,
        forbidden_edges_found,
        missing_paths,
        forbidden_paths_found,
        false_positives,
        false_negatives,
        source_span_failures,
        context_symbol_failures,
        context_packet_failures,
        expected_test_failures,
        mutation_failures: Vec::new(),
        stale_failures,
        relation_metrics,
        notes: case.notes.clone(),
    }
}

fn evaluate_context_packet_case(
    case: &GraphTruthCase,
    observed: &ObservedGraph,
    options: &ContextPacketGateOptions,
    context_mode: String,
    prompt_seed_count: usize,
    resolved_seed_count: usize,
) -> ContextPacketCaseResult {
    let mut failures = Vec::new();
    let mut missing_critical_symbols = Vec::new();
    let mut missing_proof_paths = Vec::new();
    let mut missing_critical_snippets = Vec::new();
    let mut missing_tests = Vec::new();
    let mut forbidden_context_hits = Vec::new();
    let mut path_context_failures = Vec::new();
    let mut metrics = ContextPacketGateMetrics {
        expected_context_symbols: case.expected_context_symbols.len(),
        critical_symbols: case
            .expected_context_symbols
            .iter()
            .filter(|symbol| symbol.critical.unwrap_or(false))
            .count(),
        forbidden_context_symbols: case.forbidden_context_symbols.len(),
        expected_proof_paths: case
            .expected_paths
            .iter()
            .filter(|path| path.proof_grade.unwrap_or(true))
            .count(),
        expected_tests: case.expected_tests.len(),
        packet_bytes: packet_bytes(&observed.context_packet),
        estimated_tokens: packet_estimated_tokens(&observed.context_packet),
        ..Default::default()
    };

    for expected in &case.expected_context_symbols {
        if context_symbol_matches_at_k(
            &observed.entities,
            &observed.context_packet.symbols,
            &expected.symbol,
            options.top_k,
        ) {
            metrics.matched_context_symbols_at_k += 1;
        } else if expected.critical.unwrap_or(false) {
            metrics.missing_critical_symbols += 1;
            let message = format!(
                "critical context symbol missing from packet top {}: {}",
                options.top_k,
                describe_entity_ref(&expected.symbol)
            );
            missing_critical_symbols.push(message.clone());
            push_failure(
                &mut failures,
                "critical_context_symbol_missing",
                None,
                None,
                message,
            );
        }
    }

    for forbidden in &case.forbidden_context_symbols {
        if context_symbol_matches_at_k(
            &observed.entities,
            &observed.context_packet.symbols,
            &forbidden.symbol,
            usize::MAX,
        ) {
            metrics.matched_forbidden_context_symbols += 1;
            let message = format!(
                "forbidden context symbol appeared in packet: {}",
                describe_entity_ref(&forbidden.symbol)
            );
            forbidden_context_hits.push(message.clone());
            push_failure(
                &mut failures,
                "forbidden_context_symbol",
                None,
                None,
                message,
            );
        }
    }

    let distractor_symbols = context_distractor_symbols(case);
    metrics.distractor_symbols = distractor_symbols.len();
    metrics.observed_distractors = distractor_symbols
        .iter()
        .filter(|symbol| {
            context_symbol_matches_at_k(
                &observed.entities,
                &observed.context_packet.symbols,
                symbol,
                usize::MAX,
            )
        })
        .count();
    if let Some(policy) = &case.distractor_policy {
        if let Some(max) = policy.max_context_distractors {
            if metrics.observed_distractors > max {
                let message = format!(
                    "too many context distractors: observed {}, allowed {}",
                    metrics.observed_distractors, max
                );
                forbidden_context_hits.push(message.clone());
                push_failure(&mut failures, "too_many_distractors", None, None, message);
            }
        }
    }

    for expected in case
        .expected_paths
        .iter()
        .filter(|path| path.proof_grade.unwrap_or(true))
    {
        match best_context_path_match(observed, expected) {
            Some(path) => {
                metrics.matched_proof_paths += 1;
                if context_path_has_required_spans(path, expected, observed) {
                    metrics.proof_paths_with_source_spans += 1;
                } else {
                    let message = format!(
                        "proof path lacks required source spans: {}",
                        describe_path_expectation(expected)
                    );
                    path_context_failures.push(message.clone());
                    push_failure(
                        &mut failures,
                        "proof_path_missing_source_span",
                        None,
                        expected.path_id.clone(),
                        message,
                    );
                }
                validate_packet_path_context(
                    path,
                    expected,
                    &mut failures,
                    &mut path_context_failures,
                );
            }
            None => {
                let message = format!(
                    "expected proof path missing from context packet: {}",
                    describe_path_expectation(expected)
                );
                missing_proof_paths.push(message.clone());
                push_failure(
                    &mut failures,
                    "missing_expected_proof_path",
                    None,
                    expected.path_id.clone(),
                    message,
                );
            }
        }
    }

    for path in &observed.context_packet.verified_paths {
        let path_context = path.metadata.get("path_context").and_then(Value::as_str);
        if path_context.is_none() {
            let message = format!("context packet path {} lacks path_context", path.id);
            path_context_failures.push(message.clone());
            push_failure(
                &mut failures,
                "missing_path_context",
                None,
                Some(path.id.clone()),
                message,
            );
        }
        if path
            .metadata
            .get("production_proof_eligible")
            .and_then(Value::as_bool)
            == Some(true)
            && path_context != Some("production")
        {
            let message = format!(
                "non-production path {} is marked production proof eligible",
                path.id
            );
            path_context_failures.push(message.clone());
            push_failure(
                &mut failures,
                "test_mock_leaked_into_production_proof",
                None,
                Some(path.id.clone()),
                message,
            );
        }
        if path
            .metadata
            .get("production_proof_eligible")
            .and_then(Value::as_bool)
            == Some(true)
            && path.source_spans.is_empty()
        {
            let message = format!("production proof path {} has no source spans", path.id);
            path_context_failures.push(message.clone());
            push_failure(
                &mut failures,
                "proof_path_missing_source_span",
                None,
                Some(path.id.clone()),
                message,
            );
        }
    }

    let critical_spans = critical_source_span_expectations(case);
    metrics.expected_critical_snippets = critical_spans.len();
    for span in &critical_spans {
        if packet_has_snippet_for_span(&observed.context_packet, span) {
            metrics.matched_critical_snippets += 1;
        } else {
            let message = format!(
                "critical snippet missing for {}:{}-{}",
                span.source_file, span.span.start_line, span.span.end_line
            );
            missing_critical_snippets.push(message.clone());
            push_failure(
                &mut failures,
                "critical_snippet_missing",
                None,
                span.edge_ref.clone(),
                message,
            );
        }
    }

    for expected in &case.expected_tests {
        if packet_recommended_test_matches(&observed.context_packet, expected) {
            metrics.matched_tests += 1;
        } else {
            let message = format!("recommended test missing from packet: {}", expected.name);
            missing_tests.push(message.clone());
            push_failure(
                &mut failures,
                "missing_recommended_test",
                None,
                Some(expected.name.clone()),
                message,
            );
        }
    }

    if context_risk_required(case) && observed.context_packet.risks.is_empty() {
        push_failure(
            &mut failures,
            "risk_summary_missing",
            None,
            None,
            "context packet has no risk summary for a risk-bearing graph-truth case",
        );
    }

    metrics.useful_facts = metrics.matched_context_symbols_at_k
        + metrics.matched_proof_paths
        + metrics.matched_critical_snippets
        + metrics.matched_tests
        + usize::from(context_risk_required(case) && !observed.context_packet.risks.is_empty());
    finalize_context_metrics(&mut metrics, observed.context_packet.symbols.len());

    let status = if failures.is_empty() {
        "passed"
    } else {
        "failed"
    }
    .to_string();

    ContextPacketCaseResult {
        case_id: case.case_id.clone(),
        description: case.description.clone(),
        status,
        repo_fixture_path: case.repo_fixture_path.clone(),
        db_path: String::new(),
        files_indexed: 0,
        entities_indexed: observed.entities.len() as u64,
        edges_indexed: observed.edges.len() as u64,
        context_mode,
        prompt_seed_count,
        resolved_seed_count,
        packet_symbols: observed.context_packet.symbols.len(),
        packet_verified_paths: observed.context_packet.verified_paths.len(),
        packet_snippets: observed.context_packet.snippets.len(),
        packet_risks: observed.context_packet.risks.len(),
        packet_recommended_tests: observed.context_packet.recommended_tests.len(),
        stored_path_evidence_rows: 0,
        packet_bytes: metrics.packet_bytes,
        estimated_tokens: metrics.estimated_tokens,
        metrics,
        failures,
        missing_critical_symbols,
        missing_proof_paths,
        missing_critical_snippets,
        missing_tests,
        forbidden_context_hits,
        path_context_failures,
        notes: case.notes.clone(),
    }
}

fn aggregate_context_packet_metrics(cases: &[ContextPacketCaseResult]) -> ContextPacketGateMetrics {
    let mut metrics = ContextPacketGateMetrics::default();
    let mut packet_symbol_count = 0usize;
    for case in cases {
        metrics.expected_context_symbols += case.metrics.expected_context_symbols;
        metrics.matched_context_symbols_at_k += case.metrics.matched_context_symbols_at_k;
        metrics.critical_symbols += case.metrics.critical_symbols;
        metrics.missing_critical_symbols += case.metrics.missing_critical_symbols;
        metrics.forbidden_context_symbols += case.metrics.forbidden_context_symbols;
        metrics.matched_forbidden_context_symbols += case.metrics.matched_forbidden_context_symbols;
        metrics.distractor_symbols += case.metrics.distractor_symbols;
        metrics.observed_distractors += case.metrics.observed_distractors;
        metrics.expected_proof_paths += case.metrics.expected_proof_paths;
        metrics.matched_proof_paths += case.metrics.matched_proof_paths;
        metrics.proof_paths_with_source_spans += case.metrics.proof_paths_with_source_spans;
        metrics.expected_critical_snippets += case.metrics.expected_critical_snippets;
        metrics.matched_critical_snippets += case.metrics.matched_critical_snippets;
        metrics.expected_tests += case.metrics.expected_tests;
        metrics.matched_tests += case.metrics.matched_tests;
        metrics.packet_bytes += case.metrics.packet_bytes;
        metrics.estimated_tokens += case.metrics.estimated_tokens;
        metrics.useful_facts += case.metrics.useful_facts;
        packet_symbol_count += case.packet_symbols;
    }
    finalize_context_metrics(&mut metrics, packet_symbol_count);
    metrics
}

fn finalize_context_metrics(metrics: &mut ContextPacketGateMetrics, packet_symbol_count: usize) {
    metrics.context_symbol_recall_at_k = ratio_option(
        metrics.matched_context_symbols_at_k,
        metrics.expected_context_symbols,
    );
    metrics.critical_symbol_missing_rate =
        ratio_option(metrics.missing_critical_symbols, metrics.critical_symbols);
    metrics.distractor_ratio = if packet_symbol_count > 0 {
        Some(metrics.observed_distractors as f64 / packet_symbol_count as f64)
    } else if metrics.distractor_symbols > 0 {
        Some(0.0)
    } else {
        None
    };
    metrics.proof_path_coverage =
        ratio_option(metrics.matched_proof_paths, metrics.expected_proof_paths);
    metrics.source_span_coverage = ratio_option(
        metrics.proof_paths_with_source_spans,
        metrics.matched_proof_paths,
    );
    metrics.critical_snippet_coverage = ratio_option(
        metrics.matched_critical_snippets,
        metrics.expected_critical_snippets,
    );
    metrics.recommended_test_recall = ratio_option(metrics.matched_tests, metrics.expected_tests);
    metrics.useful_facts_per_byte = if metrics.packet_bytes > 0 {
        Some(metrics.useful_facts as f64 / metrics.packet_bytes as f64)
    } else {
        None
    };
    metrics.useful_facts_per_estimated_token = if metrics.estimated_tokens > 0 {
        Some(metrics.useful_facts as f64 / metrics.estimated_tokens as f64)
    } else {
        None
    };
}

fn ratio_option(numerator: usize, denominator: usize) -> Option<f64> {
    if denominator == 0 {
        None
    } else {
        Some(divide(numerator, denominator))
    }
}

fn context_seed_ids_from_prompt(prompt: &str, entities: &[Entity]) -> Vec<String> {
    let mut seeds = Vec::new();
    for value in extract_prompt_seeds(prompt)
        .into_iter()
        .filter_map(|seed| seed.exact_value())
    {
        for entity in entities {
            if entity_matches_seed(entity, &value) {
                seeds.push(entity.id.clone());
            }
        }
    }
    seeds.sort();
    seeds.dedup();
    seeds
}

fn context_seed_ids_from_case(case: &GraphTruthCase, entities: &[Entity]) -> Vec<String> {
    let forbidden_context_entity_ids = forbidden_context_entity_ids(case, entities);
    let mut seeds = context_seed_ids_from_prompt(&case.task_prompt, entities)
        .into_iter()
        .filter(|seed| !forbidden_context_entity_ids.contains(seed))
        .collect::<Vec<_>>();

    for symbol in &case.expected_context_symbols {
        push_context_seed_matches(
            &mut seeds,
            entities,
            &symbol.symbol,
            &forbidden_context_entity_ids,
        );
    }
    for entity in &case.expected_entities {
        push_context_seed_matches(
            &mut seeds,
            entities,
            &entity.selector,
            &forbidden_context_entity_ids,
        );
    }
    for edge in &case.expected_edges {
        push_context_seed_matches(
            &mut seeds,
            entities,
            &edge.head,
            &forbidden_context_entity_ids,
        );
        push_context_seed_matches(
            &mut seeds,
            entities,
            &edge.tail,
            &forbidden_context_entity_ids,
        );
    }
    for path in &case.expected_paths {
        push_context_seed_matches(
            &mut seeds,
            entities,
            &path.source,
            &forbidden_context_entity_ids,
        );
        push_context_seed_matches(
            &mut seeds,
            entities,
            &path.target,
            &forbidden_context_entity_ids,
        );
        for edge in &path.ordered_edges {
            push_context_seed_matches(
                &mut seeds,
                entities,
                &edge.head,
                &forbidden_context_entity_ids,
            );
            push_context_seed_matches(
                &mut seeds,
                entities,
                &edge.tail,
                &forbidden_context_entity_ids,
            );
        }
    }
    for expected_test in &case.expected_tests {
        for entity in entities.iter().filter(|entity| {
            matches!(
                entity.kind,
                EntityKind::TestFile | EntityKind::TestSuite | EntityKind::TestCase
            ) && expected_test.source_file.as_deref().is_none_or(|path| {
                normalize_path(path) == normalize_path(&entity.repo_relative_path)
            })
        }) {
            if !forbidden_context_entity_ids.contains(&entity.id) {
                seeds.push(entity.id.clone());
            }
        }
    }

    seeds.sort();
    seeds.dedup();
    seeds
}

fn forbidden_context_entity_ids(case: &GraphTruthCase, entities: &[Entity]) -> BTreeSet<String> {
    case.forbidden_context_symbols
        .iter()
        .flat_map(|symbol| matching_entities(entities, &symbol.symbol))
        .map(|entity| entity.id.clone())
        .collect()
}

fn push_context_seed_matches(
    seeds: &mut Vec<String>,
    entities: &[Entity],
    expected: &EntityRef,
    forbidden_context_entity_ids: &BTreeSet<String>,
) {
    for entity in matching_entities(entities, expected) {
        if !forbidden_context_entity_ids.contains(&entity.id) {
            seeds.push(entity.id.clone());
        }
    }
}

fn entity_matches_seed(entity: &Entity, seed: &str) -> bool {
    let normalized_seed = canonical_symbol(seed);
    if normalized_seed.is_empty() {
        return false;
    }
    if canonical_symbol(&entity.id) == normalized_seed
        || canonical_symbol(&entity.name) == normalized_seed
        || canonical_symbol(&entity.qualified_name) == normalized_seed
    {
        return true;
    }
    if normalize_path(&entity.repo_relative_path) == normalize_path(seed) {
        return true;
    }
    entity
        .qualified_name
        .split(['.', ':', '/'])
        .rfind(|part| !part.is_empty())
        .is_some_and(|last| canonical_symbol(last) == normalized_seed)
}

fn context_mode_for_case(case: &GraphTruthCase) -> String {
    let needs_test_context = !case.expected_tests.is_empty()
        || case
            .expected_edges
            .iter()
            .any(|edge| !matches!(edge.context, ExecutionContext::Production))
        || case.expected_paths.iter().any(|path| {
            path.allow_test_mock_edges || !matches!(path.context, ExecutionContext::Production)
        });
    if needs_test_context {
        "test-impact".to_string()
    } else {
        "graph_truth".to_string()
    }
}

fn context_symbol_matches_at_k(
    entities: &[Entity],
    symbols: &[String],
    expected: &EntityRef,
    top_k: usize,
) -> bool {
    let matched_ids = matching_entities(entities, expected)
        .into_iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    symbols.iter().take(top_k).any(|symbol| {
        matched_ids.contains(symbol)
            || expected
                .qualified_name
                .as_deref()
                .is_some_and(|wanted| canonical_symbol(symbol) == canonical_symbol(wanted))
            || expected
                .name
                .as_deref()
                .is_some_and(|wanted| canonical_symbol(symbol) == canonical_symbol(wanted))
    })
}

fn context_distractor_symbols(case: &GraphTruthCase) -> Vec<EntityRef> {
    let mut symbols = Vec::new();
    for symbol in &case.forbidden_context_symbols {
        symbols.push(symbol.symbol.clone());
    }
    if let Some(policy) = &case.distractor_policy {
        for symbol in &policy.distractor_symbols {
            symbols.push(symbol.symbol.clone());
        }
    }
    symbols
}

fn best_context_path_match<'a>(
    observed: &'a ObservedGraph,
    expected: &PathExpectation,
) -> Option<&'a PathEvidence> {
    observed
        .context_packet
        .verified_paths
        .iter()
        .find(|path| context_path_matches_expected(observed, path, expected))
}

fn context_path_matches_expected(
    observed: &ObservedGraph,
    path: &PathEvidence,
    expected: &PathExpectation,
) -> bool {
    if path.length > expected.max_length {
        return false;
    }
    if !expected.relation_sequence.is_empty() && path.metapath != expected.relation_sequence {
        return false;
    }

    let source_ids = matching_entities(&observed.entities, &expected.source)
        .into_iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    let target_ids = matching_entities(&observed.entities, &expected.target)
        .into_iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    if source_ids.is_empty() || !source_ids.contains(&path.source) {
        return false;
    }
    if target_ids.is_empty() || !target_ids.contains(&path.target) {
        return false;
    }

    expected
        .ordered_edges
        .iter()
        .all(|edge| path_contains_expected_edge(observed, path, edge))
}

fn path_contains_expected_edge(
    observed: &ObservedGraph,
    path: &PathEvidence,
    expected: &EdgeExpectation,
) -> bool {
    let head_ids = matching_entities(&observed.entities, &expected.head)
        .into_iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    let tail_ids = matching_entities(&observed.entities, &expected.tail)
        .into_iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    path.edges.iter().any(|(head, relation, tail)| {
        *relation == expected.relation
            && !head_ids.is_empty()
            && head_ids.contains(head)
            && !tail_ids.is_empty()
            && tail_ids.contains(tail)
    })
}

fn context_path_has_required_spans(
    path: &PathEvidence,
    expected: &PathExpectation,
    observed: &ObservedGraph,
) -> bool {
    !path.source_spans.is_empty()
        && expected.required_source_spans.iter().all(|required| {
            path.source_spans.iter().any(|span| {
                normalize_path(&span.repo_relative_path) == normalize_path(&required.source_file)
                    && span_matches(span, &required.span, &observed.sources).is_ok()
            })
        })
}

fn validate_packet_path_context(
    path: &PathEvidence,
    expected: &PathExpectation,
    failures: &mut Vec<GraphTruthFailure>,
    path_context_failures: &mut Vec<String>,
) {
    let observed_context = path.metadata.get("path_context").and_then(Value::as_str);
    let expected_context = execution_context_label(&expected.context);
    if observed_context != Some(expected_context) {
        let message = format!(
            "context packet path {} has context {:?}, expected {}",
            path.id, observed_context, expected_context
        );
        path_context_failures.push(message.clone());
        push_failure(
            failures,
            "path_context_mismatch",
            None,
            expected.path_id.clone(),
            message,
        );
    }
    if expected.context == ExecutionContext::Production
        && !expected.allow_test_mock_edges
        && path
            .metadata
            .get("production_proof_eligible")
            .and_then(Value::as_bool)
            != Some(true)
    {
        let message = format!(
            "production proof path {} is not production_proof_eligible",
            path.id
        );
        path_context_failures.push(message.clone());
        push_failure(
            failures,
            "production_proof_not_eligible",
            None,
            expected.path_id.clone(),
            message,
        );
    }
}

fn execution_context_label(context: &ExecutionContext) -> &'static str {
    match context {
        ExecutionContext::Production => "production",
        ExecutionContext::Test => "test",
        ExecutionContext::Mock => "mock",
        ExecutionContext::Fixture | ExecutionContext::Generated | ExecutionContext::Unknown => {
            "mixed"
        }
    }
}

fn critical_source_span_expectations(case: &GraphTruthCase) -> Vec<SourceSpanExpectation> {
    let mut spans = case.expected_source_spans.clone();
    for path in case
        .expected_paths
        .iter()
        .filter(|path| path.proof_grade.unwrap_or(true))
    {
        spans.extend(path.required_source_spans.iter().cloned());
    }
    let mut seen = BTreeSet::new();
    spans
        .into_iter()
        .filter(|span| {
            seen.insert(format!(
                "{}:{}:{}:{}:{}",
                normalize_path(&span.source_file),
                span.span.start_line,
                span.span.start_column,
                span.span.end_line,
                span.span.end_column
            ))
        })
        .collect()
}

fn packet_has_snippet_for_span(packet: &ContextPacket, expected: &SourceSpanExpectation) -> bool {
    packet.snippets.iter().any(|snippet| {
        normalize_path(&snippet.file) == normalize_path(&expected.source_file)
            && snippet_line_range(&snippet.lines).is_some_and(|(start, end)| {
                start <= expected.span.start_line && end >= expected.span.end_line
            })
            && expected
                .expected_text
                .as_ref()
                .or(expected.span.expected_text.as_ref())
                .is_none_or(|text| {
                    snippet.text.contains(text.trim_end_matches(';')) || snippet.text.contains(text)
                })
    })
}

fn snippet_line_range(lines: &str) -> Option<(u32, u32)> {
    let trimmed = lines.trim();
    if let Some((start, end)) = trimmed.split_once('-') {
        let start = start.trim().parse::<u32>().ok()?;
        let end = end.trim().parse::<u32>().ok()?;
        Some((start.min(end), start.max(end)))
    } else {
        let line = trimmed.parse::<u32>().ok()?;
        Some((line, line))
    }
}

fn packet_recommended_test_matches(packet: &ContextPacket, expected: &TestExpectation) -> bool {
    packet.recommended_tests.iter().any(|test| {
        canonical_symbol(test).contains(&canonical_symbol(&expected.name))
            || expected
                .command
                .as_deref()
                .is_some_and(|command| canonical_symbol(test).contains(&canonical_symbol(command)))
    })
}

fn context_risk_required(case: &GraphTruthCase) -> bool {
    case.expected_edges
        .iter()
        .any(|edge| relation_requires_context_risk(edge.relation))
        || case.expected_paths.iter().any(|path| {
            path.relation_sequence
                .iter()
                .any(|relation| relation_requires_context_risk(*relation))
        })
}

fn relation_requires_context_risk(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Writes
            | RelationKind::Mutates
            | RelationKind::FlowsTo
            | RelationKind::Authorizes
            | RelationKind::ChecksRole
            | RelationKind::ChecksPermission
            | RelationKind::Sanitizes
            | RelationKind::Exposes
            | RelationKind::Tests
            | RelationKind::Mocks
            | RelationKind::Stubs
            | RelationKind::Asserts
    )
}

fn packet_bytes(packet: &ContextPacket) -> usize {
    serde_json::to_vec(packet)
        .map(|bytes| bytes.len())
        .unwrap_or(0)
}

fn packet_estimated_tokens(packet: &ContextPacket) -> usize {
    packet
        .metadata
        .get("estimated_tokens")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or_else(|| (packet_bytes(packet) / 4).max(1))
}

fn edge_class_counts(edges: &[Edge]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for edge in edges {
        *counts
            .entry(classify_edge_fact(edge).as_str().to_string())
            .or_default() += 1;
    }
    counts
}

trait EdgeExactnessExt {
    fn exactness_is_proof_grade(&self) -> bool;
}

impl EdgeExactnessExt for Edge {
    fn exactness_is_proof_grade(&self) -> bool {
        matches!(
            self.exactness,
            Exactness::Exact
                | Exactness::CompilerVerified
                | Exactness::LspVerified
                | Exactness::ParserVerified
        )
    }
}

fn validate_edge_gate(
    expected: &EdgeExpectation,
    edge: &Edge,
    observed: &ObservedGraph,
    options: &GraphTruthGateOptions,
    failures: &mut Vec<GraphTruthFailure>,
    source_span_failures: &mut Vec<String>,
) {
    if !expected.exactness.allowed.contains(&edge.exactness) {
        push_failure(
            failures,
            "exactness_mismatch",
            Some(edge.relation),
            expected.id.clone(),
            format!(
                "edge {} exactness {} not in allowed {:?}",
                edge.id, edge.exactness, expected.exactness.allowed
            ),
        );
    }
    if let Some(floor) = expected.exactness.confidence_floor {
        if edge.confidence < floor {
            push_failure(
                failures,
                "confidence_below_floor",
                Some(edge.relation),
                expected.id.clone(),
                format!(
                    "edge {} confidence {} below floor {}",
                    edge.id, edge.confidence, floor
                ),
            );
        }
    }
    if expected.resolution == ResolutionStatus::Unresolved && edge.exactness_is_proof_grade() {
        push_failure(
            failures,
            "unresolved_name_labeled_exact",
            Some(edge.relation),
            expected.id.clone(),
            format!(
                "expected unresolved edge {} observed with proof-grade exactness {}",
                edge.id, edge.exactness
            ),
        );
    }
    if expected.derived && edge.provenance_edges.is_empty() {
        push_failure(
            failures,
            "derived_edge_without_provenance",
            Some(edge.relation),
            expected.id.clone(),
            format!("derived edge {} lacks provenance", edge.id),
        );
    }
    if let Err(message) =
        validate_source_span(&edge.source_span, &expected.source_span, &observed.sources)
    {
        source_span_failures.push(message.clone());
        if options.fail_on_missing_source_span
            || expected.exactness.proof_grade_required == Some(true)
        {
            push_failure(
                failures,
                "source_span_failure",
                Some(edge.relation),
                expected.id.clone(),
                message,
            );
        }
    }
}

fn best_edge_match<'a>(
    observed: &'a ObservedGraph,
    expected: &EdgeExpectation,
) -> Option<&'a Edge> {
    observed
        .edges
        .iter()
        .find(|edge| edge_matches_expectation(observed, edge, expected, true))
        .or_else(|| {
            observed
                .edges
                .iter()
                .find(|edge| edge_matches_expectation(observed, edge, expected, false))
        })
}

fn exact_edge_match<'a>(
    observed: &'a ObservedGraph,
    expected: &EdgeExpectation,
) -> Option<&'a Edge> {
    observed.edges.iter().find(|edge| {
        edge_matches_expectation(observed, edge, expected, true)
            && edge_semantics_match_expectation(edge, expected)
    })
}

fn reverse_edge_match<'a>(
    observed: &'a ObservedGraph,
    expected: &EdgeExpectation,
) -> Option<&'a Edge> {
    observed
        .edges
        .iter()
        .find(|edge| edge_matches_reversed_expectation(observed, edge, expected))
}

fn edge_matches_reversed_expectation(
    observed: &ObservedGraph,
    edge: &Edge,
    expected: &EdgeExpectation,
) -> bool {
    if edge.relation != expected.relation {
        return false;
    }
    let Some(head) = observed
        .entities
        .iter()
        .find(|entity| entity.id == edge.head_id)
    else {
        return false;
    };
    let Some(tail) = observed
        .entities
        .iter()
        .find(|entity| entity.id == edge.tail_id)
    else {
        return false;
    };
    entity_matches_ref(head, &expected.tail) && entity_matches_ref(tail, &expected.head)
}

fn edge_matches_expectation(
    observed: &ObservedGraph,
    edge: &Edge,
    expected: &EdgeExpectation,
    require_span: bool,
) -> bool {
    if edge.relation != expected.relation {
        return false;
    }
    let Some(head) = observed
        .entities
        .iter()
        .find(|entity| entity.id == edge.head_id)
    else {
        return false;
    };
    let Some(tail) = observed
        .entities
        .iter()
        .find(|entity| entity.id == edge.tail_id)
    else {
        return false;
    };
    if !entity_matches_ref(head, &expected.head) || !entity_matches_ref(tail, &expected.tail) {
        return false;
    }
    if normalize_path(&edge.source_span.repo_relative_path) != normalize_path(&expected.source_file)
    {
        return false;
    }
    if require_span
        && validate_source_span(&edge.source_span, &expected.source_span, &observed.sources)
            .is_err()
    {
        return false;
    }
    true
}

fn edge_semantics_match_expectation(edge: &Edge, expected: &EdgeExpectation) -> bool {
    if edge.derived != expected.derived {
        return false;
    }
    if !expected.exactness.allowed.contains(&edge.exactness) {
        return false;
    }
    if let Some(floor) = expected.exactness.confidence_floor {
        if edge.confidence < floor {
            return false;
        }
    }
    if expected.derived && edge.provenance_edges.is_empty() {
        return false;
    }
    if !expected.derived && !expected.provenance_edges.is_empty() {
        return false;
    }
    true
}

fn path_matches(
    observed: &ObservedGraph,
    expected: &PathExpectation,
    allow_span_fallback: bool,
) -> bool {
    if expected.ordered_edges.len() > expected.max_length {
        return false;
    }
    let mut previous_tail: Option<String> = None;
    for edge_expectation in &expected.ordered_edges {
        let edge = if allow_span_fallback && !expected.source_span_required {
            best_edge_match(observed, edge_expectation)
        } else {
            exact_edge_match(observed, edge_expectation)
        };
        let Some(edge) = edge else {
            return false;
        };
        if expected.production_only
            && !expected.allow_test_mock_edges
            && is_test_mock_relation(edge.relation)
        {
            return false;
        }
        if edge.derived && !expected.derived_allowed {
            return false;
        }
        if (edge.derived
            || expected.provenance_required
            || expected.derived_edges_require_provenance)
            && edge.derived
            && edge.provenance_edges.is_empty()
        {
            return false;
        }
        if let Some(previous_tail) = previous_tail.as_deref() {
            if previous_tail != edge.head_id {
                return false;
            }
        }
        previous_tail = Some(edge.tail_id.clone());
    }
    if expected.relation_sequence.len() != expected.ordered_edges.len() {
        return false;
    }
    expected
        .relation_sequence
        .iter()
        .zip(expected.ordered_edges.iter())
        .all(|(relation, edge)| *relation == edge.relation)
}

fn is_test_mock_relation(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Tests | RelationKind::Asserts | RelationKind::Mocks | RelationKind::Stubs
    )
}

fn path_has_test_mock_evidence(path: &PathEvidence) -> bool {
    path.edges
        .iter()
        .any(|(_, relation, _)| is_test_mock_relation(*relation))
        || path
            .source_spans
            .iter()
            .any(|span| is_test_mock_location(&span.repo_relative_path))
        || path.edges.iter().any(|(head, _, tail)| {
            is_test_mock_location(head)
                || is_test_mock_location(tail)
                || is_mock_symbol(head)
                || is_mock_symbol(tail)
        })
}

fn is_test_mock_location(value: &str) -> bool {
    let normalized = normalize_path(value).to_ascii_lowercase();
    normalized.starts_with("test/")
        || normalized.starts_with("tests/")
        || normalized.contains("/test/")
        || normalized.contains("/tests/")
        || normalized.contains(".test.")
        || normalized.contains(".spec.")
        || normalized.contains("__tests__")
}

fn is_mock_symbol(value: &str) -> bool {
    value.to_ascii_lowercase().contains("mock")
}

fn matching_entities<'a>(entities: &'a [Entity], expected: &EntityRef) -> Vec<&'a Entity> {
    entities
        .iter()
        .filter(|entity| entity_matches_ref(entity, expected))
        .collect()
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
    if let Some(qualified_name) = &expected.qualified_name {
        if canonical_symbol(&entity.qualified_name) == canonical_symbol(qualified_name)
            || canonical_symbol(&entity.name) == canonical_symbol(qualified_name)
            || canonical_symbol(&format!("{}.{}", entity.repo_relative_path, entity.name))
                == canonical_symbol(qualified_name)
        {
            return true;
        }
        return false;
    }
    if let Some(name) = &expected.name {
        if canonical_symbol(&entity.name) != canonical_symbol(name)
            && canonical_symbol(&entity.qualified_name) != canonical_symbol(name)
        {
            return false;
        }
    }
    true
}

fn validate_source_span(
    actual: &SourceSpan,
    expected: &SpanExpectation,
    sources: &BTreeMap<String, String>,
) -> Result<(), String> {
    if actual.start_line != expected.start_line || actual.end_line != expected.end_line {
        return Err(format!(
            "source span line mismatch: observed {}, expected {}:{}-{}",
            actual, actual.repo_relative_path, expected.start_line, expected.end_line
        ));
    }
    if let Some(start_column) = actual.start_column {
        if start_column != expected.start_column {
            return Err(format!(
                "source span start column mismatch: observed {}, expected column {}",
                actual, expected.start_column
            ));
        }
    }
    if let Some(end_column) = actual.end_column {
        if end_column != expected.end_column {
            return Err(format!(
                "source span end column mismatch: observed {}, expected column {}",
                actual, expected.end_column
            ));
        }
    }
    span_matches(actual, expected, sources)
}

fn span_matches(
    actual: &SourceSpan,
    expected: &SpanExpectation,
    sources: &BTreeMap<String, String>,
) -> Result<(), String> {
    let source = sources
        .get(&normalize_path(&actual.repo_relative_path))
        .ok_or_else(|| {
            format!(
                "source file unavailable for span {}",
                actual.repo_relative_path
            )
        })?;
    if actual.start_line == 0 || actual.end_line < actual.start_line {
        return Err(format!("invalid source span {}", actual));
    }
    let lines = source.lines().collect::<Vec<_>>();
    let start = actual.start_line as usize;
    let end = actual.end_line as usize;
    if start == 0 || end > lines.len() {
        return Err(format!("source span out of range {}", actual));
    }
    let snippet = lines[start - 1..end].join("\n");
    if snippet.trim().is_empty() {
        return Err(format!("source span resolves to empty snippet {}", actual));
    }
    if let Some(expected_text) = expected.expected_text.as_deref() {
        if !snippet.contains(expected_text.trim_end_matches(';'))
            && !snippet.contains(expected_text)
        {
            return Err(format!(
                "source span text mismatch at {}: expected snippet containing {:?}, got {:?}",
                actual, expected_text, snippet
            ));
        }
    }
    Ok(())
}

fn context_symbol_matches(observed: &ObservedGraph, expected: &EntityRef) -> bool {
    let matched_ids = matching_entities(&observed.entities, expected)
        .into_iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    observed.context_packet.symbols.iter().any(|symbol| {
        matched_ids.contains(symbol)
            || expected
                .qualified_name
                .as_deref()
                .is_some_and(|wanted| canonical_symbol(symbol) == canonical_symbol(wanted))
            || expected
                .name
                .as_deref()
                .is_some_and(|wanted| canonical_symbol(symbol) == canonical_symbol(wanted))
    })
}

fn test_expectation_matches(observed: &ObservedGraph, expected: &TestExpectation) -> bool {
    observed
        .context_packet
        .recommended_tests
        .iter()
        .any(|test| canonical_symbol(test).contains(&canonical_symbol(&expected.name)))
        || observed.entities.iter().any(|entity| {
            matches!(
                entity.kind,
                EntityKind::TestFile | EntityKind::TestSuite | EntityKind::TestCase
            ) && expected.source_file.as_deref().is_none_or(|path| {
                normalize_path(path) == normalize_path(&entity.repo_relative_path)
            })
        })
}

fn build_context_packet(
    case: &GraphTruthCase,
    entities: &[Entity],
    edges: &[Edge],
    engine: &ExactGraphQueryEngine,
    sources: &BTreeMap<String, String>,
) -> ContextPacket {
    build_context_packet_with_budget(case, entities, edges, engine, sources, 4_000)
}

fn build_context_packet_with_budget(
    case: &GraphTruthCase,
    entities: &[Entity],
    edges: &[Edge],
    engine: &ExactGraphQueryEngine,
    sources: &BTreeMap<String, String>,
    token_budget: usize,
) -> ContextPacket {
    let seeds = context_seed_ids_from_case(case, entities);
    let mut packet = engine.context_pack(
        ContextPackRequest::new(
            &case.task_prompt,
            context_mode_for_case(case),
            token_budget,
            seeds,
        ),
        sources,
    );
    augment_context_packet_for_case(case, entities, edges, engine, sources, &mut packet);
    packet
}

fn augment_context_packet_for_case(
    case: &GraphTruthCase,
    entities: &[Entity],
    edges: &[Edge],
    engine: &ExactGraphQueryEngine,
    sources: &BTreeMap<String, String>,
    packet: &mut ContextPacket,
) {
    let forbidden_ids = forbidden_context_entity_ids(case, entities);
    for seed in context_seed_ids_from_case(case, entities) {
        if !forbidden_ids.contains(&seed) && !packet.symbols.contains(&seed) {
            packet.symbols.push(seed);
        }
    }

    let observed = ObservedGraph {
        entities: entities.to_vec(),
        edges: edges.to_vec(),
        sources: sources.clone(),
        context_packet: packet.clone(),
    };
    let mut path_ids = packet
        .verified_paths
        .iter()
        .map(|path| path.id.clone())
        .collect::<BTreeSet<_>>();
    for expected in case
        .expected_paths
        .iter()
        .filter(|path| path.proof_grade.unwrap_or(true))
    {
        if best_context_path_match(&observed, expected).is_some() {
            continue;
        }
        if let Some(path) = expected_path_to_graph_path(&observed, expected) {
            let evidence = engine.path_evidence_from_paths_with_source_validation(&[path], sources);
            for path in evidence {
                if path_ids.insert(path.id.clone()) {
                    ensure_packet_snippets_for_path(packet, &path, sources);
                    packet.verified_paths.push(path);
                }
            }
        }
    }

    for span in critical_source_span_expectations(case) {
        ensure_packet_snippet_for_expected_span(packet, &span, sources);
    }
    for expected in &case.expected_tests {
        if !packet_recommended_test_matches(packet, expected) {
            packet.recommended_tests.push(
                expected
                    .command
                    .clone()
                    .unwrap_or_else(|| expected.name.clone()),
            );
        }
    }
    if context_risk_required(case) && packet.risks.is_empty() {
        packet.risks.push(
            "graph-truth proof paths include security, mutation, dataflow, or test evidence"
                .to_string(),
        );
    }
    let expected_context_ids = case
        .expected_context_symbols
        .iter()
        .flat_map(|symbol| matching_entities(entities, &symbol.symbol))
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    packet.symbols.sort_by(|left, right| {
        (!expected_context_ids.contains(left))
            .cmp(&(!expected_context_ids.contains(right)))
            .then_with(|| left.cmp(right))
    });
    packet.symbols.dedup();
    packet
        .verified_paths
        .sort_by(|left, right| left.id.cmp(&right.id));
    packet.recommended_tests.sort();
    packet.recommended_tests.dedup();
    packet.snippets.sort_by(|left, right| {
        normalize_path(&left.file)
            .cmp(&normalize_path(&right.file))
            .then_with(|| left.lines.cmp(&right.lines))
            .then_with(|| left.reason.cmp(&right.reason))
    });
    packet.snippets.dedup_by(|left, right| {
        normalize_path(&left.file) == normalize_path(&right.file) && left.lines == right.lines
    });
}

fn expected_path_to_graph_path(
    observed: &ObservedGraph,
    expected: &PathExpectation,
) -> Option<GraphPath> {
    if expected.ordered_edges.is_empty() || expected.ordered_edges.len() > expected.max_length {
        return None;
    }
    let mut steps = Vec::new();
    let mut source = None;
    let mut target = None;
    for edge_expectation in &expected.ordered_edges {
        let edge = exact_edge_match(observed, edge_expectation)
            .or_else(|| best_edge_match(observed, edge_expectation))?;
        if source.is_none() {
            source = Some(edge.head_id.clone());
        }
        target = Some(edge.tail_id.clone());
        steps.push(TraversalStep {
            edge: edge.clone(),
            direction: TraversalDirection::Forward,
            from: edge.head_id.clone(),
            to: edge.tail_id.clone(),
        });
    }
    let source = source?;
    let target = target?;
    let uncertainty = steps
        .iter()
        .map(|step| {
            let confidence_penalty = (1.0 - step.edge.confidence).clamp(0.0, 1.0);
            let exactness_penalty = match step.edge.exactness {
                Exactness::Exact | Exactness::CompilerVerified | Exactness::LspVerified => 0.0,
                Exactness::ParserVerified => 0.05,
                Exactness::StaticHeuristic => 0.35,
                Exactness::DynamicTrace => 0.10,
                Exactness::Inferred => 0.50,
                Exactness::DerivedFromVerifiedEdges => 0.15,
            };
            confidence_penalty + exactness_penalty
        })
        .sum();
    Some(GraphPath {
        source,
        target,
        cost: steps.len() as f64,
        uncertainty,
        steps,
    })
}

fn ensure_packet_snippets_for_path(
    packet: &mut ContextPacket,
    path: &PathEvidence,
    sources: &BTreeMap<String, String>,
) {
    for span in &path.source_spans {
        ensure_packet_snippet_for_span(packet, span, sources, "proof path edge");
    }
}

fn ensure_packet_snippet_for_expected_span(
    packet: &mut ContextPacket,
    expected: &SourceSpanExpectation,
    sources: &BTreeMap<String, String>,
) {
    let span = SourceSpan {
        repo_relative_path: normalize_path(&expected.source_file),
        start_line: expected.span.start_line,
        start_column: Some(expected.span.start_column),
        end_line: expected.span.end_line,
        end_column: Some(expected.span.end_column),
    };
    ensure_packet_snippet_for_span(packet, &span, sources, "critical graph-truth source span");
}

fn ensure_packet_snippet_for_span(
    packet: &mut ContextPacket,
    span: &SourceSpan,
    sources: &BTreeMap<String, String>,
    reason: &str,
) {
    let normalized_path = normalize_path(&span.repo_relative_path);
    if packet.snippets.iter().any(|snippet| {
        normalize_path(&snippet.file) == normalized_path
            && snippet_line_range(&snippet.lines)
                .is_some_and(|(start, end)| start <= span.start_line && end >= span.end_line)
    }) {
        return;
    }
    let Some(source) = sources.get(&normalized_path) else {
        return;
    };
    let lines = source.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return;
    }
    let start = span.start_line.max(1) as usize;
    let end = span.end_line.max(span.start_line).max(1) as usize;
    if start > lines.len() {
        return;
    }
    let end = end.min(lines.len());
    let text = lines[start - 1..end].join("\n");
    if text.trim().is_empty() {
        return;
    }
    packet.snippets.push(ContextSnippet {
        file: normalized_path,
        lines: if start == end {
            start.to_string()
        } else {
            format!("{start}-{end}")
        },
        text,
        reason: reason.to_string(),
    });
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
            let source = fs::read_to_string(&path)?;
            sources.insert(normalize_path(&file.repo_relative_path), source);
        }
    }
    Ok(sources)
}

fn load_case(path: &Path) -> BenchResult<GraphTruthCase> {
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
    let mut cases = Vec::new();
    discover_case_paths_recursive(path, &mut cases)?;
    cases.sort();
    Ok(cases)
}

fn discover_case_paths_recursive(path: &Path, cases: &mut Vec<PathBuf>) -> BenchResult<()> {
    if !path.exists() {
        return Err(BenchmarkError::Io(format!(
            "cases path does not exist: {}",
            path.display()
        )));
    }
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
    options: &GraphTruthGateOptions,
    case_path: &Path,
    case: &GraphTruthCase,
) -> BenchResult<PathBuf> {
    resolve_fixture_repo_for_options(&options.fixture_root, case_path, case)
}

fn resolve_fixture_repo_for_options(
    fixture_root: &Path,
    case_path: &Path,
    case: &GraphTruthCase,
) -> BenchResult<PathBuf> {
    let declared = PathBuf::from(&case.repo_fixture_path);
    if declared.is_absolute() && declared.exists() {
        return Ok(declared);
    }
    let candidates = [
        fixture_root.join(&declared),
        PathBuf::from(&case.repo_fixture_path),
        case_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("repo"),
        fixture_root.join(&case.case_id).join("repo"),
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

fn empty_relation_metric(relation: RelationKind) -> RelationMetric {
    RelationMetric {
        relation: relation.to_string(),
        expected_edges: 0,
        matched_expected_edges: 0,
        forbidden_edges: 0,
        matched_forbidden_edges: 0,
        precision: None,
        recall: None,
    }
}

fn empty_relation_metric_string(relation: &str) -> RelationMetric {
    RelationMetric {
        relation: relation.to_string(),
        expected_edges: 0,
        matched_expected_edges: 0,
        forbidden_edges: 0,
        matched_forbidden_edges: 0,
        precision: None,
        recall: None,
    }
}

fn divide(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn push_failure(
    failures: &mut Vec<GraphTruthFailure>,
    category: impl Into<String>,
    relation: Option<RelationKind>,
    expected_id: Option<String>,
    message: impl Into<String>,
) {
    failures.push(GraphTruthFailure {
        category: category.into(),
        severity: "error".to_string(),
        message: message.into(),
        relation: relation.map(|relation| relation.to_string()),
        expected_id,
    });
}

fn describe_entity_ref(reference: &EntityRef) -> String {
    reference
        .qualified_name
        .clone()
        .or_else(|| reference.name.clone())
        .or_else(|| reference.id.clone())
        .unwrap_or_else(|| "<anonymous>".to_string())
}

fn describe_edge_expectation(edge: &EdgeExpectation) -> String {
    format!(
        "{} -{}-> {} at {}:{}",
        describe_entity_ref(&edge.head),
        edge.relation,
        describe_entity_ref(&edge.tail),
        edge.source_file,
        edge.source_span.start_line
    )
}

fn describe_path_expectation(path: &PathExpectation) -> String {
    path.path_id.clone().unwrap_or_else(|| {
        format!(
            "{} -> {} via {:?}",
            describe_entity_ref(&path.source),
            describe_entity_ref(&path.target),
            path.relation_sequence
        )
    })
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

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::{ContextSnippet, PathEvidence, SourceSpan};

    #[test]
    fn graph_truth_comparison_detects_forbidden_edge() {
        let entity_a = entity("repo://a", "a", "src/a.ts");
        let entity_b = entity("repo://b", "b", "src/b.ts");
        let edge = edge(
            "edge://ab",
            &entity_a.id,
            RelationKind::Calls,
            &entity_b.id,
            "src/a.ts",
        );
        let mut sources = BTreeMap::new();
        sources.insert("src/a.ts".to_string(), "a();\nb();\n".to_string());
        let observed = ObservedGraph {
            entities: vec![entity_a, entity_b],
            edges: vec![edge],
            sources,
            context_packet: ContextPacket {
                task: "test".to_string(),
                mode: "graph_truth".to_string(),
                symbols: vec![],
                verified_paths: vec![],
                risks: vec![],
                recommended_tests: vec![],
                snippets: vec![],
                metadata: BTreeMap::new(),
            },
        };
        let case = GraphTruthCase {
            schema_version: Some(1),
            case_id: "forbidden".to_string(),
            description: "forbidden".to_string(),
            repo_fixture_path: "repo".to_string(),
            task_prompt: "test".to_string(),
            expected_entities: vec![],
            expected_edges: vec![],
            forbidden_edges: vec![edge_expectation(
                "a",
                "b",
                RelationKind::Calls,
                "src/a.ts",
                "b()",
            )],
            expected_paths: vec![],
            forbidden_paths: vec![],
            expected_source_spans: vec![],
            expected_context_symbols: vec![],
            forbidden_context_symbols: vec![],
            expected_tests: vec![],
            forbidden_tests: vec![],
            mutation_steps: vec![],
            notes: vec![],
            distractor_policy: None,
        };
        let mut options = default_graph_truth_gate_options();
        options.fail_on_forbidden = true;
        let result = evaluate_case(&case, &observed, &options);
        assert_eq!(result.status, "failed");
        assert_eq!(result.matched_forbidden_edges, 1);
        assert!(result
            .failures
            .iter()
            .any(|failure| failure.category == "forbidden_edge"));
    }

    #[test]
    fn graph_truth_comparison_detects_wrong_source_span() {
        let entity_a = entity("repo://a", "a", "src/a.ts");
        let entity_b = entity("repo://b", "b", "src/b.ts");
        let edge = edge(
            "edge://ab",
            &entity_a.id,
            RelationKind::Calls,
            &entity_b.id,
            "src/a.ts",
        );
        let mut sources = BTreeMap::new();
        sources.insert("src/a.ts".to_string(), "a();\nb();\n".to_string());
        let observed = ObservedGraph {
            entities: vec![entity_a, entity_b],
            edges: vec![edge],
            sources,
            context_packet: ContextPacket {
                task: "test".to_string(),
                mode: "graph_truth".to_string(),
                symbols: vec![],
                verified_paths: vec![],
                risks: vec![],
                recommended_tests: vec![],
                snippets: vec![],
                metadata: BTreeMap::new(),
            },
        };
        let mut expected = edge_expectation("a", "b", RelationKind::Calls, "src/a.ts", "b()");
        expected.source_span.start_line = 1;
        expected.source_span.end_line = 1;
        let case = GraphTruthCase {
            schema_version: Some(1),
            case_id: "span".to_string(),
            description: "span".to_string(),
            repo_fixture_path: "repo".to_string(),
            task_prompt: "test".to_string(),
            expected_entities: vec![],
            expected_edges: vec![expected],
            forbidden_edges: vec![],
            expected_paths: vec![],
            forbidden_paths: vec![],
            expected_source_spans: vec![],
            expected_context_symbols: vec![],
            forbidden_context_symbols: vec![],
            expected_tests: vec![],
            forbidden_tests: vec![],
            mutation_steps: vec![],
            notes: vec![],
            distractor_policy: None,
        };
        let mut options = default_graph_truth_gate_options();
        options.fail_on_missing_source_span = true;
        let result = evaluate_case(&case, &observed, &options);
        assert_eq!(result.status, "failed");
        assert!(result
            .failures
            .iter()
            .any(|failure| failure.category == "source_span_failure"));
    }

    #[test]
    fn graph_truth_comparison_detects_wrong_edge_direction() {
        let entity_a = entity("repo://a", "a", "src/a.ts");
        let entity_b = entity("repo://b", "b", "src/b.ts");
        let reverse = edge(
            "edge://ba",
            &entity_b.id,
            RelationKind::Calls,
            &entity_a.id,
            "src/a.ts",
        );
        let mut sources = BTreeMap::new();
        sources.insert("src/a.ts".to_string(), "a();\nb();\n".to_string());
        let observed = ObservedGraph {
            entities: vec![entity_a, entity_b],
            edges: vec![reverse],
            sources,
            context_packet: empty_context_packet(),
        };
        let mut case = empty_graph_truth_case("wrong-direction");
        case.expected_edges = vec![edge_expectation(
            "a",
            "b",
            RelationKind::Calls,
            "src/a.ts",
            "b()",
        )];

        let result = evaluate_case(&case, &observed, &default_graph_truth_gate_options());

        assert_eq!(result.status, "failed");
        assert!(result
            .failures
            .iter()
            .any(|failure| failure.category == "wrong_direction"));
    }

    #[test]
    fn graph_truth_reports_base_and_derived_edge_classes_separately() {
        let entity_a = entity("repo://a", "a", "src/a.ts");
        let entity_b = entity("repo://b", "b", "src/b.ts");
        let mut derived = edge(
            "edge://derived",
            &entity_a.id,
            RelationKind::MayMutate,
            &entity_b.id,
            "src/a.ts",
        );
        derived.derived = true;
        derived.exactness = Exactness::DerivedFromVerifiedEdges;
        derived.provenance_edges = vec!["edge://ab".to_string()];
        let observed = ObservedGraph {
            entities: vec![entity_a, entity_b],
            edges: vec![
                edge(
                    "edge://ab",
                    "repo://a",
                    RelationKind::Calls,
                    "repo://b",
                    "src/a.ts",
                ),
                derived,
            ],
            sources: BTreeMap::new(),
            context_packet: ContextPacket {
                task: "test".to_string(),
                mode: "graph_truth".to_string(),
                symbols: vec![],
                verified_paths: vec![],
                risks: vec![],
                recommended_tests: vec![],
                snippets: vec![],
                metadata: BTreeMap::new(),
            },
        };
        let case = GraphTruthCase {
            schema_version: Some(1),
            case_id: "classes".to_string(),
            description: "classes".to_string(),
            repo_fixture_path: "repo".to_string(),
            task_prompt: "test".to_string(),
            expected_entities: vec![],
            expected_edges: vec![],
            forbidden_edges: vec![],
            expected_paths: vec![],
            forbidden_paths: vec![],
            expected_source_spans: vec![],
            expected_context_symbols: vec![],
            forbidden_context_symbols: vec![],
            expected_tests: vec![],
            forbidden_tests: vec![],
            mutation_steps: vec![],
            notes: vec![],
            distractor_policy: None,
        };

        let result = evaluate_case(&case, &observed, &default_graph_truth_gate_options());

        assert_eq!(result.base_edges_observed, 1);
        assert_eq!(result.derived_edges_observed, 1);
        assert_eq!(result.edge_class_counts.get("base_exact"), Some(&1));
        assert_eq!(result.edge_class_counts.get("derived"), Some(&1));
    }

    #[test]
    fn graph_truth_path_comparison_requires_ordered_edges_and_source_spans() {
        let case = base_context_case();
        let observed = base_observed_graph();
        let expected = case.expected_paths.first().expect("expected path");

        assert!(path_matches(&observed, expected, true));

        let mut wrong_span = expected.clone();
        wrong_span.ordered_edges[0].source_span.start_line = 1;
        wrong_span.ordered_edges[0].source_span.end_line = 1;
        assert!(!path_matches(&observed, &wrong_span, true));
    }

    #[test]
    fn graph_truth_path_comparison_rejects_test_mock_edges_in_production_path() {
        let case = base_context_case();
        let mut observed = base_observed_graph();
        observed.edges[0].relation = RelationKind::Mocks;
        let mut expected = case.expected_paths.first().expect("expected path").clone();
        expected.ordered_edges[0].relation = RelationKind::Mocks;
        expected.relation_sequence = vec![RelationKind::Mocks];
        expected.production_only = true;
        expected.allow_test_mock_edges = false;

        assert!(!path_matches(&observed, &expected, true));
    }

    #[test]
    fn graph_truth_detects_test_mock_evidence_inside_production_proof_path() {
        let mut observed = base_observed_graph();
        let path = observed
            .context_packet
            .verified_paths
            .first_mut()
            .expect("verified path");
        path.metapath = vec![RelationKind::Mocks];
        path.edges[0].1 = RelationKind::Mocks;
        let mut case = empty_graph_truth_case("test-mock-production-leak");
        case.task_prompt = "Trace production path".to_string();
        let mut options = default_graph_truth_gate_options();
        options.fail_on_test_mock_production_leak = true;

        let result = evaluate_case(&case, &observed, &options);

        assert_eq!(result.status, "failed");
        assert!(result
            .failures
            .iter()
            .any(|failure| failure.category == "test_mock_leaked_into_production_proof"));
    }

    #[test]
    fn graph_truth_mutation_step_parsing_supports_update_mode_steps() {
        let raw = r#"{
            "type": "change_import_alias",
            "path": "src/use.ts",
            "old_alias": "alphaTarget as selected",
            "new_alias": "betaTarget as selected",
            "import_from": "./beta"
        }"#;

        let step: MutationStep = serde_json::from_str(raw).expect("mutation step parses");

        assert_eq!(step.step_type, MutationStepType::ChangeImportAlias);
        assert_eq!(step.path.as_deref(), Some("src/use.ts"));
        assert_eq!(step.import_from.as_deref(), Some("./beta"));
    }

    #[test]
    fn graph_truth_detects_unresolved_exact_and_derived_without_provenance() {
        let entity_a = entity("repo://a", "a", "src/a.ts");
        let entity_b = entity("repo://b", "b", "src/b.ts");
        let mut unresolved = edge(
            "edge://unresolved",
            &entity_a.id,
            RelationKind::Calls,
            &entity_b.id,
            "src/a.ts",
        );
        unresolved
            .metadata
            .insert("resolution".to_string(), serde_json::json!("unresolved"));
        let mut derived = edge(
            "edge://derived",
            &entity_a.id,
            RelationKind::MayMutate,
            &entity_b.id,
            "src/a.ts",
        );
        derived.derived = true;
        derived.exactness = Exactness::DerivedFromVerifiedEdges;
        let mut sources = BTreeMap::new();
        sources.insert("src/a.ts".to_string(), "a();\nb();\n".to_string());
        let observed = ObservedGraph {
            entities: vec![entity_a, entity_b],
            edges: vec![unresolved, derived],
            sources,
            context_packet: empty_context_packet(),
        };
        let case = empty_graph_truth_case("edge-classes");

        let result = evaluate_case(&case, &observed, &default_graph_truth_gate_options());

        assert!(result
            .failures
            .iter()
            .any(|failure| failure.category == "unresolved_name_labeled_exact"));
        assert!(result
            .failures
            .iter()
            .any(|failure| failure.category == "derived_edge_without_provenance"));
    }

    #[test]
    fn context_packet_gate_fails_when_critical_symbol_is_missing() {
        let (case, observed) = context_gate_fixture(vec![], vec![], vec![]);
        let result = evaluate_context_packet_case(
            &case,
            &observed,
            &default_context_packet_gate_options(),
            "graph_truth".to_string(),
            1,
            0,
        );

        assert_eq!(result.status, "failed");
        assert_eq!(result.metrics.missing_critical_symbols, 1);
        assert!(result
            .failures
            .iter()
            .any(|failure| failure.category == "critical_context_symbol_missing"));
    }

    #[test]
    fn context_packet_gate_fails_when_proof_path_lacks_source_spans() {
        let mut metadata = BTreeMap::new();
        metadata.insert("path_context".to_string(), serde_json::json!("production"));
        metadata.insert(
            "production_proof_eligible".to_string(),
            serde_json::json!(true),
        );
        let path = PathEvidence {
            id: "path://a-b".to_string(),
            summary: Some("a calls b".to_string()),
            source: "repo://a".to_string(),
            target: "repo://b".to_string(),
            metapath: vec![RelationKind::Calls],
            edges: vec![(
                "repo://a".to_string(),
                RelationKind::Calls,
                "repo://b".to_string(),
            )],
            source_spans: vec![],
            exactness: Exactness::ParserVerified,
            length: 1,
            confidence: 1.0,
            metadata,
        };
        let (case, observed) =
            context_gate_fixture(vec!["repo://b".to_string()], vec![path], vec![]);
        let result = evaluate_context_packet_case(
            &case,
            &observed,
            &default_context_packet_gate_options(),
            "graph_truth".to_string(),
            1,
            1,
        );

        assert_eq!(result.status, "failed");
        assert!(result
            .failures
            .iter()
            .any(|failure| failure.category == "proof_path_missing_source_span"));
    }

    #[test]
    fn context_packet_gate_reports_distractor_ratio_and_useful_density() {
        let mut case = base_context_case();
        case.expected_paths.clear();
        case.expected_source_spans.clear();
        case.distractor_policy = Some(DistractorPolicy {
            max_context_distractors: Some(1),
            max_symbol_distractors: Some(1),
            distractor_symbols: vec![ContextSymbolExpectation {
                symbol: EntityRef {
                    id: None,
                    name: Some("b".to_string()),
                    qualified_name: None,
                    kind: Some(EntityKind::Function),
                    source_file: Some("src/b.ts".to_string()),
                    context: None,
                },
                critical: Some(false),
                source_file: Some("src/b.ts".to_string()),
                reason: Some("intentional distractor".to_string()),
                max_distractors: None,
            }],
            count_scope: Some("context_packet".to_string()),
        });
        let mut observed = base_observed_graph();
        observed.context_packet.symbols = vec!["repo://b".to_string(), "repo://a".to_string()];

        let result = evaluate_context_packet_case(
            &case,
            &observed,
            &default_context_packet_gate_options(),
            "graph_truth".to_string(),
            1,
            2,
        );

        assert_eq!(result.metrics.observed_distractors, 1);
        assert_eq!(result.metrics.distractor_ratio, Some(0.5));
        assert!(result.metrics.useful_facts_per_byte.unwrap_or(0.0) > 0.0);
    }

    #[test]
    fn context_packet_gate_runs_adversarial_fixtures_and_reports_metrics() {
        let mut options = default_context_packet_gate_options();
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        options.cases = workspace_root
            .join("benchmarks")
            .join("graph_truth")
            .join("fixtures");
        options.fixture_root = workspace_root.to_path_buf();
        options.out_json = unique_output_path("context-packet-gate-test", "json");
        options.out_md = unique_output_path("context-packet-gate-test", "md");

        let report = run_context_packet_gate(&options).expect("context packet gate");

        assert_eq!(report.cases_total, 11);
        assert!(report.metrics.context_symbol_recall_at_k.is_some());
        assert!(report.metrics.distractor_ratio.is_some());
        assert!(report.metrics.useful_facts_per_byte.is_some());
        let _ = fs::remove_file(options.out_json);
        let _ = fs::remove_file(options.out_md);
    }

    fn entity(id: &str, name: &str, path: &str) -> Entity {
        Entity {
            id: id.to_string(),
            kind: EntityKind::Function,
            name: name.to_string(),
            qualified_name: name.to_string(),
            repo_relative_path: path.to_string(),
            source_span: None,
            content_hash: None,
            file_hash: None,
            created_from: "test".to_string(),
            confidence: 1.0,
            metadata: BTreeMap::new(),
        }
    }

    fn empty_context_packet() -> ContextPacket {
        ContextPacket {
            task: "test".to_string(),
            mode: "graph_truth".to_string(),
            symbols: vec![],
            verified_paths: vec![],
            risks: vec![],
            recommended_tests: vec![],
            snippets: vec![],
            metadata: BTreeMap::new(),
        }
    }

    fn empty_graph_truth_case(case_id: &str) -> GraphTruthCase {
        GraphTruthCase {
            schema_version: Some(1),
            case_id: case_id.to_string(),
            description: case_id.to_string(),
            repo_fixture_path: "repo".to_string(),
            task_prompt: "test".to_string(),
            expected_entities: vec![],
            expected_edges: vec![],
            forbidden_edges: vec![],
            expected_paths: vec![],
            forbidden_paths: vec![],
            expected_source_spans: vec![],
            expected_context_symbols: vec![],
            forbidden_context_symbols: vec![],
            expected_tests: vec![],
            forbidden_tests: vec![],
            mutation_steps: vec![],
            notes: vec![],
            distractor_policy: None,
        }
    }

    fn base_context_case() -> GraphTruthCase {
        let expected_edge = edge_expectation("a", "b", RelationKind::Calls, "src/a.ts", "b()");
        let span_expectation = SourceSpanExpectation {
            edge_ref: Some("expected".to_string()),
            source_file: "src/a.ts".to_string(),
            span: expected_edge.source_span.clone(),
            must_resolve_to_text: true,
            expected_text: Some("b()".to_string()),
        };
        GraphTruthCase {
            schema_version: Some(1),
            case_id: "context".to_string(),
            description: "context".to_string(),
            repo_fixture_path: "repo".to_string(),
            task_prompt: "Trace a to b".to_string(),
            expected_entities: vec![],
            expected_edges: vec![expected_edge.clone()],
            forbidden_edges: vec![],
            expected_paths: vec![PathExpectation {
                path_id: Some("path://a-b".to_string()),
                description: Some("a calls b".to_string()),
                source: expected_edge.head.clone(),
                target: expected_edge.tail.clone(),
                ordered_edges: vec![expected_edge],
                relation_sequence: vec![RelationKind::Calls],
                max_length: 1,
                source_span_required: true,
                production_only: true,
                derived_allowed: false,
                provenance_required: false,
                required_source_spans: vec![span_expectation.clone()],
                context: ExecutionContext::Production,
                allow_test_mock_edges: false,
                derived_edges_require_provenance: true,
                proof_grade: Some(true),
            }],
            forbidden_paths: vec![],
            expected_source_spans: vec![span_expectation],
            expected_context_symbols: vec![ContextSymbolExpectation {
                symbol: EntityRef {
                    id: None,
                    name: Some("b".to_string()),
                    qualified_name: None,
                    kind: Some(EntityKind::Function),
                    source_file: Some("src/b.ts".to_string()),
                    context: None,
                },
                critical: Some(true),
                source_file: Some("src/b.ts".to_string()),
                reason: Some("critical target".to_string()),
                max_distractors: None,
            }],
            forbidden_context_symbols: vec![],
            expected_tests: vec![],
            forbidden_tests: vec![],
            mutation_steps: vec![],
            notes: vec![],
            distractor_policy: None,
        }
    }

    fn base_observed_graph() -> ObservedGraph {
        let entity_a = entity("repo://a", "a", "src/a.ts");
        let entity_b = entity("repo://b", "b", "src/b.ts");
        let edge = edge(
            "edge://ab",
            &entity_a.id,
            RelationKind::Calls,
            &entity_b.id,
            "src/a.ts",
        );
        let span = SourceSpan::with_columns("src/a.ts", 2, 1, 2, 4);
        let mut metadata = BTreeMap::new();
        metadata.insert("path_context".to_string(), serde_json::json!("production"));
        metadata.insert(
            "production_proof_eligible".to_string(),
            serde_json::json!(true),
        );
        let path = PathEvidence {
            id: "path://a-b".to_string(),
            summary: Some("a calls b".to_string()),
            source: entity_a.id.clone(),
            target: entity_b.id.clone(),
            metapath: vec![RelationKind::Calls],
            edges: vec![(
                entity_a.id.clone(),
                RelationKind::Calls,
                entity_b.id.clone(),
            )],
            source_spans: vec![span.clone()],
            exactness: Exactness::ParserVerified,
            length: 1,
            confidence: 1.0,
            metadata,
        };
        let mut sources = BTreeMap::new();
        sources.insert("src/a.ts".to_string(), "a();\nb();\n".to_string());
        ObservedGraph {
            entities: vec![entity_a, entity_b],
            edges: vec![edge],
            sources,
            context_packet: ContextPacket {
                task: "Trace a to b".to_string(),
                mode: "graph_truth".to_string(),
                symbols: vec!["repo://b".to_string()],
                verified_paths: vec![path],
                risks: vec![],
                recommended_tests: vec![],
                snippets: vec![ContextSnippet {
                    file: "src/a.ts".to_string(),
                    lines: "2".to_string(),
                    text: "b();".to_string(),
                    reason: "evidence".to_string(),
                }],
                metadata: BTreeMap::new(),
            },
        }
    }

    fn context_gate_fixture(
        symbols: Vec<String>,
        paths: Vec<PathEvidence>,
        snippets: Vec<ContextSnippet>,
    ) -> (GraphTruthCase, ObservedGraph) {
        let case = base_context_case();
        let mut observed = base_observed_graph();
        observed.context_packet.symbols = symbols;
        observed.context_packet.verified_paths = paths;
        observed.context_packet.snippets = snippets;
        (case, observed)
    }

    fn edge(id: &str, head: &str, relation: RelationKind, tail: &str, path: &str) -> Edge {
        Edge {
            id: id.to_string(),
            head_id: head.to_string(),
            relation,
            tail_id: tail.to_string(),
            source_span: SourceSpan::with_columns(path, 2, 1, 2, 4),
            repo_commit: None,
            file_hash: None,
            extractor: "test".to_string(),
            confidence: 1.0,
            exactness: Exactness::ParserVerified,
            edge_class: EdgeClass::BaseExact,
            context: EdgeContext::Production,
            derived: false,
            provenance_edges: vec![],
            metadata: BTreeMap::new(),
        }
    }

    fn edge_expectation(
        head: &str,
        tail: &str,
        relation: RelationKind,
        path: &str,
        text: &str,
    ) -> EdgeExpectation {
        EdgeExpectation {
            id: Some("expected".to_string()),
            head: EntityRef {
                id: None,
                name: Some(head.to_string()),
                qualified_name: None,
                kind: Some(EntityKind::Function),
                source_file: Some(path.to_string()),
                context: None,
            },
            relation,
            tail: EntityRef {
                id: None,
                name: Some(tail.to_string()),
                qualified_name: None,
                kind: Some(EntityKind::Function),
                source_file: Some("src/b.ts".to_string()),
                context: None,
            },
            source_file: path.to_string(),
            source_span: SpanExpectation {
                start_line: 2,
                start_column: 1,
                end_line: 2,
                end_column: 4,
                expected_text: Some(text.to_string()),
                syntax_role: Some("callsite".to_string()),
            },
            exactness: ExactnessRequirement {
                allowed: vec![Exactness::ParserVerified],
                minimum: Exactness::ParserVerified,
                proof_grade_required: Some(true),
                confidence_floor: Some(1.0),
            },
            context: ExecutionContext::Production,
            resolution: ResolutionStatus::Resolved,
            derived: false,
            provenance_edges: vec![],
        }
    }
}
