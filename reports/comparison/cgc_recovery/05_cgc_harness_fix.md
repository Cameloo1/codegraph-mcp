# 05 CGC Harness Fix

Generated: 2026-05-13 08:51:00 -05:00

The CodeGraph harness was fixed to invoke CGC correctly; CGC retrieval/indexing semantics were not changed.

## Changes

- Added repo-local discovery for .tools/cgc_recovery/venv312_compat/Scripts/cgc.exe.
- Preserved legacy discovery for target/cgc-competitor/.venv312-pypi/Scripts/cgc.exe and codegraphcontext.exe.
- Added isolated home handling through CGC_COMPETITOR_HOME / runner home override.
- Set HOME, USERPROFILE, LOCALAPPDATA, APPDATA, DEFAULT_DATABASE, and UTF-8 environment for CGC subprocesses.
- Added test coverage for repo-local discovery candidates.

## Verification

- cargo test --workspace: pass.
- CGC smoke test: pass.
- Fixture diagnostic harness invoked CGC 5/5 times without skip.