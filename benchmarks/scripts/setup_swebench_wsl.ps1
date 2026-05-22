param(
    [string]$Distro = "Ubuntu"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$wslList = wsl -l -v 2>&1
if ($LASTEXITCODE -ne 0 -or ($wslList -join "`n") -match "no installed distributions") {
    throw "No WSL distro is installed. Run: wsl --install -d $Distro, reboot/sign in if prompted, then rerun this script."
}

$linuxPath = (wsl wslpath -a "$repoRoot").Trim()
wsl -d $Distro bash "$linuxPath/benchmarks/scripts/setup_swebench_wsl.sh" "$linuxPath"
