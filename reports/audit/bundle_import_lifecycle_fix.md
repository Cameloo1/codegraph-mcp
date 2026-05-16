# Bundle import lifecycle fix

Generated at: 2026-05-15T23:00:37-05:00

## Source-of-truth status

- `MVP.md` is missing from the active worktree at `C:\Users\wamin\.codex\worktrees\4160\codegraph-mcp\MVP.md`.
- The canonical sibling copy at `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read first and treated as the project source of truth for this pass.
- `reports/audit/db_lifecycle_surface_inventory.md` and `reports/audit/lifecycle_preflight_api_design.md` were read before changing bundle import behavior.

## Inspection result

The reported bundle import risk was present in the active checkout before this fix. `bundle import` accepted a bundle, opened the current/default DB directly, and upserted imported files/entities/edges into that DB before writing a replacement passport. That made foreign or stale graph facts capable of contaminating the current repo DB without an explicit import mode.

## Implementation summary

Changed files for this pass:

- `crates/codegraph-cli/src/lib.rs`
- `crates/codegraph-cli/tests/cli_smoke.rs`

Bundle metadata is now schema version `2` and includes:

- `repo_identity`
- `canonical_repo_root`
- `repo_head`
- `scope_hash`
- `scope_policy_json`
- `storage_mode`
- `db_schema_version`
- `graph_digest`
- `created_at_unix_ms`

Default import semantics are now fresh-only:

- Import into a missing or empty DB can proceed.
- Import into a non-empty DB fails unless `--replace` or `--merge` is explicit.
- Foreign repo identity is rejected by default.
- Import builds into a same-directory temp DB first.
- Temp DB is validated with schema/passport/integrity lifecycle preflight before publish.
- Final DB is validated again after publish.
- Failed import removes temp artifacts and leaves the old DB untouched.

Explicit `--replace` semantics:

- Allows replacement of an existing DB.
- Publishes via backup-and-rename atomic path.
- Rolls back the old DB family if final publish fails.
- Removes backup files after a successful publish.

Explicit `--merge` semantics:

- The flag is accepted only as an explicit diagnostic mode.
- It does not mutate the DB yet.
- Output is labeled `claimable=false`, `diagnostic_only=true`, and `database_mutated=false`.
- Mutation remains refused until per-fact provenance and claimability semantics are defined.

## Important compatibility note

Old bundle schema version `1` is now rejected by import. That is intentional because schema `1` does not carry the required lifecycle metadata needed to prove repo identity, scope, storage mode, schema, and graph digest compatibility.

## Regression coverage

Added or updated CLI smoke coverage for:

- Export/import round trip with schema `2` metadata and valid passport.
- Import into non-empty DB fails by default.
- Import into non-empty DB succeeds with `--replace`.
- Foreign repo bundle fails by default.
- Failed import leaves the old DB untouched.
- Explicit `--merge` is diagnostic-only and non-mutating.
- Schema mismatch remains rejected.

Focused command:

```powershell
cargo test -p codegraph-cli bundle
```

Result: passed. The command matched 7 bundle tests and all 7 passed.

## Verification

```powershell
cargo fmt
```

Result: passed, with the existing Windows warning `could not canonicalize path C:\Users\wamin`.

```powershell
git diff --check
```

Result: passed. Git reported CRLF normalization warnings only.

```powershell
cargo test -p codegraph-cli bundle
```

Result: passed. `7 passed; 0 failed`.

```powershell
cargo test --workspace
```

Result: failed twice in the same unrelated UI server test:

- `tests::ui_server_starts_and_status_endpoint_uses_real_index`
- Error: `read HTTP response: Os { code: 10054, kind: ConnectionReset, message: "An existing connection was forcibly closed by the remote host." }`

The exact failed test was rerun separately:

```powershell
cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact
```

Result: passed. This leaves the full workspace command not green in-suite, while the bundle import regression suite and the exact failing UI test both pass in isolation.

## Dirty/generated state

The active worktree already contained broader uncommitted lifecycle source changes and many untracked audit reports before this pass. No staging was performed.

Known generated or audit artifacts remain untracked under `reports/audit/`. Generated DBs, WAL/SHM files, target outputs, raw logs, and CGC payloads were not intentionally added or staged by this pass.

## Acceptance status

- Bundle import can no longer silently merge bundle facts into the current DB by default.
- Non-empty DB writes require explicit `--replace` or explicit diagnostic `--merge`.
- Foreign repo imports fail by default.
- Fresh/replace imports validate a temp DB before atomic publish.
- Merge remains non-mutating and non-claimable.
- Focused regression tests pass.
- Full workspace tests are not green because of the repeated in-suite UI socket reset documented above.
