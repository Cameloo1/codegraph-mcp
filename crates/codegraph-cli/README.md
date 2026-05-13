# codegraph-cli

CLI crate for `codegraph-mcp`.

`MVP.md` is the source of truth for this crate's phase boundary and acceptance
criteria.

Phase 21 hardens `codegraph-mcp index <repo>` for TypeScript/JavaScript
declaration facts, conservative core static relations, and heuristic
auth/security/event/db/test relations into
`.codegraph/codegraph.sqlite`. Indexing also populates the Stage 0 SQLite FTS
index for files, entities, and snippets. `codegraph-mcp serve-mcp` starts the
local read-mostly MCP server.

Phase 16 also implements init dry-run/setup, status, `query symbols`,
`query path`, `context-pack`, `impact`, and `.cgc-bundle` export/import with
schema validation.

Phase 17 wires `init --with-templates` to install checked-in Codex templates:
`AGENTS.md`, `.codex/skills`, and `.codex/hooks`. Each template points back to
`MVP.md`, requires no subagents, prefers verified relation paths and source
spans, updates changed files after edits, and recommends practical tests.

Phase 18 adds `codegraph-mcp watch [repo]`, backed by Rust `notify`. Watch mode
debounces file events, ignores repo noise, prunes stale facts for changed files,
re-inserts updated parser output, refreshes binary signatures, and rebuilds the
watcher's in-memory adjacency cache.

Phase 19 adds `codegraph-mcp serve-ui`, a loopback-only Proof-Path UI backed by
the local `.codegraph/codegraph.sqlite` store. It serves bundled static assets,
path graph JSON, impact dashboard data, and context packet previews without a
cloud dependency.

Phase 20 adds `codegraph-mcp bench`, which runs local reproducible benchmarks
from `codegraph-bench` and emits machine-readable JSON by default. It can also
write JSON or Markdown reports with `--output`.

Phase 26 adds `codegraph-mcp bench gaps`, a measurement-only gap scoreboard
that compares CodeGraph against internal baselines and optional black-box
CodeGraphContext runs. Missing competitor data is recorded as `unknown` or
`skipped`.

Phase 30 upgrades `serve-ui` with graph modes, exactness legends, source-span
preview, graph export, context copying, and large-graph guardrails. It also
adds `bench real-repo-corpus` and `bench parity-report` for pinned real-repo
manifests and final parity artifacts.

The CLI intentionally does not expose binary-vector tuning commands or UI
builder controls. Exact query APIs live in `codegraph-query`, including
graph-only context packet construction and the integrated Bayesian-scored
funnel. Stage 1 binary sieve and Stage 2 compressed rerank APIs live in
`codegraph-vector`.
