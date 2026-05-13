# Quality Gates

The root `README.md` is the public setup contract. These gates keep changes
narrow and verifiable.

## Local Checks

Run these before handing off a phase:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --workspace --all-features
codegraph-mcp --json --version
codegraph-mcp doctor --json
codegraph-mcp bench synthetic-index --output-dir target\phase30-index-speed --files 250
codegraph-mcp bench gaps --output-dir target\phase26-gaps --competitor-bin target\missing-cgc.exe
codegraph-mcp bench real-repo-corpus
codegraph-mcp bench parity-report --output-dir target\phase30-parity
```

## CI

The GitHub Actions workflow in `.github/workflows/ci.yml` runs formatting,
Clippy, tests, all-feature compilation, release metadata validation, and a
small synthetic indexing-speed dry run. `.github/workflows/release.yml` is a
packaging dry-run template for release archives, checksums, installer dry runs,
and provenance placeholders. Feature flags are placeholders only and do not
enable optional backend dependencies yet.

## CLI Smoke Scope

The `codegraph-mcp` binary exists from Phase 01. As of Phase 21, `index <repo>`
performs TS/JS parsing, SQLite persistence, basic declaration extraction, core
static relation extraction, and heuristic auth/security/event/db/test relation
extraction. It also populates the Stage 0 FTS index for files, entities, and
snippets. `codegraph-query` can answer exact graph questions in-process and
build graph-only context packets with exact prompt seed extraction.
`codegraph-vector` can run the Stage 1 local binary sieve and Stage 2
deterministic compressed reranker in-process. `codegraph-query` can orchestrate
the integrated runtime funnel in-process and emits structured kept/dropped
trace output, then apply deterministic Bayesian ranking and uncertainty
metadata after graph verification. `codegraph-store` can persist retrieval
traces in SQLite. `serve-mcp` starts the local read-mostly MCP server and
exposes proof-oriented `codegraph.*` tools for Codex. The direct CLI now
supports init, status, symbol/path query, context-pack, impact dashboard,
bundle export/import, optional `watch` mode, `serve-ui`, and `bench`. `init
--with-templates` now generates `AGENTS.md`, `.codex/skills`, and
`.codex/hooks` from checked-in templates. Watch mode uses `notify` and
localized changed-file re-indexing; it is covered by debounce, ignore-pattern,
stale-pruning, and binary-signature tests. `serve-ui` is covered by server
startup, path graph JSON, relation filter, exactness style, source-span preview,
large-graph truncation, symbol search, and context packet preview tests.
`serve-mcp` is covered by tool schema, output schema, pagination, resource,
prompt, resource-link, and explain-missing tests. `codegraph-bench` is covered
by schema validation, synthetic repo generation, metric calculation, baseline
runner smoke tests, real-repo corpus validation, offline replay skip tests, CGC
skipped-run/report tests, and final parity artifact output tests. The CLI still
does not expose vector tuning commands or Bayesian controls.

## Acceptance Commands

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --workspace --all-features
codegraph-mcp --json --version
codegraph-mcp index . --profile --json
codegraph-mcp doctor --json
codegraph-mcp config release-metadata --json
codegraph-mcp bench --output target\phase30-benchmark-report.json
codegraph-mcp bench synthetic-index --output-dir target\phase30-index-speed --files 250
codegraph-mcp bench gaps --output-dir target\phase26-gaps --competitor-bin target\missing-cgc.exe
codegraph-mcp bench real-repo-corpus
codegraph-mcp bench parity-report --output-dir target\phase30-parity
codegraph-mcp bench cgc-comparison --output-dir target\phase30-cgc-comparison --competitor-bin target\missing-cgc.exe
powershell -NoProfile -ExecutionPolicy Bypass -File install\install.ps1 -DryRun
sh install/install.sh --dry-run
```

Targeted acceptance coverage also includes CLI fixture tests, MCP fixture
tests, context-pack fixture tests, bundle round-trip tests, watcher integration
tests, benchmark smoke tests, external CGC skipped-run/report tests, UI smoke
tests, UI graph guardrail tests, MCP schema/resource/prompt tests, real-repo
manifest validation, final parity report schema tests, profile JSON tests,
warm-index skip tests, deterministic parallel parse tests, SQLite pragma tests,
release metadata tests, and packaging-template checks.

## Workflow Guardrail

No subagents. CodeGraph keeps one linear Codex-style agent workflow. Internal
Rust parallelism may arrive later for indexing/query execution, but the product
and project prompts remain single-agent only.
