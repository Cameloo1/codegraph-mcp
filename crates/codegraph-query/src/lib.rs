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
    time::Instant,
};

use codegraph_core::{
    classify_edge_evidence_role, combine_evidence_roles, infer_edge_class, infer_edge_context,
    ContextPacket, ContextSnippet, DerivedClosureEdge, Edge, EdgeClass, Entity, EntityKind,
    EvidenceRole, Exactness, FileRecord, Metadata, PathEvidence, RelationKind, RetrievalCandidate,
    RetrievalCandidateLifecycleStatus, RetrievalCandidateSource, RetrievalProofStatus,
    RetrievalVerificationStatus, SourceSpan, VectorEmbeddingSource,
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
    Unknown,
}

impl PathContext {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Test => "test",
            Self::Mock => "mock",
            Self::Mixed => "mixed",
            Self::Unknown => "unknown",
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
    let mut saw_unknown = false;

    for step in &path.steps {
        match classify_edge_context(&step.edge) {
            PathContext::Production => saw_production = true,
            PathContext::Test => saw_test = true,
            PathContext::Mock => saw_mock = true,
            PathContext::Unknown => saw_unknown = true,
            PathContext::Mixed => {
                saw_production = true;
                saw_test = true;
            }
        }
    }

    match (saw_production, saw_test, saw_mock, saw_unknown) {
        (true, true, _, _) | (true, _, true, _) | (_, true, true, _) => PathContext::Mixed,
        (true, false, false, true) => PathContext::Mixed,
        (false, true, false, true) => PathContext::Mixed,
        (false, false, true, true) => PathContext::Mixed,
        (_, _, true, false) => PathContext::Mock,
        (_, true, _, false) => PathContext::Test,
        (true, false, false, false) => PathContext::Production,
        _ => PathContext::Unknown,
    }
}

fn classify_edge_context(edge: &Edge) -> PathContext {
    match classify_edge_evidence_role(edge).role {
        EvidenceRole::Production => PathContext::Production,
        EvidenceRole::Test => PathContext::Test,
        EvidenceRole::Mock => PathContext::Mock,
        EvidenceRole::Mixed => PathContext::Mixed,
        EvidenceRole::Unknown => PathContext::Unknown,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalMode {
    Production,
    TestImpact,
    Impact,
    DebugAudit,
}

impl TraversalMode {
    pub fn for_mode(mode: &str) -> Self {
        let normalized = mode.to_ascii_lowercase();
        if normalized.contains("debug") || normalized.contains("audit") {
            Self::DebugAudit
        } else if normalized.contains("test") {
            Self::TestImpact
        } else if normalized.contains("impact") || normalized.contains("blast") {
            Self::Impact
        } else {
            Self::Production
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::TestImpact => "test-impact",
            Self::Impact => "impact",
            Self::DebugAudit => "debug-audit",
        }
    }

    const fn allows_test_mock(self) -> bool {
        matches!(self, Self::TestImpact | Self::DebugAudit)
    }

    const fn allows_heuristic(self) -> bool {
        matches!(self, Self::DebugAudit)
    }

    const fn allows_unknown_source_role(self) -> bool {
        matches!(self, Self::DebugAudit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraversalPolicy {
    pub mode: TraversalMode,
    pub max_neighbors_per_node: Option<usize>,
    pub max_structural_expansion: Option<usize>,
    pub timeout_ms: Option<u64>,
    pub max_candidate_seeds: usize,
}

impl TraversalPolicy {
    pub fn for_mode(mode: &str) -> Self {
        match TraversalMode::for_mode(mode) {
            TraversalMode::DebugAudit => Self {
                mode: TraversalMode::DebugAudit,
                max_neighbors_per_node: Some(256),
                max_structural_expansion: Some(64),
                timeout_ms: Some(3_000),
                max_candidate_seeds: 64,
            },
            TraversalMode::TestImpact => Self {
                mode: TraversalMode::TestImpact,
                max_neighbors_per_node: Some(96),
                max_structural_expansion: Some(0),
                timeout_ms: Some(1_500),
                max_candidate_seeds: 32,
            },
            TraversalMode::Impact => Self {
                mode: TraversalMode::Impact,
                max_neighbors_per_node: Some(96),
                max_structural_expansion: Some(0),
                timeout_ms: Some(1_500),
                max_candidate_seeds: 32,
            },
            TraversalMode::Production => Self {
                mode: TraversalMode::Production,
                max_neighbors_per_node: Some(64),
                max_structural_expansion: Some(0),
                timeout_ms: Some(1_000),
                max_candidate_seeds: 16,
            },
        }
    }

    pub const fn debug_audit() -> Self {
        Self {
            mode: TraversalMode::DebugAudit,
            max_neighbors_per_node: Some(256),
            max_structural_expansion: Some(64),
            timeout_ms: Some(3_000),
            max_candidate_seeds: 64,
        }
    }

    pub const fn source_role_filter_applies(self) -> bool {
        !matches!(self.mode, TraversalMode::DebugAudit)
    }

    fn relation_allowed(self, relation: RelationKind) -> bool {
        if matches!(self.mode, TraversalMode::DebugAudit) {
            return true;
        }
        if is_structural_relation(relation) {
            return false;
        }
        production_traversal_relation_allowed(relation)
            || (self.mode.allows_test_mock() && test_traversal_relation_allowed(relation))
    }

    fn source_role_allowed(self, edge: &Edge) -> bool {
        match classify_edge_context(edge) {
            PathContext::Production => true,
            PathContext::Test | PathContext::Mock | PathContext::Mixed => {
                self.mode.allows_test_mock()
            }
            PathContext::Unknown => self.mode.allows_unknown_source_role(),
        }
    }

    fn heuristic_allowed(self) -> bool {
        self.mode.allows_heuristic()
    }

    fn derived_without_provenance_allowed(self) -> bool {
        matches!(self.mode, TraversalMode::DebugAudit)
    }
}

fn context_pack_query_limits_for_policy(policy: TraversalPolicy) -> QueryLimits {
    match policy.mode {
        TraversalMode::DebugAudit => QueryLimits {
            max_depth: 6,
            max_paths: 48,
            max_edges_visited: 4_096,
        },
        TraversalMode::TestImpact => QueryLimits {
            max_depth: 4,
            max_paths: 24,
            max_edges_visited: 2_048,
        },
        TraversalMode::Impact => QueryLimits {
            max_depth: 4,
            max_paths: 24,
            max_edges_visited: 2_048,
        },
        TraversalMode::Production => QueryLimits {
            max_depth: 3,
            max_paths: 12,
            max_edges_visited: 2_048,
        },
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphTraversalTelemetry {
    pub operation: String,
    pub seed_count: usize,
    pub seed: Option<String>,
    pub candidate_count_by_source: BTreeMap<String, usize>,
    pub relation_modes: Vec<String>,
    pub traversal_policy: String,
    pub max_depth: usize,
    pub max_paths: usize,
    pub max_edge_visits: usize,
    pub max_neighbors_per_node: Option<usize>,
    pub max_structural_expansion: Option<usize>,
    pub timeout_ms: Option<u64>,
    pub edges_visited: usize,
    pub nodes_visited: usize,
    pub neighbors_expanded: usize,
    pub neighbor_limit_hits: usize,
    pub neighbors_omitted_by_limit: usize,
    pub structural_edges_skipped: usize,
    pub structural_expansion_limit_hits: usize,
    pub relation_blocked_edges: usize,
    pub source_role_blocked_edges: usize,
    pub source_role_blocked_edge_ids: BTreeSet<String>,
    pub cycles_cut: usize,
    pub depth_limit_hits: usize,
    pub budget_stop_reason: Option<String>,
    pub paths_found: usize,
    pub paths_returned: usize,
    pub time_ms: f64,
    pub source_role_filters_applied: bool,
    pub heuristic_edges_skipped: usize,
    pub heuristic_edges_seen: usize,
    pub derived_edge_provenance_checks: usize,
    pub derived_edge_missing_provenance: usize,
    pub derived_edge_provenance_blocked_edges: usize,
    pub no_proof_fallback_reason: Option<String>,
}

impl GraphTraversalTelemetry {
    fn new_with_policy(
        operation: impl Into<String>,
        seed: impl Into<String>,
        traversals: &[Traversal],
        limits: QueryLimits,
        policy: TraversalPolicy,
    ) -> Self {
        Self {
            operation: operation.into(),
            seed_count: 1,
            seed: Some(seed.into()),
            candidate_count_by_source: BTreeMap::new(),
            relation_modes: traversal_mode_labels(traversals),
            traversal_policy: policy.mode.as_str().to_string(),
            max_depth: limits.max_depth,
            max_paths: limits.max_paths,
            max_edge_visits: limits.max_edges_visited,
            max_neighbors_per_node: policy.max_neighbors_per_node,
            max_structural_expansion: policy.max_structural_expansion,
            timeout_ms: policy.timeout_ms,
            edges_visited: 0,
            nodes_visited: 0,
            neighbors_expanded: 0,
            neighbor_limit_hits: 0,
            neighbors_omitted_by_limit: 0,
            structural_edges_skipped: 0,
            structural_expansion_limit_hits: 0,
            relation_blocked_edges: 0,
            source_role_blocked_edges: 0,
            source_role_blocked_edge_ids: BTreeSet::new(),
            cycles_cut: 0,
            depth_limit_hits: 0,
            budget_stop_reason: None,
            paths_found: 0,
            paths_returned: 0,
            time_ms: 0.0,
            source_role_filters_applied: policy.source_role_filter_applies(),
            heuristic_edges_skipped: 0,
            heuristic_edges_seen: 0,
            derived_edge_provenance_checks: 0,
            derived_edge_missing_provenance: 0,
            derived_edge_provenance_blocked_edges: 0,
            no_proof_fallback_reason: None,
        }
    }

    fn note_depth_limit(&mut self) {
        self.depth_limit_hits += 1;
        if self.budget_stop_reason.is_none() {
            self.budget_stop_reason = Some("max_depth".to_string());
        }
    }

    fn note_budget_stop(&mut self, reason: &str) {
        self.budget_stop_reason = Some(reason.to_string());
    }

    fn record_edge_visit(&mut self, edge: &Edge) {
        self.edges_visited += 1;
        if is_heuristic_edge(edge) {
            self.heuristic_edges_seen += 1;
        }
    }

    fn note_source_role_blocked(&mut self, edge: &Edge) {
        if self.source_role_blocked_edge_ids.insert(edge.id.clone()) {
            self.source_role_blocked_edges += 1;
        }
        self.source_role_filters_applied = true;
    }

    fn absorb_child_traversal(&mut self, child: &GraphTraversalTelemetry) {
        for label in &child.relation_modes {
            if !self.relation_modes.contains(label) {
                self.relation_modes.push(label.clone());
            }
        }
        self.edges_visited += child.edges_visited;
        self.nodes_visited += child.nodes_visited;
        self.neighbors_expanded += child.neighbors_expanded;
        self.neighbor_limit_hits += child.neighbor_limit_hits;
        self.neighbors_omitted_by_limit += child.neighbors_omitted_by_limit;
        self.structural_edges_skipped += child.structural_edges_skipped;
        self.structural_expansion_limit_hits += child.structural_expansion_limit_hits;
        self.relation_blocked_edges += child.relation_blocked_edges;
        self.source_role_blocked_edge_ids
            .extend(child.source_role_blocked_edge_ids.iter().cloned());
        self.source_role_blocked_edges = self.source_role_blocked_edge_ids.len();
        self.cycles_cut += child.cycles_cut;
        self.depth_limit_hits += child.depth_limit_hits;
        self.source_role_filters_applied =
            self.source_role_filters_applied || child.source_role_filters_applied;
        self.heuristic_edges_skipped += child.heuristic_edges_skipped;
        self.heuristic_edges_seen += child.heuristic_edges_seen;
        self.derived_edge_provenance_checks += child.derived_edge_provenance_checks;
        self.derived_edge_missing_provenance += child.derived_edge_missing_provenance;
        self.derived_edge_provenance_blocked_edges += child.derived_edge_provenance_blocked_edges;
        if self.budget_stop_reason.is_none() {
            self.budget_stop_reason = child.budget_stop_reason.clone();
        }
    }

    fn finish(mut self, start: Instant, paths_returned: usize) -> Self {
        self.paths_found = paths_returned;
        self.paths_returned = paths_returned;
        self.time_ms = start.elapsed().as_secs_f64() * 1000.0;
        if paths_returned == 0 && self.no_proof_fallback_reason.is_none() {
            self.no_proof_fallback_reason = Some("no matching graph path accepted".to_string());
        }
        self
    }

    pub fn result_label(&self) -> &'static str {
        if self.paths_returned > 0
            && self.heuristic_edges_seen == self.heuristic_edges_skipped
            && self.derived_edge_provenance_blocked_edges == 0
        {
            return "proof_path_found";
        }

        if self.paths_returned > 0 {
            return "traversal_unknown";
        }

        if self
            .budget_stop_reason
            .as_deref()
            .is_some_and(is_traversal_budget_stop_reason)
        {
            return "traversal_budget_exhausted";
        }

        if self.cycles_cut > 0 {
            "traversal_cycle_cut"
        } else if self.source_role_blocked_edges > 0 {
            "traversal_source_role_blocked"
        } else if self.heuristic_edges_skipped > 0 {
            "traversal_heuristic_blocked"
        } else if self.relation_blocked_edges > 0 {
            "traversal_relation_blocked"
        } else {
            "no_proof_path_found"
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "operation": self.operation.as_str(),
            "seed_count": self.seed_count,
            "seed": self.seed.as_deref(),
            "candidate_count_by_source": &self.candidate_count_by_source,
            "relation_modes": &self.relation_modes,
            "traversal_policy": self.traversal_policy,
            "max_depth": self.max_depth,
            "max_paths": self.max_paths,
            "max_edge_visits": self.max_edge_visits,
            "max_neighbors_per_node": self.max_neighbors_per_node,
            "max_structural_expansion": self.max_structural_expansion,
            "timeout_ms": self.timeout_ms,
            "edges_visited": self.edges_visited,
            "nodes_visited": self.nodes_visited,
            "neighbors_expanded": self.neighbors_expanded,
            "neighbor_limit_hits": self.neighbor_limit_hits,
            "neighbors_omitted_by_limit": self.neighbors_omitted_by_limit,
            "structural_edges_skipped": self.structural_edges_skipped,
            "structural_expansion_limit_hits": self.structural_expansion_limit_hits,
            "relation_blocked_edges": self.relation_blocked_edges,
            "source_role_blocked_edges": self.source_role_blocked_edges,
            "source_role_blocked_unique_edges": self.source_role_blocked_edge_ids.len(),
            "cycles_cut": self.cycles_cut,
            "depth_limit_hits": self.depth_limit_hits,
            "budget_stop_reason": self.budget_stop_reason.as_deref().unwrap_or("completed"),
            "result_label": self.result_label(),
            "paths_found": self.paths_found,
            "paths_returned": self.paths_returned,
            "time_ms": self.time_ms,
            "source_role_filters_applied": self.source_role_filters_applied,
            "heuristic_edges_skipped": self.heuristic_edges_skipped,
            "heuristic_edges_seen": self.heuristic_edges_seen,
            "derived_edge_provenance_checks": self.derived_edge_provenance_checks,
            "derived_edge_missing_provenance": self.derived_edge_missing_provenance,
            "derived_edge_provenance_blocked_edges": self.derived_edge_provenance_blocked_edges,
            "no_proof_fallback_reason": self.no_proof_fallback_reason.as_deref(),
        })
    }
}

fn traversal_mode_labels(traversals: &[Traversal]) -> Vec<String> {
    traversals
        .iter()
        .map(|traversal| {
            let direction = match traversal.direction {
                TraversalDirection::Forward => "forward",
                TraversalDirection::Reverse => "reverse",
            };
            format!("{}:{direction}", traversal.relation.as_str())
        })
        .collect()
}

fn is_structural_relation(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Contains
            | RelationKind::DefinedIn
            | RelationKind::Declares
            | RelationKind::Argument0
            | RelationKind::Argument1
            | RelationKind::ArgumentN
            | RelationKind::Callee
    )
}

fn is_heuristic_edge(edge: &Edge) -> bool {
    matches!(
        edge.edge_class,
        EdgeClass::BaseHeuristic | EdgeClass::Unknown
    ) || matches!(
        edge.exactness,
        Exactness::StaticHeuristic | Exactness::Inferred
    )
}

fn production_traversal_relation_allowed(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Calls
            | RelationKind::Reads
            | RelationKind::Writes
            | RelationKind::FlowsTo
            | RelationKind::AssignedFrom
            | RelationKind::Mutates
            | RelationKind::MayMutate
            | RelationKind::MayRead
            | RelationKind::ApiReaches
            | RelationKind::AsyncReaches
            | RelationKind::SchemaImpact
            | RelationKind::Imports
            | RelationKind::Exports
            | RelationKind::Reexports
            | RelationKind::AliasOf
            | RelationKind::AliasedBy
            | RelationKind::Authorizes
            | RelationKind::ChecksRole
            | RelationKind::ChecksPermission
            | RelationKind::Sanitizes
            | RelationKind::Validates
            | RelationKind::Exposes
            | RelationKind::Injects
            | RelationKind::Instantiates
            | RelationKind::Publishes
            | RelationKind::Emits
            | RelationKind::Consumes
            | RelationKind::ListensTo
            | RelationKind::SubscribesTo
            | RelationKind::Handles
            | RelationKind::Migrates
            | RelationKind::AltersColumn
            | RelationKind::DependsOnSchema
            | RelationKind::ReadsTable
            | RelationKind::WritesTable
    )
}

const CONTEXT_PACK_ONE_HOP_RELATION_HYDRATION_TRAVERSALS: [Traversal; 26] = [
    Traversal::forward(RelationKind::Calls),
    Traversal::reverse(RelationKind::Calls),
    Traversal::forward(RelationKind::Imports),
    Traversal::reverse(RelationKind::Imports),
    Traversal::forward(RelationKind::Reads),
    Traversal::reverse(RelationKind::Reads),
    Traversal::forward(RelationKind::Writes),
    Traversal::reverse(RelationKind::Writes),
    Traversal::forward(RelationKind::Mutates),
    Traversal::reverse(RelationKind::Mutates),
    Traversal::forward(RelationKind::MayMutate),
    Traversal::reverse(RelationKind::MayMutate),
    Traversal::forward(RelationKind::MayRead),
    Traversal::reverse(RelationKind::MayRead),
    Traversal::forward(RelationKind::FlowsTo),
    Traversal::reverse(RelationKind::FlowsTo),
    Traversal::forward(RelationKind::AssignedFrom),
    Traversal::reverse(RelationKind::AssignedFrom),
    Traversal::forward(RelationKind::Exports),
    Traversal::reverse(RelationKind::Exports),
    Traversal::forward(RelationKind::Reexports),
    Traversal::reverse(RelationKind::Reexports),
    Traversal::forward(RelationKind::AliasOf),
    Traversal::reverse(RelationKind::AliasOf),
    Traversal::forward(RelationKind::AliasedBy),
    Traversal::reverse(RelationKind::AliasedBy),
];

fn context_pack_one_hop_relation_hydration_traversals() -> &'static [Traversal] {
    &CONTEXT_PACK_ONE_HOP_RELATION_HYDRATION_TRAVERSALS
}

fn test_traversal_relation_allowed(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Tests
            | RelationKind::Covers
            | RelationKind::Asserts
            | RelationKind::Mocks
            | RelationKind::Stubs
            | RelationKind::FixturesFor
    )
}

fn is_traversal_budget_stop_reason(reason: &str) -> bool {
    matches!(
        reason,
        "max_depth"
            | "max_paths"
            | "max_edge_visits"
            | "max_neighbors_per_node"
            | "max_structural_expansion"
            | "timeout_ms"
    )
}

fn traversal_timed_out(start: Instant, policy: TraversalPolicy) -> bool {
    policy
        .timeout_ms
        .is_some_and(|timeout_ms| start.elapsed().as_millis() >= u128::from(timeout_ms))
}

fn aggregate_graph_traversal_telemetry_json(runs: &[GraphTraversalTelemetry]) -> serde_json::Value {
    let mut candidate_count_by_source = BTreeMap::<String, usize>::new();
    let mut relation_modes = BTreeSet::<String>::new();
    let mut traversal_policies = BTreeSet::<String>::new();
    let mut result_labels = BTreeMap::<String, usize>::new();
    let mut budget_stop_reasons = BTreeMap::<String, usize>::new();
    let mut max_depth = 0usize;
    let mut max_paths = 0usize;
    let mut max_edge_visits = 0usize;
    let mut max_neighbors_per_node = 0usize;
    let mut max_structural_expansion = 0usize;
    let mut timeout_ms = 0u64;
    let mut edges_visited = 0usize;
    let mut nodes_visited = 0usize;
    let mut neighbors_expanded = 0usize;
    let mut neighbor_limit_hits = 0usize;
    let mut neighbors_omitted_by_limit = 0usize;
    let mut structural_edges_skipped = 0usize;
    let mut structural_expansion_limit_hits = 0usize;
    let mut relation_blocked_edges = 0usize;
    let mut source_role_blocked_edge_ids = BTreeSet::new();
    let mut cycles_cut = 0usize;
    let mut depth_limit_hits = 0usize;
    let mut paths_found = 0usize;
    let mut paths_returned = 0usize;
    let mut time_ms = 0.0f64;
    let mut heuristic_edges_skipped = 0usize;
    let mut heuristic_edges_seen = 0usize;
    let mut derived_edge_provenance_checks = 0usize;
    let mut derived_edge_missing_provenance = 0usize;
    let mut derived_edge_provenance_blocked_edges = 0usize;

    for run in runs {
        for (source, count) in &run.candidate_count_by_source {
            *candidate_count_by_source.entry(source.clone()).or_default() += count;
        }
        relation_modes.extend(run.relation_modes.iter().cloned());
        traversal_policies.insert(run.traversal_policy.clone());
        *result_labels
            .entry(run.result_label().to_string())
            .or_default() += 1;
        *budget_stop_reasons
            .entry(
                run.budget_stop_reason
                    .clone()
                    .unwrap_or_else(|| "completed".to_string()),
            )
            .or_default() += 1;
        max_depth = max_depth.max(run.max_depth);
        max_paths = max_paths.max(run.max_paths);
        max_edge_visits = max_edge_visits.max(run.max_edge_visits);
        max_neighbors_per_node =
            max_neighbors_per_node.max(run.max_neighbors_per_node.unwrap_or(0));
        max_structural_expansion =
            max_structural_expansion.max(run.max_structural_expansion.unwrap_or(0));
        timeout_ms = timeout_ms.max(run.timeout_ms.unwrap_or(0));
        edges_visited += run.edges_visited;
        nodes_visited += run.nodes_visited;
        neighbors_expanded += run.neighbors_expanded;
        neighbor_limit_hits += run.neighbor_limit_hits;
        neighbors_omitted_by_limit += run.neighbors_omitted_by_limit;
        structural_edges_skipped += run.structural_edges_skipped;
        structural_expansion_limit_hits += run.structural_expansion_limit_hits;
        relation_blocked_edges += run.relation_blocked_edges;
        source_role_blocked_edge_ids.extend(run.source_role_blocked_edge_ids.iter().cloned());
        cycles_cut += run.cycles_cut;
        depth_limit_hits += run.depth_limit_hits;
        paths_found += run.paths_found;
        paths_returned += run.paths_returned;
        time_ms += run.time_ms;
        heuristic_edges_skipped += run.heuristic_edges_skipped;
        heuristic_edges_seen += run.heuristic_edges_seen;
        derived_edge_provenance_checks += run.derived_edge_provenance_checks;
        derived_edge_missing_provenance += run.derived_edge_missing_provenance;
        derived_edge_provenance_blocked_edges += run.derived_edge_provenance_blocked_edges;
    }

    serde_json::json!({
        "run_count": runs.len(),
        "seed_count": runs.iter().map(|run| run.seed_count).sum::<usize>(),
        "candidate_count_by_source": candidate_count_by_source,
        "relation_modes": relation_modes.into_iter().collect::<Vec<_>>(),
        "traversal_policies": traversal_policies.into_iter().collect::<Vec<_>>(),
        "max_depth": max_depth,
        "max_paths": max_paths,
        "max_edge_visits": max_edge_visits,
        "max_neighbors_per_node": if max_neighbors_per_node == 0 {
            serde_json::Value::Null
        } else {
            serde_json::json!(max_neighbors_per_node)
        },
        "max_structural_expansion": if max_structural_expansion == 0 {
            serde_json::Value::Null
        } else {
            serde_json::json!(max_structural_expansion)
        },
        "timeout_ms": if timeout_ms == 0 {
            serde_json::Value::Null
        } else {
            serde_json::json!(timeout_ms)
        },
        "edges_visited": edges_visited,
        "nodes_visited": nodes_visited,
        "neighbors_expanded": neighbors_expanded,
        "neighbor_limit_hits": neighbor_limit_hits,
        "neighbors_omitted_by_limit": neighbors_omitted_by_limit,
        "structural_edges_skipped": structural_edges_skipped,
        "structural_expansion_limit_hits": structural_expansion_limit_hits,
        "relation_blocked_edges": relation_blocked_edges,
        "source_role_blocked_edges": source_role_blocked_edge_ids.len(),
        "source_role_blocked_unique_edges": source_role_blocked_edge_ids.len(),
        "cycles_cut": cycles_cut,
        "depth_limit_hits": depth_limit_hits,
        "budget_stop_reasons": budget_stop_reasons,
        "result_labels": result_labels,
        "paths_found": paths_found,
        "paths_returned": paths_returned,
        "time_ms": time_ms,
        "source_role_filters_applied": runs.iter().any(|run| run.source_role_filters_applied),
        "heuristic_edges_skipped": heuristic_edges_skipped,
        "heuristic_edges_seen": heuristic_edges_seen,
        "derived_edge_provenance_checks": derived_edge_provenance_checks,
        "derived_edge_missing_provenance": derived_edge_missing_provenance,
        "derived_edge_provenance_blocked_edges": derived_edge_provenance_blocked_edges,
        "no_proof_fallback_reason": if paths_found == 0 {
            Some("no matching graph path accepted")
        } else {
            None
        },
    })
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
    pub binary_overfetch_k: usize,
    pub stage2_top_n: usize,
    pub enable_vector_candidates_by_default: bool,
    pub vector_candidate_top_k: usize,
    pub enable_nuance_rescue_by_default: bool,
    pub nuance_rescue_top_k: usize,
    pub query_limits: QueryLimits,
    pub rerank_config: RerankConfig,
    pub bayesian_config: BayesianRankerConfig,
}

impl Default for RetrievalFunnelConfig {
    fn default() -> Self {
        Self {
            binary_dimensions: 128,
            stage1_top_k: 32,
            binary_overfetch_k: 0,
            stage2_top_n: 16,
            enable_vector_candidates_by_default: false,
            vector_candidate_top_k: 16,
            enable_nuance_rescue_by_default: false,
            nuance_rescue_top_k: 8,
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

impl RetrievalFunnelConfig {
    pub fn binary_overfetch_k(&self) -> usize {
        if self.binary_overfetch_k == 0 {
            self.stage1_top_k
        } else {
            self.binary_overfetch_k
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VectorCandidateBranchStatus {
    Missing,
    Ready,
    Stale { reason: String },
}

impl VectorCandidateBranchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Ready => "ready",
            Self::Stale { .. } => "stale",
        }
    }

    fn warning(&self) -> Option<String> {
        match self {
            Self::Missing => {
                Some("vector index missing; continuing without vector candidates".into())
            }
            Self::Ready => None,
            Self::Stale { reason } => Some(format!(
                "vector index stale or incompatible; continuing without vector candidates: {reason}"
            )),
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
    pub enable_vector_candidates: bool,
    pub enable_nuance_rescue_candidates: bool,
    pub vector_candidate_diagnostics: bool,
    pub vector_branch_status: VectorCandidateBranchStatus,
    pub vector_candidates: Vec<RetrievalCandidate>,
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
            enable_vector_candidates: false,
            enable_nuance_rescue_candidates: false,
            vector_candidate_diagnostics: false,
            vector_branch_status: VectorCandidateBranchStatus::Missing,
            vector_candidates: Vec::new(),
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

    pub fn enable_vector_candidates(mut self, enabled: bool) -> Self {
        self.enable_vector_candidates = enabled;
        self
    }

    pub fn enable_nuance_rescue_candidates(mut self, enabled: bool) -> Self {
        self.enable_nuance_rescue_candidates = enabled;
        self
    }

    pub fn vector_candidate_diagnostics(mut self, enabled: bool) -> Self {
        self.vector_candidate_diagnostics = enabled;
        self
    }

    pub fn vector_branch_status(mut self, status: VectorCandidateBranchStatus) -> Self {
        self.vector_branch_status = status;
        self
    }

    pub fn vector_candidates(mut self, candidates: Vec<RetrievalCandidate>) -> Self {
        self.vector_candidates = candidates;
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
    pub vector_candidates: Vec<RetrievalCandidate>,
    pub nuance_rescue_candidates: Vec<RetrievalCandidate>,
    pub vector_warnings: Vec<String>,
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
pub enum PromptIntent {
    EntityLookup,
    FileLookup,
    TextConfigLookup,
    BuildSystemPlanning,
    BehaviorTrace,
    CallerCalleeTrace,
    TestImpact,
    DataflowTrace,
    SecurityAuthTrace,
    DocsText,
    Unknown,
}

impl PromptIntent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EntityLookup => "entity_lookup",
            Self::FileLookup => "file_lookup",
            Self::TextConfigLookup => "text_config_lookup",
            Self::BuildSystemPlanning => "build_system_planning",
            Self::BehaviorTrace => "behavior_trace",
            Self::CallerCalleeTrace => "caller_callee_trace",
            Self::TestImpact => "test_impact",
            Self::DataflowTrace => "dataflow_trace",
            Self::SecurityAuthTrace => "security_auth_trace",
            Self::DocsText => "docs_text",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PromptSeedExactness {
    Exact,
    LiteralText,
    Heuristic,
    Ignored,
    Unknown,
}

impl PromptSeedExactness {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::LiteralText => "literal_text",
            Self::Heuristic => "heuristic",
            Self::Ignored => "ignored",
            Self::Unknown => "unknown",
        }
    }
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
    ConfigToken,
    FilePattern,
    PathToken,
    TextToken,
    TaskVerbIgnored,
    Unknown,
}

impl PromptSeedKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symbol | Self::TestName | Self::Identifier => "exact_symbol",
            Self::FilePath | Self::LineNumber | Self::StackTrace | Self::PathToken => "path_token",
            Self::ConfigToken => "config_token",
            Self::FilePattern => "file_pattern",
            Self::TextToken | Self::ErrorMessage => "text_token",
            Self::TaskVerbIgnored => "task_verb_ignored",
            Self::Unknown => "unknown",
        }
    }

    pub const fn provenance_kind(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::FilePath => "file_path",
            Self::LineNumber => "line_number",
            Self::StackTrace => "stack_trace",
            Self::TestName => "test_name",
            Self::ErrorMessage => "diagnostic",
            Self::Identifier => "code_identifier",
            Self::ConfigToken => "config_token",
            Self::FilePattern => "file_pattern",
            Self::PathToken => "path_token",
            Self::TextToken => "text_token",
            Self::TaskVerbIgnored => "task_verb_ignored",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PromptSeed {
    pub kind: PromptSeedKind,
    pub value: String,
    pub source_text: String,
    pub intent_contribution: PromptIntent,
    pub exactness: PromptSeedExactness,
    pub ignored_reason: Option<String>,
    pub file_path: Option<String>,
    pub line: Option<u32>,
    pub function: Option<String>,
    pub exact: bool,
}

impl PromptSeed {
    fn simple(kind: PromptSeedKind, value: impl Into<String>, exact: bool) -> Self {
        Self::with_source(kind, value, None::<String>, exact)
    }

    fn with_source(
        kind: PromptSeedKind,
        value: impl Into<String>,
        source_text: Option<impl Into<String>>,
        exact: bool,
    ) -> Self {
        let value = value.into();
        let source_text = source_text.map(Into::into).unwrap_or_else(|| value.clone());
        Self {
            kind,
            intent_contribution: intent_contribution_for_seed(kind, &value),
            exactness: seed_exactness(kind, exact),
            ignored_reason: ignored_reason_for_seed(kind, &value),
            source_text,
            value,
            file_path: None,
            line: None,
            function: None,
            exact,
        }
    }

    pub fn exact_value(&self) -> Option<String> {
        if !self.exact {
            return None;
        }
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
            PromptSeedKind::Symbol
            | PromptSeedKind::TestName
            | PromptSeedKind::Identifier
            | PromptSeedKind::ConfigToken
            | PromptSeedKind::FilePattern
            | PromptSeedKind::PathToken
            | PromptSeedKind::TextToken => Some(self.value.clone()),
            PromptSeedKind::ErrorMessage
            | PromptSeedKind::TaskVerbIgnored
            | PromptSeedKind::Unknown => None,
        }
    }

    pub fn provenance_json(&self) -> serde_json::Value {
        serde_json::json!({
            "seed": &self.value,
            "seed_kind": self.kind.provenance_kind(),
            "value": &self.value,
            "kind": self.kind.as_str(),
            "source_text": &self.source_text,
            "intent_contribution": self.intent_contribution.as_str(),
            "exactness": self.exactness.as_str(),
            "ignored_reason": &self.ignored_reason,
            "file_path": &self.file_path,
            "line": self.line,
            "function": &self.function,
            "exact": self.exact,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSeedExtraction {
    pub intent: PromptIntent,
    pub seeds: Vec<PromptSeed>,
}

impl PromptSeedExtraction {
    pub fn provenance_json(&self) -> serde_json::Value {
        serde_json::json!({
            "intent": self.intent.as_str(),
            "seeds": self
                .seeds
                .iter()
                .map(PromptSeed::provenance_json)
                .collect::<Vec<_>>(),
        })
    }
}

pub fn extract_prompt_seeds(prompt: &str) -> Vec<PromptSeed> {
    extract_prompt_seed_provenance(prompt).seeds
}

pub fn extract_prompt_seed_provenance(prompt: &str) -> PromptSeedExtraction {
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
        extract_buildroot_phrase_seeds(trimmed, &mut seeds);

        for token in trimmed.split_whitespace() {
            extract_path_and_line_seed(token, &mut seeds);
            extract_buildroot_token_seed(token, &mut seeds);
            extract_symbol_or_identifier_seed(token, &mut seeds);
        }
    }

    let seeds = unique_prompt_seeds(seeds);
    let intent = classify_prompt_intent_from_seeds(prompt, &seeds);
    PromptSeedExtraction { intent, seeds }
}

pub fn classify_prompt_intent(prompt: &str) -> PromptIntent {
    extract_prompt_seed_provenance(prompt).intent
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskKind {
    BuildSystemPackageAuthoring,
    CodegraphInternalDebug,
    ImplementationTrace,
    StorageAccountingTrace,
    ArtifactMathTrace,
    PersistencePathTrace,
    IndexingSummaryTrace,
    SchemaViewTrace,
    BenchmarkMetricTrace,
    TestImpact,
    DataflowTrace,
    SecurityReview,
    DocsLookup,
    EntityLookup,
    FileLookup,
    Unknown,
}

impl TaskKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BuildSystemPackageAuthoring => "build_system_package_authoring",
            Self::CodegraphInternalDebug => "codegraph_internal_debug",
            Self::ImplementationTrace => "implementation_trace",
            Self::StorageAccountingTrace => "storage_accounting_trace",
            Self::ArtifactMathTrace => "artifact_math_trace",
            Self::PersistencePathTrace => "persistence_path_trace",
            Self::IndexingSummaryTrace => "indexing_summary_trace",
            Self::SchemaViewTrace => "schema_view_trace",
            Self::BenchmarkMetricTrace => "benchmark_metric_trace",
            Self::TestImpact => "test_impact",
            Self::DataflowTrace => "dataflow_trace",
            Self::SecurityReview => "security_review",
            Self::DocsLookup => "docs_lookup",
            Self::EntityLookup => "entity_lookup",
            Self::FileLookup => "file_lookup",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskIntent {
    pub task_kind: TaskKind,
    pub domain: String,
    pub confidence: f64,
    pub selected_profile: String,
    pub signals: Vec<String>,
    pub ignored_terms: Vec<String>,
    pub exact_seeds: Vec<String>,
    pub text_seeds: Vec<String>,
    pub file_path_seeds: Vec<String>,
    pub config_keys: Vec<String>,
    pub relation_goals: Vec<String>,
    pub evidence_expectation: String,
    pub ambiguity: String,
    pub why_this_intent: String,
    pub fallback_intent: Option<String>,
}

impl TaskIntent {
    pub fn task_intent_id(&self) -> String {
        format!("task_intent_{}_v1", self.task_kind.as_str())
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "task_kind": self.task_kind.as_str(),
            "task_intent_id": self.task_intent_id(),
            "domain": self.domain,
            "confidence": self.confidence,
            "selected_profile": self.selected_profile,
            "signals": self.signals,
            "ignored_terms": self.ignored_terms,
            "exact_seeds": self.exact_seeds,
            "text_seeds": self.text_seeds,
            "file_path_seeds": self.file_path_seeds,
            "config_keys": self.config_keys,
            "relation_goals": self.relation_goals,
            "evidence_expectation": self.evidence_expectation,
            "expected_evidence_type": self.evidence_expectation,
            "ambiguity": self.ambiguity,
            "why_this_intent": self.why_this_intent,
            "fallback_intent": self.fallback_intent,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskProfile {
    pub profile_id: String,
    pub profile_name: String,
    pub signals: Vec<String>,
    pub ignored_generic_terms: Vec<String>,
    pub preferred_evidence_roles: Vec<String>,
    pub preferred_file_kinds: Vec<String>,
    pub retrieval_branches: Vec<String>,
    pub graph_expectation: String,
    pub fallback_policy: String,
    pub validation_templates: Vec<String>,
    pub risk_templates: Vec<String>,
    pub role_budget: usize,
    pub confidence_rules: Vec<String>,
}

impl TaskProfile {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "profile_id": self.profile_id,
            "profile_name": self.profile_name,
            "signals": self.signals,
            "ignored_generic_terms": self.ignored_generic_terms,
            "preferred_evidence_roles": self.preferred_evidence_roles,
            "preferred_file_kinds": self.preferred_file_kinds,
            "retrieval_branches": self.retrieval_branches,
            "graph_expectation": self.graph_expectation,
            "fallback_policy": self.fallback_policy,
            "validation_templates": self.validation_templates,
            "risk_templates": self.risk_templates,
            "role_budget": self.role_budget,
            "confidence_rules": self.confidence_rules,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalQueryAtom {
    pub role: String,
    pub query_text: String,
    pub path_hints: Vec<String>,
    pub file_kind_hints: Vec<String>,
    pub evidence_role_filter: Vec<String>,
    pub candidate_source_preference: Vec<String>,
    pub max_candidates: usize,
    pub why: String,
    pub expected_signal: String,
    pub proof_expectation: String,
}

impl RetrievalQueryAtom {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "role": self.role,
            "query_text": self.query_text,
            "path_hints": self.path_hints,
            "file_kind_hints": self.file_kind_hints,
            "evidence_role_filter": self.evidence_role_filter,
            "candidate_source_preference": self.candidate_source_preference,
            "max_candidates": self.max_candidates,
            "why": self.why,
            "expected_signal": self.expected_signal,
            "proof_expectation": self.proof_expectation,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalPlan {
    pub plan_id: String,
    pub task_intent_id: String,
    pub profile_id: String,
    pub query_atoms: Vec<RetrievalQueryAtom>,
    pub candidate_branches: Vec<String>,
    pub role_budget: usize,
    pub proof_attempt_policy: String,
    pub fallback_policy: String,
    pub max_candidates: usize,
    pub max_files: usize,
    pub max_snippets: usize,
    pub max_output_bytes: usize,
    pub explain_level: String,
}

impl RetrievalPlan {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "plan_id": self.plan_id,
            "task_intent_id": self.task_intent_id,
            "profile_id": self.profile_id,
            "query_atoms": self.query_atoms.iter().map(RetrievalQueryAtom::to_json).collect::<Vec<_>>(),
            "candidate_branches": self.candidate_branches,
            "role_budget": self.role_budget,
            "proof_attempt_policy": self.proof_attempt_policy,
            "fallback_policy": self.fallback_policy,
            "max_candidates": self.max_candidates,
            "max_files": self.max_files,
            "max_snippets": self.max_snippets,
            "max_output_bytes": self.max_output_bytes,
            "explain_level": self.explain_level,
        })
    }

    pub fn summary_json(&self) -> serde_json::Value {
        serde_json::json!({
            "plan_id": self.plan_id,
            "task_intent_id": self.task_intent_id,
            "profile_id": self.profile_id,
            "query_atom_roles": self.query_atoms.iter().map(|atom| atom.role.clone()).collect::<Vec<_>>(),
            "query_atom_count": self.query_atoms.len(),
            "max_candidates": self.max_candidates,
            "max_files": self.max_files,
            "max_snippets": self.max_snippets,
            "proof_attempt_policy": self.proof_attempt_policy,
            "fallback_policy": self.fallback_policy,
        })
    }
}

pub fn parse_task_intent(task: &str) -> TaskIntent {
    let prompt_seed_extraction = extract_prompt_seed_provenance(task);
    let mut exact_seeds = Vec::new();
    let mut text_seeds = Vec::new();
    let mut file_path_seeds = Vec::new();
    let mut config_keys = Vec::new();
    let mut ignored_terms = Vec::new();

    for seed in &prompt_seed_extraction.seeds {
        match seed.kind {
            PromptSeedKind::Symbol | PromptSeedKind::TestName | PromptSeedKind::Identifier => {
                exact_seeds.push(seed.value.clone());
            }
            PromptSeedKind::FilePath
            | PromptSeedKind::LineNumber
            | PromptSeedKind::StackTrace
            | PromptSeedKind::FilePattern
            | PromptSeedKind::PathToken => {
                if let Some(value) = seed.exact_value() {
                    file_path_seeds.push(value);
                }
            }
            PromptSeedKind::ConfigToken => {
                config_keys.push(seed.value.clone());
            }
            PromptSeedKind::TextToken | PromptSeedKind::ErrorMessage => {
                text_seeds.push(seed.value.clone());
            }
            PromptSeedKind::TaskVerbIgnored => {
                ignored_terms.push(seed.value.to_ascii_lowercase());
            }
            PromptSeedKind::Unknown => {}
        }
    }

    ignored_terms.extend(routing_ignored_terms_from_text(task));
    let exact_seeds = unique_task_strings(exact_seeds);
    let text_seeds = unique_task_strings(text_seeds);
    let file_path_seeds = unique_task_strings(file_path_seeds);
    let config_keys = unique_task_strings(config_keys);
    let ignored_terms = unique_task_strings(ignored_terms);

    let mut scores = BTreeMap::<TaskKind, i32>::new();
    let mut signals = Vec::<String>::new();
    score_task_signals(task, &mut scores, &mut signals);

    let (task_kind, confidence, ambiguity, why_this_intent, fallback_intent) = select_task_kind(
        task,
        &scores,
        &signals,
        &exact_seeds,
        &file_path_seeds,
        &config_keys,
    );
    let selected_profile = profile_id_for_task_kind(task_kind).to_string();
    let domain = task_domain_for_kind(task_kind).to_string();
    let relation_goals = relation_goals_for_task_kind(task_kind);
    let evidence_expectation = evidence_expectation_for_task_kind(task_kind).to_string();

    TaskIntent {
        task_kind,
        domain,
        confidence,
        selected_profile,
        signals: unique_task_strings(signals),
        ignored_terms,
        exact_seeds,
        text_seeds,
        file_path_seeds,
        config_keys,
        relation_goals,
        evidence_expectation,
        ambiguity,
        why_this_intent,
        fallback_intent,
    }
}

pub fn task_profile_registry() -> Vec<TaskProfile> {
    [
        "build_system_package_authoring",
        "codegraph_internal_debug",
        "implementation_trace",
        "storage_accounting_trace",
        "artifact_math_trace",
        "persistence_path_trace",
        "indexing_summary_trace",
        "schema_view_trace",
        "benchmark_metric_trace",
        "test_impact",
        "dataflow_trace",
        "security_review",
        "docs_lookup",
        "unknown_fallback",
    ]
    .into_iter()
    .map(task_profile_by_id)
    .collect()
}

pub fn select_task_profile(intent: &TaskIntent) -> TaskProfile {
    task_profile_by_id(&intent.selected_profile)
}

pub fn build_retrieval_plan(intent: &TaskIntent, profile: &TaskProfile) -> RetrievalPlan {
    let query_atoms = retrieval_atoms_for_profile(intent, profile);
    let max_candidates = query_atoms
        .iter()
        .map(|atom| atom.max_candidates)
        .sum::<usize>()
        .min(32);
    RetrievalPlan {
        plan_id: format!("retrieval_plan_{}_v1", profile.profile_id),
        task_intent_id: intent.task_intent_id(),
        profile_id: profile.profile_id.clone(),
        candidate_branches: profile.retrieval_branches.clone(),
        role_budget: profile.role_budget,
        proof_attempt_policy: proof_attempt_policy_for_intent(intent),
        fallback_policy: profile.fallback_policy.clone(),
        max_candidates,
        max_files: 8,
        max_snippets: 8,
        max_output_bytes: 65_536,
        explain_level: "structured_summary".to_string(),
        query_atoms,
    }
}

pub fn plan_task_retrieval(task: &str) -> (TaskIntent, TaskProfile, RetrievalPlan) {
    let intent = parse_task_intent(task);
    let profile = select_task_profile(&intent);
    let plan = build_retrieval_plan(&intent, &profile);
    (intent, profile, plan)
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
                let role = classify_edge_evidence_role(&step.edge);
                serde_json::json!({
                    "edge_id": step.edge.id,
                    "head_id": step.edge.head_id,
                    "relation": step.edge.relation.to_string(),
                    "tail_id": step.edge.tail_id,
                    "exactness": step.edge.exactness.to_string(),
                    "confidence": step.edge.confidence,
                    "extractor": step.edge.extractor,
                    "edge_class": fact_class.as_str(),
                    "context": infer_edge_context(&step.edge).as_str(),
                    "derived": step.edge.derived,
                    "provenance_edges": step.edge.provenance_edges.clone(),
                    "source_span": step.edge.source_span.to_string(),
                    "source_span_detail": step.edge.source_span,
                    "file_hash": step.edge.file_hash,
                    "fact_class": fact_class.as_str(),
                    "proof_grade_edge_class": fact_class_is_proof_eligible(&step.edge, fact_class),
                    "evidence_role": role.role.as_str(),
                    "classification_reason": role.reason,
                    "classification_source": role.classification_source,
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
            .map(|step| {
                classify_edge_evidence_role(&step.edge)
                    .role
                    .as_str()
                    .to_string()
            })
            .collect::<Vec<_>>();
        let mut metadata = Metadata::new();
        let path_context = path.path_context();
        let evidence_role = combine_evidence_roles(
            path.steps
                .iter()
                .map(|step| classify_edge_evidence_role(&step.edge).role),
        );
        let classification_sources = path
            .steps
            .iter()
            .map(|step| classify_edge_evidence_role(&step.edge).classification_source)
            .collect::<Vec<_>>();
        let classification_reasons = path
            .steps
            .iter()
            .map(|step| classify_edge_evidence_role(&step.edge).reason)
            .collect::<Vec<_>>();
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
            "evidence_role".to_string(),
            serde_json::json!(evidence_role.as_str()),
        );
        metadata.insert(
            "classification_source".to_string(),
            serde_json::json!(if classification_sources.is_empty() {
                "empty_path".to_string()
            } else {
                classification_sources.join("+")
            }),
        );
        metadata.insert(
            "classification_reason".to_string(),
            serde_json::json!(if classification_reasons.is_empty() {
                "empty path".to_string()
            } else {
                classification_reasons.join("; ")
            }),
        );
        metadata.insert(
            "production_proof_eligible".to_string(),
            serde_json::json!(
                evidence_role.is_production() && validate_proof_path_edge_classes(path).is_ok()
            ),
        );
        metadata.insert(
            "proof_scope".to_string(),
            serde_json::json!(if evidence_role.is_production() {
                "production"
            } else if evidence_role == EvidenceRole::Unknown {
                "unknown"
            } else {
                "test_or_mock"
            }),
        );
        metadata.insert(
            "path_length".to_string(),
            serde_json::json!(path.steps.len()),
        );
        if path.steps.len() == 1 {
            metadata.insert(
                "hydration".to_string(),
                serde_json::json!("demand_driven_one_hop"),
            );
            metadata.insert(
                "single_seed_relation_hydration".to_string(),
                serde_json::json!(true),
            );
        }
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
        let prompt_seed_extraction = extract_prompt_seed_provenance(&request.task);
        let prompt_intent = prompt_seed_extraction.intent;
        let prompt_seeds = prompt_seed_extraction.seeds;
        let exact_seed_values = prompt_seeds
            .iter()
            .filter_map(PromptSeed::exact_value)
            .collect::<Vec<_>>();
        let policy = TraversalPolicy::for_mode(&request.mode);
        let mut candidate_seeds = merge_seed_values(
            request
                .seeds
                .iter()
                .chain(exact_seed_values.iter())
                .chain(request.stage0_candidates.iter()),
        );
        let candidate_seed_count_before_cap = candidate_seeds.len();
        candidate_seeds.truncate(policy.max_candidate_seeds);
        let candidate_seed_count_after_cap = candidate_seeds.len();
        let limits = context_pack_query_limits_for_policy(policy);
        let mut paths = Vec::new();
        let mut traversal_runs = Vec::new();
        for seed in &candidate_seeds {
            let (seed_paths, seed_telemetry) =
                self.context_paths_for_seed_with_policy_telemetry(seed, limits, policy);
            paths.extend(seed_paths);
            traversal_runs.extend(seed_telemetry);
        }
        let candidate_path_count_before_dedup = paths.len();
        paths = unique_paths(paths);
        let candidate_path_count_after_dedup = paths.len();
        let (split_paths, split_mixed_path_count) =
            split_mixed_paths_for_context_mode(paths, &request.mode);
        paths = split_paths;
        let path_context_counts_before = path_context_counts(&paths);
        let filtered_test_mock_path_count = paths
            .iter()
            .filter(|path| !path_allowed_for_context_mode(path, &request.mode))
            .count();
        paths.retain(|path| path_allowed_for_context_mode(path, &request.mode));
        let source_role_blocked_edge_ids = traversal_runs
            .iter()
            .flat_map(|run| run.source_role_blocked_edge_ids.iter().cloned())
            .collect::<BTreeSet<_>>();
        let source_role_blocked_edge_count = source_role_blocked_edge_ids.len();
        let rejected_test_mock_path_count =
            filtered_test_mock_path_count + source_role_blocked_edge_count;
        let candidate_path_count_after_filter = paths.len();
        let path_context_counts_after = path_context_counts(&paths);
        rank_context_packet_paths(&mut paths);
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
            "prompt_intent".to_string(),
            serde_json::json!(prompt_intent.as_str()),
        );
        metadata.insert(
            "prompt_seed_provenance".to_string(),
            serde_json::json!(prompt_seeds
                .iter()
                .map(PromptSeed::provenance_json)
                .collect::<Vec<_>>()),
        );
        metadata.insert(
            "prompt_seeds".to_string(),
            serde_json::json!(prompt_seeds
                .iter()
                .map(PromptSeed::provenance_json)
                .collect::<Vec<_>>()),
        );
        metadata.insert(
            "exact_seed_count".to_string(),
            serde_json::json!(candidate_seeds.len()),
        );
        metadata.insert(
            "candidate_seed_count_before_cap".to_string(),
            serde_json::json!(candidate_seed_count_before_cap),
        );
        metadata.insert(
            "candidate_seed_count_after_cap".to_string(),
            serde_json::json!(candidate_seed_count_after_cap),
        );
        metadata.insert(
            "candidate_seed_cap".to_string(),
            serde_json::json!(policy.max_candidate_seeds),
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
            "split_mixed_path_count".to_string(),
            serde_json::json!(split_mixed_path_count),
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
        metadata.insert(
            "source_role_blocked_edge_count".to_string(),
            serde_json::json!(source_role_blocked_edge_count),
        );
        metadata.insert(
            "traversal_telemetry".to_string(),
            serde_json::json!({
                "schema_version": 1,
                "diagnostic_only": true,
                "measurement_scope": "exact_graph_query_engine_context_pack",
                "aggregate": aggregate_graph_traversal_telemetry_json(&traversal_runs),
                "runs_omitted_by_default": true,
                "run_count": traversal_runs.len(),
            }),
        );

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
        self.find_event_flow_with_policy_telemetry(
            entity_id,
            limits,
            TraversalPolicy::debug_audit(),
        )
        .0
    }

    fn find_event_flow_with_policy_telemetry(
        &self,
        entity_id: &str,
        limits: QueryLimits,
        policy: TraversalPolicy,
    ) -> (Vec<GraphPath>, GraphTraversalTelemetry) {
        let start = Instant::now();
        let traversals = [
            Traversal::forward(RelationKind::Publishes),
            Traversal::forward(RelationKind::Emits),
            Traversal::reverse(RelationKind::Consumes),
            Traversal::reverse(RelationKind::ListensTo),
            Traversal::reverse(RelationKind::SubscribesTo),
            Traversal::forward(RelationKind::Calls),
        ];
        let mut telemetry = GraphTraversalTelemetry::new_with_policy(
            "find_event_flow",
            entity_id,
            &traversals,
            limits,
            policy,
        );
        let mut results = Vec::new();

        for publish_step in self.neighbors_with_policy_telemetry(
            entity_id,
            &[
                Traversal::forward(RelationKind::Publishes),
                Traversal::forward(RelationKind::Emits),
            ],
            &mut telemetry,
            policy,
        ) {
            if traversal_timed_out(start, policy) {
                telemetry.note_budget_stop("timeout_ms");
                let sorted = sorted_paths(results);
                return (sorted.clone(), telemetry.finish(start, sorted.len()));
            }
            if telemetry.edges_visited >= limits.max_edges_visited {
                telemetry.note_budget_stop("max_edge_visits");
                let sorted = sorted_paths(results);
                return (sorted.clone(), telemetry.finish(start, sorted.len()));
            }
            telemetry.record_edge_visit(&publish_step.edge);

            let event_node = publish_step.to.clone();
            for consumer_step in self.neighbors_with_policy_telemetry(
                &event_node,
                &[
                    Traversal::reverse(RelationKind::Consumes),
                    Traversal::reverse(RelationKind::ListensTo),
                    Traversal::reverse(RelationKind::SubscribesTo),
                ],
                &mut telemetry,
                policy,
            ) {
                if traversal_timed_out(start, policy) {
                    telemetry.note_budget_stop("timeout_ms");
                    let sorted = sorted_paths(results);
                    return (sorted.clone(), telemetry.finish(start, sorted.len()));
                }
                if telemetry.edges_visited >= limits.max_edges_visited {
                    telemetry.note_budget_stop("max_edge_visits");
                    let sorted = sorted_paths(results);
                    return (sorted.clone(), telemetry.finish(start, sorted.len()));
                }
                telemetry.record_edge_visit(&consumer_step.edge);

                let base =
                    self.path_from_steps(entity_id, vec![publish_step.clone(), consumer_step]);
                if base.steps.len() > limits.max_depth {
                    telemetry.note_depth_limit();
                    continue;
                }
                results.push(base.clone());
                if results.len() >= limits.max_paths {
                    telemetry.note_budget_stop("max_paths");
                    let sorted = sorted_paths(results);
                    return (sorted.clone(), telemetry.finish(start, sorted.len()));
                }

                let remaining = limits.max_depth.saturating_sub(base.steps.len());
                if remaining == 0 {
                    continue;
                }

                let mut call_limits = limits;
                call_limits.max_depth = remaining;
                call_limits.max_paths = limits.max_paths.saturating_sub(results.len());
                call_limits.max_edges_visited = limits
                    .max_edges_visited
                    .saturating_sub(telemetry.edges_visited);
                let (extensions, extension_telemetry) = self
                    .k_shortest_matching_with_policy_telemetry(
                        &base.target,
                        &[Traversal::forward(RelationKind::Calls)],
                        call_limits,
                        policy,
                        &|path| !path.steps.is_empty(),
                    );
                telemetry.absorb_child_traversal(&extension_telemetry);
                for extension in extensions {
                    let mut steps = base.steps.clone();
                    steps.extend(extension.steps);
                    results.push(self.path_from_steps(entity_id, steps));
                    if results.len() >= limits.max_paths {
                        telemetry.note_budget_stop("max_paths");
                        let sorted = sorted_paths(results);
                        return (sorted.clone(), telemetry.finish(start, sorted.len()));
                    }
                }
                if telemetry.edges_visited >= limits.max_edges_visited {
                    telemetry.note_budget_stop("max_edge_visits");
                    let sorted = sorted_paths(results);
                    return (sorted.clone(), telemetry.finish(start, sorted.len()));
                }
            }
        }

        let sorted = sorted_paths(results);
        (sorted.clone(), telemetry.finish(start, sorted.len()))
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
        self.context_paths_for_seed_with_telemetry(seed, limits).0
    }

    fn context_paths_for_seed_with_telemetry(
        &self,
        seed: &str,
        limits: QueryLimits,
    ) -> (Vec<GraphPath>, Vec<GraphTraversalTelemetry>) {
        self.context_paths_for_seed_with_policy_telemetry(
            seed,
            limits,
            TraversalPolicy::debug_audit(),
        )
    }

    fn context_paths_for_seed_with_policy_telemetry(
        &self,
        seed: &str,
        limits: QueryLimits,
        policy: TraversalPolicy,
    ) -> (Vec<GraphPath>, Vec<GraphTraversalTelemetry>) {
        let mut paths = Vec::new();
        let mut telemetry = Vec::new();

        let (mut result, run) = self.k_shortest_matching_with_policy_telemetry_label(
            "find_mutations",
            seed,
            &[
                Traversal::forward(RelationKind::Calls),
                Traversal::forward(RelationKind::Writes),
                Traversal::forward(RelationKind::Mutates),
            ],
            limits,
            policy,
            &|path| {
                matches!(
                    path.last_relation(),
                    Some(RelationKind::Writes | RelationKind::Mutates)
                )
            },
        );
        paths.append(&mut result);
        telemetry.push(run);

        let (mut result, run) = self.bounded_bfs_with_policy_telemetry_label(
            "find_reads",
            seed,
            &[
                Traversal::forward(RelationKind::Reads),
                Traversal::reverse(RelationKind::Reads),
            ],
            limits,
            policy,
            &|path| path.last_relation() == Some(RelationKind::Reads),
        );
        paths.append(&mut result);
        telemetry.push(run);

        let (mut result, run) = self.bounded_bfs_with_policy_telemetry_label(
            "find_writes",
            seed,
            &[
                Traversal::forward(RelationKind::Writes),
                Traversal::reverse(RelationKind::Writes),
            ],
            limits,
            policy,
            &|path| path.last_relation() == Some(RelationKind::Writes),
        );
        paths.append(&mut result);
        telemetry.push(run);

        let (mut result, run) = self.k_shortest_matching_with_policy_telemetry_label(
            "find_dataflow",
            seed,
            &[
                Traversal::forward(RelationKind::FlowsTo),
                Traversal::reverse(RelationKind::AssignedFrom),
            ],
            limits,
            policy,
            &|path| !path.steps.is_empty(),
        );
        paths.append(&mut result);
        telemetry.push(run);

        let (mut result, run) = self.k_shortest_matching_with_policy_telemetry_label(
            "find_auth_paths",
            seed,
            &[
                Traversal::forward(RelationKind::Exposes),
                Traversal::forward(RelationKind::Calls),
                Traversal::forward(RelationKind::Authorizes),
                Traversal::forward(RelationKind::ChecksRole),
                Traversal::forward(RelationKind::ChecksPermission),
            ],
            limits,
            policy,
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
        );
        paths.append(&mut result);
        telemetry.push(run);

        let (mut result, run) = self.find_event_flow_with_policy_telemetry(seed, limits, policy);
        paths.append(&mut result);
        telemetry.push(run);

        let (mut result, run) = self.k_shortest_matching_with_policy_telemetry_label(
            "find_migrations",
            seed,
            &[
                Traversal::reverse(RelationKind::Migrates),
                Traversal::reverse(RelationKind::AltersColumn),
                Traversal::reverse(RelationKind::DependsOnSchema),
            ],
            limits,
            policy,
            &|path| !path.steps.is_empty(),
        );
        paths.append(&mut result);
        telemetry.push(run);

        let (mut result, run) = self.bounded_bfs_with_policy_telemetry_label(
            "find_tests",
            seed,
            &[
                Traversal::reverse(RelationKind::Tests),
                Traversal::reverse(RelationKind::Covers),
                Traversal::reverse(RelationKind::Asserts),
                Traversal::reverse(RelationKind::Mocks),
                Traversal::reverse(RelationKind::Stubs),
                Traversal::reverse(RelationKind::FixturesFor),
            ],
            limits,
            policy,
            &|path| !path.steps.is_empty(),
        );
        paths.append(&mut result);
        telemetry.push(run);

        let (mut result, run) = self.bounded_bfs_with_policy_telemetry_label(
            "find_callers",
            seed,
            &[Traversal::reverse(RelationKind::Calls)],
            limits,
            policy,
            &|path| path.last_relation() == Some(RelationKind::Calls),
        );
        paths.append(&mut result);
        telemetry.push(run);

        let (mut result, run) = self.bounded_bfs_with_policy_telemetry_label(
            "find_callees",
            seed,
            &[Traversal::forward(RelationKind::Calls)],
            limits,
            policy,
            &|path| path.last_relation() == Some(RelationKind::Calls),
        );
        paths.append(&mut result);
        telemetry.push(run);

        if paths.is_empty() {
            let one_hop_limits = QueryLimits {
                max_depth: 1,
                max_paths: limits.max_paths.max(1),
                max_edges_visited: limits.max_edges_visited,
            };
            let (mut result, run) = self.bounded_bfs_with_policy_telemetry_label(
                "single_seed_relation_hydration_fallback",
                seed,
                context_pack_one_hop_relation_hydration_traversals(),
                one_hop_limits,
                policy,
                &|path| !path.steps.is_empty(),
            );
            paths.append(&mut result);
            telemetry.push(run);
        }

        (sorted_paths(paths), telemetry)
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
        self.bounded_bfs_with_telemetry(source_id, traversals, limits, accept)
            .0
    }

    pub fn bounded_bfs_with_telemetry(
        &self,
        source_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
        accept: &impl Fn(&GraphPath) -> bool,
    ) -> (Vec<GraphPath>, GraphTraversalTelemetry) {
        self.bounded_bfs_with_telemetry_label("bounded_bfs", source_id, traversals, limits, accept)
    }

    pub fn bounded_bfs_with_policy_telemetry(
        &self,
        source_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
        policy: TraversalPolicy,
        accept: &impl Fn(&GraphPath) -> bool,
    ) -> (Vec<GraphPath>, GraphTraversalTelemetry) {
        self.bounded_bfs_with_policy_telemetry_label(
            "bounded_bfs",
            source_id,
            traversals,
            limits,
            policy,
            accept,
        )
    }

    fn bounded_bfs_with_telemetry_label(
        &self,
        operation: &str,
        source_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
        accept: &impl Fn(&GraphPath) -> bool,
    ) -> (Vec<GraphPath>, GraphTraversalTelemetry) {
        self.bounded_bfs_with_policy_telemetry_label(
            operation,
            source_id,
            traversals,
            limits,
            TraversalPolicy::debug_audit(),
            accept,
        )
    }

    fn bounded_bfs_with_policy_telemetry_label(
        &self,
        operation: &str,
        source_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
        policy: TraversalPolicy,
        accept: &impl Fn(&GraphPath) -> bool,
    ) -> (Vec<GraphPath>, GraphTraversalTelemetry) {
        let start = Instant::now();
        let mut telemetry = GraphTraversalTelemetry::new_with_policy(
            operation, source_id, traversals, limits, policy,
        );
        let mut queue = VecDeque::from([PathState::new(source_id)]);
        let mut results = Vec::new();
        let mut visited_nodes = BTreeSet::<String>::new();

        while let Some(state) = queue.pop_front() {
            if traversal_timed_out(start, policy) {
                telemetry.note_budget_stop("timeout_ms");
                let sorted = sorted_paths(results);
                return (sorted.clone(), telemetry.finish(start, sorted.len()));
            }
            visited_nodes.insert(state.node.clone());
            telemetry.nodes_visited = visited_nodes.len();
            if state.steps.len() >= limits.max_depth {
                telemetry.note_depth_limit();
                continue;
            }

            for step in self.neighbors_with_policy_telemetry(
                &state.node,
                traversals,
                &mut telemetry,
                policy,
            ) {
                if traversal_timed_out(start, policy) {
                    telemetry.note_budget_stop("timeout_ms");
                    let sorted = sorted_paths(results);
                    return (sorted.clone(), telemetry.finish(start, sorted.len()));
                }
                if telemetry.edges_visited >= limits.max_edges_visited {
                    telemetry.note_budget_stop("max_edge_visits");
                    let sorted = sorted_paths(results);
                    return (sorted.clone(), telemetry.finish(start, sorted.len()));
                }
                telemetry.record_edge_visit(&step.edge);
                if state.seen_nodes.contains(&step.to) {
                    telemetry.cycles_cut += 1;
                    continue;
                }

                let next = state.extend(step);
                let path = self.path_from_steps(source_id, next.steps.clone());
                if accept(&path) {
                    results.push(path);
                    if results.len() >= limits.max_paths {
                        telemetry.note_budget_stop("max_paths");
                        let sorted = sorted_paths(results);
                        return (sorted.clone(), telemetry.finish(start, sorted.len()));
                    }
                }
                queue.push_back(next);
            }
        }

        let sorted = sorted_paths(results);
        (sorted.clone(), telemetry.finish(start, sorted.len()))
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
        self.k_shortest_matching_with_telemetry(source_id, traversals, limits, accept)
            .0
    }

    pub fn k_shortest_matching_with_telemetry(
        &self,
        source_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
        accept: &impl Fn(&GraphPath) -> bool,
    ) -> (Vec<GraphPath>, GraphTraversalTelemetry) {
        self.k_shortest_matching_with_telemetry_label(
            "k_shortest_matching",
            source_id,
            traversals,
            limits,
            accept,
        )
    }

    pub fn k_shortest_matching_with_policy_telemetry(
        &self,
        source_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
        policy: TraversalPolicy,
        accept: &impl Fn(&GraphPath) -> bool,
    ) -> (Vec<GraphPath>, GraphTraversalTelemetry) {
        self.k_shortest_matching_with_policy_telemetry_label(
            "k_shortest_matching",
            source_id,
            traversals,
            limits,
            policy,
            accept,
        )
    }

    fn k_shortest_matching_with_telemetry_label(
        &self,
        operation: &str,
        source_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
        accept: &impl Fn(&GraphPath) -> bool,
    ) -> (Vec<GraphPath>, GraphTraversalTelemetry) {
        self.k_shortest_matching_with_policy_telemetry_label(
            operation,
            source_id,
            traversals,
            limits,
            TraversalPolicy::debug_audit(),
            accept,
        )
    }

    fn k_shortest_matching_with_policy_telemetry_label(
        &self,
        operation: &str,
        source_id: &str,
        traversals: &[Traversal],
        limits: QueryLimits,
        policy: TraversalPolicy,
        accept: &impl Fn(&GraphPath) -> bool,
    ) -> (Vec<GraphPath>, GraphTraversalTelemetry) {
        let start = Instant::now();
        let mut telemetry = GraphTraversalTelemetry::new_with_policy(
            operation, source_id, traversals, limits, policy,
        );
        let mut heap = BinaryHeap::new();
        let mut sequence = 0usize;
        heap.push(HeapState::new(sequence, PathState::new(source_id), 0.0));
        sequence += 1;

        let mut results = Vec::new();
        let mut visited_nodes = BTreeSet::<String>::new();

        while let Some(heap_state) = heap.pop() {
            if traversal_timed_out(start, policy) {
                telemetry.note_budget_stop("timeout_ms");
                let sorted = sorted_paths(results);
                return (sorted.clone(), telemetry.finish(start, sorted.len()));
            }
            let state = heap_state.path;
            let state_depth = state.steps.len();
            visited_nodes.insert(state.node.clone());
            telemetry.nodes_visited = visited_nodes.len();

            let path = self.path_from_steps(source_id, state.steps.clone());
            if !path.steps.is_empty() && accept(&path) {
                results.push(path);
                if results.len() >= limits.max_paths {
                    telemetry.note_budget_stop("max_paths");
                    let sorted = sorted_paths(results);
                    return (sorted.clone(), telemetry.finish(start, sorted.len()));
                }
            }

            if state_depth >= limits.max_depth {
                telemetry.note_depth_limit();
                continue;
            }

            for step in self.neighbors_with_policy_telemetry(
                &state.node,
                traversals,
                &mut telemetry,
                policy,
            ) {
                if traversal_timed_out(start, policy) {
                    telemetry.note_budget_stop("timeout_ms");
                    let sorted = sorted_paths(results);
                    return (sorted.clone(), telemetry.finish(start, sorted.len()));
                }
                if telemetry.edges_visited >= limits.max_edges_visited {
                    telemetry.note_budget_stop("max_edge_visits");
                    let sorted = sorted_paths(results);
                    return (sorted.clone(), telemetry.finish(start, sorted.len()));
                }
                telemetry.record_edge_visit(&step.edge);
                if state.seen_nodes.contains(&step.to) {
                    telemetry.cycles_cut += 1;
                    continue;
                }

                let added_cost = self.step_cost(&step, state_depth + 1);
                let next = state.extend(step);
                let next_cost = heap_state.cost + added_cost;
                heap.push(HeapState::new(sequence, next, next_cost));
                sequence += 1;
            }
        }

        let sorted = sorted_paths(results);
        (sorted.clone(), telemetry.finish(start, sorted.len()))
    }

    fn neighbors_with_policy_telemetry(
        &self,
        node_id: &str,
        traversals: &[Traversal],
        telemetry: &mut GraphTraversalTelemetry,
        policy: TraversalPolicy,
    ) -> Vec<TraversalStep> {
        let mut steps = Vec::new();
        let mut structural_expanded = 0usize;

        for traversal in traversals {
            match traversal.direction {
                TraversalDirection::Forward => {
                    if let Some(indices) = self.by_head.get(node_id) {
                        for index in indices {
                            let edge = &self.edges[*index];
                            if edge.relation == traversal.relation {
                                if self.edge_allowed_for_policy(
                                    edge,
                                    policy,
                                    &mut structural_expanded,
                                    telemetry,
                                ) {
                                    steps.push(TraversalStep {
                                        edge: edge.clone(),
                                        direction: TraversalDirection::Forward,
                                        from: edge.head_id.clone(),
                                        to: edge.tail_id.clone(),
                                    });
                                }
                            } else {
                                telemetry.relation_blocked_edges += 1;
                                if is_structural_relation(edge.relation) {
                                    telemetry.structural_edges_skipped += 1;
                                }
                                if is_heuristic_edge(edge) {
                                    telemetry.heuristic_edges_skipped += 1;
                                }
                            }
                        }
                    }
                }
                TraversalDirection::Reverse => {
                    if let Some(indices) = self.by_tail.get(node_id) {
                        for index in indices {
                            let edge = &self.edges[*index];
                            if edge.relation == traversal.relation {
                                if self.edge_allowed_for_policy(
                                    edge,
                                    policy,
                                    &mut structural_expanded,
                                    telemetry,
                                ) {
                                    steps.push(TraversalStep {
                                        edge: edge.clone(),
                                        direction: TraversalDirection::Reverse,
                                        from: edge.tail_id.clone(),
                                        to: edge.head_id.clone(),
                                    });
                                }
                            } else {
                                telemetry.relation_blocked_edges += 1;
                                if is_structural_relation(edge.relation) {
                                    telemetry.structural_edges_skipped += 1;
                                }
                                if is_heuristic_edge(edge) {
                                    telemetry.heuristic_edges_skipped += 1;
                                }
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
        if let Some(max_neighbors) = policy.max_neighbors_per_node {
            if steps.len() > max_neighbors {
                telemetry.neighbor_limit_hits += 1;
                telemetry.neighbors_omitted_by_limit += steps.len() - max_neighbors;
                telemetry.note_budget_stop("max_neighbors_per_node");
                steps.truncate(max_neighbors);
            }
        }
        telemetry.neighbors_expanded += steps.len();
        steps
    }

    fn edge_allowed_for_policy(
        &self,
        edge: &Edge,
        policy: TraversalPolicy,
        structural_expanded: &mut usize,
        telemetry: &mut GraphTraversalTelemetry,
    ) -> bool {
        if !policy.relation_allowed(edge.relation) {
            telemetry.relation_blocked_edges += 1;
            if test_traversal_relation_allowed(edge.relation) && !policy.mode.allows_test_mock() {
                telemetry.note_source_role_blocked(edge);
            }
            if is_structural_relation(edge.relation) {
                telemetry.structural_edges_skipped += 1;
            }
            if is_heuristic_edge(edge) {
                telemetry.heuristic_edges_seen += 1;
                telemetry.heuristic_edges_skipped += 1;
            }
            return false;
        }

        if is_structural_relation(edge.relation) {
            if let Some(max_structural) = policy.max_structural_expansion {
                if *structural_expanded >= max_structural {
                    telemetry.structural_edges_skipped += 1;
                    telemetry.structural_expansion_limit_hits += 1;
                    telemetry.note_budget_stop("max_structural_expansion");
                    return false;
                }
            }
            *structural_expanded += 1;
        }

        if is_heuristic_edge(edge) && !policy.heuristic_allowed() {
            telemetry.heuristic_edges_seen += 1;
            telemetry.heuristic_edges_skipped += 1;
            return false;
        }

        if edge.derived {
            telemetry.derived_edge_provenance_checks += 1;
            if edge.provenance_edges.is_empty() {
                telemetry.derived_edge_missing_provenance += 1;
                if !policy.derived_without_provenance_allowed() {
                    telemetry.derived_edge_provenance_blocked_edges += 1;
                    return false;
                }
            }
        }

        if !policy.source_role_allowed(edge) {
            telemetry.note_source_role_blocked(edge);
            return false;
        }

        true
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
        let prompt_seed_extraction = extract_prompt_seed_provenance(&request.task);
        let prompt_intent = prompt_seed_extraction.intent;
        let prompt_seeds = prompt_seed_extraction.seeds;
        let prompt_exact_seeds = prompt_seeds
            .iter()
            .filter_map(prompt_seed_graph_exact_value)
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

        let vector_branch_enabled =
            self.config.enable_vector_candidates_by_default || request.enable_vector_candidates;
        let mut vector_warnings = Vec::new();
        if (vector_branch_enabled || request.vector_candidate_diagnostics)
            && request.vector_branch_status != VectorCandidateBranchStatus::Ready
        {
            if let Some(warning) = request.vector_branch_status.warning() {
                vector_warnings.push(warning);
            }
        }
        let (vector_candidates, vector_dropped) = if vector_branch_enabled
            && request.vector_branch_status == VectorCandidateBranchStatus::Ready
        {
            selected_vector_candidates(
                &request.vector_candidates,
                self.config.vector_candidate_top_k,
                &request.task,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        for candidate in &vector_candidates {
            documents
                .entry(vector_candidate_stage_id(candidate))
                .or_insert_with(|| vector_candidate_document(candidate));
        }
        let vector_stage_ids = vector_candidates
            .iter()
            .map(vector_candidate_stage_id)
            .collect::<Vec<_>>();

        let mut binary_index = InMemoryBinaryVectorIndex::new(self.config.binary_dimensions)?;
        for document in documents.values() {
            binary_index.upsert_text(&document.id, &document.text)?;
        }

        let stage0_ids = merge_seed_values(
            exact_seed_ids
                .iter()
                .chain(request_stage0_ids.iter())
                .chain(vector_stage_ids.iter())
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
        trace.push(RetrievalTraceStage::new(
            "stage0_vector_semantic_candidates",
            vector_stage_ids.clone(),
            vector_dropped.clone(),
            vec![
                format!("enabled={vector_branch_enabled}"),
                format!("status={}", request.vector_branch_status.as_str()),
                format!("candidate_count={}", vector_candidates.len()),
                "vector candidates are candidate recall only, not graph proof".to_string(),
                "exact seeds are not subject to vector candidate caps".to_string(),
            ]
            .into_iter()
            .chain(vector_warnings.iter().cloned())
            .collect(),
        ));

        let query_signature =
            BinarySignature::from_text(&request.task, self.config.binary_dimensions)?;
        let binary_overfetch_k = self.config.binary_overfetch_k();
        let stage1_candidates = binary_index.search_with_exact_seeds(
            &query_signature,
            binary_overfetch_k,
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
            vec![
                "binary candidates are suggestions only".to_string(),
                format!("binary_overfetch_k={binary_overfetch_k}"),
                format!("stage2_final_cap={}", self.config.stage2_top_n),
                "final cap is applied after union and deterministic rerank".to_string(),
            ],
        ));

        let nuance_rescue_enabled =
            self.config.enable_nuance_rescue_by_default || request.enable_nuance_rescue_candidates;
        let stage1_id_set = stage1_ids.iter().cloned().collect::<BTreeSet<_>>();
        let (nuance_rescue_candidates, nuance_rescue_dropped) = if nuance_rescue_enabled {
            selected_nuance_rescue_candidates(
                &request.task,
                &documents,
                &stage1_id_set,
                &exact_seed_ids,
                self.config.nuance_rescue_top_k,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let nuance_stage_ids = nuance_rescue_candidates
            .iter()
            .map(|candidate| candidate.candidate_id.clone())
            .collect::<Vec<_>>();
        trace.push(RetrievalTraceStage::new(
            "stage1_nuance_rescue",
            nuance_stage_ids.clone(),
            nuance_rescue_dropped.clone(),
            vec![
                format!("enabled={nuance_rescue_enabled}"),
                format!("candidate_count={}", nuance_rescue_candidates.len()),
                "rare-token and identifier rescue is candidate recall only".to_string(),
                "rescued candidates still require graph/source verification".to_string(),
            ],
        ));
        let stage1_with_rescue_ids =
            merge_seed_values(stage1_ids.iter().chain(nuance_stage_ids.iter()));
        let stage1_similarity_by_id = stage1_candidates
            .iter()
            .map(|candidate| (candidate.id.clone(), candidate.clone()))
            .collect::<BTreeMap<_, _>>();
        let nuance_by_id = nuance_rescue_candidates
            .iter()
            .map(|candidate| (candidate.candidate_id.clone(), candidate.clone()))
            .collect::<BTreeMap<_, _>>();
        let nuance_ids = nuance_stage_ids.iter().cloned().collect::<BTreeSet<_>>();

        let rerank_candidates = stage1_with_rescue_ids
            .iter()
            .map(|candidate_id| {
                let document = documents
                    .get(candidate_id)
                    .cloned()
                    .unwrap_or_else(|| RetrievalDocument::new(candidate_id, candidate_id));
                let mut rerank = RerankCandidate::new(document.id.clone(), document.text.clone())
                    .stage0_score(document.stage0_score)
                    .exact_seed(exact_seed_ids.contains(&document.id));
                if let Some(candidate) = stage1_similarity_by_id.get(candidate_id) {
                    let exact_seed = candidate.exact_seed || rerank.exact_seed;
                    rerank = rerank.exact_seed(exact_seed);
                }
                if let Some(similarity) = stage1_similarity_by_id
                    .get(candidate_id)
                    .and_then(|candidate| candidate.similarity)
                {
                    rerank = rerank.stage1_similarity(similarity);
                }
                rerank.metadata = document.metadata;
                if self.engine.by_head.contains_key(candidate_id)
                    || self.engine.by_tail.contains_key(candidate_id)
                {
                    rerank.metadata.insert(
                        "graph_verification_available".to_string(),
                        "true".to_string(),
                    );
                }
                if nuance_ids.contains(candidate_id) {
                    rerank
                        .metadata
                        .insert("candidate_source".to_string(), "nuance_rescue".to_string());
                    rerank
                        .metadata
                        .insert("rare_token_match".to_string(), "true".to_string());
                    rerank
                        .metadata
                        .insert("identifier_signature_match".to_string(), "true".to_string());
                    if let Some(candidate) = nuance_by_id.get(candidate_id) {
                        if !candidate.matched_seeds.is_empty() {
                            rerank.metadata.insert(
                                "matched_tokens".to_string(),
                                candidate.matched_seeds.join(" "),
                            );
                        }
                        if let Some(rescue_basis) = candidate
                            .metadata
                            .get("rescue_basis")
                            .and_then(serde_json::Value::as_str)
                        {
                            rerank
                                .metadata
                                .insert("rescue_basis".to_string(), rescue_basis.to_string());
                        }
                    }
                }
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
        let stage2_dropped = dropped_ids(&stage1_with_rescue_ids, &stage2_ids, &exact_seed_ids);
        trace.push(RetrievalTraceStage::new(
            "stage2_compressed_rerank",
            stage2_ids.clone(),
            stage2_dropped,
            vec![
                "deterministic local reranker returns candidates for graph verification"
                    .to_string(),
                "exact seeds are preserved even if top-N is small".to_string(),
                format!("rerank_input_count={}", rerank_candidates.len()),
                format!("rerank_output_count={}", rerank_scores.len()),
                format!(
                    "reasons={}",
                    rerank_reason_summary(&rerank_scores).join(",")
                ),
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
        metadata.insert(
            "prompt_intent".to_string(),
            serde_json::json!(prompt_intent.as_str()),
        );
        metadata.insert(
            "prompt_seed_provenance".to_string(),
            serde_json::json!(prompt_seeds
                .iter()
                .map(PromptSeed::provenance_json)
                .collect::<Vec<_>>()),
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
        metadata.insert(
            "candidate_counts_by_source".to_string(),
            vector_candidate_counts_json(
                exact_seed_ids.len(),
                request_stage0_ids.len(),
                vector_candidates.len(),
                nuance_rescue_candidates.len(),
            ),
        );
        if !vector_warnings.is_empty() {
            metadata.insert(
                "vector_candidate_warnings".to_string(),
                serde_json::json!(vector_warnings),
            );
        }
        let mut packet = self.engine.context_packet_from_paths(ContextPacketBuild {
            task: request.task.clone(),
            mode: request.mode,
            token_budget: request.token_budget,
            symbols: verified_symbols.clone(),
            paths: &verified_paths,
            sources: &request.sources,
            metadata,
        });

        if packet.verified_paths.is_empty() {
            let fallback_snippets = vector_candidates
                .iter()
                .filter_map(vector_text_fallback_snippet)
                .take(4)
                .collect::<Vec<_>>();
            if !fallback_snippets.is_empty() {
                packet.snippets.extend(fallback_snippets);
                packet.risks.push(
                    "vector_text_evidence_candidate_returned_without_graph_proof".to_string(),
                );
                packet.metadata.insert(
                    "no_proof_fallback_reason".to_string(),
                    serde_json::json!(
                        "vector text-evidence candidate available after graph verification found no proof path"
                    ),
                );
                packet
                    .metadata
                    .insert("graph_proof".to_string(), serde_json::json!(false));
                packet.metadata.insert(
                    "proof_status".to_string(),
                    serde_json::json!("no_proof_path_found"),
                );
            }
        }

        if request.vector_candidate_diagnostics {
            let no_proof_fallback_reason = packet
                .metadata
                .get("no_proof_fallback_reason")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            packet.metadata.insert(
                "vector_candidate_trace".to_string(),
                vector_candidate_trace_json(
                    vector_branch_enabled,
                    &request.vector_branch_status,
                    &request.task,
                    &request.vector_candidates,
                    &vector_candidates,
                    &vector_dropped,
                    &vector_warnings,
                    &verified_symbols,
                    no_proof_fallback_reason.as_deref(),
                ),
            );
        }

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
            vector_candidates,
            nuance_rescue_candidates,
            vector_warnings,
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

fn seed_exactness(kind: PromptSeedKind, exact: bool) -> PromptSeedExactness {
    if matches!(kind, PromptSeedKind::TaskVerbIgnored) {
        PromptSeedExactness::Ignored
    } else if exact
        && matches!(
            kind,
            PromptSeedKind::TextToken | PromptSeedKind::ErrorMessage
        )
    {
        PromptSeedExactness::LiteralText
    } else if exact {
        PromptSeedExactness::Exact
    } else if matches!(kind, PromptSeedKind::Unknown) {
        PromptSeedExactness::Unknown
    } else {
        PromptSeedExactness::Heuristic
    }
}

fn ignored_reason_for_seed(kind: PromptSeedKind, value: &str) -> Option<String> {
    if matches!(kind, PromptSeedKind::TaskVerbIgnored) {
        Some(format!(
            "generic task verb `{}` is an intent signal, not an exact symbol",
            value
        ))
    } else {
        None
    }
}

fn intent_contribution_for_seed(kind: PromptSeedKind, value: &str) -> PromptIntent {
    let lower = value.to_ascii_lowercase();
    match kind {
        PromptSeedKind::Symbol | PromptSeedKind::Identifier => PromptIntent::EntityLookup,
        PromptSeedKind::FilePath => {
            if lower.ends_with(".adoc") || lower.contains("/docs/") || lower.starts_with("docs/") {
                PromptIntent::DocsText
            } else if lower.contains("package/") && lower.ends_with(".mk") {
                PromptIntent::BuildSystemPlanning
            } else {
                PromptIntent::FileLookup
            }
        }
        PromptSeedKind::LineNumber | PromptSeedKind::StackTrace | PromptSeedKind::ErrorMessage => {
            PromptIntent::BehaviorTrace
        }
        PromptSeedKind::TestName => PromptIntent::TestImpact,
        PromptSeedKind::ConfigToken => PromptIntent::TextConfigLookup,
        PromptSeedKind::FilePattern => {
            if matches!(lower.as_str(), ".mk" | "*.mk" | "mk") {
                PromptIntent::BuildSystemPlanning
            } else if matches!(lower.as_str(), ".adoc" | "*.adoc" | "adoc") {
                PromptIntent::DocsText
            } else {
                PromptIntent::FileLookup
            }
        }
        PromptSeedKind::PathToken => {
            if lower.starts_with("support/scripts")
                || lower.starts_with("support/download")
                || lower.contains("package/")
            {
                PromptIntent::BuildSystemPlanning
            } else {
                PromptIntent::FileLookup
            }
        }
        PromptSeedKind::TextToken => {
            if contains_any(
                &lower,
                &[
                    "generic-package",
                    "host-generic-package",
                    "package infrastructure",
                    "dependencies",
                    "license",
                    "version",
                    "source url",
                ],
            ) {
                PromptIntent::BuildSystemPlanning
            } else if contains_any(&lower, &["depends on", "select"]) {
                PromptIntent::TextConfigLookup
            } else if contains_any(&lower, &["docs", ".adoc", "manual"]) {
                PromptIntent::DocsText
            } else {
                PromptIntent::TextConfigLookup
            }
        }
        PromptSeedKind::TaskVerbIgnored => match lower.as_str() {
            "trace" => PromptIntent::BehaviorTrace,
            "find" | "plan" | "inspect" | "add" | "where" | "how" => PromptIntent::Unknown,
            _ => PromptIntent::Unknown,
        },
        PromptSeedKind::Unknown => PromptIntent::Unknown,
    }
}

fn classify_prompt_intent_from_seeds(prompt: &str, seeds: &[PromptSeed]) -> PromptIntent {
    let lower = prompt.to_ascii_lowercase();
    let has_seed_kind = |kind: PromptSeedKind| seeds.iter().any(|seed| seed.kind == kind);
    let has_contribution =
        |intent: PromptIntent| seeds.iter().any(|seed| seed.intent_contribution == intent);

    if contains_word_any(
        &lower,
        &[
            "auth",
            "authorization",
            "authorize",
            "permission",
            "role",
            "security",
            "sanitize",
            "validator",
            "policy",
        ],
    ) || contains_any(
        &lower,
        &[
            "permission check",
            "role check",
            "auth check",
            "security review",
        ],
    ) {
        return PromptIntent::SecurityAuthTrace;
    }
    if contains_any(
        &lower,
        &[
            "dataflow",
            "data flow",
            "taint",
            "source to sink",
            "flow from",
            "flows to",
        ],
    ) {
        return PromptIntent::DataflowTrace;
    }
    if contains_any(
        &lower,
        &[
            "caller",
            "callers",
            "callee",
            "callees",
            "called by",
            "calls into",
        ],
    ) {
        return PromptIntent::CallerCalleeTrace;
    }
    if has_seed_kind(PromptSeedKind::TestName)
        || contains_any(
            &lower,
            &[
                "test impact",
                "failing test",
                "failed test",
                "regression test",
                "unit test",
            ],
        )
        || contains_word_any(&lower, &["test", "tests", "spec", "specs"])
    {
        return PromptIntent::TestImpact;
    }
    if has_contribution(PromptIntent::BuildSystemPlanning)
        || (contains_any(&lower, &["buildroot", "package"])
            && contains_any(
                &lower,
                &[
                    "plan",
                    "add",
                    "new package",
                    ".mk",
                    "generic-package",
                    "host-generic-package",
                    "package infrastructure",
                ],
            ))
    {
        return PromptIntent::BuildSystemPlanning;
    }
    if has_contribution(PromptIntent::TextConfigLookup)
        || contains_any(
            &lower,
            &[
                "br2_package_",
                "config.in",
                "kconfig",
                "depends on",
                "select",
            ],
        )
    {
        return PromptIntent::TextConfigLookup;
    }
    if has_contribution(PromptIntent::DocsText)
        || contains_any(
            &lower,
            &["docs", "documentation", "manual", ".adoc", "asciidoc"],
        )
    {
        return PromptIntent::DocsText;
    }
    if contains_any(
        &lower,
        &[
            "trace",
            "behavior",
            "runtime path",
            "execution path",
            "impact",
        ],
    ) || has_seed_kind(PromptSeedKind::StackTrace)
        || has_seed_kind(PromptSeedKind::ErrorMessage)
    {
        return PromptIntent::BehaviorTrace;
    }
    if has_seed_kind(PromptSeedKind::FilePath) || has_seed_kind(PromptSeedKind::PathToken) {
        return PromptIntent::FileLookup;
    }
    if has_seed_kind(PromptSeedKind::Symbol) || has_seed_kind(PromptSeedKind::Identifier) {
        return PromptIntent::EntityLookup;
    }
    PromptIntent::Unknown
}

fn contains_word_any(haystack: &str, words: &[&str]) -> bool {
    haystack
        .split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
        .any(|part| words.contains(&part))
}

fn routing_ignored_terms_from_text(task: &str) -> Vec<String> {
    task.split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
        .filter_map(|part| {
            let lower = part.to_ascii_lowercase();
            if is_prompt_task_verb(&lower) {
                Some(lower)
            } else {
                None
            }
        })
        .collect()
}

fn unique_task_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            unique.push(trimmed.to_string());
        }
    }
    unique
}

fn add_task_signal(
    scores: &mut BTreeMap<TaskKind, i32>,
    signals: &mut Vec<String>,
    signal: &str,
    weights: &[(TaskKind, i32)],
) {
    signals.push(signal.to_string());
    for (kind, weight) in weights {
        *scores.entry(*kind).or_default() += *weight;
    }
}

fn score_contains(
    lower: &str,
    scores: &mut BTreeMap<TaskKind, i32>,
    signals: &mut Vec<String>,
    needle: &str,
    signal: &str,
    weights: &[(TaskKind, i32)],
) {
    if lower.contains(needle) {
        add_task_signal(scores, signals, signal, weights);
    }
}

fn score_word(
    lower: &str,
    scores: &mut BTreeMap<TaskKind, i32>,
    signals: &mut Vec<String>,
    word: &str,
    signal: &str,
    weights: &[(TaskKind, i32)],
) {
    if contains_word_any(lower, &[word]) {
        add_task_signal(scores, signals, signal, weights);
    }
}

fn score_task_signals(task: &str, scores: &mut BTreeMap<TaskKind, i32>, signals: &mut Vec<String>) {
    let lower = task.to_ascii_lowercase();

    score_contains(
        &lower,
        scores,
        signals,
        "buildroot",
        "buildroot:Buildroot",
        &[(TaskKind::BuildSystemPackageAuthoring, 4)],
    );
    score_word(
        &lower,
        scores,
        signals,
        "package",
        "buildroot:package",
        &[(TaskKind::BuildSystemPackageAuthoring, 1)],
    );
    score_contains(
        &lower,
        scores,
        signals,
        "generic-package",
        "buildroot:generic-package",
        &[(TaskKind::BuildSystemPackageAuthoring, 4)],
    );
    score_contains(
        &lower,
        scores,
        signals,
        "generic package",
        "buildroot:generic-package",
        &[(TaskKind::BuildSystemPackageAuthoring, 4)],
    );
    score_contains(
        &lower,
        scores,
        signals,
        "config.in",
        "buildroot:Config.in",
        &[(TaskKind::BuildSystemPackageAuthoring, 3)],
    );
    if lower.contains(".mk") {
        add_task_signal(
            scores,
            signals,
            "buildroot:.mk",
            &[(TaskKind::BuildSystemPackageAuthoring, 2)],
        );
    }
    score_contains(
        &lower,
        scores,
        signals,
        "br2_package",
        "buildroot:BR2_PACKAGE",
        &[(TaskKind::BuildSystemPackageAuthoring, 3)],
    );
    score_contains(
        &lower,
        scores,
        signals,
        "package infrastructure",
        "buildroot:package infrastructure",
        &[(TaskKind::BuildSystemPackageAuthoring, 2)],
    );
    for (word, signal, weight) in [
        ("download", "buildroot:download", 2),
        ("hash", "buildroot:hash", 1),
        ("license", "buildroot:license", 1),
        ("version", "buildroot:version", 1),
        ("dependencies", "buildroot:dependencies", 1),
    ] {
        score_word(
            &lower,
            scores,
            signals,
            word,
            signal,
            &[(TaskKind::BuildSystemPackageAuthoring, weight)],
        );
    }

    score_contains(
        &lower,
        scores,
        signals,
        "db lifecycle",
        "codegraph:DB lifecycle",
        &[(TaskKind::CodegraphInternalDebug, 5)],
    );
    for (needle, signal, weight) in [
        ("passport", "codegraph:passport", 3),
        ("preflight", "codegraph:preflight", 3),
        ("stale db", "codegraph:stale DB", 3),
        ("bundle import", "codegraph:bundle import", 2),
        ("context-pack", "codegraph:context-pack", 3),
    ] {
        score_contains(
            &lower,
            scores,
            signals,
            needle,
            signal,
            &[(TaskKind::CodegraphInternalDebug, weight)],
        );
    }
    for (word, signal, weight) in [
        ("codegraph", "codegraph:CodeGraph", 2),
        ("schema", "codegraph:schema", 2),
        ("scope", "codegraph:scope", 2),
        ("doctor", "codegraph:doctor", 3),
        ("status", "codegraph:status", 2),
        ("index", "codegraph:index", 2),
    ] {
        score_word(
            &lower,
            scores,
            signals,
            word,
            signal,
            &[
                (TaskKind::CodegraphInternalDebug, weight),
                (
                    if word == "schema" {
                        TaskKind::SchemaViewTrace
                    } else if word == "index" {
                        TaskKind::IndexingSummaryTrace
                    } else {
                        TaskKind::CodegraphInternalDebug
                    },
                    if matches!(word, "schema" | "index") {
                        2
                    } else {
                        0
                    },
                ),
            ],
        );
    }

    if lower.contains("trace implementation")
        || (lower.contains("implementation") && contains_word_any(&lower, &["trace"]))
    {
        add_task_signal(
            scores,
            signals,
            "implementation:trace implementation",
            &[(TaskKind::ImplementationTrace, 5)],
        );
    }
    for (needle, signal, weights) in [
        (
            "accounting",
            "accounting:accounting",
            vec![
                (TaskKind::StorageAccountingTrace, 4),
                (TaskKind::ArtifactMathTrace, 2),
                (TaskKind::ImplementationTrace, 1),
            ],
        ),
        (
            "persisted size",
            "accounting:persisted size",
            vec![
                (TaskKind::StorageAccountingTrace, 4),
                (TaskKind::PersistencePathTrace, 2),
            ],
        ),
        (
            "vector index",
            "accounting:vector index",
            vec![
                (TaskKind::StorageAccountingTrace, 3),
                (TaskKind::ArtifactMathTrace, 3),
                (TaskKind::ImplementationTrace, 2),
            ],
        ),
        (
            "chunk selection",
            "accounting:chunk selection",
            vec![
                (TaskKind::StorageAccountingTrace, 3),
                (TaskKind::ImplementationTrace, 2),
            ],
        ),
        (
            "artifact size",
            "accounting:artifact size",
            vec![
                (TaskKind::ArtifactMathTrace, 4),
                (TaskKind::StorageAccountingTrace, 2),
            ],
        ),
        (
            "payload size",
            "accounting:payload size",
            vec![
                (TaskKind::ArtifactMathTrace, 4),
                (TaskKind::StorageAccountingTrace, 2),
            ],
        ),
        (
            "metric labels",
            "accounting:metric labels",
            vec![
                (TaskKind::ArtifactMathTrace, 3),
                (TaskKind::BenchmarkMetricTrace, 2),
            ],
        ),
        (
            "storage audit",
            "accounting:storage audit",
            vec![
                (TaskKind::StorageAccountingTrace, 4),
                (TaskKind::ArtifactMathTrace, 2),
            ],
        ),
        (
            "summary counters",
            "accounting:summary counters",
            vec![
                (TaskKind::StorageAccountingTrace, 3),
                (TaskKind::IndexingSummaryTrace, 2),
            ],
        ),
    ] {
        score_contains(&lower, scores, signals, needle, signal, &weights);
    }
    if lower.contains("db size math") || (lower.contains("db size") && lower.contains("math")) {
        add_task_signal(
            scores,
            signals,
            "accounting:DB size math",
            &[
                (TaskKind::StorageAccountingTrace, 5),
                (TaskKind::ArtifactMathTrace, 2),
            ],
        );
    }
    if lower.contains("generated") && lower.contains("selected") && lower.contains("persisted") {
        add_task_signal(
            scores,
            signals,
            "accounting:generated/selected/persisted counts",
            &[
                (TaskKind::StorageAccountingTrace, 5),
                (TaskKind::ArtifactMathTrace, 2),
            ],
        );
    }
    score_word(
        &lower,
        scores,
        signals,
        "formula",
        "accounting:formula",
        &[(TaskKind::ArtifactMathTrace, 4)],
    );
    for (needle, signal) in [
        (
            "actual_index_file_bytes",
            "vector_metrics:actual_index_file_bytes",
        ),
        (
            "estimated_f32_payload_bytes",
            "vector_metrics:estimated_f32_payload_bytes",
        ),
        ("pretty_json", "vector_metrics:pretty_json"),
        (
            "vector_payload_compression",
            "vector_metrics:vector_payload_compression",
        ),
        ("stores_chunk_text", "vector_metrics:stores_chunk_text"),
        (
            "stores_chunk_metadata",
            "vector_metrics:stores_chunk_metadata",
        ),
        ("selected chunks", "vector_metrics:selected chunks"),
        ("diversity_ranked_v1", "vector_metrics:diversity_ranked_v1"),
        ("input_order_cap", "vector_metrics:input_order_cap"),
    ] {
        score_contains(
            &lower,
            scores,
            signals,
            needle,
            signal,
            &[
                (TaskKind::ArtifactMathTrace, 4),
                (TaskKind::StorageAccountingTrace, 3),
                (TaskKind::BenchmarkMetricTrace, 2),
            ],
        );
    }

    for (needle, signal, weights) in [
        (
            "test-impact",
            "test_impact:test-impact",
            vec![(TaskKind::TestImpact, 5)],
        ),
        (
            "test impact",
            "test_impact:test impact",
            vec![(TaskKind::TestImpact, 5)],
        ),
        (
            "failing test",
            "test_impact:failing test",
            vec![(TaskKind::TestImpact, 4)],
        ),
    ] {
        score_contains(&lower, scores, signals, needle, signal, &weights);
    }
    for (word, signal, weight) in [
        ("test", "test_impact:test", 1),
        ("spec", "test_impact:spec", 1),
        ("mock", "test_impact:mock", 2),
        ("assertion", "test_impact:assertion", 2),
        ("fixture", "test_impact:fixture", 2),
    ] {
        score_word(
            &lower,
            scores,
            signals,
            word,
            signal,
            &[(TaskKind::TestImpact, weight)],
        );
    }

    for (needle, signal, weights) in [
        (
            "dataflow",
            "dataflow:dataflow",
            vec![(TaskKind::DataflowTrace, 5)],
        ),
        (
            "data flow",
            "dataflow:data flow",
            vec![(TaskKind::DataflowTrace, 5)],
        ),
        (
            "request input",
            "dataflow:request input",
            vec![(TaskKind::DataflowTrace, 4)],
        ),
        (
            "database write",
            "dataflow:database write",
            vec![(TaskKind::DataflowTrace, 4)],
        ),
    ] {
        score_contains(&lower, scores, signals, needle, signal, &weights);
    }
    for (word, signal, weight) in [
        ("flow", "dataflow:flow", 1),
        ("source", "dataflow:source", 1),
        ("sink", "dataflow:sink", 2),
        ("sanitizer", "dataflow:sanitizer", 1),
        ("mutation", "dataflow:mutation", 2),
    ] {
        score_word(
            &lower,
            scores,
            signals,
            word,
            signal,
            &[(TaskKind::DataflowTrace, weight)],
        );
    }

    for (word, signal, weight) in [
        ("auth", "security:auth", 2),
        ("authorize", "security:authorize", 3),
        ("authorization", "security:authorization", 3),
        ("role", "security:role", 2),
        ("admin", "security:admin", 3),
        ("permission", "security:permission", 3),
        ("rbac", "security:RBAC", 3),
        ("checkrole", "security:checkRole", 3),
        ("sanitizer", "security:sanitizer", 1),
    ] {
        score_word(
            &lower,
            scores,
            signals,
            word,
            signal,
            &[(TaskKind::SecurityReview, weight)],
        );
    }

    for (word, signal, weight) in [
        ("docs", "docs:docs", 3),
        ("readme", "docs:README", 3),
        ("manual", "docs:manual", 3),
        ("guide", "docs:guide", 2),
        ("explain", "docs:explain", 1),
        ("explaining", "docs:explain", 1),
    ] {
        score_word(
            &lower,
            scores,
            signals,
            word,
            signal,
            &[(TaskKind::DocsLookup, weight)],
        );
    }
}

fn score_for(scores: &BTreeMap<TaskKind, i32>, kind: TaskKind) -> i32 {
    scores.get(&kind).copied().unwrap_or_default()
}

fn has_signal_prefix(signals: &[String], prefix: &str) -> bool {
    signals.iter().any(|signal| signal.starts_with(prefix))
}

fn has_signal(signals: &[String], needle: &str) -> bool {
    signals.iter().any(|signal| signal.contains(needle))
}

fn select_task_kind(
    task: &str,
    scores: &BTreeMap<TaskKind, i32>,
    signals: &[String],
    exact_seeds: &[String],
    file_path_seeds: &[String],
    config_keys: &[String],
) -> (TaskKind, f64, String, String, Option<String>) {
    let lower = task.to_ascii_lowercase();
    let top_score = scores.values().copied().max().unwrap_or_default();
    let second_score = {
        let mut values = scores.values().copied().collect::<Vec<_>>();
        values.sort_by(|left, right| right.cmp(left));
        values.get(1).copied().unwrap_or_default()
    };
    let has_vague_unknown = contains_any(
        &lower,
        &[
            "this thing",
            "what handles this",
            "figure out what",
            "handles this thing",
        ],
    );
    let buildroot_explicit = has_signal(signals, "Buildroot")
        || has_signal(signals, "generic-package")
        || has_signal(signals, "Config.in")
        || has_signal(signals, ".mk")
        || has_signal(signals, "BR2_PACKAGE");
    let vector_metric = has_signal_prefix(signals, "vector_metrics:")
        || has_signal(signals, "generated/selected/persisted")
        || has_signal(signals, "artifact size")
        || has_signal(signals, "payload size")
        || has_signal(signals, "DB size math");
    let docs_requested = score_for(scores, TaskKind::DocsLookup) >= 3;
    let test_strong = has_signal(signals, "test-impact")
        || has_signal(signals, "test impact")
        || has_signal(signals, "failing test")
        || ((lower.contains("change") || lower.contains("changing")) && lower.contains("test"));

    let selected = if has_vague_unknown || top_score == 0 {
        TaskKind::Unknown
    } else if docs_requested && !buildroot_explicit && score_for(scores, TaskKind::DocsLookup) >= 3
    {
        TaskKind::DocsLookup
    } else if buildroot_explicit && score_for(scores, TaskKind::BuildSystemPackageAuthoring) >= 4 {
        TaskKind::BuildSystemPackageAuthoring
    } else if vector_metric
        && (score_for(scores, TaskKind::ArtifactMathTrace) >= 4
            || score_for(scores, TaskKind::StorageAccountingTrace) >= 4)
    {
        if has_signal(signals, "actual_index_file_bytes")
            || has_signal(signals, "estimated_f32_payload_bytes")
            || has_signal(signals, "formula")
            || has_signal(signals, "artifact size")
            || has_signal(signals, "payload size")
        {
            TaskKind::ArtifactMathTrace
        } else {
            TaskKind::StorageAccountingTrace
        }
    } else if score_for(scores, TaskKind::CodegraphInternalDebug) >= 5 {
        TaskKind::CodegraphInternalDebug
    } else if test_strong || score_for(scores, TaskKind::TestImpact) >= 5 {
        TaskKind::TestImpact
    } else if score_for(scores, TaskKind::SecurityReview) >= 5
        && score_for(scores, TaskKind::SecurityReview) >= score_for(scores, TaskKind::DataflowTrace)
    {
        TaskKind::SecurityReview
    } else if score_for(scores, TaskKind::DataflowTrace) >= 5 {
        TaskKind::DataflowTrace
    } else if score_for(scores, TaskKind::StorageAccountingTrace) >= 5 {
        TaskKind::StorageAccountingTrace
    } else if score_for(scores, TaskKind::ArtifactMathTrace) >= 5 {
        TaskKind::ArtifactMathTrace
    } else if score_for(scores, TaskKind::ImplementationTrace) >= 4 {
        TaskKind::ImplementationTrace
    } else if score_for(scores, TaskKind::SchemaViewTrace) >= 4 {
        TaskKind::SchemaViewTrace
    } else if score_for(scores, TaskKind::IndexingSummaryTrace) >= 4 {
        TaskKind::IndexingSummaryTrace
    } else if score_for(scores, TaskKind::BenchmarkMetricTrace) >= 4 {
        TaskKind::BenchmarkMetricTrace
    } else if !file_path_seeds.is_empty() && top_score < 3 {
        TaskKind::FileLookup
    } else if (!exact_seeds.is_empty() || !config_keys.is_empty()) && top_score < 3 {
        TaskKind::EntityLookup
    } else {
        TaskKind::Unknown
    };

    let ambiguity = if selected == TaskKind::Unknown {
        "high".to_string()
    } else if second_score >= 3 && top_score.saturating_sub(second_score) <= 1 {
        "medium_conflicting_signals".to_string()
    } else {
        "low".to_string()
    };
    let confidence = if selected == TaskKind::Unknown {
        0.18
    } else {
        (0.42 + (top_score.min(9) as f64 * 0.055)
            - if ambiguity == "medium_conflicting_signals" {
                0.08
            } else {
                0.0
            })
        .clamp(0.42, 0.94)
    };
    let fallback_intent = if confidence < 0.50 || selected == TaskKind::Unknown {
        Some("unknown".to_string())
    } else {
        None
    };
    let signal_summary = if signals.is_empty() {
        "no scoped routing signals".to_string()
    } else {
        unique_task_strings(signals.to_vec())
            .into_iter()
            .take(5)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let why = format!(
        "I classified this as {} because I found {}.",
        selected.as_str(),
        signal_summary
    );
    (selected, confidence, ambiguity, why, fallback_intent)
}

fn profile_id_for_task_kind(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::BuildSystemPackageAuthoring => "build_system_package_authoring",
        TaskKind::CodegraphInternalDebug => "codegraph_internal_debug",
        TaskKind::ImplementationTrace => "implementation_trace",
        TaskKind::StorageAccountingTrace => "storage_accounting_trace",
        TaskKind::ArtifactMathTrace => "artifact_math_trace",
        TaskKind::PersistencePathTrace => "persistence_path_trace",
        TaskKind::IndexingSummaryTrace => "indexing_summary_trace",
        TaskKind::SchemaViewTrace => "schema_view_trace",
        TaskKind::BenchmarkMetricTrace => "benchmark_metric_trace",
        TaskKind::TestImpact => "test_impact",
        TaskKind::DataflowTrace => "dataflow_trace",
        TaskKind::SecurityReview => "security_review",
        TaskKind::DocsLookup => "docs_lookup",
        TaskKind::EntityLookup | TaskKind::FileLookup | TaskKind::Unknown => "unknown_fallback",
    }
}

fn task_domain_for_kind(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::BuildSystemPackageAuthoring => "build_system_package_authoring",
        TaskKind::CodegraphInternalDebug => "codegraph_internal",
        TaskKind::ImplementationTrace => "implementation",
        TaskKind::StorageAccountingTrace => "storage_accounting",
        TaskKind::ArtifactMathTrace => "artifact_math",
        TaskKind::PersistencePathTrace => "persistence_path",
        TaskKind::IndexingSummaryTrace => "indexing_summary",
        TaskKind::SchemaViewTrace => "schema_view",
        TaskKind::BenchmarkMetricTrace => "benchmark_metrics",
        TaskKind::TestImpact => "tests",
        TaskKind::DataflowTrace => "dataflow",
        TaskKind::SecurityReview => "security",
        TaskKind::DocsLookup => "docs",
        TaskKind::EntityLookup => "entity_lookup",
        TaskKind::FileLookup => "file_lookup",
        TaskKind::Unknown => "unknown",
    }
}

fn relation_goals_for_task_kind(kind: TaskKind) -> Vec<String> {
    let goals = match kind {
        TaskKind::BuildSystemPackageAuthoring => {
            vec!["source_text", "source_navigation", "package_wiring"]
        }
        TaskKind::CodegraphInternalDebug => {
            vec!["entrypoint", "lifecycle_guard", "store_open", "tests"]
        }
        TaskKind::ImplementationTrace => {
            vec!["definitions", "callers", "callees", "helpers", "tests"]
        }
        TaskKind::StorageAccountingTrace | TaskKind::ArtifactMathTrace => vec![
            "accounting_formula",
            "artifact_writer",
            "persisted_summary",
            "artifact_or_db_inspection_required",
        ],
        TaskKind::PersistencePathTrace => vec!["writer", "persist_path", "reader"],
        TaskKind::IndexingSummaryTrace => vec!["summary_counter", "index_entrypoint", "tests"],
        TaskKind::SchemaViewTrace => vec!["schema_source", "status_surface", "docs"],
        TaskKind::BenchmarkMetricTrace => vec!["metric_label", "report_surface", "tests"],
        TaskKind::TestImpact => vec!["TESTS", "MOCKS", "ASSERTS", "source_navigation"],
        TaskKind::DataflowTrace => vec!["FLOWS_TO", "WRITES", "SANITIZES", "MUTATES"],
        TaskKind::SecurityReview => vec!["AUTHORIZES", "CHECKS_ROLE", "SANITIZES", "EXPOSES"],
        TaskKind::DocsLookup => vec!["source_text"],
        TaskKind::EntityLookup => vec!["definition", "references"],
        TaskKind::FileLookup => vec!["file_path", "source_text"],
        TaskKind::Unknown => vec![],
    };
    goals.into_iter().map(str::to_string).collect()
}

fn evidence_expectation_for_task_kind(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::BuildSystemPackageAuthoring => "source_text_and_source_navigation",
        TaskKind::CodegraphInternalDebug => "source_navigation_with_lifecycle_guards",
        TaskKind::ImplementationTrace => "source_navigation_with_optional_graph_proof",
        TaskKind::StorageAccountingTrace | TaskKind::ArtifactMathTrace => {
            "source_navigation_plus_artifact_or_db_inspection_required"
        }
        TaskKind::PersistencePathTrace => "source_navigation_plus_runtime_artifact_check",
        TaskKind::IndexingSummaryTrace => "source_navigation_and_summary_counter_tests",
        TaskKind::SchemaViewTrace => "source_text_and_schema_status",
        TaskKind::BenchmarkMetricTrace => "source_text_and_report_surface_validation",
        TaskKind::TestImpact => "source_navigation_tests",
        TaskKind::DataflowTrace => "graph_or_source_verified_dataflow",
        TaskKind::SecurityReview => "graph_or_source_verified_authorization",
        TaskKind::DocsLookup => "source_text_evidence",
        TaskKind::EntityLookup => "exact_entity_or_source_navigation",
        TaskKind::FileLookup => "exact_file_or_source_text",
        TaskKind::Unknown => "candidate_only_until_verified",
    }
}

fn task_profile_by_id(profile_id: &str) -> TaskProfile {
    match profile_id {
        "build_system_package_authoring" => task_profile(
            profile_id,
            "Build system package authoring",
            &[
                "Buildroot",
                "generic-package",
                "Config.in",
                ".mk",
                "BR2_PACKAGE",
                "download",
                "hash",
            ],
            &["source_text", "source_navigation", "config_text"],
            &[".adoc", ".mk", "Config.in", ".hash", ".patch", ".sh"],
            &[
                "authoring_docs",
                "package_metadata",
                "kconfig_wiring",
                "download_infrastructure",
            ],
            "source navigation expected; graph proof optional and never inferred from docs",
            "fall back to role-diverse source text across docs/package/kconfig/support",
            &[
                "Verify docs and package infrastructure spans before editing",
                "Inspect package metadata, Kconfig wiring, download/hash, and install hooks",
            ],
            &[
                "Do not infer graph proof from Buildroot docs text",
                "Do not treat package examples as exact target package behavior",
            ],
            8,
        ),
        "codegraph_internal_debug" => task_profile(
            profile_id,
            "CodeGraph internal debug",
            &[
                "DB lifecycle",
                "passport",
                "preflight",
                "stale DB",
                "doctor",
                "status",
                "index",
                "context-pack",
            ],
            &["source_navigation", "test_evidence", "lifecycle_diagnostic"],
            &[".rs", ".md", ".json"],
            &[
                "entrypoint_symbols",
                "lifecycle_preflight",
                "store_open",
                "tests",
            ],
            "graph proof useful only when source spans verify lifecycle relations",
            "fall back to source navigation and lifecycle diagnostics",
            &[
                "Verify lifecycle preflight decision and store open guard",
                "Run the targeted lifecycle/status test before claiming fixed behavior",
            ],
            &[
                "Do not use diagnostic-only lifecycle output as claimable evidence",
                "Do not hide stale/passport/schema/scope blockers",
            ],
            6,
        ),
        "implementation_trace" => task_profile(
            profile_id,
            "Implementation trace",
            &[
                "trace implementation",
                "definitions",
                "helpers",
                "callers",
                "callees",
                "tests",
            ],
            &["source_navigation", "graph_entity", "test_evidence"],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".go"],
            &[
                "definitions",
                "helpers",
                "call_graph",
                "tests",
                "persistence",
            ],
            "attempt graph/source verification; text and vectors remain candidate-only",
            "fall back to same-file helpers, tests, and inspection requirements",
            &[
                "Inspect definitions and helper/caller/callee spans first",
                "Validate behavior with the smallest targeted test or smoke",
            ],
            &[
                "Do not infer final persisted values from source code alone",
                "Do not overclaim graph proof when only source navigation exists",
            ],
            10,
        ),
        "storage_accounting_trace" => task_profile(
            profile_id,
            "Storage accounting trace",
            &[
                "accounting",
                "persisted size",
                "generated/selected/persisted counts",
                "actual_index_file_bytes",
                "estimated_f32_payload_bytes",
                "DB size math",
            ],
            &[
                "source_navigation",
                "metric_label",
                "artifact_inspection_requirement",
            ],
            &[".rs", ".json", ".md"],
            &[
                "vector_chunk_index_build",
                "metric_reporting",
                "artifact_writer",
                "tests",
            ],
            "implementation accounting is not graph relation proof",
            "require artifact or DB inspection before final persisted-size claims",
            &[
                "Validate generated, selected, and persisted counts separately",
                "Inspect artifact/DB state before claiming final persisted bytes",
            ],
            &[
                "Do not call pretty_json artifact size compressed-vector storage",
                "Do not collapse actual_index_file_bytes and estimated_f32_payload_bytes",
            ],
            7,
        ),
        "artifact_math_trace" => task_profile(
            profile_id,
            "Artifact math trace",
            &[
                "artifact size",
                "payload size",
                "formula",
                "metric labels",
                "actual_index_file_bytes",
                "estimated_f32_payload_bytes",
            ],
            &[
                "source_navigation",
                "metric_label",
                "artifact_inspection_requirement",
            ],
            &[".rs", ".json", ".md"],
            &[
                "metric_reporting",
                "artifact_writer",
                "persisted_index_summary",
                "tests",
            ],
            "formula/source evidence is not persisted artifact proof",
            "require artifact inspection before claims about on-disk bytes",
            &[
                "Validate metric labels against latest vector metric contract",
                "Inspect report/status surfaces that expose storage math",
            ],
            &[
                "Do not treat estimated raw f32 payload bytes as JSON artifact bytes",
                "Do not claim compression unless product compression exists",
            ],
            7,
        ),
        "persistence_path_trace" => task_profile(
            profile_id,
            "Persistence path trace",
            &["persist", "write", "artifact", "DB", "reader"],
            &["source_navigation", "artifact_inspection_requirement"],
            &[".rs", ".json", ".sqlite"],
            &["writer", "persist_path", "reader", "tests"],
            "source navigation can locate persistence path; artifact proof requires inspection",
            "fall back to writer/reader source spans and DB inspection requirements",
            &["Inspect writer and reader code before claiming persistence behavior"],
            &["Do not infer final persisted state without artifact or DB inspection"],
            6,
        ),
        "indexing_summary_trace" => task_profile(
            profile_id,
            "Indexing summary trace",
            &[
                "index",
                "summary counters",
                "generated",
                "selected",
                "persisted",
            ],
            &["source_navigation", "summary_counter", "test_evidence"],
            &[".rs", ".json", ".md"],
            &["index_entrypoint", "summary_counter", "tests", "reports"],
            "source navigation plus tests; graph proof only if verified spans exist",
            "fall back to summary structs/counters and status/report surfaces",
            &["Validate summary counter labels and test coverage"],
            &["Do not merge generated, selected, and persisted counts"],
            6,
        ),
        "schema_view_trace" => task_profile(
            profile_id,
            "Schema view trace",
            &["schema", "status", "view", "doctor"],
            &["source_text", "schema_status", "source_navigation"],
            &[".rs", ".sql", ".md", ".json"],
            &["schema_source", "status_surface", "docs", "tests"],
            "schema/status text is source evidence unless graph/source verification succeeds",
            "fall back to schema source, status/doctor surface, and docs",
            &["Verify schema surface and status output labels"],
            &["Do not claim relation precision absent proof DB relations"],
            5,
        ),
        "benchmark_metric_trace" => task_profile(
            profile_id,
            "Benchmark metric trace",
            &["benchmark", "metric", "quality gate", "report"],
            &["source_text", "metric_label", "test_evidence"],
            &[".rs", ".json", ".md"],
            &["metric_definition", "report_surface", "tests"],
            "benchmark/report evidence is diagnostic unless promoted and verified",
            "fall back to report surface and metric-definition source spans",
            &["Validate metric labels against stable contract before reporting"],
            &["Do not claim final intended-performance pass"],
            5,
        ),
        "test_impact" => task_profile(
            profile_id,
            "Test impact",
            &[
                "test-impact",
                "test",
                "spec",
                "mock",
                "assertion",
                "fixture",
            ],
            &["source_navigation", "test_evidence", "mock_assertion"],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &[
                "changed_symbol",
                "production_target",
                "test_files",
                "mocks_assertions",
            ],
            "TESTS/MOCKS/ASSERTS graph proof only if verified; otherwise source navigation",
            "fall back to exact changed symbol, nearby tests, and assertion text",
            &["Run the smallest test that covers the changed helper"],
            &["Do not assume all tests are impacted from a generic helper mention"],
            5,
        ),
        "dataflow_trace" => task_profile(
            profile_id,
            "Dataflow trace",
            &[
                "dataflow",
                "request input",
                "source",
                "sink",
                "database write",
                "mutation",
            ],
            &["source_navigation", "dataflow_edge", "sanitizer"],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".sql"],
            &[
                "source",
                "sink",
                "intermediate_helper",
                "sanitizer",
                "mutation_write",
            ],
            "proof path preferred for FLOWS_TO/WRITES; source spans required either way",
            "fall back to source/sink text and explicit proof-path attempt",
            &["Verify source, sink, sanitizer, and mutation/write spans"],
            &["Do not infer dataflow from same-file proximity alone"],
            6,
        ),
        "security_review" => task_profile(
            profile_id,
            "Security review",
            &[
                "auth",
                "authorize",
                "role",
                "admin",
                "permission",
                "RBAC",
                "checkRole",
            ],
            &["source_navigation", "auth_policy", "test_evidence"],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &[
                "auth_entrypoint",
                "role_check",
                "permission_gate",
                "sanitizer_validator",
            ],
            "AUTHORIZES/CHECKS_ROLE proof preferred; source spans required",
            "fall back to route/auth/role/test source navigation",
            &["Verify role/permission checks and tests before claiming authorization behavior"],
            &["Do not infer authorization from route names or comments"],
            6,
        ),
        "docs_lookup" => task_profile(
            profile_id,
            "Docs lookup",
            &["docs", "README", "manual", "guide", "explain"],
            &["source_text", "documentation"],
            &[".md", ".adoc", ".txt"],
            &["docs_manual", "readme_docs", "config_reference_files"],
            "docs are source text evidence, not graph proof",
            "fall back to docs/readme/manual text and related source only if requested",
            &["Verify docs text and avoid implementation claims without source inspection"],
            &["Do not treat documentation support as graph proof"],
            4,
        ),
        _ => task_profile(
            "unknown_fallback",
            "Unknown fallback",
            &["insufficient or conflicting signals"],
            &["source_text", "file_path"],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".md", ".json", ".toml"],
            &["text_evidence_lookup", "file_lookup"],
            "no graph proof expected unless exact seeds exist and verification succeeds",
            "return conservative text/file lookup atoms and label unknowns",
            &["Inspect exact file or symbol seeds first when present"],
            &["Do not infer task kind or graph proof from generic task verbs"],
            2,
        ),
    }
}

fn task_profile(
    profile_id: &str,
    profile_name: &str,
    signals: &[&str],
    preferred_evidence_roles: &[&str],
    preferred_file_kinds: &[&str],
    retrieval_branches: &[&str],
    graph_expectation: &str,
    fallback_policy: &str,
    validation_templates: &[&str],
    risk_templates: &[&str],
    role_budget: usize,
) -> TaskProfile {
    TaskProfile {
        profile_id: profile_id.to_string(),
        profile_name: profile_name.to_string(),
        signals: string_vec(signals),
        ignored_generic_terms: string_vec(&[
            "find",
            "trace",
            "plan",
            "inspect",
            "how",
            "where",
            "add",
            "fix",
            "change",
            "understand",
        ]),
        preferred_evidence_roles: string_vec(preferred_evidence_roles),
        preferred_file_kinds: string_vec(preferred_file_kinds),
        retrieval_branches: string_vec(retrieval_branches),
        graph_expectation: graph_expectation.to_string(),
        fallback_policy: fallback_policy.to_string(),
        validation_templates: string_vec(validation_templates),
        risk_templates: string_vec(risk_templates),
        role_budget,
        confidence_rules: vec![
            "exact domain signals outrank generic task verbs".to_string(),
            "generic task verbs do not become exact symbol seeds".to_string(),
            "conflicting high scores reduce confidence and preserve ambiguity".to_string(),
        ],
    }
}

fn string_vec(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn proof_attempt_policy_for_intent(intent: &TaskIntent) -> String {
    match intent.task_kind {
        TaskKind::BuildSystemPackageAuthoring | TaskKind::DocsLookup => {
            "source_text_first_graph_optional_no_overclaim".to_string()
        }
        TaskKind::StorageAccountingTrace | TaskKind::ArtifactMathTrace => {
            "source_navigation_then_artifact_or_db_inspection_required".to_string()
        }
        TaskKind::Unknown if intent.exact_seeds.is_empty() && intent.file_path_seeds.is_empty() => {
            "no_exact_graph_proof_expectation".to_string()
        }
        TaskKind::Unknown => "attempt_exact_seed_source_verification_only".to_string(),
        _ => "attempt_graph_or_source_verification_for_exact_seeds".to_string(),
    }
}

fn retrieval_atoms_for_profile(
    intent: &TaskIntent,
    profile: &TaskProfile,
) -> Vec<RetrievalQueryAtom> {
    match profile.profile_id.as_str() {
        "build_system_package_authoring" => buildroot_authoring_atoms(intent),
        "codegraph_internal_debug" => codegraph_internal_debug_atoms(intent),
        "implementation_trace" => implementation_trace_atoms(intent),
        "storage_accounting_trace" | "artifact_math_trace" => {
            vector_metric_accounting_atoms(intent)
        }
        "persistence_path_trace" => persistence_path_atoms(intent),
        "indexing_summary_trace" => indexing_summary_atoms(intent),
        "schema_view_trace" => schema_view_atoms(intent),
        "benchmark_metric_trace" => benchmark_metric_atoms(intent),
        "test_impact" => test_impact_atoms(intent),
        "dataflow_trace" => dataflow_trace_atoms(intent),
        "security_review" => security_review_atoms(intent),
        "docs_lookup" => docs_lookup_atoms(intent),
        _ => unknown_fallback_atoms(intent),
    }
}

fn atom(
    role: &str,
    query_text: String,
    path_hints: &[&str],
    file_kind_hints: &[&str],
    evidence_role_filter: &[&str],
    candidate_source_preference: &[&str],
    max_candidates: usize,
    why: &str,
    expected_signal: &str,
    proof_expectation: &str,
) -> RetrievalQueryAtom {
    RetrievalQueryAtom {
        role: role.to_string(),
        query_text,
        path_hints: string_vec(path_hints),
        file_kind_hints: string_vec(file_kind_hints),
        evidence_role_filter: string_vec(evidence_role_filter),
        candidate_source_preference: string_vec(candidate_source_preference),
        max_candidates,
        why: why.to_string(),
        expected_signal: expected_signal.to_string(),
        proof_expectation: proof_expectation.to_string(),
    }
}

fn intent_seed_query(intent: &TaskIntent, base: &str) -> String {
    let seeds = intent
        .exact_seeds
        .iter()
        .chain(intent.config_keys.iter())
        .chain(intent.file_path_seeds.iter())
        .take(4)
        .cloned()
        .collect::<Vec<_>>();
    if seeds.is_empty() {
        base.to_string()
    } else {
        format!("{base} {}", seeds.join(" "))
    }
}

fn buildroot_authoring_atoms(_intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    vec![
        atom(
            "authoring_docs",
            "generic-package package infrastructure manual".to_string(),
            &["docs/manual"],
            &[".adoc"],
            &["source_text", "documentation"],
            &["text_evidence", "file_path"],
            3,
            "Find the authoring rules before inspecting implementation files.",
            "generic-package documentation and package infrastructure wording",
            "source_text_evidence_not_graph_proof",
        ),
        atom(
            "package_metadata",
            "package metadata VERSION SITE LICENSE DEPENDENCIES generic-package".to_string(),
            &["package/*/*.mk", "package/pkg-generic.mk"],
            &[".mk"],
            &["source_text", "source_navigation"],
            &["text_evidence", "graph_entity"],
            4,
            "Package authoring depends on metadata variables and generic-package invocation.",
            "package metadata variables and generic-package call",
            "source_navigation_evidence_not_graph_proof_unless_verified",
        ),
        atom(
            "kconfig_wiring",
            "Config.in BR2_PACKAGE depends on select source package Config.in".to_string(),
            &["package/Config.in", "package/*/Config.in"],
            &["Config.in", ".in"],
            &["config_text", "source_text"],
            &["text_evidence", "file_path"],
            4,
            "Kconfig wiring is a separate authoring role from package metadata.",
            "BR2_PACKAGE config entry and package/Config.in inclusion",
            "source_text_evidence_not_graph_proof",
        ),
        atom(
            "makefile_inclusion",
            "package makefile include generic-package pkg-generic".to_string(),
            &["package/pkg-generic.mk", "package/Makefile.in"],
            &[".mk"],
            &["source_navigation", "source_text"],
            &["text_evidence", "graph_entity"],
            3,
            "Makefile inclusion proves where package infrastructure is wired.",
            "pkg-generic include or package makefile wiring",
            "source_navigation_evidence_not_graph_proof_unless_verified",
        ),
        atom(
            "download_infrastructure",
            "download site hash pkg-download support/download".to_string(),
            &["package/pkg-download.mk", "support/download"],
            &[".mk", ".sh", ".hash"],
            &["source_navigation", "source_text"],
            &["text_evidence", "file_path"],
            3,
            "Download and hash handling are separate from package metadata.",
            "download site/hash implementation or docs",
            "source_text_or_source_navigation_not_graph_proof",
        ),
        atom(
            "build_install_infrastructure",
            "inner-generic-package BUILD_CMDS INSTALL_TARGET_CMDS install infrastructure"
                .to_string(),
            &["package/pkg-generic.mk", "package/*/*.mk"],
            &[".mk"],
            &["source_navigation", "source_text"],
            &["graph_entity", "text_evidence"],
            3,
            "Build and install hooks are the execution surface for package authoring.",
            "build/install command variables and inner-generic-package",
            "source_navigation_evidence_not_graph_proof_unless_verified",
        ),
        atom(
            "support_scripts",
            "support scripts package infrastructure download helpers".to_string(),
            &["support/scripts", "support/download"],
            &[".sh", ".py", ".mk"],
            &["source_navigation", "source_text"],
            &["file_path", "text_evidence"],
            2,
            "Support scripts often explain behavior not visible in package metadata.",
            "support/download or support/scripts helpers",
            "source_navigation_evidence_not_graph_proof_unless_verified",
        ),
        atom(
            "examples",
            "generic-package Config.in package example .mk".to_string(),
            &["docs/manual", "package/*"],
            &[".adoc", ".mk", "Config.in"],
            &["source_text"],
            &["text_evidence"],
            2,
            "Examples are useful comparison evidence but not proof of the target package.",
            "small package examples using generic-package",
            "example_source_text_not_graph_proof",
        ),
    ]
}

fn codegraph_internal_debug_atoms(intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    vec![
        atom(
            "entrypoint_symbols",
            intent_seed_query(
                intent,
                "context-pack index status doctor entrypoint symbols",
            ),
            &[
                "crates/codegraph-cli/src",
                "crates/codegraph-mcp-server/src",
            ],
            &[".rs"],
            &["source_navigation", "graph_entity"],
            &["graph_entity", "text_evidence"],
            4,
            "Start from the command/tool entrypoints that expose the behavior.",
            "CLI or MCP entrypoint symbols",
            "graph_or_source_verification_required",
        ),
        atom(
            "lifecycle_preflight",
            "DB lifecycle passport preflight stale DB scope guards".to_string(),
            &[
                "crates/codegraph-index/src",
                "crates/codegraph-cli/src",
                "crates/codegraph-mcp-server/src",
            ],
            &[".rs"],
            &["source_navigation", "lifecycle_diagnostic"],
            &["graph_entity", "text_evidence"],
            4,
            "Lifecycle gates decide whether packet evidence is claimable.",
            "passport/preflight/stale DB/scope guard logic",
            "source_navigation_required_diagnostic_output_not_claimable",
        ),
        atom(
            "store_open",
            "open store SqliteGraphStore DB passport reusable DB guard".to_string(),
            &[
                "crates/codegraph-store/src",
                "crates/codegraph-index/src",
                "crates/codegraph-cli/src",
            ],
            &[".rs"],
            &["source_navigation"],
            &["graph_entity", "text_evidence"],
            3,
            "DB lifecycle bugs usually cross the store-open boundary.",
            "store open and reusable DB guard",
            "source_navigation_or_graph_proof_if_verified",
        ),
        atom(
            "status_doctor",
            "status doctor lifecycle diagnostics schema scope passport".to_string(),
            &[
                "crates/codegraph-cli/src",
                "crates/codegraph-mcp-server/src",
            ],
            &[".rs"],
            &["source_navigation", "diagnostic"],
            &["text_evidence", "graph_entity"],
            3,
            "Status and doctor surfaces expose lifecycle state to agents.",
            "status/doctor diagnostic output",
            "diagnostic_output_not_graph_proof",
        ),
        atom(
            "tests",
            "lifecycle preflight stale DB passport status doctor tests".to_string(),
            &["crates/*/src", "crates/*/tests"],
            &[".rs"],
            &["test_evidence", "source_navigation"],
            &["text_evidence", "graph_entity"],
            3,
            "Lifecycle behavior needs targeted tests before claimable changes.",
            "unit or CLI smoke tests for lifecycle guards",
            "test_source_evidence_not_graph_proof_unless_verified",
        ),
        atom(
            "schemas_docs",
            "schema passport lifecycle docs status contract".to_string(),
            &["docs", "README.md", "reports/final"],
            &[".md", ".json"],
            &["source_text", "documentation"],
            &["text_evidence"],
            2,
            "Schema/docs can explain public contract but do not prove implementation.",
            "schema or lifecycle public contract text",
            "source_text_evidence_not_graph_proof",
        ),
    ]
}

fn implementation_trace_atoms(intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    vec![
        atom(
            "definitions",
            intent_seed_query(
                intent,
                "function type definitions for implementation surface",
            ),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".go"],
            &["graph_entity", "source_navigation"],
            &["graph_entity", "text_evidence"],
            4,
            "Definitions anchor the implementation surface before expanding outward.",
            "definition span for exact symbol or implementation surface",
            "graph_or_source_verification_required",
        ),
        atom(
            "same_file_helpers",
            intent_seed_query(intent, "same-file helpers near implementation surface"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".go"],
            &["source_navigation"],
            &["graph_entity", "text_evidence"],
            3,
            "Helpers often explain behavior without requiring a broad repo pull.",
            "helper functions in the same file or module",
            "source_navigation_evidence_not_graph_proof_unless_verified",
        ),
        atom(
            "relevant_structs",
            intent_seed_query(
                intent,
                "struct types enum types config structs for implementation surface",
            ),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".go"],
            &["graph_entity", "source_navigation"],
            &["graph_entity"],
            3,
            "Types and structs define the state being traced.",
            "related type/struct/entity definitions",
            "graph_or_source_verification_required",
        ),
        atom(
            "relevant_constants",
            intent_seed_query(
                intent,
                "constants metric labels configuration keys used by implementation",
            ),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".go"],
            &["source_navigation"],
            &["text_evidence", "graph_entity"],
            2,
            "Constants often hold labels or formula terms that must remain truthful.",
            "constants or labels referenced by the implementation",
            "source_text_evidence_not_graph_proof",
        ),
        atom(
            "callers",
            intent_seed_query(intent, "callers of implementation surface"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".go"],
            &["graph_entity", "source_navigation"],
            &["graph_entity"],
            3,
            "Callers show who exercises the implementation surface.",
            "caller edges or source references",
            "graph_proof_only_if_verified_path_exists",
        ),
        atom(
            "callees",
            intent_seed_query(intent, "callees dependencies of implementation surface"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".go"],
            &["graph_entity", "source_navigation"],
            &["graph_entity"],
            3,
            "Callees show where the implementation delegates work.",
            "callee edges or source references",
            "graph_proof_only_if_verified_path_exists",
        ),
        atom(
            "related_tests",
            intent_seed_query(intent, "tests validating implementation behavior"),
            &["crates/*/src", "crates/*/tests"],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["test_evidence"],
            &["text_evidence", "graph_entity"],
            3,
            "Tests bound the behavior before edits.",
            "unit/smoke tests for the implementation surface",
            "test_source_evidence_not_graph_proof_unless_verified",
        ),
        atom(
            "persistence_path",
            intent_seed_query(
                intent,
                "write persist save artifact DB path for implementation",
            ),
            &[],
            &[".rs", ".json", ".sql"],
            &["source_navigation"],
            &["text_evidence", "graph_entity"],
            3,
            "Persistence claims require locating writer and reader paths.",
            "artifact or DB write/read source path",
            "source_navigation_then_artifact_or_db_inspection_required",
        ),
        atom(
            "accounting_summary",
            intent_seed_query(
                intent,
                "summary counters accounting generated selected persisted counts",
            ),
            &[],
            &[".rs", ".json", ".md"],
            &["source_navigation", "metric_label"],
            &["text_evidence", "graph_entity"],
            3,
            "Accounting summaries must preserve distinct generated/selected/persisted counts.",
            "summary counter or metric label source",
            "source_text_evidence_not_final_persisted_value_proof",
        ),
        atom(
            "artifact_inspection_requirements",
            "artifact inspection required for final persisted value claims".to_string(),
            &["reports", "target", ".codegraph"],
            &[".json", ".sqlite"],
            &["artifact_inspection_requirement"],
            &["artifact_or_db_inspection_requirement"],
            1,
            "Source code alone does not prove final artifact bytes.",
            "explicit artifact inspection requirement",
            "required_for_persisted_value_claims",
        ),
        atom(
            "db_inspection_requirements",
            "DB inspection required for final persisted database value claims".to_string(),
            &[".codegraph", "reports/audit/artifacts"],
            &[".sqlite", ".db"],
            &["db_inspection_requirement"],
            &["artifact_or_db_inspection_requirement"],
            1,
            "Source code alone does not prove DB row counts or persisted values.",
            "explicit DB inspection requirement",
            "required_for_persisted_value_claims",
        ),
    ]
}

fn vector_metric_accounting_atoms(_intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    vec![
        atom(
            "vector_chunk_index_build",
            "vector chunk index build generated selected persisted accounting summary".to_string(),
            &["crates/codegraph-index/src", "crates/codegraph-vector/src", "crates/codegraph-cli/src"],
            &[".rs"],
            &["source_navigation", "metric_label"],
            &["graph_entity", "text_evidence"],
            4,
            "Vector chunk index build is the source of generated/selected/persisted counts.",
            "build summary counters and vector chunk index build logic",
            "source_navigation_not_graph_relation_proof",
        ),
        atom(
            "vector_chunk_selection",
            "chunk selection diversity_ranked_v1 input_order_cap selected chunks".to_string(),
            &["crates/codegraph-index/src", "crates/codegraph-vector/src"],
            &[".rs"],
            &["source_navigation", "metric_label"],
            &["graph_entity", "text_evidence"],
            4,
            "Selection strategy must preserve selected-count semantics and representation guarantees.",
            "diversity_ranked_v1, input_order_cap, selected chunks",
            "source_navigation_not_graph_relation_proof",
        ),
        atom(
            "metric_reporting",
            "actual_index_file_bytes estimated_f32_payload_bytes pretty_json vector_payload_compression".to_string(),
            &["crates/codegraph-index/src", "crates/codegraph-cli/src", "crates/codegraph-mcp-server/src"],
            &[".rs", ".json", ".md"],
            &["metric_label", "source_text"],
            &["text_evidence", "graph_entity"],
            4,
            "Metric labels must distinguish JSON artifact bytes from estimated raw float32 payload.",
            "truthful vector metric labels",
            "source_text_evidence_not_storage_claim_without_artifact_inspection",
        ),
        atom(
            "artifact_writer",
            "vector index artifact writer JSON artifact size stores_chunk_text stores_chunk_metadata".to_string(),
            &["crates/codegraph-index/src", "crates/codegraph-cli/src"],
            &[".rs", ".json"],
            &["source_navigation"],
            &["graph_entity", "text_evidence"],
            3,
            "The writer determines what is actually persisted into the JSON artifact.",
            "artifact write path and stored chunk text/metadata fields",
            "source_navigation_then_artifact_inspection_required",
        ),
        atom(
            "persisted_index_summary",
            "persisted vector index summary selected persisted generated counts artifact size payload size".to_string(),
            &["crates/codegraph-index/src", "crates/codegraph-cli/src", "reports/final"],
            &[".rs", ".json", ".md"],
            &["source_navigation", "metric_label"],
            &["text_evidence", "graph_entity"],
            3,
            "Summary surfaces must keep generated/selected/persisted values distinct.",
            "persisted index summary and status/report fields",
            "source_text_evidence_not_final_persisted_value_proof",
        ),
        atom(
            "tests",
            "vector chunk index accounting metric labels artifact size tests".to_string(),
            &["crates/*/src", "crates/*/tests"],
            &[".rs"],
            &["test_evidence"],
            &["text_evidence", "graph_entity"],
            3,
            "Metric truthfulness needs focused tests over labels and counts.",
            "tests validating vector accounting and metric labels",
            "test_source_evidence_not_graph_proof_unless_verified",
        ),
        atom(
            "release_smoke_report_surfaces",
            "release smoke status context output vector metrics report surfaces".to_string(),
            &["reports/final", "reports/audit", "crates/codegraph-cli/src"],
            &[".md", ".json", ".rs"],
            &["source_text", "diagnostic"],
            &["text_evidence"],
            2,
            "Release/status/report surfaces are where misleading metric labels become user-visible.",
            "status/context/report vector metric output",
            "diagnostic_or_source_text_not_graph_proof",
        ),
    ]
}

fn persistence_path_atoms(intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    vec![
        atom(
            "writer",
            intent_seed_query(intent, "writer save persist artifact DB write path"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["source_navigation"],
            &["graph_entity", "text_evidence"],
            4,
            "Persistence traces start at the writer.",
            "write/persist/save source span",
            "source_navigation_then_artifact_or_db_inspection_required",
        ),
        atom(
            "reader",
            intent_seed_query(intent, "reader load persisted artifact DB read path"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["source_navigation"],
            &["graph_entity", "text_evidence"],
            3,
            "Readers determine how persisted state is consumed.",
            "load/read source span",
            "source_navigation_evidence_not_final_state_proof",
        ),
        atom(
            "tests",
            intent_seed_query(intent, "tests for persistence path"),
            &["crates/*/src", "crates/*/tests"],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["test_evidence"],
            &["text_evidence"],
            2,
            "Persistence behavior should have a targeted test or smoke.",
            "persistence test or smoke",
            "test_source_evidence_not_graph_proof_unless_verified",
        ),
    ]
}

fn indexing_summary_atoms(intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    let mut atoms = implementation_trace_atoms(intent);
    atoms.retain(|atom| {
        matches!(
            atom.role.as_str(),
            "definitions"
                | "accounting_summary"
                | "related_tests"
                | "artifact_inspection_requirements"
        )
    });
    atoms.push(atom(
        "index_entrypoint",
        "index entrypoint summary counters generated selected persisted".to_string(),
        &["crates/codegraph-index/src", "crates/codegraph-cli/src"],
        &[".rs"],
        &["source_navigation", "metric_label"],
        &["graph_entity", "text_evidence"],
        4,
        "Indexing summaries must be traced from the entrypoint that produces them.",
        "index entrypoint and summary counter source",
        "source_navigation_not_graph_relation_proof",
    ));
    atoms
}

fn schema_view_atoms(_intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    vec![
        atom(
            "schema_source",
            "schema status schema_version passport scope status fields".to_string(),
            &["crates/codegraph-store/src", "crates/codegraph-index/src"],
            &[".rs", ".sql"],
            &["source_text", "source_navigation"],
            &["text_evidence", "graph_entity"],
            4,
            "Schema view claims need the schema source and status surface.",
            "schema source or status field",
            "source_text_evidence_not_graph_proof",
        ),
        atom(
            "status_surface",
            "status doctor schema view output".to_string(),
            &[
                "crates/codegraph-cli/src",
                "crates/codegraph-mcp-server/src",
            ],
            &[".rs"],
            &["source_navigation"],
            &["text_evidence"],
            3,
            "Status/doctor surfaces reveal how schema state is exposed.",
            "schema/status output fields",
            "diagnostic_output_not_graph_proof",
        ),
        atom(
            "docs",
            "schema status docs README".to_string(),
            &["docs", "README.md"],
            &[".md"],
            &["source_text", "documentation"],
            &["text_evidence"],
            2,
            "Docs can support the public contract but cannot prove runtime state.",
            "schema docs or README text",
            "source_text_evidence_not_graph_proof",
        ),
    ]
}

fn benchmark_metric_atoms(_intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    vec![
        atom(
            "metric_definition",
            "benchmark metric quality gate report metric labels".to_string(),
            &["crates/codegraph-bench/src", "crates/codegraph-cli/src"],
            &[".rs"],
            &["metric_label", "source_navigation"],
            &["text_evidence", "graph_entity"],
            4,
            "Metric claims need the defining source, not only a report value.",
            "metric definition or label source",
            "source_text_evidence_not_final_performance_claim",
        ),
        atom(
            "report_surface",
            "final audit report quality gate metrics status".to_string(),
            &["reports/final", "reports/audit"],
            &[".md", ".json"],
            &["source_text", "diagnostic"],
            &["text_evidence"],
            3,
            "Report surfaces show current labels and diagnostic scope.",
            "report/status metric surface",
            "diagnostic_or_source_text_not_graph_proof",
        ),
        atom(
            "tests",
            "benchmark metric label tests quality gate".to_string(),
            &["crates/*/src", "crates/*/tests"],
            &[".rs"],
            &["test_evidence"],
            &["text_evidence"],
            3,
            "Metric label regressions need targeted tests.",
            "metric or quality-gate tests",
            "test_source_evidence_not_graph_proof_unless_verified",
        ),
    ]
}

fn test_impact_atoms(intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    vec![
        atom(
            "changed_symbol",
            intent_seed_query(intent, "changed helper symbol definition"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["graph_entity", "source_navigation"],
            &["graph_entity", "text_evidence"],
            3,
            "Test impact starts from the exact changed symbol or helper.",
            "changed production symbol definition",
            "graph_or_source_verification_required",
        ),
        atom(
            "production_target",
            intent_seed_query(intent, "production target using changed helper"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["source_navigation", "graph_entity"],
            &["graph_entity"],
            3,
            "The production target narrows which tests are relevant.",
            "production caller/user of changed helper",
            "graph_proof_only_if_verified_path_exists",
        ),
        atom(
            "test_files",
            intent_seed_query(intent, "tests specs covering production target"),
            &["crates/*/tests", "crates/*/src", "tests"],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["test_evidence"],
            &["text_evidence", "graph_entity"],
            4,
            "Impacted tests should be explicit files/spans, not a broad test suite pull.",
            "test or spec files for the changed surface",
            "test_source_evidence_not_graph_proof_unless_verified",
        ),
        atom(
            "mocks_assertions",
            intent_seed_query(intent, "mocks assertions fixtures around changed helper"),
            &["crates/*/tests", "crates/*/src", "tests"],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["mock_assertion", "test_evidence"],
            &["text_evidence", "graph_entity"],
            3,
            "Mocks and assertions show what behavior the test actually checks.",
            "mock/assertion/fixture evidence",
            "source_text_evidence_not_graph_proof",
        ),
        atom(
            "test_impact_fallback",
            intent_seed_query(intent, "fallback nearby tests for changed helper"),
            &["crates/*/tests", "tests"],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["test_evidence"],
            &["text_evidence"],
            2,
            "Fallback is bounded to nearby/exact-seed tests.",
            "nearby tests if exact graph relation is missing",
            "fallback_source_text_not_graph_proof",
        ),
    ]
}

fn dataflow_trace_atoms(intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    vec![
        atom(
            "source",
            intent_seed_query(intent, "request input source entrypoint"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["source_navigation", "dataflow_edge"],
            &["graph_entity", "text_evidence"],
            3,
            "A dataflow packet needs an explicit source.",
            "request/input source span",
            "graph_or_source_verification_required",
        ),
        atom(
            "sink",
            intent_seed_query(intent, "database write sink"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".sql"],
            &["source_navigation", "dataflow_edge"],
            &["graph_entity", "text_evidence"],
            3,
            "The sink defines the dataflow claim boundary.",
            "database write or sink span",
            "graph_or_source_verification_required",
        ),
        atom(
            "intermediate_helper",
            intent_seed_query(
                intent,
                "intermediate helper transforms request input before sink",
            ),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["source_navigation", "dataflow_edge"],
            &["graph_entity", "text_evidence"],
            3,
            "Intermediate helpers prevent a shallow source-to-sink jump.",
            "helper or transform between source and sink",
            "source_navigation_or_verified_dataflow_path_required",
        ),
        atom(
            "sanitizer",
            intent_seed_query(intent, "sanitizer validator before database write"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["sanitizer", "source_navigation"],
            &["graph_entity", "text_evidence"],
            3,
            "Sanitizer evidence changes the interpretation of the flow.",
            "sanitizer/validator source span",
            "source_navigation_not_sufficient_for_dataflow_proof",
        ),
        atom(
            "mutation_write",
            intent_seed_query(intent, "mutation write database write persistence"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".sql"],
            &["source_navigation", "dataflow_edge"],
            &["graph_entity", "text_evidence"],
            3,
            "Writes/mutations are the side effect to verify.",
            "mutation/write source span",
            "graph_or_source_verification_required",
        ),
        atom(
            "proof_path_attempt",
            intent_seed_query(intent, "FLOWS_TO WRITES SANITIZES proof path attempt"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["dataflow_edge", "source_navigation"],
            &["graph_entity"],
            2,
            "Dataflow claims should attempt a proof path but must label missing proof.",
            "FLOWS_TO/WRITES/SANITIZES relation path",
            "no_proof_path_found_must_remain_visible_if_missing",
        ),
    ]
}

fn security_review_atoms(intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    vec![
        atom(
            "auth_entrypoint",
            intent_seed_query(intent, "auth entrypoint route handler admin behavior"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["source_navigation", "auth_policy"],
            &["graph_entity", "text_evidence"],
            3,
            "Authorization review starts at the exposed entrypoint.",
            "auth or route entrypoint span",
            "graph_or_source_verification_required",
        ),
        atom(
            "role_check",
            intent_seed_query(intent, "role check checkRole admin RBAC"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["auth_policy", "source_navigation"],
            &["graph_entity", "text_evidence"],
            3,
            "Role checks are the core authorization gate.",
            "role/checkRole/RBAC source span",
            "AUTHORIZES_or_CHECKS_ROLE_graph_proof_only_if_verified",
        ),
        atom(
            "permission_gate",
            intent_seed_query(intent, "permission gate authorize admin-only behavior"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["auth_policy", "source_navigation"],
            &["graph_entity", "text_evidence"],
            3,
            "Permission gates decide whether admin-only behavior is protected.",
            "permission/authorize guard span",
            "source_navigation_required_graph_proof_optional",
        ),
        atom(
            "sanitizer_validator",
            intent_seed_query(intent, "sanitizer validator authorization input checks"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["sanitizer", "source_navigation"],
            &["graph_entity", "text_evidence"],
            2,
            "Validators and sanitizers can be relevant adjacent controls.",
            "validator/sanitizer source span",
            "source_navigation_not_authorization_proof",
        ),
        atom(
            "route_expose",
            intent_seed_query(intent, "route expose endpoint admin behavior"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["source_navigation"],
            &["graph_entity", "text_evidence"],
            2,
            "Exposed routes define whether the gate is reachable.",
            "route/endpoint exposure source span",
            "source_navigation_not_authorization_proof",
        ),
        atom(
            "tests",
            intent_seed_query(intent, "authorization role admin permission tests"),
            &["crates/*/tests", "tests"],
            &[".rs", ".ts", ".tsx", ".js", ".py"],
            &["test_evidence"],
            &["text_evidence", "graph_entity"],
            3,
            "Security behavior needs tests over allowed and denied paths.",
            "authorization/permission tests",
            "test_source_evidence_not_graph_proof_unless_verified",
        ),
    ]
}

fn docs_lookup_atoms(intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    vec![
        atom(
            "docs_manual",
            intent_seed_query(intent, "docs manual guide explanation"),
            &["docs", "docs/manual"],
            &[".md", ".adoc", ".txt"],
            &["source_text", "documentation"],
            &["text_evidence", "file_path"],
            4,
            "Docs/manual text is the primary evidence for docs lookup.",
            "manual/docs text matching the task",
            "source_text_evidence_not_graph_proof",
        ),
        atom(
            "readme_docs",
            intent_seed_query(intent, "README docs guide explanation"),
            &["README.md", "crates/*/README.md"],
            &[".md"],
            &["source_text", "documentation"],
            &["text_evidence", "file_path"],
            3,
            "README surfaces often hold stable public usage details.",
            "README or guide text",
            "source_text_evidence_not_graph_proof",
        ),
        atom(
            "config_reference_files",
            intent_seed_query(intent, "config reference files docs"),
            &["docs", "templates", "fixtures"],
            &[".md", ".adoc", ".json", ".toml", "Config.in"],
            &["source_text", "config_text"],
            &["text_evidence", "file_path"],
            2,
            "Reference/config files can support docs answers without broad code pulls.",
            "configuration reference text",
            "source_text_evidence_not_graph_proof",
        ),
        atom(
            "related_source_if_implementation",
            intent_seed_query(intent, "related source only if task asks implementation"),
            &["crates"],
            &[".rs"],
            &["source_navigation"],
            &["graph_entity", "text_evidence"],
            1,
            "Implementation source is secondary for docs lookup and only used when requested.",
            "related source span if implementation is explicitly requested",
            "source_navigation_not_docs_or_graph_proof",
        ),
    ]
}

fn unknown_fallback_atoms(intent: &TaskIntent) -> Vec<RetrievalQueryAtom> {
    let proof_expectation = if intent.exact_seeds.is_empty() && intent.file_path_seeds.is_empty() {
        "no_exact_graph_proof_expectation"
    } else {
        "attempt_exact_seed_source_verification_only"
    };
    vec![
        atom(
            "text_evidence_lookup",
            intent_seed_query(
                intent,
                "conservative source text evidence for ambiguous task",
            ),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".md", ".json", ".toml"],
            &["source_text"],
            &["text_evidence"],
            3,
            "Ambiguous tasks should return conservative source text evidence, not broad context.",
            "bounded text evidence around exact seeds if present",
            proof_expectation,
        ),
        atom(
            "file_lookup",
            intent_seed_query(intent, "conservative file lookup for ambiguous task"),
            &[],
            &[".rs", ".ts", ".tsx", ".js", ".py", ".md", ".json", ".toml"],
            &["source_navigation", "source_text"],
            &["file_path", "text_evidence"],
            3,
            "File lookup is bounded to explicit file/path seeds when available.",
            "exact file path or nearest file text if present",
            proof_expectation,
        ),
    ]
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
        let value = match (&function, line_number) {
            (Some(function), Some(line_number)) => format!("{function} {path}:{line_number}"),
            (Some(function), None) => format!("{function} {path}"),
            (None, Some(line_number)) => format!("{path}:{line_number}"),
            (None, None) => path.clone(),
        };
        let mut seed = PromptSeed::with_source(
            PromptSeedKind::StackTrace,
            value,
            Some(line.to_string()),
            true,
        );
        seed.file_path = Some(path.clone());
        seed.line = line_number;
        seed.function = function.clone();
        seeds.push(seed);
        if let Some(function) = function.filter(|function| !function.is_empty()) {
            seeds.push(PromptSeed::with_source(
                PromptSeedKind::Symbol,
                function,
                Some(line.to_string()),
                true,
            ));
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
        let mut seed = PromptSeed::with_source(
            PromptSeedKind::LineNumber,
            format!("line:{line_number}"),
            Some(line.to_string()),
            true,
        );
        seed.line = Some(line_number);
        seeds.push(seed);
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
    let mut path_seed = PromptSeed::with_source(
        PromptSeedKind::FilePath,
        path.clone(),
        Some(token.to_string()),
        true,
    );
    path_seed.file_path = Some(path.clone());
    seeds.push(path_seed);
    if let Some(line_number) = line {
        let mut line_seed = PromptSeed::with_source(
            PromptSeedKind::LineNumber,
            format!("{path}:{line_number}"),
            Some(token.to_string()),
            true,
        );
        line_seed.file_path = Some(path.clone());
        line_seed.line = Some(line_number);
        seeds.push(line_seed);
    }
    extract_buildroot_package_name_from_path(&path, seeds);
    extract_support_script_name_from_path(&path, seeds);
}

fn extract_buildroot_phrase_seeds(line: &str, seeds: &mut Vec<PromptSeed>) {
    let lower = line.to_ascii_lowercase();
    for (needle, value) in [
        ("generic-package", "generic-package"),
        ("generic package", "generic-package"),
        ("host-generic-package", "host-generic-package"),
        ("config.in", "Config.in"),
        ("package infrastructure", "package infrastructure"),
        ("depends on", "depends on"),
        ("source url", "source URL"),
        ("source_url", "source URL"),
        ("support/scripts", "support/scripts"),
    ] {
        if lower.contains(needle) {
            let kind = match value {
                "Config.in" => PromptSeedKind::ConfigToken,
                "support/scripts" => PromptSeedKind::PathToken,
                _ => PromptSeedKind::TextToken,
            };
            seeds.push(PromptSeed::simple(kind, value, true));
        }
    }
}

fn extract_buildroot_token_seed(token: &str, seeds: &mut Vec<PromptSeed>) {
    let cleaned = clean_symbol(token);
    let cleaned_path = clean_path_token(token).replace('\\', "/");
    let lower = cleaned.to_ascii_lowercase();
    let lower_path = cleaned_path.to_ascii_lowercase();

    if cleaned.is_empty() {
        return;
    }
    if is_prompt_task_verb(&cleaned) {
        seeds.push(PromptSeed::with_source(
            PromptSeedKind::TaskVerbIgnored,
            cleaned,
            Some(token.to_string()),
            false,
        ));
        return;
    }
    if cleaned.starts_with("BR2_PACKAGE_")
        && cleaned
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
    {
        seeds.push(PromptSeed::with_source(
            PromptSeedKind::ConfigToken,
            cleaned,
            Some(token.to_string()),
            true,
        ));
        return;
    }
    if lower == "config.in" {
        seeds.push(PromptSeed::with_source(
            PromptSeedKind::ConfigToken,
            "Config.in",
            Some(token.to_string()),
            true,
        ));
        return;
    }
    if matches!(lower.as_str(), "*.mk" | ".mk" | "mk") || lower.ends_with(".mk") {
        seeds.push(PromptSeed::with_source(
            PromptSeedKind::FilePattern,
            ".mk",
            Some(token.to_string()),
            true,
        ));
    }
    if matches!(lower.as_str(), "*.adoc" | ".adoc" | "adoc") || lower.ends_with(".adoc") {
        seeds.push(PromptSeed::with_source(
            PromptSeedKind::FilePattern,
            ".adoc",
            Some(token.to_string()),
            true,
        ));
    }
    if matches!(
        lower.as_str(),
        "generic-package"
            | "host-generic-package"
            | "license"
            | "version"
            | "dependencies"
            | "select"
            | "docs"
    ) {
        seeds.push(PromptSeed::with_source(
            PromptSeedKind::TextToken,
            cleaned.clone(),
            Some(token.to_string()),
            true,
        ));
    }
    if lower_path.starts_with("support/scripts/") || lower_path.starts_with("support/download/") {
        seeds.push(PromptSeed::with_source(
            PromptSeedKind::PathToken,
            cleaned_path.clone(),
            Some(token.to_string()),
            true,
        ));
        extract_support_script_name_from_path(&cleaned_path, seeds);
    } else if looks_like_support_script_name(&cleaned) {
        seeds.push(PromptSeed::with_source(
            PromptSeedKind::PathToken,
            cleaned,
            Some(token.to_string()),
            true,
        ));
    }
    if lower_path.starts_with("package/") {
        extract_buildroot_package_name_from_path(&cleaned_path, seeds);
    }
}

fn extract_symbol_or_identifier_seed(token: &str, seeds: &mut Vec<PromptSeed>) {
    let cleaned = clean_symbol(token);
    if is_prompt_task_verb(&cleaned) {
        seeds.push(PromptSeed::with_source(
            PromptSeedKind::TaskVerbIgnored,
            cleaned,
            Some(token.to_string()),
            false,
        ));
        return;
    }
    if cleaned.len() < 3 || looks_like_path(&cleaned) || is_keyword_or_common_word(&cleaned) {
        return;
    }

    if is_symbol(&cleaned) {
        seeds.push(PromptSeed::with_source(
            PromptSeedKind::Symbol,
            cleaned,
            Some(token.to_string()),
            true,
        ));
    } else if is_identifier(&cleaned) && looks_code_like_identifier(&cleaned) {
        seeds.push(PromptSeed::with_source(
            PromptSeedKind::Identifier,
            cleaned,
            Some(token.to_string()),
            true,
        ));
    }
}

fn extract_buildroot_package_name_from_path(path: &str, seeds: &mut Vec<PromptSeed>) {
    let normalized = path.replace('\\', "/");
    let parts = normalized.split('/').collect::<Vec<_>>();
    if parts.len() < 3 || parts.first() != Some(&"package") {
        return;
    }
    let Some(file_name) = parts.last() else {
        return;
    };
    if !file_name.ends_with(".mk") {
        return;
    }
    let package_name = parts[parts.len().saturating_sub(2)];
    let stem = file_name.trim_end_matches(".mk");
    if package_name == stem && !package_name.is_empty() {
        seeds.push(PromptSeed::simple(
            PromptSeedKind::TextToken,
            package_name.to_string(),
            true,
        ));
    }
}

fn extract_support_script_name_from_path(path: &str, seeds: &mut Vec<PromptSeed>) {
    let normalized = path.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    if !(lower.starts_with("support/scripts/") || lower.starts_with("support/download/")) {
        return;
    }
    let Some(name) = normalized.rsplit('/').next() else {
        return;
    };
    if !name.is_empty() {
        seeds.push(PromptSeed::simple(
            PromptSeedKind::PathToken,
            name.to_string(),
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
    if lower.starts_with("support/scripts/")
        || lower.starts_with("support/download/")
        || lower.starts_with("docs/")
    {
        return true;
    }
    let has_supported_extension = [
        ".js",
        ".jsx",
        ".ts",
        ".tsx",
        ".rs",
        ".py",
        ".go",
        ".java",
        ".kt",
        ".cs",
        ".cpp",
        ".c",
        ".h",
        ".hpp",
        ".sql",
        ".json",
        ".toml",
        ".yaml",
        ".yml",
        ".md",
        ".mk",
        ".adoc",
        ".asciidoc",
        ".in",
        ".sh",
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

fn is_prompt_task_verb(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "find"
            | "plan"
            | "trace"
            | "inspect"
            | "add"
            | "where"
            | "how"
            | "fix"
            | "change"
            | "understand"
    )
}

fn looks_like_support_script_name(value: &str) -> bool {
    value.contains('-')
        && !value.contains('.')
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        && value.chars().any(|ch| ch.is_ascii_alphabetic())
}

fn is_keyword_or_common_word(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    matches!(
        value.as_str(),
        "about"
            | "after"
            | "before"
            | "break"
            | "change"
            | "class"
            | "const"
            | "consumed"
            | "defined"
            | "error"
            | "false"
            | "file"
            | "figure"
            | "from"
            | "function"
            | "handle"
            | "handles"
            | "using"
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

fn prompt_seed_graph_exact_value(seed: &PromptSeed) -> Option<String> {
    match seed.kind {
        PromptSeedKind::Symbol
        | PromptSeedKind::TestName
        | PromptSeedKind::Identifier
        | PromptSeedKind::ConfigToken
        | PromptSeedKind::FilePath
        | PromptSeedKind::LineNumber
        | PromptSeedKind::StackTrace
        | PromptSeedKind::PathToken => seed.exact_value(),
        PromptSeedKind::FilePattern
        | PromptSeedKind::TextToken
        | PromptSeedKind::ErrorMessage
        | PromptSeedKind::TaskVerbIgnored
        | PromptSeedKind::Unknown => None,
    }
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

fn rerank_reason_summary(scores: &[RerankScore]) -> Vec<String> {
    let mut reasons = BTreeSet::new();
    for score in scores {
        for (component, value) in &score.components {
            if *value > 0.0 {
                reasons.insert(component.clone());
            }
        }
    }
    reasons.into_iter().collect()
}

fn selected_vector_candidates(
    candidates: &[RetrievalCandidate],
    top_k: usize,
    task: &str,
) -> (Vec<RetrievalCandidate>, Vec<String>) {
    if top_k == 0 {
        return (
            Vec::new(),
            candidates
                .iter()
                .map(|candidate| candidate.candidate_id.clone())
                .collect(),
        );
    }

    let mut selected = candidates
        .iter()
        .filter(|candidate| candidate.candidate_source == RetrievalCandidateSource::VectorSemantic)
        .cloned()
        .map(|candidate| normalize_vector_candidate(candidate, task))
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| {
        right
            .score
            .unwrap_or(0.0)
            .total_cmp(&left.score.unwrap_or(0.0))
            .then_with(|| left.candidate_id.cmp(&right.candidate_id))
    });
    let dropped = selected
        .iter()
        .skip(top_k)
        .map(vector_candidate_stage_id)
        .collect::<Vec<_>>();
    selected.truncate(top_k);
    for (index, candidate) in selected.iter_mut().enumerate() {
        candidate.rank = Some(index + 1);
    }
    (selected, dropped)
}

fn selected_nuance_rescue_candidates(
    task: &str,
    documents: &BTreeMap<String, RetrievalDocument>,
    binary_kept_ids: &BTreeSet<String>,
    exact_seed_ids: &[String],
    top_k: usize,
) -> (Vec<RetrievalCandidate>, Vec<String>) {
    let query_tokens = nuance_rescue_query_tokens(task);
    if query_tokens.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let exact_seed_set = exact_seed_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut selected = documents
        .values()
        .filter(|document| !binary_kept_ids.contains(&document.id))
        .filter(|document| !exact_seed_set.contains(&document.id))
        .filter_map(|document| nuance_rescue_candidate_for_document(task, &query_tokens, document))
        .collect::<Vec<_>>();

    selected.sort_by(|left, right| {
        right
            .score
            .unwrap_or(0.0)
            .total_cmp(&left.score.unwrap_or(0.0))
            .then_with(|| left.candidate_id.cmp(&right.candidate_id))
    });

    let dropped = selected
        .iter()
        .skip(top_k)
        .map(|candidate| candidate.candidate_id.clone())
        .collect::<Vec<_>>();
    selected.truncate(top_k);
    for (index, candidate) in selected.iter_mut().enumerate() {
        candidate.rank = Some(index + 1);
    }
    (selected, dropped)
}

fn nuance_rescue_candidate_for_document(
    task: &str,
    query_tokens: &BTreeSet<String>,
    document: &RetrievalDocument,
) -> Option<RetrievalCandidate> {
    let document_tokens = nuance_rescue_document_tokens(document);
    let matched = query_tokens
        .intersection(&document_tokens)
        .cloned()
        .collect::<Vec<_>>();
    if matched.is_empty() {
        return None;
    }

    let score = nuance_rescue_score(query_tokens, &matched, document);
    if score <= 0.0 {
        return None;
    }

    let mut candidate = RetrievalCandidate::new(
        document.id.clone(),
        RetrievalCandidateSource::NuanceRescue,
        "1-bit nuance rescue recovered this candidate by rare-token or identifier overlap; not graph proof",
    );
    candidate.matched_query_text = Some(task.to_string());
    candidate.matched_seeds = matched.clone();
    candidate.proof_status = RetrievalProofStatus::CandidateOnly;
    candidate.graph_proof = false;
    candidate.claimable = false;
    candidate.claimable_for_text = Some(false);
    candidate.claimable_for_graph = Some(false);
    candidate.score = Some(score);
    candidate.requires_graph_verification = true;
    candidate.verification_status = RetrievalVerificationStatus::NeedsGraphVerification;
    if let Some(path) = document.metadata.get("path") {
        candidate.path = Some(path.clone());
    }
    candidate.metadata.insert(
        "stage".to_string(),
        serde_json::json!("stage1_nuance_rescue"),
    );
    candidate.metadata.insert(
        "rescue_basis".to_string(),
        serde_json::json!("rare_token_identifier_overlap"),
    );
    candidate
        .metadata
        .insert("matched_tokens".to_string(), serde_json::json!(matched));
    if let Some(source) = document.metadata.get("candidate_source") {
        candidate.metadata.insert(
            "upstream_candidate_source".to_string(),
            serde_json::json!(source),
        );
    }
    Some(candidate)
}

fn nuance_rescue_query_tokens(task: &str) -> BTreeSet<String> {
    symbol_search_tokens(task)
        .into_iter()
        .filter(|token| nuance_rescue_token_allowed(token))
        .collect()
}

fn nuance_rescue_document_tokens(document: &RetrievalDocument) -> BTreeSet<String> {
    let metadata_text = document
        .metadata
        .iter()
        .map(|(key, value)| format!("{key} {value}"))
        .collect::<Vec<_>>()
        .join(" ");
    symbol_search_tokens(&format!(
        "{} {} {}",
        document.id, document.text, metadata_text
    ))
    .into_iter()
    .filter(|token| nuance_rescue_token_allowed(token))
    .collect()
}

fn nuance_rescue_token_allowed(token: &str) -> bool {
    token.len() >= 3
        && !is_keyword_or_common_word(token)
        && !matches!(
            token,
            "and"
                | "are"
                | "can"
                | "does"
                | "for"
                | "has"
                | "into"
                | "not"
                | "the"
                | "this"
                | "that"
                | "what"
                | "when"
                | "where"
                | "which"
                | "why"
        )
}

fn nuance_rescue_score(
    query_tokens: &BTreeSet<String>,
    matched_tokens: &[String],
    document: &RetrievalDocument,
) -> f64 {
    let coverage = matched_tokens.len() as f64 / query_tokens.len().max(1) as f64;
    let rare_bonus = matched_tokens
        .iter()
        .filter(|token| {
            token.len() >= 6
                || token.chars().any(|ch| ch.is_ascii_digit())
                || token.contains('_')
                || token.contains('-')
        })
        .count() as f64
        * 0.25;
    (coverage + rare_bonus + document.stage0_score.min(1.0) * 0.05).min(1.0)
}

fn normalize_vector_candidate(mut candidate: RetrievalCandidate, task: &str) -> RetrievalCandidate {
    candidate.candidate_source = RetrievalCandidateSource::VectorSemantic;
    candidate.graph_proof = false;
    candidate.claimable_for_graph = Some(false);
    candidate
        .matched_query_text
        .get_or_insert_with(|| task.to_string());

    if candidate.span.is_none() && candidate.source_span_missing_reason.is_none() {
        candidate.source_span_missing_reason =
            Some("vector candidate did not include a source span".to_string());
    }

    match candidate.embedding_source {
        Some(VectorEmbeddingSource::TextEvidence)
        | Some(VectorEmbeddingSource::Snippet)
        | Some(VectorEmbeddingSource::FilePathTitle)
            if candidate.entity_id.is_none() =>
        {
            candidate.proof_status = RetrievalProofStatus::NotGraphProof;
            candidate.requires_graph_verification = false;
            candidate.verification_status = RetrievalVerificationStatus::NotGraphProof;
            candidate.claimable = true;
            candidate.claimable_for_text = Some(true);
        }
        _ if candidate.entity_id.is_some() => {
            candidate.proof_status = RetrievalProofStatus::CandidateOnly;
            candidate.requires_graph_verification = true;
            candidate.verification_status = RetrievalVerificationStatus::NeedsGraphVerification;
            candidate.claimable = false;
            candidate.claimable_for_text.get_or_insert(false);
        }
        _ => {
            candidate.proof_status = RetrievalProofStatus::CandidateOnly;
            candidate.requires_graph_verification = true;
            candidate.verification_status = RetrievalVerificationStatus::NeedsGraphVerification;
            candidate.claimable = false;
            candidate.claimable_for_text.get_or_insert(false);
        }
    }

    if let Some(binding) = &candidate.lifecycle_binding {
        if binding.status != RetrievalCandidateLifecycleStatus::Fresh {
            candidate.verification_status = RetrievalVerificationStatus::StaleOrForeignDb;
            candidate.proof_status = RetrievalProofStatus::StaleOrForeignDb;
            candidate.claimable = false;
            candidate.claimable_for_text = Some(false);
            candidate.claimable_for_graph = Some(false);
        }
    }

    candidate
}

fn vector_candidate_stage_id(candidate: &RetrievalCandidate) -> String {
    candidate
        .entity_id
        .clone()
        .unwrap_or_else(|| candidate.candidate_id.clone())
}

fn vector_candidate_document(candidate: &RetrievalCandidate) -> RetrievalDocument {
    let id = vector_candidate_stage_id(candidate);
    let text = candidate
        .metadata
        .get("chunk_text")
        .and_then(|value| value.as_str())
        .or(candidate.matched_query_text.as_deref())
        .or(candidate.path.as_deref())
        .unwrap_or(&candidate.candidate_id)
        .to_string();
    let mut document =
        RetrievalDocument::new(id, text).stage0_score(candidate.score.unwrap_or(0.0));
    document.metadata.insert(
        "candidate_source".to_string(),
        "vector_semantic".to_string(),
    );
    if let Some(chunk_id) = &candidate.chunk_id {
        document
            .metadata
            .insert("chunk_id".to_string(), chunk_id.clone());
    }
    if let Some(path) = &candidate.path {
        document.metadata.insert("path".to_string(), path.clone());
    }
    if matches!(
        candidate.embedding_source,
        Some(VectorEmbeddingSource::TextEvidence)
            | Some(VectorEmbeddingSource::Snippet)
            | Some(VectorEmbeddingSource::FilePathTitle)
    ) {
        document
            .metadata
            .insert("text_evidence_match".to_string(), "true".to_string());
        document
            .metadata
            .insert("evidence_role".to_string(), "text_evidence".to_string());
    }
    document
}

fn vector_candidate_counts_json(
    exact_seed_count: usize,
    stage0_count: usize,
    vector_count: usize,
    nuance_rescue_count: usize,
) -> serde_json::Value {
    serde_json::json!({
        "exact_seed": exact_seed_count,
        "stage0_candidate": stage0_count,
        "vector_semantic": vector_count,
        "nuance_rescue": nuance_rescue_count,
    })
}

fn vector_candidate_trace_json(
    vector_enabled: bool,
    index_status: &VectorCandidateBranchStatus,
    query_text: &str,
    supplied_candidates: &[RetrievalCandidate],
    accepted_candidates: &[RetrievalCandidate],
    rejected_candidate_ids: &[String],
    warnings: &[String],
    graph_verified_stage_ids: &[String],
    no_proof_fallback_reason: Option<&str>,
) -> serde_json::Value {
    let supplied_vector_count = supplied_candidates
        .iter()
        .filter(|candidate| candidate.candidate_source == RetrievalCandidateSource::VectorSemantic)
        .count();
    let rejected_count = if vector_enabled && index_status == &VectorCandidateBranchStatus::Ready {
        rejected_candidate_ids.len()
    } else if vector_enabled || supplied_vector_count > 0 {
        supplied_vector_count
    } else {
        0
    };
    let (query_text, query_text_redacted, query_text_truncated) =
        redacted_vector_trace_text(query_text, 160);
    let stale_or_missing_reason =
        vector_trace_status_reason(vector_enabled, index_status, warnings);
    let graph_verified = graph_verified_stage_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let graph_verified_candidate_ids = accepted_candidates
        .iter()
        .filter_map(|candidate| {
            let stage_id = vector_candidate_stage_id(candidate);
            graph_verified
                .contains(&stage_id)
                .then(|| candidate.candidate_id.clone())
        })
        .take(8)
        .collect::<Vec<_>>();

    serde_json::json!({
        "schema_version": 1,
        "diagnostic_only": true,
        "vector_enabled": vector_enabled,
        "vector_index_status": index_status.as_str(),
        "provider": vector_candidate_provider_json(supplied_candidates, accepted_candidates),
        "query_text_sent_to_vector_branch": query_text,
        "query_text_redacted": query_text_redacted,
        "query_text_truncated": query_text_truncated,
        "chunk_count_searched": supplied_vector_count,
        "vector_candidate_count": accepted_candidates.len(),
        "top_vector_candidate_ids": accepted_candidates
            .iter()
            .take(8)
            .map(|candidate| candidate.candidate_id.clone())
            .collect::<Vec<_>>(),
        "score_range": vector_candidate_score_range_json(accepted_candidates),
        "candidates_accepted": {
            "count": accepted_candidates.len(),
            "candidate_ids": accepted_candidates
                .iter()
                .take(8)
                .map(|candidate| candidate.candidate_id.clone())
                .collect::<Vec<_>>()
        },
        "candidates_rejected": {
            "count": rejected_count,
            "candidate_ids": rejected_candidate_ids
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
        },
        "stale_missing_vector_index_reason": stale_or_missing_reason,
        "graph_verification_status_for_vector_candidates": vector_graph_verification_trace_json(
            accepted_candidates,
            &graph_verified_candidate_ids
        ),
        "no_proof_fallback_reason": no_proof_fallback_reason
            .map(|reason| redacted_vector_trace_text(reason, 240).0)
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
        "proof_contract": "vector candidates are candidate recall only and are not graph proof unless graph/source verification succeeds"
    })
}

fn vector_candidate_provider_json(
    supplied_candidates: &[RetrievalCandidate],
    accepted_candidates: &[RetrievalCandidate],
) -> serde_json::Value {
    let candidate = accepted_candidates
        .first()
        .or_else(|| supplied_candidates.first());
    serde_json::json!({
        "provider_id": candidate
            .and_then(|candidate| candidate.metadata.get("provider_id"))
            .and_then(|value| value.as_str())
            .unwrap_or("unknown"),
        "model_id": candidate
            .and_then(|candidate| candidate.embedding_model_id.as_deref())
            .unwrap_or("unknown"),
        "dimension": candidate.and_then(|candidate| candidate.embedding_dim),
        "embedding_profile": candidate
            .and_then(|candidate| candidate.embedding_profile.as_deref())
            .unwrap_or("unknown"),
        "embedding_kind": "deterministic_token_projection",
        "display_label": "deterministic token-projection candidate recall",
        "learned_semantic_embeddings": false,
        "production_semantic_quality": false
    })
}

fn vector_candidate_score_range_json(candidates: &[RetrievalCandidate]) -> serde_json::Value {
    let mut scores = candidates
        .iter()
        .filter_map(|candidate| candidate.score)
        .filter(|score| score.is_finite())
        .collect::<Vec<_>>();
    if scores.is_empty() {
        return serde_json::Value::Null;
    }
    scores.sort_by(f64::total_cmp);
    serde_json::json!({
        "min": scores[0],
        "max": scores[scores.len() - 1]
    })
}

fn vector_graph_verification_trace_json(
    candidates: &[RetrievalCandidate],
    graph_verified_candidate_ids: &[String],
) -> serde_json::Value {
    let mut status_counts = BTreeMap::<String, usize>::new();
    for candidate in candidates {
        *status_counts
            .entry(retrieval_verification_status_label(
                candidate.verification_status,
            ))
            .or_default() += 1;
    }
    serde_json::json!({
        "status_counts": status_counts,
        "requires_graph_verification_count": candidates
            .iter()
            .filter(|candidate| candidate.requires_graph_verification)
            .count(),
        "graph_verified_count": graph_verified_candidate_ids.len(),
        "graph_verified_candidate_ids": graph_verified_candidate_ids,
        "graph_proof_count": candidates
            .iter()
            .filter(|candidate| candidate.graph_proof)
            .count()
    })
}

fn retrieval_verification_status_label(status: RetrievalVerificationStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn vector_trace_status_reason(
    vector_enabled: bool,
    index_status: &VectorCandidateBranchStatus,
    warnings: &[String],
) -> serde_json::Value {
    let reason = if !vector_enabled {
        Some("vector branch disabled by request/config".to_string())
    } else {
        match index_status {
            VectorCandidateBranchStatus::Missing => {
                Some("vector index missing; continuing without vector candidates".to_string())
            }
            VectorCandidateBranchStatus::Ready => None,
            VectorCandidateBranchStatus::Stale { reason } => Some(format!(
                "vector index stale or incompatible; continuing without vector candidates: {reason}"
            )),
        }
    }
    .or_else(|| warnings.first().cloned());

    reason
        .map(|reason| redacted_vector_trace_text(&reason, 240).0)
        .map(serde_json::Value::from)
        .unwrap_or(serde_json::Value::Null)
}

fn redacted_vector_trace_text(text: &str, max_chars: usize) -> (String, bool, bool) {
    let mut redacted = false;
    let tokens = text
        .split_whitespace()
        .map(|token| {
            if vector_trace_token_looks_secret(token) {
                redacted = true;
                "[redacted]".to_string()
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>();
    let joined = tokens.join(" ");
    let char_count = joined.chars().count();
    if char_count <= max_chars {
        return (joined, redacted, false);
    }
    let mut truncated = joined.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    (truncated, redacted, true)
}

fn vector_trace_token_looks_secret(token: &str) -> bool {
    let lower = token
        .trim_matches(|ch: char| ch == '"' || ch == '\'' || ch == ',' || ch == ';')
        .to_ascii_lowercase();
    lower.starts_with("sk-")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("token=")
        || lower.contains("password=")
        || lower.contains("secret=")
        || lower.contains("authorization:")
        || lower.starts_with("bearer ")
}

fn vector_text_fallback_snippet(candidate: &RetrievalCandidate) -> Option<ContextSnippet> {
    if !matches!(
        candidate.embedding_source,
        Some(VectorEmbeddingSource::TextEvidence)
            | Some(VectorEmbeddingSource::Snippet)
            | Some(VectorEmbeddingSource::FilePathTitle)
    ) || candidate.entity_id.is_some()
    {
        return None;
    }
    let file = candidate
        .path
        .clone()
        .or_else(|| candidate.file_id.clone())?;
    let lines = candidate
        .span
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "span unavailable".to_string());
    let text = candidate
        .metadata
        .get("chunk_text")
        .and_then(|value| value.as_str())
        .or(candidate.matched_query_text.as_deref())
        .unwrap_or("")
        .to_string();
    Some(ContextSnippet {
        file,
        lines,
        text,
        reason: "deterministic token-projection text-evidence candidate; no graph proof"
            .to_string(),
    })
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

fn rank_context_packet_paths(paths: &mut [GraphPath]) {
    paths.sort_by(|left, right| {
        context_packet_path_priority(left)
            .cmp(&context_packet_path_priority(right))
            .then_with(|| right.steps.len().cmp(&left.steps.len()))
            .then_with(|| left.cost.total_cmp(&right.cost))
            .then_with(|| left.target.cmp(&right.target))
            .then_with(|| left.edge_ids().cmp(&right.edge_ids()))
    });
}

fn context_packet_path_priority(path: &GraphPath) -> u8 {
    if path.steps.iter().any(|step| {
        matches!(
            step.edge.relation,
            RelationKind::Writes
                | RelationKind::Mutates
                | RelationKind::MayMutate
                | RelationKind::WritesTable
                | RelationKind::SchemaImpact
        )
    }) {
        0
    } else if path.steps.iter().any(|step| {
        matches!(
            step.edge.relation,
            RelationKind::Authorizes
                | RelationKind::ChecksRole
                | RelationKind::ChecksPermission
                | RelationKind::Sanitizes
                | RelationKind::Validates
        )
    }) {
        1
    } else if path.contains_relation(RelationKind::Calls) {
        2
    } else {
        3
    }
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

fn split_mixed_paths_for_context_mode(
    paths: Vec<GraphPath>,
    mode: &str,
) -> (Vec<GraphPath>, usize) {
    if context_mode_allows_test_mock_edges(mode) {
        return (paths, 0);
    }

    let mut split_count = 0usize;
    let mut output = Vec::new();
    for path in paths {
        if path.path_context() == PathContext::Mixed {
            if let Some(production) = production_subpath_for_mixed_path(&path) {
                split_count += 1;
                output.push(production);
                continue;
            }
        }
        output.push(path);
    }
    (output, split_count)
}

fn production_subpath_for_mixed_path(path: &GraphPath) -> Option<GraphPath> {
    let mut best_start = 0usize;
    let mut best_len = 0usize;
    let mut current_start = 0usize;
    let mut current_len = 0usize;

    for (index, step) in path.steps.iter().enumerate() {
        if classify_edge_context(&step.edge) == PathContext::Production {
            if current_len == 0 {
                current_start = index;
            }
            current_len += 1;
            if current_len > best_len {
                best_start = current_start;
                best_len = current_len;
            }
        } else {
            current_len = 0;
        }
    }

    if best_len == 0 || best_len == path.steps.len() {
        return None;
    }
    let steps = path.steps[best_start..best_start + best_len].to_vec();
    let source = steps.first()?.from.clone();
    let target = steps.last()?.to.clone();
    Some(GraphPath {
        source,
        target,
        steps,
        cost: path.cost,
        uncertainty: path.uncertainty,
    })
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
    let mut unknown = 0usize;
    for path in paths {
        match path.path_context() {
            PathContext::Production => production += 1,
            PathContext::Test => test += 1,
            PathContext::Mock => mock += 1,
            PathContext::Mixed => mixed += 1,
            PathContext::Unknown => unknown += 1,
        }
    }
    serde_json::json!({
        "production": production,
        "test": test,
        "mock": mock,
        "mixed": mixed,
        "unknown": unknown,
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
        if compact_packet_metadata(packet, token_budget) {
            continue;
        }
        let estimated_tokens = estimate_packet_tokens(packet);
        let last_path_overrun_limit = token_budget.saturating_mul(2).saturating_add(60);
        if packet.verified_paths.len() > 1
            || (packet.verified_paths.len() == 1 && estimated_tokens > last_path_overrun_limit)
        {
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
        if compact_packet_metadata(packet, token_budget) {
            continue;
        }
        let estimated_tokens = estimate_packet_tokens(packet);
        let last_path_overrun_limit = token_budget.saturating_mul(2).saturating_add(60);
        if packet.verified_paths.len() > 1
            || (packet.verified_paths.len() == 1 && estimated_tokens > last_path_overrun_limit)
        {
            packet.verified_paths.pop();
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

fn compact_packet_metadata(packet: &mut ContextPacket, token_budget: usize) -> bool {
    const VERBOSE_METADATA_KEYS: &[&str] = &[
        "traversal_telemetry",
        "prompt_seed_provenance",
        "prompt_seeds",
        "path_context_counts_before_filter",
        "path_context_counts_after_filter",
    ];

    let mut omitted = packet
        .metadata
        .get("omitted_metadata_keys")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut changed = false;

    for key in VERBOSE_METADATA_KEYS {
        if estimate_packet_tokens(packet) <= token_budget {
            break;
        }
        if packet.metadata.remove(*key).is_some() {
            omitted.push(serde_json::json!(key));
            changed = true;
        }
    }

    if changed {
        packet
            .metadata
            .insert("metadata_truncated".to_string(), serde_json::json!(true));
        packet.metadata.insert(
            "omitted_metadata_keys".to_string(),
            serde_json::json!(omitted),
        );
    }

    changed
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

    fn edge_with_evidence_role(
        head: &str,
        relation: RelationKind,
        tail: &str,
        span: SourceSpan,
        role: EvidenceRole,
        reason: &str,
    ) -> Edge {
        let mut edge = edge_with_span(head, relation, tail, span);
        edge.context = match role {
            EvidenceRole::Production => EdgeContext::Production,
            EvidenceRole::Test => EdgeContext::Test,
            EvidenceRole::Mock => EdgeContext::Mock,
            EvidenceRole::Mixed => EdgeContext::Mixed,
            EvidenceRole::Unknown => EdgeContext::Unknown,
        };
        edge.metadata
            .insert("source_role".to_string(), role.as_str().into());
        edge.metadata
            .insert("evidence_role".to_string(), role.as_str().into());
        edge.metadata
            .insert("classification_reason".to_string(), reason.into());
        edge.metadata
            .insert("classification_source".to_string(), "test_fixture".into());
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
    fn single_seed_relation_hydration_fallback_passed() {
        let source = "import { checkRole } from \"./auth\";\nexport const ok = true;\n";
        let edge = edge_with_span(
            "fixtures/imports.ts",
            RelationKind::Imports,
            "checkRole",
            SourceSpan::with_columns("fixtures/imports.ts", 1, 1, 1, 36),
        );
        let engine = ExactGraphQueryEngine::new(vec![edge]);
        let packet = engine.context_pack(
            ContextPackRequest::new(
                "Trace fixtures/imports.ts imports",
                "impact",
                4_000,
                vec!["fixtures/imports.ts".to_string()],
            ),
            &single_source("fixtures/imports.ts", source),
        );
        let path = packet
            .verified_paths
            .iter()
            .find(|path| path.metapath == vec![RelationKind::Imports])
            .expect("single-hop import path evidence");

        assert_eq!(path.length, 1);
        assert_eq!(path.exactness, Exactness::ParserVerified);
        assert_eq!(
            path.metadata
                .get("hydration")
                .and_then(|value| value.as_str()),
            Some("demand_driven_one_hop")
        );
        assert_eq!(
            path.metadata
                .get("single_seed_relation_hydration")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            path.metadata
                .get("proof_grade_source_spans")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert!(packet
            .snippets
            .iter()
            .any(|snippet| snippet.text.contains("checkRole")));
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
    fn prompt_seed_extraction_extracts_buildroot_config_tokens_without_prompt_verbs() {
        let seeds = extract_prompt_seeds("Find where BR2_PACKAGE_OPENSSL is defined and consumed");

        assert_seed(&seeds, PromptSeedKind::ConfigToken, "BR2_PACKAGE_OPENSSL");
        assert_ignored_seed(&seeds, "Find");
        assert_ignored_seed(&seeds, "where");
        assert_no_exact_symbol_seed(&seeds, "Find");
        assert_no_exact_symbol_seed(&seeds, "where");
        assert_no_exact_symbol_seed(&seeds, "defined");
        assert_no_exact_symbol_seed(&seeds, "consumed");
    }

    #[test]
    fn prompt_intent_classifies_buildroot_config_and_build_system_prompts() {
        let config = extract_prompt_seed_provenance(
            "Find where BR2_PACKAGE_OPENSSL is selected in Config.in",
        );
        assert_eq!(config.intent, PromptIntent::TextConfigLookup);
        assert!(config.seeds.iter().any(|seed| {
            seed.value == "BR2_PACKAGE_OPENSSL"
                && seed.kind == PromptSeedKind::ConfigToken
                && seed.intent_contribution == PromptIntent::TextConfigLookup
                && seed.source_text == "BR2_PACKAGE_OPENSSL"
        }));

        let build = extract_prompt_seed_provenance(
            "Plan how to add a new Buildroot package .mk using generic-package",
        );
        assert_eq!(build.intent, PromptIntent::BuildSystemPlanning);
        assert!(build.seeds.iter().any(|seed| {
            seed.value == "generic-package"
                && seed.intent_contribution == PromptIntent::BuildSystemPlanning
        }));
    }

    #[test]
    fn prompt_intent_classifies_trace_test_dataflow_auth_docs_and_unknown() {
        assert_eq!(
            classify_prompt_intent("Trace AuthService.login through token creation"),
            PromptIntent::BehaviorTrace
        );
        assert_eq!(
            classify_prompt_intent("Which callers and callees touch AuthService.login?"),
            PromptIntent::CallerCalleeTrace
        );
        assert_eq!(
            classify_prompt_intent("What tests cover test(\"normalizes email\")?"),
            PromptIntent::TestImpact
        );
        assert_eq!(
            classify_prompt_intent("Trace dataflow from request.email to users.email"),
            PromptIntent::DataflowTrace
        );
        assert_eq!(
            classify_prompt_intent("Trace auth permission checks for adminRoute"),
            PromptIntent::SecurityAuthTrace
        );
        assert_eq!(
            classify_prompt_intent("Find docs for package infrastructure"),
            PromptIntent::BuildSystemPlanning
        );
        assert_eq!(
            classify_prompt_intent("Find docs explaining the CLI output"),
            PromptIntent::DocsText
        );
        assert_eq!(classify_prompt_intent("please help"), PromptIntent::Unknown);
    }

    #[test]
    fn prompt_seed_provenance_labels_ignored_generic_task_verbs() {
        let extraction = extract_prompt_seed_provenance("Trace Find Plan Inspect Add Where How");
        for value in ["Trace", "Find", "Plan", "Inspect", "Add", "Where", "How"] {
            let seed = extraction
                .seeds
                .iter()
                .find(|seed| seed.value.eq_ignore_ascii_case(value))
                .unwrap_or_else(|| panic!("missing ignored task verb {value}"));
            assert_eq!(seed.kind, PromptSeedKind::TaskVerbIgnored);
            assert_eq!(seed.exactness, PromptSeedExactness::Ignored);
            assert!(!seed.exact);
            assert!(seed
                .ignored_reason
                .as_deref()
                .is_some_and(|reason| { reason.contains("not an exact symbol") }));
            assert_no_exact_symbol_seed(&extraction.seeds, value);
        }

        let json = extraction.provenance_json();
        assert_eq!(json["intent"].as_str(), Some("behavior_trace"));
        assert!(json["seeds"].as_array().is_some_and(|seeds| {
            seeds.iter().any(|seed| {
                seed["seed"].as_str() == Some("Trace")
                    && seed["seed_kind"].as_str() == Some("task_verb_ignored")
                    && seed["ignored_reason"]
                        .as_str()
                        .is_some_and(|reason| reason.contains("not an exact symbol"))
            })
        }));
    }

    #[test]
    fn prompt_seed_extraction_extracts_build_system_file_patterns() {
        let seeds = extract_prompt_seeds("Plan how to add a package .mk using generic-package");

        assert_seed(&seeds, PromptSeedKind::FilePattern, ".mk");
        assert_seed(&seeds, PromptSeedKind::TextToken, "generic-package");
        assert_ignored_seed(&seeds, "Plan");
        assert_ignored_seed(&seeds, "how");
        assert_ignored_seed(&seeds, "add");
        assert_no_exact_symbol_seed(&seeds, "Plan");
        assert_no_exact_symbol_seed(&seeds, "how");
        assert_no_exact_symbol_seed(&seeds, "add");
    }

    #[test]
    fn prompt_seed_extraction_extracts_kconfig_phrases_and_doc_terms() {
        let config_seeds = extract_prompt_seeds("Inspect Config.in for depends on");

        assert_seed(&config_seeds, PromptSeedKind::ConfigToken, "Config.in");
        assert_seed(&config_seeds, PromptSeedKind::TextToken, "depends on");
        assert_ignored_seed(&config_seeds, "Inspect");
        assert_no_exact_symbol_seed(&config_seeds, "Inspect");

        let docs_seeds = extract_prompt_seeds("Find docs explaining package infrastructure");
        assert_seed(&docs_seeds, PromptSeedKind::TextToken, "docs");
        assert_seed(
            &docs_seeds,
            PromptSeedKind::TextToken,
            "package infrastructure",
        );
        assert_ignored_seed(&docs_seeds, "Find");
    }

    #[test]
    fn prompt_seed_extraction_extracts_buildroot_paths_package_names_and_support_scripts() {
        let seeds = extract_prompt_seeds(
            "Inspect package/foo/foo.mk and support/scripts/pkg-stats plus docs/manual/adding-packages.adoc",
        );

        assert_seed(&seeds, PromptSeedKind::FilePath, "package/foo/foo.mk");
        assert_seed(&seeds, PromptSeedKind::TextToken, "foo");
        assert_seed(
            &seeds,
            PromptSeedKind::PathToken,
            "support/scripts/pkg-stats",
        );
        assert_seed(&seeds, PromptSeedKind::PathToken, "pkg-stats");
        assert_seed(
            &seeds,
            PromptSeedKind::FilePath,
            "docs/manual/adding-packages.adoc",
        );
        assert_seed(&seeds, PromptSeedKind::FilePattern, ".mk");
        assert_seed(&seeds, PromptSeedKind::FilePattern, ".adoc");
    }

    #[test]
    fn prompt_seed_extraction_preserves_normal_symbol_query_seeds() {
        let seeds = extract_prompt_seeds(
            "Change AuthService.login after test(\"normalizes email\") failed in normalizeEmail",
        );

        assert_seed(&seeds, PromptSeedKind::Symbol, "AuthService.login");
        assert_seed(&seeds, PromptSeedKind::Identifier, "normalizeEmail");
        assert_seed(&seeds, PromptSeedKind::TestName, "normalizes email");
        assert!(seeds
            .iter()
            .any(|seed| seed.kind.as_str() == "exact_symbol" && seed.exact));
    }

    #[test]
    fn task_intent_buildroot_prompt_produces_package_authoring_atoms() {
        let (intent, profile, plan) =
            plan_task_retrieval("Trace Buildroot generic package flow for adding a new package.");

        assert_eq!(intent.task_kind, TaskKind::BuildSystemPackageAuthoring);
        assert_eq!(profile.profile_id, "build_system_package_authoring");
        assert!(intent
            .signals
            .iter()
            .any(|signal| signal.contains("generic-package")));
        assert_atom_roles(
            &plan,
            &[
                "authoring_docs",
                "package_metadata",
                "kconfig_wiring",
                "makefile_inclusion",
                "download_infrastructure",
                "build_install_infrastructure",
                "support_scripts",
                "examples",
            ],
        );
        assert!(plan.query_atoms.iter().any(|atom| {
            atom.role == "authoring_docs" && atom.path_hints.contains(&"docs/manual".to_string())
        }));
    }

    #[test]
    fn task_intent_codegraph_lifecycle_prompt_selects_internal_debug() {
        let (intent, profile, plan) =
            plan_task_retrieval("Trace indexing entry point and DB lifecycle guards.");

        assert_eq!(intent.task_kind, TaskKind::CodegraphInternalDebug);
        assert_eq!(profile.profile_id, "codegraph_internal_debug");
        assert_atom_roles(
            &plan,
            &[
                "entrypoint_symbols",
                "lifecycle_preflight",
                "store_open",
                "status_doctor",
                "tests",
            ],
        );
    }

    #[test]
    fn task_intent_implementation_trace_prompt_selects_implementation_trace() {
        let (intent, profile, plan) =
            plan_task_retrieval("Trace implementation for AuthService.login callers and helpers.");

        assert_eq!(intent.task_kind, TaskKind::ImplementationTrace);
        assert_eq!(profile.profile_id, "implementation_trace");
        assert_atom_roles(
            &plan,
            &[
                "definitions",
                "same_file_helpers",
                "relevant_structs",
                "relevant_constants",
                "callers",
                "callees",
                "related_tests",
                "persistence_path",
                "accounting_summary",
                "artifact_inspection_requirements",
                "db_inspection_requirements",
            ],
        );
    }

    #[test]
    fn task_intent_vector_metric_prompt_selects_accounting_atoms() {
        let (intent, _profile, plan) = plan_task_retrieval(
            "Trace vector chunk index build accounting and persisted vector index size math with actual_index_file_bytes and estimated_f32_payload_bytes.",
        );

        assert!(matches!(
            intent.task_kind,
            TaskKind::StorageAccountingTrace | TaskKind::ArtifactMathTrace
        ));
        assert!(intent
            .signals
            .iter()
            .any(|signal| signal.contains("actual_index_file_bytes")));
        assert!(intent
            .signals
            .iter()
            .any(|signal| signal.contains("estimated_f32_payload_bytes")));
        assert_atom_roles(
            &plan,
            &[
                "vector_chunk_index_build",
                "vector_chunk_selection",
                "metric_reporting",
                "artifact_writer",
                "persisted_index_summary",
                "tests",
                "release_smoke_report_surfaces",
            ],
        );
        let metric_atom = plan
            .query_atoms
            .iter()
            .find(|atom| atom.role == "metric_reporting")
            .expect("metric_reporting atom");
        assert!(metric_atom.query_text.contains("actual_index_file_bytes"));
        assert!(metric_atom
            .query_text
            .contains("estimated_f32_payload_bytes"));
    }

    #[test]
    fn task_intent_test_impact_prompt_selects_test_impact() {
        let (intent, profile, plan) =
            plan_task_retrieval("Find test impact for changing a helper used by auth tests.");

        assert_eq!(intent.task_kind, TaskKind::TestImpact);
        assert_eq!(profile.profile_id, "test_impact");
        assert_atom_roles(
            &plan,
            &[
                "changed_symbol",
                "production_target",
                "test_files",
                "mocks_assertions",
                "test_impact_fallback",
            ],
        );
    }

    #[test]
    fn task_intent_dataflow_prompt_selects_dataflow() {
        let (intent, profile, plan) =
            plan_task_retrieval("Trace request input flow to a database write.");

        assert_eq!(intent.task_kind, TaskKind::DataflowTrace);
        assert_eq!(profile.profile_id, "dataflow_trace");
        assert_atom_roles(
            &plan,
            &[
                "source",
                "sink",
                "intermediate_helper",
                "sanitizer",
                "mutation_write",
                "proof_path_attempt",
            ],
        );
    }

    #[test]
    fn task_intent_security_prompt_selects_security_review() {
        let (intent, profile, plan) =
            plan_task_retrieval("Find where role checks authorize admin-only behavior.");

        assert_eq!(intent.task_kind, TaskKind::SecurityReview);
        assert_eq!(profile.profile_id, "security_review");
        assert_atom_roles(
            &plan,
            &[
                "auth_entrypoint",
                "role_check",
                "permission_gate",
                "sanitizer_validator",
                "route_expose",
                "tests",
            ],
        );
    }

    #[test]
    fn task_intent_docs_prompt_selects_docs_lookup() {
        let (intent, profile, plan) =
            plan_task_retrieval("Find docs explaining package infrastructure.");

        assert_eq!(intent.task_kind, TaskKind::DocsLookup);
        assert_eq!(profile.profile_id, "docs_lookup");
        assert_atom_roles(
            &plan,
            &[
                "docs_manual",
                "readme_docs",
                "config_reference_files",
                "related_source_if_implementation",
            ],
        );
    }

    #[test]
    fn task_intent_ambiguous_prompt_uses_unknown_fallback() {
        let (intent, profile, plan) = plan_task_retrieval("Figure out what handles this thing.");

        assert_eq!(intent.task_kind, TaskKind::Unknown);
        assert_eq!(profile.profile_id, "unknown_fallback");
        assert_eq!(intent.ambiguity, "high");
        assert_atom_roles(&plan, &["text_evidence_lookup", "file_lookup"]);
        assert!(plan
            .proof_attempt_policy
            .contains("no_exact_graph_proof_expectation"));
    }

    #[test]
    fn task_intent_generic_verbs_are_ignored_as_exact_symbols() {
        let intent =
            parse_task_intent("Find trace plan inspect how where add fix change understand");

        for value in [
            "find",
            "trace",
            "plan",
            "inspect",
            "how",
            "where",
            "add",
            "fix",
            "change",
            "understand",
        ] {
            assert!(
                intent.ignored_terms.iter().any(|term| term == value),
                "missing ignored term {value:?} in {:?}",
                intent.ignored_terms
            );
            assert!(
                !intent
                    .exact_seeds
                    .iter()
                    .any(|seed| seed.eq_ignore_ascii_case(value)),
                "generic verb became exact seed: {value}"
            );
        }
    }

    #[test]
    fn task_intent_preserves_file_paths_and_symbols_as_seeds() {
        let intent = parse_task_intent(
            "Inspect src/auth/login.ts and AuthService.login with BR2_PACKAGE_OPENSSL.",
        );

        assert!(intent
            .file_path_seeds
            .contains(&"src/auth/login.ts".to_string()));
        assert!(intent
            .exact_seeds
            .contains(&"AuthService.login".to_string()));
        assert!(intent
            .config_keys
            .contains(&"BR2_PACKAGE_OPENSSL".to_string()));
    }

    #[test]
    fn retrieval_plan_atoms_are_precise_not_broad_context_pulls() {
        let (_intent, _profile, plan) =
            plan_task_retrieval("Trace request input flow to a database write.");

        assert!(!plan.query_atoms.is_empty());
        for atom in &plan.query_atoms {
            assert!(
                atom.max_candidates <= 4,
                "atom {} pulls too many candidates",
                atom.role
            );
            assert!(
                !atom.query_text.eq_ignore_ascii_case("context")
                    && !atom.query_text.eq_ignore_ascii_case("repo")
                    && !atom.query_text.to_ascii_lowercase().contains("whole repo"),
                "atom {} is too broad: {}",
                atom.role,
                atom.query_text
            );
        }
    }

    fn assert_seed(seeds: &[PromptSeed], kind: PromptSeedKind, value: &str) {
        assert!(
            seeds
                .iter()
                .any(|seed| seed.kind == kind && seed.value == value),
            "missing seed kind={} value={value:?} in {seeds:?}",
            kind.as_str()
        );
    }

    fn assert_ignored_seed(seeds: &[PromptSeed], value: &str) {
        assert!(
            seeds.iter().any(|seed| {
                seed.kind == PromptSeedKind::TaskVerbIgnored
                    && seed.value.eq_ignore_ascii_case(value)
                    && !seed.exact
            }),
            "missing ignored seed {value:?} in {seeds:?}"
        );
    }

    fn assert_no_exact_symbol_seed(seeds: &[PromptSeed], value: &str) {
        assert!(
            !seeds.iter().any(|seed| {
                seed.kind.as_str() == "exact_symbol" && seed.value.eq_ignore_ascii_case(value)
            }),
            "unexpected exact symbol seed {value:?} in {seeds:?}"
        );
    }

    fn assert_atom_roles(plan: &RetrievalPlan, expected_roles: &[&str]) {
        let roles = plan
            .query_atoms
            .iter()
            .map(|atom| atom.role.as_str())
            .collect::<Vec<_>>();
        for role in expected_roles {
            assert!(
                roles.contains(role),
                "missing atom role {role:?} in {roles:?}"
            );
        }
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
    fn bounded_graph_latency_baseline_fixture_telemetry() {
        let high_degree_edges = (0..64)
            .map(|index| {
                edge(
                    "seed:module",
                    RelationKind::Contains,
                    &format!("symbol:structural_child:{index:03}"),
                    index + 1,
                )
            })
            .collect::<Vec<_>>();
        let high_degree_engine = ExactGraphQueryEngine::new(high_degree_edges);
        let high_degree_limits = QueryLimits {
            max_depth: 2,
            max_paths: 4,
            max_edges_visited: 16,
        };
        let (high_degree_paths, high_degree_telemetry) = high_degree_engine
            .bounded_bfs_with_telemetry(
                "seed:module",
                &[Traversal::forward(RelationKind::Calls)],
                high_degree_limits,
                &|path| !path.steps.is_empty(),
            );
        assert!(high_degree_paths.is_empty());
        assert_eq!(high_degree_telemetry.structural_edges_skipped, 64);

        let cycle_engine = ExactGraphQueryEngine::new(vec![
            edge("symbol:A", RelationKind::Calls, "symbol:B", 1),
            edge("symbol:B", RelationKind::Calls, "symbol:C", 2),
            edge("symbol:C", RelationKind::Calls, "symbol:A", 3),
            edge("symbol:C", RelationKind::Writes, "symbol:sink", 4),
        ]);
        let cycle_limits = QueryLimits {
            max_depth: 6,
            max_paths: 2,
            max_edges_visited: 12,
        };
        let (cycle_paths, cycle_telemetry) = cycle_engine.bounded_bfs_with_telemetry(
            "symbol:A",
            &[
                Traversal::forward(RelationKind::Calls),
                Traversal::forward(RelationKind::Writes),
            ],
            cycle_limits,
            &|path| path.target == "symbol:sink",
        );
        assert_eq!(cycle_paths.len(), 1);
        assert!(cycle_telemetry.cycles_cut >= 1);

        let mut broad_flow_edges = Vec::new();
        for index in 0..12 {
            broad_flow_edges.push(edge(
                "symbol:input",
                RelationKind::FlowsTo,
                &format!("symbol:flow_branch:{index:03}"),
                index + 10,
            ));
            broad_flow_edges.push(edge(
                &format!("symbol:flow_branch:{index:03}"),
                RelationKind::FlowsTo,
                "symbol:sink",
                index + 40,
            ));
        }
        let broad_flow_engine = ExactGraphQueryEngine::new(broad_flow_edges);
        let broad_flow_limits = QueryLimits {
            max_depth: 3,
            max_paths: 3,
            max_edges_visited: 10,
        };
        let (broad_flow_paths, broad_flow_telemetry) = broad_flow_engine
            .k_shortest_matching_with_telemetry(
                "symbol:input",
                &[Traversal::forward(RelationKind::FlowsTo)],
                broad_flow_limits,
                &|path| path.target == "symbol:sink",
            );
        assert!(broad_flow_paths.len() <= broad_flow_limits.max_paths);
        assert!(broad_flow_telemetry.budget_stop_reason.is_some());

        let mixed_engine = ExactGraphQueryEngine::new(vec![
            edge("symbol:handler", RelationKind::Calls, "symbol:service", 80),
            edge_with_evidence_role(
                "symbol:handler_test",
                RelationKind::Tests,
                "symbol:handler",
                span(81),
                EvidenceRole::Test,
                "fixture test path",
            ),
        ]);
        let (mixed_prod_paths, mixed_prod_telemetry) = mixed_engine.bounded_bfs_with_telemetry(
            "symbol:handler",
            &[Traversal::forward(RelationKind::Calls)],
            QueryLimits {
                max_depth: 3,
                max_paths: 4,
                max_edges_visited: 16,
            },
            &|path| !path.steps.is_empty(),
        );
        assert_eq!(mixed_prod_paths.len(), 1);
        assert!(!mixed_prod_telemetry.source_role_filters_applied);
        let (mixed_test_paths, mixed_test_telemetry) = mixed_engine.bounded_bfs_with_telemetry(
            "symbol:handler",
            &[Traversal::reverse(RelationKind::Tests)],
            QueryLimits {
                max_depth: 3,
                max_paths: 4,
                max_edges_visited: 16,
            },
            &|path| !path.steps.is_empty(),
        );
        assert_eq!(mixed_test_paths.len(), 1);

        let heuristic_engine = ExactGraphQueryEngine::new(vec![heuristic_edge(
            "symbol:caller",
            RelationKind::Calls,
            "symbol:unresolved_target",
            90,
        )]);
        let (heuristic_paths, heuristic_telemetry) = heuristic_engine.bounded_bfs_with_telemetry(
            "symbol:caller",
            &[Traversal::forward(RelationKind::Calls)],
            QueryLimits {
                max_depth: 2,
                max_paths: 2,
                max_edges_visited: 8,
            },
            &|path| !path.steps.is_empty(),
        );
        assert_eq!(heuristic_paths.len(), 1);
        assert_eq!(heuristic_telemetry.heuristic_edges_seen, 1);
        assert_eq!(heuristic_telemetry.heuristic_edges_skipped, 0);

        let candidate_engine = ExactGraphQueryEngine::new(vec![edge(
            "symbol:handler",
            RelationKind::Calls,
            "symbol:service",
            100,
        )]);
        let (candidate_paths, mut candidate_telemetry) = candidate_engine
            .bounded_bfs_with_telemetry(
                "symbol:handler",
                &[Traversal::forward(RelationKind::Calls)],
                QueryLimits {
                    max_depth: 3,
                    max_paths: 2,
                    max_edges_visited: 12,
                },
                &|path| !path.steps.is_empty(),
            );
        assert_eq!(candidate_paths.len(), 1);
        candidate_telemetry
            .candidate_count_by_source
            .insert("exact_seed".to_string(), 1);
        candidate_telemetry
            .candidate_count_by_source
            .insert("text_evidence".to_string(), 1);
        candidate_telemetry
            .candidate_count_by_source
            .insert("vector_semantic".to_string(), 1);
        candidate_telemetry
            .candidate_count_by_source
            .insert("binary_vector".to_string(), 1);
        candidate_telemetry
            .candidate_count_by_source
            .insert("nuance_rescue".to_string(), 1);

        let mut derived_with_provenance = edge(
            "symbol:controller",
            RelationKind::Authorizes,
            "symbol:policy",
            110,
        );
        derived_with_provenance.derived = true;
        derived_with_provenance.edge_class = EdgeClass::Derived;
        derived_with_provenance.exactness = Exactness::DerivedFromVerifiedEdges;
        derived_with_provenance.provenance_edges = vec![
            "edge:controller:CALLS:policy".to_string(),
            "edge:policy:CHECKS_ROLE:admin".to_string(),
        ];
        let mut derived_missing_provenance = edge(
            "symbol:controller",
            RelationKind::Authorizes,
            "symbol:unproven_policy",
            111,
        );
        derived_missing_provenance.derived = true;
        derived_missing_provenance.edge_class = EdgeClass::Derived;
        derived_missing_provenance.exactness = Exactness::DerivedFromVerifiedEdges;
        let derived_engine =
            ExactGraphQueryEngine::new(vec![derived_with_provenance, derived_missing_provenance]);
        let (derived_paths, derived_telemetry) = derived_engine.bounded_bfs_with_telemetry(
            "symbol:controller",
            &[Traversal::forward(RelationKind::Authorizes)],
            QueryLimits {
                max_depth: 2,
                max_paths: 4,
                max_edges_visited: 8,
            },
            &|path| !path.steps.is_empty(),
        );
        assert_eq!(derived_paths.len(), 2);
        assert_eq!(derived_telemetry.derived_edge_provenance_checks, 2);
        assert_eq!(derived_telemetry.derived_edge_missing_provenance, 1);

        let baseline = serde_json::json!({
            "schema_version": 1,
            "fixture": "fixtures/bounded_graph_walking/manifest.json",
            "cases": {
                "high_degree_structural_budget": high_degree_telemetry.to_json(),
                "cycle_graph_terminates": cycle_telemetry.to_json(),
                "broad_flow_budget": broad_flow_telemetry.to_json(),
                "mixed_production_test_roles": {
                    "production_default": mixed_prod_telemetry.to_json(),
                    "test_impact": mixed_test_telemetry.to_json()
                },
                "heuristic_unresolved_blocked": heuristic_telemetry.to_json(),
                "candidate_overload_exact_seed_survives": candidate_telemetry.to_json(),
                "derived_edge_provenance_required": derived_telemetry.to_json()
            },
            "notes": [
                "baseline records current traversal behavior before optimization",
                "source-role and heuristic proof filtering are not changed by this telemetry test",
                "candidate overload counts are injected to mirror the candidate handoff contract"
            ]
        });
        println!("{}", serde_json::to_string_pretty(&baseline).unwrap());
    }

    #[test]
    fn bounded_graph_traversal_controls_enforced() {
        let production_policy = TraversalPolicy::for_mode("production");
        let test_policy = TraversalPolicy::for_mode("test-impact");
        let debug_policy = TraversalPolicy::for_mode("debug/audit");

        let high_degree_edges = (0..16)
            .map(|index| {
                edge(
                    "seed:fanout",
                    RelationKind::Calls,
                    &format!("symbol:callee:{index:02}"),
                    index + 1,
                )
            })
            .collect::<Vec<_>>();
        let high_degree_engine = ExactGraphQueryEngine::new(high_degree_edges);
        let mut high_degree_limits = limits();
        high_degree_limits.max_paths = 16;
        let high_degree_policy = TraversalPolicy {
            max_neighbors_per_node: Some(4),
            ..production_policy
        };
        let (high_degree_paths, high_degree_telemetry) = high_degree_engine
            .bounded_bfs_with_policy_telemetry(
                "seed:fanout",
                &[Traversal::forward(RelationKind::Calls)],
                high_degree_limits,
                high_degree_policy,
                &|path| !path.steps.is_empty(),
            );
        assert_eq!(high_degree_paths.len(), 4);
        assert_eq!(high_degree_telemetry.neighbor_limit_hits, 1);
        assert_eq!(high_degree_telemetry.neighbors_omitted_by_limit, 12);
        assert_eq!(
            high_degree_telemetry.budget_stop_reason.as_deref(),
            Some("max_neighbors_per_node")
        );

        let structural_edges = (0..8)
            .map(|index| {
                edge(
                    "seed:module",
                    RelationKind::Contains,
                    &format!("symbol:item:{index:02}"),
                    index + 20,
                )
            })
            .collect::<Vec<_>>();
        let structural_engine = ExactGraphQueryEngine::new(structural_edges);
        let structural_policy = TraversalPolicy {
            max_structural_expansion: Some(3),
            ..debug_policy
        };
        let (structural_paths, structural_telemetry) = structural_engine
            .bounded_bfs_with_policy_telemetry(
                "seed:module",
                &[Traversal::forward(RelationKind::Contains)],
                high_degree_limits,
                structural_policy,
                &|path| !path.steps.is_empty(),
            );
        assert_eq!(structural_paths.len(), 3);
        assert_eq!(structural_telemetry.structural_expansion_limit_hits, 5);
        assert_eq!(
            structural_telemetry.budget_stop_reason.as_deref(),
            Some("max_structural_expansion")
        );

        let cycle_engine = ExactGraphQueryEngine::new(vec![
            edge("symbol:A", RelationKind::Calls, "symbol:B", 40),
            edge("symbol:B", RelationKind::Calls, "symbol:C", 41),
            edge("symbol:C", RelationKind::Calls, "symbol:A", 42),
            edge("symbol:C", RelationKind::Writes, "symbol:sink", 43),
        ]);
        let (cycle_paths, cycle_telemetry) = cycle_engine.bounded_bfs_with_policy_telemetry(
            "symbol:A",
            &[
                Traversal::forward(RelationKind::Calls),
                Traversal::forward(RelationKind::Writes),
            ],
            limits(),
            production_policy,
            &|path| path.last_relation() == Some(RelationKind::Writes),
        );
        assert_eq!(cycle_paths.len(), 1);
        assert!(cycle_telemetry.cycles_cut >= 1);

        let broad_flow_engine = ExactGraphQueryEngine::new(
            (0..10)
                .map(|index| {
                    edge(
                        "symbol:input",
                        RelationKind::FlowsTo,
                        &format!("symbol:sink:{index:02}"),
                        index + 60,
                    )
                })
                .collect(),
        );
        let mut broad_flow_limits = limits();
        broad_flow_limits.max_paths = 3;
        let (broad_flow_paths, broad_flow_telemetry) = broad_flow_engine
            .k_shortest_matching_with_policy_telemetry(
                "symbol:input",
                &[Traversal::forward(RelationKind::FlowsTo)],
                broad_flow_limits,
                production_policy,
                &|path| !path.steps.is_empty(),
            );
        assert_eq!(broad_flow_paths.len(), 3);
        assert_eq!(
            broad_flow_telemetry.budget_stop_reason.as_deref(),
            Some("max_paths")
        );

        let depth_engine = ExactGraphQueryEngine::new(vec![
            edge("symbol:depth", RelationKind::Calls, "symbol:middle", 80),
            edge("symbol:middle", RelationKind::Writes, "symbol:sink", 81),
        ]);
        let mut depth_limits = limits();
        depth_limits.max_depth = 1;
        let (depth_paths, depth_telemetry) = depth_engine.bounded_bfs_with_policy_telemetry(
            "symbol:depth",
            &[
                Traversal::forward(RelationKind::Calls),
                Traversal::forward(RelationKind::Writes),
            ],
            depth_limits,
            production_policy,
            &|path| path.last_relation() == Some(RelationKind::Writes),
        );
        assert!(depth_paths.is_empty());
        assert_eq!(depth_telemetry.result_label(), "traversal_budget_exhausted");

        let timeout_policy = TraversalPolicy {
            timeout_ms: Some(0),
            ..production_policy
        };
        let (timeout_paths, timeout_telemetry) = depth_engine.bounded_bfs_with_policy_telemetry(
            "symbol:depth",
            &[Traversal::forward(RelationKind::Calls)],
            limits(),
            timeout_policy,
            &|path| !path.steps.is_empty(),
        );
        assert!(timeout_paths.is_empty());
        assert_eq!(
            timeout_telemetry.budget_stop_reason.as_deref(),
            Some("timeout_ms")
        );
        assert_eq!(
            timeout_telemetry.to_json()["result_label"].as_str(),
            Some("traversal_budget_exhausted")
        );

        let test_edge = edge_with_evidence_role(
            "symbol:handler",
            RelationKind::Calls,
            "symbol:test_helper",
            span(100),
            EvidenceRole::Test,
            "test fixture path",
        );
        let role_engine = ExactGraphQueryEngine::new(vec![test_edge.clone()]);
        let (production_paths, production_telemetry) = role_engine
            .bounded_bfs_with_policy_telemetry(
                "symbol:handler",
                &[Traversal::forward(RelationKind::Calls)],
                limits(),
                production_policy,
                &|path| !path.steps.is_empty(),
            );
        assert!(production_paths.is_empty());
        assert_eq!(production_telemetry.source_role_blocked_edges, 1);
        assert_eq!(
            production_telemetry.to_json()["result_label"].as_str(),
            Some("traversal_source_role_blocked")
        );

        let (test_paths, test_telemetry) = role_engine.bounded_bfs_with_policy_telemetry(
            "symbol:handler",
            &[Traversal::forward(RelationKind::Calls)],
            limits(),
            test_policy,
            &|path| !path.steps.is_empty(),
        );
        assert_eq!(test_paths.len(), 1);
        assert_eq!(test_telemetry.source_role_blocked_edges, 0);
        let test_evidence = role_engine.path_evidence(&test_paths[0]);
        assert_eq!(
            test_evidence
                .metadata
                .get("evidence_role")
                .and_then(serde_json::Value::as_str),
            Some("test")
        );

        let heuristic_engine = ExactGraphQueryEngine::new(vec![heuristic_edge(
            "symbol:caller",
            RelationKind::Calls,
            "symbol:maybe",
            120,
        )]);
        let (heuristic_production_paths, heuristic_production_telemetry) = heuristic_engine
            .bounded_bfs_with_policy_telemetry(
                "symbol:caller",
                &[Traversal::forward(RelationKind::Calls)],
                limits(),
                production_policy,
                &|path| !path.steps.is_empty(),
            );
        assert!(heuristic_production_paths.is_empty());
        assert_eq!(heuristic_production_telemetry.heuristic_edges_skipped, 1);
        assert_eq!(
            heuristic_production_telemetry.result_label(),
            "traversal_heuristic_blocked"
        );

        let (heuristic_debug_paths, heuristic_debug_telemetry) = heuristic_engine
            .bounded_bfs_with_policy_telemetry(
                "symbol:caller",
                &[Traversal::forward(RelationKind::Calls)],
                limits(),
                debug_policy,
                &|path| !path.steps.is_empty(),
            );
        assert_eq!(heuristic_debug_paths.len(), 1);
        assert_eq!(heuristic_debug_telemetry.heuristic_edges_seen, 1);
        assert_eq!(
            heuristic_debug_telemetry.result_label(),
            "traversal_unknown"
        );
        let heuristic_evidence = heuristic_engine.path_evidence(&heuristic_debug_paths[0]);
        assert_eq!(
            heuristic_evidence
                .metadata
                .get("production_proof_eligible")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );

        let mut derived_with_provenance = edge(
            "symbol:controller",
            RelationKind::Authorizes,
            "symbol:policy",
            140,
        );
        derived_with_provenance.derived = true;
        derived_with_provenance.edge_class = EdgeClass::Derived;
        derived_with_provenance.exactness = Exactness::DerivedFromVerifiedEdges;
        derived_with_provenance.provenance_edges = vec!["edge:controller:CALLS:policy".into()];
        let mut derived_missing_provenance = edge(
            "symbol:controller",
            RelationKind::Authorizes,
            "symbol:unproven_policy",
            141,
        );
        derived_missing_provenance.derived = true;
        derived_missing_provenance.edge_class = EdgeClass::Derived;
        derived_missing_provenance.exactness = Exactness::DerivedFromVerifiedEdges;
        let derived_engine =
            ExactGraphQueryEngine::new(vec![derived_with_provenance, derived_missing_provenance]);
        let (derived_paths, derived_telemetry) = derived_engine.bounded_bfs_with_policy_telemetry(
            "symbol:controller",
            &[Traversal::forward(RelationKind::Authorizes)],
            limits(),
            production_policy,
            &|path| !path.steps.is_empty(),
        );
        assert_eq!(derived_paths.len(), 1);
        assert_eq!(derived_telemetry.derived_edge_provenance_checks, 2);
        assert_eq!(derived_telemetry.derived_edge_missing_provenance, 1);
        assert_eq!(derived_telemetry.derived_edge_provenance_blocked_edges, 1);

        let mut exhausted_limits = limits();
        exhausted_limits.max_edges_visited = 0;
        let (exhausted_paths, exhausted_telemetry) = high_degree_engine
            .bounded_bfs_with_policy_telemetry(
                "seed:fanout",
                &[Traversal::forward(RelationKind::Calls)],
                exhausted_limits,
                production_policy,
                &|path| !path.steps.is_empty(),
            );
        assert!(exhausted_paths.is_empty());
        assert_eq!(
            exhausted_telemetry.to_json()["result_label"].as_str(),
            Some("traversal_budget_exhausted")
        );
        assert!(exhausted_telemetry.no_proof_fallback_reason.is_some());

        let overload_stage0 = (0..100)
            .map(|index| format!("symbol:noise:{index:03}"))
            .collect::<Vec<_>>();
        let overload_engine = ExactGraphQueryEngine::new(vec![edge(
            "symbol:exact_handler",
            RelationKind::Calls,
            "symbol:service",
            160,
        )]);
        let overload_sources = BTreeMap::from([(
            "fixtures/query.ts".to_string(),
            (0..200)
                .map(|_| "fn fixture() {}")
                .collect::<Vec<_>>()
                .join("\n"),
        )]);
        let overload_packet = overload_engine.context_pack(
            ContextPackRequest::new(
                "Find symbol:exact_handler path",
                "production",
                200_000,
                vec!["symbol:exact_handler".to_string()],
            )
            .with_stage0_candidates(overload_stage0),
            &overload_sources,
        );
        assert!(overload_packet
            .symbols
            .contains(&"symbol:exact_handler".to_string()));
        assert!(!overload_packet.verified_paths.is_empty());
        let candidate_seed_count_before_cap = overload_packet
            .metadata
            .get("candidate_seed_count_before_cap")
            .and_then(serde_json::Value::as_u64)
            .expect("candidate seed count before cap");
        assert!(candidate_seed_count_before_cap >= 101);
        assert_eq!(
            overload_packet
                .metadata
                .get("candidate_seed_count_after_cap")
                .and_then(serde_json::Value::as_u64),
            Some(production_policy.max_candidate_seeds as u64)
        );
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
    fn production_context_packet_filters_inline_rust_test_evidence_by_source_role() {
        let inline_test_edge = edge_with_evidence_role(
            "src::lib.tests.calls_prod_value",
            RelationKind::Calls,
            "src::lib.prod_value",
            SourceSpan::with_columns("src/lib.rs", 13, 9, 13, 21),
            EvidenceRole::Test,
            "qualified name is inside a tests module",
        );
        let engine = ExactGraphQueryEngine::new(vec![inline_test_edge]);
        let sources = single_source(
            "src/lib.rs",
            "pub fn prod_value() -> i32 { 1 }\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn calls_prod_value() {\n        prod_value();\n    }\n}\n",
        );

        let production_packet = engine.context_pack(
            ContextPackRequest::new(
                "Change prod_value safely",
                "impact",
                2_000,
                vec!["src::lib.prod_value".to_string()],
            ),
            &sources,
        );

        assert!(production_packet.verified_paths.is_empty());
        assert_eq!(
            production_packet
                .metadata
                .get("rejected_test_mock_path_count")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );

        let test_packet = engine.context_pack(
            ContextPackRequest::new(
                "Find tests for prod_value",
                "test-impact",
                2_000,
                vec!["src::lib.prod_value".to_string()],
            ),
            &sources,
        );
        let evidence = test_packet
            .verified_paths
            .first()
            .expect("test-impact keeps inline test evidence");
        assert_eq!(
            evidence
                .metadata
                .get("evidence_role")
                .and_then(serde_json::Value::as_str),
            Some("test")
        );
        assert_eq!(
            evidence
                .metadata
                .get("classification_source")
                .and_then(serde_json::Value::as_str),
            Some("metadata")
        );
        assert!(evidence
            .metadata
            .get("classification_reason")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|reason| reason.contains("tests module")));
    }

    #[test]
    fn mixed_paths_can_split_to_production_only_subpath() {
        let production = edge_with_evidence_role(
            "api",
            RelationKind::Calls,
            "service",
            SourceSpan::with_columns("src/lib.rs", 2, 3, 2, 12),
            EvidenceRole::Production,
            "production source",
        );
        let test = edge_with_evidence_role(
            "service",
            RelationKind::Calls,
            "src::lib.tests.assert_service",
            SourceSpan::with_columns("src/lib.rs", 12, 9, 12, 25),
            EvidenceRole::Test,
            "inline test source",
        );
        let mixed = GraphPath {
            source: "api".to_string(),
            target: "src::lib.tests.assert_service".to_string(),
            steps: vec![
                TraversalStep {
                    edge: production,
                    direction: TraversalDirection::Forward,
                    from: "api".to_string(),
                    to: "service".to_string(),
                },
                TraversalStep {
                    edge: test,
                    direction: TraversalDirection::Forward,
                    from: "service".to_string(),
                    to: "src::lib.tests.assert_service".to_string(),
                },
            ],
            cost: 2.0,
            uncertainty: 0.0,
        };

        assert_eq!(mixed.path_context(), PathContext::Mixed);
        let split = production_subpath_for_mixed_path(&mixed).expect("production split");
        assert_eq!(split.source, "api");
        assert_eq!(split.target, "service");
        assert_eq!(split.steps.len(), 1);
        assert_eq!(split.path_context(), PathContext::Production);
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
                3_000,
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

    fn vector_text_candidate(id: &str, path: &str, text: &str, score: f64) -> RetrievalCandidate {
        let mut candidate = RetrievalCandidate::new(
            id,
            RetrievalCandidateSource::VectorSemantic,
            "vector text evidence candidate",
        );
        candidate.embedding_source = Some(VectorEmbeddingSource::TextEvidence);
        candidate.file_id = Some(path.to_string());
        candidate.path = Some(path.to_string());
        candidate.span = Some(SourceSpan::new(path, 1, 3));
        candidate.matched_query_text = Some("natural language package configuration".to_string());
        candidate.evidence_role = EvidenceRole::Unknown;
        candidate.proof_status = RetrievalProofStatus::NotGraphProof;
        candidate.graph_proof = false;
        candidate.claimable = true;
        candidate.claimable_for_text = Some(true);
        candidate.claimable_for_graph = Some(false);
        candidate.score = Some(score);
        candidate.embedding_model_id =
            Some("codegraph-deterministic-token-projection-v1".to_string());
        candidate.embedding_dim = Some(64);
        candidate.embedding_profile = Some("deterministic-test-embedding-v1".to_string());
        candidate.chunk_id = Some(format!("{id}#chunk"));
        candidate.chunk_kind = Some("snippet".to_string());
        candidate.requires_graph_verification = false;
        candidate.verification_status = RetrievalVerificationStatus::NotGraphProof;
        candidate.metadata.insert(
            "provider_id".to_string(),
            serde_json::json!("codegraph-local-deterministic"),
        );
        candidate
            .metadata
            .insert("chunk_text".to_string(), serde_json::json!(text));
        candidate.metadata.insert(
            "evidence_role_raw".to_string(),
            serde_json::json!("text_evidence"),
        );
        candidate
    }

    fn vector_graph_candidate(
        id: &str,
        entity_id: &str,
        text: &str,
        score: f64,
    ) -> RetrievalCandidate {
        let mut candidate = RetrievalCandidate::new(
            id,
            RetrievalCandidateSource::VectorSemantic,
            "vector graph entity candidate",
        );
        candidate.embedding_source = Some(VectorEmbeddingSource::GraphEntity);
        candidate.file_id = Some("src/auth.ts".to_string());
        candidate.path = Some("src/auth.ts".to_string());
        candidate.entity_id = Some(entity_id.to_string());
        candidate.span = Some(SourceSpan::new("src/auth.ts", 2, 4));
        candidate.matched_query_text = Some("where is the login function implemented".to_string());
        candidate.evidence_role = EvidenceRole::Production;
        candidate.proof_status = RetrievalProofStatus::CandidateOnly;
        candidate.graph_proof = false;
        candidate.claimable = false;
        candidate.claimable_for_text = Some(false);
        candidate.claimable_for_graph = Some(false);
        candidate.score = Some(score);
        candidate.embedding_model_id =
            Some("codegraph-deterministic-token-projection-v1".to_string());
        candidate.embedding_dim = Some(64);
        candidate.embedding_profile = Some("deterministic-test-embedding-v1".to_string());
        candidate.chunk_id = Some(format!("{id}#chunk"));
        candidate.chunk_kind = Some("function".to_string());
        candidate.requires_graph_verification = true;
        candidate.verification_status = RetrievalVerificationStatus::NeedsGraphVerification;
        candidate.metadata.insert(
            "provider_id".to_string(),
            serde_json::json!("codegraph-local-deterministic"),
        );
        candidate
            .metadata
            .insert("chunk_text".to_string(), serde_json::json!(text));
        candidate
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
                "stage0_vector_semantic_candidates",
                "stage1_binary_sieve",
                "stage1_nuance_rescue",
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
    fn retrieval_funnel_keeps_text_tokens_out_of_graph_exact_seed_lane() {
        let funnel = ok(RetrievalFunnel::new(
            vec![edge(
                "Exact.seed",
                RelationKind::Calls,
                "verified-target",
                1,
            )],
            vec![RetrievalDocument::new(
                "Exact.seed",
                "explicit exact seed document",
            )],
            funnel_config(1, 1),
        ));

        let result = ok(funnel.run(
            RetrievalFunnelRequest::new(
                "semantic package metadata generic package noise",
                "impact",
                1_000,
            )
            .exact_seeds(vec!["Exact.seed".to_string()]),
        ));
        let stage0 = result
            .trace
            .iter()
            .find(|stage| stage.stage == "stage0_exact_seed_extraction")
            .expect("stage0 trace");
        let exact_rerank_ids = result
            .rerank_scores
            .iter()
            .filter(|score| score.exact_seed)
            .map(|score| score.id.as_str())
            .collect::<Vec<_>>();

        assert!(stage0.kept.contains(&"Exact.seed".to_string()));
        assert!(!stage0.kept.contains(&"generic-package".to_string()));
        assert!(exact_rerank_ids.contains(&"Exact.seed"));
        assert!(!exact_rerank_ids.contains(&"generic-package"));
        assert!(result
            .packet
            .verified_paths
            .iter()
            .any(|path| path.source == "Exact.seed"));
    }

    #[test]
    fn vector_branch_text_evidence_feeds_no_proof_fallback_without_graph_proof() {
        let mut config = funnel_config(2, 2);
        config.vector_candidate_top_k = 4;
        let funnel = ok(RetrievalFunnel::new(Vec::new(), Vec::new(), config));
        let candidate = vector_text_candidate(
            "vector://text/package/foo/Config.in",
            "package/foo/Config.in",
            "config BR2_PACKAGE_FOO\n\tbool \"foo\"\n\tdepends on BR2_USE_MMU",
            0.92,
        );

        let result = ok(funnel.run(
            RetrievalFunnelRequest::new(
                "Which Buildroot option enables the foo package?",
                "planning",
                1_000,
            )
            .enable_vector_candidates(true)
            .vector_branch_status(VectorCandidateBranchStatus::Ready)
            .vector_candidates(vec![candidate]),
        ));

        assert_eq!(result.vector_candidates.len(), 1);
        assert_eq!(
            result.vector_candidates[0].candidate_source,
            RetrievalCandidateSource::VectorSemantic
        );
        assert!(!result.vector_candidates[0].graph_proof);
        assert_eq!(
            result.vector_candidates[0].proof_status,
            RetrievalProofStatus::NotGraphProof
        );
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
                .and_then(|value| value.as_str()),
            Some("no_proof_path_found")
        );
        assert_eq!(
            result
                .packet
                .metadata
                .get("graph_proof")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn vector_branch_graph_entity_candidate_feeds_graph_verification() {
        let mut config = funnel_config(4, 4);
        config.vector_candidate_top_k = 4;
        let funnel = ok(RetrievalFunnel::new(
            vec![edge(
                "AuthService.login",
                RelationKind::Calls,
                "TokenStore.create",
                1,
            )],
            Vec::new(),
            config,
        ));
        let candidate = vector_graph_candidate(
            "vector://entity/AuthService.login",
            "AuthService.login",
            "function AuthService.login writes login token",
            0.94,
        );

        let result = ok(funnel.run(
            RetrievalFunnelRequest::new(
                "Where is login token creation implemented?",
                "impact",
                1_000,
            )
            .enable_vector_candidates(true)
            .vector_branch_status(VectorCandidateBranchStatus::Ready)
            .vector_candidates(vec![candidate]),
        ));

        assert_eq!(result.vector_candidates.len(), 1);
        assert!(!result.vector_candidates[0].graph_proof);
        assert_eq!(
            result.vector_candidates[0].verification_status,
            RetrievalVerificationStatus::NeedsGraphVerification
        );
        assert!(result.packet.verified_paths.iter().any(|path| {
            path.source == "AuthService.login" && path.target == "TokenStore.create"
        }));
    }

    #[test]
    fn exact_seed_survives_many_vector_candidates_and_vector_cap() {
        let mut config = funnel_config(1, 1);
        config.vector_candidate_top_k = 1;
        let noisy_vectors = (0..16)
            .map(|index| {
                vector_text_candidate(
                    &format!("vector://text/noisy-{index}"),
                    &format!("docs/noisy-{index}.md"),
                    "semantic noise auth login token package",
                    1.0 - (index as f64 * 0.01),
                )
            })
            .collect::<Vec<_>>();
        let funnel = ok(RetrievalFunnel::new(
            vec![edge(
                "Exact.seed",
                RelationKind::Calls,
                "verified-target",
                1,
            )],
            vec![RetrievalDocument::new("Exact.seed", "unrelated exact seed")],
            config,
        ));

        let result = ok(funnel.run(
            RetrievalFunnelRequest::new("semantic noise auth login token", "impact", 1_000)
                .exact_seeds(vec!["Exact.seed".to_string()])
                .enable_vector_candidates(true)
                .vector_branch_status(VectorCandidateBranchStatus::Ready)
                .vector_candidates(noisy_vectors),
        ));
        let vector_stage = result
            .trace
            .iter()
            .find(|stage| stage.stage == "stage0_vector_semantic_candidates")
            .expect("vector stage");
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

        assert_eq!(result.vector_candidates.len(), 1);
        assert!(!vector_stage.dropped.is_empty());
        assert!(stage1.kept.contains(&"Exact.seed".to_string()));
        assert!(stage2.kept.contains(&"Exact.seed".to_string()));
    }

    #[test]
    fn nuance_rescue_is_opt_in_and_candidate_only() {
        let mut config = funnel_config(0, 4);
        config.nuance_rescue_top_k = 4;
        let funnel = ok(RetrievalFunnel::new(
            vec![edge(
                "RareAuthGate",
                RelationKind::Calls,
                "AdminTokenVault",
                1,
            )],
            vec![RetrievalDocument::new(
                "RareAuthGate",
                "requireFreshAdminToken validates the admin route literal",
            )],
            config,
        ));

        let disabled = ok(funnel.run(RetrievalFunnelRequest::new(
            "Trace requireFreshAdminToken admin route handling",
            "security",
            1_000,
        )));
        assert!(disabled.nuance_rescue_candidates.is_empty());
        assert!(disabled.packet.verified_paths.is_empty());

        let rescued = ok(funnel.run(
            RetrievalFunnelRequest::new(
                "Trace requireFreshAdminToken admin route handling",
                "security",
                1_000,
            )
            .enable_nuance_rescue_candidates(true),
        ));
        let nuance_stage = rescued
            .trace
            .iter()
            .find(|stage| stage.stage == "stage1_nuance_rescue")
            .expect("nuance rescue stage");

        assert!(nuance_stage.kept.contains(&"RareAuthGate".to_string()));
        assert_eq!(rescued.nuance_rescue_candidates.len(), 1);
        assert_eq!(
            rescued.nuance_rescue_candidates[0].candidate_source,
            RetrievalCandidateSource::NuanceRescue
        );
        assert_eq!(
            rescued.nuance_rescue_candidates[0].proof_status,
            RetrievalProofStatus::CandidateOnly
        );
        assert!(!rescued.nuance_rescue_candidates[0].graph_proof);
        assert!(!rescued.nuance_rescue_candidates[0].claimable);
        assert_eq!(
            rescued.nuance_rescue_candidates[0].verification_status,
            RetrievalVerificationStatus::NeedsGraphVerification
        );
        assert!(rescued
            .packet
            .verified_paths
            .iter()
            .any(|path| { path.source == "RareAuthGate" && path.target == "AdminTokenVault" }));
    }

    #[test]
    fn missing_or_stale_vector_index_does_not_break_funnel() {
        let funnel = ok(RetrievalFunnel::new(
            vec![edge("a", RelationKind::Calls, "b", 1)],
            vec![RetrievalDocument::new("a", "call b")],
            funnel_config(2, 2),
        ));

        let missing = ok(funnel.run(
            RetrievalFunnelRequest::new("Change a", "impact", 1_000)
                .exact_seeds(vec!["a".to_string()])
                .enable_vector_candidates(true)
                .vector_branch_status(VectorCandidateBranchStatus::Missing),
        ));
        assert!(missing.vector_candidates.is_empty());
        assert!(missing
            .vector_warnings
            .iter()
            .any(|warning| warning.contains("missing")));
        assert!(missing
            .packet
            .verified_paths
            .iter()
            .any(|path| path.source == "a" && path.target == "b"));

        let stale = ok(funnel.run(
            RetrievalFunnelRequest::new("Change a", "impact", 1_000)
                .exact_seeds(vec!["a".to_string()])
                .enable_vector_candidates(true)
                .vector_branch_status(VectorCandidateBranchStatus::Stale {
                    reason: "provider model changed".to_string(),
                })
                .vector_candidates(vec![vector_graph_candidate(
                    "vector://entity/a",
                    "a",
                    "call b",
                    0.99,
                )]),
        ));
        assert!(stale.vector_candidates.is_empty());
        assert!(stale
            .vector_warnings
            .iter()
            .any(|warning| warning.contains("provider model changed")));
    }

    #[test]
    fn vector_diagnostics_trace_includes_branch_without_default_bloat() {
        let mut config = funnel_config(2, 2);
        config.vector_candidate_top_k = 4;
        let funnel = ok(RetrievalFunnel::new(Vec::new(), Vec::new(), config));
        let candidate = vector_text_candidate(
            "vector://text/package/foo/Config.in",
            "package/foo/Config.in",
            "config BR2_PACKAGE_FOO\n\tbool \"foo\"\n\tdepends on BR2_USE_MMU",
            0.92,
        );

        let compact = ok(funnel.run(
            RetrievalFunnelRequest::new("Which Buildroot option enables foo?", "planning", 1_000)
                .enable_vector_candidates(true)
                .vector_branch_status(VectorCandidateBranchStatus::Ready)
                .vector_candidates(vec![candidate.clone()]),
        ));
        assert!(compact
            .packet
            .metadata
            .get("vector_candidate_trace")
            .is_none());

        let diagnostic = ok(funnel.run(
            RetrievalFunnelRequest::new("Which Buildroot option enables foo?", "planning", 1_000)
                .enable_vector_candidates(true)
                .vector_candidate_diagnostics(true)
                .vector_branch_status(VectorCandidateBranchStatus::Ready)
                .vector_candidates(vec![candidate]),
        ));
        let trace = diagnostic
            .packet
            .metadata
            .get("vector_candidate_trace")
            .expect("vector candidate trace");

        assert_eq!(trace["diagnostic_only"].as_bool(), Some(true));
        assert_eq!(trace["vector_enabled"].as_bool(), Some(true));
        assert_eq!(trace["vector_index_status"].as_str(), Some("ready"));
        assert_eq!(
            trace["provider"]["provider_id"].as_str(),
            Some("codegraph-local-deterministic")
        );
        assert_eq!(
            trace["provider"]["model_id"].as_str(),
            Some("codegraph-deterministic-token-projection-v1")
        );
        assert_eq!(
            trace["provider"]["embedding_kind"].as_str(),
            Some("deterministic_token_projection")
        );
        assert_eq!(
            trace["provider"]["learned_semantic_embeddings"].as_bool(),
            Some(false)
        );
        assert_eq!(
            trace["provider"]["production_semantic_quality"].as_bool(),
            Some(false)
        );
        assert_eq!(trace["provider"]["dimension"].as_u64(), Some(64));
        assert_eq!(trace["chunk_count_searched"].as_u64(), Some(1));
        assert_eq!(trace["vector_candidate_count"].as_u64(), Some(1));
        assert_eq!(trace["candidates_accepted"]["count"].as_u64(), Some(1));
        assert_eq!(trace["candidates_rejected"]["count"].as_u64(), Some(0));
        assert_eq!(
            trace["top_vector_candidate_ids"][0].as_str(),
            Some("vector://text/package/foo/Config.in")
        );
        assert_eq!(trace["score_range"]["min"].as_f64(), Some(0.92));
        assert_eq!(trace["score_range"]["max"].as_f64(), Some(0.92));
        assert_eq!(
            trace["graph_verification_status_for_vector_candidates"]["graph_proof_count"].as_u64(),
            Some(0)
        );
        assert!(trace["no_proof_fallback_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("vector text-evidence candidate")));
        let serialized = serde_json::to_string(trace).expect("serialize vector trace");
        assert!(!serialized.contains("BR2_PACKAGE_FOO"));
        assert!(!serialized.contains("chunk_text"));
    }

    #[test]
    fn vector_diagnostics_trace_records_stale_reason() {
        let funnel = ok(RetrievalFunnel::new(
            vec![edge("a", RelationKind::Calls, "b", 1)],
            vec![RetrievalDocument::new("a", "call b")],
            funnel_config(2, 2),
        ));
        let result = ok(funnel.run(
            RetrievalFunnelRequest::new("Change a", "impact", 1_000)
                .enable_vector_candidates(true)
                .vector_candidate_diagnostics(true)
                .vector_branch_status(VectorCandidateBranchStatus::Stale {
                    reason: "provider model changed".to_string(),
                })
                .vector_candidates(vec![vector_graph_candidate(
                    "vector://entity/a",
                    "a",
                    "call b",
                    0.99,
                )]),
        ));
        let trace = result
            .packet
            .metadata
            .get("vector_candidate_trace")
            .expect("vector candidate trace");

        assert_eq!(trace["vector_enabled"].as_bool(), Some(true));
        assert_eq!(trace["vector_index_status"].as_str(), Some("stale"));
        assert_eq!(trace["vector_candidate_count"].as_u64(), Some(0));
        assert_eq!(trace["candidates_rejected"]["count"].as_u64(), Some(1));
        assert!(trace["stale_missing_vector_index_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("provider model changed")));
    }

    #[test]
    fn vector_diagnostics_trace_redacts_query_secrets() {
        let mut config = funnel_config(2, 2);
        config.vector_candidate_top_k = 4;
        let funnel = ok(RetrievalFunnel::new(Vec::new(), Vec::new(), config));
        let result = ok(funnel.run(
            RetrievalFunnelRequest::new(
                "Find package metadata api_key=sk-vector-secret password=super-secret token=abc123",
                "planning",
                1_000,
            )
            .enable_vector_candidates(true)
            .vector_candidate_diagnostics(true)
            .vector_branch_status(VectorCandidateBranchStatus::Ready)
            .vector_candidates(vec![vector_text_candidate(
                "vector://text/package/foo/foo.mk",
                "package/foo/foo.mk",
                "$(eval $(generic-package))",
                0.91,
            )]),
        ));
        let trace = result
            .packet
            .metadata
            .get("vector_candidate_trace")
            .expect("vector candidate trace");
        let serialized = serde_json::to_string(trace).expect("serialize vector trace");

        assert_eq!(trace["query_text_redacted"].as_bool(), Some(true));
        assert!(trace["query_text_sent_to_vector_branch"]
            .as_str()
            .is_some_and(|query| query.contains("[redacted]")));
        assert!(!serialized.contains("sk-vector-secret"));
        assert!(!serialized.contains("super-secret"));
        assert!(!serialized.contains("abc123"));
        assert!(!serialized.contains("generic-package"));
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

    fn binary_overfetch_rerank_test_config() -> RetrievalFunnelConfig {
        let mut config = funnel_config(1, 1);
        config.binary_overfetch_k = 4;
        config.rerank_config = RerankConfig {
            text_weight: 0.05,
            compressed_vector_weight: 0.05,
            stage1_weight: 0.05,
            metadata_weight: 0.0,
            rare_token_weight: 1.0,
            identifier_signature_weight: 1.0,
            text_evidence_weight: 0.3,
            source_role_weight: 0.3,
            graph_verification_weight: 0.2,
            ..RerankConfig::default()
        };
        config
    }

    fn near_boundary_binary_documents() -> Vec<RetrievalDocument> {
        let mut target =
            RetrievalDocument::new("ZephyrAlphaTokenGuard", "tiny guard body").stage0_score(0.1);
        target
            .metadata
            .insert("matched_tokens".to_string(), "zephyralphatoken".to_string());
        target
            .metadata
            .insert("rare_token_match".to_string(), "true".to_string());
        target
            .metadata
            .insert("identifier_signature_match".to_string(), "true".to_string());
        target.metadata.insert(
            "path".to_string(),
            "src/auth/zephyr_alpha_token_guard.ts".to_string(),
        );
        target
            .metadata
            .insert("source_role_compatible".to_string(), "true".to_string());
        target
            .metadata
            .insert("text_evidence_match".to_string(), "true".to_string());

        vec![
            RetrievalDocument::new("broad-noise-0", "trace route auth test").stage0_score(1.0),
            RetrievalDocument::new("broad-noise-1", "trace route auth test").stage0_score(1.0),
            RetrievalDocument::new("broad-noise-2", "trace route auth test").stage0_score(1.0),
            target,
        ]
    }

    #[test]
    fn binary_overfetch_rerank_recovers_near_boundary_candidate() {
        let prompt = "Trace route auth test";
        let without_overfetch = ok(RetrievalFunnel::new(
            vec![edge(
                "ZephyrAlphaTokenGuard",
                RelationKind::Calls,
                "verified-target",
                1,
            )],
            near_boundary_binary_documents(),
            {
                let mut config = binary_overfetch_rerank_test_config();
                config.binary_overfetch_k = 0;
                config
            },
        ));
        let without_result =
            ok(without_overfetch.run(RetrievalFunnelRequest::new(prompt, "security", 1_000)));
        let without_stage2 = without_result
            .trace
            .iter()
            .find(|stage| stage.stage == "stage2_compressed_rerank")
            .expect("stage2 without overfetch");
        assert!(!without_stage2
            .kept
            .contains(&"ZephyrAlphaTokenGuard".to_string()));

        let with_overfetch = ok(RetrievalFunnel::new(
            vec![edge(
                "ZephyrAlphaTokenGuard",
                RelationKind::Calls,
                "verified-target",
                1,
            )],
            near_boundary_binary_documents(),
            binary_overfetch_rerank_test_config(),
        ));
        let result = ok(with_overfetch.run(RetrievalFunnelRequest::new(prompt, "security", 1_000)));
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

        assert!(stage1.kept.contains(&"ZephyrAlphaTokenGuard".to_string()));
        assert_eq!(stage2.kept, vec!["ZephyrAlphaTokenGuard".to_string()]);
        assert!(!stage2.kept.contains(&"broad-noise-0".to_string()));
        assert!(stage1
            .notes
            .iter()
            .any(|note| note == "binary_overfetch_k=4"));
        assert!(stage2
            .notes
            .iter()
            .any(|note| note == "rerank_input_count=4"));
        assert!(stage2
            .notes
            .iter()
            .any(|note| note == "rerank_output_count=1"));
        assert!(stage2.notes.iter().any(|note| {
            note.contains("rare_token_match")
                && note.contains("identifier_signature_match")
                && note.contains("graph_verification_availability")
        }));
        assert!(result.packet.verified_paths.iter().any(|path| {
            path.source == "ZephyrAlphaTokenGuard" && path.target == "verified-target"
        }));
        let compact_rerank = result
            .packet
            .metadata
            .get("rerank_scores")
            .expect("rerank scores");
        let serialized = serde_json::to_string(compact_rerank).expect("serialize rerank scores");
        assert!(!serialized.contains("components"));
        assert!(serialized.len() < 512);
    }

    #[test]
    fn binary_overfetch_rerank_preserves_exact_seed_and_no_proof_boundary() {
        let prompt = "Trace route auth test";
        let no_proof = ok(RetrievalFunnel::new(
            Vec::new(),
            near_boundary_binary_documents(),
            binary_overfetch_rerank_test_config(),
        ));
        let no_proof_result =
            ok(no_proof.run(RetrievalFunnelRequest::new(prompt, "security", 1_000)));
        let no_proof_stage2 = no_proof_result
            .trace
            .iter()
            .find(|stage| stage.stage == "stage2_compressed_rerank")
            .expect("stage2 no proof");
        assert_eq!(
            no_proof_stage2.kept,
            vec!["ZephyrAlphaTokenGuard".to_string()]
        );
        assert!(no_proof_result.packet.verified_paths.is_empty());
        assert_ne!(
            no_proof_result
                .packet
                .metadata
                .get("graph_proof")
                .and_then(|value| value.as_bool()),
            Some(true)
        );

        let exact_seed = ok(RetrievalFunnel::new(
            vec![edge(
                "Exact.seed",
                RelationKind::Calls,
                "verified-target",
                1,
            )],
            near_boundary_binary_documents(),
            binary_overfetch_rerank_test_config(),
        ));
        let exact_result = ok(exact_seed.run(
            RetrievalFunnelRequest::new(prompt, "security", 1_000)
                .exact_seeds(vec!["Exact.seed".to_string()]),
        ));
        let exact_stage2 = exact_result
            .trace
            .iter()
            .find(|stage| stage.stage == "stage2_compressed_rerank")
            .expect("stage2 exact");

        assert!(exact_stage2.kept.contains(&"Exact.seed".to_string()));
        assert!(exact_result
            .packet
            .verified_paths
            .iter()
            .any(|path| path.source == "Exact.seed" && path.target == "verified-target"));
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
            "stage0_vector_semantic_candidates",
            "stage1_binary_sieve",
            "stage1_nuance_rescue",
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

        assert!(estimated <= 300, "estimated token count was {estimated}");
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
