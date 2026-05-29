# Troubleshooting

The root `README.md` is the public setup contract. Keep fixes local and
evidence-first.

## `CodeGraph index does not exist yet`

Run:

```powershell
codegraph-mcp index .
```

Then retry `status`, `query`, `impact`, `context-pack`, MCP, or UI commands.

For long-lived agent use, prefer an agent-use DB outside the repo:

```powershell
codegraph-mcp agent-use status --repo <repo> --json
codegraph-mcp agent-use index --repo <repo> --json
```

That keeps generated graph state out of the source tree. `agent-use status` is
read-only and does not create the external DB or profile parent; `agent-use
index` is the first mutating production-profile command. Plain `status --json`
still inspects the local `.codegraph` mode and does not silently redirect.

## DB Lifecycle Or Passport Refuses A Read

If a query or context command says the DB is mismatched, stale, corrupt,
unknown, from another repo, or from a failed/interrupted run, do not force it
quietly. Rebuild safely:

```powershell
codegraph-mcp index . --fresh --json
```

For an explicit `--db <path>`, CodeGraph is more conservative: invalid or
mismatched named DBs fail unless you explicitly pass `--fresh`. This protects
named benchmark artifacts from accidental replacement.

For an agent-use profile, the DB should live outside the source tree. If a
status or query command reports path access problems, treat that as filesystem
access first; it is separate from passport mismatch, corruption, or stale
scope.

Missing, stale, foreign, schema-mismatched, locked, permission-denied, and
publishing/interrupted agent-use DB states are non-claimable. Run the recovery
command from `agent-use status --json` or `agent-use mcp-config --json` rather
than copying a repo-local `.codegraph` DB into the profile.

For a changed-file production-profile update, use the once-only delta command
only after the profile DB is safe:

```powershell
codegraph-mcp agent-use watch --repo <repo> --once --changed src\file.ts --json
```

If that command reports `not_indexed`, stale, locked, or another unsafe state,
run `agent-use index` first. The command intentionally does not auto-index and
does not fall back to repo-local `.codegraph`.

## `serve-ui` Refuses A Host

`serve-ui` is local-only by default. Use:

```powershell
codegraph-mcp serve-ui --host 127.0.0.1 --port 7878
```

Remote bind addresses are rejected intentionally.

## SQLite Or FTS Errors

Prefer a safe fresh rebuild:

```powershell
codegraph-mcp index . --fresh
```

Do not delete source files. Delete generated `.codegraph/` state only when you
intentionally want to remove the default local index. Agent-use DBs may live
outside the repo.

Normal SQLite WAL/SHM files are reported as `sqlite_sidecars` with
`sidecar_status: normal`. They are only orphaned when the main DB is missing.

## Slow Indexing

Check for large generated files or dependency folders. The indexer ignores
`.git`, `.codegraph`, `node_modules`, `target`, `dist`, `build`, `out`,
coverage folders, lock files, source maps, bundles, and minified JS. If a large
generated source file still appears, add it to repo ignore policy before
indexing.

## Empty Context Packet

Use an exact seed:

```powershell
codegraph-mcp query symbols <query>
codegraph-mcp context-pack --task "..." --seed <resolved-symbol> --mode production
```

Vectors suggest candidates, but exact graph/source verification controls final
packet evidence.

If the agent needs tests, request them explicitly:

```powershell
codegraph-mcp context-pack --task "..." --seed <resolved-symbol> --mode test-impact --agent-json
```

Production mode excludes test/mock/mixed/unknown evidence by default. Inline
Rust `#[cfg(test)] mod tests` and `#[test]` functions are test evidence even
when they live in `src/lib.rs`.

For documentation-heavy prompts, CodeGraph may return DB health and exact
symbol matches but no proof paths/snippets. That is not a green or red product
claim; use direct document inspection for the content pass and report the
missing packet evidence honestly.

If `context-pack --agent-json` returns `no_proof_path_found` with fallback
snippets, that is useful source-text evidence, not typed graph proof. Inspect
the cited files/spans, keep the graph relation unknown, and rerun with a more
specific seed or `--mode test-impact` when the task is test-oriented.

Planning packets may include `follow_up_queries`. Treat them as bounded query
hints for the next inspection step, not as shell-ready commands.

## Vector Or Nuance Candidate Output Looks Unproven

Vector, binary-vector, and nuance-rescue candidates are recall aids. They are
expected to remain `graph_proof=false` until graph/source verification finds a
proof path.

For vector candidates, build and pass a matching local vector index:

```powershell
codegraph-mcp agent-use index --repo <repo> --json
codegraph-mcp agent-use context-pack --repo <repo> --task "..." --agent-json
```

If the vector index is missing, stale, from another DB passport, or built with
different provider metadata, rebuild it rather than treating the diagnostic as
proof.

Candidate-spool query-index access failures are reported as sidecar access or
availability problems when SQLite cannot open the sidecar. That is different
from a corrupt graph DB; graph proof still comes from the validated graph DB,
and candidate-spool context remains candidate-only.

Use `--enable-nuance-rescue-candidates` when the task depends on rare
identifiers, short symbols, config keys, route literals, test names, or
no-extension support scripts. Nuance rescue still cannot prove graph
relations by itself.

## MCP Tool Input Error

Call `tools/list` through the MCP client and match the tool schema. Invalid
inputs return structured errors instead of partial results.

## Global Flag Placement Error

Global flags such as `--repo` and `--db` belong before the command:

```powershell
codegraph-mcp --repo <repo> --db <agent-db> query symbols <symbol> --agent-json
```

If a query command reports that `--db` or `--repo` is a global flag, move it
before `query`. To search for a literal flag-like term, use `--`:

```powershell
codegraph-mcp --repo <repo> --db <agent-db> query text --agent-json -- --db
```

## Watcher Does Not React

Use a direct one-shot update to confirm ignore rules and indexing:

```powershell
codegraph-mcp watch . --once --changed src\file.ts
```

If this works, the persistent watcher is likely not receiving filesystem
events from the editor or filesystem layer.

For the production agent-use profile, use:

```powershell
codegraph-mcp agent-use watch --repo <repo> --once --changed src\file.ts --json
```

This updates the external profile DB only when lifecycle preflight says the DB
is safe to write. For long-running editor sessions, use:

```powershell
codegraph-mcp agent-use watch --repo <repo> --json
```

Persistent agent-use watch debounces save bursts and coalesces changed paths,
then calls the same once-delta update primitive. It does not auto-index a
missing/stale/foreign/schema-mismatched profile and it does not fall back to
repo-local `.codegraph`; run `agent-use index` first when status reports an
unsafe or missing profile DB.
