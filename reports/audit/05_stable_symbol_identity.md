# 05 Stable Symbol Identity Audit

Source of truth: `MVP.md`, especially section 6 on stable entity IDs and section 7 on edge metadata. The MVP examples describe deterministic IDs that include repo-relative path plus semantic identity, while entities must retain source span, file/content hash, extractor name, confidence, and metadata.

## Verdict

**Partially reliable.** Current IDs are deterministic and generally collision-resistant for syntax entities within one repository path, but they are not yet update-safe semantic identities. Edits that move source spans can change IDs, same-root paths in different repositories can hash to the same entity IDs if merged, duplicate source files lose duplicate graph facts, rename identity is not preserved or mapped, and cross-file import aliases are not bound to exported target entities by the default indexer.

## Code Inspected

| Area | Files and locations | Finding |
| --- | --- | --- |
| Core ID generation | `crates/codegraph-core/src/ids.rs:26`, `crates/codegraph-core/src/ids.rs:38`, `crates/codegraph-core/src/ids.rs:74` | Entity IDs hash normalized repo-relative path plus semantic identity using two FNV64 passes. Repo identity is not part of the digest. |
| Parser entity identity | `crates/codegraph-parser/src/lib.rs:3515` | Basic parser entity IDs include kind, qualified name, and a signature string that embeds source span lines/columns for non-static entities. |
| Static reference identity | `crates/codegraph-parser/src/lib.rs:3387` | Unresolved/static reference entities use static-reference identity and lower confidence, but still become graph entities. |
| Local symbol resolver | `crates/codegraph-parser/src/lib.rs:3358`, `crates/codegraph-parser/src/lib.rs:3373` | Resolution walks lexical scope parents only. It does not prove cross-file import aliases in the default extraction path. |
| Optional TypeScript resolver | `crates/codegraph-parser/src/lib.rs:326`, `crates/codegraph-parser/src/lib.rs:4737`, `crates/codegraph-parser/tools/typescript-resolver.mjs` | Resolver hook exists and has an ignored alias/barrel proof test, but default indexing does not integrate its results into graph edges/entities. |
| File hash | `crates/codegraph-parser/src/lib.rs:4292` | File/content hash is FNV64 over source text. It is stored on records, but not part of normal declaration entity IDs. |
| Index state | `crates/codegraph-index/src/lib.rs:469` | `repo_id` is derived from absolute repo root display, and `repo_commit` is currently `None`. |

## ID Component Coverage

| MVP-relevant component | Current behavior | Audit result |
| --- | --- | --- |
| Repo identity | Stored in `repo_index_state`, not entity ID digest. | Missing from entity identity. |
| Normalized path | Included in entity ID input through `stable_entity_id_for_kind`. | Present. |
| Language | Stored on file record, not in entity ID. | Missing from entity identity. |
| Symbol kind | Included through `EntityKind::id_prefix()`. | Present. |
| Qualified name | Included. | Present. |
| Signature/arity | No semantic arity/signature. Parser uses qualified name plus span for most entities. | Partial. |
| Source span | Included in parser signature, which helps distinguish local duplicates but harms edit stability. | Present, risky. |
| Declaration fingerprint | Not a stable semantic fingerprint. | Missing. |
| Content/file hash | Stored on entities/files but not used for normal declaration IDs. | Stored, not identity-defining. |
| Generated/test/mock context | Inferred from path/kind in some cases, not identity schema. | Partial/missing. |

## Behavior Matrix

| Scenario | Current behavior | Trust level |
| --- | --- | --- |
| Same function name in two files | Distinct IDs because repo-relative path is included. | Good. |
| Same method name in two classes | Distinct IDs because qualified name includes class scope and span. | Good. |
| Class method vs standalone function | Distinct through kind and qualified name. | Good. |
| Default export vs named export | Distinct syntax entities and function declarations. | Partial: export target binding is not semantic proof. |
| Import alias | Parser records import syntax and local unresolved calls; default indexer does not bind alias to exported target. | Not reliable. |
| Barrel export/re-export | Optional TypeScript resolver has an ignored proof test; default graph path does not use it. | Not reliable by default. |
| File edit | Changed file path is deleted and reinserted, but declaration IDs can change when spans move. | Partial. |
| File rename | Full reindex deletes old path and inserts new path. No old/new semantic identity mapping. | Not update-safe. |
| Duplicate file same content | Two file records are kept, but duplicate graph facts are suppressed for the duplicate path. | Partial and risky. |
| Deleted file | Known deleted path removes stale files, entities, spans, and connected edges. | Good when delete path is observed. |
| Generated code | No first-class generated-code identity/context field found. | Unknown/partial. |
| Tests/mocks | Path-separated identities prevent overwrite, but test/mock context is not enforced in identity or proof paths. | Partial. |

## Tests Added

Audit tests were added in `crates/codegraph-index/src/lib.rs`:

| Test | Line | Status | Covers |
| --- | ---: | --- | --- |
| `audit_same_function_name_in_two_files_has_distinct_entity_ids` | 1446 | Pass | Same function name in two files. |
| `audit_same_method_name_in_two_classes_has_distinct_entity_ids` | 1473 | Pass | Same method name in two classes. |
| `audit_default_export_and_named_export_are_distinct_syntax_entities` | 1501 | Pass | Default and named export syntax stays distinct. |
| `audit_import_alias_points_to_export_target` | 1543 | Ignored | Desired cross-file alias target proof, currently unsupported. |
| `audit_duplicate_file_content_keeps_separate_file_identity` | 1579 | Pass | Duplicate content keeps distinct file records. |
| `audit_rename_same_content_preserves_semantic_identity_or_records_mapping` | 1607 | Ignored | Desired rename identity preservation or old/new mapping, currently unsupported. |
| `audit_deleted_file_removes_stale_entities_and_edges` | 1647 | Pass | Deleted path removes stale facts. |
| `audit_test_mock_symbol_does_not_overwrite_production_symbol` | 1681 | Pass | Test mock name does not overwrite production symbol. |

The ignored tests are intentional audit markers, not hidden failures. They document required MVP-level behavior that is not implemented yet.

## Highest-Risk Collision and Stability Modes

1. **Cross-repo collision risk:** entity IDs do not include repo identity, so two repositories with the same path and semantic identity can produce the same `repo://e/...` IDs if combined.
2. **Edit instability:** parser declaration signatures include source span coordinates. Inserting lines above a declaration can change IDs even when the semantic declaration did not change.
3. **Rename instability:** path is identity-defining, but no rename map records that `src/old.ts#foo` became `src/new.ts#foo`.
4. **Duplicate-content under-representation:** duplicate files retain file records but not duplicate graph facts, so symbol identity for duplicate paths is incomplete.
5. **Alias target ambiguity:** cross-file import aliases, default exports, and barrel exports are not graph-proof by default. Calls through aliases can remain unresolved static references.
6. **Test/mock proof leakage risk:** test/mock facts are path-separated but not enforced as a production/test/mock context dimension.

## Required Fixes Before Storage Optimization

1. Define an entity identity schema that includes repo identity or bundle identity, normalized path, language, kind, semantic qualified name, and optional stable declaration fingerprint.
2. Stop using raw source span as the primary declaration signature for update-safe semantic entities; keep span as evidence metadata.
3. Add a rename/identity mapping table or manifest event record for same-hash renames.
4. Wire compiler/LSP resolver output into import/export/call target facts with exactness and provenance, or mark unresolved aliases as non-proof.
5. Preserve duplicate path identity while deduping only source payload or extraction work that can be safely projected back to each path.
6. Add first-class generated/test/mock/prod context metadata before proof-path filtering.

## Bottom Line

The current identity scheme is deterministic and practical for a compact first graph, but it is not yet the MVP-grade stable semantic identity layer. Do not optimize away dictionaries, file records, spans, or indexes until rename mapping, alias proof, and repo-aware identity invariants are added and benchmarked.
