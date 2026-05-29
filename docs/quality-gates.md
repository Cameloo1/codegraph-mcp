# Quality Gates

The root `README.md` is the public setup contract. These gates keep release
work narrow, verifiable, and separate from local benchmark or OpenEvolve lab
artifacts.

## Release-Facing Gate Rules

- Public docs describe current verified behavior and stable command surfaces.
- Candidate, vector, text, source-navigation, benchmark, and optimizer output
  must not be described as graph proof.
- Generated DBs, logs, payloads, predictions, patches, run directories, and
  WAL/SHM files stay out of source commits unless explicitly promoted.
- Missing, stale, foreign, schema-mismatched, locked, timed-out, skipped, and
  diagnostic states stay labeled.
- Benchmark and OpenEvolve details belong on `benchmark-and-openevolve-lab`
  unless a concise, claim-reviewed note is intentionally promoted.

## Local Checks

For documentation-only changes:

```text
python scripts/check_readme_artifacts.py
python scripts/check_markdown_links.py
python scripts/check_docs_hygiene.py
git diff --check
```

For Rust, CLI, MCP, indexing, parser, storage, or query behavior changes:

```text
cargo fmt --check
cargo check --workspace
cargo test --workspace
codegraph-mcp --json --version
codegraph-mcp doctor --json
```

Use the release binary for behavior or performance measurements that are meant
to describe release behavior:

```powershell
cargo build --release --bin codegraph-mcp
.\target\release\codegraph-mcp.exe agent-use status --repo <repo> --json
```

## CI

The GitHub Actions workflow in `.github/workflows/ci.yml` runs on pull requests
and pushes to `main` or `master`. It covers Windows and Linux workspace
build/test smoke, `codegraph-mcp --help`, fixture index smoke, README artifact
validation, Markdown link validation, and Docker smoke when available.

`.github/workflows/release.yml` is a manual/tag packaging dry run. It builds the
release binary, validates release metadata and archive manifest JSON, runs the
shell installer dry run, and generates a checksum in a temporary
release-dry-run directory.

## Smoke Scope

The public smoke surface is intentionally smaller than the lab benchmark suite.
It is designed to catch packaging, docs, command-contract, and integration
regressions without requiring external corpora, competitor tools, model calls,
or large generated artifacts.

Broader local checks cover DB lifecycle behavior, context-pack fixtures,
watcher updates, bundle round trips, MCP schemas, release metadata, profile
JSON, warm-index skip behavior, deterministic parallel parse behavior, SQLite
pragma behavior, and packaging templates.

## Workflow Guardrail

CodeGraph keeps a linear, inspectable agent-context workflow. Internal Rust
parallelism may be used for indexing/query execution, but public outputs should
remain deterministic, labeled, and auditable.
