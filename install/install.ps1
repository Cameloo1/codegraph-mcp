param(
    [switch] $DryRun,
    [string] $Version = "latest",
    [string] $InstallDir = "$HOME\.codegraph\bin"
)

$ErrorActionPreference = "Stop"

$Target = "x86_64-pc-windows-msvc"
$Archive = "codegraph-mcp-$Target.zip"
$Binary = Join-Path $InstallDir "codegraph-mcp.exe"

if ($DryRun) {
    [pscustomobject]@{
        status = "dry_run"
        version = $Version
        target = $Target
        archive = $Archive
        install_dir = $InstallDir
        binary = $Binary
        network = "not used in dry run"
        workflow = "single-agent-only"
    } | ConvertTo-Json -Compress
    exit 0
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Write-Error "Network download is intentionally not implemented in this template. Use GitHub release archives or cargo install for now."

