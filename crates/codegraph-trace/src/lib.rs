//! Durable JSONL tracing for CodeGraph MCP and agent-quality benchmarks.
//!
//! The trace layer is intentionally small and filesystem-backed so the CLI,
//! MCP server, and future benchmark harnesses can all produce the same event
//! stream without depending on a service process.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeSet,
    error::Error,
    fmt, fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const TRACE_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_TRACE_INLINE_BYTE_CAP: usize = 64 * 1024;
pub const DEFAULT_TRACE_ROOT: &str = "target/codegraph-agent-runs";

#[derive(Debug)]
pub enum TraceError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Validation(String),
}

impl fmt::Display for TraceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "trace I/O error: {error}"),
            Self::Json(error) => write!(formatter, "trace JSON error: {error}"),
            Self::Validation(message) => write!(formatter, "trace validation error: {message}"),
        }
    }
}

impl Error for TraceError {}

impl From<std::io::Error> for TraceError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for TraceError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceEventType {
    RunStart,
    McpRequest,
    McpResponse,
    AgentAction,
    ShellCommand,
    FileEdit,
    TestRun,
    ContextPackUsed,
    RunEnd,
}

impl TraceEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunStart => "run_start",
            Self::McpRequest => "mcp_request",
            Self::McpResponse => "mcp_response",
            Self::AgentAction => "agent_action",
            Self::ShellCommand => "shell_command",
            Self::FileEdit => "file_edit",
            Self::TestRun => "test_run",
            Self::ContextPackUsed => "context_pack_used",
            Self::RunEnd => "run_end",
        }
    }
}

impl FromStr for TraceEventType {
    type Err = TraceError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().replace('-', "_").to_ascii_lowercase().as_str() {
            "run_start" => Ok(Self::RunStart),
            "mcp_request" => Ok(Self::McpRequest),
            "mcp_response" => Ok(Self::McpResponse),
            "agent_action" => Ok(Self::AgentAction),
            "shell_command" => Ok(Self::ShellCommand),
            "file_edit" => Ok(Self::FileEdit),
            "test_run" => Ok(Self::TestRun),
            "context_pack_used" => Ok(Self::ContextPackUsed),
            "run_end" => Ok(Self::RunEnd),
            other => Err(TraceError::Validation(format!(
                "unknown trace event type: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceArtifactRecord {
    pub artifact_path: String,
    pub byte_len: usize,
    pub hash: String,
    pub truncated: bool,
    pub redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceEvent {
    pub schema_version: u32,
    pub event_type: TraceEventType,
    pub run_id: String,
    pub trace_id: String,
    pub task_id: String,
    pub timestamp_unix_ms: u64,
    pub repo_id: String,
    pub repo_root: String,
    pub actor: String,
    pub action_kind: String,
    pub tool: String,
    pub status: String,
    pub result_status: String,
    pub latency_ms: u128,
    pub token_estimate: Value,
    pub evidence_refs: Value,
    pub edited_files: Value,
    pub test_command: String,
    pub test_status: String,
    pub input_hash: String,
    pub output_hash: String,
    pub truncated: bool,
    pub artifact_path: Option<String>,
    pub error: Option<String>,
    pub input_preview: Option<Value>,
    pub output_preview: Option<Value>,
    pub artifacts: Vec<TraceArtifactRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceConfig {
    pub run_id: String,
    pub task_id: String,
    pub repo_root: PathBuf,
    pub repo_id: String,
    pub trace_root: PathBuf,
    pub max_inline_bytes: usize,
}

impl TraceConfig {
    pub fn for_repo(repo_root: impl Into<PathBuf>) -> Self {
        let repo_root = repo_root.into();
        let run_id = std::env::var("CODEGRAPH_TRACE_RUN_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(default_run_id);
        let task_id = std::env::var("CODEGRAPH_TRACE_TASK_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "unknown".to_string());
        let trace_root = std::env::var_os("CODEGRAPH_TRACE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_TRACE_ROOT));
        let repo_id = std::env::var("CODEGRAPH_TRACE_REPO_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| repo_id_for_path(&repo_root));
        Self {
            run_id,
            task_id,
            repo_id,
            repo_root,
            trace_root,
            max_inline_bytes: DEFAULT_TRACE_INLINE_BYTE_CAP,
        }
    }

    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = run_id.into();
        self
    }

    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = task_id.into();
        self
    }

    pub fn with_repo_id(mut self, repo_id: impl Into<String>) -> Self {
        self.repo_id = repo_id.into();
        self
    }

    pub fn with_trace_root(mut self, trace_root: impl Into<PathBuf>) -> Self {
        self.trace_root = trace_root.into();
        self
    }

    pub fn with_max_inline_bytes(mut self, max_inline_bytes: usize) -> Self {
        self.max_inline_bytes = max_inline_bytes;
        self
    }
}

#[derive(Debug, Clone)]
pub struct TraceLogger {
    config: TraceConfig,
    events_path: PathBuf,
    artifacts_dir: PathBuf,
}

impl TraceLogger {
    pub fn new(config: TraceConfig) -> Result<Self, TraceError> {
        let run_dir = config.trace_root.join(&config.run_id);
        let artifacts_dir = run_dir.join("artifacts");
        fs::create_dir_all(&artifacts_dir)?;
        Ok(Self {
            events_path: run_dir.join("events.jsonl"),
            artifacts_dir,
            config,
        })
    }

    pub fn events_path(&self) -> &Path {
        &self.events_path
    }

    pub fn artifacts_dir(&self) -> &Path {
        &self.artifacts_dir
    }

    pub fn run_id(&self) -> &str {
        &self.config.run_id
    }

    pub fn emit(&self, request: TraceEventRequest<'_>) -> Result<TraceEvent, TraceError> {
        let input =
            self.materialize_payload("input", request.trace_id, request.event_type, request.input)?;
        let output = self.materialize_payload(
            "output",
            request.trace_id,
            request.event_type,
            request.output,
        )?;
        let mut artifacts = Vec::new();
        if let Some(artifact) = input.artifact {
            artifacts.push(artifact);
        }
        if let Some(artifact) = output.artifact {
            artifacts.push(artifact);
        }
        let artifact_path = artifacts
            .last()
            .map(|artifact| artifact.artifact_path.clone());
        let event = TraceEvent {
            schema_version: TRACE_SCHEMA_VERSION,
            event_type: request.event_type,
            run_id: self.config.run_id.clone(),
            trace_id: request.trace_id.to_string(),
            task_id: self.config.task_id.clone(),
            timestamp_unix_ms: unix_time_ms(),
            repo_id: self.config.repo_id.clone(),
            repo_root: path_string(&self.config.repo_root),
            actor: request
                .actor
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| default_actor(request.event_type).to_string()),
            action_kind: request
                .action_kind
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| request.event_type.as_str().to_string()),
            tool: request.tool.to_string(),
            status: request.status.to_string(),
            result_status: request
                .result_status
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| request.status.to_string()),
            latency_ms: request.latency_ms,
            token_estimate: request
                .token_estimate
                .cloned()
                .unwrap_or_else(|| infer_token_estimate(request.input, request.output)),
            evidence_refs: request
                .evidence_refs
                .cloned()
                .unwrap_or_else(|| infer_evidence_refs(request.input, request.output)),
            edited_files: request
                .edited_files
                .cloned()
                .unwrap_or_else(|| infer_edited_files(request.input, request.output)),
            test_command: request
                .test_command
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| infer_test_command(request.input, request.output)),
            test_status: request
                .test_status
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| {
                    infer_test_status(request.event_type, request.status, request.output)
                }),
            input_hash: input.hash,
            output_hash: output.hash,
            truncated: input.truncated || output.truncated,
            artifact_path,
            error: request.error.map(ToOwned::to_owned),
            input_preview: input.preview,
            output_preview: output.preview,
            artifacts,
        };
        self.append(&event)?;
        Ok(event)
    }

    pub fn run_start(&self) -> Result<TraceEvent, TraceError> {
        self.emit(TraceEventRequest::new(
            TraceEventType::RunStart,
            "run",
            "ok",
            "run_start",
        ))
    }

    pub fn run_end(&self, status: &str, error: Option<&str>) -> Result<TraceEvent, TraceError> {
        self.emit(
            TraceEventRequest::new(TraceEventType::RunEnd, "run", status, "run_end")
                .with_error(error),
        )
    }

    pub fn mcp_request(
        &self,
        trace_id: &str,
        tool: &str,
        input: &Value,
    ) -> Result<TraceEvent, TraceError> {
        self.emit(
            TraceEventRequest::new(TraceEventType::McpRequest, trace_id, "ok", tool)
                .with_input(input),
        )
    }

    pub fn mcp_response(
        &self,
        trace_id: &str,
        tool: &str,
        status: &str,
        latency_ms: u128,
        output: &Value,
        error: Option<&str>,
    ) -> Result<TraceEvent, TraceError> {
        self.emit(
            TraceEventRequest::new(TraceEventType::McpResponse, trace_id, status, tool)
                .with_latency_ms(latency_ms)
                .with_output(output)
                .with_error(error),
        )
    }

    pub fn agent_action(
        &self,
        trace_id: &str,
        tool: &str,
        status: &str,
        input: Option<&Value>,
        output: Option<&Value>,
        error: Option<&str>,
    ) -> Result<TraceEvent, TraceError> {
        self.emit(
            TraceEventRequest::new(TraceEventType::AgentAction, trace_id, status, tool)
                .with_optional_input(input)
                .with_optional_output(output)
                .with_error(error),
        )
    }

    pub fn shell_command(
        &self,
        trace_id: &str,
        status: &str,
        input: &Value,
        output: &Value,
        latency_ms: u128,
        error: Option<&str>,
    ) -> Result<TraceEvent, TraceError> {
        self.emit(
            TraceEventRequest::new(
                TraceEventType::ShellCommand,
                trace_id,
                status,
                "shell_command",
            )
            .with_input(input)
            .with_output(output)
            .with_latency_ms(latency_ms)
            .with_error(error),
        )
    }

    pub fn file_edit(
        &self,
        trace_id: &str,
        status: &str,
        input: &Value,
        output: &Value,
        error: Option<&str>,
    ) -> Result<TraceEvent, TraceError> {
        self.emit(
            TraceEventRequest::new(TraceEventType::FileEdit, trace_id, status, "file_edit")
                .with_input(input)
                .with_output(output)
                .with_error(error),
        )
    }

    pub fn test_run(
        &self,
        trace_id: &str,
        status: &str,
        input: &Value,
        output: &Value,
        latency_ms: u128,
        error: Option<&str>,
    ) -> Result<TraceEvent, TraceError> {
        self.emit(
            TraceEventRequest::new(TraceEventType::TestRun, trace_id, status, "test_run")
                .with_input(input)
                .with_output(output)
                .with_latency_ms(latency_ms)
                .with_error(error),
        )
    }

    pub fn context_pack_used(
        &self,
        trace_id: &str,
        status: &str,
        input: &Value,
        output: &Value,
        latency_ms: u128,
        error: Option<&str>,
    ) -> Result<TraceEvent, TraceError> {
        self.emit(
            TraceEventRequest::new(
                TraceEventType::ContextPackUsed,
                trace_id,
                status,
                "codegraph.context_pack",
            )
            .with_input(input)
            .with_output(output)
            .with_latency_ms(latency_ms)
            .with_error(error),
        )
    }

    fn append(&self, event: &TraceEvent) -> Result<(), TraceError> {
        if let Some(parent) = self.events_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.events_path)?;
        serde_json::to_writer(&mut file, event)?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }

    fn materialize_payload(
        &self,
        label: &str,
        trace_id: &str,
        event_type: TraceEventType,
        payload: Option<&Value>,
    ) -> Result<MaterializedPayload, TraceError> {
        let Some(payload) = payload else {
            return Ok(MaterializedPayload {
                hash: stable_hash_bytes(b""),
                truncated: false,
                preview: None,
                artifact: None,
            });
        };
        let encoded = serde_json::to_vec(payload)?;
        let hash = stable_hash_bytes(&encoded);
        if encoded.len() <= self.config.max_inline_bytes {
            return Ok(MaterializedPayload {
                hash,
                truncated: false,
                preview: Some(payload.clone()),
                artifact: None,
            });
        }

        let file_name = format!(
            "{}-{}-{}-{}.json",
            safe_path_part(trace_id),
            event_type.as_str(),
            label,
            hash.trim_start_matches("fnv128:")
        );
        let artifact_path = self.artifacts_dir.join(file_name);
        fs::write(&artifact_path, &encoded)?;
        Ok(MaterializedPayload {
            hash: hash.clone(),
            truncated: true,
            preview: Some(json!({
                "truncated": true,
                "byte_len": encoded.len(),
                "hash": hash,
                "artifact_path": path_string(&artifact_path),
            })),
            artifact: Some(TraceArtifactRecord {
                artifact_path: path_string(&artifact_path),
                byte_len: encoded.len(),
                hash,
                truncated: true,
                redacted: false,
            }),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TraceEventRequest<'a> {
    pub event_type: TraceEventType,
    pub trace_id: &'a str,
    pub tool: &'a str,
    pub status: &'a str,
    pub actor: Option<&'a str>,
    pub action_kind: Option<&'a str>,
    pub result_status: Option<&'a str>,
    pub latency_ms: u128,
    pub token_estimate: Option<&'a Value>,
    pub evidence_refs: Option<&'a Value>,
    pub edited_files: Option<&'a Value>,
    pub test_command: Option<&'a str>,
    pub test_status: Option<&'a str>,
    pub input: Option<&'a Value>,
    pub output: Option<&'a Value>,
    pub error: Option<&'a str>,
}

impl<'a> TraceEventRequest<'a> {
    pub const fn new(
        event_type: TraceEventType,
        trace_id: &'a str,
        status: &'a str,
        tool: &'a str,
    ) -> Self {
        Self {
            event_type,
            trace_id,
            tool,
            status,
            actor: None,
            action_kind: None,
            result_status: None,
            latency_ms: 0,
            token_estimate: None,
            evidence_refs: None,
            edited_files: None,
            test_command: None,
            test_status: None,
            input: None,
            output: None,
            error: None,
        }
    }

    pub const fn with_latency_ms(mut self, latency_ms: u128) -> Self {
        self.latency_ms = latency_ms;
        self
    }

    pub const fn with_actor(mut self, actor: &'a str) -> Self {
        self.actor = Some(actor);
        self
    }

    pub const fn with_action_kind(mut self, action_kind: &'a str) -> Self {
        self.action_kind = Some(action_kind);
        self
    }

    pub const fn with_result_status(mut self, result_status: &'a str) -> Self {
        self.result_status = Some(result_status);
        self
    }

    pub const fn with_token_estimate(mut self, token_estimate: &'a Value) -> Self {
        self.token_estimate = Some(token_estimate);
        self
    }

    pub const fn with_evidence_refs(mut self, evidence_refs: &'a Value) -> Self {
        self.evidence_refs = Some(evidence_refs);
        self
    }

    pub const fn with_edited_files(mut self, edited_files: &'a Value) -> Self {
        self.edited_files = Some(edited_files);
        self
    }

    pub const fn with_test_command(mut self, test_command: &'a str) -> Self {
        self.test_command = Some(test_command);
        self
    }

    pub const fn with_test_status(mut self, test_status: &'a str) -> Self {
        self.test_status = Some(test_status);
        self
    }

    pub const fn with_input(mut self, input: &'a Value) -> Self {
        self.input = Some(input);
        self
    }

    pub const fn with_optional_input(mut self, input: Option<&'a Value>) -> Self {
        self.input = input;
        self
    }

    pub const fn with_output(mut self, output: &'a Value) -> Self {
        self.output = Some(output);
        self
    }

    pub const fn with_optional_output(mut self, output: Option<&'a Value>) -> Self {
        self.output = output;
        self
    }

    pub const fn with_error(mut self, error: Option<&'a str>) -> Self {
        self.error = error;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceAppendEvent {
    pub event_type: TraceEventType,
    pub trace_id: String,
    pub tool: String,
    pub status: String,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub action_kind: Option<String>,
    #[serde(default)]
    pub result_status: Option<String>,
    #[serde(default)]
    pub latency_ms: u128,
    #[serde(default)]
    pub token_estimate: Option<Value>,
    #[serde(default)]
    pub evidence_refs: Option<Value>,
    #[serde(default)]
    pub edited_files: Option<Value>,
    #[serde(default)]
    pub test_command: Option<String>,
    #[serde(default)]
    pub test_status: Option<String>,
    #[serde(default)]
    pub input: Option<Value>,
    #[serde(default)]
    pub output: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

impl TraceAppendEvent {
    pub fn request(&self) -> TraceEventRequest<'_> {
        let mut request =
            TraceEventRequest::new(self.event_type, &self.trace_id, &self.status, &self.tool)
                .with_latency_ms(self.latency_ms)
                .with_optional_input(self.input.as_ref())
                .with_optional_output(self.output.as_ref())
                .with_error(self.error.as_deref());
        if let Some(actor) = self.actor.as_deref() {
            request = request.with_actor(actor);
        }
        if let Some(action_kind) = self.action_kind.as_deref() {
            request = request.with_action_kind(action_kind);
        }
        if let Some(result_status) = self.result_status.as_deref() {
            request = request.with_result_status(result_status);
        }
        if let Some(token_estimate) = self.token_estimate.as_ref() {
            request = request.with_token_estimate(token_estimate);
        }
        if let Some(evidence_refs) = self.evidence_refs.as_ref() {
            request = request.with_evidence_refs(evidence_refs);
        }
        if let Some(edited_files) = self.edited_files.as_ref() {
            request = request.with_edited_files(edited_files);
        }
        if let Some(test_command) = self.test_command.as_deref() {
            request = request.with_test_command(test_command);
        }
        if let Some(test_status) = self.test_status.as_deref() {
            request = request.with_test_status(test_status);
        }
        request
    }
}

pub fn append_trace_event(
    config: TraceConfig,
    event: TraceAppendEvent,
) -> Result<TraceEvent, TraceError> {
    TraceLogger::new(config)?.emit(event.request())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceReplayIssue {
    pub line: usize,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceReplayReport {
    pub schema_version: u32,
    pub run_status: String,
    pub events_total: usize,
    pub valid_events: usize,
    pub malformed_lines: Vec<TraceReplayIssue>,
    pub missing_required_fields: Vec<TraceReplayIssue>,
    pub mcp_calls_made: Vec<String>,
    pub artifacts_referenced: Vec<String>,
    pub files_edited: Vec<String>,
    pub tests_run: Vec<String>,
    pub context_evidence_used: Vec<String>,
    pub context_packets_used: usize,
    pub run_ids: Vec<String>,
    pub task_ids: Vec<String>,
}

pub fn replay_trace_file(events_path: impl AsRef<Path>) -> Result<TraceReplayReport, TraceError> {
    let file = fs::File::open(events_path)?;
    let reader = BufReader::new(file);
    let mut report = TraceReplayReport {
        schema_version: TRACE_SCHEMA_VERSION,
        run_status: "unknown".to_string(),
        events_total: 0,
        valid_events: 0,
        malformed_lines: Vec::new(),
        missing_required_fields: Vec::new(),
        mcp_calls_made: Vec::new(),
        artifacts_referenced: Vec::new(),
        files_edited: Vec::new(),
        tests_run: Vec::new(),
        context_evidence_used: Vec::new(),
        context_packets_used: 0,
        run_ids: Vec::new(),
        task_ids: Vec::new(),
    };
    let mut mcp_calls = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    let mut edited_files = BTreeSet::new();
    let mut tests = BTreeSet::new();
    let mut evidence = BTreeSet::new();
    let mut run_ids = BTreeSet::new();
    let mut task_ids = BTreeSet::new();

    for (index, line) in reader.lines().enumerate() {
        let line_number = index + 1;
        report.events_total += 1;
        let line = line?;
        let value = match serde_json::from_str::<Value>(&line) {
            Ok(value) => value,
            Err(error) => {
                report.malformed_lines.push(TraceReplayIssue {
                    line: line_number,
                    kind: "malformed_json".to_string(),
                    message: error.to_string(),
                });
                continue;
            }
        };
        let missing = missing_required_event_fields(&value);
        if !missing.is_empty() {
            report.missing_required_fields.push(TraceReplayIssue {
                line: line_number,
                kind: "missing_required_fields".to_string(),
                message: missing.join(","),
            });
        }
        report.valid_events += 1;
        let event_type = value
            .get("event_type")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let tool = value
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if event_type == TraceEventType::McpRequest.as_str() {
            mcp_calls.insert(tool.to_string());
        }
        if event_type == TraceEventType::ContextPackUsed.as_str() {
            report.context_packets_used += 1;
        }
        if event_type == TraceEventType::RunEnd.as_str() {
            report.run_status = value
                .get("result_status")
                .or_else(|| value.get("status"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
        }
        collect_optional_string(&value, "artifact_path", &mut artifacts);
        collect_string_array(&value["artifacts"], "artifact_path", &mut artifacts);
        collect_replay_value(&value["edited_files"], &mut edited_files);
        collect_replay_value(&value["evidence_refs"], &mut evidence);
        collect_optional_string(&value, "test_command", &mut tests);
        collect_optional_string(&value, "run_id", &mut run_ids);
        collect_optional_string(&value, "task_id", &mut task_ids);
    }

    report.mcp_calls_made = mcp_calls.into_iter().collect();
    report.artifacts_referenced = artifacts.into_iter().collect();
    report.files_edited = edited_files.into_iter().collect();
    report.tests_run = tests
        .into_iter()
        .filter(|value| value != "unknown")
        .collect();
    report.context_evidence_used = evidence.into_iter().collect();
    report.run_ids = run_ids.into_iter().collect();
    report.task_ids = task_ids.into_iter().collect();
    Ok(report)
}

#[derive(Debug)]
struct MaterializedPayload {
    hash: String,
    truncated: bool,
    preview: Option<Value>,
    artifact: Option<TraceArtifactRecord>,
}

fn default_actor(event_type: TraceEventType) -> &'static str {
    match event_type {
        TraceEventType::RunStart | TraceEventType::RunEnd => "system",
        TraceEventType::McpRequest
        | TraceEventType::McpResponse
        | TraceEventType::ContextPackUsed => "mcp",
        TraceEventType::AgentAction
        | TraceEventType::ShellCommand
        | TraceEventType::FileEdit
        | TraceEventType::TestRun => "agent",
    }
}

fn infer_token_estimate(input: Option<&Value>, output: Option<&Value>) -> Value {
    for value in [output, input].into_iter().flatten() {
        if let Some(token_estimate) = value.get("token_estimate") {
            return token_estimate.clone();
        }
        if let Some(token_estimate) = value.pointer("/packet/metadata/estimated_tokens") {
            return token_estimate.clone();
        }
        if let Some(token_estimate) = value.pointer("/metadata/estimated_tokens") {
            return token_estimate.clone();
        }
    }
    json!("unknown")
}

fn infer_evidence_refs(input: Option<&Value>, output: Option<&Value>) -> Value {
    let mut refs = BTreeSet::new();
    for value in [input, output].into_iter().flatten() {
        collect_evidence_refs(value, &mut refs);
    }
    if refs.is_empty() {
        json!("unknown")
    } else {
        json!(refs.into_iter().collect::<Vec<_>>())
    }
}

fn collect_evidence_refs(value: &Value, refs: &mut BTreeSet<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_evidence_refs(item, refs);
            }
        }
        Value::Object(object) => {
            for key in [
                "evidence_ref",
                "source_span",
                "artifact_path",
                "context_pack_id",
                "id",
            ] {
                if let Some(text) = object.get(key).and_then(Value::as_str) {
                    if is_evidence_like(text) {
                        refs.insert(text.to_string());
                    }
                }
            }
            if let Some(resource_links) = object.get("resource_links").and_then(Value::as_object) {
                for value in resource_links.values().filter_map(Value::as_str) {
                    refs.insert(value.to_string());
                }
            }
            if let Some(span) = object.get("source_span") {
                if span.is_object() {
                    refs.insert(span.to_string());
                }
            }
            for value in object.values() {
                collect_evidence_refs(value, refs);
            }
        }
        Value::String(text) if is_evidence_like(text) => {
            refs.insert(text.to_string());
        }
        _ => {}
    }
}

fn is_evidence_like(text: &str) -> bool {
    text.starts_with("codegraph://")
        || text.starts_with("path://")
        || text.starts_with("edge://")
        || text.starts_with("repo://")
        || text.contains("source_span")
}

fn infer_edited_files(input: Option<&Value>, output: Option<&Value>) -> Value {
    let mut files = BTreeSet::new();
    for value in [input, output].into_iter().flatten() {
        collect_named_paths(
            value,
            &["edited_files", "files", "changed_files"],
            &mut files,
        );
    }
    if files.is_empty() {
        json!("unknown")
    } else {
        json!(files.into_iter().collect::<Vec<_>>())
    }
}

fn collect_named_paths(value: &Value, keys: &[&str], files: &mut BTreeSet<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_named_paths(item, keys, files);
            }
        }
        Value::Object(object) => {
            for key in keys {
                match object.get(*key) {
                    Some(Value::Array(items)) => {
                        for item in items {
                            if let Some(path) = item.as_str() {
                                files.insert(path.to_string());
                            }
                        }
                    }
                    Some(Value::String(path)) if path != "unknown" => {
                        files.insert(path.to_string());
                    }
                    _ => {}
                }
            }
            for key in ["file", "path", "repo_relative_path"] {
                if let Some(path) = object.get(key).and_then(Value::as_str) {
                    if looks_like_file_path(path) {
                        files.insert(path.to_string());
                    }
                }
            }
        }
        _ => {}
    }
}

fn looks_like_file_path(path: &str) -> bool {
    path.contains('/') || path.contains('\\') || path.contains('.')
}

fn infer_test_command(input: Option<&Value>, output: Option<&Value>) -> String {
    for value in [input, output].into_iter().flatten() {
        for pointer in ["/test_command", "/command", "/cmd"] {
            if let Some(command) = value.pointer(pointer).and_then(Value::as_str) {
                return command.to_string();
            }
        }
    }
    "unknown".to_string()
}

fn infer_test_status(event_type: TraceEventType, status: &str, output: Option<&Value>) -> String {
    if let Some(output) = output {
        if let Some(test_status) = output.get("test_status").and_then(Value::as_str) {
            return test_status.to_string();
        }
    }
    if event_type == TraceEventType::TestRun {
        status.to_string()
    } else {
        "unknown".to_string()
    }
}

fn repo_id_for_path(path: &Path) -> String {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    stable_hash_bytes(path_string(&canonical).as_bytes())
}

fn missing_required_event_fields(value: &Value) -> Vec<&'static str> {
    let required = [
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
    ];
    required
        .into_iter()
        .filter(|field| value.get(*field).is_none())
        .collect()
}

fn collect_optional_string(value: &Value, key: &str, output: &mut BTreeSet<String>) {
    if let Some(text) = value.get(key).and_then(Value::as_str) {
        output.insert(text.to_string());
    }
}

fn collect_string_array(value: &Value, key: &str, output: &mut BTreeSet<String>) {
    if let Some(items) = value.as_array() {
        for item in items {
            if let Some(text) = item.get(key).and_then(Value::as_str) {
                output.insert(text.to_string());
            }
        }
    }
}

fn collect_replay_value(value: &Value, output: &mut BTreeSet<String>) {
    match value {
        Value::String(text) if text != "unknown" => {
            output.insert(text.to_string());
        }
        Value::Array(items) => {
            for item in items {
                if let Some(text) = item.as_str() {
                    if text != "unknown" {
                        output.insert(text.to_string());
                    }
                }
            }
        }
        _ => {}
    }
}

pub fn default_run_id() -> String {
    format!("run-{}-{}", unix_time_ms(), process::id())
}

pub fn stable_hash_value(value: &Value) -> Result<String, TraceError> {
    Ok(stable_hash_bytes(&serde_json::to_vec(value)?))
}

pub fn stable_hash_bytes(bytes: &[u8]) -> String {
    let high = fnv64_with_seed(bytes, 0xcbf29ce484222325);
    let low = fnv64_with_seed(bytes, 0x9e3779b185ebca87);
    format!("fnv128:{high:016x}{low:016x}")
}

fn fnv64_with_seed(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = seed;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn unix_time_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(u128::from(u64::MAX)) as u64,
        Err(_) => 0,
    }
}

fn safe_path_part(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "codegraph-trace-test-{}-{name}-{}",
            process::id(),
            unix_time_ms()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale temp");
        }
        fs::create_dir_all(&root).expect("create temp");
        root
    }

    #[test]
    fn trace_events_are_jsonl_with_required_stable_fields() {
        let root = temp_dir("required-fields");
        let logger = TraceLogger::new(
            TraceConfig::for_repo(&root)
                .with_run_id("run-test")
                .with_task_id("task-test")
                .with_trace_root(root.join("target").join("codegraph-agent-runs")),
        )
        .expect("logger");

        logger.run_start().expect("run start");
        logger
            .mcp_request("trace-1", "codegraph.status", &json!({"repo": "fixture"}))
            .expect("mcp request");
        logger
            .mcp_response(
                "trace-1",
                "codegraph.status",
                "ok",
                3,
                &json!({"status": "ok"}),
                None,
            )
            .expect("mcp response");
        logger.run_end("ok", None).expect("run end");

        let contents = fs::read_to_string(logger.events_path()).expect("events");
        let events = contents
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("json line"))
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 4);
        for event in &events {
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
        assert!(events
            .iter()
            .any(|event| event["event_type"].as_str() == Some("mcp_request")));
        assert!(events
            .iter()
            .any(|event| event["event_type"].as_str() == Some("mcp_response")));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn append_helper_and_replay_report_reconstruct_agent_actions() {
        let root = temp_dir("replay-valid");
        let trace_root = root.join("target").join("codegraph-agent-runs");
        let config = TraceConfig::for_repo(&root)
            .with_run_id("run-replay")
            .with_task_id("task-replay")
            .with_trace_root(&trace_root);
        append_trace_event(
            config.clone(),
            TraceAppendEvent {
                event_type: TraceEventType::McpRequest,
                trace_id: "mcp-1".to_string(),
                tool: "codegraph.context_pack".to_string(),
                status: "ok".to_string(),
                actor: None,
                action_kind: None,
                result_status: None,
                latency_ms: 0,
                token_estimate: None,
                evidence_refs: None,
                edited_files: None,
                test_command: None,
                test_status: None,
                input: Some(json!({"task": "change login"})),
                output: None,
                error: None,
            },
        )
        .expect("append mcp request");
        append_trace_event(
            config.clone(),
            TraceAppendEvent {
                event_type: TraceEventType::ContextPackUsed,
                trace_id: "mcp-1".to_string(),
                tool: "codegraph.context_pack".to_string(),
                status: "ok".to_string(),
                actor: Some("mcp".to_string()),
                action_kind: Some("context_pack_used".to_string()),
                result_status: Some("ok".to_string()),
                latency_ms: 7,
                token_estimate: Some(json!(123)),
                evidence_refs: Some(json!(["codegraph://source-span/src/auth.ts:1-3"])),
                edited_files: None,
                test_command: None,
                test_status: None,
                input: None,
                output: Some(json!({"packet": {"metadata": {"estimated_tokens": 123}}})),
                error: None,
            },
        )
        .expect("append context");
        append_trace_event(
            config.clone(),
            TraceAppendEvent {
                event_type: TraceEventType::FileEdit,
                trace_id: "edit-1".to_string(),
                tool: "apply_patch".to_string(),
                status: "ok".to_string(),
                actor: Some("agent".to_string()),
                action_kind: Some("file_edit".to_string()),
                result_status: Some("ok".to_string()),
                latency_ms: 0,
                token_estimate: Some(json!("unknown")),
                evidence_refs: None,
                edited_files: Some(json!(["src/auth.ts"])),
                test_command: None,
                test_status: None,
                input: Some(json!({"edited_files": ["src/auth.ts"]})),
                output: None,
                error: None,
            },
        )
        .expect("append edit");
        append_trace_event(
            config.clone(),
            TraceAppendEvent {
                event_type: TraceEventType::TestRun,
                trace_id: "test-1".to_string(),
                tool: "cargo".to_string(),
                status: "passed".to_string(),
                actor: Some("agent".to_string()),
                action_kind: Some("test_run".to_string()),
                result_status: Some("passed".to_string()),
                latency_ms: 10,
                token_estimate: None,
                evidence_refs: None,
                edited_files: None,
                test_command: Some("cargo test -p codegraph-trace".to_string()),
                test_status: Some("passed".to_string()),
                input: None,
                output: None,
                error: None,
            },
        )
        .expect("append test");
        append_trace_event(
            config,
            TraceAppendEvent {
                event_type: TraceEventType::RunEnd,
                trace_id: "run".to_string(),
                tool: "run_end".to_string(),
                status: "ok".to_string(),
                actor: Some("system".to_string()),
                action_kind: Some("run_end".to_string()),
                result_status: Some("ok".to_string()),
                latency_ms: 0,
                token_estimate: None,
                evidence_refs: None,
                edited_files: None,
                test_command: None,
                test_status: None,
                input: None,
                output: None,
                error: None,
            },
        )
        .expect("append run end");

        let report =
            replay_trace_file(trace_root.join("run-replay").join("events.jsonl")).expect("replay");
        assert_eq!(report.run_status, "ok");
        assert!(report
            .mcp_calls_made
            .contains(&"codegraph.context_pack".to_string()));
        assert!(report.files_edited.contains(&"src/auth.ts".to_string()));
        assert!(report
            .tests_run
            .contains(&"cargo test -p codegraph-trace".to_string()));
        assert!(report
            .context_evidence_used
            .contains(&"codegraph://source-span/src/auth.ts:1-3".to_string()));
        assert!(report.malformed_lines.is_empty());
        assert!(report.missing_required_fields.is_empty());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn replay_reports_malformed_lines_and_missing_fields() {
        let root = temp_dir("replay-malformed");
        let events = root.join("events.jsonl");
        fs::write(
            &events,
            "{\"schema_version\":1,\"event_type\":\"run_start\"}\nnot-json\n",
        )
        .expect("write events");
        let report = replay_trace_file(&events).expect("replay");
        assert_eq!(report.events_total, 2);
        assert_eq!(report.valid_events, 1);
        assert_eq!(report.malformed_lines.len(), 1);
        assert_eq!(report.missing_required_fields.len(), 1);
        assert!(report.missing_required_fields[0].message.contains("run_id"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn large_payloads_are_capped_and_written_to_artifacts() {
        let root = temp_dir("artifact");
        let logger = TraceLogger::new(
            TraceConfig::for_repo(&root)
                .with_run_id("run-artifact")
                .with_trace_root(root.join("target").join("codegraph-agent-runs"))
                .with_max_inline_bytes(16),
        )
        .expect("logger");

        let large = json!({"data": "abcdefghijklmnopqrstuvwxyz"});
        let event = logger
            .mcp_response(
                "trace-large",
                "codegraph.context_pack",
                "ok",
                1,
                &large,
                None,
            )
            .expect("trace large");
        assert!(event.truncated);
        assert_eq!(event.artifacts.len(), 1);
        let artifact = &event.artifacts[0];
        assert!(artifact.byte_len > 16);
        assert!(artifact.truncated);
        assert!(!artifact.redacted);
        assert!(Path::new(&artifact.artifact_path).exists());

        let line = fs::read_to_string(logger.events_path()).expect("events");
        let parsed: Value = serde_json::from_str(line.trim()).expect("jsonl");
        assert_eq!(parsed["truncated"].as_bool(), Some(true));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
