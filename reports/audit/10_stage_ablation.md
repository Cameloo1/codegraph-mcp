# 10 Retrieval Stage Ablation

Source of truth: `MVP.md`.

## Verdict

**Partially trustworthy as stage-separation scaffolding, not trustworthy as graph-correctness proof.**

The new ablation command separates Stage 0 exact retrieval, Stage 1 binary sieve, Stage 2 int8/PQ-style rerank, Stage 3 exact graph verification, and Stage 4 context packet construction. This fixes the attribution problem called out in earlier audits: exact-symbol wins are now visible and are not credited to vector stages, and vector-only modes cannot report proof-grade path success.

The run still inherits the graph-truth failure from `reports/audit/09_graph_truth_gate.md`: relation F1 is 0 for graph-verified modes, so path recall in this report should be read as structural path discovery, not MVP proof.

Artifacts:

- JSON report: `reports/audit/artifacts/10_stage_ablation.json`
- Markdown report: `reports/audit/artifacts/10_stage_ablation.md`

Command used:

```powershell
cargo run -p codegraph-cli -- bench retrieval-ablation --cases benchmarks/graph_truth/fixtures --fixture-root . --out-json reports/audit/artifacts/10_stage_ablation.json --out-md reports/audit/artifacts/10_stage_ablation.md --top-k 10
```

## Code Surfaces

| Surface | Files |
| --- | --- |
| Ablation implementation | `crates/codegraph-bench/src/retrieval_ablation.rs`, `crates/codegraph-bench/src/lib.rs` |
| CLI command | `crates/codegraph-cli/src/lib.rs` |
| CLI integration test | `crates/codegraph-cli/tests/cli_smoke.rs` |
| Retrieval/funnel code inspected | `crates/codegraph-query/src/lib.rs` |
| Vector-stage primitives inspected | `crates/codegraph-vector/src/lib.rs` |
| Benchmark code inspected | `crates/codegraph-bench/src/lib.rs`, `crates/codegraph-bench/src/graph_truth.rs` |
| Prior report read | `reports/audit/09_graph_truth_gate.md` |
| Fixtures used | `benchmarks/graph_truth/fixtures/*/graph_truth_case.json` |

## Command Added

```text
codegraph-mcp bench retrieval-ablation --cases <path> --fixture-root <path> --out-json <path> --out-md <path> [--mode <mode>]... [--top-k <k>]
```

Supported modes:

- `stage0_exact_only`
- `stage1_binary_only`
- `stage2_int8_pq_only`
- `stage0_plus_stage1`
- `stage0_plus_stage1_plus_stage2`
- `graph_verification_only`
- `full_context_packet`

## Mode Summary

| Mode | File R@k | Symbol R@k | Path R@k | Relation F1 | FP Rate | p50 ms | p95 ms | Stage0 | Stage1 | Stage2 | Verified | Context Symbols | Proof Path Claimed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `stage0_exact_only` | 0.767 | 0.640 | 0.000 | unknown | 0.015 | 1 | 1 | 8.4 | 0.0 | 0.0 | 0.0 | 0.0 | false |
| `stage1_binary_only` | 0.967 | 0.560 | 0.000 | unknown | 0.022 | 1 | 1 | 0.0 | 10.0 | 0.0 | 0.0 | 0.0 | false |
| `stage2_int8_pq_only` | 0.967 | 0.620 | 0.000 | unknown | 0.019 | 17 | 88 | 0.0 | 0.0 | 10.0 | 0.0 | 0.0 | false |
| `stage0_plus_stage1` | 0.967 | 0.527 | 0.000 | unknown | 0.016 | 1 | 2 | 8.4 | 14.3 | 0.0 | 0.0 | 0.0 | false |
| `stage0_plus_stage1_plus_stage2` | 0.967 | 0.620 | 0.000 | unknown | 0.022 | 16 | 28 | 8.4 | 21.9 | 10.0 | 0.0 | 0.0 | false |
| `graph_verification_only` | 0.767 | 0.640 | 0.500 | 0.000 | 0.012 | 1 | 2 | 8.4 | 0.0 | 0.0 | 3.3 | 0.0 | false |
| `full_context_packet` | 0.683 | 0.560 | 0.500 | 0.000 | 0.013 | 3 | 10 | 8.4 | 12.1 | 11.7 | 4.0 | 4.0 | false |

## What Each Stage Shows

| Stage | What This Run Suggests | What It Does Not Prove |
| --- | --- | --- |
| Stage 0 exact seeds / symbols / BM25 / FTS | Carries much of the useful symbol recall by itself: 0.640 mean symbol R@k at p50 1 ms. | Does not prove paths, relations, call targets, source spans, or forbidden fact absence. |
| Stage 1 binary sieve | Raises file recall on tiny fixtures, but symbol recall is lower than Stage 0 alone in this run. | Does not prove semantic correctness and cannot claim proof-grade path success. |
| Stage 2 int8/PQ/Matryoshka interface | Recovers 0.620 symbol R@k and 0.967 file R@k in vector-only mode. | Adds clear latency on fixtures and still cannot prove graph paths without Stage 3. |
| Stage 3 exact graph verification | Only graph modes produce nonzero path recall, but relation F1 remains 0. | Current graph facts are still not proof-grade; this reflects the failures in audit 09. |
| Stage 4 compact context packet | Produces context symbols and verified-path objects separately from raw graph verification. | Current full packet did not improve recall over simpler modes and cannot be treated as trustworthy proof while relation F1 is 0. |

## Attribution Checks

- Stage 0 exact contribution is visible in every mode that uses Stage 0.
- Stage 1 and Stage 2 only modes start from the full document corpus, not Stage 0 exact seed output.
- `stage0_plus_stage1` reports Stage 0 and Stage 1 candidate counts separately.
- `stage0_plus_stage1_plus_stage2` reports Stage 0, Stage 1, and Stage 2 candidate counts separately.
- Vector-only and vector-funnel-without-graph modes force path recall to 0 and never claim proof-grade path success.
- `graph_verification_only` measures exact graph/path verification without Stage 1 or Stage 2.
- `full_context_packet` measures the complete funnel through context packet construction.

## Current Interpretation

The stage currently carrying performance is Stage 0 plus broad retrieval. Stage 0 alone gets the best mean symbol recall among the low-latency modes, and its p50/p95 is 1 ms on the fixtures. Stage 1 improves file recall but does not improve symbol recall here. Stage 2 roughly matches Stage 0 symbol recall but adds the most latency: 17 ms p50 and 88 ms p95 in Stage 2-only mode, and 16 ms p50 / 28 ms p95 in the combined vector funnel.

Binary/PQ is not yet proven to help enough to justify itself. On these tiny graph-truth fixtures, Stage 1 appears cheap but mostly file-recall oriented; Stage 2 adds latency and still cannot contribute proof without graph verification.

Exact graph verification is not the fixture-scale latency bottleneck: `graph_verification_only` is 1 ms p50 / 2 ms p95. It is the semantic bottleneck. Relation F1 is 0 because the graph truth gate already showed expected hand-labeled edges are missing, so graph verification cannot yet validate the facts the MVP depends on.

## Required Next Work

1. Fix graph-truth semantic recall before treating any path metric as proof-grade.
2. Add larger-repo ablation runs after graph correctness improves, because tiny fixtures understate Stage 3 and Stage 4 scaling costs.
3. Record per-stage candidate identities for false-positive analysis, not only counts.
4. Add relation-aware precision/recall once expected edges begin matching.
5. Re-run this ablation after import/alias/call target fixes to determine whether Stage 1 and Stage 2 are actually improving agent-relevant retrieval.

## Tests Run

- `cargo fmt`
- `cargo test -p codegraph-bench retrieval_ablation --lib`
- `cargo test -p codegraph-cli --test cli_smoke retrieval_ablation`
- `cargo run -p codegraph-cli -- bench retrieval-ablation --cases benchmarks/graph_truth/fixtures --fixture-root . --out-json reports/audit/artifacts/10_stage_ablation.json --out-md reports/audit/artifacts/10_stage_ablation.md --top-k 10`

Final workspace test and lint status is recorded in `reports/audit/AUDIT_STATUS.md`.
