---
name: large-codebase-investigate
description: Use CodeGraph context packets and verified relation paths before investigating or editing a large codebase.
---

# Large Codebase Investigate

Use this skill when a repository question or edit needs more than one obvious file.

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
1. Request a context packet for the task and seed it with known files, symbols, stack frames, or tests.
2. Read the packet's verified paths, source spans, risks, and recommended tests before broad exploration.
3. Edit the smallest source-span-backed area that satisfies the task.
4. Re-index changed files and run the packet's recommended tests.
