# Context Packet Gate

Verdict: `failed`

Cases: 11 total, 0 passed, 11 failed. Top-k: 5. Token budget: 2000.

## Aggregate Metrics

| Metric | Value |
| --- | ---: |
| Context symbol recall@5 | 0.200 |
| Critical symbol missing rate | 0.800 |
| Distractor ratio | 0.086 |
| Proof path coverage | 0.000 |
| Source span coverage | unknown |
| Critical snippet coverage | 0.176 |
| Recommended test recall | 0.000 |
| Useful facts per byte | 0.000 |
| Useful facts per estimated token | 0.001 |

## Case Results

| Case | Status | Symbol R@k | Critical Missing | Distractor Ratio | Proof Paths | Span Coverage | Snippets | Tests | Useful/byte |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `admin_user_middleware_role_separation` | `failed` | 0.000 | 2 | 0.000 | 0/2 | unknown | 0/2 | 0/0 | 0.000 |
| `barrel_export_default_export_resolution` | `failed` | 0.000 | 2 | 0.000 | 0/2 | unknown | 0/2 | 0/0 | 0.000 |
| `derived_closure_edge_requires_provenance` | `failed` | 0.500 | 1 | 0.000 | 0/1 | unknown | 1/2 | 0/0 | 0.000 |
| `dynamic_import_marked_heuristic` | `failed` | 0.000 | 1 | 0.000 | 0/0 | unknown | 1/1 | 0/0 | 0.000 |
| `file_rename_prunes_old_path` | `failed` | 0.000 | 1 | 0.167 | 0/1 | unknown | 1/1 | 0/0 | 0.000 |
| `import_alias_change_updates_target` | `failed` | 0.000 | 1 | 0.000 | 0/1 | unknown | 0/2 | 0/0 | 0.000 |
| `same_function_name_only_one_imported` | `failed` | 1.000 | 0 | 0.286 | 0/1 | unknown | 0/1 | 0/0 | 0.000 |
| `sanitizer_exists_but_not_on_flow` | `failed` | 1.000 | 0 | 0.000 | 0/0 | unknown | 0/1 | 0/0 | 0.000 |
| `source_span_exact_callsite` | `failed` | 0.000 | 1 | 0.000 | 0/1 | unknown | 0/1 | 0/0 | 0.000 |
| `stale_graph_cache_after_edit_delete` | `failed` | 0.000 | 1 | 0.000 | 0/1 | unknown | 0/1 | 0/0 | 0.000 |
| `test_mock_not_production_call` | `failed` | 0.000 | 2 | 0.000 | 0/1 | unknown | 0/3 | 0/1 | 0.000 |

## Top Failures

- `admin_user_middleware_role_separation`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: role:admin
- `admin_user_middleware_role_separation`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: role:user
- `admin_user_middleware_role_separation`: `missing_expected_proof_path` - expected proof path missing from context packet: path://roles/admin
- `admin_user_middleware_role_separation`: `missing_expected_proof_path` - expected proof path missing from context packet: path://roles/user
- `admin_user_middleware_role_separation`: `critical_snippet_missing` - critical snippet missing for src/auth.ts:2-2
- `admin_user_middleware_role_separation`: `critical_snippet_missing` - critical snippet missing for src/auth.ts:6-6
- `admin_user_middleware_role_separation`: `risk_summary_missing` - context packet has no risk summary for a risk-bearing graph-truth case
- `barrel_export_default_export_resolution`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: src/defaultFeature.default
- `barrel_export_default_export_resolution`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: src/namedFeature.feature
- `barrel_export_default_export_resolution`: `missing_expected_proof_path` - expected proof path missing from context packet: path://barrel/default
- `barrel_export_default_export_resolution`: `missing_expected_proof_path` - expected proof path missing from context packet: path://barrel/named
- `barrel_export_default_export_resolution`: `critical_snippet_missing` - critical snippet missing for src/use.ts:4-4
- `barrel_export_default_export_resolution`: `critical_snippet_missing` - critical snippet missing for src/use.ts:8-8
- `derived_closure_edge_requires_provenance`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: src/store.ordersTable
- `derived_closure_edge_requires_provenance`: `missing_expected_proof_path` - expected proof path missing from context packet: path://derived/provenance
- `derived_closure_edge_requires_provenance`: `critical_snippet_missing` - critical snippet missing for src/service.ts:4-4
- `derived_closure_edge_requires_provenance`: `risk_summary_missing` - context packet has no risk summary for a risk-bearing graph-truth case
- `dynamic_import_marked_heuristic`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: dynamic_import:./plugins/+name
- `file_rename_prunes_old_path`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: src/newName.renamedTarget
- `file_rename_prunes_old_path`: `forbidden_context_symbol` - forbidden context symbol appeared in packet: src/oldName.renamedTarget
- `file_rename_prunes_old_path`: `too_many_distractors` - too many context distractors: observed 1, allowed 0
- `file_rename_prunes_old_path`: `missing_expected_proof_path` - expected proof path missing from context packet: path://rename/run-to-new
- `import_alias_change_updates_target`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: src/beta.betaTarget
- `import_alias_change_updates_target`: `missing_expected_proof_path` - expected proof path missing from context packet: path://alias/run-to-beta
- `import_alias_change_updates_target`: `critical_snippet_missing` - critical snippet missing for src/use.ts:4-4
- `import_alias_change_updates_target`: `critical_snippet_missing` - critical snippet missing for src/use.ts:1-1
- `same_function_name_only_one_imported`: `forbidden_context_symbol` - forbidden context symbol appeared in packet: src/b.chooseUser
- `same_function_name_only_one_imported`: `too_many_distractors` - too many context distractors: observed 2, allowed 0
- `same_function_name_only_one_imported`: `missing_expected_proof_path` - expected proof path missing from context packet: path://same-function/handler-to-a
- `same_function_name_only_one_imported`: `critical_snippet_missing` - critical snippet missing for src/main.ts:6-6
- `sanitizer_exists_but_not_on_flow`: `critical_snippet_missing` - critical snippet missing for src/register.ts:6-6
- `sanitizer_exists_but_not_on_flow`: `risk_summary_missing` - context packet has no risk summary for a risk-bearing graph-truth case
- `source_span_exact_callsite`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: src/actions.second
- `source_span_exact_callsite`: `missing_expected_proof_path` - expected proof path missing from context packet: path://span/run-to-second
- `source_span_exact_callsite`: `critical_snippet_missing` - critical snippet missing for src/main.ts:6-6
- `stale_graph_cache_after_edit_delete`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: src/live.liveTarget
- `stale_graph_cache_after_edit_delete`: `missing_expected_proof_path` - expected proof path missing from context packet: path://stale/run-live
- `stale_graph_cache_after_edit_delete`: `critical_snippet_missing` - critical snippet missing for src/use.ts:4-4
- `test_mock_not_production_call`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: src/checkout.checkout
- `test_mock_not_production_call`: `critical_context_symbol_missing` - critical context symbol missing from packet top 5: tests/checkout.test.uses a test double for chargeCard

## Missing Critical Symbols

- `admin_user_middleware_role_separation`: critical context symbol missing from packet top 5: role:admin
- `admin_user_middleware_role_separation`: critical context symbol missing from packet top 5: role:user
- `barrel_export_default_export_resolution`: critical context symbol missing from packet top 5: src/defaultFeature.default
- `barrel_export_default_export_resolution`: critical context symbol missing from packet top 5: src/namedFeature.feature
- `derived_closure_edge_requires_provenance`: critical context symbol missing from packet top 5: src/store.ordersTable
- `dynamic_import_marked_heuristic`: critical context symbol missing from packet top 5: dynamic_import:./plugins/+name
- `file_rename_prunes_old_path`: critical context symbol missing from packet top 5: src/newName.renamedTarget
- `import_alias_change_updates_target`: critical context symbol missing from packet top 5: src/beta.betaTarget
- `source_span_exact_callsite`: critical context symbol missing from packet top 5: src/actions.second
- `stale_graph_cache_after_edit_delete`: critical context symbol missing from packet top 5: src/live.liveTarget
- `test_mock_not_production_call`: critical context symbol missing from packet top 5: src/checkout.checkout
- `test_mock_not_production_call`: critical context symbol missing from packet top 5: tests/checkout.test.uses a test double for chargeCard

## Missing Proof Paths

- `admin_user_middleware_role_separation`: expected proof path missing from context packet: path://roles/admin
- `admin_user_middleware_role_separation`: expected proof path missing from context packet: path://roles/user
- `barrel_export_default_export_resolution`: expected proof path missing from context packet: path://barrel/default
- `barrel_export_default_export_resolution`: expected proof path missing from context packet: path://barrel/named
- `derived_closure_edge_requires_provenance`: expected proof path missing from context packet: path://derived/provenance
- `file_rename_prunes_old_path`: expected proof path missing from context packet: path://rename/run-to-new
- `import_alias_change_updates_target`: expected proof path missing from context packet: path://alias/run-to-beta
- `same_function_name_only_one_imported`: expected proof path missing from context packet: path://same-function/handler-to-a
- `source_span_exact_callsite`: expected proof path missing from context packet: path://span/run-to-second
- `stale_graph_cache_after_edit_delete`: expected proof path missing from context packet: path://stale/run-live
- `test_mock_not_production_call`: expected proof path missing from context packet: path://mock/test-impact

## Distractor Problems

- `file_rename_prunes_old_path`: forbidden context symbol appeared in packet: src/oldName.renamedTarget
- `file_rename_prunes_old_path`: too many context distractors: observed 1, allowed 0
- `same_function_name_only_one_imported`: forbidden context symbol appeared in packet: src/b.chooseUser
- `same_function_name_only_one_imported`: too many context distractors: observed 2, allowed 0

## Notes

- Context Packet Gate builds packets from task-prompt-derived seeds, then checks critical symbols, proof paths, source spans, snippets, tests, path context labels, and distractors.
- This gate scores packet usefulness, not just graph indexing success or command execution.
