//! Exact graph query engine and graph-only context packet builder.
//!
//! Phase 08 adds graph-only traversal over extracted `Edge` facts. The query
//! engine intentionally contains no vector retrieval, MCP, or UI implementation.
//! Phase 09 turns exact paths into PathEvidence, derived closure edges, and
//! compact graph-only context packets.
//! Phase 10 adds exact prompt seed extraction and preserves those seeds before
//! any later vector filtering stage.
//! Phase 13 composes Stage 0, Stage 1, Stage 2, Stage 3, and Stage 4 into the
//! corrected runtime funnel. Phase 14 adds deterministic Bayesian-style ranking
//! and uncertainty calibration metadata without replacing exact verification.

#![forbid(unsafe_code)]

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
    str::FromStr,
};

use codegraph_core::{
    infer_edge_class, infer_edge_context, ContextPacket, ContextSnippet, DerivedClosureEdge, Edge,
    EdgeClass, EdgeContext, Entity, EntityKind, Exactness, FileRecord, Metadata, PathEvidence,
    RelationKind, SourceSpan,
};
use codegraph_vector::{
    BinarySignature, BinaryVectorError, BinaryVectorIndex, CompressedVectorReranker,
    DeterministicCompressedReranker, InMemoryBinaryVectorIndex, RerankCandidate, RerankConfig,
    RerankQuery, RerankScore,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalDirection {
    Forward,
    Reverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Traversal {
    pub relation: RelationKind,
    pub direction: TraversalDirection,
}

impl Traversal {
    pub const fn forward(relation: RelationKind) -> Self {
        Self {
            relation,
            direction: TraversalDirection::Forward,
        }
    }

    pub const fn reverse(relation: RelationKind) -> Self {
        Self {
            relation,
            direction: TraversalDirection::Reverse,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraversalStep {
    pub edge: Edge,
    pub direction: TraversalDirection,
    pub from: String,
    pub to: String,
}

impl TraversalStep {
    pub fn relation(&self) -> RelationKind {
        self.edge.relation
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphPath {
    pub source: String,
    pub target: String,
    pub steps: Vec<TraversalStep>,
    pub cost: f64,
    pub uncertainty: f64,
}

impl GraphPath {
    pub fn empty(source: impl Into<String>) -> Self {
        let source = source.into();
        Self {
            source: source.clone(),
            target: source,
            steps: Vec::new(),
            cost: 0.0,
            uncertainty: 0.0,
        }
    }

    pub fn edge_ids(&self) -> Vec<String> {
        self.steps.iter().map(|step| step.edge.id.clone()).collect()
    }

    pub fn relations(&self) -> Vec<RelationKind> {
        self.steps.iter().map(TraversalStep::relation).collect()
    }

    pub fn source_spans(&self) -> Vec<SourceSpan> {
        self.steps
            .iter()
            .map(|step| step.edge.source_span.clone())
            .collect()
    }

    pub fn contains_relation(&self, relation: RelationKind) -> bool {
        self.steps.iter().any(|step| step.edge.relation == relation)
    }

    pub fn last_relation(&self) -> Option<RelationKind> {
        self.steps.last().map(TraversalStep::relation)
    }

    pub fn path_context(&self) -> PathContext {
        classify_path_context(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PathContext {
    Production,
    Test,
    Mock,
    Mixed,
}

impl PathContext {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Test => "test",
            Self::Mock => "mock",
            Self::Mixed => "mixed",
        }
    }

    pub const fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

pub type EdgeFactClass = EdgeClass;

pub fn classify_edge_fact(edge: &Edge) -> EdgeFactClass {
    infer_edge_class(edge)
}

pub fn classify_path_context(path: &GraphPath) -> PathContext {
    let mut saw_production = false;
    let mut saw_test = false;
    let mut saw_mock = false;

    for step in &path.steps {
        match classify_edge_context(&step.edge) {
            PathContext::Production => saw_production = true,
            PathContext::Test => saw_test = true,
            PathContext::Mock => saw_mock = true,
            PathContext::Mixed => {
                saw_production = true;
                saw_test = true;
            }
        }
    }

    match (saw_production, saw_test, saw_mock) {
        (true, true, _) | (true, _, true) | (_, true, true) => PathContext::Mixed,
        (_, _, true) => PathContext::Mock,
        (_, true, _) => PathContext::Test,
        _ => PathContext::Production,
    }
}

fn classify_edge_context(edge: &Edge) -> PathContext {
    match infer_edge_context(edge) {
        EdgeContext::Production => PathContext::Production,
        EdgeContext::Test => PathContext::Test,
        EdgeContext::Mock => PathContext::Mock,
        EdgeContext::Mixed | EdgeContext::Unknown => PathContext::Mixed,
    }
}

pub fn is_proof_path_relation(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Calls
            | RelationKind::Reads
            | RelationKind::Writes
            | RelationKind::FlowsTo
            | RelationKind::Mutates
            | RelationKind::Authorizes
            | RelationKind::ChecksRole
            | RelationKind::Sanitizes
            | RelationKind::Exposes
            | RelationKind::Injects
            | RelationKind::Instantiates
            | RelationKind::Publishes
            | RelationKind::Emits
            | RelationKind::Consumes
            | RelationKind::ListensTo
            | RelationKind::Tests
            | RelationKind::Mocks
            | RelationKind::Stubs
            | RelationKind::Asserts
    )
}

fn requires_exact_source_span(relation: RelationKind) -> bool {
    is_proof_path_relation(relation)
        || matches!(
            relation,
            RelationKind::Imports
                | RelationKind::Exports
                | RelationKind::Reexports
                | RelationKind::AliasOf
                | RelationKind::AliasedBy
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceSpanProofIssueKind {
    MissingSpan,
    SourceUnavailable,
    EmptySnippet,
    OutOfRange,
    WrongSyntaxLocation,
}

impl SourceSpanProofIssueKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingSpan => "missing_span",
            Self::SourceUnavailable => "source_unavailable",
            Self::EmptySnippet => "empty_snippet",
            Self::OutOfRange => "out_of_range",
            Self::WrongSyntaxLocation => "wrong_syntax_location",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpanProofIssue {
    pub edge_id: String,
    pub relation: RelationKind,
    pub span: SourceSpan,
    pub kind: SourceSpanProofIssueKind,
    pub message: String,
}

impl SourceSpanProofIssue {
    fn new(edge: &Edge, kind: SourceSpanProofIssueKind, message: impl Into<String>) -> Self {
        Self {
            edge_id: edge.id.clone(),
            relation: edge.relation,
            span: edge.source_span.clone(),
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EdgeClassProofIssueKind {
    DerivedWithoutProvenance,
    DerivedRelationNotFlagged,
    HeuristicEdge,
    UnresolvedExact,
    InverseEdge,
    TestMockEdge,
    UnknownEdgeClass,
}

impl EdgeClassProofIssueKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DerivedWithoutProvenance => "derived_without_provenance",
            Self::DerivedRelationNotFlagged => "derived_relation_not_flagged",
            Self::HeuristicEdge => "heuristic_edge",
            Self::UnresolvedExact => "unresolved_exact",
            Self::InverseEdge => "inverse_edge",
            Self::TestMockEdge => "test_mock_edge",
            Self::UnknownEdgeClass => "unknown_edge_class",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeClassProofIssue {
    pub edge_id: String,
    pub relation: RelationKind,
    pub fact_class: EdgeFactClass,
    pub kind: EdgeClassProofIssueKind,
    pub message: String,
}

impl EdgeClassProofIssue {
    fn new(
        edge: &Edge,
        fact_class: EdgeFactClass,
        kind: EdgeClassProofIssueKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            edge_id: edge.id.clone(),
            relation: edge.relation,
            fact_class,
            kind,
            message: message.into(),
        }
    }
}

pub fn validate_proof_path_edge_classes(path: &GraphPath) -> Result<(), Vec<EdgeClassProofIssue>> {
    let mut issues = Vec::new();

    for step in &path.steps {
        let edge = &step.edge;
        let fact_class = classify_edge_fact(edge);
        if is_derived_cache_relation(edge.relation) && !edge.derived {
            issues.push(EdgeClassProofIssue::new(
                edge,
                fact_class,
                EdgeClassProofIssueKind::DerivedRelationNotFlagged,
                "derived/cache relation is not marked derived",
            ));
        }
        if (edge.derived || is_derived_cache_relation(edge.relation))
            && edge.provenance_edges.is_empty()
        {
            issues.push(EdgeClassProofIssue::new(
                edge,
                fact_class,
                EdgeClassProofIssueKind::DerivedWithoutProvenance,
                "derived/cache edge lacks provenance edge ids",
            ));
        }
        match fact_class {
            EdgeFactClass::Derived => {
                if !edge.derived && !is_derived_cache_relation(edge.relation) {
                    issues.push(EdgeClassProofIssue::new(
                        edge,
                        fact_class,
                        EdgeClassProofIssueKind::DerivedRelationNotFlagged,
                        "derived edge class is not backed by a derived flag or derived relation",
                    ));
                }
            }
            EdgeFactClass::BaseHeuristic => {
                issues.push(EdgeClassProofIssue::new(
                    edge,
                    fact_class,
                    EdgeClassProofIssueKind::HeuristicEdge,
                    "heuristic/unresolved edge is not proof-grade by default",
                ));
            }
            EdgeFactClass::Test | EdgeFactClass::Mock | EdgeFactClass::Mixed => {
                issues.push(EdgeClassProofIssue::new(
                    edge,
                    fact_class,
                    EdgeClassProofIssueKind::TestMockEdge,
                    "test/mock/mixed-context edge is not a production proof edge by default",
                ));
            }
            EdgeFactClass::Unknown => {
                if is_inverse_relation(edge.relation) {
                    issues.push(EdgeClassProofIssue::new(
                        edge,
                        fact_class,
                        EdgeClassProofIssueKind::InverseEdge,
                        "inverse edge is not counted as a raw base fact",
                    ));
                } else {
                    issues.push(EdgeClassProofIssue::new(
                        edge,
                        fact_class,
                        EdgeClassProofIssueKind::UnknownEdgeClass,
                        "unknown edge class is not proof-grade by default",
                    ));
                }
            }
            EdgeFactClass::BaseExact | EdgeFactClass::ReifiedCallsite => {}
        }

        if edge_exactness_is_proof_grade(edge.exactness) && edge_has_unresolved_resolution(edge) {
            issues.push(EdgeClassProofIssue::new(
                edge,
                fact_class,
                EdgeClassProofIssueKind::UnresolvedExact,
                "unresolved textual match is labeled proof-grade exact",
            ));
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

pub fn validate_proof_path_source_spans(
    path: &GraphPath,
    sources: &BTreeMap<String, String>,
) -> Result<(), Vec<SourceSpanProofIssue>> {
    let mut issues = Vec::new();

    for step in &path.steps {
        if !requires_exact_source_span(step.edge.relation) {
            continue;
        }
        if let Err(issue) = validate_edge_source_span(&step.edge, sources) {
            issues.push(issue);
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueryLimits {
    pub max_depth: usize,
    pub max_paths: usize,
    pub max_edges_visited: usize,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            max_depth: 6,
            max_paths: 32,
            max_edges_visited: 10_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImpactAnalysis {
    pub source: String,
    pub callers: Vec<GraphPath>,
    pub callees: Vec<GraphPath>,
    pub reads: Vec<GraphPath>,
    pub writes: Vec<GraphPath>,
    pub mutations: Vec<GraphPath>,
    pub dataflow: Vec<GraphPath>,
    pub auth_paths: Vec<GraphPath>,
    pub event_flow: Vec<GraphPath>,
    pub tests: Vec<GraphPath>,
    pub migrations: Vec<GraphPath>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolSearchHit {
    pub entity: Entity,
    pub score: f64,
    pub features: BTreeMap<String, f64>,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolSearchIndex {
    entities: Vec<Entity>,
    files_by_path: BTreeMap<String, FileRecord>,
    neighbor_text_by_entity: BTreeMap<String, String>,
    degree_by_entity: BTreeMap<String, usize>,
}

impl SymbolSearchIndex {
    pub fn new(entities: Vec<Entity>, edges: Vec<Edge>, files: Vec<FileRecord>) -> Self {
        let entity_text = entities
            .iter()
            .map(|entity| {
                (
                    entity.id.clone(),
                    format!(
                        "{} {} {} {}",
                        entity.name,
                        entity.qualified_name,
                        entity.repo_relative_path,
                        searchable_metadata_text(&entity.metadata)
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let files_by_path = files
            .into_iter()
            .map(|file| (file.repo_relative_path.clone(), file))
            .collect::<BTreeMap<_, _>>();
        let mut neighbor_text_by_entity = BTreeMap::<String, String>::new();
        let mut degree_by_entity = BTreeMap::<String, usize>::new();

        for edge in &edges {
            *degree_by_entity.entry(edge.head_id.clone()).or_default() += 1;
            *degree_by_entity.entry(edge.tail_id.clone()).or_default() += 1;
            let relation = edge.relation.to_string();
            if let Some(text) = entity_text.get(&edge.tail_id) {
                neighbor_text_by_entity
                    .entry(edge.head_id.clone())
                    .or_default()
                    .push_str(&format!(" {relation} {text}"));
            }
            if let Some(text) = entity_text.get(&edge.head_id) {
                neighbor_text_by_entity
                    .entry(edge.tail_id.clone())
                    .or_default()
                    .push_str(&format!(" {relation} {text}"));
            }
        }

        Self {
            entities,
            files_by_path,
            neighbor_text_by_entity,
            degree_by_entity,
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<SymbolSearchHit> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }
        let query_lc = query.to_ascii_lowercase();
        let query_tokens = symbol_search_tokens(query);
        let mut hits = Vec::new();

        for entity in &self.entities {
            let mut score = 0.0;
            let mut features = BTreeMap::<String, f64>::new();
            let mut matched_terms = BTreeSet::<String>::new();
            let mut textual_match = false;
            let name_lc = entity.name.to_ascii_lowercase();
            let qualified_lc = entity.qualified_name.to_ascii_lowercase();
            let path_lc = entity.repo_relative_path.to_ascii_lowercase();
            let namespace_lc = entity_namespace(entity)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let metadata_text = searchable_metadata_text(&entity.metadata);
            let metadata_lc = metadata_text.to_ascii_lowercase();
            let neighbor_lc = self
                .neighbor_text_by_entity
                .get(&entity.id)
                .map(|text| text.to_ascii_lowercase())
                .unwrap_or_default();
            let field_text = format!(
                "{} {} {} {} {} {}",
                name_lc, qualified_lc, path_lc, namespace_lc, metadata_lc, neighbor_lc
            );
            let field_tokens = symbol_search_tokens(&field_text);

            if name_lc == query_lc {
                bump_feature(&mut features, "exact_symbol_match", 1.0);
                matched_terms.insert(entity.name.clone());
                score += 140.0;
                textual_match = true;
            }
            if qualified_lc == query_lc {
                bump_feature(&mut features, "qualified_name_match", 1.0);
                matched_terms.insert(entity.qualified_name.clone());
                score += 130.0;
                textual_match = true;
            } else if qualified_lc.ends_with(&format!(".{query_lc}"))
                || qualified_lc.ends_with(&format!("::{query_lc}"))
            {
                bump_feature(&mut features, "qualified_name_match", 0.75);
                matched_terms.insert(entity.qualified_name.clone());
                score += 82.0;
                textual_match = true;
            }
            if name_lc.starts_with(&query_lc) && name_lc != query_lc {
                bump_feature(&mut features, "prefix_match", 0.8);
                matched_terms.insert(entity.name.clone());
                score += 52.0;
                textual_match = true;
            }
            if qualified_lc.starts_with(&query_lc) && qualified_lc != query_lc {
                bump_feature(&mut features, "prefix_match", 0.55);
                matched_terms.insert(entity.qualified_name.clone());
                score += 38.0;
                textual_match = true;
            }
            if path_lc.contains(&query_lc) {
                bump_feature(&mut features, "file_path_proximity", 1.0);
                matched_terms.insert(entity.repo_relative_path.clone());
                score += 36.0;
                textual_match = true;
            }
            if !namespace_lc.is_empty() && namespace_lc.contains(&query_lc) {
                bump_feature(&mut features, "same_package_or_module", 1.0);
                score += 22.0;
                textual_match = true;
            }
            if metadata_lc.contains(&query_lc) {
                bump_feature(&mut features, "metadata_match", 1.0);
                score += 28.0;
                textual_match = true;
            }
            if neighbor_lc.contains(&query_lc) {
                bump_feature(&mut features, "relation_neighbor_text", 1.0);
                score += 18.0;
                textual_match = true;
            }

            let token_overlap = query_tokens
                .iter()
                .filter(|token| field_tokens.contains(*token))
                .cloned()
                .collect::<BTreeSet<_>>();
            if !token_overlap.is_empty() {
                let ratio = token_overlap.len() as f64 / query_tokens.len().max(1) as f64;
                bump_feature(&mut features, "token_match", ratio);
                score += 42.0 * ratio;
                matched_terms.extend(token_overlap);
                textual_match = true;
            }

            if query_lc.len() >= 3 && is_subsequence(&query_lc, &name_lc) && name_lc != query_lc {
                bump_feature(&mut features, "fuzzy_match", 0.65);
                score += 14.0;
                textual_match = true;
            }

            if !textual_match {
                continue;
            }

            let degree = self.degree_by_entity.get(&entity.id).copied().unwrap_or(0);
            if degree > 0 {
                let centrality = (degree as f64 / 12.0).min(1.0);
                bump_feature(&mut features, "graph_centrality", centrality);
                score += centrality * 8.0;
            }
            if self
                .files_by_path
                .get(&entity.repo_relative_path)
                .and_then(|file| file.indexed_at_unix_ms)
                .is_some()
            {
                bump_feature(&mut features, "recent_edit_signal", 0.2);
                score += 1.0;
            }
            if is_test_entity_kind(entity.kind) || entity.repo_relative_path.contains("test") {
                bump_feature(&mut features, "source_test_role", 1.0);
                if query_lc.contains("test") || query_lc.contains("spec") {
                    score += 12.0;
                }
            }

            hits.push(SymbolSearchHit {
                entity: entity.clone(),
                score,
                features,
                matched_terms: matched_terms.into_iter().collect(),
            });
        }

        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.entity.qualified_name.cmp(&right.entity.qualified_name))
                .then_with(|| left.entity.id.cmp(&right.entity.id))
        });
        hits.truncate(limit);
        hits
    }
}

fn searchable_metadata_text(metadata: &Metadata) -> String {
    metadata
        .iter()
        .filter_map(|(key, value)| {
            value
                .as_str()
                .map(|text| format!("{key} {text}"))
                .or_else(|| value.as_bool().map(|flag| format!("{key} {flag}")))
                .or_else(|| value.as_f64().map(|number| format!("{key} {number}")))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn entity_namespace(entity: &Entity) -> Option<String> {
    entity
        .qualified_name
        .rsplit_once("::")
        .or_else(|| entity.qualified_name.rsplit_once('.'))
        .map(|(namespace, _)| namespace.to_string())
}

fn bump_feature(features: &mut BTreeMap<String, f64>, name: &str, value: f64) {
    let entry = features.entry(name.to_string()).or_default();
    *entry = (*entry).max(value);
}

fn symbol_search_tokens(input: &str) -> BTreeSet<String> {
    let mut normalized = String::with_capacity(input.len() + 8);
    let mut previous: Option<char> = None;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if let Some(prev) = previous {
                if prev.is_ascii_lowercase() && ch.is_ascii_uppercase() {
                    normalized.push(' ');
                }
            }
            normalized.push(ch.to_ascii_lowercase());
            previous = Some(ch);
        } else {
            normalized.push(' ');
            previous = None;
        }
    }
    normalized
        .split_whitespace()
        .filter(|token| token.len() >= 2)
        .map(str::to_string)
        .collect()
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = needle.chars();
    let mut wanted = chars.next();
    if wanted.is_none() {
        return true;
    }
    for ch in haystack.chars() {
        if Some(ch) == wanted {
            wanted = chars.next();
            if wanted.is_none() {
                return true;
            }
        }
    }
    false
}

fn is_test_entity_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::TestFile
            | EntityKind::TestSuite
            | EntityKind::TestCase
            | EntityKind::Fixture
            | EntityKind::Mock
            | EntityKind::Stub
            | EntityKind::Assertion
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalDocument {
    pub id: String,
    pub text: String,
    pub stage0_score: f64,
    pub metadata: BTreeMap<String, String>,
}

impl RetrievalDocument {
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            stage0_score: 0.0,
            metadata: BTreeMap::new(),
        }
    }

    pub fn stage0_score(mut self, score: f64) -> Self {
        self.stage0_score = score;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalFunnelConfig {
    pub binary_dimensions: usize,
    pub stage1_top_k: usize,
    pub stage2_top_n: usize,
    pub query_limits: QueryLimits,
    pub rerank_config: RerankConfig,
    pub bayesian_config: BayesianRankerConfig,
}

impl Default for RetrievalFunnelConfig {
    fn default() -> Self {
        Self {
            binary_dimensions: 128,
            stage1_top_k: 32,
            stage2_top_n: 16,
            query_limits: QueryLimits {
                max_depth: 6,
                max_paths: 16,
                max_edges_visited: 2_048,
            },
            rerank_config: RerankConfig::default(),
            bayesian_config: BayesianRankerConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalFunnelRequest {
    pub task: String,
    pub mode: String,
    pub token_budget: usize,
    pub exact_seeds: Vec<String>,
    pub stage0_candidates: Vec<RetrievalDocument>,
    pub sources: BTreeMap<String, String>,
}

impl RetrievalFunnelRequest {
    pub fn new(task: impl Into<String>, mode: impl Into<String>, token_budget: usize) -> Self {
        Self {
            task: task.into(),
            mode: mode.into(),
            token_budget,
            exact_seeds: Vec::new(),
            stage0_candidates: Vec::new(),
            sources: BTreeMap::new(),
        }
    }

    pub fn exact_seeds(mut self, exact_seeds: Vec<String>) -> Self {
        self.exact_seeds = exact_seeds;
        self
    }

    pub fn stage0_candidates(mut self, candidates: Vec<RetrievalDocument>) -> Self {
        self.stage0_candidates = candidates;
        self
    }

    pub fn sources(mut self, sources: BTreeMap<String, String>) -> Self {
        self.sources = sources;
        self
    }
}

pub const BAYESIAN_FEATURE_NAMES: [&str; 13] = [
    "exact_symbol_match",
    "bm25_score",
    "binary_hamming_score",
    "rerank_score",
    "graph_distance",
    "path_length",
    "relation_signature",
    "type_validity",
    "edge_confidence",
    "file_centrality",
    "test_failure_link",
    "recent_edit_link",
    "security_relation_presence",
];

#[derive(Debug, Clone, PartialEq)]
pub struct RankingFeatures {
    pub exact_symbol_match: f64,
    pub bm25_score: f64,
    pub binary_hamming_score: f64,
    pub rerank_score: f64,
    pub graph_distance: f64,
    pub path_length: f64,
    pub relation_signature: f64,
    pub type_validity: f64,
    pub edge_confidence: f64,
    pub file_centrality: f64,
    pub test_failure_link: f64,
    pub recent_edit_link: f64,
    pub security_relation_presence: f64,
}

impl Default for RankingFeatures {
    fn default() -> Self {
        Self {
            exact_symbol_match: 0.0,
            bm25_score: 0.0,
            binary_hamming_score: 0.0,
            rerank_score: 0.0,
            graph_distance: 0.0,
            path_length: 0.0,
            relation_signature: 0.0,
            type_validity: 1.0,
            edge_confidence: 1.0,
            file_centrality: 0.0,
            test_failure_link: 0.0,
            recent_edit_link: 0.0,
            security_relation_presence: 0.0,
        }
    }
}

impl RankingFeatures {
    pub fn feature_value(&self, name: &str) -> Option<f64> {
        match name {
            "exact_symbol_match" => Some(self.exact_symbol_match),
            "bm25_score" => Some(self.bm25_score),
            "binary_hamming_score" => Some(self.binary_hamming_score),
            "rerank_score" => Some(self.rerank_score),
            "graph_distance" => Some(self.graph_distance),
            "path_length" => Some(self.path_length),
            "relation_signature" => Some(self.relation_signature),
            "type_validity" => Some(self.type_validity),
            "edge_confidence" => Some(self.edge_confidence),
            "file_centrality" => Some(self.file_centrality),
            "test_failure_link" => Some(self.test_failure_link),
            "recent_edit_link" => Some(self.recent_edit_link),
            "security_relation_presence" => Some(self.security_relation_presence),
            _ => None,
        }
    }

    fn weighted_sum(&self, weights: &RankingFeatureWeights) -> f64 {
        (weights.exact_symbol_match * self.exact_symbol_match)
            + (weights.bm25_score * self.bm25_score)
            + (weights.binary_hamming_score * self.binary_hamming_score)
            + (weights.rerank_score * self.rerank_score)
            + (weights.graph_distance * self.graph_distance)
            + (weights.path_length * self.path_length)
            + (weights.relation_signature * self.relation_signature)
            + (weights.type_validity * self.type_validity)
            + (weights.edge_confidence * self.edge_confidence)
            + (weights.file_centrality * self.file_centrality)
            + (weights.test_failure_link * self.test_failure_link)
            + (weights.recent_edit_link * self.recent_edit_link)
            + (weights.security_relation_presence * self.security_relation_presence)
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "exact_symbol_match": self.exact_symbol_match,
            "bm25_score": self.bm25_score,
            "binary_hamming_score": self.binary_hamming_score,
            "rerank_score": self.rerank_score,
            "graph_distance": self.graph_distance,
            "path_length": self.path_length,
            "relation_signature": self.relation_signature,
            "type_validity": self.type_validity,
            "edge_confidence": self.edge_confidence,
            "file_centrality": self.file_centrality,
            "test_failure_link": self.test_failure_link,
            "recent_edit_link": self.recent_edit_link,
            "security_relation_presence": self.security_relation_presence,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankingFeatureWeights {
    pub exact_symbol_match: f64,
    pub bm25_score: f64,
    pub binary_hamming_score: f64,
    pub rerank_score: f64,
    pub graph_distance: f64,
    pub path_length: f64,
    pub relation_signature: f64,
    pub type_validity: f64,
    pub edge_confidence: f64,
    pub file_centrality: f64,
    pub test_failure_link: f64,
    pub recent_edit_link: f64,
    pub security_relation_presence: f64,
}

impl Default for RankingFeatureWeights {
    fn default() -> Self {
        Self {
            exact_symbol_match: 2.0,
            bm25_score: 0.7,
            binary_hamming_score: 0.45,
            rerank_score: 0.9,
            graph_distance: 0.9,
            path_length: 0.55,
            relation_signature: 0.75,
            type_validity: 1.1,
            edge_confidence: 1.35,
            file_centrality: 0.25,
            test_failure_link: 0.5,
            recent_edit_link: 0.35,
            security_relation_presence: 0.45,
        }
    }
}

impl RankingFeatureWeights {
    fn set(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "exact_symbol_match" => self.exact_symbol_match = value,
            "bm25_score" => self.bm25_score = value,
            "binary_hamming_score" => self.binary_hamming_score = value,
            "rerank_score" => self.rerank_score = value,
            "graph_distance" => self.graph_distance = value,
            "path_length" => self.path_length = value,
            "relation_signature" => self.relation_signature = value,
            "type_validity" => self.type_validity = value,
            "edge_confidence" => self.edge_confidence = value,
            "file_centrality" => self.file_centrality = value,
            "test_failure_link" => self.test_failure_link = value,
            "recent_edit_link" => self.recent_edit_link = value,
            "security_relation_presence" => self.security_relation_presence = value,
            other => return Err(format!("unknown Bayesian ranking weight: {other}")),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BayesianRankerConfig {
    pub bias: f64,
    pub uncertainty_weight: f64,
    pub weights: RankingFeatureWeights,
    pub relation_priors: BTreeMap<RelationKind, f64>,
    pub reliability_bucket_count: usize,
}

impl Default for BayesianRankerConfig {
    fn default() -> Self {
        Self {
            bias: -3.25,
            uncertainty_weight: 1.75,
            weights: RankingFeatureWeights::default(),
            relation_priors: default_relation_reliability_priors(),
            reliability_bucket_count: 10,
        }
    }
}

impl BayesianRankerConfig {
    pub fn from_json_str(input: &str) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(input).map_err(|error| error.to_string())?;
        let Some(object) = value.as_object() else {
            return Err("Bayesian ranker config must be a JSON object".to_string());
        };

        let mut config = Self::default();
        if let Some(value) = object.get("bias") {
            config.bias = json_number(value, "bias")?;
        }
        if let Some(value) = object.get("uncertainty_weight") {
            config.uncertainty_weight = json_number(value, "uncertainty_weight")?;
        }
        if let Some(value) = object.get("reliability_bucket_count") {
            let bucket_count = json_number(value, "reliability_bucket_count")?;
            if bucket_count < 1.0 {
                return Err("reliability_bucket_count must be at least 1".to_string());
            }
            config.reliability_bucket_count = bucket_count.round() as usize;
        }
        if let Some(value) = object.get("weights") {
            let Some(weights) = value.as_object() else {
                return Err("weights must be a JSON object".to_string());
            };
            for (name, value) in weights {
                config.weights.set(name, json_number(value, name)?)?;
            }
        }
        if let Some(value) = object.get("relation_priors") {
            let Some(priors) = value.as_object() else {
                return Err("relation_priors must be a JSON object".to_string());
            };
            for (relation, value) in priors {
                let relation = RelationKind::from_str(relation)
                    .map_err(|error| format!("invalid relation_prior key: {error}"))?;
                config
                    .relation_priors
                    .insert(relation, clamp_unit(json_number(value, relation.as_str())?));
            }
        }

        Ok(config)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BayesianScore {
    pub id: String,
    pub source: String,
    pub target: String,
    pub probability: f64,
    pub uncertainty: f64,
    pub logit: f64,
    pub relation_prior: f64,
    pub relation_signature: String,
    pub features: RankingFeatures,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BayesianScoreInput {
    pub id: String,
    pub source: String,
    pub target: String,
    pub features: RankingFeatures,
    pub relation_prior: f64,
    pub uncertainty: f64,
    pub relation_signature: String,
}

impl BayesianScore {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": &self.id,
            "source": &self.source,
            "target": &self.target,
            "probability": self.probability,
            "uncertainty": self.uncertainty,
            "logit": self.logit,
            "relation_prior": self.relation_prior,
            "relation_signature": &self.relation_signature,
            "features": self.features.to_json(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReliabilityBucket {
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub predicted_mean: Option<f64>,
    pub observed_mean: Option<f64>,
    pub count: usize,
}

impl ReliabilityBucket {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "lower_bound": self.lower_bound,
            "upper_bound": self.upper_bound,
            "predicted_mean": self.predicted_mean,
            "observed_mean": self.observed_mean,
            "count": self.count,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationMetrics {
    pub brier_score: Option<f64>,
    pub reliability_buckets: Vec<ReliabilityBucket>,
}

impl CalibrationMetrics {
    pub fn placeholder(scores: &[BayesianScore], bucket_count: usize) -> Self {
        let bucket_count = bucket_count.max(1);
        let mut bucket_scores = vec![Vec::<f64>::new(); bucket_count];
        for score in scores {
            let index = bucket_index(score.probability, bucket_count);
            bucket_scores[index].push(score.probability);
        }

        let reliability_buckets = bucket_scores
            .into_iter()
            .enumerate()
            .map(|(index, probabilities)| {
                let lower_bound = index as f64 / bucket_count as f64;
                let upper_bound = (index + 1) as f64 / bucket_count as f64;
                let predicted_mean = if probabilities.is_empty() {
                    None
                } else {
                    Some(probabilities.iter().sum::<f64>() / probabilities.len() as f64)
                };
                ReliabilityBucket {
                    lower_bound,
                    upper_bound,
                    predicted_mean,
                    observed_mean: None,
                    count: probabilities.len(),
                }
            })
            .collect();

        Self {
            brier_score: None,
            reliability_buckets,
        }
    }

    pub fn with_labels(scores: &[BayesianScore], labels: &BTreeMap<String, bool>) -> Self {
        let mut squared_error_sum = 0.0;
        let mut labeled = 0usize;
        for score in scores {
            if let Some(label) = labels.get(&score.id) {
                let observed = if *label { 1.0 } else { 0.0 };
                squared_error_sum += (score.probability - observed).powi(2);
                labeled += 1;
            }
        }
        let brier_score = (labeled > 0).then_some(squared_error_sum / labeled as f64);

        let mut metrics = Self::placeholder(scores, 10);
        metrics.brier_score = brier_score;
        metrics
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "brier_score": self.brier_score,
            "reliability_buckets": self.reliability_buckets.iter().map(ReliabilityBucket::to_json).collect::<Vec<_>>(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BayesianRanker {
    config: BayesianRankerConfig,
}

impl BayesianRanker {
    pub fn new(config: BayesianRankerConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &BayesianRankerConfig {
        &self.config
    }

    pub fn features_for_path(
        &self,
        candidate_id: &str,
        path: &GraphPath,
        document: Option<&RetrievalDocument>,
        rerank_score: Option<&RerankScore>,
        exact_seed_ids: &[String],
    ) -> RankingFeatures {
        let exact_symbol_match = if exact_seed_ids.iter().any(|seed| seed == candidate_id)
            || rerank_score.is_some_and(|score| score.exact_seed)
            || document
                .and_then(|document| document.metadata.get("exact_symbol_match"))
                .is_some_and(|value| metadata_flag(value))
        {
            1.0
        } else {
            0.0
        };

        RankingFeatures {
            exact_symbol_match,
            bm25_score: document
                .map(|document| clamp_unit(document.stage0_score))
                .unwrap_or(0.0),
            binary_hamming_score: rerank_score
                .and_then(|score| score.components.get("stage1").copied())
                .map(clamp_unit)
                .unwrap_or(0.0),
            rerank_score: rerank_score
                .map(|score| score_to_unit(score.score))
                .unwrap_or(0.0),
            graph_distance: 1.0 / (1.0 + path.cost.max(0.0)),
            path_length: 1.0 / (1.0 + path.steps.len() as f64),
            relation_signature: self.path_relation_prior(path),
            type_validity: type_validity_for_path(path),
            edge_confidence: aggregate_confidence(path),
            file_centrality: document
                .and_then(|document| document.metadata.get("file_centrality"))
                .and_then(|value| value.parse::<f64>().ok())
                .map(clamp_unit)
                .unwrap_or(0.0),
            test_failure_link: document
                .and_then(|document| document.metadata.get("test_failure_link"))
                .map(|value| unit_or_flag(value))
                .unwrap_or_else(|| relation_presence(path, test_relations())),
            recent_edit_link: document
                .and_then(|document| document.metadata.get("recent_edit_link"))
                .map(|value| unit_or_flag(value))
                .unwrap_or(0.0),
            security_relation_presence: relation_presence(path, security_relations()),
        }
    }

    pub fn score_features(&self, input: BayesianScoreInput) -> BayesianScore {
        let relation_prior = clamp_probability(input.relation_prior);
        let uncertainty = input.uncertainty.max(0.0);
        let logit = self.config.bias
            + input.features.weighted_sum(&self.config.weights)
            + probability_logit(relation_prior)
            - (self.config.uncertainty_weight * uncertainty);
        BayesianScore {
            id: input.id,
            source: input.source,
            target: input.target,
            probability: sigmoid(logit),
            uncertainty,
            logit,
            relation_prior,
            relation_signature: input.relation_signature,
            features: input.features,
        }
    }

    pub fn score_path(
        &self,
        candidate_id: &str,
        path: &GraphPath,
        document: Option<&RetrievalDocument>,
        rerank_score: Option<&RerankScore>,
        exact_seed_ids: &[String],
    ) -> BayesianScore {
        let features =
            self.features_for_path(candidate_id, path, document, rerank_score, exact_seed_ids);
        let relation_prior = self.path_relation_prior(path);
        let uncertainty = path_uncertainty(path, relation_prior);
        self.score_features(BayesianScoreInput {
            id: candidate_id.to_string(),
            source: path.source.clone(),
            target: path.target.clone(),
            features,
            relation_prior,
            uncertainty,
            relation_signature: relation_signature_label(path),
        })
    }

    pub fn calibration_placeholder(&self, scores: &[BayesianScore]) -> CalibrationMetrics {
        CalibrationMetrics::placeholder(scores, self.config.reliability_bucket_count)
    }

    fn path_relation_prior(&self, path: &GraphPath) -> f64 {
        if path.steps.is_empty() {
            return 1.0;
        }

        let prior_sum = path
            .steps
            .iter()
            .map(|step| {
                self.config
                    .relation_priors
                    .get(&step.edge.relation)
                    .copied()
                    .unwrap_or(0.65)
            })
            .sum::<f64>();
        clamp_probability(prior_sum / path.steps.len() as f64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalTraceStage {
    pub stage: String,
    pub kept: Vec<String>,
    pub dropped: Vec<String>,
    pub notes: Vec<String>,
}

impl RetrievalTraceStage {
    fn new(
        stage: impl Into<String>,
        kept: Vec<String>,
        dropped: Vec<String>,
        notes: Vec<String>,
    ) -> Self {
        Self {
            stage: stage.into(),
            kept,
            dropped,
            notes,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalFunnelResult {
    pub packet: ContextPacket,
    pub trace: Vec<RetrievalTraceStage>,
    pub rerank_scores: Vec<RerankScore>,
    pub bayesian_scores: Vec<BayesianScore>,
}

#[derive(Debug)]
pub struct RetrievalFunnel {
    engine: ExactGraphQueryEngine,
    config: RetrievalFunnelConfig,
    reranker: DeterministicCompressedReranker,
    bayesian_ranker: BayesianRanker,
    documents: BTreeMap<String, RetrievalDocument>,
}

struct ContextPacketBuild<'a> {
    task: String,
    mode: String,
    token_budget: usize,
    symbols: Vec<String>,
    paths: &'a [GraphPath],
    sources: &'a BTreeMap<String, String>,
    metadata: Metadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PromptSeedKind {
    Symbol,
    FilePath,
    LineNumber,
    StackTrace,
    TestName,
    ErrorMessage,
    Identifier,
}

impl PromptSeedKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::FilePath => "file_path",
            Self::LineNumber => "line_number",
            Self::StackTrace => "stack_trace",
            Self::TestName => "test_name",
            Self::ErrorMessage => "error_message",
            Self::Identifier => "identifier",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PromptSeed {
    pub kind: PromptSeedKind,
    pub value: String,
    pub file_path: Option<String>,
    pub line: Option<u32>,
    pub function: Option<String>,
    pub exact: bool,
}

impl PromptSeed {
    fn simple(kind: PromptSeedKind, value: impl Into<String>, exact: bool) -> Self {
        Self {
            kind,
            value: value.into(),
            file_path: None,
            line: None,
            function: None,
            exact,
        }
    }

    pub fn exact_value(&self) -> Option<String> {
        match self.kind {
            PromptSeedKind::FilePath => Some(self.value.clone()),
            PromptSeedKind::LineNumber => self
                .file_path
                .as_ref()
                .zip(self.line)
                .map(|(path, line)| format!("{path}:{line}"))
                .or_else(|| self.line.map(|line| format!("line:{line}"))),
            PromptSeedKind::StackTrace => self
                .function
                .clone()
                .or_else(|| self.file_path.clone())
                .or_else(|| Some(self.value.clone())),
            PromptSeedKind::Symbol | PromptSeedKind::TestName | PromptSeedKind::Identifier => {
                Some(self.value.clone())
            }
            PromptSeedKind::ErrorMessage => None,
        }
    }
}

pub fn extract_prompt_seeds(prompt: &str) -> Vec<PromptSeed> {
    let mut seeds = Vec::new();
    for line in prompt.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        extract_stack_trace_seed(trimmed, &mut seeds);
        extract_error_seed(trimmed, &mut seeds);
        extract_standalone_line_seed(trimmed, &mut seeds);
        extract_test_name_seeds(trimmed, &mut seeds);

        for token in trimmed.split_whitespace() {
            extract_path_and_line_seed(token, &mut seeds);
            extract_symbol_or_identifier_seed(token, &mut seeds);
        }
    }

    unique_prompt_seeds(seeds)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPackRequest {
    pub task: String,
    pub mode: String,
    pub token_budget: usize,
    pub seeds: Vec<String>,
    pub stage0_candidates: Vec<String>,
}

impl ContextPackRequest {
    pub fn new(
        task: impl Into<String>,
        mode: impl Into<String>,
        token_budget: usize,
        seeds: Vec<String>,
    ) -> Self {
        Self {
            task: task.into(),
            mode: mode.into(),
            token_budget,
            seeds,
            stage0_candidates: Vec::new(),
        }
    }

    pub fn with_stage0_candidates(mut self, candidates: Vec<String>) -> Self {
        self.stage0_candidates = candidates;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ExactGraphQueryEngine {
    edges: Vec<Edge>,
    by_head: BTreeMap<String, Vec<usize>>,
    by_tail: BTreeMap<String, Vec<usize>>,
    relation_costs: BTreeMap<RelationKind, f64>,
}

impl ExactGraphQueryEngine {
    pub fn new(edges: Vec<Edge>) -> Self {
        let mut by_head: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_tail: BTreeMap<String, Vec<usize>> = BTreeMap::new();

        for (index, edge) in edges.iter().enumerate() {
            by_head.entry(edge.head_id.clone()).or_default().push(index);
            by_tail.entry(edge.tail_id.clone()).or_default().push(index);
        }

        Self {
            edges,
            by_head,
            by_tail,
            relation_costs: default_relation_costs(),
        }
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn path_evidence(&self, path: &GraphPath) -> PathEvidence {
        let edge_labels = path
            .steps
            .iter()
            .map(|step| {
                let fact_class = classify_edge_fact(&step.edge);
                serde_json::json!({
                    "edge_id": step.edge.id,
                    "relation": step.edge.relation.to_string(),
                    "exactness": step.edge.exactness.to_string(),
                    "confidence": step.edge.confidence,
                    "extractor": step.edge.extractor,
                    "edge_class": fact_class.as_str(),
                    "context": infer_edge_context(&step.edge).as_str(),
                    "derived": step.edge.derived,
                    "provenance_edges": step.edge.provenance_edges.clone(),
                    "source_span": step.edge.source_span.to_string(),
                    "file_hash": step.edge.file_hash,
                    "fact_class": fact_class.as_str(),
                    "proof_grade_edge_class": fact_class_is_proof_eligible(&step.edge, fact_class),
                })
            })
            .collect::<Vec<_>>();
        let ordered_edge_ids = path.edge_ids();
        let relation_sequence = path
            .relations()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let exactness_labels = path
            .steps
            .iter()
            .map(|step| step.edge.exactness.to_string())
            .collect::<Vec<_>>();
        let confidence_labels = path
            .steps
            .iter()
            .map(|step| step.edge.confidence)
            .collect::<Vec<_>>();
        let derived_provenance_expansion = path
            .steps
            .iter()
            .filter(|step| step.edge.derived || !step.edge.provenance_edges.is_empty())
            .map(|step| {
                serde_json::json!({
                    "edge_id": step.edge.id,
                    "relation": step.edge.relation.to_string(),
                    "derived": step.edge.derived,
                    "provenance_edges": step.edge.provenance_edges.clone(),
                })
            })
            .collect::<Vec<_>>();
        let context_labels = path
            .steps
            .iter()
            .map(|step| infer_edge_context(&step.edge).as_str().to_string())
            .collect::<Vec<_>>();
        let mut metadata = Metadata::new();
        let path_context = path.path_context();
        metadata.insert("cost".to_string(), serde_json::json!(path.cost));
        metadata.insert(
            "uncertainty".to_string(),
            serde_json::json!(path.uncertainty),
        );
        metadata.insert(
            "ordered_edge_ids".to_string(),
            serde_json::json!(ordered_edge_ids),
        );
        metadata.insert(
            "relation_sequence".to_string(),
            serde_json::json!(relation_sequence),
        );
        metadata.insert(
            "source_spans".to_string(),
            serde_json::json!(path.source_spans()),
        );
        metadata.insert(
            "exactness_labels".to_string(),
            serde_json::json!(exactness_labels),
        );
        metadata.insert(
            "confidence_labels".to_string(),
            serde_json::json!(confidence_labels),
        );
        metadata.insert(
            "derived_provenance_expansion".to_string(),
            serde_json::json!(derived_provenance_expansion),
        );
        metadata.insert(
            "production_test_mock_labels".to_string(),
            serde_json::json!(context_labels),
        );
        metadata.insert("edge_labels".to_string(), serde_json::json!(edge_labels));
        metadata.insert(
            "path_context".to_string(),
            serde_json::json!(path_context.as_str()),
        );
        metadata.insert(
            "production_proof_eligible".to_string(),
            serde_json::json!(
                path_context.is_production() && validate_proof_path_edge_classes(path).is_ok()
            ),
        );
        metadata.insert(
            "proof_scope".to_string(),
            serde_json::json!(if path_context.is_production() {
                "production"
            } else {
                "test_or_mock"
            }),
        );
        match validate_proof_path_edge_classes(path) {
            Ok(()) => {
                metadata.insert(
                    "proof_grade_edge_classes".to_string(),
                    serde_json::json!(true),
                );
                metadata.insert(
                    "derived_edges_have_provenance".to_string(),
                    serde_json::json!(true),
                );
            }
            Err(issues) => {
                metadata.insert(
                    "proof_grade_edge_classes".to_string(),
                    serde_json::json!(false),
                );
                metadata.insert(
                    "derived_edges_have_provenance".to_string(),
                    serde_json::json!(issues.iter().all(|issue| {
                        issue.kind != EdgeClassProofIssueKind::DerivedWithoutProvenance
                    })),
                );
                metadata.insert(
                    "edge_class_validation".to_string(),
                    serde_json::json!(if issues.iter().all(|issue| {
                        matches!(
                            issue.kind,
                            EdgeClassProofIssueKind::HeuristicEdge
                                | EdgeClassProofIssueKind::InverseEdge
                                | EdgeClassProofIssueKind::TestMockEdge
                        )
                    }) {
                        "not_proof_grade"
                    } else {
                        "failed"
                    }),
                );
                if issues.iter().any(|issue| {
                    !matches!(
                        issue.kind,
                        EdgeClassProofIssueKind::HeuristicEdge
                            | EdgeClassProofIssueKind::InverseEdge
                            | EdgeClassProofIssueKind::TestMockEdge
                    )
                }) {
                    metadata.insert(
                        "edge_class_issues".to_string(),
                        edge_class_issues_json(&issues),
                    );
                }
            }
        }

        PathEvidence {
            id: path_evidence_id(path),
            summary: Some(path_summary(path)),
            source: path.source.clone(),
            target: path.target.clone(),
            metapath: path.relations(),
            edges: path
                .steps
                .iter()
                .map(|step| {
                    (
                        step.edge.head_id.clone(),
                        step.edge.relation,
                        step.edge.tail_id.clone(),
                    )
                })
                .collect(),
            source_spans: path.source_spans(),
            exactness: aggregate_exactness(path),
            length: path.steps.len(),
            confidence: aggregate_confidence(path),
            metadata,
        }
    }

    pub fn path_evidence_from_paths(&self, paths: &[GraphPath]) -> Vec<PathEvidence> {
        paths.iter().map(|path| self.path_evidence(path)).collect()
    }

    pub fn path_evidence_from_paths_with_source_validation(
        &self,
        paths: &[GraphPath],
        sources: &BTreeMap<String, String>,
    ) -> Vec<PathEvidence> {
        paths
            .iter()
            .map(|path| self.path_evidence_with_source_validation(path, sources))
            .collect()
    }

    fn path_evidence_with_source_validation(
        &self,
        path: &GraphPath,
        sources: &BTreeMap<String, String>,
    ) -> PathEvidence {
        let mut evidence = self.path_evidence(path);
        let path_context = path.path_context();
        let has_validated_edges = path
            .steps
            .iter()
            .any(|step| requires_exact_source_span(step.edge.relation));
        if !path_context.is_production() {
            evidence.metadata.insert(
                "production_proof_eligible".to_string(),
                serde_json::json!(false),
            );
        }
        match validate_proof_path_edge_classes(path) {
            Ok(()) => {
                evidence.metadata.insert(
                    "proof_grade_edge_classes".to_string(),
                    serde_json::json!(true),
                );
                evidence.metadata.insert(
                    "derived_edges_have_provenance".to_string(),
                    serde_json::json!(true),
                );
            }
            Err(issues) => {
                let derived_edges_have_provenance = issues
                    .iter()
                    .all(|issue| issue.kind != EdgeClassProofIssueKind::DerivedWithoutProvenance);
                let contains_only_soft_proof_issues = issues.iter().all(|issue| {
                    matches!(
                        issue.kind,
                        EdgeClassProofIssueKind::HeuristicEdge
                            | EdgeClassProofIssueKind::InverseEdge
                            | EdgeClassProofIssueKind::TestMockEdge
                    )
                });
                evidence.metadata.insert(
                    "proof_grade_edge_classes".to_string(),
                    serde_json::json!(false),
                );
                evidence.metadata.insert(
                    "derived_edges_have_provenance".to_string(),
                    serde_json::json!(derived_edges_have_provenance),
                );
                evidence.metadata.insert(
                    "edge_class_validation".to_string(),
                    serde_json::json!(if contains_only_soft_proof_issues {
                        "not_proof_grade"
                    } else {
                        "failed"
                    }),
                );
                if !contains_only_soft_proof_issues {
                    evidence.metadata.insert(
                        "edge_class_issues".to_string(),
                        edge_class_issues_json(&issues),
                    );
                }
                evidence.metadata.insert(
                    "production_proof_eligible".to_string(),
                    serde_json::json!(false),
                );
                if !contains_only_soft_proof_issues {
                    evidence.exactness = Exactness::Inferred;
                    evidence.confidence = evidence.confidence.min(0.49);
                }
            }
        }

        match validate_proof_path_source_spans(path, sources) {
            Ok(()) => {
                evidence.metadata.insert(
                    "proof_grade_source_spans".to_string(),
                    serde_json::json!(has_validated_edges),
                );
            }
            Err(issues) => {
                evidence.metadata.insert(
                    "proof_grade_source_spans".to_string(),
                    serde_json::json!(false),
                );
                evidence.metadata.insert(
                    "source_span_validation".to_string(),
                    serde_json::json!("failed"),
                );
                evidence.metadata.insert(
                    "source_span_issues".to_string(),
                    source_span_issues_json(&issues),
                );
                evidence.exactness = Exactness::Inferred;
                evidence.confidence = evidence.confidence.min(0.49);
            }
        }

        evidence
    }

    pub fn derive_closure_edges(&self, paths: &[GraphPath]) -> Vec<DerivedClosureEdge> {
        let mut derived = BTreeMap::<(String, RelationKind, String), DerivedClosureEdge>::new();

        for path in paths {
            let Some(relation) = derived_relation_for_path(path) else {
                continue;
            };
            let provenance_edges = path.edge_ids();
            if provenance_edges.is_empty() {
                continue;
            }

            let key = (path.source.clone(), relation, path.target.clone());
            let edge = DerivedClosureEdge {
                id: derived_edge_id(&path.source, relation, &path.target, &provenance_edges),
                head_id: path.source.clone(),
                relation,
                tail_id: path.target.clone(),
                provenance_edges,
                exactness: aggregate_derived_exactness(path),
                confidence: aggregate_confidence(path),
                metadata: derived_metadata(path),
            };
            derived.entry(key).or_insert(edge);
        }

        derived.into_values().collect()
    }

    pub fn context_pack(
        &self,
        request: ContextPackRequest,
        sources: &BTreeMap<String, String>,
    ) -> ContextPacket {
        let prompt_seeds = extract_prompt_seeds(&request.task);
        let exact_seed_values = prompt_seeds
            .iter()
            .filter_map(PromptSeed::exact_value)
            .collect::<Vec<_>>();
        let mut candidate_seeds = merge_seed_values(
            request
                .seeds
                .iter()
                .chain(request.stage0_candidates.iter())
                .chain(exact_seed_values.iter()),
        );
        let limits = QueryLimits {
            max_depth: 6,
            max_paths: 12,
            max_edges_visited: 2_048,
        };
        let mut paths = Vec::new();
        for seed in &candidate_seeds {
            paths.extend(self.context_paths_for_seed(seed, limits));
        }
        let candidate_path_count_before_dedup = paths.len();
        paths = unique_paths(paths);
        let candidate_path_count_after_dedup = paths.len();
        let path_context_counts_before = path_context_counts(&paths);
        let rejected_test_mock_path_count = paths
            .iter()
            .filter(|path| !path_allowed_for_context_mode(path, &request.mode))
            .count();
        paths.retain(|path| path_allowed_for_context_mode(path, &request.mode));
        let candidate_path_count_after_filter = paths.len();
        let path_context_counts_after = path_context_counts(&paths);
        paths.truncate(24);
        let candidate_path_count_after_truncate = paths.len();

        let mut metadata = Metadata::new();
        metadata.insert("phase".to_string(), serde_json::json!("09"));
        metadata.insert("retrieval".to_string(), serde_json::json!("graph-only"));
        metadata.insert(
            "stage0_policy".to_string(),
            serde_json::json!("exact seeds are preserved and bypass vector filters"),
        );
        metadata.insert(
            "prompt_seeds".to_string(),
            serde_json::json!(prompt_seeds
                .iter()
                .map(|seed| {
                    serde_json::json!({
                        "kind": seed.kind.as_str(),
                        "value": &seed.value,
                        "file_path": &seed.file_path,
                        "line": seed.line,
                        "function": &seed.function,
                        "exact": seed.exact,
                    })
                })
                .collect::<Vec<_>>()),
        );
        metadata.insert(
            "exact_seed_count".to_string(),
            serde_json::json!(candidate_seeds.len()),
        );
        metadata.insert(
            "candidate_path_count_before_dedup".to_string(),
            serde_json::json!(candidate_path_count_before_dedup),
        );
        metadata.insert(
            "candidate_path_count_after_dedup".to_string(),
            serde_json::json!(candidate_path_count_after_dedup),
        );
        metadata.insert(
            "candidate_path_count_after_filter".to_string(),
            serde_json::json!(candidate_path_count_after_filter),
        );
        metadata.insert(
            "candidate_path_count_after_truncate".to_string(),
            serde_json::json!(candidate_path_count_after_truncate),
        );
        metadata.insert(
            "token_budget".to_string(),
            serde_json::json!(request.token_budget),
        );
        metadata.insert(
            "path_context_policy".to_string(),
            serde_json::json!(context_mode_policy_label(&request.mode)),
        );
        metadata.insert(
            "test_mock_edges_allowed".to_string(),
            serde_json::json!(context_mode_allows_test_mock_edges(&request.mode)),
        );
        metadata.insert(
            "path_context_counts_before_filter".to_string(),
            path_context_counts_before,
        );
        metadata.insert(
            "path_context_counts_after_filter".to_string(),
            path_context_counts_after,
        );
        metadata.insert(
            "rejected_test_mock_path_count".to_string(),
            serde_json::json!(rejected_test_mock_path_count),
        );

        candidate_seeds.truncate(64);
        self.context_packet_from_paths(ContextPacketBuild {
            task: request.task,
            mode: request.mode,
            token_budget: request.token_budget,
            symbols: candidate_seeds,
            paths: &paths,
            sources,
            metadata,
        })
    }

    pub fn find_callers(&self, entity_id: &str, limits: QueryLimits) -> Vec<GraphPath> {
        self.bounded_bfs(
            entity_id,
            &[Traversal::reverse(RelationKind::Calls)],
            limits,
            &|path| path.last_relation() == Some(RelationKind::Calls),
        )
    }

    pub fn find_callees(&self, entity_id: &str, limits: QueryLimits) -> Vec<GraphPath> {
        self.bounded_bfs(
            entity_id,
            &[Traversal::forward(RelationKind::Calls)],
            limits,
            &|path| path.last_relation() == Some(RelationKind::Calls),
        )
    }

    pub fn find_reads(&self, entity_id: &str, limits: QueryLimits) -> Vec<GraphPath> {
        self.bounded_bfs(
            entity_id,
            &[
                Traversal::forward(RelationKind::Reads),
                Traversal::reverse(RelationKind::Reads),
            ],
            limits,
            &|path| path.last_relation() == Some(RelationKind::Reads),
        )
    }

    pub fn find_writes(&self, entity_id: &str, limits: QueryLimits) -> Vec<GraphPath> {
        self.bounded_bfs(
            entity_id,
            &[
                Traversal::forward(RelationKind::Writes),
                Traversal::reverse(RelationKind::Writes),
            ],
            limits,
            &|path| path.last_relation() == Some(RelationKind::Writes),
        )
    }

    pub fn find_mutations(&self, entity_id: &str, limits: QueryLimits) -> Vec<GraphPath> {
        self.k_shortest_matching(
            entity_id,
            &[
                Traversal::forward(RelationKind::Calls),
                Traversal::forward(RelationKind::Writes),
                Traversal::forward(RelationKind::Mutates),
            ],
            limits,
            &|path| {
                matches!(
                    path.last_relation(),
                    Some(RelationKind::Writes | RelationKind::Mutates)
                )
            },
        )
    }

    pub fn find_dataflow(&self, entity_id: &str, limits: QueryLimits) -> Vec<GraphPath> {
        self.k_shortest_matching(
            entity_id,
            &[
                Traversal::forward(RelationKind::FlowsTo),
                Traversal::reverse(RelationKind::AssignedFrom),
            ],
            limits,
            &|path| !path.steps.is_empty(),
        )
    }

    pub fn find_auth_paths(&self, entity_id: &str, limits: QueryLimits) -> Vec<GraphPath> {
        self.k_shortest_matching(
            entity_id,
            &[
                Traversal::forward(RelationKind::Exposes),
                Traversal::forward(RelationKind::Calls),
                Traversal::forward(RelationKind::Authorizes),
                Traversal::forward(RelationKind::ChecksRole),
                Traversal::forward(RelationKind::ChecksPermission),
            ],
            limits,
            &|path| {
                path.contains_relation(RelationKind::Exposes)
                    && path.steps.iter().any(|step| {
                        matches!(
                            step.edge.relation,
                            RelationKind::Authorizes
                                | RelationKind::ChecksRole
                                | RelationKind::ChecksPermission
                        )
                    })
            },
        )
    }

    pub fn find_event_flow(&self, entity_id: &str, limits: QueryLimits) -> Vec<GraphPath> {
        let mut results = Vec::new();
        let mut visited = 0usize;

        for publish_step in self.neighbors(
            entity_id,
            &[
                Traversal::forward(RelationKind::Publishes),
                Traversal::forward(RelationKind::Emits),
            ],
        ) {
            visited += 1;
            if visited > limits.max_edges_visited {
                break;
            }

            let event_node = publish_step.to.clone();
            for consumer_step in self.neighbors(
                &event_node,
                &[
                    Traversal::reverse(RelationKind::Consumes),
                    Traversal::reverse(RelationKind::ListensTo),
                    Traversal::reverse(RelationKind::SubscribesTo),
                ],
            ) {
                visited += 1;
                if visited > limits.max_edges_visited {
                    break;
                }

                let base =
                    self.path_from_steps(entity_id, vec![publish_step.clone(), consumer_step]);
                results.push(base.clone());
                if results.len() >= limits.max_paths {
                    return sorted_paths(results);
                }

                let remaining = limits.max_depth.saturating_sub(base.steps.len());
                if remaining == 0 {
                    continue;
                }

                let mut call_limits = limits;
                call_limits.max_depth = remaining;
                call_limits.max_paths = limits.max_paths.saturating_sub(results.len());
                for extension in self.k_shortest_matching(
                    &base.target,
                    &[Traversal::forward(RelationKind::Calls)],
                    call_limits,
                    &|path| !path.steps.is_empty(),
                ) {
                    let mut steps = base.steps.clone();
                    steps.extend(extension.steps);
                    results.push(self.path_from_steps(entity_id, steps));
                    if results.len() >= limits.max_paths {
                        return sorted_paths(results);
                    }
                }
            }
        }

        sorted_paths(results)
    }

    pub fn find_tests(&self, entity_id: &str, limits: QueryLimits) -> Vec<GraphPath> {
        self.k_shortest_matching(
            entity_id,
            &[
                Traversal::reverse(RelationKind::Tests),
                Traversal::reverse(RelationKind::Covers),
                Traversal::reverse(RelationKind::Asserts),
                Traversal::reverse(RelationKind::Mocks),
                Traversal::reverse(RelationKind::Stubs),
                Traversal::reverse(RelationKind::FixturesFor),
            ],
            limits,
            &|path| !path.steps.is_empty(),
        )
    }

    pub fn find_migrations(&self, entity_id: &str, limits: QueryLimits) -> Vec<GraphPath> {
        self.k_shortest_matching(
            entity_id,
            &[
                Traversal::reverse(RelationKind::Migrates),
                Traversal::reverse(RelationKind::AltersColumn),
                Traversal::reverse(RelationKind::DependsOnSchema),
                Traversal::reverse(RelationKind::ReadsTable),
                Traversal::reverse(RelationKind::WritesTable),
                Traversal::forward(RelationKind::Migrates),
                Traversal::forward(RelationKind::AltersColumn),
                Traversal::forward(RelationKind::DependsOnSchema),
            ],
            limits,
            &|path| !path.steps.is_empty(),
        )
    }

    pub fn trace_path(
        &self,
        source_id: &str,
        target_id: &str,
        allowed_relations: &[RelationKind],
        limits: QueryLimits,
    ) -> Vec<GraphPath> {
        let traversals = allowed_relations
            .iter()
            .copied()
            .flat_map(|relation| [Traversal::forward(relation), Traversal::reverse(relation)])
            .collect::<Vec<_>>();

        self.k_shortest_matching(source_id, &traversals, limits, &|path| {
            path.target == target_id
        })
    }

    pub fn impact_analysis_core(&self, entity_id: &str, limits: QueryLimits) -> ImpactAnalysis {
        ImpactAnalysis {
            source: entity_id.to_string(),
            callers: self.find_callers(entity_id, limits),
            callees: self.find_callees(entity_id, limits),
            reads: self.find_reads(entity_id, limits),
            writes: self.find_writes(entity_id, limits),
            mutations: self.find_mutations(entity_id, limits),
            dataflow: self.find_dataflow(entity_id, limits),
            auth_paths: self.find_auth_paths(entity_id, limits),
            event_flow: self.find_event_flow(entity_id, limits),
            tests: self.find_tests(entity_id, limits),
            migrations: self.find_migrations(entity_id, limits),
        }
    }

    fn context_paths_for_seed(&self, seed: &str, limits: QueryLimits) -> Vec<GraphPath> {
        let mut paths = Vec::new();
        paths.extend(self.find_mutations(seed, limits));
        paths.extend(self.find_reads(seed, limits));
        paths.extend(self.find_writes(seed, limits));
        paths.extend(self.find_dataflow(seed, limits));
        paths.extend(self.find_auth_paths(seed, limits));
        paths.extend(self.find_event_flow(seed, limits));
        paths.extend(self.find_migrations(seed, limits));
        paths.extend(self.find_tests(seed, limits));
        paths.extend(self.find_callers(seed, limits));
        paths.extend(self.find_callees(seed, limits));
        sorted_paths(paths)
    }

    fn context_packet_from_paths(&self, build: ContextPacketBuild<'_>) -> ContextPacket {
        let mut metadata = build.metadata;
        let derived_edges = self.derive_closure_edges(build.paths);
        metadata.insert(
            "derived_edges".to_string(),
            serde_json::json!(derived_edges
                .iter()
                .map(|edge| {
                    serde_json::json!({
                        "id": edge.id,
                        "head_id": edge.head_id,
                        "relation": edge.relation.to_string(),
                        "tail_id": edge.tail_id,
                        "exactness": edge.exactness.to_string(),
                        "confidence": edge.confidence,
                    })
                })
                .collect::<Vec<_>>()),
        );

        let mut packet = ContextPacket {
            task: build.task,
            mode: build.mode,
            symbols: build.symbols,
            verified_paths: self
                .path_evidence_from_paths_with_source_validation(build.paths, build.sources),
            risks: risk_summaries(build.paths),
            recommended_tests: recommended_tests_for_paths(build.paths),
            snippets: snippets_for_paths(build.paths, build.sources),
            metadata,
        };
        compact_packet(&mut packet, build.token_budget.max(32));
        packet
    }

    pub fn bounded_bfs(
        &self,
        source_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
        accept: &impl Fn(&GraphPath) -> bool,
    ) -> Vec<GraphPath> {
        let mut queue = VecDeque::from([PathState::new(source_id)]);
        let mut results = Vec::new();
        let mut visited_edges = 0usize;

        while let Some(state) = queue.pop_front() {
            if state.steps.len() >= limits.max_depth {
                continue;
            }

            for step in self.neighbors(&state.node, traversals) {
                visited_edges += 1;
                if visited_edges > limits.max_edges_visited {
                    return sorted_paths(results);
                }
                if state.seen_nodes.contains(&step.to) {
                    continue;
                }

                let next = state.extend(step);
                let path = self.path_from_steps(source_id, next.steps.clone());
                if accept(&path) {
                    results.push(path);
                    if results.len() >= limits.max_paths {
                        return sorted_paths(results);
                    }
                }
                queue.push_back(next);
            }
        }

        sorted_paths(results)
    }

    pub fn dijkstra(
        &self,
        source_id: &str,
        target_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
    ) -> Option<GraphPath> {
        self.k_shortest_matching(source_id, traversals, limits, &|path| {
            path.target == target_id
        })
        .into_iter()
        .next()
    }

    pub fn k_shortest_paths(
        &self,
        source_id: &str,
        target_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
    ) -> Vec<GraphPath> {
        self.k_shortest_matching(source_id, traversals, limits, &|path| {
            path.target == target_id
        })
    }

    pub fn k_shortest_matching(
        &self,
        source_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
        accept: &impl Fn(&GraphPath) -> bool,
    ) -> Vec<GraphPath> {
        let mut heap = BinaryHeap::new();
        let mut sequence = 0usize;
        heap.push(HeapState::new(sequence, PathState::new(source_id), 0.0));
        sequence += 1;

        let mut results = Vec::new();
        let mut visited_edges = 0usize;

        while let Some(heap_state) = heap.pop() {
            let state = heap_state.path;
            let state_depth = state.steps.len();

            let path = self.path_from_steps(source_id, state.steps.clone());
            if !path.steps.is_empty() && accept(&path) {
                results.push(path);
                if results.len() >= limits.max_paths {
                    return sorted_paths(results);
                }
            }

            if state_depth >= limits.max_depth {
                continue;
            }

            for step in self.neighbors(&state.node, traversals) {
                visited_edges += 1;
                if visited_edges > limits.max_edges_visited {
                    return sorted_paths(results);
                }
                if state.seen_nodes.contains(&step.to) {
                    continue;
                }

                let added_cost = self.step_cost(&step, state_depth + 1);
                let next = state.extend(step);
                let next_cost = heap_state.cost + added_cost;
                heap.push(HeapState::new(sequence, next, next_cost));
                sequence += 1;
            }
        }

        sorted_paths(results)
    }

    fn neighbors(&self, node_id: &str, traversals: &[Traversal]) -> Vec<TraversalStep> {
        let mut steps = Vec::new();

        for traversal in traversals {
            match traversal.direction {
                TraversalDirection::Forward => {
                    if let Some(indices) = self.by_head.get(node_id) {
                        for index in indices {
                            let edge = &self.edges[*index];
                            if edge.relation == traversal.relation {
                                steps.push(TraversalStep {
                                    edge: edge.clone(),
                                    direction: TraversalDirection::Forward,
                                    from: edge.head_id.clone(),
                                    to: edge.tail_id.clone(),
                                });
                            }
                        }
                    }
                }
                TraversalDirection::Reverse => {
                    if let Some(indices) = self.by_tail.get(node_id) {
                        for index in indices {
                            let edge = &self.edges[*index];
                            if edge.relation == traversal.relation {
                                steps.push(TraversalStep {
                                    edge: edge.clone(),
                                    direction: TraversalDirection::Reverse,
                                    from: edge.tail_id.clone(),
                                    to: edge.head_id.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        steps.sort_by(|left, right| {
            left.to
                .cmp(&right.to)
                .then_with(|| left.edge.id.cmp(&right.edge.id))
        });
        steps
    }

    fn path_from_steps(&self, source_id: &str, steps: Vec<TraversalStep>) -> GraphPath {
        let target = steps
            .last()
            .map(|step| step.to.clone())
            .unwrap_or_else(|| source_id.to_string());
        let uncertainty = steps.iter().map(edge_uncertainty).sum::<f64>();
        let cost = steps
            .iter()
            .enumerate()
            .map(|(index, step)| self.step_cost(step, index + 1))
            .sum::<f64>();

        GraphPath {
            source: source_id.to_string(),
            target,
            steps,
            cost,
            uncertainty,
        }
    }

    fn step_cost(&self, step: &TraversalStep, depth: usize) -> f64 {
        let relation_cost = self
            .relation_costs
            .get(&step.edge.relation)
            .copied()
            .unwrap_or(1.0);
        relation_cost + edge_uncertainty(step) + (depth as f64 * 0.05)
    }
}

impl RetrievalFunnel {
    pub fn new(
        edges: Vec<Edge>,
        documents: Vec<RetrievalDocument>,
        config: RetrievalFunnelConfig,
    ) -> Result<Self, BinaryVectorError> {
        let _ = InMemoryBinaryVectorIndex::new(config.binary_dimensions)?;
        let mut document_map = BTreeMap::new();
        for document in documents {
            document_map.insert(document.id.clone(), document);
        }

        Ok(Self {
            engine: ExactGraphQueryEngine::new(edges),
            reranker: DeterministicCompressedReranker::new(config.rerank_config.clone()),
            bayesian_ranker: BayesianRanker::new(config.bayesian_config.clone()),
            config,
            documents: document_map,
        })
    }

    pub fn run(
        &self,
        request: RetrievalFunnelRequest,
    ) -> Result<RetrievalFunnelResult, BinaryVectorError> {
        let prompt_seeds = extract_prompt_seeds(&request.task);
        let prompt_exact_seeds = prompt_seeds
            .iter()
            .filter_map(PromptSeed::exact_value)
            .collect::<Vec<_>>();
        let request_stage0_ids = request
            .stage0_candidates
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        let exact_seed_ids =
            merge_seed_values(request.exact_seeds.iter().chain(prompt_exact_seeds.iter()));
        let mut documents = self.documents.clone();
        for candidate in request.stage0_candidates {
            documents.insert(candidate.id.clone(), candidate);
        }
        for seed in &exact_seed_ids {
            documents
                .entry(seed.clone())
                .or_insert_with(|| RetrievalDocument::new(seed, seed));
        }
        let mut binary_index = InMemoryBinaryVectorIndex::new(self.config.binary_dimensions)?;
        for document in documents.values() {
            binary_index.upsert_text(&document.id, &document.text)?;
        }

        let stage0_ids = merge_seed_values(
            exact_seed_ids
                .iter()
                .chain(request_stage0_ids.iter())
                .chain(documents.keys()),
        );
        let mut trace = vec![RetrievalTraceStage::new(
            "stage0_exact_seed_extraction",
            stage0_ids.clone(),
            Vec::new(),
            vec![
                "prompt seeds, explicit exact seeds, and Stage 0 candidates are unioned"
                    .to_string(),
                "exact seeds bypass vector filters".to_string(),
            ],
        )];

        let query_signature =
            BinarySignature::from_text(&request.task, self.config.binary_dimensions)?;
        let stage1_candidates = binary_index.search_with_exact_seeds(
            &query_signature,
            self.config.stage1_top_k,
            &exact_seed_ids,
        )?;
        let stage1_ids = stage1_candidates
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        let stage1_dropped = dropped_ids(&stage0_ids, &stage1_ids, &exact_seed_ids);
        trace.push(RetrievalTraceStage::new(
            "stage1_binary_sieve",
            stage1_ids.clone(),
            stage1_dropped,
            vec!["binary candidates are suggestions only".to_string()],
        ));

        let rerank_candidates = stage1_candidates
            .iter()
            .map(|candidate| {
                let document = documents
                    .get(&candidate.id)
                    .cloned()
                    .unwrap_or_else(|| RetrievalDocument::new(&candidate.id, &candidate.id));
                let mut rerank = RerankCandidate::new(document.id.clone(), document.text.clone())
                    .stage0_score(document.stage0_score)
                    .exact_seed(candidate.exact_seed || exact_seed_ids.contains(&document.id));
                if let Some(similarity) = candidate.similarity {
                    rerank = rerank.stage1_similarity(similarity);
                }
                rerank.metadata = document.metadata;
                rerank
            })
            .collect::<Vec<_>>();
        let raw_scores = self.reranker.rerank(
            &RerankQuery::new(&request.task),
            &rerank_candidates,
            self.config.stage2_top_n,
        )?;
        let rerank_scores = preserve_exact_rerank_scores(raw_scores, &rerank_candidates);
        let rerank_scores_by_id = rerank_scores
            .iter()
            .map(|score| (score.id.clone(), score.clone()))
            .collect::<BTreeMap<_, _>>();
        let stage2_ids = rerank_scores
            .iter()
            .map(|score| score.id.clone())
            .collect::<Vec<_>>();
        let stage2_dropped = dropped_ids(&stage1_ids, &stage2_ids, &exact_seed_ids);
        trace.push(RetrievalTraceStage::new(
            "stage2_compressed_rerank",
            stage2_ids.clone(),
            stage2_dropped,
            vec![
                "deterministic local reranker returns candidates for graph verification"
                    .to_string(),
                "exact seeds are preserved even if top-N is small".to_string(),
            ],
        ));

        let mut verified_paths = Vec::new();
        let mut verified_seed_ids = BTreeSet::new();
        let mut stage3_dropped = Vec::new();
        let mut heuristic_labels = Vec::new();
        for id in &stage2_ids {
            let paths = unique_paths(
                self.engine
                    .context_paths_for_seed(id, self.config.query_limits),
            );
            if paths.is_empty() {
                stage3_dropped.push(id.clone());
                continue;
            }

            if paths.iter().any(|path| {
                matches!(
                    aggregate_exactness(path),
                    Exactness::StaticHeuristic | Exactness::Inferred
                )
            }) {
                heuristic_labels.push(id.clone());
            }
            verified_seed_ids.insert(id.clone());
            verified_paths.extend(paths);
        }
        verified_paths = unique_paths(verified_paths);
        let verified_symbols = verified_seed_ids.into_iter().collect::<Vec<_>>();
        trace.push(RetrievalTraceStage::new(
            "stage3_exact_graph_verification",
            verified_symbols.clone(),
            stage3_dropped,
            vec![
                "graph/source verification controls final packet membership".to_string(),
                format!(
                    "heuristic_or_inferred_paths_labeled={}",
                    heuristic_labels.len()
                ),
            ],
        ));

        let mut scored_paths = verified_paths
            .into_iter()
            .map(|path| {
                let candidate_id = path.source.clone();
                let mut score = self.bayesian_ranker.score_path(
                    &candidate_id,
                    &path,
                    documents.get(&candidate_id),
                    rerank_scores_by_id.get(&candidate_id),
                    &exact_seed_ids,
                );
                score.id = path_evidence_id(&path);
                (path, score)
            })
            .collect::<Vec<_>>();
        scored_paths.sort_by(|(left_path, left_score), (right_path, right_score)| {
            right_score
                .probability
                .total_cmp(&left_score.probability)
                .then_with(|| left_score.uncertainty.total_cmp(&right_score.uncertainty))
                .then_with(|| left_path.cost.total_cmp(&right_path.cost))
                .then_with(|| left_path.target.cmp(&right_path.target))
        });
        let bayesian_scores = scored_paths
            .iter()
            .map(|(_path, score)| score.clone())
            .collect::<Vec<_>>();
        let verified_paths = scored_paths
            .into_iter()
            .map(|(path, _score)| path)
            .collect::<Vec<_>>();
        let calibration_metrics = self
            .bayesian_ranker
            .calibration_placeholder(&bayesian_scores);
        let packet_confidence = bayesian_scores
            .iter()
            .map(|score| score.probability)
            .fold(0.0_f64, f64::max);
        let packet_uncertainty = if bayesian_scores.is_empty() {
            1.0
        } else {
            bayesian_scores
                .iter()
                .map(|score| score.uncertainty)
                .sum::<f64>()
                / bayesian_scores.len() as f64
        };

        let mut metadata = Metadata::new();
        metadata.insert("phase".to_string(), serde_json::json!("14"));
        metadata.insert(
            "retrieval".to_string(),
            serde_json::json!("corrected-runtime-funnel"),
        );
        metadata.insert(
            "policy".to_string(),
            serde_json::json!("vectors suggest; graph verifies; packet proves"),
        );
        metadata.insert("heavy_kge_required".to_string(), serde_json::json!(false));
        metadata.insert(
            "trace".to_string(),
            serde_json::json!(trace.iter().map(trace_stage_json).collect::<Vec<_>>()),
        );
        metadata.insert(
            "rerank_scores".to_string(),
            serde_json::json!(rerank_scores
                .iter()
                .map(|score| {
                    serde_json::json!({
                        "id": &score.id,
                        "score": score.score,
                        "exact_seed": score.exact_seed,
                    })
                })
                .collect::<Vec<_>>()),
        );
        let mut packet = self.engine.context_packet_from_paths(ContextPacketBuild {
            task: request.task,
            mode: request.mode,
            token_budget: request.token_budget,
            symbols: verified_symbols,
            paths: &verified_paths,
            sources: &request.sources,
            metadata,
        });

        trace.push(RetrievalTraceStage::new(
            "stage4_context_packet",
            packet.symbols.clone(),
            Vec::new(),
            vec![format!(
                "packet_paths={}, snippets={}",
                packet.verified_paths.len(),
                packet.snippets.len()
            )],
        ));
        packet.metadata.insert(
            "trace".to_string(),
            serde_json::json!(trace.iter().map(trace_stage_json).collect::<Vec<_>>()),
        );
        packet.metadata.insert(
            "bayesian_ranker".to_string(),
            serde_json::json!("deterministic-logistic"),
        );
        packet.metadata.insert(
            "confidence".to_string(),
            serde_json::json!(packet_confidence),
        );
        packet.metadata.insert(
            "uncertainty".to_string(),
            serde_json::json!(packet_uncertainty),
        );
        packet.metadata.insert(
            "bayesian_scores".to_string(),
            serde_json::json!(bayesian_scores
                .iter()
                .map(BayesianScore::to_json)
                .collect::<Vec<_>>()),
        );
        packet.metadata.insert(
            "calibration_metrics".to_string(),
            calibration_metrics.to_json(),
        );

        Ok(RetrievalFunnelResult {
            packet,
            trace,
            rerank_scores,
            bayesian_scores,
        })
    }
}

#[derive(Debug, Clone)]
struct PathState {
    node: String,
    steps: Vec<TraversalStep>,
    seen_nodes: BTreeSet<String>,
}

impl PathState {
    fn new(source: &str) -> Self {
        Self {
            node: source.to_string(),
            steps: Vec::new(),
            seen_nodes: BTreeSet::from([source.to_string()]),
        }
    }

    fn extend(&self, step: TraversalStep) -> Self {
        let mut steps = self.steps.clone();
        let node = step.to.clone();
        steps.push(step);
        let mut seen_nodes = self.seen_nodes.clone();
        seen_nodes.insert(node.clone());
        Self {
            node,
            steps,
            seen_nodes,
        }
    }
}

#[derive(Debug, Clone)]
struct HeapState {
    sequence: usize,
    path: PathState,
    cost: f64,
}

impl HeapState {
    fn new(sequence: usize, path: PathState, cost: f64) -> Self {
        Self {
            sequence,
            path,
            cost,
        }
    }
}

impl PartialEq for HeapState {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.sequence == other.sequence
    }
}

impl Eq for HeapState {}

impl PartialOrd for HeapState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

fn extract_stack_trace_seed(line: &str, seeds: &mut Vec<PromptSeed>) {
    let trimmed = line.trim_start();
    let Some(after_at) = trimmed.strip_prefix("at ") else {
        return;
    };

    let (function, location) = if let Some((function, rest)) = after_at.split_once(" (") {
        (Some(clean_symbol(function)), rest.trim_end_matches(')'))
    } else if let Some((function, rest)) = after_at.split_once(' ') {
        (Some(clean_symbol(function)), rest)
    } else {
        (None, after_at)
    };

    if let Some((path, line_number, _column)) = parse_path_line_token(location) {
        seeds.push(PromptSeed {
            kind: PromptSeedKind::StackTrace,
            value: match (&function, line_number) {
                (Some(function), Some(line_number)) => format!("{function} {path}:{line_number}"),
                (Some(function), None) => format!("{function} {path}"),
                (None, Some(line_number)) => format!("{path}:{line_number}"),
                (None, None) => path.clone(),
            },
            file_path: Some(path.clone()),
            line: line_number,
            function: function.clone(),
            exact: true,
        });
        if let Some(function) = function.filter(|function| !function.is_empty()) {
            seeds.push(PromptSeed::simple(PromptSeedKind::Symbol, function, true));
        }
    }
}

fn extract_error_seed(line: &str, seeds: &mut Vec<PromptSeed>) {
    let lower = line.to_ascii_lowercase();
    let looks_like_error = lower.contains("error:")
        || lower.contains("exception")
        || lower.contains("panic")
        || lower.contains("failed")
        || lower.contains("failure");
    if looks_like_error {
        seeds.push(PromptSeed::simple(
            PromptSeedKind::ErrorMessage,
            line.chars().take(200).collect::<String>(),
            false,
        ));
    }
}

fn extract_standalone_line_seed(line: &str, seeds: &mut Vec<PromptSeed>) {
    let lower = line.to_ascii_lowercase();
    for marker in ["line ", "line:"] {
        let Some(index) = lower.find(marker) else {
            continue;
        };
        let after = &line[index + marker.len()..];
        let digits = after
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        let Ok(line_number) = digits.parse::<u32>() else {
            continue;
        };
        seeds.push(PromptSeed {
            kind: PromptSeedKind::LineNumber,
            value: format!("line:{line_number}"),
            file_path: None,
            line: Some(line_number),
            function: None,
            exact: true,
        });
    }
}

fn extract_test_name_seeds(line: &str, seeds: &mut Vec<PromptSeed>) {
    for marker in ["test(", "it(", "describe("] {
        let Some(index) = line.find(marker) else {
            continue;
        };
        let after_marker = &line[index + marker.len()..];
        if let Some(name) = first_quoted_value(after_marker) {
            seeds.push(PromptSeed::simple(PromptSeedKind::TestName, name, true));
        }
    }

    let lower = line.to_ascii_lowercase();
    if lower.contains("test") || lower.contains("spec") {
        for quote in ['"', '\'', '`'] {
            let Some(start) = line.find(quote) else {
                continue;
            };
            let after = &line[start + quote.len_utf8()..];
            let Some(end) = after.find(quote) else {
                continue;
            };
            let value = after[..end].trim();
            if value.len() >= 3 {
                seeds.push(PromptSeed::simple(
                    PromptSeedKind::TestName,
                    value.to_string(),
                    true,
                ));
            }
        }
    }
}

fn extract_path_and_line_seed(token: &str, seeds: &mut Vec<PromptSeed>) {
    let Some((path, line, _column)) = parse_path_line_token(token) else {
        return;
    };
    seeds.push(PromptSeed {
        kind: PromptSeedKind::FilePath,
        value: path.clone(),
        file_path: Some(path.clone()),
        line: None,
        function: None,
        exact: true,
    });
    if let Some(line_number) = line {
        seeds.push(PromptSeed {
            kind: PromptSeedKind::LineNumber,
            value: format!("{path}:{line_number}"),
            file_path: Some(path),
            line: Some(line_number),
            function: None,
            exact: true,
        });
    }
}

fn extract_symbol_or_identifier_seed(token: &str, seeds: &mut Vec<PromptSeed>) {
    let cleaned = clean_symbol(token);
    if cleaned.len() < 3 || looks_like_path(&cleaned) || is_keyword_or_common_word(&cleaned) {
        return;
    }

    if is_symbol(&cleaned) {
        seeds.push(PromptSeed::simple(PromptSeedKind::Symbol, cleaned, true));
    } else if is_identifier(&cleaned) && looks_code_like_identifier(&cleaned) {
        seeds.push(PromptSeed::simple(
            PromptSeedKind::Identifier,
            cleaned,
            true,
        ));
    }
}

fn first_quoted_value(value: &str) -> Option<String> {
    let value = value.trim_start();
    let quote = value.chars().next()?;
    if !matches!(quote, '"' | '\'' | '`') {
        return None;
    }
    let after = &value[quote.len_utf8()..];
    let end = after.find(quote)?;
    let quoted = after[..end].trim();
    if quoted.is_empty() {
        None
    } else {
        Some(quoted.to_string())
    }
}

fn parse_path_line_token(token: &str) -> Option<(String, Option<u32>, Option<u32>)> {
    let cleaned = clean_path_token(token);
    if !looks_like_path(&cleaned) {
        return None;
    }

    let parts = cleaned.rsplitn(3, ':').collect::<Vec<_>>();
    let mut line = None;
    let mut column = None;
    let mut path = cleaned.as_str();

    if let Some(candidate) = parts.first().and_then(|value| value.parse::<u32>().ok()) {
        if parts.len() >= 2 {
            let before_last = parts[1];
            if let Ok(candidate_line) = before_last.parse::<u32>() {
                column = Some(candidate);
                line = Some(candidate_line);
                let suffix_len = parts[0].len() + parts[1].len() + 2;
                path = &cleaned[..cleaned.len().saturating_sub(suffix_len)];
            } else if !before_last.ends_with('\\') {
                line = Some(candidate);
                let suffix_len = parts[0].len() + 1;
                path = &cleaned[..cleaned.len().saturating_sub(suffix_len)];
            }
        }
    }

    let normalized = path.replace('\\', "/");
    if looks_like_path(&normalized) {
        Some((normalized, line, column))
    } else {
        None
    }
}

fn clean_path_token(token: &str) -> String {
    token
        .trim_matches(|ch: char| {
            matches!(
                ch,
                '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | '"' | '\'' | '`'
            )
        })
        .trim_end_matches('.')
        .to_string()
}

fn clean_symbol(token: &str) -> String {
    token
        .trim_matches(|ch: char| {
            matches!(
                ch,
                '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | ':' | '"' | '\'' | '`'
            )
        })
        .trim_end_matches('.')
        .to_string()
}

fn looks_like_path(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let has_supported_extension = [
        ".js", ".jsx", ".ts", ".tsx", ".rs", ".py", ".go", ".java", ".kt", ".cs", ".cpp", ".c",
        ".h", ".hpp", ".sql", ".json", ".toml", ".yaml", ".yml", ".md",
    ]
    .iter()
    .any(|extension| lower.contains(extension));
    has_supported_extension && (value.contains('/') || value.contains('\\') || value.contains('.'))
}

fn is_symbol(value: &str) -> bool {
    value.contains('.')
        && value
            .split('.')
            .all(|part| !part.is_empty() && is_identifier(part))
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

fn looks_code_like_identifier(value: &str) -> bool {
    value.contains('_')
        || value.contains('$')
        || value.chars().any(|ch| ch.is_ascii_uppercase())
        || value.ends_with("Error")
        || value.ends_with("Exception")
}

fn is_keyword_or_common_word(value: &str) -> bool {
    matches!(
        value,
        "about"
            | "after"
            | "before"
            | "break"
            | "change"
            | "class"
            | "const"
            | "error"
            | "false"
            | "file"
            | "from"
            | "function"
            | "import"
            | "line"
            | "return"
            | "test"
            | "true"
            | "with"
    )
}

fn unique_prompt_seeds(seeds: Vec<PromptSeed>) -> Vec<PromptSeed> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for seed in seeds {
        let key = (
            seed.kind,
            seed.value.clone(),
            seed.file_path.clone(),
            seed.line,
            seed.function.clone(),
        );
        if seen.insert(key) {
            unique.push(seed);
        }
    }
    unique
}

fn merge_seed_values<'a>(values: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut merged = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            merged.push(trimmed.to_string());
        }
    }
    merged
}

fn json_number(value: &serde_json::Value, name: &str) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("{name} must be a finite number"))
}

fn clamp_unit(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn metadata_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y"
    )
}

fn unit_or_flag(value: &str) -> f64 {
    value
        .parse::<f64>()
        .ok()
        .map(clamp_unit)
        .unwrap_or_else(|| if metadata_flag(value) { 1.0 } else { 0.0 })
}

fn score_to_unit(score: f64) -> f64 {
    if !score.is_finite() {
        0.0
    } else if score <= 1.0 {
        clamp_unit(score)
    } else {
        score / (1.0 + score)
    }
}

fn clamp_probability(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.001, 0.999)
    } else {
        0.5
    }
}

fn probability_logit(probability: f64) -> f64 {
    let probability = clamp_probability(probability);
    (probability / (1.0 - probability)).ln()
}

fn sigmoid(value: f64) -> f64 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

fn bucket_index(probability: f64, bucket_count: usize) -> usize {
    let bucket_count = bucket_count.max(1);
    let raw = (clamp_unit(probability) * bucket_count as f64).floor() as usize;
    raw.min(bucket_count - 1)
}

fn default_relation_reliability_priors() -> BTreeMap<RelationKind, f64> {
    let mut priors = BTreeMap::new();
    for relation in RelationKind::ALL {
        priors.insert(*relation, 0.68);
    }

    for relation in [
        RelationKind::Contains,
        RelationKind::DefinedIn,
        RelationKind::Defines,
        RelationKind::Declares,
        RelationKind::Exports,
        RelationKind::Imports,
        RelationKind::Calls,
        RelationKind::Callee,
        RelationKind::Argument0,
        RelationKind::Argument1,
        RelationKind::ArgumentN,
        RelationKind::Returns,
        RelationKind::Reads,
        RelationKind::Writes,
        RelationKind::Mutates,
        RelationKind::FlowsTo,
        RelationKind::AssignedFrom,
    ] {
        priors.insert(relation, 0.84);
    }

    for relation in [
        RelationKind::Authorizes,
        RelationKind::ChecksRole,
        RelationKind::ChecksPermission,
        RelationKind::Sanitizes,
        RelationKind::Validates,
        RelationKind::Exposes,
        RelationKind::Publishes,
        RelationKind::Emits,
        RelationKind::Consumes,
        RelationKind::ListensTo,
        RelationKind::SubscribesTo,
        RelationKind::Handles,
        RelationKind::Spawns,
        RelationKind::Awaits,
        RelationKind::Migrates,
        RelationKind::ReadsTable,
        RelationKind::WritesTable,
        RelationKind::AltersColumn,
        RelationKind::Tests,
        RelationKind::Asserts,
        RelationKind::Mocks,
        RelationKind::Stubs,
        RelationKind::Covers,
    ] {
        priors.insert(relation, 0.70);
    }

    for relation in [
        RelationKind::MayMutate,
        RelationKind::MayRead,
        RelationKind::ApiReaches,
        RelationKind::AsyncReaches,
        RelationKind::SchemaImpact,
    ] {
        priors.insert(relation, 0.76);
    }

    priors
}

fn type_validity_for_path(path: &GraphPath) -> f64 {
    if path.steps.iter().all(|step| {
        !step.edge.head_id.trim().is_empty()
            && !step.edge.tail_id.trim().is_empty()
            && step.edge.relation.as_str() == step.edge.relation.to_string()
    }) {
        1.0
    } else {
        0.0
    }
}

fn relation_presence(path: &GraphPath, relations: &[RelationKind]) -> f64 {
    if path
        .steps
        .iter()
        .any(|step| relations.contains(&step.edge.relation))
    {
        1.0
    } else {
        0.0
    }
}

fn security_relations() -> &'static [RelationKind] {
    &[
        RelationKind::Authorizes,
        RelationKind::ChecksRole,
        RelationKind::ChecksPermission,
        RelationKind::Sanitizes,
        RelationKind::Validates,
        RelationKind::Exposes,
        RelationKind::TrustBoundary,
        RelationKind::SourceOfTaint,
        RelationKind::SinksTo,
    ]
}

fn test_relations() -> &'static [RelationKind] {
    &[
        RelationKind::Tests,
        RelationKind::Asserts,
        RelationKind::Mocks,
        RelationKind::Stubs,
        RelationKind::Covers,
        RelationKind::FixturesFor,
    ]
}

fn path_uncertainty(path: &GraphPath, relation_prior: f64) -> f64 {
    let low_confidence = 1.0 - aggregate_confidence(path);
    let prior_uncertainty = 1.0 - clamp_probability(relation_prior);
    path.uncertainty.max(0.0) + low_confidence + prior_uncertainty
}

fn relation_signature_label(path: &GraphPath) -> String {
    let relations = path
        .relations()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if relations.is_empty() {
        "SELF".to_string()
    } else {
        relations.join("->")
    }
}

fn dropped_ids(before: &[String], after: &[String], preserved: &[String]) -> Vec<String> {
    let after = after.iter().collect::<BTreeSet<_>>();
    let preserved = preserved.iter().collect::<BTreeSet<_>>();
    before
        .iter()
        .filter(|id| !after.contains(id) && !preserved.contains(id))
        .cloned()
        .collect()
}

fn preserve_exact_rerank_scores(
    mut scores: Vec<RerankScore>,
    candidates: &[RerankCandidate],
) -> Vec<RerankScore> {
    let existing = scores
        .iter()
        .map(|score| score.id.clone())
        .collect::<BTreeSet<_>>();
    for candidate in candidates.iter().filter(|candidate| candidate.exact_seed) {
        if existing.contains(&candidate.id) {
            continue;
        }

        let mut components = BTreeMap::new();
        components.insert("exact_seed_boost".to_string(), 10.0);
        components.insert("preserved_after_rerank".to_string(), 1.0);
        scores.push(RerankScore {
            id: candidate.id.clone(),
            score: 10.0,
            exact_seed: true,
            components,
        });
    }

    scores.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| right.exact_seed.cmp(&left.exact_seed))
            .then_with(|| left.id.cmp(&right.id))
    });
    scores
}

fn trace_stage_json(stage: &RetrievalTraceStage) -> serde_json::Value {
    serde_json::json!({
        "stage": &stage.stage,
        "kept": &stage.kept,
        "dropped": &stage.dropped,
        "notes": &stage.notes,
    })
}

fn sorted_paths(mut paths: Vec<GraphPath>) -> Vec<GraphPath> {
    paths.sort_by(|left, right| {
        left.cost
            .total_cmp(&right.cost)
            .then_with(|| left.target.cmp(&right.target))
            .then_with(|| left.edge_ids().cmp(&right.edge_ids()))
    });
    paths
}

fn edge_uncertainty(step: &TraversalStep) -> f64 {
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
}

fn default_relation_costs() -> BTreeMap<RelationKind, f64> {
    let mut costs = BTreeMap::new();
    for relation in RelationKind::ALL {
        costs.insert(*relation, 1.0);
    }
    for relation in [
        RelationKind::Calls,
        RelationKind::Reads,
        RelationKind::Writes,
        RelationKind::Mutates,
        RelationKind::FlowsTo,
    ] {
        costs.insert(relation, 0.75);
    }
    for relation in [
        RelationKind::Exposes,
        RelationKind::Authorizes,
        RelationKind::ChecksRole,
        RelationKind::ChecksPermission,
        RelationKind::Publishes,
        RelationKind::Emits,
        RelationKind::Consumes,
        RelationKind::ListensTo,
        RelationKind::Migrates,
        RelationKind::Tests,
        RelationKind::Covers,
    ] {
        costs.insert(relation, 0.90);
    }
    costs
}

fn path_evidence_id(path: &GraphPath) -> String {
    let mut value = format!("{}|{}", path.source, path.target);
    for edge_id in path.edge_ids() {
        value.push('|');
        value.push_str(&edge_id);
    }
    format!("path://{:016x}", fnv64(&value))
}

fn derived_edge_id(
    source: &str,
    relation: RelationKind,
    target: &str,
    provenance_edges: &[String],
) -> String {
    let mut value = format!("{source}|{relation}|{target}");
    for edge_id in provenance_edges {
        value.push('|');
        value.push_str(edge_id);
    }
    format!("derived://{:016x}", fnv64(&value))
}

fn fnv64(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn path_summary(path: &GraphPath) -> String {
    let relations = path
        .relations()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" -> ");
    if relations.is_empty() {
        format!("{} reaches itself", path.source)
    } else {
        format!("{} reaches {} via {}", path.source, path.target, relations)
    }
}

fn aggregate_exactness(path: &GraphPath) -> Exactness {
    path.steps
        .iter()
        .map(|step| step.edge.exactness)
        .max_by_key(|exactness| exactness_rank(*exactness))
        .unwrap_or(Exactness::Exact)
}

fn aggregate_derived_exactness(path: &GraphPath) -> Exactness {
    if path.steps.iter().any(|step| {
        matches!(
            step.edge.exactness,
            Exactness::StaticHeuristic | Exactness::Inferred
        )
    }) {
        Exactness::Inferred
    } else {
        Exactness::DerivedFromVerifiedEdges
    }
}

fn exactness_rank(exactness: Exactness) -> u8 {
    match exactness {
        Exactness::Exact | Exactness::CompilerVerified | Exactness::LspVerified => 0,
        Exactness::ParserVerified => 1,
        Exactness::DynamicTrace => 2,
        Exactness::DerivedFromVerifiedEdges => 3,
        Exactness::StaticHeuristic => 4,
        Exactness::Inferred => 5,
    }
}

fn aggregate_confidence(path: &GraphPath) -> f64 {
    if path.steps.is_empty() {
        return 1.0;
    }
    let min_edge_confidence = path
        .steps
        .iter()
        .map(|step| step.edge.confidence)
        .fold(1.0_f64, f64::min);
    (min_edge_confidence / (1.0 + path.uncertainty)).clamp(0.0, 1.0)
}

fn derived_relation_for_path(path: &GraphPath) -> Option<RelationKind> {
    let relations = path.relations();
    let has_calls = relations.contains(&RelationKind::Calls);
    let has_reads = relations.contains(&RelationKind::Reads);
    let has_write_or_mutate = relations
        .iter()
        .any(|relation| matches!(relation, RelationKind::Writes | RelationKind::Mutates));
    let has_exposes = relations.contains(&RelationKind::Exposes);
    let has_async_source = relations
        .iter()
        .any(|relation| matches!(relation, RelationKind::Publishes | RelationKind::Emits));
    let has_async_sink = relations.iter().any(|relation| {
        matches!(
            relation,
            RelationKind::Consumes | RelationKind::ListensTo | RelationKind::SubscribesTo
        )
    });
    let has_schema = relations.iter().any(|relation| {
        matches!(
            relation,
            RelationKind::Migrates
                | RelationKind::WritesTable
                | RelationKind::AltersColumn
                | RelationKind::DependsOnSchema
        )
    });

    if has_calls && has_write_or_mutate {
        Some(RelationKind::MayMutate)
    } else if has_calls && has_reads {
        Some(RelationKind::MayRead)
    } else if has_exposes && has_calls {
        Some(RelationKind::ApiReaches)
    } else if has_async_source && has_async_sink {
        Some(RelationKind::AsyncReaches)
    } else if has_schema {
        Some(RelationKind::SchemaImpact)
    } else {
        None
    }
}

fn derived_metadata(path: &GraphPath) -> Metadata {
    let mut metadata = Metadata::new();
    metadata.insert("phase".to_string(), serde_json::json!("09"));
    metadata.insert("summary".to_string(), serde_json::json!(path_summary(path)));
    metadata.insert(
        "source_spans".to_string(),
        serde_json::json!(path
            .source_spans()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()),
    );
    metadata
}

fn unique_paths(paths: Vec<GraphPath>) -> Vec<GraphPath> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for path in sorted_paths(paths) {
        let key = path.edge_ids().join("|");
        if seen.insert(key) {
            unique.push(path);
        }
    }
    unique
}

fn snippets_for_paths(
    paths: &[GraphPath],
    sources: &BTreeMap<String, String>,
) -> Vec<ContextSnippet> {
    let mut seen = BTreeSet::new();
    let mut snippets = Vec::new();

    for path in paths {
        for span in path.source_spans() {
            let key = format!(
                "{}:{}-{}",
                span.repo_relative_path, span.start_line, span.end_line
            );
            if !seen.insert(key) {
                continue;
            }
            let excerpt = extract_snippet(&span, sources);
            if excerpt.trim().is_empty() {
                continue;
            }
            snippets.push(ContextSnippet {
                file: span.repo_relative_path.clone(),
                lines: if span.start_line == span.end_line {
                    span.start_line.to_string()
                } else {
                    format!("{}-{}", span.start_line, span.end_line)
                },
                text: excerpt,
                reason: format!("evidence for {}", path_summary(path)),
            });
            if snippets.len() >= 16 {
                return snippets;
            }
        }
    }

    snippets
}

fn validate_edge_source_span(
    edge: &Edge,
    sources: &BTreeMap<String, String>,
) -> Result<(), SourceSpanProofIssue> {
    let span = &edge.source_span;
    if span.repo_relative_path.trim().is_empty()
        || span.start_line == 0
        || span.end_line == 0
        || span.end_line < span.start_line
    {
        return Err(SourceSpanProofIssue::new(
            edge,
            SourceSpanProofIssueKind::MissingSpan,
            "proof-grade edge has no valid source span coordinates",
        ));
    }

    if span.start_column.is_none()
        || span.end_column.is_none()
        || span.start_column == Some(0)
        || span.end_column == Some(0)
    {
        return Err(SourceSpanProofIssue::new(
            edge,
            SourceSpanProofIssueKind::MissingSpan,
            "proof-grade edge requires exact source-span columns",
        ));
    }

    let Some(source) = sources.get(&span.repo_relative_path) else {
        return Err(SourceSpanProofIssue::new(
            edge,
            SourceSpanProofIssueKind::SourceUnavailable,
            "source file was not loaded for source-span validation",
        ));
    };

    let snippet = match exact_span_text(span, source) {
        Ok(snippet) => snippet,
        Err(kind) => {
            return Err(SourceSpanProofIssue::new(
                edge,
                kind,
                "source span could not be resolved to source text",
            ));
        }
    };

    if snippet.trim().is_empty() {
        return Err(SourceSpanProofIssue::new(
            edge,
            SourceSpanProofIssueKind::EmptySnippet,
            "source span resolved to empty source text",
        ));
    }

    if !span_text_matches_relation(edge, &snippet) {
        return Err(SourceSpanProofIssue::new(
            edge,
            SourceSpanProofIssueKind::WrongSyntaxLocation,
            "source span text does not look like the relation syntax site",
        ));
    }

    Ok(())
}

fn exact_span_text(span: &SourceSpan, source: &str) -> Result<String, SourceSpanProofIssueKind> {
    let lines = source.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return Err(SourceSpanProofIssueKind::EmptySnippet);
    }

    let start = span.start_line.saturating_sub(1) as usize;
    let end = span.end_line.saturating_sub(1) as usize;
    if start >= lines.len() || end >= lines.len() || end < start {
        return Err(SourceSpanProofIssueKind::OutOfRange);
    }

    if start == end {
        return slice_line_by_columns(lines[start], span.start_column, span.end_column);
    }

    let mut parts = Vec::with_capacity(end.saturating_sub(start) + 1);
    parts.push(slice_line_by_columns(
        lines[start],
        span.start_column,
        None,
    )?);
    for line in &lines[start + 1..end] {
        parts.push((*line).to_string());
    }
    parts.push(slice_line_by_columns(lines[end], Some(1), span.end_column)?);
    Ok(parts.join("\n"))
}

fn slice_line_by_columns(
    line: &str,
    start_column: Option<u32>,
    end_column: Option<u32>,
) -> Result<String, SourceSpanProofIssueKind> {
    let chars = line.chars().collect::<Vec<_>>();
    let start = start_column.unwrap_or(1).saturating_sub(1) as usize;
    let end = end_column
        .map(|column| column.saturating_sub(1) as usize)
        .unwrap_or(chars.len());
    if start > chars.len() || end > chars.len() || end < start {
        return Err(SourceSpanProofIssueKind::OutOfRange);
    }

    Ok(chars[start..end].iter().collect())
}

fn span_text_matches_relation(edge: &Edge, snippet: &str) -> bool {
    let trimmed = snippet.trim();
    let lower = trimmed.to_ascii_lowercase();
    let tokens = edge_identity_tokens(edge);
    let has_identity_tokens = !tokens.is_empty();
    let has_token = tokens
        .iter()
        .any(|token| lower.contains(&token.to_ascii_lowercase()));
    let token_match = !has_identity_tokens || has_token;

    match edge.relation {
        RelationKind::Calls
        | RelationKind::Callee
        | RelationKind::Instantiates
        | RelationKind::Awaits
        | RelationKind::Spawns => {
            looks_like_call_expression_span(trimmed, &lower)
                && (token_match || lower.contains("new "))
        }
        RelationKind::Imports | RelationKind::Reexports => {
            looks_like_import_declaration_span(&lower)
        }
        RelationKind::Exports => lower.contains("export") || token_match,
        RelationKind::Reads
        | RelationKind::Writes
        | RelationKind::FlowsTo
        | RelationKind::Mutates
        | RelationKind::Injects
        | RelationKind::AliasOf
        | RelationKind::AliasedBy => token_match || contains_any(&lower, &["=", ".", "=>", ":"]),
        RelationKind::Authorizes => {
            looks_like_expression_site(trimmed, &lower)
                && (token_match
                    || contains_any(&lower, &["authoriz", "auth", "policy", "permission"]))
        }
        RelationKind::ChecksRole => {
            looks_like_expression_site(trimmed, &lower)
                && (token_match || contains_any(&lower, &["role", "admin", "user"]))
        }
        RelationKind::Sanitizes => {
            looks_like_call_expression_span(trimmed, &lower)
                && (token_match || contains_any(&lower, &["sanitiz", "escape", "trim", "clean"]))
        }
        RelationKind::Exposes => {
            token_match
                || contains_any(
                    &lower,
                    &[
                        "router.", "app.", ".get(", ".post(", ".put(", ".delete(", "route",
                    ],
                )
        }
        RelationKind::Publishes | RelationKind::Emits => {
            token_match || contains_any(&lower, &["publish", "emit", "dispatch", "send"])
        }
        RelationKind::Consumes | RelationKind::ListensTo => {
            token_match || contains_any(&lower, &["consume", "listen", "subscribe", ".on("])
        }
        RelationKind::Tests => token_match || contains_any(&lower, &["test(", "it(", "describe("]),
        RelationKind::Mocks => {
            token_match || contains_any(&lower, &["mock", "vi.", "jest.", "spyon"])
        }
        RelationKind::Stubs => token_match || lower.contains("stub"),
        RelationKind::Asserts => {
            looks_like_assertion_expression_span(trimmed, &lower)
                && (token_match || contains_any(&lower, &["expect(", "assert", "should", ".tobe"]))
        }
        _ => true,
    }
}

fn looks_like_call_expression_span(trimmed: &str, lower: &str) -> bool {
    lower.contains('(') && looks_like_expression_site(trimmed, lower)
}

fn looks_like_assertion_expression_span(trimmed: &str, lower: &str) -> bool {
    looks_like_expression_site(trimmed, lower)
        && contains_any(lower, &["expect(", "assert", "should", ".tobe"])
}

fn looks_like_expression_site(trimmed: &str, lower: &str) -> bool {
    if trimmed.is_empty() || contains_code_semicolon(trimmed) {
        return false;
    }
    let lower = lower.trim_start();
    !starts_with_any(
        lower,
        &[
            "return ",
            "if ",
            "if(",
            "while ",
            "while(",
            "for ",
            "for(",
            "switch ",
            "switch(",
            "const ",
            "let ",
            "var ",
            "function ",
            "export function ",
            "class ",
            "try ",
            "catch ",
        ],
    )
}

fn looks_like_import_declaration_span(lower: &str) -> bool {
    let lower = lower.trim_start();
    lower.starts_with("import ")
        || lower.starts_with("import{")
        || (lower.starts_with("export ") && lower.contains(" from "))
        || lower.starts_with("use ")
        || lower.contains("require(")
}

fn starts_with_any(value: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| value.starts_with(prefix))
}

fn contains_code_semicolon(value: &str) -> bool {
    let mut quote = None;
    let mut escaped = false;
    for ch in value.chars() {
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
        if ch == ';' {
            return true;
        }
    }
    false
}

fn edge_identity_tokens(edge: &Edge) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    for value in [&edge.head_id, &edge.tail_id] {
        for raw in value.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')) {
            let token = raw.trim();
            if token.len() < 3
                || token.eq_ignore_ascii_case("repo")
                || token.eq_ignore_ascii_case("edge")
                || token.chars().all(|ch| ch.is_ascii_hexdigit())
            {
                continue;
            }
            tokens.insert(token.to_string());
        }
    }
    tokens.into_iter().collect()
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn source_span_issues_json(issues: &[SourceSpanProofIssue]) -> serde_json::Value {
    serde_json::json!(issues
        .iter()
        .map(|issue| {
            serde_json::json!({
                "edge_id": issue.edge_id,
                "relation": issue.relation.to_string(),
                "span": issue.span.to_string(),
                "kind": issue.kind.as_str(),
                "message": issue.message,
            })
        })
        .collect::<Vec<_>>())
}

fn edge_class_issues_json(issues: &[EdgeClassProofIssue]) -> serde_json::Value {
    serde_json::json!(issues
        .iter()
        .map(|issue| {
            serde_json::json!({
                "edge_id": issue.edge_id,
                "relation": issue.relation.to_string(),
                "fact_class": issue.fact_class.as_str(),
                "kind": issue.kind.as_str(),
                "message": issue.message,
            })
        })
        .collect::<Vec<_>>())
}

fn path_allowed_for_context_mode(path: &GraphPath, mode: &str) -> bool {
    context_mode_allows_test_mock_edges(mode) || path.path_context().is_production()
}

fn context_mode_allows_test_mock_edges(mode: &str) -> bool {
    let normalized = mode
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect::<String>();
    normalized.contains("test")
        || normalized.contains("spec")
        || normalized.contains("mock")
        || normalized.contains("fixture")
}

fn context_mode_policy_label(mode: &str) -> &'static str {
    if context_mode_allows_test_mock_edges(mode) {
        "test_mock_allowed"
    } else {
        "production_only"
    }
}

fn path_context_counts(paths: &[GraphPath]) -> serde_json::Value {
    let mut production = 0usize;
    let mut test = 0usize;
    let mut mock = 0usize;
    let mut mixed = 0usize;
    for path in paths {
        match path.path_context() {
            PathContext::Production => production += 1,
            PathContext::Test => test += 1,
            PathContext::Mock => mock += 1,
            PathContext::Mixed => mixed += 1,
        }
    }
    serde_json::json!({
        "production": production,
        "test": test,
        "mock": mock,
        "mixed": mixed,
    })
}

fn fact_class_is_proof_eligible(edge: &Edge, fact_class: EdgeFactClass) -> bool {
    match fact_class {
        EdgeFactClass::BaseExact | EdgeFactClass::ReifiedCallsite => true,
        EdgeFactClass::Derived => edge.derived && !edge.provenance_edges.is_empty(),
        EdgeFactClass::BaseHeuristic
        | EdgeFactClass::Test
        | EdgeFactClass::Mock
        | EdgeFactClass::Mixed
        | EdgeFactClass::Unknown => false,
    }
}

pub fn edge_exactness_is_proof_grade(exactness: Exactness) -> bool {
    matches!(
        exactness,
        Exactness::Exact
            | Exactness::CompilerVerified
            | Exactness::LspVerified
            | Exactness::ParserVerified
    )
}

fn edge_has_unresolved_resolution(edge: &Edge) -> bool {
    edge.metadata
        .get("resolution")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.to_ascii_lowercase().contains("unresolved"))
        || edge
            .metadata
            .get("resolved")
            .and_then(serde_json::Value::as_bool)
            .is_some_and(|resolved| !resolved)
}

fn is_derived_cache_relation(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::MayMutate
            | RelationKind::MayRead
            | RelationKind::ApiReaches
            | RelationKind::AsyncReaches
            | RelationKind::SchemaImpact
    )
}

fn is_inverse_relation(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::CalledBy
            | RelationKind::MutatedBy
            | RelationKind::DefinedIn
            | RelationKind::AliasedBy
    )
}

fn extract_snippet(span: &SourceSpan, sources: &BTreeMap<String, String>) -> String {
    let Some(source) = sources.get(&span.repo_relative_path) else {
        return String::new();
    };
    let start = span.start_line.saturating_sub(1) as usize;
    let end = span.end_line.max(span.start_line) as usize;
    source
        .lines()
        .skip(start)
        .take(end.saturating_sub(start).min(4))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn risk_summaries(paths: &[GraphPath]) -> Vec<String> {
    let mut risks = BTreeSet::new();
    for path in paths {
        if path.contains_relation(RelationKind::Writes)
            || path.contains_relation(RelationKind::Mutates)
        {
            risks.insert(format!(
                "Changing {} may mutate {}",
                path.source, path.target
            ));
        }
        if path.contains_relation(RelationKind::Authorizes)
            || path.contains_relation(RelationKind::ChecksRole)
            || path.contains_relation(RelationKind::ChecksPermission)
        {
            risks.insert(format!(
                "{} participates in an auth/security path",
                path.source
            ));
        }
        if path.contains_relation(RelationKind::Publishes)
            || path.contains_relation(RelationKind::Emits)
            || path.contains_relation(RelationKind::Consumes)
            || path.contains_relation(RelationKind::ListensTo)
        {
            risks.insert(format!(
                "{} participates in an async/event flow",
                path.source
            ));
        }
        if path.contains_relation(RelationKind::Migrates)
            || path.contains_relation(RelationKind::AltersColumn)
            || path.contains_relation(RelationKind::DependsOnSchema)
        {
            risks.insert(format!("{} has schema migration impact", path.source));
        }
        if path.contains_relation(RelationKind::Tests)
            || path.contains_relation(RelationKind::Covers)
            || path.contains_relation(RelationKind::Asserts)
        {
            risks.insert(format!("{} has test coverage paths to review", path.source));
        }
    }
    risks.into_iter().take(12).collect()
}

fn recommended_tests_for_paths(paths: &[GraphPath]) -> Vec<String> {
    let mut tests = BTreeSet::new();
    for path in paths {
        if path.steps.iter().any(|step| {
            matches!(
                step.edge.relation,
                RelationKind::Tests
                    | RelationKind::Covers
                    | RelationKind::Asserts
                    | RelationKind::Mocks
                    | RelationKind::Stubs
                    | RelationKind::FixturesFor
            )
        }) {
            tests.insert(format!("run tests covering {}", path.target));
        }
    }
    tests.into_iter().take(12).collect()
}

fn compact_packet(packet: &mut ContextPacket, token_budget: usize) {
    while estimate_packet_tokens(packet) > token_budget {
        if packet.snippets.len() > 1 {
            packet.snippets.pop();
            continue;
        }
        if packet.verified_paths.len() > 1 {
            packet.verified_paths.pop();
            continue;
        }
        if packet.risks.len() > 1 {
            packet.risks.pop();
            continue;
        }
        if packet.recommended_tests.len() > 1 {
            packet.recommended_tests.pop();
            continue;
        }
        break;
    }
    while estimate_packet_tokens(packet) > token_budget {
        if packet.snippets.pop().is_some() {
            continue;
        }
        if packet.verified_paths.pop().is_some() {
            continue;
        }
        if packet.risks.pop().is_some() {
            continue;
        }
        if packet.recommended_tests.pop().is_some() {
            continue;
        }
        break;
    }
    packet.metadata.insert(
        "estimated_tokens".to_string(),
        serde_json::json!(estimate_packet_tokens(packet)),
    );
}

fn estimate_packet_tokens(packet: &ContextPacket) -> usize {
    let serialized = match serde_json::to_string(packet) {
        Ok(value) => value,
        Err(_) => format!(
            "{}{}{}{}{}",
            packet.task,
            packet.mode,
            packet.symbols.join(""),
            packet.risks.join(""),
            packet.recommended_tests.join("")
        ),
    };
    (serialized.len() / 4).max(1)
}

#[cfg(test)]
mod tests {
    use codegraph_core::{stable_edge_id, Edge, EdgeClass, EdgeContext};

    use super::*;

    fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("expected Ok(..), got Err({error:?})"),
        }
    }

    fn span(line: u32) -> SourceSpan {
        SourceSpan::new("fixtures/query.ts", line, line)
    }

    fn edge(head: &str, relation: RelationKind, tail: &str, line: u32) -> Edge {
        let source_span = span(line);
        edge_with_span(head, relation, tail, source_span)
    }

    fn edge_with_span(
        head: &str,
        relation: RelationKind,
        tail: &str,
        source_span: SourceSpan,
    ) -> Edge {
        Edge {
            id: stable_edge_id(head, relation, tail, &source_span),
            head_id: head.to_string(),
            relation,
            tail_id: tail.to_string(),
            source_span,
            repo_commit: None,
            file_hash: Some("hash".to_string()),
            extractor: "test-fixture".to_string(),
            confidence: 1.0,
            exactness: Exactness::ParserVerified,
            edge_class: EdgeClass::BaseExact,
            context: EdgeContext::Production,
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Default::default(),
        }
    }

    fn heuristic_edge(head: &str, relation: RelationKind, tail: &str, line: u32) -> Edge {
        let mut edge = edge(head, relation, tail, line);
        edge.exactness = Exactness::StaticHeuristic;
        edge.confidence = 0.65;
        edge.extractor = "test-heuristic".to_string();
        edge
    }

    fn entity(kind: EntityKind, id: &str, name: &str, qualified_name: &str, path: &str) -> Entity {
        Entity {
            id: id.to_string(),
            kind,
            name: name.to_string(),
            qualified_name: qualified_name.to_string(),
            repo_relative_path: path.to_string(),
            source_span: Some(SourceSpan::new(path, 1, 1)),
            content_hash: None,
            file_hash: Some("hash".to_string()),
            created_from: "test".to_string(),
            confidence: 1.0,
            metadata: Default::default(),
        }
    }

    fn file(path: &str) -> FileRecord {
        FileRecord {
            repo_relative_path: path.to_string(),
            file_hash: "hash".to_string(),
            language: Some("typescript".to_string()),
            size_bytes: 12,
            indexed_at_unix_ms: Some(1),
            metadata: Default::default(),
        }
    }

    fn limits() -> QueryLimits {
        QueryLimits {
            max_depth: 6,
            max_paths: 16,
            max_edges_visited: 256,
        }
    }

    fn assert_provenance(path: &GraphPath) {
        assert!(!path.steps.is_empty());
        assert_eq!(path.edge_ids().len(), path.steps.len());
        assert_eq!(path.source_spans().len(), path.steps.len());
        assert!(path
            .source_spans()
            .iter()
            .all(|span| span.repo_relative_path == "fixtures/query.ts"));
    }

    fn proof_path(edge: Edge) -> GraphPath {
        GraphPath {
            source: edge.head_id.clone(),
            target: edge.tail_id.clone(),
            steps: vec![TraversalStep {
                from: edge.head_id.clone(),
                to: edge.tail_id.clone(),
                edge,
                direction: TraversalDirection::Forward,
            }],
            cost: 1.0,
            uncertainty: 0.0,
        }
    }

    fn single_source(path: &str, source: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(path.to_string(), source.to_string())])
    }

    #[test]
    fn proof_span_validation_accepts_exact_callsite_span() {
        let sources = single_source(
            "fixtures/proof.ts",
            "export function login(user) {\n  checkRole(user);\n}\n",
        );
        let edge = edge_with_span(
            "login",
            RelationKind::Calls,
            "checkRole",
            SourceSpan::with_columns("fixtures/proof.ts", 2, 3, 2, 18),
        );

        assert!(validate_proof_path_source_spans(&proof_path(edge), &sources).is_ok());
    }

    #[test]
    fn proof_span_validation_accepts_exact_import_span() {
        let source = "import { checkRole } from \"./auth\";\nexport const ok = true;\n";
        let sources = single_source("fixtures/imports.ts", source);
        let edge = edge_with_span(
            "module",
            RelationKind::Imports,
            "checkRole",
            SourceSpan::with_columns("fixtures/imports.ts", 1, 1, 1, 36),
        );

        assert!(validate_proof_path_source_spans(&proof_path(edge), &sources).is_ok());
    }

    #[test]
    fn proof_span_validation_accepts_exact_role_check_span() {
        let source = "if (checkRole(user, \"admin\")) { return true; }\n";
        let sources = single_source("fixtures/auth.ts", source);
        let edge = edge_with_span(
            "guard",
            RelationKind::ChecksRole,
            "admin",
            SourceSpan::with_columns("fixtures/auth.ts", 1, 5, 1, 29),
        );

        assert!(validate_proof_path_source_spans(&proof_path(edge), &sources).is_ok());
    }

    #[test]
    fn proof_span_validation_accepts_exact_assertion_span() {
        let source = "it('checks auth', () => {\n  expect(result).toBe(true);\n});\n";
        let sources = single_source("fixtures/auth.test.ts", source);
        let edge = edge_with_span(
            "auth test",
            RelationKind::Asserts,
            "result",
            SourceSpan::with_columns("fixtures/auth.test.ts", 2, 3, 2, 28),
        );

        assert!(validate_proof_path_source_spans(&proof_path(edge), &sources).is_ok());
    }

    #[test]
    fn proof_span_validation_rejects_line_only_span_for_proof_edge() {
        let sources = single_source("fixtures/proof.ts", "checkRole(user);\n");
        let edge = edge_with_span(
            "login",
            RelationKind::Calls,
            "checkRole",
            SourceSpan::new("fixtures/proof.ts", 1, 1),
        );
        let issues = validate_proof_path_source_spans(&proof_path(edge), &sources)
            .expect_err("line-only proof spans should fail");

        assert_eq!(issues[0].kind, SourceSpanProofIssueKind::MissingSpan);
    }

    #[test]
    fn proof_span_validation_rejects_broad_span_covering_two_calls() {
        let source = "export function run() {\n  first(); second();\n}\n";
        let sources = single_source("fixtures/proof.ts", source);
        let edge = edge_with_span(
            "run",
            RelationKind::Calls,
            "second",
            SourceSpan::with_columns("fixtures/proof.ts", 2, 3, 2, 21),
        );
        let issues = validate_proof_path_source_spans(&proof_path(edge), &sources)
            .expect_err("broad callsite span should fail");

        assert_eq!(
            issues[0].kind,
            SourceSpanProofIssueKind::WrongSyntaxLocation
        );
    }

    #[test]
    fn proof_span_validation_rejects_broad_span_covering_two_assertions() {
        let source =
            "it('checks', () => {\n  expect(first()).toBe(1); expect(second()).toBe(2);\n});\n";
        let sources = single_source("fixtures/assertions.test.ts", source);
        let edge = edge_with_span(
            "checks",
            RelationKind::Asserts,
            "second",
            SourceSpan::with_columns("fixtures/assertions.test.ts", 2, 3, 2, 53),
        );
        let issues = validate_proof_path_source_spans(&proof_path(edge), &sources)
            .expect_err("broad assertion span should fail");

        assert_eq!(
            issues[0].kind,
            SourceSpanProofIssueKind::WrongSyntaxLocation
        );
    }

    #[test]
    fn proof_span_validation_rejects_missing_span() {
        let edge = edge_with_span(
            "login",
            RelationKind::Calls,
            "checkRole",
            SourceSpan::with_columns("", 0, 0, 0, 0),
        );
        let issues = validate_proof_path_source_spans(&proof_path(edge), &BTreeMap::new())
            .expect_err("missing span should fail");

        assert_eq!(issues[0].kind, SourceSpanProofIssueKind::MissingSpan);
    }

    #[test]
    fn proof_span_validation_rejects_wrong_file_span() {
        let sources = BTreeMap::from([
            (
                "fixtures/proof.ts".to_string(),
                "checkRole(user);\n".to_string(),
            ),
            (
                "fixtures/other.ts".to_string(),
                "const unrelated = 1;\n".to_string(),
            ),
        ]);
        let edge = edge_with_span(
            "login",
            RelationKind::Calls,
            "checkRole",
            SourceSpan::with_columns("fixtures/other.ts", 1, 1, 1, 21),
        );
        let issues = validate_proof_path_source_spans(&proof_path(edge), &sources)
            .expect_err("wrong file should fail relation syntax validation");

        assert_eq!(
            issues[0].kind,
            SourceSpanProofIssueKind::WrongSyntaxLocation
        );
    }

    #[test]
    fn proof_span_validation_rejects_out_of_range_span() {
        let sources = single_source("fixtures/proof.ts", "checkRole(user);\n");
        let edge = edge_with_span(
            "login",
            RelationKind::Calls,
            "checkRole",
            SourceSpan::with_columns("fixtures/proof.ts", 99, 1, 99, 16),
        );
        let issues = validate_proof_path_source_spans(&proof_path(edge), &sources)
            .expect_err("out-of-range span should fail");

        assert_eq!(issues[0].kind, SourceSpanProofIssueKind::OutOfRange);
    }

    #[test]
    fn proof_span_validation_rejects_multiline_end_column_out_of_range() {
        let sources = single_source("fixtures/proof.ts", "checkRole(\n  user\n);\n");
        let edge = edge_with_span(
            "login",
            RelationKind::Calls,
            "checkRole",
            SourceSpan::with_columns("fixtures/proof.ts", 1, 1, 2, 99),
        );
        let issues = validate_proof_path_source_spans(&proof_path(edge), &sources)
            .expect_err("out-of-range multiline columns should fail");

        assert_eq!(issues[0].kind, SourceSpanProofIssueKind::OutOfRange);
    }

    #[test]
    fn context_packet_labels_invalid_span_paths_non_proof() {
        let engine = ExactGraphQueryEngine::new(vec![edge_with_span(
            "login",
            RelationKind::Calls,
            "checkRole",
            SourceSpan::new("fixtures/missing.ts", 1, 1),
        )]);
        let packet = engine.context_pack(
            ContextPackRequest::new("Review login", "impact", 1_000, vec!["login".to_string()]),
            &BTreeMap::new(),
        );
        let path = packet
            .verified_paths
            .iter()
            .find(|path| path.metapath.contains(&RelationKind::Calls))
            .expect("path evidence");

        assert_eq!(path.exactness, Exactness::Inferred);
        assert_eq!(
            path.metadata
                .get("proof_grade_source_spans")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            path.metadata
                .get("source_span_validation")
                .and_then(|value| value.as_str()),
            Some("failed")
        );
    }

    #[test]
    fn exact_edge_with_span_can_pass_proof_edge_class_validation() {
        let source = "export function login() {\n  checkRole(user);\n}\n";
        let edge = edge_with_span(
            "login",
            RelationKind::Calls,
            "checkRole",
            SourceSpan::with_columns("fixtures/proof.ts", 2, 3, 2, 18),
        );
        let path = proof_path(edge.clone());
        let engine = ExactGraphQueryEngine::new(vec![edge]);
        let evidence = engine
            .path_evidence_from_paths_with_source_validation(
                &[path],
                &single_source("fixtures/proof.ts", source),
            )
            .pop()
            .expect("path evidence");

        assert!(validate_proof_path_edge_classes(&proof_path(edge_with_span(
            "login",
            RelationKind::Calls,
            "checkRole",
            SourceSpan::with_columns("fixtures/proof.ts", 2, 3, 2, 18),
        )))
        .is_ok());
        assert_eq!(
            evidence
                .metadata
                .get("proof_grade_edge_classes")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            evidence
                .metadata
                .get("production_proof_eligible")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn heuristic_edge_cannot_be_proof_grade_by_default() {
        let edge = heuristic_edge("route", RelationKind::Calls, "maybeGuard", 1);
        let issues = validate_proof_path_edge_classes(&proof_path(edge.clone()))
            .expect_err("heuristic edge should not be proof-grade");
        let engine = ExactGraphQueryEngine::new(vec![edge.clone()]);
        let evidence = engine.path_evidence(&proof_path(edge.clone()));

        assert_eq!(classify_edge_fact(&edge), EdgeFactClass::BaseHeuristic);
        assert!(issues
            .iter()
            .any(|issue| issue.kind == EdgeClassProofIssueKind::HeuristicEdge));
        assert_eq!(
            evidence
                .metadata
                .get("proof_grade_edge_classes")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            evidence
                .metadata
                .get("production_proof_eligible")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn derived_edge_without_provenance_fails_proof_validation() {
        let mut edge = edge("updateUser", RelationKind::MayMutate, "users", 1);
        edge.derived = true;
        edge.exactness = Exactness::DerivedFromVerifiedEdges;
        let path = proof_path(edge.clone());
        let issues = validate_proof_path_edge_classes(&path)
            .expect_err("derived edge without provenance should fail");
        let engine = ExactGraphQueryEngine::new(vec![edge]);
        let evidence = engine
            .path_evidence_from_paths_with_source_validation(&[path], &BTreeMap::new())
            .pop()
            .expect("path evidence");

        assert!(issues
            .iter()
            .any(|issue| issue.kind == EdgeClassProofIssueKind::DerivedWithoutProvenance));
        assert_eq!(
            evidence
                .metadata
                .get("edge_class_validation")
                .and_then(|value| value.as_str()),
            Some("failed")
        );
        assert_eq!(
            evidence
                .metadata
                .get("derived_edges_have_provenance")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(evidence.exactness, Exactness::Inferred);
    }

    #[test]
    fn derived_edge_with_provenance_can_be_explained() {
        let mut edge = edge("updateUser", RelationKind::MayMutate, "users", 1);
        edge.derived = true;
        edge.exactness = Exactness::DerivedFromVerifiedEdges;
        edge.provenance_edges = vec!["edge://base-write".to_string()];
        let path = proof_path(edge.clone());
        let engine = ExactGraphQueryEngine::new(vec![edge]);
        let evidence = engine.path_evidence(&path);

        assert!(validate_proof_path_edge_classes(&path).is_ok());
        assert_eq!(
            classify_edge_fact(&path.steps[0].edge),
            EdgeFactClass::Derived
        );
        assert_eq!(
            evidence
                .metadata
                .get("derived_edges_have_provenance")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert!(evidence
            .metadata
            .get("edge_labels")
            .and_then(|value| value.as_array())
            .is_some_and(|labels| labels.iter().any(|label| {
                label.get("edge_class").and_then(|value| value.as_str()) == Some("derived")
                    && label
                        .get("provenance_edges")
                        .and_then(|value| value.as_array())
                        .is_some_and(|ids| !ids.is_empty())
            })));
    }

    #[test]
    fn test_mock_edge_is_not_production_proof_grade() {
        let edge = edge_with_span(
            "tests/auth.test",
            RelationKind::Mocks,
            "src/auth.login",
            SourceSpan::with_columns("tests/auth.test.ts", 3, 1, 3, 45),
        );
        let path = proof_path(edge.clone());
        let issues = validate_proof_path_edge_classes(&path)
            .expect_err("test/mock edge should not be production proof-grade");

        assert_eq!(classify_edge_fact(&edge), EdgeFactClass::Mock);
        assert!(issues
            .iter()
            .any(|issue| issue.kind == EdgeClassProofIssueKind::TestMockEdge));
    }

    #[test]
    fn inverse_edge_is_not_counted_as_base_exact() {
        let edge = edge("callee", RelationKind::CalledBy, "caller", 1);
        let issues = validate_proof_path_edge_classes(&proof_path(edge.clone()))
            .expect_err("inverse edge should not be raw base proof");

        assert_eq!(classify_edge_fact(&edge), EdgeFactClass::Unknown);
        assert!(issues
            .iter()
            .any(|issue| issue.kind == EdgeClassProofIssueKind::InverseEdge));
    }

    #[test]
    fn symbol_search_exact_match_beats_fuzzy_result() {
        let index = SymbolSearchIndex::new(
            vec![
                entity(
                    EntityKind::Function,
                    "loadUser",
                    "loadUser",
                    "src.users.loadUser",
                    "src/users.ts",
                ),
                entity(
                    EntityKind::Function,
                    "loadUserProfile",
                    "loadUserProfile",
                    "src.users.loadUserProfile",
                    "src/users.ts",
                ),
            ],
            Vec::new(),
            vec![file("src/users.ts")],
        );

        let hits = index.search("loadUser", 5);

        assert_eq!(hits[0].entity.name, "loadUser");
        assert!(hits[0].score > hits[1].score);
        assert_eq!(
            hits[0].features.get("exact_symbol_match").copied(),
            Some(1.0)
        );
    }

    #[test]
    fn symbol_search_qualified_name_beats_unrelated_same_name() {
        let index = SymbolSearchIndex::new(
            vec![
                entity(
                    EntityKind::Method,
                    "auth-run",
                    "run",
                    "auth.AuthService.run",
                    "src/auth.ts",
                ),
                entity(
                    EntityKind::Method,
                    "billing-run",
                    "run",
                    "billing.Job.run",
                    "src/billing.ts",
                ),
            ],
            Vec::new(),
            vec![file("src/auth.ts"), file("src/billing.ts")],
        );

        let hits = index.search("auth.AuthService.run", 5);

        assert_eq!(hits[0].entity.id, "auth-run");
        assert_eq!(
            hits[0].features.get("qualified_name_match").copied(),
            Some(1.0)
        );
    }

    #[test]
    fn symbol_search_uses_import_alias_and_metadata_text() {
        let mut aliased = entity(
            EntityKind::Import,
            "alias",
            "renamedName",
            "consumer.import:renamedName",
            "src/consumer.ts",
        );
        aliased
            .metadata
            .insert("canonical_symbol".to_string(), "canonicalName".into());
        aliased
            .metadata
            .insert("doc_comment".to_string(), "Loads user profile data".into());
        let index =
            SymbolSearchIndex::new(vec![aliased], Vec::new(), vec![file("src/consumer.ts")]);

        let alias_hits = index.search("canonicalName", 5);
        let doc_hits = index.search("profile data", 5);

        assert_eq!(alias_hits[0].entity.name, "renamedName");
        assert_eq!(doc_hits[0].entity.name, "renamedName");
        assert!(alias_hits[0].features.contains_key("metadata_match"));
    }

    #[test]
    fn symbol_search_tokenizes_camel_snake_and_kebab_names() {
        let index = SymbolSearchIndex::new(
            vec![
                entity(
                    EntityKind::Function,
                    "camel",
                    "userProfileLoader",
                    "src.userProfileLoader",
                    "src/user-profile.ts",
                ),
                entity(
                    EntityKind::Function,
                    "snake",
                    "user_profile_loader",
                    "src.user_profile_loader",
                    "src/user_profile.ts",
                ),
            ],
            Vec::new(),
            vec![file("src/user-profile.ts"), file("src/user_profile.ts")],
        );

        let hits = index.search("user profile", 5);
        let names = hits
            .iter()
            .map(|hit| hit.entity.name.as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&"userProfileLoader"));
        assert!(names.contains(&"user_profile_loader"));
        assert!(hits
            .iter()
            .all(|hit| hit.features.contains_key("token_match")));
    }

    #[test]
    fn symbol_search_indexes_relation_neighbor_text() {
        let route = entity(
            EntityKind::Function,
            "route",
            "adminRoute",
            "routes.adminRoute",
            "src/routes.ts",
        );
        let guard = entity(
            EntityKind::Function,
            "guard",
            "checkRole",
            "auth.checkRole",
            "src/auth.ts",
        );
        let index = SymbolSearchIndex::new(
            vec![route, guard],
            vec![edge("route", RelationKind::Calls, "guard", 1)],
            vec![file("src/routes.ts"), file("src/auth.ts")],
        );

        let hits = index.search("checkRole", 5);
        let route_hit = hits
            .iter()
            .find(|hit| hit.entity.name == "adminRoute")
            .expect("route hit via neighbor text");

        assert!(route_hit.features.contains_key("relation_neighbor_text"));
    }

    #[test]
    fn prompt_seed_extraction_finds_file_paths_and_line_numbers() {
        let seeds = extract_prompt_seeds("Fix src/auth/login.ts:82 for failing auth flow");

        assert!(seeds.iter().any(|seed| {
            seed.kind == PromptSeedKind::FilePath && seed.value == "src/auth/login.ts"
        }));
        assert!(seeds.iter().any(|seed| {
            seed.kind == PromptSeedKind::LineNumber
                && seed.file_path.as_deref() == Some("src/auth/login.ts")
                && seed.line == Some(82)
        }));
    }

    #[test]
    fn prompt_seed_extraction_finds_stack_trace_file_line_and_function() {
        let seeds = extract_prompt_seeds(
            "TypeError: bad token\n    at AuthService.login (src/auth.ts:82:13)",
        );

        assert!(seeds.iter().any(|seed| {
            seed.kind == PromptSeedKind::StackTrace
                && seed.file_path.as_deref() == Some("src/auth.ts")
                && seed.line == Some(82)
                && seed.function.as_deref() == Some("AuthService.login")
        }));
        assert!(seeds
            .iter()
            .any(|seed| seed.kind == PromptSeedKind::ErrorMessage));
    }

    #[test]
    fn prompt_seed_extraction_finds_symbols_tests_errors_and_identifiers() {
        let seeds = extract_prompt_seeds(
            "Change AuthService.login after test(\"normalizes email\") failed in normalizeEmail",
        );

        assert!(seeds
            .iter()
            .any(|seed| seed.kind == PromptSeedKind::Symbol && seed.value == "AuthService.login"));
        assert!(seeds.iter().any(|seed| {
            seed.kind == PromptSeedKind::TestName && seed.value == "normalizes email"
        }));
        assert!(seeds
            .iter()
            .any(|seed| seed.kind == PromptSeedKind::Identifier && seed.value == "normalizeEmail"));
        assert!(seeds
            .iter()
            .any(|seed| seed.kind == PromptSeedKind::ErrorMessage));
    }

    #[test]
    fn callers_and_callees_use_exact_call_edges() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge("controller", RelationKind::Calls, "service", 1),
            edge("service", RelationKind::Calls, "repo", 2),
        ]);

        let callers = engine.find_callers("service", limits());
        let callees = engine.find_callees("service", limits());

        assert_eq!(callers[0].target, "controller");
        assert_eq!(callers[0].steps[0].direction, TraversalDirection::Reverse);
        assert_eq!(callees[0].target, "repo");
        assert_eq!(callees[0].steps[0].direction, TraversalDirection::Forward);
        assert_provenance(&callers[0]);
        assert_provenance(&callees[0]);
    }

    #[test]
    fn mutation_path_follows_calls_to_writes_and_mutates() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge("api", RelationKind::Calls, "service", 1),
            edge("service", RelationKind::Calls, "repo", 2),
            edge("repo", RelationKind::Writes, "users.email", 3),
            edge("repo", RelationKind::Mutates, "cache", 4),
        ]);

        let paths = engine.find_mutations("api", limits());
        let relations = paths.iter().map(GraphPath::relations).collect::<Vec<_>>();

        assert!(relations.iter().any(|path| {
            path == &vec![
                RelationKind::Calls,
                RelationKind::Calls,
                RelationKind::Writes,
            ]
        }));
        assert!(relations.iter().any(|path| {
            path == &vec![
                RelationKind::Calls,
                RelationKind::Calls,
                RelationKind::Mutates,
            ]
        }));
        assert_provenance(&paths[0]);
    }

    #[test]
    fn auth_path_follows_exposes_calls_and_role_checks() {
        let engine = ExactGraphQueryEngine::new(vec![
            heuristic_edge("route", RelationKind::Exposes, "endpoint", 1),
            edge("endpoint", RelationKind::Calls, "guard", 2),
            heuristic_edge("guard", RelationKind::Authorizes, "policy", 3),
            heuristic_edge("guard", RelationKind::ChecksRole, "admin", 4),
        ]);

        let paths = engine.find_auth_paths("route", limits());

        assert!(paths.iter().any(|path| {
            path.contains_relation(RelationKind::Exposes)
                && path.contains_relation(RelationKind::Authorizes)
        }));
        assert!(paths.iter().any(|path| {
            path.contains_relation(RelationKind::Exposes)
                && path.contains_relation(RelationKind::ChecksRole)
        }));
        assert!(paths.iter().all(|path| path.uncertainty > 0.0));
        assert_provenance(&paths[0]);
    }

    #[test]
    fn event_flow_connects_publishers_to_consumers_and_handlers() {
        let engine = ExactGraphQueryEngine::new(vec![
            heuristic_edge("publisher", RelationKind::Emits, "UserCreated", 1),
            heuristic_edge("listener", RelationKind::ListensTo, "UserCreated", 2),
            edge("listener", RelationKind::Calls, "handler", 3),
            heuristic_edge("producer", RelationKind::Publishes, "users.created", 4),
            heuristic_edge("consumer", RelationKind::Consumes, "users.created", 5),
        ]);

        let emitted = engine.find_event_flow("publisher", limits());
        let published = engine.find_event_flow("producer", limits());

        assert!(emitted.iter().any(|path| path.target == "handler"));
        assert!(emitted.iter().any(|path| {
            path.steps
                .iter()
                .any(|step| step.direction == TraversalDirection::Reverse)
        }));
        assert!(published.iter().any(|path| path.target == "consumer"));
        assert_provenance(&emitted[0]);
    }

    #[test]
    fn migration_impact_finds_schema_edges() {
        let engine = ExactGraphQueryEngine::new(vec![
            heuristic_edge("migration001", RelationKind::Migrates, "users", 1),
            heuristic_edge("migration001", RelationKind::AltersColumn, "users.email", 2),
            heuristic_edge("migration001", RelationKind::DependsOnSchema, "users", 3),
        ]);

        let paths = engine.find_migrations("users.email", limits());

        assert!(paths.iter().any(|path| path.target == "migration001"));
        assert!(paths
            .iter()
            .any(|path| path.contains_relation(RelationKind::AltersColumn)));
        assert_provenance(&paths[0]);
    }

    #[test]
    fn test_impact_finds_related_tests() {
        let engine = ExactGraphQueryEngine::new(vec![
            heuristic_edge("auth.spec returns token", RelationKind::Tests, "login", 1),
            heuristic_edge(
                "auth.spec returns token",
                RelationKind::Asserts,
                "TokenPayload.sub",
                2,
            ),
            heuristic_edge("auth.spec returns token", RelationKind::Mocks, "client", 3),
        ]);

        let paths = engine.find_tests("login", limits());

        assert_eq!(paths[0].target, "auth.spec returns token");
        assert!(paths[0].contains_relation(RelationKind::Tests));
        assert_provenance(&paths[0]);
    }

    #[test]
    fn trace_path_uses_dijkstra_costs_and_uncertainty_penalties() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge("a", RelationKind::Calls, "b", 1),
            edge("b", RelationKind::Calls, "d", 2),
            heuristic_edge("a", RelationKind::Calls, "c", 3),
            heuristic_edge("c", RelationKind::Calls, "d", 4),
        ]);

        let paths = engine.trace_path("a", "d", &[RelationKind::Calls], limits());
        let shortest = engine
            .dijkstra(
                "a",
                "d",
                &[Traversal::forward(RelationKind::Calls)],
                limits(),
            )
            .expect("path");

        assert_eq!(paths[0].edge_ids(), shortest.edge_ids());
        assert_eq!(paths[0].target, "d");
        assert!(paths[0].uncertainty < paths[1].uncertainty);
        assert_provenance(&paths[0]);
    }

    #[test]
    fn bounded_traversal_handles_cycles() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge("a", RelationKind::Calls, "b", 1),
            edge("b", RelationKind::Calls, "c", 2),
            edge("c", RelationKind::Calls, "a", 3),
            edge("c", RelationKind::Writes, "sink", 4),
        ]);
        let mut limited = limits();
        limited.max_depth = 5;
        limited.max_edges_visited = 12;

        let paths = engine.find_mutations("a", limited);

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].target, "sink");
        assert!(paths[0].steps.len() <= limited.max_depth);
        assert_provenance(&paths[0]);
    }

    #[test]
    fn impact_analysis_core_groups_exact_query_results() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge("api", RelationKind::Calls, "service", 1),
            edge("caller", RelationKind::Calls, "api", 2),
            edge("api", RelationKind::Reads, "config", 3),
            edge("api", RelationKind::Writes, "cache", 4),
            heuristic_edge("api.spec", RelationKind::Tests, "api", 5),
            heuristic_edge("migration001", RelationKind::Migrates, "cache", 6),
        ]);

        let impact = engine.impact_analysis_core("api", limits());

        assert_eq!(impact.source, "api");
        assert!(!impact.callers.is_empty());
        assert!(!impact.callees.is_empty());
        assert!(!impact.tests.is_empty());
        assert!(!impact.writes.is_empty());
    }

    #[test]
    fn path_evidence_serializes_and_deserializes() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge("api", RelationKind::Calls, "repo", 1),
            edge("repo", RelationKind::Writes, "users.email", 2),
        ]);
        let path = engine.find_mutations("api", limits()).remove(0);
        let evidence = engine.path_evidence(&path);

        let json = match serde_json::to_string(&evidence) {
            Ok(json) => json,
            Err(error) => panic!("expected PathEvidence serialize, got {error}"),
        };
        let decoded: PathEvidence = match serde_json::from_str(&json) {
            Ok(decoded) => decoded,
            Err(error) => panic!("expected PathEvidence deserialize, got {error}"),
        };

        assert_eq!(decoded.id, evidence.id);
        assert_eq!(decoded.summary, evidence.summary);
        assert_eq!(decoded.edges, evidence.edges);
        assert!((decoded.confidence - evidence.confidence).abs() < 0.000_000_001);
        assert_eq!(decoded.length, 2);
        assert_eq!(decoded.source_spans.len(), 2);
        assert!(decoded.metadata.contains_key("edge_labels"));
        assert_eq!(
            decoded
                .metadata
                .get("ordered_edge_ids")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            decoded
                .metadata
                .get("relation_sequence")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        assert!(decoded.metadata.contains_key("exactness_labels"));
        assert!(decoded.metadata.contains_key("confidence_labels"));
        assert!(decoded
            .metadata
            .contains_key("derived_provenance_expansion"));
        assert!(decoded.metadata.contains_key("production_test_mock_labels"));
    }

    #[test]
    fn derived_closure_edges_always_include_provenance() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge("api", RelationKind::Calls, "repo", 1),
            edge("repo", RelationKind::Writes, "users.email", 2),
            edge("api", RelationKind::Calls, "reader", 3),
            edge("reader", RelationKind::Reads, "config", 4),
            heuristic_edge("route", RelationKind::Exposes, "endpoint", 5),
            edge("endpoint", RelationKind::Calls, "handler", 6),
            heuristic_edge("publisher", RelationKind::Emits, "UserCreated", 7),
            heuristic_edge("listener", RelationKind::ListensTo, "UserCreated", 8),
            heuristic_edge("migration001", RelationKind::AltersColumn, "users.email", 9),
        ]);
        let mut paths = Vec::new();
        paths.extend(engine.find_mutations("api", limits()));
        paths.extend(engine.trace_path(
            "api",
            "config",
            &[RelationKind::Calls, RelationKind::Reads],
            limits(),
        ));
        paths.extend(engine.trace_path(
            "route",
            "handler",
            &[RelationKind::Exposes, RelationKind::Calls],
            limits(),
        ));
        paths.extend(engine.find_auth_paths("route", limits()));
        paths.extend(engine.find_event_flow("publisher", limits()));
        paths.extend(engine.find_migrations("users.email", limits()));

        let derived = engine.derive_closure_edges(&paths);
        let relations = derived
            .iter()
            .map(|edge| edge.relation)
            .collect::<BTreeSet<_>>();

        assert!(relations.contains(&RelationKind::MayMutate));
        assert!(relations.contains(&RelationKind::MayRead));
        assert!(relations.contains(&RelationKind::ApiReaches));
        assert!(relations.contains(&RelationKind::AsyncReaches));
        assert!(relations.contains(&RelationKind::SchemaImpact));
        assert!(derived.iter().all(|edge| !edge.provenance_edges.is_empty()));
    }

    #[test]
    fn graph_only_context_packet_contains_production_evidence_and_filters_tests() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge("api", RelationKind::Calls, "service", 1),
            edge("service", RelationKind::Calls, "repo", 2),
            edge("repo", RelationKind::Writes, "users.email", 3),
            heuristic_edge("route", RelationKind::Exposes, "api", 4),
            heuristic_edge("auth.spec", RelationKind::Tests, "api", 5),
            heuristic_edge("auth.spec", RelationKind::Asserts, "users.email", 6),
        ]);
        let mut sources = BTreeMap::new();
        sources.insert(
            "fixtures/query.ts".to_string(),
            [
                "api();",
                "service();",
                "repo.write(users.email);",
                "router.get('/users', api);",
                "it('covers api', () => api());",
                "expect(users.email).toBeDefined();",
            ]
            .join("\n"),
        );

        let packet = engine.context_pack(
            ContextPackRequest::new(
                "Change users.email without breaking auth",
                "impact",
                2_000,
                vec!["api".to_string(), "users.email".to_string()],
            ),
            &sources,
        );

        assert_eq!(packet.task, "Change users.email without breaking auth");
        assert!(!packet.verified_paths.is_empty());
        assert!(!packet.snippets.is_empty());
        assert!(!packet.risks.is_empty());
        assert!(packet.recommended_tests.is_empty());
        assert!(packet
            .verified_paths
            .iter()
            .all(|path| !path.source_spans.is_empty()));
        assert!(packet
            .metadata
            .get("derived_edges")
            .and_then(|value| value.as_array())
            .is_some_and(|edges| !edges.is_empty()));
        assert!(packet.verified_paths.iter().all(|path| {
            path.metadata
                .get("path_context")
                .and_then(serde_json::Value::as_str)
                == Some("production")
        }));
        assert_eq!(
            packet
                .metadata
                .get("path_context_policy")
                .and_then(serde_json::Value::as_str),
            Some("production_only")
        );
        assert!(
            packet
                .metadata
                .get("rejected_test_mock_path_count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default()
                > 0
        );
    }

    #[test]
    fn production_context_packet_filters_mock_calls_by_default() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge_with_span(
                "src/checkout.checkout",
                RelationKind::Calls,
                "src/service.sendEmail",
                SourceSpan::with_columns("src/checkout.ts", 4, 10, 4, 30),
            ),
            edge_with_span(
                "src/checkout.checkout",
                RelationKind::Calls,
                "tests/checkout.test#mocked_sendEmail",
                SourceSpan::with_columns("src/checkout.ts", 4, 10, 4, 30),
            ),
            edge_with_span(
                "tests/checkout.test",
                RelationKind::Mocks,
                "src/service.sendEmail",
                SourceSpan::with_columns("tests/checkout.test.ts", 4, 1, 4, 59),
            ),
        ]);
        let mut sources = BTreeMap::new();
        sources.insert(
            "src/checkout.ts".to_string(),
            [
                "import { sendEmail } from './service';",
                "",
                "export function checkout() {",
                "  return sendEmail(\"receipt\");",
                "}",
            ]
            .join("\n"),
        );
        sources.insert(
            "tests/checkout.test.ts".to_string(),
            [
                "import { checkout } from '../src/checkout';",
                "import { sendEmail } from '../src/service';",
                "",
                "vi.mock(\"../src/service\", () => ({ sendEmail: vi.fn() }));",
            ]
            .join("\n"),
        );

        let packet = engine.context_pack(
            ContextPackRequest::new(
                "Find checkout's production email call and related test double evidence.",
                "impact",
                2_000,
                vec!["src/checkout.checkout".to_string()],
            ),
            &sources,
        );

        assert!(packet
            .verified_paths
            .iter()
            .any(|path| path.target == "src/service.sendEmail"));
        assert!(packet.verified_paths.iter().all(|path| {
            path.target != "tests/checkout.test#mocked_sendEmail"
                && path
                    .metadata
                    .get("path_context")
                    .and_then(serde_json::Value::as_str)
                    == Some("production")
                && path
                    .metadata
                    .get("production_proof_eligible")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
        }));
        assert!(packet
            .metadata
            .get("rejected_test_mock_path_count")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|count| count >= 1));
    }

    #[test]
    fn test_impact_context_packet_intentionally_includes_test_mock_edges() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge_with_span(
                "tests/checkout.test",
                RelationKind::Tests,
                "src/service.sendEmail",
                SourceSpan::with_columns("tests/checkout.test.ts", 5, 1, 7, 3),
            ),
            edge_with_span(
                "tests/checkout.test",
                RelationKind::Asserts,
                "src/service.sendEmail",
                SourceSpan::with_columns("tests/checkout.test.ts", 7, 3, 7, 39),
            ),
            edge_with_span(
                "tests/checkout.test",
                RelationKind::Mocks,
                "src/service.sendEmail",
                SourceSpan::with_columns("tests/checkout.test.ts", 4, 1, 4, 59),
            ),
            edge_with_span(
                "tests/checkout.test#mock_factory",
                RelationKind::Stubs,
                "src/service.sendEmail",
                SourceSpan::with_columns("tests/checkout.test.ts", 4, 34, 4, 56),
            ),
        ]);
        let mut sources = BTreeMap::new();
        sources.insert(
            "tests/checkout.test.ts".to_string(),
            [
                "import { checkout } from '../src/checkout';",
                "import { sendEmail } from '../src/service';",
                "",
                "vi.mock(\"../src/service\", () => ({ sendEmail: vi.fn() }));",
                "test(\"checkout sends receipt\", () => {",
                "  checkout();",
                "  expect(sendEmail).toHaveBeenCalled();",
                "});",
            ]
            .join("\n"),
        );

        let packet = engine.context_pack(
            ContextPackRequest::new(
                "Find tests and mocks for sendEmail",
                "test-impact",
                2_000,
                vec!["src/service.sendEmail".to_string()],
            ),
            &sources,
        );

        let relations = packet
            .verified_paths
            .iter()
            .flat_map(|path| path.metapath.iter().copied())
            .collect::<BTreeSet<_>>();
        for relation in [
            RelationKind::Tests,
            RelationKind::Asserts,
            RelationKind::Mocks,
            RelationKind::Stubs,
        ] {
            assert!(relations.contains(&relation), "missing {relation}");
        }
        let contexts = packet
            .verified_paths
            .iter()
            .filter_map(|path| {
                path.metadata
                    .get("path_context")
                    .and_then(serde_json::Value::as_str)
            })
            .collect::<BTreeSet<_>>();
        assert!(contexts.contains("test"));
        assert!(contexts.contains("mock") || contexts.contains("mixed"));
        assert_eq!(
            packet
                .metadata
                .get("path_context_policy")
                .and_then(serde_json::Value::as_str),
            Some("test_mock_allowed")
        );
    }

    #[test]
    fn exact_stage0_seeds_appear_in_context_packet_without_vector_layer() {
        let engine = ExactGraphQueryEngine::new(vec![edge(
            "AuthService.login",
            RelationKind::Writes,
            "TokenPayload.sub",
            82,
        )]);
        let packet = engine.context_pack(
            ContextPackRequest::new(
                "Fix AuthService.login in src/auth.ts:82 after TypeError: bad token",
                "impact",
                1_000,
                Vec::new(),
            ),
            &BTreeMap::new(),
        );

        assert!(packet
            .symbols
            .iter()
            .any(|symbol| symbol == "AuthService.login"));
        assert!(packet.symbols.iter().any(|symbol| symbol == "src/auth.ts"));
        assert!(packet
            .symbols
            .iter()
            .any(|symbol| symbol == "src/auth.ts:82"));
        assert!(packet.metadata.contains_key("stage0_policy"));
        assert!(!packet.verified_paths.is_empty());
    }

    fn funnel_config(stage1_top_k: usize, stage2_top_n: usize) -> RetrievalFunnelConfig {
        RetrievalFunnelConfig {
            stage1_top_k,
            stage2_top_n,
            query_limits: limits(),
            ..RetrievalFunnelConfig::default()
        }
    }

    #[test]
    fn retrieval_funnel_returns_expected_context_packet() {
        let funnel = ok(RetrievalFunnel::new(
            vec![
                edge(
                    "AuthService.login",
                    RelationKind::Calls,
                    "normalizeEmail",
                    1,
                ),
                edge(
                    "normalizeEmail",
                    RelationKind::Writes,
                    "TokenPayload.sub",
                    2,
                ),
                heuristic_edge("auth.spec", RelationKind::Tests, "AuthService.login", 3),
                heuristic_edge("auth.spec", RelationKind::Asserts, "TokenPayload.sub", 4),
            ],
            vec![
                RetrievalDocument::new("AuthService.login", "auth login token normalization")
                    .stage0_score(0.9),
                RetrievalDocument::new("normalizeEmail", "email normalization helper")
                    .stage0_score(0.7),
                RetrievalDocument::new("billing", "invoice payment unrelated").stage0_score(0.2),
            ],
            funnel_config(3, 2),
        ));
        let sources = BTreeMap::from([(
            "fixtures/query.ts".to_string(),
            "login();\nnormalizeEmail();\nit('auth', () => login());\nexpect(token.sub);"
                .to_string(),
        )]);

        let result = ok(funnel.run(
            RetrievalFunnelRequest::new(
                "Change AuthService.login token normalization",
                "impact",
                2_000,
            )
            .sources(sources),
        ));

        assert!(result
            .packet
            .symbols
            .iter()
            .any(|symbol| symbol == "AuthService.login"));
        assert!(result
            .packet
            .verified_paths
            .iter()
            .any(|path| path.source == "AuthService.login" && path.target == "TokenPayload.sub"));
        assert!(!result.packet.snippets.is_empty());
        assert_eq!(
            result
                .trace
                .iter()
                .map(|stage| stage.stage.as_str())
                .collect::<Vec<_>>(),
            vec![
                "stage0_exact_seed_extraction",
                "stage1_binary_sieve",
                "stage2_compressed_rerank",
                "stage3_exact_graph_verification",
                "stage4_context_packet",
            ]
        );
    }

    #[test]
    fn exact_seed_cannot_be_dropped_by_stage_one_or_stage_two() {
        let funnel = ok(RetrievalFunnel::new(
            vec![edge(
                "Exact.seed",
                RelationKind::Calls,
                "verified-target",
                1,
            )],
            vec![
                RetrievalDocument::new("semantic-match", "auth login token").stage0_score(1.0),
                RetrievalDocument::new("Exact.seed", "unrelated migration").stage0_score(0.0),
            ],
            funnel_config(1, 1),
        ));

        let result = ok(funnel.run(
            RetrievalFunnelRequest::new("auth login token", "impact", 1_000)
                .exact_seeds(vec!["Exact.seed".to_string()]),
        ));
        let stage1 = result
            .trace
            .iter()
            .find(|stage| stage.stage == "stage1_binary_sieve")
            .expect("stage1 trace");
        let stage2 = result
            .trace
            .iter()
            .find(|stage| stage.stage == "stage2_compressed_rerank")
            .expect("stage2 trace");

        assert!(stage1.kept.contains(&"Exact.seed".to_string()));
        assert!(stage2.kept.contains(&"Exact.seed".to_string()));
    }

    #[test]
    fn binary_false_positive_is_removed_by_exact_graph_verification() {
        let funnel = ok(RetrievalFunnel::new(
            vec![edge(
                "AuthService.login",
                RelationKind::Calls,
                "normalizeEmail",
                1,
            )],
            vec![
                RetrievalDocument::new("AuthService.login", "auth login token").stage0_score(0.8),
                RetrievalDocument::new("binary-false-positive", "auth login token")
                    .stage0_score(0.9),
            ],
            funnel_config(4, 4),
        ));

        let result = ok(funnel.run(
            RetrievalFunnelRequest::new("Fix AuthService.login", "impact", 1_000)
                .exact_seeds(vec!["AuthService.login".to_string()]),
        ));
        let stage3 = result
            .trace
            .iter()
            .find(|stage| stage.stage == "stage3_exact_graph_verification")
            .expect("stage3 trace");

        assert!(stage3
            .dropped
            .contains(&"binary-false-positive".to_string()));
        assert!(!result
            .packet
            .symbols
            .contains(&"binary-false-positive".to_string()));
    }

    #[test]
    fn heuristic_relation_survives_only_with_explicit_label() {
        let funnel = ok(RetrievalFunnel::new(
            vec![
                heuristic_edge("route", RelationKind::Exposes, "endpoint", 1),
                edge("endpoint", RelationKind::Calls, "guard", 2),
                heuristic_edge("guard", RelationKind::ChecksRole, "admin", 3),
            ],
            vec![RetrievalDocument::new("route", "admin auth route").stage0_score(1.0)],
            funnel_config(4, 4),
        ));

        let result = ok(funnel.run(
            RetrievalFunnelRequest::new("Review route auth", "security", 4_000)
                .exact_seeds(vec!["route".to_string()]),
        ));

        assert!(result.packet.verified_paths.iter().any(|path| {
            matches!(
                path.exactness,
                Exactness::StaticHeuristic | Exactness::Inferred
            ) && path
                .metadata
                .get("edge_labels")
                .and_then(|value| value.as_array())
                .is_some_and(|labels| {
                    labels.iter().any(|label| {
                        label.get("exactness").and_then(|value| value.as_str())
                            == Some("static_heuristic")
                    })
                })
        }));
    }

    #[test]
    fn funnel_trace_metadata_includes_all_stages() {
        let funnel = ok(RetrievalFunnel::new(
            vec![edge("a", RelationKind::Calls, "b", 1)],
            vec![RetrievalDocument::new("a", "call b")],
            funnel_config(2, 2),
        ));

        let result = ok(funnel.run(
            RetrievalFunnelRequest::new("Change a", "impact", 1_000)
                .exact_seeds(vec!["a".to_string()]),
        ));
        let trace = result
            .packet
            .metadata
            .get("trace")
            .and_then(|value| value.as_array())
            .expect("trace metadata");

        for stage in [
            "stage0_exact_seed_extraction",
            "stage1_binary_sieve",
            "stage2_compressed_rerank",
            "stage3_exact_graph_verification",
            "stage4_context_packet",
        ] {
            assert!(
                trace.iter().any(|entry| {
                    entry.get("stage").and_then(|value| value.as_str()) == Some(stage)
                }),
                "missing trace stage {stage}"
            );
        }
    }

    #[test]
    fn bayesian_feature_extraction_covers_required_signals() {
        let ranker = BayesianRanker::new(BayesianRankerConfig::default());
        let path = ExactGraphQueryEngine::new(vec![edge(
            "AuthService.login",
            RelationKind::ChecksRole,
            "admin",
            1,
        )])
        .trace_path(
            "AuthService.login",
            "admin",
            &[RelationKind::ChecksRole],
            limits(),
        )
        .remove(0);
        let mut document =
            RetrievalDocument::new("AuthService.login", "auth login").stage0_score(0.8);
        document
            .metadata
            .insert("file_centrality".to_string(), "0.6".to_string());
        document
            .metadata
            .insert("test_failure_link".to_string(), "true".to_string());
        document
            .metadata
            .insert("recent_edit_link".to_string(), "0.4".to_string());
        let mut components = BTreeMap::new();
        components.insert("stage1".to_string(), 0.7);
        let rerank = RerankScore {
            id: "AuthService.login".to_string(),
            score: 0.9,
            exact_seed: true,
            components,
        };

        let features = ranker.features_for_path(
            "AuthService.login",
            &path,
            Some(&document),
            Some(&rerank),
            &["AuthService.login".to_string()],
        );

        for name in BAYESIAN_FEATURE_NAMES {
            assert!(
                features.feature_value(name).is_some(),
                "missing feature {name}"
            );
        }
        assert_eq!(features.exact_symbol_match, 1.0);
        assert_eq!(features.security_relation_presence, 1.0);
        assert_eq!(features.test_failure_link, 1.0);
        assert_eq!(features.recent_edit_link, 0.4);
    }

    #[test]
    fn bayesian_score_is_monotonic_for_obvious_signals() {
        let ranker = BayesianRanker::new(BayesianRankerConfig::default());
        let weak = RankingFeatures {
            bm25_score: 0.1,
            rerank_score: 0.1,
            edge_confidence: 0.5,
            graph_distance: 0.2,
            path_length: 0.2,
            relation_signature: 0.5,
            ..RankingFeatures::default()
        };
        let strong = RankingFeatures {
            exact_symbol_match: 1.0,
            bm25_score: 0.9,
            rerank_score: 0.9,
            edge_confidence: 1.0,
            graph_distance: 0.9,
            path_length: 0.9,
            relation_signature: 0.9,
            ..RankingFeatures::default()
        };

        let weak_score = ranker.score_features(BayesianScoreInput {
            id: "weak".to_string(),
            source: "a".to_string(),
            target: "b".to_string(),
            features: weak,
            relation_prior: 0.6,
            uncertainty: 0.2,
            relation_signature: "CALLS".to_string(),
        });
        let strong_score = ranker.score_features(BayesianScoreInput {
            id: "strong".to_string(),
            source: "a".to_string(),
            target: "b".to_string(),
            features: strong,
            relation_prior: 0.9,
            uncertainty: 0.0,
            relation_signature: "CALLS".to_string(),
        });

        assert!(strong_score.probability > weak_score.probability);
    }

    #[test]
    fn bayesian_uncertainty_penalty_lowers_heuristic_paths() {
        let ranker = BayesianRanker::new(BayesianRankerConfig::default());
        let verified = RankingFeatures {
            edge_confidence: 1.0,
            relation_signature: 0.9,
            graph_distance: 0.8,
            path_length: 0.8,
            ..RankingFeatures::default()
        };
        let heuristic = RankingFeatures {
            edge_confidence: 0.4,
            relation_signature: 0.5,
            graph_distance: 0.8,
            path_length: 0.8,
            ..RankingFeatures::default()
        };

        let verified_score = ranker.score_features(BayesianScoreInput {
            id: "verified".to_string(),
            source: "a".to_string(),
            target: "b".to_string(),
            features: verified,
            relation_prior: 0.9,
            uncertainty: 0.0,
            relation_signature: "CALLS".to_string(),
        });
        let heuristic_score = ranker.score_features(BayesianScoreInput {
            id: "heuristic".to_string(),
            source: "a".to_string(),
            target: "b".to_string(),
            features: heuristic,
            relation_prior: 0.5,
            uncertainty: 1.0,
            relation_signature: "CHECKS_ROLE".to_string(),
        });

        assert!(verified_score.probability > heuristic_score.probability);
        assert!(heuristic_score.uncertainty > verified_score.uncertainty);
    }

    #[test]
    fn bayesian_config_loading_is_deterministic() {
        let config = ok(BayesianRankerConfig::from_json_str(
            r#"{
                "bias": -1.5,
                "uncertainty_weight": 2.25,
                "reliability_bucket_count": 5,
                "weights": {
                    "exact_symbol_match": 3.0,
                    "bm25_score": 0.25
                },
                "relation_priors": {
                    "CALLS": 0.91,
                    "CHECKS_ROLE": 0.61
                }
            }"#,
        ));

        assert_eq!(config.bias, -1.5);
        assert_eq!(config.uncertainty_weight, 2.25);
        assert_eq!(config.reliability_bucket_count, 5);
        assert_eq!(config.weights.exact_symbol_match, 3.0);
        assert_eq!(config.weights.bm25_score, 0.25);
        assert_eq!(
            config.relation_priors.get(&RelationKind::Calls).copied(),
            Some(0.91)
        );
        assert_eq!(
            config
                .relation_priors
                .get(&RelationKind::ChecksRole)
                .copied(),
            Some(0.61)
        );
    }

    #[test]
    fn context_packet_includes_confidence_and_uncertainty_fields() {
        let funnel = ok(RetrievalFunnel::new(
            vec![
                edge(
                    "AuthService.login",
                    RelationKind::Calls,
                    "normalizeEmail",
                    1,
                ),
                edge(
                    "normalizeEmail",
                    RelationKind::Writes,
                    "TokenPayload.sub",
                    2,
                ),
            ],
            vec![RetrievalDocument::new("AuthService.login", "auth login").stage0_score(1.0)],
            funnel_config(4, 4),
        ));

        let result = ok(funnel.run(
            RetrievalFunnelRequest::new("Change AuthService.login", "impact", 1_000)
                .exact_seeds(vec!["AuthService.login".to_string()]),
        ));

        assert!(!result.bayesian_scores.is_empty());
        assert!(result.packet.metadata.contains_key("confidence"));
        assert!(result.packet.metadata.contains_key("uncertainty"));
        assert!(result.packet.metadata.contains_key("bayesian_scores"));
        assert!(result.packet.metadata.contains_key("calibration_metrics"));
        assert_eq!(
            result
                .packet
                .metadata
                .get("phase")
                .and_then(|value| value.as_str()),
            Some("14")
        );
    }

    #[test]
    fn context_packet_respects_token_budget_approximately() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge("api", RelationKind::Calls, "service", 1),
            edge("service", RelationKind::Calls, "repo", 2),
            edge("repo", RelationKind::Writes, "users.email", 3),
            heuristic_edge("auth.spec", RelationKind::Tests, "api", 4),
        ]);
        let sources = BTreeMap::from([(
            "fixtures/query.ts".to_string(),
            "api();\nservice();\nrepo.write(users.email);\nit('covers api', () => api());"
                .to_string(),
        )]);
        let packet = engine.context_pack(
            ContextPackRequest::new("Small packet", "impact", 120, vec!["api".to_string()]),
            &sources,
        );
        let estimated = packet
            .metadata
            .get("estimated_tokens")
            .and_then(|value| value.as_u64())
            .unwrap_or(u64::MAX);

        assert!(estimated <= 260, "estimated token count was {estimated}");
    }

    #[test]
    fn heuristic_edges_in_context_packet_keep_explicit_labels() {
        let engine = ExactGraphQueryEngine::new(vec![
            heuristic_edge("route", RelationKind::Exposes, "endpoint", 1),
            edge("endpoint", RelationKind::Calls, "guard", 2),
            heuristic_edge("guard", RelationKind::ChecksRole, "admin", 3),
        ]);
        let packet = engine.context_pack(
            ContextPackRequest::new(
                "Review admin route",
                "security-auth-review",
                600,
                vec!["route".to_string()],
            ),
            &BTreeMap::new(),
        );

        for path in &packet.verified_paths {
            assert!(path.confidence <= 1.0);
            assert!(matches!(
                path.exactness,
                Exactness::StaticHeuristic | Exactness::Inferred | Exactness::ParserVerified
            ));
            let labels = path
                .metadata
                .get("edge_labels")
                .and_then(|value| value.as_array())
                .expect("edge labels metadata");
            assert!(labels.iter().all(|label| {
                label.get("exactness").is_some()
                    && label.get("confidence").is_some()
                    && label.get("edge_class").is_some()
                    && label.get("context").is_some()
                    && label.get("derived").is_some()
                    && label.get("provenance_edges").is_some()
            }));
        }
    }

    #[test]
    fn long_chains_are_represented_as_path_evidence() {
        let engine = ExactGraphQueryEngine::new(vec![
            edge("a", RelationKind::Calls, "b", 1),
            edge("b", RelationKind::Calls, "c", 2),
            edge("c", RelationKind::Calls, "d", 3),
            edge("d", RelationKind::Writes, "sink", 4),
        ]);
        let packet = engine.context_pack(
            ContextPackRequest::new("Trace mutation", "impact", 4_000, vec!["a".to_string()]),
            &BTreeMap::new(),
        );

        let long_path = packet
            .verified_paths
            .iter()
            .find(|path| path.length >= 4)
            .expect("long path evidence");
        assert_eq!(long_path.edges.len(), long_path.length);
        assert_eq!(long_path.metapath.len(), long_path.length);
        assert!(long_path
            .summary
            .as_ref()
            .is_some_and(|summary| summary.contains("CALLS")));
    }
}
