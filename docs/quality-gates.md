# Quality Gates

The root `README.md` is the public setup contract. These gates keep changes
narrow and verifiable.

## Current Published Status

The stable reports are the current public gate surface:

- `reports/final/comprehensive_benchmark_latest.md` / `.json`: latest
  preserved comprehensive gate.
- `reports/final/intended_tool_quality_gate.md` / `.json`: Intended Tool
  Quality Gate.
- `reports/final/manual_relation_precision.md` / `.json`: manual sampled
  precision boundary.
- `reports/comparison/codegraph_vs_cgc_latest.md` / `.json`: CGC comparison
  status.

Current summary:

- Graph Truth Gate: 11/11 pass.
- Context Packet Gate: 11/11 pass.
- DB integrity: ok.
- Proof DB size: 171.184 MiB against a 250 MiB target.
- Repeat unchanged index: 1674 ms.
- Single-file update: 336 ms.
- Intended Tool Quality Gate: **FAIL** because the stable report records
  `proof_build_only_ms = 184,297 ms` against `<=60,000 ms`.
- CGC comparison: diagnostic/incomplete; no superiority claim.

Do not use raw benchmark payloads, partial CGC artifacts, debug timing, or
fake-agent dry runs as green gate evidence.

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

For production threshold timing, build and run the release binary. Debug timing
may be kept as diagnostic evidence, but it must be marked non-claimable.

```powershell
cargo build --release --bin codegraph-mcp
.\target\release\codegraph-mcp.exe bench proof-build-only --repo <repo> --db <db> --workers 16
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

Documentation-only changes can usually use the lighter gate:

```text
python scripts/check_readme_artifacts.py
python scripts/check_markdown_links.py
git diff --check
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
