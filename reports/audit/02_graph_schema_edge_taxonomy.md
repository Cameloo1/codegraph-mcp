# 02 Graph Schema and Edge Taxonomy Audit

Verdict: graph facts are not proof-grade as a whole.

The model has many of the MVP fields needed for proof-grade facts, but the current pipeline does not consistently preserve or enforce them. Exactness and confidence are present, derived closure objects exist, and heuristic calls are often downgraded. However, production indexing can discard edge metadata, test/mock/prod context is not first-class, inverse facts are stored as ordinary base edges, and query traversal does not enforce proof-grade-only paths.

## Sources Inspected

- `MVP.md`
- `crates/codegraph-core/src/kinds.rs`
- `crates/codegraph-core/src/model.rs`
- `crates/codegraph-core/src/validation.rs`
- `crates/codegraph-store/src/traits.rs`
- `crates/codegraph-store/src/sqlite.rs`
- `crates/codegraph-parser/src/lib.rs`
- `crates/codegraph-index/src/lib.rs`
- `crates/codegraph-query/src/lib.rs`
- `crates/codegraph-cli/src/lib.rs`
- `crates/codegraph-mcp-server/src/lib.rs`
- `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/status.json`
- `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/dbstat.json`

## Current Edge Metadata Fields

The Rust model in `crates/codegraph-core/src/model.rs` defines `Edge` with:

- `id`
- `head_id`
- `relation`
- `tail_id`
- `source_span`
- `repo_commit`
- `file_hash`
- `extractor`
- `confidence`
- `exactness`
- `derived`
- `provenance_edges`
- `metadata`

The SQLite `edges` table in `crates/codegraph-store/src/sqlite.rs` stores these fields mostly in compact/dictionary form:

- head, relation, tail
- source location as `span_path_id`, start/end line/column
- `repo_commit`
- `file_hash`
- `extractor_id`
- `confidence`
- `exactness_id`
- `derived`
- `provenance_edges_json`
- `metadata_json`

Important gap: `insert_edge_after_file_delete` writes `provenance_edges_json` as `[]` and `metadata_json` as `{}`. That is the indexing path used by `crates/codegraph-index/src/lib.rs` for normal repo indexing, so heuristic pattern/framework metadata can be lost even though the model and general `upsert_edge` path support it.

## Relation Inventory

| Category | Current relations |
| --- | --- |
| Structural/ownership | `CONTAINS`, `DEFINED_IN`, `DEFINES`, `DECLARES`, `EXPORTS`, `IMPORTS`, `REEXPORTS`, `BELONGS_TO`, `CONFIGURES` |
| Type/object model | `TYPE_OF`, `RETURNS`, `IMPLEMENTS`, `EXTENDS`, `OVERRIDES`, `INSTANTIATES`, `INJECTS`, `ALIASED_BY`, `ALIAS_OF` |
| Execution | `CALLS`, `CALLED_BY`, `CALLEE`, `ARGUMENT_0`, `ARGUMENT_1`, `ARGUMENT_N`, `RETURNS_TO`, `SPAWNS`, `AWAITS`, `LISTENS_TO` |
| Data/mutation | `READS`, `WRITES`, `MUTATES`, `MUTATED_BY`, `FLOWS_TO`, `REACHING_DEF`, `ASSIGNED_FROM`, `CONTROL_DEPENDS_ON`, `DATA_DEPENDS_ON` |
| Security/auth/validation | `AUTHORIZES`, `CHECKS_ROLE`, `CHECKS_PERMISSION`, `SANITIZES`, `VALIDATES`, `EXPOSES`, `TRUST_BOUNDARY`, `SOURCE_OF_TAINT`, `SINKS_TO` |
| Async/event/messaging | `PUBLISHES`, `EMITS`, `CONSUMES`, `LISTENS_TO`, `SUBSCRIBES_TO`, `HANDLES` |
| Persistence/schema | `MIGRATES`, `READS_TABLE`, `WRITES_TABLE`, `ALTERS_COLUMN`, `DEPENDS_ON_SCHEMA` |
| Testing | `TESTS`, `ASSERTS`, `MOCKS`, `STUBS`, `COVERS`, `FIXTURES_FOR` |
| Derived/cache shortcuts | `MAY_MUTATE`, `MAY_READ`, `API_REACHES`, `ASYNC_REACHES`, `SCHEMA_IMPACT` |

All explicitly requested MVP relations exist in `RelationKind`: `CALLS`, `READS`, `WRITES`, `FLOWS_TO`, `IMPLEMENTS`, `TESTS`, `AUTHORIZES`, `CHECKS_ROLE`, `SANITIZES`, `EXPOSES`, `INJECTS`, `INSTANTIATES`, `EXTENDS`, `PUBLISHES`, `EMITS`, `CONSUMES`, `LISTENS_TO`, `SPAWNS`, `AWAITS`, `MUTATES`, `MIGRATES`, `ALIASED_BY`, `MOCKS`, `STUBS`, and `ASSERTS`.

Existence in the enum is not the same as extraction coverage. JS/TS extractor capability lists do not include several type/object-model MVP relations such as `IMPLEMENTS`, `EXTENDS`, `INSTANTIATES`, and `INJECTS`. The optional TypeScript compiler resolver lists `ALIAS_OF`, `ALIASED_BY`, `CALLS`, `IMPORTS`, and `EXPORTS`, but it is optional. The latest Autoresearch relation counts also did not show several MVP relations being produced, including `IMPLEMENTS`, `AUTHORIZES`, `EXPOSES`, `INJECTS`, `INSTANTIATES`, `EXTENDS`, `PUBLISHES`, and `ALIASED_BY`.

## Exact Code Locations for Edge Insertion

| Surface | Location | Notes |
| --- | --- | --- |
| Generic parser edges | `crates/codegraph-parser/src/lib.rs`, `GenericLanguageExtractor::push_edge`, `push_edge_with` | Emits parser-verified and static-heuristic generic language facts. |
| TS/JS parser edges | `crates/codegraph-parser/src/lib.rs`, `BasicExtractor::push_edge`, `push_edge_with`, `push_extended_edge`, `push_edge_with_extractor` | Emits core parser facts and extended heuristic framework facts. |
| Unresolved call/read placeholders | `crates/codegraph-parser/src/lib.rs`, `callee_entity`, `resolve_or_reference_symbol`, `push_reference_entity` | Creates static-heuristic unresolved entities/edges with lower confidence. |
| Test/mock/stub/assert edges | `crates/codegraph-parser/src/lib.rs`, `extract_test_call` | Emits heuristic `TESTS`, `ASSERTS`, `MOCKS`, `STUBS`, `COVERS`, `FIXTURES_FOR`. |
| Index persistence | `crates/codegraph-index/src/lib.rs`, edge loop using `insert_edge_after_file_delete` | Persists compact edge rows during normal indexing. |
| SQLite compact insert | `crates/codegraph-store/src/sqlite.rs`, `insert_edge_after_file_delete` | Preserves exactness/confidence but drops edge metadata/provenance. |
| SQLite full upsert | `crates/codegraph-store/src/sqlite.rs`, `upsert_edge` | Preserves edge metadata/provenance. Used by some import/test paths, not the hot indexing path. |
| Derived closure generation | `crates/codegraph-query/src/lib.rs`, `derive_closure_edges` | Builds `DerivedClosureEdge` objects from path provenance. |
| Context packet derived metadata | `crates/codegraph-query/src/lib.rs`, `context_packet_from_paths` | Adds derived edge summaries into packet metadata, but does not persist them as normal index edges. |
| Bundle import | `crates/codegraph-cli/src/lib.rs`, bundle import transaction | Uses full `upsert_edge` and preserves metadata when importing bundle edges. |

## Base, Derived, Heuristic, Callsite, and Test/Mock Separation

| Fact class | Current separation | Risk |
| --- | --- | --- |
| Exact/base facts | Partially present through `exactness`, `extractor`, `confidence`, `derived=false` | `parser_verified` is syntax-level, not necessarily semantic proof. |
| Heuristic facts | Present through `Exactness::StaticHeuristic`, confidence, and metadata in memory | Normal index insertion drops metadata such as `heuristic`, `pattern`, `framework`, and `resolution`. |
| Callsite facts | Represented through `CallSite`, `CALLEE`, `ARGUMENT_*`, and `CALLS` | Callsite vs direct function relation is not enforced by path queries; consumers can conflate them. |
| Derived/cache facts | `DerivedClosureEdge` type/table and `Exactness::DerivedFromVerifiedEdges` exist | Derived closures are generated on demand and not consistently persisted; derived shortcut relations share `RelationKind` with base relations. |
| Test/mock facts | Relations and entity kinds exist | No first-class `context = test/mock/prod`; query traversal can include test/mock paths unless callers filter by relation/kind. |
| Repo snapshot facts | `repo_commit` and `file_hash` exist | `repo_commit` is optional and often `None`; no required repo snapshot ID on each edge. |

## Inverse and Closure Edges

Inverse or dual facts are currently stored as ordinary base facts when emitted. For example, parser code emits both `CONTAINS` and `DEFINED_IN`, and return/callsite-related dual facts such as `RETURNS` and `RETURNS_TO`. These edges are `derived=false`; the schema does not mark them as inverse-derived from another edge.

`CALLED_BY` and `MUTATED_BY` exist in the enum but were not found as regular extractor outputs in the inspected code. Query traversal usually uses reverse traversal over base edges rather than storing every inverse edge.

Closure/transitive shortcuts are not stored as base facts by the normal indexer. `codegraph-query` creates `DerivedClosureEdge` values such as `MAY_MUTATE`, `MAY_READ`, `API_REACHES`, `ASYNC_REACHES`, and `SCHEMA_IMPACT` from path provenance. That is directionally aligned with `MVP.md`, but the persistence/query contract is incomplete.

## Can Unresolved Textual Matches Be Mislabeled Exact?

Unresolved calls are usually downgraded to `static_heuristic` with lower confidence. That is good.

The remaining risk is that `parser_verified` may be interpreted by downstream consumers as proof-grade exactness. Same-scope parser resolution can prove syntax-local identity, but it cannot prove cross-file imports, dynamic dispatch, aliasing, or framework behavior. The query engine penalizes uncertainty but does not block static-heuristic or parser-only paths from final result sets.

## Can Test/Mock Relations Enter Production Proof Paths?

Yes. The graph has test relations and test entity kinds, but there is no first-class production/test/mock context field on `Edge`. The query engine traverses relations requested by each API and preserves exactness/confidence, but it does not enforce a global invariant that production proof paths exclude `TESTS`, `MOCKS`, `STUBS`, fixtures, or test-file entities. A caller can avoid test relations by choosing relation filters, but the schema does not make contamination impossible.

## Missing Metadata Fields and Invariants

The next schema phase should add or enforce:

- `fact_class`: `base`, `callsite`, `heuristic`, `derived_cache`, `inverse`, `dynamic_trace`.
- `runtime_context`: `prod`, `test`, `mock`, `fixture`, `unknown`.
- `proof_grade`: boolean or enum derived from exactness, extractor, confidence, provenance, and context.
- Required `repo_snapshot_id` or required `repo_commit`/tree hash for every edge.
- Required `extractor_version` and language frontend/resolver version.
- Required `resolver_mode`: parser, compiler, LSP, heuristic, dynamic.
- Required provenance for `derived=true`, inverse edges, and closure shortcuts.
- Required `source_span_id` or normalized span reference, not just inline coordinates.
- Negative provenance markers for unresolved placeholders.
- A query invariant that proof paths cannot silently mix production, test, mock, and heuristic facts.
- A storage invariant that compact insert paths preserve metadata/provenance or intentionally reject facts that need it.

## Are the 2M Edges Explainable or Suspicious?

Both. The latest Autoresearch counts are explainable as a broad syntax graph:

- `CONTAINS`, `DEFINED_IN`, `DECLARES`, `CALLS`, `CALLEE`, `ARGUMENT_0`, and `FLOWS_TO` dominate the graph.
- 4975 files produced 865896 entities, 2050123 edges, and 2916019 source spans.
- `dbstat` shows `edges` as the largest table by bytes, followed by qualified-name and object-id dictionaries/indexes.

They are suspicious as proof-grade intelligence because many edges are parser-derived or heuristic, edge metadata is not consistently retained in compact indexing, and the current schema does not force test/mock/prod or base/derived/inverse separation. The edge count should be treated as raw graph volume, not proof volume.

## Highest-Risk Edge Categories

1. Heuristic framework edges: auth, events, persistence, and tests are useful but static-heuristic and can look authoritative without metadata.
2. Parser-verified calls: local syntax resolution is not compiler/LSP proof, especially for imports, aliases, method dispatch, and dynamic frameworks.
3. Test/mock/stub edges: useful for test impact but risky if included in production proof paths.
4. Derived closure shortcuts: useful if provenance is enforced, risky if later persisted or scored as base facts.
5. Inverse/dual structural edges: useful for traversal, but currently not marked as inverse-derived.

## Recommended Schema Changes

- Preserve `metadata_json` and `provenance_edges_json` in the compact indexing path, or split compact storage into explicit lossy/lossless modes.
- Add `fact_class`, `runtime_context`, `proof_grade`, `extractor_version`, and required `repo_snapshot_id`.
- Add database checks or validation tests: derived facts must have provenance, heuristic facts must have an extractor and reason, test/mock facts must carry runtime context, and proof paths must declare whether heuristic/test/mock edges are allowed.
- Store inverse edges either as derived facts with provenance or generate them at query time.
- Keep closure/transitive relations out of base `edges`; persist them only in `derived_edges` with base-edge provenance.
- Add query APIs for `proof_only=true`, `allow_heuristic=false`, and `exclude_contexts=["test","mock","fixture"]`.
- Add benchmark assertions that fail when static-heuristic unresolved calls are labeled exact or when test/mock paths satisfy production proof tasks.
