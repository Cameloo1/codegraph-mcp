//! Agent-use command surface (status/index/watch/mcp-config/query plumbing)
//! and its profile/lifecycle/RTDS helpers.
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use serde_json::{json, Value};

use crate::*;

#[cfg(test)]
thread_local! {
    static CLI_WRITE_PATH_CHAOS_FAILPOINT_OVERRIDE: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

const AGENT_USE_COMPACT_GRAPH_DELTA_TOP_LIMIT: usize = 3;
// MVP3.9.5.3: the validation delta is bounded; overflow in a blocking-relevant
// category caps the packet at unknown (`graph_delta_bounded`) instead of
// retaining an unbounded number of hydrated delta entries in memory.
pub(crate) const AGENT_USE_VALIDATION_GRAPH_DELTA_MAX_ITEMS_PER_CATEGORY: usize = 10_000;
// MVP3.9.5.3: shared per-run budget for per-edge CALLS reverification across
// all validation fan-out loops; exhaustion is labeled via
// CG_MVP3_GRAPH_DELTA_BOUNDED (unknown), never silently passed.
pub(crate) const AGENT_USE_VALIDATION_MAX_EDGE_REVERIFICATIONS: usize = 10_000;
// MVP3.9.5.3: whole-pipeline validation wall budget. On breach the current
// substage finishes, remaining skippable substages are skipped, and the
// packet is capped at unknown via CG_MVP3_GRAPH_DELTA_BOUNDED — never a
// silent pass and never a silent overrun. Default sized from the measured
// release-binary cost of a real large-file edit on a ~73k-edge repo
// (~100-130 s): the wall defends against pathological hangs, and a default
// below the genuine cost of honest validation would bound every big edit.
pub(crate) const AGENT_USE_VALIDATION_DEFAULT_MAX_WALL_MS: u64 = 300_000;
pub(crate) const AGENT_USE_MAX_VALIDATION_MS_ENV: &str = "CODEGRAPH_MAX_VALIDATION_MS";
// Test/operator overrides for the validation bounds (fixtures force them low
// to prove bounded runs are labeled `unknown`, never silently passed).
pub(crate) const AGENT_USE_VALIDATION_MAX_DELTA_ITEMS_ENV: &str =
    "CODEGRAPH_VALIDATION_MAX_DELTA_ITEMS";
pub(crate) const AGENT_USE_VALIDATION_MAX_EDGE_REVERIFICATIONS_ENV: &str =
    "CODEGRAPH_VALIDATION_MAX_EDGE_REVERIFICATIONS";

pub(crate) fn agent_use_validation_graph_delta_max_items() -> usize {
    std::env::var(AGENT_USE_VALIDATION_MAX_DELTA_ITEMS_ENV)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(AGENT_USE_VALIDATION_GRAPH_DELTA_MAX_ITEMS_PER_CATEGORY)
}

pub(crate) fn agent_use_validation_max_edge_reverifications() -> usize {
    std::env::var(AGENT_USE_VALIDATION_MAX_EDGE_REVERIFICATIONS_ENV)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(AGENT_USE_VALIDATION_MAX_EDGE_REVERIFICATIONS)
}

/// Resolves the validation wall deadline: explicit flag value, then the
/// `CODEGRAPH_MAX_VALIDATION_MS` env override, then the default.
pub(crate) fn agent_use_validation_wall(max_validation_ms: Option<u64>) -> (Instant, u64) {
    let budget_ms = max_validation_ms
        .or_else(|| {
            std::env::var(AGENT_USE_MAX_VALIDATION_MS_ENV)
                .ok()
                .and_then(|value| value.trim().parse().ok())
        })
        .unwrap_or(AGENT_USE_VALIDATION_DEFAULT_MAX_WALL_MS);
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(budget_ms))
        .unwrap_or_else(|| Instant::now() + Duration::from_secs(31_536_000));
    (deadline, budget_ms)
}
const AGENT_USE_WATCH_COMPACT_MAX_OUTPUT_BYTES: usize = 256 * 1024;
const CG_MVP3_CALLS_DANGLING_TARGET: &str = "CG_MVP3_CALLS_DANGLING_TARGET";
const CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED: &str =
    "CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED";
const CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED: &str = "CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED";
const CG_MVP3_CALLS_TARGET_ROLE_MISMATCH: &str = "CG_MVP3_CALLS_TARGET_ROLE_MISMATCH";
const CG_MVP3_CALLS_MISSING_SOURCE_SPAN: &str = "CG_MVP3_CALLS_MISSING_SOURCE_SPAN";
const CG_MVP3_CALLS_DERIVED_MISSING_PROVENANCE: &str = "CG_MVP3_CALLS_DERIVED_MISSING_PROVENANCE";
const CG_MVP3_IMPORTS_DANGLING_TARGET: &str = "CG_MVP3_IMPORTS_DANGLING_TARGET";
const CG_MVP3_IMPORTS_DELETED_EXPORT_STILL_IMPORTED: &str =
    "CG_MVP3_IMPORTS_DELETED_EXPORT_STILL_IMPORTED";
const CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED: &str =
    "CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED";
const CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH: &str = "CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH";
const CG_MVP3_IMPORTS_TARGET_ROLE_MISMATCH: &str = "CG_MVP3_IMPORTS_TARGET_ROLE_MISMATCH";
const CG_MVP3_IMPORTS_MISSING_SOURCE_SPAN: &str = "CG_MVP3_IMPORTS_MISSING_SOURCE_SPAN";
const CG_MVP3_IMPORTS_DERIVED_MISSING_PROVENANCE: &str =
    "CG_MVP3_IMPORTS_DERIVED_MISSING_PROVENANCE";
const CG_MVP3_PROOF_EDGE_MISSING_SOURCE_SPAN: &str = "CG_MVP3_PROOF_EDGE_MISSING_SOURCE_SPAN";
const CG_MVP3_CLAIMABLE_ENTITY_MISSING_SOURCE_SPAN: &str =
    "CG_MVP3_CLAIMABLE_ENTITY_MISSING_SOURCE_SPAN";
const CG_MVP3_DERIVED_EDGE_MISSING_PROVENANCE: &str = "CG_MVP3_DERIVED_EDGE_MISSING_PROVENANCE";
const CG_MVP3_DB_LIFECYCLE_MISMATCH_AFTER_UPDATE: &str =
    "CG_MVP3_DB_LIFECYCLE_MISMATCH_AFTER_UPDATE";
const CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT: &str =
    "CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT";
const CG_MVP3_TEMP_DB_CLAIMABLE: &str = "CG_MVP3_TEMP_DB_CLAIMABLE";
const CG_MVP3_OLD_GOOD_DB_NOT_PRESERVED: &str = "CG_MVP3_OLD_GOOD_DB_NOT_PRESERVED";
const CG_MVP3_CORRUPT_OR_INCOMPLETE_UPDATE_TRANSACTION: &str =
    "CG_MVP3_CORRUPT_OR_INCOMPLETE_UPDATE_TRANSACTION";
const CG_MVP3_QUERY_DURING_UPDATE_UNSAFE: &str = "CG_MVP3_QUERY_DURING_UPDATE_UNSAFE";
const CG_MVP3_GRAPH_DELTA_BOUNDED: &str = "CG_MVP3_GRAPH_DELTA_BOUNDED";
const CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL: &str = "CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL";
const CG_MVP3_REF_NEW_UNRESOLVED_IMPORT: &str = "CG_MVP3_REF_NEW_UNRESOLVED_IMPORT";
const CG_MVP3_REF_EXTERNAL_OR_BUILTIN: &str = "CG_MVP3_REF_EXTERNAL_OR_BUILTIN";
const CG_MVP3_REF_DYNAMIC: &str = "CG_MVP3_REF_DYNAMIC";
// Policy opt-in (default off): promote escalated repo-local unresolved
// references from warning to blocking. Spec §1.3.4 keeps warning as the
// default ceiling — an unresolved reference is absence of a link, not
// deterministic proof of error.
pub(crate) const AGENT_USE_BLOCK_ON_UNRESOLVED_LOCAL_ENV: &str =
    "CODEGRAPH_BLOCK_ON_UNRESOLVED_LOCAL";

pub(crate) fn agent_use_block_on_unresolved_local() -> bool {
    std::env::var(AGENT_USE_BLOCK_ON_UNRESOLVED_LOCAL_ENV)
        .map(|value| matches!(value.trim(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}
const CG_MVP3_SIDECAR_CORRUPT_VS_INACCESSIBLE_MISCLASSIFIED: &str =
    "CG_MVP3_SIDECAR_CORRUPT_VS_INACCESSIBLE_MISCLASSIFIED";
const CG_MVP3_SOURCE_ROLE_TEST_EVIDENCE_IN_PRODUCTION_PROOF: &str =
    "CG_MVP3_SOURCE_ROLE_TEST_EVIDENCE_IN_PRODUCTION_PROOF";
const CG_MVP3_SOURCE_ROLE_MOCK_EVIDENCE_IN_PRODUCTION_PROOF: &str =
    "CG_MVP3_SOURCE_ROLE_MOCK_EVIDENCE_IN_PRODUCTION_PROOF";
const CG_MVP3_SOURCE_ROLE_STUB_EVIDENCE_IN_PRODUCTION_PROOF: &str =
    "CG_MVP3_SOURCE_ROLE_STUB_EVIDENCE_IN_PRODUCTION_PROOF";
const CG_MVP3_SOURCE_ROLE_INLINE_TEST_PROMOTED_TO_PRODUCTION: &str =
    "CG_MVP3_SOURCE_ROLE_INLINE_TEST_PROMOTED_TO_PRODUCTION";
const CG_MVP3_SOURCE_ROLE_GENERATED_EVIDENCE_AS_PRODUCTION_PROOF: &str =
    "CG_MVP3_SOURCE_ROLE_GENERATED_EVIDENCE_AS_PRODUCTION_PROOF";
const CG_MVP3_TESTS_DANGLING_TARGET: &str = "CG_MVP3_TESTS_DANGLING_TARGET";
const CG_MVP3_ASSERTS_DANGLING_TARGET: &str = "CG_MVP3_ASSERTS_DANGLING_TARGET";
const CG_MVP3_OPTIONAL_TEST_TARGET_MISSING: &str = "CG_MVP3_OPTIONAL_TEST_TARGET_MISSING";
const CG_MVP3_UNSUPPORTED_TEST_RELATION_UNKNOWN: &str = "CG_MVP3_UNSUPPORTED_TEST_RELATION_UNKNOWN";
const CG_MVP3_READS_DANGLING_SYMBOL: &str = "CG_MVP3_READS_DANGLING_SYMBOL";
const CG_MVP3_WRITES_DANGLING_SYMBOL: &str = "CG_MVP3_WRITES_DANGLING_SYMBOL";
const CG_MVP3_READS_WRITES_TARGET_ROLE_MISMATCH: &str = "CG_MVP3_READS_WRITES_TARGET_ROLE_MISMATCH";
const CG_MVP3_READS_WRITES_MISSING_SOURCE_SPAN: &str = "CG_MVP3_READS_WRITES_MISSING_SOURCE_SPAN";
const CG_MVP3_READS_WRITES_DERIVED_MISSING_PROVENANCE: &str =
    "CG_MVP3_READS_WRITES_DERIVED_MISSING_PROVENANCE";
const CG_MVP3_ROUTE_HANDLER_DANGLING_TARGET: &str = "CG_MVP3_ROUTE_HANDLER_DANGLING_TARGET";
const CG_MVP3_ROUTE_HANDLER_RENAMED_NOT_UPDATED: &str = "CG_MVP3_ROUTE_HANDLER_RENAMED_NOT_UPDATED";
const CG_MVP3_ROUTE_COMPUTED_UNKNOWN: &str = "CG_MVP3_ROUTE_COMPUTED_UNKNOWN";
const CG_MVP3_ROUTE_UNSUPPORTED_FRAMEWORK_UNKNOWN: &str =
    "CG_MVP3_ROUTE_UNSUPPORTED_FRAMEWORK_UNKNOWN";
const CG_MVP3_CONFIG_PACKAGE_EXACT_MISMATCH: &str = "CG_MVP3_CONFIG_PACKAGE_EXACT_MISMATCH";
const CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING: &str = "CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING";
const CG_MVP3_CONFIG_PACKAGE_UNSUPPORTED_UNKNOWN: &str =
    "CG_MVP3_CONFIG_PACKAGE_UNSUPPORTED_UNKNOWN";

pub(crate) fn run_agent_use_command(args: &[String]) -> Result<Value, String> {
    let Some(subcommand) = args.first() else {
        return Err(
            "Usage: codegraph-mcp agent-use <status|index|query|context-pack|mcp-config|watch|validate-edit> --repo <repo> --json"
                .to_string(),
        );
    };
    match subcommand.as_str() {
        "status" => run_agent_use_status_command(&args[1..]),
        "index" => run_agent_use_index_command(&args[1..]),
        "query" => run_agent_use_query_command(&args[1..]),
        "context-pack" | "context" => run_agent_use_context_pack_command(&args[1..]),
        "mcp-config" | "mcp_config" => run_agent_use_mcp_config_command(&args[1..]),
        "watch" => run_agent_use_watch_command(&args[1..]),
        "validate-edit" | "validate_edit" => run_agent_use_validate_edit_command(&args[1..]),
        other => Err(format!(
            "unknown agent-use command: {other}; expected status, index, query, context-pack, mcp-config, watch, or validate-edit"
        )),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AgentUseBasicOptions {
    repo: PathBuf,
    detail_mode: AgentUseDetailMode,
    max_output_bytes: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentUseDetailMode {
    Compact,
    Explain,
    Audit,
}

impl AgentUseDetailMode {
    pub(crate) fn preserves_full_details(self) -> bool {
        !matches!(self, AgentUseDetailMode::Compact)
    }

    fn label(self) -> &'static str {
        match self {
            AgentUseDetailMode::Compact => "compact",
            AgentUseDetailMode::Explain => "explain",
            AgentUseDetailMode::Audit => "audit",
        }
    }

    fn default_max_output_bytes(self) -> usize {
        if self.preserves_full_details() {
            DEFAULT_AGENT_USE_EXPLAIN_MAX_OUTPUT_BYTES
        } else {
            DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AgentUseIndexOptions {
    repo: PathBuf,
    passthrough_index_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentUseWatchOptions {
    repo: PathBuf,
    once: bool,
    changed_paths: Vec<PathBuf>,
    test_events: Vec<PathBuf>,
    detail_mode: AgentUseDetailMode,
    debounce: Duration,
    max_updates: Option<usize>,
    idle_timeout: Option<Duration>,
    lock_retries: usize,
    lock_retry: Duration,
    max_batch_paths: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentUseValidateEditOptions {
    repo: PathBuf,
    changed_paths: Vec<PathBuf>,
    detail_mode: AgentUseDetailMode,
    max_output_bytes: Option<usize>,
    fail_on_blocking: bool,
    task_id: Option<String>,
    edit_intent: Option<String>,
    expected_touched_files: Vec<PathBuf>,
    max_validation_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentUseForwardOptions {
    repo: PathBuf,
    forwarded_args: Vec<String>,
}

pub(crate) fn parse_agent_use_basic_args(
    args: &[String],
    subcommand: &str,
) -> Result<AgentUseBasicOptions, String> {
    let mut repo = None;
    let mut detail_mode = AgentUseDetailMode::Compact;
    let mut max_output_bytes = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--json" | "--agent-json" | "--agent_json" => {}
            "--explain" => {
                detail_mode = AgentUseDetailMode::Explain;
            }
            "--verbose" => {
                if detail_mode == AgentUseDetailMode::Compact {
                    detail_mode = AgentUseDetailMode::Explain;
                }
            }
            "--audit-json" | "--audit_json" => {
                detail_mode = AgentUseDetailMode::Audit;
            }
            "--max-output-bytes" | "--max-bytes" | "--max_output_bytes" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--max-output-bytes requires a value".to_string());
                };
                max_output_bytes = Some(parse_context_pack_max_output_bytes(value)?);
            }
            "--repo" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--repo requires a path".to_string());
                };
                repo = Some(PathBuf::from(value));
            }
            "--db" => {
                return Err(format!(
                    "agent-use {subcommand} owns DB resolution through the production profile; --db is not accepted"
                ));
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown agent-use {subcommand} option: {value}"));
            }
            value => {
                if repo.is_some() {
                    return Err(format!(
                        "Usage: codegraph-mcp agent-use {subcommand} --repo <repo> --json"
                    ));
                }
                repo = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    Ok(AgentUseBasicOptions {
        repo: repo.unwrap_or_else(|| PathBuf::from(".")),
        detail_mode,
        max_output_bytes,
    })
}

pub(crate) fn parse_agent_use_index_args(args: &[String]) -> Result<AgentUseIndexOptions, String> {
    let mut repo = None;
    let mut passthrough_index_args = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--json" | "--agent-json" | "--agent_json" => {}
            "--repo" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--repo requires a path".to_string());
                };
                repo = Some(PathBuf::from(value));
            }
            "--db" => {
                return Err(
                    "agent-use index owns DB resolution through the production profile; --db is not accepted"
                        .to_string(),
                );
            }
            "--fresh"
            | "--rebuild"
            | "--incremental"
            | "--fail-on-db-problem"
            | "--allow-stale-reuse"
            | "--profile" => {
                passthrough_index_args.push(args[index].clone());
            }
            "--workers"
            | "--storage-mode"
            | "--build-mode"
            | "--max-db-mib"
            | "--max-artifacts-mib"
            | "--min-free-disk-gib" => {
                let flag = args[index].clone();
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(format!("{flag} requires a value"));
                };
                passthrough_index_args.push(flag);
                passthrough_index_args.push(value.clone());
            }
            value if value.starts_with('-') => {
                return Err(format!(
                    "unknown agent-use index option: {value}; use plain index with explicit --db for advanced sidecar/scope flags"
                ));
            }
            value => {
                if repo.is_some() {
                    return Err(
                        "Usage: codegraph-mcp agent-use index --repo <repo> [--fresh|--rebuild|--incremental] [--json]".to_string(),
                    );
                }
                repo = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    Ok(AgentUseIndexOptions {
        repo: repo.unwrap_or_else(|| PathBuf::from(".")),
        passthrough_index_args,
    })
}

pub(crate) fn parse_agent_use_watch_args(args: &[String]) -> Result<AgentUseWatchOptions, String> {
    let mut repo = None;
    let mut once = false;
    let mut changed_paths = Vec::new();
    let mut test_events = Vec::new();
    let mut detail_mode = AgentUseDetailMode::Compact;
    let mut debounce = Duration::from_millis(AGENT_USE_WATCH_DEFAULT_DEBOUNCE_MS);
    let mut max_updates = None;
    let mut idle_timeout = None;
    let mut lock_retries = AGENT_USE_WATCH_DEFAULT_LOCK_RETRIES;
    let mut lock_retry = Duration::from_millis(AGENT_USE_WATCH_DEFAULT_LOCK_RETRY_MS);
    let mut max_batch_paths = AGENT_USE_WATCH_DEFAULT_MAX_BATCH_PATHS;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--json" | "--agent-json" | "--agent_json" => {}
            "--explain" | "--debug" => detail_mode = AgentUseDetailMode::Explain,
            "--verbose" if detail_mode == AgentUseDetailMode::Compact => {
                detail_mode = AgentUseDetailMode::Explain
            }
            "--audit-json" | "--audit_json" => detail_mode = AgentUseDetailMode::Audit,
            "--repo" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--repo requires a path".to_string());
                };
                repo = Some(PathBuf::from(value));
            }
            "--db" => {
                return Err(
                    "agent-use watch owns DB resolution through the production profile; --db is not accepted"
                        .to_string(),
                );
            }
            "--once" => once = true,
            "--changed" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--changed requires a path".to_string());
                };
                changed_paths.push(PathBuf::from(value));
            }
            "--test-event" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--test-event requires a path".to_string());
                };
                test_events.push(PathBuf::from(value));
            }
            "--debounce-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--debounce-ms requires a value".to_string());
                };
                debounce = Duration::from_millis(parse_u64_arg("--debounce-ms", value)?);
            }
            "--max-updates" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--max-updates requires a value".to_string());
                };
                max_updates = Some(parse_usize_arg("--max-updates", value)?);
            }
            "--idle-timeout-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--idle-timeout-ms requires a value".to_string());
                };
                idle_timeout = Some(Duration::from_millis(parse_u64_arg(
                    "--idle-timeout-ms",
                    value,
                )?));
            }
            "--lock-retries" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--lock-retries requires a value".to_string());
                };
                lock_retries = parse_usize_arg("--lock-retries", value)?;
            }
            "--lock-retry-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--lock-retry-ms requires a value".to_string());
                };
                lock_retry = Duration::from_millis(parse_u64_arg("--lock-retry-ms", value)?);
            }
            "--max-batch-paths" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--max-batch-paths requires a value".to_string());
                };
                max_batch_paths = parse_usize_arg("--max-batch-paths", value)?;
            }
            value if value.starts_with('-') => {
                return Err(format!(
                    "unknown agent-use watch option: {value}; supported shapes are `agent-use watch --repo <repo> --json` and `agent-use watch --repo <repo> --once --changed <path> [--changed <path>] --json`"
                ));
            }
            value => {
                if repo.is_some() {
                    return Err(
                        "Usage: codegraph-mcp agent-use watch --repo <repo> --json [--debounce-ms <ms>] or codegraph-mcp agent-use watch --repo <repo> --once --changed <path> [--changed <path>] --json"
                            .to_string(),
                    );
                }
                repo = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    Ok(AgentUseWatchOptions {
        repo: repo.unwrap_or_else(|| PathBuf::from(".")),
        once,
        changed_paths,
        test_events,
        detail_mode,
        debounce,
        max_updates,
        idle_timeout,
        lock_retries,
        lock_retry,
        max_batch_paths,
    })
}

pub(crate) fn parse_agent_use_validate_edit_args(
    args: &[String],
) -> Result<AgentUseValidateEditOptions, String> {
    let mut repo = None;
    let mut changed_paths = Vec::new();
    let mut detail_mode = AgentUseDetailMode::Compact;
    let mut max_output_bytes = None;
    let mut fail_on_blocking = false;
    let mut task_id = None;
    let mut edit_intent = None;
    let mut expected_touched_files = Vec::new();
    let mut max_validation_ms = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--json" | "--agent-json" | "--agent_json" => {}
            "--explain" | "--debug" => detail_mode = AgentUseDetailMode::Explain,
            "--verbose" if detail_mode == AgentUseDetailMode::Compact => {
                detail_mode = AgentUseDetailMode::Explain
            }
            "--audit-json" | "--audit_json" => detail_mode = AgentUseDetailMode::Audit,
            "--repo" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--repo requires a path".to_string());
                };
                repo = Some(PathBuf::from(value));
            }
            "--changed" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--changed requires a path".to_string());
                };
                changed_paths.push(PathBuf::from(value));
            }
            "--task-id" | "--task_id" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--task-id requires a value".to_string());
                };
                task_id = Some(value.clone());
            }
            "--edit-intent" | "--edit_intent" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--edit-intent requires a value".to_string());
                };
                edit_intent = Some(value.clone());
            }
            "--expected-touched-file" | "--expected_touched_file" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--expected-touched-file requires a path".to_string());
                };
                expected_touched_files.push(PathBuf::from(value));
            }
            "--max-output-bytes" | "--max-bytes" | "--max_output_bytes" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--max-output-bytes requires a value".to_string());
                };
                max_output_bytes = Some(parse_context_pack_max_output_bytes(value)?);
            }
            "--fail-on-blocking" | "--fail_on_blocking" => fail_on_blocking = true,
            // Policy opt-in (§1.3.4): the validation pipeline reads the env
            // var, so the flag is a process-local env bridge — this also makes
            // the policy uniform across validate-edit, watch, and journal
            // replay within the same run.
            "--block-on-unresolved-local" | "--block_on_unresolved_local" => {
                std::env::set_var(AGENT_USE_BLOCK_ON_UNRESOLVED_LOCAL_ENV, "1");
            }
            "--max-validation-ms" | "--max_validation_ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--max-validation-ms requires a value".to_string());
                };
                max_validation_ms = Some(value.trim().parse::<u64>().map_err(|_| {
                    "--max-validation-ms requires a non-negative integer of milliseconds"
                        .to_string()
                })?);
            }
            "--db" => {
                return Err(
                    "agent-use validate-edit owns DB resolution through the production profile; --db is not accepted"
                        .to_string(),
                );
            }
            "--mode" => {
                return Err(
                    "agent-use validate-edit does not accept --mode; use --agent-json/--json, --explain, or --audit-json"
                        .to_string(),
                );
            }
            value
                if value.starts_with("--max-output-bytes=")
                    || value.starts_with("--max-bytes=")
                    || value.starts_with("--max_output_bytes=") =>
            {
                let value = value
                    .split_once('=')
                    .map(|(_, value)| value)
                    .unwrap_or_default();
                max_output_bytes = Some(parse_context_pack_max_output_bytes(value)?);
            }
            value if value.starts_with("--task-id=") || value.starts_with("--task_id=") => {
                task_id = Some(
                    value
                        .split_once('=')
                        .map(|(_, value)| value.to_string())
                        .unwrap_or_default(),
                );
            }
            value if value.starts_with("--edit-intent=") || value.starts_with("--edit_intent=") => {
                edit_intent = Some(
                    value
                        .split_once('=')
                        .map(|(_, value)| value.to_string())
                        .unwrap_or_default(),
                );
            }
            value
                if value.starts_with("--max-validation-ms=")
                    || value.starts_with("--max_validation_ms=") =>
            {
                let value = value
                    .split_once('=')
                    .map(|(_, value)| value)
                    .unwrap_or_default();
                max_validation_ms = Some(value.trim().parse::<u64>().map_err(|_| {
                    "--max-validation-ms requires a non-negative integer of milliseconds"
                        .to_string()
                })?);
            }
            value
                if value.starts_with("--expected-touched-file=")
                    || value.starts_with("--expected_touched_file=") =>
            {
                let value = value
                    .split_once('=')
                    .map(|(_, value)| value)
                    .unwrap_or_default();
                expected_touched_files.push(PathBuf::from(value));
            }
            value if value.starts_with('-') => {
                return Err(format!(
                    "unknown agent-use validate-edit option: {value}; supported shape is `agent-use validate-edit --repo <repo> --changed <path> [--changed <path>] --agent-json`"
                ));
            }
            value => {
                if repo.is_some() {
                    return Err(
                        "Usage: codegraph-mcp agent-use validate-edit --repo <repo> --changed <path> [--changed <path>] --agent-json"
                            .to_string(),
                    );
                }
                repo = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    let Some(repo) = repo else {
        return Err(
            "agent-use validate-edit requires --repo <repo> so the production profile can resolve the external DB"
                .to_string(),
        );
    };
    if changed_paths.is_empty() {
        return Err("agent-use validate-edit requires at least one --changed <path>".to_string());
    }
    Ok(AgentUseValidateEditOptions {
        repo,
        changed_paths,
        detail_mode,
        max_output_bytes,
        fail_on_blocking,
        task_id,
        edit_intent,
        expected_touched_files,
        max_validation_ms,
    })
}

pub(crate) fn parse_u64_arg(flag: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("{flag} must be an integer: {error}"))
}

pub(crate) fn parse_usize_arg(flag: &str, value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|error| format!("{flag} must be an integer: {error}"))
}

pub(crate) fn parse_agent_use_forward_args(
    args: &[String],
    subcommand: &str,
) -> Result<AgentUseForwardOptions, String> {
    let mut repo = None;
    let mut forwarded_args = Vec::new();
    let mut literal_args = false;
    let mut index = 0usize;
    while index < args.len() {
        if literal_args {
            forwarded_args.push(args[index].clone());
            index += 1;
            continue;
        }
        match args[index].as_str() {
            "--" => {
                literal_args = true;
                forwarded_args.push(args[index].clone());
            }
            "--repo" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--repo requires a path".to_string());
                };
                repo = Some(PathBuf::from(value));
            }
            "--db" => {
                return Err(format!(
                    "agent-use {subcommand} owns DB resolution through the production profile; --db is not accepted"
                ));
            }
            value => forwarded_args.push(value.to_string()),
        }
        index += 1;
    }
    Ok(AgentUseForwardOptions {
        repo: repo.unwrap_or_else(|| PathBuf::from(".")),
        forwarded_args,
    })
}

pub(crate) fn agent_use_forward_detail_mode(args: &[String]) -> AgentUseDetailMode {
    let mut mode = AgentUseDetailMode::Compact;
    for arg in args {
        match arg.as_str() {
            "--audit-json" | "--audit_json" => return AgentUseDetailMode::Audit,
            "--explain" | "--debug" => mode = AgentUseDetailMode::Explain,
            "--verbose" if mode == AgentUseDetailMode::Compact => {
                mode = AgentUseDetailMode::Explain
            }
            _ => {}
        }
    }
    mode
}

pub(crate) fn strip_agent_use_query_wrapper_flags(
    args: Vec<String>,
) -> Result<(Vec<String>, AgentUseDetailMode, Option<usize>), String> {
    let detail_mode = agent_use_forward_detail_mode(&args);
    let mut max_output_bytes = None;
    let mut stripped = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--max-output-bytes" | "--max-bytes" | "--max_output_bytes" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--max-output-bytes requires a value".to_string());
                };
                max_output_bytes = Some(parse_context_pack_max_output_bytes(value)?);
            }
            value
                if value.starts_with("--max-output-bytes=")
                    || value.starts_with("--max-bytes=")
                    || value.starts_with("--max_output_bytes=") =>
            {
                let value = value
                    .split_once('=')
                    .map(|(_, value)| value)
                    .unwrap_or_default();
                max_output_bytes = Some(parse_context_pack_max_output_bytes(value)?);
            }
            "--explain" | "--verbose" | "--audit-json" | "--audit_json" => {}
            _ => stripped.push(args[index].clone()),
        }
        index += 1;
    }
    Ok((stripped, detail_mode, max_output_bytes))
}

pub(crate) fn agent_use_context_max_output_bytes(
    args: &[String],
    detail_mode: AgentUseDetailMode,
) -> usize {
    context_pack_forwarded_max_output_bytes(args)
        .unwrap_or_else(|| detail_mode.default_max_output_bytes())
}

pub(crate) fn ensure_context_pack_max_output_arg(args: &mut Vec<String>, max_output_bytes: usize) {
    if context_pack_forwarded_max_output_bytes(args).is_none() {
        args.push("--max-output-bytes".to_string());
        args.push(max_output_bytes.to_string());
    }
}

pub(crate) fn query_args_request_agent_json(args: &[String]) -> bool {
    args.iter()
        .any(|arg| matches!(arg.as_str(), "--agent-json" | "--agent_json" | "--concise"))
}

pub(crate) fn context_args_request_agent_json(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--agent-json" | "--agent_json" | "--concise" | "--audit-json" | "--audit_json"
        )
    })
}

pub(crate) fn context_args_have_vector_controls(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--enable-vector-candidates"
                | "--enable_vector_candidates"
                | "--vector-index"
                | "--vector-index-path"
                | "--vector_index"
                | "--vector_index_path"
                | "--vector-runtime-sidecar"
                | "--vector_runtime_sidecar"
        )
    })
}

pub(crate) fn agent_use_index_args_have_lifecycle_policy(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--fresh"
                | "--rebuild"
                | "--incremental"
                | "--fail-on-db-problem"
                | "--allow-stale-reuse"
        )
    })
}

pub(crate) fn agent_use_index_should_auto_fresh_rebuild(preflight: &DbLifecyclePreflight) -> bool {
    if preflight.safe || preflight.path_access_status == "db_missing" {
        return false;
    }
    if preflight
        .blockers
        .iter()
        .any(|blocker| blocker.contains("previous run did not complete"))
    {
        return true;
    }
    matches!(
        preflight.db_problem_kind.as_deref(),
        Some(
            "schema_mismatch"
                | "repo_head_mismatch"
                | "passport_missing"
                | "scope_mismatch"
                | "storage_mismatch"
        )
    )
}

pub(crate) fn cli_write_path_chaos_failpoint_enabled(name: &str) -> bool {
    #[cfg(test)]
    if let Some(raw) = CLI_WRITE_PATH_CHAOS_FAILPOINT_OVERRIDE.with(|override_cell| {
        override_cell
            .borrow()
            .as_ref()
            .map(std::string::ToString::to_string)
    }) {
        return raw
            .split(',')
            .map(str::trim)
            .any(|value| value == name || value == "agent_use_profile_all");
    }

    std::env::var(WRITE_PATH_CHAOS_FAILPOINT_ENV)
        .ok()
        .is_some_and(|raw| {
            raw.split(',')
                .map(str::trim)
                .any(|value| value == name || value == "agent_use_profile_all")
        })
}

#[cfg(test)]
pub(crate) fn set_cli_write_path_chaos_failpoint_override(
    failpoint: Option<String>,
) -> Option<String> {
    CLI_WRITE_PATH_CHAOS_FAILPOINT_OVERRIDE.with(|override_cell| {
        let previous = override_cell.borrow().clone();
        *override_cell.borrow_mut() = failpoint;
        previous
    })
}

pub(crate) fn run_agent_use_query_command(args: &[String]) -> Result<Value, String> {
    let options = parse_agent_use_forward_args(args, "query")?;
    let Some(query_kind) = options.forwarded_args.first().cloned() else {
        return Err(
            "Usage: codegraph-mcp agent-use query <symbols|text|files|references|definitions|callers|callees|path|chain> <args> --repo <repo> --limit <n> --agent-json\n  codegraph-mcp agent-use query unresolved-calls --repo <repo> [--path <repo-relative-or-absolute-path>] [--class repo_local_candidate|external_dependency|builtin_or_std|macro_or_codegen|dynamic_or_computed] [--limit <n>] --agent-json"
                .to_string(),
        );
    };
    if !agent_use_query_kind_supported(query_kind.as_str()) {
        return Err(format!(
            "agent-use query supports symbols, text, files, references, definitions, callers, callees, path, chain, and unresolved-calls; got {query_kind}"
        ));
    }
    let (mut forwarded_args, detail_mode, max_output_bytes) =
        strip_agent_use_query_wrapper_flags(options.forwarded_args)?;
    let max_output_bytes =
        max_output_bytes.unwrap_or_else(|| detail_mode.default_max_output_bytes());
    let profile = resolve_agent_use_profile(&options.repo)?;
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let normal_dot_codegraph_existed_before = normal_dot_codegraph.exists();
    if let Some(problem) = agent_use_profile_root_access_problem(&profile) {
        let mut value = agent_use_profile_access_unavailable_json(
            &profile,
            "query",
            Some(query_kind.as_str()),
            &problem,
            normal_dot_codegraph_existed_before,
        );
        compact_agent_use_agent_json_envelope(
            &mut value,
            &profile,
            detail_mode,
            max_output_bytes,
            None,
        );
        return Ok(value);
    }
    let preflight = inspect_read_db_lifecycle_preflight(
        &profile.repo_root,
        &profile.db_path,
        Some(profile.scope_policy.clone()),
    )?;
    if !preflight.safe {
        let mut value = agent_use_unavailable_json(
            &profile,
            &preflight,
            "query",
            Some(query_kind.as_str()),
            None,
            normal_dot_codegraph_existed_before,
        );
        compact_agent_use_agent_json_envelope(
            &mut value,
            &profile,
            detail_mode,
            max_output_bytes,
            None,
        );
        return Ok(value);
    }

    if !query_args_request_agent_json(&forwarded_args) {
        if query_kind == "unresolved-calls" {
            forwarded_args.push("--json".to_string());
        } else {
            forwarded_args.push("--agent-json".to_string());
        }
    }
    let mut value =
        with_agent_use_profile_context(&profile, || run_query_command(&forwarded_args))?;
    annotate_agent_use_output(
        &mut value,
        &profile,
        "query",
        normal_dot_codegraph_existed_before,
    );
    add_agent_use_db_lifecycle_read(&mut value, &preflight);
    add_agent_use_durability_labels(&mut value, &profile, &preflight, None);
    add_agent_use_query_read_path_metrics(&mut value, query_kind.as_str());
    compact_agent_use_agent_json_envelope(
        &mut value,
        &profile,
        detail_mode,
        max_output_bytes,
        None,
    );
    Ok(value)
}

pub(crate) fn agent_use_query_kind_supported(kind: &str) -> bool {
    matches!(
        kind,
        "symbols"
            | "text"
            | "files"
            | "references"
            | "definitions"
            | "callers"
            | "callees"
            | "path"
            | "chain"
            | "unresolved-calls"
    )
}

pub(crate) fn run_agent_use_context_pack_command(args: &[String]) -> Result<Value, String> {
    let options = parse_agent_use_forward_args(args, "context-pack")?;
    let profile = resolve_agent_use_profile(&options.repo)?;
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let normal_dot_codegraph_existed_before = normal_dot_codegraph.exists();
    let mut forwarded_args = options.forwarded_args;
    let detail_mode = agent_use_forward_detail_mode(&forwarded_args);
    let max_output_bytes = agent_use_context_max_output_bytes(&forwarded_args, detail_mode);
    if let Some(problem) = agent_use_profile_root_access_problem(&profile) {
        let mut value = agent_use_profile_access_unavailable_json(
            &profile,
            "context-pack",
            None,
            &problem,
            normal_dot_codegraph_existed_before,
        );
        compact_agent_use_agent_json_envelope(
            &mut value,
            &profile,
            detail_mode,
            max_output_bytes,
            None,
        );
        return Ok(value);
    }
    let preflight = inspect_read_db_lifecycle_preflight(
        &profile.repo_root,
        &profile.db_path,
        Some(profile.scope_policy.clone()),
    )?;
    ensure_context_pack_max_output_arg(&mut forwarded_args, max_output_bytes);
    if !context_args_request_agent_json(&forwarded_args) {
        forwarded_args.push("--agent-json".to_string());
    }
    if preflight.safe {
        if profile.vector_runtime_path.exists()
            && !context_args_have_vector_controls(&forwarded_args)
        {
            forwarded_args.push("--enable-vector-candidates".to_string());
            forwarded_args.push("--vector-runtime-sidecar".to_string());
            forwarded_args.push(path_string(&profile.vector_runtime_path));
        }
        let mut value =
            with_agent_use_profile_context(&profile, || run_context_pack_command(&forwarded_args))?;
        annotate_agent_use_output(
            &mut value,
            &profile,
            "context-pack",
            normal_dot_codegraph_existed_before,
        );
        add_agent_use_db_lifecycle_read(&mut value, &preflight);
        add_agent_use_staged_availability(&mut value, &profile, &preflight);
        let staged_availability = value
            .get("staged_availability")
            .cloned()
            .unwrap_or_else(|| {
                staged_availability_for_cli(
                    &profile.repo_root,
                    &profile.db_path,
                    Some(&preflight),
                    Some(&profile.candidate_spool_path),
                    Some(&profile.vector_runtime_path),
                    Some(&profile.vector_audit_path),
                    None,
                )
            });
        add_agent_use_rtds_freshness_fields(&mut value, &profile, &preflight, &staged_availability);
        add_agent_use_durability_labels(&mut value, &profile, &preflight, None);
        add_agent_use_context_pack_read_path_metrics(&mut value);
        // Compact the agent-use-specific heavy sections (recovery, lifecycle,
        // read_path_metrics, etc.) BEFORE the context-pack size enforcer runs.
        // Otherwise the enforcer sees a transiently-oversized envelope and sheds
        // contract-required sections (patch_assist_packet) that would have fit
        // once the agent-use compaction shrank the rest.
        compact_agent_use_agent_json_envelope(
            &mut value,
            &profile,
            detail_mode,
            max_output_bytes,
            Some(&staged_availability),
        );
        enforce_agent_use_context_pack_max_output_bytes(&mut value, max_output_bytes);
        return Ok(value);
    }

    if preflight.path_access_status == "db_missing" && profile.candidate_spool_path.exists() {
        if !forwarded_args.iter().any(|arg| {
            matches!(
                arg.as_str(),
                "--candidate-spool"
                    | "--candidate_spool"
                    | "--early-candidates"
                    | "--early_candidates"
            )
        }) {
            forwarded_args.push("--candidate-spool".to_string());
            forwarded_args.push(path_string(&profile.candidate_spool_path));
            forwarded_args.push("--early-candidates".to_string());
        }
        return match with_agent_use_profile_context(&profile, || {
            run_context_pack_command(&forwarded_args)
        }) {
            Ok(mut value) => {
                annotate_agent_use_output(
                    &mut value,
                    &profile,
                    "context-pack",
                    normal_dot_codegraph_existed_before,
                );
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "db_lifecycle_read".to_string(),
                        db_lifecycle_preflight_json(&preflight, true, false, false),
                    );
                }
                add_agent_use_durability_labels(&mut value, &profile, &preflight, None);
                let staged_availability =
                    value
                        .get("staged_availability")
                        .cloned()
                        .unwrap_or_else(|| {
                            staged_availability_for_cli(
                                &profile.repo_root,
                                &profile.db_path,
                                Some(&preflight),
                                Some(&profile.candidate_spool_path),
                                Some(&profile.vector_runtime_path),
                                Some(&profile.vector_audit_path),
                                None,
                            )
                        });
                add_agent_use_rtds_freshness_fields(
                    &mut value,
                    &profile,
                    &preflight,
                    &staged_availability,
                );
                add_agent_use_context_pack_read_path_metrics(&mut value);
                // Compact agent-use-specific heavy sections before the context
                // enforcer so it does not shed contract-required patch_assist_packet
                // from a transiently-oversized envelope (matches the safe-DB path).
                compact_agent_use_agent_json_envelope(
                    &mut value,
                    &profile,
                    detail_mode,
                    max_output_bytes,
                    Some(&staged_availability),
                );
                enforce_agent_use_context_pack_max_output_bytes(&mut value, max_output_bytes);
                Ok(value)
            }
            Err(error) => {
                let mut value = agent_use_unavailable_json(
                    &profile,
                    &preflight,
                    "context-pack",
                    None,
                    Some(error),
                    normal_dot_codegraph_existed_before,
                );
                let staged_availability = value.get("staged_availability").cloned();
                compact_agent_use_agent_json_envelope(
                    &mut value,
                    &profile,
                    detail_mode,
                    max_output_bytes,
                    staged_availability.as_ref(),
                );
                Ok(value)
            }
        };
    }

    let mut value = agent_use_unavailable_json(
        &profile,
        &preflight,
        "context-pack",
        None,
        None,
        normal_dot_codegraph_existed_before,
    );
    let staged_availability = value.get("staged_availability").cloned();
    compact_agent_use_agent_json_envelope(
        &mut value,
        &profile,
        detail_mode,
        max_output_bytes,
        staged_availability.as_ref(),
    );
    Ok(value)
}

pub(crate) fn context_pack_forwarded_max_output_bytes(args: &[String]) -> Option<usize> {
    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index].as_str();
        if matches!(
            arg,
            "--max-output-bytes" | "--max-bytes" | "--max_output_bytes"
        ) {
            return args
                .get(index.saturating_add(1))
                .and_then(|value| parse_context_pack_max_output_bytes(value).ok());
        }
        for prefix in ["--max-output-bytes=", "--max-bytes=", "--max_output_bytes="] {
            if let Some(value) = arg.strip_prefix(prefix) {
                return parse_context_pack_max_output_bytes(value).ok();
            }
        }
        index = index.saturating_add(1);
    }
    None
}

pub(crate) fn run_agent_use_mcp_config_command(args: &[String]) -> Result<Value, String> {
    let options = parse_agent_use_basic_args(args, "mcp-config")?;
    let profile = resolve_agent_use_profile(&options.repo)?;
    let generated_at_unix_ms = unix_time_ms();
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let normal_dot_codegraph_existed_before = normal_dot_codegraph.exists();
    if let Some(problem) = agent_use_profile_root_access_problem(&profile) {
        return Ok(agent_use_profile_access_unavailable_json(
            &profile,
            "mcp-config",
            None,
            &problem,
            normal_dot_codegraph_existed_before,
        ));
    }
    let preflight = inspect_read_db_lifecycle_preflight(
        &profile.repo_root,
        &profile.db_path,
        Some(profile.scope_policy.clone()),
    )?;
    let sqlite_sidecars =
        sqlite_sidecars_status_from_health(&profile.db_path, &preflight.db_health);
    let lifecycle = db_lifecycle_preflight_json(&preflight, true, false, false);
    let safety_labels = agent_use_safety_labels(&profile, &preflight, Some(&sqlite_sidecars));
    let publish_state = agent_use_publish_state_json(&profile);
    let status = if preflight.safe {
        "ok"
    } else if preflight.path_access_status == "db_missing" {
        "not_indexed"
    } else {
        preflight.db_problem_kind.as_deref().unwrap_or("db_problem")
    };
    let binary = discover_agent_use_binary_path(&profile.repo_root);
    let sidecar_paths = agent_use_profile_sidecar_paths_json(&profile);
    let path_mapping = agent_use_profile_path_mapping_json(&profile);
    let mapping_warnings = path_mapping["warnings"].clone();
    let mcp_config_identity =
        agent_use_mcp_config_identity_json(&profile, sidecar_paths.clone(), path_mapping.clone());
    let server = json!({
        "command": binary,
        "args": profile.mcp_args.clone(),
        "cwd": path_string(&profile.repo_root),
        "env": {
            "CODEGRAPH_DB_PATH": path_string(&profile.db_path),
            "CODEGRAPH_DB_SOURCE": "agent-use profile",
            "CODEGRAPH_REPO_SOURCE": "agent-use --repo",
            "CODEGRAPH_AGENT_USE_PROFILE": profile.profile_name.clone(),
            "CODEGRAPH_AGENT_USE_REPO_IDENTITY_HASH": profile.repo_identity_hash.clone(),
            "CODEGRAPH_AGENT_USE_PROFILE_ROOT": path_string(&profile.profile_root),
        },
    });
    Ok(json!({
        "status": status,
        "config_version": 1,
        "generated_at": generated_at_unix_ms,
        "generated_at_unix_ms": generated_at_unix_ms,
        "command": "mcp-config",
        "command_namespace": "agent-use",
        "profile_name": profile.profile_name.clone(),
        "active_profile_name": profile.profile_name.clone(),
        "local_agent_profile": {
            "profile_name": profile.profile_name.clone(),
            "agent_label": Value::Null,
            "agent_label_supported": false,
            "profile_root": path_string(&profile.profile_root),
        },
        "repo": path_string(&profile.repo_root),
        "repo_root": path_string(&profile.repo_root),
        "repo_identity_label": profile.repo_identity_label.clone(),
        "repo_identity_hash": profile.repo_identity_hash.clone(),
        "repo_identity_short_hash": agent_use_repo_identity_short_hash(&profile),
        "profile_root": path_string(&profile.profile_root),
        "db": path_string(&profile.db_path),
        "db_path": path_string(&profile.db_path),
          "db_source": "agent-use profile",
          "sidecar_paths": sidecar_paths,
          "path_mapping": path_mapping,
          "mapping_warnings": mapping_warnings,
          "env_discovery": agent_use_env_discovery_json(&profile),
          "config_discovery": agent_use_config_discovery_json(&profile.repo_root),
          "artifact_hygiene": agent_use_artifact_hygiene_json(),
          "mcp_config_identity": mcp_config_identity,
          "mcp_server_args": profile.mcp_args.clone(),
        "external_db_used": true,
        "canonical_production_profile": true,
        "safe_read_only_startup": true,
        "auto_index_on_startup": false,
        "mcp_startup_auto_index": false,
        "mcp_no_dot_codegraph_fallback": true,
        "startup_policy": {
            "safe_read_only_startup": true,
            "auto_index_on_startup": false,
            "missing_db_claims_ready": false,
            "stale_db_claims_ready": false,
            "dot_codegraph_fallback": false,
        },
        "output_policy": {
            "default_stdout_json_only": true,
            "writes_files_by_default": false,
            "write_mode_supported": false,
            "explicit_output_path_supported": false,
            "output_path": Value::Null,
        },
        "claimable": preflight.safe,
        "diagnostic_only": !preflight.safe,
        "db_lifecycle_read": lifecycle,
        "publish_state": publish_state,
        "publishing": agent_use_publish_state_active(&profile),
        "safety_labels": safety_labels,
        "sqlite_sidecars": sqlite_sidecars.clone(),
        "sidecar_status": sqlite_sidecars["sidecar_status"].clone(),
        "sidecar_only_change": sqlite_sidecars["sidecar_only_change"].clone(),
        "sidecar_change_classification": sqlite_sidecars["sidecar_change_classification"].clone(),
        "mcp_config": {
            "mcpServers": {
                "codegraph-mcp": server.clone()
            }
        },
        "codex_mcp_servers": {
            "codegraph-mcp": server
        },
        "recommended_commands": agent_use_recovery_json(&profile),
        "recovery_commands": profile.recovery_commands.clone(),
        "warnings": if preflight.safe { Vec::<String>::new() } else { preflight.blockers.clone() },
        "normal_dot_codegraph_path": path_string(&normal_dot_codegraph),
        "normal_dot_codegraph_created": !normal_dot_codegraph_existed_before && normal_dot_codegraph.exists(),
        "normal_dot_codegraph_mutated": normal_dot_codegraph_existed_before != normal_dot_codegraph.exists(),
        "writes_files": false,
        "public_claim": false,
    }))
}

pub(crate) fn run_agent_use_status_command(args: &[String]) -> Result<Value, String> {
    let started = Instant::now();
    let options = parse_agent_use_basic_args(args, "status")?;
    let profile = resolve_agent_use_profile(&options.repo)?;
    let profile_parent_existed_before = profile.profile_root.exists();
    let db_existed_before = profile.db_path.exists();
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let normal_dot_codegraph_existed_before = normal_dot_codegraph.exists();
    if let Some(problem) = agent_use_profile_root_access_problem(&profile) {
        return Ok(agent_use_profile_access_unavailable_json(
            &profile,
            "status",
            None,
            &problem,
            normal_dot_codegraph_existed_before,
        ));
    }

    let preflight = inspect_read_db_lifecycle_preflight(
        &profile.repo_root,
        &profile.db_path,
        Some(profile.scope_policy.clone()),
    )?;
    let sqlite_sidecars =
        sqlite_sidecars_status_from_health(&profile.db_path, &preflight.db_health);
    let staged_availability = staged_availability_for_cli(
        &profile.repo_root,
        &profile.db_path,
        Some(&preflight),
        Some(&profile.candidate_spool_path),
        Some(&profile.vector_runtime_path),
        Some(&profile.vector_audit_path),
        None,
    );
    let staged_fields = staged_availability_top_level_fields(&staged_availability);
    let lifecycle = db_lifecycle_preflight_json(&preflight, true, false, false);
    let db_exists = profile.db_path.exists();
    let status = if preflight.safe {
        "ok".to_string()
    } else if preflight.path_access_status == "db_missing" {
        "not_indexed".to_string()
    } else {
        preflight
            .db_problem_kind
            .clone()
            .unwrap_or_else(|| "db_problem".to_string())
    };
    let mut value = agent_use_status_base_json(
        &profile,
        &preflight,
        &sqlite_sidecars,
        &lifecycle,
        &status,
        started.elapsed().as_secs_f64() * 1000.0,
    );

    if let Some(object) = value.as_object_mut() {
        object.insert(
            "db_schema_version".to_string(),
            preflight
                .db_health
                .schema_version
                .map(Value::from)
                .unwrap_or(Value::Null),
        );
        object.insert(
            "files".to_string(),
            preflight
                .db_health
                .passport
                .as_ref()
                .map(|passport| json!(passport.files_indexed))
                .unwrap_or(Value::Null),
        );
        object.insert("entities".to_string(), Value::Null);
        object.insert("relation_facts".to_string(), Value::Null);
        object.insert("source_span_facts".to_string(), Value::Null);
        object.insert("edges".to_string(), Value::Null);
        object.insert("source_spans".to_string(), Value::Null);
        object.insert("relation_counts".to_string(), Value::Null);
        object.insert("storage_accounting".to_string(), Value::Null);
        object.insert("languages".to_string(), Value::Null);
        object.insert(
            "status_detail_source".to_string(),
            json!("db_lifecycle_preflight_and_passport_only"),
        );
        object.insert("db_exists".to_string(), json!(db_exists));
        object.insert(
            "profile_parent_exists".to_string(),
            json!(profile.profile_root.exists()),
        );
        object.insert(
            "profile_parent_created".to_string(),
            json!(!profile_parent_existed_before && profile.profile_root.exists()),
        );
        object.insert(
            "db_created".to_string(),
            json!(!db_existed_before && profile.db_path.exists()),
        );
        object.insert(
            "normal_dot_codegraph_path".to_string(),
            json!(path_string(&normal_dot_codegraph)),
        );
        object.insert(
            "normal_dot_codegraph_created".to_string(),
            json!(!normal_dot_codegraph_existed_before && normal_dot_codegraph.exists()),
        );
        object.insert(
            "normal_dot_codegraph_mutated".to_string(),
            json!(normal_dot_codegraph_existed_before != normal_dot_codegraph.exists()),
        );
        object.insert(
            "validation_state".to_string(),
            agent_use_validation_state_block_compact(&profile),
        );
    }
    merge_json_object(&mut value, staged_fields);
    add_agent_use_rtds_freshness_fields(&mut value, &profile, &preflight, &staged_availability);
    add_agent_use_dirty_evidence_output_fields(&mut value, "agent-use.status", false);
    compact_agent_use_agent_json_envelope(
        &mut value,
        &profile,
        options.detail_mode,
        options
            .max_output_bytes
            .unwrap_or_else(|| options.detail_mode.default_max_output_bytes()),
        Some(&staged_availability),
    );
    Ok(value)
}

pub(crate) fn run_agent_use_index_command(args: &[String]) -> Result<Value, String> {
    let options = parse_agent_use_index_args(args)?;
    let profile = resolve_agent_use_profile(&options.repo)?;
    let profile_parent_existed_before = profile.profile_root.exists();
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let normal_dot_codegraph_existed_before = normal_dot_codegraph.exists();

    if cli_write_path_chaos_failpoint_enabled(AGENT_USE_PROFILE_PARENT_PERMISSION_DENIED_FAILPOINT)
    {
        return Err(agent_use_profile_parent_create_error_json(
            &profile,
            "permission_denied",
            &format!("chaos_failpoint:{AGENT_USE_PROFILE_PARENT_PERMISSION_DENIED_FAILPOINT}"),
            normal_dot_codegraph_existed_before,
        )?);
    }
    if cli_write_path_chaos_failpoint_enabled(
        AGENT_USE_PROFILE_PARENT_FILESYSTEM_INACCESSIBLE_FAILPOINT,
    ) {
        return Err(agent_use_profile_parent_create_error_json(
            &profile,
            "filesystem_inaccessible",
            &format!(
                "chaos_failpoint:{AGENT_USE_PROFILE_PARENT_FILESYSTEM_INACCESSIBLE_FAILPOINT}"
            ),
            normal_dot_codegraph_existed_before,
        )?);
    }

    if let Err(error) = fs::create_dir_all(&profile.profile_root) {
        let problem_kind = if error.kind() == std::io::ErrorKind::PermissionDenied {
            "permission_denied"
        } else {
            "filesystem_inaccessible"
        };
        return Err(agent_use_profile_parent_create_error_json(
            &profile,
            problem_kind,
            &format!("agent-use profile parent could not be created: {error}"),
            normal_dot_codegraph_existed_before,
        )?);
    }

    let mut passthrough_index_args = options.passthrough_index_args;
    let auto_fresh_rebuild = if agent_use_index_args_have_lifecycle_policy(&passthrough_index_args)
    {
        false
    } else {
        let preflight = inspect_read_db_lifecycle_preflight(
            &profile.repo_root,
            &profile.db_path,
            Some(profile.scope_policy.clone()),
        )?;
        if agent_use_index_should_auto_fresh_rebuild(&preflight) {
            passthrough_index_args.push("--fresh".to_string());
            true
        } else {
            false
        }
    };

    let mut index_args = vec![
        path_string(&profile.repo_root),
        "--db".to_string(),
        path_string(&profile.db_path),
        "--json".to_string(),
        "--candidate-spool".to_string(),
        path_string(&profile.candidate_spool_path),
        "--candidate-spool-policy".to_string(),
        "bounded".to_string(),
        "--candidate-spool-query-index".to_string(),
        "yes".to_string(),
        "--vector-runtime-sidecar".to_string(),
        path_string(&profile.vector_runtime_path),
        "--no-vector-audit".to_string(),
    ];
    index_args.extend(passthrough_index_args);
    write_agent_use_publish_state(&profile, "publishing", None)?;
    let mut value = match run_index_command(&index_args) {
        Ok(value) => {
            clear_agent_use_publish_state(&profile)?;
            value
        }
        Err(error) => {
            if let Err(state_error) =
                write_agent_use_publish_state(&profile, "interrupted", Some(&error))
            {
                return Err(format!(
                    "{error}; publish_state_update_failed: {state_error}"
                ));
            }
            return Err(error);
        }
    };
    // MVP3.9.5b/c: a full/incremental reindex supersedes any pending
    // validation journal, and persisted open blockers are re-checked once
    // against the freshly indexed store (kept while still contradicted).
    let had_pending_journal = matches!(load_validation_journal(&profile), Ok(Some(_)) | Err(_));
    clear_validation_journal(&profile);
    let open_blockers_recheck = agent_use_recheck_open_blockers_after_index(&profile);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "validation_journal_cleared".to_string(),
            json!(had_pending_journal),
        );
        object.insert("open_blockers_recheck".to_string(), open_blockers_recheck);
    }
    let post_preflight = inspect_read_db_lifecycle_preflight(
        &profile.repo_root,
        &profile.db_path,
        Some(profile.scope_policy.clone()),
    )?;
    let staged_availability = staged_availability_for_cli(
        &profile.repo_root,
        &profile.db_path,
        Some(&post_preflight),
        Some(&profile.candidate_spool_path),
        Some(&profile.vector_runtime_path),
        Some(&profile.vector_audit_path),
        None,
    );
    let profile_parent_created = !profile_parent_existed_before && profile.profile_root.exists();
    let normal_dot_codegraph_created =
        !normal_dot_codegraph_existed_before && normal_dot_codegraph.exists();
    let warm_unchanged_reuse = value
        .get("files_indexed")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        == 0
        && value
            .get("files_metadata_unchanged")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0;
    let cold_build = value
        .get("lifecycle")
        .and_then(|lifecycle| lifecycle.get("old_db_used"))
        .and_then(Value::as_bool)
        .map(|old_db_used| !old_db_used)
        .unwrap_or(!warm_unchanged_reuse);
    let candidate_spool_layer = staged_availability
        .pointer("/layer_readiness/candidate_spool")
        .cloned()
        .unwrap_or(Value::Null);
    let vector_runtime_layer = staged_availability
        .pointer("/layer_readiness/vector_runtime")
        .cloned()
        .unwrap_or(Value::Null);
    let vector_audit_layer = staged_availability
        .pointer("/layer_readiness/vector_audit")
        .cloned()
        .unwrap_or(Value::Null);
    let candidate_spool_bytes = fs::metadata(&profile.candidate_spool_path)
        .map(|metadata| metadata.len())
        .ok();
    let candidate_spool_query_index_bytes = fs::metadata(&profile.candidate_spool_query_index_path)
        .map(|metadata| metadata.len())
        .ok();
    let vector_runtime_bytes = fs::metadata(&profile.vector_runtime_path)
        .map(|metadata| metadata.len())
        .ok();
    let vector_audit_bytes = fs::metadata(&profile.vector_audit_path)
        .map(|metadata| metadata.len())
        .ok();
    let artifact_warnings = staged_warning_values(&staged_availability);
    if let Some(object) = value.as_object_mut() {
        object.insert("command_namespace".to_string(), json!("agent-use"));
        object.insert(
            "profile_name".to_string(),
            json!(profile.profile_name.clone()),
        );
        object.insert(
            "repo_identity_label".to_string(),
            json!(profile.repo_identity_label.clone()),
        );
        object.insert(
            "repo_identity_hash".to_string(),
            json!(profile.repo_identity_hash.clone()),
        );
        object.insert(
            "repo_identity_short_hash".to_string(),
            json!(agent_use_repo_identity_short_hash(&profile)),
        );
        object.insert("profile".to_string(), agent_use_profile_json(&profile));
        object.insert(
            "agent_use_profile".to_string(),
            agent_use_profile_json(&profile),
        );
        object.insert("repo".to_string(), json!(path_string(&profile.repo_root)));
        object.insert(
            "repo_root".to_string(),
            json!(path_string(&profile.repo_root)),
        );
        object.insert("db".to_string(), json!(path_string(&profile.db_path)));
        object.insert("db_path".to_string(), json!(path_string(&profile.db_path)));
        object.insert("external_db_used".to_string(), json!(true));
        object.insert(
            "profile_parent_created".to_string(),
            json!(profile_parent_created),
        );
        object.insert(
            "profile_parent_exists".to_string(),
            json!(profile.profile_root.exists()),
        );
        object.insert(
            "normal_dot_codegraph_path".to_string(),
            json!(path_string(&normal_dot_codegraph)),
        );
        object.insert(
            "normal_dot_codegraph_created".to_string(),
            json!(normal_dot_codegraph_created),
        );
        object.insert(
            "normal_dot_codegraph_mutated".to_string(),
            json!(normal_dot_codegraph_existed_before != normal_dot_codegraph.exists()),
        );
        object.insert(
            "warm_unchanged_reuse".to_string(),
            json!(warm_unchanged_reuse),
        );
        object.insert("cold_build".to_string(), json!(cold_build));
        object.insert(
            "agent_use_auto_fresh_rebuild".to_string(),
            json!(auto_fresh_rebuild),
        );
        object.insert(
            "publish_state".to_string(),
            agent_use_publish_state_json(&profile),
        );
        object.insert(
            "publishing".to_string(),
            json!(agent_use_publish_state_active(&profile)),
        );
        object.insert(
            "safety_labels".to_string(),
            json!(agent_use_safety_labels(&profile, &post_preflight, None)),
        );
        object.insert("candidate_spool_requested".to_string(), json!(true));
        object.insert(
            "candidate_spool_created".to_string(),
            json!(profile.candidate_spool_path.exists()),
        );
        object.insert(
            "candidate_spool_profile_path".to_string(),
            json!(path_string(&profile.candidate_spool_path)),
        );
        object.insert(
            "candidate_spool_profile_query_index_path".to_string(),
            json!(path_string(&profile.candidate_spool_query_index_path)),
        );
        object.insert(
            "candidate_spool_profile_status".to_string(),
            candidate_spool_layer
                .get("status")
                .cloned()
                .unwrap_or_else(|| json!("unknown")),
        );
        object.insert(
            "candidate_spool_query_index_status".to_string(),
            candidate_spool_layer
                .get("query_index_status")
                .cloned()
                .unwrap_or(Value::Null),
        );
        object.insert(
            "candidate_spool_query_index_created".to_string(),
            json!(profile.candidate_spool_query_index_path.exists()),
        );
        object.insert(
            "candidate_spool_artifact_lifecycle".to_string(),
            candidate_spool_layer.clone(),
        );
        object.insert("vector_runtime_requested".to_string(), json!(true));
        object.insert(
            "vector_runtime_created".to_string(),
            json!(profile.vector_runtime_path.exists()),
        );
        object.insert(
            "vector_runtime_profile_path".to_string(),
            json!(path_string(&profile.vector_runtime_path)),
        );
        object.insert(
            "vector_runtime_profile_status".to_string(),
            vector_runtime_layer
                .get("status")
                .cloned()
                .unwrap_or_else(|| json!("unknown")),
        );
        object.insert(
            "vector_runtime_artifact_lifecycle".to_string(),
            vector_runtime_layer.clone(),
        );
        object.insert("vector_audit_requested".to_string(), json!(false));
        object.insert(
            "vector_audit_created".to_string(),
            json!(profile.vector_audit_path.exists()),
        );
        object.insert(
            "vector_audit_profile_path".to_string(),
            json!(path_string(&profile.vector_audit_path)),
        );
        object.insert(
            "vector_audit_profile_status".to_string(),
            vector_audit_layer
                .get("status")
                .cloned()
                .unwrap_or_else(|| json!("missing")),
        );
        object.insert(
            "vector_audit_artifact_lifecycle".to_string(),
            vector_audit_layer.clone(),
        );
        object.insert(
            "artifact_paths".to_string(),
            json!({
                "db_path": path_string(&profile.db_path),
                "candidate_spool_path": path_string(&profile.candidate_spool_path),
                "candidate_spool_query_index_path": path_string(&profile.candidate_spool_query_index_path),
                "vector_runtime_path": path_string(&profile.vector_runtime_path),
                "vector_audit_path": path_string(&profile.vector_audit_path),
            }),
        );
        object.insert(
            "artifact_sizes".to_string(),
            json!({
                "candidate_spool_bytes": candidate_spool_bytes,
                "candidate_spool_query_index_bytes": candidate_spool_query_index_bytes,
                "vector_runtime_bytes": vector_runtime_bytes,
                "vector_audit_bytes": vector_audit_bytes,
            }),
        );
        object.insert(
            "artifact_lifecycle_binding".to_string(),
            json!({
                "repo_root": path_string(&profile.repo_root),
                "db_path": path_string(&profile.db_path),
                "profile_name": profile.profile_name.clone(),
                "candidate_spool": candidate_spool_layer,
                "vector_runtime": vector_runtime_layer,
                "vector_audit": vector_audit_layer,
            }),
        );
        object.insert("artifact_warnings".to_string(), artifact_warnings);
        object.insert(
            "staged_availability".to_string(),
            staged_availability.clone(),
        );
        object.insert("recovery".to_string(), agent_use_recovery_json(&profile));
        object.insert("public_claim".to_string(), json!(false));
    }
    merge_json_object(
        &mut value,
        staged_availability_top_level_fields(&staged_availability),
    );
    add_agent_use_rtds_freshness_fields(
        &mut value,
        &profile,
        &post_preflight,
        &staged_availability,
    );
    Ok(value)
}

pub(crate) fn run_agent_use_watch_command(args: &[String]) -> Result<Value, String> {
    let options = parse_agent_use_watch_args(args)?;
    if options.once {
        if options.changed_paths.is_empty() {
            return Err(
                "agent-use watch --once requires at least one --changed <path>".to_string(),
            );
        }
        let profile = resolve_agent_use_profile(&options.repo)?;
        let normal_dot_codegraph = profile.repo_root.join(".codegraph");
        let normal_dot_codegraph_existed_before = normal_dot_codegraph.exists();
        if let Some(problem) = agent_use_profile_root_access_problem(&profile) {
            return Ok(agent_use_profile_access_unavailable_json(
                &profile,
                "watch",
                Some("once"),
                &problem,
                normal_dot_codegraph_existed_before,
            ));
        }
        return run_agent_use_watch_once_delta(
            &profile,
            options.changed_paths,
            options.detail_mode,
            normal_dot_codegraph_existed_before,
            None,
        );
    }
    if !options.changed_paths.is_empty() {
        return Err(
            "agent-use persistent watch gathers filesystem events itself; use --once with --changed for deterministic one-shot updates"
                .to_string(),
        );
    }
    run_agent_use_persistent_watch_command(options)
}

pub(crate) fn run_agent_use_validate_edit_command(args: &[String]) -> Result<Value, String> {
    let options = parse_agent_use_validate_edit_args(args)?;
    let profile = resolve_agent_use_profile(&options.repo)?;
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let normal_dot_codegraph_existed_before = normal_dot_codegraph.exists();
    if let Some(problem) = agent_use_profile_root_access_problem(&profile) {
        return Ok(agent_use_profile_access_unavailable_json(
            &profile,
            "validate-edit",
            None,
            &problem,
            normal_dot_codegraph_existed_before,
        ));
    }
    let source_update = run_agent_use_watch_once_delta(
        &profile,
        options.changed_paths.clone(),
        options.detail_mode,
        normal_dot_codegraph_existed_before,
        options.max_validation_ms,
    )?;
    if source_update
        .get("validation_pipeline_error")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        // Pass the structured pipeline-error packet through unchanged (it
        // carries its own nonzero _cli_exit_code); wrapping it would mislabel
        // a post-commit panic as preflight_blocked.
        let mut packet = source_update;
        if let Some(object) = packet.as_object_mut() {
            object.insert("command".to_string(), json!("validate-edit"));
            object.insert(
                "agent_use_command".to_string(),
                json!("agent-use validate-edit"),
            );
        }
        return Ok(packet);
    }
    let mut packet = agent_use_validate_edit_packet_json(&profile, &options, source_update);
    let max_output_bytes = options
        .max_output_bytes
        .unwrap_or_else(|| options.detail_mode.default_max_output_bytes());
    agent_use_validate_edit_finalize_budget(&mut packet, options.detail_mode, max_output_bytes);
    agent_use_validate_edit_apply_exit_policy(&mut packet, options.fail_on_blocking);
    Ok(packet)
}

fn agent_use_validate_edit_apply_exit_policy(packet: &mut Value, fail_on_blocking: bool) {
    let must_exit_nonzero = fail_on_blocking
        && packet
            .get("must_fix_before_continuing")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    if must_exit_nonzero {
        if let Some(object) = packet.as_object_mut() {
            object.insert("_cli_exit_code".to_string(), json!(2));
        }
    }
}

pub(crate) fn run_validate_edit_alias_deferred_command(args: &[String]) -> Result<Value, String> {
    let _ = args;
    Err(serde_json::to_string(&json!({
        "status": "error",
        "error": "validate_edit_alias_deferred",
        "message": "top-level validate-edit is deferred; use the canonical agent-use validate-edit surface",
        "compatibility_alias_status": "deferred",
        "canonical_cli_surface": format!("{BIN_NAME} agent-use validate-edit --repo <repo> --changed <path> --agent-json"),
        "canonical_command": "agent-use validate-edit",
        "accepted_shape": format!("{BIN_NAME} agent-use validate-edit --repo <repo> --changed <path> [--changed <path>] --agent-json"),
        "public_claim": false,
    }))
    .unwrap_or_else(|_| {
        "{\"status\":\"error\",\"error\":\"validate_edit_alias_deferred\"}".to_string()
    }))
}

pub(crate) fn agent_use_validate_edit_packet_json(
    profile: &AgentUseProfile,
    options: &AgentUseValidateEditOptions,
    source_update: Value,
) -> Value {
    let changed_files =
        agent_use_validate_edit_changed_files(&profile.repo_root, options, &source_update);
    let validation_packet =
        agent_use_validate_edit_validation_packet(&source_update, &changed_files);
    let validation_status = validation_packet
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("diagnostic_only");
    let preflight_blocked = source_update.get("validation_packet").is_none();
    let status = if preflight_blocked {
        "preflight_blocked".to_string()
    } else {
        validation_status.to_string()
    };
    let must_fix_before_continuing = validation_packet
        .get("must_fix_before_continuing")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || preflight_blocked;
    let rejected_paths = source_update
        .get("rejected_paths")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let no_op_paths = source_update
        .get("no_op_paths")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let claimability = source_update
        .get("claimability")
        .cloned()
        .or_else(|| validation_packet.get("claimability").cloned())
        .unwrap_or_else(agent_use_validate_edit_default_non_claimable);
    let lifecycle = source_update
        .get("lifecycle")
        .cloned()
        .or_else(|| source_update.get("watch_db").cloned())
        .or_else(|| validation_packet.get("lifecycle").cloned())
        .unwrap_or_else(|| json!({"claimable": false, "diagnostic_only": true}));
    let recovery_commands = source_update
        .get("recovery_commands")
        .cloned()
        .unwrap_or_else(|| json!(profile.recovery_commands.clone()));
    let expected_touched_files =
        agent_use_report_changed_paths(&profile.repo_root, &options.expected_touched_files);
    let changed_set = changed_files.iter().cloned().collect::<BTreeSet<_>>();
    let expected_touched_files_missing = expected_touched_files
        .iter()
        .filter(|path| !changed_set.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    let source_update_packet = if options.detail_mode.preserves_full_details() {
        source_update.clone()
    } else {
        Value::Null
    };
    let validation_state = source_update
        .get("validation_state")
        .cloned()
        .unwrap_or(Value::Null);
    let journal_replay = source_update
        .get("journal_replay")
        .cloned()
        .unwrap_or(Value::Null);
    let validation_wall_bounded = source_update
        .get("validation_wall_bounded")
        .cloned()
        .unwrap_or(Value::Bool(false));
    // Small (~600 B) per-substage attribution block; copied to the top level
    // because the budget enforcer nulls source_update_packet under pressure.
    let validation_substage_summary = source_update
        .get("validation_substage_summary")
        .cloned()
        .unwrap_or(Value::Null);
    let mut value = json!({
        "schema_version": 1,
        "schema_name": "validate_edit_agent_json",
        "packet_kind": "validate_edit_packet",
        "validation_state": validation_state,
        "journal_replay": journal_replay,
        "validation_wall_bounded": validation_wall_bounded,
        "validation_substage_summary": validation_substage_summary,
        "command": "validate-edit",
        "command_namespace": "agent-use",
        "agent_use_command": "agent-use validate-edit",
        "canonical_cli_surface": format!("{BIN_NAME} agent-use validate-edit --repo <repo> --changed <path> --agent-json"),
        "compatibility_alias_status": "deferred",
        "status": status,
        "final_status": validation_packet
            .get("final_status")
            .cloned()
            .unwrap_or_else(|| json!(validation_status)),
        "severity_summary": validation_packet
            .get("severity_summary")
            .cloned()
            .unwrap_or_else(|| json!({})),
        "can_continue_with_caution": validation_packet
            .get("can_continue_with_caution")
            .cloned()
            .unwrap_or_else(|| json!(false)),
        "should_recover_tool_state": validation_packet
            .get("should_recover_tool_state")
            .cloned()
            .unwrap_or_else(|| json!(preflight_blocked)),
        "should_run_tests": validation_packet
            .get("should_run_tests")
            .cloned()
            .unwrap_or_else(|| json!(false)),
        "should_request_explain": validation_packet
            .get("should_request_explain")
            .cloned()
            .unwrap_or_else(|| json!(preflight_blocked)),
        "should_rerun_validation": validation_packet
            .get("should_rerun_validation")
            .cloned()
            .unwrap_or_else(|| json!(preflight_blocked)),
        "next_agent_action": validation_packet
            .get("next_agent_action")
            .cloned()
            .unwrap_or_else(|| {
                if preflight_blocked {
                    json!("recover_tool_state_then_rerun_validation")
                } else {
                    json!("continue")
                }
            }),
        "aggregate_guidance": validation_packet
            .get("aggregate_guidance")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "repo": path_string(&profile.repo_root),
        "repo_root": path_string(&profile.repo_root),
        "db": path_string(&profile.db_path),
        "db_path": path_string(&profile.db_path),
        "resolved_db": path_string(&profile.db_path),
        "db_source": "agent-use profile",
        "profile_name": profile.profile_name.clone(),
        "repo_identity_label": profile.repo_identity_label.clone(),
        "repo_identity_hash": profile.repo_identity_hash.clone(),
        "repo_identity_short_hash": agent_use_repo_identity_short_hash(profile),
        "uses_production_agent_use_resolver": true,
        "external_profile_db_used": true,
        "external_db_used": true,
        "no_dot_codegraph_fallback": true,
        "normal_dot_codegraph_mutated": source_update.get("normal_dot_codegraph_mutated").cloned().unwrap_or_else(|| json!(false)),
        "public_claim": false,
        "changed_files": changed_files,
        "changed_paths": source_update.get("changed_paths").cloned().unwrap_or_else(|| json!([])),
        "normalized_changed_files": source_update.get("normalized_changed_files").cloned().unwrap_or_else(|| json!([])),
        "changed_paths_requested": source_update.get("changed_paths_requested").cloned().unwrap_or_else(|| json!([])),
        "rejected_paths": rejected_paths,
        "no_op_paths": no_op_paths,
        "deleted_paths": source_update.get("deleted_paths").cloned().unwrap_or_else(|| json!([])),
        "renamed_paths": source_update.get("renamed_paths").cloned().unwrap_or_else(|| json!([])),
        "ignored_paths": source_update.get("ignored_paths").cloned().unwrap_or_else(|| json!([])),
        "generated_paths": source_update.get("generated_paths").cloned().unwrap_or_else(|| json!([])),
        "outside_repo_paths": source_update.get("outside_repo_paths").cloned().unwrap_or_else(|| json!([])),
        "duplicate_paths": source_update.get("duplicate_paths").cloned().unwrap_or_else(|| json!([])),
        "atomic_temp_paths": source_update.get("atomic_temp_paths").cloned().unwrap_or_else(|| json!([])),
        "partial_input_failures_reported": source_update.get("partial_input_failures_reported").cloned().unwrap_or_else(|| json!(false)),
        "too_many_changed_files": source_update.get("too_many_changed_files").cloned().unwrap_or_else(|| json!(false)),
        "max_changed_files": source_update.get("max_changed_files").cloned().unwrap_or_else(|| json!(AGENT_USE_WATCH_DEFAULT_MAX_BATCH_PATHS)),
        "no_silent_path_drops": source_update.get("no_silent_path_drops").cloned().unwrap_or_else(|| json!(true)),
        "input_policy": source_update.get("input_policy").cloned().unwrap_or_else(|| json!("run_unique_in_repo_updateable_paths; report_invalid_duplicate_noop_paths_explicitly")),
        "rename_policy": source_update.get("rename_policy").cloned().unwrap_or_else(|| json!("explicit rename pairs are not part of the current API; pass old deleted path and new added path in the same changed set")),
        "per_file_status": source_update.get("per_file_status").cloned().unwrap_or_else(|| json!([])),
        "input_diagnostics": source_update.get("input_diagnostics").cloned().unwrap_or_else(|| json!([])),
        "input_warnings": source_update.get("input_warnings").cloned().unwrap_or_else(|| json!([])),
        "validation_packet": validation_packet,
        "hard_interrupt_available": source_update
            .get("hard_interrupt_available")
            .cloned()
            .or_else(|| source_update.pointer("/validation_packet/hard_interrupt_available").cloned())
            .unwrap_or_else(|| json!(false)),
        "hard_interrupt": source_update
            .get("hard_interrupt")
            .cloned()
            .or_else(|| source_update.pointer("/validation_packet/hard_interrupt").cloned())
            .unwrap_or(Value::Null),
        "must_fix_before_continuing": must_fix_before_continuing,
        // §1.3.5 compact block; copied to the top level (like
        // validation_substage_summary) because the budget enforcer nulls
        // nested detail under pressure and the counts must survive.
        "unresolved_references": source_update
            .pointer("/validation_packet/unresolved_references")
            .cloned()
            .unwrap_or(Value::Null),
        "warnings": source_update
            .pointer("/validation_packet/warnings")
            .cloned()
            .or_else(|| source_update.get("warnings").cloned())
            .or_else(|| source_update.get("input_warnings").cloned())
            .unwrap_or_else(|| json!([])),
        "unknowns": source_update
            .pointer("/validation_packet/unknowns")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "diagnostics": source_update
            .pointer("/validation_packet/diagnostics")
            .cloned()
            .or_else(|| source_update.get("diagnostics").cloned())
            .or_else(|| source_update.get("input_diagnostics").cloned())
            .unwrap_or_else(|| json!([])),
        "claimability": claimability,
        "lifecycle": lifecycle,
        "staged_availability": source_update
            .get("staged_availability")
            .cloned()
            .unwrap_or(Value::Null),
        "proof_ladder_changes": source_update
            .pointer("/validation_packet/proof_ladder_changes")
            .cloned()
            .or_else(|| source_update.get("proof_ladder_changes").cloned())
            .unwrap_or_else(|| json!({})),
        "recovery_commands": recovery_commands,
        "validation_recovery_commands": validation_packet
            .get("recovery_commands")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "omitted_count": source_update
            .pointer("/validation_packet/omitted_count")
            .and_then(Value::as_u64)
            .or_else(|| source_update.get("omitted_count").and_then(Value::as_u64))
            .unwrap_or(0),
        "expansion_handles": source_update
            .pointer("/validation_packet/expansion_handles")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "timings": source_update
            .get("timings")
            .cloned()
            .unwrap_or_else(|| json!({})),
        "command_rerun_hint": agent_use_validate_edit_rerun_hint(profile, options),
        "detail_mode": options.detail_mode.label(),
        "compact_default": matches!(options.detail_mode, AgentUseDetailMode::Compact),
        "fail_on_blocking": options.fail_on_blocking,
        "fail_on_blocking_exit_code": 2,
        "default_exit_zero_on_validation_blocker": true,
        "json_printed_on_blocking": true,
        "task_id": options.task_id.clone(),
        "edit_intent": options.edit_intent.clone(),
        "expected_touched_files": expected_touched_files,
        "expected_touched_files_missing": expected_touched_files_missing,
        "source_update_status": source_update.get("status").cloned().unwrap_or_else(|| json!("unknown")),
        "source_update_command": source_update.get("agent_use_command").cloned().unwrap_or_else(|| json!("agent-use watch")),
        "source_update_packet": source_update_packet,
        "metrics": {
            "files_walked": source_update.get("files_walked").cloned().unwrap_or_else(|| json!(0)),
            "files_read": source_update.get("files_read").cloned().unwrap_or_else(|| json!(0)),
            "files_hashed": source_update.get("files_hashed").cloned().unwrap_or_else(|| json!(0)),
            "files_parsed": source_update.get("files_parsed").cloned().unwrap_or_else(|| json!(0)),
            "facts_deleted": source_update.get("facts_deleted").cloned().unwrap_or_else(|| json!(0)),
            "facts_inserted": source_update.get("facts_inserted").cloned().unwrap_or_else(|| json!(0)),
            "entities_added": source_update.get("entities_added").cloned().unwrap_or_else(|| json!(0)),
            "entities_removed": source_update.get("entities_removed").cloned().unwrap_or_else(|| json!(0)),
            "entities_changed": source_update.get("entities_changed").cloned().unwrap_or_else(|| json!(0)),
            "edges_added": source_update.get("edges_added").cloned().unwrap_or_else(|| json!(0)),
            "edges_removed": source_update.get("edges_removed").cloned().unwrap_or_else(|| json!(0)),
            "edges_changed": source_update.get("edges_changed").cloned().unwrap_or_else(|| json!(0)),
        },
    });
    agent_use_validate_edit_add_mode_aware_severity_fields(&mut value, options.detail_mode);
    add_agent_use_dirty_evidence_output_fields(
        &mut value,
        "agent-use.validate-edit",
        options.detail_mode.preserves_full_details(),
    );
    value
}

fn agent_use_validate_edit_add_mode_aware_severity_fields(
    packet: &mut Value,
    detail_mode: AgentUseDetailMode,
) {
    let validation_packet = packet
        .get("validation_packet")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let severity_summary = validation_packet
        .get("severity_summary")
        .cloned()
        .or_else(|| packet.get("severity_summary").cloned())
        .unwrap_or_else(|| json!({}));
    let final_status = validation_packet
        .get("final_status")
        .or_else(|| packet.get("final_status"))
        .or_else(|| validation_packet.get("status"))
        .or_else(|| packet.get("status"))
        .cloned()
        .unwrap_or_else(|| json!("unknown"));
    let final_severity = severity_summary
        .get("max_severity")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| final_status.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    let blocking_count =
        agent_use_validate_edit_bucket_count(&validation_packet, "blocking_errors");
    let warning_count = agent_use_validate_edit_bucket_count(&validation_packet, "warnings");
    let unknown_count = agent_use_validate_edit_bucket_count(&validation_packet, "unknowns");
    let diagnostic_count = agent_use_validate_edit_bucket_count(&validation_packet, "diagnostics");
    let editor_policy = validation_packet
        .get("editor_policy")
        .cloned()
        .unwrap_or_else(|| {
            agent_use_validate_edit_editor_policy_json(
                final_status.as_str().unwrap_or("unknown"),
                packet
                    .get("must_fix_before_continuing")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                packet
                    .get("hard_interrupt_available")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                packet
                    .get("should_request_explain")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                packet
                    .get("should_rerun_validation")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )
        });
    let severity_trace = if detail_mode.preserves_full_details() {
        json!({
            "mode": detail_mode.label(),
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
                "hard_interrupt_available": packet
                    .get("hard_interrupt_available")
                    .cloned()
                    .unwrap_or_else(|| json!(false)),
                "hard_interrupt_requires_interrupt_eligible_blocking": true,
                "normal_validation_blocker_is_tool_error": false
            },
            "tool_error_vs_validation_blocker": {
                "validation_blockers_return_json": true,
                "mcp_tool_error_reserved_for_runtime_protocol_failure": true,
                "cli_runtime_failure_exit_nonzero": true
            },
            "lifecycle_claimability_details": {
                "claimability": packet.get("claimability").cloned().unwrap_or_else(|| json!({})),
                "lifecycle": packet.get("lifecycle").cloned().unwrap_or_else(|| json!({})),
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
            "mode": detail_mode.label(),
            "summary_only": true,
            "request_full_trace_with": "--explain or --audit-json",
            "full_source_bodies_included": false,
            "full_graph_dump_included": false,
        })
    };
    let proof_ladder_changes_summary = agent_use_validate_edit_proof_ladder_summary(
        &validation_packet
            .get("proof_ladder_changes")
            .cloned()
            .or_else(|| packet.get("proof_ladder_changes").cloned())
            .unwrap_or(Value::Null),
    );

    if let Some(object) = packet.as_object_mut() {
        object.insert("final_severity".to_string(), json!(final_severity));
        object.insert(
            "stale_unsafe_blockers".to_string(),
            validation_packet
                .get("stale_unsafe_blockers")
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
        object.insert("blocking_error_count".to_string(), json!(blocking_count));
        object.insert("warning_count".to_string(), json!(warning_count));
        object.insert("unknown_count".to_string(), json!(unknown_count));
        object.insert("diagnostic_count".to_string(), json!(diagnostic_count));
        object.insert(
            "validation_blocking_error_count".to_string(),
            json!(blocking_count),
        );
        object.insert("validation_warning_count".to_string(), json!(warning_count));
        object.insert("validation_unknown_count".to_string(), json!(unknown_count));
        object.insert(
            "validation_diagnostic_count".to_string(),
            json!(diagnostic_count),
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

fn agent_use_validate_edit_bucket_count(validation_packet: &Value, key: &str) -> usize {
    validation_packet
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_else(|| {
            validation_packet
                .get(match key {
                    "blocking_errors" => "blocking_error_count",
                    "warnings" => "warning_count",
                    "unknowns" => "unknown_count",
                    "diagnostics" => "diagnostic_count",
                    _ => key,
                })
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize
        })
}

fn agent_use_validate_edit_editor_policy_json(
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

fn agent_use_validate_edit_proof_ladder_summary(proof_ladder_changes: &Value) -> Value {
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

fn agent_use_validate_edit_changed_files(
    repo_root: &Path,
    options: &AgentUseValidateEditOptions,
    source_update: &Value,
) -> Vec<String> {
    source_update
        .get("normalized_changed_files")
        .or_else(|| source_update.get("changed_files"))
        .or_else(|| source_update.get("changed_paths"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| agent_use_report_changed_paths(repo_root, &options.changed_paths))
}

fn agent_use_validate_edit_validation_packet(
    source_update: &Value,
    changed_files: &[String],
) -> Value {
    if let Some(packet) = source_update
        .get("validation_packet")
        .filter(|packet| packet.is_object())
    {
        return packet.clone();
    }
    let claimability = source_update
        .get("claimability")
        .cloned()
        .unwrap_or_else(agent_use_validate_edit_default_non_claimable);
    let lifecycle = source_update
        .get("lifecycle")
        .cloned()
        .or_else(|| source_update.get("watch_db").cloned())
        .unwrap_or_else(|| json!({"claimable": false, "diagnostic_only": true}));
    let stale_unsafe_blockers = agent_use_validate_edit_preflight_blockers(source_update);
    let mut recommended_next_steps = source_update
        .get("recovery_commands")
        .and_then(Value::as_array)
        .map(|commands| {
            commands
                .iter()
                .filter_map(Value::as_str)
                .take(3)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if recommended_next_steps.is_empty() {
        recommended_next_steps.push(
            "run agent-use index for this repo, then rerun agent-use validate-edit".to_string(),
        );
    }
    json!({
        "schema_version": 1,
        "packet_kind": "graph_validation_packet",
        "status": "diagnostic_only",
        "final_status": "diagnostic_only",
        "must_fix_before_continuing": false,
        "can_continue_with_caution": false,
        "should_recover_tool_state": true,
        "should_run_tests": false,
        "should_request_explain": true,
        "should_rerun_validation": true,
        "next_agent_action": "recover_tool_state_then_rerun_validation",
        "recovery_commands": recommended_next_steps.clone(),
        "aggregate_guidance": [
            "Recover non-claimable tool state; diagnostic output is not source-code proof."
        ],
        "changed_files": changed_files,
        "graph_delta": {},
        "blocking_errors": [],
        "warnings": [],
        "unknowns": [],
        "diagnostics": [],
        "summary_counts_by_rule_id": {},
        "summary_counts_by_classification": {},
        "summary_counts_by_relation_kind": {},
        "validation_rules_evaluated": [],
        "validation_rules_skipped": [],
        "relation_family_status": {},
        "activation_gate_state": {},
        "claimability": claimability,
        "proof_ladder_changes": source_update.get("proof_ladder_changes").cloned().unwrap_or_else(|| json!({})),
        "lifecycle": lifecycle,
        "stale_unsafe_blockers": stale_unsafe_blockers,
        "top_blocking_source_spans": [],
        "recommended_next_steps": recommended_next_steps,
        "severity_summary": {
            "schema_version": 1,
            "final_status": "diagnostic_only",
            "validation_packet_status": "diagnostic_only",
            "max_severity": "diagnostic_only",
            "tool_error": false,
            "hard_interrupt_available": false,
            "hard_interrupt": false,
            "must_fix_before_continuing": false,
            "can_continue_with_caution": false,
            "should_recover_tool_state": true,
            "should_run_tests": false,
            "should_request_explain": true,
            "should_rerun_validation": true,
            "next_agent_action": "recover_tool_state_then_rerun_validation",
            "decision_count": 1,
            "counts_by_severity": {"diagnostic_only": 1},
            "status_precedence": ["tool_error", "blocking_graph_error", "warning", "unknown", "diagnostic_only", "ok"],
            "public_claim": false
        },
        "severity_decisions": [],
        "severity_aggregation_trace": {
            "schema_version": 1,
            "decisions": [],
            "final_status": "diagnostic_only",
            "max_severity": "diagnostic_only",
            "hard_interrupt_available": false,
            "tool_error": false,
            "notes": [
                "synthetic validate-edit preflight packet; validation did not run"
            ]
        },
        "editor_policy": {
            "editor_policy_version": 1,
            "recommended_editor_action": "show_tool_state_recovery",
            "should_show_modal": false,
            "should_show_warning_panel": true,
            "should_allow_continue": true,
            "should_request_revalidation": true,
            "safe_to_autofix": false,
            "source_edits_performed": false,
            "daemon_integration_available": false,
            "plugin_integration_available": false,
            "metadata_advisory_only": true,
            "no_automatic_source_edits": true,
            "unsafe_db_states_are_not_source_code_hard_interrupts": true
        },
        "hard_interrupt_available": false,
        "hard_interrupt": Value::Null,
        "omitted_count": 0,
        "expansion_handles": [],
    })
}

fn agent_use_validate_edit_preflight_blockers(source_update: &Value) -> Vec<String> {
    let mut blockers = Vec::new();
    for key in ["errors", "blockers"] {
        if let Some(values) = source_update.get(key).and_then(Value::as_array) {
            for value in values {
                if let Some(message) = value.as_str() {
                    blockers.push(message.to_string());
                } else if !value.is_null() {
                    blockers.push(value.to_string());
                }
            }
        }
    }
    if blockers.is_empty() {
        if let Some(reason) = source_update.get("reason").and_then(Value::as_str) {
            blockers.push(reason.to_string());
        } else if let Some(status) = source_update.get("status").and_then(Value::as_str) {
            blockers.push(format!("validation_not_run:{status}"));
        }
    }
    blockers.sort();
    blockers.dedup();
    blockers
}

fn agent_use_validate_edit_default_non_claimable() -> Value {
    json!({
        "claimable": false,
        "diagnostic_only": true,
        "candidate_only": false,
        "graph_proof_available": false,
        "graph_proof_only_from_graph_source_verification": true,
        "text_evidence_is_not_graph_proof": true,
        "vector_evidence_is_not_graph_proof": true,
        "candidate_evidence_is_not_graph_proof": true,
    })
}

fn agent_use_validate_edit_rerun_hint(
    profile: &AgentUseProfile,
    options: &AgentUseValidateEditOptions,
) -> String {
    let mut command = format!(
        "{BIN_NAME} agent-use validate-edit --repo \"{}\"",
        path_string(&profile.repo_root)
    );
    for changed_path in &options.changed_paths {
        command.push_str(&format!(" --changed \"{}\"", path_string(changed_path)));
    }
    match options.detail_mode {
        AgentUseDetailMode::Compact => command.push_str(" --agent-json"),
        AgentUseDetailMode::Explain => command.push_str(" --explain"),
        AgentUseDetailMode::Audit => command.push_str(" --audit-json"),
    }
    if options.fail_on_blocking {
        command.push_str(" --fail-on-blocking");
    }
    command
}

fn agent_use_validate_edit_finalize_budget(
    packet: &mut Value,
    detail_mode: AgentUseDetailMode,
    max_output_bytes: usize,
) {
    let initial_output_bytes = serialized_json_len(packet);
    let mut output_truncated = false;
    let mut omitted_by_enforcer = 0u64;
    if initial_output_bytes > max_output_bytes && !detail_mode.preserves_full_details() {
        output_truncated = true;
        agent_use_validate_edit_enforce_compact_budget(
            packet,
            max_output_bytes,
            initial_output_bytes,
            &mut omitted_by_enforcer,
        );
        agent_use_validate_edit_add_mode_aware_severity_fields(packet, detail_mode);
        agent_use_validate_edit_enforce_compact_budget(
            packet,
            max_output_bytes,
            initial_output_bytes,
            &mut omitted_by_enforcer,
        );
    } else if initial_output_bytes > max_output_bytes {
        output_truncated = true;
        if let Some(object) = packet.as_object_mut() {
            object.insert("output_truncated".to_string(), json!(true));
            object.insert("max_output_bytes".to_string(), json!(max_output_bytes));
            object.insert(
                "pre_truncation_bytes".to_string(),
                json!(initial_output_bytes),
            );
        }
    }
    let output_bytes = serialized_json_len(packet);
    if let Some(object) = packet.as_object_mut() {
        let total_omitted = object
            .get("omitted_count")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            .saturating_add(omitted_by_enforcer);
        object.insert("omitted_count".to_string(), json!(total_omitted));
        object.insert(
            "agent_json_budget".to_string(),
            json!({
                "mode": detail_mode.label(),
                "max_output_bytes": max_output_bytes,
                "output_bytes": output_bytes,
                "pre_truncation_bytes": initial_output_bytes,
                "truncated": output_truncated,
                "output_truncated": output_truncated,
                "omitted_count": total_omitted,
                "max_output_bytes_exceeded": output_bytes > max_output_bytes,
                "required_safety_fields_preserved": true,
                "evidence_first_shedding": true,
                "metadata_shed_before_evidence": true,
            }),
        );
    }
    let final_output_bytes = serialized_json_len(packet);
    if let Some(object) = packet.as_object_mut() {
        if let Some(budget) = object
            .get_mut("agent_json_budget")
            .and_then(Value::as_object_mut)
        {
            budget.insert("output_bytes".to_string(), json!(final_output_bytes));
            budget.insert(
                "max_output_bytes_exceeded".to_string(),
                json!(final_output_bytes > max_output_bytes),
            );
        }
    }
}

fn agent_use_validate_edit_truncate_validation_packet(packet: &mut Value, limit: usize) {
    let mut omitted = packet
        .get("omitted_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if let Some(object) = packet.as_object_mut() {
        for key in ["blocking_errors", "warnings", "unknowns", "diagnostics"] {
            if let Some(items) = object.get_mut(key).and_then(Value::as_array_mut) {
                if items.len() > limit {
                    omitted += (items.len() - limit) as u64;
                    items.truncate(limit);
                }
            }
        }
        object.insert("omitted_count".to_string(), json!(omitted));
        if omitted > 0 {
            object.insert(
                "expansion_handles".to_string(),
                json!(["validation_packet:full"]),
            );
        }
        object.insert("critical_safety_fields_preserved".to_string(), json!(true));
        object.insert("full_graph_dump_included".to_string(), json!(false));
        object.insert("full_source_bodies_included".to_string(), json!(false));
    }
}

fn agent_use_validate_edit_enforce_compact_budget(
    packet: &mut Value,
    max_output_bytes: usize,
    pre_truncation_bytes: usize,
    omitted_count: &mut u64,
) {
    if let Some(object) = packet.as_object_mut() {
        object.insert("source_update_packet".to_string(), Value::Null);
        object.insert("output_truncated".to_string(), json!(true));
        object.insert("max_output_bytes".to_string(), json!(max_output_bytes));
        object.insert(
            "pre_truncation_bytes".to_string(),
            json!(pre_truncation_bytes),
        );
        object.insert(
            "compact_contract".to_string(),
            json!({
                "evidence_first_shedding": true,
                "metadata_shed_before_evidence": true,
                "findings_anchor": "validation_packet",
                "recovery_commands_ref": "validation_recovery_commands",
            }),
        );
        if let Some(lifecycle) = object.get("lifecycle").cloned() {
            object.insert(
                "lifecycle".to_string(),
                compact_agent_use_public_lifecycle_summary(&compact_agent_use_lifecycle_summary(
                    &lifecycle,
                )),
            );
        }
        if let Some(staged) = object.get("staged_availability").cloned() {
            object.insert(
                "staged_availability".to_string(),
                compact_agent_use_staged_availability_summary(&staged),
            );
        }
        if let Some(severity_summary) = object.get("severity_summary").cloned() {
            object.insert(
                "severity_summary".to_string(),
                agent_use_validate_edit_compact_severity_summary(&severity_summary),
            );
        }
        for key in ["warnings", "unknowns", "diagnostics"] {
            if let Some(items) = object.get_mut(key).and_then(Value::as_array_mut) {
                if items.len() > 3 {
                    *omitted_count = omitted_count.saturating_add((items.len() - 3) as u64);
                    items.truncate(3);
                }
                for item in items {
                    agent_use_validate_edit_compact_finding(item);
                }
            }
        }
        if let Some(validation_packet) = object.get_mut("validation_packet") {
            agent_use_validate_edit_truncate_validation_packet(validation_packet, 3);
            agent_use_validate_edit_compact_validation_packet(validation_packet);
        }
        if let Some(unresolved) = object.get_mut("unresolved_references") {
            agent_use_validate_edit_compact_unresolved_references(unresolved, 3);
        }
        let validation_omitted = object
            .get("validation_packet")
            .and_then(|packet| packet.get("omitted_count"))
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let prior_omitted = object
            .get("omitted_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        object.insert(
            "omitted_count".to_string(),
            json!(prior_omitted.saturating_add(validation_omitted)),
        );
        let expansion_handles = object
            .get("validation_packet")
            .and_then(|packet| packet.get("expansion_handles"))
            .cloned()
            .unwrap_or_else(|| json!(["validation_packet:full"]));
        object.insert("expansion_handles".to_string(), expansion_handles);
    }

    for _ in 0..128 {
        if serialized_json_len(packet) <= max_output_bytes.saturating_sub(1024) {
            break;
        }
        if agent_use_validate_edit_compact_dirty_evidence(packet) {
            *omitted_count = omitted_count.saturating_add(1);
            continue;
        }
        if agent_use_validate_edit_compact_hard_interrupt(packet) {
            *omitted_count = omitted_count.saturating_add(1);
            continue;
        }
        let mut removed = false;
        for key in [
            "command_rerun_hint",
            "canonical_cli_surface",
            "compatibility_alias_status",
            "agent_use_command",
            "repo_root",
            "resolved_db",
            "uses_production_agent_use_resolver",
            "external_profile_db_used",
            "external_db_used",
            "aggregate_guidance",
            "source_update_status",
            "source_update_command",
            "detail_mode",
            "compact_default",
            "default_exit_zero_on_validation_blocker",
            "json_printed_on_blocking",
            "fail_on_blocking",
            "fail_on_blocking_exit_code",
            "max_changed_files",
            "partial_input_failures_reported",
            "too_many_changed_files",
            "task_id",
            "edit_intent",
            "expected_touched_files",
            "expected_touched_files_missing",
            "changed_paths",
            "changed_paths_requested",
            "normalized_changed_files",
            "input_policy",
            "rename_policy",
            "no_silent_path_drops",
            "no_dot_codegraph_fallback",
            "staged_availability",
            "warnings",
            "unknowns",
            "diagnostics",
            "severity_trace",
            "proof_ladder_changes",
            "validation_recovery_commands",
            "recovery_commands",
            "timings",
            "metrics",
            "per_file_status",
            "input_diagnostics",
            "input_warnings",
            "atomic_temp_paths",
            "ignored_paths",
            "generated_paths",
            "outside_repo_paths",
            "duplicate_paths",
            "rejected_paths",
            "no_op_paths",
            "deleted_paths",
            "renamed_paths",
            "validation_substage_summary",
            "editor_policy",
        ] {
            if agent_use_remove_field(packet, key) {
                *omitted_count = omitted_count.saturating_add(1);
                removed = true;
                break;
            }
        }
        if removed {
            continue;
        }
        if agent_use_validate_edit_pop_array(packet, "warnings", 1)
            || agent_use_validate_edit_pop_array(packet, "unknowns", 1)
            || agent_use_validate_edit_pop_array(packet, "diagnostics", 1)
        {
            *omitted_count = omitted_count.saturating_add(1);
            continue;
        }
        if let Some(validation_packet) = packet.get_mut("validation_packet") {
            if agent_use_validate_edit_pop_array(validation_packet, "warnings", 1)
                || agent_use_validate_edit_pop_array(validation_packet, "unknowns", 1)
                || agent_use_validate_edit_pop_array(validation_packet, "diagnostics", 1)
                || agent_use_validate_edit_pop_array(validation_packet, "blocking_errors", 1)
            {
                *omitted_count = omitted_count.saturating_add(1);
                continue;
            }
        }
        if agent_use_validate_edit_shrink_unresolved_references(packet, 3) {
            *omitted_count = omitted_count.saturating_add(1);
            continue;
        }
        if let Some(validation_packet) = packet.get_mut("validation_packet") {
            if agent_use_validate_edit_shrink_unresolved_references(validation_packet, 3) {
                *omitted_count = omitted_count.saturating_add(1);
                continue;
            }
        }
        break;
    }
    if let Some(object) = packet.as_object_mut() {
        object
            .entry("stale_unsafe_blockers".to_string())
            .or_insert_with(|| json!([]));
        object
            .entry("severity_trace".to_string())
            .or_insert_with(|| {
                json!({
                    "mode": "compact",
                    "summary_only": true,
                    "request_full_trace_with": "--explain or --audit-json",
                    "full_source_bodies_included": false,
                    "full_graph_dump_included": false,
                })
            });
        let editor_policy = agent_use_validate_edit_editor_policy_json(
            object
                .get("final_status")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            object
                .get("must_fix_before_continuing")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            object
                .get("hard_interrupt_available")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            object
                .get("should_request_explain")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            object
                .get("should_rerun_validation")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        );
        object
            .entry("editor_policy".to_string())
            .or_insert(editor_policy);
    }
}

fn agent_use_validate_edit_compact_validation_packet(packet: &mut Value) {
    if let Some(object) = packet.as_object_mut() {
        if let Some(severity_summary) = object.get("severity_summary").cloned() {
            object.insert(
                "severity_summary".to_string(),
                agent_use_validate_edit_compact_severity_summary(&severity_summary),
            );
        }
        if let Some(graph_delta) = object.get("graph_delta").cloned() {
            object.insert(
                "graph_delta".to_string(),
                json!({
                    "summary": agent_use_validate_edit_compact_graph_delta_summary(&graph_delta),
                    "full_detail_handle": "validation_packet.graph_delta",
                    "agent_json_compacted": true,
                }),
            );
        }
        if let Some(unresolved) = object.get_mut("unresolved_references") {
            agent_use_validate_edit_compact_unresolved_references(unresolved, 3);
        }
        if let Some(lifecycle) = object.get("lifecycle").cloned() {
            object.insert(
                "lifecycle".to_string(),
                compact_agent_use_public_lifecycle_summary(&compact_agent_use_lifecycle_summary(
                    &lifecycle,
                )),
            );
        }
        if let Some(claimability) = object.get("claimability").cloned() {
            object.insert(
                "claimability".to_string(),
                json!({
                    "claimable": claimability.get("claimable").cloned().unwrap_or(Value::Null),
                    "current": claimability.get("current").cloned().unwrap_or(Value::Null),
                    "diagnostic_only": claimability.get("diagnostic_only").cloned().unwrap_or(Value::Null),
                    "non_claimable_reason": claimability.get("non_claimable_reason").cloned().unwrap_or(Value::Null),
                    "agent_json_compacted": true,
                }),
            );
        }
        if let Some(dirty_summary) = object
            .get_mut("dirty_evidence_summary")
            .and_then(Value::as_object_mut)
        {
            dirty_summary.insert("summary_only".to_string(), json!(true));
            dirty_summary.insert("agent_json_compacted".to_string(), json!(true));
            dirty_summary.remove("exact_db_path_checked");
            dirty_summary.remove("db_path");
        }
        for (key, reference) in [
            ("dirty_evidence_summary", "dirty_evidence_summary"),
            ("lifecycle", "lifecycle"),
            ("claimability", "claimability"),
            (
                "proof_ladder_changes_summary",
                "proof_ladder_changes_summary",
            ),
        ] {
            if object.remove(key).is_some() {
                object.insert(format!("{key}_ref"), json!(reference));
            }
        }
        if let Some(evaluated) = object.get("validation_rules_evaluated").cloned() {
            let count = evaluated
                .get("count")
                .and_then(Value::as_u64)
                .or_else(|| {
                    evaluated
                        .get("rule_ids")
                        .and_then(Value::as_array)
                        .map(|ids| ids.len() as u64)
                })
                .unwrap_or_default();
            object.insert(
                "validation_rules_evaluated".to_string(),
                json!({
                    "count": count,
                    "full_detail_handle": "validation_packet.validation_rules_evaluated",
                    "agent_json_compacted": true,
                }),
            );
        }
        if let Some(skipped) = object.get("validation_rules_skipped").cloned() {
            let count = skipped
                .as_object()
                .map(|rules| rules.len() as u64)
                .or_else(|| skipped.get("count").and_then(Value::as_u64))
                .unwrap_or_default();
            object.insert(
                "validation_rules_skipped".to_string(),
                json!({
                    "count": count,
                    "full_detail_handle": "validation_packet.validation_rules_skipped",
                    "agent_json_compacted": true,
                }),
            );
        }
        if let Some(proof_ladder) = object.get("proof_ladder_changes").cloned() {
            object.insert(
                "proof_ladder_changes".to_string(),
                agent_use_validate_edit_proof_ladder_summary(&proof_ladder),
            );
        }
        for key in ["blocking_errors", "warnings", "unknowns", "diagnostics"] {
            if let Some(items) = object.get_mut(key).and_then(Value::as_array_mut) {
                for item in items {
                    agent_use_validate_edit_compact_finding(item);
                }
            }
        }
        if object.remove("hard_interrupt").is_some() {
            object.insert("hard_interrupt_ref".to_string(), json!("hard_interrupt"));
        }
        for key in [
            "severity_decisions",
            "severity_aggregation_trace",
            "activation_gate_state",
            "relation_family_status",
            "editor_policy",
            "recovery_commands",
            "refreshed_evidence",
            "stale_evidence",
            "unavailable_evidence",
            "invalidated_evidence",
            "proof_ladder_change_counts",
            "sidecar_statuses",
            "severity_effect",
            "stale_non_proof_reasons",
            "aggregate_guidance",
        ] {
            object.remove(key);
        }
        object.insert(
            "recovery_commands_pointer".to_string(),
            json!("validation_recovery_commands"),
        );
        object.insert("critical_safety_fields_preserved".to_string(), json!(true));
        object.insert("full_graph_dump_included".to_string(), json!(false));
        object.insert("full_source_bodies_included".to_string(), json!(false));
    }
}

fn agent_use_validate_edit_compact_severity_summary(summary: &Value) -> Value {
    json!({
        "schema_version": summary.get("schema_version").cloned().unwrap_or_else(|| json!(1)),
        "final_status": summary.get("final_status").cloned().unwrap_or(Value::Null),
        "max_severity": summary.get("max_severity").cloned().unwrap_or(Value::Null),
        "hard_interrupt_available": summary.get("hard_interrupt_available").cloned().unwrap_or(Value::Null),
        "must_fix_before_continuing": summary.get("must_fix_before_continuing").cloned().unwrap_or(Value::Null),
        "counts_by_severity": summary.get("counts_by_severity").cloned().unwrap_or(Value::Null),
        "decision_count": summary.get("decision_count").cloned().unwrap_or(Value::Null),
        "public_claim": false,
        "agent_json_compacted": true,
    })
}

fn agent_use_validate_edit_compact_graph_delta_summary(graph_delta: &Value) -> Value {
    let summary = graph_delta.get("summary").unwrap_or(&Value::Null);
    let closure = graph_delta
        .get("closure_delta_summary")
        .unwrap_or(&Value::Null);
    json!({
        "entity_delta_count": summary.get("entity_delta_count").cloned().unwrap_or(Value::Null),
        "edge_delta_count": summary.get("edge_delta_count").cloned().unwrap_or(Value::Null),
        "source_span_delta_count": summary.get("source_span_delta_count").cloned().unwrap_or(Value::Null),
        "freshness_delta_count": summary.get("freshness_delta_count").cloned().unwrap_or(Value::Null),
        "closure_files_considered": closure
            .get("closure_files_considered")
            .and_then(Value::as_array)
            .map(|files| files.len())
            .or_else(|| closure.get("closure_files_considered").and_then(Value::as_u64).map(|count| count as usize))
            .unwrap_or_default(),
        "closure_budget_hit": closure.get("closure_budget_hit").cloned().unwrap_or(Value::Null),
        "status": closure.get("status").cloned().unwrap_or(Value::Null),
        "graph_proof": false,
        "agent_json_compacted": true,
    })
}

fn agent_use_validate_edit_compact_unresolved_references(value: &mut Value, preserve: usize) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let mut escalated_omitted = 0u64;
    if let Some(items) = object.get_mut("escalated").and_then(Value::as_array_mut) {
        let total = items.len();
        if total > preserve {
            items.truncate(preserve);
            escalated_omitted = total.saturating_sub(preserve) as u64;
        }
        for item in items {
            let Some(source) = item.as_object() else {
                continue;
            };
            let mut compact = serde_json::Map::new();
            for key in [
                "name",
                "relation",
                "reference_class",
                "file",
                "span",
                "severity",
                "proof_strength",
                "claimability",
                "repo_graph_lookup",
                "recommended_fix",
            ] {
                if let Some(value) = source.get(key) {
                    compact.insert(key.to_string(), value.clone());
                }
            }
            compact.insert("graph_proof".to_string(), json!(false));
            compact.insert("not_graph_proof".to_string(), json!(true));
            *item = Value::Object(compact);
        }
    }
    if escalated_omitted > 0 {
        let prior = object
            .get("escalated_omitted_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        object.insert(
            "escalated_omitted_count".to_string(),
            json!(prior.saturating_add(escalated_omitted)),
        );
    }
    object.insert("agent_json_compacted".to_string(), json!(true));
    object
        .entry("expansion_handle".to_string())
        .or_insert_with(|| json!("validation_packet:unresolved_references"));
}

fn agent_use_validate_edit_shrink_unresolved_references(
    value: &mut Value,
    preserve: usize,
) -> bool {
    let Some(unresolved) = value.get_mut("unresolved_references") else {
        return false;
    };
    let before = serialized_json_len(unresolved);
    agent_use_validate_edit_compact_unresolved_references(unresolved, preserve);
    serialized_json_len(unresolved) < before
}

fn agent_use_validate_edit_compact_finding(finding: &mut Value) {
    let Some(object) = finding.as_object() else {
        return;
    };
    let mut compact = serde_json::Map::new();
    for key in [
        "validation_rule_id",
        "severity",
        "classification",
        "message",
        "reason",
        "file",
        "source_span",
        "top_blocking_source_span",
        "blocking_level",
        "recommended_fix",
        "suggested_next_steps",
        "next_steps",
        "proof_level",
        "proof_strength",
        "graph_proof",
        "claimability_effect",
        "evidence_kind",
        "exactness",
        "integrity_kind",
        "expansion_handle",
    ] {
        if let Some(value) = object.get(key) {
            compact.insert(key.to_string(), value.clone());
        }
    }
    if let Some(diagnostics) = object.get("diagnostics").and_then(Value::as_array) {
        compact.insert(
            "diagnostics".to_string(),
            Value::Array(diagnostics.iter().take(1).cloned().collect()),
        );
    }
    let is_blocking = object
        .get("blocking_level")
        .and_then(Value::as_str)
        .is_some_and(|level| level == "block" || level == "blocking")
        || object
            .get("classification")
            .and_then(Value::as_str)
            .is_some_and(|classification| classification == "block");
    if is_blocking {
        if let Some(finding_id) = object.get("finding_id") {
            compact.insert("finding_id".to_string(), finding_id.clone());
        }
    }
    if is_blocking {
        if let Some(items) = object.get("evidence_items").and_then(Value::as_array) {
            let compact_items = items
                .iter()
                .take(1)
                .map(agent_use_validate_edit_compact_evidence_item)
                .collect::<Vec<_>>();
            compact.insert("evidence_items".to_string(), json!(compact_items));
        }
    }
    *finding = Value::Object(compact);
}

fn agent_use_validate_edit_compact_evidence_item(item: &Value) -> Value {
    json!({
        "evidence_id": item.get("evidence_id").cloned().unwrap_or(Value::Null),
        "evidence_kind": item.get("evidence_kind").cloned().unwrap_or(Value::Null),
        "proof_status": item.get("proof_status").cloned().unwrap_or(Value::Null),
        "graph_proof": item.get("graph_proof").cloned().unwrap_or(Value::Null),
        "claimable": item.get("claimable").cloned().unwrap_or(Value::Null),
    })
}

fn agent_use_validate_edit_compact_dirty_evidence(packet: &mut Value) -> bool {
    let mut changed = false;
    for key in [
        "invalidated_evidence",
        "refreshed_evidence",
        "stale_evidence",
        "unavailable_evidence",
        "sidecar_statuses",
        "stale_non_proof_reasons",
        "claimability_effect",
        "severity_effect",
        "proof_ladder_change_counts",
    ] {
        changed |= agent_use_remove_field(packet, key);
    }
    if let Some(summary) = packet
        .get_mut("dirty_evidence_summary")
        .and_then(Value::as_object_mut)
    {
        if summary
            .get("summary_only")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && summary
                .get("agent_json_compacted")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            return changed;
        }
        summary.insert("summary_only".to_string(), json!(true));
        summary.insert("agent_json_compacted".to_string(), json!(true));
        changed = true;
    }
    changed
}

fn agent_use_validate_edit_compact_hard_interrupt(packet: &mut Value) -> bool {
    let Some(mut hard_interrupt) = packet.get("hard_interrupt").cloned() else {
        return false;
    };
    if hard_interrupt.is_null()
        || hard_interrupt
            .get("agent_json_compacted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return false;
    }
    agent_use_validate_edit_compact_hard_interrupt_value(&mut hard_interrupt);
    if let Some(object) = packet.as_object_mut() {
        object.insert("hard_interrupt".to_string(), hard_interrupt);
        return true;
    }
    false
}

fn agent_use_validate_edit_compact_hard_interrupt_value(hard_interrupt: &mut Value) {
    if hard_interrupt.is_null()
        || hard_interrupt
            .get("agent_json_compacted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return;
    }
    let mut summary = hard_interrupt
        .get("summary")
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(summary_object) = summary.as_object_mut() {
        if let Some(steps) = summary_object
            .get_mut("top_error_suggested_next_steps")
            .and_then(Value::as_array_mut)
        {
            steps.truncate(3);
        }
    }
    let expansion_handles = hard_interrupt
        .get("expansion_handles")
        .and_then(Value::as_array)
        .map(|items| {
            Value::Array(
                items
                    .iter()
                    .take(3)
                    .map(|item| item.get("handle").cloned().unwrap_or_else(|| item.clone()))
                    .collect(),
            )
        })
        .unwrap_or_else(|| json!(["hard_interrupt:full"]));
    let compact = json!({
        "packet_kind": hard_interrupt.get("packet_kind").cloned().unwrap_or_else(|| json!("hard_interrupt")),
        "status": hard_interrupt.get("status").cloned().unwrap_or(Value::Null),
        "hard_interrupt": hard_interrupt.get("hard_interrupt").cloned().unwrap_or_else(|| json!(true)),
        "summary": summary,
        "source_validation_packet_ref": hard_interrupt.get("source_validation_packet_ref").cloned().unwrap_or_else(|| json!("validation_packet")),
        "expansion_handles": expansion_handles,
        "agent_json_compacted": true,
    });
    *hard_interrupt = compact;
}

fn agent_use_validate_edit_pop_array(packet: &mut Value, key: &str, preserve: usize) -> bool {
    let Some(items) = packet.get_mut(key).and_then(Value::as_array_mut) else {
        return false;
    };
    if items.len() <= preserve {
        return false;
    }
    items.pop();
    true
}

pub(crate) fn run_agent_use_watch_once_delta(
    profile: &AgentUseProfile,
    changed_paths: Vec<PathBuf>,
    detail_mode: AgentUseDetailMode,
    normal_dot_codegraph_existed_before: bool,
    max_validation_ms: Option<u64>,
) -> Result<Value, String> {
    let total_update_plus_delta_start = Instant::now();
    // Whole-pipeline wall budget (MVP3.9.5.3): anchored at run entry so slow
    // pre-commit snapshots also consume it; enforced at validation substage
    // boundaries (commit semantics are never aborted mid-flight).
    let validation_wall = agent_use_validation_wall(max_validation_ms);
    let mut validation_stage_tracker =
        ValidationStageTracker::start(profile, "agent-use.watch.once");
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let requested_changed_paths =
        agent_use_report_changed_paths(&profile.repo_root, &changed_paths);
    validation_stage_tracker.mark("preflight");
    let preflight_start = Instant::now();
    let preflight = inspect_db_lifecycle_surface_preflight(DbLifecycleSurfacePreflightRequest {
        repo_root: profile.repo_root.clone(),
        db_path: profile.db_path.clone(),
        surface_name: "agent-use.watch.once".to_string(),
        operation_kind: DbLifecycleOperationKind::WriteUpdate,
        allow_stale_read: false,
        allow_foreign_repo: false,
        required_storage_mode: None,
        expected_scope: Some(profile.scope_policy.clone()),
    })
    .map_err(|error| error.to_string())?;
    let preflight_ms = preflight_start.elapsed().as_millis();

    if !preflight.safe_to_write {
        return Ok(agent_use_watch_unavailable_json(
            profile,
            &preflight,
            normal_dot_codegraph_existed_before,
            &changed_paths,
        ));
    }
    let path_preflight =
        agent_use_watch_path_preflight(&profile.repo_root, &changed_paths, &profile.scope_policy);
    if !path_preflight.should_update {
        return Ok(agent_use_watch_rejected_paths_json(
            profile,
            &preflight,
            normal_dot_codegraph_existed_before,
            &path_preflight,
        ));
    }
    let update_changed_paths = path_preflight.accepted_pathbufs(&profile.repo_root);

    // MVP3.9.5b: a journal surviving from a previous run means a committed
    // update whose validation never finished — replay it FIRST so its
    // findings are recovered (persisted as open blockers and re-emitted by
    // this run's validation packet after fresh re-verification).
    let journal_replay_summary = agent_use_replay_pending_validation(profile);

    let snapshot_options = NormalizedFactSnapshotOptions {
        include_text_evidence: true,
        include_path_evidence: true,
        include_sidecar_freshness: true,
        ..NormalizedFactSnapshotOptions::default()
    };
    validation_stage_tracker.mark("closure");
    // MVP3.9.5.3 residual: one read session (one lifecycle preflight, one
    // cached store) serves every pre-commit read — closure, old snapshot,
    // and the bounded-journal re-snapshot — instead of each call re-opening
    // and re-hydrating its own connection. Dropped before the write phase:
    // a session must never span a DB commit.
    let pre_commit_read_session =
        open_normalized_fact_snapshot_session(&profile.repo_root, &profile.db_path)
            .map_err(|error| format!("old RTDS dependency closure snapshot failed: {error}"))?;
    let dependency_closure_start = Instant::now();
    let pre_update_dependency_closure = pre_commit_read_session
        .dependency_closure_for_changed_paths(&update_changed_paths)
        .map_err(|error| format!("old RTDS dependency closure snapshot failed: {error}"))?;
    let pre_update_dependency_closure_ms = dependency_closure_start.elapsed().as_millis();
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
    validation_stage_tracker.mark("old_snapshot");
    let old_snapshot_start = Instant::now();
    let old_graph_delta_snapshot = pre_commit_read_session
        .snapshot_for_paths(
            &update_changed_paths,
            &pre_update_closure_paths,
            snapshot_options.clone(),
        )
        .map_err(|error| format!("old normalized graph delta snapshot failed: {error}"))?;
    let snapshot_old_ms = old_snapshot_start.elapsed().as_millis();
    // Phase B (MVP3.9.5b): journal the pre-commit facts before anything is
    // published, so a post-commit death leaves a replayable record. Refusing
    // to proceed on journal-write failure is deliberate: committing without
    // the journal would silently reopen the atomicity hole.
    let journal_started_unix_ms = unix_time_ms();
    let mut validation_journal = ValidationJournal {
        journal_version: VALIDATION_JOURNAL_VERSION,
        run_id: format!("{journal_started_unix_ms}-{}", std::process::id()),
        surface: "agent-use.watch.once".to_string(),
        started_unix_ms: journal_started_unix_ms as u128,
        state: VALIDATION_JOURNAL_STATE_UPDATE_PENDING.to_string(),
        journal_scope: "changed_and_closure".to_string(),
        changed_files: old_graph_delta_snapshot.changed_files.clone(),
        closure_files: old_graph_delta_snapshot.closure_files.clone(),
        snapshot_options: snapshot_options.clone(),
        old_facts: old_graph_delta_snapshot.clone(),
    };
    // Serialized once: these bytes are both the size check and the write
    // (the journal is multi-MB on large-file edits; serializing it twice
    // was measurable wall time).
    let mut journal_bytes = serde_json::to_vec(&validation_journal).unwrap_or_default();
    if journal_bytes.is_empty() || journal_bytes.len() > VALIDATION_JOURNAL_MAX_BYTES {
        let changed_only_snapshot = pre_commit_read_session
            .snapshot_for_paths(&update_changed_paths, &[], snapshot_options.clone())
            .map_err(|error| format!("bounded journal snapshot failed: {error}"))?;
        validation_journal.journal_scope = "changed_files_only".to_string();
        validation_journal.closure_files = Vec::new();
        validation_journal.old_facts = changed_only_snapshot;
        journal_bytes = serde_json::to_vec(&validation_journal).map_err(|error| {
            format!("validation journal serialize failed; refusing to commit without an atomicity journal: {error}")
        })?;
    }
    write_validation_journal_bytes(profile, &journal_bytes).map_err(|error| {
        format!("validation journal write failed; refusing to commit without an atomicity journal: {error}")
    })?;
    drop(journal_bytes);
    drop(pre_commit_read_session);
    validation_stage_tracker.mark("publish");
    write_agent_use_publish_state(profile, "updating", None)?;
    validation_stage_tracker.mark("commit");
    let hot_path_update_start = Instant::now();
    let summary = match update_changed_files_to_db(
        &profile.repo_root,
        &update_changed_paths,
        &profile.db_path,
    ) {
        Ok(summary) => {
            if cli_write_path_chaos_failpoint_enabled(
                AGENT_USE_WATCH_AFTER_DELTA_COMMIT_BEFORE_STATE_CLEAR_FAILPOINT,
            ) {
                let message = format!(
                        "chaos_failpoint:{AGENT_USE_WATCH_AFTER_DELTA_COMMIT_BEFORE_STATE_CLEAR_FAILPOINT}"
                    );
                if let Err(state_error) =
                    write_agent_use_publish_state(profile, "interrupted", Some(&message))
                {
                    return Err(format!(
                        "{message}; publish_state_update_failed: {state_error}"
                    ));
                }
                // Interrupted commit: publish-state recovery owns the DB; a
                // pre-commit journal has nothing to replay.
                clear_validation_journal(profile);
                return Err(message);
            }
            // Phase D: the DB is now consistent with source; what is pending
            // is VALIDATION. Transition before clearing the publish marker so
            // no instant exists where neither sidecar covers the run. Written
            // from the in-memory journal: reloading the multi-MB file from
            // disk just to flip the state string was measurable wall time.
            validation_journal.state = VALIDATION_JOURNAL_STATE_VALIDATING.to_string();
            let _ = write_validation_journal(profile, &validation_journal);
            clear_agent_use_publish_state(profile)?;
            summary
        }
        Err(error) => {
            let message = error.to_string();
            if let Err(state_error) =
                write_agent_use_publish_state(profile, "interrupted", Some(&message))
            {
                return Err(format!(
                    "{message}; publish_state_update_failed: {state_error}"
                ));
            }
            clear_validation_journal(profile);
            return Err(message);
        }
    };
    let hot_path_update_ms = hot_path_update_start.elapsed().as_millis();
    // The update is committed and the publish-state marker is cleared: from
    // here on a panic must surface as a structured packet instead of leaving
    // the published baseline unexplained (MVP3.9.5 no-silent-failure rule).
    let post_commit_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<Value, String> {
            let mut value = serde_json::to_value(&summary).map_err(|error| error.to_string())?;

            validation_stage_tracker.mark("lifecycle_reads");
            let lifecycle = watch_update_lifecycle_metadata(
                &profile.repo_root,
                &profile.db_path,
                "agent-use.watch.once",
                true,
                Some(profile.scope_policy.clone()),
            )
            .map_err(|error| error.to_string())?;
            let post_preflight = inspect_read_db_lifecycle_preflight(
                &profile.repo_root,
                &profile.db_path,
                Some(profile.scope_policy.clone()),
            )?;
            let staged_availability = staged_availability_for_cli(
                &profile.repo_root,
                &profile.db_path,
                Some(&post_preflight),
                Some(&profile.candidate_spool_path),
                Some(&profile.vector_runtime_path),
                Some(&profile.vector_audit_path),
                None,
            );
            let changed_paths_normalized = if summary.changed_files.is_empty() {
                requested_changed_paths.clone()
            } else {
                summary.changed_files.clone()
            };
            let delta_requested_paths = if summary
                .dependency_closure
                .requested_changed_files
                .is_empty()
            {
                changed_paths_normalized.clone()
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
            validation_stage_tracker.mark("new_snapshot");
            let new_snapshot_start = Instant::now();
            // MVP3.9.5.3 residual: one post-commit read session feeds the new
            // snapshot AND the validation collectors below, so row hydration for the
            // changed/closure files is paid once. Opened after the commit, so it
            // sees (and caches) the published baseline.
            let post_commit_read_session =
                open_normalized_fact_snapshot_session(&profile.repo_root, &profile.db_path)
                    .map_err(|error| {
                        format!("new normalized graph delta snapshot failed: {error}")
                    })?;
            let new_graph_delta_snapshot = post_commit_read_session
                .snapshot_for_paths(&delta_changed_paths, &delta_closure_paths, snapshot_options)
                .map_err(|error| format!("new normalized graph delta snapshot failed: {error}"))?;
            let snapshot_new_ms = new_snapshot_start.elapsed().as_millis();
            validation_stage_tracker.mark("delta");
            let delta_compute_start = Instant::now();
            let mut validation_entity_source_role_delta = compute_entity_source_role_delta(
                &old_graph_delta_snapshot,
                &new_graph_delta_snapshot,
                EntitySourceRoleDeltaOptions {
                    max_items_per_category: agent_use_validation_graph_delta_max_items(),
                },
            );
            validation_entity_source_role_delta
                .apply_dependency_closure_summary(&summary.dependency_closure);
            validation_entity_source_role_delta.timings.diff_closure_ms =
                pre_update_dependency_closure_ms;
            let mut entity_source_role_delta = if detail_mode.preserves_full_details() {
                validation_entity_source_role_delta.clone()
            } else {
                // The compact view is derived from the full report instead of
                // recomputing the whole delta at a lower cap.
                validation_entity_source_role_delta
                    .truncated_to_max_items(AGENT_USE_COMPACT_GRAPH_DELTA_TOP_LIMIT)
            };
            let delta_compute_ms = delta_compute_start.elapsed().as_millis();
            entity_source_role_delta.timings.diff_closure_ms = pre_update_dependency_closure_ms;
            let deleted_paths = agent_use_watch_deleted_paths(&summary);
            annotate_agent_use_output(
                &mut value,
                profile,
                "watch",
                normal_dot_codegraph_existed_before,
            );
            add_agent_use_durability_labels(&mut value, profile, &post_preflight, None);
            let mut validation_was_bounded = false;
            if let Some(object) = value.as_object_mut() {
                let update_no_op_paths = agent_use_watch_no_op_paths(&summary);
                let no_op_paths = path_preflight.merge_no_op_paths(&update_no_op_paths);
                let status = agent_use_watch_status(&summary, &no_op_paths);
                let delta_state = agent_use_watch_delta_state(&summary, &no_op_paths);
                let graph_delta_packet_start = Instant::now();
                let graph_delta_json = agent_use_graph_delta_json(&entity_source_role_delta);
                let graph_delta_packet_serialize_ms =
                    graph_delta_packet_start.elapsed().as_millis();
                let graph_delta_packet_budget =
                    agent_use_graph_delta_packet_budget_json(&entity_source_role_delta);
                let graph_delta_timing_json = agent_use_graph_delta_timing_json(
                    &entity_source_role_delta,
                    hot_path_update_ms,
                    snapshot_old_ms,
                    snapshot_new_ms,
                    pre_update_dependency_closure_ms,
                    graph_delta_packet_serialize_ms,
                    delta_compute_ms,
                    total_update_plus_delta_start.elapsed().as_millis(),
                );
                validation_stage_tracker.mark("validation");
                let (validation_packet, validation_substage_summary) =
                    agent_use_exact_calls_validation_packet(
                        profile,
                        &post_preflight,
                        &validation_entity_source_role_delta,
                        graph_delta_json.clone(),
                        changed_paths_normalized.clone(),
                        Some(&mut validation_stage_tracker),
                        validation_wall,
                        Some(post_commit_read_session.store()),
                    )?;
                validation_stage_tracker.mark("packet");
                let validation_packet_json = if detail_mode.preserves_full_details() {
                    serde_json::to_value(&validation_packet).map_err(|error| error.to_string())?
                } else {
                    validation_packet.compact_agent_json(AGENT_USE_COMPACT_GRAPH_DELTA_TOP_LIMIT)
                };
                object.insert("command".to_string(), json!("watch"));
                object.insert("subcommand".to_string(), json!("once"));
                object.insert("watch_mode".to_string(), json!("once_changed"));
                object.insert("status".to_string(), json!(status));
                object.insert("agent_use_watch_available".to_string(), json!(true));
                object.insert(
                    "agent_use_watch_status".to_string(),
                    json!("implemented_once_changed"),
                );
                object.insert(
                    "delta_sync_phase".to_string(),
                    json!("real_time_delta_sync"),
                );
                object.insert("delta_sync_state".to_string(), json!(delta_state));
                object.insert("delta_state".to_string(), json!(delta_state));
                object.insert(
                    "reason".to_string(),
                    json!(agent_use_watch_reason(&summary, &no_op_paths)),
                );
                object.insert("auto_index_enabled".to_string(), json!(false));
                object.insert("changed_paths".to_string(), json!(changed_paths_normalized));
                let deleted_paths = path_preflight.merge_deleted_paths(&deleted_paths);
                object.insert("deleted_paths".to_string(), json!(deleted_paths.clone()));
                object.insert(
                    "deleted_path".to_string(),
                    if deleted_paths.len() == 1 {
                        json!(deleted_paths[0])
                    } else {
                        Value::Null
                    },
                );
                object.insert(
                    "normalized_changed_files".to_string(),
                    json!(path_preflight.normalized_changed_files.clone()),
                );
                object.insert(
                    "rejected_paths".to_string(),
                    json!(path_preflight.rejected_paths.clone()),
                );
                object.insert("no_op_paths".to_string(), json!(no_op_paths));
                object.insert(
                    "changed_paths_requested".to_string(),
                    json!(path_preflight.requested_paths.clone()),
                );
                object.insert(
                    "ignored_paths".to_string(),
                    json!(path_preflight.ignored_paths.clone()),
                );
                object.insert(
                    "generated_paths".to_string(),
                    json!(path_preflight.generated_paths.clone()),
                );
                object.insert(
                    "outside_repo_paths".to_string(),
                    json!(path_preflight.outside_repo_paths.clone()),
                );
                object.insert(
                    "duplicate_paths".to_string(),
                    json!(path_preflight.duplicate_paths.clone()),
                );
                object.insert(
                    "renamed_paths".to_string(),
                    json!(path_preflight.renamed_paths.clone()),
                );
                object.insert(
                    "atomic_temp_paths".to_string(),
                    json!(path_preflight.atomic_temp_paths.clone()),
                );
                object.insert(
                    "partial_input_failures_reported".to_string(),
                    json!(path_preflight.partial_input_failures_reported),
                );
                object.insert(
                    "too_many_changed_files".to_string(),
                    json!(path_preflight.too_many_changed_files),
                );
                object.insert(
                    "max_changed_files".to_string(),
                    json!(path_preflight.max_changed_files),
                );
                object.insert("no_silent_path_drops".to_string(), json!(true));
                object.insert(
                    "input_policy".to_string(),
                    json!(path_preflight.input_policy.clone()),
                );
                object.insert(
                    "rename_policy".to_string(),
                    json!(path_preflight.rename_policy.clone()),
                );
                object.insert(
                    "per_file_status".to_string(),
                    json!(path_preflight.per_file_status.clone()),
                );
                object.insert(
                    "input_diagnostics".to_string(),
                    json!(path_preflight.diagnostics.clone()),
                );
                object.insert(
                    "input_warnings".to_string(),
                    json!(path_preflight.warnings.clone()),
                );
                object.insert("watch_db".to_string(), lifecycle);
                object.insert(
                    "staged_availability".to_string(),
                    staged_availability.clone(),
                );
                object.insert(
            "publish_safety".to_string(),
            json!({
                "strategy": "sqlite_transaction_delta_update",
                "old_good_read_visibility": "readers see the previously committed DB state until the delta transaction commits",
                "partial_update_claimability": "non_claimable_if_publish_state_interrupted",
                "temp_db_claimability": "not_applicable_for_once_delta_update",
                "temp_db_claimable": false,
                "auto_index_on_start": false,
            }),
        );
                object.insert(
                    "freshness".to_string(),
                    agent_use_watch_freshness_json(&summary, &staged_availability),
                );
                object.insert(
                    "old_graph_valid".to_string(),
                    json!(preflight.safe_to_write),
                );
                object.insert("new_graph_valid".to_string(), json!(post_preflight.safe));
                object.insert("old_db_preserved".to_string(), json!(true));
                object.insert("temp_db_claimable".to_string(), json!(false));
                object.insert("claimable".to_string(), json!(post_preflight.safe));
                object.insert(
                    "claimability".to_string(),
                    staged_availability
                        .get("claimability")
                        .cloned()
                        .unwrap_or_else(|| {
                            json!({
                                "claimable": post_preflight.safe,
                                "diagnostic_only": !post_preflight.safe,
                                "candidate_only": false,
                                "graph_proof_available": post_preflight.safe,
                            })
                        }),
                );
                object.insert(
                    "facts_deleted".to_string(),
                    json!(
                        summary.deleted_fact_files
                            + summary.deleted_file_facts_removed
                            + summary.stale_facts_deleted_for_ignored_paths
                    ),
                );
                object.insert(
                    "facts_deleted_measurement".to_string(),
                    json!("file_fact_sets_plus_deleted_file_records"),
                );
                object.insert(
                    "facts_inserted".to_string(),
                    json!(
                        summary.files_indexed
                            + summary.entities
                            + summary.edges
                            + summary.dirty_path_evidence_count
                    ),
                );
                object.insert(
                    "facts_inserted_measurement".to_string(),
                    json!("files_plus_entities_plus_edges_plus_path_evidence_rows"),
                );
                object.insert(
                    "entities_added".to_string(),
                    json!(entity_source_role_delta.entities_added_count),
                );
                object.insert(
                    "entities_removed".to_string(),
                    json!(entity_source_role_delta.entities_removed_count),
                );
                object.insert(
                    "entities_changed".to_string(),
                    json!(entity_source_role_delta.entities_changed_count),
                );
                object.insert(
                    "source_roles_changed".to_string(),
                    json!(entity_source_role_delta.source_roles_changed_count),
                );
                object.insert("graph_delta".to_string(), graph_delta_json);
                object.insert(
                    "validation_packet".to_string(),
                    validation_packet_json.clone(),
                );
                let validation_wall_bounded = validation_substage_summary
                    .get("wall_budget")
                    .and_then(|value| value.get("exceeded"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                object.insert(
                    "validation_wall_bounded".to_string(),
                    json!(validation_wall_bounded),
                );
                validation_was_bounded = validation_substage_summary
                    .get("validation_bounded")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if detail_mode.preserves_full_details() {
                    // Compact output gets substage attribution from the stage-tracker
                    // sidecar instead; this inline mirror is for --audit-json runs.
                    object.insert(
                        "validation_substage_summary".to_string(),
                        validation_substage_summary,
                    );
                }
                object.insert(
                    "validation_status".to_string(),
                    validation_packet_json["status"].clone(),
                );
                object.insert(
                    "validation_must_fix_before_continuing".to_string(),
                    validation_packet_json["must_fix_before_continuing"].clone(),
                );
                object.insert(
                    "validation_summary_counts_by_rule_id".to_string(),
                    validation_packet_json["summary_counts_by_rule_id"].clone(),
                );
                object.insert(
                    "validation_summary_counts_by_classification".to_string(),
                    validation_packet_json["summary_counts_by_classification"].clone(),
                );
                object.insert(
                    "validation_summary_counts_by_relation_kind".to_string(),
                    validation_packet_json["summary_counts_by_relation_kind"].clone(),
                );
                object.insert(
                    "validation_stale_unsafe_blockers".to_string(),
                    validation_packet_json["stale_unsafe_blockers"].clone(),
                );
                object.insert(
                    "validation_recommended_next_steps".to_string(),
                    validation_packet_json["recommended_next_steps"].clone(),
                );
                object.insert("journal_replay".to_string(), journal_replay_summary.clone());
                object.insert(
                    "validation_blocking_error_count".to_string(),
                    json!(validation_packet.blocking_errors.len()),
                );
                object.insert(
                    "validation_warning_count".to_string(),
                    json!(validation_packet.warnings.len()),
                );
                object.insert(
                    "validation_unknown_count".to_string(),
                    json!(validation_packet.unknowns.len()),
                );
                object.insert(
                    "hard_interrupt_available".to_string(),
                    validation_packet_json["hard_interrupt_available"].clone(),
                );
                object.insert(
                    "hard_interrupt".to_string(),
                    validation_packet_json
                        .get("hard_interrupt")
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                object.insert("hard_interrupt_not_implemented".to_string(), json!(false));
                object.insert(
                    "graph_delta_detail_mode".to_string(),
                    json!(detail_mode.label()),
                );
                object.insert(
                    "delta_packet_compact_default".to_string(),
                    json!(matches!(detail_mode, AgentUseDetailMode::Compact)),
                );
                object.insert(
                    "graph_delta_packet_budget".to_string(),
                    graph_delta_packet_budget.clone(),
                );
                object.insert(
                    "edges_added".to_string(),
                    json!(entity_source_role_delta.edges_added_count),
                );
                object.insert(
                    "edges_removed".to_string(),
                    json!(entity_source_role_delta.edges_removed_count),
                );
                object.insert(
                    "edges_changed".to_string(),
                    json!(entity_source_role_delta.edges_changed_count),
                );
                object.insert(
                    "source_spans_added".to_string(),
                    json!(entity_source_role_delta.source_spans_added_count),
                );
                object.insert(
                    "source_spans_removed".to_string(),
                    json!(entity_source_role_delta.source_spans_removed_count),
                );
                object.insert(
                    "source_spans_changed".to_string(),
                    json!(entity_source_role_delta.source_spans_changed_count),
                );
                object.insert(
                    "text_evidence_changed".to_string(),
                    json!(entity_source_role_delta.text_evidence_changed_count > 0),
                );
                object.insert(
                    "path_evidence_invalidated".to_string(),
                    json!({
                        "action": agent_use_path_evidence_delta_action(&summary),
                        "dirty_path_evidence_count": summary.dirty_path_evidence_count,
                        "delta_count": entity_source_role_delta.path_evidence_invalidated_count,
                        "graph_proof": false,
                    }),
                );
                object.insert(
                    "candidate_spool_invalidated_or_rebuilt".to_string(),
                    json!(agent_use_layer_delta_action(
                        staged_availability
                            .get("candidate_spool_status")
                            .and_then(Value::as_str),
                    )),
                );
                object.insert(
                    "candidate_spool_invalidated_or_refreshed".to_string(),
                    json!(agent_use_layer_delta_action(
                        staged_availability
                            .get("candidate_spool_status")
                            .and_then(Value::as_str),
                    )),
                );
                object.insert(
                    "candidate_query_index_invalidated_or_rebuilt".to_string(),
                    json!(agent_use_layer_delta_action(
                        staged_availability
                            .pointer("/layer_readiness/candidate_spool/query_index_status")
                            .and_then(Value::as_str),
                    )),
                );
                object.insert(
                    "candidate_query_index_invalidated_or_refreshed".to_string(),
                    json!(agent_use_layer_delta_action(
                        staged_availability
                            .pointer("/layer_readiness/candidate_spool/query_index_status")
                            .and_then(Value::as_str),
                    )),
                );
                object.insert(
                    "vector_chunks_invalidated_or_rebuilt".to_string(),
                    json!(agent_use_layer_delta_action(
                        staged_availability
                            .get("vector_runtime_status")
                            .and_then(Value::as_str),
                    )),
                );
                object.insert(
                    "vector_chunks_invalidated".to_string(),
                    json!(agent_use_layer_delta_action(
                        staged_availability
                            .get("vector_runtime_status")
                            .and_then(Value::as_str),
                    )),
                );
                object.insert(
                    "vector_runtime_status_changed".to_string(),
                    json!(agent_use_layer_delta_action(
                        staged_availability
                            .get("vector_runtime_status")
                            .and_then(Value::as_str),
                    )),
                );
                object.insert(
                    "vector_audit_status_changed".to_string(),
                    json!(agent_use_layer_delta_action(
                        staged_availability
                            .get("vector_audit_status")
                            .and_then(Value::as_str),
                    )),
                );
                object.insert(
                    "binary_candidates_invalidated_or_not_applicable".to_string(),
                    agent_use_not_applicable_delta_action(
                        "binary_vector_candidates_are_request_time_context_candidates",
                    ),
                );
                object.insert(
                    "nuance_tokens_invalidated_or_not_applicable".to_string(),
                    agent_use_not_applicable_delta_action(
                        "nuance_rescue_candidates_are_request_time_context_candidates",
                    ),
                );
                object.insert(
                    "nuance_tokens_invalidated".to_string(),
                    agent_use_not_applicable_delta_action(
                        "nuance_rescue_candidates_are_request_time_context_candidates",
                    ),
                );
                object.insert(
                    "proof_path_caches_invalidated_or_not_applicable".to_string(),
                    agent_use_not_applicable_delta_action(
                        "stored_path_evidence_rows_are_the_current_proof_path_cache_surface",
                    ),
                );
                object.insert(
            "routing_handles_invalidated".to_string(),
            json!({
                "action": if summary.files_indexed > 0 || summary.files_deleted > 0 || summary.files_renamed > 0 {
                    "dirty_file_cleanup"
                } else {
                    "unchanged"
                },
                "scope": "sparse_sidecar_handles",
            }),
        );
                object.insert(
            "routing_handles_invalidated_or_not_applicable".to_string(),
            json!({
                "action": if summary.files_indexed > 0 || summary.files_deleted > 0 || summary.files_renamed > 0 {
                    "dirty_file_cleanup"
                } else {
                    "unchanged"
                },
                "scope": "sparse_sidecar_handles",
                "graph_proof": false,
            }),
        );
                object.insert(
                    "proof_ladder_changes".to_string(),
                    json!(entity_source_role_delta.proof_ladder_changes),
                );
                object.insert(
                    "file_renames_detected".to_string(),
                    json!(entity_source_role_delta.file_renames_detected.clone()),
                );
                object.insert(
                    "rename_ambiguities".to_string(),
                    json!(entity_source_role_delta.rename_ambiguities.clone()),
                );
                object.insert(
                    "closure_delta_summary".to_string(),
                    json!(entity_source_role_delta.closure_delta_summary.clone()),
                );
                object.insert(
                    "closure_files_considered".to_string(),
                    json!(summary.dependency_closure.closure_files_considered.clone()),
                );
                object.insert(
                    "closure_files_updated".to_string(),
                    json!(summary.dependency_closure.closure_files_updated.clone()),
                );
                object.insert(
                    "closure_edges_inspected".to_string(),
                    json!(summary.dependency_closure.closure_edges_inspected),
                );
                object.insert(
                    "closure_relation_classes".to_string(),
                    json!(summary.dependency_closure.closure_relation_classes.clone()),
                );
                object.insert(
                    "closure_budget_hit".to_string(),
                    json!(summary.dependency_closure.closure_budget_hit),
                );
                object.insert(
                    "degraded_relation_classes".to_string(),
                    json!(summary.dependency_closure.degraded_relation_classes.clone()),
                );
                object.insert(
                    "unsupported_relation_classes".to_string(),
                    json!(entity_source_role_delta
                        .closure_unsupported_relation_classes
                        .clone()),
                );
                object.insert(
                    "graph_output_degraded_labels".to_string(),
                    json!(summary.graph_output_degraded_labels.clone()),
                );
                object.insert(
                    "graph_output_budget_hit".to_string(),
                    json!(!summary.graph_output_degraded_labels.is_empty()),
                );
                object.insert(
                    "skipped_relation_classes".to_string(),
                    json!(summary.dependency_closure.skipped_relation_classes.clone()),
                );
                object.insert(
                    "closure_unknowns".to_string(),
                    json!(summary.dependency_closure.closure_unknowns.clone()),
                );
                object.insert(
                    "dependency_closure".to_string(),
                    json!(summary.dependency_closure.clone()),
                );
                object.insert(
                    "no_full_repo_fallback".to_string(),
                    json!(summary.dependency_closure.full_repo_fallback_avoided),
                );
                object.insert(
                    "full_repo_fallback_avoided_reason".to_string(),
                    json!(summary.dependency_closure.fallback_avoided_reason.clone()),
                );
                object.insert(
                    "manual_full_index_recommendation".to_string(),
                    json!(summary
                        .dependency_closure
                        .manual_full_index_recommendation
                        .clone()),
                );
                object.insert(
            "timings".to_string(),
            json!({
                "total_update_plus_delta_ms": graph_delta_timing_json["total_update_plus_delta_ms"].clone(),
                "preflight_ms": preflight_ms,
                "total_wall_ms": summary.profile.as_ref().map(|profile| profile.total_wall_ms),
                "file_read_ms": profile_span_ms(summary.profile.as_ref(), "file_read"),
                "file_hash_ms": profile_span_ms(summary.profile.as_ref(), "file_hash"),
                "parse_ms": profile_span_ms(summary.profile.as_ref(), "parse"),
                "stale_delete_ms": profile_span_ms(summary.profile.as_ref(), "stale_fact_delete"),
                "transaction_commit_ms": profile_span_ms(summary.profile.as_ref(), "transaction_commit"),
                "path_evidence_regeneration_ms": profile_span_ms(summary.profile.as_ref(), "refresh_path_evidence"),
                "graph_delta": graph_delta_timing_json,
            }),
        );
                object.insert(
                    "normal_dot_codegraph_created".to_string(),
                    json!(!normal_dot_codegraph_existed_before && normal_dot_codegraph.exists()),
                );
                object.insert(
                    "normal_dot_codegraph_mutated".to_string(),
                    json!(normal_dot_codegraph_existed_before != normal_dot_codegraph.exists()),
                );
                let recovery = agent_use_recovery_json(profile);
                object.insert(
                    "recovery_commands".to_string(),
                    recovery
                        .get("commands")
                        .cloned()
                        .unwrap_or_else(|| json!(profile.recovery_commands.clone())),
                );
                object.insert("recovery".to_string(), recovery);
            }
            merge_json_object(
                &mut value,
                staged_availability_top_level_fields(&staged_availability),
            );
            add_agent_use_rtds_freshness_fields(
                &mut value,
                profile,
                &post_preflight,
                &staged_availability,
            );
            add_agent_use_dirty_evidence_output_fields(
                &mut value,
                "agent-use.watch.once",
                detail_mode.preserves_full_details(),
            );
            validation_stage_tracker.mark("persist_outcome");
            persist_agent_use_last_delta_state(&mut value, profile, "agent-use.watch.once");
            // Phase F: the validation outcome is persisted (validation-state sidecar
            // written during packet construction, delta-state just now) — the
            // two-phase operation is complete and the journal can be retired. The
            // validation_state block is computed only now, after retirement, so this
            // run's own (intentionally open) journal does not read as "incomplete".
            //
            // EXCEPT when the validation itself was bounded (wall budget, edge
            // budget, or delta-cap omission): a bounded validation is incomplete, so
            // the journal stays and the NEXT run replays the pending delta — without
            // this, a wall-bounded run absorbs the unvalidated baseline and the rerun
            // reports a silent ok (found by the MVP3.9.5.3 adversarial gate probe).
            if !validation_was_bounded {
                clear_validation_journal(profile);
            }
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "validation_state".to_string(),
                    agent_use_validation_state_block(profile),
                );
            }
            validation_stage_tracker.mark("envelope");
            compact_agent_use_agent_json_envelope(
                &mut value,
                profile,
                detail_mode,
                if detail_mode.preserves_full_details() {
                    detail_mode.default_max_output_bytes()
                } else {
                    AGENT_USE_WATCH_COMPACT_MAX_OUTPUT_BYTES
                },
                Some(&staged_availability),
            );
            Ok(value)
        },
    ));
    match post_commit_result {
        Ok(result) => result,
        Err(panic_payload) => {
            let failed_stage = validation_stage_tracker
                .current_stage()
                .unwrap_or_else(|| "post_commit".to_string());
            let panic_message = panic_payload_message(panic_payload.as_ref());
            validation_stage_tracker.preserve_for_post_mortem();
            transition_validation_journal_state(profile, &format!("failed:{panic_message}"));
            Ok(agent_use_watch_pipeline_panic_packet(
                profile,
                &requested_changed_paths,
                &failed_stage,
                &panic_message,
                validation_stage_tracker.path(),
            ))
        }
    }
}

/// Per-substage wall/fan-out attribution INSIDE the validation stage
/// (MVP3.9.5.3). Substage starts are marked live on the stage-tracker sidecar
/// (post-mortem + live-readable during a slow run); elapsed ms and fan-out
/// counters are folded into a compact summary returned with the packet.
struct ValidationSubstageMeter<'a> {
    tracker: Option<&'a mut ValidationStageTracker>,
    summary: serde_json::Map<String, Value>,
    current: Option<(String, Instant)>,
}

impl<'a> ValidationSubstageMeter<'a> {
    fn new(tracker: Option<&'a mut ValidationStageTracker>) -> Self {
        Self {
            tracker,
            summary: serde_json::Map::new(),
            current: None,
        }
    }

    /// Closes the previous substage (if any) and marks the start of `name`.
    fn begin(&mut self, name: &str) {
        self.finish_with(json!({}));
        if let Some(tracker) = self.tracker.as_deref_mut() {
            tracker.mark(&format!("validation:{name}"));
        }
        self.current = Some((name.to_string(), Instant::now()));
    }

    /// Closes the current substage, recording elapsed ms plus `detail`
    /// counters (e.g. edges validated). Idempotent when nothing is open.
    fn finish_with(&mut self, detail: Value) {
        if let Some((name, start)) = self.current.take() {
            let mut record = serde_json::Map::new();
            record.insert("ms".to_string(), json!(start.elapsed().as_millis() as u64));
            if let Some(extra) = detail.as_object() {
                for (key, value) in extra {
                    record.insert(key.clone(), value.clone());
                }
            }
            self.summary.insert(name, Value::Object(record));
        }
    }

    /// Records a non-substage attribution entry (e.g. the wall-budget
    /// verdict) alongside the substage records.
    fn note(&mut self, key: &str, value: Value) {
        self.summary.insert(key.to_string(), value);
    }

    /// Finalizes the meter: the full substage map is marked once on the
    /// stage tracker (live/post-mortem lane) and returned for inline output.
    fn into_summary(mut self) -> Value {
        self.finish_with(json!({}));
        let summary = Value::Object(std::mem::take(&mut self.summary));
        if let Some(tracker) = self.tracker.as_deref_mut() {
            tracker.mark_with(
                "validation:substage_summary",
                json!({ "substages": summary.clone() }),
            );
        }
        summary
    }
}

pub(crate) fn agent_use_exact_calls_validation_packet(
    profile: &AgentUseProfile,
    preflight: &DbLifecyclePreflight,
    delta: &EntitySourceRoleDeltaReport,
    graph_delta: Value,
    changed_files: Vec<String>,
    stage_tracker: Option<&mut ValidationStageTracker>,
    validation_wall: (Instant, u64),
    // A read store whose caches are already warm from the post-commit
    // snapshot (MVP3.9.5.3 residual). `None` opens a fresh one (replay path).
    shared_read_store: Option<&SqliteGraphStore>,
) -> Result<(ValidationPacket, Value), String> {
    let (validation_deadline, validation_wall_budget_ms) = validation_wall;
    let mut wall_skipped_substages: Vec<&'static str> = Vec::new();
    let mut substages = ValidationSubstageMeter::new(stage_tracker);
    let mut rules = agent_use_exact_calls_validation_rules();
    rules.extend(agent_use_exact_imports_validation_rules());
    rules.extend(agent_use_proof_integrity_validation_rules());
    rules.extend(agent_use_source_role_tests_validation_rules());
    rules.extend(agent_use_activation_gated_contract_validation_rules());
    rules.extend(agent_use_unresolved_reference_validation_rules());
    let lifecycle = agent_use_validation_lifecycle_from_read_preflight(preflight);
    let claimability = json!({
        "claimable": lifecycle.claimable,
        "current": lifecycle.current,
        "diagnostic_only": !lifecycle.is_claimable_current(),
        "non_claimable_reason": lifecycle.non_claimable_reason,
        "db_problem_kind": preflight.db_problem_kind.clone(),
        "blockers": preflight.blockers.clone(),
        "exact_db_path_checked": preflight.exact_db_path_checked.clone(),
    });
    let lifecycle_json = json!({
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
        "exact_db_path_checked": preflight.exact_db_path_checked.clone(),
    });
    let rule_by_id = rules
        .iter()
        .map(|rule| (rule.validation_rule_id.as_str(), rule))
        .collect::<BTreeMap<_, _>>();
    substages.begin("lifecycle_integrity");
    let mut findings = agent_use_collect_lifecycle_integrity_findings(
        profile,
        &rule_by_id,
        lifecycle.clone(),
        preflight,
        delta,
    );
    if !lifecycle.is_claimable_current() {
        let packet = ValidationPacket::new(
            changed_files,
            graph_delta,
            findings,
            rules,
            Vec::new(),
            claimability,
            json!(delta.proof_ladder_changes),
            lifecycle_json,
        );
        let packet = agent_use_attach_activation_gated_contract_metadata(
            packet.with_eligible_hard_interrupts(format!("unix_ms:{}", unix_time_ms())),
            delta,
        );
        return Ok((packet, substages.into_summary()));
    }

    let owned_read_store;
    let store: &SqliteGraphStore = match shared_read_store {
        Some(shared) => shared,
        None => {
            let mut opened = SqliteGraphStore::open_read_only(&profile.db_path)
                .map_err(|error| format!("open validation DB read-only failed: {error}"))?;
            // Same session contract as the snapshot side: this store lives
            // for one validation pass over a committed baseline.
            opened.enable_session_read_caches();
            owned_read_store = opened;
            &owned_read_store
        }
    };
    substages.note("read_cache_enabled", json!(store.read_cache_enabled()));
    let renamed_old_paths = delta
        .file_renames_detected
        .iter()
        .filter_map(|entry| entry.old_path.as_ref())
        .map(|path| normalize_repo_relative_path(path))
        .collect::<BTreeSet<_>>();
    let ambiguous_rename_old_paths = delta
        .rename_ambiguities
        .iter()
        .filter_map(|entry| entry.old_path.as_ref())
        .map(|path| normalize_repo_relative_path(path))
        .collect::<BTreeSet<_>>();
    let removed_entity_paths_by_id = delta
        .entities_removed
        .iter()
        .filter_map(|entry| {
            agent_use_entity_delta_id(entry).map(|entity_id| {
                (
                    entity_id,
                    normalize_repo_relative_path(&entry.repo_relative_path),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();

    let mut seen_edge_rule = BTreeSet::<String>::new();
    let edge_reverification_budget_max = agent_use_validation_max_edge_reverifications();
    let mut edge_reverification_budget = edge_reverification_budget_max;
    let mut edge_reverifications_skipped = 0usize;
    substages.begin("proof_integrity");
    if Instant::now() >= validation_deadline {
        wall_skipped_substages.push("proof_integrity");
    } else {
        agent_use_collect_proof_integrity_findings(
            profile,
            &store,
            &rule_by_id,
            lifecycle.clone(),
            delta,
            &changed_files,
            &mut findings,
            &mut seen_edge_rule,
        )?;
    }
    substages.begin("source_role_tests");
    if Instant::now() >= validation_deadline {
        wall_skipped_substages.push("source_role_tests");
    } else {
        agent_use_collect_source_role_tests_findings(
            profile,
            &store,
            &rule_by_id,
            lifecycle.clone(),
            delta,
            &changed_files,
            &mut findings,
            &mut seen_edge_rule,
        )?;
    }
    substages.begin("activation_gated_contract");
    if Instant::now() >= validation_deadline {
        wall_skipped_substages.push("activation_gated_contract");
    } else {
        agent_use_collect_activation_gated_contract_findings(
            profile,
            &store,
            &rule_by_id,
            lifecycle.clone(),
            delta,
            &changed_files,
            &renamed_old_paths,
            &removed_entity_paths_by_id,
            &mut findings,
            &mut seen_edge_rule,
        )?;
    }

    substages.begin("unresolved_references");
    let mut unresolved_references_block = Value::Null;
    if Instant::now() >= validation_deadline {
        wall_skipped_substages.push("unresolved_references");
    } else {
        unresolved_references_block = agent_use_collect_unresolved_reference_findings(
            store,
            &rule_by_id,
            lifecycle.clone(),
            delta,
            &changed_files,
            &mut findings,
        )?;
    }

    substages.begin("removed_callee_incoming_calls");
    let mut substage_edges_validated = 0usize;
    for removed in delta
        .entities_removed
        .iter()
        .filter(|entry| exact_calls_target_entity_kind(entry.entity_kind))
    {
        if Instant::now() >= validation_deadline {
            wall_skipped_substages.push("removed_callee_incoming_calls");
            break;
        }
        let Some(removed_entity_id) = agent_use_entity_delta_id(removed) else {
            continue;
        };
        let incoming = store
            .find_edges_by_tail_relation(&removed_entity_id, RelationKind::Calls)
            .map_err(|error| format!("read incoming CALLS edges failed: {error}"))?;
        for edge in incoming {
            if edge_reverification_budget == 0 {
                edge_reverifications_skipped += 1;
                continue;
            }
            edge_reverification_budget -= 1;
            substage_edges_validated += 1;
            let rule_id = if renamed_old_paths
                .contains(&normalize_repo_relative_path(&removed.repo_relative_path))
            {
                CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED
            } else {
                CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED
            };
            agent_use_validate_current_calls_edge(
                profile,
                &store,
                &rule_by_id,
                lifecycle.clone(),
                &edge,
                Some(json!(removed)),
                Some(rule_id),
                &mut findings,
                &mut seen_edge_rule,
            )?;
        }
    }

    substages.finish_with(json!({ "edges_validated": substage_edges_validated }));
    substages.begin("removed_calls_delta_edges");
    substage_edges_validated = 0;
    for entry in delta
        .edges_removed
        .iter()
        .filter(|entry| entry.relation == RelationKind::Calls)
    {
        if Instant::now() >= validation_deadline {
            wall_skipped_substages.push("removed_calls_delta_edges");
            break;
        }
        let target_path = entry
            .target_endpoint
            .repo_relative_path
            .as_deref()
            .map(normalize_repo_relative_path)
            .unwrap_or_else(|| normalize_repo_relative_path(&entry.repo_relative_path));
        let target_removed_or_renamed = removed_entity_paths_by_id
            .contains_key(&entry.target_entity_id)
            || renamed_old_paths.contains(&target_path);
        if !target_removed_or_renamed {
            continue;
        }
        if edge_reverification_budget == 0 {
            edge_reverifications_skipped += 1;
            continue;
        }
        edge_reverification_budget -= 1;
        substage_edges_validated += 1;
        let rule_id = if renamed_old_paths.contains(&target_path) {
            CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED
        } else {
            CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED
        };
        agent_use_validate_removed_calls_delta_edge(
            profile,
            &store,
            &rule_by_id,
            lifecycle.clone(),
            entry,
            rule_id,
            &mut findings,
            &mut seen_edge_rule,
        )?;
    }

    substages.finish_with(json!({ "edges_validated": substage_edges_validated }));
    substages.begin("added_changed_calls_edges");
    substage_edges_validated = 0;
    for entry in delta
        .edges_added
        .iter()
        .chain(delta.edges_changed.iter())
        .filter(|entry| entry.relation == RelationKind::Calls)
    {
        if Instant::now() >= validation_deadline {
            wall_skipped_substages.push("added_changed_calls_edges");
            break;
        }
        if !agent_use_exactness_is_proof_grade(entry.exactness) {
            findings.push(agent_use_calls_boundary_diagnostic(
                rule_by_id[CG_MVP3_CALLS_DANGLING_TARGET],
                lifecycle.clone(),
                json!(entry),
                "non-exact CALLS delta is diagnostic only and cannot block",
            ));
            continue;
        }
        if edge_reverification_budget == 0 {
            edge_reverifications_skipped += 1;
            continue;
        }
        edge_reverification_budget -= 1;
        substage_edges_validated += 1;
        let Some(edge) = store
            .get_edge(&entry.edge_id)
            .map_err(|error| format!("read CALLS edge failed: {error}"))?
        else {
            continue;
        };
        agent_use_validate_current_calls_edge(
            profile,
            &store,
            &rule_by_id,
            lifecycle.clone(),
            &edge,
            Some(json!(entry)),
            Some(CG_MVP3_CALLS_DANGLING_TARGET),
            &mut findings,
            &mut seen_edge_rule,
        )?;
    }

    substages.finish_with(json!({ "edges_validated": substage_edges_validated }));
    substages.begin("current_edge_file_scan");
    substage_edges_validated = 0;
    let mut scan_paths = changed_files
        .iter()
        .chain(delta.closure_files_updated.iter())
        .map(|path| normalize_repo_relative_path(path))
        .collect::<Vec<_>>();
    scan_paths.sort();
    scan_paths.dedup();
    let scan_path_count = scan_paths.len();
    for path in scan_paths {
        if Instant::now() >= validation_deadline {
            wall_skipped_substages.push("current_edge_file_scan");
            break;
        }
        let edges = store
            .list_edges_by_file(&path)
            .map_err(|error| format!("read changed-file CALLS edges failed: {error}"))?;
        for edge in edges
            .into_iter()
            .filter(|edge| edge.relation == RelationKind::Calls)
        {
            if edge_reverification_budget == 0 {
                edge_reverifications_skipped += 1;
                continue;
            }
            edge_reverification_budget -= 1;
            substage_edges_validated += 1;
            let missing_target_rule_id = agent_use_missing_target_rule_for_current_edge(
                &edge,
                &removed_entity_paths_by_id,
                &renamed_old_paths,
            );
            agent_use_validate_current_calls_edge(
                profile,
                &store,
                &rule_by_id,
                lifecycle.clone(),
                &edge,
                Some(json!({
                    "source": "changed_or_closure_file_current_edges",
                    "repo_relative_path": path,
                })),
                Some(missing_target_rule_id),
                &mut findings,
                &mut seen_edge_rule,
            )?;
        }
    }

    substages.finish_with(json!({
        "edges_validated": substage_edges_validated,
        "paths_scanned": scan_path_count,
    }));
    substages.begin("exact_imports");
    if Instant::now() >= validation_deadline {
        wall_skipped_substages.push("exact_imports");
    } else {
        agent_use_collect_exact_imports_findings(
            profile,
            &store,
            &rule_by_id,
            lifecycle.clone(),
            delta,
            &changed_files,
            &renamed_old_paths,
            &ambiguous_rename_old_paths,
            &removed_entity_paths_by_id,
            &mut findings,
            &mut seen_edge_rule,
        )?;
    }

    for path in ambiguous_rename_old_paths {
        findings.push(agent_use_calls_boundary_unknown(
            rule_by_id[CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED],
            lifecycle.clone(),
            json!({
                "rename_status": "unknown",
                "old_path": path,
                "ambiguity": true,
            }),
            "ambiguous rename cannot be promoted to exact dangling CALLS proof",
        ));
    }
    if !renamed_old_paths.is_empty()
        && !findings
            .iter()
            .any(|finding| finding.validation_rule_id == CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED)
    {
        for path in renamed_old_paths {
            findings.push(agent_use_calls_boundary_unknown(
                rule_by_id[CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED],
                lifecycle.clone(),
                json!({
                    "rename_status": "detected",
                    "old_path": path,
                    "file_renames_detected": &delta.file_renames_detected,
                    "reason": "rename was detected from file lifecycle evidence, but no exact stale CALLS edge could be graph/source reverified",
                }),
                "renamed callee validation is unknown because file rename evidence alone is not graph-relation proof",
            ));
        }
    }

    // MVP3.9.5.3: if the bounded validation delta omitted entries in a
    // blocking-relevant category, or the per-run edge-reverification budget
    // ran out, the rules above only saw a prefix of the change set — the
    // packet must not claim absence of contradictions.
    substages.begin("bounded_labeling");
    wall_skipped_substages.sort_unstable();
    wall_skipped_substages.dedup();
    let wall_budget_json = json!({
        "budget_ms": validation_wall_budget_ms,
        "exceeded": !wall_skipped_substages.is_empty(),
        "skipped_substages": wall_skipped_substages.clone(),
    });
    substages.note("wall_budget", wall_budget_json.clone());
    // Bounded validation is INCOMPLETE validation: the caller must not retire
    // the journal, so the next run replays the pending delta instead of
    // absorbing the unvalidated baseline (adversarial probe finding,
    // MVP3.9.5.3 gate).
    let validation_bounded = !wall_skipped_substages.is_empty()
        || edge_reverifications_skipped > 0
        || delta.omission.entities_removed_omitted
            + delta.omission.edges_removed_omitted
            + delta.omission.edges_added_omitted
            > 0;
    substages.note("validation_bounded", json!(validation_bounded));
    if !wall_skipped_substages.is_empty() {
        findings.push(agent_use_graph_delta_bounded_unknown(
            rule_by_id[CG_MVP3_GRAPH_DELTA_BOUNDED],
            lifecycle.clone(),
            json!({
                "validation_wall_bounded": true,
                "wall_budget": wall_budget_json,
            }),
            &format!(
                "validation_wall_bounded: the validation wall budget ({validation_wall_budget_ms} ms) was exhausted and substages [{}] were skipped; rerun with a higher --max-validation-ms or run `agent-use index` and re-validate",
                wall_skipped_substages.join(", ")
            ),
        ));
    }
    if edge_reverifications_skipped > 0 {
        findings.push(agent_use_graph_delta_bounded_unknown(
            rule_by_id[CG_MVP3_GRAPH_DELTA_BOUNDED],
            lifecycle.clone(),
            json!({
                "edge_reverification_bounded": true,
                "edge_reverifications_skipped": edge_reverifications_skipped,
                "edge_reverification_budget": edge_reverification_budget_max,
            }),
            &format!(
                "edge_reverification_bounded: {edge_reverifications_skipped} CALLS edges were not re-verified because the per-run reverification budget ({edge_reverification_budget_max}) was exhausted; run `agent-use index` and re-validate"
            ),
        ));
    }
    let blocking_relevant_omitted = delta.omission.entities_removed_omitted
        + delta.omission.edges_removed_omitted
        + delta.omission.edges_added_omitted;
    if blocking_relevant_omitted > 0 {
        findings.push(agent_use_graph_delta_bounded_unknown(
            rule_by_id[CG_MVP3_GRAPH_DELTA_BOUNDED],
            lifecycle.clone(),
            json!({
                "graph_delta_bounded": true,
                "max_items_per_category": delta.omission.max_items_per_category,
                "entities_removed_omitted": delta.omission.entities_removed_omitted,
                "edges_removed_omitted": delta.omission.edges_removed_omitted,
                "edges_added_omitted": delta.omission.edges_added_omitted,
            }),
            &format!(
                "graph_delta_bounded: {blocking_relevant_omitted} blocking-relevant delta entries were omitted by the per-category cap ({}); run `agent-use index` and re-validate",
                delta.omission.max_items_per_category
            ),
        ));
    }

    // MVP3.9.5c sticky blockers: re-verify previously persisted block-class
    // findings against the CURRENT store + source and re-emit the ones that
    // are still contradicted, so an unchanged broken baseline keeps blocking.
    substages.begin("persisted_open_blockers");
    let persisted_blockers_summary = agent_use_apply_persisted_open_blockers(
        profile,
        &store,
        lifecycle.clone(),
        &mut findings,
        &mut seen_edge_rule,
    );

    let persisted_changed_files = changed_files.clone();
    substages.begin("packet_build");
    let packet = ValidationPacket::new(
        changed_files,
        graph_delta,
        findings,
        rules,
        Vec::new(),
        claimability,
        json!(delta.proof_ladder_changes),
        lifecycle_json,
    );
    let mut packet = agent_use_attach_activation_gated_contract_metadata(
        packet.with_eligible_hard_interrupts(format!("unix_ms:{}", unix_time_ms())),
        delta,
    );
    packet.unresolved_references = unresolved_references_block;
    let final_severity = serde_json::to_value(&packet.final_status)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
        .unwrap_or_default();
    substages.begin("persist_outcome");
    persist_validation_outcome(
        profile,
        "agent_use_validation_packet",
        &persisted_changed_files,
        &final_severity,
        &packet.blocking_errors,
        &persisted_blockers_summary,
    );
    Ok((packet, substages.into_summary()))
}

fn agent_use_exact_calls_validation_rules() -> Vec<ValidationRule> {
    let mut missing_source_span = ValidationRule::exact_blocking(
        CG_MVP3_CALLS_MISSING_SOURCE_SPAN,
        ValidationRuleKind::ProofIntegrity,
        Some(RelationKind::Calls),
        "claimable CALLS proof edges must carry a current source span",
        "Regenerate the CALLS edge with a valid source span or downgrade the edge.",
    );
    missing_source_span.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
    missing_source_span.source_role_requirement = ValidationSourceRoleRequirement::RolePreserved;

    let mut missing_provenance = ValidationRule::exact_blocking(
        CG_MVP3_CALLS_DERIVED_MISSING_PROVENANCE,
        ValidationRuleKind::ProofIntegrity,
        Some(RelationKind::Calls),
        "derived CALLS proof edges must carry provenance",
        "Attach provenance edges or downgrade the derived CALLS fact.",
    );
    missing_provenance.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
    missing_provenance.provenance_requirement =
        ValidationProvenanceRequirement::RequiredForDerivedEdges;
    missing_provenance.source_span_requirement =
        ValidationSourceSpanRequirement::RequiredForClaimableGraphFact;
    missing_provenance.source_role_requirement = ValidationSourceRoleRequirement::RolePreserved;

    vec![
        ValidationRule::exact_blocking(
            CG_MVP3_CALLS_DANGLING_TARGET,
            ValidationRuleKind::DanglingTarget,
            Some(RelationKind::Calls),
            "new exact CALLS edges must resolve to a current target entity",
            "Define the missing callee or update the exact call relation.",
        ),
        ValidationRule::exact_blocking(
            CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED,
            ValidationRuleKind::DanglingTarget,
            Some(RelationKind::Calls),
            "removed exact callees must not remain referenced by fresh CALLS edges",
            "Update the caller or restore the removed callee.",
        ),
        ValidationRule::exact_blocking(
            CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED,
            ValidationRuleKind::BrokenContract,
            Some(RelationKind::Calls),
            "renamed exact callees must not leave fresh CALLS edges pointing at the old target",
            "Retarget the caller to the renamed callee or represent the change as unknown.",
        ),
        ValidationRule::exact_blocking(
            CG_MVP3_CALLS_TARGET_ROLE_MISMATCH,
            ValidationRuleKind::SourceRoleBoundary,
            Some(RelationKind::Calls),
            "production CALLS proof must not resolve only to test, mock, or stub targets",
            "Retarget the production caller or mark the evidence as test/mock only.",
        ),
        missing_source_span,
        missing_provenance,
    ]
}

fn agent_use_unresolved_reference_validation_rules() -> Vec<ValidationRule> {
    vec![
        ValidationRule::diagnostic(
            CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL,
            "new unresolved repo-local call references on changed files escalate to warnings via a repo-graph definition lookup; they are never graph proof",
            "Define the called symbol, fix the name, or mark the dependency external; warnings clear when the reference resolves.",
        ),
        ValidationRule::diagnostic(
            CG_MVP3_REF_NEW_UNRESOLVED_IMPORT,
            "new unresolved repo-local import/alias/reexport references on changed files escalate to warnings via a repo-graph definition lookup; they are never graph proof",
            "Define or export the imported symbol, fix the import path, or mark the dependency external.",
        ),
        ValidationRule::diagnostic(
            CG_MVP3_REF_EXTERNAL_OR_BUILTIN,
            "unresolved references classified external_dependency/builtin_or_std/macro_or_codegen are counted as diagnostics only and never produce a warning",
            "No action required; these references resolve outside the repo graph by design.",
        ),
        ValidationRule::diagnostic(
            CG_MVP3_REF_DYNAMIC,
            "unresolved references classified dynamic_or_computed are diagnostic-only non-graph evidence and never make a clean packet top-level unknown",
            "Dynamic or computed callees cannot be verified statically; keep them visible as diagnostics, not proof of error.",
        ),
    ]
}

struct UnresolvedReferenceRepoGraphLookup {
    lookup_name: String,
    definition_count: usize,
    top_candidate: Option<String>,
}

/// Cheap escalation lookup (§1.3.4): does ANY definition-shaped entity with
/// the referenced name exist anywhere in the current repo graph? Qualified
/// names look up their final segment (the defining entity's own name).
fn agent_use_unresolved_reference_repo_graph_lookup(
    store: &SqliteGraphStore,
    reference_name: &str,
) -> Result<UnresolvedReferenceRepoGraphLookup, String> {
    let after_path = reference_name.rsplit("::").next().unwrap_or(reference_name);
    let lookup_name = after_path.rsplit('.').next().unwrap_or(after_path).trim();
    let entities = store
        .find_entities_by_exact_symbol(lookup_name)
        .map_err(|error| format!("repo graph lookup for `{lookup_name}` failed: {error}"))?;
    let definitions = entities
        .iter()
        .filter(|entity| crate::validation_journal::entity_kind_defines_symbol(entity.kind))
        .collect::<Vec<_>>();
    Ok(UnresolvedReferenceRepoGraphLookup {
        lookup_name: lookup_name.to_string(),
        definition_count: definitions.len(),
        top_candidate: definitions.first().map(|entity| {
            format!(
                "{} ({})",
                entity.qualified_name,
                normalize_repo_relative_path(&entity.repo_relative_path)
            )
        }),
    })
}

fn agent_use_unresolved_reference_warning(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    entry: &UnresolvedReferenceDeltaEntry,
    lookup: &UnresolvedReferenceRepoGraphLookup,
    block_on_unresolved_local: bool,
) -> ValidationFinding {
    let escalated = lookup.definition_count == 0;
    let relation_label = entry.relation.to_ascii_lowercase();
    let reason = if escalated {
        format!(
            "new unresolved {relation_label} reference `{}` is a likely hallucinated symbol: no defining entity named `{}` exists anywhere in the current repo graph (lookup scope: whole-graph exact symbol dictionary)",
            entry.name, lookup.lookup_name
        )
    } else {
        format!(
            "new unresolved {relation_label} reference `{}` did not resolve to a graph edge; {} definition candidate(s) named `{}` exist elsewhere in the repo (top candidate: {}) but the link could not be proven",
            entry.name,
            lookup.definition_count,
            lookup.lookup_name,
            lookup.top_candidate.as_deref().unwrap_or("unknown")
        )
    };
    let evidence_id = format!(
        "unresolved-reference://{}:{}:{}",
        entry.repo_relative_path, entry.source_span.start_line, entry.name
    );
    // The opt-in promotion must survive the MVP3.6 severity model, which
    // demotes block findings carrying only non-graph evidence. The promoted
    // case cites the same fresh contradiction pair the sticky blockers do:
    // the current source references the name at an exact span AND a fresh
    // whole-graph lookup found no defining entity.
    let promoted = escalated && block_on_unresolved_local;
    let mut input =
        ValidationReverificationInput::exact_graph_source(lifecycle, evidence_id.clone(), &reason);
    input.relation_exact = false;
    input.source_span_required = false;
    input.provenance_required = false;
    input.provenance_present = true;
    if promoted {
        input.evidence_items = vec![ValidationEvidenceItem::graph_source(
            evidence_id,
            format!(
                "fresh re-verified contradiction: {}:{} references `{}` in the current source, and a fresh whole-graph lookup found no defining entity named `{}`",
                entry.repo_relative_path,
                entry.source_span.start_line,
                entry.name,
                lookup.lookup_name
            ),
        )];
    } else {
        input.graph_source_relation_reverified = false;
        input.evidence_items = vec![ValidationEvidenceItem::non_graph(
            ValidationEvidenceKind::Diagnostic,
            evidence_id,
            &reason,
        )];
    }
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_9_5_4/unresolved-reference/{:016x}",
            agent_use_validation_stable_u64(&entry.stable_identity_key)
        ),
        input,
    );
    if promoted {
        finding.classification = ValidationClassification::Block;
        finding.blocking_level = ValidationBlockingLevel::Blocking;
        finding.proof_strength = "reverified_absence_of_definition".to_string();
    } else {
        finding.classification = ValidationClassification::Warn;
        finding.blocking_level = ValidationBlockingLevel::Warning;
        finding.proof_status = ValidationProofStatus::NotGraphProof;
        finding.proof_level = "not_graph_proof".to_string();
        finding.proof_strength = "text_evidence".to_string();
        finding.reverified_graph_source_proof = false;
    }
    finding.affected_delta = json!({
        "unresolved_reference": {
            "name": entry.name,
            "relation": entry.relation,
            "reference_class": entry.reference_class,
            "repo_graph_lookup": if escalated {
                format!("no_defining_entity_named_{}", lookup.lookup_name)
            } else {
                format!("{}_definition_candidates_exist", lookup.definition_count)
            },
            "claimability": "claimable_as_source_text_reference_only",
        }
    });
    finding.affected_file = Some(normalize_repo_relative_path(&entry.repo_relative_path));
    finding.source_span = Some(entry.source_span.clone());
    finding.source_role = Some(EvidenceRole::Unknown);
    finding.relation_kind = entry.relation.parse().ok();
    finding.exactness = Some(Exactness::StaticHeuristic);
    finding.old_fact_claim_state = "claimable_as_source_text_reference_only".to_string();
    finding.new_fact_claim_state = "claimable_as_source_text_reference_only".to_string();
    finding.reason = reason;
    finding.recommended_fix = Some(format!(
        "Define `{}` with a source span, fix the name, or mark the dependency external.",
        entry.name
    ));
    finding.suggested_next_steps = vec![format!(
        "Define `{}` with a source span, fix the name, or mark the dependency external.",
        entry.name
    )];
    finding.diagnostics = vec!["unresolved_reference_not_graph_proof".to_string()];
    finding.expansion_handle = Some("validation_packet:unresolved_references".to_string());
    finding
}

/// Evaluates the §1.3.4 CG_MVP3_REF_* family over the delta's new unresolved
/// references on changed files and returns the §1.3.5 packet block. CALLEE
/// rows are the callsite-side mirror of CALLS rows and are skipped so one
/// hallucinated call yields one finding.
fn agent_use_collect_unresolved_reference_findings(
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    delta: &EntitySourceRoleDeltaReport,
    changed_files: &[String],
    findings: &mut Vec<ValidationFinding>,
) -> Result<Value, String> {
    const ESCALATED_INLINE_LIMIT: usize = 3;
    let changed_set = changed_files
        .iter()
        .map(|path| normalize_repo_relative_path(path))
        .collect::<BTreeSet<_>>();
    let block_on_unresolved_local = agent_use_block_on_unresolved_local();
    let mut by_class = BTreeMap::<String, usize>::new();
    let mut new_count = 0usize;
    let mut escalated_inline = Vec::<Value>::new();
    let mut escalated_total = 0usize;
    let mut external_or_builtin_count = 0usize;
    let mut dynamic_or_computed_count = 0usize;

    for entry in &delta.unresolved_references_added {
        if !changed_set.contains(&normalize_repo_relative_path(&entry.repo_relative_path)) {
            continue;
        }
        if entry.relation == "CALLEE" {
            continue;
        }
        new_count += 1;
        *by_class.entry(entry.reference_class.clone()).or_insert(0) += 1;
        if entry.reference_class == REFERENCE_CLASS_REPO_LOCAL_CANDIDATE {
            let rule_id = match entry.relation.as_str() {
                "CALLS" => CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL,
                "IMPORTS" | "ALIAS_OF" | "ALIASED_BY" | "REEXPORTS" => {
                    CG_MVP3_REF_NEW_UNRESOLVED_IMPORT
                }
                _ => continue,
            };
            let Some(rule) = rule_by_id.get(rule_id) else {
                continue;
            };
            let lookup = agent_use_unresolved_reference_repo_graph_lookup(store, &entry.name)?;
            // Parser-level references cannot see cross-file resolution, so
            // "definition candidates exist" is the NORMAL state for a valid
            // new cross-module call or import — warning there fires on the
            // very edit that fixes a hallucination (2026-06-11 adversarial
            // finding). Only the no-definition-anywhere case escalates;
            // candidates-exist references stay visible in the block's
            // class counts as diagnostics.
            if lookup.definition_count > 0 {
                continue;
            }
            escalated_total += 1;
            let finding = agent_use_unresolved_reference_warning(
                rule,
                lifecycle.clone(),
                entry,
                &lookup,
                block_on_unresolved_local,
            );
            if escalated_inline.len() < ESCALATED_INLINE_LIMIT {
                escalated_inline.push(json!({
                    "name": entry.name,
                    "relation": entry.relation,
                    "reference_class": entry.reference_class,
                    "file": entry.repo_relative_path,
                    "span": {
                        "line_start": entry.source_span.start_line,
                        "line_end": entry.source_span.end_line,
                    },
                    "repo_graph_lookup": if lookup.definition_count == 0 {
                        format!("no_defining_entity_named_{}", lookup.lookup_name)
                    } else {
                        format!("{}_definition_candidates_exist", lookup.definition_count)
                    },
                    "severity": if lookup.definition_count == 0 && block_on_unresolved_local {
                        "blocking"
                    } else {
                        "warning"
                    },
                    "proof_strength": "text_evidence",
                    "claimability": "claimable_as_source_text_reference_only",
                    "recommended_fix": format!(
                        "Define {} with a source span, fix the name, or mark the dependency external.",
                        entry.name
                    ),
                }));
            }
            findings.push(finding);
        } else if entry.reference_class == REFERENCE_CLASS_DYNAMIC_OR_COMPUTED {
            dynamic_or_computed_count += 1;
        } else {
            external_or_builtin_count += 1;
        }
    }

    let resolved_count = delta
        .unresolved_references_removed
        .iter()
        .filter(|entry| {
            entry.relation != "CALLEE"
                && changed_set.contains(&normalize_repo_relative_path(&entry.repo_relative_path))
        })
        .count();

    Ok(json!({
        "schema_version": 1,
        "new_count": new_count,
        "resolved_count": resolved_count,
        "by_class": by_class,
        "escalated": escalated_inline,
        "escalated_total": escalated_total,
        "escalated_omitted_count": escalated_total.saturating_sub(
            escalated_inline.len().min(escalated_total)
        ),
        "external_or_builtin_count": external_or_builtin_count,
        "dynamic_or_computed_count": dynamic_or_computed_count,
        "block_on_unresolved_local": block_on_unresolved_local,
        "expansion_handle": "validation_packet:unresolved_references",
        "not_graph_proof": true,
    }))
}

fn agent_use_exact_imports_validation_rules() -> Vec<ValidationRule> {
    let mut missing_source_span = ValidationRule::exact_blocking(
        CG_MVP3_IMPORTS_MISSING_SOURCE_SPAN,
        ValidationRuleKind::ProofIntegrity,
        Some(RelationKind::Imports),
        "claimable import proof edges must carry a current source span",
        "Regenerate the import edge with a valid source span or downgrade the edge.",
    );
    missing_source_span.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
    missing_source_span.source_role_requirement = ValidationSourceRoleRequirement::RolePreserved;

    let mut missing_provenance = ValidationRule::exact_blocking(
        CG_MVP3_IMPORTS_DERIVED_MISSING_PROVENANCE,
        ValidationRuleKind::ProofIntegrity,
        Some(RelationKind::Imports),
        "derived import proof edges must carry provenance",
        "Attach provenance edges or downgrade the derived import fact.",
    );
    missing_provenance.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
    missing_provenance.provenance_requirement =
        ValidationProvenanceRequirement::RequiredForDerivedEdges;
    missing_provenance.source_span_requirement =
        ValidationSourceSpanRequirement::RequiredForClaimableGraphFact;
    missing_provenance.source_role_requirement = ValidationSourceRoleRequirement::RolePreserved;

    vec![
        ValidationRule::exact_blocking(
            CG_MVP3_IMPORTS_DANGLING_TARGET,
            ValidationRuleKind::DanglingTarget,
            Some(RelationKind::Imports),
            "new exact IMPORTS edges must resolve to a current import target entity",
            "Define the imported target or update the exact import relation.",
        ),
        ValidationRule::exact_blocking(
            CG_MVP3_IMPORTS_DELETED_EXPORT_STILL_IMPORTED,
            ValidationRuleKind::DanglingTarget,
            Some(RelationKind::Imports),
            "deleted exact exported targets must not remain imported",
            "Update the import, restore the exported target, or downgrade non-exact evidence.",
        ),
        ValidationRule::exact_blocking(
            CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED,
            ValidationRuleKind::BrokenContract,
            Some(RelationKind::Imports),
            "renamed exact import targets must not leave imports pointing at the old target",
            "Retarget the import to the renamed module or symbol, or report the rename as unknown.",
        ),
        ValidationRule::exact_blocking(
            CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH,
            ValidationRuleKind::BrokenContract,
            Some(RelationKind::AliasedBy),
            "exact import alias edges must resolve to their current target",
            "Update the alias import or restore the aliased target.",
        ),
        ValidationRule::exact_blocking(
            CG_MVP3_IMPORTS_TARGET_ROLE_MISMATCH,
            ValidationRuleKind::SourceRoleBoundary,
            Some(RelationKind::Imports),
            "production import proof must not resolve only to test, mock, or stub targets",
            "Move the target into production evidence, update the import, or mark the relation as test/mock evidence.",
        ),
        missing_source_span,
        missing_provenance,
    ]
}

fn agent_use_proof_integrity_validation_rules() -> Vec<ValidationRule> {
    let mut edge_missing_span = ValidationRule::exact_blocking(
        CG_MVP3_PROOF_EDGE_MISSING_SOURCE_SPAN,
        ValidationRuleKind::ProofIntegrity,
        None,
        "claimable graph-proof edges must carry a current source span",
        "Regenerate the edge with a valid source span or downgrade the edge to diagnostic evidence.",
    );
    edge_missing_span.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
    edge_missing_span.source_role_requirement = ValidationSourceRoleRequirement::RolePreserved;

    let mut entity_missing_span = ValidationRule::exact_blocking(
        CG_MVP3_CLAIMABLE_ENTITY_MISSING_SOURCE_SPAN,
        ValidationRuleKind::ProofIntegrity,
        None,
        "claimable source-bound entities must carry a current source span when the entity kind requires one",
        "Regenerate the entity with a valid source span or mark this entity kind as diagnostic/span-optional.",
    );
    entity_missing_span.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
    entity_missing_span.source_role_requirement = ValidationSourceRoleRequirement::RolePreserved;

    let mut derived_missing_provenance = ValidationRule::exact_blocking(
        CG_MVP3_DERIVED_EDGE_MISSING_PROVENANCE,
        ValidationRuleKind::ProofIntegrity,
        None,
        "derived graph-proof edges must carry provenance",
        "Attach provenance edges or downgrade the derived edge to diagnostic evidence.",
    );
    derived_missing_provenance.proof_requirement =
        ValidationProofRequirement::ReverifiedGraphIntegrity;
    derived_missing_provenance.provenance_requirement =
        ValidationProvenanceRequirement::RequiredForDerivedEdges;
    derived_missing_provenance.source_span_requirement =
        ValidationSourceSpanRequirement::RequiredForClaimableGraphFact;
    derived_missing_provenance.source_role_requirement =
        ValidationSourceRoleRequirement::RolePreserved;

    let mut lifecycle_rules = [
        (
            CG_MVP3_DB_LIFECYCLE_MISMATCH_AFTER_UPDATE,
            "validation lifecycle must match the exact DB path opened and updated",
            "Reject the packet, rerun lifecycle preflight for the exact DB path, and retry the update.",
        ),
        (
            CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT,
            "stale, foreign, partial, or schema-mismatched DB states must not produce claimable validation",
            "Reindex or select the correct external profile DB before attempting claimable validation.",
        ),
        (
            CG_MVP3_TEMP_DB_CLAIMABLE,
            "temporary or publish-stage DB files must never be claimable",
            "Keep the old-good DB visible and publish only after the final DB is complete.",
        ),
        (
            CG_MVP3_OLD_GOOD_DB_NOT_PRESERVED,
            "failed updates must preserve the old-good claimable DB",
            "Abort the update, restore the old-good DB, and rerun the changed-file update.",
        ),
        (
            CG_MVP3_CORRUPT_OR_INCOMPLETE_UPDATE_TRANSACTION,
            "corrupt or incomplete update transactions must block claimable validation",
            "Discard the incomplete update state and rebuild or retry through the lifecycle-safe update path.",
        ),
        (
            CG_MVP3_QUERY_DURING_UPDATE_UNSAFE,
            "query and context reads during publish/update must use old-good or non-claimable state only",
            "Wait for publish completion or serve only the old-good DB with non-claimable update diagnostics.",
        ),
        (
            CG_MVP3_SIDECAR_CORRUPT_VS_INACCESSIBLE_MISCLASSIFIED,
            "sidecar access failures must not be misclassified as corruption without corruption proof",
            "Classify inaccessible sidecars separately and keep sidecar freshness diagnostic-only.",
        ),
        (
            CG_MVP3_GRAPH_DELTA_BOUNDED,
            "bounded validation work (delta entry omission or reverification budget exhaustion) cannot claim absence of contradictions",
            "Run `agent-use index` for a full reindex, then re-validate the changed files.",
        ),
    ]
    .into_iter()
    .map(|(rule_id, invariant, docs_summary)| {
        let mut rule = ValidationRule::exact_blocking(
            rule_id,
            ValidationRuleKind::LifecycleIntegrity,
            None,
            invariant,
            docs_summary,
        );
        rule.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
        rule.source_span_requirement = ValidationSourceSpanRequirement::NotApplicable;
        rule.provenance_requirement = ValidationProvenanceRequirement::NotApplicable;
        rule.source_role_requirement = ValidationSourceRoleRequirement::NotApplicable;
        rule.lifecycle_requirement = ValidationLifecycleRequirement::ClaimableCurrentDb;
        rule.activation_condition =
            "claimable current DB lifecycle plus reverified validation-state invariant".to_string();
        rule
    })
    .collect::<Vec<_>>();

    let mut rules = vec![
        edge_missing_span,
        entity_missing_span,
        derived_missing_provenance,
    ];
    rules.append(&mut lifecycle_rules);
    rules
}

fn agent_use_source_role_tests_validation_rules() -> Vec<ValidationRule> {
    let source_role_rules = [
        (
            CG_MVP3_SOURCE_ROLE_TEST_EVIDENCE_IN_PRODUCTION_PROOF,
            "production proof paths must not include test evidence",
            "Keep test evidence out of production proof or mark the relation as test-impact evidence.",
        ),
        (
            CG_MVP3_SOURCE_ROLE_MOCK_EVIDENCE_IN_PRODUCTION_PROOF,
            "production proof paths must not include mock evidence",
            "Keep mock evidence out of production proof or mark the relation as mock/test evidence.",
        ),
        (
            CG_MVP3_SOURCE_ROLE_STUB_EVIDENCE_IN_PRODUCTION_PROOF,
            "production proof paths must not include stub evidence",
            "Keep stub evidence out of production proof or mark the relation as stub/test evidence.",
        ),
        (
            CG_MVP3_SOURCE_ROLE_INLINE_TEST_PROMOTED_TO_PRODUCTION,
            "inline test modules must not be emitted as production proof",
            "Preserve inline test source-role metadata and exclude inline tests from production proof.",
        ),
        (
            CG_MVP3_SOURCE_ROLE_GENERATED_EVIDENCE_AS_PRODUCTION_PROOF,
            "generated or degraded evidence must not be treated as production graph proof",
            "Exclude generated evidence from production proof or downgrade it to diagnostic evidence.",
        ),
    ]
    .into_iter()
    .map(|(rule_id, invariant, docs_summary)| {
        let mut rule = ValidationRule::exact_blocking(
            rule_id,
            ValidationRuleKind::SourceRoleBoundary,
            None,
            invariant,
            docs_summary,
        );
        rule.activation_condition =
            "claimable exact edge treated as production plus source-role re-verification"
                .to_string();
        rule
    })
    .collect::<Vec<_>>();

    let tests_dangling = ValidationRule::exact_blocking(
        CG_MVP3_TESTS_DANGLING_TARGET,
        ValidationRuleKind::DanglingTarget,
        Some(RelationKind::Tests),
        "exact TESTS edges must resolve to a current test target",
        "Restore the tested target, update the TESTS relation, or downgrade unsupported evidence.",
    );
    let mut asserts_dangling = ValidationRule::exact_blocking(
        CG_MVP3_ASSERTS_DANGLING_TARGET,
        ValidationRuleKind::DanglingTarget,
        Some(RelationKind::Asserts),
        "exact ASSERTS edges must resolve to a current assertion target",
        "Restore the assertion target, update the ASSERTS relation, or downgrade unsupported evidence.",
    );
    asserts_dangling.supported_relation_status = SupportedRelationStatus::ExactWarningCandidate;
    let mut optional_missing = ValidationRule::exact_blocking(
        CG_MVP3_OPTIONAL_TEST_TARGET_MISSING,
        ValidationRuleKind::DanglingTarget,
        Some(RelationKind::Tests),
        "optional test targets may be absent but must be reported as warning evidence",
        "Keep the optional test target warning explicit and do not promote it to production proof.",
    );
    optional_missing.supported_relation_status = SupportedRelationStatus::ExactWarningCandidate;
    let mut unsupported_tests = ValidationRule::exact_blocking(
        CG_MVP3_UNSUPPORTED_TEST_RELATION_UNKNOWN,
        ValidationRuleKind::UnsupportedRelationBoundary,
        None,
        "unsupported TESTS/ASSERTS/MOCKS/STUBS evidence must stay unknown or diagnostic",
        "Do not block or claim graph proof for unsupported or heuristic test relation evidence.",
    );
    unsupported_tests.supported_relation_status = SupportedRelationStatus::Unsupported;
    unsupported_tests.default_classification_when_unsupported = ValidationClassification::Unknown;
    unsupported_tests.activation_condition =
        "TESTS/ASSERTS/MOCKS/STUBS relation evidence is unsupported, heuristic, or not exact"
            .to_string();

    let mut rules = source_role_rules;
    rules.push(tests_dangling);
    rules.push(asserts_dangling);
    rules.push(optional_missing);
    rules.push(unsupported_tests);
    rules
}

fn agent_use_activation_gated_contract_validation_rules() -> Vec<ValidationRule> {
    let mut reads_missing = ValidationRule::exact_blocking(
        CG_MVP3_READS_DANGLING_SYMBOL,
        ValidationRuleKind::DanglingTarget,
        Some(RelationKind::Reads),
        "exact READS edges must resolve to a current symbol entity",
        "Restore the read symbol, update the READS relation, or downgrade unsupported evidence.",
    );
    reads_missing.activation_condition =
        agent_use_strict_activation_gate_condition("READS").to_string();

    let mut writes_missing = ValidationRule::exact_blocking(
        CG_MVP3_WRITES_DANGLING_SYMBOL,
        ValidationRuleKind::DanglingTarget,
        Some(RelationKind::Writes),
        "exact WRITES edges must resolve to a current symbol entity",
        "Restore the written symbol, update the WRITES relation, or downgrade unsupported evidence.",
    );
    writes_missing.activation_condition =
        agent_use_strict_activation_gate_condition("WRITES").to_string();

    let mut reads_writes_role = ValidationRule::exact_blocking(
        CG_MVP3_READS_WRITES_TARGET_ROLE_MISMATCH,
        ValidationRuleKind::SourceRoleBoundary,
        None,
        "production READS/WRITES proof must not resolve only to test, mock, or stub targets",
        "Retarget the production access, move the target into production evidence, or mark the relation as test/mock evidence.",
    );
    reads_writes_role.activation_condition =
        agent_use_strict_activation_gate_condition("READS/WRITES").to_string();

    let mut reads_writes_missing_span = ValidationRule::exact_blocking(
        CG_MVP3_READS_WRITES_MISSING_SOURCE_SPAN,
        ValidationRuleKind::ProofIntegrity,
        None,
        "claimable READS/WRITES proof edges must carry a current source span",
        "Regenerate the READS/WRITES edge with a valid source span or downgrade the edge.",
    );
    reads_writes_missing_span.proof_requirement =
        ValidationProofRequirement::ReverifiedGraphIntegrity;
    reads_writes_missing_span.source_role_requirement =
        ValidationSourceRoleRequirement::RolePreserved;
    reads_writes_missing_span.activation_condition =
        agent_use_strict_activation_gate_condition("READS/WRITES source span").to_string();

    let mut reads_writes_missing_provenance = ValidationRule::exact_blocking(
        CG_MVP3_READS_WRITES_DERIVED_MISSING_PROVENANCE,
        ValidationRuleKind::ProofIntegrity,
        None,
        "derived READS/WRITES proof edges must carry provenance",
        "Attach provenance edges or downgrade the derived READS/WRITES fact.",
    );
    reads_writes_missing_provenance.proof_requirement =
        ValidationProofRequirement::ReverifiedGraphIntegrity;
    reads_writes_missing_provenance.provenance_requirement =
        ValidationProvenanceRequirement::RequiredForDerivedEdges;
    reads_writes_missing_provenance.source_role_requirement =
        ValidationSourceRoleRequirement::RolePreserved;
    reads_writes_missing_provenance.activation_condition =
        agent_use_strict_activation_gate_condition("derived READS/WRITES provenance").to_string();

    let mut route_missing = ValidationRule::exact_blocking(
        CG_MVP3_ROUTE_HANDLER_DANGLING_TARGET,
        ValidationRuleKind::DanglingTarget,
        Some(RelationKind::Handles),
        "exact route/handler edges must resolve to a current handler or endpoint target",
        "Restore the handler/endpoint, update the route relation, or downgrade unsupported evidence.",
    );
    route_missing.activation_condition =
        agent_use_strict_activation_gate_condition("literal route/handler").to_string();

    let mut route_renamed = ValidationRule::exact_blocking(
        CG_MVP3_ROUTE_HANDLER_RENAMED_NOT_UPDATED,
        ValidationRuleKind::BrokenContract,
        Some(RelationKind::Handles),
        "renamed exact route handlers must not leave fresh route edges pointing at old targets",
        "Retarget the route handler or represent the rename as unknown when path identity is ambiguous.",
    );
    route_renamed.activation_condition =
        agent_use_strict_activation_gate_condition("renamed literal route/handler").to_string();

    let mut route_computed = ValidationRule::exact_blocking(
        CG_MVP3_ROUTE_COMPUTED_UNKNOWN,
        ValidationRuleKind::UnsupportedRelationBoundary,
        Some(RelationKind::Handles),
        "computed route or handler evidence must stay unknown unless exact graph/source proof exists",
        "Report computed routes as unknown/diagnostic until an exact source-spanned route relation is available.",
    );
    route_computed.supported_relation_status = SupportedRelationStatus::Unsupported;
    route_computed.default_classification_when_unsupported = ValidationClassification::Unknown;
    route_computed.proof_requirement = ValidationProofRequirement::NotGraphProof;
    route_computed.activation_condition =
        "computed route/handler evidence, heuristic handler edge, or non-exact route adapter"
            .to_string();

    let mut unsupported_framework = ValidationRule::exact_blocking(
        CG_MVP3_ROUTE_UNSUPPORTED_FRAMEWORK_UNKNOWN,
        ValidationRuleKind::UnsupportedRelationBoundary,
        Some(RelationKind::Handles),
        "unsupported route frameworks must stay unknown rather than blocking proof",
        "Add fixture-backed exact relation support before making unsupported framework route failures blocking.",
    );
    unsupported_framework.supported_relation_status = SupportedRelationStatus::Unsupported;
    unsupported_framework.default_classification_when_unsupported =
        ValidationClassification::Unknown;
    unsupported_framework.proof_requirement = ValidationProofRequirement::NotGraphProof;
    unsupported_framework.activation_condition =
        "route framework relation class is unsupported, convention-only, or lacks exact source-spanned evidence"
            .to_string();

    let mut config_exact = ValidationRule::exact_blocking(
        CG_MVP3_CONFIG_PACKAGE_EXACT_MISMATCH,
        ValidationRuleKind::BrokenContract,
        Some(RelationKind::Configures),
        "Config.in/package mismatch may block only when exact graph/source Configures proof exists",
        "Add fixture-backed exact Config.in/package graph relation support before blocking.",
    );
    config_exact.supported_relation_status = SupportedRelationStatus::Unsupported;
    config_exact.default_classification_when_unsupported = ValidationClassification::Unknown;
    config_exact.proof_requirement = ValidationProofRequirement::NotGraphProof;
    config_exact.activation_condition =
        "not activated: current Config.in/package support is text evidence only".to_string();

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

    let mut config_unsupported = ValidationRule::exact_blocking(
        CG_MVP3_CONFIG_PACKAGE_UNSUPPORTED_UNKNOWN,
        ValidationRuleKind::UnsupportedRelationBoundary,
        None,
        "unsupported Config.in/package relation classes must stay unknown rather than blocking",
        "Keep unsupported package/build-system relation classes explicit until exact graph/source support exists.",
    );
    config_unsupported.supported_relation_status = SupportedRelationStatus::Unsupported;
    config_unsupported.default_classification_when_unsupported = ValidationClassification::Unknown;
    config_unsupported.proof_requirement = ValidationProofRequirement::NotGraphProof;
    config_unsupported.activation_condition =
        "package/build-system exact relation class is unsupported or not fixture-backed"
            .to_string();

    vec![
        reads_missing,
        writes_missing,
        reads_writes_role,
        reads_writes_missing_span,
        reads_writes_missing_provenance,
        route_missing,
        route_renamed,
        route_computed,
        unsupported_framework,
        config_exact,
        config_text,
        config_unsupported,
    ]
}

fn agent_use_strict_activation_gate_condition(relation_family: &str) -> &'static str {
    match relation_family {
        "READS" => "READS relation exists as exact graph/source proof; target identity deterministic; source span present; source role valid; DB lifecycle claimable/current; relation class activated by parser/schema; source span re-verification succeeds",
        "WRITES" => "WRITES relation exists as exact graph/source proof; target identity deterministic; source span present; source role valid; DB lifecycle claimable/current; relation class activated by parser/schema; source span re-verification succeeds",
        "READS/WRITES" => "READS/WRITES relation exists as exact graph/source proof; target identity deterministic; source span present; source role valid; DB lifecycle claimable/current; relation class activated by parser/schema; source span re-verification succeeds",
        "READS/WRITES source span" => "READS/WRITES edge is claimable graph proof and graph/source integrity re-verification proves the required source span is missing",
        "derived READS/WRITES provenance" => "derived READS/WRITES edge is claimable graph proof and graph/source integrity re-verification proves required provenance is missing",
        "literal route/handler" => "route/handler relation exists as exact source-spanned graph proof for a literal route or direct handler; target identity deterministic; source role valid; DB lifecycle claimable/current; source span re-verification succeeds",
        "renamed literal route/handler" => "rename evidence plus exact source-spanned route/handler graph relation can be reverified against current source and target identity remains path-aware",
        _ => "exact source-spanned relation plus graph/source re-verification",
    }
}

fn agent_use_attach_activation_gated_contract_metadata(
    mut packet: ValidationPacket,
    delta: &EntitySourceRoleDeltaReport,
) -> ValidationPacket {
    packet.relation_family_status = agent_use_relation_family_status_json();
    packet.activation_gate_state = agent_use_activation_gate_state_json(delta);
    packet
}

fn agent_use_relation_family_status_json() -> Value {
    json!({
        "reads_writes": {
            "status": "exact_blocking_candidate",
            "relation_kinds": ["READS", "WRITES"],
            "activated": true,
            "unsupported_or_dynamic_behavior": "unknown_not_blocking",
            "blocking_requires": [
                "exact_graph_source_relation",
                "deterministic_target_identity",
                "current_source_span",
                "valid_source_role",
                "claimable_current_lifecycle",
                "activated_parser_schema_relation",
                "source_span_reverification"
            ]
        },
        "route_handler": {
            "status": "exact_blocking_candidate",
            "relation_kinds": ["HANDLES", "EXPOSES"],
            "activated": true,
            "activation_scope": "literal route or direct handler edges only",
            "computed_routes": "unknown_not_blocking",
            "unsupported_frameworks": "unknown_not_blocking"
        },
        "config_package": {
            "status": "text_evidence_only",
            "exact_graph_mismatch": "unsupported_unknown_until_fixture_backed",
            "text_evidence_warning_only": true,
            "graph_proof": false
        },
        "package_build_system_text_evidence": {
            "status": "text_evidence_only",
            "files": [".mk", "Config.in", ".adoc", ".md", "shell/support scripts"],
            "graph_proof": false,
            "blocking": false
        },
        "unsupported_relation_families": [
            "computed_reads_writes",
            "computed_routes",
            "framework_convention_routes",
            "exact_config_package_graph_mismatch"
        ]
    })
}

fn agent_use_activation_gate_state_json(delta: &EntitySourceRoleDeltaReport) -> Value {
    json!({
        "gate_version": "mvp3_3_activation_gated_contract_checks_v1",
        "blocking_gate_requirements": {
            "relation_exists_as_exact_graph_source_proof": true,
            "target_identity_deterministic": true,
            "source_span_required": true,
            "source_role_valid_for_validation_mode": true,
            "lifecycle_claimable_current": true,
            "relation_class_activated_by_frontend_schema": true,
            "source_span_reverification_required": true,
            "non_graph_evidence_blocking_allowed": false
        },
        "changed_files": delta.closure_delta_summary.changed_files,
        "closure_files_updated": delta.closure_files_updated,
        "text_evidence_changed_count": delta.text_evidence_changed_count,
        "text_evidence_not_graph_entity_delta": delta.text_evidence_not_graph_entity_delta,
        "text_candidate_evidence_not_graph_delta": delta.text_candidate_evidence_not_graph_delta,
        "source_navigation_only_not_graph_entity_delta": delta.source_navigation_only_not_graph_entity_delta,
        "unsupported_relation_classes": delta.unsupported_relation_classes,
        "degraded_relation_classes": delta.degraded_relation_classes,
        "hard_interrupt_eligibility_gate": "implemented",
        "hard_interrupt_not_implemented": false
    })
}

fn agent_use_collect_lifecycle_integrity_findings(
    profile: &AgentUseProfile,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    preflight: &DbLifecyclePreflight,
    delta: &EntitySourceRoleDeltaReport,
) -> Vec<ValidationFinding> {
    let mut findings = Vec::new();
    let expected_db_path = profile.db_path.display().to_string();
    if preflight.exact_db_path_checked != expected_db_path {
        agent_use_push_lifecycle_integrity_finding(
            &mut findings,
            rule_by_id[CG_MVP3_DB_LIFECYCLE_MISMATCH_AFTER_UPDATE],
            lifecycle.clone(),
            json!({
                "expected_db_path": expected_db_path,
                "exact_db_path_checked": preflight.exact_db_path_checked.clone(),
            }),
            "validation preflight did not check the same DB path as the profile DB path",
        );
    }

    if !preflight.safe || !lifecycle.is_claimable_current() {
        agent_use_push_lifecycle_integrity_finding(
            &mut findings,
            rule_by_id[CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT],
            lifecycle.clone(),
            json!({
                "safe": preflight.safe,
                "db_problem_kind": preflight.db_problem_kind.clone(),
                "schema_status": preflight.schema_status.clone(),
                "passport_status": preflight.db_health.passport_status.clone(),
                "blockers": preflight.blockers.clone(),
            }),
            "validation attempted against a DB lifecycle state that is not claimable current",
        );
    }

    let publish_state = agent_use_publish_state_json(profile);
    let publish_active = publish_state
        .get("publishing")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let publish_status = publish_state
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    if publish_active && matches!(publish_status, "updating" | "publishing") {
        agent_use_push_lifecycle_integrity_finding(
            &mut findings,
            rule_by_id[CG_MVP3_QUERY_DURING_UPDATE_UNSAFE],
            lifecycle.clone(),
            publish_state.clone(),
            "validation observed an active update/publish state that must not be served as claimable query context",
        );
    }

    if agent_use_db_path_looks_like_publish_temp(&profile.db_path)
        && lifecycle.is_claimable_current()
    {
        agent_use_push_lifecycle_integrity_finding(
            &mut findings,
            rule_by_id[CG_MVP3_TEMP_DB_CLAIMABLE],
            lifecycle.clone(),
            json!({
                "db_path": profile.db_path.display().to_string(),
                "reason": "profile DB path has a publish/temp DB filename pattern",
            }),
            "a publish/temp DB path was claimable",
        );
    }

    if agent_use_update_state_mentions_corrupt_or_incomplete(delta, preflight) {
        agent_use_push_lifecycle_integrity_finding(
            &mut findings,
            rule_by_id[CG_MVP3_CORRUPT_OR_INCOMPLETE_UPDATE_TRANSACTION],
            lifecycle.clone(),
            json!({
                "delta_status": delta.status.clone(),
                "old_snapshot_status": delta.old_snapshot_status.clone(),
                "new_snapshot_status": delta.new_snapshot_status.clone(),
                "passport_status": preflight.db_health.passport_status.clone(),
                "db_health_reasons": preflight.db_health.reasons.clone(),
            }),
            "validation observed corrupt, interrupted, partial, or incomplete update state",
        );
    }

    if !delta.old_good_db_preserved {
        agent_use_push_lifecycle_integrity_finding(
            &mut findings,
            rule_by_id[CG_MVP3_OLD_GOOD_DB_NOT_PRESERVED],
            lifecycle.clone(),
            json!({
                "old_good_db_preserved": delta.old_good_db_preserved,
                "delta_status": delta.status.clone(),
            }),
            "graph delta reported that the old-good DB was not preserved",
        );
    }

    if !delta.access_vs_corrupt_classification_safe {
        agent_use_push_lifecycle_integrity_finding(
            &mut findings,
            rule_by_id[CG_MVP3_SIDECAR_CORRUPT_VS_INACCESSIBLE_MISCLASSIFIED],
            lifecycle,
            json!({
                "access_vs_corrupt_classification_safe": delta.access_vs_corrupt_classification_safe,
                "sidecar_freshness_changed": delta.sidecar_freshness_changed.clone(),
            }),
            "sidecar access/corrupt classification was not safe",
        );
    }

    findings
}

fn agent_use_collect_proof_integrity_findings(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    delta: &EntitySourceRoleDeltaReport,
    changed_files: &[String],
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    let mut scan_paths = changed_files
        .iter()
        .chain(delta.closure_files_updated.iter())
        .map(|path| normalize_repo_relative_path(path))
        .collect::<Vec<_>>();
    scan_paths.sort();
    scan_paths.dedup();

    for path in scan_paths {
        let edges = store
            .list_edges_by_file(&path)
            .map_err(|error| format!("read changed-file proof-integrity edges failed: {error}"))?;
        for edge in edges {
            agent_use_validate_current_proof_edge_integrity(
                profile,
                rule_by_id,
                lifecycle.clone(),
                &edge,
                Some(json!({
                    "source": "changed_or_closure_file_current_edges",
                    "repo_relative_path": path,
                })),
                findings,
                seen_edge_rule,
            );
        }

        let entities = store.list_entities_by_file(&path).map_err(|error| {
            format!("read changed-file proof-integrity entities failed: {error}")
        })?;
        for entity in entities {
            agent_use_validate_current_entity_integrity(
                profile,
                rule_by_id,
                lifecycle.clone(),
                &entity,
                Some(json!({
                    "source": "changed_or_closure_file_current_entities",
                    "repo_relative_path": path,
                })),
                findings,
            );
        }
    }

    for edge_delta in delta.edges_added.iter().chain(delta.edges_changed.iter()) {
        if !agent_use_edge_delta_claimable_graph_proof(edge_delta) {
            continue;
        }
        if !edge_delta.source_span.repo_relative_path.trim().is_empty()
            && !(edge_delta.derived && edge_delta.provenance_edges.is_empty())
        {
            continue;
        }
        if let Some(edge) = store
            .get_edge(&edge_delta.edge_id)
            .map_err(|error| format!("read proof-integrity delta edge failed: {error}"))?
        {
            agent_use_validate_current_proof_edge_integrity(
                profile,
                rule_by_id,
                lifecycle.clone(),
                &edge,
                Some(json!(edge_delta)),
                findings,
                seen_edge_rule,
            );
        } else {
            agent_use_push_edge_delta_integrity_finding(
                findings,
                rule_by_id,
                lifecycle.clone(),
                edge_delta,
            );
        }
    }

    for entity_delta in delta
        .entities_added
        .iter()
        .chain(delta.entities_changed.iter())
    {
        if !entity_delta.claimability.claimable || !entity_delta.claimability.graph_proof {
            continue;
        }
        if entity_delta.source_span.is_some() {
            continue;
        }
        agent_use_push_entity_delta_integrity_finding(
            findings,
            rule_by_id[CG_MVP3_CLAIMABLE_ENTITY_MISSING_SOURCE_SPAN],
            lifecycle.clone(),
            entity_delta,
        );
    }

    Ok(())
}

fn agent_use_collect_source_role_tests_findings(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    delta: &EntitySourceRoleDeltaReport,
    changed_files: &[String],
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    let mut scan_paths = changed_files
        .iter()
        .chain(delta.closure_files_updated.iter())
        .map(|path| normalize_repo_relative_path(path))
        .collect::<Vec<_>>();
    scan_paths.sort();
    scan_paths.dedup();

    for path in scan_paths {
        let edges = store
            .list_edges_by_file(&path)
            .map_err(|error| format!("read changed-file source-role edges failed: {error}"))?;
        for edge in edges {
            agent_use_validate_current_source_role_boundary_edge(
                profile,
                store,
                rule_by_id,
                lifecycle.clone(),
                &edge,
                Some(json!({
                    "source": "changed_or_closure_file_source_role_edges",
                    "repo_relative_path": path,
                })),
                findings,
                seen_edge_rule,
            )?;
            agent_use_validate_current_tests_relation_edge(
                profile,
                store,
                rule_by_id,
                lifecycle.clone(),
                &edge,
                Some(json!({
                    "source": "changed_or_closure_file_tests_relation_edges",
                    "repo_relative_path": path,
                })),
                findings,
                seen_edge_rule,
            )?;
        }
    }

    for entry in delta.edges_added.iter().chain(delta.edges_changed.iter()) {
        let Some(edge) = store
            .get_edge(&entry.edge_id)
            .map_err(|error| format!("read source-role/test delta edge failed: {error}"))?
        else {
            continue;
        };
        agent_use_validate_current_source_role_boundary_edge(
            profile,
            store,
            rule_by_id,
            lifecycle.clone(),
            &edge,
            Some(json!(entry)),
            findings,
            seen_edge_rule,
        )?;
        agent_use_validate_current_tests_relation_edge(
            profile,
            store,
            rule_by_id,
            lifecycle.clone(),
            &edge,
            Some(json!(entry)),
            findings,
            seen_edge_rule,
        )?;
    }

    for entry in delta.source_roles_changed.iter() {
        if entry.old_source_role == Some(EvidenceRole::Production)
            && matches!(
                entry.new_source_role,
                Some(EvidenceRole::Test | EvidenceRole::Mock | EvidenceRole::Mixed)
            )
        {
            findings.push(agent_use_source_role_delta_diagnostic(
                rule_by_id[CG_MVP3_SOURCE_ROLE_TEST_EVIDENCE_IN_PRODUCTION_PROOF],
                lifecycle.clone(),
                json!(entry),
                "source role changed away from production; production validation must respect the new role",
            ));
        }
    }

    Ok(())
}

fn agent_use_collect_activation_gated_contract_findings(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    delta: &EntitySourceRoleDeltaReport,
    changed_files: &[String],
    renamed_old_paths: &BTreeSet<String>,
    removed_entity_paths_by_id: &BTreeMap<String, String>,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    for removed_entity_id in removed_entity_paths_by_id.keys() {
        for relation in [
            RelationKind::Reads,
            RelationKind::Writes,
            RelationKind::Handles,
            RelationKind::Exposes,
        ] {
            let incoming = store
                .find_edges_by_tail_relation(removed_entity_id, relation)
                .map_err(|error| format!("read incoming {relation} edges failed: {error}"))?;
            for edge in incoming {
                agent_use_validate_current_activation_gated_contract_edge(
                    profile,
                    store,
                    rule_by_id,
                    lifecycle.clone(),
                    &edge,
                    Some(json!({
                        "source": "removed_target_current_incoming_edges",
                        "removed_entity_id": removed_entity_id,
                        "removed_entity_path": removed_entity_paths_by_id.get(removed_entity_id),
                    })),
                    renamed_old_paths,
                    removed_entity_paths_by_id,
                    findings,
                    seen_edge_rule,
                )?;
            }
        }
    }

    for entry in delta
        .edges_added
        .iter()
        .chain(delta.edges_changed.iter())
        .filter(|entry| agent_use_activation_gated_contract_relation(entry.relation))
    {
        let Some(edge) = store
            .get_edge(&entry.edge_id)
            .map_err(|error| format!("read activation-gated delta edge failed: {error}"))?
        else {
            continue;
        };
        agent_use_validate_current_activation_gated_contract_edge(
            profile,
            store,
            rule_by_id,
            lifecycle.clone(),
            &edge,
            Some(json!(entry)),
            renamed_old_paths,
            removed_entity_paths_by_id,
            findings,
            seen_edge_rule,
        )?;
    }

    for entry in delta
        .edges_removed
        .iter()
        .filter(|entry| agent_use_route_validation_relation(entry.relation))
    {
        let target_path = entry
            .target_endpoint
            .repo_relative_path
            .as_deref()
            .map(normalize_repo_relative_path)
            .unwrap_or_else(|| normalize_repo_relative_path(&entry.repo_relative_path));
        let rule_id = if renamed_old_paths.contains(&target_path) {
            CG_MVP3_ROUTE_HANDLER_RENAMED_NOT_UPDATED
        } else {
            CG_MVP3_ROUTE_HANDLER_DANGLING_TARGET
        };
        agent_use_validate_removed_route_delta_edge(
            profile,
            store,
            rule_by_id,
            lifecycle.clone(),
            entry,
            rule_id,
            findings,
            seen_edge_rule,
        )?;
    }

    let mut scan_paths = changed_files
        .iter()
        .chain(delta.closure_files_updated.iter())
        .map(|path| normalize_repo_relative_path(path))
        .collect::<Vec<_>>();
    scan_paths.sort();
    scan_paths.dedup();
    for path in scan_paths {
        let edges = store
            .list_edges_by_file(&path)
            .map_err(|error| format!("read activation-gated changed-file edges failed: {error}"))?;
        for edge in edges
            .into_iter()
            .filter(|edge| agent_use_activation_gated_contract_relation(edge.relation))
        {
            agent_use_validate_current_activation_gated_contract_edge(
                profile,
                store,
                rule_by_id,
                lifecycle.clone(),
                &edge,
                Some(json!({
                    "source": "changed_or_closure_file_activation_gated_edges",
                    "repo_relative_path": path,
                })),
                renamed_old_paths,
                removed_entity_paths_by_id,
                findings,
                seen_edge_rule,
            )?;
        }
    }

    let mut warned_text_paths = BTreeSet::<String>::new();
    for entry in delta.text_evidence_changed.iter() {
        let path = normalize_repo_relative_path(&entry.repo_relative_path);
        if !agent_use_path_is_config_package_text_evidence(&path)
            || !warned_text_paths.insert(path.clone())
        {
            continue;
        }
        findings.push(agent_use_config_package_text_evidence_warning(
            rule_by_id[CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING],
            lifecycle.clone(),
            &path,
            json!(entry),
            "Config.in/package/build-system text evidence changed; this is warning/no-proof fallback context, not broken graph proof",
        ));
    }

    Ok(())
}

fn agent_use_activation_gated_contract_relation(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Reads | RelationKind::Writes | RelationKind::Handles | RelationKind::Exposes
    )
}

fn agent_use_route_validation_relation(relation: RelationKind) -> bool {
    matches!(relation, RelationKind::Handles | RelationKind::Exposes)
}

fn agent_use_validate_current_activation_gated_contract_edge(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    affected_delta: Option<Value>,
    renamed_old_paths: &BTreeSet<String>,
    removed_entity_paths_by_id: &BTreeMap<String, String>,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    match edge.relation {
        RelationKind::Reads | RelationKind::Writes => agent_use_validate_current_reads_writes_edge(
            profile,
            store,
            rule_by_id,
            lifecycle,
            edge,
            affected_delta,
            findings,
            seen_edge_rule,
        ),
        RelationKind::Handles | RelationKind::Exposes => {
            let preferred_rule_id = agent_use_route_missing_target_rule_for_current_edge(
                edge,
                removed_entity_paths_by_id,
                renamed_old_paths,
            );
            agent_use_validate_current_route_handler_edge(
                profile,
                store,
                rule_by_id,
                lifecycle,
                edge,
                affected_delta,
                preferred_rule_id,
                findings,
                seen_edge_rule,
            )
        }
        _ => Ok(()),
    }
}

fn agent_use_validate_current_reads_writes_edge(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    affected_delta: Option<Value>,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    if !matches!(edge.relation, RelationKind::Reads | RelationKind::Writes) {
        return Ok(());
    }

    let span_check = agent_use_reverify_edge_source_span(&profile.repo_root, edge);
    if span_check.is_err() {
        agent_use_push_relation_contract_finding(
            findings,
            seen_edge_rule,
            rule_by_id[CG_MVP3_READS_WRITES_MISSING_SOURCE_SPAN],
            edge,
            None,
            affected_delta.clone(),
            agent_use_graph_integrity_input(
                lifecycle.clone(),
                edge,
                true,
                false,
                false,
                true,
                span_check.as_ref().err().cloned().unwrap_or_else(|| {
                    "READS/WRITES source span failed graph/source recheck".to_string()
                }),
            ),
            "claimable exact READS/WRITES edge does not have a current valid source span",
            "Regenerate the READS/WRITES edge from source or downgrade the edge to diagnostic evidence.",
        );
        return Ok(());
    }

    if edge.derived && edge.provenance_edges.is_empty() {
        agent_use_push_relation_contract_finding(
            findings,
            seen_edge_rule,
            rule_by_id[CG_MVP3_READS_WRITES_DERIVED_MISSING_PROVENANCE],
            edge,
            None,
            affected_delta.clone(),
            agent_use_graph_integrity_input(
                lifecycle.clone(),
                edge,
                false,
                true,
                true,
                false,
                "derived exact READS/WRITES edge is missing provenance_edges",
            ),
            "derived exact READS/WRITES edge lacks required provenance",
            "Attach provenance edge IDs or downgrade the derived READS/WRITES fact.",
        );
        return Ok(());
    }

    let missing_rule_id = match edge.relation {
        RelationKind::Reads => CG_MVP3_READS_DANGLING_SYMBOL,
        RelationKind::Writes => CG_MVP3_WRITES_DANGLING_SYMBOL,
        _ => unreachable!("filtered READS/WRITES relation"),
    };

    if !agent_use_exactness_is_proof_grade(edge.exactness) {
        findings.push(agent_use_relation_contract_unknown(
            rule_by_id[missing_rule_id],
            lifecycle,
            edge,
            affected_delta.unwrap_or_else(|| json!(agent_use_generic_validation_edge_json(edge))),
            "dynamic, computed, heuristic, or unsupported READS/WRITES evidence is unknown and cannot block",
        ));
        return Ok(());
    }

    let target = store
        .get_entity(&edge.tail_id)
        .map_err(|error| format!("read READS/WRITES target entity failed: {error}"))?;
    let source_role = classify_edge_evidence_role(edge).role;
    let head_role = store
        .get_entity(&edge.head_id)
        .map_err(|error| format!("read READS/WRITES source entity failed: {error}"))?
        .as_ref()
        .map(classify_entity_source_role)
        .map(|decision| decision.role)
        .unwrap_or(source_role);
    let production_source = source_role.is_production() || head_role.is_production();

    if target.is_none() {
        agent_use_push_relation_contract_finding(
            findings,
            seen_edge_rule,
            rule_by_id[missing_rule_id],
            edge,
            None,
            affected_delta,
            agent_use_graph_source_input(
                lifecycle,
                edge,
                production_source,
                "current exact READS/WRITES edge was reverified from store/source and its target symbol entity is absent",
            ),
            "exact READS/WRITES edge points to a missing symbol entity",
            "Restore the symbol, update the access relation, or downgrade non-exact evidence.",
        );
        return Ok(());
    }

    let target = target.expect("checked target presence");
    let target_role = classify_entity_source_role(&target).role;
    if production_source && matches!(target_role, EvidenceRole::Test | EvidenceRole::Mock) {
        agent_use_push_relation_contract_finding(
            findings,
            seen_edge_rule,
            rule_by_id[CG_MVP3_READS_WRITES_TARGET_ROLE_MISMATCH],
            edge,
            Some(&target),
            affected_delta,
            agent_use_graph_source_input(
                lifecycle,
                edge,
                true,
                "production exact READS/WRITES edge was reverified from store/source and resolves only to non-production target evidence",
            ),
            "production exact READS/WRITES edge resolves to test/mock/stub target evidence",
            "Move the target into production evidence, update the access relation, or mark this relation as test/mock evidence.",
        );
    }

    Ok(())
}

fn agent_use_validate_current_route_handler_edge(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    affected_delta: Option<Value>,
    preferred_missing_target_rule_id: &'static str,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    if !matches!(edge.relation, RelationKind::Handles | RelationKind::Exposes) {
        return Ok(());
    }

    let span_check = agent_use_reverify_edge_source_span(&profile.repo_root, edge);
    if span_check.is_err() {
        findings.push(agent_use_relation_contract_unknown(
            rule_by_id[CG_MVP3_ROUTE_COMPUTED_UNKNOWN],
            lifecycle,
            edge,
            affected_delta.unwrap_or_else(|| json!(agent_use_generic_validation_edge_json(edge))),
            "route/handler edge cannot block because its source span could not be reverified",
        ));
        return Ok(());
    }

    if !agent_use_exactness_is_proof_grade(edge.exactness) {
        findings.push(agent_use_relation_contract_unknown(
            rule_by_id[CG_MVP3_ROUTE_COMPUTED_UNKNOWN],
            lifecycle,
            edge,
            affected_delta.unwrap_or_else(|| json!(agent_use_generic_validation_edge_json(edge))),
            "computed, convention-only, or heuristic route/handler evidence is unknown rather than blocking proof",
        ));
        return Ok(());
    }

    let target = store
        .get_entity(&edge.tail_id)
        .map_err(|error| format!("read route/handler target entity failed: {error}"))?;
    let source_role = classify_edge_evidence_role(edge).role;
    let head_role = store
        .get_entity(&edge.head_id)
        .map_err(|error| format!("read route/handler source entity failed: {error}"))?
        .as_ref()
        .map(classify_entity_source_role)
        .map(|decision| decision.role)
        .unwrap_or(source_role);
    let source_role_allowed = source_role.is_production() || head_role.is_production();

    if target.is_none() {
        agent_use_push_relation_contract_finding(
            findings,
            seen_edge_rule,
            rule_by_id[preferred_missing_target_rule_id],
            edge,
            None,
            affected_delta,
            agent_use_graph_source_input(
                lifecycle,
                edge,
                source_role_allowed,
                "current exact route/handler edge was reverified from store/source and its target entity is absent",
            ),
            "exact route/handler edge points to a missing target entity",
            "Restore the handler/endpoint, update the route relation, or downgrade unsupported evidence.",
        );
    }

    Ok(())
}

fn agent_use_route_missing_target_rule_for_current_edge(
    edge: &Edge,
    removed_entity_paths_by_id: &BTreeMap<String, String>,
    renamed_old_paths: &BTreeSet<String>,
) -> &'static str {
    if let Some(path) = removed_entity_paths_by_id.get(&edge.tail_id) {
        if renamed_old_paths.contains(path) {
            return CG_MVP3_ROUTE_HANDLER_RENAMED_NOT_UPDATED;
        }
    }
    CG_MVP3_ROUTE_HANDLER_DANGLING_TARGET
}

fn agent_use_validate_removed_route_delta_edge(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    entry: &codegraph_index::EdgeDeltaEntry,
    rule_id: &'static str,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    let rule = rule_by_id[rule_id];
    let edge = agent_use_validation_edge_from_delta_entry(entry);
    if !agent_use_exactness_is_proof_grade(entry.exactness) {
        findings.push(agent_use_relation_contract_unknown(
            rule,
            lifecycle,
            &edge,
            json!(entry),
            "removed non-exact route/handler delta is unknown and cannot block",
        ));
        return Ok(());
    }

    let span_text = match agent_use_reverify_edge_source_span_text(&profile.repo_root, &edge) {
        Ok(span_text) => span_text,
        Err(error) => {
            findings.push(agent_use_relation_contract_unknown(
                rule,
                lifecycle,
                &edge,
                json!({
                    "removed_route_delta": entry,
                    "source_span_recheck_error": error,
                }),
                "removed exact route/handler delta cannot block because the current source span could not be reverified",
            ));
            return Ok(());
        }
    };

    if !agent_use_removed_route_delta_span_still_names_target(&span_text, entry) {
        findings.push(agent_use_relation_contract_unknown(
            rule,
            lifecycle,
            &edge,
            json!({
                "removed_route_delta": entry,
                "source_span_text": span_text,
            }),
            "removed exact route/handler delta source span no longer names the old handler target, so it is not blocking proof",
        ));
        return Ok(());
    }

    let target = store
        .get_entity(&edge.tail_id)
        .map_err(|error| format!("read removed route/handler target entity failed: {error}"))?;
    let source_role = classify_edge_evidence_role(&edge).role;
    let head_role = store
        .get_entity(&edge.head_id)
        .map_err(|error| format!("read removed route/handler source entity failed: {error}"))?
        .as_ref()
        .map(classify_entity_source_role)
        .map(|decision| decision.role)
        .unwrap_or(source_role);
    let source_role_allowed = source_role.is_production() || head_role.is_production();

    agent_use_push_relation_contract_finding(
        findings,
        seen_edge_rule,
        rule,
        &edge,
        target.as_ref(),
        Some(json!(entry)),
        agent_use_graph_source_input(
            lifecycle,
            &edge,
            source_role_allowed,
            "removed exact route/handler edge was graph/source reverified from the current source span and the route still names the old handler target without a current exact HANDLES/EXPOSES relation",
        ),
        "exact route/handler edge was removed while the route source still names the old handler target",
        "Restore the handler, update the route registration to a current exact handler, or re-index after a deliberate safe route rewrite.",
    );
    Ok(())
}

fn agent_use_push_relation_contract_finding(
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
    rule: &ValidationRule,
    edge: &Edge,
    target: Option<&Entity>,
    affected_delta: Option<Value>,
    input: ValidationReverificationInput,
    reason: &str,
    recommended_fix: &str,
) {
    let key = format!("{}:{}", rule.validation_rule_id, edge.id);
    if !seen_edge_rule.insert(key) {
        return;
    }
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/activation-gated/{}/{}",
            rule.validation_rule_id, edge.id
        ),
        input,
    );
    finding.affected_delta = affected_delta.unwrap_or(Value::Null);
    finding.affected_edge = agent_use_generic_validation_edge_json(edge);
    finding.affected_entity = target
        .map(|entity| serde_json::to_value(entity).unwrap_or(Value::Null))
        .unwrap_or_else(|| {
            json!({
                "entity_id": edge.tail_id,
                "missing": true,
            })
        });
    finding.affected_file = Some(normalize_repo_relative_path(
        &edge.source_span.repo_relative_path,
    ));
    finding.source_span = Some(edge.source_span.clone());
    finding.source_role = Some(classify_edge_evidence_role(edge).role);
    finding.relation_kind = Some(edge.relation);
    finding.exactness = Some(edge.exactness);
    finding.provenance = json!({
        "derived": edge.derived,
        "provenance_edges": edge.provenance_edges,
        "provenance_required": edge.derived,
        "provenance_present": !edge.derived || !edge.provenance_edges.is_empty(),
    });
    finding.reason = reason.to_string();
    finding.recommended_fix = Some(recommended_fix.to_string());
    finding.suggested_next_steps = vec![recommended_fix.to_string()];
    finding.expansion_handle = Some(format!("validation_packet:edge:{}", edge.id));
    findings.push(finding);
}

fn agent_use_relation_contract_unknown(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    affected_delta: Value,
    reason: &str,
) -> ValidationFinding {
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle,
        format!("evidence://activation-gated/{}", edge.id),
        reason,
    );
    input.relation_supported = false;
    input.relation_exact = false;
    input.graph_source_relation_reverified = false;
    input.unsupported_relation = true;
    input.evidence_items = vec![ValidationEvidenceItem::non_graph(
        ValidationEvidenceKind::Diagnostic,
        format!("diagnostic://activation-gated/{}", edge.id),
        reason,
    )];
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/activation-gated/unknown/{:016x}",
            agent_use_validation_stable_u64(&format!("{}:{reason}", edge.id))
        ),
        input,
    );
    finding.affected_delta = affected_delta;
    finding.affected_edge = agent_use_generic_validation_edge_json(edge);
    finding.affected_file = Some(normalize_repo_relative_path(
        &edge.source_span.repo_relative_path,
    ));
    finding.source_span = Some(edge.source_span.clone());
    finding.source_role = Some(classify_edge_evidence_role(edge).role);
    finding.relation_kind = Some(edge.relation);
    finding.exactness = Some(edge.exactness);
    finding.reason = reason.to_string();
    finding
}

fn agent_use_config_package_text_evidence_warning(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    repo_relative_path: &str,
    affected_delta: Value,
    reason: &str,
) -> ValidationFinding {
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
            "finding://mvp3_3/config-package/text-warning/{:016x}",
            agent_use_validation_stable_u64(repo_relative_path)
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

#[cfg(test)]
fn agent_use_config_package_unsupported_unknown(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    affected_delta: Value,
    reason: &str,
) -> ValidationFinding {
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle,
        "diagnostic://config-package-unsupported",
        reason,
    );
    input.relation_supported = false;
    input.relation_exact = false;
    input.graph_source_relation_reverified = false;
    input.unsupported_relation = true;
    input.evidence_items = vec![ValidationEvidenceItem::non_graph(
        ValidationEvidenceKind::Diagnostic,
        "diagnostic://config-package-unsupported",
        reason,
    )];
    let mut finding =
        classify_validation_finding(rule, "finding://mvp3_3/config-package/unsupported", input);
    finding.affected_delta = affected_delta;
    finding.reason = reason.to_string();
    finding
}

fn agent_use_path_is_config_package_text_evidence(path: &str) -> bool {
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

fn agent_use_validate_current_source_role_boundary_edge(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    affected_delta: Option<Value>,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    if !agent_use_edge_can_be_claimable_graph_proof(edge)
        || !agent_use_exactness_is_proof_grade(edge.exactness)
    {
        return Ok(());
    }
    if agent_use_reverify_edge_source_span(&profile.repo_root, edge).is_err() {
        return Ok(());
    }
    if !agent_use_edge_treated_as_production_proof(edge) {
        return Ok(());
    }

    let head = store
        .get_entity(&edge.head_id)
        .map_err(|error| format!("read source-role edge head failed: {error}"))?;
    let tail = store
        .get_entity(&edge.tail_id)
        .map_err(|error| format!("read source-role edge tail failed: {error}"))?;
    let Some(boundary) =
        agent_use_source_role_boundary_for_edge(edge, head.as_ref(), tail.as_ref())
    else {
        return Ok(());
    };

    let rule = rule_by_id[boundary.rule_id];
    agent_use_push_source_role_finding(
        findings,
        seen_edge_rule,
        rule,
        lifecycle,
        edge,
        head.as_ref(),
        tail.as_ref(),
        affected_delta,
        boundary.source_role,
        boundary.reason,
        boundary.recommended_fix,
    );
    Ok(())
}

fn agent_use_validate_current_tests_relation_edge(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    affected_delta: Option<Value>,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    if !agent_use_tests_relation(edge.relation) {
        return Ok(());
    }

    if !agent_use_exactness_is_proof_grade(edge.exactness) {
        findings.push(agent_use_tests_relation_unknown(
            rule_by_id[CG_MVP3_UNSUPPORTED_TEST_RELATION_UNKNOWN],
            lifecycle,
            edge,
            affected_delta.unwrap_or_else(|| json!(agent_use_generic_validation_edge_json(edge))),
            "TESTS/ASSERTS/MOCKS/STUBS evidence is heuristic or unsupported, so it is unknown rather than blocking proof",
        ));
        return Ok(());
    }

    if agent_use_reverify_edge_source_span(&profile.repo_root, edge).is_err() {
        return Ok(());
    }

    let target = store
        .get_entity(&edge.tail_id)
        .map_err(|error| format!("read TESTS-family target failed: {error}"))?;
    if target.is_some() {
        return Ok(());
    }

    let rule_id = if agent_use_edge_has_optional_test_target(edge) {
        CG_MVP3_OPTIONAL_TEST_TARGET_MISSING
    } else {
        match edge.relation {
            RelationKind::Tests => CG_MVP3_TESTS_DANGLING_TARGET,
            RelationKind::Asserts => CG_MVP3_ASSERTS_DANGLING_TARGET,
            RelationKind::Mocks | RelationKind::Stubs => CG_MVP3_OPTIONAL_TEST_TARGET_MISSING,
            _ => CG_MVP3_UNSUPPORTED_TEST_RELATION_UNKNOWN,
        }
    };
    let rule = rule_by_id[rule_id];
    agent_use_push_tests_relation_finding(
        findings,
        seen_edge_rule,
        rule,
        edge,
        affected_delta,
        agent_use_graph_source_input(
            lifecycle,
            edge,
            true,
            "exact TESTS-family edge was reverified from graph/source and its target entity is absent",
        ),
        if rule_id == CG_MVP3_OPTIONAL_TEST_TARGET_MISSING {
            "optional TESTS-family target is missing"
        } else {
            "exact TESTS-family edge points to a missing target entity"
        },
        if rule_id == CG_MVP3_OPTIONAL_TEST_TARGET_MISSING {
            "Keep the missing optional test target as a warning and do not promote it to production proof."
        } else {
            "Restore the test target, update the relation, or downgrade unsupported evidence."
        },
    );
    Ok(())
}

fn agent_use_validate_current_proof_edge_integrity(
    profile: &AgentUseProfile,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    affected_delta: Option<Value>,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) {
    if !agent_use_edge_can_be_claimable_graph_proof(edge) {
        return;
    }

    let span_check = agent_use_reverify_edge_source_span(&profile.repo_root, edge);
    if span_check.is_err() {
        let reason = span_check
            .as_ref()
            .err()
            .cloned()
            .unwrap_or_else(|| "edge source span failed graph/source recheck".to_string());
        agent_use_push_proof_edge_integrity_finding(
            findings,
            seen_edge_rule,
            rule_by_id[CG_MVP3_PROOF_EDGE_MISSING_SOURCE_SPAN],
            edge,
            affected_delta.clone(),
            agent_use_graph_integrity_input(
                lifecycle.clone(),
                edge,
                true,
                false,
                false,
                true,
                reason,
            ),
            "claimable graph-proof edge does not have a current valid source span",
            "Regenerate the edge from source or downgrade it to diagnostic evidence.",
        );
        return;
    }

    if edge.derived && edge.provenance_edges.is_empty() {
        agent_use_push_proof_edge_integrity_finding(
            findings,
            seen_edge_rule,
            rule_by_id[CG_MVP3_DERIVED_EDGE_MISSING_PROVENANCE],
            edge,
            affected_delta,
            agent_use_graph_integrity_input(
                lifecycle,
                edge,
                false,
                true,
                true,
                false,
                "derived graph-proof edge is missing provenance_edges",
            ),
            "derived graph-proof edge lacks required provenance",
            "Attach provenance edge IDs or downgrade the derived edge.",
        );
    }
}

fn agent_use_validate_current_entity_integrity(
    profile: &AgentUseProfile,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    entity: &Entity,
    affected_delta: Option<Value>,
    findings: &mut Vec<ValidationFinding>,
) {
    if !agent_use_entity_can_be_claimable_graph_proof(entity) {
        return;
    }
    let span_required = agent_use_entity_kind_requires_source_span(entity.kind);
    let span_check = entity
        .source_span
        .as_ref()
        .map(|span| {
            agent_use_reverify_source_span_text(
                &profile.repo_root,
                span,
                "claimable entity graph proof",
            )
        })
        .unwrap_or_else(|| Err("claimable entity has no source span".to_string()));
    if span_check.is_ok() {
        return;
    }

    let reason = span_check
        .err()
        .unwrap_or_else(|| "entity source span failed graph/source recheck".to_string());
    let rule = rule_by_id[CG_MVP3_CLAIMABLE_ENTITY_MISSING_SOURCE_SPAN];
    let input = if span_required {
        agent_use_entity_graph_integrity_input(lifecycle, entity, true, false, reason.clone())
    } else {
        agent_use_entity_optional_span_diagnostic_input(lifecycle, entity, reason.clone())
    };
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/proof_integrity/{}/{}",
            rule.validation_rule_id, entity.id
        ),
        input,
    );
    finding.affected_delta = affected_delta.unwrap_or(Value::Null);
    finding.affected_entity = serde_json::to_value(entity).unwrap_or(Value::Null);
    finding.affected_file = Some(normalize_repo_relative_path(&entity.repo_relative_path));
    finding.source_span = entity.source_span.clone();
    finding.source_role = Some(classify_entity_source_role(entity).role);
    finding.reason = if span_required {
        "claimable source-bound entity does not have a current valid source span".to_string()
    } else {
        "claimable entity kind has optional source spans; missing/invalid span is diagnostic only"
            .to_string()
    };
    finding.recommended_fix = Some(
        "Regenerate the entity from source or keep this entity kind/span state diagnostic-only."
            .to_string(),
    );
    finding.expansion_handle = Some(format!("validation_packet:entity:{}", entity.id));
    findings.push(finding);
}

fn agent_use_push_lifecycle_integrity_finding(
    findings: &mut Vec<ValidationFinding>,
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    affected_delta: Value,
    reason: &str,
) {
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/lifecycle/{}/{:016x}",
            rule.validation_rule_id,
            agent_use_validation_stable_u64(reason)
        ),
        agent_use_lifecycle_integrity_input(lifecycle, rule.validation_rule_id.clone(), reason),
    );
    finding.affected_delta = affected_delta;
    finding.reason = reason.to_string();
    finding.recommended_fix = Some(rule.docs_summary.clone());
    finding.suggested_next_steps = vec![rule.docs_summary.clone()];
    findings.push(finding);
}

fn agent_use_push_proof_edge_integrity_finding(
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
    rule: &ValidationRule,
    edge: &Edge,
    affected_delta: Option<Value>,
    input: ValidationReverificationInput,
    reason: &str,
    recommended_fix: &str,
) {
    let key = format!("{}:{}", rule.validation_rule_id, edge.id);
    if !seen_edge_rule.insert(key) {
        return;
    }
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/proof_integrity/{}/{}",
            rule.validation_rule_id, edge.id
        ),
        input,
    );
    finding.affected_delta = affected_delta.unwrap_or(Value::Null);
    finding.affected_edge = agent_use_generic_validation_edge_json(edge);
    finding.affected_file = Some(normalize_repo_relative_path(
        &edge.source_span.repo_relative_path,
    ));
    finding.source_span = Some(edge.source_span.clone());
    finding.source_role = Some(classify_edge_evidence_role(edge).role);
    finding.relation_kind = Some(edge.relation);
    finding.exactness = Some(edge.exactness);
    finding.provenance = json!({
        "derived": edge.derived,
        "provenance_edges": edge.provenance_edges,
        "provenance_required": edge.derived,
        "provenance_present": !edge.derived || !edge.provenance_edges.is_empty(),
    });
    finding.reason = reason.to_string();
    finding.recommended_fix = Some(recommended_fix.to_string());
    finding.suggested_next_steps = vec![recommended_fix.to_string()];
    finding.expansion_handle = Some(format!("validation_packet:edge:{}", edge.id));
    findings.push(finding);
}

fn agent_use_push_edge_delta_integrity_finding(
    findings: &mut Vec<ValidationFinding>,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    edge_delta: &codegraph_index::EdgeDeltaEntry,
) {
    let (
        rule_id,
        source_span_required,
        source_span_present,
        provenance_required,
        provenance_present,
        reason,
    ) = if edge_delta.derived && edge_delta.provenance_edges.is_empty() {
        (
                CG_MVP3_DERIVED_EDGE_MISSING_PROVENANCE,
                false,
                true,
                true,
                false,
                "claimable derived edge delta is missing provenance and current edge row was unavailable",
            )
    } else {
        (
            CG_MVP3_PROOF_EDGE_MISSING_SOURCE_SPAN,
            true,
            false,
            false,
            true,
            "claimable edge delta is missing source span and current edge row was unavailable",
        )
    };
    let rule = rule_by_id[rule_id];
    let mut input =
        agent_use_lifecycle_integrity_input(lifecycle, edge_delta.edge_id.clone(), reason);
    input.source_span_required = source_span_required;
    input.source_span_present = source_span_present;
    input.provenance_required = provenance_required;
    input.provenance_present = provenance_present;
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/proof_integrity_delta/{}/{}",
            rule.validation_rule_id, edge_delta.edge_id
        ),
        input,
    );
    finding.affected_delta = json!(edge_delta);
    finding.affected_edge = json!(edge_delta);
    finding.affected_file = Some(normalize_repo_relative_path(&edge_delta.repo_relative_path));
    finding.source_span = Some(edge_delta.source_span.clone());
    finding.source_role = Some(edge_delta.source_role);
    finding.relation_kind = Some(edge_delta.relation);
    finding.exactness = Some(edge_delta.exactness);
    finding.reason = reason.to_string();
    findings.push(finding);
}

fn agent_use_push_entity_delta_integrity_finding(
    findings: &mut Vec<ValidationFinding>,
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    entity_delta: &codegraph_index::EntityDeltaEntry,
) {
    let span_required = agent_use_entity_kind_requires_source_span(entity_delta.entity_kind);
    let evidence_id = entity_delta
        .new
        .as_ref()
        .or(entity_delta.old.as_ref())
        .map(|summary| summary.entity_id.clone())
        .unwrap_or_else(|| entity_delta.stable_identity_key.clone());
    let mut input = agent_use_lifecycle_integrity_input(
        lifecycle,
        evidence_id.clone(),
        "claimable entity delta is missing source span",
    );
    input.source_span_required = span_required;
    input.source_span_present = false;
    if !span_required {
        input.evidence_items = vec![ValidationEvidenceItem::non_graph(
            ValidationEvidenceKind::Diagnostic,
            evidence_id,
            "entity kind source span is optional; missing span is diagnostic",
        )];
        input.integrity_issue_present = false;
    }
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/proof_integrity_entity_delta/{}/{}",
            rule.validation_rule_id, entity_delta.stable_identity_key
        ),
        input,
    );
    finding.affected_delta = json!(entity_delta);
    finding.affected_entity = json!(entity_delta);
    finding.affected_file = Some(normalize_repo_relative_path(
        &entity_delta.repo_relative_path,
    ));
    finding.source_span = entity_delta.source_span.clone();
    finding.source_role = Some(entity_delta.source_role);
    finding.reason = if span_required {
        "claimable source-bound entity delta is missing source span".to_string()
    } else {
        "claimable entity delta has optional source span; missing span is diagnostic".to_string()
    };
    findings.push(finding);
}

fn agent_use_validate_current_calls_edge(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    affected_delta: Option<Value>,
    preferred_missing_target_rule_id: Option<&str>,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    if edge.relation != RelationKind::Calls {
        return Ok(());
    }
    let span_check = agent_use_reverify_edge_source_span(&profile.repo_root, edge);
    if span_check.is_err() {
        let rule = rule_by_id[CG_MVP3_CALLS_MISSING_SOURCE_SPAN];
        agent_use_push_calls_finding(
            findings,
            seen_edge_rule,
            rule,
            edge,
            None,
            affected_delta.clone(),
            agent_use_graph_integrity_input(
                lifecycle.clone(),
                edge,
                true,
                false,
                false,
                true,
                span_check
                    .as_ref()
                    .err()
                    .cloned()
                    .unwrap_or_else(|| "CALLS source span failed graph/source recheck".to_string()),
            ),
            "claimable exact CALLS edge does not have a current valid source span",
            "Regenerate the CALLS edge from source or downgrade the edge to diagnostic evidence.",
        );
        return Ok(());
    }

    if edge.derived && edge.provenance_edges.is_empty() {
        let rule = rule_by_id[CG_MVP3_CALLS_DERIVED_MISSING_PROVENANCE];
        agent_use_push_calls_finding(
            findings,
            seen_edge_rule,
            rule,
            edge,
            None,
            affected_delta.clone(),
            agent_use_graph_integrity_input(
                lifecycle.clone(),
                edge,
                false,
                true,
                true,
                false,
                "derived exact CALLS edge is missing provenance_edges",
            ),
            "derived exact CALLS edge lacks required provenance",
            "Attach provenance edge IDs or downgrade the derived CALLS fact.",
        );
        return Ok(());
    }

    if !agent_use_exactness_is_proof_grade(edge.exactness) {
        let rule = rule_by_id[CG_MVP3_CALLS_DANGLING_TARGET];
        findings.push(agent_use_calls_boundary_diagnostic(
            rule,
            lifecycle,
            affected_delta.unwrap_or_else(|| json!(agent_use_validation_edge_json(edge, None))),
            "heuristic, dynamic, inferred, or unresolved CALLS evidence is diagnostic only",
        ));
        return Ok(());
    }

    let target = store
        .get_entity(&edge.tail_id)
        .map_err(|error| format!("read CALLS target entity failed: {error}"))?;
    let source_role = classify_edge_evidence_role(edge).role;
    let caller_role = store
        .get_entity(&edge.head_id)
        .map_err(|error| format!("read CALLS caller entity failed: {error}"))?
        .as_ref()
        .map(classify_entity_source_role)
        .map(|decision| decision.role)
        .unwrap_or(source_role);
    let production_source = source_role.is_production() || caller_role.is_production();

    if target.is_none() {
        let rule =
            rule_by_id[preferred_missing_target_rule_id.unwrap_or(CG_MVP3_CALLS_DANGLING_TARGET)];
        agent_use_push_calls_finding(
            findings,
            seen_edge_rule,
            rule,
            edge,
            None,
            affected_delta,
            agent_use_graph_source_input(
                lifecycle,
                edge,
                production_source,
                "current exact CALLS edge was reverified from store/source and its target entity is absent",
            ),
            "exact CALLS edge points to a missing callee entity",
            "Update the caller target, restore the callee, or downgrade non-exact evidence.",
        );
        return Ok(());
    }

    let target = target.expect("checked target presence");
    let target_role = classify_entity_source_role(&target).role;
    if production_source && matches!(target_role, EvidenceRole::Test | EvidenceRole::Mock) {
        let rule = rule_by_id[CG_MVP3_CALLS_TARGET_ROLE_MISMATCH];
        agent_use_push_calls_finding(
            findings,
            seen_edge_rule,
            rule,
            edge,
            Some(&target),
            affected_delta,
            agent_use_graph_source_input(
                lifecycle,
                edge,
                true,
                "production exact CALLS edge was reverified from store/source and resolves only to non-production target evidence",
            ),
            "production exact CALLS edge resolves to test/mock/stub target evidence",
            "Move the target into production evidence, update the caller, or mark this relation as test/mock evidence.",
        );
    }

    Ok(())
}

fn agent_use_collect_exact_imports_findings(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    delta: &EntitySourceRoleDeltaReport,
    changed_files: &[String],
    renamed_old_paths: &BTreeSet<String>,
    ambiguous_rename_old_paths: &BTreeSet<String>,
    removed_entity_paths_by_id: &BTreeMap<String, String>,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    for removed in delta
        .entities_removed
        .iter()
        .filter(|entry| exact_import_target_entity_kind(entry.entity_kind))
    {
        let Some(removed_entity_id) = agent_use_entity_delta_id(removed) else {
            continue;
        };
        let incoming_imports = store
            .find_edges_by_tail_relation(&removed_entity_id, RelationKind::Imports)
            .map_err(|error| format!("read incoming IMPORTS edges failed: {error}"))?;
        for edge in incoming_imports {
            let rule_id = if renamed_old_paths
                .contains(&normalize_repo_relative_path(&removed.repo_relative_path))
            {
                CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED
            } else {
                CG_MVP3_IMPORTS_DELETED_EXPORT_STILL_IMPORTED
            };
            agent_use_validate_current_import_edge(
                profile,
                store,
                rule_by_id,
                lifecycle.clone(),
                &edge,
                Some(json!(removed)),
                Some(rule_id),
                findings,
                seen_edge_rule,
            )?;
        }

        let alias_edges = store
            .find_edges_by_head_relation(&removed_entity_id, RelationKind::AliasedBy)
            .map_err(|error| format!("read exact import alias edges failed: {error}"))?;
        for edge in alias_edges {
            agent_use_validate_current_import_edge(
                profile,
                store,
                rule_by_id,
                lifecycle.clone(),
                &edge,
                Some(json!(removed)),
                Some(CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH),
                findings,
                seen_edge_rule,
            )?;
        }
    }

    for entry in delta
        .edges_removed
        .iter()
        .filter(|entry| agent_use_import_validation_relation(entry.relation))
    {
        let target_id = agent_use_import_delta_target_id(entry);
        let target_path = agent_use_import_delta_target_path(entry);
        let target_removed_or_renamed = removed_entity_paths_by_id.contains_key(target_id)
            || renamed_old_paths.contains(&target_path);
        if !target_removed_or_renamed {
            continue;
        }
        let rule_id = if entry.relation == RelationKind::AliasedBy {
            CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH
        } else if renamed_old_paths.contains(&target_path) {
            CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED
        } else {
            CG_MVP3_IMPORTS_DELETED_EXPORT_STILL_IMPORTED
        };
        agent_use_validate_removed_import_delta_edge(
            profile,
            store,
            rule_by_id,
            lifecycle.clone(),
            entry,
            rule_id,
            findings,
            seen_edge_rule,
        )?;
    }

    for entry in delta
        .edges_added
        .iter()
        .chain(delta.edges_changed.iter())
        .filter(|entry| agent_use_import_validation_relation(entry.relation))
    {
        if !agent_use_exactness_is_proof_grade(entry.exactness) {
            findings.push(agent_use_import_boundary_diagnostic(
                rule_by_id[CG_MVP3_IMPORTS_DANGLING_TARGET],
                lifecycle.clone(),
                json!(entry),
                "non-exact import delta is diagnostic only and cannot block",
            ));
            continue;
        }
        let Some(edge) = store
            .get_edge(&entry.edge_id)
            .map_err(|error| format!("read import edge failed: {error}"))?
        else {
            continue;
        };
        let missing_rule_id = if edge.relation == RelationKind::AliasedBy {
            CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH
        } else {
            CG_MVP3_IMPORTS_DANGLING_TARGET
        };
        agent_use_validate_current_import_edge(
            profile,
            store,
            rule_by_id,
            lifecycle.clone(),
            &edge,
            Some(json!(entry)),
            Some(missing_rule_id),
            findings,
            seen_edge_rule,
        )?;
    }

    let mut scan_paths = changed_files
        .iter()
        .chain(delta.closure_files_updated.iter())
        .map(|path| normalize_repo_relative_path(path))
        .collect::<Vec<_>>();
    scan_paths.sort();
    scan_paths.dedup();
    for path in scan_paths {
        let edges = store
            .list_edges_by_file(&path)
            .map_err(|error| format!("read changed-file import edges failed: {error}"))?;
        for edge in edges
            .into_iter()
            .filter(|edge| agent_use_import_validation_relation(edge.relation))
        {
            let missing_target_rule_id = agent_use_missing_import_target_rule_for_current_edge(
                &edge,
                removed_entity_paths_by_id,
                renamed_old_paths,
            );
            agent_use_validate_current_import_edge(
                profile,
                store,
                rule_by_id,
                lifecycle.clone(),
                &edge,
                Some(json!({
                    "source": "changed_or_closure_file_current_import_edges",
                    "repo_relative_path": path,
                })),
                Some(missing_target_rule_id),
                findings,
                seen_edge_rule,
            )?;
        }
    }

    for path in ambiguous_rename_old_paths {
        findings.push(agent_use_import_boundary_unknown(
            rule_by_id[CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED],
            lifecycle.clone(),
            json!({
                "rename_status": "unknown",
                "old_path": path,
                "ambiguity": true,
            }),
            "ambiguous rename cannot be promoted to exact dangling IMPORTS proof",
        ));
    }
    if !renamed_old_paths.is_empty()
        && !findings
            .iter()
            .any(|finding| finding.validation_rule_id == CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED)
    {
        for path in renamed_old_paths {
            findings.push(agent_use_import_boundary_unknown(
                rule_by_id[CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED],
                lifecycle.clone(),
                json!({
                    "rename_status": "detected",
                    "old_path": path,
                    "file_renames_detected": &delta.file_renames_detected,
                    "reason": "rename was detected from file lifecycle evidence, but no exact stale import edge could be graph/source reverified",
                }),
                "renamed import target validation is unknown because file rename evidence alone is not graph-relation proof",
            ));
        }
    }

    Ok(())
}

fn agent_use_validate_current_import_edge(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    affected_delta: Option<Value>,
    preferred_missing_target_rule_id: Option<&str>,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    if !agent_use_import_validation_relation(edge.relation) {
        return Ok(());
    }
    let span_check = agent_use_reverify_edge_source_span(&profile.repo_root, edge);
    if span_check.is_err() {
        let rule = rule_by_id[CG_MVP3_IMPORTS_MISSING_SOURCE_SPAN];
        agent_use_push_import_finding(
            findings,
            seen_edge_rule,
            rule,
            edge,
            None,
            affected_delta.clone(),
            agent_use_graph_integrity_input(
                lifecycle.clone(),
                edge,
                true,
                false,
                false,
                true,
                span_check.as_ref().err().cloned().unwrap_or_else(|| {
                    "import source span failed graph/source recheck".to_string()
                }),
            ),
            "claimable exact import edge does not have a current valid source span",
            "Regenerate the import edge from source or downgrade the edge to diagnostic evidence.",
        );
        return Ok(());
    }

    if edge.derived && edge.provenance_edges.is_empty() {
        let rule = rule_by_id[CG_MVP3_IMPORTS_DERIVED_MISSING_PROVENANCE];
        agent_use_push_import_finding(
            findings,
            seen_edge_rule,
            rule,
            edge,
            None,
            affected_delta.clone(),
            agent_use_graph_integrity_input(
                lifecycle.clone(),
                edge,
                false,
                true,
                true,
                false,
                "derived exact import edge is missing provenance_edges",
            ),
            "derived exact import edge lacks required provenance",
            "Attach provenance edge IDs or downgrade the derived import fact.",
        );
        return Ok(());
    }

    if !agent_use_exactness_is_proof_grade(edge.exactness) {
        let rule = rule_by_id[CG_MVP3_IMPORTS_DANGLING_TARGET];
        findings.push(agent_use_import_boundary_diagnostic(
            rule,
            lifecycle,
            affected_delta.unwrap_or_else(|| json!(agent_use_validation_edge_json(edge, None))),
            "computed, dynamic, heuristic, inferred, or unresolved import evidence is diagnostic only",
        ));
        return Ok(());
    }

    let target_id = agent_use_import_edge_target_id(edge);
    let target = store
        .get_entity(target_id)
        .map_err(|error| format!("read import target entity failed: {error}"))?;
    let importer_id = agent_use_import_edge_importer_id(edge);
    let source_role = classify_edge_evidence_role(edge).role;
    let importer_role = store
        .get_entity(importer_id)
        .map_err(|error| format!("read importing entity failed: {error}"))?
        .as_ref()
        .map(classify_entity_source_role)
        .map(|decision| decision.role)
        .unwrap_or(source_role);
    let production_source = source_role.is_production() || importer_role.is_production();

    if target.is_none() {
        let default_rule_id = if edge.relation == RelationKind::AliasedBy {
            CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH
        } else {
            CG_MVP3_IMPORTS_DANGLING_TARGET
        };
        let rule = rule_by_id[preferred_missing_target_rule_id.unwrap_or(default_rule_id)];
        agent_use_push_import_finding(
            findings,
            seen_edge_rule,
            rule,
            edge,
            None,
            affected_delta,
            agent_use_graph_source_input(
                lifecycle,
                edge,
                production_source,
                "current exact import edge was reverified from store/source and its target entity is absent",
            ),
            "exact import edge points to a missing target entity",
            "Update the import target, restore the exported declaration, or downgrade non-exact evidence.",
        );
        return Ok(());
    }

    let target = target.expect("checked target presence");
    let target_role = classify_entity_source_role(&target).role;
    if production_source && matches!(target_role, EvidenceRole::Test | EvidenceRole::Mock) {
        let rule = rule_by_id[CG_MVP3_IMPORTS_TARGET_ROLE_MISMATCH];
        agent_use_push_import_finding(
            findings,
            seen_edge_rule,
            rule,
            edge,
            Some(&target),
            affected_delta,
            agent_use_graph_source_input(
                lifecycle,
                edge,
                true,
                "production exact import edge was reverified from store/source and resolves only to non-production target evidence",
            ),
            "production exact import edge resolves to test/mock/stub target evidence",
            "Move the imported target into production evidence, update the import, or mark this relation as test/mock evidence.",
        );
    }

    Ok(())
}

fn agent_use_validate_removed_calls_delta_edge(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    entry: &codegraph_index::EdgeDeltaEntry,
    rule_id: &'static str,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    let rule = rule_by_id[rule_id];
    if !agent_use_exactness_is_proof_grade(entry.exactness) {
        findings.push(agent_use_calls_boundary_diagnostic(
            rule,
            lifecycle,
            json!(entry),
            "removed non-exact CALLS delta is diagnostic only and cannot block",
        ));
        return Ok(());
    }

    let edge = agent_use_calls_edge_from_delta_entry(entry);
    let span_text = match agent_use_reverify_edge_source_span_text(&profile.repo_root, &edge) {
        Ok(span_text) => span_text,
        Err(error) => {
            findings.push(agent_use_calls_boundary_diagnostic(
                rule,
                lifecycle,
                json!({
                    "removed_calls_delta": entry,
                    "source_span_recheck_error": error,
                }),
                "removed exact CALLS delta source span is no longer reverified in current source; without a current source-spanned reference this is diagnostic, not an actionable unknown",
            ));
            return Ok(());
        }
    };

    if !agent_use_removed_calls_delta_span_still_names_target(&span_text, entry) {
        findings.push(agent_use_calls_boundary_diagnostic(
            rule,
            lifecycle,
            json!({
                "removed_calls_delta": entry,
                "source_span_text": span_text,
            }),
            "removed exact CALLS delta source span no longer names the old callee, so it is not blocking proof",
        ));
        return Ok(());
    }

    agent_use_validate_current_calls_edge(
        profile,
        store,
        rule_by_id,
        lifecycle,
        &edge,
        Some(json!(entry)),
        Some(rule_id),
        findings,
        seen_edge_rule,
    )
}

fn agent_use_missing_target_rule_for_current_edge(
    edge: &Edge,
    removed_entity_paths_by_id: &BTreeMap<String, String>,
    renamed_old_paths: &BTreeSet<String>,
) -> &'static str {
    if let Some(path) = removed_entity_paths_by_id.get(&edge.tail_id) {
        if renamed_old_paths.contains(path) {
            return CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED;
        }
        return CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED;
    }
    CG_MVP3_CALLS_DANGLING_TARGET
}

fn agent_use_calls_edge_from_delta_entry(entry: &codegraph_index::EdgeDeltaEntry) -> Edge {
    Edge {
        id: entry.edge_id.clone(),
        head_id: entry.source_entity_id.clone(),
        relation: entry.relation,
        tail_id: entry.target_entity_id.clone(),
        source_span: entry.source_span.clone(),
        repo_commit: None,
        file_hash: None,
        extractor: "agent_use_graph_delta_validation".to_string(),
        confidence: 1.0,
        exactness: entry.exactness,
        edge_class: entry.edge_class.parse().unwrap_or(EdgeClass::Unknown),
        context: entry.edge_context.parse().unwrap_or(EdgeContext::Unknown),
        derived: entry.derived,
        provenance_edges: entry.provenance_edges.clone(),
        metadata: Default::default(),
    }
}

fn agent_use_removed_calls_delta_span_still_names_target(
    span_text: &str,
    entry: &codegraph_index::EdgeDeltaEntry,
) -> bool {
    let mut target_tokens = Vec::new();
    if let Some(name) = entry.target_endpoint.name.as_deref() {
        target_tokens.push(name.to_string());
    }
    if let Some(qualified_name) = entry.target_endpoint.qualified_name.as_deref() {
        target_tokens.extend(
            qualified_name
                .split([':', '.', '/', '\\'])
                .filter(|token| !token.is_empty())
                .map(ToOwned::to_owned),
        );
    }
    target_tokens.extend(
        entry
            .target_entity_id
            .split([':', '.', '/', '\\'])
            .filter(|token| !token.is_empty())
            .map(ToOwned::to_owned),
    );
    target_tokens.sort();
    target_tokens.dedup();
    target_tokens
        .iter()
        .any(|token| token.len() >= 2 && span_text.contains(token))
}

fn agent_use_validate_removed_import_delta_edge(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    rule_by_id: &BTreeMap<&str, &ValidationRule>,
    lifecycle: ValidationLifecycleState,
    entry: &codegraph_index::EdgeDeltaEntry,
    rule_id: &'static str,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> Result<(), String> {
    let rule = rule_by_id[rule_id];
    if !agent_use_exactness_is_proof_grade(entry.exactness) {
        findings.push(agent_use_import_boundary_diagnostic(
            rule,
            lifecycle,
            json!(entry),
            "removed non-exact import delta is diagnostic only and cannot block",
        ));
        return Ok(());
    }

    let edge = agent_use_validation_edge_from_delta_entry(entry);
    let span_text = match agent_use_reverify_edge_source_span_text(&profile.repo_root, &edge) {
        Ok(span_text) => span_text,
        Err(error) => {
            findings.push(agent_use_import_boundary_diagnostic(
                rule,
                lifecycle,
                json!({
                    "removed_import_delta": entry,
                    "source_span_recheck_error": error,
                }),
                "removed exact import delta source span is no longer reverified in current source; without a current source-spanned reference this is diagnostic, not an actionable unknown",
            ));
            return Ok(());
        }
    };

    if !agent_use_removed_import_delta_span_still_names_target(&span_text, entry) {
        findings.push(agent_use_import_boundary_diagnostic(
            rule,
            lifecycle,
            json!({
                "removed_import_delta": entry,
                "source_span_text": span_text,
            }),
            "removed exact import delta source span no longer names the old import target, so it is not blocking proof",
        ));
        return Ok(());
    }

    agent_use_validate_current_import_edge(
        profile,
        store,
        rule_by_id,
        lifecycle,
        &edge,
        Some(json!(entry)),
        Some(rule_id),
        findings,
        seen_edge_rule,
    )
}

fn agent_use_import_validation_relation(relation: RelationKind) -> bool {
    matches!(relation, RelationKind::Imports | RelationKind::AliasedBy)
}

fn agent_use_import_edge_target_id(edge: &Edge) -> &str {
    if edge.relation == RelationKind::AliasedBy {
        &edge.head_id
    } else {
        &edge.tail_id
    }
}

fn agent_use_import_edge_importer_id(edge: &Edge) -> &str {
    if edge.relation == RelationKind::AliasedBy {
        &edge.tail_id
    } else {
        &edge.head_id
    }
}

fn agent_use_import_delta_target_id(entry: &codegraph_index::EdgeDeltaEntry) -> &str {
    if entry.relation == RelationKind::AliasedBy {
        &entry.source_entity_id
    } else {
        &entry.target_entity_id
    }
}

fn agent_use_import_delta_target_path(entry: &codegraph_index::EdgeDeltaEntry) -> String {
    let endpoint = if entry.relation == RelationKind::AliasedBy {
        &entry.source_endpoint
    } else {
        &entry.target_endpoint
    };
    endpoint
        .repo_relative_path
        .as_deref()
        .map(normalize_repo_relative_path)
        .unwrap_or_else(|| normalize_repo_relative_path(&entry.repo_relative_path))
}

fn agent_use_missing_import_target_rule_for_current_edge(
    edge: &Edge,
    removed_entity_paths_by_id: &BTreeMap<String, String>,
    renamed_old_paths: &BTreeSet<String>,
) -> &'static str {
    if edge.relation == RelationKind::AliasedBy {
        return CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH;
    }
    if let Some(path) = removed_entity_paths_by_id.get(agent_use_import_edge_target_id(edge)) {
        if renamed_old_paths.contains(path) {
            return CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED;
        }
        return CG_MVP3_IMPORTS_DELETED_EXPORT_STILL_IMPORTED;
    }
    CG_MVP3_IMPORTS_DANGLING_TARGET
}

fn agent_use_validation_edge_from_delta_entry(entry: &codegraph_index::EdgeDeltaEntry) -> Edge {
    Edge {
        id: entry.edge_id.clone(),
        head_id: entry.source_entity_id.clone(),
        relation: entry.relation,
        tail_id: entry.target_entity_id.clone(),
        source_span: entry.source_span.clone(),
        repo_commit: None,
        file_hash: None,
        extractor: "agent_use_graph_delta_validation".to_string(),
        confidence: 1.0,
        exactness: entry.exactness,
        edge_class: entry.edge_class.parse().unwrap_or(EdgeClass::Unknown),
        context: entry.edge_context.parse().unwrap_or(EdgeContext::Unknown),
        derived: entry.derived,
        provenance_edges: entry.provenance_edges.clone(),
        metadata: Default::default(),
    }
}

fn agent_use_removed_route_delta_span_still_names_target(
    span_text: &str,
    entry: &codegraph_index::EdgeDeltaEntry,
) -> bool {
    let mut target_tokens = Vec::new();
    if let Some(name) = entry.target_endpoint.name.as_deref() {
        target_tokens.push(name.to_string());
    }
    if let Some(qualified_name) = entry.target_endpoint.qualified_name.as_deref() {
        target_tokens.extend(
            qualified_name
                .split([':', '.', '/', '\\'])
                .filter(|token| !token.is_empty())
                .map(ToOwned::to_owned),
        );
    }
    if let Some(path) = entry.target_endpoint.repo_relative_path.as_deref() {
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
        entry
            .target_entity_id
            .split([':', '.', '/', '\\'])
            .filter(|token| !token.is_empty())
            .map(ToOwned::to_owned),
    );
    target_tokens.sort();
    target_tokens.dedup();
    target_tokens
        .iter()
        .any(|token| token.len() >= 2 && span_text.contains(token))
}

fn agent_use_removed_import_delta_span_still_names_target(
    span_text: &str,
    entry: &codegraph_index::EdgeDeltaEntry,
) -> bool {
    let endpoint = if entry.relation == RelationKind::AliasedBy {
        &entry.source_endpoint
    } else {
        &entry.target_endpoint
    };
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
        agent_use_import_delta_target_id(entry)
            .split([':', '.', '/', '\\'])
            .filter(|token| !token.is_empty())
            .map(ToOwned::to_owned),
    );
    target_tokens.sort();
    target_tokens.dedup();
    target_tokens
        .iter()
        .any(|token| token.len() >= 2 && span_text.contains(token))
}

struct AgentUseSourceRoleBoundary {
    rule_id: &'static str,
    source_role: EvidenceRole,
    reason: &'static str,
    recommended_fix: &'static str,
}

fn agent_use_source_role_boundary_for_edge(
    edge: &Edge,
    head: Option<&Entity>,
    tail: Option<&Entity>,
) -> Option<AgentUseSourceRoleBoundary> {
    if edge_generated_or_degraded(edge)
        || head.is_some_and(entity_generated_or_degraded)
        || tail.is_some_and(entity_generated_or_degraded)
    {
        return Some(AgentUseSourceRoleBoundary {
            rule_id: CG_MVP3_SOURCE_ROLE_GENERATED_EVIDENCE_AS_PRODUCTION_PROOF,
            source_role: EvidenceRole::Unknown,
            reason: "production proof edge uses generated or degraded evidence",
            recommended_fix:
                "Exclude generated evidence from production proof or downgrade it to diagnostic evidence.",
        });
    }
    if head.is_some_and(agent_use_entity_is_inline_test)
        || tail.is_some_and(agent_use_entity_is_inline_test)
    {
        return Some(AgentUseSourceRoleBoundary {
            rule_id: CG_MVP3_SOURCE_ROLE_INLINE_TEST_PROMOTED_TO_PRODUCTION,
            source_role: EvidenceRole::Test,
            reason: "inline test module evidence is being treated as production proof",
            recommended_fix:
                "Preserve inline test source-role metadata and exclude inline tests from production proof.",
        });
    }
    if edge.relation == RelationKind::Stubs
        || path_or_symbol_looks_stub(&edge.head_id)
        || path_or_symbol_looks_stub(&edge.tail_id)
        || head.is_some_and(agent_use_entity_is_stub_like)
        || tail.is_some_and(agent_use_entity_is_stub_like)
    {
        return Some(AgentUseSourceRoleBoundary {
            rule_id: CG_MVP3_SOURCE_ROLE_STUB_EVIDENCE_IN_PRODUCTION_PROOF,
            source_role: EvidenceRole::Mock,
            reason: "production proof edge includes stub evidence",
            recommended_fix:
                "Keep stub evidence out of production proof or mark the relation as stub/test evidence.",
        });
    }
    if edge.relation == RelationKind::Mocks
        || path_or_symbol_looks_mock(&edge.head_id)
        || path_or_symbol_looks_mock(&edge.tail_id)
        || head.is_some_and(agent_use_entity_is_mock_like)
        || tail.is_some_and(agent_use_entity_is_mock_like)
    {
        return Some(AgentUseSourceRoleBoundary {
            rule_id: CG_MVP3_SOURCE_ROLE_MOCK_EVIDENCE_IN_PRODUCTION_PROOF,
            source_role: EvidenceRole::Mock,
            reason: "production proof edge includes mock evidence",
            recommended_fix:
                "Keep mock evidence out of production proof or mark the relation as mock/test evidence.",
        });
    }
    if agent_use_relation_is_test_evidence(edge.relation)
        || path_looks_test(&edge.source_span.repo_relative_path)
        || head.is_some_and(agent_use_entity_is_test_like)
        || tail.is_some_and(agent_use_entity_is_test_like)
    {
        return Some(AgentUseSourceRoleBoundary {
            rule_id: CG_MVP3_SOURCE_ROLE_TEST_EVIDENCE_IN_PRODUCTION_PROOF,
            source_role: EvidenceRole::Test,
            reason: "production proof edge includes test evidence",
            recommended_fix:
                "Keep test evidence out of production proof or mark the relation as test-impact evidence.",
        });
    }
    None
}

fn agent_use_edge_treated_as_production_proof(edge: &Edge) -> bool {
    query_evidence_role_for_edge(edge).role == "production"
        || edge.context == EdgeContext::Production
        || metadata_has_any_label(Some(&edge.metadata), &["production"])
}

fn agent_use_relation_is_test_evidence(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Tests
            | RelationKind::Asserts
            | RelationKind::Covers
            | RelationKind::FixturesFor
    )
}

fn agent_use_tests_relation(relation: RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Tests | RelationKind::Asserts | RelationKind::Mocks | RelationKind::Stubs
    )
}

fn agent_use_edge_has_optional_test_target(edge: &Edge) -> bool {
    metadata_has_any_label(
        Some(&edge.metadata),
        &["optional_test_target", "optional target", "optional_target"],
    )
}

fn agent_use_entity_is_inline_test(entity: &Entity) -> bool {
    !path_looks_test(&entity.repo_relative_path)
        && (matches!(
            entity.kind,
            EntityKind::TestSuite | EntityKind::TestCase | EntityKind::Assertion
        ) || agent_use_qualified_name_contains_test_module(&entity.qualified_name))
}

fn agent_use_entity_is_test_like(entity: &Entity) -> bool {
    matches!(
        entity.kind,
        EntityKind::TestFile | EntityKind::TestSuite | EntityKind::TestCase | EntityKind::Assertion
    ) || path_looks_test(&entity.repo_relative_path)
        || agent_use_qualified_name_contains_test_module(&entity.qualified_name)
}

fn agent_use_entity_is_mock_like(entity: &Entity) -> bool {
    matches!(entity.kind, EntityKind::Mock) || path_or_symbol_looks_mock(&entity.qualified_name)
}

fn agent_use_entity_is_stub_like(entity: &Entity) -> bool {
    matches!(entity.kind, EntityKind::Stub) || path_or_symbol_looks_stub(&entity.qualified_name)
}

fn agent_use_qualified_name_contains_test_module(value: &str) -> bool {
    let normalized = value.replace('\\', "/").to_ascii_lowercase();
    normalized == "tests"
        || normalized == "test"
        || normalized.starts_with("tests.")
        || normalized.starts_with("test.")
        || normalized.contains(".tests.")
        || normalized.contains(".test.")
        || normalized.contains("::tests::")
        || normalized.ends_with(".tests")
        || normalized.ends_with("::tests")
}

fn agent_use_push_source_role_finding(
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    head: Option<&Entity>,
    tail: Option<&Entity>,
    affected_delta: Option<Value>,
    source_role: EvidenceRole,
    reason: &str,
    recommended_fix: &str,
) {
    let key = format!("{}:{}", rule.validation_rule_id, edge.id);
    if !seen_edge_rule.insert(key) {
        return;
    }
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/source-role/{}/{}",
            rule.validation_rule_id, edge.id
        ),
        agent_use_graph_source_input(lifecycle, edge, true, reason),
    );
    finding.affected_delta = affected_delta.unwrap_or(Value::Null);
    finding.affected_edge = agent_use_generic_validation_edge_json(edge);
    finding.affected_entity = json!({
        "head": head,
        "tail": tail,
        "head_source_role": head.map(|entity| query_evidence_role_for_entity(entity).role),
        "tail_source_role": tail.map(|entity| query_evidence_role_for_entity(entity).role),
    });
    finding.affected_file = Some(normalize_repo_relative_path(
        &edge.source_span.repo_relative_path,
    ));
    finding.source_span = Some(edge.source_span.clone());
    finding.source_role = Some(source_role);
    finding.relation_kind = Some(edge.relation);
    finding.exactness = Some(edge.exactness);
    finding.provenance = json!({
        "derived": edge.derived,
        "provenance_edges": edge.provenance_edges,
        "provenance_required": edge.derived,
        "provenance_present": !edge.derived || !edge.provenance_edges.is_empty(),
    });
    finding.reason = reason.to_string();
    finding.recommended_fix = Some(recommended_fix.to_string());
    finding.suggested_next_steps = vec![recommended_fix.to_string()];
    finding.expansion_handle = Some(format!("validation_packet:edge:{}", edge.id));
    findings.push(finding);
}

fn agent_use_push_tests_relation_finding(
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
    rule: &ValidationRule,
    edge: &Edge,
    affected_delta: Option<Value>,
    input: ValidationReverificationInput,
    reason: &str,
    recommended_fix: &str,
) {
    let key = format!("{}:{}", rule.validation_rule_id, edge.id);
    if !seen_edge_rule.insert(key) {
        return;
    }
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/tests-relation/{}/{}",
            rule.validation_rule_id, edge.id
        ),
        input,
    );
    finding.affected_delta = affected_delta.unwrap_or(Value::Null);
    finding.affected_edge = agent_use_generic_validation_edge_json(edge);
    finding.affected_entity = json!({
        "entity_id": edge.tail_id,
        "missing": true,
    });
    finding.affected_file = Some(normalize_repo_relative_path(
        &edge.source_span.repo_relative_path,
    ));
    finding.source_span = Some(edge.source_span.clone());
    finding.source_role = Some(classify_edge_evidence_role(edge).role);
    finding.relation_kind = Some(edge.relation);
    finding.exactness = Some(edge.exactness);
    finding.provenance = json!({
        "derived": edge.derived,
        "provenance_edges": edge.provenance_edges,
        "provenance_required": edge.derived,
        "provenance_present": !edge.derived || !edge.provenance_edges.is_empty(),
    });
    finding.reason = reason.to_string();
    finding.recommended_fix = Some(recommended_fix.to_string());
    finding.suggested_next_steps = vec![recommended_fix.to_string()];
    finding.expansion_handle = Some(format!("validation_packet:edge:{}", edge.id));
    findings.push(finding);
}

fn agent_use_tests_relation_unknown(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    affected_delta: Value,
    reason: &str,
) -> ValidationFinding {
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle,
        "evidence://tests-relation-unsupported",
        reason,
    );
    input.relation_supported = false;
    input.relation_exact = false;
    input.graph_source_relation_reverified = false;
    input.unsupported_relation = true;
    input.evidence_items = vec![ValidationEvidenceItem::non_graph(
        ValidationEvidenceKind::Diagnostic,
        "evidence://tests-relation-unsupported",
        reason,
    )];
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/tests-relation/unknown/{:016x}",
            agent_use_validation_stable_u64(&format!("{}:{reason}", edge.id))
        ),
        input,
    );
    finding.affected_delta = affected_delta;
    finding.affected_edge = agent_use_generic_validation_edge_json(edge);
    finding.affected_file = Some(normalize_repo_relative_path(
        &edge.source_span.repo_relative_path,
    ));
    finding.source_span = Some(edge.source_span.clone());
    finding.source_role = Some(classify_edge_evidence_role(edge).role);
    finding.relation_kind = Some(edge.relation);
    finding.exactness = Some(edge.exactness);
    finding.reason = reason.to_string();
    finding
}

fn agent_use_source_role_delta_diagnostic(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    affected_delta: Value,
    reason: &str,
) -> ValidationFinding {
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle,
        "evidence://source-role-delta",
        reason,
    );
    input.relation_exact = false;
    input.graph_source_relation_reverified = false;
    input.evidence_items = vec![ValidationEvidenceItem::non_graph(
        ValidationEvidenceKind::Diagnostic,
        "evidence://source-role-delta",
        reason,
    )];
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/source-role/delta/{:016x}",
            agent_use_validation_stable_u64(reason)
        ),
        input,
    );
    finding.affected_delta = affected_delta;
    finding.reason = reason.to_string();
    finding
}

fn agent_use_push_calls_finding(
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
    rule: &ValidationRule,
    edge: &Edge,
    target: Option<&Entity>,
    affected_delta: Option<Value>,
    input: ValidationReverificationInput,
    reason: &str,
    recommended_fix: &str,
) {
    let key = format!("{}:{}", rule.validation_rule_id, edge.id);
    if !seen_edge_rule.insert(key) {
        return;
    }
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/calls/{}/{}",
            rule.validation_rule_id, edge.id
        ),
        input,
    );
    finding.affected_delta = affected_delta.unwrap_or(Value::Null);
    finding.affected_edge = agent_use_validation_edge_json(edge, target);
    finding.affected_entity = target
        .map(|entity| serde_json::to_value(entity).unwrap_or(Value::Null))
        .unwrap_or_else(|| {
            json!({
                "entity_id": edge.tail_id,
                "missing": true,
            })
        });
    finding.affected_file = Some(normalize_repo_relative_path(
        &edge.source_span.repo_relative_path,
    ));
    finding.source_span = Some(edge.source_span.clone());
    finding.source_role = Some(classify_edge_evidence_role(edge).role);
    finding.relation_kind = Some(edge.relation);
    finding.exactness = Some(edge.exactness);
    finding.provenance = json!({
        "derived": edge.derived,
        "provenance_edges": edge.provenance_edges,
        "provenance_required": edge.derived,
        "provenance_present": !edge.derived || !edge.provenance_edges.is_empty(),
    });
    finding.reason = reason.to_string();
    finding.recommended_fix = Some(recommended_fix.to_string());
    finding.suggested_next_steps = vec![recommended_fix.to_string()];
    finding.expansion_handle = Some(format!("validation_packet:edge:{}", edge.id));
    findings.push(finding);
}

fn agent_use_push_import_finding(
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
    rule: &ValidationRule,
    edge: &Edge,
    target: Option<&Entity>,
    affected_delta: Option<Value>,
    input: ValidationReverificationInput,
    reason: &str,
    recommended_fix: &str,
) {
    let key = format!("{}:{}", rule.validation_rule_id, edge.id);
    if !seen_edge_rule.insert(key) {
        return;
    }
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/imports/{}/{}",
            rule.validation_rule_id, edge.id
        ),
        input,
    );
    finding.affected_delta = affected_delta.unwrap_or(Value::Null);
    finding.affected_edge = agent_use_import_validation_edge_json(edge, target);
    finding.affected_entity = target
        .map(|entity| serde_json::to_value(entity).unwrap_or(Value::Null))
        .unwrap_or_else(|| {
            json!({
                "entity_id": agent_use_import_edge_target_id(edge),
                "missing": true,
            })
        });
    finding.affected_file = Some(normalize_repo_relative_path(
        &edge.source_span.repo_relative_path,
    ));
    finding.source_span = Some(edge.source_span.clone());
    finding.source_role = Some(classify_edge_evidence_role(edge).role);
    finding.relation_kind = Some(edge.relation);
    finding.exactness = Some(edge.exactness);
    finding.provenance = json!({
        "derived": edge.derived,
        "provenance_edges": edge.provenance_edges,
        "provenance_required": edge.derived,
        "provenance_present": !edge.derived || !edge.provenance_edges.is_empty(),
    });
    finding.reason = reason.to_string();
    finding.recommended_fix = Some(recommended_fix.to_string());
    finding.suggested_next_steps = vec![recommended_fix.to_string()];
    finding.expansion_handle = Some(format!("validation_packet:edge:{}", edge.id));
    findings.push(finding);
}

fn agent_use_graph_source_input(
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    source_role_allowed: bool,
    reason: impl Into<String>,
) -> ValidationReverificationInput {
    let reason = reason.into();
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle,
        edge.id.clone(),
        reason.clone(),
    );
    input.source_role_allowed = source_role_allowed;
    input.provenance_required = edge.derived;
    input.provenance_present = !edge.derived || !edge.provenance_edges.is_empty();
    input.reason = reason;
    input
}

fn agent_use_graph_integrity_input(
    lifecycle: ValidationLifecycleState,
    edge: &Edge,
    source_span_required: bool,
    provenance_required: bool,
    source_span_present: bool,
    provenance_present: bool,
    reason: impl Into<String>,
) -> ValidationReverificationInput {
    let reason = reason.into();
    ValidationReverificationInput {
        lifecycle,
        relation_supported: true,
        relation_exact: true,
        graph_source_relation_reverified: false,
        integrity_condition_reverified: true,
        integrity_issue_present: (source_span_required && !source_span_present)
            || (provenance_required && !provenance_present),
        source_span_required,
        source_span_present,
        provenance_required,
        provenance_present,
        source_role_allowed: true,
        over_budget: false,
        unsupported_relation: false,
        evidence_items: vec![ValidationEvidenceItem::graph_integrity(
            edge.id.clone(),
            reason.clone(),
        )],
        reason,
    }
}

fn agent_use_lifecycle_integrity_input(
    lifecycle: ValidationLifecycleState,
    evidence_id: impl Into<String>,
    reason: impl Into<String>,
) -> ValidationReverificationInput {
    let reason = reason.into();
    ValidationReverificationInput {
        lifecycle,
        relation_supported: true,
        relation_exact: true,
        graph_source_relation_reverified: false,
        integrity_condition_reverified: true,
        integrity_issue_present: true,
        source_span_required: false,
        source_span_present: true,
        provenance_required: false,
        provenance_present: true,
        source_role_allowed: true,
        over_budget: false,
        unsupported_relation: false,
        evidence_items: vec![ValidationEvidenceItem::graph_integrity(
            evidence_id,
            reason.clone(),
        )],
        reason,
    }
}

fn agent_use_entity_graph_integrity_input(
    lifecycle: ValidationLifecycleState,
    entity: &Entity,
    source_span_required: bool,
    source_span_present: bool,
    reason: impl Into<String>,
) -> ValidationReverificationInput {
    let mut input = agent_use_lifecycle_integrity_input(lifecycle, entity.id.clone(), reason);
    input.source_span_required = source_span_required;
    input.source_span_present = source_span_present;
    input.integrity_issue_present = source_span_required && !source_span_present;
    input
}

fn agent_use_entity_optional_span_diagnostic_input(
    lifecycle: ValidationLifecycleState,
    entity: &Entity,
    reason: impl Into<String>,
) -> ValidationReverificationInput {
    let reason = reason.into();
    ValidationReverificationInput {
        lifecycle,
        relation_supported: true,
        relation_exact: false,
        graph_source_relation_reverified: false,
        integrity_condition_reverified: true,
        integrity_issue_present: false,
        source_span_required: false,
        source_span_present: false,
        provenance_required: false,
        provenance_present: true,
        source_role_allowed: true,
        over_budget: false,
        unsupported_relation: false,
        evidence_items: vec![ValidationEvidenceItem::non_graph(
            ValidationEvidenceKind::Diagnostic,
            entity.id.clone(),
            reason.clone(),
        )],
        reason,
    }
}

fn agent_use_calls_boundary_diagnostic(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    affected_delta: Value,
    reason: &str,
) -> ValidationFinding {
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle,
        "evidence://calls-diagnostic",
        reason,
    );
    input.relation_exact = false;
    input.graph_source_relation_reverified = false;
    input.evidence_items = vec![ValidationEvidenceItem::non_graph(
        ValidationEvidenceKind::Diagnostic,
        "evidence://calls-diagnostic",
        reason,
    )];
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/calls/diagnostic/{:016x}",
            agent_use_validation_stable_u64(reason)
        ),
        input,
    );
    finding.affected_delta = affected_delta;
    finding.reason = reason.to_string();
    finding
}

fn agent_use_calls_boundary_unknown(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    affected_delta: Value,
    reason: &str,
) -> ValidationFinding {
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle,
        "evidence://calls-unknown",
        reason,
    );
    input.relation_exact = false;
    input.graph_source_relation_reverified = false;
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/calls/unknown/{:016x}",
            agent_use_validation_stable_u64(reason)
        ),
        input,
    );
    finding.affected_delta = affected_delta;
    finding.reason = reason.to_string();
    finding
}

fn agent_use_graph_delta_bounded_unknown(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    affected_delta: Value,
    reason: &str,
) -> ValidationFinding {
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle,
        "evidence://graph-delta-bounded",
        reason,
    );
    input.relation_exact = false;
    input.graph_source_relation_reverified = false;
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_9_5/graph-delta/bounded/{:016x}",
            agent_use_validation_stable_u64(reason)
        ),
        input,
    );
    finding.affected_delta = affected_delta;
    finding.reason = reason.to_string();
    finding
}

fn agent_use_import_boundary_diagnostic(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    affected_delta: Value,
    reason: &str,
) -> ValidationFinding {
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle,
        "evidence://imports-diagnostic",
        reason,
    );
    input.relation_exact = false;
    input.graph_source_relation_reverified = false;
    input.evidence_items = vec![ValidationEvidenceItem::non_graph(
        ValidationEvidenceKind::Diagnostic,
        "evidence://imports-diagnostic",
        reason,
    )];
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/imports/diagnostic/{:016x}",
            agent_use_validation_stable_u64(reason)
        ),
        input,
    );
    finding.affected_delta = affected_delta;
    finding.reason = reason.to_string();
    finding
}

fn agent_use_import_boundary_unknown(
    rule: &ValidationRule,
    lifecycle: ValidationLifecycleState,
    affected_delta: Value,
    reason: &str,
) -> ValidationFinding {
    let mut input = ValidationReverificationInput::exact_graph_source(
        lifecycle,
        "evidence://imports-unknown",
        reason,
    );
    input.relation_exact = false;
    input.graph_source_relation_reverified = false;
    let mut finding = classify_validation_finding(
        rule,
        format!(
            "finding://mvp3_3/imports/unknown/{:016x}",
            agent_use_validation_stable_u64(reason)
        ),
        input,
    );
    finding.affected_delta = affected_delta;
    finding.reason = reason.to_string();
    finding
}

fn agent_use_validation_edge_json(edge: &Edge, target: Option<&Entity>) -> Value {
    json!({
        "edge_id": edge.id,
        "relation_kind": edge.relation,
        "caller": edge.head_id,
        "callee": edge.tail_id,
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

fn agent_use_import_validation_edge_json(edge: &Edge, target: Option<&Entity>) -> Value {
    json!({
        "edge_id": edge.id,
        "relation_kind": edge.relation,
        "importing_entity": agent_use_import_edge_importer_id(edge),
        "import_target": agent_use_import_edge_target_id(edge),
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

fn agent_use_generic_validation_edge_json(edge: &Edge) -> Value {
    json!({
        "edge_id": edge.id,
        "relation_kind": edge.relation,
        "source_entity_id": edge.head_id,
        "target_entity_id": edge.tail_id,
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

fn agent_use_validation_lifecycle_from_read_preflight(
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
            .any(|blocker| lifecycle_blocker_is_stale_or_missing_passport(blocker)),
        foreign: kind == "repo_root_mismatch"
            || preflight
                .blockers
                .iter()
                .any(|blocker| lifecycle_blocker_is_foreign_repo(blocker)),
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

fn agent_use_exactness_is_proof_grade(exactness: Exactness) -> bool {
    matches!(
        exactness,
        Exactness::Exact
            | Exactness::CompilerVerified
            | Exactness::LspVerified
            | Exactness::ParserVerified
    )
}

fn agent_use_edge_can_be_claimable_graph_proof(edge: &Edge) -> bool {
    agent_use_exactness_is_proof_grade(edge.exactness)
        || edge.exactness == Exactness::DerivedFromVerifiedEdges
        || edge.derived
}

fn agent_use_edge_delta_claimable_graph_proof(edge: &codegraph_index::EdgeDeltaEntry) -> bool {
    edge.claimability.claimable
        && edge.claimability.graph_proof
        && (agent_use_exactness_is_proof_grade(edge.exactness)
            || edge.exactness == Exactness::DerivedFromVerifiedEdges
            || edge.derived)
}

fn agent_use_entity_can_be_claimable_graph_proof(entity: &Entity) -> bool {
    !matches!(entity.kind, EntityKind::PathEvidence)
}

fn agent_use_entity_kind_requires_source_span(kind: EntityKind) -> bool {
    !matches!(
        kind,
        EntityKind::Repository
            | EntityKind::Package
            | EntityKind::Directory
            | EntityKind::File
            | EntityKind::Module
            | EntityKind::Database
            | EntityKind::Table
            | EntityKind::Column
            | EntityKind::Dependency
            | EntityKind::PathEvidence
            | EntityKind::DerivedClosureEdge
    )
}

fn agent_use_update_state_mentions_corrupt_or_incomplete(
    delta: &EntitySourceRoleDeltaReport,
    preflight: &DbLifecyclePreflight,
) -> bool {
    let mut labels = vec![
        delta.status.as_str(),
        delta.old_snapshot_status.as_str(),
        delta.new_snapshot_status.as_str(),
        preflight.db_health.passport_status.as_str(),
    ];
    labels.extend(preflight.db_health.reasons.iter().map(String::as_str));
    labels.iter().any(|label| {
        let lower = label.to_ascii_lowercase();
        lower.contains("corrupt")
            || lower.contains("incomplete")
            || lower.contains("partial")
            || lower.contains("interrupted")
    })
}

fn agent_use_db_path_looks_like_publish_temp(db_path: &Path) -> bool {
    db_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            let lower = name.to_ascii_lowercase();
            lower.ends_with(".tmp")
                || lower.contains(".tmp.")
                || lower.contains("publish-temp")
                || lower.contains("partial")
        })
        .unwrap_or(false)
}

fn exact_calls_target_entity_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Function
            | EntityKind::Method
            | EntityKind::Constructor
            | EntityKind::TestCase
            | EntityKind::Mock
            | EntityKind::Stub
    )
}

fn exact_import_target_entity_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::File
            | EntityKind::Module
            | EntityKind::Function
            | EntityKind::Method
            | EntityKind::Class
            | EntityKind::Interface
            | EntityKind::Trait
            | EntityKind::Enum
            | EntityKind::Type
            | EntityKind::GenericType
            | EntityKind::LocalVariable
            | EntityKind::GlobalVariable
            | EntityKind::Export
            | EntityKind::Import
            | EntityKind::Table
            | EntityKind::TestCase
            | EntityKind::Mock
            | EntityKind::Stub
    )
}

fn agent_use_entity_delta_id(entry: &codegraph_index::EntityDeltaEntry) -> Option<String> {
    entry
        .new
        .as_ref()
        .or(entry.old.as_ref())
        .map(|summary| summary.entity_id.clone())
}

// MVP3.9.5.3: span reverification runs once per validated edge; without a
// cache that is one whole-file read PLUS one whole-file line split per edge.
// Contents and a precomputed line index are cached per thread keyed by
// (mtime, len) so an edited file is re-read, never served stale.
const AGENT_USE_REVERIFY_SOURCE_CACHE_MAX_FILES: usize = 64;

struct CachedReverifySource {
    content: String,
    // Byte range of each line, exclusive of the line terminator (and of a
    // trailing `\r`), matching `str::lines()` segmentation exactly.
    line_spans: Vec<(usize, usize)>,
}

impl CachedReverifySource {
    fn from_content(content: String) -> Self {
        let bytes = content.as_bytes();
        let mut line_spans = Vec::new();
        let mut line_start = 0usize;
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'\n' {
                let mut line_end = index;
                if line_end > line_start && bytes[line_end - 1] == b'\r' {
                    line_end -= 1;
                }
                line_spans.push((line_start, line_end));
                line_start = index + 1;
            }
        }
        if line_start < bytes.len() {
            line_spans.push((line_start, bytes.len()));
        }
        Self {
            content,
            line_spans,
        }
    }

    fn line(&self, index: usize) -> &str {
        let (start, end) = self.line_spans[index];
        &self.content[start..end]
    }
}

thread_local! {
    static AGENT_USE_REVERIFY_SOURCE_CACHE: std::cell::RefCell<
        BTreeMap<PathBuf, (Option<std::time::SystemTime>, u64, std::rc::Rc<CachedReverifySource>)>,
    > = std::cell::RefCell::new(BTreeMap::new());
}

fn agent_use_read_reverify_source_cached(
    source_path: &Path,
) -> Result<std::rc::Rc<CachedReverifySource>, String> {
    let metadata = std::fs::metadata(source_path)
        .map_err(|error| format!("source span file could not be read: {error}"))?;
    let modified = metadata.modified().ok();
    let len = metadata.len();
    AGENT_USE_REVERIFY_SOURCE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((cached_modified, cached_len, content)) = cache.get(source_path) {
            if *cached_modified == modified && cached_modified.is_some() && *cached_len == len {
                return Ok(content.clone());
            }
        }
        let content = std::rc::Rc::new(CachedReverifySource::from_content(
            std::fs::read_to_string(source_path)
                .map_err(|error| format!("source span file could not be read: {error}"))?,
        ));
        if cache.len() >= AGENT_USE_REVERIFY_SOURCE_CACHE_MAX_FILES {
            cache.clear();
        }
        cache.insert(source_path.to_path_buf(), (modified, len, content.clone()));
        Ok(content)
    })
}

fn agent_use_reverify_edge_source_span(repo_root: &Path, edge: &Edge) -> Result<(), String> {
    agent_use_reverify_edge_source_span_text(repo_root, edge).map(|_| ())
}

fn agent_use_reverify_edge_source_span_text(
    repo_root: &Path,
    edge: &Edge,
) -> Result<String, String> {
    agent_use_reverify_source_span_text(repo_root, &edge.source_span, "claimable graph edge proof")
}

fn agent_use_reverify_source_span_text(
    repo_root: &Path,
    span: &SourceSpan,
    column_requirement_reason: &str,
) -> Result<String, String> {
    let repo_relative_path = normalize_repo_relative_path(&span.repo_relative_path);
    if repo_relative_path.trim().is_empty()
        || repo_relative_path.contains(':')
        || Path::new(&repo_relative_path)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("source span path is not a safe repo-relative path".to_string());
    }
    if span.start_line == 0 || span.end_line == 0 || span.end_line < span.start_line {
        return Err("source span line coordinates are missing or invalid".to_string());
    }
    let (Some(start_column), Some(mut end_column)) = (span.start_column, span.end_column) else {
        return Err(format!(
            "source span columns are required for {column_requirement_reason}"
        ));
    };
    if start_column == 0 || end_column == 0 {
        return Err("source span columns must be one-based".to_string());
    }
    let source_path =
        repo_root.join(repo_relative_path.replace('/', std::path::MAIN_SEPARATOR_STR));
    let source = agent_use_read_reverify_source_cached(&source_path)?;
    let line_count = source.line_spans.len();
    let start = span.start_line.saturating_sub(1) as usize;
    let mut end = span.end_line.saturating_sub(1) as usize;
    if end == line_count && span.end_column == Some(1) && start < line_count {
        end = line_count.saturating_sub(1);
        if start == end {
            end_column = (source.line(end).len() as u32).saturating_add(1);
        }
    }
    if start >= line_count || end >= line_count {
        return Err("source span line is outside the current source file".to_string());
    }
    let snippet = if start == end {
        let line = source.line(start);
        let start_index = start_column.saturating_sub(1) as usize;
        let end_index = end_column.saturating_sub(1) as usize;
        if start_index >= line.len() || end_index > line.len() || end_index <= start_index {
            return Err("source span columns are outside the current source line".to_string());
        }
        line.get(start_index..end_index)
            .ok_or_else(|| "source span columns split a UTF-8 codepoint".to_string())?
            .to_string()
    } else {
        (start..=end)
            .map(|index| source.line(index))
            .collect::<Vec<_>>()
            .join("\n")
    };
    if snippet.trim().is_empty() {
        return Err("source span resolves to empty source text".to_string());
    }
    Ok(snippet)
}

fn agent_use_validation_stable_u64(input: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod exact_calls_validation_tests {
    use super::*;
    use codegraph_core::{NormalizedClaimabilityMetadata, ValidationClassification};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_repo() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let repo = std::env::temp_dir().join(format!(
            "codegraph-exact-calls-validation-{}-{nanos}-{counter}",
            std::process::id()
        ));
        std::fs::create_dir_all(repo.join("src")).expect("create test repo");
        repo
    }

    fn write_source(repo: &Path, path: &str, source: &str) {
        let full = repo.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("create source parent");
        }
        std::fs::write(full, source).expect("write source");
    }

    fn test_store(repo: &Path) -> SqliteGraphStore {
        SqliteGraphStore::open(&repo.join("validation.sqlite")).expect("open validation DB")
    }

    fn cleanup_repo(repo: PathBuf) {
        for attempt in 0..20 {
            match std::fs::remove_dir_all(&repo) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(
                        25 * (attempt + 1).min(10),
                    ));
                }
            }
        }
        std::fs::remove_dir_all(&repo).expect("cleanup");
    }

    fn rules() -> Vec<ValidationRule> {
        let mut rules = agent_use_exact_calls_validation_rules();
        rules.extend(agent_use_exact_imports_validation_rules());
        rules.extend(agent_use_proof_integrity_validation_rules());
        rules.extend(agent_use_source_role_tests_validation_rules());
        rules.extend(agent_use_activation_gated_contract_validation_rules());
        rules
    }

    fn rule_map(rules: &[ValidationRule]) -> BTreeMap<&str, &ValidationRule> {
        rules
            .iter()
            .map(|rule| (rule.validation_rule_id.as_str(), rule))
            .collect()
    }

    fn function_entity(path: &str, name: &str, line: u32) -> Entity {
        let mut metadata = Metadata::default();
        metadata.insert("source_role".to_string(), json!("production"));
        metadata.insert("source_role_reason".to_string(), json!("test fixture"));
        Entity {
            id: format!("entity://{path}/{name}"),
            kind: EntityKind::Function,
            name: name.to_string(),
            qualified_name: name.to_string(),
            repo_relative_path: path.to_string(),
            source_span: Some(SourceSpan::with_columns(path, line, 1, line, 12)),
            content_hash: None,
            file_hash: None,
            created_from: "exact-calls-validation-test".to_string(),
            confidence: 1.0,
            metadata,
        }
    }

    fn test_entity(path: &str, name: &str, line: u32) -> Entity {
        let mut entity = function_entity(path, name, line);
        entity.kind = EntityKind::TestCase;
        entity.metadata = Metadata::default();
        entity
            .metadata
            .insert("source_role".to_string(), json!("test"));
        entity
            .metadata
            .insert("source_role_reason".to_string(), json!("test fixture"));
        entity
    }

    fn mock_entity(path: &str, name: &str, line: u32) -> Entity {
        let mut entity = function_entity(path, name, line);
        entity.kind = EntityKind::Mock;
        entity.metadata = Metadata::default();
        entity
            .metadata
            .insert("source_role".to_string(), json!("mock"));
        entity
    }

    fn stub_entity(path: &str, name: &str, line: u32) -> Entity {
        let mut entity = function_entity(path, name, line);
        entity.kind = EntityKind::Stub;
        entity.metadata = Metadata::default();
        entity
            .metadata
            .insert("source_role".to_string(), json!("stub"));
        entity
    }

    fn inline_test_entity(path: &str, name: &str, line: u32) -> Entity {
        let mut entity = test_entity(path, name, line);
        entity.qualified_name = format!("crate::tests::{name}");
        entity
            .metadata
            .insert("source_role".to_string(), json!("production"));
        entity.metadata.insert(
            "source_role_reason".to_string(),
            json!("bad fixture metadata"),
        );
        entity
    }

    fn calls_edge(head: &Entity, tail_id: &str, span: SourceSpan) -> Edge {
        Edge {
            id: stable_edge_id(&head.id, RelationKind::Calls, tail_id, &span),
            head_id: head.id.clone(),
            relation: RelationKind::Calls,
            tail_id: tail_id.to_string(),
            source_span: span,
            repo_commit: None,
            file_hash: None,
            extractor: "exact-calls-validation-test".to_string(),
            confidence: 1.0,
            exactness: Exactness::ParserVerified,
            edge_class: EdgeClass::BaseExact,
            context: EdgeContext::Production,
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Default::default(),
        }
    }

    fn relation_edge(
        head: &Entity,
        relation: RelationKind,
        tail_id: &str,
        span: SourceSpan,
        exactness: Exactness,
        role: EvidenceRole,
    ) -> Edge {
        let mut metadata = Metadata::default();
        metadata.insert("source_role".to_string(), json!(role.as_str()));
        let context = match role {
            EvidenceRole::Production => EdgeContext::Production,
            EvidenceRole::Test => EdgeContext::Test,
            EvidenceRole::Mock => EdgeContext::Mock,
            EvidenceRole::Mixed => EdgeContext::Mixed,
            EvidenceRole::Unknown => EdgeContext::Unknown,
        };
        Edge {
            id: stable_edge_id(&head.id, relation, tail_id, &span),
            head_id: head.id.clone(),
            relation,
            tail_id: tail_id.to_string(),
            source_span: span,
            repo_commit: None,
            file_hash: None,
            extractor: "source-role-tests-validation-test".to_string(),
            confidence: 1.0,
            exactness,
            edge_class: if matches!(relation, RelationKind::Mocks | RelationKind::Stubs) {
                EdgeClass::Mock
            } else if matches!(relation, RelationKind::Tests | RelationKind::Asserts) {
                EdgeClass::Test
            } else {
                EdgeClass::BaseExact
            },
            context,
            derived: false,
            provenance_edges: Vec::new(),
            metadata,
        }
    }

    fn file_entity(path: &str) -> Entity {
        let mut metadata = Metadata::default();
        metadata.insert("source_role".to_string(), json!("production"));
        Entity {
            id: format!("entity://{path}"),
            kind: EntityKind::File,
            name: path.to_string(),
            qualified_name: path.to_string(),
            repo_relative_path: path.to_string(),
            source_span: Some(SourceSpan::with_columns(path, 1, 1, 1, 1)),
            content_hash: None,
            file_hash: None,
            created_from: "exact-imports-validation-test".to_string(),
            confidence: 1.0,
            metadata,
        }
    }

    fn import_entity(path: &str, name: &str, line: u32) -> Entity {
        let mut entity = function_entity(path, name, line);
        entity.kind = EntityKind::Import;
        entity.qualified_name = format!("{path}.import:{name}");
        entity.id = format!("entity://{path}/import/{name}");
        entity
    }

    fn route_entity(path: &str, name: &str, line: u32) -> Entity {
        let mut entity = function_entity(path, name, line);
        entity.kind = EntityKind::Route;
        entity.qualified_name = format!("{path}.route:{name}");
        entity.id = format!("entity://{path}/route/{name}");
        entity
    }

    fn imports_edge(head: &Entity, tail_id: &str, span: SourceSpan) -> Edge {
        Edge {
            id: stable_edge_id(&head.id, RelationKind::Imports, tail_id, &span),
            head_id: head.id.clone(),
            relation: RelationKind::Imports,
            tail_id: tail_id.to_string(),
            source_span: span,
            repo_commit: None,
            file_hash: None,
            extractor: "exact-imports-validation-test".to_string(),
            confidence: 1.0,
            exactness: Exactness::ParserVerified,
            edge_class: EdgeClass::BaseExact,
            context: EdgeContext::Production,
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Default::default(),
        }
    }

    fn alias_edge(target: &Entity, import_alias: &Entity, span: SourceSpan) -> Edge {
        Edge {
            id: stable_edge_id(&target.id, RelationKind::AliasedBy, &import_alias.id, &span),
            head_id: target.id.clone(),
            relation: RelationKind::AliasedBy,
            tail_id: import_alias.id.clone(),
            source_span: span,
            repo_commit: None,
            file_hash: None,
            extractor: "exact-imports-validation-test".to_string(),
            confidence: 1.0,
            exactness: Exactness::ParserVerified,
            edge_class: EdgeClass::BaseExact,
            context: EdgeContext::Production,
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Default::default(),
        }
    }

    fn test_profile(repo: &Path) -> AgentUseProfile {
        AgentUseProfile {
            profile_name: "test".to_string(),
            repo_root: repo.to_path_buf(),
            repo_identity_label: "test".to_string(),
            repo_identity_hash: "test".to_string(),
            profile_root: repo.join("profile"),
            db_path: repo.join("validation.sqlite"),
            candidate_spool_path: repo.join("candidate.jsonl"),
            candidate_spool_query_index_path: repo.join("candidate.sqlite"),
            vector_runtime_path: repo.join("vector-runtime.json"),
            vector_audit_path: repo.join("vector-audit.json"),
            lock_or_publish_state_path: repo.join("publish-state.json"),
            delta_state_path: repo.join("delta-state.json"),
            lifecycle_expectations: Vec::new(),
            recovery_commands: Vec::new(),
            mcp_args: Vec::new(),
            binary_profile: "test".to_string(),
            scope_policy: IndexScopeOptions::default(),
        }
    }

    fn validate_edit_test_options(
        repo: &Path,
        detail_mode: AgentUseDetailMode,
        fail_on_blocking: bool,
    ) -> AgentUseValidateEditOptions {
        AgentUseValidateEditOptions {
            repo: repo.to_path_buf(),
            changed_paths: vec![PathBuf::from("src/a.ts")],
            detail_mode,
            max_output_bytes: None,
            fail_on_blocking,
            task_id: Some("task-1".to_string()),
            edit_intent: Some("validate interrupt propagation".to_string()),
            expected_touched_files: vec![PathBuf::from("src/a.ts")],
            max_validation_ms: None,
        }
    }

    fn validate_edit_block_error() -> Value {
        json!({
            "validation_rule_id": CG_MVP3_CALLS_DANGLING_TARGET,
            "classification": "block",
            "blocking_level": "blocking",
            "relation_kind": "CALLS",
            "reverified_graph_source_proof": true,
            "source_span": {
                "repo_relative_path": "src/a.ts",
                "start_line": 2,
                "start_col": 3,
                "end_line": 2,
                "end_col": 24
            },
            "recommended_fix": "Define the missing callee or update the exact call relation.",
            "suggested_next_steps": [
                "Fix the dangling CALLS edge.",
                "Rerun validation with codegraph-mcp agent-use validate-edit --repo <repo> --changed src/a.ts --agent-json."
            ]
        })
    }

    fn validate_edit_blocking_source_update() -> Value {
        let block_error = validate_edit_block_error();
        json!({
            "status": "updated",
            "agent_use_command": "agent-use watch",
            "changed_paths": ["src/a.ts"],
            "normalized_changed_files": ["src/a.ts"],
            "rejected_paths": [],
            "no_op_paths": [],
            "validation_packet": {
                "schema_version": 1,
                "packet_kind": "graph_validation_packet",
                "status": "blocking_graph_error",
                "must_fix_before_continuing": true,
                "changed_files": ["src/a.ts"],
                "graph_delta": {},
                "blocking_errors": [block_error.clone()],
                "warnings": [],
                "unknowns": [],
                "diagnostics": [],
                "summary_counts_by_rule_id": {"CG_MVP3_CALLS_DANGLING_TARGET": 1},
                "summary_counts_by_classification": {"block": 1},
                "summary_counts_by_relation_kind": {"CALLS": 1},
                "validation_rules_evaluated": [CG_MVP3_CALLS_DANGLING_TARGET],
                "validation_rules_skipped": [],
                "relation_family_status": {"calls": "exact_dangling_target_checked"},
                "activation_gate_state": {"mvp3_validate_edit": "enabled"},
                "claimability": {"claimable": true},
                "proof_ladder_changes": {},
                "lifecycle": {"claimable": true},
                "stale_unsafe_blockers": [],
                "top_blocking_source_spans": [block_error["source_span"].clone()],
                "recommended_next_steps": [
                    "Rerun validation with codegraph-mcp agent-use validate-edit --repo <repo> --changed src/a.ts --agent-json."
                ],
                "hard_interrupt_available": true,
                "hard_interrupt": {
                    "packet_kind": "hard_interrupt",
                    "status": "blocking_graph_error",
                    "must_fix_before_continuing": true,
                    "hard_interrupt_available": true,
                    "error_count": 1,
                    "errors": [block_error],
                    "summary": {
                        "top_error_source_span": {
                            "repo_relative_path": "src/a.ts",
                            "start_line": 2,
                            "start_col": 3,
                            "end_line": 2,
                            "end_col": 24
                        },
                        "top_error_recommended_fix": "Define the missing callee or update the exact call relation."
                    },
                    "omitted_count": 0,
                    "expansion_handles": []
                },
                "omitted_count": 0,
                "expansion_handles": []
            },
            "hard_interrupt_available": true,
            "hard_interrupt": {
                "packet_kind": "hard_interrupt",
                "status": "blocking_graph_error",
                "must_fix_before_continuing": true,
                "hard_interrupt_available": true,
                "error_count": 1,
                "errors": [validate_edit_block_error()],
                "omitted_count": 0,
                "expansion_handles": []
            },
            "claimability": {"claimable": true},
            "lifecycle": {"claimable": true},
            "normal_dot_codegraph_mutated": false,
            "timings": {"total_update_plus_delta_ms": 1}
        })
    }

    fn validate_edit_non_interrupt_source_update() -> Value {
        json!({
            "status": "updated",
            "agent_use_command": "agent-use watch",
            "changed_paths": ["src/a.ts"],
            "normalized_changed_files": ["src/a.ts"],
            "validation_packet": {
                "schema_version": 1,
                "packet_kind": "graph_validation_packet",
                "status": "warning",
                "must_fix_before_continuing": false,
                "changed_files": ["src/a.ts"],
                "graph_delta": {},
                "blocking_errors": [],
                "warnings": [
                    {
                        "validation_rule_id": "CG_MVP3_DYNAMIC_CALL_UNKNOWN",
                        "classification": "warning",
                        "evidence_kind": "dynamic_call"
                    },
                    {
                        "validation_rule_id": "CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING",
                        "classification": "warning",
                        "evidence_kind": "text_evidence",
                        "graph_proof": false
                    }
                ],
                "unknowns": [
                    {
                        "validation_rule_id": "CG_MVP3_COMPUTED_IMPORT_UNKNOWN",
                        "classification": "unknown",
                        "evidence_kind": "computed_import"
                    }
                ],
                "diagnostics": [
                    {
                        "validation_rule_id": "CG_MVP3_STALE_SIDECAR_DIAGNOSTIC",
                        "classification": "diagnostic_only",
                        "evidence_kind": "stale_sidecar"
                    }
                ],
                "summary_counts_by_rule_id": {
                    "CG_MVP3_DYNAMIC_CALL_UNKNOWN": 1,
                    "CG_MVP3_COMPUTED_IMPORT_UNKNOWN": 1,
                    "CG_MVP3_STALE_SIDECAR_DIAGNOSTIC": 1
                },
                "summary_counts_by_classification": {
                    "warning": 2,
                    "unknown": 1,
                    "diagnostic_only": 1
                },
                "summary_counts_by_relation_kind": {},
                "validation_rules_evaluated": [],
                "validation_rules_skipped": [],
                "relation_family_status": {
                    "dynamic_calls": "warning_only",
                    "computed_imports": "unknown_only",
                    "candidate_vector_source_navigation": "not_graph_proof"
                },
                "activation_gate_state": {},
                "claimability": {"claimable": true},
                "proof_ladder_changes": {
                    "text_evidence": {"graph_proof": false},
                    "candidate_evidence": {"graph_proof": false},
                    "vector_evidence": {"graph_proof": false},
                    "source_navigation": {"graph_proof": false}
                },
                "lifecycle": {"claimable": true},
                "stale_unsafe_blockers": [],
                "top_blocking_source_spans": [],
                "recommended_next_steps": [],
                "hard_interrupt_available": false,
                "hard_interrupt": null,
                "omitted_count": 0,
                "expansion_handles": []
            },
            "hard_interrupt_available": false,
            "hard_interrupt": null,
            "claimability": {"claimable": true},
            "lifecycle": {"claimable": true},
            "normal_dot_codegraph_mutated": false
        })
    }

    #[test]
    fn validate_edit_args_parse_contract_flags() {
        let args = vec![
            "--repo".to_string(),
            "C:/repo".to_string(),
            "--changed".to_string(),
            "src/a.ts".to_string(),
            "--changed".to_string(),
            "src/b.ts".to_string(),
            "--agent-json".to_string(),
            "--fail-on-blocking".to_string(),
            "--task-id".to_string(),
            "task-1".to_string(),
            "--edit-intent=rename callee".to_string(),
            "--expected-touched-file".to_string(),
            "src/a.ts".to_string(),
            "--max-output-bytes=8192".to_string(),
        ];
        let options = parse_agent_use_validate_edit_args(&args).expect("parse validate-edit");
        assert_eq!(options.repo, PathBuf::from("C:/repo"));
        assert_eq!(
            options.changed_paths,
            vec![PathBuf::from("src/a.ts"), PathBuf::from("src/b.ts")]
        );
        assert_eq!(options.detail_mode, AgentUseDetailMode::Compact);
        assert_eq!(options.max_output_bytes, Some(8192));
        assert!(options.fail_on_blocking);
        assert_eq!(options.task_id.as_deref(), Some("task-1"));
        assert_eq!(options.edit_intent.as_deref(), Some("rename callee"));
        assert_eq!(
            options.expected_touched_files,
            vec![PathBuf::from("src/a.ts")]
        );
    }

    #[test]
    fn validate_edit_rejects_direct_db_and_mode() {
        let db_error = parse_agent_use_validate_edit_args(&[
            "--repo".to_string(),
            ".".to_string(),
            "--changed".to_string(),
            "src/a.ts".to_string(),
            "--db".to_string(),
            "graph.db".to_string(),
        ])
        .expect_err("db is rejected");
        assert!(db_error.contains("production profile"));

        let mode_error = parse_agent_use_validate_edit_args(&[
            "--repo".to_string(),
            ".".to_string(),
            "--changed".to_string(),
            "src/a.ts".to_string(),
            "--mode".to_string(),
            "validate".to_string(),
        ])
        .expect_err("mode is rejected");
        assert!(mode_error.contains("does not accept --mode"));
    }

    #[test]
    fn validate_edit_alias_returns_targeted_deferred_error() {
        let error = run_validate_edit_alias_deferred_command(&[]).expect_err("alias deferred");
        let value: Value = serde_json::from_str(&error).expect("alias json error");
        assert_eq!(value["status"].as_str(), Some("error"));
        assert_eq!(
            value["compatibility_alias_status"].as_str(),
            Some("deferred")
        );
        assert_eq!(
            value["canonical_command"].as_str(),
            Some("agent-use validate-edit")
        );
    }

    fn validate_edit_exit_packet(status: &str, must_fix_before_continuing: bool) -> Value {
        json!({
            "status": status,
            "final_status": status,
            "must_fix_before_continuing": must_fix_before_continuing,
            "hard_interrupt_available": status == "blocking_graph_error",
            "normal_dot_codegraph_mutated": false,
        })
    }

    fn validate_edit_policy_exit_code(status: &str, must_fix_before_continuing: bool) -> i64 {
        let mut packet = validate_edit_exit_packet(status, must_fix_before_continuing);
        agent_use_validate_edit_apply_exit_policy(&mut packet, true);
        packet
            .get("_cli_exit_code")
            .and_then(Value::as_i64)
            .unwrap_or(0)
    }

    #[test]
    fn cli_exit_codes_match_severity_policy() {
        assert_eq!(
            validate_edit_policy_exit_code("blocking_graph_error", true),
            2
        );
        for status in ["warning", "unknown", "diagnostic_only", "ok"] {
            assert_eq!(
                validate_edit_policy_exit_code(status, false),
                0,
                "{status} must remain exit 0"
            );
        }
    }

    #[test]
    fn default_blocking_exit_zero_preserved() {
        let mut packet = validate_edit_exit_packet("blocking_graph_error", true);
        agent_use_validate_edit_apply_exit_policy(&mut packet, false);
        assert!(packet.get("_cli_exit_code").is_none(), "{packet}");
    }

    #[test]
    fn fail_on_blocking_exit_two_preserved() {
        let mut packet = validate_edit_exit_packet("blocking_graph_error", true);
        agent_use_validate_edit_apply_exit_policy(&mut packet, true);
        assert_eq!(packet["_cli_exit_code"].as_i64(), Some(2));
    }

    #[test]
    fn warning_exit_zero() {
        assert_eq!(validate_edit_policy_exit_code("warning", false), 0);
    }

    #[test]
    fn unknown_exit_zero() {
        assert_eq!(validate_edit_policy_exit_code("unknown", false), 0);
    }

    #[test]
    fn diagnostic_only_exit_zero() {
        assert_eq!(validate_edit_policy_exit_code("diagnostic_only", false), 0);
    }

    #[test]
    fn ok_exit_zero() {
        assert_eq!(validate_edit_policy_exit_code("ok", false), 0);
    }

    #[test]
    fn no_dot_codegraph_mutation_in_cli_severity_semantics() {
        let mut packet = validate_edit_exit_packet("blocking_graph_error", true);
        agent_use_validate_edit_apply_exit_policy(&mut packet, true);
        assert_eq!(
            packet["normal_dot_codegraph_mutated"].as_bool(),
            Some(false)
        );
    }

    fn validate_edit_blocking_source_update_with_severity_trace() -> Value {
        let mut source_update = validate_edit_blocking_source_update();
        source_update["validation_packet"]["final_status"] = json!("blocking_graph_error");
        source_update["validation_packet"]["severity_summary"] = json!({
            "schema_version": 1,
            "final_status": "blocking_graph_error",
            "max_severity": "blocking",
            "hard_interrupt_available": true,
            "must_fix_before_continuing": true,
            "status_precedence": [
                "tool_error",
                "blocking_graph_error",
                "warning",
                "unknown",
                "diagnostic_only",
                "ok"
            ]
        });
        source_update["validation_packet"]["severity_decisions"] = json!([
            {
                "severity": "blocking",
                "source": "finding",
                "reason": "exact graph/source dangling CALLS proof",
                "finding_id": "finding://validate-edit/blocking",
                "validation_rule_id": CG_MVP3_CALLS_DANGLING_TARGET,
                "proof_level": "graph_source_verified",
                "claimability_effect": "claimable",
                "interrupt_eligible": true,
                "tool_error": false,
                "tool_error_kind": "none",
                "agent_action": {"must_fix_before_continuing": true},
                "lifecycle_effect": "claimable_current"
            }
        ]);
        source_update["validation_packet"]["severity_aggregation_trace"] = json!({
            "schema_version": 1,
            "decisions": source_update["validation_packet"]["severity_decisions"].clone(),
            "final_status": "blocking_graph_error",
            "max_severity": "blocking",
            "hard_interrupt_available": true,
            "tool_error": false,
            "notes": ["blocking finding wins by severity precedence"]
        });
        source_update["validation_packet"]["editor_policy"] = json!({
            "editor_policy_version": 1,
            "recommended_editor_action": "show_blocking_modal",
            "should_show_modal": true,
            "should_show_warning_panel": false,
            "should_allow_continue": false,
            "should_request_revalidation": true,
            "safe_to_autofix": false,
            "source_edits_performed": false,
            "daemon_integration_available": false,
            "plugin_integration_available": false,
            "metadata_advisory_only": true
        });
        source_update
    }

    fn validate_edit_many_findings_source_update() -> Value {
        let mut source_update = validate_edit_non_interrupt_source_update();
        let warning = source_update["validation_packet"]["warnings"][0].clone();
        let unknown = source_update["validation_packet"]["unknowns"][0].clone();
        let diagnostic = source_update["validation_packet"]["diagnostics"][0].clone();
        source_update["validation_packet"]["warnings"] = Value::Array(
            (0..8)
                .map(|index| {
                    let mut item = warning.clone();
                    item["validation_rule_id"] =
                        json!(format!("CG_MVP3_WARNING_TRUNCATION_{index}"));
                    item
                })
                .collect(),
        );
        source_update["validation_packet"]["unknowns"] = Value::Array(
            (0..6)
                .map(|index| {
                    let mut item = unknown.clone();
                    item["validation_rule_id"] =
                        json!(format!("CG_MVP3_UNKNOWN_TRUNCATION_{index}"));
                    item
                })
                .collect(),
        );
        source_update["validation_packet"]["diagnostics"] = Value::Array(
            (0..6)
                .map(|index| {
                    let mut item = diagnostic.clone();
                    item["validation_rule_id"] =
                        json!(format!("CG_MVP3_DIAGNOSTIC_TRUNCATION_{index}"));
                    item
                })
                .collect(),
        );
        source_update
    }

    fn validate_edit_bloated_budget_source_update() -> Value {
        let mut source_update = validate_edit_blocking_source_update_with_severity_trace();
        let warning = json!({
            "validation_rule_id": "CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL",
            "classification": "warn",
            "severity": "warning",
            "message": "new unresolved local call",
            "file": "src/a.ts",
            "source_span": {
                "repo_relative_path": "src/a.ts",
                "start_line": 3,
                "start_column": 5,
                "end_line": 3,
                "end_column": 20
            },
            "recommended_fix": "Define missing_fn or correct the call.",
            "suggested_next_steps": ["Define missing_fn.", "Rerun validate-edit."],
            "affected_delta": {
                "unresolved_reference": {
                    "name": "missing_fn",
                    "relation": "CALLS",
                    "reference_class": "repo_local_candidate",
                    "claimability": "claimable_as_source_text_reference_only"
                },
                "large_audit_copy": "x".repeat(2048)
            }
        });
        source_update["validation_packet"]["warnings"] = Value::Array(
            (0..8)
                .map(|index| {
                    let mut item = warning.clone();
                    item["finding_id"] = json!(format!("finding://warning/{index}"));
                    item
                })
                .collect(),
        );
        source_update["validation_packet"]["unknowns"] = Value::Array(
            (0..5)
                .map(|index| {
                    json!({
                        "finding_id": format!("finding://unknown/{index}"),
                        "validation_rule_id": format!("CG_MVP3_UNKNOWN_BUDGET_{index}"),
                        "classification": "unknown",
                        "message": "bounded unknown",
                        "recommended_fix": "Inspect source span."
                    })
                })
                .collect(),
        );
        source_update["validation_packet"]["diagnostics"] = Value::Array(
            (0..5)
                .map(|index| {
                    json!({
                        "finding_id": format!("finding://diagnostic/{index}"),
                        "validation_rule_id": format!("CG_MVP3_DIAGNOSTIC_BUDGET_{index}"),
                        "classification": "diagnostic",
                        "message": "diagnostic metadata"
                    })
                })
                .collect(),
        );
        source_update["validation_packet"]["unresolved_references"] = json!({
            "schema_version": 1,
            "new_count": 1,
            "resolved_count": 0,
            "by_class": {"repo_local_candidate": 1},
            "not_graph_proof": true,
            "escalated_total": 1,
            "escalated": [{
                "file": "src/a.ts",
                "name": "missing_fn",
                "relation": "CALLS",
                "reference_class": "repo_local_candidate",
                "severity": "warning",
                "proof_strength": "text_evidence",
                "claimability": "claimable_as_source_text_reference_only",
                "span": {"line_start": 3, "line_end": 3},
                "recommended_fix": "Define missing_fn or correct the call."
            }]
        });
        source_update["timings"] = json!({"audit_only_stage_timings": "x".repeat(4096)});
        source_update["per_file_status"] = json!((0..32)
            .map(|index| json!({"path": format!("src/{index}.ts"), "status": "accepted", "audit": "x".repeat(256)}))
            .collect::<Vec<_>>());
        source_update["staged_availability"] = json!({
            "graph_db_status": "ready",
            "candidate_spool_status": "stale",
            "vector_runtime_status": "stale",
            "vector_audit_status": "missing",
            "graph_proof_available": true,
            "candidate_only_available": false,
            "candidate_context_available": true,
            "active_candidate_sources": ["graph_db", "stage0_text_evidence", "symbol_lookup"],
            "claimability": {"claimable": true, "graph_proof_available": true},
            "layer_readiness": {
                "graph_db": {"status": "ready", "ready": true, "graph_proof": true, "verbose": "x".repeat(1024)},
                "candidate_spool": {"status": "stale", "ready": false, "candidate_only": true, "verbose": "x".repeat(1024)}
            },
            "warnings": ["x".repeat(1024)]
        });
        source_update
    }

    fn assert_compact_validate_edit_safety_fields(packet: &Value) {
        for field in [
            "status",
            "final_severity",
            "must_fix_before_continuing",
            "hard_interrupt_available",
            "changed_files",
            "claimability",
            "lifecycle",
            "recovery_commands_pointer",
            "stale_unsafe_blockers",
            "blocking_error_count",
            "warning_count",
            "unknown_count",
            "diagnostic_count",
            "proof_ladder_changes_summary",
            "omitted_count",
            "expansion_handles",
        ] {
            assert!(
                packet.get(field).is_some(),
                "missing compact field {field}: {packet}"
            );
        }
        assert!(packet["severity_summary"].is_object());
        assert!(packet["severity_trace"].is_object());
        assert!(packet["editor_policy"].is_object());
    }

    #[test]
    fn compact_severity_fields_preserved() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_blocking_source_update(),
        );

        assert_compact_validate_edit_safety_fields(&packet);
        assert_eq!(
            packet["final_severity"].as_str(),
            Some("blocking_graph_error")
        );
        assert!(packet["top_blocking_source_span"].is_object(), "{packet}");
        assert!(packet["top_recommended_fix"].as_str().is_some(), "{packet}");
        cleanup_repo(repo);
    }

    #[test]
    fn critical_fields_not_omitted_under_truncation() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_many_findings_source_update(),
        );
        agent_use_validate_edit_finalize_budget(&mut packet, AgentUseDetailMode::Compact, 1024);

        assert_eq!(packet["output_truncated"].as_bool(), Some(true), "{packet}");
        assert_compact_validate_edit_safety_fields(&packet);
        assert!(packet["omitted_count"].as_u64().unwrap_or_default() > 0);
        assert!(packet["expansion_handles"].as_array().is_some());
        assert_eq!(packet["source_update_packet"], Value::Null);
        cleanup_repo(repo);
    }

    #[test]
    fn explain_audit_severity_trace_available() {
        for detail_mode in [AgentUseDetailMode::Explain, AgentUseDetailMode::Audit] {
            let repo = test_repo();
            let profile = test_profile(&repo);
            let options = validate_edit_test_options(&repo, detail_mode, false);
            let packet = agent_use_validate_edit_packet_json(
                &profile,
                &options,
                validate_edit_blocking_source_update_with_severity_trace(),
            );

            assert_eq!(
                packet["severity_trace"]["mode"].as_str(),
                Some(detail_mode.label())
            );
            assert!(packet["severity_trace"]["per_finding_severity_mapping"]
                .as_array()
                .is_some_and(|items| !items.is_empty()));
            assert!(packet["severity_trace"]["final_aggregation_trace"].is_object());
            assert_eq!(
                packet["severity_trace"]["full_graph_dump_included"].as_bool(),
                Some(false)
            );
            cleanup_repo(repo);
        }
    }

    #[test]
    fn fail_on_blocking_mode_consistent() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let default_options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let fail_options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, true);
        let mut default_packet = agent_use_validate_edit_packet_json(
            &profile,
            &default_options,
            validate_edit_blocking_source_update_with_severity_trace(),
        );
        let mut fail_packet = agent_use_validate_edit_packet_json(
            &profile,
            &fail_options,
            validate_edit_blocking_source_update_with_severity_trace(),
        );

        agent_use_validate_edit_apply_exit_policy(&mut default_packet, false);
        agent_use_validate_edit_apply_exit_policy(&mut fail_packet, true);

        assert!(default_packet.get("_cli_exit_code").is_none());
        assert_eq!(fail_packet["_cli_exit_code"].as_i64(), Some(2));
        assert_eq!(default_packet["status"], fail_packet["status"]);
        assert_eq!(
            default_packet["must_fix_before_continuing"],
            fail_packet["must_fix_before_continuing"]
        );
        cleanup_repo(repo);
    }

    #[test]
    fn fail_on_blocking_does_not_change_severity() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let default_options = validate_edit_test_options(&repo, AgentUseDetailMode::Explain, false);
        let fail_options = validate_edit_test_options(&repo, AgentUseDetailMode::Explain, true);
        let mut default_packet = agent_use_validate_edit_packet_json(
            &profile,
            &default_options,
            validate_edit_blocking_source_update_with_severity_trace(),
        );
        let mut fail_packet = agent_use_validate_edit_packet_json(
            &profile,
            &fail_options,
            validate_edit_blocking_source_update_with_severity_trace(),
        );

        agent_use_validate_edit_apply_exit_policy(&mut default_packet, false);
        agent_use_validate_edit_apply_exit_policy(&mut fail_packet, true);

        assert_eq!(
            default_packet["final_severity"],
            fail_packet["final_severity"]
        );
        assert_eq!(
            default_packet["severity_summary"],
            fail_packet["severity_summary"]
        );
        assert_eq!(
            default_packet["severity_trace"],
            fail_packet["severity_trace"]
        );
        cleanup_repo(repo);
    }

    #[test]
    fn compact_output_no_full_graph_dump() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_blocking_source_update(),
        );
        let serialized = serde_json::to_string(&packet).expect("packet serializes");

        assert_eq!(packet["full_graph_dump_included"].as_bool(), Some(false));
        assert_eq!(packet["full_source_bodies_included"].as_bool(), Some(false));
        assert_eq!(packet["source_update_packet"], Value::Null);
        assert!(!serialized.contains("\"nodes\""), "{packet}");
        assert!(!serialized.contains("\"edges\""), "{packet}");
        cleanup_repo(repo);
    }

    #[test]
    fn audit_output_restores_severity_reasoning() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Audit, false);
        let packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_blocking_source_update_with_severity_trace(),
        );

        assert!(packet["source_update_packet"].is_object(), "{packet}");
        assert!(packet["severity_trace"]["per_finding_severity_mapping"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
        assert!(packet["severity_trace"]["severity_precedence_decision"].is_array());
        assert!(packet["severity_trace"]["tool_error_vs_validation_blocker"].is_object());
        assert!(packet["severity_trace"]["proof_ladder_details"].is_object());
        cleanup_repo(repo);
    }

    #[test]
    fn future_editor_policy_metadata_defined_without_daemon() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_blocking_source_update(),
        );
        let editor_policy = &packet["editor_policy"];

        assert_eq!(editor_policy["editor_policy_version"].as_u64(), Some(1));
        assert!(editor_policy["recommended_editor_action"]
            .as_str()
            .is_some());
        assert_eq!(editor_policy["safe_to_autofix"].as_bool(), Some(false));
        assert_eq!(
            editor_policy["source_edits_performed"].as_bool(),
            Some(false)
        );
        assert_eq!(
            editor_policy["daemon_integration_available"].as_bool(),
            Some(false)
        );
        cleanup_repo(repo);
    }

    #[test]
    fn source_edits_performed_false() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_blocking_source_update(),
        );

        assert_eq!(
            packet["editor_policy"]["source_edits_performed"].as_bool(),
            Some(false)
        );
        cleanup_repo(repo);
    }

    #[test]
    fn daemon_integration_available_false() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_blocking_source_update(),
        );

        assert_eq!(
            packet["editor_policy"]["daemon_integration_available"].as_bool(),
            Some(false)
        );
        cleanup_repo(repo);
    }

    #[test]
    fn max_output_bytes_respected_if_supported() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_many_findings_source_update(),
        );
        agent_use_validate_edit_finalize_budget(&mut packet, AgentUseDetailMode::Compact, 1024);

        let budget = &packet["agent_json_budget"];
        assert_eq!(budget["max_output_bytes"].as_u64(), Some(1024));
        assert_eq!(
            budget["required_safety_fields_preserved"].as_bool(),
            Some(true)
        );
        if budget["max_output_bytes_exceeded"].as_bool() == Some(false) {
            assert!(serialized_json_len(&packet) <= 1024);
        }
        assert_compact_validate_edit_safety_fields(&packet);
        cleanup_repo(repo);
    }

    #[test]
    fn validate_edit_compact_budget_enforced() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_bloated_budget_source_update(),
        );
        agent_use_validate_edit_finalize_budget(
            &mut packet,
            AgentUseDetailMode::Compact,
            DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        );

        assert!(
            serialized_json_len(&packet) <= DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
            "{}",
            serialized_json_len(&packet)
        );
        assert_eq!(
            packet["agent_json_budget"]["max_output_bytes"].as_u64(),
            Some(DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES as u64)
        );
        assert_eq!(
            packet["agent_json_budget"]["max_output_bytes_exceeded"].as_bool(),
            Some(false)
        );
        assert_compact_validate_edit_safety_fields(&packet);
        cleanup_repo(repo);
    }

    #[test]
    fn evidence_first_shedding_order_enforced() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_bloated_budget_source_update(),
        );
        agent_use_validate_edit_finalize_budget(
            &mut packet,
            AgentUseDetailMode::Compact,
            DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        );

        assert!(packet["validation_packet"]["blocking_errors"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
        assert!(packet["validation_packet"]["warnings"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
        assert!(packet["timings"].is_null(), "{packet}");
        assert!(packet["per_file_status"].is_null(), "{packet}");
        assert_eq!(
            packet["agent_json_budget"]["evidence_first_shedding"].as_bool(),
            Some(true)
        );
        cleanup_repo(repo);
    }

    #[test]
    fn metadata_shed_before_evidence() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_bloated_budget_source_update(),
        );
        agent_use_validate_edit_finalize_budget(
            &mut packet,
            AgentUseDetailMode::Compact,
            DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        );

        assert!(packet["source_update_packet"].is_null());
        assert!(
            packet["validation_packet"]["blocking_errors"][0]["validation_rule_id"]
                .as_str()
                .is_some()
        );
        assert!(
            packet["validation_packet"]["blocking_errors"][0]["recommended_fix"]
                .as_str()
                .is_some()
        );
        assert_eq!(
            packet["agent_json_budget"]["metadata_shed_before_evidence"].as_bool(),
            Some(true)
        );
        cleanup_repo(repo);
    }

    #[test]
    fn blocking_errors_survive_budget() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_bloated_budget_source_update(),
        );
        agent_use_validate_edit_finalize_budget(
            &mut packet,
            AgentUseDetailMode::Compact,
            DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        );
        let first = &packet["validation_packet"]["blocking_errors"][0];
        assert_eq!(
            first["validation_rule_id"].as_str(),
            Some(CG_MVP3_CALLS_DANGLING_TARGET)
        );
        assert!(first["source_span"].is_object(), "{first}");
        assert!(first["recommended_fix"].as_str().is_some(), "{first}");
        assert!(
            first["suggested_next_steps"].as_array().is_some(),
            "{first}"
        );
        cleanup_repo(repo);
    }

    #[test]
    fn unresolved_references_survive_budget() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_bloated_budget_source_update(),
        );
        agent_use_validate_edit_finalize_budget(
            &mut packet,
            AgentUseDetailMode::Compact,
            DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        );

        assert_eq!(
            packet["unresolved_references"]["new_count"].as_u64(),
            Some(1)
        );
        assert_eq!(
            packet["validation_packet"]["unresolved_references"]["not_graph_proof"].as_bool(),
            Some(true)
        );
        assert!(
            packet["validation_packet"]["unresolved_references"]["escalated"][0]["recommended_fix"]
                .as_str()
                .is_some()
        );
        cleanup_repo(repo);
    }

    #[test]
    fn finding_serialization_deduped() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_bloated_budget_source_update(),
        );
        agent_use_validate_edit_finalize_budget(
            &mut packet,
            AgentUseDetailMode::Compact,
            DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        );

        assert!(packet["hard_interrupt"]["errors"].is_null(), "{packet}");
        assert!(packet["severity_trace"]["warning_unknown_diagnostic_details"].is_null());
        assert!(packet["validation_packet"]["blocking_errors"][0].is_object());
        cleanup_repo(repo);
    }

    #[test]
    fn lifecycle_recovery_metadata_deduped() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_bloated_budget_source_update(),
        );
        agent_use_validate_edit_finalize_budget(
            &mut packet,
            AgentUseDetailMode::Compact,
            DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        );

        assert!(packet["recovery_commands"].is_null(), "{packet}");
        assert_eq!(
            packet["recovery_commands_pointer"].as_str(),
            Some("validation_recovery_commands")
        );
        assert_eq!(packet["lifecycle"]["claimable"].as_bool(), Some(true));
        cleanup_repo(repo);
    }

    #[test]
    fn critical_fields_not_omitted() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_bloated_budget_source_update(),
        );
        agent_use_validate_edit_finalize_budget(
            &mut packet,
            AgentUseDetailMode::Compact,
            DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        );

        assert_compact_validate_edit_safety_fields(&packet);
        assert!(packet["top_blocking_source_span"].is_object(), "{packet}");
        assert!(packet["top_recommended_fix"].as_str().is_some(), "{packet}");
        cleanup_repo(repo);
    }

    #[test]
    fn expansion_handles_present_when_truncated() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_bloated_budget_source_update(),
        );
        agent_use_validate_edit_finalize_budget(
            &mut packet,
            AgentUseDetailMode::Compact,
            DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES,
        );

        assert!(packet["agent_json_budget"]["truncated"].as_bool() == Some(true));
        assert!(packet["expansion_handles"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
        cleanup_repo(repo);
    }

    #[test]
    fn compact_bound_not_raised() {
        assert_eq!(DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES, 12 * 1024);
    }

    #[test]
    fn explain_audit_still_full_detail() {
        for detail_mode in [AgentUseDetailMode::Explain, AgentUseDetailMode::Audit] {
            let repo = test_repo();
            let profile = test_profile(&repo);
            let options = validate_edit_test_options(&repo, detail_mode, false);
            let packet = agent_use_validate_edit_packet_json(
                &profile,
                &options,
                validate_edit_blocking_source_update_with_severity_trace(),
            );
            assert!(packet["severity_trace"]["per_finding_severity_mapping"]
                .as_array()
                .is_some_and(|items| !items.is_empty()));
            cleanup_repo(repo);
        }
    }

    #[test]
    fn validate_edit_packet_wraps_real_validation_packet() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = AgentUseValidateEditOptions {
            repo: repo.clone(),
            changed_paths: vec![PathBuf::from("src/a.ts")],
            detail_mode: AgentUseDetailMode::Compact,
            max_output_bytes: None,
            fail_on_blocking: true,
            task_id: Some("task-1".to_string()),
            edit_intent: Some("rename".to_string()),
            expected_touched_files: vec![PathBuf::from("src/a.ts")],
            max_validation_ms: None,
        };
        let source_update = json!({
            "status": "updated",
            "agent_use_command": "agent-use watch",
            "changed_paths": ["src/a.ts"],
            "rejected_paths": [],
            "no_op_paths": [],
            "validation_packet": {
                "schema_version": 1,
                "packet_kind": "graph_validation_packet",
                "status": "blocking_graph_error",
                "must_fix_before_continuing": true,
                "changed_files": ["src/a.ts"],
                "graph_delta": {},
                "blocking_errors": [{"validation_rule_id": "CG_MVP3_CALLS_DANGLING_TARGET"}],
                "warnings": [],
                "unknowns": [],
                "diagnostics": [],
                "summary_counts_by_rule_id": {"CG_MVP3_CALLS_DANGLING_TARGET": 1},
                "summary_counts_by_classification": {"block": 1},
                "summary_counts_by_relation_kind": {"calls": 1},
                "validation_rules_evaluated": [],
                "validation_rules_skipped": [],
                "relation_family_status": {},
                "activation_gate_state": {},
                "claimability": {"claimable": true},
                "proof_ladder_changes": {},
                "lifecycle": {"claimable": true},
                "stale_unsafe_blockers": [],
                "top_blocking_source_spans": [],
                "recommended_next_steps": ["fix caller"],
                "hard_interrupt_available": true,
                "hard_interrupt": {"packet_kind": "hard_interrupt"},
                "omitted_count": 0,
                "expansion_handles": []
            },
            "hard_interrupt_available": true,
            "hard_interrupt": {"packet_kind": "hard_interrupt"},
            "claimability": {"claimable": true},
            "lifecycle": {"claimable": true},
            "recovery_commands": ["codegraph-mcp agent-use index --repo <repo> --json"],
            "normal_dot_codegraph_mutated": false,
            "timings": {"total_update_plus_delta_ms": 1}
        });

        let packet = agent_use_validate_edit_packet_json(&profile, &options, source_update);
        assert_eq!(packet["packet_kind"].as_str(), Some("validate_edit_packet"));
        assert_eq!(packet["status"].as_str(), Some("blocking_graph_error"));
        assert_eq!(packet["must_fix_before_continuing"].as_bool(), Some(true));
        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(true));
        assert_eq!(packet["external_profile_db_used"].as_bool(), Some(true));
        assert_eq!(packet["no_dot_codegraph_fallback"].as_bool(), Some(true));
        assert_eq!(packet["public_claim"].as_bool(), Some(false));
        assert_eq!(
            packet["expected_touched_files_missing"]
                .as_array()
                .expect("missing array")
                .len(),
            0
        );
        cleanup_repo(repo);
    }

    #[test]
    fn cli_hard_interrupt_propagation_correct() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_blocking_source_update(),
        );

        assert_eq!(packet["status"].as_str(), Some("blocking_graph_error"));
        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(true));
        assert_eq!(packet["must_fix_before_continuing"].as_bool(), Some(true));
        assert_eq!(
            packet["validation_packet"]["hard_interrupt_available"].as_bool(),
            Some(true)
        );
        let errors = packet["hard_interrupt"]["errors"]
            .as_array()
            .expect("hard interrupt errors");
        assert_eq!(errors.len(), 1);
        let error = &errors[0];
        assert_eq!(
            error["validation_rule_id"].as_str(),
            Some(CG_MVP3_CALLS_DANGLING_TARGET)
        );
        assert!(error["source_span"].is_object(), "{packet:?}");
        assert!(error["recommended_fix"]
            .as_str()
            .is_some_and(|fix| !fix.is_empty()));
        let steps = error["suggested_next_steps"]
            .as_array()
            .expect("suggested next steps");
        assert!(steps.iter().any(|step| {
            step.as_str()
                .is_some_and(|step| step.to_ascii_lowercase().contains("rerun validation"))
        }));
        assert_eq!(
            packet["normal_dot_codegraph_mutated"].as_bool(),
            Some(false)
        );
        cleanup_repo(repo);
    }

    #[test]
    fn non_interrupt_boundaries_preserved() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_non_interrupt_source_update(),
        );

        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(false));
        assert!(packet["hard_interrupt"].is_null(), "{packet:?}");
        assert_eq!(packet["must_fix_before_continuing"].as_bool(), Some(false));
        assert!(packet["warnings"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
        assert!(packet["unknowns"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
        assert!(packet["diagnostics"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
        assert_eq!(
            packet["validation_packet"]["proof_ladder_changes"]["text_evidence"]["graph_proof"]
                .as_bool(),
            Some(false)
        );
        assert_eq!(
            packet["validation_packet"]["proof_ladder_changes"]["candidate_evidence"]
                ["graph_proof"]
                .as_bool(),
            Some(false)
        );
        assert_eq!(
            packet["validation_packet"]["proof_ladder_changes"]["vector_evidence"]["graph_proof"]
                .as_bool(),
            Some(false)
        );
        assert_eq!(
            packet["validation_packet"]["proof_ladder_changes"]["source_navigation"]["graph_proof"]
                .as_bool(),
            Some(false)
        );
        cleanup_repo(repo);
    }

    #[test]
    fn unsafe_db_states_do_not_create_fake_interrupts() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let source_update = json!({
            "status": "stale",
            "changed_paths": ["src/a.ts"],
            "errors": ["repo_head_mismatch"],
            "claimability": {
                "claimable": false,
                "diagnostic_only": true,
                "graph_proof_available": false
            },
            "lifecycle": {
                "claimable": false,
                "diagnostic_only": true,
                "blocker_class": "lifecycle"
            },
            "normal_dot_codegraph_mutated": false
        });
        let packet = agent_use_validate_edit_packet_json(&profile, &options, source_update);

        assert_eq!(packet["status"].as_str(), Some("preflight_blocked"));
        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(false));
        assert!(packet["hard_interrupt"].is_null(), "{packet:?}");
        assert_eq!(
            packet["validation_packet"]["status"].as_str(),
            Some("diagnostic_only")
        );
        assert_eq!(
            packet["validation_packet"]["hard_interrupt_available"].as_bool(),
            Some(false)
        );
        assert_eq!(
            packet["validation_packet"]["stale_unsafe_blockers"][0].as_str(),
            Some("repo_head_mismatch")
        );
        assert_eq!(packet["claimability"]["claimable"].as_bool(), Some(false));
        assert_eq!(
            packet["normal_dot_codegraph_mutated"].as_bool(),
            Some(false)
        );
        cleanup_repo(repo);
    }

    #[test]
    fn mixed_packet_interrupt_contains_only_block_errors() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, true);
        let mut source_update = validate_edit_blocking_source_update();
        source_update["validation_packet"]["warnings"] = json!([
            {"validation_rule_id": "CG_MVP3_DYNAMIC_CALL_UNKNOWN", "classification": "warning"}
        ]);
        source_update["validation_packet"]["diagnostics"] = json!([
            {"validation_rule_id": "CG_MVP3_STALE_SIDECAR_DIAGNOSTIC", "classification": "diagnostic_only"}
        ]);
        source_update["validation_packet"]["summary_counts_by_classification"] = json!({
            "block": 1,
            "warning": 1,
            "diagnostic_only": 1
        });

        let packet = agent_use_validate_edit_packet_json(&profile, &options, source_update);
        let interrupt_errors = packet["hard_interrupt"]["errors"]
            .as_array()
            .expect("interrupt errors");
        assert_eq!(interrupt_errors.len(), 1, "{packet:?}");
        assert!(interrupt_errors
            .iter()
            .all(|error| error["classification"].as_str() == Some("block")));
        assert!(packet["warnings"]
            .as_array()
            .is_some_and(|items| items.len() == 1));
        assert!(packet["diagnostics"]
            .as_array()
            .is_some_and(|items| items.len() == 1));
        assert_eq!(
            packet["validation_packet"]["summary_counts_by_classification"]["block"].as_u64(),
            Some(1)
        );
        cleanup_repo(repo);
    }

    #[test]
    fn text_candidate_vector_source_navigation_not_interrupting() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let packet = agent_use_validate_edit_packet_json(
            &profile,
            &options,
            validate_edit_non_interrupt_source_update(),
        );

        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(false));
        assert!(packet["hard_interrupt"].is_null(), "{packet:?}");
        for key in [
            "text_evidence",
            "candidate_evidence",
            "vector_evidence",
            "source_navigation",
        ] {
            assert_eq!(
                packet["validation_packet"]["proof_ladder_changes"][key]["graph_proof"].as_bool(),
                Some(false),
                "{key} was promoted to graph proof: {packet:?}"
            );
        }
        cleanup_repo(repo);
    }

    #[test]
    fn validate_edit_preflight_block_is_non_claimable_with_synthetic_packet() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = AgentUseValidateEditOptions {
            repo: repo.clone(),
            changed_paths: vec![PathBuf::from("src/a.ts")],
            detail_mode: AgentUseDetailMode::Compact,
            max_output_bytes: None,
            fail_on_blocking: false,
            task_id: None,
            edit_intent: None,
            expected_touched_files: Vec::new(),
            max_validation_ms: None,
        };
        let source_update = json!({
            "status": "not_indexed",
            "changed_paths": ["src/a.ts"],
            "rejected_paths": [],
            "no_op_paths": [],
            "errors": ["db_missing"],
            "claimability": {"claimable": false, "diagnostic_only": true},
            "lifecycle": {"claimable": false, "diagnostic_only": true},
            "normal_dot_codegraph_mutated": false
        });

        let packet = agent_use_validate_edit_packet_json(&profile, &options, source_update);
        assert_eq!(packet["status"].as_str(), Some("preflight_blocked"));
        assert_eq!(packet["must_fix_before_continuing"].as_bool(), Some(true));
        assert_eq!(
            packet["validation_packet"]["status"].as_str(),
            Some("diagnostic_only")
        );
        assert_eq!(
            packet["validation_packet"]["stale_unsafe_blockers"][0].as_str(),
            Some("db_missing")
        );
        assert_eq!(packet["claimability"]["claimable"].as_bool(), Some(false));
        cleanup_repo(repo);
    }

    fn run_proof_integrity_for_paths(
        repo: &Path,
        store: &SqliteGraphStore,
        paths: &[&str],
        lifecycle: ValidationLifecycleState,
    ) -> Vec<ValidationFinding> {
        let rules = rules();
        let rule_by_id = rule_map(&rules);
        let profile = test_profile(repo);
        let mut findings = Vec::new();
        let mut seen = BTreeSet::new();
        for path in paths {
            for edge in store.list_edges_by_file(path).expect("list proof edges") {
                agent_use_validate_current_proof_edge_integrity(
                    &profile,
                    &rule_by_id,
                    lifecycle.clone(),
                    &edge,
                    Some(json!({"test_delta": true, "repo_relative_path": path})),
                    &mut findings,
                    &mut seen,
                );
            }
            for entity in store
                .list_entities_by_file(path)
                .expect("list proof entities")
            {
                agent_use_validate_current_entity_integrity(
                    &profile,
                    &rule_by_id,
                    lifecycle.clone(),
                    &entity,
                    Some(json!({"test_delta": true, "repo_relative_path": path})),
                    &mut findings,
                );
            }
        }
        findings
    }

    fn proof_integrity_rule(rule_id: &str) -> ValidationRule {
        agent_use_proof_integrity_validation_rules()
            .into_iter()
            .find(|rule| rule.validation_rule_id == rule_id)
            .expect("proof-integrity rule")
    }

    fn lifecycle_integrity_finding(
        rule_id: &str,
        lifecycle: ValidationLifecycleState,
        reason: &str,
    ) -> ValidationFinding {
        let rule = proof_integrity_rule(rule_id);
        classify_validation_finding(
            &rule,
            format!("finding://test/{rule_id}"),
            agent_use_lifecycle_integrity_input(lifecycle, format!("evidence://{rule_id}"), reason),
        )
    }

    fn run_edge_validation(
        repo: &Path,
        store: &SqliteGraphStore,
        edge: &Edge,
        lifecycle: ValidationLifecycleState,
    ) -> Vec<ValidationFinding> {
        run_edge_validation_with_rule(
            repo,
            store,
            edge,
            lifecycle,
            Some(CG_MVP3_CALLS_DANGLING_TARGET),
        )
    }

    fn run_edge_validation_with_rule(
        repo: &Path,
        store: &SqliteGraphStore,
        edge: &Edge,
        lifecycle: ValidationLifecycleState,
        preferred_rule_id: Option<&str>,
    ) -> Vec<ValidationFinding> {
        let rules = rules();
        let rule_by_id = rule_map(&rules);
        let profile = AgentUseProfile {
            profile_name: "test".to_string(),
            repo_root: repo.to_path_buf(),
            repo_identity_label: "test".to_string(),
            repo_identity_hash: "test".to_string(),
            profile_root: repo.join("profile"),
            db_path: repo.join("validation.sqlite"),
            candidate_spool_path: repo.join("candidate.jsonl"),
            candidate_spool_query_index_path: repo.join("candidate.sqlite"),
            vector_runtime_path: repo.join("vector-runtime.json"),
            vector_audit_path: repo.join("vector-audit.json"),
            lock_or_publish_state_path: repo.join("publish-state.json"),
            delta_state_path: repo.join("delta-state.json"),
            lifecycle_expectations: Vec::new(),
            recovery_commands: Vec::new(),
            mcp_args: Vec::new(),
            binary_profile: "test".to_string(),
            scope_policy: IndexScopeOptions::default(),
        };
        let mut findings = Vec::new();
        let mut seen = BTreeSet::new();
        agent_use_validate_current_calls_edge(
            &profile,
            store,
            &rule_by_id,
            lifecycle,
            edge,
            Some(json!({"test_delta": true})),
            preferred_rule_id,
            &mut findings,
            &mut seen,
        )
        .expect("validate edge");
        findings
    }

    fn run_import_edge_validation_with_rule(
        repo: &Path,
        store: &SqliteGraphStore,
        edge: &Edge,
        lifecycle: ValidationLifecycleState,
        preferred_rule_id: Option<&str>,
    ) -> Vec<ValidationFinding> {
        let rules = rules();
        let rule_by_id = rule_map(&rules);
        let profile = AgentUseProfile {
            profile_name: "test".to_string(),
            repo_root: repo.to_path_buf(),
            repo_identity_label: "test".to_string(),
            repo_identity_hash: "test".to_string(),
            profile_root: repo.join("profile"),
            db_path: repo.join("validation.sqlite"),
            candidate_spool_path: repo.join("candidate.jsonl"),
            candidate_spool_query_index_path: repo.join("candidate.sqlite"),
            vector_runtime_path: repo.join("vector-runtime.json"),
            vector_audit_path: repo.join("vector-audit.json"),
            lock_or_publish_state_path: repo.join("publish-state.json"),
            delta_state_path: repo.join("delta-state.json"),
            lifecycle_expectations: Vec::new(),
            recovery_commands: Vec::new(),
            mcp_args: Vec::new(),
            binary_profile: "test".to_string(),
            scope_policy: IndexScopeOptions::default(),
        };
        let mut findings = Vec::new();
        let mut seen = BTreeSet::new();
        agent_use_validate_current_import_edge(
            &profile,
            store,
            &rule_by_id,
            lifecycle,
            edge,
            Some(json!({"test_delta": true})),
            preferred_rule_id,
            &mut findings,
            &mut seen,
        )
        .expect("validate import edge");
        findings
    }

    fn run_source_role_validation(
        repo: &Path,
        store: &SqliteGraphStore,
        edge: &Edge,
        lifecycle: ValidationLifecycleState,
    ) -> Vec<ValidationFinding> {
        let rules = rules();
        let rule_by_id = rule_map(&rules);
        let profile = test_profile(repo);
        let mut findings = Vec::new();
        let mut seen = BTreeSet::new();
        agent_use_validate_current_source_role_boundary_edge(
            &profile,
            store,
            &rule_by_id,
            lifecycle,
            edge,
            Some(json!({"test_delta": true})),
            &mut findings,
            &mut seen,
        )
        .expect("validate source role edge");
        findings
    }

    fn run_tests_relation_validation(
        repo: &Path,
        store: &SqliteGraphStore,
        edge: &Edge,
        lifecycle: ValidationLifecycleState,
    ) -> Vec<ValidationFinding> {
        let rules = rules();
        let rule_by_id = rule_map(&rules);
        let profile = test_profile(repo);
        let mut findings = Vec::new();
        let mut seen = BTreeSet::new();
        agent_use_validate_current_tests_relation_edge(
            &profile,
            store,
            &rule_by_id,
            lifecycle,
            edge,
            Some(json!({"test_delta": true})),
            &mut findings,
            &mut seen,
        )
        .expect("validate TESTS-family edge");
        findings
    }

    fn run_activation_gated_contract_validation(
        repo: &Path,
        store: &SqliteGraphStore,
        edge: &Edge,
        lifecycle: ValidationLifecycleState,
    ) -> Vec<ValidationFinding> {
        let rules = rules();
        let rule_by_id = rule_map(&rules);
        let profile = test_profile(repo);
        let mut findings = Vec::new();
        let mut seen = BTreeSet::new();
        agent_use_validate_current_activation_gated_contract_edge(
            &profile,
            store,
            &rule_by_id,
            lifecycle,
            edge,
            Some(json!({"test_delta": true})),
            &BTreeSet::new(),
            &BTreeMap::new(),
            &mut findings,
            &mut seen,
        )
        .expect("validate activation-gated contract edge");
        findings
    }

    fn assert_no_blocking_findings(label: &str, findings: &[ValidationFinding]) {
        assert!(
            findings
                .iter()
                .all(|finding| finding.classification != ValidationClassification::Block),
            "{label} unexpectedly blocked: {findings:?}"
        );
        assert!(
            findings
                .iter()
                .all(|finding| !finding.reverified_graph_source_proof),
            "{label} unexpectedly claimed graph/source proof: {findings:?}"
        );
    }

    fn rule_by_id_owned(rule_id: &str) -> ValidationRule {
        rules()
            .into_iter()
            .find(|rule| rule.validation_rule_id == rule_id)
            .unwrap_or_else(|| panic!("missing validation rule {rule_id}"))
    }

    #[test]
    fn exact_reads_writes_checks_activation_gated() {
        let rules = agent_use_activation_gated_contract_validation_rules();
        let reads = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_READS_DANGLING_SYMBOL)
            .expect("READS rule");
        let writes = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_WRITES_DANGLING_SYMBOL)
            .expect("WRITES rule");
        assert_eq!(
            reads.supported_relation_status,
            SupportedRelationStatus::ExactBlockingCandidate
        );
        assert_eq!(
            writes.supported_relation_status,
            SupportedRelationStatus::ExactBlockingCandidate
        );
        assert!(reads
            .activation_condition
            .contains("source span re-verification"));
        let status = agent_use_relation_family_status_json();
        assert_eq!(
            status["reads_writes"]["status"].as_str(),
            Some("exact_blocking_candidate")
        );
        assert_eq!(status["reads_writes"]["activated"].as_bool(), Some(true));
    }

    #[test]
    fn exact_reads_missing_target_blocks_if_supported() {
        let repo = test_repo();
        write_source(&repo, "src/access.ts", "let x = missingSymbol;\n");
        let store = test_store(&repo);
        let reader = function_entity("src/access.ts", "reader", 1);
        store.upsert_entity(&reader).expect("reader");
        let edge = relation_edge(
            &reader,
            RelationKind::Reads,
            "entity://src/access.ts/missingSymbol",
            SourceSpan::with_columns("src/access.ts", 1, 9, 1, 22),
            Exactness::ParserVerified,
            EvidenceRole::Production,
        );

        let findings = run_activation_gated_contract_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let finding = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_READS_DANGLING_SYMBOL)
            .expect("READS dangling finding");
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(finding.relation_kind, Some(RelationKind::Reads));
        assert!(finding.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn exact_writes_missing_target_blocks_if_supported() {
        let repo = test_repo();
        write_source(&repo, "src/access.ts", "missingSymbol = 1;\n");
        let store = test_store(&repo);
        let writer = function_entity("src/access.ts", "writer", 1);
        store.upsert_entity(&writer).expect("writer");
        let edge = relation_edge(
            &writer,
            RelationKind::Writes,
            "entity://src/access.ts/missingSymbol",
            SourceSpan::with_columns("src/access.ts", 1, 1, 1, 14),
            Exactness::ParserVerified,
            EvidenceRole::Production,
        );

        let findings = run_activation_gated_contract_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let finding = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_WRITES_DANGLING_SYMBOL)
            .expect("WRITES dangling finding");
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(finding.relation_kind, Some(RelationKind::Writes));
        assert!(finding.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn computed_dynamic_reads_writes_unknown_not_blocking() {
        let repo = test_repo();
        write_source(&repo, "src/access.ts", "value = bag[key];\n");
        let store = test_store(&repo);
        let reader = function_entity("src/access.ts", "reader", 1);
        store.upsert_entity(&reader).expect("reader");
        let edge = relation_edge(
            &reader,
            RelationKind::Reads,
            "entity://dynamic/key",
            SourceSpan::with_columns("src/access.ts", 1, 9, 1, 17),
            Exactness::StaticHeuristic,
            EvidenceRole::Production,
        );

        let findings = run_activation_gated_contract_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let finding = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_READS_DANGLING_SYMBOL)
            .expect("dynamic READS unknown");
        assert_eq!(finding.classification, ValidationClassification::Unknown);
        assert_ne!(finding.blocking_level, ValidationBlockingLevel::Blocking);
        assert!(!finding.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn exact_route_handler_checks_activation_gated() {
        let rules = agent_use_activation_gated_contract_validation_rules();
        let route = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_ROUTE_HANDLER_DANGLING_TARGET)
            .expect("route handler rule");
        assert_eq!(
            route.supported_relation_status,
            SupportedRelationStatus::ExactBlockingCandidate
        );
        assert!(route.activation_condition.contains("literal route"));
        let status = agent_use_relation_family_status_json();
        assert_eq!(
            status["route_handler"]["status"].as_str(),
            Some("exact_blocking_candidate")
        );
        assert_eq!(
            status["route_handler"]["computed_routes"].as_str(),
            Some("unknown_not_blocking")
        );
    }

    #[test]
    fn exact_route_handler_missing_blocks_if_supported() {
        let repo = test_repo();
        write_source(&repo, "src/routes.ts", "app.get('/x', missingHandler);\n");
        let store = test_store(&repo);
        let route = route_entity("src/routes.ts", "GET /x", 1);
        store.upsert_entity(&route).expect("route");
        let edge = relation_edge(
            &route,
            RelationKind::Handles,
            "entity://src/routes.ts/missingHandler",
            SourceSpan::with_columns("src/routes.ts", 1, 1, 1, 30),
            Exactness::ParserVerified,
            EvidenceRole::Production,
        );

        let findings = run_activation_gated_contract_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let finding = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_ROUTE_HANDLER_DANGLING_TARGET)
            .expect("route handler dangling finding");
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(finding.relation_kind, Some(RelationKind::Handles));
        assert!(finding.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn removed_exact_route_handler_delta_blocks_when_source_still_names_target() {
        let repo = test_repo();
        write_source(&repo, "src/routes.ts", "app.get('/x', handleAdmin);\n");
        let store = test_store(&repo);
        let route = route_entity("src/routes.ts", "GET /x", 1);
        let handler = function_entity("src/routes.ts", "handleAdmin", 1);
        store.upsert_entity(&route).expect("route");
        let span = SourceSpan::with_columns("src/routes.ts", 1, 1, 1, 28);
        let edge = relation_edge(
            &route,
            RelationKind::Handles,
            &handler.id,
            span.clone(),
            Exactness::ParserVerified,
            EvidenceRole::Production,
        );
        let claimability =
            NormalizedClaimabilityMetadata::graph_source_proof("test exact route handler edge");
        let endpoint_span = Some(SourceSpan::with_columns("src/routes.ts", 1, 1, 1, 12));
        let removed = codegraph_index::EdgeDeltaEntry {
            stable_identity_key: edge.id.clone(),
            old_stable_identity_key: Some(edge.id.clone()),
            new_stable_identity_key: None,
            old: None,
            new: None,
            edge_id: edge.id.clone(),
            source_entity_id: route.id.clone(),
            target_entity_id: handler.id.clone(),
            source_endpoint: codegraph_index::EdgeDeltaEndpointSummary {
                entity_id: route.id.clone(),
                name: Some(route.name.clone()),
                qualified_name: Some(route.qualified_name.clone()),
                entity_kind: Some(route.kind),
                repo_relative_path: Some(route.repo_relative_path.clone()),
                source_span: endpoint_span.clone(),
                hydrated: true,
            },
            target_endpoint: codegraph_index::EdgeDeltaEndpointSummary {
                entity_id: handler.id.clone(),
                name: Some(handler.name.clone()),
                qualified_name: Some(handler.qualified_name.clone()),
                entity_kind: Some(handler.kind),
                repo_relative_path: Some(handler.repo_relative_path.clone()),
                source_span: endpoint_span,
                hydrated: true,
            },
            relation: RelationKind::Handles,
            relation_kind: RelationKind::Handles.to_string(),
            repo_relative_path: "src/routes.ts".to_string(),
            source_span: span,
            exactness: Exactness::ParserVerified,
            exactness_label: Exactness::ParserVerified.to_string(),
            derived: false,
            provenance_edges: Vec::new(),
            provenance_status: "base_fact".to_string(),
            source_role: EvidenceRole::Production,
            edge_class: EdgeClass::BaseExact.to_string(),
            edge_context: EdgeContext::Production.to_string(),
            normalized_claimability: claimability.clone(),
            claimability,
            proof_strength: "graph_source".to_string(),
            lifecycle_status: "current".to_string(),
            relation_support: "supported".to_string(),
            change_kind: "removed".to_string(),
            reason: "target_removed".to_string(),
            warnings: Vec::new(),
        };
        let rules = rules();
        let rule_by_id = rule_map(&rules);
        let mut findings = Vec::new();
        let mut seen = BTreeSet::new();

        agent_use_validate_removed_route_delta_edge(
            &test_profile(&repo),
            &store,
            &rule_by_id,
            ValidationLifecycleState::claimable_current(),
            &removed,
            CG_MVP3_ROUTE_HANDLER_DANGLING_TARGET,
            &mut findings,
            &mut seen,
        )
        .expect("removed route delta validation");

        let finding = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_ROUTE_HANDLER_DANGLING_TARGET)
            .expect("route handler dangling finding");
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(finding.relation_kind, Some(RelationKind::Handles));
        assert!(finding.reverified_graph_source_proof);
        assert!(finding.source_span.is_some());
        assert!(finding.recommended_fix.is_some());
        assert!(!finding.suggested_next_steps.is_empty());
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn framework_convention_route_unknown_not_blocking() {
        let repo = test_repo();
        write_source(&repo, "src/routes.ts", "frameworkConventionRoute('/x');\n");
        let store = test_store(&repo);
        let route = route_entity("src/routes.ts", "GET /x", 1);
        store.upsert_entity(&route).expect("route");
        let edge = relation_edge(
            &route,
            RelationKind::Handles,
            "entity://framework/convention/handler",
            SourceSpan::with_columns("src/routes.ts", 1, 1, 1, 31),
            Exactness::StaticHeuristic,
            EvidenceRole::Production,
        );

        let findings = run_activation_gated_contract_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let finding = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_ROUTE_COMPUTED_UNKNOWN)
            .expect("route unknown finding");
        assert_eq!(finding.classification, ValidationClassification::Unknown);
        assert!(!finding.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn config_package_text_evidence_warning_only_by_default() {
        let rules = rules();
        let rule = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING)
            .expect("config text rule");
        let finding = agent_use_config_package_text_evidence_warning(
            rule,
            ValidationLifecycleState::claimable_current(),
            "package/foo/Config.in",
            json!({"text_evidence_id": "text_evidence:package/foo/Config.in:1"}),
            "Config.in text evidence changed",
        );
        assert_eq!(finding.classification, ValidationClassification::Warn);
        assert_eq!(finding.proof_status, ValidationProofStatus::NotGraphProof);
        assert!(!finding.reverified_graph_source_proof);
        assert!(finding.evidence_items.iter().all(|item| item.evidence_kind
            == ValidationEvidenceKind::TextEvidence
            && !item.graph_proof));
    }

    #[test]
    fn exact_config_package_mismatch_blocks_only_if_supported() {
        let rules = rules();
        let rule = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_CONFIG_PACKAGE_EXACT_MISMATCH)
            .expect("config exact rule");
        assert_eq!(
            rule.supported_relation_status,
            SupportedRelationStatus::Unsupported
        );
        let finding = agent_use_config_package_unsupported_unknown(
            rule,
            ValidationLifecycleState::claimable_current(),
            json!({"relation_family": "config_package"}),
            "exact Config.in/package graph mismatch is not fixture-backed in the current frontend",
        );
        assert_ne!(finding.classification, ValidationClassification::Block);
        assert!(!finding.reverified_graph_source_proof);
    }

    #[test]
    fn unsupported_relation_classes_unknown_not_blocking() {
        let rules = rules();
        let rule = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_ROUTE_UNSUPPORTED_FRAMEWORK_UNKNOWN)
            .expect("unsupported route rule");
        let repo = test_repo();
        write_source(&repo, "src/routes.ts", "conventionRoute('/x');\n");
        let store = test_store(&repo);
        let route = route_entity("src/routes.ts", "GET /x", 1);
        let edge = relation_edge(
            &route,
            RelationKind::Handles,
            "entity://unsupported/framework",
            SourceSpan::with_columns("src/routes.ts", 1, 1, 1, 22),
            Exactness::StaticHeuristic,
            EvidenceRole::Production,
        );
        let finding = agent_use_relation_contract_unknown(
            rule,
            ValidationLifecycleState::claimable_current(),
            &edge,
            json!({"framework": "unsupported"}),
            "unsupported framework route relation is unknown, not proof",
        );
        assert_eq!(finding.classification, ValidationClassification::Unknown);
        assert_ne!(finding.blocking_level, ValidationBlockingLevel::Blocking);
        assert!(!finding.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn no_new_relation_support_claim_without_fixtures() {
        let status = agent_use_relation_family_status_json();
        assert_eq!(
            status["config_package"]["exact_graph_mismatch"].as_str(),
            Some("unsupported_unknown_until_fixture_backed")
        );
        let rules = agent_use_activation_gated_contract_validation_rules();
        let config_exact = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_CONFIG_PACKAGE_EXACT_MISMATCH)
            .expect("config exact rule");
        assert_eq!(
            config_exact.supported_relation_status,
            SupportedRelationStatus::Unsupported
        );
    }

    #[test]
    fn no_text_candidate_vector_evidence_promoted_to_graph_proof() {
        let rules = rules();
        let rule = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_READS_DANGLING_SYMBOL)
            .expect("READS rule");
        for evidence_kind in [
            ValidationEvidenceKind::TextEvidence,
            ValidationEvidenceKind::Candidate,
            ValidationEvidenceKind::Vector,
            ValidationEvidenceKind::SourceNavigation,
        ] {
            let mut input = ValidationReverificationInput::exact_graph_source(
                ValidationLifecycleState::claimable_current(),
                "evidence://non-graph-reads",
                "non-graph evidence cannot prove a READS dangling symbol",
            );
            input.graph_source_relation_reverified = false;
            input.evidence_items = vec![ValidationEvidenceItem::non_graph(
                evidence_kind,
                "evidence://non-graph-reads",
                "non-graph evidence cannot prove a READS dangling symbol",
            )];
            let finding = classify_validation_finding(
                rule,
                format!("finding://reads/non-graph/{evidence_kind:?}"),
                input,
            );
            assert_ne!(finding.classification, ValidationClassification::Block);
            assert!(!finding.reverified_graph_source_proof);
            assert!(finding.evidence_items.iter().all(|item| !item.graph_proof));
        }
    }

    #[test]
    fn activation_gated_contract_checks_no_dot_codegraph_mutation() {
        let repo = test_repo();
        write_source(&repo, "src/access.ts", "missingSymbol = 1;\n");
        let store = test_store(&repo);
        let writer = function_entity("src/access.ts", "writer", 1);
        store.upsert_entity(&writer).expect("writer");
        let edge = relation_edge(
            &writer,
            RelationKind::Writes,
            "entity://src/access.ts/missingSymbol",
            SourceSpan::with_columns("src/access.ts", 1, 1, 1, 14),
            Exactness::ParserVerified,
            EvidenceRole::Production,
        );

        let _findings = run_activation_gated_contract_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert!(!repo.join(".codegraph").exists());
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn heuristic_dynamic_calls_not_blocking() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "client[method](); callback();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let mut edge = calls_edge(
            &caller,
            "entity://runtime/dynamic-dispatch",
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 17),
        );
        edge.exactness = Exactness::StaticHeuristic;
        edge.edge_class = EdgeClass::BaseHeuristic;

        let findings = run_edge_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert_no_blocking_findings("heuristic dynamic call", &findings);
        assert!(findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_CALLS_DANGLING_TARGET
                && finding.classification == ValidationClassification::Diagnostic
                && !finding.reverified_graph_source_proof
        }));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn computed_imports_not_blocking() {
        let repo = test_repo();
        write_source(&repo, "src/importer.ts", "import('./' + name);\n");
        let store = test_store(&repo);
        let importer = file_entity("src/importer.ts");
        store.upsert_entity(&importer).expect("importer");
        let mut edge = imports_edge(
            &importer,
            "entity://runtime/computed-import",
            SourceSpan::with_columns("src/importer.ts", 1, 1, 1, 21),
        );
        edge.exactness = Exactness::StaticHeuristic;
        edge.edge_class = EdgeClass::BaseHeuristic;

        let findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_IMPORTS_DANGLING_TARGET),
        );

        assert_no_blocking_findings("computed import", &findings);
        assert!(findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_IMPORTS_DANGLING_TARGET
                && finding.classification == ValidationClassification::Diagnostic
                && !finding.reverified_graph_source_proof
        }));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn macro_preprocessor_unsupported_not_blocking() {
        let rule = rule_by_id_owned(CG_MVP3_CALLS_DANGLING_TARGET);
        let finding = agent_use_calls_boundary_unknown(
            &rule,
            ValidationLifecycleState::claimable_current(),
            json!({
                "language": "rust_or_c_family",
                "relation_kind": "CALLS",
                "macro_or_preprocessor_relation": true,
            }),
            "macro/preprocessor-generated call lacks expansion proof",
        );

        assert_no_blocking_findings("macro/preprocessor call", &[finding]);
    }

    #[test]
    fn framework_convention_only_not_blocking() {
        let repo = test_repo();
        write_source(&repo, "src/routes.ts", "export const GET = handler;\n");
        let store = test_store(&repo);
        let route = route_entity("src/routes.ts", "GET /convention", 1);
        store.upsert_entity(&route).expect("route");
        let edge = relation_edge(
            &route,
            RelationKind::Handles,
            "entity://framework/convention/handler",
            SourceSpan::with_columns("src/routes.ts", 1, 1, 1, 27),
            Exactness::StaticHeuristic,
            EvidenceRole::Production,
        );

        let findings = run_activation_gated_contract_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert_no_blocking_findings("framework convention route", &findings);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn text_only_relation_not_blocking() {
        let rule = rule_by_id_owned(CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING);
        let finding = agent_use_config_package_text_evidence_warning(
            &rule,
            ValidationLifecycleState::claimable_current(),
            "docs/relations.md",
            json!({"text_evidence_id": "text://docs/relations.md:1"}),
            "docs mention a function/import, but text evidence is not graph proof",
        );

        assert_eq!(finding.classification, ValidationClassification::Warn);
        assert_eq!(finding.proof_status, ValidationProofStatus::NotGraphProof);
        assert_no_blocking_findings("text-only relation", &[finding]);
    }

    #[test]
    fn candidate_vector_source_navigation_not_blocking() {
        let rule = rule_by_id_owned(CG_MVP3_CALLS_DANGLING_TARGET);
        let mut findings = Vec::new();
        for evidence_kind in [
            ValidationEvidenceKind::Candidate,
            ValidationEvidenceKind::Vector,
            ValidationEvidenceKind::SourceNavigation,
        ] {
            let mut input = ValidationReverificationInput::exact_graph_source(
                ValidationLifecycleState::claimable_current(),
                format!("evidence://{evidence_kind:?}"),
                "suggested relation is not graph/source proof",
            );
            input.graph_source_relation_reverified = false;
            input.evidence_items = vec![ValidationEvidenceItem::non_graph(
                evidence_kind,
                format!("evidence://{evidence_kind:?}"),
                "suggested relation is not graph/source proof",
            )];
            findings.push(classify_validation_finding(
                &rule,
                format!("finding://adversarial/{evidence_kind:?}"),
                input,
            ));
        }

        assert_no_blocking_findings("candidate/vector/source-navigation relation", &findings);
    }

    #[test]
    fn unsupported_language_frontend_unknown_not_blocking() {
        let call_rule = rule_by_id_owned(CG_MVP3_CALLS_DANGLING_TARGET);
        let import_rule = rule_by_id_owned(CG_MVP3_IMPORTS_DANGLING_TARGET);
        let findings = vec![
            agent_use_calls_boundary_unknown(
                &call_rule,
                ValidationLifecycleState::claimable_current(),
                json!({"language": "secondary_frontend_without_exact_calls"}),
                "unsupported frontend cannot produce exact CALLS proof",
            ),
            agent_use_import_boundary_unknown(
                &import_rule,
                ValidationLifecycleState::claimable_current(),
                json!({"language": "secondary_frontend_without_exact_import_resolution"}),
                "unsupported frontend cannot produce exact IMPORTS target proof",
            ),
        ];

        assert_no_blocking_findings("unsupported language frontend", &findings);
    }

    #[test]
    fn runtime_only_dependency_unknown_not_blocking() {
        let rule = rule_by_id_owned(CG_MVP3_IMPORTS_DANGLING_TARGET);
        let finding = agent_use_import_boundary_unknown(
            &rule,
            ValidationLifecycleState::claimable_current(),
            json!({
                "runtime_only": true,
                "selector": "process.env.PLUGIN",
                "relation_kind": "IMPORTS",
            }),
            "runtime environment or plugin loader selects the target",
        );

        assert_no_blocking_findings("runtime-only dependency", &[finding]);
    }

    #[test]
    fn ambiguous_rename_not_blocking() {
        let rule = rule_by_id_owned(CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED);
        let finding = agent_use_calls_boundary_unknown(
            &rule,
            ValidationLifecycleState::claimable_current(),
            json!({
                "rename_status": "unknown",
                "ambiguity": "duplicate_content_paths",
                "fallback_add_remove": true,
            }),
            "duplicate-content rename is ambiguous and cannot prove stale CALLS target",
        );

        assert_no_blocking_findings("ambiguous duplicate-content rename", &[finding]);
    }

    #[test]
    fn over_budget_closure_not_blocking() {
        let rule = rule_by_id_owned(CG_MVP3_CALLS_DANGLING_TARGET);
        let mut input = ValidationReverificationInput::exact_graph_source(
            ValidationLifecycleState::claimable_current(),
            "evidence://over-budget-closure",
            "closure budget hit before deterministic validation could complete",
        );
        input.over_budget = true;

        let finding = classify_validation_finding(&rule, "finding://over-budget-closure", input);

        assert_eq!(finding.classification, ValidationClassification::Degraded);
        assert_ne!(finding.blocking_level, ValidationBlockingLevel::Blocking);
        assert!(!finding.reverified_graph_source_proof);
    }

    #[test]
    fn stale_sidecar_not_blocking_graph_error() {
        let rule = rule_by_id_owned(CG_MVP3_CALLS_DANGLING_TARGET);
        let mut input = ValidationReverificationInput::exact_graph_source(
            ValidationLifecycleState::claimable_current(),
            "vector://stale-sidecar",
            "stale sidecar freshness is provenance state, not broken graph behavior",
        );
        input.graph_source_relation_reverified = false;
        input.evidence_items = vec![
            ValidationEvidenceItem::non_graph(
                ValidationEvidenceKind::Vector,
                "vector://stale-sidecar",
                "stale vector chunks are not graph proof",
            ),
            ValidationEvidenceItem::non_graph(
                ValidationEvidenceKind::Candidate,
                "candidate://stale-sidecar",
                "stale candidate spool is not graph proof",
            ),
        ];

        let finding = classify_validation_finding(&rule, "finding://stale-sidecar", input);

        assert_no_blocking_findings("stale sidecar freshness", &[finding]);
    }

    #[test]
    fn compact_omitted_detail_not_blocking() {
        let rule = rule_by_id_owned(CG_MVP3_CALLS_DANGLING_TARGET);
        let findings = (0..5)
            .map(|index| {
                agent_use_calls_boundary_diagnostic(
                    &rule,
                    ValidationLifecycleState::claimable_current(),
                    json!({"candidate_index": index}),
                    "compact packet candidate detail remains diagnostic",
                )
            })
            .collect::<Vec<_>>();
        let packet = ValidationPacket::new(
            vec!["src/caller.ts".to_string()],
            json!({"summary": {"adversarial_fixture_count": findings.len()}}),
            findings,
            vec![rule],
            Vec::new(),
            json!({"claimable": true}),
            json!({}),
            json!({"claimable": true, "current": true}),
        );

        let compact = packet.compact_agent_json(1);

        assert_eq!(compact["must_fix_before_continuing"].as_bool(), Some(false));
        assert!(compact["blocking_errors"].as_array().unwrap().is_empty());
        assert!(compact["omitted_count"].as_u64().unwrap_or_default() > 0);
        assert!(ValidationPacket::critical_safety_fields_preserved_in(
            &compact
        ));
    }

    #[test]
    fn no_false_blocking_findings_in_adversarial_matrix() {
        let call_rule = rule_by_id_owned(CG_MVP3_CALLS_DANGLING_TARGET);
        let import_rule = rule_by_id_owned(CG_MVP3_IMPORTS_DANGLING_TARGET);
        let route_rule = rule_by_id_owned(CG_MVP3_ROUTE_UNSUPPORTED_FRAMEWORK_UNKNOWN);
        let config_rule = rule_by_id_owned(CG_MVP3_CONFIG_PACKAGE_TEXT_ONLY_WARNING);

        let mut findings = Vec::new();
        findings.push(agent_use_calls_boundary_unknown(
            &call_rule,
            ValidationLifecycleState::claimable_current(),
            json!({"category": "heuristic_dynamic_call"}),
            "dynamic dispatch is heuristic",
        ));
        findings.push(agent_use_import_boundary_unknown(
            &import_rule,
            ValidationLifecycleState::claimable_current(),
            json!({"category": "computed_import"}),
            "computed import path is runtime-only",
        ));
        findings.push(agent_use_relation_contract_unknown(
            &route_rule,
            ValidationLifecycleState::claimable_current(),
            &relation_edge(
                &route_entity("src/routes.ts", "GET /convention", 1),
                RelationKind::Handles,
                "entity://framework/convention/handler",
                SourceSpan::with_columns("src/routes.ts", 1, 1, 1, 12),
                Exactness::StaticHeuristic,
                EvidenceRole::Production,
            ),
            json!({"category": "framework_convention"}),
            "framework convention-only route is unknown",
        ));
        findings.push(agent_use_config_package_text_evidence_warning(
            &config_rule,
            ValidationLifecycleState::claimable_current(),
            "package/foo/Config.in",
            json!({"category": "text_only_config"}),
            "Config/package text evidence is warning-only",
        ));

        for evidence_kind in [
            ValidationEvidenceKind::Candidate,
            ValidationEvidenceKind::Vector,
            ValidationEvidenceKind::SourceNavigation,
        ] {
            let mut input = ValidationReverificationInput::exact_graph_source(
                ValidationLifecycleState::claimable_current(),
                format!("evidence://matrix/{evidence_kind:?}"),
                "adversarial non-graph evidence",
            );
            input.graph_source_relation_reverified = false;
            input.evidence_items = vec![ValidationEvidenceItem::non_graph(
                evidence_kind,
                format!("evidence://matrix/{evidence_kind:?}"),
                "adversarial non-graph evidence",
            )];
            findings.push(classify_validation_finding(
                &call_rule,
                format!("finding://matrix/{evidence_kind:?}"),
                input,
            ));
        }

        assert_eq!(findings.len(), 7);
        assert_no_blocking_findings("adversarial false-positive matrix", &findings);
    }

    #[test]
    fn exact_calls_dangling_target_blocked() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "missingTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let edge = calls_edge(
            &caller,
            "entity://src/service.ts/missingTarget",
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 16),
        );

        let findings = run_edge_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let blocking = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_CALLS_DANGLING_TARGET)
            .expect("dangling finding");
        assert_eq!(blocking.classification, ValidationClassification::Block);
        assert_eq!(blocking.relation_kind, Some(RelationKind::Calls));
        assert_eq!(blocking.exactness, Some(Exactness::ParserVerified));
        assert!(blocking.source_span.is_some());
        assert!(blocking.reverified_graph_source_proof);
        assert!(blocking.reason.contains("missing callee"));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn production_to_test_target_role_mismatch_blocked() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "testOnlyTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        let target = test_entity("src/caller.ts", "testOnlyTarget", 1);
        store.upsert_entity(&caller).expect("caller");
        store.upsert_entity(&target).expect("target");
        let edge = calls_edge(
            &caller,
            &target.id,
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 17),
        );

        let findings = run_edge_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let blocking = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_CALLS_TARGET_ROLE_MISMATCH)
            .expect("role mismatch finding");
        assert_eq!(blocking.classification, ValidationClassification::Block);
        assert_eq!(blocking.source_role, Some(EvidenceRole::Production));
        assert!(blocking.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn test_mock_leakage_into_production_blocks() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/caller.ts",
            "testOnlyTarget(); mockTarget(); stubTarget();\n",
        );
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        let test_target = test_entity("tests/service.test.ts", "testOnlyTarget", 1);
        let mock_target = mock_entity("tests/mock_service.ts", "mockTarget", 1);
        let stub_target = stub_entity("tests/stub_service.ts", "stubTarget", 1);
        for entity in [&caller, &test_target, &mock_target, &stub_target] {
            store.upsert_entity(entity).expect("entity");
        }

        let test_edge = calls_edge(
            &caller,
            &test_target.id,
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 17),
        );
        let mock_edge = calls_edge(
            &caller,
            &mock_target.id,
            SourceSpan::with_columns("src/caller.ts", 1, 19, 1, 31),
        );
        let stub_edge = calls_edge(
            &caller,
            &stub_target.id,
            SourceSpan::with_columns("src/caller.ts", 1, 33, 1, 43),
        );

        let test_findings = run_source_role_validation(
            &repo,
            &store,
            &test_edge,
            ValidationLifecycleState::claimable_current(),
        );
        let mock_findings = run_source_role_validation(
            &repo,
            &store,
            &mock_edge,
            ValidationLifecycleState::claimable_current(),
        );
        let stub_findings = run_source_role_validation(
            &repo,
            &store,
            &stub_edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert!(test_findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_SOURCE_ROLE_TEST_EVIDENCE_IN_PRODUCTION_PROOF
                && finding.classification == ValidationClassification::Block
        }));
        assert!(mock_findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_SOURCE_ROLE_MOCK_EVIDENCE_IN_PRODUCTION_PROOF
                && finding.classification == ValidationClassification::Block
        }));
        assert!(stub_findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_SOURCE_ROLE_STUB_EVIDENCE_IN_PRODUCTION_PROOF
                && finding.classification == ValidationClassification::Block
        }));
        assert!(!repo.join(".codegraph").exists());
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn inline_test_not_promoted_to_production() {
        let repo = test_repo();
        write_source(&repo, "src/lib.rs", "inline_case();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/lib.rs", "caller", 1);
        let inline_test = inline_test_entity("src/lib.rs", "inline_case", 1);
        store.upsert_entity(&caller).expect("caller");
        store.upsert_entity(&inline_test).expect("inline test");
        let edge = calls_edge(
            &caller,
            &inline_test.id,
            SourceSpan::with_columns("src/lib.rs", 1, 1, 1, 14),
        );

        let findings = run_source_role_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let blocking = findings
            .iter()
            .find(|finding| {
                finding.validation_rule_id == CG_MVP3_SOURCE_ROLE_INLINE_TEST_PROMOTED_TO_PRODUCTION
            })
            .expect("inline test source-role finding");
        assert_eq!(blocking.classification, ValidationClassification::Block);
        assert_eq!(blocking.source_role, Some(EvidenceRole::Test));
        assert!(blocking.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn same_symbol_production_and_test_distinct() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "sharedSymbol();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        let prod_target = function_entity("src/service.ts", "sharedSymbol", 1);
        let test_target = test_entity("tests/service.test.ts", "sharedSymbol", 1);
        store.upsert_entity(&caller).expect("caller");
        store.upsert_entity(&prod_target).expect("prod target");
        store.upsert_entity(&test_target).expect("test target");
        let prod_edge = calls_edge(
            &caller,
            &prod_target.id,
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 15),
        );
        let test_edge = calls_edge(
            &caller,
            &test_target.id,
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 15),
        );

        let prod_findings = run_source_role_validation(
            &repo,
            &store,
            &prod_edge,
            ValidationLifecycleState::claimable_current(),
        );
        let test_findings = run_source_role_validation(
            &repo,
            &store,
            &test_edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert!(prod_findings.is_empty(), "{prod_findings:?}");
        assert!(test_findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_SOURCE_ROLE_TEST_EVIDENCE_IN_PRODUCTION_PROOF
                && finding.classification == ValidationClassification::Block
        }));
        assert_ne!(prod_target.id, test_target.id);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn production_to_test_target_role_mismatch_blocks() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "testOnlyTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        let target = test_entity("tests/service.test.ts", "testOnlyTarget", 1);
        store.upsert_entity(&caller).expect("caller");
        store.upsert_entity(&target).expect("target");
        let edge = calls_edge(
            &caller,
            &target.id,
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 17),
        );

        let findings = run_source_role_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert!(findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_SOURCE_ROLE_TEST_EVIDENCE_IN_PRODUCTION_PROOF
                && finding.classification == ValidationClassification::Block
        }));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn generated_evidence_not_production_proof() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/generated/client.generated.ts",
            "generatedTarget();\n",
        );
        let store = test_store(&repo);
        let caller = function_entity("src/generated/client.generated.ts", "caller", 1);
        let target = function_entity("src/service.ts", "generatedTarget", 1);
        store.upsert_entity(&caller).expect("caller");
        store.upsert_entity(&target).expect("target");
        let edge = calls_edge(
            &caller,
            &target.id,
            SourceSpan::with_columns("src/generated/client.generated.ts", 1, 1, 1, 18),
        );

        let findings = run_source_role_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert!(findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_SOURCE_ROLE_GENERATED_EVIDENCE_AS_PRODUCTION_PROOF
                && finding.classification == ValidationClassification::Block
        }));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn exact_tests_missing_target_classified() {
        let repo = test_repo();
        write_source(&repo, "tests/service.test.ts", "testedTarget();\n");
        let store = test_store(&repo);
        let test_case = test_entity("tests/service.test.ts", "tests behavior", 1);
        store.upsert_entity(&test_case).expect("test case");
        let edge = relation_edge(
            &test_case,
            RelationKind::Tests,
            "entity://src/service.ts/testedTarget",
            SourceSpan::with_columns("tests/service.test.ts", 1, 1, 1, 15),
            Exactness::ParserVerified,
            EvidenceRole::Test,
        );

        let findings = run_tests_relation_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let finding = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_TESTS_DANGLING_TARGET)
            .expect("TESTS dangling finding");
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(finding.relation_kind, Some(RelationKind::Tests));
        assert!(finding.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn optional_test_target_missing_warns() {
        let repo = test_repo();
        write_source(&repo, "tests/service.test.ts", "optionalTarget();\n");
        let store = test_store(&repo);
        let test_case = test_entity("tests/service.test.ts", "optional behavior", 1);
        store.upsert_entity(&test_case).expect("test case");
        let mut edge = relation_edge(
            &test_case,
            RelationKind::Tests,
            "entity://src/service.ts/optionalTarget",
            SourceSpan::with_columns("tests/service.test.ts", 1, 1, 1, 17),
            Exactness::ParserVerified,
            EvidenceRole::Test,
        );
        edge.metadata.insert(
            "optional_test_target".to_string(),
            json!("optional_test_target"),
        );

        let findings = run_tests_relation_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let warning = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_OPTIONAL_TEST_TARGET_MISSING)
            .expect("optional warning");
        assert_eq!(warning.classification, ValidationClassification::Warn);
        assert_eq!(warning.blocking_level, ValidationBlockingLevel::Warning);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn unsupported_tests_relation_unknown_not_blocking() {
        let repo = test_repo();
        write_source(&repo, "tests/service.test.ts", "heuristicTarget();\n");
        let store = test_store(&repo);
        let test_case = test_entity("tests/service.test.ts", "heuristic behavior", 1);
        store.upsert_entity(&test_case).expect("test case");
        let edge = relation_edge(
            &test_case,
            RelationKind::Tests,
            "entity://src/service.ts/heuristicTarget",
            SourceSpan::with_columns("tests/service.test.ts", 1, 1, 1, 18),
            Exactness::StaticHeuristic,
            EvidenceRole::Test,
        );

        let findings = run_tests_relation_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let unknown = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_UNSUPPORTED_TEST_RELATION_UNKNOWN)
            .expect("unsupported relation finding");
        assert_eq!(unknown.classification, ValidationClassification::Unknown);
        assert_ne!(unknown.blocking_level, ValidationBlockingLevel::Blocking);
        assert!(!unknown.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn text_only_test_mention_not_tests_proof() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/caller.ts",
            "// text says TESTS missingTarget\nprodTarget();\n",
        );
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 2);
        let target = function_entity("src/service.ts", "prodTarget", 1);
        store.upsert_entity(&caller).expect("caller");
        store.upsert_entity(&target).expect("target");
        let edge = calls_edge(
            &caller,
            &target.id,
            SourceSpan::with_columns("src/caller.ts", 2, 1, 2, 13),
        );

        let findings = run_tests_relation_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert!(findings.is_empty(), "{findings:?}");
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn source_role_changed_reported_and_respected() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "testOnlyTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        let target = test_entity("tests/service.test.ts", "testOnlyTarget", 1);
        store.upsert_entity(&caller).expect("caller");
        store.upsert_entity(&target).expect("target");
        let edge = calls_edge(
            &caller,
            &target.id,
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 17),
        );

        let findings = run_source_role_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let blocking = findings
            .iter()
            .find(|finding| {
                finding.validation_rule_id == CG_MVP3_SOURCE_ROLE_TEST_EVIDENCE_IN_PRODUCTION_PROOF
            })
            .expect("source role respected");
        assert_eq!(blocking.classification, ValidationClassification::Block);
        assert_eq!(blocking.source_role, Some(EvidenceRole::Test));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn stale_db_cannot_produce_blocking_source_role_finding() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "testOnlyTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        let target = test_entity("tests/service.test.ts", "testOnlyTarget", 1);
        store.upsert_entity(&caller).expect("caller");
        store.upsert_entity(&target).expect("target");
        let edge = calls_edge(
            &caller,
            &target.id,
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 17),
        );

        let findings = run_source_role_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::stale_non_claimable("repo_head_mismatch"),
        );

        assert!(findings.iter().all(|finding| {
            finding.classification != ValidationClassification::Block
                && !finding.reverified_graph_source_proof
        }));
        assert!(!repo.join(".codegraph").exists());
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn removed_callee_detected() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "removedTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let edge = calls_edge(
            &caller,
            "entity://src/service.ts/removedTarget",
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 16),
        );

        let findings = run_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED),
        );

        assert!(findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED
                && finding.classification == ValidationClassification::Block
        }));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn renamed_callee_detected_or_unknown_when_ambiguous() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "oldName();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let edge = calls_edge(
            &caller,
            "entity://src/old.ts/oldName",
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 10),
        );

        let findings = run_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED),
        );

        assert!(findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED
                && finding.classification == ValidationClassification::Block
        }));

        let rules = rules();
        let rule = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_CALLS_RENAMED_CALLEE_NOT_UPDATED)
            .expect("rename rule");
        let ambiguous = agent_use_calls_boundary_unknown(
            rule,
            ValidationLifecycleState::claimable_current(),
            json!({"rename_status": "unknown", "ambiguity": true}),
            "ambiguous rename cannot be exact CALLS proof",
        );
        assert_ne!(ambiguous.classification, ValidationClassification::Block);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn exact_imports_dangling_target_blocked() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/consumer.ts",
            "import { missingExport } from './service';\n",
        );
        let store = test_store(&repo);
        let importer = file_entity("src/consumer.ts");
        store.upsert_entity(&importer).expect("importer");
        let edge = imports_edge(
            &importer,
            "entity://src/service.ts/missingExport",
            SourceSpan::with_columns("src/consumer.ts", 1, 1, 1, 43),
        );

        let findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_IMPORTS_DANGLING_TARGET),
        );

        let blocking = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_IMPORTS_DANGLING_TARGET)
            .expect("dangling import finding");
        assert_eq!(blocking.classification, ValidationClassification::Block);
        assert_eq!(blocking.relation_kind, Some(RelationKind::Imports));
        assert!(blocking.source_span.is_some());
        assert!(blocking.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn deleted_export_still_imported_detected() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/consumer.ts",
            "import { removedExport } from './service';\n",
        );
        let store = test_store(&repo);
        let importer = file_entity("src/consumer.ts");
        store.upsert_entity(&importer).expect("importer");
        let edge = imports_edge(
            &importer,
            "entity://src/service.ts/removedExport",
            SourceSpan::with_columns("src/consumer.ts", 1, 1, 1, 43),
        );

        let findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_IMPORTS_DELETED_EXPORT_STILL_IMPORTED),
        );

        assert!(findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_IMPORTS_DELETED_EXPORT_STILL_IMPORTED
                && finding.classification == ValidationClassification::Block
        }));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn renamed_import_target_detected_or_unknown() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/consumer.ts",
            "import { renamedExport } from './old_name';\n",
        );
        let store = test_store(&repo);
        let importer = file_entity("src/consumer.ts");
        store.upsert_entity(&importer).expect("importer");
        let edge = imports_edge(
            &importer,
            "entity://src/old_name.ts/renamedExport",
            SourceSpan::with_columns("src/consumer.ts", 1, 1, 1, 44),
        );

        let findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED),
        );

        assert!(findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED
                && finding.classification == ValidationClassification::Block
        }));

        let rules = rules();
        let rule = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_IMPORTS_RENAMED_TARGET_NOT_UPDATED)
            .expect("rename import rule");
        let ambiguous = agent_use_import_boundary_unknown(
            rule,
            ValidationLifecycleState::claimable_current(),
            json!({"rename_status": "unknown", "ambiguity": true}),
            "ambiguous rename cannot be exact IMPORTS proof",
        );
        assert_ne!(ambiguous.classification, ValidationClassification::Block);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn import_alias_mismatch_detected_or_unknown() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/consumer.ts",
            "import { target as aliasTarget } from './service';\n",
        );
        let store = test_store(&repo);
        let target = function_entity("src/service.ts", "target", 1);
        let import_alias = import_entity("src/consumer.ts", "aliasTarget", 1);
        store.upsert_entity(&import_alias).expect("alias");
        let edge = alias_edge(
            &target,
            &import_alias,
            SourceSpan::with_columns("src/consumer.ts", 1, 1, 1, 51),
        );

        let findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH),
        );

        assert!(findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH
                && finding.classification == ValidationClassification::Block
        }));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn production_to_test_import_target_blocked() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/consumer.ts",
            "import { testOnly } from './test_helper';\n",
        );
        let store = test_store(&repo);
        let importer = file_entity("src/consumer.ts");
        let target = test_entity("tests/test_helper.ts", "testOnly", 1);
        store.upsert_entity(&importer).expect("importer");
        store.upsert_entity(&target).expect("target");
        let edge = imports_edge(
            &importer,
            &target.id,
            SourceSpan::with_columns("src/consumer.ts", 1, 1, 1, 42),
        );

        let findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_IMPORTS_DANGLING_TARGET),
        );

        let blocking = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_IMPORTS_TARGET_ROLE_MISMATCH)
            .expect("import role mismatch finding");
        assert_eq!(blocking.classification, ValidationClassification::Block);
        assert!(blocking.reverified_graph_source_proof);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn computed_dynamic_imports_warn_or_unknown() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/dynamic.ts",
            "export async function load(name: string) { return import(name); }\n",
        );
        let store = test_store(&repo);
        let importer = function_entity("src/dynamic.ts", "load", 1);
        let target = import_entity("src/dynamic.ts", "dynamic_import:name", 1);
        store.upsert_entity(&importer).expect("importer");
        store.upsert_entity(&target).expect("target");
        let mut edge = imports_edge(
            &importer,
            &target.id,
            SourceSpan::with_columns("src/dynamic.ts", 1, 52, 1, 64),
        );
        edge.exactness = Exactness::StaticHeuristic;
        edge.edge_class = EdgeClass::BaseHeuristic;
        edge.context = EdgeContext::Unknown;

        let findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_IMPORTS_DANGLING_TARGET),
        );

        assert!(findings.iter().all(|finding| {
            finding.classification != ValidationClassification::Block
                && !finding.reverified_graph_source_proof
        }));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn blocking_imports_have_source_spans() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/consumer.ts",
            "import { missingExport } from './service';\n",
        );
        let store = test_store(&repo);
        let importer = file_entity("src/consumer.ts");
        store.upsert_entity(&importer).expect("importer");
        let edge = imports_edge(
            &importer,
            "entity://missing",
            SourceSpan::with_columns("src/consumer.ts", 1, 1, 1, 43),
        );

        let findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_IMPORTS_DANGLING_TARGET),
        );

        assert!(findings
            .iter()
            .filter(|finding| finding.classification == ValidationClassification::Block)
            .all(|finding| finding.source_span.is_some()));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn blocking_imports_reverified_from_graph_source() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/consumer.ts",
            "import { missingExport } from './service';\n",
        );
        let store = test_store(&repo);
        let importer = file_entity("src/consumer.ts");
        store.upsert_entity(&importer).expect("importer");
        let edge = imports_edge(
            &importer,
            "entity://missing",
            SourceSpan::with_columns("src/consumer.ts", 1, 1, 1, 43),
        );

        let findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_IMPORTS_DANGLING_TARGET),
        );

        assert!(findings
            .iter()
            .any(|finding| finding.reverified_graph_source_proof));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn derived_import_missing_provenance_blocked_or_integrity_finding() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/consumer.ts",
            "import { target } from './service';\n",
        );
        let store = test_store(&repo);
        let importer = file_entity("src/consumer.ts");
        let target = function_entity("src/service.ts", "target", 1);
        store.upsert_entity(&importer).expect("importer");
        store.upsert_entity(&target).expect("target");
        let mut edge = imports_edge(
            &importer,
            &target.id,
            SourceSpan::with_columns("src/consumer.ts", 1, 1, 1, 36),
        );
        edge.derived = true;
        edge.provenance_edges = Vec::new();

        let findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_IMPORTS_DANGLING_TARGET),
        );

        assert!(findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_IMPORTS_DERIVED_MISSING_PROVENANCE
                && finding.classification == ValidationClassification::Block
        }));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn text_candidate_vector_source_navigation_imports_not_blocking() {
        let rules = rules();
        let rule = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_IMPORTS_DANGLING_TARGET)
            .expect("import rule");
        for (kind, evidence_id) in [
            (ValidationEvidenceKind::TextEvidence, "text://import"),
            (ValidationEvidenceKind::Candidate, "candidate://import"),
            (ValidationEvidenceKind::Vector, "vector://import"),
            (
                ValidationEvidenceKind::SourceNavigation,
                "source-navigation://import",
            ),
        ] {
            let mut input = ValidationReverificationInput::exact_graph_source(
                ValidationLifecycleState::claimable_current(),
                evidence_id,
                "non-graph evidence cannot prove exact import failure",
            );
            input.graph_source_relation_reverified = false;
            input.evidence_items = vec![ValidationEvidenceItem::non_graph(
                kind,
                evidence_id,
                "non-graph evidence cannot prove exact import failure",
            )];
            let finding = classify_validation_finding(
                rule,
                format!("finding://imports/non-graph/{evidence_id}"),
                input,
            );
            assert_ne!(finding.classification, ValidationClassification::Block);
            assert!(!finding.reverified_graph_source_proof);
        }
    }

    #[test]
    fn unsupported_language_import_unknown_not_blocking() {
        let rules = rules();
        let rule = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_IMPORTS_DANGLING_TARGET)
            .expect("import rule");
        let mut input = ValidationReverificationInput::exact_graph_source(
            ValidationLifecycleState::claimable_current(),
            "edge://unsupported-import",
            "unsupported frontend cannot produce exact import proof",
        );
        input.relation_supported = false;
        input.unsupported_relation = true;
        input.evidence_items = vec![ValidationEvidenceItem::non_graph(
            ValidationEvidenceKind::Diagnostic,
            "diagnostic://unsupported-import",
            "unsupported frontend import pattern",
        )];
        let finding = classify_validation_finding(rule, "finding://unsupported-import", input);
        assert_ne!(finding.classification, ValidationClassification::Block);
        assert!(!finding.reverified_graph_source_proof);
    }

    #[test]
    fn stale_db_cannot_produce_blocking_imports_finding() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/consumer.ts",
            "import { missingExport } from './service';\n",
        );
        let store = test_store(&repo);
        let importer = file_entity("src/consumer.ts");
        store.upsert_entity(&importer).expect("importer");
        let edge = imports_edge(
            &importer,
            "entity://missing",
            SourceSpan::with_columns("src/consumer.ts", 1, 1, 1, 43),
        );

        let findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::stale_non_claimable("stale_db"),
            Some(CG_MVP3_IMPORTS_DANGLING_TARGET),
        );

        assert!(findings
            .iter()
            .all(|finding| finding.classification != ValidationClassification::Block));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn exact_imports_no_dot_codegraph_mutation() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/consumer.ts",
            "import { missingExport } from './service';\n",
        );
        let store = test_store(&repo);
        let importer = file_entity("src/consumer.ts");
        store.upsert_entity(&importer).expect("importer");
        let edge = imports_edge(
            &importer,
            "entity://missing",
            SourceSpan::with_columns("src/consumer.ts", 1, 1, 1, 43),
        );

        let _findings = run_import_edge_validation_with_rule(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
            Some(CG_MVP3_IMPORTS_DANGLING_TARGET),
        );

        assert!(!repo.join(".codegraph").exists());
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn blocking_calls_have_source_spans() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "missingTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let edge = calls_edge(
            &caller,
            "entity://missing",
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 16),
        );

        let findings = run_edge_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert!(findings
            .iter()
            .filter(|finding| finding.classification == ValidationClassification::Block)
            .all(|finding| finding.source_span.is_some()));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn blocking_calls_reverified_from_graph_source() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "missingTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let edge = calls_edge(
            &caller,
            "entity://missing",
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 16),
        );

        let findings = run_edge_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert!(findings
            .iter()
            .filter(|finding| finding.classification == ValidationClassification::Block)
            .all(|finding| finding.reverified_graph_source_proof));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn derived_calls_missing_provenance_blocked_or_integrity_finding() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "missingTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let mut edge = calls_edge(
            &caller,
            "entity://missing",
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 16),
        );
        edge.derived = true;
        edge.exactness = Exactness::DerivedFromVerifiedEdges;
        edge.edge_class = EdgeClass::Derived;

        let findings = run_edge_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        let finding = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_CALLS_DERIVED_MISSING_PROVENANCE)
            .expect("missing provenance finding");
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.provenance["provenance_present"].as_bool(),
            Some(false)
        );
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn dynamic_macro_trait_calls_not_overclaimed() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "client[method]();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let mut edge = calls_edge(
            &caller,
            "entity://dynamic",
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 17),
        );
        edge.exactness = Exactness::StaticHeuristic;
        edge.edge_class = EdgeClass::BaseHeuristic;

        let findings = run_edge_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert!(findings
            .iter()
            .all(|finding| finding.classification != ValidationClassification::Block));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn text_candidate_vector_source_navigation_calls_not_blocking() {
        let rules = rules();
        let rule = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_CALLS_DANGLING_TARGET)
            .expect("rule");
        for evidence_kind in [
            ValidationEvidenceKind::TextEvidence,
            ValidationEvidenceKind::Candidate,
            ValidationEvidenceKind::Vector,
            ValidationEvidenceKind::SourceNavigation,
        ] {
            let mut input = ValidationReverificationInput::exact_graph_source(
                ValidationLifecycleState::claimable_current(),
                "evidence://non-graph",
                "non-graph evidence cannot prove exact CALLS failure",
            );
            input.evidence_items = vec![ValidationEvidenceItem::non_graph(
                evidence_kind,
                "evidence://non-graph",
                "non-graph evidence cannot prove exact CALLS failure",
            )];
            let finding = classify_validation_finding(
                rule,
                format!("finding://non-graph/{evidence_kind:?}"),
                input,
            );
            assert_ne!(finding.classification, ValidationClassification::Block);
            assert!(!finding.reverified_graph_source_proof);
        }
    }

    #[test]
    fn unsupported_language_call_unknown_not_blocking() {
        let rules = rules();
        let rule = rules
            .iter()
            .find(|rule| rule.validation_rule_id == CG_MVP3_CALLS_DANGLING_TARGET)
            .expect("rule");
        let finding = agent_use_calls_boundary_unknown(
            rule,
            ValidationLifecycleState::claimable_current(),
            json!({
                "language": "unsupported_fixture_language",
                "relation_kind": "CALLS",
                "support": "unsupported",
            }),
            "unsupported frontend cannot produce exact CALLS proof",
        );
        assert_ne!(finding.classification, ValidationClassification::Block);
        assert!(!finding.reverified_graph_source_proof);
    }

    #[test]
    fn stale_db_cannot_produce_blocking_calls_finding() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "missingTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let edge = calls_edge(
            &caller,
            "entity://missing",
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 16),
        );

        let findings = run_edge_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::stale_non_claimable("repo_head_mismatch"),
        );

        assert!(findings
            .iter()
            .all(|finding| finding.classification != ValidationClassification::Block));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn no_false_positive_from_comments_or_strings() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/caller.ts",
            "const text = \"missingTarget()\";\n// missingTarget();\n",
        );
        let store = test_store(&repo);
        let rules = rules();
        let rule_by_id = rule_map(&rules);
        let profile = AgentUseProfile {
            profile_name: "test".to_string(),
            repo_root: repo.clone(),
            repo_identity_label: "test".to_string(),
            repo_identity_hash: "test".to_string(),
            profile_root: repo.join("profile"),
            db_path: repo.join("validation.sqlite"),
            candidate_spool_path: repo.join("candidate.jsonl"),
            candidate_spool_query_index_path: repo.join("candidate.sqlite"),
            vector_runtime_path: repo.join("vector-runtime.json"),
            vector_audit_path: repo.join("vector-audit.json"),
            lock_or_publish_state_path: repo.join("publish-state.json"),
            delta_state_path: repo.join("delta-state.json"),
            lifecycle_expectations: Vec::new(),
            recovery_commands: Vec::new(),
            mcp_args: Vec::new(),
            binary_profile: "test".to_string(),
            scope_policy: IndexScopeOptions::default(),
        };
        let mut findings = Vec::new();
        let mut seen = BTreeSet::new();
        for edge in store
            .list_edges_by_file("src/caller.ts")
            .expect("list edges")
        {
            agent_use_validate_current_calls_edge(
                &profile,
                &store,
                &rule_by_id,
                ValidationLifecycleState::claimable_current(),
                &edge,
                None,
                Some(CG_MVP3_CALLS_DANGLING_TARGET),
                &mut findings,
                &mut seen,
            )
            .expect("validate edge");
        }
        assert!(findings.is_empty());
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn proof_edge_missing_source_span_blocks() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "missingTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let edge = calls_edge(
            &caller,
            "entity://missing",
            SourceSpan::with_columns("src/caller.ts", 99, 1, 99, 16),
        );
        store.upsert_edge(&edge).expect("edge");

        let findings = run_proof_integrity_for_paths(
            &repo,
            &store,
            &["src/caller.ts"],
            ValidationLifecycleState::claimable_current(),
        );

        let finding = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_PROOF_EDGE_MISSING_SOURCE_SPAN)
            .expect("generic missing span finding");
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::MissingRequiredSourceSpan
        );
        assert_eq!(finding.relation_kind, Some(RelationKind::Calls));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn source_span_eof_sentinel_does_not_block() {
        let repo = test_repo();
        write_source(
            &repo,
            "src/module.ts",
            "export const a = 1;\nexport const b = 2;\n",
        );
        let store = test_store(&repo);
        let module = file_entity("src/module.ts");
        let target = function_entity("src/module.ts", "target", 1);
        store.upsert_entity(&module).expect("module");
        store.upsert_entity(&target).expect("target");
        let edge = Edge {
            id: stable_edge_id(
                &module.id,
                RelationKind::Defines,
                &target.id,
                &SourceSpan::with_columns("src/module.ts", 1, 1, 3, 1),
            ),
            head_id: module.id.clone(),
            relation: RelationKind::Defines,
            tail_id: target.id.clone(),
            source_span: SourceSpan::with_columns("src/module.ts", 1, 1, 3, 1),
            repo_commit: None,
            file_hash: None,
            extractor: "exact-calls-validation-test".to_string(),
            confidence: 1.0,
            exactness: Exactness::ParserVerified,
            edge_class: EdgeClass::BaseExact,
            context: EdgeContext::Production,
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Default::default(),
        };
        store.upsert_edge(&edge).expect("edge");

        let findings = run_proof_integrity_for_paths(
            &repo,
            &store,
            &["src/module.ts"],
            ValidationLifecycleState::claimable_current(),
        );

        assert!(!findings.iter().any(|finding| {
            finding.validation_rule_id == CG_MVP3_PROOF_EDGE_MISSING_SOURCE_SPAN
                && finding.classification == ValidationClassification::Block
        }));
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn one_line_source_span_eof_sentinel_reverifies_text() {
        let repo = test_repo();
        write_source(
            &repo,
            "dir with spaces/spaced file.js",
            "export function spacedPathSymbol() { return 2; }\n",
        );
        let span = SourceSpan::with_columns("dir with spaces/spaced file.js", 1, 1, 2, 1);

        let snippet =
            agent_use_reverify_source_span_text(&repo, &span, "claimable graph edge proof")
                .expect("one-line EOF sentinel source span should resolve to source text");

        assert!(snippet.contains("spacedPathSymbol"));
        cleanup_repo(repo);
    }

    #[test]
    fn claimable_entity_missing_source_span_blocks_or_diagnostic_if_entity_span_optional() {
        let repo = test_repo();
        write_source(&repo, "src/service.ts", "export function service() {}\n");
        let store = test_store(&repo);
        let mut service = function_entity("src/service.ts", "service", 1);
        service.source_span = None;
        store.upsert_entity(&service).expect("service");

        let findings = run_proof_integrity_for_paths(
            &repo,
            &store,
            &["src/service.ts"],
            ValidationLifecycleState::claimable_current(),
        );

        let finding = findings
            .iter()
            .find(|finding| {
                finding.validation_rule_id == CG_MVP3_CLAIMABLE_ENTITY_MISSING_SOURCE_SPAN
            })
            .expect("entity missing span finding");
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::MissingRequiredSourceSpan
        );

        let mut file = file_entity("src/service.ts");
        file.id = "entity://src/service.ts/file-optional".to_string();
        file.source_span = None;
        store.upsert_entity(&file).expect("file entity");
        let findings = run_proof_integrity_for_paths(
            &repo,
            &store,
            &["src/service.ts"],
            ValidationLifecycleState::claimable_current(),
        );
        let optional = findings
            .iter()
            .find(|finding| {
                finding.validation_rule_id == CG_MVP3_CLAIMABLE_ENTITY_MISSING_SOURCE_SPAN
                    && finding.affected_entity["id"]
                        .as_str()
                        .is_some_and(|id| id.ends_with("file-optional"))
            })
            .expect("optional entity diagnostic");
        assert_ne!(optional.classification, ValidationClassification::Block);
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn derived_edge_missing_provenance_blocks() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "missingTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let mut edge = calls_edge(
            &caller,
            "entity://missing",
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 16),
        );
        edge.derived = true;
        edge.exactness = Exactness::DerivedFromVerifiedEdges;
        edge.edge_class = EdgeClass::Derived;
        let rules = rules();
        let rule_by_id = rule_map(&rules);
        let profile = test_profile(&repo);
        let mut findings = Vec::new();
        let mut seen = BTreeSet::new();
        agent_use_validate_current_proof_edge_integrity(
            &profile,
            &rule_by_id,
            ValidationLifecycleState::claimable_current(),
            &edge,
            Some(json!({"test_delta": true})),
            &mut findings,
            &mut seen,
        );

        let finding = findings
            .iter()
            .find(|finding| finding.validation_rule_id == CG_MVP3_DERIVED_EDGE_MISSING_PROVENANCE)
            .expect("generic missing provenance finding");
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::MissingRequiredProvenance
        );
        drop(store);
        cleanup_repo(repo);
    }

    #[test]
    fn db_lifecycle_mismatch_blocks() {
        let finding = lifecycle_integrity_finding(
            CG_MVP3_DB_LIFECYCLE_MISMATCH_AFTER_UPDATE,
            ValidationLifecycleState::claimable_current(),
            "validation preflight opened a different DB path than the update path",
        );
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::ReverifiedGraphIntegrity
        );
    }

    #[test]
    fn stale_db_validation_non_claimable() {
        let finding = lifecycle_integrity_finding(
            CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT,
            ValidationLifecycleState::stale_non_claimable("repo_head_mismatch"),
            "stale DB validation attempt must be diagnostic only",
        );
        assert_ne!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::StaleOrNonClaimableDb
        );
    }

    #[test]
    fn foreign_db_validation_non_claimable() {
        let mut lifecycle = ValidationLifecycleState::stale_non_claimable("repo_root_mismatch");
        lifecycle.stale = false;
        lifecycle.foreign = true;
        let finding = lifecycle_integrity_finding(
            CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT,
            lifecycle,
            "foreign DB validation attempt must be diagnostic only",
        );
        assert_ne!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::StaleOrNonClaimableDb
        );
    }

    #[test]
    fn schema_mismatch_validation_non_claimable() {
        let mut lifecycle = ValidationLifecycleState::stale_non_claimable("schema_mismatch");
        lifecycle.stale = false;
        lifecycle.schema_mismatched = true;
        let finding = lifecycle_integrity_finding(
            CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT,
            lifecycle,
            "schema mismatched DB validation attempt must be diagnostic only",
        );
        assert_ne!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::StaleOrNonClaimableDb
        );
    }

    #[test]
    fn temp_db_never_claimable() {
        let finding = lifecycle_integrity_finding(
            CG_MVP3_TEMP_DB_CLAIMABLE,
            ValidationLifecycleState::claimable_current(),
            "temporary DB path was marked claimable",
        );
        assert_eq!(finding.classification, ValidationClassification::Block);
    }

    #[test]
    fn old_good_db_preserved_on_failure() {
        let finding = lifecycle_integrity_finding(
            CG_MVP3_OLD_GOOD_DB_NOT_PRESERVED,
            ValidationLifecycleState::claimable_current(),
            "failed update did not preserve old-good DB",
        );
        assert_eq!(finding.classification, ValidationClassification::Block);
    }

    #[test]
    fn stale_graph_state_not_claimable() {
        let finding = lifecycle_integrity_finding(
            CG_MVP3_STALE_OR_FOREIGN_DB_VALIDATION_ATTEMPT,
            ValidationLifecycleState::stale_non_claimable("repo_head_mismatch"),
            "stale graph DB must not be used as claimable graph proof",
        );
        assert_ne!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::StaleOrNonClaimableDb
        );
    }

    #[test]
    fn partial_db_never_claimable() {
        let finding = lifecycle_integrity_finding(
            CG_MVP3_CORRUPT_OR_INCOMPLETE_UPDATE_TRANSACTION,
            ValidationLifecycleState::stale_non_claimable("partial_db"),
            "partial DB is a recovery state, not claimable source-code proof",
        );
        assert_ne!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::StaleOrNonClaimableDb
        );
    }

    #[test]
    fn corrupt_incomplete_update_blocks_validation() {
        let finding = lifecycle_integrity_finding(
            CG_MVP3_CORRUPT_OR_INCOMPLETE_UPDATE_TRANSACTION,
            ValidationLifecycleState::claimable_current(),
            "incomplete update transaction was presented as claimable",
        );
        assert_eq!(finding.classification, ValidationClassification::Block);
    }

    #[test]
    fn query_context_during_update_safe() {
        let finding = lifecycle_integrity_finding(
            CG_MVP3_QUERY_DURING_UPDATE_UNSAFE,
            ValidationLifecycleState::claimable_current(),
            "query/context attempted to serve active update DB as claimable",
        );
        assert_eq!(finding.classification, ValidationClassification::Block);

        let stale = lifecycle_integrity_finding(
            CG_MVP3_QUERY_DURING_UPDATE_UNSAFE,
            ValidationLifecycleState::stale_non_claimable("publish_state_updating"),
            "query/context during update is non-claimable",
        );
        assert_ne!(stale.classification, ValidationClassification::Block);
    }

    #[test]
    fn validate_edit_no_fake_interrupt_from_dirty_state() {
        let finding = lifecycle_integrity_finding(
            CG_MVP3_QUERY_DURING_UPDATE_UNSAFE,
            ValidationLifecycleState::stale_non_claimable("publishing_dirty"),
            "dirty or publishing DB state must be recovery-only",
        );
        assert_ne!(finding.classification, ValidationClassification::Block);

        let packet = ValidationPacket::new(
            vec!["src/service.ts".to_string()],
            json!({"status": "not_run", "reason": "dirty_state"}),
            vec![finding],
            vec![proof_integrity_rule(CG_MVP3_QUERY_DURING_UPDATE_UNSAFE)],
            Vec::new(),
            json!({"claimable": false, "current": false, "db_problem_kind": "publishing_dirty"}),
            json!({"graph_relation_proof": {"stale": true, "graph_proof": true}}),
            json!({"claimable": false, "current": false}),
        )
        .with_eligible_hard_interrupts("mvp3_7_dirty_state_test");
        assert!(!packet.hard_interrupt_available);
        assert!(packet.hard_interrupt.is_none());
    }

    #[test]
    fn sidecar_access_vs_corrupt_classification_safe() {
        let finding = lifecycle_integrity_finding(
            CG_MVP3_SIDECAR_CORRUPT_VS_INACCESSIBLE_MISCLASSIFIED,
            ValidationLifecycleState::claimable_current(),
            "sidecar access failure was mislabeled as corrupt",
        );
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::ReverifiedGraphIntegrity
        );
    }

    #[test]
    fn sidecar_failure_does_not_corrupt_graph_claimability() {
        let mut packet = dirty_output_source_packet();
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-use.status", false);
        assert_eq!(
            packet["sidecar_statuses"]["graph_db_status"].as_str(),
            Some("fresh")
        );
        assert_eq!(
            packet["sidecar_statuses"]["candidate_spool_query_index_status"].as_str(),
            Some("corrupt")
        );
        assert_eq!(
            packet["claimability_effect"].as_str(),
            Some("graph_proof_available")
        );
        assert_eq!(
            packet["sidecar_degradation_kind"].as_str(),
            Some("candidate_layer_only")
        );
        assert_eq!(
            packet["graph_validation_unaffected_by_optional_sidecars"].as_bool(),
            Some(true)
        );
        assert_eq!(
            packet["optional_sidecar_staleness_affects_graph_proof"].as_bool(),
            Some(false)
        );
    }

    #[test]
    fn recovery_commands_actionable() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let recovery = agent_use_recovery_json(&profile);
        for key in [
            "agent_use_index_command",
            "agent_use_status_command",
            "agent_use_context_pack_command",
            "agent_use_watch_once_command",
        ] {
            let command = recovery[key].as_str().expect("recovery command string");
            assert!(command.contains(BIN_NAME), "{key}: {command}");
            assert!(command.contains("--repo"), "{key}: {command}");
            assert!(
                command.contains(&path_string(&profile.repo_root)),
                "{key}: {command}"
            );
        }
        cleanup_repo(repo);
    }

    #[test]
    fn status_doctor_read_only() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let normal_dot_codegraph = repo.join(".codegraph");
        assert!(!normal_dot_codegraph.exists());

        let status_like = agent_use_persistent_watch_many_changes_json(
            &profile,
            &[PathBuf::from("src/one.ts"), PathBuf::from("src/two.ts")],
            &AgentUseWatchOptions {
                repo: repo.clone(),
                once: false,
                changed_paths: Vec::new(),
                debounce: Duration::from_millis(10),
                lock_retries: 0,
                lock_retry: Duration::from_millis(1),
                max_batch_paths: 1,
                max_updates: None,
                idle_timeout: None,
                test_events: Vec::new(),
                detail_mode: AgentUseDetailMode::Compact,
            },
        );
        assert_eq!(status_like["auto_index_enabled"].as_bool(), Some(false));
        assert_eq!(status_like["old_db_preserved"].as_bool(), Some(true));
        assert_eq!(status_like["temp_db_claimable"].as_bool(), Some(false));
        assert!(!normal_dot_codegraph.exists());
        cleanup_repo(repo);
    }

    #[test]
    fn diagnostic_only_not_graph_proof() {
        let rule = proof_integrity_rule(CG_MVP3_PROOF_EDGE_MISSING_SOURCE_SPAN);
        let mut input = ValidationReverificationInput::exact_graph_source(
            ValidationLifecycleState::claimable_current(),
            "evidence://text",
            "text evidence cannot prove graph proof integrity",
        );
        input.graph_source_relation_reverified = false;
        input.integrity_condition_reverified = true;
        input.integrity_issue_present = true;
        input.evidence_items = vec![ValidationEvidenceItem::non_graph(
            ValidationEvidenceKind::TextEvidence,
            "evidence://text",
            "text evidence is not graph proof",
        )];
        let finding = classify_validation_finding(&rule, "finding://diagnostic-only", input);
        assert_ne!(finding.classification, ValidationClassification::Block);
        assert!(!finding.reverified_graph_source_proof);
        assert_eq!(finding.proof_status, ValidationProofStatus::NotGraphProof);
    }

    fn dirty_output_graph_layer(status: &str, ready: bool) -> Value {
        json!({
            "layer": "graph_db",
            "status": status,
            "ready": ready,
            "claimable": ready,
            "graph_proof_available": ready,
            "diagnostic_only": !ready,
        })
    }

    fn dirty_output_candidate_layer(status: &str, query_index_status: &str) -> Value {
        json!({
            "layer": "candidate_spool",
            "status": status,
            "ready": matches!(status, "ready" | "bounded_ready" | "truncated_ready")
                && query_index_status == "ready",
            "candidate_only": true,
            "graph_proof": false,
            "path": "candidate.jsonl",
            "query_index_status": query_index_status,
            "query_index_path": "candidate.sqlite",
            "reason": "test candidate sidecar status",
        })
    }

    fn dirty_output_vector_layer(status: &str) -> Value {
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

    fn dirty_output_audit_layer(status: &str) -> Value {
        json!({
            "layer": "vector_audit",
            "status": status,
            "ready": matches!(status, "ready" | "diagnostic_only" | "stale"),
            "diagnostic_only": true,
            "runtime_dependency": false,
            "path": "vector-audit.json",
        })
    }

    fn dirty_output_staged(candidate_status: &str, query_index_status: &str) -> Value {
        staged_availability_from_layers(
            dirty_output_graph_layer("ready", true),
            dirty_output_candidate_layer(candidate_status, query_index_status),
            dirty_output_vector_layer("stale"),
            dirty_output_audit_layer("missing"),
        )
    }

    fn dirty_output_source_packet() -> Value {
        json!({
            "staged_availability": dirty_output_staged("query_index_corrupt", "corrupt"),
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
    fn validate_edit_reports_dirty_evidence() {
        let repo = test_repo();
        let profile = test_profile(&repo);
        let options = validate_edit_test_options(&repo, AgentUseDetailMode::Compact, false);
        let mut source_update = validate_edit_non_interrupt_source_update();
        let dirty = dirty_output_source_packet();
        if let Some(object) = source_update.as_object_mut() {
            object.extend(dirty.as_object().expect("dirty object").clone());
        }
        let packet = agent_use_validate_edit_packet_json(&profile, &options, source_update);
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
            packet["severity_effect"]["severity_model_preserved"].as_bool(),
            Some(true)
        );
        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(false));
        cleanup_repo(repo);
    }

    #[test]
    fn watch_reports_dirty_evidence() {
        let mut packet = dirty_output_source_packet();
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-use.watch.once", false);
        assert_eq!(
            packet["dirty_evidence_summary"]["surface"].as_str(),
            Some("agent-use.watch.once")
        );
        assert!(packet["invalidated_evidence"]
            .as_array()
            .expect("invalidated")
            .iter()
            .any(|item| item["surface_name"].as_str() == Some("text_evidence")));
        assert!(packet["stale_evidence"]
            .as_array()
            .expect("stale")
            .iter()
            .all(|item| item["graph_proof"].as_bool() != Some(true)
                || item["proof_ladder_level"].as_str() == Some("graph_relation_proof")));
        assert_eq!(packet["graph_validation_status"].as_str(), Some("ok"));
        assert_eq!(packet["agent_action"].as_str(), Some("continue"));
        assert_eq!(packet["candidate_recall_status"].as_str(), Some("degraded"));
        assert_eq!(
            packet["candidate_recall_action"].as_str(),
            Some("refresh_sidecars_if_candidate_recall_needed")
        );
    }

    #[test]
    fn context_pack_refuses_stale_proof() {
        let staged = staged_availability_from_layers(
            dirty_output_graph_layer("stale", false),
            dirty_output_candidate_layer("no_spool", "index_missing"),
            dirty_output_vector_layer("missing"),
            dirty_output_audit_layer("missing"),
        );
        let mut packet = json!({
            "staged_availability": staged,
            "proof_ladder_changes": {},
            "recovery_commands": [],
        });
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-use.context-pack", false);
        assert_eq!(
            packet["claimability_effect"].as_str(),
            Some("graph_proof_unavailable")
        );
        assert!(packet["stale_evidence"]
            .as_array()
            .expect("stale")
            .iter()
            .any(|item| item["surface_name"].as_str() == Some("graph_relation_proof")));
        assert_eq!(packet["graph_validation_status"].as_str(), Some("unknown"));
        assert_eq!(packet["agent_action"].as_str(), Some("refresh_index"));
        assert_eq!(
            packet["graph_validation_unaffected_by_optional_sidecars"].as_bool(),
            Some(false)
        );
    }

    #[test]
    fn context_pack_refuses_stale_candidate_as_fresh() {
        let mut packet = dirty_output_source_packet();
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-use.context-pack", false);
        assert_eq!(packet["candidate_layer_status"].as_str(), Some("corrupt"));
        assert!(packet["stale_non_proof_reasons"]
            .as_array()
            .expect("reasons")
            .iter()
            .any(|reason| reason
                .as_str()
                .is_some_and(|text| text.contains("cannot be used as graph proof"))));
        assert_eq!(packet["candidate_recall_status"].as_str(), Some("degraded"));
        assert_eq!(
            packet["sidecar_degradation_kind"].as_str(),
            Some("candidate_layer_only")
        );
        assert_eq!(
            packet["graph_validation_unaffected_by_optional_sidecars"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn clean_graph_validation_not_top_level_degraded_by_optional_sidecars() {
        let mut packet = dirty_output_source_packet();
        packet["status"] = json!("ok");
        packet["validation_packet"] = json!({
            "status": "ok",
            "hard_interrupt_available": false,
            "warnings": [],
            "unknowns": [],
            "diagnostics": [{"validation_rule_id": "CG_MVP3_SIDECAR_STALE", "severity": "diagnostic"}],
        });
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-use.validate-edit", false);
        assert_eq!(packet["status"].as_str(), Some("ok"));
        assert_eq!(packet["graph_validation_status"].as_str(), Some("ok"));
        assert_eq!(packet["agent_action"].as_str(), Some("continue"));
        assert_eq!(packet["candidate_recall_status"].as_str(), Some("degraded"));
        assert_eq!(
            packet["graph_validation_unaffected_by_optional_sidecars"].as_bool(),
            Some(true)
        );
        assert_eq!(
            packet["severity_effect"]["top_level_status_not_degraded_by_optional_sidecars"]
                .as_bool(),
            Some(true)
        );
    }

    #[test]
    fn context_pack_graph_proof_unaffected_by_optional_sidecar_stale() {
        let mut packet = dirty_output_source_packet();
        packet["status"] = json!("ok");
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-use.context-pack", false);
        assert_eq!(packet["status"].as_str(), Some("ok"));
        assert_eq!(
            packet["claimability_effect"].as_str(),
            Some("graph_proof_available")
        );
        assert_eq!(
            packet["graph_validation_unaffected_by_optional_sidecars"].as_bool(),
            Some(true)
        );
        assert_eq!(
            packet["optional_sidecar_staleness_affects_graph_proof"].as_bool(),
            Some(false)
        );
        assert_eq!(
            packet["candidate_recall_action"].as_str(),
            Some("refresh_sidecars_if_candidate_recall_needed")
        );
    }

    #[test]
    fn routing_packet_marks_stale_candidate_layers_or_not_applicable() {
        let mut packet = json!({
            "layer_readiness": dirty_output_staged("no_spool", "index_missing")["layer_readiness"].clone(),
            "graph_proof_available": true,
            "candidate_only_available": false,
            "routing_handles_invalidated_or_not_applicable": {"action": "not_applicable", "graph_proof": false},
        });
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-routing-packet", false);
        assert_eq!(
            packet["routing_handle_status"].as_str(),
            Some("not_applicable")
        );
        assert_eq!(
            packet["sidecar_statuses"]["source_navigation_status"].as_str(),
            Some("not_applicable")
        );
    }

    #[test]
    fn status_doctor_graph_sidecar_split() {
        let mut packet = dirty_output_source_packet();
        add_agent_use_dirty_evidence_output_fields(&mut packet, "status", false);
        assert_eq!(
            packet["sidecar_statuses"]["graph_db_status"].as_str(),
            Some("fresh")
        );
        assert_eq!(
            packet["sidecar_statuses"]["candidate_layer_status"].as_str(),
            Some("corrupt")
        );
        assert_eq!(
            packet["claimability_effect"].as_str(),
            Some("graph_proof_available")
        );
        assert_eq!(packet["candidate_recall_status"].as_str(), Some("degraded"));
        assert_eq!(packet["agent_action"].as_str(), Some("continue"));
    }

    #[test]
    fn proof_ladder_changes_explicit() {
        let counts = agent_use_proof_ladder_change_counts(
            &dirty_output_source_packet()["proof_ladder_changes"],
        );
        assert_eq!(counts["text_evidence"].as_u64(), Some(1));
        assert_eq!(counts["candidate_evidence"].as_u64(), Some(2));
        assert_eq!(counts["source_navigation_evidence"].as_u64(), Some(1));
        assert_eq!(counts["graph_proof_changed_count"].as_u64(), Some(0));
    }

    #[test]
    fn text_evidence_change_not_broken_graph_behavior() {
        let mut packet = dirty_output_source_packet();
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-use.validate-edit", false);
        let text = packet["invalidated_evidence"]
            .as_array()
            .expect("invalidated")
            .iter()
            .find(|item| item["surface_name"].as_str() == Some("text_evidence"))
            .expect("text evidence entry");
        assert_eq!(text["graph_proof"].as_bool(), Some(false));
        assert_eq!(
            packet["severity_effect"]["text_evidence_change_not_broken_graph_behavior"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn stale_sidecar_not_hard_interrupt() {
        let mut packet = dirty_output_source_packet();
        packet["hard_interrupt_available"] = json!(false);
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-use.validate-edit", false);
        assert_eq!(
            packet["severity_effect"]["stale_sidecar_not_hard_interrupt"].as_bool(),
            Some(true)
        );
        assert_eq!(packet["hard_interrupt_available"].as_bool(), Some(false));
    }

    #[test]
    fn severity_model_preserved() {
        let mut packet = dirty_output_source_packet();
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-use.validate-edit", false);
        assert_eq!(
            packet["severity_effect"]["severity_model_preserved"].as_bool(),
            Some(true)
        );
        assert_eq!(
            packet["severity_effect"]["hard_interrupt_eligibility_unchanged"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn compact_output_preserves_dirty_evidence_summary() {
        let mut packet = dirty_output_source_packet();
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-use.context-pack", false);
        assert!(packet["dirty_evidence_summary"].is_object());
        assert_eq!(
            packet["dirty_evidence_summary"]["summary_only"].as_bool(),
            Some(true)
        );
        assert!(packet["expansion_handles"]
            .as_array()
            .expect("handles")
            .iter()
            .any(|handle| handle.as_str() == Some("dirty_evidence:full")));
    }

    #[test]
    fn explain_audit_restores_dirty_evidence_detail() {
        let mut packet = dirty_output_source_packet();
        add_agent_use_dirty_evidence_output_fields(&mut packet, "agent-use.context-pack", true);
        assert_eq!(
            packet["dirty_evidence_summary"]["summary_only"].as_bool(),
            Some(false)
        );
        assert!(!packet
            .get("expansion_handles")
            .and_then(Value::as_array)
            .is_some_and(|handles| handles
                .iter()
                .any(|handle| handle.as_str() == Some("dirty_evidence:full"))));
    }

    #[test]
    fn no_dot_codegraph_mutation() {
        let repo = test_repo();
        write_source(&repo, "src/caller.ts", "missingTarget();\n");
        let store = test_store(&repo);
        let caller = function_entity("src/caller.ts", "caller", 1);
        store.upsert_entity(&caller).expect("caller");
        let edge = calls_edge(
            &caller,
            "entity://missing",
            SourceSpan::with_columns("src/caller.ts", 1, 1, 1, 16),
        );
        let normal_dot_codegraph = repo.join(".codegraph");
        assert!(!normal_dot_codegraph.exists());

        let _findings = run_edge_validation(
            &repo,
            &store,
            &edge,
            ValidationLifecycleState::claimable_current(),
        );

        assert!(!normal_dot_codegraph.exists());
        drop(store);
        cleanup_repo(repo);
    }
}

pub(crate) fn run_agent_use_persistent_watch_command(
    options: AgentUseWatchOptions,
) -> Result<Value, String> {
    let profile = resolve_agent_use_profile(&options.repo)?;
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let normal_dot_codegraph_existed_before = normal_dot_codegraph.exists();
    if let Some(problem) = agent_use_profile_root_access_problem(&profile) {
        return Ok(agent_use_profile_access_unavailable_json(
            &profile,
            "watch",
            Some("persistent"),
            &problem,
            normal_dot_codegraph_existed_before,
        ));
    }
    let started = Instant::now();
    let mut last_activity = started;
    let mut preflight_lock_retries = 0usize;
    let preflight = loop {
        let preflight =
            inspect_db_lifecycle_surface_preflight(DbLifecycleSurfacePreflightRequest {
                repo_root: profile.repo_root.clone(),
                db_path: profile.db_path.clone(),
                surface_name: "agent-use.watch.persistent".to_string(),
                operation_kind: DbLifecycleOperationKind::WriteUpdate,
                allow_stale_read: false,
                allow_foreign_repo: false,
                required_storage_mode: None,
                expected_scope: Some(profile.scope_policy.clone()),
            })
            .map_err(|error| error.to_string())?;
        if preflight.safe_to_write
            || !agent_use_preflight_is_transient_lock(&preflight)
            || preflight_lock_retries >= options.lock_retries
        {
            break preflight;
        }
        preflight_lock_retries += 1;
        std::thread::sleep(options.lock_retry);
    };

    if !preflight.safe_to_write {
        return Ok(agent_use_persistent_watch_unavailable_json(
            &profile,
            &preflight,
            normal_dot_codegraph_existed_before,
            &options,
            started,
            preflight_lock_retries,
        ));
    }

    let use_synthetic_events = !options.test_events.is_empty();
    let (sender, receiver) = mpsc::channel();
    let _watcher_guard = if use_synthetic_events {
        None
    } else {
        let mut watcher = notify::recommended_watcher(move |result| {
            let _ = sender.send(result);
        })
        .map_err(|error| format!("watcher init failed: {error}"))?;
        watcher
            .watch(&profile.repo_root, RecursiveMode::Recursive)
            .map_err(|error| format!("watcher start failed: {error}"))?;
        Some(watcher)
    };

    let mut debouncer = WatchDebouncer::new(options.debounce);
    if use_synthetic_events {
        let now = Instant::now();
        for path in &options.test_events {
            debouncer.push(path.clone(), now);
        }
        last_activity = now;
    }
    let tick = if options.debounce.is_zero() {
        Duration::from_millis(50)
    } else {
        std::cmp::min(options.debounce, Duration::from_millis(250))
    };
    let mut batches_processed = 0usize;
    let mut updates_attempted = 0usize;
    let mut updates_succeeded = 0usize;
    let mut lock_retry_count = preflight_lock_retries;
    let mut last_update_summary = Value::Null;
    let mut last_error = Value::Null;
    let mut last_update_state = "ready".to_string();
    let mut last_successful_update_time = Value::Null;

    let stop_reason = loop {
        if !use_synthetic_events {
            match receiver.recv_timeout(tick) {
                Ok(Ok(event)) => {
                    enqueue_event_paths(&mut debouncer, event, Instant::now());
                    last_activity = Instant::now();
                }
                Ok(Err(error)) => {
                    last_error = json!({
                        "status": "watcher_error",
                        "message": error.to_string(),
                        "retryable": true,
                    });
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    last_error = json!({
                        "status": "watcher_error",
                        "message": "watcher event channel disconnected",
                        "retryable": false,
                    });
                    break "event_channel_disconnected".to_string();
                }
            }
        } else {
            std::thread::sleep(tick);
        }

        let ready = debouncer.ready(Instant::now());
        if !ready.is_empty() {
            batches_processed += 1;
            updates_attempted += 1;
            last_activity = Instant::now();
            if ready.len() > options.max_batch_paths {
                let degraded =
                    agent_use_persistent_watch_many_changes_json(&profile, &ready, &options);
                last_update_state = "degraded".to_string();
                last_error = degraded.clone();
                last_update_summary = degraded;
            } else {
                match agent_use_watch_once_delta_with_lock_retry(
                    &profile,
                    ready,
                    options.detail_mode,
                    normal_dot_codegraph_existed_before,
                    options.lock_retries,
                    options.lock_retry,
                ) {
                    Ok((value, retries)) => {
                        lock_retry_count += retries;
                        last_update_state = value
                            .get("delta_state")
                            .and_then(Value::as_str)
                            .or_else(|| value.get("status").and_then(Value::as_str))
                            .unwrap_or("updated")
                            .to_string();
                        updates_succeeded += 1;
                        last_successful_update_time = json!(unix_time_ms());
                        last_error = Value::Null;
                        last_update_summary = value;
                    }
                    Err(error) => {
                        let retryable_lock = agent_use_watch_error_is_transient_lock(&error);
                        last_update_state = if retryable_lock {
                            "locked".to_string()
                        } else {
                            "blocked".to_string()
                        };
                        last_error = json!({
                            "status": if retryable_lock { "db_locked" } else { "error" },
                            "message": error,
                            "retryable": retryable_lock,
                        });
                        last_update_summary = Value::Null;
                    }
                }
            }
        }

        if options
            .max_updates
            .is_some_and(|max_updates| updates_attempted >= max_updates)
        {
            break "max_updates_reached".to_string();
        }
        if let Some(idle_timeout) = options.idle_timeout {
            let idle_elapsed = Instant::now()
                .checked_duration_since(last_activity)
                .unwrap_or_default();
            if debouncer.pending_len() == 0 && idle_elapsed >= idle_timeout {
                break "idle_timeout".to_string();
            }
        }
    };

    Ok(agent_use_persistent_watch_status_json(
        &profile,
        &options,
        normal_dot_codegraph_existed_before,
        started,
        debouncer.pending_len(),
        debouncer.pending_paths(),
        debouncer.events_seen(),
        debouncer.coalesced_count(),
        batches_processed,
        updates_attempted,
        updates_succeeded,
        lock_retry_count,
        last_update_state,
        last_update_summary,
        last_successful_update_time,
        last_error,
        stop_reason,
    ))
}

pub(crate) fn agent_use_watch_once_delta_with_lock_retry(
    profile: &AgentUseProfile,
    changed_paths: Vec<PathBuf>,
    detail_mode: AgentUseDetailMode,
    normal_dot_codegraph_existed_before: bool,
    lock_retries: usize,
    lock_retry: Duration,
) -> Result<(Value, usize), String> {
    agent_use_retry_transient_lock(lock_retries, lock_retry, || {
        run_agent_use_watch_once_delta(
            profile,
            changed_paths.clone(),
            detail_mode,
            normal_dot_codegraph_existed_before,
            None,
        )
    })
}

pub(crate) fn agent_use_retry_transient_lock<T, F>(
    lock_retries: usize,
    lock_retry: Duration,
    mut operation: F,
) -> Result<(T, usize), String>
where
    F: FnMut() -> Result<T, String>,
{
    let mut retries = 0usize;
    loop {
        match operation() {
            Ok(value) => return Ok((value, retries)),
            Err(error)
                if agent_use_watch_error_is_transient_lock(&error) && retries < lock_retries =>
            {
                retries += 1;
                std::thread::sleep(lock_retry);
            }
            Err(error) => return Err(error),
        }
    }
}

pub(crate) fn agent_use_watch_error_is_transient_lock(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("database is locked")
        || lower.contains("database table is locked")
        || lower.contains("db_locked")
        || lower.contains("sqlite_busy")
}

pub(crate) fn agent_use_preflight_is_transient_lock(
    preflight: &DbLifecycleSurfacePreflight,
) -> bool {
    preflight
        .blockers
        .iter()
        .chain(preflight.warnings.iter())
        .any(|message| agent_use_watch_error_is_transient_lock(message))
}

pub(crate) fn agent_use_persistent_watch_many_changes_json(
    profile: &AgentUseProfile,
    changed_paths: &[PathBuf],
    options: &AgentUseWatchOptions,
) -> Value {
    let normalized = agent_use_report_changed_paths(&profile.repo_root, changed_paths);
    json!({
        "status": "degraded",
        "delta_state": "degraded",
        "reason": "too_many_changes_branch_switch_suspected",
        "changed_paths": normalized,
        "changed_path_count": changed_paths.len(),
        "max_batch_paths": options.max_batch_paths,
        "no_full_repo_fallback": true,
        "manual_full_index_recommendation": format!("{BIN_NAME} agent-use index --repo \"{}\" --json", path_string(&profile.repo_root)),
        "claimable": false,
        "diagnostic_only": true,
        "auto_index_enabled": false,
        "old_db_preserved": true,
        "temp_db_claimable": false,
    })
}

pub(crate) fn agent_use_persistent_watch_unavailable_json(
    profile: &AgentUseProfile,
    preflight: &DbLifecycleSurfacePreflight,
    normal_dot_codegraph_existed_before: bool,
    options: &AgentUseWatchOptions,
    started: Instant,
    lock_retry_count: usize,
) -> Value {
    let lifecycle = watch_lifecycle_status_json(preflight, &profile.db_path, false);
    let status = agent_use_blocked_status_from_preflight(preflight);
    let last_error = json!({
        "status": status,
        "message": watch_lifecycle_error(preflight),
        "blockers": preflight.blockers.clone(),
        "retryable": status == "db_locked",
    });
    agent_use_persistent_watch_status_json(
        profile,
        options,
        normal_dot_codegraph_existed_before,
        started,
        0,
        Vec::new(),
        0,
        0,
        0,
        lock_retry_count,
        0,
        0,
        "blocked".to_string(),
        lifecycle,
        Value::Null,
        last_error,
        status.to_string(),
    )
}

pub(crate) fn agent_use_blocked_status_from_preflight(
    preflight: &DbLifecycleSurfacePreflight,
) -> &str {
    if preflight.artifact_freshness.as_deref() == Some("missing")
        || preflight.passport_status == "missing"
    {
        "not_indexed"
    } else if preflight.schema_status != "ok" {
        "schema_mismatch"
    } else if !preflight.repo_match {
        "repo_mismatch"
    } else if preflight
        .blockers
        .iter()
        .any(|blocker| blocker.to_ascii_lowercase().contains("locked"))
    {
        "db_locked"
    } else {
        "blocked"
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn agent_use_persistent_watch_status_json(
    profile: &AgentUseProfile,
    options: &AgentUseWatchOptions,
    normal_dot_codegraph_existed_before: bool,
    started: Instant,
    queue_depth: usize,
    pending_paths: Vec<PathBuf>,
    events_seen: usize,
    coalesced_count: usize,
    batches_processed: usize,
    updates_attempted: usize,
    updates_succeeded: usize,
    lock_retry_count: usize,
    last_update_state: String,
    last_update_summary: Value,
    last_successful_update_time: Value,
    last_error: Value,
    stop_reason: String,
) -> Value {
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let pending_paths = agent_use_report_changed_paths(&profile.repo_root, &pending_paths);
    let claimability = last_update_summary
        .get("claimability")
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "claimable": last_error.is_null(),
                "diagnostic_only": !last_error.is_null(),
                "candidate_only": false,
                "graph_proof_available": last_error.is_null(),
            })
        });
    let status = if last_error.is_null() {
        "stopped".to_string()
    } else {
        last_error
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("blocked")
            .to_string()
    };
    let publish_state = agent_use_publish_state_json(profile);
    let publish_active = publish_state
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let publish_status = publish_state
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("absent");
    let blocked_labels = last_error
        .get("blockers")
        .and_then(Value::as_array)
        .map(|blockers| {
            blockers
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut retryable_labels = Vec::new();
    if last_error
        .get("retryable")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        retryable_labels.push(status.clone());
    }
    if publish_active && matches!(publish_status, "updating" | "publishing") {
        retryable_labels.push("wait_for_current_update".to_string());
    }
    let lock_state = agent_use_profile_lock_state_json(
        profile,
        publish_active,
        publish_status,
        &blocked_labels,
        &retryable_labels,
        Some(lock_retry_count),
    );
    let update_queue_state = agent_use_profile_update_queue_state_json(
        publish_active,
        publish_status,
        last_update_summary.clone(),
        json!(pending_paths.clone()),
        queue_depth,
        updates_attempted,
        updates_succeeded,
        last_error.clone(),
    );
    json!({
        "status": status,
        "command": "watch",
        "subcommand": "persistent",
        "command_namespace": "agent-use",
        "agent_use_command": "agent-use watch",
        "watch_mode": "persistent",
        "agent_use_watch_available": true,
        "agent_use_watch_status": "implemented_persistent_scheduler",
        "persistent_watch_scheduler": true,
        "uses_once_delta_engine": true,
        "filesystem_watcher": if options.test_events.is_empty() { "notify" } else { "synthetic_test_event_source" },
        "test_event_count": options.test_events.len(),
        "writer_queue_serialized": true,
        "max_concurrent_writers": 1,
        "auto_index_enabled": false,
        "repo": path_string(&profile.repo_root),
        "repo_root": path_string(&profile.repo_root),
        "db": path_string(&profile.db_path),
        "db_path": path_string(&profile.db_path),
        "resolved_db": path_string(&profile.db_path),
        "db_source": "agent-use profile",
        "external_db_used": true,
        "profile_name": profile.profile_name.clone(),
        "debounce_ms": options.debounce.as_millis() as u64,
        "lock_retries": options.lock_retries,
        "lock_retry_ms": options.lock_retry.as_millis() as u64,
        "lock_retry_count": lock_retry_count,
        "max_batch_paths": options.max_batch_paths,
        "queue_depth": queue_depth,
        "pending_paths": pending_paths,
        "coalesced_count": coalesced_count,
        "events_seen": events_seen,
        "batches_processed": batches_processed,
        "updates_attempted": updates_attempted,
        "updates_succeeded": updates_succeeded,
        "last_update_state": last_update_state,
        "last_update_summary": last_update_summary,
        "lock_state": lock_state,
        "update_queue_state": update_queue_state,
        "last_successful_update_time_unix_ms": last_successful_update_time,
        "last_error": last_error,
        "recovery_commands": profile.recovery_commands.clone(),
        "recovery": agent_use_recovery_json(profile),
        "claimability": claimability,
        "old_db_preserved": true,
        "temp_db_claimable": false,
        "pause_state": "not_paused",
        "resume_supported": false,
        "stop_reason": stop_reason,
        "elapsed_ms": started.elapsed().as_millis() as u64,
        "normal_dot_codegraph_path": path_string(&normal_dot_codegraph),
        "normal_dot_codegraph_created": !normal_dot_codegraph_existed_before && normal_dot_codegraph.exists(),
        "normal_dot_codegraph_mutated": normal_dot_codegraph_existed_before != normal_dot_codegraph.exists(),
        "public_claim": false,
    })
}

pub(crate) fn agent_use_report_changed_paths(
    repo_root: &Path,
    changed_paths: &[PathBuf],
) -> Vec<String> {
    let mut normalized = changed_paths
        .iter()
        .map(|path| agent_use_report_changed_path(repo_root, path))
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

pub(crate) fn agent_use_report_changed_path(repo_root: &Path, changed_path: &Path) -> String {
    let repo_root_canonical =
        fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let candidate = if changed_path.is_absolute() {
        changed_path.to_path_buf()
    } else {
        repo_root.join(changed_path)
    };
    let candidate_canonical = fs::canonicalize(&candidate).unwrap_or(candidate);
    if let Ok(relative) = candidate_canonical.strip_prefix(&repo_root_canonical) {
        return path_to_graph_report_string(relative);
    }
    if let Ok(relative) = candidate_canonical.strip_prefix(repo_root) {
        return path_to_graph_report_string(relative);
    }
    path_to_graph_report_string(changed_path)
}

pub(crate) fn path_to_graph_report_string(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

pub(crate) fn agent_use_watch_path_preflight(
    repo_root: &Path,
    changed_paths: &[PathBuf],
    scope_policy: &IndexScopeOptions,
) -> ValidateEditChangedFilesPreflight {
    validate_edit_changed_files_preflight_with_scope(
        repo_root,
        changed_paths,
        scope_policy,
        AGENT_USE_WATCH_DEFAULT_MAX_BATCH_PATHS,
    )
}

pub(crate) fn agent_use_watch_rejected_paths_json(
    profile: &AgentUseProfile,
    preflight: &DbLifecycleSurfacePreflight,
    normal_dot_codegraph_existed_before: bool,
    path_preflight: &ValidateEditChangedFilesPreflight,
) -> Value {
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let lifecycle = watch_lifecycle_status_json(preflight, &profile.db_path, false);
    let db_lifecycle_read =
        db_lifecycle_preflight_json(&preflight.lifecycle_preflight, true, false, false);
    let staged_availability = staged_availability_for_cli(
        &profile.repo_root,
        &profile.db_path,
        Some(&preflight.lifecycle_preflight),
        Some(&profile.candidate_spool_path),
        Some(&profile.vector_runtime_path),
        Some(&profile.vector_audit_path),
        None,
    );
    let rejected = !path_preflight.rejected_paths.is_empty();
    let status = if rejected { "rejected" } else { "no_op" };
    let reason = agent_use_watch_preflight_no_update_reason(path_preflight);
    let delta_state = if rejected { "blocked" } else { "ready" };
    let publish_strategy = if rejected {
        "no_update_when_changed_path_preflight_has_only_rejected_inputs"
    } else {
        "no_update_when_changed_path_preflight_has_only_noop_inputs"
    };
    let mut warnings = preflight.warnings.clone();
    warnings.extend(path_preflight.warnings.clone());
    warnings.sort();
    warnings.dedup();
    let mut value = json!({
        "status": status,
        "command": "watch",
        "subcommand": "once",
        "command_namespace": "agent-use",
        "agent_use_command": "agent-use watch",
        "profile_name": profile.profile_name.clone(),
        "repo": path_string(&profile.repo_root),
        "repo_root": path_string(&profile.repo_root),
        "repo_identity_label": profile.repo_identity_label.clone(),
        "repo_identity_hash": profile.repo_identity_hash.clone(),
        "repo_identity_short_hash": agent_use_repo_identity_short_hash(profile),
        "db": path_string(&profile.db_path),
        "db_path": path_string(&profile.db_path),
        "resolved_db": path_string(&profile.db_path),
        "db_source": "agent-use profile",
        "external_db_used": true,
        "claimable": false,
        "diagnostic_only": true,
        "reason": reason,
        "path_access_status": preflight.path_access_status.clone(),
        "path_access_error": preflight.path_access_error.clone(),
        "db_problem_kind": preflight.db_problem_kind.clone(),
        "watch_mode": "once_changed",
        "watch_db": lifecycle.clone(),
        "db_lifecycle_read": db_lifecycle_read,
        "lifecycle": lifecycle,
        "agent_use_watch_available": true,
        "agent_use_watch_status": "implemented_once_changed",
        "delta_sync_phase": "real_time_delta_sync",
        "delta_sync_state": delta_state,
        "delta_state": delta_state,
        "auto_index_enabled": false,
        "validation_status": "not_applicable",
        "validation_must_fix_before_continuing": false,
        "validation_blocking_error_count": 0,
        "validation_warning_count": 0,
        "validation_unknown_count": 0,
        "hard_interrupt_available": false,
        "hard_interrupt": Value::Null,
        "hard_interrupt_not_implemented": false,
        "changed_paths": path_preflight.accepted_paths.clone(),
        "normalized_changed_files": path_preflight.normalized_changed_files.clone(),
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
        "changed_paths_requested": path_preflight.requested_paths.clone(),
        "old_graph_valid": preflight.safe_to_write,
        "new_graph_valid": preflight.safe_to_write,
        "old_db_preserved": true,
        "temp_db_claimable": false,
        "files_walked": 0,
        "files_read": 0,
        "files_hashed": 0,
        "files_parsed": 0,
        "facts_deleted": 0,
        "facts_inserted": 0,
        "entities_added": 0,
        "entities_removed": 0,
        "entities_changed": 0,
        "edges_added": 0,
        "edges_removed": 0,
        "edges_changed": 0,
        "source_spans_added": 0,
        "source_spans_removed": 0,
        "source_spans_changed": 0,
        "text_evidence_changed": false,
        "freshness": {
            "graph_db": "current",
            "files_facts": "not_applicable",
            "text_evidence": "not_applicable",
            "path_evidence": "not_applicable",
            "candidate_spool": "not_checked",
            "candidate_spool_query_index": "not_checked",
            "vector_runtime_sidecar": "not_checked",
            "routing_context_handles": "not_applicable",
        },
        "path_evidence_invalidated": {
            "action": "unchanged",
            "dirty_path_evidence_count": 0,
            "delta_count": 0,
            "graph_proof": false,
        },
        "candidate_spool_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "candidate_spool_invalidated_or_refreshed": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "candidate_query_index_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "candidate_query_index_invalidated_or_refreshed": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "vector_chunks_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "vector_chunks_invalidated": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "vector_runtime_status_changed": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "vector_audit_status_changed": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "nuance_tokens_invalidated": {
            "action": "not_applicable",
            "status": "not_applicable",
            "reason": "nuance_rescue_candidates_are_request_time_context_candidates",
            "graph_proof": false,
        },
        "routing_handles_invalidated": {
            "action": "unchanged",
            "scope": "none",
        },
        "routing_handles_invalidated_or_not_applicable": {
            "action": "unchanged",
            "scope": "none",
            "graph_proof": false,
        },
        "proof_ladder_changes": {},
        "closure_files_considered": [],
        "closure_budget_hit": false,
        "degraded_relation_classes": [],
        "timings": {},
        "claimability": {
            "claimable": false,
            "diagnostic_only": true,
            "candidate_only": false,
            "graph_proof_available": false,
        },
        "publish_state": agent_use_publish_state_json(profile),
        "publishing": agent_use_publish_state_active(profile),
        "safety_labels": agent_use_safety_labels(profile, &preflight.lifecycle_preflight, None),
        "staged_availability": staged_availability.clone(),
        "recovery": agent_use_recovery_json(profile),
        "recovery_commands": profile.recovery_commands.clone(),
        "errors": path_preflight.rejected_paths.clone(),
        "warnings": warnings,
        "normal_dot_codegraph_path": path_string(&normal_dot_codegraph),
        "normal_dot_codegraph_created": !normal_dot_codegraph_existed_before && normal_dot_codegraph.exists(),
        "normal_dot_codegraph_mutated": normal_dot_codegraph_existed_before != normal_dot_codegraph.exists(),
        "public_claim": false,
        "publish_safety": {
            "strategy": publish_strategy,
            "old_good_read_visibility": "no delta transaction started",
            "partial_update_claimability": "not_applicable",
            "temp_db_claimability": "not_applicable_for_once_delta_update",
            "temp_db_claimable": false,
            "auto_index_on_start": false,
        },
    });
    merge_json_object(
        &mut value,
        staged_availability_top_level_fields(&staged_availability),
    );
    add_agent_use_rtds_freshness_fields(
        &mut value,
        profile,
        &preflight.lifecycle_preflight,
        &staged_availability,
    );
    value
}

pub(crate) fn agent_use_watch_deleted_paths(summary: &IncrementalIndexSummary) -> Vec<String> {
    summary
        .path_cleanup_reasons
        .iter()
        .filter(|(_, reasons)| reasons.iter().any(|reason| reason == "deleted"))
        .map(|(path, _)| path.clone())
        .collect()
}

pub(crate) fn agent_use_watch_preflight_no_update_reason(
    path_preflight: &ValidateEditChangedFilesPreflight,
) -> &'static str {
    if path_preflight.outside_repo_only
        || (!path_preflight.rejected_paths.is_empty()
            && path_preflight
                .rejected_paths
                .iter()
                .all(|path| path.reason == "path_outside_repo"))
    {
        "one_or_more_changed_paths_are_outside_repo"
    } else if !path_preflight.rejected_paths.is_empty() {
        "no_updateable_changed_paths_after_input_preflight"
    } else if !path_preflight.atomic_temp_paths.is_empty()
        || !path_preflight.ignored_paths.is_empty()
        || !path_preflight.generated_paths.is_empty()
    {
        "ignored_path_no_graph_changes"
    } else {
        "changed_paths_are_noop_after_input_preflight"
    }
}

pub(crate) fn agent_use_watch_no_op_paths(summary: &IncrementalIndexSummary) -> Vec<String> {
    if !agent_use_watch_summary_has_fact_changes(summary)
        && (summary.files_metadata_unchanged > 0
            || summary.files_ignored > 0
            || summary.files_skipped > 0
            || summary.files_seen > 0)
    {
        summary.changed_files.clone()
    } else {
        Vec::new()
    }
}

pub(crate) fn agent_use_watch_summary_has_fact_changes(summary: &IncrementalIndexSummary) -> bool {
    summary.files_indexed > 0
        || summary.files_deleted > 0
        || summary.files_renamed > 0
        || summary.deleted_fact_files > 0
        || summary.deleted_file_facts_removed > 0
        || summary.stale_facts_deleted_for_ignored_paths > 0
        || summary.entities > 0
        || summary.edges > 0
        || summary.dirty_path_evidence_count > 0
}

pub(crate) fn agent_use_watch_status(
    summary: &IncrementalIndexSummary,
    no_op_paths: &[String],
) -> &'static str {
    if agent_use_watch_dependency_closure_degraded(summary) {
        "degraded"
    } else if !no_op_paths.is_empty() && !agent_use_watch_summary_has_fact_changes(summary) {
        "no_op"
    } else {
        "updated"
    }
}

pub(crate) fn agent_use_watch_delta_state(
    summary: &IncrementalIndexSummary,
    no_op_paths: &[String],
) -> &'static str {
    if agent_use_watch_dependency_closure_degraded(summary) {
        "degraded"
    } else if !no_op_paths.is_empty() && !agent_use_watch_summary_has_fact_changes(summary) {
        "ready"
    } else {
        "updated"
    }
}

fn graph_delta_freshness_layer_changed(delta: &FreshnessLayerDelta) -> bool {
    delta.action != "unchanged" || delta.old_status != delta.new_status
}

fn agent_use_graph_delta_freshness_delta_count(delta: &EntitySourceRoleDeltaReport) -> usize {
    delta.path_evidence_invalidated_count
        + delta.sidecar_freshness_changed_count
        + usize::from(graph_delta_freshness_layer_changed(
            &delta.candidate_spool_invalidated_or_refreshed,
        ))
        + usize::from(graph_delta_freshness_layer_changed(
            &delta.candidate_query_index_invalidated_or_refreshed,
        ))
        + usize::from(graph_delta_freshness_layer_changed(
            &delta.vector_chunks_invalidated,
        ))
        + usize::from(graph_delta_freshness_layer_changed(
            &delta.vector_runtime_status_changed,
        ))
        + usize::from(graph_delta_freshness_layer_changed(
            &delta.vector_audit_status_changed,
        ))
        + usize::from(graph_delta_freshness_layer_changed(
            &delta.nuance_tokens_invalidated,
        ))
        + usize::from(graph_delta_freshness_layer_changed(
            &delta.routing_handles_invalidated,
        ))
}

fn agent_use_graph_delta_omitted_count(delta: &EntitySourceRoleDeltaReport) -> usize {
    let omission = &delta.omission;
    omission.entities_added_omitted
        + omission.entities_removed_omitted
        + omission.entities_changed_omitted
        + omission.edges_added_omitted
        + omission.edges_removed_omitted
        + omission.edges_changed_omitted
        + omission.source_spans_added_omitted
        + omission.source_spans_removed_omitted
        + omission.source_spans_changed_omitted
        + omission.text_evidence_changed_omitted
        + omission.path_evidence_invalidated_omitted
        + omission.sidecar_freshness_changed_omitted
        + omission.source_roles_changed_omitted
        + omission.file_renames_detected_omitted
        + omission.rename_ambiguities_omitted
}

fn agent_use_graph_delta_truncated_sections(
    delta: &EntitySourceRoleDeltaReport,
) -> Vec<&'static str> {
    let omission = &delta.omission;
    let mut sections = Vec::new();
    if omission.entities_added_omitted > 0 {
        sections.push("entities_added");
    }
    if omission.entities_removed_omitted > 0 {
        sections.push("entities_removed");
    }
    if omission.entities_changed_omitted > 0 {
        sections.push("entities_changed");
    }
    if omission.edges_added_omitted > 0 {
        sections.push("edges_added");
    }
    if omission.edges_removed_omitted > 0 {
        sections.push("edges_removed");
    }
    if omission.edges_changed_omitted > 0 {
        sections.push("edges_changed");
    }
    if omission.source_spans_added_omitted > 0 {
        sections.push("source_spans_added");
    }
    if omission.source_spans_removed_omitted > 0 {
        sections.push("source_spans_removed");
    }
    if omission.source_spans_changed_omitted > 0 {
        sections.push("source_spans_changed");
    }
    if omission.text_evidence_changed_omitted > 0 {
        sections.push("text_evidence_changed");
    }
    if omission.path_evidence_invalidated_omitted > 0 {
        sections.push("path_evidence_invalidated");
    }
    if omission.sidecar_freshness_changed_omitted > 0 {
        sections.push("sidecar_freshness_changed");
    }
    if omission.source_roles_changed_omitted > 0 {
        sections.push("source_roles_changed");
    }
    if omission.file_renames_detected_omitted > 0 {
        sections.push("file_renames_detected");
    }
    if omission.rename_ambiguities_omitted > 0 {
        sections.push("rename_ambiguities");
    }
    sections
}

fn agent_use_graph_delta_summary_json(delta: &EntitySourceRoleDeltaReport) -> Value {
    json!({
        "entity_delta_count": delta.entities_added_count + delta.entities_removed_count + delta.entities_changed_count,
        "edge_delta_count": delta.edges_added_count + delta.edges_removed_count + delta.edges_changed_count,
        "source_span_delta_count": delta.source_spans_added_count + delta.source_spans_removed_count + delta.source_spans_changed_count,
        "source_role_delta_count": delta.source_roles_changed_count,
        "text_evidence_delta_count": delta.text_evidence_changed_count,
        "freshness_delta_count": agent_use_graph_delta_freshness_delta_count(delta),
        "closure_files_considered": delta.closure_delta_summary.closure_files_considered.len(),
        "closure_budget_hit": delta.closure_budget_hit,
    })
}

pub(crate) fn agent_use_graph_delta_packet_budget_json(
    delta: &EntitySourceRoleDeltaReport,
) -> Value {
    let truncated_sections = agent_use_graph_delta_truncated_sections(delta);
    json!({
        "default_compact": true,
        "summaries_first": true,
        "top_changed_facts_with_spans": true,
        "omitted_count": agent_use_graph_delta_omitted_count(delta),
        "truncated_sections": truncated_sections,
        "expansion_handle": delta.omission.expansion_handle,
        "expansion_handle_count": usize::from(delta.omission.expansion_handle.is_some()),
        "critical_safety_fields_preserved": true,
        "critical_safety_fields": [
            "status",
            "changed_files",
            "graph_delta.summary",
            "claimability",
            "lifecycle",
            "proof_ladder_changes",
            "stale_or_unsafe_blockers",
            "warnings_unknowns"
        ],
        "full_graph_dump_default": false,
        "full_detail_requires": "explain_or_audit_mode",
    })
}

pub(crate) fn agent_use_graph_delta_timing_json(
    delta: &EntitySourceRoleDeltaReport,
    hot_path_update_ms: u128,
    snapshot_old_ms: u128,
    snapshot_new_ms: u128,
    diff_closure_ms: u128,
    packet_serialize_ms: u128,
    delta_total_ms: u128,
    total_update_plus_delta_ms: u128,
) -> Value {
    json!({
        "total_update_plus_delta_ms": total_update_plus_delta_ms,
        "hot_path_update_ms": hot_path_update_ms,
        "snapshot_old_ms": snapshot_old_ms,
        "snapshot_new_ms": snapshot_new_ms,
        "diff_entities_ms": delta.timings.diff_entities_ms,
        "diff_edges_ms": delta.timings.diff_edges_ms,
        "diff_spans_ms": delta.timings.diff_spans_ms,
        "diff_text_evidence_ms": delta.timings.diff_text_evidence_ms,
        "diff_sidecars_ms": delta.timings.diff_sidecars_ms,
        "diff_closure_ms": diff_closure_ms,
        "packet_serialize_ms": packet_serialize_ms,
        "delta_total_ms": delta_total_ms,
    })
}

pub(crate) fn agent_use_graph_delta_json(delta: &EntitySourceRoleDeltaReport) -> Value {
    json!({
        "schema_version": delta.graph_delta_schema_version,
        "status": delta.status,
        "ready_to_report": delta.ready_to_report,
        "claimable": delta.claimable,
        "diagnostic_only": delta.diagnostic_only,
        "summary": agent_use_graph_delta_summary_json(delta),
        "snapshot_read_only": delta.snapshot_read_only,
        "snapshot_bounded_to_changed_or_closure_files": delta.snapshot_bounded_to_changed_or_closure_files,
        "entities_added_count": delta.entities_added_count,
        "entities_removed_count": delta.entities_removed_count,
        "entities_changed_count": delta.entities_changed_count,
        "edges_added_count": delta.edges_added_count,
        "edges_removed_count": delta.edges_removed_count,
        "edges_changed_count": delta.edges_changed_count,
        "source_roles_changed_count": delta.source_roles_changed_count,
        "source_spans_added_count": delta.source_spans_added_count,
        "source_spans_removed_count": delta.source_spans_removed_count,
        "source_spans_changed_count": delta.source_spans_changed_count,
        "text_evidence_changed_count": delta.text_evidence_changed_count,
        "path_evidence_invalidated_count": delta.path_evidence_invalidated_count,
        "sidecar_freshness_changed_count": delta.sidecar_freshness_changed_count,
        "entities_added": delta.entities_added,
        "entities_removed": delta.entities_removed,
        "entities_changed": delta.entities_changed,
        "edges_added": delta.edges_added,
        "edges_removed": delta.edges_removed,
        "edges_changed": delta.edges_changed,
        "source_spans_added": delta.source_spans_added,
        "source_spans_removed": delta.source_spans_removed,
        "source_spans_changed": delta.source_spans_changed,
        "text_evidence_changed": delta.text_evidence_changed,
        "path_evidence_invalidated": delta.path_evidence_invalidated,
        "sidecar_freshness_changed": delta.sidecar_freshness_changed,
        "candidate_spool_invalidated_or_refreshed": delta.candidate_spool_invalidated_or_refreshed,
        "candidate_query_index_invalidated_or_refreshed": delta.candidate_query_index_invalidated_or_refreshed,
        "vector_chunks_invalidated": delta.vector_chunks_invalidated,
        "vector_runtime_status_changed": delta.vector_runtime_status_changed,
        "vector_audit_status_changed": delta.vector_audit_status_changed,
        "nuance_tokens_invalidated": delta.nuance_tokens_invalidated,
        "routing_handles_invalidated": delta.routing_handles_invalidated,
        "proof_ladder_changes": delta.proof_ladder_changes,
        "file_renames_detected": delta.file_renames_detected,
        "rename_ambiguities": delta.rename_ambiguities,
        "closure_delta_summary": delta.closure_delta_summary,
        "closure_files_updated": delta.closure_files_updated,
        "closure_budget_hit": delta.closure_budget_hit,
        "closure_unsupported_relation_classes": delta.closure_unsupported_relation_classes,
        "closure_degraded_relation_classes": delta.closure_degraded_relation_classes,
        "closure_unknowns": delta.closure_unknowns,
        "relation_kind_counts": delta.relation_kind_counts,
        "exactness_counts": delta.exactness_counts,
        "derived_counts": delta.derived_counts,
        "source_role_counts": delta.source_role_counts,
        "degraded_relation_classes": delta.degraded_relation_classes,
        "unsupported_relation_classes": delta.unsupported_relation_classes,
        "source_roles_changed": delta.source_roles_changed,
        "timings": delta.timings.clone(),
        "packet_budget": agent_use_graph_delta_packet_budget_json(delta),
        "omitted_count": agent_use_graph_delta_omitted_count(delta),
        "truncated_sections": agent_use_graph_delta_truncated_sections(delta),
        "expansion_handle_count": usize::from(delta.omission.expansion_handle.is_some()),
        "omission": delta.omission,
        "rename_detection_status": delta.rename_detection_status,
        "text_evidence_not_graph_entity_delta": delta.text_evidence_not_graph_entity_delta,
        "text_candidate_evidence_not_graph_delta": delta.text_candidate_evidence_not_graph_delta,
        "source_navigation_only_not_graph_entity_delta": delta.source_navigation_only_not_graph_entity_delta,
        "same_name_symbols_distinct": delta.same_name_symbols_distinct,
        "endpoint_names_hydrated_where_available": delta.endpoint_names_hydrated_where_available,
        "same_name_targets_distinct": delta.same_name_targets_distinct,
        "exactness_preserved": delta.exactness_preserved,
        "derived_edges_require_provenance": delta.derived_edges_require_provenance,
        "heuristic_unsupported_edges_not_overclaimed": delta.heuristic_unsupported_edges_not_overclaimed,
        "test_mock_edges_preserved": delta.test_mock_edges_preserved,
        "source_spans_present_for_claimable_entity_deltas": delta.source_spans_present_for_claimable_entity_deltas,
        "source_spans_present_for_claimable_edge_deltas": delta.source_spans_present_for_claimable_edge_deltas,
        "source_spans_changed_reported": delta.source_spans_changed_reported,
        "text_evidence_changed_reported": delta.text_evidence_changed_reported,
        "path_evidence_invalidated_reported": delta.path_evidence_invalidated_reported,
        "candidate_freshness_delta_reported": delta.candidate_freshness_delta_reported,
        "vector_freshness_delta_reported": delta.vector_freshness_delta_reported,
        "nuance_freshness_delta_reported_or_not_applicable": delta.nuance_freshness_delta_reported_or_not_applicable,
        "routing_handle_delta_reported_or_not_applicable": delta.routing_handle_delta_reported_or_not_applicable,
        "proof_ladder_changes_reported": delta.proof_ladder_changes_reported,
        "freshness_delta_not_graph_proof": delta.freshness_delta_not_graph_proof,
        "access_vs_corrupt_classification_safe": delta.access_vs_corrupt_classification_safe,
        "stale_sidecars_not_used_as_fresh": delta.stale_sidecars_not_used_as_fresh,
        "rename_aware_delta_supported": delta.rename_aware_delta_supported,
        "rename_unknown_when_ambiguous": delta.rename_unknown_when_ambiguous,
        "closure_delta_supported": delta.closure_delta_supported,
        "closure_budget_hit_degraded": delta.closure_budget_hit_degraded,
        "unsupported_relation_unknown_not_proof": delta.unsupported_relation_unknown_not_proof,
        "duplicate_content_paths_distinct": delta.duplicate_content_paths_distinct,
        "no_silent_full_repo_fallback": delta.no_silent_full_repo_fallback,
        "source_spans_and_provenance_preserved": delta.source_spans_and_provenance_preserved,
        "old_good_db_preserved": delta.old_good_db_preserved,
        "claim_boundaries_preserved": delta.claim_boundaries_preserved,
        "public_claim": delta.public_claim,
        "warnings": delta.warnings,
    })
}

pub(crate) fn agent_use_watch_dependency_closure_degraded(
    summary: &IncrementalIndexSummary,
) -> bool {
    summary.dependency_closure.closure_budget_hit
        || !summary
            .dependency_closure
            .degraded_relation_classes
            .is_empty()
}

pub(crate) fn agent_use_watch_reason(
    summary: &IncrementalIndexSummary,
    no_op_paths: &[String],
) -> &'static str {
    if !no_op_paths.is_empty() && summary.files_ignored > 0 {
        "ignored_path_no_graph_changes"
    } else if agent_use_watch_dependency_closure_degraded(summary) {
        "dependency_closure_degraded"
    } else if !no_op_paths.is_empty() && summary.files_skipped > 0 {
        "skipped_path_no_graph_changes"
    } else if !no_op_paths.is_empty() {
        "metadata_or_content_unchanged"
    } else if summary.files_deleted > 0 && summary.files_indexed > 0 {
        "file_lifecycle_update_applied"
    } else if summary.files_deleted > 0 {
        "stale_facts_removed"
    } else {
        "changed_paths_updated"
    }
}

pub(crate) fn agent_use_watch_freshness_json(
    summary: &IncrementalIndexSummary,
    staged_availability: &Value,
) -> Value {
    let graph_changed = agent_use_watch_summary_has_fact_changes(summary);
    let path_evidence = match agent_use_path_evidence_delta_action(summary) {
        "refreshed" => "rebuilt",
        "invalidated" => "stale",
        _ => "current",
    };
    json!({
        "graph_db": "current",
        "files_facts": if graph_changed { "rebuilt" } else { "current" },
        "entities_edges_source_spans": if summary.files_parsed > 0 || summary.files_deleted > 0 || summary.files_renamed > 0 { "rebuilt" } else { "current" },
        "text_evidence": if summary.files_read > 0 || summary.files_deleted > 0 || summary.files_ignored > 0 { "rebuilt" } else { "current" },
        "path_evidence": path_evidence,
        "candidate_spool": staged_availability.get("candidate_spool_status").cloned().unwrap_or_else(|| json!("unknown")),
        "candidate_spool_query_index": staged_availability.pointer("/layer_readiness/candidate_spool/query_index_status").cloned().unwrap_or_else(|| json!("unknown")),
        "vector_runtime_sidecar": staged_availability.get("vector_runtime_status").cloned().unwrap_or_else(|| json!("unknown")),
        "vector_audit_artifact": staged_availability.get("vector_audit_status").cloned().unwrap_or_else(|| json!("unknown")),
        "binary_candidate_records": "not_applicable",
        "nuance_candidate_records": "not_applicable",
        "proof_path_caches": "not_applicable",
        "routing_context_handles": if summary.files_indexed > 0 || summary.files_deleted > 0 || summary.files_renamed > 0 { "rebuilt" } else { "current" },
    })
}

pub(crate) fn agent_use_layer_delta_action(status: Option<&str>) -> Value {
    let status = status.unwrap_or("unknown");
    let action = match status {
        "ready" | "superseded_by_graph_db" => "status_checked",
        "stale" => "invalidated",
        "missing"
        | "no_spool"
        | "query_index_missing"
        | "disabled_budget_exceeded"
        | "not_applicable" => "absent",
        "rebuilt" => "rebuilt",
        "corrupt" | "query_index_corrupt" | "sidecar_corrupt" => "error",
        "permission_denied"
        | "read_only"
        | "filesystem_inaccessible"
        | "sidecar_unavailable"
        | "sidecar_locked"
        | "blocked_by_graph_db" => "error",
        _ => "status_checked",
    };
    json!({
        "action": action,
        "status": status,
        "graph_proof": false,
    })
}

pub(crate) fn agent_use_not_applicable_delta_action(reason: &'static str) -> Value {
    json!({
        "action": "not_applicable",
        "status": "not_applicable",
        "reason": reason,
        "graph_proof": false,
    })
}

pub(crate) fn agent_use_path_evidence_delta_action(
    summary: &IncrementalIndexSummary,
) -> &'static str {
    if summary.dirty_path_evidence_count > 0 {
        "refreshed"
    } else if summary.files_indexed > 0
        || summary.files_deleted > 0
        || summary.files_renamed > 0
        || summary.deleted_fact_files > 0
        || summary.deleted_file_facts_removed > 0
        || summary.stale_facts_deleted_for_ignored_paths > 0
    {
        "invalidated"
    } else {
        "unchanged"
    }
}

pub(crate) fn agent_use_status_base_json(
    profile: &AgentUseProfile,
    preflight: &DbLifecyclePreflight,
    sqlite_sidecars: &Value,
    lifecycle: &Value,
    status: &str,
    wall_ms: f64,
) -> Value {
    let safety_labels = agent_use_safety_labels(profile, preflight, Some(sqlite_sidecars));
    json!({
        "schema_version": AGENT_JSON_SCHEMA_VERSION,
        "status": status,
        "command": "status",
        "command_namespace": "agent-use",
        "profile_name": profile.profile_name.clone(),
        "repo": path_string(&profile.repo_root),
        "repo_root": path_string(&profile.repo_root),
        "repo_identity_label": profile.repo_identity_label.clone(),
        "repo_identity_hash": profile.repo_identity_hash.clone(),
        "repo_identity_short_hash": agent_use_repo_identity_short_hash(profile),
        "db": path_string(&profile.db_path),
        "db_path": path_string(&profile.db_path),
        "external_db_used": true,
        "db_path_outside_workspace": preflight.db_path_outside_workspace,
        "outside_workspace_note": preflight.outside_workspace_note.clone(),
        "path_access_status": preflight.path_access_status.clone(),
        "path_access_error": preflight.path_access_error.clone(),
        "db_problem_kind": preflight.db_problem_kind.clone(),
        "db_health": preflight.db_health.clone(),
        "sqlite_sidecars": sqlite_sidecars.clone(),
        "sidecar_status": sqlite_sidecars["sidecar_status"].clone(),
        "sidecar_only_change": sqlite_sidecars["sidecar_only_change"].clone(),
        "sidecar_change_classification": sqlite_sidecars["sidecar_change_classification"].clone(),
        "lifecycle": lifecycle.clone(),
        "db_lifecycle_read": lifecycle.clone(),
          "publish_state": agent_use_publish_state_json(profile),
          "publishing": agent_use_publish_state_active(profile),
          "safety_labels": safety_labels,
          "claimable": preflight.safe,
          "diagnostic_only": !preflight.safe,
          "env_discovery": agent_use_env_discovery_json(profile),
          "config_discovery": agent_use_config_discovery_json(&profile.repo_root),
          "artifact_hygiene": agent_use_artifact_hygiene_json(),
          "profile": agent_use_profile_json(profile),
          "agent_use_profile": agent_use_profile_json(profile),
        "recovery": agent_use_recovery_json(profile),
        "telemetry": runtime_telemetry_unknown_json(),
        "truncation": {
            "returned_count": 1,
            "limit_applied": false,
            "omitted_count": 0,
            "total_available_unknown": false,
        },
        "result_count": 1,
        "limit": 1,
        "omitted_count": 0,
        "timings": agent_timings_from_wall_ms(wall_ms),
        "read_path_metrics": agent_use_status_read_path_metrics_json(wall_ms),
        "warnings": preflight.warnings.clone(),
        "errors": if preflight.safe { Vec::<String>::new() } else { preflight.blockers.clone() },
        "public_claim": false,
    })
}

pub(crate) fn agent_use_profile_json(profile: &AgentUseProfile) -> Value {
    json!({
        "profile_name": profile.profile_name.clone(),
        "repo_root": path_string(&profile.repo_root),
        "repo_identity_label": profile.repo_identity_label.clone(),
        "repo_identity_hash": profile.repo_identity_hash.clone(),
        "repo_identity_short_hash": agent_use_repo_identity_short_hash(profile),
        "profile_root": path_string(&profile.profile_root),
        "db_path": path_string(&profile.db_path),
        "candidate_spool_path": path_string(&profile.candidate_spool_path),
        "candidate_spool_query_index_path": path_string(&profile.candidate_spool_query_index_path),
        "vector_runtime_path": path_string(&profile.vector_runtime_path),
        "vector_audit_path": path_string(&profile.vector_audit_path),
        "lock_or_publish_state_path": path_string(&profile.lock_or_publish_state_path),
        "delta_state_path": path_string(&profile.delta_state_path),
        "sidecar_paths": agent_use_profile_sidecar_paths_json(profile),
        "path_mapping": agent_use_profile_path_mapping_json(profile),
        "env_discovery": agent_use_env_discovery_json(profile),
        "config_discovery": agent_use_config_discovery_json(&profile.repo_root),
        "artifact_hygiene": agent_use_artifact_hygiene_json(),
        "lifecycle_expectations": profile.lifecycle_expectations.clone(),
        "binary_profile": profile.binary_profile.clone(),
        "scope_policy": {
            "scope_policy_kind": SCOPE_POLICY_KIND_DEFAULT_WITH_OVERRIDES,
            "include_semantics": INCLUDE_SEMANTICS_DEFAULT_SCOPE_PLUS_OVERRIDES,
            "include_is_restrictive": false,
            "include_is_override": true,
            "scope_truth_status": SCOPE_TRUTH_STATUS_OVERRIDE_ONLY,
        },
        "mcp_args": profile.mcp_args.clone(),
        "recovery_commands": profile.recovery_commands.clone(),
    })
}

pub(crate) fn agent_use_repo_identity_short_hash(profile: &AgentUseProfile) -> String {
    profile.repo_identity_hash.chars().take(12).collect()
}

pub(crate) fn agent_use_profile_sidecar_paths_json(profile: &AgentUseProfile) -> Value {
    json!({
        "candidate_spool_path": path_string(&profile.candidate_spool_path),
        "candidate_spool_query_index_path": path_string(&profile.candidate_spool_query_index_path),
        "vector_runtime_path": path_string(&profile.vector_runtime_path),
        "vector_audit_path": path_string(&profile.vector_audit_path),
        "lock_or_publish_state_path": path_string(&profile.lock_or_publish_state_path),
        "delta_state_path": path_string(&profile.delta_state_path),
    })
}

pub(crate) fn agent_use_profile_path_mapping_json(profile: &AgentUseProfile) -> Value {
    let entries = [
        ("repo_root", profile.repo_root.as_path()),
        ("profile_root", profile.profile_root.as_path()),
        ("db_path", profile.db_path.as_path()),
        (
            "candidate_spool_path",
            profile.candidate_spool_path.as_path(),
        ),
        (
            "candidate_spool_query_index_path",
            profile.candidate_spool_query_index_path.as_path(),
        ),
        ("vector_runtime_path", profile.vector_runtime_path.as_path()),
        ("vector_audit_path", profile.vector_audit_path.as_path()),
        (
            "lock_or_publish_state_path",
            profile.lock_or_publish_state_path.as_path(),
        ),
        ("delta_state_path", profile.delta_state_path.as_path()),
    ];
    let mappings = entries
        .iter()
        .map(|(name, path)| ((*name).to_string(), agent_use_path_mapping_json(path)))
        .collect::<serde_json::Map<_, _>>();
    let warnings = mappings
        .iter()
        .filter(|&(_name, mapping)| mapping["mapping_unavailable"].as_bool().unwrap_or(false))
        .map(|(name, mapping)| {
            json!({
                "path_name": name,
                "label": "path_mapping_unavailable",
                "mapping_kind": mapping["mapping_kind"].clone(),
                "mapping_status": mapping["mapping_status"].clone(),
                "path": mapping["path"].clone(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "status": if warnings.is_empty() { "ok" } else { "mapping_warnings" },
        "warnings": warnings,
        "paths": mappings,
    })
}

pub(crate) fn agent_use_env_discovery_json(_profile: &AgentUseProfile) -> Value {
    let explicit_data_root = std::env::var_os(AGENT_USE_DATA_ROOT_ENV);
    let explicit = match explicit_data_root {
        Some(value) if value.is_empty() => json!({
            "env_var": AGENT_USE_DATA_ROOT_ENV,
            "status": "env_invalid",
            "required": false,
            "message": "env var is set but empty",
        }),
        Some(value) => json!({
            "env_var": AGENT_USE_DATA_ROOT_ENV,
            "status": "ok",
            "required": false,
            "path": path_string(&PathBuf::from(value)),
        }),
        None => json!({
            "env_var": AGENT_USE_DATA_ROOT_ENV,
            "status": "env_missing",
            "required": false,
            "message": "optional override not set; platform data dir is used",
        }),
    };
    let platform = agent_use_platform_data_dir_env_json();
    json!({
        "status": if explicit["status"] == "env_invalid" || platform["status"] == "env_missing" { "diagnostic" } else { "ok" },
        "data_root_source": if explicit["status"] == "ok" { "env" } else { "platform_default" },
        "explicit_data_root": explicit,
        "platform_data_dir": platform,
        "profile_root_ref": "profile_root",
        "db_path_ref": "db",
    })
}

pub(crate) fn agent_use_platform_data_dir_env_json() -> Value {
    #[cfg(windows)]
    {
        match std::env::var_os("LOCALAPPDATA") {
            Some(value) if !value.is_empty() => json!({
                "env_var": "LOCALAPPDATA",
                "status": "ok",
                "required": true,
                "path": path_string(&PathBuf::from(value)),
            }),
            _ => json!({
                "env_var": "LOCALAPPDATA",
                "status": "env_missing",
                "required": true,
                "message": "LOCALAPPDATA is required when CODEGRAPH_AGENT_USE_DATA_ROOT is not set",
            }),
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(value) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
            return json!({
                "env_var": "XDG_DATA_HOME",
                "status": "ok",
                "required": true,
                "path": path_string(&PathBuf::from(value)),
            });
        }
        match std::env::var_os("HOME") {
            Some(value) if !value.is_empty() => json!({
                "env_var": "HOME",
                "status": "ok",
                "required": true,
                "path": path_string(&PathBuf::from(value)),
            }),
            _ => json!({
                "env_var": "HOME|XDG_DATA_HOME",
                "status": "env_missing",
                "required": true,
                "message": "HOME or XDG_DATA_HOME is required when CODEGRAPH_AGENT_USE_DATA_ROOT is not set",
            }),
        }
    }
}

pub(crate) fn agent_use_config_discovery_json(repo_root: &Path) -> Value {
    let config_path = repo_root.join(".codex").join("config.toml");
    if !config_path.exists() {
        return json!({
          "status": "config_missing",
          "required": false,
            "diagnostic_only": true,
            "path": path_string(&config_path),
            "message": "optional Codex MCP config is missing; agent-use commands remain available",
            "recovery_ref": "recovery_commands.mcp_config",
        });
    }
    let text = match fs::read_to_string(&config_path) {
        Ok(text) => text,
        Err(error) => {
            let classification = sidecar_access_classification_from_message(&error.to_string());
            return json!({
                "status": classification.status,
                "required": false,
                "diagnostic_only": true,
                "path": path_string(&config_path),
                "path_access_status": classification.path_access_status,
                "problem_kind": classification.problem_kind,
                "message": error.to_string(),
            });
        }
    };
    let validation = validate_codex_mcp_config_text(&text);
    json!({
        "status": validation.status,
        "required": false,
        "diagnostic_only": validation.status != "ok",
        "path": path_string(&config_path),
        "unknown_field_policy": "warning",
        "unknown_fields": validation.unknown_fields,
        "warnings": validation.warnings,
        "errors": validation.errors,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodexConfigValidation {
    status: &'static str,
    unknown_fields: Vec<String>,
    warnings: Vec<String>,
    errors: Vec<String>,
}

pub(crate) fn validate_codex_mcp_config_text(text: &str) -> CodexConfigValidation {
    let allowed_keys = ["command", "args", "cwd", "env"];
    let mut current_section = String::new();
    let mut unknown_fields = Vec::new();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    for (line_index, raw_line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') {
                errors.push(format!(
                    "config_invalid: malformed section header at line {line_number}"
                ));
                continue;
            }
            current_section = line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .trim_matches('"')
                .to_string();
            if current_section != "mcp_servers.codegraph-mcp" {
                unknown_fields.push(current_section.clone());
                warnings.push(format!(
                    "config_unknown_field: unsupported section {} at line {line_number}",
                    current_section
                ));
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            errors.push(format!(
                "config_invalid: expected key = value at line {line_number}"
            ));
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() || !config_value_balanced(value) {
            errors.push(format!(
                "config_invalid: malformed value for {key} at line {line_number}"
            ));
            continue;
        }
        if current_section == "mcp_servers.codegraph-mcp" && !allowed_keys.contains(&key) {
            unknown_fields.push(key.to_string());
            warnings.push(format!(
                "config_unknown_field: unsupported key {key} in mcp_servers.codegraph-mcp at line {line_number}"
            ));
        }
    }
    let status = if !errors.is_empty() {
        "config_invalid"
    } else if !unknown_fields.is_empty() {
        "config_unknown_field"
    } else {
        "ok"
    };
    CodexConfigValidation {
        status,
        unknown_fields,
        warnings,
        errors,
    }
}

pub(crate) fn config_value_balanced(value: &str) -> bool {
    value.matches('"').count().is_multiple_of(2)
        && value.matches('[').count() == value.matches(']').count()
        && value.matches('{').count() == value.matches('}').count()
}

pub(crate) fn agent_use_artifact_hygiene_json() -> Value {
    json!({
        "status": "local_ignored",
        "reports_final_canonical_dashboard": true,
        "reports_audit_local_evidence": true,
        "raw_artifacts_not_promoted": true,
        "ignored_local_pattern_count": 9,
        "ignored_local_patterns_ref": "docs/guardrails.md",
        "public_claim": false,
    })
}

pub(crate) fn agent_use_path_mapping_json(path: &Path) -> Value {
    let raw = path_string(path);
    let normalized = raw.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    let mapping_kind = classify_agent_use_path_mapping_kind(&lower);
    let mapping_unavailable = matches!(
        mapping_kind,
        "wsl_path" | "docker_mount_path" | "unknown_mapping"
    );
    json!({
        "path": raw,
        "normalized_display_path": normalized,
        "mapping_kind": mapping_kind,
        "mapping_status": if mapping_unavailable { "mapping_unavailable" } else { "ok" },
        "mapping_unavailable": mapping_unavailable,
        "label": if mapping_unavailable { "path_mapping_unavailable" } else { "native_path" },
        "diagnostic_only": mapping_unavailable,
        "warnings": if mapping_unavailable {
            vec![json!({
                "label": "path_mapping_unavailable",
                "status": "unavailable",
                "diagnostic_only": true,
                "message": format!("{mapping_kind} requires an explicit host/container path mapping before reuse"),
            })]
        } else {
            Vec::<Value>::new()
        },
    })
}

pub(crate) fn classify_agent_use_path_mapping_kind(normalized_lower: &str) -> &'static str {
    if normalized_lower.starts_with("//wsl$/")
        || normalized_lower.starts_with("//wsl.localhost/")
        || looks_like_wsl_mount_path(normalized_lower)
    {
        return "wsl_path";
    }
    if normalized_lower.starts_with("/workspace/")
        || normalized_lower == "/workspace"
        || normalized_lower.starts_with("/workspaces/")
        || normalized_lower == "/workspaces"
        || normalized_lower.starts_with("/var/lib/docker/")
        || normalized_lower.contains("/docker/volumes/")
    {
        return "docker_mount_path";
    }
    if cfg!(windows) {
        let bytes = normalized_lower.as_bytes();
        if normalized_lower.starts_with("//")
            || (bytes.len() >= 3
                && bytes[1] == b':'
                && bytes[2] == b'/'
                && bytes[0].is_ascii_alphabetic())
        {
            return "native_windows";
        }
    } else if normalized_lower.starts_with('/') {
        return "native_unix";
    }
    if normalized_lower.starts_with("./") || !normalized_lower.starts_with('/') {
        return if cfg!(windows) {
            "native_windows"
        } else {
            "native_unix"
        };
    }
    "unknown_mapping"
}

pub(crate) fn looks_like_wsl_mount_path(normalized_lower: &str) -> bool {
    let bytes = normalized_lower.as_bytes();
    normalized_lower.starts_with("/mnt/")
        && bytes.len() >= 7
        && bytes[5].is_ascii_alphabetic()
        && bytes[6] == b'/'
}

pub(crate) fn agent_use_mcp_config_identity_json(
    profile: &AgentUseProfile,
    sidecar_paths: Value,
    path_mapping: Value,
) -> Value {
    json!({
        "config_version": 1,
        "server_name": "codegraph-mcp",
        "profile_name": profile.profile_name.clone(),
        "active_profile_name": profile.profile_name.clone(),
        "repo_root": path_string(&profile.repo_root),
        "repo_identity_label": profile.repo_identity_label.clone(),
        "repo_identity_hash": profile.repo_identity_hash.clone(),
        "repo_identity_short_hash": agent_use_repo_identity_short_hash(profile),
        "profile_root": path_string(&profile.profile_root),
        "db_path": path_string(&profile.db_path),
        "sidecar_paths": sidecar_paths,
        "path_mapping": path_mapping,
        "config_pins_repo": true,
        "config_pins_db": true,
        "config_pins_profile": true,
        "safe_read_only_startup": true,
        "auto_index_on_startup": false,
        "no_dot_codegraph_fallback": true,
    })
}

pub(crate) fn agent_use_recovery_json(profile: &AgentUseProfile) -> Value {
    let repo = path_string(&profile.repo_root);
    json!({
        "agent_use_index_command": format!("{BIN_NAME} agent-use index --repo \"{repo}\" --json"),
        "agent_use_status_command": format!("{BIN_NAME} agent-use status --repo \"{repo}\" --json"),
        "agent_use_mcp_config_command": format!("{BIN_NAME} agent-use mcp-config --repo \"{repo}\" --json"),
        "agent_use_mcp_config_available": true,
        "agent_use_mcp_config_status": "implemented",
        "agent_use_query_symbols_command": format!("{BIN_NAME} agent-use query symbols <symbol> --repo \"{repo}\" --limit 5 --agent-json"),
        "agent_use_query_text_command": format!("{BIN_NAME} agent-use query text \"<text>\" --repo \"{repo}\" --limit 5 --agent-json"),
        "agent_use_query_files_command": format!("{BIN_NAME} agent-use query files <path-or-text> --repo \"{repo}\" --limit 5 --agent-json"),
        "agent_use_context_pack_command": format!("{BIN_NAME} agent-use context-pack --repo \"{repo}\" --task \"<task>\" --agent-json"),
        "agent_use_watch_once_command": format!("{BIN_NAME} agent-use watch --repo \"{repo}\" --once --changed <path> --json"),
        "agent_use_query_available": true,
        "agent_use_query_status": "implemented",
        "agent_use_watch_available": true,
        "agent_use_watch_status": "implemented_once_changed",
        "commands": profile.recovery_commands.clone(),
    })
}

pub(crate) fn agent_use_profile_parent_create_error_json(
    profile: &AgentUseProfile,
    problem_kind: &str,
    message: &str,
    normal_dot_codegraph_existed_before: bool,
) -> Result<String, String> {
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let safety_labels = [problem_kind.to_string(), "diagnostic_only".to_string()]
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    serde_json::to_string(&json!({
        "status": problem_kind,
        "error": problem_kind,
        "message": message,
        "command": "index",
        "command_namespace": "agent-use",
        "profile_name": profile.profile_name.clone(),
        "repo": path_string(&profile.repo_root),
        "repo_root": path_string(&profile.repo_root),
        "db": path_string(&profile.db_path),
        "db_path": path_string(&profile.db_path),
        "profile_root": path_string(&profile.profile_root),
        "external_db_used": true,
        "path_access_status": problem_kind,
        "db_problem_kind": problem_kind,
        "claimable": false,
        "diagnostic_only": true,
        "safety_labels": safety_labels,
        "publish_state": agent_use_publish_state_json(profile),
        "publishing": agent_use_publish_state_active(profile),
        "recovery": agent_use_recovery_json(profile),
        "normal_dot_codegraph_created": !normal_dot_codegraph_existed_before && normal_dot_codegraph.exists(),
        "normal_dot_codegraph_mutated": normal_dot_codegraph_existed_before != normal_dot_codegraph.exists(),
        "public_claim": false,
    }))
    .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentUseProfileAccessProblem {
    pub(crate) status: &'static str,
    pub(crate) problem_kind: &'static str,
    pub(crate) message: String,
}

pub(crate) fn agent_use_profile_root_access_problem(
    profile: &AgentUseProfile,
) -> Option<AgentUseProfileAccessProblem> {
    if cli_write_path_chaos_failpoint_enabled(AGENT_USE_PROFILE_PARENT_PERMISSION_DENIED_FAILPOINT)
    {
        return Some(AgentUseProfileAccessProblem {
            status: "permission_denied",
            problem_kind: "permission_denied",
            message: format!(
                "chaos_failpoint:{AGENT_USE_PROFILE_PARENT_PERMISSION_DENIED_FAILPOINT}"
            ),
        });
    }
    if cli_write_path_chaos_failpoint_enabled(
        AGENT_USE_PROFILE_PARENT_FILESYSTEM_INACCESSIBLE_FAILPOINT,
    ) {
        return Some(AgentUseProfileAccessProblem {
            status: "filesystem_inaccessible",
            problem_kind: "filesystem_inaccessible",
            message: format!(
                "chaos_failpoint:{AGENT_USE_PROFILE_PARENT_FILESYSTEM_INACCESSIBLE_FAILPOINT}"
            ),
        });
    }
    let Some(data_root) = profile.profile_root.parent() else {
        return Some(AgentUseProfileAccessProblem {
            status: "filesystem_inaccessible",
            problem_kind: "filesystem_inaccessible",
            message: format!(
                "agent-use profile root has no parent: {}",
                profile.profile_root.display()
            ),
        });
    };
    match fs::metadata(data_root) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Some(AgentUseProfileAccessProblem {
                status: "filesystem_inaccessible",
                problem_kind: "filesystem_inaccessible",
                message: format!(
                    "agent-use profile data root is not a directory: {}",
                    data_root.display()
                ),
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Some(AgentUseProfileAccessProblem {
                status: "permission_denied",
                problem_kind: "permission_denied",
                message: format!(
                    "agent-use profile data root is not readable: {}: {error}",
                    data_root.display()
                ),
            });
        }
        Err(error) => {
            return Some(AgentUseProfileAccessProblem {
                status: "filesystem_inaccessible",
                problem_kind: "filesystem_inaccessible",
                message: format!(
                    "agent-use profile data root is not accessible: {}: {error}",
                    data_root.display()
                ),
            });
        }
    }
    match fs::metadata(&profile.profile_root) {
        Ok(metadata) if metadata.is_dir() => None,
        Ok(_) => Some(AgentUseProfileAccessProblem {
            status: "filesystem_inaccessible",
            problem_kind: "filesystem_inaccessible",
            message: format!(
                "agent-use profile root is not a directory: {}",
                profile.profile_root.display()
            ),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            Some(AgentUseProfileAccessProblem {
                status: "permission_denied",
                problem_kind: "permission_denied",
                message: format!(
                    "agent-use profile root is not readable: {}: {error}",
                    profile.profile_root.display()
                ),
            })
        }
        Err(error) => Some(AgentUseProfileAccessProblem {
            status: "filesystem_inaccessible",
            problem_kind: "filesystem_inaccessible",
            message: format!(
                "agent-use profile root is not accessible: {}: {error}",
                profile.profile_root.display()
            ),
        }),
    }
}

pub(crate) fn agent_use_profile_access_unavailable_json(
    profile: &AgentUseProfile,
    command: &str,
    subcommand: Option<&str>,
    problem: &AgentUseProfileAccessProblem,
    normal_dot_codegraph_existed_before: bool,
) -> Value {
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let safety_labels = [
        problem.status.to_string(),
        "profile_root_inaccessible".to_string(),
        "diagnostic_only".to_string(),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>()
    .into_iter()
    .collect::<Vec<_>>();
    json!({
        "status": problem.status,
        "command": command,
        "subcommand": subcommand,
        "command_namespace": "agent-use",
        "profile_name": profile.profile_name.clone(),
        "repo": path_string(&profile.repo_root),
        "repo_root": path_string(&profile.repo_root),
        "repo_identity_label": profile.repo_identity_label.clone(),
        "repo_identity_hash": profile.repo_identity_hash.clone(),
        "repo_identity_short_hash": agent_use_repo_identity_short_hash(profile),
        "profile_root": path_string(&profile.profile_root),
        "db": path_string(&profile.db_path),
        "db_path": path_string(&profile.db_path),
        "resolved_db": path_string(&profile.db_path),
        "db_source": "agent-use profile",
        "external_db_used": true,
        "claimable": false,
        "diagnostic_only": true,
        "path_access_status": problem.status,
        "path_access_error": problem.message,
        "db_problem_kind": problem.problem_kind,
        "profile_access": {
            "status": problem.status,
            "problem_kind": problem.problem_kind,
            "profile_root": path_string(&profile.profile_root),
            "db_path": path_string(&profile.db_path),
            "existing_profile_state": "unknown_due_to_access",
            "message": problem.message,
            "not_false_not_indexed": true,
        },
        "safety_labels": safety_labels,
        "recovery": agent_use_recovery_json(profile),
        "recovery_commands": profile.recovery_commands.clone(),
        "warnings": [problem.message.clone()],
        "errors": [problem.message.clone()],
        "normal_dot_codegraph_path": path_string(&normal_dot_codegraph),
        "normal_dot_codegraph_created": !normal_dot_codegraph_existed_before && normal_dot_codegraph.exists(),
        "normal_dot_codegraph_mutated": normal_dot_codegraph_existed_before != normal_dot_codegraph.exists(),
        "public_claim": false,
    })
}

pub(crate) fn write_agent_use_publish_state(
    profile: &AgentUseProfile,
    status: &str,
    error: Option<&str>,
) -> Result<(), String> {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let state = json!({
        "profile_name": profile.profile_name.clone(),
        "repo_root": path_string(&profile.repo_root),
        "db_path": path_string(&profile.db_path),
        "status": status,
        "updated_unix_ms": timestamp_ms,
        "visible_db_mutation_claim": "old_good_db_still_visible_until_atomic_publish",
        "temp_db_claimability": "never_claimable",
        "recovery": agent_use_recovery_json(profile),
        "error": error,
        "public_claim": false,
    });
    let bytes = serde_json::to_vec_pretty(&state).map_err(|error| error.to_string())?;
    fs::write(&profile.lock_or_publish_state_path, bytes).map_err(|write_error| {
        format!(
            "agent-use publish state could not be written at {}: {write_error}",
            profile.lock_or_publish_state_path.display()
        )
    })
}

pub(crate) fn clear_agent_use_publish_state(profile: &AgentUseProfile) -> Result<(), String> {
    match fs::remove_file(&profile.lock_or_publish_state_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "agent-use publish state could not be cleared at {}: {error}",
            profile.lock_or_publish_state_path.display()
        )),
    }
}

pub(crate) fn agent_use_publish_state_json(profile: &AgentUseProfile) -> Value {
    let path = &profile.lock_or_publish_state_path;
    if !path.exists() {
        return json!({
            "status": "absent",
            "path": path_string(path),
            "active": false,
            "updating": false,
            "publishing": false,
            "claimability_effect": "none",
            "temp_db_claimability": "never_claimable",
        });
    }
    let parsed = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let status = parsed
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("publishing");
    json!({
        "status": status,
        "path": path_string(path),
        "active": true,
        "updating": status == "updating",
        "publishing": true,
        "state_readable": parsed.is_some(),
        "state": parsed.unwrap_or(Value::Null),
        "claimability_effect": "old valid DB may remain readable; temp DB is never claimable",
        "temp_db_claimability": "never_claimable",
    })
}

pub(crate) fn agent_use_publish_state_status(profile: &AgentUseProfile) -> Option<String> {
    let path = &profile.lock_or_publish_state_path;
    if !path.exists() {
        return None;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| {
            value
                .get("status")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .or_else(|| Some("publishing".to_string()))
}

pub(crate) fn agent_use_publish_state_active(profile: &AgentUseProfile) -> bool {
    profile.lock_or_publish_state_path.exists()
}

pub(crate) fn agent_use_last_delta_state_json(profile: &AgentUseProfile) -> Value {
    let path = &profile.delta_state_path;
    if !path.exists() {
        return json!({
            "status": "absent",
            "path": path_string(path),
            "state_readable": false,
            "last_delta_update_summary": Value::Null,
            "public_claim": false,
        });
    }
    match fs::read_to_string(path)
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|error| error.to_string()))
    {
        Ok(mut value) => {
            if let Some(object) = value.as_object_mut() {
                object.insert("path".to_string(), json!(path_string(path)));
                object.insert("state_readable".to_string(), json!(true));
            }
            value
        }
        Err(error) => json!({
            "status": "error",
            "path": path_string(path),
            "state_readable": false,
            "error": error,
            "last_delta_update_summary": Value::Null,
            "public_claim": false,
        }),
    }
}

pub(crate) fn persist_agent_use_last_delta_state(
    value: &mut Value,
    profile: &AgentUseProfile,
    source: &str,
) {
    let state = agent_use_delta_state_record(profile, value, source);
    let result = (|| -> Result<(), String> {
        if let Some(parent) = profile.delta_state_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "agent-use delta state parent could not be created at {}: {error}",
                    parent.display()
                )
            })?;
        }
        let bytes = serde_json::to_vec_pretty(&state).map_err(|error| error.to_string())?;
        fs::write(&profile.delta_state_path, bytes).map_err(|error| {
            format!(
                "agent-use delta state could not be written at {}: {error}",
                profile.delta_state_path.display()
            )
        })
    })();
    if let Some(object) = value.as_object_mut() {
        match result {
            Ok(()) => {
                object.insert("last_delta_state_persisted".to_string(), json!(true));
                object.insert(
                    "last_delta_state_path".to_string(),
                    json!(path_string(&profile.delta_state_path)),
                );
                object.insert("last_delta_state".to_string(), state.clone());
                object.insert(
                    "last_delta_update_summary".to_string(),
                    state
                        .get("last_delta_update_summary")
                        .cloned()
                        .unwrap_or(Value::Null),
                );
            }
            Err(error) => {
                object.insert("last_delta_state_persisted".to_string(), json!(false));
                object.insert("last_delta_state_error".to_string(), json!(error.clone()));
                let warning = format!("last_delta_state_persist_failed: {error}");
                match object.get_mut("warnings") {
                    Some(Value::Array(warnings)) => warnings.push(json!(warning)),
                    _ => {
                        object.insert("warnings".to_string(), json!([warning]));
                    }
                }
            }
        }
    }
}

pub(crate) fn agent_use_delta_state_record(
    profile: &AgentUseProfile,
    value: &Value,
    source: &str,
) -> Value {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let summary = agent_use_compact_delta_summary(value);
    json!({
        "schema_version": 1,
        "status": "recorded",
        "source": source,
        "profile_name": profile.profile_name.clone(),
        "repo_root": path_string(&profile.repo_root),
        "db_path": path_string(&profile.db_path),
        "updated_unix_ms": timestamp_ms,
        "delta_state": value.get("delta_state").cloned().unwrap_or_else(|| json!("unknown")),
        "graph_freshness": value.pointer("/freshness/graph_db").cloned().unwrap_or_else(|| json!("unknown")),
        "candidate_only_available": value.get("candidate_only_available").cloned().unwrap_or_else(|| json!(false)),
        "graph_proof_available": value.get("graph_proof_available").cloned().unwrap_or_else(|| json!(false)),
        "stale_candidate_layers": agent_use_stale_candidate_layers(value.get("staged_availability").unwrap_or(&Value::Null)),
        "claimability": value.get("claimability").cloned().unwrap_or(Value::Null),
        "freshness": value.get("freshness").cloned().unwrap_or(Value::Null),
        "last_delta_update_summary": summary,
        "recovery_commands": profile.recovery_commands.clone(),
        "public_claim": false,
    })
}

pub(crate) fn agent_use_compact_delta_summary(value: &Value) -> Value {
    let mut object = serde_json::Map::new();
    for key in [
        "status",
        "delta_state",
        "watch_mode",
        "changed_paths",
        "rejected_paths",
        "no_op_paths",
        "files_walked",
        "files_read",
        "files_hashed",
        "files_parsed",
        "facts_deleted",
        "facts_inserted",
        "entities_added",
        "entities_removed",
        "entities_changed",
        "edges_added",
        "edges_removed",
        "edges_changed",
        "source_spans_added",
        "source_spans_removed",
        "source_spans_changed",
        "text_evidence_changed",
        "path_evidence_invalidated",
        "candidate_spool_invalidated_or_rebuilt",
        "candidate_spool_invalidated_or_refreshed",
        "candidate_query_index_invalidated_or_rebuilt",
        "candidate_query_index_invalidated_or_refreshed",
        "vector_chunks_invalidated_or_rebuilt",
        "vector_chunks_invalidated",
        "vector_runtime_status_changed",
        "vector_audit_status_changed",
        "nuance_tokens_invalidated",
        "routing_handles_invalidated",
        "routing_handles_invalidated_or_not_applicable",
        "proof_ladder_changes",
        "file_renames_detected",
        "rename_ambiguities",
        "closure_delta_summary",
        "closure_files_considered",
        "closure_files_updated",
        "closure_edges_inspected",
        "closure_relation_classes",
        "closure_budget_hit",
        "degraded_relation_classes",
        "unsupported_relation_classes",
        "graph_delta_detail_mode",
        "delta_packet_compact_default",
        "graph_delta_packet_budget",
        "timings",
        "old_graph_valid",
        "new_graph_valid",
        "old_db_preserved",
        "temp_db_claimable",
        "claimability",
        "dirty_evidence_summary",
        "invalidated_evidence",
        "refreshed_evidence",
        "stale_evidence",
        "unavailable_evidence",
        "sidecar_statuses",
        "candidate_layer_status",
        "vector_layer_status",
        "path_evidence_status",
        "source_navigation_status",
        "routing_handle_status",
        "proof_ladder_change_counts",
        "stale_non_proof_reasons",
        "claimability_effect",
        "severity_effect",
        "warnings",
        "recovery_commands",
    ] {
        if let Some(field) = value.get(key) {
            object.insert(key.to_string(), field.clone());
        }
    }
    Value::Object(object)
}

pub(crate) fn add_agent_use_dirty_evidence_output_fields(
    value: &mut Value,
    surface: &str,
    full_detail: bool,
) {
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
    let fields = agent_use_dirty_evidence_output_fields(
        surface,
        value,
        &staged_availability,
        &proof_ladder_changes,
        recovery_commands,
        full_detail,
    );
    merge_json_object(value, fields.clone());

    let hard_interrupt_fields = agent_use_dirty_evidence_hard_interrupt_fields(&fields);
    if let Some(packet) = value
        .get_mut("validation_packet")
        .filter(|packet| packet.is_object())
    {
        merge_json_object(packet, fields.clone());
        if let Some(interrupt) = packet
            .get_mut("hard_interrupt")
            .filter(|interrupt| interrupt.is_object())
        {
            merge_json_object(interrupt, hard_interrupt_fields.clone());
        }
    }
    if let Some(packet) = value
        .get_mut("hard_interrupt")
        .filter(|packet| packet.is_object())
    {
        merge_json_object(packet, hard_interrupt_fields);
    }
    agent_use_append_dirty_evidence_expansion_handle(value, full_detail);
}

pub(crate) fn agent_use_dirty_evidence_output_fields(
    surface: &str,
    source: &Value,
    staged_availability: &Value,
    proof_ladder_changes: &Value,
    recovery_commands: Value,
    full_detail: bool,
) -> Value {
    let graph_status = agent_use_layer_freshness(staged_availability, "graph_db");
    let candidate_layer_status = agent_use_layer_freshness(staged_availability, "candidate_spool");
    let vector_layer_status = agent_use_layer_freshness(staged_availability, "vector_runtime");
    let vector_audit_status = agent_use_layer_freshness(staged_availability, "vector_audit");
    let path_evidence_status = agent_use_path_evidence_freshness(source);
    let source_navigation_status =
        agent_use_source_navigation_freshness(staged_availability, &candidate_layer_status);
    let routing_handle_status = agent_use_routing_handle_freshness(source);
    let sidecar_statuses = json!({
        "graph_db_status": graph_status,
        "candidate_layer_status": candidate_layer_status,
        "candidate_spool_query_index_status": agent_use_candidate_query_index_freshness(staged_availability),
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
    let proof_ladder_change_counts = agent_use_proof_ladder_change_counts(proof_ladder_changes);
    let invalidated_evidence =
        agent_use_invalidated_evidence_statuses(source, proof_ladder_changes);
    let refreshed_evidence = agent_use_refreshed_evidence_statuses(source, proof_ladder_changes);
    let stale_evidence =
        agent_use_stale_evidence_statuses(staged_availability, proof_ladder_changes);
    let unavailable_evidence = agent_use_unavailable_evidence_statuses(staged_availability);
    let stale_non_proof_reasons =
        agent_use_stale_non_proof_reasons(&stale_evidence, &unavailable_evidence);
    let graph_proof_available = staged_availability
        .get("graph_proof_available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let claimability_effect =
        agent_use_output_claimability_effect(staged_availability, graph_proof_available);
    let sidecar_corrupt_or_unavailable =
        agent_use_any_sidecar_corrupt_or_unavailable(&sidecar_statuses);
    let graph_db_corrupt_or_unavailable = matches!(
        graph_status.as_str(),
        Some("corrupt" | "inaccessible" | "permission_denied")
    );
    let graph_validation_status = agent_use_graph_validation_status(
        source,
        graph_proof_available,
        graph_status.as_str().unwrap_or("unknown"),
    );
    let candidate_recall_status = agent_use_candidate_recall_status(&sidecar_statuses);
    let candidate_recall_degraded = candidate_recall_status == "degraded";
    let graph_validation_unaffected_by_optional_sidecars =
        graph_proof_available && candidate_recall_degraded && !graph_db_corrupt_or_unavailable;
    let agent_action = agent_use_agent_action_for_dirty_evidence(
        &graph_validation_status,
        graph_db_corrupt_or_unavailable,
        graph_proof_available,
    );
    let candidate_recall_action = if candidate_recall_degraded {
        "refresh_sidecars_if_candidate_recall_needed"
    } else {
        "none"
    };
    let sidecar_degradation_kind = if graph_db_corrupt_or_unavailable {
        "graph_lifecycle"
    } else if candidate_recall_degraded {
        "candidate_layer_only"
    } else {
        "none"
    };

    json!({
        "dirty_evidence_summary": {
            "schema_version": 1,
            "surface": surface,
            "surfaces_total": 8,
            "fresh_count": agent_use_count_fresh_dirty_statuses(&sidecar_statuses),
            "stale_count": stale_evidence.as_array().map(Vec::len).unwrap_or_default(),
            "dirty_count": invalidated_evidence.as_array().map(Vec::len).unwrap_or_default(),
            "unavailable_count": unavailable_evidence.as_array().map(Vec::len).unwrap_or_default(),
            "corrupt_count": agent_use_count_status(&sidecar_statuses, "corrupt"),
            "not_applicable_count": agent_use_count_status(&sidecar_statuses, "not_applicable"),
            "graph_proof_available": graph_proof_available,
            "graph_validation_status": graph_validation_status.clone(),
            "agent_action": agent_action,
            "candidate_recall_status": candidate_recall_status,
            "candidate_recall_degraded": candidate_recall_degraded,
            "candidate_recall_action": candidate_recall_action,
            "sidecar_degradation_kind": sidecar_degradation_kind,
            "graph_validation_unaffected_by_optional_sidecars": graph_validation_unaffected_by_optional_sidecars,
            "optional_sidecar_staleness_affects_graph_proof": false,
            "sidecar_corrupt_or_unavailable": sidecar_corrupt_or_unavailable,
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
        "graph_validation_status": graph_validation_status,
        "agent_action": agent_action,
        "candidate_recall_status": candidate_recall_status,
        "candidate_recall_degraded": candidate_recall_degraded,
        "candidate_recall_action": candidate_recall_action,
        "sidecar_degradation_kind": sidecar_degradation_kind,
        "graph_validation_unaffected_by_optional_sidecars": graph_validation_unaffected_by_optional_sidecars,
        "optional_sidecar_staleness_affects_graph_proof": false,
        "stale_non_proof_reasons": stale_non_proof_reasons,
        "claimability_effect": claimability_effect,
        "severity_effect": {
            "severity_model_preserved": true,
            "hard_interrupt_eligibility_unchanged": true,
            "stale_sidecar_not_hard_interrupt": true,
            "top_level_status_not_degraded_by_optional_sidecars": true,
            "text_evidence_change_not_broken_graph_behavior": true,
            "candidate_vector_source_navigation_non_proof": true,
        },
        "recovery_commands": recovery_commands,
    })
}

fn agent_use_dirty_evidence_hard_interrupt_fields(fields: &Value) -> Value {
    json!({
        "dirty_evidence_summary": fields.get("dirty_evidence_summary").cloned().unwrap_or(Value::Null),
        "proof_ladder_change_counts": fields.get("proof_ladder_change_counts").cloned().unwrap_or(Value::Null),
        "sidecar_statuses": fields.get("sidecar_statuses").cloned().unwrap_or(Value::Null),
        "stale_non_proof_reasons": fields.get("stale_non_proof_reasons").cloned().unwrap_or_else(|| json!([])),
        "claimability_effect": fields.get("claimability_effect").cloned().unwrap_or_else(|| json!("not_applicable")),
        "severity_effect": fields.get("severity_effect").cloned().unwrap_or(Value::Null),
    })
}

fn agent_use_graph_validation_status(
    source: &Value,
    graph_proof_available: bool,
    graph_status: &str,
) -> String {
    if let Some(status) = source
        .pointer("/validation_packet/status")
        .and_then(Value::as_str)
        .or_else(|| {
            source
                .pointer("/validation_packet/final_status")
                .and_then(Value::as_str)
        })
        .or_else(|| source.get("validation_status").and_then(Value::as_str))
    {
        return agent_use_normalize_graph_validation_status(status).to_string();
    }
    if source.get("new_graph_valid").and_then(Value::as_bool) == Some(true) {
        return "ok".to_string();
    }
    if source.get("new_graph_valid").and_then(Value::as_bool) == Some(false) {
        return "unknown".to_string();
    }
    if graph_proof_available && graph_status == "fresh" {
        return "ok".to_string();
    }
    if matches!(
        graph_status,
        "corrupt" | "inaccessible" | "permission_denied" | "stale" | "missing" | "absent"
    ) {
        return "unknown".to_string();
    }
    "not_applicable".to_string()
}

fn agent_use_normalize_graph_validation_status(status: &str) -> &'static str {
    match status {
        "ok" => "ok",
        "no_op" => "no_op",
        "updated" => "updated",
        "diagnostic_only" => "diagnostic_only",
        "not_applicable" => "not_applicable",
        "warning" | "warnings" => "warning",
        "blocking" | "blocking_graph_error" | "blocked" => "blocking",
        "unknown" | "unknown_with_recovery" => "unknown",
        "tool_error" | "lifecycle_error" | "unsafe_db" => "unknown",
        _ => "unknown",
    }
}

fn agent_use_candidate_recall_status(sidecar_statuses: &Value) -> &'static str {
    let keys = [
        "candidate_layer_status",
        "candidate_spool_query_index_status",
        "vector_layer_status",
        "path_evidence_status",
        "source_navigation_status",
        "routing_handle_status",
    ];
    let mut saw_applicable = false;
    let mut saw_fresh = false;
    for key in keys {
        let status = sidecar_statuses
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        match status {
            "fresh" => {
                saw_applicable = true;
                saw_fresh = true;
            }
            "not_applicable" | "diagnostic_only" => {}
            "missing" | "stale" | "truncated" | "partial" | "corrupt" | "inaccessible"
            | "permission_denied" | "rebuilding" | "publishing" | "unknown" => return "degraded",
            _ => return "degraded",
        }
    }
    if saw_fresh {
        "fresh"
    } else if saw_applicable {
        "fresh"
    } else {
        "not_applicable"
    }
}

fn agent_use_agent_action_for_dirty_evidence(
    graph_validation_status: &str,
    graph_db_corrupt_or_unavailable: bool,
    graph_proof_available: bool,
) -> &'static str {
    match graph_validation_status {
        "blocking" => "fix_blockers",
        "warning" => "inspect_warnings",
        "unknown" if graph_db_corrupt_or_unavailable || !graph_proof_available => "refresh_index",
        "unknown" => "inspect_unknowns",
        _ => "continue",
    }
}

fn agent_use_append_dirty_evidence_expansion_handle(value: &mut Value, full_detail: bool) {
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

fn agent_use_layer_freshness(staged_availability: &Value, layer_name: &str) -> Value {
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
    json!(agent_use_normalize_dirty_freshness_status(status))
}

fn agent_use_candidate_query_index_freshness(staged_availability: &Value) -> Value {
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
    json!(agent_use_normalize_dirty_freshness_status(status))
}

fn agent_use_path_evidence_freshness(source: &Value) -> Value {
    let action = source
        .get("path_evidence_invalidated")
        .and_then(|value| value.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    json!(match action {
        "refreshed" | "status_checked" | "unchanged" => "fresh",
        "invalidated" => "stale",
        "rebuilt" => "fresh",
        "not_applicable" => "not_applicable",
        "absent" => "missing",
        "error" => "inaccessible",
        _ => "unknown",
    })
}

fn agent_use_source_navigation_freshness(
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

fn agent_use_routing_handle_freshness(source: &Value) -> Value {
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

fn agent_use_normalize_dirty_freshness_status(status: &str) -> &'static str {
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

fn agent_use_output_claimability_effect(
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

fn agent_use_proof_ladder_change_counts(proof_ladder_changes: &Value) -> Value {
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
            let level = agent_use_proof_ladder_level_for_key(key);
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
    let mut value = serde_json::Map::new();
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

fn agent_use_proof_ladder_level_for_key(key: &str) -> &'static str {
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

fn agent_use_invalidated_evidence_statuses(source: &Value, proof_ladder_changes: &Value) -> Value {
    let mut surfaces = Vec::new();
    if source
        .get("text_evidence_changed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        surfaces.push(agent_use_surface_status(
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
            surfaces.push(agent_use_surface_status(
                surface,
                level,
                "stale",
                if level == "graph_relation_proof" {
                    "graph_proof_unavailable"
                } else {
                    "non_proof_evidence_only"
                },
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
                let level = agent_use_proof_ladder_level_for_key(surface);
                surfaces.push(agent_use_surface_status(
                    surface,
                    level,
                    "stale",
                    agent_use_surface_claimability_effect(level, false),
                    level == "graph_relation_proof",
                    None,
                ));
            }
        }
    }
    Value::Array(surfaces)
}

fn agent_use_refreshed_evidence_statuses(source: &Value, proof_ladder_changes: &Value) -> Value {
    let mut surfaces = Vec::new();
    if source
        .get("text_evidence_changed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        surfaces.push(agent_use_surface_status(
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
            surfaces.push(agent_use_surface_status(
                surface,
                level,
                "fresh",
                agent_use_surface_claimability_effect(level, true),
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
                let level = agent_use_proof_ladder_level_for_key(surface);
                surfaces.push(agent_use_surface_status(
                    surface,
                    level,
                    "fresh",
                    agent_use_surface_claimability_effect(level, true),
                    level == "graph_relation_proof",
                    None,
                ));
            }
        }
    }
    Value::Array(surfaces)
}

fn agent_use_stale_evidence_statuses(
    staged_availability: &Value,
    proof_ladder_changes: &Value,
) -> Value {
    let mut surfaces = agent_use_stale_candidate_layers(staged_availability)
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
            agent_use_surface_status(
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
        surfaces.push(agent_use_surface_status(
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
            surfaces.push(agent_use_surface_status(
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

fn agent_use_unavailable_evidence_statuses(staged_availability: &Value) -> Value {
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
        let freshness = agent_use_normalize_dirty_freshness_status(status);
        if matches!(
            freshness,
            "missing" | "corrupt" | "inaccessible" | "permission_denied"
        ) {
            surfaces.push(agent_use_surface_status(
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
            let query_freshness = agent_use_normalize_dirty_freshness_status(query_status);
            if matches!(
                query_freshness,
                "missing" | "corrupt" | "inaccessible" | "permission_denied"
            ) {
                surfaces.push(agent_use_surface_status(
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

fn agent_use_surface_claimability_effect(level: &str, fresh: bool) -> &'static str {
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

fn agent_use_surface_status(
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

fn agent_use_stale_non_proof_reasons(
    stale_evidence: &Value,
    unavailable_evidence: &Value,
) -> Value {
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

fn agent_use_any_sidecar_corrupt_or_unavailable(sidecar_statuses: &Value) -> bool {
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

fn agent_use_count_fresh_dirty_statuses(sidecar_statuses: &Value) -> usize {
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

fn agent_use_count_status(sidecar_statuses: &Value, status: &str) -> usize {
    sidecar_statuses
        .as_object()
        .into_iter()
        .flat_map(|object| object.values())
        .filter(|value| value.as_str() == Some(status))
        .count()
}

pub(crate) fn add_agent_use_rtds_freshness_fields(
    value: &mut Value,
    profile: &AgentUseProfile,
    preflight: &DbLifecyclePreflight,
    staged_availability: &Value,
) {
    let rtds = agent_use_rtds_freshness_json(profile, preflight, staged_availability);
    if let Some(object) = value.as_object_mut() {
        object.insert("rtds_freshness".to_string(), rtds.clone());
        object.insert(
            "graph_freshness".to_string(),
            rtds.get("graph_freshness").cloned().unwrap_or(Value::Null),
        );
        object
            .entry("delta_state".to_string())
            .or_insert_with(|| rtds.get("delta_state").cloned().unwrap_or(Value::Null));
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
            "last_delta_update_summary".to_string(),
            rtds.get("last_delta_update_summary")
                .cloned()
                .unwrap_or(Value::Null),
        );
        object.insert(
            "lock_state".to_string(),
            rtds.get("lock_state").cloned().unwrap_or(Value::Null),
        );
        object.insert(
            "update_queue_state".to_string(),
            rtds.get("update_queue_state")
                .cloned()
                .unwrap_or(Value::Null),
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
                .unwrap_or_else(|| json!(profile.recovery_commands.clone())),
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
        object.insert(
            "active_candidate_sources".to_string(),
            staged_availability
                .get("active_candidate_sources")
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
    }
}

pub(crate) fn agent_use_rtds_freshness_json(
    profile: &AgentUseProfile,
    preflight: &DbLifecyclePreflight,
    staged_availability: &Value,
) -> Value {
    let publish_state = agent_use_publish_state_json(profile);
    let publish_active = publish_state
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let publish_status = publish_state
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("absent");
    let last_delta = agent_use_last_delta_state_json(profile);
    let last_summary = last_delta
        .get("last_delta_update_summary")
        .cloned()
        .unwrap_or(Value::Null);
    let graph_freshness = if preflight.safe {
        "current".to_string()
    } else if preflight.path_access_status == "db_missing" {
        "absent".to_string()
    } else {
        preflight
            .db_problem_kind
            .clone()
            .unwrap_or_else(|| "unavailable".to_string())
    };
    let dirty_state = if publish_active {
        publish_status.to_string()
    } else if !preflight.safe {
        graph_freshness.clone()
    } else {
        "ready".to_string()
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
    for blocker in &preflight.blockers {
        blocked_labels.insert(blocker.clone());
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
    let stale_candidate_layers = agent_use_stale_candidate_layers(staged_availability);
    let blocked_labels = blocked_labels.into_iter().collect::<Vec<_>>();
    let retryable_labels = retryable_labels.into_iter().collect::<Vec<_>>();
    let lock_state = agent_use_profile_lock_state_json(
        profile,
        publish_active,
        publish_status,
        &blocked_labels,
        &retryable_labels,
        None,
    );
    let update_queue_state = agent_use_profile_update_queue_state_json(
        publish_active,
        publish_status,
        last_summary.clone(),
        Value::Null,
        0,
        0,
        0,
        Value::Null,
    );
    json!({
        "schema_version": 1,
        "profile_name": profile.profile_name.clone(),
        "repo_root": path_string(&profile.repo_root),
        "db_path": path_string(&profile.db_path),
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
        "blocked_labels": blocked_labels,
        "retryable_labels": retryable_labels,
        "lock_state": lock_state,
        "update_queue_state": update_queue_state,
        "recovery_commands": profile.recovery_commands.clone(),
        "context_pack_graph_proof_policy": "refuse_unsafe_graph_proof",
        "candidate_context_policy": "candidate_only_only_when_current_source_bound",
        "startup_auto_index": false,
        "dot_codegraph_fallback": false,
        "public_claim": false,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn agent_use_profile_update_queue_state_json(
    publish_active: bool,
    publish_status: &str,
    last_update_summary: Value,
    pending_paths: Value,
    queue_depth: usize,
    updates_attempted: usize,
    updates_succeeded: usize,
    last_error: Value,
) -> Value {
    json!({
        "queue_scope": "repo_profile",
        "persistent_watch_attached": false,
        "writer_queue_serialized": true,
        "max_concurrent_writers": 1,
        "queue_depth": queue_depth,
        "pending_paths": pending_paths,
        "updates_attempted": updates_attempted,
        "updates_succeeded": updates_succeeded,
        "active_update": publish_active,
        "status": if publish_active { publish_status } else { "idle" },
        "last_update_summary": last_update_summary,
        "last_error": last_error,
        "auto_index_enabled": false,
        "old_db_preserved": true,
        "temp_db_claimable": false,
        "unrelated_repo_blocking": false,
    })
}

pub(crate) fn agent_use_profile_lock_state_json(
    profile: &AgentUseProfile,
    publish_active: bool,
    publish_status: &str,
    blocked_labels: &[String],
    retryable_labels: &[String],
    lock_retry_count: Option<usize>,
) -> Value {
    let db_locked = blocked_labels
        .iter()
        .chain(retryable_labels.iter())
        .any(|label| {
            let label = label.to_ascii_lowercase();
            label.contains("db_locked") || label.contains("locked")
        });
    json!({
        "scope": "repo_profile",
        "profile_name": profile.profile_name.clone(),
        "repo_root": path_string(&profile.repo_root),
        "repo_identity_label": profile.repo_identity_label.clone(),
        "repo_identity_hash": profile.repo_identity_hash.clone(),
        "profile_root": path_string(&profile.profile_root),
        "db_path": path_string(&profile.db_path),
        "lock_or_publish_state_path": path_string(&profile.lock_or_publish_state_path),
        "writer_queue_serialized": true,
        "max_concurrent_writers": 1,
        "active_update": publish_active,
        "publish_status": publish_status,
        "db_locked": db_locked,
        "retryable": !retryable_labels.is_empty() || db_locked,
        "retryable_labels": retryable_labels,
        "blocked_labels": blocked_labels,
        "lock_retry_count": lock_retry_count.map(Value::from).unwrap_or(Value::Null),
        "global_lock_scope": "none_for_repo_profile_paths",
        "unrelated_repo_blocking": false,
        "old_db_preserved": true,
        "temp_db_claimable": false,
        "no_dot_codegraph_fallback": true,
    })
}

pub(crate) fn agent_use_stale_candidate_layers(staged_availability: &Value) -> Vec<Value> {
    let mut layers = Vec::new();
    for layer_name in ["candidate_spool", "vector_runtime", "vector_audit"] {
        let layer = staged_availability
            .pointer(&format!("/layer_readiness/{layer_name}"))
            .unwrap_or(&Value::Null);
        let status = layer
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if agent_use_candidate_layer_status_is_stale(status) {
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
            if agent_use_candidate_layer_status_is_stale(query_status) {
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

pub(crate) fn agent_use_candidate_layer_status_is_stale(status: &str) -> bool {
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

pub(crate) fn plain_status_agent_use_guidance_json(repo_root: &Path) -> Value {
    match resolve_agent_use_profile(repo_root) {
        Ok(profile) => {
            let repo = path_string(&profile.repo_root);
            json!({
                "agent_use_available": true,
                "agent_use_status_command": format!("{BIN_NAME} agent-use status --repo \"{repo}\" --json"),
                "agent_use_index_command": format!("{BIN_NAME} agent-use index --repo \"{repo}\" --json"),
                "agent_use_profile_name": profile.profile_name.clone(),
                "agent_use_profile_db_path": path_string(&profile.db_path),
                "agent_use_note": "plain status remains local .codegraph mode and did not redirect",
            })
        }
        Err(error) => json!({
            "agent_use_available": false,
            "agent_use_status_command": Value::Null,
            "agent_use_index_command": Value::Null,
            "agent_use_profile_error": error,
            "agent_use_note": "plain status remains local .codegraph mode and did not redirect",
        }),
    }
}

pub(crate) fn with_agent_use_profile_context<F>(
    profile: &AgentUseProfile,
    operation: F,
) -> Result<Value, String>
where
    F: FnOnce() -> Result<Value, String>,
{
    with_process_context_lock(|| {
        let _process_context = ProcessContextSnapshot::capture()?;
        std::env::set_current_dir(&profile.repo_root).map_err(|error| error.to_string())?;
        std::env::set_var("CODEGRAPH_DB_PATH", &profile.db_path);
        std::env::set_var(GLOBAL_DB_SOURCE_ENV, "agent-use profile");
        std::env::set_var(GLOBAL_REPO_SOURCE_ENV, "agent-use --repo");
        std::env::set_var(AGENT_USE_BOUNDED_READ_PATH_ENV, "1");
        operation()
    })
}

pub(crate) fn agent_use_bounded_read_path_enabled() -> bool {
    std::env::var_os(AGENT_USE_BOUNDED_READ_PATH_ENV).is_some()
}

pub(crate) fn agent_use_read_path_limits_json(limit: usize) -> Value {
    json!({
        "max_files_inspected": limit,
        "max_entities_hydrated": limit,
        "max_edges_visited": limit,
        "max_source_bytes_loaded": AGENT_USE_CONTEXT_MAX_SOURCE_BYTES,
        "max_snippets_loaded": limit,
        "max_disk_fallback_files": AGENT_USE_DISK_FALLBACK_MAX_FILES,
        "timeout_ms": Value::Null,
    })
}

pub(crate) fn agent_use_status_read_path_metrics_json(wall_ms: f64) -> Value {
    json!({
        "schema_version": 1,
        "surface": "agent-use status",
        "lookup_strategy": "db_lifecycle_preflight_and_passport_summary",
        "indexed_lookup_count": 1,
        "fts_lookup_count": 0,
        "path_dictionary_lookup_count": 0,
        "symbol_dictionary_lookup_count": 0,
        "full_scan_count": 0,
        "entity_edge_million_row_load": false,
        "source_file_load_count": AGENT_USE_STATUS_MAX_SOURCE_FILE_LOADS,
        "entities_hydrated": 0,
        "edges_hydrated": 0,
        "source_bytes_loaded": 0,
        "snippets_loaded": 0,
        "disk_fallback_used": false,
        "disk_fallback_files": 0,
        "debug_broad_scan": false,
        "diagnostic_only": false,
        "limits_apply_before_hydration": true,
        "budget_hit": false,
        "elapsed_ms": wall_ms,
        "p50_ms": Value::Null,
        "p95_ms": Value::Null,
        "latency_window": "single_command; aggregate p50/p95 is emitted by the release smoke",
        "limits": agent_use_read_path_limits_json(0),
    })
}

pub(crate) fn agent_use_query_read_path_metrics_json(kind: &str, value: &Value) -> Value {
    let limit = value
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|limit| usize::try_from(limit).ok())
        .unwrap_or(DEFAULT_QUERY_AGENT_JSON_LIMIT);
    let result_count = value
        .get("result_count")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or_default();
    let omitted_count = value
        .get("truncation")
        .and_then(|truncation| truncation.get("omitted_count"))
        .or_else(|| value.get("omitted_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let (
        lookup_strategy,
        fts_lookup_count,
        path_dictionary_lookup_count,
        symbol_dictionary_lookup_count,
        entities_hydrated,
    ) = match kind {
        "symbols" => (
            "symbol_dict_exact_lookup_plus_bounded_stage0_fts",
            1,
            0,
            1,
            result_count,
        ),
        "text" => ("stage0_fts_bounded_lookup", 1, 0, 0, 0),
        "files" => ("stage0_fts_file_path_title_lookup", 1, 1, 0, 0),
        "callers" | "callees" => ("bounded_call_relation_graph_lookup", 0, 0, 1, result_count),
        "path" | "chain" => ("bounded_graph_path_lookup", 0, 0, 2, result_count),
        "references" => (
            "bounded_symbol_reference_graph_lookup",
            0,
            0,
            1,
            result_count,
        ),
        "definitions" => ("bounded_symbol_definition_lookup", 0, 0, 1, result_count),
        "unresolved-calls" => ("bounded_unresolved_call_lookup", 0, 0, 0, result_count),
        _ => ("bounded_indexed_lookup", 0, 0, 0, result_count),
    };
    let hydrated_limit = match kind {
        "symbols" => limit
            .max(1)
            .saturating_mul(SYMBOL_SEARCH_FTS_CANDIDATE_FACTOR)
            .clamp(
                SYMBOL_SEARCH_MIN_FTS_CANDIDATES,
                SYMBOL_SEARCH_MAX_FTS_CANDIDATES,
            ),
        _ => limit.saturating_add(1).min(MAX_QUERY_RESULT_LIMIT + 1),
    };
    json!({
        "schema_version": 1,
        "surface": format!("agent-use query {kind}"),
        "lookup_strategy": lookup_strategy,
        "indexed_lookup_count": 1,
        "fts_lookup_count": fts_lookup_count,
        "path_dictionary_lookup_count": path_dictionary_lookup_count,
        "symbol_dictionary_lookup_count": symbol_dictionary_lookup_count,
        "full_scan_count": 0,
        "entity_edge_million_row_load": false,
        "source_file_load_count": AGENT_USE_QUERY_MAX_SOURCE_FILE_LOADS,
        "entities_hydrated": entities_hydrated,
        "edges_hydrated": 0,
        "source_bytes_loaded": 0,
        "snippets_loaded": 0,
        "disk_fallback_used": false,
        "disk_fallback_files": 0,
        "debug_broad_scan": false,
        "diagnostic_only": false,
        "limits_apply_before_hydration": true,
        "budget_hit": omitted_count > 0,
        "elapsed_ms": value
            .get("timings")
            .and_then(|timings| timings.get("wall_ms"))
            .cloned()
            .unwrap_or(Value::Null),
        "p50_ms": Value::Null,
        "p95_ms": Value::Null,
        "latency_window": "single_command; aggregate p50/p95 is emitted by the release smoke",
        "limits": json!({
            "max_files_inspected": if kind == "files" { limit.saturating_add(1).min(MAX_QUERY_RESULT_LIMIT + 1) } else { 0 },
            "max_entities_hydrated": hydrated_limit,
            "max_edges_visited": 0,
            "max_source_bytes_loaded": 0,
            "max_snippets_loaded": 0,
            "max_disk_fallback_files": AGENT_USE_DISK_FALLBACK_MAX_FILES,
            "timeout_ms": Value::Null,
        }),
    })
}

pub(crate) fn add_agent_use_query_read_path_metrics(value: &mut Value, kind: &str) {
    let metrics = agent_use_query_read_path_metrics_json(kind, value);
    if let Some(object) = value.as_object_mut() {
        object.insert("read_path_metrics".to_string(), metrics);
    }
}

pub(crate) fn add_agent_use_context_pack_read_path_metrics(value: &mut Value) {
    let path_telemetry = value
        .get("path_evidence_telemetry")
        .cloned()
        .or_else(|| {
            value
                .get("retrieval_explain")
                .and_then(|explain| explain.get("path_evidence_telemetry"))
                .cloned()
        })
        .unwrap_or(Value::Null);
    let source_telemetry_available = path_telemetry.is_object()
        && path_telemetry.get("status").and_then(Value::as_str) != Some("unavailable");
    let source_file_load_count = path_telemetry
        .get("snippet_source_files_loaded")
        .or_else(|| path_telemetry.get("source_files_loaded"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let source_bytes_loaded = path_telemetry
        .get("snippet_source_bytes_read")
        .or_else(|| path_telemetry.get("source_span_bytes_read"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let retained_snippet_files = context_pack_retained_snippet_files(value);
    let retained_snippet_bytes = context_pack_retained_snippet_bytes(value);
    let source_file_load_count = if source_file_load_count == 0 && retained_snippet_bytes > 0 {
        retained_snippet_files.len() as u64
    } else {
        source_file_load_count
    };
    let source_bytes_loaded = if source_bytes_loaded == 0 && retained_snippet_bytes > 0 {
        retained_snippet_bytes as u64
    } else {
        source_bytes_loaded
    };
    let snippets_loaded = value
        .get("snippets")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
        + value
            .get("fallback_snippets")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default();
    let edge_limit = value
        .get("limits")
        .and_then(|limits| limits.get("paths"))
        .and_then(Value::as_u64)
        .and_then(|limit| usize::try_from(limit).ok())
        .unwrap_or(DEFAULT_CONTEXT_AGENT_PATH_LIMIT)
        .saturating_mul(4)
        .max(16);
    let budget_hit = value
        .get("truncation")
        .and_then(|truncation| truncation.get("limit_applied"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || value
            .get("omitted_count")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            > 0;
    let metrics = json!({
        "schema_version": 1,
        "surface": "agent-use context-pack",
        "lookup_strategy": "stored_path_evidence_then_bounded_seed_edge_and_stage0_fts_fallback",
        "indexed_lookup_count": 3,
        "fts_lookup_count": if value.get("fallback_evidence_count").and_then(Value::as_u64).unwrap_or_default() > 0 { 1 } else { 0 },
        "path_dictionary_lookup_count": 1,
        "symbol_dictionary_lookup_count": 1,
        "full_scan_count": 0,
        "entity_edge_million_row_load": false,
        "source_file_load_count": source_file_load_count,
        "entities_hydrated": value
            .get("critical_symbols")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default(),
        "edges_hydrated": value
            .get("proof_paths")
            .or_else(|| value.get("paths"))
            .and_then(Value::as_array)
            .map(|paths| {
                paths.iter()
                    .filter_map(|path| path.get("edges").and_then(Value::as_array))
                    .map(Vec::len)
                    .sum::<usize>()
            })
            .unwrap_or_default(),
        "source_bytes_loaded": source_bytes_loaded,
        "source_file_load_count_measurement": if source_telemetry_available { "measured" } else { "retained_snippet_lower_bound" },
        "source_bytes_loaded_measurement": if source_telemetry_available { "measured" } else { "retained_snippet_lower_bound" },
        "snippets_loaded": snippets_loaded,
        "disk_fallback_used": false,
        "disk_fallback_files": 0,
        "debug_broad_scan": false,
        "diagnostic_only": false,
        "limits_apply_before_hydration": true,
        "budget_hit": budget_hit,
        "elapsed_ms": value
            .get("timings")
            .and_then(|timings| timings.get("wall_ms"))
            .cloned()
            .unwrap_or(Value::Null),
        "p50_ms": Value::Null,
        "p95_ms": Value::Null,
        "latency_window": "single_command; aggregate p50/p95 is emitted by the release smoke",
        "limits": json!({
            "max_files_inspected": AGENT_USE_CONTEXT_MAX_SOURCE_FILES,
            "max_entities_hydrated": DEFAULT_CONTEXT_AGENT_PATH_LIMIT.saturating_mul(16).max(16),
            "max_edges_visited": edge_limit,
            "max_source_bytes_loaded": AGENT_USE_CONTEXT_MAX_SOURCE_BYTES,
            "max_snippets_loaded": value
                .get("limits")
                .and_then(|limits| limits.get("snippets"))
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_CONTEXT_AGENT_SNIPPET_LIMIT as u64),
            "max_disk_fallback_files": AGENT_USE_DISK_FALLBACK_MAX_FILES,
            "timeout_ms": Value::Null,
        }),
    });
    if let Some(object) = value.as_object_mut() {
        object.insert("read_path_metrics".to_string(), metrics);
    }
}

pub(crate) fn context_pack_retained_snippet_files(value: &Value) -> BTreeSet<String> {
    let mut files = BTreeSet::new();
    for key in ["snippets", "fallback_snippets"] {
        if let Some(snippets) = value.get(key).and_then(Value::as_array) {
            for snippet in snippets {
                if let Some(file) = snippet.get("file").and_then(Value::as_str) {
                    files.insert(file.to_string());
                }
            }
        }
    }
    files
}

pub(crate) fn context_pack_retained_snippet_bytes(value: &Value) -> usize {
    let mut bytes = 0usize;
    for key in ["snippets", "fallback_snippets"] {
        if let Some(snippets) = value.get(key).and_then(Value::as_array) {
            for snippet in snippets {
                if let Some(text) = snippet.get("text").and_then(Value::as_str) {
                    bytes = bytes.saturating_add(text.len());
                }
            }
        }
    }
    bytes
}

pub(crate) fn annotate_agent_use_output(
    value: &mut Value,
    profile: &AgentUseProfile,
    command: &str,
    normal_dot_codegraph_existed_before: bool,
) {
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let Some(object) = value.as_object_mut() else {
        return;
    };
    object.insert("command_namespace".to_string(), json!("agent-use"));
    object.insert(
        "agent_use_command".to_string(),
        json!(format!("agent-use {command}")),
    );
    object.insert(
        "profile_name".to_string(),
        json!(profile.profile_name.clone()),
    );
    object.insert(
        "agent_use_profile_name".to_string(),
        json!(profile.profile_name.clone()),
    );
    object.insert("repo".to_string(), json!(path_string(&profile.repo_root)));
    object.insert(
        "repo_root".to_string(),
        json!(path_string(&profile.repo_root)),
    );
    object.insert(
        "repo_identity_label".to_string(),
        json!(profile.repo_identity_label.clone()),
    );
    object.insert(
        "repo_identity_hash".to_string(),
        json!(profile.repo_identity_hash.clone()),
    );
    object.insert(
        "repo_identity_short_hash".to_string(),
        json!(agent_use_repo_identity_short_hash(profile)),
    );
    object.insert("db".to_string(), json!(path_string(&profile.db_path)));
    object.insert("db_path".to_string(), json!(path_string(&profile.db_path)));
    object.insert(
        "resolved_db".to_string(),
        json!(path_string(&profile.db_path)),
    );
    object.insert("db_source".to_string(), json!("agent-use profile"));
    object.insert("external_db_used".to_string(), json!(true));
    object.insert(
        "agent_use_profile_root".to_string(),
        json!(path_string(&profile.profile_root)),
    );
    object.insert(
        "publish_state".to_string(),
        agent_use_publish_state_json(profile),
    );
    object.insert(
        "publishing".to_string(),
        json!(agent_use_publish_state_active(profile)),
    );
    object.insert(
        "normal_dot_codegraph_path".to_string(),
        json!(path_string(&normal_dot_codegraph)),
    );
    object.insert(
        "normal_dot_codegraph_created".to_string(),
        json!(!normal_dot_codegraph_existed_before && normal_dot_codegraph.exists()),
    );
    object.insert(
        "normal_dot_codegraph_mutated".to_string(),
        json!(normal_dot_codegraph_existed_before != normal_dot_codegraph.exists()),
    );
    object.insert("public_claim".to_string(), json!(false));
}

pub(crate) fn add_agent_use_staged_availability(
    value: &mut Value,
    profile: &AgentUseProfile,
    preflight: &DbLifecyclePreflight,
) {
    let staged_availability = staged_availability_for_cli(
        &profile.repo_root,
        &profile.db_path,
        Some(preflight),
        Some(&profile.candidate_spool_path),
        Some(&profile.vector_runtime_path),
        Some(&profile.vector_audit_path),
        None,
    );
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "staged_availability".to_string(),
            staged_availability.clone(),
        );
    }
    merge_json_object(
        value,
        staged_availability_top_level_fields(&staged_availability),
    );
    add_agent_use_dirty_evidence_output_fields(value, "agent-use.status", false);
}

pub(crate) fn add_agent_use_db_lifecycle_read(value: &mut Value, preflight: &DbLifecyclePreflight) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    object
        .entry("db_lifecycle_read".to_string())
        .or_insert_with(|| db_lifecycle_preflight_json(preflight, true, false, false));
}

pub(crate) fn add_agent_use_durability_labels(
    value: &mut Value,
    profile: &AgentUseProfile,
    preflight: &DbLifecyclePreflight,
    sqlite_sidecars: Option<&Value>,
) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    object.insert(
        "safety_labels".to_string(),
        json!(agent_use_safety_labels(profile, preflight, sqlite_sidecars)),
    );
    object.insert(
        "publish_state".to_string(),
        agent_use_publish_state_json(profile),
    );
    object.insert(
        "publishing".to_string(),
        json!(agent_use_publish_state_active(profile)),
    );
}

pub(crate) fn agent_use_unavailable_json(
    profile: &AgentUseProfile,
    preflight: &DbLifecyclePreflight,
    command: &str,
    subcommand: Option<&str>,
    extra_error: Option<String>,
    normal_dot_codegraph_existed_before: bool,
) -> Value {
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let sqlite_sidecars =
        sqlite_sidecars_status_from_health(&profile.db_path, &preflight.db_health);
    let lifecycle = db_lifecycle_preflight_json(preflight, true, false, false);
    let staged_availability = staged_availability_for_cli(
        &profile.repo_root,
        &profile.db_path,
        Some(preflight),
        Some(&profile.candidate_spool_path),
        Some(&profile.vector_runtime_path),
        Some(&profile.vector_audit_path),
        None,
    );
    let mut errors = preflight.blockers.clone();
    if let Some(extra_error) = extra_error {
        errors.push(extra_error);
    }
    let status = if preflight.path_access_status == "db_missing" {
        "not_indexed"
    } else {
        preflight
            .db_problem_kind
            .as_deref()
            .unwrap_or("unavailable")
    };
    let mut value = json!({
        "status": status,
        "command": command,
        "subcommand": subcommand,
        "command_namespace": "agent-use",
        "profile_name": profile.profile_name.clone(),
        "repo": path_string(&profile.repo_root),
        "repo_root": path_string(&profile.repo_root),
        "repo_identity_label": profile.repo_identity_label.clone(),
        "repo_identity_hash": profile.repo_identity_hash.clone(),
        "repo_identity_short_hash": agent_use_repo_identity_short_hash(profile),
        "db": path_string(&profile.db_path),
        "db_path": path_string(&profile.db_path),
        "resolved_db": path_string(&profile.db_path),
        "db_source": "agent-use profile",
        "external_db_used": true,
        "claimable": false,
        "diagnostic_only": true,
        "db_problem_kind": preflight.db_problem_kind.clone(),
        "path_access_status": preflight.path_access_status.clone(),
        "path_access_error": preflight.path_access_error.clone(),
        "db_lifecycle_read": lifecycle.clone(),
        "lifecycle": lifecycle,
        "publish_state": agent_use_publish_state_json(profile),
        "publishing": agent_use_publish_state_active(profile),
        "safety_labels": agent_use_safety_labels(profile, preflight, Some(&sqlite_sidecars)),
        "sqlite_sidecars": sqlite_sidecars.clone(),
        "sidecar_status": sqlite_sidecars["sidecar_status"].clone(),
        "sidecar_only_change": sqlite_sidecars["sidecar_only_change"].clone(),
        "sidecar_change_classification": sqlite_sidecars["sidecar_change_classification"].clone(),
        "staged_availability": staged_availability.clone(),
        "recovery": agent_use_recovery_json(profile),
        "warnings": preflight.warnings.clone(),
        "errors": errors.clone(),
        "normal_dot_codegraph_path": path_string(&normal_dot_codegraph),
        "normal_dot_codegraph_created": !normal_dot_codegraph_existed_before && normal_dot_codegraph.exists(),
        "normal_dot_codegraph_mutated": normal_dot_codegraph_existed_before != normal_dot_codegraph.exists(),
        "public_claim": false,
    });
    merge_json_object(
        &mut value,
        staged_availability_top_level_fields(&staged_availability),
    );
    add_agent_use_dirty_evidence_output_fields(&mut value, command, false);
    if command == "context-pack" {
        attach_unavailable_patch_assist_packet(
            &mut value,
            profile,
            &staged_availability,
            &errors,
            status,
        );
    }
    value
}

pub(crate) fn agent_use_watch_unavailable_json(
    profile: &AgentUseProfile,
    preflight: &DbLifecycleSurfacePreflight,
    normal_dot_codegraph_existed_before: bool,
    changed_paths: &[PathBuf],
) -> Value {
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let lifecycle = watch_lifecycle_status_json(preflight, &profile.db_path, false);
    let db_lifecycle_read =
        db_lifecycle_preflight_json(&preflight.lifecycle_preflight, true, false, false);
    let changed_paths_requested = changed_paths
        .iter()
        .map(|path| path_string(path))
        .collect::<Vec<_>>();
    let status = if preflight.path_access_status == "db_missing" {
        "not_indexed"
    } else {
        preflight
            .db_problem_kind
            .as_deref()
            .unwrap_or("unavailable")
    };
    json!({
        "status": status,
        "command": "watch",
        "subcommand": "once",
        "command_namespace": "agent-use",
        "agent_use_command": "agent-use watch",
        "profile_name": profile.profile_name.clone(),
        "repo": path_string(&profile.repo_root),
        "repo_root": path_string(&profile.repo_root),
        "repo_identity_label": profile.repo_identity_label.clone(),
        "repo_identity_hash": profile.repo_identity_hash.clone(),
        "repo_identity_short_hash": agent_use_repo_identity_short_hash(profile),
        "db": path_string(&profile.db_path),
        "db_path": path_string(&profile.db_path),
        "resolved_db": path_string(&profile.db_path),
        "db_source": "agent-use profile",
        "external_db_used": true,
        "claimable": false,
        "diagnostic_only": true,
        "path_access_status": preflight.path_access_status.clone(),
        "path_access_error": preflight.path_access_error.clone(),
        "db_problem_kind": preflight.db_problem_kind.clone(),
        "watch_mode": "once_changed",
        "watch_db": lifecycle.clone(),
        "db_lifecycle_read": db_lifecycle_read,
        "lifecycle": lifecycle,
        "agent_use_watch_available": true,
        "agent_use_watch_status": "implemented_once_changed",
        "delta_sync_phase": "real_time_delta_sync",
        "delta_sync_state": "blocked",
        "delta_state": "blocked",
        "auto_index_enabled": false,
        "validation_status": "not_applicable",
        "validation_must_fix_before_continuing": false,
        "validation_blocking_error_count": 0,
        "validation_warning_count": 0,
        "validation_unknown_count": 0,
        "hard_interrupt_available": false,
        "hard_interrupt": Value::Null,
        "hard_interrupt_not_implemented": false,
        "changed_paths": changed_paths_requested,
        "rejected_paths": [],
        "no_op_paths": [],
        "changed_paths_requested": changed_paths_requested,
        "old_graph_valid": false,
        "new_graph_valid": false,
        "old_db_preserved": true,
        "temp_db_claimable": false,
        "files_walked": 0,
        "files_read": 0,
        "files_hashed": 0,
        "files_parsed": 0,
        "facts_deleted": 0,
        "facts_inserted": 0,
        "entities_added": 0,
        "entities_removed": 0,
        "entities_changed": 0,
        "edges_added": 0,
        "edges_removed": 0,
        "edges_changed": 0,
        "source_spans_added": 0,
        "source_spans_removed": 0,
        "source_spans_changed": 0,
        "text_evidence_changed": false,
        "path_evidence_invalidated": {
            "action": "unchanged",
            "dirty_path_evidence_count": 0,
            "delta_count": 0,
            "graph_proof": false,
        },
        "candidate_spool_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "candidate_spool_invalidated_or_refreshed": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "candidate_query_index_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "candidate_query_index_invalidated_or_refreshed": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "vector_chunks_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "vector_chunks_invalidated": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "vector_runtime_status_changed": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "vector_audit_status_changed": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "nuance_tokens_invalidated": {
            "action": "not_applicable",
            "status": "not_applicable",
            "reason": "nuance_rescue_candidates_are_request_time_context_candidates",
            "graph_proof": false,
        },
        "routing_handles_invalidated": {
            "action": "unchanged",
            "scope": "none",
        },
        "routing_handles_invalidated_or_not_applicable": {
            "action": "unchanged",
            "scope": "none",
            "graph_proof": false,
        },
        "proof_ladder_changes": {},
        "closure_files_considered": [],
        "closure_budget_hit": false,
        "degraded_relation_classes": [],
        "timings": {},
        "claimability": {
            "claimable": false,
            "diagnostic_only": true,
            "candidate_only": false,
            "graph_proof_available": false,
        },
        "publish_state": agent_use_publish_state_json(profile),
        "publishing": agent_use_publish_state_active(profile),
        "safety_labels": agent_use_safety_labels(profile, &preflight.lifecycle_preflight, None),
        "recovery": agent_use_recovery_json(profile),
        "recovery_commands": profile.recovery_commands.clone(),
        "errors": preflight.blockers.clone(),
        "warnings": preflight.warnings.clone(),
        "normal_dot_codegraph_path": path_string(&normal_dot_codegraph),
        "normal_dot_codegraph_created": !normal_dot_codegraph_existed_before && normal_dot_codegraph.exists(),
        "normal_dot_codegraph_mutated": normal_dot_codegraph_existed_before != normal_dot_codegraph.exists(),
        "public_claim": false,
        "publish_safety": {
            "strategy": "no_update_when_profile_db_is_not_safe_to_write",
            "old_good_read_visibility": "no delta transaction started",
            "partial_update_claimability": "not_applicable",
            "temp_db_claimability": "not_applicable_for_once_delta_update",
            "temp_db_claimable": false,
            "auto_index_on_start": false,
        },
    })
}

pub(crate) fn discover_agent_use_binary_path(repo_root: &Path) -> String {
    if let Ok(current_exe) = std::env::current_exe() {
        if current_exe
            .file_stem()
            .and_then(|value| value.to_str())
            .is_some_and(|stem| stem == "codegraph-mcp")
        {
            return path_string(&current_exe);
        }
    }
    let release_binary = repo_root
        .join("target")
        .join("release")
        .join(if cfg!(windows) {
            "codegraph-mcp.exe"
        } else {
            "codegraph-mcp"
        });
    if release_binary.exists() {
        path_string(&release_binary)
    } else {
        BIN_NAME.to_string()
    }
}
