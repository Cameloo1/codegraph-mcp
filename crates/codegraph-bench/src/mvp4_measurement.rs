//! MVP4 baseline measurement and audit-only AST census helpers.
//!
//! This module is preparation-only. It never persists MVP4 facts into a
//! production graph, never exposes proof labels, and never changes parser output.

use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use codegraph_core::{EntityKind, SourceSpan};
use codegraph_parser::{
    detect_language, extract_entities_and_relations, LanguageParser, SourceLanguage,
    TreeSitterParser,
};
use codegraph_store::{SqliteGraphStore, SCHEMA_VERSION};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::{BenchResult, BenchmarkError};

pub const MVP4_MEASUREMENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4AstCensusOptions {
    pub max_file_bytes: u64,
    pub proposed_caps: Option<Mvp4ProposedCaps>,
}

impl Default for Mvp4AstCensusOptions {
    fn default() -> Self {
        Self {
            max_file_bytes: 2 * 1024 * 1024,
            proposed_caps: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4ProposedCaps {
    pub max_micro_nodes_per_function: u64,
    pub max_micro_edges_per_function: u64,
    pub max_packet_steps: u64,
    pub max_packet_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Mvp4CandidateCounts {
    pub candidate_function_frames: u64,
    pub parameters: u64,
    pub local_bindings: u64,
    pub assignments: u64,
    pub calls: u64,
    pub returns: u64,
    pub conditions: u64,
    pub mutation_sites: u64,
    pub property_accesses: u64,
    pub literal_keys: u64,
    pub estimated_micro_nodes: u64,
    pub estimated_micro_edges: u64,
}

impl Mvp4CandidateCounts {
    pub fn add_assign(&mut self, other: &Self) {
        self.candidate_function_frames += other.candidate_function_frames;
        self.parameters += other.parameters;
        self.local_bindings += other.local_bindings;
        self.assignments += other.assignments;
        self.calls += other.calls;
        self.returns += other.returns;
        self.conditions += other.conditions;
        self.mutation_sites += other.mutation_sites;
        self.property_accesses += other.property_accesses;
        self.literal_keys += other.literal_keys;
        self.estimated_micro_nodes += other.estimated_micro_nodes;
        self.estimated_micro_edges += other.estimated_micro_edges;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4AstCensusReport {
    pub schema_version: u32,
    pub method: String,
    pub parser_api_used: bool,
    pub parser_api_boundary: String,
    pub production_facts_persisted: bool,
    pub proof_labels_activated: bool,
    pub roots: Vec<String>,
    pub files: Vec<Mvp4AstCensusFile>,
    pub aggregate: Mvp4AstCensusAggregate,
    pub firehose_guards: Vec<Mvp4FirehoseGuard>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4AstCensusFile {
    pub root: String,
    pub path: String,
    pub language: String,
    pub status: String,
    pub parser_status: String,
    pub byte_len: u64,
    pub line_count: u64,
    pub parser_entity_count: u64,
    pub parser_edge_count: u64,
    pub parser_function_entities: u64,
    pub parser_callsite_entities: u64,
    pub parser_assignment_entities: u64,
    pub parser_return_entities: u64,
    pub parser_relation_counts: BTreeMap<String, u64>,
    pub syntax_diagnostic_count: u64,
    pub counts: Mvp4CandidateCounts,
    pub functions: Vec<Mvp4FunctionDensity>,
    pub functions_exceeding_proposed_caps: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4FunctionDensity {
    pub source_span: String,
    pub counts: Mvp4CandidateCounts,
    pub exceeds_proposed_caps: Option<bool>,
    pub cap_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4AstCensusAggregate {
    pub file_count: u64,
    pub parsed_file_count: u64,
    pub skipped_file_count: u64,
    pub languages: BTreeMap<String, u64>,
    pub counts: Mvp4CandidateCounts,
    pub parser_entity_count: u64,
    pub parser_edge_count: u64,
    pub syntax_diagnostic_count: u64,
    pub max_estimated_micro_nodes_per_function: u64,
    pub max_estimated_micro_edges_per_function: u64,
    pub functions_exceeding_proposed_caps: Option<u64>,
    pub caps_applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4FirehoseGuard {
    pub guard_id: String,
    pub guard_kind: String,
    pub description: String,
    pub status: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4StorageProjectionOptions {
    pub sample_micro_nodes: u64,
    pub sample_micro_edges: u64,
    pub sample_local_flow_packets: u64,
}

impl Default for Mvp4StorageProjectionOptions {
    fn default() -> Self {
        Self {
            sample_micro_nodes: 512,
            sample_micro_edges: 512,
            sample_local_flow_packets: 128,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4StorageProjection {
    pub schema_version: u32,
    pub codegraph_schema_version: u32,
    pub method: String,
    pub production_facts_persisted: bool,
    pub empty_schema_bytes: u64,
    pub empty_schema_measurement_note: String,
    pub samples: Vec<Mvp4StorageTableProjection>,
    pub temp_db_removed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4StorageTableProjection {
    pub table: String,
    pub sampled_rows: u64,
    pub payload_bytes: u64,
    pub payload_bytes_per_row: u64,
    pub sqlite_file_delta_bytes: u64,
    pub sqlite_file_delta_bytes_per_row: u64,
    pub index_overhead_note: String,
}

pub fn run_mvp4_ast_census(
    roots: &[PathBuf],
    options: &Mvp4AstCensusOptions,
) -> BenchResult<Mvp4AstCensusReport> {
    if roots.is_empty() {
        return Err(BenchmarkError::Validation(
            "mvp4 AST census requires at least one root".to_string(),
        ));
    }

    let mut files = Vec::new();
    for root in roots {
        collect_census_files(root, root, options, &mut files)?;
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let aggregate = aggregate_census(&files, options.proposed_caps.is_some());

    Ok(Mvp4AstCensusReport {
        schema_version: MVP4_MEASUREMENT_SCHEMA_VERSION,
        method: "audit_only_tree_sitter_census_with_source_text_counts_not_proof".to_string(),
        parser_api_used: true,
        parser_api_boundary: "Uses current parser parse/extract APIs for file/entity/relation context; raw AST micro-node semantics remain audit-only estimates and are not product facts.".to_string(),
        production_facts_persisted: false,
        proof_labels_activated: false,
        roots: roots
            .iter()
            .map(|root| root.display().to_string())
            .collect(),
        files,
        aggregate,
        firehose_guards: mvp4_firehose_guards(),
    })
}

pub fn measure_mvp4_sparse_sidecar_projection(
    work_dir: &Path,
    options: &Mvp4StorageProjectionOptions,
) -> BenchResult<Mvp4StorageProjection> {
    fs::create_dir_all(work_dir)?;
    let empty_db = unique_db_path(work_dir, "empty");
    let empty_schema_bytes = create_actual_schema_db_and_size(&empty_db)?;
    remove_sqlite_file_set(&empty_db)?;

    let samples = vec![
        measure_sidecar_table(
            work_dir,
            "ast_micro_nodes",
            options.sample_micro_nodes,
            insert_sample_micro_nodes,
        )?,
        measure_sidecar_table(
            work_dir,
            "ast_micro_edges",
            options.sample_micro_edges,
            insert_sample_micro_edges,
        )?,
        measure_sidecar_table(
            work_dir,
            "local_flow_packets",
            options.sample_local_flow_packets,
            insert_sample_local_flow_packets,
        )?,
    ];

    Ok(Mvp4StorageProjection {
        schema_version: MVP4_MEASUREMENT_SCHEMA_VERSION,
        codegraph_schema_version: SCHEMA_VERSION,
        method: "disposable_sqlite_dbs_created_by_current_SqliteGraphStore_migration_then_populated_with_dormant_sidecar_sample_rows".to_string(),
        production_facts_persisted: false,
        empty_schema_bytes,
        empty_schema_measurement_note: "Main SQLite file plus WAL/SHM sidecars, after current store migration and before MVP4 rows.".to_string(),
        samples,
        temp_db_removed: true,
    })
}

pub fn mvp4_firehose_guards() -> Vec<Mvp4FirehoseGuard> {
    vec![
        guard("huge_function", "synthetic huge function"),
        guard("many_repeated_assignments", "many repeated assignments"),
        guard(
            "generated_code",
            "generated source stays bounded and non-production by default",
        ),
        guard("deeply_nested_scopes", "deeply nested scopes"),
        guard("many_callsites", "many call sites"),
        guard("many_literals", "many literal keys"),
        guard("many_branches", "many branches"),
    ]
}

fn guard(id: &str, description: &str) -> Mvp4FirehoseGuard {
    Mvp4FirehoseGuard {
        guard_id: id.to_string(),
        guard_kind: "mvp4_census_firehose_boundary".to_string(),
        description: description.to_string(),
        status: "ready".to_string(),
        evidence:
            "covered by mvp4_measurement census/firehose tests; no production extraction required"
                .to_string(),
    }
}

fn collect_census_files(
    root: &Path,
    current: &Path,
    options: &Mvp4AstCensusOptions,
    files: &mut Vec<Mvp4AstCensusFile>,
) -> BenchResult<()> {
    if should_skip_path(current) {
        return Ok(());
    }
    if current.is_dir() {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            collect_census_files(root, &entry.path(), options, files)?;
        }
        return Ok(());
    }
    if current.is_file() && detect_language(current).is_some() {
        files.push(census_file(root, current, options)?);
    }
    Ok(())
}

fn should_skip_path(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::Normal(value)
                if matches!(
                    value.to_string_lossy().as_ref(),
                    ".git"
                        | ".codegraph"
                        | ".codegraph-competitors"
                        | ".agents"
                        | ".claude"
                        | ".codex"
                        | ".github"
                        | ".mypy_cache"
                        | ".pytest_cache"
                        | ".ruff_cache"
                        | ".venv"
                        | "__pycache__"
                        | "benchmarks"
                        | "assets"
                        | "target"
                        | "target-pr19-ci"
                        | "node_modules"
                        | "reports"
                        | "benchmark-results"
                        | "site-packages"
                        | "storage_tmp"
                        | "venv"
                )
        )
    })
}

fn census_file(
    root: &Path,
    path: &Path,
    options: &Mvp4AstCensusOptions,
) -> BenchResult<Mvp4AstCensusFile> {
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let language = detect_language(path)
        .map(|language| language.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let metadata = fs::metadata(path)?;
    if metadata.len() > options.max_file_bytes {
        return Ok(empty_census_file(
            root,
            &relative,
            &language,
            "skipped_file_too_large",
            metadata.len(),
        ));
    }
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            let mut file = empty_census_file(
                root,
                &relative,
                &language,
                "skipped_non_utf8_or_unreadable",
                0,
            );
            file.parser_status = error.to_string();
            return Ok(file);
        }
    };
    let byte_len = source.len() as u64;
    let line_count = source.lines().count() as u64;
    let parser = TreeSitterParser;
    let parsed = parser
        .parse(&relative, &source)
        .map_err(|error| BenchmarkError::Parse(error.to_string()))?;
    let Some(parsed) = parsed else {
        return Ok(empty_census_file(
            root,
            &relative,
            &language,
            "skipped_unsupported_language",
            byte_len,
        ));
    };
    let extraction = extract_entities_and_relations(&parsed, &source);
    let mut relation_counts = BTreeMap::new();
    for edge in &extraction.edges {
        *relation_counts
            .entry(edge.relation.as_str().to_string())
            .or_insert(0) += 1;
    }

    let mut counts = Mvp4CandidateCounts::default();
    let mut functions = Vec::new();
    walk_ast(
        parsed.tree().root_node(),
        parsed.language,
        &source,
        &mut counts,
        Some(&mut functions),
        &options.proposed_caps,
    );
    finalize_estimates(&mut counts);

    let functions_exceeding_proposed_caps = options.proposed_caps.as_ref().map(|_| {
        functions
            .iter()
            .filter(|function| function.exceeds_proposed_caps == Some(true))
            .count() as u64
    });

    Ok(Mvp4AstCensusFile {
        root: root.display().to_string(),
        path: relative,
        language,
        status: "counted".to_string(),
        parser_status: if parsed.has_syntax_errors() {
            "parsed_with_syntax_diagnostics".to_string()
        } else {
            "parsed".to_string()
        },
        byte_len,
        line_count,
        parser_entity_count: extraction.entities.len() as u64,
        parser_edge_count: extraction.edges.len() as u64,
        parser_function_entities: extraction
            .entities
            .iter()
            .filter(|entity| matches!(entity.kind, EntityKind::Function | EntityKind::Method))
            .count() as u64,
        parser_callsite_entities: extraction
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::CallSite)
            .count() as u64,
        parser_assignment_entities: extraction
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::Assignment)
            .count() as u64,
        parser_return_entities: extraction
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::ReturnSite)
            .count() as u64,
        parser_relation_counts: relation_counts,
        syntax_diagnostic_count: parsed.diagnostics.len() as u64,
        counts,
        functions,
        functions_exceeding_proposed_caps,
    })
}

fn empty_census_file(
    root: &Path,
    path: &str,
    language: &str,
    status: &str,
    byte_len: u64,
) -> Mvp4AstCensusFile {
    Mvp4AstCensusFile {
        root: root.display().to_string(),
        path: path.to_string(),
        language: language.to_string(),
        status: status.to_string(),
        parser_status: status.to_string(),
        byte_len,
        line_count: 0,
        parser_entity_count: 0,
        parser_edge_count: 0,
        parser_function_entities: 0,
        parser_callsite_entities: 0,
        parser_assignment_entities: 0,
        parser_return_entities: 0,
        parser_relation_counts: BTreeMap::new(),
        syntax_diagnostic_count: 0,
        counts: Mvp4CandidateCounts::default(),
        functions: Vec::new(),
        functions_exceeding_proposed_caps: None,
    }
}

fn walk_ast(
    node: tree_sitter::Node<'_>,
    language: SourceLanguage,
    source: &str,
    counts: &mut Mvp4CandidateCounts,
    functions: Option<&mut Vec<Mvp4FunctionDensity>>,
    caps: &Option<Mvp4ProposedCaps>,
) {
    count_node(node, language, source, counts);
    let mut functions = functions;
    if is_function_node(language, node.kind()) {
        if let Some(list) = functions.as_deref_mut() {
            let mut function_counts = Mvp4CandidateCounts::default();
            walk_ast(node, language, source, &mut function_counts, None, caps);
            finalize_estimates(&mut function_counts);
            let (exceeds, reason) = caps
                .as_ref()
                .map(|caps| function_exceeds_caps(&function_counts, caps))
                .unwrap_or((None, None));
            list.push(Mvp4FunctionDensity {
                source_span: source_span_string(node),
                counts: function_counts,
                exceeds_proposed_caps: exceeds,
                cap_reason: reason,
            });
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_ast(
            child,
            language,
            source,
            counts,
            functions.as_deref_mut(),
            caps,
        );
    }
}

fn count_node(
    node: tree_sitter::Node<'_>,
    language: SourceLanguage,
    source: &str,
    counts: &mut Mvp4CandidateCounts,
) {
    let kind = node.kind();
    if is_function_node(language, kind) {
        counts.candidate_function_frames += 1;
    }
    if is_parameter_node(kind) {
        counts.parameters += 1;
    }
    if is_local_binding_node(language, kind) {
        counts.local_bindings += 1;
    }
    if is_assignment_node(language, kind) {
        counts.assignments += 1;
    }
    if is_call_node(kind) {
        counts.calls += 1;
    }
    if is_return_node(kind) {
        counts.returns += 1;
    }
    if is_condition_node(kind) {
        counts.conditions += 1;
    }
    if is_property_access_node(kind) {
        counts.property_accesses += 1;
    }
    if is_literal_key_node(kind) {
        counts.literal_keys += 1;
    }
    if is_mutation_site_node(kind) || node_text(source, node).is_some_and(looks_like_mutation) {
        counts.mutation_sites += 1;
    }
}

fn finalize_estimates(counts: &mut Mvp4CandidateCounts) {
    counts.estimated_micro_nodes = counts.candidate_function_frames
        + counts.parameters
        + counts.local_bindings
        + counts.assignments
        + counts.calls
        + counts.returns
        + counts.conditions
        + counts.mutation_sites
        + counts.property_accesses
        + counts.literal_keys;
    counts.estimated_micro_edges = counts.assignments
        + counts.calls
        + counts.returns
        + counts.conditions
        + counts.mutation_sites
        + counts.property_accesses;
}

fn is_function_node(language: SourceLanguage, kind: &str) -> bool {
    match language {
        SourceLanguage::JavaScript
        | SourceLanguage::Jsx
        | SourceLanguage::TypeScript
        | SourceLanguage::Tsx => {
            matches!(
                kind,
                "function_declaration"
                    | "function"
                    | "function_expression"
                    | "method_definition"
                    | "arrow_function"
                    | "generator_function_declaration"
            )
        }
        SourceLanguage::Rust => matches!(kind, "function_item" | "closure_expression"),
        SourceLanguage::Python => matches!(kind, "function_definition" | "lambda"),
        SourceLanguage::Go => matches!(
            kind,
            "function_declaration" | "method_declaration" | "func_literal"
        ),
        _ => kind.contains("function") || kind.contains("method"),
    }
}

fn is_parameter_node(kind: &str) -> bool {
    matches!(
        kind,
        "required_parameter"
            | "optional_parameter"
            | "formal_parameter"
            | "parameter"
            | "self_parameter"
            | "variadic_parameter"
            | "default_parameter"
            | "typed_parameter"
    )
}

fn is_local_binding_node(language: SourceLanguage, kind: &str) -> bool {
    matches!(
        kind,
        "variable_declarator" | "lexical_declaration" | "let_declaration" | "short_var_declaration"
    ) || (language == SourceLanguage::Python && matches!(kind, "assignment"))
}

fn is_assignment_node(language: SourceLanguage, kind: &str) -> bool {
    kind.contains("assignment")
        || matches!(
            kind,
            "variable_declarator"
                | "let_declaration"
                | "short_var_declaration"
                | "augmented_assignment_expression"
        )
        || (language == SourceLanguage::Go && matches!(kind, "var_declaration"))
}

fn is_call_node(kind: &str) -> bool {
    matches!(
        kind,
        "call_expression" | "macro_invocation" | "scoped_call_expression"
    )
}

fn is_return_node(kind: &str) -> bool {
    matches!(kind, "return_statement")
}

fn is_condition_node(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement"
            | "if_expression"
            | "while_statement"
            | "while_expression"
            | "for_statement"
            | "for_in_statement"
            | "for_expression"
            | "match_expression"
            | "switch_statement"
            | "conditional_expression"
    )
}

fn is_property_access_node(kind: &str) -> bool {
    matches!(
        kind,
        "member_expression" | "field_expression" | "selector_expression" | "attribute"
    )
}

fn is_literal_key_node(kind: &str) -> bool {
    matches!(kind, "pair" | "property_identifier" | "field_identifier")
}

fn is_mutation_site_node(kind: &str) -> bool {
    matches!(
        kind,
        "update_expression" | "augmented_assignment_expression"
    )
}

fn looks_like_mutation(text: &str) -> bool {
    const MARKERS: &[&str] = &[
        ".push(", ".pop(", ".insert(", ".remove(", ".splice(", ".set(", ".append(", "+=", "-=",
        "*=", "/=",
    ];
    MARKERS.iter().any(|marker| text.contains(marker))
}

fn node_text<'source>(source: &'source str, node: tree_sitter::Node<'_>) -> Option<&'source str> {
    source.get(node.start_byte()..node.end_byte())
}

fn source_span_string(node: tree_sitter::Node<'_>) -> String {
    let start = node.start_position();
    let end = node.end_position();
    let span = SourceSpan::with_columns(
        "",
        start.row.saturating_add(1) as u32,
        start.column.saturating_add(1) as u32,
        end.row.saturating_add(1) as u32,
        end.column.saturating_add(1) as u32,
    );
    span.to_string()
}

fn function_exceeds_caps(
    counts: &Mvp4CandidateCounts,
    caps: &Mvp4ProposedCaps,
) -> (Option<bool>, Option<String>) {
    if counts.estimated_micro_nodes > caps.max_micro_nodes_per_function {
        return (
            Some(true),
            Some(format!(
                "estimated_micro_nodes {} > cap {}",
                counts.estimated_micro_nodes, caps.max_micro_nodes_per_function
            )),
        );
    }
    if counts.estimated_micro_edges > caps.max_micro_edges_per_function {
        return (
            Some(true),
            Some(format!(
                "estimated_micro_edges {} > cap {}",
                counts.estimated_micro_edges, caps.max_micro_edges_per_function
            )),
        );
    }
    (Some(false), None)
}

fn aggregate_census(files: &[Mvp4AstCensusFile], caps_applied: bool) -> Mvp4AstCensusAggregate {
    let mut languages = BTreeMap::new();
    let mut counts = Mvp4CandidateCounts::default();
    let mut parser_entity_count = 0;
    let mut parser_edge_count = 0;
    let mut syntax_diagnostic_count = 0;
    let mut max_nodes = 0;
    let mut max_edges = 0;
    let mut exceeding = 0;
    for file in files {
        *languages.entry(file.language.clone()).or_insert(0) += 1;
        counts.add_assign(&file.counts);
        parser_entity_count += file.parser_entity_count;
        parser_edge_count += file.parser_edge_count;
        syntax_diagnostic_count += file.syntax_diagnostic_count;
        for function in &file.functions {
            max_nodes = max_nodes.max(function.counts.estimated_micro_nodes);
            max_edges = max_edges.max(function.counts.estimated_micro_edges);
            if function.exceeds_proposed_caps == Some(true) {
                exceeding += 1;
            }
        }
    }
    Mvp4AstCensusAggregate {
        file_count: files.len() as u64,
        parsed_file_count: files.iter().filter(|file| file.status == "counted").count() as u64,
        skipped_file_count: files.iter().filter(|file| file.status != "counted").count() as u64,
        languages,
        counts,
        parser_entity_count,
        parser_edge_count,
        syntax_diagnostic_count,
        max_estimated_micro_nodes_per_function: max_nodes,
        max_estimated_micro_edges_per_function: max_edges,
        functions_exceeding_proposed_caps: caps_applied.then_some(exceeding),
        caps_applied,
    }
}

fn create_actual_schema_db_and_size(path: &Path) -> BenchResult<u64> {
    let store = SqliteGraphStore::open(path).map_err(store_error)?;
    assert!(
        store.sparse_sidecar_schema_ready().map_err(store_error)?,
        "current sparse sidecar schema must be ready"
    );
    drop(store);
    Ok(sqlite_file_set_size(path))
}

fn measure_sidecar_table(
    work_dir: &Path,
    table: &str,
    rows: u64,
    insert: fn(&Connection, u64) -> BenchResult<()>,
) -> BenchResult<Mvp4StorageTableProjection> {
    let db_path = unique_db_path(work_dir, table);
    let empty_bytes = create_actual_schema_db_and_size(&db_path)?;
    let connection =
        Connection::open(&db_path).map_err(|error| BenchmarkError::Store(error.to_string()))?;
    for index in 0..rows {
        insert(&connection, index)?;
    }
    connection
        .execute_batch("PRAGMA wal_checkpoint(FULL);")
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    drop(connection);
    let after_bytes = sqlite_file_set_size(&db_path);
    let store = SqliteGraphStore::open(&db_path).map_err(store_error)?;
    let accounting = store.storage_accounting().map_err(store_error)?;
    let payload_bytes = accounting
        .iter()
        .find(|row| row.name == table)
        .map(|row| row.payload_bytes)
        .unwrap_or(0);
    drop(store);
    remove_sqlite_file_set(&db_path)?;
    let delta = after_bytes.saturating_sub(empty_bytes);
    Ok(Mvp4StorageTableProjection {
        table: table.to_string(),
        sampled_rows: rows,
        payload_bytes,
        payload_bytes_per_row: divide_round_up(payload_bytes, rows),
        sqlite_file_delta_bytes: delta,
        sqlite_file_delta_bytes_per_row: divide_round_up(delta, rows),
        index_overhead_note: "SQLite file delta includes table pages, indexes, and page-allocation granularity; payload bytes use current store accounting formula.".to_string(),
    })
}

fn insert_sample_micro_nodes(connection: &Connection, index: u64) -> BenchResult<()> {
    connection
        .execute(
            "INSERT INTO ast_micro_nodes (
                micro_node_id, file_id, function_entity_id, scope_entity_id,
                micro_kind, symbol, source_span_id, extraction_version,
                schema_version, exactness, provenance_id, source_role, language,
                payload_version, claimability, lifecycle_binding
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                format!("mvp4-node-{index:08}"),
                "src/sample.ts",
                "function:sample",
                "scope:sample",
                if index % 3 == 0 {
                    "CallSite"
                } else {
                    "LocalBinding"
                },
                format!("symbol_{index}"),
                format!("span-node-{index:08}"),
                "mvp4-measurement-v1",
                1_i64,
                "exact",
                Option::<String>::None,
                "production",
                "typescript",
                1_i64,
                "source_spanned",
                "passport:measurement",
            ],
        )
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    Ok(())
}

fn insert_sample_micro_edges(connection: &Connection, index: u64) -> BenchResult<()> {
    connection
        .execute(
            "INSERT INTO ast_micro_edges (
                micro_edge_id, file_id, function_entity_id, scope_entity_id,
                core_edge_id, source_micro_node_id, target_micro_node_id,
                relation_kind, source_span_id, exactness, provenance_id,
                schema_version, extraction_version, source_role, language,
                payload_version, claimability, lifecycle_binding
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                format!("mvp4-edge-{index:08}"),
                "src/sample.ts",
                "function:sample",
                "scope:sample",
                Option::<String>::None,
                format!("mvp4-node-src-{index:08}"),
                format!("mvp4-node-dst-{index:08}"),
                "LOCAL_FLOWS_TO",
                format!("span-edge-{index:08}"),
                "derived_with_provenance",
                format!("prov-edge-{index:08}"),
                1_i64,
                "mvp4-measurement-v1",
                "production",
                "typescript",
                1_i64,
                "source_spanned",
                "passport:measurement",
            ],
        )
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    Ok(())
}

fn insert_sample_local_flow_packets(connection: &Connection, index: u64) -> BenchResult<()> {
    connection
        .execute(
            "INSERT INTO local_flow_packets (
                packet_id, file_id, function_entity_id, packet_kind,
                compressed_steps, source_span_ids_json, primary_source_span_id,
                proof_status, schema_version, extraction_version, exactness,
                provenance_id, source_role, language, payload_version,
                claimability, lifecycle_binding
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                format!("mvp4-packet-{index:08}"),
                "src/sample.ts",
                "function:sample",
                "function_local_flow_packet",
                format!(
                    r#"{{"encoding":"dict_v1","packet_body":{{"dictionary":{{"spans":{{"s0":["src/sample.ts",0,0,0,0]}},"nodes":{{"n0":["LocalBinding","node_{index}","s0","exact","claimable"]}},"edges":{{"e0":["LOCAL_FLOWS_TO","n0","n1","s0","derived_with_provenance","p0"]}},"provenance":{{"p0":["local_assignment_chain_derivation",["e0"],["s0"]]}}}},"paths":[{{"path_id":"return_path_0","steps":[["edge","e0"]]}}],"compression_contract":{{"lossless_to_audit_ordered_steps":true,"source_spans_preserved":true,"provenance_preserved":true,"exactness_preserved":true,"source_roles_preserved":true,"branch_and_return_paths_preserved":true,"unknown_gaps_preserved":true,"full_source_bodies_allowed":false}}}}}}"#
                ),
                format!("[\"span-packet-{index:08}\"]"),
                format!("span-packet-{index:08}"),
                "micro_flow_found",
                1_i64,
                "mvp4-measurement-v1",
                "derived_with_provenance",
                format!("prov-packet-{index:08}"),
                "production",
                "typescript",
                1_i64,
                "exact_local_flow",
                "passport:measurement",
            ],
        )
        .map_err(|error| BenchmarkError::Store(error.to_string()))?;
    Ok(())
}

fn sqlite_file_set_size(path: &Path) -> u64 {
    [
        path.to_path_buf(),
        path.with_extension("db-wal"),
        path.with_extension("db-shm"),
    ]
    .iter()
    .filter_map(|path| fs::metadata(path).ok().map(|metadata| metadata.len()))
    .sum()
}

fn remove_sqlite_file_set(path: &Path) -> BenchResult<()> {
    for path in [
        path.to_path_buf(),
        path.with_extension("db-wal"),
        path.with_extension("db-shm"),
    ] {
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn unique_db_path(work_dir: &Path, label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    work_dir.join(format!(
        "mvp4-measurement-{}-{}-{nanos}.db",
        std::process::id(),
        label
    ))
}

fn divide_round_up(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        0
    } else {
        value.div_ceil(divisor)
    }
}

fn store_error(error: codegraph_store::StoreError) -> BenchmarkError {
    BenchmarkError::Store(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts_source() -> String {
        "export function buildTotal(price: number, tax: number) {\n  const subtotal = price;\n  const total = subtotal + tax;\n  save(total);\n  return total;\n}\n".to_string()
    }

    #[test]
    fn mvp4_ast_census_counts_first_slice_typescript_constructs() {
        let root = unique_test_dir("mvp4-census-first-slice");
        let src = root.join("src");
        fs::create_dir_all(&src).expect("create src");
        fs::write(src.join("order.ts"), ts_source()).expect("write source");

        let report =
            run_mvp4_ast_census(&[root.clone()], &Mvp4AstCensusOptions::default()).expect("census");
        assert_eq!(report.production_facts_persisted, false);
        assert_eq!(report.proof_labels_activated, false);
        assert_eq!(report.aggregate.parsed_file_count, 1);
        assert!(report.aggregate.counts.candidate_function_frames >= 1);
        assert!(report.aggregate.counts.parameters >= 2);
        assert!(report.aggregate.counts.local_bindings >= 1);
        assert!(report.aggregate.counts.calls >= 1);
        assert!(report.aggregate.counts.returns >= 1);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn mvp4_ast_census_firehose_case_stays_audit_only() {
        let root = unique_test_dir("mvp4-census-firehose");
        let src = root.join("src");
        fs::create_dir_all(&src).expect("create src");
        let mut source =
            String::from("export function huge(input: number) {\n  let value = input;\n");
        for index in 0..150 {
            source.push_str(&format!("  value = call_{index}(value);\n"));
        }
        source.push_str("  return value;\n}\n");
        fs::write(src.join("huge.ts"), source).expect("write huge source");

        let report =
            run_mvp4_ast_census(&[root.clone()], &Mvp4AstCensusOptions::default()).expect("census");
        assert!(report.aggregate.counts.assignments >= 100);
        assert!(report.aggregate.counts.calls >= 100);
        assert_eq!(report.aggregate.caps_applied, false);
        assert!(report
            .firehose_guards
            .iter()
            .any(|guard| guard.guard_id == "huge_function"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn mvp4_storage_projection_uses_current_store_schema_without_product_rows() {
        let root = unique_test_dir("mvp4-storage-projection");
        let options = Mvp4StorageProjectionOptions {
            sample_micro_nodes: 8,
            sample_micro_edges: 8,
            sample_local_flow_packets: 4,
        };
        let projection =
            measure_mvp4_sparse_sidecar_projection(&root, &options).expect("projection");

        assert_eq!(projection.production_facts_persisted, false);
        assert_eq!(projection.codegraph_schema_version, SCHEMA_VERSION);
        assert!(projection.empty_schema_bytes > 0);
        assert_eq!(projection.samples.len(), 3);
        assert!(projection
            .samples
            .iter()
            .all(|sample| sample.payload_bytes_per_row > 0));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn mvp4_census_cap_application_is_explicit() {
        let root = unique_test_dir("mvp4-census-caps");
        let src = root.join("src");
        fs::create_dir_all(&src).expect("create src");
        fs::write(src.join("order.ts"), ts_source()).expect("write source");
        let options = Mvp4AstCensusOptions {
            max_file_bytes: 2 * 1024 * 1024,
            proposed_caps: Some(Mvp4ProposedCaps {
                max_micro_nodes_per_function: 1,
                max_micro_edges_per_function: 1,
                max_packet_steps: 1,
                max_packet_bytes: 64,
            }),
        };

        let report = run_mvp4_ast_census(&[root.clone()], &options).expect("census");
        assert_eq!(report.aggregate.caps_applied, true);
        assert!(report
            .aggregate
            .functions_exceeding_proposed_caps
            .is_some_and(|count| count > 0));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn mvp4_measurement_write_census_artifact_when_env_set() {
        let Some(output) = std::env::var_os("CODEGRAPH_MVP4_CENSUS_OUTPUT") else {
            return;
        };
        let roots = std::env::var("CODEGRAPH_MVP4_CENSUS_ROOTS")
            .expect("CODEGRAPH_MVP4_CENSUS_ROOTS must be set with ; separated paths")
            .split(';')
            .filter(|root| !root.trim().is_empty())
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let report = run_mvp4_ast_census(&roots, &Mvp4AstCensusOptions::default())
            .expect("write env requested census");
        let output = PathBuf::from(output);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).expect("create census output parent");
        }
        fs::write(
            &output,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&report).expect("serialize census")
            ),
        )
        .expect("write census output");
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("codegraph-{label}-{}-{nanos}", std::process::id()))
    }
}
