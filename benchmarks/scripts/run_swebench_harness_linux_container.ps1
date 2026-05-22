param(
    [string]$Image = "node:20-bookworm",
    [string]$InstanceId = "sympy__sympy-20590",
    [string]$RunId = "codegraph-setup-gold",
    [int]$TimeoutSeconds = 120
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$env:DOCKER_CONFIG = (Resolve-Path (Join-Path $repoRoot "benchmarks/workspaces/docker-config")).Path
$env:DOCKER_HOST = "npipe:////./pipe/dockerDesktopLinuxEngine"

$script = @"
set -eux
cd /work
mkdir -p /work/benchmarks/workspaces/swebench_gold_validation
apt-get update >/dev/null
DEBIAN_FRONTEND=noninteractive apt-get install -y python3-pip python3-venv git >/dev/null
python3 -m pip install --break-system-packages -e /work/benchmarks/upstream/SWE-bench >/dev/null
cd /work/benchmarks/workspaces/swebench_gold_validation
python3 -m swebench.harness.run_evaluation \
  --dataset_name princeton-nlp/SWE-bench_Lite \
  --predictions_path gold \
  --max_workers 1 \
  --instance_ids $InstanceId \
  --run_id $RunId \
  --timeout $TimeoutSeconds \
  --report_dir /work/benchmarks/workspaces/swebench_gold_validation/report
"@

docker run --rm `
  -v /var/run/docker.sock:/var/run/docker.sock `
  -v "${repoRoot}:/work" `
  -w /work `
  $Image bash -lc $script
