# DB Integrity Remediation

Date: 2026-05-11 00:45:08 -05:00

## Verdict

Durability remediation status: `partial pass`

The malformed-database failure mode is now guarded for tested fixture-scale cold, repeat, and single-file update operations. Cold indexes build into a same-directory temp DB and only replace the visible DB after `PRAGMA integrity_check` and `PRAGMA foreign_key_check` pass. Repeat/full indexing and incremental updates run `quick_check`/`foreign_key_check` before returning. Duplicate edge insertion is idempotent and no longer aborts repeat indexing.

Autoresearch-scale integrity is still `unknown`: two fresh cold-index attempts against `<REPO_ROOT>/Desktop\development\autoresearch-codexlab` were killed by the 10-minute command timeout before final replacement. In both attempts, the final DB path was not created, so the no-visible-replacement rule held. No CGC run, storage optimization, or benchmark scoring change was performed.

## Write Path Audit

| Path | Remediation |
| --- | --- |
| Cold index | `index_repo_to_db_with_options` now routes absent DB paths through an atomic temp DB, runs full integrity gates before and after rename, and deletes temp artifacts on normal indexing errors. |
| Repeat unchanged index | Metadata-unchanged files skip read/hash/parse. Any manifest refresh writes are transaction-wrapped. Resolver writes and repo state updates are applied through one controlled writer transaction and checked with `quick_check`. |
| Incremental update | Local file cleanup, changed-file writes, global resolver writes, repo state update, and in-transaction `quick_check` are in one transaction. Any error rolls back the complete update. |
| Delete/rename handling | `delete_facts_for_file` remains the single cleanup path for stale current facts. Incremental rename/delete tests now run with post-update integrity checks. |
| Edge insertion | `insert_edge_after_file_delete` and `upsert_edge` share one writer. Compact batch edge IDs use `ON CONFLICT(id_key) DO UPDATE`; duplicate edge insertion is idempotent. Existing object-ID edge rows remain readable. |
| Dictionary insertion | Dictionary writes remain `INSERT OR IGNORE` plus lookup and are only reached through the store writer. |
| Bulk index/index creation | Unsafe `synchronous=OFF` and `journal_mode=MEMORY` were removed. Bulk local writes run under one transaction with WAL and `synchronous=FULL`; lookup indexes are dropped inside the transaction and recreated after commit. |
| Parser workers | Workers still emit `LocalFactBundle` only. They do not open SQLite or write final cross-file graph truth. |

## Integrity Policy

- Cold index: `integrity_check` + `foreign_key_check` before visible replacement, then again on the final DB.
- Repeat/full index on existing DB: `quick_check` + `foreign_key_check` before success.
- Incremental update: `quick_check` + `foreign_key_check` inside the update transaction and again after checkpoint.
- WAL policy: WAL mode remains enabled, `synchronous=FULL` is now the default, and successful index/update paths checkpoint with `wal_checkpoint(TRUNCATE)`.
- Failed update policy: update transaction rollback leaves the previous graph visible and integrity-clean.

The current schema does not define explicit foreign keys, so `foreign_key_check` is expected to return no rows. It is still executed so future FK migrations inherit the gate.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Store duplicate/integrity tests | `cargo test -q -p codegraph-store sqlite::tests` | passed, 21/21 |
| Index durability tests | `cargo test -q -p codegraph-index` | passed, 46/47 with 1 ignored pre-existing audit-gap test |
| Full workspace tests | `cargo test --workspace` | passed |
| Graph Truth Gate | `.\target\debug\codegraph-mcp.exe bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root <REPO_ROOT>/Desktop\development\codegraph-mcp --out-json reports\audit\artifacts\db_integrity_graph_truth.json --out-md reports\audit\artifacts\db_integrity_graph_truth.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode --verbose` | failed 10/11, matching the prior known semantic failure; no new forbidden-edge/source-span/durability regression observed |
| Autoresearch cold integrity attempt | `CODEGRAPH_DB_PATH=reports\audit\artifacts\autoresearch_integrity_20260511.sqlite codegraph-mcp index <REPO_ROOT>/Desktop\development\autoresearch-codexlab --workers 4 --profile --json` | timed out before final replacement; final DB absent |

## Tests Added

- `cold_index_writes_atomic_integrity_clean_db`
- `failed_update_transaction_rolls_back_and_leaves_db_valid`
- Integrity assertions on repeat unchanged and single-file update tests.
- `compact_edge_insert_is_idempotent_for_duplicate_edges`
- Store bulk-index finish integrity preservation test updated for the new no-unsafe-VACUUM policy.

## Remaining Risk

Autoresearch-scale `integrity_check` has not completed in this shell because the cold index now exceeds the 10-minute command window in debug builds. The important safety behavior did hold: no incomplete DB was exposed. A release-build or longer-running durability job should be used to close the Autoresearch acceptance item.
