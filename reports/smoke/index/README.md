# Index Smoke Reports

`scripts/smoke_index.ps1` and `scripts/smoke_index.sh` write their logs here.

The fixture index smoke is mandatory and uses `fixtures/smoke/basic_repo`.
Full-repo `codegraph-mcp index .` is a local-only smoke by default and is skipped
under `CI=true` unless explicitly requested.
