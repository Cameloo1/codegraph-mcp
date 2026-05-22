param(
    [string]$Image = "node:20-bookworm"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")
$dockerConfig = Join-Path $repoRoot "benchmarks\tracks\crosscodeeval\workspaces\docker-config"
New-Item -ItemType Directory -Force -Path $dockerConfig | Out-Null
$env:DOCKER_CONFIG = (Resolve-Path $dockerConfig).Path
$env:DOCKER_HOST = "npipe:////./pipe/dockerDesktopLinuxEngine"

$script = @'
set -eux
CCEVAL=/work/benchmarks/tracks/crosscodeeval/upstream/cceval
if [ ! -d "$CCEVAL" ] && [ -d /work/benchmarks/upstream/cceval ]; then
  CCEVAL=/work/benchmarks/upstream/cceval
fi
cd "$CCEVAL"
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
PROBE=/work/benchmarks/tracks/crosscodeeval/workspaces/crosscodeeval_docker_probe/parser_load_smoke.py
if [ ! -f "$PROBE" ]; then
  PROBE=/work/benchmarks/workspaces/crosscodeeval_docker_probe/parser_load_smoke.py
fi
python3 "$PROBE"
'@

docker run --rm -v "${repoRoot}:/work" -w /work $Image bash -lc $script
