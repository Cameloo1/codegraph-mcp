param(
    [string]$Checkout = "benchmarks/tracks/crosscodeeval/upstream/cceval"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")
$checkoutCandidate = Join-Path $repoRoot $Checkout
if (-not (Test-Path -LiteralPath $checkoutCandidate)) {
    $checkoutCandidate = Join-Path $repoRoot "benchmarks\upstream\cceval"
}
$checkoutPath = Resolve-Path $checkoutCandidate
$venvPython = Join-Path $repoRoot "benchmarks\tracks\crosscodeeval\workspaces\benchmark-setup-venv\Scripts\python.exe"
if (-not (Test-Path -LiteralPath $venvPython)) {
    python -m venv (Join-Path $repoRoot "benchmarks\tracks\crosscodeeval\workspaces\benchmark-setup-venv")
}
& $venvPython -m pip install tree_sitter==0.20.4

$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path -LiteralPath $vcvars)) {
    throw "Visual Studio Build Tools vcvars64.bat not found. Install Microsoft.VisualStudio.Workload.VCTools."
}

$cmd = "call `"$vcvars`" && cd /d `"$checkoutPath`" && where cl && where rc && `"$venvPython`" scripts\build_ts_lib.py"
cmd.exe /d /c $cmd
if ($LASTEXITCODE -ne 0) {
    throw "CrossCodeEval MSVC parser build failed. Try benchmarks/tracks/crosscodeeval/scripts/setup_docker.ps1."
}
