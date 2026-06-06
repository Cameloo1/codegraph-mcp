//! Local MCP server for CodeGraph.
//!
//! Phase 30 exposes read-mostly, evidence-oriented CodeGraph tools over a
//! minimal stdio JSON-RPC MCP surface. Local index update tools are limited to
//! creating/updating `.codegraph/codegraph.sqlite`; no delete or destructive
//! repository operations are exposed.

#![forbid(unsafe_code)]
#![recursion_limit = "512"]

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    str::FromStr,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use codegraph_core::{
    classify_edge_evidence_role, classify_entity_source_role, classify_validation_finding,
    normalize_repo_relative_path, ContextPacket, ContextSnippet, Edge, Entity, EvidenceRole,
    Exactness, PathEvidence, RelationKind, RetrievalCandidate, SourceSpan, SupportedRelationStatus,
    ValidationBlockingLevel, ValidationClassification, ValidationEvidenceItem,
    ValidationEvidenceKind, ValidationFinding, ValidationLifecycleRequirement,
    ValidationLifecycleState, ValidationPacket, ValidationProofRequirement, ValidationProofStatus,
    ValidationProvenanceRequirement, ValidationReverificationInput, ValidationRule,
    ValidationRuleKind, ValidationSourceRoleRequirement, ValidationSourceSpanRequirement,
};
use codegraph_index::{
    candidate_spool_index_status_for_repo, compute_entity_source_role_delta, default_db_path,
    index_repo_to_db_with_options, inspect_db_lifecycle_preflight, load_vector_chunk_index_json,
    query_candidate_spool_index_for_repo, rtds_dependency_closure_for_changed_paths_to_db,
    snapshot_normalized_facts_for_paths_to_db, update_changed_files_to_db,
    validate_edit_changed_files_preflight_with_scope, validate_vector_chunk_source_bindings,
    vector_chunk_search_hit_to_retrieval_candidate, CandidateSpoolIndexLoad,
    CandidateSpoolIndexQueryResult, DbLifecyclePreflight, EdgeDeltaEntry,
    EntitySourceRoleDeltaOptions, EntitySourceRoleDeltaReport, IndexOptions, IndexScopeOptions,
    NormalizedFactSnapshotOptions, ValidateEditChangedFilesPreflight, VectorChunkIndexBuildOptions,
    UNBOUNDED_STORE_READ_LIMIT, VALIDATE_EDIT_CHANGED_FILES_MAX,
};
use codegraph_parser::language_frontends;
use codegraph_query::{
    plan_task_retrieval, ExactGraphQueryEngine, GraphPath, QueryLimits, RetrievalDocument,
    RetrievalFunnel, RetrievalFunnelConfig, RetrievalFunnelRequest, RetrievalTraceStage,
    VectorCandidateBranchStatus,
};
use codegraph_store::{
    classify_sqlite_access_problem, DbPreflightReport, GraphStore, SqliteGraphStore, TextSearchHit,
    TextSearchKind,
};
use codegraph_trace::{TraceConfig, TraceLogger};
use codegraph_vector::{DeterministicTestEmbeddingProvider, TestEmbeddingEnablement};
use serde::Serialize;
use serde_json::{json, Map, Value};

#[cfg(test)]
use codegraph_index::scope_policy_hash;

pub const SERVER_NAME: &str = "codegraph-mcp";
pub const PHASE: &str = "30";
const PRODUCTION_AGENT_USE_PROFILE_NAME: &str = "production-agent-use";
const MCP_AGENT_USE_PROFILE_DB_FILE_NAME: &str = "production-agent-use.sqlite";
const MCP_AGENT_USE_PUBLISH_STATE_FILE_NAME: &str = "production-agent-use.publish-state.json";
const MCP_AGENT_USE_DELTA_STATE_FILE_NAME: &str = "production-agent-use.delta-state.json";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const EXTERNAL_PROFILE_DB_NOTE: &str =
    "This profile DB is outside the workspace; grant access or choose a workspace-local DB.";
const DEFAULT_RESULT_LIMIT: usize = 20;
const DEFAULT_GRAPH_EDGE_LIMIT: usize = 100_000;
const MCP_VECTOR_INDEX_FILE_NAME: &str = "codegraph-vector-chunks.json";
const MCP_VECTOR_AUDIT_FILE_NAME: &str = "codegraph-vector-audit.json";
const MCP_VECTOR_SOURCE_SCOPE: &str = "context-pack-release-vector-candidates";
const MCP_VECTOR_PROVIDER_DIMENSION: usize = 64;
const MCP_VECTOR_CANDIDATE_TOP_K: usize = 16;
const MCP_CONTEXT_PACK_DEFAULT_LIMIT: usize = 12;
const MCP_CONTEXT_PACK_DOCUMENT_LIMIT: usize = 512;
const MCP_CONTEXT_PACK_EDGE_LIMIT: usize = 4_096;
const MCP_CONTEXT_PACK_SOURCE_FILE_LIMIT: usize = 64;
const MCP_CONTEXT_PACK_SOURCE_BYTE_LIMIT: usize = 256 * 1024;
const MCP_VALIDATE_EDIT_TOOL_NAME: &str = "codegraph.validate_edit";
const MCP_VALIDATE_EDIT_SCHEMA_NAME: &str = "validate_edit_agent_json";
const CG_MVP3_CALLS_DANGLING_TARGET: &str = "CG_MVP3_CALLS_DANGLING_TARGET";
const CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED: &str = "CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED";
const CG_MVP3_IMPORTS_DANGLING_TARGET: &str = "CG_MVP3_IMPORTS_DANGLING_TARGET";
const CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH: &str = "CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH";
const CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING: &str = "CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING";
const CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT: &str =
    "CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT";
const CG_MCP_VALIDATE_EDIT_REJECTED_PATHS: &str = "CG_MCP_VALIDATE_EDIT_REJECTED_PATHS";

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
    MCP_VALIDATE_EDIT_TOOL_NAME,
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
        let trace = TraceConfig::for_repo(&repo_root);
        let db_path = std::env::var_os("CODEGRAPH_DB_PATH")
            .map(PathBuf::from)
            .map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    repo_root.join(path)
                }
            })
            .unwrap_or_else(|| default_db_path(&repo_root));
        Self {
            db_path,
            repo_root,
            max_graph_edges: DEFAULT_GRAPH_EDGE_LIMIT,
            trace_enabled: true,
            trace_run_id: trace.run_id,
            trace_task_id: trace.task_id,
            trace_root: trace.trace_root,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoContext {
    repo_root: PathBuf,
    db_path: PathBuf,
    indexed: bool,
    db_path_outside_workspace: bool,
    outside_workspace_note: Option<String>,
    detected_dbs: Vec<String>,
}

impl RepoContext {
    fn to_json(&self) -> Value {
        json!({
            "repo_root": path_string(&self.repo_root),
            "db_path": path_string(&self.db_path),
            "indexed": self.indexed,
            "db_path_outside_workspace": self.db_path_outside_workspace,
            "outside_workspace_note": self.outside_workspace_note.clone(),
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
            MCP_VALIDATE_EDIT_TOOL_NAME => self.validate_edit(args),
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
                format!("unknown codegraph-mcp tool: {other}"),
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
        request
            .entry("response_mode".to_string())
            .or_insert_with(|| Value::String("verbose".to_string()));
        let pack = self.context_pack(&request)?;
        let staged_availability = pack
            .get("staged_availability")
            .cloned()
            .or_else(|| {
                pack.pointer("/packet/metadata/staged_availability")
                    .cloned()
            })
            .unwrap_or_else(|| json!({}));
        let mut value = json!({
            "status": "ok",
            "tool": "codegraph.plan_context",
            "repo_context": context.to_json(),
            "db_lifecycle_read": pack["db_lifecycle_read"].clone(),
            "packet": pack["packet"].clone(),
            "recovery_commands": pack.get("recovery_commands").cloned().unwrap_or_else(|| json!([])),
            "workflow": recommended_workflow(),
            "recommended_next": [
                "Use the packet snippets and verified paths for planning.",
                "Edit only after reviewing exactness and source spans.",
                "Call codegraph.update_changed_files after edits."
            ],
            "proof": "Plan context uses the same Stage 0-4 runtime funnel as codegraph.context_pack.",
        });
        mcp_merge_json_object(
            &mut value,
            mcp_staged_top_level_fields(&staged_availability),
        );
        mcp_attach_dirty_evidence_output_fields(&mut value, "codegraph.plan_context", false);
        Ok(value)
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
        let explicit_scope = mcp_explicit_scope_policy(args)?;
        let preflight = inspect_db_lifecycle_preflight(&repo_root, &db_path, explicit_scope)
            .map_err(ToolCallError::from)?;
        let sqlite_sidecars =
            mcp_sqlite_sidecars_status_from_health(&db_path, &preflight.db_health);
        let staged_availability =
            mcp_staged_availability(&repo_root, &db_path, Some(&preflight), args, None);
        let staged_fields = mcp_staged_top_level_fields(&staged_availability);
        if !preflight.safe {
            let problem = mcp_db_problem_kind(&preflight);
            let status = if problem == "db_missing" {
                "missing"
            } else {
                "db_problem"
            };
            let problem_value = if problem == "db_missing" {
                "index_required"
            } else {
                problem
            };
            let suggested_next = if problem == "db_missing" {
                "Call codegraph.index_repo with the shown repo/db_path before search or analysis."
            } else if preflight.db_path_outside_workspace {
                "Grant filesystem access to the configured DB path or choose a workspace-local DB, then retry."
            } else {
                "Call codegraph.index_repo to rebuild the unsafe DB before search or analysis."
            };
            let mut value = json!({
                "status": status,
                "problem": problem_value,
                "db_problem": problem,
                "safe_to_query": false,
                "server": SERVER_NAME,
                "phase": PHASE,
                "repo_root": path_string(&repo_root),
                "db_path": path_string(&db_path),
                "db_path_outside_workspace": preflight.db_path_outside_workspace,
                "outside_workspace_note": preflight.outside_workspace_note.clone(),
                "path_access_status": preflight.path_access_status.clone(),
                "path_access_error": preflight.path_access_error.clone(),
                "repo_context": context.to_json(),
                "db_health": preflight.db_health.clone(),
                "sqlite_sidecars": sqlite_sidecars.clone(),
                "sidecar_status": sqlite_sidecars["sidecar_status"].clone(),
                "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
                "passport_summary": mcp_passport_summary_json(&preflight),
                "scope_source": preflight.scope_source.clone(),
                "blockers": preflight.blockers.clone(),
                "warnings": preflight.warnings.clone(),
                "suggested_next": suggested_next,
                "read_mostly": true,
                "workflow": "single-agent-only",
            });
            mcp_merge_json_object(&mut value, staged_fields);
            mcp_attach_rtds_freshness_fields(
                &mut value,
                &repo_root,
                &db_path,
                Some(&preflight),
                &staged_availability,
            );
            mcp_attach_dirty_evidence_output_fields(&mut value, "codegraph.status", false);
            return Ok(value);
        }

        let explicit_scope = mcp_explicit_scope_policy(args)?;
        let preflight = inspect_db_lifecycle_preflight(&repo_root, &db_path, explicit_scope)
            .map_err(ToolCallError::from)?;
        if !preflight.safe {
            let problem = mcp_db_problem_kind(&preflight);
            let mut value = json!({
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
            });
            let staged_availability =
                mcp_staged_availability(&repo_root, &db_path, Some(&preflight), args, None);
            mcp_merge_json_object(
                &mut value,
                mcp_staged_top_level_fields(&staged_availability),
            );
            mcp_attach_rtds_freshness_fields(
                &mut value,
                &repo_root,
                &db_path,
                Some(&preflight),
                &staged_availability,
            );
            mcp_attach_dirty_evidence_output_fields(&mut value, "codegraph.status", false);
            return Ok(value);
        }

        let store = SqliteGraphStore::open_read_only(&db_path).map_err(mcp_store_error)?;
        let relation_facts = store.count_edges().map_err(mcp_store_error)?;
        let mut value = json!({
            "status": "ok",
            "safe_to_query": true,
            "server": SERVER_NAME,
            "phase": PHASE,
            "repo_root": path_string(&repo_root),
            "db_path": path_string(&db_path),
            "db_path_outside_workspace": preflight.db_path_outside_workspace,
            "outside_workspace_note": preflight.outside_workspace_note.clone(),
            "path_access_status": preflight.path_access_status.clone(),
            "path_access_error": preflight.path_access_error.clone(),
            "repo_context": context.to_json(),
            "db_health": preflight.db_health.clone(),
            "sqlite_sidecars": sqlite_sidecars.clone(),
            "sidecar_status": sqlite_sidecars["sidecar_status"].clone(),
            "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
            "passport_summary": mcp_passport_summary_json(&preflight),
            "scope_source": preflight.scope_source.clone(),
            "blockers": preflight.blockers.clone(),
            "warnings": preflight.warnings.clone(),
            "schema_version": store.schema_version().map_err(mcp_store_error)?,
            "files": store.count_files().map_err(mcp_store_error)?,
            "entities": store.count_entities().map_err(mcp_store_error)?,
            "relation_facts": relation_facts,
            "release_reported_relation_facts": relation_facts,
            "edges": relation_facts,
            "metric_label_notes": {
                "relation_facts": "Aggregate reported relation facts across the active store surface; not a raw table-row label.",
                "edges": "Deprecated compatibility alias for relation_facts."
            },
            "read_mostly": true,
            "destructive_tools": false,
            "workflow": "single-agent-only",
        });
        mcp_merge_json_object(&mut value, staged_fields);
        mcp_attach_rtds_freshness_fields(
            &mut value,
            &repo_root,
            &db_path,
            Some(&preflight),
            &staged_availability,
        );
        mcp_attach_dirty_evidence_output_fields(&mut value, "codegraph.status", false);
        Ok(value)
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
            "validation_packet_status": "not_applicable",
            "validation_findings_status": "not_applicable",
            "validation_findings_reason": "MVP3.3 validation packets are emitted by the canonical production agent-use watch --once --changed JSON surface; raw MCP update_changed_files calls the shared indexer only.",
            "canonical_validation_surface": "codegraph-mcp agent-use watch --repo <repo> --once --changed <path> --json",
            "hard_interrupt_available": false,
            "hard_interrupt_not_implemented": true,
            "note": "Changed-file updates use the shared compact indexer path and prune stale facts before localized re-indexing.",
        }))
    }

    fn validate_edit(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let total_start = Instant::now();
        let repo_root = self.validate_edit_repo_root(args)?;
        let (db_path, db_source, external_db_used) =
            self.validate_edit_db_path(args, &repo_root)?;
        let mode = mcp_validate_edit_mode(args)?;
        let fail_on_blocking = optional_bool_arg(args, "fail_on_blocking")?.unwrap_or(false);
        let expected_touched_files = optional_string_array(args, "expected_touched_files")?;
        let task_id = optional_string(args, "task_id");
        let edit_intent = optional_string(args, "edit_intent");
        let max_output_bytes = optional_nullable_usize(args, "max_output_bytes")?;
        let changed_files = required_validate_edit_changed_files(args)?;
        let changed_paths = changed_files.iter().map(PathBuf::from).collect::<Vec<_>>();
        let normal_dot_codegraph = repo_root.join(".codegraph");
        let normal_dot_codegraph_existed_before = normal_dot_codegraph.exists();
        let explicit_scope = mcp_explicit_scope_policy(args)?;

        let preflight_start = Instant::now();
        let preflight = inspect_db_lifecycle_preflight(&repo_root, &db_path, explicit_scope)
            .map_err(ToolCallError::from)?;
        let preflight_ms = preflight_start.elapsed().as_millis();
        let scope_policy = preflight
            .effective_scope_policy
            .clone()
            .unwrap_or_else(IndexScopeOptions::default);
        let path_preflight = validate_edit_changed_files_preflight_with_scope(
            &repo_root,
            &changed_paths,
            &scope_policy,
            VALIDATE_EDIT_CHANGED_FILES_MAX,
        );

        if !path_preflight.should_update {
            let status = if path_preflight.rejected_paths.is_empty() {
                "no_op"
            } else {
                "rejected"
            };
            let reason = if path_preflight.rejected_paths.is_empty() {
                "changed_files were all duplicate/editor-temp/no-op inputs; no DB update ran"
            } else {
                "changed_files did not contain a usable in-repo updateable path; no DB update ran"
            };
            let packet = mcp_validate_edit_diagnostic_packet(
                path_preflight.accepted_paths.clone(),
                &db_path,
                &preflight,
                CG_MCP_VALIDATE_EDIT_REJECTED_PATHS,
                reason,
                json!(path_preflight.diagnostics.clone()),
            );
            let timings = json!({
                "preflight_ms": preflight_ms,
                "update_ms": 0,
                "validation_ms": 0,
                "total_ms": total_start.elapsed().as_millis(),
            });
            return mcp_validate_edit_response(
                &repo_root,
                &db_path,
                &db_source,
                external_db_used,
                status,
                &mode,
                fail_on_blocking,
                task_id,
                edit_intent,
                expected_touched_files,
                max_output_bytes,
                &path_preflight,
                &preflight,
                &packet,
                Value::Null,
                json!({}),
                timings,
                normal_dot_codegraph_existed_before != normal_dot_codegraph.exists(),
            );
        }

        if !preflight.safe {
            let packet = mcp_validate_edit_diagnostic_packet(
                path_preflight.accepted_paths.clone(),
                &db_path,
                &preflight,
                CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT,
                "DB lifecycle preflight blocked validate-edit before update",
                json!(preflight.blockers.clone()),
            );
            let timings = json!({
                "preflight_ms": preflight_ms,
                "update_ms": 0,
                "validation_ms": 0,
                "total_ms": total_start.elapsed().as_millis(),
            });
            return mcp_validate_edit_response(
                &repo_root,
                &db_path,
                &db_source,
                external_db_used,
                "preflight_blocked",
                &mode,
                fail_on_blocking,
                task_id,
                edit_intent,
                expected_touched_files,
                max_output_bytes,
                &path_preflight,
                &preflight,
                &packet,
                Value::Null,
                json!({}),
                timings,
                normal_dot_codegraph_existed_before != normal_dot_codegraph.exists(),
            );
        }

        let update_start = Instant::now();
        let update_paths = path_preflight.accepted_pathbufs(&repo_root);
        let snapshot_options = NormalizedFactSnapshotOptions {
            include_text_evidence: true,
            include_path_evidence: true,
            include_sidecar_freshness: true,
            ..NormalizedFactSnapshotOptions::default()
        };
        let pre_update_dependency_closure_start = Instant::now();
        let pre_update_dependency_closure =
            rtds_dependency_closure_for_changed_paths_to_db(&repo_root, &update_paths, &db_path)
                .map_err(ToolCallError::from)?;
        let pre_update_dependency_closure_ms =
            pre_update_dependency_closure_start.elapsed().as_millis();
        let pre_update_requested_set = pre_update_dependency_closure
            .requested_changed_files
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let pre_update_closure_paths = pre_update_dependency_closure
            .closure_files_considered
            .iter()
            .filter(|path| !pre_update_requested_set.contains(*path))
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let old_snapshot_start = Instant::now();
        let old_graph_delta_snapshot = snapshot_normalized_facts_for_paths_to_db(
            &repo_root,
            &update_paths,
            &pre_update_closure_paths,
            &db_path,
            snapshot_options.clone(),
        )
        .map_err(ToolCallError::from)?;
        let snapshot_old_ms = old_snapshot_start.elapsed().as_millis();

        let summary = update_changed_files_to_db(&repo_root, &update_paths, &db_path)
            .map_err(ToolCallError::from)?;
        let update_ms = update_start.elapsed().as_millis();
        let source_update = serde_json::to_value(&summary).map_err(|error| {
            ToolCallError::new(
                "serialization_failed",
                format!("could not encode validate-edit update summary: {error}"),
            )
        })?;

        let post_preflight_start = Instant::now();
        let post_preflight = inspect_db_lifecycle_preflight(&repo_root, &db_path, None)
            .map_err(ToolCallError::from)?;
        let post_preflight_ms = post_preflight_start.elapsed().as_millis();

        let validation_start = Instant::now();
        let packet = if post_preflight.safe {
            let delta_requested_paths = if summary
                .dependency_closure
                .requested_changed_files
                .is_empty()
            {
                summary.changed_files.clone()
            } else {
                summary.dependency_closure.requested_changed_files.clone()
            };
            let delta_requested_set = delta_requested_paths
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            let delta_changed_paths = delta_requested_paths
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            let delta_closure_paths = summary
                .dependency_closure
                .closure_files_updated
                .iter()
                .filter(|path| !delta_requested_set.contains(*path))
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            let new_snapshot_start = Instant::now();
            let new_graph_delta_snapshot = snapshot_normalized_facts_for_paths_to_db(
                &repo_root,
                &delta_changed_paths,
                &delta_closure_paths,
                &db_path,
                snapshot_options,
            )
            .map_err(ToolCallError::from)?;
            let snapshot_new_ms = new_snapshot_start.elapsed().as_millis();
            let delta_compute_start = Instant::now();
            let mut graph_delta_report = compute_entity_source_role_delta(
                &old_graph_delta_snapshot,
                &new_graph_delta_snapshot,
                EntitySourceRoleDeltaOptions {
                    max_items_per_category: usize::MAX,
                },
            );
            graph_delta_report.apply_dependency_closure_summary(&summary.dependency_closure);
            graph_delta_report.timings.diff_closure_ms = pre_update_dependency_closure_ms;
            let delta_compute_ms = delta_compute_start.elapsed().as_millis();
            let store = SqliteGraphStore::open_read_only(&db_path).map_err(mcp_store_error)?;
            mcp_validate_edit_validation_packet(
                &repo_root,
                &store,
                path_preflight.accepted_paths.clone(),
                &post_preflight,
                source_update.clone(),
                &graph_delta_report,
                json!({
                    "pre_update_dependency_closure_ms": pre_update_dependency_closure_ms,
                    "snapshot_old_ms": snapshot_old_ms,
                    "snapshot_new_ms": snapshot_new_ms,
                    "delta_compute_ms": delta_compute_ms,
                }),
            )?
        } else {
            mcp_validate_edit_diagnostic_packet(
                path_preflight.accepted_paths.clone(),
                &db_path,
                &post_preflight,
                CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT,
                "DB lifecycle preflight blocked validate-edit after update",
                json!(post_preflight.blockers.clone()),
            )
        };
        let validation_ms = validation_start.elapsed().as_millis();
        let timings = json!({
            "preflight_ms": preflight_ms,
            "pre_update_dependency_closure_ms": pre_update_dependency_closure_ms,
            "snapshot_old_ms": snapshot_old_ms,
            "update_ms": update_ms,
            "post_update_preflight_ms": post_preflight_ms,
            "validation_ms": validation_ms,
            "total_ms": total_start.elapsed().as_millis(),
        });
        let packet_status = serde_json::to_value(packet.status)
            .ok()
            .and_then(|value| value.as_str().map(ToString::to_string))
            .unwrap_or_else(|| "unknown".to_string());
        let status = if packet.hard_interrupt_available {
            "blocking_graph_error".to_string()
        } else {
            packet_status
        };
        mcp_validate_edit_response(
            &repo_root,
            &db_path,
            &db_source,
            external_db_used,
            &status,
            &mode,
            fail_on_blocking,
            task_id,
            edit_intent,
            expected_touched_files,
            max_output_bytes,
            &path_preflight,
            &post_preflight,
            &packet,
            source_update,
            json!({}),
            timings,
            normal_dot_codegraph_existed_before != normal_dot_codegraph.exists(),
        )
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
            "proof": "MCP semantic search returns deterministic token-projection/text candidate recall only; it is not a learned semantic embedding claim and not graph proof.",
        }))
    }

    fn context_pack(&self, args: &Map<String, Value>) -> Result<Value, ToolCallError> {
        let task = required_string(args, "task")?;
        let mode = optional_string(args, "mode").unwrap_or_else(|| "impact".to_string());
        let response_mode = mcp_context_pack_response_mode(args)?;
        let response_limit = optional_limit(args)?.min(MCP_CONTEXT_PACK_DEFAULT_LIMIT);
        let token_budget = optional_usize(args, "token_budget", 2_000, 32, 100_000)?;
        let seeds = optional_string_array(args, "seeds")?;
        let stage0_candidates = optional_string_array(args, "stage0_candidates")?;
        let enable_vector_candidates = optional_bool_arg(args, "enable_vector_candidates")?
            .unwrap_or(false)
            || optional_bool_arg(args, "enableVectorCandidates")?.unwrap_or(false);
        let enable_nuance_rescue_candidates =
            optional_bool_arg(args, "enable_nuance_rescue_candidates")?.unwrap_or(false)
                || optional_bool_arg(args, "enableNuanceRescueCandidates")?.unwrap_or(false);
        let vector_index_path_arg = optional_string(args, "vector_index")
            .or_else(|| optional_string(args, "vector_index_path"))
            .or_else(|| optional_string(args, "vector_runtime_sidecar"))
            .or_else(|| optional_string(args, "vectorIndex"))
            .or_else(|| optional_string(args, "vectorIndexPath"))
            .or_else(|| optional_string(args, "vectorRuntimeSidecar"));
        let candidate_spool_path_arg = optional_string(args, "candidate_spool")
            .or_else(|| optional_string(args, "candidate_spool_path"))
            .or_else(|| optional_string(args, "candidateSpool"))
            .or_else(|| optional_string(args, "candidateSpoolPath"));
        let enable_candidate_spool = optional_bool_arg(args, "enable_candidate_spool")?
            .unwrap_or(false)
            || optional_bool_arg(args, "early_candidates")?.unwrap_or(false)
            || optional_bool_arg(args, "enableCandidateSpool")?.unwrap_or(false)
            || optional_bool_arg(args, "earlyCandidates")?.unwrap_or(false)
            || candidate_spool_path_arg.is_some();
        let allow_stale_candidate_spool = optional_bool_arg(args, "allow_stale_candidate_spool")?
            .unwrap_or(false)
            || optional_bool_arg(args, "allowStaleCandidateSpool")?.unwrap_or(false);
        let repo_root = self.repo_root(args)?;
        let db_path = self.db_path(args, &repo_root)?;
        let explicit_scope = mcp_explicit_scope_policy(args)?;
        let preflight = inspect_db_lifecycle_preflight(&repo_root, &db_path, explicit_scope)
            .map_err(ToolCallError::from)?;
        if !preflight.safe {
            if enable_candidate_spool {
                let spool_path = mcp_resolve_artifact_path(
                    candidate_spool_path_arg.as_deref(),
                    mcp_default_candidate_spool_path(&repo_root),
                );
                return mcp_candidate_spool_context_pack(
                    &task,
                    &mode,
                    response_mode.as_str(),
                    response_limit,
                    &repo_root,
                    &db_path,
                    &spool_path,
                    allow_stale_candidate_spool,
                    Some(mcp_db_problem_kind(&preflight).to_string()),
                );
            }
            return Err(mcp_unsafe_preflight_error(&db_path, &preflight));
        }
        let store = SqliteGraphStore::open_read_only(&db_path).map_err(mcp_store_error)?;
        let vector_branch = enable_vector_candidates.then(|| {
            load_mcp_vector_branch(
                &repo_root,
                &store,
                &db_path,
                vector_index_path_arg.as_deref(),
                &task,
            )
        });
        let (documents, document_metrics) =
            retrieval_documents_for_context_pack(&store, &task, &seeds)?;
        let document_paths = retrieval_document_paths(&documents);
        let (sources, source_metrics) =
            load_sources_for_paths(&repo_root, &document_paths).map_err(ToolCallError::from)?;
        let mut config = RetrievalFunnelConfig::default();
        config.vector_candidate_top_k = MCP_VECTOR_CANDIDATE_TOP_K;
        let edges = store
            .list_edges(MCP_CONTEXT_PACK_EDGE_LIMIT)
            .map_err(mcp_store_error)?;
        let edge_count = edges.len();
        let funnel = RetrievalFunnel::new(edges, documents, config)
            .map_err(|error| ToolCallError::new("retrieval_funnel_failed", error.to_string()))?;
        let read_path_metrics =
            mcp_context_pack_read_path_metrics_json(&document_metrics, &source_metrics, edge_count);
        let stage0_docs = stage0_candidates
            .iter()
            .map(|candidate| RetrievalDocument::new(candidate, candidate).stage0_score(1.0))
            .collect::<Vec<_>>();
        let mut request = RetrievalFunnelRequest::new(task.clone(), mode, token_budget)
            .exact_seeds(seeds)
            .stage0_candidates(stage0_docs)
            .sources(sources);
        if let Some(vector_branch) = &vector_branch {
            request = request
                .enable_vector_candidates(true)
                .vector_candidate_diagnostics(true)
                .vector_branch_status(vector_branch.status.clone())
                .vector_candidates(vector_branch.candidates.clone());
        }
        if enable_nuance_rescue_candidates {
            request = request.enable_nuance_rescue_candidates(true);
        }
        let result = funnel
            .run(request)
            .map_err(|error| ToolCallError::new("retrieval_funnel_failed", error.to_string()))?;
        let mut packet = result.packet;
        let nuance_rescue_diagnostics = mcp_nuance_rescue_diagnostics(
            enable_nuance_rescue_candidates,
            &result.nuance_rescue_candidates,
            &result.trace,
        );
        let vector_candidate_diagnostics = if let Some(vector_branch) = &vector_branch {
            let mut trace = packet
                .metadata
                .get("vector_candidate_trace")
                .cloned()
                .unwrap_or_else(|| mcp_vector_trace_fallback(vector_branch));
            if let Some(object) = trace.as_object_mut() {
                object.insert(
                    "vector_index_path".to_string(),
                    json!(path_string(&vector_branch.index_path)),
                );
                object.insert(
                    "warning".to_string(),
                    vector_branch
                        .warning
                        .as_ref()
                        .map(|warning| json!(warning))
                        .unwrap_or(Value::Null),
                );
            }
            packet
                .metadata
                .insert("vector_candidate_trace".to_string(), trace.clone());
            trace
        } else {
            Value::Null
        };
        let staged_availability = mcp_staged_availability(
            &repo_root,
            &db_path,
            Some(&preflight),
            args,
            vector_branch.as_ref(),
        );
        packet.metadata.insert(
            "staged_availability".to_string(),
            staged_availability.clone(),
        );
        let (task_intent, task_profile, retrieval_plan) = plan_task_retrieval(&task);

        if response_mode == "verbose" || response_mode == "explain" {
            let staged_fields = mcp_staged_top_level_fields(&staged_availability);
            let mut value = json!({
                "status": "ok",
                "schema_version": 1,
                "command": "codegraph.context_pack",
                "response_mode": response_mode,
                "task": task,
                "task_intent": task_intent.to_json(),
                "task_profile": task_profile.to_json(),
                "retrieval_plan": retrieval_plan.to_json(),
                "retrieval_plan_summary": retrieval_plan.summary_json(),
                "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
                "packet": packet,
                "funnel_trace": result.trace.iter().map(retrieval_trace_stage_json).collect::<Vec<_>>(),
                "vector_candidate_diagnostics": vector_candidate_diagnostics,
                "nuance_rescue_diagnostics": nuance_rescue_diagnostics,
                "read_path_metrics": read_path_metrics,
                "staged_availability": staged_availability.clone(),
                "proof": "Context packet is built through Stage 0 exact seeds, Stage 1 binary sieve, Stage 2 compressed rerank, Stage 3 exact graph verification, and Stage 4 packet emission.",
            });
            mcp_merge_json_object(&mut value, staged_fields);
            mcp_attach_rtds_freshness_fields(
                &mut value,
                &repo_root,
                &db_path,
                Some(&preflight),
                &staged_availability,
            );
            mcp_attach_dirty_evidence_output_fields(
                &mut value,
                "codegraph.context_pack",
                response_mode == "explain",
            );
            return Ok(value);
        }

        let mut compact = mcp_context_pack_compact_json(
            &task,
            &packet,
            &preflight,
            response_limit,
            vector_candidate_diagnostics,
            nuance_rescue_diagnostics,
            staged_availability.clone(),
        );
        if let Some(object) = compact.as_object_mut() {
            object.insert("read_path_metrics".to_string(), read_path_metrics);
        }
        mcp_attach_rtds_freshness_fields(
            &mut compact,
            &repo_root,
            &db_path,
            Some(&preflight),
            &staged_availability,
        );
        mcp_attach_dirty_evidence_output_fields(&mut compact, "codegraph.context_pack", false);
        Ok(compact)
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
        let limits = query_limits(args)?;
        let (store, preflight) = self.open_store_with_preflight(args)?;
        let (entity_id, resolution_mode, exact_entity, ambiguous_symbol_matches, suggestion) =
            self.resolve_relation_query_entity(args, &store, query_name)?;
        if !ambiguous_symbol_matches.is_empty() {
            return Ok(json!({
                "status": "ambiguous_symbol",
                "query": query_name,
                "resolution_mode": resolution_mode,
                "entity_id": null,
                "exact_resolved_entity": null,
                "exact_resolved_entity_results": [],
                "fuzzy_or_global_results": [],
                "ambiguous_symbol_matches": ambiguous_symbol_matches,
                "suggestion": suggestion,
                "db_lifecycle_read": mcp_db_lifecycle_preflight_json(&preflight),
                "proof": "Caller/callee MCP tools require one exact entity id before returning traversal proof.",
            }));
        }
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
            let paths = object.get("paths").cloned().unwrap_or_else(|| json!([]));
            object.insert("entity_id".to_string(), json!(entity_id));
            object.insert("resolution_mode".to_string(), json!(resolution_mode));
            object.insert(
                "exact_resolved_entity".to_string(),
                exact_entity
                    .as_ref()
                    .map(entity_json)
                    .unwrap_or(Value::Null),
            );
            object.insert("exact_resolved_entity_results".to_string(), paths);
            object.insert("fuzzy_or_global_results".to_string(), json!([]));
            object.insert("ambiguous_symbol_matches".to_string(), json!([]));
            object.insert(
                "db_lifecycle_read".to_string(),
                mcp_db_lifecycle_preflight_json(&preflight),
            );
        }
        Ok(value)
    }

    fn resolve_relation_query_entity(
        &self,
        args: &Map<String, Value>,
        store: &SqliteGraphStore,
        query_name: &str,
    ) -> Result<(String, &'static str, Option<Entity>, Vec<Value>, Value), ToolCallError> {
        if let Some(entity_id) =
            optional_string(args, "entity_id").or_else(|| optional_string(args, "id"))
        {
            let exact_entity = store.get_entity(&entity_id).map_err(mcp_store_error)?;
            return Ok((
                entity_id,
                "entity_id",
                exact_entity,
                Vec::new(),
                Value::Null,
            ));
        }

        let Some(query) =
            optional_string(args, "query").or_else(|| optional_string(args, "symbol"))
        else {
            return Err(ToolCallError::new(
                "invalid_input",
                "entity_id, id, query, or symbol is required",
            ));
        };
        let hits = store
            .find_entities_by_exact_symbol(&query)
            .map_err(mcp_store_error)?;
        match hits.len() {
            1 => {
                let entity = hits[0].clone();
                Ok((
                    entity.id.clone(),
                    "exact_resolved",
                    Some(entity),
                    Vec::new(),
                    Value::Null,
                ))
            }
            0 => Err(ToolCallError::new(
                "not_found",
                format!("no indexed entity matched symbol/query: {query}"),
            )),
            _ => Ok((
                String::new(),
                "ambiguous_symbol",
                None,
                hits.iter().map(entity_json).collect(),
                json!({
                    "reason": "Symbol resolved to multiple entities; choose one candidate id for exact caller/callee proof.",
                    "rerun": format!("codegraph.find_{query_name} with entity_id=<candidate-id>"),
                }),
            )),
        }
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
        match hits.len() {
            1 => Ok(hits[0].id.clone()),
            0 => Err(ToolCallError::new(
                "not_found",
                format!("no indexed entity matched symbol/query: {query}"),
            )),
            _ => Err(ToolCallError::new(
                "ambiguous_symbol",
                format!(
                    "symbol/query matched multiple entities; pass entity_id explicitly: {}",
                    hits.iter()
                        .map(|entity| entity.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
        }
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
                format!("unknown codegraph-mcp resource: {other}"),
            )),
        }
    }

    fn context_discovery(&self, args: &Map<String, Value>) -> Result<RepoContext, ToolCallError> {
        let repo_root = self.repo_root(args)?;
        let db_path = self.db_path(args, &repo_root)?;
        let indexed = db_path_present_or_inaccessible(&db_path);
        let db_path_outside_workspace = mcp_db_path_outside_workspace(&db_path, &repo_root);
        let outside_workspace_note =
            db_path_outside_workspace.then(|| EXTERNAL_PROFILE_DB_NOTE.to_string());
        let detected_dbs = detect_indexed_repos(&repo_root);
        Ok(RepoContext {
            repo_root,
            db_path,
            indexed,
            db_path_outside_workspace,
            outside_workspace_note,
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

    fn validate_edit_repo_root(&self, args: &Map<String, Value>) -> Result<PathBuf, ToolCallError> {
        let root = PathBuf::from(required_string(args, "repo")?);
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

    fn validate_edit_db_path(
        &self,
        args: &Map<String, Value>,
        repo_root: &Path,
    ) -> Result<(PathBuf, String, bool), ToolCallError> {
        if let Some(profile) = optional_string(args, "profile") {
            if profile != PRODUCTION_AGENT_USE_PROFILE_NAME {
                return Err(ToolCallError::new(
                    "unsupported_profile",
                    format!(
                        "codegraph.validate_edit supports only profile={PRODUCTION_AGENT_USE_PROFILE_NAME}; got {profile}"
                    ),
                ));
            }
        }

        if let Some(path) = optional_string(args, "db").or_else(|| optional_string(args, "db_path"))
        {
            let db_path = if Path::new(&path).is_absolute() {
                PathBuf::from(path)
            } else {
                repo_root.join(path)
            };
            let external = mcp_db_path_outside_workspace(&db_path, repo_root);
            return Ok((db_path, "explicit_db_argument".to_string(), external));
        }

        if paths_equivalent(repo_root, &self.config.repo_root)
            && mcp_db_looks_like_agent_use_profile(&self.config.db_path)
            && !paths_equivalent(&self.config.db_path, &default_db_path(repo_root))
        {
            let external = mcp_db_path_outside_workspace(&self.config.db_path, repo_root);
            return Ok((
                self.config.db_path.clone(),
                "production_agent_use_profile".to_string(),
                external,
            ));
        }

        Err(ToolCallError::new(
            "profile_db_unresolved",
            "codegraph.validate_edit requires an explicit db/db_path or a configured production-agent-use profile DB; it never falls back to repo-local .codegraph",
        ))
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
        let explicit_scope = mcp_explicit_scope_policy(args)?;
        let preflight = inspect_db_lifecycle_preflight(&repo_root, &db_path, explicit_scope)
            .map_err(ToolCallError::from)?;
        if !preflight.safe {
            return Err(mcp_unsafe_preflight_error(&db_path, &preflight));
        }
        let store = SqliteGraphStore::open_read_only(&db_path).map_err(mcp_store_error)?;
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

#[derive(Debug, Clone)]
struct McpVectorBranch {
    index_path: PathBuf,
    status: VectorCandidateBranchStatus,
    candidates: Vec<RetrievalCandidate>,
    warning: Option<String>,
}

fn load_mcp_vector_branch(
    repo_root: &Path,
    store: &SqliteGraphStore,
    db_path: &Path,
    explicit_index_path: Option<&str>,
    task: &str,
) -> McpVectorBranch {
    let index_path = mcp_vector_index_path(db_path, explicit_index_path);
    if !index_path.exists() {
        return McpVectorBranch {
            index_path,
            status: VectorCandidateBranchStatus::Missing,
            candidates: Vec::new(),
            warning: Some("vector index missing; continuing without vector candidates".to_string()),
        };
    }

    let provider = match mcp_vector_provider() {
        Ok(provider) => provider,
        Err(error) => {
            return McpVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: format!("deterministic vector provider unavailable: {error}"),
                },
                candidates: Vec::new(),
                warning: Some("deterministic vector provider unavailable".to_string()),
            };
        }
    };
    let passport = match store.get_db_passport() {
        Ok(Some(passport)) => passport,
        Ok(None) => {
            return McpVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: "db passport missing".to_string(),
                },
                candidates: Vec::new(),
                warning: Some(
                    "vector index ignored because graph DB passport is missing".to_string(),
                ),
            };
        }
        Err(error) => {
            return McpVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: format!("db passport read failed: {error}"),
                },
                candidates: Vec::new(),
                warning: Some(
                    "vector index ignored because graph DB passport could not be read".to_string(),
                ),
            };
        }
    };

    let index = match load_vector_chunk_index_json(
        &index_path,
        &provider,
        &passport,
        mcp_vector_options(),
    ) {
        Ok(index) => index,
        Err(error) => {
            return McpVectorBranch {
                    index_path,
                    status: VectorCandidateBranchStatus::Stale {
                        reason: error.to_string(),
                    },
                    candidates: Vec::new(),
                    warning: Some(format!(
                        "vector index stale or incompatible; continuing without vector candidates: {error}"
                    )),
            };
        }
    };
    let _source_binding_validation = match validate_vector_chunk_source_bindings(repo_root, &index)
    {
        Ok(validation) if validation.is_valid() => validation,
        Ok(validation) => {
            return McpVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: format!(
                        "vector_source_binding_stale: {}",
                        validation.stale_reasons.join("; ")
                    ),
                },
                candidates: Vec::new(),
                warning: Some(
                    "vector index source file bindings are stale; continuing without vector candidates"
                        .to_string(),
                ),
            };
        }
        Err(error) => {
            return McpVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: format!("vector source binding validation failed: {error}"),
                },
                candidates: Vec::new(),
                warning: Some(
                    "vector index source file binding validation failed; continuing without vector candidates"
                        .to_string(),
                ),
            };
        }
    };
    let hits = match index.search(&provider, task, MCP_VECTOR_CANDIDATE_TOP_K) {
        Ok(hits) => hits,
        Err(error) => {
            return McpVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: error.to_string(),
                },
                candidates: Vec::new(),
                warning: Some(format!(
                    "vector index query failed; continuing without vector candidates: {error}"
                )),
            };
        }
    };
    let candidates = hits
        .iter()
        .map(|hit| {
            vector_chunk_search_hit_to_retrieval_candidate(hit, provider.metadata(), task, None)
        })
        .collect();

    McpVectorBranch {
        index_path,
        status: VectorCandidateBranchStatus::Ready,
        candidates,
        warning: None,
    }
}

fn mcp_vector_trace_fallback(vector_branch: &McpVectorBranch) -> Value {
    json!({
        "schema_version": 1,
        "diagnostic_only": true,
        "vector_enabled": true,
        "vector_index_status": vector_branch.status.as_str(),
        "vector_candidate_count": vector_branch.candidates.len(),
        "proof_contract": "vector candidates are candidate recall only and are not graph proof unless graph/source verification succeeds",
    })
}

fn mcp_vector_index_path(db_path: &Path, explicit_index_path: Option<&str>) -> PathBuf {
    if let Some(path) = explicit_index_path {
        let path = PathBuf::from(path);
        return if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&path))
                .unwrap_or(path)
        };
    }
    db_path
        .parent()
        .map(|parent| parent.join(MCP_VECTOR_INDEX_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(MCP_VECTOR_INDEX_FILE_NAME))
}

fn mcp_vector_provider() -> Result<DeterministicTestEmbeddingProvider, String> {
    DeterministicTestEmbeddingProvider::new(
        MCP_VECTOR_PROVIDER_DIMENSION,
        TestEmbeddingEnablement::Explicit,
    )
    .map_err(|error| error.to_string())
}

fn mcp_vector_options() -> VectorChunkIndexBuildOptions {
    VectorChunkIndexBuildOptions {
        max_chunks: MCP_VECTOR_CANDIDATE_TOP_K.saturating_mul(256),
        source_scope: MCP_VECTOR_SOURCE_SCOPE.to_string(),
        extraction_version: codegraph_index::VECTOR_EMBEDDING_CHUNK_EXTRACTION_VERSION.to_string(),
    }
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
        MCP_VALIDATE_EDIT_TOOL_NAME => (
            "Run changed-file update plus MCP validate-edit preflight over a production profile or explicit DB.",
            validate_edit_schema(),
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
            "Return deterministic token-projection/text candidate recall; not learned semantic proof.",
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
        "codegraph.index_repo" | "codegraph.update_changed_files" | MCP_VALIDATE_EDIT_TOOL_NAME
    );
    let idempotent = !matches!(name, MCP_VALIDATE_EDIT_TOOL_NAME);
    json!({
        "readOnlyHint": read_only,
        "destructiveHint": false,
        "idempotentHint": idempotent,
        "localOnly": true,
        "sourceMutation": false,
        "writesSourceFiles": false,
        "startupAutoIndex": false,
        "dotCodegraphFallback": false,
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
            properties.insert("staged_availability".to_string(), json!({"type": "object"}));
            properties.insert("graph_db_status".to_string(), json!({"type": "string"}));
            properties.insert(
                "candidate_spool_status".to_string(),
                json!({"type": "string"}),
            );
            properties.insert(
                "vector_runtime_status".to_string(),
                json!({"type": "string"}),
            );
            properties.insert("vector_audit_status".to_string(), json!({"type": "string"}));
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
            properties.insert("staged_availability".to_string(), json!({"type": "object"}));
            properties.insert("graph_db_status".to_string(), json!({"type": "string"}));
            properties.insert(
                "candidate_spool_status".to_string(),
                json!({"type": "string"}),
            );
            properties.insert(
                "vector_runtime_status".to_string(),
                json!({"type": "string"}),
            );
            properties.insert("vector_audit_status".to_string(), json!({"type": "string"}));
        }
        MCP_VALIDATE_EDIT_TOOL_NAME => {
            properties.insert("validation_packet".to_string(), json!({"type": "object"}));
            properties.insert(
                "hard_interrupt_available".to_string(),
                json!({"type": "boolean"}),
            );
            properties.insert(
                "hard_interrupt".to_string(),
                json!({"type": ["object", "null"]}),
            );
            properties.insert(
                "must_fix_before_continuing".to_string(),
                json!({"type": "boolean"}),
            );
            properties.insert("changed_files".to_string(), json!({"type": "array"}));
            properties.insert("rejected_paths".to_string(), json!({"type": "array"}));
            properties.insert("no_op_paths".to_string(), json!({"type": "array"}));
            properties.insert("warnings".to_string(), json!({"type": "array"}));
            properties.insert("unknowns".to_string(), json!({"type": "array"}));
            properties.insert("diagnostics".to_string(), json!({"type": "array"}));
            properties.insert("claimability".to_string(), json!({"type": "object"}));
            properties.insert("lifecycle".to_string(), json!({"type": "object"}));
            properties.insert("recovery_commands".to_string(), json!({"type": "array"}));
            properties.insert("omitted_count".to_string(), json!({"type": "integer"}));
            properties.insert("expansion_handles".to_string(), json!({"type": "array"}));
            properties.insert("timings".to_string(), json!({"type": "object"}));
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

fn validate_edit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "repo": {
                "type": "string",
                "description": "Repository root. Required so validate-edit can resolve the intended production profile boundary."
            },
            "db": {
                "type": ["string", "null"],
                "description": "Explicit SQLite DB path. If omitted, the server must already be configured with a production-agent-use profile DB."
            },
            "db_path": {
                "type": ["string", "null"],
                "description": "Alias for db."
            },
            "profile": {
                "type": ["string", "null"],
                "enum": [PRODUCTION_AGENT_USE_PROFILE_NAME, null],
                "description": "Optional profile name. Only production-agent-use is accepted."
            },
            "changed_files": {
                "type": "array",
                "items": {"type": "string"},
                "minItems": 1,
                "description": "Changed repository-relative file paths to update and validate."
            },
            "mode": {
                "type": "string",
                "enum": ["agent-json", "explain", "audit-json"],
                "description": "Detail mode for the validation packet."
            },
            "fail_on_blocking": {
                "type": "boolean",
                "description": "Recorded in the packet; MCP validation blockers remain structured success responses."
            },
            "task_id": {
                "type": ["string", "null"]
            },
            "edit_intent": {
                "type": ["string", "null"]
            },
            "expected_touched_files": {
                "type": "array",
                "items": {"type": "string"}
            },
            "max_output_bytes": {
                "type": ["integer", "null"],
                "minimum": 1
            }
        },
        "required": ["repo", "changed_files"],
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
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "candidate_spool".to_string(),
            json!({"type": "string", "description": "Optional candidate spool JSONL path for staged availability inspection."}),
        );
        properties.insert(
            "vector_runtime_sidecar".to_string(),
            json!({"type": "string", "description": "Optional compact runtime vector sidecar path for staged availability inspection."}),
        );
        properties.insert(
            "vector_audit_artifact".to_string(),
            json!({"type": "string", "description": "Optional diagnostic-only vector audit artifact path for staged availability inspection."}),
        );
    }
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
            "query".to_string(),
            json!({"type": "string", "description": "Symbol name to resolve when entity_id is unknown."}),
        );
        properties.insert(
            "symbol".to_string(),
            json!({"type": "string", "description": "Alias for query when entity_id is unknown."}),
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
    schema["required"] = json!([]);
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
        properties.insert(
            "mode".to_string(),
            json!({"type": "string", "description": "Context task mode, for example impact, production, test-impact, or debug."}),
        );
        properties.insert(
            "response_mode".to_string(),
            json!({"type": "string", "enum": ["compact", "verbose", "explain"], "description": "Controls response verbosity. compact is bounded and omits the full packet and funnel trace; verbose/explain include audit detail."}),
        );
        properties.insert(
            "output_mode".to_string(),
            json!({"type": "string", "enum": ["compact", "verbose", "explain"], "description": "Alias for response_mode."}),
        );
        properties.insert(
            "limit".to_string(),
            json!({"type": "integer", "minimum": 1, "maximum": 100, "description": "Compact response item limit."}),
        );
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
        properties.insert(
            "enable_vector_candidates".to_string(),
            json!({"type": "boolean", "description": "Explicitly enable deterministic vector candidate recall diagnostics. Candidates are not graph proof."}),
        );
        properties.insert(
            "enable_nuance_rescue_candidates".to_string(),
            json!({"type": "boolean", "description": "Explicitly enable deterministic 1-bit nuance rescue candidate recall. Candidates are not graph proof."}),
        );
        properties.insert(
            "vector_index".to_string(),
            json!({"type": "string", "description": "Optional persisted vector chunk index path for diagnostic candidate recall."}),
        );
        properties.insert(
            "vector_index_path".to_string(),
            json!({"type": "string", "description": "Alias for vector_index."}),
        );
        properties.insert(
            "vector_runtime_sidecar".to_string(),
            json!({"type": "string", "description": "Alias for vector_index; expected to point at the compact runtime vector sidecar."}),
        );
        properties.insert(
            "vector_audit_artifact".to_string(),
            json!({"type": "string", "description": "Optional diagnostic-only vector audit artifact path; it is not used for runtime retrieval."}),
        );
        properties.insert(
            "candidate_spool".to_string(),
            json!({"type": "string", "description": "Optional candidate spool JSONL path for explicit early candidate context."}),
        );
        properties.insert(
            "enable_candidate_spool".to_string(),
            json!({"type": "boolean", "description": "Explicitly allow candidate-only spool context when graph DB is unavailable."}),
        );
        properties.insert(
            "early_candidates".to_string(),
            json!({"type": "boolean", "description": "Alias for enable_candidate_spool."}),
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

fn mcp_context_pack_response_mode(args: &Map<String, Value>) -> Result<String, ToolCallError> {
    match optional_string(args, "response_mode")
        .or_else(|| optional_string(args, "output_mode"))
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
            format!("response_mode must be compact, verbose, or explain; got {other}"),
        )),
    }
}

fn optional_nullable_usize(
    args: &Map<String, Value>,
    key: &str,
) -> Result<Option<usize>, ToolCallError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(|number| Some(number as usize))
            .ok_or_else(|| {
                ToolCallError::new("invalid_input", format!("{key} must be an integer"))
            }),
    }
}

fn required_validate_edit_changed_files(
    args: &Map<String, Value>,
) -> Result<Vec<String>, ToolCallError> {
    if args.contains_key("changed_files") {
        required_string_array(args, "changed_files")
    } else {
        Err(ToolCallError::new(
            "invalid_input",
            "required string array argument missing or empty: changed_files",
        ))
    }
}

fn mcp_validate_edit_mode(args: &Map<String, Value>) -> Result<String, ToolCallError> {
    match optional_string(args, "mode")
        .unwrap_or_else(|| "agent-json".to_string())
        .replace('_', "-")
        .to_ascii_lowercase()
        .as_str()
    {
        "agent-json" | "json" | "compact" => Ok("agent-json".to_string()),
        "explain" => Ok("explain".to_string()),
        "audit-json" | "audit" => Ok("audit-json".to_string()),
        other => Err(ToolCallError::new(
            "invalid_input",
            format!("mode must be agent-json, explain, or audit-json; got {other}"),
        )),
    }
}

fn mcp_validate_edit_validation_rules() -> Vec<ValidationRule> {
    let mut rules = vec![
        ValidationRule::exact_blocking(
            CG_MVP3_CALLS_DANGLING_TARGET,
            ValidationRuleKind::DanglingTarget,
            Some(RelationKind::Calls),
            "new exact CALLS edges must resolve to a current target entity",
            "Define the missing callee or update the exact call relation.",
        ),
        ValidationRule::exact_blocking(
            CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED,
            ValidationRuleKind::DanglingTarget,
            Some(RelationKind::Calls),
            "renamed callee validation requires reverified graph/source evidence",
            "Inspect the rename and update stale call sites when graph/source proof exists.",
        ),
        ValidationRule::exact_blocking(
            CG_MVP3_IMPORTS_DANGLING_TARGET,
            ValidationRuleKind::DanglingTarget,
            Some(RelationKind::Imports),
            "new exact IMPORTS edges must resolve to a current import target entity",
            "Define the imported target or update the exact import relation.",
        ),
        ValidationRule::exact_blocking(
            CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH,
            ValidationRuleKind::BrokenContract,
            Some(RelationKind::AliasedBy),
            "exact import alias edges must resolve to their current target",
            "Update the alias import or restore the aliased target.",
        ),
    ];
    let mut config_text = ValidationRule::exact_blocking(
        CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING,
        ValidationRuleKind::UnsupportedRelationBoundary,
        None,
        "Config.in/package/build-system text evidence is warning-only by default",
        "Use text evidence as no-proof fallback context; do not invent graph edges from text-only package evidence.",
    );
    config_text.supported_relation_status = SupportedRelationStatus::ExactWarningCandidate;
    config_text.proof_requirement = ValidationProofRequirement::NotGraphProof;
    config_text.source_span_requirement = ValidationSourceSpanRequirement::Optional;
    config_text.provenance_requirement = ValidationProvenanceRequirement::NotApplicable;
    config_text.source_role_requirement = ValidationSourceRoleRequirement::NotApplicable;
    config_text.lifecycle_requirement = ValidationLifecycleRequirement::DiagnosticReadOnly;
    config_text.activation_condition =
        "Stage 0 text evidence changed for Config.in, package metadata, or build-system text"
            .to_string();
    rules.push(config_text);
    rules
}

fn mcp_validate_edit_validation_packet(
    repo_root: &Path,
    store: &SqliteGraphStore,
    changed_files: Vec<String>,
    preflight: &DbLifecyclePreflight,
    graph_delta: Value,
    delta: &EntitySourceRoleDeltaReport,
    delta_metrics: Value,
) -> Result<ValidationPacket, ToolCallError> {
    let lifecycle = mcp_validate_edit_lifecycle_from_preflight(preflight);
    let rules = mcp_validate_edit_validation_rules();
    let mut findings =
        mcp_validate_edit_collect_graph_findings(store, &changed_files, &rules, lifecycle.clone())?;
    findings.extend(mcp_validate_edit_collect_delta_findings(
        repo_root,
        delta,
        &rules,
        lifecycle.clone(),
    )?);
    let mut packet = ValidationPacket::new(
        changed_files,
        json!({
            "status": "updated",
            "source_update_surface": "mcp_validate_edit_shared_indexer",
            "summary": graph_delta,
            "normalized_delta_summary": mcp_validate_edit_delta_summary_json(delta),
            "delta_metrics": delta_metrics,
        }),
        findings,
        rules,
        Vec::new(),
        mcp_validate_edit_claimability_json(preflight, &lifecycle),
        mcp_validate_edit_proof_ladder_json(delta),
        mcp_validate_edit_lifecycle_json(preflight, &lifecycle),
    )
    .with_eligible_hard_interrupts(format!("unix_ms:{}", unix_time_ms()));
    packet.relation_family_status = json!({
        "calls": "exact_dangling_and_removed_target_checked",
        "imports": "exact_dangling_and_removed_target_checked",
        "aliases": "exact_alias_target_checked",
        "reads_writes_routes_tests_config": "not_checked_by_mcp_validate_edit_v1",
    });
    packet.activation_gate_state = json!({
        "mcp_validate_edit": "enabled",
        "editor_daemon_integration": "not_implemented",
        "startup_auto_index": false,
        "dot_codegraph_fallback": false,
    });
    Ok(packet)
}

fn mcp_validate_edit_delta_summary_json(delta: &EntitySourceRoleDeltaReport) -> Value {
    json!({
        "status": delta.status,
        "ready_to_report": delta.ready_to_report,
        "claimable": delta.claimable,
        "diagnostic_only": delta.diagnostic_only,
        "entities_added_count": delta.entities_added_count,
        "entities_removed_count": delta.entities_removed_count,
        "entities_changed_count": delta.entities_changed_count,
        "edges_added_count": delta.edges_added_count,
        "edges_removed_count": delta.edges_removed_count,
        "edges_changed_count": delta.edges_changed_count,
        "closure_files_updated": delta.closure_files_updated,
        "closure_budget_hit": delta.closure_budget_hit,
        "warnings": delta.warnings,
        "omission": delta.omission,
    })
}

fn mcp_validate_edit_proof_ladder_json(delta: &EntitySourceRoleDeltaReport) -> Value {
    let mut proof_ladder =
        serde_json::to_value(&delta.proof_ladder_changes).unwrap_or_else(|_| json!({}));
    if let Some(object) = proof_ladder.as_object_mut() {
        object.insert(
            "graph_delta_surface".to_string(),
            json!("changed_file_update_with_normalized_old_new_delta"),
        );
        object.insert("text_evidence_is_not_graph_proof".to_string(), json!(true));
        object.insert("public_claim".to_string(), json!(false));
    }
    proof_ladder
}

fn mcp_validate_edit_collect_graph_findings(
    store: &SqliteGraphStore,
    changed_files: &[String],
    rules: &[ValidationRule],
    lifecycle: ValidationLifecycleState,
) -> Result<Vec<ValidationFinding>, ToolCallError> {
    let mut findings = Vec::new();
    let mut seen_edge_rule = BTreeSet::new();
    for changed_file in changed_files {
        for edge in store
            .list_edges_by_file(changed_file)
            .map_err(mcp_store_error)?
        {
            let Some(rule) = mcp_validate_edit_rule_for_edge(&edge, rules) else {
                continue;
            };
            if !mcp_validate_edit_exactness_is_proof_grade(edge.exactness) {
                continue;
            }
            let target_id = mcp_validate_edit_edge_target_id(&edge).to_string();
            let target = store.get_entity(&target_id).map_err(mcp_store_error)?;
            if target.is_some() {
                continue;
            }
            let key = format!("{}:{}", rule.validation_rule_id, edge.id);
            if !seen_edge_rule.insert(key) {
                continue;
            }
            let source_role_allowed =
                mcp_validate_edit_source_role_allowed(store, &edge).map_err(mcp_store_error)?;
            let reason = match edge.relation {
                RelationKind::Calls => {
                    "current exact CALLS edge was reverified from the store and its target entity is absent"
                }
                RelationKind::AliasedBy => {
                    "current exact import alias edge was reverified from the store and its target entity is absent"
                }
                _ => {
                    "current exact import edge was reverified from the store and its target entity is absent"
                }
            };
            let mut input = ValidationReverificationInput::exact_graph_source(
                lifecycle.clone(),
                edge.id.clone(),
                reason,
            );
            input.source_role_allowed = source_role_allowed;
            input.provenance_required = edge.derived;
            input.provenance_present = !edge.derived || !edge.provenance_edges.is_empty();
            let mut finding = classify_validation_finding(
                rule,
                format!(
                    "finding://mcp_validate_edit/{}/{}",
                    rule.validation_rule_id, edge.id
                ),
                input,
            );
            finding.affected_edge =
                mcp_validate_edit_validation_edge_json(&edge, &target_id, target.as_ref());
            finding.affected_entity = json!({
                "entity_id": target_id,
                "missing": true,
            });
            let affected_file = normalize_repo_relative_path(&edge.source_span.repo_relative_path);
            finding.file = Some(affected_file.clone());
            finding.affected_file = Some(affected_file);
            finding.source_span = Some(edge.source_span.clone());
            finding.source_role = Some(classify_edge_evidence_role(&edge).role);
            finding.relation_kind = Some(edge.relation);
            finding.exactness = Some(edge.exactness);
            finding.provenance = json!({
                "derived": edge.derived,
                "provenance_edges": edge.provenance_edges,
                "provenance_required": edge.derived,
                "provenance_present": !edge.derived || !edge.provenance_edges.is_empty(),
            });
            finding.recommended_fix = Some(rule.docs_summary.clone());
            finding.suggested_next_steps = vec![rule.docs_summary.clone()];
            finding.expansion_handle = Some(format!("validation_packet:edge:{}", edge.id));
            findings.push(finding);
        }
    }
    Ok(findings)
}

fn mcp_validate_edit_collect_delta_findings(
    repo_root: &Path,
    delta: &EntitySourceRoleDeltaReport,
    rules: &[ValidationRule],
    lifecycle: ValidationLifecycleState,
) -> Result<Vec<ValidationFinding>, ToolCallError> {
    if !lifecycle.is_claimable_current() {
        return Ok(Vec::new());
    }
    let rule_by_id = rules
        .iter()
        .map(|rule| (rule.validation_rule_id.as_str(), rule))
        .collect::<BTreeMap<_, _>>();
    let removed_entity_ids = delta
        .entities_removed
        .iter()
        .filter_map(mcp_validate_edit_entity_delta_id)
        .collect::<BTreeSet<_>>();
    let mut findings = Vec::new();
    let mut seen = BTreeSet::new();

    for entry in delta
        .edges_removed
        .iter()
        .filter(|entry| mcp_validate_edit_delta_relation_supported(entry.relation))
    {
        if !mcp_validate_edit_exactness_is_proof_grade(entry.exactness) {
            continue;
        }
        let target_id = mcp_validate_edit_delta_target_id(entry);
        if !removed_entity_ids.contains(target_id) {
            continue;
        }
        if !mcp_validate_edit_removed_delta_span_still_names_target(repo_root, entry)? {
            continue;
        }
        let rule_id = match entry.relation {
            RelationKind::Calls => CG_MVP3_CALLS_DANGLING_TARGET,
            RelationKind::AliasedBy => CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH,
            RelationKind::Imports => CG_MVP3_IMPORTS_DANGLING_TARGET,
            _ => continue,
        };
        let Some(rule) = rule_by_id.get(rule_id).copied() else {
            continue;
        };
        let key = format!("{}:{}", rule.validation_rule_id, entry.edge_id);
        if !seen.insert(key) {
            continue;
        }
        findings.push(mcp_validate_edit_delta_finding(
            rule,
            entry,
            lifecycle.clone(),
            target_id,
        )?);
    }

    if !delta.file_renames_detected.is_empty()
        && !findings
            .iter()
            .any(|finding| finding.validation_rule_id == CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED)
    {
        if let Some(rule) = rule_by_id
            .get(CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED)
            .copied()
        {
            for entry in &delta.file_renames_detected {
                findings.push(mcp_validate_edit_rename_lifecycle_unknown(
                    rule,
                    lifecycle.clone(),
                    entry,
                )?);
            }
        }
    }

    let mut warned_text_paths = BTreeSet::<String>::new();
    for entry in &delta.text_evidence_changed {
        let path = normalize_repo_relative_path(&entry.repo_relative_path);
        if !mcp_validate_edit_path_is_config_package_text_evidence(&path)
            || !warned_text_paths.insert(path.clone())
        {
            continue;
        }
        let Some(rule) = rule_by_id
            .get(CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING)
            .copied()
        else {
            continue;
        };
        let affected_delta = serde_json::to_value(entry).map_err(|error| {
            ToolCallError::new(
                "serialization_failed",
                format!("could not encode validate-edit text warning: {error}"),
            )
        })?;
        findings.push(mcp_validate_edit_text_evidence_warning(
            rule,
            lifecycle.clone(),
            &path,
            affected_delta,
        ));
    }

    Ok(findings)
}

fn mcp_validate_edit_rename_lifecycle_unknown(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    entry: &codegraph_index::FileRenameDeltaEntry,
) -> Result<ValidationFinding, ToolCallError> {
    let old_path = entry
        .old_path
        .as_deref()
        .map(normalize_repo_relative_path)
        .unwrap_or_else(|| "unknown".to_string());
    let reason = "renamed callee validation is unknown because file rename lifecycle evidence alone is not graph-relation proof";
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle,
        format!("evidence://mcp-validate-edit/rename/{old_path}"),
        reason,
    );
    input.relation_exact = false;
    input.graph_source_relation_reverified = false;
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mcp_validate_edit/calls/unknown/{:016x}",
            mcp_validate_edit_stable_u64(&format!("{}:{reason}", old_path))
        ),
        input,
    );
    finding.affected_delta = serde_json::to_value(entry).map_err(|error| {
        ToolCallError::new(
            "serialization_failed",
            format!("could not encode validate-edit rename unknown: {error}"),
        )
    })?;
    finding.affected_file = Some(old_path.clone());
    finding.file = Some(old_path);
    finding.source_role = Some(EvidenceRole::Unknown);
    finding.relation_kind = Some(RelationKind::Calls);
    finding.reason = reason.to_string();
    finding.recommended_fix = Some(
        "Inspect the rename with explain/audit output before treating it as a source blocker."
            .to_string(),
    );
    finding.suggested_next_steps = vec![
        "Inspect the rename with explain/audit output before treating it as a source blocker."
            .to_string(),
    ];
    finding.diagnostics = vec!["rename_lifecycle_not_graph_relation_proof".to_string()];
    Ok(finding)
}

fn mcp_validate_edit_entity_delta_id(entry: &codegraph_index::EntityDeltaEntry) -> Option<String> {
    entry
        .old
        .as_ref()
        .map(|summary| summary.entity_id.clone())
        .or_else(|| entry.new.as_ref().map(|summary| summary.entity_id.clone()))
}

fn mcp_validate_edit_delta_relation_supported(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Calls | RelationKind::Imports | RelationKind::AliasedBy
    )
}

fn mcp_validate_edit_delta_target_id(entry: &EdgeDeltaEntry) -> &str {
    if entry.relation == RelationKind::AliasedBy {
        &entry.source_entity_id
    } else {
        &entry.target_entity_id
    }
}

fn mcp_validate_edit_delta_target_endpoint(
    entry: &EdgeDeltaEntry,
) -> &codegraph_index::EdgeDeltaEndpointSummary {
    if entry.relation == RelationKind::AliasedBy {
        &entry.source_endpoint
    } else {
        &entry.target_endpoint
    }
}

fn mcp_validate_edit_path_is_config_package_text_evidence(path: &str) -> bool {
    let normalized = normalize_repo_relative_path(path);
    let lower = normalized.to_ascii_lowercase();
    let file_name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    lower.ends_with(".mk")
        || lower.ends_with(".adoc")
        || lower.ends_with(".asciidoc")
        || lower.ends_with(".md")
        || lower.ends_with(".markdown")
        || lower.ends_with(".sh")
        || file_name == "config.in"
        || file_name == "kconfig"
        || file_name.starts_with("kconfig.")
        || lower.contains("/kconfig/")
        || (lower.starts_with("package/")
            && (lower.ends_with(".hash") || lower.ends_with(".mk") || file_name == "config.in"))
        || ((lower.starts_with("support/scripts/") || lower.starts_with("support/download/"))
            && !file_name.contains('.'))
}

fn mcp_validate_edit_text_evidence_warning(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    repo_relative_path: &str,
    affected_delta: Value,
) -> ValidationFinding {
    let reason = "Config.in/package/build-system text evidence changed; this is warning/no-proof fallback context, not broken graph proof";
    let evidence_id = format!("text://{repo_relative_path}");
    let mut input =
        ValidationReverificationInput::exact_graph_source(lifecycle, evidence_id.clone(), reason);
    input.relation_exact = false;
    input.graph_source_relation_reverified = false;
    input.source_span_required = false;
    input.source_span_present = false;
    input.provenance_required = false;
    input.provenance_present = true;
    input.evidence_items = vec![ValidationEvidenceItem::non_graph(
        ValidationEvidenceKind::TextEvidence,
        evidence_id,
        reason,
    )];
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mcp_validate_edit/config-package/text-warning/{:016x}",
            mcp_validate_edit_stable_u64(repo_relative_path)
        ),
        input,
    );
    finding.classification = ValidationClassification::Warn;
    finding.blocking_level = ValidationBlockingLevel::Warning;
    finding.proof_status = ValidationProofStatus::NotGraphProof;
    finding.proof_level = "not_graph_proof".to_string();
    finding.proof_strength = "text_evidence_warning".to_string();
    finding.affected_delta = affected_delta;
    finding.affected_file = Some(normalize_repo_relative_path(repo_relative_path));
    finding.source_span = None;
    finding.source_role = Some(EvidenceRole::Unknown);
    finding.relation_kind = None;
    finding.exactness = None;
    finding.provenance = json!({
        "required": false,
        "present": false,
        "text_evidence_only": true,
    });
    finding.old_fact_claim_state = "text_evidence_only".to_string();
    finding.new_fact_claim_state = "text_evidence_only".to_string();
    finding.reverified_graph_source_proof = false;
    finding.reason = reason.to_string();
    finding.recommended_fix = Some(
        "Use text evidence as no-proof context; do not promote it to graph proof.".to_string(),
    );
    finding.suggested_next_steps = vec![
        "Use text evidence as no-proof context; do not promote it to graph proof.".to_string(),
    ];
    finding.diagnostics = vec!["text_evidence_not_graph_proof".to_string()];
    finding
}

fn mcp_validate_edit_stable_u64(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn mcp_validate_edit_removed_delta_span_still_names_target(
    repo_root: &Path,
    entry: &EdgeDeltaEntry,
) -> Result<bool, ToolCallError> {
    let span_text = match mcp_validate_edit_source_span_text(repo_root, &entry.source_span) {
        Ok(text) => text,
        Err(_) => return Ok(false),
    };
    let endpoint = mcp_validate_edit_delta_target_endpoint(entry);
    let mut target_tokens = Vec::new();
    if let Some(name) = endpoint.name.as_deref() {
        target_tokens.push(name.to_string());
    }
    if let Some(qualified_name) = endpoint.qualified_name.as_deref() {
        target_tokens.extend(
            qualified_name
                .split([':', '.', '/', '\\'])
                .filter(|token| !token.is_empty())
                .map(ToOwned::to_owned),
        );
    }
    if let Some(path) = endpoint.repo_relative_path.as_deref() {
        let normalized = normalize_repo_relative_path(path);
        target_tokens.extend(
            normalized
                .split(['/', '\\', '.'])
                .filter(|token| !token.is_empty())
                .map(ToOwned::to_owned),
        );
        if let Some(stem) = Path::new(&normalized)
            .file_stem()
            .and_then(|stem| stem.to_str())
        {
            target_tokens.push(stem.to_string());
        }
    }
    target_tokens.extend(
        mcp_validate_edit_delta_target_id(entry)
            .split([':', '.', '/', '\\'])
            .filter(|token| !token.is_empty())
            .map(ToOwned::to_owned),
    );
    target_tokens.sort();
    target_tokens.dedup();
    Ok(target_tokens
        .iter()
        .any(|token| token.len() >= 2 && span_text.contains(token)))
}

fn mcp_validate_edit_source_span_text(
    repo_root: &Path,
    source_span: &SourceSpan,
) -> Result<String, ToolCallError> {
    let path = repo_root.join(&source_span.repo_relative_path);
    let contents = fs::read_to_string(&path).map_err(|error| {
        ToolCallError::new(
            "source_span_recheck_failed",
            format!("could not read source span {}: {error}", source_span),
        )
    })?;
    let lines = contents.lines().collect::<Vec<_>>();
    if source_span.start_line == 0 || source_span.end_line < source_span.start_line {
        return Err(ToolCallError::new(
            "source_span_recheck_failed",
            format!("invalid source span {source_span}"),
        ));
    }
    let start_index = source_span.start_line.saturating_sub(1) as usize;
    let end_index = source_span.end_line.saturating_sub(1) as usize;
    if start_index >= lines.len() {
        return Err(ToolCallError::new(
            "source_span_recheck_failed",
            format!("source span {source_span} starts past end of file"),
        ));
    }
    let end_index = end_index.min(lines.len().saturating_sub(1));
    let mut selected = Vec::new();
    for (offset, line) in lines[start_index..=end_index].iter().enumerate() {
        let absolute_line_index = start_index + offset;
        let mut chars = line.chars().collect::<Vec<_>>();
        if absolute_line_index == start_index {
            if let Some(start_column) = source_span.start_column {
                let start_column = start_column.saturating_sub(1) as usize;
                chars = chars.into_iter().skip(start_column).collect();
            }
        }
        if absolute_line_index == end_index {
            if let Some(end_column) = source_span.end_column {
                let end_column = end_column.saturating_sub(1) as usize;
                chars = chars.into_iter().take(end_column).collect();
            }
        }
        selected.push(chars.into_iter().collect::<String>());
    }
    Ok(selected.join("\n"))
}

fn mcp_validate_edit_delta_finding(
    rule: &ValidationRule,
    entry: &EdgeDeltaEntry,
    lifecycle: ValidationLifecycleState,
    target_id: &str,
) -> Result<ValidationFinding, ToolCallError> {
    let reason = match entry.relation {
        RelationKind::Calls => {
            "removed exact CALLS delta was reverified against current source text and its target entity is absent"
        }
        RelationKind::AliasedBy => {
            "removed exact import alias delta was reverified against current source text and its target entity is absent"
        }
        _ => {
            "removed exact import delta was reverified against current source text and its target entity is absent"
        }
    };
    let mut input =
        ValidationReverificationInput::exact_graph_source(lifecycle, entry.edge_id.clone(), reason);
    input.source_role_allowed = entry.source_role.is_production();
    input.provenance_required = entry.derived;
    input.provenance_present = !entry.derived || !entry.provenance_edges.is_empty();
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mcp_validate_edit/delta/{}/{}",
            rule.validation_rule_id, entry.edge_id
        ),
        input,
    );
    let affected_delta = serde_json::to_value(entry).map_err(|error| {
        ToolCallError::new(
            "serialization_failed",
            format!("could not encode validate-edit delta finding: {error}"),
        )
    })?;
    finding.affected_delta = affected_delta;
    finding.affected_edge = json!({
        "edge_id": entry.edge_id,
        "relation_kind": entry.relation,
        "source_entity_id": entry.source_entity_id,
        "target_entity_id": entry.target_entity_id,
        "source_endpoint": entry.source_endpoint,
        "target_endpoint": entry.target_endpoint,
        "file": normalize_repo_relative_path(&entry.repo_relative_path),
        "source_span": entry.source_span,
        "source_role": entry.source_role,
        "exactness": entry.exactness,
        "derived": entry.derived,
        "provenance_edges": entry.provenance_edges,
        "edge_class": entry.edge_class,
        "edge_context": entry.edge_context,
    });
    finding.affected_entity = json!({
        "entity_id": target_id,
        "missing": true,
    });
    let affected_file = normalize_repo_relative_path(&entry.repo_relative_path);
    finding.file = Some(affected_file.clone());
    finding.affected_file = Some(affected_file);
    finding.source_span = Some(entry.source_span.clone());
    finding.source_role = Some(entry.source_role);
    finding.relation_kind = Some(entry.relation);
    finding.exactness = Some(entry.exactness);
    finding.provenance = json!({
        "derived": entry.derived,
        "provenance_edges": entry.provenance_edges,
        "provenance_required": entry.derived,
        "provenance_present": !entry.derived || !entry.provenance_edges.is_empty(),
    });
    finding.recommended_fix = Some(rule.docs_summary.clone());
    finding.suggested_next_steps = vec![
        rule.docs_summary.clone(),
        "Rerun codegraph.validate_edit after fixing the source relation.".to_string(),
    ];
    finding.expansion_handle = Some(format!("validation_packet:delta_edge:{}", entry.edge_id));
    Ok(finding)
}

fn mcp_validate_edit_rule_for_edge<'a>(
    edge: &Edge,
    rules: &'a [ValidationRule],
) -> Option<&'a ValidationRule> {
    let rule_id = match edge.relation {
        RelationKind::Calls => CG_MVP3_CALLS_DANGLING_TARGET,
        RelationKind::Imports => CG_MVP3_IMPORTS_DANGLING_TARGET,
        RelationKind::AliasedBy => CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH,
        _ => return None,
    };
    rules.iter().find(|rule| rule.validation_rule_id == rule_id)
}

fn mcp_validate_edit_edge_target_id(edge: &Edge) -> &str {
    if edge.relation == RelationKind::AliasedBy {
        &edge.head_id
    } else {
        &edge.tail_id
    }
}

fn mcp_validate_edit_source_role_allowed(
    store: &SqliteGraphStore,
    edge: &Edge,
) -> Result<bool, codegraph_store::StoreError> {
    let edge_role = classify_edge_evidence_role(edge).role;
    if edge_role.is_production() {
        return Ok(true);
    }
    let head_role = store
        .get_entity(&edge.head_id)?
        .as_ref()
        .map(classify_entity_source_role)
        .map(|decision| decision.role)
        .unwrap_or(edge_role);
    Ok(head_role == EvidenceRole::Production)
}

fn mcp_validate_edit_exactness_is_proof_grade(exactness: Exactness) -> bool {
    matches!(
        exactness,
        Exactness::Exact
            | Exactness::CompilerVerified
            | Exactness::LspVerified
            | Exactness::ParserVerified
    )
}

fn mcp_validate_edit_validation_edge_json(
    edge: &Edge,
    target_id: &str,
    target: Option<&Entity>,
) -> Value {
    json!({
        "edge_id": edge.id,
        "relation_kind": edge.relation,
        "source_entity_id": edge.head_id,
        "target_entity_id": target_id,
        "target_present": target.is_some(),
        "target": target,
        "file": normalize_repo_relative_path(&edge.source_span.repo_relative_path),
        "source_span": edge.source_span,
        "source_role": classify_edge_evidence_role(edge).role,
        "exactness": edge.exactness,
        "derived": edge.derived,
        "provenance_edges": edge.provenance_edges,
        "edge_class": edge.edge_class,
        "edge_context": edge.context,
    })
}

fn mcp_validate_edit_diagnostic_packet(
    changed_files: Vec<String>,
    db_path: &Path,
    preflight: &DbLifecyclePreflight,
    rule_id: &str,
    reason: &str,
    affected_delta: Value,
) -> ValidationPacket {
    let lifecycle = mcp_validate_edit_lifecycle_from_preflight(preflight);
    let rule = ValidationRule::diagnostic(
        rule_id,
        "validate-edit requires claimable current DB state and in-repo changed paths",
        "Resolve the validate-edit preflight blocker, then rerun the tool.",
    );
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle.clone(),
        path_string(db_path),
        reason.to_string(),
    );
    input.graph_source_relation_reverified = false;
    input.relation_exact = false;
    input.source_span_required = false;
    input.source_span_present = false;
    input.evidence_items = vec![ValidationEvidenceItem::non_graph(
        ValidationEvidenceKind::Lifecycle,
        path_string(db_path),
        reason.to_string(),
    )];
    let mut finding = classify_validation_finding(
        &rule,
        format!("finding://mcp_validate_edit/preflight/{rule_id}"),
        input,
    );
    finding.affected_delta = affected_delta;
    finding.reason = reason.to_string();
    finding.recommended_fix = Some(rule.docs_summary.clone());
    finding.suggested_next_steps = vec![rule.docs_summary.clone()];
    ValidationPacket::new(
        changed_files,
        json!({
            "status": "not_run",
            "source_update_surface": "mcp_validate_edit_shared_indexer",
            "reason": reason,
        }),
        vec![finding],
        vec![rule],
        Vec::new(),
        mcp_validate_edit_claimability_json(preflight, &lifecycle),
        json!({
            "graph_delta_surface": "not_run",
            "text_evidence_is_not_graph_proof": true,
            "public_claim": false,
        }),
        mcp_validate_edit_lifecycle_json(preflight, &lifecycle),
    )
}

fn mcp_validate_edit_lifecycle_from_preflight(
    preflight: &DbLifecyclePreflight,
) -> ValidationLifecycleState {
    if preflight.safe {
        return ValidationLifecycleState::claimable_current();
    }
    let kind = preflight
        .db_problem_kind
        .as_deref()
        .unwrap_or("non_claimable");
    ValidationLifecycleState {
        claimable: false,
        current: false,
        stale: matches!(
            kind,
            "repo_head_mismatch" | "scope_mismatch" | "storage_mismatch"
        ) || preflight
            .blockers
            .iter()
            .any(|blocker| blocker.contains("stale") || blocker.contains("passport")),
        foreign: kind == "repo_root_mismatch"
            || preflight
                .blockers
                .iter()
                .any(|blocker| blocker.contains("foreign") || blocker.contains("repo root")),
        schema_mismatched: preflight.schema_status != "ok",
        dirty: preflight
            .db_health
            .reasons
            .iter()
            .any(|reason| reason.contains("interrupted") || reason.contains("dirty")),
        partial: preflight
            .db_health
            .reasons
            .iter()
            .any(|reason| reason.contains("partial") || reason.contains("incomplete")),
        non_claimable_reason: Some(
            preflight
                .blockers
                .first()
                .cloned()
                .or_else(|| preflight.db_problem_kind.clone())
                .unwrap_or_else(|| "db_lifecycle_not_claimable_current".to_string()),
        ),
    }
}

fn mcp_validate_edit_claimability_json(
    preflight: &DbLifecyclePreflight,
    lifecycle: &ValidationLifecycleState,
) -> Value {
    json!({
        "claimable": lifecycle.claimable,
        "current": lifecycle.current,
        "diagnostic_only": !lifecycle.is_claimable_current(),
        "candidate_only": false,
        "graph_proof_available": lifecycle.is_claimable_current(),
        "non_claimable_reason": lifecycle.non_claimable_reason,
        "db_problem_kind": preflight.db_problem_kind.clone(),
        "blockers": preflight.blockers.clone(),
        "exact_db_path_checked": preflight.exact_db_path_checked.clone(),
        "public_claim": false,
    })
}

fn mcp_validate_edit_lifecycle_json(
    preflight: &DbLifecyclePreflight,
    lifecycle: &ValidationLifecycleState,
) -> Value {
    json!({
        "claimable": lifecycle.claimable,
        "current": lifecycle.current,
        "stale": lifecycle.stale,
        "foreign": lifecycle.foreign,
        "schema_mismatched": lifecycle.schema_mismatched,
        "dirty": lifecycle.dirty,
        "partial": lifecycle.partial,
        "non_claimable_reason": lifecycle.non_claimable_reason,
        "preflight_safe": preflight.safe,
        "schema_status": preflight.schema_status.clone(),
        "passport_status": preflight.db_health.passport_status.clone(),
        "path_access_status": preflight.path_access_status.clone(),
        "exact_db_path_checked": preflight.exact_db_path_checked.clone(),
    })
}

#[allow(clippy::too_many_arguments)]
fn mcp_validate_edit_response(
    repo_root: &Path,
    db_path: &Path,
    db_source: &str,
    external_db_used: bool,
    status: &str,
    mode: &str,
    fail_on_blocking: bool,
    task_id: Option<String>,
    edit_intent: Option<String>,
    expected_touched_files: Vec<String>,
    max_output_bytes: Option<usize>,
    path_preflight: &ValidateEditChangedFilesPreflight,
    preflight: &DbLifecyclePreflight,
    packet: &ValidationPacket,
    source_update: Value,
    extra_metrics: Value,
    timings: Value,
    normal_dot_codegraph_mutated: bool,
) -> Result<Value, ToolCallError> {
    let full_packet = serde_json::to_value(packet).map_err(|error| {
        ToolCallError::new(
            "serialization_failed",
            format!("could not encode validation packet: {error}"),
        )
    })?;
    let validation_packet = match mode {
        "agent-json" => packet.compact_agent_json(3),
        "explain" | "audit-json" => full_packet.clone(),
        _ => full_packet.clone(),
    };
    let expected_missing =
        mcp_validate_edit_expected_missing(&expected_touched_files, &path_preflight.accepted_paths);
    let empty_args = Map::new();
    let staged_availability =
        mcp_staged_availability(repo_root, db_path, Some(preflight), &empty_args, None);
    let rtds_freshness =
        mcp_rtds_freshness_json(repo_root, db_path, Some(preflight), &staged_availability);
    let mut response_warnings = full_packet
        .get("warnings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    response_warnings.extend(path_preflight.warnings.iter().cloned().map(Value::String));
    let mut response_diagnostics = full_packet
        .get("diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    response_diagnostics.extend(
        path_preflight
            .diagnostics
            .iter()
            .filter_map(|diagnostic| serde_json::to_value(diagnostic).ok()),
    );
    let mut value = json!({
        "schema_version": 1,
        "schema_name": MCP_VALIDATE_EDIT_SCHEMA_NAME,
        "packet_kind": "validate_edit_packet",
        "command": "validate-edit",
        "command_namespace": "mcp",
        "agent_use_command": "agent-use validate-edit",
        "mcp_tool_name": MCP_VALIDATE_EDIT_TOOL_NAME,
        "canonical_cli_surface": "codegraph-mcp agent-use validate-edit --repo <repo> --changed <path> --agent-json",
        "canonical_mcp_surface": "codegraph.validate_edit",
        "compatibility_alias_status": "implemented_as_explicit_mcp_tool",
        "status": status,
        "final_status": full_packet.get("final_status").cloned().unwrap_or_else(|| json!(status)),
        "severity_summary": full_packet.get("severity_summary").cloned().unwrap_or_else(|| json!({})),
        "can_continue_with_caution": full_packet.get("can_continue_with_caution").cloned().unwrap_or_else(|| json!(false)),
        "should_recover_tool_state": full_packet.get("should_recover_tool_state").cloned().unwrap_or_else(|| json!(false)),
        "should_run_tests": full_packet.get("should_run_tests").cloned().unwrap_or_else(|| json!(false)),
        "should_request_explain": full_packet.get("should_request_explain").cloned().unwrap_or_else(|| json!(false)),
        "should_rerun_validation": full_packet.get("should_rerun_validation").cloned().unwrap_or_else(|| json!(false)),
        "next_agent_action": full_packet.get("next_agent_action").cloned().unwrap_or_else(|| json!("continue")),
        "aggregate_guidance": full_packet.get("aggregate_guidance").cloned().unwrap_or_else(|| json!([])),
        "repo": path_string(repo_root),
        "repo_root": path_string(repo_root),
        "db": path_string(db_path),
        "db_path": path_string(db_path),
        "resolved_db": path_string(db_path),
        "db_source": db_source,
        "profile_name": if mcp_db_looks_like_agent_use_profile(db_path) { PRODUCTION_AGENT_USE_PROFILE_NAME } else { "explicit-db" },
        "uses_production_agent_use_resolver": db_source == "production_agent_use_profile",
        "external_profile_db_used": db_source == "production_agent_use_profile" && external_db_used,
        "external_db_used": external_db_used,
        "mcp_uses_external_profile_or_explicit_db": true,
        "no_dot_codegraph_fallback": true,
        "normal_dot_codegraph_mutated": normal_dot_codegraph_mutated,
        "startup_auto_index": false,
        "public_claim": false,
        "changed_files": path_preflight.accepted_paths.clone(),
        "changed_paths": path_preflight.accepted_paths.clone(),
        "normalized_changed_files": path_preflight.normalized_changed_files.clone(),
        "changed_paths_requested": path_preflight.requested_paths.clone(),
        "rejected_paths": path_preflight.rejected_paths.clone(),
        "no_op_paths": path_preflight.no_op_paths.clone(),
        "deleted_paths": path_preflight.deleted_paths.clone(),
        "renamed_paths": path_preflight.renamed_paths.clone(),
        "ignored_paths": path_preflight.ignored_paths.clone(),
        "generated_paths": path_preflight.generated_paths.clone(),
        "outside_repo_paths": path_preflight.outside_repo_paths.clone(),
        "duplicate_paths": path_preflight.duplicate_paths.clone(),
        "atomic_temp_paths": path_preflight.atomic_temp_paths.clone(),
        "partial_input_failures_reported": path_preflight.partial_input_failures_reported,
        "too_many_changed_files": path_preflight.too_many_changed_files,
        "max_changed_files": path_preflight.max_changed_files,
        "no_silent_path_drops": true,
        "input_policy": path_preflight.input_policy.clone(),
        "rename_policy": path_preflight.rename_policy.clone(),
        "per_file_status": path_preflight.per_file_status.clone(),
        "input_diagnostics": path_preflight.diagnostics.clone(),
        "input_warnings": path_preflight.warnings.clone(),
        "validation_packet": validation_packet,
        "hard_interrupt_available": packet.hard_interrupt_available,
        "hard_interrupt": packet.hard_interrupt.clone(),
        "hard_interrupt_not_implemented": false,
        "must_fix_before_continuing": packet.must_fix_before_continuing,
        "validation_must_fix_before_continuing": packet.must_fix_before_continuing,
        "validation_blocking_error_count": packet.blocking_errors.len(),
        "validation_warning_count": packet.warnings.len(),
        "validation_unknown_count": packet.unknowns.len(),
        "warnings": response_warnings,
        "unknowns": full_packet.get("unknowns").cloned().unwrap_or_else(|| json!([])),
        "diagnostics": response_diagnostics,
        "claimability": full_packet.get("claimability").cloned().unwrap_or_else(|| json!({})),
        "lifecycle": full_packet.get("lifecycle").cloned().unwrap_or_else(|| json!({})),
        "proof_ladder_changes": full_packet.get("proof_ladder_changes").cloned().unwrap_or_else(|| json!({})),
        "db_lifecycle_read": mcp_db_lifecycle_preflight_json(preflight),
        "staged_availability": staged_availability,
        "rtds_freshness": rtds_freshness,
        "profile_identity": mcp_profile_identity_json(repo_root, db_path),
        "recovery_commands": mcp_agent_use_recovery_commands(repo_root),
        "validation_recovery_commands": full_packet.get("recovery_commands").cloned().unwrap_or_else(|| json!([])),
        "omitted_count": validation_packet.get("omitted_count").and_then(Value::as_u64).unwrap_or(packet.omitted_count as u64),
        "expansion_handles": validation_packet.get("expansion_handles").cloned().unwrap_or_else(|| json!(packet.expansion_handles.clone())),
        "timings": timings,
        "command_rerun_hint": format!(
            "codegraph-mcp agent-use validate-edit --repo \"{}\" --changed <path> --agent-json",
            path_string(repo_root)
        ),
        "detail_mode": mode,
        "compact_default": mode == "agent-json",
        "fail_on_blocking": fail_on_blocking,
        "fail_on_blocking_exit_code": 2,
        "default_exit_zero_on_validation_blocker": true,
        "json_printed_on_blocking": true,
        "task_id": task_id,
        "edit_intent": edit_intent,
        "expected_touched_files": expected_touched_files,
        "expected_touched_files_missing": expected_missing,
        "source_update_status": source_update.get("status").cloned().unwrap_or_else(|| json!("not_run")),
        "source_update_command": "mcp validate-edit shared indexer update",
        "source_update_packet": if mode == "agent-json" { Value::Null } else { source_update.clone() },
        "metrics": {
            "files_walked": source_update.get("files_walked").cloned().unwrap_or_else(|| json!(0)),
            "files_read": source_update.get("files_read").cloned().unwrap_or_else(|| json!(0)),
            "files_hashed": source_update.get("files_hashed").cloned().unwrap_or_else(|| json!(0)),
            "files_parsed": source_update.get("files_parsed").cloned().unwrap_or_else(|| json!(0)),
            "facts_inserted": source_update.get("entities").cloned().unwrap_or_else(|| json!(0)),
            "entities": source_update.get("entities").cloned().unwrap_or_else(|| json!(0)),
            "edges": source_update.get("edges").cloned().unwrap_or_else(|| json!(0)),
            "files_indexed": source_update.get("files_indexed").cloned().unwrap_or_else(|| json!(0)),
            "extra": extra_metrics,
        },
    });
    mcp_validate_edit_add_mode_aware_severity_fields(&mut value, mode);
    mcp_attach_dirty_evidence_output_fields(
        &mut value,
        "codegraph.validate_edit",
        mode == "explain" || mode == "audit-json",
    );
    if let Some(max_output_bytes) = max_output_bytes {
        value = mcp_validate_edit_apply_output_budget(value, packet, max_output_bytes)?;
        mcp_validate_edit_add_mode_aware_severity_fields(&mut value, mode);
        mcp_attach_dirty_evidence_output_fields(
            &mut value,
            "codegraph.validate_edit",
            mode == "explain" || mode == "audit-json",
        );
    }
    Ok(value)
}

fn mcp_validate_edit_add_mode_aware_severity_fields(value: &mut Value, mode: &str) {
    let validation_packet = value
        .get("validation_packet")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let severity_summary = validation_packet
        .get("severity_summary")
        .cloned()
        .or_else(|| value.get("severity_summary").cloned())
        .unwrap_or_else(|| json!({}));
    let final_status = validation_packet
        .get("final_status")
        .or_else(|| value.get("final_status"))
        .or_else(|| validation_packet.get("status"))
        .or_else(|| value.get("status"))
        .cloned()
        .unwrap_or_else(|| json!("unknown"));
    let final_severity = severity_summary
        .get("max_severity")
        .and_then(Value::as_str)
        .or_else(|| final_status.as_str())
        .unwrap_or("unknown")
        .to_string();
    let hard_interrupt_available = value
        .get("hard_interrupt_available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let must_fix_before_continuing = value
        .get("must_fix_before_continuing")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let should_request_explain = value
        .get("should_request_explain")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let should_rerun_validation = value
        .get("should_rerun_validation")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let editor_policy = validation_packet
        .get("editor_policy")
        .cloned()
        .unwrap_or_else(|| {
            mcp_validate_edit_editor_policy_json(
                final_status.as_str().unwrap_or("unknown"),
                must_fix_before_continuing,
                hard_interrupt_available,
                should_request_explain,
                should_rerun_validation,
            )
        });
    let severity_trace = if mode == "explain" || mode == "audit-json" {
        json!({
            "mode": mode,
            "per_finding_severity_mapping": validation_packet
                .get("severity_decisions")
                .cloned()
                .unwrap_or_else(|| json!([])),
            "final_aggregation_trace": validation_packet
                .get("severity_aggregation_trace")
                .cloned()
                .unwrap_or_else(|| json!({})),
            "severity_precedence_decision": severity_summary
                .get("status_precedence")
                .cloned()
                .unwrap_or_else(|| json!(["tool_error", "blocking_graph_error", "warning", "unknown", "diagnostic_only", "ok"])),
            "activation_gate_state": validation_packet
                .get("activation_gate_state")
                .cloned()
                .unwrap_or_else(|| json!({})),
            "why_hard_interrupt_available_or_unavailable": {
                "hard_interrupt_available": hard_interrupt_available,
                "hard_interrupt_requires_interrupt_eligible_blocking": true,
                "normal_validation_blocker_is_tool_error": false
            },
            "tool_error_vs_validation_blocker": {
                "validation_blockers_return_structured_success": true,
                "mcp_tool_error_reserved_for_runtime_protocol_failure": true
            },
            "lifecycle_claimability_details": {
                "claimability": value.get("claimability").cloned().unwrap_or_else(|| json!({})),
                "lifecycle": value.get("lifecycle").cloned().unwrap_or_else(|| json!({})),
                "stale_unsafe_blockers": validation_packet
                    .get("stale_unsafe_blockers")
                    .cloned()
                    .unwrap_or_else(|| json!([]))
            },
            "proof_ladder_details": validation_packet
                .get("proof_ladder_changes")
                .cloned()
                .unwrap_or_else(|| json!({})),
            "warning_unknown_diagnostic_details": {
                "warnings": validation_packet.get("warnings").cloned().unwrap_or_else(|| json!([])),
                "unknowns": validation_packet.get("unknowns").cloned().unwrap_or_else(|| json!([])),
                "diagnostics": validation_packet.get("diagnostics").cloned().unwrap_or_else(|| json!([]))
            },
            "skipped_rule_details": validation_packet
                .get("validation_rules_skipped")
                .cloned()
                .unwrap_or_else(|| json!([])),
            "non_interrupt_reasons": validation_packet
                .get("aggregate_guidance")
                .cloned()
                .unwrap_or_else(|| json!([])),
            "full_source_bodies_included": false,
            "full_graph_dump_included": false,
        })
    } else {
        json!({
            "mode": mode,
            "summary_only": true,
            "request_full_trace_with": "mode=explain or mode=audit-json",
            "full_source_bodies_included": false,
            "full_graph_dump_included": false,
        })
    };
    let proof_ladder_changes_summary = mcp_validate_edit_proof_ladder_summary(
        &validation_packet
            .get("proof_ladder_changes")
            .cloned()
            .or_else(|| value.get("proof_ladder_changes").cloned())
            .unwrap_or(Value::Null),
    );

    if let Some(object) = value.as_object_mut() {
        object.insert("final_severity".to_string(), json!(final_severity));
        object.insert(
            "stale_unsafe_blockers".to_string(),
            validation_packet
                .get("stale_unsafe_blockers")
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
        object.insert(
            "blocking_error_count".to_string(),
            json!(mcp_validate_edit_bucket_count(
                &validation_packet,
                "blocking_errors"
            )),
        );
        object.insert(
            "warning_count".to_string(),
            json!(mcp_validate_edit_bucket_count(
                &validation_packet,
                "warnings"
            )),
        );
        object.insert(
            "unknown_count".to_string(),
            json!(mcp_validate_edit_bucket_count(
                &validation_packet,
                "unknowns"
            )),
        );
        object.insert(
            "diagnostic_count".to_string(),
            json!(mcp_validate_edit_bucket_count(
                &validation_packet,
                "diagnostics"
            )),
        );
        object.insert(
            "validation_blocking_error_count".to_string(),
            json!(mcp_validate_edit_bucket_count(
                &validation_packet,
                "blocking_errors"
            )),
        );
        object.insert(
            "validation_warning_count".to_string(),
            json!(mcp_validate_edit_bucket_count(
                &validation_packet,
                "warnings"
            )),
        );
        object.insert(
            "validation_unknown_count".to_string(),
            json!(mcp_validate_edit_bucket_count(
                &validation_packet,
                "unknowns"
            )),
        );
        object.insert(
            "validation_diagnostic_count".to_string(),
            json!(mcp_validate_edit_bucket_count(
                &validation_packet,
                "diagnostics"
            )),
        );
        object.insert(
            "top_blocking_source_span".to_string(),
            validation_packet
                .get("top_blocking_source_spans")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .cloned()
                .unwrap_or(Value::Null),
        );
        object.insert(
            "top_recommended_fix".to_string(),
            validation_packet
                .get("blocking_errors")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("recommended_fix"))
                .cloned()
                .unwrap_or(Value::Null),
        );
        object.insert(
            "proof_ladder_changes_summary".to_string(),
            proof_ladder_changes_summary,
        );
        object.insert(
            "recovery_commands_pointer".to_string(),
            json!("validation_recovery_commands"),
        );
        object.insert("severity_trace".to_string(), severity_trace);
        object.insert("editor_policy".to_string(), editor_policy);
        object.insert("full_graph_dump_included".to_string(), json!(false));
        object.insert("full_source_bodies_included".to_string(), json!(false));
    }
}

fn mcp_validate_edit_bucket_count(validation_packet: &Value, key: &str) -> usize {
    validation_packet
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn mcp_validate_edit_editor_policy_json(
    final_status: &str,
    must_fix_before_continuing: bool,
    hard_interrupt_available: bool,
    should_request_explain: bool,
    should_rerun_validation: bool,
) -> Value {
    let should_show_warning_panel = !hard_interrupt_available
        && (matches!(final_status, "warning" | "unknown" | "diagnostic_only")
            || should_request_explain);
    json!({
        "editor_policy_version": 1,
        "recommended_editor_action": if hard_interrupt_available {
            "show_blocking_modal"
        } else if should_show_warning_panel {
            "show_warning_panel"
        } else {
            "allow_continue"
        },
        "should_show_modal": hard_interrupt_available,
        "should_show_warning_panel": should_show_warning_panel,
        "should_allow_continue": !must_fix_before_continuing,
        "should_request_revalidation": should_rerun_validation,
        "safe_to_autofix": false,
        "source_edits_performed": false,
        "daemon_integration_available": false,
        "plugin_integration_available": false,
        "metadata_advisory_only": true,
    })
}

fn mcp_validate_edit_proof_ladder_summary(proof_ladder_changes: &Value) -> Value {
    let Some(object) = proof_ladder_changes.as_object() else {
        return json!({
            "available": !proof_ladder_changes.is_null(),
            "changed_count": 0,
            "graph_proof_changed_count": 0,
            "keys": [],
        });
    };
    let mut keys = object
        .iter()
        .filter_map(|(key, value)| value.is_object().then_some(key.clone()))
        .collect::<Vec<_>>();
    keys.sort();
    json!({
        "available": true,
        "changed_count": keys.len(),
        "graph_proof_changed_count": object
            .values()
            .filter(|value| value.is_object())
            .filter(|value| value.get("graph_proof").and_then(Value::as_bool).unwrap_or(false))
            .count(),
        "keys": keys,
        "full_detail_handle": "validation_packet.proof_ladder_changes",
    })
}

fn mcp_validate_edit_expected_missing(
    expected_touched_files: &[String],
    accepted_paths: &[String],
) -> Vec<String> {
    let accepted = accepted_paths
        .iter()
        .map(|path| normalize_repo_relative_path(path))
        .collect::<BTreeSet<_>>();
    expected_touched_files
        .iter()
        .filter_map(|path| {
            let normalized = normalize_repo_relative_path(path);
            (!accepted.contains(&normalized)).then_some(normalized)
        })
        .collect()
}

fn mcp_validate_edit_apply_output_budget(
    mut value: Value,
    packet: &ValidationPacket,
    max_output_bytes: usize,
) -> Result<Value, ToolCallError> {
    let encoded_len = serde_json::to_string(&value)
        .map_err(|error| ToolCallError::new("serialization_failed", error.to_string()))?
        .len();
    if encoded_len <= max_output_bytes {
        return Ok(value);
    }
    let compact_packet = packet.compact_agent_json(1);
    if let Some(object) = value.as_object_mut() {
        object.insert("output_truncated".to_string(), json!(true));
        object.insert("max_output_bytes".to_string(), json!(max_output_bytes));
        object.insert("pre_truncation_bytes".to_string(), json!(encoded_len));
        object.insert("source_update_packet".to_string(), Value::Null);
        object.insert("validation_packet".to_string(), compact_packet.clone());
        object.insert(
            "omitted_count".to_string(),
            compact_packet
                .get("omitted_count")
                .cloned()
                .unwrap_or_else(|| json!(packet.omitted_count)),
        );
        object.insert(
            "expansion_handles".to_string(),
            compact_packet
                .get("expansion_handles")
                .cloned()
                .unwrap_or_else(|| json!(["validation_packet:full"])),
        );
        let after_len = serde_json::to_string(&Value::Object(object.clone()))
            .map_err(|error| ToolCallError::new("serialization_failed", error.to_string()))?
            .len();
        if after_len > max_output_bytes {
            object.insert(
                "output_budget_status".to_string(),
                json!("exceeded_after_safety_field_compaction"),
            );
            object.insert("post_truncation_bytes".to_string(), json!(after_len));
        } else {
            object.insert("output_budget_status".to_string(), json!("applied"));
            object.insert("post_truncation_bytes".to_string(), json!(after_len));
        }
    }
    Ok(value)
}

fn mcp_context_pack_compact_json(
    task: &str,
    packet: &ContextPacket,
    preflight: &DbLifecyclePreflight,
    limit: usize,
    vector_candidate_diagnostics: Value,
    nuance_rescue_diagnostics: Value,
    staged_availability: Value,
) -> Value {
    let lifecycle = mcp_compact_lifecycle_summary(preflight);
    let claimable = lifecycle
        .get("claimable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let diagnostic_only = lifecycle
        .get("diagnostic_only")
        .and_then(Value::as_bool)
        .unwrap_or(!claimable);
    let proof_path_count = packet.verified_paths.len();
    let snippet_count = packet.snippets.len();
    let fallback_evidence = packet
        .metadata
        .get("fallback_evidence")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let returned_paths = packet
        .verified_paths
        .iter()
        .take(limit)
        .map(mcp_compact_path_evidence_json)
        .collect::<Vec<_>>();
    let returned_snippets = packet
        .snippets
        .iter()
        .take(limit)
        .map(mcp_compact_context_snippet_json)
        .collect::<Vec<_>>();
    let returned_fallback = fallback_evidence
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let result_count = proof_path_count + fallback_evidence.len();
    let returned_count = returned_paths.len() + returned_fallback.len();
    let omitted_count = result_count.saturating_sub(returned_count)
        + snippet_count.saturating_sub(returned_snippets.len());
    let graph_proof = packet
        .metadata
        .get("graph_proof")
        .and_then(Value::as_bool)
        .unwrap_or(proof_path_count > 0);
    let proof_status = packet
        .metadata
        .get("proof_status")
        .and_then(Value::as_str)
        .unwrap_or(if graph_proof {
            "proof_path_found"
        } else {
            "unknown"
        });
    let proof_strength =
        mcp_context_pack_proof_strength(graph_proof, &fallback_evidence, &packet.snippets);
    let (task_intent, task_profile, retrieval_plan) = plan_task_retrieval(task);
    let staged_fields = mcp_staged_top_level_fields(&staged_availability);

    let mut value = json!({
        "status": "ok",
        "schema_version": 1,
        "command": "codegraph.context_pack",
        "response_mode": "compact",
        "task": task,
        "mode": packet.mode.clone(),
        "task_intent": task_intent.to_json(),
        "task_profile": task_profile.to_json(),
        "retrieval_plan_summary": retrieval_plan.summary_json(),
        "lifecycle": lifecycle,
        "claimable": claimable,
        "diagnostic_only": diagnostic_only,
        "result_count": result_count,
        "limit": limit,
        "omitted_count": omitted_count,
        "warnings": [],
        "errors": [],
        "symbols": packet.symbols.clone(),
        "paths": returned_paths,
        "snippets": returned_snippets,
        "fallback_evidence": returned_fallback,
        "fallback_evidence_count": fallback_evidence.len(),
        "proof_path_count": proof_path_count,
        "proof_status": proof_status,
        "proof_strength": proof_strength,
        "graph_proof": graph_proof,
        "evidence_status": packet.metadata.get("evidence_status").cloned().unwrap_or(Value::Null),
        "proof_failure_reason": packet.metadata.get("proof_failure_reason").cloned().unwrap_or(Value::Null),
        "staged_availability": staged_availability.clone(),
        "truncation": {
            "returned_count": returned_count,
            "limit_applied": omitted_count > 0,
            "limit": limit,
            "omitted_count": omitted_count,
            "total_available": result_count + snippet_count,
            "total_available_unknown": false
        },
        "vector_candidate_diagnostics": mcp_compact_vector_diagnostics(vector_candidate_diagnostics),
        "nuance_rescue_diagnostics": mcp_compact_nuance_rescue_diagnostics(nuance_rescue_diagnostics),
        "proof": "Compact context-pack output omits full packet metadata and funnel trace. Use response_mode=verbose or response_mode=explain for audit detail.",
    });
    mcp_merge_json_object(&mut value, staged_fields);
    value
}

fn mcp_context_pack_proof_strength(
    graph_proof: bool,
    fallback_evidence: &[Value],
    snippets: &[ContextSnippet],
) -> &'static str {
    if graph_proof {
        return "graph_relation_proof";
    }
    if fallback_evidence.iter().any(|evidence| {
        evidence.get("evidence_role").and_then(Value::as_str) == Some("text_evidence")
            || evidence
                .get("fallback_source")
                .and_then(Value::as_str)
                .is_some_and(|source| source.contains("text_evidence"))
    }) {
        return "text_evidence";
    }
    if !fallback_evidence.is_empty() || !snippets.is_empty() {
        return "source_navigation_evidence";
    }
    "unknown"
}

fn mcp_compact_lifecycle_summary(preflight: &DbLifecyclePreflight) -> Value {
    json!({
        "claimable": preflight.safe,
        "diagnostic_only": !preflight.safe,
        "decision": if preflight.safe { "read_reuse" } else { "diagnostic_stale_reuse" },
        "db_problem_kind": preflight.db_problem_kind.clone(),
        "passport_status": preflight.db_health.passport_status.clone(),
        "schema_status": preflight.schema_status.clone(),
        "scope_status": preflight.scope_status.clone(),
        "repo_root_status": preflight.repo_root_status.clone(),
        "sidecar_status": preflight.db_health.sidecar_status.clone(),
        "path_access_status": preflight.path_access_status.clone(),
        "exact_db_path_checked": preflight.exact_db_path_checked.clone(),
    })
}

fn mcp_compact_path_evidence_json(path: &PathEvidence) -> Value {
    let edge_limit = 8usize;
    let span_limit = 8usize;
    json!({
        "path_id": path.id.clone(),
        "summary": path.summary.clone(),
        "source": path.source.clone(),
        "target": path.target.clone(),
        "relations": path.metapath.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "edges": path.edges.iter().take(edge_limit).map(|(head, relation, tail)| {
            json!({
                "head_id": head,
                "relation": relation.to_string(),
                "tail_id": tail,
            })
        }).collect::<Vec<_>>(),
        "source_spans": path.source_spans.iter().take(span_limit).collect::<Vec<_>>(),
        "exactness": path.exactness.to_string(),
        "confidence": path.confidence,
        "evidence_role": path.metadata.get("evidence_role").and_then(Value::as_str).unwrap_or("unknown"),
        "proof_status": path.metadata.get("proof_status").and_then(Value::as_str).unwrap_or("proof_path_found"),
        "classification_reason": path.metadata.get("evidence_role_reason").or_else(|| path.metadata.get("classification_reason")).cloned().unwrap_or(Value::Null),
        "omitted_edges": path.edges.len().saturating_sub(edge_limit),
        "omitted_source_spans": path.source_spans.len().saturating_sub(span_limit),
    })
}

fn mcp_compact_context_snippet_json(snippet: &ContextSnippet) -> Value {
    json!({
        "file": snippet.file.clone(),
        "lines": snippet.lines.clone(),
        "text": compact_mcp_text(&snippet.text, 600),
        "reason": snippet.reason.clone(),
    })
}

fn mcp_compact_vector_diagnostics(value: Value) -> Value {
    let Some(object) = value.as_object() else {
        return Value::Null;
    };
    let vector_candidate_count = object
        .get("vector_candidate_count")
        .or_else(|| object.get("candidate_count"))
        .cloned()
        .unwrap_or(Value::Null);
    json!({
        "vector_index_status": object.get("vector_index_status").cloned().unwrap_or(Value::Null),
        "candidate_count": vector_candidate_count.clone(),
        "vector_candidate_count": vector_candidate_count,
        "warning": object.get("warning").cloned().unwrap_or(Value::Null),
    })
}

fn mcp_nuance_rescue_diagnostics(
    enabled: bool,
    candidates: &[RetrievalCandidate],
    trace: &[RetrievalTraceStage],
) -> Value {
    let trace_stage = trace
        .iter()
        .find(|stage| stage.stage == "stage1_nuance_rescue");
    let status = if enabled {
        "active_current"
    } else {
        "disabled"
    };
    let dropped_count = trace_stage.map(|stage| stage.dropped.len()).unwrap_or(0);
    let notes = trace_stage
        .map(|stage| stage.notes.iter().take(8).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let items = candidates
        .iter()
        .take(8)
        .map(mcp_nuance_rescue_candidate_diagnostic_json)
        .collect::<Vec<_>>();

    json!({
        "schema_version": 1,
        "diagnostic_only": true,
        "nuance_rescue_enabled": enabled,
        "rescue_enabled": enabled,
        "rescue_status": status,
        "candidate_count": candidates.len(),
        "rescue_candidate_count": candidates.len(),
        "dropped_count": dropped_count,
        "notes": notes,
        "candidates": items,
        "proof_contract": "nuance rescue candidates are deterministic candidate recall only; graph_proof remains false until graph/source verification"
    })
}

fn mcp_nuance_rescue_candidate_diagnostic_json(candidate: &RetrievalCandidate) -> Value {
    let matched_token = candidate.matched_seeds.first().cloned();
    json!({
        "candidate_id": candidate.candidate_id.clone(),
        "candidate_source": mcp_enum_snake_case(candidate.candidate_source),
        "candidate_sources": mcp_retrieval_candidate_sources(candidate),
        "path": candidate.path.clone(),
        "entity_id": candidate.entity_id.clone(),
        "matched_token": matched_token,
        "matched_tokens": candidate.matched_seeds.clone(),
        "rescue_reason": candidate
            .metadata
            .get("rescue_basis")
            .and_then(Value::as_str)
            .unwrap_or("rare_token_identifier_overlap"),
        "proof_status": mcp_enum_snake_case(candidate.proof_status),
        "graph_proof": candidate.graph_proof,
        "claimable_for_graph": candidate.claimable_for_graph,
        "verification_status": mcp_enum_snake_case(candidate.verification_status),
        "score": candidate.score,
        "rank": candidate.rank,
    })
}

fn mcp_retrieval_candidate_sources(candidate: &RetrievalCandidate) -> Vec<String> {
    let mut sources = BTreeSet::new();
    sources.insert(mcp_enum_snake_case(candidate.candidate_source));
    if let Some(source) = candidate
        .metadata
        .get("upstream_candidate_source")
        .and_then(Value::as_str)
    {
        sources.insert(source.to_string());
    }
    sources.into_iter().collect()
}

fn mcp_enum_snake_case<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn mcp_compact_nuance_rescue_diagnostics(value: Value) -> Value {
    let Some(object) = value.as_object() else {
        return Value::Null;
    };
    json!({
        "nuance_rescue_enabled": object
            .get("nuance_rescue_enabled")
            .or_else(|| object.get("rescue_enabled"))
            .cloned()
            .unwrap_or(Value::Bool(false)),
        "rescue_status": object.get("rescue_status").cloned().unwrap_or(Value::Null),
        "candidate_count": object.get("candidate_count").cloned().unwrap_or(Value::Null),
        "rescue_candidate_count": object
            .get("rescue_candidate_count")
            .or_else(|| object.get("candidate_count"))
            .cloned()
            .unwrap_or(Value::Null),
        "dropped_count": object.get("dropped_count").cloned().unwrap_or(Value::Null),
        "proof_contract": object.get("proof_contract").cloned().unwrap_or(Value::Null),
    })
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct McpCandidateSpoolLoad {
    path: PathBuf,
    metadata: Value,
    chunks: Vec<Value>,
    stale: bool,
    reason: Option<String>,
}

fn mcp_default_candidate_spool_path(repo_root: &Path) -> PathBuf {
    default_db_path(repo_root)
        .parent()
        .map(|parent| parent.join("codegraph-candidate-spool.jsonl"))
        .unwrap_or_else(|| PathBuf::from("codegraph-candidate-spool.jsonl"))
}

fn mcp_default_vector_runtime_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(|parent| parent.join(MCP_VECTOR_INDEX_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(MCP_VECTOR_INDEX_FILE_NAME))
}

fn mcp_default_vector_audit_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(|parent| parent.join(MCP_VECTOR_AUDIT_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(MCP_VECTOR_AUDIT_FILE_NAME))
}

fn mcp_resolve_artifact_path(path: Option<&str>, default_path: PathBuf) -> PathBuf {
    let path = path.map(PathBuf::from).unwrap_or(default_path);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    }
}

#[allow(dead_code)]
fn mcp_load_candidate_spool_for_repo(
    repo_root: &Path,
    spool_path: &Path,
    allow_stale: bool,
) -> Result<McpCandidateSpoolLoad, ToolCallError> {
    if !spool_path.exists() {
        return Err(ToolCallError::new(
            "candidate_spool_missing",
            format!("candidate spool does not exist: {}", spool_path.display()),
        ));
    }
    let text = fs::read_to_string(spool_path).map_err(|error| {
        ToolCallError::new(
            "candidate_spool_read_failed",
            format!(
                "could not read candidate spool {}: {error}",
                spool_path.display()
            ),
        )
    })?;
    let mut metadata = Value::Null;
    let mut chunks = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|error| {
            ToolCallError::new(
                "candidate_spool_corrupt",
                format!(
                    "candidate spool JSONL line {} is invalid: {error}",
                    line_index + 1
                ),
            )
        })?;
        if value.get("chunk_id").is_some() || value.get("text").is_some() {
            chunks.push(value);
        } else if metadata.is_null() {
            metadata = value
                .get("metadata")
                .cloned()
                .unwrap_or_else(|| value.clone());
        }
    }
    let mut stale = false;
    let mut reason = None;
    if let Some(spool_repo) = metadata.get("repo_root").and_then(Value::as_str) {
        let spool_repo = PathBuf::from(spool_repo);
        if !paths_equivalent(repo_root, &spool_repo) {
            stale = true;
            reason = Some("candidate spool repo_root does not match current repo".to_string());
        }
    }
    if stale && !allow_stale {
        return Err(ToolCallError::new(
            "candidate_spool_stale",
            reason
                .clone()
                .unwrap_or_else(|| "candidate spool is stale".to_string()),
        ));
    }
    Ok(McpCandidateSpoolLoad {
        path: spool_path.to_path_buf(),
        metadata,
        chunks,
        stale,
        reason,
    })
}

fn mcp_candidate_spool_context_pack(
    task: &str,
    mode: &str,
    response_mode: &str,
    limit: usize,
    repo_root: &Path,
    db_path: &Path,
    spool_path: &Path,
    allow_stale: bool,
    db_problem: Option<String>,
) -> Result<Value, ToolCallError> {
    let mut query_results = Vec::new();
    let mut candidates_by_id = BTreeMap::<String, Value>::new();
    for subcommand in ["symbols", "files", "text"] {
        let query_result = query_candidate_spool_index_for_repo(
            repo_root,
            spool_path,
            subcommand,
            task,
            limit,
            allow_stale,
        )
        .map_err(|error| {
            ToolCallError::new("candidate_spool_query_index_failed", error.to_string())
        })?;
        for candidate in mcp_candidate_spool_index_query_hits(&query_result) {
            let key = candidate
                .get("chunk_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            candidates_by_id.entry(key).or_insert(candidate);
        }
        query_results.push(query_result);
    }
    let Some(first_query_result) = query_results.first() else {
        return Err(ToolCallError::new(
            "candidate_spool_query_index_failed",
            "no candidate spool query result",
        ));
    };
    let candidates = candidates_by_id
        .into_values()
        .take(limit)
        .collect::<Vec<_>>();
    let staged_availability = mcp_staged_availability_for_spool_index_only(
        repo_root,
        db_path,
        spool_path,
        &first_query_result.load,
        db_problem,
    );
    let staged_fields = mcp_staged_top_level_fields(&staged_availability);
    let lifecycle = mcp_candidate_spool_index_lifecycle_json(&first_query_result.load);
    let mut value = json!({
        "status": "ok",
        "schema_version": 1,
        "command": "codegraph.context_pack",
        "response_mode": response_mode,
        "task": task,
        "mode": mode,
        "repo_root": path_string(repo_root),
        "db_path": path_string(db_path),
        "proof_status": "candidate_only",
        "proof_strength": "candidate_evidence",
        "graph_proof": false,
        "candidate_only": true,
        "claimable_for_graph": false,
        "candidate_spool": lifecycle,
        "staged_availability": staged_availability.clone(),
        "snippets": candidates,
        "fallback_evidence": [],
        "paths": [],
        "proof_path_count": 0,
        "warnings": ["Graph DB is still building; returning candidate-only spool context."],
        "errors": [],
        "proof": "MCP context_pack candidate spool mode returns candidate-only source-navigation evidence and does not trigger indexing.",
    });
    mcp_merge_json_object(&mut value, staged_fields);
    mcp_attach_rtds_freshness_fields(&mut value, repo_root, db_path, None, &staged_availability);
    mcp_attach_dirty_evidence_output_fields(&mut value, "codegraph.context_pack", false);
    Ok(value)
}

#[allow(dead_code)]
fn mcp_candidate_spool_query_hits(
    spool: &McpCandidateSpoolLoad,
    task: &str,
    limit: usize,
) -> Vec<Value> {
    let terms = task
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-' && ch != '/')
        .map(|term| term.to_ascii_lowercase())
        .filter(|term| term.len() >= 2)
        .collect::<Vec<_>>();
    let mut scored = Vec::new();
    for chunk in &spool.chunks {
        let haystack = mcp_candidate_spool_haystack(chunk);
        let score = terms
            .iter()
            .filter(|term| haystack.contains(term.as_str()))
            .count();
        if score == 0 && !terms.is_empty() {
            continue;
        }
        scored.push((score, mcp_candidate_spool_hit_json(chunk)));
    }
    scored.sort_by(|left, right| {
        right.0.cmp(&left.0).then_with(|| {
            mcp_candidate_spool_sort_key(&left.1).cmp(&mcp_candidate_spool_sort_key(&right.1))
        })
    });
    scored
        .into_iter()
        .map(|(_, value)| value)
        .take(limit)
        .collect()
}

fn mcp_candidate_spool_index_query_hits(
    query_result: &CandidateSpoolIndexQueryResult,
) -> Vec<Value> {
    query_result
        .chunks
        .iter()
        .map(mcp_candidate_spool_hit_json)
        .collect()
}

#[allow(dead_code)]
fn mcp_candidate_spool_haystack(chunk: &Value) -> String {
    [
        chunk.get("path").and_then(Value::as_str).unwrap_or(""),
        chunk.get("entity_id").and_then(Value::as_str).unwrap_or(""),
        chunk.get("text").and_then(Value::as_str).unwrap_or(""),
        chunk
            .get("selection_reason")
            .and_then(Value::as_str)
            .unwrap_or(""),
        chunk
            .get("source_kind")
            .and_then(Value::as_str)
            .unwrap_or(""),
        chunk
            .get("chunk_kind")
            .and_then(Value::as_str)
            .unwrap_or(""),
    ]
    .join(" ")
    .to_ascii_lowercase()
}

fn mcp_candidate_spool_hit_json(chunk: &Value) -> Value {
    let span = chunk.get("source_span").cloned().unwrap_or(Value::Null);
    let text = chunk.get("text").and_then(Value::as_str).unwrap_or("");
    let chunk_kind = chunk
        .get("chunk_kind")
        .and_then(Value::as_str)
        .unwrap_or("candidate");
    let proof_strength = match chunk
        .get("source_kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "text_evidence" => "text_evidence",
        "graph_entity" => "symbol_evidence",
        _ if chunk_kind == "file_path_title" => "source_navigation_evidence",
        _ => "candidate_evidence",
    };
    json!({
        "chunk_id": chunk.get("chunk_id").cloned().unwrap_or(Value::Null),
        "chunk_kind": chunk.get("chunk_kind").cloned().unwrap_or(Value::Null),
        "source_kind": chunk.get("source_kind").cloned().unwrap_or(Value::Null),
        "path": chunk.get("path").cloned().unwrap_or(Value::Null),
        "source_span": span,
        "entity_id": chunk.get("entity_id").cloned().unwrap_or(Value::Null),
        "text": compact_mcp_text(text, 600),
        "selection_score": chunk.get("selection_score").cloned().unwrap_or(Value::Null),
        "selection_bucket": chunk.get("selection_bucket").cloned().unwrap_or(Value::Null),
        "selection_reason": chunk.get("selection_reason").cloned().unwrap_or(Value::Null),
        "proof_status": "candidate_only",
        "proof_strength": proof_strength,
        "graph_proof": false,
        "claimable_for_graph": false,
        "requires_graph_verification": true,
        "candidate_only": true,
    })
}

#[allow(dead_code)]
fn mcp_candidate_spool_sort_key(value: &Value) -> String {
    format!(
        "{}:{}",
        value.get("path").and_then(Value::as_str).unwrap_or(""),
        value.get("chunk_id").and_then(Value::as_str).unwrap_or("")
    )
}

#[allow(dead_code)]
fn mcp_candidate_spool_lifecycle_json(spool: &McpCandidateSpoolLoad) -> Value {
    json!({
        "candidate_spool_status": if spool.stale {
            "stale"
        } else {
            spool.metadata
                .get("candidate_spool_status")
                .and_then(Value::as_str)
                .or_else(|| spool.metadata.get("status").and_then(Value::as_str))
                .unwrap_or("unknown")
        },
        "candidate_spool_path": path_string(&spool.path),
        "artifact_kind": "candidate_spool",
        "artifact_format": "jsonl",
        "lifecycle": spool.metadata.get("lifecycle").cloned().unwrap_or(Value::Null),
        "incomplete": spool.metadata.get("incomplete").and_then(Value::as_bool).unwrap_or(true),
        "stale": spool.stale,
        "reason": spool.reason.clone(),
        "spooled_total_chunks": spool.chunks.len(),
        "candidate_only": true,
        "graph_proof": false,
        "claimable_for_graph": false,
    })
}

fn mcp_candidate_spool_index_lifecycle_json(spool: &CandidateSpoolIndexLoad) -> Value {
    json!({
        "candidate_spool_status": if spool.stale {
            "stale"
        } else {
            spool.metadata
                .get("candidate_spool_status")
                .and_then(Value::as_str)
                .or_else(|| spool.metadata.get("status").and_then(Value::as_str))
                .unwrap_or("unknown")
        },
        "candidate_spool_path": path_string(&spool.path),
        "artifact_kind": "candidate_spool",
        "artifact_format": spool.metadata.get("artifact_format").and_then(Value::as_str).unwrap_or("jsonl"),
        "lifecycle": spool.metadata.get("lifecycle").cloned().unwrap_or(Value::Null),
        "candidate_spool_policy": spool.metadata.get("candidate_spool_policy").cloned().unwrap_or_else(|| json!("bounded")),
        "candidate_spool_required": spool.metadata.get("candidate_spool_required").cloned().unwrap_or_else(|| json!(false)),
        "candidate_spool_budget_bytes": spool.metadata.get("candidate_spool_budget_bytes").cloned().unwrap_or(Value::Null),
        "candidate_spool_written_bytes": spool.metadata.get("candidate_spool_written_bytes").cloned().unwrap_or(Value::Null),
        "candidate_spool_truncated": spool.metadata.get("candidate_spool_truncated").cloned().unwrap_or_else(|| json!(false)),
        "candidate_spool_disabled_reason": spool.metadata.get("candidate_spool_disabled_reason").cloned().unwrap_or(Value::Null),
        "candidate_spool_warning": spool.metadata.get("candidate_spool_warning").cloned().unwrap_or(Value::Null),
        "artifact_budget_remaining_bytes": spool.metadata.get("artifact_budget_remaining_bytes").cloned().unwrap_or(Value::Null),
        "artifact_budget_decision": spool.metadata.get("artifact_budget_decision").cloned().unwrap_or(Value::Null),
        "incomplete": spool.metadata.get("incomplete").and_then(Value::as_bool).unwrap_or(true),
        "stale": spool.stale,
        "reason": spool.reason.clone(),
        "spooled_total_chunks": spool.query_index_record_count,
        "query_index_status": spool.query_index_status.clone(),
        "query_index_kind": spool.query_index_kind.clone(),
        "query_index_path": path_string(&spool.query_index_path),
        "query_index_bytes": spool.query_index_bytes,
        "query_index_record_count": spool.query_index_record_count,
        "query_index_source_binding_count": spool.query_index_source_binding_count,
        "query_index_version": spool.query_index_version.clone(),
        "candidate_only": true,
        "graph_proof": false,
        "claimable_for_graph": false,
    })
}

fn mcp_staged_availability(
    repo_root: &Path,
    db_path: &Path,
    preflight: Option<&DbLifecyclePreflight>,
    args: &Map<String, Value>,
    vector_branch: Option<&McpVectorBranch>,
) -> Value {
    let graph = mcp_graph_layer_status(preflight);
    let candidate_spool_path = optional_string(args, "candidate_spool")
        .or_else(|| optional_string(args, "candidate_spool_path"))
        .or_else(|| optional_string(args, "candidateSpool"))
        .or_else(|| optional_string(args, "candidateSpoolPath"));
    let spool_path = mcp_resolve_artifact_path(
        candidate_spool_path.as_deref(),
        mcp_default_candidate_spool_path(repo_root),
    );
    let spool = mcp_candidate_spool_layer_status(repo_root, &spool_path);
    let vector_path_arg = optional_string(args, "vector_index")
        .or_else(|| optional_string(args, "vector_index_path"))
        .or_else(|| optional_string(args, "vector_runtime_sidecar"))
        .or_else(|| optional_string(args, "vectorIndex"))
        .or_else(|| optional_string(args, "vectorIndexPath"))
        .or_else(|| optional_string(args, "vectorRuntimeSidecar"));
    let runtime_path = vector_branch
        .map(|branch| branch.index_path.clone())
        .unwrap_or_else(|| {
            mcp_resolve_artifact_path(
                vector_path_arg.as_deref(),
                mcp_default_vector_runtime_path(db_path),
            )
        });
    let runtime = mcp_vector_runtime_layer_status(
        repo_root,
        db_path,
        preflight,
        &runtime_path,
        vector_branch,
    );
    let audit_path_arg = optional_string(args, "vector_audit_artifact")
        .or_else(|| optional_string(args, "vectorAuditArtifact"));
    let audit_path = mcp_resolve_artifact_path(
        audit_path_arg.as_deref(),
        mcp_default_vector_audit_path(db_path),
    );
    let audit = mcp_vector_audit_layer_status(preflight, &audit_path);
    mcp_staged_availability_from_layers(graph, spool, runtime, audit)
}

fn mcp_staged_availability_for_spool_index_only(
    _repo_root: &Path,
    _db_path: &Path,
    spool_path: &Path,
    spool: &CandidateSpoolIndexLoad,
    db_problem: Option<String>,
) -> Value {
    let graph = json!({
        "layer": "graph_db",
        "status": "building",
        "ready": false,
        "graph_proof_available": false,
        "path": Value::Null,
        "reason": db_problem.unwrap_or_else(|| "Graph DB is still building or unavailable.".to_string()),
    });
    let spool = mcp_candidate_spool_layer_from_index_load(spool_path, spool);
    let runtime = json!({
        "layer": "vector_runtime",
        "status": "missing",
        "ready": false,
        "path": Value::Null,
        "candidate_only": true,
        "graph_proof": false,
        "complete_path_symbol_index": false,
        "reason": "runtime vector sidecar missing",
    });
    let audit = json!({
        "layer": "vector_audit",
        "status": "missing",
        "ready": false,
        "path": Value::Null,
        "diagnostic_only": true,
        "runtime_dependency": false,
    });
    mcp_staged_availability_from_layers(graph, spool, runtime, audit)
}

fn mcp_graph_layer_status(preflight: Option<&DbLifecyclePreflight>) -> Value {
    let Some(preflight) = preflight else {
        return json!({
            "layer": "graph_db",
            "status": "unknown",
            "ready": false,
            "graph_proof_available": false,
            "reason": "graph DB lifecycle was not inspected",
        });
    };
    let status = if preflight.safe {
        "ready"
    } else if preflight.path_access_status == "db_missing" {
        "no_index"
    } else {
        preflight.db_problem_kind.as_deref().unwrap_or("blocked")
    };
    json!({
        "layer": "graph_db",
        "status": status,
        "ready": preflight.safe,
        "graph_proof_available": preflight.safe,
        "path": preflight.exact_db_path_checked.clone(),
        "lifecycle_decision": if preflight.safe { "read_reuse" } else { "blocked" },
        "claimable": preflight.safe,
        "diagnostic_only": !preflight.safe,
        "passport_status": preflight.db_health.passport_status.clone(),
        "schema_status": preflight.schema_status.clone(),
        "scope_status": preflight.scope_status.clone(),
        "blockers": preflight.blockers.clone(),
        "warnings": preflight.warnings.clone(),
    })
}

fn mcp_candidate_spool_layer_status(repo_root: &Path, spool_path: &Path) -> Value {
    if !spool_path.exists() {
        return json!({
                "layer": "candidate_spool",
                "status": "no_spool",
                "ready": false,
                "path": path_string(spool_path),
            "candidate_only": true,
            "graph_proof": false,
            "candidate_spool_unavailable": true,
            "candidate_context_truncated": false,
        });
    }
    match candidate_spool_index_status_for_repo(repo_root, spool_path, true) {
        Ok(spool) => mcp_candidate_spool_layer_from_index_load(spool_path, &spool),
        Err(error) => json!({
            "layer": "candidate_spool",
            "status": if error.to_string().contains("corrupt") { "corrupt" } else { "foreign" },
            "ready": false,
            "path": path_string(spool_path),
            "candidate_only": true,
            "graph_proof": false,
            "candidate_spool_unavailable": true,
            "candidate_context_truncated": false,
            "reason": error.to_string(),
        }),
    }
}

fn mcp_candidate_spool_layer_from_index_load(
    spool_path: &Path,
    spool: &CandidateSpoolIndexLoad,
) -> Value {
    let lifecycle = mcp_candidate_spool_index_lifecycle_json(spool);
    let status = match spool.query_index_status.as_str() {
        "index_missing" => "query_index_missing",
        "permission_denied" => "permission_denied",
        "filesystem_inaccessible" => "filesystem_inaccessible",
        "sidecar_unavailable" => "sidecar_unavailable",
        "corrupt" => "query_index_corrupt",
        "stale" => "stale",
        _ => lifecycle
            .get("candidate_spool_status")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
    };
    let ready = matches!(
        status,
        "building"
            | "partial_ready"
            | "bounded_ready"
            | "truncated_ready"
            | "superseded_by_graph_db"
    ) && spool.query_index_status == "ready";
    let candidate_context_truncated = lifecycle
        .get("candidate_spool_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    json!({
        "layer": "candidate_spool",
        "status": status,
        "ready": ready,
        "path": path_string(spool_path),
        "candidate_only": true,
        "graph_proof": false,
        "proof_strength": "candidate_evidence",
        "source_navigation_evidence": true,
        "spooled_total_chunks": spool.query_index_record_count,
        "incomplete": lifecycle.get("incomplete").cloned().unwrap_or(Value::Null),
        "stale": spool.stale,
        "reason": spool.reason.clone(),
        "query_index_status": spool.query_index_status.clone(),
        "query_index_problem_kind": spool.query_index_status.clone(),
        "query_index_kind": spool.query_index_kind.clone(),
        "query_index_path": path_string(&spool.query_index_path),
        "query_index_bytes": spool.query_index_bytes,
        "query_index_record_count": spool.query_index_record_count,
        "query_index_source_binding_count": spool.query_index_source_binding_count,
        "query_index_version": spool.query_index_version.clone(),
        "candidate_context_truncated": candidate_context_truncated,
        "candidate_spool_unavailable": !ready,
        "complete_path_symbol_index": false,
        "candidate_spool_policy": lifecycle.get("candidate_spool_policy").cloned().unwrap_or_else(|| json!("bounded")),
        "candidate_spool_required": lifecycle.get("candidate_spool_required").cloned().unwrap_or_else(|| json!(false)),
        "candidate_spool_budget_bytes": lifecycle.get("candidate_spool_budget_bytes").cloned().unwrap_or(Value::Null),
        "candidate_spool_written_bytes": lifecycle.get("candidate_spool_written_bytes").cloned().unwrap_or(Value::Null),
        "candidate_spool_truncated": lifecycle.get("candidate_spool_truncated").cloned().unwrap_or_else(|| json!(false)),
        "candidate_spool_disabled_reason": lifecycle.get("candidate_spool_disabled_reason").cloned().unwrap_or(Value::Null),
        "candidate_spool_warning": lifecycle.get("candidate_spool_warning").cloned().unwrap_or(Value::Null),
        "artifact_budget_remaining_bytes": lifecycle.get("artifact_budget_remaining_bytes").cloned().unwrap_or(Value::Null),
        "artifact_budget_decision": lifecycle.get("artifact_budget_decision").cloned().unwrap_or(Value::Null),
    })
}

#[allow(dead_code)]
fn mcp_candidate_spool_layer_from_load(spool_path: &Path, spool: &McpCandidateSpoolLoad) -> Value {
    let lifecycle = mcp_candidate_spool_lifecycle_json(spool);
    let status = lifecycle
        .get("candidate_spool_status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let ready = matches!(
        status,
        "building"
            | "partial_ready"
            | "bounded_ready"
            | "truncated_ready"
            | "superseded_by_graph_db"
    );
    json!({
        "layer": "candidate_spool",
        "status": status,
        "ready": ready,
        "path": path_string(spool_path),
        "candidate_only": true,
        "graph_proof": false,
        "proof_strength": "candidate_evidence",
        "source_navigation_evidence": true,
        "spooled_total_chunks": spool.chunks.len(),
        "incomplete": lifecycle.get("incomplete").cloned().unwrap_or(Value::Null),
        "stale": spool.stale,
        "candidate_context_truncated": lifecycle.get("candidate_spool_truncated").cloned().unwrap_or_else(|| json!(false)),
        "candidate_spool_unavailable": !ready,
        "complete_path_symbol_index": false,
        "reason": spool.reason.clone(),
    })
}

fn mcp_vector_runtime_layer_status(
    repo_root: &Path,
    _db_path: &Path,
    preflight: Option<&DbLifecyclePreflight>,
    runtime_path: &Path,
    branch: Option<&McpVectorBranch>,
) -> Value {
    if let Some(branch) = branch {
        return json!({
            "layer": "vector_runtime",
            "status": branch.status.as_str(),
            "ready": matches!(branch.status, VectorCandidateBranchStatus::Ready),
            "path": path_string(runtime_path),
            "candidate_only": true,
            "graph_proof": false,
            "complete_path_symbol_index": false,
            "selected_chunks_only": true,
            "candidate_count": branch.candidates.len(),
            "reason": branch.warning.clone(),
        });
    }
    if !runtime_path.exists() {
        return json!({
            "layer": "vector_runtime",
            "status": "missing",
            "ready": false,
            "path": path_string(runtime_path),
            "candidate_only": true,
            "graph_proof": false,
            "complete_path_symbol_index": false,
            "reason": "runtime vector sidecar missing",
        });
    }
    let Some(preflight) = preflight else {
        return json!({
            "layer": "vector_runtime",
            "status": "present_unvalidated",
            "ready": false,
            "path": path_string(runtime_path),
            "candidate_only": true,
            "graph_proof": false,
            "reason": "runtime sidecar exists but graph DB validation was not run",
        });
    };
    if !preflight.safe {
        return json!({
            "layer": "vector_runtime",
            "status": "blocked_by_graph_db",
            "ready": false,
            "path": path_string(runtime_path),
            "candidate_only": true,
            "graph_proof": false,
            "reason": "runtime sidecar validation requires a valid graph DB passport",
        });
    }
    let Some(passport) = preflight.db_health.passport.as_ref() else {
        return json!({
            "layer": "vector_runtime",
            "status": "stale",
            "ready": false,
            "path": path_string(runtime_path),
            "candidate_only": true,
            "graph_proof": false,
            "reason": "graph DB passport missing",
        });
    };
    let provider = match mcp_vector_provider() {
        Ok(provider) => provider,
        Err(error) => {
            return json!({
                "layer": "vector_runtime",
                "status": "stale",
                "ready": false,
                "path": path_string(runtime_path),
                "candidate_only": true,
                "graph_proof": false,
                "reason": error,
            });
        }
    };
    match load_vector_chunk_index_json(runtime_path, &provider, passport, mcp_vector_options()) {
        Ok(index) => {
            let source_binding_validation = match validate_vector_chunk_source_bindings(
                repo_root, &index,
            ) {
                Ok(validation) if validation.is_valid() => validation,
                Ok(validation) => {
                    return json!({
                        "layer": "vector_runtime",
                        "status": "stale",
                        "ready": false,
                        "path": path_string(runtime_path),
                        "candidate_only": true,
                        "graph_proof": false,
                        "reason": format!("vector_source_binding_stale: {}", validation.stale_reasons.join("; ")),
                        "source_binding_validation": serde_json::to_value(validation).unwrap_or(Value::Null),
                    });
                }
                Err(error) => {
                    return json!({
                        "layer": "vector_runtime",
                        "status": "stale",
                        "ready": false,
                        "path": path_string(runtime_path),
                        "candidate_only": true,
                        "graph_proof": false,
                        "reason": format!("vector source binding validation failed: {error}"),
                    });
                }
            };
            json!({
                "layer": "vector_runtime",
                "status": "ready",
                "ready": true,
                "path": path_string(runtime_path),
                "candidate_only": true,
                "graph_proof": false,
                "complete_path_symbol_index": false,
                "selected_chunks_only": true,
                "source_binding_validation": serde_json::to_value(source_binding_validation).unwrap_or(Value::Null),
            })
        }
        Err(error) => json!({
            "layer": "vector_runtime",
            "status": "stale",
            "ready": false,
            "path": path_string(runtime_path),
            "candidate_only": true,
            "graph_proof": false,
            "reason": error.to_string(),
        }),
    }
}

fn mcp_vector_audit_layer_status(
    preflight: Option<&DbLifecyclePreflight>,
    audit_path: &Path,
) -> Value {
    if !audit_path.exists() {
        return json!({
            "layer": "vector_audit",
            "status": "missing",
            "ready": false,
            "path": path_string(audit_path),
            "diagnostic_only": true,
            "runtime_dependency": false,
        });
    }
    let value = match fs::read_to_string(audit_path)
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|error| error.to_string()))
    {
        Ok(value) => value,
        Err(error) => {
            return json!({
                "layer": "vector_audit",
                "status": "corrupt",
                "ready": false,
                "path": path_string(audit_path),
                "diagnostic_only": true,
                "runtime_dependency": false,
                "reason": error,
            });
        }
    };
    let metadata = value.get("metadata").unwrap_or(&value);
    let artifact_kind = metadata
        .get("artifact_kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let mut status = if artifact_kind == "audit_artifact" {
        "ready"
    } else {
        "unknown"
    };
    let mut reason = None;
    if let Some(preflight) = preflight.filter(|preflight| preflight.safe) {
        if let Some(passport) = preflight.db_health.passport.as_ref() {
            let artifact_scope = metadata
                .get("passport")
                .and_then(|passport| passport.get("index_scope_policy_hash"))
                .and_then(Value::as_str);
            if artifact_scope.is_some()
                && artifact_scope != Some(passport.index_scope_policy_hash.as_str())
            {
                status = "stale";
                reason = Some("audit artifact passport scope differs from graph DB".to_string());
            }
        }
    }
    json!({
        "layer": "vector_audit",
        "status": status,
        "ready": matches!(status, "ready" | "diagnostic_only" | "stale"),
        "path": path_string(audit_path),
        "artifact_kind": artifact_kind,
        "artifact_format": metadata.get("index_artifact_format").cloned().unwrap_or(Value::Null),
        "diagnostic_only": true,
        "runtime_dependency": false,
        "reason": reason,
    })
}

fn mcp_staged_availability_from_layers(
    graph: Value,
    spool: Value,
    runtime: Value,
    audit: Value,
) -> Value {
    let layers = vec![
        ("graph_db", graph),
        ("candidate_spool", spool),
        ("vector_runtime", runtime),
        ("vector_audit", audit),
    ];
    let mut layer_readiness = Map::new();
    let mut available_layers = Vec::new();
    let mut missing_layers = Vec::new();
    let mut active_candidate_sources = Vec::new();
    let mut warnings = Vec::new();
    for (name, layer) in &layers {
        let status = layer
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let ready = layer.get("ready").and_then(Value::as_bool).unwrap_or(false);
        layer_readiness.insert((*name).to_string(), layer.clone());
        if ready {
            available_layers.push((*name).to_string());
        } else if matches!(
            status,
            "missing"
                | "no_spool"
                | "no_index"
                | "blocked"
                | "corrupt"
                | "foreign"
                | "stale"
                | "query_index_missing"
                | "query_index_corrupt"
                | "permission_denied"
                | "filesystem_inaccessible"
                | "sidecar_unavailable"
                | "disabled_budget_exceeded"
        ) {
            missing_layers.push((*name).to_string());
        }
    }
    let graph_ready = mcp_layer_ready(&layer_readiness, "graph_db");
    let spool_ready = mcp_layer_ready(&layer_readiness, "candidate_spool");
    let vector_ready = mcp_layer_ready(&layer_readiness, "vector_runtime");
    let audit_ready = mcp_layer_ready(&layer_readiness, "vector_audit");
    if spool_ready {
        active_candidate_sources.push("candidate_spool".to_string());
    }
    if graph_ready {
        active_candidate_sources.push("graph_db".to_string());
        active_candidate_sources.push("stage0_text_evidence".to_string());
        active_candidate_sources.push("symbol_lookup".to_string());
    }
    if vector_ready {
        active_candidate_sources.push("vector_semantic".to_string());
    }
    if !graph_ready && spool_ready {
        warnings.push(
            "Graph DB is still building; returning candidate-only spool context.".to_string(),
        );
    }
    let spool_status = layer_readiness
        .get("candidate_spool")
        .and_then(|layer| layer.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("no_spool");
    let candidate_context_truncated = layer_readiness
        .get("candidate_spool")
        .and_then(|layer| layer.get("candidate_context_truncated"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let candidate_spool_unavailable = layer_readiness
        .get("candidate_spool")
        .and_then(|layer| layer.get("candidate_spool_unavailable"))
        .and_then(Value::as_bool)
        .unwrap_or(!spool_ready);
    if candidate_context_truncated {
        warnings.push(
            "Candidate spool context is truncated; returned spans are a bounded working set."
                .to_string(),
        );
    }
    if matches!(spool_status, "disabled_budget_exceeded") {
        warnings.push(
            "Candidate spool unavailable: storage budget disabled the optional spool.".to_string(),
        );
    }
    let vector_status = layer_readiness
        .get("vector_runtime")
        .and_then(|layer| layer.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("missing")
        .to_string();
    if vector_status == "stale" {
        warnings.push("Vector runtime sidecar is stale; vector candidates omitted.".to_string());
    }
    if vector_ready
        || matches!(
            vector_status.as_str(),
            "ready" | "not_requested" | "present_unvalidated"
        )
    {
        warnings
            .push("Selected vector sidecar is not a complete file/path/symbol index.".to_string());
    }
    if audit_ready {
        warnings.push(
            "Audit artifact exists but is diagnostic-only and not used for runtime retrieval."
                .to_string(),
        );
    }
    let candidate_only_available = spool_ready || vector_ready;
    json!({
        "schema_version": 1,
        "available_layers": available_layers,
        "missing_layers": missing_layers,
        "layer_readiness": Value::Object(layer_readiness),
        "graph_db_status": mcp_layer_status(&layers, "graph_db"),
        "candidate_spool_status": mcp_layer_status(&layers, "candidate_spool"),
        "vector_runtime_status": mcp_layer_status(&layers, "vector_runtime"),
        "vector_audit_status": mcp_layer_status(&layers, "vector_audit"),
        "active_candidate_sources": active_candidate_sources,
        "candidate_context_available": candidate_only_available || graph_ready,
        "candidate_context_truncated": candidate_context_truncated,
        "candidate_spool_unavailable": candidate_spool_unavailable,
        "candidate_only_available": candidate_only_available,
        "graph_proof_available": graph_ready,
        "lifecycle_decision": if graph_ready { "read_reuse" } else if spool_ready { "partial_candidate_context" } else { "blocked" },
        "claimability": {
            "claimable": graph_ready,
            "candidate_only": candidate_only_available && !graph_ready,
            "graph_proof_available": graph_ready,
            "diagnostic_only": false,
        },
        "recommended_next_step": if vector_status == "stale" {
            "rebuild vector sidecar"
        } else if !graph_ready && spool_ready {
            "inspect candidate spans"
        } else if !graph_ready {
            "wait_for_graph_db"
        } else {
            "run final graph verification"
        },
        "risks": [
            {"risk_id": "candidate_context_not_graph_proof", "sentence": "Candidate context is not graph proof."},
            {"risk_id": "vector_sidecar_not_complete_path_index", "sentence": "Selected vector sidecar is not a complete file/path/symbol index."},
            {"risk_id": "audit_artifact_not_runtime_source", "sentence": "Audit artifact exists only for diagnostics and is not used for runtime retrieval."}
        ],
        "warnings": warnings,
        "blockers": if graph_ready { Vec::<String>::new() } else { vec!["graph_db_not_ready".to_string()] },
        "public_claim": false,
    })
}

fn mcp_layer_ready(layer_readiness: &Map<String, Value>, layer_name: &str) -> bool {
    layer_readiness
        .get(layer_name)
        .and_then(|layer| layer.get("ready"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn mcp_layer_status(layers: &[(&str, Value)], layer_name: &str) -> Value {
    layers
        .iter()
        .find(|(name, _)| *name == layer_name)
        .and_then(|(_, layer)| layer.get("status").cloned())
        .unwrap_or_else(|| json!("unknown"))
}

fn mcp_staged_top_level_fields(staged: &Value) -> Value {
    json!({
        "staged_availability": staged,
        "graph_db_status": staged.get("graph_db_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_status": staged.get("candidate_spool_status").cloned().unwrap_or(Value::Null),
        "vector_runtime_status": staged.get("vector_runtime_status").cloned().unwrap_or(Value::Null),
        "vector_audit_status": staged.get("vector_audit_status").cloned().unwrap_or(Value::Null),
        "active_candidate_sources": staged.get("active_candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "candidate_context_truncated": staged.get("candidate_context_truncated").cloned().unwrap_or_else(|| json!(false)),
        "candidate_spool_unavailable": staged.get("candidate_spool_unavailable").cloned().unwrap_or_else(|| json!(false)),
        "candidate_only_available": staged.get("candidate_only_available").cloned().unwrap_or_else(|| json!(false)),
        "graph_proof_available": staged.get("graph_proof_available").cloned().unwrap_or_else(|| json!(false)),
        "lifecycle_decision": staged.get("lifecycle_decision").cloned().unwrap_or(Value::Null),
        "claimability": staged.get("claimability").cloned().unwrap_or(Value::Null),
        "warnings": staged.get("warnings").cloned().unwrap_or_else(|| json!([])),
        "blockers": staged.get("blockers").cloned().unwrap_or_else(|| json!([])),
    })
}

fn mcp_attach_rtds_freshness_fields(
    value: &mut Value,
    repo_root: &Path,
    db_path: &Path,
    preflight: Option<&DbLifecyclePreflight>,
    staged_availability: &Value,
) {
    let rtds = mcp_rtds_freshness_json(repo_root, db_path, preflight, staged_availability);
    let profile_identity = mcp_profile_identity_json(repo_root, db_path);
    if let Some(object) = value.as_object_mut() {
        object.insert("profile_identity".to_string(), profile_identity.clone());
        object.insert(
            "profile_name".to_string(),
            profile_identity
                .get("profile_name")
                .cloned()
                .unwrap_or(Value::Null),
        );
        object.insert(
            "active_profile_name".to_string(),
            profile_identity
                .get("active_profile_name")
                .cloned()
                .unwrap_or(Value::Null),
        );
        object.insert(
            "repo_identity_label".to_string(),
            profile_identity
                .get("repo_identity_label")
                .cloned()
                .unwrap_or(Value::Null),
        );
        object.insert(
            "repo_identity_hash".to_string(),
            profile_identity
                .get("repo_identity_hash")
                .cloned()
                .unwrap_or(Value::Null),
        );
        object.insert("rtds_freshness".to_string(), rtds.clone());
        object.insert(
            "publish_state".to_string(),
            rtds.get("publish_state").cloned().unwrap_or(Value::Null),
        );
        object.insert(
            "last_delta_update_summary".to_string(),
            rtds.get("last_delta_update_summary")
                .cloned()
                .unwrap_or(Value::Null),
        );
        object.insert(
            "graph_freshness".to_string(),
            rtds.get("graph_freshness").cloned().unwrap_or(Value::Null),
        );
        object.insert(
            "delta_state".to_string(),
            rtds.get("delta_state").cloned().unwrap_or(Value::Null),
        );
        object.insert(
            "dirty_state".to_string(),
            rtds.get("dirty_state").cloned().unwrap_or(Value::Null),
        );
        object.insert(
            "stale_candidate_layers".to_string(),
            rtds.get("stale_candidate_layers")
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
        object.insert(
            "blocked_labels".to_string(),
            rtds.get("blocked_labels")
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
        object.insert(
            "retryable_labels".to_string(),
            rtds.get("retryable_labels")
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
        object.insert(
            "recovery_commands".to_string(),
            rtds.get("recovery_commands")
                .cloned()
                .unwrap_or_else(|| json!(mcp_agent_use_recovery_commands(repo_root))),
        );
        object.insert(
            "candidate_only_available".to_string(),
            staged_availability
                .get("candidate_only_available")
                .cloned()
                .unwrap_or_else(|| json!(false)),
        );
        object.insert(
            "graph_proof_available".to_string(),
            staged_availability
                .get("graph_proof_available")
                .cloned()
                .unwrap_or_else(|| json!(false)),
        );
    }
}

fn mcp_rtds_freshness_json(
    repo_root: &Path,
    db_path: &Path,
    preflight: Option<&DbLifecyclePreflight>,
    staged_availability: &Value,
) -> Value {
    let profile_identity = mcp_profile_identity_json(repo_root, db_path);
    let publish_state = mcp_agent_use_publish_state_json(db_path);
    let publish_active = publish_state
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let publish_status = publish_state
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("absent");
    let last_delta = mcp_agent_use_last_delta_state_json(db_path);
    let last_summary = last_delta
        .get("last_delta_update_summary")
        .cloned()
        .unwrap_or(Value::Null);
    let graph_layer_status = staged_availability
        .get("graph_db_status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let graph_freshness = if preflight.is_some_and(|preflight| preflight.safe) {
        "current".to_string()
    } else if let Some(preflight) = preflight {
        if preflight.path_access_status == "db_missing" {
            "absent".to_string()
        } else {
            preflight
                .db_problem_kind
                .clone()
                .unwrap_or_else(|| "unavailable".to_string())
        }
    } else {
        graph_layer_status.to_string()
    };
    let dirty_state = if publish_active {
        publish_status.to_string()
    } else if graph_freshness == "current" {
        "ready".to_string()
    } else {
        graph_freshness.clone()
    };
    let delta_state = if publish_active {
        publish_status.to_string()
    } else {
        last_delta
            .get("delta_state")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| dirty_state.clone())
    };
    let mut blocked_labels = BTreeSet::new();
    if let Some(preflight) = preflight {
        for blocker in &preflight.blockers {
            blocked_labels.insert(blocker.clone());
        }
    }
    if let Some(blockers) = staged_availability
        .get("blockers")
        .and_then(Value::as_array)
    {
        for blocker in blockers {
            if let Some(blocker) = blocker.as_str() {
                blocked_labels.insert(blocker.to_string());
            }
        }
    }
    let mut retryable_labels = BTreeSet::new();
    if publish_active && matches!(publish_status, "updating" | "publishing") {
        retryable_labels.insert("wait_for_current_update".to_string());
    }
    if blocked_labels
        .iter()
        .any(|label| label.to_ascii_lowercase().contains("locked"))
    {
        retryable_labels.insert("db_locked".to_string());
    }
    let stale_candidate_layers = mcp_stale_candidate_layers(staged_availability);
    json!({
        "schema_version": 1,
        "profile_name": profile_identity.get("profile_name").cloned().unwrap_or_else(|| json!("unknown")),
        "active_profile_name": profile_identity.get("active_profile_name").cloned().unwrap_or_else(|| json!("unknown")),
        "profile_identity": profile_identity,
        "repo_root": path_string(repo_root),
        "db_path": path_string(db_path),
        "graph_freshness": graph_freshness,
        "dirty_state": dirty_state,
        "delta_state": delta_state,
        "publish_state": publish_state,
        "last_delta_state": last_delta,
        "last_delta_update_summary": last_summary,
        "stale_candidate_layers": stale_candidate_layers,
        "candidate_only_available": staged_availability.get("candidate_only_available").cloned().unwrap_or_else(|| json!(false)),
        "graph_proof_available": staged_availability.get("graph_proof_available").cloned().unwrap_or_else(|| json!(false)),
        "candidate_context_available": staged_availability.get("candidate_context_available").cloned().unwrap_or_else(|| json!(false)),
        "blocked_labels": blocked_labels.into_iter().collect::<Vec<_>>(),
        "retryable_labels": retryable_labels.into_iter().collect::<Vec<_>>(),
        "recovery_commands": mcp_agent_use_recovery_commands(repo_root),
        "context_pack_graph_proof_policy": "refuse_unsafe_graph_proof",
        "candidate_context_policy": "candidate_only_only_when_current_source_bound",
        "startup_auto_index": false,
        "dot_codegraph_fallback": false,
        "public_claim": false,
    })
}

fn mcp_attach_dirty_evidence_output_fields(value: &mut Value, surface: &str, full_detail: bool) {
    let staged_availability = value
        .get("staged_availability")
        .cloned()
        .unwrap_or_else(|| value.clone());
    let proof_ladder_changes = value
        .get("proof_ladder_changes")
        .cloned()
        .or_else(|| {
            value
                .pointer("/validation_packet/proof_ladder_changes")
                .cloned()
        })
        .or_else(|| {
            value
                .pointer("/packet/metadata/proof_ladder_changes")
                .cloned()
        })
        .unwrap_or_else(|| json!({}));
    let recovery_commands = value
        .get("recovery_commands")
        .cloned()
        .or_else(|| value.get("validation_recovery_commands").cloned())
        .unwrap_or_else(|| json!([]));
    let fields = mcp_dirty_evidence_output_fields(
        surface,
        value,
        &staged_availability,
        &proof_ladder_changes,
        recovery_commands,
        full_detail,
    );
    mcp_merge_json_object(value, fields.clone());
    if let Some(packet) = value
        .get_mut("validation_packet")
        .filter(|packet| packet.is_object())
    {
        mcp_merge_json_object(packet, fields.clone());
    }
    if let Some(packet) = value
        .get_mut("hard_interrupt")
        .filter(|packet| packet.is_object())
    {
        mcp_merge_json_object(packet, mcp_dirty_evidence_hard_interrupt_fields(&fields));
    }
    mcp_append_dirty_evidence_expansion_handle(value, full_detail);
}

fn mcp_dirty_evidence_output_fields(
    surface: &str,
    source: &Value,
    staged_availability: &Value,
    proof_ladder_changes: &Value,
    recovery_commands: Value,
    full_detail: bool,
) -> Value {
    let graph_status = mcp_layer_freshness(staged_availability, "graph_db");
    let candidate_layer_status = mcp_layer_freshness(staged_availability, "candidate_spool");
    let vector_layer_status = mcp_layer_freshness(staged_availability, "vector_runtime");
    let vector_audit_status = mcp_layer_freshness(staged_availability, "vector_audit");
    let path_evidence_status = mcp_path_evidence_freshness(source);
    let source_navigation_status =
        mcp_source_navigation_freshness(staged_availability, &candidate_layer_status);
    let routing_handle_status = mcp_routing_handle_freshness(source);
    let sidecar_statuses = json!({
        "graph_db_status": graph_status,
        "candidate_layer_status": candidate_layer_status,
        "candidate_spool_query_index_status": mcp_candidate_query_index_freshness(staged_availability),
        "vector_layer_status": vector_layer_status,
        "vector_audit_status": vector_audit_status,
        "path_evidence_status": path_evidence_status,
        "source_navigation_status": source_navigation_status,
        "routing_handle_status": routing_handle_status,
        "graph_db_claimable": staged_availability
            .get("claimability")
            .and_then(|claimability| claimability.get("claimable"))
            .cloned()
            .or_else(|| staged_availability.get("graph_proof_available").cloned())
            .unwrap_or_else(|| json!(false)),
        "graph_proof_available": staged_availability
            .get("graph_proof_available")
            .cloned()
            .unwrap_or_else(|| json!(false)),
        "candidate_only_available": staged_availability
            .get("candidate_only_available")
            .cloned()
            .unwrap_or_else(|| json!(false)),
    });
    let proof_ladder_change_counts = mcp_proof_ladder_change_counts(proof_ladder_changes);
    let invalidated_evidence = mcp_invalidated_evidence_statuses(source, proof_ladder_changes);
    let refreshed_evidence = mcp_refreshed_evidence_statuses(source, proof_ladder_changes);
    let stale_evidence = mcp_stale_evidence_statuses(staged_availability, proof_ladder_changes);
    let unavailable_evidence = mcp_unavailable_evidence_statuses(staged_availability);
    let stale_non_proof_reasons =
        mcp_stale_non_proof_reasons(&stale_evidence, &unavailable_evidence);
    let graph_proof_available = staged_availability
        .get("graph_proof_available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let claimability_effect =
        mcp_output_claimability_effect(staged_availability, graph_proof_available);
    let graph_db_corrupt_or_unavailable = matches!(
        graph_status.as_str(),
        Some("corrupt" | "inaccessible" | "permission_denied")
    );

    json!({
        "dirty_evidence_summary": {
            "schema_version": 1,
            "surface": surface,
            "surfaces_total": 8,
            "fresh_count": mcp_count_fresh_dirty_statuses(&sidecar_statuses),
            "stale_count": stale_evidence.as_array().map(Vec::len).unwrap_or_default(),
            "dirty_count": invalidated_evidence.as_array().map(Vec::len).unwrap_or_default(),
            "unavailable_count": unavailable_evidence.as_array().map(Vec::len).unwrap_or_default(),
            "corrupt_count": mcp_count_status(&sidecar_statuses, "corrupt"),
            "not_applicable_count": mcp_count_status(&sidecar_statuses, "not_applicable"),
            "graph_proof_available": graph_proof_available,
            "sidecar_corrupt_or_unavailable": mcp_any_sidecar_corrupt_or_unavailable(&sidecar_statuses),
            "graph_db_corrupt_or_unavailable": graph_db_corrupt_or_unavailable,
            "text_evidence_is_not_graph_proof": true,
            "candidate_evidence_is_not_graph_proof": true,
            "vector_evidence_is_not_graph_proof": true,
            "source_navigation_evidence_is_not_graph_proof": true,
            "summary_only": !full_detail,
        },
        "invalidated_evidence": invalidated_evidence,
        "refreshed_evidence": refreshed_evidence,
        "stale_evidence": stale_evidence,
        "unavailable_evidence": unavailable_evidence,
        "proof_ladder_changes": proof_ladder_changes.clone(),
        "proof_ladder_change_counts": proof_ladder_change_counts,
        "candidate_layer_status": sidecar_statuses["candidate_layer_status"].clone(),
        "vector_layer_status": sidecar_statuses["vector_layer_status"].clone(),
        "path_evidence_status": sidecar_statuses["path_evidence_status"].clone(),
        "source_navigation_status": sidecar_statuses["source_navigation_status"].clone(),
        "routing_handle_status": sidecar_statuses["routing_handle_status"].clone(),
        "sidecar_statuses": sidecar_statuses,
        "stale_non_proof_reasons": stale_non_proof_reasons,
        "claimability_effect": claimability_effect,
        "severity_effect": {
            "severity_model_preserved": true,
            "hard_interrupt_eligibility_unchanged": true,
            "stale_sidecar_not_hard_interrupt": true,
            "text_evidence_change_not_broken_graph_behavior": true,
            "candidate_vector_source_navigation_non_proof": true,
        },
        "recovery_commands": recovery_commands,
    })
}

fn mcp_dirty_evidence_hard_interrupt_fields(fields: &Value) -> Value {
    json!({
        "dirty_evidence_summary": fields.get("dirty_evidence_summary").cloned().unwrap_or(Value::Null),
        "proof_ladder_change_counts": fields.get("proof_ladder_change_counts").cloned().unwrap_or(Value::Null),
        "sidecar_statuses": fields.get("sidecar_statuses").cloned().unwrap_or(Value::Null),
        "stale_non_proof_reasons": fields.get("stale_non_proof_reasons").cloned().unwrap_or_else(|| json!([])),
        "claimability_effect": fields.get("claimability_effect").cloned().unwrap_or_else(|| json!("not_applicable")),
        "severity_effect": fields.get("severity_effect").cloned().unwrap_or(Value::Null),
    })
}

fn mcp_append_dirty_evidence_expansion_handle(value: &mut Value, full_detail: bool) {
    if full_detail {
        return;
    }
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let handles = object
        .entry("expansion_handles".to_string())
        .or_insert_with(|| json!([]));
    let Some(handles) = handles.as_array_mut() else {
        return;
    };
    if !handles
        .iter()
        .any(|handle| handle.as_str() == Some("dirty_evidence:full"))
    {
        handles.push(json!("dirty_evidence:full"));
    }
}

fn mcp_layer_freshness(staged_availability: &Value, layer_name: &str) -> Value {
    let status = staged_availability
        .pointer(&format!("/layer_readiness/{layer_name}/status"))
        .and_then(Value::as_str)
        .or_else(|| {
            let key = match layer_name {
                "graph_db" => "graph_db_status",
                "candidate_spool" => "candidate_spool_status",
                "vector_runtime" => "vector_runtime_status",
                "vector_audit" => "vector_audit_status",
                _ => "",
            };
            staged_availability.get(key).and_then(Value::as_str)
        })
        .unwrap_or("unknown");
    json!(mcp_normalize_dirty_freshness_status(status))
}

fn mcp_candidate_query_index_freshness(staged_availability: &Value) -> Value {
    let status = staged_availability
        .pointer("/layer_readiness/candidate_spool/query_index_status")
        .and_then(Value::as_str)
        .or_else(|| {
            staged_availability
                .get("candidate_spool_query_index_status")
                .and_then(Value::as_str)
        })
        .unwrap_or_else(|| {
            staged_availability
                .pointer("/layer_readiness/candidate_spool/status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        });
    json!(mcp_normalize_dirty_freshness_status(status))
}

fn mcp_path_evidence_freshness(source: &Value) -> Value {
    let action = source
        .get("path_evidence_invalidated")
        .and_then(|value| value.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    json!(match action {
        "refreshed" | "status_checked" | "unchanged" | "rebuilt" => "fresh",
        "invalidated" => "stale",
        "not_applicable" => "not_applicable",
        "absent" => "missing",
        "error" => "inaccessible",
        _ => "unknown",
    })
}

fn mcp_source_navigation_freshness(
    staged_availability: &Value,
    candidate_layer_status: &Value,
) -> Value {
    if staged_availability
        .get("active_candidate_sources")
        .and_then(Value::as_array)
        .is_some_and(|sources| {
            sources
                .iter()
                .any(|source| source.as_str() == Some("candidate_spool"))
        })
    {
        return json!("fresh");
    }
    match candidate_layer_status.as_str().unwrap_or("unknown") {
        "stale" | "corrupt" | "inaccessible" | "permission_denied" => json!("stale"),
        "missing" | "not_applicable" => json!("not_applicable"),
        "fresh" => json!("fresh"),
        _ => json!("unknown"),
    }
}

fn mcp_routing_handle_freshness(source: &Value) -> Value {
    let action = source
        .get("routing_handles_invalidated_or_not_applicable")
        .or_else(|| source.get("routing_handles_invalidated"))
        .and_then(|value| value.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("not_applicable");
    json!(match action {
        "dirty_file_cleanup" | "invalidated" => "stale",
        "unchanged" | "status_checked" | "refreshed" => "fresh",
        "not_applicable" => "not_applicable",
        "error" => "inaccessible",
        _ => "not_applicable",
    })
}

fn mcp_normalize_dirty_freshness_status(status: &str) -> &'static str {
    match status {
        "ready" | "current" | "ok" | "superseded_by_graph_db" => "fresh",
        "building" | "rebuilding" => "rebuilding",
        "publishing" | "updating" => "publishing",
        "partial_ready" => "partial",
        "truncated_ready" | "bounded_ready" => "truncated",
        "stale" | "foreign" | "blocked_by_graph_db" => "stale",
        "missing" | "no_spool" | "no_index" | "query_index_missing" | "index_missing" => "missing",
        "corrupt" | "query_index_corrupt" | "sidecar_corrupt" => "corrupt",
        "permission_denied" | "read_only" => "permission_denied",
        "filesystem_inaccessible" | "sidecar_unavailable" | "sidecar_locked" | "blocked" => {
            "inaccessible"
        }
        "not_requested" | "not_applicable" => "not_applicable",
        "diagnostic_only" | "present_unvalidated" => "diagnostic_only",
        "disabled_budget_exceeded" => "inaccessible",
        _ => "unknown",
    }
}

fn mcp_output_claimability_effect(
    staged_availability: &Value,
    graph_proof_available: bool,
) -> &'static str {
    if graph_proof_available {
        "graph_proof_available"
    } else if staged_availability
        .get("candidate_only_available")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "non_proof_evidence_only"
    } else {
        "graph_proof_unavailable"
    }
}

fn mcp_proof_ladder_change_counts(proof_ladder_changes: &Value) -> Value {
    let mut counts = BTreeMap::from([
        ("text_evidence".to_string(), 0usize),
        ("symbol_evidence".to_string(), 0),
        ("candidate_evidence".to_string(), 0),
        ("source_navigation_evidence".to_string(), 0),
        ("graph_relation_proof".to_string(), 0),
        ("mutation_proof".to_string(), 0),
        ("flow_proof".to_string(), 0),
        ("unknown".to_string(), 0),
        ("unsupported".to_string(), 0),
        ("diagnostic_only".to_string(), 0),
    ]);
    let mut graph_proof_changed = 0usize;
    if let Some(object) = proof_ladder_changes.as_object() {
        for (key, value) in object {
            if !value.is_object() {
                continue;
            }
            let level = mcp_proof_ladder_level_for_key(key);
            *counts.entry(level.to_string()).or_insert(0) += 1;
            if value
                .get("graph_proof")
                .and_then(Value::as_bool)
                .unwrap_or(level == "graph_relation_proof")
            {
                graph_proof_changed += 1;
            }
        }
    }
    let total = counts.values().copied().sum::<usize>();
    let mut value = Map::new();
    for (key, count) in counts {
        value.insert(key, json!(count));
    }
    value.insert("total".to_string(), json!(total));
    value.insert(
        "graph_proof_changed_count".to_string(),
        json!(graph_proof_changed),
    );
    Value::Object(value)
}

fn mcp_proof_ladder_level_for_key(key: &str) -> &'static str {
    match key {
        "text_evidence" => "text_evidence",
        "symbol_evidence" | "graph_entities" | "source_spans" | "source_roles" => "symbol_evidence",
        "candidate_evidence"
        | "vector_evidence"
        | "runtime_vector_chunks"
        | "candidate_spool_packets"
        | "candidate_spool_query_index_rows"
        | "binary_candidate_records"
        | "nuance_token_records" => "candidate_evidence",
        "source_navigation"
        | "source_navigation_evidence"
        | "source_navigation_handles"
        | "routing_packet_handles"
        | "context_packet_handles" => "source_navigation_evidence",
        "graph_relation_proof" | "graph_edges" | "PathEvidence" | "path_evidence" => {
            "graph_relation_proof"
        }
        "mutation_proof" => "mutation_proof",
        "flow_proof" => "flow_proof",
        "unsupported" => "unsupported",
        "diagnostic_only" => "diagnostic_only",
        _ => "unknown",
    }
}

fn mcp_invalidated_evidence_statuses(source: &Value, proof_ladder_changes: &Value) -> Value {
    let mut surfaces = Vec::new();
    if source
        .get("text_evidence_changed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        surfaces.push(mcp_surface_status(
            "text_evidence",
            "text_evidence",
            "dirty",
            "non_proof_evidence_only",
            false,
            Some(
                "text evidence changed; source-text evidence refreshed, not broken graph behavior",
            ),
        ));
    }
    for (source_key, surface, level) in [
        (
            "path_evidence_invalidated",
            "PathEvidence",
            "graph_relation_proof",
        ),
        (
            "candidate_spool_invalidated_or_refreshed",
            "candidate_spool_packets",
            "candidate_evidence",
        ),
        (
            "candidate_query_index_invalidated_or_refreshed",
            "candidate_spool_query_index_rows",
            "candidate_evidence",
        ),
        (
            "vector_chunks_invalidated",
            "runtime_vector_chunks",
            "candidate_evidence",
        ),
        (
            "routing_handles_invalidated_or_not_applicable",
            "routing_packet_handles",
            "source_navigation_evidence",
        ),
    ] {
        let action = source
            .get(source_key)
            .and_then(|value| value.get("action"))
            .and_then(Value::as_str)
            .unwrap_or("unchanged");
        if matches!(action, "invalidated" | "dirty_file_cleanup" | "stale") {
            surfaces.push(mcp_surface_status(
                surface,
                level,
                "stale",
                mcp_surface_claimability_effect(level, false),
                level == "graph_relation_proof",
                None,
            ));
        }
    }
    if let Some(object) = proof_ladder_changes.as_object() {
        for (surface, change) in object {
            if !change.is_object() {
                continue;
            }
            if change
                .get("stale")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || change.get("action").and_then(Value::as_str) == Some("invalidated")
            {
                let level = mcp_proof_ladder_level_for_key(surface);
                surfaces.push(mcp_surface_status(
                    surface,
                    level,
                    "stale",
                    mcp_surface_claimability_effect(level, false),
                    level == "graph_relation_proof",
                    None,
                ));
            }
        }
    }
    Value::Array(surfaces)
}

fn mcp_refreshed_evidence_statuses(source: &Value, proof_ladder_changes: &Value) -> Value {
    let mut surfaces = Vec::new();
    if source
        .get("text_evidence_changed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        surfaces.push(mcp_surface_status(
            "text_evidence",
            "text_evidence",
            "fresh",
            "non_proof_evidence_only",
            false,
            Some("fresh text evidence can support source-text fallback but is not graph proof"),
        ));
    }
    for (source_key, surface, level) in [
        (
            "path_evidence_invalidated",
            "PathEvidence",
            "graph_relation_proof",
        ),
        (
            "candidate_spool_invalidated_or_refreshed",
            "candidate_spool_packets",
            "candidate_evidence",
        ),
        (
            "candidate_query_index_invalidated_or_refreshed",
            "candidate_spool_query_index_rows",
            "candidate_evidence",
        ),
        (
            "vector_chunks_invalidated",
            "runtime_vector_chunks",
            "candidate_evidence",
        ),
    ] {
        let action = source
            .get(source_key)
            .and_then(|value| value.get("action"))
            .and_then(Value::as_str)
            .unwrap_or("unchanged");
        if matches!(
            action,
            "refreshed" | "rebuilt" | "status_checked" | "unchanged"
        ) {
            surfaces.push(mcp_surface_status(
                surface,
                level,
                "fresh",
                mcp_surface_claimability_effect(level, true),
                level == "graph_relation_proof",
                None,
            ));
        }
    }
    if let Some(object) = proof_ladder_changes.as_object() {
        for (surface, change) in object {
            if !change.is_object() {
                continue;
            }
            if change
                .get("refreshed")
                .or_else(|| change.get("changed"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let level = mcp_proof_ladder_level_for_key(surface);
                surfaces.push(mcp_surface_status(
                    surface,
                    level,
                    "fresh",
                    mcp_surface_claimability_effect(level, true),
                    level == "graph_relation_proof",
                    None,
                ));
            }
        }
    }
    Value::Array(surfaces)
}

fn mcp_stale_evidence_statuses(staged_availability: &Value, proof_ladder_changes: &Value) -> Value {
    let mut surfaces = mcp_stale_candidate_layers(staged_availability)
        .into_iter()
        .map(|layer| {
            let name = layer
                .get("layer")
                .and_then(Value::as_str)
                .unwrap_or("candidate_layer");
            let level = if name == "vector_audit" {
                "diagnostic_only"
            } else if name == "candidate_spool" || name == "candidate_spool_query_index" {
                "candidate_evidence"
            } else {
                "source_navigation_evidence"
            };
            mcp_surface_status(
                name,
                level,
                "stale",
                "non_proof_evidence_only",
                false,
                Some("stale candidate/vector/source-navigation evidence cannot be used as graph proof"),
            )
        })
        .collect::<Vec<_>>();
    if !staged_availability
        .get("graph_proof_available")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        surfaces.push(mcp_surface_status(
            "graph_relation_proof",
            "graph_relation_proof",
            "stale",
            "graph_proof_unavailable",
            true,
            Some("unsafe or unavailable graph DB means graph/source proof is unavailable"),
        ));
    }
    if let Some(text_change) = proof_ladder_changes
        .get("text_evidence")
        .filter(|value| value.is_object())
    {
        if text_change
            .get("stale")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            surfaces.push(mcp_surface_status(
                "text_evidence",
                "text_evidence",
                "stale",
                "non_proof_evidence_only",
                false,
                Some(
                    "stale text evidence is stale source-text evidence, not broken graph behavior",
                ),
            ));
        }
    }
    Value::Array(surfaces)
}

fn mcp_unavailable_evidence_statuses(staged_availability: &Value) -> Value {
    let mut surfaces = Vec::new();
    for (layer_name, level) in [
        ("candidate_spool", "candidate_evidence"),
        ("vector_runtime", "candidate_evidence"),
        ("vector_audit", "diagnostic_only"),
    ] {
        let layer = staged_availability
            .pointer(&format!("/layer_readiness/{layer_name}"))
            .unwrap_or(&Value::Null);
        let status = layer
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let freshness = mcp_normalize_dirty_freshness_status(status);
        if matches!(
            freshness,
            "missing" | "corrupt" | "inaccessible" | "permission_denied"
        ) {
            surfaces.push(mcp_surface_status(
                layer_name,
                level,
                freshness,
                if layer_name == "vector_audit" {
                    "diagnostic_only"
                } else {
                    "sidecar_only"
                },
                false,
                Some("optional sidecar state is separate from graph DB claimability"),
            ));
        }
        if layer_name == "candidate_spool" {
            let query_status = layer
                .get("query_index_status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let query_freshness = mcp_normalize_dirty_freshness_status(query_status);
            if matches!(
                query_freshness,
                "missing" | "corrupt" | "inaccessible" | "permission_denied"
            ) {
                surfaces.push(mcp_surface_status(
                    "candidate_spool_query_index_rows",
                    "candidate_evidence",
                    query_freshness,
                    "sidecar_only",
                    false,
                    Some("candidate query index failure is sidecar state, not graph DB corruption"),
                ));
            }
        }
    }
    Value::Array(surfaces)
}

fn mcp_surface_claimability_effect(level: &str, fresh: bool) -> &'static str {
    match (level, fresh) {
        ("graph_relation_proof", true) => "graph_proof_available",
        ("graph_relation_proof", false) => "graph_proof_unavailable",
        ("symbol_evidence", true) => "supports_graph_proof_when_joined",
        ("mutation_proof" | "flow_proof", _) => "conditional_future_proof_only",
        ("diagnostic_only", _) => "diagnostic_only",
        ("unsupported", _) => "not_applicable",
        _ => "non_proof_evidence_only",
    }
}

fn mcp_surface_status(
    surface_name: &str,
    proof_ladder_level: &str,
    freshness_state: &str,
    claimability_effect: &str,
    graph_proof_possible: bool,
    stale_non_proof_reason: Option<&str>,
) -> Value {
    let mut value = json!({
        "surface_name": surface_name,
        "evidence_kind": proof_ladder_level,
        "proof_ladder_level": proof_ladder_level,
        "freshness_state": freshness_state,
        "claimability_effect": claimability_effect,
        "graph_proof_possible": graph_proof_possible,
        "graph_proof": graph_proof_possible && freshness_state == "fresh",
    });
    if let Some(reason) = stale_non_proof_reason {
        if let Some(object) = value.as_object_mut() {
            object.insert("stale_non_proof_reason".to_string(), json!(reason));
        }
    }
    value
}

fn mcp_stale_non_proof_reasons(stale_evidence: &Value, unavailable_evidence: &Value) -> Value {
    let mut reasons = BTreeSet::new();
    for item in stale_evidence
        .as_array()
        .into_iter()
        .flatten()
        .chain(unavailable_evidence.as_array().into_iter().flatten())
    {
        if item
            .get("graph_proof_possible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(reason) = item.get("stale_non_proof_reason").and_then(Value::as_str) {
            reasons.insert(reason.to_string());
        } else if let Some(surface) = item.get("surface_name").and_then(Value::as_str) {
            reasons.insert(format!(
                "{surface} is non-proof evidence when stale or unavailable"
            ));
        }
    }
    Value::Array(reasons.into_iter().map(Value::String).collect())
}

fn mcp_any_sidecar_corrupt_or_unavailable(sidecar_statuses: &Value) -> bool {
    [
        "candidate_layer_status",
        "vector_layer_status",
        "candidate_spool_query_index_status",
    ]
    .iter()
    .any(|key| {
        matches!(
            sidecar_statuses.get(*key).and_then(Value::as_str),
            Some("corrupt" | "inaccessible" | "permission_denied")
        )
    })
}

fn mcp_count_fresh_dirty_statuses(sidecar_statuses: &Value) -> usize {
    [
        "graph_db_status",
        "candidate_layer_status",
        "vector_layer_status",
        "path_evidence_status",
    ]
    .iter()
    .filter(|key| sidecar_statuses.get(**key).and_then(Value::as_str) == Some("fresh"))
    .count()
}

fn mcp_count_status(sidecar_statuses: &Value, status: &str) -> usize {
    sidecar_statuses
        .as_object()
        .into_iter()
        .flat_map(|object| object.values())
        .filter(|value| value.as_str() == Some(status))
        .count()
}

fn mcp_profile_identity_json(repo_root: &Path, db_path: &Path) -> Value {
    let env_profile_name = std::env::var("CODEGRAPH_AGENT_USE_PROFILE").ok();
    let profile_name = if let Some(profile_name) = env_profile_name {
        profile_name
    } else if mcp_db_looks_like_agent_use_profile(db_path) {
        PRODUCTION_AGENT_USE_PROFILE_NAME.to_string()
    } else {
        "unknown".to_string()
    };
    let profile_root = std::env::var_os("CODEGRAPH_AGENT_USE_PROFILE_ROOT")
        .map(PathBuf::from)
        .or_else(|| db_path.parent().map(Path::to_path_buf));
    let (label_from_root, hash_from_root) = profile_root
        .as_deref()
        .and_then(mcp_parse_profile_root_identity)
        .unwrap_or((None, None));
    let env_hash = std::env::var("CODEGRAPH_AGENT_USE_REPO_IDENTITY_HASH").ok();
    let repo_identity_hash = env_hash.clone().or(hash_from_root);
    let repo_identity_source = if env_hash.is_some() {
        "env"
    } else if repo_identity_hash.is_some() {
        "profile_root"
    } else {
        "unknown"
    };
    json!({
        "profile_name": profile_name,
        "active_profile_name": profile_name,
        "repo_root": path_string(repo_root),
        "db_path": path_string(db_path),
        "profile_root": profile_root.as_ref().map(|path| path_string(path)),
        "repo_identity_label": label_from_root.unwrap_or_else(|| "unknown".to_string()),
        "repo_identity_hash": repo_identity_hash.unwrap_or_else(|| "unknown".to_string()),
        "repo_identity_source": repo_identity_source,
        "safe_read_only_startup": true,
        "auto_index_on_startup": false,
        "startup_auto_index": false,
        "dot_codegraph_fallback": false,
        "public_claim": false,
    })
}

fn mcp_parse_profile_root_identity(
    profile_root: &Path,
) -> Option<(Option<String>, Option<String>)> {
    let name = profile_root.file_name()?.to_str()?;
    let (label, hash) = name.rsplit_once('-')?;
    if hash.len() == 32 && hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some((Some(label.to_string()), Some(hash.to_string())))
    } else {
        Some((Some(name.to_string()), None))
    }
}

fn mcp_agent_use_publish_state_json(db_path: &Path) -> Value {
    let path = mcp_agent_use_profile_sibling_path(db_path, MCP_AGENT_USE_PUBLISH_STATE_FILE_NAME);
    if !path.exists() {
        return json!({
            "status": "absent",
            "path": path_string(&path),
            "active": false,
            "updating": false,
            "publishing": false,
            "claimability_effect": "none",
            "temp_db_claimability": "never_claimable",
        });
    }
    let parsed = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let status = parsed
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("publishing");
    json!({
        "status": status,
        "path": path_string(&path),
        "active": true,
        "updating": status == "updating",
        "publishing": true,
        "state_readable": parsed.is_some(),
        "state": parsed.unwrap_or(Value::Null),
        "claimability_effect": "old valid DB may remain readable; temp DB is never claimable",
        "temp_db_claimability": "never_claimable",
    })
}

fn mcp_agent_use_last_delta_state_json(db_path: &Path) -> Value {
    let path = mcp_agent_use_profile_sibling_path(db_path, MCP_AGENT_USE_DELTA_STATE_FILE_NAME);
    if !path.exists() {
        return json!({
            "status": "absent",
            "path": path_string(&path),
            "state_readable": false,
            "last_delta_update_summary": Value::Null,
            "public_claim": false,
        });
    }
    match fs::read_to_string(&path)
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|error| error.to_string()))
    {
        Ok(mut value) => {
            if let Some(object) = value.as_object_mut() {
                object.insert("path".to_string(), json!(path_string(&path)));
                object.insert("state_readable".to_string(), json!(true));
            }
            value
        }
        Err(error) => json!({
            "status": "error",
            "path": path_string(&path),
            "state_readable": false,
            "error": error,
            "last_delta_update_summary": Value::Null,
            "public_claim": false,
        }),
    }
}

fn mcp_agent_use_profile_sibling_path(db_path: &Path, file_name: &str) -> PathBuf {
    db_path
        .parent()
        .map(|parent| parent.join(file_name))
        .unwrap_or_else(|| PathBuf::from(file_name))
}

fn mcp_db_looks_like_agent_use_profile(db_path: &Path) -> bool {
    db_path.file_name().and_then(|value| value.to_str()) == Some(MCP_AGENT_USE_PROFILE_DB_FILE_NAME)
}

fn mcp_agent_use_recovery_commands(repo_root: &Path) -> Vec<String> {
    let repo = path_string(repo_root);
    vec![
        format!("codegraph-mcp agent-use status --repo \"{repo}\" --json"),
        format!("codegraph-mcp agent-use index --repo \"{repo}\" --json"),
        format!("codegraph-mcp agent-use watch --repo \"{repo}\" --once --changed <path> --json"),
        format!(
            "codegraph-mcp agent-use context-pack --repo \"{repo}\" --task \"<task>\" --agent-json"
        ),
        format!("codegraph-mcp agent-use mcp-config --repo \"{repo}\" --json"),
    ]
}

fn mcp_stale_candidate_layers(staged_availability: &Value) -> Vec<Value> {
    let mut layers = Vec::new();
    for layer_name in ["candidate_spool", "vector_runtime", "vector_audit"] {
        let layer = staged_availability
            .pointer(&format!("/layer_readiness/{layer_name}"))
            .unwrap_or(&Value::Null);
        let status = layer
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if mcp_candidate_layer_status_is_stale(status) {
            layers.push(json!({
                "layer": layer_name,
                "status": status,
                "path": layer.get("path").cloned().unwrap_or(Value::Null),
                "reason": layer.get("reason").cloned().unwrap_or(Value::Null),
                "graph_proof": false,
            }));
        }
        if layer_name == "candidate_spool" {
            let query_status = layer
                .get("query_index_status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if mcp_candidate_layer_status_is_stale(query_status) {
                layers.push(json!({
                    "layer": "candidate_spool_query_index",
                    "status": query_status,
                    "path": layer.get("query_index_path").cloned().unwrap_or(Value::Null),
                    "reason": layer.get("reason").cloned().unwrap_or(Value::Null),
                    "graph_proof": false,
                }));
            }
        }
    }
    layers
}

fn mcp_candidate_layer_status_is_stale(status: &str) -> bool {
    matches!(
        status,
        "stale"
            | "corrupt"
            | "query_index_corrupt"
            | "sidecar_corrupt"
            | "permission_denied"
            | "read_only"
            | "filesystem_inaccessible"
            | "sidecar_unavailable"
            | "sidecar_locked"
            | "blocked_by_graph_db"
            | "disabled_budget_exceeded"
    )
}

#[cfg(test)]
mod mvp3_7_sidecar_status_tests {
    use super::*;

    fn mcp_dirty_output_graph_layer(status: &str, ready: bool) -> Value {
        json!({
            "layer": "graph_db",
            "status": status,
            "ready": ready,
            "claimable": ready,
            "graph_proof_available": ready,
            "diagnostic_only": !ready,
        })
    }

    fn mcp_dirty_output_candidate_layer(status: &str, query_index_status: &str) -> Value {
        json!({
            "layer": "candidate_spool",
            "status": status,
            "ready": status == "ready" && query_index_status == "ready",
            "candidate_only": true,
            "graph_proof": false,
            "path": "candidate.jsonl",
            "query_index_status": query_index_status,
            "query_index_path": "candidate.sqlite",
            "reason": "test candidate sidecar status",
        })
    }

    fn mcp_dirty_output_vector_layer(status: &str) -> Value {
        json!({
            "layer": "vector_runtime",
            "status": status,
            "ready": status == "ready",
            "candidate_only": true,
            "graph_proof": false,
            "path": "vector-runtime.json",
            "reason": "test vector sidecar status",
        })
    }

    fn mcp_dirty_output_audit_layer(status: &str) -> Value {
        json!({
            "layer": "vector_audit",
            "status": status,
            "ready": matches!(status, "ready" | "diagnostic_only" | "stale"),
            "diagnostic_only": true,
            "runtime_dependency": false,
            "path": "vector-audit.json",
        })
    }

    fn mcp_dirty_output_staged(candidate_status: &str, query_index_status: &str) -> Value {
        mcp_staged_availability_from_layers(
            mcp_dirty_output_graph_layer("ready", true),
            mcp_dirty_output_candidate_layer(candidate_status, query_index_status),
            mcp_dirty_output_vector_layer("stale"),
            mcp_dirty_output_audit_layer("missing"),
        )
    }

    fn mcp_dirty_output_packet() -> Value {
        json!({
            "staged_availability": mcp_dirty_output_staged("query_index_corrupt", "corrupt"),
            "text_evidence_changed": true,
            "path_evidence_invalidated": {"action": "refreshed", "graph_proof": false},
            "candidate_spool_invalidated_or_refreshed": {"action": "error", "status": "query_index_corrupt", "graph_proof": false},
            "candidate_query_index_invalidated_or_refreshed": {"action": "error", "status": "corrupt", "graph_proof": false},
            "vector_chunks_invalidated": {"action": "invalidated", "status": "stale", "graph_proof": false},
            "routing_handles_invalidated_or_not_applicable": {"action": "dirty_file_cleanup", "graph_proof": false},
            "proof_ladder_changes": {
                "text_evidence": {"changed": true, "graph_proof": false},
                "candidate_evidence": {"changed": true, "graph_proof": false},
                "vector_evidence": {"changed": true, "graph_proof": false},
                "source_navigation": {"changed": true, "graph_proof": false}
            },
            "recovery_commands": ["codegraph-mcp agent-use index --repo <repo> --json"],
        })
    }

    #[test]
    fn mcp_sidecar_stale_status_set_matches_dirty_evidence_contract() {
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
            assert!(mcp_candidate_layer_status_is_stale(status), "{status}");
        }
        for status in ["ready", "missing", "no_spool", "not_applicable"] {
            assert!(!mcp_candidate_layer_status_is_stale(status), "{status}");
        }
    }

    #[test]
    fn mcp_validate_edit_parity() {
        let mut packet = mcp_dirty_output_packet();
        packet["validation_packet"] = json!({
            "proof_ladder_changes": packet["proof_ladder_changes"].clone(),
            "hard_interrupt_available": false,
        });
        mcp_attach_dirty_evidence_output_fields(&mut packet, "codegraph.validate_edit", false);
        assert!(packet["dirty_evidence_summary"].is_object(), "{packet:?}");
        assert_eq!(
            packet["validation_packet"]["dirty_evidence_summary"].is_object(),
            true
        );
        assert_eq!(packet["candidate_layer_status"].as_str(), Some("corrupt"));
        assert_eq!(
            packet["proof_ladder_change_counts"]["candidate_evidence"].as_u64(),
            Some(2)
        );
        assert_eq!(
            packet["severity_effect"]["stale_sidecar_not_hard_interrupt"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn mcp_context_pack_parity() {
        let mut packet = mcp_dirty_output_packet();
        mcp_attach_dirty_evidence_output_fields(&mut packet, "codegraph.context_pack", false);
        assert_eq!(
            packet["dirty_evidence_summary"]["surface"].as_str(),
            Some("codegraph.context_pack")
        );
        assert_eq!(
            packet["claimability_effect"].as_str(),
            Some("graph_proof_available")
        );
        assert!(packet["stale_non_proof_reasons"]
            .as_array()
            .expect("reasons")
            .iter()
            .any(|reason| reason
                .as_str()
                .is_some_and(|text| text.contains("cannot be used as graph proof"))));
    }
}

fn mcp_merge_json_object(target: &mut Value, fields: Value) {
    let Some(target_object) = target.as_object_mut() else {
        return;
    };
    if let Some(fields_object) = fields.as_object() {
        for (key, value) in fields_object {
            if matches!(key.as_str(), "warnings" | "blockers") {
                if let (Some(existing), Some(incoming)) = (
                    target_object.get_mut(key).and_then(Value::as_array_mut),
                    value.as_array(),
                ) {
                    let mut seen = existing
                        .iter()
                        .map(|item| item.to_string())
                        .collect::<BTreeSet<_>>();
                    for item in incoming {
                        if seen.insert(item.to_string()) {
                            existing.push(item.clone());
                        }
                    }
                    continue;
                }
            }
            target_object.insert(key.clone(), value.clone());
        }
    }
}

fn compact_mcp_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut output = value.chars().take(max_chars).collect::<String>();
    output.push_str("...");
    output
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
        "db_problem": "db_missing",
        "db_problem_kind": "db_missing",
        "path_access_status": "db_missing",
        "db_path_outside_workspace": context.db_path_outside_workspace,
        "outside_workspace_note": context.outside_workspace_note.clone(),
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

fn db_path_present_or_inaccessible(db_path: &Path) -> bool {
    match fs::metadata(db_path) {
        Ok(_) => true,
        Err(error) => error.kind() != io::ErrorKind::NotFound,
    }
}

fn mcp_db_path_outside_workspace(db_path: &Path, workspace_root: &Path) -> bool {
    let db_path = if db_path.is_absolute() {
        db_path.to_path_buf()
    } else {
        workspace_root.join(db_path)
    };
    let db_path = canonicalize_existing_prefix(&db_path);
    let workspace_root = canonicalize_existing_prefix(workspace_root);
    !mcp_path_starts_with_workspace(&db_path, &workspace_root)
}

fn canonicalize_existing_prefix(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }
    let mut current = path.to_path_buf();
    let mut suffix = Vec::<std::ffi::OsString>::new();
    while let Some(file_name) = current.file_name().map(|value| value.to_os_string()) {
        suffix.push(file_name);
        if !current.pop() {
            return path.to_path_buf();
        }
        if let Ok(mut canonical) = fs::canonicalize(&current) {
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return canonical;
        }
    }
    path.to_path_buf()
}

#[cfg(windows)]
fn mcp_path_starts_with_workspace(path: &Path, workspace_root: &Path) -> bool {
    let path = path.display().to_string().to_ascii_lowercase();
    let workspace_root = workspace_root.display().to_string().to_ascii_lowercase();
    path == workspace_root
        || path
            .strip_prefix(&workspace_root)
            .is_some_and(|rest| rest.starts_with('\\') || rest.starts_with('/'))
}

#[cfg(not(windows))]
fn mcp_path_starts_with_workspace(path: &Path, workspace_root: &Path) -> bool {
    path.starts_with(workspace_root)
}

#[derive(Debug, Clone, Copy, Default)]
struct McpContextPackDocumentMetrics {
    exact_symbol_lookups: usize,
    fts_lookups: usize,
    documents_returned: usize,
    entities_hydrated: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct McpContextPackSourceMetrics {
    files_loaded: usize,
    bytes_loaded: usize,
    budget_hit: bool,
}

fn retrieval_documents_for_context_pack(
    store: &SqliteGraphStore,
    task: &str,
    seeds: &[String],
) -> Result<(Vec<RetrievalDocument>, McpContextPackDocumentMetrics), ToolCallError> {
    let mut metrics = McpContextPackDocumentMetrics::default();
    let mut documents = Vec::new();
    let mut seen = BTreeSet::new();

    for seed in seeds {
        metrics.exact_symbol_lookups += 1;
        for entity in store
            .find_entities_by_exact_symbol(seed)
            .map_err(mcp_store_error)?
        {
            if documents.len() >= MCP_CONTEXT_PACK_DOCUMENT_LIMIT {
                break;
            }
            if seen.insert(entity.id.clone()) {
                metrics.entities_hydrated += 1;
                documents.push(retrieval_document_from_entity(entity, 1.0));
            }
        }
        if documents.len() >= MCP_CONTEXT_PACK_DOCUMENT_LIMIT {
            break;
        }
    }

    if documents.len() < MCP_CONTEXT_PACK_DOCUMENT_LIMIT {
        metrics.fts_lookups += 1;
        for hit in store
            .search_text(
                task,
                MCP_CONTEXT_PACK_DOCUMENT_LIMIT.saturating_sub(documents.len()),
            )
            .map_err(mcp_store_error)?
        {
            if documents.len() >= MCP_CONTEXT_PACK_DOCUMENT_LIMIT {
                break;
            }
            match hit.kind {
                TextSearchKind::Entity => {
                    if seen.contains(&hit.id) {
                        continue;
                    }
                    if let Some(entity) = store.get_entity(&hit.id).map_err(mcp_store_error)? {
                        if seen.insert(entity.id.clone()) {
                            metrics.entities_hydrated += 1;
                            documents.push(retrieval_document_from_entity(entity, 0.75));
                        }
                    }
                }
                TextSearchKind::File | TextSearchKind::Snippet => {
                    let document_id = format!("stage0://{}:{}", hit.kind.as_str(), hit.id);
                    if seen.insert(document_id.clone()) {
                        let mut document = RetrievalDocument::new(
                            document_id,
                            format!("{} {}", hit.title, hit.text),
                        )
                        .stage0_score(0.5);
                        document
                            .metadata
                            .insert("repo_relative_path".to_string(), hit.repo_relative_path);
                        document
                            .metadata
                            .insert("kind".to_string(), hit.kind.as_str().to_string());
                        documents.push(document);
                    }
                }
            }
        }
    }

    metrics.documents_returned = documents.len();
    Ok((documents, metrics))
}

fn retrieval_document_from_entity(entity: Entity, score: f64) -> RetrievalDocument {
    let text = format!(
        "{} {} {} {} {}",
        entity.kind,
        entity.name,
        entity.qualified_name,
        entity.repo_relative_path,
        entity.created_from
    );
    let mut document = RetrievalDocument::new(entity.id, text).stage0_score(score);
    document
        .metadata
        .insert("repo_relative_path".to_string(), entity.repo_relative_path);
    document
        .metadata
        .insert("kind".to_string(), entity.kind.to_string());
    document
}

fn retrieval_document_paths(documents: &[RetrievalDocument]) -> BTreeSet<String> {
    documents
        .iter()
        .filter_map(|document| document.metadata.get("repo_relative_path").cloned())
        .collect()
}

fn mcp_context_pack_read_path_metrics_json(
    document_metrics: &McpContextPackDocumentMetrics,
    source_metrics: &McpContextPackSourceMetrics,
    edge_count: usize,
) -> Value {
    json!({
        "schema_version": 1,
        "surface": "mcp codegraph.context_pack",
        "lookup_strategy": "bounded_exact_symbol_and_stage0_fts_candidates",
        "indexed_lookup_count": document_metrics.exact_symbol_lookups + document_metrics.fts_lookups,
        "fts_lookup_count": document_metrics.fts_lookups,
        "path_dictionary_lookup_count": 0,
        "symbol_dictionary_lookup_count": document_metrics.exact_symbol_lookups,
        "full_scan_count": 0,
        "bounded_table_scan_count": 1,
        "entity_edge_million_row_load": false,
        "source_file_load_count": source_metrics.files_loaded,
        "entities_hydrated": document_metrics.entities_hydrated,
        "edges_hydrated": edge_count,
        "source_bytes_loaded": source_metrics.bytes_loaded,
        "snippets_loaded": 0,
        "disk_fallback_used": false,
        "disk_fallback_files": 0,
        "debug_broad_scan": false,
        "diagnostic_only": false,
        "limits_apply_before_hydration": true,
        "budget_hit": source_metrics.budget_hit
            || document_metrics.documents_returned >= MCP_CONTEXT_PACK_DOCUMENT_LIMIT
            || edge_count >= MCP_CONTEXT_PACK_EDGE_LIMIT,
        "limits": {
            "max_files_inspected": MCP_CONTEXT_PACK_SOURCE_FILE_LIMIT,
            "max_entities_hydrated": MCP_CONTEXT_PACK_DOCUMENT_LIMIT,
            "max_edges_visited": MCP_CONTEXT_PACK_EDGE_LIMIT,
            "max_source_bytes_loaded": MCP_CONTEXT_PACK_SOURCE_BYTE_LIMIT,
            "max_snippets_loaded": MCP_CONTEXT_PACK_DEFAULT_LIMIT,
            "max_disk_fallback_files": 0,
            "timeout_ms": Value::Null,
        }
    })
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
    let is_text_evidence = matches!(hit.kind, TextSearchKind::File | TextSearchKind::Snippet);
    let mut value = json!({
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
    });
    if is_text_evidence {
        if let Some(object) = value.as_object_mut() {
            insert_text_evidence_labels(object);
        }
    }
    value
}

fn insert_text_evidence_labels(object: &mut serde_json::Map<String, Value>) {
    object.insert("evidence_kind".to_string(), json!("text_evidence"));
    object.insert("evidence_role".to_string(), json!("text_evidence"));
    object.insert("proof_status".to_string(), json!("not_graph_proof"));
    object.insert("graph_proof".to_string(), json!(false));
    object.insert("graph_relation_claims".to_string(), json!([]));
    object.insert(
        "claimability".to_string(),
        json!({
            "claimable": true,
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
        }),
    );
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
            let mut hit = json!({
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
            });
            if let Some(object) = hit.as_object_mut() {
                insert_text_evidence_labels(object);
            }
            hits.push(hit);
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
    let message = error.to_string();
    if let Some(problem) = classify_sqlite_access_problem(&message) {
        ToolCallError::new(problem.db_problem_kind, message)
    } else {
        ToolCallError::new("store_error", message)
    }
}

fn mcp_unsafe_preflight_error(db_path: &Path, preflight: &DbLifecyclePreflight) -> ToolCallError {
    if let Some(mismatch) = preflight.scope_mismatch.as_ref() {
        return ToolCallError::new(
            "scope_mismatch",
            mcp_scope_mismatch_message(db_path, mismatch),
        );
    }
    let outside_note = preflight
        .outside_workspace_note
        .as_deref()
        .map(|note| format!("; {note}"))
        .unwrap_or_default();
    let problem_kind = mcp_db_problem_kind(preflight);
    ToolCallError::new(
        problem_kind,
        format!(
            "CodeGraph DB is not safe to read at {}: kind={}; {}{}; call codegraph.index_repo to rebuild",
            db_path.display(),
            problem_kind,
            preflight.blockers.join("; "),
            outside_note
        ),
    )
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

fn mcp_db_problem_kind(preflight: &DbLifecyclePreflight) -> &str {
    if matches!(
        preflight.db_problem_kind.as_deref(),
        Some(
            "db_missing"
                | "filesystem_inaccessible"
                | "permission_denied"
                | "db_locked"
                | "sqlite_corrupt"
        )
    ) {
        preflight
            .db_problem_kind
            .as_deref()
            .unwrap_or("passport_invalid")
    } else if preflight.repo_root_status == "mismatched" {
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
    } else if let Some(kind) = preflight.db_problem_kind.as_deref() {
        kind
    } else {
        "passport_invalid"
    }
}

fn mcp_passport_summary_json(preflight: &DbLifecyclePreflight) -> Value {
    let passport = preflight.db_health.passport.as_ref();
    json!({
        "passport_status": preflight.db_health.passport_status.clone(),
        "db_problem_kind": preflight.db_problem_kind.clone(),
        "path_access_status": preflight.path_access_status.clone(),
        "path_access_error": preflight.path_access_error.clone(),
        "schema_version": preflight.db_health.schema_version,
        "passport_version": passport.map(|value| value.passport_version),
        "codegraph_schema_version": passport.map(|value| value.codegraph_schema_version),
        "storage_mode": passport.map(|value| value.storage_mode.clone()),
        "canonical_repo_root": passport.map(|value| value.canonical_repo_root.clone()),
        "repo_head": passport.and_then(|value| value.repo_head.clone()),
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
        "db_problem_kind": preflight.db_problem_kind.clone(),
        "path_access_status": preflight.path_access_status.clone(),
        "path_access_error": preflight.path_access_error.clone(),
        "safe": preflight.safe,
        "claimable": preflight.safe,
        "sqlite_sidecars": preflight.db_health.sqlite_sidecars.clone(),
        "sidecar_status": preflight.db_health.sidecar_status.clone(),
        "orphan_sidecars": preflight.db_health.orphan_sidecars.clone(),
        "orphan_sidecars_deprecated": true,
        "blockers": preflight.blockers.clone(),
        "warnings": preflight.warnings.clone(),
        "exact_db_path_checked": preflight.exact_db_path_checked.clone(),
        "repo_root_expected": preflight.repo_root_expected.clone(),
        "db_path_outside_workspace": preflight.db_path_outside_workspace,
        "outside_workspace_note": preflight.outside_workspace_note.clone(),
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

fn load_sources_for_paths(
    repo_root: &Path,
    paths: &BTreeSet<String>,
) -> Result<(BTreeMap<String, String>, McpContextPackSourceMetrics), McpServerError> {
    let mut sources = BTreeMap::new();
    let mut metrics = McpContextPackSourceMetrics::default();
    for repo_relative_path in paths {
        if sources.len() >= MCP_CONTEXT_PACK_SOURCE_FILE_LIMIT {
            metrics.budget_hit = true;
            break;
        }
        let path = repo_root.join(repo_relative_path);
        if !path.exists() {
            continue;
        }
        let source = fs::read_to_string(path)?;
        if metrics.bytes_loaded.saturating_add(source.len()) > MCP_CONTEXT_PACK_SOURCE_BYTE_LIMIT {
            metrics.budget_hit = true;
            continue;
        }
        metrics.bytes_loaded += source.len();
        sources.insert(repo_relative_path.clone(), source);
    }
    metrics.files_loaded = sources.len();
    Ok((sources, metrics))
}

fn mcp_sqlite_sidecars_status_for_path(db_path: &Path) -> Value {
    let wal_path = mcp_sqlite_sidecar_path(db_path, "wal");
    let shm_path = mcp_sqlite_sidecar_path(db_path, "shm");
    let wal_bytes = metadata_len(&wal_path);
    let shm_bytes = metadata_len(&shm_path);
    let wal_exists = wal_bytes > 0 || wal_path.exists();
    let shm_exists = shm_bytes > 0 || shm_path.exists();
    let main_exists = db_path.exists();
    let mut sidecars = Vec::new();
    if wal_exists {
        sidecars.push(path_string(&wal_path));
    }
    if shm_exists {
        sidecars.push(path_string(&shm_path));
    }
    let sidecar_status = if !main_exists && !sidecars.is_empty() {
        "orphan_without_main_db"
    } else {
        "normal"
    };
    let orphan_sidecars = if sidecar_status == "orphan_without_main_db" {
        sidecars.clone()
    } else {
        Vec::new()
    };
    json!({
        "status": sidecar_status,
        "sidecar_status": sidecar_status,
        "main_db_exists": main_exists,
        "wal_path": path_string(&wal_path),
        "wal_exists": wal_exists,
        "wal_bytes": wal_bytes,
        "shm_path": path_string(&shm_path),
        "shm_exists": shm_exists,
        "shm_bytes": shm_bytes,
        "sqlite_sidecars": sidecars,
        "orphan_sidecars": orphan_sidecars,
        "orphan_sidecars_deprecated": true,
    })
}

fn mcp_sqlite_sidecars_status_from_health(db_path: &Path, health: &DbPreflightReport) -> Value {
    let mut status = mcp_sqlite_sidecars_status_for_path(db_path);
    if let Some(object) = status.as_object_mut() {
        object.insert(
            "status".to_string(),
            Value::String(health.sidecar_status.clone()),
        );
        object.insert(
            "sidecar_status".to_string(),
            Value::String(health.sidecar_status.clone()),
        );
        object.insert(
            "sqlite_sidecars".to_string(),
            json!(health.sqlite_sidecars.clone()),
        );
        object.insert(
            "orphan_sidecars".to_string(),
            json!(health.orphan_sidecars.clone()),
        );
        object.insert("orphan_sidecars_deprecated".to_string(), json!(true));
    }
    status
}

fn mcp_sqlite_sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}-{}", db_path.display(), suffix))
}

fn metadata_len(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::Mutex;
    use std::time::Duration;

    static FIXTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

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

    fn write_mcp_candidate_spool_fixture(repo: &Path) -> PathBuf {
        let spool = repo.join("candidate-spool.jsonl");
        let source_path = repo.join("src").join("auth.ts");
        let source = fs::read_to_string(&source_path).expect("read mcp fixture source");
        let source_hash = codegraph_parser::content_hash(&source);
        let manifest = json!({
            "metadata": {
                "metadata_version": "candidate_spool_v1",
                "artifact_kind": "candidate_spool",
                "artifact_format": "jsonl",
                "repo_root": path_string(repo),
                "candidate_spool_status": "partial_ready",
                "lifecycle": "partial_spool",
                "incomplete": true,
                "candidate_only": true,
                "graph_proof": false,
                "claimable_for_graph": false
            }
        });
        let chunk = json!({
            "chunk_id": "candidate-spool:mcp:login",
            "chunk_kind": "signature",
            "source_kind": "graph_entity",
            "path": "src/auth.ts",
            "entity_id": "entity:login",
            "source_span": {
                "repo_relative_path": "src/auth.ts",
                "start_line": 9,
                "start_col": 1,
                "end_line": 12,
                "end_col": 1
            },
            "proof_status": "candidate_only",
            "graph_proof": false,
            "claimable_for_graph": false,
            "requires_graph_verification": true,
            "text": "function login in src/auth.ts handles email",
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
        .expect("write MCP candidate spool");
        spool
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

    #[test]
    fn mcp_default_config_uses_explicit_env_db_path_for_agent_profile() {
        let _guard = ENV_TEST_LOCK.lock().expect("env lock");
        let repo = fixture_repo();
        let profile_root = repo.join("external-profile");
        fs::create_dir_all(&profile_root).expect("profile root");
        let db_path = profile_root.join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let old_cwd = std::env::current_dir().expect("cwd");
        let old_db = std::env::var_os("CODEGRAPH_DB_PATH");
        std::env::set_current_dir(&repo).expect("set cwd");
        std::env::set_var("CODEGRAPH_DB_PATH", &db_path);

        let config = McpServerConfig::default();

        assert_eq!(config.repo_root, repo);
        assert_eq!(config.db_path, db_path);
        if let Some(old_db) = old_db {
            std::env::set_var("CODEGRAPH_DB_PATH", old_db);
        } else {
            std::env::remove_var("CODEGRAPH_DB_PATH");
        }
        std::env::set_current_dir(old_cwd).expect("restore cwd");
        fs::remove_dir_all(config.repo_root).expect("cleanup");
    }

    #[test]
    fn mcp_status_and_context_expose_agent_profile_identity() {
        let _guard = ENV_TEST_LOCK.lock().expect("env lock");
        let repo = fixture_repo();
        let profile_root = repo
            .parent()
            .expect("repo parent")
            .join("app-0123456789abcdef0123456789abcdef");
        fs::create_dir_all(&profile_root).expect("profile root");
        let db_path = profile_root.join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let old_profile = std::env::var_os("CODEGRAPH_AGENT_USE_PROFILE");
        let old_hash = std::env::var_os("CODEGRAPH_AGENT_USE_REPO_IDENTITY_HASH");
        let old_root = std::env::var_os("CODEGRAPH_AGENT_USE_PROFILE_ROOT");
        std::env::set_var(
            "CODEGRAPH_AGENT_USE_PROFILE",
            PRODUCTION_AGENT_USE_PROFILE_NAME,
        );
        std::env::set_var(
            "CODEGRAPH_AGENT_USE_REPO_IDENTITY_HASH",
            "0123456789abcdef0123456789abcdef",
        );
        std::env::set_var("CODEGRAPH_AGENT_USE_PROFILE_ROOT", &profile_root);

        let server = McpServer::new(McpServerConfig::for_repo(&repo).with_db_path(&db_path));
        ok(server.call_tool("codegraph.index_repo", &json!({"repo": path_string(&repo)})));
        let status = ok(server.call_tool("codegraph.status", &json!({"repo": path_string(&repo)})));
        let context = ok(server.call_tool(
            "codegraph.context_pack",
            &json!({"repo": path_string(&repo), "task": "Change login", "limit": 1}),
        ));

        for value in [&status, &context] {
            assert_eq!(
                value["profile_name"].as_str(),
                Some(PRODUCTION_AGENT_USE_PROFILE_NAME)
            );
            assert_eq!(
                value["active_profile_name"].as_str(),
                Some(PRODUCTION_AGENT_USE_PROFILE_NAME)
            );
            assert_eq!(value["repo_identity_label"].as_str(), Some("app"));
            assert_eq!(
                value["repo_identity_hash"].as_str(),
                Some("0123456789abcdef0123456789abcdef")
            );
            assert_eq!(
                value["profile_identity"]["repo_identity_hash"].as_str(),
                Some("0123456789abcdef0123456789abcdef")
            );
            assert_eq!(
                value["profile_identity"]["auto_index_on_startup"].as_bool(),
                Some(false)
            );
            assert_eq!(
                value["rtds_freshness"]["profile_identity"]["repo_identity_hash"].as_str(),
                Some("0123456789abcdef0123456789abcdef")
            );
            assert_eq!(value["graph_freshness"].as_str(), Some("current"));
        }
        assert_eq!(status["safe_to_query"].as_bool(), Some(true));
        assert_eq!(context["graph_proof"].as_bool().is_some(), true);

        if let Some(old_profile) = old_profile {
            std::env::set_var("CODEGRAPH_AGENT_USE_PROFILE", old_profile);
        } else {
            std::env::remove_var("CODEGRAPH_AGENT_USE_PROFILE");
        }
        if let Some(old_hash) = old_hash {
            std::env::set_var("CODEGRAPH_AGENT_USE_REPO_IDENTITY_HASH", old_hash);
        } else {
            std::env::remove_var("CODEGRAPH_AGENT_USE_REPO_IDENTITY_HASH");
        }
        if let Some(old_root) = old_root {
            std::env::set_var("CODEGRAPH_AGENT_USE_PROFILE_ROOT", old_root);
        } else {
            std::env::remove_var("CODEGRAPH_AGENT_USE_PROFILE_ROOT");
        }
        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
    }

    #[test]
    fn mcp_status_and_read_tools_report_db_locked_without_claimable_context() {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));
        ok(server.call_tool("codegraph.index_repo", &json!({"repo": path_string(&repo)})));
        let db_path = default_db_path(&repo);
        let lock = rusqlite::Connection::open(&db_path).expect("open lock connection");
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

        let before_bytes = fs::read(&db_path).expect("read locked DB before MCP inspection");
        let status = ok(server.call_tool("codegraph.status", &json!({"repo": path_string(&repo)})));
        let search_error = server
            .call_tool(
                "codegraph.search_symbols",
                &json!({"repo": path_string(&repo), "query": "login"}),
            )
            .expect_err("read tool must not return claimable context while DB is locked");
        let after_bytes = fs::read(&db_path).expect("read locked DB after MCP inspection");

        assert_eq!(status["status"].as_str(), Some("db_problem"));
        assert_eq!(status["problem"].as_str(), Some("db_locked"));
        assert_eq!(status["db_problem"].as_str(), Some("db_locked"));
        assert_eq!(status["safe_to_query"].as_bool(), Some(false));
        assert_eq!(status["graph_proof_available"].as_bool(), Some(false));
        assert_eq!(status["graph_freshness"].as_str(), Some("db_locked"));
        assert!(status["retryable_labels"]
            .as_array()
            .expect("retryable labels")
            .iter()
            .any(|label| label.as_str() == Some("db_locked")));
        assert_eq!(
            status["passport_summary"]["db_problem_kind"].as_str(),
            Some("db_locked")
        );
        assert_eq!(search_error.code, "db_locked");
        assert_eq!(before_bytes, after_bytes);

        lock.execute_batch("ROLLBACK").expect("release lock");
        drop(lock);
        fs::remove_dir_all(repo).expect("cleanup");
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
    fn mcp_call_relation_query_resolves_unambiguous_symbol_exactly() {
        let (server, repo, target_id) = mcp_relation_precision_fixture();

        let result = ok(server.call_tool(
            "codegraph.find_callers",
            &json!({"repo": path_string(&repo), "query": "preciseTarget"}),
        ));

        assert_eq!(result["status"].as_str(), Some("ok"));
        assert_eq!(result["resolution_mode"].as_str(), Some("exact_resolved"));
        assert_eq!(
            result["exact_resolved_entity"]["id"].as_str(),
            Some(target_id.as_str())
        );
        assert!(result["exact_resolved_entity_results"].is_array());
        assert!(result["fuzzy_or_global_results"]
            .as_array()
            .expect("fuzzy section")
            .is_empty());
        assert!(result["ambiguous_symbol_matches"]
            .as_array()
            .expect("ambiguous section")
            .is_empty());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    fn mcp_relation_precision_fixture() -> (McpServer, PathBuf, String) {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));
        ok(server.call_tool("codegraph.index_repo", &json!({"repo": path_string(&repo)})));
        let store = SqliteGraphStore::open(default_db_path(&repo)).expect("open store");
        let target = mcp_test_function_entity(
            "src/precise.ts",
            "preciseTarget",
            "precise.preciseTarget",
            20,
        );
        let caller = mcp_test_function_entity(
            "src/precise.ts",
            "preciseCaller",
            "precise.preciseCaller",
            25,
        );
        store.upsert_entity(&target).expect("upsert target");
        store.upsert_entity(&caller).expect("upsert caller");
        store
            .upsert_edge(&mcp_test_call_edge(&caller, &target, "src/precise.ts", 26))
            .expect("upsert edge");
        (server, repo, target.id)
    }

    fn mcp_test_function_entity(path: &str, name: &str, qualified_name: &str, line: u32) -> Entity {
        Entity {
            id: codegraph_core::stable_entity_id_for_kind(
                path,
                codegraph_core::EntityKind::Function,
                qualified_name,
                Some(qualified_name),
            ),
            kind: codegraph_core::EntityKind::Function,
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

    fn mcp_test_call_edge(head: &Entity, tail: &Entity, path: &str, line: u32) -> Edge {
        let span = SourceSpan::with_columns(path, line, 3, line, 24);
        Edge {
            id: codegraph_core::stable_edge_id(&head.id, RelationKind::Calls, &tail.id, &span),
            head_id: head.id.clone(),
            relation: RelationKind::Calls,
            tail_id: tail.id.clone(),
            source_span: span,
            repo_commit: None,
            file_hash: None,
            extractor: "unit-test".to_string(),
            confidence: 1.0,
            exactness: codegraph_core::Exactness::ParserVerified,
            edge_class: codegraph_core::EdgeClass::BaseExact,
            context: codegraph_core::EdgeContext::Production,
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Default::default(),
        }
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
    fn mcp_reference_docs_match_advertised_tools_and_safety_annotations() {
        let docs_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("docs")
            .join("mcp-reference.md");
        let docs = fs::read_to_string(&docs_path).expect("read mcp reference docs");
        for name in TOOL_NAMES {
            assert!(docs.contains(&format!("`{name}`")), "docs missing {name}");
        }
        for advertised_only in [
            "codegraph.search",
            "codegraph.analyze",
            "codegraph.plan_context",
            "codegraph.explain_missing",
        ] {
            assert!(
                docs.contains(&format!("`{advertised_only}`")),
                "docs must include high-level advertised tool {advertised_only}"
            );
        }
        assert!(docs.contains("destructiveHint = false"));
        assert!(docs.contains("readOnlyHint = true"));
        assert!(docs.contains("readOnlyHint = false"));
        assert!(
            docs.contains("MCP startup does not surprise-index"),
            "docs must preserve no-auto-index lifecycle language"
        );
    }

    #[test]
    fn mcp_validate_edit_tool_schema_and_metadata_match_contract() {
        let server = McpServer::new(McpServerConfig::default());
        let tool = server
            .tool_definitions()
            .into_iter()
            .find(|tool| tool["name"].as_str() == Some(MCP_VALIDATE_EDIT_TOOL_NAME))
            .expect("validate_edit tool definition");

        assert_eq!(tool["annotations"]["readOnlyHint"].as_bool(), Some(false));
        assert_eq!(
            tool["annotations"]["destructiveHint"].as_bool(),
            Some(false)
        );
        assert_eq!(tool["annotations"]["sourceMutation"].as_bool(), Some(false));
        assert_eq!(
            tool["annotations"]["startupAutoIndex"].as_bool(),
            Some(false)
        );
        assert_eq!(
            tool["annotations"]["dotCodegraphFallback"].as_bool(),
            Some(false)
        );
        let required = tool["inputSchema"]["required"]
            .as_array()
            .expect("required array");
        assert!(required.iter().any(|field| field.as_str() == Some("repo")));
        assert!(required
            .iter()
            .any(|field| field.as_str() == Some("changed_files")));
        assert_eq!(
            tool["inputSchema"]["properties"]["changed_files"]["minItems"].as_u64(),
            Some(1)
        );
        assert!(tool["outputSchema"]["properties"]["validation_packet"].is_object());
        assert!(tool["outputSchema"]["properties"]["hard_interrupt_available"].is_object());
    }

    #[test]
    fn mcp_validate_edit_empty_changed_files_is_tool_error() {
        let repo = fixture_repo();
        let db_path = repo
            .parent()
            .expect("repo parent")
            .join("profile-empty")
            .join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let server = McpServer::new(McpServerConfig::for_repo(&repo).with_db_path(&db_path));

        let error = server
            .call_tool(
                MCP_VALIDATE_EDIT_TOOL_NAME,
                &json!({
                    "repo": path_string(&repo),
                    "changed_files": [],
                    "mode": "agent-json"
                }),
            )
            .expect_err("empty changed_files is invalid input");
        assert_eq!(error.code, "invalid_input");

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_validate_edit_missing_profile_db_is_structured_success_without_dot_codegraph() {
        let repo = fixture_repo();
        let profile_root = repo
            .parent()
            .expect("repo parent")
            .join("app-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        if profile_root.exists() {
            fs::remove_dir_all(&profile_root).expect("remove stale profile");
        }
        let db_path = profile_root.join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let server = McpServer::new(McpServerConfig::for_repo(&repo).with_db_path(&db_path));

        let response = server
            .handle_jsonrpc(&json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": {
                    "name": MCP_VALIDATE_EDIT_TOOL_NAME,
                    "arguments": {
                        "repo": path_string(&repo),
                        "changed_files": ["src/auth.ts"],
                        "mode": "agent-json"
                    }
                }
            }))
            .expect("tools/call response");
        let result = &response["result"];
        assert_eq!(result["isError"].as_bool(), Some(false));
        let packet = &result["structuredContent"];
        assert_eq!(packet["status"].as_str(), Some("preflight_blocked"));
        assert_eq!(packet["no_dot_codegraph_fallback"].as_bool(), Some(true));
        assert_eq!(packet["startup_auto_index"].as_bool(), Some(false));
        assert_eq!(
            packet["normal_dot_codegraph_mutated"].as_bool(),
            Some(false)
        );
        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(false));
        assert!(!repo.join(".codegraph").exists());
        assert!(!db_path.exists());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_validate_edit_rejects_outside_paths_as_structured_response() {
        let repo = fixture_repo();
        let profile_root = repo
            .parent()
            .expect("repo parent")
            .join("app-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        if profile_root.exists() {
            fs::remove_dir_all(&profile_root).expect("remove stale profile");
        }
        fs::create_dir_all(&profile_root).expect("profile root");
        let db_path = profile_root.join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let server = McpServer::new(McpServerConfig::for_repo(&repo).with_db_path(&db_path));
        ok(server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&repo), "db_path": path_string(&db_path)}),
        ));

        let packet = ok(server.call_tool(
            MCP_VALIDATE_EDIT_TOOL_NAME,
            &json!({
                "repo": path_string(&repo),
                "changed_files": ["..\\outside.ts"],
                "mode": "audit-json"
            }),
        ));
        assert_eq!(packet["status"].as_str(), Some("rejected"));
        assert_eq!(
            packet["rejected_paths"].as_array().expect("rejected").len(),
            1
        );
        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(false));

        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
    }

    #[test]
    fn mcp_validate_edit_modes_return_same_safety_fields() {
        let repo = fixture_repo();
        let profile_root = repo
            .parent()
            .expect("repo parent")
            .join("app-cccccccccccccccccccccccccccccccc");
        if profile_root.exists() {
            fs::remove_dir_all(&profile_root).expect("remove stale profile");
        }
        fs::create_dir_all(&profile_root).expect("profile root");
        let db_path = profile_root.join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let server = McpServer::new(McpServerConfig::for_repo(&repo).with_db_path(&db_path));
        ok(server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&repo), "db_path": path_string(&db_path)}),
        ));

        for mode in ["agent-json", "explain", "audit-json"] {
            let packet = ok(server.call_tool(
                MCP_VALIDATE_EDIT_TOOL_NAME,
                &json!({
                    "repo": path_string(&repo),
                    "changed_files": ["src/auth.ts"],
                    "mode": mode,
                    "expected_touched_files": ["src/auth.ts"]
                }),
            ));
            assert_eq!(packet["detail_mode"].as_str(), Some(mode));
            assert!(packet["validation_packet"]["status"].is_string());
            assert!(packet["validation_packet"]["must_fix_before_continuing"].is_boolean());
            assert!(packet["validation_packet"]["hard_interrupt_available"].is_boolean());
            assert!(packet["claimability"].is_object());
            assert!(packet["lifecycle"].is_object());
            assert_eq!(
                packet["expected_touched_files_missing"]
                    .as_array()
                    .expect("missing expected")
                    .len(),
                0
            );
        }

        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
    }

    fn mcp_validate_edit_dangling_response(fail_on_blocking: bool) -> (Value, PathBuf, PathBuf) {
        let repo = fixture_repo();
        fs::write(
            repo.join("src").join("dangling.ts"),
            "export function danglingCaller() {\n  return 1;\n}\n",
        )
        .expect("write dangling source");
        let profile_id = FIXTURE_COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let profile_root = repo
            .parent()
            .expect("repo parent")
            .join(format!("app-validate-edit-dangling-{profile_id}"));
        if profile_root.exists() {
            fs::remove_dir_all(&profile_root).expect("remove stale profile");
        }
        fs::create_dir_all(&profile_root).expect("profile root");
        let db_path = profile_root.join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let server = McpServer::new(McpServerConfig::for_repo(&repo).with_db_path(&db_path));
        ok(server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&repo), "db_path": path_string(&db_path)}),
        ));
        ok(server.call_tool(
            MCP_VALIDATE_EDIT_TOOL_NAME,
            &json!({
                "repo": path_string(&repo),
                "changed_files": ["src/dangling.ts"],
                "mode": "agent-json"
            }),
        ));
        let store = SqliteGraphStore::open(&db_path).expect("open validation DB");
        let caller = mcp_test_function_entity(
            "src/dangling.ts",
            "danglingCaller",
            "dangling.danglingCaller",
            1,
        );
        let missing_target = mcp_test_function_entity(
            "src/dangling.ts",
            "missingTarget",
            "dangling.missingTarget",
            2,
        );
        store.upsert_entity(&caller).expect("upsert caller");
        store
            .upsert_edge(&mcp_test_call_edge(
                &caller,
                &missing_target,
                "src/dangling.ts",
                2,
            ))
            .expect("upsert dangling edge");
        drop(store);

        let response = server
            .handle_jsonrpc(&json!({
                "jsonrpc": "2.0",
                "id": 8,
                "method": "tools/call",
                "params": {
                    "name": MCP_VALIDATE_EDIT_TOOL_NAME,
                    "arguments": {
                        "repo": path_string(&repo),
                        "changed_files": ["src/dangling.ts"],
                        "mode": "agent-json",
                        "fail_on_blocking": fail_on_blocking
                    }
                }
            }))
            .expect("tools/call response");
        (response, repo, profile_root)
    }

    fn mcp_test_severity_rule(status: &str) -> ValidationRule {
        match status {
            "diagnostic_only" => ValidationRule::diagnostic(
                "CG_MVP3_DIAGNOSTIC_ONLY_TEST",
                "diagnostic-only test observation",
                "Diagnostic-only validation context.",
            ),
            _ => {
                let mut rule = ValidationRule::exact_blocking(
                    match status {
                        "warning" => "CG_MVP3_WARNING_TEST",
                        "unknown" => "CG_MVP3_UNKNOWN_TEST",
                        _ => CG_MVP3_CALLS_DANGLING_TARGET,
                    },
                    ValidationRuleKind::DanglingTarget,
                    Some(RelationKind::Calls),
                    "exact CALLS edges must resolve to a claimable target",
                    "Define the missing target or update the exact call relation.",
                );
                if status == "warning" {
                    rule.supported_relation_status = SupportedRelationStatus::ExactWarningCandidate;
                }
                rule
            }
        }
    }

    fn mcp_test_severity_finding(status: &str, rule: &ValidationRule) -> Option<ValidationFinding> {
        if status == "ok" {
            return None;
        }
        let mut input = ValidationReverificationInput::exact_graph_source(
            ValidationLifecycleState::claimable_current(),
            format!("edge://{status}"),
            format!("{status} severity policy fixture"),
        );
        if status == "unknown" {
            input.graph_source_relation_reverified = false;
        }
        let mut finding =
            classify_validation_finding(rule, format!("finding://mcp/{status}"), input);
        finding.affected_file = Some("src/auth.ts".to_string());
        finding.source_span = Some(SourceSpan::with_columns("src/auth.ts", 1, 1, 1, 12));
        finding.source_role = Some(EvidenceRole::Production);
        finding.recommended_fix = Some("Severity semantics fixture.".to_string());
        finding.suggested_next_steps = vec!["Rerun validate_edit after reviewing.".to_string()];
        Some(finding)
    }

    fn mcp_test_severity_packet(status: &str) -> ValidationPacket {
        let rule = mcp_test_severity_rule(status);
        let findings = mcp_test_severity_finding(status, &rule)
            .into_iter()
            .collect::<Vec<_>>();
        let mut packet = ValidationPacket::new(
            vec!["src/auth.ts".to_string()],
            json!({}),
            findings,
            vec![rule],
            Vec::new(),
            json!({"claimable": true, "current": true}),
            json!({}),
            json!({"claimable": true, "current": true}),
        );
        if status == "blocking_graph_error" {
            packet = packet.with_eligible_hard_interrupts("test-generated-at");
        }
        packet
    }

    fn mcp_packet_status_string(packet: &ValidationPacket) -> String {
        serde_json::to_value(packet.status)
            .expect("packet status json")
            .as_str()
            .expect("packet status string")
            .to_string()
    }

    fn mcp_validate_edit_severity_response(status: &str) -> (Value, PathBuf, PathBuf) {
        let repo = fixture_repo();
        let profile_id = FIXTURE_COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let profile_root = repo
            .parent()
            .expect("repo parent")
            .join(format!("app-validate-edit-severity-{profile_id}"));
        if profile_root.exists() {
            fs::remove_dir_all(&profile_root).expect("remove stale profile");
        }
        fs::create_dir_all(&profile_root).expect("profile root");
        let db_path = profile_root.join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let server = McpServer::new(McpServerConfig::for_repo(&repo).with_db_path(&db_path));
        ok(server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&repo), "db_path": path_string(&db_path)}),
        ));
        let preflight =
            inspect_db_lifecycle_preflight(&repo, &db_path, None).expect("safe DB preflight");
        let path_preflight = validate_edit_changed_files_preflight_with_scope(
            &repo,
            &[PathBuf::from("src/auth.ts")],
            &IndexScopeOptions::default(),
            VALIDATE_EDIT_CHANGED_FILES_MAX,
        );
        let packet = mcp_test_severity_packet(status);
        let packet_status = mcp_packet_status_string(&packet);
        let structured = mcp_validate_edit_response(
            &repo,
            &db_path,
            "production_agent_use_profile",
            true,
            &packet_status,
            "agent-json",
            status == "blocking_graph_error",
            None,
            None,
            Vec::new(),
            None,
            &path_preflight,
            &preflight,
            &packet,
            json!({"status": "synthetic_severity_fixture"}),
            json!({}),
            json!({"total_ms": 0}),
            false,
        )
        .expect("severity response");
        (
            json!({"result": mcp_tool_result(structured, false)}),
            repo,
            profile_root,
        )
    }

    fn assert_mcp_structured_success_for_status(status: &str) {
        let (response, repo, profile_root) = mcp_validate_edit_severity_response(status);
        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["isError"].as_bool(), Some(false));
        assert_eq!(
            response["result"]["structuredContent"]["status"].as_str(),
            Some(status)
        );
        assert_eq!(
            response["result"]["structuredContent"]["normal_dot_codegraph_mutated"].as_bool(),
            Some(false)
        );
        assert_eq!(
            response["result"]["structuredContent"]["startup_auto_index"].as_bool(),
            Some(false)
        );
        assert_eq!(
            response["result"]["structuredContent"]["no_dot_codegraph_fallback"].as_bool(),
            Some(true)
        );
        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
    }

    #[test]
    fn mcp_response_semantics_match_severity_policy() {
        for status in [
            "blocking_graph_error",
            "warning",
            "unknown",
            "diagnostic_only",
            "ok",
        ] {
            assert_mcp_structured_success_for_status(status);
        }
    }

    #[test]
    fn mcp_warning_structured_success() {
        assert_mcp_structured_success_for_status("warning");
    }

    #[test]
    fn mcp_unknown_structured_success() {
        assert_mcp_structured_success_for_status("unknown");
    }

    #[test]
    fn mcp_diagnostic_structured_success() {
        assert_mcp_structured_success_for_status("diagnostic_only");
    }

    #[test]
    fn mcp_ok_structured_success() {
        assert_mcp_structured_success_for_status("ok");
    }

    #[test]
    fn mcp_no_startup_auto_index_and_no_dot_codegraph_fallback() {
        let (response, repo, profile_root) = mcp_validate_edit_severity_response("ok");
        let packet = &response["result"]["structuredContent"];
        assert_eq!(packet["startup_auto_index"].as_bool(), Some(false));
        assert_eq!(packet["no_dot_codegraph_fallback"].as_bool(), Some(true));
        assert!(!repo.join(".codegraph").exists());
        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
    }

    #[test]
    fn mcp_hard_interrupt_propagation_correct() {
        let (response, repo, profile_root) = mcp_validate_edit_dangling_response(true);
        let result = &response["result"];
        assert_eq!(result["isError"].as_bool(), Some(false));
        let packet = &result["structuredContent"];

        assert_eq!(packet["status"].as_str(), Some("blocking_graph_error"));
        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(true));
        assert_eq!(packet["must_fix_before_continuing"].as_bool(), Some(true));
        assert_eq!(
            packet["validation_packet"]["hard_interrupt_available"].as_bool(),
            Some(true)
        );
        let rule_counts = &packet["validation_packet"]["summary_counts_by_rule_id"];
        assert!(
            rule_counts[CG_MVP3_CALLS_DANGLING_TARGET]
                .as_u64()
                .unwrap_or_default()
                > 0,
            "expected CALLS dangling rule count: {packet}"
        );
        let interrupt = &packet["hard_interrupt"];
        let errors = interrupt["errors"]
            .as_array()
            .expect("hard interrupt errors");
        assert!(!errors.is_empty(), "{packet}");
        let first_error = &errors[0];
        assert_eq!(
            first_error["validation_rule_id"].as_str(),
            Some(CG_MVP3_CALLS_DANGLING_TARGET)
        );
        assert!(first_error["source_span"].is_object(), "{packet}");
        assert!(first_error["recommended_fix"]
            .as_str()
            .is_some_and(|fix| !fix.is_empty()));
        assert!(first_error["suggested_next_steps"]
            .as_array()
            .is_some_and(|steps| !steps.is_empty()));

        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
    }

    #[test]
    fn mcp_validate_edit_release_source_fixture_hard_interrupts_without_seeded_store() {
        let repo = fixture_repo();
        fs::write(
            repo.join("src").join("service.js"),
            "export function removedTarget() {\n  return 1;\n}\n",
        )
        .expect("write service source");
        fs::write(
            repo.join("src").join("consumer.js"),
            "import { removedTarget } from './service';\n\nexport function caller() {\n  return removedTarget();\n}\n",
        )
        .expect("write consumer source");
        let profile_id = FIXTURE_COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let profile_root = repo
            .parent()
            .expect("repo parent")
            .join(format!("app-validate-edit-source-hard-{profile_id}"));
        if profile_root.exists() {
            fs::remove_dir_all(&profile_root).expect("remove stale profile");
        }
        fs::create_dir_all(&profile_root).expect("profile root");
        let db_path = profile_root.join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let server = McpServer::new(McpServerConfig::for_repo(&repo).with_db_path(&db_path));

        ok(server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&repo), "db_path": path_string(&db_path)}),
        ));
        fs::write(
            repo.join("src").join("service.js"),
            "export function keptTarget() {\n  return 1;\n}\n",
        )
        .expect("remove target source");

        let response = server
            .handle_jsonrpc(&json!({
                "jsonrpc": "2.0",
                "id": 8,
                "method": "tools/call",
                "params": {
                    "name": MCP_VALIDATE_EDIT_TOOL_NAME,
                    "arguments": {
                        "repo": path_string(&repo),
                        "db_path": path_string(&db_path),
                        "changed_files": ["src/service.js", "src/consumer.js"],
                        "mode": "agent-json",
                        "fail_on_blocking": true
                    }
                }
            }))
            .expect("tools/call response");
        let result = &response["result"];
        assert_eq!(result["isError"].as_bool(), Some(false));
        let packet = &result["structuredContent"];
        assert_eq!(packet["status"].as_str(), Some("blocking_graph_error"));
        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(true));
        assert_eq!(packet["must_fix_before_continuing"].as_bool(), Some(true));
        let errors = packet["hard_interrupt"]["errors"]
            .as_array()
            .expect("hard interrupt errors");
        assert!(!errors.is_empty(), "{packet}");
        assert!(
            errors.iter().any(|error| {
                error["validation_rule_id"].as_str() == Some(CG_MVP3_CALLS_DANGLING_TARGET)
                    && error["source_span"].is_object()
                    && error["recommended_fix"]
                        .as_str()
                        .is_some_and(|fix| !fix.is_empty())
                    && error["suggested_next_steps"]
                        .as_array()
                        .is_some_and(|steps| !steps.is_empty())
            }),
            "{packet}"
        );
        assert_eq!(
            packet["normal_dot_codegraph_mutated"].as_bool(),
            Some(false)
        );

        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
    }

    #[test]
    fn mcp_validate_edit_rename_lifecycle_unknown_structured_success() {
        let repo = fixture_repo();
        fs::write(
            repo.join("src").join("old_name.js"),
            "export function oldName() {\n  return 1;\n}\n",
        )
        .expect("write renamed source");
        let profile_id = FIXTURE_COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let profile_root = repo
            .parent()
            .expect("repo parent")
            .join(format!("app-validate-edit-rename-unknown-{profile_id}"));
        if profile_root.exists() {
            fs::remove_dir_all(&profile_root).expect("remove stale profile");
        }
        fs::create_dir_all(&profile_root).expect("profile root");
        let db_path = profile_root.join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let server = McpServer::new(McpServerConfig::for_repo(&repo).with_db_path(&db_path));

        ok(server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&repo), "db_path": path_string(&db_path)}),
        ));
        fs::rename(
            repo.join("src").join("old_name.js"),
            repo.join("src").join("new_name.js"),
        )
        .expect("rename source");

        let response = server
            .handle_jsonrpc(&json!({
                "jsonrpc": "2.0",
                "id": 18,
                "method": "tools/call",
                "params": {
                    "name": MCP_VALIDATE_EDIT_TOOL_NAME,
                    "arguments": {
                        "repo": path_string(&repo),
                        "db_path": path_string(&db_path),
                        "changed_files": ["src/old_name.js", "src/new_name.js"],
                        "mode": "agent-json"
                    }
                }
            }))
            .expect("tools/call response");
        let result = &response["result"];
        assert_eq!(result["isError"].as_bool(), Some(false), "{response}");
        let packet = &result["structuredContent"];
        assert_eq!(packet["status"].as_str(), Some("unknown"), "{packet}");
        assert_eq!(
            packet["hard_interrupt_available"].as_bool(),
            Some(false),
            "{packet}"
        );
        assert_eq!(
            packet["must_fix_before_continuing"].as_bool(),
            Some(false),
            "{packet}"
        );
        let rule_counts = &packet["validation_packet"]["summary_counts_by_rule_id"];
        assert!(
            rule_counts[CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED]
                .as_u64()
                .unwrap_or_default()
                > 0,
            "expected rename unknown rule count: {packet}"
        );
        assert_eq!(
            packet["normal_dot_codegraph_mutated"].as_bool(),
            Some(false)
        );

        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
    }

    #[test]
    fn mcp_blocking_structured_success_response() {
        let (response, repo, profile_root) = mcp_validate_edit_dangling_response(true);

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["isError"].as_bool(), Some(false));
        assert_eq!(
            response["result"]["structuredContent"]["status"].as_str(),
            Some("blocking_graph_error")
        );
        assert_eq!(
            response["result"]["structuredContent"]["hard_interrupt_available"].as_bool(),
            Some(true)
        );

        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
    }

    #[test]
    fn mcp_blocking_structured_success_preserved() {
        let (response, repo, profile_root) = mcp_validate_edit_dangling_response(true);

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["isError"].as_bool(), Some(false));
        assert_eq!(
            response["result"]["structuredContent"]["status"].as_str(),
            Some("blocking_graph_error")
        );
        assert_eq!(
            response["result"]["structuredContent"]["must_fix_before_continuing"].as_bool(),
            Some(true)
        );

        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
    }

    #[test]
    fn mcp_tool_error_reserved_for_runtime_failure() {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));

        let error = server
            .call_tool(
                MCP_VALIDATE_EDIT_TOOL_NAME,
                &json!({
                    "repo": path_string(&repo),
                    "changed_files": ["src/auth.ts"],
                    "mode": "invalid-mode"
                }),
            )
            .expect_err("profile DB resolution failure is a tool error, not a validation packet");
        assert_eq!(error.code, "profile_db_unresolved");
        assert!(!error.message.contains("hard_interrupt"));

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn mcp_tool_errors_reserved_for_runtime_protocol_failure() {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));
        let response = server
            .handle_jsonrpc(&json!({
                "jsonrpc": "2.0",
                "id": 88,
                "method": "tools/call",
                "params": {
                    "name": MCP_VALIDATE_EDIT_TOOL_NAME,
                    "arguments": {
                        "repo": path_string(&repo),
                        "changed_files": ["src/auth.ts"],
                        "mode": "agent-json"
                    }
                }
            }))
            .expect("tools/call response");

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["isError"].as_bool(), Some(true));
        let error = &response["result"]["structuredContent"];
        assert_eq!(error["status"].as_str(), Some("error"));
        assert_eq!(error["error"].as_str(), Some("profile_db_unresolved"));
        assert!(error.get("validation_packet").is_none());

        fs::remove_dir_all(repo).expect("cleanup repo");
    }

    #[test]
    fn mcp_invalid_input_behavior_documented_and_tested() {
        let repo = fixture_repo();
        let profile_root = repo
            .parent()
            .expect("repo parent")
            .join("app-validate-edit-invalid-input");
        if profile_root.exists() {
            fs::remove_dir_all(&profile_root).expect("remove stale profile");
        }
        fs::create_dir_all(&profile_root).expect("profile root");
        let db_path = profile_root.join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let server = McpServer::new(McpServerConfig::for_repo(&repo).with_db_path(&db_path));
        let response = server
            .handle_jsonrpc(&json!({
                "jsonrpc": "2.0",
                "id": 89,
                "method": "tools/call",
                "params": {
                    "name": MCP_VALIDATE_EDIT_TOOL_NAME,
                    "arguments": {
                        "repo": path_string(&repo),
                        "db_path": path_string(&db_path),
                        "changed_files": [],
                        "mode": "agent-json"
                    }
                }
            }))
            .expect("tools/call response");

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["isError"].as_bool(), Some(true));
        assert_eq!(
            response["result"]["structuredContent"]["error"].as_str(),
            Some("invalid_input")
        );

        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
    }

    #[test]
    fn mcp_validate_edit_blocking_graph_error_is_structured_success() {
        let repo = fixture_repo();
        fs::write(
            repo.join("src").join("dangling.ts"),
            "export function danglingCaller() {\n  return 1;\n}\n",
        )
        .expect("write dangling source");
        let profile_root = repo
            .parent()
            .expect("repo parent")
            .join("app-dddddddddddddddddddddddddddddddd");
        if profile_root.exists() {
            fs::remove_dir_all(&profile_root).expect("remove stale profile");
        }
        fs::create_dir_all(&profile_root).expect("profile root");
        let db_path = profile_root.join(MCP_AGENT_USE_PROFILE_DB_FILE_NAME);
        let server = McpServer::new(McpServerConfig::for_repo(&repo).with_db_path(&db_path));
        ok(server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&repo), "db_path": path_string(&db_path)}),
        ));
        ok(server.call_tool(
            MCP_VALIDATE_EDIT_TOOL_NAME,
            &json!({
                "repo": path_string(&repo),
                "changed_files": ["src/dangling.ts"],
                "mode": "agent-json"
            }),
        ));
        let store = SqliteGraphStore::open(&db_path).expect("open validation DB");
        let caller = mcp_test_function_entity(
            "src/dangling.ts",
            "danglingCaller",
            "dangling.danglingCaller",
            1,
        );
        let missing_target = mcp_test_function_entity(
            "src/dangling.ts",
            "missingTarget",
            "dangling.missingTarget",
            2,
        );
        store.upsert_entity(&caller).expect("upsert caller");
        store
            .upsert_edge(&mcp_test_call_edge(
                &caller,
                &missing_target,
                "src/dangling.ts",
                2,
            ))
            .expect("upsert dangling edge");
        drop(store);

        let response = server
            .handle_jsonrpc(&json!({
                "jsonrpc": "2.0",
                "id": 8,
                "method": "tools/call",
                "params": {
                    "name": MCP_VALIDATE_EDIT_TOOL_NAME,
                    "arguments": {
                        "repo": path_string(&repo),
                        "changed_files": ["src/dangling.ts"],
                        "mode": "agent-json",
                        "fail_on_blocking": true
                    }
                }
            }))
            .expect("tools/call response");
        let result = &response["result"];
        assert_eq!(result["isError"].as_bool(), Some(false));
        let packet = &result["structuredContent"];
        assert_eq!(packet["status"].as_str(), Some("blocking_graph_error"));
        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(true));
        assert_eq!(packet["must_fix_before_continuing"].as_bool(), Some(true));
        let rule_counts = &packet["validation_packet"]["summary_counts_by_rule_id"];
        let dangling_count = rule_counts[CG_MVP3_CALLS_DANGLING_TARGET]
            .as_u64()
            .unwrap_or(0)
            + rule_counts[CG_MVP3_IMPORTS_DANGLING_TARGET]
                .as_u64()
                .unwrap_or(0)
            + rule_counts[CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH]
                .as_u64()
                .unwrap_or(0);
        assert!(
            dangling_count > 0,
            "expected exact dangling rule count: {packet}"
        );

        fs::remove_dir_all(repo).expect("cleanup repo");
        fs::remove_dir_all(profile_root).expect("cleanup profile");
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
        assert_eq!(status["path_access_status"].as_str(), Some("ok"));
        assert_eq!(status["db_path_outside_workspace"].as_bool(), Some(false));
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
        assert_eq!(status["relation_facts"], status["edges"]);
        assert_eq!(
            status["release_reported_relation_facts"],
            status["relation_facts"]
        );
        assert_eq!(
            status["metric_label_notes"]["edges"].as_str(),
            Some("Deprecated compatibility alias for relation_facts.")
        );
        assert_eq!(status["graph_db_status"].as_str(), Some("ready"));
        assert_eq!(status["graph_proof_available"].as_bool(), Some(true));
        assert_eq!(status["graph_freshness"].as_str(), Some("current"));
        assert_eq!(status["dirty_state"].as_str(), Some("ready"));
        assert_eq!(
            status["rtds_freshness"]["startup_auto_index"].as_bool(),
            Some(false)
        );
        assert_eq!(
            status["rtds_freshness"]["dot_codegraph_fallback"].as_bool(),
            Some(false)
        );
        assert!(status["recovery_commands"].as_array().is_some());
        assert_eq!(
            status["last_delta_update_summary"],
            Value::Null,
            "fresh full index has no RTDS delta history yet"
        );
        assert!(status["active_candidate_sources"]
            .as_array()
            .expect("candidate sources")
            .iter()
            .any(|source| source.as_str() == Some("graph_db")));
        assert!(status["blockers"].as_array().expect("blockers").is_empty());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_status_missing_db_reports_index_required() {
        let repo = fixture_repo();
        let server = McpServer::new(McpServerConfig::for_repo(&repo));

        let status = ok(server.call_tool("codegraph.status", &json!({"repo": path_string(&repo)})));
        let context_error = server
            .call_tool(
                "codegraph.context_pack",
                &json!({"repo": path_string(&repo), "task": "inspect login"}),
            )
            .expect_err("context-pack must refuse unsafe graph proof without candidate context");

        assert_eq!(status["status"].as_str(), Some("missing"));
        assert_eq!(status["problem"].as_str(), Some("index_required"));
        assert_eq!(status["db_problem"].as_str(), Some("db_missing"));
        assert_eq!(status["path_access_status"].as_str(), Some("db_missing"));
        assert_eq!(status["db_path_outside_workspace"].as_bool(), Some(false));
        assert_eq!(status["safe_to_query"].as_bool(), Some(false));
        assert_eq!(status["graph_db_status"].as_str(), Some("no_index"));
        assert_eq!(status["graph_proof_available"].as_bool(), Some(false));
        assert_eq!(status["graph_freshness"].as_str(), Some("absent"));
        assert_eq!(status["delta_state"].as_str(), Some("absent"));
        assert_eq!(
            status["rtds_freshness"]["startup_auto_index"].as_bool(),
            Some(false)
        );
        assert_eq!(
            status["rtds_freshness"]["dot_codegraph_fallback"].as_bool(),
            Some(false)
        );
        assert_eq!(status["candidate_only_available"].as_bool(), Some(false));
        assert_blocker_contains(&status, "main DB does not exist");
        assert_eq!(context_error.code, "db_missing");
        assert!(status.get("files").is_none());
        assert!(status.get("entities").is_none());
        assert!(status.get("edges").is_none());

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_context_pack_candidate_spool_only_returns_candidate_labels() {
        let repo = fixture_repo();
        let spool = write_mcp_candidate_spool_fixture(&repo);
        let server = McpServer::new(McpServerConfig::for_repo(&repo));

        let result = ok(server.call_tool(
            "codegraph.context_pack",
            &json!({
                "repo": path_string(&repo),
                "task": "Change login email handling",
                "candidate_spool": path_string(&spool),
                "early_candidates": true
            }),
        ));

        assert_eq!(result["status"].as_str(), Some("ok"));
        assert_eq!(result["proof_status"].as_str(), Some("candidate_only"));
        assert_eq!(
            result["proof_strength"].as_str(),
            Some("candidate_evidence")
        );
        assert_eq!(result["graph_proof"].as_bool(), Some(false));
        assert_eq!(result["graph_proof_available"].as_bool(), Some(false));
        assert_eq!(result["candidate_only_available"].as_bool(), Some(true));
        assert_eq!(
            result["rtds_freshness"]["candidate_context_policy"].as_str(),
            Some("candidate_only_only_when_current_source_bound")
        );
        assert_eq!(
            result["rtds_freshness"]["startup_auto_index"].as_bool(),
            Some(false)
        );
        assert_eq!(
            result["candidate_spool_status"].as_str(),
            Some("partial_ready")
        );
        assert_eq!(result["candidate_context_truncated"].as_bool(), Some(false));
        assert_eq!(result["candidate_spool_unavailable"].as_bool(), Some(false));
        assert_eq!(
            result["staged_availability"]["recommended_next_step"].as_str(),
            Some("inspect candidate spans")
        );
        assert!(result["snippets"]
            .as_array()
            .expect("candidate snippets")
            .iter()
            .all(|snippet| snippet["graph_proof"].as_bool() == Some(false)));

        fs::write(
            repo.join("src").join("auth.ts"),
            "export function changedLogin() { return 'changed'; }\n",
        )
        .expect("modify source");
        let stale_status = ok(server.call_tool(
            "codegraph.status",
            &json!({
                "repo": path_string(&repo),
                "candidate_spool": path_string(&spool)
            }),
        ));
        assert_eq!(
            stale_status["candidate_spool_status"].as_str(),
            Some("stale")
        );
        assert_eq!(
            stale_status["staged_availability"]["layer_readiness"]["candidate_spool"]["ready"]
                .as_bool(),
            Some(false)
        );
        assert_eq!(
            stale_status["candidate_spool_unavailable"].as_bool(),
            Some(true)
        );

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_candidate_spool_query_index_access_error_is_not_labeled_corrupt() {
        let repo = fixture_repo();
        let spool = write_mcp_candidate_spool_fixture(&repo);
        let index_path = codegraph_index::candidate_spool_query_index_path(&spool);
        fs::create_dir(&index_path).expect("create inaccessible query index path");

        let status = super::mcp_candidate_spool_layer_status(&repo, &spool);
        assert_ne!(status["status"].as_str(), Some("query_index_corrupt"));
        assert!(matches!(
            status["status"].as_str(),
            Some("filesystem_inaccessible")
                | Some("permission_denied")
                | Some("sidecar_unavailable")
        ));
        assert_eq!(status["ready"].as_bool(), Some(false));
        assert_eq!(status["candidate_spool_unavailable"].as_bool(), Some(true));
        assert_eq!(
            status["query_index_problem_kind"].as_str(),
            status["status"].as_str()
        );

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
        assert_eq!(search.code, "repo_root_mismatch");
        assert!(search.message.contains("repo root mismatch"));

        let status = ok(server.call_tool("codegraph.status", &json!({"repo": repo_arg})));
        assert_eq!(status["status"].as_str(), Some("db_problem"));
        assert_eq!(status["problem"].as_str(), Some("repo_root_mismatch"));
        assert_eq!(status["db_problem"].as_str(), Some("repo_root_mismatch"));
        assert_eq!(
            status["db_lifecycle_read"]["db_problem_kind"].as_str(),
            Some("repo_root_mismatch")
        );
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
    fn mcp_surface_outputs_validation_findings_or_not_applicable() {
        let repo = fixture_repo();
        let db_path = repo.join("external-db").join("mcp-validation.sqlite");
        fs::create_dir_all(db_path.parent().expect("db parent")).expect("create db parent");
        let server = McpServer::new(
            McpServerConfig::for_repo(&repo)
                .with_db_path(&db_path)
                .without_trace(),
        );
        ok(server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&repo), "db_path": path_string(&db_path)}),
        ));
        fs::write(
            repo.join("src").join("auth.ts"),
            "export function login(email: string) {\n  return email.trim();\n}\n",
        )
        .expect("modify fixture source");

        let result = ok(server.call_tool(
            "codegraph.update_changed_files",
            &json!({
                "repo": path_string(&repo),
                "db_path": path_string(&db_path),
                "files": ["src/auth.ts"]
            }),
        ));

        assert_eq!(result["status"].as_str(), Some("updated"));
        assert_eq!(
            result["validation_findings_status"].as_str(),
            Some("not_applicable")
        );
        assert_eq!(
            result["canonical_validation_surface"].as_str(),
            Some("codegraph-mcp agent-use watch --repo <repo> --once --changed <path> --json")
        );
        assert_eq!(result["hard_interrupt_available"].as_bool(), Some(false));
        assert_eq!(
            result["hard_interrupt_not_implemented"].as_bool(),
            Some(true)
        );
        assert!(
            !repo.join(".codegraph").exists(),
            "MCP validation mapping test must not mutate normal .codegraph"
        );

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn mcp_surface_outputs_hard_interrupt_or_not_applicable() {
        mcp_surface_outputs_validation_findings_or_not_applicable();
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
    fn context_pack_mcp_vector_missing_index_is_diagnostic() {
        let repo = fixture_repo();
        let db_path = repo.join("external-db").join("mcp-vector.sqlite");
        let missing_index = repo.join("external-db").join("missing-vector-index.json");
        let server = McpServer::new(
            McpServerConfig::for_repo(&repo)
                .with_db_path(&db_path)
                .without_trace(),
        );
        ok(server.call_tool(
            "codegraph.index_repo",
            &json!({"repo": path_string(&repo), "db_path": path_string(&db_path)}),
        ));
        assert!(
            !repo.join(".codegraph").exists(),
            "MCP vector diagnostic test must use explicit external DB"
        );

        let result = ok(server.call_tool(
            "codegraph.context_pack",
            &json!({
                "repo": path_string(&repo),
                "db_path": path_string(&db_path),
                "task": "Change login email handling",
                "enable_vector_candidates": true,
                "vector_index": path_string(&missing_index)
            }),
        ));

        assert_eq!(result["status"].as_str(), Some("ok"));
        assert_eq!(
            result["vector_candidate_diagnostics"]["vector_index_status"].as_str(),
            Some("missing")
        );
        assert_eq!(
            result["vector_candidate_diagnostics"]["vector_candidate_count"].as_u64(),
            Some(0)
        );
        assert_eq!(result["graph_db_status"].as_str(), Some("ready"));
        assert_eq!(result["vector_runtime_status"].as_str(), Some("missing"));
        assert_eq!(result["graph_proof_available"].as_bool(), Some(true));
        assert!(result["active_candidate_sources"]
            .as_array()
            .expect("candidate sources")
            .iter()
            .all(|source| source.as_str() != Some("vector_semantic")));

        let explain = ok(server.call_tool(
            "codegraph.context_pack",
            &json!({
                "repo": path_string(&repo),
                "db_path": path_string(&db_path),
                "task": "Change login email handling",
                "response_mode": "explain",
                "enable_vector_candidates": true,
                "vector_index": path_string(&missing_index)
            }),
        ));
        assert_eq!(
            explain["packet"]["metadata"]["vector_candidate_trace"]["vector_index_status"].as_str(),
            Some("missing")
        );
        assert_eq!(explain["vector_runtime_status"].as_str(), Some("missing"));
        assert!(explain["staged_availability"].is_object());

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
            &json!({"repo": path_string(&repo), "task": "Change login", "seeds": [seed.clone()], "response_mode": "explain"}),
        ));

        assert_eq!(mcp["status"].as_str(), Some("ok"));
        assert_eq!(mcp["response_mode"].as_str(), Some("explain"));
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

    #[test]
    fn context_pack_mcp_default_is_compact_and_bounded() {
        let (server, repo, login, _sanitize) = indexed_server();

        let compact = ok(server.call_tool(
            "codegraph.context_pack",
            &json!({"repo": path_string(&repo), "task": "Change login", "seeds": [login], "limit": 1}),
        ));

        assert_eq!(compact["status"].as_str(), Some("ok"));
        assert_eq!(compact["schema_version"].as_u64(), Some(1));
        assert_eq!(compact["command"].as_str(), Some("codegraph.context_pack"));
        assert_eq!(compact["response_mode"].as_str(), Some("compact"));
        assert!(compact["packet"].is_null(), "{compact}");
        assert!(compact["funnel_trace"].is_null(), "{compact}");
        assert_eq!(compact["limit"].as_u64(), Some(1));
        assert!(compact["omitted_count"].as_u64().is_some());
        assert!(compact["claimable"].as_bool().is_some());
        assert!(compact["diagnostic_only"].as_bool().is_some());
        assert!(compact["paths"].as_array().expect("paths").len() <= 1);
        assert!(compact["snippets"].as_array().expect("snippets").len() <= 1);
        assert_eq!(
            compact["read_path_metrics"]["full_scan_count"].as_u64(),
            Some(0)
        );
        assert_eq!(
            compact["read_path_metrics"]["entity_edge_million_row_load"].as_bool(),
            Some(false)
        );
        assert_eq!(
            compact["read_path_metrics"]["limits_apply_before_hydration"].as_bool(),
            Some(true)
        );

        let encoded = serde_json::to_vec(&compact).expect("compact json");
        assert!(
            encoded.len() < 16 * 1024,
            "compact MCP output too large: {}",
            encoded.len()
        );

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn context_pack_mcp_nuance_rescue_is_explicit_and_candidate_only() {
        let (server, repo, _login, _sanitize) = indexed_server();
        let stage0_candidates = (0..48)
            .map(|index| format!("quasarnettle nuance support candidate {index}"))
            .collect::<Vec<_>>();

        let compact = ok(server.call_tool(
            "codegraph.context_pack",
            &json!({
                "repo": path_string(&repo),
                "task": "Investigate quasarnettle nuance support",
                "stage0_candidates": stage0_candidates,
                "enable_nuance_rescue_candidates": true,
                "limit": 2
            }),
        ));

        assert_eq!(compact["status"].as_str(), Some("ok"));
        assert_eq!(
            compact["nuance_rescue_diagnostics"]["nuance_rescue_enabled"].as_bool(),
            Some(true)
        );
        assert!(
            compact["nuance_rescue_diagnostics"]["rescue_candidate_count"]
                .as_u64()
                .unwrap_or(0)
                > 0,
            "{compact}"
        );
        assert!(compact["packet"].is_null(), "{compact}");
        assert!(compact["funnel_trace"].is_null(), "{compact}");

        let explain = ok(server.call_tool(
            "codegraph.context_pack",
            &json!({
                "repo": path_string(&repo),
                "task": "Investigate quasarnettle nuance support",
                "stage0_candidates": stage0_candidates,
                "enable_nuance_rescue_candidates": true,
                "response_mode": "explain"
            }),
        ));
        assert_eq!(
            explain["nuance_rescue_diagnostics"]["rescue_candidate_count"]
                .as_u64()
                .unwrap_or(0)
                > 0,
            true
        );
        let nuance_stage = explain["funnel_trace"]
            .as_array()
            .expect("funnel trace")
            .iter()
            .find(|stage| stage["stage"].as_str() == Some("stage1_nuance_rescue"))
            .expect("nuance stage");
        assert!(nuance_stage["notes"]
            .as_array()
            .expect("notes")
            .iter()
            .any(|note| note
                .as_str()
                .is_some_and(|note| note.contains("candidate recall only"))));
        assert!(explain["nuance_rescue_diagnostics"]["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .all(|candidate| candidate["graph_proof"].as_bool() == Some(false)));

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
