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
--repo <path>  --db <path>  --json  --no-color  --verbose  --quiet  --profile
```

`--repo` sets the working repository, `--db` overrides
`CODEGRAPH_DB_PATH`, and global `--profile` enables index profiling for the
`index` command. For routine agent use in this checkout, prefer the production
profile DB documented in [operational-profiles.md](operational-profiles.md)
instead of reusing development/self-test DBs.

## Commands

`init [repo] [--dry-run] [--with-codex-config] [--with-agents] [--with-skills] [--with-hooks] [--with-templates] [--index]`

Detects repo tooling, creates `.codegraph/`, and can install Codex config,
`AGENTS.md`, skill templates, hook templates, and an initial index.

`index <repo> [--db <path>] [--fresh|--rebuild] [--incremental] [--fail-on-db-problem] [--allow-stale-reuse] [--profile] [--json]`

Indexes supported language frontends into `.codegraph/codegraph.sqlite`.
Unchanged files are skipped by content hash. Changed files are parsed and
extracted through a deterministic parallel worker pool, then written in a
single batched SQLite transaction. `--profile --json` includes discovery,
parse, extraction, semantic resolver, DB write, FTS/search index, signature,
total wall time, throughput, worker count, unchanged skip count, and memory
when measurable.

DB lifecycle flags:

- `--fresh` / `--rebuild` always builds a fresh replacement and publishes it
  only after validation.
- `--incremental` requires a reusable passported DB and fails if reuse is
  unsafe.
- `--fail-on-db-problem` fails instead of safe-auto rebuilding.
- `--allow-stale-reuse` is diagnostic only; output must be labeled
  contaminated and not claimable.

Scope flags include `--include-ignored`, `--include <pattern>`,
`--exclude <pattern>`, `--no-default-excludes`,
`--respect-gitignore <true|false>`, `--explain-scope`, `--print-included`, and
`--print-excluded`. A DB built with non-default scope records that scope in the
passport, and read paths validate against it.

`status [repo]`

Reports schema version, file/entity/edge counts, detected tooling, and whether
the index exists.

`query symbols <query>`

Ranks symbols across simple names, qualified names, file paths, namespaces,
doc/signature metadata, alias/import names, identifier tokens, and
relation-neighbor text. Exact and qualified matches outrank fuzzy matches.

`query text <query>`

Searches the local SQLite FTS/BM25 index across files, entities, and snippets.

`query files <query>`

Finds repo-relative files by FTS and path proximity.

`query references <symbol>`

Lists graph edges connected to a resolved symbol or explicit unresolved
same-name placeholders.

`query definitions <symbol>`

Returns declaration/executable symbol hits.

`query callers <symbol>`

Returns `CALLS` edges whose callee resolves to the symbol.

`query callees <symbol>`

Returns `CALLS` edges emitted by the symbol.

`query chain <source> <target>`

Runs cycle-safe call-chain recovery over `CALLS` edges, preserving exactness and
confidence labels.

`query unresolved-calls`

Lists retained unresolved calls labeled as `static_heuristic`.

`query path <source> <target>`

Runs exact graph path tracing with source spans and PathEvidence.

`impact <file-or-symbol>`

Returns blast-radius sections for calls, mutations/dataflow, DB/schema,
API/auth/security, events, and tests.

`context-pack --task <task> [--budget <tokens>] [--mode <mode>] [--seed <symbol>] [--stage0-candidate <id>]`

Builds a compact proof-oriented context packet from verified graph paths and
source snippets.

Read paths run the DB passport/preflight guard. If the configured DB is from a
different repo, stale scope, incompatible storage mode, failed run, corrupt
file, or unknown old format, the command refuses to answer unless an explicit
diagnostic stale-read override is used.

`context --task <task> [--budget <tokens>] [--mode <mode>] [--seed <symbol>]`

Alias group for `context-pack`.

`bundle export --output repo.cgc-bundle`

Exports files, entities, and edges with a bundle manifest schema.

`bundle import repo.cgc-bundle`

Imports a bundle if the schema version matches.

`watch [repo] [--debounce-ms <ms>] [--once --changed <path>...]`

Watches or updates changed files only. Ignore rules cover `.git`,
`.codegraph`, dependency folders, build outputs, generated bundles, maps, lock
files, and minified JS.

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

Writes the Phase 26 gap scoreboard with machine-readable win/loss/tie/unknown
dimensions and nested CodeGraphContext artifacts. If the competitor executable
is unavailable, the report records `skipped` with a structured reason.

`bench real-repo-corpus`

Prints the real-repo maturity corpus for TypeScript, Python, Go, Rust, and
Java. It includes pinned commits, task manifests, and an offline replay plan for
`.codegraph-bench-cache/real-repos`.

`bench parity-report [--output-dir <dir>]`

Writes the final parity artifacts: `summary.json`, `summary.md`, and
`per_task.jsonl`. Unknown/skipped fields remain explicit, and the report makes no
SOTA claim without measured evidence.

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
permissions. Missing optional components are warnings, not fatal errors.

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
