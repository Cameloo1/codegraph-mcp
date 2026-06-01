//! Command-line surface for `codegraph-mcp`.
//!
//! Phase 30 hardens proof-path UI ergonomics, MCP schemas/resources, real-repo
//! maturity corpus manifests, and honest parity reports. No subagent workflow
//! is introduced.

#![forbid(unsafe_code)]
#![recursion_limit = "256"]

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    sync::{mpsc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use codegraph_bench::{
    competitors::codegraphcontext::{
        default_report_dir, run_codegraphcontext_comparison, CodeGraphContextComparisonOptions,
    },
    BaselineMode, BenchmarkReport, ContextPacketGateOptions, FinalAcceptanceGateOptions,
    GapScoreboardOptions, GraphTruthGateOptions, RetrievalAblationMode, RetrievalAblationOptions,
    TwoLayerBenchOptions,
};
use codegraph_core::{
    classify_edge_evidence_role, classify_entity_source_role, combine_evidence_roles,
    stable_edge_id, ContextPacket, ContextSnippet, Edge, Entity, EntityKind, EvidenceRole,
    EvidenceRoleDecision, Exactness, FileRecord, Metadata, PathEvidence, RelationKind,
    RepoIndexState, RetrievalCandidate, RetrievalCandidateSource, RetrievalProofStatus,
    RetrievalVerificationStatus, SourceSpan, VectorEmbeddingSource,
};
pub use codegraph_index::{
    add_index_profile_span_ms_to_summary, build_vector_chunk_index_artifacts_for_repo,
    build_vector_chunk_index_json_for_repo, candidate_spool_index_status_for_repo,
    candidate_spool_query_index_path, collect_repo_files, default_db_path, graph_fact_hash,
    index_repo, index_repo_to_db_with_options, index_repo_with_options,
    inspect_db_lifecycle_preflight, inspect_db_lifecycle_surface_preflight,
    inspect_repo_db_passport, load_vector_chunk_index_json, normalize_changed_path,
    parse_extract_pending_files, query_candidate_spool_index_for_repo,
    rebuild_candidate_spool_query_index_for_repo, refresh_index_profile_derived_fields,
    require_reusable_db_passport, scope_policy_hash, should_ignore_path,
    should_start_new_index_batch, update_changed_files, update_changed_files_to_db,
    update_changed_files_with_cache, update_changed_files_with_cache_to_db,
    validate_vector_chunk_source_bindings, vector_chunk_search_hit_to_retrieval_candidate,
    CandidateSpoolIndexLoad, CandidateSpoolIndexQueryResult, CandidateSpoolPolicy,
    DbLifecycleOperationKind, DbLifecyclePolicy, DbLifecyclePreflight, DbLifecycleSurfacePreflight,
    DbLifecycleSurfacePreflightRequest, IncrementalIndexCache, IncrementalIndexSummary,
    IndexBuildMode, IndexError, IndexIssue, IndexOptions, IndexProfile, IndexScopeOptions,
    IndexSummary, LocalFactBundle, PendingIndexFile, StorageMode, VectorChunkArtifactFormat,
    VectorChunkIndexArtifactOptions, VectorChunkIndexBuildOptions, DEFAULT_INDEX_BATCH_MAX_FILES,
    DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES, DEFAULT_STORAGE_POLICY,
    INCLUDE_SEMANTICS_DEFAULT_SCOPE_PLUS_OVERRIDES, SCOPE_POLICY_KIND_DEFAULT_WITH_OVERRIDES,
    SCOPE_TRUTH_STATUS_OVERRIDE_ONLY, UNBOUNDED_STORE_READ_LIMIT,
};
use codegraph_parser::{
    content_hash, detect_language, extract_entities_and_relations, language_frontends,
    LanguageParser, TreeSitterParser,
};
use codegraph_query::{
    extract_prompt_seed_provenance, extract_prompt_seeds, plan_task_retrieval, ContextPackRequest,
    ExactGraphQueryEngine, GraphPath, QueryLimits, RetrievalDocument, RetrievalFunnel,
    RetrievalFunnelConfig, RetrievalFunnelRequest, SymbolSearchHit, SymbolSearchIndex,
    TraversalDirection, TraversalPolicy, TraversalStep, VectorCandidateBranchStatus,
};
use codegraph_store::{
    classify_sqlite_access_problem, DbPassport, DbPreflightReport, GraphStore, SqliteGraphStore,
    TextSearchKind, DB_PASSPORT_VERSION, SCHEMA_VERSION,
};
use codegraph_trace::{
    append_trace_event, replay_trace_file, TraceAppendEvent, TraceConfig, TraceEventType,
    DEFAULT_TRACE_ROOT,
};
use codegraph_vector::{DeterministicTestEmbeddingProvider, TestEmbeddingEnablement};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

mod agent_use;
mod audit;
mod benchmark;
mod bundle;
mod cli_args;
mod context_pack;
mod db_lifecycle;
mod envelope;
mod graph_json;
mod query;
mod routing;
mod serve_ui;
mod storage_budget;

pub(crate) use agent_use::*;
pub(crate) use benchmark::*;
pub(crate) use bundle::*;
pub(crate) use cli_args::*;
pub(crate) use context_pack::*;
pub(crate) use db_lifecycle::*;
pub(crate) use envelope::*;
pub(crate) use graph_json::*;
pub(crate) use query::*;
pub(crate) use routing::*;
pub(crate) use serve_ui::*;

pub const BIN_NAME: &str = "codegraph-mcp";
pub const PHASE: &str = "30";
pub const PRODUCTION_AGENT_USE_PROFILE_NAME: &str = "production-agent-use";
const PRODUCTION_AGENT_USE_DELTA_STATE_FILE_NAME: &str = "production-agent-use.delta-state.json";
pub const BUNDLE_SCHEMA_VERSION: u32 = 2;
const GLOBAL_REPO_SOURCE_ENV: &str = "CODEGRAPH_REPO_SOURCE";
const GLOBAL_DB_SOURCE_ENV: &str = "CODEGRAPH_DB_SOURCE";
const AGENT_USE_DATA_ROOT_ENV: &str = "CODEGRAPH_AGENT_USE_DATA_ROOT";
const AGENT_USE_BOUNDED_READ_PATH_ENV: &str = "CODEGRAPH_AGENT_USE_BOUNDED_READ_PATH";
const WRITE_PATH_CHAOS_FAILPOINT_ENV: &str = "CODEGRAPH_WRITE_PATH_FAILPOINT";
const AGENT_USE_PROFILE_PARENT_PERMISSION_DENIED_FAILPOINT: &str =
    "agent_use_profile_parent_permission_denied";
const AGENT_USE_PROFILE_PARENT_FILESYSTEM_INACCESSIBLE_FAILPOINT: &str =
    "agent_use_profile_parent_filesystem_inaccessible";
const AGENT_USE_WATCH_AFTER_DELTA_COMMIT_BEFORE_STATE_CLEAR_FAILPOINT: &str =
    "agent_use_watch_after_delta_commit_before_state_clear";
const AGENT_USE_WATCH_DEFAULT_DEBOUNCE_MS: u64 = 250;
const AGENT_USE_WATCH_DEFAULT_LOCK_RETRIES: usize = 3;
const AGENT_USE_WATCH_DEFAULT_LOCK_RETRY_MS: u64 = 100;
const AGENT_USE_WATCH_DEFAULT_MAX_BATCH_PATHS: usize = 256;
const ARTIFACT_SAFE_FILENAME_MAX_CHARS: usize = 120;
const DEFAULT_UI_NODE_CAP: usize = 80;
const MAX_UI_NODE_CAP: usize = 250;
const SYMBOL_SEARCH_MIN_FTS_CANDIDATES: usize = 128;
const SYMBOL_SEARCH_MAX_FTS_CANDIDATES: usize = 2_048;
const SYMBOL_SEARCH_FTS_CANDIDATE_FACTOR: usize = 64;
const SYMBOL_SEARCH_FILE_CANDIDATE_LIMIT: usize = 256;
const DEFAULT_UNRESOLVED_CALL_LIMIT: usize = 100;
const MAX_UNRESOLVED_CALL_LIMIT: usize = 500;
const DEFAULT_QUERY_SURFACE_ITERATIONS: usize = 20;
const DEFAULT_QUERY_JSON_LIMIT: usize = 10;
const DEFAULT_QUERY_AGENT_JSON_LIMIT: usize = 5;
const DEFAULT_QUERY_VERBOSE_LIMIT: usize = 20;
const MAX_QUERY_RESULT_LIMIT: usize = 500;
const QUERY_FILE_SNIPPET_MAX_BYTES: usize = 512;
const QUERY_FILE_PREVIEW_MAX_BYTES: usize = 64 * 1024;
const RETRIEVAL_CANDIDATE_SNIPPET_MAX_BYTES: usize = 240;
const AGENT_USE_DISK_FALLBACK_MAX_FILES: usize = 0;
const AGENT_USE_STATUS_MAX_SOURCE_FILE_LOADS: usize = 0;
const AGENT_USE_QUERY_MAX_SOURCE_FILE_LOADS: usize = 0;
const AGENT_USE_CONTEXT_MAX_SOURCE_FILES: usize = 64;
const AGENT_USE_CONTEXT_MAX_SOURCE_BYTES: usize = 256 * 1024;
const CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT: usize = CONTEXT_PLANNING_PACKET_EVIDENCE_LIMIT;
const AGENT_JSON_SCHEMA_VERSION: u32 = 1;
const INDEX_CONCISE_JSON_SIZE_TARGET_BYTES: usize = 8 * 1024;
const INDEX_AGENT_JSON_SIZE_TARGET_BYTES: usize = 4 * 1024;
const CANDIDATE_SPOOL_MIN_USEFUL_BUDGET_BYTES: usize = 64 * 1024;

static PROCESS_CONTEXT_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    static PROCESS_CONTEXT_LOCK_HELD: Cell<bool> = const { Cell::new(false) };
}

struct ProcessContextHeldGuard;

impl Drop for ProcessContextHeldGuard {
    fn drop(&mut self) {
        PROCESS_CONTEXT_LOCK_HELD.with(|held| held.set(false));
    }
}

fn with_process_context_lock<F, R>(operation: F) -> R
where
    F: FnOnce() -> R,
{
    if PROCESS_CONTEXT_LOCK_HELD.with(|held| held.get()) {
        return operation();
    }
    let _guard = PROCESS_CONTEXT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    PROCESS_CONTEXT_LOCK_HELD.with(|held| held.set(true));
    let _held_guard = ProcessContextHeldGuard;
    operation()
}

struct ProcessContextSnapshot {
    cwd: PathBuf,
    db_path: Option<OsString>,
    db_source: Option<OsString>,
    repo_source: Option<OsString>,
    bounded_read_path: Option<OsString>,
    agent_use_data_root: Option<OsString>,
    restored: bool,
}

impl ProcessContextSnapshot {
    fn capture() -> Result<Self, String> {
        Ok(Self {
            cwd: std::env::current_dir().map_err(|error| error.to_string())?,
            db_path: std::env::var_os("CODEGRAPH_DB_PATH"),
            db_source: std::env::var_os(GLOBAL_DB_SOURCE_ENV),
            repo_source: std::env::var_os(GLOBAL_REPO_SOURCE_ENV),
            bounded_read_path: std::env::var_os(AGENT_USE_BOUNDED_READ_PATH_ENV),
            agent_use_data_root: std::env::var_os(AGENT_USE_DATA_ROOT_ENV),
            restored: false,
        })
    }

    fn restore(&mut self) -> Result<(), String> {
        if self.restored {
            return Ok(());
        }
        restore_env_var("CODEGRAPH_DB_PATH", self.db_path.take());
        restore_env_var(GLOBAL_DB_SOURCE_ENV, self.db_source.take());
        restore_env_var(GLOBAL_REPO_SOURCE_ENV, self.repo_source.take());
        restore_env_var(
            AGENT_USE_BOUNDED_READ_PATH_ENV,
            self.bounded_read_path.take(),
        );
        restore_env_var(AGENT_USE_DATA_ROOT_ENV, self.agent_use_data_root.take());
        std::env::set_current_dir(&self.cwd).map_err(|error| error.to_string())?;
        self.restored = true;
        Ok(())
    }
}

impl Drop for ProcessContextSnapshot {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn restore_env_var(name: &str, value: Option<OsString>) {
    if let Some(value) = value {
        std::env::set_var(name, value);
    } else {
        std::env::remove_var(name);
    }
}
const CANDIDATE_SPOOL_SAFETY_RESERVE_BYTES: usize = 1024 * 1024;
const CANDIDATE_SPOOL_QUERY_INDEX_STORAGE_FACTOR: usize = 4;
const CANDIDATE_SPOOL_RUNTIME_RESERVE_BYTES: usize = 6 * 1024 * 1024;
const CANDIDATE_SPOOL_AUDIT_RESERVE_BYTES: usize = 2 * 1024 * 1024;
#[cfg(test)]
const QUERY_AGENT_JSON_SIZE_TARGET_BYTES: usize = 12 * 1024;
const DEFAULT_CONTEXT_AGENT_PATH_LIMIT: usize = 5;
const DEFAULT_CONTEXT_AGENT_SNIPPET_LIMIT: usize = 5;
const DEFAULT_CONTEXT_AGENT_MAX_OUTPUT_BYTES: usize = 16 * 1024;
const DEFAULT_AGENT_USE_AGENT_JSON_MAX_OUTPUT_BYTES: usize = 12 * 1024;
const DEFAULT_AGENT_USE_EXPLAIN_MAX_OUTPUT_BYTES: usize = 48 * 1024;
const MAX_CONTEXT_AGENT_PATH_LIMIT: usize = 64;
const MAX_CONTEXT_AGENT_SNIPPET_LIMIT: usize = 64;
const MIN_CONTEXT_AGENT_MAX_OUTPUT_BYTES: usize = 1024;
const MAX_CONTEXT_AGENT_MAX_OUTPUT_BYTES: usize = 512 * 1024;
const CONTEXT_PLANNING_PACKET_FILE_LIMIT: usize = 8;
const CONTEXT_PLANNING_PACKET_SYMBOL_LIMIT: usize = 12;
const CONTEXT_PLANNING_PACKET_EVIDENCE_LIMIT: usize = 3;
const CONTEXT_PLANNING_PACKET_QUERY_LIMIT: usize = 6;
const CONTEXT_PLANNING_PACKET_VERIFICATION_LIMIT: usize = 3;
const CONTEXT_PACK_VECTOR_INDEX_FILE_NAME: &str = "codegraph-vector-chunks.json";
const CONTEXT_PACK_VECTOR_AUDIT_FILE_NAME: &str = "codegraph-vector-audit.json";
const CONTEXT_PACK_VECTOR_SOURCE_SCOPE: &str = "context-pack-release-vector-candidates";
const CONTEXT_PACK_VECTOR_PROVIDER_DIMENSION: usize = 64;
const CONTEXT_PACK_VECTOR_CANDIDATE_TOP_K: usize = 16;
const CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT: usize = 8;
const CONTEXT_PACK_NUANCE_RESCUE_TOKEN_LIMIT: usize = 32;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentUseProfile {
    pub profile_name: String,
    pub repo_root: PathBuf,
    pub repo_identity_label: String,
    pub repo_identity_hash: String,
    pub profile_root: PathBuf,
    pub db_path: PathBuf,
    pub candidate_spool_path: PathBuf,
    pub candidate_spool_query_index_path: PathBuf,
    pub vector_runtime_path: PathBuf,
    pub vector_audit_path: PathBuf,
    pub lock_or_publish_state_path: PathBuf,
    pub delta_state_path: PathBuf,
    pub lifecycle_expectations: Vec<String>,
    pub recovery_commands: Vec<String>,
    pub mcp_args: Vec<String>,
    pub binary_profile: String,
    pub scope_policy: IndexScopeOptions,
}

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "init",
        usage: "codegraph-mcp init [repo] [--dry-run] [--with-codex-config] [--with-agents] [--with-skills] [--with-hooks] [--with-templates] [--index]",
        description: "Initialize CodeGraph state for a repository.",
    },
    CommandSpec {
        name: "index",
        usage: "codegraph-mcp index <repo> [--db <path>] [--fresh|--rebuild] [--incremental] [--fail-on-db-problem] [--allow-stale-reuse] [--build-vector-index <runtime_path>|--vector-runtime-sidecar <runtime_path>] [--vector-audit-artifact <audit_path>] [--no-vector-audit] [--vector-artifact-format compact_json|pretty_json] [--candidate-spool <path>] [--candidate-spool-policy off|bounded|audit] [--candidate-spool-max-mib <n>] [--candidate-spool-max-bytes <n>] [--candidate-spool-max-records <n>] [--candidate-spool-required] [--candidate-spool-query-index yes|no] [--candidate-spool-per-file-max-records <n>] [--candidate-spool-per-dir-soft-cap <n>] [--candidate-spool-max-snippet-bytes <n>] [--candidate-spool-max-snippets-per-file <n>] [--candidate-spool-max-symbols-per-file <n>] [--early-candidates] [--profile] [--json|--agent-json|--audit-json] [--verbose] [--workers <n>] [--storage-mode <proof|audit|debug>] [--build-mode <proof-build-only|proof-build-plus-validation>] [--max-db-mib <n>] [--max-artifacts-mib <n>] [--min-free-disk-gib <n>] [--extended] [--stress-corpus <name>] [--include-ignored] [--include <pattern>] [--exclude <pattern>] [--no-default-excludes] [--respect-gitignore <true|false>] [--explain-scope] [--print-included] [--print-excluded]",
        description: "Index a repository into the local graph store; scope is default repo scope plus explicit include overrides.",
    },
    CommandSpec {
        name: "agent-use",
        usage: "codegraph-mcp agent-use <status|index|query|context-pack|mcp-config|watch> --repo <repo> --json\n  codegraph-mcp agent-use status --repo <repo> --json\n  codegraph-mcp agent-use index --repo <repo> [--fresh|--rebuild|--incremental] [--json]\n  codegraph-mcp agent-use query symbols|text|files|references|definitions|callers|callees|path|chain|unresolved-calls <args> --repo <repo> [--limit <n>] --agent-json\n  codegraph-mcp agent-use context-pack --repo <repo> --task <task> --agent-json\n  codegraph-mcp agent-use mcp-config --repo <repo> --json\n  codegraph-mcp agent-use watch --repo <repo> --json [--debounce-ms <ms>]\n  codegraph-mcp agent-use watch --repo <repo> --once --changed <path> [--changed <path>] --json",
        description: "Use the production agent profile outside the source tree.",
    },
    CommandSpec {
        name: "status",
        usage: "codegraph-mcp status [repo] [--json] [--candidate-spool <path>] [--vector-runtime-sidecar <path>] [--vector-audit-artifact <path>]",
        description: "Report local CodeGraph index status.",
    },
    CommandSpec {
        name: "query",
        usage: "codegraph-mcp query <symbols|text|files|references|definitions|callers|callees|chain|unresolved-calls|path> [ARGS]\n  codegraph-mcp query symbols|text|files <query> [--limit <n>] [--candidate-spool <path> --early-candidates] [--concise|--agent-json] [--verbose|--debug|--explain]\n  codegraph-mcp query callers|callees [--entity-id <id>|--exact-resolved|--fuzzy] [--limit <n>] [--concise|--agent-json] [--verbose|--debug|--explain] <symbol>\n  codegraph-mcp query unresolved-calls [--limit <n>] [--offset <n>] [--json] [--no-snippets]",
        description: "Query symbols, text, files, references, definitions, calls, chains, or relation paths.",
    },
    CommandSpec {
        name: "impact",
        usage: "codegraph-mcp impact <file-or-symbol>",
        description: "Report impact analysis for a file or symbol.",
    },
    CommandSpec {
        name: "context-pack",
        usage: "codegraph-mcp context-pack --task <task> [--budget <tokens>] [--mode <production|test-impact|debug|impact>] [--seed <symbol>] [--candidate-spool <path> --early-candidates] [--enable-vector-candidates] [--enable-nuance-rescue-candidates] [--vector-index <path>|--vector-runtime-sidecar <path>] [--agent-json|--concise|--explain|--audit-json] [--limit-paths <n>] [--limit-snippets <n>] [--max-output-bytes <n>] [--profile]",
        description: "Build a compact proof-oriented context packet.",
    },
    CommandSpec {
        name: "context",
        usage: "codegraph-mcp context --task <task> [--budget <tokens>] [--mode <mode>] [--seed <symbol>] [--profile]",
        description: "Alias group for context packet generation.",
    },
    CommandSpec {
        name: "bundle",
        usage: "codegraph-mcp bundle <export|import> [ARGS]",
        description: "Export or import a CodeGraph bundle.",
    },
    CommandSpec {
        name: "watch",
        usage: "codegraph-mcp watch [repo] [--db <path>] [--debounce-ms <ms>] [--once --changed <path>...]",
        description: "Watch a repository and incrementally re-index changed files.",
    },
    CommandSpec {
        name: "serve-mcp",
        usage: "codegraph-mcp serve-mcp",
        description: "Serve CodeGraph as a local MCP server.",
    },
    CommandSpec {
        name: "mcp",
        usage: "codegraph-mcp mcp",
        description: "Alias group for serving the local MCP server.",
    },
    CommandSpec {
        name: "serve-ui",
        usage: "codegraph-mcp serve-ui",
        description: "Serve the local proof-path UI.",
    },
    CommandSpec {
        name: "ui",
        usage: "codegraph-mcp ui [repo] [--host 127.0.0.1] [--port 7878]",
        description: "Alias group for serving the local proof-path UI.",
    },
    CommandSpec {
        name: "bench",
        usage: "codegraph-mcp bench [--baseline <mode>]... [--format <json|markdown>] [--output <path>]\n  codegraph-mcp bench graph-truth --cases <path> --fixture-root <path> --out-json <path> --out-md <path> [--fail-on-forbidden] [--fail-on-missing-source-span] [--fail-on-unresolved-exact] [--fail-on-derived-without-provenance] [--fail-on-test-mock-production-leak] [--update-mode] [--keep-workdirs] [--verbose]\n  codegraph-mcp bench context-packet --cases <path> --fixture-root <path> --out-json <path> --out-md <path> [--top-k <k>] [--budget <tokens>]\n  codegraph-mcp bench retrieval-ablation --cases <path> --fixture-root <path> --out-json <path> --out-md <path> [--mode <mode>]... [--top-k <k>]\n  codegraph-mcp bench update-integrity [--mode <update-fast|update-validated|update-debug>] [--loop-kind <combined|repeat-fast|update-fast>] [--iterations <n>] [--autoresearch-iterations <n>] [--timeout-ms <n>] [--workers <n>] [--out-json <path>] [--out-md <path>] [--workdir <path>] [--skip-autoresearch] [--only-autoresearch] [--autoresearch-repo <path>] [--autoresearch-seed-db <path>] [--max-artifacts-mib <n>] [--min-free-disk-gib <n>] [--extended] [--stress-corpus buildroot|linux]\n  codegraph-mcp bench query-surface [--repo <path>] [--db <path>] [--fresh] [--iterations <n>] [--out-json <path>] [--out-md <path>] [--max-db-mib <n>] [--max-artifacts-mib <n>] [--min-free-disk-gib <n>] [--extended] [--stress-corpus buildroot|linux]\n  codegraph-mcp bench proof-build-only <repo>|--repo <path> [--db <path>] [--workers <n>] [--allow-debug-timing] [--max-db-mib <n>] [--min-free-disk-gib <n>] [--extended] [--stress-corpus buildroot|linux]\n  codegraph-mcp bench proof-build-validated <repo>|--repo <path> [--db <path>] [--workers <n>] [--max-db-mib <n>] [--min-free-disk-gib <n>] [--extended] [--stress-corpus buildroot|linux]\n  codegraph-mcp bench comprehensive [--fresh|--use-existing-artifact <db>] [--artifact-metadata <path>] [--fail-on-stale-artifact] [--repo <path>] [--workers <n>] [--output-dir <dir>] [--baseline <path>] [--compact-gate-json <path>] [--previous <path>] [--timestamp <id>] [--allow-debug-timing] [--max-db-mib <n>] [--max-artifacts-mib <n>] [--min-free-disk-gib <n>] [--extended] [--stress-corpus buildroot|linux]\n  codegraph-mcp bench retrieval-quality [--run-id <id>] [--timeout-ms <ms>] [--top-k <k>] [--competitor-bin <path>] [--autoresearch-repo <path>]\n  codegraph-mcp bench agent-quality [--run-id <id>] [--timeout-ms <ms>] [--competitor-bin <path>] [--fake-agent]\n  codegraph-mcp bench final-gate [--output-dir <dir>] [--workspace-root <dir>] [--timeout-ms <ms>] [--competitor-bin <path>] [--cgc-db-size-bytes <n>]\n  codegraph-mcp bench gaps [--output-dir <dir>] [--timeout-ms <ms>] [--top-k <k>] [--competitor-bin <path>]\n  codegraph-mcp bench synthetic-index --output-dir <dir> [--files <n>] [--max-db-mib <n>] [--max-artifacts-mib <n>] [--min-free-disk-gib <n>] [--extended] [--stress-corpus buildroot|linux]\n  codegraph-mcp bench real-repo-corpus\n  codegraph-mcp bench parity-report [--output-dir <dir>]\n  codegraph-mcp bench cgc-comparison [--output-dir <dir>] [--timeout-ms <ms>] [--top-k <k>] [--max-artifacts-mib <n>] [--min-free-disk-gib <n>] [--extended] [--stress-corpus buildroot|linux]",
        description: "Run local reproducible CodeGraph benchmarks, including optional external CGC comparison.",
    },
    CommandSpec {
        name: "trace",
        usage: "codegraph-mcp trace append --event-type <type> --trace-id <id> --tool <tool> --status <status> [--repo <path>] [--run-id <id>] [--task-id <id>] [--input-json <json>] [--output-json <json>]\n  codegraph-mcp trace replay --events <path>",
        description: "Append replayable Agent/MCP JSONL trace events or replay/validate an events.jsonl file.",
    },
    CommandSpec {
        name: "audit",
        usage: "codegraph-mcp audit index-scope <repo> [--json [path]] [--markdown <path>] [--include-ignored] [--include <pattern>] [--exclude <pattern>] [--no-default-excludes] [--respect-gitignore true|false] [--explain-scope] [--print-included] [--print-excluded]\n  codegraph-mcp audit vector-chunks --artifact <path> [--db <path>] [--repo <path>] [--json [path]] [--markdown <path>] [--sample <n>]\n  codegraph-mcp audit vector-chunks --db <path> [--vectors <path>] [--json [path]] [--sample <n>]\n  codegraph-mcp audit storage --db <path> [--json <path>] [--markdown <path>]\n  codegraph-mcp audit storage-micro --out <dir> [--cases simple,expression,inline-tests,duplicates,excluded-junk,all] [--batch-sizes 1,10,100] [--keep-artifacts] [--json [path]] [--markdown [path]] [--no-context-pack] [--respect-gitignore true|false] [--max-db-mib <n>] [--max-artifacts-mib <n>] [--min-free-disk-gib <n>] [--extended] [--stress-corpus buildroot|linux]\n  codegraph-mcp audit schema-check --db <path> [--json <path>] [--markdown <path>]\n  codegraph-mcp audit storage-experiments --db <path> [--workdir <dir>] [--json <path>] [--markdown <path>] [--keep-copies]\n  codegraph-mcp audit sample-edges --db <path> [--relation <RELATION>] [--limit <n>] [--seed <n>] [--json <path>] [--markdown <path>] [--include-snippets]\n  codegraph-mcp audit sample-paths --db <path> [--limit <n>] [--seed <n>] [--json <path>] [--markdown <path>] [--include-snippets] [--max-edge-load <n>] [--timeout-ms <ms>] [--mode <proof|audit|debug>]\n  codegraph-mcp audit relation-counts --db <path> [--json <path>] [--markdown <path>]\n  codegraph-mcp audit label-samples --edges-json <path> [--edges-md <path>] [--paths-json <path>] [--paths-md <path>] [--json <path>] [--markdown <path>]\n  codegraph-mcp audit summarize-labels [--labels <path>] [--dir <path>] [--json <path>] [--markdown <path>]",
        description: "Run read-only audit inspections for vector chunks, storage, sampled edges, relation counts, and manual sample labels.",
    },
    CommandSpec {
        name: "languages",
        usage: "codegraph-mcp languages [--json]",
        description: "List language frontends, support tiers, exactness, and known limitations.",
    },
    CommandSpec {
        name: "doctor",
        usage: "codegraph-mcp doctor [repo] [--json]",
        description: "Check local CodeGraph installation, index, optional resolver, MCP config, and UI assets.",
    },
    CommandSpec {
        name: "config",
        usage: "codegraph-mcp config [show|completions|release-metadata] [--shell <powershell|bash|zsh|fish>]",
        description: "Print config, shell completions, or release/install metadata.",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Copy)]
struct CommandSpec {
    name: &'static str,
    usage: &'static str,
    description: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct TemplateFile {
    name: &'static str,
    relative_path: &'static str,
    contents: &'static str,
}

const AGENTS_TEMPLATE: &str = include_str!("../../../templates/AGENTS.md");

const SKILL_TEMPLATES: &[TemplateFile] = &[
    TemplateFile {
        name: "large-codebase-investigate",
        relative_path: "large-codebase-investigate/SKILL.md",
        contents: include_str!("../../../templates/skills/large-codebase-investigate/SKILL.md"),
    },
    TemplateFile {
        name: "impact-analysis",
        relative_path: "impact-analysis/SKILL.md",
        contents: include_str!("../../../templates/skills/impact-analysis/SKILL.md"),
    },
    TemplateFile {
        name: "trace-dataflow",
        relative_path: "trace-dataflow/SKILL.md",
        contents: include_str!("../../../templates/skills/trace-dataflow/SKILL.md"),
    },
    TemplateFile {
        name: "security-auth-review",
        relative_path: "security-auth-review/SKILL.md",
        contents: include_str!("../../../templates/skills/security-auth-review/SKILL.md"),
    },
    TemplateFile {
        name: "api-contract-change",
        relative_path: "api-contract-change/SKILL.md",
        contents: include_str!("../../../templates/skills/api-contract-change/SKILL.md"),
    },
    TemplateFile {
        name: "event-flow-debug",
        relative_path: "event-flow-debug/SKILL.md",
        contents: include_str!("../../../templates/skills/event-flow-debug/SKILL.md"),
    },
    TemplateFile {
        name: "schema-migration-impact",
        relative_path: "schema-migration-impact/SKILL.md",
        contents: include_str!("../../../templates/skills/schema-migration-impact/SKILL.md"),
    },
    TemplateFile {
        name: "test-impact-analysis",
        relative_path: "test-impact-analysis/SKILL.md",
        contents: include_str!("../../../templates/skills/test-impact-analysis/SKILL.md"),
    },
    TemplateFile {
        name: "refactor-safety-check",
        relative_path: "refactor-safety-check/SKILL.md",
        contents: include_str!("../../../templates/skills/refactor-safety-check/SKILL.md"),
    },
];

const HOOK_TEMPLATES: &[TemplateFile] = &[
    TemplateFile {
        name: "codegraph-hooks",
        relative_path: "codegraph-hooks.json",
        contents: include_str!("../../../templates/hooks/codegraph-hooks.json"),
    },
    TemplateFile {
        name: "SessionStart",
        relative_path: "SessionStart.md",
        contents: include_str!("../../../templates/hooks/SessionStart.md"),
    },
    TemplateFile {
        name: "UserPromptSubmit",
        relative_path: "UserPromptSubmit.md",
        contents: include_str!("../../../templates/hooks/UserPromptSubmit.md"),
    },
    TemplateFile {
        name: "PreToolUse",
        relative_path: "PreToolUse.md",
        contents: include_str!("../../../templates/hooks/PreToolUse.md"),
    },
    TemplateFile {
        name: "PostToolUse",
        relative_path: "PostToolUse.md",
        contents: include_str!("../../../templates/hooks/PostToolUse.md"),
    },
    TemplateFile {
        name: "Stop",
        relative_path: "Stop.md",
        contents: include_str!("../../../templates/hooks/Stop.md"),
    },
];

const UI_INDEX_HTML: &str = include_str!("../../../codegraph-ui/static/index.html");
const UI_APP_JS: &str = include_str!("../../../codegraph-ui/static/app.js");
const UI_D3_JS: &str = include_str!("../../../codegraph-ui/static/d3.v7.min.js");
const UI_STYLES_CSS: &str = include_str!("../../../codegraph-ui/static/styles.css");

#[derive(Debug, Clone)]
struct InitOptions {
    repo: PathBuf,
    dry_run: bool,
    with_codex_config: bool,
    with_agents: bool,
    with_skills: bool,
    with_hooks: bool,
    run_index: bool,
}

#[derive(Debug, Clone)]
struct WatchOptions {
    repo: PathBuf,
    db: Option<PathBuf>,
    debounce: Duration,
    once: bool,
    changed_paths: Vec<PathBuf>,
}

struct WatchStartup {
    repo_root: PathBuf,
    requested_db_path: PathBuf,
    actual_db_path_opened: PathBuf,
    lifecycle_status: Value,
    auto_index_enabled: bool,
    cache: IncrementalIndexCache,
}

#[derive(Debug, Clone)]
struct UiOptions {
    repo: PathBuf,
    host: String,
    port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BenchReportFormat {
    Json,
    Markdown,
}

impl BenchReportFormat {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "markdown",
        }
    }
}

#[derive(Debug, Clone)]
struct BenchOptions {
    baselines: Vec<BaselineMode>,
    format: BenchReportFormat,
    output: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct SyntheticIndexOptions {
    output_dir: PathBuf,
    files: usize,
    storage_budget: storage_budget::StorageBudgetOptions,
}

#[derive(Debug, Clone)]
struct UpdateIntegrityHarnessOptions {
    iterations: usize,
    autoresearch_iterations: usize,
    workers: usize,
    medium_files: usize,
    mode: UpdateBenchmarkMode,
    loop_kind: UpdateLoopKind,
    timeout_ms: Option<u64>,
    out_json: PathBuf,
    out_md: PathBuf,
    workdir: PathBuf,
    skip_autoresearch: bool,
    only_autoresearch: bool,
    autoresearch_repo: PathBuf,
    autoresearch_seed_db: Option<PathBuf>,
    storage_budget: storage_budget::StorageBudgetOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateBenchmarkMode {
    Fast,
    Validated,
    Debug,
}

impl UpdateBenchmarkMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "update-fast",
            Self::Validated => "update-validated",
            Self::Debug => "update-debug",
        }
    }

    fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "fast" | "update-fast" => Ok(Self::Fast),
            "validated" | "update-validated" => Ok(Self::Validated),
            "debug" | "update-debug" => Ok(Self::Debug),
            _ => Err(format!(
                "invalid --mode value: {raw}; expected update-fast, update-validated, or update-debug"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateLoopKind {
    Combined,
    Repeat,
    Update,
}

impl UpdateLoopKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Combined => "combined",
            Self::Repeat => "repeat-fast",
            Self::Update => "update-fast",
        }
    }

    fn runs_repeat(self) -> bool {
        matches!(self, Self::Combined | Self::Repeat | Self::Update)
    }

    fn runs_updates(self) -> bool {
        matches!(self, Self::Combined | Self::Update)
    }

    fn repeat_iterations(self, requested: usize) -> usize {
        match self {
            Self::Repeat => requested,
            Self::Combined | Self::Update => 1,
        }
    }

    fn update_iterations(self, requested: usize) -> usize {
        match self {
            Self::Repeat => 0,
            Self::Combined | Self::Update => requested,
        }
    }

    fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "combined" | "all" => Ok(Self::Combined),
            "repeat" | "repeat-fast" | "repeat-only" => Ok(Self::Repeat),
            "update" | "update-fast" | "update-only" => Ok(Self::Update),
            _ => Err(format!(
                "invalid --loop-kind value: {raw}; expected combined, repeat-fast, or update-fast"
            )),
        }
    }
}

#[derive(Debug, Clone)]
struct CgcComparisonOptions {
    report_dir: PathBuf,
    timeout_ms: u64,
    top_k: usize,
    competitor_executable: Option<PathBuf>,
    storage_budget: storage_budget::StorageBudgetOptions,
}

#[derive(Debug, Clone)]
struct ComprehensiveBenchmarkOptions {
    output_dir: PathBuf,
    baseline_json: PathBuf,
    compact_gate_json: PathBuf,
    previous_json: Option<PathBuf>,
    timestamp: String,
    artifact_mode: ComprehensiveArtifactMode,
    repo: PathBuf,
    artifact_metadata: Option<PathBuf>,
    fail_on_stale_artifact: bool,
    workers: Option<usize>,
    allow_debug_timing: bool,
    storage_budget: storage_budget::StorageBudgetOptions,
}

#[derive(Debug, Clone)]
enum ComprehensiveArtifactMode {
    Fresh,
    Existing(PathBuf),
}

#[derive(Debug, Clone)]
struct QuerySurfaceBenchmarkOptions {
    repo: PathBuf,
    db_path: Option<PathBuf>,
    fresh: bool,
    iterations: usize,
    out_json: PathBuf,
    out_md: PathBuf,
    workers: Option<usize>,
    storage_budget: storage_budget::StorageBudgetOptions,
}

#[derive(Debug, Clone)]
struct TraceAppendOptions {
    repo: PathBuf,
    repo_id: Option<String>,
    trace_root: PathBuf,
    run_id: Option<String>,
    task_id: Option<String>,
    event: TraceAppendEvent,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GlobalOptions {
    repo: Option<PathBuf>,
    db: Option<PathBuf>,
    json: bool,
    agent_json: bool,
    limit: Option<usize>,
    no_color: bool,
    verbose: bool,
    quiet: bool,
    profile: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BundleManifest {
    schema_version: u32,
    created_by: String,
    created_at_unix_ms: u64,
    repo_root: String,
    #[serde(default)]
    repo_identity: Option<String>,
    #[serde(default)]
    canonical_repo_root: Option<String>,
    #[serde(default)]
    repo_head: Option<String>,
    #[serde(default)]
    scope_hash: Option<String>,
    #[serde(default)]
    scope_policy_json: Option<String>,
    #[serde(default)]
    storage_mode: Option<String>,
    #[serde(default)]
    db_schema_version: Option<u32>,
    #[serde(default)]
    graph_digest: Option<String>,
    file_count: usize,
    entity_count: usize,
    edge_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BundleImportMode {
    Fresh,
    Replace,
    Merge,
}

impl BundleImportMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Replace => "replace",
            Self::Merge => "merge",
        }
    }
}

#[derive(Debug, Clone)]
struct BundleImportOptions {
    input: PathBuf,
    mode: BundleImportMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BundleManifestEvidence {
    repo_identity: String,
    canonical_repo_root: String,
    repo_head: Option<String>,
    scope_hash: String,
    scope_policy_json: String,
    storage_mode: StorageMode,
    db_schema_version: u32,
    graph_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BundleTargetState {
    Missing,
    Empty,
    NonEmpty,
    Uninspectable,
}

impl BundleTargetState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Empty => "empty",
            Self::NonEmpty => "non_empty",
            Self::Uninspectable => "uninspectable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CodeGraphBundle {
    manifest: BundleManifest,
    files: Vec<FileRecord>,
    entities: Vec<Entity>,
    edges: Vec<Edge>,
}

pub fn run<I, S>(args: I) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    with_process_context_lock(|| run_unlocked(args))
}

fn run_unlocked<I, S>(args: I) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let _process_context = match ProcessContextSnapshot::capture() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return command_error(
                "process_context_failed",
                &format!("could not capture process context: {error}"),
            );
        }
    };
    let mut args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    if args.is_empty() {
        args.push(BIN_NAME.to_string());
    }

    let (globals, rest) = match parse_global_options(&args[1..]) {
        Ok(parsed) => parsed,
        Err(error) => return command_error("invalid_global_options", &error),
    };
    if let Some(repo) = &globals.repo {
        if let Err(error) = std::env::set_current_dir(repo) {
            return command_error(
                "invalid_global_options",
                &format!("--repo could not be used as current directory: {error}"),
            );
        }
        std::env::set_var(GLOBAL_REPO_SOURCE_ENV, "global --repo");
    } else {
        std::env::remove_var(GLOBAL_REPO_SOURCE_ENV);
    }
    if let Some(db) = &globals.db {
        let resolved_db = absolutize_path(db).unwrap_or_else(|_| db.to_path_buf());
        std::env::set_var("CODEGRAPH_DB_PATH", &resolved_db);
        std::env::set_var(GLOBAL_DB_SOURCE_ENV, "global --db");
    } else if std::env::var_os("CODEGRAPH_DB_PATH").is_some() {
        std::env::set_var(GLOBAL_DB_SOURCE_ENV, "env CODEGRAPH_DB_PATH");
    } else {
        std::env::remove_var(GLOBAL_DB_SOURCE_ENV);
    }

    let rest = rest.as_slice();
    if rest.is_empty() || matches!(rest[0].as_str(), "--help" | "-h") {
        return success(help_text());
    }

    if matches!(rest[0].as_str(), "--version" | "-V") {
        return success(if globals.json {
            json_line(build_metadata_json())
        } else {
            format!(
                "{BIN_NAME} {} ({})\n",
                env!("CARGO_PKG_VERSION"),
                build_commit()
            )
        });
    }

    let command_name = rest[0].as_str();
    let Some(command) = COMMANDS.iter().find(|spec| spec.name == command_name) else {
        return structured_error("unknown_command", command_name);
    };

    let mut command_args = rest[1..].to_vec();
    if globals.profile
        && matches!(command.name, "index" | "context-pack" | "context")
        && !command_args.iter().any(|arg| arg == "--profile")
    {
        command_args.push("--profile".to_string());
    }
    if globals.verbose
        && command.name == "index"
        && !command_args.iter().any(|arg| arg == "--verbose")
    {
        command_args.push("--verbose".to_string());
    }
    if globals.json
        && matches!(
            command.name,
            "index" | "agent-use" | "query" | "doctor" | "languages" | "config" | "status"
        )
        && !command_args.iter().any(|arg| arg == "--json")
    {
        command_args.push("--json".to_string());
    }
    if globals.agent_json {
        if !matches!(command.name, "query" | "context-pack" | "context") {
            return command_error(
                "invalid_global_options",
                &format!(
                    "--agent-json can only be used before query or context-pack: {BIN_NAME} --agent-json query ..."
                ),
            );
        }
        if !command_args
            .iter()
            .any(|arg| matches!(arg.as_str(), "--agent-json" | "--agent_json"))
        {
            command_args.push("--agent-json".to_string());
        }
    }
    if let Some(limit) = globals.limit {
        match command.name {
            "query" => {
                if !has_flag_with_value(&command_args, "--limit") {
                    command_args.push("--limit".to_string());
                    command_args.push(limit.to_string());
                }
            }
            "context-pack" | "context" => {
                if !has_flag_with_value(&command_args, "--limit-paths") {
                    command_args.push("--limit-paths".to_string());
                    command_args.push(limit.to_string());
                }
                if !has_flag_with_value(&command_args, "--limit-snippets") {
                    command_args.push("--limit-snippets".to_string());
                    command_args.push(limit.to_string());
                }
            }
            _ => {
                return command_error(
                    "invalid_global_options",
                    &format!(
                        "--limit can only be used before query or context-pack: {BIN_NAME} --limit <n> query ..."
                    ),
                );
            }
        }
    }
    if command_args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return success(command_help_text(command));
    }

    match command.name {
        "init" => run_json_command("init_failed", run_init_command(&command_args)),
        "index" => run_json_command("index_failed", run_index_command(&command_args)),
        "agent-use" => run_json_command("agent_use_failed", run_agent_use_command(&command_args)),
        "status" => run_json_command("status_failed", run_status_command(&command_args)),
        "query" => run_json_command("query_failed", run_query_command(&command_args)),
        "impact" => run_json_command("impact_failed", run_impact_command(&command_args)),
        "context-pack" | "context" => run_json_command(
            "context_pack_failed",
            run_context_pack_command(&command_args),
        ),
        "bundle" => run_json_command("bundle_failed", run_bundle_command(&command_args)),
        "watch" => run_watch_command(&command_args),
        "serve-mcp" | "mcp" => run_serve_mcp_command(&command_args),
        "serve-ui" | "ui" => run_serve_ui_command(&command_args),
        "bench" => run_json_command("bench_failed", run_bench_command(&command_args)),
        "trace" => run_json_command("trace_failed", run_trace_command(&command_args)),
        "audit" => run_json_command("audit_failed", audit::run_audit_command(&command_args)),
        "languages" => run_languages_command(&command_args),
        "doctor" => run_json_command("doctor_failed", run_doctor_command(&command_args)),
        "config" => run_json_command("config_failed", run_config_command(&command_args)),
        _ => success(not_implemented_json(command, &command_args)),
    }
}

fn run_json_command(error_kind: &str, result: Result<Value, String>) -> CliOutput {
    match result {
        Ok(value) if error_kind == "bench_failed" => {
            success(json_line(add_benchmark_binary_metadata(value)))
        }
        Ok(value) => success(json_line(value)),
        Err(error) if error_kind == "bench_failed" => command_error_json(
            error_kind,
            add_benchmark_binary_metadata(serde_json::from_str::<Value>(&error).unwrap_or_else(
                |_| {
                    json!({
                        "status": "error",
                        "error": error_kind,
                        "message": error,
                    })
                },
            )),
        ),
        Err(error) => match serde_json::from_str::<Value>(&error) {
            Ok(value) => command_error_json(error_kind, value),
            Err(_) => command_error(error_kind, &error),
        },
    }
}

fn parse_global_options(args: &[String]) -> Result<(GlobalOptions, Vec<String>), String> {
    let mut globals = GlobalOptions::default();
    let mut rest = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--repo" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--repo requires a path".to_string());
                };
                globals.repo = Some(PathBuf::from(value));
            }
            "--db" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--db requires a path".to_string());
                };
                globals.db = Some(PathBuf::from(value));
            }
            "--json" => globals.json = true,
            "--agent-json" | "--agent_json" => globals.agent_json = true,
            "--limit" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--limit requires a value".to_string());
                };
                globals.limit = Some(parse_limit_value(value)?);
            }
            value if value.starts_with("--limit=") => {
                let value = value.trim_start_matches("--limit=");
                globals.limit = Some(parse_limit_value(value)?);
            }
            "--no-color" => globals.no_color = true,
            "--verbose" => globals.verbose = true,
            "--quiet" => globals.quiet = true,
            "--profile" => globals.profile = true,
            value => {
                rest.extend(args[index..].iter().cloned());
                if value.is_empty() {
                    return Err("empty command".to_string());
                }
                break;
            }
        }
        index += 1;
    }
    Ok((globals, rest))
}

fn has_flag_with_value(args: &[String], flag: &str) -> bool {
    args.iter()
        .any(|arg| arg == flag || arg.starts_with(&format!("{flag}=")))
}

fn canonical_global_flag(arg: &str) -> Option<&'static str> {
    match arg {
        "--repo" => Some("--repo"),
        "--db" => Some("--db"),
        "--json" => Some("--json"),
        "--no-color" => Some("--no-color"),
        "--verbose" => Some("--verbose"),
        "--quiet" => Some("--quiet"),
        "--profile" => Some("--profile"),
        _ if arg.starts_with("--repo=") => Some("--repo"),
        _ if arg.starts_with("--db=") => Some("--db"),
        _ => None,
    }
}

fn misplaced_global_flag_message(flag: &str, command_name: &str) -> String {
    let value_hint = match flag {
        "--repo" | "--db" => " <path>",
        _ => "",
    };
    format!(
        "{flag} is a global flag. Put it before the command: {BIN_NAME} {flag}{value_hint} {command_name} ..."
    )
}

fn misplaced_global_flag_error(arg: &str, command_name: &str) -> Option<String> {
    canonical_global_flag(arg).map(|flag| misplaced_global_flag_message(flag, command_name))
}

fn success(stdout: String) -> CliOutput {
    CliOutput {
        exit_code: 0,
        stdout,
        stderr: String::new(),
    }
}

fn structured_error(kind: &str, value: &str) -> CliOutput {
    CliOutput {
        exit_code: 2,
        stdout: String::new(),
        stderr: json_line(json!({
            "status": "error",
            "error": kind,
            "value": value,
            "message": format!("Run '{BIN_NAME} --help' for supported commands."),
        })),
    }
}

fn command_error(kind: &str, message: &str) -> CliOutput {
    CliOutput {
        exit_code: 1,
        stdout: String::new(),
        stderr: json_line(json!({
            "status": "error",
            "error": kind,
            "message": message,
        })),
    }
}

fn command_error_json(kind: &str, mut value: Value) -> CliOutput {
    if let Some(object) = value.as_object_mut() {
        object
            .entry("status".to_string())
            .or_insert_with(|| json!("error"));
        object
            .entry("error".to_string())
            .or_insert_with(|| json!(kind));
    }
    CliOutput {
        exit_code: 1,
        stdout: String::new(),
        stderr: json_line(value),
    }
}

fn help_text() -> String {
    let mut output = String::from(
        "CodeGraph Memory Layer CLI\n\n\
         Usage:\n  codegraph-mcp <COMMAND> [ARGS]\n  codegraph-mcp --help\n\n\
         Global flags:\n  --repo <path>  --db <path>  --json  --no-color  --verbose  --quiet  --profile\n\n\
         Commands:\n",
    );

    for command in COMMANDS {
        output.push_str(&format!("  {:<13} {}\n", command.name, command.description));
    }

    output.push_str(
        "\nPhase 30: proof-focused UI/MCP ergonomics, real-repo corpus manifests, and honest parity reporting are available without weakening proof labels.\n\
         Workflow rule: no subagents; single-agent use only.\n",
    );

    output
}

fn run_init_command(args: &[String]) -> Result<Value, String> {
    let options = parse_init_options(args)?;
    let repo_root = resolve_repo_root(&options.repo)?;
    let languages = detect_tooling(&repo_root)?;
    let codegraph_dir = repo_root.join(".codegraph");
    let codex_dir = repo_root.join(".codex");
    let config_path = codex_dir.join("config.toml");
    let skills_dir = codex_dir.join("skills");
    let hooks_dir = codex_dir.join("hooks");
    let mut actions = vec![json!({
        "action": "create_dir",
        "path": path_string(&codegraph_dir),
    })];

    if options.with_codex_config {
        actions.push(json!({
            "action": "create_file_if_missing",
            "path": path_string(&config_path),
            "template": "mcp_servers.codegraph-mcp",
        }));
    }
    if options.with_agents {
        actions.push(json!({
            "action": "create_file_if_missing",
            "path": path_string(&repo_root.join("AGENTS.md")),
            "template": "single-agent CodeGraph instructions",
        }));
    }
    if options.with_skills {
        actions.push(json!({
            "action": "create_dir",
            "path": path_string(&skills_dir),
            "templates": template_names(SKILL_TEMPLATES),
        }));
    }
    if options.with_hooks {
        actions.push(json!({
            "action": "create_dir",
            "path": path_string(&hooks_dir),
            "templates": template_names(HOOK_TEMPLATES),
        }));
    }
    if options.run_index {
        actions.push(json!({
            "action": "run_index",
            "repo": path_string(&repo_root),
        }));
    }

    let mut index_summary = None;
    if !options.dry_run {
        fs::create_dir_all(&codegraph_dir).map_err(|error| error.to_string())?;
        if options.with_codex_config || options.with_skills || options.with_hooks {
            fs::create_dir_all(&codex_dir).map_err(|error| error.to_string())?;
        }
        if options.with_codex_config {
            write_if_missing(&config_path, &codex_config_template(&repo_root))?;
        }
        if options.with_agents {
            write_if_missing(&repo_root.join("AGENTS.md"), agents_template())?;
        }
        if options.with_skills {
            fs::create_dir_all(&skills_dir).map_err(|error| error.to_string())?;
            write_template_files(&skills_dir, SKILL_TEMPLATES)?;
        }
        if options.with_hooks {
            fs::create_dir_all(&hooks_dir).map_err(|error| error.to_string())?;
            write_template_files(&hooks_dir, HOOK_TEMPLATES)?;
        }
        if options.run_index {
            index_summary = Some(index_repo(&repo_root).map_err(|error| error.to_string())?);
        }
    }

    Ok(json!({
        "status": if options.dry_run { "dry_run" } else { "initialized" },
        "phase": PHASE,
        "repo_root": path_string(&repo_root),
        "detected": languages,
        "actions": actions,
        "index_summary": index_summary,
        "next_commands": [
            "codegraph-mcp index .",
            "codegraph-mcp status",
            "codegraph-mcp query symbols <query>",
            "codegraph-mcp query callers <symbol>",
            "codegraph-mcp context-pack --task <task>"
        ],
        "workflow": "single-agent-only",
    }))
}

fn run_index_command(args: &[String]) -> Result<Value, String> {
    let (repo, db, mut options, output_mode, budget_options, vector_index_options) =
        parse_index_command_options(args)?;
    let started = Instant::now();
    let repo_root = resolve_repo_root(Path::new(&repo))?;
    let db_path = db
        .clone()
        .map(|path| normalize_db_path_for_repo(&repo_root, &path))
        .unwrap_or_else(|| selected_db_path_for_repo(&repo_root).path);
    let vector_index_path = vector_index_options.runtime_path.as_ref().map(|path| {
        if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    });
    let vector_audit_artifact_path =
        vector_index_options
            .audit_artifact_path
            .as_ref()
            .map(|path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    std::env::current_dir()
                        .unwrap_or_else(|_| PathBuf::from("."))
                        .join(path)
                }
            });
    let candidate_spool_path = options.candidate_spool_path.as_ref().map(|path| {
        if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    });
    options.candidate_spool_path = candidate_spool_path.clone();
    let mut candidate_spool_budget_decision =
        apply_candidate_spool_budget_policy(&mut options, &budget_options, &vector_index_options);
    if options.candidate_spool_required
        && candidate_spool_budget_decision.disabled_reason.is_some()
        && matches!(
            candidate_spool_budget_decision.decision.as_str(),
            "candidate_spool_required_budget_exceeded"
                | "candidate_spool_required_without_path"
                | "candidate_spool_policy_off"
        )
    {
        return Err(
            serde_json::to_string(&candidate_spool_required_error_value(
                &candidate_spool_budget_decision,
                false,
                false,
                "candidate spool was marked required but cannot be built under the requested policy/budget",
            ))
            .map_err(|error| error.to_string())?,
        );
    }
    let candidate_spool_path = options.candidate_spool_path.clone();
    let preflight = storage_budget::storage_budget_preflight(
        &budget_options,
        storage_budget::StorageBudgetContext {
            command: "codegraph-mcp index".to_string(),
            repo_root: Some(repo_root.clone()),
            db_path: Some(db_path.clone()),
            out_path: vector_index_path.clone().or(candidate_spool_path.clone()),
            explicit_db: db.is_some(),
            explicit_out: vector_index_path.is_some()
                || vector_audit_artifact_path.is_some()
                || candidate_spool_path.is_some(),
            diagnostic_only: false,
        },
    );
    if preflight.is_refused() {
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value("codegraph-mcp index", &preflight),
        ));
    }
    let mut summary = if let Some(db) = db {
        index_repo_to_db_with_options(Path::new(&repo), &db, options)
    } else {
        index_repo_with_options(Path::new(&repo), options)
    }
    .map_err(|error| error.to_string())?;
    let vector_index_summary = if let Some(vector_index_path) = vector_index_path.as_ref() {
        let provider = context_pack_vector_provider()?;
        let vector_summary = build_vector_chunk_index_artifacts_for_repo(
            &repo_root,
            &db_path,
            vector_index_path,
            &provider,
            context_pack_vector_build_options(),
            VectorChunkIndexArtifactOptions {
                runtime_format: vector_index_options.runtime_format,
                audit_artifact_path: vector_audit_artifact_path.clone(),
            },
        )
        .map_err(|error| error.to_string())?;
        add_index_profile_span_ms_to_summary(
            &mut summary,
            "vector_runtime_sidecar_build",
            vector_summary.build_timings.total_ms,
            1,
            vector_summary.persisted_total_chunks as u64,
            "runtime vector sidecar build total from vector build summary",
        );
        if vector_audit_artifact_path.is_some() {
            add_index_profile_span_ms_to_summary(
                &mut summary,
                "vector_audit_artifact_build",
                vector_summary.build_timings.write_json_ms,
                1,
                vector_summary.audit_total_chunks as u64,
                "audit artifact write is measured inside aggregate vector write_json_ms; runtime/audit write split is not yet separable",
            );
        }
        refresh_index_profile_derived_fields(&mut summary);
        Some(vector_summary)
    } else {
        None
    };
    let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut vector_outputs = vector_index_path
        .iter()
        .chain(vector_audit_artifact_path.iter())
        .cloned()
        .collect::<Vec<PathBuf>>();
    if candidate_spool_budget_decision.required {
        if let Some(path) = candidate_spool_path.as_ref() {
            vector_outputs.push(path.clone());
            vector_outputs.push(candidate_spool_query_index_path(path));
        }
    }
    let budget = storage_budget::storage_budget_postflight(preflight, &[db_path], &vector_outputs);
    let max_artifact_bytes = artifact_budget_bytes(&budget_options);
    let hard_artifact_bytes = budget.artifact_bytes.unwrap_or(0);
    let artifact_budget_remaining_bytes = max_artifact_bytes.saturating_sub(hard_artifact_bytes);
    candidate_spool_budget_decision.artifact_budget_remaining_bytes =
        Some(artifact_budget_remaining_bytes);
    let spool_footprint_bytes = candidate_spool_footprint_bytes(&summary);
    let runtime_sidecar_ready = vector_index_summary
        .as_ref()
        .is_some_and(|summary| summary.status == "ok" || summary.status == "ready");
    if let Some(spool) = summary.candidate_spool.as_mut() {
        spool.candidate_spool_required = candidate_spool_budget_decision.required;
        spool.candidate_spool_policy = candidate_spool_budget_decision.policy.as_str().to_string();
        spool.candidate_spool_budget_bytes =
            candidate_spool_budget_decision.budget_bytes.unwrap_or(0);
        spool.artifact_budget_remaining_bytes = Some(artifact_budget_remaining_bytes);
        if spool_footprint_bytes > artifact_budget_remaining_bytes {
            spool.candidate_spool_truncated = true;
            spool.candidate_spool_partial = true;
            spool.artifact_budget_decision =
                Some("candidate_spool_exceeds_remaining_budget".to_string());
            spool.candidate_spool_warning = Some(
                "Candidate spool footprint exceeded remaining artifact budget; graph DB/runtime outputs remain governed by their own validation.".to_string(),
            );
        } else {
            spool.artifact_budget_decision = Some(candidate_spool_budget_decision.decision.clone());
            spool.candidate_spool_warning = candidate_spool_budget_decision.warning.clone();
        }
        spool.candidate_spool_disabled_reason =
            candidate_spool_budget_decision.disabled_reason.clone();
    }
    let mut value = match output_mode {
        IndexJsonOutputMode::Audit => index_summary_json(&summary),
        IndexJsonOutputMode::Agent => index_summary_agent_json(&summary, wall_ms),
        IndexJsonOutputMode::Concise => index_summary_concise_json(&summary, wall_ms),
    }?;
    if let Some(object) = value.as_object_mut() {
        if let Some(vector_index_summary) = vector_index_summary {
            object.insert(
                "vector_index".to_string(),
                serde_json::to_value(vector_index_summary).map_err(|error| error.to_string())?,
            );
            object.insert("external_provider".to_string(), json!(false));
            object.insert("source_leaves_machine".to_string(), json!(false));
        }
        if output_mode == IndexJsonOutputMode::Agent {
            let budget_json = budget.to_json();
            object.insert(
                "storage_budget".to_string(),
                json!({
                    "budget_status": budget_json.get("budget_status").cloned().unwrap_or(Value::Null),
                    "claimable": budget_json.get("claimable").cloned().unwrap_or(Value::Null),
                    "db_bytes": budget_json.get("db_bytes").cloned().unwrap_or(Value::Null),
                    "artifact_bytes": budget_json.get("artifact_bytes").cloned().unwrap_or(Value::Null),
                    "refusal_reason": budget_json.get("refusal_reason").cloned().unwrap_or(Value::Null),
                }),
            );
        } else {
            object.insert("storage_budget".to_string(), budget.to_json());
        }
        apply_candidate_spool_budget_json_fields(
            object,
            &summary,
            &candidate_spool_budget_decision,
            spool_footprint_bytes,
            index_summary_claimable(&summary),
            runtime_sidecar_ready,
        );
        if output_mode == IndexJsonOutputMode::Agent {
            object.remove("candidate_spool_budget");
            if let Some(summary) = object.get_mut("summary").and_then(Value::as_object_mut) {
                summary.remove("candidate_spool_budget");
            }
        }
    }
    if budget.is_refused() {
        if candidate_spool_budget_decision.required {
            return Err(serde_json::to_string(&candidate_spool_required_error_value(
                &candidate_spool_budget_decision,
                index_summary_claimable(&summary),
                runtime_sidecar_ready,
                budget.refusal_reason.clone().unwrap_or_else(|| {
                    "candidate spool required and storage budget refused".to_string()
                }),
            ))
            .map_err(|error| error.to_string())?);
        }
        return Err(storage_budget::structured_error_string(
            storage_budget::storage_budget_error_value("codegraph-mcp index", &budget),
        ));
    }
    Ok(value)
}

fn run_status_command(args: &[String]) -> Result<Value, String> {
    let status_options = parse_status_args(args)?;
    let repo = status_options.repo;
    let repo_root = resolve_repo_root(&repo)?;
    let db_path = resolved_db_path_for_repo(&repo_root);
    let preflight = inspect_read_db_lifecycle_preflight(&repo_root, &db_path, None)?;
    let sqlite_sidecars = sqlite_sidecars_status_from_health(&db_path, &preflight.db_health);
    let staged_availability = staged_availability_for_cli(
        &repo_root,
        &db_path,
        Some(&preflight),
        status_options.candidate_spool_path.as_deref(),
        status_options.vector_runtime_path.as_deref(),
        status_options.vector_audit_path.as_deref(),
        None,
    );
    let staged_fields = staged_availability_top_level_fields(&staged_availability);
    if preflight.path_access_status == "db_missing" {
        let mut value = json!({
            "status": "not_indexed",
            "db_problem_kind": preflight.db_problem_kind.clone(),
            "repo_root": path_string(&repo_root),
            "db_path": path_string(&db_path),
            "db_path_outside_workspace": preflight.db_path_outside_workspace,
            "outside_workspace_note": preflight.outside_workspace_note.clone(),
            "path_access_status": preflight.path_access_status.clone(),
            "path_access_error": preflight.path_access_error.clone(),
            "sqlite_sidecars": sqlite_sidecars.clone(),
            "sidecar_status": sqlite_sidecars["sidecar_status"].clone(),
            "db_lifecycle_read": db_lifecycle_preflight_json(&preflight, true, false, false),
            "telemetry": runtime_telemetry_unknown_json(),
            "next_command": "codegraph-mcp index .",
        });
        merge_json_object(&mut value, staged_fields);
        merge_json_object(&mut value, plain_status_agent_use_guidance_json(&repo_root));
        return Ok(value);
    }

    if !preflight.safe {
        let mut value = json!({
            "status": "db_problem",
            "db_problem_kind": preflight.db_problem_kind.clone(),
            "repo_root": path_string(&repo_root),
            "db_path": path_string(&db_path),
            "db_path_outside_workspace": preflight.db_path_outside_workspace,
            "outside_workspace_note": preflight.outside_workspace_note.clone(),
            "path_access_status": preflight.path_access_status.clone(),
            "path_access_error": preflight.path_access_error.clone(),
            "db_health": preflight.db_health.clone(),
            "sqlite_sidecars": sqlite_sidecars.clone(),
            "sidecar_status": sqlite_sidecars["sidecar_status"].clone(),
            "db_lifecycle_read": db_lifecycle_preflight_json(&preflight, true, false, false),
            "telemetry": runtime_telemetry_unknown_json(),
            "next_command": "codegraph-mcp index . --fresh",
        });
        merge_json_object(&mut value, staged_fields);
        merge_json_object(&mut value, plain_status_agent_use_guidance_json(&repo_root));
        return Ok(value);
    }

    macro_rules! status_read {
        ($expr:expr, $context:expr) => {
            match $expr {
                Ok(value) => value,
                Err(error) => {
                    let error = error.to_string();
                    if let Some(value) = status_db_read_blocker_json(
                        &repo_root,
                        &db_path,
                        &preflight,
                        &sqlite_sidecars,
                        $context,
                        &error,
                    ) {
                        return Ok(value);
                    }
                    return Err(error);
                }
            }
        };
    }

    let store = status_read!(SqliteGraphStore::open_read_only(&db_path), "open_read_only");
    let files = status_read!(store.list_files(10_000), "list_files");
    let languages = files
        .iter()
        .filter_map(|file| file.language.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let storage_accounting = status_read!(store.storage_accounting(), "storage_accounting")
        .into_iter()
        .map(|row| {
            json!({
                "name": row.name,
                "row_count": row.row_count,
                "payload_bytes": row.payload_bytes,
            })
        })
        .collect::<Vec<_>>();
    let relation_facts = status_read!(store.count_edges(), "count_edges");
    let source_span_facts = status_read!(store.count_source_spans(), "count_source_spans");

    let mut value = json!({
        "status": "ok",
        "phase": PHASE,
        "repo_root": path_string(&repo_root),
        "db_path": path_string(&db_path),
        "db_path_outside_workspace": preflight.db_path_outside_workspace,
        "outside_workspace_note": preflight.outside_workspace_note.clone(),
        "path_access_status": preflight.path_access_status.clone(),
        "path_access_error": preflight.path_access_error.clone(),
        "db_size_bytes": sqlite_family_size_bytes(&db_path).map_err(|error| error.to_string())?,
        "storage_policy": DEFAULT_STORAGE_POLICY,
        "db_health": preflight.db_health.clone(),
        "sqlite_sidecars": sqlite_sidecars.clone(),
        "sidecar_status": sqlite_sidecars["sidecar_status"].clone(),
        "db_lifecycle_read": db_lifecycle_preflight_json(&preflight, true, false, false),
        "telemetry": runtime_telemetry_unknown_json(),
        "schema_version": status_read!(store.schema_version(), "schema_version"),
        "files": status_read!(store.count_files(), "count_files"),
        "entities": status_read!(store.count_entities(), "count_entities"),
        "relation_facts": relation_facts,
        "source_span_facts": source_span_facts,
        "release_reported_relation_facts": relation_facts,
        "release_reported_source_span_facts": source_span_facts,
        "edges": relation_facts,
        "source_spans": source_span_facts,
        "metric_label_notes": {
            "relation_facts": "Aggregate reported relation facts across the active store surface; not a raw table-row label unless storage_accounting says table rows.",
            "source_span_facts": "Aggregate reported source-span facts across the active store surface; not a raw table-row label unless storage_accounting says table rows.",
            "edges": "Deprecated compatibility alias for relation_facts.",
            "source_spans": "Deprecated compatibility alias for source_span_facts."
        },
        "relation_counts": status_read!(store.relation_counts(), "relation_counts"),
        "storage_accounting": storage_accounting,
        "languages": languages,
    });
    merge_json_object(&mut value, staged_fields);
    Ok(value)
}

fn status_db_read_blocker_json(
    repo_root: &Path,
    db_path: &Path,
    preflight: &DbLifecyclePreflight,
    sqlite_sidecars: &Value,
    context: &str,
    error: &str,
) -> Option<Value> {
    let problem = classify_sqlite_access_problem(error)?;
    Some(json!({
        "status": "db_problem",
        "db_problem_kind": problem.db_problem_kind,
        "repo_root": path_string(repo_root),
        "db_path": path_string(db_path),
        "db_path_outside_workspace": preflight.db_path_outside_workspace,
        "outside_workspace_note": preflight.outside_workspace_note.clone(),
        "path_access_status": problem.path_access_status,
        "path_access_error": error,
        "db_health": preflight.db_health.clone(),
        "sqlite_sidecars": sqlite_sidecars.clone(),
        "sidecar_status": sqlite_sidecars["sidecar_status"].clone(),
        "db_lifecycle_read": db_lifecycle_preflight_json(preflight, true, false, false),
        "telemetry": runtime_telemetry_unknown_json(),
        "read_only_mode_used": true,
        "immutable_mode_used": !sqlite_sidecar_path(db_path, "wal").exists()
            && !sqlite_sidecar_path(db_path, "shm").exists()
            && !sqlite_rollback_journal_path(db_path).exists(),
        "blocked_operation": context,
        "blockers": [
            format!(
                "read-only status inspection blocked by {} during {}: {}",
                problem.db_problem_kind, context, error
            )
        ],
        "suggested_next": "Retry after the current writer/publisher releases the SQLite DB lock; do not treat this diagnostic as claimable context.",
        "next_command": "codegraph-mcp status . --json",
    }))
}

fn run_languages_command(args: &[String]) -> CliOutput {
    match args {
        [] => success(render_languages_table()),
        [flag] if flag == "--json" => success(json_line(json!({
            "status": "ok",
            "phase": PHASE,
            "source_of_truth": "MVP.md Prompt 27",
            "frontends": language_frontends(),
        }))),
        _ => command_error(
            "languages_failed",
            "Usage: codegraph-mcp languages [--json]",
        ),
    }
}

fn run_doctor_command(args: &[String]) -> Result<Value, String> {
    let mut repo = PathBuf::from(".");
    let mut json_output = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json_output = true,
            value if value.starts_with('-') => {
                if let Some(error) = misplaced_global_flag_error(value, "doctor") {
                    return Err(error);
                }
                return Err(format!("unknown doctor option: {value}"));
            }
            value => repo = PathBuf::from(value),
        }
        index += 1;
    }
    let _json_output = json_output;
    let repo_root = resolve_repo_root(&repo)?;
    let db_path = resolved_db_path_for_repo(&repo_root);
    let mut checks = Vec::new();
    let mut warnings = 0usize;
    let mut errors = 0usize;

    push_doctor_check(
        &mut checks,
        "codegraph_dir_permissions",
        if repo_root.join(".codegraph").exists() {
            "ok"
        } else {
            "warning"
        },
        ".codegraph directory is writable or can be created",
        repo_root
            .join(".codegraph")
            .parent()
            .is_some_and(|parent| parent.exists()),
    );
    let db_lifecycle = inspect_db_lifecycle_surface_preflight(DbLifecycleSurfacePreflightRequest {
        repo_root: repo_root.clone(),
        db_path: db_path.clone(),
        surface_name: "cli.doctor".to_string(),
        operation_kind: DbLifecycleOperationKind::NormalRead,
        allow_stale_read: false,
        allow_foreign_repo: false,
        required_storage_mode: None,
        expected_scope: None,
    })
    .map_err(|error| error.to_string())?;
    let database_exists = db_lifecycle.path_access_status != "db_missing";
    let lifecycle_evidence = db_lifecycle_surface_preflight_json(&db_lifecycle);
    let sqlite_sidecars =
        sqlite_sidecars_status_from_health(&db_path, &db_lifecycle.lifecycle_preflight.db_health);
    let db_check_status = if database_exists { "error" } else { "warning" };
    let db_check_message = if db_lifecycle.safe_to_read {
        "local SQLite graph database is lifecycle-safe to query"
    } else if database_exists {
        "local SQLite graph database exists but lifecycle/passport checks block normal reads"
    } else {
        "local SQLite graph database is missing; run `codegraph-mcp index .` first"
    };
    push_doctor_check(
        &mut checks,
        "database",
        db_check_status,
        db_check_message,
        db_lifecycle.safe_to_read,
    );
    push_doctor_check(
        &mut checks,
        "language_frontends",
        "ok",
        "language frontend registry is available",
        !language_frontends().is_empty(),
    );
    let node_ok = Command::new("node").arg("--version").output().is_ok();
    push_doctor_check(
        &mut checks,
        "typescript_resolver",
        if node_ok { "ok" } else { "warning" },
        "optional Node/TypeScript semantic resolver",
        node_ok,
    );
    let mcp_config = repo_root.join(".codex").join("config.toml");
    push_doctor_check(
        &mut checks,
        "mcp_config",
        if mcp_config.exists() { "ok" } else { "warning" },
        ".codex/config.toml MCP config exists",
        mcp_config.exists(),
    );
    push_doctor_check(
        &mut checks,
        "ui_assets",
        "ok",
        "bundled local Proof-Path UI assets are compiled into the CLI",
        true,
    );

    for check in &checks {
        match check.get("status").and_then(Value::as_str) {
            Some("error") => errors += 1,
            Some("warning") => warnings += 1,
            _ => {}
        }
    }
    let storage_mode = db_lifecycle
        .lifecycle_preflight
        .db_health
        .passport
        .as_ref()
        .map(|passport| passport.storage_mode.clone());
    let schema_status = doctor_lifecycle_field_status(
        &db_lifecycle,
        &["schema version mismatch", "passport schema mismatch"],
        &db_lifecycle.schema_status,
    );
    let staged_availability = staged_availability_for_cli(
        &repo_root,
        &db_path,
        Some(&db_lifecycle.lifecycle_preflight),
        None,
        None,
        None,
        None,
    );
    let staged_fields = staged_availability_top_level_fields(&staged_availability);

    let mut value = json!({
        "status": if errors == 0 { "ok" } else { "error" },
        "phase": PHASE,
        "repo_root": path_string(&repo_root),
        "db_path": path_string(&db_path),
        "db_path_outside_workspace": db_lifecycle.db_path_outside_workspace,
        "outside_workspace_note": db_lifecycle.outside_workspace_note.clone(),
        "db_problem_kind": db_lifecycle.db_problem_kind.clone(),
        "path_access_status": db_lifecycle.path_access_status.clone(),
        "path_access_error": db_lifecycle.path_access_error.clone(),
        "database_exists": database_exists,
        "safe_to_query": db_lifecycle.safe_to_read,
        "passport_status": db_lifecycle.passport_status.clone(),
        "repo_match": db_lifecycle.repo_match,
        "scope_match": db_lifecycle.scope_match,
        "schema_status": schema_status,
        "storage_mode": storage_mode,
        "storage_mode_status": db_lifecycle.lifecycle_preflight.storage_mode_status.clone(),
        "blockers": db_lifecycle.blockers.clone(),
        "lifecycle_warnings": db_lifecycle.warnings.clone(),
        "sqlite_sidecars": sqlite_sidecars.clone(),
        "sidecar_status": sqlite_sidecars["sidecar_status"].clone(),
        "db_lifecycle_read": lifecycle_evidence,
        "telemetry": runtime_telemetry_unknown_json(),
        "checks": checks,
        "warnings": warnings,
        "warning_count": warnings,
        "errors": errors,
        "proof": "Doctor is local-only and treats missing optional components as warnings.",
    });
    merge_json_object(&mut value, staged_fields);
    Ok(value)
}

fn push_doctor_check(
    checks: &mut Vec<Value>,
    name: &str,
    status_if_missing: &str,
    message: &str,
    ok: bool,
) {
    checks.push(json!({
        "name": name,
        "status": if ok { "ok" } else { status_if_missing },
        "message": message,
        "optional": status_if_missing == "warning",
    }));
}

fn sqlite_sidecars_status_for_path(db_path: &Path) -> Value {
    let wal_path = sqlite_sidecar_path(db_path, "wal");
    let shm_path = sqlite_sidecar_path(db_path, "shm");
    let wal_bytes = metadata_len(&wal_path).unwrap_or(0);
    let shm_bytes = metadata_len(&shm_path).unwrap_or(0);
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
    let sidecar_only_change = main_exists && !sidecars.is_empty();
    let sidecar_change_classification = if sidecar_status == "orphan_without_main_db" {
        "orphan_without_main_db"
    } else if sidecar_only_change {
        "sidecar_only_change"
    } else {
        "none"
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
        "sidecar_only_change": sidecar_only_change,
        "sidecar_change_classification": sidecar_change_classification,
        "main_db_mutation_claim": if sidecar_only_change { "sidecar_presence_not_counted_as_main_db_mutation" } else { "no_sidecar_only_change" },
        "orphan_sidecars": orphan_sidecars,
        "orphan_sidecars_deprecated": true,
    })
}

fn sqlite_sidecars_status_from_health(db_path: &Path, health: &DbPreflightReport) -> Value {
    let mut status = sqlite_sidecars_status_for_path(db_path);
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
        if health.sidecar_status == "orphan_without_main_db" {
            object.insert(
                "sidecar_change_classification".to_string(),
                json!("orphan_without_main_db"),
            );
            object.insert("sidecar_only_change".to_string(), json!(false));
            object.insert(
                "main_db_mutation_claim".to_string(),
                json!("orphan_sidecars_without_main_db"),
            );
        }
    }
    status
}

struct ReadOnlySqliteOpen {
    connection: Connection,
    read_only_mode_used: String,
    immutable_mode_used: bool,
    immutable_mode_reason: String,
}

fn open_sqlite_read_only_side_effect_minimal(db_path: &Path) -> Result<ReadOnlySqliteOpen, String> {
    if !db_path.exists() {
        return Err(format!("database does not exist: {}", db_path.display()));
    }
    let rollback_journal_exists = sqlite_rollback_journal_path(db_path).exists();
    let immutable_mode_used = !sqlite_sidecar_path(db_path, "wal").exists()
        && !sqlite_sidecar_path(db_path, "shm").exists()
        && !rollback_journal_exists;
    let immutable_mode_reason = if immutable_mode_used {
        "immutable=1 used because no WAL/SHM sidecars or rollback journal were present before inspection".to_string()
    } else if rollback_journal_exists {
        "immutable=1 not used because a rollback journal was present; strict mode=ro preserves lock visibility".to_string()
    } else {
        "immutable=1 not used because WAL/SHM sidecars were present; strict mode=ro preserves WAL visibility".to_string()
    };
    let uri = sqlite_read_only_uri(db_path, immutable_mode_used)?;
    let connection = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|error| format!("failed to open {} read-only: {error}", db_path.display()))?;
    connection
        .execute_batch(
            "
            PRAGMA query_only = ON;
            PRAGMA busy_timeout = 5000;
            ",
        )
        .map_err(|error| format!("failed to mark {} query-only: {error}", db_path.display()))?;
    Ok(ReadOnlySqliteOpen {
        connection,
        read_only_mode_used: if immutable_mode_used {
            "sqlite_uri_mode_ro_immutable_query_only".to_string()
        } else {
            "sqlite_uri_mode_ro_query_only".to_string()
        },
        immutable_mode_used,
        immutable_mode_reason,
    })
}

fn sqlite_read_only_uri(db_path: &Path, immutable: bool) -> Result<String, String> {
    let absolute = if db_path.is_absolute() {
        db_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(db_path)
    };
    let mut raw = absolute.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = raw.strip_prefix("//?/") {
        raw = stripped.to_string();
    }
    let encoded = percent_encode_sqlite_uri_path(&raw);
    let path = encoded.trim_start_matches('/');
    let immutable_param = if immutable { "&immutable=1" } else { "" };
    Ok(format!("file:///{path}?mode=ro{immutable_param}"))
}

fn percent_encode_sqlite_uri_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        let keep =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':');
        if keep {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn doctor_lifecycle_field_status(
    preflight: &DbLifecycleSurfacePreflight,
    needles: &[&str],
    fallback: &str,
) -> String {
    if preflight
        .blockers
        .iter()
        .chain(preflight.lifecycle_preflight.db_health.reasons.iter())
        .any(|reason| needles.iter().any(|needle| reason.contains(needle)))
    {
        "mismatched".to_string()
    } else {
        fallback.to_string()
    }
}

fn run_config_command(args: &[String]) -> Result<Value, String> {
    let args = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .cloned()
        .collect::<Vec<_>>();
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Ok(json!({
            "status": "ok",
            "phase": PHASE,
            "config": {
                "default_db": ".codegraph/codegraph.sqlite",
                "env_db_override": "CODEGRAPH_DB_PATH",
                "workflow": "single-agent-only",
                "source_of_truth": "MVP.md",
            },
            "global_flags": ["--repo", "--db", "--json", "--no-color", "--verbose", "--quiet", "--profile"],
            "install_paths": release_install_paths_json(),
            "commands": COMMANDS.iter().map(|command| command.name).collect::<Vec<_>>(),
        }));
    };
    match subcommand {
        "show" => run_config_command(&[]),
        "release-metadata" => Ok(json!({
            "status": "ok",
            "release": release_metadata_json(),
        })),
        "completions" => {
            let shell = completion_shell(&args)?;
            Ok(json!({
                "status": "ok",
                "shell": shell,
                "script": shell_completion_script(&shell),
            }))
        }
        other => Err(format!("unknown config subcommand: {other}")),
    }
}

fn render_languages_table() -> String {
    let mut rows = Vec::new();
    rows.push(format!(
        "{:<12} {:<20} {:<6} {:<8} {:<8} {:<8} {}",
        "Language", "Extensions", "Tier", "Grammar", "Compiler", "LSP", "Exactness"
    ));
    rows.push("-".repeat(96));
    for frontend in language_frontends() {
        let exactness = frontend
            .extractors
            .iter()
            .map(|extractor| extractor.exactness.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");
        rows.push(format!(
            "{:<12} {:<20} {:<6} {:<8} {:<8} {:<8} {}",
            frontend.language_id,
            frontend.file_extensions.join(","),
            frontend.support_tier.number(),
            yes_no(frontend.tree_sitter_grammar_available),
            yes_no(frontend.compiler_resolver_available),
            yes_no(frontend.lsp_resolver_available),
            exactness,
        ));
    }
    rows.push(String::new());
    rows.push("Support tiers: 0=file discovery, 1=syntax/entities, 2=imports/exports/packages, 3=calls, 4=compiler/LSP verification, 5=dataflow/security/test impact.".to_string());
    rows.push("Unsupported capabilities are explicit in `codegraph-mcp languages --json`; new language frontends do not fake call/dataflow/security support.".to_string());
    format!("{}\n", rows.join("\n"))
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn run_query_command(args: &[String]) -> Result<Value, String> {
    let mut args = args.to_vec();
    let candidate_spool_path = remove_path_option(
        &mut args,
        &["--candidate-spool", "--candidate_spool", "--spool"],
    )?;
    let early_candidates = remove_flag(&mut args, "--early-candidates")
        || remove_flag(&mut args, "--early_candidates")
        || candidate_spool_path.is_some();
    let allow_stale_candidate_spool = remove_flag(&mut args, "--allow-stale-candidate-spool")
        || remove_flag(&mut args, "--allow_stale_candidate_spool")
        || remove_flag(&mut args, "--diagnostic-candidate-spool");
    reject_misplaced_global_flags_in_query_args(&args)?;
    let allow_stale_read = remove_flag(&mut args, "--allow-stale-read");
    let allow_foreign_db = remove_flag(&mut args, "--allow-foreign-db");
    let explicit_scope = parse_read_scope_options(&mut args)?;
    let repo_root = current_repo_root()?;
    if early_candidates {
        let spool_path =
            candidate_spool_path.unwrap_or_else(|| default_candidate_spool_path(&repo_root));
        return run_candidate_spool_query_command(
            &repo_root,
            &args,
            &spool_path,
            allow_stale_candidate_spool,
        );
    }
    if args.first().map(String::as_str) == Some("unresolved-calls") {
        return run_query_command_inner(
            &args,
            allow_stale_read,
            allow_foreign_db,
            explicit_scope,
            None,
        );
    }
    let db_path = resolved_db_path_for_repo(&repo_root);
    let db_lifecycle_read = read_db_lifecycle_guard(
        &repo_root,
        &db_path,
        allow_stale_read,
        allow_foreign_db,
        explicit_scope.clone(),
    )?;
    let compact_lifecycle = compact_lifecycle_summary(&db_lifecycle_read);
    let result = with_lifecycle_override_env(allow_stale_read, allow_foreign_db, || {
        run_query_command_inner(
            &args,
            allow_stale_read,
            allow_foreign_db,
            explicit_scope,
            Some(compact_lifecycle),
        )
    });
    let mut value = result?;
    if !query_response_uses_compact_lifecycle(&value) {
        if let Some(object) = value.as_object_mut() {
            object.insert("db_lifecycle_read".to_string(), db_lifecycle_read);
        }
    }
    add_query_resolution_fields(&mut value, &repo_root, &db_path);
    Ok(value)
}

fn reject_misplaced_global_flags_in_query_args(args: &[String]) -> Result<(), String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Ok(());
    };
    if let Some(error) = misplaced_global_flag_error(subcommand, "query") {
        return Err(error);
    }

    let disallowed_flags: &[&str] = match subcommand {
        "symbols" | "text" | "files" | "callers" | "callees" => {
            &["--repo", "--db", "--no-color", "--quiet", "--profile"]
        }
        "unresolved-calls" => &["--repo", "--no-color", "--quiet", "--profile", "--verbose"],
        _ => &[],
    };
    if disallowed_flags.is_empty() {
        return Ok(());
    }

    for arg in args.iter().skip(1) {
        if arg == "--" {
            break;
        }
        if let Some(flag) = canonical_global_flag(arg) {
            if disallowed_flags.contains(&flag) {
                return Err(misplaced_global_flag_message(flag, "query"));
            }
        }
    }
    Ok(())
}

fn run_query_command_inner(
    args: &[String],
    allow_stale_read: bool,
    allow_foreign_db: bool,
    explicit_scope_policy: Option<IndexScopeOptions>,
    lifecycle_summary: Option<Value>,
) -> Result<Value, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Err("Usage: codegraph-mcp query <symbols|text|files|references|definitions|callers|callees|chain|unresolved-calls|path> [ARGS]".to_string());
    };
    match subcommand {
        "symbols" => {
            if args.len() < 2 {
                return Err(list_query_usage("symbols"));
            }
            let options = parse_list_query_args("symbols", &args[1..])?;
            query_symbols_with_options(&current_repo_root()?, &options, lifecycle_summary.as_ref())
        }
        "text" => {
            if args.len() < 2 {
                return Err(list_query_usage("text"));
            }
            let options = parse_list_query_args("text", &args[1..])?;
            query_text_with_options(&current_repo_root()?, &options, lifecycle_summary.as_ref())
        }
        "files" => {
            if args.len() < 2 {
                return Err(list_query_usage("files"));
            }
            let options = parse_list_query_args("files", &args[1..])?;
            query_files_with_options(&current_repo_root()?, &options, lifecycle_summary.as_ref())
        }
        "references" => {
            if args.len() < 2 {
                return Err("Usage: codegraph-mcp query references <symbol>".to_string());
            }
            let options = parse_list_query_args("references", &args[1..])?;
            query_references_with_options(
                &current_repo_root()?,
                &options,
                lifecycle_summary.as_ref(),
            )
        }
        "definitions" => {
            if args.len() < 2 {
                return Err("Usage: codegraph-mcp query definitions <symbol>".to_string());
            }
            let options = parse_list_query_args("definitions", &args[1..])?;
            query_definitions_with_options(
                &current_repo_root()?,
                &options,
                lifecycle_summary.as_ref(),
            )
        }
        "callers" => {
            if args.len() < 2 {
                return Err(call_relation_usage("callers"));
            }
            let parsed = parse_call_relation_args_with_output("callers", &args[1..])?;
            query_call_relation_with_output(
                &current_repo_root()?,
                parsed,
                CallQueryDirection::Callers,
                lifecycle_summary.as_ref(),
            )
        }
        "callees" => {
            if args.len() < 2 {
                return Err(call_relation_usage("callees"));
            }
            let parsed = parse_call_relation_args_with_output("callees", &args[1..])?;
            query_call_relation_with_output(
                &current_repo_root()?,
                parsed,
                CallQueryDirection::Callees,
                lifecycle_summary.as_ref(),
            )
        }
        "chain" => {
            let options = parse_path_query_args("chain", &args[1..])?;
            query_chain_with_options(&current_repo_root()?, &options, lifecycle_summary.as_ref())
        }
        "unresolved-calls" => {
            let mut options = parse_unresolved_calls_args(&args[1..])?;
            options.allow_stale_read = allow_stale_read;
            options.allow_foreign_db = allow_foreign_db;
            options.explicit_scope_policy = explicit_scope_policy;
            query_unresolved_calls(&current_repo_root()?, options)
        }
        "path" => {
            let options = parse_path_query_args("path", &args[1..])?;
            query_path_with_options(&current_repo_root()?, &options, lifecycle_summary.as_ref())
        }
        other => Err(format!("unknown query subcommand: {other}")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryOutputMode {
    RichJson,
    Concise,
    AgentJson,
}

impl QueryOutputMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::RichJson => "json",
            Self::Concise => "concise",
            Self::AgentJson => "agent_json",
        }
    }

    fn is_compact(self) -> bool {
        matches!(self, Self::Concise | Self::AgentJson)
    }
}

#[derive(Debug, Clone)]
struct QueryListOptions {
    query: String,
    limit: usize,
    explicit_limit: bool,
    output_mode: QueryOutputMode,
    verbose: bool,
    debug: bool,
    explain: bool,
}

impl QueryListOptions {
    fn rich(query: &str, limit: usize) -> Self {
        Self {
            query: query.to_string(),
            limit: sanitize_query_limit(limit),
            explicit_limit: true,
            output_mode: QueryOutputMode::RichJson,
            verbose: false,
            debug: false,
            explain: false,
        }
    }

    fn fetch_limit(&self) -> usize {
        if self.output_mode.is_compact() {
            self.limit.saturating_add(1).min(MAX_QUERY_RESULT_LIMIT + 1)
        } else {
            self.limit
        }
    }
}

#[derive(Debug, Clone)]
struct QueryOutputOptions {
    limit: usize,
    explicit_limit: bool,
    output_mode: QueryOutputMode,
}

impl QueryOutputOptions {
    fn fetch_limit(&self) -> usize {
        if self.output_mode.is_compact() {
            self.limit.saturating_add(1).min(MAX_QUERY_RESULT_LIMIT + 1)
        } else {
            self.limit
        }
    }
}

#[derive(Debug, Clone)]
struct ParsedCallRelationArgs {
    options: CallRelationQueryOptions,
    output: QueryOutputOptions,
}

#[derive(Debug, Clone)]
struct PathQueryOptions {
    source: String,
    target: String,
    output: QueryOutputOptions,
}

#[derive(Debug, Clone)]
struct QueryTruncation {
    returned_count: usize,
    limit: usize,
    omitted_count: usize,
    omitted_count_is_lower_bound: bool,
    total_available_unknown: bool,
}

fn sanitize_query_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_QUERY_RESULT_LIMIT)
}

fn parse_limit_value(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map(sanitize_query_limit)
        .map_err(|_| "invalid --limit value".to_string())
}

fn parse_list_query_args(command_name: &str, args: &[String]) -> Result<QueryListOptions, String> {
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
            "--limit" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(list_query_usage(command_name));
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
                    "unknown query {command_name} option: {value}\n{}",
                    list_query_usage(command_name)
                ));
            }
            value => query_parts.push(value.to_string()),
        }
        index += 1;
    }

    if query_parts.is_empty() {
        return Err(list_query_usage(command_name));
    }
    let default_limit = match output_mode {
        QueryOutputMode::AgentJson => DEFAULT_QUERY_AGENT_JSON_LIMIT,
        QueryOutputMode::Concise => DEFAULT_QUERY_JSON_LIMIT,
        QueryOutputMode::RichJson if verbose || debug || explain => DEFAULT_QUERY_VERBOSE_LIMIT,
        QueryOutputMode::RichJson => DEFAULT_QUERY_JSON_LIMIT,
    };
    Ok(QueryListOptions {
        query: query_parts.join(" "),
        limit: explicit_limit.unwrap_or(default_limit),
        explicit_limit: explicit_limit.is_some(),
        output_mode,
        verbose,
        debug,
        explain,
    })
}

fn parse_path_query_args(command_name: &str, args: &[String]) -> Result<PathQueryOptions, String> {
    let mut output_mode = QueryOutputMode::RichJson;
    let mut explicit_limit = None;
    let mut terms = Vec::new();
    let mut index = 0usize;
    let mut literal_terms = false;
    while index < args.len() {
        if literal_terms {
            terms.push(args[index].to_string());
            index += 1;
            continue;
        }
        match args[index].as_str() {
            "--" => literal_terms = true,
            "--limit" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(path_query_usage(command_name));
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
                    "unknown query {command_name} option: {value}\n{}",
                    path_query_usage(command_name)
                ));
            }
            value => terms.push(value.to_string()),
        }
        index += 1;
    }

    if terms.len() != 2 {
        return Err(path_query_usage(command_name));
    }
    let default_limit = match output_mode {
        QueryOutputMode::AgentJson => DEFAULT_QUERY_AGENT_JSON_LIMIT,
        QueryOutputMode::Concise | QueryOutputMode::RichJson => DEFAULT_QUERY_JSON_LIMIT,
    };
    let limit = explicit_limit.unwrap_or(default_limit);
    Ok(PathQueryOptions {
        source: terms[0].clone(),
        target: terms[1].clone(),
        output: QueryOutputOptions {
            limit,
            explicit_limit: explicit_limit.is_some(),
            output_mode,
        },
    })
}

fn path_query_usage(command_name: &str) -> String {
    format!(
        "Usage: codegraph-mcp query {command_name} <source> <target> [--limit <n>] [--concise|--agent-json]"
    )
}

fn list_query_usage(command_name: &str) -> String {
    format!(
        "Usage: codegraph-mcp query {command_name} <query> [--limit <n>] [--concise|--agent-json] [--candidate-spool <path> --early-candidates] [--verbose|--debug|--explain]\nLiteral flag-like query terms: codegraph-mcp query {command_name} [options] -- --db"
    )
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CandidateSpoolLoad {
    path: PathBuf,
    metadata: Value,
    chunks: Vec<Value>,
    stale: bool,
    reason: Option<String>,
}

fn default_candidate_spool_path(repo_root: &Path) -> PathBuf {
    resolved_db_path_for_repo(repo_root)
        .parent()
        .map(|parent| parent.join("codegraph-candidate-spool.jsonl"))
        .unwrap_or_else(|| PathBuf::from("codegraph-candidate-spool.jsonl"))
}

fn run_candidate_spool_query_command(
    repo_root: &Path,
    args: &[String],
    spool_path: &Path,
    allow_stale: bool,
) -> Result<Value, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Err("Usage: codegraph-mcp query <files|text|symbols> <query> --candidate-spool <path> --early-candidates".to_string());
    };
    if !matches!(subcommand, "files" | "text" | "symbols") {
        return Err(format!(
            "candidate spool early mode only supports query files/text/symbols, got {subcommand}"
        ));
    }
    if args.len() < 2 {
        return Err(list_query_usage(subcommand));
    }
    let options = parse_list_query_args(subcommand, &args[1..])?;
    let started = Instant::now();
    let query_result = query_candidate_spool_index_for_repo(
        repo_root,
        spool_path,
        subcommand,
        &options.query,
        options.fetch_limit(),
        allow_stale,
    )
    .map_err(|error| error.to_string())?;
    let hits = candidate_spool_index_query_hits(&query_result, subcommand);
    let lifecycle = candidate_spool_index_lifecycle_json(&query_result.load);
    let staged_availability =
        staged_availability_for_spool_index_only(repo_root, spool_path, &query_result.load, None);
    let staged_fields = staged_availability_top_level_fields(&staged_availability);
    if options.output_mode.is_compact() {
        let (hits, truncation) = truncate_for_agent(hits, options.limit);
        let mut value = json!({
            "schema_name": format!("query_{subcommand}_candidate_spool_agent_json"),
            "schema_version": AGENT_JSON_SCHEMA_VERSION,
            "status": "ok",
            "command": format!("query {subcommand}"),
            "output_mode": options.output_mode.as_str(),
            "repo": path_string(repo_root),
            "db": Value::Null,
            "candidate_spool": lifecycle,
            "index_in_progress": query_result.load.metadata.get("incomplete").and_then(Value::as_bool).unwrap_or(true),
            "proof_status": "candidate_only",
            "proof_strength": candidate_spool_proof_strength(subcommand, &Value::Null),
            "graph_proof": false,
            "candidate_only": true,
            "staged_availability": staged_availability.clone(),
            "query": {
                "text": options.query,
                "explicit_limit": options.explicit_limit,
            },
            "results": hits,
            "result_count": hits.len(),
            "omitted_count": query_result.omitted_count,
            "limit": options.limit,
            "truncation": query_truncation_json(&truncation),
            "timings": agent_timings_json(started),
            "warnings": staged_warning_values(&staged_availability),
            "errors": [],
        });
        merge_json_object(&mut value, staged_fields);
        return Ok(value);
    }
    let hits = hits.into_iter().take(options.limit).collect::<Vec<_>>();
    let mut value = json!({
        "status": "ok",
        "query": options.query,
        "result_count": hits.len(),
        "limit": options.limit,
        "explicit_limit": options.explicit_limit,
        "output_mode": options.output_mode.as_str(),
        "candidate_spool": lifecycle,
        "index_in_progress": query_result.load.metadata.get("incomplete").and_then(Value::as_bool).unwrap_or(true),
        "proof_status": "candidate_only",
        "proof_strength": candidate_spool_proof_strength(subcommand, &Value::Null),
        "graph_proof": false,
        "candidate_only": true,
        "staged_availability": staged_availability.clone(),
        "hits": hits,
        "omitted_count": query_result.omitted_count,
        "proof": "Fast Candidate Spool query returns candidate-only source-navigation evidence; it cannot answer graph proof.",
    });
    merge_json_object(&mut value, staged_fields);
    Ok(value)
}

#[allow(dead_code)]
fn load_candidate_spool_for_repo(
    repo_root: &Path,
    spool_path: &Path,
    allow_stale: bool,
) -> Result<CandidateSpoolLoad, String> {
    if !spool_path.exists() {
        return Err(format!(
            "candidate_spool_missing: {} does not exist",
            spool_path.display()
        ));
    }
    let text = fs::read_to_string(spool_path)
        .map_err(|error| format!("candidate_spool_read_failed: {error}"))?;
    let mut metadata = Value::Null;
    let mut chunks = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|error| format!("candidate_spool_corrupt line {}: {error}", line_index + 1))?;
        if value.get("chunk_id").is_some() || value.get("text").is_some() {
            chunks.push(value);
        } else if metadata.is_null() {
            metadata = value
                .get("metadata")
                .cloned()
                .unwrap_or_else(|| value.clone());
        }
    }
    let kind = metadata
        .get("artifact_kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if kind != "candidate_spool" {
        return Err(format!(
            "candidate_spool_wrong_kind: expected candidate_spool, got {kind}"
        ));
    }
    let mut stale_reasons = Vec::new();
    if let Some(recorded_repo) = metadata.get("repo_root").and_then(Value::as_str) {
        let current_repo = path_string(repo_root);
        if !paths_equivalent_string(recorded_repo, &current_repo) {
            stale_reasons.push(format!(
                "foreign_repo: artifact repo_root={recorded_repo}, current_repo_root={current_repo}"
            ));
        }
    }
    for chunk in &chunks {
        let Some(path) = chunk.get("path").and_then(Value::as_str) else {
            continue;
        };
        let source_path = repo_root.join(path);
        if !source_path.exists() {
            stale_reasons.push(format!("deleted_file: {path}"));
            continue;
        }
        if let Some(expected_size) = chunk.get("source_file_size_bytes").and_then(Value::as_u64) {
            let actual_size = fs::metadata(&source_path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if actual_size != expected_size {
                stale_reasons.push(format!(
                    "changed_file_size: {path} expected={expected_size} actual={actual_size}"
                ));
                continue;
            }
        }
        if let Some(expected_hash) = chunk
            .get("source_file_content_hash")
            .and_then(Value::as_str)
        {
            match fs::read_to_string(&source_path) {
                Ok(source) => {
                    let actual_hash = content_hash(&source);
                    if actual_hash != expected_hash {
                        stale_reasons.push(format!("changed_file_hash: {path}"));
                    }
                }
                Err(error) => stale_reasons.push(format!("read_failed: {path}: {error}")),
            }
        }
    }
    stale_reasons.sort();
    stale_reasons.dedup();
    let stale = !stale_reasons.is_empty();
    if stale && !allow_stale {
        return Err(format!(
            "candidate_spool_stale: {}",
            stale_reasons.join("; ")
        ));
    }
    Ok(CandidateSpoolLoad {
        path: spool_path.to_path_buf(),
        metadata,
        chunks,
        stale,
        reason: stale.then(|| stale_reasons.join("; ")),
    })
}

#[allow(dead_code)]
fn paths_equivalent_string(left: &str, right: &str) -> bool {
    let left = normalize_path_identity_string(left);
    let right = normalize_path_identity_string(right);
    left == right || windows_path_identity_strings_equivalent(&left, &right)
}

fn normalize_path_identity_string(value: &str) -> String {
    if let Ok(canonical) = fs::canonicalize(Path::new(value)) {
        return normalize_path_identity_display(&path_string(&canonical));
    }
    normalize_path_identity_display(value)
}

fn normalize_path_identity_display(value: &str) -> String {
    let mut normalized = value.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("//?/UNC/") {
        normalized = format!("//{rest}");
    } else if let Some(rest) = normalized.strip_prefix("//?/") {
        normalized = rest.to_string();
    }
    normalized.trim_end_matches('/').to_ascii_lowercase()
}

#[cfg(windows)]
fn windows_path_identity_strings_equivalent(left: &str, right: &str) -> bool {
    let left_parts = left
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let right_parts = right
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    left_parts.len() == right_parts.len()
        && left_parts
            .iter()
            .zip(right_parts.iter())
            .all(|(left, right)| windows_path_component_equivalent(left, right))
}

#[cfg(not(windows))]
fn windows_path_identity_strings_equivalent(_left: &str, _right: &str) -> bool {
    false
}

#[cfg(windows)]
fn windows_path_component_equivalent(left: &str, right: &str) -> bool {
    left == right
        || windows_short_alias_component_matches(left, right)
        || windows_short_alias_component_matches(right, left)
}

#[cfg(windows)]
fn windows_short_alias_component_matches(short: &str, long: &str) -> bool {
    let Some((prefix, suffix)) = short.split_once('~') else {
        return false;
    };
    if prefix.len() < 3 {
        return false;
    }
    let digit_count = suffix.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digit_count == 0 {
        return false;
    }
    let suffix_tail = &suffix[digit_count..];
    if !suffix_tail.is_empty() && !suffix_tail.starts_with('.') {
        return false;
    }
    let long_compact = long
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    long_compact.starts_with(prefix)
}

fn candidate_spool_index_lifecycle_json(load: &CandidateSpoolIndexLoad) -> Value {
    json!({
        "candidate_spool_status": if load.stale {
            "stale"
        } else {
            load.metadata
                .get("candidate_spool_status")
                .and_then(Value::as_str)
                .or_else(|| load.metadata.get("status").and_then(Value::as_str))
                .unwrap_or("unknown")
        },
        "candidate_spool_path": path_string(&load.path),
        "artifact_kind": "candidate_spool",
        "artifact_format": load.metadata.get("artifact_format").and_then(Value::as_str).unwrap_or("jsonl"),
        "lifecycle": load.metadata.get("lifecycle").cloned().unwrap_or(Value::Null),
        "candidate_spool_policy": load.metadata.get("candidate_spool_policy").cloned().unwrap_or_else(|| json!("bounded")),
        "candidate_spool_required": load.metadata.get("candidate_spool_required").cloned().unwrap_or_else(|| json!(false)),
        "candidate_spool_budget_bytes": load.metadata.get("candidate_spool_budget_bytes").cloned().unwrap_or(Value::Null),
        "candidate_spool_written_bytes": load.metadata.get("candidate_spool_written_bytes").cloned().unwrap_or(Value::Null),
        "candidate_spool_truncated": load.metadata.get("candidate_spool_truncated").cloned().unwrap_or_else(|| json!(false)),
        "candidate_spool_disabled_reason": load.metadata.get("candidate_spool_disabled_reason").cloned().unwrap_or(Value::Null),
        "candidate_spool_warning": load.metadata.get("candidate_spool_warning").cloned().unwrap_or(Value::Null),
        "artifact_budget_remaining_bytes": load.metadata.get("artifact_budget_remaining_bytes").cloned().unwrap_or(Value::Null),
        "artifact_budget_decision": load.metadata.get("artifact_budget_decision").cloned().unwrap_or(Value::Null),
        "incomplete": load.metadata.get("incomplete").and_then(Value::as_bool).unwrap_or(true),
        "stale": load.stale,
        "reason": load.reason.clone(),
        "spooled_total_chunks": load.query_index_record_count,
        "query_index_status": load.query_index_status.clone(),
        "query_index_kind": load.query_index_kind.clone(),
        "query_index_path": path_string(&load.query_index_path),
        "query_index_bytes": load.query_index_bytes,
        "query_index_record_count": load.query_index_record_count,
        "query_index_source_binding_count": load.query_index_source_binding_count,
        "query_index_version": load.query_index_version.clone(),
        "query_index_bound_manifest_hash": load.query_index_bound_manifest_hash.clone(),
        "candidate_only": true,
        "graph_proof": false,
        "claimable_for_graph": false,
        "creates_graph_relations": false,
        "can_answer_graph_proof": false,
        "source_navigation_evidence": true,
    })
}

fn candidate_spool_index_query_hits(
    query_result: &CandidateSpoolIndexQueryResult,
    subcommand: &str,
) -> Vec<Value> {
    query_result
        .chunks
        .iter()
        .map(|chunk| candidate_spool_hit_json(chunk, subcommand))
        .collect()
}

#[allow(dead_code)]
fn candidate_spool_lifecycle_json(spool: &CandidateSpoolLoad) -> Value {
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
        "creates_graph_relations": false,
        "can_answer_graph_proof": false,
        "source_navigation_evidence": true,
    })
}

#[allow(dead_code)]
fn candidate_spool_query_hits(
    spool: &CandidateSpoolLoad,
    subcommand: &str,
    query: &str,
    limit: usize,
) -> Vec<Value> {
    let query_lc = query.to_ascii_lowercase();
    let aliases = split_file_query_aliases(query);
    let mut scored = Vec::new();
    for chunk in &spool.chunks {
        if !candidate_spool_chunk_matches_subcommand(chunk, subcommand) {
            continue;
        }
        let haystack = candidate_spool_chunk_haystack(chunk);
        let mut score = 0i64;
        if haystack.contains(&query_lc) {
            score += 100;
        }
        for alias in &aliases {
            if alias.len() >= 2 && haystack.contains(alias) {
                score += 10;
            }
        }
        if score == 0 {
            continue;
        }
        scored.push((score, candidate_spool_hit_json(chunk, subcommand)));
    }
    scored.sort_by(|left, right| {
        right.0.cmp(&left.0).then_with(|| {
            candidate_spool_sort_key(&left.1).cmp(&candidate_spool_sort_key(&right.1))
        })
    });
    scored
        .into_iter()
        .map(|(_, value)| value)
        .take(limit)
        .collect()
}

#[allow(dead_code)]
fn candidate_spool_chunk_matches_subcommand(chunk: &Value, subcommand: &str) -> bool {
    let chunk_kind = chunk
        .get("chunk_kind")
        .and_then(Value::as_str)
        .unwrap_or("");
    let source_kind = chunk
        .get("source_kind")
        .and_then(Value::as_str)
        .unwrap_or("");
    let selection_bucket = chunk
        .get("selection_bucket")
        .and_then(Value::as_str)
        .unwrap_or("");
    match subcommand {
        "files" => chunk_kind == "file_path_title",
        "text" => source_kind == "text_evidence" || chunk_kind == "snippet",
        "symbols" => {
            source_kind == "graph_entity"
                || selection_bucket.contains("symbol")
                || selection_bucket.contains("import")
                || selection_bucket.contains("export")
        }
        _ => false,
    }
}

#[allow(dead_code)]
fn candidate_spool_chunk_haystack(chunk: &Value) -> String {
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

fn candidate_spool_hit_json(chunk: &Value, subcommand: &str) -> Value {
    let span = chunk.get("source_span").cloned().unwrap_or(Value::Null);
    let line = span
        .get("start_line")
        .and_then(Value::as_u64)
        .or_else(|| span.get("line").and_then(Value::as_u64));
    let text = chunk.get("text").and_then(Value::as_str).unwrap_or("");
    json!({
        "kind": subcommand.trim_end_matches('s'),
        "id": chunk.get("chunk_id").cloned().unwrap_or(Value::Null),
        "chunk_id": chunk.get("chunk_id").cloned().unwrap_or(Value::Null),
        "chunk_kind": chunk.get("chunk_kind").cloned().unwrap_or(Value::Null),
        "source_kind": chunk.get("source_kind").cloned().unwrap_or(Value::Null),
        "repo_relative_path": chunk.get("path").cloned().unwrap_or(Value::Null),
        "path": chunk.get("path").cloned().unwrap_or(Value::Null),
        "line": line.map(Value::from).unwrap_or(Value::Null),
        "source_span": span,
        "entity_id": chunk.get("entity_id").cloned().unwrap_or(Value::Null),
        "title": chunk.get("path").cloned().unwrap_or(Value::Null),
        "text": truncate_candidate_spool_text(text),
        "text_preview_truncated": text.len() > RETRIEVAL_CANDIDATE_SNIPPET_MAX_BYTES,
        "match": "candidate_spool",
        "score": chunk.get("selection_score").cloned().unwrap_or_else(|| json!(0.0)),
        "selection_bucket": chunk.get("selection_bucket").cloned().unwrap_or(Value::Null),
        "selection_reason": chunk.get("selection_reason").cloned().unwrap_or(Value::Null),
        "proof_status": "candidate_only",
        "proof_strength": candidate_spool_proof_strength(subcommand, chunk),
        "graph_proof": false,
        "claimable_for_graph": false,
        "requires_graph_verification": true,
        "verification_status": "needs_graph_verification",
        "graph_verification_status": "needs_graph_verification",
        "source_navigation_evidence": true,
        "candidate_only": true,
        "index_in_progress": chunk.get("incomplete").and_then(Value::as_bool).unwrap_or(true),
    })
}

fn candidate_spool_proof_strength(subcommand: &str, chunk: &Value) -> &'static str {
    match subcommand {
        "text" => "text_evidence",
        "symbols" => "symbol_evidence",
        "files" => "source_navigation_evidence",
        _ => match chunk.get("source_kind").and_then(Value::as_str) {
            Some("text_evidence") => "text_evidence",
            Some("graph_entity") => "symbol_evidence",
            _ => "candidate_evidence",
        },
    }
}

fn default_vector_runtime_sidecar_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(|parent| parent.join(CONTEXT_PACK_VECTOR_INDEX_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(CONTEXT_PACK_VECTOR_INDEX_FILE_NAME))
}

fn default_vector_audit_artifact_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(|parent| parent.join(CONTEXT_PACK_VECTOR_AUDIT_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(CONTEXT_PACK_VECTOR_AUDIT_FILE_NAME))
}

fn resolve_optional_artifact_path(path: Option<&Path>, default_path: PathBuf) -> PathBuf {
    let path = path.map(Path::to_path_buf).unwrap_or(default_path);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    }
}

fn merge_json_object(target: &mut Value, fields: Value) {
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

fn staged_availability_for_cli(
    repo_root: &Path,
    db_path: &Path,
    preflight: Option<&DbLifecyclePreflight>,
    candidate_spool_path: Option<&Path>,
    vector_runtime_path: Option<&Path>,
    vector_audit_path: Option<&Path>,
    vector_branch: Option<&ContextPackVectorBranch>,
) -> Value {
    let graph = graph_db_layer_status(preflight);
    let spool_path = resolve_optional_artifact_path(
        candidate_spool_path,
        default_candidate_spool_path(repo_root),
    );
    let spool = candidate_spool_layer_status(repo_root, &spool_path);
    let runtime_path = vector_branch
        .map(|branch| branch.index_path.clone())
        .unwrap_or_else(|| {
            resolve_optional_artifact_path(
                vector_runtime_path,
                default_vector_runtime_sidecar_path(db_path),
            )
        });
    let runtime =
        vector_runtime_layer_status(repo_root, db_path, preflight, &runtime_path, vector_branch);
    let audit_path = resolve_optional_artifact_path(
        vector_audit_path,
        default_vector_audit_artifact_path(db_path),
    );
    let audit = vector_audit_layer_status(db_path, preflight, &audit_path);
    staged_availability_from_layers(graph, spool, runtime, audit)
}

fn staged_availability_for_context_pack(
    repo_root: &Path,
    db_path: &Path,
    db_lifecycle_read: &Value,
    options: &ContextPackOptions,
    vector_branch: Option<&ContextPackVectorBranch>,
) -> Value {
    let graph = graph_db_layer_status_from_lifecycle(db_lifecycle_read);
    let spool_path = resolve_optional_artifact_path(
        options.candidate_spool_path.as_deref(),
        default_candidate_spool_path(repo_root),
    );
    let spool = candidate_spool_layer_status(repo_root, &spool_path);
    let runtime_path = vector_branch
        .map(|branch| branch.index_path.clone())
        .unwrap_or_else(|| {
            resolve_optional_artifact_path(
                options.vector_index_path.as_deref(),
                default_vector_runtime_sidecar_path(db_path),
            )
        });
    let runtime = if let Some(branch) = vector_branch {
        vector_runtime_layer_status_from_branch(&runtime_path, branch)
    } else if options.enable_vector_candidates || options.vector_index_path.is_some() {
        vector_runtime_layer_status(repo_root, db_path, None, &runtime_path, None)
    } else {
        vector_runtime_layer_status_not_requested(&runtime_path)
    };
    let audit_path = resolve_optional_artifact_path(
        options.vector_audit_artifact_path.as_deref(),
        default_vector_audit_artifact_path(db_path),
    );
    let audit = vector_audit_layer_status(db_path, None, &audit_path);
    staged_availability_from_layers(graph, spool, runtime, audit)
}

fn staged_availability_for_spool_index_only(
    repo_root: &Path,
    spool_path: &Path,
    spool: &CandidateSpoolIndexLoad,
    db_error: Option<&str>,
) -> Value {
    let graph = json!({
        "layer": "graph_db",
        "status": "building",
        "ready": false,
        "graph_proof_available": false,
        "path": Value::Null,
        "reason": db_error.unwrap_or("Graph DB is still building or unavailable."),
    });
    let spool_layer = candidate_spool_layer_from_index_load(spool_path, spool);
    let db_path = resolved_db_path_for_repo(repo_root);
    let runtime =
        vector_runtime_layer_status_not_requested(&default_vector_runtime_sidecar_path(&db_path));
    let audit = vector_audit_layer_status(
        &db_path,
        None,
        &default_vector_audit_artifact_path(&db_path),
    );
    staged_availability_from_layers(graph, spool_layer, runtime, audit)
}

fn graph_db_layer_status(preflight: Option<&DbLifecyclePreflight>) -> Value {
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
        "repo_root_status": preflight.repo_root_status.clone(),
        "blockers": preflight.blockers.clone(),
        "warnings": preflight.warnings.clone(),
    })
}

fn graph_db_layer_status_from_lifecycle(lifecycle: &Value) -> Value {
    let ready = lifecycle
        .get("claimable")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && !lifecycle
            .get("diagnostic_only")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    json!({
        "layer": "graph_db",
        "status": if ready { "ready" } else { lifecycle.get("db_problem_kind").and_then(Value::as_str).unwrap_or("blocked") },
        "ready": ready,
        "graph_proof_available": ready,
        "path": lifecycle.get("exact_db_path_checked").cloned().unwrap_or(Value::Null),
        "lifecycle_decision": lifecycle.get("decision").cloned().unwrap_or_else(|| json!(if ready { "read_reuse" } else { "blocked" })),
        "claimable": lifecycle.get("claimable").cloned().unwrap_or_else(|| json!(ready)),
        "diagnostic_only": lifecycle.get("diagnostic_only").cloned().unwrap_or_else(|| json!(!ready)),
        "passport_status": lifecycle.get("passport_status").cloned().unwrap_or(Value::Null),
        "schema_status": lifecycle.get("schema_status").cloned().unwrap_or(Value::Null),
        "scope_status": lifecycle.get("scope_status").cloned().unwrap_or(Value::Null),
        "blockers": lifecycle.get("blockers").cloned().unwrap_or_else(|| json!([])),
        "warnings": lifecycle.get("warnings").cloned().unwrap_or_else(|| json!([])),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SidecarAccessClassification {
    status: &'static str,
    problem_kind: &'static str,
    path_access_status: &'static str,
}

fn sidecar_access_classification_from_message(message: &str) -> SidecarAccessClassification {
    let lower = message.to_ascii_lowercase();
    // Passport / version / scope incompatibility means the sidecar was built
    // against an older graph DB and is now STALE (e.g. after a changed-file delta
    // updates the DB passport). This is not an access error: it must classify as
    // stale so the delta layer action reports `invalidated`, matching how the
    // candidate spool reports a passport-superseded sidecar.
    if lower.contains("changed")
        || lower.contains("mismatch")
        || lower.contains("stale")
        || lower.contains("superseded")
        || lower.contains("incompatible")
    {
        return SidecarAccessClassification {
            status: "stale",
            problem_kind: "sidecar_stale",
            path_access_status: "ok",
        };
    }
    if lower.contains("locked") || lower.contains("database is busy") {
        return SidecarAccessClassification {
            status: "sidecar_locked",
            problem_kind: "sidecar_locked",
            path_access_status: "ok",
        };
    }
    if lower.contains("readonly database")
        || lower.contains("read-only")
        || lower.contains("read only")
    {
        return SidecarAccessClassification {
            status: "read_only",
            problem_kind: "read_only",
            path_access_status: "permission_denied",
        };
    }
    if lower.contains("permission denied")
        || lower.contains("access is denied")
        || lower.contains("access permission denied")
        || lower.contains("authorization denied")
    {
        return SidecarAccessClassification {
            status: "permission_denied",
            problem_kind: "permission_denied",
            path_access_status: "permission_denied",
        };
    }
    if lower.contains("corrupt")
        || lower.contains("malformed")
        || lower.contains("not a database")
        || lower.contains("file is not a database")
        || lower.contains("failed to parse")
        || lower.contains("expected value")
    {
        return SidecarAccessClassification {
            status: "sidecar_corrupt",
            problem_kind: "sidecar_corrupt",
            path_access_status: "ok",
        };
    }
    if lower.contains("is a directory")
        || lower.contains("unable to open")
        || lower.contains("disk i/o error")
        || lower.contains("i/o error")
        || lower.contains("io error")
        || lower.contains("system cannot find")
    {
        return SidecarAccessClassification {
            status: "sidecar_unavailable",
            problem_kind: "filesystem_inaccessible",
            path_access_status: "filesystem_inaccessible",
        };
    }
    SidecarAccessClassification {
        status: "sidecar_unavailable",
        problem_kind: "filesystem_inaccessible",
        path_access_status: "filesystem_inaccessible",
    }
}

fn sidecar_access_error_layer_json(
    layer: &str,
    path: &Path,
    error: &str,
    diagnostic_only: bool,
) -> Value {
    let classification = sidecar_access_classification_from_message(error);
    json!({
        "layer": layer,
        "status": classification.status,
        "ready": false,
        "path": path_string(path),
        "path_access_status": classification.path_access_status,
        "sidecar_problem_kind": classification.problem_kind,
        "sidecar_access_label": classification.status,
        "candidate_only": true,
        "graph_proof": false,
        "diagnostic_only": diagnostic_only,
        "reason": error,
    })
}

fn candidate_spool_layer_status(repo_root: &Path, spool_path: &Path) -> Value {
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
        Ok(spool) => candidate_spool_layer_from_index_load(spool_path, &spool),
        Err(error) => {
            let error = error.to_string();
            if error.contains("foreign_repo") || error.contains("candidate_spool_stale") {
                json!({
                    "layer": "candidate_spool",
                    "status": "foreign",
                    "ready": false,
                    "path": path_string(spool_path),
                    "candidate_only": true,
                    "graph_proof": false,
                    "candidate_spool_unavailable": true,
                    "candidate_context_truncated": false,
                    "reason": error,
                })
            } else {
                let mut value =
                    sidecar_access_error_layer_json("candidate_spool", spool_path, &error, true);
                if let Some(object) = value.as_object_mut() {
                    object.insert("candidate_spool_unavailable".to_string(), json!(true));
                    object.insert("candidate_context_truncated".to_string(), json!(false));
                }
                value
            }
        }
    }
}

fn candidate_spool_layer_from_index_load(
    spool_path: &Path,
    spool: &CandidateSpoolIndexLoad,
) -> Value {
    let lifecycle = candidate_spool_index_lifecycle_json(spool);
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
        "query_index_problem_kind": spool.query_index_status.as_str(),
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
fn candidate_spool_layer_from_load(spool_path: &Path, spool: &CandidateSpoolLoad) -> Value {
    let lifecycle = candidate_spool_lifecycle_json(spool);
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

fn vector_runtime_layer_status(
    repo_root: &Path,
    db_path: &Path,
    preflight: Option<&DbLifecyclePreflight>,
    runtime_path: &Path,
    branch: Option<&ContextPackVectorBranch>,
) -> Value {
    if let Some(branch) = branch {
        return vector_runtime_layer_status_from_branch(runtime_path, branch);
    }
    if !runtime_path.exists() {
        return vector_runtime_layer_status_not_requested(runtime_path);
    }
    if preflight.is_some_and(|preflight| !preflight.safe) {
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
    if preflight.is_none() {
        return json!({
            "layer": "vector_runtime",
            "status": "present_unvalidated",
            "ready": false,
            "path": path_string(runtime_path),
            "candidate_only": true,
            "graph_proof": false,
            "reason": "runtime sidecar exists but was not validated against the graph DB in this compact status path",
        });
    }
    let provider = match context_pack_vector_provider() {
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
    let store = match SqliteGraphStore::open_read_only(db_path) {
        Ok(store) => store,
        Err(error) => {
            return json!({
                "layer": "vector_runtime",
                "status": "stale",
                "ready": false,
                "path": path_string(runtime_path),
                "candidate_only": true,
                "graph_proof": false,
                "reason": format!("graph DB open failed: {error}"),
            });
        }
    };
    let passport = match store.get_db_passport() {
        Ok(Some(passport)) => passport,
        Ok(None) => {
            return json!({
                "layer": "vector_runtime",
                "status": "stale",
                "ready": false,
                "path": path_string(runtime_path),
                "candidate_only": true,
                "graph_proof": false,
                "reason": "graph DB passport missing",
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
                "reason": format!("graph DB passport read failed: {error}"),
            });
        }
    };
    match load_vector_chunk_index_json(
        runtime_path,
        &provider,
        &passport,
        context_pack_vector_build_options(),
    ) {
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
            let mut metrics = context_pack_vector_index_metrics_json(runtime_path, &index);
            if let Some(object) = metrics.as_object_mut() {
                object.insert(
                    "source_binding_validation".to_string(),
                    serde_json::to_value(&source_binding_validation).unwrap_or(Value::Null),
                );
            }
            json!({
                "layer": "vector_runtime",
                "status": "ready",
                "ready": true,
                "path": path_string(runtime_path),
                "artifact_kind": metrics.get("artifact_kind").cloned().unwrap_or(Value::Null),
                "candidate_only": true,
                "graph_proof": false,
                "complete_path_symbol_index": false,
                "selected_chunks_only": true,
                "metrics": metrics,
            })
        }
        Err(error) => {
            let error = error.to_string();
            let classification = sidecar_access_classification_from_message(&error);
            json!({
                "layer": "vector_runtime",
                "status": classification.status,
                "ready": false,
                "path": path_string(runtime_path),
                "candidate_only": true,
                "graph_proof": false,
                "sidecar_problem_kind": classification.problem_kind,
                "sidecar_access_label": classification.status,
                "path_access_status": classification.path_access_status,
                "reason": error,
            })
        }
    }
}

fn vector_runtime_layer_status_from_branch(
    runtime_path: &Path,
    branch: &ContextPackVectorBranch,
) -> Value {
    json!({
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
        "metrics": branch.index_metrics.clone().unwrap_or(Value::Null),
    })
}

fn vector_runtime_layer_status_not_requested(runtime_path: &Path) -> Value {
    json!({
        "layer": "vector_runtime",
        "status": if runtime_path.exists() { "not_requested" } else { "missing" },
        "ready": false,
        "path": path_string(runtime_path),
        "candidate_only": true,
        "graph_proof": false,
        "complete_path_symbol_index": false,
        "reason": if runtime_path.exists() {
            "runtime vector sidecar exists but vector candidate lane was not enabled"
        } else {
            "runtime vector sidecar missing"
        },
    })
}

fn vector_audit_layer_status(
    db_path: &Path,
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
    let text = match fs::read_to_string(audit_path) {
        Ok(text) => text,
        Err(error) => {
            let mut value = sidecar_access_error_layer_json(
                "vector_audit",
                audit_path,
                &error.to_string(),
                true,
            );
            if let Some(object) = value.as_object_mut() {
                object.insert("runtime_dependency".to_string(), json!(false));
            }
            return value;
        }
    };
    let value = match serde_json::from_str::<Value>(&text) {
        Ok(value) => value,
        Err(error) => {
            return json!({
                "layer": "vector_audit",
                "status": "sidecar_corrupt",
                "ready": false,
                "path": path_string(audit_path),
                "diagnostic_only": true,
                "runtime_dependency": false,
                "sidecar_problem_kind": "sidecar_corrupt",
                "path_access_status": "ok",
                "reason": error.to_string(),
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
            let artifact_repo = metadata
                .get("passport")
                .and_then(|passport| passport.get("canonical_repo_root"))
                .and_then(Value::as_str);
            if artifact_scope.is_some()
                && artifact_scope != Some(passport.index_scope_policy_hash.as_str())
                || artifact_repo.is_some()
                    && artifact_repo != Some(passport.canonical_repo_root.as_str())
            {
                status = "stale";
                reason = Some(
                    "audit artifact passport scope/repo differs from current graph DB".to_string(),
                );
            }
        }
    } else if !db_path.exists() {
        status = "diagnostic_only";
        reason = Some("audit artifact exists without a validated graph DB".to_string());
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

fn staged_availability_from_layers(
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
    let mut layer_readiness = serde_json::Map::new();
    let mut available_layers = Vec::new();
    let mut missing_layers = Vec::new();
    let mut active_candidate_sources = Vec::new();
    let mut warnings = Vec::<String>::new();

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
                | "read_only"
                | "filesystem_inaccessible"
                | "sidecar_unavailable"
                | "sidecar_corrupt"
                | "sidecar_locked"
                | "disabled_budget_exceeded"
        ) {
            missing_layers.push((*name).to_string());
        }
    }

    let graph_ready = layer_ready(&layer_readiness, "graph_db");
    let spool_ready = layer_ready(&layer_readiness, "candidate_spool");
    let vector_ready = layer_ready(&layer_readiness, "vector_runtime");
    let audit_ready = layer_ready(&layer_readiness, "vector_audit");

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
        .unwrap_or("missing");
    if vector_status == "stale" {
        warnings.push("Vector runtime sidecar is stale; vector candidates omitted.".to_string());
    }
    if vector_ready
        || matches!(
            vector_status,
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

    let recommended_next_step = if vector_status == "stale" {
        "rebuild vector sidecar"
    } else if !graph_ready && spool_ready {
        "inspect candidate spans"
    } else if !graph_ready {
        "wait_for_graph_db"
    } else {
        "run final graph verification"
    };
    let candidate_only_available = spool_ready || vector_ready;
    let graph_proof_available = graph_ready;

    json!({
        "schema_version": 1,
        "available_layers": available_layers,
        "missing_layers": missing_layers,
        "layer_readiness": Value::Object(layer_readiness),
        "graph_db_status": layer_status(&layers, "graph_db"),
        "candidate_spool_status": layer_status(&layers, "candidate_spool"),
        "vector_runtime_status": layer_status(&layers, "vector_runtime"),
        "vector_audit_status": layer_status(&layers, "vector_audit"),
        "active_candidate_sources": active_candidate_sources,
        "candidate_context_available": candidate_only_available || graph_ready,
        "candidate_context_truncated": candidate_context_truncated,
        "candidate_spool_unavailable": candidate_spool_unavailable,
        "candidate_only_available": candidate_only_available,
        "graph_proof_available": graph_proof_available,
        "lifecycle_decision": if graph_ready { "read_reuse" } else if spool_ready { "partial_candidate_context" } else { "blocked" },
        "claimability": {
            "claimable": graph_ready,
            "candidate_only": candidate_only_available && !graph_ready,
            "graph_proof_available": graph_proof_available,
            "diagnostic_only": false,
        },
        "recommended_next_step": recommended_next_step,
        "risks": staged_availability_risks(candidate_only_available || vector_ready || audit_ready),
        "warnings": warnings,
        "blockers": if graph_ready { Vec::<String>::new() } else { vec!["graph_db_not_ready".to_string()] },
        "public_claim": false,
    })
}

fn layer_ready(layer_readiness: &serde_json::Map<String, Value>, layer_name: &str) -> bool {
    layer_readiness
        .get(layer_name)
        .and_then(|layer| layer.get("ready"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn layer_status(layers: &[(&str, Value)], layer_name: &str) -> Value {
    layers
        .iter()
        .find(|(name, _)| *name == layer_name)
        .and_then(|(_, layer)| layer.get("status").cloned())
        .unwrap_or_else(|| json!("unknown"))
}

fn staged_availability_risks(include_candidate_risk: bool) -> Vec<Value> {
    let mut risks = Vec::new();
    if include_candidate_risk {
        risks.push(json!({
            "risk_id": "candidate_context_not_graph_proof",
            "sentence": "Candidate context is not graph proof.",
        }));
    }
    risks.push(json!({
        "risk_id": "vector_sidecar_not_complete_path_index",
        "sentence": "Selected vector sidecar is not a complete file/path/symbol index.",
    }));
    risks.push(json!({
        "risk_id": "audit_artifact_not_runtime_source",
        "sentence": "Audit artifact exists only for diagnostics and is not used for runtime retrieval.",
    }));
    risks
}

fn staged_availability_top_level_fields(staged: &Value) -> Value {
    json!({
        "staged_availability": staged,
        "graph_db_status": staged.get("graph_db_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_status": staged.get("candidate_spool_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_query_index_status": staged.pointer("/layer_readiness/candidate_spool/query_index_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_query_index_kind": staged.pointer("/layer_readiness/candidate_spool/query_index_kind").cloned().unwrap_or(Value::Null),
        "candidate_spool_query_index_path": staged.pointer("/layer_readiness/candidate_spool/query_index_path").cloned().unwrap_or(Value::Null),
        "candidate_spool_query_index_bytes": staged.pointer("/layer_readiness/candidate_spool/query_index_bytes").cloned().unwrap_or(Value::Null),
        "candidate_spool_query_index_record_count": staged.pointer("/layer_readiness/candidate_spool/query_index_record_count").cloned().unwrap_or(Value::Null),
        "vector_runtime_status": staged.get("vector_runtime_status").cloned().unwrap_or(Value::Null),
        "vector_audit_status": staged.get("vector_audit_status").cloned().unwrap_or(Value::Null),
        "active_candidate_sources": staged.get("active_candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "candidate_context_truncated": staged.get("candidate_context_truncated").cloned().unwrap_or_else(|| json!(false)),
        "candidate_spool_unavailable": staged.get("candidate_spool_unavailable").cloned().unwrap_or_else(|| json!(false)),
        "candidate_only_available": staged.get("candidate_only_available").cloned().unwrap_or_else(|| json!(false)),
        "graph_proof_available": staged.get("graph_proof_available").cloned().unwrap_or_else(|| json!(false)),
        "lifecycle_decision": staged.get("lifecycle_decision").cloned().unwrap_or(Value::Null),
        "claimability": staged.get("claimability").cloned().unwrap_or(Value::Null),
        "warnings": staged_warning_values(staged),
        "blockers": staged.get("blockers").cloned().unwrap_or_else(|| json!([])),
    })
}

fn staged_availability_compact_top_level_fields(staged: &Value) -> Value {
    json!({
        "graph_db_status": staged.get("graph_db_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_status": staged.get("candidate_spool_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_query_index_status": staged.pointer("/layer_readiness/candidate_spool/query_index_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_query_index_kind": staged.pointer("/layer_readiness/candidate_spool/query_index_kind").cloned().unwrap_or(Value::Null),
        "vector_runtime_status": staged.get("vector_runtime_status").cloned().unwrap_or(Value::Null),
        "vector_audit_status": staged.get("vector_audit_status").cloned().unwrap_or(Value::Null),
        "active_candidate_sources": staged.get("active_candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "candidate_context_truncated": staged.get("candidate_context_truncated").cloned().unwrap_or_else(|| json!(false)),
        "candidate_spool_unavailable": staged.get("candidate_spool_unavailable").cloned().unwrap_or_else(|| json!(false)),
        "candidate_only_available": staged.get("candidate_only_available").cloned().unwrap_or_else(|| json!(false)),
        "graph_proof_available": staged.get("graph_proof_available").cloned().unwrap_or_else(|| json!(false)),
        "lifecycle_decision": staged.get("lifecycle_decision").cloned().unwrap_or(Value::Null),
        "staged_claimability": staged.get("claimability").cloned().unwrap_or(Value::Null),
        "warnings": staged_warning_values(staged),
        "blockers": staged.get("blockers").cloned().unwrap_or_else(|| json!([])),
    })
}

fn staged_warning_values(staged: &Value) -> Value {
    staged.get("warnings").cloned().unwrap_or_else(|| json!([]))
}

fn staged_availability_from_db_lifecycle_only(db_lifecycle_read: &Value) -> Value {
    let graph = graph_db_layer_status_from_lifecycle(db_lifecycle_read);
    let spool = json!({
        "layer": "candidate_spool",
        "status": "no_spool",
        "ready": false,
        "path": Value::Null,
        "candidate_only": true,
        "graph_proof": false,
    });
    let runtime = json!({
        "layer": "vector_runtime",
        "status": "missing",
        "ready": false,
        "path": Value::Null,
        "candidate_only": true,
        "graph_proof": false,
        "complete_path_symbol_index": false,
        "reason": "runtime vector sidecar missing or not requested",
    });
    let audit = json!({
        "layer": "vector_audit",
        "status": "missing",
        "ready": false,
        "path": Value::Null,
        "diagnostic_only": true,
        "runtime_dependency": false,
    });
    staged_availability_from_layers(graph, spool, runtime, audit)
}

fn staged_availability_for_packet(packet: &ContextPacket, db_lifecycle_read: &Value) -> Value {
    packet
        .metadata
        .get("staged_availability")
        .cloned()
        .unwrap_or_else(|| staged_availability_from_db_lifecycle_only(db_lifecycle_read))
}

fn context_pack_response_proof_strength(
    graph_proof: bool,
    fallback_evidence: &[Value],
    snippets: &[Value],
    candidate_set: &ContextAgentCandidateSet,
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
    if candidate_set.candidates.iter().any(|candidate| {
        candidate
            .get("candidate_sources")
            .and_then(Value::as_array)
            .is_some_and(|sources| {
                sources
                    .iter()
                    .any(|source| source.as_str() == Some("symbol"))
            })
            || candidate
                .get("evidence_role")
                .and_then(Value::as_str)
                .is_some_and(|role| role.contains("symbol"))
    }) {
        return "symbol_evidence";
    }
    if !fallback_evidence.is_empty() || !snippets.is_empty() || candidate_set.total_count > 0 {
        return "source_navigation_evidence";
    }
    "unknown"
}

fn staged_routing_packet_for_spool(
    task: &str,
    mode: &str,
    snippets: &[Value],
    staged: &Value,
) -> Value {
    json!({
        "packet_kind": "agent_routing_packet",
        "schema_version": 1,
        "task": task,
        "mode": mode,
        "available_layers": staged.get("available_layers").cloned().unwrap_or_else(|| json!([])),
        "missing_layers": staged.get("missing_layers").cloned().unwrap_or_else(|| json!([])),
        "layer_readiness": staged.get("layer_readiness").cloned().unwrap_or(Value::Null),
        "candidate_context_available": true,
        "graph_proof_available": false,
        "recommended_next_step": staged.get("recommended_next_step").cloned().unwrap_or_else(|| json!("inspect candidate spans")),
        "proof_status": "candidate_only",
        "proof_strength": "candidate_evidence",
        "graph_proof": false,
        "text_evidence": snippets.iter().filter(|snippet| snippet.get("proof_strength").and_then(Value::as_str) == Some("text_evidence")).cloned().collect::<Vec<_>>(),
        "source_navigation_evidence": snippets.iter().filter(|snippet| snippet.get("proof_strength").and_then(Value::as_str) != Some("text_evidence")).cloned().collect::<Vec<_>>(),
        "risks": staged.get("risks").cloned().unwrap_or_else(|| json!([])),
        "warnings": staged.get("warnings").cloned().unwrap_or_else(|| json!([])),
        "proof_contract": "Fast Candidate Spool routing packet is candidate-only; graph proof waits for a valid graph DB.",
    })
}

const PATCH_ASSIST_PACKET_TARGET_BYTES: usize = 12 * 1024;
const PATCH_ASSIST_PACKET_FILE_LIMIT: usize = 8;
const PATCH_ASSIST_PACKET_SYMBOL_LIMIT: usize = 8;
const PATCH_ASSIST_PACKET_EVIDENCE_LIMIT: usize = 8;
const PATCH_ASSIST_PACKET_QUERY_LIMIT: usize = 6;
const PATCH_ASSIST_PACKET_WARNING_LIMIT: usize = 8;

fn attach_context_pack_patch_assist_packet(
    value: &mut Value,
    task: &str,
    mode: &str,
    staged_availability: &Value,
    max_output_bytes: usize,
) {
    let packet = context_pack_patch_assist_packet_from_response(
        value,
        task,
        mode,
        staged_availability,
        max_output_bytes,
    );
    if let Some(object) = value.as_object_mut() {
        object.insert("patch_assist_packet".to_string(), packet);
    }
}

fn attach_unavailable_patch_assist_packet(
    value: &mut Value,
    profile: &AgentUseProfile,
    staged_availability: &Value,
    errors: &[String],
    status: &str,
) {
    let packet = unavailable_patch_assist_packet(profile, staged_availability, errors, status);
    if let Some(object) = value.as_object_mut() {
        object.insert("patch_assist_packet".to_string(), packet);
    }
}

fn context_pack_patch_assist_packet_from_response(
    response: &Value,
    task: &str,
    mode: &str,
    staged_availability: &Value,
    max_output_bytes: usize,
) -> Value {
    let routing = response.get("routing_packet").unwrap_or(&Value::Null);
    let proof_status = response
        .get("proof_status")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            routing
                .get("proof_status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        });
    let proof_strength = response
        .get("proof_strength")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            routing
                .get("proof_strength")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        });
    let graph_proof = response
        .get("graph_proof")
        .and_then(Value::as_bool)
        .or_else(|| routing.get("graph_proof").and_then(Value::as_bool))
        .unwrap_or(false);
    let candidate_evidence = patch_assist_candidate_evidence_from_response(response);
    let source_navigation_evidence = routing_take_array(
        routing,
        "source_navigation_evidence",
        PATCH_ASSIST_PACKET_EVIDENCE_LIMIT,
    );
    let text_evidence =
        routing_take_array(routing, "text_evidence", PATCH_ASSIST_PACKET_EVIDENCE_LIMIT);
    let critical_files =
        routing_take_array(routing, "critical_files", PATCH_ASSIST_PACKET_FILE_LIMIT);
    let critical_symbols = routing_take_array(
        routing,
        "critical_symbols",
        PATCH_ASSIST_PACKET_SYMBOL_LIMIT,
    );
    let graph_proof_paths = if graph_proof {
        routing_take_array(routing, "verified_paths", 3)
    } else {
        json!([])
    };
    let unknowns = routing_take_array(routing, "unknowns", PATCH_ASSIST_PACKET_EVIDENCE_LIMIT);
    let risks = routing_take_array(routing, "risks", PATCH_ASSIST_PACKET_EVIDENCE_LIMIT);
    let validation_steps =
        routing_take_array(routing, "validation_steps", PATCH_ASSIST_PACKET_QUERY_LIMIT);
    let follow_up_queries = patch_assist_follow_up_queries(routing);
    let expansion_handles = routing_take_array(
        routing,
        "expansion_handles",
        PATCH_ASSIST_PACKET_QUERY_LIMIT,
    );
    let artifact_or_db_inspection_requirements = patch_assist_artifact_or_db_requirements(routing);
    let degradation_warnings = patch_assist_degradation_warnings(
        &critical_files,
        &critical_symbols,
        &source_navigation_evidence,
        &text_evidence,
        &candidate_evidence,
        staged_availability,
    );
    let candidate_evidence_available = candidate_evidence
        .as_array()
        .is_some_and(|items| !items.is_empty());
    let degraded_warning_available = degradation_warnings
        .as_array()
        .is_some_and(|items| !items.is_empty());
    let first_use_state = patch_assist_first_use_state(
        staged_availability,
        response,
        graph_proof,
        candidate_evidence_available,
        degraded_warning_available,
    );
    let available = first_use_state != "unavailable";
    let mut packet = json!({
        "packet_kind": "patch_assist_staged_context",
        "schema_version": 1,
        "task": task,
        "mode": mode,
        "task_intent": routing.get("task_intent").cloned().unwrap_or_else(|| json!({
            "task_kind": "unknown",
            "signals": [],
        })),
        "task_roles": routing.get("task_roles").cloned().unwrap_or_else(|| json!([])),
        "first_use_state": first_use_state,
        "critical_files": critical_files,
        "critical_symbols": critical_symbols,
        "source_navigation_evidence": source_navigation_evidence,
        "text_evidence": text_evidence,
        "candidate_evidence": candidate_evidence,
        "graph_proof_paths": graph_proof_paths,
        "proof_status": proof_status,
        "proof_strength": proof_strength,
        "graph_proof": graph_proof,
        "unknowns": unknowns,
        "risks": risks,
        "validation_steps": validation_steps,
        "follow_up_queries": follow_up_queries,
        "expansion_handles": expansion_handles,
        "artifact_or_db_inspection_requirements": artifact_or_db_inspection_requirements,
        "degradation_warnings": degradation_warnings,
        "staged_availability": patch_assist_compact_staged_availability(staged_availability),
        "staged_availability_summary": {
            "graph_db_status": staged_availability.get("graph_db_status").cloned().unwrap_or_else(|| json!("unknown")),
            "candidate_spool_status": staged_availability.get("candidate_spool_status").cloned().unwrap_or_else(|| json!("unknown")),
            "vector_runtime_status": staged_availability.get("vector_runtime_status").cloned().unwrap_or_else(|| json!("unknown")),
            "candidate_context_available": staged_availability.get("candidate_context_available").cloned().unwrap_or_else(|| json!(false)),
            "candidate_only_available": staged_availability.get("candidate_only_available").cloned().unwrap_or_else(|| json!(false)),
            "graph_proof_available": staged_availability.get("graph_proof_available").cloned().unwrap_or_else(|| json!(graph_proof)),
            "recommended_next_step": staged_availability.get("recommended_next_step").cloned().unwrap_or_else(|| json!(if graph_proof { "inspect verified graph/source paths" } else { "inspect candidate spans then run graph/source verification" })),
        },
        "claimability": {
            "claimable": response.get("claimable").and_then(Value::as_bool).unwrap_or(false),
            "graph_proof": graph_proof,
            "graph_proof_only_from_graph_source_verification": true,
            "candidate_evidence_graph_proof": false,
            "text_evidence_graph_proof": false,
            "source_navigation_evidence_graph_proof": false,
            "vector_evidence_graph_proof": false,
        },
        "unavailable": !available,
        "recovery": response.get("recovery").cloned().unwrap_or(Value::Null),
        "proof_contract": "Patch-assist context may use source/text/path/symbol/candidate evidence for orientation, but graph_proof=true is allowed only from verified graph/source paths.",
    });
    enforce_patch_assist_packet_budget(&mut packet, max_output_bytes);
    packet
}

fn unavailable_patch_assist_packet(
    profile: &AgentUseProfile,
    staged_availability: &Value,
    errors: &[String],
    status: &str,
) -> Value {
    let mut packet = json!({
        "packet_kind": "patch_assist_staged_context",
        "schema_version": 1,
        "task": Value::Null,
        "mode": "agent-use",
        "task_intent": {
            "task_kind": "unknown",
            "signals": [],
            "reason": "context unavailable before task routing",
        },
        "task_roles": [],
        "first_use_state": patch_assist_first_use_state(staged_availability, &Value::Null, false, false, false),
        "critical_files": [],
        "critical_symbols": [],
        "source_navigation_evidence": [],
        "text_evidence": [],
        "candidate_evidence": [],
        "graph_proof_paths": [],
        "proof_status": "not_available_until_index",
        "proof_strength": "none",
        "graph_proof": false,
        "unknowns": [{
            "claim": "patch-assist context",
            "reason": status,
            "sentence": "No safe graph or candidate context is currently available for this repository profile."
        }],
        "risks": [{
            "risk_id": "unavailable_context_is_not_evidence",
            "forbidden_claim": "graph proof or candidate freshness",
            "sentence": "Do not infer source, candidate, or graph proof from an unavailable context packet."
        }],
        "validation_steps": [],
        "follow_up_queries": [],
        "expansion_handles": [],
        "artifact_or_db_inspection_requirements": [{
            "claim": "safe patch-assist context",
            "required": true,
            "why": "agent-use profile has no safe current graph or candidate context",
            "repair_action": profile.recovery_commands.first().cloned().unwrap_or_else(|| format!("{BIN_NAME} agent-use index --repo \"{}\" --json", path_string(&profile.repo_root))),
        }],
        "degradation_warnings": patch_assist_layer_warnings(staged_availability),
        "staged_availability": patch_assist_compact_staged_availability(staged_availability),
        "staged_availability_summary": {
            "graph_db_status": staged_availability.get("graph_db_status").cloned().unwrap_or_else(|| json!("unknown")),
            "candidate_spool_status": staged_availability.get("candidate_spool_status").cloned().unwrap_or_else(|| json!("unknown")),
            "vector_runtime_status": staged_availability.get("vector_runtime_status").cloned().unwrap_or_else(|| json!("unknown")),
            "candidate_context_available": false,
            "candidate_only_available": false,
            "graph_proof_available": false,
            "recommended_next_step": staged_availability.get("recommended_next_step").cloned().unwrap_or_else(|| json!("run agent-use index")),
        },
        "claimability": {
            "claimable": false,
            "graph_proof": false,
            "graph_proof_only_from_graph_source_verification": true,
            "candidate_evidence_graph_proof": false,
            "text_evidence_graph_proof": false,
            "source_navigation_evidence_graph_proof": false,
            "vector_evidence_graph_proof": false,
        },
        "unavailable": true,
        "unavailable_reason": status,
        "recovery": {
            "commands": profile.recovery_commands.clone(),
            "agent_use_index_command": profile.recovery_commands.first().cloned().unwrap_or_else(|| format!("{BIN_NAME} agent-use index --repo \"{}\" --json", path_string(&profile.repo_root))),
        },
        "errors": errors,
        "proof_contract": "Unavailable context is not evidence. Build or refresh the agent-use profile before patch-assist can return claim-labeled source/candidate context.",
    });
    enforce_patch_assist_packet_budget(&mut packet, DEFAULT_CONTEXT_AGENT_MAX_OUTPUT_BYTES);
    packet
}

fn patch_assist_candidate_evidence_from_response(response: &Value) -> Value {
    let candidates = response
        .get("candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let compact = candidates
        .iter()
        .take(PATCH_ASSIST_PACKET_EVIDENCE_LIMIT)
        .map(patch_assist_candidate_evidence_json)
        .collect::<Vec<_>>();
    Value::Array(compact)
}

fn patch_assist_candidate_evidence_json(candidate: &Value) -> Value {
    let file = candidate
        .get("file")
        .or_else(|| candidate.get("path"))
        .or_else(|| candidate.get("repo_relative_path"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let mut value = json!({
        "evidence_id": candidate.get("candidate_id")
            .or_else(|| candidate.get("chunk_id"))
            .or_else(|| candidate.get("id"))
            .cloned()
            .unwrap_or_else(|| json!(format!("candidate://{}", context_agent_stable_component(file)))),
        "file": file,
        "symbol": candidate.get("symbol").or_else(|| candidate.get("name")).cloned().unwrap_or(Value::Null),
        "span": candidate.get("span").or_else(|| candidate.get("source_span")).cloned().unwrap_or(Value::Null),
        "candidate_sources": candidate.get("candidate_sources").cloned().unwrap_or_else(|| {
            candidate.get("source_labels").cloned().unwrap_or_else(|| json!([]))
        }),
        "evidence_role": candidate.get("evidence_role").cloned().unwrap_or_else(|| json!("candidate_evidence")),
        "proof_status": candidate.get("proof_status").cloned().unwrap_or_else(|| json!("candidate_only")),
        "proof_strength": candidate.get("proof_strength").cloned().unwrap_or_else(|| json!("candidate_evidence")),
        "graph_proof": candidate.get("graph_proof").and_then(Value::as_bool).unwrap_or(false),
        "claimability": candidate.get("claimability").cloned().unwrap_or_else(|| json!({
            "claimable_as": [],
            "not_claimable_as": ["graph_relation_proof"]
        })),
        "reason": candidate.get("reason").or_else(|| candidate.get("match_reason")).cloned().unwrap_or_else(|| json!("candidate evidence")),
        "text_preview": candidate.get("text")
            .or_else(|| candidate.pointer("/snippet/text"))
            .and_then(Value::as_str)
            .map(|text| text.chars().take(160).collect::<String>())
            .map(Value::from)
            .unwrap_or(Value::Null),
    });
    if let Some(object) = value.as_object_mut() {
        copy_context_candidate_degradation_fields(candidate, object);
    }
    value
}

fn patch_assist_follow_up_queries(routing: &Value) -> Value {
    let queries = routing
        .get("follow_up_queries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(PATCH_ASSIST_PACKET_QUERY_LIMIT)
        .map(|query| {
            let mut object = query.as_object().cloned().unwrap_or_default();
            object.insert("kind".to_string(), json!("codegraph_query_hint"));
            object.insert("shell_ready".to_string(), json!(false));
            if !object.contains_key("risk") {
                object.insert(
                    "risk".to_string(),
                    json!("candidate_only_until_graph_source_verified"),
                );
            }
            Value::Object(object)
        })
        .collect::<Vec<_>>();
    Value::Array(queries)
}

fn patch_assist_artifact_or_db_requirements(routing: &Value) -> Value {
    let mut requirements = Vec::new();
    for key in [
        "artifact_inspection_requirements",
        "db_inspection_requirements",
    ] {
        requirements.extend(
            routing
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .take(3)
                .cloned(),
        );
    }
    requirements.truncate(6);
    Value::Array(requirements)
}

fn patch_assist_compact_staged_availability(staged_availability: &Value) -> Value {
    let layer_status = |name: &str| {
        let layer = staged_availability
            .pointer(&format!("/layer_readiness/{name}"))
            .unwrap_or(&Value::Null);
        json!({
            "status": layer.get("status").cloned().unwrap_or_else(|| json!("unknown")),
            "ready": layer.get("ready").cloned().unwrap_or_else(|| json!(false)),
            "graph_proof": layer
                .get("graph_proof")
                .or_else(|| layer.get("graph_proof_available"))
                .cloned()
                .unwrap_or_else(|| json!(false)),
            "candidate_only": layer.get("candidate_only").cloned().unwrap_or_else(|| json!(name != "graph_db")),
            "diagnostic_only": layer.get("diagnostic_only").cloned().unwrap_or_else(|| json!(false)),
            "reason": layer.get("reason").cloned().unwrap_or(Value::Null),
        })
    };
    json!({
        "graph_db_status": staged_availability.get("graph_db_status").cloned().unwrap_or_else(|| json!("unknown")),
        "candidate_spool_status": staged_availability.get("candidate_spool_status").cloned().unwrap_or_else(|| json!("unknown")),
        "vector_runtime_status": staged_availability.get("vector_runtime_status").cloned().unwrap_or_else(|| json!("unknown")),
        "vector_audit_status": staged_availability.get("vector_audit_status").cloned().unwrap_or_else(|| json!("unknown")),
        "candidate_context_available": staged_availability.get("candidate_context_available").cloned().unwrap_or_else(|| json!(false)),
        "candidate_only_available": staged_availability.get("candidate_only_available").cloned().unwrap_or_else(|| json!(false)),
        "graph_proof_available": staged_availability.get("graph_proof_available").cloned().unwrap_or_else(|| json!(false)),
        "active_candidate_sources": staged_availability.get("active_candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "available_layers": staged_availability.get("available_layers").cloned().unwrap_or_else(|| json!([])),
        "missing_layers": staged_availability.get("missing_layers").cloned().unwrap_or_else(|| json!([])),
        "recommended_next_step": staged_availability.get("recommended_next_step").cloned().unwrap_or_else(|| json!("inspect current evidence then verify graph/source proof")),
        "layer_readiness": {
            "graph_db": layer_status("graph_db"),
            "candidate_spool": layer_status("candidate_spool"),
            "vector_runtime": layer_status("vector_runtime"),
            "vector_audit": layer_status("vector_audit"),
        },
    })
}

fn patch_assist_degradation_warnings(
    critical_files: &Value,
    critical_symbols: &Value,
    source_navigation_evidence: &Value,
    text_evidence: &Value,
    candidate_evidence: &Value,
    staged_availability: &Value,
) -> Value {
    let mut warnings = Vec::new();
    for (section, values) in [
        ("critical_files", critical_files),
        ("critical_symbols", critical_symbols),
        ("source_navigation_evidence", source_navigation_evidence),
        ("text_evidence", text_evidence),
        ("candidate_evidence", candidate_evidence),
    ] {
        if let Some(array) = values.as_array() {
            for item in array {
                if let Some(warning) = patch_assist_degradation_warning(section, item) {
                    warnings.push(warning);
                }
                if warnings.len() >= PATCH_ASSIST_PACKET_WARNING_LIMIT {
                    return Value::Array(warnings);
                }
            }
        }
    }
    warnings.extend(patch_assist_layer_warnings(staged_availability));
    warnings.truncate(PATCH_ASSIST_PACKET_WARNING_LIMIT);
    Value::Array(warnings)
}

fn patch_assist_degradation_warning(section: &str, item: &Value) -> Option<Value> {
    let degraded = item
        .get("graph_output_degraded")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || item
            .get("degradation_labels")
            .and_then(Value::as_array)
            .is_some_and(|labels| !labels.is_empty())
        || item.get("graph_extraction_skip_reason").is_some();
    if !degraded {
        return None;
    }
    Some(json!({
        "section": section,
        "file": item.get("file").or_else(|| item.get("path")).cloned().unwrap_or(Value::Null),
        "labels": item.get("degradation_labels").cloned().unwrap_or_else(|| json!([])),
        "reason": item.get("graph_extraction_skip_reason").cloned().unwrap_or_else(|| json!("degraded_graph_output")),
        "claimability": item.get("claimability").cloned().unwrap_or_else(|| json!({
            "claimable_as": [],
            "not_claimable_as": ["complete_graph_for_degraded_file", "graph_relation_proof"]
        })),
        "graph_proof": false,
    }))
}

fn patch_assist_layer_warnings(staged_availability: &Value) -> Vec<Value> {
    let mut warnings = Vec::new();
    if let Some(layer_readiness) = staged_availability
        .get("layer_readiness")
        .and_then(Value::as_object)
    {
        for (layer_name, layer) in layer_readiness {
            let status = layer
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if agent_use_candidate_layer_status_is_stale(status) {
                warnings.push(json!({
                    "section": "staged_availability",
                    "layer": layer_name,
                    "status": status,
                    "reason": layer.get("reason").cloned().unwrap_or(Value::Null),
                    "graph_proof": false,
                    "candidate_evidence_used": false,
                }));
            }
        }
    }
    warnings
}

fn patch_assist_first_use_state(
    staged_availability: &Value,
    response: &Value,
    graph_proof: bool,
    candidate_evidence_available: bool,
    degraded_warning_available: bool,
) -> &'static str {
    let graph_ready = graph_proof
        || staged_availability
            .get("graph_proof_available")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || staged_availability
            .get("graph_db_status")
            .and_then(Value::as_str)
            == Some("ready");
    if graph_ready {
        return "graph_ready";
    }
    let candidate_context_available = candidate_evidence_available
        || response
            .get("candidate_only")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || staged_availability
            .get("candidate_context_available")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || staged_availability
            .get("candidate_only_available")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    if candidate_context_available && !patch_assist_has_stale_candidate_layers(staged_availability)
    {
        return "candidate_ready_no_graph";
    }
    if degraded_warning_available || patch_assist_has_stale_candidate_layers(staged_availability) {
        return "stale_or_degraded";
    }
    "unavailable"
}

fn patch_assist_has_stale_candidate_layers(staged_availability: &Value) -> bool {
    ["candidate_spool", "vector_runtime", "vector_audit"]
        .iter()
        .any(|layer_name| {
            let pointer = format!("/layer_readiness/{layer_name}/status");
            staged_availability
                .pointer(&pointer)
                .and_then(Value::as_str)
                .is_some_and(agent_use_candidate_layer_status_is_stale)
        })
}

fn enforce_patch_assist_packet_budget(packet: &mut Value, max_output_bytes: usize) {
    let packet_budget = max_output_bytes.min(PATCH_ASSIST_PACKET_TARGET_BYTES);
    let mut omitted = 0usize;
    for _ in 0..64 {
        if serialized_json_len(packet) <= packet_budget {
            break;
        }
        if pop_patch_assist_array_item(packet, "candidate_evidence")
            || pop_patch_assist_array_item(packet, "source_navigation_evidence")
            || pop_patch_assist_array_item(packet, "text_evidence")
            || pop_patch_assist_array_item(packet, "follow_up_queries")
            || pop_patch_assist_array_item(packet, "expansion_handles")
            || pop_patch_assist_array_item(packet, "critical_symbols")
            || pop_patch_assist_array_item(packet, "critical_files")
            || pop_patch_assist_array_item(packet, "degradation_warnings")
        {
            omitted += 1;
            continue;
        }
        if let Some(object) = packet.as_object_mut() {
            object.remove("staged_availability");
        }
        break;
    }
    let serialized_bytes = serialized_json_len(packet);
    if let Some(object) = packet.as_object_mut() {
        object.insert(
            "packet_budget_status".to_string(),
            json!({
                "packet_budget_enforced": true,
                "packet_budget_bytes": packet_budget,
                "serialized_bytes": serialized_bytes,
                "status": if serialized_bytes <= packet_budget { "within_budget" } else { "bounded_with_omissions" },
                "omitted_by_packet_budget": omitted,
            }),
        );
    }
}

fn pop_patch_assist_array_item(packet: &mut Value, key: &str) -> bool {
    packet
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .is_some_and(|array| array.pop().is_some())
}

fn compact_context_agent_patch_assist_packet(response: &mut Value) -> bool {
    let Some(packet_value) = response.get_mut("patch_assist_packet") else {
        return false;
    };
    let already_compacted = packet_value
        .get("packet_budget_status")
        .and_then(|status| status.get("agent_json_compacted"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if already_compacted {
        return false;
    }
    let staged = packet_value
        .get("staged_availability")
        .unwrap_or(&Value::Null);
    let compact_staged = json!({
        "graph_db_status": staged.get("graph_db_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_status": staged.get("candidate_spool_status").cloned().unwrap_or(Value::Null),
        "vector_runtime_status": staged.get("vector_runtime_status").cloned().unwrap_or(Value::Null),
        "vector_audit_status": staged.get("vector_audit_status").cloned().unwrap_or(Value::Null),
        "graph_proof_available": staged.get("graph_proof_available").cloned().unwrap_or(Value::Null),
        "candidate_only_available": staged.get("candidate_only_available").cloned().unwrap_or(Value::Null),
        "active_candidate_sources": staged.get("active_candidate_sources").cloned().unwrap_or_else(|| json!([])),
    });
    let compact_claimability = json!({
        "claimable": packet_value
            .pointer("/claimability/claimable")
            .and_then(Value::as_bool)
            .or_else(|| packet_value.get("claimable").and_then(Value::as_bool))
            .unwrap_or(false),
        "graph_proof": packet_value.get("graph_proof").cloned().unwrap_or_else(|| json!(false)),
        "graph_proof_only_from_graph_source_verification": true,
        "candidate_evidence_graph_proof": false,
        "text_evidence_graph_proof": false,
        "vector_evidence_graph_proof": false,
    });
    let compact = json!({
        "packet_kind": packet_value.get("packet_kind").cloned().unwrap_or_else(|| json!("patch_assist_staged_context")),
        "schema_version": packet_value.get("schema_version").cloned().unwrap_or_else(|| json!(1)),
        "first_use_state": packet_value.get("first_use_state").cloned().unwrap_or_else(|| json!("unavailable")),
        "proof_status": packet_value.get("proof_status").cloned().unwrap_or_else(|| json!("unknown")),
        "proof_strength": packet_value.get("proof_strength").cloned().unwrap_or_else(|| json!("unknown")),
        "graph_proof": packet_value.get("graph_proof").cloned().unwrap_or_else(|| json!(false)),
        "claimability": compact_claimability,
        "staged_availability": compact_staged,
        "candidate_evidence": routing_take_array(packet_value, "candidate_evidence", 1),
        "source_navigation_evidence": routing_take_array(packet_value, "source_navigation_evidence", 1),
        "degradation_warnings": routing_take_array(packet_value, "degradation_warnings", 2),
        "unavailable": packet_value.get("unavailable").cloned().unwrap_or_else(|| json!(false)),
        "recovery_command": packet_value.get("recovery_command").cloned().unwrap_or(Value::Null),
        "degradation_warning_count": packet_value.get("degradation_warnings").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        "candidate_evidence_count": packet_value.get("candidate_evidence").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        "source_navigation_evidence_count": packet_value.get("source_navigation_evidence").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        "packet_budget_status": {
            "packet_budget_enforced": true,
            "status": "bounded_with_omissions",
            "patch_assist_packet_compacted": true,
            "agent_json_compacted": true,
            "preserved_core_state": true,
        },
    });
    *packet_value = compact;
    true
}

#[allow(dead_code)]
fn candidate_spool_sort_key(value: &Value) -> String {
    format!(
        "{}:{}:{}",
        value.get("path").and_then(Value::as_str).unwrap_or(""),
        value.get("line").and_then(Value::as_u64).unwrap_or(0),
        value.get("chunk_id").and_then(Value::as_str).unwrap_or("")
    )
}

fn truncate_candidate_spool_text(text: &str) -> String {
    if text.len() <= RETRIEVAL_CANDIDATE_SNIPPET_MAX_BYTES {
        return text.to_string();
    }
    text.chars()
        .take(RETRIEVAL_CANDIDATE_SNIPPET_MAX_BYTES)
        .collect()
}

fn query_response_uses_compact_lifecycle(value: &Value) -> bool {
    value
        .get("schema_name")
        .and_then(Value::as_str)
        .is_some_and(|name| name.ends_with("_agent_json"))
        || matches!(
            value.get("output_mode").and_then(Value::as_str),
            Some("agent_json" | "concise")
        )
}

fn compact_lifecycle_summary(lifecycle: &Value) -> Value {
    let claimable = lifecycle
        .get("claimable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let diagnostic_only = lifecycle
        .get("diagnostic_only")
        .and_then(Value::as_bool)
        .unwrap_or(!claimable);
    let mut object = serde_json::Map::new();
    object.insert("claimable".to_string(), json!(claimable));
    object.insert("diagnostic_only".to_string(), json!(diagnostic_only));
    for key in [
        "decision",
        "db_problem_kind",
        "passport_status",
        "schema_status",
        "scope_status",
        "repo_root_status",
        "sidecar_status",
        "path_access_status",
        "allow_stale_read",
        "allow_foreign_db",
    ] {
        if let Some(value) = lifecycle.get(key).filter(|value| !value.is_null()) {
            object.insert(key.to_string(), value.clone());
        }
    }
    if diagnostic_only || !claimable {
        for key in [
            "blockers",
            "warnings",
            "safety_labels",
            "exact_db_path_checked",
            "repo_root_expected",
        ] {
            if let Some(value) = lifecycle.get(key).filter(|value| !value.is_null()) {
                object.insert(key.to_string(), value.clone());
            }
        }
    }
    Value::Object(object)
}

fn unknown_lifecycle_summary() -> Value {
    json!({
        "claimable": false,
        "diagnostic_only": true,
        "decision": "unknown",
    })
}

fn lifecycle_summary_or_unknown(lifecycle: Option<&Value>) -> Value {
    lifecycle.cloned().unwrap_or_else(unknown_lifecycle_summary)
}

fn add_query_resolution_fields(value: &mut Value, repo_root: &Path, db_path: &Path) {
    let lifecycle_status = query_lifecycle_status(value);
    let Some(object) = value.as_object_mut() else {
        return;
    };
    object
        .entry("resolved_repo".to_string())
        .or_insert_with(|| json!(path_string(repo_root)));
    object
        .entry("resolved_db".to_string())
        .or_insert_with(|| json!(path_string(db_path)));
    object
        .entry("repo_source".to_string())
        .or_insert_with(|| json!(repo_source_label()));
    object
        .entry("db_source".to_string())
        .or_insert_with(|| json!(db_source_label()));
    object
        .entry("lifecycle_status".to_string())
        .or_insert(lifecycle_status);
}

fn query_lifecycle_status(value: &Value) -> Value {
    for key in ["lifecycle", "db_lifecycle_read"] {
        if let Some(decision) = value
            .get(key)
            .and_then(|lifecycle| lifecycle.get("decision"))
            .filter(|decision| !decision.is_null())
        {
            return decision.clone();
        }
    }
    json!("unknown")
}

fn agent_timings_json(started: Instant) -> Value {
    json!({
        "wall_ms": started.elapsed().as_secs_f64() * 1000.0,
    })
}

fn agent_timings_from_wall_ms(wall_ms: f64) -> Value {
    json!({
        "wall_ms": wall_ms,
    })
}

fn truncate_for_agent<T>(mut values: Vec<T>, limit: usize) -> (Vec<T>, QueryTruncation) {
    let fetched = values.len();
    let omitted_count = usize::from(fetched > limit);
    if values.len() > limit {
        values.truncate(limit);
    }
    let returned_count = values.len();
    (
        values,
        QueryTruncation {
            returned_count,
            limit,
            omitted_count,
            omitted_count_is_lower_bound: omitted_count > 0,
            total_available_unknown: true,
        },
    )
}

fn query_truncation_json(truncation: &QueryTruncation) -> Value {
    json!({
        "returned_count": truncation.returned_count,
        "limit": truncation.limit,
        "limit_applied": true,
        "omitted_count": truncation.omitted_count,
        "omitted_count_is_lower_bound": truncation.omitted_count_is_lower_bound,
        "total_available_unknown": truncation.total_available_unknown,
    })
}

fn bounded_message(code: &str, message: &str, severity: &str) -> Value {
    json!({
        "code": code,
        "message": message,
        "severity": severity,
    })
}

fn canonical_agent_query_response(
    schema_name: &str,
    command: &str,
    repo_root: &Path,
    db_path: &Path,
    status: &str,
    lifecycle: Option<&Value>,
    truncation: QueryTruncation,
    query: Value,
    results: Vec<Value>,
    warnings: Vec<Value>,
    errors: Vec<Value>,
    output_mode: QueryOutputMode,
    timings: Value,
) -> Value {
    let lifecycle = lifecycle_summary_or_unknown(lifecycle);
    let claimable = lifecycle
        .get("claimable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let diagnostic_only = lifecycle
        .get("diagnostic_only")
        .and_then(Value::as_bool)
        .unwrap_or(!claimable);
    json!({
        "schema_name": schema_name,
        "schema_version": AGENT_JSON_SCHEMA_VERSION,
        "status": status,
        "command": command,
        "repo": path_string(repo_root),
        "db": path_string(db_path),
        "resolved_repo": path_string(repo_root),
        "resolved_db": path_string(db_path),
        "repo_source": repo_source_label(),
        "db_source": db_source_label(),
        "output_mode": output_mode.as_str(),
        "lifecycle": lifecycle.clone(),
        "lifecycle_status": lifecycle
            .get("decision")
            .cloned()
            .unwrap_or_else(|| json!("unknown")),
        "claimable": claimable,
        "diagnostic_only": diagnostic_only,
        "truncation": query_truncation_json(&truncation),
        "result_count": truncation.returned_count,
        "limit": truncation.limit,
        "omitted_count": truncation.omitted_count,
        "timings": timings,
        "warnings": warnings.into_iter().take(8).collect::<Vec<_>>(),
        "errors": errors.into_iter().take(8).collect::<Vec<_>>(),
        "query": query,
        "results": results,
    })
}

fn agent_status_from_rich(status: &str) -> &'static str {
    match status {
        "ok" => "ok",
        "error" => "error",
        _ => "warning",
    }
}

fn remove_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let before = args.len();
    args.retain(|arg| arg != flag);
    args.len() != before
}

fn remove_path_option(args: &mut Vec<String>, flags: &[&str]) -> Result<Option<PathBuf>, String> {
    let mut retained = Vec::new();
    let mut found = None;
    let mut index = 0usize;
    while index < args.len() {
        let arg = &args[index];
        if flags.iter().any(|flag| arg == flag) {
            index += 1;
            let Some(value) = args.get(index) else {
                return Err(format!("{} requires a path", flags[0]));
            };
            found = Some(PathBuf::from(value));
        } else if let Some((flag, value)) = arg.split_once('=') {
            if flags.contains(&flag) {
                found = Some(PathBuf::from(value));
            } else {
                retained.push(arg.clone());
            }
        } else {
            retained.push(arg.clone());
        }
        index += 1;
    }
    *args = retained;
    Ok(found)
}

fn parse_read_scope_options(args: &mut Vec<String>) -> Result<Option<IndexScopeOptions>, String> {
    let mut options = IndexScopeOptions::default();
    let mut explicit = false;
    let mut retained = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--include-ignored" => {
                options.include_ignored = true;
                explicit = true;
            }
            "--no-default-excludes" => {
                options.no_default_excludes = true;
                explicit = true;
            }
            "--include" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--include requires a pattern".to_string());
                };
                options.include_patterns.push(raw.to_string());
                explicit = true;
            }
            "--exclude" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--exclude requires a pattern".to_string());
                };
                options.exclude_patterns.push(raw.to_string());
                explicit = true;
            }
            "--respect-gitignore" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--respect-gitignore requires true or false".to_string());
                };
                options.respect_gitignore = parse_index_bool(raw, "--respect-gitignore")?;
                explicit = true;
            }
            value => retained.push(value.to_string()),
        }
        index += 1;
    }
    *args = retained;
    Ok(explicit.then_some(options))
}

fn run_context_pack_command(args: &[String]) -> Result<Value, String> {
    let options = parse_context_pack_args(args)?;
    let profile_enabled = options.profile;
    let repo_root = current_repo_root()?;
    let total_start = Instant::now();
    let mut profile_spans = Vec::new();
    let db_path = resolved_db_path_for_repo(&repo_root);
    let db_lifecycle_read = match read_db_lifecycle_guard(
        &repo_root,
        &db_path,
        options.allow_stale_read,
        options.allow_foreign_db,
        options.explicit_scope_policy.clone(),
    ) {
        Ok(value) => value,
        Err(error) if options.enable_candidate_spool => {
            let spool_path = options
                .candidate_spool_path
                .clone()
                .unwrap_or_else(|| default_candidate_spool_path(&repo_root));
            return run_candidate_spool_context_pack_command(
                &repo_root,
                &spool_path,
                &options,
                options.allow_stale_candidate_spool,
                total_start,
                Some(error),
            );
        }
        Err(error) => return Err(error),
    };
    let open_start = Instant::now();
    let connection = open_context_pack_connection(&db_path)?;
    profile_spans.push(profile_span_json(
        "open_store",
        open_start.elapsed(),
        1,
        0,
        json!({ "db_path": path_string(&db_path) }),
    ));

    let budgets = ContextPackBudgets::for_options(&options);
    let seed_start = Instant::now();
    let raw_seed_values = context_pack_seed_values(&options, budgets.max_seed_entities);
    let seed_entities =
        resolve_context_seed_entities(&connection, &raw_seed_values, budgets.max_seed_entities)?;
    let seed_ids = context_seed_ids(&raw_seed_values, &seed_entities, budgets.max_seed_entities);
    profile_spans.push(profile_span_json(
        "seed_resolution",
        seed_start.elapsed(),
        1,
        seed_ids.len() as u64,
        json!({
            "raw_seed_count": raw_seed_values.len(),
            "resolved_entity_count": seed_entities.len(),
            "max_seed_entities": budgets.max_seed_entities,
        }),
    ));

    let stored_start = Instant::now();
    let stored_load =
        load_stored_context_path_evidence(&connection, &seed_ids, &options.mode, budgets)?;
    let mut stored_paths = stored_load.paths;
    let mut path_evidence_telemetry = stored_load.telemetry;
    profile_spans.push(profile_span_json(
        "sql_query_execution",
        stored_start.elapsed(),
        1,
        stored_paths.len() as u64,
        json!({
            "query": "load_stored_path_evidence_for_context_pack",
            "rows_returned": stored_paths.len(),
            "rows_scanned": Value::Null
        }),
    ));
    let stored_path_count = stored_paths.len();

    let fallback_start = Instant::now();
    let fallback_edges = if stored_paths.is_empty() {
        load_bounded_context_edges(&connection, &seed_ids, &options.mode, budgets)?
    } else {
        Vec::new()
    };
    profile_spans.push(profile_span_json(
        "sql_query_execution",
        fallback_start.elapsed(),
        1,
        fallback_edges.len() as u64,
        json!({
            "query": "load_bounded_edges_for_context_pack_fallback",
            "rows_returned": fallback_edges.len(),
            "rows_scanned": Value::Null,
            "policy": "used only when stored PathEvidence has no matching candidates"
        }),
    ));
    let test_impact_fallback_edges = if context_pack_mode_needs_test_impact_fallback(&options.mode)
    {
        if fallback_edges.is_empty() {
            load_bounded_context_edges(&connection, &seed_ids, &options.mode, budgets)?
        } else {
            fallback_edges.clone()
        }
    } else {
        Vec::new()
    };

    let source_load_start = Instant::now();
    let mut candidate_spans = stored_paths
        .iter()
        .flat_map(|path| path.source_spans.iter().cloned())
        .collect::<Vec<_>>();
    candidate_spans.extend(
        fallback_edges
            .iter()
            .map(|edge| edge.source_span.clone())
            .collect::<Vec<_>>(),
    );
    candidate_spans.sort_by(|left, right| {
        left.repo_relative_path
            .cmp(&right.repo_relative_path)
            .then_with(|| left.start_line.cmp(&right.start_line))
            .then_with(|| left.end_line.cmp(&right.end_line))
    });
    candidate_spans.dedup();
    let context_max_source_files = if agent_use_bounded_read_path_enabled() {
        AGENT_USE_CONTEXT_MAX_SOURCE_FILES
    } else {
        usize::MAX
    };
    let context_max_source_bytes = if agent_use_bounded_read_path_enabled() {
        budgets
            .max_hydration_bytes
            .min(AGENT_USE_CONTEXT_MAX_SOURCE_BYTES)
    } else {
        usize::MAX
    };
    let (sources, _, source_bytes, source_files_loaded, source_budget_hit) =
        load_context_sources_and_snippets_capped(
            &repo_root,
            &candidate_spans,
            0,
            context_max_source_files,
            context_max_source_bytes,
        )?;
    profile_spans.push(profile_span_json(
        "source_loading",
        source_load_start.elapsed(),
        source_files_loaded as u64,
        source_bytes as u64,
        json!({
            "source_files_loaded": source_files_loaded,
            "source_bytes_loaded": source_bytes,
            "candidate_spans": candidate_spans.len(),
            "source_budget_hit": source_budget_hit,
            "snippets_returned": 0,
            "policy": "load source files referenced by candidate proof/source spans for fallback graph verification; snippets are loaded after evidence-role filtering"
        }),
    ));
    if let Some(object) = path_evidence_telemetry.as_object_mut() {
        object.insert(
            "source_span_load_time_ms".to_string(),
            json!(source_load_start.elapsed().as_secs_f64() * 1000.0),
        );
        object.insert(
            "source_span_count".to_string(),
            json!(candidate_spans.len()),
        );
        object.insert("source_span_bytes_read".to_string(), json!(source_bytes));
        object.insert(
            "source_span_budget_hit".to_string(),
            json!(source_budget_hit),
        );
    }

    let context_start = Instant::now();
    let fallback_packet = if !fallback_edges.is_empty() {
        let engine_start = Instant::now();
        let engine = ExactGraphQueryEngine::new(fallback_edges.clone());
        profile_spans.push(profile_span_json(
            "context_engine_build",
            engine_start.elapsed(),
            1,
            fallback_edges.len() as u64,
            json!({ "policy": "bounded fallback edge graph only" }),
        ));
        Some(engine.context_pack(
            ContextPackRequest::new(
                options.task.clone(),
                options.mode.clone(),
                options.token_budget,
                seed_ids.clone(),
            ),
            &sources,
        ))
    } else {
        profile_spans.push(profile_span_json(
            "context_engine_build",
            Duration::ZERO,
            0,
            0,
            json!({ "policy": "skipped because stored PathEvidence was used" }),
        ));
        None
    };
    let fallback_traversal_telemetry = fallback_packet
        .as_ref()
        .and_then(|packet| packet.metadata.get("traversal_telemetry").cloned());
    if let Some(packet) = &fallback_packet {
        stored_paths.extend(packet.verified_paths.clone());
    }
    stored_paths = filter_and_sort_context_path_evidence(stored_paths, &options.mode, budgets);
    let fallback_evidence = build_test_impact_fallback_evidence(
        &connection,
        &options.mode,
        &seed_entities,
        &test_impact_fallback_edges,
        &stored_paths,
        budgets,
    )?;
    let text_fallback_start = Instant::now();
    let text_fallback_evidence = if stored_paths.is_empty() {
        build_context_pack_text_evidence_fallback(&connection, &options, &raw_seed_values, budgets)?
    } else {
        Vec::new()
    };
    let proof_span_keys = stored_paths
        .iter()
        .flat_map(|path| path.source_spans.iter())
        .map(context_span_key)
        .collect::<BTreeSet<_>>();
    let plan_atom_source_fallback = if stored_paths.is_empty() {
        build_context_pack_plan_atom_source_navigation_fallback(
            &connection,
            &options,
            &proof_span_keys,
            budgets,
        )?
    } else {
        Vec::new()
    };
    let mut fallback_evidence = fallback_evidence;
    fallback_evidence.extend(text_fallback_evidence);
    fallback_evidence.extend(plan_atom_source_fallback);
    profile_spans.push(profile_span_json(
        "sql_query_execution",
        text_fallback_start.elapsed(),
        1,
        fallback_evidence.len() as u64,
        json!({
            "query": "load_context_pack_text_evidence_fallback",
            "rows_returned": fallback_evidence.len(),
            "rows_scanned": Value::Null,
            "policy": "used only when no graph proof path survives filtering; rows are non-proof text evidence"
        }),
    ));
    let snippet_load_start = Instant::now();
    let requested_spans = context_source_spans_for_paths(&stored_paths);
    let (_, mut snippets, snippet_source_bytes, snippet_source_files_loaded, snippet_budget_hit) =
        load_context_sources_and_snippets_capped(
            &repo_root,
            &requested_spans,
            budgets.max_snippets,
            context_max_source_files,
            context_max_source_bytes,
        )?;
    let fallback_snippet_budget = budgets.max_snippets.saturating_sub(snippets.len());
    let fallback_snippets =
        load_context_fallback_snippets(&repo_root, &fallback_evidence, fallback_snippet_budget)?;
    snippets.extend(fallback_snippets);
    let requested_span_count = requested_spans.len() + fallback_evidence.len();
    profile_spans.push(profile_span_json(
        "snippet_loading",
        snippet_load_start.elapsed(),
        snippet_source_files_loaded as u64,
        snippet_source_bytes as u64,
        json!({
            "source_files_loaded": snippet_source_files_loaded,
            "source_bytes_loaded": snippet_source_bytes,
            "requested_spans": requested_spans.len(),
            "snippets_returned": snippets.len(),
            "fallback_evidence_count": fallback_evidence.len(),
            "source_budget_hit": snippet_budget_hit,
            "policy": "load snippets only for evidence-role-filtered proof/source spans"
        }),
    ));
    if let Some(object) = path_evidence_telemetry.as_object_mut() {
        object.insert(
            "snippet_load_time_ms".to_string(),
            json!(snippet_load_start.elapsed().as_secs_f64() * 1000.0),
        );
        object.insert(
            "snippet_source_bytes_read".to_string(),
            json!(snippet_source_bytes),
        );
        object.insert(
            "snippet_source_files_loaded".to_string(),
            json!(snippet_source_files_loaded),
        );
        object.insert("snippets_returned".to_string(), json!(snippets.len()));
        object.insert(
            "snippet_source_budget_hit".to_string(),
            json!(snippet_budget_hit),
        );
        object.insert(
            "source_snippet_omitted_count".to_string(),
            json!(requested_span_count.saturating_sub(snippets.len())),
        );
    }
    let fallback_evidence_count = fallback_evidence.len();
    let mut packet = build_context_packet_from_stored_evidence(
        &options,
        &raw_seed_values,
        &seed_ids,
        &seed_entities,
        stored_paths,
        snippets,
        fallback_evidence,
        fallback_packet,
        budgets,
        stored_path_count,
        requested_span_count,
    );
    packet.metadata.insert(
        "path_evidence_telemetry".to_string(),
        path_evidence_telemetry,
    );
    packet.metadata.insert(
        "traversal_telemetry".to_string(),
        context_pack_traversal_telemetry_json(
            &options,
            &raw_seed_values,
            &seed_ids,
            budgets,
            stored_path_count,
            packet.verified_paths.len(),
            fallback_edges.len(),
            fallback_traversal_telemetry,
            fallback_evidence_count,
        ),
    );
    let mut staged_vector_branch = None;
    if options.enable_vector_candidates {
        let vector_start = Instant::now();
        let vector_branch = load_context_pack_vector_branch(&repo_root, &db_path, &options);
        let vector_trace =
            context_pack_vector_diagnostic_trace(&options, &fallback_edges, &vector_branch)?;
        profile_spans.push(profile_span_json(
            "vector_candidate_branch",
            vector_start.elapsed(),
            1,
            vector_branch.candidates.len() as u64,
            json!({
                "enabled": true,
                "vector_index_path": path_string(&vector_branch.index_path),
                "vector_index_status": vector_branch.status.as_str(),
                "candidate_count": vector_branch.candidates.len(),
                "warning": vector_branch.warning.clone(),
                "policy": "explicit opt-in candidate recall only; candidates are not graph proof"
            }),
        ));
        packet
            .metadata
            .insert("vector_candidate_trace".to_string(), vector_trace);
        let lifecycle_claimable = db_lifecycle_read
            .get("claimable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let vector_candidates = vector_branch
            .candidates
            .iter()
            .map(|candidate| {
                context_pack_vector_retrieval_candidate_json(candidate, lifecycle_claimable)
            })
            .collect::<Vec<_>>();
        packet.metadata.insert(
            "vector_semantic_candidates".to_string(),
            json!(vector_candidates),
        );
        staged_vector_branch = Some(vector_branch);
    }
    if options.enable_nuance_rescue_candidates {
        let nuance_start = Instant::now();
        let lifecycle_claimable = db_lifecycle_read
            .get("claimable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let branch = load_context_pack_nuance_rescue_branch(
            &connection,
            &repo_root,
            &options,
            &raw_seed_values,
            lifecycle_claimable,
            budgets,
        )
        .unwrap_or_else(|error| ContextPackNuanceRescueBranch {
            candidates: Vec::new(),
            trace: json!({
                "schema_version": 1,
                "diagnostic_only": true,
                "rescue_enabled": true,
                "rescue_status": "error",
                "rescue_candidate_count": 0,
                "candidate_cap": CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT,
                "ignored_generic_tokens": [],
                "rescue_reasons": [],
                "error": error,
                "proof_contract": "nuance rescue candidates are candidate recall only; branch failure does not change graph proof"
            }),
        });
        profile_spans.push(profile_span_json(
            "nuance_rescue_candidate_branch",
            nuance_start.elapsed(),
            1,
            branch.candidates.len() as u64,
            json!({
                "enabled": true,
                "candidate_count": branch.candidates.len(),
                "candidate_cap": CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT,
                "policy": "explicit opt-in rare-token/identifier/path rescue only; candidates are not graph proof"
            }),
        ));
        packet
            .metadata
            .insert("nuance_rescue_trace".to_string(), branch.trace);
        packet.metadata.insert(
            "nuance_rescue_candidates".to_string(),
            json!(branch.candidates),
        );
    }
    let staged_availability = staged_availability_for_context_pack(
        &repo_root,
        &db_path,
        &db_lifecycle_read,
        &options,
        staged_vector_branch.as_ref(),
    );
    packet
        .metadata
        .insert("staged_availability".to_string(), staged_availability);
    profile_spans.push(profile_span_json(
        "context_pack_graph_and_packet",
        context_start.elapsed(),
        1,
        packet.verified_paths.len() as u64,
        json!({
            "candidate_seed_count": packet.metadata.get("exact_seed_count").cloned().unwrap_or(Value::Null),
            "candidate_paths_before_dedup": packet.metadata.get("candidate_path_count_before_dedup").cloned().unwrap_or(Value::Null),
            "candidate_paths_after_dedup": packet.metadata.get("candidate_path_count_after_dedup").cloned().unwrap_or(Value::Null),
            "candidate_paths_after_filter": packet.metadata.get("candidate_path_count_after_filter").cloned().unwrap_or(Value::Null),
            "candidate_paths_after_truncate": packet.metadata.get("candidate_path_count_after_truncate").cloned().unwrap_or(Value::Null),
            "snippets_returned": packet.snippets.len(),
            "verified_paths_returned": packet.verified_paths.len(),
            "stored_path_evidence_candidates": stored_path_count,
        }),
    ));

    let serialization_start = Instant::now();
    let serialized_packet_bytes = serde_json::to_vec(&packet)
        .map_err(|error| format!("failed to serialize context packet for profile: {error}"))?
        .len();
    profile_spans.push(profile_span_json(
        "json_serialization",
        serialization_start.elapsed(),
        1,
        serialized_packet_bytes as u64,
        json!({ "packet_json_bytes": serialized_packet_bytes }),
    ));

    let profile = if profile_enabled {
        Some(json!({
            "total_wall_ms": total_start.elapsed().as_secs_f64() * 1000.0,
            "spans": profile_spans,
            "explain_query_plan": context_pack_explain_query_plans(&db_path).unwrap_or_else(|error| {
                vec![json!({
                    "name": "context_pack_explain_query_plan",
                    "status": "error",
                    "error": error,
                })]
            }),
            "sql_timings": {
                "stored_path_evidence_rows_returned": stored_path_count,
                "fallback_edge_rows_returned": fallback_edges.len(),
                "file_load_rows_returned": source_files_loaded,
            },
            "rows_scanned_available": false,
            "notes": [
                "SQLite row-scan counts are not exposed by rusqlite; EXPLAIN QUERY PLAN and rows_returned are emitted instead.",
                "context_pack uses stored PathEvidence first, falls back only to bounded seed-adjacent edge loads, and loads snippets only for returned proof spans."
            ],
        }))
    } else {
        None
    };

    if options.output_mode.is_compact() {
        return Ok(context_pack_agent_json_response(
            &options,
            &packet,
            &db_lifecycle_read,
            budgets,
            &repo_root,
            &db_path,
            agent_timings_json(total_start),
        ));
    }

    Ok(json!({
        "status": "ok",
        "task": options.task,
        "mode": options.mode,
        "packet": packet,
        "profile": profile,
        "db_lifecycle_read": db_lifecycle_read,
        "proof": "Context packet is built from local graph/source evidence.",
    }))
}

fn run_candidate_spool_context_pack_command(
    repo_root: &Path,
    spool_path: &Path,
    options: &ContextPackOptions,
    allow_stale: bool,
    total_start: Instant,
    db_error: Option<String>,
) -> Result<Value, String> {
    let mut query_results = Vec::new();
    let mut candidates = BTreeMap::<String, Value>::new();
    for subcommand in ["symbols", "files", "text"] {
        let query_result = query_candidate_spool_index_for_repo(
            repo_root,
            spool_path,
            subcommand,
            &options.task,
            CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT,
            allow_stale,
        )
        .map_err(|error| error.to_string())?;
        for candidate in candidate_spool_index_query_hits(&query_result, subcommand) {
            let key = candidate
                .get("chunk_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            candidates.entry(key).or_insert(candidate);
        }
        query_results.push(query_result);
    }
    let Some(first_query_result) = query_results.first() else {
        return Err("candidate_spool_query_index_unavailable: no indexed query result".to_string());
    };
    let limit = options
        .limit_snippets
        .unwrap_or(DEFAULT_CONTEXT_AGENT_SNIPPET_LIMIT)
        .min(CONTEXT_AGENT_RETRIEVAL_CANDIDATE_LIMIT);
    let snippets = candidates.values().take(limit).cloned().collect::<Vec<_>>();
    let lifecycle = candidate_spool_index_lifecycle_json(&first_query_result.load);
    let staged_availability = staged_availability_for_spool_index_only(
        repo_root,
        spool_path,
        &first_query_result.load,
        db_error.as_deref(),
    );
    let staged_fields = staged_availability_top_level_fields(&staged_availability);
    let trace = json!({
        "schema_version": 1,
        "candidate_spool_enabled": true,
        "candidate_spool_status": lifecycle.get("candidate_spool_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_path": path_string(spool_path),
        "query_index_status": lifecycle.get("query_index_status").cloned().unwrap_or(Value::Null),
        "query_index_kind": lifecycle.get("query_index_kind").cloned().unwrap_or(Value::Null),
        "query_index_bytes": lifecycle.get("query_index_bytes").cloned().unwrap_or(Value::Null),
        "candidate_count": candidates.len(),
        "index_in_progress": lifecycle.get("incomplete").cloned().unwrap_or_else(|| json!(true)),
        "db_read_error": db_error,
        "staged_availability": staged_availability.clone(),
        "proof_contract": "Fast Candidate Spool context-pack is candidate-only source navigation; graph_proof remains false and normal proof waits for a valid graph DB."
    });
    if options.output_mode.is_compact() {
        let mut value = json!({
            "schema_name": "context_pack_candidate_spool_agent_json",
            "schema_version": AGENT_JSON_SCHEMA_VERSION,
            "status": "ok",
            "task": options.task,
            "mode": options.mode,
            "repo": path_string(repo_root),
            "db": Value::Null,
            "proof_status": "candidate_only",
            "proof_strength": "candidate_evidence",
            "evidence_status": "source_navigation_evidence",
            "graph_proof": false,
            "candidate_only": true,
            "claimable_for_graph": false,
            "index_in_progress": lifecycle.get("incomplete").and_then(Value::as_bool).unwrap_or(true),
            "context_lifecycle": "partial_candidate_context",
            "lifecycle": {
                "decision": "partial_candidate_context",
                "indexing_in_progress": lifecycle.get("incomplete").and_then(Value::as_bool).unwrap_or(true),
                "claimable": false,
                "diagnostic_only": false,
            },
            "graph_verification": {
                "status": "no_graph_db",
                "proof_status": "no_proof_path_found",
                "graph_proof": false,
                "reason": "Graph proof unavailable until final DB is ready."
            },
            "candidate_spool": lifecycle,
            "candidate_spool_trace": trace,
            "staged_availability": staged_availability.clone(),
            "routing_packet": staged_routing_packet_for_spool(&options.task, &options.mode, &snippets, &staged_availability),
            "snippets": snippets,
            "candidates": candidates.into_values().collect::<Vec<_>>(),
            "timings": agent_timings_json(total_start),
            "warnings": staged_warning_values(&staged_availability),
            "errors": [],
        });
        merge_json_object(&mut value, staged_fields);
        attach_context_pack_patch_assist_packet(
            &mut value,
            &options.task,
            &options.mode,
            &staged_availability,
            options
                .max_output_bytes
                .unwrap_or(DEFAULT_CONTEXT_AGENT_MAX_OUTPUT_BYTES),
        );
        return Ok(value);
    }
    let mut value = json!({
        "status": "ok",
        "task": options.task,
        "mode": options.mode,
        "packet": {
            "task": options.task,
            "mode": options.mode,
            "verified_paths": [],
            "snippets": snippets,
            "metadata": {
                "candidate_spool_trace": trace,
                "proof_status": "candidate_only",
                "proof_strength": "candidate_evidence",
                "graph_proof": false,
                "candidate_only": true,
                "staged_availability": staged_availability.clone(),
            }
        },
        "candidate_spool": lifecycle,
        "proof_status": "candidate_only",
        "proof_strength": "candidate_evidence",
        "evidence_status": "source_navigation_evidence",
        "graph_proof": false,
        "candidate_only": true,
        "claimable_for_graph": false,
        "staged_availability": staged_availability.clone(),
        "proof": "Fast Candidate Spool context-pack returns candidate-only source-navigation evidence and cannot answer graph proof.",
    });
    merge_json_object(&mut value, staged_fields);
    attach_context_pack_patch_assist_packet(
        &mut value,
        &options.task,
        &options.mode,
        &staged_availability,
        options
            .max_output_bytes
            .unwrap_or(DEFAULT_CONTEXT_AGENT_MAX_OUTPUT_BYTES),
    );
    Ok(value)
}

#[derive(Debug, Clone)]
struct ContextPackVectorBranch {
    index_path: PathBuf,
    status: VectorCandidateBranchStatus,
    candidates: Vec<RetrievalCandidate>,
    index_metrics: Option<Value>,
    warning: Option<String>,
}

fn load_context_pack_vector_branch(
    repo_root: &Path,
    db_path: &Path,
    options: &ContextPackOptions,
) -> ContextPackVectorBranch {
    let index_path = context_pack_vector_index_path(db_path, options);
    if !index_path.exists() {
        return ContextPackVectorBranch {
            index_path,
            status: VectorCandidateBranchStatus::Missing,
            candidates: Vec::new(),
            index_metrics: None,
            warning: Some("vector index missing; continuing without vector candidates".to_string()),
        };
    }

    let provider = match context_pack_vector_provider() {
        Ok(provider) => provider,
        Err(error) => {
            return ContextPackVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: format!("deterministic vector provider unavailable: {error}"),
                },
                candidates: Vec::new(),
                index_metrics: None,
                warning: Some("deterministic vector provider unavailable".to_string()),
            };
        }
    };
    let store = match SqliteGraphStore::open_read_only(db_path) {
        Ok(store) => store,
        Err(error) => {
            return ContextPackVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: format!("db open failed for vector index validation: {error}"),
                },
                candidates: Vec::new(),
                index_metrics: None,
                warning: Some("vector index validation could not open graph DB".to_string()),
            };
        }
    };
    let passport = match store.get_db_passport() {
        Ok(Some(passport)) => passport,
        Ok(None) => {
            return ContextPackVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: "db passport missing".to_string(),
                },
                candidates: Vec::new(),
                index_metrics: None,
                warning: Some(
                    "vector index ignored because graph DB passport is missing".to_string(),
                ),
            };
        }
        Err(error) => {
            return ContextPackVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: format!("db passport read failed: {error}"),
                },
                candidates: Vec::new(),
                index_metrics: None,
                warning: Some(
                    "vector index ignored because graph DB passport could not be read".to_string(),
                ),
            };
        }
    };

    let build_options = context_pack_vector_build_options();
    let index = match load_vector_chunk_index_json(&index_path, &provider, &passport, build_options)
    {
        Ok(index) => index,
        Err(error) => {
            return ContextPackVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: error.to_string(),
                },
                candidates: Vec::new(),
                index_metrics: None,
                warning: Some(format!(
                    "vector index stale or incompatible; continuing without vector candidates: {error}"
                )),
            };
        }
    };
    let source_binding_validation = match validate_vector_chunk_source_bindings(repo_root, &index) {
        Ok(validation) if validation.is_valid() => validation,
        Ok(validation) => {
            return ContextPackVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: format!(
                        "vector_source_binding_stale: {}",
                        validation.stale_reasons.join("; ")
                    ),
                },
                candidates: Vec::new(),
                index_metrics: Some(json!({
                    "source_binding_validation": validation,
                })),
                warning: Some(
                    "vector index source file bindings are stale; continuing without vector candidates"
                        .to_string(),
                ),
            };
        }
        Err(error) => {
            return ContextPackVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: format!("vector source binding validation failed: {error}"),
                },
                candidates: Vec::new(),
                index_metrics: None,
                warning: Some(
                    "vector index source file binding validation failed; continuing without vector candidates"
                        .to_string(),
                ),
            };
        }
    };
    let hits = match index.search(
        &provider,
        &options.task,
        CONTEXT_PACK_VECTOR_CANDIDATE_TOP_K,
    ) {
        Ok(hits) => hits,
        Err(error) => {
            return ContextPackVectorBranch {
                index_path,
                status: VectorCandidateBranchStatus::Stale {
                    reason: error.to_string(),
                },
                candidates: Vec::new(),
                index_metrics: None,
                warning: Some(format!(
                    "vector index query failed; continuing without vector candidates: {error}"
                )),
            };
        }
    };
    let mut index_metrics_value = context_pack_vector_index_metrics_json(&index_path, &index);
    if let Some(object) = index_metrics_value.as_object_mut() {
        object.insert(
            "source_binding_validation".to_string(),
            serde_json::to_value(&source_binding_validation).unwrap_or(Value::Null),
        );
    }
    let index_metrics = Some(index_metrics_value);
    let candidates = hits
        .iter()
        .map(|hit| {
            vector_chunk_search_hit_to_retrieval_candidate(
                hit,
                provider.metadata(),
                &options.task,
                None,
            )
        })
        .collect::<Vec<_>>();

    ContextPackVectorBranch {
        index_path,
        status: VectorCandidateBranchStatus::Ready,
        candidates,
        index_metrics,
        warning: None,
    }
}

fn context_pack_vector_index_metrics_json(
    index_path: &Path,
    index: &codegraph_index::InMemoryVectorChunkIndex,
) -> Value {
    let metadata = index.metadata();
    let actual_index_file_bytes = fs::metadata(index_path).map(|metadata| metadata.len()).ok();
    let artifact_kind =
        context_pack_vector_artifact_kind(index_path, &metadata.index_artifact_format);
    json!({
        "actual_index_file_bytes": actual_index_file_bytes,
        "artifact_kind": artifact_kind,
        "runtime_sidecar_bytes": if artifact_kind == "vector_runtime_sidecar" {
            actual_index_file_bytes.map(Value::from).unwrap_or(Value::Null)
        } else {
            Value::Null
        },
        "audit_artifact_bytes": Value::Null,
        "pretty_json_overhead": Value::Null,
        "estimated_f32_payload_bytes": metadata.estimated_f32_payload_bytes,
        "estimated_f32_payload_dim": metadata.estimated_f32_payload_dim,
        "estimated_f32_payload_count": metadata.estimated_f32_payload_count,
        "index_artifact_format": metadata.index_artifact_format.clone(),
        "vector_payload_compression": metadata.vector_payload_compression.clone(),
        "stores_chunk_text": metadata.stores_chunk_text,
        "stores_chunk_metadata": metadata.stores_chunk_metadata,
        "stores_full_source_body": metadata.stores_full_source_body,
        "generated_total_chunks": metadata.generated_total_chunks,
        "selected_total_chunks": metadata.selected_total_chunks,
        "persisted_total_chunks": metadata.persisted_total_chunks,
        "runtime_selected_chunks": if artifact_kind == "vector_runtime_sidecar" {
            metadata.selected_total_chunks
        } else {
            0
        },
        "audit_chunks": 0,
        "generated_text_evidence_chunks": metadata.generated_text_evidence_chunks,
        "selected_text_evidence_chunks": metadata.selected_text_evidence_chunks,
        "persisted_text_evidence_chunks": metadata.persisted_text_evidence_chunks,
        "generated_graph_entity_chunks": metadata.generated_graph_entity_chunks,
        "selected_graph_entity_chunks": metadata.selected_graph_entity_chunks,
        "persisted_graph_entity_chunks": metadata.persisted_graph_entity_chunks,
        "generated_file_path_title_chunks": metadata.generated_file_path_title_chunks,
        "selected_file_path_title_chunks": metadata.selected_file_path_title_chunks,
        "persisted_file_path_title_chunks": metadata.persisted_file_path_title_chunks,
        "chunk_selection_strategy": metadata.chunk_selection_strategy.clone(),
        "input_order_cap": metadata.input_order_cap,
        "chunk_cap": metadata.chunk_cap,
        "chunk_cap_applied": metadata.chunk_cap_applied,
        "persisted_chunks_by_top_level_dir": metadata.persisted_chunks_by_top_level_dir.clone(),
        "persisted_chunks_by_file_kind": metadata.persisted_chunks_by_file_kind.clone(),
        "persisted_chunks_by_source_kind": metadata.persisted_chunks_by_source_kind.clone(),
    })
}

fn context_pack_vector_artifact_kind(index_path: &Path, artifact_format: &str) -> &'static str {
    if let Ok(text) = fs::read_to_string(index_path) {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if let Some(kind) = value
                .get("metadata")
                .and_then(|metadata| metadata.get("artifact_kind"))
                .and_then(Value::as_str)
            {
                return match kind {
                    "vector_runtime_sidecar" => "vector_runtime_sidecar",
                    "audit_artifact" => "audit_artifact",
                    "candidate_spool" => "candidate_spool",
                    "legacy_pretty_json_vector_artifact" => "legacy_pretty_json_vector_artifact",
                    _ => "unknown",
                };
            }
        }
    }
    if artifact_format == "pretty_json" {
        "legacy_pretty_json_vector_artifact"
    } else if artifact_format == "compact_json" {
        "vector_runtime_sidecar"
    } else {
        "unknown"
    }
}

fn context_pack_vector_diagnostic_trace(
    options: &ContextPackOptions,
    fallback_edges: &[Edge],
    vector_branch: &ContextPackVectorBranch,
) -> Result<Value, String> {
    let mut config = RetrievalFunnelConfig::default();
    config.vector_candidate_top_k = CONTEXT_PACK_VECTOR_CANDIDATE_TOP_K;
    let funnel = RetrievalFunnel::new(
        fallback_edges.to_vec(),
        Vec::<RetrievalDocument>::new(),
        config,
    )
    .map_err(|error| format!("vector diagnostic funnel failed: {error}"))?;
    let result = funnel
        .run(
            RetrievalFunnelRequest::new(
                options.task.clone(),
                options.mode.clone(),
                options.token_budget,
            )
            .exact_seeds(options.seeds.clone())
            .enable_vector_candidates(true)
            .vector_candidate_diagnostics(true)
            .vector_branch_status(vector_branch.status.clone())
            .vector_candidates(vector_branch.candidates.clone()),
        )
        .map_err(|error| format!("vector diagnostic funnel failed: {error}"))?;
    let mut trace = result
        .packet
        .metadata
        .get("vector_candidate_trace")
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "schema_version": 1,
                "diagnostic_only": true,
                "vector_enabled": true,
                "vector_index_status": vector_branch.status.as_str(),
                "vector_candidate_count": vector_branch.candidates.len(),
                "proof_contract": "vector candidates are candidate recall only and are not graph proof unless graph/source verification succeeds"
            })
        });
    if let Some(object) = trace.as_object_mut() {
        object.insert(
            "vector_index_path".to_string(),
            json!(path_string(&vector_branch.index_path)),
        );
        if let Some(index_metrics) = &vector_branch.index_metrics {
            object.insert("vector_index_metrics".to_string(), index_metrics.clone());
        }
    }
    Ok(trace)
}

fn context_pack_vector_index_path(db_path: &Path, options: &ContextPackOptions) -> PathBuf {
    if let Some(path) = &options.vector_index_path {
        return if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.clone())
        };
    }
    db_path
        .parent()
        .map(|parent| parent.join(CONTEXT_PACK_VECTOR_INDEX_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(CONTEXT_PACK_VECTOR_INDEX_FILE_NAME))
}

fn context_pack_vector_provider() -> Result<DeterministicTestEmbeddingProvider, String> {
    DeterministicTestEmbeddingProvider::new(
        CONTEXT_PACK_VECTOR_PROVIDER_DIMENSION,
        TestEmbeddingEnablement::Explicit,
    )
    .map_err(|error| error.to_string())
}

fn context_pack_vector_build_options() -> VectorChunkIndexBuildOptions {
    VectorChunkIndexBuildOptions {
        max_chunks: CONTEXT_PACK_VECTOR_CANDIDATE_TOP_K.saturating_mul(256),
        source_scope: CONTEXT_PACK_VECTOR_SOURCE_SCOPE.to_string(),
        extraction_version: codegraph_index::VECTOR_EMBEDDING_CHUNK_EXTRACTION_VERSION.to_string(),
    }
}

#[derive(Debug, Clone)]
struct ContextPackNuanceRescueBranch {
    candidates: Vec<Value>,
    trace: Value,
}

#[derive(Debug, Clone)]
struct ContextPackNuanceToken {
    value: String,
    normalized: String,
    raw_lower: String,
    kind: &'static str,
    rescue_reason: &'static str,
    exact_match: bool,
    seed_value: Option<String>,
}

fn load_context_pack_nuance_rescue_branch(
    connection: &Connection,
    repo_root: &Path,
    options: &ContextPackOptions,
    raw_seed_values: &[String],
    lifecycle_claimable: bool,
    budgets: ContextPackBudgets,
) -> Result<ContextPackNuanceRescueBranch, String> {
    let (tokens, ignored_generic_tokens) =
        context_pack_nuance_rescue_tokens(options, raw_seed_values);
    let mut candidates = BTreeMap::<String, Value>::new();
    let schema_ready = sqlite_table_exists(connection, "entities")?
        && sqlite_table_exists(connection, "files")?
        && sqlite_table_exists(connection, "object_id_dict")?
        && sqlite_table_exists(connection, "symbol_dict")?
        && sqlite_table_exists(connection, "qualified_name_dict")?
        && sqlite_table_exists(connection, "path_dict")?;

    if schema_ready {
        for token in &tokens {
            if candidates.len() >= CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT {
                break;
            }
            let token_limit = if token.kind == "route_literal" {
                CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT
            } else if token.kind == "rare_token" {
                4
            } else {
                2
            };
            let per_token_limit = CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT
                .saturating_sub(candidates.len())
                .min(token_limit)
                .max(1);
            for entity in load_context_pack_nuance_entity_hits(connection, token, per_token_limit)?
            {
                let source_line_candidate = context_pack_nuance_source_line_candidate_json(
                    repo_root,
                    &entity.repo_relative_path,
                    token,
                    raw_seed_values,
                    lifecycle_claimable,
                );
                if let Some(candidate) = context_pack_nuance_entity_candidate_json(
                    repo_root,
                    &entity,
                    token,
                    raw_seed_values,
                    lifecycle_claimable,
                ) {
                    context_pack_insert_nuance_candidate(&mut candidates, candidate);
                }
                if let Some(candidate) = source_line_candidate {
                    context_pack_insert_nuance_candidate(&mut candidates, candidate);
                }
                if candidates.len() >= CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT {
                    break;
                }
            }
        }

        for token in &tokens {
            if candidates.len() >= CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT {
                break;
            }
            let remaining = CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT
                .saturating_sub(candidates.len())
                .max(1);
            for path in load_context_pack_nuance_path_hits(connection, token, remaining * 4)? {
                if let Some(candidate) = context_pack_nuance_path_candidate_json(
                    &path,
                    token,
                    raw_seed_values,
                    lifecycle_claimable,
                ) {
                    context_pack_insert_nuance_candidate(&mut candidates, candidate);
                }
                if candidates.len() >= CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT {
                    break;
                }
            }
        }
    }

    if sqlite_table_exists(connection, "stage0_fts")?
        && sqlite_table_exists(connection, "files")?
        && sqlite_table_exists(connection, "path_dict")?
        && sqlite_table_has_column(connection, "files", "metadata_json")?
    {
        for token in &tokens {
            if candidates.len() >= CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT {
                break;
            }
            let remaining = CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT
                .saturating_sub(candidates.len())
                .max(1);
            for hit in
                load_context_pack_text_evidence_hits(connection, &token.value, remaining * 2)?
            {
                if !context_pack_text_evidence_allowed_for_mode(
                    &hit.repo_relative_path,
                    &hit.metadata,
                    &options.mode,
                ) {
                    continue;
                }
                if let Some(candidate) = context_pack_nuance_text_candidate_json(
                    hit,
                    token,
                    raw_seed_values,
                    lifecycle_claimable,
                ) {
                    context_pack_insert_nuance_candidate(&mut candidates, candidate);
                }
                if candidates.len() >= CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT {
                    break;
                }
            }
        }
    }

    let candidates = candidates.into_values().collect::<Vec<_>>();
    let trace = context_pack_nuance_rescue_trace(
        true,
        &tokens,
        &ignored_generic_tokens,
        &candidates,
        if schema_ready {
            "ready"
        } else {
            "missing_schema"
        },
    );
    let _ = budgets;
    Ok(ContextPackNuanceRescueBranch { candidates, trace })
}

fn context_pack_nuance_rescue_trace(
    enabled: bool,
    tokens: &[ContextPackNuanceToken],
    ignored_generic_tokens: &[String],
    candidates: &[Value],
    status: &str,
) -> Value {
    let rescue_candidate_sources = context_pack_nuance_candidate_sources(candidates);
    let rescue_reasons = candidates
        .iter()
        .take(CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT)
        .map(|candidate| {
            json!({
                "candidate_id": candidate.get("candidate_id").cloned().unwrap_or(Value::Null),
                "path": candidate.get("path").cloned().unwrap_or(Value::Null),
                "entity_id": candidate.get("entity_id").cloned().unwrap_or(Value::Null),
                "rescue_reason": candidate.get("rescue_reason").cloned().unwrap_or(Value::Null),
                "matched_token": context_pack_trace_safe_value(candidate.get("matched_token")),
                "evidence_role": candidate.get("evidence_role").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    let candidates_accepted = candidates
        .iter()
        .take(CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT)
        .map(context_pack_nuance_trace_candidate_item)
        .collect::<Vec<_>>();
    let safe_ignored_generic_tokens = ignored_generic_tokens
        .iter()
        .take(32)
        .map(|token| context_pack_trace_safe_string(token))
        .collect::<Vec<_>>();
    json!({
        "schema_version": 1,
        "diagnostic_only": true,
        "nuance_rescue_enabled": enabled,
        "rescue_enabled": enabled,
        "rescue_status": status,
        "rescue_candidate_count": candidates.len(),
        "rescue_candidate_sources": rescue_candidate_sources,
        "candidate_cap": CONTEXT_PACK_NUANCE_RESCUE_CANDIDATE_LIMIT,
        "token_count": tokens.len(),
        "matched_tokens": tokens.iter().map(|token| context_pack_trace_safe_string(&token.value)).collect::<Vec<_>>(),
        "rare_tokens": context_pack_nuance_trace_tokens_by_kind(tokens, "rare_token"),
        "identifier_tokens": context_pack_nuance_trace_tokens_by_kind(tokens, "identifier_signature"),
        "path_title_tokens": context_pack_nuance_trace_tokens_by_kind(tokens, "path_title"),
        "route_tokens": context_pack_nuance_trace_tokens_by_kind(tokens, "route_literal"),
        "config_tokens": context_pack_nuance_trace_tokens_by_kind(tokens, "config_key"),
        "test_tokens": context_pack_nuance_trace_tokens_by_kind(tokens, "test_name"),
        "route_config_test_tokens": context_pack_nuance_trace_tokens_by_kinds(
            tokens,
            &["route_literal", "config_key", "test_name"],
        ),
        "ignored_generic_tokens": safe_ignored_generic_tokens,
        "rescue_reasons": rescue_reasons,
        "candidates_accepted": {
            "count": candidates_accepted.len(),
            "items": candidates_accepted
        },
        "candidates_rejected": {
            "count": ignored_generic_tokens.len(),
            "reason": "generic_or_negative_token_ignored",
            "tokens": ignored_generic_tokens.iter().take(32).map(|token| context_pack_trace_safe_string(token)).collect::<Vec<_>>()
        },
        "graph_verification_status": context_pack_nuance_graph_status_summary(candidates),
        "no_proof_fallback_status": "not_evaluated_before_context_pack_graph_verification",
        "proof_contract": "nuance rescue candidates are deterministic candidate recall only; graph_proof remains false until graph/source verification"
    })
}

fn context_pack_nuance_trace_tokens_by_kind(
    tokens: &[ContextPackNuanceToken],
    kind: &str,
) -> Vec<String> {
    context_pack_nuance_trace_tokens_by_kinds(tokens, &[kind])
}

fn context_pack_nuance_trace_tokens_by_kinds(
    tokens: &[ContextPackNuanceToken],
    kinds: &[&str],
) -> Vec<String> {
    tokens
        .iter()
        .filter(|token| kinds.contains(&token.kind))
        .map(|token| context_pack_trace_safe_string(&token.value))
        .take(32)
        .collect()
}

fn context_pack_nuance_candidate_sources(candidates: &[Value]) -> Vec<String> {
    let mut sources = BTreeSet::<String>::new();
    for candidate in candidates {
        for source in context_agent_candidate_sources(candidate) {
            sources.insert(source);
        }
    }
    sources.into_iter().collect()
}

fn context_pack_nuance_trace_candidate_item(candidate: &Value) -> Value {
    json!({
        "candidate_id": candidate.get("candidate_id").cloned().unwrap_or(Value::Null),
        "path": candidate.get("path").cloned().unwrap_or(Value::Null),
        "entity_id": candidate.get("entity_id").cloned().unwrap_or(Value::Null),
        "candidate_sources": candidate.get("candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "rescue_reason": candidate.get("rescue_reason").cloned().unwrap_or(Value::Null),
        "matched_token": context_pack_trace_safe_value(candidate.get("matched_token")),
        "evidence_role": candidate.get("evidence_role").cloned().unwrap_or(Value::Null),
        "proof_status": candidate.get("proof_status").cloned().unwrap_or(Value::Null),
        "graph_proof": candidate.get("graph_proof").cloned().unwrap_or(Value::Null),
        "verification_status": candidate.get("verification_status").cloned().unwrap_or(Value::Null),
        "graph_verification_status": candidate.get("graph_verification_status").cloned().unwrap_or(Value::Null),
        "text_evidence_status": candidate.get("text_evidence_status").cloned().unwrap_or(Value::Null),
    })
}

fn context_pack_nuance_graph_status_summary(candidates: &[Value]) -> Value {
    let mut status_counts = BTreeMap::<String, usize>::new();
    let mut graph_proof_count = 0usize;
    let mut graph_verified_count = 0usize;
    for candidate in candidates {
        let status = candidate
            .get("graph_verification_status")
            .or_else(|| candidate.get("verification_status"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        *status_counts.entry(status.to_string()).or_default() += 1;
        if candidate
            .get("graph_proof")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            graph_proof_count += 1;
        }
        if status == "graph_verified" {
            graph_verified_count += 1;
        }
    }
    json!({
        "status_counts": status_counts,
        "candidate_count": candidates.len(),
        "graph_verified_count": graph_verified_count,
        "graph_proof_count": graph_proof_count,
    })
}

fn context_pack_trace_safe_value(value: Option<&Value>) -> Value {
    value
        .and_then(Value::as_str)
        .map(|token| json!(context_pack_trace_safe_string(token)))
        .unwrap_or(Value::Null)
}

fn context_pack_trace_safe_string(value: &str) -> String {
    if context_pack_trace_secret_like(value) {
        "[redacted_secret_like_token]".to_string()
    } else {
        value.chars().take(96).collect()
    }
}

fn context_pack_trace_safe_text(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let contains_secret_marker = lower.contains("secret_token")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("password=")
        || lower.contains("token=")
        || lower.contains("secret=");
    let contains_secret_part = value.split_whitespace().any(|part| {
        let trimmed = part.trim_matches(|character: char| {
            matches!(
                character,
                ',' | ';' | ':' | '(' | ')' | '[' | ']' | '"' | '\''
            )
        });
        context_pack_trace_secret_like(trimmed)
    });
    if contains_secret_marker || contains_secret_part {
        "[redacted_secret_like_text]".to_string()
    } else {
        value.chars().take(512).collect()
    }
}

fn context_pack_trace_secret_like(value: &str) -> bool {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("xoxb-")
        || lower.contains("secret_token")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("password=")
        || lower.contains("token=")
        || lower.contains("secret=")
    {
        return true;
    }
    let alnum_count = trimmed
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .count();
    alnum_count >= 32
        && trimmed
            .chars()
            .any(|character| character.is_ascii_lowercase())
        && trimmed
            .chars()
            .any(|character| character.is_ascii_uppercase())
        && trimmed.chars().any(|character| character.is_ascii_digit())
}

fn context_pack_insert_nuance_candidate(
    candidates: &mut BTreeMap<String, Value>,
    candidate: Value,
) {
    let key = context_pack_nuance_candidate_key(&candidate);
    if let Some(existing) = candidates.get_mut(&key) {
        merge_context_agent_candidate(existing, candidate);
    } else {
        candidates.insert(key, candidate);
    }
}

fn context_pack_nuance_candidate_key(candidate: &Value) -> String {
    if let Some(entity_id) = candidate.get("entity_id").and_then(Value::as_str) {
        if !entity_id.trim().is_empty() {
            return format!("entity:{entity_id}");
        }
    }
    if let Some(path) = candidate.get("path").and_then(Value::as_str) {
        return format!("path:{}", context_agent_path_key(path));
    }
    context_agent_candidate_id(candidate)
}

fn context_pack_nuance_rescue_tokens(
    options: &ContextPackOptions,
    raw_seed_values: &[String],
) -> (Vec<ContextPackNuanceToken>, Vec<String>) {
    let (positive_task, negative_task) = context_pack_nuance_positive_task(&options.task);
    let mut ignored = BTreeSet::<String>::new();
    for token in context_pack_nuance_split_terms(&negative_task) {
        ignored.insert(token);
    }

    let mut tokens = Vec::<ContextPackNuanceToken>::new();
    let mut seen = BTreeSet::<String>::new();
    for seed in raw_seed_values
        .iter()
        .chain(options.seeds.iter())
        .chain(options.stage0_candidates.iter())
    {
        context_pack_push_nuance_token(
            &mut tokens,
            &mut seen,
            &mut ignored,
            seed,
            Some(seed.clone()),
            true,
        );
    }
    for token in context_pack_nuance_split_terms(&positive_task) {
        context_pack_push_nuance_token(&mut tokens, &mut seen, &mut ignored, &token, None, false);
    }

    (
        tokens
            .into_iter()
            .take(CONTEXT_PACK_NUANCE_RESCUE_TOKEN_LIMIT)
            .collect(),
        ignored.into_iter().collect(),
    )
}

fn context_pack_push_nuance_token(
    tokens: &mut Vec<ContextPackNuanceToken>,
    seen: &mut BTreeSet<String>,
    ignored: &mut BTreeSet<String>,
    raw: &str,
    seed_value: Option<String>,
    seed_exact: bool,
) {
    let trimmed = raw
        .trim()
        .trim_matches(|ch: char| matches!(ch, ',' | ';' | '"' | '\''));
    if trimmed.is_empty() {
        return;
    }
    let kind = context_pack_nuance_token_kind(trimmed, seed_exact);
    let normalized = context_pack_nuance_normalized_token(trimmed);
    let raw_lower = trimmed.replace('\\', "/").to_ascii_lowercase();
    if normalized.is_empty() || context_pack_nuance_generic_token(&normalized, kind) {
        ignored.insert(normalized);
        return;
    }
    let key = format!("{kind}:{normalized}");
    if !seen.insert(key) {
        return;
    }
    tokens.push(ContextPackNuanceToken {
        value: trimmed.to_string(),
        normalized,
        raw_lower,
        kind,
        rescue_reason: context_pack_nuance_rescue_reason(kind),
        exact_match: seed_exact || context_pack_nuance_token_requires_exact(kind),
        seed_value,
    });
}

fn context_pack_nuance_positive_task(task: &str) -> (String, String) {
    let lower = task.to_ascii_lowercase();
    let mut split_at = None;
    for delimiter in [
        " without ",
        ", not ",
        "; not ",
        " but not ",
        " rather than ",
        " instead of ",
        " except ",
        " not ",
    ] {
        if let Some(index) = lower.find(delimiter) {
            split_at = Some(split_at.map_or(index, |current: usize| current.min(index)));
        }
    }
    if let Some(index) = split_at {
        (task[..index].to_string(), task[index..].to_string())
    } else {
        (task.to_string(), String::new())
    }
}

fn context_pack_nuance_split_terms(value: &str) -> Vec<String> {
    let mut output = BTreeSet::<String>::new();
    for raw in value.split_whitespace() {
        let token = raw.trim_matches(|ch: char| {
            matches!(ch, ',' | ';' | ':' | '(' | ')' | '[' | ']' | '"' | '\'')
        });
        if token.contains('/') || token.contains('-') || token.contains('_') {
            let normalized = token
                .trim_matches(|ch: char| matches!(ch, ',' | ';' | '"' | '\''))
                .to_string();
            if !normalized.is_empty() {
                output.insert(normalized);
            }
        }
        for part in context_pack_nuance_identifier_parts(token) {
            output.insert(part);
        }
    }
    output.into_iter().collect()
}

fn context_pack_nuance_identifier_parts(value: &str) -> Vec<String> {
    let mut parts = Vec::<String>::new();
    let mut current = String::new();
    let mut previous_lowercase = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if character.is_ascii_uppercase() && previous_lowercase && !current.is_empty() {
                parts.push(current.to_ascii_lowercase());
                current.clear();
            }
            current.push(character);
            previous_lowercase = character.is_ascii_lowercase();
        } else {
            if !current.is_empty() {
                parts.push(current.to_ascii_lowercase());
                current.clear();
            }
            previous_lowercase = false;
        }
    }
    if !current.is_empty() {
        parts.push(current.to_ascii_lowercase());
    }
    parts
}

fn context_pack_nuance_token_kind(raw: &str, seed_exact: bool) -> &'static str {
    let lower = raw.replace('\\', "/").to_ascii_lowercase();
    if lower.starts_with('/') || lower.contains(":/") || lower.contains("/:") {
        "route_literal"
    } else if lower.starts_with("br2_") || lower.contains("config.in") {
        "config_key"
    } else if lower.contains('/') || lower.contains('\\') || lower.contains('-') {
        "path_title"
    } else if lower.contains("test") || lower.contains("spec") || lower.starts_with("failing_") {
        "test_name"
    } else if seed_exact
        || raw.contains('_')
        || raw.chars().any(|character| character.is_ascii_uppercase())
    {
        "identifier_signature"
    } else {
        "rare_token"
    }
}

fn context_pack_nuance_rescue_reason(kind: &str) -> &'static str {
    match kind {
        "route_literal" => "route_literal_rescue",
        "config_key" => "config_key_rescue",
        "path_title" => "path_title_rescue",
        "test_name" => "test_name_rescue",
        "identifier_signature" => "identifier_signature_rescue",
        _ => "rare_token_lexical_rescue",
    }
}

fn context_pack_nuance_token_requires_exact(kind: &str) -> bool {
    matches!(
        kind,
        "route_literal" | "config_key" | "path_title" | "test_name" | "identifier_signature"
    )
}

fn context_pack_nuance_normalized_token(value: &str) -> String {
    value
        .replace('\\', "/")
        .trim()
        .trim_matches(|ch: char| matches!(ch, ',' | ';' | '"' | '\''))
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '_' | '-' | '/' | '.' | ':' | '#')
        })
        .collect::<String>()
        .to_ascii_lowercase()
}

fn context_pack_nuance_generic_token(token: &str, kind: &str) -> bool {
    if matches!(token, "test" | "tests" | "spec" | "specs") {
        return true;
    }
    if matches!(kind, "route_literal" | "config_key" | "path_title") && token.len() >= 4 {
        return false;
    }
    if kind == "test_name" && token.len() >= 8 {
        return false;
    }
    if token.len() < 3 || token.chars().all(|character| character.is_ascii_digit()) {
        return true;
    }
    matches!(
        token,
        "find"
            | "the"
            | "that"
            | "with"
            | "from"
            | "and"
            | "for"
            | "or"
            | "to"
            | "in"
            | "whether"
            | "decides"
            | "tiny"
            | "into"
            | "under"
            | "code"
            | "task"
            | "file"
            | "files"
            | "path"
            | "paths"
            | "title"
            | "symbol"
            | "symbols"
            | "function"
            | "config"
            | "route"
            | "literal"
            | "test"
            | "tests"
            | "name"
            | "package"
            | "metadata"
            | "infrastructure"
            | "support"
            | "script"
            | "docs"
            | "document"
            | "candidate"
            | "candidates"
            | "branch"
            | "rescue"
            | "check"
            | "checks"
            | "token"
            | "tokens"
            | "user"
            | "users"
            | "process"
            | "plural"
            | "batch"
            | "admin"
            | "api"
            | "id"
    )
}

fn load_context_pack_nuance_entity_hits(
    connection: &Connection,
    token: &ContextPackNuanceToken,
    limit: usize,
) -> Result<Vec<ContextEntitySummary>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let (lookup_value, raw_lookup_value) = context_pack_nuance_sql_lookup_values(token);
    let lookup = context_pack_sql_like_pattern(&lookup_value);
    let raw_lookup = context_pack_sql_like_pattern(&raw_lookup_value);
    let mut statement = connection
        .prepare(
            "
            SELECT DISTINCT e.id_key
            FROM entities e
            JOIN symbol_dict name ON name.id = e.name_id
            JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
            JOIN path_dict path ON path.id = e.path_id
            LEFT JOIN path_dict span_path ON span_path.id = e.span_path_id
            LEFT JOIN entity_kind_dict kind ON kind.id = e.kind_id
            WHERE lower(name.value) LIKE ?1 ESCAPE '\\'
               OR lower(qname.value) LIKE ?1 ESCAPE '\\'
               OR lower(path.value) LIKE ?1 ESCAPE '\\'
               OR lower(span_path.value) LIKE ?1 ESCAPE '\\'
               OR lower(name.value) LIKE ?2 ESCAPE '\\'
               OR lower(qname.value) LIKE ?2 ESCAPE '\\'
               OR lower(path.value) LIKE ?2 ESCAPE '\\'
               OR lower(span_path.value) LIKE ?2 ESCAPE '\\'
            ORDER BY
                CASE WHEN lower(name.value) = ?3 OR lower(qname.value) = ?3 THEN 0 ELSE 1 END,
                CASE WHEN lower(path.value) LIKE 'tests/%' OR lower(path.value) LIKE '%/tests/%' THEN 1 ELSE 0 END,
                CASE kind.value
                    WHEN 'Const' THEN 0
                    WHEN 'Variable' THEN 0
                    WHEN 'Function' THEN 1
                    WHEN 'Method' THEN 1
                    ELSE 2
                END,
                e.start_line, length(name.value), qname.value, e.id_key
            LIMIT ?4
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![lookup, raw_lookup, token.normalized, limit as i64],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?;
    let keys = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    load_context_entities_by_keys(connection, &keys, limit)
}

fn load_context_pack_nuance_path_hits(
    connection: &Connection,
    token: &ContextPackNuanceToken,
    limit: usize,
) -> Result<Vec<String>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let lookup = context_pack_sql_like_pattern(&token.normalized);
    let raw_lookup = context_pack_sql_like_pattern(&token.raw_lower);
    let mut statement = connection
        .prepare(
            "
            SELECT DISTINCT path.value
            FROM path_dict path
            JOIN files file ON file.path_id = path.id
            WHERE lower(path.value) LIKE ?1 ESCAPE '\\'
               OR lower(path.value) LIKE ?2 ESCAPE '\\'
            ORDER BY length(path.value), path.value
            LIMIT ?3
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![lookup, raw_lookup, limit as i64], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn context_pack_nuance_sql_lookup_values(token: &ContextPackNuanceToken) -> (String, String) {
    if token.kind == "route_literal" {
        let route_parts = context_pack_nuance_route_discriminator_parts(&token.raw_lower);
        if let Some(part) = route_parts.first() {
            return (part.clone(), part.clone());
        }
    }
    (token.normalized.clone(), token.raw_lower.clone())
}

fn context_pack_sql_like_pattern(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn context_pack_nuance_entity_candidate_json(
    repo_root: &Path,
    entity: &ContextEntitySummary,
    token: &ContextPackNuanceToken,
    raw_seed_values: &[String],
    lifecycle_claimable: bool,
) -> Option<Value> {
    let excerpt = context_pack_nuance_entity_excerpt(repo_root, entity);
    let haystack = format!(
        "{}\n{}\n{}\n{}",
        entity.name, entity.qualified_name, entity.repo_relative_path, excerpt
    );
    if !context_pack_nuance_haystack_matches(&haystack, token) {
        return None;
    }
    let matched_seeds = context_pack_nuance_matched_seeds(&haystack, token, raw_seed_values);
    let role = context_entity_fallback_role(entity);
    let span = entity
        .source_span
        .as_ref()
        .map(agent_source_span_json)
        .unwrap_or(Value::Null);
    let (snippet, snippet_truncated) = bounded_retrieval_candidate_snippet_text(&excerpt);
    Some(context_pack_nuance_candidate_json(
        format!("nuance-rescue://entity/{}", entity.id),
        Some(entity.repo_relative_path.clone()),
        Some(entity.id.clone()),
        span,
        if snippet.is_empty() {
            Value::Null
        } else {
            json!(snippet)
        },
        role.role.as_str(),
        token,
        matched_seeds,
        lifecycle_claimable,
        snippet_truncated,
        true,
        false,
    ))
}

fn context_pack_nuance_path_candidate_json(
    path: &str,
    token: &ContextPackNuanceToken,
    raw_seed_values: &[String],
    lifecycle_claimable: bool,
) -> Option<Value> {
    if !context_pack_nuance_haystack_matches(path, token) {
        return None;
    }
    let matched_seeds = context_pack_nuance_matched_seeds(path, token, raw_seed_values);
    Some(context_pack_nuance_candidate_json(
        format!(
            "nuance-rescue://path/{}",
            context_agent_stable_component(path)
        ),
        Some(path.to_string()),
        None,
        Value::Null,
        Value::Null,
        "unknown",
        token,
        matched_seeds,
        lifecycle_claimable,
        false,
        false,
        true,
    ))
}

fn context_pack_nuance_text_candidate_json(
    hit: ContextPackTextEvidenceHit,
    token: &ContextPackNuanceToken,
    raw_seed_values: &[String],
    lifecycle_claimable: bool,
) -> Option<Value> {
    let haystack = format!("{}\n{}\n{}", hit.repo_relative_path, hit.title, hit.body);
    if !context_pack_nuance_haystack_matches(&haystack, token) {
        return None;
    }
    let line = context_pack_text_evidence_hit_line(&hit);
    let span = SourceSpan::new(&hit.repo_relative_path, line, line);
    let matched_seeds = context_pack_nuance_matched_seeds(&haystack, token, raw_seed_values);
    let (snippet, snippet_truncated) = bounded_retrieval_candidate_snippet_text(&hit.body);
    let mut candidate = context_pack_nuance_candidate_json(
        format!(
            "nuance-rescue://text/{}:{}",
            hit.id,
            context_span_key(&span)
        ),
        Some(hit.repo_relative_path),
        None,
        agent_source_span_json(&span),
        if snippet.is_empty() {
            Value::Null
        } else {
            json!(snippet)
        },
        "text_evidence",
        token,
        matched_seeds,
        lifecycle_claimable,
        snippet_truncated,
        false,
        false,
    );
    if let Some(object) = candidate.as_object_mut() {
        object.insert(
            "candidate_sources".to_string(),
            json!(["nuance_rescue", "text_evidence", "lexical_fts"]),
        );
        object.insert("proof_status".to_string(), json!("not_graph_proof"));
        object.insert("claimable".to_string(), json!(lifecycle_claimable));
        object.insert("claimable_for_text".to_string(), json!(lifecycle_claimable));
        object.insert("requires_graph_verification".to_string(), json!(false));
        object.insert("verification_status".to_string(), json!("not_graph_proof"));
        object.insert(
            "graph_verification_status".to_string(),
            json!("not_graph_proof"),
        );
        object.insert("text_evidence_status".to_string(), json!("not_graph_proof"));
        object.insert(
            "source_labels".to_string(),
            json!([
                "nuance_rescue",
                token.rescue_reason,
                "text_evidence",
                "candidate_only",
                "no_graph_proof"
            ]),
        );
    }
    Some(candidate)
}

fn context_pack_nuance_source_line_candidate_json(
    repo_root: &Path,
    repo_relative_path: &str,
    token: &ContextPackNuanceToken,
    raw_seed_values: &[String],
    lifecycle_claimable: bool,
) -> Option<Value> {
    let source_path = repo_root.join(repo_relative_path);
    let source = fs::read_to_string(source_path).ok()?;
    let (line_index, line) = source.lines().enumerate().find(|(_, line)| {
        let line_haystack = format!("{repo_relative_path}\n{line}");
        context_pack_nuance_haystack_matches(&line_haystack, token)
    })?;
    let line_number = (line_index + 1) as u32;
    let span = SourceSpan::new(repo_relative_path, line_number, line_number);
    let line_haystack = format!("{repo_relative_path}\n{line}");
    let matched_seeds = context_pack_nuance_matched_seeds(&line_haystack, token, raw_seed_values);
    let (snippet, snippet_truncated) = bounded_retrieval_candidate_snippet_text(line);
    let evidence_role = if context_pack_test_path(repo_relative_path) {
        "test"
    } else {
        "production"
    };
    let mut candidate = context_pack_nuance_candidate_json(
        format!(
            "nuance-rescue://source/{}:{}",
            context_agent_stable_component(repo_relative_path),
            line_number
        ),
        Some(repo_relative_path.to_string()),
        None,
        agent_source_span_json(&span),
        if snippet.is_empty() {
            Value::Null
        } else {
            json!(snippet)
        },
        evidence_role,
        token,
        matched_seeds,
        lifecycle_claimable,
        snippet_truncated,
        false,
        false,
    );
    if let Some(object) = candidate.as_object_mut() {
        object.insert(
            "candidate_sources".to_string(),
            json!(["nuance_rescue", "source_text_scan"]),
        );
        object.insert(
            "source_labels".to_string(),
            json!([
                "nuance_rescue",
                token.rescue_reason,
                "source_text_scan",
                "candidate_only",
                "no_graph_proof"
            ]),
        );
    }
    Some(candidate)
}

#[allow(clippy::too_many_arguments)]
fn context_pack_nuance_candidate_json(
    candidate_id: String,
    path: Option<String>,
    entity_id: Option<String>,
    span: Value,
    snippet: Value,
    evidence_role: &str,
    token: &ContextPackNuanceToken,
    matched_seeds: Vec<String>,
    lifecycle_claimable: bool,
    snippet_truncated: bool,
    symbol_entity_match: bool,
    file_path_match: bool,
) -> Value {
    let exact_seed_match = !matched_seeds.is_empty();
    let source_score = context_pack_nuance_source_score(token, exact_seed_match);
    let text_evidence_status = if evidence_role == "text_evidence" {
        "not_graph_proof"
    } else {
        "absent"
    };
    let ranking_features = json!({
        "exact_seed_match": exact_seed_match,
        "file_path_match": file_path_match || matches!(token.kind, "path_title" | "route_literal" | "config_key"),
        "text_evidence_match": evidence_role == "text_evidence",
        "symbol_entity_match": symbol_entity_match,
        "graph_proximity": false,
        "source_role_compatible": matches!(evidence_role, "production" | "test" | "text_evidence" | "unknown"),
        "lifecycle_claimable": lifecycle_claimable,
        "proof_available": false,
        "vector_score_available": false,
        "binary_score_available": false,
        "rescue_match": true
    });
    let source_labels = json!([
        "nuance_rescue",
        token.rescue_reason,
        "candidate_only",
        "no_graph_proof"
    ]);
    json!({
        "candidate_id": candidate_id,
        "candidate_source": "nuance_rescue",
        "candidate_sources": [
            "nuance_rescue",
            if symbol_entity_match { "symbol_lookup" } else { "lexical_fts" }
        ],
        "candidate_source_label": token.rescue_reason,
        "file_id": path.clone().map(Value::from).unwrap_or(Value::Null),
        "path": path.map(Value::from).unwrap_or(Value::Null),
        "entity_id": entity_id.map(Value::from).unwrap_or(Value::Null),
        "span": span,
        "snippet": snippet,
        "evidence_role": evidence_role,
        "proof_status": "candidate_only",
        "graph_proof": false,
        "claimable": false,
        "claimable_for_text": false,
        "claimable_for_graph": false,
        "diagnostic_only": false,
        "score": Value::Null,
        "source_score": source_score,
        "rank": Value::Null,
        "matched_seeds": matched_seeds,
        "matched_token": token.value,
        "matched_tokens": [token.value.clone()],
        "rescue_reason": token.rescue_reason,
        "rescue_kinds": [token.kind],
        "requires_graph_verification": true,
        "verification_status": "needs_graph_verification",
        "graph_verification_status": "needs_graph_verification",
        "text_evidence_status": text_evidence_status,
        "reason": format!("{} matched {}; candidate-only until graph/source verification", token.rescue_reason, token.value),
        "omitted": false,
        "truncated": snippet_truncated,
        "ranking_features": ranking_features,
        "source_labels": source_labels
    })
}

fn context_pack_nuance_source_score(token: &ContextPackNuanceToken, exact_seed_match: bool) -> f64 {
    let mut score = match token.kind {
        "route_literal" => 9.0,
        "config_key" => 8.5,
        "test_name" => 8.0,
        "identifier_signature" => 7.5,
        "path_title" => 7.0,
        _ => 4.0,
    };
    if exact_seed_match {
        score += 2.0;
    }
    if token.value.len() >= 16 {
        score += 0.5;
    }
    score
}

fn context_pack_nuance_entity_excerpt(repo_root: &Path, entity: &ContextEntitySummary) -> String {
    let Some(span) = entity.source_span.as_ref() else {
        return String::new();
    };
    let path = repo_root.join(&span.repo_relative_path);
    let Ok(source) = fs::read_to_string(path) else {
        return String::new();
    };
    let start = span.start_line.saturating_sub(1) as usize;
    let end = span.end_line.max(span.start_line) as usize;
    source
        .lines()
        .skip(start)
        .take(end.saturating_sub(start).max(1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn context_pack_nuance_haystack_matches(haystack: &str, token: &ContextPackNuanceToken) -> bool {
    let lower = haystack.replace('\\', "/").to_ascii_lowercase();
    if token.kind == "route_literal" {
        if lower.contains(&token.raw_lower) {
            return true;
        }
        let route_parts = context_pack_nuance_route_discriminator_parts(&token.raw_lower);
        if route_parts.is_empty() {
            return false;
        }
        let full_terms = context_pack_nuance_haystack_terms(haystack);
        let route_context = full_terms.contains("route")
            || full_terms.contains("routes")
            || full_terms.contains("literal")
            || lower.contains("/routes")
            || lower.contains("routes/");
        return route_context
            && route_parts
                .iter()
                .all(|part| full_terms.contains(part) || lower.contains(part));
    }
    if token.kind == "path_title" && lower.contains(&token.raw_lower) {
        return true;
    }
    if matches!(token.kind, "config_key" | "test_name") && lower.contains(&token.normalized) {
        return true;
    }
    let full_terms = context_pack_nuance_haystack_terms(haystack);
    if full_terms.contains(&token.normalized) {
        return true;
    }
    if token.exact_match {
        return false;
    }
    full_terms.contains(&token.raw_lower) || lower.contains(&token.raw_lower)
}

fn context_pack_nuance_route_discriminator_parts(value: &str) -> Vec<String> {
    context_pack_nuance_identifier_parts(value)
        .into_iter()
        .filter(|part| {
            !matches!(
                part.as_str(),
                "api" | "id" | "user" | "users" | "route" | "routes"
            )
        })
        .collect()
}

fn context_pack_nuance_haystack_terms(haystack: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::<String>::new();
    for token in haystack.split(|ch: char| {
        !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/' | '.' | ':' | '#'))
    }) {
        let normalized = context_pack_nuance_normalized_token(token);
        if !normalized.is_empty() {
            terms.insert(normalized.clone());
            terms.insert(
                normalized
                    .chars()
                    .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
                    .collect::<String>(),
            );
        }
        for part in context_pack_nuance_identifier_parts(token) {
            if !part.is_empty() {
                terms.insert(part);
            }
        }
    }
    terms.retain(|term| !term.is_empty());
    terms
}

fn context_pack_nuance_matched_seeds(
    haystack: &str,
    token: &ContextPackNuanceToken,
    raw_seed_values: &[String],
) -> Vec<String> {
    let mut seeds = BTreeSet::<String>::new();
    if let Some(seed) = token.seed_value.as_ref() {
        seeds.insert(seed.clone());
    }
    for seed in raw_seed_values {
        let seed_token = ContextPackNuanceToken {
            value: seed.clone(),
            normalized: context_pack_nuance_normalized_token(seed),
            raw_lower: seed.replace('\\', "/").to_ascii_lowercase(),
            kind: context_pack_nuance_token_kind(seed, true),
            rescue_reason: "exact_seed_overlap",
            exact_match: true,
            seed_value: Some(seed.clone()),
        };
        if context_pack_nuance_haystack_matches(haystack, &seed_token) {
            seeds.insert(seed.clone());
        }
    }
    seeds.into_iter().collect()
}

fn run_impact_command(args: &[String]) -> Result<Value, String> {
    let mut args = args.to_vec();
    let allow_stale_read = remove_flag(&mut args, "--allow-stale-read");
    let allow_foreign_db = remove_flag(&mut args, "--allow-foreign-db");
    let explicit_scope = parse_read_scope_options(&mut args)?;
    if args.len() != 1 {
        return Err("Usage: codegraph-mcp impact <file-or-symbol> [scope flags]".to_string());
    }
    let repo_root = current_repo_root()?;
    let db_path = resolved_db_path_for_repo(&repo_root);
    let db_lifecycle_read = read_db_lifecycle_guard(
        &repo_root,
        &db_path,
        allow_stale_read,
        allow_foreign_db,
        explicit_scope,
    )?;
    let result = with_lifecycle_override_env(allow_stale_read, allow_foreign_db, || {
        impact_value(&repo_root, &args[0])
    });
    let mut value = result?;
    if let Some(object) = value.as_object_mut() {
        object.insert("db_lifecycle_read".to_string(), db_lifecycle_read);
    }
    Ok(value)
}

fn impact_value(repo_root: &Path, target: &str) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let engine = query_engine(&store)?;
    let seeds = resolve_impact_seeds(&store, target)?;
    let limits = default_query_limits();
    let mut callers_callees = Vec::new();
    let mut mutations = Vec::new();
    let mut db_schema = Vec::new();
    let mut apis_auth_security = Vec::new();
    let mut events = Vec::new();
    let mut tests = Vec::new();

    for seed in &seeds {
        let impact = engine.impact_analysis_core(seed, limits);
        callers_callees.extend(impact.callers);
        callers_callees.extend(impact.callees);
        mutations.extend(impact.writes);
        mutations.extend(impact.mutations);
        mutations.extend(impact.dataflow);
        db_schema.extend(impact.migrations);
        apis_auth_security.extend(impact.auth_paths);
        events.extend(impact.event_flow);
        tests.extend(impact.tests);
    }
    let entity_by_id = entities_by_id(
        &store
            .list_entities(UNBOUNDED_STORE_READ_LIMIT)
            .map_err(|error| error.to_string())?,
    );
    let recommended_test_set = minimal_test_set(&tests, &entity_by_id);

    let sections = json!({
        "callers_callees": paths_json(&engine, callers_callees),
        "mutations_dataflow": paths_json(&engine, mutations),
        "db_schema_tables_columns": paths_json(&engine, db_schema),
        "apis_auth_security": paths_json(&engine, apis_auth_security),
        "events_messages": paths_json(&engine, events),
        "tests_assertions_mocks_stubs": paths_json(&engine, tests),
    });
    let summary = section_counts(&sections);

    Ok(json!({
        "status": "ok",
        "target": target,
        "seeds": seeds,
        "summary": summary,
        "blast_radius": sections,
        "recommended_test_set": recommended_test_set,
        "proof": "Impact dashboard is exact graph traversal with PathEvidence outputs.",
    }))
}

fn run_bundle_command(args: &[String]) -> Result<Value, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Err("Usage: codegraph-mcp bundle <export|import> [ARGS]".to_string());
    };
    match subcommand {
        "export" => run_bundle_export(&args[1..]),
        "import" => run_bundle_import(&args[1..]),
        other => Err(format!("unknown bundle subcommand: {other}")),
    }
}

fn parse_init_options(args: &[String]) -> Result<InitOptions, String> {
    let mut repo = None;
    let mut options = InitOptions {
        repo: PathBuf::from("."),
        dry_run: false,
        with_codex_config: false,
        with_agents: false,
        with_skills: false,
        with_hooks: false,
        run_index: false,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dry-run" => options.dry_run = true,
            "--with-codex-config" => options.with_codex_config = true,
            "--with-agents" => options.with_agents = true,
            "--with-skills" => options.with_skills = true,
            "--with-hooks" => options.with_hooks = true,
            "--with-templates" => {
                options.with_agents = true;
                options.with_skills = true;
                options.with_hooks = true;
            }
            "--index" => options.run_index = true,
            "--yes" | "-y" => {}
            "--repo" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--repo requires a path".to_string());
                };
                repo = Some(PathBuf::from(value));
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown init option: {value}"));
            }
            value => {
                if repo.is_some() {
                    return Err(format!("unexpected init argument: {value}"));
                }
                repo = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    if let Some(repo) = repo {
        options.repo = repo;
    }
    Ok(options)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndexJsonOutputMode {
    Concise,
    Agent,
    Audit,
}

impl IndexJsonOutputMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Concise => "concise",
            Self::Agent => "agent_json",
            Self::Audit => "audit_json",
        }
    }
}

#[cfg(test)]
fn parse_index_options(args: &[String]) -> Result<(String, Option<PathBuf>, IndexOptions), String> {
    let (repo, db, options, _, _, _) = parse_index_command_options(args)?;
    Ok((repo, db, options))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VectorIndexCliOptions {
    runtime_path: Option<PathBuf>,
    audit_artifact_path: Option<PathBuf>,
    runtime_format: VectorChunkArtifactFormat,
    no_audit: bool,
}

impl Default for VectorIndexCliOptions {
    fn default() -> Self {
        Self {
            runtime_path: None,
            audit_artifact_path: None,
            runtime_format: VectorChunkArtifactFormat::CompactJson,
            no_audit: false,
        }
    }
}

#[derive(Debug, Clone)]
struct CandidateSpoolBudgetDecision {
    policy: CandidateSpoolPolicy,
    required: bool,
    requested_path: Option<PathBuf>,
    effective_path: Option<PathBuf>,
    budget_bytes: Option<u64>,
    artifact_budget_remaining_bytes: Option<u64>,
    decision: String,
    disabled_reason: Option<String>,
    warning: Option<String>,
}

impl CandidateSpoolBudgetDecision {
    fn base(options: &IndexOptions) -> Self {
        Self {
            policy: options.candidate_spool_policy,
            required: options.candidate_spool_required,
            requested_path: options.candidate_spool_path.clone(),
            effective_path: options.candidate_spool_path.clone(),
            budget_bytes: Some(options.candidate_spool_caps.global_max_bytes as u64),
            artifact_budget_remaining_bytes: None,
            decision: "candidate_spool_not_requested".to_string(),
            disabled_reason: None,
            warning: None,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "candidate_spool_policy": self.policy.as_str(),
            "candidate_spool_required": self.required,
            "candidate_spool_budget_bytes": self.budget_bytes,
            "artifact_budget_remaining_bytes": self.artifact_budget_remaining_bytes,
            "artifact_budget_decision": self.decision,
            "candidate_spool_disabled_reason": self.disabled_reason,
            "candidate_spool_warning": self.warning,
            "candidate_spool_requested_path": self.requested_path.as_ref().map(|path| path_string(path)),
            "candidate_spool_effective_path": self.effective_path.as_ref().map(|path| path_string(path)),
        })
    }
}

fn artifact_budget_bytes(options: &storage_budget::StorageBudgetOptions) -> u64 {
    (options.max_artifacts_mib * 1024.0 * 1024.0).max(0.0) as u64
}

fn apply_candidate_spool_budget_policy(
    options: &mut IndexOptions,
    budget_options: &storage_budget::StorageBudgetOptions,
    vector_options: &VectorIndexCliOptions,
) -> CandidateSpoolBudgetDecision {
    let mut decision = CandidateSpoolBudgetDecision::base(options);
    if options.candidate_spool_policy == CandidateSpoolPolicy::Off {
        decision.effective_path = None;
        decision.budget_bytes = Some(0);
        decision.decision = "candidate_spool_policy_off".to_string();
        decision.disabled_reason = Some("candidate_spool_policy_off".to_string());
        options.candidate_spool_path = None;
        options.candidate_spool_caps.global_max_bytes = 0;
        options.candidate_spool_caps.global_max_records = 0;
        return decision;
    }
    if options.candidate_spool_path.is_none() {
        decision.decision = if options.candidate_spool_required {
            "candidate_spool_required_without_path".to_string()
        } else {
            "candidate_spool_not_requested".to_string()
        };
        if options.candidate_spool_required {
            decision.disabled_reason = Some("candidate_spool_required_without_path".to_string());
        }
        return decision;
    }

    let artifact_budget = artifact_budget_bytes(budget_options);
    let runtime_reserve = if vector_options.runtime_path.is_some() {
        CANDIDATE_SPOOL_RUNTIME_RESERVE_BYTES.min((artifact_budget as usize).saturating_div(2))
    } else {
        0
    };
    let audit_reserve = if vector_options.audit_artifact_path.is_some() {
        CANDIDATE_SPOOL_AUDIT_RESERVE_BYTES.min((artifact_budget as usize).saturating_div(4))
    } else {
        0
    };
    let safety_reserve =
        CANDIDATE_SPOOL_SAFETY_RESERVE_BYTES.min((artifact_budget as usize).saturating_div(4));
    let remaining = (artifact_budget as usize)
        .saturating_sub(runtime_reserve)
        .saturating_sub(audit_reserve)
        .saturating_sub(safety_reserve);
    decision.artifact_budget_remaining_bytes = Some(remaining as u64);

    if remaining < CANDIDATE_SPOOL_MIN_USEFUL_BUDGET_BYTES {
        options.candidate_spool_caps.global_max_bytes = 0;
        options.candidate_spool_caps.global_max_records = 0;
        decision.budget_bytes = Some(0);
        decision.decision = if options.candidate_spool_required {
            "candidate_spool_required_budget_exceeded".to_string()
        } else {
            "candidate_spool_disabled_budget_exceeded".to_string()
        };
        decision.disabled_reason = Some("candidate_spool_budget_too_small".to_string());
        decision.warning = Some(
            "Candidate spool disabled because the remaining artifact budget is below the minimum useful bounded working-set threshold.".to_string(),
        );
        if !options.candidate_spool_required {
            options.candidate_spool_path = None;
            decision.effective_path = None;
        }
        return decision;
    }

    let storage_factor = if options.candidate_spool_query_index {
        CANDIDATE_SPOOL_QUERY_INDEX_STORAGE_FACTOR
    } else {
        1
    };
    let adaptive_jsonl_budget = remaining
        .saturating_div(storage_factor)
        .max(CANDIDATE_SPOOL_MIN_USEFUL_BUDGET_BYTES)
        .min(remaining);
    let original = options.candidate_spool_caps.global_max_bytes;
    let effective = original.min(adaptive_jsonl_budget);
    options.candidate_spool_caps.global_max_bytes = effective;
    decision.budget_bytes = Some(effective as u64);
    if effective < original {
        decision.decision = "candidate_spool_truncated_to_fit_artifact_budget".to_string();
        decision.warning = Some(format!(
            "Candidate spool cap reduced from {original} to {effective} bytes to leave room for required artifacts and the query index."
        ));
    } else {
        decision.decision = "candidate_spool_within_artifact_budget".to_string();
    }
    if options.candidate_spool_policy == CandidateSpoolPolicy::Audit {
        let note = "candidate_spool_policy=audit is explicit diagnostic mode; normal query/status/context-pack still treat spool records as candidate-only and do not use them as graph proof.";
        decision.warning = Some(match decision.warning.take() {
            Some(existing) => format!("{existing} {note}"),
            None => note.to_string(),
        });
    }
    decision
}

fn candidate_spool_footprint_bytes(summary: &IndexSummary) -> u64 {
    summary
        .candidate_spool
        .as_ref()
        .map(|spool| spool.spooled_bytes.saturating_add(spool.query_index_bytes))
        .unwrap_or(0)
}

fn candidate_spool_required_error_value(
    decision: &CandidateSpoolBudgetDecision,
    graph_db_claimable: bool,
    runtime_sidecar_ready: bool,
    message: impl Into<String>,
) -> Value {
    json!({
        "status": "error",
        "error": "candidate_spool_required_budget_exceeded",
        "message": message.into(),
        "candidate_spool_policy": decision.policy.as_str(),
        "candidate_spool_required": decision.required,
        "candidate_spool_budget_bytes": decision.budget_bytes,
        "artifact_budget_remaining_bytes": decision.artifact_budget_remaining_bytes,
        "artifact_budget_decision": decision.decision,
        "candidate_spool_disabled_reason": decision.disabled_reason,
        "candidate_spool_warning": decision.warning,
        "graph_db_claimable": graph_db_claimable,
        "runtime_sidecar_ready": runtime_sidecar_ready,
        "index_exit_status_reason": "candidate_spool_required_budget_exceeded",
        "public_claim": false,
    })
}

fn apply_candidate_spool_budget_json_fields(
    object: &mut serde_json::Map<String, Value>,
    summary: &IndexSummary,
    decision: &CandidateSpoolBudgetDecision,
    spool_footprint_bytes: u64,
    graph_db_claimable: bool,
    runtime_sidecar_ready: bool,
) {
    let spool = summary.candidate_spool.as_ref();
    let disabled_reason = spool
        .and_then(|spool| spool.candidate_spool_disabled_reason.clone())
        .or_else(|| decision.disabled_reason.clone());
    let warning = spool
        .and_then(|spool| spool.candidate_spool_warning.clone())
        .or_else(|| decision.warning.clone());
    let artifact_decision = spool
        .and_then(|spool| spool.artifact_budget_decision.clone())
        .unwrap_or_else(|| decision.decision.clone());
    let candidate_spool_truncated = spool
        .map(|spool| spool.candidate_spool_truncated)
        .unwrap_or(false)
        || matches!(
            artifact_decision.as_str(),
            "candidate_spool_truncated_to_fit_artifact_budget"
                | "candidate_spool_exceeds_remaining_budget"
                | "candidate_spool_disabled_budget_exceeded"
        );
    let exit_reason = if disabled_reason.is_some() {
        "indexed_candidate_spool_disabled_or_unavailable"
    } else if warning.is_some() || candidate_spool_truncated {
        "indexed_candidate_spool_warning"
    } else {
        "indexed"
    };
    let fields = [
        (
            "candidate_spool_policy",
            json!(spool
                .map(|spool| spool.candidate_spool_policy.as_str())
                .unwrap_or_else(|| decision.policy.as_str())),
        ),
        (
            "candidate_spool_required",
            json!(spool
                .map(|spool| spool.candidate_spool_required)
                .unwrap_or(decision.required)),
        ),
        (
            "candidate_spool_budget_bytes",
            json!(spool
                .map(|spool| spool.candidate_spool_budget_bytes)
                .or(decision.budget_bytes)
                .unwrap_or(0)),
        ),
        (
            "candidate_spool_written_bytes",
            json!(spool_footprint_bytes),
        ),
        (
            "candidate_spool_truncated",
            json!(candidate_spool_truncated),
        ),
        ("candidate_spool_disabled_reason", json!(disabled_reason)),
        ("candidate_spool_warning", json!(warning)),
        (
            "artifact_budget_remaining_bytes",
            json!(spool
                .and_then(|spool| spool.artifact_budget_remaining_bytes)
                .or(decision.artifact_budget_remaining_bytes)),
        ),
        ("artifact_budget_decision", json!(artifact_decision)),
        ("index_exit_status_reason", json!(exit_reason)),
        ("graph_db_claimable", json!(graph_db_claimable)),
        ("runtime_sidecar_ready", json!(runtime_sidecar_ready)),
    ];
    for (key, value) in fields {
        object.insert(key.to_string(), value);
    }
    object.insert("candidate_spool_budget".to_string(), decision.to_json());
    let mirror = [
        "candidate_spool_policy",
        "candidate_spool_required",
        "candidate_spool_budget_bytes",
        "candidate_spool_written_bytes",
        "candidate_spool_truncated",
        "candidate_spool_disabled_reason",
        "candidate_spool_warning",
        "artifact_budget_remaining_bytes",
        "artifact_budget_decision",
        "index_exit_status_reason",
        "graph_db_claimable",
        "runtime_sidecar_ready",
    ];
    let mirror_values = mirror
        .iter()
        .filter_map(|key| object.get(*key).cloned().map(|value| (*key, value)))
        .collect::<Vec<_>>();
    if let Some(summary_value) = object.get_mut("summary").and_then(Value::as_object_mut) {
        for (key, value) in mirror_values {
            summary_value.insert(key.to_string(), value);
        }
    }
}

fn parse_index_command_options(
    args: &[String],
) -> Result<
    (
        String,
        Option<PathBuf>,
        IndexOptions,
        IndexJsonOutputMode,
        storage_budget::StorageBudgetOptions,
        VectorIndexCliOptions,
    ),
    String,
> {
    let mut repo = None;
    let mut db = None;
    let mut vector_index = VectorIndexCliOptions::default();
    let mut options = IndexOptions::default();
    let mut storage_budget = storage_budget::StorageBudgetOptions::normal_self_use();
    let mut output_mode = IndexJsonOutputMode::Concise;
    let mut agent_json_requested = false;
    let mut audit_json_requested = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--profile" => {
                options.profile = true;
                options.json = true;
            }
            "--json" => options.json = true,
            "--agent-json" | "--agent_json" => {
                options.json = true;
                output_mode = IndexJsonOutputMode::Agent;
                agent_json_requested = true;
            }
            "--concise" => {
                options.json = true;
                if !agent_json_requested {
                    output_mode = IndexJsonOutputMode::Concise;
                }
            }
            "--audit-json" | "--audit_json" => {
                options.json = true;
                output_mode = IndexJsonOutputMode::Audit;
                audit_json_requested = true;
            }
            "--verbose" => {
                options.profile = true;
                options.json = true;
                output_mode = IndexJsonOutputMode::Audit;
                audit_json_requested = true;
            }
            "--db" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--db requires a path".to_string());
                };
                db = Some(PathBuf::from(raw));
                options.db_lifecycle.explicit_db_path = true;
            }
            "--fresh" | "--rebuild" => {
                options.db_lifecycle.policy = DbLifecyclePolicy::FreshRebuild;
            }
            "--incremental" => {
                options.db_lifecycle.policy = DbLifecyclePolicy::IncrementalRequired;
            }
            "--fail-on-db-problem" => {
                options.db_lifecycle.policy = DbLifecyclePolicy::FailOnDbProblem;
            }
            "--allow-stale-reuse" => {
                options.db_lifecycle.policy = DbLifecyclePolicy::DiagnosticStaleReuse;
            }
            "--build-vector-index" | "--vector-runtime-sidecar" | "--vector_runtime_sidecar" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--build-vector-index requires a path".to_string());
                };
                vector_index.runtime_path = Some(PathBuf::from(raw));
                options.json = true;
            }
            "--vector-audit-artifact" | "--vector_audit_artifact" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--vector-audit-artifact requires a path".to_string());
                };
                vector_index.audit_artifact_path = Some(PathBuf::from(raw));
                options.json = true;
            }
            "--no-vector-audit" | "--no_vector_audit" => {
                vector_index.no_audit = true;
            }
            "--vector-artifact-format" | "--vector_artifact_format" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--vector-artifact-format requires a value".to_string());
                };
                vector_index.runtime_format = raw.parse::<VectorChunkArtifactFormat>()?;
                options.json = true;
            }
            "--candidate-spool" | "--emit-candidate-spool" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--candidate-spool requires a path".to_string());
                };
                options.candidate_spool_path = Some(PathBuf::from(raw));
                options.json = true;
            }
            "--candidate-spool-policy" | "--candidate_spool_policy" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err(
                        "--candidate-spool-policy requires off, bounded, or audit".to_string()
                    );
                };
                options.candidate_spool_policy = raw.parse::<CandidateSpoolPolicy>()?;
                options.json = true;
            }
            "--candidate-spool-required" | "--candidate_spool_required" => {
                options.candidate_spool_required = true;
                options.json = true;
            }
            "--candidate-spool-query-index" | "--candidate_spool_query_index" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--candidate-spool-query-index requires yes or no".to_string());
                };
                options.candidate_spool_query_index =
                    parse_index_bool(raw, "--candidate-spool-query-index")?;
                options.json = true;
            }
            "--candidate-spool-max-mib" | "--candidate_spool_max_mib" => {
                index += 1;
                let mib =
                    parse_index_positive_f64_arg(args.get(index), "--candidate-spool-max-mib")?;
                options.candidate_spool_caps.global_max_bytes =
                    (mib * 1024.0 * 1024.0).round() as usize;
            }
            "--candidate-spool-max-bytes" | "--candidate_spool_max_bytes" => {
                index += 1;
                options.candidate_spool_caps.global_max_bytes =
                    parse_index_usize_arg(args.get(index), "--candidate-spool-max-bytes")?;
            }
            "--candidate-spool-max-records" | "--candidate_spool_max_records" => {
                index += 1;
                options.candidate_spool_caps.global_max_records =
                    parse_index_usize_arg(args.get(index), "--candidate-spool-max-records")?;
            }
            "--candidate-spool-per-file-max-records" | "--candidate_spool_per_file_max_records" => {
                index += 1;
                options.candidate_spool_caps.per_file_max_records = parse_index_usize_arg(
                    args.get(index),
                    "--candidate-spool-per-file-max-records",
                )?;
            }
            "--candidate-spool-per-dir-soft-cap" | "--candidate_spool_per_dir_soft_cap" => {
                index += 1;
                options.candidate_spool_caps.per_top_level_dir_soft_cap =
                    parse_index_usize_arg(args.get(index), "--candidate-spool-per-dir-soft-cap")?;
            }
            "--candidate-spool-max-snippet-bytes" | "--candidate_spool_max_snippet_bytes" => {
                index += 1;
                options.candidate_spool_caps.max_snippet_bytes =
                    parse_index_usize_arg(args.get(index), "--candidate-spool-max-snippet-bytes")?;
            }
            "--candidate-spool-max-snippets-per-file"
            | "--candidate_spool_max_snippets_per_file" => {
                index += 1;
                options.candidate_spool_caps.max_snippets_per_file_packet = parse_index_usize_arg(
                    args.get(index),
                    "--candidate-spool-max-snippets-per-file",
                )?;
            }
            "--candidate-spool-max-symbols-per-file" | "--candidate_spool_max_symbols_per_file" => {
                index += 1;
                options.candidate_spool_caps.max_symbols_per_file_packet = parse_index_usize_arg(
                    args.get(index),
                    "--candidate-spool-max-symbols-per-file",
                )?;
            }
            "--early-candidates" => {
                options.json = true;
            }
            "--workers" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--workers requires a value".to_string());
                };
                let workers = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --workers value: {raw}"))?;
                if workers == 0 {
                    return Err("--workers must be at least 1".to_string());
                }
                options.worker_count = Some(workers);
            }
            "--storage-mode" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--storage-mode requires a value".to_string());
                };
                options.storage_mode = raw.parse::<StorageMode>()?;
            }
            "--build-mode" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--build-mode requires a value".to_string());
                };
                options.build_mode = raw.parse::<IndexBuildMode>()?;
            }
            "--include-ignored" => options.scope.include_ignored = true,
            "--include" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--include requires a pattern".to_string());
                };
                options.scope.include_patterns.push(raw.to_string());
            }
            "--exclude" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--exclude requires a pattern".to_string());
                };
                options.scope.exclude_patterns.push(raw.to_string());
            }
            "--no-default-excludes" => options.scope.no_default_excludes = true,
            "--respect-gitignore" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--respect-gitignore requires true or false".to_string());
                };
                options.scope.respect_gitignore = parse_index_bool(raw, "--respect-gitignore")?;
            }
            "--explain-scope" => options.scope.explain_scope = true,
            "--print-included" => options.scope.print_included = true,
            "--print-excluded" => options.scope.print_excluded = true,
            value => {
                if storage_budget::parse_storage_budget_flag(args, &mut index, &mut storage_budget)?
                {
                    index += 1;
                    continue;
                }
                if value.starts_with('-') {
                    return Err(format!("unknown index option: {value}"));
                }
                if repo.is_some() {
                    return Err(format!("unexpected index argument: {value}"));
                }
                repo = Some(value.to_string());
            }
        }
        index += 1;
    }
    if !agent_json_requested
        && (audit_json_requested || options.profile || options.scope.has_print_or_explain())
    {
        output_mode = IndexJsonOutputMode::Audit;
    }
    if vector_index.no_audit && vector_index.audit_artifact_path.is_some() {
        return Err(
            "--no-vector-audit cannot be combined with --vector-audit-artifact".to_string(),
        );
    }
    if vector_index.audit_artifact_path.is_some() && vector_index.runtime_path.is_none() {
        return Err(
            "--vector-audit-artifact requires --build-vector-index or --vector-runtime-sidecar"
                .to_string(),
        );
    }
    Ok((
        repo.ok_or_else(|| {
            "Usage: codegraph-mcp index <repo> [--db <path>] [--fresh|--rebuild] [--incremental] [--fail-on-db-problem] [--allow-stale-reuse] [--build-vector-index <runtime_path>|--vector-runtime-sidecar <runtime_path>] [--vector-audit-artifact <audit_path>] [--no-vector-audit] [--vector-artifact-format compact_json|pretty_json] [--candidate-spool <path>] [--candidate-spool-policy off|bounded|audit] [--candidate-spool-max-mib <n>] [--candidate-spool-max-bytes <n>] [--candidate-spool-max-records <n>] [--candidate-spool-required] [--candidate-spool-query-index yes|no] [--early-candidates] [--profile] [--json|--agent-json|--audit-json] [--verbose] [--workers <n>] [--storage-mode <proof|audit|debug>] [--build-mode <proof-build-only|proof-build-plus-validation>] [--max-db-mib <n>] [--max-artifacts-mib <n>] [--min-free-disk-gib <n>] [--extended] [--stress-corpus <name>] [--include-ignored] [--include <pattern>] [--exclude <pattern>] [--no-default-excludes] [--respect-gitignore <true|false>] [--explain-scope] [--print-included] [--print-excluded]\nScope: default repo scope plus explicit include overrides; --include is not a restrictive only-these-globs filter.".to_string()
        })?,
        db,
        options,
        output_mode,
        storage_budget,
        vector_index,
    ))
}

fn generate_large_synthetic_repo(root: &Path, files: usize) -> std::io::Result<()> {
    let generated_dir = root.join("src").join("generated");
    fs::create_dir_all(&generated_dir)?;
    fs::write(
        root.join("package.json"),
        "{\n  \"name\": \"codegraph-index-speed-fixture\",\n  \"private\": true,\n  \"type\": \"module\"\n}\n",
    )?;
    fs::write(
        root.join("tsconfig.json"),
        "{\n  \"compilerOptions\": { \"target\": \"ES2022\", \"module\": \"ESNext\", \"strict\": true }\n}\n",
    )?;
    for index in 0..files {
        let next = (index + 1) % files;
        let previous = if index == 0 { files - 1 } else { index - 1 };
        let source = format!(
            "import {{ service{next} }} from './file_{next}';\n\
             export interface Payload{index} {{ value: number; label: string; }}\n\
             export class Worker{index} {{\n\
               run(input: Payload{index}) {{\n\
                 const nextValue = mutate{index}(input.value);\n\
                 return service{next}({{ value: nextValue, label: input.label }});\n\
               }}\n\
             }}\n\
             export function service{index}(input: Payload{index}) {{\n\
               const local = input.value + {index};\n\
               audit{index}(local);\n\
               return helper{index}(local) + {previous};\n\
             }}\n\
             export function mutate{index}(value: number) {{\n\
               let current = value;\n\
               current = current + 1;\n\
               return current;\n\
             }}\n\
             export function helper{index}(value: number) {{ return value; }}\n\
             export function audit{index}(value: number) {{ return value > {previous}; }}\n"
        );
        fs::write(generated_dir.join(format!("file_{index}.ts")), source)?;
    }
    Ok(())
}

fn index_summary_json(summary: &IndexSummary) -> Result<Value, String> {
    let mut value = serde_json::to_value(summary).map_err(|error| error.to_string())?;
    let Some(object) = value.as_object_mut() else {
        return Err("failed to encode index summary".to_string());
    };
    object.insert("status".to_string(), json!("indexed"));
    object.insert(
        "telemetry".to_string(),
        index_runtime_telemetry_json(summary),
    );
    object.insert(
        "indexing_durability".to_string(),
        index_batch_durability_json(summary),
    );
    Ok(value)
}

fn index_summary_concise_json(summary: &IndexSummary, wall_ms: f64) -> Result<Value, String> {
    let counts = index_graph_counts(summary);
    let lifecycle = index_lifecycle_summary_json(summary);
    Ok(json!({
        "schema_version": AGENT_JSON_SCHEMA_VERSION,
        "status": "indexed",
        "output_mode": IndexJsonOutputMode::Concise.as_str(),
        "repo_root": summary.repo_root,
        "db_path": summary.db_path,
        "lifecycle": lifecycle,
        "db_lifecycle": lifecycle,
        "claimable": index_summary_claimable(summary),
        "diagnostic_only": !index_summary_claimable(summary),
        "build_mode": summary.build_mode,
        "storage_policy": summary.storage_policy,
        "files_seen": summary.files_seen,
        "files_read": summary.files_read,
        "files_hashed": summary.files_hashed,
        "files_indexed": summary.files_indexed,
        "files_parsed": summary.files_parsed,
        "files_skipped": summary.files_skipped,
        "files_metadata_unchanged": summary.files_metadata_unchanged,
        "entities": summary.entities,
        "edges": summary.edges,
        "source_spans": counts.source_spans,
        "counts": {
            "files": index_file_counts_json(summary),
            "graph": index_graph_counts_json(summary, counts.source_spans),
            "batches": {
                "total": summary.batches_total,
                "completed": summary.batches_completed,
            }
        },
        "timing": index_timing_summary_json(summary, wall_ms),
        "telemetry": index_runtime_telemetry_json(summary),
        "indexing_durability": index_batch_durability_json(summary),
        "graph_output_budgets": index_concise_graph_output_budgets_json(summary),
        "warnings_count": index_warning_count(summary),
        "issue_counts": summary.issue_counts,
        "issues_count": summary.issues.len(),
        "scope": index_scope_summary_json(summary),
        "candidate_spool": summary.candidate_spool.clone(),
        "candidate_spool_status": summary
            .candidate_spool
            .as_ref()
            .map(|spool| spool.candidate_spool_status.clone())
            .unwrap_or_else(|| "no_spool".to_string()),
        "candidate_spool_path": summary
            .candidate_spool
            .as_ref()
            .map(|spool| spool.candidate_spool_path.clone()),
        "spooled_total_chunks": summary
            .candidate_spool
            .as_ref()
            .map(|spool| spool.spooled_total_chunks)
            .unwrap_or(0),
        "candidate_only": true,
        "graph_proof": false,
        "artifact_freshness": index_artifact_freshness_json(summary),
        "output_contract": {
            "scope_examples_included": false,
            "audit_payload_included": false,
            "audit_flags": ["--audit-json", "--verbose", "--explain-scope", "--print-included", "--print-excluded"],
            "size_target_bytes": INDEX_CONCISE_JSON_SIZE_TARGET_BYTES,
        },
    }))
}

fn index_summary_agent_json(summary: &IndexSummary, wall_ms: f64) -> Result<Value, String> {
    let counts = index_graph_counts(summary);
    let warnings = index_warning_messages(summary);
    let status = if warnings.is_empty() { "ok" } else { "warning" };
    let mut summary_object = serde_json::Map::new();
    summary_object.insert("repo".to_string(), json!(summary.repo_root));
    summary_object.insert("db_path".to_string(), json!(summary.db_path));
    summary_object.insert("build_mode".to_string(), json!(summary.build_mode));
    summary_object.insert("storage_policy".to_string(), json!(summary.storage_policy));
    summary_object.insert("files_seen".to_string(), json!(summary.files_seen));
    summary_object.insert("files_indexed".to_string(), json!(summary.files_indexed));
    summary_object.insert("files_parsed".to_string(), json!(summary.files_parsed));
    summary_object.insert("files_skipped".to_string(), json!(summary.files_skipped));
    summary_object.insert(
        "files_metadata_unchanged".to_string(),
        json!(summary.files_metadata_unchanged),
    );
    summary_object.insert("entities".to_string(), json!(summary.entities));
    summary_object.insert("edges".to_string(), json!(summary.edges));
    if let Some(source_spans) = counts.source_spans {
        summary_object.insert("source_spans".to_string(), json!(source_spans));
    }
    summary_object.insert("duration_ms".to_string(), json!(wall_ms));
    summary_object.insert(
        "warnings_count".to_string(),
        json!(index_warning_count(summary)),
    );
    summary_object.insert("scope".to_string(), index_scope_summary_agent_json(summary));
    summary_object.insert(
        "candidate_spool".to_string(),
        serde_json::to_value(summary.candidate_spool.clone()).map_err(|error| error.to_string())?,
    );
    summary_object.insert(
        "candidate_spool_status".to_string(),
        json!(summary
            .candidate_spool
            .as_ref()
            .map(|spool| spool.candidate_spool_status.clone())
            .unwrap_or_else(|| "no_spool".to_string())),
    );
    summary_object.insert("candidate_only".to_string(), json!(true));
    summary_object.insert("graph_proof".to_string(), json!(false));
    summary_object.insert(
        "artifact_freshness".to_string(),
        index_artifact_freshness_json(summary),
    );
    summary_object.insert(
        "indexing_durability".to_string(),
        index_batch_durability_agent_json(summary),
    );
    summary_object.insert(
        "graph_output_budgets".to_string(),
        index_graph_output_budgets_agent_json(summary),
    );

    Ok(json!({
        "schema_name": "index_agent_json",
        "schema_version": AGENT_JSON_SCHEMA_VERSION,
        "status": status,
        "command": "index",
        "repo": summary.repo_root,
        "db": summary.db_path,
        "output_mode": IndexJsonOutputMode::Agent.as_str(),
        "lifecycle": index_lifecycle_summary_json(summary),
        "claimable": index_summary_claimable(summary),
        "diagnostic_only": !index_summary_claimable(summary),
        "summary": Value::Object(summary_object),
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
        "indexing_durability": index_batch_durability_agent_json(summary),
        "graph_output_budgets": index_graph_output_budgets_agent_json(summary),
        "warnings": warnings,
        "errors": [],
        "limits": {
            "max_output_bytes_target": INDEX_AGENT_JSON_SIZE_TARGET_BYTES,
        },
    }))
}

#[derive(Debug, Clone, Copy)]
struct IndexGraphCounts {
    source_spans: Option<u64>,
}

fn index_graph_counts(summary: &IndexSummary) -> IndexGraphCounts {
    match graph_counts_for_db(Path::new(&summary.db_path)) {
        Ok((_, _, source_spans)) => IndexGraphCounts {
            source_spans: Some(source_spans),
        },
        Err(_) => IndexGraphCounts { source_spans: None },
    }
}

fn index_summary_claimable(summary: &IndexSummary) -> bool {
    summary
        .db_lifecycle
        .as_ref()
        .map(|lifecycle| lifecycle.claimable)
        .unwrap_or(false)
}

fn index_lifecycle_summary_json(summary: &IndexSummary) -> Value {
    let claimable = index_summary_claimable(summary);
    let mut object = serde_json::Map::new();
    object.insert("claimable".to_string(), json!(claimable));
    object.insert("diagnostic_only".to_string(), json!(!claimable));
    if let Some(lifecycle) = summary.db_lifecycle.as_ref() {
        object.insert("mode".to_string(), json!(lifecycle.mode));
        object.insert("decision".to_string(), json!(lifecycle.decision));
        object.insert(
            "passport_status".to_string(),
            json!(lifecycle.passport_status),
        );
        object.insert("old_db_used".to_string(), json!(lifecycle.old_db_used));
        object.insert(
            "old_db_replaced".to_string(),
            json!(lifecycle.old_db_replaced),
        );
        object.insert(
            "explicit_db_path".to_string(),
            json!(lifecycle.explicit_db_path),
        );
        object.insert("reasons_count".to_string(), json!(lifecycle.reasons.len()));
        if let Some(preflight) = lifecycle.preflight.as_ref() {
            if let Some(kind) = preflight.db_problem_kind.as_deref() {
                object.insert("db_problem_kind".to_string(), json!(kind));
            }
            object.insert(
                "path_access_status".to_string(),
                json!(preflight.path_access_status),
            );
            object.insert(
                "schema_status".to_string(),
                json!(preflight.passport_status),
            );
            object.insert(
                "sidecar_status".to_string(),
                json!(preflight.sidecar_status),
            );
            if let Some(schema_version) = preflight.schema_version {
                object.insert("db_schema_version".to_string(), json!(schema_version));
            }
        }
    } else {
        object.insert("decision".to_string(), json!("unknown"));
    }
    Value::Object(object)
}

fn index_file_counts_json(summary: &IndexSummary) -> Value {
    json!({
        "seen": summary.files_seen,
        "walked": summary.files_walked,
        "metadata_unchanged": summary.files_metadata_unchanged,
        "read": summary.files_read,
        "hashed": summary.files_hashed,
        "parsed": summary.files_parsed,
        "indexed": summary.files_indexed,
        "skipped": summary.files_skipped,
        "deleted": summary.files_deleted,
        "renamed": summary.files_renamed,
        "stale_deleted": summary.stale_files_deleted,
        "failed_deleted": summary.failed_files_deleted,
        "parse_errors": summary.parse_errors,
        "syntax_errors": summary.syntax_errors,
    })
}

fn index_graph_counts_json(summary: &IndexSummary, source_spans: Option<u64>) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("entities".to_string(), json!(summary.entities));
    object.insert("edges".to_string(), json!(summary.edges));
    object.insert(
        "duplicate_edges_upserted".to_string(),
        json!(summary.duplicate_edges_upserted),
    );
    if let Some(source_spans) = source_spans {
        object.insert("source_spans".to_string(), json!(source_spans));
    }
    Value::Object(object)
}

fn index_timing_summary_json(summary: &IndexSummary, wall_ms: f64) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("wall_ms".to_string(), json!(wall_ms));
    object.insert(
        "profile_available".to_string(),
        json!(summary.profile.is_some()),
    );
    object.insert("build_profile".to_string(), json!(build_profile()));
    object.insert("binary_profile".to_string(), json!(build_profile()));
    object.insert(
        "debug_assertions".to_string(),
        json!(cfg!(debug_assertions)),
    );
    object.insert("debug_timing_not_used_for_claim".to_string(), json!(true));
    object.insert(
        "profile_timing_claim_scope".to_string(),
        json!("local_diagnostic_only"),
    );
    if let Some(profile) = summary.profile.as_ref() {
        object.insert("total_wall_ms".to_string(), json!(profile.total_wall_ms));
        object.insert(
            "file_discovery_ms".to_string(),
            json!(profile.file_discovery_ms),
        );
        object.insert("parse_ms".to_string(), json!(profile.parse_ms));
        object.insert("extraction_ms".to_string(), json!(profile.extraction_ms));
        object.insert("db_write_ms".to_string(), json!(profile.db_write_ms));
        object.insert(
            "db_write_measurement".to_string(),
            json!(profile.db_write_measurement),
        );
        object.insert(
            "fts_search_index_ms".to_string(),
            json!(profile.fts_search_index_ms),
        );
        object.insert(
            "fts_search_index_measurement".to_string(),
            json!(profile.fts_search_index_measurement),
        );
        object.insert("worker_count".to_string(), json!(profile.worker_count));
    }
    object.insert(
        "memory".to_string(),
        json!(summary
            .profile
            .as_ref()
            .and_then(|profile| profile.memory_bytes)
            .map(|bytes| bytes.to_string())
            .unwrap_or_else(|| "unknown".to_string())),
    );
    object.insert(
        "memory_bytes".to_string(),
        summary
            .profile
            .as_ref()
            .and_then(|profile| profile.memory_bytes)
            .map(Value::from)
            .unwrap_or(Value::Null),
    );
    object.insert(
        "memory_measured".to_string(),
        json!(summary
            .profile
            .as_ref()
            .map(|profile| profile.memory_measured)
            .unwrap_or(false)),
    );
    object.insert(
        "memory_status".to_string(),
        json!(summary
            .profile
            .as_ref()
            .map(|profile| profile.memory_status.as_str())
            .unwrap_or("unknown")),
    );
    object.insert(
        "memory_measurement_kind".to_string(),
        json!(summary
            .profile
            .as_ref()
            .map(|profile| profile.memory_measurement_kind.as_str())
            .unwrap_or("not_measured")),
    );
    if summary.profile.is_some() {
        object.insert(
            "timing_fields".to_string(),
            index_profile_timing_fields_json(summary),
        );
    }
    Value::Object(object)
}

fn index_runtime_telemetry_json(summary: &IndexSummary) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("build_profile".to_string(), json!(build_profile()));
    object.insert("binary_profile".to_string(), json!(build_profile()));
    object.insert(
        "debug_assertions".to_string(),
        json!(cfg!(debug_assertions)),
    );
    object.insert("debug_timing_not_used_for_claim".to_string(), json!(true));
    object.insert(
        "profile_timing_claim_scope".to_string(),
        json!("local_diagnostic_only"),
    );
    object.insert(
        "profile_available".to_string(),
        json!(summary.profile.is_some()),
    );
    object.insert(
        "memory".to_string(),
        json!(summary
            .profile
            .as_ref()
            .and_then(|profile| profile.memory_bytes)
            .map(|bytes| bytes.to_string())
            .unwrap_or_else(|| "unknown".to_string())),
    );
    object.insert(
        "memory_bytes".to_string(),
        summary
            .profile
            .as_ref()
            .and_then(|profile| profile.memory_bytes)
            .map(Value::from)
            .unwrap_or(Value::Null),
    );
    object.insert(
        "memory_measured".to_string(),
        json!(summary
            .profile
            .as_ref()
            .map(|profile| profile.memory_measured)
            .unwrap_or(false)),
    );
    object.insert(
        "memory_status".to_string(),
        json!(summary
            .profile
            .as_ref()
            .map(|profile| profile.memory_status.as_str())
            .unwrap_or("unknown")),
    );
    object.insert(
        "memory_measurement_kind".to_string(),
        json!(summary
            .profile
            .as_ref()
            .map(|profile| profile.memory_measurement_kind.as_str())
            .unwrap_or("not_measured")),
    );
    object.insert(
        "timing_truth_status".to_string(),
        json!(if summary.profile.is_some() {
            "measured_or_explicit_unknown"
        } else {
            "profile_not_requested"
        }),
    );
    if summary.profile.is_some() {
        object.insert(
            "timing_fields".to_string(),
            index_profile_timing_fields_json(summary),
        );
    }
    Value::Object(object)
}

fn runtime_telemetry_unknown_json() -> Value {
    json!({
        "build_profile": build_profile(),
        "binary_profile": build_profile(),
        "debug_assertions": cfg!(debug_assertions),
        "profile_available": false,
        "memory": "unknown",
        "memory_bytes": Value::Null,
        "memory_measured": false,
        "memory_status": "unknown",
        "memory_measurement_kind": "not_measured",
        "timing_truth_status": "not_applicable",
    })
}

fn index_profile_timing_fields_json(summary: &IndexSummary) -> Value {
    let profile = summary.profile.as_ref();
    json!({
        "db_write": timing_field_from_profile_total(
            profile,
            profile.map(|profile| profile.db_write_ms as f64),
            profile
                .map(|profile| profile.db_write_measurement.as_str())
                .unwrap_or("profile_not_requested"),
            "measured aggregate of SQL write spans; not FTS build or commit"
        ),
        "fts_build": timing_field_from_profile_total(
            profile,
            profile.map(|profile| profile.fts_search_index_ms as f64),
            profile
                .map(|profile| profile.fts_search_index_measurement.as_str())
                .unwrap_or("profile_not_requested"),
            "measured from fts_build span when present"
        ),
        "transaction_commit": timing_field_from_span(profile, "transaction_commit", "measured commit span"),
        "reducer": timing_field_from_span(profile, "reducer", "measured reducer span"),
        "edge_insert": timing_field_from_span(profile, "edge_insert", "measured local edge insert span"),
        "proof_edge_insert": timing_field_from_span(profile, "proof_edge_insert", "measured SQLite edge insert span"),
        "dictionary_lookup_insert": timing_field_from_span(profile, "dictionary_lookup_insert", "measured shared dictionary lookup/insert span"),
        "candidate_spool_build": timing_field_from_span(profile, "candidate_spool_build", "measured candidate spool chunk build/write span when candidate spool is active"),
        "vector_sidecar_build": timing_field_from_span(profile, "vector_runtime_sidecar_build", "measured runtime vector sidecar build span when requested"),
        "audit_artifact_build": timing_field_from_span(profile, "vector_audit_artifact_build", "measured audit artifact write span when requested"),
        "profile_total": timing_field_from_profile_total(
            profile,
            profile.map(|profile| profile.total_wall_ms as f64),
            if profile.is_some() { "measured_wall_clock" } else { "profile_not_requested" },
            "index profile total wall clock"
        ),
    })
}

fn timing_field_from_profile_total(
    profile: Option<&IndexProfile>,
    elapsed_ms: Option<f64>,
    measurement: &str,
    note: &str,
) -> Value {
    match (profile, elapsed_ms) {
        (Some(_), Some(elapsed_ms)) => json!({
            "status": "measured",
            "elapsed_ms": elapsed_ms,
            "measurement": measurement,
            "note": note,
        }),
        (Some(_), None) => json!({
            "status": "unknown",
            "elapsed_ms": Value::Null,
            "measurement": measurement,
            "note": note,
        }),
        (None, _) => json!({
            "status": "profile_not_requested",
            "elapsed_ms": Value::Null,
            "measurement": "profile_not_requested",
            "note": note,
        }),
    }
}

fn timing_field_from_span(profile: Option<&IndexProfile>, span_name: &str, note: &str) -> Value {
    if profile.is_none() {
        return json!({
            "status": "profile_not_requested",
            "elapsed_ms": Value::Null,
            "measurement": "profile_not_requested",
            "note": note,
        });
    }
    let elapsed_ms = profile_span_ms(profile, span_name);
    let count = profile_span_count(profile, span_name);
    if count == 0 {
        return json!({
            "status": "unknown_or_not_run",
            "elapsed_ms": Value::Null,
            "measurement": span_name,
            "note": note,
        });
    }
    json!({
        "status": "measured",
        "elapsed_ms": elapsed_ms,
        "count": count,
        "measurement": span_name,
        "note": note,
    })
}

fn index_warning_count(summary: &IndexSummary) -> usize {
    summary
        .scope
        .as_ref()
        .map(|scope| scope.warnings)
        .unwrap_or_default()
        + summary.issues.len()
        + summary.parse_errors
        + summary.syntax_errors
}

fn index_warning_messages(summary: &IndexSummary) -> Vec<Value> {
    let mut warnings = Vec::new();
    if let Some(scope) = summary.scope.as_ref() {
        if scope.warnings > 0 {
            warnings.push(json!({
                "code": "scope_warnings",
                "message": format!("index scope produced {} warning(s)", scope.warnings),
                "severity": "warning",
            }));
        }
    }
    if summary.parse_errors > 0 {
        warnings.push(json!({
            "code": "parse_errors",
            "message": format!("{} file(s) had parse errors", summary.parse_errors),
            "severity": "warning",
        }));
    }
    if summary.syntax_errors > 0 {
        warnings.push(json!({
            "code": "syntax_errors",
            "message": format!("{} syntax error(s) were observed", summary.syntax_errors),
            "severity": "warning",
        }));
    }
    if !summary.issues.is_empty() {
        warnings.push(json!({
            "code": "index_issues",
            "message": format!("{} index issue(s) were recorded", summary.issues.len()),
            "severity": "warning",
        }));
    }
    warnings.truncate(8);
    warnings
}

fn index_scope_summary_json(summary: &IndexSummary) -> Value {
    let Some(scope) = summary.scope.as_ref() else {
        return json!({
            "available": false,
            "scope_policy_kind": SCOPE_POLICY_KIND_DEFAULT_WITH_OVERRIDES,
            "include_semantics": INCLUDE_SEMANTICS_DEFAULT_SCOPE_PLUS_OVERRIDES,
            "include_is_restrictive": false,
            "include_is_override": true,
            "scope_truth_status": SCOPE_TRUTH_STATUS_OVERRIDE_ONLY,
            "examples_included": false,
        });
    };
    json!({
        "available": true,
        "scope_policy_kind": scope.scope_policy_kind,
        "include_semantics": scope.include_semantics,
        "include_is_restrictive": scope.include_is_restrictive,
        "include_is_override": scope.include_is_override,
        "scope_truth_status": scope.scope_truth_status,
        "default_excludes_enabled": scope.default_excludes_enabled,
        "include_ignored": scope.include_ignored,
        "no_default_excludes": scope.no_default_excludes,
        "respect_gitignore": scope.respect_gitignore,
        "include_patterns_count": scope.include_patterns.len(),
        "exclude_patterns_count": scope.exclude_patterns.len(),
        "paths_evaluated": scope.paths_evaluated,
        "files_included": scope.files_included,
        "paths_excluded": scope.paths_excluded,
        "files_excluded": scope.files_excluded,
        "warnings": scope.warnings,
        "included_examples_count": scope.included_examples.len(),
        "excluded_examples_count": scope.excluded_examples.len(),
        "warning_examples_count": scope.warning_examples.len(),
        "directory_prune_decisions_count": scope.directory_prune_decisions.len(),
        "examples_included": false,
    })
}

fn index_scope_summary_agent_json(summary: &IndexSummary) -> Value {
    let Some(scope) = summary.scope.as_ref() else {
        return json!({
            "available": false,
            "include_is_restrictive": false,
            "include_is_override": true,
            "examples_included": false,
        });
    };
    json!({
        "available": true,
        "include_is_restrictive": scope.include_is_restrictive,
        "include_is_override": scope.include_is_override,
        "warnings": scope.warnings,
        "examples_included": false,
    })
}

fn index_artifact_freshness_json(summary: &IndexSummary) -> Value {
    let Some(lifecycle) = summary.db_lifecycle.as_ref() else {
        return json!({
            "status": "unknown",
        });
    };
    let status = if lifecycle.decision.contains("fresh") {
        "fresh"
    } else if lifecycle.old_db_used {
        "warm_reuse"
    } else {
        "unknown"
    };
    json!({
        "status": status,
        "decision": lifecycle.decision,
        "mode": lifecycle.mode,
        "passport_status": lifecycle.passport_status,
        "claimable": lifecycle.claimable,
        "old_db_used": lifecycle.old_db_used,
        "old_db_replaced": lifecycle.old_db_replaced,
        "explicit_db_path": lifecycle.explicit_db_path,
    })
}

fn index_batch_durability_json(summary: &IndexSummary) -> Value {
    let lifecycle = summary.db_lifecycle.as_ref();
    let fresh_temp_db_path = lifecycle.and_then(|lifecycle| lifecycle.fresh_temp_db_path.clone());
    let atomic_temp_publish_used = fresh_temp_db_path.is_some();
    let visible_db_exists = Path::new(&summary.db_path).exists();
    let visible_db_update_claimed = lifecycle
        .map(|lifecycle| !lifecycle.old_db_used || lifecycle.old_db_replaced)
        .unwrap_or(false)
        || summary.batches_completed > 0;
    json!({
        "batch_progress_vocabulary": {
            "processed": "parser/reducer/DB write work for a batch finished, but this does not mean visible production DB durability",
            "staged": "facts are staged in memory or an open transaction",
            "committed": "SQLite transaction commit completed for the current DB file",
            "published": "validated temp DB was atomically renamed into the visible production DB path"
        },
        "batches_processed": summary.batches_completed,
        "batches_completed_legacy_alias_for": "batches_processed",
        "batch_progress_status": "processed_not_durably_committed_until_transaction_commit",
        "resumable_batches_supported": false,
        "batch_durability_scope": "processed/staged batches become durable only at SQLite transaction commit; hidden temp DBs become visible only after atomic publish",
        "processed_batches_are_visible_db_durable": false,
        "atomic_temp_publish_used": atomic_temp_publish_used,
        "visible_db_old_good_until_publish": atomic_temp_publish_used,
        "temp_db_never_claimable": true,
        "temp_db_path": fresh_temp_db_path,
        "visible_db_path": summary.db_path,
        "visible_db_exists": visible_db_exists,
        "temp_db_transaction_committed": atomic_temp_publish_used && visible_db_exists,
        "visible_db_published": if atomic_temp_publish_used { visible_db_exists } else { visible_db_update_claimed },
        "visible_db_updated": visible_db_exists && visible_db_update_claimed,
        "old_db_replaced": lifecycle
            .map(|lifecycle| lifecycle.old_db_replaced)
            .unwrap_or(false),
        "publish_status": if atomic_temp_publish_used && visible_db_exists {
            "published"
        } else if atomic_temp_publish_used
        {
            "not_published"
        } else {
            "not_atomic_temp_publish"
        },
        "visible_db_mutation_claim": "visible DB changes are claimed only after commit or atomic publish events",
    })
}

fn index_batch_durability_agent_json(summary: &IndexSummary) -> Value {
    let lifecycle = summary.db_lifecycle.as_ref();
    let atomic_temp_publish_used = lifecycle
        .and_then(|lifecycle| lifecycle.fresh_temp_db_path.as_ref())
        .is_some();
    let visible_db_exists = Path::new(&summary.db_path).exists();
    let visible_db_update_claimed = lifecycle
        .map(|lifecycle| !lifecycle.old_db_used || lifecycle.old_db_replaced)
        .unwrap_or(false)
        || summary.batches_completed > 0;
    json!({
        "batches_processed": summary.batches_completed,
        "batch_progress_status": "processed_not_durably_committed_until_transaction_commit",
        "resumable_batches_supported": false,
        "processed_batches_are_visible_db_durable": false,
        "atomic_temp_publish_used": atomic_temp_publish_used,
        "temp_db_never_claimable": true,
        "visible_db_published": if atomic_temp_publish_used { visible_db_exists } else { visible_db_update_claimed },
        "visible_db_updated": visible_db_exists && visible_db_update_claimed,
        "old_db_replaced": lifecycle
            .map(|lifecycle| lifecycle.old_db_replaced)
            .unwrap_or(false),
        "publish_status": if atomic_temp_publish_used && visible_db_exists {
            "published"
        } else if atomic_temp_publish_used {
            "not_published"
        } else {
            "not_atomic_temp_publish"
        },
    })
}

/// Concise (`--json`) graph-output-budgets block. Serializes the full summary
/// but drops the verbose `configured` budget-limit sub-object when nothing was
/// degraded, keeping the no-op concise envelope under its size target. The
/// required `claimability_label` and `worker_dispatch_source_clone_policy`
/// fields are always preserved.
fn index_concise_graph_output_budgets_json(summary: &IndexSummary) -> Value {
    let budgets = &summary.graph_output_budgets;
    let mut value = serde_json::to_value(budgets).unwrap_or_else(|_| json!({}));
    let no_degradation = budgets.files_degraded == 0 && budgets.degraded_files.is_empty();
    if no_degradation {
        if let Some(object) = value.as_object_mut() {
            object.remove("configured");
        }
    }
    value
}

fn index_graph_output_budgets_agent_json(summary: &IndexSummary) -> Value {
    let budgets = &summary.graph_output_budgets;
    // Compact-by-default: when nothing was degraded, the ~30 per-budget counters
    // are all zero and only bloat the agent-json envelope past its size target.
    // Emit a minimal "no degradation" summary in that case; the full counter
    // breakdown stays available on the concise/audit (`--json`) surface.
    if budgets.files_degraded == 0 && budgets.degraded_files.is_empty() {
        return json!({
            "claimability_label": budgets.claimability_label.clone(),
            "files_degraded": 0,
            "degraded_files_count": 0,
            "degraded_files": [],
        });
    }
    json!({
        "claimability_label": budgets.claimability_label.clone(),
        "files_degraded": budgets.files_degraded,
        "local_fact_budget_hits": budgets.local_fact_budget_hits,
        "relation_fanout_budget_hits": budgets.relation_fanout_budget_hits,
        "derived_edge_budget_hits": budgets.derived_edge_budget_hits,
        "source_span_budget_hits": budgets.source_span_budget_hits,
        "reducer_edge_budget_hits": budgets.reducer_edge_budget_hits,
        "entity_budget_hits": budgets.entity_budget_hits,
        "edge_budget_hits": budgets.edge_budget_hits,
        "local_read_budget_hits": budgets.local_read_budget_hits,
        "local_write_budget_hits": budgets.local_write_budget_hits,
        "local_flow_budget_hits": budgets.local_flow_budget_hits,
        "callsite_budget_hits": budgets.callsite_budget_hits,
        "argument_budget_hits": budgets.argument_budget_hits,
        "source_bytes_budget_hits": budgets.source_bytes_budget_hits,
        "omitted_local_facts": budgets.omitted_local_facts,
        "omitted_relation_fanout_edges": budgets.omitted_relation_fanout_edges,
        "omitted_derived_edges": budgets.omitted_derived_edges,
        "omitted_source_span_facts": budgets.omitted_source_span_facts,
        "omitted_reducer_edges": budgets.omitted_reducer_edges,
        "omitted_entities": budgets.omitted_entities,
        "omitted_edges": budgets.omitted_edges,
        "omitted_local_reads": budgets.omitted_local_reads,
        "omitted_local_writes": budgets.omitted_local_writes,
        "omitted_local_flows": budgets.omitted_local_flows,
        "omitted_callsites": budgets.omitted_callsites,
        "omitted_arguments": budgets.omitted_arguments,
        "omitted_source_bytes": budgets.omitted_source_bytes,
        "warnings_count": budgets.warnings.len(),
        "degraded_files_count": budgets.degraded_files.len(),
        "degraded_files": budgets.degraded_files,
    })
}

fn detect_tooling(repo_root: &Path) -> Result<Vec<String>, String> {
    let mut detected = BTreeSet::new();
    if repo_root.join("Cargo.toml").exists() {
        detected.insert("rust/cargo".to_string());
    }
    if repo_root.join("package.json").exists() {
        detected.insert("node".to_string());
    }
    if repo_root.join("tsconfig.json").exists() {
        detected.insert("typescript".to_string());
    }
    for file in collect_repo_files(repo_root).map_err(|error| error.to_string())? {
        match file.extension().and_then(|extension| extension.to_str()) {
            Some("rs") => {
                detected.insert("rust".to_string());
            }
            Some("ts" | "tsx") => {
                detected.insert("typescript".to_string());
            }
            Some("js" | "jsx") => {
                detected.insert("javascript".to_string());
            }
            _ => {}
        }
    }
    Ok(detected.into_iter().collect())
}

fn generate_update_integrity_small_repo(root: &Path) -> Result<String, String> {
    if root.exists() {
        fs::remove_dir_all(root).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    fs::write(
        root.join("src").join("service.ts"),
        "export function compute(value: number) { return value + 1; }\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        root.join("src").join("consumer.ts"),
        "import { compute } from './service';\nexport function run() { return compute(41); }\n",
    )
    .map_err(|error| error.to_string())?;
    Ok("src/service.ts".to_string())
}

fn generate_update_integrity_medium_repo(root: &Path, files: usize) -> Result<String, String> {
    if root.exists() {
        fs::remove_dir_all(root).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    let count = files.max(2);
    for index in 0..count {
        let previous = if index == 0 { count - 1 } else { index - 1 };
        fs::write(
            root.join("src").join(format!("module_{index:03}.ts")),
            format!(
                "import {{ value_{previous:03} }} from './module_{previous:03}';\n\
                 export function value_{index:03}(input: number) {{ return value_{previous:03}(input) + {index}; }}\n"
            ),
        )
        .map_err(|error| error.to_string())?;
    }
    Ok("src/module_000.ts".to_string())
}

fn choose_update_integrity_mutation_file(repo: &Path) -> Result<String, String> {
    let preferred = repo
        .join("python")
        .join("autoresearch_utils")
        .join("metrics_tools.py");
    if preferred.exists() {
        return Ok("python/autoresearch_utils/metrics_tools.py".to_string());
    }
    let files = collect_repo_files(repo).map_err(|error| error.to_string())?;
    for file in files {
        if detect_language(&file).is_some() {
            return file
                .strip_prefix(repo)
                .map(|path| path_string(path).replace('\\', "/"))
                .map_err(|error| error.to_string());
        }
    }
    Err(format!(
        "no supported source file found for update-integrity mutation in {}",
        repo.display()
    ))
}

fn update_integrity_mutation_text(path: &Path, iteration: usize) -> String {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("py") => format!(
            "def codegraph_integrity_harness_iteration_{iteration}():\n    return {iteration}\n"
        ),
        Some("rs") => format!(
            "pub fn codegraph_integrity_harness_iteration_{iteration}() -> i32 {{ {iteration} }}\n"
        ),
        Some("go") => format!(
            "func CodegraphIntegrityHarnessIteration{iteration}() int {{ return {iteration} }}\n"
        ),
        _ => format!(
            "export function codegraphIntegrityHarnessIteration{iteration}() {{ return {iteration}; }}\n"
        ),
    }
}

fn update_integrity_seed_step(
    repo: &Path,
    db: &Path,
    mode: UpdateBenchmarkMode,
    seed_db: Option<&Path>,
    artifact_copy_ms: u64,
) -> Result<Value, String> {
    let started = Instant::now();
    let open_start = Instant::now();
    let schema_version = SqliteGraphStore::open_read_only(db)
        .and_then(|store| store.schema_version())
        .map_err(|error| error.to_string())?;
    let artifact_open_ms = elapsed_ms(open_start);
    let hash_start = Instant::now();
    let graph_fact_hash = graph_fact_hash_for_db(db)?;
    let graph_fact_hash_ms = hash_start.elapsed().as_secs_f64() * 1000.0;
    let (entity_count, edge_count, source_span_count, graph_counts_ran) = match mode {
        UpdateBenchmarkMode::Fast => (Value::Null, Value::Null, Value::Null, false),
        UpdateBenchmarkMode::Validated | UpdateBenchmarkMode::Debug => {
            let (entities, edges, source_spans) = graph_counts_for_db(db)?;
            (json!(entities), json!(edges), json!(source_spans), true)
        }
    };
    let integrity_start = Instant::now();
    let (integrity_status, integrity_check_kind) = match mode {
        UpdateBenchmarkMode::Fast => (quick_integrity_status(db), "quick_check_setup"),
        UpdateBenchmarkMode::Validated | UpdateBenchmarkMode::Debug => {
            (full_integrity_status(db), "full_integrity_check_setup")
        }
    };
    let integrity_check_ms = integrity_start.elapsed().as_secs_f64() * 1000.0;
    let lifecycle_status = benchmark_inspection_lifecycle_status(
        repo,
        db,
        "bench.update_integrity.seed",
        Some(StorageMode::Proof),
    );
    let claimable = benchmark_inspection_claimable(&lifecycle_status);
    Ok(json_object(vec![
        ("step", json!("cold_index")),
        ("status", json!("seeded")),
        ("source", json!("seed_db_copy")),
        ("seed_db", json!(seed_db.map(path_string))),
        ("repo_root", json!(path_string(repo))),
        ("wall_ms", json!(elapsed_ms(started))),
        (
            "setup_timings",
            json!({
                "artifact_copy_ms": artifact_copy_ms,
                "artifact_open_ms": artifact_open_ms,
                "schema_validation_ms": artifact_open_ms,
                "graph_fact_hash_ms": graph_fact_hash_ms,
                "validation_ms": integrity_check_ms,
            }),
        ),
        ("schema_version", json!(schema_version)),
        ("files_walked", Value::Null),
        ("files_read", Value::Null),
        ("files_hashed", Value::Null),
        ("files_parsed", Value::Null),
        ("entities_inserted", Value::Null),
        ("edges_inserted", Value::Null),
        ("duplicate_edges_upserted", Value::Null),
        ("transaction_status", json!("seeded_copy_integrity_checked")),
        ("integrity_status", json!(integrity_status)),
        ("integrity_check_kind", json!(integrity_check_kind)),
        ("integrity_check_ms", json!(integrity_check_ms)),
        ("graph_fact_hash", json!(graph_fact_hash)),
        ("graph_fact_hash_ms", json!(graph_fact_hash_ms)),
        ("entity_count", entity_count),
        ("edge_count", edge_count),
        ("source_span_count", source_span_count),
        ("graph_counts_ran", json!(graph_counts_ran)),
        (
            "db_family_size_bytes",
            json!(sqlite_family_size_bytes(db).unwrap_or(0)),
        ),
        ("inspection_read_only", json!(true)),
        ("artifact_mutated_during_inspection", json!(false)),
        ("mutation_capable_operation", json!("seed_db_copy_setup")),
        ("lifecycle_status", lifecycle_status),
        ("claimable", json!(claimable)),
    ]))
}

fn update_integrity_step_from_index(
    step: &str,
    elapsed: Duration,
    summary: &IndexSummary,
    repo: &Path,
    db: &Path,
    mode: UpdateBenchmarkMode,
) -> Result<Value, String> {
    let hash_start = Instant::now();
    let (graph_fact_hash, graph_digest_kind, global_hash_check_ran) = match mode {
        UpdateBenchmarkMode::Fast if step != "cold_index" => (
            incremental_graph_digest_for_db(db)?.unwrap_or_else(|| "unknown".to_string()),
            "incremental_graph_digest",
            false,
        ),
        UpdateBenchmarkMode::Validated | UpdateBenchmarkMode::Debug => {
            (graph_fact_hash_for_db(db)?, "full_graph_fact_hash", true)
        }
        UpdateBenchmarkMode::Fast => (
            graph_fact_hash_for_db(db)?,
            "setup_full_graph_fact_hash",
            true,
        ),
    };
    let graph_fact_hash_ms = hash_start.elapsed().as_secs_f64() * 1000.0;
    let (entity_count, edge_count, source_span_count, graph_counts_ran) = match mode {
        UpdateBenchmarkMode::Fast => (Value::Null, Value::Null, Value::Null, false),
        UpdateBenchmarkMode::Validated | UpdateBenchmarkMode::Debug => {
            let (entities, edges, source_spans) = graph_counts_for_db(db)?;
            (json!(entities), json!(edges), json!(source_spans), true)
        }
    };
    let integrity_start = Instant::now();
    let (integrity_status, integrity_check_kind) = match mode {
        UpdateBenchmarkMode::Fast => (quick_integrity_status(db), "quick_check_post_measurement"),
        UpdateBenchmarkMode::Validated | UpdateBenchmarkMode::Debug => (
            full_integrity_status(db),
            "full_integrity_check_post_measurement",
        ),
    };
    let integrity_check_ms = integrity_start.elapsed().as_secs_f64() * 1000.0;
    let lifecycle_status = benchmark_inspection_lifecycle_status(
        repo,
        db,
        "bench.update_integrity.index_step",
        Some(StorageMode::Proof),
    );
    let claimable = benchmark_inspection_claimable(&lifecycle_status);
    Ok(json_object(vec![
        ("step", json!(step)),
        ("status", json!("ok")),
        ("mode", json!(mode.as_str())),
        (
            "wall_ms",
            json!(elapsed.as_millis().min(u128::from(u64::MAX)) as u64),
        ),
        (
            "timings",
            index_summary_timing_breakdown(summary, graph_fact_hash_ms, integrity_check_ms),
        ),
        ("files_walked", json!(summary.files_walked)),
        ("files_read", json!(summary.files_read)),
        ("files_hashed", json!(summary.files_hashed)),
        ("files_parsed", json!(summary.files_parsed)),
        ("entities_inserted", json!(summary.entities)),
        ("edges_inserted", json!(summary.edges)),
        (
            "duplicate_edges_upserted",
            json!(summary.duplicate_edges_upserted),
        ),
        ("transaction_status", json!("committed")),
        ("integrity_status", json!(integrity_status)),
        ("integrity_check_kind", json!(integrity_check_kind)),
        ("integrity_check_ms", json!(integrity_check_ms)),
        ("graph_fact_hash", json!(graph_fact_hash)),
        ("graph_fact_hash_ms", json!(graph_fact_hash_ms)),
        ("graph_digest_kind", json!(graph_digest_kind)),
        ("global_hash_check_ran", json!(global_hash_check_ran)),
        ("entity_count", entity_count),
        ("edge_count", edge_count),
        ("source_span_count", source_span_count),
        ("graph_counts_ran", json!(graph_counts_ran)),
        (
            "db_family_size_bytes",
            json!(sqlite_family_size_bytes(db).unwrap_or(0)),
        ),
        ("profile", json!(summary.profile.clone())),
        ("inspection_read_only", json!(true)),
        ("artifact_mutated_during_inspection", json!(false)),
        (
            "mutation_capable_operation",
            json!("index_repo_to_db_with_options"),
        ),
        ("lifecycle_status", lifecycle_status),
        ("claimable", json!(claimable)),
    ]))
}

fn update_integrity_step_from_incremental(
    step: &str,
    elapsed: Duration,
    summary: &IncrementalIndexSummary,
    repo: &Path,
    db: &Path,
    mode: UpdateBenchmarkMode,
) -> Result<Value, String> {
    let hash_start = Instant::now();
    let (graph_fact_hash, graph_digest_kind, global_hash_check_ran) = match mode {
        UpdateBenchmarkMode::Fast => (
            incremental_graph_digest_for_db(db)?.unwrap_or_else(|| "unknown".to_string()),
            "incremental_graph_digest",
            false,
        ),
        UpdateBenchmarkMode::Validated | UpdateBenchmarkMode::Debug => {
            (graph_fact_hash_for_db(db)?, "full_graph_fact_hash", true)
        }
    };
    let graph_fact_hash_ms = hash_start.elapsed().as_secs_f64() * 1000.0;
    let (entity_count, edge_count, source_span_count, graph_counts_ran) = match mode {
        UpdateBenchmarkMode::Fast => (Value::Null, Value::Null, Value::Null, false),
        UpdateBenchmarkMode::Validated | UpdateBenchmarkMode::Debug => {
            let (entities, edges, source_spans) = graph_counts_for_db(db)?;
            (json!(entities), json!(edges), json!(source_spans), true)
        }
    };
    let integrity_start = Instant::now();
    let (integrity_status, integrity_check_kind, post_measurement_integrity_check_ran) = match mode
    {
        UpdateBenchmarkMode::Fast => (
            quick_integrity_status(db),
            "quick_check_post_measurement",
            true,
        ),
        UpdateBenchmarkMode::Validated | UpdateBenchmarkMode::Debug => (
            full_integrity_status(db),
            "full_integrity_check_post_measurement",
            true,
        ),
    };
    let integrity_check_ms = integrity_start.elapsed().as_secs_f64() * 1000.0;
    let lifecycle_status = benchmark_inspection_lifecycle_status(
        repo,
        db,
        "bench.update_integrity.incremental_step",
        Some(StorageMode::Proof),
    );
    let claimable = benchmark_inspection_claimable(&lifecycle_status);
    Ok(json_object(vec![
        ("step", json!(step)),
        ("status", json!("ok")),
        ("mode", json!(mode.as_str())),
        (
            "wall_ms",
            json!(elapsed.as_millis().min(u128::from(u64::MAX)) as u64),
        ),
        (
            "timings",
            incremental_summary_timing_breakdown(summary, graph_fact_hash_ms, integrity_check_ms),
        ),
        ("files_walked", json!(summary.files_walked)),
        ("files_read", json!(summary.files_read)),
        ("files_hashed", json!(summary.files_hashed)),
        ("files_parsed", json!(summary.files_parsed)),
        ("entities_inserted", json!(summary.entities)),
        ("edges_inserted", json!(summary.edges)),
        (
            "duplicate_edges_upserted",
            json!(summary.duplicate_edges_upserted),
        ),
        ("transaction_status", json!("committed")),
        ("integrity_status", json!(integrity_status)),
        ("integrity_check_ms", json!(integrity_check_ms)),
        ("integrity_check_kind", json!(integrity_check_kind)),
        (
            "post_measurement_integrity_check_ran",
            json!(post_measurement_integrity_check_ran),
        ),
        ("graph_fact_hash", json!(graph_fact_hash)),
        ("graph_fact_hash_ms", json!(graph_fact_hash_ms)),
        ("graph_digest_kind", json!(graph_digest_kind)),
        ("entity_count", entity_count),
        ("edge_count", edge_count),
        ("source_span_count", source_span_count),
        ("graph_counts_ran", json!(graph_counts_ran)),
        (
            "db_family_size_bytes",
            json!(sqlite_family_size_bytes(db).unwrap_or(0)),
        ),
        ("changed_files", json!(summary.changed_files.clone())),
        ("deleted_fact_files", json!(summary.deleted_fact_files)),
        (
            "dirty_path_evidence_count",
            json!(summary.dirty_path_evidence_count),
        ),
        ("ignored_paths_seen", json!(summary.ignored_paths_seen)),
        (
            "ignored_paths_with_existing_facts",
            json!(summary.ignored_paths_with_existing_facts),
        ),
        (
            "stale_facts_deleted_for_ignored_paths",
            json!(summary.stale_facts_deleted_for_ignored_paths),
        ),
        (
            "deleted_file_facts_removed",
            json!(summary.deleted_file_facts_removed),
        ),
        (
            "path_cleanup_reasons",
            json!(summary.path_cleanup_reasons.clone()),
        ),
        ("global_hash_check_ran", json!(global_hash_check_ran)),
        ("storage_audit_ran", json!(summary.storage_audit_ran)),
        ("integrity_check_ran", json!(summary.integrity_check_ran)),
        ("profile", json!(summary.profile.clone())),
        ("inspection_read_only", json!(true)),
        ("artifact_mutated_during_inspection", json!(false)),
        (
            "mutation_capable_operation",
            json!("update_changed_files_to_db"),
        ),
        ("lifecycle_status", lifecycle_status),
        ("claimable", json!(claimable)),
    ]))
}

fn index_summary_timing_breakdown(
    summary: &IndexSummary,
    graph_fact_hash_ms: f64,
    integrity_check_ms: f64,
) -> Value {
    json!({
        "artifact_open_ms": profile_span_ms(summary.profile.as_ref(), "open_store"),
        "repo_walk_ms": profile_span_ms(summary.profile.as_ref(), "file_discovery")
            .or_else(|| profile_span_ms(summary.profile.as_ref(), "file_walk")),
        "metadata_diff_ms": profile_span_ms(summary.profile.as_ref(), "metadata_diff"),
        "file_read_ms": profile_span_ms(summary.profile.as_ref(), "file_read"),
        "file_hash_ms": profile_span_ms(summary.profile.as_ref(), "file_hash"),
        "db_write_ms": summary.profile.as_ref().map(|profile| profile.db_write_ms as f64),
        "graph_hash_ms": graph_fact_hash_ms,
        "validation_ms": integrity_check_ms,
        "report_generation_ms": Value::Null,
        "json_output_creation_ms": Value::Null,
    })
}

fn incremental_summary_timing_breakdown(
    summary: &IncrementalIndexSummary,
    graph_fact_hash_ms: f64,
    integrity_check_ms: f64,
) -> Value {
    json!({
        "lifecycle_preflight_ms": profile_span_ms(summary.profile.as_ref(), "lifecycle_preflight"),
        "artifact_open_ms": profile_span_ms(summary.profile.as_ref(), "open_store"),
        "metadata_diff_ms": profile_span_ms(summary.profile.as_ref(), "metadata_diff"),
        "file_read_ms": profile_span_ms(summary.profile.as_ref(), "file_read"),
        "file_hash_ms": profile_span_ms(summary.profile.as_ref(), "file_hash"),
        "parse_update_ms": summary.profile.as_ref().map(|profile| profile.total_wall_ms as f64),
        "parse_ms": profile_span_ms(summary.profile.as_ref(), "parse"),
        "extract_ms": profile_span_ms(summary.profile.as_ref(), "extract_entities_and_relations"),
        "stale_delete_ms": profile_span_ms(summary.profile.as_ref(), "stale_fact_delete"),
        "stale_missing_manifest_scan_ms": profile_span_ms(summary.profile.as_ref(), "stale_missing_manifest_scan"),
        "template_invalidation_ms": profile_span_ms(summary.profile.as_ref(), "template_invalidation"),
        "template_insert_update_ms": profile_span_ms(summary.profile.as_ref(), "content_template_upsert"),
        "proof_entity_edge_update_ms": summary.profile.as_ref().map(|profile| profile.db_write_ms as f64),
        "path_evidence_regeneration_ms": profile_span_ms(summary.profile.as_ref(), "path_evidence_insert"),
        "transaction_commit_ms": profile_span_ms(summary.profile.as_ref(), "transaction_commit"),
        "wal_checkpoint_ms": profile_span_ms(summary.profile.as_ref(), "wal_checkpoint"),
        "cache_refresh_ms": profile_span_ms(summary.profile.as_ref(), "cache_refresh"),
        "graph_hash_ms": graph_fact_hash_ms,
        "validation_ms": integrity_check_ms,
        "storage_audit_ms": Value::Null,
        "report_generation_ms": Value::Null,
        "json_output_creation_ms": Value::Null,
    })
}

fn profile_span_ms(profile: Option<&IndexProfile>, name: &str) -> Option<f64> {
    profile
        .and_then(|profile| profile.spans.iter().find(|span| span.name == name))
        .map(|span| span.elapsed_ms)
}

fn profile_span_count(profile: Option<&IndexProfile>, name: &str) -> u64 {
    profile
        .and_then(|profile| profile.spans.iter().find(|span| span.name == name))
        .map(|span| span.count)
        .unwrap_or(0)
}

fn quick_integrity_status(db: &Path) -> String {
    match SqliteGraphStore::open_read_only(db).and_then(|store| store.quick_integrity_gate()) {
        Ok(()) => "ok".to_string(),
        Err(error) => format!("failed: {error}"),
    }
}

fn full_integrity_status(db: &Path) -> String {
    match SqliteGraphStore::open_read_only(db).and_then(|store| store.full_integrity_gate()) {
        Ok(()) => "ok".to_string(),
        Err(error) => format!("failed: {error}"),
    }
}

fn graph_counts_for_db(db: &Path) -> Result<(u64, u64, u64), String> {
    let store = SqliteGraphStore::open_read_only(db).map_err(|error| error.to_string())?;
    Ok((
        store.count_entities().map_err(|error| error.to_string())?,
        store.count_edges().map_err(|error| error.to_string())?,
        store
            .count_source_spans()
            .map_err(|error| error.to_string())?,
    ))
}

fn graph_fact_hash_for_db(db: &Path) -> Result<String, String> {
    let store = SqliteGraphStore::open_read_only(db).map_err(|error| error.to_string())?;
    store.graph_fact_digest().map_err(|error| error.to_string())
}

fn incremental_graph_digest_for_db(db: &Path) -> Result<Option<String>, String> {
    let store = SqliteGraphStore::open_read_only(db).map_err(|error| error.to_string())?;
    store
        .incremental_graph_digest()
        .map_err(|error| error.to_string())
}

fn prime_incremental_graph_digest_for_update(
    db: &Path,
    mutation_file: &str,
    repeat_graph_fact_hash: &str,
) -> Result<(), String> {
    let store = SqliteGraphStore::open(db).map_err(|error| error.to_string())?;
    store
        .transaction(|tx| {
            let now = Some(unix_time_ms());
            tx.replace_repo_graph_digest(repeat_graph_fact_hash, now)?;
            tx.rebuild_file_graph_digest(mutation_file, now)?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}

fn replace_repo_graph_digest_for_harness(db: &Path, graph_fact_hash: &str) -> Result<(), String> {
    let store = SqliteGraphStore::open(db).map_err(|error| error.to_string())?;
    store
        .transaction(|tx| {
            tx.replace_repo_graph_digest(graph_fact_hash, Some(unix_time_ms()))?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}

fn prime_staged_update_repo_index_state(
    db: &Path,
    source_repo: &Path,
    update_repo: &Path,
) -> Result<(), String> {
    let store = SqliteGraphStore::open(db).map_err(|error| error.to_string())?;
    let source_repo = fs::canonicalize(source_repo).unwrap_or_else(|_| source_repo.to_path_buf());
    let update_repo = fs::canonicalize(update_repo).unwrap_or_else(|_| update_repo.to_path_buf());
    let source_repo_id = format!("repo://{}", source_repo.display());
    let update_repo_id = format!("repo://{}", update_repo.display());
    let mut state = if let Some(state) = store
        .get_repo_index_state(&source_repo_id)
        .map_err(|error| error.to_string())?
    {
        state
    } else {
        let (entity_count, edge_count, _) = graph_counts_for_db(db)?;
        RepoIndexState {
            repo_id: source_repo_id,
            repo_root: path_string(&source_repo),
            repo_commit: None,
            schema_version: store.schema_version().map_err(|error| error.to_string())?,
            indexed_at_unix_ms: Some(unix_time_ms()),
            files_indexed: store.count_files().map_err(|error| error.to_string())?,
            entity_count,
            edge_count,
            metadata: Metadata::default(),
        }
    };
    state.repo_id = update_repo_id;
    state.repo_root = path_string(&update_repo);
    state
        .metadata
        .insert("update_harness_staged_repo".to_string(), Value::from(true));
    state.metadata.insert(
        "source_repo_root".to_string(),
        Value::from(path_string(&source_repo)),
    );
    store
        .upsert_repo_index_state(&state)
        .map_err(|error| error.to_string())?;
    if let Some(mut passport) = store.get_db_passport().map_err(|error| error.to_string())? {
        passport.canonical_repo_root = path_string(&update_repo);
        passport.git_remote =
            git_output_for_passport(&update_repo, &["config", "--get", "remote.origin.url"]);
        passport.worktree_root =
            git_output_for_passport(&update_repo, &["rev-parse", "--show-toplevel"])
                .or_else(|| Some(path_string(&update_repo)));
        passport.repo_head = git_head(&update_repo);
        passport.last_completed_run_id = Some(format!(
            "staged-update-prime-{}-{}",
            unix_time_ms(),
            std::process::id()
        ));
        passport.updated_at_unix_ms = unix_time_ms();
        store
            .upsert_db_passport(&passport)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn copy_seed_db(seed_db: &Path, db: &Path) -> Result<(), String> {
    if let Some(parent) = db.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::copy(seed_db, db).map_err(|error| {
        format!(
            "failed to copy seed DB {} to {}: {error}",
            seed_db.display(),
            db.display()
        )
    })?;
    copy_if_exists(
        &sqlite_sidecar_path(seed_db, "wal"),
        &sqlite_sidecar_path(db, "wal"),
    )?;
    copy_if_exists(
        &sqlite_sidecar_path(seed_db, "shm"),
        &sqlite_sidecar_path(db, "shm"),
    )?;
    Ok(())
}

fn stage_update_integrity_mutation_repo(
    source_repo: &Path,
    mutation_file: &str,
    stage_root: &Path,
) -> Result<PathBuf, String> {
    if stage_root.exists() {
        fs::remove_dir_all(stage_root).map_err(|error| {
            format!(
                "failed to clear staged update workspace {}: {error}",
                stage_root.display()
            )
        })?;
    }
    let source_file = source_repo.join(mutation_file);
    let staged_file = stage_root.join(mutation_file);
    if let Some(parent) = staged_file.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::copy(&source_file, &staged_file).map_err(|error| {
        format!(
            "failed to stage mutation file {} to {}: {error}",
            source_file.display(),
            staged_file.display()
        )
    })?;
    Ok(stage_root.to_path_buf())
}

fn git_output_for_passport(repo_root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn deadline_expired(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

fn update_integrity_timeout_report(
    options: &UpdateIntegrityHarnessOptions,
    repos: Vec<Value>,
    stage: &str,
) -> Value {
    json!({
        "schema_version": 1,
        "status": "timeout",
        "verdict": "timeout",
        "phase": PHASE,
        "generated_at_unix_ms": unix_time_ms(),
        "update_mode": options.mode.as_str(),
        "loop_kind": options.loop_kind.as_str(),
        "timeout_ms": options.timeout_ms,
        "timeout_stage": stage,
        "iterations_requested": options.iterations,
        "autoresearch_iterations_requested": options.autoresearch_iterations,
        "workers": options.workers,
        "workdir": path_string(&options.workdir),
        "repos": repos,
        "notes": [
            "Partial benchmark artifact emitted because the internal harness timeout was reached.",
            "Fast-path operation timings exclude setup, validation, graph hash, and report generation fields where those are reported separately."
        ],
    })
}

fn update_integrity_timeout_repo(
    name: &str,
    repo: &Path,
    db: &Path,
    mutation_file: &str,
    mode: UpdateBenchmarkMode,
    loop_kind: UpdateLoopKind,
    iterations: usize,
    timeout_stage: &str,
    repeat_iterations: Vec<Value>,
    iteration_results: Vec<Value>,
    cold: Option<Value>,
) -> Value {
    let repeat = repeat_iterations
        .last()
        .and_then(|iteration| iteration.get("repeat"))
        .cloned()
        .unwrap_or_else(|| json!({"step": "repeat_unchanged_index", "status": "not_completed"}));
    json!({
        "name": name,
        "status": "timeout",
        "repo_path": path_string(repo),
        "db_path": path_string(db),
        "mutation_file": mutation_file,
        "update_mode": mode.as_str(),
        "loop_kind": loop_kind.as_str(),
        "iterations": iterations,
        "timeout_stage": timeout_stage,
        "cold": cold.unwrap_or_else(|| json!({"step": "cold_index", "status": "not_completed"})),
        "repeat_unchanged": repeat,
        "repeat_iterations": repeat_iterations,
        "iteration_results": iteration_results,
        "graph_fact_hash_stable_on_repeat": Value::Null,
        "changed_file_updates_graph_fact_hash": Value::Null,
        "restore_returns_to_repeat_graph_fact_hash": Value::Null,
        "all_integrity_checks_passed": Value::Null,
        "notes": [
            "Partial repo result emitted before all requested iterations completed."
        ],
    })
}

fn update_integrity_error_report(options: &UpdateIntegrityHarnessOptions, error: &str) -> Value {
    json!({
        "schema_version": 1,
        "status": "failed",
        "verdict": "failed",
        "phase": PHASE,
        "generated_at_unix_ms": unix_time_ms(),
        "update_mode": options.mode.as_str(),
        "loop_kind": options.loop_kind.as_str(),
        "timeout_ms": options.timeout_ms,
        "iterations_requested": options.iterations,
        "autoresearch_iterations_requested": options.autoresearch_iterations,
        "workers": options.workers,
        "workdir": path_string(&options.workdir),
        "error": error,
        "repos": [],
        "notes": [
            "Error artifact emitted so benchmark failures do not disappear as missing JSON.",
            "No pass claim is made for failed harness runs."
        ],
    })
}

fn copy_if_exists(source: &Path, target: &Path) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }
    fs::copy(source, target)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn remove_sqlite_family(db: &Path) -> Result<(), String> {
    remove_file_if_exists(db)?;
    remove_file_if_exists(&sqlite_sidecar_path(db, "wal"))?;
    remove_file_if_exists(&sqlite_sidecar_path(db, "shm"))
}

fn sqlite_sidecar_path(db: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}-{suffix}", db.to_string_lossy()))
}

fn sqlite_rollback_journal_path(db: &Path) -> PathBuf {
    PathBuf::from(format!("{}-journal", db.to_string_lossy()))
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn absolute_cli_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path))
    }
}

fn render_update_integrity_harness_markdown(report: &Value) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Autoresearch Update Repro Fix\n\n");
    markdown.push_str(&format!(
        "Verdict: `{}`\n\n",
        report["verdict"].as_str().unwrap_or("unknown")
    ));
    markdown.push_str(&format!(
        "Update mode: `{}`\n\n",
        report["update_mode"].as_str().unwrap_or("unknown")
    ));
    markdown.push_str("| Repo | Status | Iterations | Repeat Hash Stable | Changed Hash | Restore Hash | Integrity |\n");
    markdown.push_str("| --- | --- | ---: | --- | --- | --- | --- |\n");
    if let Some(repos) = report["repos"].as_array() {
        for repo in repos {
            markdown.push_str(&format!(
                "| `{}` | `{}` | {} | `{}` | `{}` | `{}` | `{}` |\n",
                repo["name"].as_str().unwrap_or("unknown"),
                repo["status"].as_str().unwrap_or("unknown"),
                repo["iterations"].as_u64().unwrap_or(0),
                repo["graph_fact_hash_stable_on_repeat"]
                    .as_bool()
                    .unwrap_or(false),
                repo["changed_file_updates_graph_fact_hash"]
                    .as_bool()
                    .unwrap_or(false),
                repo["restore_returns_to_repeat_graph_fact_hash"]
                    .as_bool()
                    .unwrap_or(false),
                repo["all_integrity_checks_passed"]
                    .as_bool()
                    .unwrap_or(false),
            ));
        }
    }
    markdown.push_str("\n## Step Metrics\n\n");
    if let Some(repos) = report["repos"].as_array() {
        for repo in repos {
            markdown.push_str(&format!(
                "### `{}`\n\nMutation file: `{}`\n\n",
                repo["name"].as_str().unwrap_or("unknown"),
                repo["mutation_file"].as_str().unwrap_or("unknown")
            ));
            markdown.push_str("| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |\n");
            markdown.push_str(
                "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n",
            );
            for step in [repo.get("cold"), repo.get("repeat_unchanged")]
                .into_iter()
                .flatten()
            {
                push_update_integrity_step_row(&mut markdown, step);
            }
            if let Some(iterations) = repo["iteration_results"].as_array() {
                for iteration in iterations {
                    if let Some(update) = iteration.get("update") {
                        push_update_integrity_step_row(&mut markdown, update);
                    }
                    if let Some(restore) = iteration.get("restore") {
                        push_update_integrity_step_row(&mut markdown, restore);
                    }
                }
            }
            markdown.push('\n');
        }
    }
    markdown
}

fn push_update_integrity_step_row(markdown: &mut String, step: &Value) {
    markdown.push_str(&format!(
        "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} | `{}` |\n",
        step["step"].as_str().unwrap_or("unknown"),
        step["wall_ms"].as_u64().unwrap_or(0),
        nullable_u64(&step["files_walked"]),
        nullable_u64(&step["files_read"]),
        nullable_u64(&step["files_hashed"]),
        nullable_u64(&step["files_parsed"]),
        nullable_u64(&step["entities_inserted"]),
        nullable_u64(&step["edges_inserted"]),
        nullable_u64(&step["duplicate_edges_upserted"]),
        step["integrity_status"].as_str().unwrap_or("unknown"),
    ));
}

fn nullable_u64(value: &Value) -> String {
    value
        .as_u64()
        .map(|number| number.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn write_if_missing(path: &Path, contents: &str) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, contents).map_err(|error| error.to_string())
}

fn write_text_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, contents).map_err(|error| error.to_string())
}

fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
    let contents = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    write_text_file(path, &contents)
}

fn write_template_files(root: &Path, templates: &[TemplateFile]) -> Result<(), String> {
    for template in templates {
        write_if_missing(&root.join(template.relative_path), template.contents)?;
    }
    Ok(())
}

fn template_names(templates: &[TemplateFile]) -> Vec<&'static str> {
    templates.iter().map(|template| template.name).collect()
}

fn codex_config_template(repo_root: &Path) -> String {
    format!(
        "[mcp_servers.codegraph-mcp]\ncommand = \"codegraph-mcp\"\nargs = [\"serve-mcp\"]\ncwd = \"{}\"\n",
        path_string(repo_root).replace('\\', "\\\\")
    )
}

fn agents_template() -> &'static str {
    AGENTS_TEMPLATE
}

fn unix_time_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(u128::from(u64::MAX)) as u64,
        Err(_) => 0,
    }
}

fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn source_snippet(span: &SourceSpan, source: &str) -> String {
    let start = span.start_line.saturating_sub(1) as usize;
    let end = span.end_line.max(span.start_line) as usize;
    source
        .lines()
        .skip(start)
        .take(end.saturating_sub(start).min(8))
        .collect::<Vec<_>>()
        .join("\n")
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn sqlite_family_size_bytes(path: &Path) -> std::io::Result<u64> {
    let mut total = metadata_len(path)?;
    total += metadata_len(&PathBuf::from(format!("{}-wal", path.to_string_lossy())))?;
    total += metadata_len(&PathBuf::from(format!("{}-shm", path.to_string_lossy())))?;
    Ok(total)
}

fn metadata_len(path: &Path) -> std::io::Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn build_commit() -> &'static str {
    option_env!("GITHUB_SHA")
        .or(option_env!("VERGEN_GIT_SHA"))
        .unwrap_or("unknown")
}

const NON_CLAIMABLE_DEBUG_TIMING: &str = "non_claimable_debug_timing";

fn add_benchmark_binary_metadata(mut value: Value) -> Value {
    let metadata = benchmark_binary_metadata();
    if let Some(object) = value.as_object_mut() {
        for field in [
            "current_exe",
            "debug_assertions",
            "binary_profile",
            "exact_command",
            "claimable_for_thresholds",
            "diagnostic_only",
        ] {
            if !object.contains_key(field) {
                object.insert(field.to_string(), metadata[field].clone());
            }
        }
        if !object.contains_key("binary_metadata") {
            object.insert("binary_metadata".to_string(), metadata);
        }
    }
    value
}

fn benchmark_binary_metadata() -> Value {
    benchmark_binary_metadata_for(
        current_exe_string(),
        cfg!(debug_assertions),
        exact_command_line(),
    )
}

fn benchmark_binary_metadata_for(
    current_exe: String,
    debug_assertions: bool,
    exact_command: String,
) -> Value {
    let binary_profile = if debug_assertions { "debug" } else { "release" };
    let claimable_for_thresholds = !debug_assertions;
    json!({
        "current_exe": current_exe,
        "debug_assertions": debug_assertions,
        "binary_profile": binary_profile,
        "exact_command": exact_command,
        "claimable_for_thresholds": claimable_for_thresholds,
        "diagnostic_only": debug_assertions,
    })
}

fn current_exe_string() -> String {
    std::env::current_exe()
        .map(|path| path_string(&path))
        .unwrap_or_else(|error| format!("unknown: {error}"))
}

fn exact_command_line() -> String {
    std::env::args()
        .map(|arg| shell_quote_arg(&arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote_arg(value: &str) -> String {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

fn build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn enabled_feature_flags() -> Vec<&'static str> {
    let mut flags = Vec::new();
    if cfg!(feature = "sqlite") {
        flags.push("sqlite");
    }
    if cfg!(feature = "rocksdb") {
        flags.push("rocksdb");
    }
    if cfg!(feature = "qdrant") {
        flags.push("qdrant");
    }
    if cfg!(feature = "faiss") {
        flags.push("faiss");
    }
    if cfg!(feature = "ui") {
        flags.push("ui");
    }
    if cfg!(feature = "mcp") {
        flags.push("mcp");
    }
    flags
}

fn build_metadata_json() -> Value {
    json!({
        "schema_version": 1,
        "name": BIN_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "git_commit": build_commit(),
        "build_profile": build_profile(),
        "feature_flags": enabled_feature_flags(),
        "target": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "family": std::env::consts::FAMILY,
        },
        "checksums": "generated during release packaging",
        "provenance": "SLSA-style provenance template is emitted by the release packaging workflow",
    })
}

fn release_metadata_json() -> Value {
    json!({
        "schema_version": 1,
        "source_of_truth": "public release metadata template",
        "build": build_metadata_json(),
        "archives": release_archive_manifest_json(),
        "install_paths": release_install_paths_json(),
        "supply_chain": {
            "checksum_algorithm": "sha256",
            "provenance": "SLSA-style provenance and checksums are generated by the release workflow templates.",
            "attestation": "template"
        },
        "workflow": "single-agent-only"
    })
}

fn release_archive_manifest_json() -> Value {
    let targets = [
        (
            "windows-x64",
            "supported_tested",
            "x86_64-pc-windows-msvc",
            "zip",
            "codegraph-mcp.exe",
        ),
        (
            "macos-apple-silicon",
            "planned_not_tested_no_ci",
            "aarch64-apple-darwin",
            "tar.gz",
            "codegraph-mcp",
        ),
        (
            "macos-intel",
            "planned_not_tested_no_ci",
            "x86_64-apple-darwin",
            "tar.gz",
            "codegraph-mcp",
        ),
        (
            "linux-x64",
            "supported_tested_via_docker_and_ci",
            "x86_64-unknown-linux-gnu",
            "tar.gz",
            "codegraph-mcp",
        ),
    ];
    json!(targets
        .into_iter()
        .map(
            |(name, support_status, triple, archive_format, binary)| json!({
                "name": name,
                "support_status": support_status,
                "target_triple": triple,
                "archive": format!("{BIN_NAME}-{triple}.{archive_format}"),
                "binary": binary,
                "checksum": format!("{BIN_NAME}-{triple}.{archive_format}.sha256"),
                "provenance": format!("{BIN_NAME}-{triple}.{archive_format}.intoto.jsonl")
            })
        )
        .collect::<Vec<_>>())
}

fn release_install_paths_json() -> Value {
    json!({
        "github_release_archives": "dist/archive-manifest.json",
        "powershell_installer": "install/install.ps1",
        "shell_installer": "install/install.sh",
        "cargo_install": "cargo install --path crates/codegraph-cli",
        "cargo_binstall_metadata": "dist/cargo-binstall.example.toml",
        "homebrew_formula_template": "packaging/homebrew/codegraph-mcp.rb",
        "npm_wrapper": "not included for Phase 29"
    })
}

fn completion_shell(args: &[String]) -> Result<String, String> {
    let mut shell = "powershell".to_string();
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--shell" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--shell requires powershell, bash, zsh, or fish".to_string());
                };
                shell = value.to_ascii_lowercase();
            }
            "--json" => {}
            value => return Err(format!("unknown completions option: {value}")),
        }
        index += 1;
    }
    match shell.as_str() {
        "powershell" | "pwsh" => Ok("powershell".to_string()),
        "bash" | "zsh" | "fish" => Ok(shell),
        other => Err(format!("unsupported completion shell: {other}")),
    }
}

fn shell_completion_script(shell: &str) -> String {
    let commands = COMMANDS
        .iter()
        .map(|command| command.name)
        .collect::<Vec<_>>()
        .join(" ");
    let globals = "--repo --db --json --no-color --verbose --quiet --profile";
    match shell {
        "bash" => format!(
            r#"_codegraph_mcp_complete() {{
  local cur="${{COMP_WORDS[COMP_CWORD]}}"
  COMPREPLY=( $(compgen -W "{commands} {globals}" -- "$cur") )
}}
complete -F _codegraph_mcp_complete codegraph-mcp
"#
        ),
        "zsh" => format!(
            r#"#compdef codegraph-mcp
_arguments '*:: :->args'
case $state in
  args) _values 'codegraph-mcp commands' {commands} {globals} ;;
esac
"#
        ),
        "fish" => format!(
            r#"complete -c codegraph-mcp -f -a "{commands}"
complete -c codegraph-mcp -l repo -r
complete -c codegraph-mcp -l db -r
complete -c codegraph-mcp -l json
complete -c codegraph-mcp -l no-color
complete -c codegraph-mcp -l verbose
complete -c codegraph-mcp -l quiet
complete -c codegraph-mcp -l profile
"#
        ),
        _ => format!(
            r#"Register-ArgumentCompleter -Native -CommandName codegraph-mcp -ScriptBlock {{
    param($wordToComplete)
    "{commands} {globals}".Split(" ") | Where-Object {{ $_ -like "$wordToComplete*" }} | ForEach-Object {{
        [System.Management.Automation.CompletionResult]::new($_, $_, "ParameterValue", $_)
    }}
}}
"#
        ),
    }
}

fn json_line(value: Value) -> String {
    let mut line = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
    line.push('\n');
    line
}

fn command_help_text(command: &CommandSpec) -> String {
    let status = match command.name {
        "serve-ui" => "Implemented for Phase 19 as a local-only Proof-Path UI.",
        "bench" => {
            "Implemented for Phase 20 as a local reproducible benchmark harness; Phase 21.1 adds optional CodeGraphContext external comparison; Phase 29 adds synthetic indexing-speed profiling."
        }
        _ => "Implemented for current MVP phase.",
    };
    format!(
        "{}\n\nUsage:\n  {}\n\nStatus:\n  {}\n",
        command.description, command.usage, status
    )
}

fn not_implemented_json(command: &CommandSpec, args: &[String]) -> String {
    json_line(json!({
        "status": "not_implemented",
        "phase": PHASE,
        "command": command.name,
        "args": args,
        "message": "Command parsed successfully. Implementation is intentionally deferred to a later MVP.md phase.",
    }))
}

#[cfg(test)]
mod tests;
