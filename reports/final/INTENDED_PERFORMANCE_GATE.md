# Intended Performance Gate

Generated: 2026-05-11 04:48:52 -05:00

Important: this is not MVP readiness. It checks the intended real large-codebase performance standard.

## Executive Verdict

Verdict: fail.

The app now passes the semantic preconditions that previously blocked a fair Autoresearch-scale performance read: graph truth passes 11/11, context packet quality passes 11/11, repeat/update durability is clean, and no malformed DB was produced by the clean rerun.

It still does not meet the intended large-codebase standard. Cold index, repeat unchanged index, single-file update latency, DB size, and large-repo `context_pack` latency all fail target.

## Pass/Fail Criteria

| Criterion | Status | Target | Observed |
| --- | --- | --- | --- |
| all_adversarial_fixtures_pass | pass | all fixtures pass | 11/11 |
| forbidden_edges_paths_zero | pass | 0 | edges=0, paths=0 |
| proof_grade_source_span_coverage | pass | 100% / 0 failures | graph truth source-span failures=0; context proof span coverage=100% |
| unresolved_exact_zero | pass | 0 | 0 |
| derived_without_provenance_zero | pass | 0 | 0 |
| test_mock_leakage_zero | pass | 0 | 0 |
| stale_fact_zero | pass | 0 | 0 |
| context_critical_symbol_recall_100 | pass | 100% | 100% (15/15) |
| context_proof_path_coverage_100 | pass | 100% | 100% (11/11) |
| autoresearch_db_integrity_clean | pass | integrity ok | cold/repeat/update integrity ok |
| autoresearch_no_duplicate_repeat_failure | pass | no duplicate insertion failure | repeat/update duplicate_edges_upserted=0 |
| autoresearch_cold_index_under_60s | fail | <=60000 ms | 442145 ms profile; 676400 ms shell |
| autoresearch_unchanged_repeat_under_5s | fail | <=5000 ms | 13571 ms profile |
| autoresearch_single_file_update_under_750ms | fail | <=750 ms p95 | 80207 ms update; 73249 ms restore |
| autoresearch_db_family_under_250mib | fail | <=262144000 bytes | 1712730112 bytes |
| context_pack_p95_under_2s | fail | <=2000 ms p95 on large repo | 30315 ms |
| unresolved_calls_page_under_1s | pass | <=1000 ms | 68 ms |
| no_cgc_win_claim_on_incomplete | pass | no win when CGC timed out/skipped/errored | CGC was not rerun in this clean Autoresearch pass; no win claimed |

## Correctness

Fixture pass/fail: 11/11 passed.

Forbidden edges: 0. Forbidden paths: 0. Source-span failures: 0.

Derived-without-provenance: 0. Exact-labeled unresolved: 0. Test/mock production leakage: 0. Stale fact failures: 0.

Graph truth now includes the previously failing derived provenance case. `ordersTable`, `WRITES`, derived `MAY_MUTATE`, and `path://derived/provenance` all pass.

## Relation Quality

Manual sample audit status: samples exist, but the clean Autoresearch samples are not manually labeled. Real relation precision remains unknown until human labels are ingested.

Fixture relation metrics are green:

| Graph Truth Relation | Expected | Matched | Forbidden Hits | Precision | Recall |
| --- | ---: | ---: | ---: | ---: | ---: |
| ASSERTS | 1 | 1 | 0 | 1.0 | 1.0 |
| CALLS | 9 | 9 | 0 | 1.0 | 1.0 |
| CHECKS_ROLE | 2 | 2 | 0 | 1.0 | 1.0 |
| FLOWS_TO | 1 | 1 | 0 | 1.0 | 1.0 |
| IMPORTS | 3 | 3 | 0 | 1.0 | 1.0 |
| MAY_MUTATE | 1 | 1 | 0 | 1.0 | 1.0 |
| MOCKS | 1 | 1 | 0 | 1.0 | 1.0 |
| SANITIZES | 0 | 0 | 0 | unknown | unknown |
| STUBS | 1 | 1 | 0 | 1.0 | 1.0 |
| WRITES | 1 | 1 | 0 | 1.0 | 1.0 |

Clean Autoresearch top relation counts:

| Relation | Edges | Source spans |
| --- | ---: | ---: |
| CONTAINS | 907236 | 907236 |
| DEFINED_IN | 897965 | 897965 |
| CALLEE | 524760 | 524760 |
| CALLS | 516392 | 516392 |
| ARGUMENT_0 | 332104 | 332104 |
| DECLARES | 214406 | 214406 |
| FLOWS_TO | 163481 | 163481 |
| ARGUMENT_1 | 73913 | 73913 |
| DEFINES | 72897 | 72897 |
| IMPORTS | 72626 | 72626 |

Sample artifacts exist for CALLS, READS, WRITES, FLOWS_TO, and PathEvidence under `reports/audit/artifacts/clean_autoresearch_sample_*`.

## Context Packet Quality

Fixture context packet gate: 11/11 passed.

| Metric | Value |
| --- | ---: |
| Critical symbol recall | 100% |
| Proof-path coverage | 100% |
| Proof-path source-span coverage | 100% |
| Critical snippet coverage | 100% |
| Expected tests recall | 100% |
| Distractor ratio | 0% |

Stored PathEvidence exists on the clean Autoresearch DB: 4,096 rows. The sampler returned 20 stored PathEvidence samples and did not need generated fallback paths.

Large-repo `context_pack` remains a performance failure: p95 was 30,315 ms across 5 iterations.

## Indexing

Clean Autoresearch cold index:

| Metric | Value |
| --- | ---: |
| Files walked | 8,561 |
| Files read | 4,975 |
| Files hashed | 4,975 |
| Files parsed | 4,975 |
| Files skipped | 3,586 |
| Entities | 1,630,146 |
| Edges | 3,866,431 |
| Source spans | 5,496,577 |
| Wall time | 442,145 ms profile; 676.4 s shell |
| Graph fact hash | `fnv64:bc69dbacfcb9987e` |

Clean Autoresearch unchanged repeat:

| Metric | Value |
| --- | ---: |
| Files walked | 8,561 |
| Metadata unchanged | 4,975 |
| Files read | 0 |
| Files hashed | 0 |
| Files parsed | 0 |
| Wall time | 13,571 ms profile |
| Duplicate edges upserted | 0 |
| Graph fact hash | `fnv64:bc69dbacfcb9987e` |

Clean Autoresearch single-file update:

| Metric | Value |
| --- | ---: |
| Mutation file | `python/autoresearch_utils/metrics_tools.py` |
| Files read/hashed/parsed | 1/1/1 |
| Update wall time | 80,207 ms |
| Restore wall time | 73,249 ms |
| Transaction status | committed |
| Integrity status | ok |
| Duplicate edges upserted | 0 |

Worker-count determinism was not rerun as a separate full Autoresearch 1-vs-N comparison in this clean pass. Fixture determinism remains covered by the prior gate and tests; this clean report does not upgrade that to a fresh large-repo determinism claim.

## Storage

Clean Autoresearch DB family:

| Metric | Value |
| --- | ---: |
| DB bytes | 1,712,730,112 |
| WAL bytes at audit | 0 |
| SHM sidecar observed after sampler | 32,768 |
| Average database bytes per edge | 442.97 |
| Edge count | 3,866,431 |
| PathEvidence rows | 4,096 |

Source snippet storage policy: snippets are loaded from source files for audit/context; snippets are not stored redundantly in SQLite by default.

Top storage contributors:

| Object | Type | Bytes |
| --- | --- | ---: |
| edges | table | 573,857,792 |
| sqlite_autoindex_qualified_name_dict_1 | index | 218,021,888 |
| qualified_name_dict | table | 205,287,424 |
| entities | table | 117,542,912 |
| sqlite_autoindex_object_id_dict_1 | index | 91,865,088 |
| object_id_dict | table | 82,673,664 |
| idx_edges_tail_relation | index | 73,654,272 |
| idx_edges_head_relation | index | 73,617,408 |
| idx_edges_span_path | index | 62,033,920 |
| sqlite_autoindex_qname_prefix_dict_1 | index | 46,792,704 |

## Query Latency

Clean Autoresearch latency:

| Query | P95 | Target | Status |
| --- | ---: | ---: | --- |
| context_pack | 30,315 ms | 2,000 ms | fail |
| unresolved-calls page | 68 ms | 1,000 ms | pass |

`context_pack` is the highest-priority performance bug now that stored PathEvidence and context quality are present.

## CodeGraph vs CGC

CGC was not rerun in this clean Autoresearch pass. The fair comparison verdict remains unknown/incomplete. No CodeGraph benchmark win is claimed.

## Known Unsupported Or Unknown Areas

- Real relation precision on Autoresearch is unknown until manual sample labels are completed.
- Large-repo worker-count determinism was not freshly measured in this clean pass.
- Dynamic/computed roles, complex interprocedural sanitizer/dataflow, and broad framework route syntaxes remain unsupported or heuristic unless explicitly proven.
- Real model coding quality was not run; fake-agent dry runs are not model-quality evidence.

## Next Required Work

1. Profile and reduce cold index write time; current profile is dominated by DB/write/FTS-index work.
2. Bring large-repo `context_pack` p95 from about 30s to <=2s while preserving proof-path/source-span semantics.
3. Reduce single-file update latency from about 80s to <=750 ms p95.
4. Resume storage optimization only from the clean DB evidence; current size is about 1.63 GiB versus the 250 MiB target.
5. Complete manual relation labeling before claiming real relation precision.

## Exact Failed Targets

- autoresearch_cold_index_under_60s: 442145 ms profile, 676400 ms shell.
- autoresearch_unchanged_repeat_under_5s: 13571 ms profile.
- autoresearch_single_file_update_under_750ms: 80207 ms update, 73249 ms restore.
- autoresearch_db_family_under_250mib: 1712730112 bytes.
- context_pack_p95_under_2s: 30315 ms.

## Exact Passed Targets

- all_adversarial_fixtures_pass: 11/11.
- forbidden_edges_paths_zero: edges=0, paths=0.
- proof_grade_source_span_coverage: 100%.
- unresolved_exact_zero: 0.
- derived_without_provenance_zero: 0.
- test_mock_leakage_zero: 0.
- stale_fact_zero: 0.
- context_critical_symbol_recall_100: 100%.
- context_proof_path_coverage_100: 100%.
- autoresearch_db_integrity_clean: cold/repeat/update integrity ok.
- autoresearch_no_duplicate_repeat_failure: repeat/update duplicate_edges_upserted=0.
- unresolved_calls_page_under_1s: 68 ms.
- no_cgc_win_claim_on_incomplete: no CGC rerun, no win claimed.

## Final Answer

Final verdict: fail. Correctness and DB durability are now good enough to measure honestly, but the app does not meet the intended real large-codebase performance standard.
