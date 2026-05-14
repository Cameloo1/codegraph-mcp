# Context Packet Gate

Verdict: `passed`

Cases: 11 total, 11 passed, 0 failed. Top-k: 8. Token budget: 1800.

## Aggregate Metrics

| Metric | Value |
| --- | ---: |
| Context symbol recall@8 | 1.000 |
| Critical symbol missing rate | 0.000 |
| Distractor ratio | 0.000 |
| Proof path coverage | 1.000 |
| Source span coverage | 1.000 |
| Critical snippet coverage | 1.000 |
| Recommended test recall | 1.000 |
| Useful facts per byte | 0.001 |
| Useful facts per estimated token | 0.003 |

## Case Results

| Case | Status | Symbol R@k | Critical Missing | Distractor Ratio | Proof Paths | Stored Paths | Span Coverage | Snippets | Tests | Useful/byte |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `admin_user_middleware_role_separation` | `passed` | 1.000 | 0 | 0.000 | 2/2 | 38 | 1.000 | 2/2 | 0/0 | 0.002 |
| `barrel_export_default_export_resolution` | `passed` | 1.000 | 0 | 0.000 | 2/2 | 19 | 1.000 | 2/2 | 0/0 | 0.001 |
| `derived_closure_edge_requires_provenance` | `passed` | 1.000 | 0 | 0.000 | 1/1 | 18 | 1.000 | 2/2 | 0/0 | 0.001 |
| `dynamic_import_marked_heuristic` | `passed` | 1.000 | 0 | 0.000 | 0/0 | 10 | unknown | 1/1 | 0/0 | 0.000 |
| `file_rename_prunes_old_path` | `passed` | 1.000 | 0 | 0.000 | 1/1 | 10 | 1.000 | 1/1 | 0/0 | 0.000 |
| `import_alias_change_updates_target` | `passed` | 1.000 | 0 | 0.000 | 1/1 | 13 | 1.000 | 2/2 | 0/0 | 0.001 |
| `same_function_name_only_one_imported` | `passed` | 1.000 | 0 | 0.000 | 1/1 | 24 | 1.000 | 1/1 | 0/0 | 0.000 |
| `sanitizer_exists_but_not_on_flow` | `passed` | 1.000 | 0 | 0.000 | 0/0 | 24 | unknown | 1/1 | 0/0 | 0.000 |
| `source_span_exact_callsite` | `passed` | 1.000 | 0 | 0.000 | 1/1 | 13 | 1.000 | 1/1 | 0/0 | 0.001 |
| `stale_graph_cache_after_edit_delete` | `passed` | 1.000 | 0 | 0.000 | 1/1 | 8 | 1.000 | 1/1 | 0/0 | 0.001 |
| `test_mock_not_production_call` | `passed` | 1.000 | 0 | 0.000 | 1/1 | 32 | 1.000 | 3/3 | 1/1 | 0.001 |

## Top Failures


## Missing Critical Symbols


## Missing Proof Paths


## Distractor Problems


## Notes

- Context Packet Gate stages each graph-truth fixture, applies mutation_steps, builds packets from graph-truth-aware critical seeds plus prompt seeds, then checks critical symbols, proof paths, source spans, snippets, tests, path context labels, and distractors.
- This gate scores packet usefulness, not just graph indexing success or command execution.
