---
name: refactor-safety-check
description: Use CodeGraph to verify callers, dataflow, mutations, tests, and derived impact before refactoring.
---

# Refactor Safety Check

Use this skill before renaming, moving, splitting, merging, or simplifying code with non-local effects.

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
1. Request context for the refactor target and inspect verified callers, callees, dataflow, mutations, and tests.
2. Preserve externally visible contracts unless the task explicitly changes them.
3. Edit in source-span-backed steps and keep behavior evidence attached.
4. Re-index changed files and run the packet's recommended tests.
