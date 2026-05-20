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
routine agent use, prefer a release-binary agent DB outside the source tree
instead of reusing temporary development or benchmark DBs.

Command-local flags remain after the command. Ambiguous global flags after
query subcommands return targeted corrections instead of becoming query text.
For example, `query symbols greet --db <db>` fails with guidance to put `--db`
before `query`. Use `--` for literal flag-shaped search terms, such as
`query text --agent-json -- --db`.

## Agent-Friendly Output

Use `--agent-json` for tight coding-agent loops and `--limit <n>` to keep
results bounded:

```powershell
codegraph-mcp --repo <repo> --db <agent-db> query symbols <symbol> `
  --limit 5 --agent-json

codegraph-mcp --repo <repo> --db <agent-db> context-pack `
  --task "Trace the change impact" `
  --seed <symbol> `
  --mode production `
  --limit-paths 5 `
  --limit-snippets 5 `
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

## Commands

`init [repo] [--dry-run] [--with-codex-config] [--with-agents] [--with-skills] [--with-hooks] [--with-templates] [--index]`

Detects repo tooling, creates `.codegraph/`, and can install Codex config,
`AGENTS.md`, skill templates, hook templates, and an initial index.

`index <repo> [--db <path>] [--fresh|--rebuild] [--incremental] [--fail-on-db-problem] [--allow-stale-reuse] [--build-vector-index <path>] [--profile] [--json|--agent-json|--concise|--audit-json] [--verbose]`

Indexes supported language frontends into `.codegraph/codegraph.sqlite`.
Unchanged files are skipped by content hash. Changed files are parsed and
extracted through a deterministic parallel worker pool, then written in a
single batched SQLite transaction. `--profile --json` includes discovery,
parse, extraction, semantic resolver, DB write, FTS/search index, signature,
total wall time, throughput, worker count, unchanged skip count, and memory
when measurable.

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

`query references <symbol>`

Lists graph edges connected to a resolved symbol or explicit unresolved
same-name placeholders.

`query definitions <symbol>`

Returns declaration/executable symbol hits.

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

`query chain <source> <target>`

Runs cycle-safe call-chain recovery over `CALLS` edges, preserving exactness and
confidence labels.

`query unresolved-calls [--limit <n>] [--offset <n>] [--json] [--no-snippets|--include-snippets] [--db <path>]`

Lists retained unresolved calls labeled as `static_heuristic`. The exact DB path
used by this command is checked with the same lifecycle/passport preflight as
other read paths.

`query path <source> <target>`

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

`bench [--baseline <mode>]... [--format <json|markdown>] [--output <path>]`

Runs the local benchmark suite. Baselines are `vanilla_no_retrieval`,
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

Writes parity summaries. Unknown/skipped fields remain explicit, and the report
makes no SOTA claim without measured evidence.

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
`synchronous = NORMAL`, and a 5000ms busy timeout. These are documented because
they improve local indexing throughput without silently moving the database to
an unsafe durability mode.

## Installability

See `docs/install.md` for GitHub release archive names, PowerShell and shell
installer templates, cargo/cargo-binstall/Homebrew paths, and release metadata
dry-run commands.
