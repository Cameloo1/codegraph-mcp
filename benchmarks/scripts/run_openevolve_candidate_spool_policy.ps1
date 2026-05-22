$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$target = Join-Path $repoRoot "benchmarks\tracks\openevolve\scripts\run_candidate_spool_policy.ps1"
& $target @args
exit $LASTEXITCODE
