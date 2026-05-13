---
name: test-impact-analysis
description: Use CodeGraph test relations to select targeted tests for a change or failure investigation.
---

# Test Impact Analysis

Use this skill when selecting tests, explaining failed tests, or mapping code changes to coverage evidence.

Rules:
1. Read `MVP.md` first if it is available; treat it as the source of truth.
2. Do not use subagents.
3. Call `codegraph.context_pack` before non-trivial edits.
4. Prefer verified relation paths over semantic similarity.
5. Use exact source spans returned by CodeGraph when reading or editing code.
6. For auth/security tasks, call `codegraph.find_auth_paths`.
7. For mutation or persistence tasks, call `codegraph.find_mutations` or `codegraph.impact_analysis`.
8. After edits, call `codegraph.update_changed_files`.
9. Run recommended tests when practical.

Workflow:
1. Request context for changed files, symbols, failed test names, or stack traces.
2. Prefer verified TESTS, ASSERTS, MOCKS, STUBS, COVERS, and FIXTURES_FOR paths.
3. Choose the smallest test set that covers the verified blast radius.
4. Re-index changed files after edits and record pass/fail output.
