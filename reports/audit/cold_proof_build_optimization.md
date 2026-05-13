# Cold Proof Build Optimization

Generated: 2026-05-12 15:07:49 -05:00

Source of truth: `MVP.md`.

## Verdict

Status: **fail, improved but not at target**.

The production `proof-build-only` path is now separated from validation/audit/full-gate work and uses a hidden-temp bulk-load mode, but the Autoresearch proof build still did not complete inside the measurement cap.

Target: `<=60s`.

Observed this phase: `>244s` before timeout, with only 9 batches completed.

This is still not close enough. The remaining bottleneck is not CGC, storage audit, relation sampling, markdown reporting, or manual labels. It is still cold-build persistence for large template-heavy batches.

## Modes Confirmed

| Mode | Command | Purpose | Counted as production cold target? |
| --- | --- | --- | --- |
| `proof-build-only` | `codegraph-mcp index <repo> --build-mode proof-build-only --storage-mode proof --profile --json` | Production proof DB build. Excludes storage audit, relation sampler, CGC, manual labels, repeated rebuilds, and full markdown report generation. | yes |
| `proof-build-plus-validation` | `codegraph-mcp index <repo> --build-mode proof-build-plus-validation --storage-mode proof --profile --json` | Proof DB build plus full integrity validation. | no |
| `proof-build-plus-audit` | full gate or explicit audit/debug sidecar build | Audit/debug sidecar work. | no |
| `full-comprehensive-gate` | `codegraph-mcp bench comprehensive` | Aggregated gate/reporting surface. | no |

## What Changed

- Added explicit `proof-build-only` and `proof-build-plus-validation` build modes to the index path.
- Added CLI support for `--build-mode <proof-build-only|proof-build-plus-validation>`.
- Kept cold index atomic: build hidden temp DB first, publish only after the configured gate passes.
- Added hidden-temp bulk-load pragmas for `proof-build-only`: `journal_mode=OFF`, `synchronous=OFF`, memory temp store, larger cache, and cache-spill disabled during the hidden temp build.
- Restored durable WAL/FULL behavior before publish.
- Skipped the in-transaction quick check for hidden proof-only cold temp builds; final hidden-temp and final visible DB quick gates still run after commit.
- Kept full integrity behavior in `proof-build-plus-validation`.
- Kept proof-build-only fast bulk finish separate from validation-mode `ANALYZE` and checkpoint-heavy maintenance.
- Kept prepared template inserts and store-local dictionary cache.

## Autoresearch Partial Benchmark

Command:

```text
codegraph-mcp index <REPO_ROOT>/Desktop/development/autoresearch-codexlab --db reports/audit/artifacts/cold_proof_build_fast_autoresearch_after_pragmas.sqlite --profile --json --workers 16 --storage-mode proof --build-mode proof-build-only
```

Result: **timeout at 244s**.

No completed Autoresearch DB was produced by this run, so this phase does not claim a clean large-repo DB size or integrity result.

Comparable first 9 completed batches:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| DB write time | `209,128 ms` | `151,785 ms` | `-57,343 ms` / `-27.42%` |

Largest comparable batch improvements:

| Batch | Before DB Write | After DB Write | Delta | Entities | Edges |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | `107,936 ms` | `81,335 ms` | `-26,601 ms` / `-24.65%` | 51,240 | 41,263 |
| 2 | `28,337 ms` | `16,426 ms` | `-11,911 ms` / `-42.03%` | 47,590 | 14,112 |
| 8 | `23,298 ms` | `13,050 ms` | `-10,248 ms` / `-43.99%` | 29,200 | 10,917 |
| 9 | `47,637 ms` | `39,268 ms` | `-8,369 ms` / `-17.57%` | 51,994 | 17,722 |

The improvement is real, but the remaining batch cost is still far too high. Batch 1 alone is above the entire intended cold-build budget.

## Fixture Build Checks

| Mode | Status | Wall | DB Write | Gate |
| --- | --- | ---: | ---: | --- |
| `proof-build-only` | passed | `928 ms` | `61 ms` | quick publish checks |
| `proof-build-plus-validation` | passed | `985 ms` | `117 ms` | full integrity checks |

Artifacts:

- `reports/audit/artifacts/cold_proof_build_fixture_proof_only.json`
- `reports/audit/artifacts/cold_proof_build_fixture_validation.json`

## Correctness Gates

| Gate | Result |
| --- | --- |
| Graph Truth Gate | `11/11 passed` |
| Context Packet Gate | `11/11 passed` |
| Full Rust tests | `cargo test --workspace` passed |
| Comprehensive benchmark | refreshed; still fail because persisted Autoresearch cold build and storage targets remain failed |

## Remaining Bottlenecks

1. Template and dictionary persistence inside large batches. Batch 1 still spent `81.335s` in `db_write_ms`.
2. Large generated/source-heavy templates. Batches with tens of thousands of template entities dominate the cold build.
3. Per-row dictionary and template writes. Even the tiny fixture profile shows dictionary lookup/insert is the dominant profiled insert span.
4. Template table materialization model. The system still writes hundreds of thousands of template rows during cold proof build.
5. Post-build index creation and validation are now separated; they are not the primary explanation for the timeout.

## Storage Safety Questions

1. Did graph truth still pass? **Yes, 11/11.**
2. Did context packet quality still pass? **Yes, 11/11.**
3. Did proof DB size decrease? **Not claimed.** No completed Autoresearch DB was produced by this phase.
4. Did removed data move to a sidecar, become derivable, or get proven unnecessary? **No proof data was removed.** This phase changed build-time durability/profile behavior only.

## Next Required Work

- Split template/dictionary persistence further inside `db_write_ms`.
- Replace per-row dictionary lookup/insert with batch interning for symbols, qname prefixes, qualified names, and provenance IDs.
- Bulk-load `template_entities` and `template_edges` through temporary staging tables or prepared multi-row chunks.
- Keep validation, audit, relation sampling, and reporting out of proof-build-only measurements.

## Conclusion

Production cold proof build time moved in the right direction, but not enough. The system remains blocked from claiming intended large-repo cold-build performance until template/dictionary persistence is redesigned for true bulk load.
