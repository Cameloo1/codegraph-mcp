# Agent Use Quality Gate

## Verdict

Overall status: **PASS**

Agent-facing output ready: **true**

This gate covers output shape, context-pack evidence classification, CLI flag UX, schema contract tests, docs checks, and one isolated release-binary self-use smoke. It does not claim a final intended-performance pass, CodeGraph-vs-CGC superiority, storage-gate pass, cold-build pass, or real-world recall.

## Stable Report Decision

Decision: **promote `reports/final/agent_use_quality_gate.md` and `.json` as stable public reports**.

Rationale: this is the canonical final pass/fail report for the verified agent-use output contract. The stable report intentionally excludes local absolute paths, raw logs, temporary DB paths, smoke fixture paths, and command-level artifact payloads.

Detailed raw evidence remains in ignored audit artifacts and is not part of this public final report.

## Required Gates

| Gate | Status |
| --- | --- |
| Cargo workspace build | PASS |
| Cargo workspace tests | PASS |
| Release binary build | PASS |
| Context-pack source-role tests | PASS |
| Query agent JSON tests | PASS |
| Index concise JSON tests | PASS |
| Index explain-scope tests | PASS |
| CLI global flag tests | PASS |
| Agent JSON schema tests | PASS |
| Isolated release-binary self-use smoke | PASS |
| Docs link and hygiene checks | PASS |
| Production context excludes inline tests | PASS |
| Test-impact includes inline tests intentionally | PASS |
| Query symbols is bounded or returns targeted correction | PASS |
| Agent JSON outputs are bounded | PASS |
| Index JSON is concise by default | PASS |
| Audit scope output is explicit | PASS |
| Global flag after command returns targeted correction | PASS |

## Isolated Self-Use Smoke Summary

The smoke used a release binary, an isolated fixture, and explicit external SQLite DB paths outside the fixture repo. It did not create a normal in-repo `.codegraph` DB.

| Check | Result |
| --- | --- |
| Release binary used | true |
| External DB used | true |
| Normal `.codegraph` DB created | false |
| DB under fixture repo | false |
| SQLite files inside fixture repo | 0 |
| Global flags before command worked | true |
| Global `--db` after query returned targeted correction | true |

Output sizes from the smoke stayed bounded:

| Surface | Bytes |
| --- | ---: |
| Release `--help` | 1818 |
| Index `--agent-json` | 2248 |
| Index `--json` concise | 2543 |
| Query symbols `--agent-json --limit 5` | 3181 |
| Context-pack production `--agent-json` | 3863 |
| Context-pack test-impact `--agent-json` | 6923 |
| Index `--json --explain-scope` | 3251 |

## Evidence Classification

| Check | Result |
| --- | --- |
| Production context evidence roles | `production` |
| Test-impact evidence roles | `mixed`, `unknown` |
| Inline tests appear in production mode | false |
| Test-impact includes inline test evidence intentionally | true |

## Claim Boundaries

- No final intended-performance pass is claimed from this gate.
- No CodeGraph vs CGC superiority claim is made.
- No real-world recall claim is made.
- Manual relation precision remains sampled precision only unless separate stable reports prove otherwise.
- Absent proof-mode relations have no precision claim from this gate.

## Public Report Hygiene

This stable report should remain small and public-safe. Do not add local absolute paths, raw command log names, generated database paths, fixture working directories, or raw command payloads here. Keep that material in ignored audit artifacts unless it is intentionally promoted after review.
