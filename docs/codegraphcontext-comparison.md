# CodeGraphContext Comparison

The root `README.md` is the public setup contract. This external competitor
benchmark does not weaken graph-first correctness gates. Do not use subagents.

## Competitor

- Name: CodeGraphContext / CGC
- Source: `https://github.com/CodeGraphContext/CodeGraphContext`
- Observed version hint: `0.4.7`
- Interface: black-box CLI only
- Executables: `cgc` or `codegraphcontext`

The benchmark never depends on CGC internal Python APIs.

## Local Setup Plan

```powershell
git clone https://github.com/CodeGraphContext/CodeGraphContext .codegraph-competitors\CodeGraphContext
git -C .codegraph-competitors\CodeGraphContext rev-parse HEAD
python -m venv .codegraph-competitors\CodeGraphContext\.venv
.\.codegraph-competitors\CodeGraphContext\.venv\Scripts\python.exe -m pip install --upgrade pip
.\.codegraph-competitors\CodeGraphContext\.venv\Scripts\python.exe -m pip install -e .\.codegraph-competitors\CodeGraphContext
$env:CGC_COMPETITOR_BIN = ".\.codegraph-competitors\CodeGraphContext\.venv\Scripts\cgc.exe"
$env:CGC_COMPETITOR_COMMIT = "<commit-sha-from-rev-parse>"
$env:CGC_COMPETITOR_INSTALL_MODE = "editable"
```

Normal workspace tests do not require this setup and do not require network.
Network is only needed when you choose to clone/install the competitor.

## Run

```powershell
codegraph-mcp bench cgc-comparison
```

Optional controls:

```powershell
codegraph-mcp bench cgc-comparison --output-dir reports\cgc-comparison\manual --timeout-ms 60000 --top-k 10 --competitor-bin <path-to-cgc>
```

## Fixtures

- `ts-call-chain`
- `auth-route`
- `event-flow`
- `mutation-impact`
- `test-impact`

Each fixture writes a `ground_truth.json` with query text, expected files,
expected symbols, expected relation sequence, expected path symbols, critical
source spans, and explicitly allowed unsupported fields.

## Fairness Rules

- Same fixture repo.
- Same query wording.
- Fresh index per run unless a warm-cache mode is explicitly labeled.
- Same top-k.
- Same timeout.
- No network during benchmark execution.
- Unsupported capability is distinct from incorrect result.
- Do not claim SOTA superiority without measured evidence.

## Outputs

```text
reports/cgc-comparison/<timestamp>/
|-- run.json
|-- per_task.jsonl
|-- summary.md
|-- normalized_outputs/
|   |-- codegraph/
|   `-- codegraphcontext/
`-- raw_artifacts/
    `-- codegraphcontext/
```

`run.json` includes the competitor manifest: source URL, pinned commit when
provided through `CGC_COMPETITOR_COMMIT`, detected package version when the
executable reports it, Python version, executable path, backend hint, install
mode, timestamp, and host/platform metadata.
