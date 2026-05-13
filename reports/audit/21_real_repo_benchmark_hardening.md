# 21 Real-Repo Benchmark Hardening

Source of truth: `MVP.md`. The relevant MVP rule is that vectors suggest, exact graph/source verification proves, and the final context packet is only useful when it carries compact proof-oriented evidence. This phase hardens benchmark reporting so incomplete competitor data, storage misses, and fake-agent dry runs cannot become superiority claims.

## Verdict

Overall real-repo superiority verdict: **unknown**.

CodeGraph can currently claim that some indexing/retrieval/context gates executed and emitted artifacts. It cannot claim real-repo agent superiority, SOTA performance, or a fair storage win over CGC.

## Code Inspected

| Surface | Files/functions inspected |
| --- | --- |
| Real-repo/two-layer harness | `crates/codegraph-bench/src/two_layer.rs::run_retrieval_quality_benchmark`, `run_agent_quality_benchmark`, `fake_agent_record`, `autoresearch_record`, `summarize_comparisons`, `render_two_layer_summary` |
| Final parity/final gate | `crates/codegraph-bench/src/lib.rs::final_parity_report`, `render_final_parity_markdown`, `final_acceptance_gate_report`, `final_gate_cgc_comparison`, `final_gate_size_targets`, `final_gate_verdict` |
| CGC harness | `crates/codegraph-bench/src/competitors/codegraphcontext.rs::run_codegraphcontext_mode`, `aggregate_external_runs`, `render_external_markdown` |
| Graph truth | `crates/codegraph-bench/src/graph_truth.rs` |
| Context packet gate | `crates/codegraph-bench/src/graph_truth.rs::run_context_packet_gate`, `evaluate_context_packet_case` |
| Stage ablation | `crates/codegraph-bench/src/retrieval_ablation.rs` |

## Latest Artifacts Read

| Artifact | Finding |
| --- | --- |
| `target/codegraph-bench-report/LATEST.txt` | Points to `target/codegraph-bench-report/sweep-20260509-223900`. |
| `target/codegraph-bench-report/sweep-20260509-223900/summary-data.json` | Current sweep has CGC skipped, retrieval synthetic DB size `430080` bytes, and agent hidden/visible tests `unknown`. |
| `target/cgc-comparison-full/run.json` | Separate CGC fixture run completed 5 fixture tasks, but it is fixture-only and not an Autoresearch full-repo comparison. |
| `reports/audit/artifacts/03_storage_latest.json` | Autoresearch DB family is `803495936` bytes; this is a storage-target failure signal, not a superiority signal. |
| `reports/audit/artifacts/19_graph_truth_gate.json` | Graph Truth Gate after prior fixes: `6/10` fixtures passed, final status `failed`. |
| `reports/audit/artifacts/10_stage_ablation.json` | Stage metrics are separated; vector-only stages do not claim proof-grade path success. |
| `reports/audit/artifacts/20_context_packet_gate.json` | Context Packet Gate: `2/10` fixtures passed; proof-path coverage `0.0`; critical-symbol missing rate `0.636`. |

## Hardened Verdict Logic

The benchmark verdict model now includes `pass`, `fail`, `incomplete`, and `unknown`.

| Condition | Required outcome |
| --- | --- |
| CGC skipped | Superiority `unknown`. |
| CGC timeout | CGC comparison `incomplete`; superiority `unknown`. |
| CGC incomplete with partial stats | CGC comparison `incomplete`; partial size is not comparable. |
| CodeGraph indexes but exceeds storage target | Storage `fail`; indexing completion is still allowed as a separate claim. |
| Internal fixtures pass but competitor is incomplete | Final superiority `unknown`. |
| Fake-agent dry run only | Agent coding quality `unknown`; trace contract may be claimed, not model superiority. |

Implemented guardrails:

- `FinalGateVerdict` now supports `incomplete`.
- `honest_superiority_verdict` rejects superiority claims from fake-agent-only or incomplete competitor evidence.
- `real_repo_storage_target_verdict` marks completed indexing with oversized DB as `fail`.
- `final_gate_cgc_comparison` maps CGC timeout warnings to `incomplete`.
- `final_gate_size_targets` returns `not_comparable_incomplete_cgc` unless CGC completed on the same scope.
- `fake_agent_record` is now labeled `fake_agent_dry_run`, carries `claim_scope: fake_agent_dry_run_only`, and contributes no numeric win score.
- Two-layer summaries now include explicit claim boundaries.
- Parity markdown now has explicit sections for indexing/storage, graph truth, retrieval ablation, context packet quality, real model coding quality, and fake-agent dry run.

## Section Status

| Section | Current status | Honest claim |
| --- | --- | --- |
| Indexing speed/storage | Mixed | CodeGraph completed the recorded Autoresearch index in prior artifacts, but the `803495936` byte DB family fails compact-storage goals. |
| Graph truth | Fail | Current graph correctness is improving but not fully trustworthy; latest strict gate still fails `4/10` fixtures. |
| Retrieval ablation | Partial | Stage-separated metrics exist; Stage 0/1/2 contributions are visible, and vector-only modes cannot claim proof-grade path success. |
| Context packet quality | Fail | Current prompt-derived context packets are not strict-useful: `2/10` pass, proof-path coverage `0.0`. |
| Real model coding quality | Unknown | No real model patch/test benchmark is implemented in these artifacts. |
| Fake-agent dry run | Pass for trace shape only | The fake runner can prove artifact shape and event plumbing, not coding quality. |
| CGC comparison | Unknown/incomplete depending run | Latest sweep has CGC skipped; separate fixture-only CGC artifacts do not establish full-repo superiority. |

## Claims Now Allowed

- CodeGraph completed indexing for the recorded Autoresearch run, with the DB-size caveat.
- The benchmark harness can emit machine-readable indexing, graph-truth, retrieval-ablation, context-packet, CGC, and fake-agent artifacts.
- Stage ablation reports can say which retrieval stage produced candidates and latency.
- Fake-agent dry runs can be used to test trace/report plumbing.
- Storage compactness can fail independently from indexing completion.

## Claims Explicitly Forbidden

- CGC timeout, skipped, or incomplete results are not CodeGraph wins.
- Partial CGC DB size must not be compared to completed CodeGraph DB size.
- Fake-agent dry runs must not be described as real agent coding success.
- Internal fixture passes must not be described as SOTA agent superiority.
- Synthetic fixture retrieval numbers must not be merged with Autoresearch full-repo indexing/storage numbers as one benchmark verdict.
- Vector-only or retrieval-only modes must not claim proof-grade path success without graph/source verification.

## Remaining Risks

- `aggregate_external_runs` still reports zero-valued averages for modes with zero completed runs; downstream readers must use completed/skipped counts before interpreting averages.
- The latest sweep and the standalone `target/cgc-comparison-full` run have different scopes; scripts should label this explicitly when building dashboards.
- Real model coding quality remains unimplemented, so any “agent-quality” report must retain fake-agent labels or unknown verdicts.
- Autoresearch full-repo indexing/storage is not yet coupled to graph-truth/context-packet quality on that repo.

## Next Work

1. Add a real model coding-quality runner only after graph truth/context packet gates improve.
2. Promote completion status into every aggregate table so zero completed competitor runs cannot look like zero performance.
3. Add a single real-repo report assembler that imports indexing, storage, graph truth, retrieval ablation, context packet, CGC, and agent-quality artifacts by scope and refuses mixed-scope final verdicts.
