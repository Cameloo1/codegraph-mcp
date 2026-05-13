# CodeGraph vs CGC Latest

Generated: 2026-05-13 08:51:00 -05:00

## Verdict

Official comparison: blocked_by_codegraph_gate_and_cgc_incomplete.

Diagnostic recovery: partial_pass_install_smoke_fixture_autoresearch_timeout.

No CodeGraph superiority claim is made.

## CodeGraph Status

- Fresh comprehensive verdict: fail
- Blocking target: proof-build-only 184297 ms > 60,000 ms
- Graph Truth: 11/11
- Context Packet: 11/11
- DB integrity: ok
- Proof DB: 171.184 MiB
- cargo test --workspace: pass

## CGC Status

- Mode: stock_reinstall_compat_dependency
- Executable: <REPO_ROOT>/Desktop\development\codegraph-mcp\.tools\cgc_recovery\venv312_compat\Scripts\cgc.exe
- Smoke test: pass
- Fixture diagnostic: completed_not_comparable
- Autoresearch 180s: timeout
- Extended diagnostic: not run
- Partial artifacts comparable: false

## Why This Is Not A Win Claim

- CodeGraph current fresh comprehensive gate fails the proof-build-only target.
- CGC Autoresearch did not complete under the controlled 180s cap.
- CGC fixture output is not graph-truth/source-span/path comparable.
- Partial CGC DB/WAL files are not final artifacts.