# Upstream Benchmark Pins

This directory stores source pins and setup notes.

Tracked files:

- `README.md`
- `.gitkeep`
- `pinned_sources.json`

Ignored/local paths:

- `SWE-bench/`
- `repobench/`
- `cceval/`
- any other checked-out upstream benchmark repository

The ignored checkouts may exist locally for setup verification, but they are not
part of the public repo. `pinned_sources.json` records the pinned commit, local
path if present, setup command, current status, and exact blocker.
