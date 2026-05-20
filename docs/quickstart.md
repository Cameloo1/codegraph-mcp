# Quickstart

CodeGraph is Rust-first, local-first, exact graph first, vectors second, and
single-agent only.

## Build

```powershell
cargo build --workspace
cargo test --workspace
```

## Initialize A Repo

From a TypeScript or JavaScript repository:

```powershell
codegraph-mcp init --with-templates --with-codex-config
codegraph-mcp index .
codegraph-mcp status
```

This creates `.codegraph/codegraph.sqlite`, optional Codex templates, and a
local MCP config when requested.

## Query Evidence

```powershell
codegraph-mcp query symbols profileRoute
codegraph-mcp query callers saveProfile
codegraph-mcp query callees profileRoute
codegraph-mcp query chain profileRoute saveProfile
codegraph-mcp query path profileRoute saveProfile
codegraph-mcp impact profileRoute
codegraph-mcp context-pack --task "Trace profileRoute auth and mutation impact" --seed profileRoute --budget 1600
```

The CLI returns JSON with graph/source evidence, source spans, exactness, and
confidence labels.

For a coding-agent loop, use the release binary, an explicit DB path, and
bounded agent JSON:

```powershell
cargo build --release --bin codegraph-mcp
codegraph-mcp --repo <repo> --db <agent-db> index <repo> --json
codegraph-mcp --repo <repo> --db <agent-db> query symbols profileRoute `
  --limit 5 --agent-json
codegraph-mcp --repo <repo> --db <agent-db> context-pack `
  --task "Trace profileRoute auth and mutation impact" `
  --seed profileRoute `
  --mode production `
  --limit-paths 5 `
  --limit-snippets 5 `
  --agent-json
```

Use `--mode test-impact --agent-json` when the task intentionally needs
test/mock evidence. Production context excludes test/mock/mixed/unknown
evidence by default.

Optional vector and nuance-rescue lanes can improve candidate recall for hard
tasks, but they remain candidates until graph/source verification succeeds.

## Serve MCP

```powershell
codegraph-mcp serve-mcp
```

Generated Codex config points to this command. MCP tools are read-mostly and
proof-oriented.

## Watch Changes

```powershell
codegraph-mcp watch . --debounce-ms 250
```

Watcher mode re-indexes changed files only, prunes stale facts for those files,
updates binary signatures, and refreshes local adjacency state.

## Proof-Path UI

```powershell
codegraph-mcp serve-ui --port 7878
```

Open `http://127.0.0.1:7878`. The UI is loopback-only by default and uses the
local `.codegraph/codegraph.sqlite` store. It supports proof path,
neighborhood, impact, auth/security, event flow, test impact, and unresolved
call views with exactness legends, source-span preview, graph JSON export, and
truncation warnings for large graphs.

## Benchmarks

```powershell
codegraph-mcp bench --output target\codegraph-benchmark-report.json
codegraph-mcp bench --baseline graph-only --format markdown --output target\graph-only.md
codegraph-mcp bench real-repo-corpus
codegraph-mcp bench parity-report --output-dir target\parity
```

Benchmark reports are deterministic and machine-readable. They compare vanilla,
BM25, vector-only, graph-only, graph+binary/PQ, graph+Bayesian, and full
context-packet modes. The parity report keeps CodeGraphContext and real-repo
results as skipped or unknown unless they were actually measured.
