# Quickstart

Use this page for the first local run. For the product overview and docs map,
start with the root [README](../README.md).

CodeGraph is Rust-first, local-first, proof-first, and single-agent oriented.
Normal developer tools such as `rg`, editing, and tests stay in the loop;
CodeGraph adds lifecycle-checked graph context beside them.

## For Your Coding Agent

Use these instructions when setting up CodeGraph for a workspace:

> You are installing CodeGraph MCP as a dedicated repo-context tool for this
> project. Clone CodeGraph into a separate tools folder outside the project you
> will inspect. Build the release binary. Verify
> `codegraph-mcp --json --version`. Run
> `codegraph-mcp agent-use status --repo <target-repo> --json`. Generate the MCP
> config with
> `codegraph-mcp agent-use mcp-config --repo <target-repo> --json`. Keep
> CodeGraph's checkout, generated databases, logs, diagnostic artifacts, and
> temporary files separate from the target project. Do not create or mutate a
> repo-local `.codegraph` unless explicitly asked. Do not index until the target
> repo path and generated MCP config have been shown to the user.

After the user approves the target repo and config, index explicitly:

```powershell
codegraph-mcp agent-use index --repo <target-repo> --json
```

## Build

```powershell
cargo build --workspace
cargo test --workspace
```

## Initialize A Repo

From a repository checkout:

```powershell
codegraph-mcp init --with-templates --with-codex-config
codegraph-mcp index .
codegraph-mcp status
```

This creates `.codegraph/codegraph.sqlite` for the default local CLI profile,
plus optional Codex templates and a local MCP config when requested. For routine
coding-agent use, prefer the `agent-use` profile below because it keeps the DB
outside the source tree.

## Query Evidence

```powershell
codegraph-mcp query symbols profileRoute
codegraph-mcp query callers saveProfile
codegraph-mcp query callees profileRoute
codegraph-mcp query chain profileRoute saveProfile
codegraph-mcp query path profileRoute saveProfile
codegraph-mcp impact profileRoute
codegraph-mcp context-pack --task "Trace profileRoute auth and mutation impact" --seed profileRoute --budget 1600
```

The CLI returns JSON with graph/source evidence, source spans, exactness, and
confidence labels. Candidate/text/vector lanes can guide inspection, but graph
relations are only proof when graph/source verification succeeds.

For a coding-agent loop, use the release binary and the first-class
`agent-use` profile. This keeps the agent DB outside the source tree and makes
status checks read-only until you explicitly index:

```powershell
cargo build --release --bin codegraph-mcp
codegraph-mcp agent-use status --repo <repo> --json
codegraph-mcp agent-use index --repo <repo> --json
codegraph-mcp agent-use query symbols profileRoute --repo <repo> `
  --limit 5 --agent-json
codegraph-mcp agent-use context-pack --repo <repo> `
  --task "Trace profileRoute auth and mutation impact" `
  --agent-json
codegraph-mcp agent-use mcp-config --repo <repo> --json
```

Use `--mode test-impact --agent-json` when the task intentionally needs
test/mock evidence. Production context excludes test/mock/mixed/unknown
evidence by default.

Optional vector and nuance-rescue lanes can improve candidate recall for hard
tasks, but they remain candidates until graph/source verification succeeds.

## Serve MCP

```powershell
codegraph-mcp agent-use mcp-config --repo <repo> --json
```

Use the generated config in your agent client. MCP tools are read-mostly and
proof-oriented; they read the same external production profile DB used by the
`agent-use` CLI.

## Watch Changes

```powershell
codegraph-mcp agent-use watch --repo <repo> --once --changed src\file.ts --json
codegraph-mcp agent-use watch --repo <repo> --json
```

The one-shot command updates a changed file only after the existing profile DB
passes lifecycle preflight. Persistent watch debounces editor save bursts and
schedules the same changed-file update primitive.

## Proof-Path UI

```powershell
codegraph-mcp serve-ui --port 7878
```

Open `http://127.0.0.1:7878`. The UI is loopback-only by default and uses the
local `.codegraph/codegraph.sqlite` store. It supports proof path,
neighborhood, impact, auth/security, event flow, test impact, and unresolved
call views with exactness legends, source-span preview, graph JSON export, and
truncation warnings for large graphs.

## Related Docs

- [Install And Release Notes](install.md) covers packaged install and release
  template paths.
- [CLI Reference](cli-reference.md) lists the full command and flag surface.
- [Troubleshooting](troubleshooting.md) covers common lifecycle, flag-placement,
  watch, and empty-packet issues.
