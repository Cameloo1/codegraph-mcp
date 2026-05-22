$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$target = Join-Path $repoRoot "benchmarks\tracks\swebench_lite\scripts\setup_wsl.ps1"
& $target @args
exit $LASTEXITCODE
