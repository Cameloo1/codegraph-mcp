# Fresh-Clone Smoke Reports

These reports are produced by:

- `scripts/smoke_fresh_clone.ps1` on Windows.
- `scripts/smoke_fresh_clone.sh` on Linux.

Each script creates a disposable filesystem snapshot of the current working tree in a temporary directory whose path contains spaces, then runs:

1. `cargo metadata --workspace`
2. `cargo build --workspace`
3. `cargo test --workspace`
4. `cargo run --bin codegraph-mcp -- --help`

The scripts write per-run logs under this directory:

- `environment.txt`
- `copy.log`
- `cargo_metadata.log`
- `cargo_build.log`
- `cargo_test.log`
- `codegraph_mcp_help.log`
- `summary.json`

The smoke is intentionally limited to fresh-clone buildability and CLI startup. It does not require CGC, Autoresearch, external benchmark artifacts, or network access beyond normal Cargo dependency resolution.

The scripts first run the required `cargo metadata --workspace` command. Some Cargo versions reject `--workspace` for `cargo metadata` because metadata is already workspace-scoped; when that exact incompatibility is detected, the scripts record it in `cargo_metadata.log` and continue with `cargo metadata --format-version 1`.

On Windows systems with script execution disabled, run the PowerShell smoke with `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke_fresh_clone.ps1`. This does not persistently change execution policy.

The `summary.json` file records whether a path with spaces was tested. If a platform cannot create such a path, the script must report that explicitly instead of implying coverage.
