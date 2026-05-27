# Benchmark Guide

The root `README.md` is the public setup contract. Benchmarks are local,
reproducible, single-agent, and evidence-labeled.

The current benchmark framing is agent reliability, not `CodeGraph vs rg`.
Component diagnostics may compare retrieval providers, but the product
benchmark is:

```text
same agent + normal rg/search/edit/test tools
same agent + normal rg/search/edit/test tools + CodeGraph
```

For the newer agent-infrastructure benchmark layer, current diagnostic findings,
agent-reliability lab plan, and SWE-bench readiness plan, see
`docs/agent-benchmarking.md`, `docs/agent-reliability-benchmark-lab.md`,
`docs/benchmark-findings.md`, and `docs/swe-bench-readiness.md`.

There are two report classes:

- **Release-facing summaries** are compact docs or visuals that describe current
  verified behavior with strict claim boundaries.
- **Run artifacts** are DBs, WAL/SHM files, raw stdout/stderr, copied fixtures,
  and temporary benchmark payloads. Keep them ignored unless a small summary is
  explicitly promoted.

## Release-Facing Benchmark Surface

The root README is the public setup contract. It intentionally summarizes
benchmark-lab evidence through compact, claim-bounded visuals instead of linking
raw run outputs.

Current release-facing benchmark docs:

- [Agent Benchmarking](agent-benchmarking.md) explains the v0/v0.5/v1 framing.
- [Agent Reliability Benchmark Lab](agent-reliability-benchmark-lab.md) defines
  the product benchmark shape: same agent with normal tools versus the same
  agent with normal tools plus CodeGraph.
- [Current Benchmark Findings](benchmark-findings.md) summarizes the latest
  local diagnostic sweep without making public benchmark claims.
- [SWE-bench Readiness](swe-bench-readiness.md) tracks local harness readiness
  and what remains before CodeGraph-attributed patch outcomes can be claimed.

Older preserved reports under `reports/final/` remain useful historical gates,
but they are not the current README status surface unless a doc explicitly says
so. Detailed benchmark harness code, raw run interpretation, OpenEvolve policy
experiments, and evolving scorecards belong on the
`benchmark-and-openevolve-lab` branch.

## Run

Use the release binary for timing that might be compared to production
thresholds. For current benchmark-layer diagnostics, prefer the operator-grade
suite:

```powershell
cargo build --release --bin codegraph-mcp
python -m benchmarks.harness.runners.run_benchmark_suite --suite full --output-dir benchmarks/results/summaries/<run_id>
```

Legacy in-binary bench commands still exist for targeted local diagnostics:

```powershell
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

The scoreboard classifies every dimension as `win`, `loss`, `tie`, or
`unknown`. Missing CodeGraphContext data is `unknown` or `skipped`, never
guessed. Raw stdout/stderr belongs in ignored run artifacts; publish only the
curated summary.

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

The harness preserves raw CGC stdout/stderr locally and marks unsupported or
unparseable fields separately from incorrect results. It does not claim SOTA
superiority unless measured results directly support that claim.

The latest stable CGC report is diagnostic only: CGC version 0.4.7 was recovered
enough for smoke and fixture diagnostics, the fixture diagnostic was not
comparable to CodeGraph proof/path/source-span evidence, and the Autoresearch
diagnostic timed out under the 180s cap. Partial CGC artifacts are not final
storage artifacts.
