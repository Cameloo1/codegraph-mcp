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
codegraph-mcp bench synthetic-index --output-dir target\index-speed --files 250
codegraph-mcp bench gaps --output-dir target\gap-scoreboard --competitor-bin target\missing-cgc.exe
codegraph-mcp bench real-repo-corpus
codegraph-mcp bench parity-report --output-dir target\parity
```

For production threshold timing, build and run the release binary. Debug timing
may be kept as diagnostic evidence, but it must be marked non-claimable.

```powershell
cargo build --release --bin codegraph-mcp
.\target\release\codegraph-mcp.exe bench proof-build-only --repo <repo> --db <db> --workers 16
```

## CI

The GitHub Actions workflow in `.github/workflows/ci.yml` runs on pull requests
and pushes to `main` or `master`. It currently checks Windows and Linux
workspace build/test smoke, `codegraph-mcp --help`, fixture index smoke,
README artifact validation, Markdown link validation, and a Docker smoke job.
It does not currently run formatting, Clippy, all-feature compilation, or
synthetic indexing-speed gates.

`.github/workflows/release.yml` is a manual/tag packaging dry run. It builds the
release binary, validates release metadata and archive manifest JSON, runs the
shell installer dry run, and generates a checksum in a temporary release-dry-run
directory.

## Smoke Scope

The current smoke surface covers workspace build/test, help output, fixture
indexing, README asset validation, Markdown links, and Docker smoke when a
daemon is available. Broader checks cover DB lifecycle behavior, context-pack
fixtures, watcher updates, bundle round trips, MCP schemas, release metadata,
and benchmark report schema generation.

The smoke scope is intentionally smaller than the full benchmark suite. It is
designed to catch packaging and integration regressions without requiring CGC,
Autoresearch, network access, or large generated artifacts.

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
codegraph-mcp bench --output target\benchmark-report.json
codegraph-mcp bench synthetic-index --output-dir target\index-speed --files 250
codegraph-mcp bench gaps --output-dir target\gap-scoreboard --competitor-bin target\missing-cgc.exe
codegraph-mcp bench real-repo-corpus
codegraph-mcp bench parity-report --output-dir target\parity
codegraph-mcp bench cgc-comparison --output-dir target\cgc-comparison --competitor-bin target\missing-cgc.exe
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

CodeGraph keeps a linear, inspectable agent-context workflow. Internal Rust
parallelism may be used for indexing/query execution, but public outputs should
remain deterministic, labeled, and auditable.
