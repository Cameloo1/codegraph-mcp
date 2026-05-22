$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$target = Join-Path $repoRoot "benchmarks\tracks\crosscodeeval\scripts\setup_windows_msvc.ps1"
& $target @args
exit $LASTEXITCODE
