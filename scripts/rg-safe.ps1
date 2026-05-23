param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RgArgs
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$workspaceRg = Join-Path $repoRoot ".codex-tools\rg.exe"
if (Test-Path -LiteralPath $workspaceRg) {
    $rg = $workspaceRg
} else {
    $cmd = Get-Command "rg" -ErrorAction Stop
    $rg = $cmd.Source
}

$ignoreFile = Join-Path $repoRoot ".rgignore"
& $rg --ignore-file $ignoreFile @RgArgs
exit $LASTEXITCODE
