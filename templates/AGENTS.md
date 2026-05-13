# CodeGraph project rules

For non-trivial debugging, refactoring, auth, security, dataflow, schema, event-flow, or impact-analysis tasks:

1. Read `MVP.md` if present; treat it as the source of truth.
2. Do not use subagents.
3. Call `codegraph.context_pack` before editing.
4. Prefer verified relation paths over semantic similarity.
5. Use exact source spans from CodeGraph.
6. For auth/security tasks, call `codegraph.find_auth_paths`.
7. For mutation or persistence tasks, call `codegraph.find_mutations` or `codegraph.impact_analysis`.
8. After edits, call `codegraph.update_changed_files`.
9. Run recommended tests when practical.
