# Stop

Purpose: record a compact task trace before the session ends.

Rules:
1. Read `MVP.md` if present; treat it as the source of truth.
2. Do not use subagents.
3. Call `codegraph.context_pack` before non-trivial edits.
4. Prefer verified relation paths over semantic similarity.
5. Use exact source spans from CodeGraph.
6. After edits, call `codegraph.update_changed_files`.
7. Run recommended tests when practical.

Template behavior:
- Record task, retrieved paths, edited files, tests run, pass/fail, and trace metrics.
- Emit final status through `codegraph-mcp trace append --event-type run_end --actor system --action-kind run_end --result-status <ok|failed|timeout|unknown> --status <ok|failed|timeout|unknown>`.
- Validate replayability through `codegraph-mcp trace replay --events target/codegraph-agent-runs/<run_id>/events.jsonl` when practical.
- Keep source-span and exactness evidence attached to the summary.
