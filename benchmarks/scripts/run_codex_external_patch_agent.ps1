$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$target = Join-Path $repoRoot "benchmarks\tracks\swebench_lite\scripts\run_codex_external_patch_agent.ps1"
& $target @args
exit $LASTEXITCODE
