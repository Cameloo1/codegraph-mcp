$ErrorActionPreference = 'Continue'

$Root = (Get-Location).Path
$Base = 'reports/comparison/cgc_recovery'
$Logs = Join-Path $Base 'logs'
$Artifacts = Join-Path $Base 'artifacts'
$Tools = '.tools/cgc_recovery'
$Repo = if ($env:CGC_RECOVERY_AUTORESEARCH_REPO) { $env:CGC_RECOVERY_AUTORESEARCH_REPO } else { '..\autoresearch-codexlab' }
$CgcExe = if ($env:CGC_RECOVERY_CGC_EXE) { $env:CGC_RECOVERY_CGC_EXE } else { '.tools/cgc_recovery/venv312_compat/Scripts/cgc.exe' }
$TimeoutSec = if ($env:CGC_RECOVERY_TIMEOUT_SEC) { [int]$env:CGC_RECOVERY_TIMEOUT_SEC } else { 180 }

New-Item -ItemType Directory -Force -Path $Logs, $Artifacts, $Tools | Out-Null

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
    '"' + $Arg.Replace('"', '\"') + '"'
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

$repoPath = (Resolve-Path $Repo).Path
$cgcHome = if ($env:CGC_RECOVERY_AUTORESEARCH_HOME) { $env:CGC_RECOVERY_AUTORESEARCH_HOME } else { Join-Path $Tools 'home_autoresearch' }
$sharedHome = Join-Path $Tools 'home_shared'
New-Item -ItemType Directory -Force -Path $cgcHome, (Join-Path $cgcHome 'AppData\Local'), (Join-Path $cgcHome 'AppData\Roaming') | Out-Null
$sharedCache = Join-Path $sharedHome 'AppData\Local\tree-sitter-language-pack'
$targetCache = Join-Path $cgcHome 'AppData\Local\tree-sitter-language-pack'
if ((Test-Path $sharedCache) -and -not (Test-Path $targetCache)) {
    Copy-Item -Path $sharedCache -Destination (Split-Path -Parent $targetCache) -Recurse -Force
}

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
$env:LOCALAPPDATA = (Resolve-Path (Join-Path $cgcHome 'AppData\Local')).Path
$env:APPDATA = (Resolve-Path (Join-Path $cgcHome 'AppData\Roaming')).Path
$env:DEFAULT_DATABASE = 'kuzudb'
$env:CGC_RUNTIME_DB_TYPE = 'kuzudb'
$env:CGC_ALLOWED_ROOTS = $repoPath
$env:INDEX_SOURCE = 'true'
$env:IGNORE_HIDDEN_FILES = 'true'
$env:PYTHONUTF8 = '1'
$env:PYTHONIOENCODING = 'utf-8'
$env:NO_COLOR = '1'
$env:TERM = 'dumb'
$env:DEBUG_LOGS = 'true'
$env:DEBUG_LOG_PATH = Join-Path (Resolve-Path $cgcHome).Path 'cgc_debug.log'
$env:ENABLE_APP_LOGS = 'INFO'

$fileCount = (Get-ChildItem -Path $repoPath -Recurse -File -ErrorAction SilentlyContinue | Measure-Object).Count
$results = @()
$results += Run-Cgc -Name 'phase7_autoresearch_version' -ArgList @('--version') -Cwd $repoPath -TimeoutSec 30
$results += Run-Cgc -Name 'phase7_autoresearch_index_180s' -ArgList @('--database', 'kuzudb', 'index', $repoPath, '--force') -Cwd $repoPath -TimeoutSec $TimeoutSec
$indexResult = $results[-1]
if (-not $indexResult.timed_out -and $indexResult.exit_code -eq 0) {
    $results += Run-Cgc -Name 'phase7_autoresearch_stats' -ArgList @('--database', 'kuzudb', 'stats', $repoPath) -Cwd $repoPath -TimeoutSec 120
    $results += Run-Cgc -Name 'phase7_autoresearch_find_content' -ArgList @('--database', 'kuzudb', 'find', 'content', 'Autoresearch') -Cwd $repoPath -TimeoutSec 120
    $results += Run-Cgc -Name 'phase7_autoresearch_query_files' -ArgList @('--database', 'kuzudb', 'query', 'MATCH (f:File) RETURN count(f) AS files') -Cwd $repoPath -TimeoutSec 120
    $results += Run-Cgc -Name 'phase7_autoresearch_query_functions' -ArgList @('--database', 'kuzudb', 'query', 'MATCH (fn:Function) RETURN count(fn) AS functions') -Cwd $repoPath -TimeoutSec 120
}

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
$dbPath = Join-Path $cgcHome '.codegraphcontext\global\db\kuzudb'
$dbSize = if (Test-Path $dbPath) { (Get-Item $dbPath).Length } else { 0 }
$summary = [ordered]@{
    schema_version = 1
    generated_at = (Get-Date).ToString('o')
    cgc_executable = if (Test-Path $CgcExe) { (Resolve-Path $CgcExe).Path } else { $CgcExe }
    install_mode = if ($env:CGC_RECOVERY_INSTALL_MODE) { $env:CGC_RECOVERY_INSTALL_MODE } else { 'stock_reinstall_compat_dependency' }
    repo = $repoPath
    file_count = $fileCount
    timeout_sec = $TimeoutSec
    cgc_home = (Resolve-Path $cgcHome).Path
    db_path = $dbPath
    db_size_bytes = $dbSize
    commands = $results
    artifacts = $artifactRows
    completed_under_timeout = (-not $indexResult.timed_out -and $indexResult.exit_code -eq 0)
}
Write-Utf8 'reports/comparison/cgc_recovery/07_cgc_autoresearch_diagnostic.json' (($summary | ConvertTo-Json -Depth 12) + "`n")
Write-Output (($summary | ConvertTo-Json -Depth 8))
