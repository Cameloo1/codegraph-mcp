param(
    [ValidateSet("dev", "prod-agent")]
    [string] $Profile = "prod-agent",

    [ValidateSet("status", "index", "context-pack", "query-symbols", "mcp-config", "serve-mcp")]
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
    $dbParent = Split-Path -Parent $Db
    New-Item -ItemType Directory -Force -Path $dbParent | Out-Null
} else {
    $label = "PRODUCTION_AGENT_USE"
    if ($Db) {
        throw "-Db is not supported for prod-agent; the native agent-use resolver owns the external profile DB path."
    }
    if (-not $Binary) {
        $Binary = Join-Path $repoRoot "target\release\codegraph-mcp.exe"
    }
    $buildArgs = @("build", "--release", "--bin", "codegraph-mcp")
}

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
Write-Host "[CodeGraph binary] $Binary"

if ($Profile -eq "prod-agent") {
    Write-Host "[CodeGraph db] resolved by native agent-use profile"

    $argsList = @()
    switch ($Action) {
        "status" {
            $argsList = @("agent-use", "status", "--repo", $repoRoot, "--json")
        }
        "index" {
            $argsList = @("agent-use", "index", "--repo", $repoRoot, "--json", "--profile")
        }
        "context-pack" {
            if (-not $Task) {
                throw "-Task is required for context-pack."
            }
            $argsList = @("agent-use", "context-pack", "--repo", $repoRoot, "--task", $Task, "--budget", "$Budget", "--agent-json")
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
            $argsList = @("agent-use", "query", "symbols", $Query, "--repo", $repoRoot, "--agent-json")
        }
        "mcp-config" {
            $argsList = @("agent-use", "mcp-config", "--repo", $repoRoot, "--json")
        }
        "serve-mcp" {
            $configRaw = & $Binary agent-use mcp-config --repo $repoRoot --json
            if ($LASTEXITCODE -ne 0) {
                exit $LASTEXITCODE
            }
            $config = $configRaw | ConvertFrom-Json
            $server = $config.mcp_config.mcpServers.'codegraph-mcp'
            & $Binary @($server.args)
            exit $LASTEXITCODE
        }
    }

    & $Binary @argsList
    exit $LASTEXITCODE
}

Write-Host "[CodeGraph db] $Db"

if ($Action -eq "mcp-config") {
    throw "-Action mcp-config is only supported for -Profile prod-agent."
}

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
