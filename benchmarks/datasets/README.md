# Legacy Benchmark Dataset Compatibility

Canonical tracked datasets and fixtures now live under the owning benchmark
track in `benchmarks/tracks/<track>/`.

This directory is kept only as a compatibility reference while older commands
and docs migrate. Large official benchmark datasets, cloned repositories,
Docker layers, raw model outputs, predictions, and patches belong under ignored
track-local workspaces or results directories.

Current tiny non-official adapter fixtures:

- `benchmarks/tracks/repobench/fixtures/`
- `benchmarks/tracks/crosscodeeval/fixtures/`

Do not cite these fixtures as RepoBench or CrossCodeEval results.
