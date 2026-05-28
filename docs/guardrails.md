# Guardrails

The root `README.md` is the public project contract. This document records the
operational rules that keep CodeGraph evidence useful instead of turning it into
noisy diagnostic or development residue.

## Evidence Boundary

Use public docs for stable command contracts and promoted behavior. Keep
evolving lab reports, optimizer experiments, and raw local diagnostics on lab
branches unless a small summary is intentionally promoted.

Do not infer current quality from old run payloads, raw logs, copied fixtures,
temporary DBs, or local experiment directories. Unsupported, skipped, timed-out,
debug-only, partial, or diagnostic data must stay labeled that way.

## Repository Hygiene

Keep these out of committed source unless a small summary is explicitly
promoted:

- SQLite DBs and sidecars: `*.sqlite`, `*.db`, `*.sqlite-wal`, `*.sqlite-shm`
- build and dependency outputs: `target/`, `node_modules/`, `.venv/`, caches
- raw diagnostic and comparison payloads
- final local-run artifact directories
- audit scratch artifact directories
- raw competitor payload directories
- normalized task payload directories
- timestamped local run directories

Public docs should not contain machine-local absolute paths, private checkout
paths, copied experiment trees, or raw artifact inventories.

## DB Lifecycle

Default indexing is safe-auto:

- no DB: build a fresh DB, validate it, then publish it
- valid matching DB: reuse incrementally
- invalid, mismatched, stale, corrupt, or unknown default DB: build a fresh
  replacement and publish only after validation
- failed replacement: leave the old DB untouched

Explicit `--db <path>` is more conservative. Invalid or mismatched named DBs
fail unless the caller explicitly requests a fresh rebuild. Query, context,
status, MCP, watch, doctor, and bundle read paths should surface the
passport/preflight decision instead of silently trusting an unsafe DB.

## Metrics And Claims

Any metric or report output must include enough metadata to decide whether a
result is claimable:

- exact command
- binary profile and debug-assertion status
- DB lifecycle decision
- whether the DB was fresh, reused, stale, or diagnostic
- timeout and skip reasons
- artifact path and freshness classification

Debug timing can be useful diagnosis, but it must not be used for production
threshold verdicts. Partial comparison artifacts are not final storage
artifacts and do not support superiority claims. Lab and optimizer details stay
off release-facing docs unless explicitly promoted.

## Retrieval Contract

The runtime contract is:

```text
vectors suggest; graph verifies; source spans support the packet
```

Retrieval candidates may come from exact seeds, text search, binary signatures,
compressed vectors, or rankers. Final context packets should be grounded in
typed graph facts, exactness labels, source spans, provenance, and explicit
path evidence.

Current candidate lanes include exact seeds, Stage 0 text evidence,
lexical/FTS matches, vector semantic candidates, binary-vector candidates,
nuance-rescue candidates, graph-neighborhood candidates, PathEvidence
candidates, and fallback source-text evidence. These are candidate/source
evidence until graph/source verification promotes them. If no proof path is
found, output should say `no_proof_path_found` and label any source-text
fallback as text evidence, not graph proof.

Planning fields such as `follow_up_queries`, validation hints, and risks are
agent-orientation metadata. They are not shell-ready commands, are not internal
`rg` execution, and do not override evidence-role or proof-status labels.

## Failure Handling

Failures should be first-class and inspectable:

- fail closed for unsafe DB reads
- keep old DBs until replacements pass validation
- preserve timeout and skip reasons
- report unknowns as unknown
- avoid silently converting unsupported capabilities into incorrect results
- keep raw evidence locally when useful, but publish only curated summaries

These rules are intentionally boring. They protect the normal user path while
still leaving enough evidence to debug hard failures.
