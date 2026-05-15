//! Local MCP server for CodeGraph.
//!
//! Phase 30 exposes read-mostly, evidence-oriented CodeGraph tools over a
//! minimal stdio JSON-RPC MCP surface. Local index update tools are limited to
//! creating/updating `.codegraph/codegraph.sqlite`; no delete or destructive
//! repository operations are exposed.

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    str::FromStr,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use codegraph_core::{Edge, Entity, FileRecord, RelationKind, SourceSpan};
use codegraph_index::{
    default_db_path, index_repo_to_db_with_options, inspect_db_lifecycle_preflight,
    update_changed_files_to_db, DbLifecyclePreflight, IndexOptions, IndexScopeOptions,
    UNBOUNDED_STORE_READ_LIMIT,
};
use codegraph_parser::language_frontends;
use codegraph_query::{
    ExactGraphQueryEngine, GraphPath, QueryLimits, RetrievalDocument, RetrievalFunnel,
    RetrievalFunnelConfig, RetrievalFunnelRequest, RetrievalTraceStage,
};
use codegraph_store::{GraphStore, SqliteGraphStore, TextSearchHit, TextSearchKind};
use codegraph_trace::{TraceConfig, TraceLogger};
use serde_json::{json, Map, Value};

#[cfg(test)]
use codegraph_index::scope_policy_hash;

pub const SERVER_NAME: &str = "codegraph-mcp";
pub const PHASE: &str = "30";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const DEFAULT_RESULT_LIMIT: usize = 20;
const DEFAULT_GRAPH_EDGE_LIMIT: usize = 100_000;

const MCP_RESOURCE_URIS: &[&str] = &[
    "codegraph://status",
    "codegraph://schema",
    "codegraph://languages",
    "codegraph://bench/latest",
    "codegraph://context/<id>",
];

const MCP_PROMPT_NAMES: &[&str] = &[
    "impact-analysis",
    "trace-dataflow",
    "auth-review",
    "test-impact",
    "refactor-safety",
];

const TOOL_NAMES: &[&str] = &[
    "codegraph.search",
    "codegraph.analyze",
    "codegraph.plan_context",
    "codegraph.explain_missing",
    "codegraph.status",
    "codegraph.index_repo",
    "codegraph.update_changed_files",
    "codegraph.search_symbols",
    "codegraph.search_text",
    "codegraph.search_semantic",
    "codegraph.context_pack",
    "codegraph.trace_path",
    "codegraph.impact_analysis",
    "codegraph.find_callers",
    "codegraph.find_callees",
    "codegraph.find_reads",
    "codegraph.find_writes",
    "codegraph.find_mutations",
    "codegraph.find_dataflow",
    "codegraph.find_auth_paths",
    "codegraph.find_event_flow",
    "codegraph.find_tests",
    "codegraph.find_migrations",
    "codegraph.explain_edge",
    "codegraph.explain_path",
];

#[derive(Debug)]
pub enum McpServerError {
    Io(io::Error),
    Json(serde_json::Error),
    Store(codegraph_store::StoreError),
    Parse(codegraph_parser::ParseError),
    Message(String),
}

impl fmt::Display for McpServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Json(error) => write!(formatter, "JSON error: {error}"),
            Self::Store(error) => write!(formatter, "store error: {error}"),
            Self::Parse(error) => write!(formatter, "parse error: {error}"),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl Error for McpServerError {}

impl From<io::Error> for McpServerError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for McpServerError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<codegraph_store::StoreError> for McpServerError {
    fn from(error: codegraph_store::StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<codegraph_parser::ParseError> for McpServerError {
    fn from(error: codegraph_parser::ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<codegraph_index::IndexError> for McpServerError {
    fn from(error: codegraph_index::IndexError) -> Self {
        Self::Message(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallError {
    pub code: String,
    pub message: String,
}

impl ToolCallError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "status": "error",
            "error": self.code,
            "message": self.message,
        })
    }
}

impl fmt::Display for ToolCallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for ToolCallError {}

impl From<McpServerError> for ToolCallError {
    fn from(error: McpServerError) -> Self {
        Self::new("server_error", error.to_string())
    }
}

impl From<codegraph_index::IndexError> for ToolCallError {
    fn from(error: codegraph_index::IndexError) -> Self {
        Self::new("index_error", error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerConfig {
    pub repo_root: PathBuf,
    pub db_path: PathBuf,
    pub max_graph_edges: usize,
    pub trace_enabled: bool,
    pub trace_run_id: String,
    pub trace_task_id: String,
    pub trace_root: PathBuf,
}

impl McpServerConfig {
    pub fn for_repo(repo_root: impl Into<PathBuf>) -> Self {
        let repo_root = repo_root.into();
        let trace = TraceConfig::for_repo(&repo_root);
        Self {
            db_path: default_db_path(&repo_root),
            repo_root,
            max_graph_edges: DEFAULT_GRAPH_EDGE_LIMIT,
            trace_enabled: true,
            trace_run_id: trace.run_id,
            trace_task_id: trace.task_id,
            trace_root: trace.trace_root,
        }
    }

    pub fn with_db_path(mut self, db_path: impl Into<PathBuf>) -> Self {
        self.db_path = db_path.into();
        self
    }

    pub fn with_trace_root(mut self, trace_root: impl Into<PathBuf>) -> Self {
        self.trace_root = trace_root.into();
        self
    }

    pub fn with_trace_run_id(mut self, trace_run_id: impl Into<String>) -> Self {
        self.trace_run_id = trace_run_id.into();
        self
    }

    pub fn with_trace_task_id(mut self, trace_task_id: impl Into<String>) -> Self {
        self.trace_task_id = trace_task_id.into();
        self
    }

    pub fn without_trace(mut self) -> Self {
        self.trace_enabled = false;
        self
    }
}

impl Default for McpServerConfig {
    fn default() -> Self {
        let repo_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::for_repo(repo_root)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoContext {
    repo_root: PathBuf,
    db_path: PathBuf,
    indexed: bool,
    detected_dbs: Vec<String>,
}

impl RepoContext {
    fn to_json(&self) -> Value {
        json!({
            "repo_root": path_string(&self.repo_root),
            "db_path": path_string(&self.db_path),
            "indexed": self.indexed,
            "detected_indexed_dbs": self.detected_dbs,
            "index_command": format!("codegraph-mcp --db \"{}\" index \"{}\"", self.db_path.display(), self.repo_root.display()),
            "index_tool": {
                "name": "codegraph.index_repo",
                "arguments": {
                    "repo": path_string(&self.repo_root),
                    "db_path": path_string(&self.db_path),
                }
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct McpServer {
    config: McpServerConfig,
}

impl McpServer {
    pub fn new(config: McpServerConfig) -> Self {
        let server = Self { config };
        server.trace_run_start();
        server
    }

    pub fn config(&self) -> &McpServerConfig {
        &self.config
    }

    pub fn tool_definitions(&self) -> Vec<Value> {
        TOOL_NAMES
            .iter()
            .map(|name| tool_definition(name))
            .collect()
    }

    pub fn resource_definitions(&self) -> Vec<Value> {
        MCP_RESOURCE_URIS
            .iter()
            .map(|uri| resource_definition(uri))
            .collect()
    }

    pub fn prompt_definitions(&self) -> Vec<Value> {
        MCP_PROMPT_NAMES
            .iter()
            .map(|name| prompt_definition(name))
            .collect()
    }

    pub fn call_tool(&self, name: &str, arguments: &Value) -> Result<Value, ToolCallError> {
        let trace_id = trace_id_for(name);
        let repo_root = trace_repo_root(arguments, &self.config.repo_root);
        self.trace_mcp_request(&repo_root, &trace_id, name, arguments);
        let start = Instant::now();
        let result = self.call_tool_inner(name, arguments);
        let latency_ms = start.elapsed().as_millis();
        match &result {
            Ok(value) => {
                self.trace_mcp_response(&repo_root, &trace_id, name, "ok", latency_ms, value, None);
                if name == "codegraph.context_pack" {
                    self.trace_context_pack_used(
                        &repo_root, &trace_id, "ok", latency_ms, arguments, value, None,
                    );
                }
            }
            Err(error) => {
                let value = error.to_json();
                self.trace_mcp_response(
                    &repo_root,
                    &trace_id,
                    name,
                    "error",
                    latency_ms,
                    &value,
                    Some(&error.message),
                );
                if name == "codegraph.context_pack" {
                    self.trace_context_pack_used(
                        &repo_root,
                        &trace_id,
                        "error",
                        latency_ms,
                        arguments,
                        &value,
                        Some(&error.message),
                    );
                }
            }
        }
        result
    }

    fn call_tool_inner(&self, name: &str, arguments: &Value) -> Result<Value, ToolCallError> {
        let args = object_arguments(arguments)?;
        match name {
            "codegraph.search" => self.search(args),
            "codegraph.analyze" => self.analyze(args),
            "codegraph.plan_context" => self.plan_context(args),
            "codegraph.explain_missing" => self.explain_missing(args),
            "codegraph.status" => self.status(args),
            "codegraph.index_repo" => self.index_repo(args),
            "codegraph.update_changed_files" => self.update_changed_files(args),
            "codegraph.search_symbols" => self.search_symbols(args),
            "codegraph.search_text" => self.search_text(args),
            "codegraph.search_semantic" => self.search_semantic(args),
            "codegraph.context_pack" => self.context_pack(args),
            "codegraph.trace_path" => self.trace_path(args),
            "codegraph.impact_analysis" => self.impact_analysis(args),
            "codegraph.find_callers" => self.relation_query(args, "callers"),
            "codegraph.find_callees" => self.relation_query(args, "callees"),
            "codegraph.find_reads" => self.relation_query(args, "reads"),
            "codegraph.find_writes" => self.relation_query(args, "writes"),
            "codegraph.find_mutations" => self.relation_query(args, "mutations"),
            "codegraph.find_dataflow" => self.relation_query(args, "dataflow"),
            "codegraph.find_auth_paths" => self.relation_query(args, "auth_paths"),
            "codegraph.find_event_flow" => self.relation_query(args, "event_flow"),
            "codegraph.find_tests" => self.relation_query(args, "tests"),
            "codegraph.find_migrations" => self.relation_query(args, "migrations"),
            "codegraph.explain_edge" => self.explain_edge(args),
            "codegraph.explain_path" => self.explain_path(args),
            other => Err(ToolCallError::new(
                "unknown_tool",
                format!("unknown CodeGraph MCP tool: {other}"),
            )),
        }
    }

    pub fn handle_jsonrpc(&self, message: &Value) -> Option<Value> {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let trace_id = trace_id_for(method);
        let repo_root = self.config.repo_root.clone();
        self.trace_mcp_request(&repo_root, &trace_id, method, message);
        let start = Instant::now();
        let response = self.handle_jsonrpc_inner(message);
        if let Some(value) = &response {
            let status = if value.get("error").is_some() {
                "error"
            } else {
                "ok"
            };
            self.trace_mcp_response(
                &repo_root,
                &trace_id,
                method,
                status,
                start.elapsed().as_millis(),
                value,
                value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str),
            );
        }
        response
    }

    fn handle_jsonrpc_inner(&self, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Some(jsonrpc_error(
                id,
                -32600,
                "invalid_request",
                "missing method",
            ));
        };

        if method.starts_with("notifications/") {
            return None;
        }

        match method {
            "initialize" => Some(jsonrpc_result(
                id,
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {
                        "tools": {},
                        "resources": {},
                        "prompts": {}
                    },
                    "serverInfo": {
                        "name": SERVER_NAME,
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )),
            "ping" => Some(jsonrpc_result(id, json!({}))),
            "tools/list" => Some(jsonrpc_result(
                id,
                json!({
                    "tools": self.tool_definitions()
                }),
            )),
            "tools/call" => Some(self.handle_tool_call(id, message.get("params"))),
            "resources/list" => Some(jsonrpc_result(
                id,
                json!({
                    "resources": self.resource_definitions()
                }),
            )),
            "resources/read" => Some(self.handle_resource_read(id, message.get("params"))),
            "prompts/list" => Some(jsonrpc_result(
                id,
                json!({
                    "prompts": self.prompt_definitions()
                }),
            )),
            "prompts/get" => Some(self.handle_prompt_get(id, message.get("params"))),
            other => Some(jsonrpc_error(
                id,
                -32601,
                "method_not_found",
                &format!("unsupported MCP method: {other}"),
            )),
        }
    }

    fn handle_resource_read(&self, id: Value, params: Option<&Value>) -> Value {
        let Some(params) = params.and_then(Value::as_object) else {
            return jsonrpc_error(
                id,
                -32602,
                "invalid_params",
                "resources/read params must be object",
            );
        };
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return jsonrpc_error(id, -32602, "invalid_params", "resources/read requires uri");
        };
        match self.read_resource(uri, params) {
            Ok(value) => jsonrpc_result(id, mcp_resource_result(uri, value)),
            Err(error) => jsonrpc_error(id, -32602, &error.code, &error.message),
        }
    }

    fn handle_prompt_get(&self, id: Value, params: Option<&Value>) -> Value {
        let Some(params) = params.and_then(Value::as_object) else {
            return jsonrpc_error(
                id,
                -32602,
                "invalid_params",
                "prompts/get params must be object",
            );
        };
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return jsonrpc_error(id, -32602, "invalid_params", "prompts/get requires name");
        };
        match prompt_template(name) {
            Some(value) => jsonrpc_result(id, value),
            None => jsonrpc_error(
                id,
                -32602,
                "unknown_prompt",
                &format!("unknown CodeGraph prompt template: {name}"),
            ),
        }
    }

    fn handle_tool_call(&self, id: Value, params: Option<&Value>) -> Value {
        let Some(params) = params.and_then(Value::as_object) else {
            return jsonrpc_error(
                id,
                -32602,
                "invalid_params",
                "tools/call params must be object",
            );
        };
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return jsonrpc_error(id, -32602, "invalid_params", "tools/call requires name");
        };
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        match self.call_tool(name, &arguments) {
            Ok(value) => jsonrpc_result(id, mcp_tool_result(value, false)),
            Err(error) => jsonrpc_result(id, mcp_tool_result(error.to_json(), true)),
        }
    }

    fn trace_logger(&self, repo_root: &Path) -> Option<TraceLogger> {
        if !self.config.trace_enabled {
            return None;
        }
        let config = TraceConfig::for_repo(repo_root)
            .with_run_id(self.config.trace_run_id.clone())
            .with_task_id(self.config.trace_task_id.clone())
            .with_trace_root(self.config.trace_root.clone());
        TraceLogger::new(config).ok()
    }

    fn trace_run_start(&self) {
        if let Some(logger) = self.trace_logger(&self.config.repo_root) {
            let _ = logger.run_start();
        }
    }

    fn trace_run_end(&self, status: &str, error: Option<&str>) {
        if let Some(logger) = self.trace_logger(&self.config.repo_root) {
            let _ = logger.run_end(status, error);
        }
    }

    fn trace_mcp_request(&self, repo_root: &Path, trace_id: &str, tool: &str, input: &Value) {
        if let Some(logger) = self.trace_logger(repo_root) {
            let _ = logger.mcp_request(trace_id, tool, input);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn trace_mcp_response(
        &self,
        repo_root: &Path,
        trace_id: &str,
        tool: &str,
        status: &str,
        latency_ms: u128,
        output: &Value,
        error: Option<&str>,
    ) {
        if let Some(logger) = self.trace_logger(repo_root) {
            let _ = logger.mcp_response(trace_id, tool, status, latency_ms, output, error);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn trace_context_pack_used(
        &self,
        repo_root: &Path,
        trace_id: &str,
        status: &str,
        latency_ms: u128,
        input: &Value,
        output: &Value,
        error: Option<&str>,
    ) {
        if let Some(logger) = self.trace_logger(repo_root) {
            let _ = logger.context_pack_used(trace_id, status, input, output, latency_ms, error);
        }
    }

    fn search(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let query = required_string(args, "query")?;
        let limit = optional_limit(args)?;
        let offset = optional_offset(args)?;
        let probe_limit = limit.saturating_add(offset).saturating_add(1);
        let mode = response_mode(args)?;
        let context = self.context_discovery(args)?;
        if !context.indexed {
            return Ok(not_indexed_response("search", &context));
        }

        let mut inner_args = args.clone();
        inner_args.insert("offset".to_string(), json!(0));
        inner_args.insert("limit".to_string(), json!(probe_limit));
        let symbol = self.search_symbols(&inner_args)?;
        let text = self.search_text(&inner_args)?;
        let db_lifecycle_read = symbol
            .get("db_lifecycle_read")
            .cloned()
            .or_else(|| text.get("db_lifecycle_read").cloned())
            .unwrap_or(Value::Null);
        let mut combined = Vec::new();
        for hit in symbol["hits"].as_array().cloned().unwrap_or_default() {
            combined.push(json!({
                "channel": "symbol",
                "proof_quality": {
                    "exactness": hit.pointer("/entity/exactness").cloned().unwrap_or_else(|| json!("unknown")),
                    "confidence": hit.pointer("/entity/confidence").cloned().unwrap_or(Value::Null),
                    "source_span": hit.pointer("/entity/source_span").cloned().unwrap_or(Value::Null),
                    "heuristic": hit.pointer("/entity/heuristic").cloned().unwrap_or(Value::Bool(false))
                },
                "hit": hit,
            }));
        }
        for hit in text["hits"].as_array().cloned().unwrap_or_default() {
            combined.push(json!({
                "channel": "text",
                "proof_quality": {
                    "exactness": "textual_exact_match",
                    "confidence": "unknown",
                    "source_span": text_hit_source_span(&hit),
                    "heuristic": false
                },
                "hit": hit,
            }));
        }
        let (hits, pagination) = paginate_values(combined, offset, limit);

        Ok(json!({
            "status": "ok",
            "tool": "codegraph.search",
            "query": query,
            "mode": mode,
            "repo_context": context.to_json(),
            "db_lifecycle_read": db_lifecycle_read,
            "recommended_next": [
                "Use codegraph.analyze on the best entity id.",
                "Use codegraph.plan_context before editing.",
                "Use codegraph.update_changed_files after edits."
            ],
            "hits": hits,
            "pagination": pagination,
            "workflow": recommended_workflow(),
            "resource_links": result_resource_links(),
            "proof": "LLM-friendly search combines exact symbol lookup and bounded source text evidence; no semantic claim is returned without proof labels.",
        }))
    }

    fn analyze(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let limit = optional_limit(args)?;
        let offset = optional_offset(args)?;
        let mode = response_mode(args)?;
        let context = self.context_discovery(args)?;
        if !context.indexed {
            return Ok(not_indexed_response("analyze", &context));
        }
        let entity_id = if let Some(entity_id) =
            optional_string(args, "entity_id").or_else(|| optional_string(args, "id"))
        {
            entity_id
        } else {
            let query = required_string_alias(args, &["query", "symbol"])?;
            self.resolve_entity_id(args, &query)?
        };
        let analysis = optional_string(args, "analysis")
            .unwrap_or_else(|| "impact".to_string())
            .replace('-', "_")
            .to_ascii_lowercase();

        let (store, preflight) = self.open_store_with_preflight(args)?;
        let engine = self.query_engine(&store, args)?;
        let limits = query_limits(args)?;
        let paths = match analysis.as_str() {
            "callers" => engine.find_callers(&entity_id, limits),
            "callees" => engine.find_callees(&entity_id, limits),
            "dataflow" => engine.find_dataflow(&entity_id, limits),
            "tests" => engine.find_tests(&entity_id, limits),
            "auth" | "auth_paths" => engine.find_auth_paths(&entity_id, limits),
            "event_flow" => engine.find_event_flow(&entity_id, limits),
            "migrations" => engine.find_migrations(&entity_id, limits),
            "impact" => {
                let impact = engine.impact_analysis_core(&entity_id, limits);
                [
                    impact.callers,
                    impact.callees,
                    impact.reads,
                    impact.writes,
                    impact.mutations,
                    impact.dataflow,
                    impact.auth_paths,
                    impact.event_flow,
                    impact.tests,
                    impact.migrations,
                ]
                .into_iter()
                .flatten()
                .collect()
            }
            other => {
                return Err(ToolCallError::new(
                    "invalid_input",
                    format!("analysis must be impact, callers, callees, dataflow, auth_paths, event_flow, tests, or migrations; got {other}"),
                ));
            }
        };
        let evidence = engine.path_evidence_from_paths(&paths);
        let path_values = serde_json::to_value(evidence)
            .ok()
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        let (paths_json, pagination) = paginate_values(path_values, offset, limit);

        Ok(json!({
            "status": "ok",
            "tool": "codegraph.analyze",
            "entity_id": entity_id,
            "analysis": analysis,
            "mode": mode,
            "repo_context": context.to_json(),
            "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
            "paths": paths_json,
            "pagination": pagination,
            "explain_missing": if paths.is_empty() {
                json!({
                    "category": "symbol_found_but_no_matching_relation",
                    "reason": "The entity exists, but no matching high-level analysis paths were found within the requested bounds.",
                    "bounds": {
                        "max_depth": limits.max_depth,
                        "max_paths": limits.max_paths,
                        "max_edges_visited": limits.max_edges_visited,
                    }
                })
            } else {
                json!(null)
            },
            "workflow": recommended_workflow(),
            "proof": "Analysis is a high-level wrapper over exact graph traversals and preserves path exactness, confidence, and source spans.",
        }))
    }

    fn plan_context(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let context = self.context_discovery(args)?;
        if !context.indexed {
            return Ok(not_indexed_response("plan_context", &context));
        }
        let mut request = args.clone();
        if !request.contains_key("task") {
            if let Some(query) =
                optional_string(args, "query").or_else(|| optional_string(args, "symbol"))
            {
                request.insert("task".to_string(), Value::String(query));
            }
        }
        let pack = self.context_pack(&request)?;
        Ok(json!({
            "status": "ok",
            "tool": "codegraph.plan_context",
            "repo_context": context.to_json(),
            "db_lifecycle_read": pack["db_lifecycle_read"].clone(),
            "packet": pack["packet"].clone(),
            "workflow": recommended_workflow(),
            "recommended_next": [
                "Use the packet snippets and verified paths for planning.",
                "Edit only after reviewing exactness and source spans.",
                "Call codegraph.update_changed_files after edits."
            ],
            "proof": "Plan context uses the same Stage 0-4 runtime funnel as codegraph.context_pack.",
        }))
    }

    fn explain_missing(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let context = self.context_discovery(args)?;
        if !context.indexed {
            return Ok(not_indexed_response("explain_missing", &context));
        }
        if let Some(language) = optional_string(args, "language") {
            let relations = optional_relations_or_single(args)?;
            if let Some(frontend) = language_frontends()
                .iter()
                .find(|frontend| frontend.language_id == language)
            {
                let unsupported = relations
                    .iter()
                    .filter(|relation| !frontend.supported_relation_kinds.contains(relation))
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                if !unsupported.is_empty() {
                    return Ok(json!({
                        "status": "ok",
                        "category": "relation_unsupported_for_language",
                        "language": language,
                        "unsupported_relations": unsupported,
                        "resolver": resolver_status_for_language(frontend.language_id),
                        "repo_context": context.to_json(),
                        "proof": "Unsupported relation category is derived from the Rust parser frontend registry.",
                    }));
                }
            }
        }
        let store = self.open_store(args)?;
        if let Some(symbol) =
            optional_string(args, "symbol").or_else(|| optional_string(args, "query"))
        {
            if store
                .find_entities_by_exact_symbol(&symbol)
                .map_err(mcp_store_error)?
                .is_empty()
            {
                return Ok(json!({
                    "status": "ok",
                    "category": "no_symbol_found",
                    "symbol": symbol,
                    "repo_context": context.to_json(),
                    "suggested_next": "Run codegraph.search with a broader query or re-index if the file was recently added.",
                    "proof": "No exact symbol/entity match exists in the local graph.",
                }));
            }
        }
        if args.contains_key("source") && args.contains_key("target") {
            let source = required_string(args, "source")?;
            let target = required_string(args, "target")?;
            let relations = optional_relations_or_single(args)?;
            let limits = query_limits(args)?;
            let explanation = explain_missing_path(&store, &source, &target, &relations, limits)?;
            return Ok(json!({
                "status": "ok",
                "repo_context": context.to_json(),
                "explanation": explanation,
                "category": explanation["category"].clone(),
                "proof": "Missing-path category is derived from symbol existence, relation support, resolver availability, and traversal bounds.",
            }));
        }
        Ok(json!({
            "status": "ok",
            "category": "unknown",
            "reason": "Provide symbol/query or source+target to classify the missing evidence.",
            "repo_context": context.to_json(),
            "proof": "Missing data is reported as unknown instead of guessed.",
        }))
    }

    fn status(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let context = self.context_discovery(args)?;
        let repo_root = context.repo_root.clone();
        let db_path = context.db_path.clone();
        if !context.indexed {
            return Ok(json!({
                "status": "missing",
                "problem": "index_required",
                "db_problem": "db_missing",
                "safe_to_query": false,
                "server": SERVER_NAME,
                "phase": PHASE,
                "repo_root": path_string(&repo_root),
                "db_path": path_string(&db_path),
                "repo_context": context.to_json(),
                "blockers": [format!("db_missing: {}", db_path.display())],
                "warnings": [],
                "suggested_next": "Call codegraph.index_repo with the shown repo/db_path before search or analysis.",
                "read_mostly": true,
                "workflow": "single-agent-only",
            }));
        }

        let explicit_scope = mcp_explicit_scope_policy(args)?;
        let preflight = inspect_db_lifecycle_preflight(&repo_root, &db_path, explicit_scope)
            .map_err(ToolCallError::from)?;
        if !preflight.safe {
            let problem = mcp_db_problem_kind(&preflight);
            return Ok(json!({
                "status": "db_problem",
                "problem": problem,
                "db_problem": problem,
                "safe_to_query": false,
                "server": SERVER_NAME,
                "phase": PHASE,
                "repo_root": path_string(&repo_root),
                "db_path": path_string(&db_path),
                "repo_context": context.to_json(),
                "db_health": preflight.db_health.clone(),
                "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
                "passport_summary": mcp_passport_summary_json(&preflight),
                "scope_source": preflight.scope_source.clone(),
                "blockers": preflight.blockers.clone(),
                "warnings": preflight.warnings.clone(),
                "suggested_next": "Call codegraph.index_repo to rebuild the unsafe DB before search or analysis.",
                "read_mostly": true,
                "workflow": "single-agent-only",
            }));
        }

        let store = SqliteGraphStore::open(&db_path).map_err(mcp_store_error)?;
        Ok(json!({
            "status": "ok",
            "safe_to_query": true,
            "server": SERVER_NAME,
            "phase": PHASE,
            "repo_root": path_string(&repo_root),
            "db_path": path_string(&db_path),
            "repo_context": context.to_json(),
            "db_health": preflight.db_health.clone(),
            "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
            "passport_summary": mcp_passport_summary_json(&preflight),
            "scope_source": preflight.scope_source.clone(),
            "blockers": preflight.blockers.clone(),
            "warnings": preflight.warnings.clone(),
            "schema_version": store.schema_version().map_err(mcp_store_error)?,
            "files": store.count_files().map_err(mcp_store_error)?,
            "entities": store.count_entities().map_err(mcp_store_error)?,
            "edges": store.count_edges().map_err(mcp_store_error)?,
            "read_mostly": true,
            "destructive_tools": false,
            "workflow": "single-agent-only",
        }))
    }

    fn index_repo(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let repo_root = self.repo_root(args)?;
        let db_path = self.db_path(args, &repo_root)?;
        let mut options = IndexOptions::default();
        options.db_lifecycle.explicit_db_path = args.contains_key("db_path");
        let summary = index_repo_to_db_with_options(&repo_root, &db_path, options)
            .map_err(ToolCallError::from)?;
        let mut value = serde_json::to_value(summary).map_err(|error| {
            ToolCallError::new(
                "serialization_failed",
                format!("could not encode summary: {error}"),
            )
        })?;
        if let Some(object) = value.as_object_mut() {
            object.insert("status".to_string(), json!("indexed"));
        }
        Ok(value)
    }

    fn update_changed_files(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let repo_root = self.repo_root(args)?;
        let db_path = self.db_path(args, &repo_root)?;
        let files = required_string_array(args, "files")?;
        let changed_paths = files.iter().map(PathBuf::from).collect::<Vec<_>>();
        let summary = update_changed_files_to_db(&repo_root, &changed_paths, &db_path)
            .map_err(ToolCallError::from)?;
        Ok(json!({
            "status": "updated",
            "summary": summary,
            "note": "Changed-file updates use the shared compact indexer path and prune stale facts before localized re-indexing.",
        }))
    }

    fn search_symbols(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let query = required_string(args, "query")?;
        let limit = optional_limit(args)?;
        let offset = optional_offset(args)?;
        let requested = limit.saturating_add(offset).saturating_add(1);
        let mode = response_mode(args)?;
        let (store, preflight) = self.open_store_with_preflight(args)?;
        let mut seen = BTreeSet::new();
        let mut hits = Vec::new();

        for entity in store
            .find_entities_by_exact_symbol(&query)
            .map_err(mcp_store_error)?
        {
            seen.insert(entity.id.clone());
            hits.push(json!({
                "match": "exact_symbol",
                "entity": entity_json(&entity),
            }));
        }

        for text_hit in store
            .search_text(&query, requested)
            .map_err(mcp_store_error)?
        {
            if text_hit.kind != TextSearchKind::Entity || seen.contains(&text_hit.id) {
                continue;
            }
            if let Some(entity) = store.get_entity(&text_hit.id).map_err(mcp_store_error)? {
                seen.insert(entity.id.clone());
                hits.push(json!({
                    "match": "fts_entity",
                    "score": text_hit.score,
                    "entity": entity_json(&entity),
                }));
            }
            if hits.len() >= requested {
                break;
            }
        }

        let (hits, pagination) = paginate_values(hits, offset, limit);
        Ok(json!({
            "status": "ok",
            "query": query,
            "mode": mode,
            "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
            "hits": hits,
            "pagination": pagination,
            "resource_links": result_resource_links(),
            "proof": "exact symbol lookup plus SQLite FTS entity evidence",
        }))
    }

    fn search_text(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let query = required_string(args, "query")?;
        let limit = optional_limit(args)?;
        let offset = optional_offset(args)?;
        let mode = response_mode(args)?;
        let repo_root = self.repo_root(args)?;
        let (store, preflight) = self.open_store_with_preflight(args)?;
        let requested = limit.saturating_add(offset).saturating_add(1);
        let mut raw_hits = store
            .search_text(&query, requested)
            .map_err(mcp_store_error)?
            .iter()
            .map(text_hit_json)
            .collect::<Vec<_>>();
        if raw_hits.len() < requested {
            raw_hits.extend(
                source_scan_text_hits(
                    &repo_root,
                    &store,
                    &query,
                    requested.saturating_sub(raw_hits.len()),
                )
                .map_err(ToolCallError::from)?,
            );
        }
        let (hits, pagination) = paginate_values(raw_hits, offset, limit);
        Ok(json!({
            "status": "ok",
            "query": query,
            "mode": mode,
            "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
            "hits": hits,
            "pagination": pagination,
            "resource_links": result_resource_links(),
            "proof": "Text query uses SQLite FTS when present and falls back to bounded on-demand source scanning over indexed files.",
        }))
    }

    fn search_semantic(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let query = required_string(args, "query")?;
        let limit = optional_limit(args)?;
        let offset = optional_offset(args)?;
        let mode = response_mode(args)?;
        let repo_root = self.repo_root(args)?;
        let (store, preflight) = self.open_store_with_preflight(args)?;
        let requested = limit.saturating_add(offset);
        let mut raw_hits = store
            .search_text(&query, limit.saturating_add(offset))
            .map_err(mcp_store_error)?
            .iter()
            .map(text_hit_json)
            .collect::<Vec<_>>();
        if raw_hits.len() < requested {
            raw_hits.extend(
                source_scan_text_hits(
                    &repo_root,
                    &store,
                    &query,
                    requested.saturating_sub(raw_hits.len()),
                )
                .map_err(ToolCallError::from)?,
            );
        }
        let (hits, pagination) = paginate_values(raw_hits, offset, limit);
        Ok(json!({
            "status": "ok",
            "query": query,
            "mode": mode,
            "semantic_mode": "deterministic_text_fallback",
            "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
            "hits": hits,
            "pagination": pagination,
            "resource_links": result_resource_links(),
            "proof": "Persistent semantic vector serving is not exposed through MCP in Phase 30; this tool returns Stage 0 text evidence only.",
        }))
    }

    fn context_pack(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let task = required_string(args, "task")?;
        let mode = optional_string(args, "mode").unwrap_or_else(|| "impact".to_string());
        let token_budget = optional_usize(args, "token_budget", 2_000, 32, 100_000)?;
        let seeds = optional_string_array(args, "seeds")?;
        let stage0_candidates = optional_string_array(args, "stage0_candidates")?;
        let repo_root = self.repo_root(args)?;
        let (store, preflight) = self.open_store_with_preflight(args)?;
        let sources = load_sources(&repo_root, &store).map_err(ToolCallError::from)?;
        let documents = retrieval_documents(&store)?;
        let funnel = RetrievalFunnel::new(
            store
                .list_edges(self.config.max_graph_edges)
                .map_err(mcp_store_error)?,
            documents,
            RetrievalFunnelConfig::default(),
        )
        .map_err(|error| ToolCallError::new("retrieval_funnel_failed", error.to_string()))?;
        let stage0_docs = stage0_candidates
            .iter()
            .map(|candidate| RetrievalDocument::new(candidate, candidate).stage0_score(1.0))
            .collect::<Vec<_>>();
        let result = funnel
            .run(
                RetrievalFunnelRequest::new(task.clone(), mode, token_budget)
                    .exact_seeds(seeds)
                    .stage0_candidates(stage0_docs)
                    .sources(sources),
            )
            .map_err(|error| ToolCallError::new("retrieval_funnel_failed", error.to_string()))?;

        Ok(json!({
            "status": "ok",
            "task": task,
            "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
            "packet": result.packet,
            "funnel_trace": result.trace.iter().map(retrieval_trace_stage_json).collect::<Vec<_>>(),
            "proof": "Context packet is built through Stage 0 exact seeds, Stage 1 binary sieve, Stage 2 compressed rerank, Stage 3 exact graph verification, and Stage 4 packet emission.",
        }))
    }

    fn trace_path(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let source = required_string(args, "source")?;
        let target = required_string(args, "target")?;
        let relations = optional_relations(args)?;
        let limits = query_limits(args)?;
        let (store, preflight) = self.open_store_with_preflight(args)?;
        let engine = self.query_engine(&store, args)?;
        let paths = engine.trace_path(&source, &target, &relations, limits);
        let mut value = paths_response(
            "trace_path",
            &engine,
            paths,
            args,
            Some((&store, &source, &target, &relations, limits)),
        )?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "db_lifecycle_read".to_string(),
                mcp_db_lifecycle_preflight_json(&preflight),
            );
        }
        Ok(value)
    }

    fn impact_analysis(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let entity_id = entity_id_arg(args)?;
        let limits = query_limits(args)?;
        let (store, preflight) = self.open_store_with_preflight(args)?;
        let engine = self.query_engine(&store, args)?;
        let impact = engine.impact_analysis_core(&entity_id, limits);
        Ok(json!({
            "status": "ok",
            "entity_id": entity_id,
            "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
            "impact": {
                "callers": path_evidence_json(&engine, impact.callers),
                "callees": path_evidence_json(&engine, impact.callees),
                "reads": path_evidence_json(&engine, impact.reads),
                "writes": path_evidence_json(&engine, impact.writes),
                "mutations": path_evidence_json(&engine, impact.mutations),
                "dataflow": path_evidence_json(&engine, impact.dataflow),
                "auth_paths": path_evidence_json(&engine, impact.auth_paths),
                "event_flow": path_evidence_json(&engine, impact.event_flow),
                "tests": path_evidence_json(&engine, impact.tests),
                "migrations": path_evidence_json(&engine, impact.migrations),
            },
            "proof": "Impact analysis is exact graph traversal over persisted edges.",
        }))
    }

    fn relation_query(
        &self,
        args: &Map<String, Value>,
        query_name: &str,
    ) -> Result<Value, ToolCallError> {
        let entity_id = entity_id_arg(args)?;
        let limits = query_limits(args)?;
        let (store, preflight) = self.open_store_with_preflight(args)?;
        let engine = self.query_engine(&store, args)?;
        let paths = match query_name {
            "callers" => engine.find_callers(&entity_id, limits),
            "callees" => engine.find_callees(&entity_id, limits),
            "reads" => engine.find_reads(&entity_id, limits),
            "writes" => engine.find_writes(&entity_id, limits),
            "mutations" => engine.find_mutations(&entity_id, limits),
            "dataflow" => engine.find_dataflow(&entity_id, limits),
            "auth_paths" => engine.find_auth_paths(&entity_id, limits),
            "event_flow" => engine.find_event_flow(&entity_id, limits),
            "tests" => engine.find_tests(&entity_id, limits),
            "migrations" => engine.find_migrations(&entity_id, limits),
            _ => Vec::new(),
        };
        let mut value = paths_response(query_name, &engine, paths, args, None)?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "db_lifecycle_read".to_string(),
                mcp_db_lifecycle_preflight_json(&preflight),
            );
        }
        Ok(value)
    }

    fn explain_edge(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let edge_id = required_string_alias(args, &["edge_id", "id"])?;
        let (store, preflight) = self.open_store_with_preflight(args)?;
        let Some(edge) = store.get_edge(&edge_id).map_err(mcp_store_error)? else {
            return Err(ToolCallError::new(
                "not_found",
                format!("edge not found: {edge_id}"),
            ));
        };
        Ok(json!({
            "status": "ok",
            "edge": edge_json(&edge),
            "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
            "proof": "Edge explanation is read directly from the local graph store.",
        }))
    }

    fn explain_path(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let source = required_string(args, "source")?;
        let target = required_string(args, "target")?;
        let relations = optional_relations(args)?;
        let limits = query_limits(args)?;
        let (store, preflight) = self.open_store_with_preflight(args)?;
        let engine = self.query_engine(&store, args)?;
        let paths = engine.trace_path(&source, &target, &relations, limits);
        let evidence = engine.path_evidence_from_paths(&paths);
        let (paths_json, pagination) = paginate_values(
            serde_json::to_value(evidence)
                .ok()
                .and_then(|value| value.as_array().cloned())
                .unwrap_or_default(),
            optional_offset(args)?,
            optional_limit(args)?,
        );
        Ok(json!({
            "status": "ok",
            "source": source,
            "target": target,
            "mode": response_mode(args)?,
            "paths": paths_json,
            "pagination": pagination,
            "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
            "explain_missing": if paths.is_empty() {
                explain_missing_path(&store, &source, &target, &relations, limits)?
            } else {
                json!(null)
            },
            "proof": "Path explanation includes metapath, base edges, source spans, exactness, and confidence.",
        }))
    }

    fn resolve_entity_id(
        &self,
        args: &Map<String, Value>,
        query: &str,
    ) -> Result<String, ToolCallError> {
        let store = self.open_store(args)?;
        let hits = store
            .find_entities_by_exact_symbol(query)
            .map_err(mcp_store_error)?;
        hits.first().map(|entity| entity.id.clone()).ok_or_else(|| {
            ToolCallError::new(
                "not_found",
                format!("no indexed entity matched symbol/query: {query}"),
            )
        })
    }

    fn read_resource(&self, uri: &str, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        match uri {
            "codegraph://status" => self.status(args),
            "codegraph://schema" => Ok(json!({
                "status": "ok",
                "phase": PHASE,
                "tools": self.tool_definitions(),
                "resources": self.resource_definitions(),
                "prompts": self.prompt_definitions(),
                "safety": mcp_safety_metadata(true),
                "recommended_workflow": recommended_workflow(),
                "background_indexing": {
                    "available": false,
                    "reason": "Not exposed yet; foreground indexing is bounded by the caller and avoids hidden long-running hangs."
                },
            })),
            "codegraph://languages" => Ok(json!({
                "status": "ok",
                "phase": PHASE,
                "frontends": language_frontends(),
                "proof": "Language capabilities are declared by the parser frontend registry.",
            })),
            "codegraph://bench/latest" => Ok(json!({
                "status": "unknown",
                "reason": "No persistent latest benchmark pointer is stored by the MCP server yet.",
                "unknown": true,
                "proof": "Unknown is reported explicitly instead of fabricating benchmark results.",
            })),
            value if value.starts_with("codegraph://context/") => Ok(json!({
                "status": "unknown",
                "context_id": value.trim_start_matches("codegraph://context/"),
                "reason": "Context packet resources are only returned when explicitly generated by codegraph.context_pack.",
                "unknown": true,
            })),
            other => Err(ToolCallError::new(
                "unknown_resource",
                format!("unknown CodeGraph MCP resource: {other}"),
            )),
        }
    }

    fn context_discovery(&self, args: &Map<String, Value>) -> Result<RepoContext, ToolCallError> {
        let repo_root = self.repo_root(args)?;
        let db_path = self.db_path(args, &repo_root)?;
        let indexed = db_path.exists();
        let detected_dbs = detect_indexed_repos(&repo_root);
        Ok(RepoContext {
            repo_root,
            db_path,
            indexed,
            detected_dbs,
        })
    }

    fn repo_root(&self, args: &Map<String, Value>) -> Result<PathBuf, ToolCallError> {
        let root = optional_string(args, "repo")
            .or_else(|| optional_string(args, "repo_root"))
            .map(PathBuf::from)
            .unwrap_or_else(|| self.config.repo_root.clone());
        if !root.exists() {
            return Err(ToolCallError::new(
                "repo_not_found",
                format!("repository path does not exist: {}", root.display()),
            ));
        }
        fs::canonicalize(&root).map_err(|error| {
            ToolCallError::new(
                "repo_not_found",
                format!(
                    "could not resolve repository path {}: {error}",
                    root.display()
                ),
            )
        })
    }

    fn db_path(
        &self,
        args: &Map<String, Value>,
        repo_root: &Path,
    ) -> Result<PathBuf, ToolCallError> {
        optional_string(args, "db_path")
            .map(PathBuf::from)
            .map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    repo_root.join(path)
                }
            })
            .map(Ok)
            .unwrap_or_else(|| {
                if paths_equivalent(repo_root, &self.config.repo_root) {
                    Ok(self.config.db_path.clone())
                } else {
                    Ok(default_db_path(repo_root))
                }
            })
    }

    fn open_store(&self, args: &Map<String, Value>) -> Result<SqliteGraphStore, ToolCallError> {
        let (store, _preflight) = self.open_store_with_preflight(args)?;
        Ok(store)
    }

    fn open_store_with_preflight(
        &self,
        args: &Map<String, Value>,
    ) -> Result<(SqliteGraphStore, DbLifecyclePreflight), ToolCallError> {
        let repo_root = self.repo_root(args)?;
        let db_path = self.db_path(args, &repo_root)?;
        if !db_path.exists() {
            return Err(ToolCallError::new(
                "not_indexed",
                format!(
                    "CodeGraph index does not exist yet at {}; call codegraph.index_repo first",
                    db_path.display()
                ),
            ));
        }
        let explicit_scope = mcp_explicit_scope_policy(args)?;
        let preflight = inspect_db_lifecycle_preflight(&repo_root, &db_path, explicit_scope)
            .map_err(ToolCallError::from)?;
        if !preflight.safe {
            if let Some(mismatch) = preflight.scope_mismatch.as_ref() {
                return Err(ToolCallError::new(
                    "scope_mismatch",
                    mcp_scope_mismatch_message(&db_path, mismatch),
                ));
            }
            return Err(ToolCallError::new(
                "db_lifecycle_blocked",
                format!(
                    "CodeGraph DB is not safe to read at {}: {}; call codegraph.index_repo to rebuild",
                    db_path.display(),
                    preflight.blockers.join("; ")
                ),
            ));
        }
        let store = SqliteGraphStore::open(&db_path).map_err(mcp_store_error)?;
        Ok((store, preflight))
    }

    fn query_engine(
        &self,
        store: &SqliteGraphStore,
        args: &Map<String, Value>,
    ) -> Result<ExactGraphQueryEngine, ToolCallError> {
        let max_edges = optional_usize(
            args,
            "max_graph_edges",
            self.config.max_graph_edges,
            1,
            DEFAULT_GRAPH_EDGE_LIMIT,
        )?;
        let edges = store.list_edges(max_edges).map_err(mcp_store_error)?;
        Ok(ExactGraphQueryEngine::new(edges))
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        self.trace_run_end("ok", None);
    }
}

pub fn serve_stdio() -> Result<(), McpServerError> {
    let server = McpServer::new(McpServerConfig::default());
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = serde_json::from_str(&line)?;
        if let Some(response) = server.handle_jsonrpc(&message) {
            writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
            stdout.flush()?;
        }
    }

    Ok(())
}

fn tool_definition(name: &str) -> Value {
    let (description, schema) = match name {
        "codegraph.search" => (
            "LLM-first find-code tool combining exact symbol and bounded text evidence.",
            search_schema(),
        ),
        "codegraph.analyze" => (
            "LLM-first relationship analysis wrapper over exact graph traversals.",
            high_level_schema(),
        ),
        "codegraph.plan_context" => (
            "LLM-first Stage 0-4 context planning tool for editing tasks.",
            plan_context_schema(),
        ),
        "codegraph.explain_missing" => (
            "Classify why requested evidence was not found without guessing.",
            explain_missing_schema(),
        ),
        "codegraph.status" => ("Report local CodeGraph index status.", status_schema()),
        "codegraph.index_repo" => (
            "Index a repository into the local CodeGraph SQLite store.",
            repo_schema(vec![("repo", "string", "Repository root to index.")]),
        ),
        "codegraph.update_changed_files" => (
            "Prune stale facts and re-index a supplied list of changed files.",
            repo_schema(vec![("files", "array", "Repository-relative file paths.")]),
        ),
        "codegraph.search_symbols" => (
            "Find indexed entities by exact symbol and FTS-backed symbol evidence.",
            search_schema(),
        ),
        "codegraph.search_text" => (
            "Search indexed file/entity/snippet text with SQLite FTS5/BM25.",
            search_schema(),
        ),
        "codegraph.search_semantic" => (
            "Return deterministic Stage 0 text evidence as Phase 15 semantic fallback.",
            search_schema(),
        ),
        "codegraph.context_pack" => (
            "Build a compact graph/source verified context packet.",
            context_pack_schema(),
        ),
        "codegraph.trace_path" | "codegraph.explain_path" => (
            "Trace and explain exact graph paths between two entities.",
            path_schema(),
        ),
        "codegraph.explain_edge" => (
            "Explain one persisted edge by id.",
            repo_schema(vec![("edge_id", "string", "Persisted edge id.")]),
        ),
        _ => (
            "Run a read-only exact graph relation query.",
            entity_query_schema(),
        ),
    };

    json!({
        "name": name,
        "description": description,
        "inputSchema": schema,
        "outputSchema": output_schema_for_tool(name),
        "annotations": tool_annotations(name),
    })
}

fn tool_annotations(name: &str) -> Value {
    let read_only = !matches!(
        name,
        "codegraph.index_repo" | "codegraph.update_changed_files"
    );
    json!({
        "readOnlyHint": read_only,
        "destructiveHint": false,
        "idempotentHint": true,
        "localOnly": true,
        "safety": mcp_safety_metadata(read_only),
    })
}

fn mcp_safety_metadata(read_only: bool) -> Value {
    json!({
        "read_only": read_only,
        "local_only": true,
        "destructive": false,
        "single_agent_workflow": true,
        "source_of_truth": "MVP.md",
    })
}

fn output_schema_for_tool(name: &str) -> Value {
    let mut properties = Map::from_iter([
        ("status".to_string(), json!({"type": "string"})),
        ("proof".to_string(), json!({"type": "string"})),
        ("resource_links".to_string(), json!({"type": "object"})),
    ]);
    match name {
        "codegraph.search"
        | "codegraph.search_symbols"
        | "codegraph.search_text"
        | "codegraph.search_semantic" => {
            properties.insert("hits".to_string(), json!({"type": "array"}));
            properties.insert("pagination".to_string(), json!({"type": "object"}));
        }
        "codegraph.analyze" => {
            properties.insert("paths".to_string(), json!({"type": "array"}));
            properties.insert("pagination".to_string(), json!({"type": "object"}));
            properties.insert("repo_context".to_string(), json!({"type": "object"}));
        }
        "codegraph.plan_context" => {
            properties.insert("packet".to_string(), json!({"type": "object"}));
            properties.insert("workflow".to_string(), json!({"type": "array"}));
            properties.insert("repo_context".to_string(), json!({"type": "object"}));
        }
        "codegraph.explain_missing" => {
            properties.insert("category".to_string(), json!({"type": "string"}));
            properties.insert("repo_context".to_string(), json!({"type": "object"}));
        }
        "codegraph.trace_path"
        | "codegraph.explain_path"
        | "codegraph.find_callers"
        | "codegraph.find_callees"
        | "codegraph.find_reads"
        | "codegraph.find_writes"
        | "codegraph.find_mutations"
        | "codegraph.find_dataflow"
        | "codegraph.find_auth_paths"
        | "codegraph.find_event_flow"
        | "codegraph.find_tests"
        | "codegraph.find_migrations" => {
            properties.insert("paths".to_string(), json!({"type": "array"}));
            properties.insert("pagination".to_string(), json!({"type": "object"}));
            properties.insert(
                "explain_missing".to_string(),
                json!({"type": ["object", "null"]}),
            );
        }
        "codegraph.context_pack" => {
            properties.insert("packet".to_string(), json!({"type": "object"}));
        }
        "codegraph.status" => {
            properties.insert("safe_to_query".to_string(), json!({"type": "boolean"}));
            properties.insert("blockers".to_string(), json!({"type": "array"}));
            properties.insert("warnings".to_string(), json!({"type": "array"}));
            properties.insert("passport_summary".to_string(), json!({"type": "object"}));
            properties.insert("scope_source".to_string(), json!({"type": "string"}));
            properties.insert("db_lifecycle_read".to_string(), json!({"type": "object"}));
            properties.insert("problem".to_string(), json!({"type": "string"}));
            properties.insert("files".to_string(), json!({"type": "integer"}));
            properties.insert("entities".to_string(), json!({"type": "integer"}));
            properties.insert("edges".to_string(), json!({"type": "integer"}));
        }
        _ => {}
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": ["status"],
        "additionalProperties": true,
    })
}

fn resource_definition(uri: &str) -> Value {
    let (name, description) = match uri {
        "codegraph://status" => ("status", "Current local index status."),
        "codegraph://schema" => (
            "schema",
            "MCP tool/resource/prompt schemas and safety metadata.",
        ),
        "codegraph://languages" => (
            "languages",
            "Language frontend support tiers and exactness.",
        ),
        "codegraph://bench/latest" => ("bench/latest", "Latest benchmark summary when available."),
        _ => ("context/<id>", "Context packet resource placeholder by id."),
    };
    json!({
        "uri": uri,
        "name": name,
        "description": description,
        "mimeType": "application/json",
        "annotations": {
            "readOnlyHint": true,
            "destructiveHint": false,
            "localOnly": true
        }
    })
}

fn prompt_definition(name: &str) -> Value {
    let description = match name {
        "impact-analysis" => {
            "Analyze blast radius from a symbol or file using verified graph evidence."
        }
        "trace-dataflow" => "Trace dataflow with exactness/confidence labels.",
        "auth-review" => "Review auth/security paths without inventing unsupported relations.",
        "test-impact" => "Select evidence-backed impacted tests.",
        "refactor-safety" => "Prepare a refactor safety checklist from verified paths.",
        _ => "CodeGraph prompt template.",
    };
    json!({
        "name": name,
        "description": description,
        "arguments": [
            {"name": "target", "description": "Symbol, file, route, or task target.", "required": true},
            {"name": "mode", "description": "compact, verbose, or explain.", "required": false}
        ],
    })
}

fn prompt_template(name: &str) -> Option<Value> {
    let text = match name {
        "impact-analysis" => "Read MVP.md. Do not use subagents. Call codegraph.context_pack for {{target}}, then inspect codegraph.impact_analysis paths. Prefer verified relation paths and cite source spans.",
        "trace-dataflow" => "Read MVP.md. Do not use subagents. Use codegraph.find_dataflow and codegraph.trace_path for {{target}}. Preserve exactness/confidence labels and explain missing paths explicitly.",
        "auth-review" => "Read MVP.md. Do not use subagents. Use codegraph.find_auth_paths and relation filters for EXPOSES/AUTHORIZES/CHECKS_ROLE. Do not treat heuristic edges as exact.",
        "test-impact" => "Read MVP.md. Do not use subagents. Use codegraph.find_tests and context packets to recommend minimal tests with explicit evidence paths.",
        "refactor-safety" => "Read MVP.md. Do not use subagents. Build a context packet, trace callers/callees/dataflow, update changed files after edits, and run recommended tests when practical.",
        _ => return None,
    };
    Some(json!({
        "description": prompt_definition(name)["description"],
        "messages": [
            {
                "role": "user",
                "content": {
                    "type": "text",
                    "text": text
                }
            }
        ]
    }))
}

fn repo_schema(required: Vec<(&str, &str, &str)>) -> Value {
    let mut properties = Map::from_iter([
        (
            "repo".to_string(),
            json!({"type": "string", "description": "Repository root. Defaults to server cwd."}),
        ),
        (
            "db_path".to_string(),
            json!({"type": "string", "description": "Optional SQLite DB path."}),
        ),
    ]);
    let mut required_names = Vec::new();
    for (name, kind, description) in required {
        let schema = if kind == "array" {
            json!({"type": "array", "items": {"type": "string"}, "description": description})
        } else {
            json!({"type": kind, "description": description})
        };
        properties.insert(name.to_string(), schema);
        required_names.push(name);
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required_names,
        "additionalProperties": false,
    })
}

fn add_read_scope_schema_properties(schema: &mut Value) {
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "include_ignored".to_string(),
            json!({"type": "boolean", "description": "Explicit read scope: include gitignored files if the DB passport was built with that policy."}),
        );
        properties.insert(
            "no_default_excludes".to_string(),
            json!({"type": "boolean", "description": "Explicit read scope: disable CodeGraph default excludes when matching the DB passport."}),
        );
        properties.insert(
            "respect_gitignore".to_string(),
            json!({"type": "boolean", "description": "Explicit read scope: respect repository ignore files when matching the DB passport."}),
        );
        properties.insert(
            "include".to_string(),
            json!({"type": ["string", "array"], "items": {"type": "string"}, "description": "Explicit read scope include pattern or patterns."}),
        );
        properties.insert(
            "exclude".to_string(),
            json!({"type": ["string", "array"], "items": {"type": "string"}, "description": "Explicit read scope exclude pattern or patterns."}),
        );
    }
}

fn search_schema() -> Value {
    let mut schema = repo_schema(vec![("query", "string", "Search query.")]);
    add_read_scope_schema_properties(&mut schema);
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "limit".to_string(),
            json!({"type": "integer", "minimum": 1, "maximum": 100}),
        );
        properties.insert(
            "offset".to_string(),
            json!({"type": "integer", "minimum": 0, "description": "Pagination offset."}),
        );
        properties.insert(
            "mode".to_string(),
            json!({"type": "string", "enum": ["compact", "verbose", "explain"]}),
        );
    }
    schema
}

fn status_schema() -> Value {
    let mut schema = repo_schema(Vec::new());
    add_read_scope_schema_properties(&mut schema);
    schema
}

fn entity_query_schema() -> Value {
    let mut schema = repo_schema(Vec::new());
    add_read_scope_schema_properties(&mut schema);
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "entity_id".to_string(),
            json!({"type": "string", "description": "Entity id or qualified symbol."}),
        );
        properties.insert(
            "max_depth".to_string(),
            json!({"type": "integer", "minimum": 1, "maximum": 12}),
        );
        properties.insert(
            "max_paths".to_string(),
            json!({"type": "integer", "minimum": 1, "maximum": 100}),
        );
        properties.insert(
            "offset".to_string(),
            json!({"type": "integer", "minimum": 0, "description": "Pagination offset."}),
        );
        properties.insert(
            "limit".to_string(),
            json!({"type": "integer", "minimum": 1, "maximum": 100}),
        );
        properties.insert(
            "mode".to_string(),
            json!({"type": "string", "enum": ["compact", "verbose", "explain"]}),
        );
    }
    schema["required"] = json!(["entity_id"]);
    schema
}

fn path_schema() -> Value {
    let mut schema = repo_schema(vec![
        ("source", "string", "Source entity id."),
        ("target", "string", "Target entity id."),
    ]);
    add_read_scope_schema_properties(&mut schema);
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "relations".to_string(),
            json!({"type": "array", "items": {"type": "string"}}),
        );
        properties.insert(
            "offset".to_string(),
            json!({"type": "integer", "minimum": 0}),
        );
        properties.insert(
            "limit".to_string(),
            json!({"type": "integer", "minimum": 1, "maximum": 100}),
        );
        properties.insert(
            "mode".to_string(),
            json!({"type": "string", "enum": ["compact", "verbose", "explain"]}),
        );
    }
    schema
}

fn context_pack_schema() -> Value {
    let mut schema = repo_schema(vec![("task", "string", "User task to build context for.")]);
    add_read_scope_schema_properties(&mut schema);
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert("mode".to_string(), json!({"type": "string"}));
        properties.insert(
            "token_budget".to_string(),
            json!({"type": "integer", "minimum": 32}),
        );
        properties.insert(
            "seeds".to_string(),
            json!({"type": "array", "items": {"type": "string"}}),
        );
        properties.insert(
            "stage0_candidates".to_string(),
            json!({"type": "array", "items": {"type": "string"}}),
        );
    }
    schema
}

fn high_level_schema() -> Value {
    let mut schema = repo_schema(Vec::new());
    add_read_scope_schema_properties(&mut schema);
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "query".to_string(),
            json!({"type": "string", "description": "Symbol, file, or task phrase to resolve."}),
        );
        properties.insert(
            "symbol".to_string(),
            json!({"type": "string", "description": "Symbol name to resolve when entity_id is unknown."}),
        );
        properties.insert(
            "analysis".to_string(),
            json!({"type": "string", "enum": ["impact", "callers", "callees", "dataflow", "auth_paths", "event_flow", "tests", "migrations"]}),
        );
        properties.insert(
            "entity_id".to_string(),
            json!({"type": "string", "description": "Entity id for analysis/context planning."}),
        );
        properties.insert(
            "id".to_string(),
            json!({"type": "string", "description": "Alias for entity_id."}),
        );
        properties.insert(
            "max_depth".to_string(),
            json!({"type": "integer", "minimum": 1, "maximum": 12}),
        );
        properties.insert(
            "max_paths".to_string(),
            json!({"type": "integer", "minimum": 1, "maximum": 100}),
        );
        properties.insert(
            "max_edges_visited".to_string(),
            json!({"type": "integer", "minimum": 1, "maximum": 100000}),
        );
        properties.insert(
            "limit".to_string(),
            json!({"type": "integer", "minimum": 1, "maximum": 100}),
        );
        properties.insert(
            "offset".to_string(),
            json!({"type": "integer", "minimum": 0, "description": "Pagination offset."}),
        );
        properties.insert(
            "mode".to_string(),
            json!({"type": "string", "enum": ["compact", "verbose", "explain"]}),
        );
    }
    schema
}

fn plan_context_schema() -> Value {
    let mut schema = repo_schema(Vec::new());
    add_read_scope_schema_properties(&mut schema);
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "task".to_string(),
            json!({"type": "string", "description": "Editing or planning task to build context for."}),
        );
        properties.insert(
            "query".to_string(),
            json!({"type": "string", "description": "Fallback task phrase when task is omitted."}),
        );
        properties.insert(
            "symbol".to_string(),
            json!({"type": "string", "description": "Fallback symbol seed when task is omitted."}),
        );
        properties.insert(
            "mode".to_string(),
            json!({"type": "string", "enum": ["impact", "compact", "verbose", "explain"]}),
        );
        properties.insert(
            "token_budget".to_string(),
            json!({"type": "integer", "minimum": 32}),
        );
        properties.insert(
            "seeds".to_string(),
            json!({"type": "array", "items": {"type": "string"}, "description": "Exact entity ids or symbols that must bypass vector filters."}),
        );
        properties.insert(
            "stage0_candidates".to_string(),
            json!({"type": "array", "items": {"type": "string"}}),
        );
        properties.insert(
            "limit".to_string(),
            json!({"type": "integer", "minimum": 1, "maximum": 100}),
        );
        properties.insert(
            "offset".to_string(),
            json!({"type": "integer", "minimum": 0, "description": "Reserved for future packet pagination."}),
        );
    }
    schema
}

fn explain_missing_schema() -> Value {
    let mut schema = repo_schema(Vec::new());
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert("symbol".to_string(), json!({"type": "string"}));
        properties.insert("query".to_string(), json!({"type": "string"}));
        properties.insert("source".to_string(), json!({"type": "string"}));
        properties.insert("target".to_string(), json!({"type": "string"}));
        properties.insert("language".to_string(), json!({"type": "string"}));
        properties.insert("relation".to_string(), json!({"type": "string"}));
        properties.insert(
            "relations".to_string(),
            json!({"type": "array", "items": {"type": "string"}}),
        );
        properties.insert(
            "max_depth".to_string(),
            json!({"type": "integer", "minimum": 1}),
        );
        properties.insert(
            "max_paths".to_string(),
            json!({"type": "integer", "minimum": 1}),
        );
        properties.insert(
            "max_edges_visited".to_string(),
            json!({"type": "integer", "minimum": 1}),
        );
    }
    schema
}

fn object_arguments(value: &Value) -> Result<&Map<String, Value>, ToolCallError> {
    value
        .as_object()
        .ok_or_else(|| ToolCallError::new("invalid_input", "tool arguments must be an object"))
}

fn required_string(args: &Map<String, Value>, key: &str) -> Result<String, ToolCallError> {
    required_string_alias(args, &[key])
}

fn required_string_alias(
    args: &Map<String, Value>,
    keys: &[&str],
) -> Result<String, ToolCallError> {
    for key in keys {
        if let Some(value) = args.get(*key).and_then(Value::as_str) {
            let value = value.trim();
            if !value.is_empty() {
                return Ok(value.to_string());
            }
        }
    }
    Err(ToolCallError::new(
        "invalid_input",
        format!("required string argument missing: {}", keys.join("|")),
    ))
}

fn optional_string(args: &Map<String, Value>, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn required_string_array(
    args: &Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, ToolCallError> {
    let values = optional_string_array(args, key)?;
    if values.is_empty() {
        Err(ToolCallError::new(
            "invalid_input",
            format!("required string array argument missing or empty: {key}"),
        ))
    } else {
        Ok(values)
    }
}

fn optional_string_array(
    args: &Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, ToolCallError> {
    let Some(value) = args.get(key) else {
        return Ok(Vec::new());
    };
    let Some(array) = value.as_array() else {
        return Err(ToolCallError::new(
            "invalid_input",
            format!("{key} must be an array of strings"),
        ));
    };
    array
        .iter()
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                ToolCallError::new("invalid_input", format!("{key} must contain only strings"))
            })
        })
        .collect()
}

fn optional_usize(
    args: &Map<String, Value>,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, ToolCallError> {
    let Some(value) = args.get(key) else {
        return Ok(default);
    };
    let Some(number) = value.as_u64() else {
        return Err(ToolCallError::new(
            "invalid_input",
            format!("{key} must be an integer"),
        ));
    };
    Ok((number as usize).clamp(min, max))
}

fn optional_limit(args: &Map<String, Value>) -> Result<usize, ToolCallError> {
    optional_usize(args, "limit", DEFAULT_RESULT_LIMIT, 1, 100)
}

fn optional_offset(args: &Map<String, Value>) -> Result<usize, ToolCallError> {
    optional_usize(args, "offset", 0, 0, 100_000)
}

fn response_mode(args: &Map<String, Value>) -> Result<String, ToolCallError> {
    match optional_string(args, "mode")
        .unwrap_or_else(|| "compact".to_string())
        .replace('-', "_")
        .to_ascii_lowercase()
        .as_str()
    {
        "compact" => Ok("compact".to_string()),
        "verbose" => Ok("verbose".to_string()),
        "explain" => Ok("explain".to_string()),
        other => Err(ToolCallError::new(
            "invalid_input",
            format!("mode must be compact, verbose, or explain; got {other}"),
        )),
    }
}

fn query_limits(args: &Map<String, Value>) -> Result<QueryLimits, ToolCallError> {
    Ok(QueryLimits {
        max_depth: optional_usize(args, "max_depth", 6, 1, 12)?,
        max_paths: optional_usize(args, "max_paths", 24, 1, 100)?,
        max_edges_visited: optional_usize(args, "max_edges_visited", 5_000, 1, 100_000)?,
    })
}

fn paginate_values(values: Vec<Value>, offset: usize, limit: usize) -> (Vec<Value>, Value) {
    let total = values.len();
    let items = values
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let next_offset = (offset + items.len() < total).then_some(offset + items.len());
    let returned = next_offset.map_or(total.saturating_sub(offset).min(limit), |next| {
        next.saturating_sub(offset)
    });
    (
        items,
        json!({
            "offset": offset,
            "limit": limit,
            "returned": returned,
            "total_available": total,
            "truncated": next_offset.is_some(),
            "next_offset": next_offset,
        }),
    )
}

fn optional_relations_or_single(
    args: &Map<String, Value>,
) -> Result<Vec<RelationKind>, ToolCallError> {
    if let Some(relation) = optional_string(args, "relation") {
        return RelationKind::from_str(&relation)
            .map(|relation| vec![relation])
            .map_err(|error| {
                ToolCallError::new(
                    "invalid_relation",
                    format!("invalid relation {relation}: {error}"),
                )
            });
    }
    optional_relations(args)
}

fn optional_relations(args: &Map<String, Value>) -> Result<Vec<RelationKind>, ToolCallError> {
    let values = optional_string_array(args, "relations")?;
    if values.is_empty() {
        return Ok(RelationKind::ALL.to_vec());
    }
    values
        .iter()
        .map(|value| {
            RelationKind::from_str(value).map_err(|error| {
                ToolCallError::new(
                    "invalid_relation",
                    format!("invalid relation {value}: {error}"),
                )
            })
        })
        .collect()
}

fn entity_id_arg(args: &Map<String, Value>) -> Result<String, ToolCallError> {
    required_string_alias(args, &["entity_id", "id", "symbol"])
}

fn paths_response(
    name: &str,
    engine: &ExactGraphQueryEngine,
    paths: Vec<GraphPath>,
    args: &Map<String, Value>,
    missing_context: Option<(&SqliteGraphStore, &str, &str, &[RelationKind], QueryLimits)>,
) -> Result<Value, ToolCallError> {
    let evidence = engine.path_evidence_from_paths(&paths);
    let (paths_json, pagination) = paginate_values(
        serde_json::to_value(evidence)
            .ok()
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default(),
        optional_offset(args)?,
        optional_limit(args)?,
    );
    let explain_missing = if paths.is_empty() {
        if let Some((store, source, target, relations, limits)) = missing_context {
            explain_missing_path(store, source, target, relations, limits)?
        } else {
            json!({
                "reason": "no matching relation paths found within the requested bounds",
                "category": "symbol_found_but_no_matching_relation"
            })
        }
    } else {
        json!(null)
    };
    Ok(json!({
        "status": "ok",
        "query": name,
        "mode": response_mode(args)?,
        "paths": paths_json,
        "pagination": pagination,
        "explain_missing": explain_missing,
        "resource_links": result_resource_links(),
        "proof": "Returned paths are exact graph traversal results with provenance and source spans.",
    }))
}

fn path_evidence_json(engine: &ExactGraphQueryEngine, paths: Vec<GraphPath>) -> Value {
    serde_json::to_value(engine.path_evidence_from_paths(&paths)).unwrap_or_else(|_| json!([]))
}

fn explain_missing_path(
    store: &SqliteGraphStore,
    source: &str,
    target: &str,
    relations: &[RelationKind],
    limits: QueryLimits,
) -> Result<Value, ToolCallError> {
    let source_entity = store.get_entity(source).map_err(mcp_store_error)?;
    let target_entity = store.get_entity(target).map_err(mcp_store_error)?;
    if source_entity.is_none()
        && store
            .find_entities_by_exact_symbol(source)
            .map_err(mcp_store_error)?
            .is_empty()
    {
        return Ok(json!({
            "category": "no_symbol_found",
            "symbol": source,
            "reason": "source symbol/entity could not be found in the local graph"
        }));
    }
    if target_entity.is_none()
        && store
            .find_entities_by_exact_symbol(target)
            .map_err(mcp_store_error)?
            .is_empty()
    {
        return Ok(json!({
            "category": "no_symbol_found",
            "symbol": target,
            "reason": "target symbol/entity could not be found in the local graph"
        }));
    }
    if let Some(entity) = source_entity.as_ref().or(target_entity.as_ref()) {
        if let Some(frontend) = language_frontends().iter().find(|frontend| {
            frontend
                .file_extensions
                .iter()
                .any(|ext| entity.repo_relative_path.ends_with(ext))
        }) {
            let unsupported = relations
                .iter()
                .filter(|relation| !frontend.supported_relation_kinds.contains(relation))
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            if !unsupported.is_empty() {
                return Ok(json!({
                    "category": "relation_unsupported_for_language",
                    "language": frontend.language_id,
                    "unsupported_relations": unsupported,
                    "reason": "requested relation is not declared for this language frontend"
                }));
            }
            if frontend.compiler_resolver_available && !typescript_resolver_available() {
                return Ok(json!({
                    "category": "resolver_unavailable",
                    "language": frontend.language_id,
                    "reason": "compiler resolver is optional and not available in this environment"
                }));
            }
        }
    }
    let matching_relation_exists = store
        .list_edges(DEFAULT_GRAPH_EDGE_LIMIT)
        .map_err(mcp_store_error)?
        .iter()
        .any(|edge| relations.contains(&edge.relation));
    if !matching_relation_exists {
        return Ok(json!({
            "category": "symbol_found_but_no_matching_relation",
            "relations": relations.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "reason": "symbols exist, but no indexed edges match the requested relation filter"
        }));
    }
    Ok(json!({
        "category": "path_exceeds_bound_or_disconnected",
        "max_depth": limits.max_depth,
        "max_paths": limits.max_paths,
        "max_edges_visited": limits.max_edges_visited,
        "reason": "matching relations exist, but no path was found within the traversal bounds"
    }))
}

fn typescript_resolver_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok()
}

fn entity_json(entity: &Entity) -> Value {
    let heuristic = entity.created_from.contains("heuristic")
        || entity.metadata.keys().any(|key| key.contains("heuristic"));
    json!({
        "id": entity.id,
        "kind": entity.kind.to_string(),
        "name": entity.name,
        "qualified_name": entity.qualified_name,
        "repo_relative_path": entity.repo_relative_path,
        "source_span": entity.source_span,
        "resource_links": entity.source_span.as_ref().map(source_span_resource_links).unwrap_or_default(),
        "created_from": entity.created_from,
        "exactness": if heuristic { "static_heuristic" } else { "unknown" },
        "heuristic": heuristic,
        "unsupported": false,
        "confidence": entity.confidence,
        "metadata": entity.metadata,
    })
}

fn edge_json(edge: &Edge) -> Value {
    json!({
        "id": edge.id,
        "head_id": edge.head_id,
        "relation": edge.relation.to_string(),
        "tail_id": edge.tail_id,
        "source_span": edge.source_span,
        "resource_links": source_span_resource_links(&edge.source_span),
        "exactness": edge.exactness.to_string(),
        "confidence": edge.confidence,
        "extractor": edge.extractor,
        "provenance_edges": edge.provenance_edges,
        "metadata": edge.metadata,
    })
}

fn source_span_resource_links(span: &SourceSpan) -> Value {
    json!({
        "file": format!("codegraph://file/{}", span.repo_relative_path),
        "source_span": format!(
            "codegraph://source-span/{}:{}-{}",
            span.repo_relative_path, span.start_line, span.end_line
        )
    })
}

fn result_resource_links() -> Value {
    json!({
        "status": "codegraph://status",
        "schema": "codegraph://schema",
        "languages": "codegraph://languages",
    })
}

fn recommended_workflow() -> Value {
    json!([
        {
            "step": 1,
            "action": "locate relevant symbols/files",
            "primary_tool": "codegraph.search",
            "fallback_tools": ["codegraph.search_symbols", "codegraph.search_text"]
        },
        {
            "step": 2,
            "action": "analyze relation paths",
            "primary_tool": "codegraph.analyze",
            "fallback_tools": ["codegraph.trace_path", "codegraph.impact_analysis", "codegraph.find_callers", "codegraph.find_callees"]
        },
        {
            "step": 3,
            "action": "build compact context packet",
            "primary_tool": "codegraph.plan_context",
            "fallback_tools": ["codegraph.context_pack"]
        },
        {
            "step": 4,
            "action": "edit using exactness/confidence/source-span evidence",
            "primary_tool": "agent_editor",
            "fallback_tools": []
        },
        {
            "step": 5,
            "action": "update changed files",
            "primary_tool": "codegraph.update_changed_files",
            "fallback_tools": []
        },
        {
            "step": 6,
            "action": "run recommended tests",
            "primary_tool": "local_test_runner",
            "fallback_tools": ["codegraph.find_tests"]
        }
    ])
}

fn not_indexed_response(tool: &str, context: &RepoContext) -> Value {
    json!({
        "status": "not_indexed",
        "tool": format!("codegraph.{tool}"),
        "repo_context": context.to_json(),
        "suggested_next": "Index this repo first with codegraph.index_repo using the shown repo/db_path.",
        "background_indexing": {
            "available": false,
            "reason": "Hidden background jobs are not exposed until they can be bounded and monitored safely."
        },
        "proof": "The configured SQLite DB does not exist, so CodeGraph refuses to invent search or analysis results.",
    })
}

fn detect_indexed_repos(repo_root: &Path) -> Vec<String> {
    let candidates = [
        default_db_path(repo_root),
        repo_root.join(".codegraph").join("codegraph.sqlite"),
    ];
    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|path| path.exists())
        .map(|path| path_string(&path))
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn retrieval_documents(store: &SqliteGraphStore) -> Result<Vec<RetrievalDocument>, ToolCallError> {
    Ok(store
        .list_entities(50_000)
        .map_err(mcp_store_error)?
        .into_iter()
        .map(|entity| {
            let text = format!(
                "{} {} {} {} {}",
                entity.kind,
                entity.name,
                entity.qualified_name,
                entity.repo_relative_path,
                entity.created_from
            );
            let mut document = RetrievalDocument::new(entity.id, text).stage0_score(0.25);
            document
                .metadata
                .insert("repo_relative_path".to_string(), entity.repo_relative_path);
            document
                .metadata
                .insert("kind".to_string(), entity.kind.to_string());
            document
        })
        .collect())
}

fn retrieval_trace_stage_json(stage: &RetrievalTraceStage) -> Value {
    json!({
        "stage": stage.stage,
        "kept": stage.kept,
        "dropped": stage.dropped,
        "kept_count": stage.kept.len(),
        "dropped_count": stage.dropped.len(),
        "notes": stage.notes,
    })
}

fn text_hit_source_span(hit: &Value) -> Value {
    let Some(path) = hit.get("repo_relative_path").and_then(Value::as_str) else {
        return json!(null);
    };
    let line = hit.get("line").and_then(Value::as_u64).unwrap_or(1);
    json!({
        "repo_relative_path": path,
        "start_line": line,
        "end_line": line,
    })
}

fn resolver_status_for_language(language_id: &str) -> Value {
    json!({
        "language": language_id,
        "compiler_resolver_available": if language_id == "typescript" {
            typescript_resolver_available()
        } else {
            false
        },
        "lsp_resolver_available": false,
        "unknown": language_id != "typescript",
    })
}

fn text_hit_json(hit: &TextSearchHit) -> Value {
    json!({
        "kind": hit.kind.as_str(),
        "id": hit.id,
        "repo_relative_path": hit.repo_relative_path,
        "line": hit.line,
        "title": hit.title,
        "text": hit.text,
        "score": hit.score,
        "proof_quality": {
            "exactness": "textual_exact_match",
            "confidence": "unknown",
            "heuristic": false,
            "unsupported": false
        }
    })
}

fn source_scan_text_hits(
    repo_root: &Path,
    store: &SqliteGraphStore,
    query: &str,
    limit: usize,
) -> Result<Vec<Value>, McpServerError> {
    if query.trim().is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let query_lc = query.to_ascii_lowercase();
    let mut hits = Vec::new();
    for file in store.list_files(UNBOUNDED_STORE_READ_LIMIT)? {
        let path = repo_root.join(&file.repo_relative_path);
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        for (line_index, line) in source.lines().enumerate() {
            if !line.to_ascii_lowercase().contains(&query_lc) {
                continue;
            }
            let repo_relative_path = file.repo_relative_path.clone();
            hits.push(json!({
                "kind": "file",
                "id": repo_relative_path.clone(),
                "repo_relative_path": repo_relative_path,
                "line": line_index + 1,
                "title": file.repo_relative_path,
                "text": line.trim(),
                "score": 0.0,
                "match": "source_scan",
                "proof_quality": {
                    "exactness": "textual_exact_match",
                    "confidence": "unknown",
                    "heuristic": false,
                    "unsupported": false
                },
            }));
            if hits.len() >= limit {
                break;
            }
        }
        if hits.len() >= limit {
            break;
        }
    }
    Ok(hits)
}

fn jsonrpc_result(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn jsonrpc_error(id: Value, code: i64, error: &str, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
            "data": {
                "error": error,
            }
        }
    })
}

fn mcp_tool_result(value: Value, is_error: bool) -> Value {
    let text = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ],
        "structuredContent": value,
        "isError": is_error,
    })
}

fn mcp_resource_result(uri: &str, value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string());
    json!({
        "contents": [
            {
                "uri": uri,
                "mimeType": "application/json",
                "text": text
            }
        ]
    })
}

fn mcp_store_error(error: codegraph_store::StoreError) -> ToolCallError {
    ToolCallError::new("store_error", error.to_string())
}

fn mcp_explicit_scope_policy(
    args: &Map<String, Value>,
) -> Result<Option<IndexScopeOptions>, ToolCallError> {
    let mut options = IndexScopeOptions::default();
    let mut explicit = false;

    if let Some(value) = optional_bool_arg(args, "include_ignored")? {
        options.include_ignored = value;
        explicit = true;
    }
    if let Some(value) = optional_bool_arg(args, "includeIgnored")? {
        options.include_ignored = value;
        explicit = true;
    }
    if let Some(value) = optional_bool_arg(args, "no_default_excludes")? {
        options.no_default_excludes = value;
        explicit = true;
    }
    if let Some(value) = optional_bool_arg(args, "noDefaultExcludes")? {
        options.no_default_excludes = value;
        explicit = true;
    }
    if let Some(value) = optional_bool_arg(args, "respect_gitignore")? {
        options.respect_gitignore = value;
        explicit = true;
    }
    if let Some(value) = optional_bool_arg(args, "respectGitignore")? {
        options.respect_gitignore = value;
        explicit = true;
    }
    for value in optional_scope_patterns(args, &["include", "includes", "include_patterns"])? {
        options.include_patterns.push(value);
        explicit = true;
    }
    for value in optional_scope_patterns(args, &["exclude", "excludes", "exclude_patterns"])? {
        options.exclude_patterns.push(value);
        explicit = true;
    }

    Ok(explicit.then_some(options))
}

fn optional_bool_arg(args: &Map<String, Value>, name: &str) -> Result<Option<bool>, ToolCallError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(ToolCallError::new(
            "invalid_input",
            format!("{name} must be a boolean"),
        )),
    }
}

fn optional_scope_patterns(
    args: &Map<String, Value>,
    names: &[&str],
) -> Result<Vec<String>, ToolCallError> {
    let mut values = Vec::new();
    for name in names {
        let Some(value) = args.get(*name) else {
            continue;
        };
        match value {
            Value::Null => {}
            Value::String(pattern) => values.push(pattern.clone()),
            Value::Array(items) => {
                for item in items {
                    let Some(pattern) = item.as_str() else {
                        return Err(ToolCallError::new(
                            "invalid_input",
                            format!("{name} entries must be strings"),
                        ));
                    };
                    values.push(pattern.to_string());
                }
            }
            _ => {
                return Err(ToolCallError::new(
                    "invalid_input",
                    format!("{name} must be a string or string array"),
                ));
            }
        }
    }
    Ok(values)
}

fn mcp_db_problem_kind(preflight: &DbLifecyclePreflight) -> &'static str {
    if preflight.repo_root_status == "mismatched" {
        "repo_root_mismatch"
    } else if preflight.scope_status == "mismatched" {
        "scope_mismatch"
    } else if preflight.schema_status == "mismatched" {
        "schema_mismatch"
    } else if preflight.storage_mode_status == "mismatched" {
        "storage_mismatch"
    } else if preflight.db_health.passport_status == "missing" {
        "passport_missing"
    } else if preflight.db_health.passport_status == "corrupt" {
        "passport_corrupt"
    } else {
        "passport_invalid"
    }
}

fn mcp_passport_summary_json(preflight: &DbLifecyclePreflight) -> Value {
    let passport = preflight.db_health.passport.as_ref();
    json!({
        "passport_status": preflight.db_health.passport_status.clone(),
        "schema_version": preflight.db_health.schema_version,
        "passport_version": passport.map(|value| value.passport_version),
        "codegraph_schema_version": passport.map(|value| value.codegraph_schema_version),
        "storage_mode": passport.map(|value| value.storage_mode.clone()),
        "canonical_repo_root": passport.map(|value| value.canonical_repo_root.clone()),
        "scope_source": preflight.scope_source.clone(),
        "scope_status": preflight.scope_status.clone(),
        "passport_scope_hash": preflight.passport_scope_hash.clone(),
        "explicit_scope_hash": preflight.explicit_scope_hash.clone(),
        "last_run_status": passport.map(|value| value.last_run_status.clone()),
        "integrity_gate_result": passport.map(|value| value.integrity_gate_result.clone()),
        "files_seen": passport.map(|value| value.files_seen),
        "files_indexed": passport.map(|value| value.files_indexed),
        "updated_at_unix_ms": passport.map(|value| value.updated_at_unix_ms),
    })
}

fn mcp_db_lifecycle_preflight_json(preflight: &DbLifecyclePreflight) -> Value {
    json!({
        "decision": if preflight.safe { "read_reuse" } else { "blocked" },
        "passport_status": preflight.db_health.passport_status.clone(),
        "safe": preflight.safe,
        "claimable": preflight.safe,
        "blockers": preflight.blockers.clone(),
        "warnings": preflight.warnings.clone(),
        "repo_root_status": preflight.repo_root_status.clone(),
        "schema_status": preflight.schema_status.clone(),
        "storage_mode_status": preflight.storage_mode_status.clone(),
        "scope_status": preflight.scope_status.clone(),
        "scope_source": preflight.scope_source.clone(),
        "passport_scope_hash": preflight.passport_scope_hash.clone(),
        "explicit_scope_hash": preflight.explicit_scope_hash.clone(),
        "scope_mismatch": preflight.scope_mismatch.clone(),
        "passport_scope_policy": preflight.passport_scope_policy.clone(),
        "explicit_scope_policy": preflight.explicit_scope_policy.clone(),
    })
}

fn mcp_scope_mismatch_message(
    db_path: &Path,
    mismatch: &codegraph_index::ScopeMismatchDetails,
) -> String {
    format!(
        "{} at {}: expected passport_scope_hash={}, observed explicit_scope_hash={}",
        mismatch.message,
        db_path.display(),
        mismatch.expected_scope_hash.as_deref().unwrap_or("unknown"),
        mismatch.observed_scope_hash.as_deref().unwrap_or("unknown")
    )
}

fn trace_id_for(tool: &str) -> String {
    let safe_tool = tool
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{safe_tool}-{}", unix_time_ms())
}

fn trace_repo_root(arguments: &Value, fallback: &Path) -> PathBuf {
    let candidate = arguments
        .as_object()
        .and_then(|args| {
            args.get("repo")
                .or_else(|| args.get("repo_root"))
                .and_then(Value::as_str)
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback.to_path_buf());
    fs::canonicalize(&candidate).unwrap_or(candidate)
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    left == right
}

fn unix_time_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(u128::from(u64::MAX)) as u64,
        Err(_) => 0,
    }
}

fn load_sources(
    repo_root: &Path,
    store: &SqliteGraphStore,
) -> Result<BTreeMap<String, String>, McpServerError> {
    let mut sources = BTreeMap::new();
    for file in store.list_files(10_000)? {
        read_indexed_file(repo_root, &file, &mut sources)?;
    }
    Ok(sources)
}

fn read_indexed_file(
    repo_root: &Path,
    file: &FileRecord,
    sources: &mut BTreeMap<String, String>,
) -> Result<(), McpServerError> {
    let path = repo_root.join(&file.repo_relative_path);
    if path.exists() {
        sources.insert(file.repo_relative_path.clone(), fs::read_to_string(path)?);
    }
    Ok(())
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    static FIXTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("expected Ok(..), got Err({error:?})"),
        }
    }

    fn fixture_repo() -> PathBuf {
        let counter = FIXTURE_COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "codegraph-mcp-server-fixture-{}-{counter}",
            std::process::id(),
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        fs::create_dir_all(root.join("src")).expect("create fixture");
        fs::write(
            root.join("src").join("auth.ts"),
            "export function sanitize(input: string) {\n  return input.trim();\n}\n\nexport function saveUser(email: string) {\n  return email;\n}\n\nexport function login(req: any) {\n  const email = sanitize(req.body.email);\n  saveUser(email);\n  return email;\n}\n",
        )
        .expect("write source");
        root
    }

    fn non_default_scope_repo() -> PathBuf {
        let counter = FIXTURE_COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "codegraph-mcp-server-non-default-scope-{}-{counter}",
            std::process::id(),
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        fs::create_dir_all(root.join("src")).expect("create fixture");
        fs::write(root.join(".gitignore"), "ignored.ts\n").expect("write ignore config");
        fs::write(
            root.join("ignored.ts"),
            "export function ignored_scope_symbol() {\n  return 1;\n}\n",
        )
        .expect("write ignored source");
        fs::write(
            root.join("src").join("visible.ts"),
            "export function visible_scope_symbol() {\n  return ignored_scope_symbol();\n}\n",
        )
        .expect("write visible source");
        root
    }

    fn indexed_server() -> (McpServer, PathBuf, String, String) {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));
        ok(server.call_tool("codegraph.index_repo", &json!({"repo": path_string(&repo)})));
        let login = first_symbol_id(&server, &repo, "login");
        let sanitize = first_symbol_id(&server, &repo, "sanitize");
        (server, repo, login, sanitize)
    }

    fn first_symbol_id(server: &McpServer, repo: &Path, query: &str) -> String {
        let result = ok(server.call_tool(
            "codegraph.search_symbols",
            &json!({"repo": path_string(repo), "query": query}),
        ));
        result["hits"][0]["entity"]["id"]
            .as_str()
            .expect("entity id")
            .to_string()
    }

    fn mutate_db_passport(repo: &Path, mut mutate: impl FnMut(&mut codegraph_store::DbPassport)) {
        let db_path = default_db_path(repo);
        let store = SqliteGraphStore::open(&db_path).expect("open DB");
        let mut passport = store
            .get_db_passport()
            .expect("read passport")
            .expect("passport");
        mutate(&mut passport);
        store
            .upsert_db_passport(&passport)
            .expect("write mutated passport");
    }

    fn assert_blocker_contains(status: &Value, expected: &str) {
        assert!(
            status["blockers"]
                .as_array()
                .expect("blockers")
                .iter()
                .any(|reason| reason.as_str().is_some_and(|text| text.contains(expected))),
            "expected blocker containing {expected:?}: {status:?}"
        );
    }

    #[test]
    fn mcp_server_lists_required_tools_and_schemas() {
        let server = McpServer::new(McpServerConfig::default());
        let tools = server.tool_definitions();

        for name in TOOL_NAMES {
            assert!(
                tools
                    .iter()
                    .any(|tool| tool.get("name").and_then(Value::as_str) == Some(*name)),
                "missing tool {name}"
            );
        }
        for forbidden in ["delete", "remove", "drop", "sql", "cypher"] {
            assert!(
                TOOL_NAMES.iter().all(|name| !name.contains(forbidden)),
                "destructive or direct-query tool leaked: {forbidden}"
            );
        }
        assert!(tools
            .iter()
            .all(|tool| tool.get("inputSchema").and_then(Value::as_object).is_some()));
        assert!(tools.iter().all(|tool| tool
            .get("outputSchema")
            .and_then(Value::as_object)
            .is_some()));
        assert!(tools.iter().all(|tool| {
            tool["annotations"]["readOnlyHint"].as_bool().is_some()
                && tool["annotations"]["destructiveHint"].as_bool() == Some(false)
                && tool["annotations"]["localOnly"].as_bool() == Some(true)
        }));

        let plan_schema = tools
            .iter()
            .find(|tool| tool["name"].as_str() == Some("codegraph.plan_context"))
            .and_then(|tool| tool.get("inputSchema"))
            .expect("plan_context schema");
        let empty = Vec::new();
        let required = plan_schema["required"].as_array().unwrap_or(&empty);
        assert!(
            !required.iter().any(|field| field.as_str() == Some("query")),
            "plan_context should not force search-style query input"
        );
        assert!(plan_schema["properties"]["task"].is_object());
        assert!(plan_schema["properties"]["seeds"].is_object());
    }

    #[test]
    fn mcp_jsonrpc_initialize_and_tools_list_work() {
        let server = McpServer::new(McpServerConfig::default());

        let initialize = server
            .handle_jsonrpc(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}))
            .expect("initialize response");
        assert_eq!(
            initialize["result"]["serverInfo"]["name"].as_str(),
            Some(SERVER_NAME)
        );

        let tools = server
            .handle_jsonrpc(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
            .expect("tools response");
        assert_eq!(
            tools["result"]["tools"].as_array().expect("tools").len(),
            TOOL_NAMES.len()
        );
    }

    #[test]
    fn mcp_resources_and_prompts_are_discoverable() {
        let server = McpServer::new(McpServerConfig::default());

        let resources = server
            .handle_jsonrpc(&json!({"jsonrpc": "2.0", "id": 1, "method": "resources/list"}))
            .expect("resources response");
        assert!(resources["result"]["resources"]
            .as_array()
            .expect("resources")
            .iter()
            .any(|resource| resource["uri"].as_str() == Some("codegraph://schema")));

        let schema = server
            .handle_jsonrpc(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "resources/read",
                "params": {"uri": "codegraph://schema"}
            }))
            .expect("resource read response");
        let text = schema["result"]["contents"][0]["text"]
            .as_str()
            .expect("resource text");
        assert!(text.contains("\"tools\""));
        assert!(text.contains("\"destructiveHint\": false"));
        assert!(text.contains("recommended_workflow"));
        assert!(text.contains("codegraph.plan_context"));

        let prompts = server
            .handle_jsonrpc(&json!({"jsonrpc": "2.0", "id": 3, "method": "prompts/list"}))
            .expect("prompts response");
        assert!(prompts["result"]["prompts"]
            .as_array()
            .expect("prompts")
            .iter()
            .any(|prompt| prompt["name"].as_str() == Some("impact-analysis")));

        let prompt = server
            .handle_jsonrpc(&json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "prompts/get",
                "params": {"name": "impact-analysis"}
            }))
            .expect("prompt get response");
        let prompt_text = prompt["result"]["messages"][0]["content"]["text"]
            .as_str()
            .expect("prompt text");
        assert!(prompt_text.contains("MVP.md"));
        assert!(prompt_text.contains("Do not use subagents"));
        assert_no_subagent_recommendations(prompt_text);
    }

    #[test]
    fn invalid_tool_input_returns_clear_error() {
        let server = McpServer::new(McpServerConfig::default());

        let error = server
            .call_tool("codegraph.search_text", &json!({}))
            .expect_err("missing query should fail");

        assert_eq!(error.code, "invalid_input");
        assert!(error.message.contains("query"));
    }

    #[test]
    fn mcp_trace_logs_request_and_response_jsonl() {
        let repo = fixture_repo();
        let trace_root = repo.join("trace-root");
        let run_id = "mcp-trace-fixture";
        let server = McpServer::new(
            McpServerConfig::for_repo(&repo)
                .with_trace_root(&trace_root)
                .with_trace_run_id(run_id)
                .with_trace_task_id("trace-test"),
        );

        let status = ok(server.call_tool("codegraph.status", &json!({"repo": path_string(&repo)})));
        assert!(matches!(status["status"].as_str(), Some("missing" | "ok")));
        drop(server);

        let events_path = trace_root.join(run_id).join("events.jsonl");
        let contents = fs::read_to_string(&events_path).expect("read trace events");
        let events = contents
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("valid jsonl"))
            .collect::<Vec<_>>();
        assert!(events
            .iter()
            .any(|event| event["event_type"].as_str() == Some("mcp_request")));
        assert!(events
            .iter()
            .any(|event| event["event_type"].as_str() == Some("mcp_response")));
        for event in events {
            for field in [
                "schema_version",
                "run_id",
                "trace_id",
                "task_id",
                "timestamp_unix_ms",
                "repo_id",
                "repo_root",
                "actor",
                "action_kind",
                "tool",
                "status",
                "result_status",
                "latency_ms",
                "token_estimate",
                "evidence_refs",
                "edited_files",
                "test_command",
                "test_status",
                "input_hash",
                "output_hash",
                "truncated",
                "artifact_path",
                "error",
            ] {
                assert!(event.get(field).is_some(), "missing field {field}");
            }
        }

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_status_safe_db_reports_counts_passport_and_scope_source() {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));
        ok(server.call_tool("codegraph.index_repo", &json!({"repo": path_string(&repo)})));

        let status = ok(server.call_tool("codegraph.status", &json!({"repo": path_string(&repo)})));

        assert_eq!(status["status"].as_str(), Some("ok"));
        assert_eq!(status["safe_to_query"].as_bool(), Some(true));
        assert_eq!(status["scope_source"].as_str(), Some("passport"));
        assert_eq!(
            status["db_lifecycle_read"]["scope_source"].as_str(),
            Some("passport")
        );
        assert_eq!(
            status["passport_summary"]["passport_status"].as_str(),
            Some("valid")
        );
        assert_eq!(
            status["passport_summary"]["scope_source"].as_str(),
            Some("passport")
        );
        assert!(status["files"].as_u64().is_some());
        assert!(status["entities"].as_u64().is_some());
        assert!(status["edges"].as_u64().is_some());
        assert!(status["blockers"].as_array().expect("blockers").is_empty());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_status_missing_db_reports_index_required() {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));

        let status = ok(server.call_tool("codegraph.status", &json!({"repo": path_string(&repo)})));

        assert_eq!(status["status"].as_str(), Some("missing"));
        assert_eq!(status["problem"].as_str(), Some("index_required"));
        assert_eq!(status["db_problem"].as_str(), Some("db_missing"));
        assert_eq!(status["safe_to_query"].as_bool(), Some(false));
        assert_blocker_contains(&status, "db_missing");
        assert!(status.get("files").is_none());
        assert!(status.get("entities").is_none());
        assert!(status.get("edges").is_none());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_status_passport_gate_reports_db_problem_for_mismatched_db() {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));
        ok(server.call_tool("codegraph.index_repo", &json!({"repo": path_string(&repo)})));
        let repo_arg = path_string(&repo);
        mutate_db_passport(&repo, |passport| {
            passport.canonical_repo_root = path_string(&repo.join("different-root"));
        });

        let search = server
            .call_tool(
                "codegraph.search_symbols",
                &json!({"repo": repo_arg, "query": "login"}),
            )
            .expect_err("search should reject mismatched passport");
        assert_eq!(search.code, "db_lifecycle_blocked");
        assert!(search.message.contains("repo root mismatch"));

        let status = ok(server.call_tool("codegraph.status", &json!({"repo": repo_arg})));
        assert_eq!(status["status"].as_str(), Some("db_problem"));
        assert_eq!(status["problem"].as_str(), Some("repo_root_mismatch"));
        assert_eq!(status["safe_to_query"].as_bool(), Some(false));
        assert_eq!(
            status["db_health"]["passport_status"].as_str(),
            Some("mismatched")
        );
        assert_eq!(
            status["db_lifecycle_read"]["repo_root_status"].as_str(),
            Some("mismatched")
        );
        assert_blocker_contains(&status, "repo root mismatch");
        assert!(
            status.get("files").is_none() && status.get("entities").is_none(),
            "unsafe status must not pretend direct counts are enough: {status:?}"
        );

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_status_scope_mismatch_reports_scope_mismatch_blocker() {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));
        ok(server.call_tool("codegraph.index_repo", &json!({"repo": path_string(&repo)})));
        mutate_db_passport(&repo, |passport| {
            passport.index_scope_policy_hash = "tampered-scope-policy-hash".to_string();
        });

        let status = ok(server.call_tool("codegraph.status", &json!({"repo": path_string(&repo)})));

        assert_eq!(status["status"].as_str(), Some("db_problem"));
        assert_eq!(status["problem"].as_str(), Some("scope_mismatch"));
        assert_eq!(status["safe_to_query"].as_bool(), Some(false));
        assert_eq!(
            status["db_lifecycle_read"]["scope_status"].as_str(),
            Some("mismatched")
        );
        assert_blocker_contains(&status, "index scope policy hash mismatch");
        assert!(status.get("files").is_none());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_status_schema_mismatch_reports_schema_blocker() {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));
        ok(server.call_tool("codegraph.index_repo", &json!({"repo": path_string(&repo)})));
        mutate_db_passport(&repo, |passport| {
            passport.codegraph_schema_version = passport.codegraph_schema_version.saturating_add(1);
        });

        let status = ok(server.call_tool("codegraph.status", &json!({"repo": path_string(&repo)})));

        assert_eq!(status["status"].as_str(), Some("db_problem"));
        assert_eq!(status["problem"].as_str(), Some("schema_mismatch"));
        assert_eq!(status["safe_to_query"].as_bool(), Some(false));
        assert_eq!(
            status["db_lifecycle_read"]["schema_status"].as_str(),
            Some("mismatched")
        );
        assert_blocker_contains(&status, "passport schema mismatch");
        assert!(status.get("files").is_none());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_status_storage_mismatch_reports_storage_blocker() {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));
        ok(server.call_tool("codegraph.index_repo", &json!({"repo": path_string(&repo)})));
        mutate_db_passport(&repo, |passport| {
            passport.storage_mode = "unknown_storage_mode".to_string();
        });

        let status = ok(server.call_tool("codegraph.status", &json!({"repo": path_string(&repo)})));

        assert_eq!(status["status"].as_str(), Some("db_problem"));
        assert_eq!(status["problem"].as_str(), Some("storage_mismatch"));
        assert_eq!(status["safe_to_query"].as_bool(), Some(false));
        assert_eq!(
            status["db_lifecycle_read"]["storage_mode_status"].as_str(),
            Some("mismatched")
        );
        assert_blocker_contains(&status, "storage mode");
        assert!(status.get("files").is_none());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_non_default_scope_search_analyze_and_status_use_passport_scope() {
        let repo = non_default_scope_repo();
        let db_path = default_db_path(&repo);
        let mut options = IndexOptions::default();
        options.scope.include_ignored = true;
        index_repo_to_db_with_options(&repo, &db_path, options.clone())
            .expect("index non-default scope fixture");
        let expected_scope_hash = scope_policy_hash(&options.scope).expect("scope hash");
        let server = McpServer::new(McpServerConfig::for_repo(&repo));
        let repo_arg = path_string(&repo);

        let search = ok(server.call_tool(
            "codegraph.search",
            &json!({"repo": repo_arg, "query": "ignored_scope_symbol"}),
        ));
        assert_eq!(search["status"].as_str(), Some("ok"));
        assert_eq!(
            search["db_lifecycle_read"]["scope_source"].as_str(),
            Some("passport")
        );
        assert_eq!(
            search["db_lifecycle_read"]["passport_scope_hash"].as_str(),
            Some(expected_scope_hash.as_str())
        );

        let symbols = ok(server.call_tool(
            "codegraph.search_symbols",
            &json!({"repo": repo_arg, "query": "ignored_scope_symbol"}),
        ));
        assert_eq!(
            symbols["db_lifecycle_read"]["scope_source"].as_str(),
            Some("passport")
        );
        let entity_id = symbols["hits"][0]["entity"]["id"]
            .as_str()
            .expect("entity id")
            .to_string();

        let analyze = ok(server.call_tool(
            "codegraph.analyze",
            &json!({"repo": repo_arg, "entity_id": entity_id}),
        ));
        assert_eq!(analyze["status"].as_str(), Some("ok"));
        assert_eq!(
            analyze["db_lifecycle_read"]["scope_source"].as_str(),
            Some("passport")
        );
        assert_eq!(
            analyze["db_lifecycle_read"]["passport_scope_hash"].as_str(),
            Some(expected_scope_hash.as_str())
        );

        let status = ok(server.call_tool("codegraph.status", &json!({"repo": repo_arg})));
        assert_eq!(status["status"].as_str(), Some("ok"));
        assert_eq!(
            status["db_lifecycle_read"]["scope_source"].as_str(),
            Some("passport")
        );
        assert_eq!(
            status["db_lifecycle_read"]["passport_scope_hash"].as_str(),
            Some(expected_scope_hash.as_str())
        );

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_index_repo_uses_shared_compact_indexer_with_external_db() {
        let repo = fixture_repo();
        let db_path = repo.join("external-db").join("mcp.sqlite");
        let server = McpServer::new(
            McpServerConfig::for_repo(&repo)
                .with_db_path(&db_path)
                .without_trace(),
        );

        let result = ok(server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&repo), "db_path": path_string(&db_path)}),
        ));
        assert_eq!(result["status"].as_str(), Some("indexed"));
        assert_eq!(result["files_indexed"].as_u64(), Some(1));
        assert!(db_path.exists());
        assert!(
            !repo.join(".codegraph").exists(),
            "external db indexing must not create repo-local .codegraph state"
        );

        let store = SqliteGraphStore::open(&db_path).expect("store");
        assert_eq!(store.count_files().expect("files"), 1);
        assert!(
            store
                .search_text("sanitize", 10)
                .expect("raw fts")
                .is_empty(),
            "compact MCP indexing must not use the old full file/entity/snippet FTS path"
        );
        drop(store);

        let text = ok(server.call_tool(
            "codegraph.search_text",
            &json!({
                "repo": path_string(&repo),
                "db_path": path_string(&db_path),
                "query": "sanitize"
            }),
        ));
        assert_eq!(text["status"].as_str(), Some("ok"));
        assert!(!text["hits"].as_array().expect("hits").is_empty());
        assert!(text["hits"]
            .as_array()
            .expect("hits")
            .iter()
            .any(|hit| hit["match"].as_str() == Some("source_scan")));

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_and_shared_cli_index_counts_match_on_fixture() {
        let repo = fixture_repo();
        let cli_db = repo.join("target").join("shared-cli.sqlite");
        let mcp_db = repo.join("external-db").join("shared-mcp.sqlite");
        let cli_summary =
            codegraph_index::index_repo_to_db(&repo, &cli_db).expect("shared cli index");
        let server = McpServer::new(
            McpServerConfig::for_repo(&repo)
                .with_db_path(&mcp_db)
                .without_trace(),
        );
        let mcp = ok(server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&repo), "db_path": path_string(&mcp_db)}),
        ));

        assert_eq!(
            mcp["files_indexed"].as_u64(),
            Some(cli_summary.files_indexed as u64)
        );
        assert_eq!(mcp["entities"].as_u64(), Some(cli_summary.entities as u64));
        assert_eq!(mcp["edges"].as_u64(), Some(cli_summary.edges as u64));

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_pagination_and_resource_links_are_returned() {
        let (server, repo, _login, _sanitize) = indexed_server();
        let result = ok(server.call_tool(
            "codegraph.search_text",
            &json!({
                "repo": path_string(&repo),
                "query": "return",
                "limit": 1,
                "offset": 1,
                "mode": "verbose"
            }),
        ));

        assert_eq!(result["status"].as_str(), Some("ok"));
        assert_eq!(result["mode"].as_str(), Some("verbose"));
        assert_eq!(result["pagination"]["offset"].as_u64(), Some(1));
        assert_eq!(result["pagination"]["limit"].as_u64(), Some(1));
        assert_eq!(result["pagination"]["truncated"].as_bool(), Some(true));
        assert!(result["resource_links"]["schema"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("codegraph://")));

        let symbol = ok(server.call_tool(
            "codegraph.search_symbols",
            &json!({"repo": path_string(&repo), "query": "login", "limit": 1}),
        ));
        assert!(symbol["hits"][0]["entity"]["resource_links"]["source_span"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("codegraph://source-span/")));

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn missing_paths_explain_why_no_proof_was_found() {
        let (server, repo, login, _sanitize) = indexed_server();
        let result = ok(server.call_tool(
            "codegraph.trace_path",
            &json!({
                "repo": path_string(&repo),
                "source": login,
                "target": "does-not-exist",
                "relations": ["CALLS"],
                "mode": "explain"
            }),
        ));

        assert_eq!(result["status"].as_str(), Some("ok"));
        assert_eq!(result["paths"].as_array().expect("paths").len(), 0);
        assert_eq!(result["mode"].as_str(), Some("explain"));
        assert_eq!(
            result["explain_missing"]["category"].as_str(),
            Some("no_symbol_found")
        );

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn high_level_tools_return_llm_friendly_json() {
        let (server, repo, login, _sanitize) = indexed_server();

        let search = ok(server.call_tool(
            "codegraph.search",
            &json!({"repo": path_string(&repo), "query": "login", "limit": 3}),
        ));
        assert_eq!(search["status"].as_str(), Some("ok"));
        assert_eq!(search["tool"].as_str(), Some("codegraph.search"));
        assert_eq!(search["repo_context"]["indexed"].as_bool(), Some(true));
        assert!(!search["hits"].as_array().expect("hits").is_empty());
        assert!(search["hits"][0]["proof_quality"].as_object().is_some());
        assert!(search["workflow"]
            .as_array()
            .is_some_and(|steps| steps.len() >= 6));

        let analyze = ok(server.call_tool(
            "codegraph.analyze",
            &json!({
                "repo": path_string(&repo),
                "entity_id": login.clone(),
                "analysis": "impact",
                "limit": 3
            }),
        ));
        assert_eq!(analyze["status"].as_str(), Some("ok"));
        assert_eq!(analyze["tool"].as_str(), Some("codegraph.analyze"));
        assert!(analyze["paths"].as_array().is_some());
        assert!(analyze["pagination"].as_object().is_some());
        assert!(analyze["proof"]
            .as_str()
            .is_some_and(|proof| proof.contains("exact graph")));

        let plan = ok(server.call_tool(
            "codegraph.plan_context",
            &json!({
                "repo": path_string(&repo),
                "task": "Change login safely",
                "seeds": [login],
                "token_budget": 512
            }),
        ));
        assert_eq!(plan["status"].as_str(), Some("ok"));
        assert_eq!(plan["tool"].as_str(), Some("codegraph.plan_context"));
        assert!(plan["packet"].as_object().is_some());
        assert!(plan["packet"]["metadata"]["trace"].as_array().is_some());
        assert!(plan["workflow"].as_array().is_some());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn explain_missing_categories_are_structured() {
        let (server, repo, _login, _sanitize) = indexed_server();

        let no_symbol = ok(server.call_tool(
            "codegraph.explain_missing",
            &json!({"repo": path_string(&repo), "symbol": "definitelyMissingSymbol"}),
        ));
        assert_eq!(no_symbol["status"].as_str(), Some("ok"));
        assert_eq!(no_symbol["category"].as_str(), Some("no_symbol_found"));

        let unsupported = ok(server.call_tool(
            "codegraph.explain_missing",
            &json!({
                "repo": path_string(&repo),
                "language": "rust",
                "relation": "CHECKS_ROLE"
            }),
        ));
        assert_eq!(unsupported["status"].as_str(), Some("ok"));
        assert_eq!(
            unsupported["category"].as_str(),
            Some("relation_unsupported_for_language")
        );
        assert!(unsupported["unsupported_relations"]
            .as_array()
            .is_some_and(|relations| relations.iter().any(|relation| relation == "CHECKS_ROLE")));

        fs::remove_dir_all(repo).expect("cleanup");

        let counter = FIXTURE_COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let rust_repo = std::env::temp_dir().join(format!(
            "codegraph-mcp-server-rust-fixture-{}-{counter}",
            std::process::id(),
        ));
        if rust_repo.exists() {
            fs::remove_dir_all(&rust_repo).expect("remove stale rust fixture");
        }
        fs::create_dir_all(rust_repo.join("src")).expect("create rust fixture");
        fs::write(
            rust_repo.join("src").join("lib.rs"),
            "fn alpha() {\n    beta();\n}\n\nfn beta() {}\n",
        )
        .expect("write rust source");
        let rust_server = McpServer::new(McpServerConfig::for_repo(&rust_repo));
        ok(rust_server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&rust_repo)}),
        ));
        let alpha = first_symbol_id(&rust_server, &rust_repo, "alpha");
        let beta = first_symbol_id(&rust_server, &rust_repo, "beta");
        let no_relation = ok(rust_server.call_tool(
            "codegraph.explain_missing",
            &json!({
                "repo": path_string(&rust_repo),
                "source": alpha,
                "target": beta,
                "relation": "ARGUMENT_1"
            }),
        ));
        assert_eq!(no_relation["status"].as_str(), Some("ok"));
        assert_eq!(
            no_relation["category"].as_str(),
            Some("symbol_found_but_no_matching_relation")
        );

        fs::remove_dir_all(rust_repo).expect("cleanup rust fixture");
    }

    #[test]
    fn each_tool_returns_structured_response_on_fixture_repo() {
        let (server, repo, login, sanitize) = indexed_server();
        let repo_arg = path_string(&repo);
        let calls = [
            (
                "codegraph.search",
                json!({"repo": repo_arg, "query": "login", "limit": 3}),
            ),
            (
                "codegraph.analyze",
                json!({"repo": repo_arg, "entity_id": login, "analysis": "impact", "limit": 3}),
            ),
            (
                "codegraph.plan_context",
                json!({"repo": repo_arg, "task": "Change login", "seeds": [login], "limit": 3}),
            ),
            (
                "codegraph.explain_missing",
                json!({"repo": repo_arg, "symbol": "login"}),
            ),
            ("codegraph.status", json!({"repo": repo_arg})),
            ("codegraph.index_repo", json!({"repo": repo_arg})),
            (
                "codegraph.update_changed_files",
                json!({"repo": repo_arg, "files": ["src/auth.ts"]}),
            ),
            (
                "codegraph.search_symbols",
                json!({"repo": repo_arg, "query": "login"}),
            ),
            (
                "codegraph.search_text",
                json!({"repo": repo_arg, "query": "sanitize"}),
            ),
            (
                "codegraph.search_semantic",
                json!({"repo": repo_arg, "query": "login email"}),
            ),
            (
                "codegraph.context_pack",
                json!({"repo": repo_arg, "task": "Change login", "seeds": [login]}),
            ),
            (
                "codegraph.trace_path",
                json!({"repo": repo_arg, "source": login, "target": sanitize}),
            ),
            (
                "codegraph.impact_analysis",
                json!({"repo": repo_arg, "entity_id": login}),
            ),
            (
                "codegraph.find_callers",
                json!({"repo": repo_arg, "entity_id": login}),
            ),
            (
                "codegraph.find_callees",
                json!({"repo": repo_arg, "entity_id": login}),
            ),
            (
                "codegraph.find_reads",
                json!({"repo": repo_arg, "entity_id": login}),
            ),
            (
                "codegraph.find_writes",
                json!({"repo": repo_arg, "entity_id": login}),
            ),
            (
                "codegraph.find_mutations",
                json!({"repo": repo_arg, "entity_id": login}),
            ),
            (
                "codegraph.find_dataflow",
                json!({"repo": repo_arg, "entity_id": login}),
            ),
            (
                "codegraph.find_auth_paths",
                json!({"repo": repo_arg, "entity_id": login}),
            ),
            (
                "codegraph.find_event_flow",
                json!({"repo": repo_arg, "entity_id": login}),
            ),
            (
                "codegraph.find_tests",
                json!({"repo": repo_arg, "entity_id": login}),
            ),
            (
                "codegraph.find_migrations",
                json!({"repo": repo_arg, "entity_id": login}),
            ),
            (
                "codegraph.explain_path",
                json!({"repo": repo_arg, "source": login, "target": sanitize}),
            ),
        ];

        for (tool, args) in calls {
            let result = ok(server.call_tool(tool, &args));
            assert!(
                matches!(
                    result["status"].as_str(),
                    Some("ok" | "indexed" | "updated")
                ),
                "{tool}: {result}"
            );
        }

        let store = SqliteGraphStore::open(default_db_path(&repo)).expect("store");
        let edge_id = store.list_edges(1).expect("edges")[0].id.clone();
        drop(store);
        let explained = ok(server.call_tool(
            "codegraph.explain_edge",
            &json!({"repo": repo_arg, "edge_id": edge_id}),
        ));
        assert_eq!(explained["status"].as_str(), Some("ok"));

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn context_pack_mcp_call_exposes_runtime_funnel_trace() {
        let (server, repo, login, _sanitize) = indexed_server();
        let seed = login.clone();

        let mcp = ok(server.call_tool(
            "codegraph.context_pack",
            &json!({"repo": path_string(&repo), "task": "Change login", "seeds": [seed.clone()]}),
        ));

        assert_eq!(mcp["status"].as_str(), Some("ok"));
        let trace = mcp["packet"]["metadata"]["trace"]
            .as_array()
            .expect("packet trace");
        for stage in [
            "stage0_exact_seed_extraction",
            "stage1_binary_sieve",
            "stage2_compressed_rerank",
            "stage3_exact_graph_verification",
            "stage4_context_packet",
        ] {
            assert!(
                trace
                    .iter()
                    .any(|entry| entry["stage"].as_str() == Some(stage)),
                "missing {stage}: {trace:?}"
            );
        }
        for stage in ["stage1_binary_sieve", "stage2_compressed_rerank"] {
            let entry = trace_entry(trace, stage);
            assert!(
                json_array_contains_string(&entry["kept"], &seed),
                "exact seed should be kept by {stage}: {entry:?}"
            );
            assert!(
                !json_array_contains_string(&entry["dropped"], &seed),
                "exact seed should not be dropped by {stage}: {entry:?}"
            );
        }
        fs::remove_dir_all(repo).expect("cleanup");
    }

    fn trace_entry<'a>(trace: &'a [Value], stage: &str) -> &'a Value {
        trace
            .iter()
            .find(|entry| entry["stage"].as_str() == Some(stage))
            .unwrap_or_else(|| panic!("missing trace stage {stage}"))
    }

    fn json_array_contains_string(value: &Value, expected: &str) -> bool {
        value
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(expected)))
    }

    fn assert_no_subagent_recommendations(contents: &str) {
        let normalized = contents.to_ascii_lowercase();
        for forbidden in [
            "spawn_agent",
            "sub-agent",
            "parallel agents",
            "delegate to subagents",
            "use subagents to",
            "use subagents for",
            "launch subagents",
        ] {
            assert!(
                !normalized.contains(forbidden),
                "forbidden subagent instruction {forbidden:?} in {contents}"
            );
        }
    }
}
