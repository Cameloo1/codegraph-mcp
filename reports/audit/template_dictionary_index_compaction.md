# Template/Dictionary Index Compaction

Generated: 2026-05-12 14:10:23 -05:00

## Verdict

One index was proven safe to remove: `idx_files_content_template`.

The larger dictionary indexes were not safe to remove. On copied DB experiments, dropping them converted default lookup paths into table scans and produced clear p95 regressions for symbol/qname lookup. This phase therefore takes the small, evidence-backed win and leaves the active lookup indexes in place.

## Storage Rule Answers

| Rule | Answer |
| --- | --- |
| Did graph truth still pass? | Yes. Graph Truth Gate passed 11/11. |
| Did context packet quality still pass? | Yes. Context Packet Gate passed 11/11. |
| Did proof DB size decrease? | Yes. 170,508,288 bytes to 170,262,528 bytes, saving 245,760 bytes / 0.234 MiB on the copied proof artifact. |
| Did removed data move, become derivable, or prove unnecessary? | The removed object is only a redundant secondary index. File/content-template rows remain stored; default proof queries still use primary-key and reverse-map paths. |

## Copied-DB Experiment Policy

Source DB: `reports/audit/artifacts/symbol_qname_compaction_probe_fast.sqlite`

All index experiments were run on copied DBs only. The source DB was not mutated. The detailed raw experiment output is in `reports/audit/template_dictionary_index_experiments.json` and `reports/audit/template_dictionary_index_experiments.md`.

## Index Decisions

| Index | Table | Bytes | Saved if dropped | Decision | Evidence |
| --- | --- | ---: | ---: | --- | --- |
| `idx_symbol_dict_hash` | `symbol_dict` | 3,334,144 | 3,477,504 | Keep | `symbol_lookup` p95 regressed 0.0059 ms to 4.0570 ms; `qname_lookup` regressed 0.0277 ms to 4.9642 ms. |
| `idx_qname_prefix_dict_hash` | `qname_prefix_dict` | 929,792 | 1,073,152 | Keep | `qname_lookup` p95 regressed 0.0277 ms to 3.7755 ms. |
| `idx_qualified_name_parts` | `qualified_name_dict` | 4,255,744 | 4,399,104 | Keep | `qname_lookup` p95 regressed 0.0277 ms to 8.0735 ms. |
| `idx_files_content_template` | `files` | 102,400 | 245,760 | Drop | No default proof-query budget regression. Content-template helper lookup rose to 0.9938 ms p95 but stayed below 1 ms and is not on the normal proof path. |

No partial/composite replacement beat the existing dictionary hash indexes. The dictionary indexes already represent the compact form needed by default lookup queries.

## Default Query Coverage

The experiment set covered the requested default query surfaces:

| Query surface | Coverage |
| --- | --- |
| Graph truth lookup | Full strict Graph Truth Gate rerun after the schema change. |
| Context packet | Context Packet Gate plus context_pack p95 benchmark. |
| Symbol lookup | `symbol_lookup` p95 and EXPLAIN plans. |
| Qname lookup | `qname_prefix_lookup`, `qname_parts_lookup`, and combined `qname_lookup` p95. |
| Relation lookup | `relation_lookup` p95 and EXPLAIN plan. |
| PathEvidence lookup | `path_evidence_lookup` p95 and EXPLAIN plan. |
| Single-file stale deletion | `single_file_update_stale_lookup` p95 plus update-integrity fixture run. |
| Repeat manifest lookup | `repeat_manifest_lookup` p95 plus repeat unchanged fixture run. |

## Applied Change

Schema version is now 18. `idx_files_content_template` is no longer created for new compact proof DBs, and migrations drop it from older DBs. The store migration test now asserts that this index is absent.

No proof facts, source spans, PathEvidence rows, symbols, qname prefixes, or template rows were removed.

## Post-Change Measurements

| Metric | Result |
| --- | --- |
| Storage audit DB | `reports/audit/artifacts/template_dictionary_index_probe.sqlite` |
| DB integrity | `ok` |
| DB bytes before | 170,508,288 |
| DB bytes after | 170,262,528 |
| Saved bytes | 245,760 |
| Saved MiB | 0.234 |
| Context pack p95 | 148.091 ms process p95 / 9.274 ms profiled p95 |
| Unresolved-calls page p95 | 190.782 ms process p95 |
| Small fixture update times | 41 ms, 38 ms, 40 ms |
| Medium fixture update times | 287 ms, 287 ms, 294 ms |

## Correctness Gates

| Gate | Result |
| --- | --- |
| Graph Truth Gate | Passed, 11/11 |
| Expected entities | 29/29 |
| Expected edges | 20/20 |
| Expected paths | 11/11 |
| Forbidden edges found | 0 |
| Forbidden paths found | 0 |
| Source-span failures | 0 |
| Context Packet Gate | Passed, 11/11 |
| Critical symbol recall | 100% |
| Proof-path coverage | 100% |
| Source-span coverage | 100% |
| Expected tests recall | 100% |
| Distractor ratio | 0% |

## Remaining Work

This phase does not materially move the proof DB target by itself. The high-value storage work remains in table payload and larger mapping/index contributors, especially `template_entities`, `template_edges`, `file_entities`, `path_evidence`, `symbol_dict`, and `qname_prefix_dict`. The comprehensive benchmark remains failed because the aggregate full-gate artifact still carries unrelated proof-size and cold-build failures.

The important outcome here is negative evidence: the large dictionary lookup indexes are doing real work and should not be removed until a replacement lookup design is proven on copied DBs with graph/context gates passing.
