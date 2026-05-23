param(
    [string]$Image = "node:20-bookworm",
    [string]$InstanceId = "sympy__sympy-20590",
    [string]$RunId = "codegraph-setup-gold",
    [int]$TimeoutSeconds = 120,
    [string]$DockerCommand = "docker"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")
$dockerConfig = Join-Path $repoRoot "benchmarks\tracks\swebench_lite\workspaces\docker-config"
New-Item -ItemType Directory -Force -Path $dockerConfig | Out-Null
$env:DOCKER_CONFIG = (Resolve-Path $dockerConfig).Path
$env:DOCKER_HOST = "npipe:////./pipe/dockerDesktopLinuxEngine"

$script = @"
set -eux
cd /work
mkdir -p /work/benchmarks/tracks/swebench_lite/workspaces/gold_validation
apt-get update >/dev/null
DEBIAN_FRONTEND=noninteractive apt-get install -y python3-pip python3-venv git >/dev/null
SWEBENCH=/work/benchmarks/tracks/swebench_lite/upstream/SWE-bench
if [ ! -d "$SWEBENCH" ] && [ -d /work/benchmarks/upstream/SWE-bench ]; then
  SWEBENCH=/work/benchmarks/upstream/SWE-bench
fi
python3 -m pip install --break-system-packages -e "$SWEBENCH" >/dev/null
cd /work/benchmarks/tracks/swebench_lite/workspaces/gold_validation
python3 -m swebench.harness.run_evaluation \
  --dataset_name princeton-nlp/SWE-bench_Lite \
  --predictions_path gold \
  --max_workers 1 \
  --instance_ids $InstanceId \
  --run_id $RunId \
  --timeout $TimeoutSeconds \
  --report_dir /work/benchmarks/tracks/swebench_lite/workspaces/gold_validation/report
"@
$script = $script -replace "`r`n", "`n"
$script = $script -replace "`r", "`n"

& $DockerCommand run --rm `
  -v /var/run/docker.sock:/var/run/docker.sock `
  -v "${repoRoot}:/work" `
  -w /work `
  $Image bash -lc $script

if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
