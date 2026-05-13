# 01 Existing CGC Install Inventory

Generated: 2026-05-13 08:51:00 -05:00

## Install State

- Existing executables: target/cgc-competitor/.venv312-pypi/Scripts/cgc.exe and target/cgc-competitor/.venv312-pypi/Scripts/codegraphcontext.exe
- Package: codegraphcontext 0.4.7
- Python: Python 3.12.13
- PATH discovery: unavailable from ordinary where cgc / where codegraphcontext
- Git commit: unavailable in this workspace because .git/HEAD is missing.

## Failure Class

- Harness discovery gap: repo-local CGC venv was not discovered by the latest comparison report.
- Historical CLI mismatch: older reports used unsupported query forms and received command errors.
- Environment isolation gap: unisolated CGC tried to write <REPO_ROOT>/.codegraphcontext and hit WinError 5.
- Parser dependency mismatch: existing venv has tree-sitter-language-pack==1.8.0; compatible diagnostic venv uses tree-sitter-language-pack==1.6.2.

## Raw Logs

Raw logs are in reports/comparison/cgc_recovery/logs/, including version/help, pip show, and pip freeze captures.