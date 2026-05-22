$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$target = Join-Path $repoRoot "benchmarks\tracks\swebench_lite\scripts\run_harness_linux_container.ps1"
& $target @args
exit $LASTEXITCODE
