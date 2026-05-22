param(
    [string]$EnvFile = "",
    [string]$OutputDir = "",
    [int]$Iterations = 0,
    [switch]$ValidateOnly
)

$ErrorActionPreference = "Stop"

function Get-RepoRoot {
    $scriptDir = Split-Path -Parent $PSCommandPath
    return (Resolve-Path -LiteralPath (Join-Path $scriptDir "..\..")).Path
}

function Import-DotEnvFile {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "env_file_missing: $Path"
    }

    $loaded = New-Object System.Collections.Generic.List[string]
    $lines = Get-Content -LiteralPath $Path
    foreach ($line in $lines) {
        $trimmed = $line.Trim()
        if (-not $trimmed -or $trimmed.StartsWith("#")) {
            continue
        }

        if ($trimmed -notmatch '^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$') {
            throw "invalid_env_line: $trimmed"
        }

        $name = $Matches[1]
        $value = $Matches[2].Trim()
        if (
            ($value.StartsWith('"') -and $value.EndsWith('"')) -or
            ($value.StartsWith("'") -and $value.EndsWith("'"))
        ) {
            $value = $value.Substring(1, $value.Length - 2)
        }

        [Environment]::SetEnvironmentVariable($name, $value, "Process")
        [void]$loaded.Add($name)
    }

    return $loaded
}

function Assert-OpenEvolveKey {
    $key = [Environment]::GetEnvironmentVariable("OPENAI_API_KEY", "Process")
    if (-not $key -or -not $key.Trim()) {
        throw "openai_api_key_missing: set OPENAI_API_KEY in benchmarks/openevolve/.env.local"
    }
    if ($key -match 'your_key_here|<provider-key>|<.*>|paste') {
        throw "openai_api_key_placeholder: replace the placeholder in benchmarks/openevolve/.env.local"
    }
}

function Write-TextFile {
    param([string]$Path, [string]$Text)
    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    Set-Content -LiteralPath $Path -Value $Text -Encoding UTF8
}

$repo = Get-RepoRoot
if (-not $EnvFile) {
    $EnvFile = Join-Path $repo "benchmarks\openevolve\.env.local"
}
$EnvFile = (Resolve-Path -LiteralPath $EnvFile).Path

$loadedVars = Import-DotEnvFile -Path $EnvFile
Assert-OpenEvolveKey
[Environment]::SetEnvironmentVariable("PYTHONIOENCODING", "utf-8", "Process")
[Environment]::SetEnvironmentVariable("PYTHONUTF8", "1", "Process")

if ($Iterations -le 0) {
    $envIterations = [Environment]::GetEnvironmentVariable("CODEGRAPH_OPENEOLVE_ITERATIONS", "Process")
    if ($envIterations) {
        $Iterations = [int]$envIterations
    } else {
        $Iterations = 10
    }
}

$openEvolveRoot = Join-Path $repo "benchmarks\workspaces\openevolve_research"
$python = Join-Path $openEvolveRoot ".venv\Scripts\python.exe"
$runner = Join-Path $openEvolveRoot "openevolve-run.py"
$target = Join-Path $repo "benchmarks\openevolve\targets\candidate_spool_policy.py"
$evaluator = Join-Path $repo "benchmarks\openevolve\evaluators\evaluate_candidate_spool_policy.py"
$config = Join-Path $repo "benchmarks\openevolve\configs\candidate_spool_policy_smoke.yaml"

foreach ($required in @($python, $runner, $target, $evaluator, $config)) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "required_path_missing: $required"
    }
}

if (-not $OutputDir) {
    $runId = Get-Date -Format "yyyyMMdd_HHmmss"
    $OutputDir = Join-Path $repo "benchmarks\workspaces\openevolve_runs\candidate_spool_policy_$runId"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$OutputDir = (Resolve-Path -LiteralPath $OutputDir).Path

$baselinePath = Join-Path $OutputDir "baseline_metrics.json"
$logPath = Join-Path $OutputDir "openevolve_smoke.log"
$commandPath = Join-Path $OutputDir "run_command.txt"

$displayCommand = @(
    "powershell -ExecutionPolicy Bypass -File .\benchmarks\scripts\run_openevolve_candidate_spool_policy.ps1",
    "-OutputDir `"$OutputDir`"",
    "-Iterations $Iterations"
) -join " "
Write-TextFile -Path $commandPath -Text $displayCommand

Write-Host "Loaded env names from: $EnvFile"
Write-Host (($loadedVars | Sort-Object -Unique) -join ", ")
Write-Host "Output dir: $OutputDir"
Write-Host "Iterations: $Iterations"
Write-Host "Run command saved to: $commandPath"

if ($ValidateOnly) {
    Write-Host "validate_only_ok"
    exit 0
}

& $python $evaluator $target --output $baselinePath
if ($LASTEXITCODE -ne 0) {
    throw "baseline_evaluator_failed: exit=$LASTEXITCODE"
}

Push-Location $openEvolveRoot
try {
    $oldErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $nativeOutput = & $python $runner $target $evaluator --config $config --output $OutputDir --iterations $Iterations --log-level INFO 2>&1
    $exitCode = $LASTEXITCODE
} finally {
    $ErrorActionPreference = $oldErrorActionPreference
    Pop-Location
}

$nativeOutput | ForEach-Object { $_.ToString() } | Tee-Object -FilePath $logPath

if ($exitCode -ne 0) {
    throw "openevolve_failed: exit=$exitCode log=$logPath"
}

Write-Host "openevolve_run_complete"
Write-Host "Log: $logPath"
