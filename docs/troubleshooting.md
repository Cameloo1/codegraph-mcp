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
codegraph-mcp --repo <repo> --db <agent-db> index <repo> --json
```

That keeps generated graph state out of the source tree.

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
codegraph-mcp --repo <repo> --db <agent-db> index <repo> `
  --json `
  --build-vector-index <agent-vector-index>

codegraph-mcp --repo <repo> --db <agent-db> context-pack `
  --task "..." `
  --agent-json `
  --enable-vector-candidates `
  --vector-index <agent-vector-index>
```

If the vector index is missing, stale, from another DB passport, or built with
different provider metadata, rebuild it rather than treating the diagnostic as
proof.

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

## Benchmarks Look Too Small

The benchmark suite uses controlled synthetic repos by design. It is meant to
compare modes reproducibly. Real-repo commit replay is represented as a
non-destructive replay plan when a git checkout is available.

## Benchmark Or CGC Results Look Incomplete

Incomplete means incomplete. A CGC timeout, skipped competitor executable,
partial CGC DB/WAL files, debug timing, or fake-agent dry run must stay labeled
as `timeout`, `skipped`, `diagnostic`, or `unknown`. Do not turn those into a
CodeGraph win claim.
