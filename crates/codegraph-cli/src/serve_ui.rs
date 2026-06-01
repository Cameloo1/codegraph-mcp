//! HTTP UI server, serve-mcp/serve-ui/watch commands, and graph-render helpers.
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use codegraph_core::{Edge, RelationKind, SourceSpan};
use codegraph_query::GraphPath;
use serde_json::{json, Value};

use crate::*;

pub(crate) fn run_serve_mcp_command(args: &[String]) -> CliOutput {
    if !args.is_empty() {
        return command_error("invalid_arguments", "Usage: codegraph-mcp serve-mcp");
    }

    match codegraph_mcp_server::serve_stdio() {
        Ok(()) => success(String::new()),
        Err(error) => command_error("serve_mcp_failed", &error.to_string()),
    }
}

pub(crate) fn run_serve_ui_command(args: &[String]) -> CliOutput {
    let options = match parse_ui_options(args) {
        Ok(options) => options,
        Err(error) => return command_error("serve_ui_failed", &error),
    };

    match serve_ui(&options.repo, &options.host, options.port) {
        Ok(()) => success(String::new()),
        Err(error) => command_error("serve_ui_failed", &error.to_string()),
    }
}

pub(crate) fn run_watch_command(args: &[String]) -> CliOutput {
    let options = match parse_watch_options(args) {
        Ok(options) => options,
        Err(error) => return command_error("watch_failed", &error),
    };

    let repo_root = match resolve_repo_root(&options.repo) {
        Ok(repo_root) => repo_root,
        Err(error) => return command_error("watch_failed", &error),
    };
    let requested_db_path = watch_requested_db_path(&repo_root, options.db.as_deref());

    if options.once {
        let changed_paths = if options.changed_paths.is_empty() {
            Vec::new()
        } else {
            options.changed_paths
        };
        let update = update_changed_files_to_db(&repo_root, &changed_paths, &requested_db_path);
        return run_json_command(
            "watch_failed",
            update
                .and_then(|summary| {
                    let lifecycle = watch_update_lifecycle_metadata(
                        &repo_root,
                        &requested_db_path,
                        "cli.watch.once",
                        true,
                        None,
                    )?;
                    let mut value = serde_json::to_value(summary)
                        .map_err(|error| IndexError::Message(error.to_string()))?;
                    if let Some(object) = value.as_object_mut() {
                        object.insert("watch_db".to_string(), lifecycle);
                    }
                    Ok(value)
                })
                .map_err(|error| error.to_string()),
        );
    }

    match watch_repo(&repo_root, Some(&requested_db_path), options.debounce) {
        Ok(()) => success(String::new()),
        Err(error) => command_error("watch_failed", &error.to_string()),
    }
}

pub(crate) fn watch_repo(
    repo_path: &Path,
    requested_db_path: Option<&Path>,
    debounce: Duration,
) -> Result<(), IndexError> {
    let mut startup =
        prepare_watch_startup(repo_path, requested_db_path, "cli.watch.long_running")?;

    let (sender, receiver) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = sender.send(result);
    })
    .map_err(|error| IndexError::Message(format!("watcher init failed: {error}")))?;
    watcher
        .watch(&startup.repo_root, RecursiveMode::Recursive)
        .map_err(|error| IndexError::Message(format!("watcher start failed: {error}")))?;

    eprintln!(
        "codegraph watch: lifecycle {}",
        json_line(startup.lifecycle_status.clone()).trim_end()
    );
    eprintln!(
        "codegraph watch: watching {} with {}ms debounce requested_db={} actual_db={} auto_index_enabled={}",
        startup.repo_root.display(),
        debounce.as_millis(),
        startup.requested_db_path.display(),
        startup.actual_db_path_opened.display(),
        startup.auto_index_enabled
    );

    let mut debouncer = WatchDebouncer::new(debounce);
    let tick = if debounce.is_zero() {
        Duration::from_millis(50)
    } else {
        std::cmp::min(debounce, Duration::from_millis(250))
    };

    loop {
        match receiver.recv_timeout(tick) {
            Ok(Ok(event)) => {
                enqueue_event_paths(&mut debouncer, event, Instant::now());
            }
            Ok(Err(error)) => {
                eprintln!("codegraph watch: notify error: {error}");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(IndexError::Message(
                    "watcher event channel disconnected".to_string(),
                ));
            }
        }

        let ready = debouncer.ready(Instant::now());
        if ready.is_empty() {
            continue;
        }

        let summary = update_changed_files_with_cache_to_db(
            &startup.repo_root,
            &ready,
            &startup.actual_db_path_opened,
            &mut startup.cache,
        )?;
        log_watch_summary(&summary);
    }
}

pub(crate) fn watch_requested_db_path(repo_root: &Path, db_path: Option<&Path>) -> PathBuf {
    db_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_db_path(repo_root))
}

pub(crate) fn prepare_watch_startup(
    repo_path: &Path,
    requested_db_path: Option<&Path>,
    surface_name: &str,
) -> Result<WatchStartup, IndexError> {
    if !repo_path.exists() {
        return Err(IndexError::RepoNotFound(repo_path.to_path_buf()));
    }
    let repo_root = fs::canonicalize(repo_path)?;
    let requested_db_path = watch_requested_db_path(&repo_root, requested_db_path);
    let preflight = inspect_db_lifecycle_surface_preflight(DbLifecycleSurfacePreflightRequest {
        repo_root: repo_root.clone(),
        db_path: requested_db_path.clone(),
        surface_name: surface_name.to_string(),
        operation_kind: DbLifecycleOperationKind::WriteUpdate,
        allow_stale_read: false,
        allow_foreign_repo: false,
        required_storage_mode: None,
        expected_scope: None,
    })
    .map_err(|error| IndexError::Message(error.to_string()))?;
    if !preflight.safe_to_write {
        return Err(IndexError::Message(watch_lifecycle_error(&preflight)));
    }
    let actual_db_path_opened = PathBuf::from(&preflight.exact_db_path_checked);
    let lifecycle_status = watch_lifecycle_status_json(&preflight, &requested_db_path, false);
    let mut cache = IncrementalIndexCache::new(256)?;
    let store = SqliteGraphStore::open(&actual_db_path_opened)?;
    cache.refresh_from_store(&store)?;
    drop(store);
    Ok(WatchStartup {
        repo_root,
        requested_db_path,
        actual_db_path_opened,
        lifecycle_status,
        auto_index_enabled: false,
        cache,
    })
}

pub(crate) fn watch_update_lifecycle_metadata(
    repo_root: &Path,
    requested_db_path: &Path,
    surface_name: &str,
    safe_to_write: bool,
    expected_scope: Option<IndexScopeOptions>,
) -> Result<Value, IndexError> {
    let preflight = inspect_db_lifecycle_surface_preflight(DbLifecycleSurfacePreflightRequest {
        repo_root: repo_root.to_path_buf(),
        db_path: requested_db_path.to_path_buf(),
        surface_name: surface_name.to_string(),
        operation_kind: DbLifecycleOperationKind::WriteUpdate,
        allow_stale_read: false,
        allow_foreign_repo: false,
        required_storage_mode: None,
        expected_scope,
    })
    .map_err(|error| IndexError::Message(error.to_string()))?;
    let mut value = watch_lifecycle_status_json(&preflight, requested_db_path, false);
    if let Some(object) = value.as_object_mut() {
        object.insert("safe_to_write".to_string(), Value::Bool(safe_to_write));
    }
    Ok(value)
}

pub(crate) fn watch_lifecycle_status_json(
    preflight: &DbLifecycleSurfacePreflight,
    requested_db_path: &Path,
    auto_index_enabled: bool,
) -> Value {
    json!({
        "requested_db_path": path_string(requested_db_path),
        "actual_db_path_opened": preflight.exact_db_path_checked.clone(),
        "lifecycle_status": if preflight.safe_to_write { "safe_to_write" } else { "blocked" },
        "auto_index_enabled": auto_index_enabled,
        "safe_to_read": preflight.safe_to_read,
        "safe_to_write": preflight.safe_to_write,
        "claimable": preflight.claimable,
        "diagnostic_only": preflight.diagnostic_only,
        "passport_status": preflight.passport_status.clone(),
        "repo_match": preflight.repo_match,
        "scope_match": preflight.scope_match,
        "schema_status": preflight.schema_status.clone(),
        "storage_mode_match": preflight.storage_mode_match,
        "blockers": preflight.blockers.clone(),
        "warnings": preflight.warnings.clone(),
        "repo_root_expected": preflight.repo_root_expected.clone(),
        "repo_root_observed": preflight.repo_root_observed.clone(),
        "artifact_freshness": preflight.artifact_freshness.clone(),
        "operation_kind": preflight.operation_kind.as_str(),
        "surface_name": preflight.surface_name.clone(),
    })
}

pub(crate) fn read_lifecycle_artifact_freshness(
    preflight: &DbLifecyclePreflight,
) -> Option<String> {
    if preflight
        .db_health
        .reasons
        .iter()
        .any(|reason| reason.contains("main DB does not exist"))
    {
        return Some("missing".to_string());
    }
    let Some(passport) = preflight.db_health.passport.as_ref() else {
        return Some("passport_missing".to_string());
    };
    if passport.last_run_status != "completed" {
        return Some(format!("incomplete:{}", passport.last_run_status));
    }
    if passport.integrity_gate_result != "ok" {
        return Some(format!(
            "integrity_not_ok:{}",
            passport.integrity_gate_result
        ));
    }
    if preflight
        .db_health
        .reasons
        .iter()
        .any(|reason| reason.contains("repo head mismatch"))
    {
        return Some("repo_head_mismatch".to_string());
    }
    Some("fresh".to_string())
}

pub(crate) fn watch_lifecycle_error(preflight: &DbLifecycleSurfacePreflight) -> String {
    let db_path = &preflight.exact_db_path_checked;
    let blockers = if preflight.blockers.is_empty() {
        "unknown lifecycle blocker".to_string()
    } else {
        preflight.blockers.join("; ")
    };
    if preflight.artifact_freshness.as_deref() == Some("missing")
        || preflight.passport_status == "missing"
    {
        return format!(
            "CodeGraph index does not exist or is missing a valid passport at {db_path}; run `codegraph-mcp index . --db {}` first. Long-running watch does not auto-index by default.",
            db_path
        );
    }
    format!("CodeGraph DB is not safe for watch updates at {db_path}: {blockers}")
}

pub(crate) fn enqueue_event_paths(debouncer: &mut WatchDebouncer, event: Event, now: Instant) {
    if matches!(event.kind, EventKind::Access(_)) {
        return;
    }

    for path in event.paths {
        debouncer.push(path, now);
    }
}

pub(crate) fn log_watch_summary(summary: &IncrementalIndexSummary) {
    eprintln!(
        "codegraph watch: seen={} walked={} metadata_unchanged={} read={} hashed={} parsed={} indexed={} deleted={} renamed={} skipped={} ignored={} entities={} edges={} signatures={} adjacency_edges={}",
        summary.files_seen,
        summary.files_walked,
        summary.files_metadata_unchanged,
        summary.files_read,
        summary.files_hashed,
        summary.files_parsed,
        summary.files_indexed,
        summary.files_deleted,
        summary.files_renamed,
        summary.files_skipped,
        summary.files_ignored,
        summary.entities,
        summary.edges,
        summary.binary_signatures_updated,
        summary.adjacency_edges,
    );
}

pub(crate) fn serve_ui(repo_path: &Path, host: &str, port: u16) -> Result<(), IndexError> {
    if !is_local_host(host) {
        return Err(IndexError::Message(format!(
            "serve-ui is local-only by default; refusing host {host}"
        )));
    }
    if !repo_path.exists() {
        return Err(IndexError::RepoNotFound(repo_path.to_path_buf()));
    }

    let repo_root = fs::canonicalize(repo_path)?;
    let listener = TcpListener::bind(format!("{host}:{port}"))?;
    let local_addr = listener.local_addr()?;
    eprintln!("codegraph ui: http://{local_addr}");
    serve_ui_loop(repo_root, listener, None)
}

pub(crate) fn serve_ui_loop(
    repo_root: PathBuf,
    listener: TcpListener,
    shutdown: Option<mpsc::Receiver<()>>,
) -> Result<(), IndexError> {
    listener.set_nonblocking(true)?;
    loop {
        if let Some(receiver) = &shutdown {
            match receiver.try_recv() {
                Ok(()) | Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        match listener.accept() {
            Ok((stream, _)) => {
                if let Err(error) = handle_ui_stream(&repo_root, stream) {
                    eprintln!("codegraph ui: request failed: {error}");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(IndexError::Io(error)),
        }
    }
}

pub(crate) fn handle_ui_stream(repo_root: &Path, mut stream: TcpStream) -> Result<(), IndexError> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first_line = String::new();
    reader.read_line(&mut first_line)?;
    if first_line.trim().is_empty() {
        return Ok(());
    }

    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    let response = route_ui_request(repo_root, method, target);
    write_http_response(&mut stream, response)?;
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct UiResponse {
    pub(crate) status: u16,
    pub(crate) content_type: &'static str,
    pub(crate) body: String,
}

pub(crate) fn route_ui_request(repo_root: &Path, method: &str, target: &str) -> UiResponse {
    if method != "GET" {
        return ui_json_error(405, "method_not_allowed", "serve-ui only supports GET");
    }

    let (path, query) = target
        .split_once('?')
        .map_or((target, ""), |(path, query)| (path, query));
    let params = parse_query_params(query);

    match path {
        "/" | "/index.html" => UiResponse {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: UI_INDEX_HTML.to_string(),
        },
        "/assets/app.js" => UiResponse {
            status: 200,
            content_type: "application/javascript; charset=utf-8",
            body: UI_APP_JS.to_string(),
        },
        "/assets/d3.v7.min.js" => UiResponse {
            status: 200,
            content_type: "application/javascript; charset=utf-8",
            body: UI_D3_JS.to_string(),
        },
        "/assets/styles.css" => UiResponse {
            status: 200,
            content_type: "text/css; charset=utf-8",
            body: UI_STYLES_CSS.to_string(),
        },
        "/api/status" => ui_json(ui_status(repo_root)),
        "/api/path-graph" => ui_json(ui_path_graph(repo_root, &params)),
        "/api/symbol-search" => ui_json(ui_symbol_search(repo_root, &params)),
        "/api/source-span" => ui_json(ui_source_span(repo_root, &params)),
        "/api/path-compare" => ui_json(ui_path_compare(repo_root, &params)),
        "/api/unresolved-calls" => ui_json(ui_unresolved_calls(repo_root, &params)),
        "/api/impact" => ui_json(ui_impact(repo_root, &params)),
        "/api/context-pack" => ui_json(ui_context_pack(repo_root, &params)),
        _ => ui_json_error(404, "not_found", "unknown Proof-Path UI route"),
    }
}

pub(crate) fn write_http_response(
    stream: &mut TcpStream,
    response: UiResponse,
) -> Result<(), IndexError> {
    let status_text = match response.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let body = response.body.as_bytes();
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:\r\nConnection: close\r\n\r\n",
        response.status,
        status_text,
        response.content_type,
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}

pub(crate) fn ui_json(result: Result<Value, String>) -> UiResponse {
    match result {
        Ok(value) => UiResponse {
            status: 200,
            content_type: "application/json; charset=utf-8",
            body: json_line(value),
        },
        Err(message) => ui_json_error(500, "ui_error", &message),
    }
}

pub(crate) fn ui_json_error(status: u16, error: &str, message: &str) -> UiResponse {
    UiResponse {
        status,
        content_type: "application/json; charset=utf-8",
        body: json_line(json!({
            "status": "error",
            "error": error,
            "message": message,
        })),
    }
}

pub(crate) fn ui_status(repo_root: &Path) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let languages = detect_tooling(repo_root)?;
    Ok(json!({
        "status": "ok",
        "phase": PHASE,
        "repo_root": path_string(repo_root),
        "schema_version": store.schema_version().map_err(|error| error.to_string())?,
        "files": store.count_files().map_err(|error| error.to_string())?,
        "entities": store.count_entities().map_err(|error| error.to_string())?,
        "edges": store.count_edges().map_err(|error| error.to_string())?,
        "languages": languages,
        "local_only": true,
    }))
}

pub(crate) fn ui_path_graph(
    repo_root: &Path,
    params: &BTreeMap<String, String>,
) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let engine = query_engine(&store)?;
    let mode = parse_ui_graph_mode(params);
    let requested_relations = parse_relation_filter(params.get("relations").map(String::as_str))?;
    let relations = if requested_relations.is_empty() {
        mode_default_relations(&mode)
    } else {
        requested_relations
    };
    let node_cap = parse_ui_node_cap(params);
    let source = params
        .get("source")
        .map(String::as_str)
        .unwrap_or("")
        .trim();
    let target = params
        .get("target")
        .map(String::as_str)
        .unwrap_or("")
        .trim();

    let (paths, resolved_source, resolved_target) = if mode == "unresolved_calls" {
        (
            unresolved_call_paths(store.list_edges(1_000).map_err(|error| error.to_string())?),
            None,
            None,
        )
    } else if !source.is_empty() && !target.is_empty() {
        let resolved_source = resolve_symbol_or_literal(&store, source)?;
        let resolved_target = resolve_symbol_or_literal(&store, target)?;
        let relation_slice = if relations.is_empty() {
            RelationKind::ALL
        } else {
            relations.as_slice()
        };
        (
            engine.trace_path(
                &resolved_source,
                &resolved_target,
                relation_slice,
                default_query_limits(),
            ),
            Some(resolved_source),
            Some(resolved_target),
        )
    } else if !source.is_empty() {
        let resolved_source = resolve_symbol_or_literal(&store, source)?;
        let paths = match mode.as_str() {
            "neighborhood" => neighborhood_paths(
                store
                    .list_edges(UNBOUNDED_STORE_READ_LIMIT)
                    .map_err(|error| error.to_string())?,
                &resolved_source,
                &relations,
            ),
            _ => filtered_impact_paths(&engine, &resolved_source, &relations),
        };
        (paths, Some(resolved_source), None)
    } else {
        (
            one_step_paths(
                store.list_edges(500).map_err(|error| error.to_string())?,
                &relations,
            ),
            None,
            None,
        )
    };

    let graph = graph_json_from_paths(&store, &paths, node_cap)?;
    let source_spans = paths
        .iter()
        .flat_map(GraphPath::source_spans)
        .collect::<Vec<_>>();
    Ok(json!({
        "status": "ok",
        "source": source,
        "target": target,
        "resolved_source": resolved_source,
        "resolved_target": resolved_target,
        "filters": {
            "mode": mode,
            "relations": relations.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "node_cap": node_cap,
        },
        "graph": graph,
        "paths": engine.path_evidence_from_paths(&paths),
        "source_spans": source_spans,
        "examples": [
            "CALLS -> MUTATES",
            "EXPOSES -> AUTHORIZES -> CHECKS_ROLE",
            "PUBLISHES/EMITS -> CONSUMES/LISTENS_TO",
            "MIGRATES -> DB impact -> TESTS"
        ],
        "proof": "Path graph is built from local CodeGraph edges and PathEvidence.",
    }))
}

pub(crate) fn ui_symbol_search(
    repo_root: &Path,
    params: &BTreeMap<String, String>,
) -> Result<Value, String> {
    let query = params.get("query").map(String::as_str).unwrap_or("").trim();
    if query.is_empty() {
        return Err("missing query parameter".to_string());
    }
    query_symbols(repo_root, query, parse_ui_limit(params, 12, 50))
}

pub(crate) fn ui_source_span(
    repo_root: &Path,
    params: &BTreeMap<String, String>,
) -> Result<Value, String> {
    let file = params
        .get("file")
        .or_else(|| params.get("repo_relative_path"))
        .map(String::as_str)
        .unwrap_or("")
        .trim();
    if file.is_empty() {
        return Err("missing file query parameter".to_string());
    }
    let start = params
        .get("start")
        .or_else(|| params.get("start_line"))
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    let end = params
        .get("end")
        .or_else(|| params.get("end_line"))
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(start)
        .max(start);
    let span = SourceSpan::new(file, start, end);
    let path = repo_root.join(file);
    if !path.exists() {
        return Ok(json!({
            "status": "missing",
            "file": file,
            "span": span,
            "snippet": "",
            "unknown": "source file is not available on disk",
        }));
    }
    let source = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "ok",
        "file": file,
        "span": span,
        "snippet": source_snippet(&span, &source),
        "resource": format!("codegraph://source-span/{file}:{start}-{end}"),
        "proof": "Source preview is read from the local file by source span.",
    }))
}

pub(crate) fn ui_path_compare(
    repo_root: &Path,
    params: &BTreeMap<String, String>,
) -> Result<Value, String> {
    let mut left_params = params.clone();
    left_params.insert("mode".to_string(), "proof_path".to_string());
    let left = ui_path_graph(repo_root, &left_params)?;
    let mut right_params = params.clone();
    if let Some(compare_target) = params.get("compare_target") {
        right_params.insert("target".to_string(), compare_target.clone());
    }
    right_params.insert("mode".to_string(), "proof_path".to_string());
    let right = ui_path_graph(repo_root, &right_params)?;
    Ok(json!({
        "status": "ok",
        "left": left,
        "right": right,
        "proof": "Path comparison runs the same local proof-path endpoint twice with explicit targets.",
    }))
}

pub(crate) fn ui_unresolved_calls(
    repo_root: &Path,
    params: &BTreeMap<String, String>,
) -> Result<Value, String> {
    let requested_limit = parse_ui_limit(
        params,
        DEFAULT_UNRESOLVED_CALL_LIMIT,
        MAX_UNRESOLVED_CALL_LIMIT,
    );
    let include_snippets = params
        .get("include_snippets")
        .or_else(|| params.get("snippets"))
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"));
    query_unresolved_calls(
        repo_root,
        UnresolvedCallsOptions {
            requested_limit,
            limit: requested_limit,
            offset: params
                .get("offset")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0),
            include_snippets,
            surface_name: "ui.unresolved_calls".to_string(),
            ..UnresolvedCallsOptions::default()
        },
    )
}

pub(crate) fn ui_impact(
    repo_root: &Path,
    params: &BTreeMap<String, String>,
) -> Result<Value, String> {
    let Some(target) = params
        .get("target")
        .filter(|target| !target.trim().is_empty())
    else {
        return Err("missing target query parameter".to_string());
    };
    impact_value(repo_root, target)
}

pub(crate) fn ui_context_pack(
    repo_root: &Path,
    params: &BTreeMap<String, String>,
) -> Result<Value, String> {
    let task = params
        .get("task")
        .cloned()
        .unwrap_or_else(|| "Inspect proof paths".to_string());
    let mode = params
        .get("mode")
        .cloned()
        .unwrap_or_else(|| "impact".to_string());
    let token_budget = params
        .get("budget")
        .and_then(|budget| budget.parse::<usize>().ok())
        .unwrap_or(1_200);
    let store = open_existing_store(repo_root)?;
    let seeds = match params.get("seed").filter(|seed| !seed.trim().is_empty()) {
        Some(seed) => resolve_impact_seeds(&store, seed)?,
        None => store
            .list_entities(1)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|entity| entity.id)
            .collect::<Vec<_>>(),
    };
    let engine = query_engine(&store)?;
    let sources = load_sources(repo_root, &store)?;
    let packet = engine.context_pack(
        ContextPackRequest::new(task.clone(), mode.clone(), token_budget, seeds),
        &sources,
    );
    Ok(json!({
        "status": "ok",
        "task": task,
        "mode": mode,
        "packet": packet,
        "proof": "Context packet preview uses local graph/source evidence.",
    }))
}

pub(crate) fn filtered_impact_paths(
    engine: &ExactGraphQueryEngine,
    source: &str,
    relations: &[RelationKind],
) -> Vec<GraphPath> {
    let impact = engine.impact_analysis_core(source, default_query_limits());
    let mut paths = Vec::new();
    paths.extend(impact.callers);
    paths.extend(impact.callees);
    paths.extend(impact.reads);
    paths.extend(impact.writes);
    paths.extend(impact.mutations);
    paths.extend(impact.dataflow);
    paths.extend(impact.auth_paths);
    paths.extend(impact.event_flow);
    paths.extend(impact.tests);
    paths.extend(impact.migrations);
    if relations.is_empty() {
        paths
    } else {
        paths
            .into_iter()
            .filter(|path| {
                path.steps
                    .iter()
                    .any(|step| relations.contains(&step.edge.relation))
            })
            .collect()
    }
}

pub(crate) fn one_step_paths(edges: Vec<Edge>, relations: &[RelationKind]) -> Vec<GraphPath> {
    edges
        .into_iter()
        .filter(|edge| relations.is_empty() || relations.contains(&edge.relation))
        .map(|edge| {
            let source = edge.head_id.clone();
            let target = edge.tail_id.clone();
            GraphPath {
                source: source.clone(),
                target: target.clone(),
                steps: vec![TraversalStep {
                    edge,
                    direction: TraversalDirection::Forward,
                    from: source,
                    to: target,
                }],
                cost: 1.0,
                uncertainty: 0.0,
            }
        })
        .collect()
}

pub(crate) fn neighborhood_paths(
    edges: Vec<Edge>,
    source: &str,
    relations: &[RelationKind],
) -> Vec<GraphPath> {
    edges
        .into_iter()
        .filter(|edge| edge.head_id == source || edge.tail_id == source)
        .filter(|edge| relations.is_empty() || relations.contains(&edge.relation))
        .map(|edge| {
            let from = edge.head_id.clone();
            let to = edge.tail_id.clone();
            GraphPath {
                source: source.to_string(),
                target: if from == source {
                    to.clone()
                } else {
                    from.clone()
                },
                steps: vec![TraversalStep {
                    edge,
                    direction: TraversalDirection::Forward,
                    from,
                    to,
                }],
                cost: 1.0,
                uncertainty: 0.0,
            }
        })
        .collect()
}

pub(crate) fn unresolved_call_paths(edges: Vec<Edge>) -> Vec<GraphPath> {
    edges
        .into_iter()
        .filter(|edge| {
            edge.relation == RelationKind::Calls
                && (edge.exactness == Exactness::StaticHeuristic
                    || edge
                        .metadata
                        .get("resolution")
                        .and_then(Value::as_str)
                        .is_some_and(|resolution| resolution.contains("unresolved")))
        })
        .map(|edge| {
            let source = edge.head_id.clone();
            let target = edge.tail_id.clone();
            GraphPath {
                source: source.clone(),
                target: target.clone(),
                steps: vec![TraversalStep {
                    edge,
                    direction: TraversalDirection::Forward,
                    from: source,
                    to: target,
                }],
                cost: 1.0,
                uncertainty: 0.5,
            }
        })
        .collect()
}

pub(crate) fn graph_json_from_paths(
    store: &SqliteGraphStore,
    paths: &[GraphPath],
    node_cap: usize,
) -> Result<Value, String> {
    let mut node_ids = BTreeSet::new();
    let mut seen_edges = BTreeSet::new();
    let mut edges = Vec::new();
    let node_cap = node_cap.clamp(1, MAX_UI_NODE_CAP);
    let mut total_node_ids = BTreeSet::new();
    let mut total_steps = 0usize;
    for path in paths {
        total_node_ids.insert(path.source.clone());
        total_node_ids.insert(path.target.clone());
        insert_limited_node(&mut node_ids, &path.source, node_cap);
        insert_limited_node(&mut node_ids, &path.target, node_cap);
        for step in &path.steps {
            total_steps += 1;
            total_node_ids.insert(step.from.clone());
            total_node_ids.insert(step.to.clone());
            insert_limited_node(&mut node_ids, &step.from, node_cap);
            insert_limited_node(&mut node_ids, &step.to, node_cap);
            let edge_key = (step.edge.id.clone(), step.from.clone(), step.to.clone());
            if !seen_edges.insert(edge_key) {
                continue;
            }
            if !node_ids.contains(&step.from) || !node_ids.contains(&step.to) {
                continue;
            }
            edges.push(json!({
                "id": step.edge.id,
                "source": step.from,
                "target": step.to,
                "head_id": step.edge.head_id,
                "tail_id": step.edge.tail_id,
                "relation": step.edge.relation.to_string(),
                "exactness": step.edge.exactness.to_string(),
                "confidence": step.edge.confidence,
                "source_span": step.edge.source_span,
                "resource_links": source_span_resource_links(&step.edge.source_span),
                "provenance_edges": step.edge.provenance_edges,
                "metadata": step.edge.metadata,
            }));
        }
    }

    let mut nodes = Vec::new();
    for node_id in node_ids {
        let entity = store
            .get_entity(&node_id)
            .map_err(|error| error.to_string())?;
        nodes.push(match entity {
            Some(entity) => json!({
                "id": node_id,
                "label": entity.qualified_name,
                "kind": entity.kind.to_string(),
                "repo_relative_path": entity.repo_relative_path,
                "source_span": entity.source_span,
                "resource_links": entity.source_span.as_ref().map(source_span_resource_links).unwrap_or_default(),
                "confidence": entity.confidence,
            }),
            None => json!({
                "id": node_id,
                "label": compact_node_label(&node_id),
                "kind": "unknown",
            }),
        });
    }

    let mut relation_counts: BTreeMap<String, usize> = BTreeMap::new();
    for edge in &edges {
        if let Some(relation) = edge.get("relation").and_then(Value::as_str) {
            *relation_counts.entry(relation.to_string()).or_default() += 1;
        }
    }

    Ok(json!({
        "nodes": nodes,
        "edges": edges,
        "relation_counts": relation_counts,
        "layout": {
            "engine": "d3-layered-dag",
            "direction": "left-to-right",
            "edge_routing": "straight",
            "documented_alternative": "Cytoscape.js/ELK can replace this local D3 layered layout later without changing the graph JSON contract"
        },
        "style": {
            "exactness": exactness_style_legend(),
            "confidence": "opacity and stroke width increase with confidence"
        },
        "guardrails": {
            "visible_node_cap": node_cap,
            "truncated": total_node_ids.len() > nodes.len() || total_steps > edges.len(),
            "omitted_nodes": total_node_ids.len().saturating_sub(nodes.len()),
            "omitted_edges": total_steps.saturating_sub(edges.len()),
            "server_side_filtering": true,
            "expand_on_click": true,
            "truncation_warning": if total_node_ids.len() > nodes.len() || total_steps > edges.len() {
                "Large graph truncated; refine relation filters or expand selected nodes."
            } else {
                ""
            }
        }
    }))
}

pub(crate) fn insert_limited_node(nodes: &mut BTreeSet<String>, node_id: &str, cap: usize) {
    if nodes.len() < cap || nodes.contains(node_id) {
        nodes.insert(node_id.to_string());
    }
}

pub(crate) fn source_span_resource_links(span: &SourceSpan) -> Value {
    json!({
        "source_span": format!(
            "codegraph://source-span/{}:{}-{}",
            span.repo_relative_path, span.start_line, span.end_line
        ),
        "file": format!("codegraph://file/{}", span.repo_relative_path),
    })
}

pub(crate) fn exactness_style_legend() -> Value {
    json!({
        "exact": {"stroke": "#63d297", "line": "solid"},
        "compiler_verified": {"stroke": "#63d297", "line": "solid"},
        "lsp_verified": {"stroke": "#6ab8ff", "line": "solid"},
        "parser_verified": {"stroke": "#70b7ff", "line": "solid"},
        "static_heuristic": {"stroke": "#e0b85a", "line": "dashed"},
        "dynamic_trace": {"stroke": "#c891ff", "line": "solid"},
        "inferred": {"stroke": "#f08a8a", "line": "dotted"},
        "derived_from_verified_edges": {"stroke": "#89d6d3", "line": "double"}
    })
}

pub(crate) fn parse_ui_graph_mode(params: &BTreeMap<String, String>) -> String {
    match params
        .get("mode")
        .map(|mode| mode.trim().replace('-', "_").to_ascii_lowercase())
        .as_deref()
    {
        Some(
            "proof_path" | "neighborhood" | "impact" | "auth_security" | "event_flow"
            | "test_impact" | "unresolved_calls",
        ) => params
            .get("mode")
            .map(|mode| mode.trim().replace('-', "_").to_ascii_lowercase())
            .unwrap_or_else(|| "proof_path".to_string()),
        _ => "proof_path".to_string(),
    }
}

pub(crate) fn mode_default_relations(mode: &str) -> Vec<RelationKind> {
    match mode {
        "auth_security" => vec![
            RelationKind::Exposes,
            RelationKind::Calls,
            RelationKind::Authorizes,
            RelationKind::ChecksRole,
            RelationKind::ChecksPermission,
            RelationKind::Sanitizes,
            RelationKind::Validates,
            RelationKind::TrustBoundary,
            RelationKind::SourceOfTaint,
            RelationKind::SinksTo,
        ],
        "event_flow" => vec![
            RelationKind::Publishes,
            RelationKind::Emits,
            RelationKind::Consumes,
            RelationKind::ListensTo,
            RelationKind::SubscribesTo,
            RelationKind::Handles,
            RelationKind::Spawns,
            RelationKind::Awaits,
            RelationKind::Calls,
            RelationKind::Mutates,
        ],
        "test_impact" => vec![
            RelationKind::Tests,
            RelationKind::Asserts,
            RelationKind::Mocks,
            RelationKind::Stubs,
            RelationKind::Covers,
            RelationKind::FixturesFor,
            RelationKind::Calls,
        ],
        "impact" => vec![
            RelationKind::Calls,
            RelationKind::Reads,
            RelationKind::Writes,
            RelationKind::Mutates,
            RelationKind::FlowsTo,
            RelationKind::Migrates,
            RelationKind::Tests,
        ],
        "unresolved_calls" => vec![RelationKind::Calls],
        _ => Vec::new(),
    }
}

pub(crate) fn parse_ui_node_cap(params: &BTreeMap<String, String>) -> usize {
    params
        .get("node_cap")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_UI_NODE_CAP)
        .clamp(1, MAX_UI_NODE_CAP)
}

pub(crate) fn parse_ui_limit(
    params: &BTreeMap<String, String>,
    default: usize,
    max: usize,
) -> usize {
    params
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(1, max)
}

pub(crate) fn compact_node_label(id: &str) -> String {
    id.rsplit(['#', '/', ':'])
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(id)
        .to_string()
}

pub(crate) fn parse_relation_filter(raw: Option<&str>) -> Result<Vec<RelationKind>, String> {
    let Some(raw) = raw.filter(|raw| !raw.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<RelationKind>()
                .map_err(|_| format!("unknown relation filter: {value}"))
        })
        .collect()
}

pub(crate) fn parse_query_params(raw: &str) -> BTreeMap<String, String> {
    raw.split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            Some((percent_decode(key)?, percent_decode(value)?))
        })
        .collect()
}

pub(crate) fn percent_decode(value: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut chars = value.as_bytes().iter().copied();
    while let Some(byte) = chars.next() {
        match byte {
            b'+' => bytes.push(b' '),
            b'%' => {
                let high = chars.next()?;
                let low = chars.next()?;
                let hex = [high, low];
                let text = std::str::from_utf8(&hex).ok()?;
                bytes.push(u8::from_str_radix(text, 16).ok()?);
            }
            other => bytes.push(other),
        }
    }
    String::from_utf8(bytes).ok()
}

pub(crate) fn is_local_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

#[derive(Debug)]
pub(crate) struct WatchDebouncer {
    delay: Duration,
    pending: BTreeMap<PathBuf, Instant>,
    events_seen: usize,
    coalesced_count: usize,
}

impl WatchDebouncer {
    pub(crate) fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending: BTreeMap::new(),
            events_seen: 0,
            coalesced_count: 0,
        }
    }

    pub(crate) fn push(&mut self, path: PathBuf, now: Instant) {
        self.events_seen += 1;
        if self.pending.insert(path, now).is_some() {
            self.coalesced_count += 1;
        }
    }

    pub(crate) fn ready(&mut self, now: Instant) -> Vec<PathBuf> {
        let ready_paths = self
            .pending
            .iter()
            .filter_map(|(path, last_event_at)| {
                let elapsed = now
                    .checked_duration_since(*last_event_at)
                    .unwrap_or_default();
                (elapsed >= self.delay).then(|| path.clone())
            })
            .collect::<Vec<_>>();

        for path in &ready_paths {
            self.pending.remove(path);
        }

        ready_paths
    }

    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub(crate) fn pending_paths(&self) -> Vec<PathBuf> {
        self.pending.keys().cloned().collect()
    }

    pub(crate) fn coalesced_count(&self) -> usize {
        self.coalesced_count
    }

    pub(crate) fn events_seen(&self) -> usize {
        self.events_seen
    }
}
