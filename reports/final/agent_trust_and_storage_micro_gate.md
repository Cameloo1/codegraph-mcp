# Agent Trust And Storage Micro Gate

## Verdict

Overall status: **PASS**

Sequence complete: **true**

This gate covers context-pack trust/correctness, CLI flag and path UX, read-only inspection side effects, storage lab v2 methodology, the built-in `audit storage-micro` diagnostic command, and generic storage/stress guardrails for larger corpus testing.

This is not a CGC gate, not a final intended-performance gate, and not a real-world recall gate.

## Stable Report Decision

Decision: **promote `reports/final/agent_trust_and_storage_micro_gate.md` and `.json` as stable public reports**.

Rationale: this is the canonical public pass/fail summary for the agent trust plus storage-micro remediation sequence. The stable report intentionally excludes local absolute paths, raw command logs, temporary DB paths, generated fixture paths, and command-level artifact payloads.

Detailed raw evidence remains in ignored audit artifacts and is not part of this public final report.

## Pass Criteria

| Criterion | Pass |
| --- | ---: |
| Cargo workspace build | true |
| Cargo workspace tests | true |
| Release binary build | true |
| Production context-pack excludes inline tests | true |
| Test-impact surfaces inline tests | true |
| Test-impact no-path fallback is useful | true |
| Global flags are targeted or supported | true |
| DB/repo resolution is explicit | true |
| Misplaced globals are not swallowed | true |
| Read-only inspection does not mutate the main DB | true |
| Sidecar-only changes are reported | true |
| Storage lab v2 includes N-series measurements | true |
| Built-in storage-micro command works | true |
| Storage/stress budget guardrails work | true |
| Buildroot medium-corpus opt-in is supported | true |
| Linux max-stress opt-in is supported | true |
| Normal `.codegraph` DB is not mutated | true |
| JSON reports validate | true |
| Docs checks pass after public-report cleanup | true |

## Storage Micro Summary

| Check | Result |
| --- | --- |
| Full storage-micro status | `ok` |
| Full run cases | `8` |
| Tiny smoke status | `ok` |
| Tiny smoke cases | `2` |
| DBSTAT available for full run cases | true |
| Explicit DB paths used | true |
| Normal `.codegraph` DB created | false |
| Production agent-use DB touched | false |
| Schema baseline bytes | `483328` |
| Simple-file slope, N=1 to N=10 | `65991.111` main DB bytes/file |
| Simple-file slope, N=10 to N=100 | `64079.644` main DB bytes/file |

## Storage And Stress Guardrail Summary

| Check | Result |
| --- | --- |
| Policy version | `storage-budget-v1` |
| Fixture/smoke DB budget | `10 MiB` |
| Fixture/smoke artifact budget | `10 MiB` |
| Normal self-use target | `250 MiB` |
| Extended opt-in threshold | `500 MiB` |
| Buildroot corpus role | medium corpus, opt-in only |
| Linux corpus role | max stress corpus, opt-in only |
| Stress runs require `--extended` | true |
| Stress runs require explicit external DB/output paths | true |
| Stress runs refuse default `.codegraph` | true |
| Stress raw artifacts refuse `reports/final` | true |
| Structured `storage_budget` JSON emitted | true |
| VHDX admin run completed | false |
| VHDX fallback evidence completed | true |
| Buildroot corpus indexed in this gate | false |
| Linux corpus indexed in this gate | false |

Buildroot is now the intended first realistic medium-corpus target for future external/VHDX-backed testing. This gate only validates the opt-in guardrails and refusal behavior; it does not claim Buildroot or Linux corpus benchmark results.

## Context Trust Summary

| Check | Result |
| --- | --- |
| Production context excludes inline tests | true |
| Production evidence roles | `production` |
| Test-impact surfaces inline tests | true |
| Test-impact evidence roles | `production`, `test` |
| Test-impact recommended tests include inline test | true |
| Fallback sources are labeled | true |
| Fallback evidence remains non-proof | true |

## CLI And DB Safety Summary

| Check | Result |
| --- | --- |
| Global flag placement has targeted behavior | true |
| DB/repo path resolution is explicit | true |
| Mismatched repo/DB reads are blocked | true |
| Read-only audit records main DB before/after state | true |
| Read-only audit records sidecar before/after state | true |
| Clean read-only inspection uses side-effect-minimal SQLite open mode | true |
| Existing sidecars are classified separately from main DB mutation | true |

## Claim Boundaries

- No final intended-performance pass is claimed from this gate.
- No CodeGraph vs CGC superiority claim is made.
- No real-world recall claim is made.
- No unsupported relation precision claim is made.
- Storage-micro is a diagnostic surface, not a broad benchmark claim.

## Public Report Hygiene

This stable report should remain small and public-safe. Do not add local absolute paths, raw command log names, generated DB paths, fixture working directories, or raw command payloads here. Keep that material in ignored audit artifacts unless it is intentionally promoted after review.
