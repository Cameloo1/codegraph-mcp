# 20 Context Packet Gate

Verdict: context packet gate added; current context packets are not yet strict-useful.

`MVP.md` says the graph is truth, vectors suggest, exact graph/source verification proves, and the compact context packet is the final evidence object the coding agent receives. This phase therefore scores packet usefulness directly instead of treating graph indexing or CLI success as enough.

## Inputs Read

- `MVP.md`
- `crates/codegraph-query/src/lib.rs`
- `crates/codegraph-bench/src/graph_truth.rs`
- `crates/codegraph-cli/src/lib.rs`
- `benchmarks/graph_truth/schemas/graph_truth_case.schema.json`
- `benchmarks/graph_truth/fixtures/*/graph_truth_case.json`
- `reports/audit/10_stage_ablation.md`

## Command Added

```powershell
codegraph-mcp bench context-packet --cases <path> --fixture-root <path> --out-json <path> --out-md <path> [--top-k <k>] [--budget <tokens>]
```

Aliases:

- `context-packet`
- `context-packet-gate`
- `context`

The gate indexes each graph-truth fixture, builds a context packet from task-prompt-derived seeds, and checks the packet against hand-labeled expected and forbidden facts.

Unlike the older graph-truth context check, this gate does not seed the packet with expected context symbols. That makes the score stricter and exposes whether Stage 0/task prompts are actually enough to produce useful packet contents.

## Metrics

The report includes:

- context symbol recall@k
- critical symbol missing rate
- distractor ratio
- proof path coverage
- source span coverage
- critical snippet coverage
- recommended test recall
- useful facts per byte
- useful facts per estimated token

Failure rules include:

- critical context symbol missing
- forbidden context symbol present
- too many distractors
- expected proof path missing
- proof path missing required source spans
- missing path context label
- non-production path marked production-proof eligible
- missing critical snippet
- missing recommended test
- missing risk summary for risk-bearing cases

## First Strict Run

Command:

```powershell
cargo run -p codegraph-cli -- bench context-packet --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\20_context_packet_gate.json --out-md reports\audit\artifacts\20_context_packet_gate.md --top-k 10
```

Artifacts:

- `reports/audit/artifacts/20_context_packet_gate.json`
- `reports/audit/artifacts/20_context_packet_gate.md`

Summary:

| Metric | Value |
| --- | ---: |
| Cases passed | 2/10 |
| Context symbol recall@10 | 0.364 |
| Critical symbol missing rate | 0.636 |
| Distractor ratio | 0.054 |
| Proof path coverage | 0.000 |
| Source span coverage | unknown |
| Critical snippet coverage | 0.692 |
| Recommended test recall | 0.000 |
| Useful facts per byte | 0.0002607 |
| Useful facts per estimated token | 0.001048 |

Case results:

| Case | Status | Main issue |
| --- | --- | --- |
| `admin_user_roles_not_conflated` | failed | Missing `admin` and `user` critical symbols; no risk summary. |
| `derived_edge_requires_provenance` | failed | Missing `users` critical symbol and derived base-proof path. |
| `dynamic_import_marked_heuristic` | passed | Heuristic dynamic import case had the expected critical packet content. |
| `file_rename_prunes_old_path` | passed | Live symbol/snippet expectations were met. |
| `import_alias_change_updates_target` | failed | Missing `src/beta.target` critical symbol. |
| `mock_call_not_production_call` | failed | Missing production service symbol, proof path, snippets, and recommended test. |
| `same_name_only_one_imported` | failed | Wrong same-name distractor entered the packet and proof path was missing. |
| `sanitizer_exists_but_not_on_flow` | failed | Raw-flow proof path missing and no risk summary. |
| `source_span_exact_callsite` | failed | Missing `src/actions.second`, proof path, and exact callsite snippet. |
| `stale_cache_after_delete` | failed | Missing live symbol and source snippet. |

## Interpretation

Current context packets are not yet strict-useful for agent planning. They can carry some source snippets, but they do not reliably include critical target symbols, proof paths, risk summaries, or recommended tests. The most severe gap is proof path coverage: `0/5` expected proof paths were present in the packet under strict task-prompt-derived seeding.

The top missing symbols/paths are:

- `admin`
- `user`
- `users`
- `src/beta.target`
- `src/service.sendEmail`
- `src/actions.second`
- `src/live.live`
- `path://derived/base-proof`
- `path://mock/prod-checkout-to-service`
- `path://same-name/handler-to-a`
- `path://sanitizer/raw-flow-to-save`
- `path://span/run-to-second`

Distractor problems are currently concentrated in `same_name_only_one_imported`, where `src/b.chooseUser` appeared even though the case forbids the wrong same-name target.

## Code Changes

- `crates/codegraph-bench/src/graph_truth.rs`
  - Added `ContextPacketGateOptions`, report types, metrics, strict comparator, Markdown renderer, and tests.
  - Added prompt-derived seed resolution for context packet runs.
  - Added proof path/source-span/snippet/test/distractor/useful-density scoring.
- `crates/codegraph-bench/src/lib.rs`
  - Exported the context packet gate API.
- `crates/codegraph-cli/src/lib.rs`
  - Added `codegraph-mcp bench context-packet`.
- `crates/codegraph-cli/tests/cli_smoke.rs`
  - Added CLI integration coverage over all ten adversarial fixtures.

## Tests Run

- `cargo fmt --all`
- `cargo test -p codegraph-bench context_packet --lib`
- `cargo test -p codegraph-cli --test cli_smoke bench_context_packet_gate_runs_adversarial_fixtures_and_writes_reports`
- `cargo run -p codegraph-cli -- bench context-packet --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\20_context_packet_gate.json --out-md reports\audit\artifacts\20_context_packet_gate.md --top-k 10`
- `cargo fmt --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Next Work

Fix packet construction before trusting agent-facing context:

1. Resolve prompt-derived symbols into entity IDs inside normal `context-pack`, not only inside this benchmark gate.
2. Add path endpoints and proof targets into `ContextPacket.symbols`, not only seed IDs.
3. Ensure expected proof paths survive packet compaction when they are critical.
4. Generate risk summaries from matched security/dataflow/test-impact paths.
5. Generate recommended tests from expected `TESTS`/`ASSERTS`/`MOCKS`/`STUBS` evidence without letting mock paths satisfy production proof.
