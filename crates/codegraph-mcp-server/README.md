# codegraph-mcp-server

Local stdio JSON-RPC MCP server crate.

The server exposes the `codegraph.*` tools from `MVP.md`, validates tool inputs,
advertises input/output schemas and safety annotations, provides resources and
prompt templates, paginates large results, returns resource links for
files/source spans, and explains missing paths.

The packet surface includes `codegraph.query_local_flow_packets` for bounded
compact rows and `codegraph.open_local_flow_packet` for an exact packet id.
Opened packets include the persisted `dict_v1` body; verbose ordered steps are
opt-in. Querying or opening evidence does not create proof.

`codegraph.index_repo`, `codegraph.update_changed_files`, and
`codegraph.validate_edit` may update local CodeGraph index/profile state.
Read tools remain read-only, and no MCP tool edits source files, invokes an
external service, or performs a destructive action. Production agent workflows
use the external agent profile rather than silently falling back to repo-local
`.codegraph`.
