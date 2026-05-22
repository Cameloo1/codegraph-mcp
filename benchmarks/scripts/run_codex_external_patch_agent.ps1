param(
    [switch]$ValidateOnly,
    [string]$CodexCommand = "",
    [string]$WorkspaceRoot = "",
    [int]$TimeoutSeconds = 1800,
    [string]$Model = ""
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
        [string]$ModelName
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

    if (-not $proc.WaitForExit($Timeout * 1000)) {
        try { $proc.Kill($true) } catch { }
        throw "codex_cli_timeout"
    }

    if ($proc.ExitCode -ne 0) {
        throw "codex_cli_exit_$($proc.ExitCode)"
    }
}

function Get-GitDiff {
    param([string]$Workspace)

    $diff = & git -C $Workspace diff --no-ext-diff --binary -- 2>$null
    if ($LASTEXITCODE -ne 0) {
        throw "git_diff_failed"
    }
    $text = ($diff -join [Environment]::NewLine)
    if (-not $text.Trim().StartsWith("diff --git")) {
        throw "no_patch_generated"
    }
    return $text
}

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
    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $logRoot = Join-Path (Join-Path (Get-Location).Path "benchmarks\workspaces\external_patch_agent_logs") "$safeTaskId`_$timestamp"
    New-Item -ItemType Directory -Force -Path $logRoot | Out-Null

    $prompt = New-AgentPrompt -Payload $payload
    Invoke-CodexPatchAgent -CodexPath $codexPath -Workspace $workspace -Prompt $prompt -LogRoot $logRoot -Timeout $TimeoutSeconds -ModelName $Model
    $patch = Get-GitDiff -Workspace $workspace
    [Console]::Out.WriteLine($patch)
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
