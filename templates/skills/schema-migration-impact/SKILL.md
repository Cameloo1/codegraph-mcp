---
name: schema-migration-impact
description: Use CodeGraph migration and persistence relations to trace schema impact before changing tables or columns.
---

# Schema Migration Impact

Use this skill for database migrations, table or column changes, persistence bugs, and schema-dependent behavior.

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
1. Request context for the table, column, migration file, or persistence symbol.
2. Prefer `codegraph.find_migrations`, `codegraph.find_mutations`, and `codegraph.impact_analysis`.
3. Trace READS_TABLE, WRITES_TABLE, MIGRATES, ALTERS_COLUMN, and DEPENDS_ON_SCHEMA evidence.
4. Re-index changed files and run migration or persistence tests when practical.
