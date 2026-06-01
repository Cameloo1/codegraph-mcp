# Agent Use

Use this profile when a coding agent needs compact, proof-grounded CodeGraph
context while working on a real repository. CodeGraph is not a replacement for
normal search, editing, or tests; it is the verified repo-context layer beside
those tools.

## Recommended Pattern

Build or install the release binary, keep the DB outside the source tree, and
ask for bounded agent JSON:

```powershell
cargo build --release --bin codegraph-mcp

codegraph-mcp agent-use status --repo <repo> --json

codegraph-mcp agent-use index --repo <repo> --json

codegraph-mcp agent-use watch --repo <repo> `
  --once --changed src\file.ts `
  --json

codegraph-mcp agent-use watch --repo <repo> --json

codegraph-mcp agent-use query symbols <symbol> --repo <repo> `
  --limit 5 --agent-json

codegraph-mcp agent-use query text "token or phrase" --repo <repo> `
  --limit 5 --agent-json

codegraph-mcp agent-use query files service --repo <repo> `
  --limit 5 --agent-json

codegraph-mcp agent-use query callers handle_request --repo <repo> `
  --limit 5 --agent-json

codegraph-mcp agent-use query callees handle_request --repo <repo> `
  --limit 5 --agent-json

codegraph-mcp agent-use query path handle_request save_record --repo <repo> `
  --limit 3 --agent-json

codegraph-mcp agent-use context-pack --repo <repo> `
  --task "Trace the change impact" `
  --agent-json

codegraph-mcp agent-use mcp-config --repo <repo> --json
```

The first-class `agent-use` namespace resolves the `production-agent-use`
profile to an external DB path under the platform data directory, for example
LocalAppData on Windows. It is collision-safe for repos with the same basename
and does not use normal repo-local `.codegraph` paths.

`agent-use status` is strictly read-only: it does not create the DB, the profile
parent directory, WAL/SHM files, or repo-local `.codegraph`. `agent-use index`
is the first mutating command and writes only to the external production profile
DB and its bounded profile artifacts. `agent-use query` and `agent-use
context-pack` use that same external DB and refuse unsafe DB states instead of
falling back to `.codegraph`. `agent-use mcp-config` emits config JSON only by
default; it does not write a config file.

`agent-use query` supports the compact symbol, text, file, caller, callee,
path, chain, reference, definition, and unresolved-call read surfaces when the
underlying plain query supports them. Relation/navigation output carries
relation kind, exactness, source spans, evidence role, `proof_status`, and
`proof_strength`. `graph_proof=true` is reserved for verified graph/source
relations or proof paths; definitions are symbol-location evidence, and
references distinguish graph references from text references.

Real-Time Delta Sync now has two production-profile surfaces. `agent-use watch
--once --changed <path>` is the deterministic changed-file primitive. It
updates the same external production profile DB only when an existing graph DB
is safe to write. It rejects `--db`, does not auto-index a missing/stale DB, and
does not fall back to repo-local `.codegraph`. Persistent `agent-use watch
--repo <repo> --json` is scheduling over that same once primitive: it debounces
editor save bursts, coalesces changed paths, serializes writes, retries
transient locks within bounds, exposes queue depth and last-update state, and
refuses missing/stale/foreign/schema-mismatched DBs unless the profile is
explicitly indexed first.

RTDS output includes `watch_db`, `delta_sync_phase`, `delta_sync_state`,
publish-safety labels, queue/update summaries where applicable, and staged
availability for graph, candidate-spool, vector-runtime, routing/context, and
audit layers. Candidate/vector sidecars remain candidate-only and may be
reported stale after a changed-file graph update; rebuild with `agent-use
index` when those sidecars need to be refreshed.

`agent-use validate-edit` is intentionally deferred to MVP3 expansion. Current
RTDS freshness packets can expose changed/removed symbols and exact graph
freshness, but they do not claim to replace compilers, tests, or a complete
dangling-edge validator.

The release binary and separate DB keep routine agent reads away from
development, lab, and temporary self-test artifacts. These outputs are usable
coding-agent context, not public metric verdicts by themselves.

Optional candidate recall for harder tasks:

```powershell
codegraph-mcp agent-use index --repo <repo> --json
codegraph-mcp agent-use context-pack --repo <repo> `
  --task "Find rare config/test/name references" `
  --agent-json
```

The production profile builds bounded candidate spool and runtime vector
sidecar artifacts where supported. They can help find plausible files, symbols,
rare identifiers, config keys, route literals, test names, and no-extension
scripts, but they do not become graph proof until graph/source verification
succeeds. Optional vector audit artifacts are diagnostic-only and are not
runtime proof sources.

## Output Modes

- `--agent-json` emits a bounded, schema-versioned JSON envelope for tight
  coding-agent loops. It includes compact lifecycle state, claim flags,
  result counts, truncation fields, warnings/errors, timings, and top results.
- `--concise` emits compact human/machine output where supported without the
  full audit payload.
- `--verbose`, `--debug`, `--profile`, and audit/report commands preserve rich
  diagnostics when explicitly requested.
- `index --json` is concise by default. Use `--explain-scope`,
  `--print-included`, `--print-excluded`, `--verbose`, or `--audit-json` when
  you need full scope examples or audit-grade index detail.

The public agent JSON schemas live under `docs/schemas/agent-json/`, with the
versioning policy in [agent-json.md](agent-json.md).

Telemetry fields distinguish measured, unknown, and aggregated values. Memory
is reported as `memory: "unknown"` with `memory_measured: false` unless it is
actually measured. Timing substages that cannot be separated yet are labeled as
unknown or aggregated rather than presented as precise measurements.

## Graph Verification Diagnostics

Candidates are retrieval inputs, not proof. Exact, text, lexical, vector,
binary, and nuance-rescue candidates only become graph proof after graph/source
verification returns a proof path. Text evidence can support source-text
existence, but it does not prove typed graph relations by itself.

Default `context-pack --agent-json` output stays compact. It reports
proof/no-proof status, claimability, short evidence summaries, and omitted
counts without dumping traversal traces. Add `--explain` when you need the
diagnostic trace for a bounded graph walk:

```powershell
codegraph-mcp --repo <repo> --db <agent-db> context-pack `
  --task "Trace the change impact" `
  --seed <symbol> `
  --mode production `
  --agent-json `
  --explain
```

Explain output includes traversal mode, relation allowlist, source-role filter,
candidate sources that requested graph verification, traversal budgets, visited
edge/node counts, cycle and structural-skip counters, budget stop reason, no
proof fallback reason, and PathEvidence lookup/hydration timings.

Traversal budgets include max depth, max paths, max edge visits,
max-neighbors-per-node, candidate caps, structural expansion caps, and timeout
metadata where available. Relation modes keep the walk scoped:
`production` is the default proof path mode, `test-impact` intentionally admits
test/mock paths and labels them, and debug/audit modes may expose more
diagnostic detail while remaining bounded.

PathEvidence lookup and hydration are bounded. Truncation and omission labels
such as `path_evidence_truncated`, `path_evidence_omitted_count`,
`hydration_budget_exhausted`, `source_snippet_omitted_count`, and the compact
`omitted_count` fields mean evidence was capped or omitted, not silently
promoted.

When graph verification cannot prove a path, output must remain
`no_proof_path_found` and may return claimable source/text fallback evidence.
That fallback is not typed graph proof.

Planning packets may include bounded `follow_up_queries` when the packet can
orient the next inspection step. They are query hints, not shell-ready commands
and not internally executed `rg` probes.

## CLI Flag Placement

Plain commands still support explicit global flags for lower-level and
diagnostic workflows. Put those global flags before the command:

```powershell
codegraph-mcp --repo <repo> --db <agent-db> query symbols <symbol> --agent-json
```

For routine production agent use, prefer the first-class `agent-use` commands
above. Plain `status --json` remains local `.codegraph` status and does not
silently redirect to the production profile.

Command-local flags stay after the command. For example, query `--json`,
`--agent-json`, `--concise`, `--limit`, and context-pack limits are local.

Ambiguous command-tail global flags return a targeted correction instead of
being swallowed as query text. For example, `query symbols greet --db <db>`
fails with guidance to place `--db` before `query`.

To search for a literal flag-like term, use the standard `--` escape:

```powershell
codegraph-mcp --repo <repo> --db <agent-db> query text --agent-json -- --db
```

## Evidence Roles

Context and proof evidence is labeled with one of:

- `production`
- `test`
- `mock`
- `stub`
- `generated`
- `mixed`
- `text_evidence`
- `unknown`

Default production context excludes test, mock, stub, generated, mixed,
text-evidence, and unknown evidence unless a production-only subpath can be
split safely. `test-impact` mode
intentionally includes test/mock evidence and labels it clearly.

Inline Rust tests in `src/lib.rs`, including `#[cfg(test)] mod tests` and
`#[test]` functions, are classified as test evidence. Production context-pack
output excludes them by default.

## Claim Boundaries

- Absent proof-mode relations have no precision claim.
- Candidate, vector, text, and source-navigation evidence are not graph proof.
- Local diagnostic metrics do not become public product claims unless they are
  intentionally promoted and claim-reviewed.

## Related Docs

- [Operational Profiles](operational-profiles.md) explains the profile split
  between default CLI, development/self-test, benchmark, and production
  agent-use databases.
- [Agent JSON Contract](agent-json.md) documents the agent-facing schema and
  compatibility policy.
- [MCP Reference](mcp-reference.md) covers the read-mostly MCP tools exposed to
  agent clients.
- [CLI Reference](cli-reference.md) lists the lower-level command and flag
  surface.
- [Troubleshooting](troubleshooting.md) covers stale DBs, unsafe lifecycle
  states, empty packets, and flag-placement corrections.
