---
name: event-flow-debug
description: Use CodeGraph event-flow paths to debug publish, emit, consume, listen, spawn, and await behavior.
---

# Event Flow Debug

Use this skill when debugging asynchronous behavior, events, queues, listeners, publishers, subscriptions, or task spawning.

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
1. Request context for the event name, handler, publisher, or failure symptom.
2. Prefer `codegraph.find_event_flow` and verified PUBLISHES, EMITS, CONSUMES, LISTENS_TO, SPAWNS, and AWAITS paths.
3. Follow exact source spans from publisher through handler before editing.
4. Re-index changed files and run the packet's recommended tests.
