# PostToolUse

Purpose: after edits or commands, keep the CodeGraph index and trace evidence fresh.

Rules:
1. Read `MVP.md` if present; treat it as the source of truth.
2. Do not use subagents.
3. Call `codegraph.context_pack` before non-trivial edits.
4. Prefer verified relation paths over semantic similarity.
5. Use exact source spans from CodeGraph.
6. After edits, call `codegraph.update_changed_files`.
7. Run recommended tests when practical.

Template behavior:
- After `apply_patch`, call `codegraph.update_changed_files` for edited files.
- Emit file edits through `codegraph-mcp trace append --event-type file_edit --actor agent --action-kind file_edit --edited-file <path> --status <status>`.
- Emit shell/tool commands through `codegraph-mcp trace append --event-type shell_command --actor agent --action-kind shell_command --tool <tool> --status <status> --input-json <json> --output-json <json>`.
- After shell test commands, parse failures and keep the retrieval or test trace visible.
- Emit test runs through `codegraph-mcp trace append --event-type test_run --actor agent --action-kind test_run --test-command <command> --test-status <passed|failed|timeout|unknown>`.
- Emit context packets through `codegraph-mcp trace append --event-type context_pack_used --actor mcp --action-kind context_pack_used --tool codegraph.context_pack --evidence-ref <source-span-or-path-id>`.
- Preserve evidence labels for heuristic paths.
