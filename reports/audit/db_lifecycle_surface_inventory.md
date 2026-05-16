# DB lifecycle surface inventory

Generated at: 2026-05-15T22:01:40-05:00

Scope: audit-only inventory of current DB open surfaces in `C:\Users\wamin\.codex\worktrees\4160\codegraph-mcp`. No production behavior was changed.

## Source-of-truth status

- `MVP.md` is missing from this active worktree, so it could not be read from `C:\Users\wamin\.codex\worktrees\4160\codegraph-mcp\MVP.md`.
- The canonical sibling checkout copy at `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read for project intent. The current inventory is still based on the active worktree's actual code.
- Relevant MVP directives preserved for this pass: SQLite is the local MVP store, graph truth remains authoritative, vectors are not truth, and audit artifacts must not weaken gates or silently hide unsafe behavior.

## Current state summary

Current checkout already has the shared DB lifecycle/passport preflight for the main read and MCP surfaces:

- CLI `status` uses `inspect_read_db_lifecycle_preflight` before opening the store.
- CLI default `query`, `context-pack`, and `impact` call `read_db_lifecycle_guard`.
- `open_existing_store` now guards default/env DB reads before opening `SqliteGraphStore`.
- MCP `status` and MCP read tools call `inspect_db_lifecycle_preflight` through the shared preflight path.
- Incremental update paths call `require_db_lifecycle_preflight` before opening the mutation-capable store.

Confirmed remaining concern surfaces are narrower than the original broad report, but not zero:

- `query unresolved-calls --db <path>` can open a subcommand DB path that was not the exact path preflighted by the top-level query wrapper.
- `/api/unresolved-calls` in the local UI calls the same unguarded unresolved-calls helper outside the top-level query guard.
- Persistent `watch` parses `--db` but does not pass it to `watch_repo`, and `watch_repo` opens the default/env DB before any explicit lifecycle preflight.
- `doctor` still uses a direct mutation-capable `SqliteGraphStore::open` as its DB health probe.
- `bundle import` opens and mutates the default/env DB directly before writing a replacement passport.
- Several benchmark/audit helper opens intentionally inspect arbitrary DBs, but their outputs should be treated as diagnostic unless the calling benchmark has a freshness/passport contract.

## Inventory

| ID | File/function | Command/API/benchmark | DB path source | User override | Role | Guard used | Exact path guarded | Open mode | Claimability | Risk |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| store.primitive.open | `crates/codegraph-store/src/sqlite.rs:187` `SqliteGraphStore::open` | Shared primitive used by all store callers | Caller-provided path | Caller-dependent | Store open and migration primitive | None inside primitive | Caller-dependent | Mutation-capable; `Connection::open`, configure, migrate | Not output-producing itself | Not applicable primitive |
| store.primitive.memory | `crates/codegraph-store/src/sqlite.rs:205` `open_in_memory` | Synthetic benchmarks and tests | In-memory | No | Ephemeral store | None | Not applicable | Mutation-capable memory DB | Diagnostic/test only | Not applicable |
| store.preflight | `crates/codegraph-store/src/sqlite.rs:2423` `inspect_db_preflight` | Shared lifecycle/passport guard | Exact caller-provided DB path | Caller-dependent | Repo/schema/storage/scope/passport/integrity inspection | This is the guard | Yes | Read-only SQLite flags | Guard evidence is claimable when caller enforces it | Already fixed |
| index.lifecycle | `crates/codegraph-index/src/lib.rs:849` `index_repo_to_db_with_options` | `index`, MCP `index_repo`, fresh benchmark builds | Default DB, env/global DB, CLI/MCP/benchmark explicit DB | Yes | Write/index | `inspect_db_preflight` plus `decide_db_lifecycle` | Yes | Mutation-capable after lifecycle decision | Claimable when decision says claimable | Already fixed |
| index.open-existing | `crates/codegraph-index/src/lib.rs:1072` `index_repo_to_existing_db_with_options` | Incremental reuse and cold-temp index internals | Normalized lifecycle-approved path | Yes through caller | Write/index | Caller lifecycle decision | Yes through caller | Mutation-capable | Claimable through index summary lifecycle evidence | Already fixed |
| index.atomic-temp | `crates/codegraph-index/src/lib.rs:1793` and `1851` | Fresh atomic proof/audit builds | Hidden temp DB then final DB | Yes through caller | Publish gate and final artifact check | Fresh build decision plus quick/full integrity gate | Yes through caller | Mutation-capable | Claimable when post-index check passes | Already fixed |
| index.update | `crates/codegraph-index/src/lib.rs:4192` / `4212` | CLI watch once, MCP update, benchmark update loop | Default DB or explicit update DB | Yes through caller | Incremental mutation | `require_db_lifecycle_preflight` and passport scope baseline | Yes | Mutation-capable | Claimable for update outputs if preflight passes | Already fixed |
| cli.status | `crates/codegraph-cli/src/lib.rs:836` / `849` / `861` | `codegraph-mcp status [repo]` | `default_db_path(repo_root)` including `CODEGRAPH_DB_PATH` global/env | Global `--db`/env only | Status/count inspection | `inspect_read_db_lifecycle_preflight` before store open | Yes | Mutation-capable store open after safe preflight | Claimable status if safe | Already fixed |
| cli.doctor | `crates/codegraph-cli/src/lib.rs:920` / `955` | `codegraph-mcp doctor [repo]` | `default_db_path(repo_root)` including `CODEGRAPH_DB_PATH` | Global `--db`/env only | Diagnostic DB-open probe | None; opens store and checks `schema_version` | No | Mutation-capable store open | Diagnostic, but can report DB `ok` without passport | P0 confirmed unsafe diagnostic open |
| cli.query-wrapper | `crates/codegraph-cli/src/lib.rs:1112` / `1119` | `query symbols/text/files/references/definitions/callers/callees/chain/path/unresolved-calls` | `default_db_path(current_repo_root)` including `CODEGRAPH_DB_PATH` | Global `--db`/env only | Shared read guard | `read_db_lifecycle_guard` | Yes for default/env DB | Guard only; subcommands open separately | Claimable when subcommand uses same path | Already fixed except unresolved subcommand `--db` |
| cli.open-existing | `crates/codegraph-cli/src/lib.rs:11186` / `11194` | CLI symbol/text/file/relation/path reads, impact internals, UI endpoints, bundle export | `default_db_path(repo_root)` including env/global DB | Global `--db`/env only | Shared store read helper | `read_db_lifecycle_guard` | Yes | Mutation-capable store open after safe preflight | Claimable when caller output is claimable | Already fixed |
| cli.context-pack | `crates/codegraph-cli/src/lib.rs:1263` / `1270` / `1277`; `11334` | `context-pack` / `context` | `default_db_path(repo_root)` including env/global DB | Global `--db`/env only | Context packet read | `read_db_lifecycle_guard` before read-only connection | Yes | Read-only SQLite URI with `query_only=ON` | Claimable if guard safe | Already fixed |
| cli.context-pack-explain | `crates/codegraph-cli/src/lib.rs:12412` | `context-pack --profile` explain plans | Same guarded context-pack DB path | No separate override | Diagnostic explain plan | Caller has already run read guard | Yes through caller | Read-only | Diagnostic sub-output | Already fixed |
| cli.impact | `crates/codegraph-cli/src/lib.rs:1485` / `1495` / `1513` | `impact <target>` | `default_db_path(repo_root)` including env/global DB | Global `--db`/env only | Graph traversal read | `read_db_lifecycle_guard` then `open_existing_store` | Yes | Mutation-capable store open after safe preflight | Claimable if guard safe | Already fixed |
| cli.query-unresolved-subdb | `crates/codegraph-cli/src/lib.rs:8914` / `8961` / `8995` / `9012` / `9047` | `query unresolved-calls --db <path>` | Subcommand `--db`; otherwise default/env DB | Yes, subcommand-specific | SQL read and optional bounded source scan | Top-level query guard checks default/env DB, not necessarily this subcommand path | No when subcommand `--db` differs | Read-only connection, plus mutation-capable store if `--source-scan` | Can look claimable but exact opened DB may be unguarded | P0 confirmed unsafe read path |
| ui.status-paths | `crates/codegraph-cli/src/lib.rs:7669`, `7685`, `7784`, `7881`, `7891` | Local Proof-Path UI `/api/status`, `/api/path-graph`, `/api/symbol-search`, `/api/impact`, `/api/context-pack` | `default_db_path(repo_root)` including env/global DB | No route-level DB override | Local API read | `open_existing_store` for DB-backed routes | Yes | Mutation-capable store open after safe preflight | Local diagnostic/UI, safe relative to guard | Already fixed |
| ui.source-span | `crates/codegraph-cli/src/lib.rs:7792` | Local Proof-Path UI `/api/source-span` | No DB | No | Source file read only | Not applicable | Not applicable | File read | Diagnostic/UI | Not applicable |
| ui.unresolved-calls | `crates/codegraph-cli/src/lib.rs:7853` / `7866` | Local Proof-Path UI `/api/unresolved-calls` | Default/env DB via `query_unresolved_calls` | No route-level DB override | SQL unresolved-call read | Does not pass through top-level query guard | No | Read-only connection | Local diagnostic/UI; may disagree with guarded reads | P0 confirmed unsafe read path |
| watch.once | `crates/codegraph-cli/src/lib.rs:7396` / `7402` | `watch --once --changed ... [--db <path>]` | `--db` or default/env via update helper | Yes | One-shot update | `update_changed_files_to_db` preflight | Yes | Mutation-capable after preflight | Claimable update JSON if preflight passes | Already fixed |
| watch.persistent | `crates/codegraph-cli/src/lib.rs:7418` / `7424` / `7432` / `7478`; parser at `9997` / `10017` | Long-running `watch [repo] [--db <path>]` | `watch_repo` uses default/env DB; parsed local `--db` is not passed | Parsed but ignored unless global/env | Long-running cache refresh then update loop | No guard before initial store open; later update helper is guarded | No for startup open; no for local `--db` exact path | Mutation-capable store open before update events | Operational path, not benchmark claim | P0 confirmed unsafe startup open; P3 ignored local `--db` |
| bundle.export | `crates/codegraph-cli/src/lib.rs:8357` / `8360` | `bundle export --output ...` | Default/env DB | Global `--db`/env only | Export/read | `open_existing_store` | Yes | Mutation-capable store open after safe preflight | Claimable export if guard safe | Already fixed |
| bundle.import | `crates/codegraph-cli/src/lib.rs:8394` / `8411` / `8431` | `bundle import repo.cgc-bundle` | Default/env DB | Global `--db`/env only | Import/write | Bundle schema check and post-write quick integrity; no preflight before mutating DB | No | Mutation-capable transaction | Import output is not safely claimable before lifecycle hardening | P0 confirmed unsafe import path |
| audit.read-only | `crates/codegraph-cli/src/audit.rs:860`, `887`, `929`, `952`, `983`, `1331`, `1392`, `3172`, `4230`, `5047` | `audit storage`, `schema-check`, `sample-edges`, `sample-paths`, `relation-counts` | `--db`, `CODEGRAPH_DB_PATH`, or `.codegraph/codegraph.sqlite` | Yes | Diagnostic/audit read | None; intentionally opens arbitrary DB read-only | No | Read-only | Diagnostic unless calling benchmark wraps freshness/lifecycle | P2 diagnostic/claimability issue |
| audit.storage-experiments | `crates/codegraph-cli/src/audit.rs:1582`, `2582`, `2730` | `audit storage-experiments --db ...` | Original `--db` copied to workdir | Yes | Mutate copied DB for experiments | Original read is not lifecycle-guarded; mutation is copy-only with path refusal for original | Original no; copy yes by construction | Original read-only/copy; copied DB mutation-capable | Diagnostic only | P2/P3 diagnostic, no original DB mutation |
| cli.query-surface-bench | `crates/codegraph-cli/src/lib.rs:2147`, `2159`, `2182`, `5333`, `5342` | `bench query-surface [--repo] [--db] [--fresh]` and comprehensive query-surface artifact | Fresh DB or existing `--db`/default DB | Yes | Benchmark read surface | Fresh path uses index lifecycle; existing path directly opens read-only | Partial | Read-only for report; fresh build mutates generated DB | Benchmark output should be claimable only when artifact freshness/lifecycle is established | P2 diagnostic/claimability issue |
| cli.proof-build-bench | `crates/codegraph-cli/src/lib.rs:2200`, `2261`, `2399`, `3632`, `3640` | `bench proof-build-only`, `bench proof-build-validated` | Explicit `--db` or default index DB | Yes | Production proof-build metric and metadata | Index lifecycle for built DB; read-only metadata helpers after build | Yes for built DB | Mutation-capable index, read-only metadata | Release binary required for production claimability; debug is diagnostic-only | Already fixed for binary claimability |
| cli.comprehensive-fresh | `crates/codegraph-cli/src/lib.rs:2919`, `2933`, `2971`, `2974`, `2977` | `bench comprehensive --fresh` | Generated artifact DB under output dir | Yes via output/repo options | Full benchmark gate | Fresh index lifecycle plus storage audit/integrity | Yes for build; audit helper has no passport guard | Mutation-capable generated DB, read-only audit | Claimable only when release/freshness metadata says claimable | Already fixed for fresh build/binary metadata; audit read remains diagnostic sub-surface |
| cli.comprehensive-existing | `crates/codegraph-cli/src/lib.rs:3076`, `3102`, `3113`, `3117`, `3120` | `bench comprehensive --use-existing-artifact <db>` | User-provided artifact DB | Yes | Existing artifact benchmark reuse | Metadata/schema/storage/integrity checks, but no shared passport preflight | No | Read-only audit/schema checks | Existing-artifact result can be stale/non-claimable; `--fail-on-stale-artifact` helps but does not prove repo passport scope | P2 diagnostic/claimability issue |
| cli.update-integrity | `crates/codegraph-cli/src/lib.rs:6882`, `7049`, `7086`, `7211`, `7230`, `12915`, `13191`, `13205`, `13216`, `13221`, `13228`, `13244`, `13254` | `bench update-integrity` | Harness-generated DBs or copied `--autoresearch-seed-db` | Yes for seed DB and workdir | Benchmark repeat/update/integrity harness | Fresh/repeat/update calls use index/update lifecycle; helper hash/count/integrity opens do not | Partial | Mutation-capable helper opens; generated/copy DBs | Diagnostic/benchmark; claimability depends on harness artifact freshness | P2 diagnostic/claimability issue |
| bench.final-acceptance | `crates/codegraph-bench/src/lib.rs:1753`, `1778`, `1780` | Internal final acceptance gate | Generated fixture DBs | No external DB override here | Benchmark comparison snapshot | DBs are just generated by `index_repo_to_db`/MCP index in same run | Yes by construction, not separately preflighted | Mutation-capable store read after generation | Diagnostic benchmark evidence | P2 diagnostic/benchmark surface |
| bench.synthetic-memory | `crates/codegraph-bench/src/lib.rs:3457` | Synthetic benchmark suite | In-memory DB | No | Synthetic fixture indexing | Not applicable | Not applicable | In-memory mutation | Diagnostic benchmark | Not applicable |
| bench.graph-truth | `crates/codegraph-bench/src/graph_truth.rs:1053`, `1357` | `bench graph-truth`, `bench context-packet` fixture cases | Generated fixture/audit DB | No user DB path | Benchmark query/evaluation | DB generated by benchmark case indexing | Yes by construction, not shared preflight | Mutation-capable store open after generated index | Gate evidence for fixtures; not a user DB read surface | P2 diagnostic/benchmark surface |
| bench.retrieval-ablation | `crates/codegraph-bench/src/retrieval_ablation.rs:501`, `503` | `bench retrieval-ablation` | Unique generated DB path | No user DB path | Benchmark read/evaluation | DB generated by benchmark case indexing | Yes by construction, not shared preflight | Mutation-capable store open after generated index | Diagnostic benchmark | P2 diagnostic/benchmark surface |
| tests.direct-opens | Multiple `#[cfg(test)]` sections in CLI/index/MCP/store/parser crates | Unit and smoke tests | Temp fixture DBs | Test-only | Fixture setup/assertions | Test-specific | Test-specific | Mixed | Not production output | Not applicable |

## Prioritized fix matrix

### P0: confirmed unsafe read/write path

1. `query unresolved-calls --db <path>`: the top-level `query` wrapper preflights `default_db_path(current_repo_root)`, but the unresolved-calls helper can open `options.db_path` directly. The exact path is not guarded if subcommand `--db` differs from global/env DB.
2. UI `/api/unresolved-calls`: calls `query_unresolved_calls` directly, outside the top-level query lifecycle guard.
3. Long-running `watch`: `watch_repo` opens `SqliteGraphStore::open(default_db_path(&repo_root))` before any lifecycle preflight. Local `watch --db` is parsed but not passed into long-running watch.
4. `doctor`: uses `SqliteGraphStore::open(&db_path).and_then(|store| store.schema_version())` as its database check and can report `ok` without passport validation.
5. `bundle import`: mutates the default/env DB directly after bundle schema validation. It runs quick integrity and writes a passport after import, but there is no pre-mutation lifecycle decision.

### P1: likely unsafe but needs proof

- Persistent `watch --db` has a dual problem: the explicit local `--db` path is ignored in the persistent path, and the default/env startup open is unguarded. A small regression test should prove both exact-path handling and unsafe DB behavior before fixing.
- Existing-artifact benchmark reuse does not run shared passport preflight. It has metadata/schema/integrity freshness checks, so the remaining risk is claimability drift rather than a normal production read bypass.

### P2: diagnostic/benchmark claimability issue

- Audit commands intentionally open arbitrary DBs read-only. Their reports should stay diagnostic unless a caller separately establishes lifecycle/freshness/passport safety.
- `bench query-surface` on an existing `--db` opens read-only without lifecycle preflight.
- `bench comprehensive --use-existing-artifact` validates metadata/schema/storage/integrity but not shared repo/scope passport compatibility.
- `bench update-integrity` helper hash/count/integrity opens work on generated or copied DBs; update mutation calls are guarded, but helper outputs should remain benchmark-scoped.
- Internal benchmark direct opens in `codegraph-bench` are generated-fixture surfaces, not user DB read paths.

### P3: usability clarity issue

- `watch --db` is documented and parsed for both modes, but only `--once` honors the local option. Persistent watch should either honor it with exact-path lifecycle preflight or reject it clearly.
- `doctor` should report lifecycle/passport blockers instead of a generic "local SQLite graph database exists and opens" check.

### Already fixed

- CLI `status`.
- CLI default `query` paths using `open_existing_store`.
- CLI `context-pack`.
- CLI `impact`.
- CLI `bundle export`.
- CLI `watch --once`.
- MCP `status`.
- MCP search/analyze/read tools using `open_store_with_preflight`.
- MCP `update_changed_files` through guarded update path.
- Incremental update stale cleanup path.
- Include-aware pruning helper and audit logging.
- Proof-build binary claimability metadata for debug versus release benchmark timing.

### Not applicable

- `SqliteGraphStore::open` and `open_in_memory` as primitives.
- `inspect_db_preflight` read-only open, which is the lifecycle guard itself.
- Test-only direct opens under `#[cfg(test)]`.
- UI `/api/source-span`, which reads source files only and does not open a DB.

## Explicit classification of the five suspected surfaces

| Surface | Current classification | Evidence |
| --- | --- | --- |
| `query unresolved-calls --db` | Present, P0 | Subcommand `--db` is parsed at `crates/codegraph-cli/src/lib.rs:8961`; direct read-only open happens at `9012`; optional source scan opens `SqliteGraphStore` at `9047`. The wrapper guard at `1119` checks the default/env DB, not necessarily this exact path. |
| Long-running `watch --db` | Partially present, P0 plus P3 | `--db` is parsed at `10017`, honored only in the `--once` branch at `7402`, but persistent watch calls `watch_repo(&options.repo, ...)` at `7418`; startup opens default/env DB directly at `7432`. |
| `doctor` | Present, P0 | `run_doctor_command` checks DB health with direct `SqliteGraphStore::open(&db_path).schema_version()` at `955` without passport preflight. |
| `bundle import` | Present, P0 | `run_bundle_import` opens the default/env DB directly at `8411`, mutates it in a transaction at `8413`, then writes a passport after the import. |
| Benchmark/update-integrity helpers | Partially present, P2 | Update mutations use guarded `update_changed_files_to_db`, but benchmark helpers open generated/copied DBs directly for schema/hash/count/integrity at `12915`, `13191`, `13205`, `13216`, `13221`, `13228`, `13244`, and `13254`. |

## Verification plan

## Verification results

- `python -m json.tool reports/audit/db_lifecycle_surface_inventory.json`: passed.
- `cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact`: passed after the workspace failure below.
- `cargo test --workspace`: failed twice on the same test, `tests::ui_server_starts_and_status_endpoint_uses_real_index`, with Windows socket error `Os { code: 10054, kind: ConnectionReset, message: "An existing connection was forcibly closed by the remote host." }`.

The failure is documented rather than fixed in this prompt because the requested scope was an inventory before behavior changes. The exact failed test passing alone suggests a local UI smoke/server timing or workspace-order issue, but the required full workspace command is currently red in this worktree.
