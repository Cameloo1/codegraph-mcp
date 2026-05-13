param(
    [switch] $KeepTemp
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $PSCommandPath
$repoRoot = (Resolve-Path (Join-Path $scriptDir "..")).Path
$runId = "$(Get-Date -Format 'yyyyMMdd_HHmmss')_$(([guid]::NewGuid().ToString('N')).Substring(0, 8))"
$logRoot = Join-Path $repoRoot "reports\smoke\fresh_clone"
$runLogDir = Join-Path $logRoot "windows_$runId"
$tempParent = Join-Path ([System.IO.Path]::GetTempPath()) "codegraph fresh clone $runId"
$cloneRoot = Join-Path $tempParent "repo under test"
$summaryPath = Join-Path $runLogDir "summary.json"

New-Item -ItemType Directory -Force -Path $runLogDir | Out-Null
New-Item -ItemType Directory -Force -Path $cloneRoot | Out-Null

$pathWithSpacesTested = $cloneRoot.Contains(" ")
$steps = @()

function Add-StepResult {
    param(
        [string] $Name,
        [int] $ExitCode,
        [double] $DurationMs,
        [string] $LogPath,
        [string] $Notes = $null
    )

    $script:steps += [pscustomobject]@{
        name = $Name
        exit_code = $ExitCode
        duration_ms = [math]::Round($DurationMs, 3)
        log = $LogPath
        notes = $Notes
    }
}

function ConvertTo-CmdArgument {
    param([string] $Value)

    '"' + ($Value -replace '"', '\"') + '"'
}

function Invoke-NativeCommandToLog {
    param(
        [string[]] $Command,
        [string] $LogPath
    )

    $exe = $Command[0]
    $args = @()
    if ($Command.Length -gt 1) {
        $args = $Command[1..($Command.Length - 1)]
    }

    $commandLine = "cd /d $(ConvertTo-CmdArgument $cloneRoot) && " +
        ((@($exe) + $args | ForEach-Object { ConvertTo-CmdArgument $_ }) -join " ") +
        " >> $(ConvertTo-CmdArgument $LogPath) 2>&1"

    & $env:ComSpec /d /s /c $commandLine
    return $LASTEXITCODE
}

function Write-Summary {
    param(
        [string] $Status,
        [string] $Failure = $null
    )

    [pscustomobject]@{
        schema_version = 1
        status = $Status
        failure = $Failure
        run_id = $runId
        generated_at = (Get-Date).ToString("o")
        platform = "windows"
        repo_root = $repoRoot
        temp_parent = $tempParent
        clone_root = $cloneRoot
        copy_mode = "filesystem_snapshot"
        path_with_spaces_tested = $pathWithSpacesTested
        logs_dir = $runLogDir
        steps = @($steps)
        requirements = @{
            cgc_required = $false
            autoresearch_required = $false
            external_benchmark_artifacts_required = $false
            network_required = "only normal Cargo dependency resolution"
        }
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $summaryPath -Encoding UTF8
}

function Invoke-SmokeStep {
    param(
        [string] $Name,
        [string[]] $Command
    )

    $logPath = Join-Path $runLogDir "$Name.log"
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    "repo: $cloneRoot" | Set-Content -LiteralPath $logPath -Encoding UTF8
    "command: $($Command -join ' ')" | Add-Content -LiteralPath $logPath -Encoding UTF8
    "" | Add-Content -LiteralPath $logPath -Encoding UTF8

    try {
        $exitCode = Invoke-NativeCommandToLog -Command $Command -LogPath $logPath
    } finally {
        $timer.Stop()
    }

    if ($null -eq $exitCode) {
        $exitCode = 0
    }

    Add-StepResult -Name $Name -ExitCode $exitCode -DurationMs $timer.Elapsed.TotalMilliseconds -LogPath $logPath
    if ($exitCode -ne 0) {
        throw "$Name failed with exit code $exitCode. See $logPath"
    }
}

function Invoke-CargoMetadataStep {
    $name = "cargo_metadata"
    $logPath = Join-Path $runLogDir "$name.log"
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    "repo: $cloneRoot" | Set-Content -LiteralPath $logPath -Encoding UTF8
    "command: cargo metadata --workspace" | Add-Content -LiteralPath $logPath -Encoding UTF8
    "" | Add-Content -LiteralPath $logPath -Encoding UTF8

    try {
        $exitCode = Invoke-NativeCommandToLog -Command @("cargo", "metadata", "--workspace") -LogPath $logPath
        $notes = $null
        if ($exitCode -ne 0) {
            $metadataOutput = Get-Content -LiteralPath $logPath -Raw
            if ($metadataOutput -match "unexpected argument.+--workspace") {
                $notes = "cargo metadata --workspace is unsupported by this Cargo; fallback used because metadata is workspace-scoped by default"
                "" | Add-Content -LiteralPath $logPath -Encoding UTF8
                "compatibility fallback: cargo metadata --format-version 1" | Add-Content -LiteralPath $logPath -Encoding UTF8
                $exitCode = Invoke-NativeCommandToLog -Command @("cargo", "metadata", "--format-version", "1") -LogPath $logPath
            }
        }
    } finally {
        $timer.Stop()
    }

    Add-StepResult -Name $name -ExitCode $exitCode -DurationMs $timer.Elapsed.TotalMilliseconds -LogPath $logPath -Notes $notes
    if ($exitCode -ne 0) {
        throw "$name failed with exit code $exitCode. See $logPath"
    }
}

try {
    $copyLog = Join-Path $runLogDir "copy.log"
    $excludeDirs = @(
        ".git",
        "target",
        ".codegraph",
        ".codegraph-index",
        ".codegraph-competitors",
        ".codegraph-bench-cache",
        ".codex-tools",
        ".tools",
        "node_modules",
        ".next",
        ".nuxt",
        "coverage"
    )
    $excludeFullDirs = @(
        $runLogDir,
        (Join-Path $repoRoot "reports\audit"),
        (Join-Path $repoRoot "reports\final\artifacts"),
        (Join-Path $repoRoot "reports\comparison\cgc_recovery"),
        (Join-Path $repoRoot "reports\smoke\index"),
        (Join-Path $repoRoot "reports\smoke\docker")
    )
    $excludeFiles = @(
        "*.db",
        "*.db-shm",
        "*.db-wal",
        "*.sqlite",
        "*.sqlite-shm",
        "*.sqlite-wal",
        "*.sqlite3",
        "*.sqlite3-shm",
        "*.sqlite3-wal",
        "*.log",
        "*.cgc-bundle",
        "*.cgc-bundle.tmp"
    )

    $robocopyArgs = @($repoRoot, $cloneRoot, "/E", "/XD") +
        $excludeDirs +
        $excludeFullDirs +
        @("/XF") +
        $excludeFiles +
        @("/R:2", "/W:1", "/NFL", "/NDL", "/NP", "/LOG:$copyLog")

    $copyTimer = [System.Diagnostics.Stopwatch]::StartNew()
    & robocopy @robocopyArgs | Out-Null
    $copyExit = $LASTEXITCODE
    $copyTimer.Stop()
    $normalizedCopyExit = if ($copyExit -le 7) { 0 } else { $copyExit }
    Add-StepResult -Name "copy_worktree" -ExitCode $normalizedCopyExit -DurationMs $copyTimer.Elapsed.TotalMilliseconds -LogPath $copyLog -Notes "robocopy raw exit code $copyExit"
    if ($copyExit -gt 7) {
        throw "filesystem snapshot failed with robocopy exit code $copyExit. See $copyLog"
    }

    Set-Content -LiteralPath (Join-Path $runLogDir "environment.txt") -Encoding UTF8 -Value @(
        "repo_root=$repoRoot",
        "clone_root=$cloneRoot",
        "path_with_spaces_tested=$pathWithSpacesTested",
        "copy_mode=filesystem_snapshot",
        "cargo=$((Get-Command cargo).Source)"
    )

    Invoke-CargoMetadataStep
    Invoke-SmokeStep -Name "cargo_build" -Command @("cargo", "build", "--workspace")
    Invoke-SmokeStep -Name "cargo_test" -Command @("cargo", "test", "--workspace")
    Invoke-SmokeStep -Name "codegraph_mcp_help" -Command @("cargo", "run", "--bin", "codegraph-mcp", "--", "--help")

    Write-Summary -Status "pass"
    Write-Host "fresh clone smoke passed"
    Write-Host "logs: $runLogDir"
    Write-Host "clone path: $cloneRoot"
} catch {
    Write-Summary -Status "fail" -Failure $_.Exception.Message
    Write-Error $_
    exit 1
} finally {
    if (-not $KeepTemp -and (Test-Path -LiteralPath $tempParent)) {
        Remove-Item -LiteralPath $tempParent -Recurse -Force
    }
}
