//! Context-pack command: seed resolution, entity/path-evidence loading,
//! candidate assembly, agent-json planning packet, and CLI compaction.
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, Instant};

use codegraph_core::{
    classify_edge_evidence_role, classify_entity_source_role,
    mvp4_3_local_micro_flow_packet_active_languages,
    mvp4_3_local_micro_flow_packet_source_supported_for_source, ContextPacket, Edge, Entity,
    EntityKind, EvidenceRole, Exactness, FileRecord, Metadata, MicroSourceRole, RelationKind,
    RetrievalCandidate, SourceSpan,
};
use codegraph_parser::detect_language;
use codegraph_query::{PromptSeed, PromptSeedKind};
use codegraph_store::{
    GraphStore, LocalFlowPacketQueryOptions, LocalFlowPacketRow, SqliteGraphStore,
};
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::*;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextPackBudgets {
    pub(crate) max_seed_entities: usize,
    pub(crate) max_candidate_paths: usize,
    pub(crate) max_returned_proof_paths: usize,
    pub(crate) max_snippets: usize,
    pub(crate) max_traversal_depth: usize,
    pub(crate) max_path_evidence_rows: usize,
    pub(crate) max_path_edges_per_path: usize,
    pub(crate) max_hydration_bytes: usize,
    pub(crate) path_evidence_timeout_ms: u64,
}

impl ContextPackBudgets {
    pub(crate) fn for_mode(mode: &str) -> Self {
        let normalized = mode.to_ascii_lowercase();
        if normalized.contains("debug") {
            return Self {
                max_seed_entities: 64,
                max_candidate_paths: 512,
                max_returned_proof_paths: 48,
                max_snippets: 64,
                max_traversal_depth: 6,
                max_path_evidence_rows: 512,
                max_path_edges_per_path: 6,
                max_hydration_bytes: 1024 * 1024,
                path_evidence_timeout_ms: 3000,
            };
        }
        if normalized.contains("impact") {
            return Self {
                max_seed_entities: 32,
                max_candidate_paths: 256,
                max_returned_proof_paths: 24,
                max_snippets: 24,
                max_traversal_depth: 4,
                max_path_evidence_rows: 256,
                max_path_edges_per_path: 4,
                max_hydration_bytes: 512 * 1024,
                path_evidence_timeout_ms: 1500,
            };
        }
        Self {
            max_seed_entities: 16,
            max_candidate_paths: 128,
            max_returned_proof_paths: 12,
            max_snippets: 12,
            max_traversal_depth: 3,
            max_path_evidence_rows: 128,
            max_path_edges_per_path: 3,
            max_hydration_bytes: 256 * 1024,
            path_evidence_timeout_ms: 1000,
        }
    }

    pub(crate) fn for_options(options: &ContextPackOptions) -> Self {
        let mut budgets = Self::for_mode(&options.mode);
        if options.output_mode.is_compact() {
            budgets.max_returned_proof_paths = budgets.max_returned_proof_paths.min(
                options
                    .limit_paths
                    .unwrap_or(DEFAULT_CONTEXT_AGENT_PATH_LIMIT),
            );
            budgets.max_snippets = budgets.max_snippets.min(
                options
                    .limit_snippets
                    .unwrap_or(DEFAULT_CONTEXT_AGENT_SNIPPET_LIMIT),
            );
        } else {
            if let Some(limit) = options.limit_paths {
                budgets.max_returned_proof_paths = budgets.max_returned_proof_paths.min(limit);
            }
            if let Some(limit) = options.limit_snippets {
                budgets.max_snippets = budgets.max_snippets.min(limit);
            }
        }
        budgets.max_candidate_paths = budgets
            .max_candidate_paths
            .min(budgets.max_returned_proof_paths.saturating_mul(16).max(16));
        budgets.max_path_evidence_rows = budgets
            .max_path_evidence_rows
            .min(budgets.max_candidate_paths);
        budgets.max_path_edges_per_path = budgets
            .max_path_edges_per_path
            .min(budgets.max_traversal_depth.max(1));
        budgets
    }
}

pub(crate) fn open_context_pack_connection(db_path: &Path) -> Result<Connection, String> {
    if !db_path.exists() {
        return Err(format!(
            "CodeGraph index does not exist at {}; run `codegraph-mcp index .` first",
            db_path.display()
        ));
    }
    let connection = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "
            PRAGMA query_only = ON;
            PRAGMA busy_timeout = 5000;
            ",
        )
        .map_err(|error| error.to_string())?;
    Ok(connection)
}

#[cfg(test)]
pub(crate) fn context_pack_seed_values(
    options: &ContextPackOptions,
    max_seeds: usize,
) -> Vec<String> {
    let prompt_exact = context_pack_prompt_exact_seed_values(&options.task);
    unique_limited_strings(
        options
            .seeds
            .iter()
            .chain(options.stage0_candidates.iter())
            .chain(prompt_exact.iter())
            .cloned(),
        max_seeds.max(1),
    )
}

pub(crate) fn context_pack_seed_values_for_connection(
    connection: &Connection,
    options: &ContextPackOptions,
    max_seeds: usize,
) -> Result<Vec<String>, String> {
    let prompt_exact = context_pack_prompt_exact_seed_values_for_connection(
        connection,
        &options.task,
        max_seeds.max(1),
    )?;
    Ok(unique_limited_strings(
        options
            .seeds
            .iter()
            .chain(options.stage0_candidates.iter())
            .chain(prompt_exact.iter())
            .cloned(),
        max_seeds.max(1),
    ))
}

pub(crate) fn context_pack_prompt_exact_seed_values(task: &str) -> Vec<String> {
    let (positive_task, negative_task) = context_pack_nuance_positive_task(task);
    let positive_lower = positive_task.to_ascii_lowercase();
    let negative_lower = negative_task.to_ascii_lowercase();
    extract_prompt_seeds(task)
        .into_iter()
        .filter_map(|seed| context_pack_prompt_seed_syntactic_exact_value(&seed))
        .filter(|seed| {
            let lower = seed.to_ascii_lowercase();
            positive_lower.contains(&lower) && !negative_lower.contains(&lower)
        })
        .collect()
}

pub(crate) fn context_pack_prompt_exact_seed_values_for_connection(
    connection: &Connection,
    task: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    let (positive_task, negative_task) = context_pack_nuance_positive_task(task);
    let positive_lower = positive_task.to_ascii_lowercase();
    let negative_lower = negative_task.to_ascii_lowercase();
    let mut output = Vec::new();
    for seed in extract_prompt_seeds(task) {
        let Some(value) = context_pack_prompt_seed_syntactic_exact_value(&seed) else {
            continue;
        };
        let lower = value.to_ascii_lowercase();
        if !positive_lower.contains(&lower) || negative_lower.contains(&lower) {
            continue;
        }
        if context_pack_prompt_seed_resolves_in_graph(connection, &value)?
            || (context_pack_prompt_seed_source_is_quoted(&seed)
                && !context_pack_prompt_seed_is_generic_prose(&value))
        {
            output.push(value);
        }
        if output.len() >= limit.max(1) {
            break;
        }
    }
    Ok(unique_limited_strings(output, limit.max(1)))
}

pub(crate) fn context_pack_prompt_seed_hygiene_json(
    connection: &Connection,
    task: &str,
    limit: usize,
) -> Result<Value, String> {
    let (positive_task, negative_task) = context_pack_nuance_positive_task(task);
    let positive_lower = positive_task.to_ascii_lowercase();
    let negative_lower = negative_task.to_ascii_lowercase();
    let mut accepted_exact = Vec::new();
    let mut symbol_seeds = Vec::new();
    let mut file_seeds = Vec::new();
    let mut text_query_terms = Vec::new();
    let mut ignored_prose_terms = Vec::new();
    let mut role_task_terms = Vec::new();
    let mut rejected_exact_candidates = Vec::new();

    for seed in extract_prompt_seeds(task) {
        let value = seed.value.trim().to_string();
        if value.is_empty() {
            continue;
        }
        if matches!(seed.kind, PromptSeedKind::TaskVerbIgnored) {
            role_task_terms.push(value.clone());
            ignored_prose_terms.push(value);
            continue;
        }
        if matches!(
            seed.kind,
            PromptSeedKind::TextToken | PromptSeedKind::ErrorMessage | PromptSeedKind::Unknown
        ) {
            text_query_terms.push(value.clone());
            if context_pack_prompt_seed_is_generic_prose(&value) {
                ignored_prose_terms.push(value);
            }
            continue;
        }

        let exact_value = context_pack_prompt_seed_syntactic_exact_value(&seed);
        let Some(exact_value) = exact_value else {
            text_query_terms.push(value.clone());
            if context_pack_prompt_seed_is_generic_prose(&value) {
                ignored_prose_terms.push(value);
            }
            continue;
        };
        let lower = exact_value.to_ascii_lowercase();
        let in_positive_scope = positive_lower.contains(&lower) && !negative_lower.contains(&lower);
        let resolved = if in_positive_scope {
            context_pack_prompt_seed_resolves_in_graph(connection, &exact_value)?
        } else {
            false
        };
        let quoted_explicit = context_pack_prompt_seed_source_is_quoted(&seed)
            && !context_pack_prompt_seed_is_generic_prose(&exact_value);
        if in_positive_scope && (resolved || quoted_explicit) {
            accepted_exact.push(json!({
                "seed": exact_value.clone(),
                "kind": seed.kind.provenance_kind(),
                "resolution": if resolved { "graph_dictionary" } else { "quoted_explicit" },
                "source_text": seed.source_text.clone(),
            }));
            match seed.kind {
                PromptSeedKind::FilePath
                | PromptSeedKind::LineNumber
                | PromptSeedKind::StackTrace
                | PromptSeedKind::PathToken => file_seeds.push(exact_value),
                _ => symbol_seeds.push(exact_value),
            }
        } else {
            if context_pack_prompt_seed_is_generic_prose(&exact_value) {
                ignored_prose_terms.push(exact_value.clone());
            } else {
                text_query_terms.push(exact_value.clone());
            }
            rejected_exact_candidates.push(json!({
                "seed": exact_value.clone(),
                "kind": seed.kind.provenance_kind(),
                "reason": if !in_positive_scope {
                    "outside_positive_task_scope"
                } else if context_pack_prompt_seed_is_generic_prose(&value) {
                    "generic_prose_term"
                } else {
                    "not_in_symbol_or_path_dictionary"
                },
                "source_text": seed.source_text.clone(),
            }));
        }
    }

    let accepted_count = accepted_exact.len();
    if accepted_exact.len() > limit.max(1) {
        accepted_exact.truncate(limit.max(1));
    }
    Ok(json!({
        "schema_version": 1,
        "diagnostic_only": true,
        "accepted_exact_seeds": accepted_exact,
        "accepted_exact_count": accepted_count,
        "accepted_exact_cap": limit.max(1),
        "symbol_seeds": unique_limited_strings(symbol_seeds, limit.max(1)),
        "file_seeds": unique_limited_strings(file_seeds, limit.max(1)),
        "text_query_terms": unique_limited_strings(text_query_terms, limit.saturating_mul(2).max(1)),
        "ignored_prose_terms": unique_limited_strings(ignored_prose_terms, limit.saturating_mul(2).max(1)),
        "role_task_terms": unique_limited_strings(role_task_terms, limit.saturating_mul(2).max(1)),
        "rejected_exact_seed_candidates": rejected_exact_candidates,
        "seed_hygiene_contract": "prompt-derived exact graph seeds must resolve through graph dictionaries or be explicitly quoted; nonmatching prose remains text evidence or ignored intent"
    }))
}

fn context_pack_prompt_seed_syntactic_exact_value(seed: &PromptSeed) -> Option<String> {
    let value = seed.exact_value()?;
    if context_pack_prompt_seed_is_generic_prose(&value) {
        return None;
    }
    match seed.kind {
        PromptSeedKind::Symbol
        | PromptSeedKind::TestName
        | PromptSeedKind::Identifier
        | PromptSeedKind::ConfigToken => Some(value),
        PromptSeedKind::FilePath
        | PromptSeedKind::LineNumber
        | PromptSeedKind::StackTrace
        | PromptSeedKind::PathToken => {
            if context_pack_prompt_seed_looks_path_like(&value)
                || context_pack_prompt_seed_source_is_quoted(seed)
            {
                Some(value)
            } else {
                None
            }
        }
        PromptSeedKind::FilePattern
        | PromptSeedKind::TextToken
        | PromptSeedKind::ErrorMessage
        | PromptSeedKind::TaskVerbIgnored
        | PromptSeedKind::Unknown => None,
    }
}

fn context_pack_prompt_seed_resolves_in_graph(
    connection: &Connection,
    value: &str,
) -> Result<bool, String> {
    if lookup_i64_if_available(connection, "object_id_lookup", value)?.is_some()
        || lookup_i64_if_available(connection, "symbol_dict", value)?.is_some()
        || lookup_i64_if_available(connection, "qualified_name_dict", value)?.is_some()
        || lookup_i64_if_available(connection, "path_dict", value)?.is_some()
    {
        return Ok(true);
    }
    let normalized = value.replace('\\', "/");
    if normalized != value
        && lookup_i64_if_available(connection, "path_dict", &normalized)?.is_some()
    {
        return Ok(true);
    }
    if let Some((path, line)) = normalized.rsplit_once(':') {
        if !path.is_empty()
            && line.chars().all(|character| character.is_ascii_digit())
            && lookup_i64_if_available(connection, "path_dict", path)?.is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn lookup_i64_if_available(
    connection: &Connection,
    table: &str,
    value: &str,
) -> Result<Option<i64>, String> {
    match lookup_i64(connection, table, value) {
        Ok(value) => Ok(value),
        Err(error)
            if error.contains("no such table")
                || error.contains("no such column")
                || error.contains("no such view") =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn context_pack_prompt_seed_source_is_quoted(seed: &PromptSeed) -> bool {
    let source = seed.source_text.trim();
    source.len() >= 2
        && ((source.starts_with('"') && source.ends_with('"'))
            || (source.starts_with('\'') && source.ends_with('\''))
            || (source.starts_with('`') && source.ends_with('`')))
}

fn context_pack_prompt_seed_looks_path_like(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    value.contains('/')
        || value.contains('\\')
        || [
            ".rs",
            ".py",
            ".js",
            ".jsx",
            ".ts",
            ".tsx",
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
            ".sh",
        ]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn context_pack_prompt_seed_is_generic_prose(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "markdown"
            | "function"
            | "functions"
            | "file"
            | "files"
            | "code"
            | "build"
            | "package"
            | "packages"
            | "docs"
            | "doc"
            | "documentation"
            | "module"
            | "modules"
            | "crate"
            | "crates"
            | "repo"
            | "repository"
            | "project"
            | "agent-use"
            | "agent"
            | "routing"
            | "route"
            | "context"
            | "packet"
            | "pack"
    )
}

pub(crate) fn unique_limited_strings(
    values: impl IntoIterator<Item = String>,
    limit: usize,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            continue;
        }
        output.push(trimmed.to_string());
        if output.len() >= limit {
            break;
        }
    }
    output
}

pub(crate) fn context_seed_ids(
    raw_seed_values: &[String],
    seed_entities: &[ContextEntitySummary],
    limit: usize,
) -> Vec<String> {
    unique_limited_strings(
        raw_seed_values
            .iter()
            .cloned()
            .chain(seed_entities.iter().map(|entity| entity.id.clone())),
        limit.max(1),
    )
}

#[derive(Debug, Clone)]
pub(crate) struct ContextEntitySummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) qualified_name: String,
    pub(crate) repo_relative_path: String,
    pub(crate) kind: Option<EntityKind>,
    pub(crate) source_span: Option<SourceSpan>,
    pub(crate) metadata: Metadata,
}

pub(crate) fn resolve_context_seed_entities(
    connection: &Connection,
    seeds: &[String],
    limit: usize,
) -> Result<Vec<ContextEntitySummary>, String> {
    if seeds.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let mut entity_keys = BTreeSet::<i64>::new();
    for seed in seeds {
        if entity_keys.len() >= limit {
            break;
        }
        if let Some(key) = lookup_i64(connection, "object_id_lookup", seed)? {
            entity_keys.insert(key);
        }
        if entity_keys.len() >= limit {
            break;
        }
        if let Some(name_id) = lookup_i64(connection, "symbol_dict", seed)? {
            let remaining = limit.saturating_sub(entity_keys.len());
            for key in entity_keys_by_column(connection, "name_id", name_id, remaining)? {
                entity_keys.insert(key);
                if entity_keys.len() >= limit {
                    break;
                }
            }
        }
        if entity_keys.len() >= limit {
            break;
        }
        if let Some(qname_id) = lookup_i64(connection, "qualified_name_dict", seed)? {
            let remaining = limit.saturating_sub(entity_keys.len());
            for key in entity_keys_by_column(connection, "qualified_name_id", qname_id, remaining)?
            {
                entity_keys.insert(key);
                if entity_keys.len() >= limit {
                    break;
                }
            }
        }
    }
    if entity_keys.is_empty() {
        return Ok(Vec::new());
    }
    load_context_entities_by_keys(
        connection,
        &entity_keys.into_iter().collect::<Vec<_>>(),
        limit,
    )
}

pub(crate) fn lookup_i64(
    connection: &Connection,
    table: &str,
    value: &str,
) -> Result<Option<i64>, String> {
    if !table
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(format!("invalid dictionary table name: {table}"));
    }
    connection
        .query_row(
            &format!("SELECT id FROM {table} WHERE value = ?1"),
            [value],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub(crate) fn entity_keys_by_column(
    connection: &Connection,
    column: &str,
    value: i64,
    limit: usize,
) -> Result<Vec<i64>, String> {
    if limit == 0
        || !matches!(
            column,
            "name_id" | "qualified_name_id" | "path_id" | "id_key"
        )
    {
        return Ok(Vec::new());
    }
    let sql = format!("SELECT id_key FROM entities WHERE {column} = ?1 ORDER BY id_key LIMIT ?2");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![value, limit as i64], |row| row.get::<_, i64>(0))
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(crate) fn load_context_entities_by_keys(
    connection: &Connection,
    keys: &[i64],
    limit: usize,
) -> Result<Vec<ContextEntitySummary>, String> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let keys = keys.iter().take(limit).copied().collect::<Vec<_>>();
    let placeholders = sql_placeholders(keys.len());
    let sql = format!(
        "
        SELECT oid.value AS id, name.value AS name,
               qname.value AS qualified_name, path.value AS repo_relative_path,
               kind.value AS kind, span_path.value AS span_repo_relative_path,
               e.start_line, e.start_column, e.end_line, e.end_column,
               e.metadata_json
        FROM entities e
        JOIN object_id_lookup oid ON oid.id = e.id_key
        JOIN symbol_dict name ON name.id = e.name_id
        JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
        JOIN path_dict path ON path.id = e.path_id
        LEFT JOIN entity_kind_dict kind ON kind.id = e.kind_id
        LEFT JOIN path_dict span_path ON span_path.id = e.span_path_id
        WHERE e.id_key IN ({placeholders})
        ORDER BY qname.value, oid.value
        LIMIT {}
        ",
        limit as i64
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(keys.iter()), |row| {
            let span_repo_relative_path =
                row.get::<_, Option<String>>("span_repo_relative_path")?;
            let start_line = row.get::<_, Option<u32>>("start_line")?;
            let end_line = row.get::<_, Option<u32>>("end_line")?;
            let source_span = match (span_repo_relative_path, start_line, end_line) {
                (Some(path), Some(start_line), Some(end_line)) => Some(SourceSpan {
                    repo_relative_path: path,
                    start_line,
                    start_column: row.get::<_, Option<u32>>("start_column")?,
                    end_line,
                    end_column: row.get::<_, Option<u32>>("end_column")?,
                }),
                _ => None,
            };
            let metadata_json = row
                .get::<_, Option<String>>("metadata_json")?
                .unwrap_or_else(|| "{}".to_string());
            Ok(ContextEntitySummary {
                id: row.get("id")?,
                name: row.get("name")?,
                qualified_name: row.get("qualified_name")?,
                repo_relative_path: row.get("repo_relative_path")?,
                kind: row
                    .get::<_, Option<String>>("kind")?
                    .as_deref()
                    .and_then(|raw| raw.parse().ok()),
                source_span,
                metadata: serde_json::from_str::<Metadata>(&metadata_json)
                    .map_err(sql_json_error)?,
            })
        })
        .map_err(|error| error.to_string())?;
    collect_sql_rows(rows)
}

pub(crate) fn load_context_entities_by_ids(
    connection: &Connection,
    ids: impl IntoIterator<Item = String>,
    limit: usize,
) -> Result<BTreeMap<String, ContextEntitySummary>, String> {
    if limit == 0 {
        return Ok(BTreeMap::new());
    }
    let mut entity_keys = BTreeSet::<i64>::new();
    for id in ids {
        if entity_keys.len() >= limit {
            break;
        }
        if let Some(key) = lookup_i64(connection, "object_id_lookup", &id)? {
            entity_keys.insert(key);
        }
    }
    let entities = load_context_entities_by_keys(
        connection,
        &entity_keys.into_iter().collect::<Vec<_>>(),
        limit,
    )?;
    Ok(entities
        .into_iter()
        .map(|entity| (entity.id.clone(), entity))
        .collect())
}

pub(crate) fn load_context_entities_by_paths(
    connection: &Connection,
    paths: impl IntoIterator<Item = String>,
    limit: usize,
) -> Result<Vec<ContextEntitySummary>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut entity_keys = BTreeSet::<i64>::new();
    for path in paths {
        if entity_keys.len() >= limit {
            break;
        }
        let Some(path_id) = lookup_i64(connection, "path_dict", &path)? else {
            continue;
        };
        let remaining = limit.saturating_sub(entity_keys.len());
        for key in entity_keys_by_column(connection, "path_id", path_id, remaining)? {
            entity_keys.insert(key);
            if entity_keys.len() >= limit {
                break;
            }
        }
    }
    load_context_entities_by_keys(
        connection,
        &entity_keys.into_iter().collect::<Vec<_>>(),
        limit,
    )
}

#[derive(Debug, Clone)]
pub(crate) struct StoredContextPathEvidenceLoad {
    pub(crate) paths: Vec<PathEvidence>,
    pub(crate) telemetry: Value,
}

pub(crate) fn load_stored_context_path_evidence(
    connection: &Connection,
    seed_ids: &[String],
    mode: &str,
    budgets: ContextPackBudgets,
) -> Result<StoredContextPathEvidenceLoad, String> {
    if seed_ids.is_empty()
        || budgets.max_candidate_paths == 0
        || budgets.max_path_evidence_rows == 0
    {
        return Ok(StoredContextPathEvidenceLoad {
            paths: Vec::new(),
            telemetry: context_pack_empty_path_evidence_telemetry(seed_ids.len(), budgets),
        });
    }
    let lookup_start = Instant::now();
    let use_symbols = sqlite_table_exists(connection, "path_evidence_symbols")?
        && sqlite_row_count(connection, "path_evidence_symbols")? > 0;
    let placeholders = sql_placeholders(seed_ids.len());
    let sql = if use_symbols {
        format!(
            "
            SELECT DISTINCT p.id, p.source, p.target, p.summary, p.metapath_json,
                   p.edges_json, p.source_spans_json, p.exactness, p.length,
                   p.confidence, p.metadata_json
            FROM path_evidence p
            JOIN path_evidence_symbols s ON s.path_id = p.id
            WHERE s.entity_id IN ({placeholders})
              AND p.length <= ?{}
            ORDER BY p.confidence DESC, p.length ASC, p.id
            LIMIT ?{}
            ",
            seed_ids.len() + 1,
            seed_ids.len() + 2
        )
    } else {
        format!(
            "
            SELECT p.id, p.source, p.target, p.summary, p.metapath_json,
                   p.edges_json, p.source_spans_json, p.exactness, p.length,
                   p.confidence, p.metadata_json
            FROM path_evidence p
            WHERE (p.source IN ({placeholders}) OR p.target IN ({placeholders}))
              AND p.length <= ?{}
            ORDER BY p.confidence DESC, p.length ASC, p.id
            LIMIT ?{}
            ",
            seed_ids.len() * 2 + 1,
            seed_ids.len() * 2 + 2
        )
    };
    let mut params = Vec::<String>::new();
    params.extend(seed_ids.iter().cloned());
    if !use_symbols {
        params.extend(seed_ids.iter().cloned());
    }
    params.push(budgets.max_path_edges_per_path.to_string());
    params.push((budgets.max_path_evidence_rows + 1).to_string());
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            rusqlite::params_from_iter(params.iter()),
            path_evidence_from_sql_row,
        )
        .map_err(|error| error.to_string())?;
    let mut paths = collect_sql_rows(rows)?;
    let lookup_truncated = paths.len() > budgets.max_path_evidence_rows;
    let lookup_omitted_lower_bound = paths.len().saturating_sub(budgets.max_path_evidence_rows);
    paths.truncate(budgets.max_path_evidence_rows);
    let lookup_time_ms = lookup_start.elapsed().as_secs_f64() * 1000.0;
    let rows_read = paths.len();
    let bytes_read = serde_json::to_vec(&paths)
        .map(|bytes| bytes.len())
        .unwrap_or_default();
    let hydration_start = Instant::now();
    let hydration_stats = hydrate_stored_path_evidence_metadata(
        connection,
        &mut paths,
        ContextPathEvidenceHydrationBudget::for_budgets(budgets, rows_read),
    )?;
    let hydration_time_ms = hydration_start.elapsed().as_secs_f64() * 1000.0;
    let filtered = filter_and_sort_context_path_evidence(paths, mode, budgets);
    let filtered_omitted = rows_read.saturating_sub(filtered.len());
    let path_evidence_omitted_count = lookup_omitted_lower_bound.saturating_add(filtered_omitted);
    let path_evidence_truncated = lookup_truncated || hydration_stats.truncated;
    let storage_contributors = context_pack_path_evidence_storage_contributors(connection)
        .unwrap_or_else(|error| json!({ "status": "unavailable", "error": error }));
    let telemetry = json!({
        "schema_version": 1,
        "diagnostic_only": true,
        "measurement_scope": "context_pack_stored_path_evidence",
        "max_path_evidence_rows": budgets.max_path_evidence_rows,
        "max_edges_per_path": budgets.max_path_edges_per_path,
        "max_snippets": budgets.max_snippets,
        "max_hydration_bytes": budgets.max_hydration_bytes,
        "max_time_budget_ms": budgets.path_evidence_timeout_ms,
        "lookup_query_count": if use_symbols { 3 } else { 3 },
        "lookup_time_ms": lookup_time_ms,
        "hydration_query_count": if rows_read == 0 { 0 } else { 1 },
        "hydration_time_ms": hydration_time_ms,
        "source_span_load_time_ms": Value::Null,
        "snippet_load_time_ms": Value::Null,
        "n_plus_one_query_count": 0,
        "n_plus_one_detectable": true,
        "rows_read": rows_read,
        "bytes_read": bytes_read,
        "hydrated_edge_rows_read": hydration_stats.loaded_edge_rows,
        "hydration_bytes_read": hydration_stats.bytes_read,
        "hydration_budget_exhausted": hydration_stats.budget_exhausted,
        "hydration_stop_reason": hydration_stats.stop_reason,
        "path_evidence_edge_rows_omitted": hydration_stats.omitted_edge_rows,
        "path_evidence_rows_returned": filtered.len(),
        "path_evidence_rows_omitted": path_evidence_omitted_count,
        "path_evidence_omitted_count": path_evidence_omitted_count,
        "path_evidence_truncated": path_evidence_truncated,
        "source_snippet_omitted_count": Value::Null,
        "lookup_strategy": if use_symbols { "path_evidence_symbols_join" } else { "source_target_scan" },
        "storage_contributors": storage_contributors,
        "notes": [
            "rows_scanned is not available from rusqlite",
            "bytes_read is serialized PathEvidence payload size, not SQLite page IO",
            "hydration uses one bulk materialized edge/entity query; no N+1 query pattern was detected in this loader",
            "path_evidence_omitted_count is a bounded lower-bound count from lookup cap plus source-role/proof filtering"
        ]
    });
    Ok(StoredContextPathEvidenceLoad {
        paths: filtered,
        telemetry,
    })
}

pub(crate) fn context_pack_empty_path_evidence_telemetry(
    seed_count: usize,
    budgets: ContextPackBudgets,
) -> Value {
    json!({
        "schema_version": 1,
        "diagnostic_only": true,
        "measurement_scope": "context_pack_stored_path_evidence",
        "max_path_evidence_rows": budgets.max_path_evidence_rows,
        "max_edges_per_path": budgets.max_path_edges_per_path,
        "max_snippets": budgets.max_snippets,
        "max_hydration_bytes": budgets.max_hydration_bytes,
        "max_time_budget_ms": budgets.path_evidence_timeout_ms,
        "lookup_query_count": 0,
        "lookup_time_ms": 0.0,
        "hydration_query_count": 0,
        "hydration_time_ms": 0.0,
        "source_span_load_time_ms": Value::Null,
        "snippet_load_time_ms": Value::Null,
        "n_plus_one_query_count": 0,
        "n_plus_one_detectable": true,
        "rows_read": 0,
        "bytes_read": 0,
        "hydrated_edge_rows_read": 0,
        "hydration_bytes_read": 0,
        "hydration_budget_exhausted": false,
        "hydration_stop_reason": Value::Null,
        "path_evidence_edge_rows_omitted": 0,
        "path_evidence_rows_returned": 0,
        "path_evidence_rows_omitted": 0,
        "path_evidence_omitted_count": 0,
        "path_evidence_truncated": false,
        "source_snippet_omitted_count": 0,
        "lookup_strategy": "skipped_empty_seed_or_zero_budget",
        "seed_count": seed_count,
        "max_candidate_paths": budgets.max_candidate_paths,
        "notes": [
            "PathEvidence lookup skipped because no seed ids or candidate path budget were available"
        ]
    })
}

pub(crate) fn context_pack_path_evidence_storage_contributors(
    connection: &Connection,
) -> Result<Value, String> {
    let tables = [
        "path_evidence",
        "path_evidence_lookup",
        "path_evidence_edges",
        "path_evidence_debug_metadata",
        "path_evidence_symbols",
        "path_evidence_tests",
        "path_evidence_files",
        "file_path_evidence",
    ];
    let mut contributors = Vec::new();
    for table in tables {
        if sqlite_table_exists(connection, table)? {
            contributors.push(json!({
                "table": table,
                "row_count": sqlite_row_count(connection, table).unwrap_or_default(),
            }));
        }
    }
    Ok(json!({
        "status": "row_counts_only",
        "contributors": contributors,
        "bytes_available": false,
        "note": "table/index byte contributors require dbstat or audit storage; this lightweight path emits row counts only"
    }))
}

#[derive(Debug)]
pub(crate) struct ContextPathEvidenceHydrationBudget {
    max_edges_per_path: usize,
    max_total_edge_rows: usize,
    max_hydration_bytes: usize,
    timeout_ms: u64,
}

impl ContextPathEvidenceHydrationBudget {
    fn for_budgets(budgets: ContextPackBudgets, path_count: usize) -> Self {
        let max_edges_per_path = budgets.max_path_edges_per_path.max(1);
        Self {
            max_edges_per_path,
            max_total_edge_rows: path_count.saturating_mul(max_edges_per_path).max(1),
            max_hydration_bytes: budgets.max_hydration_bytes.max(1),
            timeout_ms: budgets.path_evidence_timeout_ms.max(1),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ContextPathEvidenceHydrationStats {
    loaded_edge_rows: usize,
    omitted_edge_rows: usize,
    bytes_read: usize,
    truncated: bool,
    budget_exhausted: bool,
    stop_reason: Value,
}

impl ContextPathEvidenceHydrationStats {
    fn note_budget_stop(&mut self, reason: &'static str) {
        self.truncated = true;
        self.budget_exhausted = true;
        if self.stop_reason.is_null() {
            self.stop_reason = json!(reason);
        }
    }
}

#[derive(Debug)]
pub(crate) struct StoredContextPathEdgeMetadata {
    path_id: String,
    ordinal: usize,
    edge_id: String,
    head_id: String,
    relation: String,
    tail_id: String,
    source_span_path: Option<String>,
    exactness: Option<String>,
    confidence: Option<f64>,
    derived: bool,
    edge_class: Option<String>,
    context: Option<String>,
    provenance_edges: Vec<String>,
    head_entity: Option<StoredContextEntityMetadata>,
    tail_entity: Option<StoredContextEntityMetadata>,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredContextEntityMetadata {
    id: String,
    kind: Option<EntityKind>,
    name: String,
    qualified_name: String,
    repo_relative_path: String,
    source_span: Option<SourceSpan>,
    metadata: Metadata,
}

#[derive(Debug, Clone)]
pub(crate) struct CliEvidenceRoleDecision {
    pub(crate) role: EvidenceRole,
    pub(crate) reason: String,
    pub(crate) source: String,
}

impl CliEvidenceRoleDecision {
    fn new(role: EvidenceRole, reason: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            role,
            reason: reason.into(),
            source: source.into(),
        }
    }
}

pub(crate) fn stored_context_entity_from_row(
    row: &rusqlite::Row<'_>,
    prefix: &str,
) -> rusqlite::Result<Option<StoredContextEntityMetadata>> {
    let kind_column = format!("{prefix}_kind");
    let name_column = format!("{prefix}_name");
    let qname_column = format!("{prefix}_qualified_name");
    let path_column = format!("{prefix}_repo_relative_path");
    let span_path_column = format!("{prefix}_span_repo_relative_path");
    let start_line_column = format!("{prefix}_start_line");
    let start_column_column = format!("{prefix}_start_column");
    let end_line_column = format!("{prefix}_end_line");
    let end_column_column = format!("{prefix}_end_column");
    let metadata_column = format!("{prefix}_metadata_json");

    let Some(name) = row.get::<_, Option<String>>(name_column.as_str())? else {
        return Ok(None);
    };
    let kind_raw = row.get::<_, Option<String>>(kind_column.as_str())?;
    let qualified_name = row
        .get::<_, Option<String>>(qname_column.as_str())?
        .unwrap_or_else(|| name.clone());
    let repo_relative_path = row
        .get::<_, Option<String>>(path_column.as_str())?
        .unwrap_or_default();
    let span_repo_relative_path = row.get::<_, Option<String>>(span_path_column.as_str())?;
    let start_line = row.get::<_, Option<u32>>(start_line_column.as_str())?;
    let end_line = row.get::<_, Option<u32>>(end_line_column.as_str())?;
    let source_span = match (span_repo_relative_path, start_line, end_line) {
        (Some(path), Some(start_line), Some(end_line)) => Some(SourceSpan {
            repo_relative_path: path,
            start_line,
            start_column: row.get::<_, Option<u32>>(start_column_column.as_str())?,
            end_line,
            end_column: row.get::<_, Option<u32>>(end_column_column.as_str())?,
        }),
        _ => None,
    };
    let metadata_json = row
        .get::<_, Option<String>>(metadata_column.as_str())?
        .unwrap_or_else(|| "{}".to_string());
    let metadata = serde_json::from_str::<Metadata>(&metadata_json).map_err(sql_json_error)?;
    let id_column = format!("{prefix}_id");
    Ok(Some(StoredContextEntityMetadata {
        id: row.get::<_, String>(id_column.as_str()).unwrap_or_default(),
        kind: kind_raw.as_deref().and_then(|raw| raw.parse().ok()),
        name,
        qualified_name,
        repo_relative_path,
        source_span,
        metadata,
    }))
}

pub(crate) fn stored_context_entity_role(
    entity: Option<&StoredContextEntityMetadata>,
) -> CliEvidenceRoleDecision {
    let Some(entity) = entity else {
        return CliEvidenceRoleDecision::new(
            EvidenceRole::Unknown,
            "missing endpoint entity metadata",
            "fallback",
        );
    };
    if let Some(kind) = entity.kind {
        let classified = classify_entity_source_role(&Entity {
            id: entity.id.clone(),
            kind,
            name: entity.name.clone(),
            qualified_name: entity.qualified_name.clone(),
            repo_relative_path: entity.repo_relative_path.clone(),
            source_span: entity.source_span.clone(),
            content_hash: None,
            file_hash: None,
            created_from: "context-pack-hydration".to_string(),
            confidence: 1.0,
            metadata: entity.metadata.clone(),
        });
        return CliEvidenceRoleDecision::new(
            classified.role,
            classified.reason,
            classified.classification_source,
        );
    }
    if qualified_name_has_test_module(&entity.qualified_name) {
        return CliEvidenceRoleDecision::new(
            EvidenceRole::Test,
            "qualified name contains tests module",
            "qualified_name",
        );
    }
    CliEvidenceRoleDecision::new(
        EvidenceRole::Unknown,
        "endpoint entity kind/source role metadata missing",
        "fallback",
    )
}

pub(crate) fn context_entity_role(entity: &ContextEntitySummary) -> CliEvidenceRoleDecision {
    if let Some(kind) = entity.kind {
        let classified = classify_entity_source_role(&Entity {
            id: entity.id.clone(),
            kind,
            name: entity.name.clone(),
            qualified_name: entity.qualified_name.clone(),
            repo_relative_path: entity.repo_relative_path.clone(),
            source_span: entity.source_span.clone(),
            content_hash: None,
            file_hash: None,
            created_from: "context-pack-seed".to_string(),
            confidence: 1.0,
            metadata: entity.metadata.clone(),
        });
        return CliEvidenceRoleDecision::new(
            classified.role,
            classified.reason,
            classified.classification_source,
        );
    }
    if qualified_name_has_test_module(&entity.qualified_name) {
        return CliEvidenceRoleDecision::new(
            EvidenceRole::Test,
            "qualified name contains tests module",
            "qualified_name",
        );
    }
    CliEvidenceRoleDecision::new(
        EvidenceRole::Unknown,
        "entity kind/source role metadata missing",
        "fallback",
    )
}

pub(crate) fn stored_context_edge_role(
    row: &StoredContextPathEdgeMetadata,
) -> CliEvidenceRoleDecision {
    let relation_role = match row.relation.as_str() {
        "MOCKS" | "STUBS" => Some((EvidenceRole::Mock, "relation kind is mock/stub evidence")),
        "TESTS" | "ASSERTS" | "COVERS" | "FIXTURES_FOR" => Some((
            EvidenceRole::Test,
            "relation kind is test/assertion evidence",
        )),
        _ => None,
    };
    if let Some((role, reason)) = relation_role {
        return CliEvidenceRoleDecision::new(role, reason, "relation_kind");
    }
    if row
        .source_span_path
        .as_deref()
        .is_some_and(context_pack_test_path)
    {
        return CliEvidenceRoleDecision::new(
            EvidenceRole::Test,
            "edge source span is in a test/spec path",
            "file_path",
        );
    }

    let head = stored_context_entity_role(row.head_entity.as_ref());
    let tail = stored_context_entity_role(row.tail_entity.as_ref());
    let endpoint_role = combine_evidence_roles([head.role, tail.role]);
    if endpoint_role != EvidenceRole::Unknown {
        return CliEvidenceRoleDecision::new(
            endpoint_role,
            format!("head: {}; tail: {}", head.reason, tail.reason),
            format!("endpoint:{}+{}", head.source, tail.source),
        );
    }

    match row
        .context
        .as_deref()
        .and_then(context_pack_role_from_label)
    {
        Some(EvidenceRole::Test | EvidenceRole::Mock | EvidenceRole::Mixed) => {
            let role = context_pack_role_from_label(row.context.as_deref().unwrap_or_default())
                .unwrap_or(EvidenceRole::Unknown);
            CliEvidenceRoleDecision::new(role, "materialized edge context", "materialized_context")
        }
        Some(EvidenceRole::Production) => CliEvidenceRoleDecision::new(
            EvidenceRole::Unknown,
            "materialized production context lacks source-role metadata",
            "fallback",
        ),
        Some(EvidenceRole::Unknown) | None => CliEvidenceRoleDecision::new(
            EvidenceRole::Unknown,
            "missing edge source-role metadata",
            "fallback",
        ),
    }
}

pub(crate) fn qualified_name_has_test_module(value: &str) -> bool {
    let normalized = value.replace('\\', "/").to_ascii_lowercase();
    normalized == "tests"
        || normalized.starts_with("tests.")
        || normalized.contains(".tests.")
        || normalized.contains("::tests::")
        || normalized.ends_with(".tests")
        || normalized.ends_with("::tests")
}

pub(crate) fn context_pack_test_path(path: &str) -> bool {
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

pub(crate) fn context_pack_mock_name(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("mock") || normalized.contains("stub")
}

pub(crate) fn context_pack_role_from_label(value: &str) -> Option<EvidenceRole> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.contains("mixed") {
        Some(EvidenceRole::Mixed)
    } else if normalized.contains("mock") || normalized.contains("stub") {
        Some(EvidenceRole::Mock)
    } else if normalized.contains("test") || normalized.contains("spec") {
        Some(EvidenceRole::Test)
    } else if normalized.contains("production") {
        Some(EvidenceRole::Production)
    } else if normalized.contains("unknown") || normalized.contains("unresolved") {
        Some(EvidenceRole::Unknown)
    } else {
        None
    }
}

pub(crate) fn stored_context_edge_metadata_approx_bytes(
    row: &StoredContextPathEdgeMetadata,
) -> usize {
    row.path_id.len()
        + row.edge_id.len()
        + row.head_id.len()
        + row.relation.len()
        + row.tail_id.len()
        + row.source_span_path.as_ref().map(String::len).unwrap_or(0)
        + row.exactness.as_ref().map(String::len).unwrap_or(0)
        + row.edge_class.as_ref().map(String::len).unwrap_or(0)
        + row.context.as_ref().map(String::len).unwrap_or(0)
        + row.provenance_edges.iter().map(String::len).sum::<usize>()
        + row
            .head_entity
            .as_ref()
            .map(stored_context_entity_approx_bytes)
            .unwrap_or(0)
        + row
            .tail_entity
            .as_ref()
            .map(stored_context_entity_approx_bytes)
            .unwrap_or(0)
        + 64
}

pub(crate) fn stored_context_entity_approx_bytes(entity: &StoredContextEntityMetadata) -> usize {
    entity.id.len()
        + entity.name.len()
        + entity.qualified_name.len()
        + entity.repo_relative_path.len()
        + serde_json::to_vec(&entity.metadata)
            .map(|bytes| bytes.len())
            .unwrap_or_default()
        + 32
}

pub(crate) fn hydrate_stored_path_evidence_metadata(
    connection: &Connection,
    paths: &mut [PathEvidence],
    budget: ContextPathEvidenceHydrationBudget,
) -> Result<ContextPathEvidenceHydrationStats, String> {
    let start = Instant::now();
    let mut stats = ContextPathEvidenceHydrationStats::default();
    if paths.is_empty()
        || !sqlite_table_exists(connection, "path_evidence_edges")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "exactness")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "confidence")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "derived")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "edge_class")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "context")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "provenance_edges_json")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "head_id")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "tail_id")?
    {
        return Ok(stats);
    }
    let ids = paths.iter().map(|path| path.id.clone()).collect::<Vec<_>>();
    let placeholders = sql_placeholders(ids.len());
    let sql = format!(
        "
        SELECT pe.path_id, pe.ordinal, pe.edge_id, pe.head_id, pe.relation, pe.tail_id,
               pe.source_span_path, pe.exactness, pe.confidence, pe.derived,
               pe.edge_class, pe.context, pe.provenance_edges_json,
               head_kind.value AS head_kind, head_name.value AS head_name,
               head_qname.value AS head_qualified_name, head_path.value AS head_repo_relative_path,
               head_span_path.value AS head_span_repo_relative_path,
               head_e.start_line AS head_start_line, head_e.start_column AS head_start_column,
               head_e.end_line AS head_end_line, head_e.end_column AS head_end_column,
               head_e.metadata_json AS head_metadata_json,
               tail_kind.value AS tail_kind, tail_name.value AS tail_name,
               tail_qname.value AS tail_qualified_name, tail_path.value AS tail_repo_relative_path,
               tail_span_path.value AS tail_span_repo_relative_path,
               tail_e.start_line AS tail_start_line, tail_e.start_column AS tail_start_column,
               tail_e.end_line AS tail_end_line, tail_e.end_column AS tail_end_column,
               tail_e.metadata_json AS tail_metadata_json
        FROM path_evidence_edges pe
        LEFT JOIN object_id_lookup head_oid ON head_oid.value = pe.head_id
        LEFT JOIN entities head_e ON head_e.id_key = head_oid.id
        LEFT JOIN entity_kind_dict head_kind ON head_kind.id = head_e.kind_id
        LEFT JOIN symbol_dict head_name ON head_name.id = head_e.name_id
        LEFT JOIN qualified_name_lookup head_qname ON head_qname.id = head_e.qualified_name_id
        LEFT JOIN path_dict head_path ON head_path.id = head_e.path_id
        LEFT JOIN path_dict head_span_path ON head_span_path.id = head_e.span_path_id
        LEFT JOIN object_id_lookup tail_oid ON tail_oid.value = pe.tail_id
        LEFT JOIN entities tail_e ON tail_e.id_key = tail_oid.id
        LEFT JOIN entity_kind_dict tail_kind ON tail_kind.id = tail_e.kind_id
        LEFT JOIN symbol_dict tail_name ON tail_name.id = tail_e.name_id
        LEFT JOIN qualified_name_lookup tail_qname ON tail_qname.id = tail_e.qualified_name_id
        LEFT JOIN path_dict tail_path ON tail_path.id = tail_e.path_id
        LEFT JOIN path_dict tail_span_path ON tail_span_path.id = tail_e.span_path_id
        WHERE pe.path_id IN ({placeholders})
          AND pe.ordinal < ?{}
        ORDER BY pe.path_id, pe.ordinal
        LIMIT ?{}
        ",
        ids.len() + 1,
        ids.len() + 2
    );
    let mut params = ids.iter().map(String::as_str).collect::<Vec<_>>();
    let max_edges_per_path = budget.max_edges_per_path.to_string();
    let max_total_edge_rows = (budget.max_total_edge_rows + 1).to_string();
    params.push(max_edges_per_path.as_str());
    params.push(max_total_edge_rows.as_str());
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(params), |row| {
            let provenance_json: Option<String> = row.get("provenance_edges_json")?;
            let provenance_edges = provenance_json
                .as_deref()
                .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
                .unwrap_or_default();
            Ok(StoredContextPathEdgeMetadata {
                path_id: row.get("path_id")?,
                ordinal: row.get::<_, i64>("ordinal")?.max(0) as usize,
                edge_id: row.get("edge_id")?,
                head_id: row.get("head_id")?,
                relation: row.get("relation")?,
                tail_id: row.get("tail_id")?,
                source_span_path: row.get("source_span_path")?,
                exactness: row.get("exactness")?,
                confidence: row.get("confidence")?,
                derived: row.get::<_, i64>("derived")? != 0,
                edge_class: row.get("edge_class")?,
                context: row.get("context")?,
                provenance_edges,
                head_entity: stored_context_entity_from_row(row, "head")?,
                tail_entity: stored_context_entity_from_row(row, "tail")?,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut by_path = BTreeMap::<String, Vec<StoredContextPathEdgeMetadata>>::new();
    for row in rows {
        if start.elapsed() > Duration::from_millis(budget.timeout_ms) {
            stats.note_budget_stop("timeout_ms");
            break;
        }
        let row = row.map_err(|error| error.to_string())?;
        if stats.loaded_edge_rows >= budget.max_total_edge_rows {
            stats.omitted_edge_rows = stats.omitted_edge_rows.saturating_add(1);
            stats.note_budget_stop("max_edge_rows");
            break;
        }
        let row_bytes = stored_context_edge_metadata_approx_bytes(&row);
        if stats.bytes_read.saturating_add(row_bytes) > budget.max_hydration_bytes {
            stats.omitted_edge_rows = stats.omitted_edge_rows.saturating_add(1);
            stats.note_budget_stop("max_hydration_bytes");
            break;
        }
        stats.bytes_read = stats.bytes_read.saturating_add(row_bytes);
        stats.loaded_edge_rows = stats.loaded_edge_rows.saturating_add(1);
        by_path.entry(row.path_id.clone()).or_default().push(row);
    }
    for rows in by_path.values_mut() {
        rows.sort_by_key(|row| row.ordinal);
    }
    for path in paths {
        let Some(rows) = by_path.get(&path.id) else {
            continue;
        };
        path.metadata
            .insert("hydrated_edge_row_count".to_string(), json!(rows.len()));
        path.metadata.insert(
            "ordered_edge_ids".to_string(),
            json!(rows
                .iter()
                .map(|row| row.edge_id.clone())
                .collect::<Vec<_>>()),
        );
        path.metadata.insert(
            "exactness_labels".to_string(),
            json!(rows
                .iter()
                .map(|row| {
                    row.exactness
                        .clone()
                        .unwrap_or_else(|| path.exactness.to_string())
                })
                .collect::<Vec<_>>()),
        );
        path.metadata.insert(
            "confidence_labels".to_string(),
            json!(rows
                .iter()
                .map(|row| row.confidence.unwrap_or(path.confidence))
                .collect::<Vec<_>>()),
        );
        path.metadata.insert(
            "production_test_mock_labels".to_string(),
            json!(rows
                .iter()
                .map(|row| stored_context_edge_role(row).role.as_str().to_string())
                .collect::<Vec<_>>()),
        );
        path.metadata.insert(
            "edge_labels".to_string(),
            json!(
                rows.iter()
                    .map(|row| {
                        let role = stored_context_edge_role(row);
                        json!({
                            "edge_id": row.edge_id.clone(),
                            "head_id": row.head_id.clone(),
                            "head_entity": stored_context_entity_agent_json(row.head_entity.as_ref()),
                            "relation": row.relation.clone(),
                            "tail_id": row.tail_id.clone(),
                            "tail_entity": stored_context_entity_agent_json(row.tail_entity.as_ref()),
                            "source_span": row.source_span_path.clone(),
                            "source_span_detail": stored_context_edge_source_span_json(row),
                            "exactness": row.exactness.clone().unwrap_or_else(|| path.exactness.to_string()),
                            "confidence": row.confidence.unwrap_or(path.confidence),
                            "derived": row.derived,
                            "edge_class": row.edge_class.clone(),
                            "fact_class": row.edge_class.clone(),
                            "context": row.context.clone().unwrap_or_else(|| "production".to_string()),
                            "evidence_role": role.role.as_str(),
                            "classification_reason": role.reason,
                            "classification_source": role.source,
                            "head_source_role": stored_context_entity_role(row.head_entity.as_ref()).role.as_str(),
                            "tail_source_role": stored_context_entity_role(row.tail_entity.as_ref()).role.as_str(),
                            "provenance_edges": row.provenance_edges.clone(),
                        })
                    })
                    .collect::<Vec<_>>()
            ),
        );
        let derived_expansion = rows
            .iter()
            .filter(|row| row.derived || !row.provenance_edges.is_empty())
            .map(|row| {
                json!({
                    "edge_id": row.edge_id.clone(),
                    "relation": row.relation.clone(),
                    "derived": row.derived,
                    "provenance_edges": row.provenance_edges.clone(),
                })
            })
            .collect::<Vec<_>>();
        path.metadata.insert(
            "derived_provenance_expansion".to_string(),
            json!(derived_expansion),
        );
        path.metadata
            .insert("source_spans".to_string(), json!(path.source_spans.clone()));
        path.metadata.insert(
            "metadata_storage".to_string(),
            json!("hydrated_materialized_rows"),
        );
        annotate_context_path_evidence_role(path);
    }
    Ok(stats)
}

pub(crate) fn stored_context_entity_agent_json(
    entity: Option<&StoredContextEntityMetadata>,
) -> Value {
    let Some(entity) = entity else {
        return Value::Null;
    };
    let display_name = if entity.name.trim().is_empty() {
        entity.id.as_str()
    } else {
        entity.name.as_str()
    };
    let role = stored_context_entity_role(Some(entity));
    let mut object = serde_json::Map::new();
    object.insert("entity_id".to_string(), json!(entity.id));
    object.insert("id".to_string(), json!(entity.id));
    object.insert("display_name".to_string(), json!(display_name));
    object.insert("name".to_string(), json!(entity.name));
    object.insert("symbol".to_string(), json!(entity.name));
    object.insert("qualified_name".to_string(), json!(entity.qualified_name));
    if let Some(kind) = entity.kind {
        object.insert("kind".to_string(), json!(kind.to_string()));
    }
    object.insert("file".to_string(), json!(entity.repo_relative_path));
    object.insert("path".to_string(), json!(entity.repo_relative_path));
    object.insert(
        "name_unavailable".to_string(),
        json!(entity.name.trim().is_empty()),
    );
    object.insert("evidence_role".to_string(), json!(role.role.as_str()));
    object.insert("classification_reason".to_string(), json!(role.reason));
    object.insert("classification_source".to_string(), json!(role.source));
    if let Some(span) = entity.source_span.as_ref() {
        object.insert("span".to_string(), agent_source_span_json(span));
        object.insert("source_span".to_string(), agent_source_span_json(span));
    }
    Value::Object(object)
}

pub(crate) fn stored_context_edge_source_span_json(row: &StoredContextPathEdgeMetadata) -> Value {
    let Some(path) = row.source_span_path.as_deref() else {
        return Value::Null;
    };
    json!({
        "file": path,
        "start_line": 1,
        "end_line": 1,
        "span_unavailable": true,
    })
}

pub(crate) fn load_bounded_context_edges(
    connection: &Connection,
    seed_ids: &[String],
    mode: &str,
    budgets: ContextPackBudgets,
) -> Result<Vec<Edge>, String> {
    if seed_ids.is_empty() {
        return Ok(Vec::new());
    }
    let relations = context_pack_allowed_relation_names(mode);
    let seed_placeholders = sql_placeholders(seed_ids.len());
    let relation_placeholders = sql_placeholders(relations.len());
    let limit = (budgets.max_candidate_paths * budgets.max_traversal_depth.max(1) * 4).max(16);
    let sql = format!(
        "
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
        WHERE (head.value IN ({seed_placeholders}) OR tail.value IN ({seed_placeholders}))
          AND relation.value IN ({relation_placeholders})
        ORDER BY e.confidence DESC, e.id_key
        LIMIT ?{}
        ",
        seed_ids.len() * 2 + relations.len() + 1
    );
    let mut params = Vec::<String>::new();
    params.extend(seed_ids.iter().cloned());
    params.extend(seed_ids.iter().cloned());
    params.extend(relations.iter().map(|relation| relation.to_string()));
    params.push(limit.to_string());
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            rusqlite::params_from_iter(params.iter()),
            edge_from_context_sql_row,
        )
        .map_err(|error| error.to_string())?;
    let mut edges = collect_sql_rows(rows)?;
    if mode.to_ascii_lowercase().contains("debug") {
        edges.extend(load_bounded_heuristic_context_edges(
            connection, seed_ids, &relations, limit,
        )?);
        edges.sort_by(|left, right| left.id.cmp(&right.id));
        edges.dedup_by(|left, right| left.id == right.id);
    }
    Ok(edges)
}

pub(crate) fn load_bounded_heuristic_context_edges(
    connection: &Connection,
    seed_ids: &[String],
    relations: &[&str],
    limit: usize,
) -> Result<Vec<Edge>, String> {
    if seed_ids.is_empty()
        || relations.is_empty()
        || !sqlite_table_exists(connection, "heuristic_edges")?
    {
        return Ok(Vec::new());
    }
    let seed_placeholders = sql_placeholders(seed_ids.len());
    let relation_placeholders = sql_placeholders(relations.len());
    let sql = format!(
        "
        SELECT edge_id AS id, head_id, relation, tail_id,
               source_span_path AS span_repo_relative_path,
               start_line, start_column, end_line, end_column,
               repo_commit, file_hash, extractor, confidence, exactness,
               edge_class, context, derived, provenance_edges_json, metadata_json
        FROM heuristic_edges
        WHERE (head_id IN ({seed_placeholders}) OR tail_id IN ({seed_placeholders}))
          AND relation IN ({relation_placeholders})
        ORDER BY confidence DESC, id_key
        LIMIT ?{}
        ",
        seed_ids.len() * 2 + relations.len() + 1
    );
    let mut params = Vec::<String>::new();
    params.extend(seed_ids.iter().cloned());
    params.extend(seed_ids.iter().cloned());
    params.extend(relations.iter().map(|relation| relation.to_string()));
    params.push(limit.to_string());
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            rusqlite::params_from_iter(params.iter()),
            edge_from_context_sql_row,
        )
        .map_err(|error| error.to_string())?;
    collect_sql_rows(rows)
}

pub(crate) fn filter_and_sort_context_path_evidence(
    paths: Vec<PathEvidence>,
    mode: &str,
    budgets: ContextPackBudgets,
) -> Vec<PathEvidence> {
    let mut seen = BTreeSet::new();
    let mut filtered = paths
        .into_iter()
        .flat_map(|path| context_path_evidence_candidates_for_mode(path, mode))
        .filter(|path| path.length <= budgets.max_traversal_depth)
        .filter(|path| context_path_evidence_allowed_for_mode(path, mode))
        .filter(|path| context_path_evidence_relation_allowed(path, mode))
        .filter(|path| seen.insert(path.id.clone()))
        .collect::<Vec<_>>();
    filtered.sort_by(|left, right| {
        right
            .confidence
            .partial_cmp(&left.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.length.cmp(&right.length))
            .then_with(|| left.id.cmp(&right.id))
    });
    filtered.truncate(budgets.max_returned_proof_paths);
    filtered
}

pub(crate) fn context_path_evidence_candidates_for_mode(
    mut path: PathEvidence,
    mode: &str,
) -> Vec<PathEvidence> {
    annotate_context_path_evidence_role(&mut path);
    if context_pack_mode_allows_test_mock(mode) {
        return vec![path];
    }
    if path.metadata.get("evidence_role").and_then(Value::as_str) == Some("mixed") {
        if let Some(split) = production_subpath_from_mixed_path_evidence(&path) {
            return vec![split, path];
        }
    }
    vec![path]
}

pub(crate) fn annotate_context_path_evidence_role(path: &mut PathEvidence) {
    let mut roles = context_path_edge_roles(path);
    if roles.is_empty() {
        roles = path
            .metapath
            .iter()
            .filter_map(|relation| match relation {
                RelationKind::Mocks | RelationKind::Stubs => Some(EvidenceRole::Mock),
                RelationKind::Tests
                | RelationKind::Asserts
                | RelationKind::Covers
                | RelationKind::FixturesFor => Some(EvidenceRole::Test),
                _ => None,
            })
            .collect();
    }
    let role = if roles.is_empty() {
        EvidenceRole::Unknown
    } else {
        combine_evidence_roles(roles.iter().copied())
    };
    let role_labels = if roles.is_empty() {
        vec![EvidenceRole::Unknown.as_str().to_string()]
    } else {
        roles
            .iter()
            .map(|role| role.as_str().to_string())
            .collect::<Vec<_>>()
    };
    let (classification_source, classification_reason) = context_path_classification_summary(path);
    path.metadata
        .insert("evidence_role".to_string(), json!(role.as_str()));
    path.metadata
        .insert("path_context".to_string(), json!(role.as_str()));
    path.metadata.insert(
        "production_test_mock_labels".to_string(),
        json!(role_labels),
    );
    path.metadata.insert(
        "classification_source".to_string(),
        json!(classification_source),
    );
    path.metadata.insert(
        "classification_reason".to_string(),
        json!(classification_reason),
    );
    let proof_grade_edges = path
        .metadata
        .get("proof_grade_edge_classes")
        .and_then(Value::as_bool)
        .unwrap_or(!matches!(
            path.exactness,
            Exactness::StaticHeuristic | Exactness::Inferred
        ));
    path.metadata.insert(
        "production_proof_eligible".to_string(),
        json!(role == EvidenceRole::Production && proof_grade_edges),
    );
    path.metadata.insert(
        "proof_scope".to_string(),
        json!(if role == EvidenceRole::Production {
            "production"
        } else if role == EvidenceRole::Unknown {
            "unknown"
        } else {
            "test_or_mock"
        }),
    );
}

pub(crate) fn context_path_edge_roles(path: &PathEvidence) -> Vec<EvidenceRole> {
    path.metadata
        .get("edge_labels")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|label| {
                    label
                        .get("evidence_role")
                        .or_else(|| label.get("source_role"))
                        .or_else(|| label.get("context"))
                        .and_then(Value::as_str)
                        .and_then(context_pack_role_from_label)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub(crate) fn context_path_classification_summary(path: &PathEvidence) -> (String, String) {
    let Some(labels) = path.metadata.get("edge_labels").and_then(Value::as_array) else {
        return (
            "fallback".to_string(),
            "missing edge source-role metadata".to_string(),
        );
    };
    let sources = labels
        .iter()
        .filter_map(|label| {
            label
                .get("classification_source")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect::<Vec<_>>();
    let reasons = labels
        .iter()
        .filter_map(|label| {
            label
                .get("classification_reason")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect::<Vec<_>>();
    (
        if sources.is_empty() {
            "fallback".to_string()
        } else {
            sources.join("+")
        },
        if reasons.is_empty() {
            "missing edge source-role metadata".to_string()
        } else {
            reasons.join("; ")
        },
    )
}

pub(crate) fn production_subpath_from_mixed_path_evidence(
    path: &PathEvidence,
) -> Option<PathEvidence> {
    let labels = path.metadata.get("edge_labels")?.as_array()?;
    let roles = labels
        .iter()
        .map(|label| {
            label
                .get("evidence_role")
                .or_else(|| label.get("source_role"))
                .or_else(|| label.get("context"))
                .and_then(Value::as_str)
                .and_then(context_pack_role_from_label)
                .unwrap_or(EvidenceRole::Unknown)
        })
        .collect::<Vec<_>>();
    let mut best_start = 0usize;
    let mut best_len = 0usize;
    let mut current_start = 0usize;
    let mut current_len = 0usize;
    for (index, role) in roles.iter().enumerate() {
        if *role == EvidenceRole::Production {
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
    if best_len == 0 || best_len == path.edges.len() {
        return None;
    }
    let end = best_start + best_len;
    let mut split = path.clone();
    split.id = format!("{}::production-subpath:{}-{}", path.id, best_start, end - 1);
    split.edges = path.edges[best_start..end].to_vec();
    split.metapath = path.metapath[best_start..end].to_vec();
    split.source_spans = path.source_spans[best_start..end].to_vec();
    split.length = split.edges.len();
    if let Some((head, _, _)) = split.edges.first() {
        split.source = head.clone();
    }
    if let Some((_, _, tail)) = split.edges.last() {
        split.target = tail.clone();
    }
    split.metadata.insert(
        "edge_labels".to_string(),
        json!(labels[best_start..end].to_vec()),
    );
    split.metadata.insert(
        "source_spans".to_string(),
        json!(split.source_spans.clone()),
    );
    split.metadata.insert(
        "evidence_role".to_string(),
        json!(EvidenceRole::Production.as_str()),
    );
    split.metadata.insert(
        "path_context".to_string(),
        json!(EvidenceRole::Production.as_str()),
    );
    split.metadata.insert(
        "production_test_mock_labels".to_string(),
        json!(vec![EvidenceRole::Production.as_str(); split.length]),
    );
    split
        .metadata
        .insert("classification_source".to_string(), json!("split"));
    split.metadata.insert(
        "classification_reason".to_string(),
        json!("mixed path split to production-only subpath"),
    );
    split
        .metadata
        .insert("split_from_path_id".to_string(), json!(path.id.clone()));
    split.metadata.insert(
        "production_proof_eligible".to_string(),
        json!(path
            .metadata
            .get("production_proof_eligible")
            .and_then(Value::as_bool)
            .unwrap_or(!matches!(
                split.exactness,
                Exactness::StaticHeuristic | Exactness::Inferred
            ))),
    );
    split
        .metadata
        .insert("proof_scope".to_string(), json!("production"));
    Some(split)
}

pub(crate) fn context_path_evidence_allowed_for_mode(path: &PathEvidence, mode: &str) -> bool {
    let normalized = mode.to_ascii_lowercase();
    if !normalized.contains("debug")
        && matches!(
            path.exactness,
            Exactness::StaticHeuristic | Exactness::Inferred
        )
    {
        return false;
    }
    if context_pack_mode_allows_test_mock(mode) {
        return true;
    }
    let evidence_role = path
        .metadata
        .get("evidence_role")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    evidence_role == "production"
}

pub(crate) fn context_pack_mode_allows_test_mock(mode: &str) -> bool {
    let normalized = mode.to_ascii_lowercase();
    normalized.contains("test") || normalized.contains("debug")
}

pub(crate) fn context_path_evidence_relation_allowed(path: &PathEvidence, mode: &str) -> bool {
    if mode.to_ascii_lowercase().contains("debug") {
        return true;
    }
    path.metapath
        .iter()
        .all(|relation| !context_pack_excluded_structural_relation(*relation))
}

pub(crate) fn context_pack_excluded_structural_relation(relation: RelationKind) -> bool {
    match relation {
        RelationKind::Contains | RelationKind::DefinedIn | RelationKind::Declares => true,
        _ => relation.to_string().starts_with("ARGUMENT_"),
    }
}

pub(crate) fn context_pack_allowed_relation_names(mode: &str) -> Vec<&'static str> {
    let mut relations = vec![
        "CALLS",
        "READS",
        "WRITES",
        "FLOWS_TO",
        "MUTATES",
        "MAY_MUTATE",
        "IMPORTS",
        "EXPORTS",
        "REEXPORTS",
        "ALIAS_OF",
        "ALIASED_BY",
        "AUTHORIZES",
        "CHECKS_ROLE",
        "CHECKS_PERMISSION",
        "SANITIZES",
        "EXPOSES",
        "PUBLISHES",
        "EMITS",
        "CONSUMES",
        "LISTENS_TO",
        "SUBSCRIBES_TO",
        "MIGRATES",
        "ALTERS_COLUMN",
        "DEPENDS_ON_SCHEMA",
        "READS_TABLE",
        "WRITES_TABLE",
    ];
    if context_pack_mode_allows_test_mock(mode) {
        relations.extend([
            "TESTS",
            "COVERS",
            "ASSERTS",
            "MOCKS",
            "STUBS",
            "FIXTURES_FOR",
        ]);
    }
    relations
}

#[derive(Debug, Clone)]
pub(crate) struct ContextPackFallbackEvidence {
    pub(crate) id: String,
    pub(crate) symbol: String,
    pub(crate) kind: String,
    pub(crate) source_span: SourceSpan,
    pub(crate) score: Option<f64>,
    pub(crate) evidence_role: EvidenceRole,
    pub(crate) evidence_role_label: Option<String>,
    pub(crate) proof_status: Option<String>,
    pub(crate) graph_proof: bool,
    pub(crate) claimability: Option<Value>,
    pub(crate) seed_matches: Vec<String>,
    pub(crate) follow_up_queries: Vec<String>,
    pub(crate) classification_reason: String,
    pub(crate) classification_source: String,
    pub(crate) fallback_source: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ContextPackTextEvidenceHit {
    pub(crate) kind: String,
    pub(crate) id: String,
    pub(crate) repo_relative_path: String,
    pub(crate) line: Option<u32>,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) score: f64,
    pub(crate) metadata: Metadata,
    pub(crate) seed_match: String,
}

pub(crate) const CONTEXT_PLANNING_ROLES: [&str; 9] = [
    "docs_authoring_guidance",
    "package_metadata",
    "kconfig_config_wiring",
    "makefile_inclusion",
    "download_infrastructure",
    "build_install_infrastructure",
    "support_scripts",
    "examples",
    "unknown",
];

pub(crate) fn context_planning_role_for_text_evidence(
    file: &str,
    symbol: &str,
    kind: &str,
    text: &str,
) -> &'static str {
    let normalized = context_planning_normalize_path(file);
    let lower_file = normalized.to_ascii_lowercase();
    let lower_text = format!("{symbol}\n{kind}\n{text}").to_ascii_lowercase();

    if lower_file.contains("docs/manual/adding-packages")
        || (lower_file.starts_with("docs/")
            && (lower_text.contains("generic-package")
                || lower_text.contains("package infrastructure")))
    {
        return "docs_authoring_guidance";
    }
    if lower_file == "package/config.in"
        || lower_file == "package/makefile.in"
        || lower_text.contains("source \"package/")
    {
        return "makefile_inclusion";
    }
    if lower_file.ends_with("config.in")
        || lower_file.contains("/config.in")
        || lower_text.contains("br2_package_")
        || lower_text.contains("depends on")
        || lower_text.contains("select ")
    {
        return "kconfig_config_wiring";
    }
    if lower_file.starts_with("support/scripts/") || lower_file.starts_with("support/download/") {
        return "support_scripts";
    }
    if lower_file == "package/pkg-download.mk"
        || lower_text.contains("download")
        || lower_text.contains("_site")
        || lower_text.contains("dl-wrapper")
    {
        return "download_infrastructure";
    }
    if lower_file == "package/pkg-generic.mk"
        || lower_text.contains("install_target")
        || lower_text.contains("install_staging")
    {
        return "build_install_infrastructure";
    }
    if lower_file.ends_with(".mk")
        && (lower_text.contains("_version")
            || lower_text.contains("_license")
            || lower_text.contains("_dependencies")
            || lower_text.contains("_site"))
    {
        return "package_metadata";
    }
    if lower_text.contains("generic-package") {
        return "build_install_infrastructure";
    }
    if lower_file.starts_with("package/")
        && (lower_file.ends_with(".mk") || lower_file.ends_with("config.in"))
    {
        return "examples";
    }
    "unknown"
}

pub(crate) fn context_planning_role_for_evidence(
    evidence: &ContextPackFallbackEvidence,
) -> &'static str {
    context_planning_role_for_text_evidence(
        &evidence.source_span.repo_relative_path,
        &evidence.symbol,
        &evidence.kind,
        &format!(
            "{}\n{}\n{}",
            evidence.classification_reason,
            evidence.seed_matches.join("\n"),
            evidence.follow_up_queries.join("\n")
        ),
    )
}

pub(crate) fn context_planning_role_for_value(value: &Value) -> &'static str {
    if let Some(role) = value.get("planning_role").and_then(Value::as_str) {
        if CONTEXT_PLANNING_ROLES.contains(&role) {
            return CONTEXT_PLANNING_ROLES
                .iter()
                .copied()
                .find(|candidate| *candidate == role)
                .unwrap_or("unknown");
        }
    }
    let file = context_planning_value_string(value, "file").unwrap_or_else(|| {
        value
            .get("source_span")
            .or_else(|| value.get("span"))
            .and_then(|span| context_planning_value_string(span, "file"))
            .unwrap_or_default()
    });
    let symbol = context_planning_value_string(value, "symbol").unwrap_or_default();
    let kind = context_planning_value_string(value, "kind").unwrap_or_default();
    let mut text = String::new();
    for key in [
        "text",
        "text_preview",
        "reason",
        "classification_reason",
        "fallback_source",
    ] {
        if let Some(value) = value.get(key).and_then(Value::as_str) {
            text.push_str(value);
            text.push('\n');
        }
    }
    context_planning_role_for_text_evidence(&file, &symbol, &kind, &text)
}

pub(crate) fn context_planning_role_rank(role: &str) -> usize {
    CONTEXT_PLANNING_ROLES
        .iter()
        .position(|candidate| *candidate == role)
        .unwrap_or(CONTEXT_PLANNING_ROLES.len())
}

pub(crate) fn context_planning_central_file_score(file: &str) -> usize {
    let lower = context_planning_normalize_path(file).to_ascii_lowercase();
    match lower.as_str() {
        "docs/manual/adding-packages-generic.adoc" => 0,
        "docs/manual/adding-packages.adoc" => 1,
        "package/pkg-generic.mk" => 2,
        "package/config.in" => 3,
        "package/pkg-download.mk" => 4,
        "support/download/dl-wrapper" => 5,
        _ if lower.starts_with("support/download/") => 6,
        _ if lower.starts_with("support/scripts/") => 7,
        _ if lower.starts_with("docs/manual/") => 8,
        _ if lower.starts_with("package/") && lower.ends_with(".mk") => 20,
        _ if lower.starts_with("package/") && lower.ends_with("config.in") => 21,
        _ => 30,
    }
}

pub(crate) fn context_pack_fallback_role_rank_reason(
    evidence: &ContextPackFallbackEvidence,
) -> String {
    let role = context_planning_role_for_evidence(evidence);
    let file = evidence.source_span.repo_relative_path.as_str();
    if context_planning_central_file_score(file) < 10 {
        format!("{role}: central Buildroot planning surface selected before package examples")
    } else if role == "examples" {
        "examples: package example evidence is useful but deduped after central surfaces"
            .to_string()
    } else {
        format!("{role}: selected by role-diverse text evidence ranking")
    }
}

pub(crate) fn context_pack_select_role_diverse_fallback_evidence(
    candidates: Vec<ContextPackFallbackEvidence>,
    limit: usize,
) -> Vec<ContextPackFallbackEvidence> {
    if candidates.len() <= limit {
        return candidates;
    }

    let mut indexed = candidates.into_iter().enumerate().collect::<Vec<_>>();
    indexed.sort_by(|(left_index, left), (right_index, right)| {
        let left_role = context_planning_role_for_evidence(left);
        let right_role = context_planning_role_for_evidence(right);
        context_planning_role_rank(left_role)
            .cmp(&context_planning_role_rank(right_role))
            .then_with(|| {
                context_planning_central_file_score(&left.source_span.repo_relative_path).cmp(
                    &context_planning_central_file_score(&right.source_span.repo_relative_path),
                )
            })
            .then_with(|| {
                left.score
                    .unwrap_or(f64::INFINITY)
                    .partial_cmp(&right.score.unwrap_or(f64::INFINITY))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left_index.cmp(right_index))
    });

    let mut selected = Vec::<(usize, ContextPackFallbackEvidence)>::new();
    let mut selected_keys = BTreeSet::new();
    let mut example_count = 0usize;

    for role in CONTEXT_PLANNING_ROLES {
        if selected.len() >= limit {
            break;
        }
        let Some(position) = indexed.iter().position(|(_, evidence)| {
            context_planning_role_for_evidence(evidence) == role
                && !selected_keys.contains(&context_span_key(&evidence.source_span))
                && (role != "examples" || example_count == 0)
        }) else {
            continue;
        };
        let (index, evidence) = indexed.remove(position);
        if role == "examples" {
            example_count += 1;
        }
        selected_keys.insert(context_span_key(&evidence.source_span));
        selected.push((index, evidence));
    }

    for (index, evidence) in indexed {
        if selected.len() >= limit {
            break;
        }
        let key = context_span_key(&evidence.source_span);
        if selected_keys.contains(&key) {
            continue;
        }
        let role = context_planning_role_for_evidence(&evidence);
        if role == "examples" && example_count >= 2 {
            continue;
        }
        if role == "examples" {
            example_count += 1;
        }
        selected_keys.insert(key);
        selected.push((index, evidence));
    }

    selected.into_iter().map(|(_, evidence)| evidence).collect()
}

pub(crate) fn context_pack_mode_needs_test_impact_fallback(mode: &str) -> bool {
    mode.to_ascii_lowercase().contains("test")
}

pub(crate) fn build_test_impact_fallback_evidence(
    connection: &Connection,
    mode: &str,
    seed_entities: &[ContextEntitySummary],
    local_edges: &[Edge],
    verified_paths: &[PathEvidence],
    budgets: ContextPackBudgets,
) -> Result<Vec<ContextPackFallbackEvidence>, String> {
    if !context_pack_mode_needs_test_impact_fallback(mode) {
        return Ok(Vec::new());
    }

    let mut endpoint_ids = BTreeSet::new();
    for edge in local_edges {
        endpoint_ids.insert(edge.head_id.clone());
        endpoint_ids.insert(edge.tail_id.clone());
    }
    let endpoint_entities = load_context_entities_by_ids(
        connection,
        endpoint_ids,
        budgets
            .max_seed_entities
            .saturating_add(local_edges.len().saturating_mul(2))
            .max(1),
    )?;

    let proof_span_keys = verified_paths
        .iter()
        .flat_map(|path| path.source_spans.iter())
        .map(context_span_key)
        .collect::<BTreeSet<_>>();
    let mut evidence = Vec::new();
    let mut seen = BTreeSet::new();
    let evidence_limit = budgets.max_snippets.saturating_mul(2).max(4);
    let same_file_entities = load_context_entities_by_paths(
        connection,
        seed_entities
            .iter()
            .map(|entity| entity.repo_relative_path.clone())
            .collect::<BTreeSet<_>>(),
        evidence_limit.saturating_mul(4),
    )?;
    let seed_has_test_or_mock = seed_entities.iter().any(|entity| {
        matches!(
            context_entity_role(entity).role,
            EvidenceRole::Test | EvidenceRole::Mock | EvidenceRole::Mixed
        )
    });

    for entity in seed_entities {
        let role = context_entity_fallback_role(entity);
        let include_production_seed =
            verified_paths.is_empty() && role.role == EvidenceRole::Production;
        if matches!(
            role.role,
            EvidenceRole::Test | EvidenceRole::Mock | EvidenceRole::Mixed
        ) || include_production_seed
        {
            push_entity_fallback_evidence(
                &mut evidence,
                &mut seen,
                &proof_span_keys,
                entity,
                role,
                "symbol/source_role/source_span",
                evidence_limit,
            );
        }
    }
    for entity in &same_file_entities {
        let role = context_entity_fallback_role(entity);
        if !context_fallback_entity_kind_allowed(entity.kind, role.role) {
            continue;
        }
        if matches!(
            role.role,
            EvidenceRole::Test | EvidenceRole::Mock | EvidenceRole::Mixed
        ) || (seed_has_test_or_mock && role.role == EvidenceRole::Production)
        {
            push_entity_fallback_evidence(
                &mut evidence,
                &mut seen,
                &proof_span_keys,
                entity,
                role,
                "same_file_source_role",
                evidence_limit,
            );
        }
        if evidence.len() >= evidence_limit {
            break;
        }
    }

    for edge in local_edges {
        let head = endpoint_entities.get(&edge.head_id);
        let tail = endpoint_entities.get(&edge.tail_id);
        let edge_role = classify_edge_evidence_role(edge);
        let head_role = head.map(context_entity_fallback_role);
        let tail_role = tail.map(context_entity_fallback_role);
        let combined_role = combine_context_fallback_roles(
            edge_role.role,
            head_role.as_ref().map(|role| role.role),
            tail_role.as_ref().map(|role| role.role),
        );
        let test_related = matches!(
            combined_role,
            EvidenceRole::Test | EvidenceRole::Mock | EvidenceRole::Mixed
        ) || matches!(edge_role.role, EvidenceRole::Test | EvidenceRole::Mock);

        if test_related {
            push_edge_fallback_evidence(
                &mut evidence,
                &mut seen,
                &proof_span_keys,
                edge,
                combined_role,
                fallback_edge_reason(&edge_role, head_role.as_ref(), tail_role.as_ref()),
                fallback_edge_source(&edge_role, head_role.as_ref(), tail_role.as_ref()),
                evidence_limit,
            );
            if let Some((entity, role)) = head.zip(head_role.as_ref()) {
                if matches!(
                    role.role,
                    EvidenceRole::Test | EvidenceRole::Mock | EvidenceRole::Mixed
                ) {
                    push_entity_fallback_evidence(
                        &mut evidence,
                        &mut seen,
                        &proof_span_keys,
                        entity,
                        role.clone(),
                        "local_relation_endpoint",
                        evidence_limit,
                    );
                }
            }
            if let Some((entity, role)) = tail.zip(tail_role.as_ref()) {
                if matches!(
                    role.role,
                    EvidenceRole::Test | EvidenceRole::Mock | EvidenceRole::Mixed
                ) || (head_role.as_ref().is_some_and(|head| {
                    matches!(
                        head.role,
                        EvidenceRole::Test | EvidenceRole::Mock | EvidenceRole::Mixed
                    )
                }) && role.role == EvidenceRole::Production)
                {
                    push_entity_fallback_evidence(
                        &mut evidence,
                        &mut seen,
                        &proof_span_keys,
                        entity,
                        role.clone(),
                        "local_relation_endpoint",
                        evidence_limit,
                    );
                }
            }
        }
        if evidence.len() >= evidence_limit {
            break;
        }
    }

    Ok(evidence)
}

pub(crate) fn context_pack_entity_allowed_for_symbol_output(
    entity: &ContextEntitySummary,
    mode: &str,
) -> bool {
    let normalized = mode.to_ascii_lowercase();
    if normalized.contains("test") || normalized.contains("debug") {
        return true;
    }
    context_entity_role(entity).role == EvidenceRole::Production
}

pub(crate) fn build_context_pack_text_evidence_fallback(
    connection: &Connection,
    options: &ContextPackOptions,
    raw_seed_values: &[String],
    budgets: ContextPackBudgets,
) -> Result<Vec<ContextPackFallbackEvidence>, String> {
    if !sqlite_table_exists(connection, "stage0_fts")?
        || !sqlite_table_exists(connection, "files")?
        || !sqlite_table_exists(connection, "path_dict")?
        || !sqlite_table_has_column(connection, "files", "metadata_json")?
    {
        return Ok(Vec::new());
    }

    let follow_up_queries = context_pack_text_fallback_queries(options, raw_seed_values);
    if follow_up_queries.is_empty() {
        return Ok(Vec::new());
    }

    let evidence_limit = budgets.max_snippets.saturating_mul(2).max(6);
    let candidate_limit = evidence_limit.saturating_mul(4).max(24);
    let per_query_limit = candidate_limit.max(8);
    let mut evidence = Vec::new();
    let mut seen = BTreeSet::new();
    let mut seen_files = BTreeSet::new();
    let mut deferred_hits = Vec::new();
    if context_pack_is_buildroot_package_task(&options.task) {
        for hit in load_context_pack_text_evidence_hits_for_paths(
            connection,
            &context_pack_buildroot_central_planning_files(),
        )? {
            if !context_pack_text_evidence_allowed_for_mode(
                &hit.repo_relative_path,
                &hit.metadata,
                &options.mode,
            ) {
                continue;
            }
            if seen_files.insert(hit.repo_relative_path.clone()) {
                push_context_pack_text_evidence_fallback(
                    &mut evidence,
                    &mut seen,
                    hit,
                    candidate_limit,
                );
            } else {
                deferred_hits.push(hit);
            }
        }
    }

    for query in &follow_up_queries {
        if evidence.len() >= candidate_limit {
            break;
        }
        let hits = load_context_pack_text_evidence_hits(connection, query, per_query_limit)?;
        for hit in hits {
            if !context_pack_text_evidence_allowed_for_mode(
                &hit.repo_relative_path,
                &hit.metadata,
                &options.mode,
            ) {
                continue;
            }
            if seen_files.insert(hit.repo_relative_path.clone()) {
                push_context_pack_text_evidence_fallback(
                    &mut evidence,
                    &mut seen,
                    hit,
                    candidate_limit,
                );
            } else {
                deferred_hits.push(hit);
            }
        }
    }
    for hit in deferred_hits {
        if evidence.len() >= candidate_limit {
            break;
        }
        push_context_pack_text_evidence_fallback(&mut evidence, &mut seen, hit, candidate_limit);
    }
    Ok(context_pack_select_role_diverse_fallback_evidence(
        evidence,
        evidence_limit,
    ))
}

pub(crate) fn build_context_pack_plan_atom_source_navigation_fallback(
    connection: &Connection,
    options: &ContextPackOptions,
    proof_span_keys: &BTreeSet<String>,
    budgets: ContextPackBudgets,
) -> Result<Vec<ContextPackFallbackEvidence>, String> {
    let (task_intent, _task_profile, retrieval_plan) = plan_task_retrieval(&options.task);
    if !context_pack_plan_atom_source_navigation_enabled(task_intent.task_kind.as_str()) {
        return Ok(Vec::new());
    }

    let evidence_limit = budgets.max_snippets.saturating_mul(2).max(6);
    let mut evidence = Vec::new();
    let mut seen = BTreeSet::new();
    for atom in &retrieval_plan.query_atoms {
        if evidence.len() >= evidence_limit {
            break;
        }
        let per_atom_limit = atom.max_candidates.clamp(1, 3);
        let entities = load_context_pack_plan_atom_entity_hits(
            connection,
            atom,
            &options.mode,
            per_atom_limit,
        )?;
        for entity in entities {
            if evidence.len() >= evidence_limit {
                break;
            }
            let role = context_entity_fallback_role(&entity);
            if !context_pack_plan_atom_entity_allowed(&atom.role, role.role, &options.mode) {
                continue;
            }
            push_plan_atom_source_navigation_evidence(
                &mut evidence,
                &mut seen,
                proof_span_keys,
                &entity,
                role,
                atom,
                evidence_limit,
            );
        }
    }

    Ok(evidence)
}

pub(crate) fn context_pack_plan_atom_source_navigation_enabled(task_kind: &str) -> bool {
    matches!(
        task_kind,
        "codegraph_internal_debug"
            | "implementation_trace"
            | "storage_accounting_trace"
            | "artifact_math_trace"
            | "persistence_path_trace"
            | "indexing_summary_trace"
            | "schema_view_trace"
            | "benchmark_metric_trace"
            | "test_impact"
            | "dataflow_trace"
            | "security_review"
    )
}

pub(crate) fn context_pack_plan_atom_entity_allowed(
    role: &str,
    evidence_role: EvidenceRole,
    mode: &str,
) -> bool {
    let role_lower = role.to_ascii_lowercase();
    if context_pack_mode_allows_test_mock(mode) || role_lower.contains("test") {
        return true;
    }
    !matches!(
        evidence_role,
        EvidenceRole::Test | EvidenceRole::Mock | EvidenceRole::Mixed
    )
}

pub(crate) fn load_context_pack_plan_atom_entity_hits(
    connection: &Connection,
    atom: &codegraph_query::RetrievalQueryAtom,
    mode: &str,
    limit: usize,
) -> Result<Vec<ContextEntitySummary>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let terms = context_pack_plan_atom_lookup_terms(atom);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let path_patterns = context_pack_plan_atom_path_patterns(atom);
    let mut entity_keys = BTreeSet::<i64>::new();
    let lookup_limit = limit.saturating_mul(12).max(12);
    let mut statement = connection
        .prepare(
            "
            SELECT e.id_key
            FROM entities e
            JOIN symbol_dict name ON name.id = e.name_id
            JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
            JOIN path_dict path ON path.id = e.path_id
            LEFT JOIN entity_kind_dict kind ON kind.id = e.kind_id
            WHERE (name.value LIKE ?1 ESCAPE '\\'
                   OR qname.value LIKE ?1 ESCAPE '\\'
                   OR path.value LIKE ?1 ESCAPE '\\')
              AND path.value LIKE ?2 ESCAPE '\\'
            ORDER BY
              CASE kind.value
                WHEN 'Function' THEN 0
                WHEN 'Method' THEN 1
                WHEN 'Struct' THEN 2
                WHEN 'Class' THEN 3
                WHEN 'Enum' THEN 4
                WHEN 'Trait' THEN 5
                WHEN 'Interface' THEN 6
                WHEN 'Module' THEN 7
                ELSE 9
              END,
              length(qname.value),
              qname.value
            LIMIT ?3
            ",
        )
        .map_err(|error| error.to_string())?;

    'outer: for pattern in &path_patterns {
        for term in &terms {
            if entity_keys.len() >= lookup_limit {
                break 'outer;
            }
            let term_pattern = context_pack_sql_like_pattern(term);
            let rows = statement
                .query_map(params![term_pattern, pattern, lookup_limit as i64], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(|error| error.to_string())?;
            for key in collect_sql_rows(rows)? {
                entity_keys.insert(key);
                if entity_keys.len() >= lookup_limit {
                    break 'outer;
                }
            }
        }
    }
    if entity_keys.is_empty() {
        return Ok(Vec::new());
    }

    let mut entities = load_context_entities_by_keys(
        connection,
        &entity_keys.into_iter().collect::<Vec<_>>(),
        lookup_limit,
    )?;
    entities.retain(|entity| {
        let role = context_entity_fallback_role(entity);
        context_pack_plan_atom_entity_allowed(&atom.role, role.role, mode)
            && entity.source_span.is_some()
            && context_pack_plan_atom_file_kind_matches(atom, &entity.repo_relative_path)
    });
    entities.sort_by(|left, right| {
        context_pack_plan_atom_entity_score(atom, right)
            .cmp(&context_pack_plan_atom_entity_score(atom, left))
            .then_with(|| left.repo_relative_path.cmp(&right.repo_relative_path))
            .then_with(|| left.qualified_name.cmp(&right.qualified_name))
    });
    entities.truncate(limit);
    Ok(entities)
}

pub(crate) fn context_pack_plan_atom_lookup_terms(
    atom: &codegraph_query::RetrievalQueryAtom,
) -> Vec<String> {
    let ignored = BTreeSet::from([
        "source",
        "text",
        "graph",
        "proof",
        "candidate",
        "candidates",
        "required",
        "span",
        "spans",
        "files",
        "file",
        "role",
        "roles",
        "trace",
        "find",
        "where",
        "helper",
    ]);
    let mut terms = Vec::new();
    for raw in format!("{} {}", atom.query_text, atom.expected_signal)
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
    {
        let lower = raw.trim().to_ascii_lowercase();
        if lower.len() < 4 || ignored.contains(lower.as_str()) {
            continue;
        }
        terms.push(lower.clone());
        if lower.contains('-') {
            terms.push(lower.replace('-', "_"));
            terms.extend(
                lower
                    .split('-')
                    .filter(|part| part.len() >= 4 && !ignored.contains(*part))
                    .map(str::to_string),
            );
        }
    }
    unique_limited_strings(terms, 10)
}

pub(crate) fn context_pack_plan_atom_path_patterns(
    atom: &codegraph_query::RetrievalQueryAtom,
) -> Vec<String> {
    if atom.path_hints.is_empty() {
        return vec!["%".to_string()];
    }
    unique_limited_strings(
        atom.path_hints
            .iter()
            .map(|hint| context_pack_plan_atom_path_pattern(hint)),
        8,
    )
}

pub(crate) fn context_pack_plan_atom_path_pattern(hint: &str) -> String {
    let normalized = hint.trim().replace('\\', "/");
    if normalized.is_empty() {
        return "%".to_string();
    }
    let mut escaped = String::new();
    for ch in normalized.chars() {
        match ch {
            '*' => escaped.push('%'),
            '%' => escaped.push_str("\\%"),
            '_' => escaped.push_str("\\_"),
            '\\' => escaped.push_str("\\\\"),
            other => escaped.push(other),
        }
    }
    if escaped.ends_with('%') {
        escaped
    } else if escaped.contains('.') && !escaped.ends_with('/') {
        format!("%{escaped}%")
    } else {
        format!("{escaped}%")
    }
}

pub(crate) fn context_pack_plan_atom_file_kind_matches(
    atom: &codegraph_query::RetrievalQueryAtom,
    file: &str,
) -> bool {
    if atom.file_kind_hints.is_empty() {
        return true;
    }
    let lower_file = file.to_ascii_lowercase();
    atom.file_kind_hints.iter().any(|hint| {
        let hint = hint.to_ascii_lowercase();
        if hint == ".rs" {
            lower_file.ends_with(".rs")
        } else if hint.starts_with('.') {
            lower_file.ends_with(&hint)
        } else {
            lower_file.contains(&hint)
        }
    })
}

pub(crate) fn context_pack_plan_atom_entity_score(
    atom: &codegraph_query::RetrievalQueryAtom,
    entity: &ContextEntitySummary,
) -> i64 {
    let terms = context_pack_plan_atom_lookup_terms(atom);
    let file = entity.repo_relative_path.to_ascii_lowercase();
    let name = entity.name.to_ascii_lowercase();
    let qualified_name = entity.qualified_name.to_ascii_lowercase();
    let mut score = 0i64;
    for term in &terms {
        if name.contains(term) {
            score += 50;
        }
        if qualified_name.contains(term) {
            score += 25;
        }
        if file.contains(term) {
            score += 10;
        }
    }
    if atom
        .path_hints
        .iter()
        .any(|hint| routing_path_hint_matches(&file, &hint.to_ascii_lowercase()))
    {
        score += 30;
    }
    score += match entity.kind {
        Some(EntityKind::Function | EntityKind::Method) => 20,
        Some(EntityKind::Class | EntityKind::Enum | EntityKind::Type) => 16,
        Some(EntityKind::Trait | EntityKind::Interface | EntityKind::Module) => 12,
        Some(EntityKind::TestCase | EntityKind::TestSuite) => 8,
        _ => 0,
    };
    score
}

pub(crate) fn push_plan_atom_source_navigation_evidence(
    evidence: &mut Vec<ContextPackFallbackEvidence>,
    seen: &mut BTreeSet<String>,
    proof_span_keys: &BTreeSet<String>,
    entity: &ContextEntitySummary,
    role: CliEvidenceRoleDecision,
    atom: &codegraph_query::RetrievalQueryAtom,
    limit: usize,
) {
    if evidence.len() >= limit {
        return;
    }
    let Some(span) = entity.source_span.clone() else {
        return;
    };
    let span_key = context_span_key(&span);
    let key = format!("plan-atom:{}:{span_key}", entity.id);
    if proof_span_keys.contains(&span_key) || !seen.insert(key) {
        return;
    }
    evidence.push(ContextPackFallbackEvidence {
        id: format!("plan-atom://{}:{}", atom.role, entity.id),
        symbol: entity.name.clone(),
        kind: entity
            .kind
            .map(|kind| kind.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        source_span: span,
        score: Some(context_pack_plan_atom_entity_score(atom, entity) as f64),
        evidence_role: role.role,
        evidence_role_label: Some("source_navigation".to_string()),
        proof_status: Some("no_proof_path_found".to_string()),
        graph_proof: false,
        claimability: None,
        seed_matches: vec![atom.query_text.clone()],
        follow_up_queries: vec![atom.query_text.clone()],
        classification_reason: format!(
            "matched retrieval plan atom `{}` for expected signal: {}",
            atom.role, atom.expected_signal
        ),
        classification_source: "retrieval_plan_atom/source_navigation".to_string(),
        fallback_source: "retrieval_plan_atom/source_navigation/no_proof_path_found".to_string(),
    });
}

pub(crate) fn push_context_pack_text_evidence_fallback(
    evidence: &mut Vec<ContextPackFallbackEvidence>,
    seen: &mut BTreeSet<String>,
    hit: ContextPackTextEvidenceHit,
    limit: usize,
) {
    if evidence.len() >= limit {
        return;
    }
    let line = context_pack_text_evidence_hit_line(&hit);
    let span = SourceSpan::new(&hit.repo_relative_path, line, line);
    let key = context_span_key(&span);
    if !seen.insert(key.clone()) {
        return;
    }
    let symbol = context_pack_text_evidence_symbol(&hit);
    evidence.push(ContextPackFallbackEvidence {
        id: format!("text-evidence://{}:{key}", hit.id),
        symbol,
        kind: hit
            .metadata
            .get("source_file_kind")
            .and_then(Value::as_str)
            .unwrap_or(&hit.kind)
            .to_string(),
        source_span: span,
        score: Some(hit.score),
        evidence_role: EvidenceRole::Unknown,
        evidence_role_label: Some("text_evidence".to_string()),
        proof_status: Some("no_proof_path_found".to_string()),
        graph_proof: false,
        claimability: Some(text_evidence_claimability_json()),
        seed_matches: vec![hit.seed_match.clone()],
        follow_up_queries: vec![hit.seed_match.clone()],
        classification_reason: format!(
            "matched bounded text evidence via stage0_fts; score={:.6}",
            hit.score
        ),
        classification_source: "stage0_fts/text_evidence".to_string(),
        fallback_source: "text_evidence/no_proof_path_found".to_string(),
    });
}

pub(crate) fn load_context_pack_text_evidence_hits(
    connection: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<ContextPackTextEvidenceHit>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let fts_query = context_pack_text_fallback_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(
            "
            SELECT stage0_fts.kind, stage0_fts.id, stage0_fts.repo_relative_path,
                   stage0_fts.line, stage0_fts.title, stage0_fts.body,
                   bm25(stage0_fts) AS rank, files.metadata_json
            FROM stage0_fts
            JOIN path_dict ON path_dict.value = stage0_fts.repo_relative_path
            JOIN files ON files.path_id = path_dict.id
            WHERE stage0_fts MATCH ?1
              AND stage0_fts.kind IN ('file', 'snippet')
            ORDER BY rank, stage0_fts.kind, stage0_fts.id
            LIMIT ?2
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![fts_query, limit as i64], |row| {
            let line = row
                .get::<_, Option<i64>>("line")?
                .and_then(|value| u32::try_from(value).ok());
            let metadata_json: String = row.get("metadata_json")?;
            Ok(ContextPackTextEvidenceHit {
                kind: row.get("kind")?,
                id: row.get("id")?,
                repo_relative_path: row.get("repo_relative_path")?,
                line,
                title: row.get("title")?,
                body: row.get("body")?,
                score: row.get("rank")?,
                metadata: serde_json::from_str::<Metadata>(&metadata_json)
                    .map_err(sql_json_error)?,
                seed_match: query.to_string(),
            })
        })
        .map_err(|error| error.to_string())?;
    let mut hits = collect_sql_rows(rows)?;
    hits.retain(|hit| {
        hit.metadata.get("evidence_kind").and_then(Value::as_str) == Some("text_evidence")
            && hit.metadata.get("proof_status").and_then(Value::as_str) == Some("not_graph_proof")
    });
    Ok(hits)
}

pub(crate) fn load_context_pack_text_evidence_hits_for_paths(
    connection: &Connection,
    paths: &[&str],
) -> Result<Vec<ContextPackTextEvidenceHit>, String> {
    let mut hits = Vec::new();
    let mut seen_paths = BTreeSet::new();
    let mut statement = connection
        .prepare(
            "
            SELECT stage0_fts.kind, stage0_fts.id, stage0_fts.repo_relative_path,
                   stage0_fts.line, stage0_fts.title, stage0_fts.body,
                   0.0 AS rank, files.metadata_json
            FROM stage0_fts
            JOIN path_dict ON path_dict.value = stage0_fts.repo_relative_path
            JOIN files ON files.path_id = path_dict.id
            WHERE stage0_fts.repo_relative_path = ?1
              AND stage0_fts.kind IN ('file', 'snippet')
            ORDER BY
              CASE stage0_fts.kind WHEN 'snippet' THEN 0 ELSE 1 END,
              COALESCE(stage0_fts.line, 0),
              stage0_fts.id
            LIMIT 1
            ",
        )
        .map_err(|error| error.to_string())?;
    for path in paths {
        let rows = statement
            .query_map(params![path], |row| {
                let line = row
                    .get::<_, Option<i64>>("line")?
                    .and_then(|value| u32::try_from(value).ok());
                let metadata_json: String = row.get("metadata_json")?;
                Ok(ContextPackTextEvidenceHit {
                    kind: row.get("kind")?,
                    id: row.get("id")?,
                    repo_relative_path: row.get("repo_relative_path")?,
                    line,
                    title: row.get("title")?,
                    body: row.get("body")?,
                    score: row.get("rank")?,
                    metadata: serde_json::from_str::<Metadata>(&metadata_json)
                        .map_err(sql_json_error)?,
                    seed_match: (*path).to_string(),
                })
            })
            .map_err(|error| error.to_string())?;
        for hit in collect_sql_rows(rows)? {
            if hit.metadata.get("evidence_kind").and_then(Value::as_str) == Some("text_evidence")
                && hit.metadata.get("proof_status").and_then(Value::as_str)
                    == Some("not_graph_proof")
                && seen_paths.insert(hit.repo_relative_path.clone())
            {
                hits.push(hit);
            }
        }
    }
    Ok(hits)
}

pub(crate) fn context_pack_text_fallback_queries(
    options: &ContextPackOptions,
    raw_seed_values: &[String],
) -> Vec<String> {
    let prompt_seeds = extract_prompt_seeds(&options.task);
    let prompt_values = prompt_seeds
        .iter()
        .filter(|seed| seed.kind.as_str() != "task_verb_ignored")
        .map(|seed| seed.value.clone());
    let default_queries = context_pack_default_text_fallback_queries(&options.task);
    unique_limited_strings(
        default_queries
            .into_iter()
            .chain(raw_seed_values.iter().cloned())
            .chain(options.seeds.iter().cloned())
            .chain(options.stage0_candidates.iter().cloned())
            .chain(prompt_values),
        options
            .limit_snippets
            .unwrap_or(DEFAULT_CONTEXT_AGENT_SNIPPET_LIMIT)
            .saturating_mul(4)
            .max(12),
    )
}

pub(crate) fn context_pack_default_text_fallback_queries(task: &str) -> Vec<String> {
    let mut queries = Vec::new();
    if context_pack_is_buildroot_package_task(task) {
        queries.extend([
            "adding-packages-generic".to_string(),
            "adding-packages".to_string(),
            "pkg-generic".to_string(),
            "pkg-download".to_string(),
            "dl-wrapper".to_string(),
            "generic-package".to_string(),
            "Config.in".to_string(),
            "depends on".to_string(),
            "select".to_string(),
            "package infrastructure".to_string(),
            ".mk".to_string(),
            ".adoc".to_string(),
            "support/scripts".to_string(),
            "support/download".to_string(),
        ]);
    }
    queries
}

pub(crate) fn context_pack_is_buildroot_package_task(task: &str) -> bool {
    let lower = task.to_ascii_lowercase();
    lower.contains("buildroot") && lower.contains("package")
}

pub(crate) fn context_pack_buildroot_central_planning_files() -> [&'static str; 5] {
    [
        "docs/manual/adding-packages-generic.adoc",
        "package/pkg-generic.mk",
        "package/Config.in",
        "package/pkg-download.mk",
        "support/download/dl-wrapper",
    ]
}

pub(crate) fn context_pack_text_fallback_fts_query(query: &str) -> String {
    query
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        .filter(|part| part.len() >= 2)
        .map(|part| format!("\"{}\"", part.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

pub(crate) fn context_pack_text_evidence_allowed_for_mode(
    repo_relative_path: &str,
    metadata: &Metadata,
    mode: &str,
) -> bool {
    if context_pack_mode_allows_test_mock(mode) {
        return true;
    }
    let source_role = metadata
        .get("source_role")
        .or_else(|| metadata.get("evidence_role"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if matches!(source_role, "test" | "mock" | "mixed") {
        return false;
    }
    !context_pack_test_path(repo_relative_path)
        && !context_pack_mock_name(repo_relative_path)
        && !context_pack_mock_name(
            Path::new(repo_relative_path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
        )
}

pub(crate) fn context_pack_text_evidence_hit_line(hit: &ContextPackTextEvidenceHit) -> u32 {
    hit.line
        .or_else(|| {
            query_snippet_from_text(&hit.body, &hit.seed_match)
                .and_then(|(line, _)| u32::try_from(line).ok())
        })
        .unwrap_or(1)
        .max(1)
}

pub(crate) fn context_pack_text_evidence_symbol(hit: &ContextPackTextEvidenceHit) -> String {
    hit.metadata
        .get("source_file_label")
        .and_then(Value::as_str)
        .filter(|label| !label.trim().is_empty())
        .map(str::to_string)
        .or_else(|| (!hit.title.trim().is_empty()).then(|| hit.title.clone()))
        .unwrap_or_else(|| hit.repo_relative_path.clone())
}

pub(crate) fn context_fallback_entity_kind_allowed(
    kind: Option<EntityKind>,
    role: EvidenceRole,
) -> bool {
    match role {
        EvidenceRole::Production => matches!(
            kind,
            Some(
                EntityKind::Class
                    | EntityKind::Interface
                    | EntityKind::Trait
                    | EntityKind::Enum
                    | EntityKind::Function
                    | EntityKind::Method
                    | EntityKind::Constructor
                    | EntityKind::Route
                    | EntityKind::Endpoint
                    | EntityKind::Middleware
            )
        ),
        EvidenceRole::Test | EvidenceRole::Mixed => matches!(
            kind,
            Some(
                EntityKind::Module
                    | EntityKind::Function
                    | EntityKind::Method
                    | EntityKind::TestSuite
                    | EntityKind::TestCase
                    | EntityKind::Fixture
                    | EntityKind::Assertion
            )
        ),
        EvidenceRole::Mock => matches!(
            kind,
            Some(
                EntityKind::Function
                    | EntityKind::Method
                    | EntityKind::Class
                    | EntityKind::Mock
                    | EntityKind::Stub
            )
        ),
        EvidenceRole::Unknown => false,
    }
}

pub(crate) fn context_entity_fallback_role(
    entity: &ContextEntitySummary,
) -> CliEvidenceRoleDecision {
    let role = context_entity_role(entity);
    if role.role != EvidenceRole::Unknown {
        return role;
    }
    if !context_pack_test_path(&entity.repo_relative_path)
        && !qualified_name_has_test_module(&entity.qualified_name)
        && !context_pack_mock_name(&entity.name)
        && !context_pack_mock_name(&entity.qualified_name)
    {
        return CliEvidenceRoleDecision::new(
            EvidenceRole::Production,
            "entity is outside detected test/mock context; fallback is non-proof",
            "source_role_fallback",
        );
    }
    role
}

pub(crate) fn combine_context_fallback_roles(
    edge_role: EvidenceRole,
    head_role: Option<EvidenceRole>,
    tail_role: Option<EvidenceRole>,
) -> EvidenceRole {
    let roles = [Some(edge_role), head_role, tail_role]
        .into_iter()
        .flatten()
        .filter(|role| *role != EvidenceRole::Unknown)
        .collect::<Vec<_>>();
    if roles.is_empty() {
        EvidenceRole::Unknown
    } else {
        combine_evidence_roles(roles)
    }
}

pub(crate) fn push_entity_fallback_evidence(
    evidence: &mut Vec<ContextPackFallbackEvidence>,
    seen: &mut BTreeSet<String>,
    proof_span_keys: &BTreeSet<String>,
    entity: &ContextEntitySummary,
    role: CliEvidenceRoleDecision,
    fallback_source: &str,
    limit: usize,
) {
    if evidence.len() >= limit {
        return;
    }
    let Some(span) = entity.source_span.clone() else {
        return;
    };
    let key = format!("entity:{}:{}", entity.id, context_span_key(&span));
    if proof_span_keys.contains(&context_span_key(&span)) || !seen.insert(key) {
        return;
    }
    evidence.push(ContextPackFallbackEvidence {
        id: entity.id.clone(),
        symbol: entity.name.clone(),
        kind: entity
            .kind
            .map(|kind| kind.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        source_span: span,
        score: None,
        evidence_role: role.role,
        evidence_role_label: None,
        proof_status: Some("no_proof_path_found".to_string()),
        graph_proof: false,
        claimability: None,
        seed_matches: Vec::new(),
        follow_up_queries: Vec::new(),
        classification_reason: role.reason,
        classification_source: role.source,
        fallback_source: fallback_source.to_string(),
    });
}

pub(crate) fn push_edge_fallback_evidence(
    evidence: &mut Vec<ContextPackFallbackEvidence>,
    seen: &mut BTreeSet<String>,
    proof_span_keys: &BTreeSet<String>,
    edge: &Edge,
    role: EvidenceRole,
    classification_reason: String,
    classification_source: String,
    limit: usize,
) {
    if evidence.len() >= limit {
        return;
    }
    let span = edge.source_span.clone();
    let key = format!("edge:{}:{}", edge.id, context_span_key(&span));
    if proof_span_keys.contains(&context_span_key(&span)) || !seen.insert(key) {
        return;
    }
    evidence.push(ContextPackFallbackEvidence {
        id: edge.id.clone(),
        symbol: format!("{} {} {}", edge.head_id, edge.relation, edge.tail_id),
        kind: "relation".to_string(),
        source_span: span,
        score: None,
        evidence_role: role,
        evidence_role_label: None,
        proof_status: Some("no_proof_path_found".to_string()),
        graph_proof: false,
        claimability: None,
        seed_matches: Vec::new(),
        follow_up_queries: Vec::new(),
        classification_reason,
        classification_source,
        fallback_source: "local_relation/source_span".to_string(),
    });
}

pub(crate) fn fallback_edge_reason(
    edge: &EvidenceRoleDecision,
    head: Option<&CliEvidenceRoleDecision>,
    tail: Option<&CliEvidenceRoleDecision>,
) -> String {
    let mut parts = vec![format!("edge: {}", edge.reason)];
    if let Some(head) = head {
        parts.push(format!("head: {}", head.reason));
    }
    if let Some(tail) = tail {
        parts.push(format!("tail: {}", tail.reason));
    }
    parts.join("; ")
}

pub(crate) fn fallback_edge_source(
    edge: &EvidenceRoleDecision,
    head: Option<&CliEvidenceRoleDecision>,
    tail: Option<&CliEvidenceRoleDecision>,
) -> String {
    let mut parts = vec![format!("edge:{}", edge.classification_source)];
    if let Some(head) = head {
        parts.push(format!("head:{}", head.source));
    }
    if let Some(tail) = tail {
        parts.push(format!("tail:{}", tail.source));
    }
    parts.join("+")
}

pub(crate) fn context_span_key(span: &SourceSpan) -> String {
    format!(
        "{}:{}:{}",
        span.repo_relative_path, span.start_line, span.end_line
    )
}

pub(crate) fn context_line_range_for_span(span: &SourceSpan) -> String {
    if span.start_line == span.end_line {
        span.start_line.to_string()
    } else {
        format!("{}-{}", span.start_line, span.end_line)
    }
}

pub(crate) fn fallback_evidence_json(evidence: &ContextPackFallbackEvidence) -> Value {
    let evidence_role = evidence
        .evidence_role_label
        .as_deref()
        .unwrap_or_else(|| evidence.evidence_role.as_str());
    let proof_status = evidence
        .proof_status
        .as_deref()
        .unwrap_or("no_proof_path_found");
    let planning_role = context_planning_role_for_evidence(evidence);
    let candidate_source = if evidence_role == "text_evidence" {
        "text_evidence"
    } else if evidence
        .fallback_source
        .contains("retrieval_plan_atom/source_navigation")
    {
        "source_navigation"
    } else {
        "no_proof_fallback"
    };
    let mut value = json!({
        "id": evidence.id,
        "symbol": evidence.symbol,
        "kind": evidence.kind,
        "file": evidence.source_span.repo_relative_path,
        "span": agent_source_span_json(&evidence.source_span),
        "source_span": agent_source_span_json(&evidence.source_span),
        "evidence_role": evidence_role,
        "proof_status": proof_status,
        "graph_proof": evidence.graph_proof,
        "candidate_source": candidate_source,
        "classification_reason": evidence.classification_reason,
        "classification_source": evidence.classification_source,
        "planning_role": planning_role,
        "role_rank_reason": context_pack_fallback_role_rank_reason(evidence),
        "fallback_source": evidence.fallback_source,
        "proof_path_available": false,
        "seed_matches": evidence.seed_matches.clone(),
        "follow_up_queries": evidence.follow_up_queries.clone(),
        "matched_seeds": evidence.seed_matches.clone(),
        "score": evidence.score,
        "claimable_for_text": evidence_role == "text_evidence",
        "claimable_for_graph": false,
        "language_capability": context_pack_source_language_capability_json(
            Some(&evidence.source_span.repo_relative_path),
            if evidence_role == "text_evidence" { "text_evidence" } else { "source_navigation_evidence" },
            proof_status,
            false,
            "source_text",
            evidence_role,
        ),
    });
    if let Some(claimability) = evidence.claimability.as_ref() {
        if let Some(object) = value.as_object_mut() {
            object.insert("claimability".to_string(), claimability.clone());
            object.insert("graph_relation_claims".to_string(), json!([]));
        }
    }
    value
}

#[derive(Debug, Clone)]
pub(crate) struct ContextAgentCandidateSet {
    pub(crate) candidates: Vec<Value>,
    pub(crate) total_count: usize,
    pub(crate) omitted_count: usize,
    pub(crate) omitted_candidates: Vec<Value>,
    pub(crate) exact_seed_cap_override: bool,
}

pub(crate) fn text_evidence_retrieval_candidate_json(
    evidence: &Value,
    rank: usize,
    snippets: &[Value],
    lifecycle_claimable: bool,
) -> Option<Value> {
    if evidence.get("evidence_role").and_then(Value::as_str) != Some("text_evidence") {
        return None;
    }
    let file = evidence
        .get("file")
        .and_then(Value::as_str)
        .or_else(|| evidence.pointer("/span/file").and_then(Value::as_str))?;
    let span = evidence
        .get("source_span")
        .cloned()
        .or_else(|| evidence.get("span").cloned())
        .unwrap_or_else(|| json!(null));
    let snippet_text = snippets
        .iter()
        .find(|snippet| snippet.get("file").and_then(Value::as_str) == Some(file))
        .and_then(|snippet| snippet.get("text").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    let (snippet, snippet_truncated) = bounded_retrieval_candidate_snippet_text(&snippet_text);
    let proof_status = evidence
        .get("proof_status")
        .and_then(Value::as_str)
        .unwrap_or("no_proof_path_found");
    Some(json!({
        "candidate_id": evidence.get("id").cloned().unwrap_or_else(|| json!(format!("text-evidence://{file}:{rank}"))),
        "candidate_source": "text_evidence",
        "candidate_sources": [
            "text_evidence",
            "lexical_fts"
        ],
        "candidate_source_label": "stage0_text_evidence",
        "file_id": file,
        "path": file,
        "entity_id": Value::Null,
        "span": span,
        "snippet": snippet,
        "evidence_role": "text_evidence",
        "proof_status": proof_status,
        "graph_proof": false,
        "claimable": lifecycle_claimable,
        "claimable_for_text": true,
        "claimable_for_graph": false,
        "diagnostic_only": false,
        "score": Value::Null,
        "source_score": evidence.get("score").cloned().unwrap_or(Value::Null),
        "rank": rank,
        "matched_seeds": evidence
            .get("matched_seeds")
            .or_else(|| evidence.get("seed_matches"))
            .cloned()
            .unwrap_or_else(|| json!([])),
        "requires_graph_verification": false,
        "verification_status": proof_status,
        "graph_verification_status": proof_status,
        "text_evidence_status": proof_status,
        "language_capability": evidence
            .get("language_capability")
            .cloned()
            .unwrap_or_else(|| context_pack_source_language_capability_json(
                Some(file),
                "text_evidence",
                proof_status,
                false,
                "source_text",
                "text_evidence",
            )),
        "reason": evidence
            .get("classification_reason")
            .and_then(Value::as_str)
            .unwrap_or("text evidence candidate; not graph proof"),
        "omitted": false,
        "truncated": snippet_truncated,
        "ranking_features": {
            "exact_seed_match": false,
            "file_path_match": false,
            "text_evidence_match": true,
            "symbol_entity_match": false,
            "graph_proximity": false,
            "source_role_compatible": true,
            "lifecycle_claimable": lifecycle_claimable,
            "proof_available": false,
            "vector_score_available": false,
            "binary_score_available": false,
            "rescue_match": false
        },
        "source_labels": [
            "stage0_fts",
            "text_evidence",
            "no_graph_proof"
        ]
    }))
}

pub(crate) fn context_pack_vector_retrieval_candidate_json(
    candidate: &RetrievalCandidate,
    lifecycle_claimable: bool,
) -> Value {
    let candidate_source =
        context_pack_retrieval_candidate_source_label(candidate.candidate_source);
    let mut sources = BTreeSet::from([candidate_source.to_string()]);
    if matches!(
        candidate.candidate_source,
        RetrievalCandidateSource::VectorSemantic | RetrievalCandidateSource::VectorRerank
    ) {
        sources.insert("vector_semantic".to_string());
    }
    if candidate.candidate_source == RetrievalCandidateSource::VectorBinary {
        sources.insert("binary_vector".to_string());
    }
    if candidate.entity_id.is_some() {
        sources.insert("symbol_lookup".to_string());
    }
    let text_evidence_match = matches!(
        candidate.embedding_source,
        Some(VectorEmbeddingSource::TextEvidence)
            | Some(VectorEmbeddingSource::Snippet)
            | Some(VectorEmbeddingSource::FilePathTitle)
    ) || candidate.evidence_role == EvidenceRole::Unknown
        && candidate.entity_id.is_none()
        && candidate.path.is_some();
    if text_evidence_match {
        sources.insert("text_evidence".to_string());
    }
    let proof_status = context_pack_retrieval_proof_status_label(candidate.proof_status);
    let verification_status =
        context_pack_retrieval_verification_status_label(candidate.verification_status);
    let evidence_role = if text_evidence_match {
        "text_evidence"
    } else {
        candidate.evidence_role.as_str()
    };
    let source_labels = sources
        .iter()
        .cloned()
        .chain(["candidate_only".to_string(), "no_graph_proof".to_string()])
        .collect::<BTreeSet<_>>();
    let vector_semantic_candidate = sources.contains("vector_semantic");
    let binary_vector_candidate = sources.contains("binary_vector");
    let ranking_features = json!({
        "exact_seed_match": false,
        "file_path_match": matches!(candidate.embedding_source, Some(VectorEmbeddingSource::FilePathTitle)),
        "text_evidence_match": text_evidence_match,
        "symbol_entity_match": candidate.entity_id.is_some(),
        "graph_proximity": false,
        "source_role_compatible": matches!(candidate.evidence_role, EvidenceRole::Production | EvidenceRole::Test | EvidenceRole::Unknown),
        "lifecycle_claimable": lifecycle_claimable,
        "proof_available": false,
        "vector_score_available": sources.contains("vector_semantic") && candidate.score.is_some(),
        "binary_score_available": sources.contains("binary_vector") && candidate.score.is_some(),
        "rescue_match": false
    });
    json!({
        "candidate_id": candidate.candidate_id.clone(),
        "candidate_source": candidate_source,
        "candidate_sources": sources.iter().cloned().collect::<Vec<_>>(),
        "candidate_source_label": candidate_source,
        "file_id": candidate.file_id.clone(),
        "path": candidate.path.clone(),
        "entity_id": candidate.entity_id.clone(),
        "span": candidate.span.as_ref().map(agent_source_span_json).unwrap_or(Value::Null),
        "snippet": Value::Null,
        "evidence_role": evidence_role,
        "proof_status": proof_status,
        "graph_proof": false,
        "claimable": false,
        "claimable_for_text": candidate.claimable_for_text.unwrap_or(false) && lifecycle_claimable,
        "claimable_for_graph": false,
        "diagnostic_only": candidate.diagnostic_only,
        "score": Value::Null,
        "source_score": candidate.score,
        "vector_score": if sources.contains("vector_semantic") { candidate.score } else { None },
        "binary_score": if sources.contains("binary_vector") { candidate.score } else { None },
        "semantic_backend": if vector_semantic_candidate { json!("deterministic_token_projection") } else { Value::Null },
        "semantic_provider_kind": if vector_semantic_candidate { json!("deterministic_token_projection") } else { Value::Null },
        "learned_semantic_embeddings": if vector_semantic_candidate { json!(false) } else { Value::Null },
        "semantic_embedding_claim": if vector_semantic_candidate { json!("not_learned_semantic_embedding") } else { Value::Null },
        "vector_candidate_claim_boundary": if vector_semantic_candidate || binary_vector_candidate {
            json!("vector_evidence_candidate_only; graph_proof_requires_graph_source_verification")
        } else {
            Value::Null
        },
        "rank": candidate.rank,
        "matched_seeds": candidate.matched_seeds.clone(),
        "matched_token": candidate.matched_seeds.first().cloned(),
        "matched_tokens": candidate.matched_seeds.clone(),
        "requires_graph_verification": candidate.requires_graph_verification,
        "verification_status": verification_status,
        "graph_verification_status": verification_status,
        "text_evidence_status": if text_evidence_match { proof_status } else { "absent" },
        "reason": candidate.reason.clone(),
        "omitted": candidate.omitted,
        "truncated": candidate.truncated,
        "ranking_features": ranking_features,
        "source_labels": source_labels.iter().cloned().collect::<Vec<_>>()
    })
}

pub(crate) fn context_pack_retrieval_candidate_source_label(
    source: RetrievalCandidateSource,
) -> &'static str {
    match source {
        RetrievalCandidateSource::ExactSeed => "exact_seed",
        RetrievalCandidateSource::FilePathSeed => "file_path_seed",
        RetrievalCandidateSource::TextEvidence => "text_evidence",
        RetrievalCandidateSource::LexicalFts => "lexical_fts",
        RetrievalCandidateSource::SymbolLookup => "symbol_lookup",
        RetrievalCandidateSource::VectorBinary => "binary_vector",
        RetrievalCandidateSource::VectorRerank => "vector_rerank",
        RetrievalCandidateSource::VectorSemantic => "vector_semantic",
        RetrievalCandidateSource::NuanceRescue => "nuance_rescue",
        RetrievalCandidateSource::GraphNeighbor => "graph_neighbor",
        RetrievalCandidateSource::PathEvidence => "path_evidence",
        RetrievalCandidateSource::NoProofFallback => "no_proof_fallback",
        RetrievalCandidateSource::Diagnostic => "diagnostic",
        RetrievalCandidateSource::Unknown => "unknown",
    }
}

pub(crate) fn context_pack_retrieval_proof_status_label(
    status: RetrievalProofStatus,
) -> &'static str {
    match status {
        RetrievalProofStatus::ProofPathFound => "proof_path_found",
        RetrievalProofStatus::NoProofPathFound => "no_proof_path_found",
        RetrievalProofStatus::NotGraphProof => "not_graph_proof",
        RetrievalProofStatus::CandidateOnly => "candidate_only",
        RetrievalProofStatus::DiagnosticOnly => "diagnostic_only",
        RetrievalProofStatus::StaleOrForeignDb => "stale_or_foreign_db",
        RetrievalProofStatus::Unknown => "unknown",
    }
}

pub(crate) fn context_pack_retrieval_verification_status_label(
    status: RetrievalVerificationStatus,
) -> &'static str {
    match status {
        RetrievalVerificationStatus::Unverified => "unverified",
        RetrievalVerificationStatus::NeedsGraphVerification => "needs_graph_verification",
        RetrievalVerificationStatus::GraphVerified => "graph_verified",
        RetrievalVerificationStatus::NoProofPathFound => "no_proof_path_found",
        RetrievalVerificationStatus::NotGraphProof => "not_graph_proof",
        RetrievalVerificationStatus::CandidateOnly => "candidate_only",
        RetrievalVerificationStatus::DiagnosticOnly => "diagnostic_only",
        RetrievalVerificationStatus::StaleOrForeignDb => "stale_or_foreign_db",
        RetrievalVerificationStatus::Omitted => "omitted",
        RetrievalVerificationStatus::Truncated => "truncated",
        RetrievalVerificationStatus::Unknown => "unknown",
    }
}

pub(crate) fn build_context_agent_retrieval_candidates(
    options: &ContextPackOptions,
    packet: &ContextPacket,
    paths: &[Value],
    fallback_evidence: &[Value],
    snippets: &[Value],
    lifecycle_claimable: bool,
    proof_path_available: bool,
) -> ContextAgentCandidateSet {
    let graph_verification_status = packet
        .metadata
        .get("graph_verification_status")
        .and_then(Value::as_str)
        .unwrap_or(if proof_path_available {
            "graph_verified"
        } else {
            "unknown"
        });
    let proof_gate = ContextAgentProofGate::new(graph_verification_status, lifecycle_claimable);
    let exact_seeds = context_agent_candidate_seed_values(options);
    let exact_path_keys = exact_seeds
        .iter()
        .filter(|seed| context_agent_seed_is_path(seed))
        .map(|seed| context_agent_path_key(seed))
        .collect::<BTreeSet<_>>();
    let exact_seed_keys = exact_seeds
        .iter()
        .map(|seed| context_agent_seed_key(seed))
        .collect::<BTreeSet<_>>();
    let mut candidates = Vec::new();

    for seed in &exact_seeds {
        candidates.push(exact_seed_retrieval_candidate_json(
            seed,
            lifecycle_claimable,
            graph_verification_status,
        ));
    }
    for path in paths {
        candidates.push(path_evidence_retrieval_candidate_json(
            path,
            &exact_seeds,
            lifecycle_claimable,
            proof_path_available,
        ));
    }
    for (index, evidence) in fallback_evidence.iter().enumerate() {
        if let Some(candidate) = text_evidence_retrieval_candidate_json(
            evidence,
            index.saturating_add(1),
            snippets,
            lifecycle_claimable,
        ) {
            candidates.push(candidate);
        } else {
            candidates.push(no_proof_fallback_retrieval_candidate_json(
                evidence,
                index.saturating_add(1),
                lifecycle_claimable,
            ));
        }
    }
    for key in ["vector_semantic_candidates", "binary_vector_candidates"] {
        if let Some(packet_candidates) = packet.metadata.get(key).and_then(Value::as_array) {
            for candidate in packet_candidates
                .iter()
                .take(CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT.saturating_mul(4))
            {
                let mut candidate = candidate.clone();
                proof_gate.apply_to_candidate(&mut candidate);
                candidates.push(candidate);
            }
        }
    }
    if let Some(nuance_candidates) = packet
        .metadata
        .get("nuance_rescue_candidates")
        .and_then(Value::as_array)
    {
        for candidate in nuance_candidates
            .iter()
            .take(CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT)
        {
            let mut candidate = candidate.clone();
            proof_gate.apply_to_candidate(&mut candidate);
            candidates.push(candidate);
        }
    }

    let mut deduped = BTreeMap::<String, Value>::new();
    for candidate in candidates {
        let key = context_agent_candidate_dedup_key(&candidate, &exact_path_keys, &exact_seed_keys);
        if let Some(existing) = deduped.get_mut(&key) {
            merge_context_agent_candidate(existing, candidate);
        } else {
            deduped.insert(key, candidate);
        }
    }

    let mut ranked_candidates = deduped.into_values().collect::<Vec<_>>();
    ranked_candidates.sort_by(|left, right| {
        context_agent_candidate_rank_score(right)
            .cmp(&context_agent_candidate_rank_score(left))
            .then_with(|| context_agent_candidate_id(left).cmp(&context_agent_candidate_id(right)))
    });

    let total_count = ranked_candidates.len();
    let ranked_candidates_for_omission = ranked_candidates.clone();
    let protected = ranked_candidates
        .iter()
        .filter(|candidate| context_agent_candidate_has_source(candidate, "exact_seed"))
        .cloned()
        .collect::<Vec<_>>();
    let mut selected = protected;
    let mut selected_keys = selected
        .iter()
        .map(context_agent_candidate_id)
        .collect::<BTreeSet<_>>();
    let mut protected_text_count = selected
        .iter()
        .filter(|candidate| context_agent_candidate_has_source(candidate, "text_evidence"))
        .count();
    for candidate in ranked_candidates
        .iter()
        .filter(|candidate| context_agent_candidate_has_source(candidate, "text_evidence"))
    {
        if protected_text_count >= CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT {
            break;
        }
        let candidate_id = context_agent_candidate_id(candidate);
        if selected_keys.insert(candidate_id) {
            selected.push(candidate.clone());
            protected_text_count += 1;
        }
    }
    for candidate in ranked_candidates {
        if selected.len() >= CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT {
            break;
        }
        if !selected_keys.insert(context_agent_candidate_id(&candidate)) {
            continue;
        }
        selected.push(candidate);
    }
    let omitted_count = total_count.saturating_sub(selected.len());
    let selected_ids = selected
        .iter()
        .map(context_agent_candidate_id)
        .collect::<BTreeSet<_>>();
    let omitted_candidates = ranked_candidates_for_omission
        .iter()
        .filter(|candidate| !selected_ids.contains(&context_agent_candidate_id(candidate)))
        .take(12)
        .map(context_agent_candidate_omission_json)
        .collect::<Vec<_>>();
    let exact_seed_cap_override = selected.len() > CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT;
    for (index, candidate) in selected.iter_mut().enumerate() {
        enforce_context_agent_candidate_vector_truth(candidate);
        let rank = index.saturating_add(1);
        let score = context_agent_candidate_rank_score(candidate);
        if let Some(object) = candidate.as_object_mut() {
            object.insert("rank".to_string(), json!(rank));
            object.insert("score".to_string(), json!(score as f64));
            object.insert(
                "candidate_cap".to_string(),
                json!(CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT),
            );
            object.insert(
                "candidate_cap_policy".to_string(),
                json!(
                    "exact_seeds_protected_then_no_proof_text_then_nuance_and_deterministic_rank"
                ),
            );
        }
    }

    let _ = packet;
    ContextAgentCandidateSet {
        candidates: selected,
        total_count,
        omitted_count,
        omitted_candidates,
        exact_seed_cap_override,
    }
}

pub(crate) fn annotate_context_agent_candidates_with_file_degradation(
    candidates: &mut [Value],
    db_path: &Path,
) {
    if candidates.is_empty() {
        return;
    }
    let Ok(store) = SqliteGraphStore::open_read_only(db_path) else {
        return;
    };
    let mut cache = BTreeMap::<String, Option<FileRecord>>::new();
    for candidate in candidates {
        let Some(path) = context_agent_candidate_file_path(candidate) else {
            continue;
        };
        let key = context_agent_path_key(&path);
        let file = if let Some(file) = cache.get(&key) {
            file.clone()
        } else {
            let loaded = store.get_file(&path).ok().flatten();
            cache.insert(key, loaded.clone());
            loaded
        };
        if let Some(file) = file {
            insert_context_candidate_degradation_from_file(&file, candidate);
        }
    }
}

pub(crate) fn context_agent_candidate_file_path(candidate: &Value) -> Option<String> {
    candidate
        .get("path")
        .or_else(|| candidate.get("file_id"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .map(|path| path.replace('\\', "/"))
}

pub(crate) fn insert_context_candidate_degradation_from_file(
    file: &FileRecord,
    candidate: &mut Value,
) {
    let source_labels = context_agent_string_set(candidate, "source_labels");
    let Some(object) = candidate.as_object_mut() else {
        return;
    };
    let metadata = &file.metadata;
    let graph_output_budget_hit = metadata
        .get("graph_output_budget_hit")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let degradation_labels = metadata
        .get("degradation_labels")
        .or_else(|| metadata.get("graph_output_degradation_labels"))
        .cloned();
    let diagnostic_only = metadata
        .get("diagnostic_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !graph_output_budget_hit && degradation_labels.is_none() && !diagnostic_only {
        return;
    }

    object.insert("graph_output_degraded".to_string(), json!(true));
    object.insert(
        "degradation_labels".to_string(),
        degradation_labels.unwrap_or_else(|| json!([])),
    );
    object.insert(
        "graph_output_claimability".to_string(),
        metadata
            .get("graph_output_claimability")
            .cloned()
            .unwrap_or_else(|| json!("degraded_file_nonclaimable_for_omitted_facts")),
    );
    object.insert(
        "graph_relation_claims".to_string(),
        metadata
            .get("graph_relation_claims")
            .cloned()
            .unwrap_or_else(|| json!("partial")),
    );
    object.insert(
        "graph_output_budget_hits".to_string(),
        metadata
            .get("graph_output_budget_hits")
            .cloned()
            .unwrap_or_else(|| json!([])),
    );
    if let Some(reason) = metadata.get("graph_extraction_skip_reason").cloned() {
        object.insert("graph_extraction_skip_reason".to_string(), reason);
    }
    object.insert("diagnostic_only".to_string(), json!(diagnostic_only));
    object.insert(
        "degraded_output_not_complete_graph_proof".to_string(),
        json!(true),
    );
    object.insert(
        "claimability".to_string(),
        json!({
            "claimable": false,
            "claimable_as": [],
            "not_claimable_as": [
                "complete_graph_for_degraded_file",
                "omitted_relation_classes",
                "graph_relation_proof"
            ],
            "diagnostic_only": diagnostic_only,
            "reason": "file has degraded graph output; context candidates must not be treated as complete graph proof"
        }),
    );
    let mut merged_labels = source_labels.into_iter().collect::<Vec<_>>();
    if !merged_labels
        .iter()
        .any(|label| label == "degraded_graph_output")
    {
        merged_labels.push("degraded_graph_output".to_string());
    }
    object.insert("source_labels".to_string(), json!(merged_labels));
}

pub(crate) fn copy_context_candidate_degradation_fields(
    from: &Value,
    object: &mut serde_json::Map<String, Value>,
) {
    for key in [
        "graph_output_degraded",
        "degradation_labels",
        "graph_output_claimability",
        "graph_output_budget_hits",
        "graph_relation_claims",
        "graph_extraction_skip_reason",
        "diagnostic_only",
        "degraded_output_not_complete_graph_proof",
        "claimability",
        "language_capability",
    ] {
        if let Some(value) = from.get(key).cloned() {
            object.insert(key.to_string(), value);
        }
    }
}

/// Applies the context-pack claim boundary after candidate recall branches have
/// suggested files or symbols but before agent JSON can present them as proof.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextAgentProofGate<'a> {
    graph_verification_status: &'a str,
    lifecycle_claimable: bool,
}

impl<'a> ContextAgentProofGate<'a> {
    const fn new(graph_verification_status: &'a str, lifecycle_claimable: bool) -> Self {
        Self {
            graph_verification_status,
            lifecycle_claimable,
        }
    }

    fn apply_to_candidate(self, candidate: &mut Value) {
        let graph_claim_blocked = self.blocks_graph_claim(candidate);
        let existing_source_labels = context_agent_string_set(candidate, "source_labels");

        let Some(object) = candidate.as_object_mut() else {
            return;
        };
        if !self.lifecycle_claimable {
            object.insert("claimable".to_string(), json!(false));
            object.insert("claimable_for_graph".to_string(), json!(false));
        }
        if graph_claim_blocked {
            object.insert("proof_status".to_string(), json!("no_proof_path_found"));
            object.insert(
                "verification_status".to_string(),
                json!("no_proof_path_found"),
            );
            object.insert(
                "graph_verification_status".to_string(),
                json!("no_proof_path_found"),
            );
            object.insert("graph_proof".to_string(), json!(false));
            object.insert("claimable".to_string(), json!(false));
            object.insert("claimable_for_graph".to_string(), json!(false));
            let reason = object
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("candidate requires graph verification")
                .to_string();
            if !reason.contains("no proof path found") {
                object.insert(
                    "reason".to_string(),
                    json!(format!("{reason}; graph verification found no proof path")),
                );
            }
            let mut source_labels = existing_source_labels;
            source_labels.insert("no_graph_proof".to_string());
            object.insert(
                "source_labels".to_string(),
                json!(source_labels.iter().cloned().collect::<Vec<_>>()),
            );
        }
    }

    fn blocks_graph_claim(self, candidate: &Value) -> bool {
        let requires_graph_verification = candidate
            .get("requires_graph_verification")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let graph_proof = candidate
            .get("graph_proof")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        requires_graph_verification
            && !graph_proof
            && !context_agent_candidate_is_text_only_source_evidence(candidate)
            && matches!(
                self.graph_verification_status,
                "no_proof_path_found" | "no_graph_candidates"
            )
    }
}

pub(crate) fn context_agent_candidate_is_text_only_source_evidence(candidate: &Value) -> bool {
    context_agent_candidate_has_source(candidate, "text_evidence")
        && candidate.get("entity_id").is_none_or(Value::is_null)
        && !context_agent_candidate_has_source(candidate, "path_evidence")
        && !context_agent_candidate_has_source(candidate, "graph_neighbor")
}

pub(crate) fn context_agent_candidate_omission_json(candidate: &Value) -> Value {
    let candidate = context_pack_public_candidate_json(candidate);
    json!({
        "candidate_id": candidate.get("candidate_id").cloned().unwrap_or(Value::Null),
        "candidate_sources": candidate.get("candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "reason": candidate.get("reason").cloned().unwrap_or(Value::Null),
        "verification_status": candidate.get("verification_status").cloned().unwrap_or(Value::Null),
        "proof_status": candidate.get("proof_status").cloned().unwrap_or(Value::Null),
        "graph_proof": candidate.get("graph_proof").cloned().unwrap_or(Value::Null),
        "omission_reason": "candidate_cap_exceeded_after_exact_seed_text_priority"
    })
}

pub(crate) fn context_agent_candidate_seed_values(options: &ContextPackOptions) -> Vec<String> {
    let prompt_exact = context_pack_prompt_exact_seed_values(&options.task);
    unique_limited_strings(
        options
            .seeds
            .iter()
            .chain(options.stage0_candidates.iter())
            .chain(prompt_exact.iter())
            .cloned(),
        CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT
            .saturating_mul(8)
            .max(16),
    )
}

pub(crate) fn context_agent_seed_key(seed: &str) -> String {
    seed.trim().replace('\\', "/").to_ascii_lowercase()
}

pub(crate) fn exact_seed_retrieval_candidate_json(
    seed: &str,
    lifecycle_claimable: bool,
    graph_verification_status: &str,
) -> Value {
    let is_path = context_agent_seed_is_path(seed);
    let is_symbol = context_agent_seed_is_symbol(seed);
    let graph_verification_failed = graph_verification_status == "no_proof_path_found";
    let mut sources = vec!["exact_seed"];
    if is_path {
        sources.push("file_path_seed");
    }
    if is_symbol {
        sources.push("symbol_lookup");
    }
    json!({
        "candidate_id": format!("exact-seed://{}", context_agent_stable_component(seed)),
        "candidate_source": "exact_seed",
        "candidate_sources": sources.clone(),
        "candidate_source_label": if is_path { "exact_file_path_seed" } else { "exact_symbol_seed" },
        "file_id": if is_path { json!(seed) } else { Value::Null },
        "path": if is_path { json!(seed) } else { Value::Null },
        "entity_id": Value::Null,
        "span": Value::Null,
        "evidence_role": "unknown",
        "proof_status": if graph_verification_failed { "no_proof_path_found" } else { "candidate_only" },
        "graph_proof": false,
        "claimable": false,
        "claimable_for_text": false,
        "claimable_for_graph": false,
        "diagnostic_only": false,
        "score": Value::Null,
        "rank": Value::Null,
        "matched_seeds": [seed],
        "requires_graph_verification": true,
        "verification_status": if graph_verification_failed { "no_proof_path_found" } else { "needs_graph_verification" },
        "graph_verification_status": if graph_verification_failed { "no_proof_path_found" } else { "needs_graph_verification" },
        "text_evidence_status": "absent",
        "reason": if graph_verification_failed {
            "exact seed preserved after graph verification; no proof path found, source/text fallback required before claiming"
        } else {
            "exact seed preserved before candidate cap; still requires source or graph verification"
        },
        "omitted": false,
        "truncated": false,
        "ranking_features": {
            "exact_seed_match": true,
            "file_path_match": is_path,
            "text_evidence_match": false,
            "symbol_entity_match": is_symbol,
            "graph_proximity": false,
            "source_role_compatible": true,
            "lifecycle_claimable": lifecycle_claimable,
            "proof_available": false,
            "vector_score_available": false,
            "binary_score_available": false,
            "rescue_match": false
        },
        "source_labels": sources
    })
}

pub(crate) fn path_evidence_retrieval_candidate_json(
    path: &Value,
    exact_seeds: &[String],
    lifecycle_claimable: bool,
    proof_path_available: bool,
) -> Value {
    let candidate_id = path
        .get("path_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown-path");
    let first_span = path
        .get("source_spans")
        .and_then(Value::as_array)
        .and_then(|spans| spans.first())
        .cloned()
        .unwrap_or(Value::Null);
    let file = first_span
        .get("file")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            first_span
                .get("repo_relative_path")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let evidence_role = path
        .get("evidence_role")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let graph_entity_id = path
        .get("target")
        .and_then(Value::as_str)
        .or_else(|| path.get("source").and_then(Value::as_str));
    let matched_seeds = exact_seeds
        .iter()
        .filter(|seed| context_path_candidate_matches_seed(path, seed))
        .cloned()
        .collect::<Vec<_>>();
    json!({
        "candidate_id": format!("path-evidence://{candidate_id}"),
        "candidate_source": "path_evidence",
        "candidate_sources": [
            "path_evidence",
            "graph_neighbor"
        ],
        "candidate_source_label": "graph_verified_path_evidence",
        "file_id": file.clone().map(Value::from).unwrap_or(Value::Null),
        "path": file.clone().map(Value::from).unwrap_or(Value::Null),
        "entity_id": graph_entity_id.map(Value::from).unwrap_or(Value::Null),
        "span": first_span,
        "evidence_role": evidence_role,
        "proof_status": if proof_path_available { "proof_path_found" } else { "candidate_only" },
        "graph_proof": proof_path_available,
        "claimable": lifecycle_claimable && proof_path_available,
        "claimable_for_text": false,
        "claimable_for_graph": lifecycle_claimable && proof_path_available,
        "diagnostic_only": false,
        "score": Value::Null,
        "source_score": path.get("confidence").cloned().unwrap_or(Value::Null),
        "rank": Value::Null,
        "matched_seeds": matched_seeds,
        "requires_graph_verification": false,
        "verification_status": if proof_path_available { "graph_verified" } else { "candidate_only" },
        "graph_verification_status": if proof_path_available { "graph_verified" } else { "candidate_only" },
        "text_evidence_status": "absent",
        "language_capability": path
            .get("language_capability")
            .cloned()
            .unwrap_or_else(|| context_pack_source_language_capability_json(
                file.as_deref(),
                "graph_path",
                if proof_path_available { "proof_path_found" } else { "candidate_only" },
                proof_path_available,
                path.get("exactness").and_then(Value::as_str).unwrap_or("unknown"),
                evidence_role,
            )),
        "reason": path
            .get("classification_reason")
            .and_then(Value::as_str)
            .unwrap_or("path evidence candidate"),
        "omitted": false,
        "truncated": false,
        "ranking_features": {
            "exact_seed_match": false,
            "file_path_match": false,
            "text_evidence_match": false,
            "symbol_entity_match": path
                .get("relations")
                .and_then(Value::as_array)
                .is_some_and(|relations| !relations.is_empty()),
            "graph_proximity": true,
            "source_role_compatible": evidence_role == "production" || evidence_role == "unknown",
            "lifecycle_claimable": lifecycle_claimable,
            "proof_available": proof_path_available,
            "vector_score_available": false,
            "binary_score_available": false,
            "rescue_match": false
        },
        "source_labels": [
            "path_evidence",
            "graph_verified"
        ]
    })
}

pub(crate) fn no_proof_fallback_retrieval_candidate_json(
    evidence: &Value,
    rank: usize,
    lifecycle_claimable: bool,
) -> Value {
    let file = evidence
        .get("file")
        .and_then(Value::as_str)
        .or_else(|| evidence.pointer("/span/file").and_then(Value::as_str));
    let span = evidence
        .get("source_span")
        .cloned()
        .or_else(|| evidence.get("span").cloned())
        .unwrap_or(Value::Null);
    let evidence_role = evidence
        .get("evidence_role")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    json!({
        "candidate_id": evidence.get("id").cloned().unwrap_or_else(|| json!(format!("fallback://{rank}"))),
        "candidate_source": "no_proof_fallback",
        "candidate_sources": [
            "no_proof_fallback"
        ],
        "candidate_source_label": "bounded_no_proof_fallback",
        "file_id": file.map(Value::from).unwrap_or(Value::Null),
        "path": file.map(Value::from).unwrap_or(Value::Null),
        "entity_id": evidence.get("id").cloned().unwrap_or(Value::Null),
        "span": span,
        "evidence_role": evidence_role,
        "proof_status": evidence
            .get("proof_status")
            .and_then(Value::as_str)
            .unwrap_or("no_proof_path_found"),
        "graph_proof": false,
        "claimable": false,
        "claimable_for_text": false,
        "claimable_for_graph": false,
        "diagnostic_only": false,
        "score": Value::Null,
        "rank": Value::Null,
        "matched_seeds": evidence
            .get("matched_seeds")
            .or_else(|| evidence.get("seed_matches"))
            .cloned()
            .unwrap_or_else(|| json!([])),
        "requires_graph_verification": true,
        "verification_status": "no_proof_path_found",
        "graph_verification_status": "no_proof_path_found",
        "text_evidence_status": "absent",
        "language_capability": evidence
            .get("language_capability")
            .cloned()
            .unwrap_or_else(|| context_pack_source_language_capability_json(
                file,
                "source_navigation_evidence",
                evidence
                    .get("proof_status")
                    .and_then(Value::as_str)
                    .unwrap_or("no_proof_path_found"),
                false,
                "source_navigation",
                evidence_role,
            )),
        "reason": evidence
            .get("classification_reason")
            .and_then(Value::as_str)
            .unwrap_or("fallback candidate; no graph proof"),
        "omitted": false,
        "truncated": false,
        "ranking_features": {
            "exact_seed_match": false,
            "file_path_match": false,
            "text_evidence_match": false,
            "symbol_entity_match": evidence.get("symbol").is_some(),
            "graph_proximity": false,
            "source_role_compatible": evidence_role == "production" || evidence_role == "unknown",
            "lifecycle_claimable": lifecycle_claimable,
            "proof_available": false,
            "vector_score_available": false,
            "binary_score_available": false,
            "rescue_match": false
        },
        "source_labels": [
            "no_proof_fallback"
        ]
    })
}

pub(crate) fn context_agent_seed_is_path(seed: &str) -> bool {
    let normalized = seed.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    normalized.contains('/')
        || lower == "config.in"
        || lower == "makefile"
        || lower.ends_with(".rs")
        || lower.ends_with(".py")
        || lower.ends_with(".js")
        || lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".jsx")
        || lower.ends_with(".go")
        || lower.ends_with(".java")
        || lower.ends_with(".c")
        || lower.ends_with(".h")
        || lower.ends_with(".cpp")
        || lower.ends_with(".hpp")
        || lower.ends_with(".mk")
        || lower.ends_with(".toml")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
        || lower.ends_with(".json")
        || lower.ends_with(".md")
        || lower.ends_with(".adoc")
}

pub(crate) fn context_agent_seed_is_symbol(seed: &str) -> bool {
    let trimmed = seed.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .any(|character| character.is_ascii_alphabetic() || character == '_')
        && trimmed.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | ':' | '.' | '-' | '*')
        })
}

pub(crate) fn context_agent_path_key(path: &str) -> String {
    path.replace('\\', "/").trim().to_ascii_lowercase()
}

pub(crate) fn context_agent_stable_component(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
}

pub(crate) fn context_path_candidate_matches_seed(path: &Value, seed: &str) -> bool {
    let seed_lower = seed.to_ascii_lowercase();
    path.get("path_id")
        .and_then(Value::as_str)
        .is_some_and(|value| value.to_ascii_lowercase().contains(&seed_lower))
        || path
            .get("summary")
            .and_then(Value::as_str)
            .is_some_and(|value| value.to_ascii_lowercase().contains(&seed_lower))
        || path
            .get("source")
            .and_then(Value::as_str)
            .is_some_and(|value| value.to_ascii_lowercase().contains(&seed_lower))
        || path
            .get("target")
            .and_then(Value::as_str)
            .is_some_and(|value| value.to_ascii_lowercase().contains(&seed_lower))
        || path
            .get("source_spans")
            .and_then(Value::as_array)
            .is_some_and(|spans| {
                spans.iter().any(|span| {
                    span.get("file")
                        .or_else(|| span.get("repo_relative_path"))
                        .and_then(Value::as_str)
                        .is_some_and(|file| file.to_ascii_lowercase().contains(&seed_lower))
                })
            })
}

pub(crate) fn context_agent_candidate_dedup_key(
    candidate: &Value,
    exact_path_keys: &BTreeSet<String>,
    exact_seed_keys: &BTreeSet<String>,
) -> String {
    if let Some(path) = candidate.get("path").and_then(Value::as_str) {
        let key = context_agent_path_key(path);
        if context_agent_candidate_has_source(candidate, "exact_seed")
            || exact_path_keys.contains(&key)
        {
            return format!("path:{key}");
        }
        if context_agent_candidate_has_source(candidate, "text_evidence") {
            return format!("text-path:{key}");
        }
    }
    if let Some(token) = candidate.get("matched_token").and_then(Value::as_str) {
        let key = context_agent_seed_key(token);
        if exact_seed_keys.contains(&key) {
            return format!("seed:{key}");
        }
    }
    for seed in context_agent_string_set(candidate, "matched_seeds") {
        let key = context_agent_seed_key(&seed);
        if exact_seed_keys.contains(&key) {
            return format!("seed:{key}");
        }
    }
    if let Some(entity_id) = candidate.get("entity_id").and_then(Value::as_str) {
        if !entity_id.trim().is_empty() {
            return format!("entity:{entity_id}");
        }
    }
    if context_agent_candidate_has_source(candidate, "path_evidence") {
        return format!("path-evidence:{}", context_agent_candidate_id(candidate));
    }
    if let (Some(path), Some(span)) = (
        candidate.get("path").and_then(Value::as_str),
        candidate.get("span"),
    ) {
        if let (Some(start), Some(end)) = (
            span.get("start_line").and_then(Value::as_u64),
            span.get("end_line").and_then(Value::as_u64),
        ) {
            return format!("span:{}:{start}:{end}", context_agent_path_key(path));
        }
    }
    format!("candidate:{}", context_agent_candidate_id(candidate))
}

pub(crate) fn context_agent_candidate_id(candidate: &Value) -> String {
    candidate
        .get("candidate_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

pub(crate) fn context_agent_candidate_sources(candidate: &Value) -> BTreeSet<String> {
    let mut sources = BTreeSet::new();
    if let Some(array) = candidate.get("candidate_sources").and_then(Value::as_array) {
        for source in array.iter().filter_map(Value::as_str) {
            sources.insert(source.to_string());
        }
    }
    if let Some(source) = candidate.get("candidate_source").and_then(Value::as_str) {
        sources.insert(source.to_string());
    }
    sources
}

pub(crate) fn context_agent_candidate_has_source(candidate: &Value, expected: &str) -> bool {
    context_agent_candidate_sources(candidate).contains(expected)
}

pub(crate) fn enforce_context_agent_candidate_vector_truth(candidate: &mut Value) {
    let sources = context_agent_candidate_sources(candidate);
    let vector_semantic_candidate =
        sources.contains("vector_semantic") || sources.contains("vector_rerank");
    let binary_vector_candidate = sources.contains("binary_vector");
    if !vector_semantic_candidate && !binary_vector_candidate {
        return;
    }
    let Some(object) = candidate.as_object_mut() else {
        return;
    };
    object.insert(
        "vector_candidate_claim_boundary".to_string(),
        json!("vector_evidence_candidate_only; graph_proof_requires_graph_source_verification"),
    );
    if vector_semantic_candidate {
        object.insert(
            "semantic_backend".to_string(),
            json!("deterministic_token_projection"),
        );
        object.insert(
            "semantic_provider_kind".to_string(),
            json!("deterministic_token_projection"),
        );
        object.insert("learned_semantic_embeddings".to_string(), json!(false));
        object.insert(
            "semantic_embedding_claim".to_string(),
            json!("not_learned_semantic_embedding"),
        );
    }
    if binary_vector_candidate {
        object.insert(
            "binary_vector_projection".to_string(),
            json!("candidate_only"),
        );
    }
}

pub(crate) fn context_agent_candidate_sources_verified(candidates: &[Value]) -> Vec<String> {
    let mut sources = BTreeSet::new();
    for candidate in candidates {
        let requires_graph_verification = candidate
            .get("requires_graph_verification")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let graph_proof = candidate
            .get("graph_proof")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let verification_status = candidate
            .get("verification_status")
            .or_else(|| candidate.get("graph_verification_status"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if requires_graph_verification
            || graph_proof
            || matches!(verification_status, "graph_verified")
        {
            sources.extend(context_agent_candidate_sources(candidate));
        }
    }
    sources.into_iter().collect()
}

pub(crate) fn context_agent_string_set(candidate: &Value, key: &str) -> BTreeSet<String> {
    candidate
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default()
}

pub(crate) fn merge_context_agent_candidate(existing: &mut Value, incoming: Value) {
    let existing_sources = context_agent_candidate_sources(existing);
    let incoming_sources = context_agent_candidate_sources(&incoming);
    let sources = existing_sources
        .union(&incoming_sources)
        .cloned()
        .collect::<BTreeSet<_>>();
    let source_labels = context_agent_string_set(existing, "source_labels")
        .union(&context_agent_string_set(&incoming, "source_labels"))
        .cloned()
        .collect::<BTreeSet<_>>();
    let matched_seeds = context_agent_string_set(existing, "matched_seeds")
        .union(&context_agent_string_set(&incoming, "matched_seeds"))
        .cloned()
        .collect::<BTreeSet<_>>();
    let matched_tokens = context_agent_string_set(existing, "matched_tokens")
        .union(&context_agent_string_set(&incoming, "matched_tokens"))
        .cloned()
        .collect::<BTreeSet<_>>();
    let rescue_kinds = context_agent_string_set(existing, "rescue_kinds")
        .union(&context_agent_string_set(&incoming, "rescue_kinds"))
        .cloned()
        .collect::<BTreeSet<_>>();
    let rescue_reasons = unique_limited_strings(
        context_agent_rescue_reason_parts(existing)
            .into_iter()
            .chain(context_agent_rescue_reason_parts(&incoming)),
        4,
    );
    let matched_token = existing
        .get("matched_token")
        .and_then(Value::as_str)
        .or_else(|| incoming.get("matched_token").and_then(Value::as_str))
        .map(str::to_string)
        .or_else(|| matched_tokens.iter().next().cloned());
    let ranking_features = merge_context_agent_ranking_features(existing, &incoming);
    let proof_status = merge_context_agent_status(
        existing.get("proof_status").and_then(Value::as_str),
        incoming.get("proof_status").and_then(Value::as_str),
        &[
            "proof_path_found",
            "no_proof_path_found",
            "not_graph_proof",
            "candidate_only",
            "unknown",
        ],
    );
    let verification_status = merge_context_agent_status(
        existing.get("verification_status").and_then(Value::as_str),
        incoming.get("verification_status").and_then(Value::as_str),
        &[
            "graph_verified",
            "no_proof_path_found",
            "not_graph_proof",
            "needs_graph_verification",
            "candidate_only",
            "unknown",
        ],
    );
    let graph_verification_status = merge_context_agent_status(
        existing
            .get("graph_verification_status")
            .and_then(Value::as_str)
            .or_else(|| existing.get("verification_status").and_then(Value::as_str)),
        incoming
            .get("graph_verification_status")
            .and_then(Value::as_str)
            .or_else(|| incoming.get("verification_status").and_then(Value::as_str)),
        &[
            "graph_verified",
            "no_proof_path_found",
            "not_graph_proof",
            "needs_graph_verification",
            "candidate_only",
            "unknown",
        ],
    );
    let text_evidence_status = merge_context_agent_status(
        existing.get("text_evidence_status").and_then(Value::as_str),
        incoming.get("text_evidence_status").and_then(Value::as_str),
        &[
            "not_graph_proof",
            "no_proof_path_found",
            "candidate_only",
            "absent",
            "unknown",
        ],
    );
    let evidence_role = merge_context_agent_status(
        existing.get("evidence_role").and_then(Value::as_str),
        incoming.get("evidence_role").and_then(Value::as_str),
        &[
            "production",
            "text_evidence",
            "test",
            "mock",
            "mixed",
            "unknown",
        ],
    );
    let reason = unique_limited_strings(
        [
            existing
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            incoming
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ],
        4,
    )
    .join("; ");
    let graph_proof = existing
        .get("graph_proof")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || incoming
            .get("graph_proof")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let claimable = existing
        .get("claimable")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || incoming
            .get("claimable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let claimable_for_text = existing
        .get("claimable_for_text")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || incoming
            .get("claimable_for_text")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let claimable_for_graph = existing
        .get("claimable_for_graph")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || incoming
            .get("claimable_for_graph")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let requires_graph_verification = existing
        .get("requires_graph_verification")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || incoming
            .get("requires_graph_verification")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let truncated = existing
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || incoming
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let numeric_values = ["source_score", "vector_score", "binary_score"]
        .iter()
        .filter_map(|key| {
            max_context_agent_numeric_field(existing, &incoming, key).map(|value| (*key, value))
        })
        .collect::<Vec<_>>();

    let Some(object) = existing.as_object_mut() else {
        return;
    };
    for key in [
        "file_id",
        "path",
        "entity_id",
        "span",
        "snippet",
        "source_score",
    ] {
        let needs_value = object.get(key).is_none_or(Value::is_null)
            || (key == "snippet"
                && object
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(str::is_empty));
        if needs_value {
            if let Some(value) = incoming.get(key) {
                if !value.is_null() {
                    object.insert(key.to_string(), value.clone());
                }
            }
        }
    }
    for (key, value) in numeric_values {
        object.insert(key.to_string(), json!(value));
    }
    object.insert(
        "candidate_source".to_string(),
        json!(primary_context_agent_candidate_source(&sources)),
    );
    object.insert(
        "candidate_sources".to_string(),
        json!(sources.iter().cloned().collect::<Vec<_>>()),
    );
    object.insert(
        "source_labels".to_string(),
        json!(source_labels.iter().cloned().collect::<Vec<_>>()),
    );
    object.insert(
        "matched_seeds".to_string(),
        json!(matched_seeds.iter().cloned().collect::<Vec<_>>()),
    );
    if !matched_tokens.is_empty() {
        object.insert(
            "matched_tokens".to_string(),
            json!(matched_tokens.iter().cloned().collect::<Vec<_>>()),
        );
    }
    if let Some(matched_token) = matched_token {
        object.insert("matched_token".to_string(), json!(matched_token));
    }
    if !rescue_kinds.is_empty() {
        object.insert(
            "rescue_kinds".to_string(),
            json!(rescue_kinds.iter().cloned().collect::<Vec<_>>()),
        );
    }
    if !rescue_reasons.is_empty() {
        object.insert(
            "rescue_reason".to_string(),
            json!(rescue_reasons.join("; ")),
        );
    }
    object.insert("evidence_role".to_string(), json!(evidence_role));
    object.insert("proof_status".to_string(), json!(proof_status));
    object.insert("graph_proof".to_string(), json!(graph_proof));
    object.insert("claimable".to_string(), json!(claimable));
    object.insert("claimable_for_text".to_string(), json!(claimable_for_text));
    object.insert(
        "claimable_for_graph".to_string(),
        json!(claimable_for_graph),
    );
    object.insert(
        "requires_graph_verification".to_string(),
        json!(requires_graph_verification),
    );
    object.insert(
        "verification_status".to_string(),
        json!(verification_status),
    );
    object.insert(
        "graph_verification_status".to_string(),
        json!(graph_verification_status),
    );
    object.insert(
        "text_evidence_status".to_string(),
        json!(text_evidence_status),
    );
    object.insert("reason".to_string(), json!(reason));
    object.insert("truncated".to_string(), json!(truncated));
    object.insert("ranking_features".to_string(), ranking_features);
}

pub(crate) fn context_agent_rescue_reason_parts(candidate: &Value) -> Vec<String> {
    candidate
        .get("rescue_reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn max_context_agent_numeric_field(
    left: &Value,
    right: &Value,
    key: &str,
) -> Option<f64> {
    match (
        left.get(key).and_then(Value::as_f64),
        right.get(key).and_then(Value::as_f64),
    ) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

pub(crate) fn merge_context_agent_ranking_features(existing: &Value, incoming: &Value) -> Value {
    let mut merged = serde_json::Map::new();
    for key in [
        "exact_seed_match",
        "file_path_match",
        "text_evidence_match",
        "symbol_entity_match",
        "graph_proximity",
        "source_role_compatible",
        "lifecycle_claimable",
        "proof_available",
        "vector_score_available",
        "binary_score_available",
        "rescue_match",
    ] {
        let value = existing
            .pointer(&format!("/ranking_features/{key}"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || incoming
                .pointer(&format!("/ranking_features/{key}"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        merged.insert(key.to_string(), json!(value));
    }
    Value::Object(merged)
}

pub(crate) fn merge_context_agent_status(
    left: Option<&str>,
    right: Option<&str>,
    priority: &[&str],
) -> String {
    let left = left.unwrap_or("unknown");
    let right = right.unwrap_or("unknown");
    for value in priority {
        if left == *value || right == *value {
            return (*value).to_string();
        }
    }
    left.to_string()
}

pub(crate) fn primary_context_agent_candidate_source(sources: &BTreeSet<String>) -> String {
    for source in [
        "exact_seed",
        "file_path_seed",
        "path_evidence",
        "graph_neighbor",
        "nuance_rescue",
        "vector_semantic",
        "binary_vector",
        "vector_rerank",
        "symbol_lookup",
        "text_evidence",
        "lexical_fts",
        "no_proof_fallback",
        "diagnostic",
        "unknown",
    ] {
        if sources.contains(source) {
            return source.to_string();
        }
    }
    "unknown".to_string()
}

pub(crate) fn context_agent_candidate_rank_score(candidate: &Value) -> i64 {
    let feature = |key: &str| {
        candidate
            .pointer(&format!("/ranking_features/{key}"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let mut score = 0i64;
    if feature("exact_seed_match") {
        score += 10_000;
    }
    if feature("file_path_match") {
        score += 2_000;
    }
    if feature("text_evidence_match") {
        score += 1_500;
    }
    if feature("symbol_entity_match") {
        score += 1_250;
    }
    if context_agent_candidate_has_source(candidate, "nuance_rescue") {
        score += 2_200;
    }
    if context_agent_candidate_has_source(candidate, "vector_semantic") {
        score += 900;
    }
    if context_agent_candidate_has_source(candidate, "binary_vector") {
        score += 700;
    }
    if feature("rescue_match") {
        score += 300;
    }
    if feature("graph_proximity") {
        score += 1_000;
    }
    if feature("proof_available") {
        score += 750;
    }
    if feature("source_role_compatible") {
        score += 400;
    }
    if feature("lifecycle_claimable") {
        score += 250;
    }
    if feature("text_evidence_match") {
        if let Some(path) = candidate.get("path").and_then(Value::as_str) {
            let lower = context_agent_path_key(path);
            if lower.ends_with(".mk") {
                score += 500;
            } else if lower.ends_with("config.in") {
                score += 450;
            } else if lower.ends_with(".toml")
                || lower.ends_with(".yaml")
                || lower.ends_with(".yml")
                || lower.ends_with(".json")
            {
                score += 350;
            } else if lower.contains("/docs/") || lower.ends_with(".md") || lower.ends_with(".adoc")
            {
                score += 150;
            }
            if lower.contains("/package/") {
                score += 100;
            }
        }
    }
    if let Some(source_score) = candidate.get("source_score").and_then(Value::as_f64) {
        score += (source_score * 100.0).round() as i64;
    }
    if let Some(vector_score) = candidate.get("vector_score").and_then(Value::as_f64) {
        score += (vector_score * 100.0).round() as i64;
    }
    if let Some(binary_score) = candidate.get("binary_score").and_then(Value::as_f64) {
        score += (binary_score * 100.0).round() as i64;
    }
    score += (context_agent_candidate_sources(candidate).len() as i64) * 50;
    score
}

pub(crate) fn recommended_tests_from_fallback_evidence(
    fallback_evidence: &[ContextPackFallbackEvidence],
) -> Vec<String> {
    let mut tests = BTreeSet::new();
    for evidence in fallback_evidence {
        if matches!(
            evidence.evidence_role,
            EvidenceRole::Test | EvidenceRole::Mock | EvidenceRole::Mixed
        ) && context_fallback_evidence_kind_recommends_test(&evidence.kind)
        {
            let symbol = evidence.symbol.trim();
            if !symbol.is_empty()
                && symbol
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
            {
                tests.insert(format!("cargo test {symbol}"));
            }
        }
    }
    tests.into_iter().take(12).collect()
}

pub(crate) fn context_fallback_evidence_kind_recommends_test(kind: &str) -> bool {
    matches!(
        kind.trim().to_ascii_lowercase().as_str(),
        "function" | "method" | "testcase" | "test_case" | "test-case" | "test"
    )
}

pub(crate) fn load_context_fallback_snippets(
    repo_root: &Path,
    fallback_evidence: &[ContextPackFallbackEvidence],
    max_snippets: usize,
) -> Result<Vec<ContextSnippet>, String> {
    if fallback_evidence.is_empty() || max_snippets == 0 {
        return Ok(Vec::new());
    }
    let spans = fallback_evidence
        .iter()
        .map(|evidence| evidence.source_span.clone())
        .collect::<Vec<_>>();
    let (_, mut snippets, _, _) =
        load_context_sources_and_snippets(repo_root, &spans, max_snippets)?;
    let reason_by_key = fallback_evidence
        .iter()
        .map(|evidence| {
            (
                format!(
                    "{}:{}",
                    evidence.source_span.repo_relative_path,
                    context_line_range_for_span(&evidence.source_span)
                ),
                format!(
                    "{} fallback ({})",
                    evidence.fallback_source, evidence.classification_reason
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for snippet in &mut snippets {
        if let Some(reason) = reason_by_key.get(&format!("{}:{}", snippet.file, snippet.lines)) {
            snippet.reason = reason.clone();
        } else {
            snippet.reason = "test-impact fallback source span".to_string();
        }
    }
    Ok(snippets)
}

pub(crate) fn build_context_packet_from_stored_evidence(
    options: &ContextPackOptions,
    raw_seed_values: &[String],
    seed_ids: &[String],
    seed_entities: &[ContextEntitySummary],
    verified_paths: Vec<PathEvidence>,
    snippets: Vec<ContextSnippet>,
    fallback_evidence: Vec<ContextPackFallbackEvidence>,
    fallback_packet: Option<ContextPacket>,
    budgets: ContextPackBudgets,
    stored_path_count: usize,
    requested_span_count: usize,
) -> ContextPacket {
    let symbol_seed_entities = seed_entities
        .iter()
        .filter(|entity| context_pack_entity_allowed_for_symbol_output(entity, &options.mode))
        .collect::<Vec<_>>();
    let mut symbols = unique_limited_strings(
        raw_seed_values
            .iter()
            .cloned()
            .chain(symbol_seed_entities.iter().map(|entity| entity.id.clone()))
            .chain(
                symbol_seed_entities
                    .iter()
                    .map(|entity| entity.name.clone()),
            )
            .chain(
                symbol_seed_entities
                    .iter()
                    .map(|entity| entity.qualified_name.clone()),
            )
            .chain(
                symbol_seed_entities
                    .iter()
                    .map(|entity| entity.repo_relative_path.clone()),
            )
            .chain(
                verified_paths
                    .iter()
                    .flat_map(|path| [path.source.clone(), path.target.clone()]),
            )
            .chain(
                fallback_evidence
                    .iter()
                    .map(|evidence| evidence.symbol.clone()),
            ),
        budgets.max_seed_entities * 4,
    );
    symbols.sort();
    symbols.dedup();

    let mut recommended_tests = recommended_tests_from_path_evidence(&verified_paths);
    recommended_tests.extend(recommended_tests_from_fallback_evidence(&fallback_evidence));
    let mut risks = risks_from_path_evidence(&verified_paths);
    if let Some(packet) = fallback_packet {
        recommended_tests.extend(packet.recommended_tests);
        risks.extend(packet.risks);
    }
    recommended_tests.sort();
    recommended_tests.dedup();
    risks.sort();
    risks.dedup();

    let mut metadata = Metadata::new();
    metadata.insert("phase".to_string(), json!("30"));
    metadata.insert("retrieval".to_string(), json!("stored-path-evidence-first"));
    metadata.insert("token_budget".to_string(), json!(options.token_budget));
    metadata.insert("exact_seed_count".to_string(), json!(seed_ids.len()));
    let prompt_seed_extraction = extract_prompt_seed_provenance(&options.task);
    metadata.insert(
        "prompt_intent".to_string(),
        json!(prompt_seed_extraction.intent.as_str()),
    );
    metadata.insert(
        "prompt_seed_provenance".to_string(),
        json!(prompt_seed_extraction
            .seeds
            .iter()
            .map(|seed| seed.provenance_json())
            .collect::<Vec<_>>()),
    );
    metadata.insert(
        "candidate_path_count_before_dedup".to_string(),
        json!(stored_path_count),
    );
    metadata.insert(
        "candidate_path_count_after_dedup".to_string(),
        json!(verified_paths.len()),
    );
    metadata.insert(
        "candidate_path_count_after_filter".to_string(),
        json!(verified_paths.len()),
    );
    metadata.insert(
        "candidate_path_count_after_truncate".to_string(),
        json!(verified_paths.len()),
    );
    metadata.insert(
        "hard_budgets".to_string(),
        json!({
            "max_seed_entities": budgets.max_seed_entities,
            "max_candidate_paths": budgets.max_candidate_paths,
            "max_returned_proof_paths": budgets.max_returned_proof_paths,
            "max_snippets": budgets.max_snippets,
            "max_traversal_depth": budgets.max_traversal_depth,
            "max_path_evidence_rows": budgets.max_path_evidence_rows,
            "max_path_edges_per_path": budgets.max_path_edges_per_path,
            "max_path_evidence_hydration_bytes": budgets.max_hydration_bytes,
            "path_evidence_timeout_ms": budgets.path_evidence_timeout_ms,
        }),
    );
    metadata.insert(
        "excluded_structural_relations_default".to_string(),
        json!(["CONTAINS", "DEFINED_IN", "DECLARES", "ARGUMENT_N"]),
    );
    metadata.insert(
        "requested_source_span_count".to_string(),
        json!(requested_span_count),
    );
    metadata.insert("planning_roles".to_string(), json!(CONTEXT_PLANNING_ROLES));
    metadata.insert(
        "planning_packet_budgets".to_string(),
        json!({
            "evidence_budget": budgets.max_candidate_paths,
            "snippet_budget": budgets.max_snippets,
            "likely_files_budget": CONTEXT_PLANNING_PACKET_FILE_LIMIT,
            "planning_summary_budget": 1,
            "follow_up_queries_budget": CONTEXT_PLANNING_PACKET_QUERY_LIMIT,
            "warning_budget": 8,
            "explain_debug_budget": "separate_from_agent_json_evidence_budget",
        }),
    );
    metadata.insert(
        "source_span_coverage".to_string(),
        json!(if requested_span_count == 0 {
            1.0
        } else {
            snippets.len() as f64 / requested_span_count as f64
        }),
    );
    metadata.insert(
        "proof_path_available".to_string(),
        json!(!verified_paths.is_empty()),
    );
    let proof_path_available = !verified_paths.is_empty();
    let proof_status = if proof_path_available {
        "proof_path_found"
    } else if !fallback_evidence.is_empty() {
        "no_proof_path_found"
    } else {
        "unknown"
    };
    let graph_verification_candidate_count = seed_ids.len() + seed_entities.len();
    let graph_verification_status = if proof_path_available {
        "graph_verified"
    } else if graph_verification_candidate_count > 0 {
        "no_proof_path_found"
    } else {
        "no_graph_candidates"
    };
    let graph_verification_reason = if proof_path_available {
        "graph verification found at least one proof path"
    } else if !fallback_evidence.is_empty() && graph_verification_candidate_count > 0 {
        "graph verification found no proof path; returning fallback source/text evidence"
    } else if !fallback_evidence.is_empty() {
        "no graph-verifiable candidates were available; returning fallback source/text evidence"
    } else if graph_verification_candidate_count > 0 {
        "graph verification found no proof path and no fallback source/text evidence was found"
    } else {
        "no graph-verifiable candidates and no fallback source/text evidence were found"
    };
    metadata.insert("proof_status".to_string(), json!(proof_status));
    metadata.insert("graph_proof".to_string(), json!(proof_path_available));
    metadata.insert(
        "graph_verification_status".to_string(),
        json!(graph_verification_status),
    );
    metadata.insert(
        "graph_verification_candidate_count".to_string(),
        json!(graph_verification_candidate_count),
    );
    metadata.insert(
        "graph_verification_reason".to_string(),
        json!(graph_verification_reason),
    );
    metadata.insert(
        "proof_failure_reason".to_string(),
        if proof_path_available {
            Value::Null
        } else {
            json!(graph_verification_reason)
        },
    );
    metadata.insert(
        "evidence_status".to_string(),
        json!(if proof_path_available {
            "proof_path_found"
        } else if !fallback_evidence.is_empty() {
            "fallback_evidence_found"
        } else {
            "no_evidence_found"
        }),
    );
    if !proof_path_available {
        let has_text_evidence = fallback_evidence
            .iter()
            .any(|evidence| evidence.evidence_role_label.as_deref() == Some("text_evidence"));
        metadata.insert(
            "claimability".to_string(),
            if has_text_evidence {
                text_evidence_claimability_json()
            } else if fallback_evidence.is_empty() {
                context_pack_unknown_claimability_json()
            } else {
                context_pack_fallback_claimability_json()
            },
        );
    }
    if !fallback_evidence.is_empty() {
        let selected_role_coverage = unique_limited_strings(
            fallback_evidence
                .iter()
                .map(context_planning_role_for_evidence)
                .map(str::to_string)
                .filter(|role| role != "unknown"),
            CONTEXT_PLANNING_ROLES.len(),
        );
        let likely_files = unique_limited_strings(
            fallback_evidence
                .iter()
                .map(|evidence| evidence.source_span.repo_relative_path.clone()),
            budgets.max_snippets.max(1),
        );
        let follow_up_queries = unique_limited_strings(
            fallback_evidence
                .iter()
                .flat_map(|evidence| evidence.follow_up_queries.iter().cloned()),
            budgets.max_snippets.max(1),
        );
        metadata.insert("likely_files".to_string(), json!(likely_files));
        metadata.insert("follow_up_queries".to_string(), json!(follow_up_queries));
        metadata.insert(
            "selected_role_coverage".to_string(),
            json!(selected_role_coverage),
        );
        metadata.insert("omitted_by_dedup".to_string(), json!(0));
        let metadata_fallback_limit = budgets.max_snippets.max(1);
        metadata.insert(
            "fallback_evidence".to_string(),
            json!(fallback_evidence
                .iter()
                .take(metadata_fallback_limit)
                .map(fallback_evidence_json)
                .collect::<Vec<_>>()),
        );
    }

    let mut packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols,
        verified_paths,
        risks,
        recommended_tests,
        snippets,
        metadata,
    };
    compact_context_packet_for_cli(&mut packet, options.token_budget.max(32));
    packet
}

pub(crate) fn context_pack_traversal_telemetry_json(
    options: &ContextPackOptions,
    raw_seed_values: &[String],
    seed_ids: &[String],
    budgets: ContextPackBudgets,
    stored_path_count: usize,
    paths_returned: usize,
    fallback_edge_count: usize,
    fallback_traversal_telemetry: Option<Value>,
    fallback_evidence_count: usize,
) -> Value {
    let traversal_policy = TraversalPolicy::for_mode(&options.mode);
    let relation_modes = context_pack_allowed_relation_names(&options.mode)
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let no_proof_fallback_reason = if paths_returned == 0 && fallback_evidence_count > 0 {
        Some("graph verification found no proof path; fallback source/text evidence returned")
    } else if paths_returned == 0 {
        Some("graph verification found no proof path")
    } else {
        None
    };
    json!({
        "schema_version": 1,
        "diagnostic_only": true,
        "measurement_scope": "context_pack_graph_verification",
        "seed_count": seed_ids.len(),
        "raw_seed_count": raw_seed_values.len(),
        "candidate_count_by_source": {
            "path_evidence": stored_path_count,
            "graph_neighbor": fallback_edge_count,
            "no_proof_fallback": fallback_evidence_count
        },
        "traversal_mode": traversal_policy.mode.as_str(),
        "relation_allowlist": relation_modes.clone(),
        "relation_modes": relation_modes,
        "traversal_policy": traversal_policy.mode.as_str(),
        "max_depth": budgets.max_traversal_depth,
        "max_paths": budgets.max_candidate_paths,
        "max_edge_visits": 2048,
        "max_neighbors_per_node": traversal_policy.max_neighbors_per_node,
        "max_candidates_per_seed": budgets.max_seed_entities,
        "max_structural_expansion": traversal_policy.max_structural_expansion,
        "timeout_ms": traversal_policy.timeout_ms,
        "edges_visited": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/edges_visited"))
            .cloned()
            .unwrap_or_else(|| json!(fallback_edge_count)),
        "nodes_visited": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/nodes_visited"))
            .cloned()
            .unwrap_or(Value::Null),
        "neighbors_expanded": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/neighbors_expanded"))
            .cloned()
            .unwrap_or_else(|| json!(fallback_edge_count)),
        "structural_edges_skipped": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/structural_edges_skipped"))
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "source_role_blocked_edges": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/source_role_blocked_edges"))
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "cycles_cut": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/cycles_cut"))
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "budget_stop_reason": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/budget_stop_reasons"))
            .cloned()
            .unwrap_or_else(|| json!({"not_available_for_stored_path_lookup": 1})),
        "paths_found": stored_path_count,
        "paths_returned": paths_returned,
        "time_ms": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/time_ms"))
            .cloned()
            .unwrap_or(Value::Null),
        "source_role_filter": {
            "applied": true,
            "mode": options.mode,
            "blocked_edges": fallback_traversal_telemetry
                .as_ref()
                .and_then(|value| value.pointer("/aggregate/source_role_blocked_edges"))
                .cloned()
                .unwrap_or_else(|| json!(0)),
        },
        "source_role_filters_applied": true,
        "heuristic_edges_skipped": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/heuristic_edges_skipped"))
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "heuristic_edges_seen": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/heuristic_edges_seen"))
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "derived_edge_provenance_checks": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/derived_edge_provenance_checks"))
            .cloned()
            .unwrap_or(Value::Null),
        "derived_edge_provenance_blocked_edges": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/derived_edge_provenance_blocked_edges"))
            .cloned()
            .unwrap_or(Value::Null),
        "traversal_result_labels": fallback_traversal_telemetry
            .as_ref()
            .and_then(|value| value.pointer("/aggregate/result_labels"))
            .cloned()
            .unwrap_or(Value::Null),
        "no_proof_fallback_reason": no_proof_fallback_reason,
        "fallback_engine_telemetry": fallback_traversal_telemetry,
        "notes": [
            "stored PathEvidence lookup does not expose true edge-visit counts",
            "fallback engine telemetry is present only when context-pack falls back to bounded seed-adjacent edges",
            "default agent output remains compact; this object is surfaced through retrieval_explain"
        ]
    })
}

pub(crate) fn context_pack_graph_verification_json(
    packet: &ContextPacket,
    proof_status: &str,
    graph_proof: bool,
    evidence_status: &str,
    fallback_evidence_count: usize,
) -> Value {
    let status = packet
        .metadata
        .get("graph_verification_status")
        .and_then(Value::as_str)
        .unwrap_or(if graph_proof {
            "graph_verified"
        } else if proof_status == "no_proof_path_found" {
            "no_proof_path_found"
        } else {
            "unknown"
        });
    let candidate_count = packet
        .metadata
        .get("graph_verification_candidate_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let reason = packet
        .metadata
        .get("graph_verification_reason")
        .and_then(Value::as_str)
        .unwrap_or(if graph_proof {
            "graph verification found at least one proof path"
        } else if fallback_evidence_count > 0 {
            "graph verification found no proof path; returning fallback source/text evidence"
        } else {
            "no graph proof path and no fallback source/text evidence were found"
        });
    let proof_failure_reason = packet
        .metadata
        .get("proof_failure_reason")
        .cloned()
        .unwrap_or(if graph_proof {
            Value::Null
        } else {
            json!(reason)
        });

    json!({
        "status": status,
        "candidate_count": candidate_count,
        "proof_status": proof_status,
        "graph_proof": graph_proof,
        "evidence_status": evidence_status,
        "fallback_evidence_count": fallback_evidence_count,
        "text_only_candidates_require_graph_verification": false,
        "source_text_fallback_available": fallback_evidence_count > 0,
        "reason": reason,
        "proof_failure_reason": proof_failure_reason,
        "contract": {
            "graph_entities_can_be_verified": true,
            "text_only_candidates_are_source_evidence": true,
            "text_evidence_is_not_graph_proof": true,
            "missing_proof_must_remain_explicit": true
        }
    })
}

pub(crate) fn context_pack_retrieval_architecture_json(
    packet: &ContextPacket,
    paths: &[Value],
    fallback_evidence: &[Value],
    snippets: &[Value],
    proof_status: &str,
    graph_proof: bool,
    evidence_status: &str,
    follow_up_queries: &Value,
    likely_files: &Value,
) -> Value {
    let exact_seed_count = packet
        .metadata
        .get("exact_seed_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let candidate_paths_before_dedup = packet
        .metadata
        .get("candidate_path_count_before_dedup")
        .and_then(Value::as_u64)
        .unwrap_or(paths.len() as u64);
    let candidate_paths_after_dedup = packet
        .metadata
        .get("candidate_path_count_after_dedup")
        .and_then(Value::as_u64)
        .unwrap_or(packet.verified_paths.len() as u64);
    let fallback_evidence_count = packet
        .metadata
        .get("fallback_evidence")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(fallback_evidence.len());
    let follow_up_query_count = follow_up_queries.as_array().map(Vec::len).unwrap_or(0);
    let likely_file_count = likely_files.as_array().map(Vec::len).unwrap_or(0);
    let prompt_intent = packet
        .metadata
        .get("prompt_intent")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let prompt_seed_provenance = packet
        .metadata
        .get("prompt_seed_provenance")
        .and_then(Value::as_array)
        .map(|seeds| {
            seeds
                .iter()
                .take(12)
                .map(|seed| {
                    json!({
                        "seed": seed.get("seed").or_else(|| seed.get("value")).cloned().unwrap_or(Value::Null),
                        "seed_kind": seed.get("seed_kind").or_else(|| seed.get("kind")).cloned().unwrap_or(Value::Null),
                        "source_text": seed.get("source_text").cloned().unwrap_or(Value::Null),
                        "intent_contribution": seed.get("intent_contribution").cloned().unwrap_or_else(|| json!("unknown")),
                        "exactness": seed.get("exactness").cloned().unwrap_or_else(|| json!("unknown")),
                        "ignored_reason": seed.get("ignored_reason").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let graph_verification = context_pack_graph_verification_json(
        packet,
        proof_status,
        graph_proof,
        evidence_status,
        fallback_evidence_count,
    );
    let graph_verification_status = graph_verification
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let graph_verification_reason = graph_verification
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("graph verification status unknown");
    let graph_stage_status = if graph_proof {
        "proof_path_found"
    } else if proof_status == "no_proof_path_found" {
        "checked_no_proof_path_found"
    } else {
        "no_graph_candidate_proof"
    };
    let text_stage_status = if fallback_evidence_count > 0 {
        "active_current"
    } else {
        "no_text_evidence_candidates"
    };
    let lexical_stage_status = if follow_up_query_count > 0 {
        "bounded_follow_up_queries_only"
    } else {
        "no_follow_up_queries"
    };
    let nuance_trace = packet.metadata.get("nuance_rescue_trace");
    let nuance_stage_enabled = nuance_trace
        .and_then(|trace| trace.get("rescue_enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let nuance_candidate_count = nuance_trace
        .and_then(|trace| trace.get("rescue_candidate_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let nuance_stage_status = if nuance_stage_enabled {
        nuance_trace
            .and_then(|trace| trace.get("rescue_status"))
            .and_then(Value::as_str)
            .unwrap_or("active_current")
    } else {
        "inactive_requires_explicit_flag"
    };
    let inactive_lanes = if nuance_stage_enabled {
        vec!["binary_vector_candidates", "shell_ready_probe_commands"]
    } else {
        vec![
            "binary_vector_candidates",
            "nuance_rescue_candidates",
            "shell_ready_probe_commands",
        ]
    };
    let (task_intent, task_profile, retrieval_plan) = plan_task_retrieval(&packet.task);
    let task_profile_summary = json!({
        "profile_id": task_profile.profile_id,
        "profile_name": task_profile.profile_name,
        "graph_expectation": task_profile.graph_expectation,
        "fallback_policy": task_profile.fallback_policy,
        "role_budget": task_profile.role_budget,
    });
    let retrieval_plan_summary = retrieval_plan.summary_json();

    json!({
        "schema_version": 1,
        "contract": "candidate_sources_are_separate_and_only_graph_verified_paths_are_relation_proof",
        "current_phase_scope": "retrieval_architecture_clarity",
        "proof_status": proof_status,
        "graph_proof": graph_proof,
        "evidence_status": evidence_status,
        "graph_verification": graph_verification,
        "prompt_intent": prompt_intent,
        "prompt_seed_provenance": prompt_seed_provenance,
        "task_intent": task_intent.to_json(),
        "task_profile": task_profile_summary,
        "retrieval_plan": retrieval_plan_summary,
        "retrieval_plan_summary": retrieval_plan.summary_json(),
        "candidate_flow": [
            {
                "stage": "lifecycle_preflight",
                "status": "active_current",
                "proof_contract": "claimability_gate_before_context"
            },
            {
                "stage": "prompt_seed_extraction",
                "status": "active_current",
                "candidate_count": exact_seed_count,
                "proof_contract": "seeds_are_candidates_not_proof"
            },
            {
                "stage": "exact_seed_candidates",
                "status": if exact_seed_count > 0 { "active_current" } else { "no_exact_seed_candidates" },
                "candidate_count": exact_seed_count,
                "proof_contract": "exact_matches_still_require_graph_or_source_verification"
            },
            {
                "stage": "text_evidence_candidates",
                "status": text_stage_status,
                "candidate_count": fallback_evidence_count,
                "proof_contract": "text_evidence_is_source_text_existence_not_graph_relation_proof"
            },
            {
                "stage": "lexical_candidates",
                "status": lexical_stage_status,
                "candidate_count": follow_up_query_count,
                "proof_contract": "follow_up_queries_are_non_shell_candidate_queries"
            },
            {
                "stage": "binary_vector_candidates",
                "status": "inactive_post_mvp",
                "candidate_count": 0,
                "proof_contract": "vectors_suggest_candidates_not_proof"
            },
            {
                "stage": "nuance_rescue_candidates",
                "status": nuance_stage_status,
                "candidate_count": nuance_candidate_count,
                "proof_contract": "nuance_rescue_candidates_are_candidate_only_until_graph_or_source_verification"
            },
            {
                "stage": "graph_neighborhood_candidates",
                "status": graph_stage_status,
                "candidate_count_before_dedup": candidate_paths_before_dedup,
                "candidate_count_after_dedup": candidate_paths_after_dedup,
                "returned_proof_path_count": paths.len(),
                "proof_contract": "only_verified_graph_paths_are_relation_proof"
            },
            {
                "stage": "union_dedup_rank",
                "status": "active_current",
                "candidate_count": paths.len() + fallback_evidence_count + snippets.len(),
                "proof_contract": "mixed_candidates_remain_role_labeled"
            },
            {
                "stage": "exact_graph_source_verification",
                "status": graph_verification_status,
                "proof_status": proof_status,
                "graph_proof": graph_proof,
                "reason": graph_verification_reason,
                "proof_contract": "source_text_can_be_claimable_without_relation_proof"
            },
            {
                "stage": "compact_context_packet",
                "status": "active_current",
                "likely_file_count": likely_file_count,
                "follow_up_query_count": follow_up_query_count,
                "proof_contract": "agent_packet_must_remain_bounded_and_label_claimability"
            }
        ],
        "inactive_lanes": inactive_lanes
    })
}

pub(crate) fn context_pack_retrieval_explain_json(
    packet: &ContextPacket,
    candidate_set: &ContextAgentCandidateSet,
    retrieval_architecture: &Value,
    graph_verification: &Value,
    paths: &[Value],
    fallback_evidence: &[Value],
    snippets: &[Value],
    proof_status: &str,
    graph_proof: bool,
    evidence_status: &str,
    omitted_paths: usize,
    omitted_snippets: usize,
    omitted_fallback_evidence: usize,
    omitted_recommended_tests: usize,
    omitted_risks: usize,
) -> Value {
    let seed_provenance = packet
        .metadata
        .get("prompt_seed_provenance")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let seeds_extracted = seed_provenance
        .iter()
        .filter(|seed| seed.get("exactness").and_then(Value::as_str) != Some("ignored"))
        .cloned()
        .collect::<Vec<_>>();
    let ignored_seeds = seed_provenance
        .iter()
        .filter(|seed| seed.get("exactness").and_then(Value::as_str) == Some("ignored"))
        .cloned()
        .collect::<Vec<_>>();
    let candidate_counts_by_source =
        context_pack_candidate_counts_by_source_json(&candidate_set.candidates);
    let candidate_sources = candidate_counts_by_source
        .as_object()
        .map(|counts| counts.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let ranking_reasons = candidate_set
        .candidates
        .iter()
        .map(context_pack_candidate_ranking_explain_json)
        .collect::<Vec<_>>();
    let proof_path_ids = paths
        .iter()
        .filter_map(|path| {
            path.get("path_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    let no_proof_fallback_reason = graph_verification
        .get("proof_failure_reason")
        .cloned()
        .filter(|value| !value.is_null())
        .or_else(|| packet.metadata.get("proof_failure_reason").cloned())
        .unwrap_or(Value::Null);
    let nuance_rescue_trace = packet
        .metadata
        .get("nuance_rescue_trace")
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "schema_version": 1,
                "diagnostic_only": true,
                "nuance_rescue_enabled": false,
                "rescue_enabled": false,
                "rescue_candidate_count": 0,
                "rescue_candidate_sources": [],
                "candidate_cap": CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT,
                "ignored_generic_tokens": [],
                "rare_tokens": [],
                "identifier_tokens": [],
                "route_tokens": [],
                "config_tokens": [],
                "test_tokens": [],
                "route_config_test_tokens": [],
                "rescue_reasons": [],
                "candidates_accepted": {"count": 0, "items": []},
                "candidates_rejected": {"count": 0, "tokens": []}
            })
        });
    let no_proof_fallback_status = context_pack_no_proof_fallback_status(
        graph_proof,
        fallback_evidence.len(),
        snippets.len(),
        &no_proof_fallback_reason,
    );
    let final_nuance_selection =
        context_pack_final_nuance_selection_json(packet, &candidate_set.candidates);
    let traversal_telemetry = packet
        .metadata
        .get("traversal_telemetry")
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "schema_version": 1,
                "diagnostic_only": true,
                "measurement_scope": "context_pack_graph_verification",
                "status": "unavailable"
            })
        });
    let path_evidence_telemetry = packet
        .metadata
        .get("path_evidence_telemetry")
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "schema_version": 1,
                "diagnostic_only": true,
                "measurement_scope": "context_pack_stored_path_evidence",
                "status": "unavailable"
            })
        });
    let traversal_mode = traversal_telemetry
        .get("traversal_mode")
        .or_else(|| traversal_telemetry.get("traversal_policy"))
        .cloned()
        .unwrap_or_else(|| json!("unknown"));
    let relation_allowlist = traversal_telemetry
        .get("relation_allowlist")
        .or_else(|| traversal_telemetry.get("relation_modes"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    let source_role_filter = traversal_telemetry
        .get("source_role_filter")
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "applied": traversal_telemetry
                    .get("source_role_filters_applied")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "blocked_edges": traversal_telemetry
                    .get("source_role_blocked_edges")
                    .cloned()
                    .unwrap_or_else(|| json!(0))
            })
        });
    let candidate_sources_verified =
        context_agent_candidate_sources_verified(&candidate_set.candidates);
    let omitted_count = omitted_paths
        .saturating_add(omitted_snippets)
        .saturating_add(omitted_fallback_evidence)
        .saturating_add(omitted_recommended_tests)
        .saturating_add(omitted_risks)
        .saturating_add(candidate_set.omitted_count);

    let mut explain = serde_json::Map::new();
    explain.insert("schema_version".to_string(), json!(1));
    explain.insert("diagnostic_only".to_string(), json!(true));
    explain.insert(
        "trigger".to_string(),
        json!("context-pack --explain/--audit-json"),
    );
    explain.insert(
        "compact_default".to_string(),
        json!("retrieval_explain is omitted unless an explicit diagnostic flag is requested"),
    );
    explain.insert(
        "prompt_intent".to_string(),
        json!(packet
            .metadata
            .get("prompt_intent")
            .and_then(Value::as_str)
            .unwrap_or("unknown")),
    );
    explain.insert("seeds_extracted".to_string(), json!(seeds_extracted));
    explain.insert("ignored_seeds".to_string(), json!(ignored_seeds));
    explain.insert(
        "seed_hygiene".to_string(),
        packet
            .metadata
            .get("prompt_seed_hygiene")
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "schema_version": 1,
                    "diagnostic_only": true,
                    "status": "unavailable"
                })
            }),
    );
    explain.insert("candidate_sources".to_string(), json!(candidate_sources));
    explain.insert("traversal_mode".to_string(), traversal_mode);
    explain.insert("relation_allowlist".to_string(), relation_allowlist);
    explain.insert("source_role_filter".to_string(), source_role_filter);
    explain.insert(
        "candidate_sources_verified".to_string(),
        json!(candidate_sources_verified),
    );
    explain.insert(
        "path_evidence_lookup_time_ms".to_string(),
        path_evidence_telemetry
            .get("lookup_time_ms")
            .cloned()
            .unwrap_or(Value::Null),
    );
    explain.insert(
        "path_evidence_hydration_time_ms".to_string(),
        path_evidence_telemetry
            .get("hydration_time_ms")
            .cloned()
            .unwrap_or(Value::Null),
    );
    explain.insert("omitted_count".to_string(), json!(omitted_count));
    explain.insert(
        "candidate_counts_by_source".to_string(),
        candidate_counts_by_source,
    );
    explain.insert(
        "candidate_caps".to_string(),
        json!({
            "candidate_cap": CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT,
            "candidate_cap_policy": "exact_seeds_protected_then_no_proof_text_then_nuance_and_deterministic_rank",
            "candidate_total_count": candidate_set.total_count,
            "candidate_returned_count": candidate_set.candidates.len(),
            "candidate_omitted_count": candidate_set.omitted_count,
            "candidate_omitted_reason": if candidate_set.omitted_count > 0 {
                "candidate_cap_exceeded_after_exact_seed_text_priority"
            } else {
                "none"
            },
            "exact_seed_cap_override": candidate_set.exact_seed_cap_override
        }),
    );
    explain.insert(
        "candidates_omitted".to_string(),
        json!({
            "count": candidate_set.omitted_count,
            "reason": if candidate_set.omitted_count > 0 {
                "candidate_cap_exceeded_after_exact_seed_text_priority"
            } else {
                "none"
            },
            "items": candidate_set.omitted_candidates
        }),
    );
    explain.insert(
        "dedup_results".to_string(),
        json!({
            "candidate_path_count_before_dedup": context_pack_metadata_u64(packet, "candidate_path_count_before_dedup"),
            "candidate_path_count_after_dedup": context_pack_metadata_u64(packet, "candidate_path_count_after_dedup"),
            "candidate_path_count_after_filter": context_pack_metadata_u64(packet, "candidate_path_count_after_filter"),
            "candidate_path_count_after_truncate": context_pack_metadata_u64(packet, "candidate_path_count_after_truncate"),
            "retrieval_candidate_count_after_dedup": candidate_set.total_count,
            "retrieval_candidate_count_after_cap": candidate_set.candidates.len()
        }),
    );
    explain.insert("ranking_reasons".to_string(), json!(ranking_reasons));
    explain.insert(
        "graph_verification_attempts".to_string(),
        graph_verification.clone(),
    );
    explain.insert(
        "graph_verification_status".to_string(),
        json!(graph_verification
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")),
    );
    explain.insert("traversal_telemetry".to_string(), traversal_telemetry);
    explain.insert(
        "path_evidence_telemetry".to_string(),
        path_evidence_telemetry,
    );
    explain.insert("proof_paths_found".to_string(), json!(paths.len()));
    explain.insert("proof_path_ids".to_string(), json!(proof_path_ids));
    explain.insert("proof_status".to_string(), json!(proof_status));
    explain.insert("graph_proof".to_string(), json!(graph_proof));
    explain.insert("evidence_status".to_string(), json!(evidence_status));
    explain.insert(
        "vector_trace".to_string(),
        packet
            .metadata
            .get("vector_candidate_trace")
            .cloned()
            .unwrap_or(Value::Null),
    );
    explain.insert(
        "nuance_rescue_enabled".to_string(),
        json!(nuance_rescue_trace
            .get("nuance_rescue_enabled")
            .or_else(|| nuance_rescue_trace.get("rescue_enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false)),
    );
    for key in [
        "ignored_generic_tokens",
        "rare_tokens",
        "identifier_tokens",
        "route_tokens",
        "config_tokens",
        "test_tokens",
        "route_config_test_tokens",
        "rescue_candidate_sources",
        "rescue_reasons",
    ] {
        explain.insert(
            key.to_string(),
            nuance_rescue_trace
                .get(key)
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
    }
    explain.insert(
        "rescue_candidate_count".to_string(),
        nuance_rescue_trace
            .get("rescue_candidate_count")
            .cloned()
            .unwrap_or_else(|| json!(0)),
    );
    explain.insert(
        "overfetch_rerank_settings".to_string(),
        context_pack_overfetch_rerank_settings_json(packet),
    );
    explain.insert(
        "candidates_accepted".to_string(),
        final_nuance_selection
            .get("candidates_accepted")
            .cloned()
            .unwrap_or_else(|| json!({"count": 0, "items": []})),
    );
    explain.insert(
        "candidates_rejected".to_string(),
        final_nuance_selection
            .get("candidates_rejected")
            .cloned()
            .unwrap_or_else(|| json!({"count": 0, "items": []})),
    );
    explain.insert("nuance_rescue_trace".to_string(), nuance_rescue_trace);
    explain.insert(
        "no_proof_fallback_reason".to_string(),
        no_proof_fallback_reason,
    );
    explain.insert(
        "no_proof_fallback_status".to_string(),
        json!(no_proof_fallback_status),
    );
    explain.insert(
        "fallback_evidence_count".to_string(),
        json!(fallback_evidence.len()),
    );
    explain.insert("snippet_count".to_string(), json!(snippets.len()));
    explain.insert(
        "omitted_counts".to_string(),
        json!({
            "paths": omitted_paths,
            "snippets": omitted_snippets,
            "fallback_evidence": omitted_fallback_evidence,
            "recommended_tests": omitted_recommended_tests,
            "risks": omitted_risks,
            "candidates": candidate_set.omitted_count
        }),
    );
    explain.insert(
        "retrieval_architecture_candidate_flow".to_string(),
        retrieval_architecture
            .get("candidate_flow")
            .cloned()
            .unwrap_or_else(|| json!([])),
    );
    Value::Object(explain)
}

pub(crate) fn context_pack_retrieval_explain_budget_summary(
    retrieval_explain: &Value,
    full_explain_bytes: usize,
    max_output_bytes: usize,
) -> Value {
    let mut summary = json!({
        "schema_version": 1,
        "diagnostic_only": true,
        "budget_limited": true,
        "status": "summary_included_full_explain_omitted_by_explain_budget",
        "reason": "retrieval_explain is separated from normal evidence budget; full diagnostic payload did not fit under --max-output-bytes",
        "full_explain_bytes": full_explain_bytes,
        "max_output_bytes": max_output_bytes,
        "candidate_counts_by_source": retrieval_explain
            .get("candidate_counts_by_source")
            .cloned()
            .unwrap_or_else(|| json!({})),
        "proof_status": retrieval_explain
            .get("proof_status")
            .cloned()
            .unwrap_or(Value::Null),
        "graph_proof": retrieval_explain
            .get("graph_proof")
            .cloned()
            .unwrap_or(Value::Null),
        "seed_hygiene": retrieval_explain
            .get("seed_hygiene")
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "schema_version": 1,
                    "diagnostic_only": true,
                    "status": "unavailable"
                })
            }),
        "seeds_extracted": retrieval_explain
            .get("seeds_extracted")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "ignored_seeds": retrieval_explain
            .get("ignored_seeds")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "ranking_reasons": {
            "available_in_full_explain": true,
            "omitted_by_explain_debug_budget": true
        },
        "budget_decisions": [
            "normal agent-json evidence budget enforced before diagnostic explain attachment",
            "fallback snippets and likely files are not pruned to make room for full explain payload"
        ]
    });
    // Vector operability diagnostics stay visible in the bounded summary:
    // index/runtime status and candidate counts are how an agent tells "vector
    // lane is down" apart from "vector lane found nothing".
    if let Some(vector_trace) = retrieval_explain.get("vector_trace") {
        if let Some(object) = summary.as_object_mut() {
            let mut compact_vector_trace = json!({
                "diagnostic_only": true,
                "compacted": true,
                "vector_index_status": vector_trace.get("vector_index_status").cloned().unwrap_or(Value::Null),
                "vector_runtime_status": vector_trace.get("vector_runtime_status").cloned().unwrap_or(Value::Null),
                "vector_candidate_count": vector_trace.get("vector_candidate_count").cloned().unwrap_or(Value::Null),
                "graph_verification_status_for_vector_candidates": vector_trace
                    .get("graph_verification_status_for_vector_candidates")
                    .cloned()
                    .unwrap_or(Value::Null),
            });
            if let Some(stale_reason) = vector_trace.get("stale_missing_vector_index_reason") {
                if let Some(compact_object) = compact_vector_trace.as_object_mut() {
                    compact_object.insert(
                        "stale_missing_vector_index_reason".to_string(),
                        stale_reason.clone(),
                    );
                }
            }
            object.insert("vector_trace".to_string(), compact_vector_trace);
        }
    }
    summary
}

pub(crate) fn context_pack_seed_hygiene_summary_json(retrieval_explain: &Value) -> Option<Value> {
    let seed_hygiene = retrieval_explain.get("seed_hygiene")?;
    let accepted = seed_hygiene
        .get("accepted_exact_seeds")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("seed")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| item.as_str().map(str::to_string))
                })
                .take(8)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let ignored = seed_hygiene
        .get("ignored_prose_terms")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .take(12)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let rejected_count = seed_hygiene
        .get("rejected_exact_seed_candidates")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    Some(json!({
        "schema_version": 1,
        "diagnostic_only": true,
        "status": "available",
        "accepted_exact_seeds": accepted,
        "ignored_prose_terms": ignored,
        "rejected_exact_seed_candidate_count": rejected_count,
        "proof_contract": "seed hygiene is diagnostic provenance only; it does not create graph proof"
    }))
}

pub(crate) fn context_pack_no_proof_fallback_status(
    graph_proof: bool,
    fallback_evidence_count: usize,
    snippet_count: usize,
    no_proof_fallback_reason: &Value,
) -> &'static str {
    if graph_proof {
        "not_needed_graph_proof_found"
    } else if fallback_evidence_count > 0 || snippet_count > 0 {
        "active_no_graph_proof_text_fallback"
    } else if !no_proof_fallback_reason.is_null() {
        "no_graph_proof_no_fallback"
    } else {
        "not_evaluated"
    }
}

pub(crate) fn context_pack_overfetch_rerank_settings_json(packet: &ContextPacket) -> Value {
    let binary_overfetch_k = context_pack_metadata_u64(packet, "binary_overfetch_k");
    let rerank_input_count = context_pack_metadata_u64(packet, "rerank_input_count");
    let rerank_output_count = context_pack_metadata_u64(packet, "rerank_output_count");
    let present = !binary_overfetch_k.is_null()
        || !rerank_input_count.is_null()
        || !rerank_output_count.is_null();
    json!({
        "present": present,
        "binary_overfetch_k": binary_overfetch_k,
        "rerank_input_count": rerank_input_count,
        "rerank_output_count": rerank_output_count,
        "notes": packet
            .metadata
            .get("overfetch_rerank_notes")
            .cloned()
            .unwrap_or_else(|| json!([])),
    })
}

pub(crate) fn context_pack_final_nuance_selection_json(
    packet: &ContextPacket,
    final_candidates: &[Value],
) -> Value {
    let final_nuance_items = final_candidates
        .iter()
        .filter(|candidate| context_agent_candidate_has_source(candidate, "nuance_rescue"))
        .map(context_pack_nuance_trace_candidate_item)
        .collect::<Vec<_>>();
    let final_ids = final_candidates
        .iter()
        .map(context_agent_candidate_id)
        .collect::<BTreeSet<_>>();
    let branch_candidates = packet
        .metadata
        .get("nuance_rescue_candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let rejected_items = branch_candidates
        .iter()
        .filter(|candidate| !final_ids.contains(&context_agent_candidate_id(candidate)))
        .take(CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT)
        .map(|candidate| {
            let mut item = context_pack_nuance_trace_candidate_item(candidate);
            if let Some(object) = item.as_object_mut() {
                object.insert(
                    "rejection_reason".to_string(),
                    json!("not_returned_after_union_cap_or_dedup"),
                );
            }
            item
        })
        .collect::<Vec<_>>();
    json!({
        "candidates_accepted": {
            "count": final_nuance_items.len(),
            "items": final_nuance_items
        },
        "candidates_rejected": {
            "count": rejected_items.len(),
            "items": rejected_items
        }
    })
}

pub(crate) fn context_pack_candidate_counts_by_source_json(candidates: &[Value]) -> Value {
    let mut counts = BTreeMap::<String, usize>::new();
    for candidate in candidates {
        for source in context_agent_candidate_sources(candidate) {
            *counts.entry(source).or_default() += 1;
        }
    }
    json!(counts)
}

pub(crate) fn context_pack_candidate_sources_json(candidates: &[Value]) -> Value {
    let sources = candidates
        .iter()
        .flat_map(context_agent_candidate_sources)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    json!(sources)
}

pub(crate) fn context_pack_candidate_ranking_explain_json(candidate: &Value) -> Value {
    let candidate = context_pack_public_candidate_json(candidate);
    json!({
        "candidate_id": candidate.get("candidate_id").cloned().unwrap_or(Value::Null),
        "rank": candidate.get("rank").cloned().unwrap_or(Value::Null),
        "score": candidate.get("score").cloned().unwrap_or(Value::Null),
        "source_score": candidate.get("source_score").cloned().unwrap_or(Value::Null),
        "vector_score": candidate.get("vector_score").cloned().unwrap_or(Value::Null),
        "binary_score": candidate.get("binary_score").cloned().unwrap_or(Value::Null),
        "candidate_sources": candidate.get("candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "matched_seeds": candidate.get("matched_seeds").cloned().unwrap_or_else(|| json!([])),
        "matched_token": candidate.get("matched_token").cloned().unwrap_or(Value::Null),
        "matched_tokens": candidate.get("matched_tokens").cloned().unwrap_or_else(|| json!([])),
        "rescue_reason": candidate.get("rescue_reason").cloned().unwrap_or(Value::Null),
        "rescue_kinds": candidate.get("rescue_kinds").cloned().unwrap_or_else(|| json!([])),
        "reason": candidate.get("reason").cloned().unwrap_or(Value::Null),
        "ranking_features": candidate.get("ranking_features").cloned().unwrap_or_else(|| json!({})),
        "verification_status": candidate.get("verification_status").cloned().unwrap_or(Value::Null),
        "graph_verification_status": candidate.get("graph_verification_status").cloned().unwrap_or(Value::Null),
        "text_evidence_status": candidate.get("text_evidence_status").cloned().unwrap_or(Value::Null),
        "proof_status": candidate.get("proof_status").cloned().unwrap_or(Value::Null),
        "graph_proof": candidate.get("graph_proof").cloned().unwrap_or(Value::Null),
        "omitted": candidate.get("omitted").cloned().unwrap_or(Value::Null),
        "truncated": candidate.get("truncated").cloned().unwrap_or(Value::Null)
    })
}

pub(crate) fn context_pack_public_candidate_json(candidate: &Value) -> Value {
    let mut candidate = candidate.clone();
    let Some(object) = candidate.as_object_mut() else {
        return candidate;
    };
    if let Some(value) = object.get("matched_token").cloned() {
        object.insert(
            "matched_token".to_string(),
            context_pack_trace_safe_value(Some(&value)),
        );
    }
    for key in ["matched_tokens", "matched_seeds"] {
        if let Some(values) = object.get(key).and_then(Value::as_array).cloned() {
            object.insert(
                key.to_string(),
                json!(values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(context_pack_trace_safe_string)
                    .collect::<Vec<_>>()),
            );
        }
    }
    for key in ["reason", "rescue_reason"] {
        if let Some(value) = object.get(key).and_then(Value::as_str) {
            object.insert(key.to_string(), json!(context_pack_trace_safe_text(value)));
        }
    }
    candidate
}

pub(crate) fn context_pack_compact_candidate_json(candidate: &Value) -> Value {
    let mut value = json!({
        "candidate_id": candidate.get("candidate_id").cloned().unwrap_or(Value::Null),
        "candidate_source": candidate.get("candidate_source").cloned().unwrap_or(Value::Null),
        "candidate_sources": candidate.get("candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "path": candidate.get("path").cloned().unwrap_or(Value::Null),
        "entity_id": candidate.get("entity_id").cloned().unwrap_or(Value::Null),
        "span": candidate.get("span").cloned().unwrap_or_else(|| candidate.get("source_span").cloned().unwrap_or(Value::Null)),
        "evidence_role": candidate.get("evidence_role").cloned().unwrap_or(Value::Null),
        "matched_seeds": candidate.get("matched_seeds").cloned().unwrap_or_else(|| json!([])),
        "proof_status": candidate.get("proof_status").cloned().unwrap_or(Value::Null),
        "graph_proof": candidate.get("graph_proof").cloned().unwrap_or(Value::Null),
        "claimable": candidate.get("claimable").cloned().unwrap_or(Value::Null),
        "claimable_for_graph": candidate.get("claimable_for_graph").cloned().unwrap_or(Value::Null),
        "claimable_for_text": candidate.get("claimable_for_text").cloned().unwrap_or(Value::Null),
        "requires_graph_verification": candidate.get("requires_graph_verification").cloned().unwrap_or(Value::Null),
        "verification_status": candidate.get("verification_status").cloned().unwrap_or(Value::Null),
        "graph_verification_status": candidate.get("graph_verification_status").cloned().unwrap_or(Value::Null),
        "text_evidence_status": candidate.get("text_evidence_status").cloned().unwrap_or(Value::Null),
        "reason": candidate.get("reason").cloned().unwrap_or(Value::Null),
        "rank": candidate.get("rank").cloned().unwrap_or(Value::Null),
        "score": candidate.get("score").cloned().unwrap_or(Value::Null),
        "language_capability": candidate.get("language_capability").cloned().unwrap_or(Value::Null),
    });
    if let Some(object) = value.as_object_mut() {
        copy_context_candidate_degradation_fields(candidate, object);
    }
    value
}

pub(crate) fn context_pack_budget_candidate_json(candidate: &Value) -> Value {
    let mut value = context_pack_compact_candidate_json(candidate);
    if let Some(object) = value.as_object_mut() {
        object.remove("language_capability");
    }
    value
}

pub(crate) fn context_pack_candidate_values_with_compact_fallback(
    candidates: &[Value],
) -> (Vec<Value>, bool) {
    let full = candidates
        .iter()
        .map(context_pack_public_candidate_json)
        .collect::<Vec<_>>();
    let full_bytes = serde_json::to_vec(&full)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    if full_bytes <= 8192 {
        return (full, false);
    }
    (
        candidates
            .iter()
            .map(context_pack_compact_candidate_json)
            .collect(),
        true,
    )
}

pub(crate) fn context_pack_metadata_u64(packet: &ContextPacket, key: &str) -> Value {
    packet
        .metadata
        .get(key)
        .and_then(Value::as_u64)
        .map(Value::from)
        .unwrap_or(Value::Null)
}

pub(crate) fn context_pack_capability_metadata_json(db_path: &Path) -> Value {
    match SqliteGraphStore::open_read_only(db_path) {
        Ok(store) => query_capability_metadata_summary_json(&store),
        Err(error) => json!({
            "status": "unavailable",
            "error": error.to_string(),
            "not_graph_proof": true,
            "proof_boundary": "capability metadata summary unavailable; context-pack evidence must rely on per-evidence proof labels only",
        }),
    }
}

pub(crate) fn context_pack_language_capability_context_json(
    capability_metadata: &Value,
    packet: &ContextPacket,
    proof_status: &str,
    graph_proof: bool,
    evidence_status: &str,
    proof_strength: &str,
) -> Value {
    let summary = capability_metadata
        .get("summary")
        .cloned()
        .unwrap_or(Value::Null);
    let unknown_boundary_rows = capability_metadata
        .pointer("/summary/unknown_boundary_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let resolver_metadata_rows = capability_metadata
        .pointer("/summary/resolver_metadata_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let missing_provenance_rows = capability_metadata
        .pointer("/summary/exact_or_derived_rows_missing_provenance")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fallback_evidence_count = packet
        .metadata
        .get("fallback_evidence")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let micro_flow_summary = packet
        .metadata
        .get("micro_flow_packet_summary")
        .cloned()
        .unwrap_or_else(|| context_pack_micro_flow_handle_summary_json(&[]));
    let packet_language_registry = mvp4_local_flow_packet_language_registry_json();
    json!({
        "status": if capability_metadata.get("status").and_then(Value::as_str) == Some("ok") {
            "available"
        } else {
            "unavailable"
        },
        "capability_metadata_status": capability_metadata.get("status").cloned().unwrap_or_else(|| json!("unknown")),
        "rows_by_language": summary
            .get("rows_by_language")
            .cloned()
            .unwrap_or(Value::Null),
        "rows_by_capability_status": summary
            .get("rows_by_capability_status")
            .cloned()
            .unwrap_or(Value::Null),
        "proof_status": proof_status,
        "proof_strength": proof_strength,
        "graph_proof": graph_proof,
        "evidence_status": evidence_status,
        "resolver_metadata_rows": resolver_metadata_rows,
        "exact_or_derived_rows_missing_provenance": missing_provenance_rows,
        "unknown_boundary_rows": unknown_boundary_rows,
        "fallback_evidence_count": fallback_evidence_count,
        "evidence_ladder": [
            {
                "layer": "resolver_backed_graph_proof",
                "status": if graph_proof && resolver_metadata_rows > 0 { "available_when_path_provenance_matches" } else { "not_present_or_not_required" },
                "proof_boundary": "compiler/LSP/resolver facts require recorded resolver provenance before semantic exactness is claimable"
            },
            {
                "layer": "parser_graph_or_source_facts",
                "status": if graph_proof { "graph_path_found" } else { "parser_or_source_facts_require_fallback_labels" },
                "proof_boundary": "parser facts can support source-spanned facts; caller/callee proof still requires exact graph relation evidence"
            },
            {
                "layer": "source_text_evidence",
                "status": if fallback_evidence_count > 0 { "available" } else { "not_returned" },
                "proof_boundary": "text/source-navigation evidence is useful context but not graph relation proof"
            },
            {
                "layer": "unknown_dynamic_runtime_macro",
                "status": if unknown_boundary_rows > 0 { "present" } else { "none_in_bounded_metadata_summary" },
                "proof_boundary": "unknown, dynamic, runtime, macro, preprocessor, compiler_required, and lsp_required facts stay non-blocking/non-proof"
            }
        ],
        "local_flow_packet_boundary": {
            "active_languages": packet_language_registry["active_packet_languages"],
            "active_packet_languages": packet_language_registry["active_packet_languages"],
            "active_packet_language_count": packet_language_registry["active_packet_language_count"],
            "active_non_typescript_packet_languages": packet_language_registry["active_non_typescript_packet_languages"],
            "active_non_typescript_packet_language_count": packet_language_registry["active_non_typescript_packet_language_count"],
            "default_packet_query_language": packet_language_registry["default_packet_query_language"],
            "packet_language_registry": packet_language_registry,
            "typescript_packet_handles_preserved": packet_language_registry["typescript_packet_handles_preserved"],
            "inactive_packet_languages_seen": [],
            "inactive_packet_language_count": 0,
            "inactive_packet_support": MVP4_3_LOCAL_FLOW_PACKET_INACTIVE_SUPPORT,
            "inactive_packet_overclaim_count": 0,
            "active_non_typescript_packet_support": MVP4_3_LOCAL_FLOW_PACKET_REGISTRY_SCOPED_SUPPORT,
            "active_non_typescript_packet_overclaim_count": 0,
            "non_typescript_packet_languages_seen": packet_language_registry["active_non_typescript_packet_languages"],
            "non_typescript_packet_language_count": packet_language_registry["active_non_typescript_packet_language_count"],
            "non_typescript_packet_support": MVP4_3_LOCAL_FLOW_PACKET_REGISTRY_SCOPED_SUPPORT,
            "non_typescript_packet_overclaim_count": 0,
            "compatibility_aliases": {
                "non_typescript_packet_languages_seen": {
                    "deprecated": true,
                    "alias_of": "active_non_typescript_packet_languages"
                },
                "non_typescript_packet_language_count": {
                    "deprecated": true,
                    "alias_of": "active_non_typescript_packet_language_count"
                },
                "non_typescript_packet_support": {
                    "deprecated": true,
                    "alias_of": "active_non_typescript_packet_support"
                },
                "non_typescript_packet_overclaim_count": {
                    "deprecated": true,
                    "alias_of": "active_non_typescript_packet_overclaim_count"
                }
            },
            "micro_flow_packet_summary": micro_flow_summary,
            "handles_do_not_create_proof": true,
            "context_entry_command_activated": false
        },
        "compact_output_contract": {
            "evidence_first": true,
            "source_spans_required_for_claimable_facts": true,
            "capability_metadata_does_not_create_graph_proof": true,
            "candidate_vector_nuance_evidence_not_graph_proof": true
        }
    })
}

pub(crate) fn context_pack_source_language_capability_json(
    file: Option<&str>,
    evidence_kind: &str,
    proof_status: &str,
    graph_proof: bool,
    exactness: &str,
    source_role: &str,
) -> Value {
    let language = file
        .and_then(context_pack_frontend_from_repo_relative_path)
        .unwrap_or_else(|| "unknown".to_string());
    let proof_strength = if graph_proof {
        "graph_relation_proof"
    } else if evidence_kind == "text_evidence" {
        "text_evidence_non_graph"
    } else if evidence_kind == "source_navigation_evidence" {
        "source_navigation_evidence"
    } else if matches!(exactness, "exact" | "parser_verified") {
        "parser_source_fact"
    } else {
        "unknown_non_graph"
    };
    let claimable_as = if graph_proof {
        vec!["graph_relation_proof", "source_text_existence"]
    } else if evidence_kind == "text_evidence" {
        vec!["source_text_existence"]
    } else {
        Vec::<&str>::new()
    };
    json!({
        "language": language,
        "frontend": language,
        "source_role": source_role,
        "capability_flags": [evidence_kind],
        "capability_status": match evidence_kind {
            "text_evidence" => "source_text_evidence",
            "source_navigation_evidence" => "source_navigation_evidence",
            "graph_path" => "graph_relation_evidence",
            _ => "unknown",
        },
        "exactness": exactness,
        "resolver_status": "not_claimed_by_context_pack",
        "proof_status": proof_status,
        "proof_strength": proof_strength,
        "claimability": {
            "claimable": !claimable_as.is_empty(),
            "claimable_as": claimable_as,
            "not_claimable_as": if graph_proof { Vec::<&str>::new() } else { vec!["typed_graph_relation", "caller_callee_proof", "resolver_exactness"] },
            "graph_proof": graph_proof
        },
        "source_span_available": file.is_some(),
        "not_graph_proof": !graph_proof
    })
}

pub(crate) fn context_pack_frontend_from_repo_relative_path(path: &str) -> Option<String> {
    let language = detect_language(path)?.as_str();
    mvp4_3_local_micro_flow_packet_source_supported_for_source(
        language,
        path,
        MicroSourceRole::Production,
    )
    .then(|| language.to_string())
}

pub(crate) const CONTEXT_PACK_MICRO_FLOW_HANDLE_LIMIT: usize = 4;

pub(crate) fn attach_context_pack_micro_flow_handles(
    packet: &mut ContextPacket,
    db_path: &Path,
) -> Result<usize, String> {
    let store = SqliteGraphStore::open_read_only(db_path).map_err(|error| {
        format!(
            "failed to open local micro-flow packet DB {} read-only: {error}",
            db_path.display()
        )
    })?;
    let rows = load_context_pack_micro_flow_packet_rows(
        &store,
        packet,
        CONTEXT_PACK_MICRO_FLOW_HANDLE_LIMIT,
    )?;
    let handles = rows
        .iter()
        .map(context_pack_micro_flow_handle_json)
        .collect::<Vec<_>>();
    let summary = context_pack_micro_flow_handle_summary_json(&handles);
    packet
        .metadata
        .insert("micro_flow_handles".to_string(), json!(handles));
    packet
        .metadata
        .insert("micro_flow_packet_summary".to_string(), summary);
    packet.metadata.insert(
        "micro_flow_handle_policy".to_string(),
        json!({
            "compact_default_full_packet_body_inline": false,
            "compact_default_ordered_steps_inline": false,
            "exact_function_seeds_prioritized": true,
            "compact_first_handle_preserves_seed_priority": true,
            "packet_handles_do_not_create_proof": true,
            "context_entry_command_activated": false,
            "proof_boundary": "handles reference persisted local_flow_packets only; opening the audit handle is required for dict_v1 body or ordered_steps expansion"
        }),
    );
    Ok(rows.len())
}

fn load_context_pack_micro_flow_packet_rows(
    store: &SqliteGraphStore,
    packet: &ContextPacket,
    limit: usize,
) -> Result<Vec<LocalFlowPacketRow>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let active_languages = mvp4_3_local_micro_flow_packet_active_languages();
    if active_languages.is_empty() {
        return Ok(Vec::new());
    }
    let mut rows = Vec::<LocalFlowPacketRow>::new();
    let mut seen_packet_ids = BTreeSet::<String>::new();

    // Exact function/entity seeds are the narrowest user-selected evidence.
    // Preserve them ahead of broader file matches so the bounded collection and
    // the compact one-handle envelope cannot shed the requested packet.
    for function in context_pack_micro_flow_relevant_functions(packet) {
        for language in &active_languages {
            let query = LocalFlowPacketQueryOptions {
                limit,
                function_query: Some(function.clone()),
                language: Some((*language).to_string()),
                source_role: Some("production".to_string()),
                ..LocalFlowPacketQueryOptions::default()
            };
            let candidates = store
                .query_local_flow_packets(&query)
                .map_err(|error| error.to_string())?;
            if extend_context_pack_micro_flow_packet_rows(
                &mut rows,
                &mut seen_packet_ids,
                candidates,
                limit,
            ) {
                return Ok(rows);
            }
        }
    }
    for file in context_pack_micro_flow_relevant_files(packet) {
        for language in &active_languages {
            let query = LocalFlowPacketQueryOptions {
                limit,
                file_id: Some(file.clone()),
                language: Some((*language).to_string()),
                source_role: Some("production".to_string()),
                ..LocalFlowPacketQueryOptions::default()
            };
            let candidates = store
                .query_local_flow_packets(&query)
                .map_err(|error| error.to_string())?;
            if extend_context_pack_micro_flow_packet_rows(
                &mut rows,
                &mut seen_packet_ids,
                candidates,
                limit,
            ) {
                return Ok(rows);
            }
        }
    }
    Ok(rows)
}

pub(crate) fn extend_context_pack_micro_flow_packet_rows(
    rows: &mut Vec<LocalFlowPacketRow>,
    seen_packet_ids: &mut BTreeSet<String>,
    candidates: impl IntoIterator<Item = LocalFlowPacketRow>,
    limit: usize,
) -> bool {
    if rows.len() >= limit {
        return true;
    }
    for row in candidates {
        if seen_packet_ids.insert(row.packet_id.clone()) {
            rows.push(row);
        }
        if rows.len() >= limit {
            return true;
        }
    }
    false
}

fn context_pack_micro_flow_relevant_files(packet: &ContextPacket) -> Vec<String> {
    let mut files = BTreeSet::<String>::new();
    for path in &packet.verified_paths {
        for span in &path.source_spans {
            let normalized = context_planning_normalize_path(&span.repo_relative_path);
            if !normalized.is_empty() {
                files.insert(normalized);
            }
        }
    }
    for snippet in &packet.snippets {
        let normalized = context_planning_normalize_path(&snippet.file);
        if !normalized.is_empty() {
            files.insert(normalized);
        }
    }
    if let Some(likely_files) = packet
        .metadata
        .get("likely_files")
        .and_then(Value::as_array)
    {
        for file in likely_files.iter().filter_map(Value::as_str) {
            let normalized = context_planning_normalize_path(file);
            if !normalized.is_empty() {
                files.insert(normalized);
            }
        }
    }
    files
        .into_iter()
        .take(CONTEXT_PACK_MICRO_FLOW_HANDLE_LIMIT)
        .collect()
}

fn context_pack_micro_flow_relevant_functions(packet: &ContextPacket) -> Vec<String> {
    unique_limited_strings(
        packet
            .symbols
            .iter()
            .filter(|symbol| context_planning_symbol_allowed(symbol))
            .cloned(),
        CONTEXT_PACK_MICRO_FLOW_HANDLE_LIMIT,
    )
}

pub(crate) fn context_pack_micro_flow_handle_json(row: &LocalFlowPacketRow) -> Value {
    let cap_state: Value = serde_json::from_str(&row.cap_state_json).unwrap_or(Value::Null);
    let source_span_ids: Value =
        serde_json::from_str(&row.source_span_ids_json).unwrap_or_else(|_| json!([]));
    let packet_body: Value = serde_json::from_str(&row.packet_body).unwrap_or(Value::Null);
    let path_count = packet_body
        .get("paths")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let step_count = packet_body
        .get("paths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|path| path.get("steps").and_then(Value::as_array))
        .map(Vec::len)
        .sum::<usize>();
    json!({
        "handle_id": format!("micro-flow-handle:{}", row.packet_id),
        "packet_id": row.packet_id,
        "packet_kind": row.packet_kind,
        "function_identity": row.function_entity_id,
        "function_frame_micro_node_id": row.function_frame_micro_node_id,
        "file": row.file_id,
        "function_span": row.primary_source_span_id,
        "source_span_ids": source_span_ids,
        "proof_status": row.proof_status,
        "proof_strength": row.proof_strength,
        "source_role": row.source_role,
        "language": row.language,
        "relation_path_summary": {
            "encoding": row.encoding,
            "path_count": path_count,
            "step_count": step_count,
            "compact_body_bytes": row.compact_body_bytes,
            "audit_body_bytes": row.audit_body_bytes,
        },
        "unknown_count": cap_state.get("unknown_or_gap_count").or_else(|| cap_state.get("gap_count")).cloned().unwrap_or_else(|| json!(0)),
        "risk_count": cap_state.get("risk_count").cloned().unwrap_or_else(|| json!(0)),
        "omitted_count": row.omitted_count,
        "truncation_reason": cap_state.get("truncation_reason").cloned().unwrap_or(Value::Null),
        "expansion_handle": format!("audit.local_flow_packets.packet:{}", row.packet_id),
        "lifecycle": {
            "packet_status": row.packet_status,
            "claimability": row.claimability,
            "lifecycle_binding": row.lifecycle_binding,
        },
        "currentness": row.packet_status,
        "included_reason": "matched context-pack file/function seed against persisted local_flow_packets",
        "packet_body_inline": false,
        "ordered_steps_inline": false,
        "full_source_body_output": false,
        "handle_creates_proof": false,
        "proof_boundary": "handle summarizes persisted packet availability only; dict_v1 body and ordered_steps require explicit audit/explain expansion",
    })
}

pub(crate) fn context_pack_micro_flow_handle_summary_json(handles: &[Value]) -> Value {
    let mut proof_strength_counts = BTreeMap::<String, u64>::new();
    let mut proof_status_counts = BTreeMap::<String, u64>::new();
    let omitted_count = handles
        .iter()
        .map(|handle| {
            handle
                .get("omitted_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
        })
        .sum::<u64>();
    let unknown_count = handles
        .iter()
        .map(|handle| {
            handle
                .get("unknown_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
        })
        .sum::<u64>();
    for handle in handles {
        if let Some(proof_strength) = handle.get("proof_strength").and_then(Value::as_str) {
            *proof_strength_counts
                .entry(proof_strength.to_string())
                .or_default() += 1;
        }
        if let Some(proof_status) = handle.get("proof_status").and_then(Value::as_str) {
            *proof_status_counts
                .entry(proof_status.to_string())
                .or_default() += 1;
        }
    }
    json!({
        "handle_count": handles.len(),
        "proof_strength_counts": proof_strength_counts,
        "proof_status_counts": proof_status_counts,
        "unknown_count": unknown_count,
        "omitted_count": omitted_count,
        "compact_default_full_packet_body_inline": false,
        "compact_default_ordered_steps_inline": false,
        "explain_audit_expansion_available": !handles.is_empty(),
        "packet_handles_do_not_create_proof": true,
        "context_entry_command_activated": false,
        "full_source_body_output": false,
    })
}

pub(crate) fn context_pack_agent_json_response(
    options: &ContextPackOptions,
    packet: &ContextPacket,
    db_lifecycle_read: &Value,
    budgets: ContextPackBudgets,
    repo_root: &Path,
    db_path: &Path,
    timings: Value,
) -> Value {
    let lifecycle = compact_lifecycle_summary(db_lifecycle_read);
    let path_limit = budgets.max_returned_proof_paths;
    let snippet_limit = budgets.max_snippets;
    let max_output_bytes = options
        .max_output_bytes
        .unwrap_or(DEFAULT_CONTEXT_AGENT_MAX_OUTPUT_BYTES);

    let mut paths = packet
        .verified_paths
        .iter()
        .take(path_limit)
        .map(agent_context_path_json)
        .collect::<Vec<_>>();
    // Resolve any proof-path edge endpoints still shown as opaque entity ids
    // (freshly-walked fallback edges carry no resolved entity in their label) to
    // human-readable symbol names via a single bounded store lookup. No-op when
    // the DB cannot be opened (e.g. unit fixtures), so the proof boundary and
    // existing behavior are unchanged.
    resolve_context_path_edge_names(&mut paths, db_path);
    let all_fallback_evidence = packet
        .metadata
        .get("fallback_evidence")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let proof_path_available = packet
        .metadata
        .get("proof_path_available")
        .and_then(Value::as_bool)
        .unwrap_or(!packet.verified_paths.is_empty());
    let fallback_evidence = all_fallback_evidence
        .iter()
        .take(snippet_limit)
        .cloned()
        .collect::<Vec<_>>();
    let proof_status = packet
        .metadata
        .get("proof_status")
        .and_then(Value::as_str)
        .unwrap_or(if proof_path_available {
            "proof_path_found"
        } else {
            "unknown"
        });
    let graph_proof = packet
        .metadata
        .get("graph_proof")
        .and_then(Value::as_bool)
        .unwrap_or(proof_path_available);
    let evidence_status = packet
        .metadata
        .get("evidence_status")
        .and_then(Value::as_str)
        .unwrap_or(if proof_path_available {
            "proof_path_found"
        } else if fallback_evidence.is_empty() {
            "no_evidence_found"
        } else {
            "fallback_evidence_found"
        });
    let likely_files = packet
        .metadata
        .get("likely_files")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let follow_up_queries = packet
        .metadata
        .get("follow_up_queries")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let micro_flow_handles = packet
        .metadata
        .get("micro_flow_handles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let micro_flow_packet_summary = packet
        .metadata
        .get("micro_flow_packet_summary")
        .cloned()
        .unwrap_or_else(|| context_pack_micro_flow_handle_summary_json(&[]));
    let claimability = packet.metadata.get("claimability").cloned();
    let snippets = packet
        .snippets
        .iter()
        .take(snippet_limit)
        .map(|snippet| {
            agent_context_snippet_json(snippet, &packet.verified_paths, &fallback_evidence)
        })
        .collect::<Vec<_>>();
    let mut candidate_set = build_context_agent_retrieval_candidates(
        options,
        packet,
        &paths,
        &fallback_evidence,
        &snippets,
        db_lifecycle_read
            .get("claimable")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        proof_path_available,
    );
    annotate_context_agent_candidates_with_file_degradation(&mut candidate_set.candidates, db_path);
    let candidate_count = candidate_set.candidates.len();
    let candidate_total_count = candidate_set.total_count;
    let candidate_omitted_count = candidate_set.omitted_count;
    let candidate_exact_seed_cap_override = candidate_set.exact_seed_cap_override;
    let proof_strength = context_pack_response_proof_strength(
        graph_proof,
        &fallback_evidence,
        &snippets,
        &candidate_set,
    );
    let capability_metadata = context_pack_capability_metadata_json(db_path);
    let language_capability_context = context_pack_language_capability_context_json(
        &capability_metadata,
        packet,
        proof_status,
        graph_proof,
        evidence_status,
        proof_strength,
    );
    let staged_availability = staged_availability_for_packet(packet, db_lifecycle_read);
    let staged_fields = if options.explain {
        staged_availability_top_level_fields(&staged_availability)
    } else {
        staged_availability_compact_top_level_fields(&staged_availability)
    };
    let (candidate_values_vec, candidate_compacted) =
        context_pack_candidate_values_with_compact_fallback(&candidate_set.candidates);
    let candidate_values = Value::Array(candidate_values_vec.clone());
    let response_limits = json!({
        "paths": path_limit,
        "snippets": snippet_limit,
        "candidates": CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT,
        "fallback_evidence": snippet_limit,
        "recommended_tests": 12,
        "risks": 12,
        "max_output_bytes": max_output_bytes,
    });
    let graph_verification = context_pack_graph_verification_json(
        packet,
        proof_status,
        graph_proof,
        evidence_status,
        fallback_evidence.len(),
    );
    let retrieval_architecture = context_pack_retrieval_architecture_json(
        packet,
        &paths,
        &fallback_evidence,
        &snippets,
        proof_status,
        graph_proof,
        evidence_status,
        &follow_up_queries,
        &likely_files,
    );
    let recommended_tests = packet
        .recommended_tests
        .iter()
        .take(12)
        .cloned()
        .collect::<Vec<_>>();
    let risks = packet.risks.iter().take(12).cloned().collect::<Vec<_>>();

    let candidate_paths = packet
        .metadata
        .get("candidate_path_count_before_dedup")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(packet.verified_paths.len());
    let requested_spans = packet
        .metadata
        .get("requested_source_span_count")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(packet.snippets.len());
    let mut omitted_paths = candidate_paths.saturating_sub(paths.len());
    let mut omitted_snippets = requested_spans.saturating_sub(snippets.len());
    let mut omitted_fallback_evidence = all_fallback_evidence
        .len()
        .saturating_sub(fallback_evidence.len());
    let mut omitted_recommended_tests = packet.recommended_tests.len().saturating_sub(12);
    let mut omitted_risks = packet.risks.len().saturating_sub(12);
    let mut omitted_planning_packet = 0;
    let lifecycle_claimable = db_lifecycle_read
        .get("claimable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut planning_packet = context_pack_planning_packet_json(
        options,
        packet,
        &paths,
        &fallback_evidence,
        &snippets,
        lifecycle_claimable,
        proof_status,
        graph_proof,
        omitted_paths + omitted_snippets + omitted_fallback_evidence,
    );
    if let Some(object) = planning_packet.as_object_mut() {
        object.insert(
            "language_capability_context".to_string(),
            language_capability_context.clone(),
        );
    }
    let mut routing_packet = context_pack_routing_packet_json(
        options,
        packet,
        db_lifecycle_read,
        &planning_packet,
        &candidate_set,
        &paths,
        &fallback_evidence,
        &all_fallback_evidence,
        &snippets,
        lifecycle_claimable,
        proof_status,
        graph_proof,
        evidence_status,
        omitted_paths + omitted_snippets + omitted_fallback_evidence,
    );
    if let Some(object) = routing_packet.as_object_mut() {
        object.insert(
            "language_capability_context".to_string(),
            language_capability_context.clone(),
        );
    }
    let fallback_snippets = snippets
        .iter()
        .filter(|snippet| snippet.get("fallback_source").is_some())
        .cloned()
        .collect::<Vec<_>>();
    let selected_role_coverage =
        context_planning_selected_role_coverage(&fallback_evidence, &snippets, &likely_files);
    let omitted_by_dedup = packet
        .metadata
        .get("omitted_by_dedup")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let omitted_by_budget = omitted_paths + omitted_snippets + omitted_fallback_evidence;
    let evidence_budget_status = json!({
        "status": if omitted_by_budget == 0 { "within_budget" } else { "bounded_with_omissions" },
        "evidence_budget": budgets.max_candidate_paths,
        "snippet_budget": snippet_limit,
        "likely_files_budget": CONTEXT_PLANNING_PACKET_FILE_LIMIT,
        "fallback_evidence_returned": fallback_evidence.len(),
        "fallback_snippets_returned": fallback_snippets.len(),
        "likely_files_returned": likely_files.as_array().map(Vec::len).unwrap_or_default(),
        "omitted_by_budget": omitted_by_budget,
        "omitted_by_dedup": omitted_by_dedup,
    });
    let explain_budget_status = json!({
        "enabled": options.explain,
        "status": if options.explain { "pending_separate_budget_check" } else { "disabled" },
        "separate_from_evidence_budget": true,
    });
    let retrieval_explain = options.explain.then(|| {
        context_pack_retrieval_explain_json(
            packet,
            &candidate_set,
            &retrieval_architecture,
            &graph_verification,
            &paths,
            &fallback_evidence,
            &snippets,
            proof_status,
            graph_proof,
            evidence_status,
            omitted_paths,
            omitted_snippets,
            omitted_fallback_evidence,
            omitted_recommended_tests,
            omitted_risks,
        )
    });

    let mut response = json!({
        "schema_name": "context_pack_agent_json",
        "schema_version": AGENT_JSON_SCHEMA_VERSION,
        "status": "ok",
        "command": "context-pack",
        "repo": path_string(repo_root),
        "db": path_string(db_path),
        "output_mode": options.output_mode.as_str(),
        "task": packet.task,
        "mode": packet.mode,
        "lifecycle": lifecycle,
        "db_lifecycle_read": db_lifecycle_read,
        "claimable": lifecycle_claimable,
        "diagnostic_only": db_lifecycle_read.get("diagnostic_only").and_then(Value::as_bool).unwrap_or_else(|| {
            !lifecycle_claimable
        }),
        "critical_symbols": packet.symbols,
        "symbols": packet.symbols,
        "paths": paths,
        "proof_paths": Value::Null,
        "proof_path_available": proof_path_available,
        "proof_status": proof_status,
        "proof_strength": proof_strength,
        "graph_proof": graph_proof,
        "evidence_status": evidence_status,
        "graph_verification": graph_verification,
        "capability_metadata": capability_metadata,
        "language_capability_context": language_capability_context,
        "micro_flow_handles": micro_flow_handles,
        "micro_flow_packet_summary": micro_flow_packet_summary,
        "micro_flow_packet_proof_boundary": {
            "handles_do_not_create_proof": true,
            "full_packet_body_inline": false,
            "ordered_steps_inline": false,
            "context_entry_command_activated": false,
            "flow_proof_requires_opened_verified_packet": true
        },
        "proof_failure_reason": packet.metadata.get("proof_failure_reason").cloned().unwrap_or(Value::Null),
        "proof_path_count": packet.verified_paths.len(),
        "fallback_evidence": fallback_evidence,
        "fallback_evidence_count": all_fallback_evidence.len(),
        "likely_files": likely_files,
        "follow_up_queries": follow_up_queries,
        "snippets": snippets,
        "recommended_tests": recommended_tests,
        "risks": risks,
        "warnings": staged_warning_values(&staged_availability),
        "errors": [],
        "result_count": paths.len() + packet.metadata
            .get("fallback_evidence")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default(),
        "limit": path_limit,
        "omitted_count": 0,
        "timings": timings,
        "limits": response_limits,
    });

    if let Some(object) = response.as_object_mut() {
        if let Some(paths) = object.get("paths").cloned() {
            object.insert("proof_paths".to_string(), paths);
        }
        object.insert("retrieval_architecture".to_string(), retrieval_architecture);
        object.insert("planning_packet".to_string(), planning_packet);
        object.insert("routing_packet".to_string(), routing_packet);
        if options.explain {
            object.insert(
                "staged_availability".to_string(),
                staged_availability.clone(),
            );
        }
        object.insert("fallback_snippets".to_string(), json!(fallback_snippets));
        object.insert(
            "selected_role_coverage".to_string(),
            json!(selected_role_coverage),
        );
        object.insert("omitted_by_budget".to_string(), json!(omitted_by_budget));
        object.insert("omitted_by_dedup".to_string(), json!(omitted_by_dedup));
        object.insert("evidence_budget_status".to_string(), evidence_budget_status);
        object.insert("explain_budget_status".to_string(), explain_budget_status);
        object.insert("candidates".to_string(), json!([]));
        object.insert("candidate_count".to_string(), json!(0));
        object.insert(
            "candidate_sources".to_string(),
            context_pack_candidate_sources_json(&candidate_set.candidates),
        );
        object.insert(
            "candidate_source_counts".to_string(),
            context_pack_candidate_counts_by_source_json(&candidate_set.candidates),
        );
        object.insert(
            "candidate_total_count".to_string(),
            json!(candidate_total_count),
        );
        object.insert(
            "candidate_omitted_count".to_string(),
            json!(candidate_total_count),
        );
        object.insert(
            "candidate_omitted_reason".to_string(),
            json!(if candidate_total_count > 0 {
                "candidates_omitted_until_compact_candidate_payload_fits"
            } else {
                "none"
            }),
        );
        object.insert(
            "candidate_cap".to_string(),
            json!(CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT),
        );
        object.insert(
            "candidate_cap_policy".to_string(),
            json!("exact_seeds_protected_then_no_proof_text_then_nuance_and_deterministic_rank"),
        );
        object.insert(
            "candidate_payload_compacted".to_string(),
            json!(candidate_compacted),
        );
        object.insert(
            "candidate_exact_seed_cap_override".to_string(),
            json!(candidate_exact_seed_cap_override),
        );
        if let Some(claimability) = claimability {
            object.insert("claimability".to_string(), claimability);
        }
    }
    merge_json_object(&mut response, staged_fields);
    if let Some(claimability) = packet.metadata.get("claimability").cloned() {
        if let Some(object) = response.as_object_mut() {
            object.insert("claimability".to_string(), claimability);
        }
    }
    if let Some(object) = response.as_object_mut() {
        object.entry("claimability".to_string()).or_insert_with(|| {
            context_pack_top_level_claimability_json(lifecycle_claimable, graph_proof)
        });
    }
    add_agent_use_dirty_evidence_output_fields(
        &mut response,
        "agent-use.context-pack",
        options.explain,
    );

    let mut max_output_bytes_exceeded = false;
    let mut omitted_retrieval_architecture = 0;
    let mut omitted_routing_packet = 0;
    for _ in 0..8 {
        let omitted_by_size =
            enforce_context_agent_max_output_bytes(&mut response, max_output_bytes);
        omitted_retrieval_architecture += omitted_by_size.retrieval_architecture;
        omitted_paths += omitted_by_size.paths;
        omitted_snippets += omitted_by_size.snippets;
        omitted_fallback_evidence += omitted_by_size.fallback_evidence;
        omitted_recommended_tests += omitted_by_size.recommended_tests;
        omitted_risks += omitted_by_size.risks;
        omitted_planning_packet += omitted_by_size.planning_packet;
        omitted_routing_packet += omitted_by_size.routing_packet;
        max_output_bytes_exceeded |= omitted_by_size.max_output_bytes_exceeded;
        update_context_agent_truncation(
            &mut response,
            path_limit,
            omitted_retrieval_architecture,
            omitted_paths,
            omitted_snippets,
            omitted_fallback_evidence,
            omitted_recommended_tests,
            omitted_risks,
            omitted_planning_packet,
            omitted_routing_packet,
            max_output_bytes_exceeded,
        );
        if serialized_json_len(&response) <= max_output_bytes || max_output_bytes_exceeded {
            break;
        }
    }
    let final_max_output_bytes_exceeded =
        max_output_bytes_exceeded && serialized_json_len(&response) > max_output_bytes;
    update_context_agent_truncation(
        &mut response,
        path_limit,
        omitted_retrieval_architecture,
        omitted_paths,
        omitted_snippets,
        omitted_fallback_evidence,
        omitted_recommended_tests,
        omitted_risks,
        omitted_planning_packet,
        omitted_routing_packet,
        final_max_output_bytes_exceeded,
    );
    if candidate_count > 0 {
        let candidate_bytes = serialized_json_len(&candidate_values);
        if serialized_json_len(&response).saturating_add(candidate_bytes) > max_output_bytes
            && compact_context_agent_routing_packet(&mut response)
        {
            omitted_routing_packet += 1;
            update_context_agent_truncation(
                &mut response,
                path_limit,
                omitted_retrieval_architecture,
                omitted_paths,
                omitted_snippets,
                omitted_fallback_evidence,
                omitted_recommended_tests,
                omitted_risks,
                omitted_planning_packet,
                omitted_routing_packet,
                max_output_bytes_exceeded,
            );
        }
        let mut candidate_response = response.clone();
        if let Some(object) = candidate_response.as_object_mut() {
            object.insert("candidates".to_string(), candidate_values);
            object.insert("candidate_count".to_string(), json!(candidate_count));
            object.insert(
                "candidate_payload_compacted".to_string(),
                json!(candidate_compacted),
            );
            object.insert(
                "candidate_omitted_count".to_string(),
                json!(candidate_omitted_count),
            );
            object.insert(
                "candidate_omitted_reason".to_string(),
                json!(if candidate_omitted_count > 0 {
                    "candidate_cap_exceeded_after_exact_seed_text_priority"
                } else {
                    "none"
                }),
            );
        }
        if serialized_json_len(&candidate_response) <= max_output_bytes {
            response = candidate_response;
        } else {
            // Candidate rows are evidence-adjacent: under the evidence-first
            // budget contract they outrank routing/metadata boilerplate. When
            // even one candidate cannot fit, escalate routing reduction
            // (slim rows -> compact -> minimize -> handle compaction -> stub)
            // and retry before giving up on candidates entirely.
            let routing_reduction_steps: [fn(&mut Value) -> bool; 5] = [
                slim_context_agent_routing_row_boilerplate,
                compact_context_agent_routing_packet,
                compact_context_agent_routing_packet,
                compact_context_agent_micro_flow_handles,
                stub_context_agent_routing_packet,
            ];
            let mut next_reduction_step = 0;
            'candidate_fit: loop {
                for keep_count in (1..=candidate_values_vec.len()).rev() {
                    let mut compact_candidate_response = response.clone();
                    if let Some(object) = compact_candidate_response.as_object_mut() {
                        object.insert(
                            "candidates".to_string(),
                            Value::Array(
                                candidate_set
                                    .candidates
                                    .iter()
                                    .take(keep_count)
                                    .map(context_pack_budget_candidate_json)
                                    .collect(),
                            ),
                        );
                        object.insert("candidate_count".to_string(), json!(keep_count));
                        object.insert("candidate_payload_compacted".to_string(), json!(true));
                        object.insert(
                            "candidate_omitted_count".to_string(),
                            json!(candidate_total_count.saturating_sub(keep_count)),
                        );
                        object.insert(
                            "candidate_omitted_reason".to_string(),
                            json!("candidate_budget_compacted_to_fit_max_output_bytes"),
                        );
                    }
                    if serialized_json_len(&compact_candidate_response) <= max_output_bytes {
                        response = compact_candidate_response;
                        break 'candidate_fit;
                    }
                }
                let mut reduced = false;
                while next_reduction_step < routing_reduction_steps.len() {
                    let step = routing_reduction_steps[next_reduction_step];
                    next_reduction_step += 1;
                    if step(&mut response) {
                        reduced = true;
                        break;
                    }
                }
                if !reduced {
                    break 'candidate_fit;
                }
            }
        }
    }
    if let Some(retrieval_explain) = retrieval_explain {
        let mut explain_response = response.clone();
        let full_explain_bytes = serialized_json_len(&retrieval_explain);
        let seed_hygiene_summary = context_pack_seed_hygiene_summary_json(&retrieval_explain);
        if let Some(object) = response.as_object_mut() {
            if let Some(summary) = seed_hygiene_summary.clone() {
                object.insert("seed_hygiene_summary".to_string(), summary);
            }
        }
        if let Some(object) = explain_response.as_object_mut() {
            object.insert("retrieval_explain".to_string(), retrieval_explain.clone());
            if let Some(summary) = seed_hygiene_summary.clone() {
                object.insert("seed_hygiene_summary".to_string(), summary);
            }
            object.insert(
                "explain_budget_status".to_string(),
                json!({
                    "enabled": true,
                    "status": "full_explain_included",
                    "separate_from_evidence_budget": true,
                    "full_explain_bytes": full_explain_bytes,
                }),
            );
        }
        if serialized_json_len(&explain_response) <= max_output_bytes {
            response = explain_response;
        } else if let Some(object) = response.as_object_mut() {
            object.insert(
                "retrieval_explain".to_string(),
                context_pack_retrieval_explain_budget_summary(
                    &retrieval_explain,
                    full_explain_bytes,
                    max_output_bytes,
                ),
            );
            object.insert(
                "explain_budget_status".to_string(),
                json!({
                    "enabled": true,
                    "status": "summary_included_full_explain_omitted_by_explain_budget",
                    "separate_from_evidence_budget": true,
                    "full_explain_bytes": full_explain_bytes,
                }),
            );
        }
        if serialized_json_len(&response) > max_output_bytes {
            remove_context_agent_field(&mut response, "retrieval_architecture");
            if let Some(object) = response.as_object_mut() {
                object.insert(
                    "explain_budget_status".to_string(),
                    json!({
                        "enabled": true,
                        "status": "summary_included_full_explain_omitted_by_explain_budget",
                        "separate_from_evidence_budget": true,
                        "retrieval_architecture_omitted_for_explain_budget": true,
                        "full_explain_bytes": full_explain_bytes,
                    }),
                );
            }
        }
    } else if let Some(object) = response.as_object_mut() {
        object.insert(
            "explain_budget_status".to_string(),
            json!({
                "enabled": false,
                "status": "disabled",
                "separate_from_evidence_budget": true,
            }),
        );
    }
    attach_context_pack_patch_assist_packet(
        &mut response,
        &packet.task,
        &packet.mode,
        &staged_availability,
        max_output_bytes,
    );
    for _ in 0..4 {
        if serialized_json_len(&response) <= max_output_bytes {
            break;
        }
        let omitted_by_size =
            enforce_context_agent_max_output_bytes(&mut response, max_output_bytes);
        omitted_retrieval_architecture += omitted_by_size.retrieval_architecture;
        omitted_paths += omitted_by_size.paths;
        omitted_snippets += omitted_by_size.snippets;
        omitted_fallback_evidence += omitted_by_size.fallback_evidence;
        omitted_recommended_tests += omitted_by_size.recommended_tests;
        omitted_risks += omitted_by_size.risks;
        omitted_planning_packet += omitted_by_size.planning_packet;
        omitted_routing_packet += omitted_by_size.routing_packet;
        max_output_bytes_exceeded |= omitted_by_size.max_output_bytes_exceeded;
        if serialized_json_len(&response) <= max_output_bytes {
            break;
        }
        if compact_context_agent_patch_assist_packet(&mut response) {
            omitted_routing_packet += 1;
            continue;
        }
        break;
    }
    update_context_agent_truncation(
        &mut response,
        path_limit,
        omitted_retrieval_architecture,
        omitted_paths,
        omitted_snippets,
        omitted_fallback_evidence,
        omitted_recommended_tests,
        omitted_risks,
        omitted_planning_packet,
        omitted_routing_packet,
        max_output_bytes_exceeded,
    );
    response
}

#[derive(Debug, Clone)]
pub(crate) struct ContextPlanningEvidenceSource {
    id: String,
    file: String,
    symbol: String,
    text: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ContextPlanningFollowUpQuery {
    priority: u8,
    query_text: String,
    path_scope: String,
    reason: String,
    source_evidence_id: String,
    expected_signal: String,
    risk: String,
    max_results_hint: usize,
}

impl ContextPlanningFollowUpQuery {
    fn key(&self) -> String {
        format!(
            "{}\u{1f}{}",
            self.query_text.to_ascii_lowercase(),
            self.path_scope.to_ascii_lowercase()
        )
    }

    fn to_json(&self) -> Value {
        json!({
            "query_text": self.query_text,
            "path_scope": self.path_scope,
            "reason": self.reason,
            "source_evidence_id": self.source_evidence_id,
            "expected_signal": self.expected_signal,
            "risk": self.risk,
            "max_results_hint": self.max_results_hint,
        })
    }
}

pub(crate) fn context_pack_planning_packet_json(
    options: &ContextPackOptions,
    packet: &ContextPacket,
    paths: &[Value],
    fallback_evidence: &[Value],
    snippets: &[Value],
    lifecycle_claimable: bool,
    proof_status: &str,
    graph_proof: bool,
    upstream_omitted_count: usize,
) -> Value {
    let evidence_type =
        context_planning_evidence_type(paths, fallback_evidence, snippets, graph_proof);
    let likely_files = context_planning_likely_files(packet, paths, fallback_evidence, snippets);
    let likely_symbols = unique_limited_strings(
        packet
            .symbols
            .iter()
            .filter(|symbol| context_planning_symbol_allowed(symbol))
            .cloned(),
        CONTEXT_PLANNING_PACKET_SYMBOL_LIMIT,
    );
    let evidence_items = context_planning_evidence_items(paths, fallback_evidence, snippets);
    let follow_up_queries =
        context_planning_follow_up_queries(options, &likely_symbols, &evidence_items, snippets);
    let suggested_verification_commands = context_planning_verification_hints(&follow_up_queries);
    let likely_files_value = json!(likely_files.clone());
    let selected_role_coverage =
        context_planning_selected_role_coverage(fallback_evidence, snippets, &likely_files_value);
    let micro_flow_handles = packet
        .metadata
        .get("micro_flow_handles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let micro_flow_packet_summary = packet
        .metadata
        .get("micro_flow_packet_summary")
        .cloned()
        .unwrap_or_else(|| context_pack_micro_flow_handle_summary_json(&[]));
    let fallback_snippets = snippets
        .iter()
        .filter(|snippet| snippet.get("fallback_source").is_some())
        .cloned()
        .collect::<Vec<_>>();
    let claimability =
        context_planning_claimability_json(&evidence_type, lifecycle_claimable, graph_proof);
    let claimable = claimability
        .get("claimable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let confidence = match evidence_type.as_str() {
        "graph_proof" => 0.95,
        "text_evidence" => 0.72,
        "fallback" => 0.50,
        _ => 0.0,
    };
    let mut unknowns = Vec::new();
    if !graph_proof {
        unknowns.push("no_graph_proof_path_found".to_string());
    }
    if evidence_type == "unknown" {
        unknowns.push("no_evidence_found".to_string());
    }
    if follow_up_queries.is_empty() {
        unknowns.push("no_follow_up_query_found".to_string());
    }
    let mut warnings = Vec::new();
    if evidence_type != "graph_proof" {
        warnings.push("planning_packet_is_not_graph_proof".to_string());
    }
    if follow_up_queries.is_empty() {
        warnings.push("no_follow_up_query_found".to_string());
    }
    warnings.push("suggested_verification_commands_are_non_shell_hints".to_string());

    let evidence_candidate_count = paths.len() + fallback_evidence.len() + snippets.len();
    let planning_omitted_count = upstream_omitted_count
        + evidence_candidate_count.saturating_sub(evidence_items.len())
        + likely_files
            .len()
            .saturating_sub(CONTEXT_PLANNING_PACKET_FILE_LIMIT);
    let omitted_by_dedup = packet
        .metadata
        .get("omitted_by_dedup")
        .and_then(Value::as_u64)
        .unwrap_or_default();

    json!({
        "task": packet.task,
        "mode": packet.mode,
        "claimable": claimable,
        "claimability": claimability,
        "proof_status": proof_status,
        "graph_proof": graph_proof,
        "likely_files": likely_files,
        "fallback_snippets": fallback_snippets,
        "selected_role_coverage": selected_role_coverage,
        "likely_symbols": likely_symbols,
        "evidence_items": evidence_items,
        "micro_flow_handles": micro_flow_handles,
        "micro_flow_packet_summary": micro_flow_packet_summary,
        "micro_flow_packet_policy": {
            "handles_do_not_create_proof": true,
            "compact_default_full_packet_body_inline": false,
            "compact_default_ordered_steps_inline": false,
            "expansion_handle_required_for_packet_body": true
        },
        "evidence_type": evidence_type,
        "confidence": confidence,
        "rank": if evidence_type == "unknown" { 0 } else { 1 },
        "unknowns": unknowns,
        "follow_up_queries": follow_up_queries,
        "suggested_verification_commands": suggested_verification_commands,
        "do_not_touch_areas": context_planning_do_not_touch_areas(&likely_files),
        "omitted_count": planning_omitted_count,
        "omitted_by_budget": planning_omitted_count,
        "omitted_by_dedup": omitted_by_dedup,
        "evidence_budget_status": {
            "status": if planning_omitted_count == 0 { "within_budget" } else { "bounded_with_omissions" },
            "evidence_budget": CONTEXT_PLANNING_PACKET_EVIDENCE_LIMIT,
            "snippet_budget": snippets.len(),
            "likely_files_budget": CONTEXT_PLANNING_PACKET_FILE_LIMIT,
            "follow_up_queries_budget": CONTEXT_PLANNING_PACKET_QUERY_LIMIT,
            "omitted_by_budget": planning_omitted_count,
            "omitted_by_dedup": omitted_by_dedup,
        },
        "packet_budget_status": {
            "status": if planning_omitted_count == 0 { "within_budget" } else { "bounded_with_omissions" },
            "role_diversity_applied": true,
        },
        "warnings": warnings,
    })
}

pub(crate) fn context_planning_evidence_type(
    paths: &[Value],
    fallback_evidence: &[Value],
    snippets: &[Value],
    graph_proof: bool,
) -> String {
    if graph_proof && !paths.is_empty() {
        return "graph_proof".to_string();
    }
    if fallback_evidence.iter().any(|evidence| {
        evidence.get("evidence_role").and_then(Value::as_str) == Some("text_evidence")
    }) || snippets.iter().any(|snippet| {
        snippet.get("evidence_role").and_then(Value::as_str) == Some("text_evidence")
    }) {
        return "text_evidence".to_string();
    }
    if !fallback_evidence.is_empty() || !snippets.is_empty() {
        return "fallback".to_string();
    }
    "unknown".to_string()
}

pub(crate) fn context_planning_selected_role_coverage(
    fallback_evidence: &[Value],
    snippets: &[Value],
    likely_files: &Value,
) -> Vec<Value> {
    let mut roles = BTreeMap::<String, usize>::new();
    for value in fallback_evidence.iter().chain(snippets.iter()) {
        let role = context_planning_role_for_value(value);
        if role != "unknown" {
            *roles.entry(role.to_string()).or_default() += 1;
        }
    }
    if let Some(files) = likely_files.as_array() {
        for file in files.iter().filter_map(Value::as_str) {
            let role = context_planning_role_for_text_evidence(file, file, "file", "");
            if role != "unknown" {
                *roles.entry(role.to_string()).or_default() += 1;
            }
        }
    }
    let mut coverage = roles
        .into_iter()
        .map(|(role, count)| {
            json!({
                "role": role,
                "selected_count": count,
            })
        })
        .collect::<Vec<_>>();
    coverage.sort_by(|left, right| {
        context_planning_role_rank(
            left.get("role")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
        )
        .cmp(&context_planning_role_rank(
            right
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
        ))
    });
    coverage
}

pub(crate) fn context_planning_likely_files(
    packet: &ContextPacket,
    paths: &[Value],
    fallback_evidence: &[Value],
    snippets: &[Value],
) -> Vec<String> {
    let metadata_files = packet
        .metadata
        .get("likely_files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string);
    let path_files = paths.iter().filter_map(context_planning_first_path_file);
    let fallback_files = fallback_evidence
        .iter()
        .filter_map(|evidence| context_planning_value_string(evidence, "file"));
    let snippet_files = snippets
        .iter()
        .filter_map(|snippet| context_planning_value_string(snippet, "file"));
    unique_limited_strings(
        metadata_files
            .chain(path_files)
            .chain(fallback_files)
            .chain(snippet_files),
        CONTEXT_PLANNING_PACKET_FILE_LIMIT,
    )
}

pub(crate) fn context_planning_evidence_items(
    paths: &[Value],
    fallback_evidence: &[Value],
    snippets: &[Value],
) -> Vec<Value> {
    let mut items = Vec::new();
    let graph_path_item_limit = if fallback_evidence.is_empty() && snippets.is_empty() {
        CONTEXT_PLANNING_PACKET_EVIDENCE_LIMIT.min(2)
    } else {
        CONTEXT_PLANNING_PACKET_EVIDENCE_LIMIT
    };
    for path in paths.iter().take(graph_path_item_limit) {
        if items.len() >= CONTEXT_PLANNING_PACKET_EVIDENCE_LIMIT {
            break;
        }
        let id = context_planning_value_string(path, "path_id")
            .unwrap_or_else(|| format!("proof-path-{}", items.len() + 1));
        let source_spans = path
            .get("source_spans")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let file = source_spans
            .first()
            .and_then(|span| context_planning_value_string(span, "file"));
        let mut item = serde_json::Map::new();
        item.insert("id".to_string(), json!(id));
        item.insert("rank".to_string(), json!(items.len() + 1));
        item.insert("evidence_type".to_string(), json!("graph_proof"));
        item.insert(
            "evidence_role".to_string(),
            json!(path
                .get("evidence_role")
                .and_then(Value::as_str)
                .unwrap_or("unknown")),
        );
        item.insert("planning_role".to_string(), json!("unknown"));
        item.insert(
            "role_rank_reason".to_string(),
            json!("graph proof path ranked by existing graph evidence"),
        );
        item.insert("proof_status".to_string(), json!("proof_path_found"));
        item.insert("graph_proof".to_string(), json!(true));
        item.insert("confidence".to_string(), json!(0.95));
        if let Some(file) = file {
            item.insert("file".to_string(), json!(file));
        }
        if let Some(source_span) = source_spans.first().cloned() {
            item.insert("source_span".to_string(), source_span);
        }
        if let Some(relations) = path.get("relations").cloned() {
            item.insert("relations".to_string(), relations);
        }
        if let Some(reason) = path.get("classification_reason").cloned() {
            item.insert("reason".to_string(), reason);
        }
        if let Some(capability) = path.get("language_capability").cloned() {
            item.insert("language_capability".to_string(), capability);
        }
        items.push(Value::Object(item));
    }

    for evidence in fallback_evidence {
        if items.len() >= CONTEXT_PLANNING_PACKET_EVIDENCE_LIMIT {
            break;
        }
        let evidence_role = evidence
            .get("evidence_role")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let evidence_type = if evidence_role == "text_evidence" {
            "text_evidence"
        } else {
            "fallback"
        };
        let mut item = serde_json::Map::new();
        item.insert(
            "id".to_string(),
            json!(context_planning_value_string(evidence, "id")
                .unwrap_or_else(|| format!("fallback-evidence-{}", items.len() + 1))),
        );
        item.insert("rank".to_string(), json!(items.len() + 1));
        item.insert("evidence_type".to_string(), json!(evidence_type));
        item.insert("evidence_role".to_string(), json!(evidence_role));
        item.insert(
            "planning_role".to_string(),
            json!(context_planning_role_for_value(evidence)),
        );
        if let Some(reason) = evidence.get("role_rank_reason").cloned() {
            item.insert("role_rank_reason".to_string(), reason);
        }
        item.insert(
            "proof_status".to_string(),
            json!(evidence
                .get("proof_status")
                .and_then(Value::as_str)
                .unwrap_or("no_proof_path_found")),
        );
        item.insert(
            "graph_proof".to_string(),
            json!(evidence
                .get("graph_proof")
                .and_then(Value::as_bool)
                .unwrap_or(false)),
        );
        item.insert(
            "confidence".to_string(),
            json!(if evidence_type == "text_evidence" {
                0.72
            } else {
                0.50
            }),
        );
        for key in ["file", "symbol"] {
            if let Some(value) = evidence.get(key).cloned() {
                item.insert(key.to_string(), value);
            }
        }
        for key in ["source_span"] {
            if let Some(value) = evidence.get(key).cloned() {
                item.insert(key.to_string(), value);
            }
        }
        if let Some(capability) = evidence.get("language_capability").cloned() {
            item.insert("language_capability".to_string(), capability);
        }
        items.push(Value::Object(item));
    }

    for snippet in snippets {
        if items.len() >= CONTEXT_PLANNING_PACKET_EVIDENCE_LIMIT {
            break;
        }
        let Some(file) = context_planning_value_string(snippet, "file") else {
            continue;
        };
        let evidence_role = snippet
            .get("evidence_role")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let evidence_type = if evidence_role == "text_evidence" {
            "text_evidence"
        } else {
            "fallback"
        };
        let planning_role = context_planning_role_for_value(snippet);
        let mut item = json!({
            "id": format!("snippet://{}:{}", file, snippet.get("lines").and_then(Value::as_str).unwrap_or("unknown")),
            "rank": items.len() + 1,
            "evidence_type": evidence_type,
            "evidence_role": evidence_role,
            "planning_role": planning_role,
            "role_rank_reason": snippet.get("role_rank_reason").and_then(Value::as_str).unwrap_or("snippet retained under snippet budget"),
            "proof_status": snippet.get("proof_status").and_then(Value::as_str).unwrap_or("not_graph_proof"),
            "graph_proof": snippet.get("graph_proof").and_then(Value::as_bool).unwrap_or(false),
            "confidence": if evidence_type == "text_evidence" { 0.68 } else { 0.45 },
            "file": file,
            "lines": snippet.get("lines").cloned().unwrap_or_else(|| json!("unknown")),
            "text_preview": snippet.get("text").and_then(Value::as_str).unwrap_or_default().chars().take(160).collect::<String>(),
            "reason": snippet.get("reason").and_then(Value::as_str).unwrap_or("context snippet"),
        });
        if let Some(object) = item.as_object_mut() {
            if let Some(capability) = snippet.get("language_capability").cloned() {
                object.insert("language_capability".to_string(), capability);
            }
        }
        items.push(item);
    }
    items
}

pub(crate) fn context_planning_follow_up_queries(
    options: &ContextPackOptions,
    likely_symbols: &[String],
    evidence_items: &[Value],
    snippets: &[Value],
) -> Vec<Value> {
    let evidence_sources = context_planning_evidence_sources(evidence_items, snippets);
    if evidence_sources.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    let task_lower = options.task.to_ascii_lowercase();
    for source in &evidence_sources {
        let normalized_file = context_planning_normalize_path(&source.file);
        let file_lower = normalized_file.to_ascii_lowercase();
        let combined_text = format!("{}\n{}\n{}", source.file, source.symbol, source.text);
        let lower = combined_text.to_ascii_lowercase();
        for token in context_planning_br2_tokens(&combined_text) {
            context_planning_push_query(
                &mut candidates,
                10,
                token,
                "package/*/Config.in",
                format!("matched Buildroot config token in {}", source.file),
                &source.id,
                "Config option definition or references",
                "precise",
                10,
            );
        }
        if lower.contains("generic-package") {
            context_planning_push_query(
                &mut candidates,
                20,
                "generic-package",
                "package/",
                format!("matched package infrastructure token in {}", source.file),
                &source.id,
                ".mk package infrastructure usage",
                "precise",
                10,
            );
        }
        if lower.contains("host-generic-package") {
            context_planning_push_query(
                &mut candidates,
                21,
                "host-generic-package",
                "package/",
                format!(
                    "matched host package infrastructure token in {}",
                    source.file
                ),
                &source.id,
                "host .mk package infrastructure usage",
                "precise",
                10,
            );
        }

        let config_surface = file_lower.ends_with("config.in")
            || lower.contains("config.in")
            || lower.contains("br2_package_");
        if config_surface && (lower.contains("depends on") || task_lower.contains("depends on")) {
            context_planning_push_query(
                &mut candidates,
                30,
                "depends on",
                "package/*/Config.in",
                format!("matched Kconfig dependency surface in {}", source.file),
                &source.id,
                "Kconfig dependency constraints",
                "precise",
                10,
            );
        }
        if config_surface && (lower.contains("select") || task_lower.contains("select")) {
            context_planning_push_query(
                &mut candidates,
                31,
                "select",
                "package/*/Config.in",
                format!("matched Kconfig select surface in {}", source.file),
                &source.id,
                "Kconfig selected symbols",
                "precise",
                10,
            );
        }

        let package_metadata_surface = file_lower.ends_with(".mk")
            || lower.contains(".mk")
            || lower.contains("_license")
            || lower.contains("_version")
            || lower.contains("_dependencies");
        if package_metadata_surface {
            context_planning_push_query(
                &mut candidates,
                40,
                "license version dependencies",
                "package/",
                format!("matched package metadata surface in {}", source.file),
                &source.id,
                "Makefile metadata assignments such as *_LICENSE, *_VERSION, *_DEPENDENCIES",
                "broad",
                10,
            );
        }
        if lower.contains("package infrastructure") {
            let scope = if file_lower.contains("docs/") {
                "docs/"
            } else {
                "*"
            };
            context_planning_push_query(
                &mut candidates,
                50,
                "package infrastructure",
                scope,
                format!(
                    "matched documentation/build-system phrase in {}",
                    source.file
                ),
                &source.id,
                "documentation describing package infrastructure",
                "precise",
                10,
            );
        }
        if file_lower.starts_with("support/scripts/") {
            if let Some(script_name) = context_planning_basename(&normalized_file) {
                context_planning_push_query(
                    &mut candidates,
                    60,
                    script_name,
                    "support/scripts/",
                    format!("matched support script path {}", source.file),
                    &source.id,
                    "support script name or references",
                    "precise",
                    10,
                );
            }
        }
        if file_lower.starts_with("support/download/") {
            if let Some(script_name) = context_planning_basename(&normalized_file) {
                context_planning_push_query(
                    &mut candidates,
                    61,
                    script_name,
                    "support/download/",
                    format!("matched download support path {}", source.file),
                    &source.id,
                    "download wrapper or backend support script",
                    "precise",
                    10,
                );
            }
        }
        if file_lower.ends_with(".adoc") && !lower.contains("package infrastructure") {
            context_planning_push_query(
                &mut candidates,
                70,
                ".adoc",
                "docs/",
                format!("matched AsciiDoc planning file {}", source.file),
                &source.id,
                "documentation files",
                "noisy",
                10,
            );
        }
        if file_lower.ends_with(".mk") {
            context_planning_push_query(
                &mut candidates,
                71,
                ".mk",
                "package/",
                format!("matched package makefile {}", source.file),
                &source.id,
                "package makefiles",
                "noisy",
                10,
            );
        }
    }

    if context_pack_is_buildroot_package_task(&options.task) {
        context_planning_push_query(
            &mut candidates,
            22,
            "package/Config.in",
            "package/",
            "broad Buildroot package task needs top-level package menu wiring".to_string(),
            "task://buildroot-package-planning",
            "top-level package Config.in inclusion surface",
            "precise",
            10,
        );
        context_planning_push_query(
            &mut candidates,
            23,
            "pkg-download",
            "package/",
            "broad Buildroot package task needs download infrastructure".to_string(),
            "task://buildroot-package-planning",
            "download infrastructure makefile",
            "precise",
            10,
        );
        context_planning_push_query(
            &mut candidates,
            24,
            "dl-wrapper",
            "support/download/",
            "broad Buildroot package task needs download support wrappers".to_string(),
            "task://buildroot-package-planning",
            "download wrapper or backend support script",
            "precise",
            10,
        );
        context_planning_push_query(
            &mut candidates,
            26,
            "license version dependencies",
            "package/",
            "broad Buildroot package task needs package metadata assignments".to_string(),
            "task://buildroot-package-planning",
            "Makefile metadata assignments such as *_LICENSE, *_VERSION, *_DEPENDENCIES",
            "broad",
            10,
        );
    }

    if !evidence_items.is_empty() {
        for symbol in likely_symbols {
            if !context_planning_symbol_allowed(symbol) {
                continue;
            }
            context_planning_push_query(
                &mut candidates,
                90,
                symbol.clone(),
                "*",
                "derived from graph/text seed".to_string(),
                &format!("seed://{symbol}"),
                "symbol, path, or text references",
                "precise",
                10,
            );
        }
    }

    let mut deduped = BTreeMap::<String, ContextPlanningFollowUpQuery>::new();
    for candidate in candidates {
        let key = candidate.key();
        let replace = deduped
            .get(&key)
            .is_none_or(|existing| candidate.priority < existing.priority);
        if replace {
            deduped.insert(key, candidate);
        }
    }
    let mut queries = deduped.into_values().collect::<Vec<_>>();
    queries.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.query_text.cmp(&right.query_text))
            .then_with(|| left.path_scope.cmp(&right.path_scope))
    });
    queries
        .into_iter()
        .take(CONTEXT_PLANNING_PACKET_QUERY_LIMIT)
        .map(|query| query.to_json())
        .collect()
}

pub(crate) fn context_planning_evidence_sources(
    evidence_items: &[Value],
    snippets: &[Value],
) -> Vec<ContextPlanningEvidenceSource> {
    let mut sources = Vec::new();
    let mut seen = BTreeSet::new();
    for item in evidence_items {
        let id = context_planning_value_string(item, "id")
            .unwrap_or_else(|| format!("evidence-item-{}", sources.len() + 1));
        let file = context_planning_value_string(item, "file")
            .or_else(|| {
                item.get("source_span")
                    .and_then(|span| context_planning_value_string(span, "file"))
            })
            .unwrap_or_default();
        let symbol = context_planning_value_string(item, "symbol").unwrap_or_else(|| file.clone());
        let mut text_parts = Vec::new();
        for key in ["symbol", "kind", "reason", "snippet", "text_preview"] {
            if let Some(value) = item.get(key).and_then(Value::as_str) {
                text_parts.push(value.to_string());
            }
        }
        for key in ["seed_matches", "follow_up_queries", "relations"] {
            if let Some(values) = item.get(key).and_then(Value::as_array) {
                text_parts.extend(values.iter().filter_map(Value::as_str).map(str::to_string));
            }
        }
        let text = text_parts.join("\n");
        let key = format!("{id}\u{1f}{file}");
        if seen.insert(key) {
            sources.push(ContextPlanningEvidenceSource {
                id,
                file,
                symbol,
                text,
            });
        }
    }
    for snippet in snippets {
        let Some(file) = context_planning_value_string(snippet, "file") else {
            continue;
        };
        let id = format!(
            "snippet://{}:{}",
            file,
            snippet
                .get("lines")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
        let key = format!("{id}\u{1f}{file}");
        if !seen.insert(key) {
            continue;
        }
        let text = [
            snippet
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            snippet
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ]
        .join("\n");
        sources.push(ContextPlanningEvidenceSource {
            id,
            file: file.clone(),
            symbol: file.clone(),
            text,
        });
    }
    sources
}

pub(crate) fn context_planning_push_query(
    candidates: &mut Vec<ContextPlanningFollowUpQuery>,
    priority: u8,
    query_text: impl Into<String>,
    path_scope: impl Into<String>,
    reason: impl Into<String>,
    source_evidence_id: &str,
    expected_signal: impl Into<String>,
    risk: &'static str,
    max_results_hint: usize,
) {
    let query_text = query_text.into();
    if query_text.trim().is_empty() {
        return;
    }
    candidates.push(ContextPlanningFollowUpQuery {
        priority,
        query_text,
        path_scope: path_scope.into(),
        reason: reason.into(),
        source_evidence_id: source_evidence_id.to_string(),
        expected_signal: expected_signal.into(),
        risk: risk.to_string(),
        max_results_hint,
    });
}

pub(crate) fn context_planning_verification_hints(follow_up_queries: &[Value]) -> Vec<Value> {
    follow_up_queries
        .iter()
        .take(CONTEXT_PLANNING_PACKET_VERIFICATION_LIMIT)
        .map(|query| {
            json!({
                "kind": "codegraph_query_text",
                "shell_ready": false,
                "query_text": query.get("query_text").and_then(Value::as_str).unwrap_or_default(),
                "path_scope": query.get("path_scope").and_then(Value::as_str).unwrap_or_default(),
                "reason": query.get("reason").and_then(Value::as_str).unwrap_or_default(),
                "expected_signal": query.get("expected_signal").and_then(Value::as_str).unwrap_or_default(),
                "max_results_hint": query.get("max_results_hint").and_then(Value::as_u64).unwrap_or(10),
            })
        })
        .collect()
}

pub(crate) fn context_planning_claimability_json(
    evidence_type: &str,
    lifecycle_claimable: bool,
    graph_proof: bool,
) -> Value {
    if graph_proof && evidence_type == "graph_proof" && lifecycle_claimable {
        return json!({
            "claimable": true,
            "claimable_as": ["graph_relation_proof", "source_text_existence"],
            "not_claimable_as": [],
        });
    }
    if matches!(evidence_type, "text_evidence" | "fallback") && lifecycle_claimable {
        return json!({
            "claimable": true,
            "claimable_as": ["source_text_existence"],
            "not_claimable_as": ["graph_relation_proof"],
        });
    }
    json!({
        "claimable": false,
        "claimable_as": [],
        "not_claimable_as": ["graph_relation_proof"],
    })
}

pub(crate) fn context_pack_top_level_claimability_json(
    lifecycle_claimable: bool,
    graph_proof: bool,
) -> Value {
    if lifecycle_claimable && graph_proof {
        return json!({
            "claimable": true,
            "claimable_as": ["graph_relation_proof", "source_text_existence"],
            "not_claimable_as": [],
            "graph_proof_only_from_graph_source_verification": true,
        });
    }
    if lifecycle_claimable {
        return json!({
            "claimable": true,
            "claimable_as": ["source_text_existence"],
            "not_claimable_as": ["graph_relation_proof"],
            "graph_proof_only_from_graph_source_verification": true,
        });
    }
    json!({
        "claimable": false,
        "claimable_as": [],
        "not_claimable_as": ["graph_relation_proof"],
        "graph_proof_only_from_graph_source_verification": true,
    })
}

pub(crate) fn context_planning_do_not_touch_areas(likely_files: &[String]) -> Vec<String> {
    let build_or_config_surface = likely_files.iter().any(|file| {
        let lower = context_planning_normalize_path(file).to_ascii_lowercase();
        lower.starts_with("package/")
            || lower.starts_with("support/scripts/")
            || lower.starts_with("docs/")
            || lower.ends_with(".mk")
            || lower.ends_with("config.in")
            || lower.ends_with(".adoc")
    });
    if !build_or_config_surface {
        return Vec::new();
    }
    vec![
        "generated/build/cache directories".to_string(),
        "target/".to_string(),
        "node_modules/".to_string(),
        "reports/final generated artifacts".to_string(),
        "database, WAL, and SHM artifacts".to_string(),
    ]
}

pub(crate) fn context_planning_br2_tokens(text: &str) -> Vec<String> {
    unique_limited_strings(
        text.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .filter(|part| part.starts_with("BR2_PACKAGE_"))
            .map(str::to_string),
        8,
    )
}

pub(crate) fn context_planning_first_path_file(path: &Value) -> Option<String> {
    path.get("source_spans")
        .and_then(Value::as_array)
        .and_then(|spans| spans.first())
        .and_then(|span| context_planning_value_string(span, "file"))
}

pub(crate) fn context_planning_value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
}

pub(crate) fn context_planning_basename(path: &str) -> Option<String> {
    path.rsplit('/')
        .next()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

pub(crate) fn context_planning_normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

pub(crate) fn context_planning_symbol_allowed(symbol: &str) -> bool {
    let trimmed = symbol.trim();
    if trimmed.len() < 2 {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    !matches!(
        lower.as_str(),
        "find"
            | "plan"
            | "trace"
            | "inspect"
            | "add"
            | "where"
            | "how"
            | "defined"
            | "consumed"
            | "using"
    )
}

#[derive(Debug, Default)]
pub(crate) struct ContextAgentSizeOmissions {
    retrieval_architecture: usize,
    paths: usize,
    snippets: usize,
    fallback_evidence: usize,
    recommended_tests: usize,
    risks: usize,
    planning_packet: usize,
    routing_packet: usize,
    max_output_bytes_exceeded: bool,
}

pub(crate) fn enforce_context_agent_max_output_bytes(
    response: &mut Value,
    max_output_bytes: usize,
) -> ContextAgentSizeOmissions {
    let mut omitted = ContextAgentSizeOmissions::default();
    while serde_json::to_vec(response)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
        > max_output_bytes
    {
        if remove_context_agent_field(response, "retrieval_architecture") {
            omitted.retrieval_architecture += 1;
            continue;
        }
        // Downgrade a full explain to its bounded summary before generic field
        // removal. Patch-assist is attached after the first explain budget
        // decision, so it can push a previously fitting full explain over the
        // final cap. The summary preserves vector operability status/counts and
        // remains the explain-mode contract surface under that later pressure.
        let full_retrieval_explain = response
            .get("retrieval_explain")
            .filter(|explain| explain.get("budget_limited").and_then(Value::as_bool) != Some(true))
            .cloned();
        if let Some(full_retrieval_explain) = full_retrieval_explain {
            let full_explain_bytes = serialized_json_len(&full_retrieval_explain);
            let summary = context_pack_retrieval_explain_budget_summary(
                &full_retrieval_explain,
                full_explain_bytes,
                max_output_bytes,
            );
            if let Some(object) = response.as_object_mut() {
                object.insert("retrieval_explain".to_string(), summary);
                object.insert(
                    "explain_budget_status".to_string(),
                    json!({
                        "enabled": true,
                        "status": "summary_included_full_explain_omitted_by_explain_budget",
                        "separate_from_evidence_budget": true,
                        "full_explain_bytes": full_explain_bytes,
                    }),
                );
            }
            omitted.retrieval_architecture += 1;
            continue;
        }
        if pop_context_agent_planning_array_item(response, "evidence_items") {
            omitted.planning_packet += 1;
            continue;
        }
        if pop_context_agent_planning_array_item(response, "suggested_verification_commands") {
            omitted.planning_packet += 1;
            continue;
        }
        if pop_context_agent_planning_array_item(response, "follow_up_queries") {
            omitted.planning_packet += 1;
            continue;
        }
        if pop_context_agent_planning_array_item(response, "likely_files") {
            omitted.planning_packet += 1;
            continue;
        }
        if pop_context_agent_planning_array_item(response, "likely_symbols") {
            omitted.planning_packet += 1;
            continue;
        }
        if pop_context_agent_planning_array_item(response, "do_not_touch_areas") {
            omitted.planning_packet += 1;
            continue;
        }
        if compact_context_agent_routing_packet(response) {
            omitted.routing_packet += 1;
            continue;
        }
        // Top-level micro_flow_handles carry full expandable handle bodies; the
        // agent-use wrapper already compacts them before shedding evidence, but
        // the direct builder path reached the evidence pops with them intact.
        if compact_context_agent_micro_flow_handles(response) {
            omitted.retrieval_architecture += 1;
            continue;
        }
        if pop_context_agent_routing_array_item(response, "expansion_handles") {
            omitted.routing_packet += 1;
            continue;
        }
        if pop_context_agent_routing_array_item(response, "implementation_trace_micro_flow_handles")
        {
            omitted.routing_packet += 1;
            continue;
        }
        if pop_context_agent_routing_array_item(response, "edit_plan") {
            omitted.routing_packet += 1;
            continue;
        }
        if pop_context_agent_routing_array_item(response, "validation_steps") {
            omitted.routing_packet += 1;
            continue;
        }
        if pop_context_agent_routing_array_item(response, "follow_up_queries") {
            omitted.routing_packet += 1;
            continue;
        }
        if pop_context_agent_routing_array_item(response, "risks") {
            omitted.routing_packet += 1;
            continue;
        }
        if pop_context_agent_routing_array_item(response, "formulas_or_accounting_notes") {
            omitted.routing_packet += 1;
            continue;
        }
        if pop_context_agent_routing_array_item(response, "critical_symbols") {
            omitted.routing_packet += 1;
            continue;
        }
        if compact_context_agent_capability_metadata(response) {
            omitted.retrieval_architecture += 1;
            continue;
        }
        if compact_context_agent_db_lifecycle_read(response) {
            omitted.retrieval_architecture += 1;
            continue;
        }
        if compact_context_agent_staged_availability(response) {
            omitted.retrieval_architecture += 1;
            continue;
        }
        // Per-row policy boilerplate and duplicated full path bodies inside the
        // routing packet are metadata weight, not evidence: slim them before any
        // evidence section below is popped.
        if slim_context_agent_routing_row_boilerplate(response) {
            omitted.routing_packet += 1;
            continue;
        }
        if compact_context_agent_routing_packet(response) {
            omitted.routing_packet += 1;
            continue;
        }
        // Never remove planning_packet outright: its proof-labeled scalar fields
        // (evidence_type/proof_status/graph_proof/claimability) are part of the
        // contract. Reduce it to a minimal proof stub instead, dropping the bulky
        // arrays already trimmed above.
        if compact_context_agent_planning_packet_minimal(response) {
            omitted.planning_packet += 1;
            continue;
        }
        if compact_context_agent_patch_assist_packet(response) {
            omitted.routing_packet += 1;
            continue;
        }
        if strip_context_agent_evidence_language_capabilities(response) {
            omitted.retrieval_architecture += 1;
            continue;
        }
        if compact_context_agent_graph_verification(response) {
            omitted.retrieval_architecture += 1;
            continue;
        }
        let mut removed_compact_field = false;
        for key in [
            "stale_evidence",
            "refreshed_evidence",
            "unavailable_evidence",
            "stale_non_proof_reasons",
            "sidecar_statuses",
            "proof_ladder_change_counts",
            "severity_effect",
            "candidate_source_counts",
            "candidate_omitted_reason",
            "candidate_exact_seed_cap_override",
            "candidate_cap_policy",
            "selected_role_coverage",
            "evidence_budget_status",
            "dirty_evidence_summary",
            // NOTE: fallback_snippets and explain_budget_status are intentionally
            // NOT in this early-removal list. fallback_snippets is reduced via
            // pop-preserve-one below (and only fully removed at last resort), and
            // explain_budget_status is part of the explain contract.
        ] {
            if remove_context_agent_field(response, key) {
                omitted.retrieval_architecture += 1;
                removed_compact_field = true;
                break;
            }
        }
        if removed_compact_field {
            continue;
        }
        if compact_context_agent_planning_packet_minimal(response) {
            omitted.planning_packet += 1;
            continue;
        }
        // Candidate rows are evidence-adjacent: pop them only after the compact
        // metadata fields above are gone, so an exact-seed candidate survives
        // budget pressure that pure metadata could have absorbed.
        if pop_context_agent_candidate_item(response) {
            omitted.retrieval_architecture += 1;
            continue;
        }
        if pop_context_agent_array_item_preserve_one(response, "recommended_tests") {
            omitted.recommended_tests += 1;
            continue;
        }
        if pop_context_agent_array_item(response, "risks") {
            omitted.risks += 1;
            continue;
        }
        if pop_context_agent_array_item(response, "snippets") {
            omitted.snippets += 1;
            continue;
        }
        if pop_context_agent_array_item_preserve_one(response, "fallback_snippets") {
            omitted.snippets += 1;
            continue;
        }
        // fallback_evidence is the source/text evidence surface when there is no
        // graph proof: keep at least one row through normal pressure (full
        // removal stays available in the last-resort section for extreme
        // budgets), so `result_count` cannot silently drop to zero while
        // metadata survives.
        if pop_context_agent_array_item_preserve_one(response, "fallback_evidence") {
            omitted.fallback_evidence += 1;
            if let Some(object) = response.as_object_mut() {
                let fallback_count = object
                    .get("fallback_evidence")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or_default();
                object.insert("fallback_evidence_count".to_string(), json!(fallback_count));
            }
            continue;
        }
        if pop_context_agent_routing_array_item(response, "critical_files") {
            omitted.routing_packet += 1;
            continue;
        }
        if pop_context_agent_routing_array_item(response, "text_evidence") {
            omitted.routing_packet += 1;
            continue;
        }
        if pop_context_agent_array_item(response, "paths") {
            omitted.paths += 1;
            if let Some(object) = response.as_object_mut() {
                if let Some(paths) = object.get("paths").cloned() {
                    object.insert("proof_paths".to_string(), paths);
                }
            }
            continue;
        }
        // Last-resort trims under extreme budgets (e.g. 4 KiB). These are not in
        // the always-protected scalar set (claimability, graph_verification,
        // lifecycle, proof_status/strength, graph_proof) the contract guarantees,
        // so they may be reduced only after every lighter trim above is exhausted.
        // patch_assist is first reduced to its minimal proof stub (keeps
        // first_use_state / graph_proof / bounded candidate_evidence) and only
        // fully removed if even that minimal form does not fit.
        // Reduce patch_assist to its minimal proof stub first. Its full removal is
        // deferred to the very end (after candidates / planning / fallback) so the
        // minimal stub — which carries first_use_state / graph_proof — survives
        // whenever it can fit, since it is the patch-assist contract surface.
        if compact_context_agent_patch_assist_packet_minimal(response) {
            omitted.routing_packet += 1;
            continue;
        }
        if pop_context_agent_candidate_item_inner(response, true) {
            omitted.retrieval_architecture += 1;
            continue;
        }
        // planning_packet is droppable before fallback_snippets: the latter is the
        // actual source/text evidence an agent reads when there is no graph proof,
        // so it is the last narrative section to go.
        if remove_context_agent_field(response, "planning_packet") {
            omitted.planning_packet += 1;
            continue;
        }
        // The minimal routing packet still keeps a multi-kilobyte floor
        // (preserved evidence arrays + capability plan). Under budgets that
        // floor cannot fit, reduce it to a stub BEFORE any remaining source/
        // text evidence is dropped — routing guidance is metadata, snippets
        // are what the agent actually reads.
        if stub_context_agent_routing_packet(response) {
            omitted.routing_packet += 1;
            continue;
        }
        // Extreme budgets only: the preserve-one fallback rows above may still
        // not fit; drop them entirely before the snippet evidence goes.
        if remove_context_agent_field(response, "fallback_evidence") {
            omitted.fallback_evidence += 1;
            if let Some(object) = response.as_object_mut() {
                object.insert("fallback_evidence_count".to_string(), json!(0));
            }
            continue;
        }
        if remove_context_agent_field(response, "fallback_snippets") {
            omitted.snippets += 1;
            continue;
        }
        // The minimal patch_assist stub (first_use_state / graph_proof) is the
        // patch-assist contract surface and is intentionally NOT fully removed
        // here: it is tiny and must survive. If the envelope is still over budget
        // after every other trim, report the overflow rather than dropping it.
        omitted.max_output_bytes_exceeded = true;
        break;
    }
    omitted
}

pub(crate) fn compact_context_agent_capability_metadata(response: &mut Value) -> bool {
    let mut changed = false;
    let metadata_compacted = response
        .get("capability_metadata")
        .and_then(|metadata| metadata.get("compacted"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if metadata_compacted {
        if let Some(object) = response.as_object_mut() {
            object.remove("capability_metadata");
            changed = true;
        }
    } else if let Some(metadata) = response.get_mut("capability_metadata") {
        let compact = json!({
            "status": metadata.get("status").cloned().unwrap_or_else(|| json!("unknown")),
            "bounded": metadata.get("bounded").cloned().unwrap_or_else(|| json!(true)),
            "not_graph_proof": true,
            "proof_boundary": metadata.get("proof_boundary").cloned().unwrap_or_else(|| json!("capability metadata labels facts; it does not create graph proof")),
            "compacted": true,
        });
        *metadata = compact;
        changed = true;
    }
    changed |= compact_context_agent_language_capability_context(response);
    if let Some(planning) = response.get_mut("planning_packet") {
        changed |= compact_nested_language_capability_context(planning);
        changed |= remove_compacted_nested_language_capability_context(planning);
    }
    if let Some(routing) = response.get_mut("routing_packet") {
        changed |= compact_nested_language_capability_context(routing);
        changed |= remove_compacted_nested_language_capability_context(routing);
    }
    changed
}

pub(crate) fn compact_context_agent_language_capability_context(value: &mut Value) -> bool {
    compact_nested_language_capability_context(value)
}

fn compact_nested_language_capability_context(value: &mut Value) -> bool {
    let Some(context) = value.get_mut("language_capability_context") else {
        return false;
    };
    let packet_language_registry = mvp4_local_flow_packet_language_registry_json();
    if context
        .get("compacted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        if context
            .get("minimal")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return false;
        }
        let minimal = json!({
            "status": context.get("status").cloned().unwrap_or_else(|| json!("unknown")),
            "proof_status": context.get("proof_status").cloned().unwrap_or_else(|| json!("unknown")),
            "proof_strength": context.get("proof_strength").cloned().unwrap_or_else(|| json!("unknown")),
            "graph_proof": context.get("graph_proof").cloned().unwrap_or_else(|| json!(false)),
            "capability_metadata_does_not_create_graph_proof": true,
            "active_packet_languages": packet_language_registry["active_packet_languages"],
            "active_packet_language_count": packet_language_registry["active_packet_language_count"],
            "default_packet_query_language": packet_language_registry["default_packet_query_language"],
            "typescript_packet_handles_preserved": packet_language_registry["typescript_packet_handles_preserved"],
            "inactive_packet_overclaim_count": 0,
            "non_typescript_packet_overclaim_count": 0,
            "context_entry_command_activated": false,
            "compacted": true,
            "minimal": true,
        });
        *context = minimal;
        return true;
    }
    let compact = json!({
        "status": context.get("status").cloned().unwrap_or_else(|| json!("unknown")),
        "proof_status": context.get("proof_status").cloned().unwrap_or_else(|| json!("unknown")),
        "proof_strength": context.get("proof_strength").cloned().unwrap_or_else(|| json!("unknown")),
        "graph_proof": context.get("graph_proof").cloned().unwrap_or_else(|| json!(false)),
        "evidence_status": context.get("evidence_status").cloned().unwrap_or_else(|| json!("unknown")),
        "unknown_boundary_rows": context.get("unknown_boundary_rows").cloned().unwrap_or_else(|| json!(0)),
        "resolver_metadata_rows": context.get("resolver_metadata_rows").cloned().unwrap_or_else(|| json!(0)),
        "local_flow_packet_boundary": context.get("local_flow_packet_boundary").cloned().unwrap_or_else(|| json!({
            "active_languages": packet_language_registry["active_packet_languages"],
            "active_packet_languages": packet_language_registry["active_packet_languages"],
            "active_packet_language_count": packet_language_registry["active_packet_language_count"],
            "default_packet_query_language": packet_language_registry["default_packet_query_language"],
            "typescript_packet_handles_preserved": packet_language_registry["typescript_packet_handles_preserved"],
            "inactive_packet_overclaim_count": 0,
            "non_typescript_packet_overclaim_count": 0,
            "handles_do_not_create_proof": true,
            "context_entry_command_activated": false
        })),
        "capability_metadata_does_not_create_graph_proof": true,
        "compacted": true,
    });
    *context = compact;
    true
}

fn remove_compacted_nested_language_capability_context(value: &mut Value) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let should_remove = object
        .get("language_capability_context")
        .and_then(|context| context.get("compacted"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if should_remove {
        object.remove("language_capability_context");
        return true;
    }
    false
}

pub(crate) fn strip_context_agent_evidence_language_capabilities(response: &mut Value) -> bool {
    let mut changed = false;
    for key in [
        "paths",
        "snippets",
        "fallback_evidence",
        "fallback_snippets",
        "candidates",
    ] {
        changed |= strip_language_capability_from_array(response, key);
    }
    if let Some(planning) = response.get_mut("planning_packet") {
        changed |= strip_language_capability_from_array(planning, "evidence_items");
    }
    if let Some(routing) = response.get_mut("routing_packet") {
        for key in [
            "critical_files",
            "verified_paths",
            "text_evidence",
            "source_navigation_evidence",
            "fallback_snippets",
        ] {
            changed |= strip_language_capability_from_array(routing, key);
        }
    }
    changed
}

fn strip_language_capability_from_array(value: &mut Value, key: &str) -> bool {
    let Some(items) = value.get_mut(key).and_then(Value::as_array_mut) else {
        return false;
    };
    let mut changed = false;
    for item in items {
        if let Some(object) = item.as_object_mut() {
            if object.remove("language_capability").is_some() {
                changed = true;
            }
        }
    }
    changed
}

pub(crate) fn compact_context_agent_graph_verification(response: &mut Value) -> bool {
    let Some(graph) = response.get_mut("graph_verification") else {
        return false;
    };
    if graph
        .get("compacted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    let compact = json!({
        "status": graph.get("status").cloned().unwrap_or_else(|| json!("unknown")),
        "candidate_count": graph.get("candidate_count").cloned().unwrap_or_else(|| json!(0)),
        "proof_status": graph.get("proof_status").cloned().unwrap_or_else(|| json!("unknown")),
        "graph_proof": graph.get("graph_proof").cloned().unwrap_or_else(|| json!(false)),
        "evidence_status": graph.get("evidence_status").cloned().unwrap_or_else(|| json!("unknown")),
        "reason": graph.get("reason").cloned().unwrap_or_else(|| json!("graph verification reason unavailable")),
        "proof_failure_reason": graph.get("proof_failure_reason").cloned().unwrap_or(Value::Null),
        "compacted": true,
        "contract": {
            "graph_source_verification_only": true,
            "text_evidence_is_not_graph_proof": true,
            "candidate_evidence_is_not_graph_proof": true,
            "vector_evidence_is_not_graph_proof": true,
        },
    });
    *graph = compact;
    true
}

pub(crate) fn compact_context_agent_staged_availability(response: &mut Value) -> bool {
    let Some(staged) = response.get_mut("staged_availability") else {
        return false;
    };
    if staged
        .get("agent_json_compacted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    let compact = json!({
        "graph_db_status": staged.get("graph_db_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_status": staged.get("candidate_spool_status").cloned().unwrap_or(Value::Null),
        "vector_runtime_status": staged.get("vector_runtime_status").cloned().unwrap_or(Value::Null),
        "vector_audit_status": staged.get("vector_audit_status").cloned().unwrap_or(Value::Null),
        "graph_proof_available": staged.get("graph_proof_available").cloned().unwrap_or(Value::Null),
        "candidate_only_available": staged.get("candidate_only_available").cloned().unwrap_or(Value::Null),
        "claimability": staged.get("claimability").cloned().unwrap_or(Value::Null),
        "active_candidate_sources": staged.get("active_candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "public_claim": false,
        "agent_json_compacted": true,
    });
    *staged = compact;
    true
}

pub(crate) fn compact_context_agent_patch_assist_packet_minimal(response: &mut Value) -> bool {
    let Some(packet) = response.get_mut("patch_assist_packet") else {
        return false;
    };
    if packet
        .pointer("/packet_budget_status/agent_use_wrapper_minimal")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    let compact = json!({
        "packet_kind": packet.get("packet_kind").cloned().unwrap_or_else(|| json!("patch_assist_staged_context")),
        "schema_version": packet.get("schema_version").cloned().unwrap_or_else(|| json!(1)),
        "first_use_state": packet.get("first_use_state").cloned().unwrap_or_else(|| json!("unknown")),
        "proof_status": packet.get("proof_status").cloned().unwrap_or_else(|| json!("unknown")),
        "proof_strength": packet.get("proof_strength").cloned().unwrap_or_else(|| json!("unknown")),
        "graph_proof": packet.get("graph_proof").cloned().unwrap_or_else(|| json!(false)),
        "claimability": packet.get("claimability").cloned().unwrap_or(Value::Null),
        "unavailable": packet.get("unavailable").cloned().unwrap_or_else(|| json!(false)),
        // Keep a bounded slice of the proof-relevant arrays (not just counts): the
        // contract surfaces at least one candidate-evidence item and the layer
        // degradation warnings agents act on. Counts are preserved from the source.
        "candidate_evidence": routing_take_array(packet, "candidate_evidence", 1),
        "degradation_warnings": routing_take_array(packet, "degradation_warnings", 2),
        "candidate_evidence_count": packet.get("candidate_evidence_count").cloned().unwrap_or_else(|| json!(0)),
        "degradation_warning_count": packet.get("degradation_warning_count").cloned().unwrap_or_else(|| json!(0)),
        "source_navigation_evidence_count": packet.get("source_navigation_evidence_count").cloned().unwrap_or_else(|| json!(0)),
        "packet_budget_status": {
            "packet_budget_enforced": true,
            "status": "bounded_with_omissions",
            "agent_json_compacted": true,
            "agent_use_wrapper_minimal": true,
            "preserved_core_state": true,
        },
    });
    *packet = compact;
    true
}

pub(crate) fn compact_context_agent_db_lifecycle_read(response: &mut Value) -> bool {
    let Some(lifecycle) = response.get_mut("db_lifecycle_read") else {
        return false;
    };
    if lifecycle
        .get("agent_json_compacted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    let compact = json!({
        "claimable": lifecycle.get("claimable").cloned().unwrap_or_else(|| json!(false)),
        "diagnostic_only": lifecycle.get("diagnostic_only").cloned().unwrap_or_else(|| json!(true)),
        "decision": lifecycle.get("decision").cloned().unwrap_or_else(|| json!("unknown")),
        "exact_db_path_checked": lifecycle.get("exact_db_path_checked").cloned().unwrap_or(Value::Null),
        "artifact_freshness": lifecycle.get("artifact_freshness").cloned().unwrap_or(Value::Null),
        "passport_status": lifecycle.get("passport_status").cloned().unwrap_or(Value::Null),
        "path_access_status": lifecycle.get("path_access_status").cloned().unwrap_or(Value::Null),
        "repo_root_status": lifecycle.get("repo_root_status").cloned().unwrap_or(Value::Null),
        "schema_status": lifecycle.get("schema_status").cloned().unwrap_or(Value::Null),
        "scope_status": lifecycle.get("scope_status").cloned().unwrap_or(Value::Null),
        "sidecar_status": lifecycle.get("sidecar_status").cloned().unwrap_or(Value::Null),
        "db_path_outside_workspace": lifecycle.get("db_path_outside_workspace").cloned().unwrap_or(Value::Null),
        "agent_json_compacted": true,
    });
    *lifecycle = compact;
    true
}

pub(crate) fn serialized_json_len(value: &Value) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

pub(crate) fn pop_context_agent_candidate_item(response: &mut Value) -> bool {
    pop_context_agent_candidate_item_inner(response, false)
}

pub(crate) fn pop_context_agent_candidate_item_inner(
    response: &mut Value,
    allow_empty: bool,
) -> bool {
    let Some(candidates) = response.get_mut("candidates").and_then(Value::as_array_mut) else {
        return false;
    };
    // Under moderate pressure keep at least one candidate so a proof-bearing
    // (path_evidence) candidate survives; only the extreme last-resort pass
    // (allow_empty) may empty the array entirely.
    let floor = if allow_empty { 0 } else { 1 };
    if candidates.len() <= floor {
        return false;
    }
    // Prefer dropping a non-proof candidate so a graph-verified / path_evidence
    // candidate is the one retained under pressure. Fall back to popping the
    // last element only when every remaining candidate is proof-bearing.
    let is_proof_candidate = |candidate: &Value| -> bool {
        candidate["verification_status"].as_str() == Some("graph_verified")
            || candidate["proof_status"].as_str() == Some("proof_path_found")
            || candidate["candidate_sources"]
                .as_array()
                .is_some_and(|sources| {
                    sources
                        .iter()
                        .any(|source| source.as_str() == Some("path_evidence"))
                })
    };
    if let Some(index) = candidates.iter().rposition(|c| !is_proof_candidate(c)) {
        candidates.remove(index);
    } else {
        candidates.pop();
    }
    let candidate_count = candidates.len();
    if let Some(object) = response.as_object_mut() {
        object.insert("candidate_count".to_string(), json!(candidate_count));
        object.insert("candidate_payload_compacted".to_string(), json!(true));
        object.insert(
            "candidate_omitted_reason".to_string(),
            json!(
                "candidate_payload_omitted_to_preserve_required_agent_state_under_max_output_bytes"
            ),
        );
    }
    true
}

pub(crate) fn pop_context_agent_array_item(response: &mut Value, key: &str) -> bool {
    let Some(array) = response.get_mut(key).and_then(Value::as_array_mut) else {
        return false;
    };
    if array.is_empty() {
        return false;
    }
    array.pop();
    true
}

pub(crate) fn pop_context_agent_array_item_preserve_one(response: &mut Value, key: &str) -> bool {
    let Some(array) = response.get_mut(key).and_then(Value::as_array_mut) else {
        return false;
    };
    if array.len() <= 1 {
        return false;
    }
    array.pop();
    true
}

pub(crate) fn remove_context_agent_field(response: &mut Value, key: &str) -> bool {
    let Some(object) = response.as_object_mut() else {
        return false;
    };
    object.remove(key).is_some()
}

/// Reduce `planning_packet` to a minimal proof-labeled stub under byte pressure,
/// preserving the contract scalar fields while dropping bulky arrays/sub-objects.
/// Returns false when there is no planning_packet or it is already minimal, so
/// the size-enforcement loop makes progress instead of spinning.
pub(crate) fn compact_context_agent_planning_packet_minimal(response: &mut Value) -> bool {
    let Some(packet) = response.get("planning_packet").and_then(Value::as_object) else {
        return false;
    };
    // Already minimal (only the preserved keys present) -> nothing more to do.
    if packet.get("agent_json_compacted").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    let minimal = json!({
        "evidence_type": packet.get("evidence_type").cloned().unwrap_or(Value::Null),
        "proof_status": packet.get("proof_status").cloned().unwrap_or(Value::Null),
        "graph_proof": packet.get("graph_proof").cloned().unwrap_or(Value::Null),
        "claimable": packet.get("claimable").cloned().unwrap_or(Value::Null),
        "claimability": packet.get("claimability").cloned().unwrap_or(Value::Null),
        "confidence": packet.get("confidence").cloned().unwrap_or(Value::Null),
        "agent_json_compacted": true,
    });
    if let Some(object) = response.as_object_mut() {
        object.insert("planning_packet".to_string(), minimal);
        return true;
    }
    false
}

pub(crate) fn pop_context_agent_planning_array_item(response: &mut Value, key: &str) -> bool {
    let Some(packet) = response
        .get_mut("planning_packet")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let popped = packet
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .is_some_and(|array| array.pop().is_some());
    if popped {
        let omitted_count = packet
            .get("omitted_count")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            + 1;
        packet.insert("omitted_count".to_string(), json!(omitted_count));
        packet.insert("omitted_by_budget".to_string(), json!(omitted_count));
    }
    popped
}

pub(crate) fn pop_context_agent_routing_array_item(response: &mut Value, key: &str) -> bool {
    let Some(packet) = response
        .get_mut("routing_packet")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let preserve_limit = match key {
        "critical_files" => Some(8),
        "critical_symbols" => Some(12),
        "verified_paths" => Some(3),
        "text_evidence" => Some(5),
        "source_navigation_evidence" => Some(6),
        "fallback_snippets" => Some(3),
        "micro_flow_handles" => Some(4),
        "risks" => Some(8),
        "validation_steps" => Some(6),
        "language_validation_steps" => Some(6),
        "follow_up_queries" => Some(6),
        "artifact_inspection_requirements" => Some(3),
        "db_inspection_requirements" => Some(3),
        "formulas_or_accounting_notes" => Some(4),
        "unknowns" => Some(3),
        _ => None,
    };
    let popped = packet
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .is_some_and(|array| {
            if preserve_limit.is_some_and(|limit| array.len() <= limit) {
                return false;
            }
            array.pop().is_some()
        });
    if popped {
        let omitted_count = packet
            .get("omitted_count")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            + 1;
        packet.insert("omitted_count".to_string(), json!(omitted_count));
        if let Some(status) = packet
            .get_mut("budget_status")
            .and_then(Value::as_object_mut)
        {
            let omitted_by_budget = status
                .get("omitted_by_budget")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                + 1;
            status.insert("omitted_by_budget".to_string(), json!(omitted_by_budget));
            status.insert("status".to_string(), json!("bounded_with_omissions"));
        }
    }
    popped
}

pub(crate) fn compact_context_agent_routing_packet(response: &mut Value) -> bool {
    let Some(packet) = response.get_mut("routing_packet") else {
        return false;
    };
    if packet
        .get("budget_status")
        .and_then(|status| status.get("routing_packet_compacted"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return minimize_context_agent_routing_packet(packet);
    }
    let query_atom_roles = packet
        .pointer("/retrieval_plan_summary/query_atoms")
        .and_then(Value::as_array)
        .map(|atoms| {
            atoms
                .iter()
                .filter_map(|atom| atom.get("role").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let compact_retrieval_plan = json!({
        "plan_id": packet.pointer("/retrieval_plan_summary/plan_id").cloned().unwrap_or(Value::Null),
        "profile_id": packet.pointer("/retrieval_plan_summary/profile_id").cloned().unwrap_or(Value::Null),
        "query_atom_roles": query_atom_roles,
        "proof_attempt_policy": packet.pointer("/retrieval_plan_summary/proof_attempt_policy").cloned().unwrap_or(Value::Null),
        "fallback_policy": packet.pointer("/retrieval_plan_summary/fallback_policy").cloned().unwrap_or(Value::Null),
        "compacted": true,
    });
    let compact = json!({
        "packet_kind": packet.get("packet_kind").cloned().unwrap_or_else(|| json!("agent_routing_packet")),
        "schema_version": packet.get("schema_version").cloned().unwrap_or_else(|| json!(1)),
        "task_intent": packet.get("task_intent").cloned().unwrap_or(Value::Null),
        "task_profile": packet.get("task_profile").cloned().unwrap_or(Value::Null),
        "task_roles": packet.get("task_roles").cloned().unwrap_or_else(|| json!([])),
        "retrieval_plan_summary": compact_retrieval_plan,
        "claimability": packet.get("claimability").cloned().unwrap_or(Value::Null),
        "critical_files": routing_take_array(packet, "critical_files", 8),
        "critical_symbols": routing_take_array(packet, "critical_symbols", 12),
        "verified_paths": routing_take_array(packet, "verified_paths", 3),
        "text_evidence": routing_take_array(packet, "text_evidence", 5),
        "source_navigation_evidence": routing_take_array(packet, "source_navigation_evidence", 6),
        "fallback_snippets": routing_take_array(packet, "fallback_snippets", 3),
        "micro_flow_handles": routing_take_array(packet, "micro_flow_handles", 4),
        "micro_flow_packet_summary": packet.get("micro_flow_packet_summary").cloned().unwrap_or(Value::Null),
        "agent_investigation_layer": compact_routing_agent_investigation_layer(packet),
        "language_capability_plan": compact_routing_language_capability_plan(packet),
        "micro_flow_handle_policy": packet.get("micro_flow_handle_policy").cloned().unwrap_or_else(|| json!({
            "packet_handles_do_not_create_proof": true,
            "compact_default_full_packet_body_inline": false,
            "compact_default_ordered_steps_inline": false,
            "context_entry_command_activated": false,
            "route_bridge_pull_forward_count": 0
        })),
        "unknowns": routing_take_array(packet, "unknowns", 3),
        "risks": routing_take_array(packet, "risks", 8),
        "validation_steps": routing_take_array(packet, "validation_steps", 6),
        "language_validation_steps": routing_take_array(packet, "language_validation_steps", 6),
        "validation_steps_language_aware": packet.get("validation_steps_language_aware").cloned().unwrap_or_else(|| json!(true)),
        "follow_up_queries": routing_take_array(packet, "follow_up_queries", 6),
        "expansion_command_available": packet.get("expansion_command_available").cloned().unwrap_or_else(|| json!(false)),
        "artifact_inspection_requirements": routing_take_array(packet, "artifact_inspection_requirements", 3),
        "db_inspection_requirements": routing_take_array(packet, "db_inspection_requirements", 3),
        "formulas_or_accounting_notes": routing_take_array(packet, "formulas_or_accounting_notes", 4),
        "omitted_count": packet.get("omitted_count").cloned().unwrap_or_else(|| json!(0)),
        "budget_status": {
            "status": "bounded_with_omissions",
            "routing_packet_compacted": true,
            "fallback_snippets_survive_compaction": true,
            "source_navigation_evidence_survives_for_implementation_trace": true,
            "explain_debug_budget_separate": true,
        },
        "deterministic_summary": packet.get("deterministic_summary").cloned().unwrap_or(Value::Null),
    });
    *packet = compact;
    true
}

fn minimize_context_agent_routing_packet(packet: &mut Value) -> bool {
    if packet
        .get("budget_status")
        .and_then(|status| status.get("routing_packet_minimal"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    let compact = json!({
        "packet_kind": packet.get("packet_kind").cloned().unwrap_or_else(|| json!("agent_routing_packet")),
        "schema_version": packet.get("schema_version").cloned().unwrap_or_else(|| json!(1)),
        "task_intent": minimize_routing_task_intent(packet),
        "claimability": packet.get("claimability").cloned().unwrap_or(Value::Null),
        "critical_files": routing_minimal_evidence_array(packet, "critical_files", 4),
        "critical_symbols": routing_minimal_symbol_array(packet, "critical_symbols", 12),
        "verified_paths": routing_take_array(packet, "verified_paths", 3)
            .as_array()
            .map(|paths| json!(paths.iter().map(slim_routing_verified_path_row).collect::<Vec<_>>()))
            .unwrap_or_else(|| json!([])),
        "source_navigation_evidence": routing_minimal_evidence_array(packet, "source_navigation_evidence", 4),
        "fallback_snippets": routing_minimal_fallback_snippets(packet, 1),
        "agent_investigation_layer": minimize_routing_agent_investigation_layer(packet),
        "language_capability_plan": minimize_routing_language_capability_plan(packet),
        "unknowns": routing_take_array(packet, "unknowns", 3),
        "risks": routing_minimal_risk_array(packet, "risks", 8),
        "validation_steps": routing_take_array(packet, "validation_steps", 2),
        "validation_steps_language_aware": packet.get("validation_steps_language_aware").cloned().unwrap_or_else(|| json!(true)),
        "language_validation_steps": routing_take_array(packet, "language_validation_steps", 1),
        "follow_up_queries": routing_take_array(packet, "follow_up_queries", 3)
            .as_array()
            .map(|queries| json!(queries.iter().map(slim_routing_follow_up_query_row).collect::<Vec<_>>()))
            .unwrap_or_else(|| json!([])),
        "follow_up_query_policy": routing_follow_up_query_policy_note(),
        "deterministic_summary": minimize_routing_deterministic_summary(packet),
        "artifact_inspection_requirements": routing_take_array(packet, "artifact_inspection_requirements", 3),
        "db_inspection_requirements": routing_take_array(packet, "db_inspection_requirements", 3),
        "formulas_or_accounting_notes": routing_take_array(packet, "formulas_or_accounting_notes", 4),
        "omitted_count": packet.get("omitted_count").cloned().unwrap_or_else(|| json!(0)),
        "micro_flow_handle_policy": {
            "packet_handles_do_not_create_proof": true,
            "compact_default_full_packet_body_inline": false,
            "compact_default_ordered_steps_inline": false,
            "context_entry_command_activated": false,
            "route_bridge_pull_forward_count": 0
        },
        "budget_status": {
            "status": "bounded_with_omissions",
            "routing_packet_compacted": true,
            "routing_packet_minimal": true,
            "fallback_snippets_survive_compaction": true,
            "source_navigation_evidence_survives_for_implementation_trace": true,
            "explain_debug_budget_separate": true,
        },
    });
    *packet = compact;
    true
}

/// Last-resort routing reduction: replace the routing packet with a stub that
/// keeps only its identity, task kind, omission accounting, and one minimal
/// fallback snippet (evidence outlives routing boilerplate). Fires once, only
/// after compact + minimize could not reach the budget, and always before
/// remaining source/text evidence would be dropped.
pub(crate) fn stub_context_agent_routing_packet(response: &mut Value) -> bool {
    let Some(packet) = response.get_mut("routing_packet") else {
        return false;
    };
    if packet
        .get("budget_status")
        .and_then(|status| status.get("routing_packet_stub"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    let omitted_count = packet
        .get("omitted_count")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        + 1;
    let minimal_fallback_snippets = routing_minimal_fallback_snippets(packet, 1);
    let minimal_critical_files = routing_minimal_evidence_array(packet, "critical_files", 4);
    *packet = json!({
        "packet_kind": packet.get("packet_kind").cloned().unwrap_or_else(|| json!("agent_routing_packet")),
        "schema_version": packet.get("schema_version").cloned().unwrap_or_else(|| json!(1)),
        "task_kind": packet.pointer("/task_intent/task_kind").cloned().unwrap_or(Value::Null),
        "omitted_count": omitted_count,
        "budget_status": {
            "status": "bounded_with_omissions",
            "routing_packet_compacted": true,
            "routing_packet_minimal": true,
            "routing_packet_stub": true,
            "routing_packet_reduced_before_evidence_removal": true,
        },
    });
    // Fallback snippets and critical-file pointers are evidence, not routing
    // boilerplate: even the stub keeps minimal rows so evidence never vanishes
    // with the routing metadata it happened to ride in.
    if let Some(object) = packet.as_object_mut() {
        if minimal_fallback_snippets
            .as_array()
            .is_some_and(|items| !items.is_empty())
        {
            object.insert("fallback_snippets".to_string(), minimal_fallback_snippets);
            if let Some(status) = object
                .get_mut("budget_status")
                .and_then(Value::as_object_mut)
            {
                status.insert(
                    "fallback_snippets_survive_compaction".to_string(),
                    json!(true),
                );
            }
        }
        if minimal_critical_files
            .as_array()
            .is_some_and(|items| !items.is_empty())
        {
            object.insert("critical_files".to_string(), minimal_critical_files);
        }
    }
    true
}

/// One budget-pressure pass over routing-packet rows: strip the per-row policy
/// boilerplate (hoisted into one `follow_up_query_policy` note) and reduce
/// duplicated full path bodies to endpoint/relation summaries. The slimmed rows
/// keep their evidence identity; full row bodies remain in explain/audit modes
/// and in packets that fit their budget without pressure.
pub(crate) fn slim_context_agent_routing_row_boilerplate(response: &mut Value) -> bool {
    let Some(packet) = response
        .get_mut("routing_packet")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    if packet
        .get("budget_status")
        .and_then(|status| status.get("routing_rows_slimmed"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    let mut changed = false;
    if let Some(queries) = packet
        .get_mut("follow_up_queries")
        .and_then(Value::as_array_mut)
    {
        for row in queries.iter_mut() {
            let slim = slim_routing_follow_up_query_row(row);
            if *row != slim {
                *row = slim;
                changed = true;
            }
        }
    }
    if let Some(paths) = packet
        .get_mut("verified_paths")
        .and_then(Value::as_array_mut)
    {
        for row in paths.iter_mut() {
            let slim = slim_routing_verified_path_row(row);
            if *row != slim {
                *row = slim;
                changed = true;
            }
        }
    }
    if !changed {
        return false;
    }
    packet.insert(
        "follow_up_query_policy".to_string(),
        routing_follow_up_query_policy_note(),
    );
    if let Some(status) = packet
        .get_mut("budget_status")
        .and_then(Value::as_object_mut)
    {
        status.insert("routing_rows_slimmed".to_string(), json!(true));
    } else {
        packet.insert(
            "budget_status".to_string(),
            json!({
                "status": "bounded_with_omissions",
                "routing_rows_slimmed": true,
            }),
        );
    }
    true
}

/// Shared policy statement for slimmed follow-up query rows: the safety
/// boundary formerly repeated per row rides once at the packet level.
fn routing_follow_up_query_policy_note() -> Value {
    json!({
        "applies_to_all_follow_up_queries": true,
        "capability_boundary": "follow-up query results remain candidate/source evidence until graph/source/resolver verification proves the relation",
        "unsupported_relations_are_not_blockers": true,
        "shell_ready": false,
    })
}

fn slim_routing_follow_up_query_row(row: &Value) -> Value {
    if row
        .get("compacted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return row.clone();
    }
    json!({
        "query_text": row.get("query_text").cloned().unwrap_or(Value::Null),
        "why": row.get("why").cloned().unwrap_or(Value::Null),
        "risk": row.get("risk").cloned().unwrap_or(Value::Null),
        "expected_signal": row.get("expected_signal").cloned().unwrap_or(Value::Null),
        "language_scope": row.get("language_scope").cloned().unwrap_or_else(|| json!([])),
        "shell_ready": false,
        "compacted": true,
    })
}

/// Reduce a routing-packet verified-path duplicate to an endpoint/relation
/// summary. The canonical full path evidence lives in the top-level
/// `paths`/`proof_paths` sections; this keeps the routing copy claim-accurate
/// (endpoints, relation kinds, exactness, claimability) without the per-edge
/// bodies.
fn slim_routing_verified_path_row(row: &Value) -> Value {
    if row
        .get("compacted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return row.clone();
    }
    let edges = row.get("edges").and_then(Value::as_array);
    let first = edges.and_then(|edges| edges.first());
    let last = edges.and_then(|edges| edges.last());
    let relations = edges
        .map(|edges| {
            edges
                .iter()
                .filter_map(|edge| edge.get("relation").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut files: Vec<String> = Vec::new();
    if let Some(edges) = edges {
        for edge in edges {
            if let Some(spans) = edge.get("source_spans").and_then(Value::as_array) {
                for span in spans {
                    if let Some(file) = span.get("file").and_then(Value::as_str) {
                        if !files.iter().any(|known| known == file) {
                            files.push(file.to_string());
                        }
                    }
                }
            }
            if files.len() >= 3 {
                break;
            }
        }
    }
    json!({
        "source": first
            .and_then(|edge| edge.pointer("/source/name"))
            .cloned()
            .unwrap_or(Value::Null),
        "target": last
            .and_then(|edge| edge.pointer("/target/name"))
            .cloned()
            .unwrap_or(Value::Null),
        "relations": relations,
        "edge_count": edges.map(Vec::len).unwrap_or_default(),
        "exactness": first
            .and_then(|edge| edge.get("exactness"))
            .cloned()
            .unwrap_or(Value::Null),
        "confidence": row.get("confidence").cloned().unwrap_or(Value::Null),
        "claimability": row.get("claimability").cloned().unwrap_or(Value::Null),
        "files": files,
        "full_detail_in_top_level_paths": true,
        "compacted": true,
    })
}

/// Minimal task-intent form for the routing minimize pass: keeps the routing
/// decision (kind/profile/confidence/why) and drops the seed/term working sets.
fn minimize_routing_task_intent(packet: &Value) -> Value {
    let intent = packet.get("task_intent").unwrap_or(&Value::Null);
    if !intent.is_object() {
        return intent.clone();
    }
    json!({
        "task_intent_id": intent.get("task_intent_id").cloned().unwrap_or(Value::Null),
        "task_kind": intent.get("task_kind").cloned().unwrap_or(Value::Null),
        "domain": intent.get("domain").cloned().unwrap_or(Value::Null),
        "selected_profile": intent.get("selected_profile").cloned().unwrap_or(Value::Null),
        "confidence": intent.get("confidence").cloned().unwrap_or(Value::Null),
        "why_this_intent": intent.get("why_this_intent").cloned().unwrap_or(Value::Null),
        "compacted": true,
    })
}

/// Minimal deterministic summary for the routing minimize pass: keeps the
/// proof statement, the next inspection pointer, and the top proof-boundary
/// risk lines; drops the null diagnostic slots and long text-evidence prose.
fn minimize_routing_deterministic_summary(packet: &Value) -> Value {
    let summary = packet.get("deterministic_summary").unwrap_or(&Value::Null);
    if !summary.is_object() {
        return summary.clone();
    }
    let risks = summary
        .get("risk")
        .and_then(Value::as_array)
        .map(|risks| risks.iter().take(2).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    json!({
        "proof": summary.get("proof").cloned().unwrap_or(Value::Null),
        "next_inspection": summary.get("next_inspection").cloned().unwrap_or(Value::Null),
        "risk": risks,
        "compacted": true,
    })
}

fn minimize_routing_language_capability_plan(packet: &Value) -> Value {
    let plan = packet
        .get("language_capability_plan")
        .unwrap_or(&Value::Null);
    let local_flow = plan
        .get("local_flow_packet_boundary")
        .unwrap_or(&Value::Null);
    let packet_language_registry = mvp4_local_flow_packet_language_registry_json();
    let active_non_typescript_packet_languages =
        packet_language_registry["active_non_typescript_packet_languages"].clone();
    let inactive_packet_languages = local_flow
        .get("inactive_packet_languages_seen")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let inactive_packet_language_count = inactive_packet_languages
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    json!({
        "status": plan.get("status").cloned().unwrap_or_else(|| json!("unknown")),
        "languages": plan.get("languages").cloned().unwrap_or_else(|| json!([])),
        "unknown_dynamic_risks": compact_routing_language_risks(plan, 8),
        "unsupported_relations": compact_routing_unsupported_relations(plan, 2),
        "local_flow_packet_boundary": {
            "active_languages": local_flow.get("active_languages").cloned().unwrap_or_else(|| packet_language_registry["active_packet_languages"].clone()),
            "active_packet_languages": local_flow.get("active_packet_languages").cloned().unwrap_or_else(|| packet_language_registry["active_packet_languages"].clone()),
            "active_packet_language_count": packet_language_registry["active_packet_language_count"],
            "active_non_typescript_packet_languages": active_non_typescript_packet_languages,
            "active_non_typescript_packet_language_count": packet_language_registry["active_non_typescript_packet_language_count"],
            "default_packet_query_language": packet_language_registry["default_packet_query_language"],
            "typescript_packet_handles_preserved": packet_language_registry["typescript_packet_handles_preserved"],
            "inactive_packet_languages_seen": inactive_packet_languages,
            "inactive_packet_language_count": inactive_packet_language_count,
            "inactive_packet_support": MVP4_3_LOCAL_FLOW_PACKET_INACTIVE_SUPPORT,
            "inactive_packet_overclaim_count": 0,
            "active_non_typescript_packet_support": MVP4_3_LOCAL_FLOW_PACKET_REGISTRY_SCOPED_SUPPORT,
            "active_non_typescript_packet_overclaim_count": 0,
            "non_typescript_packet_languages_seen": active_non_typescript_packet_languages,
            "non_typescript_packet_language_count": packet_language_registry["active_non_typescript_packet_language_count"],
            "non_typescript_packet_support": MVP4_3_LOCAL_FLOW_PACKET_REGISTRY_SCOPED_SUPPORT,
            "non_typescript_packet_overclaim_count": 0,
            "compatibility_aliases": {
                "non_typescript_packet_languages_seen": {
                    "deprecated": true,
                    "alias_of": "active_non_typescript_packet_languages"
                }
            },
            "packet_handles_do_not_create_proof": true,
            "flow_proof_not_emitted_for_unsupported_languages": true,
            "context_entry_command_activated": false
        },
        "proof_boundary": {
            "capability_metadata_does_not_create_graph_proof": true,
            "unsupported_relations_are_not_blockers": true,
            "candidate_or_text_evidence_not_graph_proof": true,
            "route_bridge_context_entry_activated": false,
            "mutation_proof_activated": false
        },
        "unsupported_relations_are_not_blockers": true,
        "capability_metadata_does_not_create_graph_proof": true,
        "non_typescript_packet_overclaim_count": 0,
        "context_entry_command_activated": false,
        "compacted": true,
        "minimal": true,
    })
}

#[cfg(test)]
pub(crate) fn minimize_routing_language_capability_plan_for_test(packet: &Value) -> Value {
    minimize_routing_language_capability_plan(packet)
}

fn minimize_routing_agent_investigation_layer(packet: &Value) -> Value {
    let layer = packet
        .get("agent_investigation_layer")
        .unwrap_or(&Value::Null);
    let plan = packet
        .get("language_capability_plan")
        .unwrap_or(&Value::Null);
    json!({
        "layer": layer.get("layer").cloned().unwrap_or_else(|| json!("agent_investigation")),
        "language_capability_plan": {
            "status": plan.get("status").cloned().unwrap_or_else(|| json!("unknown")),
            "unknown_dynamic_risks": compact_routing_language_risks(plan, 8),
            "unsupported_relations_are_not_blockers": true,
            "capability_metadata_does_not_create_graph_proof": true,
            "compacted": true,
            "minimal": true
        },
        "proof_boundary": {
            "packet_handles_do_not_create_proof": true,
            "candidate_evidence_not_raised_to_proof": true,
            "capability_metadata_does_not_create_graph_proof": true,
            "unsupported_relations_are_not_blockers": true,
            "non_typescript_packet_handles_are_not_proof": true
        },
        "compacted": true,
        "minimal": true,
    })
}

pub(crate) fn routing_take_array(packet: &Value, key: &str, limit: usize) -> Value {
    packet
        .get(key)
        .and_then(Value::as_array)
        .map(|items| Value::Array(items.iter().take(limit).cloned().collect()))
        .unwrap_or_else(|| json!([]))
}

fn routing_minimal_evidence_array(packet: &Value, key: &str, limit: usize) -> Value {
    packet
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            Value::Array(
                items
                    .iter()
                    .take(limit)
                    .map(|item| {
                        json!({
                            "file": item.get("file").cloned().unwrap_or(Value::Null),
                            "role": item.get("role").cloned().unwrap_or_else(|| json!("unknown")),
                            "span": item.get("span").cloned().unwrap_or(Value::Null),
                            "evidence_id": item.get("evidence_id").cloned().unwrap_or(Value::Null),
                            "graph_proof": item.get("graph_proof").cloned().unwrap_or_else(|| json!(false)),
                            "proof_status": item.get("proof_status").cloned().unwrap_or_else(|| json!("not_graph_proof")),
                            "proof_strength": item
                                .pointer("/language_capability/proof_strength")
                                .cloned()
                                .unwrap_or_else(|| json!("non_graph_evidence")),
                            "capability_status": item
                                .pointer("/language_capability/capability_status")
                                .cloned()
                                .unwrap_or_else(|| json!("unknown")),
                        })
                    })
                    .collect(),
            )
        })
        .unwrap_or_else(|| json!([]))
}

fn routing_minimal_symbol_array(packet: &Value, key: &str, limit: usize) -> Value {
    packet
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            Value::Array(
                items
                    .iter()
                    .take(limit)
                    .map(|item| {
                        json!({
                            "symbol": item.get("symbol").cloned().unwrap_or(Value::Null),
                            "evidence_ids": item.get("evidence_ids").cloned().unwrap_or_else(|| json!([])),
                            "proof_status": item.get("proof_status").cloned().unwrap_or_else(|| json!("candidate_or_source_navigation")),
                            "graph_proof": item.get("graph_proof").cloned().unwrap_or_else(|| json!(false)),
                            "capability_status": item
                                .pointer("/language_capability/capability_status")
                                .cloned()
                                .unwrap_or_else(|| json!("unknown")),
                        })
                    })
                    .collect(),
            )
        })
        .unwrap_or_else(|| json!([]))
}

fn routing_minimal_risk_array(packet: &Value, key: &str, limit: usize) -> Value {
    packet
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            Value::Array(
                items
                    .iter()
                    .take(limit)
                    .map(|item| {
                        json!({
                            "risk_id": item.get("risk_id").cloned().unwrap_or(Value::Null),
                            "forbidden_claim": item.get("forbidden_claim").cloned().unwrap_or(Value::Null),
                            "evidence_type": item.get("evidence_type").cloned().unwrap_or(Value::Null),
                            "sentence": item.get("sentence").cloned().unwrap_or_else(|| json!("risk remains non-proof until verified")),
                        })
                    })
                    .collect(),
            )
        })
        .unwrap_or_else(|| json!([]))
}

fn routing_minimal_fallback_snippets(packet: &Value, limit: usize) -> Value {
    packet
        .get("fallback_snippets")
        .and_then(Value::as_array)
        .map(|items| {
            Value::Array(
                items
                    .iter()
                    .take(limit)
                    .map(|item| {
                        json!({
                            "file": item.get("file").cloned().unwrap_or(Value::Null),
                            "span": item.get("span").cloned().unwrap_or(Value::Null),
                            "evidence_id": item.get("evidence_id").cloned().unwrap_or(Value::Null),
                            "fallback_source": item.get("fallback_source").cloned().unwrap_or_else(|| json!("text_evidence_compact_fallback")),
                            "proof_status": item.get("proof_status").cloned().unwrap_or_else(|| json!("no_proof_path_found")),
                            "graph_proof": false,
                        })
                    })
                    .collect(),
            )
        })
        .unwrap_or_else(|| json!([]))
}

fn compact_routing_language_capability_plan(packet: &Value) -> Value {
    let plan = packet
        .get("language_capability_plan")
        .unwrap_or(&Value::Null);
    let local_flow = plan
        .get("local_flow_packet_boundary")
        .unwrap_or(&Value::Null);
    let resolver = plan
        .get("resolver_compiler_availability")
        .unwrap_or(&Value::Null);
    let packet_language_registry = mvp4_local_flow_packet_language_registry_json();
    let active_non_typescript_packet_languages = local_flow
        .get("active_non_typescript_packet_languages")
        .cloned()
        .unwrap_or_else(|| {
            packet_language_registry["active_non_typescript_packet_languages"].clone()
        });
    let inactive_packet_languages = local_flow
        .get("inactive_packet_languages_seen")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let inactive_packet_language_count = inactive_packet_languages
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    json!({
        "status": plan.get("status").cloned().unwrap_or_else(|| json!("unknown")),
        "languages": plan.get("languages").cloned().unwrap_or_else(|| json!([])),
        "capability_status_values": plan.get("capability_status_values").cloned().unwrap_or_else(|| json!([])),
        "resolver_compiler_availability": {
            "resolver_claim_rows": resolver.get("resolver_claim_rows").cloned().unwrap_or_else(|| json!(0)),
            "resolver_claims_missing_provenance": resolver.get("resolver_claims_missing_provenance").cloned().unwrap_or_else(|| json!(0)),
            "semantic_exactness_requires_recorded_provenance": true
        },
        "unknown_dynamic_risks": compact_routing_language_risks(plan, 8),
        "unsupported_relations": compact_routing_unsupported_relations(plan, 2),
        "local_flow_packet_boundary": {
            "active_languages": local_flow.get("active_languages").cloned().unwrap_or_else(|| packet_language_registry["active_packet_languages"].clone()),
            "active_packet_languages": local_flow.get("active_packet_languages").cloned().unwrap_or_else(|| packet_language_registry["active_packet_languages"].clone()),
            "active_packet_language_count": packet_language_registry["active_packet_language_count"],
            "active_non_typescript_packet_languages": active_non_typescript_packet_languages,
            "active_non_typescript_packet_language_count": packet_language_registry["active_non_typescript_packet_language_count"],
            "default_packet_query_language": packet_language_registry["default_packet_query_language"],
            "typescript_packet_handles_preserved": packet_language_registry["typescript_packet_handles_preserved"],
            "inactive_packet_languages_seen": inactive_packet_languages,
            "inactive_packet_language_count": inactive_packet_language_count,
            "inactive_packet_overclaim_count": 0,
            "active_non_typescript_packet_support": MVP4_3_LOCAL_FLOW_PACKET_REGISTRY_SCOPED_SUPPORT,
            "active_non_typescript_packet_overclaim_count": 0,
            "non_typescript_packet_languages_seen": active_non_typescript_packet_languages,
            "non_typescript_packet_language_count": packet_language_registry["active_non_typescript_packet_language_count"],
            "non_typescript_packet_overclaim_count": 0,
            "compatibility_aliases": {
                "non_typescript_packet_languages_seen": {
                    "deprecated": true,
                    "alias_of": "active_non_typescript_packet_languages"
                },
                "non_typescript_packet_language_count": {
                    "deprecated": true,
                    "alias_of": "active_non_typescript_packet_language_count"
                }
            },
            "packet_handles_do_not_create_proof": true,
            "flow_proof_not_emitted_for_unsupported_languages": true,
            "context_entry_command_activated": false
        },
        "proof_boundary": {
            "capability_metadata_does_not_create_graph_proof": true,
            "unsupported_relations_are_not_blockers": true,
            "candidate_or_text_evidence_not_graph_proof": true
        },
        "compacted": true,
    })
}

fn compact_routing_language_risks(plan: &Value, limit: usize) -> Value {
    plan.get("unknown_dynamic_risks")
        .and_then(Value::as_array)
        .map(|items| {
            Value::Array(
                items
                    .iter()
                    .take(limit)
                    .map(|item| {
                        json!({
                            "risk_id": item.get("risk_id").cloned().unwrap_or(Value::Null),
                            "language": item.get("language").cloned().unwrap_or(Value::Null),
                            "boundary": item.get("boundary").cloned().unwrap_or_else(|| json!("unknown")),
                            "blocking": false,
                            "not_graph_proof": true
                        })
                    })
                    .collect(),
            )
        })
        .unwrap_or_else(|| json!([]))
}

fn compact_routing_unsupported_relations(plan: &Value, limit: usize) -> Value {
    plan.get("unsupported_relations")
        .and_then(Value::as_array)
        .map(|items| {
            Value::Array(
                items
                    .iter()
                    .take(limit)
                    .map(|item| {
                        json!({
                            "relation": item.get("relation").cloned().unwrap_or_else(|| json!("unknown")),
                            "status": item.get("status").cloned().unwrap_or_else(|| json!("unsupported")),
                            "blocking": false
                        })
                    })
                    .collect(),
            )
        })
        .unwrap_or_else(|| json!([]))
}

fn compact_routing_agent_investigation_layer(packet: &Value) -> Value {
    let layer = packet
        .get("agent_investigation_layer")
        .unwrap_or(&Value::Null);
    let plan = packet
        .get("language_capability_plan")
        .unwrap_or(&Value::Null);
    let unknown_dynamic_risk_count = plan
        .get("unknown_dynamic_risks")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let unsupported_relation_count = plan
        .get("unsupported_relations")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    json!({
        "layer": layer.get("layer").cloned().unwrap_or_else(|| json!("agent_investigation")),
        "task_kind": layer.get("task_kind").cloned().unwrap_or(Value::Null),
        "micro_flow_handle_count": layer.get("micro_flow_handle_count").cloned().unwrap_or_else(|| json!(0)),
        "micro_flow_handles": routing_take_array(layer, "micro_flow_handles", 1),
        "language_capability_plan": {
            "status": plan.get("status").cloned().unwrap_or_else(|| json!("unknown")),
            "languages": plan.get("languages").cloned().unwrap_or_else(|| json!([])),
            "unknown_dynamic_risk_count": unknown_dynamic_risk_count,
            "unknown_dynamic_risks": compact_routing_language_risks(plan, 8),
            "unsupported_relation_count": unsupported_relation_count,
            "unsupported_relations_are_not_blockers": true,
            "capability_metadata_does_not_create_graph_proof": true,
            "compacted": true
        },
        "usable_for": layer.get("usable_for").cloned().unwrap_or_else(|| json!([
            "trace_boundary",
            "explain_behavior",
            "verify_claim",
            "plan_change",
            "validate_change"
        ])),
        "proof_boundary": layer.get("proof_boundary").cloned().unwrap_or_else(|| json!({
            "packet_handles_do_not_create_proof": true,
            "capability_metadata_does_not_create_graph_proof": true,
            "unsupported_relations_are_not_blockers": true,
            "non_typescript_packet_handles_are_not_proof": true
        })),
        "expansion_required_for_dict_v1_body": true,
        "ordered_steps_audit_only": true,
        "compacted": true,
    })
}

pub(crate) fn update_context_agent_truncation(
    response: &mut Value,
    path_limit: usize,
    omitted_retrieval_architecture: usize,
    omitted_paths: usize,
    omitted_snippets: usize,
    omitted_fallback_evidence: usize,
    omitted_recommended_tests: usize,
    omitted_risks: usize,
    omitted_planning_packet: usize,
    omitted_routing_packet: usize,
    max_output_bytes_exceeded: bool,
) {
    let returned_paths = response
        .get("paths")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let returned_fallback_evidence = response
        .get("fallback_evidence")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let omitted_count = omitted_paths
        + omitted_retrieval_architecture
        + omitted_snippets
        + omitted_fallback_evidence
        + omitted_recommended_tests
        + omitted_risks
        + omitted_planning_packet
        + omitted_routing_packet;
    let omitted_by_budget =
        omitted_paths + omitted_snippets + omitted_fallback_evidence + omitted_planning_packet;
    let byte_count = serde_json::to_vec(response)
        .map(|bytes| bytes.len())
        .unwrap_or_default();
    if let Some(object) = response.as_object_mut() {
        let budget_max_output_bytes = object
            .get("agent_json_budget")
            .and_then(|budget| budget.get("max_output_bytes"))
            .and_then(Value::as_u64)
            .map(|value| value as usize);
        if let Some(budget) = object
            .get_mut("agent_json_budget")
            .and_then(Value::as_object_mut)
        {
            budget.insert("output_bytes".to_string(), json!(byte_count));
            budget.insert(
                "max_output_bytes_exceeded".to_string(),
                json!(budget_max_output_bytes
                    .map(|max| byte_count > max)
                    .unwrap_or(max_output_bytes_exceeded)),
            );
        }
        object.insert("omitted_count".to_string(), json!(omitted_count));
        object.insert("omitted_by_budget".to_string(), json!(omitted_by_budget));
        if let Some(status) = object
            .get_mut("evidence_budget_status")
            .and_then(Value::as_object_mut)
        {
            status.insert("omitted_by_budget".to_string(), json!(omitted_by_budget));
            status.insert(
                "status".to_string(),
                json!(if omitted_by_budget == 0 {
                    "within_budget"
                } else {
                    "bounded_with_omissions"
                }),
            );
        }
        object.insert(
            "fallback_evidence_count".to_string(),
            json!(returned_fallback_evidence),
        );
        object.insert(
            "result_count".to_string(),
            json!(returned_paths + returned_fallback_evidence),
        );
        object.insert(
            "omitted".to_string(),
            json!({
                "retrieval_architecture": omitted_retrieval_architecture,
                "paths": omitted_paths,
                "snippets": omitted_snippets,
                "fallback_evidence": omitted_fallback_evidence,
                "recommended_tests": omitted_recommended_tests,
                "risks": omitted_risks,
                "planning_packet": omitted_planning_packet,
                "routing_packet": omitted_routing_packet,
            }),
        );
        object.insert(
            "truncation".to_string(),
            json!({
                "returned_count": returned_paths,
                "limit": path_limit,
                "limit_applied": true,
                "omitted_count": omitted_count,
                "omitted_count_is_lower_bound": true,
                "total_available_unknown": true,
                "output_bytes": byte_count,
            }),
        );
        if max_output_bytes_exceeded {
            object.insert("status".to_string(), json!("warning"));
            object.insert(
                "warnings".to_string(),
                json!([{
                    "code": "max_output_bytes_exceeded",
                    "message": "context-pack agent JSON could not fit under --max-output-bytes without removing all compact sections",
                    "severity": "warning"
                }]),
            );
        } else {
            // A transient over-budget pass may have stamped `status: "warning"`
            // before later compaction (e.g. the agent-use wrapper shrink) got the
            // envelope under budget. The budget stamp is the only writer of
            // "warning" on this builder, so once the final size fits, restore
            // "ok" along with removing the stamp's warning row — advisory
            // sidecar notes in `warnings` must not degrade top-level status.
            if object.get("status").and_then(Value::as_str) == Some("warning") {
                object.insert("status".to_string(), json!("ok"));
            }
            if let Some(warnings) = object.get_mut("warnings").and_then(Value::as_array_mut) {
                warnings.retain(|warning| {
                    warning.get("code").and_then(Value::as_str) != Some("max_output_bytes_exceeded")
                });
            }
        }
    }
}

pub(crate) fn agent_context_path_json(path: &PathEvidence) -> Value {
    let evidence_role = path
        .metadata
        .get("evidence_role")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let classification_reason = path
        .metadata
        .get("classification_reason")
        .and_then(Value::as_str)
        .unwrap_or("missing edge source-role metadata");
    let classification_source = path
        .metadata
        .get("classification_source")
        .and_then(Value::as_str)
        .unwrap_or("fallback");
    let edge_labels = path
        .metadata
        .get("edge_labels")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let source_spans = path
        .source_spans
        .iter()
        .map(agent_source_span_json)
        .collect::<Vec<_>>();
    let edges = path
        .edges
        .iter()
        .enumerate()
        .map(|(index, (head, relation, tail))| {
            let label = edge_labels.get(index);
            let edge_role = label
                .and_then(|label| label.get("evidence_role"))
                .and_then(Value::as_str)
                .unwrap_or(evidence_role);
            let edge_reason = label
                .and_then(|label| label.get("classification_reason"))
                .and_then(Value::as_str)
                .unwrap_or(classification_reason);
            let edge_source = label
                .and_then(|label| label.get("classification_source"))
                .and_then(Value::as_str)
                .unwrap_or(classification_source);
            let mut edge = serde_json::Map::new();
            if let Some(edge_id) = label
                .and_then(|label| label.get("edge_id"))
                .and_then(Value::as_str)
            {
                edge.insert("edge_id".to_string(), json!(edge_id));
            }
            edge.insert("relation".to_string(), json!(relation.to_string()));
            edge.insert(
                "source".to_string(),
                agent_context_entity_ref_resolved(head, label, "head"),
            );
            edge.insert(
                "target".to_string(),
                agent_context_entity_ref_resolved(tail, label, "tail"),
            );
            edge.insert("exactness".to_string(), json!(path.exactness.to_string()));
            edge.insert("confidence".to_string(), json!(path.confidence));
            edge.insert("evidence_role".to_string(), json!(edge_role));
            edge.insert("classification_reason".to_string(), json!(edge_reason));
            edge.insert("classification_source".to_string(), json!(edge_source));
            if let Some(span) = path.source_spans.get(index) {
                edge.insert(
                    "source_spans".to_string(),
                    json!([agent_source_span_json(span)]),
                );
            }
            Value::Object(edge)
        })
        .collect::<Vec<_>>();
    let mut object = serde_json::Map::new();
    object.insert("path_id".to_string(), json!(path.id));
    if let Some(summary) = path.summary.as_deref() {
        object.insert("summary".to_string(), json!(summary));
    }
    object.insert("evidence_role".to_string(), json!(evidence_role));
    object.insert(
        "classification_reason".to_string(),
        json!(classification_reason),
    );
    object.insert(
        "classification_source".to_string(),
        json!(classification_source),
    );
    object.insert(
        "production_proof_eligible".to_string(),
        json!(path
            .metadata
            .get("production_proof_eligible")
            .and_then(Value::as_bool)
            .unwrap_or(false)),
    );
    object.insert(
        "relations".to_string(),
        json!(path
            .metapath
            .iter()
            .map(|relation| relation.to_string())
            .collect::<Vec<_>>()),
    );
    object.insert("edges".to_string(), json!(edges));
    object.insert("source_spans".to_string(), json!(source_spans));
    object.insert("exactness".to_string(), json!(path.exactness.to_string()));
    object.insert("confidence".to_string(), json!(path.confidence));
    let primary_file = path
        .source_spans
        .first()
        .map(|span| span.repo_relative_path.as_str());
    let path_proof_status = path
        .metadata
        .get("proof_status")
        .and_then(Value::as_str)
        .unwrap_or("proof_path_found");
    object.insert(
        "language_capability".to_string(),
        context_pack_source_language_capability_json(
            primary_file,
            "graph_path",
            path_proof_status,
            true,
            &path.exactness.to_string(),
            evidence_role,
        ),
    );
    Value::Object(object)
}

/// Resolve a proof-path edge endpoint to a human-readable symbol ref. The stored
/// PathEvidence loader attaches resolved `head_entity`/`tail_entity` objects (with
/// name/qualified_name/kind/source span) to each edge label; prefer those so proof
/// paths show real symbol names instead of opaque `repo://e/...` ids. When no
/// resolved entity is present (e.g. freshly-walked fallback edges that were never
/// persisted with name metadata) it falls back to the bare id as the name — an
/// endpoint whose `name == id` is treated as still-unresolved by the store-backed
/// `resolve_context_path_edge_names` pass. This fallback is byte-identical to the
/// historical `{id, name}` ref so it never inflates the context-pack budget; the
/// richer fields are only emitted once we actually have a real (shorter) name to
/// substitute for the opaque id.
pub(crate) fn agent_context_entity_ref_resolved(
    id: &str,
    label: Option<&Value>,
    prefix: &str,
) -> Value {
    if let Some(entity) = label
        .and_then(|label| label.get(format!("{prefix}_entity")))
        .filter(|value| value.is_object())
    {
        let name = entity
            .get("display_name")
            .or_else(|| entity.get("name"))
            .or_else(|| entity.get("symbol"))
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty() && *text != id);
        if let Some(name) = name {
            let mut object = serde_json::Map::new();
            object.insert("id".to_string(), json!(id));
            object.insert("name".to_string(), json!(name));
            object.insert("display_name".to_string(), json!(name));
            object.insert("name_unavailable".to_string(), json!(false));
            for key in ["kind", "qualified_name", "file", "source_span"] {
                if let Some(value) = entity.get(key) {
                    object.insert(key.to_string(), value.clone());
                }
            }
            return Value::Object(object);
        }
    }
    json!({
        "id": id,
        "name": id,
    })
}

/// Batch-resolve proof-path edge endpoints whose name is still the opaque entity id
/// (i.e. `name == id`, the unresolved fallback shape) to their real symbol names by a
/// single read-only store lookup keyed on entity id. Patches the `source`/`target`
/// objects in place, substituting the short real name for the long opaque id (so the
/// envelope shrinks rather than grows). Silently no-ops when no ids need resolving or
/// the DB cannot be opened read-only, so fixture/unit callers and the proof boundary
/// are unaffected.
pub(crate) fn resolve_context_path_edge_names(paths: &mut [Value], db_path: &Path) {
    // Collect endpoint ids still shown as the opaque id (name == id).
    let mut unresolved: BTreeSet<String> = BTreeSet::new();
    for path in paths.iter() {
        if let Some(edges) = path.get("edges").and_then(Value::as_array) {
            for edge in edges {
                for side in ["source", "target"] {
                    if let Some(id) = context_path_endpoint_unresolved_id(edge.get(side)) {
                        unresolved.insert(id);
                    }
                }
            }
        }
    }
    if unresolved.is_empty() {
        return;
    }
    let Ok(connection) = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return;
    };
    let ids: Vec<String> = unresolved.into_iter().collect();
    let placeholders = sql_placeholders(ids.len());
    let sql = format!(
        "SELECT oid.value AS id, name.value AS name, qname.value AS qualified_name,
                kind.value AS kind
         FROM object_id_lookup oid
         JOIN entities e ON e.id_key = oid.id
         LEFT JOIN symbol_dict name ON name.id = e.name_id
         LEFT JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
         LEFT JOIN entity_kind_dict kind ON kind.id = e.kind_id
         WHERE oid.value IN ({placeholders})"
    );
    let mut resolved: BTreeMap<String, (String, Option<String>, Option<String>)> = BTreeMap::new();
    if let Ok(mut statement) = connection.prepare(&sql) {
        let rows = statement.query_map(rusqlite::params_from_iter(ids.iter()), |row| {
            let id: String = row.get("id")?;
            let name: Option<String> = row.get("name").ok();
            let qualified_name: Option<String> = row.get("qualified_name").ok();
            let kind: Option<String> = row.get("kind").ok();
            Ok((id, name, qualified_name, kind))
        });
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                if let Some(name) = row.1.filter(|n| !n.trim().is_empty()) {
                    resolved.insert(row.0, (name, row.2, row.3));
                }
            }
        }
    }
    if resolved.is_empty() {
        return;
    }
    for path in paths.iter_mut() {
        let Some(edges) = path.get_mut("edges").and_then(Value::as_array_mut) else {
            continue;
        };
        for edge in edges.iter_mut() {
            for side in ["source", "target"] {
                let Some(id) = context_path_endpoint_unresolved_id(edge.get(side)) else {
                    continue;
                };
                let Some((name, qualified_name, kind)) = resolved.get(&id) else {
                    continue;
                };
                let Some(endpoint) = edge.get_mut(side).and_then(Value::as_object_mut) else {
                    continue;
                };
                endpoint.insert("name".to_string(), json!(name));
                endpoint.insert("display_name".to_string(), json!(name));
                endpoint.insert("name_unavailable".to_string(), json!(false));
                if let Some(qualified_name) = qualified_name {
                    endpoint.insert("qualified_name".to_string(), json!(qualified_name));
                }
                if let Some(kind) = kind {
                    endpoint.insert("kind".to_string(), json!(kind));
                }
            }
        }
    }
}

/// Return the entity id of a proof-path edge endpoint that is still unresolved — i.e.
/// its `name` is just the opaque `id` (the byte-minimal fallback shape). Returns
/// `None` for already-resolved endpoints (real name substituted) or malformed ones.
pub(crate) fn context_path_endpoint_unresolved_id(endpoint: Option<&Value>) -> Option<String> {
    let endpoint = endpoint?;
    let id = endpoint.get("id").and_then(Value::as_str)?;
    let name = endpoint.get("name").and_then(Value::as_str)?;
    if name == id {
        Some(id.to_string())
    } else {
        None
    }
}

pub(crate) fn agent_context_snippet_json(
    snippet: &ContextSnippet,
    paths: &[PathEvidence],
    fallback_evidence: &[Value],
) -> Value {
    let label = context_snippet_label(snippet, paths, fallback_evidence);
    let mut object = serde_json::Map::new();
    object.insert("file".to_string(), json!(snippet.file));
    object.insert("lines".to_string(), json!(snippet.lines));
    object.insert("text".to_string(), json!(snippet.text));
    object.insert("reason".to_string(), json!(snippet.reason));
    object.insert("evidence_role".to_string(), json!(label.role));
    object.insert(
        "classification_reason".to_string(),
        json!(label.classification_reason),
    );
    object.insert(
        "proof_path_available".to_string(),
        json!(label.proof_path_available),
    );
    object.insert(
        "proof_status".to_string(),
        json!(if label.proof_path_available {
            "proof_path_found"
        } else if label.fallback_source.is_some() {
            "no_proof_path_found"
        } else {
            "unknown"
        }),
    );
    object.insert("graph_proof".to_string(), json!(label.proof_path_available));
    if !label.proof_path_available {
        object.insert("graph_relation_claims".to_string(), json!([]));
    }
    if label.role == "text_evidence" {
        object.insert(
            "claimability".to_string(),
            text_evidence_claimability_json(),
        );
    }
    if let Some(source) = label.fallback_source {
        object.insert("fallback_source".to_string(), json!(source));
    }
    if let Some(symbol) = label.fallback_symbol {
        object.insert("fallback_symbol".to_string(), json!(symbol));
    }
    let planning_role = context_planning_role_for_text_evidence(
        &snippet.file,
        &snippet.file,
        "snippet",
        &format!("{}\n{}", snippet.reason, snippet.text),
    );
    object.insert("planning_role".to_string(), json!(planning_role));
    object.insert(
        "role_rank_reason".to_string(),
        json!(if context_planning_central_file_score(&snippet.file) < 10 {
            format!("{planning_role}: central Buildroot planning snippet")
        } else {
            format!("{planning_role}: snippet retained under snippet budget")
        }),
    );
    object.insert(
        "language_capability".to_string(),
        context_pack_source_language_capability_json(
            Some(&snippet.file),
            if label.role == "text_evidence" {
                "text_evidence"
            } else {
                "source_navigation_evidence"
            },
            object
                .get("proof_status")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            label.proof_path_available,
            "source_span",
            label.role,
        ),
    );
    Value::Object(object)
}

#[derive(Debug)]
pub(crate) struct ContextSnippetLabel {
    role: &'static str,
    classification_reason: String,
    fallback_source: Option<String>,
    fallback_symbol: Option<String>,
    proof_path_available: bool,
}

pub(crate) fn context_snippet_label(
    snippet: &ContextSnippet,
    paths: &[PathEvidence],
    fallback_evidence: &[Value],
) -> ContextSnippetLabel {
    if let Some((role, reason)) = context_snippet_proof_role(snippet, paths) {
        return ContextSnippetLabel {
            role,
            classification_reason: reason,
            fallback_source: None,
            fallback_symbol: None,
            proof_path_available: true,
        };
    }
    if let Some((role, reason, source, symbol)) =
        context_snippet_fallback_role(snippet, fallback_evidence)
    {
        return ContextSnippetLabel {
            role,
            classification_reason: reason,
            fallback_source: Some(source),
            fallback_symbol: symbol,
            proof_path_available: false,
        };
    }
    ContextSnippetLabel {
        role: "unknown",
        classification_reason:
            "snippet did not match a returned proof path or fallback source span".to_string(),
        fallback_source: None,
        fallback_symbol: None,
        proof_path_available: false,
    }
}

pub(crate) fn context_snippet_proof_role(
    snippet: &ContextSnippet,
    paths: &[PathEvidence],
) -> Option<(&'static str, String)> {
    let (start, end) = parse_context_snippet_lines(&snippet.lines)?;
    for path in paths {
        let role = path
            .metadata
            .get("evidence_role")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        for span in &path.source_spans {
            if span.repo_relative_path == snippet.file
                && span.start_line <= end
                && span.end_line >= start
            {
                let reason = path
                    .metadata
                    .get("classification_reason")
                    .and_then(Value::as_str)
                    .unwrap_or("matched proof path source span")
                    .to_string();
                return Some((context_pack_role_label(role), reason));
            }
        }
    }
    None
}

pub(crate) fn context_snippet_fallback_role(
    snippet: &ContextSnippet,
    fallback_evidence: &[Value],
) -> Option<(&'static str, String, String, Option<String>)> {
    let (start, end) = parse_context_snippet_lines(&snippet.lines)?;
    let mut overlap_candidate = None;
    for evidence in fallback_evidence {
        let Some(span) = evidence
            .get("source_span")
            .or_else(|| evidence.get("span"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        let Some(file) = span.get("file").and_then(Value::as_str) else {
            continue;
        };
        if file != snippet.file {
            continue;
        }
        let Some(span_start) = span.get("start_line").and_then(Value::as_u64) else {
            continue;
        };
        let span_end = span
            .get("end_line")
            .and_then(Value::as_u64)
            .unwrap_or(span_start);
        if span_start as u32 <= end && span_end as u32 >= start {
            let role = evidence
                .get("evidence_role")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let reason = evidence
                .get("classification_reason")
                .and_then(Value::as_str)
                .unwrap_or("matched fallback source span")
                .to_string();
            let source = evidence
                .get("fallback_source")
                .and_then(Value::as_str)
                .unwrap_or("source_span")
                .to_string();
            let symbol = evidence
                .get("symbol")
                .and_then(Value::as_str)
                .map(str::to_string);
            let candidate = (context_pack_role_label(role), reason, source, symbol);
            if span_start as u32 == start && span_end as u32 == end {
                return Some(candidate);
            }
            if overlap_candidate.is_none() {
                overlap_candidate = Some(candidate);
            }
        }
    }
    overlap_candidate
}

pub(crate) fn context_pack_role_label(value: &str) -> &'static str {
    match value {
        "text_evidence" => return "text_evidence",
        "fallback" => return "fallback",
        "diagnostic" => return "diagnostic",
        "graph_proof" => return "graph_proof",
        _ => {}
    }
    match context_pack_role_from_label(value).unwrap_or(EvidenceRole::Unknown) {
        EvidenceRole::Production => "production",
        EvidenceRole::Test => "test",
        EvidenceRole::Mock => "mock",
        EvidenceRole::Mixed => "mixed",
        EvidenceRole::Unknown => "unknown",
    }
}

pub(crate) fn parse_context_snippet_lines(value: &str) -> Option<(u32, u32)> {
    if let Some((start, end)) = value.split_once('-') {
        let start = start.trim().parse::<u32>().ok()?;
        let end = end.trim().parse::<u32>().ok()?;
        return Some((start, end));
    }
    let line = value.trim().parse::<u32>().ok()?;
    Some((line, line))
}

pub(crate) fn recommended_tests_from_path_evidence(paths: &[PathEvidence]) -> Vec<String> {
    let mut tests = BTreeSet::new();
    for path in paths {
        for (head, relation, tail) in &path.edges {
            if matches!(
                relation,
                RelationKind::Tests
                    | RelationKind::Covers
                    | RelationKind::Asserts
                    | RelationKind::Mocks
                    | RelationKind::Stubs
                    | RelationKind::FixturesFor
            ) {
                tests.insert(format!("run tests covering {tail}"));
                tests.insert(format!("inspect test evidence from {head}"));
            }
        }
    }
    tests.into_iter().take(12).collect()
}

pub(crate) fn risks_from_path_evidence(paths: &[PathEvidence]) -> Vec<String> {
    let mut risks = BTreeSet::new();
    for path in paths {
        if path.metapath.iter().any(|relation| {
            matches!(
                relation,
                RelationKind::Writes
                    | RelationKind::Mutates
                    | RelationKind::MayMutate
                    | RelationKind::FlowsTo
            )
        }) {
            risks.insert(
                "mutation or dataflow evidence is included; verify affected callers and tests"
                    .to_string(),
            );
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
            risks.insert("security-sensitive proof path is included; preserve exact auth/sanitizer semantics".to_string());
        }
    }
    risks.into_iter().collect()
}

pub(crate) fn load_context_sources_and_snippets(
    repo_root: &Path,
    spans: &[SourceSpan],
    max_snippets: usize,
) -> Result<(BTreeMap<String, String>, Vec<ContextSnippet>, usize, usize), String> {
    let (sources, snippets, source_bytes, source_files_loaded, _) =
        load_context_sources_and_snippets_capped(
            repo_root,
            spans,
            max_snippets,
            usize::MAX,
            usize::MAX,
        )?;
    Ok((sources, snippets, source_bytes, source_files_loaded))
}

pub(crate) fn load_context_sources_and_snippets_capped(
    repo_root: &Path,
    spans: &[SourceSpan],
    max_snippets: usize,
    max_source_files: usize,
    max_source_bytes: usize,
) -> Result<
    (
        BTreeMap<String, String>,
        Vec<ContextSnippet>,
        usize,
        usize,
        bool,
    ),
    String,
> {
    let mut file_ids = spans
        .iter()
        .map(|span| span.repo_relative_path.clone())
        .collect::<Vec<_>>();
    file_ids.sort();
    file_ids.dedup();

    let mut sources = BTreeMap::new();
    let mut source_bytes = 0usize;
    let mut budget_hit = false;
    for file_id in file_ids {
        if sources.len() >= max_source_files {
            budget_hit = true;
            break;
        }
        let path = repo_root.join(&file_id);
        if path.exists() {
            let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
            if source_bytes.saturating_add(source.len()) > max_source_bytes {
                budget_hit = true;
                continue;
            }
            source_bytes += source.len();
            sources.insert(file_id, source);
        }
    }

    let mut snippets = Vec::new();
    let mut seen = BTreeSet::new();
    for span in spans {
        if snippets.len() >= max_snippets {
            break;
        }
        let key = format!(
            "{}:{}:{}",
            span.repo_relative_path, span.start_line, span.end_line
        );
        if !seen.insert(key) {
            continue;
        }
        let Some(source) = sources.get(&span.repo_relative_path) else {
            continue;
        };
        let text = source_snippet_for_span(source, span);
        if text.trim().is_empty() {
            continue;
        }
        snippets.push(ContextSnippet {
            file: span.repo_relative_path.clone(),
            lines: if span.start_line == span.end_line {
                span.start_line.to_string()
            } else {
                format!("{}-{}", span.start_line, span.end_line)
            },
            text,
            reason: "proof path source span".to_string(),
        });
    }
    let source_files_loaded = sources.len();
    Ok((
        sources,
        snippets,
        source_bytes,
        source_files_loaded,
        budget_hit,
    ))
}

pub(crate) fn context_source_spans_for_paths(paths: &[PathEvidence]) -> Vec<SourceSpan> {
    let mut spans = paths
        .iter()
        .flat_map(|path| path.source_spans.iter().cloned())
        .collect::<Vec<_>>();
    spans.sort_by(|left, right| {
        left.repo_relative_path
            .cmp(&right.repo_relative_path)
            .then_with(|| left.start_line.cmp(&right.start_line))
            .then_with(|| left.end_line.cmp(&right.end_line))
    });
    spans.dedup();
    spans
}

pub(crate) fn source_snippet_for_span(source: &str, span: &SourceSpan) -> String {
    let lines = source.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return String::new();
    }
    let start = span.start_line.saturating_sub(2).max(1) as usize;
    let end = (span.end_line as usize + 2).min(lines.len());
    if start > end {
        return String::new();
    }
    (start..=end)
        .filter_map(|line_number| {
            lines
                .get(line_number - 1)
                .map(|line| format!("{line_number}: {line}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn compact_context_packet_for_cli(packet: &mut ContextPacket, token_budget: usize) {
    // Track the packet's serialized byte length incrementally instead of
    // re-serializing the whole packet on every iteration (the loop popped one
    // item at a time, so the old shape was O(N^2) on a hot agent path). This is
    // byte-exact for compact serde_json: array elements are comma-joined with no
    // padding, and we only pop the last element of an array that still has >1
    // element, so each drop removes precisely `,<element>` (one separator byte
    // plus the element's own serialized bytes). The token estimate therefore
    // stays identical to a full re-serialize and the loop terminates at exactly
    // the same point as before. On the (practically impossible) serialization
    // failure of a single element we resync with a full re-serialize so the
    // estimate can never drift.
    let mut serialized_bytes = serialized_context_packet_len(packet);
    while estimated_tokens_from_bytes(serialized_bytes) > token_budget {
        if packet.snippets.len() > 1 {
            serialized_bytes = adjust_for_dropped_element(packet, serialized_bytes, |p| {
                serialized_element_len(p.snippets.pop())
            });
            continue;
        }
        if packet.verified_paths.len() > 1 {
            serialized_bytes = adjust_for_dropped_element(packet, serialized_bytes, |p| {
                serialized_element_len(p.verified_paths.pop())
            });
            continue;
        }
        if packet.recommended_tests.len() > 1 {
            serialized_bytes = adjust_for_dropped_element(packet, serialized_bytes, |p| {
                serialized_element_len(p.recommended_tests.pop())
            });
            continue;
        }
        if packet.risks.len() > 1 {
            serialized_bytes = adjust_for_dropped_element(packet, serialized_bytes, |p| {
                serialized_element_len(p.risks.pop())
            });
            continue;
        }
        break;
    }
    packet.metadata.insert(
        "estimated_tokens".to_string(),
        json!(estimate_context_packet_tokens(packet)),
    );
}

/// Drop the last element of one packet array via `drop_element`, which returns the
/// dropped element's serialized byte length, then return the packet's new
/// serialized byte length. The dropped element's length plus one separator byte is
/// subtracted from `current_bytes`; if the element could not be serialized (which
/// cannot happen once the initial full serialize succeeded) we resync with a full
/// re-serialize so the running estimate can never silently drift from the truth.
pub(crate) fn adjust_for_dropped_element(
    packet: &mut ContextPacket,
    current_bytes: usize,
    drop_element: impl FnOnce(&mut ContextPacket) -> Option<usize>,
) -> usize {
    match drop_element(packet) {
        Some(element_bytes) => current_bytes.saturating_sub(element_bytes + 1),
        None => serialized_context_packet_len(packet),
    }
}

pub(crate) fn serialized_element_len<T: serde::Serialize>(element: Option<T>) -> Option<usize> {
    element.and_then(|value| {
        serde_json::to_string(&value)
            .ok()
            .map(|serialized| serialized.len())
    })
}

pub(crate) fn serialized_context_packet_len(packet: &ContextPacket) -> usize {
    serde_json::to_string(packet)
        .map(|serialized| serialized.len())
        .unwrap_or(0)
}

pub(crate) fn estimated_tokens_from_bytes(serialized_bytes: usize) -> usize {
    (serialized_bytes / 4).max(1)
}

pub(crate) fn estimate_context_packet_tokens(packet: &ContextPacket) -> usize {
    estimated_tokens_from_bytes(serialized_context_packet_len(packet))
}

pub(crate) fn path_evidence_from_sql_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PathEvidence> {
    let metapath_json: String = row.get("metapath_json")?;
    let edges_json: String = row.get("edges_json")?;
    let source_spans_json: String = row.get("source_spans_json")?;
    let exactness: String = row.get("exactness")?;
    let metadata_json: String = row.get("metadata_json")?;
    Ok(PathEvidence {
        id: row.get("id")?,
        source: row.get("source")?,
        target: row.get("target")?,
        summary: row.get("summary")?,
        metapath: serde_json::from_str(&metapath_json).map_err(sql_json_error)?,
        edges: serde_json::from_str(&edges_json).map_err(sql_json_error)?,
        source_spans: serde_json::from_str(&source_spans_json).map_err(sql_json_error)?,
        exactness: exactness.parse().map_err(sql_parse_error)?,
        length: row.get("length")?,
        confidence: row.get("confidence")?,
        metadata: serde_json::from_str(&metadata_json).map_err(sql_json_error)?,
    })
}

/// Resolve the canonical id for a context fallback edge. When the stored id is
/// absent or blank (e.g. a dangling/pruned `id_key`, or a derived edge that was
/// never assigned a persisted object id), recompute the same deterministic id the
/// store mints from the edge's own components. This keeps distinct id-less edges
/// distinct so they survive the `dedup_by(|l, r| l.id == r.id)` in
/// `load_bounded_context_edges` instead of all collapsing onto one shared
/// `"edge://unknown"` placeholder (which silently drops real graph neighbors from
/// the agent context pack). Reconstruction from real components, not fabrication.
pub(crate) fn resolve_context_edge_id(
    stored_id: Option<String>,
    head_id: &str,
    relation: RelationKind,
    tail_id: &str,
    source_span: &SourceSpan,
) -> String {
    stored_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| stable_edge_id(head_id, relation, tail_id, source_span))
}

pub(crate) fn edge_from_context_sql_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Edge> {
    let relation: RelationKind = row
        .get::<_, String>("relation")?
        .parse()
        .map_err(sql_parse_error)?;
    let exactness: String = row.get("exactness")?;
    let edge_class: Option<String> = row.get("edge_class")?;
    let context: Option<String> = row.get("context")?;
    let provenance_edges_json: String = row.get("provenance_edges_json")?;
    let metadata_json: String = row.get("metadata_json")?;
    let head_id: String = row.get("head_id")?;
    let tail_id: String = row.get("tail_id")?;
    let source_span = SourceSpan {
        repo_relative_path: row.get("span_repo_relative_path")?,
        start_line: row.get("start_line")?,
        start_column: row.get("start_column")?,
        end_line: row.get("end_line")?,
        end_column: row.get("end_column")?,
    };
    let id = resolve_context_edge_id(
        row.get::<_, Option<String>>("id")?,
        &head_id,
        relation,
        &tail_id,
        &source_span,
    );
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
        exactness: exactness.parse().map_err(sql_parse_error)?,
        edge_class: edge_class
            .as_deref()
            .unwrap_or("unknown")
            .parse()
            .map_err(sql_parse_error)?,
        context: context
            .as_deref()
            .unwrap_or("unknown")
            .parse()
            .map_err(sql_parse_error)?,
        derived: row.get::<_, i64>("derived")? != 0,
        provenance_edges: serde_json::from_str(&provenance_edges_json).map_err(sql_json_error)?,
        metadata: serde_json::from_str(&metadata_json).map_err(sql_json_error)?,
    })
}
