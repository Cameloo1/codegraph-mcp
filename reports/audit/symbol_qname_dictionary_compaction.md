# Symbol/Qname Dictionary Compaction

Timestamp: `2026-05-12 13:49:03 -05:00`

Verdict: `pass_for_phase`

Global intended-performance verdict remains `fail` until the long Autoresearch compact proof gate is rerun and the remaining cold-build/update targets are fixed.

## What Changed

Compact proof storage no longer stores raw source-derived template identity strings for:

- `CallSite`
- `Expression`
- `ReturnSite`

Those template entities now store a compact proof identity of:

- entity kind
- deterministic `local_template_entity_id`
- `content_template_id`
- file-instance overlay path
- exact source span

Example stored qualified name:

`@template.callsite.1`

Example synthesized external qualified name for a duplicate path:

`src::auth-copy.callsite@1`

The removed display/qname text is not deleted as proof evidence. It is derivable or recoverable from the source span and file path. Human-readable source text remains available by loading the snippet for the stored span.

## Storage Result

Measured on copied proof artifact:

- Before: `reports/audit/artifacts/template_compaction_probe.sqlite`
- After: `reports/audit/artifacts/symbol_qname_compaction_probe_fast.sqlite`
- Integrity: `ok`

| Metric | Before | After | Delta | Status |
| --- | ---: | ---: | ---: | --- |
| Proof DB family | 224.855 MiB | 162.609 MiB | -62.246 MiB | pass |
| Dictionary family | 88.250 MiB | 25.926 MiB | -62.324 MiB | pass |
| Target savings | 25.000 MiB | 62.246 MiB | +37.246 MiB | pass |

## Dictionary Contributors

| Object | Before Rows | After Rows | Before MiB | After MiB |
| --- | ---: | ---: | ---: | ---: |
| `symbol_dict` | 696,373 | 175,239 | 28.984 | 7.633 |
| `idx_symbol_dict_hash` | unknown | unknown | 12.668 | 3.180 |
| `qname_prefix_dict` | 155,741 | 48,207 | 21.242 | 5.832 |
| `idx_qname_prefix_dict_hash` | unknown | unknown | 2.867 | 0.887 |
| `qualified_name_dict` | 727,372 | 274,171 | 11.609 | 4.336 |
| `idx_qualified_name_parts` | unknown | unknown | 10.879 | 4.059 |

## Cardinality Change

| Metric | Delta |
| --- | ---: |
| Source-derived template rows compacted | 471,932 |
| `symbol_dict` rows | -519,815 |
| `symbol_dict` value bytes | -11,680,780 |
| `qname_prefix_dict` rows | -107,138 |
| `qname_prefix_dict` value bytes | -13,392,126 |
| `qualified_name_dict` rows | -451,541 |
| New compact qname keys | 16,112 |

## Storage Rule Answers

1. Graph Truth still passed: yes, `11/11`.
2. Context Packet quality still passed: yes, `11/11`.
3. Proof DB size decreased: yes, copied proof artifact saved `62.246 MiB`.
4. Removed data moved or became derivable: yes, source-derived template display text is recovered from source spans/snippets and compact local template identity, not needed as proof dictionary payload.

## Gate Results

| Gate | Result | Artifact |
| --- | --- | --- |
| Graph Truth Gate | `11/11 passed` | `reports/audit/artifacts/symbol_qname_compaction_graph_truth.md` |
| Context Packet Gate | `11/11 passed` | `reports/audit/artifacts/symbol_qname_compaction_context_packet.md` |
| Relation/source-span sampler | `ok` | `reports/audit/artifacts/symbol_qname_compaction_sample_CALLS.md` |
| Storage audit before | `ok` | `reports/audit/artifacts/symbol_qname_compaction_before_storage.md` |
| Storage audit after | `ok` | `reports/audit/artifacts/symbol_qname_compaction_after_storage.md` |
| Comprehensive benchmark | `fail, reported` | `reports/final/comprehensive_benchmark_symbol_qname_compaction.md` |
| Full workspace tests | `passed` | `cargo test --workspace` |

## Notes

The comprehensive benchmark was regenerated, but it aggregates the latest full compact proof gate artifacts. It still reports the unrelated intended-performance failures until a fresh long Autoresearch proof gate is run.

This change does not compact function, method, class, import/export, role, table, route, sanitizer, or test symbol names. Those remain proof-facing identities.
