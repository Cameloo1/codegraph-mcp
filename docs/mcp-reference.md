# MCP Reference

The root `README.md` is the public setup contract. The MCP server is local,
read-mostly, and evidence-oriented. It does not expose destructive source-edit
tools.

## Start

```powershell
codegraph-mcp serve-mcp
```

For long-lived agent use, prefer the agent-use profile from
[operational-profiles.md](operational-profiles.md). That profile uses a release
binary and a DB outside the source tree, separate from development and benchmark
indexes.

For CLI-driven agent loops, see [agent-use.md](agent-use.md). The CLI
`--agent-json` schemas are the stable compact machine-readable contract for
index, query, callers/callees, and context-pack output.

Suggested generic Codex config:

```toml
[mcp_servers.codegraph-mcp]
command = "codegraph-mcp"
args = ["serve-mcp"]
cwd = "<repo>"
```

Suggested config for an agent-use profile:

```toml
[mcp_servers.codegraph-mcp-agent]
command = "<codegraph-mcp-release-binary>"
args = [
  "--repo", "<repo>",
  "--db", "<agent-db>",
  "serve-mcp"
]
cwd = "<repo>"
```

## Tools

- `codegraph.status`
- `codegraph.index_repo`
- `codegraph.update_changed_files`
- `codegraph.search_symbols`
- `codegraph.search_text`
- `codegraph.search_semantic`
- `codegraph.context_pack`
- `codegraph.trace_path`
- `codegraph.impact_analysis`
- `codegraph.find_callers`
- `codegraph.find_callees`
- `codegraph.find_reads`
- `codegraph.find_writes`
- `codegraph.find_mutations`
- `codegraph.find_dataflow`
- `codegraph.find_auth_paths`
- `codegraph.find_event_flow`
- `codegraph.find_tests`
- `codegraph.find_migrations`
- `codegraph.explain_edge`
- `codegraph.explain_path`

Every listed tool advertises an `inputSchema`, an `outputSchema`, and safety
annotations. The annotations mark tools as local-only and
`destructiveHint = false`; index/update tools write only `.codegraph`
index state and never edit source files.

## Resources

- `codegraph://status`
- `codegraph://schema`
- `codegraph://languages`
- `codegraph://bench/latest`
- `codegraph://context/<id>`

Resources return JSON text payloads for Codex clients that prefer a stable
reference URI over an immediate tool call. The schema resource includes the
current tool, resource, prompt, and safety metadata.

## Prompts

- `impact-analysis`
- `trace-dataflow`
- `auth-review`
- `test-impact`
- `refactor-safety`

Prompt templates reference project guardrails, ban subagents, and steer the
caller toward verified paths, source spans, and explicit exactness/confidence
labels.

## Inputs

Most tools accept `repo` when repository context is needed. Query tools accept
symbol/entity ids, relation filters, and bounded traversal limits depending on
the tool schema. Search/path tools also accept `limit`, `offset`, and `mode`
where applicable. Invalid input returns a structured JSON-RPC error.

Caller/callee tools preserve exact traversal when an `entity_id` is supplied.
For symbol queries, an unambiguous symbol resolves to exact entity results;
ambiguous symbols return candidate ids instead of silently choosing one match.

## Output Contract

Tool responses are compact JSON values suitable for Codex:

- graph/source-verified ids
- PathEvidence and source spans
- exactness and confidence labels
- compact snippets where relevant
- pagination for large result sets
- `resource_links` for files/source spans where available
- `explain_missing` when a requested path is absent
- no fake citations or hidden source failures

Context/proof responses label evidence as `production`, `test`, `mock`,
`mixed`, or `unknown` when that evidence classification is available.
Production context excludes test/mock/mixed/unknown evidence by default; test
impact requests include test evidence intentionally. Inline Rust `#[cfg(test)]`
modules and `#[test]` functions are test evidence even when they are inside a
normal source file.

`explain_missing` distinguishes no symbol found, symbol found but no matching
relation, path exceeds traversal bounds, relation unsupported for language, and
optional resolver unavailable cases.

## DB Lifecycle

MCP read paths use the same passport/preflight guard as the CLI. A matching,
completed, integrity-checked DB is reusable. Missing, stale, mismatched, corrupt,
or unknown DB state is not silently trusted.

- `codegraph.index_repo` can build or update the configured DB.
- `codegraph.update_changed_files` requires a reusable DB and refuses unsafe
  state instead of writing over unknown data.
- `codegraph.status` reports DB health and blockers rather than bypassing the
  lifecycle gate.
- Query and context tools refuse mismatched DBs unless an explicit diagnostic
  stale-read path is used, and diagnostic output must be labeled as such.
- Status output includes passport state, the exact checked DB path, access
  classification, `sqlite_sidecars`, and `sidecar_status`.

## Safety

`codegraph.index_repo` and `codegraph.update_changed_files` update only the
configured local SQLite index. The default project-local path is
`.codegraph/codegraph.sqlite`; the production agent-use profile deliberately
uses a DB outside the source tree. These tools do not edit source files or run
project tests.
