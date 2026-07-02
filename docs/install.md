# Install And Release Notes

The public setup contract is the root `README.md`. CodeGraph remains
Rust-first, local-first, and evidence-oriented. No installer, shell completion,
or release template changes that contract.

## Install Paths

Current release templates document these install paths. Unless a GitHub release
is actually published for a version, the checkout build is the authoritative
local install path.

- GitHub release archives from `dist/archive-manifest.json`
- PowerShell installer template: `install/install.ps1`
- POSIX shell installer template: `install/install.sh`
- `cargo install --path crates/codegraph-cli` from a checkout
- Windows WSL2 source checkout path for WDAC/SAC machines:
  [Quickstart WSL2 path](quickstart.md#windows-wsl2-path-for-wdacsac-machines)
- cargo-binstall metadata template: `dist/cargo-binstall.example.toml`
- Homebrew formula template: `packaging/homebrew/codegraph-mcp.rb`

The npm wrapper path is intentionally not included because it would add
packaging surface without improving the Rust-first core.

This workspace currently has `publish = false` and version `0.0.0`; do not
treat `cargo install codegraph-mcp` as a verified crates.io install path. If a
registry package named `codegraph-cli` exists, verify that it matches the
intended release before using it as a distribution claim.

## Recommended Agent Setup

For routine agent use, build or install a release binary and keep the agent DB
outside the source tree:

```powershell
cargo build --release --bin codegraph-mcp
codegraph-mcp agent-use status --repo C:\path\to\repo --json
codegraph-mcp agent-use index --repo C:\path\to\repo --json
codegraph-mcp agent-use mcp-config --repo C:\path\to\repo --json
```

Use a development DB only when testing CodeGraph itself. The `agent-use`
namespace resolves the production profile path for you and does not silently
fall back to repo-local `.codegraph`.

## Windows WSL2 Install Path

On Windows machines where Windows Application Control, Smart App Control, WDAC,
antivirus, or reputation checks block freshly built unsigned executables, use
WSL2 as the tested source-install path instead of trying to weaken the policy.
The verified shape is:

- clone the repo from inside Ubuntu under `/mnt/c/...` when Windows-side editors
  need to see the files
- set `CARGO_TARGET_DIR` to a directory on the Linux filesystem, such as
  `$HOME/cg-target/codegraph-mcp`
- set `CODEGRAPH_AGENT_USE_DATA_ROOT` outside both the CodeGraph checkout and
  the target repo
- run `scripts/smoke_wsl2_agent_use.sh`

```sh
cd /mnt/c/Users/<windows-user>/source/codegraph-mcp
export CARGO_TARGET_DIR="$HOME/cg-target/codegraph-mcp"
export CODEGRAPH_AGENT_USE_DATA_ROOT="$HOME/.local/share/codegraph-agent-use"
scripts/smoke_wsl2_agent_use.sh
```

The smoke builds the release binary and exercises `agent-use` against a
disposable target repo. It fails if a repo-local `.codegraph` directory appears
in the CodeGraph checkout or target repo. For a real target repo, reuse the
same release binary and external data root:

```sh
"$CARGO_TARGET_DIR/release/codegraph-mcp" agent-use status --repo /mnt/c/path/to/repo --json
"$CARGO_TARGET_DIR/release/codegraph-mcp" agent-use index --repo /mnt/c/path/to/repo --json
```

## Local Dry Runs

```powershell
codegraph-mcp config release-metadata --json
codegraph-mcp config completions --shell powershell --json
powershell -NoProfile -ExecutionPolicy Bypass -File install\install.ps1 -DryRun
```

```sh
./install/install.sh --dry-run
```

The installer templates do not download anything in dry-run mode.

## Release Metadata

Every release archive is expected to include:

- `codegraph-mcp` or `codegraph-mcp.exe`
- `README.md`
- `LICENSE`
- a SHA-256 checksum file
- SLSA-style provenance or attestation metadata when the release workflow is
  enabled

`codegraph-mcp --json --version` and
`codegraph-mcp config release-metadata --json` expose the CLI version, build
profile, git commit when injected by CI, target platform, feature flags, archive
names, checksum names, and provenance template paths.

## Distribution Targets

Release templates cover the following targets:

- Windows x64: `x86_64-pc-windows-msvc` (supported/tested by local smoke)
- Linux x64: `x86_64-unknown-linux-gnu` (supported/tested through Linux/Docker
  workflow when a daemon is available)
- macOS Apple Silicon: `aarch64-apple-darwin` (planned, not currently tested,
  no CI coverage)
- macOS Intel: `x86_64-apple-darwin` (planned, not currently tested, no CI
  coverage)

Cross-compilation details remain a release engineering concern. The local
workspace tests validate the metadata and templates without requiring those
targets to be installed.
