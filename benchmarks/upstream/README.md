# Upstream Benchmark Pins

This directory stores the aggregate compatibility source manifest. Canonical
per-track upstream pins now live under
`benchmarks/tracks/<track>/upstream/pinned_source.json`.

Tracked files:

- `README.md`
- `.gitkeep`
- `pinned_sources.json`

Legacy ignored/local paths:

- `SWE-bench/`
- `repobench/`
- `cceval/`
- any other checked-out upstream benchmark repository

The ignored checkouts may exist locally for setup verification during the
compatibility window, but they are not part of the public repo.
`pinned_sources.json` remains the aggregate manifest, while new setup should
prefer each track's `upstream/` directory.
