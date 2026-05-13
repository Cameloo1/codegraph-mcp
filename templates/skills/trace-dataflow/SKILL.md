---
name: trace-dataflow
description: Use CodeGraph source spans and verified dataflow paths to trace reads, writes, mutations, arguments, returns, and direct flows.
---

# Trace Dataflow

Use this skill when following how a value, request field, variable, return value, or mutation moves through code.

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
1. Request context for the variable, symbol, file, stack frame, or error message.
2. Prefer `codegraph.find_dataflow`, `codegraph.find_reads`, `codegraph.find_writes`, and `codegraph.find_mutations`.
3. Follow exact FLOWS_TO, READS, WRITES, MUTATES, ARGUMENT_N, RETURNS, and RETURNS_TO paths.
4. Re-index changed files and run the packet's recommended tests.
