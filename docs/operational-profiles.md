# Operational Profiles

CodeGraph has two different operating modes in this checkout. Keep them
separate so development experiments do not contaminate the graph used by an
agent for real work.

## Profile: Development / Self-Test

Use this when changing CodeGraph itself.

| Field | Value |
|---|---|
| Label | `DEVELOPMENT_SELF_TEST` |
| Purpose | Build, test, debug, benchmark, and inspect CodeGraph changes. |
| Binary | `target/debug/codegraph-mcp.exe` or `cargo run --bin codegraph-mcp -- ...` |
| DB path | `.codegraph/development-self-test.sqlite` |
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

Use this when Codex or another coding agent needs CodeGraph context while
working on this repo.

| Field | Value |
|---|---|
| Label | `PRODUCTION_AGENT_USE` |
| Purpose | Stable local context source for agent prompts and implementation work. |
| Binary | `target/release/codegraph-mcp.exe` or an installed release binary. |
| DB path | `%LOCALAPPDATA%\CodeGraphMCP\agent-indexes\codegraph-mcp\production-agent-use.sqlite` |
| Claim boundary | Usable agent context, not a benchmark result by itself. |
| Allowed work | Status, safe index/update, search, trace, impact, and context-pack calls. |
| Not allowed | Debug timing claims, benchmark comparison claims, or reuse of dev DB artifacts. |

Example:

```powershell
cargo build --release --bin codegraph-mcp
.\scripts\codegraph-profile.ps1 -Profile prod-agent -Action index
.\scripts\codegraph-profile.ps1 -Profile prod-agent -Action context-pack `
  -Task "Trace README benchmark visual generation" `
  -Seed generate_readme_agent_benchmark_visuals
```

Suggested Codex MCP config:

```toml
[mcp_servers.codegraph-mcp-production]
command = "C:\\Users\\wamin\\Desktop\\development\\codegraph-mcp\\target\\release\\codegraph-mcp.exe"
args = [
  "--repo", "C:\\Users\\wamin\\Desktop\\development\\codegraph-mcp",
  "--db", "C:\\Users\\wamin\\AppData\\Local\\CodeGraphMCP\\agent-indexes\\codegraph-mcp\\production-agent-use.sqlite",
  "serve-mcp"
]
cwd = "C:\\Users\\wamin\\Desktop\\development\\codegraph-mcp"
```

## Operating Rules

- Label every CodeGraph run as `DEVELOPMENT_SELF_TEST` or `PRODUCTION_AGENT_USE`.
- Use the production profile for routine agent context.
- Use the development profile only when testing CodeGraph changes.
- Keep production agent-use DBs outside the source tree.
- Do not use debug proof-build timings as production gate evidence.
- Do not compare partial, stale, or diagnostic DBs as final artifacts.
- If the production profile reports a mismatched, stale, corrupt, or unknown DB,
  rebuild it safely instead of reusing it.
- Do not index generated benchmark payloads, local DBs, `target/`, `.venv/`,
  `node_modules/`, or report artifacts as source evidence.

## Routine Agent Workflow

For future prompts on this repo, the expected local flow is:

1. Run production profile `status`.
2. If missing or stale, run production profile `index`.
3. Ask for a focused `context-pack` for the task.
4. Use the returned files, symbols, source spans, and paths as evidence.
5. Keep normal code edits and verification separate from CodeGraph benchmark
   claims.

This makes CodeGraph part of the everyday coding loop without letting its
development artifacts become the evidence source for itself.
