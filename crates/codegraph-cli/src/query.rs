//! Query command implementations (symbols/text/files/definitions/references/
//! chain/unresolved-calls/path/callers/callees) and their agent-json response
//! formatting + symbol/path resolution helpers.
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use codegraph_core::{Edge, Entity, Exactness, FileRecord, PathEvidence, RelationKind, SourceSpan};
use codegraph_query::{
    ExactGraphQueryEngine, GraphPath, QueryLimits, SymbolSearchHit, TraversalDirection,
};
use codegraph_store::{GraphStore, SqliteGraphStore, TextSearchKind};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::*;

pub(crate) fn query_symbols(repo_root: &Path, query: &str, limit: usize) -> Result<Value, String> {
    query_symbols_with_options(repo_root, &QueryListOptions::rich(query, limit), None)
}

pub(crate) fn query_symbols_with_options(
    repo_root: &Path,
    options: &QueryListOptions,
    lifecycle_summary: Option<&Value>,
) -> Result<Value, String> {
    let started = Instant::now();
    let db_path = resolved_db_path_for_repo(repo_root);
    let store = open_existing_store(repo_root)?;
    let hits = symbol_search_hits(&store, &options.query, options.fetch_limit())?;

    if options.output_mode.is_compact() {
        let (hits, truncation) = truncate_for_agent(hits, options.limit);
        let results = hits
            .iter()
            .map(agent_symbol_search_hit_json)
            .collect::<Vec<_>>();
        return Ok(canonical_agent_query_response(
            "query_symbols_agent_json",
            "query symbols",
            repo_root,
            &db_path,
            "ok",
            lifecycle_summary,
            truncation,
            json!({
                "text": options.query,
                "explicit_limit": options.explicit_limit,
            }),
            results,
            Vec::new(),
            Vec::new(),
            options.output_mode,
            agent_timings_json(started),
        ));
    }

    let hits = hits
        .into_iter()
        .take(options.limit)
        .map(|hit| symbol_search_hit_json(&hit))
        .collect::<Vec<_>>();

    Ok(json!({
        "status": "ok",
        "query": options.query,
        "result_count": hits.len(),
        "limit": options.limit,
        "explicit_limit": options.explicit_limit,
        "output_mode": options.output_mode.as_str(),
        "verbose": options.verbose,
        "debug": options.debug,
        "explain": options.explain,
        "hits": hits,
        "ranking": [
            "exact symbol match",
            "qualified-name match",
            "prefix match",
            "fuzzy match",
            "file path proximity",
        "same package/module",
        "recent edit signal",
        "source/test role",
        "aliases/import names",
        "Stage 0 file-path FTS candidates",
        "bounded semantic SQL fallback"
    ],
        "proof": "Symbol search is ranked over exact symbol hits, file-path FTS candidates, and a bounded semantic SQL fallback.",
    }))
}

pub(crate) fn symbol_search_hits(
    store: &SqliteGraphStore,
    query: &str,
    limit: usize,
) -> Result<Vec<SymbolSearchHit>, String> {
    let entities = symbol_search_candidate_entities(store, query, limit)?;
    let mut file_paths = BTreeSet::new();
    for entity in &entities {
        file_paths.insert(entity.repo_relative_path.clone());
    }
    let mut files = Vec::new();
    for repo_relative_path in file_paths {
        if let Some(file) = store
            .get_file(&repo_relative_path)
            .map_err(|error| error.to_string())?
        {
            files.push(file);
        }
    }

    Ok(SymbolSearchIndex::new(entities, Vec::new(), files).search(query, limit))
}

pub(crate) fn symbol_search_candidate_entities(
    store: &SqliteGraphStore,
    query: &str,
    limit: usize,
) -> Result<Vec<Entity>, String> {
    let candidate_limit = limit
        .max(1)
        .saturating_mul(SYMBOL_SEARCH_FTS_CANDIDATE_FACTOR)
        .clamp(
            SYMBOL_SEARCH_MIN_FTS_CANDIDATES,
            SYMBOL_SEARCH_MAX_FTS_CANDIDATES,
        );
    let mut seen = BTreeSet::new();
    let mut entities = Vec::new();

    for entity in store
        .find_entities_by_exact_symbol(query)
        .map_err(|error| error.to_string())?
    {
        if seen.insert(entity.id.clone()) {
            entities.push(entity);
        }
    }

    let text_hits = store
        .search_text(query, candidate_limit)
        .map_err(|error| error.to_string())?;
    let mut file_candidate_paths = BTreeSet::new();
    for hit in &text_hits {
        match hit.kind {
            TextSearchKind::Entity => {
                if seen.contains(&hit.id) {
                    continue;
                }
                if let Some(entity) = store
                    .get_entity(&hit.id)
                    .map_err(|error| error.to_string())?
                {
                    if seen.insert(entity.id.clone()) {
                        entities.push(entity);
                    }
                }
            }
            TextSearchKind::File | TextSearchKind::Snippet => {
                if file_candidate_paths.len() < SYMBOL_SEARCH_FILE_CANDIDATE_LIMIT {
                    file_candidate_paths.insert(hit.repo_relative_path.clone());
                }
            }
        }
        if entities.len() >= candidate_limit {
            break;
        }
    }

    if entities.len() < limit {
        let query_lc = query.to_ascii_lowercase();
        let query_aliases = split_symbol_query_aliases(query);
        for repo_relative_path in file_candidate_paths {
            for entity in store
                .list_entities_by_file(&repo_relative_path)
                .map_err(|error| error.to_string())?
            {
                if seen.contains(&entity.id) {
                    continue;
                }
                if !symbol_candidate_matches(&entity, &query_lc, &query_aliases) {
                    continue;
                }
                if seen.insert(entity.id.clone()) {
                    entities.push(entity);
                }
                if entities.len() >= candidate_limit {
                    break;
                }
            }
            if entities.len() >= candidate_limit {
                break;
            }
        }
    }

    if entities.len() < limit && !agent_use_bounded_read_path_enabled() {
        let query_lc = query.to_ascii_lowercase();
        let query_aliases = split_symbol_query_aliases(query);
        for entity in store
            .list_entities(UNBOUNDED_STORE_READ_LIMIT)
            .map_err(|error| error.to_string())?
        {
            if seen.contains(&entity.id) {
                continue;
            }
            if !symbol_candidate_matches(&entity, &query_lc, &query_aliases) {
                continue;
            }
            if seen.insert(entity.id.clone()) {
                entities.push(entity);
            }
            if entities.len() >= candidate_limit {
                break;
            }
        }
    }

    Ok(entities)
}

pub(crate) fn split_symbol_query_aliases(query: &str) -> BTreeSet<String> {
    query
        .to_ascii_lowercase()
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|token| token.len() >= 2)
        .map(str::to_string)
        .collect()
}

pub(crate) fn symbol_candidate_matches(
    entity: &Entity,
    query_lc: &str,
    query_aliases: &BTreeSet<String>,
) -> bool {
    let metadata = entity_metadata_search_text(entity).to_ascii_lowercase();
    let haystack = format!(
        "{} {} {} {}",
        entity.name.to_ascii_lowercase(),
        entity.qualified_name.to_ascii_lowercase(),
        entity.repo_relative_path.to_ascii_lowercase(),
        metadata
    );
    haystack.contains(query_lc)
        || query_aliases
            .iter()
            .any(|alias| alias.len() >= 2 && haystack.contains(alias))
}

pub(crate) fn entity_metadata_search_text(entity: &Entity) -> String {
    entity
        .metadata
        .iter()
        .flat_map(|(key, value)| match value {
            Value::String(text) => vec![key.clone(), text.clone()],
            Value::Bool(value) => vec![key.clone(), value.to_string()],
            Value::Number(value) => vec![key.clone(), value.to_string()],
            _ => vec![key.clone()],
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Honesty disclosure for the agent-use bounded read path.
///
/// Under `agent-use`, the Stage-0 FTS store holds no full source body — file
/// rows match on path only, and content recall flows exclusively through
/// indexed entity text and snippet spans (the deliberate DB-only,
/// `AGENT_USE_DISK_FALLBACK_MAX_FILES = 0` determinism contract). That means
/// `query text` / `query files` are NOT an exhaustive grep: imports, top-level
/// comments, license headers, non-entity constants outside any entity span, and
/// text-evidence lines beyond the per-file snippet cap are not matchable here.
/// This signal tells a consuming agent to fall back to a full-text search (e.g.
/// ripgrep) when it needs exhaustive content coverage rather than over-trusting
/// an empty result as "definitely absent".
fn bounded_recall_scope_disclosure() -> Value {
    json!({
        "exhaustive": false,
        "scope": "entity_and_snippet_spans",
        "excludes": [
            "imports",
            "top_level_comments",
            "license_headers",
            "non_entity_constants_outside_entity_spans",
            "text_evidence_lines_beyond_snippet_cap",
        ],
        "reason": "agent-use bounded read path stores no full source body in SQLite; file FTS rows match path only",
        "recommended_fallback": "use full-text search (e.g. ripgrep) for exhaustive content matches; an empty result here is not proof of absence",
    })
}

/// Attach the bounded-recall disclosure to a response object, but only when the
/// agent-use bounded read path is active. Non-bounded callers (which still have
/// the on-demand source-scan backstop) keep exhaustive recall and get no signal.
fn attach_recall_scope_disclosure(response: &mut Value) {
    if !agent_use_bounded_read_path_enabled() {
        return;
    }
    if let Some(object) = response.as_object_mut() {
        object.insert(
            "recall_scope".to_string(),
            bounded_recall_scope_disclosure(),
        );
    }
}

pub(crate) fn query_text_with_options(
    repo_root: &Path,
    options: &QueryListOptions,
    lifecycle_summary: Option<&Value>,
) -> Result<Value, String> {
    let started = Instant::now();
    let db_path = resolved_db_path_for_repo(repo_root);
    let store = open_existing_store(repo_root)?;
    let mut hits = store
        .search_text(&options.query, options.fetch_limit())
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(text_search_hit_json)
        .collect::<Vec<_>>();
    if hits.len() < options.fetch_limit() && !agent_use_bounded_read_path_enabled() {
        hits.extend(source_scan_text_hits(
            repo_root,
            &store,
            &options.query,
            options.fetch_limit().saturating_sub(hits.len()),
        )?);
    }

    if options.output_mode.is_compact() {
        let (hits, truncation) = truncate_for_agent(hits, options.limit);
        let results = hits.iter().map(agent_text_hit_json).collect::<Vec<_>>();
        let mut response = canonical_agent_query_response(
            "query_text_agent_json",
            "query text",
            repo_root,
            &db_path,
            "ok",
            lifecycle_summary,
            truncation,
            json!({
                "text": options.query,
                "explicit_limit": options.explicit_limit,
            }),
            results,
            Vec::new(),
            Vec::new(),
            options.output_mode,
            agent_timings_json(started),
        );
        attach_recall_scope_disclosure(&mut response);
        return Ok(response);
    }

    hits.truncate(options.limit);

    let mut response = json!({
        "status": "ok",
        "query": options.query,
        "result_count": hits.len(),
        "limit": options.limit,
        "explicit_limit": options.explicit_limit,
        "output_mode": options.output_mode.as_str(),
        "verbose": options.verbose,
        "debug": options.debug,
        "explain": options.explain,
        "hits": hits,
        "proof": "Text query uses SQLite FTS when present and falls back to bounded on-demand source scanning over indexed files.",
    });
    attach_recall_scope_disclosure(&mut response);
    Ok(response)
}

pub(crate) fn source_scan_text_hits(
    repo_root: &Path,
    store: &SqliteGraphStore,
    query: &str,
    limit: usize,
) -> Result<Vec<Value>, String> {
    if query.trim().is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let query_lc = query.to_ascii_lowercase();
    let mut hits = Vec::new();
    for file in store
        .list_files(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?
    {
        let path = repo_root.join(&file.repo_relative_path);
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        for (line_index, line) in source.lines().enumerate() {
            if !line.to_ascii_lowercase().contains(&query_lc) {
                continue;
            }
            let mut hit = json!({
                "kind": "file",
                "id": file.repo_relative_path,
                "repo_relative_path": file.repo_relative_path,
                "line": line_index + 1,
                "title": file.repo_relative_path,
                "text": line.trim(),
                "score": 0.0,
                "match": "source_scan",
            });
            if let Some(object) = hit.as_object_mut() {
                insert_text_evidence_labels(object);
            }
            hits.push(hit);
            break;
        }
        if hits.len() >= limit {
            break;
        }
    }
    Ok(hits)
}

pub(crate) fn query_files_with_options(
    repo_root: &Path,
    options: &QueryListOptions,
    lifecycle_summary: Option<&Value>,
) -> Result<Value, String> {
    let started = Instant::now();
    let db_path = resolved_db_path_for_repo(repo_root);
    let store = open_existing_store(repo_root)?;
    let query_lc = options.query.to_ascii_lowercase();
    let query_aliases = split_file_query_aliases(&options.query);
    let mut seen = BTreeSet::new();
    let mut hits = Vec::new();
    for hit in store
        .search_text(&options.query, options.fetch_limit())
        .map_err(|error| error.to_string())?
    {
        if hit.kind == TextSearchKind::File && seen.insert(hit.repo_relative_path.clone()) {
            let repo_relative_path = hit.repo_relative_path.clone();
            let mut value = text_search_hit_json(hit);
            if let Some(file) = store
                .get_file(&repo_relative_path)
                .map_err(|error| error.to_string())?
            {
                enrich_file_hit_with_text_evidence_preview(
                    repo_root,
                    &file,
                    &options.query,
                    &mut value,
                )?;
            }
            hits.push(value);
        }
    }
    if !agent_use_bounded_read_path_enabled() {
        for file in store
            .list_files(UNBOUNDED_STORE_READ_LIMIT)
            .map_err(|error| error.to_string())?
        {
            if hits.len() >= options.fetch_limit() {
                break;
            }
            if file_record_matches_file_query(&file, &query_lc, &query_aliases)
                && seen.insert(file.repo_relative_path.clone())
            {
                let mut hit = json!({
                    "kind": "file",
                    "id": file.repo_relative_path.clone(),
                    "repo_relative_path": file.repo_relative_path.clone(),
                    "line": null,
                    "title": file.repo_relative_path.clone(),
                    "score": 0.0,
                    "match": "path_contains",
                });
                enrich_file_hit_with_text_evidence_preview(
                    repo_root,
                    &file,
                    &options.query,
                    &mut hit,
                )?;
                hits.push(hit);
            }
        }
    }

    if options.output_mode.is_compact() {
        let (hits, truncation) = truncate_for_agent(hits, options.limit);
        let results = hits.iter().map(agent_file_hit_json).collect::<Vec<_>>();
        let mut response = canonical_agent_query_response(
            "query_files_agent_json",
            "query files",
            repo_root,
            &db_path,
            "ok",
            lifecycle_summary,
            truncation,
            json!({
                "text": options.query,
                "explicit_limit": options.explicit_limit,
            }),
            results,
            Vec::new(),
            Vec::new(),
            options.output_mode,
            agent_timings_json(started),
        );
        attach_recall_scope_disclosure(&mut response);
        return Ok(response);
    }

    hits.truncate(options.limit);

    let mut response = json!({
        "status": "ok",
        "query": options.query,
        "result_count": hits.len(),
        "limit": options.limit,
        "explicit_limit": options.explicit_limit,
        "output_mode": options.output_mode.as_str(),
        "verbose": options.verbose,
        "debug": options.debug,
        "explain": options.explain,
        "hits": hits,
        "proof": "File query combines SQLite FTS file-path rows with repo-relative path matching.",
    });
    attach_recall_scope_disclosure(&mut response);
    Ok(response)
}

pub(crate) fn split_file_query_aliases(query: &str) -> BTreeSet<String> {
    query
        .to_ascii_lowercase()
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')))
        .map(|token| token.trim_matches(|ch: char| matches!(ch, '-' | '.')))
        .filter(|token| token.len() >= 2)
        .map(str::to_string)
        .collect()
}

pub(crate) fn file_record_matches_file_query(
    file: &FileRecord,
    query_lc: &str,
    query_aliases: &BTreeSet<String>,
) -> bool {
    if query_lc.trim().is_empty() {
        return false;
    }
    let haystack = file_record_query_haystack(file);
    haystack.contains(query_lc)
        || query_aliases
            .iter()
            .any(|alias| alias.len() >= 2 && haystack.contains(alias))
}

pub(crate) fn file_record_query_haystack(file: &FileRecord) -> String {
    let mut parts = vec![file.repo_relative_path.to_ascii_lowercase()];
    for key in [
        "evidence_kind",
        "evidence_role",
        "proof_status",
        "source_file_kind",
        "source_file_label",
        "claim_state",
        "graph_output_claimability",
        "graph_output_degradation_labels",
        "degradation_labels",
        "graph_extraction_skip_reason",
    ] {
        if let Some(value) = file.metadata.get(key).and_then(Value::as_str) {
            parts.push(value.to_ascii_lowercase());
        } else if let Some(values) = file.metadata.get(key).and_then(Value::as_array) {
            parts.extend(
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_ascii_lowercase),
            );
        }
    }
    if let Some(tokens) = file
        .metadata
        .get("text_evidence")
        .and_then(|value| value.get("tokens"))
        .and_then(Value::as_array)
    {
        parts.extend(
            tokens
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_ascii_lowercase),
        );
    }
    parts.join(" ")
}

pub(crate) fn enrich_file_hit_with_text_evidence_preview(
    repo_root: &Path,
    file: &FileRecord,
    query: &str,
    hit: &mut Value,
) -> Result<(), String> {
    insert_file_degradation_labels(file, hit);
    if let Some(object) = hit.as_object_mut() {
        insert_query_evidence_role_labels(object, query_evidence_role_for_file(file));
    }
    if !file_record_is_text_evidence(file) {
        return Ok(());
    }
    let Some(object) = hit.as_object_mut() else {
        return Ok(());
    };
    insert_text_evidence_labels(object);
    if let Some(kind) = file
        .metadata
        .get("source_file_kind")
        .and_then(Value::as_str)
    {
        object.insert("source_file_kind".to_string(), json!(kind));
    }
    if let Some(label) = file
        .metadata
        .get("source_file_label")
        .and_then(Value::as_str)
    {
        object.insert("source_file_label".to_string(), json!(label));
    }

    let existing_text = object
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let preview = if existing_text.trim().is_empty() {
        if agent_use_bounded_read_path_enabled() {
            return Ok(());
        }
        read_bounded_query_file_preview(&repo_root.join(&file.repo_relative_path))?
            .and_then(|text| query_snippet_from_text(&text, query))
    } else {
        query_snippet_from_text(existing_text, query)
    };
    if let Some((line, snippet)) = preview {
        object.insert("line".to_string(), json!(line));
        object.insert("text".to_string(), json!(snippet));
    }
    Ok(())
}

pub(crate) fn insert_file_degradation_labels(file: &FileRecord, hit: &mut Value) {
    let Some(object) = hit.as_object_mut() else {
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
    let graph_output_claimability = metadata
        .get("graph_output_claimability")
        .and_then(Value::as_str)
        .map(str::to_string);
    let graph_relation_claims = metadata.get("graph_relation_claims").cloned();
    let diagnostic_only = metadata
        .get("diagnostic_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if graph_output_budget_hit || degradation_labels.is_some() || diagnostic_only {
        object.insert("graph_output_degraded".to_string(), json!(true));
        object.insert(
            "degradation_labels".to_string(),
            degradation_labels.unwrap_or_else(|| json!([])),
        );
        object.insert(
            "graph_output_claimability".to_string(),
            json!(graph_output_claimability
                .unwrap_or_else(|| { "degraded_file_nonclaimable_for_omitted_facts".to_string() })),
        );
        object.insert(
            "graph_relation_claims".to_string(),
            graph_relation_claims.unwrap_or_else(|| json!("partial")),
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
            "claimability".to_string(),
            json!({
                "claimable": true,
                "claimable_as": ["emitted_graph_facts_with_source_spans"],
                "not_claimable_as": ["complete_graph_for_degraded_file", "omitted_relation_classes"],
                "diagnostic_only": diagnostic_only,
                "reason": "file has degraded graph output; emitted facts remain source-span claimable, omitted facts are not claimable"
            }),
        );
    }
}

pub(crate) fn file_record_is_text_evidence(file: &FileRecord) -> bool {
    file.metadata.get("evidence_kind").and_then(Value::as_str) == Some("text_evidence")
        && file.metadata.get("proof_status").and_then(Value::as_str) == Some("not_graph_proof")
}

pub(crate) fn read_bounded_query_file_preview(path: &Path) -> Result<Option<String>, String> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut buffer = vec![0u8; QUERY_FILE_PREVIEW_MAX_BYTES];
    let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
    if read == 0 {
        return Ok(None);
    }
    buffer.truncate(read);
    Ok(Some(String::from_utf8_lossy(&buffer).into_owned()))
}

pub(crate) fn query_snippet_from_text(text: &str, query: &str) -> Option<(u64, String)> {
    let query_lc = query.trim().to_ascii_lowercase();
    if !query_lc.is_empty() {
        for (line_index, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.to_ascii_lowercase().contains(&query_lc) {
                return Some(((line_index + 1) as u64, bounded_query_snippet_text(trimmed)));
            }
        }
    }
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(((line_index + 1) as u64, bounded_query_snippet_text(trimmed)));
        }
    }
    None
}

pub(crate) fn bounded_query_snippet_text(text: &str) -> String {
    if text.len() <= QUERY_FILE_SNIPPET_MAX_BYTES {
        return text.to_string();
    }
    let mut end = QUERY_FILE_SNIPPET_MAX_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

pub(crate) fn bounded_retrieval_candidate_snippet_text(text: &str) -> (String, bool) {
    if text.len() <= RETRIEVAL_CANDIDATE_SNIPPET_MAX_BYTES {
        return (text.to_string(), false);
    }
    let mut end = RETRIEVAL_CANDIDATE_SNIPPET_MAX_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

pub(crate) fn query_definitions_with_options(
    repo_root: &Path,
    options: &QueryListOptions,
    lifecycle_summary: Option<&Value>,
) -> Result<Value, String> {
    let started = Instant::now();
    let db_path = resolved_db_path_for_repo(repo_root);
    let store = open_existing_store(repo_root)?;
    let hits = symbol_search_hits(&store, &options.query, options.fetch_limit() * 2)?
        .into_iter()
        .filter(|hit| is_definition_kind(hit.entity.kind))
        .take(options.fetch_limit())
        .collect::<Vec<_>>();

    if options.output_mode.is_compact() {
        let (hits, truncation) = truncate_for_agent(hits, options.limit);
        let results = hits
            .iter()
            .map(agent_definition_result_json)
            .collect::<Vec<_>>();
        let warnings = if results.is_empty() {
            vec![bounded_message(
                "no_definition_found",
                "No symbol definition matched the query.",
                "warning",
            )]
        } else {
            Vec::new()
        };
        return Ok(canonical_agent_query_response(
            "query_definitions_agent_json",
            "query definitions",
            repo_root,
            &db_path,
            if results.is_empty() { "warning" } else { "ok" },
            lifecycle_summary,
            truncation,
            json!({
                "text": options.query,
                "explicit_limit": options.explicit_limit,
            }),
            results,
            warnings,
            Vec::new(),
            options.output_mode,
            agent_timings_json(started),
        ));
    }

    let hits = hits.iter().map(symbol_search_hit_json).collect::<Vec<_>>();

    Ok(json!({
        "status": "ok",
        "query": options.query,
        "result_count": hits.len(),
        "limit": options.limit,
        "explicit_limit": options.explicit_limit,
        "output_mode": options.output_mode.as_str(),
        "definitions": hits,
        "graph_proof": false,
        "proof_status": if hits.is_empty() { "no_proof_path_found" } else { "symbol_definition_found" },
        "proof_strength": "symbol_definition",
        "proof": "Definitions are symbol-search hits constrained to declaration/executable entity kinds.",
    }))
}

pub(crate) fn query_references_with_options(
    repo_root: &Path,
    options: &QueryListOptions,
    lifecycle_summary: Option<&Value>,
) -> Result<Value, String> {
    let started = Instant::now();
    let db_path = resolved_db_path_for_repo(repo_root);
    let store = open_existing_store(repo_root)?;
    let entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let entity_by_id = entities_by_id(&entities);
    let seeds = resolve_symbol_candidates(&store, &options.query, 8)?;
    let seed_ids = seeds
        .iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    let seed_aliases = alias_set_for_entities(&seeds);
    let mut references = Vec::new();
    for edge in store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?
    {
        if references.len() >= options.fetch_limit() {
            break;
        }
        let head_aliases = entity_by_id
            .get(&edge.head_id)
            .map(entity_aliases)
            .unwrap_or_default();
        let tail_aliases = entity_by_id
            .get(&edge.tail_id)
            .map(entity_aliases)
            .unwrap_or_default();
        let matches_seed = seed_ids.contains(&edge.head_id)
            || seed_ids.contains(&edge.tail_id)
            || aliases_overlap(&seed_aliases, &head_aliases)
            || aliases_overlap(&seed_aliases, &tail_aliases);
        if matches_seed {
            references.push(edge);
        }
    }

    if options.output_mode.is_compact() {
        let (references, truncation) = truncate_for_agent(references, options.limit);
        let results = references
            .iter()
            .map(|edge| agent_reference_edge_json(edge, &entity_by_id))
            .collect::<Vec<_>>();
        let warnings = if results.is_empty() {
            vec![bounded_message(
                "no_references_found",
                "No graph references matched the resolved symbol candidates.",
                "warning",
            )]
        } else {
            Vec::new()
        };
        return Ok(canonical_agent_query_response(
            "query_references_agent_json",
            "query references",
            repo_root,
            &db_path,
            if results.is_empty() { "warning" } else { "ok" },
            lifecycle_summary,
            truncation,
            json!({
                "text": options.query,
                "explicit_limit": options.explicit_limit,
                "resolved_symbols": seeds.iter().map(agent_entity_ref_json).collect::<Vec<_>>(),
            }),
            results,
            warnings,
            Vec::new(),
            options.output_mode,
            agent_timings_json(started),
        ));
    }

    let graph_proof = references.iter().any(edge_is_graph_relation_proof);
    let reference_rows = references
        .iter()
        .map(|edge| edge_with_entities_json(edge, &entity_by_id))
        .collect::<Vec<_>>();

    Ok(json!({
        "status": "ok",
        "query": options.query,
        "result_count": reference_rows.len(),
        "limit": options.limit,
        "explicit_limit": options.explicit_limit,
        "output_mode": options.output_mode.as_str(),
        "resolved_symbols": seeds.iter().map(entity_json).collect::<Vec<_>>(),
        "references": reference_rows,
        "text_reference_count": 0,
        "graph_proof": graph_proof,
        "proof_status": if graph_proof { "proof_path_found" } else { "no_proof_path_found" },
        "proof_strength": if graph_proof { "graph_relation_proof" } else { "none" },
        "proof": "References are graph edges connected to resolved symbol ids or explicit same-name unresolved placeholders.",
    }))
}

pub(crate) fn query_chain_with_options(
    repo_root: &Path,
    options: &PathQueryOptions,
    lifecycle_summary: Option<&Value>,
) -> Result<Value, String> {
    let started = Instant::now();
    let db_path = resolved_db_path_for_repo(repo_root);
    let store = open_existing_store(repo_root)?;
    let entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let edges = store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let entity_by_id = entities_by_id(&entities);
    let source_entities = resolve_symbol_candidates(&store, &options.source, 8)?;
    let target_entities = resolve_symbol_candidates(&store, &options.target, 8)?;
    let target_aliases = alias_set_for_entities(&target_entities)
        .into_iter()
        .chain([normalize_symbol_alias(&options.target)])
        .collect::<BTreeSet<_>>();
    let mut limits = default_query_limits();
    limits.max_paths = options.output.fetch_limit();
    let paths = call_chain_paths(
        &edges,
        &entity_by_id,
        &source_entities,
        &target_aliases,
        limits,
    );
    let engine = ExactGraphQueryEngine::new(edges);
    let mut evidence = engine.path_evidence_from_paths(&paths);
    hydrate_path_evidence_endpoint_labels(&mut evidence, &entity_by_id);
    if options.output.output_mode.is_compact() {
        return Ok(agent_path_query_response(
            "query_chain_agent_json",
            "query chain",
            repo_root,
            &db_path,
            lifecycle_summary,
            options,
            evidence,
            agent_timings_json(started),
        ));
    }
    let graph_proof = evidence.iter().any(path_evidence_is_graph_relation_proof);

    Ok(json!({
        "status": "ok",
        "source": options.source,
        "target": options.target,
        "resolved_sources": source_entities.iter().map(entity_json).collect::<Vec<_>>(),
        "resolved_targets": target_entities.iter().map(entity_json).collect::<Vec<_>>(),
        "paths": evidence,
        "result_count": evidence.len(),
        "limit": options.output.limit,
        "explicit_limit": options.output.explicit_limit,
        "output_mode": options.output.output_mode.as_str(),
        "graph_proof": graph_proof,
        "proof_status": path_query_proof_status(&evidence),
        "proof_strength": path_query_proof_strength(&evidence),
        "chain_confidence": evidence.iter().map(|path| path.confidence).fold(0.0_f64, f64::max),
        "resolver_order": [
            "direct verified calls",
            "resolved imports / alias names",
            "same-module parser-verified calls",
            "static heuristic fallback"
        ],
        "proof": "Call-chain query traverses CALLS edges with explicit exactness and confidence labels; unresolved same-name joins remain heuristic evidence.",
    }))
}

#[derive(Debug, Clone)]
pub(crate) struct UnresolvedCallsOptions {
    pub(crate) db_path: Option<PathBuf>,
    pub(crate) requested_limit: usize,
    pub(crate) limit: usize,
    pub(crate) offset: usize,
    pub(crate) include_snippets: bool,
    pub(crate) source_scan: bool,
    pub(crate) count_total: bool,
    pub(crate) allow_stale_read: bool,
    pub(crate) allow_foreign_db: bool,
    pub(crate) explicit_scope_policy: Option<IndexScopeOptions>,
    pub(crate) surface_name: String,
    pub(crate) operation_kind: DbLifecycleOperationKind,
}

impl Default for UnresolvedCallsOptions {
    fn default() -> Self {
        Self {
            db_path: None,
            requested_limit: DEFAULT_UNRESOLVED_CALL_LIMIT,
            limit: DEFAULT_UNRESOLVED_CALL_LIMIT,
            offset: 0,
            include_snippets: false,
            source_scan: false,
            count_total: false,
            allow_stale_read: false,
            allow_foreign_db: false,
            explicit_scope_policy: None,
            surface_name: "cli.query.unresolved_calls".to_string(),
            operation_kind: DbLifecycleOperationKind::NormalRead,
        }
    }
}

pub(crate) fn parse_unresolved_calls_args(
    args: &[String],
) -> Result<UnresolvedCallsOptions, String> {
    let mut options = UnresolvedCallsOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--limit" => {
                index += 1;
                let raw = args
                    .get(index)
                    .ok_or_else(unresolved_calls_usage)?
                    .parse::<usize>()
                    .map_err(|_| "invalid --limit value".to_string())?;
                options.requested_limit = raw;
                options.limit = raw.clamp(1, MAX_UNRESOLVED_CALL_LIMIT);
            }
            "--offset" | "--cursor" => {
                index += 1;
                options.offset = args
                    .get(index)
                    .ok_or_else(unresolved_calls_usage)?
                    .parse::<usize>()
                    .map_err(|_| "invalid --offset value".to_string())?;
            }
            "--db" => {
                index += 1;
                let path = args.get(index).ok_or_else(unresolved_calls_usage)?;
                options.db_path = Some(PathBuf::from(path));
            }
            "--json" | "--agent-json" | "--agent_json" => {}
            "--no-snippets" => {
                options.include_snippets = false;
            }
            "--include-snippets" => {
                options.include_snippets = true;
            }
            "--source-scan" => {
                options.source_scan = true;
            }
            "--count-total" => {
                options.count_total = true;
            }
            "--allow-stale-read" => {
                options.allow_stale_read = true;
            }
            "--allow-foreign-db" => {
                options.allow_foreign_db = true;
            }
            other => {
                return Err(format!(
                    "unknown unresolved-calls option: {other}\n{}",
                    unresolved_calls_usage()
                ));
            }
        }
        index += 1;
    }
    Ok(options)
}

pub(crate) fn unresolved_calls_usage() -> String {
    "Usage: codegraph-mcp query unresolved-calls [--limit <n>] [--offset <n>] [--json] [--no-snippets] [--include-snippets] [--db <path>] [--allow-stale-read] [--allow-foreign-db]".to_string()
}

pub(crate) fn unresolved_calls_lifecycle_preflight(
    repo_root: &Path,
    db_path: &Path,
    options: &UnresolvedCallsOptions,
) -> Result<DbLifecycleSurfacePreflight, String> {
    let allow_stale_read = options.allow_stale_read || allow_stale_read_enabled();
    let allow_foreign_db = options.allow_foreign_db || allow_foreign_read_enabled();
    let request = DbLifecycleSurfacePreflightRequest {
        repo_root: repo_root.to_path_buf(),
        db_path: db_path.to_path_buf(),
        surface_name: options.surface_name.clone(),
        operation_kind: options.operation_kind,
        allow_stale_read: false,
        allow_foreign_repo: false,
        required_storage_mode: None,
        expected_scope: options.explicit_scope_policy.clone(),
    };

    if options.operation_kind == DbLifecycleOperationKind::NormalRead
        && (allow_stale_read || allow_foreign_db)
    {
        let normal = inspect_db_lifecycle_surface_preflight(request.clone())
            .map_err(|error| error.to_string())?;
        if normal.safe_to_read {
            return Ok(normal);
        }
        let mut diagnostic = request;
        diagnostic.operation_kind = DbLifecycleOperationKind::DiagnosticRead;
        diagnostic.allow_stale_read = allow_stale_read;
        diagnostic.allow_foreign_repo = allow_foreign_db;
        return inspect_db_lifecycle_surface_preflight(diagnostic)
            .map_err(|error| error.to_string());
    }

    let mut request = request;
    if matches!(
        request.operation_kind,
        DbLifecycleOperationKind::DiagnosticRead | DbLifecycleOperationKind::BenchmarkInspection
    ) {
        request.allow_stale_read = allow_stale_read;
        request.allow_foreign_repo = allow_foreign_db;
    }
    inspect_db_lifecycle_surface_preflight(request).map_err(|error| error.to_string())
}

pub(crate) fn require_unresolved_calls_lifecycle_preflight(
    repo_root: &Path,
    db_path: &Path,
    options: &UnresolvedCallsOptions,
) -> Result<DbLifecycleSurfacePreflight, String> {
    let preflight = unresolved_calls_lifecycle_preflight(repo_root, db_path, options)?;
    if preflight.safe_to_read {
        return Ok(preflight);
    }
    if let Some(mismatch) = preflight.lifecycle_preflight.scope_mismatch.as_ref() {
        return Err(scope_mismatch_message(db_path, mismatch));
    }
    let outside_note = preflight
        .outside_workspace_note
        .as_deref()
        .map(|note| format!("; {note}"))
        .unwrap_or_default();
    Err(format!(
        "CodeGraph DB is not safe to read at {}: kind={}; {}{}; run `codegraph-mcp index . --fresh`; pass --allow-stale-read for stale/passport diagnostic output or --allow-foreign-db for foreign-repo diagnostic output",
        preflight.exact_db_path_checked,
        preflight
            .db_problem_kind
            .as_deref()
            .unwrap_or("unknown"),
        preflight.blockers.join("; "),
        outside_note
    ))
}

pub(crate) fn db_lifecycle_surface_preflight_json(
    preflight: &DbLifecycleSurfacePreflight,
) -> Value {
    json!({
        "decision": if preflight.safe_to_read && preflight.claimable {
            "read_reuse"
        } else if preflight.safe_to_read && preflight.diagnostic_only {
            "diagnostic_stale_reuse"
        } else {
            "blocked"
        },
        "surface_name": preflight.surface_name.clone(),
        "operation_kind": preflight.operation_kind.as_str(),
        "safe_to_read": preflight.safe_to_read,
        "safe_to_write": preflight.safe_to_write,
        "db_problem_kind": preflight.db_problem_kind.clone(),
        "path_access_status": preflight.path_access_status.clone(),
        "path_access_error": preflight.path_access_error.clone(),
        "passport_status": preflight.passport_status.clone(),
        "claimable": preflight.claimable,
        "diagnostic_only": preflight.diagnostic_only,
        "contaminated": preflight.diagnostic_only || !preflight.claimable,
        "repo_match": preflight.repo_match,
        "scope_match": preflight.scope_match,
        "schema_status": preflight.schema_status.clone(),
        "storage_mode_match": preflight.storage_mode_match,
        "reasons": preflight.lifecycle_preflight.db_health.reasons.clone(),
        "sqlite_sidecars": preflight.lifecycle_preflight.db_health.sqlite_sidecars.clone(),
        "sidecar_status": preflight.lifecycle_preflight.db_health.sidecar_status.clone(),
        "orphan_sidecars": preflight.lifecycle_preflight.db_health.orphan_sidecars.clone(),
        "orphan_sidecars_deprecated": true,
        "blockers": preflight.blockers.clone(),
        "warnings": preflight.warnings.clone(),
        "exact_db_path_checked": preflight.exact_db_path_checked.clone(),
        "repo_root_expected": preflight.repo_root_expected.clone(),
        "db_path_outside_workspace": preflight.db_path_outside_workspace,
        "outside_workspace_note": preflight.outside_workspace_note.clone(),
        "repo_root_observed": preflight.repo_root_observed.clone(),
        "artifact_freshness": preflight.artifact_freshness.clone(),
        "repo_root_status": preflight.lifecycle_preflight.repo_root_status.clone(),
        "storage_mode_status": preflight.lifecycle_preflight.storage_mode_status.clone(),
        "scope_status": preflight.lifecycle_preflight.scope_status.clone(),
        "scope_source": preflight.scope_source.clone(),
        "passport_scope_hash": preflight.passport_scope_hash.clone(),
        "explicit_scope_hash": preflight.explicit_scope_hash.clone(),
        "scope_mismatch": preflight.lifecycle_preflight.scope_mismatch.clone(),
        "passport_scope_policy": preflight.lifecycle_preflight.passport_scope_policy.clone(),
        "explicit_scope_policy": preflight.lifecycle_preflight.explicit_scope_policy.clone(),
    })
}

pub(crate) fn query_unresolved_calls(
    repo_root: &Path,
    options: UnresolvedCallsOptions,
) -> Result<Value, String> {
    let total_start = Instant::now();
    let db_path = options
        .db_path
        .as_ref()
        .map(|path| normalize_db_path_for_repo(repo_root, path))
        .unwrap_or_else(|| resolved_db_path_for_repo(repo_root));
    let db_source = if options.db_path.is_some() {
        "command --db".to_string()
    } else {
        db_source_label()
    };
    let preflight = require_unresolved_calls_lifecycle_preflight(repo_root, &db_path, &options)?;
    let db_lifecycle_read = db_lifecycle_surface_preflight_json(&preflight);
    let checked_db_path = PathBuf::from(&preflight.exact_db_path_checked);

    let open_start = Instant::now();
    let connection =
        Connection::open_with_flags(&checked_db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| error.to_string())?;
    let open_ms = elapsed_ms(open_start);

    let explain_start = Instant::now();
    let explain_plan = unresolved_calls_query_plan(&connection, options.limit, options.offset)?;
    let explain_query_plan_ms = elapsed_ms(explain_start);
    let query_plan_analysis = analyze_sqlite_query_plan(&explain_plan);

    let count_start = Instant::now();
    let total_matching = if options.count_total {
        Some(count_unresolved_calls(&connection)?)
    } else {
        None
    };
    let count_ms = elapsed_ms(count_start);

    let page_start = Instant::now();
    let mut unresolved = query_unresolved_calls_page(
        &connection,
        repo_root,
        options.limit,
        options.offset,
        options.include_snippets,
    )?;
    let page_query_ms = elapsed_ms(page_start);

    let mut source_scan = json!({
        "enabled": false,
        "reason": "disabled_by_default_to_avoid_unbounded_reparse",
        "rows_added": 0,
        "elapsed_ms": 0,
    });
    if options.source_scan && unresolved.len() < options.limit {
        let scan_start = Instant::now();
        let store = SqliteGraphStore::open_read_only(&checked_db_path)
            .map_err(|error| error.to_string())?;
        let before = unresolved.len();
        unresolved.extend(source_scan_unresolved_calls(
            repo_root,
            &store,
            options.limit.saturating_sub(unresolved.len()),
        )?);
        source_scan = json!({
            "enabled": true,
            "rows_added": unresolved.len().saturating_sub(before),
            "elapsed_ms": elapsed_ms(scan_start),
            "bounded_by_remaining_limit": options.limit.saturating_sub(before),
        });
    }

    let total_ms = elapsed_ms(total_start);
    Ok(json!({
        "status": "ok",
        "resolved_repo": path_string(repo_root),
        "resolved_db": path_string(&checked_db_path),
        "repo_source": repo_source_label(),
        "db_source": db_source,
        "lifecycle_status": db_lifecycle_read
            .get("decision")
            .cloned()
            .unwrap_or_else(|| json!("unknown")),
        "db_lifecycle_read": db_lifecycle_read,
        "claimable": preflight.claimable,
        "diagnostic_only": preflight.diagnostic_only,
        "exact_db_path_checked": preflight.exact_db_path_checked,
        "calls": unresolved,
        "pagination": {
            "requested_limit": options.requested_limit,
            "effective_limit": options.limit,
            "limit_capped": options.requested_limit != options.limit,
            "max_limit": MAX_UNRESOLVED_CALL_LIMIT,
            "offset": options.offset,
            "next_offset": options.offset.saturating_add(unresolved.len()),
        },
        "row_counts": {
            "returned": unresolved.len(),
            "total_matching": total_matching,
            "total_matching_counted": options.count_total,
            "total_matching_note": if options.count_total {
                "counted_with_the_same_unresolved_call_filter"
            } else {
                "skipped_by_default; use --count-total for audit-only full counts"
            },
        },
        "instrumentation": {
            "db_path": checked_db_path,
            "sql": {
                "page_query": UNRESOLVED_CALLS_PAGE_SQL,
                "count_query": if options.count_total { Value::String(UNRESOLVED_CALLS_COUNT_SQL.to_string()) } else { Value::Null },
            },
            "explain_query_plan": explain_plan,
            "query_plan_analysis": query_plan_analysis,
            "elapsed_ms": {
                "open_db": open_ms,
                "explain_query_plan": explain_query_plan_ms,
                "count_total": count_ms,
                "page_query": page_query_ms,
                "total": total_ms,
            },
            "snippets": {
                "requested": options.include_snippets,
                "default": "not_loaded_unless_requested",
            },
            "source_scan": source_scan,
        },
        "proof": "Unresolved-call enumeration is bounded by limit/offset, filters CALLS/static-heuristic evidence in SQL, and does not load snippets or reparse source unless explicitly requested.",
    }))
}

pub(crate) const UNRESOLVED_CALLS_PAGE_SQL: &str = r#"
SELECT e.edge_id AS edge_id,
       e.head_id AS head_id,
       e.relation AS relation,
       e.tail_id AS tail_id,
       e.source_span_path AS span_repo_relative_path,
       e.start_line, e.start_column, e.end_line, e.end_column,
       e.repo_commit, e.file_hash AS file_hash, e.extractor AS extractor,
       e.confidence, e.exactness AS exactness, e.derived,
       e.provenance_edges_json, e.metadata_json AS edge_metadata_json,
       COALESCE(head_kind.value, head_static.kind, 'Function') AS head_kind,
       COALESCE(head_name.value, head_static.name, e.head_id) AS head_name,
       COALESCE(head_qname.value, head_static.qualified_name, e.head_id) AS head_qualified_name,
       COALESCE(head_path.value, head_static.repo_relative_path, e.source_span_path) AS head_repo_relative_path,
       COALESCE(head_span_path.value, head_static.source_span_path) AS head_span_repo_relative_path,
       head.start_line AS head_start_line,
       head.start_column AS head_start_column,
       head.end_line AS head_end_line,
       head.end_column AS head_end_column,
       COALESCE(head_extractor.value, head_static.created_from, 'heuristic_sidecar') AS head_created_from,
       COALESCE(head.confidence, head_static.confidence, e.confidence) AS head_confidence,
       COALESCE(head.metadata_json, head_static.metadata_json, '{}') AS head_metadata_json,
       COALESCE(tail_kind.value, tail_static.kind, 'Function') AS tail_kind,
       COALESCE(tail_name.value, tail_static.name, e.tail_id) AS tail_name,
       COALESCE(tail_qname.value, tail_static.qualified_name, e.tail_id) AS tail_qualified_name,
       COALESCE(tail_path.value, tail_static.repo_relative_path, e.source_span_path) AS tail_repo_relative_path,
       COALESCE(tail_span_path.value, tail_static.source_span_path) AS tail_span_repo_relative_path,
       COALESCE(tail.start_line, tail_static.start_line) AS tail_start_line,
       COALESCE(tail.start_column, tail_static.start_column) AS tail_start_column,
       COALESCE(tail.end_line, tail_static.end_line) AS tail_end_line,
       COALESCE(tail.end_column, tail_static.end_column) AS tail_end_column,
       COALESCE(tail_extractor.value, tail_static.created_from, 'heuristic_sidecar') AS tail_created_from,
       COALESCE(tail.confidence, tail_static.confidence, e.confidence) AS tail_confidence,
       COALESCE(tail.metadata_json, tail_static.metadata_json, '{}') AS tail_metadata_json
FROM heuristic_edges e
LEFT JOIN object_id_lookup head_oid ON head_oid.value = e.head_id
LEFT JOIN object_id_lookup tail_oid ON tail_oid.value = e.tail_id
LEFT JOIN entities head ON head.id_key = head_oid.id
LEFT JOIN entity_kind_dict head_kind ON head_kind.id = head.kind_id
LEFT JOIN symbol_dict head_name ON head_name.id = head.name_id
LEFT JOIN qualified_name_lookup head_qname ON head_qname.id = head.qualified_name_id
LEFT JOIN path_dict head_path ON head_path.id = head.path_id
LEFT JOIN path_dict head_span_path ON head_span_path.id = head.span_path_id
LEFT JOIN extractor_dict head_extractor ON head_extractor.id = head.created_from_id
LEFT JOIN static_references head_static ON head_static.entity_id = e.head_id
LEFT JOIN entities tail ON tail.id_key = tail_oid.id
LEFT JOIN entity_kind_dict tail_kind ON tail_kind.id = tail.kind_id
LEFT JOIN symbol_dict tail_name ON tail_name.id = tail.name_id
LEFT JOIN qualified_name_lookup tail_qname ON tail_qname.id = tail.qualified_name_id
LEFT JOIN path_dict tail_path ON tail_path.id = tail.path_id
LEFT JOIN path_dict tail_span_path ON tail_span_path.id = tail.span_path_id
LEFT JOIN extractor_dict tail_extractor ON tail_extractor.id = tail.created_from_id
LEFT JOIN static_references tail_static ON tail_static.entity_id = e.tail_id
WHERE e.relation = 'CALLS'
  AND (
      e.exactness = 'static_heuristic'
      OR lower(COALESCE(e.metadata_json, '')) LIKE '%unresolved%'
      OR lower(COALESCE(tail_static.metadata_json, tail.metadata_json, '')) LIKE '%unresolved%'
      OR lower(COALESCE(tail_static.created_from, tail_extractor.value, '')) LIKE '%heuristic%'
      OR lower(COALESCE(tail_static.name, tail_name.value, '')) LIKE '%unknown_callee%'
      OR COALESCE(tail_static.qualified_name, tail_qname.value, '') LIKE 'static_reference:%'
  )
ORDER BY e.id_key
LIMIT ?1 OFFSET ?2
"#;

pub(crate) const UNRESOLVED_CALLS_COUNT_SQL: &str = r#"
SELECT COUNT(*)
FROM heuristic_edges e
LEFT JOIN static_references tail_static ON tail_static.entity_id = e.tail_id
WHERE e.relation = 'CALLS'
  AND (
      e.exactness = 'static_heuristic'
      OR lower(COALESCE(e.metadata_json, '')) LIKE '%unresolved%'
      OR lower(COALESCE(tail_static.metadata_json, '')) LIKE '%unresolved%'
      OR lower(COALESCE(tail_static.created_from, '')) LIKE '%heuristic%'
      OR lower(COALESCE(tail_static.name, '')) LIKE '%unknown_callee%'
      OR COALESCE(tail_static.qualified_name, '') LIKE 'static_reference:%'
  )
"#;

pub(crate) fn unresolved_calls_query_plan(
    connection: &Connection,
    limit: usize,
    offset: usize,
) -> Result<Vec<Value>, String> {
    if !sqlite_table_exists(connection, "heuristic_edges")? {
        return Ok(Vec::new());
    }
    let sql = format!("EXPLAIN QUERY PLAN {UNRESOLVED_CALLS_PAGE_SQL}");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![limit as i64, offset as i64], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "parent": row.get::<_, i64>(1)?,
                "notused": row.get::<_, i64>(2)?,
                "detail": row.get::<_, String>(3)?,
            }))
        })
        .map_err(|error| error.to_string())?;
    collect_sqlite_values(rows)
}

pub(crate) fn analyze_sqlite_query_plan(plan: &[Value]) -> Value {
    let mut indexes = BTreeSet::new();
    let mut full_scans = Vec::new();
    for row in plan {
        let Some(detail) = row.get("detail").and_then(Value::as_str) else {
            continue;
        };
        let lower = detail.to_ascii_lowercase();
        if lower.contains(" scan ") || lower.starts_with("scan ") {
            full_scans.push(detail.to_string());
        }
        if let Some(index) = query_plan_index_name(detail) {
            indexes.insert(index);
        }
    }
    json!({
        "uses_indexes": !indexes.is_empty(),
        "indexes_used": indexes.into_iter().collect::<Vec<_>>(),
        "full_scans": full_scans,
    })
}

pub(crate) fn query_plan_index_name(detail: &str) -> Option<String> {
    for marker in [
        "USING COVERING INDEX ",
        "USING INDEX ",
        "USING INTEGER PRIMARY KEY ",
    ] {
        if let Some((_, rest)) = detail.split_once(marker) {
            return rest
                .split_whitespace()
                .next()
                .map(|name| name.trim_matches(|ch| ch == '(' || ch == ')').to_string());
        }
    }
    None
}

pub(crate) fn count_unresolved_calls(connection: &Connection) -> Result<i64, String> {
    if !sqlite_table_exists(connection, "heuristic_edges")? {
        return Ok(0);
    }
    connection
        .query_row(UNRESOLVED_CALLS_COUNT_SQL, [], |row| row.get::<_, i64>(0))
        .map_err(|error| error.to_string())
}

pub(crate) fn query_unresolved_calls_page(
    connection: &Connection,
    repo_root: &Path,
    limit: usize,
    offset: usize,
    include_snippets: bool,
) -> Result<Vec<Value>, String> {
    if !sqlite_table_exists(connection, "heuristic_edges")? {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(UNRESOLVED_CALLS_PAGE_SQL)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![limit as i64, offset as i64],
            unresolved_call_row_json,
        )
        .map_err(|error| error.to_string())?;
    let mut calls = collect_sqlite_values(rows)?;
    if include_snippets {
        attach_unresolved_call_snippets(repo_root, &mut calls);
    }
    Ok(calls)
}

pub(crate) fn unresolved_call_row_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let edge_metadata = json_from_sql_text(row.get::<_, String>("edge_metadata_json")?);
    let provenance_edges = json_from_sql_text(row.get::<_, String>("provenance_edges_json")?);
    let tail = entity_json_from_unresolved_row(row, "tail")?;
    let exactness: String = row.get("exactness")?;
    let unresolved = exactness == "static_heuristic"
        || value_contains_unresolved(&edge_metadata)
        || tail
            .get("qualified_name")
            .and_then(Value::as_str)
            .is_some_and(|qualified_name| qualified_name.starts_with("static_reference:"))
        || tail
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name.contains("unknown_callee"))
        || tail
            .get("created_from")
            .and_then(Value::as_str)
            .is_some_and(|created_from| created_from.contains("heuristic"))
        || tail.get("metadata").is_some_and(value_contains_unresolved);

    Ok(json!({
        "edge": {
            "id": row.get::<_, String>("edge_id")?,
            "head_id": row.get::<_, String>("head_id")?,
            "relation": row.get::<_, String>("relation")?,
            "tail_id": row.get::<_, String>("tail_id")?,
            "source_span": source_span_value(
                row.get::<_, String>("span_repo_relative_path")?,
                row.get::<_, i64>("start_line")?,
                row.get::<_, Option<i64>>("start_column")?,
                row.get::<_, i64>("end_line")?,
                row.get::<_, Option<i64>>("end_column")?,
            ),
            "repo_commit": row.get::<_, Option<String>>("repo_commit")?,
            "file_hash": row.get::<_, Option<String>>("file_hash")?,
            "extractor": row.get::<_, String>("extractor")?,
            "confidence": row.get::<_, f64>("confidence")?,
            "exactness": exactness,
            "derived": row.get::<_, i64>("derived")? != 0,
            "provenance_edges": provenance_edges,
            "metadata": edge_metadata,
        },
        "head": entity_json_from_unresolved_row(row, "head")?,
        "tail": tail,
        "unresolved": unresolved,
        "source_snippet": {
            "requested": false,
            "loaded": false,
        },
    }))
}

pub(crate) fn entity_json_from_unresolved_row(
    row: &rusqlite::Row<'_>,
    prefix: &str,
) -> rusqlite::Result<Value> {
    let span_path =
        row.get::<_, Option<String>>(format!("{prefix}_span_repo_relative_path").as_str())?;
    let source_span = optional_source_span_value(
        span_path,
        row.get::<_, Option<i64>>(format!("{prefix}_start_line").as_str())?,
        row.get::<_, Option<i64>>(format!("{prefix}_start_column").as_str())?,
        row.get::<_, Option<i64>>(format!("{prefix}_end_line").as_str())?,
        row.get::<_, Option<i64>>(format!("{prefix}_end_column").as_str())?,
    );
    Ok(json!({
        "id": row.get::<_, String>(format!("{prefix}_id").as_str())?,
        "kind": row.get::<_, String>(format!("{prefix}_kind").as_str())?,
        "name": row.get::<_, String>(format!("{prefix}_name").as_str())?,
        "qualified_name": row.get::<_, String>(format!("{prefix}_qualified_name").as_str())?,
        "repo_relative_path": row.get::<_, String>(format!("{prefix}_repo_relative_path").as_str())?,
        "source_span": source_span,
        "created_from": row.get::<_, String>(format!("{prefix}_created_from").as_str())?,
        "confidence": row.get::<_, f64>(format!("{prefix}_confidence").as_str())?,
        "metadata": json_from_sql_text(row.get::<_, String>(format!("{prefix}_metadata_json").as_str())?),
    }))
}

pub(crate) fn optional_source_span_value(
    path: Option<String>,
    start_line: Option<i64>,
    start_column: Option<i64>,
    end_line: Option<i64>,
    end_column: Option<i64>,
) -> Value {
    match (path, start_line, end_line) {
        (Some(path), Some(start_line), Some(end_line)) => {
            source_span_value(path, start_line, start_column, end_line, end_column)
        }
        _ => Value::Null,
    }
}

pub(crate) fn source_span_value(
    path: String,
    start_line: i64,
    start_column: Option<i64>,
    end_line: i64,
    end_column: Option<i64>,
) -> Value {
    let start_line = sqlite_u32(start_line);
    let end_line = sqlite_u32(end_line).max(start_line);
    let span = match (start_column, end_column) {
        (Some(start_column), Some(end_column)) => SourceSpan::with_columns(
            path,
            start_line,
            sqlite_u32(start_column),
            end_line,
            sqlite_u32(end_column),
        ),
        _ => SourceSpan::new(path, start_line, end_line),
    };
    json!(span)
}

pub(crate) fn sqlite_u32(value: i64) -> u32 {
    value.clamp(0, i64::from(u32::MAX)) as u32
}

pub(crate) fn json_from_sql_text(raw: String) -> Value {
    serde_json::from_str(&raw).unwrap_or_else(|_| {
        json!({
            "_invalid_json": raw,
        })
    })
}

pub(crate) fn value_contains_unresolved(value: &Value) -> bool {
    value
        .to_string()
        .to_ascii_lowercase()
        .contains("unresolved")
}

pub(crate) fn collect_sqlite_values(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<Value>>,
) -> Result<Vec<Value>, String> {
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(crate) fn attach_unresolved_call_snippets(repo_root: &Path, calls: &mut [Value]) {
    for call in calls {
        let snippet = unresolved_call_snippet_value(repo_root, call);
        if let Some(object) = call.as_object_mut() {
            object.insert("source_snippet".to_string(), snippet);
        }
    }
}

pub(crate) fn unresolved_call_snippet_value(repo_root: &Path, call: &Value) -> Value {
    let Some(span) = call
        .get("edge")
        .and_then(|edge| edge.get("source_span"))
        .and_then(source_span_from_value)
    else {
        return json!({
            "requested": true,
            "loaded": false,
            "error": "missing_source_span",
        });
    };
    let path = repo_root.join(&span.repo_relative_path);
    match fs::read_to_string(&path) {
        Ok(source) => json!({
            "requested": true,
            "loaded": true,
            "repo_relative_path": span.repo_relative_path,
            "text": source_snippet(&span, &source),
        }),
        Err(error) => json!({
            "requested": true,
            "loaded": false,
            "repo_relative_path": span.repo_relative_path,
            "error": error.to_string(),
        }),
    }
}

pub(crate) fn source_span_from_value(value: &Value) -> Option<SourceSpan> {
    let path = value.get("repo_relative_path")?.as_str()?.to_string();
    let start_line = value.get("start_line")?.as_u64()?.min(u64::from(u32::MAX)) as u32;
    let end_line = value.get("end_line")?.as_u64()?.min(u64::from(u32::MAX)) as u32;
    match (
        value.get("start_column").and_then(Value::as_u64),
        value.get("end_column").and_then(Value::as_u64),
    ) {
        (Some(start_column), Some(end_column)) => Some(SourceSpan::with_columns(
            path,
            start_line,
            start_column.min(u64::from(u32::MAX)) as u32,
            end_line,
            end_column.min(u64::from(u32::MAX)) as u32,
        )),
        _ => Some(SourceSpan::new(path, start_line, end_line)),
    }
}

pub(crate) fn source_scan_unresolved_calls(
    repo_root: &Path,
    store: &SqliteGraphStore,
    limit: usize,
) -> Result<Vec<Value>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let parser = TreeSitterParser;
    let mut calls = Vec::new();
    for file in store
        .list_files(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?
    {
        let path = repo_root.join(&file.repo_relative_path);
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = match parser.parse(&file.repo_relative_path, &source) {
            Ok(Some(parsed)) => parsed,
            _ => continue,
        };
        let extraction = extract_entities_and_relations(&parsed, &source);
        let local_entity_by_id = entities_by_id(&extraction.entities);
        for edge in extraction
            .edges
            .iter()
            .filter(|edge| edge_is_unresolved_call(edge, &local_entity_by_id))
        {
            calls.push(edge_with_entities_json(edge, &local_entity_by_id));
            if calls.len() >= limit {
                return Ok(calls);
            }
        }
    }
    Ok(calls)
}

pub(crate) fn edge_is_unresolved_call(
    edge: &Edge,
    entity_by_id: &BTreeMap<String, Entity>,
) -> bool {
    edge.relation == RelationKind::Calls
        && (edge.exactness == Exactness::StaticHeuristic
            || edge
                .metadata
                .get("resolution")
                .and_then(|value| value.as_str())
                .is_some_and(|resolution| resolution.contains("unresolved"))
            || entity_by_id
                .get(&edge.tail_id)
                .is_some_and(entity_is_unresolved_reference))
}

pub(crate) fn query_path_with_options(
    repo_root: &Path,
    options: &PathQueryOptions,
    lifecycle_summary: Option<&Value>,
) -> Result<Value, String> {
    let started = Instant::now();
    let db_path = resolved_db_path_for_repo(repo_root);
    let store = open_existing_store(repo_root)?;
    let engine = query_engine(&store)?;
    let source_id = resolve_symbol_or_literal(&store, &options.source)?;
    let target_id = resolve_symbol_or_literal(&store, &options.target)?;
    let mut limits = default_query_limits();
    limits.max_paths = options.output.fetch_limit();
    let paths = engine.trace_path(&source_id, &target_id, RelationKind::ALL, limits);
    let mut evidence = engine.path_evidence_from_paths(&paths);
    let entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let entity_by_id = entities_by_id(&entities);
    hydrate_path_evidence_endpoint_labels(&mut evidence, &entity_by_id);
    if options.output.output_mode.is_compact() {
        return Ok(agent_path_query_response(
            "query_path_agent_json",
            "query path",
            repo_root,
            &db_path,
            lifecycle_summary,
            options,
            evidence,
            agent_timings_json(started),
        ));
    }
    let graph_proof = evidence.iter().any(path_evidence_is_graph_relation_proof);
    Ok(json!({
        "status": "ok",
        "source": options.source,
        "target": options.target,
        "resolved_source": source_id,
        "resolved_target": target_id,
        "paths": evidence,
        "result_count": evidence.len(),
        "limit": options.output.limit,
        "explicit_limit": options.output.explicit_limit,
        "output_mode": options.output.output_mode.as_str(),
        "graph_proof": graph_proof,
        "proof_status": path_query_proof_status(&evidence),
        "proof_strength": path_query_proof_strength(&evidence),
        "proof": "Path query is exact graph traversal over local persisted edges.",
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallQueryDirection {
    Callers,
    Callees,
}

#[derive(Debug, Clone)]
pub(crate) struct CallRelationQueryOptions {
    pub(crate) query: Option<String>,
    pub(crate) entity_id: Option<String>,
    pub(crate) limit: usize,
    pub(crate) exact_resolved: bool,
    pub(crate) fuzzy: bool,
}

#[cfg(test)]
pub(crate) fn parse_call_relation_args(
    command_name: &str,
    args: &[String],
) -> Result<CallRelationQueryOptions, String> {
    parse_call_relation_args_with_output(command_name, args).map(|parsed| parsed.options)
}

pub(crate) fn parse_call_relation_args_with_output(
    command_name: &str,
    args: &[String],
) -> Result<ParsedCallRelationArgs, String> {
    let mut options = CallRelationQueryOptions {
        query: None,
        entity_id: None,
        limit: DEFAULT_QUERY_JSON_LIMIT,
        exact_resolved: false,
        fuzzy: false,
    };
    let mut output_mode = QueryOutputMode::RichJson;
    let mut explicit_limit = None;
    let mut verbose = false;
    let mut debug = false;
    let mut explain = false;
    let mut query_parts = Vec::new();
    let mut index = 0usize;
    let mut literal_query_terms = false;
    while index < args.len() {
        if literal_query_terms {
            query_parts.push(args[index].to_string());
            index += 1;
            continue;
        }
        match args[index].as_str() {
            "--" => {
                literal_query_terms = true;
            }
            "--entity-id" | "--entity_id" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(call_relation_usage(command_name));
                };
                options.entity_id = Some(value.clone());
            }
            "--exact-resolved" | "--exact_resolved" => {
                options.exact_resolved = true;
            }
            "--fuzzy" | "--global" => {
                options.fuzzy = true;
            }
            "--limit" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(call_relation_usage(command_name));
                };
                explicit_limit = Some(parse_limit_value(value)?);
            }
            "--agent-json" | "--agent_json" => {
                output_mode = QueryOutputMode::AgentJson;
            }
            "--concise" => {
                if output_mode != QueryOutputMode::AgentJson {
                    output_mode = QueryOutputMode::Concise;
                }
            }
            "--verbose" => {
                verbose = true;
            }
            "--debug" => {
                debug = true;
                verbose = true;
            }
            "--explain" => {
                explain = true;
            }
            "--json" => {}
            value if value.starts_with("--limit=") => {
                let value = value.trim_start_matches("--limit=");
                explicit_limit = Some(parse_limit_value(value)?);
            }
            value if value.starts_with("--") => {
                if let Some(error) = misplaced_global_flag_error(value, "query") {
                    return Err(error);
                }
                return Err(format!(
                    "unknown {command_name} option: {value}\n{}",
                    call_relation_usage(command_name)
                ));
            }
            value => query_parts.push(value.to_string()),
        }
        index += 1;
    }

    if !query_parts.is_empty() {
        options.query = Some(query_parts.join(" "));
    }
    if options.entity_id.is_none() && options.query.is_none() {
        return Err(call_relation_usage(command_name));
    }
    if options.entity_id.is_some() && options.fuzzy {
        return Err(
            "--entity-id selects exact mode and cannot be combined with --fuzzy".to_string(),
        );
    }
    let default_limit = match output_mode {
        QueryOutputMode::AgentJson => DEFAULT_QUERY_AGENT_JSON_LIMIT,
        QueryOutputMode::Concise => DEFAULT_QUERY_JSON_LIMIT,
        QueryOutputMode::RichJson if verbose || debug || explain => 32,
        QueryOutputMode::RichJson => DEFAULT_QUERY_JSON_LIMIT,
    };
    options.limit = explicit_limit.unwrap_or(default_limit);
    let fetch_limit = QueryOutputOptions {
        limit: options.limit,
        explicit_limit: explicit_limit.is_some(),
        output_mode,
    }
    .fetch_limit();
    let output = QueryOutputOptions {
        limit: options.limit,
        explicit_limit: explicit_limit.is_some(),
        output_mode,
    };
    options.limit = fetch_limit;
    Ok(ParsedCallRelationArgs { options, output })
}

pub(crate) fn call_relation_usage(command_name: &str) -> String {
    format!(
        "Usage: codegraph-mcp query {command_name} [--entity-id <id>|--exact-resolved|--fuzzy] [--limit <n>] [--concise|--agent-json] [--verbose|--debug|--explain] <symbol>\nLiteral flag-like symbols: codegraph-mcp query {command_name} [options] -- --db"
    )
}

pub(crate) fn query_call_relation_with_output(
    repo_root: &Path,
    parsed: ParsedCallRelationArgs,
    direction: CallQueryDirection,
    lifecycle_summary: Option<&Value>,
) -> Result<Value, String> {
    let started = Instant::now();
    let db_path = resolved_db_path_for_repo(repo_root);
    let rich = query_call_relation(repo_root, parsed.options.clone(), direction)?;
    if parsed.output.output_mode.is_compact() {
        return Ok(agent_call_relation_response(
            &rich,
            &parsed.output,
            direction,
            lifecycle_summary,
            repo_root,
            &db_path,
            agent_timings_json(started),
        ));
    }
    Ok(rich)
}

pub(crate) fn query_call_relation(
    repo_root: &Path,
    options: CallRelationQueryOptions,
    direction: CallQueryDirection,
) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let entity_by_id = entities_by_id(&entities);

    let key = match direction {
        CallQueryDirection::Callers => "callers",
        CallQueryDirection::Callees => "callees",
    };
    let query_label = options
        .query
        .as_deref()
        .or(options.entity_id.as_deref())
        .unwrap_or_default()
        .to_string();

    let mut resolved_symbols = Vec::new();
    let mut exact_resolved_entity_results = Vec::new();
    let mut fuzzy_or_global_results = Vec::new();
    let mut ambiguous_symbol_matches = Vec::new();
    let mut exact_entity = None;
    let mut status = "ok";
    let mut resolution_mode = "fuzzy_or_global";
    let mut suggestion = Value::Null;

    if let Some(entity_id) = options.entity_id.as_deref() {
        let entity = store
            .get_entity(entity_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("entity_id not found: {entity_id}"))?;
        exact_resolved_entity_results = exact_call_relation_rows(
            &store,
            &entity_by_id,
            &entity.id,
            options.limit,
            direction,
            "entity_id",
        )?;
        resolved_symbols.push(entity.clone());
        exact_entity = Some(entity);
        resolution_mode = "entity_id";
    } else if let Some(query) = options.query.as_deref() {
        let exact_candidates = resolve_exact_symbol_candidates(&store, query)?;
        resolved_symbols = resolve_symbol_candidates(&store, query, 8)?;
        if options.fuzzy {
            fuzzy_or_global_results = fuzzy_call_relation_rows(
                &store,
                &entity_by_id,
                &resolved_symbols,
                query,
                options.limit,
                direction,
            )?;
            resolution_mode = "fuzzy_or_global";
        } else if exact_candidates.len() == 1 {
            let entity = exact_candidates[0].clone();
            exact_resolved_entity_results = exact_call_relation_rows(
                &store,
                &entity_by_id,
                &entity.id,
                options.limit,
                direction,
                "exact_resolved",
            )?;
            exact_entity = Some(entity);
            resolution_mode = "exact_resolved";
        } else if exact_candidates.len() > 1 {
            status = "ambiguous_symbol";
            resolution_mode = "ambiguous_symbol";
            ambiguous_symbol_matches = exact_candidates.iter().map(entity_json).collect();
            fuzzy_or_global_results = fuzzy_call_relation_rows(
                &store,
                &entity_by_id,
                &resolved_symbols,
                query,
                options.limit,
                direction,
            )?;
            suggestion = json!({
                "reason": "Symbol resolved to multiple entities; choose one candidate id for exact caller/callee proof.",
                "rerun": format!("codegraph-mcp query {key} --entity-id <candidate-id>"),
            });
        } else if options.exact_resolved {
            status = "not_found";
            resolution_mode = "exact_resolved_not_found";
            suggestion = json!({
                "reason": "--exact-resolved requires exactly one symbol match.",
                "rerun": format!("codegraph-mcp query {key} --fuzzy {query}"),
            });
        } else {
            fuzzy_or_global_results = fuzzy_call_relation_rows(
                &store,
                &entity_by_id,
                &resolved_symbols,
                query,
                options.limit,
                direction,
            )?;
            resolution_mode = "fuzzy_or_global";
        }
    }

    let legacy_rows = if exact_entity.is_some() {
        exact_resolved_entity_results.clone()
    } else if status == "ambiguous_symbol" && !options.fuzzy {
        Vec::new()
    } else {
        fuzzy_or_global_results.clone()
    };

    let mut response = json!({
        "status": status,
        "query": query_label,
        "resolution_mode": resolution_mode,
        "exact_mode_available": exact_entity.is_some(),
        "exact_resolved_entity": exact_entity.as_ref().map(entity_json),
        "resolved_symbols": resolved_symbols.iter().map(entity_json).collect::<Vec<_>>(),
        "exact_resolved_entity_results": exact_resolved_entity_results,
        "fuzzy_or_global_results": fuzzy_or_global_results,
        "ambiguous_symbol_matches": ambiguous_symbol_matches,
        "suggestion": suggestion,
        "proof": "Caller/callee results preserve CALLS edge exactness, confidence, source spans, and proof labels. Exact modes match a single persisted entity id; --fuzzy keeps alias/global matching available.",
    });
    if let Some(object) = response.as_object_mut() {
        object.insert(key.to_string(), json!(legacy_rows));
    }
    Ok(response)
}

pub(crate) fn resolve_exact_symbol_candidates(
    store: &SqliteGraphStore,
    value: &str,
) -> Result<Vec<Entity>, String> {
    let mut seen = BTreeSet::new();
    let mut entities = Vec::new();
    for entity in store
        .find_entities_by_exact_symbol(value)
        .map_err(|error| error.to_string())?
    {
        if seen.insert(entity.id.clone()) {
            entities.push(entity);
        }
    }
    Ok(entities)
}

pub(crate) fn exact_call_relation_rows(
    store: &SqliteGraphStore,
    entity_by_id: &BTreeMap<String, Entity>,
    entity_id: &str,
    limit: usize,
    direction: CallQueryDirection,
    match_kind: &str,
) -> Result<Vec<Value>, String> {
    let edges = match direction {
        CallQueryDirection::Callers => {
            store.find_edges_by_tail_relation(entity_id, RelationKind::Calls)
        }
        CallQueryDirection::Callees => {
            store.find_edges_by_head_relation(entity_id, RelationKind::Calls)
        }
    }
    .map_err(|error| error.to_string())?;

    Ok(edges
        .into_iter()
        .take(limit)
        .map(|edge| call_edge_result_json(&edge, entity_by_id, match_kind, Some(entity_id)))
        .collect())
}

pub(crate) fn fuzzy_call_relation_rows(
    store: &SqliteGraphStore,
    entity_by_id: &BTreeMap<String, Entity>,
    seeds: &[Entity],
    query: &str,
    limit: usize,
    direction: CallQueryDirection,
) -> Result<Vec<Value>, String> {
    let seed_ids = seeds
        .iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    let seed_aliases = alias_set_for_entities(seeds)
        .into_iter()
        .chain([normalize_symbol_alias(query)])
        .collect::<BTreeSet<_>>();
    let mut rows = Vec::new();

    for edge in store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|edge| edge.relation == RelationKind::Calls)
    {
        if rows.len() >= limit {
            break;
        }
        let head_aliases = entity_by_id
            .get(&edge.head_id)
            .map(entity_aliases)
            .unwrap_or_default();
        let tail_aliases = entity_by_id
            .get(&edge.tail_id)
            .map(entity_aliases)
            .unwrap_or_default();
        let matches = match direction {
            CallQueryDirection::Callers => {
                seed_ids.contains(&edge.tail_id) || aliases_overlap(&seed_aliases, &tail_aliases)
            }
            CallQueryDirection::Callees => {
                seed_ids.contains(&edge.head_id) || aliases_overlap(&seed_aliases, &head_aliases)
            }
        };
        if matches {
            rows.push(call_edge_result_json(
                &edge,
                entity_by_id,
                "fuzzy_or_global",
                None,
            ));
        }
    }
    Ok(rows)
}

pub(crate) fn call_edge_result_json(
    edge: &Edge,
    entity_by_id: &BTreeMap<String, Entity>,
    match_kind: &str,
    exact_entity_id: Option<&str>,
) -> Value {
    let unresolved = entity_by_id
        .get(&edge.tail_id)
        .is_some_and(entity_is_unresolved_reference);
    let role = query_evidence_role_for_edge(edge);
    json!({
        "match_kind": match_kind,
        "exact_entity_id": exact_entity_id,
        "caller": entity_by_id.get(&edge.head_id).map(entity_json),
        "callee": entity_by_id.get(&edge.tail_id).map(entity_json),
        "edge": edge_json(edge),
        "source_span": edge.source_span,
        "evidence_role": role.role,
        "classification_reason": role.reason,
        "classification_source": role.source,
        "proof_labels": {
            "relation": edge.relation.to_string(),
            "exactness": edge.exactness.to_string(),
            "confidence": edge.confidence,
            "edge_class": edge.edge_class.to_string(),
            "context": edge.context.to_string(),
            "derived": edge.derived,
            "provenance_edges": edge.provenance_edges,
            "unresolved": unresolved,
        },
        "unresolved": unresolved,
    })
}

pub(crate) fn agent_call_relation_response(
    rich: &Value,
    output: &QueryOutputOptions,
    direction: CallQueryDirection,
    lifecycle_summary: Option<&Value>,
    repo_root: &Path,
    db_path: &Path,
    timings: Value,
) -> Value {
    let key = match direction {
        CallQueryDirection::Callers => "callers",
        CallQueryDirection::Callees => "callees",
    };
    let rows = rich
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (rows, truncation) = truncate_for_agent(rows, output.limit);
    let results = rows
        .iter()
        .map(agent_call_relation_row_json)
        .collect::<Vec<_>>();
    let rich_status = rich
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("warning");
    let warnings = if rich_status == "ok" {
        Vec::new()
    } else {
        vec![bounded_message(
            rich_status,
            &format!("call relation query returned {rich_status}"),
            "warning",
        )]
    };
    let mut query = serde_json::Map::new();
    query.insert(
        "text".to_string(),
        rich.get("query").cloned().unwrap_or_else(|| json!("")),
    );
    if let Some(entity_id) = rich
        .get("exact_resolved_entity")
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
    {
        query.insert("entity_id".to_string(), json!(entity_id));
    }
    if let Some(resolution_mode) = rich.get("resolution_mode").and_then(Value::as_str) {
        query.insert("resolution_mode".to_string(), json!(resolution_mode));
    }
    if let Some(entity) = rich
        .get("exact_resolved_entity")
        .filter(|value| !value.is_null())
    {
        query.insert(
            "resolved_entity".to_string(),
            agent_compact_entity_ref_from_value(entity),
        );
    }
    query.insert("explicit_limit".to_string(), json!(output.explicit_limit));
    let graph_proof = results
        .iter()
        .any(|result| result.get("graph_proof").and_then(Value::as_bool) == Some(true));

    let mut response = canonical_agent_query_response(
        "callers_callees_agent_json",
        "query callers-callees",
        repo_root,
        db_path,
        agent_status_from_rich(rich_status),
        lifecycle_summary,
        truncation,
        Value::Object(query),
        results,
        warnings,
        Vec::new(),
        output.output_mode,
        timings,
    );
    if let Some(object) = response.as_object_mut() {
        object.insert("direction".to_string(), json!(key));
        object.insert("graph_proof".to_string(), json!(graph_proof));
        object.insert(
            "proof_status".to_string(),
            json!(if graph_proof {
                "proof_path_found"
            } else {
                "no_proof_path_found"
            }),
        );
        object.insert(
            "proof_strength".to_string(),
            json!(if graph_proof {
                "graph_relation_proof"
            } else {
                "source_navigation_evidence"
            }),
        );
    }
    response
}

pub(crate) fn agent_call_relation_row_json(row: &Value) -> Value {
    let caller = row
        .get("caller")
        .map(agent_compact_entity_ref_from_value)
        .unwrap_or_else(|| json!({}));
    let callee = row
        .get("callee")
        .map(agent_compact_entity_ref_from_value)
        .unwrap_or_else(|| json!({}));
    let edge = row.get("edge").cloned().unwrap_or_else(|| json!({}));
    let relation = edge
        .get("relation")
        .and_then(Value::as_str)
        .unwrap_or("CALLS");
    let source_span = row
        .get("source_span")
        .and_then(agent_source_span_from_value)
        .or_else(|| {
            edge.get("source_span")
                .and_then(agent_source_span_from_value)
        });
    let mut compact_edge = serde_json::Map::new();
    if let Some(edge_id) = edge.get("id").and_then(Value::as_str) {
        compact_edge.insert("edge_id".to_string(), json!(edge_id));
    }
    compact_edge.insert("relation".to_string(), json!(relation));
    compact_edge.insert("source".to_string(), caller.clone());
    compact_edge.insert("target".to_string(), callee.clone());
    if let Some(exactness) = edge.get("exactness").and_then(Value::as_str) {
        compact_edge.insert("exactness".to_string(), json!(exactness));
    }
    if let Some(confidence) = edge.get("confidence").and_then(Value::as_f64) {
        compact_edge.insert("confidence".to_string(), json!(confidence));
    }
    let evidence_role = row
        .get("evidence_role")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    compact_edge.insert("evidence_role".to_string(), json!(evidence_role));
    if let Some(reason) = row.get("classification_reason").and_then(Value::as_str) {
        compact_edge.insert("classification_reason".to_string(), json!(reason));
    }
    if let Some(source) = row.get("classification_source").and_then(Value::as_str) {
        compact_edge.insert("classification_source".to_string(), json!(source));
    }
    if let Some(span) = source_span.clone() {
        compact_edge.insert("source_spans".to_string(), json!([span]));
    }

    let mut result = serde_json::Map::new();
    result.insert("edge".to_string(), Value::Object(compact_edge));
    result.insert("caller".to_string(), caller);
    result.insert("callee".to_string(), callee);
    result.insert("evidence_role".to_string(), json!(evidence_role));
    if let Some(span) = source_span {
        result.insert("span".to_string(), span.clone());
        result.insert("source_span".to_string(), span);
    }
    let graph_proof = edge_value_is_graph_relation_proof(&edge);
    result.insert("graph_proof".to_string(), json!(graph_proof));
    result.insert(
        "proof_status".to_string(),
        json!(if graph_proof {
            "proof_path_found"
        } else {
            "no_proof_path_found"
        }),
    );
    result.insert(
        "proof_strength".to_string(),
        json!(if graph_proof {
            "graph_relation_proof"
        } else {
            "source_navigation_evidence"
        }),
    );
    if let Some(match_kind) = row.get("match_kind").and_then(Value::as_str) {
        result.insert("match_reason".to_string(), json!(match_kind));
    }
    Value::Object(result)
}

pub(crate) fn agent_definition_result_json(hit: &SymbolSearchHit) -> Value {
    let mut value = agent_symbol_search_hit_json(hit);
    if let Some(object) = value.as_object_mut() {
        object.insert("graph_proof".to_string(), json!(false));
        object.insert("proof_status".to_string(), json!("symbol_definition_found"));
        object.insert("proof_strength".to_string(), json!("symbol_definition"));
        object.insert(
            "claimability".to_string(),
            json!({
                "claimable_as": ["symbol_definition"],
                "not_claimable_as": ["graph_relation_proof", "behavior_proof"],
            }),
        );
    }
    value
}

pub(crate) fn agent_reference_edge_json(
    edge: &Edge,
    entity_by_id: &BTreeMap<String, Entity>,
) -> Value {
    let role = query_evidence_role_for_edge(edge);
    let graph_proof = edge_is_graph_relation_proof(edge);
    json!({
        "reference_evidence_kind": "graph_reference",
        "text_reference": false,
        "graph_proof": graph_proof,
        "proof_status": if graph_proof { "proof_path_found" } else { "no_proof_path_found" },
        "proof_strength": if graph_proof { "graph_relation_proof" } else { "source_navigation_evidence" },
        "evidence_role": role.role,
        "classification_reason": role.reason,
        "classification_source": role.source,
        "edge": {
            "edge_id": edge.id,
            "relation": edge.relation.to_string(),
            "exactness": edge.exactness.to_string(),
            "confidence": edge.confidence,
            "source": entity_by_id.get(&edge.head_id).map(agent_entity_ref_json),
            "target": entity_by_id.get(&edge.tail_id).map(agent_entity_ref_json),
            "source_spans": [agent_source_span_json(&edge.source_span)],
            "provenance_edges": edge.provenance_edges,
        },
        "head": entity_by_id.get(&edge.head_id).map(agent_entity_ref_json),
        "tail": entity_by_id.get(&edge.tail_id).map(agent_entity_ref_json),
        "source_span": agent_source_span_json(&edge.source_span),
    })
}

pub(crate) fn agent_path_query_response(
    schema_name: &str,
    command: &str,
    repo_root: &Path,
    db_path: &Path,
    lifecycle_summary: Option<&Value>,
    options: &PathQueryOptions,
    evidence: Vec<PathEvidence>,
    timings: Value,
) -> Value {
    let proof_status = path_query_proof_status(&evidence);
    let proof_strength = path_query_proof_strength(&evidence);
    let graph_proof = evidence.iter().any(path_evidence_is_graph_relation_proof);
    let (evidence, truncation) = truncate_for_agent(evidence, options.output.limit);
    let results = evidence
        .iter()
        .map(agent_path_evidence_result_json)
        .collect::<Vec<_>>();
    let warnings = if results.is_empty() {
        vec![bounded_message(
            "no_proof_path_found",
            "No verified graph path was found for the requested endpoints.",
            "warning",
        )]
    } else {
        Vec::new()
    };
    let mut response = canonical_agent_query_response(
        schema_name,
        command,
        repo_root,
        db_path,
        if results.is_empty() { "warning" } else { "ok" },
        lifecycle_summary,
        truncation,
        json!({
            "source": options.source,
            "target": options.target,
            "explicit_limit": options.output.explicit_limit,
        }),
        results,
        warnings,
        Vec::new(),
        options.output.output_mode,
        timings,
    );
    if let Some(object) = response.as_object_mut() {
        object.insert("graph_proof".to_string(), json!(graph_proof));
        object.insert("proof_status".to_string(), json!(proof_status));
        object.insert("proof_strength".to_string(), json!(proof_strength));
    }
    response
}

pub(crate) fn agent_path_evidence_result_json(path: &PathEvidence) -> Value {
    let graph_proof = path_evidence_is_graph_relation_proof(path);
    let evidence_role = path
        .metadata
        .get("evidence_role")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let edges = agent_path_edges_json(path);
    json!({
        "path_id": path.id,
        "summary": path.summary,
        "source": path.source,
        "source_endpoint": agent_path_endpoint_from_path_id(path, &path.source, "source"),
        "target": path.target,
        "target_endpoint": agent_path_endpoint_from_path_id(path, &path.target, "target"),
        "relations": path.metapath.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "edges": edges,
        "source_spans": path.source_spans.iter().map(agent_source_span_json).collect::<Vec<_>>(),
        "exactness": path.exactness.to_string(),
        "confidence": path.confidence,
        "length": path.length,
        "evidence_role": evidence_role,
        "classification_reason": path.metadata.get("classification_reason").cloned().unwrap_or_else(|| json!("unknown")),
        "classification_source": path.metadata.get("classification_source").cloned().unwrap_or_else(|| json!("unknown")),
        "production_proof_eligible": path.metadata.get("production_proof_eligible").cloned().unwrap_or_else(|| json!(false)),
        "edge_labels": path.metadata.get("edge_labels").cloned().unwrap_or_else(|| json!([])),
        "graph_proof": graph_proof,
        "proof_status": if graph_proof { "proof_path_found" } else { "not_graph_relation_proof" },
        "proof_strength": if graph_proof { "graph_relation_proof" } else { "source_navigation_evidence" },
    })
}

pub(crate) fn agent_path_edges_json(path: &PathEvidence) -> Vec<Value> {
    if let Some(labels) = path.metadata.get("edge_labels").and_then(Value::as_array) {
        let edges = labels
            .iter()
            .map(agent_path_edge_from_label_json)
            .collect::<Vec<_>>();
        if !edges.is_empty() {
            return edges;
        }
    }

    path.edges
        .iter()
        .map(|(source, relation, target)| {
            json!({
                "edge_id": null,
                "edge_id_unavailable": true,
                "edge_id_unavailable_reason": "stored PathEvidence edge tuple has no persisted edge id",
                "source": agent_path_endpoint_ref_from_id(source),
                "relation": relation.to_string(),
                "target": agent_path_endpoint_ref_from_id(target),
                "evidence_role": path
                    .metadata
                    .get("evidence_role")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown"),
                "classification_reason": path
                    .metadata
                    .get("classification_reason")
                    .cloned()
                    .unwrap_or_else(|| json!("unhydrated PathEvidence edge tuple")),
            })
        })
        .collect()
}

pub(crate) fn agent_path_edge_from_label_json(label: &Value) -> Value {
    let edge_id = label.get("edge_id").and_then(Value::as_str);
    let mut object = serde_json::Map::new();
    if let Some(edge_id) = edge_id.filter(|value| !value.trim().is_empty()) {
        object.insert("edge_id".to_string(), json!(edge_id));
    } else {
        object.insert("edge_id".to_string(), Value::Null);
        object.insert("edge_id_unavailable".to_string(), json!(true));
        object.insert(
            "edge_id_unavailable_reason".to_string(),
            json!("persisted edge id was not available in hydrated PathEvidence metadata"),
        );
    }
    object.insert(
        "source".to_string(),
        agent_path_endpoint_from_label(label, "head", "head_id"),
    );
    object.insert(
        "target".to_string(),
        agent_path_endpoint_from_label(label, "tail", "tail_id"),
    );
    object.insert(
        "relation".to_string(),
        label
            .get("relation")
            .cloned()
            .unwrap_or_else(|| json!("unknown")),
    );
    for key in [
        "exactness",
        "confidence",
        "derived",
        "edge_class",
        "fact_class",
        "context",
        "evidence_role",
        "classification_reason",
        "classification_source",
        "provenance_edges",
    ] {
        if let Some(value) = label.get(key).cloned() {
            object.insert(key.to_string(), value);
        }
    }
    if let Some(span) = label
        .get("source_span_detail")
        .and_then(agent_source_span_from_value)
        .or_else(|| {
            label
                .get("source_span")
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
                .map(|path| {
                    json!({
                        "file": path,
                        "start_line": 1,
                        "end_line": 1,
                        "span_unavailable": true,
                    })
                })
        })
    {
        object.insert("source_spans".to_string(), json!([span]));
    }
    Value::Object(object)
}

pub(crate) fn agent_path_endpoint_from_label(label: &Value, prefix: &str, id_key: &str) -> Value {
    let entity_key = format!("{prefix}_entity");
    if let Some(entity) = label.get(&entity_key).filter(|value| value.is_object()) {
        return agent_path_endpoint_ref_from_value(entity);
    }
    let id = label
        .get(id_key)
        .and_then(Value::as_str)
        .unwrap_or_default();
    agent_path_endpoint_ref_from_id(id)
}

pub(crate) fn agent_path_endpoint_from_path_id(
    path: &PathEvidence,
    id: &str,
    _endpoint: &str,
) -> Value {
    if let Some(labels) = path.metadata.get("edge_labels").and_then(Value::as_array) {
        for label in labels {
            for (prefix, id_key) in [("head", "head_id"), ("tail", "tail_id")] {
                if label
                    .get(id_key)
                    .and_then(Value::as_str)
                    .is_some_and(|value| value == id)
                {
                    return agent_path_endpoint_from_label(label, prefix, id_key);
                }
            }
        }
    }
    agent_path_endpoint_ref_from_id(id)
}

pub(crate) fn agent_path_endpoint_ref_from_value(value: &Value) -> Value {
    let id = value
        .get("entity_id")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let display_name = value
        .get("display_name")
        .or_else(|| value.get("name"))
        .or_else(|| value.get("symbol"))
        .or_else(|| value.get("qualified_name"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty());
    let mut object = serde_json::Map::new();
    object.insert("entity_id".to_string(), json!(id));
    object.insert("id".to_string(), json!(id));
    if let Some(display_name) = display_name {
        object.insert("display_name".to_string(), json!(display_name));
        object.insert("name".to_string(), json!(display_name));
        object.insert("symbol".to_string(), json!(display_name));
        object.insert("name_unavailable".to_string(), json!(false));
    } else {
        object.insert("display_name".to_string(), json!(id));
        object.insert("name".to_string(), json!(id));
        object.insert("symbol".to_string(), json!(id));
        object.insert("name_unavailable".to_string(), json!(true));
    }
    for (target, source) in [
        ("qualified_name", "qualified_name"),
        ("kind", "kind"),
        ("file", "file"),
        ("file", "repo_relative_path"),
        ("path", "path"),
        ("path", "repo_relative_path"),
        ("evidence_role", "evidence_role"),
        ("classification_reason", "classification_reason"),
        ("classification_source", "classification_source"),
    ] {
        if object.contains_key(target) {
            continue;
        }
        if let Some(text) = value.get(source).and_then(Value::as_str) {
            object.insert(target.to_string(), json!(text));
        }
    }
    if let Some(span) = value
        .get("span")
        .and_then(agent_source_span_from_value)
        .or_else(|| {
            value
                .get("source_span")
                .and_then(agent_source_span_from_value)
        })
    {
        object.insert("span".to_string(), span.clone());
        object.insert("source_span".to_string(), span);
    }
    Value::Object(object)
}

pub(crate) fn agent_path_endpoint_ref_from_id(id: &str) -> Value {
    json!({
        "entity_id": id,
        "id": id,
        "display_name": id,
        "name": id,
        "symbol": id,
        "name_unavailable": true,
    })
}

pub(crate) fn hydrate_path_evidence_endpoint_labels(
    paths: &mut [PathEvidence],
    entity_by_id: &BTreeMap<String, Entity>,
) {
    for path in paths {
        let Some(labels) = path
            .metadata
            .get_mut("edge_labels")
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for label in labels {
            let Some(object) = label.as_object_mut() else {
                continue;
            };
            let head_id = object
                .get("head_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(head_id) = head_id.as_deref() {
                if let Some(entity) = entity_by_id.get(head_id) {
                    object.insert("head_entity".to_string(), agent_entity_ref_json(entity));
                    let role = query_evidence_role_for_entity(entity);
                    object.insert("head_source_role".to_string(), json!(role.role));
                }
            }
            let tail_id = object
                .get("tail_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(tail_id) = tail_id.as_deref() {
                if let Some(entity) = entity_by_id.get(tail_id) {
                    object.insert("tail_entity".to_string(), agent_entity_ref_json(entity));
                    let role = query_evidence_role_for_entity(entity);
                    object.insert("tail_source_role".to_string(), json!(role.role));
                }
            }
        }
    }
}

pub(crate) fn edge_value_is_graph_relation_proof(edge: &Value) -> bool {
    let exactness = edge
        .get("exactness")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    !matches!(exactness, "static_heuristic" | "inferred" | "unknown")
        && (edge.get("source_spans").is_some() || edge.get("source_span").is_some())
}

pub(crate) fn edge_is_graph_relation_proof(edge: &Edge) -> bool {
    !matches!(
        edge.exactness,
        Exactness::StaticHeuristic | Exactness::Inferred
    ) && !edge.source_span.repo_relative_path.is_empty()
}

pub(crate) fn path_evidence_is_graph_relation_proof(path: &PathEvidence) -> bool {
    if path.length == 0
        || matches!(
            path.exactness,
            Exactness::StaticHeuristic | Exactness::Inferred
        )
        || path.source_spans.len() < path.length
    {
        return false;
    }
    if path
        .metadata
        .get("proof_grade_edge_classes")
        .and_then(Value::as_bool)
        == Some(false)
    {
        return false;
    }
    if path
        .metadata
        .get("derived_edges_have_provenance")
        .and_then(Value::as_bool)
        == Some(false)
    {
        return false;
    }
    true
}

pub(crate) fn path_query_proof_status(paths: &[PathEvidence]) -> &'static str {
    if paths.iter().any(path_evidence_is_graph_relation_proof) {
        "proof_path_found"
    } else if paths.is_empty() {
        "no_proof_path_found"
    } else {
        "not_graph_relation_proof"
    }
}

pub(crate) fn path_query_proof_strength(paths: &[PathEvidence]) -> &'static str {
    if paths.iter().any(path_evidence_is_graph_relation_proof) {
        "graph_relation_proof"
    } else if paths.is_empty() {
        "none"
    } else {
        "source_navigation_evidence"
    }
}

pub(crate) fn agent_compact_entity_ref_from_value(value: &Value) -> Value {
    let id = value
        .get("id")
        .or_else(|| value.get("entity_id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = value
        .get("name")
        .or_else(|| value.get("display_name"))
        .or_else(|| value.get("symbol"))
        .or_else(|| value.get("qualified_name"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(id);
    let mut object = serde_json::Map::new();
    object.insert("id".to_string(), json!(id));
    object.insert("name".to_string(), json!(name));
    if let Some(qualified_name) = value.get("qualified_name").and_then(Value::as_str) {
        object.insert("qualified_name".to_string(), json!(qualified_name));
    }
    if let Some(kind) = value.get("kind").and_then(Value::as_str) {
        object.insert("kind".to_string(), json!(kind));
    }
    if let Some(file) = value
        .get("repo_relative_path")
        .or_else(|| value.get("file"))
        .or_else(|| value.get("path"))
        .and_then(Value::as_str)
    {
        object.insert("file".to_string(), json!(file));
    }
    let name_unavailable = name.is_empty() || name == id;
    if name_unavailable {
        object.insert("name_unavailable".to_string(), json!(true));
    }
    if let Some(span) = value
        .get("source_span")
        .and_then(agent_source_span_from_value)
    {
        object.insert("source_span".to_string(), span);
    }
    Value::Object(object)
}

pub(crate) fn agent_source_span_from_value(value: &Value) -> Option<Value> {
    let file = value
        .get("repo_relative_path")
        .or_else(|| value.get("file"))
        .and_then(Value::as_str)?;
    let start_line = value.get("start_line").and_then(Value::as_u64)?;
    let end_line = value
        .get("end_line")
        .and_then(Value::as_u64)
        .unwrap_or(start_line);
    let mut object = serde_json::Map::new();
    object.insert("file".to_string(), json!(file));
    object.insert("start_line".to_string(), json!(start_line));
    object.insert("end_line".to_string(), json!(end_line));
    if let Some(start_column) = value.get("start_column").and_then(Value::as_u64) {
        object.insert("start_column".to_string(), json!(start_column));
    }
    if let Some(end_column) = value.get("end_column").and_then(Value::as_u64) {
        object.insert("end_column".to_string(), json!(end_column));
    }
    Some(Value::Object(object))
}

pub(crate) fn resolve_symbol_candidates(
    store: &SqliteGraphStore,
    value: &str,
    limit: usize,
) -> Result<Vec<Entity>, String> {
    let mut seen = BTreeSet::new();
    let mut entities = Vec::new();
    for entity in store
        .find_entities_by_exact_symbol(value)
        .map_err(|error| error.to_string())?
    {
        if seen.insert(entity.id.clone()) {
            entities.push(entity);
        }
    }
    if entities.len() < limit {
        for hit in symbol_search_hits(store, value, limit)? {
            if seen.insert(hit.entity.id.clone()) {
                entities.push(hit.entity);
            }
            if entities.len() >= limit {
                break;
            }
        }
    }
    Ok(entities)
}

pub(crate) fn call_chain_paths(
    edges: &[Edge],
    entity_by_id: &BTreeMap<String, Entity>,
    source_entities: &[Entity],
    target_aliases: &BTreeSet<String>,
    limits: QueryLimits,
) -> Vec<GraphPath> {
    let mut outgoing = BTreeMap::<String, Vec<Edge>>::new();
    for edge in edges
        .iter()
        .filter(|edge| edge.relation == RelationKind::Calls)
    {
        outgoing
            .entry(edge.head_id.clone())
            .or_default()
            .push(edge.clone());
    }

    let ids_by_alias = ids_by_alias(entity_by_id);
    let mut paths = Vec::new();
    let mut queue = std::collections::VecDeque::<(String, String, Vec<TraversalStep>)>::new();
    for source in source_entities {
        queue.push_back((source.id.clone(), source.id.clone(), Vec::new()));
    }
    let mut visited = BTreeSet::<(String, usize)>::new();
    let mut edges_visited = 0usize;

    while let Some((origin, current, steps)) = queue.pop_front() {
        if paths.len() >= limits.max_paths || edges_visited >= limits.max_edges_visited {
            break;
        }
        if steps.len() >= limits.max_depth {
            continue;
        }
        if !visited.insert((current.clone(), steps.len())) {
            continue;
        }
        for head_id in equivalent_entity_ids(&current, entity_by_id, &ids_by_alias) {
            let Some(call_edges) = outgoing.get(&head_id) else {
                continue;
            };
            for edge in call_edges {
                edges_visited += 1;
                let mut next_steps = steps.clone();
                next_steps.push(TraversalStep {
                    edge: edge.clone(),
                    direction: TraversalDirection::Forward,
                    from: edge.head_id.clone(),
                    to: edge.tail_id.clone(),
                });
                let tail_aliases = entity_by_id
                    .get(&edge.tail_id)
                    .map(entity_aliases)
                    .unwrap_or_else(|| BTreeSet::from([normalize_symbol_alias(&edge.tail_id)]));
                if aliases_overlap(&tail_aliases, target_aliases)
                    || target_aliases.contains(&edge.tail_id.to_ascii_lowercase())
                {
                    paths.push(call_graph_path(&origin, next_steps.clone()));
                    if paths.len() >= limits.max_paths {
                        break;
                    }
                }
                for next_id in equivalent_entity_ids(&edge.tail_id, entity_by_id, &ids_by_alias) {
                    if next_steps
                        .iter()
                        .filter(|step| step.from == next_id || step.to == next_id)
                        .count()
                        > 1
                    {
                        continue;
                    }
                    queue.push_back((origin.clone(), next_id, next_steps.clone()));
                }
            }
        }
    }

    paths.sort_by(|left, right| {
        left.steps
            .len()
            .cmp(&right.steps.len())
            .then_with(|| right.confidence_score().total_cmp(&left.confidence_score()))
    });
    paths
}

pub(crate) fn call_graph_path(source: &str, steps: Vec<TraversalStep>) -> GraphPath {
    let target = steps
        .last()
        .map(|step| step.to.clone())
        .unwrap_or_else(|| source.to_string());
    let uncertainty = steps
        .iter()
        .map(|step| match step.edge.exactness {
            Exactness::Exact | Exactness::CompilerVerified | Exactness::LspVerified => 0.0,
            Exactness::ParserVerified => 0.08,
            Exactness::DynamicTrace => 0.04,
            Exactness::DerivedFromVerifiedEdges => 0.12,
            Exactness::StaticHeuristic | Exactness::Inferred => 0.35,
        })
        .sum::<f64>();
    GraphPath {
        source: source.to_string(),
        target,
        cost: steps.len() as f64 + uncertainty,
        uncertainty,
        steps,
    }
}

pub(crate) trait GraphPathConfidence {
    fn confidence_score(&self) -> f64;
}

impl GraphPathConfidence for GraphPath {
    fn confidence_score(&self) -> f64 {
        if self.steps.is_empty() {
            return 1.0;
        }
        self.steps
            .iter()
            .map(|step| step.edge.confidence)
            .fold(1.0_f64, f64::min)
            / (1.0 + self.uncertainty)
    }
}

pub(crate) fn resolve_symbol_or_literal(
    store: &SqliteGraphStore,
    value: &str,
) -> Result<String, String> {
    let hits = store
        .find_entities_by_exact_symbol(value)
        .map_err(|error| error.to_string())?;
    Ok(hits
        .first()
        .map(|entity| entity.id.clone())
        .unwrap_or_else(|| value.to_string()))
}

pub(crate) fn resolve_impact_seeds(
    store: &SqliteGraphStore,
    target: &str,
) -> Result<Vec<String>, String> {
    let normalized = target.replace('\\', "/");
    let mut seeds = Vec::new();
    for entity in store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?
    {
        if entity.repo_relative_path == normalized
            || entity.repo_relative_path.ends_with(&normalized)
        {
            seeds.push(entity.id);
        }
    }
    if seeds.is_empty() {
        seeds.extend(
            store
                .find_entities_by_exact_symbol(target)
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|entity| entity.id),
        );
    }
    if seeds.is_empty() {
        seeds.push(target.to_string());
    }
    seeds.sort();
    seeds.dedup();
    Ok(seeds)
}
