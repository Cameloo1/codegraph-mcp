# codegraph-mcp-server

Local MCP server crate.

Phase 30 implements a stdio JSON-RPC MCP server named `codegraph-mcp`. It
exposes the required read-mostly `codegraph.*` tools from `MVP.md`, validates
tool inputs, advertises input/output schemas, provides resources and prompt
templates, paginates large results, returns resource links for files/source
spans, and explains missing paths.

The only index-mutating tools are `codegraph.index_repo` and
`codegraph.update_changed_files`, both scoped to local `.codegraph` state. No
destructive tools or subagent workflow are introduced.
