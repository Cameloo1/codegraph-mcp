use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{
    normalize_repo_relative_path, EdgeClass, EdgeContext, EntityKind, EvidenceRole, Exactness,
    RelationKind,
};

pub type Metadata = BTreeMap<String, serde_json::Value>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub repo_relative_path: String,
    pub start_line: u32,
    pub start_column: Option<u32>,
    pub end_line: u32,
    pub end_column: Option<u32>,
}

impl SourceSpan {
    pub fn new(repo_relative_path: impl AsRef<str>, start_line: u32, end_line: u32) -> Self {
        Self {
            repo_relative_path: normalize_repo_relative_path(repo_relative_path),
            start_line,
            start_column: None,
            end_line,
            end_column: None,
        }
    }

    pub fn with_columns(
        repo_relative_path: impl AsRef<str>,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
    ) -> Self {
        Self {
            repo_relative_path: normalize_repo_relative_path(repo_relative_path),
            start_line,
            start_column: Some(start_column),
            end_line,
            end_column: Some(end_column),
        }
    }
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.start_column, self.end_column) {
            (Some(start_column), Some(end_column)) => write!(
                formatter,
                "{}:{}:{}-{}:{}",
                self.repo_relative_path, self.start_line, start_column, self.end_line, end_column
            ),
            _ if self.start_line == self.end_line => {
                write!(formatter, "{}:{}", self.repo_relative_path, self.start_line)
            }
            _ => write!(
                formatter,
                "{}:{}-{}",
                self.repo_relative_path, self.start_line, self.end_line
            ),
        }
    }
}

impl FromStr for SourceSpan {
    type Err = SourceSpanParseError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let Some((path, range)) = raw.rsplit_once(':') else {
            return Err(SourceSpanParseError::new(raw));
        };
        let Some((start, end)) = range.split_once('-') else {
            let line = range
                .parse::<u32>()
                .map_err(|_| SourceSpanParseError::new(raw))?;
            return Ok(Self::new(path, line, line));
        };
        let start_line = start
            .parse::<u32>()
            .map_err(|_| SourceSpanParseError::new(raw))?;
        let end_line = end
            .parse::<u32>()
            .map_err(|_| SourceSpanParseError::new(raw))?;
        Ok(Self::new(path, start_line, end_line))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpanParseError {
    value: String,
}

impl SourceSpanParseError {
    fn new(value: &str) -> Self {
        Self {
            value: value.to_string(),
        }
    }
}

impl fmt::Display for SourceSpanParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid source span: {}", self.value)
    }
}

impl std::error::Error for SourceSpanParseError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub kind: EntityKind,
    pub name: String,
    pub qualified_name: String,
    pub repo_relative_path: String,
    pub source_span: Option<SourceSpan>,
    pub content_hash: Option<String>,
    pub file_hash: Option<String>,
    pub created_from: String,
    pub confidence: f64,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub head_id: String,
    pub relation: RelationKind,
    pub tail_id: String,
    pub source_span: SourceSpan,
    pub repo_commit: Option<String>,
    pub file_hash: Option<String>,
    pub extractor: String,
    pub confidence: f64,
    pub exactness: Exactness,
    #[serde(default = "default_edge_class")]
    pub edge_class: EdgeClass,
    #[serde(default = "default_edge_context")]
    pub context: EdgeContext,
    pub derived: bool,
    #[serde(default)]
    pub provenance_edges: Vec<String>,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRoleDecision {
    pub role: EvidenceRole,
    pub reason: String,
    pub classification_source: String,
}

impl EvidenceRoleDecision {
    pub fn new(
        role: EvidenceRole,
        reason: impl Into<String>,
        classification_source: impl Into<String>,
    ) -> Self {
        Self {
            role,
            reason: reason.into(),
            classification_source: classification_source.into(),
        }
    }
}

fn default_edge_class() -> EdgeClass {
    EdgeClass::Unknown
}

fn default_edge_context() -> EdgeContext {
    EdgeContext::Unknown
}

pub fn normalize_edge_classification(edge: &mut Edge) {
    edge.context = infer_edge_context(edge);
    edge.edge_class = infer_edge_class(edge);
}

/// Only definition-shaped entities resolve a missing-symbol lookup.
/// Import/export bindings and usage sites (call sites, locals) share the
/// target's name but do not define it — counting them would silently clear
/// real blockers (MVP3.9.5.2) or suppress real escalations (MVP3.9.5.4).
pub fn entity_kind_defines_symbol(kind: EntityKind) -> bool {
    !matches!(
        kind,
        EntityKind::Import
            | EntityKind::Export
            | EntityKind::CallSite
            | EntityKind::ReturnSite
            | EntityKind::Expression
            | EntityKind::Assignment
            | EntityKind::Parameter
            | EntityKind::LocalVariable
    )
}

pub fn classify_entity_source_role(entity: &Entity) -> EvidenceRoleDecision {
    if let Some(role) = metadata_evidence_role(&entity.metadata) {
        return EvidenceRoleDecision::new(
            role,
            metadata_reason(&entity.metadata)
                .unwrap_or_else(|| "entity metadata source_role".into()),
            "metadata",
        );
    }
    if let Some(role) = entity_kind_source_role(entity.kind) {
        return EvidenceRoleDecision::new(
            role,
            format!("entity kind {}", entity.kind),
            "entity_kind",
        );
    }
    if qualified_name_contains_test_module(&entity.qualified_name) {
        return EvidenceRoleDecision::new(
            EvidenceRole::Test,
            "qualified name contains tests module",
            "qualified_name",
        );
    }
    if looks_mock_like(&entity.name) || looks_mock_like(&entity.qualified_name) {
        return EvidenceRoleDecision::new(
            EvidenceRole::Mock,
            "entity name looks like mock/stub",
            "qualified_name",
        );
    }
    if is_test_path(&entity.repo_relative_path) {
        return EvidenceRoleDecision::new(
            EvidenceRole::Test,
            "entity is in a test/spec path",
            "file_path",
        );
    }
    EvidenceRoleDecision::new(
        EvidenceRole::Unknown,
        "missing source-role metadata",
        "fallback",
    )
}

pub fn classify_edge_evidence_role(edge: &Edge) -> EvidenceRoleDecision {
    if let Some(role) = metadata_evidence_role(&edge.metadata) {
        return EvidenceRoleDecision::new(
            role,
            metadata_reason(&edge.metadata).unwrap_or_else(|| "edge metadata source_role".into()),
            "metadata",
        );
    }
    if let Some(role) = relation_source_role(edge.relation) {
        return EvidenceRoleDecision::new(
            role,
            format!("relation kind {}", edge.relation),
            "relation_kind",
        );
    }
    if is_test_path(&edge.source_span.repo_relative_path) {
        return EvidenceRoleDecision::new(
            EvidenceRole::Test,
            "edge source span is in a test/spec path",
            "file_path",
        );
    }
    if endpoint_looks_mock(&edge.head_id) || endpoint_looks_mock(&edge.tail_id) {
        return EvidenceRoleDecision::new(
            EvidenceRole::Mock,
            "edge endpoint looks like mock/stub",
            "endpoint_id",
        );
    }
    if endpoint_looks_test(&edge.head_id) || endpoint_looks_test(&edge.tail_id) {
        return EvidenceRoleDecision::new(
            EvidenceRole::Test,
            "edge endpoint looks like test/spec",
            "endpoint_id",
        );
    }
    if edge.context != EdgeContext::Unknown {
        return EvidenceRoleDecision::new(
            evidence_role_from_edge_context(edge.context),
            "stored edge context",
            "edge_context",
        );
    }
    EvidenceRoleDecision::new(
        EvidenceRole::Unknown,
        "missing edge source-role metadata",
        "fallback",
    )
}

pub fn combine_evidence_roles<I>(roles: I) -> EvidenceRole
where
    I: IntoIterator<Item = EvidenceRole>,
{
    let mut saw_production = false;
    let mut saw_test = false;
    let mut saw_mock = false;
    let mut saw_unknown = false;

    for role in roles {
        match role {
            EvidenceRole::Production => saw_production = true,
            EvidenceRole::Test => saw_test = true,
            EvidenceRole::Mock => saw_mock = true,
            EvidenceRole::Mixed => {
                saw_production = true;
                saw_test = true;
            }
            EvidenceRole::Unknown => saw_unknown = true,
        }
    }

    match (saw_production, saw_test, saw_mock, saw_unknown) {
        (true, true, _, _) | (true, _, true, _) | (_, true, true, _) => EvidenceRole::Mixed,
        (true, false, false, true) => EvidenceRole::Mixed,
        (false, true, false, true) => EvidenceRole::Mixed,
        (false, false, true, true) => EvidenceRole::Mixed,
        (_, _, true, false) => EvidenceRole::Mock,
        (_, true, _, false) => EvidenceRole::Test,
        (true, false, false, false) => EvidenceRole::Production,
        _ => EvidenceRole::Unknown,
    }
}

pub fn infer_edge_class(edge: &Edge) -> EdgeClass {
    let context = infer_edge_context(edge);
    match context {
        EdgeContext::Mixed => return EdgeClass::Mixed,
        EdgeContext::Mock => return EdgeClass::Mock,
        EdgeContext::Test => return EdgeClass::Test,
        EdgeContext::Production | EdgeContext::Unknown => {}
    }

    if edge.derived
        || is_derived_cache_relation(edge.relation)
        || edge.exactness == Exactness::DerivedFromVerifiedEdges
    {
        return EdgeClass::Derived;
    }
    if is_reified_callsite_relation(edge.relation) || endpoint_looks_callsite(&edge.head_id) {
        return EdgeClass::ReifiedCallsite;
    }
    if edge_is_heuristic(edge) || edge_has_unresolved_resolution(edge) {
        return EdgeClass::BaseHeuristic;
    }
    if is_inverse_relation(edge.relation) {
        return EdgeClass::Unknown;
    }
    if edge_exactness_is_proof_grade(edge.exactness) {
        return EdgeClass::BaseExact;
    }
    EdgeClass::Unknown
}

pub fn infer_edge_context(edge: &Edge) -> EdgeContext {
    if let Some(context) = edge_metadata_context(edge) {
        return context;
    }
    if is_mock_relation(edge.relation)
        || endpoint_looks_mock(&edge.head_id)
        || endpoint_looks_mock(&edge.tail_id)
    {
        return EdgeContext::Mock;
    }
    if is_test_relation(edge.relation)
        || is_test_path(&edge.source_span.repo_relative_path)
        || endpoint_looks_test(&edge.head_id)
        || endpoint_looks_test(&edge.tail_id)
    {
        return EdgeContext::Test;
    }
    if edge.context != EdgeContext::Unknown {
        return edge.context;
    }
    EdgeContext::Production
}

fn edge_metadata_context(edge: &Edge) -> Option<EdgeContext> {
    for key in [
        "evidence_role",
        "source_role",
        "path_context",
        "context",
        "execution_context",
        "scope",
    ] {
        if let Some(value) = edge.metadata.get(key).and_then(serde_json::Value::as_str) {
            return Some(normalize_context_label(value));
        }
    }
    None
}

fn metadata_evidence_role(metadata: &Metadata) -> Option<EvidenceRole> {
    for key in [
        "evidence_role",
        "source_role",
        "path_context",
        "context",
        "execution_context",
        "scope",
    ] {
        if let Some(value) = metadata.get(key).and_then(serde_json::Value::as_str) {
            return Some(evidence_role_from_label(value));
        }
    }
    None
}

fn metadata_reason(metadata: &Metadata) -> Option<String> {
    for key in [
        "classification_reason",
        "source_role_reason",
        "evidence_role_reason",
        "context_reason",
    ] {
        if let Some(value) = metadata.get(key).and_then(serde_json::Value::as_str) {
            return Some(value.to_string());
        }
    }
    None
}

fn evidence_role_from_label(value: &str) -> EvidenceRole {
    match normalize_context_label(value) {
        EdgeContext::Production => EvidenceRole::Production,
        EdgeContext::Test => EvidenceRole::Test,
        EdgeContext::Mock => EvidenceRole::Mock,
        EdgeContext::Mixed => EvidenceRole::Mixed,
        EdgeContext::Unknown => EvidenceRole::Unknown,
    }
}

fn evidence_role_from_edge_context(context: EdgeContext) -> EvidenceRole {
    match context {
        EdgeContext::Production => EvidenceRole::Production,
        EdgeContext::Test => EvidenceRole::Test,
        EdgeContext::Mock => EvidenceRole::Mock,
        EdgeContext::Mixed => EvidenceRole::Mixed,
        EdgeContext::Unknown => EvidenceRole::Unknown,
    }
}

fn entity_kind_source_role(kind: EntityKind) -> Option<EvidenceRole> {
    match kind {
        EntityKind::TestFile
        | EntityKind::TestSuite
        | EntityKind::TestCase
        | EntityKind::Fixture
        | EntityKind::Assertion => Some(EvidenceRole::Test),
        EntityKind::Mock | EntityKind::Stub => Some(EvidenceRole::Mock),
        _ => None,
    }
}

fn relation_source_role(relation: RelationKind) -> Option<EvidenceRole> {
    if is_mock_relation(relation) {
        Some(EvidenceRole::Mock)
    } else if is_test_relation(relation) {
        Some(EvidenceRole::Test)
    } else {
        None
    }
}

fn normalize_context_label(value: &str) -> EdgeContext {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.contains("mixed") {
        EdgeContext::Mixed
    } else if normalized.contains("mock") || normalized.contains("stub") {
        EdgeContext::Mock
    } else if normalized.contains("test") || normalized.contains("spec") {
        EdgeContext::Test
    } else if normalized.contains("unknown") || normalized.contains("unresolved") {
        EdgeContext::Unknown
    } else {
        EdgeContext::Production
    }
}

fn edge_exactness_is_proof_grade(exactness: Exactness) -> bool {
    matches!(
        exactness,
        Exactness::Exact
            | Exactness::CompilerVerified
            | Exactness::LspVerified
            | Exactness::ParserVerified
    )
}

fn edge_is_heuristic(edge: &Edge) -> bool {
    matches!(
        edge.exactness,
        Exactness::StaticHeuristic | Exactness::Inferred
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

fn is_reified_callsite_relation(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Callee
            | RelationKind::Argument0
            | RelationKind::Argument1
            | RelationKind::ArgumentN
            | RelationKind::ReturnsTo
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

fn endpoint_looks_callsite(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("callsite") || normalized.contains("#call:")
}

fn is_test_relation(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Tests
            | RelationKind::Asserts
            | RelationKind::Covers
            | RelationKind::FixturesFor
    )
}

fn is_mock_relation(relation: RelationKind) -> bool {
    matches!(relation, RelationKind::Mocks | RelationKind::Stubs)
}

fn endpoint_looks_test(value: &str) -> bool {
    let normalized = value.replace('\\', "/").to_ascii_lowercase();
    normalized.contains("/tests/")
        || normalized.contains("/test/")
        || normalized.contains(".test.")
        || normalized.contains(".spec.")
        || normalized.contains("#test")
        || normalized.contains("testcase")
        || normalized.contains("testfile")
}

fn endpoint_looks_mock(value: &str) -> bool {
    looks_mock_like(value)
}

fn looks_mock_like(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("mock") || normalized.contains("stub")
}

fn qualified_name_contains_test_module(value: &str) -> bool {
    let normalized = value.replace('\\', "/").to_ascii_lowercase();
    normalized == "tests"
        || normalized == "test"
        || normalized.starts_with("tests.")
        || normalized.starts_with("test.")
        || normalized.contains(".tests.")
        || normalized.contains(".test.")
        || normalized.contains("::tests::")
        || normalized.ends_with(".tests")
        || normalized.ends_with("::tests")
}

fn is_test_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    normalized.contains("/tests/")
        || normalized.contains("/test/")
        || normalized.ends_with(".test.ts")
        || normalized.ends_with(".test.tsx")
        || normalized.ends_with(".test.js")
        || normalized.ends_with(".test.jsx")
        || normalized.ends_with(".spec.ts")
        || normalized.ends_with(".spec.tsx")
        || normalized.ends_with(".spec.js")
        || normalized.ends_with(".spec.jsx")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileRecord {
    pub repo_relative_path: String,
    pub file_hash: String,
    pub language: Option<String>,
    pub size_bytes: u64,
    pub indexed_at_unix_ms: Option<u64>,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoIndexState {
    pub repo_id: String,
    pub repo_root: String,
    pub repo_commit: Option<String>,
    pub schema_version: u32,
    pub indexed_at_unix_ms: Option<u64>,
    pub files_indexed: u64,
    pub entity_count: u64,
    pub edge_count: u64,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathEvidence {
    pub id: String,
    pub summary: Option<String>,
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub metapath: Vec<RelationKind>,
    #[serde(default)]
    pub edges: Vec<(String, RelationKind, String)>,
    #[serde(default)]
    pub source_spans: Vec<SourceSpan>,
    pub exactness: Exactness,
    pub length: usize,
    pub confidence: f64,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalCandidateSource {
    ExactSeed,
    FilePathSeed,
    TextEvidence,
    LexicalFts,
    SymbolLookup,
    #[serde(rename = "binary_vector", alias = "vector_binary")]
    VectorBinary,
    VectorRerank,
    VectorSemantic,
    NuanceRescue,
    GraphNeighbor,
    PathEvidence,
    NoProofFallback,
    Diagnostic,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorEmbeddingSource {
    GraphEntity,
    TextEvidence,
    FilePathTitle,
    Snippet,
    DocComment,
    Signature,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalCandidateLifecycleStatus {
    Fresh,
    Stale,
    Invalid,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalCandidateLifecycleBinding {
    pub status: RetrievalCandidateLifecycleStatus,
    pub db_passport_fingerprint: Option<String>,
    pub repo_head: Option<String>,
    pub scope_policy_hash: Option<String>,
    pub embedding_model_id: Option<String>,
    pub embedding_profile: Option<String>,
    pub stale_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalProofStatus {
    ProofPathFound,
    NoProofPathFound,
    NotGraphProof,
    CandidateOnly,
    DiagnosticOnly,
    StaleOrForeignDb,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalVerificationStatus {
    Unverified,
    NeedsGraphVerification,
    GraphVerified,
    NoProofPathFound,
    NotGraphProof,
    CandidateOnly,
    DiagnosticOnly,
    StaleOrForeignDb,
    Omitted,
    Truncated,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalCandidate {
    pub candidate_id: String,
    pub candidate_source: RetrievalCandidateSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_source: Option<VectorEmbeddingSource>,
    pub file_id: Option<String>,
    pub path: Option<String>,
    pub entity_id: Option<String>,
    pub span: Option<SourceSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_span_missing_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_query_text: Option<String>,
    pub evidence_role: EvidenceRole,
    pub proof_status: RetrievalProofStatus,
    pub graph_proof: bool,
    pub claimable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimable_for_text: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimable_for_graph: Option<bool>,
    pub diagnostic_only: bool,
    pub score: Option<f64>,
    pub rank: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_dim: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_kind: Option<String>,
    #[serde(default)]
    pub matched_seeds: Vec<String>,
    pub requires_graph_verification: bool,
    pub verification_status: RetrievalVerificationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_binding: Option<RetrievalCandidateLifecycleBinding>,
    pub reason: String,
    #[serde(default)]
    pub omitted: bool,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub metadata: Metadata,
}

impl RetrievalCandidate {
    pub fn new(
        candidate_id: impl Into<String>,
        candidate_source: RetrievalCandidateSource,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            candidate_id: candidate_id.into(),
            candidate_source,
            embedding_source: None,
            file_id: None,
            path: None,
            entity_id: None,
            span: None,
            source_span_missing_reason: None,
            matched_query_text: None,
            evidence_role: EvidenceRole::Unknown,
            proof_status: RetrievalProofStatus::Unknown,
            graph_proof: false,
            claimable: false,
            claimable_for_text: None,
            claimable_for_graph: None,
            diagnostic_only: false,
            score: None,
            rank: None,
            embedding_model_id: None,
            embedding_dim: None,
            embedding_profile: None,
            chunk_id: None,
            chunk_kind: None,
            matched_seeds: Vec::new(),
            requires_graph_verification: true,
            verification_status: RetrievalVerificationStatus::Unknown,
            lifecycle_binding: None,
            reason: reason.into(),
            omitted: false,
            truncated: false,
            metadata: Metadata::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedClosureEdge {
    pub id: String,
    pub head_id: String,
    pub relation: RelationKind,
    pub tail_id: String,
    #[serde(default)]
    pub provenance_edges: Vec<String>,
    pub exactness: Exactness,
    pub confidence: f64,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextPacket {
    pub task: String,
    pub mode: String,
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default)]
    pub verified_paths: Vec<PathEvidence>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub recommended_tests: Vec<String>,
    #[serde(default)]
    pub snippets: Vec<ContextSnippet>,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextSnippet {
    pub file: String,
    pub lines: String,
    #[serde(default)]
    pub text: String,
    pub reason: String,
}
