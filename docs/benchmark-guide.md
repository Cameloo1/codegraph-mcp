# Benchmark Guide

The root `README.md` is the public setup contract. Benchmarks are local and
reproducible. They measure CodeGraph against baseline implementations without
changing retrieval logic and without using subagents.

## Run

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

## Reports

JSON reports are machine-readable and include per-run metrics plus aggregate
baseline summaries. Markdown reports are compact human summaries.

Phase 26 gap scoreboard:

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

Final parity report:

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

Prompt 21.1 adds a separate external competitor harness for CodeGraphContext /
CGC. This is not part of the internal baseline enum and does not replace the
MVP benchmark suite.

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
