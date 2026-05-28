# README Terminal Demo Recording

This folder is for producing a fresh terminal animation for the root README.
Generated `.cast`, `.svg`, logs, and DB files should stay local until a cleaned
animation is intentionally promoted.

The old restored SVG was useful as a style reference, but it contained stale
commands and a machine-local path. Use this script path instead so a new
recording reflects current CLI behavior.

## Record

Build the release binary first:

```powershell
cargo build --release --bin codegraph-mcp
```

Record with asciinema from the repository root:

```powershell
asciinema rec reports/audit/artifacts/readme_terminal_demo/codegraph_quickstart.cast
```

Inside the recording session, run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\readme-terminal-demo\play_quickstart_demo.ps1
```

End the asciinema recording when the script finishes.

## Render

Render the `.cast` to SVG with your preferred asciinema renderer. Keep the
rendered SVG under `reports/audit/artifacts/readme_terminal_demo/` until it has
been reviewed for:

- current command syntax;
- no machine-local paths;
- no secrets;
- no unsupported performance or external-score claims;
- no normal `.codegraph` mutation.

Only after review should the final SVG be copied to
`docs/assets/readme/codegraph_terminal.svg`, referenced from `README.md`, and
added to `scripts/check_readme_artifacts.py`.
