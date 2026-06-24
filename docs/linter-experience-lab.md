# Linter Experience Lab

The linter experience lab is a local diagnostic runner for the
`agent-use validate-edit` loop. It creates disposable fixture repositories,
indexes them with the release `codegraph-mcp` binary, mutates selected files,
captures the actual validation packets, and records DB/profile sizes and command
timings.

It is meant to answer practical product questions:

- what does the agent actually receive after an edit?
- what does the DB/profile footprint look like?
- which findings are blocking, warning, unknown, or diagnostic-only?
- does a repair clear the packet?
- did the run mutate normal repo-local `.codegraph` state?

This lab is not a public benchmark, SWE-bench score, real-agent patch-quality
result, or CodeGraph-over-`rg` claim.

## Run

Build or reuse the release binary, then run:

```powershell
cargo build --release --bin codegraph-mcp
python scripts\run_linter_experience_lab.py --clean
```

The runner uses:

- release binary: `target\release\codegraph-mcp.exe`
- disposable workspaces/profiles: `target\linter-experience-lab\`
- stable report: `reports\audit\linter_experience_lab.md`
- stable JSON: `reports\audit\linter_experience_lab.json`
- gallery HTML: `reports\audit\artifacts\linter_experience_lab\latest\linter_experience_lab.html`
- command log: `reports\audit\artifacts\linter_experience_lab\latest\commands.jsonl`

The disposable profile root is intentionally short and ignored. Earlier attempts
to put production-profile DBs under deep report paths can fail on Windows when
SQLite opens temporary publish files.

## What It Shows

Each fixture follows the same pattern:

1. create a disposable repo
2. run `agent-use status --json`
3. run `agent-use index --json`
4. mutate one or more files
5. run `agent-use validate-edit --agent-json`
6. record packet summary, full packet JSON, DB anatomy, and command logs

The default scenarios cover:

- clean no-op validation
- a Python invented-call edit followed by a repair
- a Rust same-file deleted-callee edit followed by a repair

The report surfaces:

- validation status
- hard-interrupt availability
- packet byte size
- top findings
- DB size
- selected SQLite table row counts
- profile artifact size
- exact argv/stdout/stderr/timing records

## How To Read Results

Use the compact packet for the agent-facing view:

- `validation_packet.status`
- `severity_summary`
- `must_fix_before_continuing`
- `hard_interrupt_available`
- `summary_counts_by_classification`
- `top_findings`
- `recommended_fix`
- `omitted_count`
- `expansion_handles`

Use the DB anatomy only as local diagnostic context. Row counts and file sizes
explain the footprint behind the packet; they are not product quality claims.

The most important linter-experience checks are:

- bad edit produces a useful finding
- fixing the edit clears the finding
- stale, missing, or unsafe DB state stays non-claimable
- candidate/text/vector/source-navigation evidence does not become graph proof
- normal `.codegraph` is not created or mutated
