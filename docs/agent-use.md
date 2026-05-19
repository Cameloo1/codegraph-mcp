# Agent Use

Use this profile when a coding agent needs compact, proof-grounded CodeGraph
context while working on a real repository.

## Recommended Pattern

Build or install the release binary, keep the DB outside the source tree, and
ask for bounded agent JSON:

```powershell
cargo build --release --bin codegraph-mcp

codegraph-mcp --repo <repo> --db <agent-db> index <repo> --json

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

For test-impact work, request test evidence explicitly:

```powershell
codegraph-mcp --repo <repo> --db <agent-db> context-pack `
  --task "Find tests impacted by this change" `
  --seed <symbol> `
  --mode test-impact `
  --agent-json
```

The release binary and separate DB keep routine agent reads away from
development, benchmark, and temporary self-test artifacts. These outputs are
usable coding-agent context, not benchmark verdicts by themselves.

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

## CLI Flag Placement

Put global flags before the command:

```powershell
codegraph-mcp --repo <repo> --db <agent-db> query symbols <symbol> --agent-json
```

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
- `mixed`
- `unknown`

Default production context excludes test, mock, mixed, and unknown evidence
unless a production-only subpath can be split safely. `test-impact` mode
intentionally includes test/mock evidence and labels it clearly.

Inline Rust tests in `src/lib.rs`, including `#[cfg(test)] mod tests` and
`#[test]` functions, are classified as test evidence. Production context-pack
output excludes them by default.

## Claim Boundaries

- Manual relation precision is sampled precision only.
- Recall is unknown unless a false-negative gold benchmark exists.
- Absent proof-mode relations have no precision claim.
- CodeGraph does not claim superiority over CodeGraphContext unless an
  official comparable comparison completes and says so.
