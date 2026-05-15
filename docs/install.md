# Install And Release Notes

The public setup contract is the root `README.md`. CodeGraph remains
Rust-first, local-first, and single-agent only. No installer, shell completion,
or release template changes that workflow.

## Install Paths

Current release templates document these install paths. Unless a GitHub release
is actually published for a version, the checkout build is the authoritative
local install path.

- GitHub release archives from `dist/archive-manifest.json`
- PowerShell installer template: `install/install.ps1`
- POSIX shell installer template: `install/install.sh`
- `cargo install --path crates/codegraph-cli` from a checkout
- cargo-binstall metadata template: `dist/cargo-binstall.example.toml`
- Homebrew formula template: `packaging/homebrew/codegraph-mcp.rb`

The npm wrapper path is intentionally not included because it would add
packaging surface without improving the Rust-first core.

## Recommended Local Agent Setup

For routine Codex/agent use from this checkout:

```powershell
cargo build --release --bin codegraph-mcp
.\scripts\codegraph-profile.ps1 -Profile prod-agent -Action status
.\scripts\codegraph-profile.ps1 -Profile prod-agent -Action index
```

That profile uses `target\release\codegraph-mcp.exe` and stores the agent-use
DB under LocalAppData instead of inside the source tree. Use the development
profile only when testing CodeGraph itself.

## Local Dry Runs

```powershell
codegraph-mcp config release-metadata --json
codegraph-mcp config completions --shell powershell --json
codegraph-mcp bench synthetic-index --output-dir target\phase29-index-speed --files 250
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
