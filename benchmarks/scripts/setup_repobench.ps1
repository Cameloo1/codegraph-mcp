$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$target = Join-Path $repoRoot "benchmarks\tracks\repobench\scripts\setup_repobench.ps1"
& $target @args
exit $LASTEXITCODE
