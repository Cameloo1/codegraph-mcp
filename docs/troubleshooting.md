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

If `agent-use status` reports `permission_denied`, `profile_inaccessible`, or
another path-access state, treat that as an access problem, not as proof that
the repo is unindexed. `agent-use mcp-config --repo <repo> --json` should
resolve the same profile identity as status when access is available; if the
two disagree, keep both packets and rebuild/index only after the profile path
and access state are clear.

For a changed-file production-profile update, use the once-only delta command
only after the profile DB is safe:

```powershell
codegraph-mcp agent-use watch --repo <repo> --once --changed src\file.ts --json
```

If that command reports `not_indexed`, stale, locked, or another unsafe state,
run `agent-use index` first. The command intentionally does not auto-index and
does not fall back to repo-local `.codegraph`.

## Validate-Edit Reports A Blocker Or Fails

For agent/editor hooks, use the canonical production-profile command:

```powershell
codegraph-mcp agent-use validate-edit --repo <repo> --changed src\file.ts --agent-json
```

Repeat `--changed` for multi-file edits. Add `--fail-on-blocking` only when a
pre-commit or CI step should return a distinct blocker exit while preserving
stdout JSON:

```powershell
codegraph-mcp agent-use validate-edit --repo <repo> `
  --changed src\file.ts `
  --changed src\helper.ts `
  --fail-on-blocking `
  --agent-json > validate-edit.json

if ($LASTEXITCODE -eq 2) {
  Write-Error "CodeGraph validate-edit hard interrupt; inspect validate-edit.json"
}
```

Exit 0 means validation completed and emitted JSON, even when the packet status
is `blocking_graph_error`. Exit 2 with `--fail-on-blocking` means validation
completed, JSON was printed, and `hard_interrupt_available=true`. Other
nonzero exits mean validation did not complete because of command, config,
lifecycle, tool, runtime, or protocol failure. Agents should parse stdout JSON;
do not infer proof from exit code alone.

`blocking` or `blocking_graph_error` findings are only hard interrupts when
they come from reverified graph/source proof or eligible integrity/lifecycle
proof failures. `warning`, `unknown`, unsupported, degraded, and
`diagnostic_only` findings do not interrupt by default. Text, candidate,
vector, and source-navigation evidence cannot hard-interrupt by themselves.
Unsafe DB state is a lifecycle blocker, not source-code proof.

If validate-edit reports `not_indexed`, stale, foreign, schema-mismatched,
locked, permission-denied, publishing, interrupted, or otherwise unsafe DB
state, run:

```powershell
codegraph-mcp agent-use status --repo <repo> --json
codegraph-mcp agent-use index --repo <repo> --json
codegraph-mcp agent-use validate-edit --repo <repo> --changed src\file.ts --agent-json
```

The command resolves the external production agent-use DB. It does not
auto-index, does not fall back to repo-local `.codegraph`, and does not edit
source files. An editor save hook may call the same command for saved files, but
this is not a persistent editor daemon, editor plugin, background loop, or
unsaved-buffer integration claim.

If validate-edit, watch, status, doctor, or audit reports MVP4.2 micro-edge
state as `unavailable`, `stale`, `incompatible`, `corrupt`, or `truncated`,
treat it as optional layer state unless a reverified proof-integrity finding is
also present. Normal `LOCAL_RETURNS_TO` add/remove/change deltas are not source
errors. A persisted micro-edge integrity contradiction should recommend
reindexing or repairing CodeGraph state before rerunning validation; it does not
by itself prove that the source must be edited. Micro-edge visibility does not
activate local-flow packets, `flow_proof`, `mutation_proof`, or route/auth
semantics.

## Windows Application Control Blocks A Fresh Build Or Test

If `cargo build`, `cargo test`, or a clean-clone smoke fails before the Rust test
body runs with:

```text
An Application Control policy has blocked this file. (os error 4551)
```

diagnose it first as a local Windows application-control/WDAC policy block on a
freshly built unsigned executable. Check the blocked path in the error, the
Cargo target directory, and local policy. Do not rewrite product code or mark a
test assertion failed until the same test reaches its Rust body or fails with a
normal Rust panic/assertion.

## `query unresolved-calls` Is Empty

Use filters, not a positional symbol:

```powershell
codegraph-mcp agent-use query unresolved-calls --repo <repo> `
  --path src\file.ts `
  --class repo_local_candidate `
  --limit 20 --agent-json
```

Accepted classes are `repo_local_candidate`, `external_dependency`,
`builtin_or_std`, `macro_or_codegen`, and `dynamic_or_computed`. The command
does not accept `unresolved-calls <symbol>`; that shape returns a targeted
parser error.

An empty result can mean the file has no current unresolved-reference lane rows,
the path/class filters exclude the rows, the DB is stale/missing, or the edit
has already been fixed and revalidated. Run `agent-use validate-edit` on the
changed file and then query the same `--path`. Unresolved-reference rows are
warning/query parity evidence and stay `not_graph_proof`.

## Relation Query Says `no_proof_path_found`

`no_proof_path_found` means CodeGraph did not find a supported, bounded,
source-spanned graph proof path for that query. It is not proof that the
relationship is impossible. Try an exact symbol from `query symbols`, use an
explicit `--entity-id` when the symbol is ambiguous, and inspect any fallback
source snippets. Fallback snippets, text matches, source-navigation evidence,
candidate rows, and stale sidecars remain non-proof.

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

If status, validate-edit, watch, or context-pack reports stale, corrupt, or
inaccessible candidate/vector/PathEvidence/routing sidecars while the graph DB
is claimable, the graph DB is not automatically corrupt. Rebuild sidecars with
`agent-use index --repo <repo> --json` when candidate recall matters. Do not
treat stale sidecars as fresh proof, and do not treat optional sidecar
degradation as a source-code hard interrupt.

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
