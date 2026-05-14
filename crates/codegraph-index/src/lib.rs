//! Shared compact repository indexer for CodeGraph.
//!
//! This crate is the single indexing implementation used by both the CLI and
//! MCP server. It preserves the compact path: streaming batches, duplicate
//! source-content dedupe, source-only ignores, file-only duplicate records, and
//! no default full source/snippet/source-span FTS writes.

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use codegraph_core::{
    normalize_repo_relative_path as normalize_graph_path, stable_edge_id,
    stable_entity_id_for_kind, Edge, EdgeClass, EdgeContext, Entity, EntityKind, Exactness,
    FileRecord, Metadata, PathEvidence, RelationKind, RepoIndexState, SourceSpan,
};
use codegraph_parser::{
    content_hash, detect_language, extract_entities_and_relations, BasicExtraction, LanguageParser,
    TreeSitterParser,
};
use codegraph_query::{
    is_proof_path_relation, ExactGraphQueryEngine, GraphPath, TraversalDirection, TraversalStep,
};
use codegraph_store::{reset_sqlite_profile, take_sqlite_profile};
use codegraph_store::{
    inspect_db_preflight, DbPassport, DbPreflightReport, ExpectedDbPassport, GraphStore,
    SqliteGraphStore, StoreError, DB_PASSPORT_VERSION, SCHEMA_VERSION,
};
use codegraph_vector::{BinarySignature, BinaryVectorIndex, InMemoryBinaryVectorIndex};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub mod scope;
pub use scope::IndexScopeOptions;
use scope::{IndexScope, IndexScopeRuntimeReport, ScopeAction, ScopePathKind};

pub const UNBOUNDED_STORE_READ_LIMIT: usize = 1_000_000;
pub const DEFAULT_INDEX_BATCH_MAX_FILES: usize = 128;
pub const DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
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
    pub scope: Option<IndexScopeRuntimeReport>,
    pub profile: Option<IndexProfile>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexOptions {
    pub profile: bool,
    pub json: bool,
    pub worker_count: Option<usize>,
    pub storage_mode: StorageMode,
    pub build_mode: IndexBuildMode,
    pub scope: IndexScopeOptions,
    pub db_lifecycle: DbLifecycleOptions,
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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IncrementalIndexSummary {
    pub status: String,
    pub repo_root: String,
    pub db_path: String,
    pub changed_files: Vec<String>,
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
    pub global_hash_check_ran: bool,
    pub storage_audit_ran: bool,
    pub integrity_check_ran: bool,
    pub profile: Option<IndexProfile>,
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
            let mut summary =
                index_repo_to_atomic_cold_db(&repo_root, &db_path, options, Some(temp_db_path.clone()))?;
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
        scope: None,
        profile: None,
    };

    let discovery_start = Instant::now();
    let scoped_files = collect_repo_files_with_scope(&repo_root, &options.scope)?;
    let files = scoped_files.files;
    summary.scope = Some(scoped_files.scope_report);
    let file_discovery_ms = discovery_start.elapsed().as_millis();
    phase_profile.add_ms("file_walk", file_discovery_ms as f64, 1, files.len() as u64);
    let indexed_at = unix_time_ms();
    let mut source_candidates = Vec::new();
    let mut skipped_unchanged_files = 0usize;
    for file_path in files {
        summary.files_seen += 1;
        summary.files_walked += 1;
        if detect_language(&file_path).is_none() {
            summary.files_skipped += 1;
            continue;
        }
        let repo_relative_path = repo_relative_path(&repo_root, &file_path)?;
        source_candidates.push((file_path, repo_relative_path));
    }

    let current_repo_paths = source_candidates
        .iter()
        .map(|(_, repo_relative_path)| repo_relative_path.clone())
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
                let commit_start = Instant::now();
                if let Err(error) = store.commit_bulk_index_transaction() {
                    let _ = store.rollback_bulk_index_transaction();
                    return Err(error.into());
                }
                phase_profile.add_duration("transaction_commit", commit_start.elapsed(), 1, 0);
                db_write_ms += commit_start.elapsed().as_millis();
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
        let mut security_plan = reduce_security_edges_from_store(repo_root, &store)?;
        security_plan.sort();
        let mut test_plan = reduce_test_edges_from_store(repo_root, &store)?;
        test_plan.sort();
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
        let total_wall_ms = total_start.elapsed().as_millis();
        summary.profile = Some(IndexProfile {
            file_discovery_ms,
            parse_ms,
            extraction_ms,
            semantic_resolver_ms: 0,
            db_write_ms,
            fts_search_index_ms: db_write_ms,
            vector_signature_ms: 0,
            total_wall_ms,
            files_per_sec: rate_per_second(summary.files_indexed, total_wall_ms),
            entities_per_sec: rate_per_second(summary.entities, total_wall_ms),
            edges_per_sec: rate_per_second(summary.edges, total_wall_ms),
            memory_bytes: current_process_memory_bytes(),
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
    let build_mode = options.build_mode;
    let publish_check = options.build_mode.post_index_check();
    let result = index_repo_to_existing_db_with_options(
        repo_root,
        &temp_db_path,
        options,
        PostIndexCheck::None,
        BulkIndexLoadDurability::HiddenAtomicColdTemp,
    );
    match result {
        Ok(mut summary) => {
            let temp_finalize_start = Instant::now();
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
            let replace_start = Instant::now();
            publish_atomic_sqlite_db(&temp_db_path, final_db_path)?;
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
            }
            add_profile_span_to_summary(
                &mut summary,
                "atomic_final_db_validate",
                final_finalize_start.elapsed(),
                1,
                0,
                "open, configured publish gate, and checkpoint after visible replacement",
            );
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
    if had_old_db {
        if let Err(error) = rename_sqlite_file_family(final_db_path, &backup_db_path) {
            let _ = rename_sqlite_file_family(&backup_db_path, final_db_path);
            return Err(error);
        }
    } else {
        remove_sqlite_sidecars(final_db_path)?;
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
    rename_file_if_exists(&sqlite_sidecar_path(from, "wal"), &sqlite_sidecar_path(to, "wal"))?;
    rename_file_if_exists(&sqlite_sidecar_path(from, "shm"), &sqlite_sidecar_path(to, "shm"))?;
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
    let (local_bundles, stats) = parse_extract_pending_files(batch.files, worker_count)?;
    profile.add_duration(
        "parse_extract_workers_wall",
        parse_extract_start.elapsed(),
        1,
        stats.len() as u64,
    );
    let reduce_start = Instant::now();
    let reduced_plan = reduce_local_fact_bundles(local_bundles);
    profile.add_duration(
        "reducer",
        reduce_start.elapsed(),
        1,
        reduced_plan.bundles.len() as u64,
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
        json!({
            "event": "index_batch_completed",
            "batch_index": batch_index,
            "files_indexed": persisted.files,
            "entities": persisted.entities,
            "edges": persisted.edges,
            "duplicate_edges_upserted": persisted.duplicate_edges_upserted,
            "parse_errors": stats.iter().filter(|stat| stat.parse_error).count(),
            "syntax_errors": stats.iter().filter(|stat| stat.syntax_error).count(),
            "db_write_ms": db_write_ms,
            "parse_ms": parse_ms,
            "extraction_ms": extraction_ms,
            "bundle_ms": bundle_ms,
            "worker_count": worker_count,
        }),
    );

    Ok(IndexBatchProfile {
        parse_ms,
        extraction_ms,
        bundle_ms,
        db_write_ms,
        worker_count,
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

        let persisted_entity_ids = persisted_entity_ids(&extraction.entities);
        if indexed.template_required {
            let preparation_start = Instant::now();
            let template_entities = extraction
                .entities
                .iter()
                .filter(|entity| should_persist_entity(entity))
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
            .filter(|entity| should_persist_entity(entity))
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
    mut pending: Vec<PendingIndexFile>,
    worker_count: usize,
) -> Result<(Vec<IndexedFileOutput>, Vec<ParseExtractStat>), IndexError> {
    pending.sort_by(|left, right| left.repo_relative_path.cmp(&right.repo_relative_path));
    if pending.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let worker_count = worker_count.max(1).min(pending.len());
    let chunk_size = pending.len().div_ceil(worker_count);
    let mut handles = Vec::new();
    for chunk in pending.chunks(chunk_size) {
        let work = chunk.to_vec();
        handles.push(thread::spawn(move || {
            let parser = TreeSitterParser;
            let mut outputs = Vec::new();
            let mut stats = Vec::new();
            for file in work {
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
                    stats.push(ParseExtractStat {
                        repo_relative_path: file.repo_relative_path,
                        parse_ms: 0,
                        extraction_ms: 0,
                        bundle_ms,
                        parse_error: false,
                        syntax_error: false,
                        skipped: false,
                        message: None,
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
                        extraction.file.metadata =
                            file_manifest_metadata(file.modified_unix_nanos.clone());
                        let extraction_ms = extraction_start.elapsed().as_millis();
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
                        stats.push(ParseExtractStat {
                            repo_relative_path: parsed.repo_relative_path.clone(),
                            parse_ms,
                            extraction_ms,
                            bundle_ms,
                            parse_error: false,
                            syntax_error,
                            skipped: false,
                            message: None,
                        });
                    }
                    Ok(None) => {
                        stats.push(ParseExtractStat {
                            repo_relative_path: file.repo_relative_path,
                            parse_ms,
                            extraction_ms: 0,
                            bundle_ms: 0,
                            parse_error: false,
                            syntax_error: false,
                            skipped: true,
                            message: Some("unsupported language after detection".to_string()),
                        });
                    }
                    Err(error) => {
                        stats.push(ParseExtractStat {
                            repo_relative_path: file.repo_relative_path,
                            parse_ms,
                            extraction_ms: 0,
                            bundle_ms: 0,
                            parse_error: true,
                            syntax_error: false,
                            skipped: false,
                            message: Some(error.to_string()),
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
    require_reusable_db_passport(&repo_root, &db_path, &IndexOptions::default())?;
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
        global_hash_check_ran: false,
        storage_audit_ran: false,
        integrity_check_ran: false,
        profile: None,
    };

    let metadata_start = Instant::now();
    let mut normalized = changed_paths
        .iter()
        .map(|path| normalize_changed_path(&repo_root, path))
        .collect::<Result<Vec<_>, _>>()?;
    normalized.sort_by(|left, right| left.1.cmp(&right.1));
    normalized.dedup_by(|left, right| left.1 == right.1);
    summary.changed_files = normalized
        .iter()
        .map(|(_, repo_relative_path)| repo_relative_path.clone())
        .collect();
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
        for (file_path, repo_relative_path) in &normalized {
            summary.files_seen += 1;
            summary.files_walked += 1;
            if should_ignore_path(&repo_root, file_path) {
                summary.files_ignored += 1;
                continue;
            }

            if !file_path.exists() || !file_path.is_file() {
                let existed = tx.get_file(repo_relative_path)?.is_some();
                if path_has_static_resolver_language(repo_relative_path) {
                    changed_static_resolver_inputs = true;
                }
                if cache.has_cached_facts() {
                    let cache_lookup_start = Instant::now();
                    removed_cache_entity_ids.extend(
                        tx.list_entities_by_file(repo_relative_path)?
                            .into_iter()
                            .map(|entity| entity.id),
                    );
                    phase_profile.add_duration(
                        "cache_old_fact_lookup",
                        cache_lookup_start.elapsed(),
                        1,
                        removed_cache_entity_ids.len() as u64,
                    );
                }
                let delete_start = Instant::now();
                tx.delete_facts_for_file(repo_relative_path)?;
                phase_profile.add_duration("stale_fact_delete", delete_start.elapsed(), 1, 1);
                if existed {
                    changed_fact_paths.insert(normalize_graph_path(repo_relative_path));
                    summary.files_deleted += 1;
                    summary.deleted_fact_files += 1;
                }
                continue;
            }

            let Some(language) = detect_language(file_path) else {
                summary.files_skipped += 1;
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
            if cache.has_cached_facts() {
                let cache_lookup_start = Instant::now();
                removed_cache_entity_ids.extend(
                    tx.list_entities_by_file(repo_relative_path)?
                        .into_iter()
                        .map(|entity| entity.id),
                );
                phase_profile.add_duration(
                    "cache_old_fact_lookup",
                    cache_lookup_start.elapsed(),
                    1,
                    removed_cache_entity_ids.len() as u64,
                );
            }
            let delete_start = Instant::now();
            tx.delete_facts_for_file(repo_relative_path)?;
            phase_profile.add_duration("stale_fact_delete", delete_start.elapsed(), 1, 1);
            summary.deleted_fact_files += 1;
            summary.files_parsed += 1;
            let parse_start = Instant::now();
            let parsed = match parser.parse(repo_relative_path, &source) {
                Ok(Some(parsed)) => parsed,
                Ok(None) => {
                    summary.files_skipped += 1;
                    continue;
                }
                Err(_) => {
                    summary.parse_errors += 1;
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
            extraction.file.metadata = file_manifest_metadata(modified_unix_nanos(&file_metadata));
            let mut snippets = None;
            let file_start = Instant::now();
            tx.upsert_file(&extraction.file)?;
            phase_profile.add_duration("file_manifest_upsert", file_start.elapsed(), 1, 1);

            let persisted_entity_ids = persisted_entity_ids(&extraction.entities);
            let mut entity_count = 0usize;
            let mut edge_count = 0usize;

            for entity in extraction
                .entities
                .iter()
                .filter(|entity| should_persist_entity(entity))
            {
                let entity_start = Instant::now();
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
    summary.profile = Some(IndexProfile {
        file_discovery_ms: 0,
        parse_ms: phase_profile
            .spans
            .get("parse")
            .map(|span| span.elapsed_ms as u128)
            .unwrap_or(0),
        extraction_ms: phase_profile
            .spans
            .get("extract_entities_and_relations")
            .map(|span| span.elapsed_ms as u128)
            .unwrap_or(0),
        semantic_resolver_ms: phase_profile
            .spans
            .get("reducer")
            .map(|span| span.elapsed_ms as u128)
            .unwrap_or(0),
        db_write_ms: phase_profile
            .spans
            .get("entity_insert")
            .map(|span| span.elapsed_ms as u128)
            .unwrap_or(0)
            + phase_profile
                .spans
                .get("edge_insert")
                .map(|span| span.elapsed_ms as u128)
                .unwrap_or(0),
        fts_search_index_ms: phase_profile
            .spans
            .get("snippet_loading")
            .map(|span| span.elapsed_ms as u128)
            .unwrap_or(0),
        vector_signature_ms: 0,
        total_wall_ms,
        files_per_sec: rate_per_second(summary.files_indexed, total_wall_ms),
        entities_per_sec: rate_per_second(summary.entities, total_wall_ms),
        edges_per_sec: rate_per_second(summary.edges, total_wall_ms),
        memory_bytes: current_process_memory_bytes(),
        worker_count: 1,
        skipped_unchanged_files: summary.files_metadata_unchanged,
        spans: phase_profile.into_spans(),
    });
    Ok(summary)
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
    for call in calls.iter().filter(|edge| !edge.derived) {
        for write in writes_by_head
            .get(call.tail_id.as_str())
            .into_iter()
            .flatten()
        {
            plan.push_edge(derived_mutation_edge(call, write));
        }
    }
    plan.sort();
    Ok(plan)
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
        edge_class: if context.eq_ignore_ascii_case("mock") || context.eq_ignore_ascii_case("stub")
        {
            EdgeClass::Mock
        } else {
            EdgeClass::Test
        },
        context: if context.eq_ignore_ascii_case("mock") || context.eq_ignore_ascii_case("stub") {
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
    normalized.ends_with(".test.ts")
        || normalized.ends_with(".test.tsx")
        || normalized.ends_with(".test.js")
        || normalized.ends_with(".test.jsx")
        || normalized.ends_with(".spec.ts")
        || normalized.ends_with(".spec.tsx")
        || normalized.ends_with(".spec.js")
        || normalized.ends_with(".spec.jsx")
}

fn source_may_have_test_relation(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.contains("expect(")
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
            let decision = scope.evaluate_repo_path(relative, ScopePathKind::Directory);
            emit_scope_decision(scope.options(), &decision);
            let excluded = decision.excluded();
            scope_report.record(&decision);
            if excluded && !scope.options().has_include_patterns() {
                return Ok(());
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
    let repo_relative_path = repo_relative_path(root, &normalized)?;
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

pub fn default_db_path(repo_root: &Path) -> PathBuf {
    std::env::var_os("CODEGRAPH_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join(".codegraph").join("codegraph.sqlite"))
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

fn persisted_entity_ids(entities: &[Entity]) -> BTreeSet<String> {
    entities
        .iter()
        .filter(|entity| should_persist_entity(entity))
        .map(|entity| entity.id.clone())
        .collect()
}

fn should_persist_entity(entity: &Entity) -> bool {
    !should_route_static_reference_entity(entity) && !should_route_unresolved_entity(entity)
}

fn should_persist_edge(edge: &Edge, persisted_entity_ids: &BTreeSet<String>) -> bool {
    persisted_entity_ids.contains(&edge.head_id)
        && persisted_entity_ids.contains(&edge.tail_id)
        && !should_route_heuristic_edge(edge)
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

    fn write_test_file(root: &Path, relative: &str, source: &str) {
        let path = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, source).expect("write test file");
    }

    fn collected_rel_paths(root: &Path, options: &IndexScopeOptions) -> BTreeSet<String> {
        collect_repo_files_with_scope(root, options)
            .expect("collect files")
            .files
            .into_iter()
            .map(|path| repo_relative_path(root, &path).expect("relative path"))
            .collect()
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
    fn db_lifecycle_fresh_index_writes_passport_and_warm_reuses() {
        let repo = temp_repo("passport-fresh-warm");
        write_test_file(
            &repo,
            "src/main.ts",
            "export function lifecycle_target() { return 1; }\n",
        );
        let db = repo.join(".codegraph").join("codegraph.sqlite");

        let cold = index_repo_to_db_with_options(&repo, &db, IndexOptions::default())
            .expect("cold index");
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

        let warm = index_repo_to_db_with_options(&repo, &db, IndexOptions::default())
            .expect("warm index");
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
        let summary = index_repo_to_db_with_options(&repo, &db, fresh)
            .expect("--fresh explicit DB rebuilds");
        assert_eq!(
            summary.db_lifecycle.as_ref().map(|lifecycle| lifecycle.decision.as_str()),
            Some("fresh_rebuild")
        );
        assert!(inspect_repo_db_passport(&repo, &db, &IndexOptions::default())
            .expect("preflight")
            .valid);

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
        let preflight_b =
            inspect_repo_db_passport(&repo_b, &shared_db, &IndexOptions::default())
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
