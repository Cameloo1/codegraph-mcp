param(
    [string]$Checkout = "benchmarks/upstream/cceval"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$checkoutPath = Resolve-Path (Join-Path $repoRoot $Checkout)
$venvPython = Join-Path $repoRoot "benchmarks/workspaces/benchmark-setup-venv/Scripts/python.exe"
if (-not (Test-Path -LiteralPath $venvPython)) {
    python -m venv (Join-Path $repoRoot "benchmarks/workspaces/benchmark-setup-venv")
}
& $venvPython -m pip install tree_sitter==0.20.4

$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path -LiteralPath $vcvars)) {
    throw "Visual Studio Build Tools vcvars64.bat not found. Install Microsoft.VisualStudio.Workload.VCTools."
}

$cmd = "call `"$vcvars`" && where cl && where rc && `"$venvPython`" scripts\build_ts_lib.py"
cmd.exe /d /c $cmd
if ($LASTEXITCODE -ne 0) {
    throw "CrossCodeEval MSVC parser build failed. Try benchmarks/scripts/setup_crosscodeeval_docker.ps1."
}
