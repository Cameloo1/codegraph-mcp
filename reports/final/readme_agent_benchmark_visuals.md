# README Agent Benchmark Visuals

Generated: 2026-05-13T14:53:14-05:00

These visuals summarize benchmark evidence for agent-context quality, trusted proof relations, and agent-loop readiness. They do not claim final intended-performance pass, CodeGraph-vs-CGC superiority, real-world recall, or precision for absent proof-mode relations.

## Charts

- `agent_context_quality`: `docs/assets/readme/agent_visual_01_context_quality.png`
- `trusted_proof_relations`: `docs/assets/readme/agent_visual_02_trusted_relations.png`
- `agent_loop_readiness`: `docs/assets/readme/agent_visual_03_agent_loop_readiness.png`

## Metrics Used

- agent_visual_01_context_quality: Graph Truth Gate = 100.0 percent from `reports/final/comprehensive_benchmark_latest.json`. Claim boundary: Graph Truth Gate pass rate over 11 adversarial fixtures.
- agent_visual_01_context_quality: Context Packet Gate = 100.0 percent from `reports/final/comprehensive_benchmark_latest.json`. Claim boundary: Context Packet Gate pass rate over 11 adversarial fixtures.
- agent_visual_01_context_quality: Critical symbol recall = 100.0 percent from `reports/final/comprehensive_benchmark_latest.json`. Claim boundary: Context packet gate validation; not a real-world recall claim.
- agent_visual_01_context_quality: Proof-path coverage = 100.0 percent from `reports/final/comprehensive_benchmark_latest.json`. Claim boundary: Context packet gate validation over adversarial fixtures.
- agent_visual_01_context_quality: Proof-path source-span coverage = 100.0 percent from `reports/final/comprehensive_benchmark_latest.json`. Claim boundary: Proof paths include source spans in the context packet gate.
- agent_visual_01_context_quality: Expected tests recall = 100.0 percent from `reports/final/comprehensive_benchmark_latest.json`. Claim boundary: Expected-test fixture recall; not a real-world test recall claim.
- agent_visual_01_context_quality: Distractor-free packet = 100.0 percent from `reports/final/comprehensive_benchmark_latest.json`. Claim boundary: Clean-context score derived as 100 * (1 - distractor_ratio) for the context packet gate.
- agent_visual_02_trusted_relations: CALLS = 2363.0 proof_db_edges from `reports/final/manual_relation_precision.json`, status `pass`. Claim boundary: Sampled precision for present compact-proof relation only; recall is not claimed.
- agent_visual_02_trusted_relations: READS = 1970.0 proof_db_edges from `reports/final/manual_relation_precision.json`, status `pass`. Claim boundary: Sampled precision for present compact-proof relation only; recall is not claimed.
- agent_visual_02_trusted_relations: WRITES = 1104.0 proof_db_edges from `reports/final/manual_relation_precision.json`, status `pass`. Claim boundary: Sampled precision for present compact-proof relation only; recall is not claimed.
- agent_visual_02_trusted_relations: FLOWS_TO = 14801.0 proof_db_edges from `reports/final/manual_relation_precision.json`, status `pass`. Claim boundary: Sampled precision for present compact-proof relation only; recall is not claimed.
- agent_visual_02_trusted_relations: MUTATES = 177.0 proof_db_edges from `reports/final/manual_relation_precision.json`, status `reported_no_target`. Claim boundary: Sampled precision for present compact-proof relation only; recall is not claimed.
- agent_visual_02_trusted_relations: MAY_MUTATE = 1130.0 proof_db_edges from `reports/final/manual_relation_precision.json`, status `reported_no_target`. Claim boundary: Sampled precision for present compact-proof relation only; recall is not claimed.
- agent_visual_02_trusted_relations: PathEvidence = 20.0 labeled_path_evidence_samples from `reports/final/manual_relation_precision.json`, status `pass`. Claim boundary: Sampled PathEvidence correctness only; recall is not claimed.
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
