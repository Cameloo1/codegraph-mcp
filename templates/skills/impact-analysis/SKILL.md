---
name: impact-analysis
description: Use CodeGraph impact analysis to report blast radius across calls, mutations, APIs, auth, events, schema, and tests.
---

# Impact Analysis

Use this skill when asked what a file, symbol, API, table, event, or behavior change may affect.

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
1. Run `codegraph.impact_analysis` for the target file or symbol.
2. Group results by callers, mutations, schema, API/auth/security, events, and tests.
3. Report evidence with source spans and exactness labels.
4. Re-index changed files if the analysis leads to edits.
