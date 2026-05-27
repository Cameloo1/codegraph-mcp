# Benchmark Claim Boundaries

CodeGraph is evaluated as agent infrastructure: a context, retrieval, and trust
layer. It is not the model, the coding agent, or the official benchmark harness.

Every serious comparison must hold constant:

- same model
- same agent scaffold
- same task set
- same time budget
- same token/tool budget
- same evaluator
- same environment
- same scoring code

Component diagnostics may vary the retrieval/context provider:

- `baseline`
- `none`
- `rg_only`
- `rg_planned`
- `codegraph_exact_text`
- `codegraph_full`
- `codegraph_planned`

`rg_planned` and `codegraph_planned` are Benchmark Layer v0.5 provider modes.
Their implementation reports and v0.5 comparison reports are local diagnostic
evidence only; no CodeGraph-over-rg or public benchmark claim is made from
them.

For product claims, `rg` must remain available in both arms:

```text
Mode A: same agent + normal rg/search/edit/test tools
Mode B: same agent + normal rg/search/edit/test tools + CodeGraph
```

CodeGraph is not evaluated as an `rg` replacement. It is evaluated by the
additional reliability it provides on top of normal `rg` use.

## Safe Wording

- "On a pinned local diagnostic retrieval set, provider X returned these gold
  files with this recall, ranking, cost, and proof-label behavior."
- "On a pinned SWE-bench Lite 10-task diagnostic subset, the same configured
  rg-using agent solved A tasks without CodeGraph and B tasks with CodeGraph."
- "On a pinned diagnostic patch subset, the same agent using rg + CodeGraph had
  fewer wrong-file edits than the same agent using rg alone."
- "This is a local diagnostic ablation, not an official leaderboard result."
- "CodeGraph reduced claimability violations from A to B on this pinned task
  set."

## Unsafe Wording

Do not say:

- "CodeGraph gets X% on SWE-bench."
- "CodeGraph beats rg."
- "CodeGraph beats CGC."
- "CodeGraph improves real-world recall."
- "CodeGraph solves SWE-bench."
- "Vector or nuance candidates prove behavior."

These statements are only allowed when the exact official or comparable harness
requirements were met, documented, and the result is not diagnostic-only.

Also do not cite RepoBench or CrossCodeEval numbers from runs where provider
query terms came from gold files, gold symbols, hidden context filenames,
expected completions, or answer metadata. Those artifacts are
`gold_hint_diagnostic` / `over_assisted_diagnostic` only and are not clean
retrieval comparisons.

## Proof Boundaries

- Text evidence is source-text existence, not graph proof.
- Symbol evidence is entity existence, not behavior proof.
- Lexical, vector, binary, nuance, and graph-neighborhood results are candidate
  evidence only.
- Graph/source verification is required before a graph relation is claimable.
- Mock-agent and plumbing runs never count as model quality.
- Recall/MRR without precision, wrong-context, and context-poison metrics is not
  enough to claim retrieval quality.
- Forbidden-context hits or dangerous-context flags must be reported, not hidden
  by high recall.

## Setup Status Boundaries

- `ready` means the configured local files/tools are present for the named
  setup level. It does not mean a benchmark score exists.
- `partial_smoke_ready` means the adapter can load or inspect a small task set,
  but a larger configured run is not yet verified.
- `ready_for_full_run` means the configured diagnostic retrieval run can run on
  available local data.
- `ready_for_official_smoke` means the upstream harness/setup path has completed
  a bounded smoke, but this is still not a public score.
- `ready_for_gold_validation` means SWE-bench gold validation has completed or
  can be rerun through the recorded Linux route.
- `gold_validation_live_ready` means the current preflight can reach the
  Docker/Linux harness route. It is a setup/live-run readiness state, not a
  patch-quality result.
- `ready_for_official_smoke` on SWE-bench patch quality means the external
  agent command validates and setup can run. It is not a real-agent score until
  predictions are generated and evaluated through the harness.
- `ready_for_retrieval_blocked_official_generation` means product-ablation
  retrieval/context scoring can run, but the upstream generation scorer is not
  ready.
- `fixture_only` means only tiny tracked non-official fixture data is available.
- `blocked_*` means the blocker must be fixed before that external benchmark can
  produce comparable results.
- `blocked_host_platform` means the local host cannot run the upstream harness
  as documented, for example Windows Python missing a Unix-only module.

Do not convert setup readiness into performance language. A source checkout,
extracted dataset, Docker check, or mock-agent run is not a CodeGraph quality
result.

## Suite Claim Boundaries

The operator suite command:

```powershell
python -m benchmarks.harness.runners.run_benchmark_suite --suite full --output-dir benchmarks/results/summaries/<run_id>
```

is still local diagnostic infrastructure unless the exact official/comparable
harness requirements are met and the result is intentionally promoted. The
presence of `summary.json`, charts, timing buckets, or quality-per-budget
metrics is not a public claim by itself.

SWE-bench boundaries:

- Cached prior gold validation is evidence only and does not count as a current
  live gold-validation run.
- Docker failure must produce a blocked live-run status, not a green result.
- Missing `CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND` blocks patch quality.
- Current live gold validation may be reported only when the Docker/Linux
  harness actually reruns in the current environment. On 2026-05-23,
  `codegraph-live-gold-20260523-083023` completed 1/1 gold validation for
  `sympy__sympy-20590`; this remains harness readiness evidence, not model
  quality.
- The Codex external-agent wrapper can be setup-ready via
  `CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND`, but setup readiness is not a
  patch-quality score.
- The one-task local external-agent smoke
  `swebench_official_compatible_smoke_20260523_174023` produced resolving
  patches for `baseline` and `rg_only`, but both failed the clean-source-patch
  gate due an extra test-file edit. This is local diagnostic evidence only, not
  an official SWE-bench score and not CodeGraph attribution.
- The later one-task E2E diagnostic
  `swebench_lite_e2e_20260523_192718` confirmed real external-agent patch
  generation and Docker evaluation for `baseline` and `rg_only`. Both measured
  modes resolved but failed the clean-source-patch gate due the same extra
  test-file edit. `codegraph_exact_text` and `codegraph_full` were skipped
  before agent execution because context was invalid for attribution:
  `blocked_index_timeout; candidate_spool_present_but_no_gold_hit`. This is
  local diagnostic evidence only, not an official SWE-bench score and not
  CodeGraph patch-quality evidence.
- Mock-agent runs are scaffold-only and never count as model quality.
- No SWE-bench score or real-agent patch-quality claim exists unless actual
  external-agent predictions are evaluated through the official-compatible
  harness and skipped/failed tasks are reported.

Safe current wording:

```text
The local one-task SWE-bench Lite E2E path can generate real Codex patches and
evaluate them through Docker for baseline and rg_only.
```

Unsafe current wording:

```text
CodeGraph improves SWE-bench patch quality.
CodeGraph solved SWE-bench.
The CodeGraph SWE-bench readiness gate is complete.
```

Timing boundaries:

- Cold DB build and optional vector sidecar build are setup costs and must be
  shown separately.
- Warm retrieval excludes cold setup but includes query/context-pack subprocess
  cost where the current CLI path pays it.
- Raw end-to-end first-use timing remains visible; cold setup is never hidden
  inside one task average.

Leakage boundaries:

- Provider-visible fields and evaluator-only fields are separate.
- Gold files/symbols/context paths, expected answer files, forbidden files, and
  oracle patch metadata must never be passed to providers.
- `blocked_query_leakage` means a track did not produce a clean benchmark
  result and must not be summarized as completed.
