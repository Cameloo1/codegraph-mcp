use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{
    normalize_repo_relative_path, EdgeClass, EdgeContext, EntityKind, Exactness, RelationKind,
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
    EdgeContext::Production
}

fn edge_metadata_context(edge: &Edge) -> Option<EdgeContext> {
    for key in ["path_context", "context", "execution_context", "scope"] {
        if let Some(value) = edge.metadata.get(key).and_then(serde_json::Value::as_str) {
            return Some(normalize_context_label(value));
        }
    }
    None
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
    let normalized = value.to_ascii_lowercase();
    normalized.contains("mock") || normalized.contains("stub")
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
