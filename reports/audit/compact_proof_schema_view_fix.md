# Compact Proof Schema/View Compatibility Fix

Generated: 2026-05-12 19:24:58 -05:00

## Verdict

**Pass.** The default compact proof query surface no longer fails with `sqlite error in view file_instance: no such column: files.file_id`.

## Root Cause

The workspace default `.codegraph/codegraph.sqlite` reproduced the failure as a legacy schema DB:

- `PRAGMA user_version = 4`
- `files` still used legacy columns: `path_id`, `file_hash`, `language_id`, `size_bytes`, `indexed_at_unix_ms`, `metadata_json`
- `file_instance` had already been created with the compact view definition, referencing `files.file_id`

SQLite validates dependent views during some schema rewrites. The migration needed to normalize `files`, but the stale `file_instance` view made the migration fail before `files.file_id` could be created.

## Fix

- Drop stale compatibility views before migration work that can rewrite `files` or graph fact tables.
- Recreate `file_instance` after file-hash normalization with the current compact-proof schema:
  - `files.file_id`
  - `files.path_id`
  - `path_dict.value AS repo_relative_path`
  - `files.content_template_id`
  - `files.content_hash`
- Require fast-path schema checks to verify critical compatibility views compile, not only that they exist.
- Added read-only `codegraph-mcp audit schema-check --db <path>` to compile:
  - compact proof views
  - symbol lookup SQL
  - text/FTS query SQL
  - bounded relation query SQL
  - context PathEvidence lookup SQL
  - source-span batch-load SQL

## Verification

| Gate | Result |
| --- | --- |
| Local stale DB migration | pass; schema upgraded to version 18 |
| `audit schema-check` on `.codegraph/codegraph.sqlite` | pass; 0 failures |
| Symbol query smoke | pass; `query symbols login` returned hits |
| Text query smoke | pass; `query text import` returned hits |
| Bounded relation smoke | pass; `query callers sanitize` returned CALLS evidence |
| Context packet smoke | pass; `context-pack --seed login` returned verified path evidence |
| DB integrity | pass; `PRAGMA integrity_check` and `quick_check` returned `ok` |
| Graph Truth Gate | pass; 11/11 |
| Context Packet Gate | pass; 11/11 |
| `cargo test --workspace` | pass |

Artifacts:

- `reports/audit/artifacts/compact_proof_schema_check.json`
- `reports/audit/artifacts/compact_proof_schema_check.md`
- `reports/audit/artifacts/compact_proof_schema_view_fix_graph_truth.json`
- `reports/audit/artifacts/compact_proof_schema_view_fix_context_packet.json`

## Storage Safety Questions

1. Did Graph Truth still pass? **Yes, 11/11.**
2. Did Context Packet quality still pass? **Yes, 11/11.**
3. Did proof DB size decrease? **No storage-size change was attempted.**
4. Did removed data move to a sidecar, become derivable, or get proven unnecessary? **No data was removed; only stale compatibility views are dropped and recreated from canonical tables.**

