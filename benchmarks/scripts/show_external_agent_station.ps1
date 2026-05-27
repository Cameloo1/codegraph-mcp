param(
    [Parameter(Mandatory=$true)][string]$LogRoot,
    [string]$TaskId = "",
    [string]$Workspace = "",
    [int]$TimeoutSeconds = 1800,
    [string]$StatusPath = "",
    [int]$RefreshMs = 750,
    [int]$TailLines = 14,
    [switch]$Once
)

$ErrorActionPreference = "SilentlyContinue"

function Use-Color {
    return -not $env:NO_COLOR
}

function Get-ConsoleWidth {
    try {
        $width = [Console]::WindowWidth
        if ($width -ge 60) { return $width }
    } catch { }
    return 120
}

function Write-ColorLine {
    param([string]$Text, [string]$Color = "Gray")
    if (Use-Color) {
        Write-Host $Text -ForegroundColor $Color
    } else {
        Write-Host $Text
    }
}

function Write-StatusBar {
    param([string]$Text, [string]$State)
    $width = Get-ConsoleWidth
    if ($Text.Length -gt $width) {
        $Text = $Text.Substring(0, $width)
    }
    $line = $Text.PadRight($width)
    if (-not (Use-Color)) {
        Write-Host $line
        return
    }
    $bg = switch ($State) {
        "completed" { "DarkGreen" }
        "running" { "DarkCyan" }
        "launching" { "DarkCyan" }
        "collecting_diff" { "DarkCyan" }
        "timeout" { "DarkMagenta" }
        "resource_limit" { "DarkMagenta" }
        "failed" { "DarkRed" }
        "blocked" { "DarkGray" }
        default { "DarkBlue" }
    }
    Write-Host $line -BackgroundColor $bg -ForegroundColor White
}

function State-Color {
    param([string]$State)
    switch ($State) {
        "completed" { return "Green" }
        "running" { return "Cyan" }
        "launching" { return "Cyan" }
        "collecting_diff" { return "Cyan" }
        "timeout" { return "Magenta" }
        "resource_limit" { return "Magenta" }
        "failed" { return "Red" }
        "blocked" { return "DarkGray" }
        default { return "Yellow" }
    }
}

function Read-JsonFile {
    param([string]$Path)
    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    try {
        return Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
    } catch {
        return $null
    }
}

function Read-TailText {
    param([string]$Path, [int]$Lines)
    if (-not (Test-Path -LiteralPath $Path)) {
        return @("<waiting: $(Split-Path -Leaf $Path)>")
    }
    try {
        $tail = Get-Content -LiteralPath $Path -Tail $Lines -Encoding UTF8
        if ($null -eq $tail -or $tail.Count -eq 0) {
            return @("<empty>")
        }
        return $tail
    } catch {
        return @("<unable to read $Path>")
    }
}

function Get-StatusValue {
    param($Status, [string]$Name, $Default = "")
    if ($Status -and $Status.PSObject.Properties.Name -contains $Name) {
        $value = $Status.$Name
        if ($null -ne $value -and [string]$value -ne "") {
            return $value
        }
    }
    return $Default
}

function Format-BytesMiB {
    param($Bytes)
    if ($null -eq $Bytes -or [string]$Bytes -eq "") {
        return "n/a"
    }
    try {
        return ("{0:N1} MiB" -f ([double]$Bytes / 1MB))
    } catch {
        return "n/a"
    }
}

function Format-MiBValue {
    param($MiB)
    if ($null -eq $MiB -or [string]$MiB -eq "") {
        return "n/a"
    }
    try {
        return ("{0:N1} MiB" -f [double]$MiB)
    } catch {
        return "$MiB MiB"
    }
}

function Get-SystemAvailableMiB {
    try {
        $os = Get-CimInstance Win32_OperatingSystem
        return [math]::Round([double]$os.FreePhysicalMemory / 1024, 1)
    } catch {
        return $null
    }
}

function Get-ProcessTreeWorkingSetBytes {
    param($RootPid)
    if (-not $RootPid) {
        return $null
    }
    try {
        $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, WorkingSetSize
        $children = @{}
        foreach ($proc in $all) {
            $parent = [int]$proc.ParentProcessId
            if (-not $children.ContainsKey($parent)) {
                $children[$parent] = New-Object System.Collections.Generic.List[int]
            }
            $children[$parent].Add([int]$proc.ProcessId)
        }
        $seen = New-Object 'System.Collections.Generic.HashSet[int]'
        $stack = New-Object 'System.Collections.Generic.Stack[int]'
        $stack.Push([int]$RootPid)
        while ($stack.Count -gt 0) {
            $pid = $stack.Pop()
            if ($seen.Add($pid) -and $children.ContainsKey($pid)) {
                foreach ($child in $children[$pid]) {
                    $stack.Push($child)
                }
            }
        }
        $total = 0L
        foreach ($proc in $all) {
            if ($seen.Contains([int]$proc.ProcessId) -and $proc.WorkingSetSize) {
                $total += [int64]$proc.WorkingSetSize
            }
        }
        return $total
    } catch {
        return $null
    }
}

function Write-SectionHeader {
    param([string]$Title)
    Write-ColorLine ""
    Write-ColorLine $Title "Yellow"
    Write-ColorLine ("-" * [math]::Min(76, (Get-ConsoleWidth))) "DarkGray"
}

function Get-PhaseGlyph {
    param([string]$PhaseName, [string]$State, [string]$Phase)
    if ($State -eq "failed" -or $State -eq "timeout" -or $State -eq "resource_limit") {
        if ($PhaseName -eq $Phase) { return "[fail]" }
    }
    if ($State -eq "completed") { return "[ok]" }
    if ($PhaseName -eq $Phase) { return "[run]" }
    if ($PhaseName -eq "context" -and $Phase -in @("agent", "patch", "eval", "completed")) { return "[ok]" }
    if ($PhaseName -eq "agent" -and $Phase -in @("patch", "eval", "completed")) { return "[ok]" }
    if ($PhaseName -eq "patch" -and $Phase -in @("eval", "completed")) { return "[ok]" }
    return "[wait]"
}

function Write-PhaseTimeline {
    param($Status, [string]$State)
    $phase = [string](Get-StatusValue $Status "phase" "waiting")
    $items = @("context", "agent", "patch", "eval", "clean-source-gate")
    $parts = foreach ($item in $items) {
        "$(Get-PhaseGlyph $item $State $phase) $item"
    }
    Write-ColorLine ($parts -join "  ->  ") "Gray"
}

function Render-MachineMetrics {
    param($Status, $RootProcessId, [string]$ProcRamText, [string]$AvailableText)
    $guard = Get-StatusValue $Status "resource_guard" $null
    Write-ColorLine "Process tree RAM: $ProcRamText"
    Write-ColorLine "System free RAM:  $AvailableText"
    if ($guard) {
        $maxRss = Get-StatusValue $guard "max_process_tree_rss_mib" ""
        $maxObserved = Get-StatusValue $guard "max_observed_process_tree_rss_mib" ""
        $minAvail = Get-StatusValue $guard "min_system_available_mib" ""
        $minObserved = Get-StatusValue $guard "min_observed_system_available_mib" ""
        $violation = Get-StatusValue $guard "violation_reason" "none"
        $killed = Get-StatusValue $guard "killed_process_ids" @()
        Write-ColorLine "Guard RSS limit:  $(Format-MiBValue $maxRss) | max observed $(Format-MiBValue $maxObserved)"
        Write-ColorLine "Guard free limit: $(Format-MiBValue $minAvail) | min observed $(Format-MiBValue $minObserved)"
        Write-ColorLine "Guard violation:  $violation"
        if ($killed -and $killed.Count -gt 0) {
            Write-ColorLine "Killed PIDs:      $($killed -join ', ')" "Magenta"
        }
    } else {
        Write-ColorLine "Resource guard:   n/a"
    }
    Write-ColorLine "Codex PID:        $(if ($RootProcessId) { $RootProcessId } else { 'n/a' })"
}

function Render-Station {
    if (-not $StatusPath) {
        $script:StatusPath = Join-Path $LogRoot "agent_status.json"
    }
    $status = Read-JsonFile $StatusPath
    $state = [string](Get-StatusValue $status "state" "waiting")
    $phase = [string](Get-StatusValue $status "phase" "waiting")
    $pid = Get-StatusValue $status "codex_pid" $null
    $startedAt = Get-StatusValue $status "started_at_epoch" ([double](Get-Date -UFormat %s))
    try { $elapsed = [int]([double](Get-Date -UFormat %s) - [double]$startedAt) } catch { $elapsed = 0 }
    $procRam = Format-BytesMiB (Get-ProcessTreeWorkingSetBytes $pid)
    $available = Get-SystemAvailableMiB
    $availableText = if ($null -eq $available) { "n/a" } else { "$available MiB" }
    $mode = Get-StatusValue $status "mode" "n/a"
    $contextBytes = Get-StatusValue $status "context_bytes" 0
    $toolCalls = Get-StatusValue $status "tool_calls" 0
    $message = Get-StatusValue $status "message" "waiting for status file"
    $currentCommand = Get-StatusValue $status "current_command" "n/a"
    $stationLaunchStatus = Get-StatusValue $status "station_launch_status" "n/a"
    $stationLaunchError = Get-StatusValue $status "station_launch_error" ""
    $blockedReason = Get-StatusValue $status "blocked_reason" ""
    $patchBytes = Get-StatusValue $status "patch_bytes" "n/a"
    $cleanSourcePatch = Get-StatusValue $status "clean_source_patch" "n/a"

    if (-not $Once) {
        Clear-Host
    }

    Write-ColorLine "CodeGraph External Agent Station" "Cyan"
    Write-ColorLine ("=" * [math]::Min(76, (Get-ConsoleWidth))) "DarkCyan"
    Write-ColorLine "Task:       $TaskId"
    Write-ColorLine "Mode:       $mode"
    Write-ColorLine "State:      $state" (State-Color $state)
    Write-ColorLine "Phase:      $phase"
    Write-ColorLine "Elapsed:    ${elapsed}s / ${TimeoutSeconds}s"
    Write-ColorLine "Workspace:  $Workspace"
    Write-ColorLine "Log root:   $LogRoot"
    Write-ColorLine "Status:     $StatusPath"

    Write-SectionHeader "Phase Timeline"
    Write-PhaseTimeline $status $state

    Write-SectionHeader "Machine And Guard Metrics"
    Render-MachineMetrics $status $pid $procRam $availableText

    Write-SectionHeader "Current Event"
    Write-ColorLine "Message:          $message" (State-Color $state)
    Write-ColorLine "Current command:  $currentCommand"
    Write-ColorLine "Context/tools:    $contextBytes bytes | $toolCalls tool calls"
    Write-ColorLine "Patch gate:       patch_bytes=$patchBytes | clean_source_patch=$cleanSourcePatch"
    Write-ColorLine "Station launch:   $stationLaunchStatus"
    if ($stationLaunchError) {
        Write-ColorLine "Station error:    $stationLaunchError" "Red"
    }
    if ($blockedReason) {
        Write-ColorLine "Blocked reason:   $blockedReason" "DarkGray"
    }

    Write-SectionHeader "Last Codex Message"
    Read-TailText (Join-Path $LogRoot "codex_last_message.txt") 8 | ForEach-Object { Write-Host $_ }

    Write-SectionHeader "STDERR Tail"
    Read-TailText (Join-Path $LogRoot "codex_stderr.txt") $TailLines | ForEach-Object { Write-Host $_ }

    Write-SectionHeader "STDOUT JSONL Tail"
    Read-TailText (Join-Path $LogRoot "codex_stdout.jsonl") $TailLines | ForEach-Object { Write-Host $_ }

    $bar = " $state | phase $phase | elapsed ${elapsed}s/${TimeoutSeconds}s | proc RAM $procRam | free $availableText | station $stationLaunchStatus "
    Write-ColorLine ""
    Write-StatusBar $bar $state
}

if (-not $StatusPath) {
    $StatusPath = Join-Path $LogRoot "agent_status.json"
}

do {
    Render-Station
    if ($Once) { break }
    $status = Read-JsonFile $StatusPath
    $state = if ($status -and $status.state) { [string]$status.state } else { "" }
    if ($state -in @("completed", "failed", "timeout", "resource_limit", "blocked")) {
        Start-Sleep -Seconds 4
        break
    }
    Start-Sleep -Milliseconds $RefreshMs
} while ($true)
