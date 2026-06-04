# CLI Reference

The root `README.md` is the public setup contract. The CLI is local-first and
keeps the workflow single-agent only. No command recommends or launches
subagents.

## Global

```powershell
codegraph-mcp --help
codegraph-mcp <command> --help
codegraph-mcp --json --version
```

Most commands emit one JSON object to stdout on success and one structured JSON
error to stderr on failure.

Global flags are accepted before the command name:

```text
--repo <path>  --db <path>  --json  --agent-json  --limit <n>
--no-color  --verbose  --quiet  --profile
```

`--repo` sets the working repository, `--db` overrides
`CODEGRAPH_DB_PATH`, and global `--profile` enables index profiling for the
`index` command. Global `--agent-json` and `--limit` are forwarded only to
agent-use query/context-pack surfaces where the command supports them. For
routine agent use, prefer the `agent-use` namespace instead of reusing
temporary development or lab DBs.

Command-local flags remain after the command. Ambiguous global flags after
query subcommands return targeted corrections instead of becoming query text.
For example, `query symbols greet --db <db>` fails with guidance to put `--db`
before `query`. Use `--` for literal flag-shaped search terms, such as
`query text --agent-json -- --db`.

## Agent-Friendly Output

Use `--agent-json` for tight coding-agent loops and `--limit <n>` to keep
results bounded:

```powershell
codegraph-mcp agent-use query symbols <symbol> --repo <repo> `
  --limit 5 --agent-json
codegraph-mcp agent-use query callers <symbol> --repo <repo> `
  --limit 5 --agent-json
codegraph-mcp agent-use query callees <symbol> --repo <repo> `
  --limit 5 --agent-json
codegraph-mcp agent-use query path <source> <target> --repo <repo> `
  --limit 3 --agent-json

codegraph-mcp agent-use context-pack --repo <repo> `
  --task "Trace the change impact" `
  --agent-json

codegraph-mcp agent-use validate-edit --repo <repo> `
  --changed src\file.ts `
  --agent-json
```

Supported compact modes:

- `--agent-json`: schema-versioned, bounded JSON for agent loops.
- `--concise`: compact output where supported.
- `--verbose`, `--debug`, `--profile`, and `--audit-json`: explicit rich or
  audit detail.
- `--explain-scope`, `--print-included`, and `--print-excluded`: explicit
  index scope examples.

`index --json` is concise by default. It excludes full scope examples and audit
payloads unless one of the explicit audit/scope flags is supplied.

## Production Agent-Use Namespace

Use the first-class namespace for routine coding-agent work:

```powershell
codegraph-mcp agent-use status --repo <repo> --json
codegraph-mcp agent-use index --repo <repo> --json
codegraph-mcp agent-use query symbols <symbol> --repo <repo> --limit 5 --agent-json
codegraph-mcp agent-use query text "text" --repo <repo> --limit 5 --agent-json
codegraph-mcp agent-use query files service --repo <repo> --limit 5 --agent-json
codegraph-mcp agent-use query callers handle_request --repo <repo> --limit 5 --agent-json
codegraph-mcp agent-use query callees handle_request --repo <repo> --limit 5 --agent-json
codegraph-mcp agent-use query path handle_request save_record --repo <repo> --limit 3 --agent-json
codegraph-mcp agent-use context-pack --repo <repo> --task "Trace the change impact" --agent-json
codegraph-mcp agent-use mcp-config --repo <repo> --json
codegraph-mcp agent-use watch --repo <repo> --once --changed src\file.ts --json
codegraph-mcp agent-use validate-edit --repo <repo> --changed src\file.ts --agent-json
codegraph-mcp agent-use validate-edit --repo <repo> --changed src\file.ts --changed src\helper.ts --fail-on-blocking --agent-json
codegraph-mcp agent-use watch --repo <repo> --json
```

The namespace resolves the `production-agent-use` profile to an external DB
under the platform data directory, such as LocalAppData on Windows. Repos with
the same basename receive distinct profile paths. No `agent-use` command
silently falls back to repo-local `.codegraph`.

`agent-use status` is read-only and does not create the DB, profile parent, or
SQLite sidecars. `agent-use index` is the first mutating command and writes the
external profile DB plus bounded candidate/vector artifacts. `agent-use query`
and `agent-use context-pack` read the same external DB. `agent-use mcp-config`
emits config JSON only by default and must agree with `agent-use status` on the
DB path. `agent-use watch --once --changed <path>` is the deterministic
changed-file update primitive for that same profile DB. It requires an existing
safe graph DB, rejects `--db`, does not auto-index, and reports staged sidecar
freshness after the update. Persistent `agent-use watch --repo <repo> --json`
debounces and coalesces filesystem events, serializes writer work, retries
transient locks within bounds, and calls the same changed-file update contract
rather than owning a second delta engine. It also refuses unsafe profile DB
states instead of creating a new graph silently.

`agent-use query` maps to the same query engine as plain `query` while resolving
the external production profile DB first. It supports `symbols`, `text`,
`files`, `callers`, `callees`, `path`, `chain`, `references`, `definitions`,
and `unresolved-calls` where the corresponding plain query exists. Unsafe
missing, stale, foreign, or schema-mismatched profile DB states fail closed.
Relation and path agent JSON includes relation kind, exactness/provenance,
source spans, evidence role, `proof_status`, and `proof_strength`;
`graph_proof=true` is only for verified graph/source relation proof.

`agent-use validate-edit` is the canonical agent/editor validation surface:

```powershell
codegraph-mcp agent-use validate-edit --repo <repo> --changed <path> --agent-json
```

Repeat `--changed` for multi-file edits. Add `--fail-on-blocking` when a
pre-commit or CI gate should return exit 2 for a hard interrupt while still
printing stdout JSON. Use `--explain` or `--audit-json` only for richer
diagnostic output. The top-level compatibility alias
`codegraph-mcp validate-edit ...` is deferred; use the canonical `agent-use`
command above.

Exit 0 means validation completed and emitted JSON, even when the packet status
is `blocking_graph_error`. With `--fail-on-blocking`, exit 2 means validation
completed, stdout JSON was printed, and `hard_interrupt_available=true`. Other
nonzero exits mean validation did not complete because of command, config,
lifecycle, tool, runtime, or protocol failure. Agents should parse stdout JSON
and must not infer proof from exit code alone.

CodeGraph does not replace compilers, tests, type checkers, linters, runtime
checks, or security review. Hard interrupts derive only from reverified
graph/source proof or eligible integrity/lifecycle proof failures. Warning,
unknown, unsupported, degraded, diagnostic-only, text, candidate, vector, and
source-navigation evidence do not interrupt by default; unsafe DB state is a
lifecycle blocker, not source-code proof.

Staged candidate spool and runtime vector sidecar output is candidate context,
not graph proof. Optional audit artifacts are diagnostic-only. Missing, stale,
foreign, schema-mismatched, locked, permission-denied, and publishing states are
non-claimable unless explicitly labeled diagnostic-only. Plain `status --json`
remains the local `.codegraph` status surface and may include guidance to
agent-use without redirecting.

Telemetry fields distinguish measured, unknown, and aggregated values. Memory
is `memory: "unknown"` with `memory_measured: false` unless measured. Timing
substage fields that cannot be separated are labeled unknown or aggregated.

## Commands

`init [repo] [--dry-run] [--with-codex-config] [--with-agents] [--with-skills] [--with-hooks] [--with-templates] [--index]`

Detects repo tooling, creates `.codegraph/`, and can install Codex config,
`AGENTS.md`, skill templates, hook templates, and an initial index.

`index <repo> [--db <path>] [--fresh|--rebuild] [--incremental] [--fail-on-db-problem] [--allow-stale-reuse] [--build-vector-index <path>] [--profile] [--json|--agent-json|--concise|--audit-json] [--verbose]`

Indexes supported language frontends into `.codegraph/codegraph.sqlite`.
Unchanged files are skipped by content hash. Changed files are parsed and
extracted through a deterministic parallel worker pool, then written in a
single batched SQLite transaction. Scope starts with the default repo policy;
`--include <pattern>` is an explicit include override for paths that policy
would otherwise exclude, not a restrictive only-these-globs filter.
`--profile --json` includes discovery, parse, extraction, semantic resolver,
DB write, FTS/search index, signature, total wall time, throughput, worker
count, unchanged skip count, and explicit measurement status for memory and
timing substages. Unmeasured memory is reported as `memory: "unknown"` with
`memory_measured: false`.

`--build-vector-index <path>` writes an optional deterministic local vector
chunk index for later candidate recall. The vector index is lifecycle-bound to
the DB passport, provider metadata, scope, and extraction version. It is a
candidate source only; it does not make vector results graph proof.

DB lifecycle flags:

- `--fresh` / `--rebuild` always builds a fresh replacement and publishes it
  only after validation.
- `--incremental` requires a reusable passported DB and fails if reuse is
  unsafe.
- `--fail-on-db-problem` fails instead of safe-auto rebuilding.
- `--allow-stale-reuse` is diagnostic only; output must be labeled
  contaminated and not claimable.
- Query, context, impact, and unresolved-call read paths also expose explicit
  diagnostic read flags where supported: `--allow-stale-read` for stale or
  passport diagnostic output, and `--allow-foreign-db` for foreign-repo
  diagnostic output. These are not normal agent-use flags.

Scope flags include `--include-ignored`, `--include <pattern>`,
`--exclude <pattern>`, `--no-default-excludes`,
`--respect-gitignore <true|false>`, `--explain-scope`, `--print-included`, and
`--print-excluded`. A DB built with non-default scope records that scope in the
passport, and read paths validate against it.

`status [repo]`

Reports schema version, file/entity/edge counts, detected tooling, DB
lifecycle health, passport status, and SQLite sidecar status.

`query symbols <query> [--limit <n>] [--concise|--agent-json] [--verbose|--debug|--explain]`

Ranks symbols across simple names, qualified names, file paths, namespaces,
doc/signature metadata, alias/import names, identifier tokens, and
relation-neighbor text. Exact and qualified matches outrank fuzzy matches.

`query text <query> [--limit <n>] [--concise|--agent-json] [--verbose|--debug|--explain]`

Searches the local SQLite FTS/BM25 index across files, entities, and snippets.

`query files <query> [--limit <n>] [--concise|--agent-json] [--verbose|--debug|--explain]`

Finds repo-relative files by FTS and path proximity.

`query references <symbol> [--limit <n>] [--concise|--agent-json]`

Lists graph edges connected to a resolved symbol or explicit unresolved
same-name placeholders. Agent JSON labels graph references separately from text
references.

`query definitions <symbol> [--limit <n>] [--concise|--agent-json]`

Returns declaration/executable symbol hits. Definitions are symbol-location
evidence and do not claim behavior by themselves.

`query callers [--entity-id <id>|--exact-resolved|--fuzzy] [--limit <n>] [--concise|--agent-json] <symbol>`

Returns `CALLS` edges whose callee resolves to the symbol. When a symbol
resolves to exactly one persisted entity, default output uses exact resolved
entity results. Ambiguous symbols return candidate entity ids instead of
pretending broad results are exact. Use `--entity-id` for exact persisted-entity
traversal, `--exact-resolved` to require one symbol match, or `--fuzzy` for the
older broad alias/global behavior.

`query callees [--entity-id <id>|--exact-resolved|--fuzzy] [--limit <n>] [--concise|--agent-json] <symbol>`

Returns `CALLS` edges emitted by the symbol. It uses the same exact,
ambiguous, and fuzzy modes as `query callers`.

`query chain <source> <target> [--limit <n>] [--concise|--agent-json]`

Runs cycle-safe call-chain recovery over `CALLS` edges, preserving exactness and
confidence labels.

`query unresolved-calls [--limit <n>] [--offset <n>] [--json] [--no-snippets|--include-snippets] [--db <path>]`

Lists retained unresolved calls labeled as `static_heuristic`. The exact DB path
used by this command is checked with the same lifecycle/passport preflight as
other read paths.

`query path <source> <target> [--limit <n>] [--concise|--agent-json]`

Runs exact graph path tracing with source spans and PathEvidence.

`impact <file-or-symbol>`

Returns blast-radius sections for calls, mutations/dataflow, DB/schema,
API/auth/security, events, and tests.

`context-pack --task <task> [--budget <tokens>] [--mode <production|test-impact|debug|impact>] [--seed <symbol>] [--stage0-candidate <id>] [--enable-vector-candidates] [--vector-index <path>] [--enable-nuance-rescue-candidates] [--agent-json|--concise|--explain|--audit-json] [--limit-paths <n>] [--limit-snippets <n>] [--max-output-bytes <n>]`

Builds a compact proof-oriented context packet from verified graph paths and
source snippets.

Optional candidate lanes:

- `--enable-vector-candidates --vector-index <path>` loads a matching vector
  chunk index built by `index --build-vector-index`.
- `--enable-nuance-rescue-candidates` enables deterministic rare-token,
  identifier, path/title, config, test-name, and no-extension-script candidate
  rescue.

These lanes add candidates to the union/ranking step. They remain
`graph_proof=false` until graph/source verification finds a proof path.
`--explain` exposes bounded funnel diagnostics; default agent JSON stays
compact.

Production mode excludes test, mock, mixed, and unknown evidence by default.
`test-impact` mode intentionally includes test/mock evidence and labels it.
Inline Rust `#[cfg(test)] mod tests` and `#[test]` functions are classified as
test evidence even when they live in `src/lib.rs`.

When no graph proof path is found, context-pack may still return bounded
source-text fallback evidence labeled `no_proof_path_found`. Planning packets
may include `follow_up_queries`; these are bounded query hints, not shell-ready
commands and not internally executed `rg` probes.

Read paths run the DB passport/preflight guard. If the configured DB is from a
different repo, stale scope, incompatible storage mode, failed run, corrupt
file, or unknown old format, the command refuses to answer unless an explicit
diagnostic stale-read override is used.

`agent-use validate-edit --repo <repo> --changed <path> [--changed <path>...] [--agent-json|--explain|--audit-json] [--fail-on-blocking]`

Runs the production-profile changed-file update and validation packet wrapper
for an explicit post-edit file set. The command resolves the external
`production-agent-use` DB, refuses direct `--db`, does not auto-index, does not
fall back to repo-local `.codegraph`, and does not edit source files. Default
validation results print JSON and exit 0. `--fail-on-blocking` exits 2 only
when `hard_interrupt_available=true`, while still printing the JSON packet.
Runtime/config/lifecycle failures that prevent validation are separate nonzero
failures.

`context --task <task> [--budget <tokens>] [--mode <mode>] [--seed <symbol>]`

Alias group for `context-pack`.

`bundle export --output repo.cgc-bundle`

Exports files, entities, and edges with a bundle manifest schema.

`bundle import repo.cgc-bundle [--replace|--merge]`

Imports a bundle if the schema version and repo identity are safe. The default
fresh mode refuses to write into a non-empty DB. `--replace` performs an atomic
replacement after validation. `--merge` is currently diagnostic-only and refuses
mutation until merged facts have a stronger provenance contract.

`watch [repo] [--db <path>] [--debounce-ms <ms>] [--once --changed <path>...]`

Watches or updates changed files only. Ignore rules cover `.git`,
`.codegraph`, dependency folders, build outputs, generated bundles, maps, lock
files, and minified JS. Persistent watch mode honors the configured DB path and
runs lifecycle preflight before opening it.

For production agent-use, prefer `agent-use watch --repo <repo> --once
--changed <path> --json` for deterministic updates, or `agent-use watch --repo
<repo> --json` for persistent scheduling over the same update primitive. That
wrapper owns the external production profile DB resolver and will not mutate
repo-local `.codegraph`.

`serve-mcp`

Starts the local stdio JSON-RPC MCP server.

`mcp`

Alias group for `serve-mcp`.

`serve-ui [repo] [--host 127.0.0.1] [--port 7878]`

Starts the loopback-only Proof-Path UI.

The local UI API includes `/api/path-graph`, `/api/symbol-search`,
`/api/source-span`, `/api/path-compare`, `/api/unresolved-calls`, `/api/impact`,
and `/api/context-pack`. Path graph JSON includes layout metadata, exactness
style hints, resource links, and guardrails for visible node caps and
truncation.

`ui [repo] [--host 127.0.0.1] [--port 7878]`

Alias group for `serve-ui`.

`languages [--json]`

Lists language frontends, extensions, support tiers, tree-sitter grammar
availability, optional compiler/LSP resolver availability, exactness per
extractor, and known limitations. Use `--json` for machine-readable capability
metadata.

## Developer / Diagnostic Commands

`bench [--baseline <mode>]... [--format <json|markdown>] [--output <path>]`

Runs the local developer benchmark suite. Baselines are `vanilla_no_retrieval`,
`grep_bm25`, `vector_only`, `graph_only`, `graph_binary_pq_funnel`,
`graph_bayesian_ranker`, and `full_context_packet`.

`bench synthetic-index --output-dir <dir> [--files <n>]`

Generates a large deterministic TypeScript fixture repo and indexes it with
profiling enabled. The command writes `synthetic-index-run.json` for indexing
speed regression checks.

`bench gaps [--output-dir <dir>] [--timeout-ms <ms>] [--top-k <k>] [--competitor-bin <path>]`

Writes a gap scoreboard with machine-readable win/loss/tie/unknown dimensions.
If the competitor executable is unavailable, the report records `skipped` with
a structured reason.

`bench real-repo-corpus`

Prints the real-repo maturity corpus for TypeScript, Python, Go, Rust, and
Java. It includes pinned commits, task manifests, and an offline replay plan for
`.codegraph-bench-cache/real-repos`.

`bench parity-report [--output-dir <dir>]`

Writes parity summaries. Unknown/skipped fields remain explicit, and diagnostic
outputs do not support superiority claims.

`bench cgc-comparison [--output-dir <dir>] [--timeout-ms <ms>] [--top-k <k>] [--competitor-bin <path>]`

Runs the optional external CodeGraphContext / CGC comparison harness. The
subcommand skips CGC with a structured reason when `CGC_COMPETITOR_BIN`, `cgc`,
and `codegraphcontext` are unavailable.

`trace append|replay|validate ...`

Appends replayable Agent/MCP JSONL trace events or replays/validates an
`events.jsonl` file. Trace files are evidence artifacts; keep them out of
public claims unless summarized.

`audit storage|schema-check|storage-experiments|sample-edges|sample-paths|relation-counts|label-samples|summarize-labels ...`

Runs read-only audit inspections over DBs and manual-label artifacts. Audit
outputs can support stable summaries, but raw audit DBs/logs are not final
benchmark artifacts by themselves.

`doctor [repo] [--json]`

Checks the local SQLite DB, language frontends, optional Node/TypeScript
resolver, `.codex/config.toml`, bundled UI assets, and `.codegraph`
permissions. DB inspection is read-only and lifecycle-aware. JSON output
includes passport status plus `sqlite_sidecars` and `sidecar_status`; normal
WAL/SHM files are not reported as orphaned unless the main DB is missing.

`config [show|completions|release-metadata] [--shell <powershell|bash|zsh|fish>]`

Prints local config defaults, shell completions, and release/install metadata.
The release metadata mirrors `dist/archive-manifest.json`, installer template
paths, feature flags, build profile, and provenance/checksum expectations.

## SQLite Tuning

The SQLite store enables `foreign_keys`, WAL mode for file-backed DBs,
`synchronous = FULL`, and a 5000ms busy timeout. These are documented because
they preserve local durability expectations without silently moving the database
to an unsafe mode.

## Installability

See `docs/install.md` for GitHub release archive names, PowerShell and shell
installer templates, cargo/cargo-binstall/Homebrew paths, and release metadata
dry-run commands.
