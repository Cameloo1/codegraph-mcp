$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$target = Join-Path $repoRoot "benchmarks\tracks\crosscodeeval\scripts\setup_docker.ps1"
& $target @args
exit $LASTEXITCODE
