param(
    [ValidateSet("dev", "prod-agent")]
    [string] $Profile = "prod-agent",

    [ValidateSet("status", "index", "context-pack", "query-symbols", "serve-mcp")]
    [string] $Action = "status",

    [string] $Repo,
    [string] $Db,
    [string] $Binary,
    [string] $Task,
    [string[]] $Seed,
    [string] $Query,
    [int] $Budget = 1600,
    [switch] $BuildIfMissing
)

$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRootDefault = Resolve-Path -LiteralPath (Join-Path $scriptRoot "..")
if (-not $Repo) {
    $Repo = $repoRootDefault.Path
}
$repoRoot = (Resolve-Path -LiteralPath $Repo).Path

if ($Profile -eq "dev") {
    $label = "DEVELOPMENT_SELF_TEST"
    if (-not $Db) {
        $Db = Join-Path $repoRoot ".codegraph\development-self-test.sqlite"
    }
    if (-not $Binary) {
        $Binary = Join-Path $repoRoot "target\debug\codegraph-mcp.exe"
    }
    $buildArgs = @("build", "--bin", "codegraph-mcp")
} else {
    $label = "PRODUCTION_AGENT_USE"
    if (-not $Db) {
        $safeName = (Split-Path -Leaf $repoRoot) -replace "[^A-Za-z0-9._-]", "_"
        $Db = Join-Path $env:LOCALAPPDATA "CodeGraphMCP\agent-indexes\$safeName\production-agent-use.sqlite"
    }
    if (-not $Binary) {
        $Binary = Join-Path $repoRoot "target\release\codegraph-mcp.exe"
    }
    $buildArgs = @("build", "--release", "--bin", "codegraph-mcp")
}

$dbParent = Split-Path -Parent $Db
New-Item -ItemType Directory -Force -Path $dbParent | Out-Null

if (-not (Test-Path -LiteralPath $Binary)) {
    if (-not $BuildIfMissing) {
        throw "CodeGraph binary missing for $label`: $Binary. Run with -BuildIfMissing or build it explicitly."
    }
    Push-Location $repoRoot
    try {
        & cargo @buildArgs
        if ($LASTEXITCODE -ne 0) {
            throw "cargo $($buildArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

Write-Host "[CodeGraph profile] $label"
Write-Host "[CodeGraph repo] $repoRoot"
Write-Host "[CodeGraph db] $Db"
Write-Host "[CodeGraph binary] $Binary"

$argsList = @("--repo", $repoRoot, "--db", $Db)

switch ($Action) {
    "status" {
        $argsList += @("--json", "status", $repoRoot)
    }
    "index" {
        $argsList += @("--json", "--profile", "index", $repoRoot)
    }
    "context-pack" {
        if (-not $Task) {
            throw "-Task is required for context-pack."
        }
        $argsList += @("--json", "context-pack", "--task", $Task, "--budget", "$Budget")
        $seedValues = @()
        foreach ($seedInput in $Seed) {
            $seedValues += ($seedInput -split "," | ForEach-Object { $_.Trim() } | Where-Object { $_ })
        }
        foreach ($seedValue in $seedValues) {
            $argsList += @("--seed", $seedValue)
        }
    }
    "query-symbols" {
        if (-not $Query) {
            throw "-Query is required for query-symbols."
        }
        $argsList += @("--json", "query", "symbols", $Query)
    }
    "serve-mcp" {
        $argsList += @("serve-mcp")
    }
}

& $Binary @argsList
exit $LASTEXITCODE
