# README Agent Benchmark Visuals

Generated: 2026-05-15T16:53:26-05:00

These visuals summarize benchmark evidence for large-repo improvement, sampled evidence reliability, and warm agent-loop latency. They do not claim final intended-performance pass, CodeGraph-vs-CGC superiority, real-world recall, or precision for absent proof-mode relations.

## Charts

- `large_repo_improvement`: `docs/assets/readme/agent_visual_01_context_quality.png`
- `evidence_reliability`: `docs/assets/readme/agent_visual_02_trusted_relations.png`
- `warm_agent_loop_latency`: `docs/assets/readme/agent_visual_03_agent_loop_readiness.png`

## Metrics Used

- agent_visual_01_context_quality: Proof DB size = 9.539442938592392 improvement_factor from `reports/final/comprehensive_benchmark_latest.md`. Claim boundary: Before/after regression row from the comprehensive benchmark; lower is better and this is not a CGC comparison.
- agent_visual_01_context_quality: context_pack p95 = 36.31178183464634 improvement_factor from `reports/final/comprehensive_benchmark_latest.md`. Claim boundary: Before/after regression row from the comprehensive benchmark; lower is better and this is not a CGC comparison.
- agent_visual_01_context_quality: Single-file update = 47.34769775678867 improvement_factor from `reports/final/comprehensive_benchmark_latest.md`. Claim boundary: Before/after regression row from the comprehensive benchmark; lower is better and this is not a CGC comparison.
- agent_visual_01_context_quality: Repeat unchanged = 8.106929510155316 improvement_factor from `reports/final/comprehensive_benchmark_latest.md`. Claim boundary: Before/after regression row from the comprehensive benchmark; lower is better and this is not a CGC comparison.
- agent_visual_02_trusted_relations: Sampled relation precision = 100.0 percent from `reports/final/manual_relation_precision.json`. Claim boundary: Sampled precision only across present compact-proof relations; recall is unknown.
- agent_visual_02_trusted_relations: No false/stale/leak events = 100.0 percent from `reports/final/manual_relation_precision.json`. Claim boundary: Manual sample taxonomy found no false positives, stale facts, test/mock leakage, or derived-without-provenance events.
- agent_visual_02_trusted_relations: Wrong-span avoidance = 100.0 percent from `reports/final/manual_relation_precision.json`. Claim boundary: Source-span correctness across labeled samples; recall is not claimed.
- agent_visual_02_trusted_relations: Source-span precision = 100.0 percent from `reports/final/manual_relation_precision.json`. Claim boundary: Manual source-span precision over eligible samples; recall is unknown.
- agent_visual_02_trusted_relations: PathEvidence correctness = 100.0 percent from `reports/final/manual_relation_precision.json`. Claim boundary: Sampled PathEvidence correctness only; recall is not claimed.
- agent_visual_03_agent_loop_readiness: Proof DB size = 0.684736 observed/target_ratio from `reports/final/intended_tool_quality_gate.json`, status `pass`. Claim boundary: Compact proof DB size from the Intended Tool Quality Gate; audit/debug sidecars excluded.
- agent_visual_03_agent_loop_readiness: Repeat unchanged = 0.3348 observed/target_ratio from `reports/final/intended_tool_quality_gate.json`, status `pass`. Claim boundary: Warm unchanged index loop from the Intended Tool Quality Gate.
- agent_visual_03_agent_loop_readiness: Single-file update = 0.448 observed/target_ratio from `reports/final/intended_tool_quality_gate.json`, status `pass`. Claim boundary: Single-file update loop from the Intended Tool Quality Gate.
- agent_visual_03_agent_loop_readiness: context_pack p95 = 0.07012965 observed/target_ratio from `reports/final/comprehensive_benchmark_latest.json`, status `pass`. Claim boundary: Normal context-pack query latency from the comprehensive benchmark; not an agent success-rate metric.
- agent_visual_03_agent_loop_readiness: Unresolved calls p95 = 0.0983916 observed/target_ratio from `reports/final/comprehensive_benchmark_latest.json`, status `pass`. Claim boundary: Bounded unresolved-calls pagination latency from the comprehensive benchmark.
- agent_visual_03_agent_loop_readiness: Cold proof build = 3.071616666666667 observed/target_ratio from `reports/final/intended_tool_quality_gate.json`, status `fail`. Claim boundary: Cold proof-build-only timing from the Intended Tool Quality Gate; this is the remaining blocker.

## Historical Metrics Found

- `proof_db_mib_vs_clean_1_63_gib`: previous 1633.0, current 171.184, status improved from `reports/final/comprehensive_benchmark_latest.md`.
- `context_pack_p95_ms_vs_clean_30s`: previous 30315.0, current 834.853, status improved from `reports/final/comprehensive_benchmark_latest.md`.
- `single_file_update_ms_vs_clean_80s`: previous 80207.0, current 1694.0, status improved from `reports/final/comprehensive_benchmark_latest.md`.
- `repeat_unchanged_ms_vs_clean_13s`: previous 13571.0, current 1674.0, status improved from `reports/final/comprehensive_benchmark_latest.md`.
- `cold_proof_build_ms_vs_compact_baseline`: previous 184297.0, current 184297.0, status unchanged from `reports/final/comprehensive_benchmark_latest.md`.
- `proof_db_mib_vs_compact_baseline`: previous 320.63, current 171.184, status improved from `reports/final/comprehensive_benchmark_latest.md`.

## Omitted Metrics

- None.

## Caveats

- The Intended Tool Quality Gate remains FAIL because proof_build_only_ms exceeds the 60,000 ms target.
- CGC comparison remains diagnostic/blocked/incomplete; no CodeGraph-vs-CGC superiority claim is made.
- Manual relation precision is sampled precision only; recall is unknown without a false-negative gold denominator.
- Absent proof-mode relations are not plotted and have no precision claim.
- macOS is coming soon, not currently tested, and has no CI coverage.
