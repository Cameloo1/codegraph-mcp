//! Low-level SQL row/collection helpers and graph/query JSON builders
//! (entity/edge/path JSON, evidence-role classification, alias + test-set sets).
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use std::collections::{BTreeMap, BTreeSet};

use codegraph_core::{Edge, Entity, FileRecord, Metadata};
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::*;

pub(crate) fn sql_json_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

pub(crate) fn sql_parse_error<E>(error: E) -> rusqlite::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

pub(crate) fn collect_sql_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>, String> {
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(crate) fn sqlite_table_exists(connection: &Connection, table: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(|error| error.to_string())
}

pub(crate) fn sqlite_table_has_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, String> {
    if !table
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(format!("invalid table name: {table}"));
    }
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?;
    for row in rows {
        if row.map_err(|error| error.to_string())? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn sqlite_row_count(connection: &Connection, table: &str) -> Result<u64, String> {
    if !table
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(format!("invalid table name: {table}"));
    }
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get::<_, u64>(0)
        })
        .map_err(|error| error.to_string())
}

pub(crate) fn sql_placeholders(count: usize) -> String {
    (0..count).map(|_| "?").collect::<Vec<_>>().join(", ")
}

pub(crate) fn context_pack_explain_query_plans(db_path: &Path) -> Result<Vec<Value>, String> {
    let connection = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    let mut plans = Vec::new();
    if sqlite_table_exists(&connection, "path_evidence_symbols").unwrap_or(false) {
        plans.push((
            "load_stored_path_evidence_for_context_pack",
            "
            SELECT p.id
            FROM path_evidence p
            JOIN path_evidence_symbols s ON s.path_id = p.id
            WHERE s.entity_id = 'repo://e/example'
            ORDER BY p.confidence DESC, p.length ASC, p.id
            LIMIT 256
            ",
        ));
    } else {
        plans.push((
            "load_stored_path_evidence_for_context_pack_legacy",
            "
            SELECT p.id
            FROM path_evidence p
            WHERE p.source = 'repo://e/example' OR p.target = 'repo://e/example'
            ORDER BY p.confidence DESC, p.length ASC, p.id
            LIMIT 256
            ",
        ));
    }
    plans.push((
        "load_bounded_edges_for_context_pack_fallback",
        "
            SELECT e.id_key
            FROM edges e
            JOIN object_id_lookup head ON head.id = e.head_id_key
            JOIN object_id_lookup tail ON tail.id = e.tail_id_key
            JOIN relation_kind_dict relation ON relation.id = e.relation_id
            WHERE (head.value = 'repo://e/example' OR tail.value = 'repo://e/example')
              AND relation.value IN ('CALLS', 'READS', 'WRITES', 'FLOWS_TO')
            ORDER BY e.confidence DESC, e.id_key
            LIMIT 256
            ",
    ));
    plans
        .iter()
        .map(|(name, sql)| explain_query_plan(&connection, name, sql))
        .collect()
}

pub(crate) fn explain_query_plan(
    connection: &Connection,
    name: &str,
    sql: &str,
) -> Result<Value, String> {
    let mut statement = connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "parent": row.get::<_, i64>(1)?,
                "detail": row.get::<_, String>(3)?,
            }))
        })
        .map_err(|error| error.to_string())?;
    let mut details = Vec::new();
    for row in rows {
        details.push(row.map_err(|error| error.to_string())?);
    }
    Ok(json!({
        "name": name,
        "status": "ok",
        "sql": sql.split_whitespace().collect::<Vec<_>>().join(" "),
        "plan": details,
    }))
}

pub(crate) fn default_query_limits() -> QueryLimits {
    QueryLimits {
        max_depth: 6,
        max_paths: 32,
        max_edges_visited: 10_000,
    }
}

pub(crate) fn load_sources(
    repo_root: &Path,
    store: &SqliteGraphStore,
) -> Result<BTreeMap<String, String>, String> {
    let mut sources = BTreeMap::new();
    for file in store
        .list_files(10_000)
        .map_err(|error| error.to_string())?
    {
        let path = repo_root.join(&file.repo_relative_path);
        if path.exists() {
            let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
            sources.insert(file.repo_relative_path, source);
        }
    }
    Ok(sources)
}

pub(crate) fn paths_json(engine: &ExactGraphQueryEngine, paths: Vec<GraphPath>) -> Value {
    serde_json::to_value(engine.path_evidence_from_paths(&paths)).unwrap_or_else(|_| json!([]))
}

pub(crate) fn section_counts(sections: &Value) -> Value {
    let mut counts = serde_json::Map::new();
    if let Some(object) = sections.as_object() {
        for (key, value) in object {
            counts.insert(
                key.clone(),
                json!(value.as_array().map(Vec::len).unwrap_or_default()),
            );
        }
    }
    Value::Object(counts)
}

pub(crate) fn entity_json(entity: &Entity) -> Value {
    json!({
        "id": entity.id,
        "kind": entity.kind.to_string(),
        "name": entity.name,
        "qualified_name": entity.qualified_name,
        "repo_relative_path": entity.repo_relative_path,
        "source_span": entity.source_span,
        "confidence": entity.confidence,
    })
}

pub(crate) fn edge_json(edge: &Edge) -> Value {
    json!({
        "id": edge.id,
        "head_id": edge.head_id,
        "relation": edge.relation.to_string(),
        "tail_id": edge.tail_id,
        "source_span": edge.source_span,
        "confidence": edge.confidence,
        "exactness": edge.exactness.to_string(),
        "edge_class": edge.edge_class.to_string(),
        "context": edge.context.to_string(),
        "derived": edge.derived,
        "provenance_edges": edge.provenance_edges,
        "extractor": edge.extractor,
        "metadata": edge.metadata,
    })
}

pub(crate) fn edge_with_entities_json(
    edge: &Edge,
    entity_by_id: &BTreeMap<String, Entity>,
) -> Value {
    json!({
        "edge": edge_json(edge),
        "head": entity_by_id.get(&edge.head_id).map(entity_json),
        "tail": entity_by_id.get(&edge.tail_id).map(entity_json),
        "unresolved": entity_by_id.get(&edge.tail_id).is_some_and(entity_is_unresolved_reference),
    })
}

pub(crate) fn text_search_hit_json(hit: codegraph_store::TextSearchHit) -> Value {
    let is_text_evidence = matches!(hit.kind, TextSearchKind::File | TextSearchKind::Snippet);
    let mut value = json!({
        "kind": hit.kind.as_str(),
        "id": hit.id,
        "repo_relative_path": hit.repo_relative_path,
        "line": hit.line,
        "title": hit.title,
        "text": hit.text,
        "score": hit.score,
    });
    if is_text_evidence {
        if let Some(object) = value.as_object_mut() {
            insert_text_evidence_labels(object);
        }
    }
    value
}

pub(crate) fn insert_text_evidence_labels(object: &mut serde_json::Map<String, Value>) {
    object.insert("evidence_kind".to_string(), json!("text_evidence"));
    object.insert("evidence_role".to_string(), json!("text_evidence"));
    object.insert(
        "classification_reason".to_string(),
        json!("bounded text evidence; not graph proof"),
    );
    object.insert(
        "classification_source".to_string(),
        json!("text_evidence_lane"),
    );
    object.insert("proof_status".to_string(), json!("not_graph_proof"));
    object.insert("graph_proof".to_string(), json!(false));
    object.insert("claimable_for_text".to_string(), json!(true));
    object.insert("claimable_for_graph".to_string(), json!(false));
    object.insert("graph_relation_claims".to_string(), json!([]));
    object.insert(
        "claimability".to_string(),
        text_evidence_claimability_json(),
    );
}

pub(crate) fn text_evidence_claimability_json() -> Value {
    json!({
        "claimable": true,
        "claimable_for_text": true,
        "claimable_for_graph": false,
        "claimable_as": ["source_text_existence"],
        "not_claimable_as": [
            "typed_graph_relation",
            "CALLS",
            "READS",
            "WRITES",
            "FLOWS_TO",
            "MUTATES",
            "TESTS",
            "ASSERTS"
        ],
        "diagnostic_only": false,
        "reason": "lifecycle-safe indexed text evidence; not graph proof"
    })
}

#[derive(Debug, Clone)]
pub(crate) struct QueryEvidenceRoleLabel {
    pub(crate) role: String,
    pub(crate) reason: String,
    pub(crate) source: String,
}

impl QueryEvidenceRoleLabel {
    fn new(role: impl Into<String>, reason: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            reason: reason.into(),
            source: source.into(),
        }
    }
}

pub(crate) fn query_evidence_role_for_entity(entity: &Entity) -> QueryEvidenceRoleLabel {
    if entity_generated_or_degraded(entity) {
        return QueryEvidenceRoleLabel::new(
            "generated",
            "entity path or metadata identifies generated/degraded source",
            "path_or_degradation_metadata",
        );
    }
    if matches!(entity.kind, EntityKind::Stub) || path_or_symbol_looks_stub(&entity.name) {
        return QueryEvidenceRoleLabel::new(
            "stub",
            "entity is stub evidence",
            "entity_kind_or_name",
        );
    }
    let classified = classify_entity_source_role(entity);
    if classified.role == EvidenceRole::Mock && path_or_symbol_looks_stub(&entity.qualified_name) {
        return QueryEvidenceRoleLabel::new(
            "stub",
            classified.reason,
            classified.classification_source,
        );
    }
    if classified.role != EvidenceRole::Unknown {
        return QueryEvidenceRoleLabel::new(
            classified.role.as_str(),
            classified.reason,
            classified.classification_source,
        );
    }
    query_evidence_role_for_path_and_metadata(
        &entity.repo_relative_path,
        Some(&entity.metadata),
        None,
        "entity_path_fallback",
    )
}

pub(crate) fn query_evidence_role_for_file(file: &FileRecord) -> QueryEvidenceRoleLabel {
    if file_record_is_text_evidence(file) {
        return QueryEvidenceRoleLabel::new(
            "text_evidence",
            "indexed file is a bounded text-evidence lane, not graph proof",
            "file_metadata",
        );
    }
    query_evidence_role_for_path_and_metadata(
        &file.repo_relative_path,
        Some(&file.metadata),
        file.language.as_deref(),
        "file_record",
    )
}

pub(crate) fn query_evidence_role_for_hit_path(path: &str, hit: &Value) -> QueryEvidenceRoleLabel {
    if value_is_text_evidence_hit(hit) {
        return QueryEvidenceRoleLabel::new(
            "text_evidence",
            "query text/file result is bounded text evidence, not graph proof",
            "query_hit",
        );
    }
    query_evidence_role_for_path_and_metadata(path, None, None, "query_hit_path")
}

pub(crate) fn query_evidence_role_for_edge(edge: &Edge) -> QueryEvidenceRoleLabel {
    if edge.relation == RelationKind::Stubs
        || path_or_symbol_looks_stub(&edge.head_id)
        || path_or_symbol_looks_stub(&edge.tail_id)
    {
        return QueryEvidenceRoleLabel::new(
            "stub",
            "edge is stub evidence",
            "relation_or_endpoint",
        );
    }
    if edge_generated_or_degraded(edge) {
        return QueryEvidenceRoleLabel::new(
            "generated",
            "edge path or metadata identifies generated/degraded source",
            "path_or_degradation_metadata",
        );
    }
    let classified = classify_edge_evidence_role(edge);
    if classified.role != EvidenceRole::Unknown {
        return QueryEvidenceRoleLabel::new(
            classified.role.as_str(),
            classified.reason,
            classified.classification_source,
        );
    }
    query_evidence_role_for_path_and_metadata(
        &edge.source_span.repo_relative_path,
        Some(&edge.metadata),
        None,
        "edge_path_fallback",
    )
}

pub(crate) fn query_evidence_role_for_path_and_metadata(
    path: &str,
    metadata: Option<&Metadata>,
    language_or_kind: Option<&str>,
    source: &str,
) -> QueryEvidenceRoleLabel {
    if metadata_has_any_label(metadata, &["text_evidence"]) {
        return QueryEvidenceRoleLabel::new(
            "text_evidence",
            "metadata identifies bounded text evidence, not graph proof",
            source,
        );
    }
    if metadata_has_any_label(
        metadata,
        &["generated", "generated_large", "skipped_generated"],
    ) || path_looks_generated(path)
    {
        return QueryEvidenceRoleLabel::new(
            "generated",
            "path or metadata identifies generated source",
            source,
        );
    }
    if metadata_has_any_label(metadata, &["stub"]) || path_or_symbol_looks_stub(path) {
        return QueryEvidenceRoleLabel::new(
            "stub",
            "path or metadata identifies stub evidence",
            source,
        );
    }
    if metadata_has_any_label(metadata, &["mock"]) || path_or_symbol_looks_mock(path) {
        return QueryEvidenceRoleLabel::new(
            "mock",
            "path or metadata identifies mock evidence",
            source,
        );
    }
    if metadata_has_any_label(metadata, &["test", "fixture"]) || path_looks_test(path) {
        return QueryEvidenceRoleLabel::new(
            "test",
            "path or metadata identifies test evidence",
            source,
        );
    }
    if path.trim().is_empty() && language_or_kind.is_none() {
        return QueryEvidenceRoleLabel::new(
            "unknown",
            "source role cannot be determined from missing path and metadata",
            source,
        );
    }
    QueryEvidenceRoleLabel::new(
        "production",
        "default source role for non-test, non-mock, non-generated source",
        source,
    )
}

pub(crate) fn insert_query_evidence_role_labels(
    object: &mut serde_json::Map<String, Value>,
    label: QueryEvidenceRoleLabel,
) {
    object.insert("evidence_role".to_string(), json!(label.role));
    object.insert("classification_reason".to_string(), json!(label.reason));
    object.insert("classification_source".to_string(), json!(label.source));
}

pub(crate) fn metadata_has_any_label(metadata: Option<&Metadata>, needles: &[&str]) -> bool {
    let Some(metadata) = metadata else {
        return false;
    };
    metadata
        .values()
        .any(|value| value_contains_any_label(value, needles))
}

pub(crate) fn value_contains_any_label(value: &Value, needles: &[&str]) -> bool {
    match value {
        Value::String(text) => {
            let normalized = text.to_ascii_lowercase();
            needles.iter().any(|needle| normalized.contains(needle))
        }
        Value::Array(values) => values
            .iter()
            .any(|value| value_contains_any_label(value, needles)),
        Value::Object(object) => object
            .values()
            .any(|value| value_contains_any_label(value, needles)),
        _ => false,
    }
}

pub(crate) fn entity_generated_or_degraded(entity: &Entity) -> bool {
    path_looks_generated(&entity.repo_relative_path)
        || metadata_has_any_label(
            Some(&entity.metadata),
            &["generated", "generated_large", "skipped_generated"],
        )
}

pub(crate) fn edge_generated_or_degraded(edge: &Edge) -> bool {
    path_looks_generated(&edge.source_span.repo_relative_path)
        || metadata_has_any_label(
            Some(&edge.metadata),
            &["generated", "generated_large", "skipped_generated"],
        )
}

pub(crate) fn path_looks_generated(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    normalized.contains("/generated/")
        || normalized.contains(".generated.")
        || normalized.ends_with(".pb.go")
        || normalized.ends_with(".g.dart")
}

pub(crate) fn path_looks_test(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    normalized.contains("/tests/")
        || normalized.contains("/test/")
        || normalized.contains("/fixtures/")
        || normalized.ends_with("_test.py")
        || normalized.ends_with("_test.go")
        || normalized.ends_with(".test.ts")
        || normalized.ends_with(".test.tsx")
        || normalized.ends_with(".test.js")
        || normalized.ends_with(".test.jsx")
        || normalized.ends_with(".spec.ts")
        || normalized.ends_with(".spec.tsx")
        || normalized.ends_with(".spec.js")
        || normalized.ends_with(".spec.jsx")
}

pub(crate) fn path_or_symbol_looks_mock(value: &str) -> bool {
    value.to_ascii_lowercase().contains("mock")
}

pub(crate) fn path_or_symbol_looks_stub(value: &str) -> bool {
    value.to_ascii_lowercase().contains("stub")
}

pub(crate) fn context_pack_fallback_claimability_json() -> Value {
    json!({
        "claimable": false,
        "claimable_as": [],
        "not_claimable_as": [
            "graph_proof_path",
            "typed_graph_relation",
            "CALLS",
            "READS",
            "WRITES",
            "FLOWS_TO",
            "MUTATES",
            "TESTS",
            "ASSERTS"
        ],
        "diagnostic_only": false,
        "reason": "context-pack fallback evidence is source-span context, not a graph proof path"
    })
}

pub(crate) fn context_pack_unknown_claimability_json() -> Value {
    json!({
        "claimable": false,
        "claimable_as": [],
        "not_claimable_as": [
            "source_text_existence",
            "graph_proof_path",
            "typed_graph_relation"
        ],
        "diagnostic_only": false,
        "reason": "no graph proof path or text evidence was found"
    })
}

pub(crate) fn symbol_search_hit_json(hit: &SymbolSearchHit) -> Value {
    json!({
        "score": hit.score,
        "features": hit.features,
        "matched_terms": hit.matched_terms,
        "entity": entity_json(&hit.entity),
    })
}

pub(crate) fn agent_source_span_json(span: &SourceSpan) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("file".to_string(), json!(span.repo_relative_path));
    object.insert("start_line".to_string(), json!(span.start_line));
    object.insert("end_line".to_string(), json!(span.end_line));
    if let Some(start_column) = span.start_column {
        object.insert("start_column".to_string(), json!(start_column));
    }
    if let Some(end_column) = span.end_column {
        object.insert("end_column".to_string(), json!(end_column));
    }
    Value::Object(object)
}

pub(crate) fn agent_entity_ref_json(entity: &Entity) -> Value {
    let role = query_evidence_role_for_entity(entity);
    let mut object = serde_json::Map::new();
    object.insert("entity_id".to_string(), json!(entity.id));
    object.insert("id".to_string(), json!(entity.id));
    object.insert("display_name".to_string(), json!(entity.name));
    object.insert("name".to_string(), json!(entity.name));
    object.insert("symbol".to_string(), json!(entity.name));
    object.insert("qualified_name".to_string(), json!(entity.qualified_name));
    object.insert("kind".to_string(), json!(entity.kind.to_string()));
    object.insert("file".to_string(), json!(entity.repo_relative_path));
    object.insert("path".to_string(), json!(entity.repo_relative_path));
    object.insert(
        "name_unavailable".to_string(),
        json!(entity.name.trim().is_empty()),
    );
    object.insert("evidence_role".to_string(), json!(role.role));
    object.insert("classification_reason".to_string(), json!(role.reason));
    object.insert("classification_source".to_string(), json!(role.source));
    if let Some(span) = entity.source_span.as_ref() {
        object.insert("span".to_string(), agent_source_span_json(span));
        object.insert("source_span".to_string(), agent_source_span_json(span));
    }
    Value::Object(object)
}

pub(crate) fn agent_symbol_search_hit_json(hit: &SymbolSearchHit) -> Value {
    let role = query_evidence_role_for_entity(&hit.entity);
    let mut object = serde_json::Map::new();
    object.insert("file".to_string(), json!(hit.entity.repo_relative_path));
    object.insert("symbol".to_string(), json!(hit.entity.name));
    object.insert("kind".to_string(), json!(hit.entity.kind.to_string()));
    if let Some(span) = hit.entity.source_span.as_ref().map(agent_source_span_json) {
        object.insert("span".to_string(), span);
    }
    object.insert("score".to_string(), json!(hit.score));
    object.insert("evidence_role".to_string(), json!(role.role));
    object.insert("classification_reason".to_string(), json!(role.reason));
    object.insert("classification_source".to_string(), json!(role.source));
    object.insert("entity".to_string(), agent_entity_ref_json(&hit.entity));
    Value::Object(object)
}

pub(crate) fn agent_text_hit_json(hit: &Value) -> Value {
    let file = hit
        .get("repo_relative_path")
        .or_else(|| hit.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let hit_text = hit.get("text").and_then(Value::as_str).unwrap_or_default();
    let preview = query_snippet_from_text(hit_text, "");
    let line = hit
        .get("line")
        .and_then(Value::as_u64)
        .or_else(|| preview.as_ref().map(|(line, _)| *line));
    let span = line.map(|line| {
        json!({
            "file": file,
            "start_line": line,
            "end_line": line,
        })
    });
    let lines = line
        .map(|line| line.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let score = hit.get("score").and_then(Value::as_f64).unwrap_or(0.0);
    let match_reason = hit
        .get("match")
        .or_else(|| hit.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("text_match");
    let mut object = serde_json::Map::new();
    object.insert("file".to_string(), json!(file));
    object.insert("score".to_string(), json!(score));
    if let Some(span) = span {
        object.insert("span".to_string(), span.clone());
        object.insert("source_span".to_string(), span);
    }
    object.insert(
        "snippet".to_string(),
        json!({
            "file": file,
            "lines": lines,
            "text": preview
                .as_ref()
                .map(|(_, text)| text.as_str())
                .unwrap_or_default(),
            "reason": match_reason,
        }),
    );
    object.insert("match_reason".to_string(), json!(match_reason));
    for key in [
        "graph_output_degraded",
        "degradation_labels",
        "graph_output_claimability",
        "graph_output_budget_hits",
        "graph_relation_claims",
        "graph_extraction_skip_reason",
        "diagnostic_only",
        "claimability",
    ] {
        if let Some(value) = hit.get(key).cloned() {
            object.insert(key.to_string(), value);
        }
    }
    if value_is_text_evidence_hit(hit) {
        insert_text_evidence_labels(&mut object);
    } else {
        insert_text_evidence_labels(&mut object);
        if !file.is_empty() {
            let source_role = query_evidence_role_for_hit_path(file, hit);
            object.insert("source_role".to_string(), json!(source_role.role));
            object.insert("source_role_reason".to_string(), json!(source_role.reason));
            object.insert("source_role_source".to_string(), json!(source_role.source));
        }
    }
    Value::Object(object)
}

pub(crate) fn agent_file_hit_json(hit: &Value) -> Value {
    let file = hit
        .get("repo_relative_path")
        .or_else(|| hit.get("title"))
        .or_else(|| hit.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let score = hit.get("score").and_then(Value::as_f64).unwrap_or(0.0);
    let hit_text = hit.get("text").and_then(Value::as_str).unwrap_or_default();
    let preview = query_snippet_from_text(hit_text, "");
    let line = hit
        .get("line")
        .and_then(Value::as_u64)
        .or_else(|| preview.as_ref().map(|(line, _)| *line));
    let match_reason = hit
        .get("match")
        .or_else(|| hit.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("file_match");
    let mut object = serde_json::Map::new();
    object.insert("file".to_string(), json!(file));
    object.insert("score".to_string(), json!(score));
    if let Some(line) = line {
        let span = json!({
            "file": file,
            "start_line": line,
            "end_line": line,
        });
        object.insert("span".to_string(), span.clone());
        object.insert("source_span".to_string(), span);
    }
    if let Some((line, text)) = preview {
        object.insert(
            "snippet".to_string(),
            json!({
                "file": file,
                "lines": line.to_string(),
                "text": text,
                "reason": match_reason,
            }),
        );
    }
    object.insert("match_reason".to_string(), json!(match_reason));
    for key in [
        "graph_output_degraded",
        "degradation_labels",
        "graph_output_claimability",
        "graph_output_budget_hits",
        "graph_relation_claims",
        "graph_extraction_skip_reason",
        "diagnostic_only",
        "claimability",
    ] {
        if let Some(value) = hit.get(key).cloned() {
            object.insert(key.to_string(), value);
        }
    }
    if value_is_text_evidence_hit(hit) {
        insert_text_evidence_labels(&mut object);
    } else {
        insert_query_evidence_role_labels(&mut object, query_evidence_role_for_hit_path(file, hit));
    }
    Value::Object(object)
}

pub(crate) fn value_is_text_evidence_hit(hit: &Value) -> bool {
    hit.get("evidence_kind").and_then(Value::as_str) == Some("text_evidence")
        || hit.get("evidence_role").and_then(Value::as_str) == Some("text_evidence")
        || hit.get("proof_status").and_then(Value::as_str) == Some("not_graph_proof")
}

pub(crate) fn entities_by_id(entities: &[Entity]) -> BTreeMap<String, Entity> {
    entities
        .iter()
        .map(|entity| (entity.id.clone(), entity.clone()))
        .collect()
}

pub(crate) fn is_definition_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Repository
            | EntityKind::Package
            | EntityKind::Module
            | EntityKind::Class
            | EntityKind::Interface
            | EntityKind::Trait
            | EntityKind::Enum
            | EntityKind::Function
            | EntityKind::Method
            | EntityKind::Constructor
            | EntityKind::GlobalVariable
            | EntityKind::Field
            | EntityKind::Property
            | EntityKind::Type
            | EntityKind::GenericType
            | EntityKind::Import
            | EntityKind::Export
            | EntityKind::Route
            | EntityKind::Endpoint
            | EntityKind::Middleware
            | EntityKind::TestFile
            | EntityKind::TestSuite
            | EntityKind::TestCase
            | EntityKind::Fixture
    )
}

pub(crate) fn entity_aliases(entity: &Entity) -> BTreeSet<String> {
    let mut aliases = BTreeSet::new();
    for value in [
        &entity.id,
        &entity.name,
        &entity.qualified_name,
        &entity.repo_relative_path,
    ] {
        insert_aliases(&mut aliases, value);
    }
    for value in entity.metadata.values() {
        if let Some(text) = value.as_str() {
            insert_aliases(&mut aliases, text);
        }
    }
    aliases
}

pub(crate) fn alias_set_for_entities(entities: &[Entity]) -> BTreeSet<String> {
    entities.iter().flat_map(entity_aliases).collect()
}

pub(crate) fn ids_by_alias(
    entity_by_id: &BTreeMap<String, Entity>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut ids = BTreeMap::<String, BTreeSet<String>>::new();
    for entity in entity_by_id.values() {
        for alias in entity_aliases(entity) {
            ids.entry(alias).or_default().insert(entity.id.clone());
        }
    }
    ids
}

pub(crate) fn equivalent_entity_ids(
    entity_id: &str,
    entity_by_id: &BTreeMap<String, Entity>,
    ids_by_alias: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut ids = BTreeSet::from([entity_id.to_string()]);
    let aliases = entity_by_id
        .get(entity_id)
        .map(entity_aliases)
        .unwrap_or_else(|| BTreeSet::from([normalize_symbol_alias(entity_id)]));
    for alias in aliases {
        if let Some(matches) = ids_by_alias.get(&alias) {
            ids.extend(matches.iter().cloned());
        }
    }
    ids
}

pub(crate) fn insert_aliases(aliases: &mut BTreeSet<String>, value: &str) {
    let normalized = normalize_symbol_alias(value);
    if !normalized.is_empty() {
        aliases.insert(normalized.clone());
    }
    for separator in ['.', ':', '/', '\\', '#', ' '] {
        if let Some(last) = normalized.rsplit(separator).next() {
            if last.len() >= 2 {
                aliases.insert(last.to_string());
            }
        }
    }
    for token in normalized
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|token| token.len() >= 2)
    {
        aliases.insert(token.to_string());
    }
}

pub(crate) fn normalize_symbol_alias(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("call:")
        .trim_start_matches("import:")
        .to_ascii_lowercase()
}

pub(crate) fn aliases_overlap(left: &BTreeSet<String>, right: &BTreeSet<String>) -> bool {
    left.iter().any(|alias| right.contains(alias))
}

pub(crate) fn entity_is_unresolved_reference(entity: &Entity) -> bool {
    entity.created_from.contains("heuristic")
        || entity.name.contains("unknown_callee")
        || entity
            .metadata
            .get("expression_reason")
            .and_then(|value| value.as_str())
            .is_some_and(|reason| reason.contains("unresolved"))
}

pub(crate) fn minimal_test_set(
    paths: &[GraphPath],
    entity_by_id: &BTreeMap<String, Entity>,
) -> Value {
    let mut selected = BTreeMap::<String, (Entity, BTreeSet<String>, f64)>::new();
    for path in paths {
        let Some(test_entity) = test_entity_for_path(path, entity_by_id) else {
            continue;
        };
        let entry = selected.entry(test_entity.id.clone()).or_insert((
            test_entity,
            BTreeSet::new(),
            path.confidence_score(),
        ));
        entry.1.insert(path.source.clone());
        entry.2 = entry.2.max(path.confidence_score());
    }
    let mut rows = selected.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .1
            .len()
            .cmp(&left.1.len())
            .then_with(|| right.2.total_cmp(&left.2))
            .then_with(|| left.0.qualified_name.cmp(&right.0.qualified_name))
    });

    json!({
        "strategy": "greedy cover changed-symbol paths, then prefer higher confidence",
        "runtime_policy": "unknown runtimes are reported as unknown, not guessed",
        "safety_floor": "include every directly connected TESTS/COVERS/ASSERTS/MOCKS/STUBS/FIXTURES_FOR path unless a smaller set covers the same changed symbols",
        "selected_tests": rows.into_iter().map(|(entity, covers, confidence)| {
            json!({
                "test": entity.qualified_name,
                "kind": entity.kind.to_string(),
                "file": entity.repo_relative_path,
                "runtime": "unknown",
                "covers": covers.into_iter().collect::<Vec<_>>(),
                "confidence": confidence,
            })
        }).collect::<Vec<_>>(),
    })
}

pub(crate) fn test_entity_for_path(
    path: &GraphPath,
    entity_by_id: &BTreeMap<String, Entity>,
) -> Option<Entity> {
    entity_by_id
        .get(&path.target)
        .filter(|entity| is_test_kind(entity.kind))
        .cloned()
        .or_else(|| {
            path.steps.iter().find_map(|step| {
                entity_by_id
                    .get(&step.edge.head_id)
                    .filter(|entity| is_test_kind(entity.kind))
                    .cloned()
                    .or_else(|| {
                        entity_by_id
                            .get(&step.edge.tail_id)
                            .filter(|entity| is_test_kind(entity.kind))
                            .cloned()
                    })
            })
        })
}

pub(crate) fn is_test_kind(kind: EntityKind) -> bool {
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
