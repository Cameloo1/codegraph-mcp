param(
    [switch]$NoPause
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path (Join-Path $scriptDir "..\..")
$artifactRoot = Join-Path $repoRoot "reports\audit\artifacts\readme_terminal_demo"
$dbPath = Join-Path $artifactRoot "codegraph-demo.sqlite"
$fixtureRepo = Join-Path $repoRoot "fixtures\smoke\basic_repo"
$binary = Join-Path $repoRoot "target\release\codegraph-mcp.exe"

function Wait-Demo {
    if (-not $NoPause) {
        Start-Sleep -Milliseconds 850
    }
}

function Invoke-DemoCommand {
    param(
        [string]$Display,
        [string[]]$Args,
        [string[]]$Summary
    )

    Write-Host ""
    Write-Host "codegraph-demo> $Display" -ForegroundColor Green
    Wait-Demo
    $output = & $binary @Args 2>&1
    if ($LASTEXITCODE -ne 0) {
        $output | ForEach-Object { Write-Host $_ }
        throw "Command failed with exit code $LASTEXITCODE`: $Display"
    }
    foreach ($line in $Summary) {
        Write-Host $line
    }
    Wait-Demo
}

Set-Location $repoRoot
New-Item -ItemType Directory -Force -Path $artifactRoot | Out-Null
Remove-Item -LiteralPath $dbPath -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath "$dbPath-wal" -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath "$dbPath-shm" -Force -ErrorAction SilentlyContinue

if (-not (Test-Path -LiteralPath $binary)) {
    throw "Release binary not found. Run: cargo build --release --bin codegraph-mcp"
}

Write-Host "codegraph quickstart demo" -ForegroundColor Cyan
Write-Host "fixture repo: fixtures\smoke\basic_repo" -ForegroundColor DarkGray
Write-Host "demo DB: reports\audit\artifacts\readme_terminal_demo\codegraph-demo.sqlite" -ForegroundColor DarkGray
Wait-Demo

Invoke-DemoCommand `
    -Display "codegraph-mcp index fixtures\smoke\basic_repo --db reports\audit\artifacts\readme_terminal_demo\codegraph-demo.sqlite --fresh --json" `
    -Args @(
        "index", $fixtureRepo,
        "--db", $dbPath,
        "--fresh",
        "--json",
        "--no-vector-audit"
    ) `
    -Summary @(
        "ok: graph DB written with lifecycle/passport metadata",
        "ok: source spans and exactness labels captured"
    )

Invoke-DemoCommand `
    -Display "codegraph-mcp --db reports\audit\artifacts\readme_terminal_demo\codegraph-demo.sqlite query symbols greet --json" `
    -Args @(
        "--db", $dbPath,
        "query", "symbols", "greet",
        "--json"
    ) `
    -Summary @(
        "evidence: symbol `greet` found in fixture source",
        "proof label: query evidence is source-spanned, not a benchmark claim"
    )

Invoke-DemoCommand `
    -Display "codegraph-mcp --db reports\audit\artifacts\readme_terminal_demo\codegraph-demo.sqlite context-pack --task `"Trace greet caller`" --seed greet --budget 1200 --json" `
    -Args @(
        "--db", $dbPath,
        "context-pack",
        "--task", "Trace greet caller",
        "--seed", "greet",
        "--budget", "1200",
        "--json"
    ) `
    -Summary @(
        "packet: bounded context emitted for the task",
        "boundary: graph/source verification controls proof",
        "candidate evidence stays labeled until verified"
    )

Write-Host ""
Write-Host "done: graph/source verification controls proof; candidate evidence stays labeled." -ForegroundColor Cyan
