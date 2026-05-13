# SessionStart

Purpose: detect the local CodeGraph index and add a small context reminder.

Rules:
1. Read `MVP.md` if present; treat it as the source of truth.
2. Do not use subagents.
3. Call `codegraph.context_pack` before non-trivial edits.
4. Prefer verified relation paths over semantic similarity.
5. Use exact source spans from CodeGraph.
6. After edits, call `codegraph.update_changed_files`.
7. Run recommended tests when practical.

Template behavior:
- Detect `.codegraph/codegraph.sqlite` when present.
- Add this reminder: `CodeGraph index available; use codegraph.context_pack for large-repo tasks.`
