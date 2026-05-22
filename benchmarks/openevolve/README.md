# OpenEvolve Policy Lab

This directory contains small, isolated OpenEvolve experiments for CodeGraph policy tuning. These runs are experimental design inputs, not shipped product behavior and not public benchmark evidence.

The first experiment tunes a standalone Python candidate-spool policy for packet grouping, ranking, and caps. OpenEvolve mutates only the policy file given to its CLI. It must not edit CodeGraph Rust crates, normal benchmark reports, README assets, MVP docs, or any normal `.codegraph` database.

Generated run outputs belong under:

```text
benchmarks/workspaces/openevolve_runs/
```

That path is ignored. A good candidate from this lab still requires a separate human-reviewed promotion pass before any Rust implementation change.

## Local Env File

The recommended local secret file is:

```text
benchmarks/openevolve/.env.local
```

That file is ignored by git. It should contain `OPENAI_API_KEY` and the exact runner command as a comment.

## Smoke Run

Paste the key into `benchmarks/openevolve/.env.local`, then run from the repo root:

```powershell
powershell -ExecutionPolicy Bypass -File .\benchmarks\scripts\run_openevolve_candidate_spool_policy.ps1
```

The runner loads only process-local environment variables from `.env.local`, writes outputs under the ignored `benchmarks/workspaces/openevolve_runs/` tree, and does not print secret values.

The raw OpenEvolve command remains:

```powershell
$runId = Get-Date -Format "yyyyMMdd_HHmmss"
$repo = (Get-Location).Path
$out = "$repo\benchmarks\workspaces\openevolve_runs\candidate_spool_policy_$runId"

python "$repo\benchmarks\workspaces\openevolve_research\openevolve-run.py" `
  "$repo\benchmarks\openevolve\targets\candidate_spool_policy.py" `
  "$repo\benchmarks\openevolve\evaluators\evaluate_candidate_spool_policy.py" `
  --config "$repo\benchmarks\openevolve\configs\candidate_spool_policy_smoke.yaml" `
  --output "$out" `
  --iterations 10 `
  --log-level INFO
```

The evaluator hard-fails policies that drop required gold hits, emit graph-proof claims from candidate evidence, exceed packet budgets, or behave nondeterministically.
