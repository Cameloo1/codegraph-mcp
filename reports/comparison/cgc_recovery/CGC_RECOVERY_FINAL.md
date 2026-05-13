# CGC Recovery Final

Generated: 2026-05-13 08:51:00 -05:00

## Verdict

- Official comparison verdict: blocked_by_codegraph_gate_and_cgc_incomplete
- Diagnostic CGC recovery verdict: partial_pass_install_smoke_fixture_autoresearch_timeout
- No CodeGraph superiority claim: true

## CGC Install Status

- Mode: stock_reinstall_compat_dependency
- Executable: <REPO_ROOT>/Desktop\development\codegraph-mcp\.tools\cgc_recovery\venv312_compat\Scripts\cgc.exe
- Version: 0.4.7
- Python: Python 3.12.13
- Semantics patched: false
- Harness fixed: true

## Results

| Check | Result |
| --- | --- |
| Smoke fixture | True |
| Existing 5-fixture harness | completed_not_comparable |
| Autoresearch 180s | timeout |
| Extended 600s | not_run |
| Partial CGC artifacts comparable | false |
| CodeGraph current comprehensive | fail |
| CodeGraph intended gate | fail |
| CodeGraph Graph Truth | 11/11 |
| CodeGraph Context Packet | 11/11 |
| cargo test --workspace | pass |

## Root Cause

- Old comparison/harness discovery checked CGC_COMPETITOR_BIN and PATH but not the repo-local disposable CGC virtualenv, so current reports marked CGC unavailable even though target/cgc-competitor contained executables.
- Older comparison artifacts invoked obsolete/guessed CGC syntax such as query symbols/query callers instead of the installed 0.4.7 CLI syntax: find name, find content, analyze calls, analyze callers, analyze chain.
- The existing stock venv used tree-sitter-language-pack 1.8.0 with tree-sitter 0.25.2 and failed parser initialization; the diagnostic compatibility reinstall keeps codegraphcontext 0.4.7 but pins tree-sitter-language-pack==1.6.2.
- Unisolated CGC commands tried to create <REPO_ROOT>/.codegraphcontext and hit WinError 5; the fixed harness sets a disposable CGC_COMPETITOR_HOME/USERPROFILE/LOCALAPPDATA/APPDATA.
- Early fake-home isolation broke parser cache discovery; the final smoke run uses a Windows-profile-shaped disposable home with AppData\Local parser cache.
- Autoresearch CGC now starts correctly but times out at the 180s diagnostic cap while still parsing/writing; only a small DB file plus partial WAL is produced, so it is not comparable.

## Raw Evidence

- Logs: reports/comparison/cgc_recovery/logs/
- Smoke artifact: reports/comparison/cgc_recovery/02_cgc_smoke_test.json
- Fixture artifacts: reports/comparison/cgc_recovery/artifacts/fixture_diagnostic/
- Autoresearch diagnostic: reports/comparison/cgc_recovery/07_cgc_autoresearch_diagnostic.json
- Partial Autoresearch CGC DB: .tools/cgc_recovery/home_autoresearch/.codegraphcontext/global/db/ (not comparable)

## CodeGraph Gate

Fresh comprehensive benchmark is currently fail because proof-build-only is 184297 ms against the <=60,000 ms target. Graph Truth and Context Packet are still 11/11 and DB integrity is ok.