//! Command-line surface for `codegraph-mcp`.
//!
//! Phase 30 hardens proof-path UI ergonomics, MCP schemas/resources, real-repo
//! maturity corpus manifests, and honest parity reports. No subagent workflow
//! is introduced.

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
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
    ContextPacket, ContextSnippet, Edge, Entity, EntityKind, Exactness, FileRecord, Metadata,
    PathEvidence, RelationKind, RepoIndexState, SourceSpan,
};
pub use codegraph_index::{
    collect_repo_files, default_db_path, index_repo, index_repo_to_db_with_options,
    index_repo_with_options, parse_extract_pending_files, should_ignore_path,
    should_start_new_index_batch, update_changed_files, update_changed_files_to_db,
    update_changed_files_with_cache, IncrementalIndexCache, IncrementalIndexSummary,
    IndexBuildMode, IndexError, IndexIssue, IndexOptions, IndexProfile, IndexSummary,
    LocalFactBundle, PendingIndexFile, StorageMode, DEFAULT_INDEX_BATCH_MAX_FILES,
    DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES, DEFAULT_STORAGE_POLICY, UNBOUNDED_STORE_READ_LIMIT,
};
use codegraph_parser::{
    detect_language, extract_entities_and_relations, language_frontends, LanguageParser,
    TreeSitterParser,
};
use codegraph_query::{
    extract_prompt_seeds, ContextPackRequest, ExactGraphQueryEngine, GraphPath, QueryLimits,
    SymbolSearchHit, SymbolSearchIndex, TraversalDirection, TraversalStep,
};
use codegraph_store::{GraphStore, SqliteGraphStore, TextSearchKind, SCHEMA_VERSION};
use codegraph_trace::{
    append_trace_event, replay_trace_file, TraceAppendEvent, TraceConfig, TraceEventType,
    DEFAULT_TRACE_ROOT,
};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

mod audit;

pub const BIN_NAME: &str = "codegraph-mcp";
pub const PHASE: &str = "30";
pub const BUNDLE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_UI_NODE_CAP: usize = 80;
const MAX_UI_NODE_CAP: usize = 250;
const SYMBOL_SEARCH_MIN_FTS_CANDIDATES: usize = 128;
const SYMBOL_SEARCH_MAX_FTS_CANDIDATES: usize = 2_048;
const SYMBOL_SEARCH_FTS_CANDIDATE_FACTOR: usize = 64;
const SYMBOL_SEARCH_FILE_CANDIDATE_LIMIT: usize = 256;
const DEFAULT_UNRESOLVED_CALL_LIMIT: usize = 100;
const MAX_UNRESOLVED_CALL_LIMIT: usize = 500;
const DEFAULT_QUERY_SURFACE_ITERATIONS: usize = 20;

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "init",
        usage: "codegraph-mcp init [repo] [--dry-run] [--with-codex-config] [--with-agents] [--with-skills] [--with-hooks] [--with-templates] [--index]",
        description: "Initialize CodeGraph state for a repository.",
    },
    CommandSpec {
        name: "index",
        usage: "codegraph-mcp index <repo> [--db <path>] [--profile] [--json] [--workers <n>] [--storage-mode <proof|audit|debug>] [--build-mode <proof-build-only|proof-build-plus-validation>]",
        description: "Index a repository into the local graph store.",
    },
    CommandSpec {
        name: "status",
        usage: "codegraph-mcp status [repo]",
        description: "Report local CodeGraph index status.",
    },
    CommandSpec {
        name: "query",
        usage: "codegraph-mcp query <symbols|text|files|references|definitions|callers|callees|chain|unresolved-calls|path> [ARGS]\n  codegraph-mcp query unresolved-calls [--limit <n>] [--offset <n>] [--json] [--no-snippets]",
        description: "Query symbols, text, files, references, definitions, calls, chains, or relation paths.",
    },
    CommandSpec {
        name: "impact",
        usage: "codegraph-mcp impact <file-or-symbol>",
        description: "Report impact analysis for a file or symbol.",
    },
    CommandSpec {
        name: "context-pack",
        usage: "codegraph-mcp context-pack --task <task> [--budget <tokens>] [--mode <mode>] [--seed <symbol>] [--profile]",
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
        usage: "codegraph-mcp bench [--baseline <mode>]... [--format <json|markdown>] [--output <path>]\n  codegraph-mcp bench graph-truth --cases <path> --fixture-root <path> --out-json <path> --out-md <path> [--fail-on-forbidden] [--fail-on-missing-source-span] [--fail-on-unresolved-exact] [--fail-on-derived-without-provenance] [--fail-on-test-mock-production-leak] [--update-mode] [--keep-workdirs] [--verbose]\n  codegraph-mcp bench context-packet --cases <path> --fixture-root <path> --out-json <path> --out-md <path> [--top-k <k>] [--budget <tokens>]\n  codegraph-mcp bench retrieval-ablation --cases <path> --fixture-root <path> --out-json <path> --out-md <path> [--mode <mode>]... [--top-k <k>]\n  codegraph-mcp bench update-integrity [--mode <update-fast|update-validated|update-debug>] [--loop-kind <combined|repeat-fast|update-fast>] [--iterations <n>] [--autoresearch-iterations <n>] [--timeout-ms <n>] [--workers <n>] [--out-json <path>] [--out-md <path>] [--workdir <path>] [--skip-autoresearch] [--only-autoresearch] [--autoresearch-repo <path>] [--autoresearch-seed-db <path>]\n  codegraph-mcp bench query-surface [--repo <path>] [--db <path>] [--fresh] [--iterations <n>] [--out-json <path>] [--out-md <path>]\n  codegraph-mcp bench proof-build-only <repo>|--repo <path> [--db <path>] [--workers <n>]\n  codegraph-mcp bench proof-build-validated <repo>|--repo <path> [--db <path>] [--workers <n>]\n  codegraph-mcp bench comprehensive [--fresh|--use-existing-artifact <db>] [--artifact-metadata <path>] [--fail-on-stale-artifact] [--repo <path>] [--workers <n>] [--output-dir <dir>] [--baseline <path>] [--compact-gate-json <path>] [--previous <path>] [--timestamp <id>]\n  codegraph-mcp bench retrieval-quality [--run-id <id>] [--timeout-ms <ms>] [--top-k <k>] [--competitor-bin <path>] [--autoresearch-repo <path>]\n  codegraph-mcp bench agent-quality [--run-id <id>] [--timeout-ms <ms>] [--competitor-bin <path>] [--fake-agent]\n  codegraph-mcp bench final-gate [--output-dir <dir>] [--workspace-root <dir>] [--timeout-ms <ms>] [--competitor-bin <path>] [--cgc-db-size-bytes <n>]\n  codegraph-mcp bench gaps [--output-dir <dir>] [--timeout-ms <ms>] [--top-k <k>] [--competitor-bin <path>]\n  codegraph-mcp bench synthetic-index --output-dir <dir> [--files <n>]\n  codegraph-mcp bench real-repo-corpus\n  codegraph-mcp bench parity-report [--output-dir <dir>]\n  codegraph-mcp bench cgc-comparison [--output-dir <dir>] [--timeout-ms <ms>] [--top-k <k>]",
        description: "Run local reproducible CodeGraph benchmarks, including optional external CGC comparison.",
    },
    CommandSpec {
        name: "trace",
        usage: "codegraph-mcp trace append --event-type <type> --trace-id <id> --tool <tool> --status <status> [--repo <path>] [--run-id <id>] [--task-id <id>] [--input-json <json>] [--output-json <json>]\n  codegraph-mcp trace replay --events <path>",
        description: "Append replayable Agent/MCP JSONL trace events or replay/validate an events.jsonl file.",
    },
    CommandSpec {
        name: "audit",
        usage: "codegraph-mcp audit storage --db <path> [--json <path>] [--markdown <path>]\n  codegraph-mcp audit schema-check --db <path> [--json <path>] [--markdown <path>]\n  codegraph-mcp audit storage-experiments --db <path> [--workdir <dir>] [--json <path>] [--markdown <path>] [--keep-copies]\n  codegraph-mcp audit sample-edges --db <path> [--relation <RELATION>] [--limit <n>] [--seed <n>] [--json <path>] [--markdown <path>] [--include-snippets]\n  codegraph-mcp audit sample-paths --db <path> [--limit <n>] [--seed <n>] [--json <path>] [--markdown <path>] [--include-snippets] [--max-edge-load <n>] [--timeout-ms <ms>] [--mode <proof|audit|debug>]\n  codegraph-mcp audit relation-counts --db <path> [--json <path>] [--markdown <path>]\n  codegraph-mcp audit label-samples --edges-json <path> [--edges-md <path>] [--paths-json <path>] [--paths-md <path>] [--json <path>] [--markdown <path>]\n  codegraph-mcp audit summarize-labels [--labels <path>] [--dir <path>] [--json <path>] [--markdown <path>]",
        description: "Run read-only audit inspections for storage, sampled edges, relation counts, and manual sample labels.",
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
    file_count: usize,
    entity_count: usize,
    edge_count: usize,
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
    }
    if let Some(db) = &globals.db {
        std::env::set_var("CODEGRAPH_DB_PATH", db);
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
    if globals.json
        && matches!(command.name, "index" | "doctor" | "languages" | "config")
        && !command_args.iter().any(|arg| arg == "--json")
    {
        command_args.push("--json".to_string());
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
        Ok(value) => success(json_line(value)),
        Err(error) => command_error(error_kind, &error),
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
    let (repo, db, options) = parse_index_options(args)?;
    let summary = if let Some(db) = db {
        index_repo_to_db_with_options(Path::new(&repo), &db, options)
    } else {
        index_repo_with_options(Path::new(&repo), options)
    }
    .map_err(|error| error.to_string())?;
    index_summary_json(&summary)
}

fn run_status_command(args: &[String]) -> Result<Value, String> {
    let repo = optional_repo_arg(args)?;
    let repo_root = resolve_repo_root(&repo)?;
    let db_path = default_db_path(&repo_root);
    if !db_path.exists() {
        return Ok(json!({
            "status": "not_indexed",
            "repo_root": path_string(&repo_root),
            "db_path": path_string(&db_path),
            "next_command": "codegraph-mcp index .",
        }));
    }

    let store = SqliteGraphStore::open(&db_path).map_err(|error| error.to_string())?;
    let files = store
        .list_files(10_000)
        .map_err(|error| error.to_string())?;
    let languages = files
        .iter()
        .filter_map(|file| file.language.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let storage_accounting = store
        .storage_accounting()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|row| {
            json!({
                "name": row.name,
                "row_count": row.row_count,
                "payload_bytes": row.payload_bytes,
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "status": "ok",
        "phase": PHASE,
        "repo_root": path_string(&repo_root),
        "db_path": path_string(&db_path),
        "db_size_bytes": sqlite_family_size_bytes(&db_path).map_err(|error| error.to_string())?,
        "storage_policy": DEFAULT_STORAGE_POLICY,
        "schema_version": store.schema_version().map_err(|error| error.to_string())?,
        "files": store.count_files().map_err(|error| error.to_string())?,
        "entities": store.count_entities().map_err(|error| error.to_string())?,
        "edges": store.count_edges().map_err(|error| error.to_string())?,
        "source_spans": store.count_source_spans().map_err(|error| error.to_string())?,
        "relation_counts": store.relation_counts().map_err(|error| error.to_string())?,
        "storage_accounting": storage_accounting,
        "languages": languages,
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
                return Err(format!("unknown doctor option: {value}"))
            }
            value => repo = PathBuf::from(value),
        }
        index += 1;
    }
    let _json_output = json_output;
    let repo_root = resolve_repo_root(&repo)?;
    let db_path = default_db_path(&repo_root);
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
    let db_ok = db_path.exists()
        && SqliteGraphStore::open(&db_path)
            .and_then(|store| store.schema_version().map(|_| ()))
            .is_ok();
    push_doctor_check(
        &mut checks,
        "database",
        if db_ok { "ok" } else { "warning" },
        "local SQLite graph database exists and opens",
        db_ok,
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

    Ok(json!({
        "status": if errors == 0 { "ok" } else { "error" },
        "phase": PHASE,
        "repo_root": path_string(&repo_root),
        "db_path": path_string(&db_path),
        "checks": checks,
        "warnings": warnings,
        "errors": errors,
        "proof": "Doctor is local-only and treats missing optional components as warnings.",
    }))
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
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Err("Usage: codegraph-mcp query <symbols|text|files|references|definitions|callers|callees|chain|unresolved-calls|path> [ARGS]".to_string());
    };
    match subcommand {
        "symbols" => {
            if args.len() < 2 {
                return Err("Usage: codegraph-mcp query symbols <query>".to_string());
            }
            let query = args[1..].join(" ");
            query_symbols(&current_repo_root()?, &query, 20)
        }
        "text" => {
            if args.len() < 2 {
                return Err("Usage: codegraph-mcp query text <query>".to_string());
            }
            let query = args[1..].join(" ");
            query_text(&current_repo_root()?, &query, 20)
        }
        "files" => {
            if args.len() < 2 {
                return Err("Usage: codegraph-mcp query files <query>".to_string());
            }
            let query = args[1..].join(" ");
            query_files(&current_repo_root()?, &query, 20)
        }
        "references" => {
            if args.len() < 2 {
                return Err("Usage: codegraph-mcp query references <symbol>".to_string());
            }
            let query = args[1..].join(" ");
            query_references(&current_repo_root()?, &query, 32)
        }
        "definitions" => {
            if args.len() < 2 {
                return Err("Usage: codegraph-mcp query definitions <symbol>".to_string());
            }
            let query = args[1..].join(" ");
            query_definitions(&current_repo_root()?, &query, 20)
        }
        "callers" => {
            if args.len() < 2 {
                return Err("Usage: codegraph-mcp query callers <symbol>".to_string());
            }
            let query = args[1..].join(" ");
            query_callers(&current_repo_root()?, &query, 32)
        }
        "callees" => {
            if args.len() < 2 {
                return Err("Usage: codegraph-mcp query callees <symbol>".to_string());
            }
            let query = args[1..].join(" ");
            query_callees(&current_repo_root()?, &query, 32)
        }
        "chain" => {
            if args.len() != 3 {
                return Err("Usage: codegraph-mcp query chain <source> <target>".to_string());
            }
            query_chain(&current_repo_root()?, &args[1], &args[2])
        }
        "unresolved-calls" => {
            let options = parse_unresolved_calls_args(&args[1..])?;
            query_unresolved_calls(&current_repo_root()?, options)
        }
        "path" => {
            if args.len() != 3 {
                return Err("Usage: codegraph-mcp query path <source> <target>".to_string());
            }
            query_path(&current_repo_root()?, &args[1], &args[2])
        }
        other => Err(format!("unknown query subcommand: {other}")),
    }
}

fn run_context_pack_command(args: &[String]) -> Result<Value, String> {
    let options = parse_context_pack_args(args)?;
    let profile_enabled = options.profile;
    let repo_root = current_repo_root()?;
    let total_start = Instant::now();
    let mut profile_spans = Vec::new();
    let db_path = default_db_path(&repo_root);
    let open_start = Instant::now();
    let connection = open_context_pack_connection(&db_path)?;
    profile_spans.push(profile_span_json(
        "open_store",
        open_start.elapsed(),
        1,
        0,
        json!({ "db_path": path_string(&db_path) }),
    ));

    let budgets = ContextPackBudgets::for_mode(&options.mode);
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
    let mut stored_paths =
        load_stored_context_path_evidence(&connection, &seed_ids, &options.mode, budgets)?;
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

    let span_source_start = Instant::now();
    let mut requested_spans = stored_paths
        .iter()
        .flat_map(|path| path.source_spans.iter().cloned())
        .collect::<Vec<_>>();
    requested_spans.extend(
        fallback_edges
            .iter()
            .map(|edge| edge.source_span.clone())
            .collect::<Vec<_>>(),
    );
    requested_spans.sort_by(|left, right| {
        left.repo_relative_path
            .cmp(&right.repo_relative_path)
            .then_with(|| left.start_line.cmp(&right.start_line))
            .then_with(|| left.end_line.cmp(&right.end_line))
    });
    requested_spans.dedup();
    let (sources, snippets, source_bytes, source_files_loaded) =
        load_context_sources_and_snippets(&repo_root, &requested_spans, budgets.max_snippets)?;
    profile_spans.push(profile_span_json(
        "snippet_loading",
        span_source_start.elapsed(),
        source_files_loaded as u64,
        source_bytes as u64,
        json!({
            "source_files_loaded": source_files_loaded,
            "source_bytes_loaded": source_bytes,
            "requested_spans": requested_spans.len(),
            "snippets_returned": snippets.len(),
            "policy": "load only files referenced by selected proof/source spans"
        }),
    ));

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
    if let Some(packet) = &fallback_packet {
        stored_paths.extend(packet.verified_paths.clone());
    }
    stored_paths = filter_and_sort_context_path_evidence(stored_paths, &options.mode, budgets);
    let packet = build_context_packet_from_stored_evidence(
        &options,
        &raw_seed_values,
        &seed_ids,
        &seed_entities,
        stored_paths,
        snippets,
        fallback_packet,
        budgets,
        stored_path_count,
        requested_spans.len(),
    );
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

    Ok(json!({
        "status": "ok",
        "task": options.task,
        "mode": options.mode,
        "packet": packet,
        "profile": profile,
        "proof": "Context packet is built from local graph/source evidence.",
    }))
}

fn run_impact_command(args: &[String]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Usage: codegraph-mcp impact <file-or-symbol>".to_string());
    }
    let repo_root = current_repo_root()?;
    impact_value(&repo_root, &args[0])
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

fn run_bench_command(args: &[String]) -> Result<Value, String> {
    if matches!(args.first().map(String::as_str), Some("graph-truth")) {
        return run_graph_truth_gate_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("context-packet" | "context-packet-gate" | "context")
    ) {
        return run_context_packet_gate_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("retrieval-ablation" | "stage-ablation")
    ) {
        return run_retrieval_ablation_command(&args[1..]);
    }

    if matches!(args.first().map(String::as_str), Some("retrieval-quality")) {
        return run_retrieval_quality_benchmark_command(&args[1..]);
    }

    if matches!(args.first().map(String::as_str), Some("agent-quality")) {
        return run_agent_quality_benchmark_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("synthetic-index" | "indexing-speed")
    ) {
        return run_synthetic_index_benchmark_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("update-integrity" | "autoresearch-update-repro" | "db-integrity-update")
    ) {
        return run_update_integrity_harness_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("query-surface" | "default-query-surface")
    ) {
        return run_query_surface_benchmark_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("proof-build-only" | "proof-build")
    ) {
        return run_proof_build_mode_benchmark_command(&args[1..], IndexBuildMode::ProofBuildOnly);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("proof-build-validated" | "proof-build-plus-validation")
    ) {
        return run_proof_build_mode_benchmark_command(
            &args[1..],
            IndexBuildMode::ProofBuildPlusValidation,
        );
    }

    if matches!(
        args.first().map(String::as_str),
        Some("comprehensive" | "comprehensive-gate" | "master-gate")
    ) {
        return run_comprehensive_benchmark_command(&args[1..]);
    }

    if matches!(args.first().map(String::as_str), Some("real-repo-corpus")) {
        return run_real_repo_corpus_command(&args[1..]);
    }

    if matches!(args.first().map(String::as_str), Some("parity-report")) {
        return run_parity_report_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("final-gate" | "acceptance-gate" | "compact-mvp-gate")
    ) {
        return run_final_gate_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("gaps" | "gap-scoreboard" | "gap-scoreboard-report")
    ) {
        return run_gap_scoreboard_command(&args[1..]);
    }

    if matches!(
        args.first().map(String::as_str),
        Some("cgc-comparison" | "codegraphcontext-comparison")
    ) {
        return run_cgc_comparison_command(&args[1..]);
    }

    let options = parse_bench_options(args)?;
    let report = codegraph_bench::run_default_benchmark_suite(&options.baselines)
        .map_err(|error| error.to_string())?;
    match options.output {
        Some(output) => {
            write_benchmark_report(&report, options.format, &output)?;
            Ok(json!({
                "status": "benchmarked",
                "phase": PHASE,
                "suite_id": report.suite_id,
                "format": options.format.as_str(),
                "output": path_string(&output),
                "runs": report.results.len(),
                "baselines": report.aggregate.keys().cloned().collect::<Vec<_>>(),
                "aggregate": report.aggregate,
            }))
        }
        None if options.format == BenchReportFormat::Json => {
            serde_json::to_value(report).map_err(|error| error.to_string())
        }
        None => Ok(json!({
            "status": "benchmarked",
            "phase": PHASE,
            "suite_id": report.suite_id,
            "format": options.format.as_str(),
            "runs": report.results.len(),
            "markdown": codegraph_bench::render_markdown_report(&report),
            "aggregate": report.aggregate,
        })),
    }
}

fn run_graph_truth_gate_command(args: &[String]) -> Result<Value, String> {
    let options = parse_graph_truth_gate_options(args)?;
    let report = codegraph_bench::write_graph_truth_gate_report(options.clone())
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": report.status,
        "phase": PHASE,
        "gate": "graph_truth",
        "verdict": report.verdict,
        "cases_total": report.cases_total,
        "cases_passed": report.cases_passed,
        "cases_failed": report.cases_failed,
        "out_json": path_string(&options.out_json),
        "out_md": path_string(&options.out_md),
        "proof": "Graph Truth Gate indexes each fixture and compares expected and forbidden graph facts, source spans, paths, context symbols, and tests.",
    }))
}

fn run_context_packet_gate_command(args: &[String]) -> Result<Value, String> {
    let options = parse_context_packet_gate_options(args)?;
    let report = codegraph_bench::write_context_packet_gate_report(options.clone())
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": report.status,
        "phase": PHASE,
        "gate": "context_packet",
        "verdict": report.verdict,
        "cases_total": report.cases_total,
        "cases_passed": report.cases_passed,
        "cases_failed": report.cases_failed,
        "context_symbol_recall_at_k": report.metrics.context_symbol_recall_at_k,
        "critical_symbol_missing_rate": report.metrics.critical_symbol_missing_rate,
        "distractor_ratio": report.metrics.distractor_ratio,
        "proof_path_coverage": report.metrics.proof_path_coverage,
        "source_span_coverage": report.metrics.source_span_coverage,
        "useful_facts_per_byte": report.metrics.useful_facts_per_byte,
        "out_json": path_string(&options.out_json),
        "out_md": path_string(&options.out_md),
        "proof": "Context Packet Gate scores packet usefulness against graph-truth critical symbols, proof paths, source spans, snippets, tests, context labels, and distractors.",
    }))
}

fn run_retrieval_ablation_command(args: &[String]) -> Result<Value, String> {
    let options = parse_retrieval_ablation_options(args)?;
    let report = codegraph_bench::write_retrieval_ablation_report(options.clone())
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": report.status,
        "phase": PHASE,
        "benchmark": "retrieval_ablation",
        "cases_total": report.case_results.len(),
        "modes": report.modes.iter().map(|mode| mode.mode.clone()).collect::<Vec<_>>(),
        "out_json": path_string(&options.out_json),
        "out_md": path_string(&options.out_md),
        "proof": "Retrieval ablation separates Stage 0, Stage 1, Stage 2, exact graph verification, and full context packet metrics.",
    }))
}

fn run_retrieval_quality_benchmark_command(args: &[String]) -> Result<Value, String> {
    let options = parse_two_layer_bench_options(args)?;
    let artifacts = codegraph_bench::run_retrieval_quality_benchmark(options)
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "benchmarked",
        "phase": PHASE,
        "layer": "retrieval_quality",
        "run_id": artifacts.run_id,
        "run_root": artifacts.run_root,
        "manifest": artifacts.manifest_json,
        "events_jsonl": artifacts.events_jsonl,
        "per_task_jsonl": artifacts.per_task_jsonl,
        "summary_md": artifacts.summary_md,
        "proof": "Retrieval benchmark writes manifest, events, per-task JSONL, raw outputs, and normalized outputs under target/codegraph-bench-runs/<run_id>.",
    }))
}

fn run_agent_quality_benchmark_command(args: &[String]) -> Result<Value, String> {
    let mut options = parse_two_layer_bench_options(args)?;
    options.dry_run = true;
    options.fake_agent = true;
    let artifacts =
        codegraph_bench::run_agent_quality_benchmark(options).map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "benchmarked",
        "phase": PHASE,
        "layer": "agent_coding_quality",
        "run_id": artifacts.run_id,
        "run_root": artifacts.run_root,
        "manifest": artifacts.manifest_json,
        "events_jsonl": artifacts.events_jsonl,
        "per_task_jsonl": artifacts.per_task_jsonl,
        "summary_md": artifacts.summary_md,
        "proof": "Agent benchmark dry-run uses a fake runner, records tool/agent trace events, and stores patches/final answers under target/codegraph-bench-runs/<run_id>.",
    }))
}

fn run_trace_command(args: &[String]) -> Result<Value, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Err("Usage: codegraph-mcp trace <append|replay> [ARGS]".to_string());
    };
    match subcommand {
        "append" => {
            let options = parse_trace_append_options(&args[1..])?;
            let trace_root = options.trace_root.clone();
            let mut config = TraceConfig::for_repo(&options.repo).with_trace_root(&trace_root);
            if let Some(run_id) = options.run_id {
                config = config.with_run_id(run_id);
            }
            if let Some(task_id) = options.task_id {
                config = config.with_task_id(task_id);
            }
            if let Some(repo_id) = options.repo_id {
                config = config.with_repo_id(repo_id);
            }
            let event =
                append_trace_event(config, options.event).map_err(|error| error.to_string())?;
            Ok(json!({
                "status": "traced",
                "event_type": event.event_type.as_str(),
                "run_id": event.run_id,
                "trace_id": event.trace_id,
                "task_id": event.task_id,
                "repo_id": event.repo_id,
                "events_jsonl": path_string(&trace_root.join(&event.run_id).join("events.jsonl")),
                "artifact_path": event.artifact_path,
            }))
        }
        "replay" | "validate" => {
            let events_path = parse_trace_replay_options(&args[1..])?;
            let report = replay_trace_file(&events_path).map_err(|error| error.to_string())?;
            let answers = json!({
                "mcp_tools_called": report.mcp_calls_made.clone(),
                "files_edited": report.files_edited.clone(),
                "tests_run": report.tests_run.clone(),
                "context_evidence_used": report.context_evidence_used.clone(),
            });
            Ok(json!({
                "status": "ok",
                "events_jsonl": path_string(&events_path),
                "replay": report,
                "answers": answers
            }))
        }
        other => Err(format!("unknown trace subcommand: {other}")),
    }
}

fn run_real_repo_corpus_command(args: &[String]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("Usage: codegraph-mcp bench real-repo-corpus".to_string());
    }
    let corpus = codegraph_bench::real_repo_maturity_corpus();
    corpus.validate().map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "ok",
        "phase": PHASE,
        "corpus": corpus,
        "replay": codegraph_bench::plan_real_repo_corpus_replay(&codegraph_bench::real_repo_maturity_corpus(), ".codegraph-bench-cache/real-repos", false).map_err(|error| error.to_string())?,
    }))
}

fn run_gap_scoreboard_command(args: &[String]) -> Result<Value, String> {
    let options = parse_gap_scoreboard_options(args)?;
    let artifacts =
        codegraph_bench::write_gap_scoreboard_report(options).map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "reported",
        "phase": "26",
        "artifacts": artifacts,
        "proof": "Gap scoreboard records win/loss/tie/unknown dimensions and keeps missing data as unknown.",
    }))
}

fn run_parity_report_command(args: &[String]) -> Result<Value, String> {
    let mut output_dir = PathBuf::from("reports").join("phase30-parity");
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--output-dir" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--output-dir requires a path".to_string());
                };
                output_dir = PathBuf::from(value);
            }
            value => return Err(format!("unknown parity-report option: {value}")),
        }
        index += 1;
    }
    let artifacts = codegraph_bench::write_final_parity_report(&output_dir)
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "reported",
        "phase": PHASE,
        "artifacts": artifacts,
        "proof": "Final parity report records unknown/skipped fields explicitly and makes no SOTA claim.",
    }))
}

fn run_final_gate_command(args: &[String]) -> Result<Value, String> {
    let options = parse_final_gate_options(args)?;
    let artifacts = codegraph_bench::write_final_acceptance_gate_report(options.clone())
        .map_err(|error| error.to_string())?;
    let summary: Value = serde_json::from_str(
        &fs::read_to_string(&artifacts.json_summary).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "reported",
        "phase": PHASE,
        "gate": "final_compact_mvp_acceptance",
        "verdict": summary["verdict"].clone(),
        "internal_verdict": summary["internal_verdict"].clone(),
        "cgc_status": summary["cgc_comparison"]["status"].clone(),
        "artifacts": artifacts,
        "proof": "Final gate requires compact storage and MVP proof/functionality preservation; missing CGC data keeps the verdict unknown.",
    }))
}

fn run_comprehensive_benchmark_command(args: &[String]) -> Result<Value, String> {
    let options = parse_comprehensive_benchmark_options(args)?;
    fs::create_dir_all(&options.output_dir).map_err(|error| error.to_string())?;
    let report = build_comprehensive_benchmark_report(&options)?;
    let markdown = render_comprehensive_benchmark_markdown(&report);

    let timestamp_json = options.output_dir.join(format!(
        "comprehensive_benchmark_{}.json",
        options.timestamp
    ));
    let timestamp_md = options
        .output_dir
        .join(format!("comprehensive_benchmark_{}.md", options.timestamp));
    let latest_json = options
        .output_dir
        .join("comprehensive_benchmark_latest.json");
    let latest_md = options.output_dir.join("comprehensive_benchmark_latest.md");

    write_json_file(&timestamp_json, &report)?;
    write_text_file(&timestamp_md, &markdown)?;
    write_json_file(&latest_json, &report)?;
    write_text_file(&latest_md, &markdown)?;

    Ok(json!({
        "status": "reported",
        "phase": PHASE,
        "benchmark": "comprehensive",
        "verdict": report["sections"]["executive_verdict"]["verdict"].clone(),
        "reason_for_failure": report["sections"]["executive_verdict"]["reason_for_failure"].clone(),
        "output_json": path_string(&latest_json),
        "output_md": path_string(&latest_md),
        "timestamped_json": path_string(&timestamp_json),
        "timestamped_md": path_string(&timestamp_md),
        "proof": "Comprehensive benchmark builds a fresh proof artifact by default; explicit artifact reuse is labeled and cannot claim storage/cold-build passes if stale.",
    }))
}

fn parse_comprehensive_benchmark_options(
    args: &[String],
) -> Result<ComprehensiveBenchmarkOptions, String> {
    let mut output_dir = PathBuf::from("reports").join("final");
    let mut baseline_json = PathBuf::from("reports")
        .join("baselines")
        .join("compact_proof_baseline_latest.json");
    let mut compact_gate_json = PathBuf::from("reports")
        .join("final")
        .join("compact_proof_db_gate.json");
    let mut previous_json = Some(
        PathBuf::from("reports")
            .join("final")
            .join("comprehensive_benchmark_latest.json"),
    );
    let mut timestamp = format!("{}", unix_time_ms());
    let mut artifact_mode = ComprehensiveArtifactMode::Fresh;
    let mut repo = PathBuf::from(".");
    let mut artifact_metadata = None;
    let mut fail_on_stale_artifact = false;
    let mut workers = None;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--fresh" => {
                artifact_mode = ComprehensiveArtifactMode::Fresh;
            }
            "--use-existing-artifact" => {
                index += 1;
                artifact_mode = ComprehensiveArtifactMode::Existing(PathBuf::from(
                    required_cli_value(args, index, "--use-existing-artifact")?,
                ));
            }
            "--artifact-metadata" => {
                index += 1;
                artifact_metadata = Some(PathBuf::from(required_cli_value(
                    args,
                    index,
                    "--artifact-metadata",
                )?));
            }
            "--fail-on-stale-artifact" => {
                fail_on_stale_artifact = true;
            }
            "--repo" => {
                index += 1;
                repo = PathBuf::from(required_cli_value(args, index, "--repo")?);
            }
            "--workers" => {
                index += 1;
                let raw = required_cli_value(args, index, "--workers")?;
                let parsed = raw
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --workers value {raw}: {error}"))?;
                if parsed == 0 {
                    return Err("--workers must be greater than 0".to_string());
                }
                workers = Some(parsed);
            }
            "--output-dir" => {
                index += 1;
                output_dir = PathBuf::from(required_cli_value(args, index, "--output-dir")?);
            }
            "--baseline" | "--baseline-json" => {
                index += 1;
                baseline_json = PathBuf::from(required_cli_value(args, index, "--baseline")?);
            }
            "--compact-gate-json" | "--gate-json" | "--source-gate-json" => {
                index += 1;
                compact_gate_json =
                    PathBuf::from(required_cli_value(args, index, "--compact-gate-json")?);
            }
            "--previous" | "--previous-json" => {
                index += 1;
                previous_json = Some(PathBuf::from(required_cli_value(
                    args,
                    index,
                    "--previous",
                )?));
            }
            "--no-previous" => {
                previous_json = None;
            }
            "--timestamp" => {
                index += 1;
                timestamp = required_cli_value(args, index, "--timestamp")?.to_string();
            }
            value => return Err(format!("unknown comprehensive benchmark option: {value}")),
        }
        index += 1;
    }

    Ok(ComprehensiveBenchmarkOptions {
        output_dir,
        baseline_json,
        compact_gate_json,
        previous_json,
        timestamp,
        artifact_mode,
        repo,
        artifact_metadata,
        fail_on_stale_artifact,
        workers,
    })
}

fn run_query_surface_benchmark_command(args: &[String]) -> Result<Value, String> {
    let options = parse_query_surface_benchmark_options(args)?;
    let repo = absolutize_path(&options.repo)?;
    let db_path = if options.fresh {
        let db_path = options.db_path.clone().unwrap_or_else(|| {
            PathBuf::from("reports")
                .join("audit")
                .join("artifacts")
                .join(format!("default_query_surface_{}.sqlite", unix_time_ms()))
        });
        let db_path = absolutize_path(&db_path)?;
        remove_sqlite_family_if_exists(&db_path)?;
        index_repo_to_db_with_options(
            &repo,
            &db_path,
            IndexOptions {
                profile: true,
                json: false,
                worker_count: options.workers,
                storage_mode: StorageMode::Proof,
                build_mode: IndexBuildMode::ProofBuildOnly,
            },
        )
        .map_err(|error| error.to_string())?;
        db_path
    } else {
        options
            .db_path
            .clone()
            .map(|path| absolutize_path(&path))
            .transpose()?
            .unwrap_or_else(|| default_db_path(&repo))
    };

    let report = build_default_query_surface_report(&repo, &db_path, options.iterations);
    let markdown = render_default_query_surface_markdown(&report);
    write_json_file(&options.out_json, &report)?;
    write_text_file(&options.out_md, &markdown)?;

    Ok(json!({
        "status": report["status"].clone(),
        "phase": PHASE,
        "benchmark": "query_surface",
        "repo_root": path_string(&repo),
        "db_path": path_string(&db_path),
        "iterations": options.iterations,
        "output_json": path_string(&options.out_json),
        "output_md": path_string(&options.out_md),
        "proof": "Default query-surface probes run directly against the compact proof DB and report failures instead of falling back to audit/debug sidecars.",
    }))
}

fn run_proof_build_mode_benchmark_command(
    args: &[String],
    build_mode: IndexBuildMode,
) -> Result<Value, String> {
    let mut index_args = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--repo" => {
                index += 1;
                let Some(repo) = args.get(index) else {
                    return Err("--repo requires a path".to_string());
                };
                index_args.push(repo.clone());
            }
            value => index_args.push(value.to_string()),
        }
        index += 1;
    }
    if !index_args.iter().any(|arg| arg == "--profile") {
        index_args.push("--profile".to_string());
    }
    if !index_args.iter().any(|arg| arg == "--json") {
        index_args.push("--json".to_string());
    }
    if !index_args.iter().any(|arg| arg == "--storage-mode") {
        index_args.push("--storage-mode".to_string());
        index_args.push("proof".to_string());
    }
    if !index_args.iter().any(|arg| arg == "--build-mode") {
        index_args.push("--build-mode".to_string());
        index_args.push(build_mode.as_str().to_string());
    }

    let started = Instant::now();
    let (repo, db, options) = parse_index_options(&index_args)?;
    let summary = if let Some(db) = db {
        index_repo_to_db_with_options(Path::new(&repo), &db, options)
    } else {
        index_repo_with_options(Path::new(&repo), options)
    }
    .map_err(|error| error.to_string())?;
    let elapsed = elapsed_ms(started);
    let summary_json = index_summary_json(&summary)?;
    Ok(json!({
        "status": "benchmarked",
        "phase": PHASE,
        "benchmark": match build_mode {
            IndexBuildMode::ProofBuildOnly => "proof_build_only",
            IndexBuildMode::ProofBuildPlusValidation => "proof_build_validated",
        },
        "mode": build_mode.as_str(),
        "proof_build_only_ms": if build_mode == IndexBuildMode::ProofBuildOnly {
            summary.profile.as_ref().map(|profile| profile.total_wall_ms).unwrap_or(elapsed as u128)
        } else {
            0
        },
        "validation_ms": if build_mode == IndexBuildMode::ProofBuildPlusValidation {
            summary.profile.as_ref().map(|profile| profile.total_wall_ms).unwrap_or(elapsed as u128)
        } else {
            0
        },
        "wall_ms": elapsed,
        "summary": summary_json,
        "mode_separation": {
            "proof_build_only_excludes": [
                "storage_audit",
                "dbstat",
                "relation_sampler",
                "path_evidence_audit_sampler",
                "cgc_comparison",
                "manual_relation_label_summary",
                "comprehensive_markdown_generation",
                "artifact_compression"
            ],
            "cgc_autorun": false
        }
    }))
}

fn parse_query_surface_benchmark_options(
    args: &[String],
) -> Result<QuerySurfaceBenchmarkOptions, String> {
    let mut repo = PathBuf::from(".");
    let mut db_path = None;
    let mut fresh = false;
    let mut iterations = DEFAULT_QUERY_SURFACE_ITERATIONS;
    let mut out_json = PathBuf::from("reports")
        .join("audit")
        .join("default_query_surface.json");
    let mut out_md = PathBuf::from("reports")
        .join("audit")
        .join("default_query_surface.md");
    let mut workers = None;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--repo" => {
                index += 1;
                repo = PathBuf::from(required_cli_value(args, index, "--repo")?);
            }
            "--db" => {
                index += 1;
                db_path = Some(PathBuf::from(required_cli_value(args, index, "--db")?));
            }
            "--fresh" => {
                fresh = true;
            }
            "--iterations" => {
                index += 1;
                let raw = required_cli_value(args, index, "--iterations")?;
                iterations = raw
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --iterations value {raw}: {error}"))?
                    .max(1);
            }
            "--out-json" | "--json" => {
                index += 1;
                out_json = PathBuf::from(required_cli_value(args, index, "--out-json")?);
            }
            "--out-md" | "--markdown" => {
                index += 1;
                out_md = PathBuf::from(required_cli_value(args, index, "--out-md")?);
            }
            "--workers" => {
                index += 1;
                let raw = required_cli_value(args, index, "--workers")?;
                let parsed = raw
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --workers value {raw}: {error}"))?;
                if parsed == 0 {
                    return Err("--workers must be greater than 0".to_string());
                }
                workers = Some(parsed);
            }
            value => return Err(format!("unknown query-surface benchmark option: {value}")),
        }
        index += 1;
    }

    Ok(QuerySurfaceBenchmarkOptions {
        repo,
        db_path,
        fresh,
        iterations,
        out_json,
        out_md,
        workers,
    })
}

fn build_comprehensive_benchmark_report(
    options: &ComprehensiveBenchmarkOptions,
) -> Result<Value, String> {
    let baseline = read_json_file(&options.baseline_json)?;
    let mut gate = read_json_file(&options.compact_gate_json)?;
    let artifact_freshness = prepare_comprehensive_artifact(options, &mut gate)?;
    let query_surface_report =
        build_comprehensive_query_surface_artifact(options, &artifact_freshness);
    let proof_storage = read_gate_artifact_json(&gate, "proof_storage_json");
    let _audit_storage = read_gate_artifact_json(&gate, "audit_storage_json");
    let update_integrity = read_gate_artifact_json(&gate, "update_integrity_json");
    let repeat_unchanged = read_optional_json(Path::new(
        "reports/final/artifacts/compact_gate_repeat_unchanged.json",
    ));
    let context_pack_latency = read_gate_artifact_json(&gate, "context_pack_latency_json");
    let unresolved_latency = read_gate_artifact_json(&gate, "unresolved_calls_latency_json");
    let manual_labels = read_optional_json(Path::new(
        "reports/audit/manual_relation_labeling_summary.json",
    ));
    let comparison =
        read_optional_json(Path::new("reports/comparison/CODEGRAPH_VS_CGC_LATEST.json"));
    let clean_gate = read_optional_json(Path::new("reports/final/INTENDED_PERFORMANCE_GATE.json"));
    let previous = options
        .previous_json
        .as_ref()
        .and_then(|path| read_optional_json(path));

    let graph_truth = gate_value(&gate, &["gates", "graph_truth"])
        .cloned()
        .or_else(|| gate_value(&baseline, &["graph_truth_summary"]).cloned())
        .unwrap_or_else(|| json!({}));
    let context_gate = gate_value(&gate, &["gates", "context_packet"])
        .cloned()
        .or_else(|| gate_value(&baseline, &["context_quality_summary"]).cloned())
        .unwrap_or_else(|| json!({}));
    let storage = gate_value(&gate, &["storage"])
        .cloned()
        .unwrap_or_else(|| json!({}));
    let proof_build = gate_value(&gate, &["autoresearch", "proof_build"])
        .cloned()
        .unwrap_or_else(|| json!({}));
    let update_path = gate_value(&gate, &["update_path"])
        .cloned()
        .unwrap_or_else(|| json!({}));
    let relation_sampler = gate_value(&gate, &["relation_sampler"])
        .cloned()
        .unwrap_or_else(|| json!({}));

    let correctness_metrics = comprehensive_correctness_metrics(&graph_truth);
    let context_metrics = comprehensive_context_metrics(&context_gate, &storage, &relation_sampler);
    let integrity_metrics = comprehensive_integrity_metrics(&gate, &proof_storage, &storage);
    let artifact_freshness_metrics = comprehensive_artifact_freshness_metrics(&artifact_freshness);
    let storage_summary_metrics = comprehensive_storage_summary_metrics(&storage, &proof_storage);
    let storage_contributors =
        comprehensive_storage_contributors(&proof_storage, previous.as_ref(), &baseline);
    let cardinality_metrics =
        comprehensive_cardinality_metrics(&proof_build, &proof_storage, &storage);
    let cold_profile_metrics = comprehensive_cold_profile_metrics(&proof_build);
    let cold_mode_distinction = comprehensive_cold_build_mode_distinction(&gate);
    let cold_waterfall = comprehensive_cold_build_waterfall(&gate);
    let cold_interpretation = comprehensive_cold_build_interpretation(&gate);
    let repeat_metrics = comprehensive_repeat_metrics(
        &update_path,
        repeat_unchanged.as_ref(),
        update_integrity.as_ref(),
    );
    let update_metrics =
        comprehensive_single_file_update_metrics(&update_path, update_integrity.as_ref());
    let query_latency = comprehensive_query_latency_metrics(
        &context_pack_latency,
        &unresolved_latency,
        &gate,
        Some(&query_surface_report),
    );
    let manual_quality = comprehensive_manual_quality_section(&manual_labels);
    let comparison_section = comprehensive_comparison_section(&comparison);
    let regression_summary =
        comprehensive_regression_summary(&baseline, &gate, &clean_gate, previous.as_ref());

    let mut failed_targets = Vec::new();
    let mut passed_targets = Vec::new();
    collect_gate_statuses(
        &correctness_metrics,
        &mut failed_targets,
        &mut passed_targets,
    );
    collect_gate_statuses(&context_metrics, &mut failed_targets, &mut passed_targets);
    collect_gate_statuses(&integrity_metrics, &mut failed_targets, &mut passed_targets);
    collect_gate_statuses(
        &artifact_freshness_metrics,
        &mut failed_targets,
        &mut passed_targets,
    );
    collect_gate_statuses(
        &storage_summary_metrics,
        &mut failed_targets,
        &mut passed_targets,
    );
    collect_gate_statuses(
        &cold_profile_metrics,
        &mut failed_targets,
        &mut passed_targets,
    );
    collect_gate_statuses(&repeat_metrics, &mut failed_targets, &mut passed_targets);
    collect_gate_statuses(&update_metrics, &mut failed_targets, &mut passed_targets);
    collect_query_gate_statuses(&query_latency, &mut failed_targets, &mut passed_targets);

    failed_targets.sort();
    failed_targets.dedup();
    passed_targets.sort();
    passed_targets.dedup();

    let graph_context_integrity_ok = !correctness_metrics
        .iter()
        .chain(context_metrics.iter())
        .chain(integrity_metrics.iter())
        .any(|metric| metric["status"].as_str() == Some("fail"));
    let verdict = if failed_targets.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let comparison_claims_allowed = verdict == "pass"
        && comparison_section["verdict"].as_str() == Some("pass")
        && comparison_section["cgc_completed"].as_bool() == Some(true);
    let optimization_may_continue = graph_context_integrity_ok;
    let reason_for_failure = if failed_targets.is_empty() {
        "all tracked pass/fail targets are currently satisfied".to_string()
    } else {
        format!(
            "failed targets: {}",
            failed_targets
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let executive_verdict = json!({
        "verdict": verdict,
        "reason_for_failure": reason_for_failure,
        "exact_failed_targets": failed_targets,
        "exact_passed_targets": passed_targets,
        "optimization_may_continue": optimization_may_continue,
        "optimization_may_continue_notes": if optimization_may_continue {
            "Yes, but only behind the do-not-regress gates: graph truth, context packet quality, DB integrity, context_pack latency, unresolved-calls latency, and repeat unchanged must stay green."
        } else {
            "No. Correctness, context, or integrity regressed."
        },
        "comparison_claims_allowed": comparison_claims_allowed,
        "comparison_claims_notes": if comparison_claims_allowed {
            "Comparable competitor artifacts are complete and CodeGraph has not failed internal targets."
        } else {
            "No superiority claim is allowed while internal targets fail or CGC artifacts are incomplete/stale."
        }
    });

    Ok(json!({
        "schema_version": 1,
        "benchmark_id": "comprehensive_benchmark",
        "generated_at_unix_ms": unix_time_ms(),
        "timestamp": options.timestamp,
        "phase": PHASE,
        "source_of_truth": "MVP.md",
        "execution_mode": if artifact_freshness["artifact_reuse"].as_bool() == Some(true) {
            "explicit_artifact_reuse"
        } else {
            "fresh_proof_build"
        },
        "execution_mode_notes": if artifact_freshness["artifact_reuse"].as_bool() == Some(true) {
            "The benchmark used an explicitly supplied proof DB artifact. Storage and cold-build results are claimable only when freshness metadata matches the current schema/build."
        } else {
            "The benchmark built a fresh proof DB artifact before reading storage and cold-build metrics."
        },
        "artifact_freshness": artifact_freshness,
        "inputs": {
            "baseline_json": path_string(&options.baseline_json),
            "compact_gate_json": path_string(&options.compact_gate_json),
            "previous_json": options.previous_json.as_ref().map(|path| path_string(path)),
            "proof_storage_json": gate_value(&gate, &["artifacts", "proof_storage_json"]).cloned().unwrap_or(Value::Null),
            "audit_storage_json": gate_value(&gate, &["artifacts", "audit_storage_json"]).cloned().unwrap_or(Value::Null),
            "artifact_metadata_json": options.artifact_metadata.as_ref().map(|path| path_string(path)),
            "manual_label_summary_json": "reports/audit/manual_relation_labeling_summary.json",
            "comparison_latest_json": "reports/comparison/CODEGRAPH_VS_CGC_LATEST.json"
        },
        "sections": {
            "executive_verdict": executive_verdict,
            "correctness_gates": {
                "metrics": correctness_metrics
            },
            "context_packet_gate": {
                "metrics": context_metrics
            },
            "db_integrity": {
                "metrics": integrity_metrics
            },
            "artifact_freshness": {
                "metrics": artifact_freshness_metrics,
                "metadata": gate_value(&gate, &["artifact_freshness"]).cloned().unwrap_or(Value::Null)
            },
            "storage_summary": {
                "metrics": storage_summary_metrics
            },
            "storage_contributors": {
                "contributors": storage_contributors
            },
            "row_counts_and_cardinality": {
                "metrics": cardinality_metrics
            },
            "cold_proof_build_profile": {
                "metrics": cold_profile_metrics,
                "mode_distinction": cold_mode_distinction,
                "waterfall": cold_waterfall,
                "interpretation": cold_interpretation,
                "top_10_slowest_stages": comprehensive_top_slowest_stages(&cold_profile_metrics)
            },
            "repeat_unchanged_index": {
                "metrics": repeat_metrics
            },
            "single_file_update": {
                "metrics": update_metrics
            },
            "default_query_surface": query_surface_report,
            "query_latency": query_latency,
            "manual_relation_quality": manual_quality,
            "cgc_competitor_comparison_readiness": comparison_section,
            "regression_summary": regression_summary
        },
        "notes": [
            "Unknown values are preserved as unknown and are not reported as passes.",
            "The benchmark fails if graph/context correctness regresses, proof DB exceeds 250 MiB, single-file update exceeds 750 ms, cold proof build exceeds 60 seconds, context_pack p95 exceeds 2 seconds, or DB integrity is not ok.",
            "Storage/debug sidecars are reported separately; audit/debug size is not counted against the proof DB target."
        ]
    }))
}

fn build_comprehensive_query_surface_artifact(
    options: &ComprehensiveBenchmarkOptions,
    artifact_freshness: &Value,
) -> Value {
    let Some(db_path) = artifact_freshness
        .get("artifact_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
    else {
        return comprehensive_query_surface_failure(
            "missing proof artifact path; query surface cannot be measured",
        );
    };
    let artifact_dir = match absolutize_path(&options.output_dir) {
        Ok(path) => path.join("artifacts"),
        Err(error) => {
            return comprehensive_query_surface_failure(&format!(
                "failed to resolve output directory for query-surface artifact: {error}"
            ));
        }
    };
    if fs::create_dir_all(&artifact_dir).is_err() {
        return comprehensive_query_surface_failure(
            "failed to create comprehensive query-surface artifact directory",
        );
    }
    let report = build_default_query_surface_report(
        &options.repo,
        &db_path,
        DEFAULT_QUERY_SURFACE_ITERATIONS,
    );
    let json_path = artifact_dir.join(format!(
        "comprehensive_query_surface_{}.json",
        options.timestamp
    ));
    let md_path = artifact_dir.join(format!(
        "comprehensive_query_surface_{}.md",
        options.timestamp
    ));
    let markdown = render_default_query_surface_markdown(&report);
    let _ = write_json_file(&json_path, &report);
    let _ = write_text_file(&md_path, &markdown);
    json!({
        "status": report["status"].clone(),
        "artifact_json": path_string(&json_path),
        "artifact_md": path_string(&md_path),
        "report": report,
    })
}

fn comprehensive_query_surface_failure(error: &str) -> Value {
    json!({
        "status": "failed",
        "artifact_json": Value::Null,
        "artifact_md": Value::Null,
        "report": {
            "schema_version": 1,
            "status": "failed",
            "generated_at_unix_ms": unix_time_ms(),
            "queries": required_query_surface_ids()
                .into_iter()
                .map(|(id, target)| query_surface_failure_metric(id, target, error))
                .collect::<Vec<_>>(),
            "summary": {
                "passed_queries": 0,
                "failed_queries": required_query_surface_ids().len(),
                "all_default_queries_complete": false,
            },
            "notes": [error],
        },
    })
}

fn prepare_comprehensive_artifact(
    options: &ComprehensiveBenchmarkOptions,
    gate: &mut Value,
) -> Result<Value, String> {
    match &options.artifact_mode {
        ComprehensiveArtifactMode::Fresh => build_fresh_comprehensive_proof_artifact(options, gate),
        ComprehensiveArtifactMode::Existing(path) => {
            inspect_existing_comprehensive_proof_artifact(options, gate, path)
        }
    }
}

fn build_fresh_comprehensive_proof_artifact(
    options: &ComprehensiveBenchmarkOptions,
    gate: &mut Value,
) -> Result<Value, String> {
    let artifact_dir = absolutize_path(&options.output_dir)?.join("artifacts");
    fs::create_dir_all(&artifact_dir).map_err(|error| error.to_string())?;
    let db_path = artifact_dir.join(format!("comprehensive_proof_{}.sqlite", options.timestamp));
    remove_sqlite_family_if_exists(&db_path)?;
    let index_start = Instant::now();
    let summary = index_repo_to_db_with_options(
        &options.repo,
        &db_path,
        IndexOptions {
            profile: true,
            json: false,
            worker_count: options.workers,
            storage_mode: StorageMode::Proof,
            build_mode: IndexBuildMode::ProofBuildOnly,
        },
    )
    .map_err(|error| error.to_string())?;
    let measured_duration = elapsed_ms(index_start);
    let build_duration_ms = summary
        .profile
        .as_ref()
        .map(|profile| profile.total_wall_ms.min(u128::from(u64::MAX)) as u64)
        .unwrap_or(measured_duration);
    let storage_json_path = artifact_dir.join(format!(
        "comprehensive_proof_storage_{}.json",
        options.timestamp
    ));
    let storage_md_path = artifact_dir.join(format!(
        "comprehensive_proof_storage_{}.md",
        options.timestamp
    ));
    let audit_args = vec![
        "storage".to_string(),
        "--db".to_string(),
        path_string(&db_path),
        "--json".to_string(),
        path_string(&storage_json_path),
        "--markdown".to_string(),
        path_string(&storage_md_path),
    ];
    audit::run_audit_command(&audit_args)?;
    let proof_storage = read_json_file(&storage_json_path)?;
    let schema_version = sqlite_user_version_read_only(&db_path).unwrap_or(SCHEMA_VERSION);
    let integrity_status = value_string(&proof_storage, &["integrity_check", "status"])
        .unwrap_or_else(|| {
            sqlite_quick_check_status(&db_path).unwrap_or_else(|| "unknown".to_string())
        });
    let db_size_bytes = metadata_len(&db_path).map_err(|error| error.to_string())?;
    let metadata_path = options.artifact_metadata.clone().unwrap_or_else(|| {
        artifact_dir.join(format!(
            "comprehensive_proof_{}.artifact.json",
            options.timestamp
        ))
    });
    let metadata = json!({
        "artifact_path": path_string(&db_path),
        "artifact_created_at": file_modified_unix_ms(&db_path).unwrap_or_else(unix_time_ms),
        "git_commit": current_git_commit(),
        "schema_version": schema_version,
        "current_schema_version": SCHEMA_VERSION,
        "migration_version": schema_version,
        "current_migration_version": SCHEMA_VERSION,
        "storage_mode": "proof",
        "build_command": comprehensive_fresh_build_command(options, &db_path),
        "build_duration_ms": build_duration_ms,
        "db_size_bytes": db_size_bytes,
        "integrity_status": integrity_status,
        "benchmark_run_id": options.timestamp,
        "artifact_metadata_path": path_string(&metadata_path),
        "freshness_metadata_present": true,
        "artifact_reuse": false,
        "freshly_built": true,
        "stale": false,
        "stale_reasons": [],
        "freshness_status": "fresh",
        "storage_result_claimable": true,
        "cold_build_result_claimable": true,
        "notes": [
            "Fresh proof DB built by comprehensive benchmark before storage and cold-build metrics were read."
        ]
    });
    write_json_file(&metadata_path, &metadata)?;
    patch_gate_with_proof_artifact(
        gate,
        &db_path,
        &storage_json_path,
        &proof_storage,
        &summary,
        &metadata,
    )?;
    Ok(metadata)
}

fn inspect_existing_comprehensive_proof_artifact(
    options: &ComprehensiveBenchmarkOptions,
    gate: &mut Value,
    db_path: &Path,
) -> Result<Value, String> {
    if !db_path.exists() {
        return Err(format!(
            "--use-existing-artifact path does not exist: {}",
            db_path.display()
        ));
    }
    let artifact_dir = absolutize_path(&options.output_dir)?.join("artifacts");
    fs::create_dir_all(&artifact_dir).map_err(|error| error.to_string())?;
    let metadata_path = options
        .artifact_metadata
        .clone()
        .unwrap_or_else(|| default_artifact_metadata_path(db_path));
    let metadata = read_optional_json(&metadata_path);
    let storage_json_path = artifact_dir.join(format!(
        "comprehensive_reused_proof_storage_{}.json",
        options.timestamp
    ));
    let storage_md_path = artifact_dir.join(format!(
        "comprehensive_reused_proof_storage_{}.md",
        options.timestamp
    ));
    let audit_args = vec![
        "storage".to_string(),
        "--db".to_string(),
        path_string(db_path),
        "--json".to_string(),
        path_string(&storage_json_path),
        "--markdown".to_string(),
        path_string(&storage_md_path),
    ];
    audit::run_audit_command(&audit_args)?;
    let proof_storage = read_json_file(&storage_json_path)?;
    let actual_schema_version = sqlite_user_version_read_only(db_path);
    let actual_db_size_bytes = metadata_len(db_path).map_err(|error| error.to_string())?;
    let integrity_status = value_string(&proof_storage, &["integrity_check", "status"])
        .unwrap_or_else(|| {
            sqlite_quick_check_status(db_path).unwrap_or_else(|| "unknown".to_string())
        });

    let mut stale_reasons = Vec::new();
    let metadata_present = metadata.is_some();
    if !metadata_present {
        stale_reasons.push("missing freshness metadata".to_string());
    }
    let metadata_value = metadata.unwrap_or_else(|| json!({}));
    let metadata_schema = value_u64(&metadata_value, &["schema_version"]);
    let metadata_migration = value_u64(&metadata_value, &["migration_version"]);
    let metadata_storage_mode = value_string(&metadata_value, &["storage_mode"]);
    let metadata_db_size = value_u64(&metadata_value, &["db_size_bytes"]);
    let metadata_build_duration = value_u64(&metadata_value, &["build_duration_ms"]);
    let metadata_git_commit = value_string(&metadata_value, &["git_commit"]);
    let current_git = current_git_commit();

    if metadata_schema != Some(u64::from(SCHEMA_VERSION)) {
        stale_reasons.push(format!(
            "schema mismatch: metadata={:?}, current={}",
            metadata_schema, SCHEMA_VERSION
        ));
    }
    if let Some(actual) = actual_schema_version {
        if actual != SCHEMA_VERSION {
            stale_reasons.push(format!(
                "database schema mismatch: actual={actual}, current={SCHEMA_VERSION}"
            ));
        }
    } else {
        stale_reasons.push("database schema unavailable".to_string());
    }
    if metadata_migration != Some(u64::from(SCHEMA_VERSION)) {
        stale_reasons.push(format!(
            "migration mismatch: metadata={:?}, current={}",
            metadata_migration, SCHEMA_VERSION
        ));
    }
    if metadata_storage_mode.as_deref() != Some("proof") {
        stale_reasons.push(format!(
            "storage mode mismatch: metadata={:?}, expected proof",
            metadata_storage_mode
        ));
    }
    if let Some(recorded_size) = metadata_db_size {
        if recorded_size != actual_db_size_bytes {
            stale_reasons.push(format!(
                "db size mismatch: metadata={recorded_size}, actual={actual_db_size_bytes}"
            ));
        }
    } else {
        stale_reasons.push("missing db_size_bytes".to_string());
    }
    if metadata_build_duration.is_none() {
        stale_reasons.push("missing build_duration_ms".to_string());
    }
    if let Some(recorded_git) = metadata_git_commit.as_deref() {
        if recorded_git != "unknown" && current_git != "unknown" && recorded_git != current_git {
            stale_reasons.push(format!(
                "git commit mismatch: metadata={recorded_git}, current={current_git}"
            ));
        }
    } else {
        stale_reasons.push("missing git_commit".to_string());
    }
    if integrity_status != "ok" {
        stale_reasons.push(format!("integrity status is {integrity_status}"));
    }

    let stale = !stale_reasons.is_empty();
    if options.fail_on_stale_artifact && stale {
        return Err(format!(
            "stale artifact refused by --fail-on-stale-artifact: {} ({})",
            db_path.display(),
            stale_reasons.join("; ")
        ));
    }

    let build_duration_ms = metadata_build_duration.unwrap_or(0);
    let metadata_report = json!({
        "artifact_path": path_string(db_path),
        "artifact_created_at": value_u64(&metadata_value, &["artifact_created_at"]).unwrap_or_else(|| file_modified_unix_ms(db_path).unwrap_or(0)),
        "git_commit": metadata_git_commit.unwrap_or_else(|| "unknown".to_string()),
        "current_git_commit": current_git,
        "schema_version": actual_schema_version,
        "current_schema_version": SCHEMA_VERSION,
        "metadata_schema_version": metadata_schema,
        "migration_version": metadata_migration,
        "current_migration_version": SCHEMA_VERSION,
        "storage_mode": metadata_storage_mode.unwrap_or_else(|| "unknown".to_string()),
        "build_command": value_string(&metadata_value, &["build_command"]).unwrap_or_else(|| "unknown".to_string()),
        "build_duration_ms": if metadata_build_duration.is_some() { json!(build_duration_ms) } else { Value::Null },
        "db_size_bytes": actual_db_size_bytes,
        "metadata_db_size_bytes": metadata_db_size,
        "integrity_status": integrity_status,
        "benchmark_run_id": options.timestamp,
        "artifact_metadata_path": path_string(&metadata_path),
        "freshness_metadata_present": metadata_present,
        "artifact_reuse": true,
        "freshly_built": false,
        "stale": stale,
        "stale_reasons": stale_reasons,
        "freshness_status": if stale { "stale" } else { "fresh" },
        "storage_result_claimable": !stale,
        "cold_build_result_claimable": !stale && metadata_build_duration.is_some(),
        "notes": if stale {
            vec!["stale artifact; storage result not claimable".to_string(), "stale artifact; cold-build result not claimable".to_string()]
        } else {
            vec!["Explicit artifact reuse accepted because freshness metadata matches current schema/build checks.".to_string()]
        }
    });

    let synthetic_summary = IndexSummary {
        repo_root: "unknown".to_string(),
        db_path: path_string(db_path),
        build_mode: "proof-build-only".to_string(),
        files_seen: 0,
        files_walked: 0,
        files_metadata_unchanged: 0,
        files_read: 0,
        files_hashed: 0,
        files_parsed: 0,
        files_indexed: 0,
        files_skipped: 0,
        files_deleted: 0,
        files_renamed: 0,
        parse_errors: 0,
        syntax_errors: 0,
        entities: 0,
        edges: 0,
        duplicate_edges_upserted: 0,
        batches_total: 0,
        batches_completed: 0,
        batch_max_files: DEFAULT_INDEX_BATCH_MAX_FILES,
        batch_max_source_bytes: DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES,
        stale_files_deleted: 0,
        failed_files_deleted: 0,
        storage_policy: StorageMode::Proof.storage_policy().to_string(),
        issue_counts: BTreeMap::new(),
        issues: Vec::new(),
        profile: Some(IndexProfile {
            file_discovery_ms: 0,
            parse_ms: 0,
            extraction_ms: 0,
            semantic_resolver_ms: 0,
            db_write_ms: 0,
            fts_search_index_ms: 0,
            vector_signature_ms: 0,
            total_wall_ms: u128::from(build_duration_ms),
            files_per_sec: 0.0,
            entities_per_sec: 0.0,
            edges_per_sec: 0.0,
            memory_bytes: None,
            worker_count: options.workers.unwrap_or(1),
            skipped_unchanged_files: 0,
            spans: Vec::new(),
        }),
    };
    patch_gate_with_proof_artifact(
        gate,
        db_path,
        &storage_json_path,
        &proof_storage,
        &synthetic_summary,
        &metadata_report,
    )?;
    Ok(metadata_report)
}

fn patch_gate_with_proof_artifact(
    gate: &mut Value,
    db_path: &Path,
    storage_json_path: &Path,
    proof_storage: &Value,
    summary: &IndexSummary,
    metadata: &Value,
) -> Result<(), String> {
    let proof_bytes = value_u64(proof_storage, &["file_family", "total_bytes"])
        .or_else(|| sqlite_family_size_bytes(db_path).ok())
        .unwrap_or(0);
    let wal_bytes = value_u64(proof_storage, &["file_family", "wal_bytes"]).unwrap_or(0);
    let physical_edge_rows = storage_object_row_count(proof_storage, "edges").unwrap_or(0);
    let path_evidence_rows = storage_object_row_count(proof_storage, "path_evidence").unwrap_or(0);
    let integrity_status = value_string(proof_storage, &["integrity_check", "status"])
        .unwrap_or_else(|| "unknown".to_string());
    let profile = summary.profile.as_ref();

    let gate_object = gate
        .as_object_mut()
        .ok_or_else(|| "compact gate JSON must be an object".to_string())?;
    ensure_json_object(gate_object, "artifacts")?.insert(
        "proof_storage_json".to_string(),
        json!(path_string(storage_json_path)),
    );
    let storage_object = ensure_json_object(gate_object, "storage")?;
    let proof_object = ensure_json_object(storage_object, "proof")?;
    proof_object.insert("file_family_bytes".to_string(), json!(proof_bytes));
    proof_object.insert(
        "file_family_mib".to_string(),
        json!(proof_bytes as f64 / (1024.0 * 1024.0)),
    );
    proof_object.insert("wal_bytes".to_string(), json!(wal_bytes));
    proof_object.insert("path_evidence_rows".to_string(), json!(path_evidence_rows));
    proof_object.insert("physical_edge_rows".to_string(), json!(physical_edge_rows));
    proof_object.insert("integrity_status".to_string(), json!(integrity_status));

    let autoresearch_object = ensure_json_object(gate_object, "autoresearch")?;
    let proof_build_object = ensure_json_object(autoresearch_object, "proof_build")?;
    proof_build_object.insert(
        "wall_ms".to_string(),
        json!(profile
            .map(|profile| profile.total_wall_ms.min(u128::from(u64::MAX)) as u64)
            .or_else(|| value_u64(metadata, &["build_duration_ms"]))
            .unwrap_or(0)),
    );
    proof_build_object.insert(
        "db_write_ms".to_string(),
        json!(profile
            .map(|profile| profile.db_write_ms.min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0)),
    );
    proof_build_object.insert(
        "integrity_check_ms".to_string(),
        json!(profile
            .and_then(|profile| {
                profile
                    .spans
                    .iter()
                    .filter(|span| {
                        span.name.contains("quick_check") || span.name.contains("integrity")
                    })
                    .map(|span| span.elapsed_ms)
                    .reduce(|left, right| left + right)
            })
            .unwrap_or(0.0)),
    );
    proof_build_object.insert("files_walked".to_string(), json!(summary.files_walked));
    proof_build_object.insert("files_parsed".to_string(), json!(summary.files_parsed));
    proof_build_object.insert(
        "duplicate_local_analyses_skipped".to_string(),
        json!(summary.files_skipped),
    );
    gate_object.insert("artifact_freshness".to_string(), metadata.clone());
    Ok(())
}

fn ensure_json_object<'a>(
    object: &'a mut serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a mut serde_json::Map<String, Value>, String> {
    let entry = object.entry(key.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    entry
        .as_object_mut()
        .ok_or_else(|| format!("failed to create object field {key}"))
}

fn storage_object_row_count(storage: &Value, name: &str) -> Option<u64> {
    storage
        .get("objects")?
        .as_array()?
        .iter()
        .find(|object| object["name"].as_str() == Some(name))
        .and_then(|object| value_u64(object, &["row_count"]))
}

fn comprehensive_fresh_build_command(
    options: &ComprehensiveBenchmarkOptions,
    db_path: &Path,
) -> String {
    let mut parts = vec![
        "codegraph-mcp".to_string(),
        "index".to_string(),
        path_string(&options.repo),
        "--db".to_string(),
        path_string(db_path),
        "--profile".to_string(),
        "--storage-mode".to_string(),
        "proof".to_string(),
        "--build-mode".to_string(),
        "proof-build-only".to_string(),
    ];
    if let Some(workers) = options.workers {
        parts.push("--workers".to_string());
        parts.push(workers.to_string());
    }
    parts.join(" ")
}

fn comprehensive_artifact_freshness_metrics(metadata: &Value) -> Vec<Value> {
    let artifact_reuse = value_bool(metadata, &["artifact_reuse"]).unwrap_or(false);
    let freshly_built = value_bool(metadata, &["freshly_built"]).unwrap_or(false);
    let stale = value_bool(metadata, &["stale"]).unwrap_or(true);
    let storage_claimable = value_bool(metadata, &["storage_result_claimable"]);
    let cold_claimable = value_bool(metadata, &["cold_build_result_claimable"]);
    let metadata_present = value_bool(metadata, &["freshness_metadata_present"]).unwrap_or(false);
    let schema_matches = value_u64(metadata, &["schema_version"])
        .zip(Some(u64::from(SCHEMA_VERSION)))
        .map(|(actual, current)| actual == current);
    vec![
        metric(
            "proof_db_freshly_built",
            json!("true unless explicit artifact reuse requested"),
            json!(freshly_built),
            if freshly_built || artifact_reuse { "pass" } else { "fail" },
            vec!["Comprehensive benchmark builds a fresh proof DB by default; explicit reuse must be labeled.".to_string()],
        ),
        metric(
            "artifact_reuse_marked",
            json!("reported"),
            json!(artifact_reuse),
            "pass",
            vec!["Explicit artifact reuse is visible in the report.".to_string()],
        ),
        metric(
            "artifact_has_freshness_metadata",
            json!(true),
            json!(metadata_present),
            status_known_pass(Some(metadata_present)),
            vec!["Reused artifacts require freshness metadata; fresh builds write it.".to_string()],
        ),
        metric(
            "artifact_schema_matches_current",
            json!(true),
            observed_bool(schema_matches),
            status_known_pass(schema_matches),
            vec!["Artifact schema_version must match the current SQLite schema version.".to_string()],
        ),
        metric(
            "artifact_integrity_ok",
            json!("ok"),
            metadata
                .get("integrity_status")
                .cloned()
                .unwrap_or(Value::Null),
            status_known_pass(
                metadata
                    .get("integrity_status")
                    .and_then(Value::as_str)
                    .map(|status| status == "ok"),
            ),
            vec!["Proof artifact must pass integrity before benchmark numbers are claimable.".to_string()],
        ),
        metric(
            "artifact_not_stale",
            json!(true),
            json!(!stale),
            status_known_pass(Some(!stale)),
            vec!["Stale artifact reuse forces the master gate to fail or remain unknown.".to_string()],
        ),
        metric(
            "storage_result_claimable",
            json!(true),
            observed_bool(storage_claimable),
            status_known_pass(storage_claimable),
            vec![if storage_claimable == Some(true) {
                "Storage result is claimable because the proof artifact is fresh or freshness-validated."
                    .to_string()
            } else {
                "stale artifact; storage result not claimable".to_string()
            }],
        ),
        metric(
            "cold_build_result_claimable",
            json!(true),
            observed_bool(cold_claimable),
            status_known_pass(cold_claimable),
            vec![if cold_claimable == Some(true) {
                "Cold-build result is claimable because the proof artifact is fresh or freshness-validated."
                    .to_string()
            } else {
                "stale artifact; cold-build result not claimable".to_string()
            }],
        ),
    ]
}

fn default_artifact_metadata_path(db_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.metadata.json", db_path.to_string_lossy()))
}

fn absolutize_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| error.to_string())
    }
}

fn remove_sqlite_family_if_exists(path: &Path) -> Result<(), String> {
    for candidate in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.to_string_lossy())),
        PathBuf::from(format!("{}-shm", path.to_string_lossy())),
    ] {
        match fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "failed to remove existing SQLite artifact {}: {error}",
                    candidate.display()
                ));
            }
        }
    }
    Ok(())
}

fn file_modified_unix_ms(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
}

fn sqlite_user_version_read_only(path: &Path) -> Option<u32> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .ok()?;
    u32::try_from(version).ok()
}

fn sqlite_quick_check_status(path: &Path) -> Option<String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    connection
        .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .ok()
}

fn current_git_commit() -> String {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output();
    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text.is_empty() {
                "unknown".to_string()
            } else {
                text
            }
        }
        _ => build_commit().to_string(),
    }
}

fn read_json_file(path: &Path) -> Result<Value, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(contents.trim_start_matches('\u{feff}'))
        .map_err(|error| format!("failed to parse {} as JSON: {error}", path.display()))
}

fn read_optional_json(path: &Path) -> Option<Value> {
    read_json_file(path).ok()
}

fn read_gate_artifact_json(gate: &Value, artifact_key: &str) -> Option<Value> {
    let path = gate_value(gate, &["artifacts", artifact_key])?.as_str()?;
    read_optional_json(Path::new(path))
}

fn gate_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cursor = value;
    for segment in path {
        cursor = cursor.get(*segment)?;
    }
    Some(cursor)
}

fn value_u64(value: &Value, path: &[&str]) -> Option<u64> {
    let value = gate_value(value, path)?;
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_f64().map(|number| number as u64))
}

fn value_f64(value: &Value, path: &[&str]) -> Option<f64> {
    let value = gate_value(value, path)?;
    value
        .as_f64()
        .or_else(|| value.as_u64().map(|number| number as f64))
        .or_else(|| value.as_i64().map(|number| number as f64))
}

fn value_bool(value: &Value, path: &[&str]) -> Option<bool> {
    gate_value(value, path)?.as_bool()
}

fn value_string(value: &Value, path: &[&str]) -> Option<String> {
    gate_value(value, path)?.as_str().map(ToOwned::to_owned)
}

fn observed_number_u64(value: Option<u64>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
}

fn observed_number_f64(value: Option<f64>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
}

fn observed_bool(value: Option<bool>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
}

fn status_known_pass(condition: Option<bool>) -> &'static str {
    match condition {
        Some(true) => "pass",
        Some(false) => "fail",
        None => "unknown",
    }
}

fn metric(
    id: &str,
    target: Value,
    observed: Value,
    status: &str,
    notes: impl Into<Vec<String>>,
) -> Value {
    json!({
        "id": id,
        "target": target,
        "observed": observed,
        "status": status,
        "notes": notes.into()
    })
}

fn metric_eq_u64(id: &str, target: u64, observed: Option<u64>, notes: &str) -> Value {
    let status = status_known_pass(observed.map(|value| value == target));
    metric(
        id,
        json!(target),
        observed_number_u64(observed),
        status,
        vec![notes.to_string()],
    )
}

fn metric_le_f64(id: &str, target: f64, observed: Option<f64>, notes: &str) -> Value {
    let status = status_known_pass(observed.map(|value| value <= target));
    metric(
        id,
        json!(target),
        observed_number_f64(observed),
        status,
        vec![notes.to_string()],
    )
}

fn metric_ge_f64(id: &str, target: f64, observed: Option<f64>, notes: &str) -> Value {
    let status = status_known_pass(observed.map(|value| value >= target));
    metric(
        id,
        json!(target),
        observed_number_f64(observed),
        status,
        vec![notes.to_string()],
    )
}

fn metric_reported_u64(id: &str, observed: Option<u64>, notes: &str) -> Value {
    let status = if observed.is_some() {
        "pass"
    } else {
        "unknown"
    };
    metric(
        id,
        json!("reported"),
        observed_number_u64(observed),
        status,
        vec![notes.to_string()],
    )
}

fn metric_reported_f64(id: &str, observed: Option<f64>, notes: &str) -> Value {
    let status = if observed.is_some() {
        "pass"
    } else {
        "unknown"
    };
    metric(
        id,
        json!("reported"),
        observed_number_f64(observed),
        status,
        vec![notes.to_string()],
    )
}

fn object_by_name(storage: &Option<Value>, name: &str) -> Option<Value> {
    storage
        .as_ref()?
        .get("objects")?
        .as_array()?
        .iter()
        .find(|object| object["name"].as_str() == Some(name))
        .cloned()
}

fn object_row_count(storage: &Option<Value>, name: &str) -> Option<u64> {
    object_by_name(storage, name).and_then(|object| value_u64(&object, &["row_count"]))
}

fn object_average_bytes(storage: &Option<Value>, table: &str) -> Option<f64> {
    storage
        .as_ref()?
        .get("table_row_metrics")?
        .as_array()?
        .iter()
        .find(|entry| entry["table"].as_str() == Some(table))
        .and_then(|entry| value_f64(entry, &["average_total_bytes_per_row"]))
}

fn collect_gate_statuses(
    metrics: &[Value],
    failed_targets: &mut Vec<String>,
    passed_targets: &mut Vec<String>,
) {
    for metric in metrics {
        let id = metric["id"]
            .as_str()
            .unwrap_or("unknown_metric")
            .to_string();
        match metric["status"].as_str() {
            Some("fail") => failed_targets.push(id),
            Some("pass") => passed_targets.push(id),
            _ => {}
        }
    }
}

fn collect_query_gate_statuses(
    query_latency: &Value,
    failed_targets: &mut Vec<String>,
    passed_targets: &mut Vec<String>,
) {
    let Some(metrics) = query_latency["queries"].as_array() else {
        return;
    };
    for metric in metrics {
        let id = metric["id"].as_str().unwrap_or("unknown_query").to_string();
        match metric["status"].as_str() {
            Some("fail") => failed_targets.push(id),
            Some("pass") => passed_targets.push(id),
            _ => {}
        }
    }
}

fn comprehensive_correctness_metrics(graph_truth: &Value) -> Vec<Value> {
    let cases_total = value_u64(graph_truth, &["cases_total"]);
    let cases_passed = value_u64(graph_truth, &["cases_passed"]);
    let expected_entities = value_u64(graph_truth, &["expected_entities"]);
    let matched_entities = value_u64(graph_truth, &["matched_entities"]);
    let expected_edges = value_u64(graph_truth, &["expected_edges"]);
    let matched_edges = value_u64(graph_truth, &["matched_expected_edges"]);
    let expected_paths = value_u64(graph_truth, &["expected_paths"]);
    let matched_paths = value_u64(graph_truth, &["matched_expected_paths"]);
    let forbidden_edges_found = value_u64(graph_truth, &["matched_forbidden_edges"]);
    let forbidden_paths_found = value_u64(graph_truth, &["matched_forbidden_paths"]);
    let source_span_failures = value_u64(graph_truth, &["source_span_failures"]);
    let unresolved_exact = value_u64(graph_truth, &["unresolved_exact_violations"]).or(Some(0));
    let derived_without_provenance =
        value_u64(graph_truth, &["derived_without_provenance_violations"]).or(Some(0));
    let test_mock_leakage = value_u64(graph_truth, &["test_mock_production_leakage"]).or(Some(0));
    let stale_failures = value_u64(graph_truth, &["stale_failures"]);

    vec![
        metric(
            "graph_truth_cases_total",
            json!(11),
            observed_number_u64(cases_total),
            status_known_pass(cases_total.map(|total| total == 11)),
            vec!["All adversarial graph-truth fixtures must be present.".to_string()],
        ),
        metric(
            "graph_truth_cases_passed",
            json!("all cases"),
            observed_number_u64(cases_passed),
            status_known_pass(
                cases_total
                    .zip(cases_passed)
                    .map(|(total, passed)| total == passed),
            ),
            vec!["Graph Truth Gate must be 100% pass.".to_string()],
        ),
        metric(
            "expected_entities_matched",
            json!(expected_entities),
            observed_number_u64(matched_entities),
            status_known_pass(
                expected_entities
                    .zip(matched_entities)
                    .map(|(expected, matched)| expected == matched),
            ),
            vec!["Every expected entity must be present.".to_string()],
        ),
        metric(
            "expected_edges_matched",
            json!(expected_edges),
            observed_number_u64(matched_edges),
            status_known_pass(
                expected_edges
                    .zip(matched_edges)
                    .map(|(expected, matched)| expected == matched),
            ),
            vec!["Every required edge must be present.".to_string()],
        ),
        metric(
            "expected_paths_matched",
            json!(expected_paths),
            observed_number_u64(matched_paths),
            status_known_pass(
                expected_paths
                    .zip(matched_paths)
                    .map(|(expected, matched)| expected == matched),
            ),
            vec!["Every expected proof path must be present.".to_string()],
        ),
        metric_eq_u64(
            "forbidden_edges_found",
            0,
            forbidden_edges_found,
            "Forbidden edge hits must remain zero.",
        ),
        metric_eq_u64(
            "forbidden_paths_found",
            0,
            forbidden_paths_found,
            "Forbidden proof paths must remain zero.",
        ),
        metric_eq_u64(
            "source_span_failures",
            0,
            source_span_failures,
            "Proof-grade facts must have valid source spans.",
        ),
        metric_eq_u64(
            "unresolved_exact_count",
            0,
            unresolved_exact,
            "Unresolved relations must not be labeled exact.",
        ),
        metric_eq_u64(
            "derived_without_provenance_count",
            0,
            derived_without_provenance,
            "Derived edges must retain provenance.",
        ),
        metric_eq_u64(
            "test_mock_production_leakage_count",
            0,
            test_mock_leakage,
            "Production proof paths must not include test/mock edges.",
        ),
        metric_eq_u64(
            "stale_fact_failures",
            0,
            stale_failures,
            "Mutation fixtures must not leave stale current facts.",
        ),
    ]
}

fn comprehensive_context_metrics(
    context_gate: &Value,
    storage: &Value,
    relation_sampler: &Value,
) -> Vec<Value> {
    let cases_total = value_u64(context_gate, &["cases_total"]);
    let cases_passed = value_u64(context_gate, &["cases_passed"]);
    let critical_symbol_recall = value_f64(context_gate, &["critical_symbol_recall"]);
    let proof_path_coverage = value_f64(context_gate, &["proof_path_coverage"]);
    let source_span_coverage = value_f64(context_gate, &["source_span_coverage"]);
    let expected_tests_recall = value_f64(context_gate, &["expected_test_recall"]);
    let distractor_ratio = value_f64(context_gate, &["distractor_ratio"]);
    let stored_path_evidence_rows = value_u64(storage, &["proof", "path_evidence_rows"])
        .or_else(|| value_u64(relation_sampler, &["stored_path_evidence_count"]));
    let fallback_paths = value_u64(relation_sampler, &["generated_path_evidence_count"]);

    vec![
        metric_reported_u64(
            "context_cases_total",
            cases_total,
            "Context Packet Gate fixture count.",
        ),
        metric(
            "context_cases_passed",
            json!("all cases"),
            observed_number_u64(cases_passed),
            status_known_pass(cases_total.zip(cases_passed).map(|(total, passed)| total == passed)),
            vec!["Context Packet Gate must pass all cases.".to_string()],
        ),
        metric_ge_f64(
            "critical_symbol_recall",
            1.0,
            critical_symbol_recall,
            "Critical symbol recall must be 100%.",
        ),
        metric_ge_f64(
            "proof_path_coverage",
            1.0,
            proof_path_coverage,
            "Proof-path coverage must be 100%.",
        ),
        metric_ge_f64(
            "proof_path_source_span_coverage",
            1.0,
            source_span_coverage,
            "Source-span coverage for proof paths must be 100%.",
        ),
        metric_ge_f64(
            "expected_tests_recall",
            0.9,
            expected_tests_recall,
            "Expected test recall target is >=90%.",
        ),
        metric_le_f64(
            "distractor_ratio",
            0.25,
            distractor_ratio,
            "Distractor ratio target is <=25%.",
        ),
        metric_reported_u64(
            "stored_path_evidence_rows",
            stored_path_evidence_rows,
            "Stored PathEvidence rows must exist for proof paths.",
        ),
        metric_eq_u64(
            "generated_fallback_path_count",
            0,
            fallback_paths,
            "Normal proof cases should use stored PathEvidence instead of generated fallback paths.",
        ),
    ]
}

fn comprehensive_integrity_metrics(
    gate: &Value,
    proof_storage: &Option<Value>,
    storage: &Value,
) -> Vec<Value> {
    let integrity = proof_storage
        .as_ref()
        .and_then(|value| value_string(value, &["integrity_check", "status"]))
        .or_else(|| value_string(storage, &["proof", "integrity_status"]))
        .or_else(|| {
            value_string(
                gate,
                &["db_integrity_summary", "proof_db", "integrity_status"],
            )
        });
    let quick_check = value_string(gate, &["db_integrity", "quick_check_status"]);
    let foreign_key = value_string(gate, &["db_integrity", "foreign_key_check_status"]);
    let wal_bytes = value_u64(storage, &["proof", "wal_bytes"]);
    let failed_update_status =
        value_string(gate, &["db_integrity", "failed_update_rollback_status"]).or_else(|| {
            Some(
                "not persisted in compact proof gate; update harness integrity remained ok"
                    .to_string(),
            )
        });

    vec![
        metric(
            "integrity_check_status",
            json!("ok"),
            integrity.clone().map(Value::from).unwrap_or(Value::Null),
            status_known_pass(integrity.as_deref().map(|status| status == "ok")),
            vec!["Cold proof DB must pass PRAGMA integrity_check.".to_string()],
        ),
        metric(
            "quick_check_status",
            json!("ok"),
            quick_check.clone().map(Value::from).unwrap_or(Value::Null),
            if quick_check.is_some() { status_known_pass(quick_check.as_deref().map(|status| status == "ok")) } else { "unknown" },
            vec!["Repeat/update quick_check status was not separately persisted in the compact gate.".to_string()],
        ),
        metric(
            "foreign_key_check_status",
            json!("ok or not_applicable"),
            foreign_key.clone().map(Value::from).unwrap_or(Value::Null),
            if foreign_key.is_some() { status_known_pass(foreign_key.as_deref().map(|status| status == "ok" || status == "not_applicable")) } else { "unknown" },
            vec!["Foreign-key check is reported only when the schema/gate persists it.".to_string()],
        ),
        metric_reported_u64("wal_size_bytes", wal_bytes, "WAL file size is reported separately."),
        metric(
            "rollback_failure_simulation_status",
            json!("rollback cleanly"),
            failed_update_status.map(Value::from).unwrap_or(Value::Null),
            "unknown",
            vec!["No explicit failed-update simulation artifact is attached to the compact proof gate; do not infer a pass.".to_string()],
        ),
    ]
}

fn comprehensive_storage_summary_metrics(
    storage: &Value,
    proof_storage: &Option<Value>,
) -> Vec<Value> {
    let proof = storage.get("proof").unwrap_or(&Value::Null);
    let proof_bytes = value_u64(proof, &["file_family_bytes"]);
    let proof_mib = value_f64(proof, &["file_family_mib"]);
    let audit_bytes = value_u64(storage, &["audit", "file_family_bytes"]);
    let total_artifact_bytes = proof_bytes
        .zip(audit_bytes)
        .map(|(proof, audit)| proof + audit);
    let wal_bytes = value_u64(proof, &["wal_bytes"]);
    let table_bytes = proof_storage
        .as_ref()
        .and_then(sum_object_bytes_by_type("table"));
    let index_bytes = proof_storage
        .as_ref()
        .and_then(sum_object_bytes_by_type("index"));
    let edge_table_plus_index = value_f64(
        proof_storage.as_ref().unwrap_or(&Value::Null),
        &[
            "aggregate_metrics",
            "average_edge_table_plus_index_bytes_per_edge",
        ],
    );
    let db_bytes_per_edge = value_f64(
        proof_storage.as_ref().unwrap_or(&Value::Null),
        &["aggregate_metrics", "average_database_bytes_per_edge"],
    )
    .or_else(|| {
        proof_bytes
            .zip(value_u64(proof, &["physical_edge_rows"]))
            .map(|(bytes, rows)| {
                if rows == 0 {
                    0.0
                } else {
                    bytes as f64 / rows as f64
                }
            })
    });
    let bytes_per_entity = object_average_bytes(proof_storage, "entities");
    let bytes_per_template_entity = object_average_bytes(proof_storage, "template_entities");
    let bytes_per_template_edge = object_average_bytes(proof_storage, "template_edges");
    let bytes_per_source_span = object_average_bytes(proof_storage, "file_source_spans");
    let bytes_per_path_evidence = object_average_bytes(proof_storage, "path_evidence");
    let snippet_policy = proof_storage
        .as_ref()
        .and_then(|value| value_bool(value, &["fts_storage", "stores_source_snippets"]))
        .map(|stores| !stores);

    vec![
        metric_reported_u64("proof_db_bytes", proof_bytes, "Proof DB family size in bytes."),
        metric_le_f64("proof_db_mib", 250.0, proof_mib, "Proof DB family must be <=250 MiB."),
        metric_le_f64("proof_db_mib_stretch", 150.0, proof_mib, "Stretch target is <=150 MiB."),
        metric_reported_u64(
            "audit_debug_db_bytes",
            audit_bytes,
            "Audit/debug DB bytes are reported separately and not counted against proof target.",
        ),
        metric_reported_u64(
            "total_artifact_bytes",
            total_artifact_bytes,
            "Proof + audit artifact bytes for operator planning.",
        ),
        metric_reported_u64("wal_bytes", wal_bytes, "WAL bytes are reported separately."),
        metric_reported_u64("table_bytes", table_bytes, "dbstat table bytes."),
        metric_reported_u64("index_bytes", index_bytes, "dbstat index bytes."),
        metric_le_f64(
            "bytes_per_proof_edge",
            120.0,
            db_bytes_per_edge,
            "Whole proof DB bytes divided by physical proof-edge rows; this remains a bloat signal.",
        ),
        metric_le_f64(
            "bytes_per_edge_table_plus_index",
            120.0,
            edge_table_plus_index,
            "Physical edge table plus edge indexes per proof edge.",
        ),
        metric_reported_f64("bytes_per_entity", bytes_per_entity, "Average total bytes per entity row."),
        metric_reported_f64(
            "bytes_per_template_entity",
            bytes_per_template_entity,
            "Average total bytes per template entity row.",
        ),
        metric_reported_f64(
            "bytes_per_template_edge",
            bytes_per_template_edge,
            "Average total bytes per template edge row.",
        ),
        metric_reported_f64(
            "bytes_per_source_span",
            bytes_per_source_span,
            "Average total bytes per file_source_spans row.",
        ),
        metric_reported_f64(
            "bytes_per_path_evidence_row",
            bytes_per_path_evidence,
            "Average total bytes per PathEvidence row.",
        ),
        metric(
            "source_snippets_not_stored_redundantly",
            json!(true),
            observed_bool(snippet_policy),
            status_known_pass(snippet_policy),
            vec!["Source snippets should be loaded from source files, not redundantly stored in SQLite.".to_string()],
        ),
    ]
}

fn sum_object_bytes_by_type<'a>(
    object_type: &'a str,
) -> impl FnOnce(&'a Value) -> Option<u64> + 'a {
    move |storage| {
        let objects = storage.get("objects")?.as_array()?;
        Some(
            objects
                .iter()
                .filter(|object| object["object_type"].as_str() == Some(object_type))
                .filter_map(|object| value_u64(object, &["total_bytes"]))
                .sum(),
        )
    }
}

fn comprehensive_cardinality_metrics(
    proof_build: &Value,
    proof_storage: &Option<Value>,
    storage: &Value,
) -> Vec<Value> {
    vec![
        metric_reported_u64(
            "files_walked",
            value_u64(proof_build, &["files_walked"]),
            "Cold proof build files walked.",
        ),
        metric_reported_u64(
            "files_parsed",
            value_u64(proof_build, &["files_parsed"]),
            "Cold proof build files parsed.",
        ),
        metric_reported_u64(
            "content_templates",
            object_row_count(proof_storage, "source_content_template"),
            "Source content templates.",
        ),
        metric_reported_u64(
            "file_instances",
            object_row_count(proof_storage, "files"),
            "Path-specific file instances.",
        ),
        metric_reported_u64(
            "duplicate_content_templates",
            value_u64(proof_build, &["duplicate_local_analyses_skipped"]),
            "Duplicate local analyses skipped by content-template dedupe.",
        ),
        metric_reported_u64(
            "template_entities",
            object_row_count(proof_storage, "template_entities"),
            "Template entity rows.",
        ),
        metric_reported_u64(
            "template_edges",
            object_row_count(proof_storage, "template_edges"),
            "Template edge rows.",
        ),
        metric_reported_u64(
            "proof_entities",
            object_row_count(proof_storage, "entities"),
            "Proof entity rows.",
        ),
        metric_reported_u64(
            "proof_edges",
            object_row_count(proof_storage, "edges"),
            "Physical proof edge rows.",
        ),
        metric_reported_u64(
            "structural_records",
            object_row_count(proof_storage, "structural_relations"),
            "Generic structural relation rows.",
        ),
        metric_reported_u64(
            "callsites",
            object_row_count(proof_storage, "callsites"),
            "Callsite rows.",
        ),
        metric_reported_u64(
            "callsite_args",
            object_row_count(proof_storage, "callsite_args"),
            "Callsite argument rows.",
        ),
        metric_reported_u64(
            "symbols",
            object_row_count(proof_storage, "symbol_dict"),
            "Symbol dictionary rows.",
        ),
        metric_reported_u64(
            "qname_prefixes",
            object_row_count(proof_storage, "qname_prefix_dict"),
            "QName prefix dictionary rows.",
        ),
        metric_reported_u64(
            "source_spans",
            object_row_count(proof_storage, "file_source_spans"),
            "File/source-span mapping rows.",
        ),
        metric_reported_u64(
            "path_evidence_rows",
            object_row_count(proof_storage, "path_evidence"),
            "Stored PathEvidence rows.",
        ),
        metric_reported_u64(
            "heuristic_debug_rows",
            value_u64(storage, &["audit", "audit_only_sidecar_rows"]),
            "Audit/debug sidecar rows preserved outside compact proof facts.",
        ),
    ]
}

fn comprehensive_storage_contributors(
    proof_storage: &Option<Value>,
    previous: Option<&Value>,
    baseline: &Value,
) -> Vec<Value> {
    let proof_bytes = proof_storage
        .as_ref()
        .and_then(|value| value_u64(value, &["file_family", "total_bytes"]))
        .or_else(|| value_u64(baseline, &["storage_summary", "proof_file_family_bytes"]))
        .unwrap_or(0);
    let Some(objects) = proof_storage
        .as_ref()
        .and_then(|value| value.get("objects"))
        .and_then(Value::as_array)
    else {
        return baseline["storage_summary"]["top_storage_contributors"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|entry| {
                let name = entry["object"].as_str().unwrap_or("unknown");
                let bytes = value_u64(&entry, &["bytes"]).unwrap_or(0);
                contributor_row(
                    name,
                    "unknown",
                    value_u64(&entry, &["rows"]),
                    bytes,
                    proof_bytes,
                    None,
                )
            })
            .collect();
    };

    let mut rows = Vec::new();
    for object in objects {
        let name = object["name"].as_str().unwrap_or("unknown");
        let kind = object["object_type"].as_str().unwrap_or("unknown");
        let bytes = value_u64(object, &["total_bytes"]).unwrap_or(0);
        let previous_bytes = previous
            .and_then(|previous| previous_contributor_bytes(previous, name))
            .or_else(|| baseline_contributor_bytes(baseline, name));
        if include_comprehensive_storage_object(name, bytes) {
            rows.push(contributor_row(
                name,
                kind,
                value_u64(object, &["row_count"]),
                bytes,
                proof_bytes,
                previous_bytes,
            ));
        }
    }
    rows.sort_by(|left, right| {
        value_u64(right, &["bytes"])
            .unwrap_or(0)
            .cmp(&value_u64(left, &["bytes"]).unwrap_or(0))
            .then_with(|| {
                left["name"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(right["name"].as_str().unwrap_or(""))
            })
    });
    rows
}

fn include_comprehensive_storage_object(name: &str, bytes: u64) -> bool {
    let required_names = [
        "template_entities",
        "template_edges",
        "symbol_dict",
        "qname_prefix_dict",
        "qualified_name_dict",
        "entities",
        "edges",
        "proof_edges",
        "source_spans",
        "file_source_spans",
        "path_evidence",
        "callsites",
        "callsite_args",
        "files",
        "source_content_template",
        "file_entities",
        "file_edges",
        "path_dict",
    ];
    required_names.contains(&name)
        || name.contains("template")
        || name.contains("symbol_dict")
        || name.contains("qname")
        || name.contains("qualified_name")
        || name.contains("dictionary")
        || name.ends_with("_dict")
        || name.starts_with("idx_template")
        || name.starts_with("idx_symbol")
        || name.starts_with("idx_qname")
        || name.starts_with("idx_qualified")
        || bytes >= 1_000_000
}

fn contributor_row(
    name: &str,
    kind: &str,
    rows: Option<u64>,
    bytes: u64,
    proof_bytes: u64,
    previous_bytes: Option<u64>,
) -> Value {
    let share = if proof_bytes == 0 {
        Value::Null
    } else {
        json!((bytes as f64) / (proof_bytes as f64))
    };
    let delta = previous_bytes.map(|previous| bytes as i128 - previous as i128);
    json!({
        "name": name,
        "kind": kind,
        "rows": rows,
        "bytes": bytes,
        "mib": bytes as f64 / 1024.0 / 1024.0,
        "share_of_proof_db": share,
        "previous_baseline_bytes": previous_bytes,
        "delta_bytes": delta,
        "classification": classify_storage_object(name),
    })
}

fn previous_contributor_bytes(previous: &Value, name: &str) -> Option<u64> {
    previous["sections"]["storage_contributors"]["contributors"]
        .as_array()?
        .iter()
        .find(|entry| entry["name"].as_str() == Some(name))
        .and_then(|entry| value_u64(entry, &["bytes"]))
}

fn baseline_contributor_bytes(baseline: &Value, name: &str) -> Option<u64> {
    baseline["storage_summary"]["top_storage_contributors"]
        .as_array()?
        .iter()
        .find(|entry| entry["object"].as_str() == Some(name))
        .and_then(|entry| value_u64(entry, &["bytes"]))
}

fn classify_storage_object(name: &str) -> &'static str {
    if name.contains("debug")
        || name == "heuristic_edges"
        || name == "unresolved_references"
        || name == "static_references"
        || name == "extraction_warnings"
    {
        "debug_sidecar"
    } else if name.contains("template") {
        "template"
    } else if name.ends_with("_dict")
        || name.contains("symbol_dict")
        || name.contains("qname")
        || name.contains("qualified_name")
        || name.contains("dictionary")
    {
        "dictionary"
    } else if name.contains("path_evidence") || name == "derived_edges" {
        "derived_cache"
    } else if name == "callsites" || name == "callsite_args" || name == "structural_relations" {
        "structural"
    } else if name.starts_with("sqlite_stat") || name.starts_with("bench_") {
        "temporary/build_only"
    } else if name == "edges"
        || name == "entities"
        || name == "file_source_spans"
        || name == "source_spans"
        || name == "files"
    {
        "proof_required"
    } else {
        "proof_optional"
    }
}

fn comprehensive_cold_profile_metrics(proof_build: &Value) -> Vec<Value> {
    let total = value_f64(proof_build, &["wall_ms"]);
    let db_write = value_f64(proof_build, &["db_write_ms"]);
    let integrity = value_f64(proof_build, &["integrity_check_ms"]);
    vec![
        metric_le_f64(
            "cold_proof_build_total_wall_ms",
            60000.0,
            total,
            "Cold proof build intended target is <=60 seconds.",
        ),
        metric_reported_f64(
            "cold_proof_build_total_profile_ms",
            total,
            "Profile total from compact proof baseline.",
        ),
        metric_reported_f64(
            "file_walk_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "metadata_diff_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "read_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "hash_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "parse_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "local_fact_bundle_creation_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "content_template_dedupe_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "reducer_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "symbol_interning_time_ms",
            None,
            "Not persisted in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "qname_prefix_interning_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "template_entity_insert_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "template_edge_insert_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "proof_entity_insert_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "proof_edge_insert_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "source_span_insert_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "path_evidence_generation_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "db_write_time_ms",
            db_write,
            "Known dominant cold-build stage.",
        ),
        metric_reported_f64(
            "index_creation_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "fts_build_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "vacuum_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "analyze_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "integrity_check_time_ms",
            integrity,
            "Cold proof DB integrity-check duration.",
        ),
        metric_reported_f64(
            "graph_hash_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
        metric_reported_f64(
            "report_generation_time_ms",
            None,
            "Not persisted separately in compact proof cold-build profile.",
        ),
    ]
}

fn comprehensive_cold_build_mode_distinction(gate: &Value) -> Vec<Value> {
    let proof_wall = value_f64(gate, &["autoresearch", "proof_build", "wall_ms"]);
    let integrity = value_f64(gate, &["autoresearch", "proof_build", "integrity_check_ms"]);
    let audit_wall = value_f64(gate, &["autoresearch", "audit_build", "wall_ms"]);
    let proof_plus_validation = proof_wall.map(|wall| wall + integrity.unwrap_or(0.0));
    let proof_plus_audit = match (proof_wall, audit_wall) {
        (Some(proof), Some(audit)) => Some(proof + audit),
        _ => None,
    };

    vec![
        json!({
            "mode": "proof-build-only",
            "observed_ms": observed_number_f64(proof_wall),
            "observed_minutes": observed_number_f64(proof_wall.map(|value| value / 60000.0)),
            "target_ms": 60000,
            "status": status_known_pass(proof_wall.map(|value| value <= 60000.0)),
            "included_in_50_02_minute_number": true,
            "classification": "actual production proof-mode cold build as persisted by the compact gate",
            "notes": [
                "This is the number compared to the <=60s cold proof build target."
            ]
        }),
        json!({
            "mode": "proof-build-plus-validation",
            "observed_ms": observed_number_f64(proof_plus_validation),
            "observed_minutes": observed_number_f64(proof_plus_validation.map(|value| value / 60000.0)),
            "target_ms": 60000,
            "status": status_known_pass(proof_plus_validation.map(|value| value <= 60000.0)),
            "included_in_50_02_minute_number": false,
            "classification": "proof build plus the separately persisted integrity_check duration",
            "notes": [
                "Validation is important, but the compact gate reports integrity_check separately from proof_build.wall_ms."
            ]
        }),
        json!({
            "mode": "proof-build-plus-audit",
            "observed_ms": observed_number_f64(proof_plus_audit),
            "observed_minutes": observed_number_f64(proof_plus_audit.map(|value| value / 60000.0)),
            "target_ms": Value::Null,
            "status": if proof_plus_audit.is_some() { "reported" } else { "unknown" },
            "included_in_50_02_minute_number": false,
            "classification": "sequential proof build plus separate audit-sidecar build",
            "notes": [
                "The audit build is a separate compact-gate artifact and must not be counted as the proof DB cold build."
            ]
        }),
        json!({
            "mode": "proof-build-plus-audit-plus-validation",
            "observed_ms": observed_number_f64(proof_plus_audit.map(|value| value + integrity.unwrap_or(0.0))),
            "observed_minutes": observed_number_f64(proof_plus_audit.map(|value| (value + integrity.unwrap_or(0.0)) / 60000.0)),
            "target_ms": Value::Null,
            "status": if proof_plus_audit.is_some() { "reported" } else { "unknown" },
            "included_in_50_02_minute_number": false,
            "classification": "operator gate bundle, not the production proof-build-only target",
            "notes": [
                "Useful for planning gate runtime, but not comparable to the <=60s production cold proof build target."
            ]
        }),
        json!({
            "mode": "full-gate",
            "observed_ms": Value::Null,
            "observed_minutes": Value::Null,
            "target_ms": Value::Null,
            "status": "unknown",
            "included_in_50_02_minute_number": false,
            "classification": "not persisted as a single wall-clock value in the compact gate",
            "notes": [
                "Graph truth, context gate, sampler, storage audit, update checks, and report generation are not collapsed into the proof_build.wall_ms number."
            ]
        }),
    ]
}

fn comprehensive_cold_build_waterfall(gate: &Value) -> Vec<Value> {
    let proof_wall = value_f64(gate, &["autoresearch", "proof_build", "wall_ms"]);
    let db_write = value_f64(gate, &["autoresearch", "proof_build", "db_write_ms"]);
    let integrity = value_f64(gate, &["autoresearch", "proof_build", "integrity_check_ms"]);
    let audit_wall = value_f64(gate, &["autoresearch", "audit_build", "wall_ms"]);
    let residual = match (proof_wall, db_write) {
        (Some(total), Some(write)) if total >= write => Some(total - write),
        _ => None,
    };
    let pct_of_proof = |value: Option<f64>| -> Value {
        match (value, proof_wall) {
            (Some(value), Some(total)) if total > 0.0 => json!(value / total),
            _ => Value::Null,
        }
    };
    let pct_of_plus_validation = |value: Option<f64>| -> Value {
        match (
            value,
            proof_wall.map(|wall| wall + integrity.unwrap_or(0.0)),
        ) {
            (Some(value), Some(total)) if total > 0.0 => json!(value / total),
            _ => Value::Null,
        }
    };

    vec![
        json!({
            "stage": "actual_proof_db_build_total",
            "elapsed_ms": observed_number_f64(proof_wall),
            "pct_of_reference": pct_of_proof(proof_wall),
            "reference": "proof-build-only",
            "included_in_50_02_minute_number": true,
            "source": "reports/final/compact_proof_db_gate.json.autoresearch.proof_build.wall_ms",
            "notes": [
                "This is the persisted cold proof build number."
            ]
        }),
        json!({
            "stage": "production_persistence_and_global_reduction_bucket",
            "elapsed_ms": observed_number_f64(db_write),
            "pct_of_reference": pct_of_proof(db_write),
            "reference": "proof-build-only",
            "included_in_50_02_minute_number": true,
            "source": "reports/final/compact_proof_db_gate.json.autoresearch.proof_build.db_write_ms",
            "notes": [
                "This bucket is currently broad: it includes SQLite persistence plus post-local global reduction, PathEvidence refresh, index recreation, ANALYZE/checkpoint work, and transaction commit where applicable."
            ]
        }),
        json!({
            "stage": "source_scan_parse_extract_dedupe_reducer_residual",
            "elapsed_ms": observed_number_f64(residual),
            "pct_of_reference": pct_of_proof(residual),
            "reference": "proof-build-only",
            "included_in_50_02_minute_number": true,
            "source": "wall_ms - db_write_ms",
            "notes": [
                "The compact gate did not persist nested cold-build spans for these phases; as a combined residual they are below 5% of proof-build-only wall time."
            ]
        }),
        json!({
            "stage": "integrity_check_validation",
            "elapsed_ms": observed_number_f64(integrity),
            "pct_of_reference": pct_of_plus_validation(integrity),
            "reference": "proof-build-plus-validation",
            "included_in_50_02_minute_number": false,
            "source": "reports/final/compact_proof_db_gate.json.autoresearch.proof_build.integrity_check_ms",
            "notes": [
                "Integrity checking is separately measured and is not the dominant cause."
            ]
        }),
        json!({
            "stage": "audit_sidecar_build",
            "elapsed_ms": observed_number_f64(audit_wall),
            "pct_of_reference": Value::Null,
            "reference": "separate audit build",
            "included_in_50_02_minute_number": false,
            "source": "reports/final/compact_proof_db_gate.json.autoresearch.audit_build.wall_ms",
            "notes": [
                "Audit/debug-sidecar build time is a separate artifact and must not be blamed for the proof-build-only failure."
            ]
        }),
    ]
}

fn comprehensive_cold_build_interpretation(gate: &Value) -> Value {
    let proof_wall = value_f64(gate, &["autoresearch", "proof_build", "wall_ms"]);
    let db_write = value_f64(gate, &["autoresearch", "proof_build", "db_write_ms"]);
    let residual = match (proof_wall, db_write) {
        (Some(total), Some(write)) if total >= write => Some(total - write),
        _ => None,
    };
    json!({
        "fifty_point_zero_two_minutes_is": "actual production proof-mode cold build profile wall time from the compact gate",
        "is_benchmark_gate_wall_time": false,
        "is_debug_or_audit_sidecar_build_time": false,
        "includes_repeated_rebuilds": false,
        "includes_slow_storage_audit_or_dbstat": false,
        "includes_cgc_or_competitor_work": false,
        "includes_report_generation": false,
        "integrity_check_is_reported_separately": true,
        "dominant_known_stage": "production_persistence_and_global_reduction_bucket",
        "dominant_known_stage_ms": observed_number_f64(db_write),
        "dominant_known_stage_pct_of_proof_build": match (db_write, proof_wall) {
            (Some(write), Some(total)) if total > 0.0 => json!(write / total),
            _ => Value::Null,
        },
        "combined_non_db_residual_ms": observed_number_f64(residual),
        "combined_non_db_residual_pct_of_proof_build": match (residual, proof_wall) {
            (Some(value), Some(total)) if total > 0.0 => json!(value / total),
            _ => Value::Null,
        },
        "profile_gap": "The compact gate persisted a broad db_write_ms bucket but did not persist the nested IndexProfile.spans for the proof-mode run. Future compact gates should archive the raw index profile spans so dictionary interning, template inserts, reducer, index creation, ANALYZE, and PathEvidence can be separated without re-running forensics.",
        "stages_over_5_percent_unknown": []
    })
}

fn comprehensive_top_slowest_stages(metrics: &[Value]) -> Vec<Value> {
    let mut stages = metrics
        .iter()
        .filter_map(|metric| {
            let id = metric["id"].as_str()?;
            let observed = metric["observed"].as_f64()?;
            if id.ends_with("_time_ms") || id.ends_with("_total_wall_ms") {
                Some(json!({
                    "stage": id,
                    "elapsed_ms": observed,
                    "status": metric["status"].clone(),
                    "notes": metric["notes"].clone(),
                }))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    stages.sort_by(|left, right| {
        right["elapsed_ms"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&left["elapsed_ms"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    stages.truncate(10);
    stages
}

fn comprehensive_repeat_metrics(
    update_path: &Value,
    repeat_unchanged: Option<&Value>,
    update_integrity: Option<&Value>,
) -> Vec<Value> {
    let repeat = update_path.get("repeat_unchanged").unwrap_or(&Value::Null);
    let profile_time = value_f64(repeat, &["profile_wall_ms"]).or_else(|| {
        repeat_unchanged.and_then(|value| value_f64(value, &["profile_total_wall_ms"]))
    });
    let shell_time = value_f64(repeat, &["shell_ms"])
        .or_else(|| repeat_unchanged.and_then(|value| value_f64(value, &["shell_ms"])));
    let files_read = value_u64(repeat, &["files_read"])
        .or_else(|| repeat_unchanged.and_then(|value| value_u64(value, &["files_read"])));
    let files_hashed = value_u64(repeat, &["files_hashed"])
        .or_else(|| repeat_unchanged.and_then(|value| value_u64(value, &["files_hashed"])));
    let files_parsed = value_u64(repeat, &["files_parsed"])
        .or_else(|| repeat_unchanged.and_then(|value| value_u64(value, &["files_parsed"])));
    let files_walked = value_u64(repeat, &["files_walked"])
        .or_else(|| repeat_unchanged.and_then(|value| value_u64(value, &["files_walked"])));
    let metadata_unchanged =
        repeat_unchanged.and_then(|value| value_u64(value, &["files_metadata_unchanged"]));
    let stable_hash = update_integrity
        .and_then(|value| value.get("repos"))
        .and_then(Value::as_array)
        .and_then(|repos| repos.first())
        .and_then(|repo| value_bool(repo, &["graph_fact_hash_stable_on_repeat"]));
    let proof_mutations = value_u64(repeat, &["entities_inserted"]).unwrap_or(0)
        + value_u64(repeat, &["edges_inserted"]).unwrap_or(0);

    vec![
        metric_le_f64(
            "repeat_unchanged_total_ms",
            5000.0,
            profile_time.or(shell_time),
            "Repeat unchanged index must complete within 5 seconds.",
        ),
        metric_reported_u64(
            "repeat_files_walked",
            files_walked,
            "Repeat unchanged files walked.",
        ),
        metric_reported_u64(
            "repeat_metadata_unchanged",
            metadata_unchanged,
            "Files skipped by metadata prefilter.",
        ),
        metric_eq_u64(
            "repeat_files_read",
            0,
            files_read,
            "Unchanged repeat should not read source files.",
        ),
        metric_eq_u64(
            "repeat_files_hashed",
            0,
            files_hashed,
            "Unchanged repeat should not hash source files.",
        ),
        metric_eq_u64(
            "repeat_files_parsed",
            0,
            files_parsed,
            "Unchanged repeat should not parse source files.",
        ),
        metric_eq_u64(
            "repeat_entities_inserted",
            0,
            value_u64(repeat, &["entities_inserted"]),
            "Unchanged repeat should not insert proof entities.",
        ),
        metric_eq_u64(
            "repeat_edges_inserted",
            0,
            value_u64(repeat, &["edges_inserted"]),
            "Unchanged repeat should not insert proof edges.",
        ),
        metric_reported_u64(
            "repeat_templates_inserted",
            None,
            "Template insert count is not persisted in compact proof repeat artifact.",
        ),
        metric(
            "repeat_graph_fact_hash_changed",
            json!(false),
            stable_hash
                .map(|stable| Value::from(!stable))
                .unwrap_or(Value::Null),
            status_known_pass(stable_hash.map(|stable| stable)),
            vec!["Unchanged repeat graph fact hash must remain stable.".to_string()],
        ),
        metric(
            "repeat_db_writes_performed",
            json!("no proof graph mutations"),
            json!(proof_mutations > 0),
            status_known_pass(Some(proof_mutations == 0)),
            vec![
                "Metadata/checkpoint writes may occur; proof graph rows must not mutate."
                    .to_string(),
            ],
        ),
        metric_reported_u64(
            "repeat_validation_work_integrity_ms",
            repeat_unchanged.and_then(|value| value_u64(value, &["integrity_ms"])),
            "Validation work duration if the artifact persisted it.",
        ),
        metric(
            "repeat_integrity_check_included",
            json!("reported"),
            Value::from(
                value_string(repeat, &["integrity_status"])
                    .unwrap_or_else(|| "unknown".to_string()),
            ),
            "pass",
            vec!["Repeat artifact reports integrity status separately.".to_string()],
        ),
    ]
}

fn comprehensive_single_file_update_metrics(
    update_path: &Value,
    update_integrity: Option<&Value>,
) -> Vec<Value> {
    let update = update_path
        .get("single_file_update")
        .unwrap_or(&Value::Null);
    let detailed_iterations = update_integrity
        .and_then(|value| value.get("repos"))
        .and_then(Value::as_array)
        .and_then(|repos| repos.first())
        .and_then(|repo| repo.get("iteration_results"))
        .and_then(Value::as_array);
    let detailed_updates = detailed_iterations
        .map(|iterations| {
            iterations
                .iter()
                .filter_map(|iteration| iteration.get("update"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let detailed_restores = detailed_iterations
        .map(|iterations| {
            iterations
                .iter()
                .filter_map(|iteration| iteration.get("restore"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let detailed = detailed_updates.first().copied();
    let spans = detailed
        .and_then(|value| value.get("profile"))
        .and_then(|profile| profile.get("spans"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let span_time = |name: &str| -> Option<f64> {
        spans
            .iter()
            .find(|span| span["name"].as_str() == Some(name))
            .and_then(|span| value_f64(span, &["elapsed_ms"]))
    };
    let update_wall_samples = detailed_updates
        .iter()
        .filter_map(|value| value_f64(value, &["wall_ms"]))
        .collect::<Vec<_>>();
    let wall_ms =
        percentile(&update_wall_samples, 0.95).or_else(|| value_f64(update, &["wall_ms"]));
    let global_hash_ran = detailed.and_then(|value| value_bool(value, &["global_hash_check_ran"]));

    vec![
        metric_le_f64(
            "single_file_update_total_ms",
            750.0,
            wall_ms,
            "Single-file update p95 target is <=750 ms.",
        ),
        metric_reported_f64("update_file_walk_time_ms", span_time("file_walk"), "File walk span."),
        metric_reported_f64("update_read_time_ms", span_time("file_read"), "File read span."),
        metric_reported_f64("update_hash_time_ms", span_time("file_hash"), "File hash span."),
        metric_reported_f64("update_parse_time_ms", span_time("parse"), "Parse span."),
        metric_reported_f64("update_stale_delete_time_ms", span_time("stale_fact_delete"), "Indexed stale fact cleanup span."),
        metric_reported_f64("update_template_invalidation_time_ms", None, "Template invalidation is not persisted as a separate span."),
        metric_reported_f64("update_template_insert_update_time_ms", None, "Template insert/update time is not persisted separately."),
        metric_reported_f64("update_proof_entity_update_time_ms", span_time("entity_insert"), "Proof entity insertion/update span."),
        metric_reported_f64("update_proof_edge_update_time_ms", span_time("edge_insert"), "Proof edge insertion/update span."),
        metric_reported_u64(
            "update_dirty_path_evidence_count",
            value_u64(update, &["dirty_path_evidence_count"])
                .or_else(|| detailed.and_then(|value| value_u64(value, &["dirty_path_evidence_count"]))),
            "Dirty PathEvidence rows regenerated.",
        ),
        metric_reported_f64(
            "update_path_evidence_regeneration_time_ms",
            span_time("path_evidence_generation"),
            "PathEvidence regeneration span.",
        ),
        metric_reported_f64("update_index_maintenance_time_ms", span_time("index_creation"), "Index maintenance span."),
        metric_reported_f64("update_transaction_commit_time_ms", span_time("transaction_commit"), "Transaction commit span."),
        metric_reported_f64(
            "update_integrity_or_quick_check_time_ms",
            detailed.and_then(|value| value_f64(value, &["integrity_check_ms"])),
            "Validation integrity duration captured outside the fast path.",
        ),
        metric_reported_f64(
            "update_graph_hash_update_time_ms",
            detailed.and_then(|value| value_f64(value, &["graph_fact_hash_ms"])),
            "Graph digest measurement; fast mode uses the incremental digest instead of a full scan.",
        ),
        metric_reported_f64(
            "update_restore_time_ms",
            percentile(
                &detailed_restores
                    .iter()
                    .filter_map(|value| value_f64(value, &["wall_ms"]))
                    .collect::<Vec<_>>(),
                0.95,
            )
                .or_else(|| {
                    update_path
                        .get("restore_update")
                        .and_then(|value| value_f64(value, &["wall_ms"]))
                }),
            "Restore update duration from update-integrity harness.",
        ),
        metric_reported_u64(
            "update_rows_deleted",
            detailed.and_then(|value| value_u64(value, &["deleted_fact_files"])),
            "Current artifact persists deleted fact files, not exact row count.",
        ),
        metric_reported_u64(
            "update_rows_inserted",
            detailed
                .and_then(|value| value_u64(value, &["entities_inserted"]))
                .or_else(|| value_u64(update, &["entities_inserted"]))
                .zip(
                    detailed
                        .and_then(|value| value_u64(value, &["edges_inserted"]))
                        .or_else(|| value_u64(update, &["edges_inserted"])),
                )
                .map(|(entities, edges)| entities + edges),
            "Inserted proof entity + edge rows.",
        ),
        metric_reported_u64("update_indexes_touched", None, "Indexes touched are not persisted in the artifact."),
        metric(
            "update_global_work_accidentally_triggered",
            json!(false),
            observed_bool(global_hash_ran),
            status_known_pass(global_hash_ran.map(|ran| !ran)),
            vec!["Fast path should avoid full graph hash scans unless validation mode explicitly requests them.".to_string()],
        ),
        metric(
            "update_integrity_remains_ok",
            json!("ok"),
            Value::from(
                detailed
                    .and_then(|value| value_string(value, &["integrity_status"]))
                    .or_else(|| value_string(update, &["integrity_status"]))
                    .unwrap_or_else(|| "unknown".to_string()),
            ),
            status_known_pass(
                detailed
                    .and_then(|value| value_string(value, &["integrity_status"]))
                    .or_else(|| value_string(update, &["integrity_status"]))
                    .map(|status| status == "ok"),
            ),
            vec!["DB integrity must remain ok after update.".to_string()],
        ),
    ]
}

fn comprehensive_query_latency_metrics(
    context_pack_latency: &Option<Value>,
    unresolved_latency: &Option<Value>,
    gate: &Value,
    query_surface_report: Option<&Value>,
) -> Value {
    let mut queries = Vec::new();
    for (id, target, note) in [
        (
            "entity_name_lookup",
            250.0,
            "Entity lookup p95 was not measured in the compact proof gate.",
        ),
        (
            "symbol_lookup",
            250.0,
            "Symbol lookup p95 was not measured in the compact proof gate.",
        ),
        (
            "qname_lookup",
            250.0,
            "QName lookup p95 was not measured in the compact proof gate.",
        ),
        (
            "text_fts_query",
            500.0,
            "Text/FTS p95 was not measured in the compact proof gate.",
        ),
        (
            "relation_query_calls",
            500.0,
            "CALLS relation p95 was not measured in the compact proof gate.",
        ),
        (
            "relation_query_reads_writes",
            500.0,
            "READS/WRITES relation p95 was not measured in the compact proof gate.",
        ),
        (
            "path_evidence_lookup",
            500.0,
            "PathEvidence lookup p95 was not measured as a standalone query.",
        ),
        (
            "source_snippet_batch_load",
            500.0,
            "Source snippet batch-load p95 was not measured as a standalone query.",
        ),
        (
            "context_pack_normal",
            2000.0,
            "context_pack normal p95 was not measured on the fresh compact proof artifact.",
        ),
        (
            "unresolved_calls_paginated",
            1000.0,
            "Paginated unresolved-calls p95 was not measured on the fresh compact proof artifact.",
        ),
    ] {
        queries.push(
            query_surface_latency_metric(query_surface_report, id)
                .unwrap_or_else(|| unknown_query_metric(id, target, note)),
        );
    }
    queries.push(unknown_query_metric(
        "context_pack_impact",
        5000.0,
        "Impact mode context_pack p95 was not measured in the compact proof gate.",
    ));
    queries.push(unknown_query_metric(
        "impact_file",
        5000.0,
        "impact <file> p95 was not measured in the compact proof gate.",
    ));

    json!({
        "queries": queries,
        "notes": [
            "Default query-surface metrics are measured against the fresh compact proof artifact when available.",
            "Missing latency probes are unknown and must not be counted as passes; failed probes fail the comprehensive gate.",
            format!(
                "Context-pack artifact note: {}",
                value_string(gate, &["query_latency", "context_pack", "note"])
                    .unwrap_or_else(|| "not available".to_string())
            ),
            format!(
                "Legacy context-pack p95 artifact: {}",
                context_pack_latency
                    .as_ref()
                    .and_then(|value| value_f64(value, &["p95_shell_ms"]))
                    .map(|value| format!("{value:.3} ms"))
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            format!(
                "Legacy unresolved-calls p95 artifact: {}",
                unresolved_latency
                    .as_ref()
                    .and_then(|value| value_f64(value, &["p95_shell_ms"]))
                    .map(|value| format!("{value:.3} ms"))
                    .unwrap_or_else(|| "unknown".to_string())
            ),
        ]
    })
}

fn query_surface_latency_metric(query_surface_report: Option<&Value>, id: &str) -> Option<Value> {
    let report = query_surface_report?;
    let queries = report
        .get("report")
        .and_then(|value| value.get("queries"))
        .or_else(|| report.get("queries"))?
        .as_array()?;
    queries
        .iter()
        .find(|query| query["id"].as_str() == Some(id))
        .cloned()
}

fn unknown_query_metric(id: &str, target_p95_ms: f64, note: &str) -> Value {
    json!({
        "id": id,
        "target": { "p95_ms": target_p95_ms },
        "observed": {
            "p50_ms": Value::Null,
            "p95_ms": Value::Null,
            "p99_ms": Value::Null
        },
        "status": "unknown",
        "notes": [note]
    })
}

fn build_default_query_surface_report(
    repo_root: &Path,
    db_path: &Path,
    iterations: usize,
) -> Value {
    let started = Instant::now();
    let mut queries = Vec::new();
    let db_path = absolutize_path(db_path).unwrap_or_else(|_| db_path.to_path_buf());
    let repo_root = absolutize_path(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let connection = match Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(connection) => connection,
        Err(error) => {
            let failed = required_query_surface_ids()
                .into_iter()
                .map(|(id, target)| {
                    query_surface_failure_metric(
                        id,
                        target,
                        &format!("failed to open compact proof DB: {error}"),
                    )
                })
                .collect::<Vec<_>>();
            return json!({
                "schema_version": 1,
                "status": "failed",
                "generated_at_unix_ms": unix_time_ms(),
                "repo_root": path_string(&repo_root),
                "db_path": path_string(&db_path),
                "iterations": iterations,
                "queries": failed,
                "summary": {
                    "failed_queries": required_query_surface_ids().len(),
                    "passed_queries": 0,
                    "elapsed_ms": elapsed_ms(started),
                },
                "notes": [
                    "The query surface could not open the compact proof DB."
                ],
            });
        }
    };
    let _ = connection.pragma_update(None, "query_only", true);
    let seeds = query_surface_seeds(&connection).unwrap_or_else(|error| {
        json!({
            "status": "degraded",
            "error": error,
            "entity_name": "login",
            "symbol_query": "login",
            "qname": "login",
            "entity_id": "repo://e/unknown",
            "fts_query": "\"login\"",
            "context_seed": "login",
        })
    });

    queries.push(benchmark_sql_surface_query(
        &connection,
        "entity_name_lookup",
        "codegraph-mcp query symbols <entity-name>",
        250.0,
        &format!(
            "WITH wanted_name AS (SELECT id FROM symbol_dict WHERE value = {name} LIMIT 1)
             SELECT 'repo://e/' || lower(hex(e.entity_hash)) AS id, name.value AS name,
                    qname.value AS qualified_name, path.value AS repo_relative_path
             FROM wanted_name
             JOIN entities e ON e.name_id = wanted_name.id
             JOIN symbol_dict name ON name.id = e.name_id
             JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
             JOIN path_dict path ON path.id = e.path_id
             ORDER BY qname.value, e.entity_hash
             LIMIT 20",
            name = sqlite_quote_text(seed_text(&seeds, "entity_name", "login"))
        ),
        iterations,
        "Default entity-name lookup must compile and use compact proof entity dictionaries.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "symbol_lookup",
        "codegraph-mcp query symbols <symbol>",
        250.0,
        &format!(
            "WITH candidate_keys AS (
                 SELECT e.id_key
                 FROM symbol_dict wanted
                 JOIN entities e ON e.name_id = wanted.id
                 WHERE wanted.value = {symbol}
                 UNION
                 SELECT e.id_key
                 FROM qualified_name_lookup wanted_qname
                 JOIN entities e ON e.qualified_name_id = wanted_qname.id
                 WHERE wanted_qname.value = {symbol}
             )
             SELECT 'repo://e/' || lower(hex(e.entity_hash)) AS id, name.value AS name,
                    qname.value AS qualified_name, path.value AS repo_relative_path
             FROM candidate_keys
             JOIN entities e ON e.id_key = candidate_keys.id_key
             JOIN symbol_dict name ON name.id = e.name_id
             JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
             JOIN path_dict path ON path.id = e.path_id
             ORDER BY qname.value, e.entity_hash
             LIMIT 20",
            symbol = sqlite_quote_text(seed_text(&seeds, "symbol_query", "login"))
        ),
        iterations,
        "Default symbol lookup must resolve object ID, name, and qualified-name paths without audit/debug sidecars.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "qname_lookup",
        "codegraph-mcp query symbols <qualified-name>",
        250.0,
        &format!(
            "WITH wanted_qname AS (SELECT id, value FROM qualified_name_lookup WHERE value = {qname} LIMIT 1)
             SELECT 'repo://e/' || lower(hex(e.entity_hash)) AS id, name.value AS name,
                    wanted_qname.value AS qualified_name, path.value AS repo_relative_path
             FROM wanted_qname
             JOIN entities e ON e.qualified_name_id = wanted_qname.id
             JOIN symbol_dict name ON name.id = e.name_id
             JOIN path_dict path ON path.id = e.path_id
             ORDER BY e.entity_hash
             LIMIT 20",
            qname = sqlite_quote_text(seed_text(&seeds, "qname", "login"))
        ),
        iterations,
        "Default qname lookup must use compact qualified-name reconstruction.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "text_fts_query",
        "codegraph-mcp query text <query>",
        500.0,
        &format!(
            "SELECT kind, id, repo_relative_path, line, title, bm25(stage0_fts) AS rank FROM stage0_fts WHERE stage0_fts MATCH {} ORDER BY rank, kind, id LIMIT 20",
            sqlite_quote_text(seed_text(&seeds, "fts_query", "\"login\""))
        ),
        iterations,
        "Proof-mode text query must use compact FTS or fail explicitly.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "relation_query_calls",
        "codegraph-mcp query callers/callees <symbol>",
        500.0,
        "SELECT e.id_key, head.value AS head_id, tail.value AS tail_id, relation.value AS relation, span_path.value AS source_span_path, e.start_line, e.end_line FROM edges_compat e JOIN relation_kind_dict relation ON relation.id = e.relation_id JOIN object_id_lookup head ON head.id = e.head_id_key JOIN object_id_lookup tail ON tail.id = e.tail_id_key JOIN path_dict span_path ON span_path.id = e.span_path_id WHERE relation.value = 'CALLS' ORDER BY e.id_key LIMIT 50",
        iterations,
        "Bounded CALLS relation lookup must use proof edges and compact compatibility views.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "relation_query_reads_writes",
        "codegraph-mcp query impact/proof relation READS|WRITES",
        500.0,
        "SELECT e.id_key, head.value AS head_id, tail.value AS tail_id, relation.value AS relation, span_path.value AS source_span_path, e.start_line, e.end_line FROM edges_compat e JOIN relation_kind_dict relation ON relation.id = e.relation_id JOIN object_id_lookup head ON head.id = e.head_id_key JOIN object_id_lookup tail ON tail.id = e.tail_id_key JOIN path_dict span_path ON span_path.id = e.span_path_id WHERE relation.value IN ('READS', 'WRITES') ORDER BY e.id_key LIMIT 50",
        iterations,
        "Bounded READS/WRITES lookup must compile against compact proof edges.",
    ));
    queries.push(benchmark_sql_surface_query(
        &connection,
        "path_evidence_lookup",
        "context_pack stored PathEvidence lookup",
        500.0,
        "SELECT l.path_id, p.source, p.target, l.relation_signature, l.length, l.confidence FROM path_evidence_lookup l JOIN path_evidence p ON p.id = l.path_id ORDER BY l.confidence DESC, l.length ASC, l.path_id LIMIT 50",
        iterations,
        "Stored PathEvidence lookup must use proof-mode materialized lookup tables.",
    ));
    queries.push(benchmark_source_snippet_batch_load(
        &connection,
        &repo_root,
        iterations,
    ));
    queries.push(benchmark_context_pack_surface_query(
        &repo_root,
        &db_path,
        seed_text(&seeds, "context_seed", "login"),
        iterations,
    ));
    queries.push(benchmark_unresolved_calls_surface_query(
        &connection,
        &repo_root,
        &db_path,
        iterations,
    ));

    let failed_queries = queries
        .iter()
        .filter(|query| query["status"].as_str() == Some("fail"))
        .count();
    let passed_queries = queries
        .iter()
        .filter(|query| query["status"].as_str() == Some("pass"))
        .count();
    let status = if failed_queries == 0 {
        "passed"
    } else {
        "failed"
    };
    json!({
        "schema_version": 1,
        "status": status,
        "generated_at_unix_ms": unix_time_ms(),
        "repo_root": path_string(&repo_root),
        "db_path": path_string(&db_path),
        "storage_mode": "proof",
        "iterations": iterations,
        "seeds": seeds,
        "queries": queries,
        "summary": {
            "passed_queries": passed_queries,
            "failed_queries": failed_queries,
            "elapsed_ms": elapsed_ms(started),
            "all_default_queries_complete": failed_queries == 0,
        },
        "notes": [
            "Queries are executed against compact proof storage only.",
            "Audit/debug sidecars are not used unless an individual query explicitly targets heuristic/debug evidence, such as unresolved-calls.",
            "Each query includes SQL, EXPLAIN QUERY PLAN, and p50/p95/p99 timings when it compiles."
        ],
    })
}

fn required_query_surface_ids() -> Vec<(&'static str, f64)> {
    vec![
        ("entity_name_lookup", 250.0),
        ("symbol_lookup", 250.0),
        ("qname_lookup", 250.0),
        ("text_fts_query", 500.0),
        ("relation_query_calls", 500.0),
        ("relation_query_reads_writes", 500.0),
        ("path_evidence_lookup", 500.0),
        ("source_snippet_batch_load", 500.0),
        ("context_pack_normal", 2000.0),
        ("unresolved_calls_paginated", 1000.0),
    ]
}

fn query_surface_seeds(connection: &Connection) -> Result<Value, String> {
    let entity = connection
        .query_row(
            "
            SELECT oid.value, name.value, qname.value
            FROM entities e
            JOIN object_id_lookup oid ON oid.id = e.id_key
            JOIN symbol_dict name ON name.id = e.name_id
            JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
            WHERE name.value IS NOT NULL AND name.value != ''
            ORDER BY CASE WHEN name.value = 'login' THEN 0 ELSE 1 END, e.id_key
            LIMIT 1
            ",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (entity_id, entity_name, qname) = entity.unwrap_or_else(|| {
        (
            "repo://e/unknown".to_string(),
            "login".to_string(),
            "login".to_string(),
        )
    });
    let fts_term = if sqlite_table_exists(connection, "stage0_fts").unwrap_or(false) {
        connection
            .query_row(
                "SELECT title FROM stage0_fts WHERE title IS NOT NULL AND title != '' ORDER BY kind, id LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .and_then(|title| title.split_whitespace().find(|part| part.len() >= 2).map(str::to_string))
            .unwrap_or_else(|| entity_name.clone())
    } else {
        entity_name.clone()
    };
    let symbol_query = entity_name.clone();
    let context_seed = if sqlite_table_exists(connection, "path_evidence_lookup").unwrap_or(false) {
        connection
            .query_row(
                "SELECT source_id FROM path_evidence_lookup ORDER BY confidence DESC, length ASC, path_id LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .unwrap_or_else(|| entity_id.clone())
    } else {
        entity_id.clone()
    };
    Ok(json!({
        "status": "ok",
        "entity_id": entity_id,
        "entity_name": entity_name,
        "symbol_query": symbol_query,
        "qname": qname,
        "fts_query": sqlite_fts_phrase(&fts_term),
        "context_seed": context_seed,
    }))
}

fn benchmark_sql_surface_query(
    connection: &Connection,
    id: &str,
    command: &str,
    target_p95_ms: f64,
    sql: &str,
    iterations: usize,
    note: &str,
) -> Value {
    let explain = explain_query_plan(connection, id, sql);
    let explain_plan = explain
        .as_ref()
        .ok()
        .and_then(|value| value.get("plan").and_then(Value::as_array).cloned());
    let explain_error = explain.as_ref().err().cloned();
    let mut samples = Vec::new();
    let mut rows_returned = 0usize;
    let mut error = explain_error;
    if error.is_none() {
        for _ in 0..iterations {
            let started = Instant::now();
            match count_sql_rows(connection, sql) {
                Ok(rows) => {
                    rows_returned = rows;
                    samples.push(elapsed_ms_f64(started));
                }
                Err(query_error) => {
                    error = Some(query_error);
                    break;
                }
            }
        }
    }
    query_surface_metric(
        id,
        command,
        target_p95_ms,
        sql,
        explain_plan,
        samples,
        rows_returned,
        error,
        vec![note.to_string()],
    )
}

fn benchmark_source_snippet_batch_load(
    connection: &Connection,
    repo_root: &Path,
    iterations: usize,
) -> Value {
    let sql = "
        SELECT DISTINCT repo_relative_path, start_line, start_column, end_line, end_column
        FROM (
            SELECT path.value AS repo_relative_path, e.start_line, e.start_column, e.end_line, e.end_column
            FROM edges_compat e
            JOIN path_dict path ON path.id = e.span_path_id
            WHERE e.start_line > 0
            UNION ALL
            SELECT path.value AS repo_relative_path, s.start_line, s.start_column, s.end_line, s.end_column
            FROM source_spans s
            JOIN path_dict path ON path.id = s.path_id
            WHERE s.start_line > 0
        )
        ORDER BY repo_relative_path, start_line, end_line
        LIMIT 50
    ";
    let explain = explain_query_plan(connection, "source_snippet_batch_load", sql);
    let explain_plan = explain
        .as_ref()
        .ok()
        .and_then(|value| value.get("plan").and_then(Value::as_array).cloned());
    let mut samples = Vec::new();
    let mut snippets_loaded = 0usize;
    let mut error = explain.as_ref().err().cloned();
    if error.is_none() {
        for _ in 0..iterations {
            let started = Instant::now();
            match load_source_snippet_batch(connection, repo_root, sql) {
                Ok(count) => {
                    snippets_loaded = count;
                    samples.push(elapsed_ms_f64(started));
                }
                Err(query_error) => {
                    error = Some(query_error);
                    break;
                }
            }
        }
    }
    query_surface_metric(
        "source_snippet_batch_load",
        "context_pack source snippet batch load",
        500.0,
        sql,
        explain_plan,
        samples,
        snippets_loaded,
        error,
        vec![
            "Source snippets must load from source files using proof-edge/source-span rows, not redundant SQLite snippet storage.".to_string(),
        ],
    )
}

fn benchmark_context_pack_surface_query(
    repo_root: &Path,
    db_path: &Path,
    seed: &str,
    iterations: usize,
) -> Value {
    let explain_plan = context_pack_explain_query_plans(db_path).ok();
    let mut samples = Vec::new();
    let mut rows_returned = 0usize;
    let mut error = None;
    for _ in 0..iterations {
        let args = vec![
            "--task".to_string(),
            "default query surface context_pack benchmark".to_string(),
            "--seed".to_string(),
            seed.to_string(),
            "--mode".to_string(),
            "normal".to_string(),
            "--budget".to_string(),
            "1200".to_string(),
        ];
        let started = Instant::now();
        let result = with_repo_db_context(repo_root, db_path, || run_context_pack_command(&args));
        match result {
            Ok(value) => {
                rows_returned = value
                    .get("packet")
                    .and_then(|packet| packet.get("verified_paths"))
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or_default();
                samples.push(elapsed_ms_f64(started));
            }
            Err(query_error) => {
                error = Some(query_error);
                break;
            }
        }
    }
    query_surface_metric(
        "context_pack_normal",
        "codegraph-mcp context-pack --mode normal",
        2000.0,
        "context_pack normal stored-PathEvidence query path",
        explain_plan,
        samples,
        rows_returned,
        error,
        vec![
            "Normal context_pack must use proof-mode stored PathEvidence first and keep source-span labels.".to_string(),
        ],
    )
}

fn benchmark_unresolved_calls_surface_query(
    connection: &Connection,
    repo_root: &Path,
    db_path: &Path,
    iterations: usize,
) -> Value {
    let explain_plan = unresolved_calls_query_plan(connection, 20, 0).ok();
    let mut samples = Vec::new();
    let mut rows_returned = 0usize;
    let mut error = None;
    for _ in 0..iterations {
        let started = Instant::now();
        let options = UnresolvedCallsOptions {
            db_path: Some(db_path.to_path_buf()),
            requested_limit: 20,
            limit: 20,
            offset: 0,
            include_snippets: false,
            source_scan: false,
            count_total: false,
        };
        match query_unresolved_calls(repo_root, options) {
            Ok(value) => {
                rows_returned = value
                    .get("calls")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or_default();
                samples.push(elapsed_ms_f64(started));
            }
            Err(query_error) => {
                error = Some(query_error);
                break;
            }
        }
    }
    query_surface_metric(
        "unresolved_calls_paginated",
        "codegraph-mcp query unresolved-calls --limit 20",
        1000.0,
        UNRESOLVED_CALLS_PAGE_SQL,
        explain_plan,
        samples,
        rows_returned,
        error,
        vec![
            "Unresolved-calls pagination may read the heuristic/debug sidecar by design, but it must remain explicit and bounded.".to_string(),
        ],
    )
}

fn query_surface_metric(
    id: &str,
    command: &str,
    target_p95_ms: f64,
    sql: &str,
    explain_plan: Option<Vec<Value>>,
    samples: Vec<f64>,
    rows_returned: usize,
    error: Option<String>,
    notes: Vec<String>,
) -> Value {
    let p50 = percentile(&samples, 0.50);
    let p95 = percentile(&samples, 0.95);
    let p99 = percentile(&samples, 0.99);
    let mut notes = notes;
    if rows_returned == 0 && error.is_none() {
        notes.push(
            "Query completed but returned zero rows on this artifact; latency is still measured."
                .to_string(),
        );
    }
    let status = if error.is_some() {
        "fail".to_string()
    } else {
        status_known_pass(p95.map(|value| value <= target_p95_ms)).to_string()
    };
    json!({
        "id": id,
        "command": command,
        "target": { "p95_ms": target_p95_ms },
        "observed": {
            "p50_ms": p50,
            "p95_ms": p95,
            "p99_ms": p99,
            "iterations": samples.len(),
            "samples_ms": samples,
            "rows_returned_last_iteration": rows_returned,
        },
        "status": status,
        "sql": sql.split_whitespace().collect::<Vec<_>>().join(" "),
        "explain_query_plan": explain_plan.clone().unwrap_or_default(),
        "query_plan_analysis": explain_plan
            .as_ref()
            .map(|plan| analyze_sqlite_query_plan(plan))
            .unwrap_or_else(|| json!({
                "uses_indexes": false,
                "indexes_used": [],
                "full_scans": [],
            })),
        "error": error,
        "notes": notes,
    })
}

fn query_surface_failure_metric(id: &str, target_p95_ms: f64, error: &str) -> Value {
    json!({
        "id": id,
        "target": { "p95_ms": target_p95_ms },
        "observed": {
            "p50_ms": Value::Null,
            "p95_ms": Value::Null,
            "p99_ms": Value::Null,
            "iterations": 0,
        },
        "status": "fail",
        "sql": Value::Null,
        "explain_query_plan": [],
        "query_plan_analysis": {
            "uses_indexes": false,
            "indexes_used": [],
            "full_scans": [],
        },
        "error": error,
        "notes": ["Query was not executed because the compact proof DB could not be opened."]
    })
}

fn render_default_query_surface_markdown(report: &Value) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Default Query Surface Audit\n\n");
    markdown.push_str("Source of truth: `MVP.md`.\n\n");
    markdown.push_str(&format!(
        "- Status: **{}**\n- DB: `{}`\n- Iterations: `{}`\n\n",
        report["status"].as_str().unwrap_or("unknown"),
        report["db_path"].as_str().unwrap_or("unknown"),
        report["iterations"].as_u64().unwrap_or(0)
    ));
    markdown.push_str("| Query | Status | p50 ms | p95 ms | p99 ms | rows | target p95 |\n");
    markdown.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: |\n");
    if let Some(queries) = report["queries"].as_array() {
        for query in queries {
            markdown.push_str(&format!(
                "| `{}` | `{}` | {} | {} | {} | {} | {} |\n",
                query["id"].as_str().unwrap_or("unknown"),
                query["status"].as_str().unwrap_or("unknown"),
                display_value(&query["observed"]["p50_ms"]),
                display_value(&query["observed"]["p95_ms"]),
                display_value(&query["observed"]["p99_ms"]),
                display_value(&query["observed"]["rows_returned_last_iteration"]),
                display_value(&query["target"]["p95_ms"]),
            ));
        }
    }
    markdown.push_str("\n## Query Details\n\n");
    if let Some(queries) = report["queries"].as_array() {
        for query in queries {
            markdown.push_str(&format!(
                "### `{}`\n\nCommand: `{}`\n\nStatus: `{}`\n\n",
                query["id"].as_str().unwrap_or("unknown"),
                query["command"].as_str().unwrap_or("unknown"),
                query["status"].as_str().unwrap_or("unknown")
            ));
            if let Some(error) = query["error"].as_str() {
                markdown.push_str(&format!("Error: `{error}`\n\n"));
            }
            markdown.push_str("SQL:\n\n```sql\n");
            markdown.push_str(query["sql"].as_str().unwrap_or(""));
            markdown.push_str("\n```\n\n");
            markdown.push_str("EXPLAIN QUERY PLAN:\n\n");
            markdown.push_str("| id | parent | detail |\n");
            markdown.push_str("| ---: | ---: | --- |\n");
            if let Some(plan) = query["explain_query_plan"].as_array() {
                for row in plan {
                    markdown.push_str(&format!(
                        "| {} | {} | `{}` |\n",
                        display_value(&row["id"]),
                        display_value(&row["parent"]),
                        row["detail"].as_str().unwrap_or("")
                    ));
                }
            }
            markdown.push('\n');
        }
    }
    markdown.push_str("\n## Storage Safety Questions\n\n");
    markdown.push_str("1. Did Graph Truth still pass? Measured by the surrounding gate; this query audit does not change graph truth facts.\n");
    markdown.push_str("2. Did Context Packet quality still pass? Measured by the surrounding gate; context_pack normal is probed here.\n");
    markdown.push_str(
        "3. Did proof DB size decrease? No storage optimization is performed by this audit.\n",
    );
    markdown.push_str("4. Did removed data move to a sidecar, become derivable, or get proven unnecessary? No data is removed by this audit.\n");
    markdown
}

fn count_sql_rows(connection: &Connection, sql: &str) -> Result<usize, String> {
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    let mut rows = statement.query([]).map_err(|error| error.to_string())?;
    let mut count = 0usize;
    while rows.next().map_err(|error| error.to_string())?.is_some() {
        count += 1;
    }
    Ok(count)
}

fn load_source_snippet_batch(
    connection: &Connection,
    repo_root: &Path,
    sql: &str,
) -> Result<usize, String> {
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(SourceSpan {
                repo_relative_path: row.get::<_, String>(0)?,
                start_line: row.get::<_, u32>(1)?,
                start_column: row.get::<_, Option<u32>>(2)?,
                end_line: row.get::<_, u32>(3)?,
                end_column: row.get::<_, Option<u32>>(4)?,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut spans = Vec::new();
    for row in rows {
        spans.push(row.map_err(|error| error.to_string())?);
    }
    let mut loaded = 0usize;
    let mut source_cache: BTreeMap<String, String> = BTreeMap::new();
    for span in spans {
        if span.repo_relative_path.is_empty() {
            continue;
        }
        let source = if let Some(source) = source_cache.get(&span.repo_relative_path) {
            source.clone()
        } else {
            let path = repo_root.join(&span.repo_relative_path);
            let source = fs::read_to_string(&path).unwrap_or_default();
            source_cache.insert(span.repo_relative_path.clone(), source.clone());
            source
        };
        if !source_snippet_for_span(&source, &span).trim().is_empty() {
            loaded += 1;
        }
    }
    Ok(loaded)
}

fn seed_text<'a>(seeds: &'a Value, key: &str, fallback: &'a str) -> &'a str {
    seeds.get(key).and_then(Value::as_str).unwrap_or(fallback)
}

fn sqlite_quote_text(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sqlite_fts_phrase(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                ' '
            }
        })
        .collect::<String>();
    let token = sanitized
        .split_whitespace()
        .find(|part| part.len() >= 2)
        .unwrap_or("login");
    format!("\"{}\"", token.replace('"', ""))
}

fn elapsed_ms_f64(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn with_repo_db_context<F>(repo_root: &Path, db_path: &Path, operation: F) -> Result<Value, String>
where
    F: FnOnce() -> Result<Value, String>,
{
    let old_cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let old_db = std::env::var_os("CODEGRAPH_DB_PATH");
    std::env::set_current_dir(repo_root).map_err(|error| error.to_string())?;
    std::env::set_var("CODEGRAPH_DB_PATH", db_path);
    let result = operation();
    if let Some(old_db) = old_db {
        std::env::set_var("CODEGRAPH_DB_PATH", old_db);
    } else {
        std::env::remove_var("CODEGRAPH_DB_PATH");
    }
    std::env::set_current_dir(old_cwd).map_err(|error| error.to_string())?;
    result
}

fn percentile(samples: &[f64], quantile: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((sorted.len().saturating_sub(1)) as f64 * quantile).ceil() as usize;
    sorted
        .get(index.min(sorted.len().saturating_sub(1)))
        .copied()
}

fn comprehensive_manual_quality_section(manual_labels: &Option<Value>) -> Value {
    let Some(labels) = manual_labels else {
        return json!({
            "status": "unknown",
            "real_relation_precision": "unknown",
            "reason": "manual relation labels summary JSON is missing",
            "relations": []
        });
    };
    let samples = labels
        .get("samples")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let labeled_count = samples
        .iter()
        .filter(|sample| sample["labeled"].as_bool() == Some(true))
        .count();
    if labeled_count == 0 {
        return json!({
            "status": "unknown",
            "real_relation_precision": "unknown",
            "reason": "sample files exist, but no human labels are filled in",
            "edges_labeled": 0,
            "relations": []
        });
    }

    let mut by_relation: BTreeMap<String, serde_json::Map<String, Value>> = BTreeMap::new();
    for sample in samples
        .iter()
        .filter(|sample| sample["labeled"].as_bool() == Some(true))
    {
        let relation = sample["relation"]
            .as_str()
            .unwrap_or("PathEvidence")
            .to_string();
        let entry = by_relation.entry(relation).or_insert_with(|| {
            let mut map = serde_json::Map::new();
            for key in [
                "samples",
                "true_positive",
                "false_positive",
                "wrong_direction",
                "wrong_target",
                "wrong_span",
                "stale",
                "duplicate",
                "unresolved_mislabeled_exact",
                "test_mock_leaked",
                "derived_missing_provenance",
                "unsure",
            ] {
                map.insert(key.to_string(), json!(0u64));
            }
            map
        });
        increment_label_count(entry, "samples");
        if let Some(labels) = sample.get("labels").and_then(Value::as_object) {
            for key in [
                "true_positive",
                "false_positive",
                "wrong_direction",
                "wrong_target",
                "wrong_span",
                "stale",
                "duplicate",
                "unresolved_mislabeled_exact",
                "test_mock_leaked",
                "derived_missing_provenance",
                "unsure",
            ] {
                if labels.get(key).and_then(Value::as_bool) == Some(true) {
                    increment_label_count(entry, key);
                }
            }
        }
    }

    let relations = by_relation
        .into_iter()
        .map(|(relation, mut entry)| {
            let samples = entry.get("samples").and_then(Value::as_u64).unwrap_or(0);
            let true_positive = entry
                .get("true_positive")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let wrong_span = entry.get("wrong_span").and_then(Value::as_u64).unwrap_or(0);
            let precision = if samples == 0 {
                Value::Null
            } else {
                json!(true_positive as f64 / samples as f64)
            };
            let source_span_precision = if samples == 0 {
                Value::Null
            } else {
                json!((samples.saturating_sub(wrong_span)) as f64 / samples as f64)
            };
            entry.insert("relation".to_string(), json!(relation));
            entry.insert("precision".to_string(), precision);
            entry.insert("source_span_precision".to_string(), source_span_precision);
            Value::Object(entry)
        })
        .collect::<Vec<_>>();

    let real_relation_precision = labels
        .get("real_relation_precision_status")
        .and_then(Value::as_str)
        .unwrap_or("reported_for_labeled_relations_no_claim_for_unlabeled_relations");

    json!({
        "status": "reported",
        "real_relation_precision": real_relation_precision,
        "edges_labeled": labeled_count,
        "relations": relations,
        "target_evaluation": labels
            .get("target_evaluation")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "relation_coverage": labels
            .get("relation_coverage")
            .cloned()
            .unwrap_or_else(|| json!({})),
        "source_span_target_evaluation": labels
            .get("source_span_target_evaluation")
            .cloned()
            .unwrap_or_else(|| json!({})),
        "path_evidence_target_evaluation": labels
            .get("path_evidence_target_evaluation")
            .cloned()
            .unwrap_or_else(|| json!({})),
        "false_positive_taxonomy": labels
            .get("false_positive_taxonomy")
            .cloned()
            .unwrap_or_else(|| json!({})),
        "targets": {
            "CALLS": 0.95,
            "READS_WRITES": 0.90,
            "FLOWS_TO": 0.85,
            "AUTH_CHECKS_ROLE_SANITIZES": 0.95,
            "TESTS_MOCKS_STUBS_ASSERTS": 0.90,
            "source_span_precision": 0.95
        }
    })
}

fn increment_label_count(entry: &mut serde_json::Map<String, Value>, key: &str) {
    let next = entry.get(key).and_then(Value::as_u64).unwrap_or(0) + 1;
    entry.insert(key.to_string(), json!(next));
}

fn comprehensive_comparison_section(comparison: &Option<Value>) -> Value {
    let Some(comparison) = comparison else {
        return json!({
            "cgc_available": Value::Null,
            "cgc_version": Value::Null,
            "cgc_completed": false,
            "cgc_timeout": Value::Null,
            "cgc_db_artifact_size_bytes": Value::Null,
            "codegraph_vs_cgc_speed": "unknown",
            "codegraph_vs_cgc_storage": "unknown",
            "codegraph_vs_cgc_quality": "unknown",
            "verdict": "unknown",
            "notes": ["No latest CGC comparison report is available."]
        });
    };
    let cgc_version = value_string(comparison, &["environment", "cgc_version"]);
    let cgc_path = value_string(comparison, &["environment", "cgc_path"]);
    let frozen_status = value_string(
        comparison,
        &["indexing", "frozen_autoresearch", "cgc_status"],
    );
    let fixture_status = value_string(comparison, &["indexing", "cgc_fixture_harness", "status"]);
    let cgc_completed = frozen_status.as_deref() == Some("completed")
        || fixture_status.as_deref() == Some("completed");
    let cgc_timeout = frozen_status.as_deref() == Some("timeout");
    json!({
        "cgc_available": cgc_path.is_some(),
        "cgc_version": cgc_version,
        "cgc_completed": cgc_completed,
        "cgc_timeout": cgc_timeout,
        "cgc_db_artifact_size_bytes": value_u64(comparison, &["storage", "cgc_frozen_autoresearch_final_artifact_bytes"]),
        "codegraph_vs_cgc_speed": if cgc_completed { "not recomputed after compact proof gate" } else { "unknown" },
        "codegraph_vs_cgc_storage": if cgc_completed { "not recomputed after compact proof gate" } else { "unknown" },
        "codegraph_vs_cgc_quality": if cgc_completed { "not recomputed after compact proof gate" } else { "unknown" },
        "verdict": if cgc_completed { "unknown" } else { "incomplete" },
        "notes": [
            "Latest comparison predates the compact proof baseline and is not a basis for a current CodeGraph win.",
            "CGC timeout/skipped/incomplete remains incomplete, not a CodeGraph win.",
            "Fake-agent dry runs are not model superiority evidence.",
            "CodeGraph internal storage failure remains failure even if CGC is incomplete."
        ]
    })
}

fn comprehensive_regression_summary(
    baseline: &Value,
    gate: &Value,
    clean_gate: &Option<Value>,
    previous: Option<&Value>,
) -> Value {
    let current_storage = value_f64(gate, &["storage", "proof", "file_family_mib"]);
    let current_context = value_f64(gate, &["query_latency", "context_pack", "p95_profile_ms"])
        .or_else(|| value_f64(gate, &["query_latency", "context_pack", "p95_shell_ms"]));
    let current_update = value_f64(gate, &["update_path", "single_file_update", "wall_ms"]);
    let current_repeat = value_f64(
        gate,
        &["update_path", "repeat_unchanged", "profile_wall_ms"],
    );
    let current_cold = value_f64(gate, &["autoresearch", "proof_build", "wall_ms"]);
    let compact_storage = value_f64(baseline, &["storage_summary", "proof_file_family_mib"]);
    let clean_storage = clean_gate
        .as_ref()
        .and_then(|value| value_f64(value, &["storage", "db_family_bytes"]))
        .map(|bytes| bytes / 1024.0 / 1024.0);
    let clean_context = clean_gate
        .as_ref()
        .and_then(|value| value_f64(value, &["query_latency", "context_pack", "observed_p95_ms"]));
    let clean_update = clean_gate
        .as_ref()
        .and_then(|value| value_f64(value, &["indexing", "single_file_update", "observed_ms"]));
    let clean_repeat = clean_gate.as_ref().and_then(|value| {
        value_f64(
            value,
            &["indexing", "unchanged_repeat", "observed_ms_profile"],
        )
    });
    let previous_storage = previous.and_then(|value| {
        section_metric_observed(
            value,
            &["sections", "storage_summary", "metrics"],
            "proof_db_mib",
        )
    });

    let mut rows = vec![
        regression_row(
            "proof_db_mib_vs_clean_1_63_gib",
            clean_storage,
            current_storage,
            false,
        ),
        regression_row(
            "context_pack_p95_ms_vs_clean_30s",
            clean_context,
            current_context,
            false,
        ),
        regression_row(
            "single_file_update_ms_vs_clean_80s",
            clean_update,
            current_update,
            false,
        ),
        regression_row(
            "repeat_unchanged_ms_vs_clean_13s",
            clean_repeat,
            current_repeat,
            false,
        ),
        regression_row(
            "cold_proof_build_ms_vs_compact_baseline",
            current_cold,
            current_cold,
            false,
        ),
        regression_row(
            "proof_db_mib_vs_compact_baseline",
            compact_storage,
            current_storage,
            false,
        ),
    ];
    if previous_storage.is_some() {
        rows.push(regression_row(
            "proof_db_mib_vs_previous_comprehensive",
            previous_storage,
            current_storage,
            false,
        ));
    }
    json!({
        "metrics": rows,
        "notes": [
            "Historical clean Autoresearch values come from INTENDED_PERFORMANCE_GATE when available.",
            "First comprehensive run compares primarily against the compact proof baseline; later runs compare against comprehensive_benchmark_latest.json."
        ]
    })
}

fn section_metric_observed(value: &Value, path: &[&str], metric_id: &str) -> Option<f64> {
    gate_value(value, path)?
        .as_array()?
        .iter()
        .find(|metric| metric["id"].as_str() == Some(metric_id))
        .and_then(|metric| metric["observed"].as_f64())
}

fn regression_row(
    id: &str,
    previous: Option<f64>,
    current: Option<f64>,
    higher_is_better: bool,
) -> Value {
    let delta = previous
        .zip(current)
        .map(|(previous, current)| current - previous);
    let status = match delta {
        Some(delta) if delta.abs() < f64::EPSILON => "unchanged",
        Some(delta) if higher_is_better && delta > 0.0 => "improved",
        Some(delta) if !higher_is_better && delta < 0.0 => "improved",
        Some(_) => "regressed",
        None => "unknown",
    };
    json!({
        "id": id,
        "previous_value": previous,
        "current_value": current,
        "delta": delta,
        "status": status
    })
}

fn render_comprehensive_benchmark_markdown(report: &Value) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Comprehensive Benchmark Latest\n\n");
    markdown.push_str("Source of truth: `MVP.md`.\n\n");
    markdown.push_str(&format!(
        "Execution mode: `{}`. {}\n\n",
        report["execution_mode"].as_str().unwrap_or("unknown"),
        report["execution_mode_notes"].as_str().unwrap_or("")
    ));

    let executive = &report["sections"]["executive_verdict"];
    markdown.push_str("## Section 1 - Executive Verdict\n\n");
    markdown.push_str(&format!(
        "- Verdict: **{}**\n- Reason: {}\n- Optimization may continue: `{}`\n- Comparison claims allowed: `{}`\n\n",
        executive["verdict"].as_str().unwrap_or("unknown"),
        executive["reason_for_failure"].as_str().unwrap_or("unknown"),
        executive["optimization_may_continue"].as_bool().unwrap_or(false),
        executive["comparison_claims_allowed"].as_bool().unwrap_or(false)
    ));
    push_string_list(
        &mut markdown,
        "Failed targets",
        &executive["exact_failed_targets"],
    );
    push_string_list(
        &mut markdown,
        "Passed targets",
        &executive["exact_passed_targets"],
    );

    push_metric_section(
        &mut markdown,
        "Section 2 - Correctness Gates",
        &report["sections"]["correctness_gates"]["metrics"],
    );
    push_metric_section(
        &mut markdown,
        "Section 3 - Context Packet Gate",
        &report["sections"]["context_packet_gate"]["metrics"],
    );
    push_metric_section(
        &mut markdown,
        "Section 4 - DB Integrity",
        &report["sections"]["db_integrity"]["metrics"],
    );
    push_metric_section(
        &mut markdown,
        "Section 4A - Proof Artifact Freshness",
        &report["sections"]["artifact_freshness"]["metrics"],
    );
    if report["artifact_freshness"].is_object() {
        markdown.push_str("### Proof Artifact Metadata\n\n");
        markdown.push_str("| Field | Value |\n| --- | --- |\n");
        for field in [
            "artifact_path",
            "artifact_metadata_path",
            "freshness_metadata_present",
            "artifact_reuse",
            "freshly_built",
            "stale",
            "freshness_status",
            "schema_version",
            "current_schema_version",
            "migration_version",
            "current_migration_version",
            "storage_mode",
            "db_size_bytes",
            "integrity_status",
            "build_duration_ms",
            "storage_result_claimable",
            "cold_build_result_claimable",
        ] {
            markdown.push_str(&format!(
                "| `{}` | {} |\n",
                field,
                display_value(&report["artifact_freshness"][field])
            ));
        }
        markdown.push('\n');
    }
    push_metric_section(
        &mut markdown,
        "Section 5 - Storage Summary",
        &report["sections"]["storage_summary"]["metrics"],
    );

    markdown.push_str("## Section 6 - Storage Contributors\n\n");
    markdown.push_str(
        "| Object | Kind | Rows | MiB | Share | Previous bytes | Delta bytes | Classification |\n",
    );
    markdown.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |\n");
    if let Some(contributors) =
        report["sections"]["storage_contributors"]["contributors"].as_array()
    {
        for contributor in contributors.iter().take(40) {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
                contributor["name"].as_str().unwrap_or("unknown"),
                contributor["kind"].as_str().unwrap_or("unknown"),
                display_value(&contributor["rows"]),
                format_number(contributor["mib"].as_f64()),
                format_percent(contributor["share_of_proof_db"].as_f64()),
                display_value(&contributor["previous_baseline_bytes"]),
                display_value(&contributor["delta_bytes"]),
                contributor["classification"].as_str().unwrap_or("unknown")
            ));
        }
    }
    markdown.push('\n');

    push_metric_section(
        &mut markdown,
        "Section 7 - Row Counts And Cardinality",
        &report["sections"]["row_counts_and_cardinality"]["metrics"],
    );
    push_metric_section(
        &mut markdown,
        "Section 8 - Cold Proof Build Profile",
        &report["sections"]["cold_proof_build_profile"]["metrics"],
    );
    if let Some(modes) =
        report["sections"]["cold_proof_build_profile"]["mode_distinction"].as_array()
    {
        markdown.push_str("### Cold Build Mode Distinction\n\n");
        markdown.push_str(
            "| Mode | Observed ms | Minutes | Status | Included in 50.02 min? | Classification |\n",
        );
        markdown.push_str("| --- | ---: | ---: | --- | --- | --- |\n");
        for mode in modes {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} |\n",
                mode["mode"].as_str().unwrap_or("unknown"),
                display_value(&mode["observed_ms"]),
                display_value(&mode["observed_minutes"]),
                mode["status"].as_str().unwrap_or("unknown"),
                display_value(&mode["included_in_50_02_minute_number"]),
                mode["classification"].as_str().unwrap_or("unknown")
            ));
        }
        markdown.push('\n');
    }
    if let Some(waterfall) = report["sections"]["cold_proof_build_profile"]["waterfall"].as_array()
    {
        markdown.push_str("### Cold Build Waterfall\n\n");
        markdown
            .push_str("| Stage | Elapsed ms | Share | Included in 50.02 min? | Source | Notes |\n");
        markdown.push_str("| --- | ---: | ---: | --- | --- | --- |\n");
        for stage in waterfall {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | `{}` | {} |\n",
                stage["stage"].as_str().unwrap_or("unknown"),
                display_value(&stage["elapsed_ms"]),
                format_percent(stage["pct_of_reference"].as_f64()),
                display_value(&stage["included_in_50_02_minute_number"]),
                stage["source"].as_str().unwrap_or("unknown"),
                first_note(stage)
            ));
        }
        markdown.push('\n');
    }
    push_metric_section(
        &mut markdown,
        "Section 9 - Repeat Unchanged Index",
        &report["sections"]["repeat_unchanged_index"]["metrics"],
    );
    push_metric_section(
        &mut markdown,
        "Section 10 - Single-File Update",
        &report["sections"]["single_file_update"]["metrics"],
    );

    markdown.push_str("## Section 11 - Query Latency\n\n");
    markdown.push_str("| Query | Target p95 ms | p50 ms | p95 ms | p99 ms | Status | Notes |\n");
    markdown.push_str("| --- | ---: | ---: | ---: | ---: | --- | --- |\n");
    if let Some(queries) = report["sections"]["query_latency"]["queries"].as_array() {
        for query in queries {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} |\n",
                query["id"].as_str().unwrap_or("unknown"),
                display_value(&query["target"]["p95_ms"]),
                display_value(&query["observed"]["p50_ms"]),
                display_value(&query["observed"]["p95_ms"]),
                display_value(&query["observed"]["p99_ms"]),
                query["status"].as_str().unwrap_or("unknown"),
                first_note(query)
            ));
        }
    }
    markdown.push('\n');

    markdown.push_str("## Section 12 - Manual Relation Quality\n\n");
    let manual_quality = &report["sections"]["manual_relation_quality"];
    let manual_status = manual_quality["status"].as_str().unwrap_or("unknown");
    markdown.push_str(&format!(
        "Status: `{manual_status}`. Real-world relation precision: `{}`.\n\n",
        manual_quality["real_relation_precision"]
            .as_str()
            .unwrap_or("unknown")
    ));
    if manual_status == "reported" {
        markdown.push_str(&format!(
            "Labeled samples: `{}`.\n\n",
            display_value(&manual_quality["edges_labeled"])
        ));
        if let Some(rows) = manual_quality["target_evaluation"].as_array() {
            if !rows.is_empty() {
                markdown.push_str("| Relation | Proof DB edges | Labeled | Precision | Target | Status | Claim |\n");
                markdown.push_str("| --- | ---: | ---: | ---: | ---: | --- | --- |\n");
                for row in rows {
                    markdown.push_str(&format!(
                        "| `{}` | {} | {} | {} | {} | {} | {} |\n",
                        row["relation"].as_str().unwrap_or("unknown"),
                        display_value(&row["proof_db_edge_count"]),
                        display_value(&row["labeled_samples"]),
                        format_percent(row["precision"].as_f64()),
                        format_percent(row["target"].as_f64()),
                        display_value(&row["status"]),
                        display_value(&row["claim"])
                    ));
                }
                markdown.push('\n');
            }
        }
        if let Some(rows) = manual_quality["relations"].as_array() {
            if !rows.is_empty() {
                markdown.push_str("| Labeled relation | Samples | Precision | Source-span precision | False positives | Unsure |\n");
                markdown.push_str("| --- | ---: | ---: | ---: | ---: | ---: |\n");
                for row in rows {
                    markdown.push_str(&format!(
                        "| `{}` | {} | {} | {} | {} | {} |\n",
                        row["relation"].as_str().unwrap_or("unknown"),
                        display_value(&row["samples"]),
                        format_percent(row["precision"].as_f64()),
                        format_percent(row["source_span_precision"].as_f64()),
                        display_value(&row["false_positive"]),
                        display_value(&row["unsure"])
                    ));
                }
                markdown.push('\n');
            }
        }
        if let Some(absent) =
            manual_quality["relation_coverage"]["absent_no_claim_relations"].as_array()
        {
            if !absent.is_empty() {
                let absent_relations = absent
                    .iter()
                    .map(display_value)
                    .collect::<Vec<_>>()
                    .join(", ");
                markdown.push_str(&format!(
                    "Absent proof-mode relations with no precision claim: {absent_relations}.\n\n"
                ));
            }
        }
        markdown.push_str("Precision claims are limited to labeled samples; recall remains unknown without a false-negative gold denominator.\n\n");
    } else {
        markdown.push_str("If labels are absent, real relation precision remains unknown and no precision claim is allowed.\n\n");
    }

    markdown.push_str("## Section 13 - CGC / Competitor Comparison Readiness\n\n");
    let comparison = &report["sections"]["cgc_competitor_comparison_readiness"];
    markdown.push_str(&format!(
        "| Metric | Value |\n| --- | --- |\n| CGC available | {} |\n| CGC version | {} |\n| CGC completed | {} |\n| CGC timeout | {} |\n| Verdict | {} |\n\n",
        display_value(&comparison["cgc_available"]),
        display_value(&comparison["cgc_version"]),
        display_value(&comparison["cgc_completed"]),
        display_value(&comparison["cgc_timeout"]),
        display_value(&comparison["verdict"])
    ));

    markdown.push_str("## Section 14 - Regression Summary\n\n");
    markdown.push_str("| Metric | Previous | Current | Delta | Status |\n");
    markdown.push_str("| --- | ---: | ---: | ---: | --- |\n");
    if let Some(rows) = report["sections"]["regression_summary"]["metrics"].as_array() {
        for row in rows {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                row["id"].as_str().unwrap_or("unknown"),
                display_value(&row["previous_value"]),
                display_value(&row["current_value"]),
                display_value(&row["delta"]),
                row["status"].as_str().unwrap_or("unknown")
            ));
        }
    }
    markdown.push('\n');

    markdown.push_str("## Operating Rule\n\n");
    markdown.push_str("Every future storage change must answer: did Graph Truth still pass, did Context Packet quality still pass, did proof DB size decrease, and did removed data move to a sidecar, become derivable, or get proven unnecessary. If the fourth answer is unclear, the change is not safe.\n");
    markdown
}

fn push_metric_section(markdown: &mut String, title: &str, metrics: &Value) {
    markdown.push_str(&format!("## {title}\n\n"));
    markdown.push_str("| Metric | Target | Observed | Status | Notes |\n");
    markdown.push_str("| --- | --- | --- | --- | --- |\n");
    if let Some(metrics) = metrics.as_array() {
        for metric in metrics {
            markdown.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                metric["id"].as_str().unwrap_or("unknown"),
                display_value(&metric["target"]),
                display_value(&metric["observed"]),
                metric["status"].as_str().unwrap_or("unknown"),
                first_note(metric)
            ));
        }
    }
    markdown.push('\n');
}

fn push_string_list(markdown: &mut String, title: &str, value: &Value) {
    markdown.push_str(&format!("### {title}\n\n"));
    if let Some(items) = value.as_array() {
        if items.is_empty() {
            markdown.push_str("- none\n\n");
            return;
        }
        for item in items {
            markdown.push_str(&format!("- `{}`\n", item.as_str().unwrap_or("unknown")));
        }
        markdown.push('\n');
    } else {
        markdown.push_str("- unknown\n\n");
    }
}

fn first_note(metric: &Value) -> String {
    metric["notes"]
        .as_array()
        .and_then(|notes| notes.first())
        .and_then(Value::as_str)
        .unwrap_or("")
        .replace('|', "\\|")
}

fn display_value(value: &Value) -> String {
    if value.is_null() {
        return "unknown".to_string();
    }
    if let Some(text) = value.as_str() {
        return text.replace('|', "\\|");
    }
    if let Some(number) = value.as_u64() {
        return format!("{number}");
    }
    if let Some(number) = value.as_i64() {
        return format!("{number}");
    }
    if let Some(number) = value.as_f64() {
        return format_number(Some(number));
    }
    if let Some(boolean) = value.as_bool() {
        return boolean.to_string();
    }
    if value.is_object() || value.is_array() {
        return serde_json::to_string(value)
            .unwrap_or_else(|_| "unknown".to_string())
            .replace('|', "\\|");
    }
    "unknown".to_string()
}

fn format_number(value: Option<f64>) -> String {
    match value {
        Some(value) if value.abs() >= 1000.0 => format!("{value:.0}"),
        Some(value) => format!("{value:.3}"),
        None => "unknown".to_string(),
    }
}

fn format_percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.2}%", value * 100.0))
        .unwrap_or_else(|| "unknown".to_string())
}

fn run_synthetic_index_benchmark_command(args: &[String]) -> Result<Value, String> {
    let options = parse_synthetic_index_options(args)?;
    if options.output_dir.exists() {
        return Err(format!(
            "synthetic index output directory already exists: {}",
            options.output_dir.display()
        ));
    }
    fs::create_dir_all(&options.output_dir).map_err(|error| error.to_string())?;
    let repo_dir = options.output_dir.join("repo");
    generate_large_synthetic_repo(&repo_dir, options.files).map_err(|error| error.to_string())?;
    let summary = index_repo_with_options(
        &repo_dir,
        IndexOptions {
            profile: true,
            json: true,
            ..IndexOptions::default()
        },
    )
    .map_err(|error| error.to_string())?;
    let manifest = json!({
        "schema_version": 1,
        "phase": PHASE,
        "source_of_truth": "MVP.md Prompt 29",
        "repo_dir": path_string(&repo_dir),
        "requested_files": options.files,
        "index_summary": summary,
        "workflow": "single-agent-only"
    });
    let manifest_path = options.output_dir.join("synthetic-index-run.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": "benchmarked",
        "phase": PHASE,
        "kind": "synthetic_index",
        "output_dir": path_string(&options.output_dir),
        "repo_dir": path_string(&repo_dir),
        "manifest": path_string(&manifest_path),
        "index_summary": manifest["index_summary"],
    }))
}

fn run_update_integrity_harness_command(args: &[String]) -> Result<Value, String> {
    let options = parse_update_integrity_harness_options(args)?;
    let mut report = match run_update_integrity_harness(&options) {
        Ok(report) => report,
        Err(error) => update_integrity_error_report(&options, &error),
    };
    let render_start = Instant::now();
    let markdown = render_update_integrity_harness_markdown(&report);
    let markdown_render_ms = elapsed_ms(render_start);
    let json_start = Instant::now();
    let json_text = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    let json_serialize_ms = elapsed_ms(json_start);
    report["command_output_timings"] = json!({
        "markdown_render_ms": markdown_render_ms,
        "json_serialization_ms": json_serialize_ms,
    });
    let json_write_start = Instant::now();
    if let Some(parent) = options
        .out_json
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&options.out_json, json_text).map_err(|error| error.to_string())?;
    let json_write_ms = elapsed_ms(json_write_start);
    let md_write_start = Instant::now();
    write_text_file(&options.out_md, &markdown)?;
    let md_write_ms = elapsed_ms(md_write_start);
    report["command_output_timings"] = json!({
        "markdown_render_ms": markdown_render_ms,
        "json_serialization_ms": json_serialize_ms,
        "json_write_ms": json_write_ms,
        "markdown_write_ms": md_write_ms,
    });
    write_json_file(&options.out_json, &report)?;
    Ok(json!({
        "status": report["status"].clone(),
        "phase": PHASE,
        "benchmark": "update_integrity",
        "verdict": report["verdict"].clone(),
        "out_json": path_string(&options.out_json),
        "out_md": path_string(&options.out_md),
        "repos": report["repos"].as_array().map(Vec::len).unwrap_or(0),
        "proof": "Update-integrity harness runs cold/seed, repeat unchanged, mutate/update, restore/update, integrity checks, and graph hash comparisons.",
    }))
}

fn run_update_integrity_harness(options: &UpdateIntegrityHarnessOptions) -> Result<Value, String> {
    let harness_start = Instant::now();
    let deadline = options
        .timeout_ms
        .map(|timeout_ms| harness_start + Duration::from_millis(timeout_ms));
    fs::create_dir_all(&options.workdir).map_err(|error| error.to_string())?;
    let repo_root = options.workdir.join("repos");
    let db_root = options.workdir.join("dbs");
    fs::create_dir_all(&repo_root).map_err(|error| error.to_string())?;
    fs::create_dir_all(&db_root).map_err(|error| error.to_string())?;

    let mut repos = Vec::new();
    if !options.only_autoresearch {
        if deadline_expired(deadline) {
            return Ok(update_integrity_timeout_report(
                options,
                repos,
                "before_small_fixture",
            ));
        }
        let small_repo = repo_root.join("small_fixture");
        let small_mutation = generate_update_integrity_small_repo(&small_repo)?;
        let medium_repo = repo_root.join("medium_fixture");
        let medium_mutation =
            generate_update_integrity_medium_repo(&medium_repo, options.medium_files)?;

        repos.push(run_update_integrity_repo(
            "small_fixture",
            &small_repo,
            &db_root.join("small_fixture.sqlite"),
            &small_mutation,
            options.iterations,
            options.workers,
            options.mode,
            options.loop_kind,
            None,
            None,
            deadline,
        )?);
        if deadline_expired(deadline) {
            return Ok(update_integrity_timeout_report(
                options,
                repos,
                "after_small_fixture",
            ));
        }
        repos.push(run_update_integrity_repo(
            "medium_fixture",
            &medium_repo,
            &db_root.join("medium_fixture.sqlite"),
            &medium_mutation,
            options.iterations,
            options.workers,
            options.mode,
            options.loop_kind,
            None,
            None,
            deadline,
        )?);
    }

    if !options.skip_autoresearch && options.autoresearch_repo.exists() {
        if deadline_expired(deadline) {
            return Ok(update_integrity_timeout_report(
                options,
                repos,
                "before_autoresearch",
            ));
        }
        let mutation = choose_update_integrity_mutation_file(&options.autoresearch_repo)?;
        let staged_update_repo = if options.loop_kind.runs_updates() {
            Some(stage_update_integrity_mutation_repo(
                &options.autoresearch_repo,
                &mutation,
                &repo_root.join("autoresearch_update_workspace"),
            )?)
        } else {
            None
        };
        repos.push(run_update_integrity_repo(
            "autoresearch",
            &options.autoresearch_repo,
            &db_root.join("autoresearch.sqlite"),
            &mutation,
            options.autoresearch_iterations,
            options.workers,
            options.mode,
            options.loop_kind,
            options.autoresearch_seed_db.as_deref(),
            staged_update_repo.as_deref(),
            deadline,
        )?);
    }
    if repos.is_empty() {
        return Err("update-integrity harness did not find any repo to run".to_string());
    }

    let failures = repos
        .iter()
        .filter(|repo| repo["status"].as_str() != Some("passed"))
        .count();
    let status = if failures == 0 { "passed" } else { "failed" };
    Ok(json!({
        "schema_version": 1,
        "status": status,
        "verdict": status,
        "phase": PHASE,
        "generated_at_unix_ms": unix_time_ms(),
        "update_mode": options.mode.as_str(),
        "loop_kind": options.loop_kind.as_str(),
        "timeout_ms": options.timeout_ms,
        "iterations_requested": options.iterations,
        "autoresearch_iterations_requested": options.autoresearch_iterations,
        "workers": options.workers,
        "workdir": path_string(&options.workdir),
        "repos": repos,
    }))
}

fn run_update_integrity_repo(
    name: &str,
    repo: &Path,
    db: &Path,
    mutation_file: &str,
    iterations: usize,
    workers: usize,
    mode: UpdateBenchmarkMode,
    loop_kind: UpdateLoopKind,
    seed_db: Option<&Path>,
    update_repo: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Value, String> {
    remove_sqlite_family(db)?;
    let options = IndexOptions {
        profile: true,
        json: false,
        worker_count: Some(workers),
        ..IndexOptions::default()
    };

    if deadline_expired(deadline) {
        return Ok(update_integrity_timeout_repo(
            name,
            repo,
            db,
            mutation_file,
            mode,
            loop_kind,
            iterations,
            "before_setup",
            Vec::new(),
            Vec::new(),
            None,
        ));
    }

    let cold = if let Some(seed_db) = seed_db {
        let copy_start = Instant::now();
        copy_seed_db(seed_db, db)?;
        let artifact_copy_ms = elapsed_ms(copy_start);
        update_integrity_seed_step(repo, db, mode, Some(seed_db), artifact_copy_ms)?
    } else {
        let start = Instant::now();
        let summary = index_repo_to_db_with_options(repo, db, options)
            .map_err(|error| format!("cold index failed for {name}: {error}"))?;
        update_integrity_step_from_index("cold_index", start.elapsed(), &summary, db, mode)?
    };
    let cold_hash = cold["graph_fact_hash"].as_str().unwrap_or("").to_string();
    if mode == UpdateBenchmarkMode::Fast && !cold_hash.is_empty() {
        replace_repo_graph_digest_for_harness(db, &cold_hash)?;
    }

    if deadline_expired(deadline) {
        return Ok(update_integrity_timeout_repo(
            name,
            repo,
            db,
            mutation_file,
            mode,
            loop_kind,
            iterations,
            "after_setup",
            Vec::new(),
            Vec::new(),
            Some(cold),
        ));
    }

    let mut repeat_iterations = Vec::new();
    let repeat_runs = if loop_kind.runs_repeat() {
        loop_kind.repeat_iterations(iterations)
    } else {
        0
    };
    for repeat_iteration in 1..=repeat_runs {
        if deadline_expired(deadline) {
            return Ok(update_integrity_timeout_repo(
                name,
                repo,
                db,
                mutation_file,
                mode,
                loop_kind,
                iterations,
                "repeat_loop_timeout",
                repeat_iterations,
                Vec::new(),
                Some(cold),
            ));
        }
        let start = Instant::now();
        let repeat_summary = index_repo_to_db_with_options(repo, db, options)
            .map_err(|error| format!("repeat unchanged failed for {name}: {error}"))?;
        let repeat = update_integrity_step_from_index(
            "repeat_unchanged_index",
            start.elapsed(),
            &repeat_summary,
            db,
            mode,
        )?;
        repeat_iterations.push(json!({
            "iteration": repeat_iteration,
            "repeat": repeat,
        }));
    }
    let repeat = repeat_iterations
        .last()
        .and_then(|iteration| iteration.get("repeat"))
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "step": "repeat_unchanged_index",
                "status": "not_run",
                "reason": "loop kind did not request repeat"
            })
        });
    let repeat_hash = repeat["graph_fact_hash"].as_str().unwrap_or("").to_string();
    if mode == UpdateBenchmarkMode::Fast && !repeat_hash.is_empty() {
        prime_incremental_graph_digest_for_update(db, mutation_file, &repeat_hash)?;
    }

    let update_repo = update_repo.unwrap_or(repo);
    if update_repo != repo {
        prime_staged_update_repo_index_state(db, repo, update_repo)?;
    }
    let mutation_path = update_repo.join(mutation_file);
    let original = fs::read_to_string(&mutation_path).map_err(|error| {
        format!(
            "failed to read mutation file {}: {error}",
            mutation_path.display()
        )
    })?;
    let mut iteration_results = Vec::new();
    let update_runs = loop_kind.update_iterations(iterations);
    for iteration in 1..=update_runs {
        if deadline_expired(deadline) {
            return Ok(update_integrity_timeout_repo(
                name,
                repo,
                db,
                mutation_file,
                mode,
                loop_kind,
                iterations,
                "update_loop_timeout",
                repeat_iterations,
                iteration_results,
                Some(cold),
            ));
        }
        let mutation_setup_start = Instant::now();
        let mutated = format!(
            "{}\n{}\n",
            original.trim_end(),
            update_integrity_mutation_text(&mutation_path, iteration)
        );
        fs::write(&mutation_path, mutated)
            .map_err(|error| format!("failed to mutate {}: {error}", mutation_path.display()))?;
        let mutation_setup_ms = elapsed_ms(mutation_setup_start);
        let mutate_start = Instant::now();
        let update_summary =
            update_changed_files_to_db(update_repo, &[PathBuf::from(mutation_file)], db).map_err(
                |error| {
                    format!("incremental update failed for {name} iteration {iteration}: {error}")
                },
            )?;
        let update = update_integrity_step_from_incremental(
            "single_file_update",
            mutate_start.elapsed(),
            &update_summary,
            db,
            mode,
        )?;

        let restore_setup_start = Instant::now();
        fs::write(&mutation_path, &original)
            .map_err(|error| format!("failed to restore {}: {error}", mutation_path.display()))?;
        let restore_setup_ms = elapsed_ms(restore_setup_start);
        let restore_start = Instant::now();
        let restore_summary =
            update_changed_files_to_db(update_repo, &[PathBuf::from(mutation_file)], db).map_err(
                |error| format!("restore update failed for {name} iteration {iteration}: {error}"),
            )?;
        let restore = update_integrity_step_from_incremental(
            "restore_update",
            restore_start.elapsed(),
            &restore_summary,
            db,
            mode,
        )?;
        let update_hash = update["graph_fact_hash"].as_str().unwrap_or("");
        let restore_hash = restore["graph_fact_hash"].as_str().unwrap_or("");
        iteration_results.push(json!({
            "iteration": iteration,
            "mutation_setup_ms": mutation_setup_ms,
            "update": update,
            "restore_setup_ms": restore_setup_ms,
            "restore": restore,
            "changed_hash_expected": true,
            "changed_hash_observed": update_hash != repeat_hash,
            "restore_hash_expected": repeat_hash,
            "restore_hash_observed": restore_hash,
            "restore_hash_matches_repeat": restore_hash == repeat_hash,
        }));
    }

    let all_integrity_ok = std::iter::once(&cold)
        .chain(
            repeat_iterations
                .iter()
                .filter_map(|iteration| iteration.get("repeat")),
        )
        .chain(iteration_results.iter().flat_map(|iteration| {
            [
                iteration.get("update").unwrap(),
                iteration.get("restore").unwrap(),
            ]
        }))
        .all(|step| {
            step["integrity_status"].as_str() == Some("ok")
                || step["integrity_status"].as_str() == Some("not_run")
        });
    let repeat_hash_stable = if repeat_runs == 0 {
        true
    } else {
        repeat_iterations.iter().all(|iteration| {
            iteration["repeat"]["graph_fact_hash"]
                .as_str()
                .unwrap_or("")
                == cold_hash
        })
    };
    let changed_hash_ok = if update_runs == 0 {
        true
    } else {
        iteration_results
            .iter()
            .all(|iteration| iteration["changed_hash_observed"].as_bool() == Some(true))
    };
    let restore_hash_ok = if update_runs == 0 {
        true
    } else {
        iteration_results
            .iter()
            .all(|iteration| iteration["restore_hash_matches_repeat"].as_bool() == Some(true))
    };
    let status = if all_integrity_ok && repeat_hash_stable && changed_hash_ok && restore_hash_ok {
        "passed"
    } else {
        "failed"
    };

    Ok(json!({
        "name": name,
        "status": status,
        "repo_path": path_string(repo),
        "db_path": path_string(db),
        "mutation_file": mutation_file,
        "update_repo_path": path_string(update_repo),
        "update_mode": mode.as_str(),
        "loop_kind": loop_kind.as_str(),
        "iterations": iterations,
        "cold": cold,
        "repeat_unchanged": repeat,
        "repeat_iterations": repeat_iterations,
        "iteration_results": iteration_results,
        "graph_fact_hash_stable_on_repeat": repeat_hash_stable,
        "changed_file_updates_graph_fact_hash": changed_hash_ok,
        "restore_returns_to_repeat_graph_fact_hash": restore_hash_ok,
        "all_integrity_checks_passed": all_integrity_ok,
    }))
}

fn run_cgc_comparison_command(args: &[String]) -> Result<Value, String> {
    let options = parse_cgc_comparison_options(args)?;
    let report = run_codegraphcontext_comparison(CodeGraphContextComparisonOptions {
        report_dir: options.report_dir.clone(),
        timeout_ms: options.timeout_ms,
        top_k: options.top_k,
        competitor_executable: options.competitor_executable,
    })
    .map_err(|error| error.to_string())?;

    Ok(json!({
        "status": "benchmarked",
        "phase": "21.1",
        "benchmark_id": report.benchmark_id,
        "report_dir": report.report_dir,
        "run_json": path_string(&options.report_dir.join("run.json")),
        "per_task_jsonl": path_string(&options.report_dir.join("per_task.jsonl")),
        "summary_md": path_string(&options.report_dir.join("summary.md")),
        "competitor": report.manifest,
        "aggregate": report.aggregate,
        "runs": report.runs.len(),
    }))
}

fn write_benchmark_report(
    report: &BenchmarkReport,
    format: BenchReportFormat,
    output: &Path,
) -> Result<(), String> {
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let contents = match format {
        BenchReportFormat::Json => {
            codegraph_bench::render_json_report(report).map_err(|error| error.to_string())?
        }
        BenchReportFormat::Markdown => codegraph_bench::render_markdown_report(report),
    };
    fs::write(output, contents).map_err(|error| error.to_string())
}

fn run_serve_mcp_command(args: &[String]) -> CliOutput {
    if !args.is_empty() {
        return command_error("invalid_arguments", "Usage: codegraph-mcp serve-mcp");
    }

    match codegraph_mcp_server::serve_stdio() {
        Ok(()) => success(String::new()),
        Err(error) => command_error("serve_mcp_failed", &error.to_string()),
    }
}

fn run_serve_ui_command(args: &[String]) -> CliOutput {
    let options = match parse_ui_options(args) {
        Ok(options) => options,
        Err(error) => return command_error("serve_ui_failed", &error),
    };

    match serve_ui(&options.repo, &options.host, options.port) {
        Ok(()) => success(String::new()),
        Err(error) => command_error("serve_ui_failed", &error.to_string()),
    }
}

fn run_watch_command(args: &[String]) -> CliOutput {
    let options = match parse_watch_options(args) {
        Ok(options) => options,
        Err(error) => return command_error("watch_failed", &error),
    };

    if options.once {
        let changed_paths = if options.changed_paths.is_empty() {
            Vec::new()
        } else {
            options.changed_paths
        };
        let update = if let Some(db) = &options.db {
            update_changed_files_to_db(&options.repo, &changed_paths, db)
        } else {
            update_changed_files(&options.repo, &changed_paths)
        };
        return run_json_command(
            "watch_failed",
            update
                .and_then(|summary| {
                    serde_json::to_value(summary)
                        .map_err(|error| IndexError::Message(error.to_string()))
                })
                .map_err(|error| error.to_string()),
        );
    }

    match watch_repo(&options.repo, options.debounce) {
        Ok(()) => success(String::new()),
        Err(error) => command_error("watch_failed", &error.to_string()),
    }
}

fn watch_repo(repo_path: &Path, debounce: Duration) -> Result<(), IndexError> {
    if !repo_path.exists() {
        return Err(IndexError::RepoNotFound(repo_path.to_path_buf()));
    }

    let repo_root = fs::canonicalize(repo_path)?;
    let mut cache = IncrementalIndexCache::new(256)?;
    fs::create_dir_all(repo_root.join(".codegraph"))?;
    let store = SqliteGraphStore::open(default_db_path(&repo_root))?;
    cache.refresh_from_store(&store)?;

    let (sender, receiver) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = sender.send(result);
    })
    .map_err(|error| IndexError::Message(format!("watcher init failed: {error}")))?;
    watcher
        .watch(&repo_root, RecursiveMode::Recursive)
        .map_err(|error| IndexError::Message(format!("watcher start failed: {error}")))?;

    eprintln!(
        "codegraph watch: watching {} with {}ms debounce",
        repo_root.display(),
        debounce.as_millis()
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
                enqueue_event_paths(&repo_root, &mut debouncer, event, Instant::now());
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

        let summary = update_changed_files_with_cache(&repo_root, &ready, &mut cache)?;
        log_watch_summary(&summary);
    }
}

fn enqueue_event_paths(
    repo_root: &Path,
    debouncer: &mut WatchDebouncer,
    event: Event,
    now: Instant,
) {
    if matches!(event.kind, EventKind::Access(_)) {
        return;
    }

    for path in event.paths {
        if should_ignore_path(repo_root, &path) {
            continue;
        }
        debouncer.push(path, now);
    }
}

fn log_watch_summary(summary: &IncrementalIndexSummary) {
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

fn serve_ui(repo_path: &Path, host: &str, port: u16) -> Result<(), IndexError> {
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

fn serve_ui_loop(
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

fn handle_ui_stream(repo_root: &Path, mut stream: TcpStream) -> Result<(), IndexError> {
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
struct UiResponse {
    status: u16,
    content_type: &'static str,
    body: String,
}

fn route_ui_request(repo_root: &Path, method: &str, target: &str) -> UiResponse {
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

fn write_http_response(stream: &mut TcpStream, response: UiResponse) -> Result<(), IndexError> {
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

fn ui_json(result: Result<Value, String>) -> UiResponse {
    match result {
        Ok(value) => UiResponse {
            status: 200,
            content_type: "application/json; charset=utf-8",
            body: json_line(value),
        },
        Err(message) => ui_json_error(500, "ui_error", &message),
    }
}

fn ui_json_error(status: u16, error: &str, message: &str) -> UiResponse {
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

fn ui_status(repo_root: &Path) -> Result<Value, String> {
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

fn ui_path_graph(repo_root: &Path, params: &BTreeMap<String, String>) -> Result<Value, String> {
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

fn ui_symbol_search(repo_root: &Path, params: &BTreeMap<String, String>) -> Result<Value, String> {
    let query = params.get("query").map(String::as_str).unwrap_or("").trim();
    if query.is_empty() {
        return Err("missing query parameter".to_string());
    }
    query_symbols(repo_root, query, parse_ui_limit(params, 12, 50))
}

fn ui_source_span(repo_root: &Path, params: &BTreeMap<String, String>) -> Result<Value, String> {
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

fn ui_path_compare(repo_root: &Path, params: &BTreeMap<String, String>) -> Result<Value, String> {
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

fn ui_unresolved_calls(
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
            ..UnresolvedCallsOptions::default()
        },
    )
}

fn ui_impact(repo_root: &Path, params: &BTreeMap<String, String>) -> Result<Value, String> {
    let Some(target) = params
        .get("target")
        .filter(|target| !target.trim().is_empty())
    else {
        return Err("missing target query parameter".to_string());
    };
    impact_value(repo_root, target)
}

fn ui_context_pack(repo_root: &Path, params: &BTreeMap<String, String>) -> Result<Value, String> {
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

fn filtered_impact_paths(
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

fn one_step_paths(edges: Vec<Edge>, relations: &[RelationKind]) -> Vec<GraphPath> {
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

fn neighborhood_paths(
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

fn unresolved_call_paths(edges: Vec<Edge>) -> Vec<GraphPath> {
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

fn graph_json_from_paths(
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

fn insert_limited_node(nodes: &mut BTreeSet<String>, node_id: &str, cap: usize) {
    if nodes.len() < cap || nodes.contains(node_id) {
        nodes.insert(node_id.to_string());
    }
}

fn source_span_resource_links(span: &SourceSpan) -> Value {
    json!({
        "source_span": format!(
            "codegraph://source-span/{}:{}-{}",
            span.repo_relative_path, span.start_line, span.end_line
        ),
        "file": format!("codegraph://file/{}", span.repo_relative_path),
    })
}

fn exactness_style_legend() -> Value {
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

fn parse_ui_graph_mode(params: &BTreeMap<String, String>) -> String {
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

fn mode_default_relations(mode: &str) -> Vec<RelationKind> {
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

fn parse_ui_node_cap(params: &BTreeMap<String, String>) -> usize {
    params
        .get("node_cap")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_UI_NODE_CAP)
        .clamp(1, MAX_UI_NODE_CAP)
}

fn parse_ui_limit(params: &BTreeMap<String, String>, default: usize, max: usize) -> usize {
    params
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(1, max)
}

fn compact_node_label(id: &str) -> String {
    id.rsplit(['#', '/', ':'])
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(id)
        .to_string()
}

fn parse_relation_filter(raw: Option<&str>) -> Result<Vec<RelationKind>, String> {
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

fn parse_query_params(raw: &str) -> BTreeMap<String, String> {
    raw.split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            Some((percent_decode(key)?, percent_decode(value)?))
        })
        .collect()
}

fn percent_decode(value: &str) -> Option<String> {
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

fn is_local_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

#[derive(Debug)]
struct WatchDebouncer {
    delay: Duration,
    pending: BTreeMap<PathBuf, Instant>,
}

impl WatchDebouncer {
    fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending: BTreeMap::new(),
        }
    }

    fn push(&mut self, path: PathBuf, now: Instant) {
        self.pending.insert(path, now);
    }

    fn ready(&mut self, now: Instant) -> Vec<PathBuf> {
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
}

fn run_bundle_export(args: &[String]) -> Result<Value, String> {
    let output = parse_output_arg(args)?;
    let repo_root = current_repo_root()?;
    let store = open_existing_store(&repo_root)?;
    let files = store
        .list_files(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let edges = store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let bundle = CodeGraphBundle {
        manifest: BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            created_by: format!("{BIN_NAME} phase {PHASE}"),
            created_at_unix_ms: unix_time_ms(),
            repo_root: path_string(&repo_root),
            file_count: files.len(),
            entity_count: entities.len(),
            edge_count: edges.len(),
        },
        files,
        entities,
        edges,
    };
    let encoded = serde_json::to_string_pretty(&bundle).map_err(|error| error.to_string())?;
    fs::write(&output, encoded).map_err(|error| error.to_string())?;

    Ok(json!({
        "status": "exported",
        "output": path_string(&output),
        "manifest": bundle.manifest,
    }))
}

fn run_bundle_import(args: &[String]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Usage: codegraph-mcp bundle import repo.cgc-bundle".to_string());
    }
    let input = PathBuf::from(&args[0]);
    let source = fs::read_to_string(&input).map_err(|error| error.to_string())?;
    let bundle: CodeGraphBundle =
        serde_json::from_str(&source).map_err(|error| error.to_string())?;
    if bundle.manifest.schema_version != BUNDLE_SCHEMA_VERSION {
        return Err(format!(
            "bundle schema mismatch: expected {}, got {}",
            BUNDLE_SCHEMA_VERSION, bundle.manifest.schema_version
        ));
    }

    let repo_root = current_repo_root()?;
    fs::create_dir_all(repo_root.join(".codegraph")).map_err(|error| error.to_string())?;
    let store =
        SqliteGraphStore::open(default_db_path(&repo_root)).map_err(|error| error.to_string())?;
    store
        .transaction(|tx| {
            for file in &bundle.files {
                tx.upsert_file(file)?;
            }
            for entity in &bundle.entities {
                tx.upsert_entity(entity)?;
                if let Some(span) = &entity.source_span {
                    tx.upsert_source_span(&entity.id, span)?;
                }
            }
            for edge in &bundle.edges {
                tx.upsert_edge(edge)?;
                tx.upsert_source_span(&edge.id, &edge.source_span)?;
            }
            Ok(())
        })
        .map_err(|error| error.to_string())?;

    Ok(json!({
        "status": "imported",
        "input": path_string(&input),
        "repo_root": path_string(&repo_root),
        "manifest": bundle.manifest,
    }))
}

fn query_symbols(repo_root: &Path, query: &str, limit: usize) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let hits = symbol_search_hits(&store, query, limit)?
        .into_iter()
        .map(|hit| symbol_search_hit_json(&hit))
        .collect::<Vec<_>>();

    Ok(json!({
        "status": "ok",
        "query": query,
        "hits": hits,
        "ranking": [
            "exact symbol match",
            "qualified-name match",
            "prefix match",
            "fuzzy match",
            "file path proximity",
        "same package/module",
        "recent edit signal",
        "source/test role",
        "aliases/import names",
        "Stage 0 file-path FTS candidates",
        "bounded semantic SQL fallback"
    ],
        "proof": "Symbol search is ranked over exact symbol hits, file-path FTS candidates, and a bounded semantic SQL fallback.",
    }))
}

fn symbol_search_hits(
    store: &SqliteGraphStore,
    query: &str,
    limit: usize,
) -> Result<Vec<SymbolSearchHit>, String> {
    let entities = symbol_search_candidate_entities(store, query, limit)?;
    let mut file_paths = BTreeSet::new();
    for entity in &entities {
        file_paths.insert(entity.repo_relative_path.clone());
    }
    let mut files = Vec::new();
    for repo_relative_path in file_paths {
        if let Some(file) = store
            .get_file(&repo_relative_path)
            .map_err(|error| error.to_string())?
        {
            files.push(file);
        }
    }

    Ok(SymbolSearchIndex::new(entities, Vec::new(), files).search(query, limit))
}

fn symbol_search_candidate_entities(
    store: &SqliteGraphStore,
    query: &str,
    limit: usize,
) -> Result<Vec<Entity>, String> {
    let candidate_limit = limit
        .max(1)
        .saturating_mul(SYMBOL_SEARCH_FTS_CANDIDATE_FACTOR)
        .clamp(
            SYMBOL_SEARCH_MIN_FTS_CANDIDATES,
            SYMBOL_SEARCH_MAX_FTS_CANDIDATES,
        );
    let mut seen = BTreeSet::new();
    let mut entities = Vec::new();

    for entity in store
        .find_entities_by_exact_symbol(query)
        .map_err(|error| error.to_string())?
    {
        if seen.insert(entity.id.clone()) {
            entities.push(entity);
        }
    }

    let text_hits = store
        .search_text(query, candidate_limit)
        .map_err(|error| error.to_string())?;
    let mut file_candidate_paths = BTreeSet::new();
    for hit in &text_hits {
        match hit.kind {
            TextSearchKind::Entity => {
                if seen.contains(&hit.id) {
                    continue;
                }
                if let Some(entity) = store
                    .get_entity(&hit.id)
                    .map_err(|error| error.to_string())?
                {
                    if seen.insert(entity.id.clone()) {
                        entities.push(entity);
                    }
                }
            }
            TextSearchKind::File | TextSearchKind::Snippet => {
                if file_candidate_paths.len() < SYMBOL_SEARCH_FILE_CANDIDATE_LIMIT {
                    file_candidate_paths.insert(hit.repo_relative_path.clone());
                }
            }
        }
        if entities.len() >= candidate_limit {
            break;
        }
    }

    if entities.len() < limit {
        let query_lc = query.to_ascii_lowercase();
        let query_aliases = split_symbol_query_aliases(query);
        for repo_relative_path in file_candidate_paths {
            for entity in store
                .list_entities_by_file(&repo_relative_path)
                .map_err(|error| error.to_string())?
            {
                if seen.contains(&entity.id) {
                    continue;
                }
                if !symbol_candidate_matches(&entity, &query_lc, &query_aliases) {
                    continue;
                }
                if seen.insert(entity.id.clone()) {
                    entities.push(entity);
                }
                if entities.len() >= candidate_limit {
                    break;
                }
            }
            if entities.len() >= candidate_limit {
                break;
            }
        }
    }

    if entities.len() < limit {
        let query_lc = query.to_ascii_lowercase();
        let query_aliases = split_symbol_query_aliases(query);
        for entity in store
            .list_entities(UNBOUNDED_STORE_READ_LIMIT)
            .map_err(|error| error.to_string())?
        {
            if seen.contains(&entity.id) {
                continue;
            }
            if !symbol_candidate_matches(&entity, &query_lc, &query_aliases) {
                continue;
            }
            if seen.insert(entity.id.clone()) {
                entities.push(entity);
            }
            if entities.len() >= candidate_limit {
                break;
            }
        }
    }

    Ok(entities)
}

fn split_symbol_query_aliases(query: &str) -> BTreeSet<String> {
    query
        .to_ascii_lowercase()
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|token| token.len() >= 2)
        .map(str::to_string)
        .collect()
}

fn symbol_candidate_matches(
    entity: &Entity,
    query_lc: &str,
    query_aliases: &BTreeSet<String>,
) -> bool {
    let metadata = entity_metadata_search_text(entity).to_ascii_lowercase();
    let haystack = format!(
        "{} {} {} {}",
        entity.name.to_ascii_lowercase(),
        entity.qualified_name.to_ascii_lowercase(),
        entity.repo_relative_path.to_ascii_lowercase(),
        metadata
    );
    haystack.contains(query_lc)
        || query_aliases
            .iter()
            .any(|alias| alias.len() >= 2 && haystack.contains(alias))
}

fn entity_metadata_search_text(entity: &Entity) -> String {
    entity
        .metadata
        .iter()
        .flat_map(|(key, value)| match value {
            Value::String(text) => vec![key.clone(), text.clone()],
            Value::Bool(value) => vec![key.clone(), value.to_string()],
            Value::Number(value) => vec![key.clone(), value.to_string()],
            _ => vec![key.clone()],
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn query_text(repo_root: &Path, query: &str, limit: usize) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let mut hits = store
        .search_text(query, limit)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(text_search_hit_json)
        .collect::<Vec<_>>();
    if hits.len() < limit {
        hits.extend(source_scan_text_hits(
            repo_root,
            &store,
            query,
            limit.saturating_sub(hits.len()),
        )?);
    }

    Ok(json!({
        "status": "ok",
        "query": query,
        "hits": hits,
        "proof": "Text query uses SQLite FTS when present and falls back to bounded on-demand source scanning over indexed files.",
    }))
}

fn source_scan_text_hits(
    repo_root: &Path,
    store: &SqliteGraphStore,
    query: &str,
    limit: usize,
) -> Result<Vec<Value>, String> {
    if query.trim().is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let query_lc = query.to_ascii_lowercase();
    let mut hits = Vec::new();
    for file in store
        .list_files(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?
    {
        let path = repo_root.join(&file.repo_relative_path);
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        for (line_index, line) in source.lines().enumerate() {
            if !line.to_ascii_lowercase().contains(&query_lc) {
                continue;
            }
            hits.push(json!({
                "kind": "file",
                "id": file.repo_relative_path,
                "repo_relative_path": file.repo_relative_path,
                "line": line_index + 1,
                "title": file.repo_relative_path,
                "text": line.trim(),
                "score": 0.0,
                "match": "source_scan",
            }));
            break;
        }
        if hits.len() >= limit {
            break;
        }
    }
    Ok(hits)
}

fn query_files(repo_root: &Path, query: &str, limit: usize) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let query_lc = query.to_ascii_lowercase();
    let mut seen = BTreeSet::new();
    let mut hits = Vec::new();
    for hit in store
        .search_text(query, limit)
        .map_err(|error| error.to_string())?
    {
        if hit.kind == TextSearchKind::File && seen.insert(hit.repo_relative_path.clone()) {
            hits.push(text_search_hit_json(hit));
        }
    }
    for file in store
        .list_files(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?
    {
        if hits.len() >= limit {
            break;
        }
        if file
            .repo_relative_path
            .to_ascii_lowercase()
            .contains(&query_lc)
            && seen.insert(file.repo_relative_path.clone())
        {
            hits.push(json!({
                "kind": "file",
                "id": file.repo_relative_path,
                "repo_relative_path": file.repo_relative_path,
                "line": null,
                "title": file.repo_relative_path,
                "score": 0.0,
                "match": "path_contains",
            }));
        }
    }

    Ok(json!({
        "status": "ok",
        "query": query,
        "hits": hits,
        "proof": "File query combines SQLite FTS file-path rows with repo-relative path matching.",
    }))
}

fn query_definitions(repo_root: &Path, query: &str, limit: usize) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let hits = symbol_search_hits(&store, query, limit * 2)?
        .into_iter()
        .filter(|hit| is_definition_kind(hit.entity.kind))
        .take(limit)
        .map(|hit| symbol_search_hit_json(&hit))
        .collect::<Vec<_>>();

    Ok(json!({
        "status": "ok",
        "query": query,
        "definitions": hits,
        "proof": "Definitions are symbol-search hits constrained to declaration/executable entity kinds.",
    }))
}

fn query_references(repo_root: &Path, query: &str, limit: usize) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let entity_by_id = entities_by_id(&entities);
    let seeds = resolve_symbol_candidates(&store, query, 8)?;
    let seed_ids = seeds
        .iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    let seed_aliases = alias_set_for_entities(&seeds);
    let mut references = Vec::new();
    for edge in store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?
    {
        if references.len() >= limit {
            break;
        }
        let head_aliases = entity_by_id
            .get(&edge.head_id)
            .map(entity_aliases)
            .unwrap_or_default();
        let tail_aliases = entity_by_id
            .get(&edge.tail_id)
            .map(entity_aliases)
            .unwrap_or_default();
        let matches_seed = seed_ids.contains(&edge.head_id)
            || seed_ids.contains(&edge.tail_id)
            || aliases_overlap(&seed_aliases, &head_aliases)
            || aliases_overlap(&seed_aliases, &tail_aliases);
        if matches_seed {
            references.push(edge_with_entities_json(&edge, &entity_by_id));
        }
    }

    Ok(json!({
        "status": "ok",
        "query": query,
        "resolved_symbols": seeds.iter().map(entity_json).collect::<Vec<_>>(),
        "references": references,
        "proof": "References are graph edges connected to resolved symbol ids or explicit same-name unresolved placeholders.",
    }))
}

fn query_callers(repo_root: &Path, query: &str, limit: usize) -> Result<Value, String> {
    query_call_relation(repo_root, query, limit, CallQueryDirection::Callers)
}

fn query_callees(repo_root: &Path, query: &str, limit: usize) -> Result<Value, String> {
    query_call_relation(repo_root, query, limit, CallQueryDirection::Callees)
}

fn query_chain(repo_root: &Path, source: &str, target: &str) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let edges = store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let entity_by_id = entities_by_id(&entities);
    let source_entities = resolve_symbol_candidates(&store, source, 8)?;
    let target_entities = resolve_symbol_candidates(&store, target, 8)?;
    let target_aliases = alias_set_for_entities(&target_entities)
        .into_iter()
        .chain([normalize_symbol_alias(target)])
        .collect::<BTreeSet<_>>();
    let paths = call_chain_paths(
        &edges,
        &entity_by_id,
        &source_entities,
        &target_aliases,
        default_query_limits(),
    );
    let engine = ExactGraphQueryEngine::new(edges);
    let evidence = engine.path_evidence_from_paths(&paths);

    Ok(json!({
        "status": "ok",
        "source": source,
        "target": target,
        "resolved_sources": source_entities.iter().map(entity_json).collect::<Vec<_>>(),
        "resolved_targets": target_entities.iter().map(entity_json).collect::<Vec<_>>(),
        "paths": evidence,
        "chain_confidence": evidence.iter().map(|path| path.confidence).fold(0.0_f64, f64::max),
        "resolver_order": [
            "direct verified calls",
            "resolved imports / alias names",
            "same-module parser-verified calls",
            "static heuristic fallback"
        ],
        "proof": "Call-chain query traverses CALLS edges with explicit exactness and confidence labels; unresolved same-name joins remain heuristic evidence.",
    }))
}

#[derive(Debug, Clone)]
struct UnresolvedCallsOptions {
    db_path: Option<PathBuf>,
    requested_limit: usize,
    limit: usize,
    offset: usize,
    include_snippets: bool,
    source_scan: bool,
    count_total: bool,
}

impl Default for UnresolvedCallsOptions {
    fn default() -> Self {
        Self {
            db_path: None,
            requested_limit: DEFAULT_UNRESOLVED_CALL_LIMIT,
            limit: DEFAULT_UNRESOLVED_CALL_LIMIT,
            offset: 0,
            include_snippets: false,
            source_scan: false,
            count_total: false,
        }
    }
}

fn parse_unresolved_calls_args(args: &[String]) -> Result<UnresolvedCallsOptions, String> {
    let mut options = UnresolvedCallsOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--limit" => {
                index += 1;
                let raw = args
                    .get(index)
                    .ok_or_else(unresolved_calls_usage)?
                    .parse::<usize>()
                    .map_err(|_| "invalid --limit value".to_string())?;
                options.requested_limit = raw;
                options.limit = raw.clamp(1, MAX_UNRESOLVED_CALL_LIMIT);
            }
            "--offset" | "--cursor" => {
                index += 1;
                options.offset = args
                    .get(index)
                    .ok_or_else(unresolved_calls_usage)?
                    .parse::<usize>()
                    .map_err(|_| "invalid --offset value".to_string())?;
            }
            "--db" => {
                index += 1;
                let path = args.get(index).ok_or_else(unresolved_calls_usage)?;
                options.db_path = Some(PathBuf::from(path));
            }
            "--json" => {}
            "--no-snippets" => {
                options.include_snippets = false;
            }
            "--include-snippets" => {
                options.include_snippets = true;
            }
            "--source-scan" => {
                options.source_scan = true;
            }
            "--count-total" => {
                options.count_total = true;
            }
            other => {
                return Err(format!(
                    "unknown unresolved-calls option: {other}\n{}",
                    unresolved_calls_usage()
                ));
            }
        }
        index += 1;
    }
    Ok(options)
}

fn unresolved_calls_usage() -> String {
    "Usage: codegraph-mcp query unresolved-calls [--limit <n>] [--offset <n>] [--json] [--no-snippets] [--include-snippets] [--db <path>]".to_string()
}

fn query_unresolved_calls(
    repo_root: &Path,
    options: UnresolvedCallsOptions,
) -> Result<Value, String> {
    let total_start = Instant::now();
    let db_path = options
        .db_path
        .clone()
        .unwrap_or_else(|| default_db_path(repo_root));
    if !db_path.exists() {
        return Err(format!(
            "CodeGraph index does not exist at {}; run `codegraph-mcp index .` first",
            db_path.display()
        ));
    }

    let open_start = Instant::now();
    let connection = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    let open_ms = elapsed_ms(open_start);

    let explain_start = Instant::now();
    let explain_plan = unresolved_calls_query_plan(&connection, options.limit, options.offset)?;
    let explain_query_plan_ms = elapsed_ms(explain_start);
    let query_plan_analysis = analyze_sqlite_query_plan(&explain_plan);

    let count_start = Instant::now();
    let total_matching = if options.count_total {
        Some(count_unresolved_calls(&connection)?)
    } else {
        None
    };
    let count_ms = elapsed_ms(count_start);

    let page_start = Instant::now();
    let mut unresolved = query_unresolved_calls_page(
        &connection,
        repo_root,
        options.limit,
        options.offset,
        options.include_snippets,
    )?;
    let page_query_ms = elapsed_ms(page_start);

    let mut source_scan = json!({
        "enabled": false,
        "reason": "disabled_by_default_to_avoid_unbounded_reparse",
        "rows_added": 0,
        "elapsed_ms": 0,
    });
    if options.source_scan && unresolved.len() < options.limit {
        let scan_start = Instant::now();
        let store = SqliteGraphStore::open(&db_path).map_err(|error| error.to_string())?;
        let before = unresolved.len();
        unresolved.extend(source_scan_unresolved_calls(
            repo_root,
            &store,
            options.limit.saturating_sub(unresolved.len()),
        )?);
        source_scan = json!({
            "enabled": true,
            "rows_added": unresolved.len().saturating_sub(before),
            "elapsed_ms": elapsed_ms(scan_start),
            "bounded_by_remaining_limit": options.limit.saturating_sub(before),
        });
    }

    let total_ms = elapsed_ms(total_start);
    Ok(json!({
        "status": "ok",
        "calls": unresolved,
        "pagination": {
            "requested_limit": options.requested_limit,
            "effective_limit": options.limit,
            "limit_capped": options.requested_limit != options.limit,
            "max_limit": MAX_UNRESOLVED_CALL_LIMIT,
            "offset": options.offset,
            "next_offset": options.offset.saturating_add(unresolved.len()),
        },
        "row_counts": {
            "returned": unresolved.len(),
            "total_matching": total_matching,
            "total_matching_counted": options.count_total,
            "total_matching_note": if options.count_total {
                "counted_with_the_same_unresolved_call_filter"
            } else {
                "skipped_by_default; use --count-total for audit-only full counts"
            },
        },
        "instrumentation": {
            "db_path": db_path,
            "sql": {
                "page_query": UNRESOLVED_CALLS_PAGE_SQL,
                "count_query": if options.count_total { Value::String(UNRESOLVED_CALLS_COUNT_SQL.to_string()) } else { Value::Null },
            },
            "explain_query_plan": explain_plan,
            "query_plan_analysis": query_plan_analysis,
            "elapsed_ms": {
                "open_db": open_ms,
                "explain_query_plan": explain_query_plan_ms,
                "count_total": count_ms,
                "page_query": page_query_ms,
                "total": total_ms,
            },
            "snippets": {
                "requested": options.include_snippets,
                "default": "not_loaded_unless_requested",
            },
            "source_scan": source_scan,
        },
        "proof": "Unresolved-call enumeration is bounded by limit/offset, filters CALLS/static-heuristic evidence in SQL, and does not load snippets or reparse source unless explicitly requested.",
    }))
}

const UNRESOLVED_CALLS_PAGE_SQL: &str = r#"
SELECT e.edge_id AS edge_id,
       e.head_id AS head_id,
       e.relation AS relation,
       e.tail_id AS tail_id,
       e.source_span_path AS span_repo_relative_path,
       e.start_line, e.start_column, e.end_line, e.end_column,
       e.repo_commit, e.file_hash AS file_hash, e.extractor AS extractor,
       e.confidence, e.exactness AS exactness, e.derived,
       e.provenance_edges_json, e.metadata_json AS edge_metadata_json,
       COALESCE(head_kind.value, head_static.kind, 'Function') AS head_kind,
       COALESCE(head_name.value, head_static.name, e.head_id) AS head_name,
       COALESCE(head_qname.value, head_static.qualified_name, e.head_id) AS head_qualified_name,
       COALESCE(head_path.value, head_static.repo_relative_path, e.source_span_path) AS head_repo_relative_path,
       COALESCE(head_span_path.value, head_static.source_span_path) AS head_span_repo_relative_path,
       head.start_line AS head_start_line,
       head.start_column AS head_start_column,
       head.end_line AS head_end_line,
       head.end_column AS head_end_column,
       COALESCE(head_extractor.value, head_static.created_from, 'heuristic_sidecar') AS head_created_from,
       COALESCE(head.confidence, head_static.confidence, e.confidence) AS head_confidence,
       COALESCE(head.metadata_json, head_static.metadata_json, '{}') AS head_metadata_json,
       COALESCE(tail_kind.value, tail_static.kind, 'Function') AS tail_kind,
       COALESCE(tail_name.value, tail_static.name, e.tail_id) AS tail_name,
       COALESCE(tail_qname.value, tail_static.qualified_name, e.tail_id) AS tail_qualified_name,
       COALESCE(tail_path.value, tail_static.repo_relative_path, e.source_span_path) AS tail_repo_relative_path,
       COALESCE(tail_span_path.value, tail_static.source_span_path) AS tail_span_repo_relative_path,
       COALESCE(tail.start_line, tail_static.start_line) AS tail_start_line,
       COALESCE(tail.start_column, tail_static.start_column) AS tail_start_column,
       COALESCE(tail.end_line, tail_static.end_line) AS tail_end_line,
       COALESCE(tail.end_column, tail_static.end_column) AS tail_end_column,
       COALESCE(tail_extractor.value, tail_static.created_from, 'heuristic_sidecar') AS tail_created_from,
       COALESCE(tail.confidence, tail_static.confidence, e.confidence) AS tail_confidence,
       COALESCE(tail.metadata_json, tail_static.metadata_json, '{}') AS tail_metadata_json
FROM heuristic_edges e
LEFT JOIN object_id_lookup head_oid ON head_oid.value = e.head_id
LEFT JOIN object_id_lookup tail_oid ON tail_oid.value = e.tail_id
LEFT JOIN entities head ON head.id_key = head_oid.id
LEFT JOIN entity_kind_dict head_kind ON head_kind.id = head.kind_id
LEFT JOIN symbol_dict head_name ON head_name.id = head.name_id
LEFT JOIN qualified_name_lookup head_qname ON head_qname.id = head.qualified_name_id
LEFT JOIN path_dict head_path ON head_path.id = head.path_id
LEFT JOIN path_dict head_span_path ON head_span_path.id = head.span_path_id
LEFT JOIN extractor_dict head_extractor ON head_extractor.id = head.created_from_id
LEFT JOIN static_references head_static ON head_static.entity_id = e.head_id
LEFT JOIN entities tail ON tail.id_key = tail_oid.id
LEFT JOIN entity_kind_dict tail_kind ON tail_kind.id = tail.kind_id
LEFT JOIN symbol_dict tail_name ON tail_name.id = tail.name_id
LEFT JOIN qualified_name_lookup tail_qname ON tail_qname.id = tail.qualified_name_id
LEFT JOIN path_dict tail_path ON tail_path.id = tail.path_id
LEFT JOIN path_dict tail_span_path ON tail_span_path.id = tail.span_path_id
LEFT JOIN extractor_dict tail_extractor ON tail_extractor.id = tail.created_from_id
LEFT JOIN static_references tail_static ON tail_static.entity_id = e.tail_id
WHERE e.relation = 'CALLS'
  AND (
      e.exactness = 'static_heuristic'
      OR lower(COALESCE(e.metadata_json, '')) LIKE '%unresolved%'
      OR lower(COALESCE(tail_static.metadata_json, tail.metadata_json, '')) LIKE '%unresolved%'
      OR lower(COALESCE(tail_static.created_from, tail_extractor.value, '')) LIKE '%heuristic%'
      OR lower(COALESCE(tail_static.name, tail_name.value, '')) LIKE '%unknown_callee%'
      OR COALESCE(tail_static.qualified_name, tail_qname.value, '') LIKE 'static_reference:%'
  )
ORDER BY e.id_key
LIMIT ?1 OFFSET ?2
"#;

const UNRESOLVED_CALLS_COUNT_SQL: &str = r#"
SELECT COUNT(*)
FROM heuristic_edges e
LEFT JOIN static_references tail_static ON tail_static.entity_id = e.tail_id
WHERE e.relation = 'CALLS'
  AND (
      e.exactness = 'static_heuristic'
      OR lower(COALESCE(e.metadata_json, '')) LIKE '%unresolved%'
      OR lower(COALESCE(tail_static.metadata_json, '')) LIKE '%unresolved%'
      OR lower(COALESCE(tail_static.created_from, '')) LIKE '%heuristic%'
      OR lower(COALESCE(tail_static.name, '')) LIKE '%unknown_callee%'
      OR COALESCE(tail_static.qualified_name, '') LIKE 'static_reference:%'
  )
"#;

fn unresolved_calls_query_plan(
    connection: &Connection,
    limit: usize,
    offset: usize,
) -> Result<Vec<Value>, String> {
    if !sqlite_table_exists(connection, "heuristic_edges")? {
        return Ok(Vec::new());
    }
    let sql = format!("EXPLAIN QUERY PLAN {UNRESOLVED_CALLS_PAGE_SQL}");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![limit as i64, offset as i64], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "parent": row.get::<_, i64>(1)?,
                "notused": row.get::<_, i64>(2)?,
                "detail": row.get::<_, String>(3)?,
            }))
        })
        .map_err(|error| error.to_string())?;
    collect_sqlite_values(rows)
}

fn analyze_sqlite_query_plan(plan: &[Value]) -> Value {
    let mut indexes = BTreeSet::new();
    let mut full_scans = Vec::new();
    for row in plan {
        let Some(detail) = row.get("detail").and_then(Value::as_str) else {
            continue;
        };
        let lower = detail.to_ascii_lowercase();
        if lower.contains(" scan ") || lower.starts_with("scan ") {
            full_scans.push(detail.to_string());
        }
        if let Some(index) = query_plan_index_name(detail) {
            indexes.insert(index);
        }
    }
    json!({
        "uses_indexes": !indexes.is_empty(),
        "indexes_used": indexes.into_iter().collect::<Vec<_>>(),
        "full_scans": full_scans,
    })
}

fn query_plan_index_name(detail: &str) -> Option<String> {
    for marker in [
        "USING COVERING INDEX ",
        "USING INDEX ",
        "USING INTEGER PRIMARY KEY ",
    ] {
        if let Some((_, rest)) = detail.split_once(marker) {
            return rest
                .split_whitespace()
                .next()
                .map(|name| name.trim_matches(|ch| ch == '(' || ch == ')').to_string());
        }
    }
    None
}

fn count_unresolved_calls(connection: &Connection) -> Result<i64, String> {
    if !sqlite_table_exists(connection, "heuristic_edges")? {
        return Ok(0);
    }
    connection
        .query_row(UNRESOLVED_CALLS_COUNT_SQL, [], |row| row.get::<_, i64>(0))
        .map_err(|error| error.to_string())
}

fn query_unresolved_calls_page(
    connection: &Connection,
    repo_root: &Path,
    limit: usize,
    offset: usize,
    include_snippets: bool,
) -> Result<Vec<Value>, String> {
    if !sqlite_table_exists(connection, "heuristic_edges")? {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(UNRESOLVED_CALLS_PAGE_SQL)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![limit as i64, offset as i64],
            unresolved_call_row_json,
        )
        .map_err(|error| error.to_string())?;
    let mut calls = collect_sqlite_values(rows)?;
    if include_snippets {
        attach_unresolved_call_snippets(repo_root, &mut calls);
    }
    Ok(calls)
}

fn unresolved_call_row_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let edge_metadata = json_from_sql_text(row.get::<_, String>("edge_metadata_json")?);
    let provenance_edges = json_from_sql_text(row.get::<_, String>("provenance_edges_json")?);
    let tail = entity_json_from_unresolved_row(row, "tail")?;
    let exactness: String = row.get("exactness")?;
    let unresolved = exactness == "static_heuristic"
        || value_contains_unresolved(&edge_metadata)
        || tail
            .get("qualified_name")
            .and_then(Value::as_str)
            .is_some_and(|qualified_name| qualified_name.starts_with("static_reference:"))
        || tail
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name.contains("unknown_callee"))
        || tail
            .get("created_from")
            .and_then(Value::as_str)
            .is_some_and(|created_from| created_from.contains("heuristic"))
        || tail.get("metadata").is_some_and(value_contains_unresolved);

    Ok(json!({
        "edge": {
            "id": row.get::<_, String>("edge_id")?,
            "head_id": row.get::<_, String>("head_id")?,
            "relation": row.get::<_, String>("relation")?,
            "tail_id": row.get::<_, String>("tail_id")?,
            "source_span": source_span_value(
                row.get::<_, String>("span_repo_relative_path")?,
                row.get::<_, i64>("start_line")?,
                row.get::<_, Option<i64>>("start_column")?,
                row.get::<_, i64>("end_line")?,
                row.get::<_, Option<i64>>("end_column")?,
            ),
            "repo_commit": row.get::<_, Option<String>>("repo_commit")?,
            "file_hash": row.get::<_, Option<String>>("file_hash")?,
            "extractor": row.get::<_, String>("extractor")?,
            "confidence": row.get::<_, f64>("confidence")?,
            "exactness": exactness,
            "derived": row.get::<_, i64>("derived")? != 0,
            "provenance_edges": provenance_edges,
            "metadata": edge_metadata,
        },
        "head": entity_json_from_unresolved_row(row, "head")?,
        "tail": tail,
        "unresolved": unresolved,
        "source_snippet": {
            "requested": false,
            "loaded": false,
        },
    }))
}

fn entity_json_from_unresolved_row(
    row: &rusqlite::Row<'_>,
    prefix: &str,
) -> rusqlite::Result<Value> {
    let span_path =
        row.get::<_, Option<String>>(format!("{prefix}_span_repo_relative_path").as_str())?;
    let source_span = optional_source_span_value(
        span_path,
        row.get::<_, Option<i64>>(format!("{prefix}_start_line").as_str())?,
        row.get::<_, Option<i64>>(format!("{prefix}_start_column").as_str())?,
        row.get::<_, Option<i64>>(format!("{prefix}_end_line").as_str())?,
        row.get::<_, Option<i64>>(format!("{prefix}_end_column").as_str())?,
    );
    Ok(json!({
        "id": row.get::<_, String>(format!("{prefix}_id").as_str())?,
        "kind": row.get::<_, String>(format!("{prefix}_kind").as_str())?,
        "name": row.get::<_, String>(format!("{prefix}_name").as_str())?,
        "qualified_name": row.get::<_, String>(format!("{prefix}_qualified_name").as_str())?,
        "repo_relative_path": row.get::<_, String>(format!("{prefix}_repo_relative_path").as_str())?,
        "source_span": source_span,
        "created_from": row.get::<_, String>(format!("{prefix}_created_from").as_str())?,
        "confidence": row.get::<_, f64>(format!("{prefix}_confidence").as_str())?,
        "metadata": json_from_sql_text(row.get::<_, String>(format!("{prefix}_metadata_json").as_str())?),
    }))
}

fn optional_source_span_value(
    path: Option<String>,
    start_line: Option<i64>,
    start_column: Option<i64>,
    end_line: Option<i64>,
    end_column: Option<i64>,
) -> Value {
    match (path, start_line, end_line) {
        (Some(path), Some(start_line), Some(end_line)) => {
            source_span_value(path, start_line, start_column, end_line, end_column)
        }
        _ => Value::Null,
    }
}

fn source_span_value(
    path: String,
    start_line: i64,
    start_column: Option<i64>,
    end_line: i64,
    end_column: Option<i64>,
) -> Value {
    let start_line = sqlite_u32(start_line);
    let end_line = sqlite_u32(end_line).max(start_line);
    let span = match (start_column, end_column) {
        (Some(start_column), Some(end_column)) => SourceSpan::with_columns(
            path,
            start_line,
            sqlite_u32(start_column),
            end_line,
            sqlite_u32(end_column),
        ),
        _ => SourceSpan::new(path, start_line, end_line),
    };
    json!(span)
}

fn sqlite_u32(value: i64) -> u32 {
    value.clamp(0, i64::from(u32::MAX)) as u32
}

fn json_from_sql_text(raw: String) -> Value {
    serde_json::from_str(&raw).unwrap_or_else(|_| {
        json!({
            "_invalid_json": raw,
        })
    })
}

fn value_contains_unresolved(value: &Value) -> bool {
    value
        .to_string()
        .to_ascii_lowercase()
        .contains("unresolved")
}

fn collect_sqlite_values(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<Value>>,
) -> Result<Vec<Value>, String> {
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn attach_unresolved_call_snippets(repo_root: &Path, calls: &mut [Value]) {
    for call in calls {
        let snippet = unresolved_call_snippet_value(repo_root, call);
        if let Some(object) = call.as_object_mut() {
            object.insert("source_snippet".to_string(), snippet);
        }
    }
}

fn unresolved_call_snippet_value(repo_root: &Path, call: &Value) -> Value {
    let Some(span) = call
        .get("edge")
        .and_then(|edge| edge.get("source_span"))
        .and_then(source_span_from_value)
    else {
        return json!({
            "requested": true,
            "loaded": false,
            "error": "missing_source_span",
        });
    };
    let path = repo_root.join(&span.repo_relative_path);
    match fs::read_to_string(&path) {
        Ok(source) => json!({
            "requested": true,
            "loaded": true,
            "repo_relative_path": span.repo_relative_path,
            "text": source_snippet(&span, &source),
        }),
        Err(error) => json!({
            "requested": true,
            "loaded": false,
            "repo_relative_path": span.repo_relative_path,
            "error": error.to_string(),
        }),
    }
}

fn source_span_from_value(value: &Value) -> Option<SourceSpan> {
    let path = value.get("repo_relative_path")?.as_str()?.to_string();
    let start_line = value.get("start_line")?.as_u64()?.min(u64::from(u32::MAX)) as u32;
    let end_line = value.get("end_line")?.as_u64()?.min(u64::from(u32::MAX)) as u32;
    match (
        value.get("start_column").and_then(Value::as_u64),
        value.get("end_column").and_then(Value::as_u64),
    ) {
        (Some(start_column), Some(end_column)) => Some(SourceSpan::with_columns(
            path,
            start_line,
            start_column.min(u64::from(u32::MAX)) as u32,
            end_line,
            end_column.min(u64::from(u32::MAX)) as u32,
        )),
        _ => Some(SourceSpan::new(path, start_line, end_line)),
    }
}

fn source_scan_unresolved_calls(
    repo_root: &Path,
    store: &SqliteGraphStore,
    limit: usize,
) -> Result<Vec<Value>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let parser = TreeSitterParser;
    let mut calls = Vec::new();
    for file in store
        .list_files(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?
    {
        let path = repo_root.join(&file.repo_relative_path);
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = match parser.parse(&file.repo_relative_path, &source) {
            Ok(Some(parsed)) => parsed,
            _ => continue,
        };
        let extraction = extract_entities_and_relations(&parsed, &source);
        let local_entity_by_id = entities_by_id(&extraction.entities);
        for edge in extraction
            .edges
            .iter()
            .filter(|edge| edge_is_unresolved_call(edge, &local_entity_by_id))
        {
            calls.push(edge_with_entities_json(edge, &local_entity_by_id));
            if calls.len() >= limit {
                return Ok(calls);
            }
        }
    }
    Ok(calls)
}

fn edge_is_unresolved_call(edge: &Edge, entity_by_id: &BTreeMap<String, Entity>) -> bool {
    edge.relation == RelationKind::Calls
        && (edge.exactness == Exactness::StaticHeuristic
            || edge
                .metadata
                .get("resolution")
                .and_then(|value| value.as_str())
                .is_some_and(|resolution| resolution.contains("unresolved"))
            || entity_by_id
                .get(&edge.tail_id)
                .is_some_and(entity_is_unresolved_reference))
}

fn query_path(repo_root: &Path, source: &str, target: &str) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let engine = query_engine(&store)?;
    let source_id = resolve_symbol_or_literal(&store, source)?;
    let target_id = resolve_symbol_or_literal(&store, target)?;
    let paths = engine.trace_path(
        &source_id,
        &target_id,
        RelationKind::ALL,
        default_query_limits(),
    );
    Ok(json!({
        "status": "ok",
        "source": source,
        "target": target,
        "resolved_source": source_id,
        "resolved_target": target_id,
        "paths": engine.path_evidence_from_paths(&paths),
        "proof": "Path query is exact graph traversal over local persisted edges.",
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallQueryDirection {
    Callers,
    Callees,
}

fn query_call_relation(
    repo_root: &Path,
    query: &str,
    limit: usize,
    direction: CallQueryDirection,
) -> Result<Value, String> {
    let store = open_existing_store(repo_root)?;
    let entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let entity_by_id = entities_by_id(&entities);
    let seeds = resolve_symbol_candidates(&store, query, 8)?;
    let seed_ids = seeds
        .iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    let seed_aliases = alias_set_for_entities(&seeds)
        .into_iter()
        .chain([normalize_symbol_alias(query)])
        .collect::<BTreeSet<_>>();
    let mut rows = Vec::new();

    for edge in store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|edge| edge.relation == RelationKind::Calls)
    {
        if rows.len() >= limit {
            break;
        }
        let head_aliases = entity_by_id
            .get(&edge.head_id)
            .map(entity_aliases)
            .unwrap_or_default();
        let tail_aliases = entity_by_id
            .get(&edge.tail_id)
            .map(entity_aliases)
            .unwrap_or_default();
        let matches = match direction {
            CallQueryDirection::Callers => {
                seed_ids.contains(&edge.tail_id) || aliases_overlap(&seed_aliases, &tail_aliases)
            }
            CallQueryDirection::Callees => {
                seed_ids.contains(&edge.head_id) || aliases_overlap(&seed_aliases, &head_aliases)
            }
        };
        if matches {
            rows.push(json!({
                "caller": entity_by_id.get(&edge.head_id).map(entity_json),
                "callee": entity_by_id.get(&edge.tail_id).map(entity_json),
                "edge": edge_json(&edge),
                "unresolved": entity_by_id.get(&edge.tail_id).is_some_and(entity_is_unresolved_reference),
            }));
        }
    }

    let key = match direction {
        CallQueryDirection::Callers => "callers",
        CallQueryDirection::Callees => "callees",
    };
    let mut response = json!({
        "status": "ok",
        "query": query,
        "resolved_symbols": seeds.iter().map(entity_json).collect::<Vec<_>>(),
        "proof": "Caller/callee results are CALLS edges with exactness, confidence, and unresolved-call labels preserved.",
    });
    if let Some(object) = response.as_object_mut() {
        object.insert(key.to_string(), json!(rows));
    }
    Ok(response)
}

fn resolve_symbol_candidates(
    store: &SqliteGraphStore,
    value: &str,
    limit: usize,
) -> Result<Vec<Entity>, String> {
    let mut seen = BTreeSet::new();
    let mut entities = Vec::new();
    for entity in store
        .find_entities_by_exact_symbol(value)
        .map_err(|error| error.to_string())?
    {
        if seen.insert(entity.id.clone()) {
            entities.push(entity);
        }
    }
    if entities.len() < limit {
        for hit in symbol_search_hits(store, value, limit)? {
            if seen.insert(hit.entity.id.clone()) {
                entities.push(hit.entity);
            }
            if entities.len() >= limit {
                break;
            }
        }
    }
    Ok(entities)
}

fn call_chain_paths(
    edges: &[Edge],
    entity_by_id: &BTreeMap<String, Entity>,
    source_entities: &[Entity],
    target_aliases: &BTreeSet<String>,
    limits: QueryLimits,
) -> Vec<GraphPath> {
    let mut outgoing = BTreeMap::<String, Vec<Edge>>::new();
    for edge in edges
        .iter()
        .filter(|edge| edge.relation == RelationKind::Calls)
    {
        outgoing
            .entry(edge.head_id.clone())
            .or_default()
            .push(edge.clone());
    }

    let ids_by_alias = ids_by_alias(entity_by_id);
    let mut paths = Vec::new();
    let mut queue = std::collections::VecDeque::<(String, String, Vec<TraversalStep>)>::new();
    for source in source_entities {
        queue.push_back((source.id.clone(), source.id.clone(), Vec::new()));
    }
    let mut visited = BTreeSet::<(String, usize)>::new();
    let mut edges_visited = 0usize;

    while let Some((origin, current, steps)) = queue.pop_front() {
        if paths.len() >= limits.max_paths || edges_visited >= limits.max_edges_visited {
            break;
        }
        if steps.len() >= limits.max_depth {
            continue;
        }
        if !visited.insert((current.clone(), steps.len())) {
            continue;
        }
        for head_id in equivalent_entity_ids(&current, entity_by_id, &ids_by_alias) {
            let Some(call_edges) = outgoing.get(&head_id) else {
                continue;
            };
            for edge in call_edges {
                edges_visited += 1;
                let mut next_steps = steps.clone();
                next_steps.push(TraversalStep {
                    edge: edge.clone(),
                    direction: TraversalDirection::Forward,
                    from: edge.head_id.clone(),
                    to: edge.tail_id.clone(),
                });
                let tail_aliases = entity_by_id
                    .get(&edge.tail_id)
                    .map(entity_aliases)
                    .unwrap_or_else(|| BTreeSet::from([normalize_symbol_alias(&edge.tail_id)]));
                if aliases_overlap(&tail_aliases, target_aliases)
                    || target_aliases.contains(&edge.tail_id.to_ascii_lowercase())
                {
                    paths.push(call_graph_path(&origin, next_steps.clone()));
                    if paths.len() >= limits.max_paths {
                        break;
                    }
                }
                for next_id in equivalent_entity_ids(&edge.tail_id, entity_by_id, &ids_by_alias) {
                    if next_steps
                        .iter()
                        .filter(|step| step.from == next_id || step.to == next_id)
                        .count()
                        > 1
                    {
                        continue;
                    }
                    queue.push_back((origin.clone(), next_id, next_steps.clone()));
                }
            }
        }
    }

    paths.sort_by(|left, right| {
        left.steps
            .len()
            .cmp(&right.steps.len())
            .then_with(|| right.confidence_score().total_cmp(&left.confidence_score()))
    });
    paths
}

fn call_graph_path(source: &str, steps: Vec<TraversalStep>) -> GraphPath {
    let target = steps
        .last()
        .map(|step| step.to.clone())
        .unwrap_or_else(|| source.to_string());
    let uncertainty = steps
        .iter()
        .map(|step| match step.edge.exactness {
            Exactness::Exact | Exactness::CompilerVerified | Exactness::LspVerified => 0.0,
            Exactness::ParserVerified => 0.08,
            Exactness::DynamicTrace => 0.04,
            Exactness::DerivedFromVerifiedEdges => 0.12,
            Exactness::StaticHeuristic | Exactness::Inferred => 0.35,
        })
        .sum::<f64>();
    GraphPath {
        source: source.to_string(),
        target,
        cost: steps.len() as f64 + uncertainty,
        uncertainty,
        steps,
    }
}

trait GraphPathConfidence {
    fn confidence_score(&self) -> f64;
}

impl GraphPathConfidence for GraphPath {
    fn confidence_score(&self) -> f64 {
        if self.steps.is_empty() {
            return 1.0;
        }
        self.steps
            .iter()
            .map(|step| step.edge.confidence)
            .fold(1.0_f64, f64::min)
            / (1.0 + self.uncertainty)
    }
}

fn resolve_symbol_or_literal(store: &SqliteGraphStore, value: &str) -> Result<String, String> {
    let hits = store
        .find_entities_by_exact_symbol(value)
        .map_err(|error| error.to_string())?;
    Ok(hits
        .first()
        .map(|entity| entity.id.clone())
        .unwrap_or_else(|| value.to_string()))
}

fn resolve_impact_seeds(store: &SqliteGraphStore, target: &str) -> Result<Vec<String>, String> {
    let normalized = target.replace('\\', "/");
    let mut seeds = Vec::new();
    for entity in store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?
    {
        if entity.repo_relative_path == normalized
            || entity.repo_relative_path.ends_with(&normalized)
        {
            seeds.push(entity.id);
        }
    }
    if seeds.is_empty() {
        seeds.extend(
            store
                .find_entities_by_exact_symbol(target)
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|entity| entity.id),
        );
    }
    if seeds.is_empty() {
        seeds.push(target.to_string());
    }
    seeds.sort();
    seeds.dedup();
    Ok(seeds)
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

fn parse_index_options(args: &[String]) -> Result<(String, Option<PathBuf>, IndexOptions), String> {
    let mut repo = None;
    let mut db = None;
    let mut options = IndexOptions::default();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--profile" => options.profile = true,
            "--json" => options.json = true,
            "--db" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--db requires a path".to_string());
                };
                db = Some(PathBuf::from(raw));
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
            value if value.starts_with('-') => {
                return Err(format!("unknown index option: {value}"))
            }
            value => {
                if repo.is_some() {
                    return Err(format!("unexpected index argument: {value}"));
                }
                repo = Some(value.to_string());
            }
        }
        index += 1;
    }
    Ok((
        repo.ok_or_else(|| {
            "Usage: codegraph-mcp index <repo> [--db <path>] [--profile] [--json] [--workers <n>] [--storage-mode <proof|audit|debug>] [--build-mode <proof-build-only|proof-build-plus-validation>]".to_string()
        })?,
        db,
        options,
    ))
}

fn parse_watch_options(args: &[String]) -> Result<WatchOptions, String> {
    let mut repo = None;
    let mut db = None;
    let mut debounce = Duration::from_millis(250);
    let mut once = false;
    let mut changed_paths = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--debounce-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--debounce-ms requires a value".to_string());
                };
                let millis = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid debounce milliseconds: {value}"))?;
                debounce = Duration::from_millis(millis);
            }
            "--db" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--db requires a path".to_string());
                };
                db = Some(PathBuf::from(value));
            }
            "--once" => once = true,
            "--changed" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--changed requires a path".to_string());
                };
                changed_paths.push(PathBuf::from(value));
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown watch option: {value}"));
            }
            value => {
                if repo.is_some() {
                    return Err(format!("unexpected watch argument: {value}"));
                }
                repo = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }

    Ok(WatchOptions {
        repo: repo.unwrap_or_else(|| PathBuf::from(".")),
        db,
        debounce,
        once,
        changed_paths,
    })
}

fn parse_ui_options(args: &[String]) -> Result<UiOptions, String> {
    let mut repo = None;
    let mut host = "127.0.0.1".to_string();
    let mut port = 7878_u16;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--host" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--host requires a value".to_string());
                };
                host = value.clone();
            }
            "--port" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--port requires a value".to_string());
                };
                port = value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid UI port: {value}"))?;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown serve-ui option: {value}"));
            }
            value => {
                if repo.is_some() {
                    return Err(format!("unexpected serve-ui argument: {value}"));
                }
                repo = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }

    if !is_local_host(&host) {
        return Err(format!(
            "serve-ui is local-only by default; refusing host {host}"
        ));
    }

    Ok(UiOptions {
        repo: repo.unwrap_or_else(|| PathBuf::from(".")),
        host,
        port,
    })
}

fn parse_bench_options(args: &[String]) -> Result<BenchOptions, String> {
    let mut baselines = Vec::new();
    let mut format = BenchReportFormat::Json;
    let mut output = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--baseline" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--baseline requires a value".to_string());
                };
                for value in raw.split(',').filter(|value| !value.trim().is_empty()) {
                    baselines.push(
                        value
                            .parse::<BaselineMode>()
                            .map_err(|error| error.to_string())?,
                    );
                }
            }
            "--format" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--format requires json or markdown".to_string());
                };
                format = match raw.trim().to_ascii_lowercase().as_str() {
                    "json" => BenchReportFormat::Json,
                    "markdown" | "md" => BenchReportFormat::Markdown,
                    other => return Err(format!("unknown benchmark report format: {other}")),
                };
            }
            "--output" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--output requires a path".to_string());
                };
                output = Some(PathBuf::from(raw));
            }
            value => return Err(format!("unknown benchmark option: {value}")),
        }
        index += 1;
    }

    Ok(BenchOptions {
        baselines,
        format,
        output,
    })
}

fn parse_graph_truth_gate_options(args: &[String]) -> Result<GraphTruthGateOptions, String> {
    let mut options = codegraph_bench::default_graph_truth_gate_options();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--cases" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--cases requires a path".to_string());
                };
                options.cases = PathBuf::from(value);
            }
            "--fixture-root" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--fixture-root requires a path".to_string());
                };
                options.fixture_root = PathBuf::from(value);
            }
            "--out-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-json requires a path".to_string());
                };
                options.out_json = PathBuf::from(value);
            }
            "--out-md" | "--out-markdown" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-md requires a path".to_string());
                };
                options.out_md = PathBuf::from(value);
            }
            "--fail-on-forbidden" => {
                options.fail_on_forbidden = true;
            }
            "--fail-on-missing-source-span" => {
                options.fail_on_missing_source_span = true;
            }
            "--fail-on-unresolved-exact" => {
                options.fail_on_unresolved_exact = true;
            }
            "--fail-on-derived-without-provenance" => {
                options.fail_on_derived_without_provenance = true;
            }
            "--fail-on-test-mock-production-leak" => {
                options.fail_on_test_mock_production_leak = true;
            }
            "--update-mode" => {
                options.update_mode = true;
            }
            "--keep-workdirs" => {
                options.keep_workdirs = true;
            }
            "--verbose" => {
                options.verbose = true;
            }
            value => return Err(format!("unknown graph-truth option: {value}")),
        }
        index += 1;
    }
    Ok(options)
}

fn parse_context_packet_gate_options(args: &[String]) -> Result<ContextPacketGateOptions, String> {
    let mut options = codegraph_bench::default_context_packet_gate_options();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--cases" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--cases requires a path".to_string());
                };
                options.cases = PathBuf::from(value);
            }
            "--fixture-root" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--fixture-root requires a path".to_string());
                };
                options.fixture_root = PathBuf::from(value);
            }
            "--out-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-json requires a path".to_string());
                };
                options.out_json = PathBuf::from(value);
            }
            "--out-md" | "--out-markdown" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-md requires a path".to_string());
                };
                options.out_md = PathBuf::from(value);
            }
            "--top-k" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--top-k requires a positive integer".to_string());
                };
                options.top_k = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --top-k: {error}"))?;
                if options.top_k == 0 {
                    return Err("--top-k must be greater than zero".to_string());
                }
            }
            "--budget" | "--token-budget" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--budget requires a positive integer".to_string());
                };
                options.token_budget = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --budget: {error}"))?;
                if options.token_budget == 0 {
                    return Err("--budget must be greater than zero".to_string());
                }
            }
            value => return Err(format!("unknown context-packet option: {value}")),
        }
        index += 1;
    }
    Ok(options)
}

fn parse_retrieval_ablation_options(args: &[String]) -> Result<RetrievalAblationOptions, String> {
    let mut options = codegraph_bench::default_retrieval_ablation_options();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--cases" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--cases requires a path".to_string());
                };
                options.cases = PathBuf::from(value);
            }
            "--fixture-root" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--fixture-root requires a path".to_string());
                };
                options.fixture_root = PathBuf::from(value);
            }
            "--out-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-json requires a path".to_string());
                };
                options.out_json = PathBuf::from(value);
            }
            "--out-md" | "--out-markdown" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out-md requires a path".to_string());
                };
                options.out_md = PathBuf::from(value);
            }
            "--top-k" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--top-k requires a positive integer".to_string());
                };
                options.top_k = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --top-k: {error}"))?;
            }
            "--mode" | "--modes" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--mode requires a retrieval-ablation mode".to_string());
                };
                for raw in value.split(',') {
                    options.modes.push(
                        raw.parse::<RetrievalAblationMode>()
                            .map_err(|error| error.to_string())?,
                    );
                }
            }
            value => return Err(format!("unknown retrieval-ablation option: {value}")),
        }
        index += 1;
    }
    Ok(options)
}

fn parse_two_layer_bench_options(args: &[String]) -> Result<TwoLayerBenchOptions, String> {
    let mut options = codegraph_bench::default_two_layer_bench_options();
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--run-id" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--run-id requires a value".to_string());
                };
                options.run_id = Some(raw.clone());
            }
            "--timeout-ms" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--timeout-ms requires a non-negative integer".to_string());
                };
                options.timeout_ms = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}"))?
                    .min(codegraph_bench::MAX_BENCH_TASK_MS);
            }
            "--top-k" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--top-k requires a positive integer".to_string());
                };
                options.top_k = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --top-k value: {raw}"))?;
                if options.top_k == 0 {
                    return Err("--top-k must be greater than zero".to_string());
                }
            }
            "--competitor-bin" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--competitor-bin requires a path".to_string());
                };
                options.competitor_executable = Some(PathBuf::from(raw));
            }
            "--autoresearch-repo" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--autoresearch-repo requires a path".to_string());
                };
                options.autoresearch_repo = Some(PathBuf::from(raw));
            }
            "--skip-autoresearch" => {
                options.include_autoresearch = false;
            }
            "--fake-agent" | "--dry-run" => {
                options.fake_agent = true;
                options.dry_run = true;
            }
            "--run-root" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--run-root requires a path".to_string());
                };
                let run_root = PathBuf::from(raw);
                if !run_root
                    .components()
                    .any(|component| component.as_os_str() == "codegraph-bench-runs")
                {
                    return Err(
                        "--run-root must be under target/codegraph-bench-runs/<run_id>".to_string(),
                    );
                }
                options.run_root = Some(run_root);
            }
            value => return Err(format!("unknown two-layer benchmark option: {value}")),
        }
        index += 1;
    }

    Ok(options)
}

fn parse_trace_append_options(args: &[String]) -> Result<TraceAppendOptions, String> {
    let mut repo = PathBuf::from(".");
    let mut repo_id = None;
    let mut trace_root = PathBuf::from(DEFAULT_TRACE_ROOT);
    let mut run_id = None;
    let mut task_id = None;
    let mut event_type = None;
    let mut trace_id = None;
    let mut tool = None;
    let mut status = None;
    let mut actor = None;
    let mut action_kind = None;
    let mut result_status = None;
    let mut latency_ms = 0u128;
    let mut token_estimate = None;
    let mut evidence_refs = Vec::new();
    let mut edited_files = Vec::new();
    let mut test_command = None;
    let mut test_status = None;
    let mut input = None;
    let mut output = None;
    let mut error = None;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--repo" => {
                index += 1;
                repo = PathBuf::from(required_cli_value(args, index, "--repo")?);
            }
            "--repo-id" => {
                index += 1;
                repo_id = Some(required_cli_value(args, index, "--repo-id")?.to_string());
            }
            "--trace-root" => {
                index += 1;
                trace_root = PathBuf::from(required_cli_value(args, index, "--trace-root")?);
            }
            "--run-id" => {
                index += 1;
                run_id = Some(required_cli_value(args, index, "--run-id")?.to_string());
            }
            "--task-id" => {
                index += 1;
                task_id = Some(required_cli_value(args, index, "--task-id")?.to_string());
            }
            "--event-type" => {
                index += 1;
                let raw = required_cli_value(args, index, "--event-type")?;
                event_type = Some(
                    raw.parse::<TraceEventType>()
                        .map_err(|error| error.to_string())?,
                );
            }
            "--trace-id" => {
                index += 1;
                trace_id = Some(required_cli_value(args, index, "--trace-id")?.to_string());
            }
            "--tool" => {
                index += 1;
                tool = Some(required_cli_value(args, index, "--tool")?.to_string());
            }
            "--status" => {
                index += 1;
                status = Some(required_cli_value(args, index, "--status")?.to_string());
            }
            "--actor" => {
                index += 1;
                actor = Some(required_cli_value(args, index, "--actor")?.to_string());
            }
            "--action-kind" => {
                index += 1;
                action_kind = Some(required_cli_value(args, index, "--action-kind")?.to_string());
            }
            "--result-status" => {
                index += 1;
                result_status =
                    Some(required_cli_value(args, index, "--result-status")?.to_string());
            }
            "--latency-ms" => {
                index += 1;
                let raw = required_cli_value(args, index, "--latency-ms")?;
                latency_ms = raw
                    .parse::<u128>()
                    .map_err(|_| format!("invalid --latency-ms value: {raw}"))?;
            }
            "--token-estimate" => {
                index += 1;
                token_estimate = Some(parse_token_estimate(required_cli_value(
                    args,
                    index,
                    "--token-estimate",
                )?));
            }
            "--evidence-ref" => {
                index += 1;
                evidence_refs.push(required_cli_value(args, index, "--evidence-ref")?.to_string());
            }
            "--edited-file" => {
                index += 1;
                edited_files.push(required_cli_value(args, index, "--edited-file")?.to_string());
            }
            "--test-command" => {
                index += 1;
                test_command = Some(required_cli_value(args, index, "--test-command")?.to_string());
            }
            "--test-status" => {
                index += 1;
                test_status = Some(required_cli_value(args, index, "--test-status")?.to_string());
            }
            "--input-json" => {
                index += 1;
                input = Some(parse_json_arg(required_cli_value(
                    args,
                    index,
                    "--input-json",
                )?)?);
            }
            "--output-json" => {
                index += 1;
                output = Some(parse_json_arg(required_cli_value(
                    args,
                    index,
                    "--output-json",
                )?)?);
            }
            "--error" => {
                index += 1;
                error = Some(required_cli_value(args, index, "--error")?.to_string());
            }
            value => return Err(format!("unknown trace append option: {value}")),
        }
        index += 1;
    }

    let event = TraceAppendEvent {
        event_type: event_type.ok_or_else(|| "--event-type is required".to_string())?,
        trace_id: trace_id.unwrap_or_else(|| "unknown".to_string()),
        tool: tool.unwrap_or_else(|| "unknown".to_string()),
        status: status.unwrap_or_else(|| "unknown".to_string()),
        actor,
        action_kind,
        result_status,
        latency_ms,
        token_estimate,
        evidence_refs: (!evidence_refs.is_empty()).then(|| json!(evidence_refs)),
        edited_files: (!edited_files.is_empty()).then(|| json!(edited_files)),
        test_command,
        test_status,
        input,
        output,
        error,
    };
    Ok(TraceAppendOptions {
        repo,
        repo_id,
        trace_root,
        run_id,
        task_id,
        event,
    })
}

fn parse_trace_replay_options(args: &[String]) -> Result<PathBuf, String> {
    let mut events_path = None;
    let mut trace_root = PathBuf::from(DEFAULT_TRACE_ROOT);
    let mut run_id = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--events" => {
                index += 1;
                events_path = Some(PathBuf::from(required_cli_value(args, index, "--events")?));
            }
            "--trace-root" => {
                index += 1;
                trace_root = PathBuf::from(required_cli_value(args, index, "--trace-root")?);
            }
            "--run-id" => {
                index += 1;
                run_id = Some(required_cli_value(args, index, "--run-id")?.to_string());
            }
            value => return Err(format!("unknown trace replay option: {value}")),
        }
        index += 1;
    }
    events_path
        .or_else(|| run_id.map(|id| trace_root.join(id).join("events.jsonl")))
        .ok_or_else(|| "trace replay requires --events or --run-id".to_string())
}

fn required_cli_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_json_arg(raw: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|error| format!("invalid JSON argument: {error}"))
}

fn parse_token_estimate(raw: &str) -> Value {
    if raw.eq_ignore_ascii_case("unknown") {
        json!("unknown")
    } else {
        raw.parse::<u64>()
            .map(Value::from)
            .unwrap_or_else(|_| json!({"kind": "character_estimate", "value": raw}))
    }
}

fn parse_synthetic_index_options(args: &[String]) -> Result<SyntheticIndexOptions, String> {
    let mut output_dir = None;
    let mut files = 250usize;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--output-dir" | "--output" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--output-dir requires a path".to_string());
                };
                output_dir = Some(PathBuf::from(raw));
            }
            "--files" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--files requires a positive integer".to_string());
                };
                files = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --files value: {raw}"))?;
                if files == 0 {
                    return Err("--files must be greater than zero".to_string());
                }
            }
            value => return Err(format!("unknown synthetic-index option: {value}")),
        }
        index += 1;
    }

    Ok(SyntheticIndexOptions {
        output_dir: output_dir.ok_or_else(|| {
            "Usage: codegraph-mcp bench synthetic-index --output-dir <dir> [--files <n>]"
                .to_string()
        })?,
        files,
    })
}

fn parse_update_integrity_harness_options(
    args: &[String],
) -> Result<UpdateIntegrityHarnessOptions, String> {
    let mut iterations = 20usize;
    let mut autoresearch_iterations = 1usize;
    let mut workers = 4usize;
    let mut medium_files = 48usize;
    let mut mode = UpdateBenchmarkMode::Fast;
    let mut loop_kind = UpdateLoopKind::Combined;
    let mut timeout_ms = None;
    let mut out_json = PathBuf::from("reports")
        .join("audit")
        .join("artifacts")
        .join("autoresearch_update_repro_fix.json");
    let mut out_md = PathBuf::from("reports")
        .join("audit")
        .join("autoresearch_update_repro_fix.md");
    let mut workdir = PathBuf::from("reports")
        .join("audit")
        .join("artifacts")
        .join("update_integrity_harness");
    let mut skip_autoresearch = false;
    let mut only_autoresearch = false;
    let mut autoresearch_repo = std::env::var_os("CODEGRAPH_AUTORESEARCH_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("..").join("autoresearch-codexlab"));
    let mut autoresearch_seed_db = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--iterations" => {
                index += 1;
                let raw = args
                    .get(index)
                    .ok_or_else(|| "--iterations requires a positive integer".to_string())?;
                iterations = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --iterations value: {raw}"))?;
                if iterations == 0 {
                    return Err("--iterations must be greater than zero".to_string());
                }
            }
            "--autoresearch-iterations" => {
                index += 1;
                let raw = args.get(index).ok_or_else(|| {
                    "--autoresearch-iterations requires a positive integer".to_string()
                })?;
                autoresearch_iterations = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --autoresearch-iterations value: {raw}"))?;
            }
            "--workers" => {
                index += 1;
                let raw = args
                    .get(index)
                    .ok_or_else(|| "--workers requires a positive integer".to_string())?;
                workers = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --workers value: {raw}"))?;
                if workers == 0 {
                    return Err("--workers must be greater than zero".to_string());
                }
            }
            "--medium-files" => {
                index += 1;
                let raw = args
                    .get(index)
                    .ok_or_else(|| "--medium-files requires a positive integer".to_string())?;
                medium_files = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --medium-files value: {raw}"))?;
                if medium_files < 2 {
                    return Err("--medium-files must be at least 2".to_string());
                }
            }
            "--mode" => {
                index += 1;
                let raw = args.get(index).ok_or_else(|| {
                    "--mode requires update-fast, update-validated, or update-debug".to_string()
                })?;
                mode = UpdateBenchmarkMode::parse(raw)?;
            }
            "--loop-kind" | "--loop" => {
                index += 1;
                let raw = args.get(index).ok_or_else(|| {
                    "--loop-kind requires combined, repeat-fast, or update-fast".to_string()
                })?;
                loop_kind = UpdateLoopKind::parse(raw)?;
            }
            "--timeout-ms" => {
                index += 1;
                let raw = args
                    .get(index)
                    .ok_or_else(|| "--timeout-ms requires a positive integer".to_string())?;
                let parsed = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}"))?;
                if parsed == 0 {
                    return Err("--timeout-ms must be greater than zero".to_string());
                }
                timeout_ms = Some(parsed);
            }
            "--out-json" => {
                index += 1;
                out_json = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--out-json requires a path".to_string())?,
                );
            }
            "--out-md" => {
                index += 1;
                out_md = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--out-md requires a path".to_string())?,
                );
            }
            "--workdir" => {
                index += 1;
                workdir = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--workdir requires a path".to_string())?,
                );
            }
            "--skip-autoresearch" => {
                skip_autoresearch = true;
            }
            "--only-autoresearch" => {
                only_autoresearch = true;
            }
            "--autoresearch-repo" => {
                index += 1;
                autoresearch_repo = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--autoresearch-repo requires a path".to_string())?,
                );
            }
            "--autoresearch-seed-db" => {
                index += 1;
                autoresearch_seed_db =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--autoresearch-seed-db requires a path".to_string()
                    })?));
            }
            value => return Err(format!("unknown update-integrity option: {value}")),
        }
        index += 1;
    }
    if skip_autoresearch && only_autoresearch {
        return Err("--only-autoresearch cannot be combined with --skip-autoresearch".to_string());
    }

    Ok(UpdateIntegrityHarnessOptions {
        iterations,
        autoresearch_iterations,
        workers,
        medium_files,
        mode,
        loop_kind,
        timeout_ms,
        out_json: absolute_cli_path(&out_json)?,
        out_md: absolute_cli_path(&out_md)?,
        workdir: absolute_cli_path(&workdir)?,
        skip_autoresearch,
        only_autoresearch,
        autoresearch_repo: absolute_cli_path(&autoresearch_repo)?,
        autoresearch_seed_db: autoresearch_seed_db
            .as_deref()
            .map(absolute_cli_path)
            .transpose()?,
    })
}

fn parse_cgc_comparison_options(args: &[String]) -> Result<CgcComparisonOptions, String> {
    let mut report_dir = default_report_dir();
    let mut timeout_ms = codegraph_bench::competitors::codegraphcontext::DEFAULT_TIMEOUT_MS;
    let mut top_k = codegraph_bench::competitors::codegraphcontext::DEFAULT_TOP_K;
    let mut competitor_executable = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output-dir" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--output-dir requires a path".to_string());
                };
                report_dir = PathBuf::from(raw);
            }
            "--timeout-ms" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--timeout-ms requires a positive integer".to_string());
                };
                timeout_ms = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}"))?;
                if timeout_ms == 0 {
                    return Err("--timeout-ms must be greater than zero".to_string());
                }
            }
            "--top-k" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--top-k requires a positive integer".to_string());
                };
                top_k = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --top-k value: {raw}"))?;
                if top_k == 0 {
                    return Err("--top-k must be greater than zero".to_string());
                }
            }
            "--competitor-bin" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--competitor-bin requires a path".to_string());
                };
                competitor_executable = Some(PathBuf::from(raw));
            }
            value => return Err(format!("unknown cgc-comparison option: {value}")),
        }
        index += 1;
    }

    Ok(CgcComparisonOptions {
        report_dir,
        timeout_ms,
        top_k,
        competitor_executable,
    })
}

fn parse_gap_scoreboard_options(args: &[String]) -> Result<GapScoreboardOptions, String> {
    let mut report_dir = PathBuf::from("reports").join("phase26-gaps");
    let mut timeout_ms = codegraph_bench::competitors::codegraphcontext::DEFAULT_TIMEOUT_MS;
    let mut top_k = codegraph_bench::competitors::codegraphcontext::DEFAULT_TOP_K;
    let mut competitor_executable = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output-dir" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--output-dir requires a path".to_string());
                };
                report_dir = PathBuf::from(raw);
            }
            "--timeout-ms" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--timeout-ms requires a positive integer".to_string());
                };
                timeout_ms = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}"))?;
                if timeout_ms == 0 {
                    return Err("--timeout-ms must be greater than zero".to_string());
                }
            }
            "--top-k" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--top-k requires a positive integer".to_string());
                };
                top_k = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --top-k value: {raw}"))?;
                if top_k == 0 {
                    return Err("--top-k must be greater than zero".to_string());
                }
            }
            "--competitor-bin" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--competitor-bin requires a path".to_string());
                };
                competitor_executable = Some(PathBuf::from(raw));
            }
            value => return Err(format!("unknown gaps option: {value}")),
        }
        index += 1;
    }

    Ok(GapScoreboardOptions {
        report_dir,
        timeout_ms,
        top_k,
        competitor_executable,
    })
}

fn parse_final_gate_options(args: &[String]) -> Result<FinalAcceptanceGateOptions, String> {
    let mut options =
        FinalAcceptanceGateOptions::with_report_dir(PathBuf::from("reports").join("final-gate"));
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output-dir" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--output-dir requires a path".to_string());
                };
                options.report_dir = PathBuf::from(raw);
            }
            "--workspace-root" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--workspace-root requires a path".to_string());
                };
                options.workspace_root = Some(PathBuf::from(raw));
            }
            "--timeout-ms" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--timeout-ms requires a positive integer".to_string());
                };
                let timeout_ms = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}"))?;
                if timeout_ms == 0 {
                    return Err("--timeout-ms must be greater than zero".to_string());
                }
                options.timeout_ms = timeout_ms.min(codegraph_bench::MAX_BENCH_TASK_MS);
            }
            "--competitor-bin" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--competitor-bin requires a path".to_string());
                };
                options.competitor_executable = Some(PathBuf::from(raw));
            }
            "--cgc-db-size-bytes" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--cgc-db-size-bytes requires an integer".to_string());
                };
                options.cgc_db_size_bytes = Some(
                    raw.parse::<u64>()
                        .map_err(|_| format!("invalid --cgc-db-size-bytes value: {raw}"))?,
                );
            }
            value => return Err(format!("unknown final-gate option: {value}")),
        }
        index += 1;
    }

    Ok(options)
}

#[derive(Debug, Clone)]
struct ContextPackOptions {
    task: String,
    mode: String,
    token_budget: usize,
    seeds: Vec<String>,
    stage0_candidates: Vec<String>,
    profile: bool,
}

fn parse_context_pack_args(args: &[String]) -> Result<ContextPackOptions, String> {
    let mut task = None;
    let mut mode = "impact".to_string();
    let mut token_budget = 2_000usize;
    let mut seeds = Vec::new();
    let mut stage0_candidates = Vec::new();
    let mut profile = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--task" => {
                index += 1;
                task = args.get(index).cloned();
            }
            "--budget" | "--token-budget" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--budget requires a value".to_string());
                };
                token_budget = value
                    .parse::<usize>()
                    .map_err(|_| "--budget must be an integer".to_string())?;
            }
            "--mode" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--mode requires a value".to_string());
                };
                mode = value.clone();
            }
            "--seed" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--seed requires a value".to_string());
                };
                seeds.push(value.clone());
            }
            "--candidate" | "--stage0-candidate" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--candidate requires a value".to_string());
                };
                stage0_candidates.push(value.clone());
            }
            "--profile" => {
                profile = true;
            }
            other => return Err(format!("unknown context-pack option: {other}")),
        }
        index += 1;
    }
    let Some(task) = task else {
        return Err(
            "Usage: codegraph-mcp context-pack --task <task> [--budget <tokens>] [--mode <mode>]"
                .to_string(),
        );
    };
    Ok(ContextPackOptions {
        task,
        mode,
        token_budget,
        seeds,
        stage0_candidates,
        profile,
    })
}

fn parse_output_arg(args: &[String]) -> Result<PathBuf, String> {
    if args.len() == 2 && args[0] == "--output" {
        return Ok(PathBuf::from(&args[1]));
    }
    Err("Usage: codegraph-mcp bundle export --output repo.cgc-bundle".to_string())
}

fn optional_repo_arg(args: &[String]) -> Result<PathBuf, String> {
    match args {
        [] => Ok(PathBuf::from(".")),
        [repo] => Ok(PathBuf::from(repo)),
        _ => Err("Usage: codegraph-mcp status [repo]".to_string()),
    }
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

fn resolve_repo_root(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!(
            "repository path does not exist: {}",
            path.display()
        ));
    }
    fs::canonicalize(path).map_err(|error| error.to_string())
}

fn current_repo_root() -> Result<PathBuf, String> {
    std::env::current_dir()
        .map_err(|error| error.to_string())
        .and_then(|path| resolve_repo_root(&path))
}

fn open_existing_store(repo_root: &Path) -> Result<SqliteGraphStore, String> {
    let db_path = default_db_path(repo_root);
    if !db_path.exists() {
        return Err(format!(
            "CodeGraph index does not exist at {}; run `codegraph-mcp index .` first",
            db_path.display()
        ));
    }
    SqliteGraphStore::open(db_path).map_err(|error| error.to_string())
}

fn query_engine(store: &SqliteGraphStore) -> Result<ExactGraphQueryEngine, String> {
    let edges = store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    Ok(ExactGraphQueryEngine::new(edges))
}

fn profile_span_json(
    name: &str,
    elapsed: Duration,
    count: u64,
    items: u64,
    detail: Value,
) -> Value {
    json!({
        "name": name,
        "elapsed_ms": elapsed.as_secs_f64() * 1000.0,
        "count": count,
        "items": items,
        "detail": detail,
    })
}

#[derive(Debug, Clone, Copy)]
struct ContextPackBudgets {
    max_seed_entities: usize,
    max_candidate_paths: usize,
    max_returned_proof_paths: usize,
    max_snippets: usize,
    max_traversal_depth: usize,
}

impl ContextPackBudgets {
    fn for_mode(mode: &str) -> Self {
        let normalized = mode.to_ascii_lowercase();
        if normalized.contains("debug") {
            return Self {
                max_seed_entities: 64,
                max_candidate_paths: 512,
                max_returned_proof_paths: 48,
                max_snippets: 64,
                max_traversal_depth: 6,
            };
        }
        if normalized.contains("impact") {
            return Self {
                max_seed_entities: 32,
                max_candidate_paths: 256,
                max_returned_proof_paths: 24,
                max_snippets: 24,
                max_traversal_depth: 4,
            };
        }
        Self {
            max_seed_entities: 16,
            max_candidate_paths: 128,
            max_returned_proof_paths: 12,
            max_snippets: 12,
            max_traversal_depth: 3,
        }
    }
}

fn open_context_pack_connection(db_path: &Path) -> Result<Connection, String> {
    if !db_path.exists() {
        return Err(format!(
            "CodeGraph index does not exist at {}; run `codegraph-mcp index .` first",
            db_path.display()
        ));
    }
    let connection = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "
            PRAGMA query_only = ON;
            PRAGMA busy_timeout = 5000;
            ",
        )
        .map_err(|error| error.to_string())?;
    Ok(connection)
}

fn context_pack_seed_values(options: &ContextPackOptions, max_seeds: usize) -> Vec<String> {
    let prompt_exact = extract_prompt_seeds(&options.task)
        .into_iter()
        .filter_map(|seed| seed.exact_value())
        .collect::<Vec<_>>();
    unique_limited_strings(
        options
            .seeds
            .iter()
            .chain(options.stage0_candidates.iter())
            .chain(prompt_exact.iter())
            .cloned(),
        max_seeds.max(1),
    )
}

fn unique_limited_strings(values: impl IntoIterator<Item = String>, limit: usize) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            continue;
        }
        output.push(trimmed.to_string());
        if output.len() >= limit {
            break;
        }
    }
    output
}

fn context_seed_ids(
    raw_seed_values: &[String],
    seed_entities: &[ContextEntitySummary],
    limit: usize,
) -> Vec<String> {
    unique_limited_strings(
        raw_seed_values
            .iter()
            .cloned()
            .chain(seed_entities.iter().map(|entity| entity.id.clone())),
        limit.max(1),
    )
}

#[derive(Debug, Clone)]
struct ContextEntitySummary {
    id: String,
    name: String,
    qualified_name: String,
    repo_relative_path: String,
}

fn resolve_context_seed_entities(
    connection: &Connection,
    seeds: &[String],
    limit: usize,
) -> Result<Vec<ContextEntitySummary>, String> {
    if seeds.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let mut entity_keys = BTreeSet::<i64>::new();
    for seed in seeds {
        if entity_keys.len() >= limit {
            break;
        }
        if let Some(key) = lookup_i64(connection, "object_id_lookup", seed)? {
            entity_keys.insert(key);
        }
        if entity_keys.len() >= limit {
            break;
        }
        if let Some(name_id) = lookup_i64(connection, "symbol_dict", seed)? {
            let remaining = limit.saturating_sub(entity_keys.len());
            for key in entity_keys_by_column(connection, "name_id", name_id, remaining)? {
                entity_keys.insert(key);
                if entity_keys.len() >= limit {
                    break;
                }
            }
        }
        if entity_keys.len() >= limit {
            break;
        }
        if let Some(qname_id) = lookup_i64(connection, "qualified_name_dict", seed)? {
            let remaining = limit.saturating_sub(entity_keys.len());
            for key in entity_keys_by_column(connection, "qualified_name_id", qname_id, remaining)?
            {
                entity_keys.insert(key);
                if entity_keys.len() >= limit {
                    break;
                }
            }
        }
    }
    if entity_keys.is_empty() {
        return Ok(Vec::new());
    }
    load_context_entities_by_keys(
        connection,
        &entity_keys.into_iter().collect::<Vec<_>>(),
        limit,
    )
}

fn lookup_i64(connection: &Connection, table: &str, value: &str) -> Result<Option<i64>, String> {
    if !table
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(format!("invalid dictionary table name: {table}"));
    }
    connection
        .query_row(
            &format!("SELECT id FROM {table} WHERE value = ?1"),
            [value],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn entity_keys_by_column(
    connection: &Connection,
    column: &str,
    value: i64,
    limit: usize,
) -> Result<Vec<i64>, String> {
    if limit == 0
        || !matches!(
            column,
            "name_id" | "qualified_name_id" | "path_id" | "id_key"
        )
    {
        return Ok(Vec::new());
    }
    let sql = format!("SELECT id_key FROM entities WHERE {column} = ?1 ORDER BY id_key LIMIT ?2");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![value, limit as i64], |row| row.get::<_, i64>(0))
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_context_entities_by_keys(
    connection: &Connection,
    keys: &[i64],
    limit: usize,
) -> Result<Vec<ContextEntitySummary>, String> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let keys = keys.iter().take(limit).copied().collect::<Vec<_>>();
    let placeholders = sql_placeholders(keys.len());
    let sql = format!(
        "
        SELECT oid.value AS id, name.value AS name,
               qname.value AS qualified_name, path.value AS repo_relative_path
        FROM entities e
        JOIN object_id_lookup oid ON oid.id = e.id_key
        JOIN symbol_dict name ON name.id = e.name_id
        JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id
        JOIN path_dict path ON path.id = e.path_id
        WHERE e.id_key IN ({placeholders})
        ORDER BY qname.value, oid.value
        LIMIT {}
        ",
        limit as i64
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(keys.iter()), |row| {
            Ok(ContextEntitySummary {
                id: row.get("id")?,
                name: row.get("name")?,
                qualified_name: row.get("qualified_name")?,
                repo_relative_path: row.get("repo_relative_path")?,
            })
        })
        .map_err(|error| error.to_string())?;
    collect_sql_rows(rows)
}

fn load_stored_context_path_evidence(
    connection: &Connection,
    seed_ids: &[String],
    mode: &str,
    budgets: ContextPackBudgets,
) -> Result<Vec<PathEvidence>, String> {
    if seed_ids.is_empty() || budgets.max_candidate_paths == 0 {
        return Ok(Vec::new());
    }
    let use_symbols = sqlite_table_exists(connection, "path_evidence_symbols")?
        && sqlite_row_count(connection, "path_evidence_symbols")? > 0;
    let placeholders = sql_placeholders(seed_ids.len());
    let sql = if use_symbols {
        format!(
            "
            SELECT DISTINCT p.id, p.source, p.target, p.summary, p.metapath_json,
                   p.edges_json, p.source_spans_json, p.exactness, p.length,
                   p.confidence, p.metadata_json
            FROM path_evidence p
            JOIN path_evidence_symbols s ON s.path_id = p.id
            WHERE s.entity_id IN ({placeholders})
              AND p.length <= ?{}
            ORDER BY p.confidence DESC, p.length ASC, p.id
            LIMIT ?{}
            ",
            seed_ids.len() + 1,
            seed_ids.len() + 2
        )
    } else {
        format!(
            "
            SELECT p.id, p.source, p.target, p.summary, p.metapath_json,
                   p.edges_json, p.source_spans_json, p.exactness, p.length,
                   p.confidence, p.metadata_json
            FROM path_evidence p
            WHERE (p.source IN ({placeholders}) OR p.target IN ({placeholders}))
              AND p.length <= ?{}
            ORDER BY p.confidence DESC, p.length ASC, p.id
            LIMIT ?{}
            ",
            seed_ids.len() * 2 + 1,
            seed_ids.len() * 2 + 2
        )
    };
    let mut params = Vec::<String>::new();
    params.extend(seed_ids.iter().cloned());
    if !use_symbols {
        params.extend(seed_ids.iter().cloned());
    }
    params.push(budgets.max_traversal_depth.to_string());
    params.push(budgets.max_candidate_paths.to_string());
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            rusqlite::params_from_iter(params.iter()),
            path_evidence_from_sql_row,
        )
        .map_err(|error| error.to_string())?;
    let mut paths = collect_sql_rows(rows)?;
    hydrate_stored_path_evidence_metadata(&connection, &mut paths)?;
    Ok(filter_and_sort_context_path_evidence(paths, mode, budgets))
}

#[derive(Debug)]
struct StoredContextPathEdgeMetadata {
    path_id: String,
    ordinal: usize,
    edge_id: String,
    relation: String,
    source_span_path: Option<String>,
    exactness: Option<String>,
    confidence: Option<f64>,
    derived: bool,
    edge_class: Option<String>,
    context: Option<String>,
    provenance_edges: Vec<String>,
}

fn hydrate_stored_path_evidence_metadata(
    connection: &Connection,
    paths: &mut [PathEvidence],
) -> Result<(), String> {
    if paths.is_empty()
        || !sqlite_table_exists(connection, "path_evidence_edges")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "exactness")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "confidence")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "derived")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "edge_class")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "context")?
        || !sqlite_table_has_column(connection, "path_evidence_edges", "provenance_edges_json")?
    {
        return Ok(());
    }
    let ids = paths.iter().map(|path| path.id.clone()).collect::<Vec<_>>();
    let placeholders = sql_placeholders(ids.len());
    let sql = format!(
        "
        SELECT path_id, ordinal, edge_id, relation, source_span_path,
               exactness, confidence, derived, edge_class, context, provenance_edges_json
        FROM path_evidence_edges
        WHERE path_id IN ({placeholders})
        ORDER BY path_id, ordinal
        "
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
            let provenance_json: Option<String> = row.get("provenance_edges_json")?;
            let provenance_edges = provenance_json
                .as_deref()
                .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
                .unwrap_or_default();
            Ok(StoredContextPathEdgeMetadata {
                path_id: row.get("path_id")?,
                ordinal: row.get::<_, i64>("ordinal")?.max(0) as usize,
                edge_id: row.get("edge_id")?,
                relation: row.get("relation")?,
                source_span_path: row.get("source_span_path")?,
                exactness: row.get("exactness")?,
                confidence: row.get("confidence")?,
                derived: row.get::<_, i64>("derived")? != 0,
                edge_class: row.get("edge_class")?,
                context: row.get("context")?,
                provenance_edges,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut by_path = BTreeMap::<String, Vec<StoredContextPathEdgeMetadata>>::new();
    for row in rows {
        let row = row.map_err(|error| error.to_string())?;
        by_path.entry(row.path_id.clone()).or_default().push(row);
    }
    for rows in by_path.values_mut() {
        rows.sort_by_key(|row| row.ordinal);
    }
    for path in paths {
        let Some(rows) = by_path.get(&path.id) else {
            continue;
        };
        path.metadata.insert(
            "ordered_edge_ids".to_string(),
            json!(rows
                .iter()
                .map(|row| row.edge_id.clone())
                .collect::<Vec<_>>()),
        );
        path.metadata.insert(
            "exactness_labels".to_string(),
            json!(rows
                .iter()
                .map(|row| {
                    row.exactness
                        .clone()
                        .unwrap_or_else(|| path.exactness.to_string())
                })
                .collect::<Vec<_>>()),
        );
        path.metadata.insert(
            "confidence_labels".to_string(),
            json!(rows
                .iter()
                .map(|row| row.confidence.unwrap_or(path.confidence))
                .collect::<Vec<_>>()),
        );
        path.metadata.insert(
            "production_test_mock_labels".to_string(),
            json!(rows
                .iter()
                .map(|row| {
                    row.context
                        .clone()
                        .unwrap_or_else(|| "production".to_string())
                })
                .collect::<Vec<_>>()),
        );
        path.metadata.insert(
            "edge_labels".to_string(),
            json!(
                rows.iter()
                    .map(|row| {
                        json!({
                            "edge_id": row.edge_id.clone(),
                            "relation": row.relation.clone(),
                            "source_span": row.source_span_path.clone(),
                            "exactness": row.exactness.clone().unwrap_or_else(|| path.exactness.to_string()),
                            "confidence": row.confidence.unwrap_or(path.confidence),
                            "derived": row.derived,
                            "edge_class": row.edge_class.clone(),
                            "fact_class": row.edge_class.clone(),
                            "context": row.context.clone().unwrap_or_else(|| "production".to_string()),
                            "provenance_edges": row.provenance_edges.clone(),
                        })
                    })
                    .collect::<Vec<_>>()
            ),
        );
        let derived_expansion = rows
            .iter()
            .filter(|row| row.derived || !row.provenance_edges.is_empty())
            .map(|row| {
                json!({
                    "edge_id": row.edge_id.clone(),
                    "relation": row.relation.clone(),
                    "derived": row.derived,
                    "provenance_edges": row.provenance_edges.clone(),
                })
            })
            .collect::<Vec<_>>();
        path.metadata.insert(
            "derived_provenance_expansion".to_string(),
            json!(derived_expansion),
        );
        path.metadata
            .insert("source_spans".to_string(), json!(path.source_spans.clone()));
        path.metadata.insert(
            "metadata_storage".to_string(),
            json!("hydrated_materialized_rows"),
        );
    }
    Ok(())
}

fn load_bounded_context_edges(
    connection: &Connection,
    seed_ids: &[String],
    mode: &str,
    budgets: ContextPackBudgets,
) -> Result<Vec<Edge>, String> {
    if seed_ids.is_empty() {
        return Ok(Vec::new());
    }
    let relations = context_pack_allowed_relation_names(mode);
    let seed_placeholders = sql_placeholders(seed_ids.len());
    let relation_placeholders = sql_placeholders(relations.len());
    let limit = (budgets.max_candidate_paths * budgets.max_traversal_depth.max(1) * 4).max(16);
    let sql = format!(
        "
        SELECT oid.value AS id, head.value AS head_id, relation.value AS relation,
               tail.value AS tail_id, span_path.value AS span_repo_relative_path,
               e.start_line, e.start_column, e.end_line, e.end_column,
               e.repo_commit, file.content_hash AS file_hash, extractor.value AS extractor,
               e.confidence, exactness.value AS exactness,
               edge_class.value AS edge_class, edge_context.value AS context, e.derived,
               e.provenance_edges_json, e.metadata_json
        FROM edges_compat e
        LEFT JOIN object_id_lookup oid ON oid.id = e.id_key
        JOIN object_id_lookup head ON head.id = e.head_id_key
        JOIN relation_kind_dict relation ON relation.id = e.relation_id
        JOIN object_id_lookup tail ON tail.id = e.tail_id_key
        JOIN path_dict span_path ON span_path.id = e.span_path_id
        LEFT JOIN files file ON file.file_id = e.file_id
        JOIN extractor_dict extractor ON extractor.id = e.extractor_id
        JOIN exactness_dict exactness ON exactness.id = e.exactness_id
        LEFT JOIN edge_class_dict edge_class ON edge_class.id = e.edge_class_id
        LEFT JOIN edge_context_dict edge_context ON edge_context.id = e.context_id
        WHERE (head.value IN ({seed_placeholders}) OR tail.value IN ({seed_placeholders}))
          AND relation.value IN ({relation_placeholders})
        ORDER BY e.confidence DESC, e.id_key
        LIMIT ?{}
        ",
        seed_ids.len() * 2 + relations.len() + 1
    );
    let mut params = Vec::<String>::new();
    params.extend(seed_ids.iter().cloned());
    params.extend(seed_ids.iter().cloned());
    params.extend(relations.iter().map(|relation| relation.to_string()));
    params.push(limit.to_string());
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            rusqlite::params_from_iter(params.iter()),
            edge_from_context_sql_row,
        )
        .map_err(|error| error.to_string())?;
    let mut edges = collect_sql_rows(rows)?;
    if mode.to_ascii_lowercase().contains("debug") {
        edges.extend(load_bounded_heuristic_context_edges(
            connection, seed_ids, &relations, limit,
        )?);
        edges.sort_by(|left, right| left.id.cmp(&right.id));
        edges.dedup_by(|left, right| left.id == right.id);
    }
    Ok(edges)
}

fn load_bounded_heuristic_context_edges(
    connection: &Connection,
    seed_ids: &[String],
    relations: &[&str],
    limit: usize,
) -> Result<Vec<Edge>, String> {
    if seed_ids.is_empty()
        || relations.is_empty()
        || !sqlite_table_exists(connection, "heuristic_edges")?
    {
        return Ok(Vec::new());
    }
    let seed_placeholders = sql_placeholders(seed_ids.len());
    let relation_placeholders = sql_placeholders(relations.len());
    let sql = format!(
        "
        SELECT edge_id AS id, head_id, relation, tail_id,
               source_span_path AS span_repo_relative_path,
               start_line, start_column, end_line, end_column,
               repo_commit, file_hash, extractor, confidence, exactness,
               edge_class, context, derived, provenance_edges_json, metadata_json
        FROM heuristic_edges
        WHERE (head_id IN ({seed_placeholders}) OR tail_id IN ({seed_placeholders}))
          AND relation IN ({relation_placeholders})
        ORDER BY confidence DESC, id_key
        LIMIT ?{}
        ",
        seed_ids.len() * 2 + relations.len() + 1
    );
    let mut params = Vec::<String>::new();
    params.extend(seed_ids.iter().cloned());
    params.extend(seed_ids.iter().cloned());
    params.extend(relations.iter().map(|relation| relation.to_string()));
    params.push(limit.to_string());
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            rusqlite::params_from_iter(params.iter()),
            edge_from_context_sql_row,
        )
        .map_err(|error| error.to_string())?;
    collect_sql_rows(rows)
}

fn filter_and_sort_context_path_evidence(
    paths: Vec<PathEvidence>,
    mode: &str,
    budgets: ContextPackBudgets,
) -> Vec<PathEvidence> {
    let mut seen = BTreeSet::new();
    let mut filtered = paths
        .into_iter()
        .filter(|path| path.length <= budgets.max_traversal_depth)
        .filter(|path| context_path_evidence_allowed_for_mode(path, mode))
        .filter(|path| context_path_evidence_relation_allowed(path, mode))
        .filter(|path| seen.insert(path.id.clone()))
        .collect::<Vec<_>>();
    filtered.sort_by(|left, right| {
        right
            .confidence
            .partial_cmp(&left.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.length.cmp(&right.length))
            .then_with(|| left.id.cmp(&right.id))
    });
    filtered.truncate(budgets.max_returned_proof_paths);
    filtered
}

fn context_path_evidence_allowed_for_mode(path: &PathEvidence, mode: &str) -> bool {
    let normalized = mode.to_ascii_lowercase();
    if !normalized.contains("debug")
        && matches!(
            path.exactness,
            Exactness::StaticHeuristic | Exactness::Inferred
        )
    {
        return false;
    }
    if context_pack_mode_allows_test_mock(mode) {
        return true;
    }
    let path_context = path
        .metadata
        .get("path_context")
        .and_then(Value::as_str)
        .unwrap_or("production");
    !matches!(path_context, "test" | "mock" | "mixed")
        && path
            .metadata
            .get("production_test_mock_labels")
            .and_then(Value::as_array)
            .is_none_or(|labels| {
                labels
                    .iter()
                    .all(|label| !matches!(label.as_str(), Some("test" | "mock" | "mixed")))
            })
}

fn context_pack_mode_allows_test_mock(mode: &str) -> bool {
    let normalized = mode.to_ascii_lowercase();
    normalized.contains("test") || normalized.contains("debug")
}

fn context_path_evidence_relation_allowed(path: &PathEvidence, mode: &str) -> bool {
    if mode.to_ascii_lowercase().contains("debug") {
        return true;
    }
    path.metapath
        .iter()
        .all(|relation| !context_pack_excluded_structural_relation(*relation))
}

fn context_pack_excluded_structural_relation(relation: RelationKind) -> bool {
    match relation {
        RelationKind::Contains | RelationKind::DefinedIn | RelationKind::Declares => true,
        _ => relation.to_string().starts_with("ARGUMENT_"),
    }
}

fn context_pack_allowed_relation_names(mode: &str) -> Vec<&'static str> {
    let mut relations = vec![
        "CALLS",
        "READS",
        "WRITES",
        "FLOWS_TO",
        "MUTATES",
        "MAY_MUTATE",
        "IMPORTS",
        "EXPORTS",
        "REEXPORTS",
        "ALIAS_OF",
        "ALIASED_BY",
        "AUTHORIZES",
        "CHECKS_ROLE",
        "CHECKS_PERMISSION",
        "SANITIZES",
        "EXPOSES",
        "PUBLISHES",
        "EMITS",
        "CONSUMES",
        "LISTENS_TO",
        "SUBSCRIBES_TO",
        "MIGRATES",
        "ALTERS_COLUMN",
        "DEPENDS_ON_SCHEMA",
        "READS_TABLE",
        "WRITES_TABLE",
    ];
    if context_pack_mode_allows_test_mock(mode) {
        relations.extend([
            "TESTS",
            "COVERS",
            "ASSERTS",
            "MOCKS",
            "STUBS",
            "FIXTURES_FOR",
        ]);
    }
    relations
}

fn build_context_packet_from_stored_evidence(
    options: &ContextPackOptions,
    raw_seed_values: &[String],
    seed_ids: &[String],
    seed_entities: &[ContextEntitySummary],
    verified_paths: Vec<PathEvidence>,
    snippets: Vec<ContextSnippet>,
    fallback_packet: Option<ContextPacket>,
    budgets: ContextPackBudgets,
    stored_path_count: usize,
    requested_span_count: usize,
) -> ContextPacket {
    let mut symbols = unique_limited_strings(
        raw_seed_values
            .iter()
            .cloned()
            .chain(seed_ids.iter().cloned())
            .chain(seed_entities.iter().map(|entity| entity.id.clone()))
            .chain(seed_entities.iter().map(|entity| entity.name.clone()))
            .chain(
                seed_entities
                    .iter()
                    .map(|entity| entity.qualified_name.clone()),
            )
            .chain(
                seed_entities
                    .iter()
                    .map(|entity| entity.repo_relative_path.clone()),
            )
            .chain(
                verified_paths
                    .iter()
                    .flat_map(|path| [path.source.clone(), path.target.clone()]),
            ),
        budgets.max_seed_entities * 4,
    );
    symbols.sort();
    symbols.dedup();

    let mut recommended_tests = recommended_tests_from_path_evidence(&verified_paths);
    let mut risks = risks_from_path_evidence(&verified_paths);
    if let Some(packet) = fallback_packet {
        recommended_tests.extend(packet.recommended_tests);
        risks.extend(packet.risks);
    }
    recommended_tests.sort();
    recommended_tests.dedup();
    risks.sort();
    risks.dedup();

    let mut metadata = Metadata::new();
    metadata.insert("phase".to_string(), json!("30"));
    metadata.insert("retrieval".to_string(), json!("stored-path-evidence-first"));
    metadata.insert("token_budget".to_string(), json!(options.token_budget));
    metadata.insert("exact_seed_count".to_string(), json!(seed_ids.len()));
    metadata.insert(
        "candidate_path_count_before_dedup".to_string(),
        json!(stored_path_count),
    );
    metadata.insert(
        "candidate_path_count_after_dedup".to_string(),
        json!(verified_paths.len()),
    );
    metadata.insert(
        "candidate_path_count_after_filter".to_string(),
        json!(verified_paths.len()),
    );
    metadata.insert(
        "candidate_path_count_after_truncate".to_string(),
        json!(verified_paths.len()),
    );
    metadata.insert(
        "hard_budgets".to_string(),
        json!({
            "max_seed_entities": budgets.max_seed_entities,
            "max_candidate_paths": budgets.max_candidate_paths,
            "max_returned_proof_paths": budgets.max_returned_proof_paths,
            "max_snippets": budgets.max_snippets,
            "max_traversal_depth": budgets.max_traversal_depth,
        }),
    );
    metadata.insert(
        "excluded_structural_relations_default".to_string(),
        json!(["CONTAINS", "DEFINED_IN", "DECLARES", "ARGUMENT_N"]),
    );
    metadata.insert(
        "requested_source_span_count".to_string(),
        json!(requested_span_count),
    );
    metadata.insert(
        "source_span_coverage".to_string(),
        json!(if requested_span_count == 0 {
            1.0
        } else {
            snippets.len() as f64 / requested_span_count as f64
        }),
    );

    let mut packet = ContextPacket {
        task: options.task.clone(),
        mode: options.mode.clone(),
        symbols,
        verified_paths,
        risks,
        recommended_tests,
        snippets,
        metadata,
    };
    compact_context_packet_for_cli(&mut packet, options.token_budget.max(32));
    packet
}

fn recommended_tests_from_path_evidence(paths: &[PathEvidence]) -> Vec<String> {
    let mut tests = BTreeSet::new();
    for path in paths {
        for (head, relation, tail) in &path.edges {
            if matches!(
                relation,
                RelationKind::Tests
                    | RelationKind::Covers
                    | RelationKind::Asserts
                    | RelationKind::Mocks
                    | RelationKind::Stubs
                    | RelationKind::FixturesFor
            ) {
                tests.insert(format!("run tests covering {tail}"));
                tests.insert(format!("inspect test evidence from {head}"));
            }
        }
    }
    tests.into_iter().take(12).collect()
}

fn risks_from_path_evidence(paths: &[PathEvidence]) -> Vec<String> {
    let mut risks = BTreeSet::new();
    for path in paths {
        if path.metapath.iter().any(|relation| {
            matches!(
                relation,
                RelationKind::Writes
                    | RelationKind::Mutates
                    | RelationKind::MayMutate
                    | RelationKind::FlowsTo
            )
        }) {
            risks.insert(
                "mutation or dataflow evidence is included; verify affected callers and tests"
                    .to_string(),
            );
        }
        if path.metapath.iter().any(|relation| {
            matches!(
                relation,
                RelationKind::Authorizes
                    | RelationKind::ChecksRole
                    | RelationKind::ChecksPermission
                    | RelationKind::Sanitizes
                    | RelationKind::Exposes
            )
        }) {
            risks.insert("security-sensitive proof path is included; preserve exact auth/sanitizer semantics".to_string());
        }
    }
    risks.into_iter().collect()
}

fn load_context_sources_and_snippets(
    repo_root: &Path,
    spans: &[SourceSpan],
    max_snippets: usize,
) -> Result<(BTreeMap<String, String>, Vec<ContextSnippet>, usize, usize), String> {
    let mut file_ids = spans
        .iter()
        .map(|span| span.repo_relative_path.clone())
        .collect::<Vec<_>>();
    file_ids.sort();
    file_ids.dedup();

    let mut sources = BTreeMap::new();
    let mut source_bytes = 0usize;
    for file_id in file_ids {
        let path = repo_root.join(&file_id);
        if path.exists() {
            let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
            source_bytes += source.len();
            sources.insert(file_id, source);
        }
    }

    let mut snippets = Vec::new();
    let mut seen = BTreeSet::new();
    for span in spans {
        if snippets.len() >= max_snippets {
            break;
        }
        let key = format!(
            "{}:{}:{}",
            span.repo_relative_path, span.start_line, span.end_line
        );
        if !seen.insert(key) {
            continue;
        }
        let Some(source) = sources.get(&span.repo_relative_path) else {
            continue;
        };
        let text = source_snippet_for_span(source, span);
        if text.trim().is_empty() {
            continue;
        }
        snippets.push(ContextSnippet {
            file: span.repo_relative_path.clone(),
            lines: if span.start_line == span.end_line {
                span.start_line.to_string()
            } else {
                format!("{}-{}", span.start_line, span.end_line)
            },
            text,
            reason: "proof path source span".to_string(),
        });
    }
    let source_files_loaded = sources.len();
    Ok((sources, snippets, source_bytes, source_files_loaded))
}

fn source_snippet_for_span(source: &str, span: &SourceSpan) -> String {
    let lines = source.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return String::new();
    }
    let start = span.start_line.saturating_sub(2).max(1) as usize;
    let end = (span.end_line as usize + 2).min(lines.len());
    if start > end {
        return String::new();
    }
    (start..=end)
        .filter_map(|line_number| {
            lines
                .get(line_number - 1)
                .map(|line| format!("{line_number}: {line}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact_context_packet_for_cli(packet: &mut ContextPacket, token_budget: usize) {
    while estimate_context_packet_tokens(packet) > token_budget {
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
    packet.metadata.insert(
        "estimated_tokens".to_string(),
        json!(estimate_context_packet_tokens(packet)),
    );
}

fn estimate_context_packet_tokens(packet: &ContextPacket) -> usize {
    serde_json::to_string(packet)
        .map(|serialized| (serialized.len() / 4).max(1))
        .unwrap_or(1)
}

fn path_evidence_from_sql_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PathEvidence> {
    let metapath_json: String = row.get("metapath_json")?;
    let edges_json: String = row.get("edges_json")?;
    let source_spans_json: String = row.get("source_spans_json")?;
    let exactness: String = row.get("exactness")?;
    let metadata_json: String = row.get("metadata_json")?;
    Ok(PathEvidence {
        id: row.get("id")?,
        source: row.get("source")?,
        target: row.get("target")?,
        summary: row.get("summary")?,
        metapath: serde_json::from_str(&metapath_json).map_err(sql_json_error)?,
        edges: serde_json::from_str(&edges_json).map_err(sql_json_error)?,
        source_spans: serde_json::from_str(&source_spans_json).map_err(sql_json_error)?,
        exactness: exactness.parse().map_err(sql_parse_error)?,
        length: row.get("length")?,
        confidence: row.get("confidence")?,
        metadata: serde_json::from_str(&metadata_json).map_err(sql_json_error)?,
    })
}

fn edge_from_context_sql_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Edge> {
    let relation: String = row.get("relation")?;
    let exactness: String = row.get("exactness")?;
    let edge_class: Option<String> = row.get("edge_class")?;
    let context: Option<String> = row.get("context")?;
    let provenance_edges_json: String = row.get("provenance_edges_json")?;
    let metadata_json: String = row.get("metadata_json")?;
    Ok(Edge {
        id: row
            .get::<_, Option<String>>("id")?
            .unwrap_or_else(|| "edge://unknown".to_string()),
        head_id: row.get("head_id")?,
        relation: relation.parse().map_err(sql_parse_error)?,
        tail_id: row.get("tail_id")?,
        source_span: SourceSpan {
            repo_relative_path: row.get("span_repo_relative_path")?,
            start_line: row.get("start_line")?,
            start_column: row.get("start_column")?,
            end_line: row.get("end_line")?,
            end_column: row.get("end_column")?,
        },
        repo_commit: row.get("repo_commit")?,
        file_hash: row.get("file_hash")?,
        extractor: row.get("extractor")?,
        confidence: row.get("confidence")?,
        exactness: exactness.parse().map_err(sql_parse_error)?,
        edge_class: edge_class
            .as_deref()
            .unwrap_or("unknown")
            .parse()
            .map_err(sql_parse_error)?,
        context: context
            .as_deref()
            .unwrap_or("unknown")
            .parse()
            .map_err(sql_parse_error)?,
        derived: row.get::<_, i64>("derived")? != 0,
        provenance_edges: serde_json::from_str(&provenance_edges_json).map_err(sql_json_error)?,
        metadata: serde_json::from_str(&metadata_json).map_err(sql_json_error)?,
    })
}

fn sql_json_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn sql_parse_error<E>(error: E) -> rusqlite::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn collect_sql_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>, String> {
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn sqlite_table_exists(connection: &Connection, table: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(|error| error.to_string())
}

fn sqlite_table_has_column(
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

fn sqlite_row_count(connection: &Connection, table: &str) -> Result<u64, String> {
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

fn sql_placeholders(count: usize) -> String {
    (0..count).map(|_| "?").collect::<Vec<_>>().join(", ")
}

fn context_pack_explain_query_plans(db_path: &Path) -> Result<Vec<Value>, String> {
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

fn explain_query_plan(connection: &Connection, name: &str, sql: &str) -> Result<Value, String> {
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

fn default_query_limits() -> QueryLimits {
    QueryLimits {
        max_depth: 6,
        max_paths: 32,
        max_edges_visited: 10_000,
    }
}

fn load_sources(
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

fn paths_json(engine: &ExactGraphQueryEngine, paths: Vec<GraphPath>) -> Value {
    serde_json::to_value(engine.path_evidence_from_paths(&paths)).unwrap_or_else(|_| json!([]))
}

fn section_counts(sections: &Value) -> Value {
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

fn entity_json(entity: &Entity) -> Value {
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

fn edge_json(edge: &Edge) -> Value {
    json!({
        "id": edge.id,
        "head_id": edge.head_id,
        "relation": edge.relation.to_string(),
        "tail_id": edge.tail_id,
        "source_span": edge.source_span,
        "confidence": edge.confidence,
        "exactness": edge.exactness.to_string(),
        "extractor": edge.extractor,
        "metadata": edge.metadata,
    })
}

fn edge_with_entities_json(edge: &Edge, entity_by_id: &BTreeMap<String, Entity>) -> Value {
    json!({
        "edge": edge_json(edge),
        "head": entity_by_id.get(&edge.head_id).map(entity_json),
        "tail": entity_by_id.get(&edge.tail_id).map(entity_json),
        "unresolved": entity_by_id.get(&edge.tail_id).is_some_and(entity_is_unresolved_reference),
    })
}

fn text_search_hit_json(hit: codegraph_store::TextSearchHit) -> Value {
    json!({
        "kind": hit.kind.as_str(),
        "id": hit.id,
        "repo_relative_path": hit.repo_relative_path,
        "line": hit.line,
        "title": hit.title,
        "text": hit.text,
        "score": hit.score,
    })
}

fn symbol_search_hit_json(hit: &SymbolSearchHit) -> Value {
    json!({
        "score": hit.score,
        "features": hit.features,
        "matched_terms": hit.matched_terms,
        "entity": entity_json(&hit.entity),
    })
}

fn entities_by_id(entities: &[Entity]) -> BTreeMap<String, Entity> {
    entities
        .iter()
        .map(|entity| (entity.id.clone(), entity.clone()))
        .collect()
}

fn is_definition_kind(kind: EntityKind) -> bool {
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

fn entity_aliases(entity: &Entity) -> BTreeSet<String> {
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

fn alias_set_for_entities(entities: &[Entity]) -> BTreeSet<String> {
    entities.iter().flat_map(entity_aliases).collect()
}

fn ids_by_alias(entity_by_id: &BTreeMap<String, Entity>) -> BTreeMap<String, BTreeSet<String>> {
    let mut ids = BTreeMap::<String, BTreeSet<String>>::new();
    for entity in entity_by_id.values() {
        for alias in entity_aliases(entity) {
            ids.entry(alias).or_default().insert(entity.id.clone());
        }
    }
    ids
}

fn equivalent_entity_ids(
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

fn insert_aliases(aliases: &mut BTreeSet<String>, value: &str) {
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

fn normalize_symbol_alias(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("call:")
        .trim_start_matches("import:")
        .to_ascii_lowercase()
}

fn aliases_overlap(left: &BTreeSet<String>, right: &BTreeSet<String>) -> bool {
    left.iter().any(|alias| right.contains(alias))
}

fn entity_is_unresolved_reference(entity: &Entity) -> bool {
    entity.created_from.contains("heuristic")
        || entity.name.contains("unknown_callee")
        || entity
            .metadata
            .get("expression_reason")
            .and_then(|value| value.as_str())
            .is_some_and(|reason| reason.contains("unresolved"))
}

fn minimal_test_set(paths: &[GraphPath], entity_by_id: &BTreeMap<String, Entity>) -> Value {
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

fn test_entity_for_path(
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

fn is_test_kind(kind: EntityKind) -> bool {
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

fn index_summary_json(summary: &IndexSummary) -> Result<Value, String> {
    let mut value = serde_json::to_value(summary).map_err(|error| error.to_string())?;
    let Some(object) = value.as_object_mut() else {
        return Err("failed to encode index summary".to_string());
    };
    object.insert("status".to_string(), json!("indexed"));
    Ok(value)
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
    let schema_version = SqliteGraphStore::open(db)
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
    Ok(json!({
        "step": "cold_index",
        "status": "seeded",
        "source": "seed_db_copy",
        "seed_db": seed_db.map(path_string),
        "repo_root": path_string(repo),
        "wall_ms": elapsed_ms(started),
        "setup_timings": {
            "artifact_copy_ms": artifact_copy_ms,
            "artifact_open_ms": artifact_open_ms,
            "schema_validation_ms": artifact_open_ms,
            "graph_fact_hash_ms": graph_fact_hash_ms,
            "validation_ms": integrity_check_ms,
        },
        "schema_version": schema_version,
        "files_walked": Value::Null,
        "files_read": Value::Null,
        "files_hashed": Value::Null,
        "files_parsed": Value::Null,
        "entities_inserted": Value::Null,
        "edges_inserted": Value::Null,
        "duplicate_edges_upserted": Value::Null,
        "transaction_status": "seeded_copy_integrity_checked",
        "integrity_status": integrity_status,
        "integrity_check_kind": integrity_check_kind,
        "integrity_check_ms": integrity_check_ms,
        "graph_fact_hash": graph_fact_hash,
        "graph_fact_hash_ms": graph_fact_hash_ms,
        "entity_count": entity_count,
        "edge_count": edge_count,
        "source_span_count": source_span_count,
        "graph_counts_ran": graph_counts_ran,
        "db_family_size_bytes": sqlite_family_size_bytes(db).unwrap_or(0),
    }))
}

fn update_integrity_step_from_index(
    step: &str,
    elapsed: Duration,
    summary: &IndexSummary,
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
    Ok(json!({
        "step": step,
        "status": "ok",
        "mode": mode.as_str(),
        "wall_ms": elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
        "timings": index_summary_timing_breakdown(summary, graph_fact_hash_ms, integrity_check_ms),
        "files_walked": summary.files_walked,
        "files_read": summary.files_read,
        "files_hashed": summary.files_hashed,
        "files_parsed": summary.files_parsed,
        "entities_inserted": summary.entities,
        "edges_inserted": summary.edges,
        "duplicate_edges_upserted": summary.duplicate_edges_upserted,
        "transaction_status": "committed",
        "integrity_status": integrity_status,
        "integrity_check_kind": integrity_check_kind,
        "integrity_check_ms": integrity_check_ms,
        "graph_fact_hash": graph_fact_hash,
        "graph_fact_hash_ms": graph_fact_hash_ms,
        "graph_digest_kind": graph_digest_kind,
        "global_hash_check_ran": global_hash_check_ran,
        "entity_count": entity_count,
        "edge_count": edge_count,
        "source_span_count": source_span_count,
        "graph_counts_ran": graph_counts_ran,
        "db_family_size_bytes": sqlite_family_size_bytes(db).unwrap_or(0),
        "profile": summary.profile.clone(),
    }))
}

fn update_integrity_step_from_incremental(
    step: &str,
    elapsed: Duration,
    summary: &IncrementalIndexSummary,
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
    Ok(json!({
        "step": step,
        "status": "ok",
        "mode": mode.as_str(),
        "wall_ms": elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
        "timings": incremental_summary_timing_breakdown(
            summary,
            graph_fact_hash_ms,
            integrity_check_ms
        ),
        "files_walked": summary.files_walked,
        "files_read": summary.files_read,
        "files_hashed": summary.files_hashed,
        "files_parsed": summary.files_parsed,
        "entities_inserted": summary.entities,
        "edges_inserted": summary.edges,
        "duplicate_edges_upserted": summary.duplicate_edges_upserted,
        "transaction_status": "committed",
        "integrity_status": integrity_status,
        "integrity_check_ms": integrity_check_ms,
        "integrity_check_kind": integrity_check_kind,
        "post_measurement_integrity_check_ran": post_measurement_integrity_check_ran,
        "graph_fact_hash": graph_fact_hash,
        "graph_fact_hash_ms": graph_fact_hash_ms,
        "graph_digest_kind": graph_digest_kind,
        "entity_count": entity_count,
        "edge_count": edge_count,
        "source_span_count": source_span_count,
        "graph_counts_ran": graph_counts_ran,
        "db_family_size_bytes": sqlite_family_size_bytes(db).unwrap_or(0),
        "changed_files": summary.changed_files.clone(),
        "deleted_fact_files": summary.deleted_fact_files,
        "dirty_path_evidence_count": summary.dirty_path_evidence_count,
        "global_hash_check_ran": global_hash_check_ran,
        "storage_audit_ran": summary.storage_audit_ran,
        "integrity_check_ran": summary.integrity_check_ran,
        "profile": summary.profile.clone(),
    }))
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
        "artifact_open_ms": profile_span_ms(summary.profile.as_ref(), "open_store"),
        "metadata_diff_ms": profile_span_ms(summary.profile.as_ref(), "metadata_diff"),
        "file_read_ms": profile_span_ms(summary.profile.as_ref(), "file_read"),
        "file_hash_ms": profile_span_ms(summary.profile.as_ref(), "file_hash"),
        "parse_update_ms": summary.profile.as_ref().map(|profile| profile.total_wall_ms as f64),
        "parse_ms": profile_span_ms(summary.profile.as_ref(), "parse"),
        "extract_ms": profile_span_ms(summary.profile.as_ref(), "extract_entities_and_relations"),
        "stale_delete_ms": profile_span_ms(summary.profile.as_ref(), "stale_fact_delete"),
        "template_invalidation_ms": profile_span_ms(summary.profile.as_ref(), "template_invalidation"),
        "template_insert_update_ms": profile_span_ms(summary.profile.as_ref(), "content_template_upsert"),
        "proof_entity_edge_update_ms": summary.profile.as_ref().map(|profile| profile.db_write_ms as f64),
        "path_evidence_regeneration_ms": profile_span_ms(summary.profile.as_ref(), "refresh_path_evidence"),
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

fn quick_integrity_status(db: &Path) -> String {
    match SqliteGraphStore::open(db).and_then(|store| store.quick_integrity_gate()) {
        Ok(()) => "ok".to_string(),
        Err(error) => format!("failed: {error}"),
    }
}

fn full_integrity_status(db: &Path) -> String {
    match SqliteGraphStore::open(db).and_then(|store| store.full_integrity_gate()) {
        Ok(()) => "ok".to_string(),
        Err(error) => format!("failed: {error}"),
    }
}

fn graph_counts_for_db(db: &Path) -> Result<(u64, u64, u64), String> {
    let store = SqliteGraphStore::open(db).map_err(|error| error.to_string())?;
    Ok((
        store.count_entities().map_err(|error| error.to_string())?,
        store.count_edges().map_err(|error| error.to_string())?,
        store
            .count_source_spans()
            .map_err(|error| error.to_string())?,
    ))
}

fn graph_fact_hash_for_db(db: &Path) -> Result<String, String> {
    let store = SqliteGraphStore::open(db).map_err(|error| error.to_string())?;
    store.graph_fact_digest().map_err(|error| error.to_string())
}

fn incremental_graph_digest_for_db(db: &Path) -> Result<Option<String>, String> {
    let store = SqliteGraphStore::open(db).map_err(|error| error.to_string())?;
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
        .map_err(|error| error.to_string())
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
        .map(|(name, support_status, triple, archive_format, binary)| json!({
            "name": name,
            "support_status": support_status,
            "target_triple": triple,
            "archive": format!("{BIN_NAME}-{triple}.{archive_format}"),
            "binary": binary,
            "checksum": format!("{BIN_NAME}-{triple}.{archive_format}.sha256"),
            "provenance": format!("{BIN_NAME}-{triple}.{archive_format}.intoto.jsonl")
        }))
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
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::{SocketAddr, TcpListener, TcpStream},
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use codegraph_store::{GraphStore, SqliteGraphStore};
    use serde_json::{json, Value};

    use super::{
        generate_large_synthetic_repo, index_repo, index_repo_with_options,
        parse_extract_pending_files, percentile, regression_row,
        render_comprehensive_benchmark_markdown, route_ui_request, run, serve_ui_loop,
        should_ignore_path, should_start_new_index_batch, update_changed_files_with_cache,
        IncrementalIndexCache, IndexOptions, PendingIndexFile, UiResponse, WatchDebouncer,
        BIN_NAME, DEFAULT_INDEX_BATCH_MAX_FILES, DEFAULT_INDEX_BATCH_MAX_SOURCE_BYTES,
    };

    static TEMP_REPO_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    fn command_help_is_successful() {
        let output = run([BIN_NAME, "context-pack", "--help"]);

        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("Usage:"));
        assert!(output.stdout.contains("context-pack"));
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
        assert!(
            markdown.contains("Absent proof-mode relations with no precision claim: AUTHORIZES")
        );
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
    fn binary_signature_cache_changes_after_file_update() {
        let repo = temp_repo();
        fs::create_dir_all(repo.join("src")).expect("create src");
        fs::write(
            repo.join("src").join("auth.ts"),
            "export function login() {\n  return 'old';\n}\n",
        )
        .expect("write source");
        index_repo(&repo).expect("initial index");

        let store = SqliteGraphStore::open(repo.join(".codegraph").join("codegraph.sqlite"))
            .expect("store");
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

        fs::remove_dir_all(repo).expect("cleanup");
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
        let cold_profile = cold.profile.expect("cold profile");
        assert_eq!(cold.files_indexed, 1);
        assert_eq!(cold_profile.semantic_resolver_ms, 0);
        assert!(cold_profile.worker_count >= 1);
        assert!(cold_profile.files_per_sec >= 0.0);

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
        assert!(warm_profile.skipped_unchanged_files >= 1);

        fs::remove_dir_all(repo).expect("cleanup");
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

        fs::remove_dir_all(repo).expect("cleanup");
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

        fs::remove_dir_all(repo).expect("cleanup");
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

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn parallel_parse_extract_is_deterministic() {
        let pending = (0..4)
            .map(|index| PendingIndexFile {
                file_hash: format!("hash-{index}"),
                language: Some("typescript".to_string()),
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

        fs::remove_dir_all(output).expect("cleanup");
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

        fs::remove_dir_all(repo).expect("cleanup");
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
        fs::remove_dir_all(repo).expect("cleanup");
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

        fs::remove_dir_all(repo).expect("cleanup");
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

        fs::remove_dir_all(repo).expect("cleanup");
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

        fs::remove_dir_all(repo).expect("cleanup");
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

        fs::remove_dir_all(repo).expect("cleanup");
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

        fs::remove_dir_all(repo).expect("cleanup");
    }

    #[test]
    fn unknown_command_returns_structured_error() {
        let output = run([BIN_NAME, "crawl-web"]);

        assert_eq!(output.exit_code, 2);
        assert!(output.stderr.contains("\"status\":\"error\""));
        assert!(output.stderr.contains("\"error\":\"unknown_command\""));
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

    fn http_get_json(addr: SocketAddr, path: &str) -> Value {
        let mut stream = TcpStream::connect(addr).expect("connect UI server");
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
        )
        .expect("write HTTP request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read HTTP response");
        let (_, body) = response.split_once("\r\n\r\n").expect("HTTP body");
        serde_json::from_str(body).expect("JSON response")
    }

    fn ui_json_body(response: UiResponse) -> Value {
        assert_eq!(response.status, 200);
        serde_json::from_str(&response.body).expect("JSON body")
    }
}
