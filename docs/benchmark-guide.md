# Benchmark Guide

The root `README.md` is the public setup contract. Benchmarks are local,
reproducible, single-agent, and evidence-labeled. They measure CodeGraph against
internal baselines and optional black-box CGC runs without changing retrieval
logic.

There are two report classes:

- **Stable public summaries** are durable Markdown/JSON files linked from the
  README.
- **Run artifacts** are DBs, WAL/SHM files, raw stdout/stderr, copied fixtures,
  and temporary benchmark payloads. Keep them ignored unless a report
  explicitly promotes a small summary artifact.

## Stable Reports

Use these as the current public status surface:

- `reports/final/comprehensive_benchmark_latest.md` / `.json` - latest
  preserved comprehensive gate.
- `reports/final/intended_tool_quality_gate.md` / `.json` - Intended Tool
  Quality Gate.
- `reports/final/manual_relation_precision.md` / `.json` - manual sampled
  precision boundary.
- `reports/comparison/codegraph_vs_cgc_latest.md` / `.json` - CGC comparison
  status.

Current stable status: Graph Truth and Context Packet gates pass, warm repeat
and single-file update pass, DB integrity passes, and proof DB size is under the
250 MiB target. The Intended Tool Quality Gate is still **FAIL** in the stable
report because `proof_build_only_ms = 184,297 ms` is above `<=60,000 ms`. The
CGC comparison is diagnostic/incomplete and does not support a superiority
claim.

## Run

Use the release binary for timing that might be compared to production
thresholds:

```powershell
cargo build --release --bin codegraph-mcp
.\target\release\codegraph-mcp.exe bench comprehensive --fresh --output-dir reports\final
```

Debug runs are allowed for diagnosis only. If a debug binary produces
proof-build timing, mark it non-claimable and do not compare it to the
production threshold.

Small local benchmark:

```powershell
codegraph-mcp bench --output target\codegraph-benchmark-report.json
```

Run one or more baselines:

```powershell
codegraph-mcp bench --baseline grep-bm25 --baseline graph-only
codegraph-mcp bench --baseline graph-only --format markdown --output target\graph-only.md
```

## Families

- relation extraction
- long-chain path
- context retrieval
- agent patch
- compression
- security/auth
- async/event
- test impact

## Baselines

- `vanilla_no_retrieval`
- `grep_bm25`
- `vector_only`
- `graph_only`
- `graph_binary_pq_funnel`
- `graph_bayesian_ranker`
- `full_context_packet`

## Metrics

- precision, recall, F1
- relation precision, recall, F1
- path recall@k
- file recall@k
- symbol recall@k
- MRR and NDCG
- token cost
- latency estimate
- memory estimate
- patch/test success where feasible

## Synthetic Repos

The harness generates controlled TS repos with known ground truth for each
family. These repos are indexed through the same parser/store/query/vector
crates used by normal CLI and MCP paths.

## Real-Repo Replay

Real-repo commit replay is planned when a local git checkout is available. The
harness records the diff/index/test commands needed for replay. It does not run
destructive git operations.

## Real-Repo Maturity Corpus

```powershell
codegraph-mcp bench real-repo-corpus
```

The corpus records pinned TypeScript, Python, Go, Rust, and Java repositories
with task manifests for symbol search, caller/callee, call chain,
changed-file impact, test impact, and context retrieval. Normal tests do not
clone these repos. Replay commands clone into `.codegraph-bench-cache/real-repos`,
which is ignored by the repository.

Offline replay planning:

```powershell
scripts\replay-real-repo-corpus.ps1
```

Live replay requires an explicit network opt-in:

```powershell
scripts\replay-real-repo-corpus.ps1 -AllowNetwork
```

## Optional Report Commands

JSON reports are machine-readable and include per-run metrics plus aggregate
baseline summaries. Markdown reports are compact human summaries.

Gap scoreboard:

```powershell
codegraph-mcp bench gaps --output-dir reports\phase26-gaps
```

Output files:

- `summary.json`
- `summary.md`
- `per_task.jsonl`
- `external-codegraphcontext/`

The scoreboard classifies every dimension as `win`, `loss`, `tie`, or
`unknown`. Missing CodeGraphContext data is `unknown` or `skipped`, never
guessed. The nested `external-codegraphcontext` directory contains the black-box
CGC comparison report and raw stdout/stderr artifacts when CGC actually runs.

Parity report:

```powershell
codegraph-mcp bench parity-report --output-dir reports\phase30-parity
```

Output files:

- `summary.json`
- `summary.md`
- `per_task.jsonl`

The report includes exact CodeGraph version metadata, pinned real-repo commits,
OS/arch metadata, skipped/unknown fields, and no fabricated SOTA claims.

## External CodeGraphContext Comparison

The external competitor harness treats CodeGraphContext / CGC as a black-box
CLI. It is not part of the internal baseline enum and does not replace the
CodeGraph correctness gates.

Run it locally:

```powershell
codegraph-mcp bench cgc-comparison --output-dir reports\cgc-comparison\manual
```

Executable discovery:

- use `CGC_COMPETITOR_BIN` when set
- otherwise try `cgc`
- otherwise try `codegraphcontext`
- if unavailable, write a skipped `codegraphcontext_cli` result with a
  structured reason

The comparison modes are:

- `codegraph_graph_only`
- `codegraph_full_context_packet`
- `codegraphcontext_cli`

Report layout:

```text
reports/cgc-comparison/<timestamp>/
|-- run.json
|-- per_task.jsonl
|-- summary.md
|-- fixtures/
|-- normalized_outputs/
|   |-- codegraph/
|   `-- codegraphcontext/
`-- raw_artifacts/
    `-- codegraphcontext/
```

The harness preserves raw CGC stdout/stderr artifacts and marks unsupported or
unparseable fields separately from incorrect results. It does not claim SOTA
superiority unless measured results directly support that claim.

The latest stable CGC report is diagnostic only: CGC version 0.4.7 was recovered
enough for smoke and fixture diagnostics, the fixture diagnostic was not
comparable to CodeGraph proof/path/source-span evidence, and the Autoresearch
diagnostic timed out under the 180s cap. Partial CGC artifacts are not final
storage artifacts.
