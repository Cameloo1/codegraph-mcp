---
name: api-contract-change
description: Use CodeGraph to trace API contract callers, exposed routes, tests, and affected dataflow before changing interfaces.
---

# API Contract Change

Use this skill when changing a public function, route, schema, type, request, response, event, or package interface.

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
1. Request context for the API symbol, file, or route.
2. Trace callers, callees, exposed endpoints, event consumers, tests, and migrations.
3. Edit only the verified contract surface and directly affected call sites.
4. Re-index changed files and run the packet's recommended tests.
