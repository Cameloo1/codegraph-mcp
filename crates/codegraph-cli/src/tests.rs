use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use codegraph_core::{
    stable_edge_id, stable_entity_id_for_kind, ContextPacket, ContextSnippet, Edge, EdgeClass,
    EdgeContext, Entity, EntityKind, EvidenceRole, Exactness, FileRecord, Metadata, PathEvidence,
    RelationKind, SourceSpan,
};
use codegraph_store::{GraphStore, SqliteGraphStore};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};

use super::{
    benchmark_binary_metadata_for, benchmark_inspection_lifecycle_status,
    compact_context_packet_for_cli, default_db_path, generate_large_synthetic_repo,
    generate_update_integrity_small_repo, index_repo, index_repo_to_db_with_options,
    index_repo_with_options, parse_call_relation_args, parse_call_relation_args_with_output,
    parse_extract_pending_files, parse_index_options, parse_list_query_args, path_string,
    percentile, prepare_watch_startup, query_call_relation, query_call_relation_with_output,
    query_files_with_options, query_symbols_with_options, query_text_with_options, regression_row,
    render_comprehensive_benchmark_markdown, resolve_context_edge_id, route_ui_request, run,
    run_doctor_command, run_status_command, run_update_integrity_repo, serve_ui_loop,
    should_ignore_path, should_start_new_index_batch, stage_update_integrity_mutation_repo,
    update_changed_files_with_cache, CallQueryDirection, CallRelationQueryOptions,
    IncrementalIndexCache, IndexOptions, PendingIndexFile, QueryOutputMode, StorageMode,
    UiResponse, UpdateBenchmarkMode, UpdateLoopKind, WatchDebouncer, BIN_NAME,
    DEFAULT_INDEX_BATCH_MAX_FILES, DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES, SCHEMA_VERSION,
};

static TEMP_REPO_COUNTER: AtomicU64 = AtomicU64::new(0);
static BUNDLE_TEST_LOCK: Mutex<()> = Mutex::new(());
static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_bundle_test() -> std::sync::MutexGuard<'static, ()> {
    BUNDLE_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_env_test() -> std::sync::MutexGuard<'static, ()> {
    ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn id_less_context_fallback_edges_get_distinct_canonical_ids() {
    // Regression guard for the context-pack fallback dedup collision: edges
    // loaded with no persisted canonical id (the historical `edge://unknown`
    // placeholder case) must not collapse onto a single id under the
    // `dedup_by(|l, r| l.id == r.id)` in `load_bounded_context_edges`.
    let span_a = SourceSpan {
        repo_relative_path: "src/a.rs".to_string(),
        start_line: 10,
        start_column: Some(1),
        end_line: 10,
        end_column: Some(20),
    };
    let span_b = SourceSpan {
        repo_relative_path: "src/b.rs".to_string(),
        start_line: 5,
        start_column: Some(1),
        end_line: 5,
        end_column: Some(12),
    };

    // Two distinct id-less edges must resolve to two distinct canonical ids.
    let id_a = resolve_context_edge_id(None, "fn://a", RelationKind::Calls, "fn://b", &span_a);
    let id_b = resolve_context_edge_id(None, "fn://a", RelationKind::Calls, "fn://c", &span_b);
    assert_ne!(
        id_a, id_b,
        "distinct id-less edges must not collapse onto a shared placeholder"
    );
    for id in [&id_a, &id_b] {
        assert!(
            id.starts_with("edge://"),
            "derived id must be canonical: {id}"
        );
        assert_ne!(
            id, "edge://unknown",
            "derived id must not be the placeholder"
        );
    }

    // A blank stored id is treated as absent and derives the same id as `None`.
    let id_blank = resolve_context_edge_id(
        Some("   ".to_string()),
        "fn://a",
        RelationKind::Calls,
        "fn://b",
        &span_a,
    );
    assert_eq!(
        id_blank, id_a,
        "blank stored id must derive the same canonical id as None"
    );

    // A real stored id passes through unchanged (no recomputation).
    let id_real = resolve_context_edge_id(
        Some("edge://real-123".to_string()),
        "fn://a",
        RelationKind::Calls,
        "fn://b",
        &span_a,
    );
    assert_eq!(id_real, "edge://real-123");
}

/// Reference implementation of context-packet compaction using the original
/// full-`serde_json::to_string`-every-iteration shape. The production
/// `compact_context_packet_for_cli` now tracks the serialized byte length
/// incrementally; this mirror lets the test below prove the two are
/// byte-exactly equivalent (identical retained items and identical
/// `estimated_tokens` metadata) across a sweep of budgets.
fn reference_compact_context_packet(packet: &mut ContextPacket, token_budget: usize) {
    fn estimate(packet: &ContextPacket) -> usize {
        serde_json::to_string(packet)
            .map(|serialized| (serialized.len() / 4).max(1))
            .unwrap_or(1)
    }
    while estimate(packet) > token_budget {
        if packet.snippets.len() > 1 {
            packet.snippets.pop();
            continue;
        }
        if packet.verified_paths.len() > 1 {
            packet.verified_paths.pop();
            continue;
        }
        if packet.recommended_tests.len() > 1 {
            packet.recommended_tests.pop();
            continue;
        }
        if packet.risks.len() > 1 {
            packet.risks.pop();
            continue;
        }
        break;
    }
    packet
        .metadata
        .insert("estimated_tokens".to_string(), json!(estimate(packet)));
}

#[test]
fn incremental_compaction_matches_full_reserialize_across_budgets() {
    // F3: the incremental byte-length tracking in compact_context_packet_for_cli
    // must terminate at exactly the same point as the old O(N^2) full-reserialize
    // loop. Sweep budgets from "drop nothing" down through every drop boundary to
    // "drop everything poppable", asserting the optimized result is byte-identical
    // to the reference for each budget — including the recorded estimated_tokens.
    // A small packet keeps the exhaustive every-integer budget sweep below
    // cheap while still populating all four poppable arrays with >1 element.
    let mut metadata = Metadata::new();
    metadata.insert("seed".to_string(), json!("prod_value"));
    let template = ContextPacket {
        task: "Trace prod_value".to_string(),
        mode: "trace".to_string(),
        symbols: vec!["prod_value".to_string()],
        verified_paths: (0u32..3)
            .map(|index| context_agent_test_path(&format!("prod-path-{index}"), index + 3, "prod"))
            .collect(),
        risks: (0..3).map(|index| format!("risk {index}")).collect(),
        recommended_tests: (0..3).map(|index| format!("cargo test t{index}")).collect(),
        snippets: (0..3)
            .map(|index| ContextSnippet {
                file: "src/lib.rs".to_string(),
                lines: format!("{}", index + 3),
                text: format!("fn s{index}() {{}}"),
                reason: "span".to_string(),
            })
            .collect(),
        metadata,
    };
    let full_tokens = {
        let mut probe = template.clone();
        compact_context_packet_for_cli(&mut probe, usize::MAX);
        probe
            .metadata
            .get("estimated_tokens")
            .and_then(|value| value.as_u64())
            .expect("estimated_tokens present") as usize
    };

    // Cover the whole range, plus a hard floor that forces every guarded pop.
    let mut budgets: Vec<usize> = (1..=full_tokens + 2).collect();
    budgets.push(0);
    for token_budget in budgets {
        let mut optimized = template.clone();
        let mut reference = template.clone();
        compact_context_packet_for_cli(&mut optimized, token_budget);
        reference_compact_context_packet(&mut reference, token_budget);
        assert_eq!(
            optimized, reference,
            "incremental compaction diverged from full re-serialize at budget {token_budget}"
        );
    }
}

#[test]
fn top_level_help_lists_required_commands() {
    let output = run([BIN_NAME, "--help"]);

    assert_eq!(output.exit_code, 0);
    for command in [
        "init",
        "index",
        "status",
        "query",
        "impact",
        "context-pack",
        "context",
        "bundle",
        "watch",
        "serve-mcp",
        "mcp",
        "serve-ui",
        "ui",
        "bench",
        "doctor",
        "languages",
        "config",
    ] {
        assert!(output.stdout.contains(command), "missing {command}");
    }
}

#[test]
fn path_identity_accepts_raw_and_canonical_spellings() {
    let repo = temp_repo();
    let canonical = fs::canonicalize(&repo).expect("canonical repo");

    assert!(
        super::paths_equivalent_string(&path_string(&repo), &path_string(&canonical)),
        "raw repo path and canonical repo path should identify the same directory"
    );
}

#[test]
fn candidate_spool_query_returns_candidate_only_results() {
    let repo = temp_repo();
    let spool = write_candidate_spool_cli_fixture(&repo);
    let args = vec![
        "symbols".to_string(),
        "spool_target".to_string(),
        "--agent-json".to_string(),
    ];
    let value =
        super::run_candidate_spool_query_command(&repo, &args, &spool, false).expect("query");
    assert_eq!(value["status"].as_str(), Some("ok"));
    assert_eq!(value["candidate_only"].as_bool(), Some(true));
    assert_eq!(value["graph_proof"].as_bool(), Some(false));
    assert_eq!(value["proof_strength"].as_str(), Some("symbol_evidence"));
    assert_eq!(
        value["candidate_spool_status"].as_str(),
        Some("partial_ready")
    );
    assert_eq!(value["candidate_context_truncated"].as_bool(), Some(false));
    assert_eq!(value["candidate_spool_unavailable"].as_bool(), Some(false));
    assert_eq!(value["graph_db_status"].as_str(), Some("building"));
    assert_eq!(value["graph_proof_available"].as_bool(), Some(false));
    assert_eq!(value["candidate_only_available"].as_bool(), Some(true));
    assert_eq!(
        value["candidate_spool"]["query_index_status"].as_str(),
        Some("ready")
    );
    assert_eq!(
        value["candidate_spool"]["query_index_kind"].as_str(),
        Some("sqlite")
    );
    let result = &value["results"][0];
    assert_eq!(result["proof_status"].as_str(), Some("candidate_only"));
    assert_eq!(result["graph_proof"].as_bool(), Some(false));
    assert_eq!(
        result["graph_verification_status"].as_str(),
        Some("needs_graph_verification")
    );
}

#[test]
fn agent_file_json_exposes_degraded_graph_output_claimability() {
    let mut metadata = Metadata::default();
    metadata.insert("graph_output_budget_hit".to_string(), json!(true));
    metadata.insert(
        "degradation_labels".to_string(),
        json!([
            "generated_large",
            "extraction_budget_hit",
            "diagnostic_only"
        ]),
    );
    metadata.insert(
        "graph_output_claimability".to_string(),
        json!("degraded_file_nonclaimable_for_omitted_facts"),
    );
    metadata.insert("graph_relation_claims".to_string(), json!("partial"));
    metadata.insert("diagnostic_only".to_string(), json!(true));
    metadata.insert(
        "graph_extraction_skip_reason".to_string(),
        json!("large_generated_or_test_source_budget"),
    );
    metadata.insert(
        "graph_output_budget_hits".to_string(),
        json!([{
            "repo_relative_path": "src/generated/big.generated.ts",
            "stage": "local_extraction",
            "kind": "source_bytes_per_file",
            "before": 256,
            "after": 0,
            "budget": 32,
            "omitted": 256,
            "unit": "bytes",
            "labels": ["extraction_budget_hit", "diagnostic_only"],
            "claimability_label": "degraded_file_nonclaimable_for_omitted_facts"
        }]),
    );
    let file = FileRecord {
        repo_relative_path: "src/generated/big.generated.ts".to_string(),
        file_hash: "hash".to_string(),
        language: Some("typescript".to_string()),
        size_bytes: 256,
        indexed_at_unix_ms: None,
        metadata,
    };
    let mut hit = json!({
        "repo_relative_path": "src/generated/big.generated.ts",
        "score": 1.0,
        "match": "path_contains",
        "text": "export const generatedValue = 1;"
    });

    super::insert_file_degradation_labels(&file, &mut hit);
    assert_eq!(hit["graph_output_degraded"].as_bool(), Some(true));
    assert_eq!(hit["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(
        hit["graph_extraction_skip_reason"].as_str(),
        Some("large_generated_or_test_source_budget")
    );
    assert_eq!(
        hit["claimability"]["not_claimable_as"][0].as_str(),
        Some("complete_graph_for_degraded_file")
    );

    let agent = super::agent_file_hit_json(&hit);
    assert_eq!(agent["graph_output_degraded"].as_bool(), Some(true));
    assert_eq!(
        agent["graph_output_claimability"].as_str(),
        Some("degraded_file_nonclaimable_for_omitted_facts")
    );
    assert_eq!(
        agent["graph_extraction_skip_reason"].as_str(),
        Some("large_generated_or_test_source_budget")
    );
    assert_eq!(
        agent["claimability"]["not_claimable_as"][1].as_str(),
        Some("omitted_relation_classes")
    );
}

#[test]
fn context_candidate_json_exposes_degraded_graph_output_claimability() {
    let mut metadata = Metadata::default();
    metadata.insert("graph_output_budget_hit".to_string(), json!(true));
    metadata.insert(
        "degradation_labels".to_string(),
        json!([
            "generated_large",
            "extraction_budget_hit",
            "diagnostic_only"
        ]),
    );
    metadata.insert(
        "graph_output_claimability".to_string(),
        json!("degraded_file_nonclaimable_for_omitted_facts"),
    );
    metadata.insert("graph_relation_claims".to_string(), json!([]));
    metadata.insert("diagnostic_only".to_string(), json!(true));
    metadata.insert(
        "graph_extraction_skip_reason".to_string(),
        json!("large_generated_or_test_source_budget"),
    );
    metadata.insert(
        "graph_output_budget_hits".to_string(),
        json!([{
            "repo_relative_path": "src/generated/large.generated.ts",
            "stage": "local_extraction",
            "kind": "source_bytes_per_file",
            "before": 256,
            "after": 0,
            "budget": 32,
            "omitted": 256,
            "unit": "bytes",
            "labels": ["extraction_budget_hit", "diagnostic_only"],
            "claimability_label": "degraded_file_nonclaimable_for_omitted_facts"
        }]),
    );
    let file = FileRecord {
        repo_relative_path: "src/generated/large.generated.ts".to_string(),
        file_hash: "hash".to_string(),
        language: Some("typescript".to_string()),
        size_bytes: 256,
        indexed_at_unix_ms: None,
        metadata,
    };
    let mut candidate = super::exact_seed_retrieval_candidate_json(
        "src/generated/large.generated.ts",
        true,
        "no_proof_path_found",
    );

    super::insert_context_candidate_degradation_from_file(&file, &mut candidate);

    assert_eq!(candidate["graph_output_degraded"].as_bool(), Some(true));
    assert_eq!(
        candidate["graph_extraction_skip_reason"].as_str(),
        Some("large_generated_or_test_source_budget")
    );
    assert_eq!(
        candidate["claimability"]["not_claimable_as"][0].as_str(),
        Some("complete_graph_for_degraded_file")
    );
    assert_eq!(
        candidate["degraded_output_not_complete_graph_proof"].as_bool(),
        Some(true)
    );

    let compact = super::context_pack_compact_candidate_json(&candidate);
    assert_eq!(compact["graph_output_degraded"].as_bool(), Some(true));
    assert_eq!(
        compact["degradation_labels"][0].as_str(),
        Some("generated_large")
    );
    assert_eq!(
        compact["claimability"]["not_claimable_as"][2].as_str(),
        Some("graph_relation_proof")
    );
}

#[test]
fn candidate_spool_context_pack_returns_no_graph_proof() {
    let repo = temp_repo();
    let spool = write_candidate_spool_cli_fixture(&repo);
    let options = super::parse_context_pack_args(&[
        "--task".to_string(),
        "spool_target".to_string(),
        "--candidate-spool".to_string(),
        path_string(&spool),
        "--agent-json".to_string(),
    ])
    .expect("parse context-pack candidate spool args");
    let value = super::run_candidate_spool_context_pack_command(
        &repo,
        &spool,
        &options,
        false,
        Instant::now(),
        Some("db missing".to_string()),
    )
    .expect("candidate spool context pack");
    assert_eq!(value["status"].as_str(), Some("ok"));
    assert_eq!(value["proof_status"].as_str(), Some("candidate_only"));
    assert_eq!(value["proof_strength"].as_str(), Some("candidate_evidence"));
    assert_eq!(value["graph_proof"].as_bool(), Some(false));
    assert_eq!(
        value["candidate_spool_status"].as_str(),
        Some("partial_ready")
    );
    assert_eq!(value["graph_db_status"].as_str(), Some("building"));
    assert_eq!(value["graph_proof_available"].as_bool(), Some(false));
    assert_eq!(
        value["staged_availability"]["recommended_next_step"].as_str(),
        Some("inspect candidate spans")
    );
    assert_eq!(
        value["routing_packet"]["available_layers"][0].as_str(),
        Some("candidate_spool")
    );
    assert_eq!(
        value["graph_verification"]["status"].as_str(),
        Some("no_graph_db")
    );
    assert!(value["snippets"].as_array().map(Vec::len).unwrap_or(0) <= 5);
    assert_eq!(
        value["patch_assist_packet"]["first_use_state"].as_str(),
        Some("candidate_ready_no_graph")
    );
    assert_eq!(
        value["patch_assist_packet"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert!(!value["patch_assist_packet"]["candidate_evidence"]
        .as_array()
        .expect("candidate evidence")
        .is_empty());
}

#[test]
fn candidate_spool_status_reports_missing_index_without_full_scan() {
    let repo = temp_repo();
    let spool = write_candidate_spool_cli_fixture(&repo);
    let status = super::candidate_spool_layer_status(&repo, &spool);
    assert_eq!(status["status"].as_str(), Some("query_index_missing"));
    assert_eq!(status["ready"].as_bool(), Some(false));
    assert_eq!(status["query_index_status"].as_str(), Some("index_missing"));
    assert_eq!(status["candidate_spool_unavailable"].as_bool(), Some(true));
}

#[test]
fn candidate_spool_corrupt_query_index_is_structured_status() {
    let repo = temp_repo();
    let spool = write_candidate_spool_cli_fixture(&repo);
    let index_path = super::candidate_spool_query_index_path(&spool);
    let _ = fs::remove_file(&index_path);
    Connection::open(&index_path).expect("create empty corrupt query index");
    let status = super::candidate_spool_layer_status(&repo, &spool);
    assert_eq!(status["status"].as_str(), Some("query_index_corrupt"));
    assert_eq!(status["ready"].as_bool(), Some(false));
    assert_eq!(status["candidate_spool_unavailable"].as_bool(), Some(true));
    assert!(status["reason"]
        .as_str()
        .unwrap_or("")
        .contains("candidate_spool_query_index"));
}

#[test]
fn candidate_spool_query_index_access_error_is_not_labeled_corrupt() {
    let repo = temp_repo();
    let spool = repo.join("codegraph-candidate-spool.jsonl");
    let load = super::CandidateSpoolIndexLoad {
            path: spool.clone(),
            query_index_path: super::candidate_spool_query_index_path(&spool),
            metadata: json!({
                "candidate_spool_status": "bounded_ready",
                "candidate_spool_truncated": false,
                "incomplete": false,
            }),
            stale: true,
            reason: Some("candidate_spool_query_index_open_failed: filesystem_inaccessible: unable to open database file".to_string()),
            query_index_status: "filesystem_inaccessible".to_string(),
            query_index_kind: "sqlite".to_string(),
            query_index_bytes: 0,
            query_index_record_count: 0,
            query_index_version: "candidate_spool_query_index_v1".to_string(),
            query_index_bound_manifest_hash: None,
            query_index_source_binding_count: 0,
        };
    let status = super::candidate_spool_layer_from_index_load(&spool, &load);
    assert_eq!(status["status"].as_str(), Some("filesystem_inaccessible"));
    assert_eq!(
        status["query_index_problem_kind"].as_str(),
        Some("filesystem_inaccessible")
    );
    assert_eq!(status["ready"].as_bool(), Some(false));
    assert_eq!(status["candidate_spool_unavailable"].as_bool(), Some(true));

    remove_dir_all_with_retry(&repo, "cleanup repo");
}

#[test]
fn candidate_spool_indexed_status_and_query_reject_changed_and_deleted_files() {
    let repo = temp_repo();
    let spool = write_candidate_spool_cli_fixture(&repo);
    let args = vec![
        "symbols".to_string(),
        "spool_target".to_string(),
        "--agent-json".to_string(),
    ];
    let first =
        super::run_candidate_spool_query_command(&repo, &args, &spool, false).expect("query");
    assert_eq!(first["result_count"].as_u64(), Some(1));

    write_cli_fixture_file(&repo, "src/lib.rs", "pub fn changed_spool_target() {}\n");
    let status = super::candidate_spool_layer_status(&repo, &spool);
    assert_eq!(status["status"].as_str(), Some("stale"));
    assert_eq!(status["ready"].as_bool(), Some(false));
    assert_eq!(status["query_index_status"].as_str(), Some("stale"));
    assert_eq!(status["candidate_spool_unavailable"].as_bool(), Some(true));
    assert!(status["reason"]
        .as_str()
        .unwrap_or("")
        .contains("changed_file"));
    let error = super::run_candidate_spool_query_command(&repo, &args, &spool, false)
        .expect_err("changed source must reject normal spool query");
    assert!(error.contains("candidate_spool_stale"), "{error}");
    let diagnostic = super::run_candidate_spool_query_command(&repo, &args, &spool, true)
        .expect("diagnostic stale query");
    assert_eq!(diagnostic["candidate_spool_status"].as_str(), Some("stale"));
    assert_eq!(diagnostic["graph_proof"].as_bool(), Some(false));

    fs::remove_file(repo.join("src").join("lib.rs")).expect("delete source");
    let delete_status = super::candidate_spool_layer_status(&repo, &spool);
    assert_eq!(delete_status["status"].as_str(), Some("stale"));
    assert!(delete_status["reason"]
        .as_str()
        .unwrap_or("")
        .contains("deleted_file"));
}

fn mvp3_7_sidecar_graph_ready_layer() -> Value {
    json!({
        "layer": "graph_db",
        "status": "ready",
        "ready": true,
        "graph_proof_available": true,
        "claimable": true,
        "diagnostic_only": false,
        "path": "fixture.sqlite",
    })
}

fn mvp3_7_sidecar_missing_layer(layer: &str) -> Value {
    json!({
        "layer": layer,
        "status": "missing",
        "ready": false,
        "candidate_only": true,
        "graph_proof": false,
        "diagnostic_only": layer == "vector_audit",
        "runtime_dependency": false,
    })
}

fn mvp3_7_sidecar_candidate_layer(status: &str) -> Value {
    json!({
        "layer": "candidate_spool",
        "status": status,
        "ready": false,
        "path": "candidate-spool.jsonl",
        "query_index_status": status,
        "query_index_path": "candidate-spool.jsonl.query.sqlite",
        "candidate_only": true,
        "graph_proof": false,
        "candidate_spool_unavailable": true,
        "reason": "mvp3.7 fixture sidecar state",
    })
}

#[test]
fn candidate_spool_invalidated_or_refreshed() {
    let action = super::agent_use_layer_delta_action(Some("stale"));
    assert_eq!(action["action"].as_str(), Some("invalidated"));
    assert_eq!(action["status"].as_str(), Some("stale"));
    assert_eq!(action["graph_proof"].as_bool(), Some(false));
}

#[test]
fn candidate_query_index_invalidated_or_refreshed() {
    let stale = super::agent_use_layer_delta_action(Some("stale"));
    let corrupt = super::agent_use_layer_delta_action(Some("query_index_corrupt"));
    assert_eq!(stale["action"].as_str(), Some("invalidated"));
    assert_eq!(corrupt["action"].as_str(), Some("error"));
    assert_eq!(corrupt["graph_proof"].as_bool(), Some(false));
}

#[test]
fn stale_candidate_hits_not_fresh() {
    let repo = temp_repo();
    let spool = write_candidate_spool_cli_fixture(&repo);
    let args = vec![
        "symbols".to_string(),
        "spool_target".to_string(),
        "--agent-json".to_string(),
    ];
    write_cli_fixture_file(&repo, "src/lib.rs", "pub fn changed_spool_target() {}\n");

    let error = super::run_candidate_spool_query_command(&repo, &args, &spool, false)
        .expect_err("normal query must reject stale candidate hits");
    assert!(error.contains("candidate_spool_stale"), "{error}");
    let diagnostic = super::run_candidate_spool_query_command(&repo, &args, &spool, true)
        .expect("diagnostic stale query");
    assert_eq!(diagnostic["candidate_spool_status"].as_str(), Some("stale"));
    assert_eq!(diagnostic["graph_proof"].as_bool(), Some(false));
    assert_no_dot_codegraph_sqlite(&repo);
    remove_dir_all_with_retry(&repo, "cleanup stale candidate repo");
}

#[test]
fn deleted_file_candidate_rows_not_fresh() {
    let repo = temp_repo();
    let spool = write_candidate_spool_cli_fixture(&repo);
    let args = vec![
        "symbols".to_string(),
        "spool_target".to_string(),
        "--agent-json".to_string(),
    ];
    super::run_candidate_spool_query_command(&repo, &args, &spool, false)
        .expect("warm query index before delete");
    fs::remove_file(repo.join("src").join("lib.rs")).expect("delete source");

    let status = super::candidate_spool_layer_status(&repo, &spool);
    assert_eq!(status["status"].as_str(), Some("stale"));
    assert_eq!(status["ready"].as_bool(), Some(false));
    assert_eq!(status["candidate_spool_unavailable"].as_bool(), Some(true));
    assert!(status["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("deleted_file"));
    assert_no_dot_codegraph_sqlite(&repo);
    remove_dir_all_with_retry(&repo, "cleanup deleted candidate repo");
}

#[test]
fn candidate_query_index_corrupt_non_graph_db_corruption() {
    let repo = temp_repo();
    let spool = write_candidate_spool_cli_fixture(&repo);
    let index_path = super::candidate_spool_query_index_path(&spool);
    let _ = fs::remove_file(&index_path);
    fs::write(&index_path, "not sqlite").expect("write corrupt sidecar");

    let status = super::candidate_spool_layer_status(&repo, &spool);
    assert_eq!(status["status"].as_str(), Some("query_index_corrupt"));
    assert_eq!(status["graph_proof"].as_bool(), Some(false));
    assert_eq!(status["candidate_spool_unavailable"].as_bool(), Some(true));

    let staged = super::staged_availability_from_layers(
        mvp3_7_sidecar_graph_ready_layer(),
        status,
        mvp3_7_sidecar_missing_layer("vector_runtime"),
        mvp3_7_sidecar_missing_layer("vector_audit"),
    );
    assert_eq!(staged["graph_proof_available"].as_bool(), Some(true));
    assert_eq!(staged["claimability"]["claimable"].as_bool(), Some(true));
    assert!(!staged["active_candidate_sources"]
        .as_array()
        .expect("active sources")
        .iter()
        .any(|source| source.as_str() == Some("candidate_spool")));
    assert_no_dot_codegraph_sqlite(&repo);
    remove_dir_all_with_retry(&repo, "cleanup corrupt query index repo");
}

#[test]
fn candidate_query_index_inaccessible_not_corrupt() {
    let repo = temp_repo();
    let spool = repo.join("codegraph-candidate-spool.jsonl");
    let load = super::CandidateSpoolIndexLoad {
        path: spool.clone(),
        query_index_path: super::candidate_spool_query_index_path(&spool),
        metadata: json!({
            "candidate_spool_status": "bounded_ready",
            "candidate_spool_truncated": false,
            "incomplete": false,
        }),
        stale: true,
        reason: Some(
            "candidate_spool_query_index_open_failed: filesystem_inaccessible: unable to open database file"
                .to_string(),
        ),
        query_index_status: "filesystem_inaccessible".to_string(),
        query_index_kind: "sqlite".to_string(),
        query_index_bytes: 0,
        query_index_record_count: 0,
        query_index_version: "candidate_spool_query_index_v1".to_string(),
        query_index_bound_manifest_hash: None,
        query_index_source_binding_count: 0,
    };
    let status = super::candidate_spool_layer_from_index_load(&spool, &load);
    assert_eq!(status["status"].as_str(), Some("filesystem_inaccessible"));
    assert_eq!(
        status["query_index_problem_kind"].as_str(),
        Some("filesystem_inaccessible")
    );
    assert_ne!(status["status"].as_str(), Some("query_index_corrupt"));
    assert_eq!(status["graph_proof"].as_bool(), Some(false));
    remove_dir_all_with_retry(&repo, "cleanup inaccessible query index repo");
}

#[test]
fn optional_candidate_spool_failure_does_not_fail_claimable_graph() {
    let staged = super::staged_availability_from_layers(
        mvp3_7_sidecar_graph_ready_layer(),
        mvp3_7_sidecar_candidate_layer("disabled_budget_exceeded"),
        mvp3_7_sidecar_missing_layer("vector_runtime"),
        mvp3_7_sidecar_missing_layer("vector_audit"),
    );
    assert_eq!(staged["graph_proof_available"].as_bool(), Some(true));
    assert_eq!(staged["claimability"]["claimable"].as_bool(), Some(true));
    assert_eq!(
        staged["candidate_spool_status"].as_str(),
        Some("disabled_budget_exceeded")
    );
    assert!(!staged["active_candidate_sources"]
        .as_array()
        .expect("active sources")
        .iter()
        .any(|source| source.as_str() == Some("candidate_spool")));
}

#[test]
fn required_candidate_spool_failure_structured() {
    let failure = json!({
        "status": "index_failed",
        "error": "candidate_spool_required_budget_exceeded",
        "candidate_spool_required": true,
        "candidate_spool_disabled_reason": "candidate_spool_budget_too_small",
        "candidate_only": true,
        "graph_proof": false,
    });
    assert_eq!(
        failure["error"].as_str(),
        Some("candidate_spool_required_budget_exceeded")
    );
    assert_eq!(failure["candidate_spool_required"].as_bool(), Some(true));
    assert_eq!(failure["graph_proof"].as_bool(), Some(false));
}

#[test]
fn vector_runtime_invalidated_or_refreshed() {
    let action = super::agent_use_layer_delta_action(Some("stale"));
    assert_eq!(action["action"].as_str(), Some("invalidated"));
    assert_eq!(action["graph_proof"].as_bool(), Some(false));
}

#[test]
fn vector_audit_diagnostic_only() {
    let repo = temp_repo();
    let audit_path = repo.join("vector-audit.json");
    fs::write(
        &audit_path,
        serde_json::to_string(&json!({
            "metadata": {
                "artifact_kind": "audit_artifact",
                "index_artifact_format": "pretty_json"
            }
        }))
        .expect("audit json"),
    )
    .expect("write audit");
    let status = super::vector_audit_layer_status(&repo.join("missing.sqlite"), None, &audit_path);
    assert_eq!(status["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(status["runtime_dependency"].as_bool(), Some(false));
    assert_ne!(status["graph_proof"].as_bool(), Some(true));
    assert_no_dot_codegraph_sqlite(&repo);
    remove_dir_all_with_retry(&repo, "cleanup vector audit repo");
}

#[test]
fn stale_vector_chunks_not_fresh() {
    let staged = super::staged_availability_from_layers(
        mvp3_7_sidecar_graph_ready_layer(),
        mvp3_7_sidecar_candidate_layer("no_spool"),
        json!({
            "layer": "vector_runtime",
            "status": "stale",
            "ready": false,
            "candidate_only": true,
            "graph_proof": false,
            "reason": "vector_source_binding_stale",
        }),
        mvp3_7_sidecar_missing_layer("vector_audit"),
    );
    assert_eq!(staged["graph_proof_available"].as_bool(), Some(true));
    assert!(!staged["active_candidate_sources"]
        .as_array()
        .expect("active sources")
        .iter()
        .any(|source| source.as_str() == Some("vector_semantic")));
}

#[test]
fn nuance_records_invalidated_or_not_applicable() {
    let action = super::agent_use_not_applicable_delta_action(
        "nuance_rescue_candidates_are_request_time_context_candidates",
    );
    assert_eq!(action["status"].as_str(), Some("not_applicable"));
    assert_eq!(action["graph_proof"].as_bool(), Some(false));
}

#[test]
fn source_navigation_handles_invalidated_or_not_applicable() {
    assert!(super::agent_use_candidate_layer_status_is_stale("stale"));
    let action = super::agent_use_not_applicable_delta_action(
        "source_navigation_handles_are_candidate_inspection_aids",
    );
    assert_eq!(action["status"].as_str(), Some("not_applicable"));
    assert_eq!(action["graph_proof"].as_bool(), Some(false));
}

#[test]
fn routing_handles_invalidated_or_not_applicable() {
    let action = json!({
        "action": "dirty_file_cleanup",
        "scope": "sparse_sidecar_handles",
        "graph_proof": false,
    });
    assert_eq!(action["action"].as_str(), Some("dirty_file_cleanup"));
    assert_eq!(action["graph_proof"].as_bool(), Some(false));
}

#[test]
fn stale_sidecars_not_used_as_fresh() {
    for status in [
        "stale",
        "query_index_corrupt",
        "sidecar_corrupt",
        "permission_denied",
        "read_only",
        "filesystem_inaccessible",
        "sidecar_unavailable",
        "sidecar_locked",
        "blocked_by_graph_db",
        "disabled_budget_exceeded",
    ] {
        assert!(
            super::agent_use_candidate_layer_status_is_stale(status),
            "{status}"
        );
        let action = super::agent_use_layer_delta_action(Some(status));
        assert_ne!(
            action["action"].as_str(),
            Some("status_checked"),
            "{status}"
        );
        assert_eq!(action["graph_proof"].as_bool(), Some(false));
    }
}

#[test]
fn corrupt_sidecar_not_graph_db_corruption() {
    let staged = super::staged_availability_from_layers(
        mvp3_7_sidecar_graph_ready_layer(),
        mvp3_7_sidecar_candidate_layer("sidecar_corrupt"),
        mvp3_7_sidecar_missing_layer("vector_runtime"),
        mvp3_7_sidecar_missing_layer("vector_audit"),
    );
    assert_eq!(staged["graph_db_status"].as_str(), Some("ready"));
    assert_eq!(
        staged["candidate_spool_status"].as_str(),
        Some("sidecar_corrupt")
    );
    assert_eq!(staged["graph_proof_available"].as_bool(), Some(true));
}

#[test]
fn graph_claimable_when_optional_sidecar_corrupt() {
    let staged = super::staged_availability_from_layers(
        mvp3_7_sidecar_graph_ready_layer(),
        mvp3_7_sidecar_candidate_layer("sidecar_corrupt"),
        json!({
            "layer": "vector_runtime",
            "status": "sidecar_corrupt",
            "ready": false,
            "candidate_only": true,
            "graph_proof": false,
            "reason": "runtime sidecar corrupt",
        }),
        json!({
            "layer": "vector_audit",
            "status": "sidecar_corrupt",
            "ready": false,
            "diagnostic_only": true,
            "runtime_dependency": false,
            "graph_proof": false,
            "reason": "audit sidecar corrupt",
        }),
    );
    assert_eq!(staged["graph_proof_available"].as_bool(), Some(true));
    assert_eq!(staged["claimability"]["claimable"].as_bool(), Some(true));
    assert_eq!(staged["candidate_only_available"].as_bool(), Some(false));
}

#[test]
fn no_dot_codegraph_mutation_for_sidecar_status_helpers() {
    let repo = temp_repo();
    let _ = super::staged_availability_from_layers(
        mvp3_7_sidecar_graph_ready_layer(),
        mvp3_7_sidecar_candidate_layer("sidecar_corrupt"),
        mvp3_7_sidecar_missing_layer("vector_runtime"),
        mvp3_7_sidecar_missing_layer("vector_audit"),
    );
    assert_no_dot_codegraph_sqlite(&repo);
    remove_dir_all_with_retry(&repo, "cleanup sidecar helper repo");
}

#[test]
fn candidate_spool_legacy_firehose_without_index_is_not_hot_path() {
    let repo = temp_repo();
    let spool = repo.join("legacy-firehose-spool.jsonl");
    let manifest = json!({
        "metadata": {
            "metadata_version": "candidate_spool_v1",
            "artifact_kind": "candidate_spool",
            "artifact_format": "jsonl",
            "repo_root": path_string(&repo),
            "candidate_spool_status": "partial_ready",
            "spooled_total_chunks": 217514,
            "persisted_total_chunks": 217514,
            "candidate_only": true,
            "graph_proof": false
        }
    });
    let chunk = json!({
        "chunk_id": "candidate-spool:legacy:one",
        "chunk_kind": "signature",
        "source_kind": "graph_entity",
        "path": "src/lib.rs",
        "text": "function legacy_firehose_target",
        "proof_status": "candidate_only",
        "graph_proof": false,
        "claimable_for_graph": false
    });
    fs::write(
        &spool,
        format!(
            "{}\n{}\n",
            serde_json::to_string(&manifest).expect("manifest"),
            serde_json::to_string(&chunk).expect("chunk")
        ),
    )
    .expect("write legacy spool");
    let args = vec![
        "symbols".to_string(),
        "legacy_firehose_target".to_string(),
        "--agent-json".to_string(),
    ];
    let error = super::run_candidate_spool_query_command(&repo, &args, &spool, false)
        .expect_err("legacy firehose without index should not be scanned");
    assert!(
        error.contains("legacy or oversized candidate spool"),
        "{error}"
    );
}

#[test]
fn candidate_spool_loader_rejects_stale_and_allows_diagnostic_stale() {
    let repo = temp_repo();
    let spool = write_candidate_spool_cli_fixture(&repo);
    write_cli_fixture_file(&repo, "src/lib.rs", "pub fn changed() {}\n");
    let error = super::load_candidate_spool_for_repo(&repo, &spool, false)
        .expect_err("stale spool should be rejected by default");
    assert!(error.contains("candidate_spool_stale"), "{error}");
    let diagnostic =
        super::load_candidate_spool_for_repo(&repo, &spool, true).expect("diagnostic stale load");
    assert!(diagnostic.stale);
}

#[test]
fn candidate_spool_loader_reports_corrupt_artifact_without_panic() {
    let repo = temp_repo();
    let spool = repo.join("candidate-spool.jsonl");
    fs::write(&spool, "{not-json}\n").expect("write corrupt spool");
    let error = super::load_candidate_spool_for_repo(&repo, &spool, false)
        .expect_err("corrupt spool should error");
    assert!(error.contains("candidate_spool_corrupt"), "{error}");
}

#[test]
fn candidate_spool_repo_binding_accepts_windows_extended_path_prefix() {
    assert!(super::paths_equivalent_string(
        r"\\?\C:\repo\codegraph",
        r"C:\repo\codegraph"
    ));
}

#[cfg(windows)]
#[test]
fn candidate_spool_repo_binding_accepts_windows_short_home_alias() {
    assert!(super::paths_equivalent_string(
        r"\\?\C:\Users\runneradmin\AppData\Local\Temp\codegraph-cli-unit-1",
        r"C:\Users\RUNNER~1\AppData\Local\Temp\codegraph-cli-unit-1"
    ));
    assert!(!super::paths_equivalent_string(
        r"\\?\C:\Users\runneradmin\AppData\Local\Temp\codegraph-cli-unit-1",
        r"C:\Users\RUNNER~1\AppData\Local\Temp\codegraph-cli-unit-2"
    ));
}

#[test]
fn command_help_is_successful() {
    let output = run([BIN_NAME, "context-pack", "--help"]);

    assert_eq!(output.exit_code, 0);
    assert!(output.stdout.contains("Usage:"));
    assert!(output.stdout.contains("context-pack"));
}

#[test]
fn bundle_import_fresh_succeeds_and_non_empty_default_fails() {
    let _guard = lock_bundle_test();
    let repo = bundle_fixture_repo("bundle_fresh_symbol");
    let source_db = repo.join("source.sqlite");
    let target_db = repo.join("target.sqlite");
    let bundle_path = repo.join("fresh.cgc-bundle");
    index_repo_to_db_with_options(&repo, &source_db, IndexOptions::default())
        .expect("index source bundle DB");

    super::with_repo_db_context(&repo, &source_db, || {
        super::run_bundle_export(&["--output".to_string(), path_string(&bundle_path)])
    })
    .expect("export bundle");

    let imported = super::with_repo_db_context(&repo, &target_db, || {
        super::run_bundle_import(&[path_string(&bundle_path)])
    })
    .expect("fresh import");
    assert_eq!(imported["status"].as_str(), Some("imported"));
    assert_eq!(imported["claimable"].as_bool(), Some(true));
    assert!(db_has_symbol(&target_db, "bundle_fresh_symbol"));

    let error = super::with_repo_db_context(&repo, &target_db, || {
        super::run_bundle_import(&[path_string(&bundle_path)])
    })
    .expect_err("non-empty import without replace must fail");
    assert!(error.contains("without --replace"), "{error}");

    remove_dir_all_with_retry(&repo, "cleanup repo");
}

#[test]
fn bundle_import_rejects_foreign_corrupt_schema_and_repo_head_mismatch() {
    let _guard = lock_bundle_test();
    let repo_a = bundle_fixture_repo("bundle_origin_symbol");
    let repo_b = bundle_fixture_repo("bundle_foreign_target");
    let source_db = repo_a.join("source.sqlite");
    let target_db = repo_b.join("target.sqlite");
    let bundle_path = repo_a.join("origin.cgc-bundle");
    index_repo_to_db_with_options(&repo_a, &source_db, IndexOptions::default())
        .expect("index origin DB");
    super::with_repo_db_context(&repo_a, &source_db, || {
        super::run_bundle_export(&["--output".to_string(), path_string(&bundle_path)])
    })
    .expect("export origin bundle");

    let foreign_error = super::with_repo_db_context(&repo_b, &target_db, || {
        super::run_bundle_import(&[path_string(&bundle_path)])
    })
    .expect_err("foreign bundle must fail");
    assert!(
        foreign_error.contains("bundle repo identity mismatch"),
        "{foreign_error}"
    );

    let corrupt_path = repo_a.join("corrupt.cgc-bundle");
    fs::write(&corrupt_path, "{not-json").expect("write corrupt bundle");
    let corrupt_error =
        super::with_repo_db_context(&repo_a, &repo_a.join("corrupt.sqlite"), || {
            super::run_bundle_import(&[path_string(&corrupt_path)])
        })
        .expect_err("corrupt bundle must fail");
    assert!(!corrupt_error.is_empty());

    let mut schema_bundle: Value =
        serde_json::from_str(&fs::read_to_string(&bundle_path).expect("read bundle"))
            .expect("parse bundle");
    schema_bundle["manifest"]["schema_version"] = json!(999_u32);
    let schema_path = repo_a.join("schema-mismatch.cgc-bundle");
    fs::write(
        &schema_path,
        serde_json::to_string_pretty(&schema_bundle).expect("encode schema bundle"),
    )
    .expect("write schema bundle");
    let schema_error = super::with_repo_db_context(&repo_a, &repo_a.join("schema.sqlite"), || {
        super::run_bundle_import(&[path_string(&schema_path)])
    })
    .expect_err("schema mismatch bundle must fail");
    assert!(
        schema_error.contains("bundle schema mismatch"),
        "{schema_error}"
    );

    let git_repo = bundle_fixture_repo("bundle_head_symbol");
    let current_head = init_git_repo_for_bundle_test(&git_repo);
    let git_source_db = git_repo.join("source.sqlite");
    let git_bundle_path = git_repo.join("head.cgc-bundle");
    index_repo_to_db_with_options(&git_repo, &git_source_db, IndexOptions::default())
        .expect("index git bundle DB");
    super::with_repo_db_context(&git_repo, &git_source_db, || {
        super::run_bundle_export(&["--output".to_string(), path_string(&git_bundle_path)])
    })
    .expect("export git bundle");
    let mut head_bundle: Value =
        serde_json::from_str(&fs::read_to_string(&git_bundle_path).expect("read git bundle"))
            .expect("parse git bundle");
    assert_eq!(
        head_bundle["manifest"]["repo_head"].as_str(),
        Some(current_head.as_str())
    );
    head_bundle["manifest"]["repo_head"] = json!("stale-head-for-test");
    let stale_head_path = git_repo.join("head-stale.cgc-bundle");
    fs::write(
        &stale_head_path,
        serde_json::to_string_pretty(&head_bundle).expect("encode stale head bundle"),
    )
    .expect("write stale head bundle");
    let head_error =
        super::with_repo_db_context(&git_repo, &git_repo.join("head-target.sqlite"), || {
            super::run_bundle_import(&[path_string(&stale_head_path)])
        })
        .expect_err("repo-head mismatch bundle must fail");
    assert!(
        head_error.contains("bundle repo head mismatch"),
        "{head_error}"
    );

    remove_dir_all_with_retry(&repo_a, "cleanup repo A");
    remove_dir_all_with_retry(&repo_b, "cleanup repo B");
    remove_dir_all_with_retry(&git_repo, "cleanup git repo");
}

#[test]
fn bundle_replace_failpoints_preserve_old_db_and_success_replaces_atomically() {
    let _guard = lock_bundle_test();
    // CODEGRAPH_WRITE_PATH_FAILPOINT is process-wide; every other test that
    // sets it holds ENV_TEST_LOCK, so this test must too or they clobber each
    // other's failpoint under parallel `cargo test`.
    let _env_guard_lock = lock_env_test();
    let repo = temp_repo();
    write_cli_fixture_file(
        &repo,
        "src/old.ts",
        "export function bundle_old_symbol() { return 1; }\n",
    );
    let target_db = repo.join("target.sqlite");
    index_repo_to_db_with_options(&repo, &target_db, IndexOptions::default())
        .expect("index old target DB");
    assert!(db_has_symbol(&target_db, "bundle_old_symbol"));

    fs::remove_file(repo.join("src").join("old.ts")).expect("remove old source");
    write_cli_fixture_file(
        &repo,
        "src/new.ts",
        "export function bundle_new_symbol() { return 2; }\n",
    );
    let source_db = repo.join("source.sqlite");
    let bundle_path = repo.join("replace.cgc-bundle");
    index_repo_to_db_with_options(&repo, &source_db, IndexOptions::default())
        .expect("index replacement source DB");
    super::with_repo_db_context(&repo, &source_db, || {
        super::run_bundle_export(&["--output".to_string(), path_string(&bundle_path)])
    })
    .expect("export replacement bundle");

    for failpoint in [
        "bundle_after_temp_db_creation_before_validation",
        "bundle_after_validation_before_publish",
        "bundle_during_publish",
    ] {
        let _env_guard = BundleFailpointEnvGuard::set(failpoint);
        let error = super::with_repo_db_context(&repo, &target_db, || {
            super::run_bundle_import(&[path_string(&bundle_path), "--replace".to_string()])
        })
        .expect_err("replace failpoint must fail");
        assert!(
            error.contains(failpoint),
            "failpoint={failpoint} error={error}"
        );
        assert!(
            db_has_symbol(&target_db, "bundle_old_symbol"),
            "old DB lost after {failpoint}"
        );
        assert!(
            !db_has_symbol(&target_db, "bundle_new_symbol"),
            "new DB leaked after {failpoint}"
        );
    }

    let imported = super::with_repo_db_context(&repo, &target_db, || {
        super::run_bundle_import(&[path_string(&bundle_path), "--replace".to_string()])
    })
    .expect("successful replace import");
    assert_eq!(imported["status"].as_str(), Some("imported"));
    assert_eq!(imported["atomic_publish"].as_bool(), Some(true));
    assert_eq!(imported["claimable"].as_bool(), Some(true));
    assert!(db_has_symbol(&target_db, "bundle_new_symbol"));
    assert!(!db_has_symbol(&target_db, "bundle_old_symbol"));

    remove_dir_all_with_retry(&repo, "cleanup repo");
}

#[test]
fn comprehensive_percentile_is_deterministic_for_shuffled_samples() {
    let samples = vec![30.0, 10.0, 20.0, 40.0];

    assert_eq!(percentile(&samples, 0.50), Some(30.0));
    assert_eq!(percentile(&samples, 0.95), Some(40.0));
    assert_eq!(percentile(&samples, 0.99), Some(40.0));
}

#[test]
fn comprehensive_regression_row_marks_improvement_and_regression() {
    let improved = regression_row("db_size", Some(100.0), Some(80.0), false);
    let regressed = regression_row("recall", Some(0.9), Some(0.8), true);

    assert_eq!(improved["status"].as_str(), Some("improved"));
    assert_eq!(regressed["status"].as_str(), Some("regressed"));
}

#[test]
fn benchmark_binary_metadata_for_release_records_exact_command() {
    let metadata = benchmark_binary_metadata_for(
            "C:/repo/target/release/codegraph-mcp.exe".to_string(),
            false,
            "C:/repo/target/release/codegraph-mcp.exe bench proof-build-only --repo repo --db artifact.sqlite".to_string(),
        );

    assert_eq!(
        metadata["current_exe"].as_str(),
        Some("C:/repo/target/release/codegraph-mcp.exe")
    );
    assert_eq!(metadata["debug_assertions"].as_bool(), Some(false));
    assert_eq!(metadata["binary_profile"].as_str(), Some("release"));
    assert_eq!(metadata["claimable_for_thresholds"].as_bool(), Some(true));
    assert_eq!(metadata["diagnostic_only"].as_bool(), Some(false));
    assert!(metadata["exact_command"]
        .as_str()
        .expect("exact command")
        .contains("bench proof-build-only"));
}

#[test]
fn benchmark_inspection_lifecycle_blocks_mismatched_db_as_nonclaimable() {
    let repo_a = ui_fixture_repo();
    let repo_b = ui_fixture_repo();
    let db_path = repo_a.join("benchmark-custom.sqlite");
    index_repo_to_db_with_options(&repo_a, &db_path, IndexOptions::default())
        .expect("index repo A custom DB");

    let lifecycle = benchmark_inspection_lifecycle_status(
        &repo_b,
        &db_path,
        "test.benchmark_mismatch",
        Some(StorageMode::Proof),
    );

    assert_eq!(lifecycle["safe_to_read"].as_bool(), Some(false));
    assert_eq!(lifecycle["claimable"].as_bool(), Some(false));
    assert_eq!(lifecycle["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(
        lifecycle["exact_db_path_checked"].as_str(),
        Some(path_string(&db_path).as_str())
    );
    assert!(
        lifecycle["blockers"]
            .as_array()
            .expect("blockers")
            .iter()
            .any(|blocker| blocker
                .as_str()
                .is_some_and(|value| value.contains("repo root mismatch"))),
        "{lifecycle:?}"
    );

    remove_dir_all_with_retry(&repo_a, "cleanup repo A");
    remove_dir_all_with_retry(&repo_b, "cleanup repo B");
}

#[test]
fn update_integrity_report_marks_inspection_read_only_and_setup_mutations() {
    let workdir = temp_repo();
    let repo = workdir.join("repo");
    let db = workdir.join("fixture.sqlite");
    let mutation_file =
        generate_update_integrity_small_repo(&repo).expect("generate small fixture");

    let result = run_update_integrity_repo(
        "fixture",
        &repo,
        &db,
        &mutation_file,
        1,
        1,
        UpdateBenchmarkMode::Fast,
        UpdateLoopKind::Repeat,
        None,
        None,
        None,
    )
    .expect("run update integrity fixture");

    assert_eq!(result["status"].as_str(), Some("passed"));
    assert_eq!(result["inspection_read_only"].as_bool(), Some(true));
    assert_eq!(
        result["artifact_mutated_during_inspection"].as_bool(),
        Some(false)
    );
    assert_eq!(result["claimable"].as_bool(), Some(true));
    for step in [
        result.get("cold").expect("cold"),
        result.get("repeat_unchanged").expect("repeat"),
    ] {
        assert_eq!(step["inspection_read_only"].as_bool(), Some(true));
        assert_eq!(
            step["artifact_mutated_during_inspection"].as_bool(),
            Some(false)
        );
        assert_eq!(step["claimable"].as_bool(), Some(true));
        assert_eq!(step["lifecycle_status"]["claimable"].as_bool(), Some(true));
    }
    assert_eq!(
        result["cold"]["mutation_capable_operation"].as_str(),
        Some("index_repo_to_db_with_options")
    );

    remove_dir_all_with_retry(&workdir, "cleanup");
}

#[test]
fn update_integrity_staged_update_workspace_rewrites_passport_scope() {
    let workdir = temp_repo();
    let repo = workdir.join("repo");
    let db = workdir.join("fixture.sqlite");
    let stage = workdir.join("staged_update");
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
        repo.join("src").join("auth.ts"),
        "export function login() { return 'ok'; }\n",
    )
    .expect("write fixture");
    let mutation_file = "src/auth.ts";
    let staged_update_repo =
        stage_update_integrity_mutation_repo(&repo, mutation_file, &stage).expect("stage update");

    let result = run_update_integrity_repo(
        "fixture",
        &repo,
        &db,
        mutation_file,
        1,
        1,
        UpdateBenchmarkMode::Fast,
        UpdateLoopKind::Update,
        None,
        Some(&staged_update_repo),
        None,
    )
    .expect("run staged update integrity fixture");

    assert_eq!(result["status"].as_str(), Some("passed"));
    assert_eq!(result["claimable"].as_bool(), Some(true));
    assert_eq!(
        result["update_repo_path"].as_str(),
        Some(path_string(&staged_update_repo).as_str())
    );
    let update = &result["iteration_results"][0]["update"];
    assert_eq!(update["claimable"].as_bool(), Some(true));
    assert_eq!(
        update["lifecycle_status"]["repo_match"].as_bool(),
        Some(true)
    );
    assert_eq!(
        update["lifecycle_status"]["repo_root_observed"].as_str(),
        update["lifecycle_status"]["repo_root_expected"].as_str()
    );

    remove_dir_all_with_retry(&workdir, "cleanup");
}

#[test]
fn comprehensive_markdown_renderer_contains_all_major_sections() {
    let report = json!({
        "sections": {
            "executive_verdict": {
                "verdict": "fail",
                "reason_for_failure": "fixture",
                "optimization_may_continue": true,
                "comparison_claims_allowed": false,
                "exact_failed_targets": ["proof_db_mib"],
                "exact_passed_targets": ["graph_truth_cases_passed"]
            },
            "correctness_gates": { "metrics": [] },
            "context_packet_gate": { "metrics": [] },
            "db_integrity": { "metrics": [] },
            "storage_summary": { "metrics": [] },
            "storage_contributors": { "contributors": [] },
            "row_counts_and_cardinality": { "metrics": [] },
            "cold_proof_build_profile": { "metrics": [] },
            "repeat_unchanged_index": { "metrics": [] },
            "single_file_update": { "metrics": [] },
            "query_latency": { "queries": [] },
            "manual_relation_quality": {
                "status": "unknown",
                "real_relation_precision": "unknown"
            },
            "cgc_competitor_comparison_readiness": {
                "cgc_available": null,
                "cgc_version": null,
                "cgc_completed": false,
                "cgc_timeout": null,
                "verdict": "unknown"
            },
            "regression_summary": { "metrics": [] }
        }
    });
    let markdown = render_comprehensive_benchmark_markdown(&report);

    assert!(markdown.contains("Section 1 - Executive Verdict"));
    assert!(markdown.contains("Section 11 - Query Latency"));
    assert!(markdown.contains("Section 14 - Regression Summary"));
    assert!(markdown.contains("Every future storage change must answer"));
}

#[test]
fn comprehensive_markdown_renderer_reports_manual_precision_table() {
    let report = json!({
        "sections": {
            "executive_verdict": {
                "verdict": "fail",
                "reason_for_failure": "cold build",
                "optimization_may_continue": true,
                "comparison_claims_allowed": false,
                "exact_failed_targets": ["cold_proof_build_total_wall_ms"],
                "exact_passed_targets": ["graph_truth_cases_passed"]
            },
            "correctness_gates": { "metrics": [] },
            "context_packet_gate": { "metrics": [] },
            "db_integrity": { "metrics": [] },
            "storage_summary": { "metrics": [] },
            "storage_contributors": { "contributors": [] },
            "row_counts_and_cardinality": { "metrics": [] },
            "cold_proof_build_profile": { "metrics": [] },
            "repeat_unchanged_index": { "metrics": [] },
            "single_file_update": { "metrics": [] },
            "query_latency": { "queries": [] },
            "manual_relation_quality": {
                "status": "reported",
                "real_relation_precision": "reported_for_labeled_relations_no_claim_for_absent_relations",
                "edges_labeled": 50,
                "target_evaluation": [{
                    "relation": "CALLS",
                    "proof_db_edge_count": 100,
                    "labeled_samples": 50,
                    "precision": 1.0,
                    "target": 0.95,
                    "status": "pass",
                    "claim": "sampled_precision_estimate"
                }],
                "relations": [{
                    "relation": "CALLS",
                    "samples": 50,
                    "precision": 1.0,
                    "source_span_precision": 1.0,
                    "false_positive": 0,
                    "unsure": 0
                }],
                "relation_coverage": {
                    "absent_no_claim_relations": ["AUTHORIZES"]
                }
            },
            "cgc_competitor_comparison_readiness": {
                "cgc_available": null,
                "cgc_version": null,
                "cgc_completed": false,
                "cgc_timeout": null,
                "verdict": "unknown"
            },
            "regression_summary": { "metrics": [] }
        }
    });
    let markdown = render_comprehensive_benchmark_markdown(&report);

    assert!(markdown.contains("sampled_precision_estimate"));
    assert!(markdown.contains("Absent proof-mode relations with no precision claim: AUTHORIZES"));
    assert!(!markdown.contains("If labels are absent"));
}

#[test]
fn debounce_waits_until_paths_are_quiet() {
    let mut debouncer = WatchDebouncer::new(Duration::from_millis(100));
    let now = Instant::now();
    let path = PathBuf::from("src/auth.ts");

    debouncer.push(path.clone(), now);
    assert!(debouncer.ready(now + Duration::from_millis(99)).is_empty());

    debouncer.push(path.clone(), now + Duration::from_millis(99));
    assert!(debouncer.ready(now + Duration::from_millis(150)).is_empty());
    assert_eq!(
        debouncer.ready(now + Duration::from_millis(205)),
        vec![path]
    );
}

#[test]
fn persistent_watch_debouncer_coalesces_rapid_save_and_atomic_paths() {
    let mut debouncer = WatchDebouncer::new(Duration::from_millis(25));
    let now = Instant::now();
    let source = PathBuf::from("src/service.ts");
    let temp = PathBuf::from("src/.service.ts.tmp");

    debouncer.push(source.clone(), now);
    debouncer.push(source.clone(), now + Duration::from_millis(5));
    debouncer.push(temp.clone(), now + Duration::from_millis(6));
    debouncer.push(source.clone(), now + Duration::from_millis(7));

    assert_eq!(debouncer.events_seen(), 4);
    assert_eq!(debouncer.coalesced_count(), 2);
    assert_eq!(debouncer.pending_len(), 2);
    assert!(debouncer.ready(now + Duration::from_millis(20)).is_empty());
    let ready = debouncer.ready(now + Duration::from_millis(40));
    assert_eq!(ready, vec![temp, source]);
    assert_eq!(debouncer.pending_len(), 0);
}

#[test]
fn persistent_watch_retry_helper_retries_transient_locks() {
    let mut attempts = 0usize;
    let (value, retries) = super::agent_use_retry_transient_lock(3, Duration::ZERO, || {
        attempts += 1;
        if attempts < 3 {
            Err("database is locked".to_string())
        } else {
            Ok(json!({"status": "updated"}))
        }
    })
    .expect("retry should eventually succeed");

    assert_eq!(attempts, 3);
    assert_eq!(retries, 2);
    assert_eq!(value["status"].as_str(), Some("updated"));
}

#[test]
fn ignore_patterns_cover_repo_noise() {
    let root = Path::new("/repo");

    assert!(should_ignore_path(root, Path::new("/repo/.git/config")));
    assert!(should_ignore_path(
        root,
        Path::new("/repo/node_modules/pkg/index.js")
    ));
    assert!(should_ignore_path(
        root,
        Path::new("/repo/target/debug/app")
    ));
    assert!(should_ignore_path(
        root,
        Path::new("/repo/.codegraph/codegraph.sqlite")
    ));
    assert!(should_ignore_path(
        root,
        Path::new("/repo/static/d3.min.js")
    ));
    assert!(!should_ignore_path(root, Path::new("/repo/src/auth.ts")));
}

#[test]
fn parse_index_options_accepts_scope_overrides() {
    let args = vec![
        ".".to_string(),
        "--include-ignored".to_string(),
        "--include".to_string(),
        "target/keep.ts".to_string(),
        "--exclude".to_string(),
        "reports/private/**".to_string(),
        "--no-default-excludes".to_string(),
        "--respect-gitignore".to_string(),
        "false".to_string(),
        "--explain-scope".to_string(),
        "--print-included".to_string(),
        "--print-excluded".to_string(),
    ];
    let (repo, db, options) = parse_index_options(&args).expect("parse index options");

    assert_eq!(repo, ".");
    assert!(db.is_none());
    assert!(options.scope.include_ignored);
    assert!(options.scope.no_default_excludes);
    assert!(!options.scope.respect_gitignore);
    assert_eq!(options.scope.include_patterns, vec!["target/keep.ts"]);
    assert_eq!(options.scope.exclude_patterns, vec!["reports/private/**"]);
    assert!(options.scope.explain_scope);
    assert!(options.scope.print_included);
    assert!(options.scope.print_excluded);
}

#[test]
fn index_json_concise_output_excludes_scope_examples_by_default() {
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let value = run_index_output_json(
        &repo,
        &db,
        &["--fresh", "--json", "--exclude", "src/excluded.ts"],
    );

    assert_eq!(value["status"].as_str(), Some("indexed"));
    assert_eq!(value["schema_version"].as_u64(), Some(1));
    assert_eq!(value["output_mode"].as_str(), Some("concise"));
    assert!(value["repo_root"].as_str().is_some());
    assert!(value["db_path"].as_str().is_some());
    assert_eq!(value["scope"]["examples_included"].as_bool(), Some(false));
    assert!(value["scope"]["included_examples"].is_null());
    assert!(value["scope"]["excluded_examples"].is_null());
    assert_eq!(
        value["scope"]["include_semantics"].as_str(),
        Some(super::INCLUDE_SEMANTICS_DEFAULT_SCOPE_PLUS_OVERRIDES)
    );
    assert_eq!(
        value["scope"]["include_is_restrictive"].as_bool(),
        Some(false)
    );
    assert_eq!(value["scope"]["include_is_override"].as_bool(), Some(true));
    assert_eq!(
        value["scope"]["scope_truth_status"].as_str(),
        Some(super::SCOPE_TRUTH_STATUS_OVERRIDE_ONLY)
    );
    assert!(value["db_lifecycle"]["decision"].as_str().is_some());
    assert!(value["db_lifecycle"]["preflight"].is_null());
    assert!(value["lifecycle"]["claimable"].as_bool().is_some());
    assert!(value["counts"]["files"]["seen"].as_u64().is_some());
    assert!(value["counts"]["graph"]["entities"].as_u64().is_some());
    assert!(value["counts"]["graph"]["edges"].as_u64().is_some());
    assert!(value["counts"]["graph"]["source_spans"].as_u64().is_some());
    assert!(value["timing"]["wall_ms"].as_f64().is_some());
    assert_eq!(value["telemetry"]["memory_measured"].as_bool(), Some(false));
    assert_eq!(value["telemetry"]["memory"].as_str(), Some("unknown"));
    assert!(
        serde_json::to_vec(&value).expect("serialize").len()
            < super::INDEX_CONCISE_JSON_SIZE_TARGET_BYTES,
        "{value}"
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn index_json_scope_audit_flags_preserve_examples() {
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");

    let explain = run_index_output_json(
        &repo,
        &db,
        &[
            "--fresh",
            "--json",
            "--explain-scope",
            "--exclude",
            "src/excluded.ts",
        ],
    );
    assert_eq!(explain["status"].as_str(), Some("indexed"));
    assert!(explain["scope"]["included_examples"]
        .as_array()
        .is_some_and(|examples| !examples.is_empty()));
    assert!(explain["scope"]["excluded_examples"]
        .as_array()
        .is_some_and(|examples| !examples.is_empty()));

    let included = run_index_output_json(&repo, &db, &["--fresh", "--json", "--print-included"]);
    assert!(included["scope"]["included_examples"]
        .as_array()
        .is_some_and(|examples| !examples.is_empty()));

    let excluded = run_index_output_json(
        &repo,
        &db,
        &[
            "--fresh",
            "--json",
            "--print-excluded",
            "--exclude",
            "src/excluded.ts",
        ],
    );
    assert!(excluded["scope"]["excluded_examples"]
        .as_array()
        .is_some_and(|examples| !examples.is_empty()));

    let audit = run_index_output_json(&repo, &db, &["--fresh", "--audit-json"]);
    assert!(audit["scope"]["included_examples"].is_array());
    assert!(audit["db_lifecycle"].is_object());

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn index_agent_json_follows_compact_schema() {
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let value = run_index_output_json(&repo, &db, &["--fresh", "--agent-json"]);

    assert_eq!(value["schema_name"].as_str(), Some("index_agent_json"));
    assert_eq!(value["schema_version"].as_u64(), Some(1));
    assert_eq!(value["status"].as_str(), Some("ok"));
    assert_eq!(value["output_mode"].as_str(), Some("agent_json"));
    assert_agent_json_contract(
        &value,
        "index_agent_json",
        "index",
        super::INDEX_AGENT_JSON_SIZE_TARGET_BYTES,
    );
    assert!(value["lifecycle"]["claimable"].as_bool().is_some());
    assert!(value["summary"]["repo"].as_str().is_some());
    assert!(value["summary"]["db_path"].as_str().is_some());
    assert!(value["summary"]["files_seen"].as_u64().is_some());
    assert!(value["summary"]["files_indexed"].as_u64().is_some());
    assert!(value["summary"]["source_spans"].as_u64().is_some());
    assert!(value["summary"]["scope"]["included_examples"].is_null());
    assert!(value["summary"]["scope"]["excluded_examples"].is_null());
    assert_eq!(value["truncation"]["limit_applied"].as_bool(), Some(false));
    assert!(
        serde_json::to_vec(&value).expect("serialize").len()
            < super::INDEX_AGENT_JSON_SIZE_TARGET_BYTES
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn warm_noop_index_json_stays_below_concise_size_target() {
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let _cold = run_index_output_json(&repo, &db, &["--fresh", "--json"]);
    let warm = run_index_output_json(&repo, &db, &["--incremental", "--json"]);

    assert_eq!(warm["output_mode"].as_str(), Some("concise"));
    assert_eq!(warm["scope"]["included_examples"].is_null(), true);
    assert_eq!(warm["scope"]["excluded_examples"].is_null(), true);
    let bytes = serde_json::to_vec(&warm).expect("serialize").len();
    assert!(
        bytes < super::INDEX_CONCISE_JSON_SIZE_TARGET_BYTES,
        "warm no-op concise JSON was {bytes} bytes: {warm}"
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn index_profile_json_labels_unknown_memory_and_timing_truth() {
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let value = run_index_output_json(&repo, &db, &["--fresh", "--json", "--profile"]);

    assert_eq!(value["telemetry"]["memory"].as_str(), Some("unknown"));
    assert_eq!(value["telemetry"]["memory_measured"].as_bool(), Some(false));
    assert_eq!(
        value["telemetry"]["memory_measurement_kind"].as_str(),
        Some("not_measured")
    );
    let timing_fields = &value["telemetry"]["timing_fields"];
    assert_eq!(
        timing_fields["db_write"]["measurement"].as_str(),
        Some("measured_sql_write_aggregate")
    );
    assert!(timing_fields["transaction_commit"]["status"]
        .as_str()
        .is_some());
    assert!(
        timing_fields["reducer"]["status"].as_str().is_some(),
        "{timing_fields}"
    );
    assert_eq!(
        timing_fields["candidate_spool_build"]["status"].as_str(),
        Some("unknown_or_not_run")
    );
    assert_eq!(
        timing_fields["vector_sidecar_build"]["measurement"].as_str(),
        Some("vector_runtime_sidecar_build")
    );
    assert_eq!(
        value["telemetry"]["debug_timing_not_used_for_claim"].as_bool(),
        Some(true)
    );
    assert_eq!(
        value["telemetry"]["binary_profile"].as_str(),
        Some(crate::build_profile())
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn index_release_profile_attribution_includes_slowest_files_and_unknowns() {
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let value = run_index_output_json(&repo, &db, &["--fresh", "--json", "--profile"]);
    let profile = &value["profile"];

    assert!(profile["stage_attribution"]
        .as_array()
        .expect("stage attribution")
        .iter()
        .any(|stage| stage["name"].as_str() == Some("parse")
            && stage["total_ms"].is_number()
            && stage["p50_ms"].is_number()));
    assert!(profile["stage_attribution"]
        .as_array()
        .expect("stage attribution")
        .iter()
        .any(|stage| stage["name"].as_str() == Some("db_write")
            && stage["distribution"].as_str() == Some("aggregate_only")));
    assert!(profile["slowest_stages"]
        .as_array()
        .expect("slowest stages")
        .iter()
        .any(|stage| stage["name"].as_str().is_some()));

    let files = profile["file_attribution"]
        .as_array()
        .expect("file attribution");
    let service = files
        .iter()
        .find(|file| file["path"].as_str() == Some("src/service.ts"))
        .expect("service file attribution");
    assert_eq!(service["file_kind"].as_str(), Some("typescript"));
    assert_eq!(service["source_role"].as_str(), Some("production"));
    assert!(service["bytes"].as_u64().unwrap_or_default() > 0);
    assert!(service["read_ms"].is_number());
    assert!(service["hash_ms"].is_number());
    assert!(service["parse_ms"].is_number());
    assert!(service["extract_ms"].is_number());
    assert!(service["local_fact_count"].as_u64().unwrap_or_default() > 0);
    assert!(service["entity_count"].as_u64().unwrap_or_default() > 0);
    assert!(service["edge_count"].as_u64().is_some());
    assert!(service["source_span_count"].as_u64().is_some());
    assert!(service["text_evidence_count"].as_u64().is_some());
    assert!(service["db_write_ms"].is_null());
    assert_eq!(
        service["db_write_attribution"].as_str(),
        Some("aggregate_only")
    );

    assert!(profile["slowest_files"]
        .as_array()
        .expect("slowest files")
        .iter()
        .any(|file| file["path"].as_str() == Some("src/service.ts")));
    assert!(profile["slowest_files_by_parse"]
        .as_array()
        .expect("slowest by parse")
        .iter()
        .any(|file| file["path"].as_str() == Some("src/service.ts")));
    assert!(profile["highest_entity_files"]
        .as_array()
        .expect("highest entity files")
        .iter()
        .any(|file| file["path"].as_str() == Some("src/service.ts")));
    assert_eq!(profile["memory_measured"].as_bool(), Some(false));
    assert!(profile["memory_bytes"].is_null());
    assert_eq!(profile["memory_status"].as_str(), Some("unknown"));
    assert_eq!(
        profile["source_clone_count_status"].as_str(),
        Some("unknown")
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn warm_unchanged_release_profile_labels_unread_files_truthfully() {
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let cold = run_index_output_json(&repo, &db, &["--fresh", "--json", "--profile"]);
    let warm = run_index_output_json(&repo, &db, &["--json", "--profile"]);

    assert!(cold["files_read"].as_u64().unwrap_or_default() > 0);
    assert_eq!(warm["files_read"].as_u64(), Some(0));
    assert_eq!(warm["files_parsed"].as_u64(), Some(0));
    let warm_files = warm["profile"]["file_attribution"]
        .as_array()
        .expect("warm file attribution");
    let service = warm_files
        .iter()
        .find(|file| file["path"].as_str() == Some("src/service.ts"))
        .expect("warm service attribution");
    assert!(service["read_ms"].is_null());
    assert!(service["hash_ms"].is_null());
    assert!(service["parse_ms"].is_null());
    assert!(service["skipped_labels"]
        .as_array()
        .expect("skipped labels")
        .iter()
        .any(|label| label.as_str() == Some("metadata_unchanged")));

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn parse_index_command_options_accepts_agent_json_and_audit_json() {
    let agent = vec![".".to_string(), "--agent-json".to_string()];
    let (_, _, options, mode, _, _) =
        super::parse_index_command_options(&agent).expect("parse agent index options");
    assert!(options.json);
    assert_eq!(mode, super::IndexJsonOutputMode::Agent);

    let audit = vec![".".to_string(), "--audit-json".to_string()];
    let (_, _, options, mode, _, _) =
        super::parse_index_command_options(&audit).expect("parse audit index options");
    assert!(options.json);
    assert_eq!(mode, super::IndexJsonOutputMode::Audit);

    let explain = vec![
        ".".to_string(),
        "--json".to_string(),
        "--explain-scope".to_string(),
    ];
    let (_, _, _, mode, _, _) =
        super::parse_index_command_options(&explain).expect("parse explain index options");
    assert_eq!(mode, super::IndexJsonOutputMode::Audit);
}

#[test]
fn parse_index_command_options_accepts_build_vector_index() {
    let args = vec![
        ".".to_string(),
        "--agent-json".to_string(),
        "--build-vector-index".to_string(),
        "vectors/codegraph-vector-chunks.json".to_string(),
    ];
    let (_, _, options, mode, _, vector_index_output) =
        super::parse_index_command_options(&args).expect("parse vector index option");

    assert!(options.json);
    assert_eq!(mode, super::IndexJsonOutputMode::Agent);
    assert_eq!(
        vector_index_output.runtime_path,
        Some(PathBuf::from("vectors/codegraph-vector-chunks.json"))
    );
    assert_eq!(
        vector_index_output.runtime_format,
        super::VectorChunkArtifactFormat::CompactJson
    );
}

#[test]
fn parse_index_command_options_accepts_candidate_spool_caps() {
    let args = vec![
        ".".to_string(),
        "--candidate-spool".to_string(),
        "candidate-spool.jsonl".to_string(),
        "--candidate-spool-policy".to_string(),
        "bounded".to_string(),
        "--candidate-spool-required".to_string(),
        "--candidate-spool-query-index".to_string(),
        "yes".to_string(),
        "--candidate-spool-max-mib".to_string(),
        "1".to_string(),
        "--candidate-spool-max-bytes".to_string(),
        "1048576".to_string(),
        "--candidate-spool-max-records".to_string(),
        "123".to_string(),
        "--candidate-spool-per-file-max-records".to_string(),
        "7".to_string(),
        "--candidate-spool-per-dir-soft-cap".to_string(),
        "23".to_string(),
        "--candidate-spool-max-snippet-bytes".to_string(),
        "96".to_string(),
        "--candidate-spool-max-snippets-per-file".to_string(),
        "2".to_string(),
        "--candidate-spool-max-symbols-per-file".to_string(),
        "5".to_string(),
        "--max-artifacts-mib".to_string(),
        "100".to_string(),
    ];
    let (_, _, options, _, budget, _) =
        super::parse_index_command_options(&args).expect("parse candidate spool caps");
    assert_eq!(
        options.candidate_spool_path,
        Some(PathBuf::from("candidate-spool.jsonl"))
    );
    assert_eq!(
        options.candidate_spool_policy,
        super::CandidateSpoolPolicy::Bounded
    );
    assert!(options.candidate_spool_required);
    assert!(options.candidate_spool_query_index);
    assert_eq!(options.candidate_spool_caps.global_max_bytes, 1_048_576);
    assert_eq!(options.candidate_spool_caps.global_max_records, 123);
    assert_eq!(options.candidate_spool_caps.per_file_max_records, 7);
    assert_eq!(options.candidate_spool_caps.per_top_level_dir_soft_cap, 23);
    assert_eq!(options.candidate_spool_caps.max_snippet_bytes, 96);
    assert_eq!(options.candidate_spool_caps.max_snippets_per_file_packet, 2);
    assert_eq!(options.candidate_spool_caps.max_symbols_per_file_packet, 5);
    assert_eq!(budget.max_artifacts_mib, 100.0);
}

#[test]
fn index_optional_candidate_spool_budget_tight_disables_without_failing_db() {
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let spool = repo.join("candidate-spool.jsonl");
    let value = run_index_output_json(
        &repo,
        &db,
        &[
            "--fresh",
            "--agent-json",
            "--candidate-spool",
            spool.to_str().expect("spool path"),
            "--max-artifacts-mib",
            "0.01",
        ],
    );

    assert_eq!(value["status"].as_str(), Some("ok"));
    assert_eq!(value["graph_db_claimable"].as_bool(), Some(true));
    assert_eq!(value["candidate_spool_required"].as_bool(), Some(false));
    assert_eq!(
        value["candidate_spool_disabled_reason"].as_str(),
        Some("candidate_spool_budget_too_small")
    );
    assert_eq!(
        value["artifact_budget_decision"].as_str(),
        Some("candidate_spool_disabled_budget_exceeded")
    );
    assert_eq!(
        value["index_exit_status_reason"].as_str(),
        Some("indexed_candidate_spool_disabled_or_unavailable")
    );
    assert_eq!(
        value["summary"]["candidate_spool_status"].as_str(),
        Some("no_spool")
    );
    assert!(!spool.exists());
    assert!(db.exists());

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn index_required_candidate_spool_budget_tight_fails_explicitly() {
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let spool = repo.join("candidate-spool.jsonl");
    let output = run([
        BIN_NAME.to_string(),
        "index".to_string(),
        path_string(&repo),
        "--db".to_string(),
        path_string(&db),
        "--fresh".to_string(),
        "--agent-json".to_string(),
        "--candidate-spool".to_string(),
        path_string(&spool),
        "--candidate-spool-required".to_string(),
        "--max-artifacts-mib".to_string(),
        "0.01".to_string(),
    ]);

    assert_eq!(output.exit_code, 1);
    let error: Value = serde_json::from_str(&output.stderr).expect("error JSON");
    assert_eq!(
        error["error"].as_str(),
        Some("candidate_spool_required_budget_exceeded")
    );
    assert_eq!(error["candidate_spool_required"].as_bool(), Some(true));
    assert_eq!(
        error["candidate_spool_disabled_reason"].as_str(),
        Some("candidate_spool_budget_too_small")
    );
    assert_eq!(error["graph_db_claimable"].as_bool(), Some(false));
    assert!(!spool.exists());

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn index_agent_json_can_build_deterministic_vector_index() {
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let vector_index = repo.join("vectors").join("codegraph-vector-chunks.json");
    let value = run_index_output_json(
        &repo,
        &db,
        &[
            "--fresh",
            "--agent-json",
            "--build-vector-index",
            vector_index.to_str().expect("utf8 vector path"),
        ],
    );

    assert_eq!(value["status"].as_str(), Some("ok"));
    assert_eq!(value["vector_index"]["status"].as_str(), Some("ok"));
    assert!(value["vector_index"]["chunk_count"].as_u64().unwrap_or(0) > 0);
    assert_eq!(
        value["vector_index"]["persisted_total_chunks"],
        value["vector_index"]["chunk_count"]
    );
    assert_eq!(
        value["vector_index"]["selected_total_chunks"],
        value["vector_index"]["persisted_total_chunks"]
    );
    assert!(
        value["vector_index"]["generated_total_chunks"]
            .as_u64()
            .unwrap_or(0)
            >= value["vector_index"]["persisted_total_chunks"]
                .as_u64()
                .unwrap_or(0)
    );
    assert!(
        value["vector_index"]["actual_index_file_bytes"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );
    assert!(
        value["vector_index"]["estimated_f32_payload_bytes"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );
    assert_eq!(
        value["vector_index"]["estimated_vector_bytes_deprecated_alias_for"].as_str(),
        Some("estimated_f32_payload_bytes")
    );
    assert_eq!(
        value["vector_index"]["index_artifact_format"].as_str(),
        Some("compact_json")
    );
    assert_eq!(
        value["vector_index"]["runtime_sidecar_bytes"],
        value["vector_index"]["actual_index_file_bytes"]
    );
    assert_eq!(
        value["vector_index"]["runtime_selected_chunks"],
        value["vector_index"]["selected_total_chunks"]
    );
    assert_eq!(value["vector_index"]["audit_artifact_bytes"], Value::Null);
    assert_eq!(value["vector_index"]["audit_chunks"].as_u64(), Some(0));
    assert_eq!(value["vector_index"]["pretty_json_overhead"], Value::Null);
    assert_eq!(
        value["vector_index"]["runtime_sidecar_path"].as_str(),
        vector_index.to_str()
    );
    assert_eq!(
        value["vector_index"]["vector_payload_compression"].as_str(),
        Some("none")
    );
    assert_eq!(
        value["vector_index"]["stores_full_source_body"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["vector_index"]["chunk_selection_strategy"].as_str(),
        Some("diversity_ranked_v1")
    );
    assert_eq!(
        value["vector_index"]["input_order_cap"].as_bool(),
        Some(false)
    );
    assert!(value["vector_index"]["persisted_chunks_by_source_kind"].is_object());
    assert_eq!(
        value["vector_index"]["provider_id"].as_str(),
        Some("codegraph-deterministic-test")
    );
    assert_eq!(value["vector_index"]["graph_proof"].as_bool(), Some(false));
    assert!(value["vector_index"]["build_timings"].is_object());
    assert!(
        value["vector_index"]["build_timings"]["total_ms"]
            .as_f64()
            .unwrap_or(0.0)
            >= 0.0
    );
    assert!(
        value["vector_index"]["build_timings"]["selection_ms"]
            .as_f64()
            .unwrap_or(0.0)
            >= 0.0
    );
    assert!(
        value["vector_index"]["build_timings"]["embedding_ms"]
            .as_f64()
            .unwrap_or(0.0)
            >= 0.0
    );
    assert_eq!(value["external_provider"].as_bool(), Some(false));
    assert_eq!(value["source_leaves_machine"].as_bool(), Some(false));
    assert!(vector_index.exists(), "vector index file was not written");
    let runtime_text = fs::read_to_string(&vector_index).expect("runtime sidecar");
    assert!(
        !runtime_text.contains("selection_reason"),
        "runtime sidecar must not store verbose selection_reason: {runtime_text}"
    );
    assert!(runtime_text.contains("\"artifact_kind\":\"vector_runtime_sidecar\""));

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn index_vector_runtime_and_audit_artifacts_are_split() {
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let runtime = repo.join("vectors").join("runtime.json");
    let audit = repo.join("vectors").join("audit.json");
    let value = run_index_output_json(
        &repo,
        &db,
        &[
            "--fresh",
            "--agent-json",
            "--build-vector-index",
            runtime.to_str().expect("runtime path"),
            "--vector-audit-artifact",
            audit.to_str().expect("audit path"),
        ],
    );

    assert_eq!(value["status"].as_str(), Some("ok"));
    assert!(runtime.exists(), "runtime sidecar was not written");
    assert!(audit.exists(), "audit artifact was not written");
    assert_eq!(
        value["vector_index"]["index_artifact_format"].as_str(),
        Some("compact_json")
    );
    assert!(
        value["vector_index"]["runtime_sidecar_bytes"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(
        value["vector_index"]["audit_artifact_bytes"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(
        value["vector_index"]["pretty_json_overhead"]
            .as_i64()
            .unwrap_or_default()
            > 0
    );
    assert_eq!(
        value["vector_index"]["audit_chunks"],
        value["vector_index"]["selected_total_chunks"]
    );
    let runtime_text = fs::read_to_string(&runtime).expect("runtime");
    let audit_text = fs::read_to_string(&audit).expect("audit");
    assert!(!runtime_text.contains("selection_reason"));
    assert!(audit_text.contains("selection_reason"));
    assert!(runtime_text.contains("\"artifact_kind\":\"vector_runtime_sidecar\""));
    assert!(audit_text.contains("\"artifact_kind\": \"audit_artifact\""));

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn index_vector_audit_artifact_requires_runtime_sidecar() {
    let args = vec![
        ".".to_string(),
        "--agent-json".to_string(),
        "--vector-audit-artifact".to_string(),
        "audit.json".to_string(),
    ];
    let error = super::parse_index_command_options(&args)
        .expect_err("audit-only vector artifact should be rejected");
    assert!(error.contains("--vector-audit-artifact requires"));
}

#[test]
fn context_pack_loads_compact_runtime_vector_sidecar() {
    let _guard = lock_bundle_test();
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let runtime = repo.join("vectors").join("runtime.json");
    let _index = run_index_output_json(
        &repo,
        &db,
        &[
            "--fresh",
            "--agent-json",
            "--build-vector-index",
            runtime.to_str().expect("runtime path"),
        ],
    );
    let args = vec![
        "--task".to_string(),
        "indexOutputService".to_string(),
        "--enable-vector-candidates".to_string(),
        "--vector-runtime-sidecar".to_string(),
        runtime.to_str().expect("runtime path").to_string(),
        "--agent-json".to_string(),
        "--explain".to_string(),
        "--max-output-bytes".to_string(),
        "1048576".to_string(),
    ];
    let value = super::with_repo_db_context(&repo, &db, || super::run_context_pack_command(&args))
        .expect("context-pack");

    let trace = &value["retrieval_explain"]["vector_trace"];
    assert_eq!(trace["vector_index_status"].as_str(), Some("ready"));
    assert_eq!(value["vector_runtime_status"].as_str(), Some("ready"));
    assert_eq!(value["graph_db_status"].as_str(), Some("ready"));
    assert_eq!(value["graph_proof_available"].as_bool(), Some(true));
    assert!(value["active_candidate_sources"]
        .as_array()
        .expect("candidate sources")
        .iter()
        .any(|source| source.as_str() == Some("vector_semantic")));
    assert_eq!(
        value["routing_packet"]["layer_readiness"]["vector_runtime"]["status"].as_str(),
        Some("ready")
    );
    assert!(trace["vector_candidate_count"].as_u64().unwrap_or_default() > 0);
    let metrics = &trace["vector_index_metrics"];
    assert_eq!(
        metrics["artifact_kind"].as_str(),
        Some("vector_runtime_sidecar")
    );
    assert_eq!(
        metrics["index_artifact_format"].as_str(),
        Some("compact_json")
    );
    assert!(
        metrics["runtime_sidecar_bytes"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert_eq!(metrics["audit_artifact_bytes"], Value::Null);

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn context_pack_omits_runtime_vector_sidecar_after_source_file_delete() {
    let _guard = lock_bundle_test();
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let runtime = repo.join("vectors").join("runtime.json");
    let _index = run_index_output_json(
        &repo,
        &db,
        &[
            "--fresh",
            "--agent-json",
            "--build-vector-index",
            runtime.to_str().expect("runtime path"),
        ],
    );
    fs::remove_file(repo.join("src").join("service.ts")).expect("delete source file");

    let args = vec![
        "--task".to_string(),
        "indexOutputService".to_string(),
        "--enable-vector-candidates".to_string(),
        "--vector-runtime-sidecar".to_string(),
        runtime.to_str().expect("runtime path").to_string(),
        "--agent-json".to_string(),
        "--explain".to_string(),
        "--max-output-bytes".to_string(),
        "1048576".to_string(),
    ];
    let value = super::with_repo_db_context(&repo, &db, || super::run_context_pack_command(&args))
        .expect("context-pack with stale runtime");

    let trace = &value["retrieval_explain"]["vector_trace"];
    assert_eq!(trace["vector_index_status"].as_str(), Some("stale"));
    assert_eq!(value["vector_runtime_status"].as_str(), Some("stale"));
    assert_eq!(trace["vector_candidate_count"].as_u64(), Some(0));
    assert!(trace["stale_missing_vector_index_reason"]
        .as_str()
        .unwrap_or_default()
        .contains("vector_source_binding_stale"));
    assert_eq!(value["graph_proof_available"].as_bool(), Some(true));

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn context_pack_loads_legacy_pretty_vector_artifact() {
    let _guard = lock_bundle_test();
    let repo = index_output_fixture_repo();
    let db = repo.join("index-output.sqlite");
    let runtime = repo.join("vectors").join("runtime.json");
    let legacy = repo.join("vectors").join("legacy-pretty.json");
    let _index = run_index_output_json(
        &repo,
        &db,
        &[
            "--fresh",
            "--agent-json",
            "--build-vector-index",
            runtime.to_str().expect("runtime path"),
        ],
    );
    let mut legacy_value: Value =
        serde_json::from_str(&fs::read_to_string(&runtime).expect("runtime json"))
            .expect("runtime value");
    if let Some(metadata) = legacy_value
        .get_mut("metadata")
        .and_then(Value::as_object_mut)
    {
        metadata.remove("artifact_kind");
    }
    legacy_value["metadata"]["index_artifact_format"] = json!("pretty_json");
    fs::write(
        &legacy,
        serde_json::to_string_pretty(&legacy_value).expect("legacy pretty json"),
    )
    .expect("write legacy artifact");

    let args = vec![
        "--task".to_string(),
        "indexOutputService".to_string(),
        "--enable-vector-candidates".to_string(),
        "--vector-index".to_string(),
        legacy.to_str().expect("legacy path").to_string(),
        "--agent-json".to_string(),
        "--explain".to_string(),
        "--max-output-bytes".to_string(),
        "1048576".to_string(),
    ];
    let value = super::with_repo_db_context(&repo, &db, || super::run_context_pack_command(&args))
        .expect("context-pack");

    let metrics = &value["retrieval_explain"]["vector_trace"]["vector_index_metrics"];
    assert_eq!(
        metrics["artifact_kind"].as_str(),
        Some("legacy_pretty_json_vector_artifact")
    );
    assert_eq!(
        metrics["index_artifact_format"].as_str(),
        Some("pretty_json")
    );
    assert_eq!(metrics["runtime_sidecar_bytes"], Value::Null);
    assert!(
        value["retrieval_explain"]["vector_trace"]["vector_candidate_count"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn binary_signature_cache_changes_after_file_update() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
        repo.join("src").join("auth.ts"),
        "export function login() {\n  return 'old';\n}\n",
    )
    .expect("write source");
    index_repo(&repo).expect("initial index");

    let store =
        SqliteGraphStore::open(repo.join(".codegraph").join("codegraph.sqlite")).expect("store");
    let entity = store
        .find_entities_by_exact_symbol("login")
        .expect("symbol lookup")
        .into_iter()
        .find(|entity| entity.name == "login")
        .expect("login entity");
    let mut cache = IncrementalIndexCache::new(256).expect("cache");
    cache.refresh_from_store(&store).expect("initial cache");
    let before = cache
        .signature_words(&entity.id)
        .expect("initial signature")
        .to_vec();
    drop(store);

    fs::write(
        repo.join("src").join("auth.ts"),
        "export function login() {\n  return 'new';\n}\n",
    )
    .expect("rewrite source");
    let summary =
        update_changed_files_with_cache(&repo, &[PathBuf::from("src/auth.ts")], &mut cache)
            .expect("incremental update");
    let after = cache
        .signature_words(&entity.id)
        .expect("updated signature")
        .to_vec();

    assert_eq!(summary.files_indexed, 1);
    assert_eq!(summary.binary_signatures_updated, summary.entities);
    assert_ne!(before, after);

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn index_profile_json_schema_and_warm_skip_are_reported() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
        repo.join("src").join("auth.ts"),
        "export function login() {\n  return 'ok';\n}\n",
    )
    .expect("write source");

    let cold = index_repo_with_options(
        &repo,
        IndexOptions {
            profile: true,
            json: true,
            ..IndexOptions::default()
        },
    )
    .expect("cold index");
    let cold_full_json = super::index_summary_json(&cold).expect("full profile json");
    assert_eq!(
        cold_full_json["indexing_durability"]["temp_db_never_claimable"].as_bool(),
        Some(true)
    );
    assert_eq!(
        cold_full_json["indexing_durability"]["publish_status"].as_str(),
        Some("published")
    );
    assert!(
        cold_full_json["graph_output_budgets"]["worker_dispatch_source_clone_policy"]
            .as_str()
            .unwrap_or_default()
            .contains("without cloning")
    );
    let cold_profile = cold.profile.expect("cold profile");
    assert_eq!(cold.files_indexed, 1);
    assert!(cold_profile.semantic_resolver_ms <= cold_profile.total_wall_ms);
    assert_eq!(cold_profile.memory_measured, false);
    assert_eq!(cold_profile.memory_status, "unknown");
    assert_eq!(cold_profile.memory_measurement_kind, "not_measured");
    assert_eq!(cold_profile.memory_bytes, None);
    assert_eq!(
        cold_profile.db_write_measurement,
        "measured_sql_write_aggregate"
    );
    assert_ne!(
        cold_profile.fts_search_index_measurement,
        "legacy_db_write_bucket"
    );
    assert!(cold_profile.worker_count >= 1);
    assert!(cold_profile.files_per_sec >= 0.0);
    assert_eq!(
        cold.graph_output_budgets
            .worker_dispatch_source_clone_policy,
        "pending source buffers are moved into worker chunks without cloning"
    );
    assert_eq!(
        cold.graph_output_budgets.claimability_label,
        "full_graph_output_with_no_budget_degradation"
    );

    let warm = index_repo_with_options(
        &repo,
        IndexOptions {
            profile: true,
            json: true,
            ..IndexOptions::default()
        },
    )
    .expect("warm index");
    let warm_profile = warm.profile.expect("warm profile");
    assert_eq!(warm.files_indexed, 0);
    assert_eq!(warm.files_read, 0);
    assert_eq!(warm.files_parsed, 0);
    assert!(warm_profile.skipped_unchanged_files >= 1);
    assert_eq!(warm_profile.memory_measured, false);
    assert_eq!(warm_profile.memory_status, "unknown");

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn index_batch_boundary_uses_file_and_byte_limits() {
    assert!(should_start_new_index_batch(
        DEFAULT_INDEX_BATCH_MAX_FILES,
        1,
        1,
        DEFAULT_INDEX_BATCH_MAX_FILES,
        DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES
    ));
    assert!(should_start_new_index_batch(
        1,
        DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES - 4,
        8,
        DEFAULT_INDEX_BATCH_MAX_FILES,
        DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES
    ));
    assert!(!should_start_new_index_batch(
        0,
        0,
        DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES + 1,
        DEFAULT_INDEX_BATCH_MAX_FILES,
        DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES
    ));
}

#[test]
fn cold_index_commits_multiple_batches_and_reports_profile_fields() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    for index in 0..(DEFAULT_INDEX_BATCH_MAX_FILES + 2) {
        fs::write(
            repo.join("src").join(format!("file_{index}.ts")),
            format!("export function service{index}() {{ return {index}; }}\n"),
        )
        .expect("write source");
    }

    let summary = index_repo_with_options(
        &repo,
        IndexOptions {
            profile: true,
            json: true,
            ..IndexOptions::default()
        },
    )
    .expect("index repo");

    assert_eq!(summary.files_indexed, DEFAULT_INDEX_BATCH_MAX_FILES + 2);
    assert_eq!(summary.batches_total, 2);
    assert_eq!(summary.batches_completed, 2);
    assert_eq!(summary.batch_max_files, DEFAULT_INDEX_BATCH_MAX_FILES);
    assert_eq!(
        summary.batch_max_source_bytes,
        DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES
    );
    assert!(summary.profile.expect("profile").worker_count >= 1);

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn bad_utf8_file_is_skipped_reported_and_old_facts_are_deleted() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    let file = repo.join("src").join("bad.py");
    fs::write(&file, "def broken():\n    return 1\n").expect("write valid source");
    let cold = index_repo_with_options(
        &repo,
        IndexOptions {
            profile: true,
            json: true,
            ..IndexOptions::default()
        },
    )
    .expect("cold index");
    assert_eq!(cold.files_indexed, 1);

    fs::write(&file, [0xff, 0xfe, 0xfd]).expect("write invalid utf8");
    let second = index_repo_with_options(
        &repo,
        IndexOptions {
            profile: true,
            json: true,
            ..IndexOptions::default()
        },
    )
    .expect("second index");

    assert_eq!(second.files_indexed, 0);
    assert_eq!(second.failed_files_deleted, 1);
    assert_eq!(second.issue_counts.get("read_error").copied(), Some(1));
    assert!(second
        .issues
        .iter()
        .any(|issue| issue.repo_relative_path == "src/bad.py"
            && issue.action == "skipped_and_deleted_old_facts"));

    let store = SqliteGraphStore::open(repo.join(".codegraph").join("codegraph.sqlite"))
        .expect("open store");
    assert!(store.get_file("src/bad.py").expect("get file").is_none());
    drop(store);

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn source_only_discovery_ignores_local_generated_state_dirs() {
    let repo = temp_repo();
    for ignored in [
        ".arl",
        ".codegraphcontext",
        ".codegraph-competitors",
        ".codegraph-bench-cache",
        ".tools",
        ".codex-tools",
        "__pycache__",
    ] {
        fs::create_dir_all(repo.join(ignored).join("src")).expect("create ignored dir");
        fs::write(
            repo.join(ignored).join("src").join("ignored.ts"),
            "export function ignored() { return 1; }\n",
        )
        .expect("write ignored source");
        assert!(should_ignore_path(
            &repo,
            &repo.join(ignored).join("src").join("ignored.ts")
        ));
    }

    let summary = index_repo_with_options(
        &repo,
        IndexOptions {
            profile: true,
            json: true,
            ..IndexOptions::default()
        },
    )
    .expect("index repo");
    assert_eq!(summary.files_indexed, 0);

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn parallel_parse_extract_is_deterministic() {
    let pending = (0..4)
        .map(|index| PendingIndexFile {
            file_hash: format!("hash-{index}"),
            language: Some("typescript".to_string()),
            file_kind: "typescript".to_string(),
            source_role: "production".to_string(),
            source_role_classification_ms: 0.0,
            repo_relative_path: format!("src/file_{index}.ts"),
            source: format!(
                "export function service{index}(value: number) {{\n  return value + {index};\n}}\n"
            ),
            size_bytes: 64,
            modified_unix_nanos: None,
            needs_delete: false,
            duplicate_of: None,
            template_required: false,
        })
        .collect::<Vec<_>>();

    let (serial, _) = parse_extract_pending_files(pending.clone(), 1).expect("serial parse");
    let (parallel, _) = parse_extract_pending_files(pending, 4).expect("parallel parse");
    let serial_ids = serial
        .iter()
        .flat_map(|file| {
            file.extraction
                .entities
                .iter()
                .map(|entity| entity.id.clone())
        })
        .collect::<Vec<_>>();
    let parallel_ids = parallel
        .iter()
        .flat_map(|file| {
            file.extraction
                .entities
                .iter()
                .map(|entity| entity.id.clone())
        })
        .collect::<Vec<_>>();
    assert_eq!(serial_ids, parallel_ids);
}

#[test]
fn watch_long_running_startup_uses_external_db_without_touching_default() {
    let repo = ui_fixture_repo();
    let external_db = repo.join("watch-external.sqlite");
    index_repo_to_db_with_options(&repo, &external_db, IndexOptions::default())
        .expect("index external watch DB");
    let default_db = repo.join(".codegraph").join("codegraph.sqlite");
    assert!(
        !default_db.exists(),
        "external DB setup should not create default DB"
    );

    let startup = prepare_watch_startup(&repo, Some(&external_db), "test.watch.long_running")
        .expect("prepare watch startup");

    assert_eq!(startup.requested_db_path, external_db);
    assert_eq!(startup.actual_db_path_opened, external_db);
    assert!(!startup.auto_index_enabled);
    assert_eq!(
        startup.lifecycle_status["requested_db_path"].as_str(),
        Some(external_db.to_string_lossy().as_ref())
    );
    assert_eq!(
        startup.lifecycle_status["actual_db_path_opened"].as_str(),
        Some(external_db.to_string_lossy().as_ref())
    );
    assert_eq!(
        startup.lifecycle_status["lifecycle_status"].as_str(),
        Some("safe_to_write")
    );
    assert_eq!(
        startup.lifecycle_status["auto_index_enabled"].as_bool(),
        Some(false)
    );
    assert!(
        !default_db.exists(),
        "long-running watch startup must not touch default DB when --db is supplied"
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn watch_long_running_startup_rejects_unsafe_external_db() {
    let repo_a = ui_fixture_repo();
    let repo_b = ui_fixture_repo();
    let external_db = repo_a.join("watch-external.sqlite");
    index_repo_to_db_with_options(&repo_a, &external_db, IndexOptions::default())
        .expect("index repo A external DB");

    let error = match prepare_watch_startup(&repo_b, Some(&external_db), "test.watch.long_running")
    {
        Ok(_) => panic!("repo B must reject repo A DB"),
        Err(error) => error,
    };
    let message = error.to_string();
    assert!(message.contains("not safe for watch updates"), "{message}");
    assert!(message.contains("repo root mismatch"), "{message}");

    remove_dir_all_with_retry(&repo_a, "cleanup repo A");
    remove_dir_all_with_retry(&repo_b, "cleanup repo B");
}

#[test]
fn watch_long_running_startup_missing_db_is_clear_and_does_not_auto_index() {
    let repo = ui_fixture_repo();
    let missing_external = repo.join("missing-watch.sqlite");
    let error =
        match prepare_watch_startup(&repo, Some(&missing_external), "test.watch.long_running") {
            Ok(_) => panic!("missing external DB must fail"),
            Err(error) => error,
        };
    let message = error.to_string();
    assert!(
        message.contains("run `codegraph-mcp index . --db"),
        "{message}"
    );
    assert!(
        message.contains("does not auto-index by default"),
        "{message}"
    );
    assert!(
        !repo.join(".codegraph").exists(),
        "missing external DB startup must not create default state"
    );

    let missing_default = match prepare_watch_startup(&repo, None, "test.watch.long_running") {
        Ok(_) => panic!("missing default DB must fail"),
        Err(error) => error,
    };
    let default_message = missing_default.to_string();
    assert!(
        default_message.contains("does not auto-index by default"),
        "{default_message}"
    );
    assert!(
        !repo.join(".codegraph").exists(),
        "missing default DB startup must not auto-create .codegraph"
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn large_synthetic_index_generator_smoke_test() {
    let output = temp_repo();
    let repo = output.join("repo");

    generate_large_synthetic_repo(&repo, 8).expect("generate synthetic repo");
    let summary = index_repo_with_options(
        &repo,
        IndexOptions {
            profile: true,
            json: true,
            ..IndexOptions::default()
        },
    )
    .expect("index synthetic repo");

    assert_eq!(summary.files_seen, 10);
    assert_eq!(summary.files_indexed, 8);
    assert!(summary.entities > 0);
    assert!(summary.profile.expect("profile").worker_count >= 1);

    remove_dir_all_with_retry(&output, "cleanup");
}

#[test]
fn doctor_and_config_outputs_are_structured_nonfatal() {
    let repo = temp_repo();

    let doctor = run([
        BIN_NAME,
        "doctor",
        repo.to_str().expect("repo path"),
        "--json",
    ]);
    assert_eq!(doctor.exit_code, 0, "stderr={}", doctor.stderr);
    let doctor_json: Value = serde_json::from_str(&doctor.stdout).expect("doctor JSON");
    assert_eq!(doctor_json["status"].as_str(), Some("ok"));
    assert!(doctor_json["checks"].as_array().expect("checks").len() >= 5);
    assert_eq!(doctor_json["database_exists"].as_bool(), Some(false));
    assert_eq!(doctor_json["safe_to_query"].as_bool(), Some(false));
    assert_eq!(
        doctor_json["path_access_status"].as_str(),
        Some("db_missing")
    );
    assert_eq!(doctor_json["db_problem_kind"].as_str(), Some("db_missing"));
    assert!(doctor_json["db_lifecycle_read"].is_object());
    assert!(doctor_json["sqlite_sidecars"].is_object());
    assert_eq!(doctor_json["telemetry"]["memory"].as_str(), Some("unknown"));
    assert_eq!(
        doctor_json["telemetry"]["memory_measured"].as_bool(),
        Some(false)
    );

    let metadata = run([BIN_NAME, "config", "release-metadata", "--json"]);
    assert_eq!(metadata.exit_code, 0, "stderr={}", metadata.stderr);
    let metadata_json: Value = serde_json::from_str(&metadata.stdout).expect("metadata JSON");
    assert_eq!(metadata_json["release"]["schema_version"].as_u64(), Some(1));
    assert!(metadata_json["release"]["archives"]
        .as_array()
        .expect("archives")
        .iter()
        .any(|archive| archive["name"].as_str() == Some("windows-x64")));

    let completions = run([
        BIN_NAME,
        "config",
        "completions",
        "--shell",
        "powershell",
        "--json",
    ]);
    assert_eq!(completions.exit_code, 0, "stderr={}", completions.stderr);
    let completions_json: Value =
        serde_json::from_str(&completions.stdout).expect("completions JSON");
    assert!(completions_json["script"]
        .as_str()
        .expect("script")
        .contains("Register-ArgumentCompleter"));

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn doctor_rejects_misplaced_db_global_with_targeted_error() {
    let output = run([BIN_NAME, "doctor", "--db", "graph.sqlite"]);

    assert_eq!(output.exit_code, 1);
    let error: Value = serde_json::from_str(&output.stderr).expect("error JSON");
    assert_eq!(error["error"].as_str(), Some("doctor_failed"));
    assert!(error["message"]
        .as_str()
        .expect("message")
        .contains("--db is a global flag"));
}

#[test]
fn agent_use_profile_resolver_is_collision_safe_and_stable() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let parent = temp_repo();
    let repo_a = parent.join("left").join("app");
    let repo_b = parent.join("right").join("app");
    fs::create_dir_all(&repo_a).expect("create repo a");
    fs::create_dir_all(&repo_b).expect("create repo b");
    add_git_remote_for_test(&parent, "https://example.invalid/acme/parent.git");

    let profile_a =
        super::resolve_agent_use_profile_with_data_root(&repo_a, &data_root).expect("profile a");
    let profile_a_again = super::resolve_agent_use_profile_with_data_root(&repo_a, &data_root)
        .expect("profile a again");
    let profile_a_canonical = super::resolve_agent_use_profile_with_data_root(
        &fs::canonicalize(&repo_a).expect("canonical repo a"),
        &data_root,
    )
    .expect("profile a canonical");
    let profile_b =
        super::resolve_agent_use_profile_with_data_root(&repo_b, &data_root).expect("profile b");

    assert_eq!(
        profile_a.profile_name,
        super::PRODUCTION_AGENT_USE_PROFILE_NAME
    );
    assert_eq!(profile_a.db_path, profile_a_again.db_path);
    assert_eq!(profile_a.db_path, profile_a_canonical.db_path);
    assert_eq!(profile_a.repo_identity_hash.len(), 32);
    assert_eq!(
        super::agent_use_repo_identity_short_hash(&profile_a),
        profile_a
            .repo_identity_hash
            .chars()
            .take(12)
            .collect::<String>()
    );
    assert_eq!(profile_a.repo_identity_label, "app");
    assert!(profile_a
        .profile_root
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.starts_with("app-")));
    assert_ne!(profile_a.db_path, profile_b.db_path);
    assert_ne!(profile_a.profile_root, profile_b.profile_root);
    assert_ne!(
        profile_a.candidate_spool_path,
        profile_b.candidate_spool_path
    );
    assert_ne!(
        profile_a.candidate_spool_query_index_path,
        profile_b.candidate_spool_query_index_path
    );
    assert_ne!(profile_a.vector_runtime_path, profile_b.vector_runtime_path);
    assert_ne!(profile_a.vector_audit_path, profile_b.vector_audit_path);
    assert_ne!(
        profile_a.lock_or_publish_state_path,
        profile_b.lock_or_publish_state_path
    );
    assert_ne!(profile_a.delta_state_path, profile_b.delta_state_path);
    assert_ne!(profile_a.repo_identity_hash, profile_b.repo_identity_hash);
    assert!(profile_a.db_path.starts_with(&data_root));
    assert!(!profile_a.db_path.starts_with(repo_a.join(".codegraph")));
    assert_eq!(
        profile_a
            .db_path
            .file_name()
            .and_then(|value| value.to_str()),
        Some("production-agent-use.sqlite")
    );
    assert_eq!(
        profile_a.candidate_spool_query_index_path,
        super::candidate_spool_query_index_path(&profile_a.candidate_spool_path)
    );
    assert_eq!(
        profile_a
            .delta_state_path
            .file_name()
            .and_then(|value| value.to_str()),
        Some(super::PRODUCTION_AGENT_USE_DELTA_STATE_FILE_NAME)
    );
    assert_eq!(
        profile_a.mcp_args,
        vec![
            "--repo".to_string(),
            path_string(&profile_a.repo_root),
            "--db".to_string(),
            path_string(&profile_a.db_path),
            "serve-mcp".to_string()
        ]
    );

    remove_dir_all_with_retry(&data_root, "cleanup data root");
    remove_dir_all_with_retry(&parent, "cleanup parent");
}

#[test]
fn agent_use_profile_resolver_handles_remote_path_spaces_unicode_and_case() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let (spaces_parent, spaces_repo) = temp_repo_named("repo with spaces");
    let (unicode_parent, unicode_repo) = temp_repo_named("unicode-repo-é");
    let (remote_parent, remote_repo) = temp_repo_named("remote-app");
    let long_name = "very-long-repository-name-".repeat(4);
    let (long_parent, long_repo) = temp_repo_named(&long_name);
    add_git_remote_for_test(&remote_repo, "https://example.invalid/acme/remote-app.git");

    let spaces = super::resolve_agent_use_profile_with_data_root(&spaces_repo, &data_root)
        .expect("spaces profile");
    let unicode = super::resolve_agent_use_profile_with_data_root(&unicode_repo, &data_root)
        .expect("unicode profile");
    let remote = super::resolve_agent_use_profile_with_data_root(&remote_repo, &data_root)
        .expect("remote profile");
    let remote_again = super::resolve_agent_use_profile_with_data_root(&remote_repo, &data_root)
        .expect("remote profile again");
    let long = super::resolve_agent_use_profile_with_data_root(&long_repo, &data_root)
        .expect("long profile");
    let relative_spaces = {
        let mut process = super::ProcessContextSnapshot::capture().expect("capture context");
        std::env::set_current_dir(&spaces_parent).expect("set cwd to spaces parent");
        let relative = spaces_repo
            .file_name()
            .map(PathBuf::from)
            .expect("spaces repo name");
        let profile = super::resolve_agent_use_profile_with_data_root(&relative, &data_root)
            .expect("relative spaces profile");
        process.restore().expect("restore context");
        profile
    };

    assert!(spaces.db_path.starts_with(&data_root));
    assert!(unicode.db_path.starts_with(&data_root));
    assert!(remote.db_path.starts_with(&data_root));
    assert!(long.db_path.starts_with(&data_root));
    assert_eq!(spaces.db_path, relative_spaces.db_path);
    assert_eq!(
        spaces.repo_identity_hash,
        relative_spaces.repo_identity_hash
    );
    assert_eq!(remote.db_path, remote_again.db_path);
    assert_eq!(
        remote.repo_identity_hash,
        super::stable_agent_use_identity_hash(
            b"remote:https://example.invalid/acme/remote-app.git"
        )
    );
    let expected_spaces_material = format!(
        "path:{}",
        super::normalize_agent_use_identity_path(&path_string(&spaces.repo_root))
    );
    assert_eq!(
        spaces.repo_identity_hash,
        super::stable_agent_use_identity_hash(expected_spaces_material.as_bytes())
    );
    assert_ne!(spaces.repo_identity_hash, unicode.repo_identity_hash);
    assert_ne!(remote.repo_identity_hash, spaces.repo_identity_hash);
    assert!(long.repo_identity_label.len() <= 64);
    assert!(long
        .profile_root
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.len() <= 97));
    assert!(
        !spaces.db_path.to_string_lossy().contains(".codegraph"),
        "{spaces:?}"
    );
    assert!(
        !unicode.db_path.to_string_lossy().contains(".codegraph"),
        "{unicode:?}"
    );

    #[cfg(windows)]
    {
        let upper = PathBuf::from(path_string(&spaces_repo).to_ascii_uppercase());
        if upper.exists() {
            let upper_profile = super::resolve_agent_use_profile_with_data_root(&upper, &data_root)
                .expect("upper-case profile");
            assert_eq!(spaces.db_path, upper_profile.db_path);
        }
    }

    remove_dir_all_with_retry(&data_root, "cleanup data root");
    remove_dir_all_with_retry(&spaces_parent, "cleanup spaces");
    remove_dir_all_with_retry(&unicode_parent, "cleanup unicode");
    remove_dir_all_with_retry(&remote_parent, "cleanup remote");
    remove_dir_all_with_retry(&long_parent, "cleanup long");
}

#[test]
fn git_metadata_unavailable_identity_does_not_drift() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    add_git_remote_for_test(&repo, "https://example.invalid/acme/profile-drift.git");
    let remote_profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("remote profile");
    assert_eq!(
        remote_profile.repo_identity_hash,
        super::stable_agent_use_identity_hash(
            b"remote:https://example.invalid/acme/profile-drift.git"
        )
    );
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("index remote-keyed profile");
    assert!(
        remote_profile.db_path.exists(),
        "indexed remote profile DB should exist"
    );

    let _git_unavailable =
        BundleFailpointEnvGuard::set(super::AGENT_USE_GIT_METADATA_UNAVAILABLE_FAILPOINT);
    let degraded_profile = with_agent_use_data_root(&data_root, || {
        super::resolve_agent_use_profile(&repo).expect("degraded profile")
    });
    assert_eq!(degraded_profile.db_path, remote_profile.db_path);
    assert_eq!(
        degraded_profile.repo_identity_hash,
        remote_profile.repo_identity_hash
    );

    let status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("status with git metadata unavailable");
    assert_ne!(status["status"].as_str(), Some("not_indexed"));
    assert_eq!(
        status["db_path"].as_str(),
        Some(path_string(&remote_profile.db_path).as_str())
    );
    assert_eq!(
        status["repo_identity_hash"].as_str(),
        Some(remote_profile.repo_identity_hash.as_str())
    );

    let config = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "mcp-config".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("mcp config with git metadata unavailable");
    assert_ne!(config["status"].as_str(), Some("not_indexed"));
    assert_eq!(
        config["db_path"].as_str(),
        Some(path_string(&remote_profile.db_path).as_str())
    );
    assert_eq!(
        config["repo_identity_hash"].as_str(),
        Some(remote_profile.repo_identity_hash.as_str())
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn same_repo_same_profile_identity_across_status_index_mcp_config() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");

    let missing_status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("missing status");
    assert_eq!(missing_status["status"].as_str(), Some("not_indexed"));
    assert_eq!(
        missing_status["db_path"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert!(
        !profile.profile_root.exists(),
        "status must not create the production profile"
    );

    let indexed = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("index");
    assert_eq!(indexed["status"].as_str(), Some("indexed"));
    assert_eq!(
        indexed["db_path"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_eq!(
        indexed["repo_identity_hash"].as_str(),
        Some(profile.repo_identity_hash.as_str())
    );

    let status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("status");
    let config = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "mcp-config".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("mcp config");
    assert_eq!(status["db_path"], indexed["db_path"]);
    assert_eq!(config["db_path"], indexed["db_path"]);
    assert_eq!(status["repo_identity_hash"], config["repo_identity_hash"]);
    assert_eq!(
        config["mcp_config_identity"]["repo_identity_hash"],
        status["repo_identity_hash"]
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn same_repo_same_profile_identity_across_query_context_validate_watch() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("index");

    let query = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "symbols".to_string(),
            "agentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("query");
    let context = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--task".to_string(),
            "Find agentUseTarget callers".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("context-pack");
    let validate = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "validate-edit".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--changed".to_string(),
            "src/service.ts".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("validate-edit");
    let watch = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/service.ts".to_string(),
            "--json".to_string(),
        ])
    })
    .expect("watch");

    for value in [&query, &context, &validate, &watch] {
        assert_eq!(
            value["db_path"].as_str().or_else(|| value["db"].as_str()),
            Some(path_string(&profile.db_path).as_str()),
            "{value:?}"
        );
        assert_ne!(value["status"].as_str(), Some("not_indexed"), "{value:?}");
    }
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn permission_denied_not_false_not_indexed() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let _failpoint =
        BundleFailpointEnvGuard::set(super::AGENT_USE_PROFILE_PARENT_PERMISSION_DENIED_FAILPOINT);

    for command in ["status", "mcp-config"] {
        let value = with_agent_use_data_root(&data_root, || {
            super::run_agent_use_command(&[
                command.to_string(),
                "--repo".to_string(),
                path_string(&repo),
                "--json".to_string(),
            ])
        })
        .expect("structured access status");
        assert_eq!(value["status"].as_str(), Some("permission_denied"));
        assert_ne!(value["status"].as_str(), Some("not_indexed"));
        assert_eq!(
            value["profile_access"]["not_false_not_indexed"].as_bool(),
            Some(true)
        );
        assert_json_array_contains(&value, "safety_labels", "profile_root_inaccessible");
    }
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn inaccessible_profile_root_structured_error() {
    let _guard = lock_env_test();
    let parent = temp_repo();
    let data_root = parent.join("agent-use-data-root-is-a-file");
    fs::write(&data_root, "not a directory").expect("write file data root");
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);

    for command in ["status", "mcp-config"] {
        let value = with_agent_use_data_root(&data_root, || {
            super::run_agent_use_command(&[
                command.to_string(),
                "--repo".to_string(),
                path_string(&repo),
                "--json".to_string(),
            ])
        })
        .expect("structured inaccessible profile root");
        assert_eq!(value["status"].as_str(), Some("filesystem_inaccessible"));
        assert_ne!(value["status"].as_str(), Some("not_indexed"));
        assert_eq!(
            value["profile_access"]["not_false_not_indexed"].as_bool(),
            Some(true)
        );
        assert_json_array_contains(&value, "safety_labels", "profile_root_inaccessible");
    }
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&parent, "cleanup parent");
}

#[test]
fn artifact_safe_filename_shortens_deterministically_and_preserves_origin_metadata() {
    let long_name = format!(
        "{}-report.sqlite",
        "very-long-generated-artifact-name".repeat(10)
    );
    let original = PathBuf::from("reports")
        .join("audit")
        .join("artifacts")
        .join(&long_name);

    let first = super::artifact_safe_file_name_for_path(&original, ".metadata.json", 96);
    let second = super::artifact_safe_file_name_for_path(&original, ".metadata.json", 96);
    let other = super::artifact_safe_file_name_for_path(
        &PathBuf::from("reports")
            .join("audit")
            .join("artifacts")
            .join(format!("other-{long_name}")),
        ".metadata.json",
        96,
    );

    assert_eq!(first, second);
    assert!(first.shortened, "{first:?}");
    assert!(first.file_name.chars().count() <= 96, "{first:?}");
    assert!(first.file_name.ends_with(".metadata.json"), "{first:?}");
    assert!(first.extension_preserved);
    assert_ne!(first.file_name, other.file_name);

    let metadata = super::artifact_safe_file_name_metadata(&first);
    assert_eq!(
        metadata["original_path"].as_str(),
        Some(super::path_string(&original).as_str())
    );
    assert_eq!(metadata["shortened"].as_bool(), Some(true));
    assert_eq!(
        metadata["collision_strategy"].as_str(),
        Some("readable_prefix_plus_stable_hash_suffix")
    );

    let default_plan = super::artifact_safe_file_name_for_path(
        &original,
        ".metadata.json",
        super::ARTIFACT_SAFE_FILENAME_MAX_CHARS,
    );
    let default_metadata_path = super::default_artifact_metadata_path(&original);
    assert_eq!(
        default_metadata_path
            .file_name()
            .and_then(|value| value.to_str()),
        Some(default_plan.file_name.as_str())
    );
}

#[test]
fn context_pack_status_query_and_doctor_share_explicit_external_db() {
    let _guard = lock_env_test();
    let repo = temp_repo();
    let db_root = temp_repo();
    let db_path = db_root.join("external").join("production-agent-use.sqlite");
    write_agent_use_context_fixture(&repo);
    index_repo_to_db_with_options(&repo, &db_path, IndexOptions::default())
        .expect("index external DB");
    assert_no_dot_codegraph_sqlite(&repo);
    let db_string = path_string(&db_path);

    let status = super::with_repo_db_context(&repo, &db_path, || {
        run_status_command(&[path_string(&repo)])
    })
    .expect("status");
    let doctor = super::with_repo_db_context(&repo, &db_path, || {
        run_doctor_command(&[path_string(&repo), "--json".to_string()])
    })
    .expect("doctor");
    let query = super::with_repo_db_context(&repo, &db_path, || {
        super::run_query_command(&[
            "symbols".to_string(),
            "agentUseTarget".to_string(),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("query");
    let context = super::with_repo_db_context(&repo, &db_path, || {
        super::run_context_pack_command(&[
            "--task".to_string(),
            "Find agentUseTarget in the fixture repo".to_string(),
            "--seed".to_string(),
            "agentUseTarget".to_string(),
            "--mode".to_string(),
            "production".to_string(),
            "--limit-paths".to_string(),
            "5".to_string(),
            "--limit-snippets".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("context-pack");

    assert_eq!(status["db_path"].as_str(), Some(db_string.as_str()));
    assert_eq!(
        status["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
        Some(db_string.as_str())
    );
    assert_eq!(doctor["db_path"].as_str(), Some(db_string.as_str()));
    assert_eq!(
        doctor["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
        Some(db_string.as_str())
    );
    assert_eq!(query["db"].as_str(), Some(db_string.as_str()));
    assert_eq!(query["resolved_db"].as_str(), Some(db_string.as_str()));
    assert_eq!(context["db"].as_str(), Some(db_string.as_str()));
    assert_eq!(
        context["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
        Some(db_string.as_str())
    );
    assert_eq!(
        context["lifecycle"]["decision"].as_str(),
        Some("read_reuse")
    );
    assert_eq!(context["status"].as_str(), Some("ok"));
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&db_root, "cleanup db root");
}

#[test]
fn context_pack_missing_external_db_reports_missing_without_dot_codegraph_fallback() {
    let _guard = lock_env_test();
    let repo = temp_repo();
    let db_root = temp_repo();
    let db_path = db_root.join("missing").join("production-agent-use.sqlite");
    write_agent_use_context_fixture(&repo);

    let error = super::with_repo_db_context(&repo, &db_path, || {
        super::run_context_pack_command(&[
            "--task".to_string(),
            "Find agentUseTarget in the fixture repo".to_string(),
            "--seed".to_string(),
            "agentUseTarget".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect_err("missing external DB should fail");

    assert!(error.contains("db_missing"), "{error}");
    assert!(error.contains(&path_string(&db_path)), "{error}");
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&db_root, "cleanup db root");
}

#[test]
fn context_pack_stale_external_db_is_diagnostic_and_nonclaimable() {
    let _guard = lock_env_test();
    let repo = temp_repo();
    let db_root = temp_repo();
    let db_path = db_root.join("stale").join("production-agent-use.sqlite");
    write_agent_use_context_fixture(&repo);
    index_repo_to_db_with_options(&repo, &db_path, IndexOptions::default())
        .expect("index external DB");
    {
        let connection = Connection::open(&db_path).expect("open external DB");
        connection
            .execute(
                "UPDATE codegraph_db_passport SET last_run_status = 'interrupted' WHERE id = 1",
                [],
            )
            .expect("mark DB stale");
    }

    let context = super::with_repo_db_context(&repo, &db_path, || {
        super::run_context_pack_command(&[
            "--task".to_string(),
            "Find agentUseTarget in the fixture repo".to_string(),
            "--seed".to_string(),
            "agentUseTarget".to_string(),
            "--allow-stale-read".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("diagnostic stale context-pack");

    assert_eq!(context["claimable"].as_bool(), Some(false));
    assert_eq!(context["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(
        context["db_lifecycle_read"]["claimable"].as_bool(),
        Some(false)
    );
    assert_eq!(
        context["db_lifecycle_read"]["artifact_freshness"].as_str(),
        Some("incomplete:interrupted")
    );
    assert_eq!(
        context["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
        Some(path_string(&db_path).as_str())
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&db_root, "cleanup db root");
}

#[test]
fn agent_use_status_missing_db_is_readonly() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
        &repo,
        "src/service.ts",
        "export function agentUseTarget() {\n  return \"stale-service-token\";\n}\n",
    );
    write_cli_fixture_file(
            &repo,
            "src/main.ts",
            "import { agentUseTarget } from './service';\n\nexport function callAgentUseTarget() {\n  return agentUseTarget();\n}\n",
        );
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    assert!(!profile.profile_root.exists());

    let status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use status");

    assert_eq!(status["status"].as_str(), Some("not_indexed"));
    assert_eq!(status["claimable"].as_bool(), Some(false));
    assert_eq!(status["external_db_used"].as_bool(), Some(true));
    assert_eq!(
        status["db"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_eq!(
        status["db_lifecycle_read"]["path_access_status"].as_str(),
        Some("db_missing")
    );
    assert!(!profile.profile_root.exists());
    assert!(!profile.db_path.exists());
    assert_eq!(
        status["recovery"]["agent_use_mcp_config_available"].as_bool(),
        Some(true)
    );
    assert_eq!(
        status["recovery"]["agent_use_query_available"].as_bool(),
        Some(true)
    );
    assert_eq!(count_key_occurrences(&status, "recovery_commands"), 1);
    assert!(status["recovery"]["commands"].is_null());
    assert_eq!(
        status["recovery"]["commands_ref"].as_str(),
        Some("recovery_commands")
    );
    assert!(
        serialized_len_for_test(&status) <= super::DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        "{} bytes: {status}",
        serialized_len_for_test(&status)
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_rejects_db_override() {
    let status_error = super::run_agent_use_command(&[
        "status".to_string(),
        "--repo".to_string(),
        ".".to_string(),
        "--db".to_string(),
        "elsewhere.sqlite".to_string(),
        "--json".to_string(),
    ])
    .expect_err("status --db must be rejected");
    assert!(status_error.contains("--db is not accepted"));

    let index_error = super::run_agent_use_command(&[
        "index".to_string(),
        "--repo".to_string(),
        ".".to_string(),
        "--db".to_string(),
        "elsewhere.sqlite".to_string(),
        "--json".to_string(),
    ])
    .expect_err("index --db must be rejected");
    assert!(index_error.contains("--db is not accepted"));

    let watch_error = super::run_agent_use_command(&[
        "watch".to_string(),
        "--repo".to_string(),
        ".".to_string(),
        "--db".to_string(),
        "elsewhere.sqlite".to_string(),
        "--once".to_string(),
        "--changed".to_string(),
        "src/service.ts".to_string(),
        "--json".to_string(),
    ])
    .expect_err("watch --db must be rejected");
    assert!(watch_error.contains("--db is not accepted"));
}

#[test]
fn agent_use_index_and_status_use_external_profile_db() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
        &repo,
        "src/service.ts",
        "export function agentUseTarget() {\n  return \"obsoletezz\";\n}\n",
    );
    write_cli_fixture_file(
            &repo,
            "src/main.ts",
            "import { agentUseTarget } from './service';\n\nexport function callAgentUseTarget() {\n  return agentUseTarget();\n}\n",
        );
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");

    let cold = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");
    assert_eq!(cold["status"].as_str(), Some("indexed"));
    assert_eq!(cold["output_mode"].as_str(), Some("concise"));
    assert_eq!(cold["external_db_used"].as_bool(), Some(true));
    assert_eq!(
        cold["db_path"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_eq!(cold["normal_dot_codegraph_mutated"].as_bool(), Some(false));
    assert_eq!(cold["candidate_spool_requested"].as_bool(), Some(true));
    assert_eq!(cold["candidate_spool_created"].as_bool(), Some(true));
    assert_eq!(
        cold["candidate_spool_query_index_status"].as_str(),
        Some("ready")
    );
    assert_eq!(cold["vector_runtime_requested"].as_bool(), Some(true));
    assert_eq!(cold["vector_runtime_created"].as_bool(), Some(true));
    assert_eq!(cold["vector_runtime_status"].as_str(), Some("ready"));
    assert_eq!(cold["vector_audit_requested"].as_bool(), Some(false));
    assert_eq!(cold["vector_audit_created"].as_bool(), Some(false));
    assert_eq!(cold["vector_audit_status"].as_str(), Some("missing"));
    assert_eq!(
        cold["indexing_durability"]["temp_db_never_claimable"].as_bool(),
        Some(true)
    );
    assert_eq!(
        cold["indexing_durability"]["batch_progress_status"].as_str(),
        Some("processed_not_durably_committed_until_transaction_commit")
    );
    assert_eq!(
        cold["indexing_durability"]["publish_status"].as_str(),
        Some("published")
    );
    assert_eq!(
        cold["graph_output_budgets"]["worker_dispatch_source_clone_policy"].as_str(),
        Some("pending source buffers are moved into worker chunks without cloning")
    );
    assert_eq!(
        cold["graph_output_budgets"]["claimability_label"].as_str(),
        Some("full_graph_output_with_no_budget_degradation")
    );
    assert_eq!(
        cold["scope"]["include_semantics"].as_str(),
        Some(super::INCLUDE_SEMANTICS_DEFAULT_SCOPE_PLUS_OVERRIDES)
    );
    assert_eq!(
        cold["scope"]["include_is_restrictive"].as_bool(),
        Some(false)
    );
    assert_eq!(cold["telemetry"]["memory"].as_str(), Some("unknown"));
    assert_eq!(cold["telemetry"]["memory_measured"].as_bool(), Some(false));
    assert!(profile.db_path.exists());
    assert!(profile.candidate_spool_path.exists());
    assert!(profile.candidate_spool_query_index_path.exists());
    assert!(profile.vector_runtime_path.exists());
    assert!(!profile.vector_audit_path.exists());
    assert_no_dot_codegraph_sqlite(&repo);

    let wal_before = PathBuf::from(format!("{}-wal", profile.db_path.to_string_lossy()));
    let shm_before = PathBuf::from(format!("{}-shm", profile.db_path.to_string_lossy()));
    let wal_existed_before_status = wal_before.exists();
    let shm_existed_before_status = shm_before.exists();
    let status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use status");
    assert_eq!(status["status"].as_str(), Some("ok"));
    assert_eq!(status["claimable"].as_bool(), Some(true));
    assert_eq!(wal_before.exists(), wal_existed_before_status);
    assert_eq!(shm_before.exists(), shm_existed_before_status);
    assert_eq!(
        status["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_eq!(status["graph_db_status"].as_str(), Some("ready"));
    assert_eq!(
        status["staged_availability"]["layer_readiness"]["candidate_spool"]["query_index_status"]
            .as_str(),
        Some("ready")
    );
    assert_eq!(status["vector_runtime_status"].as_str(), Some("ready"));
    assert_eq!(
        status["staged_availability"]["layer_readiness"]["vector_audit"]["status"].as_str(),
        Some("missing")
    );
    assert_eq!(status["graph_proof_available"].as_bool(), Some(true));
    assert_eq!(status["candidate_only_available"].as_bool(), Some(true));
    assert_eq!(
        status["db_schema_version"].as_u64(),
        Some(SCHEMA_VERSION as u64)
    );
    assert_eq!(
        status["status_detail_source"].as_str(),
        Some("db_lifecycle_preflight_and_passport_only")
    );
    if !status["read_path_metrics"].is_null() {
        assert_eq!(
            status["read_path_metrics"]["full_scan_count"].as_u64(),
            Some(0)
        );
        if !status["read_path_metrics"]["source_file_load_count"].is_null() {
            assert_eq!(
                status["read_path_metrics"]["source_file_load_count"].as_u64(),
                Some(0)
            );
        }
        if !status["read_path_metrics"]["entities_hydrated"].is_null() {
            assert_eq!(
                status["read_path_metrics"]["entities_hydrated"].as_u64(),
                Some(0)
            );
        }
        if !status["read_path_metrics"]["edges_hydrated"].is_null() {
            assert_eq!(
                status["read_path_metrics"]["edges_hydrated"].as_u64(),
                Some(0)
            );
        }
    }
    assert!(status["storage_accounting"].is_null());
    assert!(status["relation_counts"].is_null());
    assert_eq!(status["schema_name"].as_str(), Some("status_compact_json"));
    assert_eq!(count_key_occurrences(&status, "recovery_commands"), 1);
    assert!(status["profile"].is_null());
    assert!(status["agent_use_profile"].is_null());
    assert_eq!(
        status["agent_json_budget"]["max_output_bytes"].as_u64(),
        Some(super::DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES as u64)
    );
    assert!(
        serialized_len_for_test(&status) <= super::DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        "{} bytes: {status}",
        serialized_len_for_test(&status)
    );
    let status_explain = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
            "--explain".to_string(),
        ])
    })
    .expect("agent-use status explain");
    assert!(!status_explain["profile"].is_null());
    assert!(status_explain["db_lifecycle_read"]["reasons"].is_array());
    assert_eq!(
        status_explain["agent_json_detail_mode"].as_str(),
        Some("explain")
    );
    assert_eq!(
        count_key_occurrences(&status_explain, "recovery_commands"),
        1
    );
    assert_no_dot_codegraph_sqlite(&repo);

    let warm = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("warm agent-use index");
    assert_eq!(warm["external_db_used"].as_bool(), Some(true));
    assert_eq!(warm["warm_unchanged_reuse"].as_bool(), Some(true));
    assert_eq!(warm["candidate_spool_requested"].as_bool(), Some(true));
    assert_eq!(warm["vector_runtime_requested"].as_bool(), Some(true));
    assert_eq!(warm["telemetry"]["memory"].as_str(), Some("unknown"));
    assert_eq!(warm["telemetry"]["memory_measured"].as_bool(), Some(false));
    assert_eq!(warm["files_read"].as_u64(), Some(0));
    assert_eq!(warm["files_parsed"].as_u64(), Some(0));
    assert_eq!(
        warm["indexing_durability"]["temp_db_never_claimable"].as_bool(),
        Some(true)
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_validate_edit_panic_failpoint_emits_structured_error_packet_and_stage_file() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function stagePanicTarget() {\n  return \"before\";\n}\n",
    );
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");

    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function stagePanicTarget() {\n  return \"after\";\n}\n",
    );

    let packet = {
        let _failpoint = BundleFailpointEnvGuard::set("agent_use_validation_panic_at_delta");
        with_agent_use_data_root(&data_root, || {
            super::run_agent_use_command(&[
                "validate-edit".to_string(),
                "--repo".to_string(),
                path_string(&repo),
                "--changed".to_string(),
                "src/service.js".to_string(),
                "--agent-json".to_string(),
            ])
        })
        .expect("a post-commit panic must surface as a structured packet, not an Err/abort")
    };

    assert_eq!(packet["status"].as_str(), Some("error"), "{packet}");
    assert_eq!(packet["validation_pipeline_error"].as_bool(), Some(true));
    assert_eq!(packet["validation_state"].as_str(), Some("incomplete"));
    assert_eq!(packet["failed_stage"].as_str(), Some("delta"));
    assert!(
        packet["panic_message"]
            .as_str()
            .unwrap_or_default()
            .contains("agent_use_validation_panic_at_delta"),
        "{packet}"
    );
    assert_eq!(packet["_cli_exit_code"].as_i64(), Some(1));
    assert_eq!(packet["command"].as_str(), Some("validate-edit"));
    assert_eq!(packet["graph_claimability_unchanged"].as_bool(), Some(true));
    assert_eq!(packet["must_fix_before_continuing"].as_bool(), Some(false));
    assert!(
        packet["recovery_commands"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0)
            > 0,
        "{packet}"
    );

    let stage_path = super::agent_use_validation_stage_path(&profile);
    assert!(
        stage_path.exists(),
        "stage sidecar must survive a caught panic for post-mortem"
    );
    let stage: Value =
        serde_json::from_str(&fs::read_to_string(&stage_path).expect("read stage sidecar"))
            .expect("stage sidecar json");
    let last_stage = stage["stages"]
        .as_array()
        .and_then(|stages| stages.last())
        .cloned()
        .unwrap_or_default();
    assert_eq!(last_stage["name"].as_str(), Some("delta"), "{stage}");

    let clean = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "validate-edit".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--changed".to_string(),
            "src/service.js".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("clean validate-edit after the failpoint is removed");
    assert_ne!(clean["status"].as_str(), Some("error"), "{clean}");
    assert!(
        !stage_path.exists(),
        "a clean run must remove the stage sidecar again"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

/// Warning findings from a validate-edit packet, robust to the compact
/// budget contract: the findings anchor is `validation_packet.warnings`;
/// older shapes carried the array top-level.
fn validate_edit_warning_findings(packet: &Value) -> Vec<Value> {
    packet
        .pointer("/validation_packet/warnings")
        .and_then(Value::as_array)
        .or_else(|| packet.get("warnings").and_then(Value::as_array))
        .cloned()
        .unwrap_or_default()
}

fn validate_edit_args_for(repo: &Path) -> Vec<String> {
    vec![
        "validate-edit".to_string(),
        "--repo".to_string(),
        path_string(repo),
        "--changed".to_string(),
        "src/service.js".to_string(),
        "--agent-json".to_string(),
        "--fail-on-blocking".to_string(),
    ]
}

#[test]
fn validate_edit_warns_on_new_unresolved_local_call_and_clears_on_fix() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    let clean_source = "export function login(input) {\n  return input;\n}\n";
    write_cli_fixture_file(&repo, "src/service.js", clean_source);
    // A sibling module whose helper exists (indexed): a NEW valid
    // cross-module call must stay diagnostic (candidates-exist never warns —
    // 2026-06-11 adversarial finding: warning there fires on the edit that
    // fixes a hallucination).
    write_cli_fixture_file(
        &repo,
        "src/format.js",
        "export function formatHelper(value) {\n  return value;\n}\n",
    );
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    // Forward-direction hallucination (Q4): a NEW call to a nonexistent local
    // symbol, plus a builtin call and a valid cross-module call that must
    // both stay quiet.
    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function login(input) {\n  console.log(input);\n  formatHelper(input);\n  return hallucinatedHelper(input);\n}\n",
    );
    let first = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("validate-edit with hallucinated call");

    let block = &first["unresolved_references"];
    assert!(
        block["new_count"].as_u64().unwrap_or_default() >= 1,
        "{first}"
    );
    assert!(
        block["by_class"]["repo_local_candidate"]
            .as_u64()
            .unwrap_or_default()
            >= 1,
        "{block}"
    );
    assert_eq!(block["not_graph_proof"].as_bool(), Some(true), "{block}");
    let escalated = block["escalated"].as_array().expect("escalated array");
    assert!(
        escalated.iter().any(|item| {
            item["name"].as_str() == Some("hallucinatedHelper")
                && item["repo_graph_lookup"]
                    .as_str()
                    .unwrap_or_default()
                    .starts_with("no_defining_entity_named_")
                && item["severity"].as_str() == Some("warning")
        }),
        "{block}"
    );
    let warnings = validate_edit_warning_findings(&first);
    let warning = warnings
        .iter()
        .find(|finding| {
            finding["validation_rule_id"].as_str() == Some("CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL")
        })
        .unwrap_or_else(|| panic!("expected unresolved-local-call warning; got {first}"));
    // Budget compaction may shed the top-level `reason` (it survives in
    // evidence_items); assert over the serialized finding.
    let warning_text = warning.to_string();
    assert!(warning_text.contains("hallucinatedHelper"), "{warning}");
    assert_eq!(
        warning["proof_strength"].as_str(),
        Some("text_evidence"),
        "{warning}"
    );
    // The warning ceiling holds: no blocking, exit 0 even with
    // --fail-on-blocking, and the builtin call produced no warning.
    assert_eq!(
        first["must_fix_before_continuing"].as_bool(),
        Some(false),
        "{first}"
    );
    assert!(first.get("_cli_exit_code").is_none(), "{first}");
    assert!(
        warnings.iter().all(|finding| {
            let text = finding.to_string();
            !text.contains("console") && !text.contains("formatHelper")
        }),
        "builtin and candidates-exist references must never warn; got {warnings:?}"
    );
    assert!(
        escalated
            .iter()
            .all(|item| item["name"].as_str() != Some("formatHelper")),
        "candidates-exist references must not escalate; got {block}"
    );

    // Fixing the reference clears the warning and reports it resolved.
    write_cli_fixture_file(&repo, "src/service.js", clean_source);
    let second = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("validate-edit after fix");
    assert!(
        second["unresolved_references"]["resolved_count"]
            .as_u64()
            .unwrap_or_default()
            >= 1,
        "{second}"
    );
    assert!(
        validate_edit_warning_findings(&second)
            .iter()
            .all(|finding| {
                finding["validation_rule_id"]
                    .as_str()
                    .map(|rule| !rule.starts_with("CG_MVP3_REF_"))
                    .unwrap_or(true)
            }),
        "{second}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn restored_original_status_not_unknown_without_findings() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    let clean_source =
        "export function login(input, key) {\n  const value = input[key];\n  return value;\n}\n";
    write_cli_fixture_file(&repo, "src/service.js", clean_source);
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function login(input, key) {\n  const value = input[key];\n  hallucinatedOne(input);\n  hallucinatedTwo(value);\n  return value;\n}\n",
    );
    let bad = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("validate-edit with missing helpers");
    assert_eq!(bad["status"].as_str(), Some("warning"), "{bad}");
    assert_eq!(bad["blocking_error_count"].as_u64(), Some(0), "{bad}");

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "function hallucinatedOne(input) {\n  return input;\n}\n\nfunction hallucinatedTwo(input) {\n  return input;\n}\n\nexport function login(input, key) {\n  const value = input[key];\n  hallucinatedOne(input);\n  hallucinatedTwo(value);\n  return value;\n}\n",
    );
    let fixed = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("validate-edit after helper definitions");
    assert_eq!(fixed["status"].as_str(), Some("ok"), "{fixed}");
    assert_eq!(fixed["warning_count"].as_u64(), Some(0), "{fixed}");
    assert_eq!(fixed["unknown_count"].as_u64(), Some(0), "{fixed}");
    assert_eq!(fixed["blocking_error_count"].as_u64(), Some(0), "{fixed}");

    write_cli_fixture_file(&repo, "src/service.js", clean_source);
    let restored = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("validate-edit after restoring original source");
    assert_ne!(restored["status"].as_str(), Some("unknown"), "{restored}");
    assert!(matches!(
        restored["status"].as_str(),
        Some("ok") | Some("diagnostic_only")
    ));
    assert_eq!(
        restored["blocking_error_count"].as_u64(),
        Some(0),
        "{restored}"
    );
    assert_eq!(restored["warning_count"].as_u64(), Some(0), "{restored}");
    assert_eq!(restored["unknown_count"].as_u64(), Some(0), "{restored}");
    assert_eq!(
        restored["validation_state"]["open_blocker_count"].as_u64(),
        Some(0),
        "{restored}"
    );
    assert_eq!(
        restored["hard_interrupt_available"].as_bool(),
        Some(false),
        "{restored}"
    );

    let watch = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/service.js".to_string(),
            "--json".to_string(),
        ])
    })
    .expect("watch --once after restoring original source");
    assert_ne!(
        watch["validation_status"].as_str(),
        Some("unknown"),
        "{watch}"
    );
    assert_eq!(watch["validation_status"].as_str(), Some("ok"), "{watch}");
    assert!(matches!(
        watch["status"].as_str(),
        Some("updated") | Some("no_op")
    ));
    assert_eq!(
        watch["validation_warning_count"].as_u64(),
        Some(0),
        "{watch}"
    );
    assert_eq!(
        watch["validation_unknown_count"].as_u64(),
        Some(0),
        "{watch}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn query_unresolved_calls_reads_populated_lane_with_class_filter() {
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function login(input) {\n  console.log(input);\n  return hallucinatedHelper(input);\n}\n",
    );
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    let query_args = |extra: &[&str]| {
        let mut args = vec![
            "query".to_string(),
            "unresolved-calls".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--agent-json".to_string(),
        ];
        args.extend(extra.iter().map(ToString::to_string));
        args
    };

    // Proof-mode profile DB: the lane must be populated even though the
    // legacy heuristic-edge sidecar is empty.
    let unfiltered = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&query_args(&[]))
    })
    .expect("query unresolved-calls");
    assert_eq!(unfiltered["status"].as_str(), Some("ok"), "{unfiltered}");
    let lane = &unfiltered["unresolved_references"];
    let items = lane["items"].as_array().expect("lane items");
    assert!(
        items.iter().any(|item| {
            item["name"].as_str() == Some("hallucinatedHelper")
                && item["reference_class"].as_str() == Some("repo_local_candidate")
        }),
        "lane must surface the hallucinated reference; got {lane}"
    );
    assert_eq!(lane["not_graph_proof"].as_bool(), Some(true));

    // --class filters to one tier.
    let local_only = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&query_args(&["--class", "repo_local_candidate"]))
    })
    .expect("query unresolved-calls --class");
    let local_items = local_only["unresolved_references"]["items"]
        .as_array()
        .expect("filtered items");
    assert!(
        !local_items.is_empty()
            && local_items
                .iter()
                .all(|item| { item["reference_class"].as_str() == Some("repo_local_candidate") }),
        "{local_only}"
    );

    // --class for a different tier excludes the hallucinated reference.
    let builtin_only = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&query_args(&["--class", "builtin_or_std"]))
    })
    .expect("query unresolved-calls --class builtin");
    assert!(
        builtin_only["unresolved_references"]["items"]
            .as_array()
            .expect("builtin items")
            .iter()
            .all(|item| item["name"].as_str() != Some("hallucinatedHelper")),
        "{builtin_only}"
    );

    // --path filters by file.
    let other_path = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&query_args(&["--path", "src/other.js"]))
    })
    .expect("query unresolved-calls --path");
    assert_eq!(
        other_path["unresolved_references"]["rows"].as_u64(),
        Some(0),
        "{other_path}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn validate_edit_unresolved_findings_queryable() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    let clean_source = "export function login(input) {\n  return input;\n}\n";
    write_cli_fixture_file(&repo, "src/service.js", clean_source);

    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function login(input) {\n  missingFirstHelper(input);\n  return missingSecondHelper(input);\n}\n",
    );
    let validate = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("validate-edit bad edit");
    assert_eq!(validate["status"].as_str(), Some("warning"), "{validate}");
    assert_eq!(
        validate["must_fix_before_continuing"].as_bool(),
        Some(false),
        "{validate}"
    );
    assert!(validate.get("_cli_exit_code").is_none(), "{validate}");
    assert!(
        validate["unresolved_references"]["new_count"]
            .as_u64()
            .unwrap_or_default()
            >= 2,
        "{validate}"
    );
    let escalated_names = validate["unresolved_references"]["escalated"]
        .as_array()
        .expect("escalated unresolved references")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect::<BTreeSet<_>>();
    assert!(escalated_names.contains("missingFirstHelper"), "{validate}");
    assert!(
        escalated_names.contains("missingSecondHelper"),
        "{validate}"
    );

    let query_args = |extra: &[String]| {
        let mut args = vec![
            "query".to_string(),
            "unresolved-calls".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--agent-json".to_string(),
        ];
        args.extend(extra.iter().cloned());
        args
    };
    let assert_query_has_missing_refs = |value: &Value, label: &str| {
        assert_eq!(value["status"].as_str(), Some("ok"), "{label}: {value}");
        let lane = &value["unresolved_references"];
        assert_eq!(
            lane["not_graph_proof"].as_bool(),
            Some(true),
            "{label}: {value}"
        );
        assert!(
            lane["rows"].as_u64().unwrap_or_default() >= 2,
            "{label}: {value}"
        );
        let names = lane["items"]
            .as_array()
            .expect("lane items")
            .iter()
            .filter_map(|item| item["name"].as_str())
            .collect::<BTreeSet<_>>();
        assert!(names.contains("missingFirstHelper"), "{label}: {value}");
        assert!(names.contains("missingSecondHelper"), "{label}: {value}");
        assert!(
            lane["items"]
                .as_array()
                .expect("lane items")
                .iter()
                .all(|item| item["not_graph_proof"].as_bool() == Some(true)),
            "{label}: {value}"
        );
        assert!(
            lane["items"]
                .as_array()
                .expect("lane items")
                .iter()
                .all(|item| item["relation"].as_str() == Some("CALLS")
                    && item["definition_candidate_count"].as_u64() == Some(0)),
            "{label}: {value}"
        );
    };

    let unfiltered = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&query_args(&[]))
    })
    .expect("query unresolved-calls unfiltered");
    assert_query_has_missing_refs(&unfiltered, "unfiltered");

    let by_class = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&query_args(&[
            "--class".to_string(),
            "repo_local_candidate".to_string(),
        ]))
    })
    .expect("query unresolved-calls --class");
    assert_query_has_missing_refs(&by_class, "class");
    assert_eq!(
        by_class["unresolved_references"]["filters"]["class"].as_str(),
        Some("repo_local_candidate"),
        "{by_class}"
    );

    let by_relative_path = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&query_args(&[
            "--path".to_string(),
            "src/service.js".to_string(),
        ]))
    })
    .expect("query unresolved-calls --path relative");
    assert_query_has_missing_refs(&by_relative_path, "relative path");
    assert_eq!(
        by_relative_path["unresolved_references"]["filters"]["path"].as_str(),
        Some("src/service.js"),
        "{by_relative_path}"
    );

    let absolute_source = repo.join("src").join("service.js");
    let by_absolute_path = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&query_args(&[
            "--path".to_string(),
            path_string(&absolute_source),
        ]))
    })
    .expect("query unresolved-calls --path absolute");
    assert_query_has_missing_refs(&by_absolute_path, "absolute path");
    assert_eq!(
        by_absolute_path["unresolved_references"]["filters"]["path"].as_str(),
        Some("src/service.js"),
        "{by_absolute_path}"
    );
    assert_eq!(
        by_absolute_path["unresolved_references"]["filters"]["path_input"].as_str(),
        Some(path_string(&absolute_source).as_str()),
        "{by_absolute_path}"
    );

    write_cli_fixture_file(&repo, "src/service.js", clean_source);
    let fixed = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("validate-edit after fix");
    assert!(
        fixed["unresolved_references"]["resolved_count"]
            .as_u64()
            .unwrap_or_default()
            >= 2,
        "{fixed}"
    );
    let after_fix = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&query_args(&[]))
    })
    .expect("query unresolved-calls after fix");
    assert_eq!(
        after_fix["unresolved_references"]["rows"].as_u64(),
        Some(0),
        "{after_fix}"
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn query_unresolved_calls_help_and_parser_agree() {
    let usage = super::unresolved_calls_usage();
    assert!(usage.contains("--path <repo-relative-or-absolute-path>"));
    assert!(usage.contains("--class <reference_class>"));
    assert!(usage.contains("repo_local_candidate"));
    assert!(usage.contains("external_dependency"));
    assert!(usage.contains("builtin_or_std"));
    assert!(usage.contains("macro_or_codegen"));
    assert!(usage.contains("dynamic_or_computed"));
    assert!(usage.contains("does not accept a positional"));

    let parsed = super::parse_unresolved_calls_args(&[
        "--class".to_string(),
        "repo_local_candidate".to_string(),
        "--path".to_string(),
        "src/service.js".to_string(),
        "--limit".to_string(),
        "5".to_string(),
        "--agent-json".to_string(),
    ])
    .expect("parse documented unresolved-calls flags");
    assert_eq!(parsed.class_filter.as_deref(), Some("repo_local_candidate"));
    assert_eq!(parsed.path_filter.as_deref(), Some("src/service.js"));
    assert_eq!(parsed.limit, 5);

    let parsed_equals = super::parse_unresolved_calls_args(&[
        "--class=repo_local_candidate".to_string(),
        "--path=src/service.js".to_string(),
    ])
    .expect("parse equals-form unresolved-calls flags");
    assert_eq!(
        parsed_equals.class_filter.as_deref(),
        Some("repo_local_candidate")
    );
    assert_eq!(parsed_equals.path_filter.as_deref(), Some("src/service.js"));

    let invalid_class =
        super::parse_unresolved_calls_args(&["--class".to_string(), "local".to_string()])
            .expect_err("invalid class rejected");
    assert!(invalid_class.contains("invalid unresolved-calls --class value"));
    assert!(invalid_class.contains("repo_local_candidate"));

    let positional = super::parse_unresolved_calls_args(&["missingHelper".to_string()])
        .expect_err("positional unresolved-calls argument rejected");
    assert!(positional.contains("does not accept positional query arguments"));
    assert!(positional.contains("--path"));
    assert!(positional.contains("--class"));
}

#[test]
fn explain_query_plan_removed_from_compact() {
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function login(input) {\n  return hallucinatedHelper(input);\n}\n",
    );
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    let compact = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "unresolved-calls".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query unresolved-calls");

    assert_eq!(compact["status"].as_str(), Some("ok"), "{compact}");
    assert!(compact
        .pointer("/instrumentation/explain_query_plan")
        .is_none());
    assert_eq!(
        compact["instrumentation"]["explain_query_plan_omitted"].as_bool(),
        Some(true)
    );
    assert!(compact["unresolved_references"]["items"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn validate_edit_forward_fixture_matrix_python() {
    // §1.3.6 forward cases, Python first (the no-compiler niche gates first).
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(
        &repo,
        "src/tools.py",
        "def helper(value):\n    return value\n",
    );
    let clean_source = "def run(value):\n    return value\n";
    write_cli_fixture_file(&repo, "src/api.py", clean_source);
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    // Same-file unqualified call to a nonexistent fn + sibling-module call to
    // a nonexistent fn + builtin call (must stay quiet).
    write_cli_fixture_file(
        &repo,
        "src/api.py",
        "def run(value):\n    print(value)\n    tools.missing_sibling_fn(value)\n    return summarize_results(value)\n",
    );
    let validate_args = vec![
        "validate-edit".to_string(),
        "--repo".to_string(),
        path_string(&repo),
        "--changed".to_string(),
        "src/api.py".to_string(),
        "--agent-json".to_string(),
        "--fail-on-blocking".to_string(),
    ];
    let first =
        with_agent_use_data_root(&data_root, || super::run_agent_use_command(&validate_args))
            .expect("validate-edit python forward case");

    // The compact budget enforcer truncates the warnings LIST to one item
    // when the packet is over budget (§4.3.1, parked for 9.5.5), so the
    // multi-warning contract is carried by the summary counts and the
    // unresolved_references block (which keeps its top-3 escalated inline).
    assert_eq!(
        first
            .pointer("/validation_packet/summary_counts_by_rule_id/CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL")
            .and_then(Value::as_u64),
        Some(2),
        "{first}"
    );
    let warnings = validate_edit_warning_findings(&first);
    assert!(
        warnings.iter().any(|finding| {
            finding["validation_rule_id"].as_str() == Some("CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL")
        }),
        "at least the top escalated warning must survive the budget; got {first}"
    );
    let escalated = first["unresolved_references"]["escalated"]
        .as_array()
        .expect("escalated");
    for expected in ["summarize_results", "tools.missing_sibling_fn"] {
        assert!(
            escalated.iter().any(|item| {
                item["name"].as_str() == Some(expected)
                    && item["repo_graph_lookup"]
                        .as_str()
                        .unwrap_or_default()
                        .starts_with("no_defining_entity_named_")
            }),
            "expected escalated entry for `{expected}`; got {first}"
        );
    }
    assert!(
        warnings.iter().all(|finding| {
            !finding["reason"]
                .as_str()
                .unwrap_or_default()
                .contains("print")
        }),
        "builtin `print` must stay quiet; got {warnings:?}"
    );
    assert_eq!(first["must_fix_before_continuing"].as_bool(), Some(false));
    assert!(first.get("_cli_exit_code").is_none(), "{first}");
    assert!(
        first["unresolved_references"]["by_class"]["repo_local_candidate"]
            .as_u64()
            .unwrap_or_default()
            >= 2,
        "{first}"
    );

    // Fix → warnings clear, resolved_count reported.
    write_cli_fixture_file(&repo, "src/api.py", clean_source);
    let second =
        with_agent_use_data_root(&data_root, || super::run_agent_use_command(&validate_args))
            .expect("validate-edit python after fix");
    assert!(
        validate_edit_warning_findings(&second)
            .iter()
            .all(|finding| {
                finding["validation_rule_id"]
                    .as_str()
                    .map(|rule| !rule.starts_with("CG_MVP3_REF_"))
                    .unwrap_or(true)
            }),
        "{second}"
    );
    assert!(
        second["unresolved_references"]["resolved_count"]
            .as_u64()
            .unwrap_or_default()
            >= 2,
        "{second}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn validate_edit_forward_fixture_matrix_js_ts() {
    // §1.3.6 forward cases, JS/TS before Rust: same-file missing call,
    // imported/local missing call, builtin/dependency/dynamic negatives, and
    // post-fix resolution.
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(
        &repo,
        "package.json",
        "{\n  \"type\": \"module\",\n  \"dependencies\": {\"lodash\": \"^4.17.21\"}\n}\n",
    );
    write_cli_fixture_file(
        &repo,
        "src/local.js",
        "export function existingLocal(value) {\n  return value;\n}\n",
    );
    let clean_source = "export function run(value) {\n  return value;\n}\n";
    write_cli_fixture_file(&repo, "src/service.js", clean_source);
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "import { missingImportedHelper } from './local.js';\n\nexport function run(value, obj, key) {\n  console.log(value);\n  lodash.map([value], item => item);\n  obj[key](value);\n  missingImportedHelper(value);\n  return missingSameFile(value);\n}\n",
    );
    let validate_args = vec![
        "validate-edit".to_string(),
        "--repo".to_string(),
        path_string(&repo),
        "--changed".to_string(),
        "src/service.js".to_string(),
        "--agent-json".to_string(),
        "--fail-on-blocking".to_string(),
    ];
    let first =
        with_agent_use_data_root(&data_root, || super::run_agent_use_command(&validate_args))
            .expect("validate-edit js forward case");

    assert_eq!(
        first
            .pointer("/validation_packet/summary_counts_by_rule_id/CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL")
            .and_then(Value::as_u64),
        Some(2),
        "{first}"
    );
    let escalated = first["unresolved_references"]["escalated"]
        .as_array()
        .expect("escalated");
    for expected in ["missingImportedHelper", "missingSameFile"] {
        assert!(
            escalated.iter().any(|item| {
                item["name"].as_str() == Some(expected)
                    && item["proof_strength"].as_str() == Some("text_evidence")
                    && item["claimability"].as_str()
                        == Some("claimable_as_source_text_reference_only")
            }),
            "expected escalated entry for `{expected}`; got {first}"
        );
    }
    let warnings = validate_edit_warning_findings(&first);
    assert!(
        warnings.iter().all(|finding| {
            let reason = finding["reason"].as_str().unwrap_or_default();
            !reason.contains("console") && !reason.contains("lodash")
        }),
        "builtin/dependency references must stay quiet; got {warnings:?}"
    );
    assert_eq!(first["must_fix_before_continuing"].as_bool(), Some(false));
    assert!(first.get("_cli_exit_code").is_none(), "{first}");
    assert_eq!(
        first["unresolved_references"]["not_graph_proof"].as_bool(),
        Some(true),
        "{first}"
    );
    assert!(
        !first.to_string().contains("obj[key]"),
        "computed property calls must not become warning/blocking findings; got {first}"
    );

    write_cli_fixture_file(&repo, "src/service.js", clean_source);
    let second =
        with_agent_use_data_root(&data_root, || super::run_agent_use_command(&validate_args))
            .expect("validate-edit js after fix");
    assert!(
        second["unresolved_references"]["resolved_count"]
            .as_u64()
            .unwrap_or_default()
            >= 2,
        "{second}"
    );
    assert!(
        validate_edit_warning_findings(&second)
            .iter()
            .all(|finding| {
                finding["validation_rule_id"]
                    .as_str()
                    .map(|rule| !rule.starts_with("CG_MVP3_REF_"))
                    .unwrap_or(true)
            }),
        "{second}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn validate_edit_forward_fixture_matrix_rust() {
    // §1.3.6 Rust forward cases: cross-module qualified call, import of a
    // nonexistent symbol, macro and external-dependency negatives.
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(
        &repo,
        "Cargo.toml",
        "[package]\nname = \"fixture-crate\"\nversion = \"0.0.0\"\n\n[dependencies]\nserde_json = \"1\"\n",
    );
    write_cli_fixture_file(
        &repo,
        "src/auth.rs",
        "pub fn real_auth_fn() -> u32 {\n    1\n}\n",
    );
    let clean_source = "mod auth;\n\nfn main() {\n    let value = 1;\n    let _ = value;\n}\n";
    write_cli_fixture_file(&repo, "src/main.rs", clean_source);
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    write_cli_fixture_file(
        &repo,
        "src/main.rs",
        "mod auth;\nuse crate::auth::nonexistent_thing;\n\nfn main() {\n    let token = auth::revoke_token();\n    audit_login_attempt(token);\n    println!(\"{token:?}\");\n    let _ = serde_json::to_string(\"x\");\n}\n",
    );
    let validate_args = vec![
        "validate-edit".to_string(),
        "--repo".to_string(),
        path_string(&repo),
        "--changed".to_string(),
        "src/main.rs".to_string(),
        "--agent-json".to_string(),
        "--fail-on-blocking".to_string(),
    ];
    let result =
        with_agent_use_data_root(&data_root, || super::run_agent_use_command(&validate_args))
            .expect("validate-edit rust forward case");

    let warnings = validate_edit_warning_findings(&result);
    // Cross-module qualified call to a nonexistent fn (sibling src/auth.rs
    // makes auth:: repo-local) and same-file unqualified call both escalate.
    // The budget enforcer truncates the warnings LIST to one item over
    // budget (§4.3.1, 9.5.5 scope); the escalated block keeps all inline.
    let escalated = result["unresolved_references"]["escalated"]
        .as_array()
        .expect("escalated");
    for expected in ["auth::revoke_token", "audit_login_attempt"] {
        assert!(
            escalated.iter().any(|item| {
                item["name"].as_str() == Some(expected)
                    && item["repo_graph_lookup"]
                        .as_str()
                        .unwrap_or_default()
                        .starts_with("no_defining_entity_named_")
            }),
            "expected escalated entry for `{expected}`; got {result}"
        );
    }
    assert!(
        warnings.iter().any(|finding| {
            finding["validation_rule_id"]
                .as_str()
                .is_some_and(|rule| rule.starts_with("CG_MVP3_REF_NEW_UNRESOLVED_"))
        }),
        "at least the top escalated warning must survive the budget; got {result}"
    );
    // §1.3.6: `use crate::auth::nonexistent_thing` must escalate via the
    // imports rule family.
    assert!(
        escalated.iter().any(|item| {
            item["name"]
                .as_str()
                .unwrap_or_default()
                .contains("nonexistent_thing")
        }),
        "expected escalated entry for the nonexistent import; got {result}"
    );
    // Macro and declared-external-dependency calls never warn and never block.
    assert!(
        warnings.iter().all(|finding| {
            let reason = finding.to_string();
            !reason.contains("println") && !reason.contains("serde_json")
        }),
        "macro/external references must stay quiet; got {warnings:?}"
    );
    assert_eq!(result["must_fix_before_continuing"].as_bool(), Some(false));
    assert!(result.get("_cli_exit_code").is_none(), "{result}");
    let by_class = &result["unresolved_references"]["by_class"];
    assert!(
        by_class["repo_local_candidate"]
            .as_u64()
            .unwrap_or_default()
            >= 2,
        "{result}"
    );

    // Fix -> forward warnings clear and the lane reports resolved rows.
    write_cli_fixture_file(&repo, "src/main.rs", clean_source);
    let fixed =
        with_agent_use_data_root(&data_root, || super::run_agent_use_command(&validate_args))
            .expect("validate-edit rust after fix");
    assert!(
        fixed["unresolved_references"]["resolved_count"]
            .as_u64()
            .unwrap_or_default()
            >= 3,
        "{fixed}"
    );
    assert!(
        validate_edit_warning_findings(&fixed)
            .iter()
            .all(|finding| {
                finding["validation_rule_id"]
                    .as_str()
                    .map(|rule| !rule.starts_with("CG_MVP3_REF_"))
                    .unwrap_or(true)
            }),
        "{fixed}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn validate_edit_block_on_unresolved_local_promotes_escalated_warning() {
    // Holds ENV_TEST_LOCK: this test sets a process-wide policy env var.
    let _guard = lock_env_test();
    struct PolicyEnvGuard {
        previous: Option<std::ffi::OsString>,
    }
    impl PolicyEnvGuard {
        fn set() -> Self {
            let key = super::AGENT_USE_BLOCK_ON_UNRESOLVED_LOCAL_ENV;
            let previous = std::env::var_os(key);
            std::env::set_var(key, "1");
            Self { previous }
        }
    }
    impl Drop for PolicyEnvGuard {
        fn drop(&mut self) {
            let key = super::AGENT_USE_BLOCK_ON_UNRESOLVED_LOCAL_ENV;
            match self.previous.take() {
                Some(previous) => std::env::set_var(key, previous),
                None => std::env::remove_var(key),
            }
        }
    }

    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function login(input) {\n  return input;\n}\n",
    );
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function login(input) {\n  return hallucinatedHelper(input);\n}\n",
    );
    let _policy = PolicyEnvGuard::set();
    let result = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("validate-edit with promotion policy");
    assert_eq!(
        result["must_fix_before_continuing"].as_bool(),
        Some(true),
        "{result}"
    );
    assert_eq!(result["_cli_exit_code"].as_i64(), Some(2), "{result}");
    let blocking = result
        .pointer("/validation_packet/blocking_errors")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected blocking_errors in validation packet; got {result}"));
    assert!(
        blocking.iter().any(|finding| {
            finding["validation_rule_id"].as_str() == Some("CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL")
        }),
        "{result}"
    );
    assert!(
        result["unresolved_references"]["escalated"]
            .as_array()
            .expect("escalated")
            .iter()
            .any(|item| item["severity"].as_str() == Some("blocking")),
        "{result}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_validate_edit_crash_after_commit_replays_blocking_finding() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_hard_interrupt_fixture(&repo, 1);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    remove_agent_use_hard_interrupt_targets(&repo);

    // Run 1: the edit commits, then the validation pipeline dies post-commit
    // (the exact dogfood poisoning window).
    let crashed = {
        let _failpoint =
            BundleFailpointEnvGuard::set("agent_use_validation_panic_at_lifecycle_reads");
        with_agent_use_data_root(&data_root, || {
            super::run_agent_use_command(&validate_edit_args_for(&repo))
        })
        .expect("post-commit panic must yield a structured packet")
    };
    assert_eq!(crashed["status"].as_str(), Some("error"), "{crashed}");
    assert_eq!(crashed["validation_state"].as_str(), Some("incomplete"));

    // The journal survives in a failed state and status reports incomplete.
    let journal_path = super::agent_use_validation_journal_path(&profile);
    assert!(journal_path.exists(), "journal must survive the crash");
    let journal: Value =
        serde_json::from_str(&fs::read_to_string(&journal_path).expect("read journal"))
            .expect("journal json");
    assert!(
        journal["state"]
            .as_str()
            .unwrap_or_default()
            .starts_with("failed:"),
        "{journal}"
    );
    let status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("status");
    assert_eq!(
        status["validation_state"]["state"].as_str(),
        Some("incomplete"),
        "{status}"
    );

    // Run 2: identical command, no failpoint. The pending validation is
    // replayed from the journal and the lost blocking finding is recovered.
    let replayed = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("validate-edit after crash");
    assert_eq!(
        replayed["journal_replay"]["replayed"].as_bool(),
        Some(true),
        "{replayed}"
    );
    assert!(
        replayed["journal_replay"]["blocking_error_count"]
            .as_u64()
            .unwrap_or_default()
            >= 1,
        "{replayed}"
    );
    assert_eq!(
        replayed["status"].as_str(),
        Some("blocking_graph_error"),
        "{replayed}"
    );
    assert_eq!(replayed["must_fix_before_continuing"].as_bool(), Some(true));
    assert_eq!(replayed["_cli_exit_code"].as_i64(), Some(2));
    assert!(
        !journal_path.exists(),
        "journal must be cleared after a completed replay + run"
    );
    assert_eq!(
        replayed["validation_state"]["state"].as_str(),
        Some("blocked"),
        "{replayed}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_validate_edit_blocking_is_sticky_until_source_fixed() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_hard_interrupt_fixture(&repo, 1);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    remove_agent_use_hard_interrupt_targets(&repo);

    // Run 1: the break is detected and blocks.
    let first = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("first validate-edit");
    assert_eq!(
        first["status"].as_str(),
        Some("blocking_graph_error"),
        "{first}"
    );
    assert_eq!(first["_cli_exit_code"].as_i64(), Some(2));

    // Run 2: identical command on the UNCHANGED broken source. Before
    // MVP3.9.5c this returned ok (the broken graph became the baseline);
    // the persisted blocker must now be re-verified and re-emitted.
    let second = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("second validate-edit");
    assert_eq!(
        second["status"].as_str(),
        Some("blocking_graph_error"),
        "sticky blocker must keep blocking on unchanged broken source: {second}"
    );
    assert_eq!(second["must_fix_before_continuing"].as_bool(), Some(true));
    assert_eq!(second["_cli_exit_code"].as_i64(), Some(2));
    assert_eq!(
        second["validation_state"]["state"].as_str(),
        Some("blocked")
    );
    let reemitted = serde_json::to_string(&second["validation_packet"]).unwrap_or_default();
    assert!(
        reemitted.contains("persisted_open_blocker"),
        "re-emitted finding must be labeled with its persisted origin: {second}"
    );

    // Run 3: fix the source (restore the deleted target) -> blocker is
    // re-verified against the current graph, resolved, and cleared.
    write_agent_use_hard_interrupt_fixture(&repo, 1);
    let third = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("third validate-edit");
    assert_ne!(
        third["status"].as_str(),
        Some("blocking_graph_error"),
        "{third}"
    );
    assert!(third.get("_cli_exit_code").is_none(), "{third}");
    assert_eq!(third["validation_state"]["state"].as_str(), Some("ok"));
    let record_path = super::agent_use_validation_state_path(&profile);
    let record: Value =
        serde_json::from_str(&fs::read_to_string(&record_path).expect("read validation state"))
            .expect("validation state json");
    assert!(
        record["resolved_blockers_total"].as_u64().unwrap_or(0) >= 1,
        "{record}"
    );
    assert_eq!(record["open_blockers"].as_array().map(Vec::len), Some(0));

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_index_clears_validation_journal_and_rechecks_blockers() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_hard_interrupt_fixture(&repo, 1);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    remove_agent_use_hard_interrupt_targets(&repo);
    let crashed = {
        let _failpoint =
            BundleFailpointEnvGuard::set("agent_use_validation_panic_at_lifecycle_reads");
        with_agent_use_data_root(&data_root, || {
            super::run_agent_use_command(&validate_edit_args_for(&repo))
        })
        .expect("structured packet")
    };
    assert_eq!(crashed["status"].as_str(), Some("error"));
    let journal_path = super::agent_use_validation_journal_path(&profile);
    assert!(journal_path.exists());

    let reindex = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("reindex");
    assert_eq!(
        reindex["validation_journal_cleared"].as_bool(),
        Some(true),
        "{reindex}"
    );
    assert!(
        !journal_path.exists(),
        "a full reindex supersedes the pending validation journal"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_validate_edit_wall_budget_breach_is_labeled_never_silent_ok() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_hard_interrupt_fixture(&repo, 1);
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    // A real break that a full validation would catch as blocking.
    remove_agent_use_hard_interrupt_targets(&repo);

    // Wall budget 0: the deadline is already breached when validation
    // starts, so every skippable substage is skipped deterministically.
    let mut args = validate_edit_args_for(&repo);
    args.push("--max-validation-ms".to_string());
    args.push("0".to_string());
    let packet = with_agent_use_data_root(&data_root, || super::run_agent_use_command(&args))
        .expect("validate-edit with zero wall budget");

    assert_eq!(
        packet["validation_wall_bounded"].as_bool(),
        Some(true),
        "{packet}"
    );
    assert_ne!(
        packet["status"].as_str(),
        Some("ok"),
        "a wall-bounded validation must never report a silent pass: {packet}"
    );
    let serialized = serde_json::to_string(&packet).unwrap_or_default();
    assert!(
        serialized.contains("validation_wall_bounded"),
        "the bounded-unknown finding must name the wall budget: {packet}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_validate_edit_bounded_run_keeps_journal_and_rerun_recovers_blocking() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_hard_interrupt_fixture(&repo, 1);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    remove_agent_use_hard_interrupt_targets(&repo);

    // Run 1: wall budget 0 — the blocking detection is skipped (bounded),
    // the run is labeled, AND the journal must survive so the validation can
    // be replayed (without this, the bounded run absorbs the broken baseline
    // and the rerun is a silent ok — the gate-probe regression).
    let mut args = validate_edit_args_for(&repo);
    args.push("--max-validation-ms".to_string());
    args.push("0".to_string());
    let bounded = with_agent_use_data_root(&data_root, || super::run_agent_use_command(&args))
        .expect("bounded validate-edit");
    assert_eq!(bounded["validation_wall_bounded"].as_bool(), Some(true));
    assert_ne!(bounded["status"].as_str(), Some("ok"), "{bounded}");
    let journal_path = super::agent_use_validation_journal_path(&profile);
    assert!(
        journal_path.exists(),
        "bounded validation must keep the journal for replay"
    );
    assert_eq!(
        bounded["validation_state"]["state"].as_str(),
        Some("incomplete"),
        "{bounded}"
    );

    // Run 2: default wall. The pending validation replays from the journal,
    // re-derives the delta, and the lost blocking finding is recovered.
    let recovered = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("recovery validate-edit");
    assert_eq!(
        recovered["journal_replay"]["replayed"].as_bool(),
        Some(true),
        "{recovered}"
    );
    assert_eq!(
        recovered["status"].as_str(),
        Some("blocking_graph_error"),
        "the replayed validation must recover the blocking finding: {recovered}"
    );
    assert_eq!(recovered["_cli_exit_code"].as_i64(), Some(2));

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_validate_edit_low_delta_cap_is_labeled_graph_delta_bounded() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_hard_interrupt_fixture(&repo, 3);
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    // Removing 3 targets produces a multi-entry delta; cap of 1 omits
    // blocking-relevant entries.
    remove_agent_use_hard_interrupt_targets(&repo);
    let env_name = super::AGENT_USE_VALIDATION_MAX_DELTA_ITEMS_ENV;
    let _env = ProcessEnvGuard {
        name: env_name.to_string(),
        old: std::env::var_os(env_name),
    };
    std::env::set_var(env_name, "1");
    let packet = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("validate-edit with low delta cap");

    assert_ne!(
        packet["status"].as_str(),
        Some("ok"),
        "a bounded delta must never report a silent pass: {packet}"
    );
    let serialized = serde_json::to_string(&packet).unwrap_or_default();
    assert!(
        serialized.contains("graph_delta_bounded"),
        "blocking-relevant delta omission must be labeled graph_delta_bounded: {packet}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_validate_edit_exhausted_edge_budget_is_labeled_bounded() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_hard_interrupt_fixture(&repo, 2);
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    remove_agent_use_hard_interrupt_targets(&repo);
    let env_name = super::AGENT_USE_VALIDATION_MAX_EDGE_REVERIFICATIONS_ENV;
    let _env = ProcessEnvGuard {
        name: env_name.to_string(),
        old: std::env::var_os(env_name),
    };
    std::env::set_var(env_name, "0");
    let packet = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&validate_edit_args_for(&repo))
    })
    .expect("validate-edit with zero edge reverification budget");

    assert_ne!(
        packet["status"].as_str(),
        Some("ok"),
        "an exhausted reverification budget must never report a silent pass: {packet}"
    );
    let serialized = serde_json::to_string(&packet).unwrap_or_default();
    assert!(
        serialized.contains("edge_reverification_bounded"),
        "skipped edge reverifications must be labeled: {packet}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_watch_once_updates_external_profile_db_without_dot_codegraph() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
            &repo,
            "src/service.js",
            "export function oldAgentUseTarget() {\n  return \"agent-use-old-text\";\n}\n\nexport function callOldAgentUseTarget() {\n  return oldAgentUseTarget();\n}\n",
        );
    write_cli_fixture_file(
        &repo,
        "src/unchanged.js",
        "export function untouchedAgentUseHelper() {\n  return \"still-here\";\n}\n",
    );
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");

    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");
    assert_no_dot_codegraph_sqlite(&repo);

    let old_before = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "symbols".to_string(),
            "oldAgentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("query old symbol before update");
    assert!(
        old_before["result_count"].as_u64().unwrap_or_default() > 0,
        "{old_before:?}"
    );

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function newAgentUseTarget() {\n  return \"delta-ok\";\n}\n\nexport function callNewAgentUseTarget() {\n  return newAgentUseTarget();\n}\n",
    );

    let watch = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/service.js".to_string(),
            "--json".to_string(),
        ])
    })
    .expect("agent-use watch once");
    assert_eq!(watch["status"].as_str(), Some("updated"));
    assert_eq!(watch["command_namespace"].as_str(), Some("agent-use"));
    assert_eq!(watch["agent_use_command"].as_str(), Some("agent-use watch"));
    assert_eq!(watch["command"].as_str(), Some("watch"));
    assert_eq!(watch["subcommand"].as_str(), Some("once"));
    assert_eq!(watch["watch_mode"].as_str(), Some("once_changed"));
    assert_eq!(
        watch["delta_sync_phase"].as_str(),
        Some("real_time_delta_sync")
    );
    assert_eq!(watch["delta_sync_state"].as_str(), Some("updated"));
    assert_eq!(watch["delta_state"].as_str(), Some("updated"));
    assert_eq!(watch["external_db_used"].as_bool(), Some(true));
    assert_eq!(watch["auto_index_enabled"].as_bool(), Some(false));
    assert_eq!(watch["changed_paths"][0].as_str(), Some("src/service.js"));
    assert_eq!(watch["rejected_paths"].as_array().map(Vec::len), Some(0));
    assert_eq!(watch["no_op_paths"].as_array().map(Vec::len), Some(0));
    assert_eq!(watch["files_walked"].as_u64(), Some(1));
    assert_eq!(watch["files_read"].as_u64(), Some(1));
    assert_eq!(watch["files_hashed"].as_u64(), Some(1));
    assert_eq!(watch["files_parsed"].as_u64(), Some(1));
    assert_eq!(watch["files_indexed"].as_u64(), Some(1));
    assert_eq!(
        watch["files_metadata_unchanged"].as_u64(),
        Some(0),
        "only the changed file should be considered in this once update"
    );
    assert!(
        watch["facts_deleted"].as_u64().unwrap_or_default() > 0,
        "{watch:?}"
    );
    assert!(
        watch["facts_inserted"].as_u64().unwrap_or_default() > 0,
        "{watch:?}"
    );
    assert!(
        watch["entities_added"].as_u64().unwrap_or_default() > 0,
        "{watch:?}"
    );
    assert!(
        watch["entities_removed"].as_u64().unwrap_or_default() > 0,
        "{watch:?}"
    );
    assert_eq!(
        watch["graph_delta"]["schema_version"].as_str(),
        Some("mvp3_graph_delta_closure_rename_v1")
    );
    assert_eq!(watch["graph_delta"]["status"].as_str(), Some("complete"));
    assert_eq!(watch["graph_delta"]["claimable"].as_bool(), Some(true));
    assert_eq!(
        watch["graph_delta"]["snapshot_read_only"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["snapshot_bounded_to_changed_or_closure_files"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["entities_added_count"].as_u64(),
        watch["entities_added"].as_u64()
    );
    assert_eq!(
        watch["graph_delta"]["entities_removed_count"].as_u64(),
        watch["entities_removed"].as_u64()
    );
    assert_eq!(
        watch["graph_delta"]["edges_added_count"].as_u64(),
        watch["edges_added"].as_u64()
    );
    assert_eq!(
        watch["graph_delta"]["edges_removed_count"].as_u64(),
        watch["edges_removed"].as_u64()
    );
    assert_eq!(
        watch["graph_delta"]["edges_changed_count"].as_u64(),
        watch["edges_changed"].as_u64()
    );
    assert!(
        watch["graph_delta"]["edges_added_count"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "{watch:?}"
    );
    assert!(
        watch["graph_delta"]["edges_removed_count"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "{watch:?}"
    );
    assert_eq!(
        watch["graph_delta"]["source_spans_changed_reported"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["text_evidence_changed_reported"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["path_evidence_invalidated_reported"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["candidate_freshness_delta_reported"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["vector_freshness_delta_reported"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["proof_ladder_changes_reported"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["freshness_delta_not_graph_proof"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["access_vs_corrupt_classification_safe"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["stale_sidecars_not_used_as_fresh"].as_bool(),
        Some(true)
    );
    assert!(
        watch["graph_delta"]["source_spans_added_count"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "{watch:?}"
    );
    assert!(
        watch["graph_delta"]["source_spans_added"]
            .as_array()
            .expect("source span additions")
            .iter()
            .any(
                |entry| entry["associated_fact_kind"].as_str() == Some("entity")
                    && entry["new_span"].is_object()
                    && entry["claimability"]["graph_proof"].as_bool() == Some(true)
            ),
        "{watch:?}"
    );
    assert!(
        watch["graph_delta"]["text_evidence_changed"]
            .as_array()
            .expect("text evidence deltas")
            .iter()
            .all(|entry| entry["graph_proof"].as_bool() == Some(false)),
        "{watch:?}"
    );
    assert_eq!(
        watch["graph_delta"]["candidate_spool_invalidated_or_refreshed"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(
        watch["graph_delta"]["vector_chunks_invalidated"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert!(
        watch["graph_delta"]["proof_ladder_changes"]["text_evidence"].is_object(),
        "{watch:?}"
    );
    assert_eq!(
        watch["candidate_spool_invalidated_or_refreshed"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(
        watch["candidate_query_index_invalidated_or_refreshed"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(
        watch["vector_chunks_invalidated"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(
        watch["routing_handles_invalidated_or_not_applicable"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(
        watch["graph_delta"]["text_evidence_not_graph_entity_delta"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["text_candidate_evidence_not_graph_delta"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["source_navigation_only_not_graph_entity_delta"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["source_spans_present_for_claimable_entity_deltas"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["source_spans_present_for_claimable_edge_deltas"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["exactness_preserved"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["derived_edges_require_provenance"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["heuristic_unsupported_edges_not_overclaimed"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["endpoint_names_hydrated_where_available"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["same_name_targets_distinct"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["claim_boundaries_preserved"].as_bool(),
        Some(true)
    );
    assert_eq!(watch["graph_delta"]["public_claim"].as_bool(), Some(false));
    assert!(watch["validation_packet"].is_object(), "{watch:?}");
    assert_eq!(
        watch["validation_packet"]["packet_kind"].as_str(),
        Some("graph_validation_packet")
    );
    assert!(
        matches!(
            watch["validation_packet"]["status"].as_str(),
            Some("ok" | "diagnostic_only")
        ),
        "{watch:?}"
    );
    assert_eq!(
        watch["validation_packet"]["must_fix_before_continuing"].as_bool(),
        Some(false)
    );
    assert_eq!(
        watch["validation_packet"]["hard_interrupt_available"].as_bool(),
        Some(false)
    );
    assert_eq!(
        watch["validation_packet"]["summary_counts_by_rule_id"].is_object(),
        true
    );
    assert_eq!(
        watch["validation_packet"]["summary_counts_by_classification"].is_object(),
        true
    );
    assert!(watch["validation_packet"]["stale_unsafe_blockers"].is_array());
    assert!(watch["validation_packet"]["recommended_next_steps"].is_array());
    assert_eq!(
        watch["validation_summary_counts_by_classification"].is_object(),
        true
    );
    assert!(watch["validation_stale_unsafe_blockers"].is_array());
    assert!(watch["validation_recommended_next_steps"].is_array());
    assert_eq!(watch["hard_interrupt_available"].as_bool(), Some(false));
    assert_eq!(
        watch["hard_interrupt_not_implemented"].as_bool(),
        Some(false)
    );
    let compact_entities_added = watch["graph_delta"]["entities_added"]
        .as_array()
        .expect("added deltas");
    assert!(
        compact_entities_added
            .iter()
            .any(|entry| entry["name"].as_str() == Some("newAgentUseTarget")
                && entry["new"]["source_span"].is_object())
            || watch["graph_delta"]["entities_added_count"]
                .as_u64()
                .unwrap_or_default()
                > compact_entities_added.len() as u64,
        "{watch:?}"
    );
    let compact_entities_removed = watch["graph_delta"]["entities_removed"]
        .as_array()
        .expect("removed deltas");
    assert!(
        compact_entities_removed
            .iter()
            .any(|entry| entry["name"].as_str() == Some("oldAgentUseTarget"))
            || watch["graph_delta"]["entities_removed_count"]
                .as_u64()
                .unwrap_or_default()
                > compact_entities_removed.len() as u64,
        "{watch:?}"
    );
    assert!(
        watch["graph_delta"]["edges_added"]
            .as_array()
            .expect("added edge deltas")
            .iter()
            .any(|entry| entry["source_span"].is_object()
                && entry["relation_kind"].as_str().is_some()
                && entry["exactness_label"].as_str().is_some()
                && entry["source_endpoint"]["hydrated"].as_bool().is_some()),
        "{watch:?}"
    );
    assert!(
        watch["graph_delta"]["edges_removed"]
            .as_array()
            .expect("removed edge deltas")
            .iter()
            .any(|entry| entry["source_span"].is_object()
                && entry["relation_kind"].as_str().is_some()),
        "{watch:?}"
    );
    assert!(watch["graph_delta"]["relation_kind_counts"].is_object());
    assert!(watch["graph_delta"]["exactness_counts"].is_object());
    assert!(watch["graph_delta"]["derived_counts"].is_object());
    assert!(watch["graph_delta"]["source_role_counts"].is_object());
    assert!(watch["graph_delta"]["degraded_relation_classes"].is_object());
    assert!(watch["graph_delta"]["unsupported_relation_classes"].is_object());
    assert!(watch["graph_delta"]["summary"].is_object(), "{watch:?}");
    assert_eq!(
        watch["graph_delta"]["summary"]["entity_delta_count"].as_u64(),
        Some(
            watch["entities_added"].as_u64().unwrap_or_default()
                + watch["entities_removed"].as_u64().unwrap_or_default()
                + watch["entities_changed"].as_u64().unwrap_or_default()
        )
    );
    assert_eq!(watch["delta_packet_compact_default"].as_bool(), Some(true));
    assert_eq!(
        watch["graph_delta_packet_budget"]["critical_safety_fields_preserved"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta_packet_budget"]["full_graph_dump_default"].as_bool(),
        Some(false)
    );
    assert!(watch["graph_delta"]["packet_budget"].is_object());
    assert!(watch["graph_delta"]["timings"].is_object());
    for key in [
        "total_update_plus_delta_ms",
        "hot_path_update_ms",
        "snapshot_old_ms",
        "snapshot_new_ms",
        "diff_entities_ms",
        "diff_edges_ms",
        "diff_spans_ms",
        "diff_text_evidence_ms",
        "diff_sidecars_ms",
        "diff_closure_ms",
        "packet_serialize_ms",
        "delta_total_ms",
    ] {
        assert!(
            watch["timings"]["graph_delta"][key].as_u64().is_some(),
            "missing graph delta timing {key}: {watch:?}"
        );
    }
    assert!(
        watch["source_spans_added"].as_u64().unwrap_or_default() > 0,
        "{watch:?}"
    );
    assert_eq!(watch["old_graph_valid"].as_bool(), Some(true));
    assert_eq!(watch["new_graph_valid"].as_bool(), Some(true));
    assert_eq!(watch["old_db_preserved"].as_bool(), Some(true));
    assert_eq!(watch["temp_db_claimable"].as_bool(), Some(false));
    assert_eq!(
        watch["publish_safety"]["temp_db_claimable"].as_bool(),
        Some(false)
    );
    assert_eq!(watch["claimability"]["claimable"].as_bool(), Some(true));
    assert_eq!(watch["text_evidence_changed"].as_bool(), Some(true));
    assert_eq!(
        watch["routing_handles_invalidated"]["action"].as_str(),
        Some("dirty_file_cleanup")
    );
    assert!(watch["timings"].is_object());
    assert!(watch["recovery_commands"].is_array());
    assert_eq!(
        watch["db_path"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_eq!(
        watch["watch_db"]["actual_db_path_opened"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_eq!(
        watch["watch_db"]["lifecycle_status"].as_str(),
        Some("safe_to_write")
    );
    assert_eq!(
        watch["watch_db"]["operation_kind"].as_str(),
        Some("write_update")
    );
    assert_eq!(
        watch["watch_db"]["surface_name"].as_str(),
        Some("agent-use.watch.once")
    );
    assert_eq!(
        watch["publish_safety"]["strategy"].as_str(),
        Some("sqlite_transaction_delta_update")
    );
    assert!(watch["staged_availability"].is_object());
    assert!(watch["candidate_spool_status"].is_string());
    assert_eq!(watch["normal_dot_codegraph_mutated"].as_bool(), Some(false));
    assert_no_dot_codegraph_sqlite(&repo);

    let store = SqliteGraphStore::open_read_only(&profile.db_path).expect("open profile DB");
    let service_entities = store
        .list_entities_by_file("src/service.js")
        .expect("list service entities");
    assert!(
        service_entities
            .iter()
            .any(|entity| entity.source_span.is_some()),
        "updated service entities must preserve source spans: {service_entities:?}"
    );
    drop(store);

    let query = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "symbols".to_string(),
            "newAgentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("query updated symbol");
    assert_eq!(query["status"].as_str(), Some("ok"));
    assert!(
        query["result_count"].as_u64().unwrap_or_default() > 0,
        "{query:?}"
    );
    assert_eq!(
        query["db"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_no_dot_codegraph_sqlite(&repo);

    let old_after = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "symbols".to_string(),
            "oldAgentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("query old symbol after update");
    assert_eq!(
        old_after["result_count"].as_u64(),
        Some(0),
        "old symbol must be removed after changed-file update: {old_after:?}"
    );

    let new_text = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "text".to_string(),
            "newAgentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("query new text");
    assert!(
        new_text["result_count"].as_u64().unwrap_or_default() > 0,
        "{new_text:?}"
    );
    let old_text = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "text".to_string(),
            "oldAgentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("query old text");
    assert_eq!(
        old_text["result_count"].as_u64(),
        Some(0),
        "old text evidence must be removed after update: {old_text:?}"
    );

    let unchanged = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "symbols".to_string(),
            "untouchedAgentUseHelper".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("query unchanged symbol");
    assert!(
        unchanged["result_count"].as_u64().unwrap_or_default() > 0,
        "unchanged file facts should remain available: {unchanged:?}"
    );

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_watch_once_delta_packet_budget_compact_audit_and_noop() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");

    let old_source = (0..16)
        .map(|index| {
            format!("export function oldBudgetSymbol{index}() {{\n  return {index};\n}}\n")
        })
        .collect::<String>();
    write_cli_fixture_file(&repo, "src/budget.js", &old_source);

    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    let compact_source = (0..16)
        .map(|index| {
            format!("export function compactBudgetSymbol{index}() {{\n  return {index};\n}}\n")
        })
        .collect::<String>();
    write_cli_fixture_file(&repo, "src/budget.js", &compact_source);
    let compact = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/budget.js".to_string(),
            "--json".to_string(),
        ])
    })
    .expect("compact watch");

    assert_eq!(compact["status"].as_str(), Some("updated"));
    assert_eq!(
        compact["delta_packet_compact_default"].as_bool(),
        Some(true)
    );
    assert_eq!(compact["graph_delta_detail_mode"].as_str(), Some("compact"));
    assert_eq!(
        compact["graph_delta"]["omission"]["truncated"].as_bool(),
        Some(true),
        "{compact:?}"
    );
    assert!(
        compact["graph_delta"]["omitted_count"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "{compact:?}"
    );
    assert_eq!(
        compact["graph_delta"]["expansion_handle_count"].as_u64(),
        Some(1)
    );
    assert!(
        compact["graph_delta"]["truncated_sections"]
            .as_array()
            .is_some_and(|sections| !sections.is_empty()),
        "{compact:?}"
    );
    assert!(
        compact["graph_delta"]["entities_added"]
            .as_array()
            .is_some_and(|entries| entries.len() <= 10),
        "{compact:?}"
    );
    assert_eq!(
        compact["graph_delta_packet_budget"]["critical_safety_fields_preserved"].as_bool(),
        Some(true)
    );
    assert_eq!(
        compact["graph_delta_packet_budget"]["full_graph_dump_default"].as_bool(),
        Some(false)
    );
    assert!(compact["status"].is_string());
    assert!(compact["changed_paths"].is_array());
    assert!(compact["graph_delta"]["summary"].is_object());
    assert!(compact["claimability"].is_object());
    assert!(compact["watch_db"].is_object());
    assert!(compact["graph_delta"]["proof_ladder_changes"].is_object());
    assert!(compact["warnings"].is_array());
    for key in [
        "total_update_plus_delta_ms",
        "hot_path_update_ms",
        "snapshot_old_ms",
        "snapshot_new_ms",
        "diff_entities_ms",
        "diff_edges_ms",
        "diff_spans_ms",
        "diff_text_evidence_ms",
        "diff_sidecars_ms",
        "diff_closure_ms",
        "packet_serialize_ms",
        "delta_total_ms",
    ] {
        assert!(
            compact["timings"]["graph_delta"][key].as_u64().is_some(),
            "missing graph delta timing {key}: {compact:?}"
        );
    }

    let no_op = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/budget.js".to_string(),
            "--json".to_string(),
        ])
    })
    .expect("no-op watch");
    assert_eq!(no_op["status"].as_str(), Some("no_op"), "{no_op:?}");
    assert_eq!(no_op["files_read"].as_u64(), Some(0));
    assert_eq!(no_op["files_hashed"].as_u64(), Some(0));
    assert_eq!(no_op["files_parsed"].as_u64(), Some(0));
    assert_eq!(
        no_op["graph_delta"]["summary"]["entity_delta_count"].as_u64(),
        Some(0)
    );
    assert_eq!(
        no_op["graph_delta"]["summary"]["edge_delta_count"].as_u64(),
        Some(0)
    );
    assert_eq!(
        no_op["graph_delta"]["summary"]["source_span_delta_count"].as_u64(),
        Some(0)
    );

    let audit_source = (0..16)
        .map(|index| {
            format!("export function auditBudgetSymbol{index}() {{\n  return {index};\n}}\n")
        })
        .collect::<String>();
    write_cli_fixture_file(&repo, "src/budget.js", &audit_source);
    let audit = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/budget.js".to_string(),
            "--json".to_string(),
            "--audit-json".to_string(),
        ])
    })
    .expect("audit watch");

    assert_eq!(audit["status"].as_str(), Some("updated"));
    assert_eq!(audit["graph_delta_detail_mode"].as_str(), Some("audit"));
    assert_eq!(audit["delta_packet_compact_default"].as_bool(), Some(false));
    assert_eq!(
        audit["graph_delta"]["omission"]["truncated"].as_bool(),
        Some(false),
        "{audit:?}"
    );
    assert_eq!(audit["graph_delta"]["omitted_count"].as_u64(), Some(0));
    assert_eq!(
        audit["graph_delta"]["expansion_handle_count"].as_u64(),
        Some(0)
    );
    assert!(
        audit["graph_delta"]["entities_added"]
            .as_array()
            .is_some_and(|entries| entries.len() > 10),
        "{audit:?}"
    );
    assert_eq!(audit["normal_dot_codegraph_mutated"].as_bool(), Some(false));
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn cli_surface_outputs_validation_packet() {
    agent_use_watch_once_updates_external_profile_db_without_dot_codegraph();
}

#[test]
fn agent_use_watch_outputs_hard_interrupt_when_blocking() {
    let _guard = lock_env_test();
    let watch = run_agent_use_hard_interrupt_watch(1, &[]);

    assert_watch_hard_interrupt_blocking(&watch);
    assert_eq!(
        watch["graph_delta_detail_mode"].as_str(),
        Some("compact"),
        "{watch:?}"
    );
}

#[test]
fn agent_use_watch_no_interrupt_when_only_warnings() {
    let _guard = lock_env_test();
    let watch = run_agent_use_warning_only_watch();

    assert_eq!(watch["status"].as_str(), Some("updated"), "{watch:?}");
    assert_eq!(watch["validation_status"].as_str(), Some("warning"));
    assert_eq!(
        watch["validation_must_fix_before_continuing"].as_bool(),
        Some(false)
    );
    assert!(
        watch["validation_warning_count"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "{watch:?}"
    );
    assert_eq!(watch["validation_blocking_error_count"].as_u64(), Some(0));
    assert_eq!(watch["hard_interrupt_available"].as_bool(), Some(false));
    assert!(watch["hard_interrupt"].is_null(), "{watch:?}");
    assert_eq!(
        watch["validation_packet"]["hard_interrupt_available"].as_bool(),
        Some(false)
    );
    assert!(watch["validation_packet"]["hard_interrupt"].is_null());
}

#[test]
fn agent_use_watch_no_interrupt_when_unknown_degraded_diagnostic() {
    let _guard = lock_env_test();
    let watch = run_agent_use_over_budget_degraded_watch();

    assert_eq!(watch["status"].as_str(), Some("degraded"), "{watch:?}");
    assert_eq!(watch["delta_state"].as_str(), Some("degraded"));
    assert_eq!(watch["closure_budget_hit"].as_bool(), Some(true));
    assert_eq!(
        watch["graph_delta"]["closure_budget_hit_degraded"].as_bool(),
        Some(true)
    );
    assert!(
        watch["degraded_relation_classes"]
            .as_array()
            .is_some_and(|classes| !classes.is_empty()),
        "{watch:?}"
    );
    assert_eq!(watch["hard_interrupt_available"].as_bool(), Some(false));
    assert!(watch["hard_interrupt"].is_null(), "{watch:?}");
    assert_eq!(
        watch["validation_packet"]["hard_interrupt_available"].as_bool(),
        Some(false)
    );
    assert!(watch["validation_packet"]["hard_interrupt"].is_null());
}

#[test]
fn agent_use_watch_compact_default_includes_hard_interrupt() {
    let _guard = lock_env_test();
    let watch = run_agent_use_hard_interrupt_watch(14, &[]);
    assert_watch_hard_interrupt_blocking(&watch);

    let interrupt = &watch["hard_interrupt"];
    assert_eq!(watch["graph_delta_detail_mode"].as_str(), Some("compact"));
    assert_eq!(watch["delta_packet_compact_default"].as_bool(), Some(true));
    assert!(
        interrupt["error_count"].as_u64().unwrap_or_default()
            > interrupt["errors"]
                .as_array()
                .expect("interrupt errors")
                .len() as u64,
        "{watch:?}"
    );
    assert!(
        interrupt["omitted_count"].as_u64().unwrap_or_default() > 0,
        "{watch:?}"
    );
    assert!(interrupt["errors"][0].get("evidence_items").is_none());
    assert!(interrupt["errors"][0].get("eligibility").is_none());
    assert!(interrupt["expansion_handles"]
        .as_array()
        .expect("interrupt expansion handles")
        .iter()
        .any(|handle| handle["handle"].as_str() == Some("hard_interrupt_packet:full")));
}

#[test]
fn agent_use_watch_explain_includes_interrupt_detail() {
    let _guard = lock_env_test();
    let watch = run_agent_use_hard_interrupt_watch(2, &["--explain"]);
    assert_watch_hard_interrupt_blocking(&watch);

    let interrupt = &watch["hard_interrupt"];
    assert_eq!(watch["graph_delta_detail_mode"].as_str(), Some("explain"));
    assert_eq!(watch["delta_packet_compact_default"].as_bool(), Some(false));
    assert_eq!(
        interrupt["errors"]
            .as_array()
            .map(Vec::len)
            .map(|len| len as u64),
        interrupt["error_count"].as_u64()
    );
    assert!(interrupt["errors"][0]["evidence_items"].is_array());
    assert!(interrupt["errors"][0]["eligibility"].is_object());
    assert!(interrupt["errors"][0]["provenance"].is_object());
    assert!(interrupt["errors"][0]["fix_hint"].is_object());
}

#[test]
fn agent_use_persistent_watch_synthetic_events_use_once_delta_contract() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);

    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    write_cli_fixture_file(
        &repo,
        "src/service.ts",
        "export function persistentWatchNewSymbol() {\n  return \"persistent-ok\";\n}\n",
    );
    let watch = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
            "--debounce-ms".to_string(),
            "0".to_string(),
            "--max-updates".to_string(),
            "1".to_string(),
            "--test-event".to_string(),
            "src/service.ts".to_string(),
            "--test-event".to_string(),
            "src/service.ts".to_string(),
        ])
    })
    .expect("persistent watch synthetic event");

    assert_eq!(watch["status"].as_str(), Some("stopped"));
    assert_eq!(watch["watch_mode"].as_str(), Some("persistent"));
    assert_eq!(watch["uses_once_delta_engine"].as_bool(), Some(true));
    assert_eq!(watch["writer_queue_serialized"].as_bool(), Some(true));
    assert_eq!(watch["queue_depth"].as_u64(), Some(0));
    assert_eq!(watch["events_seen"].as_u64(), Some(2));
    assert_eq!(watch["coalesced_count"].as_u64(), Some(1));
    assert_eq!(watch["updates_attempted"].as_u64(), Some(1));
    assert_eq!(watch["updates_succeeded"].as_u64(), Some(1));
    assert_eq!(watch["last_update_state"].as_str(), Some("updated"));
    assert_eq!(
        watch["lock_state"]["writer_queue_serialized"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["lock_state"]["unrelated_repo_blocking"].as_bool(),
        Some(false)
    );
    assert_eq!(watch["lock_state"]["lock_retry_count"].as_u64(), Some(0));
    assert_eq!(
        watch["update_queue_state"]["updates_attempted"].as_u64(),
        Some(1)
    );
    assert_eq!(
        watch["update_queue_state"]["updates_succeeded"].as_u64(),
        Some(1)
    );
    assert_eq!(
        watch["update_queue_state"]["unrelated_repo_blocking"].as_bool(),
        Some(false)
    );
    assert_eq!(
        watch["last_update_summary"]["watch_mode"].as_str(),
        Some("once_changed")
    );
    assert_eq!(watch["last_update_summary"]["files_read"].as_u64(), Some(1));
    assert_eq!(
        watch["last_update_summary"]["files_parsed"].as_u64(),
        Some(1)
    );
    assert_eq!(
        watch["last_update_summary"]["external_db_used"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["last_update_summary"]["temp_db_claimable"].as_bool(),
        Some(false)
    );
    assert_eq!(watch["normal_dot_codegraph_mutated"].as_bool(), Some(false));
    assert_no_dot_codegraph_sqlite(&repo);

    assert!(agent_use_query_count(&data_root, &repo, "symbols", "persistentWatchNewSymbol") > 0);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_persistent_watch_missing_and_stale_db_do_not_auto_index() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let missing_repo = temp_repo();
    write_agent_use_context_fixture(&missing_repo);
    let missing_profile =
        super::resolve_agent_use_profile_with_data_root(&missing_repo, &data_root)
            .expect("missing profile");

    let missing = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&missing_repo),
            "--json".to_string(),
            "--max-updates".to_string(),
            "1".to_string(),
        ])
    })
    .expect("missing persistent watch");
    assert_eq!(missing["status"].as_str(), Some("not_indexed"));
    assert_eq!(missing["auto_index_enabled"].as_bool(), Some(false));
    assert_eq!(missing["queue_depth"].as_u64(), Some(0));
    assert_eq!(missing["updates_attempted"].as_u64(), Some(0));
    assert!(!missing_profile.profile_root.exists());
    assert_no_dot_codegraph_sqlite(&missing_repo);

    let stale_repo = temp_repo();
    write_agent_use_context_fixture(&stale_repo);
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&stale_repo),
            "--json".to_string(),
        ])
    })
    .expect("index stale repo");
    let stale_profile = super::resolve_agent_use_profile_with_data_root(&stale_repo, &data_root)
        .expect("stale profile");
    {
        let connection = Connection::open(&stale_profile.db_path).expect("open stale DB");
        connection
            .execute(
                "UPDATE codegraph_db_passport SET last_run_status = 'interrupted' WHERE id = 1",
                [],
            )
            .expect("mark stale");
    }
    let stale = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&stale_repo),
            "--json".to_string(),
            "--max-updates".to_string(),
            "1".to_string(),
        ])
    })
    .expect("stale persistent watch");
    assert_eq!(stale["auto_index_enabled"].as_bool(), Some(false));
    assert_eq!(stale["queue_depth"].as_u64(), Some(0));
    assert_eq!(stale["updates_attempted"].as_u64(), Some(0));
    assert_eq!(stale["last_update_state"].as_str(), Some("blocked"));
    assert_ne!(stale["status"].as_str(), Some("stopped"));
    assert_no_dot_codegraph_sqlite(&stale_repo);

    remove_dir_all_with_retry(&missing_repo, "cleanup missing repo");
    remove_dir_all_with_retry(&stale_repo, "cleanup stale repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_persistent_watch_ignored_and_many_change_events_are_safe() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, ".gitignore", "generated/\n");
    write_agent_use_context_fixture(&repo);
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    write_cli_fixture_file(
        &repo,
        "generated/ignored.ts",
        "export function ignoredPersistentWatchSymbol() { return 'ignored'; }\n",
    );
    let ignored = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
            "--debounce-ms".to_string(),
            "0".to_string(),
            "--max-updates".to_string(),
            "1".to_string(),
            "--test-event".to_string(),
            "generated/ignored.ts".to_string(),
        ])
    })
    .expect("ignored persistent event");
    assert_eq!(ignored["status"].as_str(), Some("stopped"));
    assert_eq!(ignored["last_update_state"].as_str(), Some("ready"));
    assert_eq!(
        ignored["last_update_summary"]["status"].as_str(),
        Some("no_op")
    );
    assert_eq!(
        ignored["last_update_summary"]["reason"].as_str(),
        Some("ignored_path_no_graph_changes")
    );
    assert_eq!(
        agent_use_query_count(&data_root, &repo, "symbols", "ignoredPersistentWatchSymbol"),
        0
    );

    write_cli_fixture_file(
        &repo,
        "src/service.ts",
        "export function branchSwitchA() { return 'a'; }\n",
    );
    write_cli_fixture_file(
        &repo,
        "src/main.ts",
        "export function branchSwitchB() { return 'b'; }\n",
    );
    let many = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
            "--debounce-ms".to_string(),
            "0".to_string(),
            "--max-updates".to_string(),
            "1".to_string(),
            "--max-batch-paths".to_string(),
            "1".to_string(),
            "--test-event".to_string(),
            "src/service.ts".to_string(),
            "--test-event".to_string(),
            "src/main.ts".to_string(),
        ])
    })
    .expect("many-change persistent event");
    assert_eq!(many["status"].as_str(), Some("degraded"));
    assert_eq!(many["last_update_state"].as_str(), Some("degraded"));
    assert_eq!(
        many["lock_state"]["writer_queue_serialized"].as_bool(),
        Some(true)
    );
    assert_eq!(
        many["update_queue_state"]["last_update_summary"]["status"].as_str(),
        Some("degraded")
    );
    assert_eq!(
        many["update_queue_state"]["last_update_summary"]["old_db_preserved"].as_bool(),
        Some(true)
    );
    assert_eq!(
        many["last_update_summary"]["reason"].as_str(),
        Some("too_many_changes_branch_switch_suspected")
    );
    assert_eq!(
        many["last_update_summary"]["no_full_repo_fallback"].as_bool(),
        Some(true)
    );
    assert_eq!(many["updates_succeeded"].as_u64(), Some(0));
    assert_eq!(many["normal_dot_codegraph_mutated"].as_bool(), Some(false));
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_watch_once_graph_delta_reports_exact_dependency_closure_summary() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function closureTarget() {\n  return \"old\";\n}\n",
    );
    write_cli_fixture_file(
        &repo,
        "src/consumer.js",
        "import { closureTarget } from './service';\n\
         export function runClosureConsumer() {\n  return closureTarget();\n}\n",
    );

    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function closureTarget() {\n  return \"new\";\n}\n",
    );
    let watch = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/service.js".to_string(),
            "--json".to_string(),
        ])
    })
    .expect("agent-use exact closure watch");

    assert_eq!(watch["status"].as_str(), Some("updated"));
    assert_eq!(watch["no_full_repo_fallback"].as_bool(), Some(true));
    assert_eq!(
        watch["graph_delta"]["schema_version"].as_str(),
        Some("mvp3_graph_delta_closure_rename_v1")
    );
    assert_eq!(
        watch["graph_delta"]["closure_delta_supported"].as_bool(),
        Some(true),
        "{watch:?}"
    );
    assert_eq!(
        watch["graph_delta"]["no_silent_full_repo_fallback"].as_bool(),
        Some(true)
    );
    assert!(
        watch["graph_delta"]["closure_delta_summary"]["closure_files_considered"]
            .as_array()
            .expect("closure files")
            .iter()
            .any(|path| path.as_str() == Some("src/consumer.js")),
        "{watch:?}"
    );
    assert!(
        watch["graph_delta"]["closure_delta_summary"]["closure_relation_classes"]
            .as_array()
            .expect("closure relation classes")
            .iter()
            .any(|class| class.as_str() == Some("direct_static_importer")
                || class.as_str() == Some("deleted_or_changed_callable_reference")),
        "{watch:?}"
    );
    assert_eq!(
        watch["graph_delta"]["closure_delta_summary"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(
        watch["graph_delta"]["unsupported_relation_unknown_not_proof"].as_bool(),
        Some(true)
    );
    assert_eq!(watch["normal_dot_codegraph_mutated"].as_bool(), Some(false));
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_watch_once_over_budget_dependency_closure_reports_degraded() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function targetForBudget() {\n  return \"old\";\n}\n",
    );
    for index in 0..3 {
        write_cli_fixture_file(
            &repo,
            &format!("src/consumer{index}.js"),
            "import { targetForBudget } from './service';\n\
                 export function run() {\n  return targetForBudget();\n}\n",
        );
    }

    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function targetForBudget() {\n  return \"new\";\n}\n",
    );
    let watch = with_process_env_var("CODEGRAPH_RTDS_CLOSURE_MAX_DIRTY_FILES", "1", || {
        with_agent_use_data_root(&data_root, || {
            super::run_agent_use_command(&[
                "watch".to_string(),
                "--repo".to_string(),
                path_string(&repo),
                "--once".to_string(),
                "--changed".to_string(),
                "src/service.js".to_string(),
                "--json".to_string(),
            ])
        })
    })
    .expect("agent-use over-budget watch");

    assert_eq!(watch["status"].as_str(), Some("degraded"));
    assert_eq!(watch["delta_state"].as_str(), Some("degraded"));
    assert_eq!(watch["closure_budget_hit"].as_bool(), Some(true));
    assert_eq!(watch["no_full_repo_fallback"].as_bool(), Some(true));
    assert_eq!(
        watch["graph_delta"]["closure_budget_hit"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["closure_budget_hit_degraded"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["graph_delta"]["closure_delta_summary"]["status"].as_str(),
        Some("degraded")
    );
    assert_eq!(
        watch["graph_delta"]["closure_delta_summary"]["no_silent_full_repo_fallback"].as_bool(),
        Some(true)
    );
    assert_eq!(watch["files_walked"].as_u64(), Some(1));
    assert!(watch["degraded_relation_classes"]
        .as_array()
        .is_some_and(|classes| !classes.is_empty()));
    assert!(watch["manual_full_index_recommendation"]
        .as_str()
        .is_some_and(|recommendation| recommendation.contains("agent-use index --fresh")));
    assert_eq!(watch["normal_dot_codegraph_mutated"].as_bool(), Some(false));
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_watch_once_missing_db_reports_unavailable_without_fallback() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");

    let watch = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/service.ts".to_string(),
            "--json".to_string(),
        ])
    })
    .expect("missing DB watch once");
    assert_eq!(watch["status"].as_str(), Some("not_indexed"));
    assert_eq!(watch["claimable"].as_bool(), Some(false));
    assert_eq!(watch["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(watch["delta_sync_state"].as_str(), Some("blocked"));
    assert_eq!(watch["delta_state"].as_str(), Some("blocked"));
    assert_eq!(watch["external_db_used"].as_bool(), Some(true));
    assert_eq!(watch["auto_index_enabled"].as_bool(), Some(false));
    assert_eq!(watch["changed_paths"][0].as_str(), Some("src/service.ts"));
    assert_eq!(watch["old_graph_valid"].as_bool(), Some(false));
    assert_eq!(watch["new_graph_valid"].as_bool(), Some(false));
    assert_eq!(watch["old_db_preserved"].as_bool(), Some(true));
    assert_eq!(watch["temp_db_claimable"].as_bool(), Some(false));
    assert_eq!(watch["claimability"]["claimable"].as_bool(), Some(false));
    assert_eq!(watch["hard_interrupt_available"].as_bool(), Some(false));
    assert!(watch["hard_interrupt"].is_null(), "{watch:?}");
    assert_eq!(watch["files_read"].as_u64(), Some(0));
    assert_eq!(watch["facts_inserted"].as_u64(), Some(0));
    assert!(watch["recovery_commands"].is_array());
    assert_eq!(
        watch["watch_db"]["requested_db_path"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_eq!(watch["watch_db"]["safe_to_write"].as_bool(), Some(false));
    assert_eq!(
        watch["watch_db"]["surface_name"].as_str(),
        Some("agent-use.watch.once")
    );
    assert!(
        !profile.profile_root.exists(),
        "missing watch preflight must not create the production profile parent"
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_watch_once_rejects_stale_and_foreign_profile_dbs() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let stale_repo = temp_repo();
    write_agent_use_context_fixture(&stale_repo);
    let stale_profile = super::resolve_agent_use_profile_with_data_root(&stale_repo, &data_root)
        .expect("stale profile");

    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&stale_repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index stale repo");
    let valid_foreign_source_db = data_root.join("valid-foreign-source.sqlite");
    fs::copy(&stale_profile.db_path, &valid_foreign_source_db)
        .expect("copy valid foreign source DB");
    {
        let connection = Connection::open(&stale_profile.db_path).expect("open stale DB");
        connection
            .execute(
                "UPDATE codegraph_db_passport SET last_run_status = 'interrupted' WHERE id = 1",
                [],
            )
            .expect("mark stale");
    }
    write_cli_fixture_file(
        &stale_repo,
        "src/service.ts",
        "export function staleWatchChange() { return 'blocked'; }\n",
    );
    let stale_watch = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&stale_repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/service.ts".to_string(),
            "--json".to_string(),
        ])
    })
    .expect("stale watch");
    assert_eq!(stale_watch["claimable"].as_bool(), Some(false));
    assert_eq!(stale_watch["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(stale_watch["delta_state"].as_str(), Some("blocked"));
    assert_eq!(stale_watch["auto_index_enabled"].as_bool(), Some(false));
    assert_eq!(stale_watch["temp_db_claimable"].as_bool(), Some(false));
    assert_eq!(
        stale_watch["hard_interrupt_available"].as_bool(),
        Some(false)
    );
    assert!(stale_watch["hard_interrupt"].is_null(), "{stale_watch:?}");
    assert_no_dot_codegraph_sqlite(&stale_repo);

    let foreign_repo = temp_repo();
    write_agent_use_context_fixture(&foreign_repo);
    let foreign_profile =
        super::resolve_agent_use_profile_with_data_root(&foreign_repo, &data_root)
            .expect("foreign profile");
    fs::create_dir_all(&foreign_profile.profile_root).expect("create foreign profile parent");
    fs::copy(&valid_foreign_source_db, &foreign_profile.db_path).expect("copy foreign DB");
    let foreign_watch = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&foreign_repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/service.ts".to_string(),
            "--json".to_string(),
        ])
    })
    .expect("foreign watch");
    assert_eq!(foreign_watch["claimable"].as_bool(), Some(false));
    assert_eq!(foreign_watch["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(foreign_watch["delta_state"].as_str(), Some("blocked"));
    assert_eq!(foreign_watch["auto_index_enabled"].as_bool(), Some(false));
    assert_eq!(foreign_watch["temp_db_claimable"].as_bool(), Some(false));
    assert_eq!(
        foreign_watch["hard_interrupt_available"].as_bool(),
        Some(false)
    );
    assert!(
        foreign_watch["hard_interrupt"].is_null(),
        "{foreign_watch:?}"
    );
    assert_eq!(
        foreign_watch["db_problem_kind"].as_str(),
        Some("repo_root_mismatch")
    );
    assert_no_dot_codegraph_sqlite(&foreign_repo);

    remove_dir_all_with_retry(&stale_repo, "cleanup stale repo");
    remove_dir_all_with_retry(&foreign_repo, "cleanup foreign repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_watch_once_failpoint_preserves_old_good_db_and_recovery_state() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function oldFailpointSymbol() {\n  return \"old\";\n}\n",
    );
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");
    assert!(db_has_symbol(&profile.db_path, "oldFailpointSymbol"));

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function newFailpointSymbol() {\n  return \"new\";\n}\n",
    );
    let failpoint = BundleFailpointEnvGuard::set("incremental_during_entity_insert");
    let error = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/service.js".to_string(),
            "--json".to_string(),
        ])
    })
    .expect_err("failpoint should abort once update");
    drop(failpoint);
    assert!(
        error.contains("incremental_during_entity_insert"),
        "{error}"
    );

    let store = SqliteGraphStore::open_read_only(&profile.db_path).expect("open old DB");
    store
        .full_integrity_gate()
        .expect("old DB remains valid after failed watch");
    drop(store);
    assert!(db_has_symbol(&profile.db_path, "oldFailpointSymbol"));
    assert!(!db_has_symbol(&profile.db_path, "newFailpointSymbol"));
    let publish_state = super::agent_use_publish_state_json(&profile);
    assert_eq!(publish_state["status"].as_str(), Some("interrupted"));
    assert_eq!(
        publish_state["temp_db_claimability"].as_str(),
        Some("never_claimable")
    );
    assert_eq!(
        publish_state["state"]["temp_db_claimability"].as_str(),
        Some("never_claimable")
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_watch_once_post_commit_interrupt_reports_recovered_complete_db() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function oldPostCommitSymbol() {\n  return \"old\";\n}\n",
    );
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function newPostCommitSymbol() {\n  return \"new\";\n}\n",
    );
    let failpoint = BundleFailpointEnvGuard::set(
        super::AGENT_USE_WATCH_AFTER_DELTA_COMMIT_BEFORE_STATE_CLEAR_FAILPOINT,
    );
    let error = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--once".to_string(),
            "--changed".to_string(),
            "src/service.js".to_string(),
            "--json".to_string(),
        ])
    })
    .expect_err("post-commit failpoint should leave inspectable interrupted state");
    drop(failpoint);
    assert!(
        error.contains(super::AGENT_USE_WATCH_AFTER_DELTA_COMMIT_BEFORE_STATE_CLEAR_FAILPOINT),
        "{error}"
    );

    let store = SqliteGraphStore::open_read_only(&profile.db_path).expect("open DB");
    store
        .full_integrity_gate()
        .expect("post-commit DB remains valid");
    drop(store);
    assert!(!db_has_symbol(&profile.db_path, "oldPostCommitSymbol"));
    assert!(db_has_symbol(&profile.db_path, "newPostCommitSymbol"));

    let status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("status after post-commit interrupt");
    assert_eq!(status["claimable"].as_bool(), Some(true));
    assert_eq!(
        status["publish_state"]["status"].as_str(),
        Some("interrupted")
    );
    assert_json_array_contains(&status, "safety_labels", "interrupted");
    assert_json_array_contains(&status, "safety_labels", "recovered");
    assert_json_array_contains(&status, "safety_labels", "publishing");
    assert_eq!(
        status["normal_dot_codegraph_mutated"].as_bool(),
        Some(false)
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_watch_once_file_lifecycle_cases_use_external_profile_db() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(&repo, ".gitignore", "generated/\n");
    write_cli_fixture_file(
        &repo,
        "src/delete_me.js",
        "export function deletedLifecycleSymbol() {\n  return \"delete\";\n}\n",
    );
    write_cli_fixture_file(
        &repo,
        "src/rename_old.js",
        "export function renamedLifecycleSymbol() {\n  return \"rename\";\n}\n",
    );
    write_cli_fixture_file(
        &repo,
        "src/rename_edit_old.js",
        "export function renameEditOldLifecycleSymbol() {\n  return \"rename-edit-old\";\n}\n",
    );
    write_cli_fixture_file(
        &repo,
        "src/duplicate_a.js",
        "export function duplicateLifecycleSymbol() {\n  return \"same\";\n}\n",
    );
    write_cli_fixture_file(
        &repo,
        "src/casefile.js",
        "export function oldCaseLifecycleSymbol() {\n  return \"old-case\";\n}\n",
    );
    write_cli_fixture_file(
        &repo,
        "src/atomic.js",
        "export function oldAtomicLifecycleSymbol() {\n  return \"old-atomic\";\n}\n",
    );
    write_cli_fixture_file(
        &repo,
        "docs/delete_me.md",
        "deletedMarkdownLifecycleUniqueToken\n",
    );
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    run_agent_use_test_command(
        &data_root,
        &["index", "--repo", path_string(&repo).as_str(), "--json"],
    )
    .expect("agent-use index");
    assert_no_dot_codegraph_sqlite(&repo);

    write_cli_fixture_file(
        &repo,
        "src/added.js",
        "export function addedLifecycleSymbol() {\n  return \"added\";\n}\n",
    );
    let added = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "src/added.js",
            "--json",
        ],
    )
    .expect("watch added source");
    assert_eq!(added["status"].as_str(), Some("updated"));
    assert_eq!(added["files_parsed"].as_u64(), Some(1));
    assert_eq!(added["files_read"].as_u64(), Some(1));
    assert_eq!(added["changed_paths"][0].as_str(), Some("src/added.js"));
    assert_eq!(added["normal_dot_codegraph_mutated"].as_bool(), Some(false));
    assert!(agent_use_query_count(&data_root, &repo, "symbols", "addedLifecycleSymbol") > 0);
    {
        let store = SqliteGraphStore::open_read_only(&profile.db_path).expect("open DB");
        let added_entities = store
            .list_entities_by_file("src/added.js")
            .expect("added entities");
        assert!(!added_entities.is_empty());
        assert!(added_entities.iter().any(|entity| {
            let role = super::classify_entity_source_role(entity);
            !role.role.as_str().is_empty()
                && !role.reason.is_empty()
                && !role.classification_source.is_empty()
        }));
    }

    write_cli_fixture_file(
        &repo,
        "docs/lifecycle.md",
        "added markdown lifecycle evidence token\n",
    );
    let added_text = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "docs/lifecycle.md",
            "--json",
        ],
    )
    .expect("watch added text evidence");
    assert_eq!(added_text["status"].as_str(), Some("updated"));
    assert_eq!(added_text["files_parsed"].as_u64(), Some(0));
    assert_eq!(added_text["files_read"].as_u64(), Some(1));
    assert!(agent_use_query_count(&data_root, &repo, "text", "markdown lifecycle evidence") > 0);

    fs::remove_file(repo.join("src").join("delete_me.js")).expect("delete source");
    let deleted = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "src/delete_me.js",
            "--json",
        ],
    )
    .expect("watch deleted source");
    assert_eq!(deleted["status"].as_str(), Some("updated"));
    assert!(deleted["files_deleted"].as_u64().unwrap_or_default() > 0);
    assert_eq!(
        agent_use_query_count(&data_root, &repo, "symbols", "deletedLifecycleSymbol"),
        0
    );
    assert!(
        !agent_use_query_result_paths(&data_root, &repo, "files", "src/delete_me.js")
            .contains("src/delete_me.js")
    );
    {
        let store = SqliteGraphStore::open_read_only(&profile.db_path).expect("open DB");
        assert!(store
            .get_file("src/delete_me.js")
            .expect("deleted file lookup")
            .is_none());
        assert!(store
            .list_entities_by_file("src/delete_me.js")
            .expect("deleted entities")
            .is_empty());
    }
    let deleted_context = run_agent_use_test_command(
        &data_root,
        &[
            "context-pack",
            "--repo",
            path_string(&repo).as_str(),
            "--task",
            "Find deletedLifecycleSymbol",
            "--agent-json",
        ],
    )
    .expect("context deleted source");
    assert_eq!(deleted_context["status"].as_str(), Some("ok"));
    assert_eq!(
        deleted_context["fallback_evidence_count"].as_u64(),
        Some(0),
        "deleted file must not be returned as fresh fallback evidence: {deleted_context:?}"
    );
    assert_eq!(
        deleted_context["proof_path_count"].as_u64(),
        Some(0),
        "deleted file must not be returned through graph proof paths: {deleted_context:?}"
    );

    fs::remove_file(repo.join("docs").join("delete_me.md")).expect("delete markdown");
    let deleted_text = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "docs/delete_me.md",
            "--json",
        ],
    )
    .expect("watch deleted text evidence");
    assert_eq!(deleted_text["status"].as_str(), Some("updated"));
    assert!(deleted_text["files_deleted"].as_u64().unwrap_or_default() > 0);
    assert_eq!(
        agent_use_query_count(
            &data_root,
            &repo,
            "text",
            "deletedMarkdownLifecycleUniqueToken"
        ),
        0
    );

    fs::rename(
        repo.join("src").join("rename_old.js"),
        repo.join("src").join("rename_new.js"),
    )
    .expect("rename source");
    let renamed = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "src/rename_new.js",
            "--json",
        ],
    )
    .expect("watch renamed source");
    assert_eq!(renamed["status"].as_str(), Some("updated"));
    assert!(renamed["files_renamed"].as_u64().unwrap_or_default() > 0);
    assert!(
        !agent_use_query_result_paths(&data_root, &repo, "files", "rename_old")
            .contains("src/rename_old.js")
    );
    assert!(agent_use_query_count(&data_root, &repo, "files", "rename_new") > 0);
    assert!(agent_use_query_count(&data_root, &repo, "symbols", "renamedLifecycleSymbol") > 0);

    fs::rename(
        repo.join("src").join("rename_edit_old.js"),
        repo.join("src").join("rename_edit_new.js"),
    )
    .expect("rename edited source");
    write_cli_fixture_file(
        &repo,
        "src/rename_edit_new.js",
        "export function renameEditNewLifecycleSymbol() {\n  return \"rename-edit-new\";\n}\n",
    );
    let renamed_edit = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "src/rename_edit_new.js",
            "--json",
        ],
    )
    .expect("watch edited rename source");
    assert_eq!(renamed_edit["status"].as_str(), Some("updated"));
    assert!(
        renamed_edit["files_deleted"].as_u64().unwrap_or_default() > 0,
        "rename plus edit should prune the missing old path: {renamed_edit:?}"
    );
    assert_eq!(
        agent_use_query_count(&data_root, &repo, "symbols", "renameEditOldLifecycleSymbol"),
        0
    );
    assert!(
        agent_use_query_count(&data_root, &repo, "symbols", "renameEditNewLifecycleSymbol") > 0
    );
    assert!(
        !agent_use_query_result_paths(&data_root, &repo, "files", "rename_edit_old")
            .contains("src/rename_edit_old.js")
    );

    write_cli_fixture_file(
        &repo,
        "src/duplicate_b.js",
        "export function duplicateLifecycleSymbol() {\n  return \"same\";\n}\n",
    );
    let duplicate = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "src/duplicate_b.js",
            "--json",
        ],
    )
    .expect("watch duplicate content");
    assert_eq!(duplicate["status"].as_str(), Some("updated"));
    {
        let store = SqliteGraphStore::open_read_only(&profile.db_path).expect("open DB");
        assert!(store
            .get_file("src/duplicate_a.js")
            .expect("duplicate A")
            .is_some());
        assert!(store
            .get_file("src/duplicate_b.js")
            .expect("duplicate B")
            .is_some());
        assert!(!store
            .list_entities_by_file("src/duplicate_a.js")
            .expect("duplicate A entities")
            .is_empty());
        assert!(!store
            .list_entities_by_file("src/duplicate_b.js")
            .expect("duplicate B entities")
            .is_empty());
    }

    write_cli_fixture_file(
        &repo,
        "generated/ignored.js",
        "export function ignoredGeneratedLifecycleSymbol() {\n  return \"ignored\";\n}\n",
    );
    let ignored = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "generated/ignored.js",
            "--json",
        ],
    )
    .expect("watch ignored generated file");
    assert_eq!(ignored["status"].as_str(), Some("no_op"));
    assert_eq!(ignored["delta_state"].as_str(), Some("ready"));
    assert_eq!(
        ignored["reason"].as_str(),
        Some("ignored_path_no_graph_changes")
    );
    assert_eq!(
        ignored["no_op_paths"][0].as_str(),
        Some("generated/ignored.js")
    );
    assert_eq!(
        agent_use_query_count(
            &data_root,
            &repo,
            "symbols",
            "ignoredGeneratedLifecycleSymbol"
        ),
        0
    );

    write_cli_fixture_file(
        &repo,
        "src/casefile.js",
        "export function newCaseLifecycleSymbol() {\n  return \"new-case\";\n}\n",
    );
    let case_changed_arg = if cfg!(windows) {
        "SRC/CASEFILE.JS"
    } else {
        "src/casefile.js"
    };
    let case_update = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            case_changed_arg,
            "--json",
        ],
    )
    .expect("watch case-normalized source");
    assert_eq!(case_update["status"].as_str(), Some("updated"));
    assert_eq!(
        case_update["changed_paths"][0].as_str(),
        Some("src/casefile.js")
    );
    assert_eq!(
        agent_use_query_count(&data_root, &repo, "symbols", "oldCaseLifecycleSymbol"),
        0
    );
    assert!(agent_use_query_count(&data_root, &repo, "symbols", "newCaseLifecycleSymbol") > 0);
    {
        let store = SqliteGraphStore::open_read_only(&profile.db_path).expect("open DB");
        let matching_files = store
            .list_files(super::UNBOUNDED_STORE_READ_LIMIT)
            .expect("list files")
            .into_iter()
            .filter(|file| {
                file.repo_relative_path
                    .eq_ignore_ascii_case("src/casefile.js")
            })
            .count();
        assert_eq!(
            matching_files, 1,
            "case-equivalent paths must not duplicate"
        );
    }

    write_cli_fixture_file(
        &repo,
        "src/.atomic.js.tmp",
        "export function tempAtomicLifecycleSymbol() {\n  return \"temp\";\n}\n",
    );
    let temp = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "src/.atomic.js.tmp",
            "--json",
        ],
    )
    .expect("watch atomic temp");
    assert_eq!(temp["status"].as_str(), Some("no_op"));
    assert_eq!(
        agent_use_query_count(&data_root, &repo, "symbols", "tempAtomicLifecycleSymbol"),
        0
    );
    fs::remove_file(repo.join("src").join("atomic.js")).expect("remove old atomic");
    fs::rename(
        repo.join("src").join(".atomic.js.tmp"),
        repo.join("src").join("atomic.js"),
    )
    .expect("atomic rename into place");
    write_cli_fixture_file(
        &repo,
        "src/atomic.js",
        "export function newAtomicLifecycleSymbol() {\n  return \"new-atomic\";\n}\n",
    );
    let atomic = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "src/atomic.js",
            "--json",
        ],
    )
    .expect("watch atomic final");
    assert_eq!(atomic["status"].as_str(), Some("updated"));
    assert_eq!(
        agent_use_query_count(&data_root, &repo, "symbols", "oldAtomicLifecycleSymbol"),
        0
    );
    assert!(agent_use_query_count(&data_root, &repo, "symbols", "newAtomicLifecycleSymbol") > 0);
    assert_eq!(
        agent_use_query_count(&data_root, &repo, "files", ".atomic.js.tmp"),
        0
    );

    let outside = data_root.join("outside.js");
    fs::write(
        &outside,
        "export function outsideRepoLifecycleSymbol() { return 1; }\n",
    )
    .expect("write outside");
    let before_db = fs::read(&profile.db_path).expect("read DB before outside reject");
    let outside_reject = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            path_string(&outside).as_str(),
            "--json",
        ],
    )
    .expect("watch outside path");
    let after_db = fs::read(&profile.db_path).expect("read DB after outside reject");
    assert_eq!(outside_reject["status"].as_str(), Some("rejected"));
    assert_eq!(outside_reject["delta_state"].as_str(), Some("blocked"));
    assert_eq!(
        outside_reject["rejected_paths"][0]["reason"].as_str(),
        Some("path_outside_repo")
    );
    let outside_requested = path_string(&outside).replace('\\', "/");
    let outside_original = path_string(&outside);
    assert_eq!(
        outside_reject["rejected_paths"][0]["requested_path"].as_str(),
        Some(outside_requested.as_str())
    );
    assert_eq!(
        outside_reject["rejected_paths"][0]["original_path"].as_str(),
        Some(outside_original.as_str())
    );
    assert!(
        outside_reject["rejected_paths"][0]["path_mapping"]["mapping_status"]
            .as_str()
            .is_some()
    );
    assert_eq!(outside_reject["files_read"].as_u64(), Some(0));
    assert_eq!(
        before_db, after_db,
        "outside path rejection must not mutate DB bytes"
    );
    assert_eq!(
        agent_use_query_count(&data_root, &repo, "symbols", "outsideRepoLifecycleSymbol"),
        0
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_query_uses_external_profile_db() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    let symbols = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "symbols".to_string(),
            "agentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "1".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query symbols");
    assert_eq!(symbols["status"].as_str(), Some("ok"));
    assert_eq!(
        symbols["profile_name"].as_str(),
        Some(super::PRODUCTION_AGENT_USE_PROFILE_NAME)
    );
    assert_eq!(symbols["db_source"].as_str(), Some("agent-use profile"));
    assert_eq!(
        symbols["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_eq!(symbols["limit"].as_u64(), Some(1));
    assert!(
        symbols["result_count"].as_u64().unwrap_or(0) <= 1,
        "{symbols:?}"
    );
    assert_eq!(
        symbols["read_path_metrics"]["lookup_strategy"].as_str(),
        Some("symbol_dict_exact_lookup_plus_bounded_stage0_fts")
    );
    assert_eq!(
        symbols["read_path_metrics"]["full_scan_count"].as_u64(),
        Some(0)
    );
    assert_eq!(
        symbols["read_path_metrics"]["disk_fallback_used"].as_bool(),
        Some(false)
    );

    let text = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "text".to_string(),
            "agent-use-ok".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query text");
    assert_eq!(text["status"].as_str(), Some("ok"));
    assert_eq!(
        text["db"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_eq!(text["external_db_used"].as_bool(), Some(true));
    assert_eq!(
        text["read_path_metrics"]["lookup_strategy"].as_str(),
        Some("stage0_fts_bounded_lookup")
    );
    assert_eq!(
        text["read_path_metrics"]["full_scan_count"].as_u64(),
        Some(0)
    );
    assert_eq!(
        text["read_path_metrics"]["source_file_load_count"].as_u64(),
        Some(0)
    );

    let files = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "files".to_string(),
            "service".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query files");
    assert_eq!(files["status"].as_str(), Some("ok"));
    assert_eq!(
        files["db"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_eq!(
        files["read_path_metrics"]["lookup_strategy"].as_str(),
        Some("stage0_fts_file_path_title_lookup")
    );
    assert_eq!(
        files["read_path_metrics"]["full_scan_count"].as_u64(),
        Some(0)
    );
    assert_eq!(
        files["read_path_metrics"]["disk_fallback_used"].as_bool(),
        Some(false)
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_relation_queries_use_external_profile_db() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_rust_relation_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");

    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    let callees = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "callees".to_string(),
            "caller".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "2".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query callees");
    assert_agent_use_relation_query_claims_profile_db(&callees, &profile);
    assert_eq!(callees["direction"].as_str(), Some("callees"));
    assert_eq!(callees["limit"].as_u64(), Some(2));
    assert!(
        callees["result_count"].as_u64().unwrap_or_default() > 0,
        "{callees:?}"
    );
    assert!(
        callees["result_count"].as_u64().unwrap_or_default() <= 2,
        "{callees:?}"
    );
    assert_eq!(callees["graph_proof"].as_bool(), Some(true));
    assert_eq!(callees["proof_status"].as_str(), Some("proof_path_found"));
    assert_eq!(
        callees["read_path_metrics"]["lookup_strategy"].as_str(),
        Some("bounded_call_relation_graph_lookup")
    );
    assert_eq!(
        callees["results"][0]["edge"]["relation"].as_str(),
        Some("CALLS")
    );
    assert_eq!(
        callees["results"][0]["edge"]["exactness"].as_str(),
        Some("parser_verified")
    );
    assert!(callees["results"][0]["source_span"].is_object());
    assert_eq!(
        callees["results"][0]["evidence_role"].as_str(),
        Some("production")
    );
    assert_eq!(
        callees["results"][0]["proof_strength"].as_str(),
        Some("graph_relation_proof")
    );

    let plain_callees = super::with_repo_db_context(&repo, &profile.db_path, || {
        super::run_query_command(&[
            "callees".to_string(),
            "caller".to_string(),
            "--limit".to_string(),
            "2".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("plain query callees");
    assert_eq!(
        callees["result_count"].as_u64(),
        plain_callees["result_count"].as_u64()
    );

    let callers = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "callers".to_string(),
            "leaf".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query callers");
    assert_agent_use_relation_query_claims_profile_db(&callers, &profile);
    assert_eq!(callers["direction"].as_str(), Some("callers"));
    assert_eq!(callers["graph_proof"].as_bool(), Some(true));
    assert_eq!(
        callers["results"][0]["edge"]["evidence_role"].as_str(),
        Some("production")
    );
    assert!(
        serialized_len_for_test(&callers) <= super::DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        "{} bytes: {callers}",
        serialized_len_for_test(&callers)
    );
    assert_eq!(count_key_occurrences(&callers, "recovery_commands"), 1);
    assert!(callers["profile"].is_null());
    assert!(callers["agent_use_profile"].is_null());
    assert_eq!(
        callers["agent_json_budget"]["max_output_bytes"].as_u64(),
        Some(super::DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES as u64)
    );

    let path = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "path".to_string(),
            "caller".to_string(),
            "leaf".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "3".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query path");
    assert_agent_use_relation_query_claims_profile_db(&path, &profile);
    assert_eq!(path["schema_name"].as_str(), Some("query_path_agent_json"));
    assert_eq!(path["graph_proof"].as_bool(), Some(true));
    assert_eq!(path["proof_status"].as_str(), Some("proof_path_found"));
    if !path["read_path_metrics"].is_null() {
        assert_eq!(
            path["read_path_metrics"]["lookup_strategy"].as_str(),
            Some("bounded_graph_path_lookup")
        );
    }
    assert_eq!(
        path["results"][0]["proof_strength"].as_str(),
        Some("graph_relation_proof")
    );
    assert!(path["results"][0]["source_spans"]
        .as_array()
        .is_some_and(|spans| !spans.is_empty()));

    let chain = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "chain".to_string(),
            "caller".to_string(),
            "leaf".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "3".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query chain");
    assert_agent_use_relation_query_claims_profile_db(&chain, &profile);
    assert_eq!(
        chain["schema_name"].as_str(),
        Some("query_chain_agent_json")
    );
    assert_eq!(chain["graph_proof"].as_bool(), Some(true));

    let references = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "references".to_string(),
            "leaf".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query references");
    assert_agent_use_relation_query_claims_profile_db(&references, &profile);
    assert_eq!(
        references["schema_name"].as_str(),
        Some("query_references_agent_json")
    );
    assert!(
        references["result_count"].as_u64().unwrap_or_default() > 0,
        "{references:?}"
    );
    assert_eq!(
        references["results"][0]["reference_evidence_kind"].as_str(),
        Some("graph_reference")
    );
    assert_eq!(
        references["results"][0]["text_reference"].as_bool(),
        Some(false)
    );

    let definitions = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "definitions".to_string(),
            "leaf".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query definitions");
    assert_agent_use_relation_query_claims_profile_db(&definitions, &profile);
    assert_eq!(
        definitions["schema_name"].as_str(),
        Some("query_definitions_agent_json")
    );
    assert!(
        definitions["result_count"].as_u64().unwrap_or_default() > 0,
        "{definitions:?}"
    );
    assert_eq!(
        definitions["results"][0]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(
        definitions["results"][0]["proof_strength"].as_str(),
        Some("symbol_definition")
    );
    assert_eq!(
        definitions["read_path_metrics"]["lookup_strategy"].as_str(),
        Some("bounded_symbol_definition_lookup")
    );

    let no_path = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "path".to_string(),
            "leaf".to_string(),
            "not_present".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query no path");
    assert_agent_use_relation_query_claims_profile_db(&no_path, &profile);
    assert_eq!(no_path["graph_proof"].as_bool(), Some(false));
    assert_eq!(
        no_path["proof_status"].as_str(),
        Some("no_proof_path_found")
    );

    let unresolved = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "unresolved-calls".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--source-scan".to_string(),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use query unresolved-calls");
    assert_eq!(unresolved["status"].as_str(), Some("ok"));
    if !unresolved["read_path_metrics"].is_null() {
        assert_eq!(
            unresolved["read_path_metrics"]["lookup_strategy"].as_str(),
            Some("bounded_unresolved_call_lookup")
        );
    }
    if !unresolved["db_lifecycle_read"].is_null() {
        assert_eq!(
            unresolved["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
            Some(path_string(&profile.db_path).as_str())
        );
    }
    assert_eq!(unresolved["external_db_used"].as_bool(), Some(true));
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_query_reports_unsafe_profile_db_without_fallback() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");

    let missing = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "callees".to_string(),
            "callAgentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--agent-json".to_string(),
        ])
    })
    .expect("missing query");
    assert_eq!(missing["status"].as_str(), Some("not_indexed"));
    assert_eq!(missing["claimable"].as_bool(), Some(false));
    assert!(missing["recovery"]["agent_use_index_command"].is_string());
    assert!(!profile.profile_root.exists());

    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");
    {
        let connection = Connection::open(&profile.db_path).expect("open profile DB");
        connection
            .execute(
                "UPDATE codegraph_db_passport SET last_run_status = 'interrupted' WHERE id = 1",
                [],
            )
            .expect("mark stale");
    }
    let stale = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "callers".to_string(),
            "agentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--agent-json".to_string(),
        ])
    })
    .expect("stale query");
    assert_eq!(stale["claimable"].as_bool(), Some(false));
    assert_eq!(
        stale["db_lifecycle_read"]["artifact_freshness"].as_str(),
        Some("incomplete:interrupted")
    );

    let repo_foreign = temp_repo();
    write_agent_use_context_fixture(&repo_foreign);
    let profile_foreign =
        super::resolve_agent_use_profile_with_data_root(&repo_foreign, &data_root)
            .expect("foreign profile");
    fs::create_dir_all(&profile_foreign.profile_root).expect("create foreign profile parent");
    fs::copy(&profile.db_path, &profile_foreign.db_path).expect("copy foreign DB");
    let foreign = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "path".to_string(),
            "callAgentUseTarget".to_string(),
            "agentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo_foreign),
            "--agent-json".to_string(),
        ])
    })
    .expect("foreign query");
    assert_eq!(foreign["claimable"].as_bool(), Some(false));
    assert_eq!(
        foreign["db_problem_kind"].as_str(),
        Some("repo_root_mismatch")
    );

    let repo_old = temp_repo();
    write_agent_use_context_fixture(&repo_old);
    let profile_old = super::resolve_agent_use_profile_with_data_root(&repo_old, &data_root)
        .expect("old profile");
    fs::create_dir_all(&profile_old.profile_root).expect("create old parent");
    {
        let connection = Connection::open(&profile_old.db_path).expect("open old DB");
        connection
            .execute_batch("PRAGMA user_version = 1; CREATE TABLE legacy_only(id INTEGER);")
            .expect("old schema");
    }
    let old = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "definitions".to_string(),
            "agentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo_old),
            "--agent-json".to_string(),
        ])
    })
    .expect("old schema query");
    assert_eq!(old["claimable"].as_bool(), Some(false));
    assert_eq!(old["db_problem_kind"].as_str(), Some("schema_mismatch"));
    assert_no_dot_codegraph_sqlite(&repo);
    assert_no_dot_codegraph_sqlite(&repo_foreign);
    assert_no_dot_codegraph_sqlite(&repo_old);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&repo_foreign, "cleanup foreign repo");
    remove_dir_all_with_retry(&repo_old, "cleanup old repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_context_pack_uses_graph_or_candidate_profile_context() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    let graph = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--seed".to_string(),
            "agentUseTarget".to_string(),
            "--limit-paths".to_string(),
            "5".to_string(),
            "--limit-snippets".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use context graph");
    assert_eq!(graph["status"].as_str(), Some("ok"));
    assert_eq!(
        graph["profile_name"].as_str(),
        Some(super::PRODUCTION_AGENT_USE_PROFILE_NAME)
    );
    assert_eq!(
        graph["db"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert_eq!(graph["external_db_used"].as_bool(), Some(true));
    assert!(
        graph["staged_availability"].is_object() || graph["sidecar_statuses"].is_object(),
        "agent-use context-pack should expose staged or sidecar state: {graph}"
    );
    assert_eq!(
        graph["read_path_metrics"]["full_scan_count"].as_u64(),
        Some(0)
    );
    assert_eq!(
        graph["read_path_metrics"]["disk_fallback_used"].as_bool(),
        Some(false)
    );
    assert_eq!(
        graph["read_path_metrics"]["limits_apply_before_hydration"].as_bool(),
        Some(true)
    );
    assert_eq!(
        graph["patch_assist_packet"]["first_use_state"].as_str(),
        Some("graph_ready")
    );
    assert_eq!(
        graph["patch_assist_packet"]["claimability"]
            ["graph_proof_only_from_graph_source_verification"]
            .as_bool(),
        Some(true)
    );
    let graph_budget = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--seed".to_string(),
            "agentUseTarget".to_string(),
            "--budget".to_string(),
            "1600".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use budgeted context graph");
    assert!(
        serialized_len_for_test(&graph_budget)
            <= super::DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        "{} bytes: {graph_budget}",
        serialized_len_for_test(&graph_budget)
    );
    assert_eq!(count_key_occurrences(&graph_budget, "recovery_commands"), 1);
    assert!(graph_budget["profile"].is_null());
    assert!(graph_budget["agent_use_profile"].is_null());
    assert_eq!(
        graph_budget["agent_json_budget"]["max_output_bytes"].as_u64(),
        Some(super::DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES as u64)
    );
    assert!(!graph_budget["claimability"].is_null());
    assert!(!graph_budget["db_lifecycle_read"].is_null());
    assert_eq!(graph_budget["graph_proof"].as_bool(), Some(true));
    let graph_compact = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--seed".to_string(),
            "agentUseTarget".to_string(),
            "--agent-json".to_string(),
            "--max-output-bytes".to_string(),
            "4096".to_string(),
        ])
    })
    .expect("agent-use compact context graph");
    let graph_compact_len = serialized_len_for_test(&graph_compact);
    if graph_compact_len > 4096 {
        assert_eq!(
            graph_compact["agent_json_budget"]["max_output_bytes_exceeded"].as_bool(),
            Some(true),
            "{} bytes without max_output_bytes_exceeded: {graph_compact}",
            graph_compact_len
        );
        assert_eq!(
            graph_compact["agent_json_budget"]["required_safety_fields_preserved"].as_bool(),
            Some(true)
        );
    }
    assert!(!graph_compact["claimability"].is_null());
    assert_eq!(
        graph_compact["db_source"].as_str(),
        Some("agent-use profile")
    );
    assert!(!graph_compact["recovery"].is_null());
    assert!(!graph_compact["lifecycle"].is_null());
    assert_eq!(graph_compact["graph_proof"].as_bool(), Some(true));
    assert_eq!(
        graph_compact["agent_json_budget"]["required_safety_fields_preserved"].as_bool(),
        Some(true)
    );
    assert_eq!(
        graph_compact["normal_dot_codegraph_mutated"].as_bool(),
        Some(false)
    );
    assert_no_dot_codegraph_sqlite(&repo);

    let spool_repo = temp_repo();
    let source_spool = write_candidate_spool_cli_fixture(&spool_repo);
    let spool_profile = super::resolve_agent_use_profile_with_data_root(&spool_repo, &data_root)
        .expect("spool profile");
    fs::create_dir_all(&spool_profile.profile_root).expect("create spool profile parent");
    fs::copy(&source_spool, &spool_profile.candidate_spool_path).expect("copy profile spool");
    super::rebuild_candidate_spool_query_index_for_repo(
        &spool_repo,
        &spool_profile.candidate_spool_path,
    )
    .expect("build profile spool query index");
    let candidate = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&spool_repo),
            "--task".to_string(),
            "spool_target".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("agent-use context spool");
    assert_eq!(candidate["status"].as_str(), Some("ok"));
    assert_eq!(candidate["graph_proof"].as_bool(), Some(false));
    assert_eq!(candidate["candidate_only"].as_bool(), Some(true));
    if !candidate["read_path_metrics"].is_null() {
        assert_eq!(
            candidate["read_path_metrics"]["full_scan_count"].as_u64(),
            Some(0)
        );
    }
    if !candidate["db_lifecycle_read"].is_null() {
        assert_eq!(
            candidate["db_lifecycle_read"]["path_access_status"].as_str(),
            Some("db_missing")
        );
    }
    assert_eq!(
        candidate["patch_assist_packet"]["first_use_state"].as_str(),
        Some("candidate_ready_no_graph")
    );
    assert_eq!(
        candidate["patch_assist_packet"]["graph_proof"].as_bool(),
        Some(false)
    );
    let candidate_evidence_count = candidate["patch_assist_packet"]["candidate_evidence"]
        .as_array()
        .map(Vec::len)
        .or_else(|| {
            candidate["patch_assist_packet"]["candidate_evidence_count"]
                .as_u64()
                .and_then(|count| usize::try_from(count).ok())
        })
        .unwrap_or_default();
    assert!(candidate_evidence_count > 0, "{candidate}");

    write_cli_fixture_file(&spool_repo, "src/lib.rs", "pub fn changed() {}\n");
    let stale = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&spool_repo),
            "--task".to_string(),
            "spool_target".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("stale spool context");
    assert_eq!(stale["claimable"].as_bool(), Some(false));
    assert!(stale["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .any(|error| error
            .as_str()
            .unwrap_or("")
            .contains("candidate_spool_stale")));
    assert_eq!(
        stale["patch_assist_packet"]["first_use_state"].as_str(),
        Some("stale_or_degraded")
    );
    let stale_candidate_evidence_count = stale["patch_assist_packet"]["candidate_evidence"]
        .as_array()
        .map(Vec::len)
        .or_else(|| {
            stale["patch_assist_packet"]["candidate_evidence_count"]
                .as_u64()
                .and_then(|count| usize::try_from(count).ok())
        })
        .unwrap_or_default();
    assert_eq!(stale_candidate_evidence_count, 0, "{stale}");
    assert_no_dot_codegraph_sqlite(&spool_repo);

    let missing_repo = temp_repo();
    write_agent_use_context_fixture(&missing_repo);
    let missing = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&missing_repo),
            "--task".to_string(),
            "agentUseTarget".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("missing context");
    assert_eq!(missing["status"].as_str(), Some("not_indexed"));
    assert_eq!(missing["claimable"].as_bool(), Some(false));
    assert_eq!(
        missing["patch_assist_packet"]["first_use_state"].as_str(),
        Some("unavailable")
    );
    assert_eq!(
        missing["patch_assist_packet"]["proof_status"].as_str(),
        Some("not_available_until_index")
    );
    assert_no_dot_codegraph_sqlite(&missing_repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&spool_repo, "cleanup spool repo");
    remove_dir_all_with_retry(&missing_repo, "cleanup missing repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_staged_warm_start_artifacts_are_candidate_only_and_source_bound() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");

    let missing = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use status missing");
    assert_eq!(missing["status"].as_str(), Some("not_indexed"));
    assert_eq!(missing["graph_db_status"].as_str(), Some("no_index"));
    assert_eq!(missing["candidate_spool_status"].as_str(), Some("no_spool"));
    assert_eq!(missing["vector_runtime_status"].as_str(), Some("missing"));
    assert_eq!(missing["vector_audit_status"].as_str(), Some("missing"));
    assert!(!profile.profile_root.exists());

    let indexed = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use staged index");
    assert_eq!(indexed["candidate_spool_requested"].as_bool(), Some(true));
    assert_eq!(indexed["candidate_spool_created"].as_bool(), Some(true));
    assert_eq!(
        indexed["candidate_spool_query_index_status"].as_str(),
        Some("ready")
    );
    assert_eq!(indexed["vector_runtime_requested"].as_bool(), Some(true));
    assert_eq!(indexed["vector_runtime_created"].as_bool(), Some(true));
    assert_eq!(indexed["vector_runtime_status"].as_str(), Some("ready"));
    assert_eq!(indexed["vector_audit_requested"].as_bool(), Some(false));
    assert_eq!(indexed["vector_audit_created"].as_bool(), Some(false));
    assert_eq!(indexed["vector_audit_status"].as_str(), Some("missing"));
    assert!(profile.candidate_spool_path.exists());
    assert!(profile.candidate_spool_query_index_path.exists());
    assert!(profile.vector_runtime_path.exists());
    assert!(!profile.vector_audit_path.exists());

    let graph_context = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--agent-json".to_string(),
            "--explain".to_string(),
        ])
    })
    .expect("graph context with staged artifacts");
    assert_eq!(graph_context["status"].as_str(), Some("ok"));
    assert_eq!(graph_context["graph_proof_available"].as_bool(), Some(true));
    assert_eq!(
        graph_context["vector_runtime_status"].as_str(),
        Some("ready")
    );
    assert!(graph_context["active_candidate_sources"]
        .as_array()
        .expect("active sources")
        .iter()
        .any(|source| source.as_str() == Some("vector_semantic")));
    assert_eq!(
        graph_context["staged_availability"]["layer_readiness"]["vector_runtime"]["graph_proof"]
            .as_bool(),
        Some(false)
    );

    let audit_index = super::run_index_command(&[
        path_string(&repo),
        "--db".to_string(),
        path_string(&profile.db_path),
        "--fresh".to_string(),
        "--json".to_string(),
        "--build-vector-index".to_string(),
        path_string(&profile.vector_runtime_path),
        "--vector-audit-artifact".to_string(),
        path_string(&profile.vector_audit_path),
    ])
    .expect("build audit artifact through existing index surface");
    assert_eq!(audit_index["status"].as_str(), Some("indexed"));
    assert!(profile.vector_audit_path.exists());
    let audit_status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use status with audit artifact");
    assert_eq!(audit_status["vector_audit_status"].as_str(), Some("ready"));
    assert_eq!(
        audit_status["staged_availability"]["layer_readiness"]["vector_audit"]["diagnostic_only"]
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        audit_status["staged_availability"]["layer_readiness"]["vector_audit"]
            ["runtime_dependency"]
            .as_bool(),
        Some(false)
    );

    fs::remove_file(repo.join("src").join("service.ts")).expect("delete source");
    let stale_vector = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--agent-json".to_string(),
            "--explain".to_string(),
        ])
    })
    .expect("context with stale runtime vector sidecar");
    assert_eq!(
        stale_vector["vector_runtime_status"].as_str(),
        Some("stale")
    );
    assert_eq!(stale_vector["graph_proof_available"].as_bool(), Some(true));
    assert!(!stale_vector["active_candidate_sources"]
        .as_array()
        .expect("active sources")
        .iter()
        .any(|source| source.as_str() == Some("vector_semantic")));
    assert!(stale_vector["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .any(|warning| warning
            .as_str()
            .unwrap_or_default()
            .contains("Vector runtime sidecar is stale")));
    assert_eq!(
        stale_vector["patch_assist_packet"]["first_use_state"].as_str(),
        Some("graph_ready")
    );
    assert!(stale_vector["patch_assist_packet"]["degradation_warnings"]
        .as_array()
        .expect("degradation warnings")
        .iter()
        .any(
            |warning| warning["layer"].as_str() == Some("vector_runtime")
                && warning["status"].as_str() == Some("stale")
        ));
    assert!(!stale_vector["patch_assist_packet"]["candidate_evidence"]
        .as_array()
        .expect("candidate evidence")
        .iter()
        .any(|candidate| candidate["candidate_sources"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|source| source.as_str() == Some("vector_semantic"))));

    let candidate_repo = temp_repo();
    write_agent_use_context_fixture(&candidate_repo);
    let candidate_profile =
        super::resolve_agent_use_profile_with_data_root(&candidate_repo, &data_root)
            .expect("candidate profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&candidate_repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index for candidate-only fallback");
    super::remove_sqlite_file_family(&candidate_profile.db_path)
        .expect("remove graph DB family for candidate-only fallback");
    let candidate_only = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&candidate_repo),
            "--task".to_string(),
            "agentUseTarget".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("candidate-only context");
    assert_eq!(candidate_only["status"].as_str(), Some("ok"));
    assert_eq!(candidate_only["candidate_only"].as_bool(), Some(true));
    assert_eq!(candidate_only["graph_proof"].as_bool(), Some(false));
    assert_eq!(
        candidate_only["candidate_spool_query_index_status"].as_str(),
        Some("ready")
    );
    assert_eq!(
        candidate_only["db_lifecycle_read"]["path_access_status"].as_str(),
        Some("db_missing")
    );

    write_cli_fixture_file(
        &candidate_repo,
        "src/service.ts",
        "export function changedAfterSpool() { return \"stale\"; }\n",
    );
    let stale_spool = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&candidate_repo),
            "--task".to_string(),
            "agentUseTarget".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("stale candidate spool context");
    assert_eq!(stale_spool["claimable"].as_bool(), Some(false));
    assert_eq!(
        stale_spool["candidate_spool_status"].as_str(),
        Some("stale")
    );
    assert_eq!(
        stale_spool["candidate_only_available"].as_bool(),
        Some(false)
    );
    assert!(stale_spool["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .any(|error| error
            .as_str()
            .unwrap_or_default()
            .contains("candidate_spool_stale")));

    assert_no_dot_codegraph_sqlite(&repo);
    assert_no_dot_codegraph_sqlite(&candidate_repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&candidate_repo, "cleanup candidate repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_watch_once_dirty_sidecars_do_not_masquerade_as_fresh_context() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");

    run_agent_use_test_command(
        &data_root,
        &["index", "--repo", path_string(&repo).as_str(), "--json"],
    )
    .expect("agent-use index");
    assert!(profile.candidate_spool_path.exists());
    assert!(profile.candidate_spool_query_index_path.exists());
    assert!(profile.vector_runtime_path.exists());
    assert!(
        path_evidence_total_count(&profile.db_path) > 0,
        "fixture should persist stored PathEvidence before the delta"
    );

    write_cli_fixture_file(
            &repo,
            "src/service.ts",
            "export function agentUseTarget() {\n  return \"freshzz\";\n}\n\nexport function dirtyEvidenceFreshSymbol() {\n  return agentUseTarget();\n}\n",
        );
    let changed = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "src/service.ts",
            "--json",
        ],
    )
    .expect("watch changed source");
    assert_eq!(changed["status"].as_str(), Some("updated"));
    assert_eq!(changed["delta_state"].as_str(), Some("updated"));
    assert_eq!(changed["new_graph_valid"].as_bool(), Some(true));
    assert_eq!(changed["freshness"]["graph_db"].as_str(), Some("current"));
    assert_eq!(
        changed["freshness"]["binary_candidate_records"].as_str(),
        Some("not_applicable")
    );
    assert_eq!(
        changed["freshness"]["nuance_candidate_records"].as_str(),
        Some("not_applicable")
    );
    assert_eq!(
        changed["freshness"]["proof_path_caches"].as_str(),
        Some("not_applicable")
    );
    assert!(matches!(
        changed["path_evidence_invalidated"]["action"].as_str(),
        Some("refreshed") | Some("invalidated")
    ));
    assert_eq!(
        changed["candidate_spool_invalidated_or_rebuilt"]["action"].as_str(),
        Some("invalidated")
    );
    assert_eq!(
        changed["candidate_query_index_invalidated_or_rebuilt"]["action"].as_str(),
        Some("invalidated")
    );
    assert_eq!(
        changed["vector_chunks_invalidated_or_rebuilt"]["action"].as_str(),
        Some("invalidated")
    );
    assert_eq!(
        changed["binary_candidates_invalidated_or_not_applicable"]["status"].as_str(),
        Some("not_applicable")
    );
    assert_eq!(
        changed["nuance_tokens_invalidated_or_not_applicable"]["status"].as_str(),
        Some("not_applicable")
    );
    assert_eq!(
        changed["proof_path_caches_invalidated_or_not_applicable"]["status"].as_str(),
        Some("not_applicable")
    );
    assert_eq!(
        changed["nuance_tokens_invalidated_or_not_applicable"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(
        changed["candidate_spool_invalidated_or_rebuilt"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(
        changed["staged_availability"]["layer_readiness"]["candidate_spool"]["graph_proof"]
            .as_bool(),
        Some(false)
    );
    assert_eq!(
        changed["staged_availability"]["layer_readiness"]["vector_runtime"]["graph_proof"]
            .as_bool(),
        Some(false)
    );
    assert_eq!(
        changed["staged_availability"]["layer_readiness"]["candidate_spool"]["status"].as_str(),
        Some("stale")
    );
    assert_eq!(
        changed["staged_availability"]["layer_readiness"]["candidate_spool"]["query_index_status"]
            .as_str(),
        Some("stale")
    );
    assert_eq!(
        changed["staged_availability"]["layer_readiness"]["vector_runtime"]["status"].as_str(),
        Some("stale")
    );
    assert_ne!(changed["status"].as_str(), Some("degraded"));
    assert_eq!(changed["graph_validation_status"].as_str(), Some("ok"));
    assert_eq!(changed["agent_action"].as_str(), Some("continue"));
    assert_eq!(
        changed["candidate_recall_status"].as_str(),
        Some("degraded")
    );
    assert_eq!(
        changed["candidate_recall_action"].as_str(),
        Some("refresh_sidecars_if_candidate_recall_needed")
    );
    assert_eq!(
        changed["sidecar_degradation_kind"].as_str(),
        Some("candidate_layer_only")
    );
    assert_eq!(
        changed["graph_validation_unaffected_by_optional_sidecars"].as_bool(),
        Some(true)
    );
    assert_eq!(
        changed["optional_sidecar_staleness_affects_graph_proof"].as_bool(),
        Some(false)
    );
    assert!(
        path_evidence_total_count(&profile.db_path) > 0,
        "fresh PathEvidence should remain source-bound to the changed file"
    );
    assert!(agent_use_query_count(&data_root, &repo, "symbols", "agentUseTarget") > 0);
    assert!(agent_use_query_count(&data_root, &repo, "symbols", "dirtyEvidenceFreshSymbol") > 0);
    assert_eq!(
        agent_use_query_count(&data_root, &repo, "text", "obsoletezz"),
        0
    );
    assert!(agent_use_query_count(&data_root, &repo, "text", "freshzz") > 0);
    assert!(profile.delta_state_path.exists());

    let status_after_delta = run_agent_use_test_command(
        &data_root,
        &["status", "--repo", path_string(&repo).as_str(), "--json"],
    )
    .expect("status after delta");
    assert_eq!(
        status_after_delta["graph_freshness"].as_str(),
        Some("current")
    );
    assert_eq!(status_after_delta["dirty_state"].as_str(), Some("ready"));
    assert_eq!(
        status_after_delta["last_delta_update_summary"]["status"].as_str(),
        Some("updated")
    );
    assert_eq!(
        status_after_delta["rtds_freshness"]["startup_auto_index"].as_bool(),
        Some(false)
    );
    assert_eq!(
        status_after_delta["rtds_freshness"]["dot_codegraph_fallback"].as_bool(),
        Some(false)
    );
    assert!(status_after_delta["stale_candidate_layers"]
        .as_array()
        .expect("stale candidate layers")
        .iter()
        .any(|layer| layer["layer"].as_str() == Some("candidate_spool")));

    let context = run_agent_use_test_command(
        &data_root,
        &[
            "context-pack",
            "--repo",
            path_string(&repo).as_str(),
            "--task",
            "Find dirtyEvidenceFreshSymbol",
            "--agent-json",
            "--explain",
        ],
    )
    .expect("context after dirty sidecar invalidation");
    assert_eq!(context["status"].as_str(), Some("ok"));
    assert_eq!(context["graph_proof_available"].as_bool(), Some(true));
    assert_eq!(context["graph_freshness"].as_str(), Some("current"));
    assert_eq!(context["graph_validation_status"].as_str(), Some("ok"));
    assert_eq!(context["agent_action"].as_str(), Some("continue"));
    assert_eq!(
        context["candidate_recall_status"].as_str(),
        Some("degraded")
    );
    assert_eq!(
        context["candidate_recall_action"].as_str(),
        Some("refresh_sidecars_if_candidate_recall_needed")
    );
    assert_eq!(
        context["graph_validation_unaffected_by_optional_sidecars"].as_bool(),
        Some(true)
    );
    assert_eq!(
        context["last_delta_update_summary"]["status"].as_str(),
        Some("updated")
    );
    assert_eq!(
        context["rtds_freshness"]["candidate_context_policy"].as_str(),
        Some("candidate_only_only_when_current_source_bound")
    );
    assert_eq!(context["candidate_spool_status"].as_str(), Some("stale"));
    assert_eq!(context["vector_runtime_status"].as_str(), Some("stale"));
    assert!(!context["active_candidate_sources"]
        .as_array()
        .expect("active sources")
        .iter()
        .any(|source| source.as_str() == Some("candidate_spool")
            || source.as_str() == Some("vector_semantic")));

    fs::remove_file(repo.join("src").join("service.ts")).expect("delete source");
    let deleted = run_agent_use_test_command(
        &data_root,
        &[
            "watch",
            "--repo",
            path_string(&repo).as_str(),
            "--once",
            "--changed",
            "src/service.ts",
            "--json",
        ],
    )
    .expect("watch deleted source");
    assert_eq!(deleted["status"].as_str(), Some("updated"));
    assert_eq!(
        deleted["path_evidence_invalidated"]["action"].as_str(),
        Some("invalidated")
    );
    assert_eq!(
        deleted["freshness"]["path_evidence"].as_str(),
        Some("stale")
    );
    assert_eq!(
        path_evidence_total_count(&profile.db_path),
        0,
        "delete must remove PathEvidence references to the deleted file"
    );
    assert_eq!(
        agent_use_query_count(&data_root, &repo, "symbols", "dirtyEvidenceFreshSymbol"),
        0
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_mcp_config_is_readonly_and_matches_status_path() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let parent = temp_repo();
    let repo_a = parent.join("left").join("app");
    let repo_b = parent.join("right").join("app");
    let unicode_repo = parent.join("repo with spaces é");
    fs::create_dir_all(&repo_a).expect("repo a");
    fs::create_dir_all(&repo_b).expect("repo b");
    fs::create_dir_all(&unicode_repo).expect("unicode repo");
    write_agent_use_context_fixture(&repo_a);
    write_agent_use_context_fixture(&repo_b);
    write_agent_use_context_fixture(&unicode_repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo_a, &data_root).expect("profile");
    let profile_b =
        super::resolve_agent_use_profile_with_data_root(&repo_b, &data_root).expect("profile b");
    let profile_unicode =
        super::resolve_agent_use_profile_with_data_root(&unicode_repo, &data_root)
            .expect("unicode profile");

    let status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo_a),
            "--json".to_string(),
        ])
    })
    .expect("agent-use status");
    let config = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "mcp-config".to_string(),
            "--repo".to_string(),
            path_string(&repo_a),
            "--json".to_string(),
        ])
    })
    .expect("mcp config");
    assert_eq!(config["status"].as_str(), Some("not_indexed"));
    assert_eq!(
        status["config_discovery"]["status"].as_str(),
        Some("config_missing")
    );
    assert_eq!(
        config["config_discovery"]["status"].as_str(),
        Some("config_missing")
    );
    assert_eq!(
        config["artifact_hygiene"]["reports_audit_local_evidence"].as_bool(),
        Some(true)
    );
    assert_eq!(config["config_version"].as_u64(), Some(1));
    assert!(config["generated_at"].as_u64().is_some());
    assert!(config["generated_at_unix_ms"].as_u64().is_some());
    assert_eq!(
        config["profile_name"].as_str(),
        Some(super::PRODUCTION_AGENT_USE_PROFILE_NAME)
    );
    assert_eq!(
        config["active_profile_name"].as_str(),
        Some(super::PRODUCTION_AGENT_USE_PROFILE_NAME)
    );
    assert_eq!(
        config["repo_identity_label"].as_str(),
        Some(profile.repo_identity_label.as_str())
    );
    assert_eq!(
        config["repo_identity_hash"].as_str(),
        Some(profile.repo_identity_hash.as_str())
    );
    assert_eq!(
        config["repo_identity_short_hash"].as_str(),
        Some(super::agent_use_repo_identity_short_hash(&profile).as_str())
    );
    assert_eq!(
        config["profile_root"].as_str(),
        Some(path_string(&profile.profile_root).as_str())
    );
    assert_eq!(
        config["local_agent_profile"]["profile_name"].as_str(),
        Some(super::PRODUCTION_AGENT_USE_PROFILE_NAME)
    );
    assert_eq!(
        config["local_agent_profile"]["agent_label_supported"].as_bool(),
        Some(false)
    );
    assert!(config["local_agent_profile"]["agent_label"].is_null());
    assert_eq!(config["writes_files"].as_bool(), Some(false));
    assert_eq!(config["db_path"].as_str(), status["db_path"].as_str());
    assert_eq!(
        config["mcp_server_args"].as_array().expect("mcp args"),
        profile
            .mcp_args
            .iter()
            .map(|arg| Value::String(arg.clone()))
            .collect::<Vec<_>>()
            .as_slice()
    );
    assert_eq!(
        config["sidecar_paths"]["candidate_spool_path"].as_str(),
        Some(path_string(&profile.candidate_spool_path).as_str())
    );
    assert_eq!(
        config["sidecar_paths"]["candidate_spool_query_index_path"].as_str(),
        Some(path_string(&profile.candidate_spool_query_index_path).as_str())
    );
    assert_eq!(
        config["sidecar_paths"]["vector_runtime_path"].as_str(),
        Some(path_string(&profile.vector_runtime_path).as_str())
    );
    assert_eq!(
        config["sidecar_paths"]["vector_audit_path"].as_str(),
        Some(path_string(&profile.vector_audit_path).as_str())
    );
    assert_eq!(
        config["sidecar_paths"]["lock_or_publish_state_path"].as_str(),
        Some(path_string(&profile.lock_or_publish_state_path).as_str())
    );
    assert_eq!(
        config["sidecar_paths"]["delta_state_path"].as_str(),
        Some(path_string(&profile.delta_state_path).as_str())
    );
    let native_mapping_kind = if cfg!(windows) {
        "native_windows"
    } else {
        "native_unix"
    };
    assert_eq!(
        config["path_mapping"]["paths"]["repo_root"]["mapping_kind"].as_str(),
        Some(native_mapping_kind)
    );
    assert_eq!(
        config["path_mapping"]["paths"]["db_path"]["mapping_status"].as_str(),
        Some("ok")
    );
    assert_eq!(
        config["path_mapping"]["warnings"]
            .as_array()
            .expect("path mapping warnings")
            .len(),
        0
    );
    assert_eq!(
        config["mcp_config_identity"]["repo_identity_hash"].as_str(),
        Some(profile.repo_identity_hash.as_str())
    );
    assert_eq!(
        config["mcp_config_identity"]["db_path"].as_str(),
        config["db_path"].as_str()
    );
    assert_eq!(
        config["mcp_config_identity"]["sidecar_paths"]["candidate_spool_path"].as_str(),
        config["sidecar_paths"]["candidate_spool_path"].as_str()
    );
    assert_eq!(
        config["mcp_config_identity"]["path_mapping"]["paths"]["repo_root"]["mapping_kind"]
            .as_str(),
        Some(native_mapping_kind)
    );
    assert_eq!(
        config["mcp_config_identity"]["config_pins_repo"].as_bool(),
        Some(true)
    );
    assert_eq!(
        config["mcp_config_identity"]["config_pins_db"].as_bool(),
        Some(true)
    );
    assert_eq!(
        config["mcp_config_identity"]["config_pins_profile"].as_bool(),
        Some(true)
    );
    assert_eq!(
        config["mcp_config_identity"]["safe_read_only_startup"].as_bool(),
        Some(true)
    );
    assert_eq!(
        config["mcp_config_identity"]["auto_index_on_startup"].as_bool(),
        Some(false)
    );
    assert_eq!(
        config["mcp_config"]["mcpServers"]["codegraph-mcp"]["args"][0].as_str(),
        Some("--repo")
    );
    assert_eq!(
        config["mcp_config"]["mcpServers"]["codegraph-mcp"]["env"]["CODEGRAPH_DB_PATH"],
        config["db_path"]
    );
    assert_eq!(
        config["mcp_config"]["mcpServers"]["codegraph-mcp"]["env"]["CODEGRAPH_AGENT_USE_PROFILE"]
            .as_str(),
        Some(super::PRODUCTION_AGENT_USE_PROFILE_NAME)
    );
    assert_eq!(
        config["mcp_config"]["mcpServers"]["codegraph-mcp"]["env"]
            ["CODEGRAPH_AGENT_USE_REPO_IDENTITY_HASH"]
            .as_str(),
        Some(profile.repo_identity_hash.as_str())
    );
    assert_eq!(
        config["mcp_config"]["mcpServers"]["codegraph-mcp"]["env"]
            ["CODEGRAPH_AGENT_USE_PROFILE_ROOT"]
            .as_str(),
        Some(path_string(&profile.profile_root).as_str())
    );
    assert_eq!(config["mcp_startup_auto_index"].as_bool(), Some(false));
    assert_eq!(config["auto_index_on_startup"].as_bool(), Some(false));
    assert_eq!(config["safe_read_only_startup"].as_bool(), Some(true));
    assert_eq!(
        config["startup_policy"]["missing_db_claims_ready"].as_bool(),
        Some(false)
    );
    assert_eq!(
        config["startup_policy"]["stale_db_claims_ready"].as_bool(),
        Some(false)
    );
    assert_eq!(
        config["mcp_no_dot_codegraph_fallback"].as_bool(),
        Some(true)
    );
    assert_eq!(
        config["output_policy"]["default_stdout_json_only"].as_bool(),
        Some(true)
    );
    assert_eq!(
        config["output_policy"]["write_mode_supported"].as_bool(),
        Some(false)
    );
    assert!(config["output_policy"]["output_path"].is_null());
    assert!(config["recovery_commands"]
        .as_array()
        .expect("recovery commands")
        .iter()
        .any(|command| command
            .as_str()
            .is_some_and(|value| value.contains("agent-use index"))));
    assert!(!profile.profile_root.exists());

    let config_a = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "mcp-config".to_string(),
            "--repo".to_string(),
            path_string(&repo_a),
            "--json".to_string(),
        ])
    })
    .expect("config a");
    let config_b = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "mcp-config".to_string(),
            "--repo".to_string(),
            path_string(&repo_b),
            "--json".to_string(),
        ])
    })
    .expect("config b");
    let config_unicode = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "mcp-config".to_string(),
            "--repo".to_string(),
            path_string(&unicode_repo),
            "--json".to_string(),
        ])
    })
    .expect("unicode config");
    assert_ne!(config_a["db_path"], config_b["db_path"]);
    assert_ne!(config_a["profile_root"], config_b["profile_root"]);
    assert_ne!(
        config_a["repo_identity_hash"],
        config_b["repo_identity_hash"]
    );
    assert_ne!(
        config_a["sidecar_paths"]["candidate_spool_path"],
        config_b["sidecar_paths"]["candidate_spool_path"]
    );
    assert_ne!(
        config_a["sidecar_paths"]["vector_runtime_path"],
        config_b["sidecar_paths"]["vector_runtime_path"]
    );
    assert_eq!(
        config_b["mcp_config_identity"]["repo_identity_hash"].as_str(),
        Some(profile_b.repo_identity_hash.as_str())
    );
    assert_eq!(
        config_unicode["mcp_config_identity"]["repo_identity_hash"].as_str(),
        Some(profile_unicode.repo_identity_hash.as_str())
    );
    assert!(config_unicode["repo_root"]
        .as_str()
        .expect("unicode repo root")
        .contains("repo with spaces"));
    assert!(config_unicode["mcp_server_args"][1]
        .as_str()
        .expect("unicode mcp repo arg")
        .contains("repo with spaces"));
    let serialized_config =
        serde_json::to_string(&config_unicode).expect("serialize unicode mcp config");
    let reparsed_config: Value =
        serde_json::from_str(&serialized_config).expect("reparse unicode mcp config");
    assert_eq!(reparsed_config["repo_root"], config_unicode["repo_root"]);
    assert_no_dot_codegraph_sqlite(&repo_a);
    assert_no_dot_codegraph_sqlite(&repo_b);
    assert_no_dot_codegraph_sqlite(&unicode_repo);

    remove_dir_all_with_retry(&parent, "cleanup parent");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_path_mapping_classifies_wsl_docker_and_native() {
    let native = super::agent_use_path_mapping_json(&std::env::temp_dir());
    assert_eq!(native["mapping_status"].as_str(), Some("ok"));
    assert_eq!(
        native["mapping_kind"].as_str(),
        Some(if cfg!(windows) {
            "native_windows"
        } else {
            "native_unix"
        })
    );

    let wsl = super::agent_use_path_mapping_json(Path::new(r"\\wsl$\Ubuntu\home\repo"));
    assert_eq!(wsl["mapping_kind"].as_str(), Some("wsl_path"));
    assert_eq!(wsl["mapping_status"].as_str(), Some("mapping_unavailable"));
    assert_eq!(wsl["mapping_unavailable"].as_bool(), Some(true));
    assert_eq!(
        wsl["warnings"][0]["label"].as_str(),
        Some("path_mapping_unavailable")
    );

    let docker = super::agent_use_path_mapping_json(Path::new("/workspace/repo"));
    assert_eq!(docker["mapping_kind"].as_str(), Some("docker_mount_path"));
    assert_eq!(
        docker["mapping_status"].as_str(),
        Some("mapping_unavailable")
    );
}

#[test]
fn agent_use_status_reports_unsafe_dbs_without_migration() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo_a = temp_repo();
    let repo_b = temp_repo();
    write_agent_use_context_fixture(&repo_a);
    write_agent_use_context_fixture(&repo_b);
    let profile_a =
        super::resolve_agent_use_profile_with_data_root(&repo_a, &data_root).expect("profile a");
    let profile_b =
        super::resolve_agent_use_profile_with_data_root(&repo_b, &data_root).expect("profile b");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo_a),
            "--json".to_string(),
        ])
    })
    .expect("index repo a");
    fs::create_dir_all(&profile_b.profile_root).expect("create profile b parent");
    fs::copy(&profile_a.db_path, &profile_b.db_path).expect("copy foreign DB");

    let foreign = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo_b),
            "--json".to_string(),
        ])
    })
    .expect("foreign status");
    assert_eq!(foreign["claimable"].as_bool(), Some(false));
    assert_eq!(
        foreign["db_problem_kind"].as_str(),
        Some("repo_root_mismatch")
    );

    let repo_old = temp_repo();
    write_agent_use_context_fixture(&repo_old);
    let profile_old = super::resolve_agent_use_profile_with_data_root(&repo_old, &data_root)
        .expect("old profile");
    fs::create_dir_all(&profile_old.profile_root).expect("create old profile parent");
    {
        let connection = Connection::open(&profile_old.db_path).expect("open old schema");
        connection
            .execute_batch(
                "
                    PRAGMA user_version = 1;
                    CREATE TABLE legacy_only(id INTEGER PRIMARY KEY);
                    INSERT INTO legacy_only(id) VALUES (1);
                    ",
            )
            .expect("create old schema");
    }
    let before_bytes = fs::read(&profile_old.db_path).expect("read old DB");
    let old = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo_old),
            "--json".to_string(),
        ])
    })
    .expect("old status");
    let after_bytes = fs::read(&profile_old.db_path).expect("read old DB after");
    assert_eq!(old["claimable"].as_bool(), Some(false));
    assert_eq!(old["db_problem_kind"].as_str(), Some("schema_mismatch"));
    assert_eq!(before_bytes, after_bytes);
    let replaced_old = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo_old),
            "--json".to_string(),
        ])
    })
    .expect("replace old schema through agent-use index");
    assert_eq!(replaced_old["status"].as_str(), Some("indexed"));
    assert_eq!(replaced_old["claimable"].as_bool(), Some(true));
    assert_eq!(replaced_old["external_db_used"].as_bool(), Some(true));

    let repo_stale = temp_repo();
    write_agent_use_context_fixture(&repo_stale);
    let profile_stale = super::resolve_agent_use_profile_with_data_root(&repo_stale, &data_root)
        .expect("stale profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo_stale),
            "--json".to_string(),
        ])
    })
    .expect("index stale repo");
    {
        let connection = Connection::open(&profile_stale.db_path).expect("open stale DB");
        connection
            .execute(
                "UPDATE codegraph_db_passport SET last_run_status = 'interrupted' WHERE id = 1",
                [],
            )
            .expect("mark stale");
    }
    let stale_status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo_stale),
            "--json".to_string(),
        ])
    })
    .expect("stale status");
    assert_eq!(stale_status["claimable"].as_bool(), Some(false));
    assert_eq!(
        stale_status["db_lifecycle_read"]["artifact_freshness"].as_str(),
        Some("incomplete:interrupted")
    );
    let stale_config = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "mcp-config".to_string(),
            "--repo".to_string(),
            path_string(&repo_stale),
            "--json".to_string(),
        ])
    })
    .expect("stale mcp-config");
    assert_ne!(stale_config["status"].as_str(), Some("ok"));
    assert_eq!(stale_config["claimable"].as_bool(), Some(false));
    assert_eq!(stale_config["safe_read_only_startup"].as_bool(), Some(true));
    assert_eq!(stale_config["auto_index_on_startup"].as_bool(), Some(false));
    assert_eq!(
        stale_config["startup_policy"]["stale_db_claims_ready"].as_bool(),
        Some(false)
    );
    assert!(stale_config["recovery_commands"]
        .as_array()
        .expect("stale recovery commands")
        .iter()
        .any(|command| command
            .as_str()
            .is_some_and(|value| value.contains("agent-use index"))));
    let rebuilt_stale = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo_stale),
            "--json".to_string(),
        ])
    })
    .expect("rebuild stale profile");
    assert_eq!(rebuilt_stale["claimable"].as_bool(), Some(true));
    assert_no_dot_codegraph_sqlite(&repo_a);
    assert_no_dot_codegraph_sqlite(&repo_b);
    assert_no_dot_codegraph_sqlite(&repo_old);
    assert_no_dot_codegraph_sqlite(&repo_stale);

    remove_dir_all_with_retry(&repo_a, "cleanup repo a");
    remove_dir_all_with_retry(&repo_b, "cleanup repo b");
    remove_dir_all_with_retry(&repo_old, "cleanup old repo");
    remove_dir_all_with_retry(&repo_stale, "cleanup stale repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_profile_durability_labels_lock_sidecars_permission_and_publish_state() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");

    let missing_status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("missing status");
    assert_eq!(missing_status["status"].as_str(), Some("not_indexed"));
    assert_json_array_contains(&missing_status, "safety_labels", "not_indexed");
    assert_json_array_contains(&missing_status, "safety_labels", "diagnostic_only");

    let missing_config = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "mcp-config".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("missing mcp-config");
    assert_eq!(missing_config["status"].as_str(), Some("not_indexed"));
    assert_json_array_contains(&missing_config, "safety_labels", "not_indexed");
    assert!(!profile.profile_root.exists());

    let permission_repo = temp_repo();
    write_agent_use_context_fixture(&permission_repo);
    let permission_profile =
        super::resolve_agent_use_profile_with_data_root(&permission_repo, &data_root)
            .expect("permission profile");
    let permission_error = {
        let _failpoint = BundleFailpointEnvGuard::set(
            super::AGENT_USE_PROFILE_PARENT_PERMISSION_DENIED_FAILPOINT,
        );
        with_agent_use_data_root(&data_root, || {
            super::run_agent_use_command(&[
                "index".to_string(),
                "--repo".to_string(),
                path_string(&permission_repo),
                "--json".to_string(),
            ])
        })
    }
    .expect_err("permission failpoint must block profile parent creation");
    let permission: Value = serde_json::from_str(&permission_error).expect("permission error JSON");
    assert_eq!(permission["status"].as_str(), Some("permission_denied"));
    assert_json_array_contains(&permission, "safety_labels", "permission_denied");
    assert!(!permission_profile.profile_root.exists());

    let filesystem_repo = temp_repo();
    write_agent_use_context_fixture(&filesystem_repo);
    let filesystem_profile =
        super::resolve_agent_use_profile_with_data_root(&filesystem_repo, &data_root)
            .expect("filesystem profile");
    let filesystem_error = {
        let _failpoint = BundleFailpointEnvGuard::set(
            super::AGENT_USE_PROFILE_PARENT_FILESYSTEM_INACCESSIBLE_FAILPOINT,
        );
        with_agent_use_data_root(&data_root, || {
            super::run_agent_use_command(&[
                "index".to_string(),
                "--repo".to_string(),
                path_string(&filesystem_repo),
                "--json".to_string(),
            ])
        })
    }
    .expect_err("filesystem failpoint must block profile parent creation");
    let filesystem: Value = serde_json::from_str(&filesystem_error).expect("filesystem error JSON");
    assert_eq!(
        filesystem["status"].as_str(),
        Some("filesystem_inaccessible")
    );
    assert_json_array_contains(&filesystem, "safety_labels", "filesystem_inaccessible");
    assert!(!filesystem_profile.profile_root.exists());

    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    let wal_connection = keep_wal_sidecars_for_test(&profile.db_path);
    let sidecar_status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("status with sidecars");
    assert_eq!(sidecar_status["claimable"].as_bool(), Some(true));
    assert_eq!(sidecar_status["sidecar_status"].as_str(), Some("normal"));
    assert_eq!(sidecar_status["sidecar_only_change"].as_bool(), Some(true));
    assert_eq!(
        sidecar_status["sidecar_change_classification"].as_str(),
        Some("sidecar_only_change")
    );
    assert_json_array_contains(&sidecar_status, "safety_labels", "sidecar_only_change");
    drop(wal_connection);

    fs::write(
        &profile.lock_or_publish_state_path,
        serde_json::to_vec(&json!({
            "status": "publishing",
            "temp_db_claimability": "never_claimable"
        }))
        .expect("publish state JSON"),
    )
    .expect("write publish state");
    let publishing_query = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "symbols".to_string(),
            "agentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--agent-json".to_string(),
        ])
    })
    .expect("query while publish state exists");
    assert_eq!(publishing_query["claimable"].as_bool(), Some(true));
    assert_eq!(publishing_query["publishing"].as_bool(), Some(true));
    assert_json_array_contains(&publishing_query, "safety_labels", "publishing");
    let publishing_context = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("context while publish state exists");
    assert_eq!(publishing_context["claimable"].as_bool(), Some(true));
    assert_json_array_contains(&publishing_context, "safety_labels", "publishing");
    fs::remove_file(&profile.lock_or_publish_state_path).expect("clear publish state");

    let lock = Connection::open(&profile.db_path).expect("open lock connection");
    lock.busy_timeout(Duration::from_millis(0))
        .expect("set busy timeout");
    let _: String = lock
        .query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))
        .expect("switch to rollback journal");
    lock.execute_batch(
        "
            PRAGMA locking_mode=EXCLUSIVE;
            BEGIN EXCLUSIVE;
            CREATE TABLE IF NOT EXISTS lock_marker(id INTEGER PRIMARY KEY);
            INSERT INTO lock_marker(id) VALUES (1);
            ",
    )
    .expect("hold exclusive write lock");

    let locked_status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("locked status");
    assert_eq!(locked_status["status"].as_str(), Some("db_locked"));
    assert_eq!(
        locked_status["lock_state"]["db_locked"].as_bool(),
        Some(true)
    );
    assert_eq!(
        locked_status["lock_state"]["retryable"].as_bool(),
        Some(true)
    );
    assert_eq!(
        locked_status["update_queue_state"]["old_db_preserved"].as_bool(),
        Some(true)
    );
    assert_json_array_contains(&locked_status, "safety_labels", "db_locked");
    assert_json_array_contains(&locked_status, "safety_labels", "diagnostic_only");

    let locked_query = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "symbols".to_string(),
            "agentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--agent-json".to_string(),
        ])
    })
    .expect("locked query");
    assert_eq!(locked_query["claimable"].as_bool(), Some(false));
    assert_json_array_contains(&locked_query, "safety_labels", "db_locked");

    let locked_context = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("locked context-pack");
    assert_eq!(locked_context["claimable"].as_bool(), Some(false));
    assert_json_array_contains(&locked_context, "safety_labels", "db_locked");

    let locked_index = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect_err("locked DB must block agent-use index");
    assert!(
        locked_index.contains("db_locked") || locked_index.contains("locked"),
        "{locked_index}"
    );
    lock.execute_batch("ROLLBACK").expect("release lock");
    drop(lock);

    assert_no_dot_codegraph_sqlite(&repo);
    assert_no_dot_codegraph_sqlite(&permission_repo);
    assert_no_dot_codegraph_sqlite(&filesystem_repo);
    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&permission_repo, "cleanup permission repo");
    remove_dir_all_with_retry(&filesystem_repo, "cleanup filesystem repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_config_discovery_reports_missing_invalid_and_unknown_fields() {
    let _guard = lock_env_test();
    let repo = temp_repo();

    let missing = super::agent_use_config_discovery_json(&repo);
    assert_eq!(missing["status"].as_str(), Some("config_missing"));
    assert_eq!(missing["required"].as_bool(), Some(false));
    assert_eq!(missing["diagnostic_only"].as_bool(), Some(true));

    let config_dir = repo.join(".codex");
    fs::create_dir_all(&config_dir).expect("create config dir");
    let config_path = config_dir.join("config.toml");
    fs::write(
        &config_path,
        "[mcp_servers.codegraph-mcp\ncommand = \"codegraph-mcp\"\n",
    )
    .expect("write invalid config");
    let invalid = super::agent_use_config_discovery_json(&repo);
    assert_eq!(invalid["status"].as_str(), Some("config_invalid"));
    assert!(
        invalid["errors"]
            .as_array()
            .expect("errors")
            .iter()
            .any(|error| error
                .as_str()
                .unwrap_or_default()
                .contains("config_invalid")),
        "{invalid:?}"
    );

    fs::write(
            &config_path,
            "[mcp_servers.codegraph-mcp]\ncommand = \"codegraph-mcp\"\nargs = [\"serve-mcp\"]\ncwd = \".\"\nsurprise = true\n",
        )
        .expect("write config with unknown field");
    let unknown = super::agent_use_config_discovery_json(&repo);
    assert_eq!(unknown["status"].as_str(), Some("config_unknown_field"));
    assert!(
        unknown["unknown_fields"]
            .as_array()
            .expect("unknown fields")
            .iter()
            .any(|field| field.as_str() == Some("surprise")),
        "{unknown:?}"
    );
    assert_eq!(unknown["unknown_field_policy"].as_str(), Some("warning"));

    assert_no_dot_codegraph_sqlite(&repo);
    remove_dir_all_with_retry(&repo, "cleanup repo");
}

#[test]
fn agent_use_env_discovery_labels_invalid_and_preserves_space_unicode_root() {
    let _guard = lock_env_test();
    let parent = temp_repo();
    let data_root = parent.join(format!("data root with spaces {}", '\u{00e9}'));
    let repo = parent.join("repo");
    fs::create_dir_all(&repo).expect("create repo");

    let invalid = with_process_env_var(super::AGENT_USE_DATA_ROOT_ENV, "", || {
        super::agent_use_profile_data_root()
    })
    .expect_err("empty env override must be invalid");
    assert!(invalid.contains("env_invalid"), "{invalid}");

    let profile = with_agent_use_data_root(&data_root, || super::resolve_agent_use_profile(&repo))
        .expect("profile");
    let env =
        with_agent_use_data_root(&data_root, || super::agent_use_env_discovery_json(&profile));
    assert_eq!(env["explicit_data_root"]["status"].as_str(), Some("ok"));
    assert_eq!(env["data_root_source"].as_str(), Some("env"));
    assert_eq!(
        env["explicit_data_root"]["path"].as_str(),
        Some(path_string(&data_root).as_str())
    );
    assert_eq!(env["profile_root_ref"].as_str(), Some("profile_root"));
    assert!(
        path_string(&profile.profile_root).contains("data root with spaces"),
        "{profile:?}"
    );

    assert_no_dot_codegraph_sqlite(&repo);
    remove_dir_all_with_retry(&parent, "cleanup parent");
}

#[test]
fn agent_use_sidecar_access_distinguishes_access_from_corrupt() {
    let _guard = lock_env_test();
    assert_eq!(
        super::sidecar_access_classification_from_message("permission denied").status,
        "permission_denied"
    );
    assert_eq!(
        super::sidecar_access_classification_from_message("database is locked").status,
        "sidecar_locked"
    );
    assert_eq!(
        super::sidecar_access_classification_from_message("attempt to write a readonly database")
            .status,
        "read_only"
    );
    assert_eq!(
        super::sidecar_access_classification_from_message("failed to parse vector chunk index")
            .status,
        "sidecar_corrupt"
    );

    let repo = temp_repo();
    let spool_path = repo.join("candidate-spool.jsonl");
    fs::create_dir_all(&spool_path).expect("directory at spool path");
    let spool = super::candidate_spool_layer_status(&repo, &spool_path);
    assert_ne!(spool["status"].as_str(), Some("corrupt"));
    assert_ne!(spool["status"].as_str(), Some("foreign"));
    assert_eq!(spool["candidate_spool_unavailable"].as_bool(), Some(true));

    let audit_dir = repo.join("vector-audit.json");
    fs::create_dir_all(&audit_dir).expect("directory at audit path");
    let audit_access =
        super::vector_audit_layer_status(&repo.join("graph.sqlite"), None, &audit_dir);
    assert_ne!(audit_access["status"].as_str(), Some("corrupt"));
    assert_eq!(audit_access["runtime_dependency"].as_bool(), Some(false));
    assert_eq!(audit_access["diagnostic_only"].as_bool(), Some(true));

    fs::remove_dir_all(&audit_dir).expect("remove audit dir");
    fs::write(&audit_dir, "{not json").expect("write corrupt audit json");
    let audit_corrupt =
        super::vector_audit_layer_status(&repo.join("graph.sqlite"), None, &audit_dir);
    assert_eq!(audit_corrupt["status"].as_str(), Some("sidecar_corrupt"));
    assert_eq!(
        audit_corrupt["sidecar_problem_kind"].as_str(),
        Some("sidecar_corrupt")
    );

    assert_no_dot_codegraph_sqlite(&repo);
    remove_dir_all_with_retry(&repo, "cleanup repo");
}

#[test]
fn agent_use_artifact_hygiene_policy_marks_raw_artifacts_local() {
    let hygiene = super::agent_use_artifact_hygiene_json();
    assert_eq!(hygiene["status"].as_str(), Some("local_ignored"));
    assert_eq!(
        hygiene["reports_audit_local_evidence"].as_bool(),
        Some(true)
    );
    assert_eq!(
        hygiene["reports_final_canonical_dashboard"].as_bool(),
        Some(true)
    );
    assert_eq!(hygiene["raw_artifacts_not_promoted"].as_bool(), Some(true));
    assert_eq!(hygiene["ignored_local_pattern_count"].as_u64(), Some(9));
    assert_eq!(
        hygiene["ignored_local_patterns_ref"].as_str(),
        Some("docs/guardrails.md")
    );
}

#[test]
fn agent_use_readers_see_old_good_db_during_uncommitted_delta_update() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index");

    let writer = SqliteGraphStore::open(&profile.db_path).expect("open writer");
    writer
        .begin_write_transaction()
        .expect("begin uncommitted update transaction");
    writer
        .delete_facts_for_file("src/service.ts")
        .expect("delete facts inside uncommitted transaction");
    super::write_agent_use_publish_state(&profile, "updating", None).expect("write updating state");

    let query_during_update = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "symbols".to_string(),
            "agentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("query during uncommitted update");
    assert_eq!(query_during_update["claimable"].as_bool(), Some(true));
    assert_eq!(
        query_during_update["publish_state"]["status"].as_str(),
        Some("updating")
    );
    assert_eq!(
        query_during_update["publish_state"]["updating"].as_bool(),
        Some(true)
    );
    assert_json_array_contains(&query_during_update, "safety_labels", "updating");
    assert_json_array_contains(&query_during_update, "safety_labels", "publishing");
    assert!(
            query_during_update["result_count"]
                .as_u64()
                .unwrap_or_default()
                > 0,
            "reader must see old committed graph facts, not the uncommitted deletion: {query_during_update:?}"
        );

    let context_during_update = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("context-pack during uncommitted update");
    assert_eq!(context_during_update["claimable"].as_bool(), Some(true));
    assert_eq!(
        context_during_update["publish_state"]["status"].as_str(),
        Some("updating")
    );
    assert_json_array_contains(&context_during_update, "safety_labels", "updating");
    assert_json_array_contains(&context_during_update, "safety_labels", "publishing");

    let status_during_update = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("status during uncommitted update");
    assert_eq!(status_during_update["claimable"].as_bool(), Some(true));
    assert_eq!(
        status_during_update["publish_state"]["status"].as_str(),
        Some("updating")
    );
    assert_eq!(
        status_during_update["lock_state"]["active_update"].as_bool(),
        Some(true)
    );
    assert_eq!(
        status_during_update["lock_state"]["unrelated_repo_blocking"].as_bool(),
        Some(false)
    );
    assert_eq!(
        status_during_update["update_queue_state"]["status"].as_str(),
        Some("updating")
    );
    assert_eq!(
        status_during_update["update_queue_state"]["old_db_preserved"].as_bool(),
        Some(true)
    );
    assert_json_array_contains(&status_during_update, "safety_labels", "updating");
    assert_json_array_contains(&status_during_update, "safety_labels", "publishing");

    writer
        .rollback_write_transaction()
        .expect("rollback uncommitted update transaction");
    drop(writer);
    super::clear_agent_use_publish_state(&profile).expect("clear updating state");

    let query_after_rollback = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "symbols".to_string(),
            "agentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("query after rollback");
    assert!(
        query_after_rollback["result_count"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "rollback must preserve old-good facts: {query_after_rollback:?}"
    );
    assert_eq!(
        query_after_rollback["publish_state"]["status"].as_str(),
        Some("absent")
    );
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_multi_repo_local_agents_do_not_cross_contaminate_during_update() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let parent = temp_repo();
    let repo_a = parent.join("left").join("app");
    let repo_b = parent.join("right").join("app");
    fs::create_dir_all(&repo_a).expect("create repo a");
    fs::create_dir_all(&repo_b).expect("create repo b");
    write_cli_fixture_file(&repo_a, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
        &repo_a,
        "src/a.ts",
        "export function repoAOnlyLocalAgentSymbol() {\n  return 'repo-a';\n}\n",
    );
    write_cli_fixture_file(&repo_b, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
        &repo_b,
        "src/b.ts",
        "export function repoBOnlyLocalAgentSymbol() {\n  return 'repo-b';\n}\n",
    );
    let profile_a =
        super::resolve_agent_use_profile_with_data_root(&repo_a, &data_root).expect("profile a");
    let profile_b =
        super::resolve_agent_use_profile_with_data_root(&repo_b, &data_root).expect("profile b");
    assert_ne!(profile_a.db_path, profile_b.db_path);
    assert_ne!(
        profile_a.candidate_spool_path,
        profile_b.candidate_spool_path
    );
    assert_ne!(profile_a.vector_runtime_path, profile_b.vector_runtime_path);

    run_agent_use_test_command(
        &data_root,
        &["index", "--repo", path_string(&repo_a).as_str(), "--json"],
    )
    .expect("index repo a");
    run_agent_use_test_command(
        &data_root,
        &["index", "--repo", path_string(&repo_b).as_str(), "--json"],
    )
    .expect("index repo b");

    assert!(agent_use_query_count(&data_root, &repo_a, "symbols", "repoAOnlyLocalAgentSymbol") > 0);
    assert_eq!(
        agent_use_query_count(&data_root, &repo_b, "symbols", "repoAOnlyLocalAgentSymbol"),
        0
    );
    assert!(agent_use_query_count(&data_root, &repo_b, "symbols", "repoBOnlyLocalAgentSymbol") > 0);
    assert_eq!(
        agent_use_query_count(&data_root, &repo_a, "symbols", "repoBOnlyLocalAgentSymbol"),
        0
    );

    let config_a = run_agent_use_test_command(
        &data_root,
        &[
            "mcp-config",
            "--repo",
            path_string(&repo_a).as_str(),
            "--json",
        ],
    )
    .expect("config a");
    let config_b = run_agent_use_test_command(
        &data_root,
        &[
            "mcp-config",
            "--repo",
            path_string(&repo_b).as_str(),
            "--json",
        ],
    )
    .expect("config b");
    assert_ne!(config_a["db_path"], config_b["db_path"]);
    assert_ne!(
        config_a["mcp_config_identity"]["repo_identity_hash"],
        config_b["mcp_config_identity"]["repo_identity_hash"]
    );
    assert_ne!(
        config_a["sidecar_paths"]["candidate_spool_path"],
        config_b["sidecar_paths"]["candidate_spool_path"]
    );

    let writer = SqliteGraphStore::open(&profile_a.db_path).expect("open repo a writer");
    writer
        .begin_write_transaction()
        .expect("begin repo a uncommitted update");
    writer
        .delete_facts_for_file("src/a.ts")
        .expect("delete repo a facts inside uncommitted transaction");
    super::write_agent_use_publish_state(&profile_a, "updating", None)
        .expect("write repo a updating state");

    let status_a = run_agent_use_test_command(
        &data_root,
        &["status", "--repo", path_string(&repo_a).as_str(), "--json"],
    )
    .expect("repo a status during update");
    assert_eq!(status_a["claimable"].as_bool(), Some(true));
    assert_eq!(
        status_a["publish_state"]["status"].as_str(),
        Some("updating")
    );
    assert_eq!(
        status_a["lock_state"]["active_update"].as_bool(),
        Some(true)
    );
    assert_eq!(
        status_a["lock_state"]["unrelated_repo_blocking"].as_bool(),
        Some(false)
    );
    assert_eq!(
        status_a["update_queue_state"]["old_db_preserved"].as_bool(),
        Some(true)
    );

    let query_a_during_update = run_agent_use_test_command(
        &data_root,
        &[
            "query",
            "symbols",
            "repoAOnlyLocalAgentSymbol",
            "--repo",
            path_string(&repo_a).as_str(),
            "--limit",
            "5",
            "--agent-json",
        ],
    )
    .expect("repo a query during update");
    assert_eq!(query_a_during_update["claimable"].as_bool(), Some(true));
    assert!(
            query_a_during_update["result_count"]
                .as_u64()
                .unwrap_or_default()
                > 0,
            "repo A reader must see old committed facts, not half-updated graph proof: {query_a_during_update:?}"
        );

    let status_b = run_agent_use_test_command(
        &data_root,
        &["status", "--repo", path_string(&repo_b).as_str(), "--json"],
    )
    .expect("repo b status during repo a update");
    assert_eq!(status_b["claimable"].as_bool(), Some(true));
    assert_eq!(status_b["publish_state"]["active"].as_bool(), Some(false));
    assert_eq!(
        status_b["lock_state"]["active_update"].as_bool(),
        Some(false)
    );
    assert_eq!(
        status_b["lock_state"]["unrelated_repo_blocking"].as_bool(),
        Some(false)
    );
    assert!(agent_use_query_count(&data_root, &repo_b, "symbols", "repoBOnlyLocalAgentSymbol") > 0);
    assert_eq!(
        agent_use_query_count(&data_root, &repo_b, "symbols", "repoAOnlyLocalAgentSymbol"),
        0
    );

    writer
        .rollback_write_transaction()
        .expect("rollback repo a update");
    drop(writer);
    super::clear_agent_use_publish_state(&profile_a).expect("clear repo a update state");

    assert_no_dot_codegraph_sqlite(&repo_a);
    assert_no_dot_codegraph_sqlite(&repo_b);
    remove_dir_all_with_retry(&parent, "cleanup parent");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_context_pack_refuses_unsafe_profile_db_without_fallback() {
    let _guard = lock_env_test();
    let data_root = temp_repo();

    let repo_stale = temp_repo();
    write_agent_use_context_fixture(&repo_stale);
    let profile_stale = super::resolve_agent_use_profile_with_data_root(&repo_stale, &data_root)
        .expect("stale profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo_stale),
            "--json".to_string(),
        ])
    })
    .expect("index stale repo");
    Connection::open(&profile_stale.db_path)
        .expect("open stale DB")
        .execute(
            "UPDATE codegraph_db_passport SET last_run_status = 'interrupted' WHERE id = 1",
            [],
        )
        .expect("mark stale");
    let stale = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo_stale),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("stale context blocked");
    assert_eq!(stale["claimable"].as_bool(), Some(false));
    assert_json_array_contains(&stale, "safety_labels", "stale");

    let repo_valid = temp_repo();
    let repo_foreign = temp_repo();
    write_agent_use_context_fixture(&repo_valid);
    write_agent_use_context_fixture(&repo_foreign);
    let profile_valid = super::resolve_agent_use_profile_with_data_root(&repo_valid, &data_root)
        .expect("valid profile");
    let profile_foreign =
        super::resolve_agent_use_profile_with_data_root(&repo_foreign, &data_root)
            .expect("foreign profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo_valid),
            "--json".to_string(),
        ])
    })
    .expect("index valid repo");
    fs::create_dir_all(&profile_foreign.profile_root).expect("create foreign parent");
    fs::copy(&profile_valid.db_path, &profile_foreign.db_path).expect("copy foreign DB");
    let foreign = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo_foreign),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("foreign context blocked");
    assert_eq!(foreign["claimable"].as_bool(), Some(false));
    assert_json_array_contains(&foreign, "safety_labels", "repo_mismatch");
    assert_json_array_contains(&foreign, "safety_labels", "foreign");

    let repo_old = temp_repo();
    write_agent_use_context_fixture(&repo_old);
    let profile_old = super::resolve_agent_use_profile_with_data_root(&repo_old, &data_root)
        .expect("old profile");
    fs::create_dir_all(&profile_old.profile_root).expect("create old parent");
    Connection::open(&profile_old.db_path)
        .expect("open old DB")
        .execute_batch("PRAGMA user_version = 1; CREATE TABLE legacy_only(id INTEGER);")
        .expect("old schema");
    let old = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo_old),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("old schema context blocked");
    assert_eq!(old["claimable"].as_bool(), Some(false));
    assert_json_array_contains(&old, "safety_labels", "schema_mismatch");

    let repo_corrupt = temp_repo();
    write_agent_use_context_fixture(&repo_corrupt);
    let profile_corrupt =
        super::resolve_agent_use_profile_with_data_root(&repo_corrupt, &data_root)
            .expect("corrupt passport profile");
    fs::create_dir_all(&profile_corrupt.profile_root).expect("create corrupt parent");
    drop(SqliteGraphStore::open(&profile_corrupt.db_path).expect("initialize corrupt DB"));
    Connection::open(&profile_corrupt.db_path)
        .expect("open corrupt passport DB")
        .execute(
            "INSERT INTO codegraph_db_passport (
                    id, passport_version, codegraph_schema_version, storage_mode,
                    index_scope_policy_hash, scope_policy_json, canonical_repo_root,
                    source_discovery_policy_version, last_run_status, integrity_gate_result,
                    files_seen, files_indexed, created_at_unix_ms, updated_at_unix_ms
                ) VALUES (1, ?1, ?2, 'proof', 'scope-hash', '{}', 'repo',
                    'scope-v1', 'completed', 'ok', -1, 0, 1, 1)",
            rusqlite::params![super::DB_PASSPORT_VERSION, super::SCHEMA_VERSION],
        )
        .expect("insert corrupt passport row");
    let corrupt = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo_corrupt),
            "--json".to_string(),
        ])
    })
    .expect("corrupt passport status");
    assert_eq!(corrupt["claimable"].as_bool(), Some(false));
    assert_json_array_contains(&corrupt, "safety_labels", "passport_corrupt");

    assert_no_dot_codegraph_sqlite(&repo_stale);
    assert_no_dot_codegraph_sqlite(&repo_valid);
    assert_no_dot_codegraph_sqlite(&repo_foreign);
    assert_no_dot_codegraph_sqlite(&repo_old);
    assert_no_dot_codegraph_sqlite(&repo_corrupt);
    remove_dir_all_with_retry(&repo_stale, "cleanup stale repo");
    remove_dir_all_with_retry(&repo_valid, "cleanup valid repo");
    remove_dir_all_with_retry(&repo_foreign, "cleanup foreign repo");
    remove_dir_all_with_retry(&repo_old, "cleanup old repo");
    remove_dir_all_with_retry(&repo_corrupt, "cleanup corrupt repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn agent_use_interrupted_index_keeps_old_profile_db_claimable() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("initial index");

    let interrupt_error = {
        let _failpoint = BundleFailpointEnvGuard::set("cold_after_validation_before_publish");
        with_agent_use_data_root(&data_root, || {
            super::run_agent_use_command(&[
                "index".to_string(),
                "--repo".to_string(),
                path_string(&repo),
                "--fresh".to_string(),
                "--json".to_string(),
            ])
        })
    }
    .expect_err("publish failpoint must interrupt index");
    assert!(
        interrupt_error.contains("cold_after_validation_before_publish"),
        "{interrupt_error}"
    );
    assert!(profile.lock_or_publish_state_path.exists());

    let status = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "status".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("status after interrupted index");
    assert_eq!(status["status"].as_str(), Some("ok"));
    assert_eq!(status["claimable"].as_bool(), Some(true));
    assert_eq!(
        status["publish_state"]["status"].as_str(),
        Some("interrupted")
    );
    assert_json_array_contains(&status, "safety_labels", "publishing");
    assert_json_array_contains(&status, "safety_labels", "interrupted");
    assert_json_array_contains(&status, "safety_labels", "recovered");

    let query = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "query".to_string(),
            "symbols".to_string(),
            "agentUseTarget".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--agent-json".to_string(),
        ])
    })
    .expect("query after interrupted index");
    assert_eq!(query["claimable"].as_bool(), Some(true));
    assert_json_array_contains(&query, "safety_labels", "publishing");
    assert_json_array_contains(&query, "safety_labels", "interrupted");
    assert_json_array_contains(&query, "safety_labels", "recovered");

    let context = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "context-pack".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--task".to_string(),
            "Find agentUseTarget".to_string(),
            "--agent-json".to_string(),
        ])
    })
    .expect("context after interrupted index");
    assert_eq!(context["claimable"].as_bool(), Some(true));
    assert_json_array_contains(&context, "safety_labels", "publishing");
    assert_json_array_contains(&context, "safety_labels", "interrupted");
    assert_json_array_contains(&context, "safety_labels", "recovered");
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn plain_status_missing_db_guides_to_agent_use_without_redirect() {
    let _guard = lock_env_test();
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_context_fixture(&repo);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");

    let status = with_agent_use_data_root(&data_root, || run_status_command(&[path_string(&repo)]))
        .expect("plain status");
    let repo_root = fs::canonicalize(&repo).expect("canonical repo");

    assert_eq!(status["status"].as_str(), Some("not_indexed"));
    assert_eq!(
        status["db_path"].as_str(),
        Some(path_string(&default_db_path(&repo_root)).as_str())
    );
    assert_eq!(status["agent_use_available"].as_bool(), Some(true));
    assert!(status["agent_use_status_command"]
        .as_str()
        .expect("status command")
        .contains("agent-use status"));
    assert!(status["agent_use_index_command"]
        .as_str()
        .expect("index command")
        .contains("agent-use index"));
    assert_eq!(
        status["agent_use_profile_db_path"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    assert!(!profile.profile_root.exists());
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup repo");
    remove_dir_all_with_retry(&data_root, "cleanup data root");
}

#[test]
fn doctor_valid_db_reports_readonly_lifecycle_evidence() {
    let repo = ui_fixture_repo();
    index_repo(&repo).expect("index fixture");
    let repo_root = fs::canonicalize(&repo).expect("canonical repo");
    let db_path = default_db_path(&repo_root);

    let doctor = run_doctor_command(&[path_string(&repo), "--json".to_string()]).expect("doctor");

    assert_eq!(doctor["status"].as_str(), Some("ok"));
    assert_eq!(doctor["database_exists"].as_bool(), Some(true));
    assert_eq!(doctor["safe_to_query"].as_bool(), Some(true));
    assert_eq!(doctor["passport_status"].as_str(), Some("valid"));
    assert_eq!(doctor["path_access_status"].as_str(), Some("ok"));
    assert_eq!(doctor["db_path_outside_workspace"].as_bool(), Some(false));
    assert_eq!(doctor["repo_match"].as_bool(), Some(true));
    assert_eq!(doctor["scope_match"].as_bool(), Some(true));
    assert_eq!(doctor["schema_status"].as_str(), Some("ok"));
    assert_eq!(doctor["storage_mode"].as_str(), Some("proof"));
    let db_path_string = path_string(&db_path);
    assert_eq!(
        doctor["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
        Some(db_path_string.as_str())
    );
    assert_eq!(
        doctor["db_lifecycle_read"]["safe_to_read"].as_bool(),
        Some(true)
    );
    assert!(doctor["sqlite_sidecars"]["status"].is_string());

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn status_and_doctor_do_not_migrate_old_schema_db() {
    let repo = temp_repo();
    let db_path = default_db_path(&repo);
    fs::create_dir_all(db_path.parent().expect("db parent")).expect("create db parent");
    {
        let connection = Connection::open(&db_path).expect("open old schema");
        connection
            .execute_batch(
                "
                    PRAGMA user_version = 1;
                    CREATE TABLE legacy_only(id INTEGER PRIMARY KEY);
                    INSERT INTO legacy_only(id) VALUES (1);
                    ",
            )
            .expect("create old schema");
    }
    let before_bytes = fs::read(&db_path).expect("read old schema DB before inspection");
    let before_metadata = fs::metadata(&db_path).expect("metadata before inspection");

    let status = run_status_command(&[path_string(&repo)]).expect("status");
    let doctor = run_doctor_command(&[path_string(&repo), "--json".to_string()]).expect("doctor");

    let after_metadata = fs::metadata(&db_path).expect("metadata after inspection");
    let after_bytes = fs::read(&db_path).expect("read old schema DB after inspection");
    let user_version_after =
        Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open old schema read-only")
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .expect("user version");

    assert_eq!(status["status"].as_str(), Some("db_problem"));
    assert_eq!(status["db_problem_kind"].as_str(), Some("schema_mismatch"));
    assert_eq!(
        status["mvp4_micro_node_status"].as_str(),
        Some("incompatible")
    );
    assert_eq!(
        status["mvp4_micro_nodes"]["graph_claimability_unchanged"].as_bool(),
        Some(true)
    );
    assert_eq!(doctor["db_problem_kind"].as_str(), Some("schema_mismatch"));
    assert_eq!(
        doctor["mvp4_micro_node_status"].as_str(),
        Some("incompatible")
    );
    assert_eq!(doctor["safe_to_query"].as_bool(), Some(false));
    assert_eq!(user_version_after, 1);
    assert_eq!(before_bytes, after_bytes);
    assert_eq!(before_metadata.len(), after_metadata.len());
    assert_eq!(
        before_metadata.modified().expect("mtime before"),
        after_metadata.modified().expect("mtime after")
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn status_and_doctor_report_mvp4_micro_nodes_bounded_read_only() {
    let repo = temp_repo();
    write_cli_fixture_file(
        &repo,
        "src/service.ts",
        "export function handle(user: User, client: Client) {\n  const token = user.name;\n  let result = token;\n  result = client.send(token);\n  return result;\n}\n",
    );
    index_repo(&repo).expect("index TypeScript fixture");
    let db_path = default_db_path(&repo);
    let before_bytes = fs::read(&db_path).expect("read DB before status");

    let status = run_status_command(&[path_string(&repo)]).expect("status");
    let doctor = run_doctor_command(&[path_string(&repo), "--json".to_string()]).expect("doctor");
    let after_bytes = fs::read(&db_path).expect("read DB after status");

    assert_eq!(before_bytes, after_bytes);
    for surface in [&status, &doctor] {
        let micro = &surface["mvp4_micro_nodes"];
        assert_eq!(surface["mvp4_micro_node_status"].as_str(), Some("ready"));
        assert_eq!(micro["status"].as_str(), Some("ready"));
        assert_eq!(micro["ready"].as_bool(), Some(true));
        assert!(micro["total_rows"].as_u64().unwrap_or(0) > 0);
        assert!(
            micro["rows_by_node_kind"]["function_frame"]
                .as_u64()
                .unwrap_or(0)
                > 0
        );
        assert_eq!(micro["sample_count"].as_u64(), Some(0));
        assert!(micro.get("sample").is_none());
        assert_eq!(micro["default_full_table_scan"].as_bool(), Some(false));
        assert_eq!(micro["full_source_body_output"].as_bool(), Some(false));
        assert_eq!(
            micro["availability_separate_from_graph_claimability"].as_bool(),
            Some(true)
        );
        assert_eq!(
            micro["proof_boundary"]["not_micro_edge_or_flow_proof"].as_bool(),
            Some(true)
        );
        assert_eq!(
            micro["proof_boundary"]["mutation_proof_activated"].as_bool(),
            Some(false)
        );
        assert_eq!(
            micro["proof_boundary"]["flow_proof_activated"].as_bool(),
            Some(false)
        );
        assert!(surface["staged_availability"]["layer_readiness"]["mvp4_micro_nodes"].is_object());
    }

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn audit_micro_nodes_sample_is_bounded_read_only_and_not_proof() {
    let repo = temp_repo();
    write_cli_fixture_file(
        &repo,
        "src/service.ts",
        "export function handle(user: User, client: Client) {\n  const token = user.name;\n  return client.send(token);\n}\n",
    );
    index_repo(&repo).expect("index TypeScript fixture");
    let db_path = default_db_path(&repo);
    let before_bytes = fs::read(&db_path).expect("read DB before audit");

    let audit = super::audit::run_audit_command(&[
        "micro-nodes".to_string(),
        "--db".to_string(),
        path_string(&db_path),
        "--sample".to_string(),
        "2".to_string(),
    ])
    .expect("micro-node audit");
    let after_bytes = fs::read(&db_path).expect("read DB after audit");

    assert_eq!(before_bytes, after_bytes);
    assert_eq!(audit["status"].as_str(), Some("ready"));
    assert!(audit["total_rows"].as_u64().unwrap_or(0) > 0);
    let sample = audit["sample"].as_array().expect("bounded sample");
    assert!(!sample.is_empty());
    assert!(sample.len() <= 2);
    for row in sample {
        assert!(row["micro_node_id"].is_string());
        assert!(row["kind"].is_string());
        assert!(row["file"].is_string());
        assert!(row["source_span"].is_object());
        assert!(row.get("source_body").is_none());
        assert!(row.get("full_source").is_none());
        assert!(row.get("text").is_none());
    }
    assert_eq!(audit["default_full_table_scan"].as_bool(), Some(false));
    assert_eq!(audit["full_source_body_output"].as_bool(), Some(false));
    assert_eq!(
        audit["proof_boundary"]["not_micro_edge_or_flow_proof"].as_bool(),
        Some(true)
    );
    assert_eq!(
        audit["proof_boundary"]["route_auth_security_semantics"].as_bool(),
        Some(false)
    );
    assert_eq!(
        audit["artifact_mutated_during_inspection"].as_bool(),
        Some(false)
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn status_and_doctor_report_normal_sqlite_sidecars_for_valid_db() {
    let repo = ui_fixture_repo();
    index_repo(&repo).expect("index fixture");
    let db_path = default_db_path(&repo);
    let wal_connection = keep_wal_sidecars_for_test(&db_path);

    let status = run_status_command(&[path_string(&repo)]).expect("status");
    let doctor = run_doctor_command(&[path_string(&repo), "--json".to_string()]).expect("doctor");

    assert_eq!(status["status"].as_str(), Some("ok"));
    assert_eq!(doctor["status"].as_str(), Some("ok"));
    assert_eq!(status["graph_db_status"].as_str(), Some("ready"));
    assert_eq!(doctor["graph_db_status"].as_str(), Some("ready"));
    assert_eq!(status["graph_proof_available"].as_bool(), Some(true));
    assert_eq!(doctor["graph_proof_available"].as_bool(), Some(true));
    assert_eq!(status["candidate_spool_status"].as_str(), Some("no_spool"));
    assert_eq!(status["vector_runtime_status"].as_str(), Some("missing"));
    assert_eq!(status["sidecar_status"].as_str(), Some("normal"));
    assert_eq!(doctor["sidecar_status"].as_str(), Some("normal"));
    assert_eq!(status["telemetry"]["memory"].as_str(), Some("unknown"));
    assert_eq!(
        status["telemetry"]["memory_measured"].as_bool(),
        Some(false)
    );
    assert_eq!(doctor["telemetry"]["memory"].as_str(), Some("unknown"));
    assert_eq!(
        doctor["telemetry"]["memory_measured"].as_bool(),
        Some(false)
    );
    assert_eq!(
        status["sqlite_sidecars"]["sidecar_status"].as_str(),
        Some("normal")
    );
    assert_eq!(
        doctor["sqlite_sidecars"]["sidecar_status"].as_str(),
        Some("normal")
    );
    assert!(
        !status["sqlite_sidecars"]["sqlite_sidecars"]
            .as_array()
            .expect("status sidecars")
            .is_empty(),
        "{status:?}"
    );
    assert!(status["db_health"]["orphan_sidecars"]
        .as_array()
        .expect("deprecated orphan sidecars")
        .is_empty());
    assert!(doctor["sqlite_sidecars"]["orphan_sidecars"]
        .as_array()
        .expect("doctor deprecated orphan sidecars")
        .is_empty());

    drop(wal_connection);
    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn status_and_doctor_report_db_locked_without_mutating_main_db() {
    let repo = ui_fixture_repo();
    index_repo(&repo).expect("index fixture");
    let db_path = default_db_path(&repo);

    let lock = Connection::open(&db_path).expect("open lock connection");
    lock.busy_timeout(Duration::from_millis(0))
        .expect("set busy timeout");
    let _: String = lock
        .query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))
        .expect("switch to rollback journal");
    lock.execute_batch(
        "
            PRAGMA locking_mode=EXCLUSIVE;
            BEGIN EXCLUSIVE;
            CREATE TABLE IF NOT EXISTS lock_marker(id INTEGER PRIMARY KEY);
            INSERT INTO lock_marker(id) VALUES (1);
            ",
    )
    .expect("hold exclusive write lock");

    let before_bytes = fs::read(&db_path).expect("read locked DB before inspection");
    let before_metadata = fs::metadata(&db_path).expect("metadata before inspection");
    let status = run_status_command(&[path_string(&repo)]).expect("status");
    let doctor = run_doctor_command(&[path_string(&repo), "--json".to_string()]).expect("doctor");
    let mut cache = IncrementalIndexCache::new(256).expect("cache");
    let update_error =
        update_changed_files_with_cache(&repo, &[PathBuf::from("src/auth.ts")], &mut cache)
            .expect_err("locked DB must block incremental update");
    let after_metadata = fs::metadata(&db_path).expect("metadata after inspection");
    let after_bytes = fs::read(&db_path).expect("read locked DB after inspection");

    assert_eq!(status["status"].as_str(), Some("db_problem"));
    assert_eq!(status["db_problem_kind"].as_str(), Some("db_locked"));
    assert_eq!(status["path_access_status"].as_str(), Some("ok"));
    assert_eq!(doctor["db_problem_kind"].as_str(), Some("db_locked"));
    assert_eq!(doctor["safe_to_query"].as_bool(), Some(false));
    assert!(
        update_error.to_string().contains("db_locked")
            || update_error.to_string().contains("locked"),
        "{update_error}"
    );
    assert_eq!(before_bytes, after_bytes);
    assert_eq!(before_metadata.len(), after_metadata.len());
    assert_eq!(
        before_metadata.modified().expect("mtime before"),
        after_metadata.modified().expect("mtime after")
    );

    lock.execute_batch("ROLLBACK").expect("release lock");
    drop(lock);
    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn status_and_doctor_report_orphan_without_main_db_for_sidecars_only() {
    let repo = temp_repo();
    let db_path = default_db_path(&repo);
    fs::create_dir_all(db_path.parent().expect("db parent")).expect("create db parent");
    fs::write(sqlite_sidecar_path_for_test(&db_path, "wal"), "stale wal").expect("write stale wal");
    fs::write(sqlite_sidecar_path_for_test(&db_path, "shm"), "stale shm").expect("write stale shm");

    let status = run_status_command(&[path_string(&repo)]).expect("status");
    let doctor = run_doctor_command(&[path_string(&repo), "--json".to_string()]).expect("doctor");

    assert_eq!(status["status"].as_str(), Some("not_indexed"));
    assert_eq!(status["db_problem_kind"].as_str(), Some("db_missing"));
    assert_eq!(status["path_access_status"].as_str(), Some("db_missing"));
    assert_eq!(status["db_path_outside_workspace"].as_bool(), Some(false));
    assert_eq!(
        status["sidecar_status"].as_str(),
        Some("orphan_without_main_db")
    );
    assert_eq!(
        status["sqlite_sidecars"]["sidecar_status"].as_str(),
        Some("orphan_without_main_db")
    );
    assert_eq!(
        doctor["sidecar_status"].as_str(),
        Some("orphan_without_main_db")
    );
    assert_eq!(
        doctor["sqlite_sidecars"]["sidecar_status"].as_str(),
        Some("orphan_without_main_db")
    );
    assert!(
        !doctor["sqlite_sidecars"]["orphan_sidecars"]
            .as_array()
            .expect("doctor orphan sidecars")
            .is_empty(),
        "{doctor:?}"
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn doctor_repo_mismatch_matches_status_blockers() {
    let repo_a = ui_fixture_repo();
    let repo_b = ui_fixture_repo();
    index_repo(&repo_a).expect("index repo A");
    let repo_a_db = default_db_path(&repo_a);
    let repo_b_db = default_db_path(&repo_b);
    fs::create_dir_all(repo_b_db.parent().expect("repo B db parent")).expect("create db dir");
    fs::copy(&repo_a_db, &repo_b_db).expect("copy mismatched DB");

    let doctor = run_doctor_command(&[path_string(&repo_b), "--json".to_string()]).expect("doctor");
    let status = run_status_command(&[path_string(&repo_b)]).expect("status");

    assert_eq!(doctor["status"].as_str(), Some("error"));
    assert_eq!(doctor["database_exists"].as_bool(), Some(true));
    assert_eq!(doctor["safe_to_query"].as_bool(), Some(false));
    assert_eq!(
        doctor["db_problem_kind"].as_str(),
        Some("repo_root_mismatch")
    );
    assert_eq!(doctor["repo_match"].as_bool(), Some(false));
    assert_eq!(status["status"].as_str(), Some("db_problem"));
    assert!(
        doctor["blockers"]
            .as_array()
            .expect("doctor blockers")
            .iter()
            .any(|blocker| blocker
                .as_str()
                .is_some_and(|value| value.contains("repo root mismatch"))),
        "{doctor:?}"
    );
    assert_eq!(
        doctor["db_lifecycle_read"]["blockers"],
        status["db_lifecycle_read"]["blockers"]
    );

    remove_dir_all_with_retry(&repo_a, "cleanup repo A");
    remove_dir_all_with_retry(&repo_b, "cleanup repo B");
}

#[test]
fn doctor_reports_scope_mismatch_without_opening_mutably() {
    let repo = ui_fixture_repo();
    index_repo(&repo).expect("index fixture");
    let db_path = default_db_path(&repo);
    let connection = rusqlite::Connection::open(&db_path).expect("open DB");
    connection
            .execute(
                "UPDATE codegraph_db_passport SET index_scope_policy_hash = 'tampered-scope-hash' WHERE id = 1",
                [],
            )
            .expect("tamper scope hash");
    drop(connection);

    let doctor = run_doctor_command(&[path_string(&repo), "--json".to_string()]).expect("doctor");

    assert_eq!(doctor["status"].as_str(), Some("error"));
    assert_eq!(doctor["safe_to_query"].as_bool(), Some(false));
    assert_eq!(doctor["scope_match"].as_bool(), Some(false));
    assert_eq!(
        doctor["db_lifecycle_read"]["scope_status"].as_str(),
        Some("mismatched")
    );
    assert!(
        doctor["blockers"]
            .as_array()
            .expect("blockers")
            .iter()
            .any(|blocker| blocker
                .as_str()
                .is_some_and(|value| value.contains("index scope policy hash mismatch"))),
        "{doctor:?}"
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn doctor_reports_missing_passport_for_existing_db() {
    let repo = temp_repo();
    let db_path = default_db_path(&repo);
    fs::create_dir_all(db_path.parent().expect("db parent")).expect("create db dir");
    let connection = rusqlite::Connection::open(&db_path).expect("create DB");
    connection
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .expect("set current user_version");
    drop(connection);

    let doctor = run_doctor_command(&[path_string(&repo), "--json".to_string()]).expect("doctor");

    assert_eq!(doctor["status"].as_str(), Some("error"));
    assert_eq!(doctor["database_exists"].as_bool(), Some(true));
    assert_eq!(doctor["safe_to_query"].as_bool(), Some(false));
    assert_eq!(doctor["passport_status"].as_str(), Some("missing"));
    assert_eq!(doctor["db_problem_kind"].as_str(), Some("passport_missing"));
    assert!(
        doctor["blockers"]
            .as_array()
            .expect("blockers")
            .iter()
            .any(|blocker| blocker
                .as_str()
                .is_some_and(|value| value.contains("codegraph_db_passport table is missing"))),
        "{doctor:?}"
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn doctor_does_not_migrate_old_schema_db() {
    let repo = temp_repo();
    let db_path = default_db_path(&repo);
    fs::create_dir_all(db_path.parent().expect("db parent")).expect("create db dir");
    let connection = rusqlite::Connection::open(&db_path).expect("create DB");
    connection
        .pragma_update(None, "user_version", 1u32)
        .expect("set old user_version");
    drop(connection);

    let doctor = run_doctor_command(&[path_string(&repo), "--json".to_string()]).expect("doctor");
    let observed_version = sqlite_user_version_for_test(&db_path);
    let passport_table_exists = sqlite_table_exists_for_test(&db_path, "codegraph_db_passport");

    assert_eq!(doctor["status"].as_str(), Some("error"));
    assert_eq!(doctor["database_exists"].as_bool(), Some(true));
    assert_eq!(doctor["safe_to_query"].as_bool(), Some(false));
    assert_eq!(doctor["schema_status"].as_str(), Some("mismatched"));
    assert_eq!(doctor["db_problem_kind"].as_str(), Some("schema_mismatch"));
    assert_eq!(observed_version, 1);
    assert!(!passport_table_exists);

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn ui_server_starts_and_status_endpoint_uses_real_index() {
    let repo = ui_fixture_repo();
    index_repo(&repo).expect("index fixture");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind UI listener");
    let addr = listener.local_addr().expect("listener addr");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server_repo = repo.clone();
    let handle = thread::spawn(move || serve_ui_loop(server_repo, listener, Some(shutdown_rx)));

    let response = http_get_json(addr, "/api/status");
    assert_eq!(response["status"].as_str(), Some("ok"));
    assert_eq!(response["files"].as_u64(), Some(1));
    assert!(response["entities"].as_u64().unwrap_or_default() > 0);
    assert_eq!(response["local_only"].as_bool(), Some(true));

    shutdown_tx.send(()).expect("stop UI server");
    handle
        .join()
        .expect("join UI server")
        .expect("UI server ok");
    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn path_graph_api_returns_structural_json_from_real_graph() {
    let repo = ui_fixture_repo();
    index_repo(&repo).expect("index fixture");

    let response = ui_json_body(route_ui_request(
        &repo,
        "GET",
        "/api/path-graph?source=login&target=sanitize",
    ));
    let graph = &response["graph"];

    assert_eq!(response["status"].as_str(), Some("ok"));
    assert!(graph["nodes"].as_array().expect("nodes").len() >= 2);
    assert!(!graph["edges"].as_array().expect("edges").is_empty());
    assert!(graph["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .any(|edge| edge["source_span"].is_object()));
    assert_eq!(graph["layout"]["engine"].as_str(), Some("d3-layered-dag"));
    assert_eq!(
        graph["style"]["exactness"]["static_heuristic"]["line"].as_str(),
        Some("dashed")
    );
    assert_eq!(
        graph["guardrails"]["server_side_filtering"].as_bool(),
        Some(true)
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn relation_filter_api_limits_graph_edges() {
    let repo = ui_fixture_repo();
    index_repo(&repo).expect("index fixture");

    let response = ui_json_body(route_ui_request(
        &repo,
        "GET",
        "/api/path-graph?relations=CALLS",
    ));
    let edges = response["graph"]["edges"].as_array().expect("edges");

    assert!(!edges.is_empty());
    assert!(edges
        .iter()
        .all(|edge| edge["relation"].as_str() == Some("CALLS")));

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn large_path_graph_response_is_truncated_by_node_cap() {
    let repo = ui_fixture_repo();
    index_repo(&repo).expect("index fixture");

    let response = ui_json_body(route_ui_request(&repo, "GET", "/api/path-graph?node_cap=1"));
    let graph = &response["graph"];

    assert_eq!(response["status"].as_str(), Some("ok"));
    assert!(graph["nodes"].as_array().expect("nodes").len() <= 1);
    assert_eq!(graph["guardrails"]["truncated"].as_bool(), Some(true));
    assert!(graph["guardrails"]["truncation_warning"]
        .as_str()
        .expect("warning")
        .contains("truncated"));

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn source_span_and_symbol_search_ui_endpoints_are_evidence_oriented() {
    let repo = ui_fixture_repo();
    index_repo(&repo).expect("index fixture");

    let symbols = ui_json_body(route_ui_request(
        &repo,
        "GET",
        "/api/symbol-search?query=login",
    ));
    assert_eq!(symbols["status"].as_str(), Some("ok"));
    assert!(symbols["hits"].as_array().expect("hits").iter().any(|hit| {
        hit["entity"]["qualified_name"]
            .as_str()
            .is_some_and(|name| name.contains("login"))
    }));

    let span = ui_json_body(route_ui_request(
        &repo,
        "GET",
        "/api/source-span?file=src/auth.ts&start=1&end=2",
    ));
    assert_eq!(span["status"].as_str(), Some("ok"));
    assert!(span["snippet"]
        .as_str()
        .expect("snippet")
        .contains("sanitize"));
    assert!(span["resource"]
        .as_str()
        .expect("resource")
        .starts_with("codegraph://source-span/"));

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn unresolved_calls_ui_endpoint_refuses_unsafe_db() {
    let repo_a = ui_fixture_repo();
    let repo_b = ui_fixture_repo();
    index_repo(&repo_a).expect("index repo A");
    let repo_a_db = repo_a.join(".codegraph").join("codegraph.sqlite");
    let repo_b_db_dir = repo_b.join(".codegraph");
    fs::create_dir_all(&repo_b_db_dir).expect("create repo B DB dir");
    fs::copy(&repo_a_db, repo_b_db_dir.join("codegraph.sqlite")).expect("copy mismatched DB");

    let response = route_ui_request(&repo_b, "GET", "/api/unresolved-calls?limit=1");
    assert_eq!(response.status, 500);
    let body: Value = serde_json::from_str(&response.body).expect("JSON body");
    assert_eq!(body["status"].as_str(), Some("error"));
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| message.contains("repo root mismatch")),
        "{body:?}"
    );

    remove_dir_all_with_retry(&repo_a, "cleanup repo A");
    remove_dir_all_with_retry(&repo_b, "cleanup repo B");
}

#[test]
fn lifecycle_regression_suite_internal_surfaces_share_guardrails() {
    let watch_repo = ui_fixture_repo();
    let external_db = watch_repo.join("suite-long-watch.sqlite");
    index_repo_to_db_with_options(&watch_repo, &external_db, IndexOptions::default())
        .expect("index external watch DB");
    let default_watch_db = default_db_path(&watch_repo);
    assert!(
        !default_watch_db.exists(),
        "external watch setup must not create default DB"
    );
    let watch_startup =
        prepare_watch_startup(&watch_repo, Some(&external_db), "suite.watch.long_running")
            .expect("prepare long-running watch startup");
    assert_eq!(watch_startup.requested_db_path, external_db);
    assert_eq!(watch_startup.actual_db_path_opened, external_db);
    assert_eq!(
        watch_startup.lifecycle_status["lifecycle_status"].as_str(),
        Some("safe_to_write")
    );
    assert_eq!(
        watch_startup.lifecycle_status["auto_index_enabled"].as_bool(),
        Some(false)
    );
    assert!(
        !default_watch_db.exists(),
        "long-running watch must not touch default DB when external DB is configured"
    );

    let ui_repo_a = ui_fixture_repo();
    let ui_repo_b = ui_fixture_repo();
    index_repo(&ui_repo_a).expect("index UI repo A");
    let ui_repo_a_db = default_db_path(&ui_repo_a);
    let ui_repo_b_db = default_db_path(&ui_repo_b);
    fs::create_dir_all(ui_repo_b_db.parent().expect("ui repo B db parent"))
        .expect("create UI repo B db dir");
    fs::copy(&ui_repo_a_db, &ui_repo_b_db).expect("copy mismatched UI DB");
    let ui_response = route_ui_request(&ui_repo_b, "GET", "/api/unresolved-calls?limit=1");
    assert_eq!(ui_response.status, 500);
    let ui_body: Value = serde_json::from_str(&ui_response.body).expect("UI JSON body");
    assert_eq!(ui_body["status"].as_str(), Some("error"));
    assert!(
        ui_body["message"]
            .as_str()
            .is_some_and(|message| message.contains("repo root mismatch")),
        "{ui_body:?}"
    );

    let doctor_repo = temp_repo();
    let doctor_db = default_db_path(&doctor_repo);
    fs::create_dir_all(doctor_db.parent().expect("doctor db parent"))
        .expect("create doctor db dir");
    let doctor_connection = rusqlite::Connection::open(&doctor_db).expect("create old DB");
    doctor_connection
        .pragma_update(None, "user_version", 1u32)
        .expect("set old doctor user_version");
    drop(doctor_connection);
    let doctor =
        run_doctor_command(&[path_string(&doctor_repo), "--json".to_string()]).expect("doctor");
    assert_eq!(doctor["status"].as_str(), Some("error"));
    assert_eq!(doctor["database_exists"].as_bool(), Some(true));
    assert_eq!(doctor["safe_to_query"].as_bool(), Some(false));
    assert_eq!(doctor["schema_status"].as_str(), Some("mismatched"));
    assert_eq!(doctor["db_problem_kind"].as_str(), Some("schema_mismatch"));
    assert!(
        !doctor["blockers"]
            .as_array()
            .expect("doctor blockers")
            .is_empty(),
        "{doctor:?}"
    );
    assert_eq!(sqlite_user_version_for_test(&doctor_db), 1);
    assert!(!sqlite_table_exists_for_test(
        &doctor_db,
        "codegraph_db_passport"
    ));

    let benchmark_repo = temp_repo();
    let benchmark_db = default_db_path(&benchmark_repo);
    fs::create_dir_all(benchmark_db.parent().expect("benchmark db parent"))
        .expect("create benchmark db dir");
    let benchmark_connection =
        rusqlite::Connection::open(&benchmark_db).expect("create benchmark DB");
    benchmark_connection
        .pragma_update(None, "user_version", 1u32)
        .expect("set benchmark user_version");
    drop(benchmark_connection);
    let benchmark_lifecycle = benchmark_inspection_lifecycle_status(
        &benchmark_repo,
        &benchmark_db,
        "suite.benchmark_inspection",
        Some(StorageMode::Proof),
    );
    assert_eq!(benchmark_lifecycle["safe_to_read"].as_bool(), Some(false));
    assert_eq!(benchmark_lifecycle["claimable"].as_bool(), Some(false));
    assert_eq!(benchmark_lifecycle["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(
        benchmark_lifecycle["exact_db_path_checked"].as_str(),
        Some(path_string(&benchmark_db).as_str())
    );
    assert_eq!(sqlite_user_version_for_test(&benchmark_db), 1);
    assert!(!sqlite_table_exists_for_test(
        &benchmark_db,
        "codegraph_db_passport"
    ));

    let sidecar_repo = ui_fixture_repo();
    index_repo(&sidecar_repo).expect("index sidecar repo");
    let sidecar_db = default_db_path(&sidecar_repo);
    let wal_connection = keep_wal_sidecars_for_test(&sidecar_db);
    let sidecar_status = run_status_command(&[path_string(&sidecar_repo)]).expect("status");
    let sidecar_doctor =
        run_doctor_command(&[path_string(&sidecar_repo), "--json".to_string()]).expect("doctor");
    assert_eq!(sidecar_status["status"].as_str(), Some("ok"));
    assert_eq!(sidecar_doctor["status"].as_str(), Some("ok"));
    assert_eq!(sidecar_status["sidecar_status"].as_str(), Some("normal"));
    assert_eq!(sidecar_doctor["sidecar_status"].as_str(), Some("normal"));
    drop(wal_connection);

    let orphan_repo = temp_repo();
    let orphan_db = default_db_path(&orphan_repo);
    fs::create_dir_all(orphan_db.parent().expect("orphan db parent"))
        .expect("create orphan db dir");
    fs::write(sqlite_sidecar_path_for_test(&orphan_db, "wal"), "stale wal")
        .expect("write orphan wal");
    fs::write(sqlite_sidecar_path_for_test(&orphan_db, "shm"), "stale shm")
        .expect("write orphan shm");
    let orphan_status = run_status_command(&[path_string(&orphan_repo)]).expect("status");
    let orphan_doctor =
        run_doctor_command(&[path_string(&orphan_repo), "--json".to_string()]).expect("doctor");
    assert_eq!(
        orphan_status["sidecar_status"].as_str(),
        Some("orphan_without_main_db")
    );
    assert_eq!(
        orphan_doctor["sidecar_status"].as_str(),
        Some("orphan_without_main_db")
    );

    let call_fixture = caller_callee_precision_fixture();
    let exact_callers = query_call_relation(
        &call_fixture.repo,
        CallRelationQueryOptions {
            query: Some("uniqueTarget".to_string()),
            entity_id: None,
            limit: 32,
            exact_resolved: false,
            fuzzy: false,
        },
        CallQueryDirection::Callers,
    )
    .expect("exact callers");
    assert_eq!(exact_callers["status"].as_str(), Some("ok"));
    assert_eq!(
        exact_callers["resolution_mode"].as_str(),
        Some("exact_resolved")
    );
    assert_eq!(
        exact_callers["exact_resolved_entity"]["id"].as_str(),
        Some(call_fixture.unique_target_id.as_str())
    );
    assert_eq!(
        exact_callers["exact_resolved_entity_results"]
            .as_array()
            .expect("exact results")
            .len(),
        1
    );
    assert!(exact_callers["fuzzy_or_global_results"]
        .as_array()
        .expect("fuzzy results")
        .is_empty());

    let ambiguous_callers = query_call_relation(
        &call_fixture.repo,
        CallRelationQueryOptions {
            query: Some("target".to_string()),
            entity_id: None,
            limit: 32,
            exact_resolved: false,
            fuzzy: false,
        },
        CallQueryDirection::Callers,
    )
    .expect("ambiguous callers");
    assert_eq!(
        ambiguous_callers["status"].as_str(),
        Some("ambiguous_symbol")
    );
    assert_eq!(
        ambiguous_callers["resolution_mode"].as_str(),
        Some("ambiguous_symbol")
    );
    let candidates = ambiguous_callers["ambiguous_symbol_matches"]
        .as_array()
        .expect("ambiguous candidates");
    assert!(candidates
        .iter()
        .any(|candidate| candidate["id"].as_str() == Some(call_fixture.alpha_target_id.as_str())));
    assert!(candidates
        .iter()
        .any(|candidate| candidate["id"].as_str() == Some(call_fixture.beta_target_id.as_str())));
    assert!(ambiguous_callers["exact_resolved_entity_results"]
        .as_array()
        .expect("ambiguous exact results")
        .is_empty());
    assert!(ambiguous_callers["suggestion"]["rerun"]
        .as_str()
        .expect("rerun suggestion")
        .contains("--entity-id"));

    remove_dir_all_with_retry(&watch_repo, "cleanup watch repo");
    remove_dir_all_with_retry(&ui_repo_a, "cleanup UI repo A");
    remove_dir_all_with_retry(&ui_repo_b, "cleanup UI repo B");
    remove_dir_all_with_retry(&doctor_repo, "cleanup doctor repo");
    remove_dir_all_with_retry(&benchmark_repo, "cleanup benchmark repo");
    remove_dir_all_with_retry(&sidecar_repo, "cleanup sidecar repo");
    remove_dir_all_with_retry(&orphan_repo, "cleanup orphan repo");
    remove_dir_all_with_retry(&call_fixture.repo, "cleanup caller fixture repo");
}

#[test]
fn context_packet_preview_api_uses_local_context_pack() {
    let repo = ui_fixture_repo();
    index_repo(&repo).expect("index fixture");

    let response = ui_json_body(route_ui_request(
        &repo,
        "GET",
        "/api/context-pack?task=Change+login&seed=login&budget=1200",
    ));

    assert_eq!(response["status"].as_str(), Some("ok"));
    assert!(response["packet"]["verified_paths"].is_array());
    assert_eq!(
        response["proof"].as_str(),
        Some("Context packet preview uses local graph/source evidence.")
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn unknown_command_returns_structured_error() {
    let output = run([BIN_NAME, "crawl-web"]);

    assert_eq!(output.exit_code, 2);
    assert!(output.stderr.contains("\"status\":\"error\""));
    assert!(output.stderr.contains("\"error\":\"unknown_command\""));
}

#[test]
fn query_symbols_agent_json_is_compact_limited_and_does_not_swallow_flags() {
    let fixture = caller_callee_precision_fixture();
    let args = vec![
        "target".to_string(),
        "--limit".to_string(),
        "1".to_string(),
        "--agent-json".to_string(),
        "--json".to_string(),
    ];
    let options = parse_list_query_args("symbols", &args).expect("parse query options");
    assert_eq!(options.query, "target");
    assert_eq!(options.limit, 1);
    assert_eq!(options.fetch_limit(), 2);
    assert_eq!(options.output_mode, QueryOutputMode::AgentJson);

    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
        "passport_status": "valid",
        "schema_status": "valid",
        "scope_status": "match",
    });
    let result =
        query_symbols_with_options(&fixture.repo, &options, Some(&lifecycle)).expect("query");

    assert_eq!(
        result["schema_name"].as_str(),
        Some("query_symbols_agent_json")
    );
    assert_eq!(result["schema_version"].as_u64(), Some(1));
    assert_eq!(result["status"].as_str(), Some("ok"));
    assert_agent_json_contract(
        &result,
        "query_symbols_agent_json",
        "query symbols",
        super::QUERY_AGENT_JSON_SIZE_TARGET_BYTES,
    );
    assert_eq!(result["query"]["text"].as_str(), Some("target"));
    assert_eq!(result["result_count"].as_u64(), Some(1));
    assert_eq!(result["limit"].as_u64(), Some(1));
    assert!(result["db_lifecycle_read"].is_null());
    let results = result["results"].as_array().expect("results");
    assert_eq!(results.len(), 1, "{result:?}");
    assert!(results[0]["file"].as_str().is_some());
    assert!(results[0]["symbol"].as_str().is_some());
    assert!(results[0]["kind"].as_str().is_some());
    assert!(results[0]["score"].as_f64().is_some());
    assert!(results[0]["evidence_role"].as_str().is_some());
    assert!(result["lifecycle"]["claimable"].as_bool().unwrap_or(false));
    assert!(!result["lifecycle"]["diagnostic_only"]
        .as_bool()
        .unwrap_or(true));
    let diagnostic_result =
        query_symbols_with_options(&fixture.repo, &options, None).expect("query diagnostic");
    assert_eq!(
        diagnostic_result["claimable"].as_bool(),
        Some(false),
        "{diagnostic_result}"
    );
    assert_eq!(
        diagnostic_result["diagnostic_only"].as_bool(),
        Some(true),
        "{diagnostic_result}"
    );

    remove_dir_all_with_retry(&fixture.repo, "cleanup");
}

#[test]
fn query_compact_parsers_apply_documented_default_limits() {
    let (globals, rest) = super::parse_global_options(&[
        "--agent-json".to_string(),
        "--limit=3".to_string(),
        "query".to_string(),
        "symbols".to_string(),
        "knownSymbol".to_string(),
    ])
    .expect("global agent flags");
    assert!(globals.agent_json);
    assert_eq!(globals.limit, Some(3));
    assert_eq!(
        rest,
        vec![
            "query".to_string(),
            "symbols".to_string(),
            "knownSymbol".to_string()
        ]
    );

    let normal = parse_list_query_args(
        "symbols",
        &["knownSymbol".to_string(), "--json".to_string()],
    )
    .expect("parse normal json");
    assert_eq!(normal.query, "knownSymbol");
    assert_eq!(normal.limit, 10);

    let agent = parse_list_query_args(
        "symbols",
        &["knownSymbol".to_string(), "--agent-json".to_string()],
    )
    .expect("parse agent json");
    assert_eq!(agent.query, "knownSymbol");
    assert_eq!(agent.limit, 5);
    assert_eq!(agent.fetch_limit(), 6);

    let callers = parse_call_relation_args_with_output(
        "callers",
        &["knownSymbol".to_string(), "--agent-json".to_string()],
    )
    .expect("parse callers");
    assert_eq!(callers.output.limit, 5);
    assert_eq!(callers.options.limit, 6);
}

#[test]
fn query_parsers_reject_misplaced_globals_and_escape_literal_flags() {
    let misplaced_db = parse_list_query_args(
        "symbols",
        &[
            "knownSymbol".to_string(),
            "--db".to_string(),
            "graph.sqlite".to_string(),
        ],
    )
    .expect_err("misplaced --db should be rejected");
    assert!(misplaced_db.contains("--db is a global flag"));
    assert!(misplaced_db.contains("codegraph-mcp --db <path> query ..."));

    let misplaced_repo = super::reject_misplaced_global_flags_in_query_args(&[
        "symbols".to_string(),
        "knownSymbol".to_string(),
        "--repo".to_string(),
        ".".to_string(),
    ])
    .expect_err("misplaced --repo should be rejected before DB preflight");
    assert!(misplaced_repo.contains("--repo is a global flag"));

    let literal = parse_list_query_args(
        "text",
        &[
            "--agent-json".to_string(),
            "--".to_string(),
            "--db".to_string(),
        ],
    )
    .expect("literal flag-like query term");
    assert_eq!(literal.query, "--db");
    assert_eq!(literal.output_mode, QueryOutputMode::AgentJson);

    let relation_literal =
        parse_call_relation_args_with_output("callers", &["--".to_string(), "--db".to_string()])
            .expect("literal flag-like relation symbol");
    assert_eq!(relation_literal.options.query.as_deref(), Some("--db"));
}

#[test]
fn query_text_and_files_agent_json_use_same_compact_contract() {
    let fixture = caller_callee_precision_fixture();
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
    });

    let text_args = vec![
        "seed".to_string(),
        "--limit=5".to_string(),
        "--agent-json".to_string(),
    ];
    let text_options = parse_list_query_args("text", &text_args).expect("parse text");
    assert_eq!(text_options.query, "seed");
    let text =
        query_text_with_options(&fixture.repo, &text_options, Some(&lifecycle)).expect("text");
    assert_eq!(text["schema_name"].as_str(), Some("query_text_agent_json"));
    assert_agent_json_contract(
        &text,
        "query_text_agent_json",
        "query text",
        super::QUERY_AGENT_JSON_SIZE_TARGET_BYTES,
    );
    assert!(text["results"].as_array().expect("text results").len() <= 5);
    assert_eq!(text["truncation"]["limit"].as_u64(), Some(5));

    let file_args = vec![
        "seed.ts".to_string(),
        "--agent-json".to_string(),
        "--limit".to_string(),
        "5".to_string(),
    ];
    let file_options = parse_list_query_args("files", &file_args).expect("parse files");
    assert_eq!(file_options.query, "seed.ts");
    let files =
        query_files_with_options(&fixture.repo, &file_options, Some(&lifecycle)).expect("files");
    assert_eq!(
        files["schema_name"].as_str(),
        Some("query_files_agent_json")
    );
    assert_agent_json_contract(
        &files,
        "query_files_agent_json",
        "query files",
        super::QUERY_AGENT_JSON_SIZE_TARGET_BYTES,
    );
    assert!(files["results"].as_array().expect("file results").len() <= 5);
    assert!(files["results"][0]["file"].as_str().is_some());

    remove_dir_all_with_retry(&fixture.repo, "cleanup");
}

#[test]
fn query_text_and_files_agent_json_label_text_evidence_as_non_graph_proof() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("package").join("foo")).expect("create package");
    fs::write(
        repo.join("package").join("foo").join("foo.mk"),
        "FOO_VERSION = 1.2.3\n$(eval $(generic-package))\n",
    )
    .expect("write mk");
    index_repo(&repo).expect("index text evidence fixture");
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
    });

    let text_options = parse_list_query_args(
        "text",
        &[
            "generic-package".to_string(),
            "--agent-json".to_string(),
            "--limit=3".to_string(),
        ],
    )
    .expect("parse text");
    let text = query_text_with_options(&repo, &text_options, Some(&lifecycle)).expect("query text");
    let text_result = text["results"]
        .as_array()
        .expect("text results")
        .iter()
        .find(|result| result["file"].as_str() == Some("package/foo/foo.mk"))
        .expect("foo.mk text result");
    assert_eq!(text_result["evidence_kind"].as_str(), Some("text_evidence"));
    assert_eq!(text_result["evidence_role"].as_str(), Some("text_evidence"));
    assert_eq!(
        text_result["proof_status"].as_str(),
        Some("not_graph_proof")
    );
    assert_eq!(text_result["graph_proof"].as_bool(), Some(false));
    assert_eq!(text_result["claimable_for_text"].as_bool(), Some(true));
    assert_eq!(text_result["claimable_for_graph"].as_bool(), Some(false));
    assert!(text_result["graph_relation_claims"]
        .as_array()
        .expect("graph claims")
        .is_empty());
    assert!(text_result["claimability"]["not_claimable_as"]
        .as_array()
        .expect("not claimable")
        .iter()
        .any(|value| value.as_str() == Some("CALLS")));

    let file_options = parse_list_query_args(
        "files",
        &[
            "foo.mk".to_string(),
            "--agent-json".to_string(),
            "--limit=3".to_string(),
        ],
    )
    .expect("parse files");
    let files =
        query_files_with_options(&repo, &file_options, Some(&lifecycle)).expect("query files");
    let file_result = files["results"]
        .as_array()
        .expect("file results")
        .iter()
        .find(|result| result["file"].as_str() == Some("package/foo/foo.mk"))
        .expect("foo.mk file result");
    assert_eq!(file_result["evidence_kind"].as_str(), Some("text_evidence"));
    assert_eq!(
        file_result["proof_status"].as_str(),
        Some("not_graph_proof")
    );
    assert_eq!(file_result["claimable_for_text"].as_bool(), Some(true));
    assert_eq!(file_result["claimable_for_graph"].as_bool(), Some(false));
    assert_eq!(file_result["graph_proof"].as_bool(), Some(false));

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn p2_agent_path_json_hydrates_endpoint_names_and_never_uses_unknown_edge_id() {
    let path = PathEvidence {
        id: "path-readable".to_string(),
        summary: Some("handle_request calls save_record".to_string()),
        source: "repo://e/handle_request".to_string(),
        target: "repo://e/save_record".to_string(),
        metapath: vec![RelationKind::Calls],
        edges: vec![(
            "repo://e/handle_request".to_string(),
            RelationKind::Calls,
            "repo://e/save_record".to_string(),
        )],
        source_spans: vec![SourceSpan {
            repo_relative_path: "src/app.ts".to_string(),
            start_line: 3,
            start_column: Some(3),
            end_line: 3,
            end_column: Some(16),
        }],
        exactness: Exactness::ParserVerified,
        length: 1,
        confidence: 0.99,
        metadata: Metadata::from([(
            "edge_labels".to_string(),
            json!([{
                "edge_id": "edge://calls/handle-save",
                "head_id": "repo://e/handle_request",
                "head_entity": {
                    "entity_id": "repo://e/handle_request",
                    "display_name": "handle_request",
                    "name": "handle_request",
                    "qualified_name": "app.handle_request",
                    "kind": "Function",
                    "file": "src/app.ts"
                },
                "relation": "CALLS",
                "tail_id": "repo://e/save_record",
                "tail_entity": {
                    "entity_id": "repo://e/save_record",
                    "display_name": "save_record",
                    "name": "save_record",
                    "qualified_name": "db.save_record",
                    "kind": "Function",
                    "file": "src/db.ts"
                },
                "source_span_detail": {
                    "file": "src/app.ts",
                    "start_line": 3,
                    "end_line": 3
                },
                "evidence_role": "production",
                "classification_reason": "source role metadata",
                "classification_source": "metadata"
            }]),
        )]),
    };

    let value = super::agent_path_evidence_result_json(&path);
    let edge = &value["edges"][0];
    assert_eq!(edge["edge_id"].as_str(), Some("edge://calls/handle-save"));
    assert_eq!(
        edge["source"]["display_name"].as_str(),
        Some("handle_request")
    );
    assert_eq!(edge["target"]["display_name"].as_str(), Some("save_record"));
    assert_eq!(
        edge["source"]["entity_id"].as_str(),
        Some("repo://e/handle_request")
    );
    assert_eq!(
        edge["target"]["qualified_name"].as_str(),
        Some("db.save_record")
    );
    assert_eq!(
        value["source_endpoint"]["display_name"].as_str(),
        Some("handle_request")
    );
    assert_eq!(
        value["target_endpoint"]["display_name"].as_str(),
        Some("save_record")
    );
    let serialized = serde_json::to_string(&value).expect("serialize path");
    assert!(
        !serialized.contains("edge://unknown"),
        "path JSON must not silently invent edge://unknown: {serialized}"
    );

    let reversed_endpoint_path = PathEvidence {
        source: "repo://e/save_record".to_string(),
        target: "repo://e/handle_request".to_string(),
        ..path
    };
    let reversed = super::agent_path_evidence_result_json(&reversed_endpoint_path);
    assert_eq!(
        reversed["source_endpoint"]["display_name"].as_str(),
        Some("save_record")
    );
    assert_eq!(
        reversed["target_endpoint"]["display_name"].as_str(),
        Some("handle_request")
    );
}

#[test]
fn p2_agent_path_json_marks_unhydrated_endpoint_names_unavailable() {
    let path = PathEvidence {
        id: "path-unhydrated".to_string(),
        summary: None,
        source: "repo://e/source".to_string(),
        target: "repo://e/target".to_string(),
        metapath: vec![RelationKind::Calls],
        edges: vec![(
            "repo://e/source".to_string(),
            RelationKind::Calls,
            "repo://e/target".to_string(),
        )],
        source_spans: vec![SourceSpan {
            repo_relative_path: "src/app.ts".to_string(),
            start_line: 1,
            start_column: None,
            end_line: 1,
            end_column: None,
        }],
        exactness: Exactness::ParserVerified,
        length: 1,
        confidence: 0.9,
        metadata: Metadata::new(),
    };

    let value = super::agent_path_evidence_result_json(&path);
    let edge = &value["edges"][0];
    assert!(edge["edge_id"].is_null());
    assert_eq!(edge["edge_id_unavailable"].as_bool(), Some(true));
    assert_eq!(edge["source"]["name_unavailable"].as_bool(), Some(true));
    assert_eq!(edge["target"]["name_unavailable"].as_bool(), Some(true));
    let serialized = serde_json::to_string(&value).expect("serialize path");
    assert!(!serialized.contains("edge://unknown"));
}

#[test]
fn p2_query_agent_json_populates_evidence_roles_for_symbols_files_and_text() {
    let repo = temp_repo();
    write_cli_fixture_file(
        &repo,
        "src/service.ts",
        "export function production_entry() { return 1; }\n",
    );
    write_cli_fixture_file(
        &repo,
        "src/service.test.ts",
        "export function test_entry() { return production_entry(); }\n",
    );
    index_repo(&repo).expect("index role fixture");
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
    });

    let production_options = parse_list_query_args(
        "symbols",
        &[
            "production_entry".to_string(),
            "--agent-json".to_string(),
            "--limit=3".to_string(),
        ],
    )
    .expect("parse production symbol");
    let production = query_symbols_with_options(&repo, &production_options, Some(&lifecycle))
        .expect("query production symbol");
    assert_eq!(
        production["results"][0]["evidence_role"].as_str(),
        Some("production")
    );
    assert!(production["results"][0]["classification_reason"]
        .as_str()
        .is_some());

    let test_options = parse_list_query_args(
        "symbols",
        &[
            "test_entry".to_string(),
            "--agent-json".to_string(),
            "--limit=3".to_string(),
        ],
    )
    .expect("parse test symbol");
    let test =
        query_symbols_with_options(&repo, &test_options, Some(&lifecycle)).expect("query test");
    assert_eq!(test["results"][0]["evidence_role"].as_str(), Some("test"));

    let files_options = parse_list_query_args(
        "files",
        &[
            "service.test.ts".to_string(),
            "--agent-json".to_string(),
            "--limit=3".to_string(),
        ],
    )
    .expect("parse file query");
    let files =
        query_files_with_options(&repo, &files_options, Some(&lifecycle)).expect("query files");
    assert_eq!(files["results"][0]["evidence_role"].as_str(), Some("test"));

    let text_options = parse_list_query_args(
        "text",
        &[
            "production_entry".to_string(),
            "--agent-json".to_string(),
            "--limit=3".to_string(),
        ],
    )
    .expect("parse text query");
    let text = query_text_with_options(&repo, &text_options, Some(&lifecycle)).expect("query text");
    assert_eq!(
        text["results"][0]["evidence_role"].as_str(),
        Some("text_evidence")
    );
    assert_eq!(text["results"][0]["graph_proof"].as_bool(), Some(false));

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn p2_query_evidence_role_helper_labels_stub_generated_and_unknown() {
    let mut generated_metadata = Metadata::new();
    generated_metadata.insert("degradation_labels".to_string(), json!(["generated_large"]));
    let generated = Entity {
        id: "repo://e/generated".to_string(),
        kind: EntityKind::Function,
        name: "generated_client".to_string(),
        qualified_name: "generated_client".to_string(),
        repo_relative_path: "src/generated/client.generated.ts".to_string(),
        source_span: None,
        content_hash: None,
        file_hash: None,
        created_from: "unit-test".to_string(),
        confidence: 1.0,
        metadata: generated_metadata,
    };
    assert_eq!(
        super::query_evidence_role_for_entity(&generated).role,
        "generated"
    );

    let stub = Entity {
        id: "repo://e/stub".to_string(),
        kind: EntityKind::Stub,
        name: "stub_client".to_string(),
        qualified_name: "tests.stub_client".to_string(),
        repo_relative_path: "tests/client.ts".to_string(),
        source_span: None,
        content_hash: None,
        file_hash: None,
        created_from: "unit-test".to_string(),
        confidence: 1.0,
        metadata: Metadata::new(),
    };
    assert_eq!(super::query_evidence_role_for_entity(&stub).role, "stub");

    let unknown = super::query_evidence_role_for_path_and_metadata("", None, None, "unit-test");
    assert_eq!(unknown.role, "unknown");
    assert!(unknown.reason.contains("cannot be determined"));
}

#[test]
fn p2_compact_lifecycle_preserves_diagnostic_override_blockers() {
    let lifecycle = json!({
        "claimable": false,
        "diagnostic_only": true,
        "decision": "diagnostic_stale_reuse",
        "db_problem_kind": "repo_root_mismatch",
        "allow_foreign_db": true,
        "allow_stale_read": false,
        "blockers": ["repo root mismatch: expected A, observed B"],
        "warnings": ["diagnostic_read allowed foreign-repo blocker; output is non-claimable"],
        "safety_labels": ["foreign"],
        "exact_db_path_checked": "external.sqlite",
        "repo_root_expected": "repo-b",
    });
    let compact = super::compact_lifecycle_summary(&lifecycle);
    assert_eq!(compact["claimable"].as_bool(), Some(false));
    assert_eq!(compact["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(compact["allow_foreign_db"].as_bool(), Some(true));
    assert_eq!(
        compact["blockers"][0].as_str(),
        Some("repo root mismatch: expected A, observed B")
    );
    assert_eq!(compact["safety_labels"][0].as_str(), Some("foreign"));
}

#[test]
fn p2_rust_inline_cfg_test_symbol_is_test_evidence() {
    let repo = temp_repo();
    write_cli_fixture_file(
            &repo,
            "Cargo.toml",
            "[package]\nname = \"p2-inline-rust-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        );
    write_cli_fixture_file(
            &repo,
            "src/lib.rs",
            "pub fn production_entry() -> i32 { 1 }\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn inline_works() {\n        assert_eq!(super::production_entry(), 1);\n    }\n}\n",
        );
    index_repo(&repo).expect("index rust inline test fixture");
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
    });
    let options = parse_list_query_args(
        "symbols",
        &[
            "inline_works".to_string(),
            "--agent-json".to_string(),
            "--limit=3".to_string(),
        ],
    )
    .expect("parse rust inline query");
    let result = query_symbols_with_options(&repo, &options, Some(&lifecycle))
        .expect("query rust inline test");
    assert_eq!(result["results"][0]["evidence_role"].as_str(), Some("test"));
    assert!(result["results"][0]["classification_reason"]
        .as_str()
        .is_some_and(|reason| reason.contains("test") || reason.contains("tests")));

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn query_text_evidence_fts_path_title_snippet_buildroot_mini_terms() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("package").join("foo")).expect("create package");
    fs::create_dir_all(repo.join("docs").join("manual")).expect("create docs");
    fs::create_dir_all(repo.join("support").join("scripts")).expect("create support");
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
        repo.join("package").join("foo").join("foo.mk"),
        [
            "FOO_VERSION = 1.2.3",
            "FOO_SITE = https://example.invalid/foo",
            "FOO_LICENSE = MIT",
            "FOO_DEPENDENCIES = host-pkgconf",
            "$(eval $(generic-package))",
            "$(eval $(host-generic-package))",
            "",
        ]
        .join("\n"),
    )
    .expect("write mk");
    fs::write(
        repo.join("package").join("foo").join("Config.in"),
        [
            "config BR2_PACKAGE_FOO",
            "    bool \"foo\"",
            "    depends on BR2_USE_MMU",
            "    select BR2_PACKAGE_ZLIB",
            "",
        ]
        .join("\n"),
    )
    .expect("write Config.in");
    fs::write(
        repo.join("docs")
            .join("manual")
            .join("adding-packages.adoc"),
        [
            "= Adding packages",
            "",
            "The package infrastructure uses generic-package helpers.",
            "Every package should add a Config.in entry.",
            "",
        ]
        .join("\n"),
    )
    .expect("write docs");
    fs::write(
        repo.join("support").join("scripts").join("pkg-stats"),
        [
            "#!/bin/sh",
            "echo BR2_PACKAGE_FOO",
            "echo generic-package",
            "",
        ]
        .join("\n"),
    )
    .expect("write no-extension script");
    fs::write(
            repo.join("expected_text_evidence.json"),
            [
                "{",
                "  \"fixture_name\": \"buildroot_text_evidence_mini\",",
                "  \"purpose\": \"Expected text evidence manifest\",",
                "  \"expected_surfaces\": [\"Makefile\", \"Config.in\", \"manual\", \"support script\"]",
                "}",
                "",
            ]
            .join("\n"),
        )
        .expect("write expected text evidence manifest");
    fs::write(
        repo.join("src").join("download.c"),
        "int download_archive(void) {\n    return 0;\n}\n",
    )
    .expect("write c source");
    index_repo(&repo).expect("index buildroot mini query fixture");
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
    });

    let text_query = |query: &str, limit: usize| {
        let mut args = query
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        args.push("--agent-json".to_string());
        args.push(format!("--limit={limit}"));
        let options = parse_list_query_args("text", &args).expect("parse text query");
        query_text_with_options(&repo, &options, Some(&lifecycle)).expect("query text")
    };
    let file_query = |query: &str, limit: usize| {
        let mut args = query
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        args.push("--agent-json".to_string());
        args.push(format!("--limit={limit}"));
        let options = parse_list_query_args("files", &args).expect("parse files query");
        query_files_with_options(&repo, &options, Some(&lifecycle)).expect("query files")
    };
    let find_file = |value: &Value, file: &str| {
        value["results"]
            .as_array()
            .expect("results")
            .iter()
            .find(|result| result["file"].as_str() == Some(file))
            .unwrap_or_else(|| panic!("missing {file} in {value}"))
            .clone()
    };
    let assert_text_evidence = |result: &Value| {
        assert_eq!(result["evidence_role"].as_str(), Some("text_evidence"));
        assert_eq!(result["proof_status"].as_str(), Some("not_graph_proof"));
        assert_eq!(result["graph_proof"].as_bool(), Some(false));
        assert_eq!(result["claimability"]["claimable"].as_bool(), Some(true));
        assert_eq!(result["claimable_for_text"].as_bool(), Some(true));
        assert_eq!(result["claimable_for_graph"].as_bool(), Some(false));
        assert!(result["graph_relation_claims"]
            .as_array()
            .expect("graph claims")
            .is_empty());
        assert!(result.get("span").is_some(), "{result}");
        let snippet = result["snippet"]["text"].as_str().expect("snippet text");
        assert!(!snippet.is_empty(), "{result}");
        assert!(snippet.len() <= super::QUERY_FILE_SNIPPET_MAX_BYTES);
    };

    let br2 = text_query("BR2_PACKAGE_FOO", 5);
    assert_text_evidence(&find_file(&br2, "package/foo/Config.in"));

    let generic = text_query("generic-package", 10);
    assert_text_evidence(&find_file(&generic, "package/foo/foo.mk"));
    assert_text_evidence(&find_file(&generic, "docs/manual/adding-packages.adoc"));

    let host_generic = text_query("host-generic-package", 5);
    assert_text_evidence(&find_file(&host_generic, "package/foo/foo.mk"));

    let config = file_query("Config.in", 5);
    assert_text_evidence(&find_file(&config, "package/foo/Config.in"));

    let kconfig_kind = file_query("Kconfig", 5);
    assert_text_evidence(&find_file(&kconfig_kind, "package/foo/Config.in"));

    let mk_name = file_query("foo.mk", 5);
    assert_text_evidence(&find_file(&mk_name, "package/foo/foo.mk"));

    let mk_ext = file_query(".mk", 5);
    assert_text_evidence(&find_file(&mk_ext, "package/foo/foo.mk"));

    let docs = text_query("package infrastructure", 5);
    assert_text_evidence(&find_file(&docs, "docs/manual/adding-packages.adoc"));

    let script = file_query("pkg-stats", 5);
    assert_text_evidence(&find_file(&script, "support/scripts/pkg-stats"));

    let manifest_by_path = file_query("expected_text_evidence", 5);
    assert_text_evidence(&find_file(&manifest_by_path, "expected_text_evidence.json"));

    let manifest_by_body = text_query("fixture_name", 5);
    assert_text_evidence(&find_file(&manifest_by_body, "expected_text_evidence.json"));

    let parsed_source = file_query("download.c", 5);
    let parsed_source_result = find_file(&parsed_source, "src/download.c");
    assert_ne!(
        parsed_source_result["evidence_role"].as_str(),
        Some("text_evidence"),
        "graph-parsed source files must not be relabeled as Stage 0 text evidence"
    );
    assert_ne!(
        parsed_source_result["proof_status"].as_str(),
        Some("not_graph_proof"),
        "graph-parsed source files must not inherit text-evidence proof labels"
    );

    let limited = text_query("generic-package", 1);
    assert_eq!(limited["limit"].as_u64(), Some(1));
    assert_eq!(limited["result_count"].as_u64(), Some(1));
    assert!(limited["omitted_count"].as_u64().unwrap_or_default() >= 1);
    assert_eq!(
        limited["truncation"]["omitted_count"].as_u64(),
        limited["omitted_count"].as_u64()
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn query_symbols_verbose_keeps_rich_output_available() {
    let fixture = caller_callee_precision_fixture();
    let args = vec!["target".to_string(), "--verbose".to_string()];
    let options = parse_list_query_args("symbols", &args).expect("parse query options");
    assert_eq!(options.output_mode, QueryOutputMode::RichJson);
    assert_eq!(options.limit, 20);

    let result = query_symbols_with_options(&fixture.repo, &options, None).expect("query");
    assert_eq!(result["status"].as_str(), Some("ok"));
    assert!(result["hits"].is_array());
    assert!(result["ranking"].is_array());
    assert!(result["proof"].as_str().is_some());
    assert!(result["schema_name"].is_null());

    remove_dir_all_with_retry(&fixture.repo, "cleanup");
}

#[test]
fn callers_agent_json_is_limited_and_compact() {
    let fixture = caller_callee_precision_fixture();
    let args = vec![
        "--fuzzy".to_string(),
        "target".to_string(),
        "--limit".to_string(),
        "1".to_string(),
        "--agent-json".to_string(),
    ];
    let parsed = parse_call_relation_args_with_output("callers", &args).expect("parse callers");
    assert_eq!(parsed.output.limit, 1);
    assert_eq!(parsed.options.limit, 2);
    assert_eq!(parsed.output.output_mode, QueryOutputMode::AgentJson);

    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
    });
    let result = query_call_relation_with_output(
        &fixture.repo,
        parsed,
        CallQueryDirection::Callers,
        Some(&lifecycle),
    )
    .expect("query callers");

    assert_eq!(
        result["schema_name"].as_str(),
        Some("callers_callees_agent_json")
    );
    assert_agent_json_contract(
        &result,
        "callers_callees_agent_json",
        "query callers-callees",
        super::QUERY_AGENT_JSON_SIZE_TARGET_BYTES,
    );
    assert_eq!(result["direction"].as_str(), Some("callers"));
    assert_eq!(result["result_count"].as_u64(), Some(1));
    assert_eq!(result["limit"].as_u64(), Some(1));
    assert!(result["results"].as_array().expect("results").len() <= 1);
    assert!(result["results"][0]["edge"]["evidence_role"]
        .as_str()
        .is_some());
    assert!(result["results"][0]["edge"]["source_spans"].is_array());

    remove_dir_all_with_retry(&fixture.repo, "cleanup");
}

#[test]
fn callers_default_to_exact_results_when_symbol_is_unambiguous() {
    let fixture = caller_callee_precision_fixture();

    let result = query_call_relation(
        &fixture.repo,
        CallRelationQueryOptions {
            query: Some("uniqueTarget".to_string()),
            entity_id: None,
            limit: 32,
            exact_resolved: false,
            fuzzy: false,
        },
        CallQueryDirection::Callers,
    )
    .expect("query callers");

    assert_eq!(result["status"].as_str(), Some("ok"));
    assert_eq!(result["resolution_mode"].as_str(), Some("exact_resolved"));
    assert_eq!(
        result["exact_resolved_entity"]["id"].as_str(),
        Some(fixture.unique_target_id.as_str())
    );
    let exact = result["exact_resolved_entity_results"]
        .as_array()
        .expect("exact results");
    assert_eq!(exact.len(), 1, "{result:?}");
    assert_eq!(
        exact[0]["callee"]["id"].as_str(),
        Some(fixture.unique_target_id.as_str())
    );
    assert_eq!(
        exact[0]["proof_labels"]["exactness"].as_str(),
        Some("parser_verified")
    );
    assert!(exact[0]["source_span"].is_object());
    assert!(result["fuzzy_or_global_results"]
        .as_array()
        .expect("fuzzy results")
        .is_empty());
    assert_eq!(
        result["callers"].as_array().expect("legacy callers").len(),
        1
    );

    remove_dir_all_with_retry(&fixture.repo, "cleanup");
}

#[test]
fn callers_ambiguous_symbol_lists_candidates_without_legacy_exact_noise() {
    let fixture = caller_callee_precision_fixture();

    let result = query_call_relation(
        &fixture.repo,
        CallRelationQueryOptions {
            query: Some("target".to_string()),
            entity_id: None,
            limit: 32,
            exact_resolved: false,
            fuzzy: false,
        },
        CallQueryDirection::Callers,
    )
    .expect("query callers");

    assert_eq!(result["status"].as_str(), Some("ambiguous_symbol"));
    assert_eq!(result["resolution_mode"].as_str(), Some("ambiguous_symbol"));
    let candidates = result["ambiguous_symbol_matches"]
        .as_array()
        .expect("ambiguous candidates");
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate["id"].as_str() == Some(fixture.alpha_target_id.as_str())),
        "{candidates:?}"
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate["id"].as_str() == Some(fixture.beta_target_id.as_str())),
        "{candidates:?}"
    );
    assert!(result["exact_resolved_entity_results"]
        .as_array()
        .expect("exact results")
        .is_empty());
    assert!(result["callers"]
        .as_array()
        .expect("legacy callers")
        .is_empty());
    assert!(
        result["fuzzy_or_global_results"]
            .as_array()
            .expect("fuzzy section")
            .len()
            >= 2
    );
    assert!(result["suggestion"]["rerun"]
        .as_str()
        .expect("rerun suggestion")
        .contains("--entity-id"));

    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
    });
    let parsed = parse_call_relation_args_with_output(
        "callers",
        &[
            "target".to_string(),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ],
    )
    .expect("parse compact ambiguous callers");
    let compact = query_call_relation_with_output(
        &fixture.repo,
        parsed,
        CallQueryDirection::Callers,
        Some(&lifecycle),
    )
    .expect("compact ambiguous callers");
    assert_eq!(compact["status"].as_str(), Some("warning"));
    assert_eq!(compact["graph_proof"].as_bool(), Some(false));
    assert_eq!(
        compact["proof_status"].as_str(),
        Some("relation_resolution_ambiguous")
    );
    assert_eq!(
        compact["proof_strength"].as_str(),
        Some("symbol_candidate_evidence")
    );
    let compact_candidates = compact["query"]["candidate_entities"]
        .as_array()
        .expect("compact ambiguous candidates");
    assert!(compact_candidates
        .iter()
        .any(|candidate| { candidate["id"].as_str() == Some(fixture.alpha_target_id.as_str()) }));
    assert!(compact_candidates
        .iter()
        .any(|candidate| { candidate["id"].as_str() == Some(fixture.beta_target_id.as_str()) }));
    assert!(compact["query"]["rerun_with_entity_id_template"]
        .as_str()
        .expect("rerun template")
        .contains("--entity-id <candidate-id>"));
    assert_eq!(
        compact["relation_resolution"]["source_navigation_fallback"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(compact["result_count"].as_u64(), Some(0));

    remove_dir_all_with_retry(&fixture.repo, "cleanup");
}

#[test]
fn callers_entity_id_mode_returns_only_selected_entity_callers() {
    let fixture = caller_callee_precision_fixture();
    let options = CallRelationQueryOptions {
        query: None,
        entity_id: Some(fixture.alpha_target_id.clone()),
        limit: 32,
        exact_resolved: false,
        fuzzy: false,
    };

    let result = query_call_relation(&fixture.repo, options, CallQueryDirection::Callers)
        .expect("exact callers");

    assert_eq!(result["status"].as_str(), Some("ok"));
    assert_eq!(result["resolution_mode"].as_str(), Some("entity_id"));
    let callers = result["callers"].as_array().expect("callers");
    assert_eq!(callers.len(), 1, "{result:?}");
    assert_eq!(
        callers[0]["caller"]["id"].as_str(),
        Some(fixture.alpha_caller_id.as_str())
    );
    assert_eq!(
        callers[0]["callee"]["id"].as_str(),
        Some(fixture.alpha_target_id.as_str())
    );
    assert_ne!(
        callers[0]["caller"]["id"].as_str(),
        Some(fixture.beta_caller_id.as_str())
    );

    remove_dir_all_with_retry(&fixture.repo, "cleanup");
}

#[test]
fn callees_entity_id_mode_returns_exact_callees_and_fuzzy_mode_still_works() {
    let fixture = caller_callee_precision_fixture();
    let exact_options = CallRelationQueryOptions {
        query: None,
        entity_id: Some(fixture.alpha_caller_id.clone()),
        limit: 32,
        exact_resolved: false,
        fuzzy: false,
    };

    let exact = query_call_relation(&fixture.repo, exact_options, CallQueryDirection::Callees)
        .expect("exact callees");

    assert_eq!(exact["status"].as_str(), Some("ok"));
    assert_eq!(exact["resolution_mode"].as_str(), Some("entity_id"));
    let callees = exact["callees"].as_array().expect("callees");
    assert_eq!(callees.len(), 1, "{exact:?}");
    assert_eq!(
        callees[0]["callee"]["id"].as_str(),
        Some(fixture.alpha_target_id.as_str())
    );

    let fuzzy_args = vec!["--fuzzy".to_string(), "target".to_string()];
    let fuzzy_options =
        parse_call_relation_args("callers", &fuzzy_args).expect("parse fuzzy options");
    let fuzzy = query_call_relation(&fixture.repo, fuzzy_options, CallQueryDirection::Callers)
        .expect("fuzzy callers");

    assert_eq!(fuzzy["status"].as_str(), Some("ok"));
    assert_eq!(fuzzy["resolution_mode"].as_str(), Some("fuzzy_or_global"));
    assert!(fuzzy["exact_resolved_entity_results"]
        .as_array()
        .expect("exact results")
        .is_empty());
    assert!(
        fuzzy["callers"]
            .as_array()
            .expect("legacy fuzzy callers")
            .len()
            >= 2
    );

    remove_dir_all_with_retry(&fixture.repo, "cleanup");
}

#[test]
fn relation_query_no_proof_path_found_is_for_relationless_exact_symbol() {
    let fixture = caller_callee_precision_fixture();
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
    });
    let parsed = parse_call_relation_args_with_output(
        "callers",
        &[
            "callUnique".to_string(),
            "--limit".to_string(),
            "5".to_string(),
            "--agent-json".to_string(),
        ],
    )
    .expect("parse relationless callers");

    let result = query_call_relation_with_output(
        &fixture.repo,
        parsed,
        CallQueryDirection::Callers,
        Some(&lifecycle),
    )
    .expect("query relationless callers");

    assert_eq!(result["status"].as_str(), Some("ok"));
    assert_eq!(result["graph_proof"].as_bool(), Some(false));
    assert_eq!(result["proof_status"].as_str(), Some("no_proof_path_found"));
    assert_eq!(
        result["proof_strength"].as_str(),
        Some("source_navigation_evidence")
    );
    assert_eq!(result["result_count"].as_u64(), Some(0));
    assert_eq!(
        result["relation_resolution"]["source_navigation_fallback"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(
        result["relation_resolution"]["source_navigation_fallback"]["candidate_entities"][0]["id"]
            .as_str(),
        Some(fixture.unique_caller_id.as_str())
    );

    remove_dir_all_with_retry(&fixture.repo, "cleanup");
}

#[test]
fn context_pack_old_path_without_source_role_metadata_is_unknown_not_production() {
    let span = SourceSpan::with_columns("src/lib.rs", 8, 5, 8, 17);
    let mut metadata = Metadata::new();
    metadata.insert("path_context".to_string(), json!("production"));
    let path = PathEvidence {
        id: "legacy-path".to_string(),
        summary: None,
        source: "src::lib.legacy".to_string(),
        target: "src::lib.prod_value".to_string(),
        metapath: vec![RelationKind::Calls],
        edges: vec![(
            "src::lib.legacy".to_string(),
            RelationKind::Calls,
            "src::lib.prod_value".to_string(),
        )],
        source_spans: vec![span],
        exactness: Exactness::ParserVerified,
        length: 1,
        confidence: 1.0,
        metadata,
    };
    let budgets = super::ContextPackBudgets::for_mode("impact");

    let production =
        super::filter_and_sort_context_path_evidence(vec![path.clone()], "impact", budgets);
    assert!(
        production.is_empty(),
        "missing source-role metadata must not be accepted as production proof"
    );

    let debug = super::filter_and_sort_context_path_evidence(vec![path], "debug", budgets);
    let debug_path = debug.first().expect("debug keeps unknown evidence");
    assert_eq!(
        debug_path
            .metadata
            .get("evidence_role")
            .and_then(Value::as_str),
        Some("unknown")
    );
    assert_eq!(
        debug_path
            .metadata
            .get("classification_source")
            .and_then(Value::as_str),
        Some("fallback")
    );
}

#[test]
fn context_pack_hydrates_inline_test_role_from_endpoint_qualified_name() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
        repo.join("src").join("lib.rs"),
        "pub fn prod_value() -> i32 { 1 }\n",
    )
    .expect("write seed");
    index_repo(&repo).expect("index repo");
    let db_path = default_db_path(&repo);
    let store = SqliteGraphStore::open(&db_path).expect("open store");

    let inline_test = test_function_entity(
        "src/lib.rs",
        "calls_prod_value",
        "src::lib.tests.calls_prod_value",
        12,
    );
    let production = test_function_entity("src/lib.rs", "prod_value", "src::lib.prod_value", 1);
    store
        .upsert_entity(&inline_test)
        .expect("upsert test entity");
    store
        .upsert_entity(&production)
        .expect("upsert production entity");
    let edge = test_call_edge(&inline_test, &production, "src/lib.rs", 13);
    store.upsert_edge(&edge).expect("upsert edge");

    let mut metadata = Metadata::new();
    metadata.insert("path_context".to_string(), json!("production"));
    metadata.insert(
        "production_test_mock_labels".to_string(),
        json!(["production"]),
    );
    metadata.insert(
        "edge_labels".to_string(),
        json!([{
            "edge_id": edge.id.clone(),
            "relation": "CALLS",
            "source_span": edge.source_span.to_string(),
            "exactness": "parser_verified",
            "confidence": 1.0,
            "derived": false,
            "edge_class": "base_exact",
            "fact_class": "base_exact",
            "context": "production",
            "provenance_edges": []
        }]),
    );
    let path = PathEvidence {
        id: "inline-test-path".to_string(),
        summary: None,
        source: inline_test.id.clone(),
        target: production.id.clone(),
        metapath: vec![RelationKind::Calls],
        edges: vec![(
            inline_test.id.clone(),
            RelationKind::Calls,
            production.id.clone(),
        )],
        source_spans: vec![edge.source_span.clone()],
        exactness: Exactness::ParserVerified,
        length: 1,
        confidence: 1.0,
        metadata,
    };
    store.upsert_path_evidence(&path).expect("upsert path");
    drop(store);

    let connection = rusqlite::Connection::open(&db_path).expect("open connection");
    let budgets = super::ContextPackBudgets::for_mode("impact");
    let production_paths = super::load_stored_context_path_evidence(
        &connection,
        &[production.id.clone()],
        "impact",
        budgets,
    )
    .expect("load production paths")
    .paths;
    assert!(
        production_paths.is_empty(),
        "endpoint qname tests module should exclude inline test evidence"
    );

    let test_paths = super::load_stored_context_path_evidence(
        &connection,
        &[production.id.clone()],
        "test-impact",
        budgets,
    )
    .expect("load test-impact paths")
    .paths;
    let hydrated = test_paths
        .iter()
        .find(|path| path.id == "inline-test-path")
        .expect("test-impact includes hydrated inline test path");
    assert_eq!(
        hydrated
            .metadata
            .get("evidence_role")
            .and_then(Value::as_str),
        Some("mixed")
    );
    let first_label = hydrated
        .metadata
        .get("edge_labels")
        .and_then(Value::as_array)
        .and_then(|labels| labels.first())
        .expect("edge label");
    assert_eq!(first_label["head_source_role"].as_str(), Some("test"));
    assert!(first_label["classification_reason"]
        .as_str()
        .is_some_and(|reason| reason.contains("tests module")));

    drop(connection);
    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn context_pack_path_evidence_lookup_hydration_is_bounded_and_preserves_provenance() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
        repo.join("src").join("lib.rs"),
        "pub fn prod_seed() {}\npub fn target_0() {}\npub fn target_1() {}\npub fn target_2() {}\n",
    )
    .expect("write source");
    index_repo(&repo).expect("index repo");
    let db_path = default_db_path(&repo);
    let store = SqliteGraphStore::open(&db_path).expect("open store");
    let seed = test_function_entity("src/lib.rs", "prod_seed", "src::lib.prod_seed", 1);
    store.upsert_entity(&seed).expect("upsert seed");

    let mut first_edge_id = String::new();
    for index in 0..5 {
        let target = test_function_entity(
            "src/lib.rs",
            &format!("target_{index}"),
            &format!("src::lib.target_{index}"),
            index as u32 + 2,
        );
        store.upsert_entity(&target).expect("upsert target");
        let edge = test_call_edge(&seed, &target, "src/lib.rs", index as u32 + 2);
        if index == 0 {
            first_edge_id = edge.id.clone();
        }
        store.upsert_edge(&edge).expect("upsert edge");
        let mut metadata = Metadata::new();
        metadata.insert(
                "edge_labels".to_string(),
                json!([{
                    "edge_id": edge.id.clone(),
                    "relation": "CALLS",
                    "source_span": edge.source_span.to_string(),
                    "exactness": "parser_verified",
                    "confidence": 1.0,
                    "derived": index == 0,
                    "edge_class": if index == 0 { "derived_with_provenance" } else { "base_exact" },
                    "fact_class": if index == 0 { "derived_with_provenance" } else { "base_exact" },
                    "context": "production",
                    "provenance_edges": if index == 0 { vec![first_edge_id.clone()] } else { Vec::<String>::new() },
                }]),
            );
        store
            .upsert_path_evidence(&PathEvidence {
                id: format!("bounded-path-{index}"),
                summary: None,
                source: seed.id.clone(),
                target: target.id.clone(),
                metapath: vec![RelationKind::Calls],
                edges: vec![(seed.id.clone(), RelationKind::Calls, target.id.clone())],
                source_spans: vec![edge.source_span],
                exactness: Exactness::ParserVerified,
                length: 1,
                confidence: 1.0,
                metadata,
            })
            .expect("upsert path evidence");
    }
    drop(store);

    let connection = rusqlite::Connection::open(&db_path).expect("open connection");
    let normal = super::load_stored_context_path_evidence(
        &connection,
        &[seed.id.clone()],
        "debug",
        super::ContextPackBudgets::for_mode("debug"),
    )
    .expect("load normal path evidence");
    assert!(normal.paths.len() >= 5, "{:?}", normal.paths);
    assert_eq!(normal.telemetry["hydration_query_count"].as_u64(), Some(1));
    assert_eq!(normal.telemetry["n_plus_one_query_count"].as_u64(), Some(0));
    assert_eq!(
        normal.telemetry["lookup_strategy"].as_str(),
        Some("path_evidence_symbols_join")
    );
    let preserved = normal
        .paths
        .iter()
        .find(|path| path.id == "bounded-path-0")
        .expect("preserved path");
    assert_eq!(preserved.source_spans.len(), 1);
    let first_label = preserved
        .metadata
        .get("edge_labels")
        .and_then(Value::as_array)
        .and_then(|labels| labels.first())
        .expect("hydrated edge label");
    assert_eq!(first_label["derived"].as_bool(), Some(true));
    assert!(first_label["provenance_edges"]
        .as_array()
        .is_some_and(|edges| edges
            .iter()
            .any(|edge| edge.as_str() == Some(&first_edge_id))));

    let mut bounded = super::ContextPackBudgets::for_mode("debug");
    bounded.max_candidate_paths = 2;
    bounded.max_path_evidence_rows = 2;
    bounded.max_returned_proof_paths = 2;
    bounded.max_hydration_bytes = 32;
    let bounded_load =
        super::load_stored_context_path_evidence(&connection, &[seed.id.clone()], "debug", bounded)
            .expect("load bounded path evidence");
    assert!(bounded_load.paths.len() <= 2);
    assert_eq!(
        bounded_load.telemetry["max_path_evidence_rows"].as_u64(),
        Some(2)
    );
    assert_eq!(
        bounded_load.telemetry["max_hydration_bytes"].as_u64(),
        Some(32)
    );
    assert_eq!(
        bounded_load.telemetry["path_evidence_truncated"].as_bool(),
        Some(true)
    );
    assert!(
        bounded_load.telemetry["path_evidence_omitted_count"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert_eq!(
        bounded_load.telemetry["hydration_budget_exhausted"].as_bool(),
        Some(true)
    );

    drop(connection);
    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn context_pack_production_snippets_exclude_inline_test_spans() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
        repo.join("src").join("lib.rs"),
        r#"pub fn prod_value() -> i32 {
    1
}

pub fn prod_neighbor() -> i32 {
    prod_value() + 1
}

#[test]
fn standalone_test() {
    prod_value();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prod_value_test_shadow() -> i32 {
        prod_value()
    }

    #[test]
    fn calls_prod_value() {
        prod_value_test_shadow();
        prod_value();
    }
}
"#,
    )
    .expect("write inline test fixture");

    let production_span = SourceSpan::with_columns("src/lib.rs", 6, 5, 6, 17);
    let test_span = SourceSpan::with_columns("src/lib.rs", 19, 9, 19, 21);
    let production_path = PathEvidence {
        id: "production-path".to_string(),
        summary: None,
        source: "src::lib.prod_neighbor".to_string(),
        target: "src::lib.prod_value".to_string(),
        metapath: vec![RelationKind::Calls],
        edges: vec![(
            "src::lib.prod_neighbor".to_string(),
            RelationKind::Calls,
            "src::lib.prod_value".to_string(),
        )],
        source_spans: vec![production_span],
        exactness: Exactness::ParserVerified,
        length: 1,
        confidence: 1.0,
        metadata: {
            let mut metadata = Metadata::new();
            metadata.insert(
                "edge_labels".to_string(),
                json!([{
                    "evidence_role": "production",
                    "classification_source": "stored_metadata",
                    "classification_reason": "source role metadata"
                }]),
            );
            metadata
        },
    };
    let test_path = PathEvidence {
        id: "inline-test-path".to_string(),
        summary: None,
        source: "src::lib.tests.prod_value_test_shadow".to_string(),
        target: "src::lib.prod_value".to_string(),
        metapath: vec![RelationKind::Calls],
        edges: vec![(
            "src::lib.tests.prod_value_test_shadow".to_string(),
            RelationKind::Calls,
            "src::lib.prod_value".to_string(),
        )],
        source_spans: vec![test_span],
        exactness: Exactness::ParserVerified,
        length: 1,
        confidence: 1.0,
        metadata: {
            let mut metadata = Metadata::new();
            metadata.insert(
                "edge_labels".to_string(),
                json!([{
                    "evidence_role": "test",
                    "classification_source": "endpoint:qualified_name",
                    "classification_reason": "qualified name contains tests module"
                }]),
            );
            metadata
        },
    };
    let budgets = super::ContextPackBudgets::for_mode("impact");
    let production_paths = super::filter_and_sort_context_path_evidence(
        vec![production_path.clone(), test_path.clone()],
        "impact",
        budgets,
    );
    assert!(
        production_paths.iter().all(|path| {
            path.metadata
                .get("evidence_role")
                .and_then(Value::as_str)
                .is_some_and(|role| role == "production")
        }),
        "{production_paths:?}"
    );
    let requested_spans = super::context_source_spans_for_paths(&production_paths);
    assert_eq!(requested_spans.len(), 1);
    assert_eq!(requested_spans[0].start_line, 6);
    let (_, production_snippets, _, _) =
        super::load_context_sources_and_snippets(&repo, &requested_spans, 24)
            .expect("load production snippets");
    assert!(!production_snippets.is_empty());
    for snippet in &production_snippets {
        let text = snippet.text.as_str();
        assert!(!text.contains("#[test]"), "{text}");
        assert!(!text.contains("standalone_test"), "{text}");
        assert!(!text.contains("mod tests"), "{text}");
        assert!(!text.contains("prod_value_test_shadow"), "{text}");
        assert!(!text.contains("calls_prod_value"), "{text}");
    }

    let test_paths = super::filter_and_sort_context_path_evidence(
        vec![production_path, test_path],
        "test-impact",
        budgets,
    );
    assert!(
        test_paths.iter().any(|path| {
            matches!(
                path.metadata.get("evidence_role").and_then(Value::as_str),
                Some("test" | "mock" | "mixed")
            )
        }),
        "{test_paths:?}"
    );

    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn context_pack_compact_parser_supports_agent_limits_and_verbose_audit() {
    let agent = super::parse_context_pack_args(&[
        "--task".to_string(),
        "Change prod_value".to_string(),
        "--mode".to_string(),
        "production".to_string(),
        "--seed".to_string(),
        "prod_value".to_string(),
        "--agent-json".to_string(),
        "--limit-paths".to_string(),
        "3".to_string(),
        "--limit-snippets".to_string(),
        "2".to_string(),
        "--max-output-bytes".to_string(),
        "4096".to_string(),
    ])
    .expect("parse agent context-pack");

    assert_eq!(agent.mode, "production");
    assert_eq!(agent.output_mode, QueryOutputMode::AgentJson);
    assert_eq!(agent.limit_paths, Some(3));
    assert_eq!(agent.limit_snippets, Some(2));
    assert_eq!(agent.max_output_bytes, Some(4096));
    assert!(!agent.explain);
    let budgets = super::ContextPackBudgets::for_options(&agent);
    assert_eq!(budgets.max_returned_proof_paths, 3);
    assert_eq!(budgets.max_snippets, 2);

    let verbose = super::parse_context_pack_args(&[
        "--task".to_string(),
        "Change prod_value".to_string(),
        "--verbose".to_string(),
    ])
    .expect("parse verbose context-pack");
    assert_eq!(verbose.output_mode, QueryOutputMode::RichJson);
    assert!(verbose.profile);

    let explain = super::parse_context_pack_args(&[
        "--task".to_string(),
        "Plan generic-package".to_string(),
        "--explain".to_string(),
    ])
    .expect("parse explain context-pack");
    assert_eq!(explain.output_mode, QueryOutputMode::AgentJson);
    assert!(explain.profile);
    assert!(explain.explain);

    let audit = super::parse_context_pack_args(&[
        "--task".to_string(),
        "Plan generic-package".to_string(),
        "--audit-json".to_string(),
    ])
    .expect("parse audit-json context-pack");
    assert_eq!(audit.output_mode, QueryOutputMode::AgentJson);
    assert!(audit.profile);
    assert!(audit.explain);
}

#[test]
fn context_pack_agent_json_is_compact_and_proof_labeled() {
    let options = context_agent_test_options("production", Some(2), Some(2), None);
    let packet = context_agent_test_packet("production", 1, 1, "production");
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
        "passport_status": "valid",
    });
    let result = super::context_pack_agent_json_response(
        &options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );

    assert_eq!(
        result["schema_name"].as_str(),
        Some("context_pack_agent_json")
    );
    assert_eq!(result["schema_version"].as_u64(), Some(1));
    assert_eq!(result["status"].as_str(), Some("ok"));
    assert_agent_json_contract(
        &result,
        "context_pack_agent_json",
        "context-pack",
        super::DEFAULT_CONTEXT_AGENT_MAX_OUTPUT_BYTES,
    );
    assert_eq!(result["mode"].as_str(), Some("production"));
    assert_eq!(result["claimable"].as_bool(), Some(true));
    assert_eq!(result["diagnostic_only"].as_bool(), Some(false));
    assert_eq!(result["lifecycle"]["claimable"].as_bool(), Some(true));
    assert!(result.get("retrieval_explain").is_none());
    assert_eq!(
        result["db_lifecycle_read"]["decision"].as_str(),
        Some("read_reuse")
    );
    assert!(result.get("profile").is_none());

    let paths = result["paths"].as_array().expect("paths");
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0]["evidence_role"].as_str(), Some("production"));
    assert_eq!(
        paths[0]["classification_reason"].as_str(),
        Some("stored source-role metadata")
    );
    assert_eq!(paths[0]["production_proof_eligible"].as_bool(), Some(true));
    assert!(paths[0].get("summary").is_none());
    assert!(paths[0]["edges"][0]["source_spans"]
        .as_array()
        .is_some_and(|spans| !spans.is_empty()));

    let snippets = result["snippets"].as_array().expect("snippets");
    assert_eq!(snippets.len(), 1);
    assert_eq!(snippets[0]["evidence_role"].as_str(), Some("production"));
    assert_eq!(result["proof_status"].as_str(), Some("proof_path_found"));
    assert_eq!(result["graph_proof"].as_bool(), Some(true));
    assert_eq!(
        result["graph_verification"]["status"].as_str(),
        Some("graph_verified")
    );
    assert_eq!(
        result["graph_verification"]["proof_status"].as_str(),
        Some("proof_path_found")
    );
    assert_eq!(
        result["graph_verification"]["graph_proof"].as_bool(),
        Some(true)
    );
    assert!(result["proof_failure_reason"].is_null());
    assert_eq!(
        result["planning_packet"]["evidence_type"].as_str(),
        Some("graph_proof")
    );
    assert_eq!(
        result["planning_packet"]["proof_status"].as_str(),
        Some("proof_path_found")
    );
    assert_eq!(
        result["planning_packet"]["graph_proof"].as_bool(),
        Some(true)
    );
    let candidates = result["candidates"].as_array().expect("candidates");
    if candidates.is_empty() {
        assert!(
            result["omitted_count"].as_u64().unwrap_or_default() > 0
                || result["candidate_omitted_count"]
                    .as_u64()
                    .unwrap_or_default()
                    > 0,
            "compacted candidate details should report omissions: {result}"
        );
        assert!(result["expansion_handles"]
            .as_array()
            .is_some_and(|handles| !handles.is_empty()));
    } else {
        assert!(
            candidates.iter().any(|candidate| {
                candidate["candidate_sources"]
                    .as_array()
                    .is_some_and(|sources| {
                        sources
                            .iter()
                            .any(|source| source.as_str() == Some("path_evidence"))
                    })
                    && candidate["verification_status"].as_str() == Some("graph_verified")
                    && candidate["proof_status"].as_str() == Some("proof_path_found")
                    && candidate["claimable_for_graph"].as_bool() == Some(true)
                    && candidate["span"]["file"].as_str().is_some()
            }),
            "{candidates:?}"
        );
    }
    assert_eq!(
        result["candidate_count"].as_u64(),
        Some(candidates.len() as u64)
    );
    assert_eq!(result["truncation"]["limit"].as_u64(), Some(2));
    assert!(serialized_len_for_test(&result) <= super::DEFAULT_CONTEXT_AGENT_MAX_OUTPUT_BYTES);
}

#[test]
fn context_pack_explain_mode_works_for_graph_proof_fixture() {
    let mut options = context_agent_test_options("production", Some(2), Some(2), Some(512 * 1024));
    options.explain = true;
    let packet = context_agent_test_packet("production", 1, 1, "production");
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
        "passport_status": "valid",
    });
    let result = super::context_pack_agent_json_response(
        &options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    let explain = &result["retrieval_explain"];
    assert_eq!(explain["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(explain["proof_status"].as_str(), Some("proof_path_found"));
    assert_eq!(explain["graph_proof"].as_bool(), Some(true));
    assert_eq!(explain["proof_paths_found"].as_u64(), Some(1));
    assert_eq!(explain["traversal_mode"].as_str(), Some("unknown"));
    assert!(explain["relation_allowlist"].as_array().is_some());
    assert!(explain["source_role_filter"].is_object());
    assert!(explain["candidate_sources_verified"]
        .as_array()
        .expect("candidate sources verified")
        .iter()
        .any(|source| source.as_str() == Some("path_evidence")));
    assert!(explain["path_evidence_lookup_time_ms"].is_null());
    assert!(explain["path_evidence_hydration_time_ms"].is_null());
    assert_eq!(explain["omitted_count"].as_u64(), Some(6));
    assert_eq!(
        explain["graph_verification_attempts"]["status"].as_str(),
        Some("graph_verified")
    );
    assert!(explain["candidate_sources"]
        .as_array()
        .expect("candidate sources")
        .iter()
        .any(|source| source.as_str() == Some("path_evidence")));
    assert!(explain["ranking_reasons"]
        .as_array()
        .expect("ranking reasons")
        .iter()
        .any(|reason| reason["proof_status"].as_str() == Some("proof_path_found")));
}

#[test]
fn context_pack_explain_includes_bounded_traversal_diagnostics_without_default_bloat() {
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
        "passport_status": "valid",
    });
    let mut packet = context_agent_test_packet("production", 1, 1, "production");
    packet.metadata.insert(
        "traversal_telemetry".to_string(),
        json!({
            "schema_version": 1,
            "diagnostic_only": true,
            "measurement_scope": "context_pack_graph_verification",
            "traversal_mode": "production",
            "relation_allowlist": ["CALLS", "READS"],
            "max_depth": 3,
            "max_paths": 8,
            "max_edge_visits": 32,
            "edges_visited": 7,
            "nodes_visited": 5,
            "paths_found": 1,
            "paths_returned": 1,
            "budget_stop_reason": {"proof_path_found": 1},
            "cycles_cut": 1,
            "structural_edges_skipped": 2,
            "heuristic_edges_skipped": 3,
            "source_role_filter": {
                "applied": true,
                "mode": "production",
                "blocked_edges": 4
            },
            "no_proof_fallback_reason": null
        }),
    );
    packet.metadata.insert(
        "path_evidence_telemetry".to_string(),
        json!({
            "schema_version": 1,
            "diagnostic_only": true,
            "measurement_scope": "context_pack_stored_path_evidence",
            "lookup_time_ms": 1.25,
            "hydration_time_ms": 2.5,
            "path_evidence_omitted_count": 0,
            "source_snippet_omitted_count": 0
        }),
    );

    let compact_options = context_agent_test_options("production", Some(2), Some(2), None);
    let compact = super::context_pack_agent_json_response(
        &compact_options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&compact_options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    assert!(compact.get("retrieval_explain").is_none());
    let compact_serialized = serde_json::to_string(&compact).expect("serialize compact");
    assert!(!compact_serialized.contains("traversal_telemetry"));
    assert!(!compact_serialized.contains("relation_allowlist"));
    assert!(!compact_serialized.contains("path_evidence_lookup_time_ms"));

    let mut explain_options =
        context_agent_test_options("production", Some(2), Some(2), Some(512 * 1024));
    explain_options.explain = true;
    let explain_response = super::context_pack_agent_json_response(
        &explain_options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&explain_options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    let explain = &explain_response["retrieval_explain"];
    assert_eq!(explain["traversal_mode"].as_str(), Some("production"));
    assert_eq!(explain["relation_allowlist"][0].as_str(), Some("CALLS"));
    assert_eq!(
        explain["traversal_telemetry"]["max_depth"].as_u64(),
        Some(3)
    );
    assert_eq!(
        explain["traversal_telemetry"]["max_paths"].as_u64(),
        Some(8)
    );
    assert_eq!(
        explain["traversal_telemetry"]["max_edge_visits"].as_u64(),
        Some(32)
    );
    assert_eq!(
        explain["traversal_telemetry"]["edges_visited"].as_u64(),
        Some(7)
    );
    assert_eq!(
        explain["traversal_telemetry"]["nodes_visited"].as_u64(),
        Some(5)
    );
    assert_eq!(
        explain["traversal_telemetry"]["paths_found"].as_u64(),
        Some(1)
    );
    assert_eq!(
        explain["traversal_telemetry"]["paths_returned"].as_u64(),
        Some(1)
    );
    assert_eq!(
        explain["traversal_telemetry"]["cycles_cut"].as_u64(),
        Some(1)
    );
    assert_eq!(
        explain["traversal_telemetry"]["structural_edges_skipped"].as_u64(),
        Some(2)
    );
    assert_eq!(
        explain["traversal_telemetry"]["heuristic_edges_skipped"].as_u64(),
        Some(3)
    );
    assert_eq!(
        explain["source_role_filter"]["blocked_edges"].as_u64(),
        Some(4)
    );
    assert!(explain["candidate_sources_verified"]
        .as_array()
        .expect("candidate sources verified")
        .iter()
        .any(|source| source.as_str() == Some("path_evidence")));
    assert_eq!(explain["path_evidence_lookup_time_ms"].as_f64(), Some(1.25));
    assert_eq!(
        explain["path_evidence_hydration_time_ms"].as_f64(),
        Some(2.5)
    );
    assert_eq!(explain["omitted_count"].as_u64(), Some(6));
    assert!(explain["no_proof_fallback_reason"].is_null());
}

#[test]
fn context_pack_explain_includes_vector_trace_only_when_requested() {
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
        "passport_status": "valid",
    });
    let mut packet = context_agent_test_packet("production", 1, 1, "production");
    packet.metadata.insert(
            "vector_candidate_trace".to_string(),
            json!({
                "schema_version": 1,
                "diagnostic_only": true,
                "vector_enabled": true,
                "vector_index_status": "ready",
                "provider": {
                    "provider_id": "codegraph-local-deterministic",
                    "model_id": "codegraph-deterministic-token-projection-v1",
                    "dimension": 64,
                    "embedding_profile": "deterministic-test-embedding-v1"
                },
                "query_text_sent_to_vector_branch": "Find package metadata",
                "chunk_count_searched": 2,
                "vector_candidate_count": 1,
                "top_vector_candidate_ids": ["vector://text/package/foo/foo.mk"],
                "score_range": {"min": 0.91, "max": 0.91},
                "candidates_accepted": {
                    "count": 1,
                    "candidate_ids": ["vector://text/package/foo/foo.mk"]
                },
                "candidates_rejected": {
                    "count": 1,
                    "candidate_ids": ["vector://text/package/noisy"]
                },
                "stale_missing_vector_index_reason": null,
                "graph_verification_status_for_vector_candidates": {
                    "status_counts": {"not_graph_proof": 1},
                    "requires_graph_verification_count": 0,
                    "graph_verified_count": 0,
                    "graph_verified_candidate_ids": [],
                    "graph_proof_count": 0
                },
                "no_proof_fallback_reason": "vector text-evidence candidate available after graph verification found no proof path"
            }),
        );

    let compact_options = context_agent_test_options("production", Some(2), Some(2), None);
    let compact = super::context_pack_agent_json_response(
        &compact_options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&compact_options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    assert!(compact.get("retrieval_explain").is_none());
    assert!(!serde_json::to_string(&compact)
        .expect("serialize compact response")
        .contains("vector://text/package/foo/foo.mk"));

    let mut explain_options =
        context_agent_test_options("production", Some(2), Some(2), Some(512 * 1024));
    explain_options.explain = true;
    let explain_response = super::context_pack_agent_json_response(
        &explain_options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&explain_options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    let vector_trace = &explain_response["retrieval_explain"]["vector_trace"];
    assert_eq!(vector_trace["vector_enabled"].as_bool(), Some(true));
    assert_eq!(vector_trace["vector_index_status"].as_str(), Some("ready"));
    assert_eq!(
        vector_trace["provider"]["model_id"].as_str(),
        Some("codegraph-deterministic-token-projection-v1")
    );
    assert_eq!(vector_trace["chunk_count_searched"].as_u64(), Some(2));
    assert_eq!(vector_trace["vector_candidate_count"].as_u64(), Some(1));
    assert_eq!(
        vector_trace["top_vector_candidate_ids"][0].as_str(),
        Some("vector://text/package/foo/foo.mk")
    );
    assert_eq!(
        vector_trace["graph_verification_status_for_vector_candidates"]["graph_proof_count"]
            .as_u64(),
        Some(0)
    );
}

#[test]
fn context_pack_explain_includes_nuance_rescue_trace_without_default_bloat_or_secrets() {
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
        "passport_status": "valid",
    });
    let mut options = context_agent_test_options("production", Some(2), Some(2), Some(512 * 1024));
    options.task =
        "Trace QUASAR_NETTLE_FUSE /api/admin/:id BR2_PACKAGE_FOO failing_admin_test auth"
            .to_string();
    options.explain = true;
    let secret_like_token = "SECRET_TOKEN_DO_NOT_TRACE";
    let tokens = vec![
        super::ContextPackNuanceToken {
            value: "QUASAR_NETTLE_FUSE".to_string(),
            normalized: "quasar_nettle_fuse".to_string(),
            raw_lower: "quasar_nettle_fuse".to_string(),
            kind: "identifier_signature",
            rescue_reason: "identifier_signature_rescue",
            exact_match: true,
            seed_value: Some("QUASAR_NETTLE_FUSE".to_string()),
        },
        super::ContextPackNuanceToken {
            value: "auth".to_string(),
            normalized: "auth".to_string(),
            raw_lower: "auth".to_string(),
            kind: "rare_token",
            rescue_reason: "rare_token_lexical_rescue",
            exact_match: false,
            seed_value: None,
        },
        super::ContextPackNuanceToken {
            value: "/api/admin/:id".to_string(),
            normalized: "apiadminid".to_string(),
            raw_lower: "/api/admin/:id".to_string(),
            kind: "route_literal",
            rescue_reason: "route_literal_rescue",
            exact_match: true,
            seed_value: None,
        },
        super::ContextPackNuanceToken {
            value: "BR2_PACKAGE_FOO".to_string(),
            normalized: "br2_package_foo".to_string(),
            raw_lower: "br2_package_foo".to_string(),
            kind: "config_key",
            rescue_reason: "config_key_rescue",
            exact_match: true,
            seed_value: None,
        },
        super::ContextPackNuanceToken {
            value: "failing_admin_test".to_string(),
            normalized: "failing_admin_test".to_string(),
            raw_lower: "failing_admin_test".to_string(),
            kind: "test_name",
            rescue_reason: "test_name_rescue",
            exact_match: true,
            seed_value: None,
        },
        super::ContextPackNuanceToken {
            value: secret_like_token.to_string(),
            normalized: "secret_token_do_not_trace".to_string(),
            raw_lower: "secret_token_do_not_trace".to_string(),
            kind: "identifier_signature",
            rescue_reason: "identifier_signature_rescue",
            exact_match: true,
            seed_value: None,
        },
    ];
    let candidate = json!({
        "candidate_id": "nuance-rescue://source/src/rare_identifier.ts:1",
        "candidate_source": "nuance_rescue",
        "candidate_sources": ["nuance_rescue", "text_evidence", "lexical_fts"],
        "path": "src/rare_identifier.ts",
        "entity_id": null,
        "evidence_role": "text_evidence",
        "rescue_reason": "identifier_signature_rescue",
        "matched_token": secret_like_token,
        "matched_tokens": [secret_like_token],
        "proof_status": "not_graph_proof",
        "graph_proof": false,
        "verification_status": "not_graph_proof",
        "graph_verification_status": "not_graph_proof",
        "text_evidence_status": "not_graph_proof",
        "claimable_for_graph": false,
        "full_source_body": "FULL_SOURCE_BODY_DO_NOT_TRACE",
    });
    let candidates = vec![candidate.clone()];
    let trace = super::context_pack_nuance_rescue_trace(
        true,
        &tokens,
        &["and".to_string(), "the".to_string()],
        &candidates,
        "ready",
    );
    let mut packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols: Vec::new(),
        verified_paths: Vec::new(),
        risks: Vec::new(),
        recommended_tests: Vec::new(),
        snippets: Vec::new(),
        metadata: Metadata::new(),
    };
    packet
        .metadata
        .insert("nuance_rescue_trace".to_string(), trace);
    packet
        .metadata
        .insert("nuance_rescue_candidates".to_string(), json!(candidates));
    packet.metadata.insert(
        "fallback_evidence".to_string(),
        json!([{
            "id": "nuance-rescue://source/src/rare_identifier.ts:1",
            "file": "src/rare_identifier.ts",
            "evidence_role": "text_evidence",
            "proof_status": "not_graph_proof",
            "graph_proof": false
        }]),
    );
    packet
        .metadata
        .insert("proof_path_available".to_string(), json!(false));
    packet
        .metadata
        .insert("proof_status".to_string(), json!("no_proof_path_found"));
    packet
        .metadata
        .insert("graph_proof".to_string(), json!(false));
    packet.metadata.insert(
        "evidence_status".to_string(),
        json!("fallback_evidence_found"),
    );
    packet.metadata.insert(
            "proof_failure_reason".to_string(),
            json!("no graph-verifiable candidates were available; returning fallback source/text evidence"),
        );

    let compact_options = context_agent_test_options("production", Some(2), Some(2), None);
    let compact = super::context_pack_agent_json_response(
        &compact_options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&compact_options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    let compact_serialized = serde_json::to_string(&compact).expect("serialize compact");
    assert!(compact.get("retrieval_explain").is_none());
    assert!(!compact_serialized.contains("rescue_reasons"));
    assert!(!compact_serialized.contains(secret_like_token));
    assert!(!compact_serialized.contains("FULL_SOURCE_BODY_DO_NOT_TRACE"));

    let explain_response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    let explain = &explain_response["retrieval_explain"];
    assert_eq!(explain["nuance_rescue_enabled"].as_bool(), Some(true));
    assert_eq!(explain["rescue_candidate_count"].as_u64(), Some(1));
    assert!(explain["rare_tokens"]
        .as_array()
        .expect("rare tokens")
        .iter()
        .any(|token| token.as_str() == Some("auth")));
    assert!(explain["identifier_tokens"]
        .as_array()
        .expect("identifier tokens")
        .iter()
        .any(|token| token.as_str() == Some("QUASAR_NETTLE_FUSE")));
    assert!(explain["route_config_test_tokens"]
        .as_array()
        .expect("route/config/test tokens")
        .iter()
        .any(|token| token.as_str() == Some("/api/admin/:id")));
    assert!(explain["rescue_candidate_sources"]
        .as_array()
        .expect("rescue candidate sources")
        .iter()
        .any(|source| source.as_str() == Some("nuance_rescue")));
    assert_eq!(explain["candidates_accepted"]["count"].as_u64(), Some(1));
    assert!(explain["candidates_rejected"]["count"].as_u64().is_some());
    assert_eq!(
        explain["graph_verification_status"].as_str(),
        Some("no_proof_path_found")
    );
    assert_eq!(
        explain["no_proof_fallback_status"].as_str(),
        Some("active_no_graph_proof_text_fallback")
    );
    let explain_serialized = serde_json::to_string(explain).expect("serialize explain");
    assert!(explain_serialized.contains("[redacted_secret_like_token]"));
    assert!(!explain_serialized.contains(secret_like_token));
    assert!(!explain_serialized.contains("FULL_SOURCE_BODY_DO_NOT_TRACE"));
}

#[test]
fn context_pack_candidate_union_dedup_ranking_protects_exact_seed() {
    let mut options = context_agent_test_options("production", Some(3), Some(8), Some(512 * 1024));
    options.task = "Plan generic-package changes".to_string();
    options.seeds = vec!["package/foo/foo.mk".to_string()];
    let packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols: Vec::new(),
        verified_paths: Vec::new(),
        risks: Vec::new(),
        recommended_tests: Vec::new(),
        snippets: Vec::new(),
        metadata: Metadata::new(),
    };
    let snippets = vec![
        json!({
            "file": "package/foo/foo.mk",
            "lines": "5",
            "text": "$(eval $(generic-package))",
            "reason": "text evidence fallback"
        }),
        json!({
            "file": "package/bar/bar.mk",
            "lines": "5",
            "text": "$(eval $(generic-package))",
            "reason": "text evidence fallback"
        }),
        json!({
            "file": "package/baz/baz.mk",
            "lines": "5",
            "text": "$(eval $(generic-package))",
            "reason": "text evidence fallback"
        }),
        json!({
            "file": "package/qux/qux.mk",
            "lines": "5",
            "text": "$(eval $(generic-package))",
            "reason": "text evidence fallback"
        }),
    ];
    let fallback_evidence = snippets
        .iter()
        .enumerate()
        .map(|(index, snippet)| {
            let file = snippet["file"].as_str().expect("snippet file");
            json!({
                "id": format!("text-evidence://candidate-{index}"),
                "symbol": file,
                "kind": "makefile",
                "file": file,
                "source_span": {
                    "file": file,
                    "start_line": 5,
                    "start_column": 1,
                    "end_line": 5,
                    "end_column": 32
                },
                "evidence_role": "text_evidence",
                "proof_status": "no_proof_path_found",
                "graph_proof": false,
                "matched_seeds": ["generic-package"],
                "score": index as f64,
                "classification_reason": "matched bounded text evidence via stage0_fts",
                "classification_source": "stage0_fts/text_evidence"
            })
        })
        .collect::<Vec<_>>();

    let candidate_set = super::build_context_agent_retrieval_candidates(
        &options,
        &packet,
        &[],
        &fallback_evidence,
        &snippets,
        true,
        false,
    );

    assert!(
        candidate_set.total_count > super::CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT,
        "{candidate_set:?}"
    );
    assert!(candidate_set.omitted_count > 0, "{candidate_set:?}");
    assert!(
        candidate_set.candidates.len() <= super::CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT
            || candidate_set.exact_seed_cap_override,
        "{candidate_set:?}"
    );
    let exact_text = candidate_set
        .candidates
        .iter()
        .find(|candidate| candidate["path"].as_str() == Some("package/foo/foo.mk"))
        .expect("exact path seed survives cap and merges with text evidence");
    let sources = exact_text["candidate_sources"]
        .as_array()
        .expect("candidate sources")
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    assert!(sources.contains("exact_seed"), "{sources:?}");
    assert!(sources.contains("file_path_seed"), "{sources:?}");
    assert!(sources.contains("text_evidence"), "{sources:?}");
    assert!(sources.contains("lexical_fts"), "{sources:?}");
    assert_eq!(exact_text["rank"].as_u64(), Some(1));
    assert_eq!(exact_text["graph_proof"].as_bool(), Some(false));
    assert_eq!(exact_text["claimable_for_text"].as_bool(), Some(true));
    assert_eq!(exact_text["claimable_for_graph"].as_bool(), Some(false));
    assert_eq!(
        exact_text["verification_status"].as_str(),
        Some("no_proof_path_found")
    );
    assert!(exact_text["matched_seeds"]
        .as_array()
        .expect("matched seeds")
        .iter()
        .any(|seed| seed.as_str() == Some("package/foo/foo.mk")));
    assert!(exact_text["matched_seeds"]
        .as_array()
        .expect("matched seeds")
        .iter()
        .any(|seed| seed.as_str() == Some("generic-package")));
    assert!(candidate_set.candidates.iter().all(|candidate| {
        candidate["graph_proof"].as_bool() == Some(false)
            && candidate.get("proof_edges").is_none()
            && candidate.get("relations").is_none()
    }));
}

#[test]
fn context_pack_text_evidence_fallback_returns_buildroot_planning_snippets() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("package").join("foo")).expect("create package");
    fs::create_dir_all(repo.join("docs").join("manual")).expect("create docs");
    fs::create_dir_all(repo.join("support").join("scripts")).expect("create support");
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
        repo.join("package").join("foo").join("foo.mk"),
        [
            "FOO_VERSION = 1.2.3",
            "FOO_SITE = https://example.invalid/foo",
            "FOO_LICENSE = MIT",
            "FOO_DEPENDENCIES = host-pkgconf",
            "$(eval $(generic-package))",
            "",
        ]
        .join("\n"),
    )
    .expect("write mk");
    fs::write(
        repo.join("package").join("foo").join("Config.in"),
        [
            "config BR2_PACKAGE_FOO",
            "    bool \"foo\"",
            "    depends on BR2_USE_MMU",
            "    select BR2_PACKAGE_ZLIB",
            "",
        ]
        .join("\n"),
    )
    .expect("write Config.in");
    fs::write(
        repo.join("docs")
            .join("manual")
            .join("adding-packages.adoc"),
        [
            "= Adding packages",
            "",
            "The package infrastructure uses generic-package helpers.",
            "Every package should add a Config.in entry.",
            "",
        ]
        .join("\n"),
    )
    .expect("write docs");
    fs::write(
        repo.join("support").join("scripts").join("pkg-stats"),
        [
            "#!/bin/sh",
            "echo BR2_PACKAGE_FOO",
            "echo generic-package",
            "",
        ]
        .join("\n"),
    )
    .expect("write support script");
    fs::write(
        repo.join("src").join("download.c"),
        "int download_archive(void) {\n    return 0;\n}\n",
    )
    .expect("write c source");
    index_repo(&repo).expect("index buildroot mini context-pack fixture");
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
    });
    let generic_text_options = parse_list_query_args(
        "text",
        &[
            "generic-package".to_string(),
            "--agent-json".to_string(),
            "--limit=8".to_string(),
        ],
    )
    .expect("parse generic-package text query");
    let generic_text = query_text_with_options(&repo, &generic_text_options, Some(&lifecycle))
        .expect("query generic-package text evidence");
    let generic_text_span = generic_text["results"]
        .as_array()
        .expect("generic-package text results")
        .iter()
        .find(|result| result["file"].as_str() == Some("package/foo/foo.mk"))
        .map(|result| result["span"].clone())
        .expect("generic-package foo.mk text span");

    let connection = Connection::open(default_db_path(&repo)).expect("open fixture db");
    let mut options = context_agent_test_options("production", Some(3), Some(8), Some(512 * 1024));
    options.task = "Plan how to add a new Buildroot package".to_string();
    options.token_budget = 20_000;
    options.explain = true;
    options.seeds.clear();
    let budgets = super::ContextPackBudgets::for_options(&options);
    let raw_seed_values = super::context_pack_seed_values(&options, budgets.max_seed_entities);
    let fallback_evidence = super::build_context_pack_text_evidence_fallback(
        &connection,
        &options,
        &raw_seed_values,
        budgets,
    )
    .expect("build text evidence fallback");
    let fallback_files = fallback_evidence
        .iter()
        .map(|evidence| evidence.source_span.repo_relative_path.as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        fallback_files.contains("package/foo/foo.mk"),
        "{fallback_files:?}"
    );
    assert!(
        fallback_files.contains("package/foo/Config.in"),
        "{fallback_files:?}"
    );
    assert!(
        fallback_files.contains("docs/manual/adding-packages.adoc"),
        "{fallback_files:?}"
    );
    assert!(fallback_evidence.iter().all(|evidence| {
        evidence.evidence_role_label.as_deref() == Some("text_evidence")
            && evidence.proof_status.as_deref() == Some("no_proof_path_found")
            && !evidence.graph_proof
    }));

    let fallback_snippets =
        super::load_context_fallback_snippets(&repo, &fallback_evidence, budgets.max_snippets)
            .expect("load fallback snippets");
    let snippet_files = fallback_snippets
        .iter()
        .map(|snippet| snippet.file.as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        snippet_files.contains("package/foo/foo.mk"),
        "{snippet_files:?}"
    );
    assert!(
        snippet_files.contains("package/foo/Config.in"),
        "{snippet_files:?}"
    );
    assert!(
        snippet_files.contains("docs/manual/adding-packages.adoc"),
        "{snippet_files:?}"
    );
    let fallback_evidence_count = fallback_evidence.len();
    let packet = super::build_context_packet_from_stored_evidence(
        &options,
        &raw_seed_values,
        &[],
        &[],
        Vec::new(),
        fallback_snippets,
        fallback_evidence,
        None,
        budgets,
        0,
        fallback_evidence_count,
    );
    let response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &lifecycle,
        budgets,
        &repo,
        &default_db_path(&repo),
        json!({"wall_ms": 1.0}),
    );

    assert_eq!(response["proof_path_available"].as_bool(), Some(false));
    assert_eq!(
        response["proof_status"].as_str(),
        Some("no_proof_path_found")
    );
    assert_eq!(response["graph_proof"].as_bool(), Some(false));
    assert_eq!(
        response["graph_verification"]["status"].as_str(),
        Some("no_graph_candidates")
    );
    assert_eq!(
        response["graph_verification"]["proof_status"].as_str(),
        Some("no_proof_path_found")
    );
    assert_eq!(
        response["graph_verification"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert_eq!(
        response["graph_verification"]["text_only_candidates_require_graph_verification"].as_bool(),
        Some(false)
    );
    assert!(response["proof_failure_reason"]
        .as_str()
        .is_some_and(|reason| reason.contains("fallback source/text evidence")));
    let explain = &response["retrieval_explain"];
    assert_eq!(explain["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(
        explain["prompt_intent"].as_str(),
        Some("build_system_planning")
    );
    assert!(explain["seeds_extracted"]
        .as_array()
        .expect("seeds extracted")
        .iter()
        .any(|seed| seed["seed"].as_str() == Some("Buildroot")));
    assert!(explain["ignored_seeds"]
        .as_array()
        .expect("ignored seeds")
        .iter()
        .any(|seed| {
            seed["ignored_reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("generic task verb"))
        }));
    assert_eq!(explain["proof_paths_found"].as_u64(), Some(0));
    assert_eq!(explain["graph_proof"].as_bool(), Some(false));
    assert!(explain["no_proof_fallback_reason"]
        .as_str()
        .is_some_and(|reason| reason.contains("fallback source/text evidence")));
    assert!(
        explain["candidate_counts_by_source"]["text_evidence"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(
        explain["candidate_counts_by_source"]["lexical_fts"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(explain["ranking_reasons"]
        .as_array()
        .expect("ranking reasons")
        .iter()
        .all(|reason| reason["reason"].as_str().is_some()));
    assert_eq!(
        response["claimability"]["claimable_as"][0].as_str(),
        Some("source_text_existence")
    );
    assert!(response["paths"].as_array().expect("paths").is_empty());
    assert!(response["fallback_evidence"]
        .as_array()
        .expect("fallback evidence")
        .iter()
        .all(|evidence| {
            evidence["evidence_role"].as_str() == Some("text_evidence")
                && evidence["proof_status"].as_str() == Some("no_proof_path_found")
                && evidence["graph_proof"].as_bool() == Some(false)
        }));
    let candidates = response["candidates"].as_array().expect("candidates");
    assert_eq!(
        response["candidate_count"].as_u64(),
        Some(candidates.len() as u64)
    );
    assert!(
        !candidates.is_empty(),
        "context-pack fallback must expose text evidence as retrieval candidates"
    );
    assert_eq!(
        response["candidate_cap"].as_u64(),
        Some(super::CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT as u64)
    );
    assert!(candidates.iter().all(|candidate| {
        candidate["candidate_sources"]
            .as_array()
            .is_some_and(|sources| !sources.is_empty())
            && candidate["rank"].as_u64().is_some()
            && candidate["reason"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty())
            && candidate["verification_status"].as_str().is_some()
            && candidate["graph_proof"].as_bool() == Some(false)
            && candidate["claimable_for_graph"].as_bool() == Some(false)
            && candidate["matched_seeds"]
                .as_array()
                .is_some_and(|seeds| !seeds.is_empty())
            && candidate.get("proof_edges").is_none()
            && candidate.get("relations").is_none()
    }));
    let generic_candidate = candidates
        .iter()
        .find(|candidate| candidate["path"].as_str() == Some("package/foo/foo.mk"))
        .unwrap_or_else(|| panic!("generic-package foo.mk candidate in {candidates:?}"));
    let generic_sources = generic_candidate["candidate_sources"]
        .as_array()
        .expect("generic candidate sources");
    assert!(generic_sources
        .iter()
        .any(|source| source.as_str() == Some("text_evidence")));
    assert!(generic_sources
        .iter()
        .any(|source| source.as_str() == Some("lexical_fts")));
    assert_eq!(
        generic_candidate["evidence_role"].as_str(),
        Some("text_evidence")
    );
    assert!(matches!(
        generic_candidate["proof_status"].as_str(),
        Some("no_proof_path_found" | "not_graph_proof")
    ));
    assert_eq!(
        generic_candidate["claimable_for_text"].as_bool(),
        Some(true)
    );
    assert_eq!(generic_candidate["span"], generic_text_span);
    assert!(generic_candidate["snippet"]
        .as_str()
        .is_some_and(|snippet| snippet.contains("generic-package")));
    let response_snippets = response["snippets"].as_array().expect("snippets");
    assert!(
        response_snippets.iter().any(|snippet| {
            snippet["file"].as_str() == Some("package/foo/foo.mk")
                && snippet["text"]
                    .as_str()
                    .is_some_and(|text| text.contains("generic-package"))
        }),
        "{response_snippets:?}"
    );
    assert!(response_snippets.iter().all(|snippet| {
        snippet["graph_relation_claims"]
            .as_array()
            .is_some_and(Vec::is_empty)
    }));
    assert!(response["fallback_evidence"]
        .as_array()
        .expect("fallback evidence")
        .iter()
        .all(|evidence| evidence.get("edges").is_none() && evidence.get("relations").is_none()));
    let response_fallback_snippets = response["fallback_snippets"]
        .as_array()
        .expect("fallback snippets");
    assert!(!response_fallback_snippets.is_empty(), "{response:?}");
    assert!(response_fallback_snippets
        .iter()
        .all(|snippet| snippet["graph_proof"].as_bool() == Some(false)));
    let selected_roles = response["selected_role_coverage"]
        .as_array()
        .expect("selected role coverage")
        .iter()
        .filter_map(|role| role["role"].as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        selected_roles.contains("docs_authoring_guidance")
            && selected_roles.contains("package_metadata")
            && selected_roles.contains("kconfig_config_wiring"),
        "{selected_roles:?}"
    );
    assert_eq!(
        response["explain_budget_status"]["separate_from_evidence_budget"].as_bool(),
        Some(true)
    );
    assert!(
        serde_json::to_vec(&response)
            .expect("serialize response")
            .len()
            <= 512 * 1024
    );
    let planning = &response["planning_packet"];
    assert_eq!(planning["task"].as_str(), Some(options.task.as_str()));
    assert_eq!(planning["mode"].as_str(), Some("production"));
    assert_eq!(planning["claimable"].as_bool(), Some(true));
    assert_eq!(
        planning["proof_status"].as_str(),
        Some("no_proof_path_found")
    );
    assert_eq!(planning["graph_proof"].as_bool(), Some(false));
    assert_eq!(planning["evidence_type"].as_str(), Some("text_evidence"));
    assert_eq!(
        planning["claimability"]["claimable_as"][0].as_str(),
        Some("source_text_existence")
    );
    assert_eq!(
        planning["claimability"]["not_claimable_as"][0].as_str(),
        Some("graph_relation_proof")
    );
    let planning_files = planning["likely_files"]
        .as_array()
        .expect("planning likely files")
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    assert!(
        planning_files.contains("package/foo/foo.mk"),
        "{planning_files:?}"
    );
    assert!(
        planning_files.contains("package/foo/Config.in"),
        "{planning_files:?}"
    );
    assert!(
        planning_files.contains("docs/manual/adding-packages.adoc"),
        "{planning_files:?}"
    );
    let planning_roles = planning["selected_role_coverage"]
        .as_array()
        .expect("planning role coverage")
        .iter()
        .filter_map(|role| role["role"].as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        planning_roles.contains("docs_authoring_guidance")
            && planning_roles.contains("package_metadata")
            && planning_roles.contains("kconfig_config_wiring"),
        "{planning_roles:?}"
    );
    assert!(planning["evidence_items"]
        .as_array()
        .expect("planning evidence items")
        .iter()
        .any(
            |item| item["evidence_type"].as_str() == Some("text_evidence")
                && item["graph_proof"].as_bool() == Some(false)
        ));
    let planning_queries = planning["follow_up_queries"]
        .as_array()
        .expect("planning follow-up queries");
    assert!(
        planning_query_exists(planning_queries, "generic-package", "package/"),
        "{planning_queries:?}"
    );
    assert!(
        planning_query_exists(planning_queries, "BR2_PACKAGE_FOO", "package/*/Config.in"),
        "{planning_queries:?}"
    );
    assert!(
        planning_query_exists(planning_queries, "license version dependencies", "package/"),
        "{planning_queries:?}"
    );
    assert!(planning_queries.len() <= super::CONTEXT_PLANNING_PACKET_QUERY_LIMIT);
    assert!(planning_queries.iter().all(|query| {
        query["reason"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
            && query["source_evidence_id"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
            && query["expected_signal"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
            && matches!(query["risk"].as_str(), Some("broad" | "precise" | "noisy"))
            && query["max_results_hint"].as_u64().is_some()
    }));
    let verification_hints = planning["suggested_verification_commands"]
        .as_array()
        .expect("planning verification hints");
    assert!(!verification_hints.is_empty());
    assert!(verification_hints
        .iter()
        .all(|hint| hint["shell_ready"].as_bool() == Some(false)));
    assert!(
        !serde_json::to_string(verification_hints)
            .expect("serialize hints")
            .contains("\"rg\""),
        "{verification_hints:?}"
    );
    drop(connection);
    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn context_pack_plan_atoms_surface_source_navigation_without_exact_seed() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("crates").join("codegraph-cli").join("src"))
        .expect("create cli src");
    fs::write(
            repo.join("Cargo.toml"),
            "[package]\nname = \"plan_atom_source_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"crates/codegraph-cli/src/lib.rs\"\n",
        )
        .expect("write manifest");
    fs::write(
        repo.join("crates")
            .join("codegraph-cli")
            .join("src")
            .join("lib.rs"),
        r#"pub fn index_entrypoint() {}

pub fn db_lifecycle_preflight_guard() {}

pub fn open_store_for_passport() {}

#[cfg(test)]
mod tests {
    #[test]
    fn lifecycle_preflight_tests() {}
}
"#,
    )
    .expect("write source");
    index_repo(&repo).expect("index plan atom source fixture");

    let connection =
        super::open_context_pack_connection(&default_db_path(&repo)).expect("open context DB");
    let mut options = context_agent_test_options("production", Some(8), Some(8), Some(65536));
    options.task = "Trace indexing entry point and DB lifecycle guards.".to_string();
    options.seeds.clear();
    let budgets = super::ContextPackBudgets::for_options(&options);
    let evidence = super::build_context_pack_plan_atom_source_navigation_fallback(
        &connection,
        &options,
        &BTreeSet::new(),
        budgets,
    )
    .expect("plan atom source navigation fallback");

    assert!(
        evidence.iter().any(|item| {
            item.source_span
                .repo_relative_path
                .ends_with("crates/codegraph-cli/src/lib.rs")
                && item
                    .fallback_source
                    .contains("retrieval_plan_atom/source_navigation")
                && item.evidence_role_label.as_deref() == Some("source_navigation")
                && !item.graph_proof
        }),
        "{evidence:?}"
    );

    drop(connection);
    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn context_pack_planning_packet_caps_broad_follow_up_queries() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("package").join("foo")).expect("create package");
    fs::create_dir_all(repo.join("docs").join("manual")).expect("create docs");
    fs::create_dir_all(repo.join("support").join("scripts")).expect("create support");
    fs::create_dir_all(repo.join("support").join("download")).expect("create download support");
    fs::write(
        repo.join("package").join("pkg-generic.mk"),
        [
            "# Generic package infrastructure",
            "define inner-generic-package",
            "    $($(2)_INSTALL_TARGET_CMDS)",
            "endef",
            "",
        ]
        .join("\n"),
    )
    .expect("write pkg-generic");
    fs::write(
        repo.join("package").join("pkg-download.mk"),
        [
            "# Download infrastructure",
            "$(DL_WRAPPER) $(FOO_SITE)",
            "support/download/dl-wrapper handles package downloads",
            "",
        ]
        .join("\n"),
    )
    .expect("write pkg-download");
    fs::write(
        repo.join("package").join("Config.in"),
        [
            "menu \"Target packages\"",
            "source \"package/foo/Config.in\"",
            "endmenu",
            "",
        ]
        .join("\n"),
    )
    .expect("write package Config.in");
    fs::write(
        repo.join("package").join("foo").join("foo.mk"),
        [
            "FOO_VERSION = 1.2.3",
            "FOO_SITE = https://example.invalid/foo",
            "FOO_LICENSE = MIT",
            "FOO_DEPENDENCIES = host-pkgconf",
            "$(eval $(generic-package))",
            "$(eval $(host-generic-package))",
            "",
        ]
        .join("\n"),
    )
    .expect("write mk");
    fs::write(
        repo.join("package").join("foo").join("Config.in"),
        [
            "config BR2_PACKAGE_FOO",
            "    bool \"foo\"",
            "    depends on BR2_USE_MMU",
            "    select BR2_PACKAGE_ZLIB",
            "",
        ]
        .join("\n"),
    )
    .expect("write Config.in");
    fs::write(
        repo.join("docs")
            .join("manual")
            .join("adding-packages.adoc"),
        [
            "= Adding packages",
            "package infrastructure generic-package Config.in .mk .adoc support/scripts",
            "",
        ]
        .join("\n"),
    )
    .expect("write docs");
    fs::write(
        repo.join("docs")
            .join("manual")
            .join("adding-packages-generic.adoc"),
        [
            "= Infrastructure for packages using generic-package",
            "The package infrastructure documents generic-package and Config.in wiring.",
            "",
        ]
        .join("\n"),
    )
    .expect("write generic docs");
    fs::write(
        repo.join("support").join("scripts").join("pkg-stats"),
        "#!/bin/sh\necho BR2_PACKAGE_FOO\necho generic-package\n",
    )
    .expect("write support script");
    fs::write(
        repo.join("support").join("download").join("dl-wrapper"),
        "#!/bin/sh\necho download wrapper for package infrastructure\n",
    )
    .expect("write download wrapper");
    index_repo(&repo).expect("index broad planning fixture");

    let connection = Connection::open(default_db_path(&repo)).expect("open fixture db");
    let mut options = context_agent_test_options("production", Some(4), Some(12), Some(512 * 1024));
    options.task = [
        "Plan how to add inspect trace find where Buildroot package Config.in",
        "generic-package host-generic-package depends on select license version dependencies",
        "source URL package infrastructure .adoc support/scripts package/foo/foo.mk",
    ]
    .join(" ");
    options.seeds.clear();
    let budgets = super::ContextPackBudgets::for_options(&options);
    let raw_seed_values = super::context_pack_seed_values(&options, budgets.max_seed_entities);
    let fallback_evidence = super::build_context_pack_text_evidence_fallback(
        &connection,
        &options,
        &raw_seed_values,
        budgets,
    )
    .expect("build broad text evidence fallback");
    let fallback_snippets =
        super::load_context_fallback_snippets(&repo, &fallback_evidence, budgets.max_snippets)
            .expect("load fallback snippets");
    let fallback_evidence_count = fallback_evidence.len();
    let packet = super::build_context_packet_from_stored_evidence(
        &options,
        &raw_seed_values,
        &[],
        &[],
        Vec::new(),
        fallback_snippets,
        fallback_evidence,
        None,
        budgets,
        0,
        fallback_evidence_count,
    );
    let response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &json!({"claimable": true, "diagnostic_only": false, "decision": "read_reuse"}),
        budgets,
        &repo,
        &default_db_path(&repo),
        json!({"wall_ms": 1.0}),
    );

    let planning = &response["planning_packet"];
    assert_eq!(planning["graph_proof"].as_bool(), Some(false));
    assert_eq!(planning["evidence_type"].as_str(), Some("text_evidence"));
    let planning_files = planning["likely_files"]
        .as_array()
        .expect("planning likely files")
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    assert!(
        planning_files.contains("docs/manual/adding-packages-generic.adoc")
            && planning_files.contains("package/pkg-generic.mk")
            && planning_files.contains("package/Config.in")
            && planning_files.contains("package/pkg-download.mk")
            && (planning_files.contains("support/download/dl-wrapper")
                || planning_files.contains("support/scripts/pkg-stats")),
        "{planning_files:?}"
    );
    let planning_roles = planning["selected_role_coverage"]
        .as_array()
        .expect("planning roles")
        .iter()
        .filter_map(|role| role["role"].as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        planning_roles.contains("docs_authoring_guidance")
            && planning_roles.contains("makefile_inclusion")
            && planning_roles.contains("download_infrastructure")
            && planning_roles.contains("build_install_infrastructure")
            && planning_roles.contains("support_scripts"),
        "{planning_roles:?}"
    );
    let queries = planning["follow_up_queries"]
        .as_array()
        .expect("planning follow-up queries");
    assert!(
        queries.len() <= super::CONTEXT_PLANNING_PACKET_QUERY_LIMIT,
        "{queries:?}"
    );
    assert!(queries.iter().all(|query| {
        query["source_evidence_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
            && matches!(query["risk"].as_str(), Some("broad" | "precise" | "noisy"))
    }));
    assert!(
        planning_query_exists(queries, "dl-wrapper", "support/download/")
            || planning_query_exists(queries, "pkg-stats", "support/scripts/"),
        "{queries:?}"
    );
    assert!(
        planning["evidence_items"]
            .as_array()
            .expect("planning evidence")
            .iter()
            .all(|item| item["graph_proof"].as_bool() != Some(true)),
        "{planning:?}"
    );
    assert!(
            planning.get("retrieval_architecture").is_none(),
            "retrieval architecture clarity belongs to the top-level agent JSON, not the planning packet"
        );
    let retrieval_architecture = &response["retrieval_architecture"];
    assert_eq!(retrieval_architecture["schema_version"].as_u64(), Some(1));
    assert_eq!(retrieval_architecture["graph_proof"].as_bool(), Some(false));
    assert_eq!(
        retrieval_architecture["prompt_intent"].as_str(),
        Some("build_system_planning")
    );
    assert!(retrieval_architecture["prompt_seed_provenance"]
        .as_array()
        .expect("prompt seed provenance")
        .iter()
        .any(|seed| {
            seed["seed"].as_str() == Some("generic-package")
                && seed["intent_contribution"].as_str() == Some("build_system_planning")
                && seed["exactness"].as_str().is_some()
        }));
    let candidate_flow = retrieval_architecture["candidate_flow"]
        .as_array()
        .expect("candidate flow");
    assert!(
        retrieval_stage_status(candidate_flow, "text_evidence_candidates", "active_current"),
        "{retrieval_architecture:?}"
    );
    assert!(
        retrieval_stage_status(
            candidate_flow,
            "lexical_candidates",
            "bounded_follow_up_queries_only"
        ),
        "{retrieval_architecture:?}"
    );
    assert!(
        retrieval_stage_status(
            candidate_flow,
            "binary_vector_candidates",
            "inactive_post_mvp"
        ),
        "{retrieval_architecture:?}"
    );
    assert!(
        retrieval_stage_status(
            candidate_flow,
            "nuance_rescue_candidates",
            "inactive_requires_explicit_flag"
        ),
        "{retrieval_architecture:?}"
    );
    assert!(
        retrieval_stage_status(
            candidate_flow,
            "graph_neighborhood_candidates",
            "checked_no_proof_path_found"
        ),
        "{retrieval_architecture:?}"
    );
    assert!(retrieval_architecture["inactive_lanes"]
        .as_array()
        .expect("inactive lanes")
        .iter()
        .all(|lane| lane.as_str() != Some("suggested_rg_probes")));

    drop(connection);
    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn context_pack_planning_packet_keeps_role_diverse_evidence_under_package_example_pressure() {
    let make_evidence =
        |file: &str, symbol: &str, text: &str, score: f64| super::ContextPackFallbackEvidence {
            id: format!("text-evidence://{file}:1"),
            symbol: symbol.to_string(),
            kind: "file".to_string(),
            source_span: SourceSpan::new(file, 1, 1),
            score: Some(score),
            evidence_role: EvidenceRole::Unknown,
            evidence_role_label: Some("text_evidence".to_string()),
            proof_status: Some("no_proof_path_found".to_string()),
            graph_proof: false,
            claimability: Some(super::text_evidence_claimability_json()),
            seed_matches: vec![text.to_string()],
            follow_up_queries: vec![text.to_string()],
            classification_reason:
                "matched bounded text evidence via stage0_fts; budget pressure fixture".to_string(),
            classification_source: "unit-test/stage0_fts".to_string(),
            fallback_source: "text_evidence/no_proof_path_found".to_string(),
        };
    let mut candidates = vec![
        make_evidence(
            "docs/manual/adding-packages-generic.adoc",
            "adding packages generic",
            "generic-package package infrastructure Config.in",
            0.1,
        ),
        make_evidence(
            "package/pkg-generic.mk",
            "pkg-generic",
            "generic-package install_target install_staging",
            0.2,
        ),
        make_evidence(
            "package/Config.in",
            "Config.in",
            "source \"package/foo/Config.in\" BR2_PACKAGE_FOO",
            0.3,
        ),
        make_evidence(
            "package/pkg-download.mk",
            "pkg-download",
            "download dl-wrapper _SITE",
            0.4,
        ),
        make_evidence(
            "support/download/dl-wrapper",
            "dl-wrapper",
            "download wrapper support script",
            0.5,
        ),
    ];
    for index in 0..20 {
        candidates.push(make_evidence(
            &format!("package/example{index}/example{index}.mk"),
            &format!("EXAMPLE{index}_VERSION"),
            "EXAMPLE_VERSION EXAMPLE_LICENSE $(eval $(generic-package))",
            index as f64 + 10.0,
        ));
    }

    let selected = super::context_pack_select_role_diverse_fallback_evidence(candidates, 6);
    let selected_files = selected
        .iter()
        .map(|evidence| evidence.source_span.repo_relative_path.as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        selected_files.contains("docs/manual/adding-packages-generic.adoc"),
        "{selected_files:?}"
    );
    assert!(
        selected_files.contains("package/pkg-generic.mk"),
        "{selected_files:?}"
    );
    assert!(
        selected_files.contains("package/Config.in"),
        "{selected_files:?}"
    );
    assert!(
        selected_files.contains("package/pkg-download.mk")
            || selected_files.contains("support/download/dl-wrapper"),
        "{selected_files:?}"
    );
    let example_count = selected_files
        .iter()
        .filter(|file| file.starts_with("package/example"))
        .count();
    assert!(example_count <= 2, "{selected_files:?}");

    let options = context_agent_test_options("production", Some(3), Some(6), Some(128 * 1024));
    let budgets = super::ContextPackBudgets::for_options(&options);
    let packet = super::build_context_packet_from_stored_evidence(
        &options,
        &[],
        &[],
        &[],
        Vec::new(),
        Vec::new(),
        selected,
        None,
        budgets,
        0,
        25,
    );
    let response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &json!({"claimable": true, "diagnostic_only": false, "decision": "read_reuse"}),
        budgets,
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    let likely_files = response["likely_files"]
        .as_array()
        .expect("likely files")
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    assert!(
        likely_files.contains("docs/manual/adding-packages-generic.adoc")
            && likely_files.contains("package/pkg-generic.mk")
            && likely_files.contains("package/Config.in"),
        "{likely_files:?}"
    );
    assert!(
        response["omitted_by_budget"].as_u64().unwrap_or_default() > 0
            || response["omitted_by_dedup"].as_u64().unwrap_or_default() > 0,
        "{response:?}"
    );
    assert!(serde_json::to_vec(&response).expect("serialize").len() <= 128 * 1024);
}

#[test]
fn context_pack_explain_budget_summary_does_not_starve_fallback_snippets() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), Some(12_000));
    options.explain = true;
    options.task = "Trace Buildroot generic package flow".to_string();
    let budgets = super::ContextPackBudgets::for_options(&options);
    let fallback_evidence = vec![super::ContextPackFallbackEvidence {
        id: "text-evidence://docs/manual/adding-packages-generic.adoc:1".to_string(),
        symbol: "adding packages generic".to_string(),
        kind: "documentation".to_string(),
        source_span: SourceSpan::new("docs/manual/adding-packages-generic.adoc", 1, 1),
        score: Some(1.0),
        evidence_role: EvidenceRole::Unknown,
        evidence_role_label: Some("text_evidence".to_string()),
        proof_status: Some("no_proof_path_found".to_string()),
        graph_proof: false,
        claimability: Some(super::text_evidence_claimability_json()),
        seed_matches: vec!["generic-package".to_string()],
        follow_up_queries: vec!["generic-package".to_string()],
        classification_reason: "matched bounded text evidence via stage0_fts".to_string(),
        classification_source: "unit-test/stage0_fts".to_string(),
        fallback_source: "text_evidence/no_proof_path_found".to_string(),
    }];
    let snippets = vec![ContextSnippet {
        file: "docs/manual/adding-packages-generic.adoc".to_string(),
        lines: "1".to_string(),
        text: "The generic-package infrastructure defines Buildroot package flow.".to_string(),
        reason: "text evidence fallback".to_string(),
    }];
    let mut packet = super::build_context_packet_from_stored_evidence(
        &options,
        &["Buildroot".to_string()],
        &[],
        &[],
        Vec::new(),
        snippets,
        fallback_evidence,
        None,
        budgets,
        0,
        1,
    );
    packet.metadata.insert(
        "vector_semantic_candidates".to_string(),
        json!((0..64)
            .map(|index| json!({
                "candidate_id": format!("vector://noise/{index}"),
                "candidate_source": "vector_semantic",
                "candidate_sources": ["vector_semantic"],
                "path": format!("package/noise{index}/noise{index}.mk"),
                "evidence_role": "production",
                "proof_status": "candidate_only",
                "graph_proof": false,
                "claimable": false,
                "claimable_for_text": false,
                "claimable_for_graph": false,
                "requires_graph_verification": true,
                "verification_status": "needs_graph_verification",
                "graph_verification_status": "needs_graph_verification",
                "matched_seeds": [],
                "reason": "noisy vector candidate",
                "ranking_features": {}
            }))
            .collect::<Vec<_>>()),
    );
    let response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &json!({"claimable": true, "diagnostic_only": false, "decision": "read_reuse"}),
        budgets,
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    assert!(!response["fallback_snippets"]
        .as_array()
        .expect("fallback snippets")
        .is_empty());
    assert_eq!(
        response["retrieval_explain"]["budget_limited"].as_bool(),
        Some(true)
    );
    assert_eq!(
        response["explain_budget_status"]["status"].as_str(),
        Some("summary_included_full_explain_omitted_by_explain_budget")
    );
    assert!(response["retrieval_explain"]["budget_decisions"]
        .as_array()
        .expect("budget decisions")
        .iter()
        .any(|item| item
            .as_str()
            .is_some_and(|text| text.contains("evidence budget"))));
}

#[test]
fn explain_audit_seed_trace_available() {
    let retrieval_explain = json!({
        "proof_status": "proof_path_found",
        "graph_proof": true,
        "seed_hygiene": {
            "schema_version": 1,
            "diagnostic_only": true,
            "accepted_exact_seeds": [{"seed": "target_func"}],
            "ignored_prose_terms": ["Markdown", "agent-use"]
        },
        "seeds_extracted": [{"seed": "target_func"}],
        "ignored_seeds": []
    });
    let summary =
        super::context_pack_retrieval_explain_budget_summary(&retrieval_explain, 24_000, 12_288);

    assert_eq!(
        summary["seed_hygiene"]["accepted_exact_seeds"][0]["seed"].as_str(),
        Some("target_func")
    );
    assert!(summary["seed_hygiene"]["ignored_prose_terms"]
        .as_array()
        .expect("ignored prose")
        .iter()
        .any(|value| value.as_str() == Some("Markdown")));
    assert_eq!(
        summary["seeds_extracted"][0]["seed"].as_str(),
        Some("target_func")
    );
    let compact_seed_summary = super::context_pack_seed_hygiene_summary_json(&retrieval_explain)
        .expect("seed hygiene summary");
    assert_eq!(
        compact_seed_summary["accepted_exact_seeds"][0].as_str(),
        Some("target_func")
    );
    assert!(compact_seed_summary["ignored_prose_terms"]
        .as_array()
        .expect("ignored prose summary")
        .iter()
        .any(|value| value.as_str() == Some("agent-use")));
}

#[test]
fn context_pack_agent_json_labels_empty_packet_as_unknown_no_evidence() {
    let options = context_agent_test_options("production", Some(3), Some(3), None);
    let budgets = super::ContextPackBudgets::for_options(&options);
    let packet = super::build_context_packet_from_stored_evidence(
        &options,
        &[],
        &[],
        &[],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        budgets,
        0,
        0,
    );
    let response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &json!({"claimable": true, "diagnostic_only": false, "decision": "read_reuse"}),
        budgets,
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );

    assert_eq!(response["proof_path_available"].as_bool(), Some(false));
    assert_eq!(response["proof_status"].as_str(), Some("unknown"));
    assert_eq!(
        response["evidence_status"].as_str(),
        Some("no_evidence_found")
    );
    assert_eq!(response["graph_proof"].as_bool(), Some(false));
    assert_eq!(
        response["graph_verification"]["status"].as_str(),
        Some("no_graph_candidates")
    );
    assert_eq!(
        response["graph_verification"]["proof_status"].as_str(),
        Some("unknown")
    );
    assert_eq!(
        response["graph_verification"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert!(response["proof_failure_reason"]
        .as_str()
        .is_some_and(|reason| reason.contains("no graph-verifiable candidates")));
    assert_eq!(response["claimability"]["claimable"].as_bool(), Some(false));
    assert!(response["paths"].as_array().expect("paths").is_empty());
    assert!(response["snippets"]
        .as_array()
        .expect("snippets")
        .is_empty());
    assert!(response["fallback_evidence"]
        .as_array()
        .expect("fallback evidence")
        .is_empty());
    let planning = &response["planning_packet"];
    assert_eq!(planning["evidence_type"].as_str(), Some("unknown"));
    assert_eq!(planning["proof_status"].as_str(), Some("unknown"));
    assert_eq!(planning["claimable"].as_bool(), Some(false));
    if let Some(evidence_items) = planning["evidence_items"].as_array() {
        assert!(evidence_items.is_empty());
    } else {
        assert!(
            response["omitted"]["planning_packet"]
                .as_u64()
                .unwrap_or_default()
                > 0
                || response["omitted_count"].as_u64().unwrap_or_default() > 0,
            "compacted planning packet should report omissions: {response}"
        );
    }
    if let Some(follow_up_queries) = planning["follow_up_queries"].as_array() {
        assert!(follow_up_queries.is_empty());
    }
    if let Some(unknowns) = planning["unknowns"].as_array() {
        assert!(unknowns
            .iter()
            .any(|value| value.as_str() == Some("no_follow_up_query_found")));
    }
}

#[test]
fn no_evidence_found_preserved_for_nonsense() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task = "Trace zzz_nonsense_seed_94831".to_string();
    options.seeds = vec!["zzz_nonsense_seed_94831".to_string()];
    let budgets = super::ContextPackBudgets::for_options(&options);
    let packet = super::build_context_packet_from_stored_evidence(
        &options,
        &options.seeds,
        &options.seeds,
        &[],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        budgets,
        0,
        0,
    );
    let response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &json!({"claimable": true, "diagnostic_only": false, "decision": "read_reuse"}),
        budgets,
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );

    assert_eq!(
        response["evidence_status"].as_str(),
        Some("no_evidence_found")
    );
    assert_eq!(response["graph_proof"].as_bool(), Some(false));
    assert_eq!(response["proof_path_available"].as_bool(), Some(false));
}

#[test]
fn context_pack_agent_json_makes_graph_verification_failure_explicit() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task = "Trace prod_value behavior".to_string();
    options.seeds = vec!["prod_value".to_string()];
    let budgets = super::ContextPackBudgets::for_options(&options);
    let raw_seed_values = vec!["prod_value".to_string()];
    let seed_ids = vec!["entity:prod_value".to_string()];
    let packet = super::build_context_packet_from_stored_evidence(
        &options,
        &raw_seed_values,
        &seed_ids,
        &[],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        budgets,
        0,
        0,
    );
    let response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &json!({"claimable": true, "diagnostic_only": false, "decision": "read_reuse"}),
        budgets,
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );

    assert_eq!(response["proof_path_available"].as_bool(), Some(false));
    assert_eq!(response["proof_status"].as_str(), Some("unknown"));
    assert_eq!(
        response["evidence_status"].as_str(),
        Some("no_evidence_found")
    );
    assert_eq!(response["graph_proof"].as_bool(), Some(false));
    assert_eq!(
        response["graph_verification"]["status"].as_str(),
        Some("no_proof_path_found")
    );
    assert_eq!(
        response["graph_verification"]["candidate_count"].as_u64(),
        Some(1)
    );
    assert!(response["graph_verification"]["reason"]
        .as_str()
        .is_some_and(|reason| reason.contains("no proof path")));
    assert!(response["proof_failure_reason"]
        .as_str()
        .is_some_and(|reason| reason.contains("no proof path")));

    let candidates = response["candidates"].as_array().expect("candidates");
    let exact = candidates
        .iter()
        .find(|candidate| {
            candidate["candidate_sources"]
                .as_array()
                .is_some_and(|sources| {
                    sources
                        .iter()
                        .any(|source| source.as_str() == Some("exact_seed"))
                })
        })
        .unwrap_or_else(|| panic!("exact seed candidate in {candidates:?}"));
    assert_eq!(
        exact["verification_status"].as_str(),
        Some("no_proof_path_found")
    );
    assert_eq!(exact["proof_status"].as_str(), Some("no_proof_path_found"));
    assert_eq!(exact["graph_proof"].as_bool(), Some(false));
    assert!(exact["reason"]
        .as_str()
        .is_some_and(|reason| reason.contains("no proof path")));
}

#[test]
fn context_pack_agent_json_keeps_test_impact_labels_explicit() {
    let options = context_agent_test_options("test-impact", Some(2), Some(2), None);
    let packet = context_agent_test_packet("test-impact", 1, 1, "test");
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
    });
    let result = super::context_pack_agent_json_response(
        &options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );

    assert_eq!(result["mode"].as_str(), Some("test-impact"));
    assert_eq!(result["paths"][0]["evidence_role"].as_str(), Some("test"));
    assert_eq!(
        result["paths"][0]["classification_reason"].as_str(),
        Some("qualified name contains tests module")
    );
    assert_eq!(
        result["snippets"][0]["evidence_role"].as_str(),
        Some("test")
    );
}

#[test]
fn context_pack_agent_json_respects_max_output_bytes_without_dropping_labels() {
    let max_output_bytes = 4096usize;
    let options =
        context_agent_test_options("production", Some(8), Some(8), Some(max_output_bytes));
    let packet = context_agent_test_packet("production", 10, 10, "production");
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
    });
    let result = super::context_pack_agent_json_response(
        &options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );

    let serialized_len = serialized_len_for_test(&result);
    if serialized_len > max_output_bytes {
        assert!(
            result["warnings"]
                .as_array()
                .is_some_and(|warnings| warnings
                    .iter()
                    .any(|warning| warning["code"].as_str() == Some("max_output_bytes_exceeded"))),
            "{} bytes without max_output_bytes_exceeded warning: {result}",
            serialized_len
        );
    }
    assert!(result["omitted_count"].as_u64().unwrap_or_default() > 0);
    assert_eq!(result["truncation"]["limit_applied"].as_bool(), Some(true));
    assert_eq!(
        result["limits"]["max_output_bytes"].as_u64(),
        Some(max_output_bytes as u64)
    );
    for key in [
        "claimability",
        "db_lifecycle_read",
        "graph_verification",
        "lifecycle",
        "proof_status",
        "proof_strength",
        "graph_proof",
    ] {
        assert!(
            !result[key].is_null(),
            "{key} dropped from compact agent JSON"
        );
    }
    assert_eq!(result["claimability"]["claimable"].as_bool(), Some(true));
    assert_eq!(
        result["graph_verification"]["status"].as_str(),
        Some("graph_verified")
    );
    for path in result["paths"].as_array().expect("paths") {
        assert!(path["evidence_role"].as_str().is_some());
        assert!(path["classification_reason"].as_str().is_some());
        assert!(path["source_spans"]
            .as_array()
            .is_some_and(|spans| !spans.is_empty()));
    }
}

#[test]
fn context_pack_agent_json_production_excludes_inline_test_evidence() {
    let production_path = context_agent_test_path("prod-path", 6, "production");
    let test_path = context_agent_test_path("test-path", 18, "test");
    let budgets = super::ContextPackBudgets::for_mode("production");

    let production_paths = super::filter_and_sort_context_path_evidence(
        vec![production_path.clone(), test_path.clone()],
        "production",
        budgets,
    );
    assert_eq!(production_paths.len(), 1);
    assert_eq!(
        production_paths[0]
            .metadata
            .get("evidence_role")
            .and_then(Value::as_str),
        Some("production")
    );

    let test_paths = super::filter_and_sort_context_path_evidence(
        vec![production_path, test_path],
        "test-impact",
        budgets,
    );
    assert!(
        test_paths
            .iter()
            .any(|path| path.metadata.get("evidence_role").and_then(Value::as_str) == Some("test")),
        "{test_paths:?}"
    );
}

#[test]
fn retrieval_architecture_fixture_gate_covers_core_contract() {
    let mut covered = BTreeSet::new();
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
        "passport_status": "valid",
    });

    {
        let compact_options = context_agent_test_options("production", Some(2), Some(2), None);
        let compact_packet = context_agent_test_packet("production", 1, 1, "production");
        let compact_response = super::context_pack_agent_json_response(
            &compact_options,
            &compact_packet,
            &lifecycle,
            super::ContextPackBudgets::for_options(&compact_options),
            Path::new("fixture"),
            Path::new("fixture/.codegraph/codegraph.sqlite"),
            json!({"wall_ms": 1.0}),
        );
        assert!(compact_response.get("retrieval_explain").is_none());
        assert_eq!(
            compact_response["proof_status"].as_str(),
            Some("proof_path_found")
        );
        assert_eq!(compact_response["graph_proof"].as_bool(), Some(true));
        assert!(
            serialized_len_for_test(&compact_response)
                <= super::DEFAULT_CONTEXT_AGENT_MAX_OUTPUT_BYTES
        );
        covered.insert("agent_json_compact");

        let mut explain_options =
            context_agent_test_options("production", Some(2), Some(2), Some(512 * 1024));
        explain_options.explain = true;
        let proof_response = super::context_pack_agent_json_response(
            &explain_options,
            &compact_packet,
            &lifecycle,
            super::ContextPackBudgets::for_options(&explain_options),
            Path::new("fixture"),
            Path::new("fixture/.codegraph/codegraph.sqlite"),
            json!({"wall_ms": 1.0}),
        );
        assert_agent_json_contract(
            &proof_response,
            "context_pack_agent_json",
            "context-pack",
            512 * 1024,
        );
        assert_eq!(
            proof_response["graph_verification"]["status"].as_str(),
            Some("graph_verified")
        );
        assert!(proof_response["proof_failure_reason"].is_null());
        let proof_candidates = proof_response["candidates"]
            .as_array()
            .expect("proof candidates");
        assert!(proof_candidates.iter().any(|candidate| {
            candidate["candidate_sources"]
                .as_array()
                .is_some_and(|sources| {
                    sources
                        .iter()
                        .any(|source| source.as_str() == Some("path_evidence"))
                })
                && candidate["verification_status"].as_str() == Some("graph_verified")
                && candidate["claimable_for_graph"].as_bool() == Some(true)
        }));
        let explain = &proof_response["retrieval_explain"];
        assert_eq!(explain["diagnostic_only"].as_bool(), Some(true));
        assert_eq!(explain["proof_paths_found"].as_u64(), Some(1));
        assert_eq!(
            explain["graph_verification_attempts"]["status"].as_str(),
            Some("graph_verified")
        );
        assert!(explain["candidate_sources"]
            .as_array()
            .expect("candidate sources")
            .iter()
            .any(|source| source.as_str() == Some("path_evidence")));
        covered.insert("exact_symbol_proof_path");
        covered.insert("graph_proof_still_works");
        covered.insert("explain_mode_shows_retrieval_stages");
    }

    {
        let fixture = caller_callee_precision_fixture();
        let result = query_call_relation(
            &fixture.repo,
            CallRelationQueryOptions {
                query: Some("target".to_string()),
                entity_id: None,
                limit: 32,
                exact_resolved: false,
                fuzzy: false,
            },
            CallQueryDirection::Callers,
        )
        .expect("query ambiguous callers");
        assert_eq!(result["status"].as_str(), Some("ambiguous_symbol"));
        let candidates = result["ambiguous_symbol_matches"]
            .as_array()
            .expect("ambiguous candidates");
        assert!(candidates.iter().any(|candidate| {
            candidate["id"].as_str() == Some(fixture.alpha_target_id.as_str())
        }));
        assert!(candidates.iter().any(|candidate| {
            candidate["id"].as_str() == Some(fixture.beta_target_id.as_str())
        }));
        assert!(result["exact_resolved_entity_results"]
            .as_array()
            .expect("exact results")
            .is_empty());
        remove_dir_all_with_retry(&fixture.repo, "cleanup");
        covered.insert("same_name_ambiguity");
    }

    {
        let repo = temp_repo();
        fs::create_dir_all(repo.join("package").join("foo")).expect("create package");
        fs::create_dir_all(repo.join("docs").join("manual")).expect("create docs");
        fs::create_dir_all(repo.join("support").join("scripts")).expect("create support");
        fs::write(
            repo.join("package").join("foo").join("foo.mk"),
            [
                "FOO_VERSION = 1.2.3",
                "FOO_SITE = https://example.invalid/foo",
                "FOO_LICENSE = MIT",
                "FOO_DEPENDENCIES = host-pkgconf",
                "$(eval $(generic-package))",
                "",
            ]
            .join("\n"),
        )
        .expect("write mk");
        fs::write(
            repo.join("package").join("foo").join("Config.in"),
            [
                "config BR2_PACKAGE_FOO",
                "    bool \"foo\"",
                "    depends on BR2_USE_MMU",
                "    select BR2_PACKAGE_ZLIB",
                "",
            ]
            .join("\n"),
        )
        .expect("write Config.in");
        fs::write(
            repo.join("docs")
                .join("manual")
                .join("adding-packages.adoc"),
            [
                "= Adding packages",
                "",
                "The package infrastructure uses generic-package helpers.",
                "Every package should add a Config.in entry.",
                "",
            ]
            .join("\n"),
        )
        .expect("write docs");
        fs::write(
            repo.join("support").join("scripts").join("pkg-stats"),
            "#!/bin/sh\necho BR2_PACKAGE_FOO\necho generic-package\n",
        )
        .expect("write support script");
        index_repo(&repo).expect("index buildroot fixture gate repo");

        let generic_text_options = parse_list_query_args(
            "text",
            &[
                "generic-package".to_string(),
                "--agent-json".to_string(),
                "--limit=8".to_string(),
            ],
        )
        .expect("parse query text");
        let generic_text = query_text_with_options(&repo, &generic_text_options, Some(&lifecycle))
            .expect("query text evidence");
        let generic_text_span = generic_text["results"]
            .as_array()
            .expect("generic text results")
            .iter()
            .find(|result| result["file"].as_str() == Some("package/foo/foo.mk"))
            .map(|result| result["span"].clone())
            .expect("generic-package span");

        let connection = Connection::open(default_db_path(&repo)).expect("open fixture db");
        let mut options =
            context_agent_test_options("production", Some(3), Some(8), Some(512 * 1024));
        options.task = "Plan how to add a new Buildroot package using Config.in generic-package and BR2_PACKAGE_FOO".to_string();
        options.token_budget = 20_000;
        options.explain = true;
        options.seeds.clear();
        let budgets = super::ContextPackBudgets::for_options(&options);
        let raw_seed_values = super::context_pack_seed_values(&options, budgets.max_seed_entities);
        let fallback_evidence = super::build_context_pack_text_evidence_fallback(
            &connection,
            &options,
            &raw_seed_values,
            budgets,
        )
        .expect("build text evidence fallback");
        let fallback_files = fallback_evidence
            .iter()
            .map(|evidence| evidence.source_span.repo_relative_path.as_str())
            .collect::<BTreeSet<_>>();
        assert!(
            fallback_files.contains("package/foo/foo.mk"),
            "{fallback_files:?}"
        );
        assert!(
            fallback_files.contains("package/foo/Config.in"),
            "{fallback_files:?}"
        );
        assert!(
            fallback_files.contains("docs/manual/adding-packages.adoc"),
            "{fallback_files:?}"
        );
        assert!(fallback_evidence.iter().all(|evidence| {
            evidence.evidence_role_label.as_deref() == Some("text_evidence")
                && evidence.proof_status.as_deref() == Some("no_proof_path_found")
                && !evidence.graph_proof
        }));
        let fallback_snippets =
            super::load_context_fallback_snippets(&repo, &fallback_evidence, budgets.max_snippets)
                .expect("load fallback snippets");
        let fallback_evidence_count = fallback_evidence.len();
        let packet = super::build_context_packet_from_stored_evidence(
            &options,
            &raw_seed_values,
            &[],
            &[],
            Vec::new(),
            fallback_snippets,
            fallback_evidence,
            None,
            budgets,
            0,
            fallback_evidence_count,
        );
        let response = super::context_pack_agent_json_response(
            &options,
            &packet,
            &lifecycle,
            budgets,
            &repo,
            &default_db_path(&repo),
            json!({"wall_ms": 1.0}),
        );
        assert_eq!(
            response["proof_status"].as_str(),
            Some("no_proof_path_found")
        );
        assert_eq!(response["graph_proof"].as_bool(), Some(false));
        assert_eq!(
            response["graph_verification"]["status"].as_str(),
            Some("no_graph_candidates")
        );
        assert_eq!(
            response["planning_packet"]["evidence_type"].as_str(),
            Some("text_evidence")
        );
        let text_candidate = response["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .find(|candidate| candidate["path"].as_str() == Some("package/foo/foo.mk"))
            .unwrap_or_else(|| panic!("foo.mk candidate in {response:?}"));
        assert_eq!(text_candidate["span"], generic_text_span);
        assert_eq!(
            text_candidate["evidence_role"].as_str(),
            Some("text_evidence")
        );
        assert_eq!(text_candidate["claimable_for_text"].as_bool(), Some(true));
        assert_eq!(text_candidate["claimable_for_graph"].as_bool(), Some(false));
        assert!(text_candidate["candidate_sources"]
            .as_array()
            .expect("candidate sources")
            .iter()
            .any(|source| source.as_str() == Some("lexical_fts")));
        assert!(
            response["fallback_evidence"]
                .as_array()
                .expect("fallback evidence")
                .iter()
                .all(|evidence| evidence.get("edges").is_none()
                    && evidence.get("relations").is_none())
        );
        assert!(response["snippets"]
            .as_array()
            .expect("snippets")
            .iter()
            .all(|snippet| snippet["graph_relation_claims"]
                .as_array()
                .is_some_and(Vec::is_empty)));
        assert_eq!(
            response["retrieval_architecture"]["prompt_intent"].as_str(),
            Some("build_system_planning")
        );
        let candidate_flow = response["retrieval_architecture"]["candidate_flow"]
            .as_array()
            .expect("candidate flow");
        assert!(retrieval_stage_status(
            candidate_flow,
            "text_evidence_candidates",
            "active_current"
        ));
        assert!(
            response["retrieval_explain"]["candidate_counts_by_source"]
                .as_object()
                .expect("candidate counts")
                .get("text_evidence")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                > 0
        );
        drop(connection);
        remove_dir_all_with_retry(&repo, "cleanup");
        covered.insert("text_only_buildroot_planning");
        covered.insert("text_evidence_reaches_fallback");
        covered.insert("no_proof_fallback_labeled");
        covered.insert("no_graph_proof_overclaim");
    }

    {
        let mut options =
            context_agent_test_options("production", Some(3), Some(8), Some(512 * 1024));
        options.task = "Plan generic-package changes".to_string();
        options.seeds = vec!["package/foo/foo.mk".to_string()];
        let packet = ContextPacket {
            task: options.task.clone(),
            mode: options.mode.clone(),
            symbols: Vec::new(),
            verified_paths: Vec::new(),
            risks: Vec::new(),
            recommended_tests: Vec::new(),
            snippets: Vec::new(),
            metadata: Metadata::new(),
        };
        let snippets = [
            "package/foo/foo.mk",
            "package/bar/bar.mk",
            "package/baz/baz.mk",
            "package/qux/qux.mk",
        ]
        .into_iter()
        .map(|file| {
            json!({
                "file": file,
                "lines": "5",
                "text": "$(eval $(generic-package))",
                "reason": "text evidence fallback"
            })
        })
        .collect::<Vec<_>>();
        let fallback_evidence = snippets
            .iter()
            .enumerate()
            .map(|(index, snippet)| {
                let file = snippet["file"].as_str().expect("snippet file");
                json!({
                    "id": format!("text-evidence://candidate-{index}"),
                    "symbol": file,
                    "kind": "makefile",
                    "file": file,
                    "source_span": {
                        "file": file,
                        "start_line": 5,
                        "start_column": 1,
                        "end_line": 5,
                        "end_column": 32
                    },
                    "evidence_role": "text_evidence",
                    "proof_status": "no_proof_path_found",
                    "graph_proof": false,
                    "matched_seeds": ["generic-package"],
                    "score": index as f64,
                    "classification_reason": "matched bounded text evidence via stage0_fts",
                    "classification_source": "stage0_fts/text_evidence"
                })
            })
            .collect::<Vec<_>>();
        let candidate_set = super::build_context_agent_retrieval_candidates(
            &options,
            &packet,
            &[],
            &fallback_evidence,
            &snippets,
            true,
            false,
        );
        assert!(
            candidate_set.total_count > super::CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT,
            "{candidate_set:?}"
        );
        let exact_text = candidate_set
            .candidates
            .iter()
            .find(|candidate| candidate["path"].as_str() == Some("package/foo/foo.mk"))
            .expect("exact path seed survives cap and merges with text evidence");
        let sources = exact_text["candidate_sources"]
            .as_array()
            .expect("candidate sources")
            .iter()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>();
        assert!(sources.contains("exact_seed"), "{sources:?}");
        assert!(sources.contains("file_path_seed"), "{sources:?}");
        assert!(sources.contains("text_evidence"), "{sources:?}");
        assert!(sources.contains("lexical_fts"), "{sources:?}");
        assert_eq!(exact_text["rank"].as_u64(), Some(1));
        assert_eq!(
            exact_text["verification_status"].as_str(),
            Some("no_proof_path_found")
        );
        assert_eq!(exact_text["graph_proof"].as_bool(), Some(false));
        assert_eq!(exact_text["claimable_for_text"].as_bool(), Some(true));
        assert_eq!(exact_text["claimable_for_graph"].as_bool(), Some(false));
        covered.insert("mixed_exact_text_evidence");
        covered.insert("noisy_text_candidates_exact_seed_survives");
        covered.insert("candidate_provenance_recorded");
    }

    {
        let mut options = context_agent_test_options("production", Some(3), Some(3), None);
        options.task = "Find".to_string();
        options.seeds.clear();
        let budgets = super::ContextPackBudgets::for_options(&options);
        let packet = super::build_context_packet_from_stored_evidence(
            &options,
            &[],
            &[],
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            budgets,
            0,
            0,
        );
        let response = super::context_pack_agent_json_response(
            &options,
            &packet,
            &lifecycle,
            budgets,
            Path::new("fixture"),
            Path::new("fixture/.codegraph/codegraph.sqlite"),
            json!({"wall_ms": 1.0}),
        );
        assert_eq!(response["proof_status"].as_str(), Some("unknown"));
        assert_eq!(
            response["evidence_status"].as_str(),
            Some("no_evidence_found")
        );
        assert_eq!(
            response["graph_verification"]["status"].as_str(),
            Some("no_graph_candidates")
        );
        assert!(response["candidates"]
            .as_array()
            .expect("candidates")
            .is_empty());
        covered.insert("no_evidence_unknown");
    }

    {
        let mut options =
            context_agent_test_options("production", Some(3), Some(3), Some(512 * 1024));
        options.task = "Trace MissingSymbol behavior".to_string();
        options.seeds = vec!["MissingSymbol".to_string()];
        let budgets = super::ContextPackBudgets::for_options(&options);
        let raw_seed_values = vec!["MissingSymbol".to_string()];
        let seed_ids = vec!["entity:MissingSymbol".to_string()];
        let fallback_span = SourceSpan::with_columns("docs/missing.md", 4, 1, 4, 52);
        let fallback_evidence = vec![super::ContextPackFallbackEvidence {
            id: "text-evidence://missing-symbol".to_string(),
            symbol: "MissingSymbol".to_string(),
            kind: "documentation".to_string(),
            source_span: fallback_span,
            score: Some(4.0),
            evidence_role: EvidenceRole::Unknown,
            evidence_role_label: Some("text_evidence".to_string()),
            proof_status: Some("no_proof_path_found".to_string()),
            graph_proof: false,
            claimability: Some(super::text_evidence_claimability_json()),
            seed_matches: vec!["MissingSymbol".to_string()],
            follow_up_queries: vec!["MissingSymbol docs".to_string()],
            classification_reason: "matched fallback source text after graph verification failed"
                .to_string(),
            classification_source: "unit-test/text_evidence".to_string(),
            fallback_source: "text_evidence".to_string(),
        }];
        let snippets = vec![ContextSnippet {
            file: "docs/missing.md".to_string(),
            lines: "4".to_string(),
            text: "MissingSymbol is documented only as text evidence.".to_string(),
            reason: "text evidence fallback".to_string(),
        }];
        let packet = super::build_context_packet_from_stored_evidence(
            &options,
            &raw_seed_values,
            &seed_ids,
            &[],
            Vec::new(),
            snippets,
            fallback_evidence,
            None,
            budgets,
            0,
            1,
        );
        let response = super::context_pack_agent_json_response(
            &options,
            &packet,
            &lifecycle,
            budgets,
            Path::new("fixture"),
            Path::new("fixture/.codegraph/codegraph.sqlite"),
            json!({"wall_ms": 1.0}),
        );
        assert_eq!(
            response["graph_verification"]["status"].as_str(),
            Some("no_proof_path_found")
        );
        assert_eq!(
            response["proof_status"].as_str(),
            Some("no_proof_path_found")
        );
        assert_eq!(
            response["evidence_status"].as_str(),
            Some("fallback_evidence_found")
        );
        assert!(response["proof_failure_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("returning fallback")));
        assert_eq!(response["graph_proof"].as_bool(), Some(false));
        assert_eq!(
            response["fallback_evidence"][0]["evidence_role"].as_str(),
            Some("text_evidence")
        );
        assert_eq!(
            response["snippets"][0]["proof_status"].as_str(),
            Some("no_proof_path_found")
        );
        assert!(response["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .any(|candidate| {
                candidate["candidate_sources"]
                    .as_array()
                    .is_some_and(|sources| {
                        sources
                            .iter()
                            .any(|source| source.as_str() == Some("exact_seed"))
                    })
                    && candidate["verification_status"].as_str() == Some("no_proof_path_found")
            }));
        covered.insert("graph_verification_failure_with_fallback");
    }

    {
        let production_path = context_agent_test_path("gate-prod-path", 6, "production");
        let test_path = context_agent_test_path("gate-test-path", 18, "test");
        let budgets = super::ContextPackBudgets::for_mode("production");
        let production_paths = super::filter_and_sort_context_path_evidence(
            vec![production_path.clone(), test_path.clone()],
            "production",
            budgets,
        );
        assert_eq!(production_paths.len(), 1);
        assert_eq!(production_paths[0].id, "gate-prod-path");
        let test_paths = super::filter_and_sort_context_path_evidence(
            vec![production_path, test_path],
            "test-impact",
            budgets,
        );
        assert!(test_paths.iter().any(|path| path.id == "gate-test-path"));
        covered.insert("test_evidence_excluded_in_production_mode");
    }

    let expected = BTreeSet::from([
        "agent_json_compact",
        "candidate_provenance_recorded",
        "exact_symbol_proof_path",
        "explain_mode_shows_retrieval_stages",
        "graph_proof_still_works",
        "graph_verification_failure_with_fallback",
        "mixed_exact_text_evidence",
        "no_evidence_unknown",
        "no_graph_proof_overclaim",
        "no_proof_fallback_labeled",
        "noisy_text_candidates_exact_seed_survives",
        "same_name_ambiguity",
        "test_evidence_excluded_in_production_mode",
        "text_evidence_reaches_fallback",
        "text_only_buildroot_planning",
    ]);
    assert_eq!(covered, expected);
}

#[test]
fn context_pack_test_impact_fallback_surfaces_inline_test_seed_without_path() {
    let repo = rust_inline_context_pack_fixture();
    index_repo(&repo).expect("index inline Rust fixture");
    let connection = Connection::open(default_db_path(&repo)).expect("open fixture db");
    let mut options = context_agent_test_options("test-impact", Some(4), Some(4), None);
    options.task = "greet_works".to_string();
    options.seeds = vec!["greet_works".to_string()];
    let budgets = super::ContextPackBudgets::for_options(&options);
    let raw_seed_values = super::context_pack_seed_values(&options, budgets.max_seed_entities);
    let seed_entities = super::resolve_context_seed_entities(
        &connection,
        &raw_seed_values,
        budgets.max_seed_entities,
    )
    .expect("resolve greet_works seed");
    let seed_ids =
        super::context_seed_ids(&raw_seed_values, &seed_entities, budgets.max_seed_entities);
    let test_seed = seed_entities
        .iter()
        .find(|entity| entity.name == "greet_works")
        .expect("greet_works entity");
    let test_seed_role = super::context_entity_role(test_seed);
    assert_eq!(test_seed_role.role, EvidenceRole::Test);
    assert!(
        test_seed_role.reason.contains("tests module")
            || test_seed_role.reason.contains("source_role")
            || test_seed_role.reason.contains("entity kind"),
        "{test_seed_role:?}"
    );

    let test_module_entities = super::resolve_context_seed_entities(
        &connection,
        &["tests".to_string()],
        budgets.max_seed_entities,
    )
    .expect("resolve tests module seed");
    assert!(
        test_module_entities.iter().any(|entity| {
            entity.qualified_name.ends_with(".tests")
                && super::context_entity_role(entity).role == EvidenceRole::Test
        }),
        "{test_module_entities:?}"
    );

    let local_edges =
        super::load_bounded_context_edges(&connection, &seed_ids, &options.mode, budgets)
            .expect("load local fallback edges");
    let fallback_evidence = super::build_test_impact_fallback_evidence(
        &connection,
        &options.mode,
        &seed_entities,
        &local_edges,
        &[],
        budgets,
    )
    .expect("build fallback evidence");
    assert!(
        fallback_evidence.iter().any(|evidence| {
            evidence.symbol == "greet_works" && evidence.evidence_role == EvidenceRole::Test
        }),
        "{fallback_evidence:?}"
    );
    assert!(
        fallback_evidence.iter().any(|evidence| {
            evidence.symbol == "greet" && evidence.evidence_role == EvidenceRole::Production
        }),
        "{fallback_evidence:?}"
    );

    let snippets =
        super::load_context_fallback_snippets(&repo, &fallback_evidence, budgets.max_snippets)
            .expect("load fallback snippets");
    assert!(
        snippets.iter().any(|snippet| {
            snippet.text.contains("greet_works")
                && snippet.text.contains("#[test]")
                && snippet.text.contains("assert_eq!")
        }),
        "{snippets:?}"
    );
    let fallback_evidence_count = fallback_evidence.len();
    let packet = super::build_context_packet_from_stored_evidence(
        &options,
        &raw_seed_values,
        &seed_ids,
        &seed_entities,
        Vec::new(),
        snippets,
        fallback_evidence,
        None,
        budgets,
        0,
        fallback_evidence_count,
    );
    let response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &json!({"claimable": true, "diagnostic_only": false, "decision": "read_reuse"}),
        budgets,
        &repo,
        &default_db_path(&repo),
        json!({"wall_ms": 1.0}),
    );

    assert_eq!(response["proof_path_available"].as_bool(), Some(false));
    assert!(response["paths"].as_array().expect("paths").is_empty());
    assert!(response["result_count"].as_u64().unwrap_or_default() > 0);
    assert!(response["fallback_evidence"]
        .as_array()
        .expect("fallback evidence")
        .iter()
        .any(|evidence| {
            evidence["symbol"].as_str() == Some("greet_works")
                && evidence["evidence_role"].as_str() == Some("test")
                && evidence["proof_path_available"].as_bool() == Some(false)
        }));
    assert!(response["recommended_tests"]
        .as_array()
        .expect("recommended tests")
        .iter()
        .any(|test| test.as_str() == Some("cargo test greet_works")));

    drop(connection);
    remove_dir_all_with_retry(&repo, "cleanup");
}

#[test]
fn test_impact_recommendations_accept_lowercase_function_kind() {
    let fallback_evidence = vec![super::ContextPackFallbackEvidence {
        id: "entity://test".to_string(),
        symbol: "greet_works".to_string(),
        kind: "function".to_string(),
        source_span: SourceSpan::new("src/lib.rs", 10, 12),
        score: None,
        evidence_role: EvidenceRole::Test,
        evidence_role_label: None,
        proof_status: Some("no_proof_path_found".to_string()),
        graph_proof: false,
        claimability: None,
        seed_matches: Vec::new(),
        follow_up_queries: Vec::new(),
        classification_reason: "unit test fallback".to_string(),
        classification_source: "unit-test".to_string(),
        fallback_source: "source_role".to_string(),
    }];

    assert_eq!(
        super::recommended_tests_from_fallback_evidence(&fallback_evidence),
        vec!["cargo test greet_works"]
    );
}

#[test]
fn context_pack_test_impact_fallback_finds_inline_test_for_production_seed() {
    let repo = rust_inline_context_pack_fixture();
    index_repo(&repo).expect("index inline Rust fixture");
    let connection = Connection::open(default_db_path(&repo)).expect("open fixture db");
    let mut options = context_agent_test_options("test-impact", Some(4), Some(4), None);
    options.task = "greet".to_string();
    options.seeds = vec!["greet".to_string()];
    let budgets = super::ContextPackBudgets::for_options(&options);
    let raw_seed_values = super::context_pack_seed_values(&options, budgets.max_seed_entities);
    let seed_entities = super::resolve_context_seed_entities(
        &connection,
        &raw_seed_values,
        budgets.max_seed_entities,
    )
    .expect("resolve greet seed");
    assert!(
        seed_entities.iter().any(|entity| entity.name == "greet"),
        "{seed_entities:?}"
    );
    let seed_ids =
        super::context_seed_ids(&raw_seed_values, &seed_entities, budgets.max_seed_entities);
    let local_edges =
        super::load_bounded_context_edges(&connection, &seed_ids, &options.mode, budgets)
            .expect("load local fallback edges");
    let fallback_evidence = super::build_test_impact_fallback_evidence(
        &connection,
        &options.mode,
        &seed_entities,
        &local_edges,
        &[],
        budgets,
    )
    .expect("build fallback evidence");

    assert!(
        fallback_evidence.iter().any(|evidence| {
            evidence.symbol == "greet_works" && evidence.evidence_role == EvidenceRole::Test
        }),
        "{fallback_evidence:?}"
    );
    assert!(
        fallback_evidence.iter().any(|evidence| {
            evidence.symbol == "greet" && evidence.evidence_role == EvidenceRole::Production
        }),
        "{fallback_evidence:?}"
    );
    let snippets =
        super::load_context_fallback_snippets(&repo, &fallback_evidence, budgets.max_snippets)
            .expect("load fallback snippets");
    assert!(
        snippets
            .iter()
            .any(|snippet| snippet.text.contains("greet_works")),
        "{snippets:?}"
    );

    let production_fallback = super::build_test_impact_fallback_evidence(
        &connection,
        "production",
        &seed_entities,
        &local_edges,
        &[],
        budgets,
    )
    .expect("production mode fallback");
    assert!(
        production_fallback.is_empty(),
        "production mode must not add test-impact fallback evidence"
    );

    drop(connection);
    remove_dir_all_with_retry(&repo, "cleanup");
}

struct CallerCalleePrecisionFixture {
    repo: PathBuf,
    alpha_target_id: String,
    beta_target_id: String,
    unique_target_id: String,
    alpha_caller_id: String,
    beta_caller_id: String,
    unique_caller_id: String,
}

#[test]
fn context_pack_nuance_rescue_tokens_preserve_exact_identifiers() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task = "Find processUserToken, not processUserTokens or processAdminToken".to_string();
    options.seeds = vec!["processUserToken".to_string()];
    let (tokens, ignored) = super::context_pack_nuance_rescue_tokens(&options, &options.seeds);
    assert!(tokens.iter().any(|token| {
        token.normalized == "processusertoken"
            && token.kind == "identifier_signature"
            && token.exact_match
    }));
    assert!(!tokens
        .iter()
        .any(|token| token.normalized == "processusertokens"));
    assert!(ignored.iter().any(|token| token == "process"));
    assert!(ignored.iter().any(|token| token == "tokens"));
}

#[test]
fn context_pack_nuance_rescue_distinguishes_near_duplicate_identifiers() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task = "Find processUserToken".to_string();
    options.seeds = vec!["processUserToken".to_string()];
    let (tokens, _) = super::context_pack_nuance_rescue_tokens(&options, &options.seeds);
    let token = tokens
        .iter()
        .find(|token| token.normalized == "processusertoken")
        .expect("identifier token");
    assert!(super::context_pack_nuance_haystack_matches(
        "export function processUserToken() {}",
        token
    ));
    assert!(!super::context_pack_nuance_haystack_matches(
        "export function processUserTokens() {}",
        token
    ));
    assert!(!super::context_pack_nuance_haystack_matches(
        "export function processAdminToken() {}",
        token
    ));
}

#[test]
fn context_pack_nuance_path_hits_ignore_deleted_path_dict_entries() {
    let connection = Connection::open_in_memory().expect("open memory db");
    connection
        .execute_batch(
            "
                CREATE TABLE path_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE files (path_id INTEGER NOT NULL);
                INSERT INTO path_dict (id, value) VALUES
                    (1, 'support/scripts/nuance-audit'),
                    (2, 'support/scripts/nuance-audit-deleted');
                INSERT INTO files (path_id) VALUES (1);
                ",
        )
        .expect("create minimal path schema");
    let token = super::ContextPackNuanceToken {
        value: "nuance-audit".to_string(),
        normalized: "nuance-audit".to_string(),
        raw_lower: "nuance-audit".to_string(),
        kind: "path_title",
        rescue_reason: "path_title_rescue",
        exact_match: true,
        seed_value: Some("nuance-audit".to_string()),
    };

    let paths =
        super::load_context_pack_nuance_path_hits(&connection, &token, 10).expect("load path hits");

    assert_eq!(paths, vec!["support/scripts/nuance-audit".to_string()]);
}

#[test]
fn seed_hygiene_filters_prose_tokens() {
    let connection = Connection::open_in_memory().expect("open memory db");
    connection
        .execute_batch(
            "
                CREATE TABLE object_id_lookup (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE symbol_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE qualified_name_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE path_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                INSERT INTO symbol_dict (id, value) VALUES (1, 'resolve_agent_use_profile');
                ",
        )
        .expect("create minimal dictionary schema");
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task = "Write Markdown docs for agent-use around resolve_agent_use_profile".to_string();
    options.seeds.clear();

    let seeds =
        super::context_pack_seed_values_for_connection(&connection, &options, 16).expect("seeds");
    assert!(seeds.iter().any(|seed| seed == "resolve_agent_use_profile"));
    assert!(!seeds.iter().any(|seed| seed == "Markdown"));
    assert!(!seeds.iter().any(|seed| seed == "agent-use"));

    let candidate_seeds = super::context_agent_candidate_seed_values(&options);
    assert!(candidate_seeds
        .iter()
        .any(|seed| seed == "resolve_agent_use_profile"));
    assert!(!candidate_seeds.iter().any(|seed| seed == "Markdown"));
    assert!(!candidate_seeds.iter().any(|seed| seed == "agent-use"));

    let hygiene = super::context_pack_prompt_seed_hygiene_json(&connection, &options.task, 16)
        .expect("seed hygiene trace");
    assert!(hygiene["accepted_exact_seeds"]
        .as_array()
        .expect("accepted exact seeds")
        .iter()
        .any(|item| item["seed"].as_str() == Some("resolve_agent_use_profile")));
    assert!(hygiene["ignored_prose_terms"]
        .as_array()
        .expect("ignored prose terms")
        .iter()
        .any(|item| item.as_str() == Some("Markdown")));
}

#[test]
fn nonmatching_prose_terms_do_not_become_symbol_seeds() {
    let connection = Connection::open_in_memory().expect("open memory db");
    connection
        .execute_batch(
            "
                CREATE TABLE object_id_lookup (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE symbol_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE qualified_name_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE path_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                ",
        )
        .expect("create minimal dictionary schema");
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task = "Build Markdown package docs from file code".to_string();
    options.seeds.clear();

    let seeds =
        super::context_pack_seed_values_for_connection(&connection, &options, 16).expect("seeds");
    assert!(seeds.is_empty(), "{seeds:?}");
    assert!(super::context_agent_candidate_seed_values(&options).is_empty());
}

#[test]
fn quoted_explicit_symbol_seed_survives() {
    let connection = Connection::open_in_memory().expect("open memory db");
    connection
        .execute_batch(
            "
                CREATE TABLE object_id_lookup (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE symbol_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE qualified_name_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE path_dict (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                ",
        )
        .expect("create minimal dictionary schema");
    let values = super::context_pack_prompt_exact_seed_values_for_connection(
        &connection,
        "Trace `AuthService.login` callers",
        16,
    )
    .expect("prompt exact seeds");

    assert_eq!(values, vec!["AuthService.login".to_string()]);
}

#[test]
fn context_pack_nuance_rescue_merges_with_exact_seed_without_graph_proof() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task = "Find QUASAR_NETTLE_FUSE".to_string();
    options.seeds = vec!["QUASAR_NETTLE_FUSE".to_string()];
    let mut packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols: Vec::new(),
        verified_paths: Vec::new(),
        risks: Vec::new(),
        recommended_tests: Vec::new(),
        snippets: Vec::new(),
        metadata: Metadata::new(),
    };
    packet.metadata.insert(
            "nuance_rescue_candidates".to_string(),
            json!([{
                "candidate_id": "nuance-rescue://entity/src/rare_identifier.ts#QUASAR_NETTLE_FUSE",
                "candidate_source": "nuance_rescue",
                "candidate_sources": ["nuance_rescue", "symbol_lookup"],
                "candidate_source_label": "config_key_rescue",
                "file_id": "src/rare_identifier.ts",
                "path": "src/rare_identifier.ts",
                "entity_id": "src/rare_identifier.ts#QUASAR_NETTLE_FUSE",
                "span": {
                    "file": "src/rare_identifier.ts",
                    "start_line": 1,
                    "end_line": 1
                },
                "evidence_role": "production",
                "proof_status": "candidate_only",
                "graph_proof": false,
                "claimable": false,
                "claimable_for_text": false,
                "claimable_for_graph": false,
                "diagnostic_only": false,
                "score": Value::Null,
                "source_score": 9.0,
                "rank": Value::Null,
                "matched_seeds": ["QUASAR_NETTLE_FUSE"],
                "matched_token": "QUASAR_NETTLE_FUSE",
                "matched_tokens": ["QUASAR_NETTLE_FUSE"],
                "rescue_reason": "config_key_rescue",
                "rescue_kinds": ["config_key"],
                "requires_graph_verification": true,
                "verification_status": "needs_graph_verification",
                "reason": "config_key_rescue matched QUASAR_NETTLE_FUSE; candidate-only until graph/source verification",
                "omitted": false,
                "truncated": false,
                "ranking_features": {
                    "exact_seed_match": true,
                    "file_path_match": false,
                    "text_evidence_match": false,
                    "symbol_entity_match": true,
                    "graph_proximity": false,
                    "source_role_compatible": true,
                    "lifecycle_claimable": false,
                    "proof_available": false
                },
                "source_labels": ["nuance_rescue", "config_key_rescue", "candidate_only", "no_graph_proof"]
            }]),
        );
    packet.metadata.insert(
        "nuance_rescue_trace".to_string(),
        json!({
            "schema_version": 1,
            "diagnostic_only": true,
            "rescue_enabled": true,
            "rescue_status": "ready",
            "rescue_candidate_count": 1,
            "candidate_cap": super::CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT,
            "ignored_generic_tokens": [],
            "rescue_reasons": []
        }),
    );

    let candidate_set = super::build_context_agent_retrieval_candidates(
        &options,
        &packet,
        &[],
        &[],
        &[],
        false,
        false,
    );
    let candidate = candidate_set
        .candidates
        .iter()
        .find(|candidate| {
            candidate["matched_seeds"].as_array().is_some_and(|seeds| {
                seeds
                    .iter()
                    .any(|seed| seed.as_str() == Some("QUASAR_NETTLE_FUSE"))
            })
        })
        .expect("merged exact seed candidate");
    let sources = candidate["candidate_sources"]
        .as_array()
        .expect("candidate sources");
    assert!(sources
        .iter()
        .any(|source| source.as_str() == Some("exact_seed")));
    assert!(sources
        .iter()
        .any(|source| source.as_str() == Some("nuance_rescue")));
    assert_eq!(candidate["path"].as_str(), Some("src/rare_identifier.ts"));
    assert_eq!(candidate["graph_proof"].as_bool(), Some(false));
    assert_eq!(candidate["claimable_for_graph"].as_bool(), Some(false));
    assert_eq!(
        candidate["rescue_reason"].as_str(),
        Some("config_key_rescue")
    );
}

#[test]
fn context_pack_union_merges_exact_vector_binary_and_rescue_sources() {
    let mut options = context_agent_test_options("production", Some(1), Some(1), None);
    options.task = "Find QUASAR_NETTLE_FUSE".to_string();
    options.seeds = vec!["QUASAR_NETTLE_FUSE".to_string()];
    let mut packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols: Vec::new(),
        verified_paths: Vec::new(),
        risks: Vec::new(),
        recommended_tests: Vec::new(),
        snippets: Vec::new(),
        metadata: Metadata::new(),
    };
    packet.metadata.insert(
        "vector_semantic_candidates".to_string(),
        json!([{
            "candidate_id": "vector://entity/QUASAR_NETTLE_FUSE",
            "candidate_source": "vector_semantic",
            "candidate_sources": ["vector_semantic", "symbol_lookup"],
            "path": "src/rare_identifier.ts",
            "entity_id": "src/rare_identifier.ts#QUASAR_NETTLE_FUSE",
            "evidence_role": "production",
            "proof_status": "candidate_only",
            "graph_proof": false,
            "claimable": false,
            "claimable_for_text": false,
            "claimable_for_graph": false,
            "requires_graph_verification": true,
            "verification_status": "needs_graph_verification",
            "graph_verification_status": "needs_graph_verification",
            "text_evidence_status": "absent",
            "vector_score": 0.91,
            "source_score": 0.91,
            "matched_seeds": ["QUASAR_NETTLE_FUSE"],
            "matched_tokens": ["QUASAR_NETTLE_FUSE"],
            "reason": "deterministic token-projection candidate; not graph proof",
            "ranking_features": {
                "exact_seed_match": false,
                "file_path_match": false,
                "text_evidence_match": false,
                "symbol_entity_match": true,
                "graph_proximity": false,
                "source_role_compatible": true,
                "lifecycle_claimable": true,
                "proof_available": false,
                "vector_score_available": true,
                "binary_score_available": false,
                "rescue_match": false
            },
            "source_labels": ["vector_semantic", "candidate_only", "no_graph_proof"]
        }]),
    );
    packet.metadata.insert(
        "binary_vector_candidates".to_string(),
        json!([{
            "candidate_id": "binary://entity/QUASAR_NETTLE_FUSE",
            "candidate_source": "binary_vector",
            "candidate_sources": ["binary_vector"],
            "path": "src/rare_identifier.ts",
            "entity_id": "src/rare_identifier.ts#QUASAR_NETTLE_FUSE",
            "evidence_role": "production",
            "proof_status": "candidate_only",
            "graph_proof": false,
            "claimable": false,
            "claimable_for_text": false,
            "claimable_for_graph": false,
            "requires_graph_verification": true,
            "verification_status": "needs_graph_verification",
            "graph_verification_status": "needs_graph_verification",
            "text_evidence_status": "absent",
            "binary_score": 0.77,
            "source_score": 0.77,
            "matched_seeds": ["QUASAR_NETTLE_FUSE"],
            "matched_tokens": ["QUASAR_NETTLE_FUSE"],
            "reason": "binary vector candidate; not graph proof",
            "ranking_features": {
                "exact_seed_match": false,
                "file_path_match": false,
                "text_evidence_match": false,
                "symbol_entity_match": true,
                "graph_proximity": false,
                "source_role_compatible": true,
                "lifecycle_claimable": true,
                "proof_available": false,
                "vector_score_available": false,
                "binary_score_available": true,
                "rescue_match": false
            },
            "source_labels": ["binary_vector", "candidate_only", "no_graph_proof"]
        }]),
    );
    packet.metadata.insert(
            "nuance_rescue_candidates".to_string(),
            json!([{
                "candidate_id": "nuance-rescue://entity/QUASAR_NETTLE_FUSE",
                "candidate_source": "nuance_rescue",
                "candidate_sources": ["nuance_rescue", "symbol_lookup"],
                "path": "src/rare_identifier.ts",
                "entity_id": "src/rare_identifier.ts#QUASAR_NETTLE_FUSE",
                "evidence_role": "production",
                "proof_status": "candidate_only",
                "graph_proof": false,
                "claimable": false,
                "claimable_for_text": false,
                "claimable_for_graph": false,
                "requires_graph_verification": true,
                "verification_status": "needs_graph_verification",
                "graph_verification_status": "needs_graph_verification",
                "text_evidence_status": "absent",
                "matched_seeds": ["QUASAR_NETTLE_FUSE"],
                "matched_token": "QUASAR_NETTLE_FUSE",
                "matched_tokens": ["QUASAR_NETTLE_FUSE"],
                "rescue_reason": "config_key_rescue",
                "rescue_kinds": ["config_key"],
                "source_score": 9.0,
                "reason": "config_key_rescue matched QUASAR_NETTLE_FUSE; candidate-only until graph/source verification",
                "ranking_features": {
                    "exact_seed_match": true,
                    "file_path_match": false,
                    "text_evidence_match": false,
                    "symbol_entity_match": true,
                    "graph_proximity": false,
                    "source_role_compatible": true,
                    "lifecycle_claimable": true,
                    "proof_available": false,
                    "vector_score_available": false,
                    "binary_score_available": false,
                    "rescue_match": true
                },
                "source_labels": ["nuance_rescue", "config_key_rescue", "candidate_only", "no_graph_proof"]
            }]),
        );
    for index in 0..8 {
        packet
            .metadata
            .entry("vector_semantic_candidates".to_string())
            .and_modify(|value| {
                value.as_array_mut().expect("vector array").push(json!({
                    "candidate_id": format!("vector://noise/{index}"),
                    "candidate_source": "vector_semantic",
                    "candidate_sources": ["vector_semantic"],
                    "path": format!("src/noise_{index}.ts"),
                    "evidence_role": "production",
                    "proof_status": "candidate_only",
                    "graph_proof": false,
                    "claimable": false,
                    "claimable_for_text": false,
                    "claimable_for_graph": false,
                    "requires_graph_verification": true,
                    "verification_status": "needs_graph_verification",
                    "graph_verification_status": "needs_graph_verification",
                    "text_evidence_status": "absent",
                    "vector_score": 0.99,
                    "matched_seeds": [],
                    "reason": "noisy vector candidate",
                    "ranking_features": {
                        "exact_seed_match": false,
                        "file_path_match": false,
                        "text_evidence_match": false,
                        "symbol_entity_match": false,
                        "graph_proximity": false,
                        "source_role_compatible": true,
                        "lifecycle_claimable": true,
                        "proof_available": false,
                        "vector_score_available": true,
                        "binary_score_available": false,
                        "rescue_match": false
                    },
                    "source_labels": ["vector_semantic", "candidate_only", "no_graph_proof"]
                }));
            });
    }

    let candidate_set = super::build_context_agent_retrieval_candidates(
        &options,
        &packet,
        &[],
        &[],
        &[],
        true,
        false,
    );
    let candidate = candidate_set
        .candidates
        .iter()
        .find(|candidate| {
            candidate["matched_seeds"].as_array().is_some_and(|seeds| {
                seeds
                    .iter()
                    .any(|seed| seed.as_str() == Some("QUASAR_NETTLE_FUSE"))
            })
        })
        .expect("merged exact/vector/binary/rescue candidate");
    let sources = candidate["candidate_sources"]
        .as_array()
        .expect("candidate sources")
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    for expected in [
        "exact_seed",
        "vector_semantic",
        "binary_vector",
        "nuance_rescue",
        "symbol_lookup",
    ] {
        assert!(sources.contains(expected), "{sources:?}");
    }
    assert_eq!(candidate["rank"].as_u64(), Some(1));
    assert_eq!(candidate["graph_proof"].as_bool(), Some(false));
    assert_eq!(candidate["vector_score"].as_f64(), Some(0.91));
    assert_eq!(candidate["binary_score"].as_f64(), Some(0.77));
    assert_eq!(
        candidate["semantic_backend"].as_str(),
        Some("deterministic_token_projection")
    );
    assert_eq!(
        candidate["learned_semantic_embeddings"].as_bool(),
        Some(false)
    );
    assert_eq!(
        candidate["semantic_embedding_claim"].as_str(),
        Some("not_learned_semantic_embedding")
    );
    assert_eq!(
        candidate["vector_candidate_claim_boundary"].as_str(),
        Some("vector_evidence_candidate_only; graph_proof_requires_graph_source_verification")
    );
    assert_eq!(
        candidate["rescue_reason"].as_str(),
        Some("config_key_rescue")
    );
    assert_eq!(
        candidate["graph_verification_status"].as_str(),
        Some("needs_graph_verification")
    );
    assert!(candidate_set.candidates.len() <= super::CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT);
}

#[test]
fn context_pack_union_merges_rescue_entity_with_graph_verified_path() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task = "Trace auth guard".to_string();
    options.seeds.clear();
    let mut packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols: Vec::new(),
        verified_paths: Vec::new(),
        risks: Vec::new(),
        recommended_tests: Vec::new(),
        snippets: Vec::new(),
        metadata: Metadata::new(),
    };
    packet.metadata.insert(
            "nuance_rescue_candidates".to_string(),
            json!([{
                "candidate_id": "nuance-rescue://entity/repo://e/auth_guard",
                "candidate_source": "nuance_rescue",
                "candidate_sources": ["nuance_rescue", "symbol_lookup"],
                "path": "src/auth.ts",
                "entity_id": "repo://e/auth_guard",
                "evidence_role": "production",
                "proof_status": "candidate_only",
                "graph_proof": false,
                "claimable": false,
                "claimable_for_text": false,
                "claimable_for_graph": false,
                "requires_graph_verification": true,
                "verification_status": "needs_graph_verification",
                "graph_verification_status": "needs_graph_verification",
                "text_evidence_status": "absent",
                "matched_token": "auth",
                "matched_tokens": ["auth"],
                "rescue_reason": "rare_token_lexical_rescue",
                "rescue_kinds": ["rare_token"],
                "reason": "rare token rescue",
                "ranking_features": {
                    "exact_seed_match": false,
                    "file_path_match": false,
                    "text_evidence_match": false,
                    "symbol_entity_match": true,
                    "graph_proximity": false,
                    "source_role_compatible": true,
                    "lifecycle_claimable": true,
                    "proof_available": false,
                    "vector_score_available": false,
                    "binary_score_available": false,
                    "rescue_match": true
                },
                "source_labels": ["nuance_rescue", "rare_token_lexical_rescue", "candidate_only", "no_graph_proof"]
            }]),
        );
    let graph_path = json!({
        "path_id": "auth-verified-path",
        "source": "repo://e/caller",
        "target": "repo://e/auth_guard",
        "source_spans": [{
            "file": "src/auth.ts",
            "repo_relative_path": "src/auth.ts",
            "start_line": 10,
            "start_column": 1,
            "end_line": 12,
            "end_column": 2
        }],
        "evidence_role": "production",
        "confidence": 1.0,
        "classification_reason": "parser verified path",
        "relations": ["calls"]
    });

    let candidate_set = super::build_context_agent_retrieval_candidates(
        &options,
        &packet,
        &[graph_path],
        &[],
        &[],
        true,
        true,
    );
    let candidate = candidate_set
        .candidates
        .iter()
        .find(|candidate| candidate["entity_id"].as_str() == Some("repo://e/auth_guard"))
        .expect("graph verified rescue entity candidate");
    let sources = candidate["candidate_sources"]
        .as_array()
        .expect("candidate sources")
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    assert!(sources.contains("nuance_rescue"), "{sources:?}");
    assert!(sources.contains("path_evidence"), "{sources:?}");
    assert!(sources.contains("graph_neighbor"), "{sources:?}");
    assert_eq!(candidate["graph_proof"].as_bool(), Some(true));
    assert_eq!(
        candidate["verification_status"].as_str(),
        Some("graph_verified")
    );
    assert_eq!(
        candidate["graph_verification_status"].as_str(),
        Some("graph_verified")
    );
    assert_eq!(candidate["proof_status"].as_str(), Some("proof_path_found"));
    assert_eq!(candidate["claimable_for_graph"].as_bool(), Some(true));
}

#[test]
fn context_pack_exact_seed_candidate_merges_with_verified_path() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task = "Trace auth_guard".to_string();
    options.seeds = vec!["auth_guard".to_string()];
    let packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols: Vec::new(),
        verified_paths: Vec::new(),
        risks: Vec::new(),
        recommended_tests: Vec::new(),
        snippets: Vec::new(),
        metadata: Metadata::new(),
    };
    let graph_path = json!({
        "path_id": "auth-guard-path",
        "source": "repo://e/caller",
        "target": "repo://e/auth_guard",
        "source_spans": [{
            "file": "src/auth.ts",
            "repo_relative_path": "src/auth.ts",
            "start_line": 10,
            "start_column": 1,
            "end_line": 12,
            "end_column": 2
        }],
        "evidence_role": "production",
        "confidence": 1.0,
        "classification_reason": "parser verified path",
        "relations": ["calls"]
    });

    let candidate_set = super::build_context_agent_retrieval_candidates(
        &options,
        &packet,
        &[graph_path],
        &[],
        &[],
        true,
        true,
    );
    let candidate = candidate_set
        .candidates
        .iter()
        .find(|candidate| {
            candidate["candidate_sources"]
                .as_array()
                .is_some_and(|sources| {
                    sources
                        .iter()
                        .any(|source| source.as_str() == Some("exact_seed"))
                        && sources
                            .iter()
                            .any(|source| source.as_str() == Some("path_evidence"))
                })
        })
        .expect("exact seed merged with verified path");
    let sources = candidate["candidate_sources"]
        .as_array()
        .expect("candidate sources")
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    assert!(sources.contains("exact_seed"), "{sources:?}");
    assert!(sources.contains("path_evidence"), "{sources:?}");
    assert!(sources.contains("graph_neighbor"), "{sources:?}");
    assert_eq!(candidate["graph_proof"].as_bool(), Some(true));
    assert_eq!(candidate["proof_status"].as_str(), Some("proof_path_found"));
    assert_eq!(
        candidate["graph_verification_status"].as_str(),
        Some("graph_verified")
    );
    assert_eq!(candidate["claimable_for_graph"].as_bool(), Some(true));
}

#[test]
fn context_pack_vector_candidate_verifies_only_when_graph_path_exists() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task = "Trace auth guard".to_string();
    options.seeds.clear();
    let mut packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols: Vec::new(),
        verified_paths: Vec::new(),
        risks: Vec::new(),
        recommended_tests: Vec::new(),
        snippets: Vec::new(),
        metadata: Metadata::new(),
    };
    packet.metadata.insert(
        "vector_semantic_candidates".to_string(),
        json!([{
            "candidate_id": "vector://entity/repo://e/auth_guard",
            "candidate_source": "vector_semantic",
            "candidate_sources": ["vector_semantic", "symbol_lookup"],
            "path": "src/auth.ts",
            "entity_id": "repo://e/auth_guard",
            "evidence_role": "production",
            "proof_status": "candidate_only",
            "graph_proof": false,
            "claimable": false,
            "claimable_for_text": false,
            "claimable_for_graph": false,
            "requires_graph_verification": true,
            "verification_status": "needs_graph_verification",
            "graph_verification_status": "needs_graph_verification",
            "text_evidence_status": "absent",
            "vector_score": 0.88,
            "source_score": 0.88,
            "matched_seeds": ["auth_guard"],
            "reason": "deterministic token-projection candidate; not graph proof",
            "ranking_features": {
                "exact_seed_match": false,
                "file_path_match": false,
                "text_evidence_match": false,
                "symbol_entity_match": true,
                "graph_proximity": false,
                "source_role_compatible": true,
                "lifecycle_claimable": true,
                "proof_available": false,
                "vector_score_available": true,
                "binary_score_available": false,
                "rescue_match": false
            },
            "source_labels": ["vector_semantic", "candidate_only", "no_graph_proof"]
        }]),
    );
    let graph_path = json!({
        "path_id": "auth-vector-path",
        "source": "repo://e/caller",
        "target": "repo://e/auth_guard",
        "source_spans": [{
            "file": "src/auth.ts",
            "repo_relative_path": "src/auth.ts",
            "start_line": 10,
            "start_column": 1,
            "end_line": 12,
            "end_column": 2
        }],
        "evidence_role": "production",
        "confidence": 1.0,
        "classification_reason": "parser verified path",
        "relations": ["calls"]
    });

    let candidate_set = super::build_context_agent_retrieval_candidates(
        &options,
        &packet,
        &[graph_path],
        &[],
        &[],
        true,
        true,
    );
    let candidate = candidate_set
        .candidates
        .iter()
        .find(|candidate| candidate["entity_id"].as_str() == Some("repo://e/auth_guard"))
        .expect("vector candidate merged with verified graph path");
    let sources = candidate["candidate_sources"]
        .as_array()
        .expect("candidate sources")
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    assert!(sources.contains("vector_semantic"), "{sources:?}");
    assert!(sources.contains("path_evidence"), "{sources:?}");
    assert!(sources.contains("graph_neighbor"), "{sources:?}");
    assert_eq!(candidate["graph_proof"].as_bool(), Some(true));
    assert_eq!(
        candidate["graph_verification_status"].as_str(),
        Some("graph_verified")
    );
    assert_eq!(candidate["claimable_for_graph"].as_bool(), Some(true));
}

#[test]
fn context_pack_binary_candidate_missing_graph_path_returns_no_proof() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task = "Trace missing binary candidate".to_string();
    options.seeds.clear();
    let mut packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols: Vec::new(),
        verified_paths: Vec::new(),
        risks: Vec::new(),
        recommended_tests: Vec::new(),
        snippets: Vec::new(),
        metadata: Metadata::new(),
    };
    packet.metadata.insert(
        "graph_verification_status".to_string(),
        json!("no_proof_path_found"),
    );
    packet.metadata.insert(
        "binary_vector_candidates".to_string(),
        json!([{
            "candidate_id": "binary://entity/repo://e/missing",
            "candidate_source": "binary_vector",
            "candidate_sources": ["binary_vector", "symbol_lookup"],
            "path": "src/missing.ts",
            "entity_id": "repo://e/missing",
            "evidence_role": "production",
            "proof_status": "candidate_only",
            "graph_proof": false,
            "claimable": false,
            "claimable_for_text": false,
            "claimable_for_graph": false,
            "requires_graph_verification": true,
            "verification_status": "needs_graph_verification",
            "graph_verification_status": "needs_graph_verification",
            "text_evidence_status": "absent",
            "binary_score": 0.77,
            "source_score": 0.77,
            "matched_seeds": ["missing"],
            "reason": "binary vector candidate; not graph proof",
            "ranking_features": {
                "exact_seed_match": false,
                "file_path_match": false,
                "text_evidence_match": false,
                "symbol_entity_match": true,
                "graph_proximity": false,
                "source_role_compatible": true,
                "lifecycle_claimable": true,
                "proof_available": false,
                "vector_score_available": false,
                "binary_score_available": true,
                "rescue_match": false
            },
            "source_labels": ["binary_vector", "candidate_only", "no_graph_proof"]
        }]),
    );

    let candidate_set = super::build_context_agent_retrieval_candidates(
        &options,
        &packet,
        &[],
        &[],
        &[],
        true,
        false,
    );
    let candidate = candidate_set
        .candidates
        .iter()
        .find(|candidate| {
            candidate["candidate_sources"]
                .as_array()
                .is_some_and(|sources| {
                    sources
                        .iter()
                        .any(|source| source.as_str() == Some("binary_vector"))
                })
        })
        .expect("binary candidate");
    assert_eq!(candidate["graph_proof"].as_bool(), Some(false));
    assert_eq!(
        candidate["proof_status"].as_str(),
        Some("no_proof_path_found")
    );
    assert_eq!(
        candidate["graph_verification_status"].as_str(),
        Some("no_proof_path_found")
    );
    assert_eq!(candidate["claimable_for_graph"].as_bool(), Some(false));
    assert!(candidate["reason"]
        .as_str()
        .is_some_and(|reason| reason.contains("no proof path")));
}

#[test]
fn context_pack_candidate_overload_reports_omitted_reason() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), Some(512 * 1024));
    options.explain = true;
    options.task = "Find EXACT_SEED with noisy candidates".to_string();
    options.seeds = vec!["EXACT_SEED".to_string()];
    let mut packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols: Vec::new(),
        verified_paths: Vec::new(),
        risks: Vec::new(),
        recommended_tests: Vec::new(),
        snippets: Vec::new(),
        metadata: Metadata::new(),
    };
    packet.metadata.insert(
        "graph_verification_status".to_string(),
        json!("no_proof_path_found"),
    );
    let noisy_candidates = (0..8)
        .map(|index| {
            json!({
                "candidate_id": format!("vector://noise/{index}"),
                "candidate_source": "vector_semantic",
                "candidate_sources": ["vector_semantic"],
                "path": format!("src/noise_{index}.ts"),
                "evidence_role": "production",
                "proof_status": "candidate_only",
                "graph_proof": false,
                "claimable": false,
                "claimable_for_text": false,
                "claimable_for_graph": false,
                "requires_graph_verification": true,
                "verification_status": "needs_graph_verification",
                "graph_verification_status": "needs_graph_verification",
                "text_evidence_status": "absent",
                "vector_score": 0.99,
                "source_score": 0.99,
                "matched_seeds": [],
                "reason": "noisy vector candidate",
                "ranking_features": {
                    "exact_seed_match": false,
                    "file_path_match": false,
                    "text_evidence_match": false,
                    "symbol_entity_match": false,
                    "graph_proximity": false,
                    "source_role_compatible": true,
                    "lifecycle_claimable": true,
                    "proof_available": false,
                    "vector_score_available": true,
                    "binary_score_available": false,
                    "rescue_match": false
                },
                "source_labels": ["vector_semantic", "candidate_only", "no_graph_proof"]
            })
        })
        .collect::<Vec<_>>();
    packet.metadata.insert(
        "vector_semantic_candidates".to_string(),
        json!(noisy_candidates),
    );
    let response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &json!({"claimable": true, "diagnostic_only": false, "decision": "read_reuse"}),
        super::ContextPackBudgets::for_options(&options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    let candidates = response["candidates"].as_array().expect("candidates");
    assert!(candidates.iter().any(|candidate| {
        candidate["candidate_sources"]
            .as_array()
            .is_some_and(|sources| {
                sources
                    .iter()
                    .any(|source| source.as_str() == Some("exact_seed"))
            })
    }));
    assert!(
        response["candidate_omitted_count"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "{response:?}"
    );
    assert_eq!(
        response["candidate_omitted_reason"].as_str(),
        Some("candidate_cap_exceeded_after_exact_seed_text_priority")
    );
    let omitted = &response["retrieval_explain"]["candidates_omitted"];
    assert!(omitted["count"].as_u64().unwrap_or_default() > 0);
    assert_eq!(
        omitted["reason"].as_str(),
        Some("candidate_cap_exceeded_after_exact_seed_text_priority")
    );
    assert!(omitted["items"]
        .as_array()
        .expect("omitted items")
        .iter()
        .all(|item| item["omission_reason"].as_str()
            == Some("candidate_cap_exceeded_after_exact_seed_text_priority")));
}

#[test]
fn context_pack_stale_diagnostic_graph_output_is_non_claimable() {
    let options = context_agent_test_options("production", Some(2), Some(2), None);
    let packet = context_agent_test_packet("production", 1, 1, "production");
    let lifecycle = json!({
        "claimable": false,
        "diagnostic_only": true,
        "decision": "diagnostic_allowed",
        "passport_status": "stale",
    });
    let response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    assert_eq!(response["claimable"].as_bool(), Some(false));
    assert_eq!(response["diagnostic_only"].as_bool(), Some(true));
    let candidates = response["candidates"].as_array().expect("candidates");
    assert!(candidates.iter().all(|candidate| {
        candidate["claimable"].as_bool() == Some(false)
            && candidate["claimable_for_graph"].as_bool() == Some(false)
    }));
}

#[test]
fn context_pack_nuance_text_rescue_reaches_no_proof_fallback_bounded() {
    let mut options = context_agent_test_options("production", Some(1), Some(1), None);
    options.task = "Find auth docs".to_string();
    options.seeds.clear();
    let mut packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols: Vec::new(),
        verified_paths: Vec::new(),
        risks: Vec::new(),
        recommended_tests: Vec::new(),
        snippets: Vec::new(),
        metadata: Metadata::new(),
    };
    packet.metadata.insert(
            "nuance_rescue_candidates".to_string(),
            json!([{
                "candidate_id": "nuance-rescue://text/README.md:21",
                "candidate_source": "nuance_rescue",
                "candidate_sources": ["nuance_rescue", "text_evidence", "lexical_fts"],
                "path": "README.md",
                "file_id": "README.md",
                "entity_id": Value::Null,
                "evidence_role": "text_evidence",
                "proof_status": "not_graph_proof",
                "graph_proof": false,
                "claimable": true,
                "claimable_for_text": true,
                "claimable_for_graph": false,
                "requires_graph_verification": false,
                "verification_status": "not_graph_proof",
                "graph_verification_status": "not_graph_proof",
                "text_evidence_status": "not_graph_proof",
                "matched_token": "auth",
                "matched_tokens": ["auth"],
                "rescue_reason": "rare_token_lexical_rescue",
                "rescue_kinds": ["rare_token"],
                "snippet": "- auth/role/negation",
                "reason": "rare-token text evidence rescue; not graph proof",
                "ranking_features": {
                    "exact_seed_match": false,
                    "file_path_match": false,
                    "text_evidence_match": true,
                    "symbol_entity_match": false,
                    "graph_proximity": false,
                    "source_role_compatible": true,
                    "lifecycle_claimable": true,
                    "proof_available": false,
                    "vector_score_available": false,
                    "binary_score_available": false,
                    "rescue_match": true
                },
                "source_labels": ["nuance_rescue", "rare_token_lexical_rescue", "text_evidence", "candidate_only", "no_graph_proof"]
            }]),
        );
    let lifecycle = json!({
        "claimable": true,
        "diagnostic_only": false,
        "decision": "read_reuse",
        "passport_status": "valid",
    });
    let response = super::context_pack_agent_json_response(
        &options,
        &packet,
        &lifecycle,
        super::ContextPackBudgets::for_options(&options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    );
    let candidates = response["candidates"].as_array().expect("candidates");
    let text_candidate = candidates
        .iter()
        .find(|candidate| candidate["path"].as_str() == Some("README.md"))
        .expect("nuance text candidate reaches context-pack output");
    assert_eq!(text_candidate["graph_proof"].as_bool(), Some(false));
    assert_eq!(
        text_candidate["proof_status"].as_str(),
        Some("not_graph_proof")
    );
    assert_eq!(
        text_candidate["text_evidence_status"].as_str(),
        Some("not_graph_proof")
    );
    assert_eq!(text_candidate["claimable_for_graph"].as_bool(), Some(false));
    assert!(text_candidate["candidate_sources"]
        .as_array()
        .expect("candidate sources")
        .iter()
        .any(|source| source.as_str() == Some("nuance_rescue")));
    assert!(text_candidate["candidate_sources"]
        .as_array()
        .expect("candidate sources")
        .iter()
        .any(|source| source.as_str() == Some("text_evidence")));
    assert!(candidates.len() <= super::CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT);
    assert!(serialized_len_for_test(&response) <= super::DEFAULT_CONTEXT_AGENT_MAX_OUTPUT_BYTES);
}

#[test]
fn context_pack_nuance_rescue_keeps_generic_terms_from_flooding() {
    let mut options = context_agent_test_options("production", Some(3), Some(3), None);
    options.task =
        "Find the function code route literal test token user process package metadata".to_string();
    options.seeds.clear();
    let (tokens, ignored) = super::context_pack_nuance_rescue_tokens(&options, &[]);
    assert!(tokens.is_empty(), "{tokens:?}");
    assert!(ignored.iter().any(|token| token == "function"));
    assert!(ignored.iter().any(|token| token == "package"));
}

#[test]
fn routing_packet_buildroot_role_diverse_edit_plan_and_followups() {
    let response = routing_packet_response_for_task(
        "Trace Buildroot generic package flow for adding a new package.",
        &[
            routing_fixture_evidence(
                "docs/manual/adding-packages-generic.adoc",
                "generic-package package infrastructure authoring docs",
            ),
            routing_fixture_evidence(
                "package/pkg-generic.mk",
                "inner-generic-package VERSION SITE LICENSE DEPENDENCIES",
            ),
            routing_fixture_evidence(
                "package/Config.in",
                "source \"package/foo/Config.in\" BR2_PACKAGE depends on",
            ),
            routing_fixture_evidence("package/pkg-download.mk", "download site hash support"),
            routing_fixture_evidence("support/download/dl-wrapper", "download wrapper backend"),
            routing_fixture_evidence("support/scripts/pkg-stats", "support scripts package"),
            routing_fixture_evidence("package/zlib/zlib.mk", "ZLIB_VERSION generic-package"),
        ],
        &["generic-package", "BR2_PACKAGE_FOO"],
        None,
        Some(262_144),
    );
    let routing = &response["routing_packet"];
    assert_eq!(
        routing["task_intent"]["task_kind"].as_str(),
        Some("build_system_package_authoring")
    );
    let files = routing_files(routing);
    for expected in [
        "docs/manual/adding-packages-generic.adoc",
        "package/pkg-generic.mk",
        "package/Config.in",
        "package/pkg-download.mk",
        "support/download/dl-wrapper",
    ] {
        assert!(files.iter().any(|file| file == expected), "{files:?}");
    }
    assert!(routing["text_evidence"]
        .as_array()
        .expect("text evidence")
        .iter()
        .all(|evidence| evidence["graph_proof"].as_bool() == Some(false)));
    assert!(!routing["fallback_snippets"]
        .as_array()
        .expect("fallback snippets")
        .is_empty());
    assert!(routing["follow_up_queries"]
        .as_array()
        .expect("follow up")
        .iter()
        .all(|query| query.get("query_text").is_some()
            && query.get("path_scope").is_some()
            && query.get("why").is_some()
            && query.get("max_results_hint").is_some()));
    assert!(routing.get("suggested_rg_probes").is_none());
    assert!(routing["edit_plan"]
        .as_array()
        .expect("edit plan")
        .iter()
        .any(|step| step["step"]
            .as_str()
            .is_some_and(|text| text.contains("Config.in"))));
    assert!(!routing["validation_steps"]
        .as_array()
        .expect("validation")
        .is_empty());
}

#[test]
fn routing_packet_codegraph_lifecycle_debug_surfaces_lifecycle_store_and_tests() {
    let response = routing_packet_response_for_task(
        "Trace indexing entry point and DB lifecycle guards.",
        &[
            routing_fixture_evidence(
                "crates/codegraph-cli/src/lib.rs",
                "context-pack index entrypoint symbols",
            ),
            routing_fixture_evidence(
                "crates/codegraph-index/src/lib.rs",
                "inspect_db_lifecycle_preflight passport stale DB",
            ),
            routing_fixture_evidence(
                "crates/codegraph-store/src/lib.rs",
                "SqliteGraphStore open_read_only lifecycle",
            ),
            routing_fixture_evidence(
                "crates/codegraph-cli/src/lib.rs",
                "#[test] doctor status lifecycle tests",
            ),
        ],
        &["inspect_db_lifecycle_preflight", "SqliteGraphStore"],
        None,
        Some(65_536),
    );
    let routing = &response["routing_packet"];
    assert_eq!(
        routing["task_intent"]["task_kind"].as_str(),
        Some("codegraph_internal_debug")
    );
    let roles = routing_roles(routing, "critical_files");
    assert!(
        roles.iter().any(|role| role == "lifecycle_preflight"),
        "{roles:?}"
    );
    assert!(roles.iter().any(|role| role == "store_open"), "{roles:?}");
    assert!(routing["critical_symbols"]
        .as_array()
        .expect("symbols")
        .iter()
        .any(|symbol| symbol["symbol"].as_str() == Some("SqliteGraphStore")));
    assert!(routing["unknowns"]
        .as_array()
        .expect("unknowns")
        .iter()
        .any(|unknown| unknown["reason"].as_str() == Some("no_proof_path_found")));
}

#[test]
fn routing_packet_implementation_trace_keeps_source_navigation_and_inspection_requirements() {
    let response = routing_packet_response_for_task(
        "Trace vector chunk index build accounting and persisted vector index size math.",
        &[
            routing_fixture_evidence(
                "crates/codegraph-index/src/lib.rs",
                "build_vector_chunk_index_json_for_repo generated selected persisted counts",
            ),
            routing_fixture_evidence(
                "crates/codegraph-index/src/lib.rs",
                "write_vector_chunk_index_json artifact writer",
            ),
            routing_fixture_evidence(
                "crates/codegraph-cli/src/lib.rs",
                "#[test] vector chunk index accounting tests",
            ),
        ],
        &["build_vector_chunk_index_json_for_repo"],
        None,
        Some(65_536),
    );
    let routing = &response["routing_packet"];
    assert!(matches!(
        routing["task_intent"]["task_kind"].as_str(),
        Some("storage_accounting_trace" | "artifact_math_trace" | "implementation_trace")
    ));
    assert!(!routing["source_navigation_evidence"]
        .as_array()
        .expect("source nav")
        .is_empty());
    assert!(routing["source_navigation_evidence"]
        .as_array()
        .expect("source nav")
        .iter()
        .all(|evidence| evidence["graph_proof"].as_bool() == Some(false)));
    assert!(!routing["artifact_inspection_requirements"]
        .as_array()
        .expect("artifact requirements")
        .is_empty());
    assert!(routing["artifact_inspection_requirements"]
        .as_array()
        .expect("artifact requirements")
        .iter()
        .any(
            |requirement| requirement["fields"].as_array().is_some_and(|fields| fields
                .iter()
                .any(|field| field.as_str() == Some("runtime_sidecar_bytes"))
                && fields
                    .iter()
                    .any(|field| field.as_str() == Some("audit_artifact_bytes")))
        ));
    assert!(!routing["db_inspection_requirements"]
        .as_array()
        .expect("db requirements")
        .is_empty());
    assert_eq!(
        routing["budget_status"]["source_navigation_evidence_survives_for_implementation_trace"]
            .as_bool(),
        Some(true)
    );
    assert!(routing["formulas_or_accounting_notes"]
        .as_array()
        .expect("formula notes")
        .iter()
        .any(|note| note.get("actual_index_file_bytes").is_some()
            || note.get("estimated_f32_payload_bytes").is_some()));
    assert_eq!(
        routing["deterministic_summary"]["proof"].as_str(),
        Some("I found no verified graph proof path. This packet uses source text evidence only.")
    );
}

#[test]
fn routing_packet_vector_metric_accounting_truthful_labels() {
    let response = routing_packet_response_for_task(
            "Trace actual_index_file_bytes estimated_f32_payload_bytes vector index accounting.",
            &[routing_fixture_evidence(
                "crates/codegraph-index/src/lib.rs",
                "actual_index_file_bytes estimated_f32_payload_bytes pretty_json vector_payload_compression diversity_ranked_v1 input_order_cap",
            )],
            &["actual_index_file_bytes", "estimated_f32_payload_bytes"],
            Some(routing_vector_metric_trace()),
            Some(65_536),
        );
    let note = response["routing_packet"]["formulas_or_accounting_notes"]
        .as_array()
        .expect("formula notes")
        .iter()
        .find(|note| note["note_id"].as_str() == Some("vector_metric_truthfulness"))
        .cloned()
        .expect("vector metric note");
    assert_eq!(note["actual_index_file_bytes"].as_u64(), Some(1234));
    assert_eq!(note["artifact_kind"].as_str(), Some("unknown"));
    assert_eq!(note["runtime_sidecar_bytes"].as_str(), Some("unknown"));
    assert_eq!(note["audit_artifact_bytes"].as_str(), Some("unknown"));
    assert_eq!(note["estimated_f32_payload_bytes"].as_u64(), Some(256));
    assert_eq!(note["index_artifact_format"].as_str(), Some("pretty_json"));
    assert_eq!(note["vector_payload_compression"].as_str(), Some("none"));
    assert_eq!(
        note["chunk_selection_strategy"].as_str(),
        Some("diversity_ranked_v1")
    );
    assert_eq!(note["input_order_cap"].as_bool(), Some(false));
    assert!(note["sentence"]
        .as_str()
        .expect("sentence")
        .contains("not a compressed-vector storage claim"));
}

#[test]
fn routing_packet_test_impact_keeps_production_and_test_labels() {
    let response = routing_packet_response_for_task(
        "Find test impact for changing a helper used by auth tests.",
        &[
            routing_fixture_evidence("src/auth/helper.rs", "production helper auth behavior"),
            routing_fixture_evidence(
                "tests/auth_helper_test.rs",
                "#[test] mock assertion fixture auth tests",
            ),
        ],
        &["auth_helper"],
        None,
        Some(65_536),
    );
    let routing = &response["routing_packet"];
    assert_eq!(
        routing["task_intent"]["task_kind"].as_str(),
        Some("test_impact")
    );
    let roles = routing_roles(routing, "source_navigation_evidence");
    assert!(
        roles.iter().any(|role| role == "production_target"),
        "{roles:?}"
    );
    assert!(roles.iter().any(|role| role == "test_files"), "{roles:?}");
    assert!(routing["validation_steps"]
        .as_array()
        .expect("validation")
        .iter()
        .any(|step| step["scope"].as_str() == Some("test impact")));
}

#[test]
fn routing_packet_dataflow_roles_do_not_overclaim_without_path() {
    let response = routing_packet_response_for_task(
        "Trace request input flow to a database write.",
        &[
            routing_fixture_evidence("src/http.rs", "request input source handler"),
            routing_fixture_evidence("src/validate.rs", "sanitize validate request"),
            routing_fixture_evidence("src/db.rs", "database write sink insert mutation"),
        ],
        &["request", "database_write"],
        None,
        Some(65_536),
    );
    let routing = &response["routing_packet"];
    assert_eq!(
        routing["task_intent"]["task_kind"].as_str(),
        Some("dataflow_trace")
    );
    let roles = routing_roles(routing, "source_navigation_evidence");
    assert!(roles.iter().any(|role| role == "source"), "{roles:?}");
    assert!(roles.iter().any(|role| role == "sanitizer"), "{roles:?}");
    assert!(
        roles
            .iter()
            .any(|role| role == "sink" || role == "mutation_write"),
        "{roles:?}"
    );
    assert!(routing["verified_paths"]
        .as_array()
        .expect("verified paths")
        .is_empty());
    assert!(routing["unknowns"]
        .as_array()
        .expect("unknowns")
        .iter()
        .any(|unknown| unknown["claim"].as_str() == Some("graph relation proof")));
}

#[test]
fn routing_packet_security_review_warns_strings_are_not_proof() {
    let response = routing_packet_response_for_task(
        "Find where role checks authorize admin-only behavior.",
        &[
            routing_fixture_evidence("src/auth.rs", "auth route entrypoint span authorize"),
            routing_fixture_evidence("src/routes/admin.rs", "admin role checkRole"),
            routing_fixture_evidence("src/authz.rs", "permission gate RBAC sanitizer validator"),
            routing_fixture_evidence("tests/admin_auth_test.rs", "#[test] admin role assertion"),
        ],
        &["checkRole", "admin"],
        None,
        Some(65_536),
    );
    let routing = &response["routing_packet"];
    assert_eq!(
        routing["task_intent"]["task_kind"].as_str(),
        Some("security_review")
    );
    let roles = routing_roles(routing, "source_navigation_evidence");
    assert!(
        roles.iter().any(|role| role == "auth_entrypoint"),
        "{roles:?}"
    );
    assert!(
        roles
            .iter()
            .any(|role| role == "permission_gate" || role == "role_check"),
        "{roles:?}"
    );
    assert!(routing["risks"]
        .as_array()
        .expect("risks")
        .iter()
        .any(|risk| risk["risk_id"].as_str() == Some("strings_comments_not_authorization_proof")));
}

#[test]
fn routing_packet_docs_lookup_keeps_docs_text_as_non_graph_proof() {
    let response = routing_packet_response_for_task(
        "Find docs explaining package infrastructure.",
        &[routing_fixture_evidence(
            "docs/manual/adding-packages-generic.adoc",
            "docs guide package infrastructure generic-package",
        )],
        &["generic-package"],
        None,
        Some(65_536),
    );
    let routing = &response["routing_packet"];
    assert_eq!(
        routing["task_intent"]["task_kind"].as_str(),
        Some("docs_lookup")
    );
    assert!(routing["text_evidence"]
        .as_array()
        .expect("text evidence")
        .iter()
        .all(|evidence| evidence["graph_proof"].as_bool() == Some(false)));
    assert!(routing["verified_paths"]
        .as_array()
        .expect("verified paths")
        .is_empty());
}

#[test]
fn routing_packet_unknown_task_is_conservative() {
    let response = routing_packet_response_for_task(
        "Figure out what handles this thing.",
        &[],
        &[],
        None,
        Some(65_536),
    );
    let routing = &response["routing_packet"];
    assert_eq!(
        routing["task_intent"]["task_kind"].as_str(),
        Some("unknown")
    );
    assert_eq!(
        routing["claimability"]["graph_proof"].as_bool(),
        Some(false)
    );
    assert!(routing["unknowns"]
        .as_array()
        .expect("unknowns")
        .iter()
        .any(|unknown| unknown["reason"].as_str() == Some("unknown_or_ambiguous_task")));
    assert!(routing["follow_up_queries"]
        .as_array()
        .expect("follow up")
        .iter()
        .all(|query| query["risk"].as_str() == Some("candidate_only_until_verified")));
}

#[test]
fn patch_assist_packet_has_required_sections_and_non_shell_followups() {
    let response = routing_packet_response_for_task(
        "Trace Buildroot generic package flow for adding a new package.",
        &[
            routing_fixture_evidence(
                "docs/manual/adding-packages-generic.adoc",
                "generic-package package infrastructure authoring docs",
            ),
            routing_fixture_evidence(
                "package/pkg-generic.mk",
                "inner-generic-package VERSION SITE LICENSE DEPENDENCIES",
            ),
            routing_fixture_evidence("package/Config.in", "BR2_PACKAGE depends on"),
        ],
        &["generic-package", "BR2_PACKAGE_FOO"],
        None,
        Some(65_536),
    );
    let patch = &response["patch_assist_packet"];
    assert_eq!(
        patch["packet_kind"].as_str(),
        Some("patch_assist_staged_context")
    );
    for key in [
        "task_intent",
        "task_roles",
        "critical_files",
        "critical_symbols",
        "source_navigation_evidence",
        "text_evidence",
        "candidate_evidence",
        "graph_proof_paths",
        "proof_status",
        "proof_strength",
        "graph_proof",
        "unknowns",
        "risks",
        "validation_steps",
        "follow_up_queries",
        "expansion_handles",
        "artifact_or_db_inspection_requirements",
        "degradation_warnings",
        "staged_availability",
    ] {
        assert!(patch.get(key).is_some(), "missing {key}: {patch}");
    }
    assert_eq!(patch["graph_proof"].as_bool(), Some(false));
    assert!(patch["graph_proof_paths"]
        .as_array()
        .expect("graph proof paths")
        .is_empty());
    assert!(patch["follow_up_queries"]
        .as_array()
        .expect("followups")
        .iter()
        .all(|query| query["shell_ready"].as_bool() == Some(false)
            && !query["query_text"]
                .as_str()
                .unwrap_or_default()
                .contains("rg ")));
    assert_eq!(
        patch["packet_budget_status"]["packet_budget_enforced"].as_bool(),
        Some(true)
    );
    assert!(
        patch["packet_budget_status"]["serialized_bytes"]
            .as_u64()
            .unwrap_or_default()
            <= patch["packet_budget_status"]["packet_budget_bytes"]
                .as_u64()
                .unwrap_or_default(),
        "{patch}"
    );
}

#[test]
fn patch_assist_packet_surfaces_degraded_output_without_graph_overclaim() {
    let mut degraded = routing_fixture_evidence(
        "src/generated/large.generated.ts",
        "generated high fanout implementation trace",
    );
    let object = degraded.as_object_mut().expect("degraded evidence object");
    object.insert("graph_output_degraded".to_string(), json!(true));
    object.insert(
        "degradation_labels".to_string(),
        json!([
            "generated_large",
            "extraction_budget_hit",
            "diagnostic_only"
        ]),
    );
    object.insert(
        "graph_extraction_skip_reason".to_string(),
        json!("large_generated_or_test_source_budget"),
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
            ]
        }),
    );

    let response = routing_packet_response_for_task(
        "Trace implementation path for generated large file handling.",
        &[degraded],
        &["large_generated_handler"],
        None,
        Some(65_536),
    );
    let patch = &response["patch_assist_packet"];
    assert_eq!(patch["graph_proof"].as_bool(), Some(false));
    assert!(
        patch["degradation_warnings"]
            .as_array()
            .expect("degradation warnings")
            .iter()
            .any(|warning| warning["reason"].as_str()
                == Some("large_generated_or_test_source_budget"))
    );
    assert!(patch["critical_files"]
        .as_array()
        .expect("critical files")
        .iter()
        .any(
            |file| file["degraded_output_not_complete_graph_proof"].as_bool() == Some(true)
                || file["graph_output_degraded"].as_bool() == Some(true)
        ));
}

#[test]
fn routing_packet_budget_pressure_dedupes_examples_and_keeps_survivors() {
    let mut evidence = vec![
        routing_fixture_evidence(
            "docs/manual/adding-packages-generic.adoc",
            "generic-package package infrastructure",
        ),
        routing_fixture_evidence("package/pkg-generic.mk", "inner-generic-package"),
        routing_fixture_evidence("package/Config.in", "BR2_PACKAGE source package"),
        routing_fixture_evidence("package/pkg-download.mk", "download hash"),
    ];
    for index in 0..24 {
        evidence.push(routing_fixture_evidence(
            &format!("package/example{index}/example{index}.mk"),
            "EXAMPLE_VERSION generic-package",
        ));
    }
    let response = routing_packet_response_for_task(
        "Trace Buildroot generic package flow for adding a new package.",
        &evidence,
        &["generic-package"],
        None,
        None,
    );
    assert!(
        serialized_len_for_test(&response) <= super::DEFAULT_CONTEXT_AGENT_MAX_OUTPUT_BYTES,
        "{} bytes",
        serialized_len_for_test(&response)
    );
    let routing = &response["routing_packet"];
    assert!(!routing["fallback_snippets"]
        .as_array()
        .expect("fallback snippets")
        .is_empty());
    let example_count = routing["critical_files"]
        .as_array()
        .expect("critical files")
        .iter()
        .filter(|file| file["role"].as_str() == Some("examples"))
        .count();
    assert!(example_count <= 2, "{routing}");
    assert!(routing["omitted_count"].as_u64().unwrap_or_default() > 0);
}

#[test]
fn routing_packet_deterministic_sentence_snapshot_is_stable() {
    let first = routing_packet_response_for_task(
        "Trace request input flow to a database write.",
        &[routing_fixture_evidence(
            "src/db.rs",
            "request input database write",
        )],
        &["database_write"],
        None,
        Some(65_536),
    );
    let second = routing_packet_response_for_task(
        "Trace request input flow to a database write.",
        &[routing_fixture_evidence(
            "src/db.rs",
            "request input database write",
        )],
        &["database_write"],
        None,
        Some(65_536),
    );
    assert_eq!(
        first["routing_packet"]["deterministic_summary"],
        second["routing_packet"]["deterministic_summary"]
    );
    assert_eq!(
            first["routing_packet"]["deterministic_summary"]["intent"].as_str(),
            Some("I classified this as dataflow_trace because I found dataflow:request input, dataflow:database write, dataflow:flow.")
        );
    assert_eq!(
        first["routing_packet"]["deterministic_summary"]["proof"].as_str(),
        Some("I found no verified graph proof path. This packet uses source text evidence only.")
    );
}

fn routing_fixture_evidence(file: &str, text: &str) -> Value {
    json!({
        "id": format!("text-evidence://{}:1", file),
        "symbol": file.rsplit('/').next().unwrap_or(file),
        "kind": "source_text",
        "file": file,
        "source_span": {
            "file": file,
            "start_line": 1,
            "end_line": 8
        },
        "evidence_role": "text_evidence",
        "proof_status": "no_proof_path_found",
        "graph_proof": false,
        "claimability": super::text_evidence_claimability_json(),
        "seed_matches": [text],
        "matched_seeds": [text],
        "follow_up_queries": [text],
        "classification_reason": text,
        "classification_source": "unit-test/stage0_fts",
        "fallback_source": "text_evidence/no_proof_path_found",
        "score": 1.0
    })
}

fn routing_packet_response_for_task(
    task: &str,
    fallback_evidence: &[Value],
    symbols: &[&str],
    vector_trace: Option<Value>,
    max_output_bytes: Option<usize>,
) -> Value {
    let mut options = context_agent_test_options("production", Some(8), Some(8), max_output_bytes);
    options.task = task.to_string();
    options.seeds = symbols.iter().map(|symbol| (*symbol).to_string()).collect();
    let snippets = fallback_evidence
        .iter()
        .filter_map(|evidence| {
            let file = evidence.get("file").and_then(Value::as_str)?;
            Some(ContextSnippet {
                file: file.to_string(),
                lines: "1-8".to_string(),
                text: evidence
                    .get("classification_reason")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                reason: "text evidence fallback".to_string(),
            })
        })
        .collect::<Vec<_>>();
    let mut metadata = Metadata::new();
    metadata.insert(
        "fallback_evidence".to_string(),
        json!(fallback_evidence.to_vec()),
    );
    metadata.insert("proof_path_available".to_string(), json!(false));
    metadata.insert("proof_status".to_string(), json!("no_proof_path_found"));
    metadata.insert("graph_proof".to_string(), json!(false));
    metadata.insert(
        "graph_verification_status".to_string(),
        json!("no_proof_path_found"),
    );
    metadata.insert(
        "evidence_status".to_string(),
        json!(if fallback_evidence.is_empty() {
            "no_evidence_found"
        } else {
            "fallback_evidence_found"
        }),
    );
    metadata.insert(
        "likely_files".to_string(),
        json!(fallback_evidence
            .iter()
            .filter_map(|evidence| evidence.get("file").and_then(Value::as_str))
            .collect::<Vec<_>>()),
    );
    if let Some(vector_trace) = vector_trace {
        metadata.insert("vector_candidate_trace".to_string(), vector_trace);
    }
    let packet = ContextPacket {
        task: task.to_string(),
        mode: "production".to_string(),
        symbols: symbols.iter().map(|symbol| (*symbol).to_string()).collect(),
        verified_paths: Vec::new(),
        risks: Vec::new(),
        recommended_tests: Vec::new(),
        snippets,
        metadata,
    };
    super::context_pack_agent_json_response(
        &options,
        &packet,
        &json!({"claimable": true, "diagnostic_only": false, "decision": "read_reuse"}),
        super::ContextPackBudgets::for_options(&options),
        Path::new("fixture"),
        Path::new("fixture/.codegraph/codegraph.sqlite"),
        json!({"wall_ms": 1.0}),
    )
}

fn routing_vector_metric_trace() -> Value {
    json!({
        "schema_version": 1,
        "diagnostic_only": true,
        "vector_enabled": true,
        "vector_index_status": "ready",
        "vector_candidate_count": 1,
        "vector_index_metrics": {
            "actual_index_file_bytes": 1234,
            "estimated_f32_payload_bytes": 256,
            "estimated_f32_payload_dim": 64,
            "estimated_f32_payload_count": 1,
            "index_artifact_format": "pretty_json",
            "vector_payload_compression": "none",
            "stores_chunk_text": true,
            "stores_chunk_metadata": true,
            "stores_full_source_body": false,
            "generated_total_chunks": 10,
            "selected_total_chunks": 4,
            "persisted_total_chunks": 4,
            "chunk_selection_strategy": "diversity_ranked_v1",
            "input_order_cap": false
        }
    })
}

fn routing_files(routing: &Value) -> Vec<String> {
    routing["critical_files"]
        .as_array()
        .expect("critical files")
        .iter()
        .filter_map(|file| file["file"].as_str().map(str::to_string))
        .collect()
}

fn routing_roles(routing: &Value, key: &str) -> Vec<String> {
    routing[key]
        .as_array()
        .expect("routing section")
        .iter()
        .filter_map(|item| item["role"].as_str().map(str::to_string))
        .collect()
}

fn context_agent_test_options(
    mode: &str,
    limit_paths: Option<usize>,
    limit_snippets: Option<usize>,
    max_output_bytes: Option<usize>,
) -> super::ContextPackOptions {
    super::ContextPackOptions {
        task: "Change prod_value".to_string(),
        mode: mode.to_string(),
        token_budget: 2_000,
        seeds: vec!["prod_value".to_string()],
        stage0_candidates: Vec::new(),
        profile: false,
        explain: false,
        output_mode: QueryOutputMode::AgentJson,
        limit_paths,
        limit_snippets,
        max_output_bytes,
        allow_stale_read: false,
        allow_foreign_db: false,
        explicit_scope_policy: None,
        enable_vector_candidates: false,
        enable_nuance_rescue_candidates: false,
        vector_index_path: None,
        vector_audit_artifact_path: None,
        enable_candidate_spool: false,
        candidate_spool_path: None,
        allow_stale_candidate_spool: false,
    }
}

fn context_agent_test_packet(
    mode: &str,
    path_count: usize,
    snippet_count: usize,
    role: &str,
) -> ContextPacket {
    let paths = (0..path_count)
        .map(|index| {
            context_agent_test_path(&format!("{role}-path-{index}"), index as u32 + 3, role)
        })
        .collect::<Vec<_>>();
    let snippets = (0..snippet_count)
        .map(|index| ContextSnippet {
            file: "src/lib.rs".to_string(),
            lines: format!("{}", index as u32 + 3),
            text: format!(
                "{}: pub fn {}_{}() -> i32 {{ {} }}",
                index + 3,
                role,
                index,
                "x".repeat(256)
            ),
            reason: "proof path source span".to_string(),
        })
        .collect::<Vec<_>>();
    let mut metadata = Metadata::new();
    metadata.insert(
        "candidate_path_count_before_dedup".to_string(),
        json!(path_count + 3),
    );
    metadata.insert(
        "requested_source_span_count".to_string(),
        json!(snippet_count + 3),
    );
    ContextPacket {
        task: "Change prod_value".to_string(),
        mode: mode.to_string(),
        symbols: vec!["prod_value".to_string()],
        verified_paths: paths,
        risks: (0..8)
            .map(|index| format!("risk {index}: {}", "r".repeat(128)))
            .collect(),
        recommended_tests: (0..8)
            .map(|index| format!("cargo test context_agent_{index}"))
            .collect(),
        snippets,
        metadata,
    }
}

fn context_agent_test_path(id: &str, line: u32, role: &str) -> PathEvidence {
    let span = SourceSpan::with_columns("src/lib.rs", line, 5, line, 24);
    let (source, target, reason, classification_source) = match role {
        "test" => (
            "src::lib.tests.calls_prod_value",
            "src::lib.prod_value",
            "qualified name contains tests module",
            "endpoint:qualified_name",
        ),
        "mock" => (
            "src::lib.MockService",
            "src::lib.prod_value",
            "entity kind/name identifies mock evidence",
            "entity_metadata",
        ),
        _ => (
            "src::lib.prod_neighbor",
            "src::lib.prod_value",
            "stored source-role metadata",
            "stored_metadata",
        ),
    };
    let mut metadata = Metadata::new();
    metadata.insert("evidence_role".to_string(), json!(role));
    metadata.insert("classification_reason".to_string(), json!(reason));
    metadata.insert(
        "classification_source".to_string(),
        json!(classification_source),
    );
    metadata.insert(
        "production_proof_eligible".to_string(),
        json!(role == "production"),
    );
    metadata.insert(
        "edge_labels".to_string(),
        json!([{
            "edge_id": format!("{id}-edge"),
            "evidence_role": role,
            "classification_reason": reason,
            "classification_source": classification_source
        }]),
    );
    PathEvidence {
        id: id.to_string(),
        summary: None,
        source: source.to_string(),
        target: target.to_string(),
        metapath: vec![RelationKind::Calls],
        edges: vec![(source.to_string(), RelationKind::Calls, target.to_string())],
        source_spans: vec![span],
        exactness: Exactness::ParserVerified,
        length: 1,
        confidence: 1.0,
        metadata,
    }
}

fn rust_inline_context_pack_fixture() -> PathBuf {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
            repo.join("Cargo.toml"),
            "[package]\nname = \"inline_context_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("write cargo manifest");
    fs::write(
        repo.join("src").join("lib.rs"),
        r#"pub fn greet(name: &str) -> String {
    format!("hello {name}")
}

pub fn main_call() -> String {
    greet("wasif")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_works() {
        assert_eq!(greet("wasif"), "hello wasif");
    }
}
"#,
    )
    .expect("write inline Rust fixture");
    repo
}

fn serialized_len_for_test(value: &Value) -> usize {
    serde_json::to_vec(value).expect("serialize JSON").len()
}

fn planning_query_exists(queries: &[Value], query_text: &str, path_scope: &str) -> bool {
    queries.iter().any(|query| {
        query["query_text"].as_str() == Some(query_text)
            && query["path_scope"].as_str() == Some(path_scope)
    })
}

fn retrieval_stage_status(stages: &[Value], stage: &str, status: &str) -> bool {
    stages.iter().any(|candidate_stage| {
        candidate_stage["stage"].as_str() == Some(stage)
            && candidate_stage["status"].as_str() == Some(status)
    })
}

fn schema_repo_root_for_test() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn schema_path_for_test(schema_name: &str) -> PathBuf {
    schema_repo_root_for_test()
        .join("docs")
        .join("schemas")
        .join("agent-json")
        .join(format!("{schema_name}.schema.json"))
}

fn assert_schema_required_fields_present(value: &Value, schema_name: &str) {
    let schema_path = schema_path_for_test(schema_name);
    let schema_text = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("read schema {}: {error}", schema_path.display()));
    let schema: Value = serde_json::from_str(&schema_text)
        .unwrap_or_else(|error| panic!("parse schema {}: {error}", schema_path.display()));
    let required = schema["required"].as_array().unwrap_or_else(|| {
        panic!(
            "schema {} missing top-level required array",
            schema_path.display()
        )
    });
    for field in required {
        let field = field.as_str().expect("required field name");
        assert!(
            value.get(field).is_some(),
            "{schema_name} output missing required field {field}: {value}"
        );
    }
}

fn value_contains_nonempty_array_for_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(name, child)| {
            (name == key && child.as_array().is_some_and(|items| !items.is_empty()))
                || value_contains_nonempty_array_for_key(child, key)
        }),
        Value::Array(items) => items
            .iter()
            .any(|child| value_contains_nonempty_array_for_key(child, key)),
        _ => false,
    }
}

fn write_agent_use_hard_interrupt_fixture(root: &Path, target_count: usize) {
    assert!(target_count > 0, "hard interrupt fixture needs targets");
    write_cli_fixture_file(root, "package.json", "{\n  \"type\": \"module\"\n}\n");
    let target_names = (0..target_count)
        .map(|index| format!("hardInterruptTarget{index}"))
        .collect::<Vec<_>>();
    let service_source = target_names
        .iter()
        .enumerate()
        .map(|(index, name)| format!("export function {name}() {{\n  return {index};\n}}\n\n"))
        .collect::<String>()
        + "export function hardInterruptStillHere() {\n  return \"still-here\";\n}\n";
    write_cli_fixture_file(root, "src/service.js", &service_source);

    let calls = target_names
        .iter()
        .map(|name| format!("  outputs.push({name}());\n"))
        .collect::<String>();
    write_cli_fixture_file(
        root,
        "src/consumer.js",
        &format!(
            "import {{ {} }} from './service';\n\nexport function runHardInterruptConsumer() {{\n  const outputs = [];\n{}  return outputs.join(',');\n}}\n",
            target_names.join(", "),
            calls
        ),
    );
}

fn remove_agent_use_hard_interrupt_targets(root: &Path) {
    write_cli_fixture_file(
        root,
        "src/service.js",
        "export function hardInterruptStillHere() {\n  return \"changed\";\n}\n",
    );
}

fn run_agent_use_hard_interrupt_watch(target_count: usize, extra_args: &[&str]) -> Value {
    let data_root = temp_repo();
    let repo = temp_repo();
    write_agent_use_hard_interrupt_fixture(&repo, target_count);
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index for hard interrupt fixture");

    remove_agent_use_hard_interrupt_targets(&repo);
    let mut args = vec![
        "watch".to_string(),
        "--repo".to_string(),
        path_string(&repo),
        "--once".to_string(),
        "--changed".to_string(),
        "src/service.js".to_string(),
        "--json".to_string(),
    ];
    args.extend(extra_args.iter().map(|arg| (*arg).to_string()));
    let watch = with_agent_use_data_root(&data_root, || super::run_agent_use_command(&args))
        .expect("agent-use watch hard interrupt fixture");

    assert_eq!(watch["external_db_used"].as_bool(), Some(true), "{watch:?}");
    assert_eq!(
        watch["db_path"].as_str(),
        Some(path_string(&profile.db_path).as_str()),
        "{watch:?}"
    );
    assert_eq!(
        watch["watch_db"]["actual_db_path_opened"].as_str(),
        Some(path_string(&profile.db_path).as_str()),
        "{watch:?}"
    );
    assert_eq!(watch["normal_dot_codegraph_mutated"].as_bool(), Some(false));
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup hard interrupt repo");
    remove_dir_all_with_retry(&data_root, "cleanup hard interrupt data root");
    watch
}

fn run_agent_use_warning_only_watch() -> Value {
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(
        &repo,
        "package/foo/Config.in",
        "config BR2_PACKAGE_FOO\n\tbool \"foo package\"\n",
    );
    let profile =
        super::resolve_agent_use_profile_with_data_root(&repo, &data_root).expect("profile");
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index warning-only fixture");

    write_cli_fixture_file(
        &repo,
        "package/foo/Config.in",
        "config BR2_PACKAGE_FOO\n\tbool \"foo package\"\n\thelp\n\t  Text evidence changed only; no exact graph relation is implied.\n",
    );
    let watch = with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "watch".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--once".to_string(),
            "--changed".to_string(),
            "package/foo/Config.in".to_string(),
            "--json".to_string(),
        ])
    })
    .expect("agent-use watch warning-only fixture");

    assert_eq!(watch["external_db_used"].as_bool(), Some(true), "{watch:?}");
    assert_eq!(
        watch["db_path"].as_str(),
        Some(path_string(&profile.db_path).as_str()),
        "{watch:?}"
    );
    assert_eq!(watch["normal_dot_codegraph_mutated"].as_bool(), Some(false));
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup warning-only repo");
    remove_dir_all_with_retry(&data_root, "cleanup warning-only data root");
    watch
}

fn run_agent_use_over_budget_degraded_watch() -> Value {
    let data_root = temp_repo();
    let repo = temp_repo();
    write_cli_fixture_file(&repo, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function targetForSurfaceBudget() {\n  return \"old\";\n}\n",
    );
    for index in 0..3 {
        write_cli_fixture_file(
            &repo,
            &format!("src/consumer{index}.js"),
            "import { targetForSurfaceBudget } from './service';\n\
             export function runSurfaceBudgetConsumer() {\n  return targetForSurfaceBudget();\n}\n",
        );
    }
    with_agent_use_data_root(&data_root, || {
        super::run_agent_use_command(&[
            "index".to_string(),
            "--repo".to_string(),
            path_string(&repo),
            "--json".to_string(),
        ])
    })
    .expect("agent-use index degraded fixture");

    write_cli_fixture_file(
        &repo,
        "src/service.js",
        "export function targetForSurfaceBudget() {\n  return \"new\";\n}\n",
    );
    let watch = with_process_env_var("CODEGRAPH_RTDS_CLOSURE_MAX_DIRTY_FILES", "1", || {
        with_agent_use_data_root(&data_root, || {
            super::run_agent_use_command(&[
                "watch".to_string(),
                "--repo".to_string(),
                path_string(&repo),
                "--once".to_string(),
                "--changed".to_string(),
                "src/service.js".to_string(),
                "--json".to_string(),
            ])
        })
    })
    .expect("agent-use watch degraded fixture");

    assert_eq!(watch["external_db_used"].as_bool(), Some(true), "{watch:?}");
    assert_eq!(watch["normal_dot_codegraph_mutated"].as_bool(), Some(false));
    assert_no_dot_codegraph_sqlite(&repo);

    remove_dir_all_with_retry(&repo, "cleanup degraded repo");
    remove_dir_all_with_retry(&data_root, "cleanup degraded data root");
    watch
}

fn assert_watch_hard_interrupt_blocking(watch: &Value) {
    assert_eq!(watch["status"].as_str(), Some("updated"), "{watch:?}");
    assert_eq!(
        watch["validation_status"].as_str(),
        Some("blocking_graph_error")
    );
    assert_eq!(
        watch["validation_must_fix_before_continuing"].as_bool(),
        Some(true)
    );
    assert!(
        watch["validation_blocking_error_count"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "{watch:?}"
    );
    assert_eq!(watch["hard_interrupt_available"].as_bool(), Some(true));
    assert_eq!(
        watch["hard_interrupt_not_implemented"].as_bool(),
        Some(false)
    );
    assert_eq!(
        watch["validation_packet"]["hard_interrupt_available"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["validation_packet"]["must_fix_before_continuing"].as_bool(),
        Some(true)
    );
    assert_eq!(
        watch["hard_interrupt"],
        watch["validation_packet"]["hard_interrupt"]
    );
    let interrupt = &watch["hard_interrupt"];
    assert_eq!(interrupt["packet_kind"].as_str(), Some("hard_interrupt"));
    assert_eq!(interrupt["status"].as_str(), Some("blocking_graph_error"));
    assert_eq!(
        interrupt["must_fix_before_continuing"].as_bool(),
        Some(true)
    );
    assert_eq!(interrupt["hard_interrupt_available"].as_bool(), Some(true));
    assert!(
        interrupt["error_count"].as_u64().unwrap_or_default() > 0,
        "{watch:?}"
    );
    let errors = interrupt["errors"]
        .as_array()
        .expect("hard interrupt errors");
    assert!(!errors.is_empty(), "{watch:?}");
    assert!(errors.iter().all(|error| {
        error["classification"].as_str() == Some("block")
            && error["blocking_level"].as_str() == Some("blocking")
            && error["source_span"].is_object()
            && error["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty())
            && error["recommended_fix"]
                .as_str()
                .is_some_and(|fix| !fix.is_empty())
            && error["suggested_next_steps"]
                .as_array()
                .is_some_and(|steps| !steps.is_empty())
    }));
    assert!(errors.iter().any(|error| {
        matches!(
            error["relation_kind"].as_str(),
            Some("CALLS" | "IMPORTS" | "EXPORTS" | "ALIASED_BY")
        )
    }));
    assert!(
        interrupt["summary"]["top_error_source_span"].is_object(),
        "{watch:?}"
    );
    assert!(
        interrupt["summary"]["top_error_recommended_fix"]
            .as_str()
            .is_some_and(|fix| !fix.is_empty()),
        "{watch:?}"
    );
    if let Some(preserved) = interrupt["critical_safety_fields_preserved"].as_bool() {
        assert!(preserved, "{watch:?}");
    }
}

fn assert_json_array_contains(value: &Value, key: &str, expected: &str) {
    let items = value
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{key} is not an array in {value:?}"));
    assert!(
        items.iter().any(|item| item.as_str() == Some(expected)),
        "{key} did not contain {expected}: {items:?}"
    );
}

fn count_key_occurrences(value: &Value, key: &str) -> usize {
    match value {
        Value::Object(object) => {
            let here = usize::from(object.contains_key(key));
            here + object
                .values()
                .map(|child| count_key_occurrences(child, key))
                .sum::<usize>()
        }
        Value::Array(array) => array
            .iter()
            .map(|child| count_key_occurrences(child, key))
            .sum(),
        _ => 0,
    }
}

fn assert_agent_json_contract(value: &Value, schema_name: &str, command: &str, max_bytes: usize) {
    assert_schema_required_fields_present(value, schema_name);
    assert_eq!(value["schema_name"].as_str(), Some(schema_name));
    assert_eq!(value["schema_version"].as_u64(), Some(1));
    assert_eq!(value["command"].as_str(), Some(command));
    assert!(value["repo"].as_str().is_some_and(|repo| !repo.is_empty()));
    assert!(value["db"].as_str().is_some_and(|db| !db.is_empty()));
    assert!(matches!(
        value["status"].as_str(),
        Some("ok" | "warning" | "error")
    ));
    assert!(value["lifecycle"]["claimable"].as_bool().is_some());
    assert!(value["lifecycle"]["diagnostic_only"].as_bool().is_some());
    assert!(value["claimable"].as_bool().is_some());
    assert!(value["diagnostic_only"].as_bool().is_some());
    assert!(value["warnings"].as_array().is_some());
    assert!(value["errors"].as_array().is_some());
    assert!(value["timings"].is_object(), "{schema_name}: {value}");
    assert!(
        !value_contains_nonempty_array_for_key(value, "included_examples"),
        "{schema_name} leaked included scope examples: {value}"
    );
    assert!(
        !value_contains_nonempty_array_for_key(value, "excluded_examples"),
        "{schema_name} leaked excluded scope examples: {value}"
    );
    let bytes = serialized_len_for_test(value);
    assert!(
        bytes <= max_bytes,
        "{schema_name} exceeded size target {max_bytes} with {bytes} bytes: {value}"
    );
}

#[test]
fn agent_json_schema_files_parse_and_require_common_fields() {
    let schemas = [
        "index_agent_json",
        "query_symbols_agent_json",
        "query_text_agent_json",
        "query_files_agent_json",
        "context_pack_agent_json",
        "callers_callees_agent_json",
        "status_compact_json",
        "doctor_compact_json",
    ];
    let common_required = [
        "schema_name",
        "schema_version",
        "status",
        "command",
        "repo",
        "db",
        "lifecycle",
        "claimable",
        "diagnostic_only",
        "warnings",
        "errors",
        "timings",
    ];
    for schema_name in schemas {
        let schema_path = schema_path_for_test(schema_name);
        let schema_text = fs::read_to_string(&schema_path)
            .unwrap_or_else(|error| panic!("read schema {}: {error}", schema_path.display()));
        let schema: Value = serde_json::from_str(&schema_text)
            .unwrap_or_else(|error| panic!("parse schema {}: {error}", schema_path.display()));
        assert_eq!(schema["title"].as_str(), Some(schema_name));
        let required = schema["required"]
            .as_array()
            .expect("required schema fields")
            .iter()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>();
        for field in common_required {
            assert!(
                required.contains(field),
                "{schema_name} schema does not require {field}"
            );
        }
    }
}

fn index_output_fixture_repo() -> PathBuf {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
        repo.join("src").join("service.ts"),
        "export function indexOutputService(value: number) { return value + 1; }\n",
    )
    .expect("write service");
    fs::write(
        repo.join("src").join("excluded.ts"),
        "export function excludedFromIndexOutput() { return 0; }\n",
    )
    .expect("write excluded");
    repo
}

fn run_index_output_json(repo: &Path, db: &Path, extra_args: &[&str]) -> Value {
    let mut args = vec![
        BIN_NAME.to_string(),
        "index".to_string(),
        path_string(repo),
        "--db".to_string(),
        path_string(db),
    ];
    args.extend(extra_args.iter().map(|arg| arg.to_string()));
    let output = run(args);
    assert_eq!(
        output.exit_code, 0,
        "stdout:\n{}\nstderr:\n{}",
        output.stdout, output.stderr
    );
    serde_json::from_str(&output.stdout).expect("index JSON")
}

fn caller_callee_precision_fixture() -> CallerCalleePrecisionFixture {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
        repo.join("src").join("seed.ts"),
        "export function seed() {\n  return 1;\n}\n",
    )
    .expect("write seed");
    index_repo(&repo).expect("index seed repo");
    let store = SqliteGraphStore::open(default_db_path(&repo)).expect("open store");

    let alpha_target = test_function_entity("src/alpha.ts", "target", "alpha.target", 3);
    let beta_target = test_function_entity("src/beta.ts", "target", "beta.target", 4);
    let unique_target =
        test_function_entity("src/unique.ts", "uniqueTarget", "unique.uniqueTarget", 5);
    let alpha_caller = test_function_entity("src/alpha.ts", "callAlpha", "alpha.callAlpha", 10);
    let beta_caller = test_function_entity("src/beta.ts", "callBeta", "beta.callBeta", 11);
    let unique_caller =
        test_function_entity("src/unique.ts", "callUnique", "unique.callUnique", 12);

    for entity in [
        &alpha_target,
        &beta_target,
        &unique_target,
        &alpha_caller,
        &beta_caller,
        &unique_caller,
    ] {
        store.upsert_entity(entity).expect("upsert entity");
    }
    for edge in [
        test_call_edge(&alpha_caller, &alpha_target, "src/alpha.ts", 13),
        test_call_edge(&beta_caller, &beta_target, "src/beta.ts", 14),
        test_call_edge(&unique_caller, &unique_target, "src/unique.ts", 15),
    ] {
        store.upsert_edge(&edge).expect("upsert edge");
    }

    CallerCalleePrecisionFixture {
        repo,
        alpha_target_id: alpha_target.id,
        beta_target_id: beta_target.id,
        unique_target_id: unique_target.id,
        alpha_caller_id: alpha_caller.id,
        beta_caller_id: beta_caller.id,
        unique_caller_id: unique_caller.id,
    }
}

fn test_function_entity(path: &str, name: &str, qualified_name: &str, line: u32) -> Entity {
    Entity {
        id: stable_entity_id_for_kind(
            path,
            EntityKind::Function,
            qualified_name,
            Some(qualified_name),
        ),
        kind: EntityKind::Function,
        name: name.to_string(),
        qualified_name: qualified_name.to_string(),
        repo_relative_path: path.to_string(),
        source_span: Some(SourceSpan::with_columns(path, line, 1, line, 20)),
        content_hash: None,
        file_hash: None,
        created_from: "unit-test".to_string(),
        confidence: 1.0,
        metadata: Default::default(),
    }
}

fn test_call_edge(head: &Entity, tail: &Entity, path: &str, line: u32) -> Edge {
    let span = SourceSpan::with_columns(path, line, 3, line, 24);
    Edge {
        id: stable_edge_id(&head.id, RelationKind::Calls, &tail.id, &span),
        head_id: head.id.clone(),
        relation: RelationKind::Calls,
        tail_id: tail.id.clone(),
        source_span: span,
        repo_commit: None,
        file_hash: None,
        extractor: "unit-test".to_string(),
        confidence: 1.0,
        exactness: Exactness::ParserVerified,
        edge_class: EdgeClass::BaseExact,
        context: EdgeContext::Production,
        derived: false,
        provenance_edges: Vec::new(),
        metadata: Default::default(),
    }
}

fn temp_repo() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let counter = TEMP_REPO_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "codegraph-cli-unit-{}-{nanos}-{counter}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create temp repo");
    path
}

fn remove_dir_all_with_retry(path: &Path, label: &str) {
    let mut last_error = None;
    for attempt in 0..20 {
        match fs::remove_dir_all(path) {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                last_error = Some(error);
                let backoff_ms = 25 * (attempt + 1).min(10);
                std::thread::sleep(Duration::from_millis(backoff_ms));
            }
        }
    }
    panic!(
        "{label}: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "unknown cleanup failure".to_string())
    );
}

fn temp_repo_named(name: &str) -> (PathBuf, PathBuf) {
    let parent = temp_repo();
    let repo = parent.join(name);
    fs::create_dir_all(&repo).expect("create named temp repo");
    (parent, repo)
}

fn write_cli_fixture_file(root: &Path, relative: &str, source: &str) {
    let path = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture parent");
    }
    fs::write(path, source).expect("write fixture source");
}

fn write_agent_use_context_fixture(root: &Path) {
    write_cli_fixture_file(root, "package.json", "{\n  \"type\": \"module\"\n}\n");
    write_cli_fixture_file(
            root,
            "src/service.ts",
            "export function agentUseTarget() {\n  return \"agent-use-ok\";\n}\n\nexport function callAgentUseTarget() {\n  return agentUseTarget();\n}\n",
        );
}

fn write_agent_use_rust_relation_fixture(root: &Path) {
    write_cli_fixture_file(
            root,
            "Cargo.toml",
            "[package]\nname = \"agent-use-rust-relation-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        );
    write_cli_fixture_file(
            root,
            "src/lib.rs",
            "pub mod graph {\n    pub fn leaf() {}\n\n    pub fn middle() {\n        leaf();\n    }\n\n    pub fn root() {\n        middle();\n    }\n}\n\npub struct Service;\n\nimpl Service {\n    pub fn new() -> Self {\n        Service\n    }\n\n    pub fn run(&self) {}\n}\n\npub fn caller() {\n    let service = Service::new();\n    service.run();\n    graph::root();\n}\n\npub fn reference_user() {\n    graph::leaf();\n}\n",
        );
}

fn assert_agent_use_relation_query_claims_profile_db(
    value: &Value,
    profile: &super::AgentUseProfile,
) {
    assert!(
        matches!(value["status"].as_str(), Some("ok") | Some("warning")),
        "{value:?}"
    );
    assert_eq!(
        value["profile_name"].as_str(),
        Some(super::PRODUCTION_AGENT_USE_PROFILE_NAME)
    );
    assert_eq!(value["external_db_used"].as_bool(), Some(true));
    assert_eq!(value["db_source"].as_str(), Some("agent-use profile"));
    assert_eq!(
        value["db"].as_str(),
        Some(path_string(&profile.db_path).as_str())
    );
    if !value["db_lifecycle_read"].is_null() {
        assert_eq!(
            value["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
            Some(path_string(&profile.db_path).as_str())
        );
    }
    assert_eq!(value["normal_dot_codegraph_mutated"].as_bool(), Some(false));
}

fn assert_no_dot_codegraph_sqlite(root: &Path) {
    let dot_codegraph = root.join(".codegraph");
    assert!(
        !dot_codegraph.exists(),
        "repo-local .codegraph should not exist: {}",
        dot_codegraph.display()
    );
    if dot_codegraph.exists() {
        let entries = fs::read_dir(&dot_codegraph)
            .expect("read .codegraph")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        assert!(
            entries.iter().all(|path| {
                let name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default();
                !(name.ends_with(".sqlite")
                    || name.ends_with(".sqlite-wal")
                    || name.ends_with(".sqlite-shm"))
            }),
            "repo-local SQLite artifacts found: {entries:?}"
        );
    }
}

fn add_git_remote_for_test(repo: &Path, remote: &str) {
    let init = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["init", "-q"])
        .status()
        .expect("run git init");
    assert!(init.success(), "git init failed with {init}");
    let add = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["remote", "add", "origin", remote])
        .status()
        .expect("run git remote add");
    assert!(add.success(), "git remote add failed with {add}");
}

fn with_agent_use_data_root<F, R>(data_root: &Path, operation: F) -> R
where
    F: FnOnce() -> R,
{
    super::with_process_context_lock(|| {
        let guard = ProcessEnvGuard {
            name: super::AGENT_USE_DATA_ROOT_ENV.to_string(),
            old: std::env::var_os(super::AGENT_USE_DATA_ROOT_ENV),
        };
        std::env::set_var(super::AGENT_USE_DATA_ROOT_ENV, data_root);
        let result = operation();
        drop(guard);
        result
    })
}

struct ProcessEnvGuard {
    name: String,
    old: Option<OsString>,
}

impl Drop for ProcessEnvGuard {
    fn drop(&mut self) {
        if let Some(old) = self.old.take() {
            std::env::set_var(&self.name, old);
        } else {
            std::env::remove_var(&self.name);
        }
    }
}

fn with_process_env_var<F, R>(name: &str, value: &str, operation: F) -> R
where
    F: FnOnce() -> R,
{
    super::with_process_context_lock(|| {
        let guard = ProcessEnvGuard {
            name: name.to_string(),
            old: std::env::var_os(name),
        };
        std::env::set_var(name, value);
        let result = operation();
        drop(guard);
        result
    })
}

fn run_agent_use_test_command(data_root: &Path, args: &[&str]) -> Result<Value, String> {
    let owned = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    with_agent_use_data_root(data_root, || super::run_agent_use_command(&owned))
}

fn agent_use_query_count(data_root: &Path, repo: &Path, kind: &str, query: &str) -> u64 {
    let value = run_agent_use_test_command(
        data_root,
        &[
            "query",
            kind,
            query,
            "--repo",
            path_string(repo).as_str(),
            "--limit",
            "10",
            "--agent-json",
        ],
    )
    .expect("agent-use query");
    assert_eq!(value["status"].as_str(), Some("ok"), "{value:?}");
    value["result_count"].as_u64().unwrap_or_default()
}

fn agent_use_query_result_paths(
    data_root: &Path,
    repo: &Path,
    kind: &str,
    query: &str,
) -> BTreeSet<String> {
    let value = run_agent_use_test_command(
        data_root,
        &[
            "query",
            kind,
            query,
            "--repo",
            path_string(repo).as_str(),
            "--limit",
            "10",
            "--agent-json",
        ],
    )
    .expect("agent-use query");
    assert_eq!(value["status"].as_str(), Some("ok"), "{value:?}");
    value
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|result| {
            result
                .get("repo_relative_path")
                .or_else(|| result.get("file"))
                .or_else(|| result.get("path"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

fn path_evidence_total_count(db_path: &Path) -> u64 {
    SqliteGraphStore::open_read_only(db_path)
        .expect("open path evidence DB")
        .count_path_evidence()
        .expect("count path evidence")
}

fn write_candidate_spool_cli_fixture(root: &Path) -> PathBuf {
    let source = "pub fn spool_target() {}\n";
    write_cli_fixture_file(root, "src/lib.rs", source);
    let repo_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let source_hash = codegraph_parser::content_hash(source);
    let spool = root.join("candidate-spool.jsonl");
    let manifest = json!({
        "metadata": {
            "metadata_version": "candidate_spool_v1",
            "artifact_kind": "candidate_spool",
            "artifact_format": "jsonl",
            "repo_root": path_string(&repo_root),
            "candidate_spool_status": "partial_ready",
            "lifecycle": "partial_spool",
            "incomplete": true,
            "candidate_only": true,
            "graph_proof": false,
            "claimable_for_graph": false
        }
    });
    let chunk = json!({
        "chunk_id": "candidate-spool:test:symbol",
        "chunk_kind": "signature",
        "source_kind": "graph_entity",
        "path": "src/lib.rs",
        "entity_id": "entity:spool_target",
        "source_span": {
            "repo_relative_path": "src/lib.rs",
            "start_line": 1,
            "start_col": 1,
            "end_line": 1,
            "end_col": 23
        },
        "proof_status": "candidate_only",
        "graph_proof": false,
        "claimable_for_graph": false,
        "requires_graph_verification": true,
        "text": "function spool_target in src/lib.rs",
        "selection_bucket": "symbol_signature",
        "selection_reason": "test candidate spool symbol signature",
        "source_file_content_hash": source_hash,
        "source_file_size_bytes": source.as_bytes().len() as u64,
        "lifecycle": "partial_spool",
        "incomplete": true
    });
    fs::write(
        &spool,
        format!(
            "{}\n{}\n",
            serde_json::to_string(&manifest).expect("manifest json"),
            serde_json::to_string(&chunk).expect("chunk json")
        ),
    )
    .expect("write candidate spool");
    spool
}

fn init_git_repo_for_bundle_test(root: &Path) -> String {
    let init = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("init")
        .output()
        .expect("run git init");
    assert!(
        init.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    for (key, value) in [
        ("user.email", "codegraph-tests@example.invalid"),
        ("user.name", "CodeGraph Tests"),
    ] {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["config", key, value])
            .output()
            .expect("run git config");
        assert!(
            output.status.success(),
            "git config {key} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let add = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["add", "."])
        .output()
        .expect("run git add");
    assert!(
        add.status.success(),
        "git add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let commit = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["commit", "-m", "bundle-identity-test"])
        .output()
        .expect("run git commit");
    assert!(
        commit.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&commit.stderr)
    );
    super::git_head(root).expect("bundle git head")
}

fn bundle_fixture_repo(symbol: &str) -> PathBuf {
    let repo = temp_repo();
    write_cli_fixture_file(
        &repo,
        "src/main.ts",
        &format!("export function {symbol}() {{ return 1; }}\n"),
    );
    repo
}

fn db_has_symbol(db_path: &Path, symbol: &str) -> bool {
    let store = SqliteGraphStore::open_read_only(db_path).expect("open read-only DB");
    store
        .list_entities(super::UNBOUNDED_STORE_READ_LIMIT)
        .expect("list entities")
        .iter()
        .any(|entity| entity.name == symbol || entity.qualified_name.contains(symbol))
}

struct BundleFailpointEnvGuard {
    previous: Option<std::ffi::OsString>,
}

impl BundleFailpointEnvGuard {
    fn set(failpoint: &str) -> Self {
        let previous = std::env::var_os("CODEGRAPH_WRITE_PATH_FAILPOINT");
        std::env::set_var("CODEGRAPH_WRITE_PATH_FAILPOINT", failpoint);
        Self { previous }
    }
}

impl Drop for BundleFailpointEnvGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            std::env::set_var("CODEGRAPH_WRITE_PATH_FAILPOINT", previous);
        } else {
            std::env::remove_var("CODEGRAPH_WRITE_PATH_FAILPOINT");
        }
    }
}

fn ui_fixture_repo() -> PathBuf {
    let repo = temp_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
            repo.join("src").join("auth.ts"),
            "export function sanitize(input: string) {\n  return input.trim();\n}\n\nexport function saveUser(email: string) {\n  return email;\n}\n\nexport function login(req: any) {\n  const email = sanitize(req.body.email);\n  saveUser(email);\n  return email;\n}\n",
        )
        .expect("write fixture");
    repo
}

fn sqlite_user_version_for_test(db_path: &Path) -> u32 {
    let connection = rusqlite::Connection::open(db_path).expect("open test DB");
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read user_version")
}

fn sqlite_table_exists_for_test(db_path: &Path, table_name: &str) -> bool {
    let connection = rusqlite::Connection::open(db_path).expect("open test DB");
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table_name],
            |row| row.get(0),
        )
        .expect("read sqlite_master")
}

fn sqlite_sidecar_path_for_test(db_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}-{}", db_path.display(), suffix))
}

fn keep_wal_sidecars_for_test(db_path: &Path) -> rusqlite::Connection {
    let connection = rusqlite::Connection::open(db_path).expect("open DB for WAL sidecars");
    connection
        .execute_batch(
            "
                PRAGMA journal_mode = WAL;
                CREATE TABLE IF NOT EXISTS codegraph_sidecar_status_test(
                    id INTEGER PRIMARY KEY,
                    value TEXT NOT NULL
                );
                INSERT INTO codegraph_sidecar_status_test(value) VALUES ('keep-wal-live');
                ",
        )
        .expect("create WAL sidecars");
    assert!(
        sqlite_sidecar_path_for_test(db_path, "wal").exists()
            || sqlite_sidecar_path_for_test(db_path, "shm").exists(),
        "expected WAL/SHM sidecars to exist for valid DB test"
    );
    connection
}

fn http_get_json(addr: SocketAddr, path: &str) -> Value {
    let mut stream = TcpStream::connect(addr).expect("connect UI server");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )
    .expect("write HTTP request");
    let mut response = String::new();
    match stream.read_to_string(&mut response) {
        Ok(_) => {}
        Err(error)
            if error.kind() == ErrorKind::ConnectionReset && response.contains("\r\n\r\n") => {}
        Err(error) => panic!("read HTTP response: {error}"),
    }
    let (_, body) = response.split_once("\r\n\r\n").expect("HTTP body");
    serde_json::from_str(body).expect("JSON response")
}

fn ui_json_body(response: UiResponse) -> Value {
    assert_eq!(response.status, 200);
    serde_json::from_str(&response.body).expect("JSON body")
}
