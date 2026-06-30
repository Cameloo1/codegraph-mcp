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
> CodeGraph's checkout, generated databases, logs, benchmark artifacts, and
> temporary files separate from the target project. Do not create or mutate a
> repo-local `.codegraph` unless explicitly asked. Do not index until the target
> repo path and generated MCP config have been shown to the user.

After the user approves the target repo and config, index explicitly:

```powershell
codegraph-mcp agent-use index --repo <target-repo> --json
```

## Windows WSL2 Path For WDAC/SAC Machines

Use this path on Windows 10/11 machines where Windows Application Control,
Smart App Control, WDAC, antivirus, or reputation checks block freshly built
Rust executables. It is the issue #22 tester path for a fresh clone through a
running `agent-use` profile without creating repo-local `.codegraph` state.

From Windows PowerShell, install and enter Ubuntu if needed:

```powershell
wsl --install -d Ubuntu
wsl -d Ubuntu
```

Inside Ubuntu, install prerequisites, clone on the Windows filesystem, keep
Cargo build output on the Linux filesystem, and run the smoke:

```sh
sudo apt-get update
sudo apt-get install -y build-essential ca-certificates curl git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"

WINUSER="$(cmd.exe /C echo %USERNAME% 2>/dev/null | tr -d '\r')"
mkdir -p "/mnt/c/Users/$WINUSER/source"
git clone https://github.com/Cameloo1/codegraph-mcp.git "/mnt/c/Users/$WINUSER/source/codegraph-mcp"
cd "/mnt/c/Users/$WINUSER/source/codegraph-mcp"

export CARGO_TARGET_DIR="$HOME/cg-target/codegraph-mcp"
export CODEGRAPH_AGENT_USE_DATA_ROOT="$HOME/.local/share/codegraph-agent-use"
scripts/smoke_wsl2_agent_use.sh
```

The smoke builds `codegraph-mcp`, runs `agent-use status`, `mcp-config`,
`index`, `query symbols`, and `validate-edit` against a disposable target repo,
then fails if `.codegraph` appears in either checkout. Use the same exported
paths for a real target repo:

```sh
binary="$CARGO_TARGET_DIR/release/codegraph-mcp"
target_repo="/mnt/c/path/to/your/repo"

"$binary" agent-use status --repo "$target_repo" --json
"$binary" agent-use mcp-config --repo "$target_repo" --json
"$binary" agent-use index --repo "$target_repo" --json
```

`agent-use status` and `agent-use mcp-config` are read-only. `agent-use index`
is the first mutating command and writes to `CODEGRAPH_AGENT_USE_DATA_ROOT`, not
to `<target-repo>/.codegraph`.

## Build

```powershell
cargo build --workspace
cargo test --workspace
```

On Windows, a failure that says `An Application Control policy has blocked this
file. (os error 4551)` before a Rust test body runs is a Windows application
control/WDAC policy block on a freshly built executable. Diagnose the blocked
path and local policy first; do not treat that message as a product test
assertion failure. On SAC/WDAC machines, prefer the WSL2 path above for the
fresh-clone smoke and local verification.

This checkout is not published as a `codegraph-mcp` crates.io package. For a
local install from source, build the release binary or install the workspace
package path:

```powershell
cargo build --release --bin codegraph-mcp
cargo install --path crates\codegraph-cli
```

The path install may still contact the crates.io index unless your Cargo cache
or offline settings already contain all dependencies.

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
tasks, but they remain candidates until graph/source verification succeeds. The
current local vector provider is deterministic/token-based and must not be
described as learned production semantic quality.

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

Use the one-shot command first. It updates a changed file only after the
existing profile DB passes lifecycle preflight, and it is the deterministic
release-tested update primitive. Persistent watch debounces editor save bursts
and schedules that same changed-file update primitive; it is a scheduler, not a
second proof surface.

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
