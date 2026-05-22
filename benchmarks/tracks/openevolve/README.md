# OpenEvolve Policy Lab

This directory contains small, isolated OpenEvolve experiments for CodeGraph
policy tuning. These runs are experimental design inputs, not shipped product
behavior and not public benchmark evidence.

OpenEvolve is used here as a bounded policy-search engine. It proposes small
retrieval and ranking policy variants, then fixed evaluators score them for
recall, latency, packet size, diversity, and claim-boundary safety. It must not
edit CodeGraph Rust crates, normal benchmark reports, README assets, MVP docs,
or any normal `.codegraph` database.

The first scaffold still uses a standalone Python candidate-spool policy target,
but the objective has changed. Bounded candidate-spool size, packet aggregation,
SQLite query indexing, budget-graceful optional spool behavior, and
lifecycle/source-binding checks are now product baseline. The lab should use
that baseline to tune ranking quality, planned retrieval policy, strong
human-style `rg` comparison policy, and larger-corpus replay.

Generated run outputs belong under:

```text
benchmarks/tracks/openevolve/workspaces/runs/
```

That path is ignored. A good candidate from this lab still requires a separate human-reviewed promotion pass before any Rust implementation change.

## Local Env File

The recommended local secret file is:

```text
benchmarks/tracks/openevolve/.env.local
```

That file is ignored by git. It should contain `OPENAI_API_KEY` and the exact runner command as a comment.

## Smoke Run

Paste the key into `benchmarks/tracks/openevolve/.env.local`, then run from the repo root:

```powershell
powershell -ExecutionPolicy Bypass -File .\benchmarks\tracks\openevolve\scripts\run_candidate_spool_policy.ps1
```

The runner loads only process-local environment variables from `.env.local`, writes outputs under the ignored `benchmarks/tracks/openevolve/workspaces/runs/` tree, and does not print secret values.

The raw OpenEvolve command remains:

```powershell
$runId = Get-Date -Format "yyyyMMdd_HHmmss"
$repo = (Get-Location).Path
$out = "$repo\benchmarks\tracks\openevolve\workspaces\runs\candidate_spool_policy_$runId"

python "$repo\benchmarks\workspaces\openevolve_research\openevolve-run.py" `
  "$repo\benchmarks\tracks\openevolve\targets\candidate_spool_policy.py" `
  "$repo\benchmarks\tracks\openevolve\evaluators\evaluate_candidate_spool_policy.py" `
  --config "$repo\benchmarks\tracks\openevolve\configs\candidate_spool_policy_smoke.yaml" `
  --output "$out" `
  --iterations 10 `
  --log-level INFO
```

The evaluator hard-fails policies that drop required gold hits, emit graph-proof claims from candidate evidence, exceed packet budgets, or behave nondeterministically.

## Current Priorities

See [CURRENT_PRIORITIES.md](CURRENT_PRIORITIES.md) for the current rebased
objectives. In short:

1. Replay candidate-spool policy ideas on a larger real corpus.
2. Tune candidate query ranking over the current SQLite query index.
3. Build/evaluate `codegraph_planned` provider policy.
4. Build/evaluate `rg_planned` as the fair strong baseline.
5. Tune runtime vector chunk-selection weights only after planned retrieval
   exposes quality gaps.
