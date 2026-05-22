# OpenEvolve Lab Branch

This branch is for experimental OpenEvolve policy work around CodeGraph
retrieval, candidate query ranking, planned provider behavior, and bounded
context preparation.

The lab branch is intentionally separate from the release pipeline:

- `fix` contains release-ready CodeGraph changes, stable benchmark harness code,
  claim-safe docs, and manually reviewed product improvements.
- `openevolve-lab` contains experimental policy variants, evaluator tweaks,
  sanitized metrics summaries, and replay notes.

Do not merge this branch wholesale into `fix`. Promote useful ideas by replaying
the evaluator, reviewing the candidate, manually porting the product change, and
running the normal CodeGraph checks on `fix`.

## Allowed On This Branch

- Evolvable policy modules under `benchmarks/tracks/openevolve/`.
- Evaluator variants and synthetic fixtures.
- Sanitized experiment summaries.
- Replay scripts and notes that explain how to reproduce an experiment.
- Larger fixed candidate inventories used to replay policy ideas.
- Planned retrieval policy experiments for `codegraph_planned` and
  `rg_planned`.

## Keep Local Or Ignored

- `.env.local` and all API keys.
- Raw model logs that may contain prompts, responses, or secrets.
- `benchmarks/tracks/*/workspaces/**` and legacy `benchmarks/workspaces/**` run outputs.
- Generated DBs, vector artifacts, spools, patches, predictions, and raw logs.
- Upstream cloned repositories and downloaded benchmark payloads.

## Promotion Path

1. Run the OpenEvolve experiment on this branch.
2. Save only sanitized summaries if the run is worth preserving.
3. Replay the best candidate against the fixed evaluator.
4. Compare against the baseline policy.
5. Manually port the durable idea into product code on `fix`.
6. Run targeted tests, benchmark smokes, claimability checks, and artifact checks.
7. Commit the reviewed product change to `fix`.

OpenEvolve output is optimization evidence only. It is not a public benchmark
claim and it is not product code until manually reviewed and promoted.

The old candidate-spool firehose is no longer the primary lab target. CodeGraph
now has bounded selection, packet aggregation, SQLite query indexing,
budget-graceful optional spool behavior, and lifecycle/source-binding checks in
the product baseline. OpenEvolve should now focus on improving how those
surfaces are used, ranked, and compared under fixed evaluators.
