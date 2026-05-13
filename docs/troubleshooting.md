# Troubleshooting

The root `README.md` is the public setup contract. Keep fixes local,
evidence-first, and single-agent only.

## `CodeGraph index does not exist yet`

Run:

```powershell
codegraph-mcp index .
```

Then retry `status`, `query`, `impact`, `context-pack`, MCP, or UI commands.

## `serve-ui` Refuses A Host

`serve-ui` is local-only by default. Use:

```powershell
codegraph-mcp serve-ui --host 127.0.0.1 --port 7878
```

Remote bind addresses are rejected intentionally.

## SQLite Or FTS Errors

Remove only the local generated index if you need a clean rebuild:

```powershell
Remove-Item -Recurse -Force .codegraph
codegraph-mcp index .
```

Do not delete source files. `.codegraph/` is generated local state.

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
codegraph-mcp context-pack --task "..." --seed <resolved-symbol>
```

Vectors suggest candidates, but exact graph/source verification controls final
packet evidence.

## MCP Tool Input Error

Call `tools/list` through the MCP client and match the tool schema. Invalid
inputs return structured errors instead of partial results.

## Watcher Does Not React

Use a direct one-shot update to confirm ignore rules and indexing:

```powershell
codegraph-mcp watch . --once --changed src\file.ts
```

If this works, the persistent watcher is likely not receiving filesystem
events from the editor or filesystem layer.

## Benchmarks Look Too Small

The MVP benchmark suite uses controlled synthetic repos by design. It is meant
to compare modes reproducibly. Real-repo commit replay is represented as a
non-destructive replay plan when a git checkout is available.
