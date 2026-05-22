param(
    [string]$Output = "benchmarks/tracks/repobench/workspaces/repobench_data/repobench_python_v1_1_real_smoke.jsonl",
    [int]$Rows = 20,
    [string]$Dataset = "tianyang/repobench_python_v1.1",
    [string]$Split = "cross_file_first"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")
$venvPython = Join-Path $repoRoot "benchmarks\tracks\repobench\workspaces\benchmark-setup-venv\Scripts\python.exe"
if (-not (Test-Path -LiteralPath $venvPython)) {
    python -m venv (Join-Path $repoRoot "benchmarks\tracks\repobench\workspaces\benchmark-setup-venv")
}
& $venvPython -m pip install datasets

$outputPath = Join-Path $repoRoot $Output
New-Item -ItemType Directory -Force -Path (Split-Path $outputPath) | Out-Null

$script = @"
import json
from itertools import islice
from datasets import load_dataset

dataset = load_dataset("$Dataset", split="$Split", streaming=True)
with open(r"$outputPath", "w", encoding="utf-8") as handle:
    for row in islice(dataset, $Rows):
        handle.write(json.dumps(row, ensure_ascii=False) + "\n")
print("wrote", "$outputPath")
"@

& $venvPython -c $script
