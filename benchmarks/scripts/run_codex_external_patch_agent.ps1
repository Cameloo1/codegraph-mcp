param(
    [switch]$ValidateOnly,
    [string]$CodexCommand = "",
    [string]$WorkspaceRoot = "",
    [int]$TimeoutSeconds = 1800,
    [string]$Model = "",
    [ValidateSet("auto", "always", "never")][string]$StationMode = "",
    [switch]$StationDryRun
)

$ErrorActionPreference = "Stop"

function Resolve-CodexCommand {
    param([string]$Requested)

    if ($Requested) {
        $resolved = Get-Command $Requested -ErrorAction SilentlyContinue
        if ($resolved) { return $resolved.Source }
        if (Test-Path -LiteralPath $Requested) { return (Resolve-Path -LiteralPath $Requested).Path }
        throw "codex_cli_not_found: $Requested"
    }

    # Prefer the .cmd shim on Windows so PowerShell execution policy does not
    # trip over codex.ps1.
    $cmd = Get-Command "codex.cmd" -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    $exe = Get-Command "codex.exe" -ErrorAction SilentlyContinue
    if ($exe) { return $exe.Source }

    $plain = Get-Command "codex" -ErrorAction SilentlyContinue
    if ($plain) { return $plain.Source }

    throw "codex_cli_not_found"
}

function Read-StdinText {
    $reader = [Console]::In
    return $reader.ReadToEnd()
}

function ConvertTo-JsonOrFail {
    param([string]$Text)
    if (-not $Text.Trim()) {
        throw "empty_benchmark_payload"
    }
    try {
        return $Text | ConvertFrom-Json
    } catch {
        throw "invalid_benchmark_payload_json: $($_.Exception.Message)"
    }
}

function Get-JsonField {
    param($Object, [string]$Name, [string]$Default = "")
    if ($null -ne $Object -and $Object.PSObject.Properties.Name -contains $Name) {
        $value = $Object.$Name
        if ($null -ne $value) { return [string]$value }
    }
    return $Default
}

function Resolve-Workspace {
    param($Payload, [string]$Requested)

    if ($Requested) {
        return (Resolve-Path -LiteralPath $Requested).Path
    }

    if ($env:CODEGRAPH_BENCH_AGENT_WORKDIR) {
        return (Resolve-Path -LiteralPath $env:CODEGRAPH_BENCH_AGENT_WORKDIR).Path
    }

    $task = $Payload.task
    foreach ($field in @("workspace", "workspace_path", "repo_path")) {
        $candidate = Get-JsonField $task $field
        if ($candidate -and (Test-Path -LiteralPath $candidate)) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    return (Get-Location).Path
}

function New-AgentPrompt {
    param($Payload)

    $task = $Payload.task
    $context = $Payload.context
    $taskText = Get-JsonField $task "task" "SWE-bench patch task"
    $taskId = Get-JsonField $task "task_id" (Get-JsonField $task "instance_id" "unknown_task")
    $payloadJson = $Payload | ConvertTo-Json -Depth 100

    return (@(
        "You are running as the fixed external patch agent for a local CodeGraph benchmark.",
        "",
        "Task id: $taskId",
        "Task:",
        $taskText,
        "",
        "Use only the provided task and context. Produce a patch for the current",
        "repository/workspace. At the end, ensure the workspace contains your final file",
        "edits. Do not include explanations in the final answer.",
        "",
        "The benchmark wrapper will collect git diff --no-ext-diff after you finish.",
        "If no patch is needed or you cannot produce one, say that plainly.",
        "",
        "Benchmark JSON payload:",
        '```json',
        $payloadJson,
        '```'
    ) -join [Environment]::NewLine)
}

function Write-LogFile {
    param([string]$Path, [string]$Text)
    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    Set-Content -LiteralPath $Path -Value $Text -Encoding UTF8
}

function Resolve-StationMode {
    param([string]$Requested)

    if ($Requested -and $Requested -ne "auto") {
        return $Requested
    }
    if ($env:CODEGRAPH_BENCH_AGENT_STATION -and $env:CODEGRAPH_BENCH_AGENT_STATION -ne "auto") {
        return $env:CODEGRAPH_BENCH_AGENT_STATION
    }
    if ($env:CI -and $env:CI.ToLowerInvariant() -eq "true") {
        return "never"
    }
    return "always"
}

function Write-AgentStatus {
    param(
        [string]$Path,
        [string]$State,
        [string]$TaskId,
        [string]$Workspace,
        [string]$LogRoot,
        [int]$Timeout,
        [int]$CodexPid = 0,
        [string]$Message = "",
        [int]$ContextBytes = 0,
        [int]$ToolCalls = 0,
        [string]$Mode = ""
    )

    $startedPath = Join-Path $LogRoot "agent_started_epoch.txt"
    if (-not (Test-Path -LiteralPath $startedPath)) {
        Set-Content -LiteralPath $startedPath -Value ([string][double](Get-Date -UFormat %s)) -Encoding ASCII
    }
    $started = [double](Get-Content -LiteralPath $startedPath -Raw)
    $data = [ordered]@{
        schema_version = "codegraph_external_agent_status_v1"
        state = $State
        task_id = $TaskId
        workspace = $Workspace
        log_root = $LogRoot
        timeout_seconds = $Timeout
        codex_pid = $(if ($CodexPid -gt 0) { $CodexPid } else { $null })
        message = $Message
        context_bytes = $ContextBytes
        tool_calls = $ToolCalls
        mode = $Mode
        started_at_epoch = $started
        updated_at = (Get-Date).ToString("o")
    }
    $json = $data | ConvertTo-Json -Depth 10
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.UTF8Encoding]::new($false))
}

function Start-AgentStation {
    param(
        [string]$Mode,
        [string]$LogRoot,
        [string]$TaskId,
        [string]$Workspace,
        [int]$Timeout,
        [string]$StatusPath,
        [switch]$DryRun
    )

    if ($Mode -eq "never") {
        return
    }
    $scriptPath = Join-Path (Get-Location).Path "benchmarks\scripts\show_external_agent_station.ps1"
    $stationArgs = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $scriptPath,
        "-LogRoot", $LogRoot,
        "-TaskId", $TaskId,
        "-Workspace", $Workspace,
        "-TimeoutSeconds", [string]$Timeout,
        "-StatusPath", $StatusPath
    )
    $launchText = "powershell.exe $(Join-ProcessArgs $stationArgs)"
    $launchPath = Join-Path $LogRoot "station_launch_command.txt"
    Set-Content -LiteralPath $launchPath -Value $launchText -Encoding UTF8
    if ($DryRun) {
        return
    }

    $wt = Get-Command "wt.exe" -ErrorAction SilentlyContinue
    if ($wt) {
        $safeTitle = "CodeGraph-Agent-$TaskId"
        $wtArgs = @("new-tab", "--title", $safeTitle, "powershell.exe") + $stationArgs
        Start-Process -FilePath $wt.Source -ArgumentList $wtArgs -WindowStyle Normal | Out-Null
        return
    }

    Start-Process -FilePath "powershell.exe" -ArgumentList $stationArgs -WindowStyle Normal | Out-Null
}

function Quote-ProcessArg {
    param([string]$Value)
    if ($Value -notmatch '[\s"]') {
        return $Value
    }
    return '"' + ($Value -replace '"', '\"') + '"'
}

function Join-ProcessArgs {
    param([string[]]$Values)
    return (($Values | ForEach-Object { Quote-ProcessArg $_ }) -join " ")
}

function Invoke-CodexPatchAgent {
    param(
        [string]$CodexPath,
        [string]$Workspace,
        [string]$Prompt,
        [string]$LogRoot,
        [int]$Timeout,
        [string]$ModelName,
        [string]$StatusPath,
        [string]$TaskId,
        [int]$ContextBytes = 0,
        [int]$ToolCalls = 0,
        [string]$Mode = ""
    )

    $promptPath = Join-Path $LogRoot "prompt.md"
    $stdoutPath = Join-Path $LogRoot "codex_stdout.jsonl"
    $stderrPath = Join-Path $LogRoot "codex_stderr.txt"
    $lastMessagePath = Join-Path $LogRoot "codex_last_message.txt"
    Write-LogFile $promptPath $Prompt

    $args = @(
        "exec",
        "--json",
        "--cd", $Workspace,
        "--sandbox", "workspace-write",
        "--output-last-message", $lastMessagePath
    )
    if ($ModelName) {
        $args += @("--model", $ModelName)
    }
    $args += "-"

    $commandLine = "type $(Quote-ProcessArg $promptPath) | $(Quote-ProcessArg $CodexPath) $(Join-ProcessArgs $args) > $(Quote-ProcessArg $stdoutPath) 2> $(Quote-ProcessArg $stderrPath)"
    Write-AgentStatus -Path $StatusPath -State "launching" -TaskId $TaskId -Workspace $Workspace -LogRoot $LogRoot -Timeout $Timeout -Message "launching Codex CLI" -ContextBytes $ContextBytes -ToolCalls $ToolCalls -Mode $Mode

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $env:ComSpec
    $psi.Arguments = Join-ProcessArgs @("/d", "/c", $commandLine)
    $psi.WorkingDirectory = $Workspace
    $psi.RedirectStandardInput = $false
    $psi.RedirectStandardOutput = $false
    $psi.RedirectStandardError = $false
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true

    $proc = [System.Diagnostics.Process]::new()
    $proc.StartInfo = $psi
    [void]$proc.Start()
    Write-AgentStatus -Path $StatusPath -State "running" -TaskId $TaskId -Workspace $Workspace -LogRoot $LogRoot -Timeout $Timeout -CodexPid $proc.Id -Message "Codex CLI running" -ContextBytes $ContextBytes -ToolCalls $ToolCalls -Mode $Mode

    if (-not $proc.WaitForExit($Timeout * 1000)) {
        Write-AgentStatus -Path $StatusPath -State "timeout" -TaskId $TaskId -Workspace $Workspace -LogRoot $LogRoot -Timeout $Timeout -CodexPid $proc.Id -Message "Codex CLI exceeded timeout" -ContextBytes $ContextBytes -ToolCalls $ToolCalls -Mode $Mode
        try { $proc.Kill($true) } catch { }
        throw "codex_cli_timeout"
    }

    if ($proc.ExitCode -ne 0) {
        Write-AgentStatus -Path $StatusPath -State "failed" -TaskId $TaskId -Workspace $Workspace -LogRoot $LogRoot -Timeout $Timeout -CodexPid $proc.Id -Message "Codex CLI exited $($proc.ExitCode)" -ContextBytes $ContextBytes -ToolCalls $ToolCalls -Mode $Mode
        [Console]::Error.WriteLine("codex_cli_exit_$($proc.ExitCode)")
        exit $proc.ExitCode
    }
    Write-AgentStatus -Path $StatusPath -State "collecting_diff" -TaskId $TaskId -Workspace $Workspace -LogRoot $LogRoot -Timeout $Timeout -CodexPid $proc.Id -Message "Codex finished; collecting git diff" -ContextBytes $ContextBytes -ToolCalls $ToolCalls -Mode $Mode
}

function Get-GitDiff {
    param([string]$Workspace)

    $resolvedWorkspace = (Resolve-Path -LiteralPath $Workspace).Path

    function Invoke-GitCapture {
        param([string[]]$GitArgs)

        $psi = [System.Diagnostics.ProcessStartInfo]::new()
        $psi.FileName = "git"
        $psi.Arguments = Join-ProcessArgs (@("-c", "safe.directory=$resolvedWorkspace", "-C", $resolvedWorkspace) + $GitArgs)
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $true

        $proc = [System.Diagnostics.Process]::new()
        $proc.StartInfo = $psi
        [void]$proc.Start()
        $stdout = $proc.StandardOutput.ReadToEnd()
        $stderr = $proc.StandardError.ReadToEnd()
        $proc.WaitForExit()
        return [pscustomobject]@{
            ExitCode = $proc.ExitCode
            Stdout = $stdout
            Stderr = $stderr
        }
    }

    $inside = Invoke-GitCapture -GitArgs @("rev-parse", "--is-inside-work-tree")
    if ($inside.ExitCode -ne 0) {
        [Console]::Error.WriteLine("git_workspace_invalid: $resolvedWorkspace")
        [Console]::Error.WriteLine($inside.Stderr)
        exit $inside.ExitCode
    }

    $diff = Invoke-GitCapture -GitArgs @("diff", "--no-ext-diff", "--binary", "--")
    if ($diff.ExitCode -ne 0) {
        [Console]::Error.WriteLine("git_diff_failed: $resolvedWorkspace")
        [Console]::Error.WriteLine($diff.Stderr)
        exit $diff.ExitCode
    }
    $text = $diff.Stdout
    if (-not $text.Trim().StartsWith("diff --git")) {
        if ($diff.Stderr) {
            [Console]::Error.WriteLine($diff.Stderr)
        }
        throw "no_patch_generated"
    }
    return $text
}

$statusPath = ""
$safeTaskId = "unknown_task"
$workspace = ""
$logRoot = ""

try {
    $codexPath = Resolve-CodexCommand -Requested $CodexCommand
    if ($ValidateOnly) {
        Write-Output "codex_external_patch_agent_ready: $codexPath"
        exit 0
    }

    $payloadText = Read-StdinText
    $payload = ConvertTo-JsonOrFail -Text $payloadText
    $workspace = Resolve-Workspace -Payload $payload -Requested $WorkspaceRoot
    $safeTaskId = (Get-JsonField $payload.task "task_id" (Get-JsonField $payload.task "instance_id" "unknown_task")) -replace '[^A-Za-z0-9_.-]', '_'
    $contextBytes = 0
    $toolCalls = 0
    $mode = ""
    if ($payload.PSObject.Properties.Name -contains "context") {
        if ($payload.context.PSObject.Properties.Name -contains "raw_context_bytes") { $contextBytes = [int]$payload.context.raw_context_bytes }
        if ($payload.context.PSObject.Properties.Name -contains "tool_calls") { $toolCalls = [int]$payload.context.tool_calls }
        if ($payload.context.PSObject.Properties.Name -contains "provider") { $mode = [string]$payload.context.provider }
    }
    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $logRoot = Join-Path (Join-Path (Get-Location).Path "benchmarks\workspaces\external_patch_agent_logs") "$safeTaskId`_$timestamp"
    New-Item -ItemType Directory -Force -Path $logRoot | Out-Null
    $statusPath = Join-Path $logRoot "agent_status.json"
    Write-AgentStatus -Path $statusPath -State "initialized" -TaskId $safeTaskId -Workspace $workspace -LogRoot $logRoot -Timeout $TimeoutSeconds -Message "payload parsed; station/logs initialized" -ContextBytes $contextBytes -ToolCalls $toolCalls -Mode $mode
    $resolvedStationMode = Resolve-StationMode -Requested $StationMode
    Start-AgentStation -Mode $resolvedStationMode -LogRoot $logRoot -TaskId $safeTaskId -Workspace $workspace -Timeout $TimeoutSeconds -StatusPath $statusPath -DryRun:$StationDryRun

    $prompt = New-AgentPrompt -Payload $payload
    Invoke-CodexPatchAgent -CodexPath $codexPath -Workspace $workspace -Prompt $prompt -LogRoot $logRoot -Timeout $TimeoutSeconds -ModelName $Model -StatusPath $statusPath -TaskId $safeTaskId -ContextBytes $contextBytes -ToolCalls $toolCalls -Mode $mode
    $patch = Get-GitDiff -Workspace $workspace
    $patchBytes = [System.Text.Encoding]::UTF8.GetByteCount($patch)
    Write-AgentStatus -Path $statusPath -State "completed" -TaskId $safeTaskId -Workspace $workspace -LogRoot $logRoot -Timeout $TimeoutSeconds -Message "patch collected ($patchBytes bytes)" -ContextBytes $contextBytes -ToolCalls $toolCalls -Mode $mode
    [Console]::Out.WriteLine($patch)
} catch {
    if ($statusPath) {
        Write-AgentStatus -Path $statusPath -State "failed" -TaskId $safeTaskId -Workspace $workspace -LogRoot $logRoot -Timeout $TimeoutSeconds -Message $_.Exception.Message
    }
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
