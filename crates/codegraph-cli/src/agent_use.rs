//! Agent-use command surface (status/index/watch/mcp-config/query plumbing)
//! and its profile/lifecycle/RTDS helpers.
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::*;

pub(crate) fn run_agent_use_command(args: &[String]) -> Result<Value, String> {
    let Some(subcommand) = args.first() else {
        return Err(
            "Usage: codegraph-mcp agent-use <status|index|query|context-pack|mcp-config|watch> --repo <repo> --json"
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
        other => Err(format!(
            "unknown agent-use command: {other}; expected status, index, query, context-pack, mcp-config, or watch"
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
    debounce: Duration,
    max_updates: Option<usize>,
    idle_timeout: Option<Duration>,
    lock_retries: usize,
    lock_retry: Duration,
    max_batch_paths: usize,
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
        debounce,
        max_updates,
        idle_timeout,
        lock_retries,
        lock_retry,
        max_batch_paths,
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
    std::env::var(WRITE_PATH_CHAOS_FAILPOINT_ENV)
        .ok()
        .is_some_and(|raw| {
            raw.split(',')
                .map(str::trim)
                .any(|value| value == name || value == "agent_use_profile_all")
        })
}

pub(crate) fn run_agent_use_query_command(args: &[String]) -> Result<Value, String> {
    let options = parse_agent_use_forward_args(args, "query")?;
    let Some(query_kind) = options.forwarded_args.first().cloned() else {
        return Err(
            "Usage: codegraph-mcp agent-use query <symbols|text|files|references|definitions|callers|callees|path|chain|unresolved-calls> <args> --repo <repo> --limit <n> --agent-json"
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
    let preflight = inspect_read_db_lifecycle_preflight(
        &profile.repo_root,
        &profile.db_path,
        Some(profile.scope_policy.clone()),
    )?;
    let mut forwarded_args = options.forwarded_args;
    let detail_mode = agent_use_forward_detail_mode(&forwarded_args);
    let max_output_bytes = agent_use_context_max_output_bytes(&forwarded_args, detail_mode);
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
    }
    merge_json_object(&mut value, staged_fields);
    add_agent_use_rtds_freshness_fields(&mut value, &profile, &preflight, &staged_availability);
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
        return run_agent_use_watch_once_delta(
            &profile,
            options.changed_paths,
            normal_dot_codegraph_existed_before,
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

pub(crate) fn run_agent_use_watch_once_delta(
    profile: &AgentUseProfile,
    changed_paths: Vec<PathBuf>,
    normal_dot_codegraph_existed_before: bool,
) -> Result<Value, String> {
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let requested_changed_paths =
        agent_use_report_changed_paths(&profile.repo_root, &changed_paths);
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

    if !preflight.safe_to_write {
        return Ok(agent_use_watch_unavailable_json(
            profile,
            &preflight,
            normal_dot_codegraph_existed_before,
            &changed_paths,
        ));
    }
    let path_preflight = agent_use_watch_path_preflight(&profile.repo_root, &changed_paths);
    if !path_preflight.rejected_paths.is_empty() {
        return Ok(agent_use_watch_rejected_paths_json(
            profile,
            &preflight,
            normal_dot_codegraph_existed_before,
            &path_preflight,
        ));
    }

    let old_fact_counts =
        agent_use_delta_fact_counts(&profile.db_path, &requested_changed_paths).unwrap_or_default();
    write_agent_use_publish_state(profile, "updating", None)?;
    let summary = match update_changed_files_to_db(
        &profile.repo_root,
        &changed_paths,
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
                return Err(message);
            }
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
            return Err(message);
        }
    };
    let mut value = serde_json::to_value(&summary).map_err(|error| error.to_string())?;

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
    let changed_paths_requested = changed_paths
        .iter()
        .map(|path| path_string(path))
        .collect::<Vec<_>>();
    let changed_paths_normalized = if summary.changed_files.is_empty() {
        requested_changed_paths.clone()
    } else {
        summary.changed_files.clone()
    };
    let new_fact_counts = agent_use_delta_fact_counts(&profile.db_path, &changed_paths_normalized)
        .unwrap_or_default();
    let deleted_paths = agent_use_watch_deleted_paths(&summary);
    annotate_agent_use_output(
        &mut value,
        profile,
        "watch",
        normal_dot_codegraph_existed_before,
    );
    add_agent_use_durability_labels(&mut value, profile, &post_preflight, None);
    if let Some(object) = value.as_object_mut() {
        let no_op_paths = agent_use_watch_no_op_paths(&summary);
        let status = agent_use_watch_status(&summary, &no_op_paths);
        let delta_state = agent_use_watch_delta_state(&summary, &no_op_paths);
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
        object.insert("deleted_paths".to_string(), json!(deleted_paths.clone()));
        object.insert(
            "deleted_path".to_string(),
            if deleted_paths.len() == 1 {
                json!(deleted_paths[0])
            } else {
                Value::Null
            },
        );
        object.insert("rejected_paths".to_string(), json!([]));
        object.insert("no_op_paths".to_string(), json!(no_op_paths));
        object.insert(
            "changed_paths_requested".to_string(),
            json!(changed_paths_requested),
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
            json!(new_fact_counts.entities),
        );
        object.insert(
            "entities_removed".to_string(),
            json!(old_fact_counts.entities),
        );
        object.insert("entities_changed".to_string(), json!(summary.entities));
        object.insert("edges_added".to_string(), json!(new_fact_counts.edges));
        object.insert("edges_removed".to_string(), json!(old_fact_counts.edges));
        object.insert("edges_changed".to_string(), json!(summary.edges));
        object.insert(
            "source_spans_added".to_string(),
            json!(new_fact_counts.source_spans),
        );
        object.insert(
            "source_spans_removed".to_string(),
            json!(old_fact_counts.source_spans),
        );
        object.insert(
            "source_spans_changed".to_string(),
            json!(new_fact_counts.source_spans + old_fact_counts.source_spans),
        );
        object.insert(
            "text_evidence_changed".to_string(),
            json!(summary.files_indexed > 0 && summary.files_read > 0),
        );
        object.insert(
            "path_evidence_invalidated".to_string(),
            json!({
                "action": agent_use_path_evidence_delta_action(&summary),
                "dirty_path_evidence_count": summary.dirty_path_evidence_count,
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
            "candidate_query_index_invalidated_or_rebuilt".to_string(),
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
                "total_wall_ms": summary.profile.as_ref().map(|profile| profile.total_wall_ms),
                "file_read_ms": profile_span_ms(summary.profile.as_ref(), "file_read"),
                "file_hash_ms": profile_span_ms(summary.profile.as_ref(), "file_hash"),
                "parse_ms": profile_span_ms(summary.profile.as_ref(), "parse"),
                "stale_delete_ms": profile_span_ms(summary.profile.as_ref(), "stale_fact_delete"),
                "transaction_commit_ms": profile_span_ms(summary.profile.as_ref(), "transaction_commit"),
                "path_evidence_regeneration_ms": profile_span_ms(summary.profile.as_ref(), "refresh_path_evidence"),
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
    add_agent_use_rtds_freshness_fields(&mut value, profile, &post_preflight, &staged_availability);
    persist_agent_use_last_delta_state(&mut value, profile, "agent-use.watch.once");
    Ok(value)
}

pub(crate) fn run_agent_use_persistent_watch_command(
    options: AgentUseWatchOptions,
) -> Result<Value, String> {
    let profile = resolve_agent_use_profile(&options.repo)?;
    let normal_dot_codegraph = profile.repo_root.join(".codegraph");
    let normal_dot_codegraph_existed_before = normal_dot_codegraph.exists();
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
    normal_dot_codegraph_existed_before: bool,
    lock_retries: usize,
    lock_retry: Duration,
) -> Result<(Value, usize), String> {
    agent_use_retry_transient_lock(lock_retries, lock_retry, || {
        run_agent_use_watch_once_delta(
            profile,
            changed_paths.clone(),
            normal_dot_codegraph_existed_before,
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

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AgentUseDeltaFactCounts {
    entities: u64,
    edges: u64,
    source_spans: u64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AgentUseWatchPathPreflight {
    accepted_paths: Vec<String>,
    rejected_paths: Vec<Value>,
    requested_paths: Vec<String>,
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
) -> AgentUseWatchPathPreflight {
    let mut preflight = AgentUseWatchPathPreflight::default();
    for changed_path in changed_paths {
        let requested = path_to_graph_report_string(changed_path);
        preflight.requested_paths.push(requested.clone());
        match normalize_changed_path(repo_root, changed_path) {
            Ok((_, repo_relative_path)) => {
                if !preflight.accepted_paths.contains(&repo_relative_path) {
                    preflight.accepted_paths.push(repo_relative_path);
                }
            }
            Err(error) => {
                let mapping_path = if changed_path.is_absolute() {
                    changed_path.to_path_buf()
                } else {
                    repo_root.join(changed_path)
                };
                preflight.rejected_paths.push(json!({
                    "path": requested,
                    "requested_path": requested,
                    "original_path": path_string(changed_path),
                    "reason": "path_outside_repo",
                    "status": "rejected",
                    "read": false,
                    "indexed": false,
                    "path_mapping": agent_use_path_mapping_json(&mapping_path),
                    "error": error.to_string(),
                }));
            }
        }
    }
    preflight.accepted_paths.sort();
    preflight.accepted_paths.dedup();
    preflight
}

pub(crate) fn agent_use_watch_rejected_paths_json(
    profile: &AgentUseProfile,
    preflight: &DbLifecycleSurfacePreflight,
    normal_dot_codegraph_existed_before: bool,
    path_preflight: &AgentUseWatchPathPreflight,
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
    let mut value = json!({
        "status": "rejected",
        "command": "watch",
        "subcommand": "once",
        "command_namespace": "agent-use",
        "agent_use_command": "agent-use watch",
        "profile_name": profile.profile_name.clone(),
        "repo": path_string(&profile.repo_root),
        "repo_root": path_string(&profile.repo_root),
        "db": path_string(&profile.db_path),
        "db_path": path_string(&profile.db_path),
        "resolved_db": path_string(&profile.db_path),
        "db_source": "agent-use profile",
        "external_db_used": true,
        "claimable": false,
        "diagnostic_only": true,
        "reason": "one_or_more_changed_paths_are_outside_repo",
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
        "changed_paths": path_preflight.accepted_paths.clone(),
        "rejected_paths": path_preflight.rejected_paths.clone(),
        "no_op_paths": [],
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
        },
        "candidate_spool_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "candidate_query_index_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "vector_chunks_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "routing_handles_invalidated": {
            "action": "unchanged",
            "scope": "none",
        },
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
        "warnings": preflight.warnings.clone(),
        "normal_dot_codegraph_path": path_string(&normal_dot_codegraph),
        "normal_dot_codegraph_created": !normal_dot_codegraph_existed_before && normal_dot_codegraph.exists(),
        "normal_dot_codegraph_mutated": normal_dot_codegraph_existed_before != normal_dot_codegraph.exists(),
        "public_claim": false,
        "publish_safety": {
            "strategy": "no_update_when_changed_path_is_outside_repo",
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

pub(crate) fn agent_use_delta_fact_counts(
    db_path: &Path,
    changed_paths: &[String],
) -> Result<AgentUseDeltaFactCounts, String> {
    let connection = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| {
            format!(
                "agent-use delta fact count failed to open {}: {error}",
                db_path.display()
            )
        })?;
    let mut counts = AgentUseDeltaFactCounts::default();
    for path in changed_paths {
        counts.entities += query_count_for_path(
            &connection,
            "SELECT COUNT(*) FROM entities e JOIN path_dict p ON p.id = e.path_id WHERE p.value = ?1",
            path,
        )?;
        counts.edges += query_count_for_path(
            &connection,
            "SELECT COUNT(*) FROM edges e JOIN path_dict p ON p.id = e.span_path_id WHERE p.value = ?1",
            path,
        )?;
        counts.source_spans += query_count_for_path(
            &connection,
            "SELECT COUNT(*) FROM entities e JOIN path_dict p ON p.id = COALESCE(e.span_path_id, e.path_id) WHERE p.value = ?1 AND e.start_line IS NOT NULL AND e.end_line IS NOT NULL",
            path,
        )?;
        counts.source_spans += query_count_for_path(
            &connection,
            "SELECT COUNT(*) FROM edges e JOIN path_dict p ON p.id = e.span_path_id WHERE p.value = ?1",
            path,
        )?;
    }
    Ok(counts)
}

pub(crate) fn query_count_for_path(
    connection: &Connection,
    sql: &str,
    path: &str,
) -> Result<u64, String> {
    connection
        .query_row(sql, params![path], |row| row.get::<_, u64>(0))
        .map_err(|error| format!("agent-use delta fact count query failed for {path}: {error}"))
}

pub(crate) fn agent_use_watch_deleted_paths(summary: &IncrementalIndexSummary) -> Vec<String> {
    summary
        .path_cleanup_reasons
        .iter()
        .filter(|(_, reasons)| reasons.iter().any(|reason| reason == "deleted"))
        .map(|(path, _)| path.clone())
        .collect()
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

pub(crate) fn agent_use_watch_dependency_closure_degraded(
    summary: &IncrementalIndexSummary,
) -> bool {
    summary.dependency_closure.closure_budget_hit || summary.dependency_closure.status == "degraded"
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
        "missing" | "no_spool" | "query_index_missing" => "absent",
        "rebuilt" => "rebuilt",
        "corrupt" | "query_index_corrupt" => "error",
        "permission_denied" | "filesystem_inaccessible" | "sidecar_unavailable" => "error",
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
        "candidate_query_index_invalidated_or_rebuilt",
        "vector_chunks_invalidated_or_rebuilt",
        "routing_handles_invalidated",
        "closure_files_considered",
        "closure_files_updated",
        "closure_edges_inspected",
        "closure_relation_classes",
        "closure_budget_hit",
        "degraded_relation_classes",
        "timings",
        "old_graph_valid",
        "new_graph_valid",
        "old_db_preserved",
        "temp_db_claimable",
        "claimability",
        "warnings",
        "recovery_commands",
    ] {
        if let Some(field) = value.get(key) {
            object.insert(key.to_string(), field.clone());
        }
    }
    Value::Object(object)
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
            | "permission_denied"
            | "filesystem_inaccessible"
            | "sidecar_unavailable"
            | "blocked_by_graph_db"
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
        },
        "candidate_spool_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "candidate_query_index_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "vector_chunks_invalidated_or_rebuilt": {
            "action": "unchanged",
            "status": "not_checked",
            "graph_proof": false,
        },
        "routing_handles_invalidated": {
            "action": "unchanged",
            "scope": "none",
        },
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
