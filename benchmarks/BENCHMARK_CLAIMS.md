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

Only the context layer may vary:

- `baseline`
- `rg_only`
- `codegraph_exact_text`
- `codegraph_full`

## Safe Wording

- "On a pinned internal 20-task retrieval set, CodeGraph full improved
  gold-file recall@5 from A to B over rg-only."
- "On a pinned SWE-bench Lite 10-task diagnostic subset, the same configured
  agent solved A tasks without CodeGraph and B tasks with CodeGraph."
- "This is a local diagnostic ablation, not an official leaderboard result."
- "CodeGraph reduced claimability violations from A to B on this pinned task
  set."

## Unsafe Wording

Do not say:

- "CodeGraph gets X% on SWE-bench."
- "CodeGraph beats CGC."
- "CodeGraph improves real-world recall."
- "CodeGraph solves SWE-bench."
- "Vector or nuance candidates prove behavior."

These statements are only allowed when the exact official or comparable harness
requirements were met, documented, and the result is not diagnostic-only.

## Proof Boundaries

- Text evidence is source-text existence, not graph proof.
- Symbol evidence is entity existence, not behavior proof.
- Lexical, vector, binary, nuance, and graph-neighborhood results are candidate
  evidence only.
- Graph/source verification is required before a graph relation is claimable.
- Mock-agent and plumbing runs never count as model quality.

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
