use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, Instant},
};

use codegraph_core::{
    normalize_edge_classification, normalize_repo_relative_path, stable_edge_id,
    stable_entity_id_for_kind, DerivedClosureEdge, Edge, EdgeClass, EdgeContext, Entity,
    EntityKind, Exactness, FileRecord, PathEvidence, RelationKind, RepoIndexState, SourceSpan,
};
use rusqlite::{
    functions::FunctionFlags, params, types::Type, Connection, OpenFlags, OptionalExtension, Row,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use crate::{
    GraphStore, RetrievalTraceRecord, StoreError, StoreResult, TextSearchHit, TextSearchKind,
};

pub const SCHEMA_VERSION: u32 = 20;
pub const DB_PASSPORT_VERSION: u32 = 1;
pub const MAX_SYMBOL_VALUE_BYTES: usize = 512;
pub const MAX_QUALIFIED_NAME_BYTES: usize = 1024;
pub const MAX_QNAME_PREFIX_BYTES: usize = 768;
const CONTENT_TEMPLATE_EXTRACTION_VERSION: &str = "local-fact-template-v1";

const EDGE_FLAG_HEURISTIC: i64 = 1 << 0;
const EDGE_FLAG_UNRESOLVED: i64 = 1 << 1;
const EDGE_FLAG_DYNAMIC_IMPORT: i64 = 1 << 2;
const EDGE_FLAG_STATIC_RESOLUTION: i64 = 1 << 3;
const EDGE_FLAG_RESOLVED: i64 = 1 << 4;
const ENTITY_STRUCTURAL_FLAG_DECLARED_BY_PARENT: i64 = 1 << 0;
const ENTITY_STRUCTURAL_FLAG_CONTAINED_BY_PARENT: i64 = 1 << 1;
const ENTITY_STRUCTURAL_FLAG_DEFINED_IN_PARENT: i64 = 1 << 2;
const DEFAULT_STRUCTURAL_QUERY_LIMIT: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SqliteProfileSpan {
    pub name: String,
    pub elapsed_ms: f64,
    pub count: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct SqliteProfileAccumulator {
    elapsed: Duration,
    count: u64,
}

thread_local! {
    static SQLITE_PROFILE: RefCell<Option<BTreeMap<&'static str, SqliteProfileAccumulator>>> =
        const { RefCell::new(None) };
}

pub fn reset_sqlite_profile() {
    SQLITE_PROFILE.with(|profile| {
        *profile.borrow_mut() = Some(BTreeMap::new());
    });
}

pub fn take_sqlite_profile() -> Vec<SqliteProfileSpan> {
    SQLITE_PROFILE.with(|profile| {
        let Some(accumulators) = profile.borrow_mut().take() else {
            return Vec::new();
        };
        accumulators
            .into_iter()
            .map(|(name, accumulator)| SqliteProfileSpan {
                name: name.to_string(),
                elapsed_ms: duration_ms(accumulator.elapsed),
                count: accumulator.count,
            })
            .collect()
    })
}

fn record_sqlite_profile(name: &'static str, elapsed: Duration) {
    SQLITE_PROFILE.with(|profile| {
        let mut profile = profile.borrow_mut();
        let Some(accumulators) = profile.as_mut() else {
            return;
        };
        let accumulator = accumulators.entry(name).or_default();
        accumulator.elapsed += elapsed;
        accumulator.count += 1;
    });
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageAccountingRow {
    pub name: String,
    pub row_count: u64,
    pub payload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbPassport {
    pub passport_version: u32,
    pub codegraph_schema_version: u32,
    pub storage_mode: String,
    pub index_scope_policy_hash: String,
    pub scope_policy_json: String,
    pub canonical_repo_root: String,
    pub git_remote: Option<String>,
    pub worktree_root: Option<String>,
    pub repo_head: Option<String>,
    pub source_discovery_policy_version: String,
    pub codegraph_build_version: Option<String>,
    pub last_successful_index_timestamp: Option<u64>,
    pub last_completed_run_id: Option<String>,
    pub last_run_status: String,
    pub integrity_gate_result: String,
    pub files_seen: u64,
    pub files_indexed: u64,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedDbPassport {
    pub canonical_repo_root: String,
    pub storage_mode: String,
    pub index_scope_policy_hash: String,
    pub git_remote: Option<String>,
    pub worktree_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbPreflightReport {
    pub db_path: String,
    pub passport_status: String,
    pub valid: bool,
    pub reasons: Vec<String>,
    pub schema_version: Option<u32>,
    pub passport: Option<DbPassport>,
    pub orphan_sidecars: Vec<String>,
}

impl DbPreflightReport {
    pub fn missing(db_path: &Path, reasons: Vec<String>, orphan_sidecars: Vec<PathBuf>) -> Self {
        Self {
            db_path: db_path.display().to_string(),
            passport_status: "missing".to_string(),
            valid: false,
            reasons,
            schema_version: None,
            passport: None,
            orphan_sidecars: orphan_sidecars
                .into_iter()
                .map(|path| path.display().to_string())
                .collect(),
        }
    }
}

pub struct SqliteGraphStore {
    connection: Connection,
    dictionary_cache: RefCell<DictionaryInternCache>,
    entity_file_cache: RefCell<BTreeMap<i64, Vec<String>>>,
}

#[derive(Debug, Default)]
struct DictionaryInternCache {
    values: BTreeMap<(&'static str, String), i64>,
    entity_ids: BTreeMap<String, i64>,
    file_ids: BTreeMap<String, CachedFileIds>,
    content_templates: BTreeMap<(String, i64), i64>,
    next_dense_entity_id: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
struct CachedFileIds {
    path_id: i64,
    file_id: i64,
    has_content_hash: bool,
}

impl SqliteGraphStore {
    pub fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        let connection_start = Instant::now();
        let connection = Connection::open(path)?;
        record_sqlite_profile("open_connection", connection_start.elapsed());
        let store = Self {
            connection,
            dictionary_cache: RefCell::new(DictionaryInternCache::default()),
            entity_file_cache: RefCell::new(BTreeMap::new()),
        };
        let configure_start = Instant::now();
        store.configure()?;
        record_sqlite_profile("configure_pragmas", configure_start.elapsed());
        let migrate_start = Instant::now();
        store.migrate()?;
        record_sqlite_profile("migrate_schema", migrate_start.elapsed());
        Ok(store)
    }

    pub fn open_in_memory() -> StoreResult<Self> {
        let store = Self {
            connection: Connection::open_in_memory()?,
            dictionary_cache: RefCell::new(DictionaryInternCache::default()),
            entity_file_cache: RefCell::new(BTreeMap::new()),
        };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    pub fn transaction<T>(&self, f: impl FnOnce(&Self) -> StoreResult<T>) -> StoreResult<T> {
        self.begin_write_transaction()?;
        match f(self) {
            Ok(value) => {
                self.commit_write_transaction()?;
                Ok(value)
            }
            Err(error) => {
                let _ = self.rollback_write_transaction();
                Err(error)
            }
        }
    }

    pub fn begin_write_transaction(&self) -> StoreResult<()> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        Ok(())
    }

    pub fn commit_write_transaction(&self) -> StoreResult<()> {
        self.connection.execute_batch("COMMIT")?;
        Ok(())
    }

    pub fn rollback_write_transaction(&self) -> StoreResult<()> {
        self.connection.execute_batch("ROLLBACK")?;
        self.clear_write_caches();
        Ok(())
    }

    pub fn rollback_bulk_index_transaction(&self) -> StoreResult<()> {
        self.rollback_write_transaction()
    }

    fn clear_write_caches(&self) {
        *self.dictionary_cache.borrow_mut() = DictionaryInternCache::default();
        self.entity_file_cache.borrow_mut().clear();
    }

    pub fn wal_checkpoint_truncate(&self) -> StoreResult<()> {
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
        Ok(())
    }

    pub fn integrity_check(&self) -> StoreResult<()> {
        validate_sqlite_check_rows(&self.connection, "integrity_check")
    }

    pub fn quick_check(&self) -> StoreResult<()> {
        validate_sqlite_check_rows(&self.connection, "quick_check")
    }

    pub fn foreign_key_check(&self) -> StoreResult<()> {
        let mut statement = self.connection.prepare("PRAGMA foreign_key_check")?;
        let mut rows = statement.query([])?;
        let mut failures = Vec::new();
        while let Some(row) = rows.next()? {
            let table: String = row.get(0)?;
            let rowid: Option<i64> = row.get(1)?;
            let parent: String = row.get(2)?;
            let fkid: i64 = row.get(3)?;
            failures.push(format!(
                "{table} rowid={} parent={parent} fkid={fkid}",
                rowid
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".to_string())
            ));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(StoreError::Message(format!(
                "SQLite foreign_key_check failed: {}",
                failures.join("; ")
            )))
        }
    }

    pub fn quick_integrity_gate(&self) -> StoreResult<()> {
        self.quick_check()?;
        self.foreign_key_check()?;
        Ok(())
    }

    pub fn full_integrity_gate(&self) -> StoreResult<()> {
        self.integrity_check()?;
        self.foreign_key_check()?;
        Ok(())
    }

    pub fn upsert_db_passport(&self, passport: &DbPassport) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO codegraph_db_passport (
                id, passport_version, codegraph_schema_version, storage_mode,
                index_scope_policy_hash, scope_policy_json, canonical_repo_root,
                git_remote, worktree_root, repo_head, source_discovery_policy_version,
                codegraph_build_version, last_successful_index_timestamp,
                last_completed_run_id, last_run_status, integrity_gate_result,
                files_seen, files_indexed, created_at_unix_ms, updated_at_unix_ms
            ) VALUES (
                1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19
            )
            ON CONFLICT(id) DO UPDATE SET
                passport_version = excluded.passport_version,
                codegraph_schema_version = excluded.codegraph_schema_version,
                storage_mode = excluded.storage_mode,
                index_scope_policy_hash = excluded.index_scope_policy_hash,
                scope_policy_json = excluded.scope_policy_json,
                canonical_repo_root = excluded.canonical_repo_root,
                git_remote = excluded.git_remote,
                worktree_root = excluded.worktree_root,
                repo_head = excluded.repo_head,
                source_discovery_policy_version = excluded.source_discovery_policy_version,
                codegraph_build_version = excluded.codegraph_build_version,
                last_successful_index_timestamp = excluded.last_successful_index_timestamp,
                last_completed_run_id = excluded.last_completed_run_id,
                last_run_status = excluded.last_run_status,
                integrity_gate_result = excluded.integrity_gate_result,
                files_seen = excluded.files_seen,
                files_indexed = excluded.files_indexed,
                updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                passport.passport_version,
                passport.codegraph_schema_version,
                passport.storage_mode,
                passport.index_scope_policy_hash,
                passport.scope_policy_json,
                passport.canonical_repo_root,
                passport.git_remote.as_deref(),
                passport.worktree_root.as_deref(),
                passport.repo_head.as_deref(),
                passport.source_discovery_policy_version,
                passport.codegraph_build_version.as_deref(),
                passport
                    .last_successful_index_timestamp
                    .map(|value| i64::try_from(value).unwrap_or(i64::MAX)),
                passport.last_completed_run_id.as_deref(),
                passport.last_run_status,
                passport.integrity_gate_result,
                i64::try_from(passport.files_seen).unwrap_or(i64::MAX),
                i64::try_from(passport.files_indexed).unwrap_or(i64::MAX),
                i64::try_from(passport.created_at_unix_ms).unwrap_or(i64::MAX),
                i64::try_from(passport.updated_at_unix_ms).unwrap_or(i64::MAX),
            ],
        )?;
        Ok(())
    }

    pub fn get_db_passport(&self) -> StoreResult<Option<DbPassport>> {
        read_db_passport(&self.connection)
    }

    /// Insert snippet FTS after the caller has already removed stale rows for
    /// the file. This avoids a per-snippet FTS delete during batch indexing.
    pub fn insert_snippet_text_after_file_delete(
        &self,
        id: &str,
        span: &SourceSpan,
        text: &str,
    ) -> StoreResult<()> {
        insert_fts_row(
            &self.connection,
            TextSearchKind::Snippet,
            id,
            &span.repo_relative_path,
            Some(span.start_line),
            &span.to_string(),
            text,
        )
    }

    pub fn insert_file_text_after_file_delete(
        &self,
        repo_relative_path: &str,
        text: &str,
    ) -> StoreResult<()> {
        insert_fts_row(
            &self.connection,
            TextSearchKind::File,
            repo_relative_path,
            repo_relative_path,
            None,
            repo_relative_path,
            text,
        )
    }

    pub fn insert_entity_after_file_delete(&self, entity: &Entity) -> StoreResult<()> {
        self.insert_entity_row_after_file_delete(entity)?;
        insert_entity_fts_row(&self.connection, entity)
    }

    pub fn insert_entity_record_after_file_delete(&self, entity: &Entity) -> StoreResult<()> {
        self.insert_entity_row_after_file_delete(entity)
    }

    fn insert_entity_row_after_file_delete(&self, entity: &Entity) -> StoreResult<()> {
        validate_entity_identity_fields(entity)?;
        let mut dictionary_cache = self.dictionary_cache.borrow_mut();
        let entity_id =
            intern_entity_object_id_cached(&self.connection, &mut dictionary_cache, &entity.id)?;
        let kind_id = intern_dict_value_cached(
            &self.connection,
            &mut dictionary_cache,
            "entity_kind_dict",
            &entity.kind.to_string(),
        )?;
        let name_id = intern_symbol_cached(&self.connection, &mut dictionary_cache, &entity.name)?;
        let qualified_name_id = intern_qualified_name_cached(
            &self.connection,
            &mut dictionary_cache,
            &entity.qualified_name,
        )?;
        let (path_id, file_id) = ensure_file_for_path_cached(
            &self.connection,
            &mut dictionary_cache,
            &entity.repo_relative_path,
            entity.file_hash.as_deref(),
        )?;
        let created_from_id = intern_dict_value_cached(
            &self.connection,
            &mut dictionary_cache,
            "extractor_dict",
            &entity.created_from,
        )?;
        let span = entity.source_span.as_ref();
        let span_path_id = match span {
            Some(span) => Some(intern_path_cached(
                &self.connection,
                &mut dictionary_cache,
                &span.repo_relative_path,
            )?),
            None => None,
        };
        let declaration_span_id = span.map(|_| entity_id);
        let mut statement = self.connection.prepare_cached(
            "
            INSERT INTO entities (
                id_key, entity_hash, kind_id, name_id, qualified_name_id, path_id,
                span_path_id, start_line, start_column, end_line, end_column,
                content_hash, created_from_id, confidence, metadata_json,
                parent_id, file_id, scope_id, declaration_span_id, structural_flags
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
            ",
        )?;
        let sql_start = Instant::now();
        statement.execute(params![
            entity_id,
            entity_hash_blob(&entity.id),
            kind_id,
            name_id,
            qualified_name_id,
            path_id,
            span_path_id,
            span.map(|span| span.start_line),
            span.and_then(|span| span.start_column),
            span.map(|span| span.end_line),
            span.and_then(|span| span.end_column),
            entity.content_hash,
            created_from_id,
            entity.confidence,
            "{}",
            None::<i64>,
            file_id,
            None::<i64>,
            declaration_span_id,
            0_i64,
        ])?;
        insert_entity_file_map_after_file_delete(
            &self.connection,
            &entity.repo_relative_path,
            &entity.id,
        )?;
        cache_entity_file_id(
            &self.entity_file_cache,
            entity_id,
            &entity.repo_relative_path,
        );
        record_sqlite_profile("entity_insert_sql", sql_start.elapsed());
        Ok(())
    }

    fn write_entity(&self, entity: &Entity, replace_fts: bool) -> StoreResult<()> {
        validate_entity_identity_fields(entity)?;
        let mut dictionary_cache = self.dictionary_cache.borrow_mut();
        let entity_id =
            intern_entity_object_id_cached(&self.connection, &mut dictionary_cache, &entity.id)?;
        let kind_id = intern_dict_value_cached(
            &self.connection,
            &mut dictionary_cache,
            "entity_kind_dict",
            &entity.kind.to_string(),
        )?;
        let name_id = intern_symbol_cached(&self.connection, &mut dictionary_cache, &entity.name)?;
        let qualified_name_id = intern_qualified_name_cached(
            &self.connection,
            &mut dictionary_cache,
            &entity.qualified_name,
        )?;
        let (path_id, file_id) = ensure_file_for_path_cached(
            &self.connection,
            &mut dictionary_cache,
            &entity.repo_relative_path,
            entity.file_hash.as_deref(),
        )?;
        let created_from_id = intern_dict_value_cached(
            &self.connection,
            &mut dictionary_cache,
            "extractor_dict",
            &entity.created_from,
        )?;
        let span = entity.source_span.as_ref();
        let span_path_id = match span {
            Some(span) => Some(intern_path_cached(
                &self.connection,
                &mut dictionary_cache,
                &span.repo_relative_path,
            )?),
            None => None,
        };
        let declaration_span_id = span.map(|_| entity_id);
        let mut statement = self.connection.prepare_cached(
            "
            INSERT INTO entities (
                id_key, entity_hash, kind_id, name_id, qualified_name_id, path_id,
                span_path_id, start_line, start_column, end_line, end_column,
                content_hash, created_from_id, confidence, metadata_json,
                parent_id, file_id, scope_id, declaration_span_id, structural_flags
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
            ON CONFLICT(id_key) DO UPDATE SET
                entity_hash = excluded.entity_hash,
                kind_id = excluded.kind_id,
                name_id = excluded.name_id,
                qualified_name_id = excluded.qualified_name_id,
                path_id = excluded.path_id,
                span_path_id = excluded.span_path_id,
                start_line = excluded.start_line,
                start_column = excluded.start_column,
                end_line = excluded.end_line,
                end_column = excluded.end_column,
                content_hash = excluded.content_hash,
                created_from_id = excluded.created_from_id,
                confidence = excluded.confidence,
                metadata_json = excluded.metadata_json,
                file_id = excluded.file_id,
                declaration_span_id = excluded.declaration_span_id
            ",
        )?;
        let sql_start = Instant::now();
        statement.execute(params![
            entity_id,
            entity_hash_blob(&entity.id),
            kind_id,
            name_id,
            qualified_name_id,
            path_id,
            span_path_id,
            span.map(|span| span.start_line),
            span.and_then(|span| span.start_column),
            span.map(|span| span.end_line),
            span.and_then(|span| span.end_column),
            entity.content_hash,
            created_from_id,
            entity.confidence,
            to_json(&entity.metadata)?,
            None::<i64>,
            file_id,
            None::<i64>,
            declaration_span_id,
            0_i64,
        ])?;
        map_entity_to_file(&self.connection, &entity.repo_relative_path, &entity.id)?;
        cache_entity_file_id(
            &self.entity_file_cache,
            entity_id,
            &entity.repo_relative_path,
        );
        record_sqlite_profile("entity_insert_sql", sql_start.elapsed());

        if replace_fts {
            upsert_entity_fts_row(&self.connection, entity)?;
        } else {
            insert_entity_fts_row(&self.connection, entity)?;
        }
        Ok(())
    }

    fn write_edge(&self, edge: &Edge, storage: EdgeIdStorage) -> StoreResult<()> {
        let edge = normalize_edge_for_storage(edge)?;
        let mut dictionary_cache = self.dictionary_cache.borrow_mut();
        match compact_storage_for_relation(edge.relation) {
            CompactFactStorage::GenericEdge => {}
            CompactFactStorage::StructuralRelation => {
                return write_structural_relation(
                    &self.connection,
                    &mut dictionary_cache,
                    &edge,
                    storage,
                );
            }
            CompactFactStorage::CallsiteCallee => {
                return write_callsite_callee(
                    &self.connection,
                    &mut dictionary_cache,
                    &self.entity_file_cache,
                    &edge,
                    storage,
                );
            }
            CompactFactStorage::CallsiteArgument { ordinal } => {
                return write_callsite_argument(
                    &self.connection,
                    &mut dictionary_cache,
                    &self.entity_file_cache,
                    &edge,
                    storage,
                    ordinal,
                );
            }
        }
        let edge_id = match storage {
            EdgeIdStorage::Compact => compact_edge_key(&edge.id),
            EdgeIdStorage::ExistingOrCompact => edge_storage_key(&self.connection, &edge.id)?,
        };
        if matches!(storage, EdgeIdStorage::ExistingOrCompact) {
            delete_compact_fact_by_key(&self.connection, edge_id)?;
        }
        let head_id =
            intern_object_id_cached(&self.connection, &mut dictionary_cache, &edge.head_id)?;
        let tail_id =
            intern_object_id_cached(&self.connection, &mut dictionary_cache, &edge.tail_id)?;
        let relation_id = intern_dict_value_cached(
            &self.connection,
            &mut dictionary_cache,
            "relation_kind_dict",
            &edge.relation.to_string(),
        )?;
        let (span_path_id, file_id) = ensure_file_for_path_cached(
            &self.connection,
            &mut dictionary_cache,
            &edge.source_span.repo_relative_path,
            edge.file_hash.as_deref(),
        )?;
        let extractor_id = intern_dict_value_cached(
            &self.connection,
            &mut dictionary_cache,
            "extractor_dict",
            &edge.extractor,
        )?;
        let exactness_id = intern_dict_value_cached(
            &self.connection,
            &mut dictionary_cache,
            "exactness_dict",
            &edge.exactness.to_string(),
        )?;
        let resolution_kind = edge_resolution_kind(&edge);
        let resolution_kind_id = intern_dict_value_cached(
            &self.connection,
            &mut dictionary_cache,
            "resolution_kind_dict",
            &resolution_kind,
        )?;
        let edge_class_id = intern_dict_value_cached(
            &self.connection,
            &mut dictionary_cache,
            "edge_class_dict",
            &edge.edge_class.to_string(),
        )?;
        let context_id = intern_dict_value_cached(
            &self.connection,
            &mut dictionary_cache,
            "edge_context_dict",
            &edge.context.to_string(),
        )?;
        let flags_bitset = edge_flags_bitset(&edge, &resolution_kind);
        let confidence_q = quantize_confidence(edge.confidence);
        let provenance_edges_json = to_json(&edge.provenance_edges)?;
        let provenance_id = intern_dict_value_cached(
            &self.connection,
            &mut dictionary_cache,
            "edge_provenance_dict",
            &provenance_edges_json,
        )?;
        let mut statement = self.connection.prepare_cached(
            "
            INSERT INTO edges (
                id_key, head_id_key, relation_id, tail_id_key,
                span_path_id, start_line, start_column, end_line, end_column,
                repo_commit, file_id, extractor_id, confidence, exactness_id,
                confidence_q, resolution_kind_id, edge_class_id, context_id,
                context_kind_id, flags_bitset, derived, provenance_edges_json, provenance_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
            ON CONFLICT(id_key) DO UPDATE SET
                head_id_key = excluded.head_id_key,
                relation_id = excluded.relation_id,
                tail_id_key = excluded.tail_id_key,
                span_path_id = excluded.span_path_id,
                start_line = excluded.start_line,
                start_column = excluded.start_column,
                end_line = excluded.end_line,
                end_column = excluded.end_column,
                repo_commit = excluded.repo_commit,
                file_id = excluded.file_id,
                extractor_id = excluded.extractor_id,
                confidence = excluded.confidence,
                exactness_id = excluded.exactness_id,
                confidence_q = excluded.confidence_q,
                resolution_kind_id = excluded.resolution_kind_id,
                edge_class_id = excluded.edge_class_id,
                context_id = excluded.context_id,
                context_kind_id = excluded.context_kind_id,
                flags_bitset = excluded.flags_bitset,
                derived = excluded.derived,
                provenance_edges_json = excluded.provenance_edges_json,
                provenance_id = excluded.provenance_id
            ",
        )?;
        let sql_start = Instant::now();
        statement.execute(params![
            edge_id,
            head_id,
            relation_id,
            tail_id,
            span_path_id,
            edge.source_span.start_line,
            edge.source_span.start_column,
            edge.source_span.end_line,
            edge.source_span.end_column,
            edge.repo_commit,
            file_id,
            extractor_id,
            edge.confidence,
            exactness_id,
            confidence_q,
            resolution_kind_id,
            edge_class_id,
            context_id,
            context_id,
            flags_bitset,
            edge.derived,
            provenance_edges_json,
            provenance_id,
        ])?;
        self.connection.execute(
            "DELETE FROM edge_debug_metadata WHERE edge_id = ?1",
            [edge_id],
        )?;
        if storage == EdgeIdStorage::Compact {
            insert_edge_file_map_after_file_delete(
                &self.connection,
                &edge.source_span.repo_relative_path,
                &edge.id,
            )?;
        } else {
            map_edge_to_file(
                &self.connection,
                &edge.source_span.repo_relative_path,
                &edge.id,
            )?;
        }
        map_edge_to_entity_files_cached(
            &self.connection,
            &self.entity_file_cache,
            &edge.id,
            head_id,
            tail_id,
        )?;
        if storage == EdgeIdStorage::Compact {
            insert_source_span_file_map_after_file_delete(
                &self.connection,
                &edge.source_span.repo_relative_path,
                &edge.id,
            )?;
        } else {
            map_source_span_to_file(
                &self.connection,
                &edge.source_span.repo_relative_path,
                &edge.id,
            )?;
        }
        record_sqlite_profile("edge_insert_sql", sql_start.elapsed());
        Ok(())
    }

    pub fn insert_edge_after_file_delete(&self, edge: &Edge) -> StoreResult<()> {
        self.write_edge(edge, EdgeIdStorage::Compact)
    }

    pub fn insert_heuristic_edge_after_file_delete(&self, edge: &Edge) -> StoreResult<()> {
        let edge = normalize_edge_for_storage(edge)?;
        let source_span = &edge.source_span;
        self.connection.execute(
            "
            INSERT INTO heuristic_edges (
                id_key, edge_id, head_id, relation, tail_id,
                source_span_path, start_line, start_column, end_line, end_column,
                repo_commit, file_hash, extractor, confidence, exactness,
                edge_class, context, derived, provenance_edges_json, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
            ON CONFLICT(id_key) DO UPDATE SET
                edge_id = excluded.edge_id,
                head_id = excluded.head_id,
                relation = excluded.relation,
                tail_id = excluded.tail_id,
                source_span_path = excluded.source_span_path,
                start_line = excluded.start_line,
                start_column = excluded.start_column,
                end_line = excluded.end_line,
                end_column = excluded.end_column,
                repo_commit = excluded.repo_commit,
                file_hash = excluded.file_hash,
                extractor = excluded.extractor,
                confidence = excluded.confidence,
                exactness = excluded.exactness,
                edge_class = excluded.edge_class,
                context = excluded.context,
                derived = excluded.derived,
                provenance_edges_json = excluded.provenance_edges_json,
                metadata_json = excluded.metadata_json
            ",
            params![
                compact_object_key(&format!("heuristic-edge:{}", edge.id)),
                edge.id,
                edge.head_id,
                edge.relation.to_string(),
                edge.tail_id,
                normalize_repo_relative_path(&source_span.repo_relative_path),
                source_span.start_line,
                source_span.start_column,
                source_span.end_line,
                source_span.end_column,
                edge.repo_commit,
                edge.file_hash,
                edge.extractor,
                edge.confidence,
                edge.exactness.to_string(),
                edge.edge_class.to_string(),
                edge.context.to_string(),
                edge.derived,
                to_json(&edge.provenance_edges)?,
                to_json(&edge.metadata)?,
            ],
        )?;
        Ok(())
    }

    pub fn insert_static_reference_after_file_delete(&self, entity: &Entity) -> StoreResult<()> {
        validate_entity_identity_fields(entity)?;
        let span = entity.source_span.as_ref();
        self.connection.execute(
            "
            INSERT INTO static_references (
                id_key, entity_id, kind, name, qualified_name, repo_relative_path,
                source_span_path, start_line, start_column, end_line, end_column,
                file_hash, created_from, confidence, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(id_key) DO UPDATE SET
                entity_id = excluded.entity_id,
                kind = excluded.kind,
                name = excluded.name,
                qualified_name = excluded.qualified_name,
                repo_relative_path = excluded.repo_relative_path,
                source_span_path = excluded.source_span_path,
                start_line = excluded.start_line,
                start_column = excluded.start_column,
                end_line = excluded.end_line,
                end_column = excluded.end_column,
                file_hash = excluded.file_hash,
                created_from = excluded.created_from,
                confidence = excluded.confidence,
                metadata_json = excluded.metadata_json
            ",
            params![
                compact_object_key(&format!("static-reference:{}", entity.id)),
                entity.id.as_str(),
                entity.kind.to_string(),
                entity.name.as_str(),
                entity.qualified_name.as_str(),
                normalize_repo_relative_path(&entity.repo_relative_path),
                span.map(|span| normalize_repo_relative_path(&span.repo_relative_path)),
                span.map(|span| span.start_line),
                span.and_then(|span| span.start_column),
                span.map(|span| span.end_line),
                span.and_then(|span| span.end_column),
                entity.file_hash.as_deref(),
                entity.created_from.as_str(),
                entity.confidence,
                to_json(&entity.metadata)?,
            ],
        )?;
        Ok(())
    }

    pub fn insert_unresolved_reference_after_file_delete(
        &self,
        reference_id: &str,
        name: &str,
        relation: RelationKind,
        source_span: &SourceSpan,
        file_hash: Option<&str>,
        exactness: Exactness,
        extractor: &str,
        metadata: &Value,
    ) -> StoreResult<()> {
        let key_material = format!(
            "unresolved-reference:{reference_id}:{relation}:{}:{}:{}:{}",
            source_span.repo_relative_path,
            source_span.start_line,
            source_span.start_column.unwrap_or(0),
            source_span.end_line,
        );
        self.connection.execute(
            "
            INSERT INTO unresolved_references (
                id_key, reference_id, name, relation, source_span_path,
                start_line, start_column, end_line, end_column, file_hash,
                exactness, extractor, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(id_key) DO UPDATE SET
                reference_id = excluded.reference_id,
                name = excluded.name,
                relation = excluded.relation,
                source_span_path = excluded.source_span_path,
                start_line = excluded.start_line,
                start_column = excluded.start_column,
                end_line = excluded.end_line,
                end_column = excluded.end_column,
                file_hash = excluded.file_hash,
                exactness = excluded.exactness,
                extractor = excluded.extractor,
                metadata_json = excluded.metadata_json
            ",
            params![
                compact_object_key(&key_material),
                reference_id,
                name,
                relation.to_string(),
                normalize_repo_relative_path(&source_span.repo_relative_path),
                source_span.start_line,
                source_span.start_column,
                source_span.end_line,
                source_span.end_column,
                file_hash,
                exactness.to_string(),
                extractor,
                to_json(metadata)?,
            ],
        )?;
        Ok(())
    }

    pub fn insert_extraction_warning_after_file_delete(
        &self,
        repo_relative_path: &str,
        file_hash: Option<&str>,
        warning: &str,
        metadata: &Value,
    ) -> StoreResult<()> {
        let repo_relative_path = normalize_repo_relative_path(repo_relative_path);
        self.connection.execute(
            "
            INSERT INTO extraction_warnings (
                id_key, repo_relative_path, file_hash, warning, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id_key) DO UPDATE SET
                repo_relative_path = excluded.repo_relative_path,
                file_hash = excluded.file_hash,
                warning = excluded.warning,
                metadata_json = excluded.metadata_json
            ",
            params![
                compact_object_key(&format!(
                    "extraction-warning:{repo_relative_path}:{}:{warning}",
                    file_hash.unwrap_or_default()
                )),
                repo_relative_path,
                file_hash,
                warning,
                to_json(metadata)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_heuristic_edges(&self, limit: usize) -> StoreResult<Vec<Edge>> {
        if limit == 0 || !self.table_exists("heuristic_edges")? {
            return Ok(Vec::new());
        }
        let mut statement = self.connection.prepare(
            "
            SELECT edge_id AS id, head_id, relation, tail_id,
                   source_span_path AS span_repo_relative_path,
                   start_line, start_column, end_line, end_column,
                   repo_commit, file_hash, extractor, confidence, exactness,
                   edge_class, context, derived, provenance_edges_json, metadata_json
            FROM heuristic_edges
            ORDER BY id_key
            LIMIT ?1
            ",
        )?;
        let rows = statement.query_map([limit as i64], edge_from_row)?;
        collect_rows(rows)
    }

    pub fn list_stored_edges_by_relation(
        &self,
        relation: RelationKind,
        limit: usize,
    ) -> StoreResult<Vec<Edge>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let Some(relation_id) = lookup_relation_kind(&self.connection, &relation.to_string())?
        else {
            return Ok(Vec::new());
        };
        query_edges(
            &self.connection,
            EDGE_SELECT_BY_RELATION,
            params![relation_id, limit as i64],
        )
    }

    pub fn list_static_references(&self, limit: usize) -> StoreResult<Vec<Entity>> {
        if limit == 0 || !self.table_exists("static_references")? {
            return Ok(Vec::new());
        }
        let mut statement = self.connection.prepare(
            "
            SELECT entity_id AS id, kind, name, qualified_name, repo_relative_path,
                   source_span_path AS span_repo_relative_path,
                   start_line, start_column, end_line, end_column,
                   NULL AS content_hash, file_hash, created_from, confidence, metadata_json
            FROM static_references
            ORDER BY id_key
            LIMIT ?1
            ",
        )?;
        let rows = statement.query_map([limit as i64], entity_from_row)?;
        collect_rows(rows)
    }

    pub fn insert_source_span_after_file_delete(
        &self,
        id: &str,
        span: &SourceSpan,
    ) -> StoreResult<()> {
        let id_key = intern_object_id(&self.connection, id)?;
        let path_id = intern_path(&self.connection, &span.repo_relative_path)?;
        let mut statement = self.connection.prepare_cached(
            "
            INSERT INTO source_spans (
                id_key, path_id, start_line, start_column,
                end_line, end_column
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id_key) DO UPDATE SET
                path_id = excluded.path_id,
                start_line = excluded.start_line,
                start_column = excluded.start_column,
                end_line = excluded.end_line,
                end_column = excluded.end_column
            ",
        )?;
        let sql_start = Instant::now();
        statement.execute(params![
            id_key,
            path_id,
            span.start_line,
            span.start_column,
            span.end_line,
            span.end_column,
        ])?;
        insert_source_span_file_map_after_file_delete(
            &self.connection,
            &span.repo_relative_path,
            id,
        )?;
        record_sqlite_profile("source_span_insert_sql", sql_start.elapsed());
        Ok(())
    }

    pub fn begin_bulk_index_load(&self) -> StoreResult<()> {
        self.connection.execute_batch(BULK_INDEX_DROP_SQL)?;
        Ok(())
    }

    pub fn begin_atomic_cold_bulk_index_load(&self) -> StoreResult<()> {
        self.connection
            .execute_batch(ATOMIC_COLD_BULK_INDEX_PRAGMAS_SQL)?;
        Ok(())
    }

    pub fn drop_bulk_index_lookup_indexes(&self) -> StoreResult<()> {
        self.connection
            .execute_batch(BULK_INDEX_DROP_LOOKUP_INDEXES_SQL)?;
        Ok(())
    }

    pub fn finish_bulk_index_load(&self) -> StoreResult<()> {
        self.connection.execute_batch(BULK_INDEX_CREATE_SQL)?;
        Ok(())
    }

    pub fn finish_bulk_index_load_fast(&self) -> StoreResult<()> {
        self.connection.execute_batch(BULK_INDEX_CREATE_FAST_SQL)?;
        Ok(())
    }

    pub fn begin_bulk_index_transaction(&self) -> StoreResult<()> {
        self.begin_write_transaction()
    }

    pub fn commit_bulk_index_transaction(&self) -> StoreResult<()> {
        self.commit_write_transaction()
    }

    pub fn clear_indexed_facts(&self) -> StoreResult<()> {
        self.connection.execute_batch(
            "
            DELETE FROM stage0_fts;
            DELETE FROM retrieval_traces;
            DELETE FROM derived_edges;
            DELETE FROM path_evidence;
            DELETE FROM repo_index_state;
            DELETE FROM source_spans;
            DELETE FROM edge_debug_metadata;
            DELETE FROM heuristic_edges;
            DELETE FROM unresolved_references;
            DELETE FROM static_references;
            DELETE FROM extraction_warnings;
            DELETE FROM edges;
            DELETE FROM structural_relations;
            DELETE FROM callsites;
            DELETE FROM callsite_args;
            DELETE FROM entities;
            DELETE FROM files;
            DELETE FROM file_graph_digests;
            DELETE FROM repo_graph_digest;
            ",
        )?;
        Ok(())
    }

    pub fn table_exists(&self, table_name: &str) -> StoreResult<bool> {
        exists_in_sqlite_master(&self.connection, "table", table_name)
    }

    pub fn index_exists(&self, index_name: &str) -> StoreResult<bool> {
        exists_in_sqlite_master(&self.connection, "index", index_name)
    }

    pub fn storage_accounting(&self) -> StoreResult<Vec<StorageAccountingRow>> {
        let specs = [
            (
                "object_id_dict",
                "SELECT COALESCE(SUM(length(value)), 0) FROM object_id_dict",
            ),
            (
                "path_dict",
                "SELECT COALESCE(SUM(length(value)), 0) FROM path_dict",
            ),
            (
                "symbol_dict",
                "SELECT COALESCE(SUM(length(value)), 0) FROM symbol_dict",
            ),
            (
                "qualified_name_dict",
                "SELECT COALESCE(SUM(length(value)), 0) FROM qualified_name_dict",
            ),
            (
                "qname_prefix_dict",
                "SELECT COALESCE(SUM(length(value)), 0) FROM qname_prefix_dict",
            ),
            (
                "entities",
                "SELECT COALESCE(SUM(COALESCE(length(entity_hash), 0) + COALESCE(length(content_hash), 0) + COALESCE(length(metadata_json), 0) + 8), 0) FROM entities",
            ),
            (
                "edges",
                "SELECT COALESCE(SUM(COALESCE(length(repo_commit), 0) + COALESCE(length(provenance_edges_json), 0) + 40), 0) FROM edges",
            ),
            (
                "heuristic_edges",
                "SELECT COALESCE(SUM(length(edge_id) + length(head_id) + length(relation) + length(tail_id) + length(source_span_path) + COALESCE(length(repo_commit), 0) + COALESCE(length(file_hash), 0) + length(extractor) + length(exactness) + length(edge_class) + length(context) + length(provenance_edges_json) + length(metadata_json) + 48), 0) FROM heuristic_edges",
            ),
            (
                "unresolved_references",
                "SELECT COALESCE(SUM(length(reference_id) + length(name) + length(relation) + length(source_span_path) + COALESCE(length(file_hash), 0) + length(exactness) + length(extractor) + length(metadata_json) + 40), 0) FROM unresolved_references",
            ),
            (
                "static_references",
                "SELECT COALESCE(SUM(length(entity_id) + length(kind) + length(name) + length(qualified_name) + length(repo_relative_path) + COALESCE(length(source_span_path), 0) + COALESCE(length(file_hash), 0) + length(created_from) + length(metadata_json) + 40), 0) FROM static_references",
            ),
            (
                "extraction_warnings",
                "SELECT COALESCE(SUM(length(repo_relative_path) + COALESCE(length(file_hash), 0) + length(warning) + length(metadata_json) + 16), 0) FROM extraction_warnings",
            ),
            (
                "structural_relations",
                "SELECT COALESCE(SUM(COALESCE(length(repo_commit), 0) + COALESCE(length(metadata_json), 0) + 8), 0) FROM structural_relations",
            ),
            (
                "callsites",
                "SELECT COALESCE(SUM(COALESCE(length(repo_commit), 0) + COALESCE(length(metadata_json), 0) + 8), 0) FROM callsites",
            ),
            (
                "callsite_args",
                "SELECT COALESCE(SUM(COALESCE(length(repo_commit), 0) + COALESCE(length(metadata_json), 0) + 8), 0) FROM callsite_args",
            ),
            (
                "source_spans",
                "SELECT COUNT(*) * 24 FROM source_spans",
            ),
            (
                "files",
                "SELECT COALESCE(SUM(COALESCE(length(content_hash), 0) + COALESCE(length(metadata_json), 0) + 24), 0) FROM files",
            ),
            (
                "stage0_fts",
                "SELECT COALESCE(SUM(length(id) + length(repo_relative_path) + length(title) + length(body)), 0) FROM stage0_fts",
            ),
            (
                "file_entities",
                "SELECT COALESCE(SUM(length(file_id) + length(entity_id)), 0) FROM file_entities",
            ),
            (
                "file_edges",
                "SELECT COALESCE(SUM(length(file_id) + length(edge_id)), 0) FROM file_edges",
            ),
            (
                "file_source_spans",
                "SELECT COALESCE(SUM(length(file_id) + length(span_id)), 0) FROM file_source_spans",
            ),
            (
                "file_path_evidence",
                "SELECT COALESCE(SUM(length(file_id) + length(path_id)), 0) FROM file_path_evidence",
            ),
            (
                "file_fts_rows",
                "SELECT COALESCE(SUM(length(file_id) + length(kind) + length(object_id) + 8), 0) FROM file_fts_rows",
            ),
            (
                "file_graph_digests",
                "SELECT COALESCE(SUM(length(file_id) + length(digest)), 0) FROM file_graph_digests",
            ),
        ];
        let mut rows = Vec::new();
        for (name, payload_sql) in specs {
            if !self.table_exists(name)? {
                continue;
            }
            rows.push(StorageAccountingRow {
                name: name.to_string(),
                row_count: count_rows(&self.connection, name)?,
                payload_bytes: self
                    .connection
                    .query_row(payload_sql, [], |row| row.get::<_, i64>(0))?
                    .max(0) as u64,
            });
        }
        rows.sort_by(|left, right| {
            right
                .payload_bytes
                .cmp(&left.payload_bytes)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(rows)
    }

    pub fn count_source_spans(&self) -> StoreResult<u64> {
        let entity_spans: u64 = self.connection.query_row(
            "SELECT COUNT(*) FROM entities WHERE start_line IS NOT NULL AND end_line IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        let derived_structural =
            entity_attribute_relation_count(&self.connection, RelationKind::Contains)?
                + entity_attribute_relation_count(&self.connection, RelationKind::DefinedIn)?
                + entity_attribute_relation_count(&self.connection, RelationKind::Declares)?;
        Ok(entity_spans
            + count_rows(&self.connection, "source_spans")?
            + count_rows(&self.connection, "edges")?
            + compact_fact_row_count(&self.connection)?
            + derived_structural)
    }

    pub fn graph_fact_digest(&self) -> StoreResult<String> {
        let mut hash = FNV_OFFSET;
        digest_query_rows(&self.connection, &mut hash, ENTITY_DIGEST_SQL)?;
        digest_query_rows(&self.connection, &mut hash, EDGE_DIGEST_SQL)?;
        Ok(format!("fnv64:{hash:016x}"))
    }

    pub fn current_file_graph_digest(&self, file_id: &str) -> StoreResult<String> {
        let digest = current_file_graph_digest_u64(
            &self.connection,
            &normalize_repo_relative_path(file_id),
        )?;
        Ok(format_digest(digest))
    }

    pub fn update_incremental_graph_digest_for_file(
        &self,
        file_id: &str,
        updated_at_unix_ms: Option<u64>,
    ) -> StoreResult<String> {
        update_incremental_graph_digest_for_file(
            &self.connection,
            &normalize_repo_relative_path(file_id),
            updated_at_unix_ms,
        )
    }

    pub fn replace_repo_graph_digest(
        &self,
        digest: &str,
        updated_at_unix_ms: Option<u64>,
    ) -> StoreResult<()> {
        self.connection.execute(
            "
            INSERT INTO repo_graph_digest (id, digest, updated_at_unix_ms)
            VALUES ('current', ?1, ?2)
            ON CONFLICT(id) DO UPDATE SET
                digest = excluded.digest,
                updated_at_unix_ms = excluded.updated_at_unix_ms
            ",
            params![digest, updated_at_unix_ms],
        )?;
        Ok(())
    }

    pub fn rebuild_file_graph_digest(
        &self,
        file_id: &str,
        updated_at_unix_ms: Option<u64>,
    ) -> StoreResult<Option<String>> {
        let normalized = normalize_repo_relative_path(file_id);
        let digest = current_file_graph_digest_u64(&self.connection, &normalized)?;
        if digest == 0 {
            self.connection.execute(
                "DELETE FROM file_graph_digests WHERE file_id = ?1",
                [normalized],
            )?;
            Ok(None)
        } else {
            let formatted = format_digest(digest);
            self.connection.execute(
                "
                INSERT INTO file_graph_digests (file_id, digest, updated_at_unix_ms)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(file_id) DO UPDATE SET
                    digest = excluded.digest,
                    updated_at_unix_ms = excluded.updated_at_unix_ms
                ",
                params![normalized, formatted, updated_at_unix_ms],
            )?;
            Ok(Some(formatted))
        }
    }

    pub fn incremental_graph_digest(&self) -> StoreResult<Option<String>> {
        self.connection
            .query_row(
                "SELECT digest FROM repo_graph_digest WHERE id = 'current'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn relation_counts(&self) -> StoreResult<BTreeMap<String, u64>> {
        let mut statement = self.connection.prepare(
            "
            SELECT relation_kind_dict.value, COUNT(*) AS count
            FROM (
                SELECT relation_id FROM edges
                UNION ALL
                SELECT relation_id FROM structural_relations
                UNION ALL
                SELECT relation_id FROM callsites
                UNION ALL
                SELECT relation_id FROM callsite_args
            ) edge_facts
            JOIN relation_kind_dict ON relation_kind_dict.id = edge_facts.relation_id
            GROUP BY relation_kind_dict.value
            ORDER BY relation_kind_dict.value
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?;

        let mut counts = BTreeMap::new();
        for row in rows {
            let (relation, count) = row?;
            counts.insert(relation, count);
        }
        for relation in [
            RelationKind::Contains,
            RelationKind::DefinedIn,
            RelationKind::Declares,
        ] {
            let count = entity_attribute_relation_count(&self.connection, relation)?;
            if count > 0 {
                *counts.entry(relation.to_string()).or_insert(0) += count;
            }
        }
        Ok(counts)
    }

    fn configure(&self) -> StoreResult<()> {
        self.connection.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = FULL;
            PRAGMA busy_timeout = 5000;
            ",
        )?;
        register_sqlite_functions(&self.connection)?;
        Ok(())
    }

    pub fn sqlite_pragmas(&self) -> StoreResult<BTreeMap<String, String>> {
        let foreign_keys: u32 = self
            .connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
        let journal_mode: String = self
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        let synchronous: u32 = self
            .connection
            .query_row("PRAGMA synchronous", [], |row| row.get(0))?;
        let busy_timeout_ms: u32 = self
            .connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;

        Ok(BTreeMap::from([
            ("foreign_keys".to_string(), foreign_keys.to_string()),
            ("journal_mode".to_string(), journal_mode),
            ("synchronous".to_string(), synchronous.to_string()),
            ("busy_timeout_ms".to_string(), busy_timeout_ms.to_string()),
        ]))
    }

    pub fn upsert_content_template_extraction(
        &self,
        file: &FileRecord,
        entities: &[Entity],
        edges: &[Edge],
    ) -> StoreResult<()> {
        let mut dictionary_cache = self.dictionary_cache.borrow_mut();
        upsert_content_template_extraction(
            &self.connection,
            &mut dictionary_cache,
            file,
            entities,
            edges,
        )
    }

    pub fn content_template_entity_count_for_file(
        &self,
        repo_relative_path: &str,
    ) -> StoreResult<u64> {
        let Some(instance) = template_instance_for_file(&self.connection, repo_relative_path)?
        else {
            return Ok(0);
        };
        count_template_entities(&self.connection, instance.content_template_id)
    }

    pub fn physical_entity_exists(&self, id: &str) -> StoreResult<bool> {
        let Some(id_key) = lookup_object_id(&self.connection, id)? else {
            return Ok(false);
        };
        entity_row_exists(&self.connection, id_key)
    }

    pub fn stored_edge_exists(&self, id: &str) -> StoreResult<bool> {
        edge_lookup_key(&self.connection, id).map(|key| key.is_some())
    }
}

impl GraphStore for SqliteGraphStore {
    fn migrate(&self) -> StoreResult<()> {
        let current_version: u32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if current_version == SCHEMA_VERSION
            && exists_in_sqlite_master(&self.connection, "table", "file_edges")?
            && exists_in_sqlite_master(&self.connection, "table", "file_fts_rows")?
            && exists_in_sqlite_master(&self.connection, "table", "repo_graph_digest")?
            && exists_in_sqlite_master(&self.connection, "table", "entity_id_history")?
            && exists_in_sqlite_master(&self.connection, "table", "structural_relations")?
            && exists_in_sqlite_master(&self.connection, "table", "callsites")?
            && exists_in_sqlite_master(&self.connection, "table", "callsite_args")?
            && exists_in_sqlite_master(&self.connection, "view", "qualified_name_lookup")?
            && view_compiles(&self.connection, "qualified_name_lookup")?
            && exists_in_sqlite_master(&self.connection, "index", "idx_object_id_dict_hash")?
            && exists_in_sqlite_master(&self.connection, "index", "idx_symbol_dict_hash")?
            && exists_in_sqlite_master(&self.connection, "index", "idx_qname_prefix_dict_hash")?
            && exists_in_sqlite_master(&self.connection, "index", "idx_qualified_name_parts")?
            && exists_in_sqlite_master(&self.connection, "index", "idx_file_edges_edge")?
            && exists_in_sqlite_master(&self.connection, "index", "idx_path_evidence_edges_edge")?
            && exists_in_sqlite_master(&self.connection, "table", "path_evidence_debug_metadata")?
            && table_has_column(&self.connection, "path_evidence_edges", "exactness")?
            && table_has_column(&self.connection, "path_evidence_edges", "confidence")?
            && table_has_column(&self.connection, "path_evidence_edges", "derived")?
            && table_has_column(&self.connection, "path_evidence_edges", "edge_class")?
            && table_has_column(&self.connection, "path_evidence_edges", "context")?
            && table_has_column(
                &self.connection,
                "path_evidence_edges",
                "provenance_edges_json",
            )?
            && exists_in_sqlite_master(&self.connection, "index", "idx_file_fts_rows_object")?
            && exists_in_sqlite_master(&self.connection, "table", "resolution_kind_dict")?
            && exists_in_sqlite_master(&self.connection, "table", "edge_provenance_dict")?
            && exists_in_sqlite_master(&self.connection, "table", "edge_debug_metadata")?
            && exists_in_sqlite_master(&self.connection, "table", "heuristic_edges")?
            && exists_in_sqlite_master(&self.connection, "table", "unresolved_references")?
            && exists_in_sqlite_master(&self.connection, "table", "static_references")?
            && exists_in_sqlite_master(&self.connection, "table", "extraction_warnings")?
            && exists_in_sqlite_master(&self.connection, "table", "source_content_template")?
            && exists_in_sqlite_master(&self.connection, "table", "template_entities")?
            && exists_in_sqlite_master(&self.connection, "table", "template_edges")?
            && exists_in_sqlite_master(&self.connection, "view", "file_instance")?
            && view_compiles(&self.connection, "file_instance")?
            && exists_in_sqlite_master(&self.connection, "view", "edges_compat")?
            && view_compiles(&self.connection, "edges_compat")?
            && exists_in_sqlite_master(&self.connection, "index", "idx_heuristic_edges_relation")?
            && exists_in_sqlite_master(&self.connection, "index", "idx_unresolved_references_path")?
            && !exists_in_sqlite_master(
                &self.connection,
                "index",
                "idx_template_edges_head_relation",
            )?
            && !exists_in_sqlite_master(
                &self.connection,
                "index",
                "idx_template_edges_tail_relation",
            )?
            && !exists_in_sqlite_master(&self.connection, "index", "idx_files_content_template")?
            && exists_in_sqlite_master(&self.connection, "view", "object_id_lookup")?
            && view_compiles(&self.connection, "object_id_lookup")?
            && table_has_column(&self.connection, "files", "file_id")?
            && table_has_column(&self.connection, "files", "content_hash")?
            && table_has_column(&self.connection, "entities", "entity_hash")?
            && table_has_column(&self.connection, "entities", "structural_flags")?
            && table_has_column(&self.connection, "edges", "resolution_kind_id")?
            && table_has_column(&self.connection, "edges", "context_kind_id")?
            && table_has_column(&self.connection, "edges", "flags_bitset")?
            && table_has_column(&self.connection, "edges", "confidence_q")?
            && table_has_column(&self.connection, "edges", "provenance_id")?
            && !table_has_column(&self.connection, "edges", "metadata_json")?
            && !table_has_column(&self.connection, "entities", "file_hash")?
            && !table_has_column(&self.connection, "edges", "file_hash")?
            && !table_has_column(&self.connection, "structural_relations", "file_hash")?
            && !table_has_column(&self.connection, "callsites", "file_hash")?
            && !table_has_column(&self.connection, "callsite_args", "file_hash")?
        {
            return Ok(());
        }
        self.connection.execute_batch(
            "
            DROP VIEW IF EXISTS file_instance;
            DROP VIEW IF EXISTS edges_compat;
            ",
        )?;
        migrate_legacy_text_tables(&self.connection)?;
        migrate_dictionary_compaction(&self.connection)?;
        ensure_entity_hash_column(&self.connection)?;
        self.connection.execute_batch(
            "
            DROP VIEW IF EXISTS file_instance;
            DROP VIEW IF EXISTS edges_compat;
            ",
        )?;
        self.connection.execute_batch(SCHEMA_SQL)?;
        migrate_edge_classification_columns(&self.connection)?;
        migrate_structural_compaction_columns(&self.connection)?;
        migrate_file_hash_normalization(&self.connection)?;
        migrate_edge_metadata_compaction(&self.connection)?;
        migrate_object_id_compaction(&self.connection)?;
        migrate_heuristic_debug_sidecar(&self.connection)?;
        migrate_structural_relation_attributes(&self.connection)?;
        migrate_content_template_overlay(&self.connection)?;
        migrate_template_local_id_compaction(&self.connection)?;
        migrate_template_source_identity_compaction(&self.connection)?;
        migrate_path_evidence_metadata_compaction(&self.connection)?;
        refresh_file_instance_view(&self.connection)?;
        self.connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    fn schema_version(&self) -> StoreResult<u32> {
        let version: u32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        Ok(version)
    }

    fn upsert_entity(&self, entity: &Entity) -> StoreResult<()> {
        self.write_entity(entity, true)
    }

    fn get_entity(&self, id: &str) -> StoreResult<Option<Entity>> {
        let Some(id_key) = lookup_object_id(&self.connection, id)? else {
            return find_synthesized_template_entity(&self.connection, id);
        };
        let physical = self
            .connection
            .query_row(ENTITY_SELECT_BY_ID, [id_key], entity_from_row)
            .optional()
            .map_err(StoreError::from)?;
        if physical.is_some() {
            return Ok(physical);
        }
        find_synthesized_template_entity(&self.connection, id)
    }

    fn delete_entity(&self, id: &str) -> StoreResult<bool> {
        delete_fts_row(&self.connection, TextSearchKind::Entity, id)?;
        self.connection
            .execute("DELETE FROM file_entities WHERE entity_id = ?1", [id])?;
        let Some(id_key) = lookup_object_id(&self.connection, id)? else {
            return Ok(false);
        };
        let changed = self
            .connection
            .execute("DELETE FROM entities WHERE id_key = ?1", [id_key])?;
        Ok(changed > 0)
    }

    fn list_entities_by_file(&self, repo_relative_path: &str) -> StoreResult<Vec<Entity>> {
        let Some(path_id) = lookup_path(&self.connection, repo_relative_path)? else {
            return Ok(Vec::new());
        };
        let mut statement = self.connection.prepare(ENTITY_SELECT_BY_FILE)?;
        let rows = statement.query_map([path_id], entity_from_row)?;
        let mut entities = collect_rows(rows)?;
        if let Some(instance) = template_instance_for_file(&self.connection, repo_relative_path)? {
            entities.extend(template_entities_for_instance(&self.connection, &instance)?);
            dedupe_entities(&mut entities);
        }
        Ok(entities)
    }

    fn list_entities(&self, limit: usize) -> StoreResult<Vec<Entity>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut statement = self.connection.prepare(ENTITY_SELECT_LIST)?;
        let rows = statement.query_map([limit as i64], entity_from_row)?;
        let mut entities = collect_rows(rows)?;
        if entities.len() < limit {
            let remaining = limit - entities.len();
            entities.extend(synthesized_template_entities(
                &self.connection,
                Some(remaining),
            )?);
            dedupe_entities(&mut entities);
            entities.truncate(limit);
        }
        Ok(entities)
    }

    fn count_entities(&self) -> StoreResult<u64> {
        Ok(count_rows(&self.connection, "entities")?
            + duplicate_template_entity_count(&self.connection)?)
    }

    fn upsert_edge(&self, edge: &Edge) -> StoreResult<()> {
        self.write_edge(edge, EdgeIdStorage::ExistingOrCompact)
    }

    fn get_edge(&self, id: &str) -> StoreResult<Option<Edge>> {
        let Some(id_key) = edge_lookup_key(&self.connection, id)? else {
            return find_synthesized_template_edge(&self.connection, id);
        };
        let physical = self
            .connection
            .query_row(EDGE_SELECT_BY_ID, [id_key], edge_from_row)
            .optional()
            .map_err(StoreError::from)?;
        if physical.is_some() {
            return Ok(physical);
        }
        find_synthesized_template_edge(&self.connection, id)
    }

    fn delete_edge(&self, id: &str) -> StoreResult<bool> {
        let mut changed = 0usize;
        self.connection
            .execute("DELETE FROM file_edges WHERE edge_id = ?1", [id])?;
        self.connection
            .execute("DELETE FROM file_source_spans WHERE span_id = ?1", [id])?;
        if let Some(id_key) = edge_lookup_key(&self.connection, id)? {
            changed += delete_edge_fact_by_key(&self.connection, id_key)?;
        }
        let compact = compact_edge_key(id);
        changed += delete_edge_fact_by_key(&self.connection, compact)?;
        Ok(changed > 0)
    }

    fn list_edges(&self, limit: usize) -> StoreResult<Vec<Edge>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut statement = self.connection.prepare(EDGE_SELECT_LIST)?;
        let rows = statement.query_map([limit as i64], edge_from_row)?;
        let mut edges = collect_rows(rows)?;
        if edges.len() < limit {
            let remaining = limit - edges.len();
            edges.extend(synthesized_template_edges(
                &self.connection,
                Some(remaining),
            )?);
            dedupe_edges(&mut edges);
            edges.truncate(limit);
        }
        if edges.len() < limit {
            edges.extend(query_entity_attribute_structural_edges(
                &self.connection,
                RelationKind::Contains,
                EntityStructuralFilter::Any,
                limit - edges.len(),
            )?);
        }
        if edges.len() < limit {
            edges.extend(query_entity_attribute_structural_edges(
                &self.connection,
                RelationKind::DefinedIn,
                EntityStructuralFilter::Any,
                limit - edges.len(),
            )?);
        }
        if edges.len() < limit {
            edges.extend(query_entity_attribute_structural_edges(
                &self.connection,
                RelationKind::Declares,
                EntityStructuralFilter::Any,
                limit - edges.len(),
            )?);
        }
        Ok(edges)
    }

    fn count_edges(&self) -> StoreResult<u64> {
        Ok(count_rows(&self.connection, "edges")?
            + compact_fact_row_count(&self.connection)?
            + duplicate_template_edge_count(&self.connection)?
            + entity_attribute_relation_count(&self.connection, RelationKind::Contains)?
            + entity_attribute_relation_count(&self.connection, RelationKind::DefinedIn)?
            + entity_attribute_relation_count(&self.connection, RelationKind::Declares)?)
    }

    fn find_edges_by_head_relation(
        &self,
        head_id: &str,
        relation: RelationKind,
    ) -> StoreResult<Vec<Edge>> {
        let Some(head_id_key) = lookup_object_id(&self.connection, head_id)? else {
            return synthesized_template_edges_by_relation(
                &self.connection,
                Some(head_id),
                None,
                relation,
            );
        };
        if is_entity_attribute_structural_relation(relation) {
            let mut edges = query_entity_attribute_structural_edges(
                &self.connection,
                relation,
                EntityStructuralFilter::Head(head_id_key),
                DEFAULT_STRUCTURAL_QUERY_LIMIT,
            )?;
            edges.extend(synthesized_template_edges_by_relation(
                &self.connection,
                Some(head_id),
                None,
                relation,
            )?);
            dedupe_edges(&mut edges);
            return Ok(edges);
        }
        let Some(relation_id) = lookup_relation_kind(&self.connection, &relation.to_string())?
        else {
            return synthesized_template_edges_by_relation(
                &self.connection,
                Some(head_id),
                None,
                relation,
            );
        };
        query_edges(
            &self.connection,
            EDGE_SELECT_BY_HEAD_RELATION,
            params![head_id_key, relation_id],
        )
        .and_then(|mut edges| {
            edges.extend(synthesized_template_edges_by_relation(
                &self.connection,
                Some(head_id),
                None,
                relation,
            )?);
            dedupe_edges(&mut edges);
            Ok(edges)
        })
    }

    fn find_edges_by_tail_relation(
        &self,
        tail_id: &str,
        relation: RelationKind,
    ) -> StoreResult<Vec<Edge>> {
        let Some(tail_id_key) = lookup_object_id(&self.connection, tail_id)? else {
            return synthesized_template_edges_by_relation(
                &self.connection,
                None,
                Some(tail_id),
                relation,
            );
        };
        if is_entity_attribute_structural_relation(relation) {
            let mut edges = query_entity_attribute_structural_edges(
                &self.connection,
                relation,
                EntityStructuralFilter::Tail(tail_id_key),
                DEFAULT_STRUCTURAL_QUERY_LIMIT,
            )?;
            edges.extend(synthesized_template_edges_by_relation(
                &self.connection,
                None,
                Some(tail_id),
                relation,
            )?);
            dedupe_edges(&mut edges);
            return Ok(edges);
        }
        let Some(relation_id) = lookup_relation_kind(&self.connection, &relation.to_string())?
        else {
            return synthesized_template_edges_by_relation(
                &self.connection,
                None,
                Some(tail_id),
                relation,
            );
        };
        query_edges(
            &self.connection,
            EDGE_SELECT_BY_TAIL_RELATION,
            params![tail_id_key, relation_id],
        )
        .and_then(|mut edges| {
            edges.extend(synthesized_template_edges_by_relation(
                &self.connection,
                None,
                Some(tail_id),
                relation,
            )?);
            dedupe_edges(&mut edges);
            Ok(edges)
        })
    }

    fn upsert_file(&self, file: &FileRecord) -> StoreResult<()> {
        let mut dictionary_cache = self.dictionary_cache.borrow_mut();
        let path_id = intern_path_cached(
            &self.connection,
            &mut dictionary_cache,
            &file.repo_relative_path,
        )?;
        let language_id = match &file.language {
            Some(language) => Some(intern_language_cached(
                &self.connection,
                &mut dictionary_cache,
                language,
            )?),
            None => None,
        };
        let content_template_id = ensure_content_template_for_path_id_cached(
            &self.connection,
            &mut dictionary_cache,
            &file.file_hash,
            language_id,
            path_id,
        )?;
        let sql_start = Instant::now();
        self.connection.execute(
            "
            INSERT INTO files (
                file_id, path_id, content_hash, mtime_unix_ms, size_bytes,
                language_id, indexed_at_unix_ms, content_template_id, metadata_json
            ) VALUES (?1, ?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(path_id) DO UPDATE SET
                content_hash = excluded.content_hash,
                mtime_unix_ms = excluded.mtime_unix_ms,
                language_id = excluded.language_id,
                size_bytes = excluded.size_bytes,
                indexed_at_unix_ms = excluded.indexed_at_unix_ms,
                content_template_id = excluded.content_template_id,
                metadata_json = excluded.metadata_json
            ",
            params![
                path_id,
                file.file_hash,
                file.size_bytes,
                language_id,
                file.indexed_at_unix_ms,
                content_template_id,
                to_json(&file.metadata)?,
            ],
        )?;
        dictionary_cache.file_ids.insert(
            file.repo_relative_path.clone(),
            CachedFileIds {
                path_id,
                file_id: path_id,
                has_content_hash: !file.file_hash.is_empty(),
            },
        );
        record_sqlite_profile("file_manifest_upsert_sql", sql_start.elapsed());
        Ok(())
    }

    fn get_file(&self, repo_relative_path: &str) -> StoreResult<Option<FileRecord>> {
        let Some(path_id) = lookup_path(&self.connection, repo_relative_path)? else {
            return Ok(None);
        };
        self.connection
            .query_row(FILE_SELECT_BY_PATH_ID, [path_id], file_from_row)
            .optional()
            .map_err(StoreError::from)
    }

    fn delete_file(&self, repo_relative_path: &str) -> StoreResult<bool> {
        delete_fts_row(&self.connection, TextSearchKind::File, repo_relative_path)?;
        let Some(path_id) = lookup_path(&self.connection, repo_relative_path)? else {
            return Ok(false);
        };
        let changed = self
            .connection
            .execute("DELETE FROM files WHERE path_id = ?1", [path_id])?;
        Ok(changed > 0)
    }

    fn delete_facts_for_file(&self, repo_relative_path: &str) -> StoreResult<()> {
        let repo_relative_path = normalize_repo_relative_path(repo_relative_path);
        self.clear_write_caches();
        delete_fts_rows_for_file(&self.connection, &repo_relative_path)?;
        delete_sidecar_facts_for_file(&self.connection, &repo_relative_path)?;
        let Some(path_id) = lookup_path(&self.connection, &repo_relative_path)? else {
            return Ok(());
        };
        let mapped_entity_ids = entity_ids_for_file_map(&self.connection, &repo_relative_path)?;
        let mapped_edge_ids = edge_ids_for_file_map(&self.connection, &repo_relative_path)?;
        let mapped_span_ids = source_span_ids_for_file_map(&self.connection, &repo_relative_path)?;
        let use_reverse_maps = !mapped_entity_ids.is_empty()
            || !mapped_edge_ids.is_empty()
            || !mapped_span_ids.is_empty();
        let stale_entity_ids = if use_reverse_maps {
            mapped_entity_ids
        } else {
            entity_ids_for_path(&self.connection, path_id)?
        };
        let stale_edge_ids = if use_reverse_maps {
            mapped_edge_ids
        } else {
            edge_ids_touching_path(&self.connection, path_id)?
        };
        preserve_entity_ids_for_path(&self.connection, path_id)?;

        delete_cached_path_evidence_for_file(
            &self.connection,
            &repo_relative_path,
            &stale_entity_ids,
            &stale_edge_ids,
            use_reverse_maps,
        )?;
        delete_cached_derived_edges_for_file(&self.connection, &stale_entity_ids, &stale_edge_ids)?;

        if use_reverse_maps {
            delete_edges_by_logical_ids(&self.connection, &stale_edge_ids)?;
            delete_source_spans_by_logical_ids(&self.connection, &mapped_span_ids)?;
            delete_entities_by_logical_ids(&self.connection, &stale_entity_ids)?;
        } else {
            delete_edges_by_logical_ids(&self.connection, &stale_edge_ids)?;
            let mut delete_edges = self.connection.prepare_cached(
                "DELETE FROM edges
                 WHERE head_id_key IN (
                     SELECT id_key FROM entities WHERE path_id = ?1
                 )
                    OR tail_id_key IN (
                     SELECT id_key FROM entities WHERE path_id = ?1
                 )
                    OR span_path_id = ?1",
            )?;
            delete_edges.execute([path_id])?;
            delete_compact_facts_for_path(&self.connection, path_id)?;

            let mut delete_source_spans = self
                .connection
                .prepare_cached("DELETE FROM source_spans WHERE path_id = ?1")?;
            delete_source_spans.execute([path_id])?;
            let mut delete_entities = self
                .connection
                .prepare_cached("DELETE FROM entities WHERE path_id = ?1")?;
            delete_entities.execute([path_id])?;
        }
        let mut delete_files = self
            .connection
            .prepare_cached("DELETE FROM files WHERE path_id = ?1")?;
        delete_files.execute([path_id])?;

        cleanup_file_reverse_maps(
            &self.connection,
            &repo_relative_path,
            &stale_entity_ids,
            &stale_edge_ids,
        )?;

        Ok(())
    }

    fn list_files(&self, limit: usize) -> StoreResult<Vec<FileRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut statement = self.connection.prepare(FILE_SELECT_LIST)?;
        let rows = statement.query_map([limit as i64], file_from_row)?;
        collect_rows(rows)
    }

    fn count_files(&self) -> StoreResult<u64> {
        count_rows(&self.connection, "files")
    }

    fn upsert_file_text(&self, repo_relative_path: &str, text: &str) -> StoreResult<()> {
        upsert_fts_row(
            &self.connection,
            TextSearchKind::File,
            repo_relative_path,
            repo_relative_path,
            None,
            repo_relative_path,
            text,
        )
    }

    fn upsert_source_span(&self, id: &str, span: &SourceSpan) -> StoreResult<()> {
        let id_key = intern_object_id(&self.connection, id)?;
        let path_id = intern_path(&self.connection, &span.repo_relative_path)?;
        let mut statement = self.connection.prepare_cached(
            "
            INSERT INTO source_spans (
                id_key, path_id, start_line, start_column,
                end_line, end_column
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id_key) DO UPDATE SET
                path_id = excluded.path_id,
                start_line = excluded.start_line,
                start_column = excluded.start_column,
                end_line = excluded.end_line,
                end_column = excluded.end_column
            ",
        )?;
        statement.execute(params![
            id_key,
            path_id,
            span.start_line,
            span.start_column,
            span.end_line,
            span.end_column,
        ])?;
        map_source_span_to_file(&self.connection, &span.repo_relative_path, id)?;
        Ok(())
    }

    fn get_source_span(&self, id: &str) -> StoreResult<Option<SourceSpan>> {
        let Some(id_key) = lookup_object_id(&self.connection, id)? else {
            return Ok(None);
        };
        self.connection
            .query_row(SOURCE_SPAN_SELECT_BY_ID, [id_key], source_span_from_row)
            .optional()
            .map_err(StoreError::from)
    }

    fn delete_source_span(&self, id: &str) -> StoreResult<bool> {
        delete_fts_row(&self.connection, TextSearchKind::Snippet, id)?;
        self.connection
            .execute("DELETE FROM file_source_spans WHERE span_id = ?1", [id])?;
        let Some(id_key) = lookup_object_id(&self.connection, id)? else {
            return Ok(false);
        };
        let changed = self
            .connection
            .execute("DELETE FROM source_spans WHERE id_key = ?1", [id_key])?;
        Ok(changed > 0)
    }

    fn upsert_snippet_text(&self, id: &str, span: &SourceSpan, text: &str) -> StoreResult<()> {
        upsert_fts_row(
            &self.connection,
            TextSearchKind::Snippet,
            id,
            &span.repo_relative_path,
            Some(span.start_line),
            &span.to_string(),
            text,
        )
    }

    fn upsert_repo_index_state(&self, state: &RepoIndexState) -> StoreResult<()> {
        let sql_start = Instant::now();
        self.connection.execute(
            "
            INSERT INTO repo_index_state (
                repo_id, repo_root, repo_commit, schema_version,
                indexed_at_unix_ms, files_indexed, entity_count,
                edge_count, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(repo_id) DO UPDATE SET
                repo_root = excluded.repo_root,
                repo_commit = excluded.repo_commit,
                schema_version = excluded.schema_version,
                indexed_at_unix_ms = excluded.indexed_at_unix_ms,
                files_indexed = excluded.files_indexed,
                entity_count = excluded.entity_count,
                edge_count = excluded.edge_count,
                metadata_json = excluded.metadata_json
            ",
            params![
                state.repo_id,
                state.repo_root,
                state.repo_commit,
                state.schema_version,
                state.indexed_at_unix_ms,
                state.files_indexed,
                state.entity_count,
                state.edge_count,
                to_json(&state.metadata)?,
            ],
        )?;
        record_sqlite_profile("repo_index_state_upsert_sql", sql_start.elapsed());
        Ok(())
    }

    fn get_repo_index_state(&self, repo_id: &str) -> StoreResult<Option<RepoIndexState>> {
        self.connection
            .query_row(
                "SELECT * FROM repo_index_state WHERE repo_id = ?1",
                [repo_id],
                repo_index_state_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn delete_repo_index_state(&self, repo_id: &str) -> StoreResult<bool> {
        let changed = self
            .connection
            .execute("DELETE FROM repo_index_state WHERE repo_id = ?1", [repo_id])?;
        Ok(changed > 0)
    }

    fn upsert_path_evidence(&self, path: &PathEvidence) -> StoreResult<()> {
        let sql_start = Instant::now();
        let compact_metadata = compact_path_evidence_metadata(path);
        self.connection.execute(
            "
            INSERT INTO path_evidence (
                id, source, target, summary, metapath_json, edges_json,
                source_spans_json, exactness, length, confidence, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(id) DO UPDATE SET
                source = excluded.source,
                target = excluded.target,
                summary = excluded.summary,
                metapath_json = excluded.metapath_json,
                edges_json = excluded.edges_json,
                source_spans_json = excluded.source_spans_json,
                exactness = excluded.exactness,
                length = excluded.length,
                confidence = excluded.confidence,
                metadata_json = excluded.metadata_json
            ",
            params![
                path.id,
                path.source,
                path.target,
                path.summary,
                to_json(&path.metapath)?,
                to_json(&path.edges)?,
                to_json(&path.source_spans)?,
                path.exactness.to_string(),
                path.length,
                path.confidence,
                to_json(&compact_metadata)?,
            ],
        )?;
        upsert_path_evidence_materialized_rows(&self.connection, path)?;
        self.connection.execute(
            "DELETE FROM path_evidence_debug_metadata WHERE path_id = ?1",
            [&path.id],
        )?;
        if path
            .metadata
            .get("persist_debug_metadata")
            .and_then(Value::as_bool)
            == Some(true)
        {
            self.connection.execute(
                "
                INSERT OR REPLACE INTO path_evidence_debug_metadata (path_id, metadata_json)
                VALUES (?1, ?2)
                ",
                params![path.id, to_json(&path.metadata)?],
            )?;
        }
        record_sqlite_profile("path_evidence_upsert_sql", sql_start.elapsed());
        Ok(())
    }

    fn get_path_evidence(&self, id: &str) -> StoreResult<Option<PathEvidence>> {
        self.connection
            .query_row(
                "SELECT * FROM path_evidence WHERE id = ?1",
                [id],
                path_evidence_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn delete_path_evidence(&self, id: &str) -> StoreResult<bool> {
        let changed = delete_by_id(&self.connection, "path_evidence", id)?;
        delete_path_evidence_materialized_rows(&self.connection, id)?;
        Ok(changed)
    }

    fn count_path_evidence(&self) -> StoreResult<u64> {
        count_rows(&self.connection, "path_evidence")
    }

    fn upsert_derived_edge(&self, edge: &DerivedClosureEdge) -> StoreResult<()> {
        validate_derived_closure_edge(edge)?;
        self.connection.execute(
            "
            INSERT INTO derived_edges (
                id, head_id, relation, tail_id, provenance_edges_json,
                exactness, confidence, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                head_id = excluded.head_id,
                relation = excluded.relation,
                tail_id = excluded.tail_id,
                provenance_edges_json = excluded.provenance_edges_json,
                exactness = excluded.exactness,
                confidence = excluded.confidence,
                metadata_json = excluded.metadata_json
            ",
            params![
                edge.id,
                edge.head_id,
                edge.relation.to_string(),
                edge.tail_id,
                to_json(&edge.provenance_edges)?,
                edge.exactness.to_string(),
                edge.confidence,
                to_json(&edge.metadata)?,
            ],
        )?;
        Ok(())
    }

    fn get_derived_edge(&self, id: &str) -> StoreResult<Option<DerivedClosureEdge>> {
        self.connection
            .query_row(
                "SELECT * FROM derived_edges WHERE id = ?1",
                [id],
                derived_edge_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn delete_derived_edge(&self, id: &str) -> StoreResult<bool> {
        delete_by_id(&self.connection, "derived_edges", id)
    }

    fn find_entities_by_exact_symbol(&self, symbol: &str) -> StoreResult<Vec<Entity>> {
        let object_id = lookup_object_id(&self.connection, symbol)?;
        let name_id = lookup_symbol(&self.connection, symbol)?;
        let qualified_name_id = lookup_qualified_name(&self.connection, symbol)?;
        let mut statement = self.connection.prepare(ENTITY_SELECT_BY_EXACT_SYMBOL)?;
        let rows = statement.query_map(
            params![object_id, name_id, qualified_name_id],
            entity_from_row,
        )?;
        let mut entities = collect_rows(rows)?;
        entities.extend(
            synthesized_template_entities(&self.connection, None)?
                .into_iter()
                .filter(|entity| {
                    entity.id == symbol || entity.name == symbol || entity.qualified_name == symbol
                }),
        );
        dedupe_entities(&mut entities);
        Ok(entities)
    }

    fn search_text(&self, query: &str, limit: usize) -> StoreResult<Vec<TextSearchHit>> {
        let query = fts_query(query);
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let mut statement = self.connection.prepare(
            "
            SELECT kind, id, repo_relative_path, line, title, body,
                   bm25(stage0_fts) AS rank
            FROM stage0_fts
            WHERE stage0_fts MATCH ?1
            ORDER BY rank, kind, id
            LIMIT ?2
            ",
        )?;
        let rows = statement.query_map(params![query, limit as i64], text_search_hit_from_row)?;
        collect_rows(rows)
    }

    fn upsert_retrieval_trace(&self, trace: &RetrievalTraceRecord) -> StoreResult<()> {
        self.connection.execute(
            "
            INSERT INTO retrieval_traces (
                id, task, trace_json, created_at_unix_ms
            ) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                task = excluded.task,
                trace_json = excluded.trace_json,
                created_at_unix_ms = excluded.created_at_unix_ms
            ",
            params![
                &trace.id,
                trace.task.as_deref(),
                to_json(&trace.trace_json)?,
                trace.created_at_unix_ms,
            ],
        )?;
        Ok(())
    }

    fn get_retrieval_trace(&self, id: &str) -> StoreResult<Option<RetrievalTraceRecord>> {
        self.connection
            .query_row(
                "SELECT * FROM retrieval_traces WHERE id = ?1",
                [id],
                retrieval_trace_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn delete_retrieval_trace(&self, id: &str) -> StoreResult<bool> {
        delete_by_id(&self.connection, "retrieval_traces", id)
    }
}

fn exists_in_sqlite_master(
    connection: &Connection,
    object_type: &str,
    object_name: &str,
) -> StoreResult<bool> {
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2)",
        params![object_type, object_name],
        |row| row.get::<_, bool>(0),
    )?;
    Ok(exists)
}

pub fn inspect_db_preflight(
    db_path: &Path,
    expected_schema_version: u32,
    expected: &ExpectedDbPassport,
) -> DbPreflightReport {
    let orphan_sidecars = existing_sqlite_sidecars(db_path);
    if !db_path.exists() {
        let mut reasons = vec![format!("main DB does not exist: {}", db_path.display())];
        if !orphan_sidecars.is_empty() {
            reasons.push("sidecar WAL/SHM files exist without a main DB".to_string());
        }
        return DbPreflightReport::missing(db_path, reasons, orphan_sidecars);
    }

    let mut reasons = Vec::new();
    let connection = match Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(connection) => connection,
        Err(error) => {
            let message = error.to_string();
            let passport_status = if message.to_ascii_lowercase().contains("locked") {
                "locked"
            } else if message.to_ascii_lowercase().contains("malformed")
                || message.to_ascii_lowercase().contains("not a database")
                || message
                    .to_ascii_lowercase()
                    .contains("file is not a database")
            {
                "corrupt"
            } else {
                "unknown"
            };
            return DbPreflightReport {
                db_path: db_path.display().to_string(),
                passport_status: passport_status.to_string(),
                valid: false,
                reasons: vec![format!("read-only SQLite open failed: {error}")],
                schema_version: None,
                passport: None,
                orphan_sidecars: orphan_sidecars
                    .into_iter()
                    .map(|path| path.display().to_string())
                    .collect(),
            };
        }
    };

    let mut passport_status = "valid".to_string();
    if let Err(error) = validate_sqlite_check_rows(&connection, "quick_check") {
        passport_status = "corrupt".to_string();
        reasons.push(error.to_string());
    }
    if let Err(error) = validate_foreign_key_check(&connection) {
        passport_status = "corrupt".to_string();
        reasons.push(error.to_string());
    }

    let schema_version = match connection.query_row("PRAGMA user_version", [], |row| row.get(0)) {
        Ok(version) => Some(version),
        Err(error) => {
            passport_status = "unknown".to_string();
            reasons.push(format!("schema version read failed: {error}"));
            None
        }
    };
    if schema_version != Some(expected_schema_version) {
        passport_status = "mismatched".to_string();
        reasons.push(format!(
            "schema version mismatch: expected {expected_schema_version}, observed {}",
            schema_version
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
    }

    let passport_table =
        exists_in_sqlite_master(&connection, "table", "codegraph_db_passport").unwrap_or(false);
    if !passport_table {
        if passport_status == "valid" {
            passport_status = "missing".to_string();
        }
        reasons.push("codegraph_db_passport table is missing".to_string());
        return DbPreflightReport {
            db_path: db_path.display().to_string(),
            passport_status,
            valid: false,
            reasons,
            schema_version,
            passport: None,
            orphan_sidecars: orphan_sidecars
                .into_iter()
                .map(|path| path.display().to_string())
                .collect(),
        };
    }

    let passport = match read_db_passport(&connection) {
        Ok(Some(passport)) => passport,
        Ok(None) => {
            if passport_status == "valid" {
                passport_status = "missing".to_string();
            }
            reasons.push("codegraph_db_passport row is missing".to_string());
            return DbPreflightReport {
                db_path: db_path.display().to_string(),
                passport_status,
                valid: false,
                reasons,
                schema_version,
                passport: None,
                orphan_sidecars: orphan_sidecars
                    .into_iter()
                    .map(|path| path.display().to_string())
                    .collect(),
            };
        }
        Err(error) => {
            passport_status = "corrupt".to_string();
            reasons.push(format!("passport read failed: {error}"));
            return DbPreflightReport {
                db_path: db_path.display().to_string(),
                passport_status,
                valid: false,
                reasons,
                schema_version,
                passport: None,
                orphan_sidecars: orphan_sidecars
                    .into_iter()
                    .map(|path| path.display().to_string())
                    .collect(),
            };
        }
    };

    if passport.passport_version != DB_PASSPORT_VERSION {
        passport_status = "mismatched".to_string();
        reasons.push(format!(
            "passport version mismatch: expected {}, observed {}",
            DB_PASSPORT_VERSION, passport.passport_version
        ));
    }
    if passport.codegraph_schema_version != expected_schema_version {
        passport_status = "mismatched".to_string();
        reasons.push(format!(
            "passport schema mismatch: expected {}, observed {}",
            expected_schema_version, passport.codegraph_schema_version
        ));
    }
    if passport.canonical_repo_root != expected.canonical_repo_root {
        passport_status = "mismatched".to_string();
        reasons.push(format!(
            "repo root mismatch: expected {}, observed {}",
            expected.canonical_repo_root, passport.canonical_repo_root
        ));
    }
    if passport.storage_mode != expected.storage_mode {
        passport_status = "mismatched".to_string();
        reasons.push(format!(
            "storage mode mismatch: expected {}, observed {}",
            expected.storage_mode, passport.storage_mode
        ));
    }
    if passport.index_scope_policy_hash != expected.index_scope_policy_hash {
        passport_status = "mismatched".to_string();
        reasons.push(format!(
            "index scope policy hash mismatch: expected {}, observed {}",
            expected.index_scope_policy_hash, passport.index_scope_policy_hash
        ));
    }
    if passport.last_run_status != "completed" {
        passport_status = "mismatched".to_string();
        reasons.push(format!(
            "previous run did not complete: {}",
            passport.last_run_status
        ));
    }
    if passport.integrity_gate_result != "ok" {
        passport_status = "mismatched".to_string();
        reasons.push(format!(
            "previous integrity gate was not ok: {}",
            passport.integrity_gate_result
        ));
    }
    if expected.git_remote.is_some()
        && passport.git_remote.is_some()
        && passport.git_remote != expected.git_remote
    {
        passport_status = "mismatched".to_string();
        reasons.push(format!(
            "git remote mismatch: expected {}, observed {}",
            expected.git_remote.as_deref().unwrap_or("unknown"),
            passport.git_remote.as_deref().unwrap_or("unknown")
        ));
    }

    DbPreflightReport {
        db_path: db_path.display().to_string(),
        passport_status,
        valid: reasons.is_empty(),
        reasons,
        schema_version,
        passport: Some(passport),
        orphan_sidecars: orphan_sidecars
            .into_iter()
            .map(|path| path.display().to_string())
            .collect(),
    }
}

fn existing_sqlite_sidecars(db_path: &Path) -> Vec<PathBuf> {
    let mut sidecars = Vec::new();
    for suffix in ["-wal", "-shm"] {
        let candidate = PathBuf::from(format!("{}{}", db_path.display(), suffix));
        if candidate.exists() {
            sidecars.push(candidate);
        }
    }
    sidecars
}

fn validate_foreign_key_check(connection: &Connection) -> StoreResult<()> {
    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    let mut rows = statement.query([])?;
    let mut failures = Vec::new();
    while let Some(row) = rows.next()? {
        let table: String = row.get(0)?;
        let rowid: Option<i64> = row.get(1)?;
        let parent: String = row.get(2)?;
        let fkid: i64 = row.get(3)?;
        failures.push(format!(
            "{table} rowid={} parent={parent} fkid={fkid}",
            rowid
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_string())
        ));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(StoreError::Message(format!(
            "SQLite foreign_key_check failed: {}",
            failures.join("; ")
        )))
    }
}

fn view_compiles(connection: &Connection, view_name: &str) -> StoreResult<bool> {
    if !exists_in_sqlite_master(connection, "view", view_name)? {
        return Ok(false);
    }
    match connection.prepare(&format!("SELECT * FROM {view_name} LIMIT 0")) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

fn register_sqlite_functions(connection: &Connection) -> StoreResult<()> {
    connection.create_scalar_function(
        "codegraph_text_hash",
        1,
        FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let value: String = context.get(0)?;
            Ok(stable_text_hash_key(&value))
        },
    )?;
    connection.create_scalar_function(
        "codegraph_object_key",
        1,
        FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let value: String = context.get(0)?;
            Ok(compact_object_key(&value))
        },
    )?;
    connection.create_scalar_function(
        "codegraph_entity_hash",
        1,
        FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let value: String = context.get(0)?;
            Ok(entity_hash_blob(&value))
        },
    )?;
    connection.create_scalar_function(
        "codegraph_is_canonical_entity_id",
        1,
        FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let value: String = context.get(0)?;
            Ok(i64::from(canonical_entity_hash(&value).is_some()))
        },
    )?;
    Ok(())
}

fn migrate_legacy_text_tables(connection: &Connection) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", "entities")?
        || table_has_column(connection, "entities", "id_key")?
    {
        return Ok(());
    }

    connection.execute_batch(
        "
        DROP VIEW IF EXISTS file_instance;
        DROP VIEW IF EXISTS edges_compat;
        DROP VIEW IF EXISTS object_id_lookup;
        DROP VIEW IF EXISTS object_id_debug;
        DROP VIEW IF EXISTS qualified_name_lookup;
        DROP VIEW IF EXISTS qualified_name_debug;
        ALTER TABLE entities RENAME TO legacy_entities;
        ALTER TABLE edges RENAME TO legacy_edges;
        ALTER TABLE source_spans RENAME TO legacy_source_spans;
        ALTER TABLE files RENAME TO legacy_files;
        ",
    )?;
    connection.execute_batch(SCHEMA_SQL)?;

    let files = {
        let mut statement = connection.prepare("SELECT * FROM legacy_files")?;
        let rows = statement.query_map([], file_from_row)?;
        collect_rows(rows)?
    };
    for file in &files {
        insert_file_compact_row(connection, file)?;
    }

    let entities = {
        let mut statement = connection.prepare("SELECT * FROM legacy_entities")?;
        let rows = statement.query_map([], entity_from_row)?;
        collect_rows(rows)?
    };
    for entity in &entities {
        insert_entity_compact_row(connection, entity, &to_json(&entity.metadata)?)?;
    }

    let edges = {
        let mut statement = connection.prepare("SELECT * FROM legacy_edges")?;
        let rows = statement.query_map([], edge_from_row)?;
        collect_rows(rows)?
    };
    for edge in &edges {
        let edge = normalize_edge_for_storage(edge)?;
        insert_edge_compact_row(
            connection,
            &edge,
            &to_json(&edge.provenance_edges)?,
            &to_json(&edge.metadata)?,
        )?;
    }

    let spans = {
        let mut statement = connection.prepare("SELECT * FROM legacy_source_spans")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>("id")?, source_span_from_row(row)?))
        })?;
        collect_rows(rows)?
    };
    for (id, span) in &spans {
        insert_source_span_compact_row(connection, id, span)?;
    }

    connection.execute_batch(
        "
        DROP TABLE legacy_entities;
        DROP TABLE legacy_edges;
        DROP TABLE legacy_source_spans;
        DROP TABLE legacy_files;
        ",
    )?;
    Ok(())
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> StoreResult<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn table_column_decl_type(
    connection: &Connection,
    table: &str,
    column: &str,
) -> StoreResult<Option<String>> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
    })?;
    for row in rows {
        let (name, decl_type) = row?;
        if name == column {
            return Ok(Some(decl_type.to_ascii_uppercase()));
        }
    }
    Ok(None)
}

fn migrate_dictionary_compaction(connection: &Connection) -> StoreResult<()> {
    connection.execute_batch(
        "
        DROP VIEW IF EXISTS qualified_name_lookup;
        DROP VIEW IF EXISTS qualified_name_debug;
        DROP VIEW IF EXISTS object_id_lookup;
        DROP VIEW IF EXISTS object_id_debug;
        ",
    )?;
    for table in ["object_id_dict", "symbol_dict", "qname_prefix_dict"] {
        migrate_hashed_dictionary_table(connection, table)?;
    }
    migrate_qualified_name_dictionary(connection)?;
    Ok(())
}

fn migrate_hashed_dictionary_table(connection: &Connection, table: &str) -> StoreResult<()> {
    let legacy = format!("legacy_{table}_hash_compaction");
    if !exists_in_sqlite_master(connection, "table", table)? {
        if exists_in_sqlite_master(connection, "table", &legacy)? {
            connection.execute_batch(&format!("ALTER TABLE {legacy} RENAME TO {table};"))?;
        } else {
            return Ok(());
        }
    }
    if table_has_column(connection, table, "value_hash")? {
        return Ok(());
    }
    if exists_in_sqlite_master(connection, "table", &legacy)? {
        connection.execute_batch(&format!("DROP TABLE {legacy};"))?;
    }
    connection.execute_batch(&format!(
        "
        ALTER TABLE {table} RENAME TO {legacy};
        CREATE TABLE {table} (
            id INTEGER PRIMARY KEY,
            value TEXT NOT NULL,
            value_hash INTEGER NOT NULL,
            value_len INTEGER NOT NULL
        );
        INSERT INTO {table} (id, value, value_hash, value_len)
        SELECT id, value, codegraph_text_hash(value), length(value)
        FROM {legacy}
        ORDER BY id;
        DROP TABLE {legacy};
        "
    ))?;
    Ok(())
}

fn migrate_qualified_name_dictionary(connection: &Connection) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", "qualified_name_dict")? {
        if exists_in_sqlite_master(connection, "table", "legacy_qualified_name_dict_compaction")? {
            connection.execute_batch(
                "ALTER TABLE legacy_qualified_name_dict_compaction RENAME TO qualified_name_dict;",
            )?;
        } else {
            return Ok(());
        }
    }
    if table_has_column(connection, "qualified_name_dict", "value_hash")?
        || !table_has_column(connection, "qualified_name_dict", "value")?
    {
        return Ok(());
    }
    if exists_in_sqlite_master(connection, "table", "legacy_qualified_name_dict_compaction")? {
        connection.execute_batch("DROP TABLE legacy_qualified_name_dict_compaction;")?;
    }
    connection.execute_batch(
        "
        ALTER TABLE qualified_name_dict RENAME TO legacy_qualified_name_dict_compaction;
        CREATE TABLE qualified_name_dict (
            id INTEGER PRIMARY KEY,
            prefix_id INTEGER NOT NULL,
            suffix_id INTEGER NOT NULL,
            value TEXT
        );
        INSERT INTO qualified_name_dict (id, prefix_id, suffix_id, value)
        SELECT id, prefix_id, suffix_id, NULL
        FROM legacy_qualified_name_dict_compaction
        ORDER BY id;
        DROP TABLE legacy_qualified_name_dict_compaction;
        ",
    )?;
    Ok(())
}

fn migrate_edge_classification_columns(connection: &Connection) -> StoreResult<()> {
    let user_version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let had_edge_class = table_has_column(connection, "edges", "edge_class_id")?;
    let had_context = table_has_column(connection, "edges", "context_id")?;
    if !had_edge_class {
        connection.execute("ALTER TABLE edges ADD COLUMN edge_class_id INTEGER", [])?;
    }
    if !had_context {
        connection.execute("ALTER TABLE edges ADD COLUMN context_id INTEGER", [])?;
    }
    if had_edge_class && had_context && user_version >= 6 {
        return Ok(());
    }
    let has_unclassified_edges = connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM edges
            WHERE edge_class_id IS NULL OR context_id IS NULL
            LIMIT 1
        )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !has_unclassified_edges {
        return Ok(());
    }

    let unknown_class = intern_edge_class(connection, EdgeClass::Unknown.as_str())?;
    let base_exact_class = intern_edge_class(connection, EdgeClass::BaseExact.as_str())?;
    let base_heuristic_class = intern_edge_class(connection, EdgeClass::BaseHeuristic.as_str())?;
    let reified_callsite_class =
        intern_edge_class(connection, EdgeClass::ReifiedCallsite.as_str())?;
    let derived_class = intern_edge_class(connection, EdgeClass::Derived.as_str())?;
    let test_class = intern_edge_class(connection, EdgeClass::Test.as_str())?;
    let mock_class = intern_edge_class(connection, EdgeClass::Mock.as_str())?;

    let production_context = intern_edge_context(connection, EdgeContext::Production.as_str())?;
    let test_context = intern_edge_context(connection, EdgeContext::Test.as_str())?;
    let mock_context = intern_edge_context(connection, EdgeContext::Mock.as_str())?;

    connection.execute(
        "UPDATE edges SET edge_class_id = ?1 WHERE edge_class_id IS NULL",
        [unknown_class],
    )?;
    connection.execute(
        "UPDATE edges SET context_id = ?1 WHERE context_id IS NULL",
        [production_context],
    )?;
    connection.execute(
        "
        UPDATE edges
        SET context_id = ?1
        WHERE relation_id IN (
            SELECT id FROM relation_kind_dict WHERE value IN ('MOCKS', 'STUBS')
        )
        ",
        [mock_context],
    )?;
    connection.execute(
        "
        UPDATE edges
        SET context_id = ?1
        WHERE context_id != ?2
          AND relation_id IN (
            SELECT id FROM relation_kind_dict WHERE value IN (
                'TESTS', 'ASSERTS', 'COVERS', 'FIXTURES_FOR'
            )
        )
        ",
        params![test_context, mock_context],
    )?;
    connection.execute(
        "UPDATE edges SET edge_class_id = ?1 WHERE context_id = ?2",
        params![mock_class, mock_context],
    )?;
    connection.execute(
        "UPDATE edges SET edge_class_id = ?1 WHERE context_id = ?2",
        params![test_class, test_context],
    )?;
    connection.execute(
        "
        UPDATE edges
        SET edge_class_id = ?1
        WHERE context_id = ?2
          AND (
              derived != 0
              OR relation_id IN (
                  SELECT id FROM relation_kind_dict WHERE value IN (
                      'MAY_MUTATE', 'MAY_READ', 'API_REACHES',
                      'ASYNC_REACHES', 'SCHEMA_IMPACT'
                  )
              )
              OR exactness_id IN (
                  SELECT id FROM exactness_dict WHERE value = 'derived_from_verified_edges'
              )
          )
        ",
        params![derived_class, production_context],
    )?;
    connection.execute(
        "
        UPDATE edges
        SET edge_class_id = ?1
        WHERE edge_class_id = ?2
          AND relation_id IN (
              SELECT id FROM relation_kind_dict WHERE value IN (
                  'CALLEE', 'ARGUMENT_0', 'ARGUMENT_1', 'ARGUMENT_N', 'RETURNS_TO'
              )
          )
        ",
        params![reified_callsite_class, unknown_class],
    )?;
    connection.execute(
        "
        UPDATE edges
        SET edge_class_id = ?1
        WHERE edge_class_id = ?2
          AND exactness_id IN (
              SELECT id FROM exactness_dict
              WHERE value IN ('static_heuristic', 'inferred')
          )
        ",
        params![base_heuristic_class, unknown_class],
    )?;
    connection.execute(
        "
        UPDATE edges
        SET edge_class_id = ?1
        WHERE edge_class_id = ?2
          AND exactness_id IN (
              SELECT id FROM exactness_dict
              WHERE value IN (
                  'exact', 'compiler_verified', 'lsp_verified', 'parser_verified'
              )
          )
          AND relation_id NOT IN (
              SELECT id FROM relation_kind_dict WHERE value IN (
                  'CALLED_BY', 'MUTATED_BY', 'DEFINED_IN', 'ALIASED_BY'
              )
          )
        ",
        params![base_exact_class, unknown_class],
    )?;
    Ok(())
}

fn migrate_structural_compaction_columns(connection: &Connection) -> StoreResult<()> {
    for (column, sql_type) in [
        ("parent_id", "INTEGER"),
        ("file_id", "INTEGER"),
        ("scope_id", "INTEGER"),
        ("declaration_span_id", "INTEGER"),
    ] {
        if !table_has_column(connection, "entities", column)? {
            connection.execute(
                &format!("ALTER TABLE entities ADD COLUMN {column} {sql_type}"),
                [],
            )?;
        }
    }
    connection.execute(
        "UPDATE entities
         SET file_id = path_id
         WHERE file_id IS NULL",
        [],
    )?;
    connection.execute(
        "UPDATE entities
         SET declaration_span_id = id_key
         WHERE declaration_span_id IS NULL
           AND start_line IS NOT NULL
           AND end_line IS NOT NULL",
        [],
    )?;
    Ok(())
}

fn migrate_file_hash_normalization(connection: &Connection) -> StoreResult<()> {
    connection.execute_batch(
        "
        DROP VIEW IF EXISTS file_instance;
        DROP VIEW IF EXISTS object_id_lookup;
        DROP VIEW IF EXISTS object_id_debug;
        ",
    )?;
    if exists_in_sqlite_master(connection, "table", "entities")?
        && !table_has_column(connection, "entities", "entity_hash")?
    {
        connection.execute("ALTER TABLE entities ADD COLUMN entity_hash BLOB", [])?;
    }
    if exists_in_sqlite_master(connection, "table", "files")?
        && (!table_has_column(connection, "files", "file_id")?
            || !table_has_column(connection, "files", "content_hash")?)
    {
        connection.execute_batch(
            "
            DROP TABLE IF EXISTS legacy_files_file_hash_norm;
            ALTER TABLE files RENAME TO legacy_files_file_hash_norm;
            CREATE TABLE files (
                file_id INTEGER PRIMARY KEY,
                path_id INTEGER NOT NULL UNIQUE,
                content_hash TEXT NOT NULL,
                mtime_unix_ms INTEGER,
                size_bytes INTEGER NOT NULL,
                language_id INTEGER,
                indexed_at_unix_ms INTEGER,
                content_template_id INTEGER,
                metadata_json TEXT NOT NULL
            ) WITHOUT ROWID;
            INSERT INTO files (
                file_id, path_id, content_hash, mtime_unix_ms, size_bytes,
                language_id, indexed_at_unix_ms, content_template_id, metadata_json
            )
            SELECT path_id, path_id, file_hash, NULL, size_bytes,
                   language_id, indexed_at_unix_ms, NULL, metadata_json
            FROM legacy_files_file_hash_norm
            ORDER BY path_id;
            DROP TABLE legacy_files_file_hash_norm;
            ",
        )?;
    }

    if exists_in_sqlite_master(connection, "table", "entities")?
        && table_has_column(connection, "entities", "file_hash")?
    {
        connection.execute_batch(
            "
            DROP TABLE IF EXISTS legacy_entities_file_hash_norm;
            ALTER TABLE entities RENAME TO legacy_entities_file_hash_norm;
            CREATE TABLE entities (
                id_key INTEGER PRIMARY KEY,
                entity_hash BLOB,
                kind_id INTEGER NOT NULL,
                name_id INTEGER NOT NULL,
                qualified_name_id INTEGER NOT NULL,
                path_id INTEGER NOT NULL,
                span_path_id INTEGER,
                start_line INTEGER,
                start_column INTEGER,
                end_line INTEGER,
                end_column INTEGER,
                content_hash TEXT,
                created_from_id INTEGER NOT NULL,
                confidence REAL NOT NULL,
                metadata_json TEXT NOT NULL,
                parent_id INTEGER,
                file_id INTEGER,
                scope_id INTEGER,
                declaration_span_id INTEGER
            ) WITHOUT ROWID;
            INSERT INTO entities (
                id_key, entity_hash, kind_id, name_id, qualified_name_id, path_id,
                span_path_id, start_line, start_column, end_line, end_column,
                content_hash, created_from_id, confidence, metadata_json,
                parent_id, file_id, scope_id, declaration_span_id
            )
            SELECT id_key, NULL, kind_id, name_id, qualified_name_id, path_id,
                   span_path_id, start_line, start_column, end_line, end_column,
                   content_hash, created_from_id, confidence, metadata_json,
                   parent_id, COALESCE(file_id, path_id), scope_id, declaration_span_id
            FROM legacy_entities_file_hash_norm
            ORDER BY id_key;
            DROP TABLE legacy_entities_file_hash_norm;
            ",
        )?;
    }

    rebuild_fact_table_without_file_hash(
        connection,
        "edges",
        "
        CREATE TABLE edges (
            id_key INTEGER PRIMARY KEY,
            head_id_key INTEGER NOT NULL,
            relation_id INTEGER NOT NULL,
            tail_id_key INTEGER NOT NULL,
            span_path_id INTEGER NOT NULL,
            start_line INTEGER NOT NULL,
            start_column INTEGER,
            end_line INTEGER NOT NULL,
            end_column INTEGER,
            repo_commit TEXT,
            file_id INTEGER NOT NULL,
            extractor_id INTEGER NOT NULL,
            confidence REAL NOT NULL,
            exactness_id INTEGER NOT NULL,
            edge_class_id INTEGER NOT NULL,
            context_id INTEGER NOT NULL,
            derived INTEGER NOT NULL,
            provenance_edges_json TEXT NOT NULL,
            metadata_json TEXT NOT NULL
        ) WITHOUT ROWID
        ",
        "id_key, head_id_key, relation_id, tail_id_key, span_path_id,
         start_line, start_column, end_line, end_column, repo_commit,
         COALESCE((SELECT file_id FROM files WHERE path_id = span_path_id), span_path_id),
         extractor_id, confidence, exactness_id, edge_class_id, context_id,
         derived, provenance_edges_json, metadata_json",
    )?;
    rebuild_fact_table_without_file_hash(
        connection,
        "structural_relations",
        "
        CREATE TABLE structural_relations (
            id_key INTEGER PRIMARY KEY,
            head_id_key INTEGER NOT NULL,
            relation_id INTEGER NOT NULL,
            tail_id_key INTEGER NOT NULL,
            span_path_id INTEGER NOT NULL,
            start_line INTEGER NOT NULL,
            start_column INTEGER,
            end_line INTEGER NOT NULL,
            end_column INTEGER,
            repo_commit TEXT,
            file_id INTEGER NOT NULL,
            extractor_id INTEGER NOT NULL,
            confidence REAL NOT NULL,
            exactness_id INTEGER NOT NULL,
            edge_class_id INTEGER NOT NULL,
            context_id INTEGER NOT NULL,
            metadata_json TEXT NOT NULL
        ) WITHOUT ROWID
        ",
        "id_key, head_id_key, relation_id, tail_id_key, span_path_id,
         start_line, start_column, end_line, end_column, repo_commit,
         COALESCE((SELECT file_id FROM files WHERE path_id = span_path_id), span_path_id),
         extractor_id, confidence, exactness_id, edge_class_id, context_id,
         metadata_json",
    )?;
    rebuild_fact_table_without_file_hash(
        connection,
        "callsites",
        "
        CREATE TABLE callsites (
            id_key INTEGER PRIMARY KEY,
            callsite_id_key INTEGER NOT NULL,
            caller_id_key INTEGER,
            relation_id INTEGER NOT NULL,
            callee_id_key INTEGER NOT NULL,
            span_path_id INTEGER NOT NULL,
            start_line INTEGER NOT NULL,
            start_column INTEGER,
            end_line INTEGER NOT NULL,
            end_column INTEGER,
            repo_commit TEXT,
            file_id INTEGER NOT NULL,
            extractor_id INTEGER NOT NULL,
            confidence REAL NOT NULL,
            exactness_id INTEGER NOT NULL,
            edge_class_id INTEGER NOT NULL,
            context_id INTEGER NOT NULL,
            metadata_json TEXT NOT NULL
        ) WITHOUT ROWID
        ",
        "id_key, callsite_id_key, caller_id_key, relation_id, callee_id_key,
         span_path_id, start_line, start_column, end_line, end_column, repo_commit,
         COALESCE((SELECT file_id FROM files WHERE path_id = span_path_id), span_path_id),
         extractor_id, confidence, exactness_id, edge_class_id, context_id,
         metadata_json",
    )?;
    rebuild_fact_table_without_file_hash(
        connection,
        "callsite_args",
        "
        CREATE TABLE callsite_args (
            id_key INTEGER PRIMARY KEY,
            callsite_id_key INTEGER NOT NULL,
            ordinal INTEGER NOT NULL,
            relation_id INTEGER NOT NULL,
            argument_id_key INTEGER NOT NULL,
            span_path_id INTEGER NOT NULL,
            start_line INTEGER NOT NULL,
            start_column INTEGER,
            end_line INTEGER NOT NULL,
            end_column INTEGER,
            repo_commit TEXT,
            file_id INTEGER NOT NULL,
            extractor_id INTEGER NOT NULL,
            confidence REAL NOT NULL,
            exactness_id INTEGER NOT NULL,
            edge_class_id INTEGER NOT NULL,
            context_id INTEGER NOT NULL,
            metadata_json TEXT NOT NULL
        ) WITHOUT ROWID
        ",
        "id_key, callsite_id_key, ordinal, relation_id, argument_id_key,
         span_path_id, start_line, start_column, end_line, end_column, repo_commit,
         COALESCE((SELECT file_id FROM files WHERE path_id = span_path_id), span_path_id),
         extractor_id, confidence, exactness_id, edge_class_id, context_id,
         metadata_json",
    )?;
    connection.execute_batch(BULK_INDEX_CREATE_SQL)?;
    refresh_file_instance_view(connection)?;
    Ok(())
}

fn rebuild_fact_table_without_file_hash(
    connection: &Connection,
    table: &str,
    create_sql: &str,
    select_sql: &str,
) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", table)?
        || !table_has_column(connection, table, "file_hash")?
    {
        return Ok(());
    }
    let legacy = format!("legacy_{table}_file_hash_norm");
    connection.execute_batch(&format!("DROP TABLE IF EXISTS {legacy};"))?;
    connection.execute_batch(&format!("ALTER TABLE {table} RENAME TO {legacy};"))?;
    connection.execute_batch(create_sql)?;
    connection.execute_batch(&format!(
        "INSERT INTO {table} SELECT {select_sql} FROM {legacy} ORDER BY id_key;"
    ))?;
    connection.execute_batch(&format!("DROP TABLE {legacy};"))?;
    Ok(())
}

fn migrate_edge_metadata_compaction(connection: &Connection) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", "edges")? {
        return Ok(());
    }
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS resolution_kind_dict (
            id INTEGER PRIMARY KEY,
            value TEXT NOT NULL UNIQUE
        );
        CREATE TABLE IF NOT EXISTS edge_provenance_dict (
            id INTEGER PRIMARY KEY,
            value TEXT NOT NULL UNIQUE
        );
        CREATE TABLE IF NOT EXISTS edge_debug_metadata (
            edge_id INTEGER PRIMARY KEY,
            metadata_json TEXT NOT NULL
        ) WITHOUT ROWID;
        ",
    )?;
    for value in [
        "not_recorded",
        "unresolved_static_heuristic",
        "unresolved_dynamic_import",
        "resolved_static_import",
        "resolved",
        "derived_from_base_path",
        "derived_from_verified_edges",
    ] {
        intern_resolution_kind(connection, value)?;
    }
    intern_edge_provenance(connection, "[]")?;

    let already_compact = table_has_column(connection, "edges", "resolution_kind_id")?
        && table_has_column(connection, "edges", "context_kind_id")?
        && table_has_column(connection, "edges", "flags_bitset")?
        && table_has_column(connection, "edges", "confidence_q")?
        && table_has_column(connection, "edges", "provenance_id")?
        && !table_has_column(connection, "edges", "metadata_json")?;
    if already_compact {
        refresh_edges_compat_view(connection)?;
        return Ok(());
    }

    connection.execute_batch(
        "
        DROP TABLE IF EXISTS legacy_edges_metadata_compact;
        ALTER TABLE edges RENAME TO legacy_edges_metadata_compact;
        CREATE TABLE edges (
            id_key INTEGER PRIMARY KEY,
            head_id_key INTEGER NOT NULL,
            relation_id INTEGER NOT NULL,
            tail_id_key INTEGER NOT NULL,
            span_path_id INTEGER NOT NULL,
            start_line INTEGER NOT NULL,
            start_column INTEGER,
            end_line INTEGER NOT NULL,
            end_column INTEGER,
            repo_commit TEXT,
            file_id INTEGER NOT NULL,
            extractor_id INTEGER NOT NULL,
            confidence REAL NOT NULL,
            confidence_q INTEGER NOT NULL,
            exactness_id INTEGER NOT NULL,
            resolution_kind_id INTEGER NOT NULL,
            edge_class_id INTEGER NOT NULL,
            context_id INTEGER NOT NULL,
            context_kind_id INTEGER NOT NULL,
            flags_bitset INTEGER NOT NULL,
            derived INTEGER NOT NULL,
            provenance_edges_json TEXT NOT NULL,
            provenance_id INTEGER NOT NULL
        ) WITHOUT ROWID;
        ",
    )?;

    connection.execute(
        "
        INSERT OR IGNORE INTO edge_provenance_dict(value)
        SELECT DISTINCT COALESCE(provenance_edges_json, '[]')
        FROM legacy_edges_metadata_compact
        ",
        [],
    )?;
    connection.execute(
        "
        INSERT OR IGNORE INTO resolution_kind_dict(value)
        SELECT DISTINCT
            CASE
                WHEN COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"unresolved_dynamic_import\"%' THEN 'unresolved_dynamic_import'
                WHEN COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"unresolved_static_heuristic\"%' THEN 'unresolved_static_heuristic'
                WHEN COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"resolved_static_import\"%' THEN 'resolved_static_import'
                WHEN COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"derived_from_base_path\"%' THEN 'derived_from_base_path'
                WHEN COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"resolved\"%' THEN 'resolved'
                WHEN derived != 0
                     OR exactness_id IN (
                         SELECT id FROM exactness_dict WHERE value = 'derived_from_verified_edges'
                     ) THEN 'derived_from_verified_edges'
                WHEN exactness_id IN (
                    SELECT id FROM exactness_dict WHERE value IN ('static_heuristic', 'inferred')
                ) THEN 'unresolved_static_heuristic'
                ELSE 'not_recorded'
            END
        FROM legacy_edges_metadata_compact
        ",
        [],
    )?;
    connection.execute(
        "
        INSERT INTO edges (
            id_key, head_id_key, relation_id, tail_id_key,
            span_path_id, start_line, start_column, end_line, end_column,
            repo_commit, file_id, extractor_id, confidence, confidence_q,
            exactness_id, resolution_kind_id, edge_class_id, context_id,
            context_kind_id, flags_bitset, derived, provenance_edges_json,
            provenance_id
        )
        SELECT
            id_key, head_id_key, relation_id, tail_id_key,
            span_path_id, start_line, start_column, end_line, end_column,
            repo_commit, file_id, extractor_id, confidence,
            CAST(ROUND(
                CASE
                    WHEN confidence IS NULL THEN 0.0
                    WHEN confidence < 0.0 THEN 0.0
                    WHEN confidence > 1.0 THEN 1000.0
                    ELSE confidence * 1000.0
                END
            ) AS INTEGER),
            exactness_id,
            (
                SELECT id FROM resolution_kind_dict
                WHERE value = CASE
                    WHEN COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"unresolved_dynamic_import\"%' THEN 'unresolved_dynamic_import'
                    WHEN COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"unresolved_static_heuristic\"%' THEN 'unresolved_static_heuristic'
                    WHEN COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"resolved_static_import\"%' THEN 'resolved_static_import'
                    WHEN COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"derived_from_base_path\"%' THEN 'derived_from_base_path'
                    WHEN COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"resolved\"%' THEN 'resolved'
                    WHEN derived != 0
                         OR exactness_id IN (
                             SELECT id FROM exactness_dict WHERE value = 'derived_from_verified_edges'
                         ) THEN 'derived_from_verified_edges'
                    WHEN exactness_id IN (
                        SELECT id FROM exactness_dict WHERE value IN ('static_heuristic', 'inferred')
                    ) THEN 'unresolved_static_heuristic'
                    ELSE 'not_recorded'
                END
            ),
            edge_class_id,
            context_id,
            context_id,
            (
                CASE WHEN COALESCE(metadata_json, '{}') LIKE '%\"heuristic\":true%'
                       OR exactness_id IN (
                           SELECT id FROM exactness_dict WHERE value IN ('static_heuristic', 'inferred')
                       )
                     THEN 1 ELSE 0 END
                +
                CASE WHEN COALESCE(metadata_json, '{}') LIKE '%unresolved%'
                     THEN 2 ELSE 0 END
                +
                CASE WHEN COALESCE(metadata_json, '{}') LIKE '%\"import_kind\":\"dynamic\"%'
                       OR COALESCE(metadata_json, '{}') LIKE '%dynamic_import%'
                     THEN 4 ELSE 0 END
                +
                CASE WHEN COALESCE(metadata_json, '{}') LIKE '%static%'
                     THEN 8 ELSE 0 END
                +
                CASE WHEN COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"resolved\"%'
                       OR COALESCE(metadata_json, '{}') LIKE '%\"resolution\":\"resolved_static_import\"%'
                     THEN 16 ELSE 0 END
            ),
            derived,
            COALESCE(provenance_edges_json, '[]'),
            (
                SELECT id FROM edge_provenance_dict
                WHERE value = COALESCE(legacy_edges_metadata_compact.provenance_edges_json, '[]')
            )
        FROM legacy_edges_metadata_compact
        ORDER BY id_key
        ",
        [],
    )?;
    connection.execute_batch("DROP TABLE legacy_edges_metadata_compact;")?;
    connection.execute_batch(BULK_INDEX_CREATE_SQL)?;
    refresh_edges_compat_view(connection)?;
    Ok(())
}

fn ensure_entity_hash_column(connection: &Connection) -> StoreResult<()> {
    if exists_in_sqlite_master(connection, "table", "entities")?
        && !table_has_column(connection, "entities", "entity_hash")?
    {
        connection.execute("ALTER TABLE entities ADD COLUMN entity_hash BLOB", [])?;
    }
    Ok(())
}

fn migrate_object_id_compaction(connection: &Connection) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", "entities")? {
        return Ok(());
    }
    ensure_entity_hash_column(connection)?;
    let canonical_object_rows = if exists_in_sqlite_master(connection, "table", "object_id_dict")?
        && table_has_column(connection, "object_id_dict", "value")?
    {
        connection.query_row(
            "SELECT COUNT(*) FROM object_id_dict WHERE value LIKE 'repo://e/%'",
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    if table_has_column(connection, "entities", "entity_hash")? && canonical_object_rows == 0 {
        refresh_object_id_views(connection)?;
        return Ok(());
    }

    connection.execute_batch(
        "
        DROP VIEW IF EXISTS object_id_lookup;
        DROP VIEW IF EXISTS object_id_debug;
        DROP VIEW IF EXISTS edges_compat;
        DROP INDEX IF EXISTS idx_entities_entity_hash;
        ",
    )?;

    if exists_in_sqlite_master(connection, "table", "object_id_dict")?
        && table_has_column(connection, "object_id_dict", "value")?
    {
        connection.execute(
            "
            UPDATE entities
            SET entity_hash = (
                SELECT codegraph_entity_hash(object_id_dict.value)
                FROM object_id_dict
                WHERE object_id_dict.id = entities.id_key
                  AND object_id_dict.value LIKE 'repo://e/%'
            )
            WHERE EXISTS (
                SELECT 1 FROM object_id_dict
                WHERE object_id_dict.id = entities.id_key
                  AND object_id_dict.value LIKE 'repo://e/%'
            )
            ",
            [],
        )?;
    }

    let missing_hashes = connection.query_row(
        "SELECT COUNT(*) FROM entities WHERE entity_hash IS NULL OR length(entity_hash) = 0",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if missing_hashes > 0 {
        return Err(StoreError::Message(format!(
            "object id compaction cannot reconstruct {missing_hashes} entity hashes"
        )));
    }

    let duplicate_hash = connection
        .query_row(
            "
            SELECT lower(hex(entity_hash))
            FROM entities
            GROUP BY entity_hash
            HAVING COUNT(*) > 1
            LIMIT 1
            ",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(hash) = duplicate_hash {
        return Err(StoreError::Message(format!(
            "object id compaction found duplicate entity hash {hash}"
        )));
    }

    connection.execute_batch(
        "
        DROP TABLE IF EXISTS legacy_object_id_dict_compact;
        ALTER TABLE object_id_dict RENAME TO legacy_object_id_dict_compact;
        CREATE TABLE object_id_dict (
            id INTEGER PRIMARY KEY,
            value TEXT NOT NULL,
            value_hash INTEGER NOT NULL,
            value_len INTEGER NOT NULL
        );
        INSERT OR REPLACE INTO object_id_dict (id, value, value_hash, value_len)
        SELECT id, value, codegraph_text_hash(value), length(value)
        FROM legacy_object_id_dict_compact
        WHERE value NOT LIKE 'repo://e/%'
        ORDER BY id;
        DROP TABLE legacy_object_id_dict_compact;
        ",
    )?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_object_id_dict_hash ON object_id_dict(value_hash, value_len)",
        [],
    )?;
    refresh_object_id_views(connection)?;
    refresh_edges_compat_view(connection)?;
    connection.execute_batch(BULK_INDEX_CREATE_SQL)?;
    Ok(())
}

fn migrate_heuristic_debug_sidecar(connection: &Connection) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", "edges")? {
        return Ok(());
    }
    connection.execute_batch(SCHEMA_SQL)?;
    let static_exactness_filter =
        "SELECT id FROM exactness_dict WHERE value IN ('static_heuristic', 'inferred')";
    let heuristic_edge_class_filter =
        "SELECT id FROM edge_class_dict WHERE value IN ('base_heuristic', 'unknown')";
    connection.execute(
        &format!(
            "
            DELETE FROM edges
            WHERE exactness_id IN ({static_exactness_filter})
               OR edge_class_id IN ({heuristic_edge_class_filter})
               OR (flags_bitset & 3) != 0
            "
        ),
        [],
    )?;
    for table in ["structural_relations", "callsites", "callsite_args"] {
        connection.execute(
            &format!(
                "
                DELETE FROM {table}
                WHERE exactness_id IN ({static_exactness_filter})
                   OR edge_class_id IN ({heuristic_edge_class_filter})
                "
            ),
            [],
        )?;
    }
    connection.execute(
        "
        DELETE FROM entities
        WHERE created_from_id IN (
            SELECT id FROM extractor_dict WHERE lower(value) LIKE '%heuristic%'
        )
           OR name_id IN (
            SELECT id FROM symbol_dict WHERE value = 'unknown_callee'
        )
           OR qualified_name_id IN (
            SELECT id FROM qualified_name_lookup WHERE value LIKE 'static_reference:%'
        )
        ",
        [],
    )?;
    connection.execute_batch(BULK_INDEX_CREATE_SQL)?;
    refresh_edges_compat_view(connection)?;
    Ok(())
}

fn migrate_structural_relation_attributes(connection: &Connection) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", "entities")? {
        return Ok(());
    }
    if !table_has_column(connection, "entities", "structural_flags")? {
        connection.execute(
            "ALTER TABLE entities ADD COLUMN structural_flags INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    let contains_id = intern_relation_kind(connection, &RelationKind::Contains.to_string())?;
    let defined_in_id = intern_relation_kind(connection, &RelationKind::DefinedIn.to_string())?;
    let declares_id = intern_relation_kind(connection, &RelationKind::Declares.to_string())?;
    if exists_in_sqlite_master(connection, "table", "structural_relations")? {
        connection.execute_batch("DROP TABLE IF EXISTS temp.structural_parent_map;")?;
        connection.execute(
            "
            CREATE TEMP TABLE structural_parent_map AS
            SELECT tail_id_key AS child_id,
                   COALESCE(
                       MIN(CASE WHEN relation_id = ?1 THEN head_id_key END),
                       MIN(CASE WHEN relation_id = ?2 THEN head_id_key END)
                   ) AS parent_id,
                   MAX(CASE WHEN relation_id = ?1 THEN 1 ELSE 0 END) AS contained,
                   MAX(CASE WHEN relation_id = ?2 THEN 1 ELSE 0 END) AS declared
            FROM structural_relations
            WHERE relation_id IN (?1, ?2)
            GROUP BY tail_id_key
            ",
            params![contains_id, declares_id],
        )?;
        connection.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS temp.idx_structural_parent_map_child
                ON structural_parent_map(child_id);
            ",
        )?;
        connection.execute(
            "
            UPDATE entities
            SET parent_id = COALESCE(parent_id, (
                    SELECT parent_id
                    FROM structural_parent_map
                    WHERE child_id = entities.id_key
                )),
                scope_id = COALESCE(scope_id, (
                    SELECT parent_id
                    FROM structural_parent_map
                    WHERE child_id = entities.id_key
                )),
                structural_flags = structural_flags
                    | CASE WHEN (
                        SELECT contained
                        FROM structural_parent_map
                        WHERE child_id = entities.id_key
                    ) != 0 THEN ?1 ELSE 0 END
                    | CASE WHEN (
                        SELECT declared
                        FROM structural_parent_map
                        WHERE child_id = entities.id_key
                    ) != 0 THEN ?2 ELSE 0 END
            WHERE id_key IN (SELECT child_id FROM structural_parent_map)
            ",
            params![
                ENTITY_STRUCTURAL_FLAG_CONTAINED_BY_PARENT,
                ENTITY_STRUCTURAL_FLAG_DECLARED_BY_PARENT,
            ],
        )?;
        connection.execute_batch("DROP TABLE IF EXISTS temp.defined_in_parent_map;")?;
        connection.execute(
            "
            CREATE TEMP TABLE defined_in_parent_map AS
            SELECT head_id_key AS child_id,
                   MIN(tail_id_key) AS parent_id
            FROM structural_relations
            WHERE relation_id = ?1
            GROUP BY head_id_key
            ",
            [defined_in_id],
        )?;
        connection.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS temp.idx_defined_in_parent_map_child
                ON defined_in_parent_map(child_id);
            ",
        )?;
        connection.execute(
            "
            UPDATE entities
            SET parent_id = COALESCE(parent_id, (
                    SELECT parent_id
                    FROM defined_in_parent_map
                    WHERE child_id = entities.id_key
                )),
                scope_id = COALESCE(scope_id, (
                    SELECT parent_id
                    FROM defined_in_parent_map
                    WHERE child_id = entities.id_key
                )),
                structural_flags = structural_flags | ?1
            WHERE parent_id IS NULL
              AND id_key IN (SELECT child_id FROM defined_in_parent_map)
            ",
            [ENTITY_STRUCTURAL_FLAG_DEFINED_IN_PARENT],
        )?;
        connection.execute(
            "
            UPDATE entities
            SET structural_flags = structural_flags | ?1
            WHERE parent_id IS NOT NULL
              AND id_key IN (SELECT child_id FROM defined_in_parent_map)
            ",
            [ENTITY_STRUCTURAL_FLAG_DEFINED_IN_PARENT],
        )?;
        connection.execute(
            "
            DELETE FROM structural_relations
            WHERE relation_id IN (?1, ?2, ?3)
            ",
            params![contains_id, defined_in_id, declares_id],
        )?;
        connection.execute_batch(
            "
            DROP TABLE IF EXISTS temp.structural_parent_map;
            DROP TABLE IF EXISTS temp.defined_in_parent_map;
            ",
        )?;
    }
    connection.execute_batch(BULK_INDEX_CREATE_SQL)?;
    refresh_edges_compat_view(connection)?;
    Ok(())
}

fn migrate_content_template_overlay(connection: &Connection) -> StoreResult<()> {
    connection.execute_batch(SCHEMA_SQL)?;
    let rows = {
        let mut statement = connection.prepare(
            "
            SELECT path_id, content_hash, language_id
            FROM files
            WHERE content_template_id IS NULL
              AND content_hash IS NOT NULL
              AND content_hash != ''
            ORDER BY path_id
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })?;
        collect_rows(rows)?
    };
    for (path_id, content_hash, language_id) in rows {
        let template_id =
            ensure_content_template_for_path_id(connection, &content_hash, language_id, path_id)?;
        connection.execute(
            "UPDATE files SET content_template_id = ?1 WHERE path_id = ?2",
            params![template_id, path_id],
        )?;
    }
    connection.execute_batch(BULK_INDEX_CREATE_SQL)?;
    Ok(())
}

fn migrate_template_local_id_compaction(connection: &Connection) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", "template_entities")?
        || !exists_in_sqlite_master(connection, "table", "template_edges")?
    {
        return Ok(());
    }
    let already_compact =
        table_column_decl_type(connection, "template_entities", "local_template_entity_id")?
            .is_some_and(|decl_type| decl_type == "INTEGER");
    if already_compact {
        connection.execute_batch(
            "
            DROP INDEX IF EXISTS idx_template_edges_head_relation;
            DROP INDEX IF EXISTS idx_template_edges_tail_relation;
            ",
        )?;
        return Ok(());
    }

    connection.execute_batch(
        "
        DROP INDEX IF EXISTS idx_template_edges_head_relation;
        DROP INDEX IF EXISTS idx_template_edges_tail_relation;
        DROP TABLE IF EXISTS temp.template_entity_local_id_map;
        DROP TABLE IF EXISTS temp.template_edge_local_id_map;

        CREATE TEMP TABLE template_entity_local_id_map (
            content_template_id INTEGER NOT NULL,
            old_local_template_entity_id TEXT NOT NULL,
            new_local_template_entity_id INTEGER NOT NULL,
            PRIMARY KEY (content_template_id, old_local_template_entity_id)
        ) WITHOUT ROWID;

        INSERT INTO template_entity_local_id_map (
            content_template_id,
            old_local_template_entity_id,
            new_local_template_entity_id
        )
        SELECT content_template_id,
               local_template_entity_id,
               ROW_NUMBER() OVER (
                   PARTITION BY content_template_id
                   ORDER BY local_template_entity_id
               )
        FROM template_entities;

        CREATE TEMP TABLE template_edge_local_id_map (
            content_template_id INTEGER NOT NULL,
            old_local_template_edge_id TEXT NOT NULL,
            new_local_template_edge_id INTEGER NOT NULL,
            PRIMARY KEY (content_template_id, old_local_template_edge_id)
        ) WITHOUT ROWID;

        INSERT INTO template_edge_local_id_map (
            content_template_id,
            old_local_template_edge_id,
            new_local_template_edge_id
        )
        SELECT content_template_id,
               local_template_edge_id,
               ROW_NUMBER() OVER (
                   PARTITION BY content_template_id
                   ORDER BY local_template_edge_id
               )
        FROM template_edges;

        CREATE TABLE template_entities_compact (
            content_template_id INTEGER NOT NULL,
            local_template_entity_id INTEGER NOT NULL,
            kind_id INTEGER NOT NULL,
            name_id INTEGER NOT NULL,
            qualified_name_id INTEGER NOT NULL,
            start_line INTEGER,
            start_column INTEGER,
            end_line INTEGER,
            end_column INTEGER,
            content_hash TEXT,
            created_from_id INTEGER NOT NULL,
            confidence REAL NOT NULL,
            metadata_json TEXT NOT NULL,
            PRIMARY KEY (content_template_id, local_template_entity_id)
        ) WITHOUT ROWID;

        INSERT INTO template_entities_compact (
            content_template_id, local_template_entity_id, kind_id, name_id,
            qualified_name_id, start_line, start_column, end_line, end_column,
            content_hash, created_from_id, confidence, metadata_json
        )
        SELECT te.content_template_id,
               map.new_local_template_entity_id,
               te.kind_id,
               te.name_id,
               te.qualified_name_id,
               te.start_line,
               te.start_column,
               te.end_line,
               te.end_column,
               te.content_hash,
               te.created_from_id,
               te.confidence,
               te.metadata_json
        FROM template_entities te
        JOIN template_entity_local_id_map map
          ON map.content_template_id = te.content_template_id
         AND map.old_local_template_entity_id = te.local_template_entity_id
        ORDER BY te.content_template_id, map.new_local_template_entity_id;

        CREATE TABLE template_edges_compact (
            content_template_id INTEGER NOT NULL,
            local_template_edge_id INTEGER NOT NULL,
            local_head_entity_id INTEGER NOT NULL,
            relation_id INTEGER NOT NULL,
            local_tail_entity_id INTEGER NOT NULL,
            start_line INTEGER NOT NULL,
            start_column INTEGER,
            end_line INTEGER NOT NULL,
            end_column INTEGER,
            repo_commit TEXT,
            extractor_id INTEGER NOT NULL,
            confidence REAL NOT NULL,
            confidence_q INTEGER NOT NULL,
            exactness_id INTEGER NOT NULL,
            resolution_kind_id INTEGER NOT NULL,
            edge_class_id INTEGER NOT NULL,
            context_id INTEGER NOT NULL,
            context_kind_id INTEGER NOT NULL,
            flags_bitset INTEGER NOT NULL,
            derived INTEGER NOT NULL,
            provenance_edges_json TEXT NOT NULL,
            provenance_id INTEGER NOT NULL,
            metadata_json TEXT NOT NULL,
            PRIMARY KEY (content_template_id, local_template_edge_id)
        ) WITHOUT ROWID;

        INSERT INTO template_edges_compact (
            content_template_id, local_template_edge_id, local_head_entity_id,
            relation_id, local_tail_entity_id, start_line, start_column,
            end_line, end_column, repo_commit, extractor_id, confidence,
            confidence_q, exactness_id, resolution_kind_id, edge_class_id,
            context_id, context_kind_id, flags_bitset, derived,
            provenance_edges_json, provenance_id, metadata_json
        )
        SELECT te.content_template_id,
               edge_map.new_local_template_edge_id,
               head_map.new_local_template_entity_id,
               te.relation_id,
               tail_map.new_local_template_entity_id,
               te.start_line,
               te.start_column,
               te.end_line,
               te.end_column,
               te.repo_commit,
               te.extractor_id,
               te.confidence,
               te.confidence_q,
               te.exactness_id,
               te.resolution_kind_id,
               te.edge_class_id,
               te.context_id,
               te.context_kind_id,
               te.flags_bitset,
               te.derived,
               te.provenance_edges_json,
               te.provenance_id,
               te.metadata_json
        FROM template_edges te
        JOIN template_edge_local_id_map edge_map
          ON edge_map.content_template_id = te.content_template_id
         AND edge_map.old_local_template_edge_id = te.local_template_edge_id
        JOIN template_entity_local_id_map head_map
          ON head_map.content_template_id = te.content_template_id
         AND head_map.old_local_template_entity_id = te.local_head_entity_id
        JOIN template_entity_local_id_map tail_map
          ON tail_map.content_template_id = te.content_template_id
         AND tail_map.old_local_template_entity_id = te.local_tail_entity_id
        ORDER BY te.content_template_id, edge_map.new_local_template_edge_id;

        DROP TABLE template_edges;
        DROP TABLE template_entities;
        ALTER TABLE template_entities_compact RENAME TO template_entities;
        ALTER TABLE template_edges_compact RENAME TO template_edges;

        DROP TABLE IF EXISTS temp.template_entity_local_id_map;
        DROP TABLE IF EXISTS temp.template_edge_local_id_map;
        ",
    )?;
    Ok(())
}

fn migrate_template_source_identity_compaction(connection: &Connection) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", "template_entities")?
        || !exists_in_sqlite_master(connection, "view", "qualified_name_lookup")?
    {
        return Ok(());
    }
    let rows = {
        let mut statement = connection.prepare(
            "
            SELECT te.content_template_id,
                   te.local_template_entity_id,
                   kind.value AS kind,
                   name.value AS name,
                   qname.value AS qualified_name
            FROM template_entities te
            JOIN entity_kind_dict kind ON kind.id = te.kind_id
            JOIN symbol_dict name ON name.id = te.name_id
            JOIN qualified_name_lookup qname ON qname.id = te.qualified_name_id
            WHERE kind.value IN ('CallSite', 'Expression', 'ReturnSite')
            ORDER BY te.content_template_id, te.local_template_entity_id
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                enum_column::<EntityKind>(row, 2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        collect_rows(rows)?
    };

    for (content_template_id, local_template_entity_id, kind, name, qualified_name) in rows {
        let storage_name = template_storage_display_name(kind, &name, local_template_entity_id);
        let storage_qualified_name =
            template_storage_qualified_name(kind, &qualified_name, local_template_entity_id);
        validate_template_storage_identity(&storage_name, &storage_qualified_name)?;
        let name_id = intern_symbol(connection, &storage_name)?;
        let qualified_name_id = intern_qualified_name(connection, &storage_qualified_name)?;
        connection.execute(
            "
            UPDATE template_entities
            SET name_id = ?1,
                qualified_name_id = ?2
            WHERE content_template_id = ?3
              AND local_template_entity_id = ?4
            ",
            params![
                name_id,
                qualified_name_id,
                content_template_id,
                local_template_entity_id
            ],
        )?;
    }

    prune_unused_identity_dictionary_values(connection)?;
    Ok(())
}

fn migrate_path_evidence_metadata_compaction(connection: &Connection) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", "path_evidence_edges")? {
        return Ok(());
    }
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS path_evidence_debug_metadata (
            path_id TEXT PRIMARY KEY,
            metadata_json TEXT NOT NULL
        ) WITHOUT ROWID;
        ",
    )?;
    for (column, decl) in [
        ("exactness", "TEXT"),
        ("confidence", "REAL"),
        ("derived", "INTEGER NOT NULL DEFAULT 0"),
        ("edge_class", "TEXT"),
        ("context", "TEXT"),
        ("provenance_edges_json", "TEXT NOT NULL DEFAULT '[]'"),
    ] {
        if !table_has_column(connection, "path_evidence_edges", column)? {
            connection.execute(
                &format!("ALTER TABLE path_evidence_edges ADD COLUMN {column} {decl}"),
                [],
            )?;
        }
    }
    Ok(())
}

fn prune_unused_identity_dictionary_values(connection: &Connection) -> StoreResult<()> {
    connection.execute_batch(
        "
        DROP TABLE IF EXISTS temp.used_qualified_name_ids;
        DROP TABLE IF EXISTS temp.used_qname_prefix_ids;
        DROP TABLE IF EXISTS temp.used_symbol_ids;

        CREATE TEMP TABLE used_qualified_name_ids (
            id INTEGER PRIMARY KEY
        ) WITHOUT ROWID;
        INSERT OR IGNORE INTO used_qualified_name_ids
        SELECT qualified_name_id FROM entities;
        INSERT OR IGNORE INTO used_qualified_name_ids
        SELECT qualified_name_id FROM template_entities;
        DELETE FROM qualified_name_dict
        WHERE id NOT IN (SELECT id FROM used_qualified_name_ids);

        CREATE TEMP TABLE used_qname_prefix_ids (
            id INTEGER PRIMARY KEY
        ) WITHOUT ROWID;
        INSERT OR IGNORE INTO used_qname_prefix_ids
        SELECT prefix_id FROM qualified_name_dict;
        DELETE FROM qname_prefix_dict
        WHERE id NOT IN (SELECT id FROM used_qname_prefix_ids);

        CREATE TEMP TABLE used_symbol_ids (
            id INTEGER PRIMARY KEY
        ) WITHOUT ROWID;
        INSERT OR IGNORE INTO used_symbol_ids
        SELECT name_id FROM entities;
        INSERT OR IGNORE INTO used_symbol_ids
        SELECT name_id FROM template_entities;
        INSERT OR IGNORE INTO used_symbol_ids
        SELECT suffix_id FROM qualified_name_dict;
        DELETE FROM symbol_dict
        WHERE id NOT IN (SELECT id FROM used_symbol_ids);

        DROP TABLE IF EXISTS temp.used_qualified_name_ids;
        DROP TABLE IF EXISTS temp.used_qname_prefix_ids;
        DROP TABLE IF EXISTS temp.used_symbol_ids;
        ",
    )?;
    Ok(())
}

fn refresh_edges_compat_view(connection: &Connection) -> StoreResult<()> {
    connection.execute_batch(
        "
        DROP VIEW IF EXISTS edges_compat;
        CREATE VIEW edges_compat AS
        SELECT e.*,
               COALESCE(
                   debug.metadata_json,
                   CASE
                       WHEN e.derived != 0 THEN
                           '{\"fact_class\":\"derived_cache\",\"resolution\":\"' || COALESCE(resolution.value, 'derived_from_verified_edges') || '\"}'
                       WHEN (e.flags_bitset & 4) != 0 THEN
                           '{\"heuristic\":true,\"import_kind\":\"dynamic\",\"resolution\":\"' || COALESCE(resolution.value, 'unresolved_dynamic_import') || '\"}'
                       WHEN (e.flags_bitset & 1) != 0 THEN
                           '{\"heuristic\":true,\"resolution\":\"' || COALESCE(resolution.value, 'unresolved_static_heuristic') || '\"}'
                       WHEN relation.value = 'CALLS' AND COALESCE(resolution.value, '') = 'resolved_static_import' THEN
                           '{\"resolution\":\"resolved_static_import\",\"resolver\":\"static_import_call_target\"}'
                       WHEN relation.value = 'SANITIZES' AND COALESCE(resolution.value, '') = 'resolved' THEN
                           '{\"resolution\":\"resolved\",\"resolver\":\"direct_sanitizer_call_argument\"}'
                       WHEN COALESCE(resolution.value, 'not_recorded') != 'not_recorded' THEN
                           '{\"resolution\":\"' || resolution.value || '\"}'
                       ELSE '{}'
                   END
               ) AS metadata_json
        FROM edges e
        LEFT JOIN resolution_kind_dict resolution ON resolution.id = e.resolution_kind_id
        LEFT JOIN relation_kind_dict relation ON relation.id = e.relation_id
        LEFT JOIN edge_debug_metadata debug ON debug.edge_id = e.id_key;
        ",
    )?;
    Ok(())
}

fn refresh_file_instance_view(connection: &Connection) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", "files")?
        || !exists_in_sqlite_master(connection, "table", "path_dict")?
        || !table_has_column(connection, "files", "file_id")?
        || !table_has_column(connection, "files", "content_hash")?
    {
        return Ok(());
    }
    connection.execute_batch(
        "
        DROP VIEW IF EXISTS file_instance;
        CREATE VIEW file_instance AS
        SELECT files.file_id,
               files.path_id,
               path_dict.value AS repo_relative_path,
               files.content_template_id,
               files.content_hash,
               files.mtime_unix_ms,
               files.size_bytes,
               files.language_id,
               files.indexed_at_unix_ms,
               files.metadata_json
        FROM files
        JOIN path_dict ON path_dict.id = files.path_id;
        ",
    )?;
    Ok(())
}

fn refresh_object_id_views(connection: &Connection) -> StoreResult<()> {
    connection.execute_batch(
        "
        DROP VIEW IF EXISTS object_id_lookup;
        DROP VIEW IF EXISTS object_id_debug;

        CREATE VIEW object_id_lookup AS
        SELECT e.id_key AS id,
               COALESCE(debug.value, 'repo://e/' || lower(hex(e.entity_hash))) AS value
        FROM entities e
        LEFT JOIN object_id_dict debug ON debug.id = e.id_key
        UNION ALL
        SELECT debug.id, debug.value
        FROM object_id_dict debug
        WHERE NOT EXISTS (
            SELECT 1 FROM entities e WHERE e.id_key = debug.id
        )
        UNION ALL
        SELECT history.id_key AS id,
               'repo://e/' || lower(hex(history.entity_hash)) AS value
        FROM entity_id_history history
        WHERE NOT EXISTS (
            SELECT 1 FROM entities e WHERE e.id_key = history.id_key
        )
          AND NOT EXISTS (
            SELECT 1 FROM object_id_dict debug WHERE debug.id = history.id_key
        );

        CREATE VIEW object_id_debug AS
        SELECT lookup.id, lookup.value, debug.value_hash, debug.value_len,
               entity.entity_hash
        FROM object_id_lookup lookup
        LEFT JOIN object_id_dict debug ON debug.id = lookup.id
        LEFT JOIN entities entity ON entity.id_key = lookup.id;
        ",
    )?;
    Ok(())
}

fn delete_by_id(connection: &Connection, table: &str, id: &str) -> StoreResult<bool> {
    let sql = format!("DELETE FROM {table} WHERE id = ?1");
    let changed = connection.execute(&sql, [id])?;
    Ok(changed > 0)
}

fn preserve_entity_ids_for_path(connection: &Connection, path_id: i64) -> StoreResult<()> {
    if !exists_in_sqlite_master(connection, "table", "entity_id_history")?
        || !exists_in_sqlite_master(connection, "table", "entities")?
        || !table_has_column(connection, "entities", "entity_hash")?
    {
        return Ok(());
    }
    connection.execute(
        "
        INSERT OR REPLACE INTO entity_id_history (entity_hash, id_key)
        SELECT entity_hash, id_key
        FROM entities
        WHERE path_id = ?1
          AND entity_hash IS NOT NULL
          AND length(entity_hash) > 0
        ",
        [path_id],
    )?;
    Ok(())
}

fn count_rows(connection: &Connection, table: &str) -> StoreResult<u64> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let count: i64 = connection.query_row(&sql, [], |row| row.get(0))?;
    Ok(count.max(0) as u64)
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn stable_text_hash_key(value: &str) -> i64 {
    let mut hash = FNV_OFFSET;
    digest_bytes(&mut hash, value.as_bytes());
    ((hash & 0x7fff_ffff_ffff_ffff).max(1)) as i64
}

fn stable_text_len(value: &str) -> i64 {
    value.chars().count() as i64
}

fn compact_object_key(value: &str) -> i64 {
    if let Some(hash) = canonical_entity_hash(value) {
        return entity_key_from_hash(&hash);
    }
    if canonical_edge_hash(value).is_some() {
        return compact_edge_key(value);
    }
    fallback_object_key(value)
}

fn fallback_object_key(value: &str) -> i64 {
    let mut hash = FNV_OFFSET;
    digest_bytes(&mut hash, value.as_bytes());
    let namespaced = (hash & 0x0fff_ffff_ffff_ffff) | 0x2000_0000_0000_0000;
    namespaced as i64
}

fn entity_key_from_hash(hash: &[u8; 16]) -> i64 {
    let mut high_bytes = [0_u8; 8];
    high_bytes.copy_from_slice(&hash[..8]);
    let high = u64::from_be_bytes(high_bytes);
    let namespaced = (high & 0x0fff_ffff_ffff_ffff) | 0x1000_0000_0000_0000;
    namespaced as i64
}

fn canonical_entity_hash(value: &str) -> Option<[u8; 16]> {
    value
        .strip_prefix("repo://e/")
        .and_then(|hex| decode_hex_16(hex).ok())
}

fn canonical_edge_hash(value: &str) -> Option<[u8; 16]> {
    value
        .strip_prefix("edge://")
        .and_then(|hex| decode_hex_16(hex).ok())
}

fn entity_hash_blob(value: &str) -> Vec<u8> {
    canonical_entity_hash(value)
        .unwrap_or_else(|| fallback_hash_128(value))
        .to_vec()
}

fn fallback_hash_128(value: &str) -> [u8; 16] {
    let mut high = FNV_OFFSET;
    digest_bytes(&mut high, b"object-fallback-high\0");
    digest_bytes(&mut high, value.as_bytes());
    let mut low = 0x9e37_79b1_85eb_ca87_u64;
    digest_bytes(&mut low, b"object-fallback-low\0");
    digest_bytes(&mut low, value.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&high.to_be_bytes());
    bytes[8..].copy_from_slice(&low.to_be_bytes());
    bytes
}

fn decode_hex_16(value: &str) -> Result<[u8; 16], StoreError> {
    if value.len() != 32 {
        return Err(StoreError::Message(format!(
            "expected 32 hex characters, got {}",
            value.len()
        )));
    }
    let mut bytes = [0_u8; 16];
    let raw = value.as_bytes();
    for index in 0..16 {
        let high = hex_value(raw[index * 2])?;
        let low = hex_value(raw[index * 2 + 1])?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

fn hex_value(byte: u8) -> Result<u8, StoreError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(StoreError::Message(format!(
            "invalid hex byte '{}'",
            byte as char
        ))),
    }
}

fn digest_query_rows(connection: &Connection, hash: &mut u64, sql: &str) -> StoreResult<()> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        digest_bytes(hash, row?.as_bytes());
        digest_bytes(hash, b"\n");
    }
    Ok(())
}

fn digest_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

fn format_digest(digest: u64) -> String {
    format!("fnv64:{digest:016x}")
}

fn parse_digest(digest: &str) -> u64 {
    digest
        .strip_prefix("fnv64:")
        .and_then(|value| u64::from_str_radix(value, 16).ok())
        .unwrap_or_else(|| {
            let mut hash = FNV_OFFSET;
            digest_bytes(&mut hash, digest.as_bytes());
            hash
        })
}

fn current_file_graph_digest_u64(connection: &Connection, file_id: &str) -> StoreResult<u64> {
    let Some(path_id) = lookup_path(connection, file_id)? else {
        return Ok(0);
    };
    let mut hash = FNV_OFFSET;
    let mut row_count = 0usize;
    row_count +=
        digest_query_rows_for_path(connection, &mut hash, FILE_ENTITY_DIGEST_SQL, path_id)?;
    row_count += digest_query_rows_for_path(connection, &mut hash, FILE_EDGE_DIGEST_SQL, path_id)?;
    if row_count == 0 {
        Ok(0)
    } else {
        Ok(hash)
    }
}

fn digest_query_rows_for_path(
    connection: &Connection,
    hash: &mut u64,
    sql: &str,
    path_id: i64,
) -> StoreResult<usize> {
    let mut statement = connection.prepare_cached(sql)?;
    let rows = statement.query_map([path_id], |row| row.get::<_, String>(0))?;
    let mut count = 0usize;
    for row in rows {
        digest_bytes(hash, row?.as_bytes());
        digest_bytes(hash, b"\n");
        count += 1;
    }
    Ok(count)
}

fn update_incremental_graph_digest_for_file(
    connection: &Connection,
    file_id: &str,
    updated_at_unix_ms: Option<u64>,
) -> StoreResult<String> {
    let new_file_digest = current_file_graph_digest_u64(connection, file_id)?;
    let old_file_digest = connection
        .query_row(
            "SELECT digest FROM file_graph_digests WHERE file_id = ?1",
            [file_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|digest| parse_digest(&digest))
        .unwrap_or(0);
    let old_repo_digest = connection
        .query_row(
            "SELECT digest FROM repo_graph_digest WHERE id = 'current'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|digest| parse_digest(&digest))
        .unwrap_or(0);
    let new_repo_digest = old_repo_digest ^ old_file_digest ^ new_file_digest;
    if new_file_digest == 0 {
        connection.execute(
            "DELETE FROM file_graph_digests WHERE file_id = ?1",
            [file_id],
        )?;
    } else {
        connection.execute(
            "
            INSERT INTO file_graph_digests (file_id, digest, updated_at_unix_ms)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(file_id) DO UPDATE SET
                digest = excluded.digest,
                updated_at_unix_ms = excluded.updated_at_unix_ms
            ",
            params![file_id, format_digest(new_file_digest), updated_at_unix_ms],
        )?;
    }
    connection.execute(
        "
        INSERT INTO repo_graph_digest (id, digest, updated_at_unix_ms)
        VALUES ('current', ?1, ?2)
        ON CONFLICT(id) DO UPDATE SET
            digest = excluded.digest,
            updated_at_unix_ms = excluded.updated_at_unix_ms
        ",
        params![format_digest(new_repo_digest), updated_at_unix_ms],
    )?;
    Ok(format_digest(new_repo_digest))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeIdStorage {
    Compact,
    ExistingOrCompact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompactFactStorage {
    GenericEdge,
    StructuralRelation,
    CallsiteCallee,
    CallsiteArgument { ordinal: i64 },
}

fn compact_storage_for_relation(relation: RelationKind) -> CompactFactStorage {
    match relation {
        RelationKind::Contains | RelationKind::DefinedIn | RelationKind::Declares => {
            CompactFactStorage::StructuralRelation
        }
        RelationKind::Callee => CompactFactStorage::CallsiteCallee,
        RelationKind::Argument0 => CompactFactStorage::CallsiteArgument { ordinal: 0 },
        RelationKind::Argument1 => CompactFactStorage::CallsiteArgument { ordinal: 1 },
        RelationKind::ArgumentN => CompactFactStorage::CallsiteArgument { ordinal: -1 },
        _ => CompactFactStorage::GenericEdge,
    }
}

fn edge_resolution_kind(edge: &Edge) -> String {
    if let Some(value) = edge
        .metadata
        .get("resolution")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return value.trim().to_ascii_lowercase();
    }
    if edge.derived || edge.exactness == Exactness::DerivedFromVerifiedEdges {
        return "derived_from_verified_edges".to_string();
    }
    match edge.exactness {
        Exactness::StaticHeuristic | Exactness::Inferred => {
            "unresolved_static_heuristic".to_string()
        }
        _ => "not_recorded".to_string(),
    }
}

fn edge_flags_bitset(edge: &Edge, resolution_kind: &str) -> i64 {
    let normalized_resolution = resolution_kind.to_ascii_lowercase();
    let mut flags = 0_i64;
    if edge.exactness == Exactness::StaticHeuristic
        || edge.exactness == Exactness::Inferred
        || edge
            .metadata
            .get("heuristic")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    {
        flags |= EDGE_FLAG_HEURISTIC;
    }
    if normalized_resolution.contains("unresolved")
        || edge
            .metadata
            .get("resolved")
            .and_then(serde_json::Value::as_bool)
            .is_some_and(|resolved| !resolved)
    {
        flags |= EDGE_FLAG_UNRESOLVED;
    }
    if edge
        .metadata
        .get("import_kind")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("dynamic"))
        || normalized_resolution.contains("dynamic")
    {
        flags |= EDGE_FLAG_DYNAMIC_IMPORT;
    }
    if normalized_resolution.contains("static") {
        flags |= EDGE_FLAG_STATIC_RESOLUTION;
    }
    if normalized_resolution.contains("resolved") && !normalized_resolution.contains("unresolved") {
        flags |= EDGE_FLAG_RESOLVED;
    }
    flags
}

fn quantize_confidence(confidence: f64) -> i64 {
    let normalized = if confidence.is_finite() {
        confidence.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (normalized * 1000.0).round() as i64
}

fn compact_edge_storage_key(
    connection: &Connection,
    edge: &Edge,
    storage: EdgeIdStorage,
) -> StoreResult<i64> {
    let compact = compact_edge_key(&edge.id);
    let edge_id = match storage {
        EdgeIdStorage::Compact => return Ok(compact),
        EdgeIdStorage::ExistingOrCompact => {
            if edge_fact_row_exists(connection, compact)? {
                compact
            } else if let Some(existing) = lookup_object_id(connection, &edge.id)? {
                if edge_fact_row_exists(connection, existing)? {
                    existing
                } else {
                    compact
                }
            } else {
                compact
            }
        }
    };
    if let Some(existing) = lookup_object_id(connection, &edge.id)? {
        delete_edge_fact_by_key(connection, existing)?;
    }
    delete_edge_fact_by_key(connection, compact_edge_key(&edge.id))?;
    Ok(edge_id)
}

fn write_structural_relation(
    connection: &Connection,
    dictionary_cache: &mut DictionaryInternCache,
    edge: &Edge,
    storage: EdgeIdStorage,
) -> StoreResult<()> {
    let edge_id = compact_edge_storage_key(connection, edge, storage)?;
    let head_id = intern_object_id_cached(connection, dictionary_cache, &edge.head_id)?;
    let tail_id = intern_object_id_cached(connection, dictionary_cache, &edge.tail_id)?;
    let (_span_path_id, _file_id) = ensure_file_for_path_cached(
        connection,
        dictionary_cache,
        &edge.source_span.repo_relative_path,
        edge.file_hash.as_deref(),
    )?;
    let _ = intern_dict_value_cached(
        connection,
        dictionary_cache,
        "relation_kind_dict",
        &edge.relation.to_string(),
    )?;
    let sql_start = Instant::now();
    connection.execute(
        "DELETE FROM structural_relations WHERE id_key = ?1",
        [edge_id],
    )?;
    update_entity_structural_attributes(connection, edge.relation, head_id, tail_id)?;
    record_sqlite_profile("edge_insert_sql", sql_start.elapsed());
    Ok(())
}

fn write_callsite_callee(
    connection: &Connection,
    dictionary_cache: &mut DictionaryInternCache,
    entity_file_cache: &RefCell<BTreeMap<i64, Vec<String>>>,
    edge: &Edge,
    storage: EdgeIdStorage,
) -> StoreResult<()> {
    let edge_id = compact_edge_storage_key(connection, edge, storage)?;
    let callsite_id = intern_object_id_cached(connection, dictionary_cache, &edge.head_id)?;
    let callee_id = intern_object_id_cached(connection, dictionary_cache, &edge.tail_id)?;
    let relation_id = intern_dict_value_cached(
        connection,
        dictionary_cache,
        "relation_kind_dict",
        &edge.relation.to_string(),
    )?;
    let (span_path_id, file_id) = ensure_file_for_path_cached(
        connection,
        dictionary_cache,
        &edge.source_span.repo_relative_path,
        edge.file_hash.as_deref(),
    )?;
    let extractor_id = intern_dict_value_cached(
        connection,
        dictionary_cache,
        "extractor_dict",
        &edge.extractor,
    )?;
    let exactness_id = intern_dict_value_cached(
        connection,
        dictionary_cache,
        "exactness_dict",
        &edge.exactness.to_string(),
    )?;
    let edge_class_id = intern_dict_value_cached(
        connection,
        dictionary_cache,
        "edge_class_dict",
        &edge.edge_class.to_string(),
    )?;
    let context_id = intern_dict_value_cached(
        connection,
        dictionary_cache,
        "edge_context_dict",
        &edge.context.to_string(),
    )?;
    let caller_id = edge
        .metadata
        .get("caller_id")
        .and_then(serde_json::Value::as_str)
        .map(|value| intern_object_id_cached(connection, dictionary_cache, value))
        .transpose()?;
    let sql_start = Instant::now();
    connection.execute(
        "
        INSERT INTO callsites (
            id_key, callsite_id_key, caller_id_key, relation_id, callee_id_key,
            span_path_id, start_line, start_column, end_line, end_column,
            repo_commit, file_id, extractor_id, confidence, exactness_id,
            edge_class_id, context_id, metadata_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
        ON CONFLICT(id_key) DO UPDATE SET
            callsite_id_key = excluded.callsite_id_key,
            caller_id_key = excluded.caller_id_key,
            relation_id = excluded.relation_id,
            callee_id_key = excluded.callee_id_key,
            span_path_id = excluded.span_path_id,
            start_line = excluded.start_line,
            start_column = excluded.start_column,
            end_line = excluded.end_line,
            end_column = excluded.end_column,
            repo_commit = excluded.repo_commit,
            file_id = excluded.file_id,
            extractor_id = excluded.extractor_id,
            confidence = excluded.confidence,
            exactness_id = excluded.exactness_id,
            edge_class_id = excluded.edge_class_id,
            context_id = excluded.context_id,
            metadata_json = excluded.metadata_json
        ",
        params![
            edge_id,
            callsite_id,
            caller_id,
            relation_id,
            callee_id,
            span_path_id,
            edge.source_span.start_line,
            edge.source_span.start_column,
            edge.source_span.end_line,
            edge.source_span.end_column,
            None::<String>,
            file_id,
            extractor_id,
            edge.confidence,
            exactness_id,
            edge_class_id,
            context_id,
            "{}",
        ],
    )?;
    map_compact_edge_to_files(
        connection,
        entity_file_cache,
        edge,
        callsite_id,
        callee_id,
        storage,
    )?;
    record_sqlite_profile("edge_insert_sql", sql_start.elapsed());
    Ok(())
}

fn write_callsite_argument(
    connection: &Connection,
    dictionary_cache: &mut DictionaryInternCache,
    entity_file_cache: &RefCell<BTreeMap<i64, Vec<String>>>,
    edge: &Edge,
    storage: EdgeIdStorage,
    ordinal: i64,
) -> StoreResult<()> {
    let edge_id = compact_edge_storage_key(connection, edge, storage)?;
    let callsite_id = intern_object_id_cached(connection, dictionary_cache, &edge.head_id)?;
    let argument_id = intern_object_id_cached(connection, dictionary_cache, &edge.tail_id)?;
    let relation_id = intern_dict_value_cached(
        connection,
        dictionary_cache,
        "relation_kind_dict",
        &edge.relation.to_string(),
    )?;
    let (span_path_id, file_id) = ensure_file_for_path_cached(
        connection,
        dictionary_cache,
        &edge.source_span.repo_relative_path,
        edge.file_hash.as_deref(),
    )?;
    let extractor_id = intern_dict_value_cached(
        connection,
        dictionary_cache,
        "extractor_dict",
        &edge.extractor,
    )?;
    let exactness_id = intern_dict_value_cached(
        connection,
        dictionary_cache,
        "exactness_dict",
        &edge.exactness.to_string(),
    )?;
    let edge_class_id = intern_dict_value_cached(
        connection,
        dictionary_cache,
        "edge_class_dict",
        &edge.edge_class.to_string(),
    )?;
    let context_id = intern_dict_value_cached(
        connection,
        dictionary_cache,
        "edge_context_dict",
        &edge.context.to_string(),
    )?;
    let sql_start = Instant::now();
    connection.execute(
        "
        INSERT INTO callsite_args (
            id_key, callsite_id_key, ordinal, relation_id, argument_id_key,
            span_path_id, start_line, start_column, end_line, end_column,
            repo_commit, file_id, extractor_id, confidence, exactness_id,
            edge_class_id, context_id, metadata_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
        ON CONFLICT(id_key) DO UPDATE SET
            callsite_id_key = excluded.callsite_id_key,
            ordinal = excluded.ordinal,
            relation_id = excluded.relation_id,
            argument_id_key = excluded.argument_id_key,
            span_path_id = excluded.span_path_id,
            start_line = excluded.start_line,
            start_column = excluded.start_column,
            end_line = excluded.end_line,
            end_column = excluded.end_column,
            repo_commit = excluded.repo_commit,
            file_id = excluded.file_id,
            extractor_id = excluded.extractor_id,
            confidence = excluded.confidence,
            exactness_id = excluded.exactness_id,
            edge_class_id = excluded.edge_class_id,
            context_id = excluded.context_id,
            metadata_json = excluded.metadata_json
        ",
        params![
            edge_id,
            callsite_id,
            ordinal,
            relation_id,
            argument_id,
            span_path_id,
            edge.source_span.start_line,
            edge.source_span.start_column,
            edge.source_span.end_line,
            edge.source_span.end_column,
            None::<String>,
            file_id,
            extractor_id,
            edge.confidence,
            exactness_id,
            edge_class_id,
            context_id,
            "{}",
        ],
    )?;
    map_compact_edge_to_files(
        connection,
        entity_file_cache,
        edge,
        callsite_id,
        argument_id,
        storage,
    )?;
    record_sqlite_profile("edge_insert_sql", sql_start.elapsed());
    Ok(())
}

fn map_compact_edge_to_files(
    connection: &Connection,
    entity_file_cache: &RefCell<BTreeMap<i64, Vec<String>>>,
    edge: &Edge,
    head_id_key: i64,
    tail_id_key: i64,
    storage: EdgeIdStorage,
) -> StoreResult<()> {
    if storage == EdgeIdStorage::Compact {
        insert_edge_file_map_after_file_delete(
            connection,
            &edge.source_span.repo_relative_path,
            &edge.id,
        )?;
    } else {
        map_edge_to_file(connection, &edge.source_span.repo_relative_path, &edge.id)?;
    }
    map_edge_to_entity_files_cached(
        connection,
        entity_file_cache,
        &edge.id,
        head_id_key,
        tail_id_key,
    )?;
    if storage == EdgeIdStorage::Compact {
        insert_source_span_file_map_after_file_delete(
            connection,
            &edge.source_span.repo_relative_path,
            &edge.id,
        )?;
    } else {
        map_source_span_to_file(connection, &edge.source_span.repo_relative_path, &edge.id)?;
    }
    Ok(())
}

fn update_entity_structural_attributes(
    connection: &Connection,
    relation: RelationKind,
    head_id: i64,
    tail_id: i64,
) -> StoreResult<()> {
    match relation {
        RelationKind::Contains => {
            connection.execute(
                "UPDATE entities
                 SET parent_id = COALESCE(parent_id, ?1),
                     scope_id = COALESCE(scope_id, ?1),
                     structural_flags = structural_flags | ?2
                 WHERE id_key = ?3",
                params![head_id, ENTITY_STRUCTURAL_FLAG_CONTAINED_BY_PARENT, tail_id],
            )?;
        }
        RelationKind::Declares => {
            connection.execute(
                "UPDATE entities
                 SET parent_id = COALESCE(parent_id, ?1),
                     scope_id = COALESCE(scope_id, ?1),
                     structural_flags = structural_flags | ?2
                 WHERE id_key = ?3",
                params![head_id, ENTITY_STRUCTURAL_FLAG_DECLARED_BY_PARENT, tail_id],
            )?;
        }
        RelationKind::DefinedIn => {
            connection.execute(
                "UPDATE entities
                 SET parent_id = COALESCE(parent_id, ?1),
                     scope_id = COALESCE(scope_id, ?1),
                     structural_flags = structural_flags | ?2
                 WHERE id_key = ?3",
                params![tail_id, ENTITY_STRUCTURAL_FLAG_DEFINED_IN_PARENT, head_id],
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn delete_edge_fact_by_key(connection: &Connection, id_key: i64) -> StoreResult<usize> {
    let mut changed = 0usize;
    changed += connection.execute(
        "DELETE FROM edge_debug_metadata WHERE edge_id = ?1",
        [id_key],
    )?;
    for table in [
        "edges",
        "structural_relations",
        "callsites",
        "callsite_args",
    ] {
        changed +=
            connection.execute(&format!("DELETE FROM {table} WHERE id_key = ?1"), [id_key])?;
    }
    Ok(changed)
}

fn delete_compact_fact_by_key(connection: &Connection, id_key: i64) -> StoreResult<usize> {
    let mut changed = 0usize;
    changed += connection.execute(
        "DELETE FROM edge_debug_metadata WHERE edge_id = ?1",
        [id_key],
    )?;
    for table in ["structural_relations", "callsites", "callsite_args"] {
        changed +=
            connection.execute(&format!("DELETE FROM {table} WHERE id_key = ?1"), [id_key])?;
    }
    Ok(changed)
}

fn delete_compact_facts_for_path(connection: &Connection, path_id: i64) -> StoreResult<()> {
    connection.execute(
        "
        DELETE FROM structural_relations
        WHERE span_path_id = ?1
           OR head_id_key IN (SELECT id_key FROM entities WHERE path_id = ?1)
           OR tail_id_key IN (SELECT id_key FROM entities WHERE path_id = ?1)
        ",
        [path_id],
    )?;
    connection.execute(
        "
        DELETE FROM callsites
        WHERE span_path_id = ?1
           OR callsite_id_key IN (SELECT id_key FROM entities WHERE path_id = ?1)
           OR callee_id_key IN (SELECT id_key FROM entities WHERE path_id = ?1)
           OR caller_id_key IN (SELECT id_key FROM entities WHERE path_id = ?1)
        ",
        [path_id],
    )?;
    connection.execute(
        "
        DELETE FROM callsite_args
        WHERE span_path_id = ?1
           OR callsite_id_key IN (SELECT id_key FROM entities WHERE path_id = ?1)
           OR argument_id_key IN (SELECT id_key FROM entities WHERE path_id = ?1)
        ",
        [path_id],
    )?;
    Ok(())
}

fn edge_storage_key(connection: &Connection, id: &str) -> StoreResult<i64> {
    let compact = compact_edge_key(id);
    if edge_fact_row_exists(connection, compact)? {
        return Ok(compact);
    }
    if let Some(existing) = lookup_object_id(connection, id)? {
        if edge_fact_row_exists(connection, existing)? {
            return Ok(existing);
        }
    }
    intern_object_id(connection, id)
}

fn edge_lookup_key(connection: &Connection, id: &str) -> StoreResult<Option<i64>> {
    let compact = compact_edge_key(id);
    if edge_fact_row_exists(connection, compact)? {
        return Ok(Some(compact));
    }
    if let Some(existing) = lookup_object_id(connection, id)? {
        if edge_fact_row_exists(connection, existing)? {
            return Ok(Some(existing));
        }
    }
    Ok(None)
}

fn entity_row_exists(connection: &Connection, id_key: i64) -> StoreResult<bool> {
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM entities WHERE id_key = ?1)",
        [id_key],
        |row| row.get::<_, bool>(0),
    )?;
    Ok(exists)
}

fn edge_fact_row_exists(connection: &Connection, id_key: i64) -> StoreResult<bool> {
    let exists = connection.query_row(
        "
        SELECT EXISTS(
            SELECT 1 FROM edges WHERE id_key = ?1
            UNION ALL SELECT 1 FROM structural_relations WHERE id_key = ?1
            UNION ALL SELECT 1 FROM callsites WHERE id_key = ?1
            UNION ALL SELECT 1 FROM callsite_args WHERE id_key = ?1
        )
        ",
        [id_key],
        |row| row.get::<_, bool>(0),
    )?;
    Ok(exists)
}

fn compact_fact_row_count(connection: &Connection) -> StoreResult<u64> {
    Ok(count_rows(connection, "structural_relations")?
        + count_rows(connection, "callsites")?
        + count_rows(connection, "callsite_args")?)
}

fn compact_edge_key(id: &str) -> i64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let positive = (hash & 0x7fff_ffff_ffff_ffff).max(1);
    -(positive as i64)
}

fn validate_sqlite_check_rows(connection: &Connection, pragma: &str) -> StoreResult<()> {
    let sql = format!("PRAGMA {pragma}");
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(row?);
    }
    if messages.len() == 1 && messages.first().is_some_and(|message| message == "ok") {
        Ok(())
    } else {
        Err(StoreError::Message(format!(
            "SQLite {pragma} failed: {}",
            messages.join("; ")
        )))
    }
}

fn normalize_edge_for_storage(edge: &Edge) -> StoreResult<Edge> {
    validate_edge_derivation(edge)?;
    let mut normalized = edge.clone();
    if storage_edge_has_unresolved_resolution(&normalized)
        && storage_exactness_is_proof_grade(normalized.exactness)
    {
        normalized.metadata.insert(
            "exactness_downgraded_from".to_string(),
            normalized.exactness.to_string().into(),
        );
        normalized.exactness = Exactness::StaticHeuristic;
        normalized.confidence = normalized.confidence.min(0.55);
    }
    normalize_edge_classification(&mut normalized);
    validate_edge_derivation(&normalized)?;
    Ok(normalized)
}

fn validate_edge_derivation(edge: &Edge) -> StoreResult<()> {
    if storage_relation_is_derived_cache(edge.relation) && !edge.derived {
        return Err(StoreError::Message(format!(
            "derived/cache relation {} must be stored with derived=true",
            edge.relation
        )));
    }
    if (edge.derived || storage_relation_is_derived_cache(edge.relation))
        && edge.provenance_edges.is_empty()
    {
        return Err(StoreError::Message(format!(
            "derived edge {} must include provenance_edges",
            edge.id
        )));
    }
    Ok(())
}

fn validate_derived_closure_edge(edge: &DerivedClosureEdge) -> StoreResult<()> {
    if edge.provenance_edges.is_empty() {
        return Err(StoreError::Message(format!(
            "derived edge {} must include provenance_edges",
            edge.id
        )));
    }
    Ok(())
}

fn storage_relation_is_derived_cache(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::MayMutate
            | RelationKind::MayRead
            | RelationKind::ApiReaches
            | RelationKind::AsyncReaches
            | RelationKind::SchemaImpact
    )
}

fn storage_exactness_is_proof_grade(exactness: Exactness) -> bool {
    matches!(
        exactness,
        Exactness::Exact
            | Exactness::CompilerVerified
            | Exactness::LspVerified
            | Exactness::ParserVerified
    )
}

fn storage_edge_has_unresolved_resolution(edge: &Edge) -> bool {
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

fn intern_entity_object_id(connection: &Connection, value: &str) -> StoreResult<i64> {
    if let Some(hash) = canonical_entity_hash(value) {
        if let Some(existing) = lookup_entity_id_by_hash(connection, &hash)? {
            return Ok(existing);
        }
        if let Some(existing) = lookup_hashed_dict_value(connection, "object_id_dict", value)? {
            return Ok(existing);
        }
        if let Some(existing) = lookup_entity_id_history(connection, &hash)? {
            return Ok(existing);
        }
        return next_dense_entity_id(connection);
    }
    intern_object_id(connection, value)
}

fn lookup_entity_id_by_hash(connection: &Connection, hash: &[u8; 16]) -> StoreResult<Option<i64>> {
    if !exists_in_sqlite_master(connection, "table", "entities")?
        || !table_has_column(connection, "entities", "entity_hash")?
    {
        return Ok(None);
    }
    connection
        .query_row(
            "SELECT id_key FROM entities WHERE entity_hash = ?1 LIMIT 1",
            [hash.as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(StoreError::from)
}

fn lookup_entity_id_history(connection: &Connection, hash: &[u8; 16]) -> StoreResult<Option<i64>> {
    if !exists_in_sqlite_master(connection, "table", "entity_id_history")? {
        return Ok(None);
    }
    connection
        .query_row(
            "SELECT id_key FROM entity_id_history WHERE entity_hash = ?1 LIMIT 1",
            [hash.as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(StoreError::from)
}

fn next_dense_entity_id(connection: &Connection) -> StoreResult<i64> {
    connection
        .query_row(
            "
            SELECT MAX(max_id) + 1
            FROM (
                SELECT COALESCE(MAX(id_key), 0) AS max_id FROM entities
                UNION ALL
                SELECT COALESCE(MAX(id_key), 0) AS max_id FROM entity_id_history
            )
            ",
            [],
            |row| row.get(0),
        )
        .map_err(StoreError::from)
}

fn allocate_entity_id_history(connection: &Connection, hash: &[u8; 16]) -> StoreResult<i64> {
    if let Some(existing) = lookup_entity_id_history(connection, hash)? {
        return Ok(existing);
    }
    let id_key = next_dense_entity_id(connection)?;
    connection.execute(
        "
        INSERT INTO entity_id_history (entity_hash, id_key)
        VALUES (?1, ?2)
        ON CONFLICT(entity_hash) DO UPDATE SET
            id_key = entity_id_history.id_key
        ",
        params![hash.as_slice(), id_key],
    )?;
    Ok(id_key)
}

fn intern_object_id(connection: &Connection, value: &str) -> StoreResult<i64> {
    if let Some(hash) = canonical_entity_hash(value) {
        if let Some(existing) = lookup_entity_id_by_hash(connection, &hash)? {
            return Ok(existing);
        }
        if let Some(existing) = lookup_hashed_dict_value(connection, "object_id_dict", value)? {
            return Ok(existing);
        }
        return allocate_entity_id_history(connection, &hash);
    }
    let id = compact_object_key(value);
    if canonical_edge_hash(value).is_some() {
        return Ok(id);
    }
    let existing = lookup_hashed_dict_value(connection, "object_id_dict", value)?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let conflicting = connection
        .query_row(
            "SELECT value FROM object_id_dict WHERE id = ?1",
            [id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(conflicting) = conflicting {
        if conflicting != value {
            return Err(StoreError::Message(format!(
                "object id key collision between {conflicting} and {value}"
            )));
        }
        return Ok(id);
    }
    connection.execute(
        "
        INSERT INTO object_id_dict (id, value, value_hash, value_len)
        VALUES (?1, ?2, ?3, ?4)
        ",
        params![
            id,
            value,
            stable_text_hash_key(value),
            stable_text_len(value)
        ],
    )?;
    Ok(id)
}

fn lookup_object_id(connection: &Connection, value: &str) -> StoreResult<Option<i64>> {
    if let Some(hash) = canonical_entity_hash(value) {
        if let Some(existing) = lookup_entity_id_by_hash(connection, &hash)? {
            return Ok(Some(existing));
        }
        if let Some(existing) = lookup_hashed_dict_value(connection, "object_id_dict", value)? {
            return Ok(Some(existing));
        }
        return Ok(None);
    }
    if canonical_edge_hash(value).is_some() {
        return Ok(Some(compact_object_key(value)));
    }
    if let Some(existing) = lookup_hashed_dict_value(connection, "object_id_dict", value)? {
        return Ok(Some(existing));
    }
    Ok(Some(compact_object_key(value)))
}

fn intern_path(connection: &Connection, value: &str) -> StoreResult<i64> {
    intern_dict_value(connection, "path_dict", value)
}

fn lookup_path(connection: &Connection, value: &str) -> StoreResult<Option<i64>> {
    lookup_dict_value(connection, "path_dict", value)
}

fn ensure_content_template_for_path_id(
    connection: &Connection,
    content_hash: &str,
    language_id: Option<i64>,
    canonical_path_id: i64,
) -> StoreResult<i64> {
    let language_id = language_id.unwrap_or(0);
    let template_key = format!(
        "content-template\0{}\0{}\0{}",
        content_hash, language_id, CONTENT_TEMPLATE_EXTRACTION_VERSION
    );
    let content_template_id = stable_text_hash_key(&template_key);
    connection.execute(
        "
        INSERT OR IGNORE INTO source_content_template (
            content_template_id, content_hash, language_id,
            extraction_version, canonical_path_id, metadata_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, '{}')
        ",
        params![
            content_template_id,
            content_hash,
            language_id,
            CONTENT_TEMPLATE_EXTRACTION_VERSION,
            canonical_path_id,
        ],
    )?;
    let stored_id = connection.query_row(
        "
        SELECT content_template_id
        FROM source_content_template
        WHERE content_hash = ?1
          AND language_id = ?2
          AND extraction_version = ?3
        ",
        params![
            content_hash,
            language_id,
            CONTENT_TEMPLATE_EXTRACTION_VERSION
        ],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(stored_id)
}

fn ensure_file_for_path(
    connection: &Connection,
    repo_relative_path: &str,
    file_hash: Option<&str>,
) -> StoreResult<(i64, i64)> {
    let path_id = intern_path(connection, repo_relative_path)?;
    let language_id = None;
    let content_template_id = file_hash
        .filter(|hash| !hash.is_empty())
        .map(|hash| ensure_content_template_for_path_id(connection, hash, language_id, path_id))
        .transpose()?;
    let existing_file_id = connection
        .query_row(
            "SELECT file_id FROM files WHERE path_id = ?1",
            [path_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(file_id) = existing_file_id {
        if let Some(hash) = file_hash.filter(|hash| !hash.is_empty()) {
            connection.execute(
                "UPDATE files
                 SET content_hash = ?1,
                     content_template_id = COALESCE(content_template_id, ?2)
                 WHERE file_id = ?3 AND content_hash = ''",
                params![hash, content_template_id, file_id],
            )?;
        }
        return Ok((path_id, file_id));
    }
    connection.execute(
        "
        INSERT INTO files (
            file_id, path_id, content_hash, mtime_unix_ms, size_bytes,
            language_id, indexed_at_unix_ms, content_template_id, metadata_json
        ) VALUES (?1, ?1, ?2, NULL, 0, NULL, NULL, ?3, '{}')
        ",
        params![path_id, file_hash.unwrap_or_default(), content_template_id],
    )?;
    Ok((path_id, path_id))
}

fn entity_ids_for_path(connection: &Connection, path_id: i64) -> StoreResult<Vec<String>> {
    let mut statement = connection.prepare_cached(
        "SELECT object_id_lookup.value
         FROM entities
         JOIN object_id_lookup ON object_id_lookup.id = entities.id_key
         WHERE entities.path_id = ?1",
    )?;
    let rows = statement.query_map([path_id], |row| row.get(0))?;
    collect_rows(rows)
}

fn entity_ids_for_file_map(connection: &Connection, file_id: &str) -> StoreResult<Vec<String>> {
    let mut statement =
        connection.prepare_cached("SELECT entity_id FROM file_entities WHERE file_id = ?1")?;
    let rows = statement.query_map([file_id], |row| row.get(0))?;
    collect_rows(rows)
}

fn edge_ids_touching_path(connection: &Connection, path_id: i64) -> StoreResult<Vec<String>> {
    let mut statement = connection.prepare_cached(
        "
        WITH edge_facts AS (
            SELECT id_key, head_id_key, tail_id_key, span_path_id FROM edges
            UNION ALL
            SELECT id_key, head_id_key, tail_id_key, span_path_id FROM structural_relations
            UNION ALL
            SELECT id_key, callsite_id_key AS head_id_key, callee_id_key AS tail_id_key, span_path_id FROM callsites
            UNION ALL
            SELECT id_key, callsite_id_key AS head_id_key, argument_id_key AS tail_id_key, span_path_id FROM callsite_args
        )
        SELECT COALESCE(object_id_lookup.value, 'edge-key:' || edge_facts.id_key)
        FROM edge_facts
        LEFT JOIN object_id_lookup ON object_id_lookup.id = edge_facts.id_key
        WHERE edge_facts.head_id_key IN (
            SELECT id_key FROM entities WHERE path_id = ?1
        )
           OR edge_facts.tail_id_key IN (
            SELECT id_key FROM entities WHERE path_id = ?1
        )
           OR edge_facts.span_path_id = ?1",
    )?;
    let rows = statement.query_map([path_id], |row| row.get(0))?;
    collect_rows(rows)
}

fn edge_ids_for_file_map(connection: &Connection, file_id: &str) -> StoreResult<Vec<String>> {
    let mut statement =
        connection.prepare_cached("SELECT edge_id FROM file_edges WHERE file_id = ?1")?;
    let rows = statement.query_map([file_id], |row| row.get(0))?;
    collect_rows(rows)
}

fn source_span_ids_for_file_map(
    connection: &Connection,
    file_id: &str,
) -> StoreResult<Vec<String>> {
    let mut statement =
        connection.prepare_cached("SELECT span_id FROM file_source_spans WHERE file_id = ?1")?;
    let rows = statement.query_map([file_id], |row| row.get(0))?;
    collect_rows(rows)
}

fn map_entity_to_file(
    connection: &Connection,
    repo_relative_path: &str,
    entity_id: &str,
) -> StoreResult<()> {
    let file_id = normalize_repo_relative_path(repo_relative_path);
    connection.execute(
        "DELETE FROM file_entities WHERE entity_id = ?1 AND file_id != ?2",
        params![entity_id, &file_id],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO file_entities (file_id, entity_id) VALUES (?1, ?2)",
        params![&file_id, entity_id],
    )?;
    Ok(())
}

fn insert_entity_file_map_after_file_delete(
    connection: &Connection,
    repo_relative_path: &str,
    entity_id: &str,
) -> StoreResult<()> {
    let file_id = normalize_repo_relative_path(repo_relative_path);
    connection.execute(
        "INSERT OR IGNORE INTO file_entities (file_id, entity_id) VALUES (?1, ?2)",
        params![&file_id, entity_id],
    )?;
    Ok(())
}

fn map_edge_to_file(
    connection: &Connection,
    repo_relative_path: &str,
    edge_id: &str,
) -> StoreResult<()> {
    let file_id = normalize_repo_relative_path(repo_relative_path);
    connection.execute(
        "DELETE FROM file_edges WHERE edge_id = ?1 AND file_id != ?2",
        params![edge_id, &file_id],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO file_edges (file_id, edge_id) VALUES (?1, ?2)",
        params![&file_id, edge_id],
    )?;
    Ok(())
}

fn insert_edge_file_map_after_file_delete(
    connection: &Connection,
    repo_relative_path: &str,
    edge_id: &str,
) -> StoreResult<()> {
    let file_id = normalize_repo_relative_path(repo_relative_path);
    connection.execute(
        "INSERT OR IGNORE INTO file_edges (file_id, edge_id) VALUES (?1, ?2)",
        params![&file_id, edge_id],
    )?;
    Ok(())
}

fn map_edge_to_entity_files(
    connection: &Connection,
    edge_id: &str,
    head_id_key: i64,
    tail_id_key: i64,
) -> StoreResult<()> {
    connection.execute(
        "
        INSERT OR IGNORE INTO file_edges (file_id, edge_id)
        SELECT path.value, ?1
        FROM entities
        JOIN path_dict path ON path.id = entities.path_id
        WHERE entities.id_key IN (?2, ?3)
        ",
        params![edge_id, head_id_key, tail_id_key],
    )?;
    Ok(())
}

fn map_edge_to_entity_files_cached(
    connection: &Connection,
    entity_file_cache: &RefCell<BTreeMap<i64, Vec<String>>>,
    edge_id: &str,
    head_id_key: i64,
    tail_id_key: i64,
) -> StoreResult<()> {
    let mut file_ids = BTreeSet::new();
    for entity_id_key in [head_id_key, tail_id_key] {
        for file_id in entity_file_ids_cached(connection, entity_file_cache, entity_id_key)? {
            file_ids.insert(file_id);
        }
    }
    let mut insert = connection
        .prepare_cached("INSERT OR IGNORE INTO file_edges (file_id, edge_id) VALUES (?1, ?2)")?;
    for file_id in file_ids {
        insert.execute(params![file_id, edge_id])?;
    }
    Ok(())
}

fn entity_file_ids_cached(
    connection: &Connection,
    entity_file_cache: &RefCell<BTreeMap<i64, Vec<String>>>,
    entity_id_key: i64,
) -> StoreResult<Vec<String>> {
    if let Some(cached) = entity_file_cache.borrow().get(&entity_id_key).cloned() {
        return Ok(cached);
    }
    let mut statement = connection.prepare_cached(
        "
        SELECT path.value
        FROM entities
        JOIN path_dict path ON path.id = entities.path_id
        WHERE entities.id_key = ?1
        ",
    )?;
    let rows = statement.query_map([entity_id_key], |row| row.get(0))?;
    let file_ids = collect_rows(rows)?;
    entity_file_cache
        .borrow_mut()
        .insert(entity_id_key, file_ids.clone());
    Ok(file_ids)
}

fn map_source_span_to_file(
    connection: &Connection,
    repo_relative_path: &str,
    span_id: &str,
) -> StoreResult<()> {
    let file_id = normalize_repo_relative_path(repo_relative_path);
    connection.execute(
        "DELETE FROM file_source_spans WHERE span_id = ?1 AND file_id != ?2",
        params![span_id, &file_id],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO file_source_spans (file_id, span_id) VALUES (?1, ?2)",
        params![&file_id, span_id],
    )?;
    Ok(())
}

fn insert_source_span_file_map_after_file_delete(
    connection: &Connection,
    repo_relative_path: &str,
    span_id: &str,
) -> StoreResult<()> {
    let file_id = normalize_repo_relative_path(repo_relative_path);
    connection.execute(
        "INSERT OR IGNORE INTO file_source_spans (file_id, span_id) VALUES (?1, ?2)",
        params![&file_id, span_id],
    )?;
    Ok(())
}

fn cleanup_file_reverse_maps(
    connection: &Connection,
    file_id: &str,
    stale_entity_ids: &[String],
    stale_edge_ids: &[String],
) -> StoreResult<()> {
    connection.execute("DELETE FROM file_entities WHERE file_id = ?1", [file_id])?;
    connection.execute("DELETE FROM file_edges WHERE file_id = ?1", [file_id])?;
    connection.execute(
        "DELETE FROM file_source_spans WHERE file_id = ?1",
        [file_id],
    )?;
    connection.execute(
        "DELETE FROM file_path_evidence WHERE file_id = ?1",
        [file_id],
    )?;
    connection.execute("DELETE FROM file_fts_rows WHERE file_id = ?1", [file_id])?;

    let mut delete_entity_maps =
        connection.prepare_cached("DELETE FROM file_entities WHERE entity_id = ?1")?;
    for entity_id in stale_entity_ids {
        delete_entity_maps.execute([entity_id])?;
    }
    let mut delete_edge_maps =
        connection.prepare_cached("DELETE FROM file_edges WHERE edge_id = ?1")?;
    let mut delete_span_maps =
        connection.prepare_cached("DELETE FROM file_source_spans WHERE span_id = ?1")?;
    for edge_id in stale_edge_ids {
        delete_edge_maps.execute([edge_id])?;
        delete_span_maps.execute([edge_id])?;
    }
    Ok(())
}

fn delete_sidecar_facts_for_file(
    connection: &Connection,
    repo_relative_path: &str,
) -> StoreResult<()> {
    connection.execute(
        "DELETE FROM heuristic_edges WHERE source_span_path = ?1",
        [repo_relative_path],
    )?;
    connection.execute(
        "DELETE FROM unresolved_references WHERE source_span_path = ?1",
        [repo_relative_path],
    )?;
    connection.execute(
        "DELETE FROM static_references
         WHERE repo_relative_path = ?1 OR source_span_path = ?1",
        [repo_relative_path],
    )?;
    connection.execute(
        "DELETE FROM extraction_warnings WHERE repo_relative_path = ?1",
        [repo_relative_path],
    )?;
    Ok(())
}

fn delete_edges_by_logical_ids(connection: &Connection, edge_ids: &[String]) -> StoreResult<()> {
    for edge_id in edge_ids {
        if let Some(id_key) = edge_lookup_key(connection, edge_id)? {
            delete_edge_fact_by_key(connection, id_key)?;
        }
    }
    Ok(())
}

fn delete_entities_by_logical_ids(
    connection: &Connection,
    entity_ids: &[String],
) -> StoreResult<()> {
    let mut delete_entity = connection.prepare_cached("DELETE FROM entities WHERE id_key = ?1")?;
    for entity_id in entity_ids {
        if let Some(id_key) = lookup_object_id(connection, entity_id)? {
            delete_entity.execute([id_key])?;
        }
    }
    Ok(())
}

fn delete_source_spans_by_logical_ids(
    connection: &Connection,
    span_ids: &[String],
) -> StoreResult<()> {
    let mut delete_span =
        connection.prepare_cached("DELETE FROM source_spans WHERE id_key = ?1")?;
    for span_id in span_ids {
        if let Some(id_key) = lookup_object_id(connection, span_id)? {
            delete_span.execute([id_key])?;
        }
    }
    Ok(())
}

fn delete_cached_path_evidence_for_file(
    connection: &Connection,
    repo_relative_path: &str,
    stale_entity_ids: &[String],
    stale_edge_ids: &[String],
    use_reverse_maps: bool,
) -> StoreResult<()> {
    let mut mapped_path_ids = BTreeSet::<String>::new();
    {
        let mut statement = connection.prepare_cached(
            "SELECT path_id FROM file_path_evidence WHERE file_id = ?1
             UNION
             SELECT path_id FROM path_evidence_files WHERE file_id = ?1",
        )?;
        let rows = statement.query_map([repo_relative_path], |row| row.get::<_, String>(0))?;
        for row in rows {
            mapped_path_ids.insert(row?);
        }
    }
    let mut select_entity_refs = connection
        .prepare_cached("SELECT path_id FROM path_evidence_symbols WHERE entity_id = ?1")?;
    for entity_id in stale_entity_ids {
        let rows = select_entity_refs.query_map([entity_id], |row| row.get::<_, String>(0))?;
        for row in rows {
            mapped_path_ids.insert(row?);
        }
    }
    let mut select_edge_refs =
        connection.prepare_cached("SELECT path_id FROM path_evidence_edges WHERE edge_id = ?1")?;
    for edge_id in stale_edge_ids {
        let rows = select_edge_refs.query_map([edge_id], |row| row.get::<_, String>(0))?;
        for row in rows {
            mapped_path_ids.insert(row?);
        }
    }
    delete_path_evidence_ids(connection, mapped_path_ids.iter().map(String::as_str))?;

    if use_reverse_maps {
        return Ok(());
    }

    let path_pattern = like_contains_pattern(repo_relative_path);
    connection.execute(
        "DELETE FROM path_evidence
         WHERE source_spans_json LIKE ?1 ESCAPE '\\'
            OR metadata_json LIKE ?1 ESCAPE '\\'",
        [&path_pattern],
    )?;

    let mut delete_entity_refs = connection.prepare_cached(
        "DELETE FROM path_evidence
         WHERE source = ?1
            OR target = ?1
            OR edges_json LIKE ?2 ESCAPE '\\'
            OR metadata_json LIKE ?2 ESCAPE '\\'",
    )?;
    for entity_id in stale_entity_ids {
        let pattern = like_contains_pattern(entity_id);
        delete_entity_refs.execute(params![entity_id, pattern])?;
    }

    let mut delete_edge_refs = connection.prepare_cached(
        "DELETE FROM path_evidence
         WHERE edges_json LIKE ?1 ESCAPE '\\'
            OR metadata_json LIKE ?1 ESCAPE '\\'",
    )?;
    for edge_id in stale_edge_ids {
        let pattern = like_contains_pattern(edge_id);
        delete_edge_refs.execute([pattern])?;
    }

    cleanup_orphan_path_evidence_materialized_rows(connection)?;
    Ok(())
}

fn delete_path_evidence_ids<'a>(
    connection: &Connection,
    path_ids: impl IntoIterator<Item = &'a str>,
) -> StoreResult<()> {
    let mut delete_path = connection.prepare_cached("DELETE FROM path_evidence WHERE id = ?1")?;
    for path_id in path_ids {
        delete_path_evidence_materialized_rows(connection, path_id)?;
        delete_path.execute([path_id])?;
    }
    Ok(())
}

fn cleanup_orphan_path_evidence_materialized_rows(connection: &Connection) -> StoreResult<()> {
    for table in [
        "path_evidence_lookup",
        "path_evidence_edges",
        "path_evidence_symbols",
        "path_evidence_tests",
        "path_evidence_files",
        "file_path_evidence",
    ] {
        connection.execute(
            &format!("DELETE FROM {table} WHERE path_id NOT IN (SELECT id FROM path_evidence)"),
            [],
        )?;
    }
    Ok(())
}

fn delete_cached_derived_edges_for_file(
    connection: &Connection,
    stale_entity_ids: &[String],
    stale_edge_ids: &[String],
) -> StoreResult<()> {
    let mut delete_entity_refs = connection.prepare_cached(
        "DELETE FROM derived_edges
         WHERE head_id = ?1
            OR tail_id = ?1
            OR metadata_json LIKE ?2 ESCAPE '\\'",
    )?;
    for entity_id in stale_entity_ids {
        let pattern = like_contains_pattern(entity_id);
        delete_entity_refs.execute(params![entity_id, pattern])?;
    }

    let mut delete_edge_refs = connection.prepare_cached(
        "DELETE FROM derived_edges
         WHERE provenance_edges_json LIKE ?1 ESCAPE '\\'
            OR metadata_json LIKE ?1 ESCAPE '\\'",
    )?;
    for edge_id in stale_edge_ids {
        let pattern = like_contains_pattern(edge_id);
        delete_edge_refs.execute([pattern])?;
    }

    Ok(())
}

fn like_contains_pattern(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn intern_symbol(connection: &Connection, value: &str) -> StoreResult<i64> {
    validate_identity_component("symbol", value, MAX_SYMBOL_VALUE_BYTES)?;
    intern_dict_value(connection, "symbol_dict", value)
}

fn lookup_symbol(connection: &Connection, value: &str) -> StoreResult<Option<i64>> {
    lookup_dict_value(connection, "symbol_dict", value)
}

fn intern_qualified_name(connection: &Connection, value: &str) -> StoreResult<i64> {
    validate_qualified_name_component(value)?;
    let start = Instant::now();
    let (prefix, suffix) = split_qualified_name(value);
    let prefix_id = intern_dict_value(connection, "qname_prefix_dict", prefix)?;
    let suffix_id = intern_symbol(connection, suffix)?;
    let changed = connection.execute(
        "INSERT OR IGNORE INTO qualified_name_dict (prefix_id, suffix_id, value) VALUES (?1, ?2, NULL)",
        params![prefix_id, suffix_id],
    )?;
    if changed > 0 {
        record_sqlite_profile("dictionary_lookup_insert", start.elapsed());
        return Ok(connection.last_insert_rowid());
    }
    let result = lookup_qualified_name(connection, value)?.ok_or_else(|| {
        StoreError::Message(format!(
            "failed to intern qualified name: {}",
            compact_error_value(value)
        ))
    });
    record_sqlite_profile("dictionary_lookup_insert", start.elapsed());
    result
}

fn lookup_qualified_name(connection: &Connection, value: &str) -> StoreResult<Option<i64>> {
    if value.len() > MAX_QUALIFIED_NAME_BYTES {
        return Ok(None);
    }
    let (prefix, suffix) = split_qualified_name(value);
    if prefix.len() > MAX_QNAME_PREFIX_BYTES || suffix.len() > MAX_SYMBOL_VALUE_BYTES {
        return Ok(None);
    }
    let Some(prefix_id) = lookup_dict_value(connection, "qname_prefix_dict", prefix)? else {
        return Ok(None);
    };
    let Some(suffix_id) = lookup_symbol(connection, suffix)? else {
        return Ok(None);
    };
    connection
        .query_row(
            "SELECT id FROM qualified_name_dict WHERE prefix_id = ?1 AND suffix_id = ?2",
            params![prefix_id, suffix_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(StoreError::from)
}

fn intern_entity_kind(connection: &Connection, value: &str) -> StoreResult<i64> {
    intern_dict_value(connection, "entity_kind_dict", value)
}

fn intern_relation_kind(connection: &Connection, value: &str) -> StoreResult<i64> {
    intern_dict_value(connection, "relation_kind_dict", value)
}

fn lookup_relation_kind(connection: &Connection, value: &str) -> StoreResult<Option<i64>> {
    lookup_dict_value(connection, "relation_kind_dict", value)
}

fn intern_extractor(connection: &Connection, value: &str) -> StoreResult<i64> {
    intern_dict_value(connection, "extractor_dict", value)
}

fn intern_exactness(connection: &Connection, value: &str) -> StoreResult<i64> {
    intern_dict_value(connection, "exactness_dict", value)
}

fn intern_resolution_kind(connection: &Connection, value: &str) -> StoreResult<i64> {
    intern_dict_value(connection, "resolution_kind_dict", value)
}

fn intern_edge_class(connection: &Connection, value: &str) -> StoreResult<i64> {
    intern_dict_value(connection, "edge_class_dict", value)
}

fn intern_edge_context(connection: &Connection, value: &str) -> StoreResult<i64> {
    intern_dict_value(connection, "edge_context_dict", value)
}

fn intern_language(connection: &Connection, value: &str) -> StoreResult<i64> {
    intern_dict_value(connection, "language_dict", value)
}

fn intern_edge_provenance(connection: &Connection, value: &str) -> StoreResult<i64> {
    intern_dict_value(connection, "edge_provenance_dict", value)
}

fn intern_dict_value(connection: &Connection, table: &str, value: &str) -> StoreResult<i64> {
    let start = Instant::now();
    let result = (|| {
        if hashed_dictionary_table(table) {
            return intern_hashed_dict_value(connection, table, value);
        }
        let insert_sql = format!("INSERT OR IGNORE INTO {table} (value) VALUES (?1)");
        let mut statement = connection.prepare_cached(&insert_sql)?;
        let changed = statement.execute([value])?;
        if changed > 0 {
            return Ok(connection.last_insert_rowid());
        }
        lookup_dict_value(connection, table, value)?
            .ok_or_else(|| StoreError::Message(format!("failed to intern {table} value: {value}")))
    })();
    record_sqlite_profile("dictionary_lookup_insert", start.elapsed());
    result
}

fn intern_dict_value_cached(
    connection: &Connection,
    cache: &mut DictionaryInternCache,
    table: &'static str,
    value: &str,
) -> StoreResult<i64> {
    let key = (table, value.to_string());
    if let Some(id) = cache.values.get(&key).copied() {
        record_sqlite_profile("dictionary_cache_hit", Duration::ZERO);
        return Ok(id);
    }
    let id = intern_dict_value(connection, table, value)?;
    cache.values.insert(key, id);
    Ok(id)
}

fn intern_path_cached(
    connection: &Connection,
    cache: &mut DictionaryInternCache,
    value: &str,
) -> StoreResult<i64> {
    intern_dict_value_cached(connection, cache, "path_dict", value)
}

fn intern_language_cached(
    connection: &Connection,
    cache: &mut DictionaryInternCache,
    value: &str,
) -> StoreResult<i64> {
    intern_dict_value_cached(connection, cache, "language_dict", value)
}

fn intern_entity_object_id_cached(
    connection: &Connection,
    cache: &mut DictionaryInternCache,
    value: &str,
) -> StoreResult<i64> {
    if let Some(id) = cache.entity_ids.get(value).copied() {
        record_sqlite_profile("dictionary_cache_hit", Duration::ZERO);
        return Ok(id);
    }

    let start = Instant::now();
    let id = if let Some(hash) = canonical_entity_hash(value) {
        if let Some(existing) = lookup_entity_id_by_hash(connection, &hash)? {
            existing
        } else if let Some(existing) =
            lookup_hashed_dict_value(connection, "object_id_dict", value)?
        {
            existing
        } else if let Some(existing) = lookup_entity_id_history(connection, &hash)? {
            existing
        } else {
            allocate_dense_entity_id_cached(connection, cache)?
        }
    } else {
        intern_object_id(connection, value)?
    };
    advance_dense_entity_id_cache(cache, id);
    cache.entity_ids.insert(value.to_string(), id);
    record_sqlite_profile("dictionary_lookup_insert", start.elapsed());
    Ok(id)
}

fn advance_dense_entity_id_cache(cache: &mut DictionaryInternCache, id: i64) {
    if let Some(next) = cache.next_dense_entity_id.as_mut() {
        if id >= *next {
            *next = id + 1;
        }
    }
}

fn allocate_dense_entity_id_cached(
    connection: &Connection,
    cache: &mut DictionaryInternCache,
) -> StoreResult<i64> {
    let next = match cache.next_dense_entity_id {
        Some(next) => next,
        None => next_dense_entity_id(connection)?,
    };
    cache.next_dense_entity_id = Some(next + 1);
    Ok(next)
}

fn intern_object_id_cached(
    connection: &Connection,
    cache: &mut DictionaryInternCache,
    value: &str,
) -> StoreResult<i64> {
    if canonical_edge_hash(value).is_some() {
        return Ok(compact_edge_key(value));
    }
    if canonical_entity_hash(value).is_some() {
        if let Some(id) = cache.entity_ids.get(value).copied() {
            record_sqlite_profile("dictionary_cache_hit", Duration::ZERO);
            return Ok(id);
        }
        let id = intern_object_id(connection, value)?;
        advance_dense_entity_id_cache(cache, id);
        cache.entity_ids.insert(value.to_string(), id);
        return Ok(id);
    }
    let key = ("object_id_dict", value.to_string());
    if let Some(id) = cache.values.get(&key).copied() {
        record_sqlite_profile("dictionary_cache_hit", Duration::ZERO);
        return Ok(id);
    }
    let id = intern_object_id(connection, value)?;
    cache.values.insert(key, id);
    Ok(id)
}

fn ensure_content_template_for_path_id_cached(
    connection: &Connection,
    cache: &mut DictionaryInternCache,
    content_hash: &str,
    language_id: Option<i64>,
    canonical_path_id: i64,
) -> StoreResult<i64> {
    let language_key = language_id.unwrap_or(0);
    let key = (content_hash.to_string(), language_key);
    if let Some(id) = cache.content_templates.get(&key).copied() {
        record_sqlite_profile("dictionary_cache_hit", Duration::ZERO);
        return Ok(id);
    }
    let id = ensure_content_template_for_path_id(
        connection,
        content_hash,
        language_id,
        canonical_path_id,
    )?;
    cache.content_templates.insert(key, id);
    Ok(id)
}

fn ensure_file_for_path_cached(
    connection: &Connection,
    cache: &mut DictionaryInternCache,
    repo_relative_path: &str,
    file_hash: Option<&str>,
) -> StoreResult<(i64, i64)> {
    let has_content_hash = file_hash.is_some_and(|hash| !hash.is_empty());
    if let Some(cached) = cache.file_ids.get(repo_relative_path).copied() {
        if !has_content_hash || cached.has_content_hash {
            record_sqlite_profile("dictionary_cache_hit", Duration::ZERO);
            return Ok((cached.path_id, cached.file_id));
        }
    }

    let path_id = intern_path_cached(connection, cache, repo_relative_path)?;
    let language_id = None;
    let content_template_id = file_hash
        .filter(|hash| !hash.is_empty())
        .map(|hash| {
            ensure_content_template_for_path_id_cached(
                connection,
                cache,
                hash,
                language_id,
                path_id,
            )
        })
        .transpose()?;
    let existing_file_id = connection
        .query_row(
            "SELECT file_id FROM files WHERE path_id = ?1",
            [path_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let file_id = if let Some(file_id) = existing_file_id {
        if let Some(hash) = file_hash.filter(|hash| !hash.is_empty()) {
            connection.execute(
                "UPDATE files
                 SET content_hash = ?1,
                     content_template_id = COALESCE(content_template_id, ?2)
                 WHERE file_id = ?3 AND content_hash = ''",
                params![hash, content_template_id, file_id],
            )?;
        }
        file_id
    } else {
        connection.execute(
            "
            INSERT INTO files (
                file_id, path_id, content_hash, mtime_unix_ms, size_bytes,
                language_id, indexed_at_unix_ms, content_template_id, metadata_json
            ) VALUES (?1, ?1, ?2, NULL, 0, NULL, NULL, ?3, '{}')
            ",
            params![path_id, file_hash.unwrap_or_default(), content_template_id],
        )?;
        path_id
    };
    cache.file_ids.insert(
        repo_relative_path.to_string(),
        CachedFileIds {
            path_id,
            file_id,
            has_content_hash,
        },
    );
    Ok((path_id, file_id))
}

fn cache_entity_file_id(
    entity_file_cache: &RefCell<BTreeMap<i64, Vec<String>>>,
    entity_id_key: i64,
    repo_relative_path: &str,
) {
    entity_file_cache.borrow_mut().insert(
        entity_id_key,
        vec![normalize_repo_relative_path(repo_relative_path)],
    );
}

fn intern_symbol_cached(
    connection: &Connection,
    cache: &mut DictionaryInternCache,
    value: &str,
) -> StoreResult<i64> {
    validate_identity_component("symbol", value, MAX_SYMBOL_VALUE_BYTES)?;
    intern_dict_value_cached(connection, cache, "symbol_dict", value)
}

fn intern_qualified_name_cached(
    connection: &Connection,
    cache: &mut DictionaryInternCache,
    value: &str,
) -> StoreResult<i64> {
    validate_qualified_name_component(value)?;
    let key = ("qualified_name_dict", value.to_string());
    if let Some(id) = cache.values.get(&key).copied() {
        record_sqlite_profile("dictionary_cache_hit", Duration::ZERO);
        return Ok(id);
    }
    let start = Instant::now();
    let (prefix, suffix) = split_qualified_name(value);
    let prefix_id = intern_dict_value_cached(connection, cache, "qname_prefix_dict", prefix)?;
    let suffix_id = intern_symbol_cached(connection, cache, suffix)?;
    let changed = connection.execute(
        "INSERT OR IGNORE INTO qualified_name_dict (prefix_id, suffix_id, value) VALUES (?1, ?2, NULL)",
        params![prefix_id, suffix_id],
    )?;
    let id = if changed > 0 {
        connection.last_insert_rowid()
    } else {
        connection
            .query_row(
                "SELECT id FROM qualified_name_dict WHERE prefix_id = ?1 AND suffix_id = ?2",
                params![prefix_id, suffix_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::Message(format!(
                    "failed to intern qualified name: {}",
                    compact_error_value(value)
                ))
            })?
    };
    cache.values.insert(key, id);
    record_sqlite_profile("dictionary_lookup_insert", start.elapsed());
    Ok(id)
}

fn lookup_dict_value(
    connection: &Connection,
    table: &str,
    value: &str,
) -> StoreResult<Option<i64>> {
    if hashed_dictionary_table(table) {
        return lookup_hashed_dict_value(connection, table, value);
    }
    let select_sql = format!("SELECT id FROM {table} WHERE value = ?1");
    let mut statement = connection.prepare_cached(&select_sql)?;
    statement
        .query_row([value], |row| row.get(0))
        .optional()
        .map_err(StoreError::from)
}

fn split_qualified_name(value: &str) -> (&str, &str) {
    value
        .rsplit_once('.')
        .map(|(prefix, suffix)| (prefix, suffix))
        .unwrap_or(("", value))
}

fn validate_entity_identity_fields(entity: &Entity) -> StoreResult<()> {
    validate_identity_component("entity display name", &entity.name, MAX_SYMBOL_VALUE_BYTES)?;
    validate_qualified_name_component(&entity.qualified_name)
}

fn validate_qualified_name_component(value: &str) -> StoreResult<()> {
    validate_identity_component("qualified_name", value, MAX_QUALIFIED_NAME_BYTES)?;
    let (prefix, suffix) = split_qualified_name(value);
    validate_identity_component("qname_prefix", prefix, MAX_QNAME_PREFIX_BYTES)?;
    validate_identity_component("qname_suffix", suffix, MAX_SYMBOL_VALUE_BYTES)
}

fn validate_identity_component(kind: &str, value: &str, max_bytes: usize) -> StoreResult<()> {
    if value.len() <= max_bytes {
        return Ok(());
    }
    Err(StoreError::Message(format!(
        "{kind} exceeds max identity length: {} bytes > {max_bytes}; value={}",
        value.len(),
        compact_error_value(value)
    )))
}

fn compact_error_value(value: &str) -> String {
    const ERROR_VALUE_PREVIEW_CHARS: usize = 80;
    let preview = value
        .chars()
        .take(ERROR_VALUE_PREVIEW_CHARS)
        .collect::<String>();
    if value.chars().count() > ERROR_VALUE_PREVIEW_CHARS {
        format!("{preview}...")
    } else {
        preview
    }
}

fn hashed_dictionary_table(table: &str) -> bool {
    matches!(
        table,
        "object_id_dict" | "symbol_dict" | "qname_prefix_dict"
    )
}

fn intern_hashed_dict_value(connection: &Connection, table: &str, value: &str) -> StoreResult<i64> {
    if let Some(existing) = lookup_hashed_dict_value(connection, table, value)? {
        return Ok(existing);
    }
    let insert_sql =
        format!("INSERT INTO {table} (value, value_hash, value_len) VALUES (?1, ?2, ?3)");
    let mut statement = connection.prepare_cached(&insert_sql)?;
    statement.execute(params![
        value,
        stable_text_hash_key(value),
        stable_text_len(value)
    ])?;
    Ok(connection.last_insert_rowid())
}

fn lookup_hashed_dict_value(
    connection: &Connection,
    table: &str,
    value: &str,
) -> StoreResult<Option<i64>> {
    let select_sql =
        format!("SELECT id, value FROM {table} WHERE value_hash = ?1 AND value_len = ?2");
    let mut statement = connection.prepare_cached(&select_sql)?;
    let mut rows = statement.query(params![stable_text_hash_key(value), stable_text_len(value)])?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let stored: String = row.get(1)?;
        if stored == value {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn insert_file_compact_row(connection: &Connection, file: &FileRecord) -> StoreResult<()> {
    let path_id = intern_path(connection, &file.repo_relative_path)?;
    let language_id = match &file.language {
        Some(language) => Some(intern_language(connection, language)?),
        None => None,
    };
    let content_template_id =
        ensure_content_template_for_path_id(connection, &file.file_hash, language_id, path_id)?;
    connection.execute(
        "
        INSERT INTO files (
            file_id, path_id, content_hash, mtime_unix_ms, size_bytes,
            language_id, indexed_at_unix_ms, content_template_id, metadata_json
        ) VALUES (?1, ?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(path_id) DO UPDATE SET
            content_hash = excluded.content_hash,
            mtime_unix_ms = excluded.mtime_unix_ms,
            language_id = excluded.language_id,
            size_bytes = excluded.size_bytes,
            indexed_at_unix_ms = excluded.indexed_at_unix_ms,
            content_template_id = excluded.content_template_id,
            metadata_json = excluded.metadata_json
        ",
        params![
            path_id,
            file.file_hash,
            file.size_bytes,
            language_id,
            file.indexed_at_unix_ms,
            content_template_id,
            to_json(&file.metadata)?,
        ],
    )?;
    Ok(())
}

fn insert_entity_compact_row(
    connection: &Connection,
    entity: &Entity,
    metadata_json: &str,
) -> StoreResult<()> {
    let entity_id = intern_entity_object_id(connection, &entity.id)?;
    let kind_id = intern_entity_kind(connection, &entity.kind.to_string())?;
    let name_id = intern_symbol(connection, &entity.name)?;
    let qualified_name_id = intern_qualified_name(connection, &entity.qualified_name)?;
    let (path_id, file_id) = ensure_file_for_path(
        connection,
        &entity.repo_relative_path,
        entity.file_hash.as_deref(),
    )?;
    let created_from_id = intern_extractor(connection, &entity.created_from)?;
    let span = entity.source_span.as_ref();
    let span_path_id = match span {
        Some(span) => Some(intern_path(connection, &span.repo_relative_path)?),
        None => None,
    };
    let declaration_span_id = span.map(|_| entity_id);
    let sql_start = Instant::now();
    connection.execute(
        "
        INSERT INTO entities (
            id_key, entity_hash, kind_id, name_id, qualified_name_id, path_id,
            span_path_id, start_line, start_column, end_line, end_column,
            content_hash, created_from_id, confidence, metadata_json,
            parent_id, file_id, scope_id, declaration_span_id, structural_flags
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
        ON CONFLICT(id_key) DO UPDATE SET
            entity_hash = excluded.entity_hash,
            kind_id = excluded.kind_id,
            name_id = excluded.name_id,
            qualified_name_id = excluded.qualified_name_id,
            path_id = excluded.path_id,
            span_path_id = excluded.span_path_id,
            start_line = excluded.start_line,
            start_column = excluded.start_column,
            end_line = excluded.end_line,
            end_column = excluded.end_column,
            content_hash = excluded.content_hash,
            created_from_id = excluded.created_from_id,
            confidence = excluded.confidence,
            metadata_json = excluded.metadata_json,
            file_id = excluded.file_id,
            declaration_span_id = excluded.declaration_span_id
        ",
        params![
            entity_id,
            entity_hash_blob(&entity.id),
            kind_id,
            name_id,
            qualified_name_id,
            path_id,
            span_path_id,
            span.map(|span| span.start_line),
            span.and_then(|span| span.start_column),
            span.map(|span| span.end_line),
            span.and_then(|span| span.end_column),
            entity.content_hash,
            created_from_id,
            entity.confidence,
            metadata_json,
            None::<i64>,
            file_id,
            None::<i64>,
            declaration_span_id,
            0_i64,
        ],
    )?;
    map_entity_to_file(connection, &entity.repo_relative_path, &entity.id)?;
    record_sqlite_profile("entity_insert_sql", sql_start.elapsed());
    Ok(())
}

fn insert_edge_compact_row(
    connection: &Connection,
    edge: &Edge,
    provenance_edges_json: &str,
    _metadata_json: &str,
) -> StoreResult<()> {
    let edge_id = intern_object_id(connection, &edge.id)?;
    let head_id = intern_object_id(connection, &edge.head_id)?;
    let tail_id = intern_object_id(connection, &edge.tail_id)?;
    let relation_id = intern_relation_kind(connection, &edge.relation.to_string())?;
    let (span_path_id, file_id) = ensure_file_for_path(
        connection,
        &edge.source_span.repo_relative_path,
        edge.file_hash.as_deref(),
    )?;
    let extractor_id = intern_extractor(connection, &edge.extractor)?;
    let exactness_id = intern_exactness(connection, &edge.exactness.to_string())?;
    let resolution_kind = edge_resolution_kind(edge);
    let resolution_kind_id = intern_resolution_kind(connection, &resolution_kind)?;
    let edge_class_id = intern_edge_class(connection, &edge.edge_class.to_string())?;
    let context_id = intern_edge_context(connection, &edge.context.to_string())?;
    let flags_bitset = edge_flags_bitset(edge, &resolution_kind);
    let confidence_q = quantize_confidence(edge.confidence);
    let provenance_id = intern_edge_provenance(connection, provenance_edges_json)?;
    let sql_start = Instant::now();
    connection.execute(
        "
        INSERT INTO edges (
            id_key, head_id_key, relation_id, tail_id_key,
            span_path_id, start_line, start_column, end_line, end_column,
            repo_commit, file_id, extractor_id, confidence, exactness_id,
            confidence_q, resolution_kind_id, edge_class_id, context_id,
            context_kind_id, flags_bitset, derived, provenance_edges_json, provenance_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
        ON CONFLICT(id_key) DO UPDATE SET
            head_id_key = excluded.head_id_key,
            relation_id = excluded.relation_id,
            tail_id_key = excluded.tail_id_key,
            span_path_id = excluded.span_path_id,
            start_line = excluded.start_line,
            start_column = excluded.start_column,
            end_line = excluded.end_line,
            end_column = excluded.end_column,
            repo_commit = excluded.repo_commit,
            file_id = excluded.file_id,
            extractor_id = excluded.extractor_id,
            confidence = excluded.confidence,
            exactness_id = excluded.exactness_id,
            confidence_q = excluded.confidence_q,
            resolution_kind_id = excluded.resolution_kind_id,
            edge_class_id = excluded.edge_class_id,
            context_id = excluded.context_id,
            context_kind_id = excluded.context_kind_id,
            flags_bitset = excluded.flags_bitset,
            derived = excluded.derived,
            provenance_edges_json = excluded.provenance_edges_json,
            provenance_id = excluded.provenance_id
        ",
        params![
            edge_id,
            head_id,
            relation_id,
            tail_id,
            span_path_id,
            edge.source_span.start_line,
            edge.source_span.start_column,
            edge.source_span.end_line,
            edge.source_span.end_column,
            edge.repo_commit,
            file_id,
            extractor_id,
            edge.confidence,
            exactness_id,
            confidence_q,
            resolution_kind_id,
            edge_class_id,
            context_id,
            context_id,
            flags_bitset,
            edge.derived,
            provenance_edges_json,
            provenance_id,
        ],
    )?;
    connection.execute(
        "DELETE FROM edge_debug_metadata WHERE edge_id = ?1",
        [edge_id],
    )?;
    map_edge_to_file(connection, &edge.source_span.repo_relative_path, &edge.id)?;
    map_edge_to_entity_files(connection, &edge.id, head_id, tail_id)?;
    map_source_span_to_file(connection, &edge.source_span.repo_relative_path, &edge.id)?;
    record_sqlite_profile("edge_insert_sql", sql_start.elapsed());
    Ok(())
}

fn insert_source_span_compact_row(
    connection: &Connection,
    id: &str,
    span: &SourceSpan,
) -> StoreResult<()> {
    let id_key = intern_object_id(connection, id)?;
    let path_id = intern_path(connection, &span.repo_relative_path)?;
    let sql_start = Instant::now();
    connection.execute(
        "
        INSERT INTO source_spans (
            id_key, path_id, start_line, start_column, end_line, end_column
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(id_key) DO UPDATE SET
            path_id = excluded.path_id,
            start_line = excluded.start_line,
            start_column = excluded.start_column,
            end_line = excluded.end_line,
            end_column = excluded.end_column
        ",
        params![
            id_key,
            path_id,
            span.start_line,
            span.start_column,
            span.end_line,
            span.end_column,
        ],
    )?;
    map_source_span_to_file(connection, &span.repo_relative_path, id)?;
    record_sqlite_profile("source_span_insert_sql", sql_start.elapsed());
    Ok(())
}

fn upsert_fts_row(
    connection: &Connection,
    kind: TextSearchKind,
    id: &str,
    repo_relative_path: &str,
    line: Option<u32>,
    title: &str,
    body: &str,
) -> StoreResult<()> {
    delete_fts_row(connection, kind, id)?;
    insert_fts_row(connection, kind, id, repo_relative_path, line, title, body)
}

fn upsert_entity_fts_row(connection: &Connection, entity: &Entity) -> StoreResult<()> {
    upsert_fts_row(
        connection,
        TextSearchKind::Entity,
        &entity.id,
        &entity.repo_relative_path,
        entity.source_span.as_ref().map(|span| span.start_line),
        &entity.qualified_name,
        &entity_fts_body(entity),
    )
}

fn insert_entity_fts_row(connection: &Connection, entity: &Entity) -> StoreResult<()> {
    insert_fts_row(
        connection,
        TextSearchKind::Entity,
        &entity.id,
        &entity.repo_relative_path,
        entity.source_span.as_ref().map(|span| span.start_line),
        &entity.qualified_name,
        &entity_fts_body(entity),
    )
}

fn entity_fts_body(entity: &Entity) -> String {
    format!(
        "{} {} {} {}",
        entity.kind, entity.name, entity.qualified_name, entity.created_from
    )
}

fn insert_fts_row(
    connection: &Connection,
    kind: TextSearchKind,
    id: &str,
    repo_relative_path: &str,
    line: Option<u32>,
    title: &str,
    body: &str,
) -> StoreResult<()> {
    let mut statement = connection.prepare_cached(
        "
        INSERT INTO stage0_fts (kind, id, repo_relative_path, line, title, body)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ",
    )?;
    let sql_start = Instant::now();
    statement.execute(params![
        kind.as_str(),
        id,
        repo_relative_path,
        line.map(i64::from),
        title,
        body
    ])?;
    let rowid = connection.last_insert_rowid();
    connection.execute(
        "
        INSERT OR IGNORE INTO file_fts_rows (file_id, rowid, kind, object_id)
        VALUES (?1, ?2, ?3, ?4)
        ",
        params![repo_relative_path, rowid, kind.as_str(), id],
    )?;
    record_sqlite_profile("fts_insert_sql", sql_start.elapsed());
    Ok(())
}

fn delete_fts_row(connection: &Connection, kind: TextSearchKind, id: &str) -> StoreResult<()> {
    let sql_start = Instant::now();
    let rowids = fts_rowids_for_object(connection, kind, id)?;
    if rowids.is_empty() {
        let mut statement =
            connection.prepare_cached("DELETE FROM stage0_fts WHERE kind = ?1 AND id = ?2")?;
        statement.execute(params![kind.as_str(), id])?;
    } else {
        delete_fts_rowids(connection, &rowids)?;
    }
    connection.execute(
        "DELETE FROM file_fts_rows WHERE kind = ?1 AND object_id = ?2",
        params![kind.as_str(), id],
    )?;
    record_sqlite_profile("fts_delete_sql", sql_start.elapsed());
    Ok(())
}

fn delete_fts_rows_for_file(connection: &Connection, repo_relative_path: &str) -> StoreResult<()> {
    let sql_start = Instant::now();
    let rowids = fts_rowids_for_file(connection, repo_relative_path)?;
    if rowids.is_empty() {
        let mut statement =
            connection.prepare_cached("DELETE FROM stage0_fts WHERE repo_relative_path = ?1")?;
        statement.execute([repo_relative_path])?;
    } else {
        delete_fts_rowids(connection, &rowids)?;
    }
    connection.execute(
        "DELETE FROM file_fts_rows WHERE file_id = ?1",
        [repo_relative_path],
    )?;
    record_sqlite_profile("fts_delete_sql", sql_start.elapsed());
    Ok(())
}

fn fts_rowids_for_file(connection: &Connection, file_id: &str) -> StoreResult<Vec<i64>> {
    let mut statement =
        connection.prepare_cached("SELECT rowid FROM file_fts_rows WHERE file_id = ?1")?;
    let rows = statement.query_map([file_id], |row| row.get(0))?;
    collect_rows(rows)
}

fn fts_rowids_for_object(
    connection: &Connection,
    kind: TextSearchKind,
    id: &str,
) -> StoreResult<Vec<i64>> {
    let mut statement = connection
        .prepare_cached("SELECT rowid FROM file_fts_rows WHERE kind = ?1 AND object_id = ?2")?;
    let rows = statement.query_map(params![kind.as_str(), id], |row| row.get(0))?;
    collect_rows(rows)
}

fn delete_fts_rowids(connection: &Connection, rowids: &[i64]) -> StoreResult<()> {
    let mut statement = connection.prepare_cached("DELETE FROM stage0_fts WHERE rowid = ?1")?;
    for rowid in rowids {
        statement.execute([rowid])?;
    }
    Ok(())
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&Row<'_>) -> rusqlite::Result<T>>,
) -> StoreResult<Vec<T>> {
    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

fn query_edges<P>(connection: &Connection, sql: &str, params: P) -> StoreResult<Vec<Edge>>
where
    P: rusqlite::Params,
{
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params, edge_from_row)?;
    collect_rows(rows)
}

fn fts_query(query: &str) -> String {
    query
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        .filter(|part| part.len() >= 2)
        .map(|part| format!("\"{}\"", part.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn to_json<T: Serialize>(value: &T) -> StoreResult<String> {
    Ok(serde_json::to_string(value)?)
}

fn json_column<T: DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    let value = row.get::<_, String>(index)?;
    serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn source_span_column(row: &Row<'_>, index: usize) -> rusqlite::Result<SourceSpan> {
    let value = row.get::<_, String>(index)?;
    if value.trim_start().starts_with('{') {
        return serde_json::from_str(&value).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
        });
    }
    value.parse::<SourceSpan>().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn optional_source_span_column(
    row: &Row<'_>,
    index: usize,
) -> rusqlite::Result<Option<SourceSpan>> {
    let value = row.get::<_, String>(index)?;
    if value.trim().is_empty() || value == "null" {
        return Ok(None);
    }
    if value.trim_start().starts_with('{') {
        return serde_json::from_str(&value).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
        });
    }
    value.parse::<SourceSpan>().map(Some).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn enum_column<T>(row: &Row<'_>, index: usize) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let value = row.get::<_, String>(index)?;
    value.parse().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn entity_from_row(row: &Row<'_>) -> rusqlite::Result<Entity> {
    let source_span = if let Ok(index) = row.as_ref().column_index("source_span_json") {
        optional_source_span_column(row, index)?
    } else {
        optional_source_span_from_compact_row(row)?
    };
    Ok(Entity {
        id: row.get("id")?,
        kind: enum_column_by_name(row, "kind")?,
        name: row.get("name")?,
        qualified_name: row.get("qualified_name")?,
        repo_relative_path: row.get("repo_relative_path")?,
        source_span,
        content_hash: row.get("content_hash")?,
        file_hash: row.get("file_hash")?,
        created_from: row.get("created_from")?,
        confidence: row.get("confidence")?,
        metadata: json_column_by_name(row, "metadata_json")?,
    })
}

fn edge_from_row(row: &Row<'_>) -> rusqlite::Result<Edge> {
    let source_span = if let Ok(index) = row.as_ref().column_index("source_span_json") {
        source_span_column(row, index)?
    } else {
        source_span_from_compact_row(row)?
    };
    let head_id = row.get("head_id")?;
    let relation = enum_column_by_name(row, "relation")?;
    let tail_id = row.get("tail_id")?;
    let id = row
        .get::<_, Option<String>>("id")?
        .unwrap_or_else(|| stable_edge_id(&head_id, relation, &tail_id, &source_span));
    Ok(Edge {
        id,
        head_id,
        relation,
        tail_id,
        source_span,
        repo_commit: row.get("repo_commit")?,
        file_hash: row.get("file_hash")?,
        extractor: row.get("extractor")?,
        confidence: row.get("confidence")?,
        exactness: enum_column_by_name(row, "exactness")?,
        edge_class: optional_enum_column_by_name(row, "edge_class")?.unwrap_or(EdgeClass::Unknown),
        context: optional_enum_column_by_name(row, "context")?.unwrap_or(EdgeContext::Unknown),
        derived: row.get("derived")?,
        provenance_edges: json_column_by_name(row, "provenance_edges_json")?,
        metadata: json_column_by_name(row, "metadata_json")?,
    })
}

#[derive(Debug, Clone, Copy)]
enum EntityStructuralFilter {
    Any,
    Head(i64),
    Tail(i64),
}

fn is_entity_attribute_structural_relation(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Contains | RelationKind::DefinedIn | RelationKind::Declares
    )
}

fn entity_attribute_relation_count(
    connection: &Connection,
    relation: RelationKind,
) -> StoreResult<u64> {
    if !is_entity_attribute_structural_relation(relation)
        || !exists_in_sqlite_master(connection, "table", "entities")?
        || !table_has_column(connection, "entities", "parent_id")?
    {
        return Ok(0);
    }
    if !table_has_column(connection, "entities", "structural_flags")? {
        return Ok(0);
    }
    let flag = match relation {
        RelationKind::Contains => ENTITY_STRUCTURAL_FLAG_CONTAINED_BY_PARENT,
        RelationKind::DefinedIn => ENTITY_STRUCTURAL_FLAG_DEFINED_IN_PARENT,
        RelationKind::Declares => ENTITY_STRUCTURAL_FLAG_DECLARED_BY_PARENT,
        _ => return Ok(0),
    };
    let sql = format!(
        "
        SELECT COUNT(*)
        FROM entities child
        JOIN entities parent ON parent.id_key = child.parent_id
        WHERE child.parent_id IS NOT NULL
          AND child.span_path_id IS NOT NULL
          AND child.start_line IS NOT NULL
          AND child.end_line IS NOT NULL
          AND (child.structural_flags & {flag}) != 0
        "
    );
    let count: i64 = connection.query_row(&sql, [], |row| row.get(0))?;
    Ok(count.max(0) as u64)
}

fn query_entity_attribute_structural_edges(
    connection: &Connection,
    relation: RelationKind,
    filter: EntityStructuralFilter,
    limit: usize,
) -> StoreResult<Vec<Edge>> {
    if limit == 0
        || !is_entity_attribute_structural_relation(relation)
        || !exists_in_sqlite_master(connection, "table", "entities")?
        || !table_has_column(connection, "entities", "parent_id")?
    {
        return Ok(Vec::new());
    }
    if relation == RelationKind::Declares
        && !table_has_column(connection, "entities", "structural_flags")?
    {
        return Ok(Vec::new());
    }

    let relation_label = relation.to_string();
    let (head_expr, tail_expr, head_key_expr, tail_key_expr, edge_class) = match relation {
        RelationKind::Contains | RelationKind::Declares => (
            "parent_oid.value",
            "child_oid.value",
            "child.parent_id",
            "child.id_key",
            "base_exact",
        ),
        RelationKind::DefinedIn => (
            "child_oid.value",
            "parent_oid.value",
            "child.id_key",
            "child.parent_id",
            "unknown",
        ),
        _ => return Ok(Vec::new()),
    };
    let flag = match relation {
        RelationKind::Contains => ENTITY_STRUCTURAL_FLAG_CONTAINED_BY_PARENT,
        RelationKind::DefinedIn => ENTITY_STRUCTURAL_FLAG_DEFINED_IN_PARENT,
        RelationKind::Declares => ENTITY_STRUCTURAL_FLAG_DECLARED_BY_PARENT,
        _ => return Ok(Vec::new()),
    };
    let endpoint_filter = match filter {
        EntityStructuralFilter::Any => String::new(),
        EntityStructuralFilter::Head(id) => format!("AND {head_key_expr} = {id}"),
        EntityStructuralFilter::Tail(id) => format!("AND {tail_key_expr} = {id}"),
    };
    let bounded_limit = limit.min(i64::MAX as usize) as i64;
    let sql = format!(
        "
        SELECT NULL AS id,
               {head_expr} AS head_id,
               '{relation_label}' AS relation,
               {tail_expr} AS tail_id,
               COALESCE(span_path.value, path.value) AS span_repo_relative_path,
               COALESCE(child.start_line, 1) AS start_line,
               child.start_column AS start_column,
               COALESCE(child.end_line, child.start_line, 1) AS end_line,
               child.end_column AS end_column,
               NULL AS repo_commit,
               file.content_hash AS file_hash,
               extractor.value AS extractor,
               child.confidence AS confidence,
               'parser_verified' AS exactness,
               '{edge_class}' AS edge_class,
               'production' AS context,
               0 AS derived,
               '[]' AS provenance_edges_json,
               '{{}}' AS metadata_json
        FROM entities child
        JOIN entities parent ON parent.id_key = child.parent_id
        JOIN object_id_lookup child_oid ON child_oid.id = child.id_key
        JOIN object_id_lookup parent_oid ON parent_oid.id = parent.id_key
        JOIN path_dict path ON path.id = child.path_id
        LEFT JOIN path_dict span_path ON span_path.id = child.span_path_id
        LEFT JOIN files file ON file.file_id = child.file_id
        JOIN extractor_dict extractor ON extractor.id = child.created_from_id
        WHERE child.parent_id IS NOT NULL
          AND child.span_path_id IS NOT NULL
          AND child.start_line IS NOT NULL
          AND child.end_line IS NOT NULL
          AND (child.structural_flags & {flag}) != 0
          {endpoint_filter}
        ORDER BY child.id_key
        LIMIT {bounded_limit}
        "
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], edge_from_row)?;
    collect_rows(rows)
}

fn source_span_from_row(row: &Row<'_>) -> rusqlite::Result<SourceSpan> {
    if let Ok(index) = row.as_ref().column_index("span_json") {
        return json_column(row, index);
    }
    source_span_from_compact_row(row)
}

fn optional_source_span_from_compact_row(row: &Row<'_>) -> rusqlite::Result<Option<SourceSpan>> {
    let start_line = row.get::<_, Option<u32>>("start_line")?;
    let end_line = row.get::<_, Option<u32>>("end_line")?;
    match (start_line, end_line) {
        (Some(start_line), Some(end_line)) => Ok(Some(SourceSpan {
            repo_relative_path: row.get("span_repo_relative_path")?,
            start_line,
            start_column: row.get("start_column")?,
            end_line,
            end_column: row.get("end_column")?,
        })),
        _ => Ok(None),
    }
}

fn source_span_from_compact_row(row: &Row<'_>) -> rusqlite::Result<SourceSpan> {
    Ok(SourceSpan {
        repo_relative_path: row.get("span_repo_relative_path")?,
        start_line: row.get("start_line")?,
        start_column: row.get("start_column")?,
        end_line: row.get("end_line")?,
        end_column: row.get("end_column")?,
    })
}

fn file_from_row(row: &Row<'_>) -> rusqlite::Result<FileRecord> {
    Ok(FileRecord {
        repo_relative_path: row.get("repo_relative_path")?,
        file_hash: row.get("file_hash")?,
        language: row.get("language")?,
        size_bytes: row.get("size_bytes")?,
        indexed_at_unix_ms: row.get("indexed_at_unix_ms")?,
        metadata: json_column_by_name(row, "metadata_json")?,
    })
}

fn text_search_hit_from_row(row: &Row<'_>) -> rusqlite::Result<TextSearchHit> {
    let kind = row.get::<_, String>("kind")?;
    let line = row
        .get::<_, Option<i64>>("line")?
        .and_then(|value| u32::try_from(value).ok());
    Ok(TextSearchHit {
        kind: TextSearchKind::try_from(kind.as_str()).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            )
        })?,
        id: row.get("id")?,
        repo_relative_path: row.get("repo_relative_path")?,
        line,
        title: row.get("title")?,
        text: row.get("body")?,
        score: row.get("rank")?,
    })
}

fn repo_index_state_from_row(row: &Row<'_>) -> rusqlite::Result<RepoIndexState> {
    Ok(RepoIndexState {
        repo_id: row.get("repo_id")?,
        repo_root: row.get("repo_root")?,
        repo_commit: row.get("repo_commit")?,
        schema_version: row.get("schema_version")?,
        indexed_at_unix_ms: row.get("indexed_at_unix_ms")?,
        files_indexed: row.get("files_indexed")?,
        entity_count: row.get("entity_count")?,
        edge_count: row.get("edge_count")?,
        metadata: json_column_by_name(row, "metadata_json")?,
    })
}

fn read_db_passport(connection: &Connection) -> StoreResult<Option<DbPassport>> {
    connection
        .query_row(
            "SELECT * FROM codegraph_db_passport WHERE id = 1",
            [],
            db_passport_from_row,
        )
        .optional()
        .map_err(StoreError::from)
}

fn db_passport_from_row(row: &Row<'_>) -> rusqlite::Result<DbPassport> {
    Ok(DbPassport {
        passport_version: row.get("passport_version")?,
        codegraph_schema_version: row.get("codegraph_schema_version")?,
        storage_mode: row.get("storage_mode")?,
        index_scope_policy_hash: row.get("index_scope_policy_hash")?,
        scope_policy_json: row.get("scope_policy_json")?,
        canonical_repo_root: row.get("canonical_repo_root")?,
        git_remote: row.get("git_remote")?,
        worktree_root: row.get("worktree_root")?,
        repo_head: row.get("repo_head")?,
        source_discovery_policy_version: row.get("source_discovery_policy_version")?,
        codegraph_build_version: row.get("codegraph_build_version")?,
        last_successful_index_timestamp: optional_u64_column(
            row,
            "last_successful_index_timestamp",
        )?,
        last_completed_run_id: row.get("last_completed_run_id")?,
        last_run_status: row.get("last_run_status")?,
        integrity_gate_result: row.get("integrity_gate_result")?,
        files_seen: u64_column(row, "files_seen")?,
        files_indexed: u64_column(row, "files_indexed")?,
        created_at_unix_ms: u64_column(row, "created_at_unix_ms")?,
        updated_at_unix_ms: u64_column(row, "updated_at_unix_ms")?,
    })
}

fn u64_column(row: &Row<'_>, name: &str) -> rusqlite::Result<u64> {
    let value: i64 = row.get(name)?;
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, Type::Integer, Box::new(error))
    })
}

fn optional_u64_column(row: &Row<'_>, name: &str) -> rusqlite::Result<Option<u64>> {
    let Some(value) = row.get::<_, Option<i64>>(name)? else {
        return Ok(None);
    };
    Ok(Some(u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, Type::Integer, Box::new(error))
    })?))
}

fn path_evidence_from_row(row: &Row<'_>) -> rusqlite::Result<PathEvidence> {
    Ok(PathEvidence {
        id: row.get("id")?,
        summary: row.get("summary")?,
        source: row.get("source")?,
        target: row.get("target")?,
        metapath: json_column_by_name(row, "metapath_json")?,
        edges: json_column_by_name(row, "edges_json")?,
        source_spans: json_column_by_name(row, "source_spans_json")?,
        exactness: enum_column_by_name(row, "exactness")?,
        length: row.get("length")?,
        confidence: row.get("confidence")?,
        metadata: json_column_by_name(row, "metadata_json")?,
    })
}

fn compact_path_evidence_metadata(path: &PathEvidence) -> BTreeMap<String, Value> {
    let mut metadata = path.metadata.clone();
    let mut removed_materialized_payload = false;
    for key in [
        "edge_labels",
        "ordered_edge_ids",
        "source_spans",
        "exactness_labels",
        "confidence_labels",
        "derived_provenance_expansion",
        "production_test_mock_labels",
        "relation_sequence",
    ] {
        removed_materialized_payload |= metadata.remove(key).is_some();
    }
    if removed_materialized_payload {
        metadata.insert(
            "metadata_storage".to_string(),
            Value::String("compact_materialized_rows".to_string()),
        );
        metadata.insert(
            "materialized_tables".to_string(),
            Value::Array(
                [
                    "path_evidence_edges",
                    "path_evidence_symbols",
                    "path_evidence_tests",
                    "path_evidence_files",
                ]
                .into_iter()
                .map(|value| Value::String(value.to_string()))
                .collect(),
            ),
        );
    }
    metadata
}

fn upsert_path_evidence_materialized_rows(
    connection: &Connection,
    path: &PathEvidence,
) -> StoreResult<()> {
    delete_path_evidence_materialized_rows(connection, &path.id)?;

    let relation_signature = path
        .metapath
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(">");
    let task_class = path_evidence_task_class(path);
    connection.execute(
        "
        INSERT OR REPLACE INTO path_evidence_lookup (
            path_id, source_id, target_id, task_class, relation_signature, length, confidence
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ",
        params![
            path.id,
            path.source,
            path.target,
            task_class,
            relation_signature,
            path.length as i64,
            path.confidence,
        ],
    )?;

    let ordered_edge_ids = path_evidence_ordered_edge_ids(path);
    for (ordinal, (head, relation, tail)) in path.edges.iter().enumerate() {
        let edge_id = ordered_edge_ids
            .get(ordinal)
            .cloned()
            .unwrap_or_else(|| format!("{head}|{relation}|{tail}"));
        let label = path_evidence_edge_label(path, ordinal, &edge_id);
        let source_span_path = path
            .source_spans
            .get(ordinal)
            .map(|span| span.repo_relative_path.clone());
        let exactness = label
            .and_then(|value| value.get("exactness"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| path.exactness.to_string());
        let confidence = label
            .and_then(|value| value.get("confidence"))
            .and_then(Value::as_f64)
            .unwrap_or(path.confidence);
        let derived = label
            .and_then(|value| value.get("derived"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let edge_class = label
            .and_then(|value| {
                value
                    .get("fact_class")
                    .or_else(|| value.get("edge_class"))
                    .and_then(Value::as_str)
            })
            .map(ToString::to_string);
        let context = label
            .and_then(|value| value.get("context"))
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let provenance_edges = label
            .and_then(|value| value.get("provenance_edges"))
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        connection.execute(
            "
            INSERT OR REPLACE INTO path_evidence_edges (
                path_id, ordinal, edge_id, head_id, relation, tail_id, source_span_path,
                exactness, confidence, derived, edge_class, context, provenance_edges_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ",
            params![
                path.id,
                ordinal as i64,
                edge_id,
                head,
                relation.to_string(),
                tail,
                source_span_path,
                exactness,
                confidence,
                i64::from(derived),
                edge_class,
                context,
                to_json(&provenance_edges)?,
            ],
        )?;
    }

    let mut symbols = BTreeMap::<String, &'static str>::new();
    symbols.insert(path.source.clone(), "source");
    symbols.insert(path.target.clone(), "target");
    for (head, _, tail) in &path.edges {
        symbols.entry(head.clone()).or_insert("edge_head");
        symbols.entry(tail.clone()).or_insert("edge_tail");
    }
    for (entity_id, role) in symbols {
        connection.execute(
            "
            INSERT OR IGNORE INTO path_evidence_symbols (entity_id, path_id, role)
            VALUES (?1, ?2, ?3)
            ",
            params![entity_id, path.id, role],
        )?;
    }

    let mut file_ids = path
        .source_spans
        .iter()
        .map(|span| span.repo_relative_path.clone())
        .collect::<Vec<_>>();
    file_ids.sort();
    file_ids.dedup();
    for file_id in file_ids {
        connection.execute(
            "
            INSERT OR IGNORE INTO path_evidence_files (file_id, path_id)
            VALUES (?1, ?2)
            ",
            params![&file_id, &path.id],
        )?;
        connection.execute(
            "
            INSERT OR IGNORE INTO file_path_evidence (file_id, path_id)
            VALUES (?1, ?2)
            ",
            params![&file_id, &path.id],
        )?;
    }

    for (head, relation, tail) in &path.edges {
        if path_evidence_test_relation(*relation) {
            connection.execute(
                "
                INSERT OR IGNORE INTO path_evidence_tests (path_id, test_id, relation)
                VALUES (?1, ?2, ?3)
                ",
                params![path.id, head, relation.to_string()],
            )?;
            connection.execute(
                "
                INSERT OR IGNORE INTO path_evidence_tests (path_id, test_id, relation)
                VALUES (?1, ?2, ?3)
                ",
                params![path.id, tail, relation.to_string()],
            )?;
        }
    }

    Ok(())
}

fn delete_path_evidence_materialized_rows(
    connection: &Connection,
    path_id: &str,
) -> StoreResult<()> {
    for table in [
        "path_evidence_lookup",
        "path_evidence_edges",
        "path_evidence_symbols",
        "path_evidence_tests",
        "path_evidence_files",
        "file_path_evidence",
        "path_evidence_debug_metadata",
    ] {
        connection.execute(
            &format!("DELETE FROM {table} WHERE path_id = ?1"),
            [path_id],
        )?;
    }
    Ok(())
}

fn path_evidence_edge_label<'a>(
    path: &'a PathEvidence,
    ordinal: usize,
    edge_id: &str,
) -> Option<&'a Value> {
    let labels = path.metadata.get("edge_labels")?.as_array()?;
    labels
        .iter()
        .find(|label| {
            label
                .get("edge_id")
                .and_then(Value::as_str)
                .is_some_and(|candidate| candidate == edge_id)
        })
        .or_else(|| labels.get(ordinal))
}

fn path_evidence_ordered_edge_ids(path: &PathEvidence) -> Vec<String> {
    path.metadata
        .get("ordered_edge_ids")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn path_evidence_task_class(path: &PathEvidence) -> String {
    if let Some(value) = path.metadata.get("task_class").and_then(Value::as_str) {
        return value.to_string();
    }
    if path.metapath.iter().any(|relation| {
        matches!(
            relation,
            RelationKind::Authorizes
                | RelationKind::ChecksRole
                | RelationKind::ChecksPermission
                | RelationKind::Sanitizes
                | RelationKind::Exposes
        )
    }) {
        return "security".to_string();
    }
    if path
        .metapath
        .iter()
        .any(|relation| path_evidence_test_relation(*relation))
    {
        return "test-impact".to_string();
    }
    if path.metapath.iter().any(|relation| {
        matches!(
            relation,
            RelationKind::Calls
                | RelationKind::Reads
                | RelationKind::Writes
                | RelationKind::Mutates
                | RelationKind::MayMutate
                | RelationKind::FlowsTo
        )
    }) {
        return "impact".to_string();
    }
    "normal".to_string()
}

fn path_evidence_test_relation(relation: RelationKind) -> bool {
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

fn derived_edge_from_row(row: &Row<'_>) -> rusqlite::Result<DerivedClosureEdge> {
    Ok(DerivedClosureEdge {
        id: row.get("id")?,
        head_id: row.get("head_id")?,
        relation: enum_column_by_name(row, "relation")?,
        tail_id: row.get("tail_id")?,
        provenance_edges: json_column_by_name(row, "provenance_edges_json")?,
        exactness: enum_column_by_name(row, "exactness")?,
        confidence: row.get("confidence")?,
        metadata: json_column_by_name(row, "metadata_json")?,
    })
}

fn retrieval_trace_from_row(row: &Row<'_>) -> rusqlite::Result<RetrievalTraceRecord> {
    Ok(RetrievalTraceRecord {
        id: row.get("id")?,
        task: row.get("task")?,
        trace_json: json_column_by_name(row, "trace_json")?,
        created_at_unix_ms: row.get("created_at_unix_ms")?,
    })
}

#[derive(Debug, Clone)]
struct TemplateInstance {
    content_template_id: i64,
    repo_relative_path: String,
    content_hash: String,
    canonical_repo_relative_path: String,
}

#[derive(Debug, Clone)]
struct TemplateEntityRow {
    local_template_entity_id: i64,
    kind: EntityKind,
    name: String,
    qualified_name: String,
    source_span: Option<SourceSpan>,
    content_hash: Option<String>,
    created_from: String,
    confidence: f64,
    metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
struct TemplateEdgeRow {
    local_template_edge_id: i64,
    local_head_entity_id: i64,
    relation: RelationKind,
    local_tail_entity_id: i64,
    source_span: SourceSpan,
    repo_commit: Option<String>,
    extractor: String,
    confidence: f64,
    exactness: Exactness,
    edge_class: EdgeClass,
    context: EdgeContext,
    derived: bool,
    provenance_edges: Vec<String>,
    metadata: BTreeMap<String, Value>,
}

fn template_source_derived_identity_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::CallSite | EntityKind::Expression | EntityKind::ReturnSite
    )
}

fn template_storage_display_name(
    kind: EntityKind,
    original_name: &str,
    local_template_entity_id: i64,
) -> String {
    if template_source_derived_identity_kind(kind) {
        return kind.id_prefix().to_string();
    }
    if local_template_entity_id <= 0 {
        return original_name.to_string();
    }
    original_name.to_string()
}

fn template_storage_qualified_name(
    kind: EntityKind,
    original_qualified_name: &str,
    local_template_entity_id: i64,
) -> String {
    if template_source_derived_identity_kind(kind) && local_template_entity_id > 0 {
        return format!(
            "@template.{}.{}",
            kind.id_prefix(),
            local_template_entity_id
        );
    }
    original_qualified_name.to_string()
}

fn validate_template_storage_identity(name: &str, qualified_name: &str) -> StoreResult<()> {
    validate_identity_component("entity display name", name, MAX_SYMBOL_VALUE_BYTES)?;
    validate_qualified_name_component(qualified_name)
}

fn upsert_content_template_extraction(
    connection: &Connection,
    dictionary_cache: &mut DictionaryInternCache,
    file: &FileRecord,
    entities: &[Entity],
    edges: &[Edge],
) -> StoreResult<()> {
    if entities.is_empty() && edges.is_empty() {
        return Ok(());
    }
    let path_id = intern_path(connection, &file.repo_relative_path)?;
    let language_id = match &file.language {
        Some(language) => Some(intern_language(connection, language)?),
        None => None,
    };
    let content_template_id =
        ensure_content_template_for_path_id(connection, &file.file_hash, language_id, path_id)?;

    if template_has_entities(connection, content_template_id)? {
        return Ok(());
    }

    let mut sorted_entities = entities.iter().collect::<Vec<_>>();
    sorted_entities.sort_by(|left, right| left.id.cmp(&right.id));
    let local_entity_ids = sorted_entities
        .iter()
        .enumerate()
        .map(|(index, entity)| (entity.id.clone(), (index + 1) as i64))
        .collect::<BTreeMap<_, _>>();
    let mut insert_template_entity = connection.prepare_cached(
        "
        INSERT OR IGNORE INTO template_entities (
            content_template_id, local_template_entity_id, kind_id, name_id,
            qualified_name_id, start_line, start_column, end_line, end_column,
            content_hash, created_from_id, confidence, metadata_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ",
    )?;
    for entity in sorted_entities {
        let local_template_entity_id = local_entity_ids
            .get(&entity.id)
            .copied()
            .unwrap_or_default();
        let storage_name =
            template_storage_display_name(entity.kind, &entity.name, local_template_entity_id);
        let storage_qualified_name = template_storage_qualified_name(
            entity.kind,
            &entity.qualified_name,
            local_template_entity_id,
        );
        validate_template_storage_identity(&storage_name, &storage_qualified_name)?;
        let kind_id = intern_dict_value_cached(
            connection,
            dictionary_cache,
            "entity_kind_dict",
            &entity.kind.to_string(),
        )?;
        let name_id = intern_symbol_cached(connection, dictionary_cache, &storage_name)?;
        let qualified_name_id =
            intern_qualified_name_cached(connection, dictionary_cache, &storage_qualified_name)?;
        let created_from_id = intern_dict_value_cached(
            connection,
            dictionary_cache,
            "extractor_dict",
            &entity.created_from,
        )?;
        let span = entity.source_span.as_ref();
        let sql_start = Instant::now();
        insert_template_entity.execute(params![
            content_template_id,
            local_template_entity_id,
            kind_id,
            name_id,
            qualified_name_id,
            span.map(|span| span.start_line),
            span.and_then(|span| span.start_column),
            span.map(|span| span.end_line),
            span.and_then(|span| span.end_column),
            entity.content_hash,
            created_from_id,
            entity.confidence,
            to_json(&entity.metadata)?,
        ])?;
        record_sqlite_profile("template_entity_insert_sql", sql_start.elapsed());
    }

    let mut sorted_edges = edges.iter().collect::<Vec<_>>();
    sorted_edges.sort_by(|left, right| left.id.cmp(&right.id));
    let mut insert_template_edge = connection.prepare_cached(
        "
        INSERT OR IGNORE INTO template_edges (
            content_template_id, local_template_edge_id, local_head_entity_id,
            relation_id, local_tail_entity_id, start_line, start_column, end_line,
            end_column, repo_commit, extractor_id, confidence, confidence_q,
            exactness_id, resolution_kind_id, edge_class_id, context_id,
            context_kind_id, flags_bitset, derived, provenance_edges_json,
            provenance_id, metadata_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
        ",
    )?;
    for (index, edge) in sorted_edges.into_iter().enumerate() {
        let Some(local_head_entity_id) = local_entity_ids.get(&edge.head_id).copied() else {
            continue;
        };
        let Some(local_tail_entity_id) = local_entity_ids.get(&edge.tail_id).copied() else {
            continue;
        };
        let local_template_edge_id = (index + 1) as i64;
        let relation_id = intern_dict_value_cached(
            connection,
            dictionary_cache,
            "relation_kind_dict",
            &edge.relation.to_string(),
        )?;
        let extractor_id = intern_dict_value_cached(
            connection,
            dictionary_cache,
            "extractor_dict",
            &edge.extractor,
        )?;
        let exactness_id = intern_dict_value_cached(
            connection,
            dictionary_cache,
            "exactness_dict",
            &edge.exactness.to_string(),
        )?;
        let resolution_kind = edge_resolution_kind(edge);
        let resolution_kind_id = intern_dict_value_cached(
            connection,
            dictionary_cache,
            "resolution_kind_dict",
            &resolution_kind,
        )?;
        let edge_class_id = intern_dict_value_cached(
            connection,
            dictionary_cache,
            "edge_class_dict",
            &edge.edge_class.to_string(),
        )?;
        let context_id = intern_dict_value_cached(
            connection,
            dictionary_cache,
            "edge_context_dict",
            &edge.context.to_string(),
        )?;
        let flags_bitset = edge_flags_bitset(edge, &resolution_kind);
        let confidence_q = quantize_confidence(edge.confidence);
        let provenance_edges_json = to_json(&edge.provenance_edges)?;
        let provenance_id = intern_dict_value_cached(
            connection,
            dictionary_cache,
            "edge_provenance_dict",
            &provenance_edges_json,
        )?;
        let sql_start = Instant::now();
        insert_template_edge.execute(params![
            content_template_id,
            local_template_edge_id,
            local_head_entity_id,
            relation_id,
            local_tail_entity_id,
            edge.source_span.start_line,
            edge.source_span.start_column,
            edge.source_span.end_line,
            edge.source_span.end_column,
            edge.repo_commit,
            extractor_id,
            edge.confidence,
            confidence_q,
            exactness_id,
            resolution_kind_id,
            edge_class_id,
            context_id,
            context_id,
            flags_bitset,
            edge.derived,
            provenance_edges_json,
            provenance_id,
            to_json(&edge.metadata)?,
        ])?;
        record_sqlite_profile("template_edge_insert_sql", sql_start.elapsed());
    }
    Ok(())
}

fn template_has_entities(connection: &Connection, content_template_id: i64) -> StoreResult<bool> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM template_entities WHERE content_template_id = ?1 LIMIT 1",
            [content_template_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(exists)
}

fn count_template_entities(connection: &Connection, content_template_id: i64) -> StoreResult<u64> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM template_entities WHERE content_template_id = ?1",
        [content_template_id],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as u64)
}

fn count_template_edges(connection: &Connection, content_template_id: i64) -> StoreResult<u64> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM template_edges WHERE content_template_id = ?1",
        [content_template_id],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as u64)
}

fn template_instance_for_file(
    connection: &Connection,
    repo_relative_path: &str,
) -> StoreResult<Option<TemplateInstance>> {
    let Some(path_id) = lookup_path(connection, repo_relative_path)? else {
        return Ok(None);
    };
    connection
        .query_row(
            "
            SELECT f.content_template_id,
                   path.value AS repo_relative_path,
                   f.content_hash,
                   canonical_path.value AS canonical_repo_relative_path
            FROM files f
            JOIN source_content_template template
              ON template.content_template_id = f.content_template_id
            JOIN path_dict path ON path.id = f.path_id
            JOIN path_dict canonical_path ON canonical_path.id = template.canonical_path_id
            WHERE f.path_id = ?1
              AND f.content_template_id IS NOT NULL
            ",
            [path_id],
            |row| {
                Ok(TemplateInstance {
                    content_template_id: row.get(0)?,
                    repo_relative_path: row.get(1)?,
                    content_hash: row.get(2)?,
                    canonical_repo_relative_path: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(StoreError::from)
}

fn duplicate_template_instances(connection: &Connection) -> StoreResult<Vec<TemplateInstance>> {
    let mut statement = connection.prepare(
        "
        SELECT f.content_template_id,
               path.value AS repo_relative_path,
               f.content_hash,
               canonical_path.value AS canonical_repo_relative_path
        FROM files f
        JOIN source_content_template template
          ON template.content_template_id = f.content_template_id
        JOIN path_dict path ON path.id = f.path_id
        JOIN path_dict canonical_path ON canonical_path.id = template.canonical_path_id
        WHERE EXISTS (
            SELECT 1 FROM template_entities te
            WHERE te.content_template_id = f.content_template_id
        )
        ORDER BY path.value
        ",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(TemplateInstance {
            content_template_id: row.get(0)?,
            repo_relative_path: row.get(1)?,
            content_hash: row.get(2)?,
            canonical_repo_relative_path: row.get(3)?,
        })
    })?;
    collect_rows(rows)
}

fn template_entities_for_instance(
    connection: &Connection,
    instance: &TemplateInstance,
) -> StoreResult<Vec<Entity>> {
    let rows = template_entity_rows(connection, instance)?;
    Ok(synthesize_template_entities(instance, rows))
}

fn synthesize_template_entities(
    instance: &TemplateInstance,
    rows: Vec<TemplateEntityRow>,
) -> Vec<Entity> {
    rows.into_iter()
        .map(|row| synthesize_template_entity(instance, row))
        .collect()
}

fn synthesize_template_entity(instance: &TemplateInstance, row: TemplateEntityRow) -> Entity {
    let source_span = row.source_span.map(|mut span| {
        span.repo_relative_path = instance.repo_relative_path.clone();
        span
    });
    let qualified_name = expand_template_storage_qualified_name(
        &row.qualified_name,
        row.kind,
        &instance.canonical_repo_relative_path,
        &instance.repo_relative_path,
    );
    let signature =
        entity_signature_for_path_overlay(&qualified_name, source_span.as_ref(), &row.created_from);
    let id = stable_entity_id_for_kind(
        &instance.repo_relative_path,
        row.kind,
        &qualified_name,
        Some(&signature),
    );
    let mut metadata = row.metadata;
    metadata.insert(
        "storage_overlay".to_string(),
        "content_template_path_overlay".into(),
    );
    metadata.insert(
        "content_template_id".to_string(),
        Value::from(instance.content_template_id),
    );
    metadata.insert(
        "template_entity_id".to_string(),
        row.local_template_entity_id.into(),
    );
    metadata.insert(
        "canonical_template_path".to_string(),
        instance.canonical_repo_relative_path.clone().into(),
    );
    Entity {
        id,
        kind: row.kind,
        name: row.name,
        qualified_name,
        repo_relative_path: instance.repo_relative_path.clone(),
        source_span,
        content_hash: row.content_hash,
        file_hash: Some(instance.content_hash.clone()),
        created_from: row.created_from,
        confidence: row.confidence,
        metadata,
    }
}

fn template_edges_for_instance(
    connection: &Connection,
    instance: &TemplateInstance,
) -> StoreResult<Vec<Edge>> {
    let entity_rows = template_entity_rows(connection, instance)?;
    let mut entity_id_by_local = BTreeMap::<i64, String>::new();
    for row in entity_rows {
        let local_id = row.local_template_entity_id.clone();
        let entity = synthesize_template_entity(instance, row);
        entity_id_by_local.insert(local_id, entity.id);
    }

    let edge_rows = template_edge_rows(connection, instance)?;
    let mut edges = Vec::new();
    for row in edge_rows {
        let Some(head_id) = entity_id_by_local.get(&row.local_head_entity_id).cloned() else {
            continue;
        };
        let Some(tail_id) = entity_id_by_local.get(&row.local_tail_entity_id).cloned() else {
            continue;
        };
        let mut source_span = row.source_span;
        source_span.repo_relative_path = instance.repo_relative_path.clone();
        let id = stable_edge_id(&head_id, row.relation, &tail_id, &source_span);
        let mut metadata = row.metadata;
        metadata.insert(
            "storage_overlay".to_string(),
            "content_template_path_overlay".into(),
        );
        metadata.insert(
            "content_template_id".to_string(),
            Value::from(instance.content_template_id),
        );
        metadata.insert(
            "template_edge_id".to_string(),
            row.local_template_edge_id.into(),
        );
        metadata.insert(
            "canonical_template_path".to_string(),
            instance.canonical_repo_relative_path.clone().into(),
        );
        edges.push(Edge {
            id,
            head_id,
            relation: row.relation,
            tail_id,
            source_span,
            repo_commit: row.repo_commit,
            file_hash: Some(instance.content_hash.clone()),
            extractor: row.extractor,
            confidence: row.confidence,
            exactness: row.exactness,
            edge_class: row.edge_class,
            context: row.context,
            derived: row.derived,
            provenance_edges: row.provenance_edges,
            metadata,
        });
    }
    Ok(edges)
}

fn template_entity_rows(
    connection: &Connection,
    instance: &TemplateInstance,
) -> StoreResult<Vec<TemplateEntityRow>> {
    let mut statement = connection.prepare(
        "
        SELECT te.local_template_entity_id,
               kind.value AS kind,
               name.value AS name,
               qname.value AS qualified_name,
               te.start_line, te.start_column, te.end_line, te.end_column,
               te.content_hash,
               extractor.value AS created_from,
               te.confidence,
               te.metadata_json
        FROM template_entities te
        JOIN entity_kind_dict kind ON kind.id = te.kind_id
        JOIN symbol_dict name ON name.id = te.name_id
        JOIN qualified_name_lookup qname ON qname.id = te.qualified_name_id
        JOIN extractor_dict extractor ON extractor.id = te.created_from_id
        WHERE te.content_template_id = ?1
        ORDER BY te.local_template_entity_id
        ",
    )?;
    let rows = statement.query_map([instance.content_template_id], |row| {
        let start_line = row.get::<_, Option<u32>>(4)?;
        let end_line = row.get::<_, Option<u32>>(6)?;
        let source_span = match (start_line, end_line) {
            (Some(start_line), Some(end_line)) => Some(SourceSpan {
                repo_relative_path: instance.canonical_repo_relative_path.clone(),
                start_line,
                start_column: row.get(5)?,
                end_line,
                end_column: row.get(7)?,
            }),
            _ => None,
        };
        Ok(TemplateEntityRow {
            local_template_entity_id: row.get(0)?,
            kind: enum_column(row, 1)?,
            name: row.get(2)?,
            qualified_name: row.get(3)?,
            source_span,
            content_hash: row.get(8)?,
            created_from: row.get(9)?,
            confidence: row.get(10)?,
            metadata: json_column(row, 11)?,
        })
    })?;
    collect_rows(rows)
}

fn template_edge_rows(
    connection: &Connection,
    instance: &TemplateInstance,
) -> StoreResult<Vec<TemplateEdgeRow>> {
    let mut statement = connection.prepare(
        "
        SELECT te.local_template_edge_id,
               te.local_head_entity_id,
               relation.value AS relation,
               te.local_tail_entity_id,
               te.start_line, te.start_column, te.end_line, te.end_column,
               te.repo_commit,
               extractor.value AS extractor,
               te.confidence,
               exactness.value AS exactness,
               edge_class.value AS edge_class,
               edge_context.value AS context,
               te.derived,
               te.provenance_edges_json,
               te.metadata_json
        FROM template_edges te
        JOIN relation_kind_dict relation ON relation.id = te.relation_id
        JOIN extractor_dict extractor ON extractor.id = te.extractor_id
        JOIN exactness_dict exactness ON exactness.id = te.exactness_id
        JOIN edge_class_dict edge_class ON edge_class.id = te.edge_class_id
        JOIN edge_context_dict edge_context ON edge_context.id = te.context_id
        WHERE te.content_template_id = ?1
        ORDER BY te.local_template_edge_id
        ",
    )?;
    let rows = statement.query_map([instance.content_template_id], |row| {
        Ok(TemplateEdgeRow {
            local_template_edge_id: row.get(0)?,
            local_head_entity_id: row.get(1)?,
            relation: enum_column(row, 2)?,
            local_tail_entity_id: row.get(3)?,
            source_span: SourceSpan {
                repo_relative_path: instance.canonical_repo_relative_path.clone(),
                start_line: row.get(4)?,
                start_column: row.get(5)?,
                end_line: row.get(6)?,
                end_column: row.get(7)?,
            },
            repo_commit: row.get(8)?,
            extractor: row.get(9)?,
            confidence: row.get(10)?,
            exactness: enum_column(row, 11)?,
            edge_class: enum_column(row, 12)?,
            context: enum_column(row, 13)?,
            derived: row.get(14)?,
            provenance_edges: json_column(row, 15)?,
            metadata: json_column(row, 16)?,
        })
    })?;
    collect_rows(rows)
}

fn remap_template_qualified_name(
    qualified_name: &str,
    canonical_path: &str,
    target_path: &str,
) -> String {
    let canonical_module = module_name_for_path(canonical_path);
    let target_module = module_name_for_path(target_path);
    if qualified_name == canonical_module {
        return target_module;
    }
    for separator in [".", "::"] {
        let prefix = format!("{canonical_module}{separator}");
        if let Some(rest) = qualified_name.strip_prefix(&prefix) {
            return format!("{target_module}{separator}{rest}");
        }
    }
    qualified_name.to_string()
}

fn expand_template_storage_qualified_name(
    stored_qualified_name: &str,
    kind: EntityKind,
    canonical_path: &str,
    target_path: &str,
) -> String {
    if template_source_derived_identity_kind(kind) {
        if let Some(local_id) = stored_qualified_name
            .strip_prefix("@template.")
            .and_then(|rest| rest.strip_prefix(kind.id_prefix()))
            .and_then(|rest| rest.strip_prefix('.'))
        {
            let target_module = module_name_for_path(target_path);
            return format!("{}.{}@{}", target_module, kind.id_prefix(), local_id);
        }
    }
    remap_template_qualified_name(stored_qualified_name, canonical_path, target_path)
}

fn module_name_for_path(path: &str) -> String {
    let normalized = normalize_repo_relative_path(path);
    normalized
        .rsplit_once('.')
        .map(|(without_ext, _)| without_ext)
        .unwrap_or(&normalized)
        .replace('/', "::")
}

fn entity_signature_for_path_overlay(
    qualified_name: &str,
    source_span: Option<&SourceSpan>,
    created_from: &str,
) -> String {
    if created_from == "tree-sitter-static-heuristic" {
        return format!("{qualified_name}@static-reference");
    }
    if let Some(span) = source_span {
        return format!(
            "{}@{}:{}-{}:{}",
            qualified_name,
            span.start_line,
            span.start_column.unwrap_or(1),
            span.end_line,
            span.end_column.unwrap_or(1)
        );
    }
    format!("{qualified_name}@template-overlay")
}

fn synthesized_template_entities(
    connection: &Connection,
    limit: Option<usize>,
) -> StoreResult<Vec<Entity>> {
    let mut entities = Vec::new();
    for instance in duplicate_template_instances(connection)? {
        entities.extend(template_entities_for_instance(connection, &instance)?);
        if let Some(limit) = limit {
            if entities.len() >= limit {
                entities.truncate(limit);
                break;
            }
        }
    }
    Ok(entities)
}

fn synthesized_template_edges(
    connection: &Connection,
    limit: Option<usize>,
) -> StoreResult<Vec<Edge>> {
    let mut edges = Vec::new();
    for instance in duplicate_template_instances(connection)? {
        edges.extend(template_edges_for_instance(connection, &instance)?);
        if let Some(limit) = limit {
            if edges.len() >= limit {
                edges.truncate(limit);
                break;
            }
        }
    }
    Ok(edges)
}

fn synthesized_template_edges_by_relation(
    connection: &Connection,
    head_id: Option<&str>,
    tail_id: Option<&str>,
    relation: RelationKind,
) -> StoreResult<Vec<Edge>> {
    let mut edges = synthesized_template_edges(connection, None)?;
    edges.retain(|edge| {
        edge.relation == relation
            && head_id.is_none_or(|head_id| edge.head_id == head_id)
            && tail_id.is_none_or(|tail_id| edge.tail_id == tail_id)
    });
    Ok(edges)
}

fn find_synthesized_template_entity(
    connection: &Connection,
    id: &str,
) -> StoreResult<Option<Entity>> {
    Ok(synthesized_template_entities(connection, None)?
        .into_iter()
        .find(|entity| entity.id == id))
}

fn find_synthesized_template_edge(connection: &Connection, id: &str) -> StoreResult<Option<Edge>> {
    Ok(synthesized_template_edges(connection, None)?
        .into_iter()
        .find(|edge| edge.id == id))
}

fn duplicate_template_entity_count(connection: &Connection) -> StoreResult<u64> {
    let mut total = 0_u64;
    for instance in duplicate_template_instances(connection)? {
        total += count_template_entities(connection, instance.content_template_id)?;
    }
    Ok(total)
}

fn duplicate_template_edge_count(connection: &Connection) -> StoreResult<u64> {
    let mut total = 0_u64;
    for instance in duplicate_template_instances(connection)? {
        total += count_template_edges(connection, instance.content_template_id)?;
    }
    Ok(total)
}

fn dedupe_entities(entities: &mut Vec<Entity>) {
    let mut seen = BTreeSet::<String>::new();
    entities.retain(|entity| seen.insert(entity.id.clone()));
}

fn dedupe_edges(edges: &mut Vec<Edge>) {
    let mut seen = BTreeSet::<String>::new();
    edges.retain(|edge| seen.insert(edge.id.clone()));
}

fn json_column_by_name<T: DeserializeOwned>(row: &Row<'_>, name: &str) -> rusqlite::Result<T> {
    let index = row.as_ref().column_index(name)?;
    json_column(row, index)
}

fn enum_column_by_name<T>(row: &Row<'_>, name: &str) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let index = row.as_ref().column_index(name)?;
    enum_column(row, index)
}

fn optional_enum_column_by_name<T>(row: &Row<'_>, name: &str) -> rusqlite::Result<Option<T>>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let Ok(index) = row.as_ref().column_index(name) else {
        return Ok(None);
    };
    let value = row.get::<_, Option<String>>(index)?;
    value
        .map(|value| {
            value.parse().map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
            })
        })
        .transpose()
}

const ENTITY_SELECT_BY_ID: &str = r#"
SELECT oid.value AS id, kind.value AS kind, name.value AS name,
       qname.value AS qualified_name, path.value AS repo_relative_path,
       span_path.value AS span_repo_relative_path,
       e.start_line, e.start_column, e.end_line, e.end_column,
       e.content_hash, file.content_hash AS file_hash, extractor.value AS created_from,
       e.confidence, e.metadata_json
FROM entities e
JOIN object_id_lookup oid ON oid.id = e.id_key
JOIN entity_kind_dict kind ON kind.id = e.kind_id
JOIN symbol_dict name ON name.id = e.name_id
JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
JOIN path_dict path ON path.id = e.path_id
LEFT JOIN path_dict span_path ON span_path.id = e.span_path_id
LEFT JOIN files file ON file.file_id = COALESCE(e.file_id, e.path_id)
JOIN extractor_dict extractor ON extractor.id = e.created_from_id
WHERE e.id_key = ?1
"#;

const ENTITY_SELECT_BY_FILE: &str = r#"
SELECT oid.value AS id, kind.value AS kind, name.value AS name,
       qname.value AS qualified_name, path.value AS repo_relative_path,
       span_path.value AS span_repo_relative_path,
       e.start_line, e.start_column, e.end_line, e.end_column,
       e.content_hash, file.content_hash AS file_hash, extractor.value AS created_from,
       e.confidence, e.metadata_json
FROM entities e
JOIN object_id_lookup oid ON oid.id = e.id_key
JOIN entity_kind_dict kind ON kind.id = e.kind_id
JOIN symbol_dict name ON name.id = e.name_id
JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
JOIN path_dict path ON path.id = e.path_id
LEFT JOIN path_dict span_path ON span_path.id = e.span_path_id
LEFT JOIN files file ON file.file_id = COALESCE(e.file_id, e.path_id)
JOIN extractor_dict extractor ON extractor.id = e.created_from_id
WHERE e.path_id = ?1
ORDER BY qname.value, oid.value
"#;

const ENTITY_SELECT_LIST: &str = r#"
SELECT oid.value AS id, kind.value AS kind, name.value AS name,
       qname.value AS qualified_name, path.value AS repo_relative_path,
       span_path.value AS span_repo_relative_path,
       e.start_line, e.start_column, e.end_line, e.end_column,
       e.content_hash, file.content_hash AS file_hash, extractor.value AS created_from,
       e.confidence, e.metadata_json
FROM entities e
JOIN object_id_lookup oid ON oid.id = e.id_key
JOIN entity_kind_dict kind ON kind.id = e.kind_id
JOIN symbol_dict name ON name.id = e.name_id
JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
JOIN path_dict path ON path.id = e.path_id
LEFT JOIN path_dict span_path ON span_path.id = e.span_path_id
LEFT JOIN files file ON file.file_id = COALESCE(e.file_id, e.path_id)
JOIN extractor_dict extractor ON extractor.id = e.created_from_id
ORDER BY qname.value, oid.value
LIMIT ?1
"#;

const ENTITY_SELECT_BY_EXACT_SYMBOL: &str = r#"
SELECT oid.value AS id, kind.value AS kind, name.value AS name,
       qname.value AS qualified_name, path.value AS repo_relative_path,
       span_path.value AS span_repo_relative_path,
       e.start_line, e.start_column, e.end_line, e.end_column,
       e.content_hash, file.content_hash AS file_hash, extractor.value AS created_from,
       e.confidence, e.metadata_json
FROM entities e
JOIN object_id_lookup oid ON oid.id = e.id_key
JOIN entity_kind_dict kind ON kind.id = e.kind_id
JOIN symbol_dict name ON name.id = e.name_id
JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
JOIN path_dict path ON path.id = e.path_id
LEFT JOIN path_dict span_path ON span_path.id = e.span_path_id
LEFT JOIN files file ON file.file_id = COALESCE(e.file_id, e.path_id)
JOIN extractor_dict extractor ON extractor.id = e.created_from_id
WHERE (?1 IS NOT NULL AND e.id_key = ?1)
   OR (?2 IS NOT NULL AND e.name_id = ?2)
   OR (?3 IS NOT NULL AND e.qualified_name_id = ?3)
ORDER BY qname.value, oid.value
"#;

const EDGE_SELECT_BY_ID: &str = r#"
WITH edge_facts AS (
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, derived, provenance_edges_json, metadata_json
    FROM edges_compat
    UNION ALL
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM structural_relations
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, callee_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM callsites
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, argument_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM callsite_args
)
SELECT oid.value AS id, head.value AS head_id, relation.value AS relation,
       tail.value AS tail_id, span_path.value AS span_repo_relative_path,
       e.start_line, e.start_column, e.end_line, e.end_column,
       e.repo_commit, file.content_hash AS file_hash, extractor.value AS extractor,
       e.confidence, exactness.value AS exactness,
       edge_class.value AS edge_class, edge_context.value AS context, e.derived,
       e.provenance_edges_json, e.metadata_json
FROM edge_facts e
LEFT JOIN object_id_lookup oid ON oid.id = e.id_key
JOIN object_id_lookup head ON head.id = e.head_id_key
JOIN relation_kind_dict relation ON relation.id = e.relation_id
JOIN object_id_lookup tail ON tail.id = e.tail_id_key
JOIN path_dict span_path ON span_path.id = e.span_path_id
LEFT JOIN files file ON file.file_id = e.file_id
JOIN extractor_dict extractor ON extractor.id = e.extractor_id
JOIN exactness_dict exactness ON exactness.id = e.exactness_id
LEFT JOIN edge_class_dict edge_class ON edge_class.id = e.edge_class_id
LEFT JOIN edge_context_dict edge_context ON edge_context.id = e.context_id
WHERE e.id_key = ?1
"#;

const EDGE_SELECT_LIST: &str = r#"
WITH edge_facts AS (
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, derived, provenance_edges_json, metadata_json
    FROM edges_compat
    UNION ALL
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM structural_relations
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, callee_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM callsites
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, argument_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM callsite_args
)
SELECT oid.value AS id, head.value AS head_id, relation.value AS relation,
       tail.value AS tail_id, span_path.value AS span_repo_relative_path,
       e.start_line, e.start_column, e.end_line, e.end_column,
       e.repo_commit, file.content_hash AS file_hash, extractor.value AS extractor,
       e.confidence, exactness.value AS exactness,
       edge_class.value AS edge_class, edge_context.value AS context, e.derived,
       e.provenance_edges_json, e.metadata_json
FROM edge_facts e
LEFT JOIN object_id_lookup oid ON oid.id = e.id_key
JOIN object_id_lookup head ON head.id = e.head_id_key
JOIN relation_kind_dict relation ON relation.id = e.relation_id
JOIN object_id_lookup tail ON tail.id = e.tail_id_key
JOIN path_dict span_path ON span_path.id = e.span_path_id
LEFT JOIN files file ON file.file_id = e.file_id
JOIN extractor_dict extractor ON extractor.id = e.extractor_id
JOIN exactness_dict exactness ON exactness.id = e.exactness_id
LEFT JOIN edge_class_dict edge_class ON edge_class.id = e.edge_class_id
LEFT JOIN edge_context_dict edge_context ON edge_context.id = e.context_id
ORDER BY e.id_key
LIMIT ?1
"#;

const EDGE_SELECT_BY_RELATION: &str = r#"
SELECT oid.value AS id, head.value AS head_id, relation.value AS relation,
       tail.value AS tail_id, span_path.value AS span_repo_relative_path,
       e.start_line, e.start_column, e.end_line, e.end_column,
       e.repo_commit, file.content_hash AS file_hash, extractor.value AS extractor,
       e.confidence, exactness.value AS exactness,
       edge_class.value AS edge_class, edge_context.value AS context, e.derived,
       e.provenance_edges_json, e.metadata_json
FROM edges_compat e
LEFT JOIN object_id_lookup oid ON oid.id = e.id_key
JOIN object_id_lookup head ON head.id = e.head_id_key
JOIN relation_kind_dict relation ON relation.id = e.relation_id
JOIN object_id_lookup tail ON tail.id = e.tail_id_key
JOIN path_dict span_path ON span_path.id = e.span_path_id
LEFT JOIN files file ON file.file_id = e.file_id
JOIN extractor_dict extractor ON extractor.id = e.extractor_id
JOIN exactness_dict exactness ON exactness.id = e.exactness_id
LEFT JOIN edge_class_dict edge_class ON edge_class.id = e.edge_class_id
LEFT JOIN edge_context_dict edge_context ON edge_context.id = e.context_id
WHERE e.relation_id = ?1
ORDER BY e.id_key
LIMIT ?2
"#;

const EDGE_SELECT_BY_HEAD_RELATION: &str = r#"
WITH edge_facts AS (
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, derived, provenance_edges_json, metadata_json
    FROM edges_compat
    UNION ALL
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM structural_relations
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, callee_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM callsites
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, argument_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM callsite_args
)
SELECT oid.value AS id, head.value AS head_id, relation.value AS relation,
       tail.value AS tail_id, span_path.value AS span_repo_relative_path,
       e.start_line, e.start_column, e.end_line, e.end_column,
       e.repo_commit, file.content_hash AS file_hash, extractor.value AS extractor,
       e.confidence, exactness.value AS exactness,
       edge_class.value AS edge_class, edge_context.value AS context, e.derived,
       e.provenance_edges_json, e.metadata_json
FROM edge_facts e
LEFT JOIN object_id_lookup oid ON oid.id = e.id_key
JOIN object_id_lookup head ON head.id = e.head_id_key
JOIN relation_kind_dict relation ON relation.id = e.relation_id
JOIN object_id_lookup tail ON tail.id = e.tail_id_key
JOIN path_dict span_path ON span_path.id = e.span_path_id
LEFT JOIN files file ON file.file_id = e.file_id
JOIN extractor_dict extractor ON extractor.id = e.extractor_id
JOIN exactness_dict exactness ON exactness.id = e.exactness_id
LEFT JOIN edge_class_dict edge_class ON edge_class.id = e.edge_class_id
LEFT JOIN edge_context_dict edge_context ON edge_context.id = e.context_id
WHERE e.head_id_key = ?1 AND e.relation_id = ?2
ORDER BY e.id_key
"#;

const EDGE_SELECT_BY_TAIL_RELATION: &str = r#"
WITH edge_facts AS (
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, derived, provenance_edges_json, metadata_json
    FROM edges_compat
    UNION ALL
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM structural_relations
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, callee_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM callsites
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, argument_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json, metadata_json
    FROM callsite_args
)
SELECT oid.value AS id, head.value AS head_id, relation.value AS relation,
       tail.value AS tail_id, span_path.value AS span_repo_relative_path,
       e.start_line, e.start_column, e.end_line, e.end_column,
       e.repo_commit, file.content_hash AS file_hash, extractor.value AS extractor,
       e.confidence, exactness.value AS exactness,
       edge_class.value AS edge_class, edge_context.value AS context, e.derived,
       e.provenance_edges_json, e.metadata_json
FROM edge_facts e
LEFT JOIN object_id_lookup oid ON oid.id = e.id_key
JOIN object_id_lookup head ON head.id = e.head_id_key
JOIN relation_kind_dict relation ON relation.id = e.relation_id
JOIN object_id_lookup tail ON tail.id = e.tail_id_key
JOIN path_dict span_path ON span_path.id = e.span_path_id
LEFT JOIN files file ON file.file_id = e.file_id
JOIN extractor_dict extractor ON extractor.id = e.extractor_id
JOIN exactness_dict exactness ON exactness.id = e.exactness_id
LEFT JOIN edge_class_dict edge_class ON edge_class.id = e.edge_class_id
LEFT JOIN edge_context_dict edge_context ON edge_context.id = e.context_id
WHERE e.tail_id_key = ?1 AND e.relation_id = ?2
ORDER BY e.id_key
"#;

const FILE_SELECT_BY_PATH_ID: &str = r#"
SELECT path.value AS repo_relative_path, f.content_hash AS file_hash, language.value AS language,
       f.size_bytes, f.indexed_at_unix_ms, f.metadata_json
FROM files f
JOIN path_dict path ON path.id = f.path_id
LEFT JOIN language_dict language ON language.id = f.language_id
WHERE f.path_id = ?1
"#;

const FILE_SELECT_LIST: &str = r#"
SELECT path.value AS repo_relative_path, f.content_hash AS file_hash, language.value AS language,
       f.size_bytes, f.indexed_at_unix_ms, f.metadata_json
FROM files f
JOIN path_dict path ON path.id = f.path_id
LEFT JOIN language_dict language ON language.id = f.language_id
ORDER BY path.value
LIMIT ?1
"#;

const ENTITY_DIGEST_SQL: &str = r#"
SELECT 'entity|' || e.id_key || '|' || e.kind_id || '|' || e.name_id || '|' ||
       e.qualified_name_id || '|' || e.path_id || '|' || COALESCE(e.span_path_id, '') || '|' ||
       COALESCE(e.start_line, '') || '|' || COALESCE(e.start_column, '') || '|' ||
       COALESCE(e.end_line, '') || '|' || COALESCE(e.end_column, '') || '|' ||
       lower(hex(e.entity_hash)) || '|' ||
       COALESCE(e.content_hash, '') || '|' || COALESCE(e.file_id, '') || '|' ||
       e.created_from_id || '|' || printf('%.6f', e.confidence) || '|' || e.metadata_json || '|' ||
       COALESCE(e.parent_id, '') || '|' || COALESCE(e.file_id, '') || '|' ||
       COALESCE(e.scope_id, '') || '|' || COALESCE(e.declaration_span_id, '') || '|' ||
       COALESCE(e.structural_flags, 0)
FROM entities e
ORDER BY e.id_key
"#;

const EDGE_DIGEST_SQL: &str = r#"
WITH edge_facts AS (
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, derived, provenance_edges_json,
           confidence_q, resolution_kind_id, context_kind_id, flags_bitset, provenance_id,
           '' AS metadata_json
    FROM edges
    UNION ALL
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json,
           CAST(ROUND(confidence * 1000.0) AS INTEGER) AS confidence_q,
           0 AS resolution_kind_id, context_id AS context_kind_id, 0 AS flags_bitset,
           0 AS provenance_id, metadata_json
    FROM structural_relations
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, callee_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json,
           CAST(ROUND(confidence * 1000.0) AS INTEGER) AS confidence_q,
           0 AS resolution_kind_id, context_id AS context_kind_id, 0 AS flags_bitset,
           0 AS provenance_id, metadata_json
    FROM callsites
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, argument_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json,
           CAST(ROUND(confidence * 1000.0) AS INTEGER) AS confidence_q,
           0 AS resolution_kind_id, context_id AS context_kind_id, 0 AS flags_bitset,
           0 AS provenance_id, metadata_json
    FROM callsite_args
)
SELECT 'edge|' || e.id_key || '|' || e.head_id_key || '|' || e.relation_id || '|' ||
       e.tail_id_key || '|' || e.span_path_id || '|' ||
       e.start_line || '|' || COALESCE(e.start_column, '') || '|' ||
       e.end_line || '|' || COALESCE(e.end_column, '') || '|' ||
       COALESCE(e.repo_commit, '') || '|' || COALESCE(e.file_id, '') || '|' ||
       e.extractor_id || '|' || printf('%.6f', e.confidence) || '|' ||
       e.exactness_id || '|' || e.edge_class_id || '|' || e.context_id || '|' ||
       e.derived || '|' || e.provenance_edges_json || '|' ||
       e.confidence_q || '|' || e.resolution_kind_id || '|' ||
       e.context_kind_id || '|' || e.flags_bitset || '|' || e.provenance_id || '|' ||
       e.metadata_json
FROM edge_facts e
ORDER BY e.id_key
"#;

const FILE_ENTITY_DIGEST_SQL: &str = r#"
SELECT 'entity|' || e.id_key || '|' || e.kind_id || '|' || e.name_id || '|' ||
       e.qualified_name_id || '|' || e.path_id || '|' || COALESCE(e.span_path_id, '') || '|' ||
       COALESCE(e.start_line, '') || '|' || COALESCE(e.start_column, '') || '|' ||
       COALESCE(e.end_line, '') || '|' || COALESCE(e.end_column, '') || '|' ||
       lower(hex(e.entity_hash)) || '|' ||
       COALESCE(e.content_hash, '') || '|' || COALESCE(e.file_id, '') || '|' ||
       e.created_from_id || '|' || printf('%.6f', e.confidence) || '|' || e.metadata_json || '|' ||
       COALESCE(e.parent_id, '') || '|' || COALESCE(e.file_id, '') || '|' ||
       COALESCE(e.scope_id, '') || '|' || COALESCE(e.declaration_span_id, '') || '|' ||
       COALESCE(e.structural_flags, 0)
FROM entities e
WHERE e.path_id = ?1
ORDER BY e.id_key
"#;

const FILE_EDGE_DIGEST_SQL: &str = r#"
WITH edge_facts AS (
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, derived, provenance_edges_json,
           confidence_q, resolution_kind_id, context_kind_id, flags_bitset, provenance_id,
           '' AS metadata_json
    FROM edges
    UNION ALL
    SELECT id_key, head_id_key, relation_id, tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json,
           CAST(ROUND(confidence * 1000.0) AS INTEGER) AS confidence_q,
           0 AS resolution_kind_id, context_id AS context_kind_id, 0 AS flags_bitset,
           0 AS provenance_id, metadata_json
    FROM structural_relations
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, callee_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json,
           CAST(ROUND(confidence * 1000.0) AS INTEGER) AS confidence_q,
           0 AS resolution_kind_id, context_id AS context_kind_id, 0 AS flags_bitset,
           0 AS provenance_id, metadata_json
    FROM callsites
    UNION ALL
    SELECT id_key, callsite_id_key AS head_id_key, relation_id, argument_id_key AS tail_id_key,
           span_path_id, start_line, start_column, end_line, end_column,
           repo_commit, file_id, extractor_id, confidence, exactness_id,
           edge_class_id, context_id, 0 AS derived, '[]' AS provenance_edges_json,
           CAST(ROUND(confidence * 1000.0) AS INTEGER) AS confidence_q,
           0 AS resolution_kind_id, context_id AS context_kind_id, 0 AS flags_bitset,
           0 AS provenance_id, metadata_json
    FROM callsite_args
)
SELECT 'edge|' || e.id_key || '|' || e.head_id_key || '|' || e.relation_id || '|' ||
       e.tail_id_key || '|' || e.span_path_id || '|' ||
       e.start_line || '|' || COALESCE(e.start_column, '') || '|' ||
       e.end_line || '|' || COALESCE(e.end_column, '') || '|' ||
       COALESCE(e.repo_commit, '') || '|' || COALESCE(e.file_id, '') || '|' ||
       e.extractor_id || '|' || printf('%.6f', e.confidence) || '|' ||
       e.exactness_id || '|' || e.edge_class_id || '|' || e.context_id || '|' ||
       e.derived || '|' || e.provenance_edges_json || '|' ||
       e.confidence_q || '|' || e.resolution_kind_id || '|' ||
       e.context_kind_id || '|' || e.flags_bitset || '|' || e.provenance_id || '|' ||
       e.metadata_json
FROM edge_facts e
WHERE e.span_path_id = ?1
   OR e.head_id_key IN (SELECT id_key FROM entities WHERE path_id = ?1)
   OR e.tail_id_key IN (SELECT id_key FROM entities WHERE path_id = ?1)
ORDER BY e.id_key
"#;

const SOURCE_SPAN_SELECT_BY_ID: &str = r#"
SELECT path.value AS span_repo_relative_path, s.start_line, s.start_column,
       s.end_line, s.end_column
FROM source_spans s
JOIN path_dict path ON path.id = s.path_id
WHERE s.id_key = ?1
"#;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS object_id_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL,
    value_hash INTEGER NOT NULL,
    value_len INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS path_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS symbol_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL,
    value_hash INTEGER NOT NULL,
    value_len INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS qname_prefix_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL,
    value_hash INTEGER NOT NULL,
    value_len INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS qualified_name_dict (
    id INTEGER PRIMARY KEY,
    prefix_id INTEGER NOT NULL,
    suffix_id INTEGER NOT NULL,
    value TEXT
);

CREATE TABLE IF NOT EXISTS entity_kind_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS relation_kind_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS extractor_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS exactness_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS resolution_kind_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS edge_class_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS edge_context_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS language_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS edge_provenance_dict (
    id INTEGER PRIMARY KEY,
    value TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS entities (
    id_key INTEGER PRIMARY KEY,
    entity_hash BLOB NOT NULL,
    kind_id INTEGER NOT NULL,
    name_id INTEGER NOT NULL,
    qualified_name_id INTEGER NOT NULL,
    path_id INTEGER NOT NULL,
    span_path_id INTEGER,
    start_line INTEGER,
    start_column INTEGER,
    end_line INTEGER,
    end_column INTEGER,
    content_hash TEXT,
    created_from_id INTEGER NOT NULL,
    confidence REAL NOT NULL,
    metadata_json TEXT NOT NULL,
    parent_id INTEGER,
    file_id INTEGER,
    scope_id INTEGER,
    declaration_span_id INTEGER,
    structural_flags INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS entity_id_history (
    entity_hash BLOB PRIMARY KEY,
    id_key INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS edges (
    id_key INTEGER PRIMARY KEY,
    head_id_key INTEGER NOT NULL,
    relation_id INTEGER NOT NULL,
    tail_id_key INTEGER NOT NULL,
    span_path_id INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    start_column INTEGER,
    end_line INTEGER NOT NULL,
    end_column INTEGER,
    repo_commit TEXT,
    file_id INTEGER NOT NULL,
    extractor_id INTEGER NOT NULL,
    confidence REAL NOT NULL,
    confidence_q INTEGER NOT NULL,
    exactness_id INTEGER NOT NULL,
    resolution_kind_id INTEGER NOT NULL,
    edge_class_id INTEGER NOT NULL,
    context_id INTEGER NOT NULL,
    context_kind_id INTEGER NOT NULL,
    flags_bitset INTEGER NOT NULL,
    derived INTEGER NOT NULL,
    provenance_edges_json TEXT NOT NULL,
    provenance_id INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS edge_debug_metadata (
    edge_id INTEGER PRIMARY KEY,
    metadata_json TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS heuristic_edges (
    id_key INTEGER PRIMARY KEY,
    edge_id TEXT NOT NULL,
    head_id TEXT NOT NULL,
    relation TEXT NOT NULL,
    tail_id TEXT NOT NULL,
    source_span_path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    start_column INTEGER,
    end_line INTEGER NOT NULL,
    end_column INTEGER,
    repo_commit TEXT,
    file_hash TEXT,
    extractor TEXT NOT NULL,
    confidence REAL NOT NULL,
    exactness TEXT NOT NULL,
    edge_class TEXT NOT NULL,
    context TEXT NOT NULL,
    derived INTEGER NOT NULL,
    provenance_edges_json TEXT NOT NULL,
    metadata_json TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS unresolved_references (
    id_key INTEGER PRIMARY KEY,
    reference_id TEXT NOT NULL,
    name TEXT NOT NULL,
    relation TEXT NOT NULL,
    source_span_path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    start_column INTEGER,
    end_line INTEGER NOT NULL,
    end_column INTEGER,
    file_hash TEXT,
    exactness TEXT NOT NULL,
    extractor TEXT NOT NULL,
    metadata_json TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS static_references (
    id_key INTEGER PRIMARY KEY,
    entity_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    qualified_name TEXT NOT NULL,
    repo_relative_path TEXT NOT NULL,
    source_span_path TEXT,
    start_line INTEGER,
    start_column INTEGER,
    end_line INTEGER,
    end_column INTEGER,
    file_hash TEXT,
    created_from TEXT NOT NULL,
    confidence REAL NOT NULL,
    metadata_json TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS extraction_warnings (
    id_key INTEGER PRIMARY KEY,
    repo_relative_path TEXT NOT NULL,
    file_hash TEXT,
    warning TEXT NOT NULL,
    metadata_json TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS structural_relations (
    id_key INTEGER PRIMARY KEY,
    head_id_key INTEGER NOT NULL,
    relation_id INTEGER NOT NULL,
    tail_id_key INTEGER NOT NULL,
    span_path_id INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    start_column INTEGER,
    end_line INTEGER NOT NULL,
    end_column INTEGER,
    repo_commit TEXT,
    file_id INTEGER NOT NULL,
    extractor_id INTEGER NOT NULL,
    confidence REAL NOT NULL,
    exactness_id INTEGER NOT NULL,
    edge_class_id INTEGER NOT NULL,
    context_id INTEGER NOT NULL,
    metadata_json TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS callsites (
    id_key INTEGER PRIMARY KEY,
    callsite_id_key INTEGER NOT NULL,
    caller_id_key INTEGER,
    relation_id INTEGER NOT NULL,
    callee_id_key INTEGER NOT NULL,
    span_path_id INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    start_column INTEGER,
    end_line INTEGER NOT NULL,
    end_column INTEGER,
    repo_commit TEXT,
    file_id INTEGER NOT NULL,
    extractor_id INTEGER NOT NULL,
    confidence REAL NOT NULL,
    exactness_id INTEGER NOT NULL,
    edge_class_id INTEGER NOT NULL,
    context_id INTEGER NOT NULL,
    metadata_json TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS callsite_args (
    id_key INTEGER PRIMARY KEY,
    callsite_id_key INTEGER NOT NULL,
    ordinal INTEGER NOT NULL,
    relation_id INTEGER NOT NULL,
    argument_id_key INTEGER NOT NULL,
    span_path_id INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    start_column INTEGER,
    end_line INTEGER NOT NULL,
    end_column INTEGER,
    repo_commit TEXT,
    file_id INTEGER NOT NULL,
    extractor_id INTEGER NOT NULL,
    confidence REAL NOT NULL,
    exactness_id INTEGER NOT NULL,
    edge_class_id INTEGER NOT NULL,
    context_id INTEGER NOT NULL,
    metadata_json TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS source_spans (
    id_key INTEGER PRIMARY KEY,
    path_id INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    start_column INTEGER,
    end_line INTEGER NOT NULL,
    end_column INTEGER
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS files (
    file_id INTEGER PRIMARY KEY,
    path_id INTEGER NOT NULL UNIQUE,
    content_hash TEXT NOT NULL,
    mtime_unix_ms INTEGER,
    size_bytes INTEGER NOT NULL,
    language_id INTEGER,
    indexed_at_unix_ms INTEGER,
    content_template_id INTEGER,
    metadata_json TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS source_content_template (
    content_template_id INTEGER PRIMARY KEY,
    content_hash TEXT NOT NULL,
    language_id INTEGER NOT NULL,
    extraction_version TEXT NOT NULL,
    canonical_path_id INTEGER NOT NULL,
    metadata_json TEXT NOT NULL,
    UNIQUE(content_hash, language_id, extraction_version)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS template_entities (
    content_template_id INTEGER NOT NULL,
    local_template_entity_id INTEGER NOT NULL,
    kind_id INTEGER NOT NULL,
    name_id INTEGER NOT NULL,
    qualified_name_id INTEGER NOT NULL,
    start_line INTEGER,
    start_column INTEGER,
    end_line INTEGER,
    end_column INTEGER,
    content_hash TEXT,
    created_from_id INTEGER NOT NULL,
    confidence REAL NOT NULL,
    metadata_json TEXT NOT NULL,
    PRIMARY KEY (content_template_id, local_template_entity_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS template_edges (
    content_template_id INTEGER NOT NULL,
    local_template_edge_id INTEGER NOT NULL,
    local_head_entity_id INTEGER NOT NULL,
    relation_id INTEGER NOT NULL,
    local_tail_entity_id INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    start_column INTEGER,
    end_line INTEGER NOT NULL,
    end_column INTEGER,
    repo_commit TEXT,
    extractor_id INTEGER NOT NULL,
    confidence REAL NOT NULL,
    confidence_q INTEGER NOT NULL,
    exactness_id INTEGER NOT NULL,
    resolution_kind_id INTEGER NOT NULL,
    edge_class_id INTEGER NOT NULL,
    context_id INTEGER NOT NULL,
    context_kind_id INTEGER NOT NULL,
    flags_bitset INTEGER NOT NULL,
    derived INTEGER NOT NULL,
    provenance_edges_json TEXT NOT NULL,
    provenance_id INTEGER NOT NULL,
    metadata_json TEXT NOT NULL,
    PRIMARY KEY (content_template_id, local_template_edge_id)
) WITHOUT ROWID;

CREATE VIEW IF NOT EXISTS file_instance AS
SELECT files.file_id,
       files.path_id,
       path_dict.value AS repo_relative_path,
       files.content_template_id,
       files.content_hash,
       files.mtime_unix_ms,
       files.size_bytes,
       files.language_id,
       files.indexed_at_unix_ms,
       files.metadata_json
FROM files
JOIN path_dict ON path_dict.id = files.path_id;

CREATE TABLE IF NOT EXISTS repo_index_state (
    repo_id TEXT PRIMARY KEY,
    repo_root TEXT NOT NULL,
    repo_commit TEXT,
    schema_version INTEGER NOT NULL,
    indexed_at_unix_ms INTEGER,
    files_indexed INTEGER NOT NULL,
    entity_count INTEGER NOT NULL,
    edge_count INTEGER NOT NULL,
    metadata_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS codegraph_db_passport (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    passport_version INTEGER NOT NULL,
    codegraph_schema_version INTEGER NOT NULL,
    storage_mode TEXT NOT NULL,
    index_scope_policy_hash TEXT NOT NULL,
    scope_policy_json TEXT NOT NULL,
    canonical_repo_root TEXT NOT NULL,
    git_remote TEXT,
    worktree_root TEXT,
    repo_head TEXT,
    source_discovery_policy_version TEXT NOT NULL,
    codegraph_build_version TEXT,
    last_successful_index_timestamp INTEGER,
    last_completed_run_id TEXT,
    last_run_status TEXT NOT NULL,
    integrity_gate_result TEXT NOT NULL,
    files_seen INTEGER NOT NULL,
    files_indexed INTEGER NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS path_evidence (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    summary TEXT,
    metapath_json TEXT NOT NULL,
    edges_json TEXT NOT NULL,
    source_spans_json TEXT NOT NULL,
    exactness TEXT NOT NULL,
    length INTEGER NOT NULL,
    confidence REAL NOT NULL,
    metadata_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS path_evidence_lookup (
    path_id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    task_class TEXT NOT NULL,
    relation_signature TEXT NOT NULL,
    length INTEGER NOT NULL,
    confidence REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS path_evidence_edges (
    path_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    edge_id TEXT NOT NULL,
    head_id TEXT NOT NULL,
    relation TEXT NOT NULL,
    tail_id TEXT NOT NULL,
    source_span_path TEXT,
    exactness TEXT,
    confidence REAL,
    derived INTEGER NOT NULL DEFAULT 0,
    edge_class TEXT,
    context TEXT,
    provenance_edges_json TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (path_id, ordinal)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS path_evidence_debug_metadata (
    path_id TEXT PRIMARY KEY,
    metadata_json TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS path_evidence_symbols (
    entity_id TEXT NOT NULL,
    path_id TEXT NOT NULL,
    role TEXT NOT NULL,
    PRIMARY KEY (entity_id, path_id, role)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS path_evidence_tests (
    path_id TEXT NOT NULL,
    test_id TEXT NOT NULL,
    relation TEXT NOT NULL,
    PRIMARY KEY (path_id, test_id, relation)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS path_evidence_files (
    file_id TEXT NOT NULL,
    path_id TEXT NOT NULL,
    PRIMARY KEY (file_id, path_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS file_entities (
    file_id TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    PRIMARY KEY (file_id, entity_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS file_edges (
    file_id TEXT NOT NULL,
    edge_id TEXT NOT NULL,
    PRIMARY KEY (file_id, edge_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS file_source_spans (
    file_id TEXT NOT NULL,
    span_id TEXT NOT NULL,
    PRIMARY KEY (file_id, span_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS file_path_evidence (
    file_id TEXT NOT NULL,
    path_id TEXT NOT NULL,
    PRIMARY KEY (file_id, path_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS file_fts_rows (
    file_id TEXT NOT NULL,
    rowid INTEGER NOT NULL,
    kind TEXT NOT NULL,
    object_id TEXT NOT NULL,
    PRIMARY KEY (file_id, rowid)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS file_graph_digests (
    file_id TEXT PRIMARY KEY,
    digest TEXT NOT NULL,
    updated_at_unix_ms INTEGER
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS repo_graph_digest (
    id TEXT PRIMARY KEY,
    digest TEXT NOT NULL,
    updated_at_unix_ms INTEGER
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS derived_edges (
    id TEXT PRIMARY KEY,
    head_id TEXT NOT NULL,
    relation TEXT NOT NULL,
    tail_id TEXT NOT NULL,
    provenance_edges_json TEXT NOT NULL,
    exactness TEXT NOT NULL,
    confidence REAL NOT NULL,
    metadata_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS bench_tasks (
    id TEXT PRIMARY KEY,
    task_family TEXT,
    payload_json TEXT NOT NULL,
    created_at_unix_ms INTEGER
);

CREATE TABLE IF NOT EXISTS bench_runs (
    id TEXT PRIMARY KEY,
    task_id TEXT,
    baseline TEXT,
    metrics_json TEXT NOT NULL,
    created_at_unix_ms INTEGER
);

CREATE TABLE IF NOT EXISTS retrieval_traces (
    id TEXT PRIMARY KEY,
    task TEXT,
    trace_json TEXT NOT NULL,
    created_at_unix_ms INTEGER
);

CREATE VIRTUAL TABLE IF NOT EXISTS stage0_fts USING fts5(
    kind UNINDEXED,
    id UNINDEXED,
    repo_relative_path UNINDEXED,
    line UNINDEXED,
    title,
    body,
    tokenize = 'unicode61'
);

CREATE VIEW IF NOT EXISTS qualified_name_lookup AS
SELECT qname.id,
       CASE
           WHEN qname.value IS NOT NULL AND qname.value != '' THEN qname.value
           WHEN prefix.value IS NULL OR prefix.value = '' THEN suffix.value
           ELSE prefix.value || '.' || suffix.value
       END AS value,
       qname.prefix_id,
       qname.suffix_id
FROM qualified_name_dict qname
JOIN qname_prefix_dict prefix ON prefix.id = qname.prefix_id
JOIN symbol_dict suffix ON suffix.id = qname.suffix_id;

CREATE VIEW IF NOT EXISTS qualified_name_debug AS
SELECT qname.id,
       lookup.value,
       prefix.value AS prefix,
       suffix.value AS suffix,
       qname.prefix_id,
       qname.suffix_id
FROM qualified_name_dict qname
JOIN qualified_name_lookup lookup ON lookup.id = qname.id
JOIN qname_prefix_dict prefix ON prefix.id = qname.prefix_id
JOIN symbol_dict suffix ON suffix.id = qname.suffix_id;

CREATE VIEW IF NOT EXISTS object_id_lookup AS
SELECT e.id_key AS id,
       COALESCE(debug.value, 'repo://e/' || lower(hex(e.entity_hash))) AS value
FROM entities e
LEFT JOIN object_id_dict debug ON debug.id = e.id_key
UNION ALL
SELECT debug.id, debug.value
FROM object_id_dict debug
WHERE NOT EXISTS (
    SELECT 1 FROM entities e WHERE e.id_key = debug.id
)
UNION ALL
SELECT history.id_key AS id,
       'repo://e/' || lower(hex(history.entity_hash)) AS value
FROM entity_id_history history
WHERE NOT EXISTS (
    SELECT 1 FROM entities e WHERE e.id_key = history.id_key
)
  AND NOT EXISTS (
    SELECT 1 FROM object_id_dict debug WHERE debug.id = history.id_key
);

CREATE VIEW IF NOT EXISTS object_id_debug AS
SELECT lookup.id, lookup.value, debug.value_hash, debug.value_len,
       entity.entity_hash
FROM object_id_lookup lookup
LEFT JOIN object_id_dict debug ON debug.id = lookup.id
LEFT JOIN entities entity ON entity.id_key = lookup.id;

DROP INDEX IF EXISTS idx_entities_kind;
DROP INDEX IF EXISTS idx_entities_name;
DROP INDEX IF EXISTS idx_entities_qualified_name;
DROP INDEX IF EXISTS idx_entities_repo_relative_path;
DROP INDEX IF EXISTS idx_edges_head_relation;
DROP INDEX IF EXISTS idx_edges_tail_relation;
DROP INDEX IF EXISTS idx_edges_relation;
DROP INDEX IF EXISTS idx_edges_head_relation_tail;
DROP INDEX IF EXISTS idx_heuristic_edges_relation;
DROP INDEX IF EXISTS idx_heuristic_edges_span_path;
DROP INDEX IF EXISTS idx_static_references_path;
DROP INDEX IF EXISTS idx_unresolved_references_path;
DROP INDEX IF EXISTS idx_extraction_warnings_path;
DROP INDEX IF EXISTS idx_entities_file_id;
DROP INDEX IF EXISTS idx_entities_parent_id;
DROP INDEX IF EXISTS idx_entities_scope_id;
DROP INDEX IF EXISTS idx_structural_head_relation;
DROP INDEX IF EXISTS idx_structural_tail_relation;
DROP INDEX IF EXISTS idx_structural_span_path;
DROP INDEX IF EXISTS idx_callsites_callsite_relation;
DROP INDEX IF EXISTS idx_callsites_callee;
DROP INDEX IF EXISTS idx_callsites_span_path;
DROP INDEX IF EXISTS idx_callsite_args_callsite_relation;
DROP INDEX IF EXISTS idx_callsite_args_argument;
DROP INDEX IF EXISTS idx_callsite_args_span_path;
DROP INDEX IF EXISTS idx_files_repo_relative_path;
DROP INDEX IF EXISTS idx_files_file_hash;
DROP INDEX IF EXISTS idx_entities_entity_hash;
DROP INDEX IF EXISTS idx_entities_entity_hash_build;
DROP INDEX IF EXISTS idx_files_content_template;
DROP INDEX IF EXISTS idx_template_edges_head_relation;
DROP INDEX IF EXISTS idx_template_edges_tail_relation;
CREATE INDEX IF NOT EXISTS idx_object_id_dict_hash ON object_id_dict(value_hash, value_len);
CREATE INDEX IF NOT EXISTS idx_symbol_dict_hash ON symbol_dict(value_hash, value_len);
CREATE INDEX IF NOT EXISTS idx_qname_prefix_dict_hash ON qname_prefix_dict(value_hash, value_len);
CREATE UNIQUE INDEX IF NOT EXISTS idx_qualified_name_parts ON qualified_name_dict(prefix_id, suffix_id);
CREATE INDEX IF NOT EXISTS idx_entities_path ON entities(path_id);
CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name_id);
CREATE INDEX IF NOT EXISTS idx_entities_qname ON entities(qualified_name_id);
CREATE INDEX IF NOT EXISTS idx_edges_head_relation ON edges(head_id_key, relation_id);
CREATE INDEX IF NOT EXISTS idx_edges_tail_relation ON edges(tail_id_key, relation_id);
CREATE INDEX IF NOT EXISTS idx_edges_span_path ON edges(span_path_id);
CREATE INDEX IF NOT EXISTS idx_heuristic_edges_relation ON heuristic_edges(relation, exactness);
CREATE INDEX IF NOT EXISTS idx_heuristic_edges_span_path ON heuristic_edges(source_span_path);
CREATE INDEX IF NOT EXISTS idx_static_references_path ON static_references(repo_relative_path);
CREATE INDEX IF NOT EXISTS idx_unresolved_references_path ON unresolved_references(source_span_path);
CREATE INDEX IF NOT EXISTS idx_extraction_warnings_path ON extraction_warnings(repo_relative_path);
CREATE INDEX IF NOT EXISTS idx_source_spans_path ON source_spans(path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_source ON path_evidence(source, length, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_target ON path_evidence(target, length, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_lookup_source ON path_evidence_lookup(source_id, task_class, relation_signature, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_lookup_target ON path_evidence_lookup(target_id, task_class, relation_signature, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_lookup_signature ON path_evidence_lookup(relation_signature, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_symbols_entity ON path_evidence_symbols(entity_id, path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_symbols_path ON path_evidence_symbols(path_id, entity_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_edges_path_ordinal ON path_evidence_edges(path_id, ordinal);
CREATE INDEX IF NOT EXISTS idx_path_evidence_edges_edge ON path_evidence_edges(edge_id, path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_tests_test ON path_evidence_tests(test_id, path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_files_file ON path_evidence_files(file_id, path_id);
CREATE INDEX IF NOT EXISTS idx_file_entities_entity ON file_entities(entity_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_edges_edge ON file_edges(edge_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_source_spans_span ON file_source_spans(span_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_path_evidence_path ON file_path_evidence(path_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_fts_rows_object ON file_fts_rows(object_id, kind, file_id);
CREATE INDEX IF NOT EXISTS idx_retrieval_traces_created ON retrieval_traces(created_at_unix_ms);
"#;

const BULK_INDEX_DROP_SQL: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA busy_timeout = 5000;
"#;

const ATOMIC_COLD_BULK_INDEX_PRAGMAS_SQL: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = OFF;
PRAGMA synchronous = OFF;
PRAGMA temp_store = MEMORY;
PRAGMA cache_size = -262144;
PRAGMA cache_spill = OFF;
PRAGMA busy_timeout = 5000;
"#;

const BULK_INDEX_DROP_LOOKUP_INDEXES_SQL: &str = r#"
DROP INDEX IF EXISTS idx_entities_entity_hash;
DROP INDEX IF EXISTS idx_entities_entity_hash_build;
DROP INDEX IF EXISTS idx_entities_path;
DROP INDEX IF EXISTS idx_entities_name;
DROP INDEX IF EXISTS idx_entities_qname;
DROP INDEX IF EXISTS idx_entities_file_id;
DROP INDEX IF EXISTS idx_entities_parent_id;
DROP INDEX IF EXISTS idx_entities_scope_id;
DROP INDEX IF EXISTS idx_edges_head_relation;
DROP INDEX IF EXISTS idx_edges_tail_relation;
DROP INDEX IF EXISTS idx_edges_span_path;
DROP INDEX IF EXISTS idx_heuristic_edges_relation;
DROP INDEX IF EXISTS idx_heuristic_edges_span_path;
DROP INDEX IF EXISTS idx_static_references_path;
DROP INDEX IF EXISTS idx_unresolved_references_path;
DROP INDEX IF EXISTS idx_extraction_warnings_path;
DROP INDEX IF EXISTS idx_structural_head_relation;
DROP INDEX IF EXISTS idx_structural_tail_relation;
DROP INDEX IF EXISTS idx_structural_span_path;
DROP INDEX IF EXISTS idx_callsites_callsite_relation;
DROP INDEX IF EXISTS idx_callsites_callee;
DROP INDEX IF EXISTS idx_callsites_span_path;
DROP INDEX IF EXISTS idx_callsite_args_callsite_relation;
DROP INDEX IF EXISTS idx_callsite_args_argument;
DROP INDEX IF EXISTS idx_callsite_args_span_path;
DROP INDEX IF EXISTS idx_source_spans_path;
DROP INDEX IF EXISTS idx_path_evidence_source;
DROP INDEX IF EXISTS idx_path_evidence_target;
DROP INDEX IF EXISTS idx_path_evidence_lookup_source;
DROP INDEX IF EXISTS idx_path_evidence_lookup_target;
DROP INDEX IF EXISTS idx_path_evidence_lookup_signature;
DROP INDEX IF EXISTS idx_path_evidence_symbols_entity;
DROP INDEX IF EXISTS idx_path_evidence_symbols_path;
DROP INDEX IF EXISTS idx_path_evidence_edges_path_ordinal;
DROP INDEX IF EXISTS idx_path_evidence_edges_edge;
DROP INDEX IF EXISTS idx_path_evidence_tests_test;
DROP INDEX IF EXISTS idx_path_evidence_files_file;
DROP INDEX IF EXISTS idx_file_entities_entity;
DROP INDEX IF EXISTS idx_file_edges_edge;
DROP INDEX IF EXISTS idx_file_source_spans_span;
DROP INDEX IF EXISTS idx_file_path_evidence_path;
DROP INDEX IF EXISTS idx_file_fts_rows_object;
DROP INDEX IF EXISTS idx_files_content_template;
DROP INDEX IF EXISTS idx_template_edges_head_relation;
DROP INDEX IF EXISTS idx_template_edges_tail_relation;
CREATE INDEX IF NOT EXISTS idx_entities_entity_hash_build ON entities(entity_hash);
"#;

const BULK_INDEX_CREATE_FAST_SQL: &str = r#"
DROP INDEX IF EXISTS idx_entities_kind;
DROP INDEX IF EXISTS idx_entities_name;
DROP INDEX IF EXISTS idx_entities_qualified_name;
DROP INDEX IF EXISTS idx_entities_repo_relative_path;
DROP INDEX IF EXISTS idx_edges_head_relation;
DROP INDEX IF EXISTS idx_edges_tail_relation;
DROP INDEX IF EXISTS idx_heuristic_edges_relation;
DROP INDEX IF EXISTS idx_heuristic_edges_span_path;
DROP INDEX IF EXISTS idx_static_references_path;
DROP INDEX IF EXISTS idx_unresolved_references_path;
DROP INDEX IF EXISTS idx_extraction_warnings_path;
DROP INDEX IF EXISTS idx_files_repo_relative_path;
DROP INDEX IF EXISTS idx_files_content_template;
DROP INDEX IF EXISTS idx_template_edges_head_relation;
DROP INDEX IF EXISTS idx_template_edges_tail_relation;
DROP INDEX IF EXISTS idx_entities_entity_hash_build;
CREATE INDEX IF NOT EXISTS idx_entities_path ON entities(path_id);
CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name_id);
CREATE INDEX IF NOT EXISTS idx_entities_qname ON entities(qualified_name_id);
CREATE INDEX IF NOT EXISTS idx_edges_head_relation ON edges(head_id_key, relation_id);
CREATE INDEX IF NOT EXISTS idx_edges_tail_relation ON edges(tail_id_key, relation_id);
CREATE INDEX IF NOT EXISTS idx_edges_span_path ON edges(span_path_id);
CREATE INDEX IF NOT EXISTS idx_heuristic_edges_relation ON heuristic_edges(relation, exactness);
CREATE INDEX IF NOT EXISTS idx_heuristic_edges_span_path ON heuristic_edges(source_span_path);
CREATE INDEX IF NOT EXISTS idx_static_references_path ON static_references(repo_relative_path);
CREATE INDEX IF NOT EXISTS idx_unresolved_references_path ON unresolved_references(source_span_path);
CREATE INDEX IF NOT EXISTS idx_extraction_warnings_path ON extraction_warnings(repo_relative_path);
CREATE INDEX IF NOT EXISTS idx_source_spans_path ON source_spans(path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_source ON path_evidence(source, length, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_target ON path_evidence(target, length, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_lookup_source ON path_evidence_lookup(source_id, task_class, relation_signature, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_lookup_target ON path_evidence_lookup(target_id, task_class, relation_signature, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_lookup_signature ON path_evidence_lookup(relation_signature, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_symbols_entity ON path_evidence_symbols(entity_id, path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_symbols_path ON path_evidence_symbols(path_id, entity_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_edges_path_ordinal ON path_evidence_edges(path_id, ordinal);
CREATE INDEX IF NOT EXISTS idx_path_evidence_edges_edge ON path_evidence_edges(edge_id, path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_tests_test ON path_evidence_tests(test_id, path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_files_file ON path_evidence_files(file_id, path_id);
CREATE INDEX IF NOT EXISTS idx_file_entities_entity ON file_entities(entity_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_edges_edge ON file_edges(edge_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_source_spans_span ON file_source_spans(span_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_path_evidence_path ON file_path_evidence(path_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_fts_rows_object ON file_fts_rows(object_id, kind, file_id);
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA cache_spill = ON;
PRAGMA foreign_keys = ON;
"#;

const BULK_INDEX_CREATE_SQL: &str = r#"
DROP INDEX IF EXISTS idx_entities_kind;
DROP INDEX IF EXISTS idx_entities_name;
DROP INDEX IF EXISTS idx_entities_qualified_name;
DROP INDEX IF EXISTS idx_entities_repo_relative_path;
DROP INDEX IF EXISTS idx_edges_head_relation;
DROP INDEX IF EXISTS idx_edges_tail_relation;
DROP INDEX IF EXISTS idx_heuristic_edges_relation;
DROP INDEX IF EXISTS idx_heuristic_edges_span_path;
DROP INDEX IF EXISTS idx_static_references_path;
DROP INDEX IF EXISTS idx_unresolved_references_path;
DROP INDEX IF EXISTS idx_extraction_warnings_path;
DROP INDEX IF EXISTS idx_files_repo_relative_path;
DROP INDEX IF EXISTS idx_files_content_template;
DROP INDEX IF EXISTS idx_template_edges_head_relation;
DROP INDEX IF EXISTS idx_template_edges_tail_relation;
DROP INDEX IF EXISTS idx_entities_entity_hash_build;
CREATE INDEX IF NOT EXISTS idx_entities_path ON entities(path_id);
CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name_id);
CREATE INDEX IF NOT EXISTS idx_entities_qname ON entities(qualified_name_id);
CREATE INDEX IF NOT EXISTS idx_edges_head_relation ON edges(head_id_key, relation_id);
CREATE INDEX IF NOT EXISTS idx_edges_tail_relation ON edges(tail_id_key, relation_id);
CREATE INDEX IF NOT EXISTS idx_edges_span_path ON edges(span_path_id);
CREATE INDEX IF NOT EXISTS idx_heuristic_edges_relation ON heuristic_edges(relation, exactness);
CREATE INDEX IF NOT EXISTS idx_heuristic_edges_span_path ON heuristic_edges(source_span_path);
CREATE INDEX IF NOT EXISTS idx_static_references_path ON static_references(repo_relative_path);
CREATE INDEX IF NOT EXISTS idx_unresolved_references_path ON unresolved_references(source_span_path);
CREATE INDEX IF NOT EXISTS idx_extraction_warnings_path ON extraction_warnings(repo_relative_path);
CREATE INDEX IF NOT EXISTS idx_source_spans_path ON source_spans(path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_source ON path_evidence(source, length, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_target ON path_evidence(target, length, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_lookup_source ON path_evidence_lookup(source_id, task_class, relation_signature, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_lookup_target ON path_evidence_lookup(target_id, task_class, relation_signature, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_lookup_signature ON path_evidence_lookup(relation_signature, confidence DESC);
CREATE INDEX IF NOT EXISTS idx_path_evidence_symbols_entity ON path_evidence_symbols(entity_id, path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_symbols_path ON path_evidence_symbols(path_id, entity_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_edges_path_ordinal ON path_evidence_edges(path_id, ordinal);
CREATE INDEX IF NOT EXISTS idx_path_evidence_edges_edge ON path_evidence_edges(edge_id, path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_tests_test ON path_evidence_tests(test_id, path_id);
CREATE INDEX IF NOT EXISTS idx_path_evidence_files_file ON path_evidence_files(file_id, path_id);
CREATE INDEX IF NOT EXISTS idx_file_entities_entity ON file_entities(entity_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_edges_edge ON file_edges(edge_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_source_spans_span ON file_source_spans(span_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_path_evidence_path ON file_path_evidence(path_id, file_id);
CREATE INDEX IF NOT EXISTS idx_file_fts_rows_object ON file_fts_rows(object_id, kind, file_id);
ANALYZE;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA foreign_keys = ON;
PRAGMA wal_checkpoint(TRUNCATE);
"#;

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    use codegraph_core::{
        stable_edge_id, stable_entity_id, DerivedClosureEdge, Edge, EdgeClass, EdgeContext, Entity,
        EntityKind, Exactness, FileRecord, PathEvidence, RelationKind, RepoIndexState, SourceSpan,
    };
    use rusqlite::Connection;

    use super::{
        count_rows, intern_object_id, intern_qualified_name, lookup_object_id,
        lookup_qualified_name, migrate_dictionary_compaction, register_sqlite_functions,
        stable_text_hash_key, stable_text_len, table_has_column, GraphStore, RetrievalTraceRecord,
        SqliteGraphStore, StoreError, TextSearchKind, MAX_QNAME_PREFIX_BYTES,
        MAX_QUALIFIED_NAME_BYTES, SCHEMA_SQL, SCHEMA_VERSION,
    };

    fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("expected Ok(..), got Err({error:?})"),
        }
    }

    fn store() -> SqliteGraphStore {
        ok(SqliteGraphStore::open_in_memory())
    }

    fn temp_db_path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "codegraph-store-pragmas-{}-{nanos}.sqlite",
            std::process::id()
        ))
    }

    fn remove_temp_db_family(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = fs::remove_file(path.with_extension("sqlite-shm"));
    }

    fn pragma_i64(store: &SqliteGraphStore, pragma_sql: &str) -> i64 {
        ok(store
            .connection
            .query_row(pragma_sql, [], |row| row.get::<_, i64>(0)))
    }

    fn span() -> SourceSpan {
        SourceSpan::new("src/auth.ts", 82, 91)
    }

    fn entity(id_suffix: &str) -> Entity {
        Entity {
            id: stable_entity_id("src/auth.ts", format!("method:AuthService.{id_suffix}")),
            kind: EntityKind::Method,
            name: id_suffix.to_string(),
            qualified_name: format!("AuthService.{id_suffix}"),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(span()),
            content_hash: None,
            file_hash: Some("sha256:file".to_string()),
            created_from: "test".to_string(),
            confidence: 1.0,
            metadata: Default::default(),
        }
    }

    fn sample_edge(id_suffix: &str, head_id: &str, relation: RelationKind, tail_id: &str) -> Edge {
        let span = span();
        Edge {
            id: stable_edge_id(head_id, relation, tail_id, &span).replace("edge://", id_suffix),
            head_id: head_id.to_string(),
            relation,
            tail_id: tail_id.to_string(),
            source_span: span,
            repo_commit: Some("abc123".to_string()),
            file_hash: Some("sha256:file".to_string()),
            extractor: "test".to_string(),
            confidence: 1.0,
            exactness: Exactness::ParserVerified,
            edge_class: EdgeClass::BaseExact,
            context: EdgeContext::Production,
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Default::default(),
        }
    }

    #[test]
    fn migration_creates_required_tables_indexes_and_schema_version() {
        let store = store();

        for table in [
            "object_id_dict",
            "path_dict",
            "symbol_dict",
            "qname_prefix_dict",
            "qualified_name_dict",
            "entity_kind_dict",
            "entity_id_history",
            "relation_kind_dict",
            "extractor_dict",
            "exactness_dict",
            "edge_class_dict",
            "edge_context_dict",
            "language_dict",
            "entities",
            "edges",
            "heuristic_edges",
            "unresolved_references",
            "static_references",
            "extraction_warnings",
            "structural_relations",
            "callsites",
            "callsite_args",
            "source_spans",
            "files",
            "repo_index_state",
            "path_evidence",
            "path_evidence_debug_metadata",
            "path_evidence_lookup",
            "path_evidence_edges",
            "path_evidence_symbols",
            "path_evidence_tests",
            "path_evidence_files",
            "file_entities",
            "file_edges",
            "file_source_spans",
            "file_path_evidence",
            "file_fts_rows",
            "file_graph_digests",
            "repo_graph_digest",
            "derived_edges",
            "bench_tasks",
            "bench_runs",
            "retrieval_traces",
            "stage0_fts",
        ] {
            assert!(ok(store.table_exists(table)), "missing table {table}");
        }

        for index in [
            "idx_object_id_dict_hash",
            "idx_symbol_dict_hash",
            "idx_qname_prefix_dict_hash",
            "idx_qualified_name_parts",
            "idx_retrieval_traces_created",
            "idx_path_evidence_source",
            "idx_path_evidence_target",
            "idx_path_evidence_lookup_source",
            "idx_path_evidence_lookup_target",
            "idx_path_evidence_symbols_entity",
            "idx_path_evidence_edges_path_ordinal",
            "idx_path_evidence_edges_edge",
            "idx_path_evidence_tests_test",
            "idx_path_evidence_files_file",
            "idx_heuristic_edges_relation",
            "idx_heuristic_edges_span_path",
            "idx_static_references_path",
            "idx_unresolved_references_path",
            "idx_extraction_warnings_path",
            "idx_file_entities_entity",
            "idx_file_edges_edge",
            "idx_file_source_spans_span",
            "idx_file_path_evidence_path",
            "idx_file_fts_rows_object",
        ] {
            assert!(ok(store.index_exists(index)), "missing index {index}");
        }
        assert!(!ok(store.index_exists("idx_files_content_template")));

        for view in [
            "qualified_name_lookup",
            "qualified_name_debug",
            "object_id_lookup",
            "object_id_debug",
        ] {
            assert!(
                ok(super::exists_in_sqlite_master(
                    &store.connection,
                    "view",
                    view
                )),
                "missing view {view}"
            );
        }

        assert_eq!(ok(store.schema_version()), SCHEMA_VERSION);
    }

    #[test]
    fn migration_repairs_stale_file_instance_view_before_file_normalization() {
        let db_path = temp_db_path();
        remove_temp_db_family(&db_path);
        {
            let connection = ok(Connection::open(&db_path));
            ok(connection.execute_batch(
                "
                CREATE TABLE path_dict (
                    id INTEGER PRIMARY KEY,
                    value TEXT NOT NULL UNIQUE
                );
                CREATE TABLE files (
                    path_id INTEGER PRIMARY KEY,
                    file_hash TEXT NOT NULL,
                    language_id INTEGER,
                    size_bytes INTEGER NOT NULL,
                    indexed_at_unix_ms INTEGER,
                    metadata_json TEXT NOT NULL
                ) WITHOUT ROWID;
                INSERT INTO path_dict(id, value) VALUES (1, 'src/auth.ts');
                INSERT INTO files(
                    path_id, file_hash, language_id, size_bytes,
                    indexed_at_unix_ms, metadata_json
                ) VALUES (1, 'sha256:legacy', NULL, 123, 10, '{}');
                CREATE VIEW file_instance AS
                SELECT files.file_id,
                       files.path_id,
                       path_dict.value AS repo_relative_path,
                       files.content_template_id,
                       files.content_hash,
                       files.mtime_unix_ms,
                       files.size_bytes,
                       files.language_id,
                       files.indexed_at_unix_ms,
                       files.metadata_json
                FROM files
                JOIN path_dict ON path_dict.id = files.path_id;
                PRAGMA user_version = 4;
                ",
            ));
        }

        let store = ok(SqliteGraphStore::open(&db_path));
        assert_eq!(ok(store.schema_version()), SCHEMA_VERSION);
        assert!(ok(table_has_column(&store.connection, "files", "file_id")));
        let row: (i64, String, String) = ok(store.connection.query_row(
            "SELECT file_id, repo_relative_path, content_hash FROM file_instance",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ));
        assert_eq!(
            row,
            (1, "src/auth.ts".to_string(), "sha256:legacy".to_string())
        );
        remove_temp_db_family(&db_path);
    }

    #[test]
    fn compact_schema_uses_dictionary_backed_integer_rows() {
        let store = store();
        let entity = entity("login");
        let edge = sample_edge(
            "edge-login-self",
            &entity.id,
            RelationKind::Calls,
            &entity.id,
        );
        let file = FileRecord {
            repo_relative_path: "src/auth.ts".to_string(),
            file_hash: "sha256:file".to_string(),
            language: Some("typescript".to_string()),
            size_bytes: 1234,
            indexed_at_unix_ms: Some(10),
            metadata: Default::default(),
        };

        ok(store.upsert_file(&file));
        ok(store.upsert_entity(&entity));
        ok(store.upsert_edge(&edge));
        ok(store.upsert_source_span("span-auth-login", &span()));

        assert!(ok(table_has_column(
            &store.connection,
            "entities",
            "id_key"
        )));
        assert!(ok(table_has_column(
            &store.connection,
            "entities",
            "entity_hash"
        )));
        assert!(!ok(table_has_column(&store.connection, "entities", "id")));
        assert!(ok(table_has_column(
            &store.connection,
            "edges",
            "head_id_key"
        )));
        assert!(ok(table_has_column(
            &store.connection,
            "edges",
            "edge_class_id"
        )));
        assert!(ok(table_has_column(
            &store.connection,
            "edges",
            "context_id"
        )));
        assert!(!ok(table_has_column(&store.connection, "edges", "head_id")));
        assert!(ok(table_has_column(
            &store.connection,
            "source_spans",
            "path_id"
        )));
        assert!(!ok(table_has_column(
            &store.connection,
            "source_spans",
            "repo_relative_path"
        )));
        assert!(ok(table_has_column(&store.connection, "files", "path_id")));
        assert!(ok(table_has_column(&store.connection, "files", "file_id")));
        assert!(ok(table_has_column(
            &store.connection,
            "files",
            "content_hash"
        )));
        assert!(!ok(table_has_column(
            &store.connection,
            "files",
            "file_hash"
        )));
        assert!(!ok(table_has_column(
            &store.connection,
            "files",
            "repo_relative_path"
        )));
        for table in [
            "entities",
            "edges",
            "structural_relations",
            "callsites",
            "callsite_args",
        ] {
            assert!(
                !ok(table_has_column(&store.connection, table, "file_hash")),
                "{table} should reference files.file_id instead of storing file_hash text"
            );
        }
        for table in [
            "entities",
            "edges",
            "structural_relations",
            "callsites",
            "callsite_args",
        ] {
            assert!(
                ok(table_has_column(&store.connection, table, "file_id")),
                "{table} should carry compact file_id"
            );
        }
        let stored_file_hashes: i64 = ok(store.connection.query_row(
            "SELECT COUNT(*) FROM files WHERE content_hash = 'sha256:file'",
            [],
            |row| row.get(0),
        ));
        assert_eq!(stored_file_hashes, 1);
        assert!(ok(count_rows(&store.connection, "path_dict")) > 0);
        let canonical_entity_object_rows: i64 = ok(store.connection.query_row(
            "SELECT COUNT(*) FROM object_id_dict WHERE value LIKE 'repo://e/%'",
            [],
            |row| row.get(0),
        ));
        assert_eq!(canonical_entity_object_rows, 0);
        assert!(ok(count_rows(&store.connection, "relation_kind_dict")) > 0);
        assert!(ok(count_rows(&store.connection, "edge_class_dict")) > 0);
        assert!(ok(count_rows(&store.connection, "edge_context_dict")) > 0);
    }

    #[test]
    fn compact_qualified_names_are_reconstructed_from_component_ids() {
        let store = store();
        let entity = entity("login");

        ok(store.upsert_entity(&entity));

        let raw_qname: Option<String> = ok(store.connection.query_row(
            "SELECT value FROM qualified_name_dict WHERE id = (SELECT qualified_name_id FROM entities WHERE id_key = (SELECT id FROM object_id_lookup WHERE value = ?1))",
            [entity.id.as_str()],
            |row| row.get(0),
        ));
        assert_eq!(raw_qname, None);

        let reconstructed: String = ok(store.connection.query_row(
            "SELECT value FROM qualified_name_lookup WHERE id = (SELECT qualified_name_id FROM entities WHERE id_key = (SELECT id FROM object_id_lookup WHERE value = ?1))",
            [entity.id.as_str()],
            |row| row.get(0),
        ));
        assert_eq!(reconstructed, entity.qualified_name);
        let reconstructed_id: String = ok(store.connection.query_row(
            "SELECT value FROM object_id_lookup WHERE id = (SELECT id_key FROM entities LIMIT 1)",
            [],
            |row| row.get(0),
        ));
        assert_eq!(reconstructed_id, entity.id);
        assert_eq!(
            ok(store.connection.query_row(
                "SELECT COUNT(*) FROM object_id_dict WHERE value = ?1",
                [entity.id.as_str()],
                |row| row.get::<_, i64>(0),
            )),
            0
        );
        assert!(ok(lookup_qualified_name(
            &store.connection,
            &entity.qualified_name
        ))
        .is_some());
        assert_eq!(
            ok(store.get_entity(&entity.id))
                .expect("entity")
                .qualified_name,
            entity.qualified_name
        );
    }

    #[test]
    fn hashed_dictionary_lookup_uses_exact_collision_verification() {
        let store = store();
        let original = "repo://fixture#alpha";
        let colliding = "repo://fixture#bravo";
        ok(store.connection.execute(
            "INSERT INTO object_id_dict (value, value_hash, value_len) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                colliding,
                stable_text_hash_key(original),
                stable_text_len(original)
            ],
        ));
        let colliding_id = store.connection.last_insert_rowid();
        let original_id = ok(intern_object_id(&store.connection, original));
        assert_ne!(original_id, colliding_id);

        assert_eq!(
            ok(lookup_object_id(&store.connection, original)),
            Some(original_id)
        );
        assert!(!ok(store.index_exists("sqlite_autoindex_object_id_dict_1")));
    }

    #[test]
    fn qname_and_prefix_length_validation_blocks_source_text_identity_bloat() {
        let store = store();
        let long_qname = format!("src.{}", "x".repeat(MAX_QUALIFIED_NAME_BYTES + 1));
        let result = intern_qualified_name(&store.connection, &long_qname);
        assert!(
            matches!(result, Err(StoreError::Message(ref message)) if message.contains("qualified_name exceeds max identity length")),
            "expected qname length error, got {result:?}"
        );

        let long_prefix = format!("{}.name", "p".repeat(MAX_QNAME_PREFIX_BYTES + 1));
        let result = intern_qualified_name(&store.connection, &long_prefix);
        assert!(
            matches!(result, Err(StoreError::Message(ref message)) if message.contains("qname_prefix exceeds max identity length")),
            "expected prefix length error, got {result:?}"
        );

        assert_eq!(
            ok(lookup_qualified_name(&store.connection, &long_qname)),
            None
        );
        assert_eq!(
            ok(lookup_qualified_name(&store.connection, &long_prefix)),
            None
        );

        let mut reference = entity("reference");
        reference.created_from = "tree-sitter-static-heuristic".to_string();
        reference.name = "raw".repeat(256);
        reference.qualified_name = format!("static_reference:{}", reference.name);
        let result = store.insert_static_reference_after_file_delete(&reference);
        assert!(
            matches!(result, Err(StoreError::Message(ref message)) if message.contains("entity display name exceeds max identity length")),
            "expected sidecar display-name length error, got {result:?}"
        );
    }

    #[test]
    fn legacy_dictionary_compaction_migration_preserves_ids_without_text_unique_indexes() {
        let connection = rusqlite::Connection::open_in_memory().expect("connection");
        ok(register_sqlite_functions(&connection));
        connection
            .execute_batch(
                "
                CREATE TABLE object_id_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
                CREATE TABLE symbol_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
                CREATE TABLE qname_prefix_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
                CREATE TABLE qualified_name_dict (
                    id INTEGER PRIMARY KEY,
                    prefix_id INTEGER NOT NULL,
                    suffix_id INTEGER NOT NULL,
                    value TEXT NOT NULL UNIQUE
                );
                INSERT INTO object_id_dict (id, value) VALUES (7, 'repo://fixture#login');
                INSERT INTO symbol_dict (id, value) VALUES (11, 'login');
                INSERT INTO qname_prefix_dict (id, value) VALUES (13, 'src.auth');
                INSERT INTO qualified_name_dict (id, prefix_id, suffix_id, value)
                VALUES (17, 13, 11, 'src.auth.login');
                ",
            )
            .expect("legacy schema");

        ok(migrate_dictionary_compaction(&connection));
        connection.execute_batch(SCHEMA_SQL).expect("finish schema");

        assert!(ok(table_has_column(
            &connection,
            "object_id_dict",
            "value_hash"
        )));
        assert!(ok(table_has_column(
            &connection,
            "symbol_dict",
            "value_hash"
        )));
        assert!(ok(table_has_column(
            &connection,
            "qname_prefix_dict",
            "value_hash"
        )));
        assert_eq!(
            ok(lookup_object_id(&connection, "repo://fixture#login")),
            Some(7)
        );
        assert_eq!(
            ok(lookup_qualified_name(&connection, "src.auth.login")),
            Some(17)
        );
        let compact_value: Option<String> = ok(connection.query_row(
            "SELECT value FROM qualified_name_dict WHERE id = 17",
            [],
            |row| row.get(0),
        ));
        assert_eq!(compact_value, None);
        let reconstructed: String = ok(connection.query_row(
            "SELECT value FROM qualified_name_lookup WHERE id = 17",
            [],
            |row| row.get(0),
        ));
        assert_eq!(reconstructed, "src.auth.login");
    }

    #[test]
    fn heuristic_debug_sidecars_round_trip_and_delete_by_file() {
        let store = store();
        let head = entity("caller");
        let mut reference = entity("maybeTarget");
        reference.name = "unknown_callee".to_string();
        reference.qualified_name = "static_reference:maybeTarget".to_string();
        reference.created_from = "tree-sitter-static-heuristic".to_string();
        reference.confidence = 0.55;

        let mut edge = sample_edge(
            "edge-heuristic-call",
            &head.id,
            RelationKind::Calls,
            &reference.id,
        );
        edge.exactness = Exactness::StaticHeuristic;
        edge.edge_class = EdgeClass::BaseHeuristic;
        edge.context = EdgeContext::Unknown;
        edge.confidence = 0.55;
        edge.metadata.insert(
            "resolution".to_string(),
            "unresolved_static_heuristic".into(),
        );

        ok(store.insert_heuristic_edge_after_file_delete(&edge));
        ok(store.insert_static_reference_after_file_delete(&reference));
        ok(store.insert_unresolved_reference_after_file_delete(
            &reference.id,
            &reference.name,
            RelationKind::Calls,
            reference.source_span.as_ref().expect("span"),
            reference.file_hash.as_deref(),
            Exactness::StaticHeuristic,
            &reference.created_from,
            &serde_json::json!({"resolution": "unresolved_static_heuristic"}),
        ));
        ok(store.insert_extraction_warning_after_file_delete(
            "src/auth.ts",
            Some("sha256:file"),
            "unresolved maybeTarget",
            &serde_json::json!({"kind": "unresolved_reference"}),
        ));

        assert_eq!(ok(count_rows(&store.connection, "edges")), 0);
        assert_eq!(ok(store.list_heuristic_edges(10)).len(), 1);
        assert_eq!(ok(store.list_static_references(10)).len(), 1);
        assert_eq!(
            ok(count_rows(&store.connection, "unresolved_references")),
            1
        );
        assert_eq!(ok(count_rows(&store.connection, "extraction_warnings")), 1);

        ok(store.delete_facts_for_file("src/auth.ts"));
        assert_eq!(ok(count_rows(&store.connection, "heuristic_edges")), 0);
        assert_eq!(ok(count_rows(&store.connection, "static_references")), 0);
        assert_eq!(
            ok(count_rows(&store.connection, "unresolved_references")),
            0
        );
        assert_eq!(ok(count_rows(&store.connection, "extraction_warnings")), 0);
    }

    #[test]
    fn file_reverse_maps_track_current_entities_edges_spans_and_paths() {
        let store = store();
        let entity = entity("login");
        let edge = sample_edge(
            "edge-login-self",
            &entity.id,
            RelationKind::Calls,
            &entity.id,
        );
        let path = PathEvidence {
            id: "path-login-self".to_string(),
            summary: Some("login self call".to_string()),
            source: entity.id.clone(),
            target: entity.id.clone(),
            metapath: vec![RelationKind::Calls],
            edges: vec![(entity.id.clone(), RelationKind::Calls, entity.id.clone())],
            source_spans: vec![edge.source_span.clone()],
            exactness: Exactness::ParserVerified,
            length: 1,
            confidence: 1.0,
            metadata: Default::default(),
        };

        ok(store.upsert_entity(&entity));
        ok(store.upsert_edge(&edge));
        ok(store.upsert_source_span(&edge.id, &edge.source_span));
        ok(store.upsert_path_evidence(&path));

        for (table, expected) in [
            ("file_entities", 1),
            ("file_edges", 1),
            ("file_source_spans", 1),
            ("file_path_evidence", 1),
        ] {
            assert_eq!(
                ok(count_rows(&store.connection, table)),
                expected,
                "{table}"
            );
        }

        ok(store.delete_facts_for_file("src/auth.ts"));

        for table in [
            "file_entities",
            "file_edges",
            "file_source_spans",
            "file_path_evidence",
        ] {
            assert_eq!(ok(count_rows(&store.connection, table)), 0, "{table}");
        }
        assert_eq!(ok(store.get_path_evidence(&path.id)), None);
    }

    #[test]
    fn incremental_graph_digest_updates_one_file_without_full_scan() {
        let store = store();
        let entity = entity("login");
        let edge = sample_edge(
            "edge-login-self",
            &entity.id,
            RelationKind::Calls,
            &entity.id,
        );

        assert_eq!(ok(store.incremental_graph_digest()), None);
        ok(store.upsert_entity(&entity));
        ok(store.upsert_edge(&edge));

        let file_digest = ok(store.current_file_graph_digest("src/auth.ts"));
        let repo_digest =
            ok(store.update_incremental_graph_digest_for_file("src/auth.ts", Some(1)));
        assert_ne!(file_digest, "fnv64:0000000000000000");
        assert_eq!(
            ok(store.incremental_graph_digest()),
            Some(repo_digest.clone())
        );
        assert_eq!(
            ok(store.update_incremental_graph_digest_for_file("src/auth.ts", Some(2))),
            repo_digest
        );

        ok(store.delete_facts_for_file("src/auth.ts"));
        let empty_repo_digest =
            ok(store.update_incremental_graph_digest_for_file("src/auth.ts", Some(3)));
        assert_eq!(empty_repo_digest, "fnv64:0000000000000000");
        assert_eq!(
            ok(store.current_file_graph_digest("src/auth.ts")),
            "fnv64:0000000000000000"
        );
    }

    #[test]
    fn file_backed_store_uses_documented_sqlite_pragmas() {
        let path = temp_db_path();
        let store = ok(SqliteGraphStore::open(&path));
        let pragmas = ok(store.sqlite_pragmas());

        assert_eq!(pragmas.get("foreign_keys").map(String::as_str), Some("1"));
        assert_eq!(pragmas.get("journal_mode").map(String::as_str), Some("wal"));
        assert_eq!(pragmas.get("synchronous").map(String::as_str), Some("2"));
        assert_eq!(
            pragmas.get("busy_timeout_ms").map(String::as_str),
            Some("5000")
        );

        drop(store);
        remove_temp_db_family(&path);
    }

    #[test]
    fn insert_read_update_delete_entity() {
        let store = store();
        let mut entity = entity("login");

        ok(store.upsert_entity(&entity));
        assert_eq!(ok(store.get_entity(&entity.id)), Some(entity.clone()));

        entity.confidence = 0.75;
        entity.name = "loginUpdated".to_string();
        ok(store.upsert_entity(&entity));
        assert_eq!(ok(store.get_entity(&entity.id)), Some(entity.clone()));
        assert_eq!(ok(store.count_entities()), 1);
        assert_eq!(ok(store.list_entities(10)), vec![entity.clone()]);

        assert!(ok(store.delete_entity(&entity.id)));
        assert_eq!(ok(store.get_entity(&entity.id)), None);
        assert!(!ok(store.delete_entity(&entity.id)));
    }

    #[test]
    fn exact_symbol_lookup_matches_name_qualified_name_and_id() {
        let store = store();
        let entity = entity("login");

        ok(store.upsert_entity(&entity));

        assert_eq!(
            ok(store.find_entities_by_exact_symbol("login")),
            vec![entity.clone()]
        );
        assert_eq!(
            ok(store.find_entities_by_exact_symbol("AuthService.login")),
            vec![entity.clone()]
        );
        assert_eq!(
            ok(store.find_entities_by_exact_symbol(&entity.id)),
            vec![entity]
        );
    }

    #[test]
    fn insert_read_edge_and_query_by_endpoint_relation() {
        let store = store();
        let calls_edge = sample_edge(
            "edge-login-create",
            "AuthService.login",
            RelationKind::Calls,
            "TokenStore.create",
        );
        let other = sample_edge(
            "edge-login-read",
            "AuthService.login",
            RelationKind::Reads,
            "req.body.email",
        );

        ok(store.upsert_edge(&calls_edge));
        ok(store.upsert_edge(&other));

        assert_eq!(ok(store.count_edges()), 2);
        assert_eq!(ok(store.list_edges(10)).len(), 2);
        assert_eq!(ok(store.get_edge(&calls_edge.id)), Some(calls_edge.clone()));
        assert_eq!(
            ok(store.find_edges_by_head_relation("AuthService.login", RelationKind::Calls)),
            vec![calls_edge.clone()]
        );
        assert_eq!(
            ok(store.find_edges_by_tail_relation("TokenStore.create", RelationKind::Calls)),
            vec![calls_edge]
        );
    }

    #[test]
    fn high_cardinality_structural_relations_use_entity_attributes_but_remain_queryable() {
        let store = store();
        let parent = entity("parent");
        let child = entity("child");
        let edge = sample_edge(
            &format!("edge-{}-contains", parent.id),
            &parent.id,
            RelationKind::Contains,
            &child.id,
        );
        let defined_edge = sample_edge(
            &format!("edge-{}-defined-in", child.id),
            &child.id,
            RelationKind::DefinedIn,
            &parent.id,
        );

        ok(store.upsert_entity(&parent));
        ok(store.upsert_entity(&child));
        ok(store.insert_edge_after_file_delete(&edge));
        ok(store.insert_edge_after_file_delete(&defined_edge));

        assert_eq!(ok(count_rows(&store.connection, "edges")), 0);
        assert_eq!(ok(count_rows(&store.connection, "structural_relations")), 0);
        assert_eq!(ok(store.count_edges()), 2);
        assert_eq!(
            ok(store.find_edges_by_head_relation(&parent.id, RelationKind::Contains))
                .into_iter()
                .map(|found| (found.head_id, found.relation, found.tail_id))
                .collect::<Vec<_>>(),
            vec![(parent.id.clone(), RelationKind::Contains, child.id.clone())]
        );
        assert_eq!(
            ok(store.find_edges_by_head_relation(&child.id, RelationKind::DefinedIn))
                .into_iter()
                .map(|found| (found.head_id, found.relation, found.tail_id))
                .collect::<Vec<_>>(),
            vec![(child.id.clone(), RelationKind::DefinedIn, parent.id.clone())]
        );
        assert_eq!(
            ok(store.relation_counts())
                .get(&RelationKind::Contains.to_string())
                .copied(),
            Some(1)
        );
        assert_eq!(
            ok(store.relation_counts())
                .get(&RelationKind::DefinedIn.to_string())
                .copied(),
            Some(1)
        );
        let (file_id, parent_id): (Option<i64>, Option<i64>) = ok(store.connection.query_row(
            "SELECT file_id, parent_id FROM entities WHERE id_key = (SELECT id FROM object_id_lookup WHERE value = ?1)",
            [child.id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ));
        assert!(file_id.is_some());
        assert!(parent_id.is_some());
    }

    #[test]
    fn callsite_relations_use_callsite_tables_but_remain_queryable() {
        let store = store();
        let callsite = entity("callsite");
        let callee = entity("callee");
        let argument = entity("argument");
        let callee_edge = sample_edge(
            "edge-callsite-callee",
            &callsite.id,
            RelationKind::Callee,
            &callee.id,
        );
        let argument_edge = sample_edge(
            "edge-callsite-arg0",
            &callsite.id,
            RelationKind::Argument0,
            &argument.id,
        );

        ok(store.upsert_entity(&callsite));
        ok(store.upsert_entity(&callee));
        ok(store.upsert_entity(&argument));
        ok(store.insert_edge_after_file_delete(&callee_edge));
        ok(store.insert_edge_after_file_delete(&argument_edge));

        assert_eq!(ok(count_rows(&store.connection, "edges")), 0);
        assert_eq!(ok(count_rows(&store.connection, "callsites")), 1);
        assert_eq!(ok(count_rows(&store.connection, "callsite_args")), 1);
        assert_eq!(ok(store.count_edges()), 2);
        assert_eq!(
            ok(store.find_edges_by_head_relation(&callsite.id, RelationKind::Callee))
                .into_iter()
                .map(|found| found.tail_id)
                .collect::<Vec<_>>(),
            vec![callee.id.clone()]
        );
        assert_eq!(
            ok(store.find_edges_by_tail_relation(&argument.id, RelationKind::Argument0))
                .into_iter()
                .map(|found| found.head_id)
                .collect::<Vec<_>>(),
            vec![callsite.id.clone()]
        );
        let relation_counts = ok(store.relation_counts());
        assert_eq!(
            relation_counts
                .get(&RelationKind::Callee.to_string())
                .copied(),
            Some(1)
        );
        assert_eq!(
            relation_counts
                .get(&RelationKind::Argument0.to_string())
                .copied(),
            Some(1)
        );
    }

    #[test]
    fn template_overlay_uses_compact_local_ids_without_directional_indexes() {
        let store = store();
        let file = FileRecord {
            repo_relative_path: "src/auth.ts".to_string(),
            file_hash: "sha256:file".to_string(),
            language: Some("typescript".to_string()),
            size_bytes: 1234,
            indexed_at_unix_ms: Some(10),
            metadata: Default::default(),
        };
        let duplicate = FileRecord {
            repo_relative_path: "src/auth-copy.ts".to_string(),
            file_hash: file.file_hash.clone(),
            language: file.language.clone(),
            size_bytes: file.size_bytes,
            indexed_at_unix_ms: file.indexed_at_unix_ms,
            metadata: Default::default(),
        };
        let head = entity("login");
        let tail = entity("logout");
        let edge = sample_edge("edge-login-logout", &head.id, RelationKind::Calls, &tail.id);

        ok(store.upsert_file(&file));
        ok(store.upsert_file(&duplicate));
        ok(store.upsert_content_template_extraction(&file, &[head.clone(), tail.clone()], &[edge]));

        assert_eq!(
            ok(super::table_column_decl_type(
                &store.connection,
                "template_entities",
                "local_template_entity_id",
            )),
            Some("INTEGER".to_string())
        );
        assert_eq!(
            ok(super::table_column_decl_type(
                &store.connection,
                "template_edges",
                "local_head_entity_id",
            )),
            Some("INTEGER".to_string())
        );
        assert!(!ok(store.index_exists("idx_template_edges_head_relation")));
        assert!(!ok(store.index_exists("idx_template_edges_tail_relation")));
        assert_eq!(
            ok(store.connection.query_row(
                "SELECT typeof(local_template_entity_id) FROM template_entities LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )),
            "integer"
        );

        let duplicate_entities = ok(store.list_entities_by_file("src/auth-copy.ts"));
        let duplicate_head = duplicate_entities
            .iter()
            .find(|entity| entity.name == "login")
            .expect("duplicate head");
        let duplicate_tail = duplicate_entities
            .iter()
            .find(|entity| entity.name == "logout")
            .expect("duplicate tail");
        assert_ne!(duplicate_head.id, head.id);
        assert_ne!(duplicate_tail.id, tail.id);
        assert_eq!(duplicate_head.repo_relative_path, "src/auth-copy.ts");

        let duplicate_edges =
            ok(store.find_edges_by_head_relation(&duplicate_head.id, RelationKind::Calls));
        assert!(duplicate_edges.iter().any(|found| {
            found.tail_id == duplicate_tail.id
                && found.exactness == Exactness::ParserVerified
                && found.source_span.repo_relative_path == "src/auth-copy.ts"
        }));
    }

    #[test]
    fn template_overlay_compacts_source_derived_symbol_and_qname_storage() {
        let store = store();
        let file = FileRecord {
            repo_relative_path: "src/auth.ts".to_string(),
            file_hash: "sha256:template".to_string(),
            language: Some("typescript".to_string()),
            size_bytes: 512,
            indexed_at_unix_ms: Some(10),
            metadata: Default::default(),
        };
        let duplicate = FileRecord {
            repo_relative_path: "src/auth-copy.ts".to_string(),
            file_hash: file.file_hash.clone(),
            language: file.language.clone(),
            size_bytes: file.size_bytes,
            indexed_at_unix_ms: file.indexed_at_unix_ms,
            metadata: Default::default(),
        };
        let raw_call_label =
            "call:manager.try_ensure_connection_successfully_after_expensive_setup";
        let raw_qname = "src::auth.AuthService.constructor.call:manager.try_ensure_connection_successfully_after_expensive_setup";
        let callsite = Entity {
            id: stable_entity_id("src/auth.ts", "callsite:manager.try_ensure_connection"),
            kind: EntityKind::CallSite,
            name: raw_call_label.to_string(),
            qualified_name: raw_qname.to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(SourceSpan {
                repo_relative_path: "src/auth.ts".to_string(),
                start_line: 4,
                start_column: Some(9),
                end_line: 4,
                end_column: Some(41),
            }),
            content_hash: None,
            file_hash: Some(file.file_hash.clone()),
            created_from: "test".to_string(),
            confidence: 1.0,
            metadata: Default::default(),
        };

        ok(store.upsert_file(&file));
        ok(store.upsert_file(&duplicate));
        ok(store.upsert_content_template_extraction(&file, std::slice::from_ref(&callsite), &[]));

        let (stored_name, stored_qname): (String, String) = ok(store.connection.query_row(
            "
            SELECT name.value, qname.value
            FROM template_entities te
            JOIN symbol_dict name ON name.id = te.name_id
            JOIN qualified_name_lookup qname ON qname.id = te.qualified_name_id
            ",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ));
        assert_eq!(stored_name, "callsite");
        assert_eq!(stored_qname, "@template.callsite.1");
        assert!(!stored_qname.contains("try_ensure_connection"));

        let raw_identity_rows: i64 = ok(store.connection.query_row(
            "
            SELECT
              (SELECT COUNT(*) FROM symbol_dict WHERE value LIKE '%try_ensure_connection%')
              + (SELECT COUNT(*) FROM qname_prefix_dict WHERE value LIKE '%try_ensure_connection%')
              + (SELECT COUNT(*) FROM qualified_name_lookup WHERE value LIKE '%try_ensure_connection%')
            ",
            [],
            |row| row.get(0),
        ));
        assert_eq!(raw_identity_rows, 0);

        let duplicate_entities = ok(store.list_entities_by_file("src/auth-copy.ts"));
        let duplicate_callsite = duplicate_entities
            .iter()
            .find(|entity| entity.kind == EntityKind::CallSite)
            .expect("duplicate callsite");
        assert_ne!(duplicate_callsite.id, callsite.id);
        assert_eq!(duplicate_callsite.name, "callsite");
        assert_eq!(
            duplicate_callsite.qualified_name,
            "src::auth-copy.callsite@1"
        );
        assert_eq!(
            duplicate_callsite
                .source_span
                .as_ref()
                .expect("source span")
                .repo_relative_path,
            "src/auth-copy.ts"
        );
    }

    #[test]
    fn compact_insert_preserves_edge_metadata_and_provenance() {
        let store = store();
        let mut edge = sample_edge(
            "edge-login-derived",
            "AuthService.login",
            RelationKind::Mutates,
            "TokenStore.create",
        );
        edge.derived = true;
        edge.exactness = Exactness::DerivedFromVerifiedEdges;
        edge.provenance_edges = vec!["edge-login-create".to_string()];
        edge.metadata
            .insert("fact_class".to_string(), "derived_cache".into());

        ok(store.insert_edge_after_file_delete(&edge));

        let stored = ok(store.list_edges(10)).into_iter().next().expect("edge");
        assert!(stored.derived);
        assert_eq!(stored.edge_class, EdgeClass::Derived);
        assert_eq!(stored.context, EdgeContext::Production);
        assert_eq!(stored.provenance_edges, edge.provenance_edges);
        assert_eq!(
            stored
                .metadata
                .get("fact_class")
                .and_then(|value| value.as_str()),
            Some("derived_cache")
        );
    }

    #[test]
    fn compact_edge_insert_is_idempotent_for_duplicate_edges() {
        let store = store();
        let edge = sample_edge(
            "edge-login-duplicate",
            "AuthService.login",
            RelationKind::Calls,
            "TokenStore.create",
        );

        ok(store.insert_edge_after_file_delete(&edge));
        ok(store.insert_edge_after_file_delete(&edge));

        assert_eq!(ok(store.count_edges()), 1);
        ok(store.full_integrity_gate());
    }

    #[test]
    fn graph_fact_digest_changes_when_graph_facts_change() {
        let store = store();
        let entity = entity("login");
        let edge = sample_edge(
            "edge-login-digest",
            &entity.id,
            RelationKind::Calls,
            "TokenStore.create",
        );
        let empty = ok(store.graph_fact_digest());

        ok(store.upsert_entity(&entity));
        ok(store.upsert_edge(&edge));
        let populated = ok(store.graph_fact_digest());

        assert_ne!(empty, populated);
        assert_eq!(populated, ok(store.graph_fact_digest()));
    }

    #[test]
    fn derived_edges_without_provenance_are_rejected() {
        let store = store();
        let mut edge = sample_edge(
            "edge-login-may-mutate",
            "AuthService.login",
            RelationKind::MayMutate,
            "TokenStore.create",
        );
        edge.derived = true;
        edge.exactness = Exactness::DerivedFromVerifiedEdges;

        let result = store.upsert_edge(&edge);
        assert!(
            matches!(result, Err(StoreError::Message(message)) if message.contains("provenance_edges"))
        );

        let derived = DerivedClosureEdge {
            id: "edge://derived".to_string(),
            head_id: "AuthService.login".to_string(),
            relation: RelationKind::MayMutate,
            tail_id: "TokenStore.create".to_string(),
            provenance_edges: Vec::new(),
            exactness: Exactness::DerivedFromVerifiedEdges,
            confidence: 0.9,
            metadata: Default::default(),
        };
        let result = store.upsert_derived_edge(&derived);
        assert!(
            matches!(result, Err(StoreError::Message(message)) if message.contains("provenance_edges"))
        );
    }

    #[test]
    fn unresolved_exact_edges_are_downgraded_before_storage() {
        let store = store();
        let mut edge = sample_edge(
            "edge-unresolved-call",
            "AuthService.login",
            RelationKind::Calls,
            "maybeCreate",
        );
        edge.exactness = Exactness::ParserVerified;
        edge.metadata
            .insert("resolution".to_string(), "unresolved-callee".into());

        ok(store.upsert_edge(&edge));
        let stored = ok(store.get_edge(&edge.id)).expect("edge");

        assert_eq!(stored.exactness, Exactness::StaticHeuristic);
        assert_eq!(stored.edge_class, EdgeClass::BaseHeuristic);
        assert!(stored.confidence <= 0.55);
        assert_eq!(
            stored
                .metadata
                .get("resolution")
                .and_then(|value| value.as_str()),
            Some("unresolved-callee")
        );
        assert_eq!(
            stored
                .metadata
                .get("heuristic")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn source_span_persistence_round_trips() {
        let store = store();
        let span = SourceSpan::with_columns("src/auth.ts", 82, 4, 91, 5);

        ok(store.upsert_source_span("span-auth-login", &span));
        assert_eq!(ok(store.get_source_span("span-auth-login")), Some(span));
        assert!(ok(store.delete_source_span("span-auth-login")));
        assert_eq!(ok(store.get_source_span("span-auth-login")), None);
    }

    #[test]
    fn delete_facts_for_file_removes_stale_entities_edges_spans_and_text() {
        let store = store();
        let entity = entity("login");
        let external_entity = Entity {
            id: "entity://external".to_string(),
            kind: EntityKind::Function,
            name: "external".to_string(),
            qualified_name: "external".to_string(),
            repo_relative_path: "src/external.ts".to_string(),
            source_span: Some(SourceSpan::new("src/external.ts", 1, 1)),
            content_hash: None,
            file_hash: Some("sha256:external".to_string()),
            created_from: "test".to_string(),
            confidence: 1.0,
            metadata: Default::default(),
        };
        let edge = sample_edge(
            "edge-external-login",
            &external_entity.id,
            RelationKind::Calls,
            &entity.id,
        );
        let path = PathEvidence {
            id: "path-external-login".to_string(),
            summary: Some("external calls stale login".to_string()),
            source: external_entity.id.clone(),
            target: entity.id.clone(),
            metapath: vec![RelationKind::Calls],
            edges: vec![(
                external_entity.id.clone(),
                RelationKind::Calls,
                entity.id.clone(),
            )],
            source_spans: vec![edge.source_span.clone()],
            exactness: Exactness::ParserVerified,
            length: 1,
            confidence: 1.0,
            metadata: Default::default(),
        };
        let derived = DerivedClosureEdge {
            id: "derived-external-login".to_string(),
            head_id: external_entity.id.clone(),
            relation: RelationKind::Calls,
            tail_id: entity.id.clone(),
            provenance_edges: vec![edge.id.clone()],
            exactness: Exactness::DerivedFromVerifiedEdges,
            confidence: 1.0,
            metadata: Default::default(),
        };
        let file = FileRecord {
            repo_relative_path: "src/auth.ts".to_string(),
            file_hash: "sha256:file".to_string(),
            language: Some("typescript".to_string()),
            size_bytes: 1234,
            indexed_at_unix_ms: Some(10),
            metadata: Default::default(),
        };

        ok(store.upsert_file(&file));
        ok(store.upsert_file_text("src/auth.ts", "login stale searchable text"));
        ok(store.upsert_entity(&entity));
        ok(store.upsert_entity(&external_entity));
        ok(store.upsert_source_span(&entity.id, entity.source_span.as_ref().expect("span")));
        ok(store.upsert_snippet_text(
            &entity.id,
            entity.source_span.as_ref().expect("span"),
            "login snippet",
        ));
        ok(store.upsert_edge(&edge));
        ok(store.upsert_source_span(&edge.id, &edge.source_span));
        ok(store.upsert_path_evidence(&path));
        ok(store.upsert_derived_edge(&derived));

        ok(store.delete_facts_for_file("src/auth.ts"));

        assert_eq!(ok(store.get_file("src/auth.ts")), None);
        assert_eq!(ok(store.get_entity(&entity.id)), None);
        assert_eq!(
            ok(store.get_entity(&external_entity.id)),
            Some(external_entity)
        );
        assert_eq!(ok(store.get_edge(&edge.id)), None);
        assert_eq!(ok(store.get_source_span(&entity.id)), None);
        assert_eq!(ok(store.get_path_evidence(&path.id)), None);
        assert_eq!(ok(store.get_derived_edge(&derived.id)), None);
        assert!(ok(store.search_text("stale searchable login", 10)).is_empty());
    }

    #[test]
    fn file_and_repo_index_state_round_trip() {
        let store = store();
        let file = FileRecord {
            repo_relative_path: "src/auth.ts".to_string(),
            file_hash: "sha256:file".to_string(),
            language: Some("typescript".to_string()),
            size_bytes: 1234,
            indexed_at_unix_ms: Some(10),
            metadata: Default::default(),
        };
        let state = RepoIndexState {
            repo_id: "repo://fixture".to_string(),
            repo_root: ".".to_string(),
            repo_commit: Some("abc123".to_string()),
            schema_version: SCHEMA_VERSION,
            indexed_at_unix_ms: Some(11),
            files_indexed: 1,
            entity_count: 2,
            edge_count: 3,
            metadata: Default::default(),
        };

        ok(store.upsert_file(&file));
        ok(store.upsert_repo_index_state(&state));

        assert_eq!(ok(store.count_files()), 1);
        assert_eq!(ok(store.list_files(10)), vec![file.clone()]);
        assert_eq!(ok(store.get_file("src/auth.ts")), Some(file));
        assert_eq!(
            ok(store.get_repo_index_state("repo://fixture")),
            Some(state)
        );
    }

    #[test]
    fn storage_accounting_reports_top_compact_contributors() {
        let store = store();
        let entity = entity("login");
        let edge = sample_edge(
            "edge-login-self",
            &entity.id,
            RelationKind::Calls,
            &entity.id,
        );
        ok(store.upsert_entity(&entity));
        ok(store.upsert_edge(&edge));
        ok(store.upsert_source_span("span-auth-login", &span()));

        let accounting = ok(store.storage_accounting());
        assert!(accounting.iter().any(|row| row.name == "object_id_dict"));
        assert!(accounting.iter().any(|row| row.name == "entities"));
        assert!(accounting.iter().any(|row| row.name == "edges"));
        assert!(accounting
            .windows(2)
            .all(|pair| pair[0].payload_bytes >= pair[1].payload_bytes));
    }

    #[test]
    fn finish_bulk_index_load_preserves_graph_facts_and_integrity() {
        let path = temp_db_path();
        {
            let store = ok(SqliteGraphStore::open(&path));
            let head = entity("login");
            let tail = entity("logout");
            let edge = sample_edge("edge-login-logout", &head.id, RelationKind::Calls, &tail.id);
            ok(store.upsert_entity(&head));
            ok(store.upsert_entity(&tail));
            ok(store.upsert_edge(&edge));
            ok(store.upsert_source_span("span-auth-login", &span()));

            ok(store.connection.execute_batch(
                "
                CREATE TABLE scratch_bloat(value TEXT);
                WITH RECURSIVE n(x) AS (
                    SELECT 1
                    UNION ALL
                    SELECT x + 1 FROM n WHERE x < 2048
                )
                INSERT INTO scratch_bloat(value)
                SELECT hex(randomblob(2048)) FROM n;
                DROP TABLE scratch_bloat;
                PRAGMA wal_checkpoint(TRUNCATE);
                ",
            ));
            let free_pages_before = pragma_i64(&store, "PRAGMA freelist_count");
            assert!(
                free_pages_before > 0,
                "fixture did not create free pages to compact"
            );
            let entities_before = ok(count_rows(&store.connection, "entities"));
            let edges_before = ok(count_rows(&store.connection, "edges"));
            let spans_before = ok(count_rows(&store.connection, "source_spans"));

            ok(store.finish_bulk_index_load());

            assert_eq!(
                ok(count_rows(&store.connection, "entities")),
                entities_before
            );
            assert_eq!(ok(count_rows(&store.connection, "edges")), edges_before);
            assert_eq!(
                ok(count_rows(&store.connection, "source_spans")),
                spans_before
            );
            ok(store.full_integrity_gate());
        }
        remove_temp_db_family(&path);
    }

    #[test]
    fn finish_bulk_index_load_preserves_default_edge_lookup_indexes() {
        let path = temp_db_path();
        {
            let store = ok(SqliteGraphStore::open(&path));
            let head = entity("login");
            let tail = entity("logout");
            let edge = sample_edge("edge-login-logout", &head.id, RelationKind::Calls, &tail.id);
            ok(store.upsert_entity(&head));
            ok(store.upsert_entity(&tail));
            ok(store.upsert_edge(&edge));

            ok(store.finish_bulk_index_load());

            for index in [
                "idx_edges_head_relation",
                "idx_edges_tail_relation",
                "idx_edges_span_path",
                "idx_source_spans_path",
            ] {
                assert!(ok(store.index_exists(index)), "missing index {index}");
            }

            let relation = RelationKind::Calls.to_string();
            let head_id: i64 = ok(store.connection.query_row(
                "SELECT id FROM object_id_lookup WHERE value = ?1",
                rusqlite::params![&head.id],
                |row| row.get(0),
            ));
            let tail_id: i64 = ok(store.connection.query_row(
                "SELECT id FROM object_id_lookup WHERE value = ?1",
                rusqlite::params![&tail.id],
                |row| row.get(0),
            ));
            let relation_id: i64 = ok(store.connection.query_row(
                "SELECT id FROM relation_kind_dict WHERE value = ?1",
                rusqlite::params![&relation],
                |row| row.get(0),
            ));

            let mut head_plan = ok(store.connection.prepare(
                "EXPLAIN QUERY PLAN SELECT id_key FROM edges WHERE head_id_key = ?1 AND relation_id = ?2 LIMIT 10",
            ));
            let head_plan_details = ok(head_plan
                .query_map(rusqlite::params![head_id, relation_id], |row| {
                    row.get::<_, String>(3)
                }))
            .map(|row| row.expect("head query plan row"))
            .collect::<Vec<_>>();
            assert!(
                head_plan_details
                    .iter()
                    .any(|detail| detail.contains("idx_edges_head_relation")),
                "head lookup did not use idx_edges_head_relation: {head_plan_details:?}"
            );

            let mut tail_plan = ok(store.connection.prepare(
                "EXPLAIN QUERY PLAN SELECT id_key FROM edges WHERE tail_id_key = ?1 AND relation_id = ?2 LIMIT 10",
            ));
            let tail_plan_details = ok(tail_plan
                .query_map(rusqlite::params![tail_id, relation_id], |row| {
                    row.get::<_, String>(3)
                }))
            .map(|row| row.expect("tail query plan row"))
            .collect::<Vec<_>>();
            assert!(
                tail_plan_details
                    .iter()
                    .any(|detail| detail.contains("idx_edges_tail_relation")),
                "tail lookup did not use idx_edges_tail_relation: {tail_plan_details:?}"
            );
        }
        remove_temp_db_family(&path);
    }

    #[test]
    fn proof_bulk_load_uses_transient_hash_index_and_insert_only_reverse_maps() {
        let path = temp_db_path();
        {
            let store = ok(SqliteGraphStore::open(&path));
            ok(store.begin_atomic_cold_bulk_index_load());
            ok(store.begin_bulk_index_transaction());
            ok(store.drop_bulk_index_lookup_indexes());
            assert!(ok(store.index_exists("idx_entities_entity_hash_build")));

            let head = entity("login");
            let tail = entity("logout");
            let edge = sample_edge("edge-login-logout", &head.id, RelationKind::Calls, &tail.id);
            ok(store.insert_entity_after_file_delete(&head));
            ok(store.insert_entity_after_file_delete(&tail));
            ok(store.insert_edge_after_file_delete(&edge));
            ok(store.commit_bulk_index_transaction());
            ok(store.finish_bulk_index_load_fast());

            assert!(!ok(store.index_exists("idx_entities_entity_hash_build")));
            assert_eq!(
                ok(count_rows(&store.connection, "file_entities")),
                2,
                "after-delete entity mapping should insert rows without reverse-map deletes"
            );
            assert_eq!(ok(count_rows(&store.connection, "file_edges")), 1);
            assert_eq!(ok(count_rows(&store.connection, "file_source_spans")), 1);
            ok(store.quick_integrity_gate());
        }
        remove_temp_db_family(&path);
    }

    #[test]
    fn legacy_text_rows_migrate_to_compact_schema() {
        let path = temp_db_path();
        let entity = entity("login");
        let edge = sample_edge(
            "edge-login-self",
            &entity.id,
            RelationKind::Calls,
            &entity.id,
        );
        let span = span();
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy connection");
            connection
                .execute_batch(
                    "
                    CREATE TABLE entities (
                        id TEXT PRIMARY KEY,
                        kind TEXT NOT NULL,
                        name TEXT NOT NULL,
                        qualified_name TEXT NOT NULL,
                        repo_relative_path TEXT NOT NULL,
                        source_span_json TEXT NOT NULL,
                        content_hash TEXT,
                        file_hash TEXT,
                        created_from TEXT NOT NULL,
                        confidence REAL NOT NULL,
                        metadata_json TEXT NOT NULL
                    ) WITHOUT ROWID;
                    CREATE TABLE edges (
                        id TEXT PRIMARY KEY,
                        head_id TEXT NOT NULL,
                        relation TEXT NOT NULL,
                        tail_id TEXT NOT NULL,
                        source_span_json TEXT NOT NULL,
                        repo_commit TEXT,
                        file_hash TEXT,
                        extractor TEXT NOT NULL,
                        confidence REAL NOT NULL,
                        exactness TEXT NOT NULL,
                        derived INTEGER NOT NULL,
                        provenance_edges_json TEXT NOT NULL,
                        metadata_json TEXT NOT NULL
                    ) WITHOUT ROWID;
                    CREATE TABLE source_spans (
                        id TEXT PRIMARY KEY,
                        repo_relative_path TEXT NOT NULL,
                        start_line INTEGER NOT NULL,
                        start_column INTEGER,
                        end_line INTEGER NOT NULL,
                        end_column INTEGER,
                        span_json TEXT NOT NULL
                    );
                    CREATE TABLE files (
                        repo_relative_path TEXT PRIMARY KEY,
                        file_hash TEXT NOT NULL,
                        language TEXT,
                        size_bytes INTEGER NOT NULL,
                        indexed_at_unix_ms INTEGER,
                        metadata_json TEXT NOT NULL
                    ) WITHOUT ROWID;
                    PRAGMA user_version = 3;
                    ",
                )
                .expect("legacy schema");
            connection
                .execute(
                    "INSERT INTO files VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        "src/auth.ts",
                        "sha256:file",
                        "typescript",
                        1234_i64,
                        10_i64,
                        "{}"
                    ],
                )
                .expect("legacy file");
            connection
                .execute(
                    "INSERT INTO entities VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![
                        entity.id,
                        entity.kind.to_string(),
                        entity.name,
                        entity.qualified_name,
                        entity.repo_relative_path,
                        serde_json::to_string(&entity.source_span).expect("span json"),
                        entity.content_hash,
                        entity.file_hash,
                        entity.created_from,
                        entity.confidence,
                        serde_json::to_string(&entity.metadata).expect("metadata json"),
                    ],
                )
                .expect("legacy entity");
            connection
                .execute(
                    "INSERT INTO edges VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    rusqlite::params![
                        edge.id,
                        edge.head_id,
                        edge.relation.to_string(),
                        edge.tail_id,
                        serde_json::to_string(&edge.source_span).expect("edge span json"),
                        edge.repo_commit,
                        edge.file_hash,
                        edge.extractor,
                        edge.confidence,
                        edge.exactness.to_string(),
                        edge.derived,
                        serde_json::to_string(&edge.provenance_edges).expect("provenance json"),
                        serde_json::to_string(&edge.metadata).expect("metadata json"),
                    ],
                )
                .expect("legacy edge");
            connection
                .execute(
                    "INSERT INTO source_spans VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        "span-auth-login",
                        span.repo_relative_path,
                        span.start_line,
                        span.start_column,
                        span.end_line,
                        span.end_column,
                        serde_json::to_string(&span).expect("source span json"),
                    ],
                )
                .expect("legacy source span");
        }

        let store = ok(SqliteGraphStore::open(&path));
        assert!(ok(table_has_column(&store.connection, "files", "file_id")));
        assert!(ok(table_has_column(
            &store.connection,
            "files",
            "content_hash"
        )));
        assert!(!ok(table_has_column(
            &store.connection,
            "files",
            "file_hash"
        )));
        for table in [
            "entities",
            "edges",
            "structural_relations",
            "callsites",
            "callsite_args",
        ] {
            assert!(
                !ok(table_has_column(&store.connection, table, "file_hash")),
                "migrated {table} should not retain repeated file_hash text"
            );
        }
        assert!(ok(table_has_column(
            &store.connection,
            "entities",
            "id_key"
        )));
        assert_eq!(
            ok(store.get_file("src/auth.ts")).expect("file").language,
            Some("typescript".to_string())
        );
        assert!(ok(store.find_entities_by_exact_symbol("login"))
            .into_iter()
            .any(|found| found.name == "login"));
        assert_eq!(
            ok(store.get_edge(&edge.id)).expect("edge").relation,
            RelationKind::Calls
        );
        assert_eq!(ok(store.get_source_span("span-auth-login")), Some(span));

        drop(store);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = fs::remove_file(path.with_extension("sqlite-shm"));
    }

    #[test]
    fn bm25_fts_returns_fixture_file_entity_and_snippet_hits() {
        let store = store();
        let entity = entity("normalizeEmail");
        let span = SourceSpan::new("src/auth.ts", 10, 12);

        ok(store.upsert_entity(&entity));
        ok(store.upsert_file_text(
            "src/auth.ts",
            "password reset flow normalizes email before token issuance",
        ));
        ok(store.upsert_snippet_text(
            "snippet-auth-normalize",
            &span,
            "normalizeEmail handles password reset email casing",
        ));

        let hits = ok(store.search_text("password reset normalizeEmail", 10));

        assert!(hits.iter().any(|hit| {
            hit.kind == TextSearchKind::File && hit.repo_relative_path == "src/auth.ts"
        }));
        assert!(hits
            .iter()
            .any(|hit| hit.kind == TextSearchKind::Entity && hit.id == entity.id));
        assert!(hits
            .iter()
            .any(|hit| hit.kind == TextSearchKind::Snippet && hit.line == Some(10)));
    }

    #[test]
    fn path_evidence_and_derived_edge_persistence_round_trip() {
        let store = store();
        let span = span();
        let path = PathEvidence {
            id: stable_entity_id("src/auth.ts", "path:login-create-token"),
            summary: Some("login calls token creation".to_string()),
            source: "AuthService.login".to_string(),
            target: "TokenStore.create".to_string(),
            metapath: vec![RelationKind::Calls],
            edges: vec![(
                "AuthService.login".to_string(),
                RelationKind::Calls,
                "TokenStore.create".to_string(),
            )],
            source_spans: vec![span.clone()],
            exactness: Exactness::ParserVerified,
            length: 1,
            confidence: 1.0,
            metadata: Default::default(),
        };
        let derived = DerivedClosureEdge {
            id: stable_edge_id(
                "AuthService.login",
                RelationKind::Mutates,
                "TokenStore.create",
                &span,
            ),
            head_id: "AuthService.login".to_string(),
            relation: RelationKind::Mutates,
            tail_id: "TokenStore.create".to_string(),
            provenance_edges: vec!["edge-login-create".to_string()],
            exactness: Exactness::DerivedFromVerifiedEdges,
            confidence: 0.9,
            metadata: Default::default(),
        };

        ok(store.upsert_path_evidence(&path));
        ok(store.upsert_derived_edge(&derived));

        assert_eq!(ok(store.get_path_evidence(&path.id)), Some(path.clone()));
        assert_eq!(ok(store.get_derived_edge(&derived.id)), Some(derived));

        assert_eq!(
            ok(store.connection.query_row(
                "SELECT source_id FROM path_evidence_lookup WHERE path_id = ?1",
                rusqlite::params![&path.id],
                |row| row.get::<_, String>(0),
            )),
            "AuthService.login"
        );
        assert_eq!(
            ok(store.connection.query_row(
                "SELECT COUNT(*) FROM path_evidence_symbols WHERE path_id = ?1",
                rusqlite::params![&path.id],
                |row| row.get::<_, u64>(0),
            )),
            2
        );
        assert_eq!(
            ok(store.connection.query_row(
                "SELECT source_span_path FROM path_evidence_edges WHERE path_id = ?1 AND ordinal = 0",
                rusqlite::params![&path.id],
                |row| row.get::<_, String>(0),
            )),
            "src/auth.ts"
        );
    }

    #[test]
    fn path_evidence_stores_verbose_edge_labels_as_materialized_rows() {
        let store = store();
        let span = span();
        let path = PathEvidence {
            id: "path://compact-labels".to_string(),
            summary: Some("login mutates tokens".to_string()),
            source: "AuthService.login".to_string(),
            target: "TokenStore.create".to_string(),
            metapath: vec![RelationKind::MayMutate],
            edges: vec![(
                "AuthService.login".to_string(),
                RelationKind::MayMutate,
                "TokenStore.create".to_string(),
            )],
            source_spans: vec![span],
            exactness: Exactness::DerivedFromVerifiedEdges,
            length: 1,
            confidence: 0.9,
            metadata: BTreeMap::from([
                (
                    "edge_labels".to_string(),
                    serde_json::json!([
                        {
                            "edge_id": "edge://derived",
                            "exactness": "derived_from_verified_edges",
                            "confidence": 0.9,
                            "edge_class": "derived",
                            "context": "production",
                            "derived": true,
                            "provenance_edges": ["edge://write"]
                        }
                    ]),
                ),
                (
                    "ordered_edge_ids".to_string(),
                    serde_json::json!(["edge://derived"]),
                ),
                (
                    "derived_edges_have_provenance".to_string(),
                    serde_json::json!(true),
                ),
            ]),
        };

        ok(store.upsert_path_evidence(&path));
        let stored_metadata: String = ok(store.connection.query_row(
            "SELECT metadata_json FROM path_evidence WHERE id = ?1",
            rusqlite::params![&path.id],
            |row| row.get(0),
        ));
        assert!(!stored_metadata.contains("edge_labels"));
        assert!(stored_metadata.contains("compact_materialized_rows"));
        let edge_row: (String, f64, i64, String, String, String) = ok(store.connection.query_row(
            "
            SELECT exactness, confidence, derived, edge_class, context, provenance_edges_json
            FROM path_evidence_edges
            WHERE path_id = ?1 AND ordinal = 0
            ",
            rusqlite::params![&path.id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        ));
        assert_eq!(edge_row.0, "derived_from_verified_edges");
        assert_eq!(edge_row.1, 0.9);
        assert_eq!(edge_row.2, 1);
        assert_eq!(edge_row.3, "derived");
        assert_eq!(edge_row.4, "production");
        assert_eq!(edge_row.5, "[\"edge://write\"]");
    }

    #[test]
    fn transaction_rollback_discards_writes() {
        let store = store();
        let entity = entity("login");

        let result: Result<(), StoreError> = store.transaction(|tx| {
            tx.upsert_entity(&entity)?;
            Err(StoreError::Message("force rollback".to_string()))
        });

        assert!(result.is_err());
        assert_eq!(ok(store.get_entity(&entity.id)), None);
    }

    #[test]
    fn retrieval_trace_persistence_round_trips() {
        let store = store();
        let mut trace = RetrievalTraceRecord {
            id: "trace://phase14".to_string(),
            task: Some("Change AuthService.login".to_string()),
            trace_json: serde_json::json!({
                "stages": [
                    { "stage": "stage0_exact_seed_extraction", "kept": ["AuthService.login"] },
                    { "stage": "bayesian_ranker", "confidence": 0.82, "uncertainty": 0.18 }
                ]
            }),
            created_at_unix_ms: Some(1_735_000_000_000),
        };

        ok(store.upsert_retrieval_trace(&trace));
        assert_eq!(
            ok(store.get_retrieval_trace(&trace.id)),
            Some(trace.clone())
        );

        trace.trace_json = serde_json::json!({ "stages": [], "confidence": 0.7 });
        ok(store.upsert_retrieval_trace(&trace));
        assert_eq!(
            ok(store.get_retrieval_trace(&trace.id)),
            Some(trace.clone())
        );

        assert!(ok(store.delete_retrieval_trace(&trace.id)));
        assert_eq!(ok(store.get_retrieval_trace(&trace.id)), None);
    }
}
