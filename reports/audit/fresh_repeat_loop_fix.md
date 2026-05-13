# Autoresearch Update Repro Fix

## Diagnosis

The previous fresh repeat loop timeout was a harness boundary bug: the run could cold-build the Autoresearch proof DB before measuring repeat, and result JSON was only written after the whole harness completed. That made a production repeat-fast path look like a missing-artifact timeout.

This report uses an explicit fresh seed artifact and `--loop-kind repeat-fast`, so the measured loop is only repeat unchanged indexing. Setup work such as artifact copy/open, schema validation, graph digest setup, quick checks, markdown rendering, and JSON writing is reported separately.

Verdict: `passed`. Repeat-fast p95 is `3125 ms` against the `<=5000 ms` target, with `files_read=0`, `files_hashed=0`, and `files_parsed=0` in every repeat iteration.

Verdict: `passed`

Update mode: `update-fast`

| Repo | Status | Iterations | Repeat Hash Stable | Changed Hash | Restore Hash | Integrity |
| --- | --- | ---: | --- | --- | --- | --- |
| `autoresearch` | `passed` | 5 | `true` | `true` | `true` | `true` |

## Step Metrics

### `autoresearch`

Mutation file: `python/autoresearch_utils/metrics_tools.py`

| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` | 939 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | `ok` |
| `repeat_unchanged_index` | 3070 | 8564 | 0 | 0 | 0 | 0 | 0 | 0 | `ok` |
