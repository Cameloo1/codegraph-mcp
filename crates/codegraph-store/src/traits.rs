use codegraph_core::{
    DerivedClosureEdge, Edge, Entity, FileRecord, PathEvidence, RelationKind, RepoIndexState,
    SourceSpan,
};
use serde_json::Value;

use crate::StoreResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSearchKind {
    File,
    Entity,
    Snippet,
}

impl TextSearchKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Entity => "entity",
            Self::Snippet => "snippet",
        }
    }
}

impl TryFrom<&str> for TextSearchKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "file" => Ok(Self::File),
            "entity" => Ok(Self::Entity),
            "snippet" => Ok(Self::Snippet),
            other => Err(format!("unknown text search kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextSearchHit {
    pub kind: TextSearchKind,
    pub id: String,
    pub repo_relative_path: String,
    pub line: Option<u32>,
    pub title: String,
    pub text: String,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalTraceRecord {
    pub id: String,
    pub task: Option<String>,
    pub trace_json: Value,
    pub created_at_unix_ms: Option<i64>,
}

pub trait GraphStore {
    fn migrate(&self) -> StoreResult<()>;
    fn schema_version(&self) -> StoreResult<u32>;

    fn upsert_entity(&self, entity: &Entity) -> StoreResult<()>;
    fn get_entity(&self, id: &str) -> StoreResult<Option<Entity>>;
    fn delete_entity(&self, id: &str) -> StoreResult<bool>;
    fn list_entities_by_file(&self, repo_relative_path: &str) -> StoreResult<Vec<Entity>>;
    fn list_entities(&self, limit: usize) -> StoreResult<Vec<Entity>>;
    fn count_entities(&self) -> StoreResult<u64>;

    fn upsert_edge(&self, edge: &Edge) -> StoreResult<()>;
    fn get_edge(&self, id: &str) -> StoreResult<Option<Edge>>;
    fn delete_edge(&self, id: &str) -> StoreResult<bool>;
    fn list_edges(&self, limit: usize) -> StoreResult<Vec<Edge>>;
    fn count_edges(&self) -> StoreResult<u64>;
    fn find_edges_by_head_relation(
        &self,
        head_id: &str,
        relation: RelationKind,
    ) -> StoreResult<Vec<Edge>>;
    fn find_edges_by_tail_relation(
        &self,
        tail_id: &str,
        relation: RelationKind,
    ) -> StoreResult<Vec<Edge>>;

    fn upsert_file(&self, file: &FileRecord) -> StoreResult<()>;
    fn get_file(&self, repo_relative_path: &str) -> StoreResult<Option<FileRecord>>;
    fn delete_file(&self, repo_relative_path: &str) -> StoreResult<bool>;
    fn delete_facts_for_file(&self, repo_relative_path: &str) -> StoreResult<()>;
    fn list_files(&self, limit: usize) -> StoreResult<Vec<FileRecord>>;
    fn count_files(&self) -> StoreResult<u64>;
    fn upsert_file_text(&self, repo_relative_path: &str, text: &str) -> StoreResult<()>;

    fn upsert_source_span(&self, id: &str, span: &SourceSpan) -> StoreResult<()>;
    fn get_source_span(&self, id: &str) -> StoreResult<Option<SourceSpan>>;
    fn delete_source_span(&self, id: &str) -> StoreResult<bool>;
    fn upsert_snippet_text(&self, id: &str, span: &SourceSpan, text: &str) -> StoreResult<()>;

    fn upsert_repo_index_state(&self, state: &RepoIndexState) -> StoreResult<()>;
    fn get_repo_index_state(&self, repo_id: &str) -> StoreResult<Option<RepoIndexState>>;
    fn delete_repo_index_state(&self, repo_id: &str) -> StoreResult<bool>;

    fn upsert_path_evidence(&self, path: &PathEvidence) -> StoreResult<()>;
    fn get_path_evidence(&self, id: &str) -> StoreResult<Option<PathEvidence>>;
    fn delete_path_evidence(&self, id: &str) -> StoreResult<bool>;
    fn count_path_evidence(&self) -> StoreResult<u64>;

    fn upsert_derived_edge(&self, edge: &DerivedClosureEdge) -> StoreResult<()>;
    fn get_derived_edge(&self, id: &str) -> StoreResult<Option<DerivedClosureEdge>>;
    fn delete_derived_edge(&self, id: &str) -> StoreResult<bool>;

    fn find_entities_by_exact_symbol(&self, symbol: &str) -> StoreResult<Vec<Entity>>;
    fn search_text(&self, query: &str, limit: usize) -> StoreResult<Vec<TextSearchHit>>;

    fn upsert_retrieval_trace(&self, trace: &RetrievalTraceRecord) -> StoreResult<()>;
    fn get_retrieval_trace(&self, id: &str) -> StoreResult<Option<RetrievalTraceRecord>>;
    fn delete_retrieval_trace(&self, id: &str) -> StoreResult<bool>;
}
