# UserPromptSubmit

Purpose: classify the prompt and remind the agent which CodeGraph tool applies.

Rules:
1. Read `MVP.md` if present; treat it as the source of truth.
2. Do not use subagents.
3. Call `codegraph.context_pack` before non-trivial edits.
4. Prefer verified relation paths over semantic similarity.
5. Use exact source spans from CodeGraph.
6. After edits, call `codegraph.update_changed_files`.
7. Run recommended tests when practical.

Template behavior:
- Classify prompt mode as small-edit, large-codebase, auth, security, dataflow, migration, event-flow, or test-impact.
- Add a short reminder to call the matching CodeGraph tool before broad edits.
