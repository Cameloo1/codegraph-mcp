# Operational Profiles

CodeGraph has two practical operating profiles. Keep them separate so
development experiments and benchmark runs do not contaminate the graph used by
an agent for real work.

## Profile: Development / Self-Test

Use this when changing CodeGraph itself.

| Field | Value |
|---|---|
| Label | Development / self-test |
| Purpose | Build, test, debug, benchmark, and inspect CodeGraph changes. |
| Binary | debug binary or `cargo run --bin codegraph-mcp -- ...` |
| DB path | repo-local diagnostic DB |
| Claim boundary | Diagnostic only unless a benchmark explicitly says otherwise. |
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
| Claim boundary | Usable agent context, not a benchmark result by itself. |
| Allowed work | Status, safe index/update, search, trace, impact, and context-pack calls. |
| Not allowed | Debug timing claims, benchmark comparison claims, or reuse of dev DB artifacts. |

Example:

```powershell
cargo build --release --bin codegraph-mcp
codegraph-mcp --repo C:\path\to\repo --db C:\path\to\agent-indexes\repo.sqlite index C:\path\to\repo
codegraph-mcp --repo C:\path\to\repo --db C:\path\to\agent-indexes\repo.sqlite context-pack `
  --task "Trace the indexing entry point" `
  --seed index_repo_to_db
```

Suggested MCP config:

```toml
[mcp_servers.codegraph-mcp-agent]
command = "C:\\path\\to\\codegraph-mcp.exe"
args = [
  "--repo", "C:\\path\\to\\repo",
  "--db", "C:\\path\\to\\agent-indexes\\repo.sqlite",
  "serve-mcp"
]
cwd = "C:\\path\\to\\repo"
```

## Operating Rules

- Label every CodeGraph run as development, self-test, or agent-use.
- Use the agent-use profile for routine agent context.
- Use the development profile only when testing CodeGraph changes.
- Keep production agent-use DBs outside the source tree.
- Do not use debug proof-build timings as production gate evidence.
- Do not compare partial, stale, or diagnostic DBs as final artifacts.
- If the production profile reports a mismatched, stale, corrupt, or unknown DB,
  rebuild it safely instead of reusing it.
- Do not index generated benchmark payloads, local DBs, `target/`, `.venv/`,
  `node_modules/`, or report artifacts as source evidence.

## Routine Agent Workflow

Expected local flow:

1. Run agent-use `status`.
2. If missing or stale, run agent-use `index`.
3. Ask for a focused `context-pack` for the task.
4. Use the returned files, symbols, source spans, and paths as evidence.
5. Keep normal code edits and verification separate from CodeGraph benchmark
   claims.

This makes CodeGraph part of the everyday coding loop without letting its
development artifacts become the evidence source for itself.
