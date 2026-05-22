$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$target = Join-Path $repoRoot "benchmarks\tracks\crosscodeeval\scripts\setup_wsl.ps1"
& $target @args
exit $LASTEXITCODE
