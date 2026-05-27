# Operational Profiles

CodeGraph has two practical operating profiles. Keep them separate so
development experiments and lab runs do not contaminate the graph used by an
agent for real work.

## Profile: Development / Self-Test

Use this when changing CodeGraph itself.

| Field | Value |
|---|---|
| Label | Development / self-test |
| Purpose | Build, test, debug, run local diagnostics, and inspect CodeGraph changes. |
| Binary | debug binary or `cargo run --bin codegraph-mcp -- ...` |
| DB path | repo-local diagnostic DB |
| Claim boundary | Diagnostic only unless a result is intentionally promoted and claim-reviewed. |
| Allowed work | Debug timings, fixture indexing, local experiments, failing-gate investigation. |
| Not allowed | Production readiness claims from debug binaries or contaminated DBs. |

Example:

```powershell
.\scripts\codegraph-profile.ps1 -Profile dev -Action index -BuildIfMissing
.\scripts\codegraph-profile.ps1 -Profile dev -Action context-pack `
  -Task "Find the DB lifecycle preflight path" `
  -Seed inspect_db_lifecycle_preflight
```

## Profile: Production Agent-Use

Use this when a coding agent needs CodeGraph context while working on a real
repository.

| Field | Value |
|---|---|
| Label | Agent use |
| Purpose | Stable local context source for agent prompts and implementation work. |
| Binary | release binary or installed release binary. |
| DB path | outside the source tree, for example under the user's local application data directory. |
| Claim boundary | Usable agent context, not a public metric result by itself. |
| Allowed work | Status, safe index/update, search, trace, impact, and context-pack calls. |
| Not allowed | Debug timing claims, comparison claims, or reuse of dev DB artifacts. |

Example:

```powershell
cargo build --release --bin codegraph-mcp
codegraph-mcp agent-use status --repo <repo> --json
codegraph-mcp agent-use index --repo <repo> --json
codegraph-mcp agent-use watch --repo <repo> --once --changed src\file.ts --json
codegraph-mcp agent-use query symbols <symbol> --repo <repo> `
  --limit 5 --agent-json
codegraph-mcp agent-use query text "token or phrase" --repo <repo> `
  --limit 5 --agent-json
codegraph-mcp agent-use query files service --repo <repo> `
  --limit 5 --agent-json
codegraph-mcp agent-use context-pack --repo <repo> `
  --task "Trace the indexing entry point" `
  --agent-json
codegraph-mcp agent-use mcp-config --repo <repo> --json
```

Suggested MCP config is produced by the resolver-backed command below. It emits
JSON only by default and points MCP at the external production profile DB:

```powershell
codegraph-mcp agent-use mcp-config --repo <repo> --json
```

## Operating Rules

- Label every CodeGraph run as development, self-test, or agent-use.
- Use the agent-use profile for routine agent context.
- Use the development profile only when testing CodeGraph changes.
- Keep production agent-use DBs outside the source tree.
- Plain `status --json` remains local `.codegraph` status; it may point at
  agent-use commands but must not silently redirect.
- `agent-use status` is read-only. `agent-use index` is the first mutating
  production-profile command.
- `agent-use watch --once --changed <path>` is the first changed-file delta
  primitive for the production profile. It requires an existing safe DB, owns
  the external DB resolver, rejects `--db`, and does not start persistent watch
  mode.
- Do not use debug proof-build timings as production gate evidence.
- Do not compare partial, stale, or diagnostic DBs as final artifacts.
- If the production profile reports a mismatched, stale, corrupt, or unknown DB,
  rebuild it safely instead of reusing it.
- Missing, stale, foreign, schema-mismatched, locked, permission-denied, and
  publishing/interrupted profile states are non-claimable unless an explicit
  diagnostic mode says otherwise.
- Bounded candidate spool and runtime vector sidecars are candidate-only. Audit
  artifacts are diagnostic-only.
- Do not index generated lab payloads, local DBs, `target/`, `.venv/`,
  `node_modules/`, or report artifacts as source evidence.

## Routine Agent Workflow

Expected local flow:

1. Run agent-use `status`.
2. If missing or stale, run agent-use `index`.
3. After a local edit, run agent-use `watch --once --changed <path>` to update
   the external graph DB when the DB is safe to write.
4. Ask for a focused `context-pack` for the task.
5. Use the returned files, symbols, source spans, and paths as evidence.
6. Keep normal code edits and verification separate from CodeGraph diagnostic
   or comparison claims.

Use `--agent-json` and explicit limits for agent loops. Use production
context-pack mode for production proof context; use `test-impact` only when the
agent intentionally needs test/mock evidence. Inline Rust test modules and
`#[test]` functions are test evidence and are excluded from production context
by default.

This makes CodeGraph part of the everyday coding loop without letting its
development artifacts become the evidence source for itself.
