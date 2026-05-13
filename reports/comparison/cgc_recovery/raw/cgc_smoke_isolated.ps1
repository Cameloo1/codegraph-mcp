$ErrorActionPreference = 'Continue'

$Root = (Get-Location).Path
$Base = 'reports/comparison/cgc_recovery'
$Logs = Join-Path $Base 'logs'
$Artifacts = Join-Path $Base 'artifacts'
$Tools = '.tools/cgc_recovery'
$SmokeRepo = Join-Path $Artifacts 'smoke_repo'
$CgcExe = if ($env:CGC_RECOVERY_CGC_EXE) { $env:CGC_RECOVERY_CGC_EXE } else { 'target/cgc-competitor/.venv312-pypi/Scripts/cgc.exe' }

New-Item -ItemType Directory -Force -Path $Logs, $Artifacts, $Tools, $SmokeRepo | Out-Null

function Write-Utf8 {
    param([string]$Path, [string]$Text)
    $full = Join-Path $Root $Path
    $dir = Split-Path -Parent $full
    if ($dir -and -not (Test-Path $dir)) {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
    }
    [System.IO.File]::WriteAllText($full, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Quote-Arg {
    param([string]$Arg)
    $escaped = $Arg.Replace('"', '\"')
    '"' + $escaped + '"'
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

function Run-Cgc {
    param(
        [string]$Name,
        [string[]]$ArgList,
        [string]$Cwd,
        [int]$TimeoutSec = 60
    )
    $started = Get-Date
    if (-not (Test-Path $CgcExe)) {
        return Log-Result $Name $CgcExe $Cwd 127 '' "executable not found: $CgcExe" $started (Get-Date)
    }
    $cgcPath = (Resolve-Path $CgcExe).Path
    $quotedArgs = ($ArgList | ForEach-Object { Quote-Arg $_ }) -join ' '
    $cmd = "$cgcPath $quotedArgs"

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $cgcPath
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

$smokeSource = @'
export function cgc_smoke_target(value: number): number {
  return value + 1;
}

export function cgc_smoke_caller(input: number): number {
  return cgc_smoke_target(input);
}
'@
Write-Utf8 (Join-Path $SmokeRepo 'smoke.ts') $smokeSource
$smokePython = @'
def cgc_smoke_py_target(value: int) -> int:
    return value + 1


def cgc_smoke_py_caller(input_value: int) -> int:
    return cgc_smoke_py_target(input_value)
'@
Write-Utf8 (Join-Path $SmokeRepo 'smoke.py') $smokePython

$runStamp = Get-Date -Format 'yyyyMMdd_HHmmss_fff'
$cgcHome = if ($env:CGC_RECOVERY_HOME) { $env:CGC_RECOVERY_HOME } else { Join-Path $Tools 'home_shared' }
$appData = Join-Path $cgcHome 'AppData'
New-Item -ItemType Directory -Force -Path $cgcHome, (Join-Path $appData 'Local'), (Join-Path $appData 'Roaming') | Out-Null

$oldHomeEnv = $env:HOME
$oldUserProfile = $env:USERPROFILE
$oldLocalAppData = $env:LOCALAPPDATA
$oldAppData = $env:APPDATA
$oldDefaultDb = $env:DEFAULT_DATABASE
$oldRuntimeDb = $env:CGC_RUNTIME_DB_TYPE
$oldAllowedRoots = $env:CGC_ALLOWED_ROOTS
$oldIndexSource = $env:INDEX_SOURCE
$oldIgnoreHidden = $env:IGNORE_HIDDEN_FILES
$oldPythonUtf8 = $env:PYTHONUTF8
$oldPythonIoEncoding = $env:PYTHONIOENCODING
$oldNoColor = $env:NO_COLOR
$oldTerm = $env:TERM
$oldDebugLogs = $env:DEBUG_LOGS
$oldDebugLogPath = $env:DEBUG_LOG_PATH
$oldEnableAppLogs = $env:ENABLE_APP_LOGS

$env:HOME = (Resolve-Path $cgcHome).Path
$env:USERPROFILE = (Resolve-Path $cgcHome).Path
$env:LOCALAPPDATA = (Resolve-Path (Join-Path $appData 'Local')).Path
$env:APPDATA = (Resolve-Path (Join-Path $appData 'Roaming')).Path
$env:DEFAULT_DATABASE = 'kuzudb'
$env:CGC_RUNTIME_DB_TYPE = 'kuzudb'
$env:CGC_ALLOWED_ROOTS = (Resolve-Path $SmokeRepo).Path
$env:INDEX_SOURCE = 'true'
$env:IGNORE_HIDDEN_FILES = 'false'
$env:PYTHONUTF8 = '1'
$env:PYTHONIOENCODING = 'utf-8'
$env:NO_COLOR = '1'
$env:TERM = 'dumb'
$env:DEBUG_LOGS = 'true'
$env:DEBUG_LOG_PATH = Join-Path (Resolve-Path $cgcHome).Path 'cgc_debug.log'
$env:ENABLE_APP_LOGS = 'DEBUG'

$results = @()
$repoPath = (Resolve-Path $SmokeRepo).Path
$results += Run-Cgc -Name 'phase2_smoke_version_isolated_home' -ArgList @('--version') -Cwd $repoPath -TimeoutSec 30
$results += Run-Cgc -Name 'phase2_smoke_index_isolated_home' -ArgList @('--database', 'kuzudb', 'index', $repoPath, '--force') -Cwd $repoPath -TimeoutSec 120
$results += Run-Cgc -Name 'phase2_smoke_stats_isolated_home' -ArgList @('--database', 'kuzudb', 'stats', $repoPath) -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_find_name_target_isolated_home' -ArgList @('--database', 'kuzudb', 'find', 'name', 'cgc_smoke_target') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_find_name_caller_isolated_home' -ArgList @('--database', 'kuzudb', 'find', 'name', 'cgc_smoke_caller') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_find_content_isolated_home' -ArgList @('--database', 'kuzudb', 'find', 'content', 'cgc_smoke_target') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_analyze_calls_isolated_home' -ArgList @('--database', 'kuzudb', 'analyze', 'calls', 'cgc_smoke_caller') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_analyze_callers_isolated_home' -ArgList @('--database', 'kuzudb', 'analyze', 'callers', 'cgc_smoke_target') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_find_name_py_target_isolated_home' -ArgList @('--database', 'kuzudb', 'find', 'name', 'cgc_smoke_py_target') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_find_name_py_caller_isolated_home' -ArgList @('--database', 'kuzudb', 'find', 'name', 'cgc_smoke_py_caller') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_find_content_py_isolated_home' -ArgList @('--database', 'kuzudb', 'find', 'content', 'cgc_smoke_py_target') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_analyze_calls_py_isolated_home' -ArgList @('--database', 'kuzudb', 'analyze', 'calls', 'cgc_smoke_py_caller') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_analyze_callers_py_isolated_home' -ArgList @('--database', 'kuzudb', 'analyze', 'callers', 'cgc_smoke_py_target') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_query_node_count_isolated_home' -ArgList @('--database', 'kuzudb', 'query', 'MATCH (n) RETURN count(n) AS c') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_query_files_isolated_home' -ArgList @('--database', 'kuzudb', 'query', 'MATCH (f:File) RETURN f.path AS path LIMIT 10') -Cwd $repoPath -TimeoutSec 60
$results += Run-Cgc -Name 'phase2_smoke_query_functions_isolated_home' -ArgList @('--database', 'kuzudb', 'query', 'MATCH (fn:Function) RETURN fn.name AS name, fn.path AS path LIMIT 10') -Cwd $repoPath -TimeoutSec 60

$env:HOME = $oldHomeEnv
$env:USERPROFILE = $oldUserProfile
$env:LOCALAPPDATA = $oldLocalAppData
$env:APPDATA = $oldAppData
$env:DEFAULT_DATABASE = $oldDefaultDb
$env:CGC_RUNTIME_DB_TYPE = $oldRuntimeDb
$env:CGC_ALLOWED_ROOTS = $oldAllowedRoots
$env:INDEX_SOURCE = $oldIndexSource
$env:IGNORE_HIDDEN_FILES = $oldIgnoreHidden
$env:PYTHONUTF8 = $oldPythonUtf8
$env:PYTHONIOENCODING = $oldPythonIoEncoding
$env:NO_COLOR = $oldNoColor
$env:TERM = $oldTerm
$env:DEBUG_LOGS = $oldDebugLogs
$env:DEBUG_LOG_PATH = $oldDebugLogPath
$env:ENABLE_APP_LOGS = $oldEnableAppLogs

$artifactRows = Get-ChildItem -Path $cgcHome -Recurse -File -ErrorAction SilentlyContinue |
    Select-Object FullName,Length,LastWriteTime
$summary = [ordered]@{
    schema_version = 1
    generated_at = (Get-Date).ToString('o')
    cgc_executable = if (Test-Path $CgcExe) { (Resolve-Path $CgcExe).Path } else { $CgcExe }
    install_mode = if ($env:CGC_RECOVERY_INSTALL_MODE) { $env:CGC_RECOVERY_INSTALL_MODE } else { 'stock_existing_isolated_home' }
    smoke_repo = $repoPath
    cgc_home = (Resolve-Path $cgcHome).Path
    commands = $results
    artifacts = $artifactRows
    smoke_passed = $false
}
$targetText = ''
$targetStdout = Join-Path $Logs 'phase2_smoke_find_name_py_target_isolated_home.stdout.txt'
$targetStderr = Join-Path $Logs 'phase2_smoke_find_name_py_target_isolated_home.stderr.txt'
if (Test-Path $targetStdout) { $targetText += "`n" + (Get-Content $targetStdout -Raw) }
if (Test-Path $targetStderr) { $targetText += "`n" + (Get-Content $targetStderr -Raw) }
$summary.smoke_passed = (($results | Where-Object { $_.name -eq 'phase2_smoke_index_isolated_home' }).exit_code -eq 0) -and ($targetText -match 'cgc_smoke_py_target') -and ($targetText -notmatch 'No code elements found')
Write-Utf8 'reports/comparison/cgc_recovery/02_cgc_smoke_test.json' (($summary | ConvertTo-Json -Depth 12) + "`n")
Write-Output (($summary | ConvertTo-Json -Depth 8))
