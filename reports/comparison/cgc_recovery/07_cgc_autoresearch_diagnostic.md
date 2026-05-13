# 07 CGC Autoresearch Diagnostic

Generated: 2026-05-13 08:51:00 -05:00

Status: timeout

- Repo: <REPO_ROOT>/Desktop\development\autoresearch-codexlab
- Files discovered: 9474
- Timeout: 180 seconds
- CGC executable: <REPO_ROOT>/Desktop\development\codegraph-mcp\.tools\cgc_recovery\venv312_compat\Scripts\cgc.exe
- Install mode: stock_reinstall_compat_dependency
- Completed under 180s: False
- Extended 600s diagnostic: not_run
- DB base file bytes: 4096
- Partial WAL bytes: 15675444

Root cause: CGC is now installed and starts indexing Autoresearch, but it does not complete under the standard 180s cap. The artifact is partial and non-comparable.

Raw logs:

- reports/comparison/cgc_recovery/logs/phase7_autoresearch_index_180s.stdout.txt
- reports/comparison/cgc_recovery/logs/phase7_autoresearch_index_180s.stderr.txt
- .tools/cgc_recovery/home_autoresearch/cgc_debug.log