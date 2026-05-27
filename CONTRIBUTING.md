# Contributing

CodeGraph is a local, proof-oriented context layer for coding agents. The
project accepts small, inspectable changes that preserve the normal developer
workflow and keep evidence boundaries sharp.

## Branch Lanes

Use the release branch for product code, stable public docs, install paths, CLI
contracts, MCP contracts, and tests that protect shipped behavior.

Use `benchmark-and-openevolve-lab` for benchmark harness work, OpenEvolve policy
experiments, SWE-bench diagnostics, raw run interpretation, generated charts,
and local evidence exploration.

Do not merge lab outputs wholesale into release work. Promote only small,
reviewed source/docs changes that have been replayed and verified.

## Evidence Rules

- Graph/source verification is the only graph-proof path.
- Text, source-navigation, vector, binary, nuance, and candidate-spool evidence
  can guide inspection, but they are not graph proof by themselves.
- Optimizer output, benchmark output, and docs text are not proof.
- Unknown, skipped, stale, blocked, diagnostic, or timeout states must stay
  labeled that way.

## Artifact Hygiene

Never stage generated or local-only payloads unless a maintainer explicitly asks
for that exact artifact:

- SQLite DBs, WAL/SHM files, vector sidecars, candidate spools, and raw indexes.
- benchmark payloads, predictions, patches, logs, Docker outputs, and run dirs.
- `target/`, dependency caches, local virtual environments, and temporary
  reports.
- machine-local paths, secrets, `.env.local`, API keys, or private checkout
  details.

Stable public docs should describe current verified behavior, not local scratch
state. If a diagnostic result needs to be preserved, promote a concise summary
and keep the raw evidence local or on the lab branch.

## Code Changes

- Prefer existing primitives and local helper APIs.
- Keep failure, retry, skip, abort, and recovery states explicit.
- Preserve DB lifecycle/passport checks and old-good DB behavior.
- Keep source editing outside MCP tools.
- Update CLI/MCP docs whenever command contracts, flags, schemas, or safety
  labels change.

## Documentation Changes

Public docs should be product-first:

- explain how to build, index, query, serve MCP, and use `agent-use`;
- preserve proof boundaries;
- avoid local benchmark scoreboards and raw report paths;
- point benchmark and OpenEvolve details to `benchmark-and-openevolve-lab`.

Do not add public claims that CodeGraph beats another tool or benchmark unless
there is an intentionally promoted, reproducible, claim-reviewed report.

## Local Checks

For docs-only changes:

```powershell
python scripts/check_readme_artifacts.py
python scripts/check_markdown_links.py
python scripts/check_docs_hygiene.py
git diff --check
```

For Rust or CLI/MCP behavior changes:

```powershell
cargo fmt --check
cargo check --workspace
cargo test --workspace
```

Use narrower targeted tests when the full suite is not practical, and state
what was not run.

## Before Commit Or Push

Check the staged set and excluded artifacts:

```powershell
git status --short
git diff --name-only --cached
git status --ignored --short
```

Confirm no generated DBs, raw logs, benchmark payloads, predictions, patches,
secrets, local run directories, or normal `.codegraph` state are staged.

## Related Docs

- [Guardrails](docs/guardrails.md) for proof boundaries and claim discipline.
- [Quality Gates](docs/quality-gates.md) for release-facing validation checks.
- [Agent Benchmarking](docs/agent-benchmarking.md) for benchmark claim
  boundaries.
- [Benchmark Guide](docs/benchmark-guide.md) for benchmark/lab contributors.
