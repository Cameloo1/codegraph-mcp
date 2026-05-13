$ErrorActionPreference = 'Continue'

$Root = (Get-Location).Path
$Base = 'reports/comparison/cgc_recovery'
$Logs = Join-Path $Base 'logs'
$Raw = Join-Path $Base 'raw'
$Artifacts = Join-Path $Base 'artifacts'
$Tools = '.tools/cgc_recovery'

New-Item -ItemType Directory -Force -Path $Base, $Logs, $Raw, $Artifacts, $Tools | Out-Null

function Write-Utf8 {
    param([string]$Path, [string]$Text)
    $full = Join-Path $Root $Path
    $dir = Split-Path -Parent $full
    if ($dir -and -not (Test-Path $dir)) {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
    }
    [System.IO.File]::WriteAllText($full, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Log-Result {
    param(
        [string]$Name,
        [string]$Command,
        [string]$Cwd,
        $ExitCode,
        [string]$Stdout,
        [string]$Stderr,
        [datetime]$Started,
        [datetime]$Ended,
        [bool]$TimedOut = $false
    )
    $stdoutPath = Join-Path $Logs "$Name.stdout.txt"
    $stderrPath = Join-Path $Logs "$Name.stderr.txt"
    $metaPath = Join-Path $Logs "$Name.meta.json"
    Write-Utf8 $stdoutPath $Stdout
    Write-Utf8 $stderrPath $Stderr
    $obj = [ordered]@{
        name = $Name
        command = $Command
        cwd = $Cwd
        exit_code = $ExitCode
        timed_out = $TimedOut
        started_at = $Started.ToString('o')
        ended_at = $Ended.ToString('o')
        duration_ms = [int](($Ended - $Started).TotalMilliseconds)
        stdout = $stdoutPath
        stderr = $stderrPath
    }
    Write-Utf8 $metaPath (($obj | ConvertTo-Json -Depth 6) + "`n")
    [pscustomobject]$obj
}

function Run-Cmd {
    param(
        [string]$Name,
        [string]$Exe,
        [string[]]$ArgList = @(),
        [string]$Cwd = $Root,
        [int]$TimeoutSec = 60
    )
    $started = Get-Date
    $quotedArgs = ($ArgList | ForEach-Object {
        $escaped = $_.Replace('"', '\"')
        if ($escaped -match '\s') { '"' + $escaped + '"' } else { $escaped }
    }) -join ' '
    $cmd = ($Exe + ' ' + $quotedArgs).Trim()
    $resolved = Get-Command $Exe -ErrorAction SilentlyContinue
    if (-not $resolved -and -not (Test-Path $Exe)) {
        return Log-Result $Name $cmd $Cwd 127 '' "executable not found: $Exe" $started (Get-Date)
    }

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = if (Test-Path $Exe) { (Resolve-Path $Exe).Path } else { $Exe }
    $psi.Arguments = $quotedArgs
    $psi.WorkingDirectory = $Cwd
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $psi
    try {
        [void]$process.Start()
    } catch {
        return Log-Result $Name $cmd $Cwd 126 '' $_.Exception.Message $started (Get-Date)
    }

    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $timedOut = -not $process.WaitForExit($TimeoutSec * 1000)
    if ($timedOut) {
        try { $process.Kill($true) } catch { try { $process.Kill() } catch {} }
    }
    [void]$stdoutTask.Wait(5000)
    [void]$stderrTask.Wait(5000)
    $exitCode = if ($timedOut) { $null } else { $process.ExitCode }
    Log-Result $Name $cmd $Cwd $exitCode $stdoutTask.Result $stderrTask.Result $started (Get-Date) $timedOut
}

$results = @()
$results += Run-Cmd -Name 'phase0_rustc_version' -Exe 'rustc' -ArgList @('--version')
$results += Run-Cmd -Name 'phase0_cargo_version' -Exe 'cargo' -ArgList @('--version')
$results += Run-Cmd -Name 'phase0_where_python' -Exe 'where.exe' -ArgList @('python')
$results += Run-Cmd -Name 'phase0_python_version' -Exe 'python' -ArgList @('--version')
$results += Run-Cmd -Name 'phase0_python_pip_version' -Exe 'python' -ArgList @('-m', 'pip', '--version')
$results += Run-Cmd -Name 'phase0_where_python3' -Exe 'where.exe' -ArgList @('python3')
$results += Run-Cmd -Name 'phase0_python3_version' -Exe 'python3' -ArgList @('--version')
$results += Run-Cmd -Name 'phase0_py_launcher' -Exe 'py' -ArgList @('-0p')
$results += Run-Cmd -Name 'phase0_where_cgc' -Exe 'where.exe' -ArgList @('cgc')
$results += Run-Cmd -Name 'phase0_where_codegraphcontext' -Exe 'where.exe' -ArgList @('codegraphcontext')
$gitResult = Run-Cmd -Name 'phase0_git_rev_parse_head' -Exe 'git' -ArgList @('rev-parse', 'HEAD')
$results += $gitResult

$oldCgc = 'target/cgc-competitor/.venv312-pypi/Scripts/cgc.exe'
$oldPython = 'target/cgc-competitor/.venv312-pypi/Scripts/python.exe'
if (Test-Path $oldCgc) {
    $results += Run-Cmd -Name 'phase1_existing_cgc_version' -Exe $oldCgc -ArgList @('--version')
    $results += Run-Cmd -Name 'phase1_existing_cgc_help' -Exe $oldCgc -ArgList @('--help')
    $results += Run-Cmd -Name 'phase1_existing_cgc_help_subcommand' -Exe $oldCgc -ArgList @('help')
}
if (Test-Path $oldPython) {
    $results += Run-Cmd -Name 'phase1_existing_venv_python_version' -Exe $oldPython -ArgList @('--version')
    $results += Run-Cmd -Name 'phase1_existing_venv_pip_show_cgc' -Exe $oldPython -ArgList @('-m', 'pip', 'show', 'cgc')
    $results += Run-Cmd -Name 'phase1_existing_venv_pip_freeze' -Exe $oldPython -ArgList @('-m', 'pip', 'freeze')
}

$candidates = @(
    'target/cgc-competitor/.venv312-pypi/Scripts/cgc.exe',
    'target/cgc-competitor/.venv312-pypi/Scripts/codegraphcontext.exe',
    '.tools/cgc_recovery/venv/Scripts/cgc.exe',
    '.tools/cgc_recovery/venv/Scripts/codegraphcontext.exe'
) | ForEach-Object { [ordered]@{ path = $_; exists = (Test-Path $_) } }

function Read-LogTrim {
    param([string]$Path)
    $full = Join-Path $Root $Path
    if (Test-Path $full) {
        $content = Get-Content $full -Raw
        if ($null -eq $content) { return '' }
        return $content.Trim()
    }
    ''
}

$status = [ordered]@{
    schema_version = 1
    generated_at = (Get-Date).ToString('o')
    source_of_truth = 'MVP.md'
    os = [System.Environment]::OSVersion.VersionString
    shell = 'PowerShell'
    powershell = $PSVersionTable.PSVersion.ToString()
    working_directory = $Root
    codegraph_git_commit = Read-LogTrim 'reports/comparison/cgc_recovery/logs/phase0_git_rev_parse_head.stdout.txt'
    rust = Read-LogTrim 'reports/comparison/cgc_recovery/logs/phase0_rustc_version.stdout.txt'
    cargo = Read-LogTrim 'reports/comparison/cgc_recovery/logs/phase0_cargo_version.stdout.txt'
    python_versions = [ordered]@{
        python = Read-LogTrim 'reports/comparison/cgc_recovery/logs/phase0_python_version.stdout.txt'
        python3 = Read-LogTrim 'reports/comparison/cgc_recovery/logs/phase0_python3_version.stdout.txt'
        py_launcher_error = Read-LogTrim 'reports/comparison/cgc_recovery/logs/phase0_py_launcher.stderr.txt'
    }
    pip_versions = [ordered]@{
        python_pip = Read-LogTrim 'reports/comparison/cgc_recovery/logs/phase0_python_pip_version.stdout.txt'
    }
    cgc_executable_path = Read-LogTrim 'reports/comparison/cgc_recovery/logs/phase0_where_cgc.stdout.txt'
    codegraphcontext_executable_path = Read-LogTrim 'reports/comparison/cgc_recovery/logs/phase0_where_codegraphcontext.stdout.txt'
    env_CGC_COMPETITOR_BIN = $env:CGC_COMPETITOR_BIN
    benchmark_timeout_ms = 60000
    current_cgc_candidates = $candidates
    command_logs = $results
}

Write-Utf8 'reports/comparison/cgc_recovery/cgc_recovery_status.json' (($status | ConvertTo-Json -Depth 12) + "`n")

$candidateLines = ($candidates | ForEach-Object { "- ``$($_.path)``: exists=$($_.exists)" }) -join "`n"
$md = @"
# CGC Recovery Status

Source of truth: ``MVP.md``.

## Environment

- OS: $($status.os)
- Shell: $($status.shell) $($status.powershell)
- Working directory: ``$Root``
- Rust: $($status.rust)
- Cargo: $($status.cargo)
- Python: $($status.python_versions.python)
- Pip: $($status.pip_versions.python_pip)
- CGC on PATH: ``$($status.cgc_executable_path)``
- CodeGraphContext on PATH: ``$($status.codegraphcontext_executable_path)``
- ``CGC_COMPETITOR_BIN``: ``$($status.env_CGC_COMPETITOR_BIN)``

## Candidate Executables

$candidateLines

## Raw Logs

Raw command logs are under ``reports/comparison/cgc_recovery/logs/``.
"@
Write-Utf8 'reports/comparison/cgc_recovery/CGC_RECOVERY_STATUS.md' $md

Write-Output (($status | ConvertTo-Json -Depth 8))
