# Local Footprint And Timing

This page records local footprint and cold-vs-warm timing examples for
`agent-use`. It is meant to help users estimate the shape of a normal local
setup. These measurements are not public benchmark claims, not performance
guarantees, and not comparisons against other tools.

All storage numbers below are shown in MB. Raw byte counts are omitted from the
public table because the useful question is the practical size class, not exact
filesystem accounting.

## What Was Measured

Each run used the production `agent-use` profile with an explicit external data
root. No repo-local `.codegraph` directory was created or mutated.

Measured commands:

```powershell
codegraph-mcp agent-use index --repo <repo> --json
codegraph-mcp agent-use status --repo <repo> --json
codegraph-mcp agent-use query symbols <symbol> --repo <repo> --limit 5 --agent-json
codegraph-mcp agent-use context-pack --repo <repo> --task "<task>" --seed <symbol> --agent-json
```

Two timing layers matter:

- **Process wall time:** the whole CLI invocation, including process startup,
  DB open, lifecycle checks, JSON serialization, and exit.
- **Read-path time:** the internal read-path timing reported in the agent JSON.

For README-level expectations, process wall time is the more honest user-facing
number.

## Summary

| Repo / fixture | Source footprint | Files seen / indexed | Graph DB | Sidecars | Cold index wall time | Warm status / query / context |
|---|---:|---:|---:|---:|---:|---:|
| Smoke fixture | 0.52 MB | 3 / 2 | 0.55 MB | 0.29 MB | 1.07 s | 0.36 s / 0.44 s / 0.35 s |
| codegraph-mcp clean worktree | 11.80 MB | 263 / 181 | 53.11 MB | 18.63 MB | 71.35 s | 0.79 s / 1.49 s / 1.87 s |

Warm timings are p50 process wall time from three repeated calls after the DB
already existed.

## Cold Vs Warm Behavior

Cold indexing is the expensive step because it walks the repo, applies scope
rules, parses supported source files, extracts graph facts and source spans,
writes SQLite state, and builds bounded candidate sidecars.

Warm `agent-use` reads are different. They reuse the existing external profile
DB, run lifecycle/passport checks, perform bounded DB lookups, and emit compact
agent JSON.

That distinction is why the clean `codegraph-mcp` worktree took about 71 s to
index cold, while warm status/query/context-pack calls were around 1-2 s at the
whole-process level.

## Detailed Metrics

### Smoke fixture

| Metric | Value |
|---|---:|
| Source footprint | 0.52 MB |
| Source files in fixture | 6 |
| Files seen / indexed | 3 / 2 |
| Source spans | 40 |
| Graph DB | 0.55 MB |
| Candidate spool | 0.02 MB |
| Candidate-spool query index | 0.25 MB |
| Runtime vector sidecar | 0.02 MB |
| External agent-use data total | 0.88 MB |
| Cold index process wall time | 1.07 s |
| Cold index reported wall time | 0.83 s |
| Warm status p50 process / read-path | 0.36 s / 0.35 s |
| Warm query p50 process / read-path | 0.44 s / 0.17 s |
| Warm context-pack p50 process / read-path | 0.35 s / 0.18 s |

### codegraph-mcp clean worktree

| Metric | Value |
|---|---:|
| Source footprint | 11.80 MB |
| Tracked files | 289 |
| Files seen / indexed | 263 / 181 |
| Source spans | 136,997 |
| Graph DB | 53.11 MB |
| Candidate spool | 1.05 MB |
| Candidate-spool query index | 11.16 MB |
| Runtime vector sidecar | 6.42 MB |
| External agent-use data total | 71.77 MB |
| Cold index process wall time | 71.35 s |
| Cold index reported wall time | 70.51 s |
| Warm status p50 process / read-path | 0.79 s / 0.77 s |
| Warm query p50 process / read-path | 1.49 s / 0.32 s |
| Warm context-pack p50 process / read-path | 1.87 s / 1.05 s |

## What The Sidecars Mean

The graph DB is the proof-bearing SQLite artifact. Sidecars are bounded support
artifacts:

- candidate spool: candidate-only context packets created during indexing;
- candidate-spool query index: SQLite index over candidate packets;
- runtime vector sidecar: deterministic local candidate chunks for recall help.

Candidate and vector sidecars are not graph proof. They can route attention, but
proof still comes from graph/source verification.

## Reproducing A Local Run

Use a clean checkout and a disposable external data root:

```powershell
$env:CODEGRAPH_AGENT_USE_DATA_ROOT = "C:\tmp\codegraph-footprint"
cargo build --release --bin codegraph-mcp
codegraph-mcp agent-use index --repo <repo> --json
codegraph-mcp agent-use status --repo <repo> --json
codegraph-mcp agent-use query symbols <symbol> --repo <repo> --limit 5 --agent-json
codegraph-mcp agent-use context-pack --repo <repo> --task "<task>" --seed <symbol> --agent-json
```

Measure cold indexing separately from warm reads. For warm reads, run the
status/query/context commands multiple times after the DB exists and report a
median or p50.

## Caveats

- These numbers came from a local Windows run and should be treated as a size
  and timing example, not as a benchmark result.
- A fresh release rebuild was blocked in this session by Windows Application
  Control / WDAC on a generated build-script executable. The measurements used
  an existing runnable release binary from the local `fix` worktree.
- Repo shape matters. DB size and index time depend on language mix, number of
  supported source files, relation density, source spans, ignored paths, and
  optional sidecar settings.
- The current local vector lane is deterministic/token-based and candidate-only.
  It is not learned production semantic quality and is not graph proof.
