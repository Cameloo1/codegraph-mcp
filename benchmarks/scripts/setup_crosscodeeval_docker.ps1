param(
    [string]$Image = "node:20-bookworm"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$env:DOCKER_CONFIG = (Resolve-Path (Join-Path $repoRoot "benchmarks/workspaces/docker-config")).Path
$env:DOCKER_HOST = "npipe:////./pipe/dockerDesktopLinuxEngine"

$script = @'
set -eux
cd /work/benchmarks/upstream/cceval
apt-get update >/dev/null
DEBIAN_FRONTEND=noninteractive apt-get install -y python3-pip python3-setuptools python3-wheel git build-essential >/dev/null
python3 -m pip install --break-system-packages tree_sitter==0.20.4 >/dev/null
git -C ts_package/tree-sitter-python checkout --force v0.20.4
git -C ts_package/tree-sitter-java checkout --force v0.20.2
git -C ts_package/tree-sitter-typescript checkout --force v0.20.6
git -C ts_package/tree-sitter-c-sharp checkout --force v0.20.0
rm -rf build
mkdir -p build
python3 scripts/build_ts_lib.py
python3 /work/benchmarks/workspaces/crosscodeeval_docker_probe/parser_load_smoke.py
'@

docker run --rm -v "${repoRoot}:/work" -w /work $Image bash -lc $script
