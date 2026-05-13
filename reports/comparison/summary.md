# CodeGraphContext External Comparison

Benchmark: `codegraphcontext-external-comparison`

Competitor: `CodeGraphContext` from `https://github.com/CodeGraphContext/CodeGraphContext`

Executable: `skipped: CodeGraphContext executable unavailable; set CGC_COMPETITOR_BIN or install cgc/codegraphcontext`

| Mode | Runs | Skipped | File R@10 | Symbol R@10 | Path R@10 | Relation F1 | MRR | NDCG | Unsupported | False Proofs |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `codegraph_full_context_packet` | 5 | 0 | 1.000 | 0.000 | 0.000 | 0.397 | 1.000 | 0.564 | 0 | 14 |
| `codegraph_graph_only` | 5 | 0 | 1.000 | 0.000 | 0.000 | 0.297 | 1.000 | 0.564 | 0 | 16 |
| `codegraphcontext_cli` | 0 | 5 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 5 | 0 |

Unsupported or unparseable competitor fields are counted separately from incorrect results. No SOTA claim is implied by this report.
