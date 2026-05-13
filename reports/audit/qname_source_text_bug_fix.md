# Qname Source Text Bug Fix

Generated: 2026-05-11 23:43:11 -05:00

## Verdict

Status: **passed**

The reported identity bug was real. The parser was building a taint-source entity name from collapsed argument source text at `crates/codegraph-parser/src/lib.rs` around the reported line 2507:

```text
taint:{collapse_whitespace(argument_text)}
```

That value then flowed into entity `name`, `qualified_name`, qname prefix/suffix interning, symbol storage, and object-id hashing. A long tainted expression could therefore become identity material instead of remaining source evidence.

## Fix

- Replaced taint-source raw text identity with a compact synthetic identity:
  - `taint_source@<start_byte>-<end_byte>#<fnv64-prefix>`
- Added source-span/debug metadata without copying source text:
  - `identity_material = source_span_and_text_hash`
  - `source_text_hash = fnv64:<hash>`
  - `debug_display = load_source_snippet_from_span`
- Changed source-expression async entities created from `new` and `await` expressions to use the same span/hash identity pattern instead of expression text labels.
- Added store-side identity guards:
  - `MAX_QUALIFIED_NAME_BYTES = 1024`
  - `MAX_QNAME_PREFIX_BYTES = 768`
  - `MAX_SYMBOL_VALUE_BYTES = 512`
- Applied validation to compact entity writes and audit/debug static-reference sidecar writes.

## Ingress Audit

| Field | Raw source risk | Fix or guard |
| --- | --- | --- |
| `qualified_name` | Taint-source argument text was unbounded and raw. | Fixed to span/hash synthetic ID. |
| `qname_prefix` | Derived from `qualified_name`; inherited the taint-source bug. | Fixed upstream and capped in storage. |
| `object_id` | Entity ID hash could include raw qname identity input. | Fixed by removing raw taint text from qname input. |
| `symbol` | Entity `name` could hold the raw taint text. | Fixed upstream and capped in storage. |
| entity display name | Taint-source display name could hold raw source. | Fixed to synthetic span/hash display; snippet remains span-loadable. |
| dynamic import qname | Short unresolved specifier remains for graph-truth compatibility. | Storage length guard blocks pathological specifier bloat. |
| import/export fallback labels | Bounded statement labels still exist for syntax display. | Storage length guard prevents pathological qname growth; future work can make these fully span/hash based. |

## Four-Question Storage Rule

| Question | Result |
| --- | --- |
| Did graph truth still pass? | Yes. Graph Truth Gate passed 11/11. |
| Did context packet quality still pass? | Yes. Context Packet Gate passed 11/11 with 100% critical symbol recall and 100% proof-path coverage. |
| Did proof DB size decrease? | Not claimed. This was an identity hygiene fix, not a storage optimization. The fresh fixture audit DB was 1,527,808 bytes. Existing large DBs must be rebuilt or migrated to remove already-written bad qnames. |
| Did the removed data move to a sidecar, become derivable, or get proven unnecessary? | Yes. Raw source text moved out of identity and remains available through source spans/snippet loading; identity keeps only byte-span position plus compact text hash. |

## Tests Added

- Parser unit test: long taint/source expression does not appear in entity `name` or `qualified_name`, qname length stays bounded, and the source snippet remains recoverable through the span.
- Store unit test: overlong qname, overlong qname prefix, and overlong sidecar display name are rejected before dictionary/sidecar bloat.

## Verification

| Check | Result |
| --- | --- |
| Focused parser test | Passed |
| Focused store test | Passed |
| Graph Truth Gate | Passed, 11/11 fixtures |
| Context Packet Gate | Passed, 11/11 fixtures |
| Fresh fixture storage audit | Passed, integrity `ok` |
| Full Rust test suite | Passed |

## Artifacts

- `reports/audit/artifacts/qname_source_text_bug_fix_graph_truth.json`
- `reports/audit/artifacts/qname_source_text_bug_fix_graph_truth.md`
- `reports/audit/artifacts/qname_source_text_bug_fix_context_packet.json`
- `reports/audit/artifacts/qname_source_text_bug_fix_context_packet.md`
- `reports/audit/artifacts/qname_source_text_bug_fix_storage.json`
- `reports/audit/artifacts/qname_source_text_bug_fix_storage.md`
- `reports/audit/artifacts/qname_source_text_bug_fix_fixture.sqlite`

## Caveats

- This does not rewrite already-created large artifacts. A clean reindex or a specific legacy qname cleanup migration is required to remove bad qnames from old DBs.
- Import/export statement fallback labels are still bounded source-derived display labels. They are no longer allowed to grow into pathological identity strings, but a later cleanup can convert them to span/hash identities too.
