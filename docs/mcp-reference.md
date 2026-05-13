# MCP Reference

The root `README.md` is the public setup contract. The MCP server is local,
read-mostly, and evidence-oriented. It does not expose destructive tools and
does not introduce subagents.

## Start

```powershell
codegraph-mcp serve-mcp
```

Suggested Codex config:

```toml
[mcp_servers.codegraph-mcp]
command = "codegraph-mcp"
args = ["serve-mcp"]
cwd = "C:\\path\\to\\repo"
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

`explain_missing` distinguishes no symbol found, symbol found but no matching
relation, path exceeds traversal bounds, relation unsupported for language, and
optional resolver unavailable cases.

## Safety

`codegraph.index_repo` and `codegraph.update_changed_files` update only the
local `.codegraph/codegraph.sqlite` index. They do not edit source files or run
project tests.
