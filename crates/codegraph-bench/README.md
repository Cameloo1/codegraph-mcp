# codegraph-bench

Phase 30 benchmark harness for the CodeGraph MVP and post-MVP parity work.

`MVP.md` is the source of truth for this crate's phase boundary and acceptance
criteria.

This crate provides:

- benchmark task and ground-truth schemas
- synthetic controlled TS repos for relation extraction, long-chain paths,
  context retrieval, agent patch, compression, security/auth, async/event, and
  test impact families
- local baseline runners for vanilla, BM25, vector-only, graph-only,
  graph+binary/PQ, graph+Bayesian, and full context packet modes
- precision/recall/F1, recall@k, MRR, NDCG, token, latency, memory, and
  patch/test success metrics
- real-repo commit replay planning when a local git checkout is available
- JSON and Markdown report generation
- Phase 26 gap scoreboard reports with win/loss/tie/unknown dimensions
- separate Prompt 21.1 external CodeGraphContext / CGC comparison reports
- pinned TypeScript, Python, Go, Rust, and Java real-repo maturity corpus
- offline replay plans into ignored `.codegraph-bench-cache/real-repos`
- final parity report artifacts with explicit skipped/unknown fields and no
  fabricated SOTA claims

The harness is local and deterministic. It uses existing parser, store, query,
and vector crates and does not modify core retrieval logic.
