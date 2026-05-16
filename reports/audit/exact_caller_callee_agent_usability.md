# Exact Caller/Callee Agent Usability

Timestamp: 2026-05-15 23:47:27 -05:00

## Source Of Truth

- `MVP.md` was required before implementation.
- Active worktree note: `MVP.md` is not present in `C:\Users\wamin\.codex\worktrees\4160\codegraph-mcp`; the canonical sibling file at `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read.
- The requested AGENTS.md completion timestamp was not written because the active worktree does not contain `MVP.md`, and writing the sibling checkout would be outside this workspace.

## Behavior Added

- CLI `query callers` / `query callees` now accept:
  - `--entity-id <id>` for exact persisted-entity traversal.
  - `--exact-resolved` for symbol input that must resolve to exactly one entity.
  - `--fuzzy` to preserve the prior alias/global matching behavior explicitly.
  - `--limit <n>`.
- Default symbol behavior now separates result classes:
  - exactly one entity match: returns `exact_resolved_entity_results`.
  - multiple exact matches: returns `status=ambiguous_symbol`, `ambiguous_symbol_matches`, and no legacy `callers`/`callees` rows pretending to be exact.
  - no exact match: falls back to `fuzzy_or_global_results` unless `--exact-resolved` was requested.
- Exact rows include source spans and proof labels: relation, exactness, confidence, edge class, context, derived flag, provenance edges, and unresolved flag.
- MCP `codegraph.find_callers` / `codegraph.find_callees` now support unambiguous `query`/`symbol` resolution while keeping `entity_id` exact.
- MCP ambiguous symbol resolution returns `status=ambiguous_symbol` with candidate IDs instead of choosing the first match.

## Compatibility

- Graph semantics were not changed.
- Stored entities, edges, relation kinds, source spans, and proof generation are unchanged.
- The previous broad caller/callee behavior remains available through `--fuzzy`.
- Existing CLI smoke coverage was updated to call `--fuzzy` where it intentionally checks broad alias/global behavior.

## Tests Added

- Same function name in two files:
  - `callers_ambiguous_symbol_lists_candidates_without_legacy_exact_noise`
- Unambiguous symbol exact mode:
  - `callers_default_to_exact_results_when_symbol_is_unambiguous`
- `--entity-id` exact callers:
  - `callers_entity_id_mode_returns_only_selected_entity_callers`
- `--entity-id` exact callees plus fuzzy preservation:
  - `callees_entity_id_mode_returns_exact_callees_and_fuzzy_mode_still_works`
- MCP exact symbol resolution smoke:
  - `mcp_call_relation_query_resolves_unambiguous_symbol_exactly`

## Verification

- `cargo test -p codegraph-cli caller` passed: 3 tests.
- `cargo test -p codegraph-cli callee` passed: 1 test.
- `cargo test -p codegraph-mcp-server mcp_call_relation` passed: 1 test.
- `cargo test -p codegraph-cli --test cli_smoke index_status_and_query_commands_work_on_fixture_repo` passed after updating that smoke to use `--fuzzy` for legacy broad behavior.
- `cargo test -p codegraph-bench graph_truth` passed: 19 tests.
- `cargo test --workspace` passed.
- `git diff --check` passed; Git only reported CRLF normalization warnings for existing dirty files.

## Notes

- The first full workspace run failed because the old fixture smoke expected `query callers sanitize` to be `ok`. That fixture symbol is ambiguous under exact resolution, so the smoke was corrected to test legacy behavior with `--fuzzy`.
- The worktree already contained unrelated lifecycle/report changes before this task; this report covers only the caller/callee usability change.
