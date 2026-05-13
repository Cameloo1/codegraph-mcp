param(
    [switch] $RunRepoIndex,
    [switch] $SkipRepoIndex
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $PSCommandPath
$repoRoot = (Resolve-Path (Join-Path $scriptDir "..")).Path
$runId = "$(Get-Date -Format 'yyyyMMdd_HHmmss')_$(([guid]::NewGuid().ToString('N')).Substring(0, 8))"
$logRoot = Join-Path $repoRoot "reports\smoke\index"
$runLogDir = Join-Path $logRoot "windows_$runId"
$summaryPath = Join-Path $runLogDir "summary.json"
$steps = @()

New-Item -ItemType Directory -Force -Path $runLogDir | Out-Null

function Add-StepResult {
    param(
        [string] $Name,
        [string] $Status,
        [int] $ExitCode,
        [double] $DurationMs,
        [string] $LogPath,
        [string] $Command,
        [string] $Notes = $null
    )

    $script:steps += [pscustomobject]@{
        name = $Name
        status = $Status
        exit_code = $ExitCode
        duration_ms = [math]::Round($DurationMs, 3)
        log = $LogPath
        command = $Command
        notes = $Notes
    }
}

function ConvertTo-CmdArgument {
    param([string] $Value)

    '"' + ($Value -replace '"', '\"') + '"'
}

function Invoke-LoggedCommand {
    param(
        [string] $Name,
        [string[]] $Command,
        [string] $DisplayCommand = $null,
        [string] $Notes = $null
    )

    $logPath = Join-Path $runLogDir "$Name.log"
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    $display = if ($DisplayCommand) { $DisplayCommand } else { $Command -join " " }
    "repo: $repoRoot" | Set-Content -LiteralPath $logPath -Encoding UTF8
    "command: $display" | Add-Content -LiteralPath $logPath -Encoding UTF8
    "" | Add-Content -LiteralPath $logPath -Encoding UTF8

    $commandLine = "cd /d $(ConvertTo-CmdArgument $repoRoot) && " +
        (($Command | ForEach-Object { ConvertTo-CmdArgument $_ }) -join " ") +
        " >> $(ConvertTo-CmdArgument $logPath) 2>&1"

    try {
        & $env:ComSpec /d /s /c $commandLine
        $exitCode = $LASTEXITCODE
    } finally {
        $timer.Stop()
    }

    if ($null -eq $exitCode) {
        $exitCode = 0
    }

    Add-StepResult -Name $Name -Status "completed" -ExitCode $exitCode -DurationMs $timer.Elapsed.TotalMilliseconds -LogPath $logPath -Command $display -Notes $Notes
    if ($exitCode -ne 0) {
        throw "$Name failed with exit code $exitCode. See $logPath"
    }
}

function Add-SkippedStep {
    param(
        [string] $Name,
        [string] $Command,
        [string] $Reason
    )

    $logPath = Join-Path $runLogDir "$Name.log"
    "status: skipped" | Set-Content -LiteralPath $logPath -Encoding UTF8
    "command: $Command" | Add-Content -LiteralPath $logPath -Encoding UTF8
    "reason: $Reason" | Add-Content -LiteralPath $logPath -Encoding UTF8
    Add-StepResult -Name $Name -Status "skipped" -ExitCode 0 -DurationMs 0 -LogPath $logPath -Command $Command -Notes $Reason
}

function Set-Utf8NoBomFile {
    param(
        [string] $Path,
        [string] $Value
    )

    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Value, $encoding)
}

function Write-Summary {
    param(
        [string] $Status,
        [string] $Failure = $null
    )

    $summaryJson = [pscustomobject]@{
        schema_version = 1
        status = $Status
        failure = $Failure
        run_id = $runId
        generated_at = (Get-Date).ToString("o")
        platform = "windows"
        repo_root = $repoRoot
        fixture_repo = "fixtures/smoke/basic_repo"
        logs_dir = $runLogDir
        fixture_index_mandatory = $true
        repo_index_local_only = $true
        repo_index_requested = (-not $SkipRepoIndex -and (-not $env:CI -or $RunRepoIndex))
        requirements = @{
            cgc_required = $false
            autoresearch_required = $false
            external_benchmark_artifacts_required = $false
            network_required = "only normal Cargo dependency resolution"
        }
        steps = @($steps)
    } | ConvertTo-Json -Depth 8

    Set-Utf8NoBomFile -Path $summaryPath -Value ($summaryJson + [Environment]::NewLine)
}

try {
    $binaryPath = Join-Path $repoRoot "target\debug\codegraph-mcp.exe"
    $fixtureDb = Join-Path $runLogDir "basic_repo.sqlite"
    $repoDb = Join-Path $runLogDir "repo_index.sqlite"

    Set-Content -LiteralPath (Join-Path $runLogDir "environment.txt") -Encoding UTF8 -Value @(
        "repo_root=$repoRoot",
        "fixture_repo=fixtures/smoke/basic_repo",
        "fixture_index_mandatory=true",
        "repo_index_local_only=true",
        "ci=$env:CI",
        "cargo=$((Get-Command cargo).Source)"
    )

    Invoke-LoggedCommand -Name "cargo_build" -Command @("cargo", "build", "--workspace")
    Invoke-LoggedCommand `
        -Name "fixture_index" `
        -Command @($binaryPath, "index", "fixtures/smoke/basic_repo", "--db", $fixtureDb, "--profile", "--json") `
        -DisplayCommand "target/debug/codegraph-mcp index fixtures/smoke/basic_repo --db $fixtureDb --profile --json" `
        -Notes "mandatory CI smoke; uses explicit --db to keep fixture source clean"

    if ($SkipRepoIndex) {
        Add-SkippedStep -Name "repo_index" -Command "target/debug/codegraph-mcp index ." -Reason "skipped by -SkipRepoIndex"
    } elseif ($env:CI -and -not $RunRepoIndex) {
        Add-SkippedStep -Name "repo_index" -Command "target/debug/codegraph-mcp index ." -Reason "repo index is local-only by default in CI; fixture index is the mandatory CI smoke"
    } else {
        Invoke-LoggedCommand `
            -Name "repo_index" `
            -Command @($binaryPath, "index", ".", "--db", $repoDb, "--profile", "--json") `
            -DisplayCommand "target/debug/codegraph-mcp index . --db $repoDb --profile --json" `
            -Notes "local-only smoke; can be skipped in CI because full-repo indexing is larger than the deterministic fixture"
    }

    Write-Summary -Status "pass"
    Write-Host "index smoke passed"
    Write-Host "logs: $runLogDir"
} catch {
    Write-Summary -Status "fail" -Failure $_.Exception.Message
    Write-Error $_
    exit 1
}
