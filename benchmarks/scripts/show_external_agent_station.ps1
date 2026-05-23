param(
    [Parameter(Mandatory=$true)][string]$LogRoot,
    [string]$TaskId = "",
    [string]$Workspace = "",
    [int]$TimeoutSeconds = 1800,
    [string]$StatusPath = "",
    [int]$RefreshMs = 750,
    [int]$TailLines = 18,
    [switch]$Once
)

$ErrorActionPreference = "SilentlyContinue"

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
        return @()
    }
    try {
        return Get-Content -LiteralPath $Path -Tail $Lines -Encoding UTF8
    } catch {
        return @("<unable to read $Path>")
    }
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
            if (-not $children.ContainsKey([int]$proc.ParentProcessId)) {
                $children[[int]$proc.ParentProcessId] = New-Object System.Collections.Generic.List[int]
            }
            $children[[int]$proc.ParentProcessId].Add([int]$proc.ProcessId)
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

function Draw-Bar {
    param([string]$Status, [int]$Elapsed, [int]$Timeout, [string]$Ram, [string]$Avail)
    $width = [Console]::WindowWidth
    if ($width -lt 40) { $width = 120 }
    $text = " $Status | elapsed ${Elapsed}s / ${Timeout}s | proc RAM $Ram | system free $Avail "
    if ($text.Length -gt $width) {
        $text = $text.Substring(0, $width)
    }
    return $text.PadRight($width)
}

function Render-Station {
    $status = Read-JsonFile $StatusPath
    $state = if ($status -and $status.state) { [string]$status.state } else { "waiting" }
    $pid = if ($status -and $status.codex_pid) { [int]$status.codex_pid } else { $null }
    $startedAt = if ($status -and $status.started_at_epoch) { [double]$status.started_at_epoch } else { [double](Get-Date -UFormat %s) }
    $elapsed = [int]([double](Get-Date -UFormat %s) - $startedAt)
    $procRam = Format-BytesMiB (Get-ProcessTreeWorkingSetBytes $pid)
    $available = Get-SystemAvailableMiB
    $availableText = if ($null -eq $available) { "n/a" } else { "$available MiB" }

    Clear-Host
    Write-Host "CodeGraph External Agent Station" -ForegroundColor Cyan
    Write-Host ("=" * 76) -ForegroundColor DarkCyan
    Write-Host "Task:       $TaskId"
    Write-Host "State:      $state"
    Write-Host "Workspace:  $Workspace"
    Write-Host "Log root:   $LogRoot"
    Write-Host "Status:     $StatusPath"
    Write-Host "Codex PID:  $(if ($pid) { $pid } else { 'n/a' })"
    Write-Host "Elapsed:    ${elapsed}s / ${TimeoutSeconds}s"
    Write-Host "Process RAM tree: $procRam"
    Write-Host "System available: $availableText"
    if ($status) {
        Write-Host "Mode:       $($status.mode)"
        Write-Host "Context:    $($status.context_bytes) bytes | tools $($status.tool_calls)"
        Write-Host "Message:    $($status.message)"
    }
    Write-Host ""
    Write-Host "Last Codex Message" -ForegroundColor Yellow
    Read-TailText (Join-Path $LogRoot "codex_last_message.txt") 8 | ForEach-Object { Write-Host $_ }
    Write-Host ""
    Write-Host "STDERR Tail" -ForegroundColor Yellow
    Read-TailText (Join-Path $LogRoot "codex_stderr.txt") $TailLines | ForEach-Object { Write-Host $_ }
    Write-Host ""
    Write-Host "STDOUT Tail" -ForegroundColor Yellow
    Read-TailText (Join-Path $LogRoot "codex_stdout.jsonl") $TailLines | ForEach-Object { Write-Host $_ }
    Write-Host ""
    Write-Host (Draw-Bar $state $elapsed $TimeoutSeconds $procRam $availableText) -BackgroundColor DarkBlue -ForegroundColor White
}

if (-not $StatusPath) {
    $StatusPath = Join-Path $LogRoot "agent_status.json"
}

do {
    Render-Station
    if ($Once) { break }
    $status = Read-JsonFile $StatusPath
    $state = if ($status -and $status.state) { [string]$status.state } else { "" }
    if ($state -in @("completed", "failed", "timeout")) {
        Start-Sleep -Seconds 4
        break
    }
    Start-Sleep -Milliseconds $RefreshMs
} while ($true)
