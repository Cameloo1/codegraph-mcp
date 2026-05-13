# Compact Proof Vs Debug Storage Design

Generated: 2026-05-11 20:04:44 -05:00

## Verdict

Design only. No runtime behavior was changed in this phase.

The current Autoresearch artifact is still a debug-scale graph: `820.83 MiB` after structural and dictionary compaction. The next storage boundary must be explicit: a compact proof database for agent-facing evidence, plus optional audit/debug sidecars for diagnostics and manual labeling.

The intended proof-mode target remains `<=250 MiB` on the Autoresearch-scale repo. Audit/debug sidecars may be larger, but they must be separately named, separately measured, and excluded from the compact proof target only when the proof database can pass Graph Truth and Context Packet gates by itself.

## Evidence From Latest Storage Investigation

The latest copied Autoresearch artifact is:

`reports/audit/artifacts/dictionary_index_compaction_autoresearch_sim.sqlite`

Measured size after `VACUUM` / `ANALYZE`: `860,700,672` bytes, or `820.83 MiB`.

Top remaining storage contributors:

| Object | Rows | Bytes | Classification pressure |
| --- | ---: | ---: | --- |
| `structural_relations` | 2,019,607 | 139,460,608 | structural-derived |
| `edges` | 855,108 | 131,022,848 | mixed proof plus heuristic/test/debug |
| `entities` | 1,630,146 | 119,382,016 | compact proof required, but too wide |
| `object_id_dict` | 1,631,103 | 99,954,688 | compact proof required, string form too wide |
| `qname_prefix_dict` | 339,395 | 46,112,768 | compact proof optional / audit helpful |
| `callsites` | 524,760 | 41,451,520 | structural-derived callsite model |
| `symbol_dict` | 789,868 | 35,655,680 | compact proof required |
| `callsite_args` | 466,956 | 33,017,856 | structural-derived callsite model |

The qname compaction succeeded. The remaining problem is row volume and row width:

- `file_hash` is repeated through entity/fact tables instead of being stored once per file snapshot.
- `metadata_json` carries repeated resolver/debug flags in proof-adjacent rows.
- unresolved/static heuristic facts are stored beside proof facts.
- callsites, arguments, and structural facts are persisted as full fact rows.
- object IDs are still stored as human-readable `repo://e/<hash>` strings.
- near-duplicate `upstream` and `runtime` trees are preserved as separate path identities, which is correct, but currently duplicates nearly all row payload.

## Storage Modes

### `proof`

Purpose: default agent-facing graph. It must be small, deterministic, integrity-clean, and sufficient for Graph Truth and Context Packet gates.

Rules:

- Store only proof-grade base facts and provenance-bearing derived facts.
- Exclude heuristic, unresolved, unsupported, debug-only, parser-warning, and broad structural cache facts by default.
- Store exact source spans for proof facts.
- Store PathEvidence needed for context packets.
- Store test/mock facts only as labeled test/proof facts; production proof paths must still reject them by default.
- Store one file/snapshot hash per file, not one copy per entity or fact row.
- Store compact IDs in primary rows; human-readable debug strings come from views or sidecars.

### `audit`

Purpose: relation/source-span audit, manual labeling, correctness forensics, and unsupported-pattern inspection.

Rules:

- Include everything from proof mode.
- Preserve heuristic/unresolved facts, unsupported relation attempts, extractor warnings, and diagnostic metadata in sidecar tables.
- Preserve structural/callsite details that proof mode derives or compresses.
- Do not let audit-only rows satisfy Graph Truth or production Context Packet proof paths.
- Audit commands can sample from proof plus audit sidecar.

### `debug`

Purpose: implementation debugging and reproducibility for parser/reducer/storage development.

Rules:

- Include everything from audit mode.
- Preserve raw LocalFactBundle payloads, resolver traces, retrieval traces, benchmark runs, FTS/debug snippets if enabled, and verbose JSON metadata.
- Debug mode is not a performance target.
- Debug mode must be explicit; it cannot be silently used for intended-performance numbers.

## Physical Layout

Use physically separate database files so proof size is honest:

| Mode | Files | Counts against `<=250 MiB` proof target |
| --- | --- | --- |
| `proof` | `<name>.proof.sqlite` or existing `--db` path when mode is proof | yes |
| `audit` | proof DB plus `<name>.audit.sqlite` sidecar | proof DB only |
| `debug` | proof DB plus audit sidecar plus `<name>.debug.sqlite` sidecar | proof DB only |

The proof DB is the only database used by default MCP/context operations. Audit/debug commands may open sidecars when present and must report when a sidecar is absent.

## Build Behavior

Proposed CLI:

```text
codegraph-mcp index --storage-mode proof --db <path> <repo>
codegraph-mcp index --storage-mode audit --db <path> <repo>
codegraph-mcp index --storage-mode debug --db <path> <repo>
```

Reducer behavior:

1. Parser workers still emit full LocalFactBundle objects.
2. The reducer classifies every fact before writing.
3. The proof writer inserts only proof-eligible facts.
4. The audit writer records rejected or downgraded facts with rejection reason.
5. The debug writer records raw bundles and resolver traces.
6. Graph fact hash is computed from proof facts only unless `--include-audit` is explicitly requested.

Proof eligibility predicate for an edge:

```text
relation is proof/test/derived-cache eligible
AND exactness is exact, parser_verified, compiler_verified, or lsp_verified
AND source span exists and validates
AND edge_class is base_exact, test, mock, reified_callsite, or derived
AND if derived, provenance edge IDs are non-empty and resolvable
AND if context is test/mock, relation is test/proof relation or query mode explicitly allows it
```

Rows that fail the predicate are not deleted. They are routed to audit/debug sidecars when those modes are enabled.

## Current Table Classification

| Table | Classification | Proof mode | Audit mode | Debug mode | Notes |
| --- | --- | --- | --- | --- | --- |
| `object_id_dict` | compact proof required | compact binary/int IDs or minimal string lookup | full lookup allowed | full lookup plus debug view | Current string payload is still too expensive. |
| `path_dict` | compact proof required | include | include | include | Small and needed for spans/files. |
| `symbol_dict` | compact proof required | include | include | include | Needed for names and context. |
| `qname_prefix_dict` | compact proof optional | include only until qnames can be reconstructed from hierarchy | include | include | Target proof should derive more from entity hierarchy. |
| `qualified_name_dict` | compact proof optional | compact prefix/suffix IDs only | include | include | Full `value` remains debug/backcompat only. |
| `entity_kind_dict` | compact proof required | include | include | include | Small enum dictionary. |
| `relation_kind_dict` | compact proof required | include | include | include | Small enum dictionary. |
| `extractor_dict` | compact proof required | include | include | include | Required provenance metadata. |
| `exactness_dict` | compact proof required | include | include | include | Required proof validation metadata. |
| `edge_class_dict` | compact proof required | include | include | include | Required proof contamination checks. |
| `edge_context_dict` | compact proof required | include | include | include | Required production/test/mock checks. |
| `language_dict` | compact proof required | include | include | include | Needed by files/entities. |
| `entities` | compact proof required | include compact columns only | include extra metadata | include verbose metadata | Remove repeated `file_hash`; store file/snapshot once. |
| `edges` | compact proof required | proof-eligible edges only | proof plus audit edge refs | proof plus audit refs | Heuristic/unresolved rows must leave this table in proof mode. |
| `structural_relations` | structural-derived | omit when derivable from entity attributes; otherwise compact side table | include | include | `CONTAINS`, `DEFINED_IN`, `DECLARES` should not be proof edges. |
| `callsites` | structural-derived | only compact rows needed by proof PathEvidence | include | include | Do not create full debug callsite graph by default. |
| `callsite_args` | structural-derived | only argument rows needed by proof flow evidence | include | include | High-cardinality argument rows belong outside proof unless used. |
| `source_spans` | compact proof required | include normalized proof spans | include all extracted spans | include all extracted spans | Current artifact stores many spans inline; proof target should normalize. |
| `files` | compact proof required | include | include | include | Owns `file_hash`, size, language, indexed state. |
| `repo_index_state` | compact proof required | include with `storage_mode` metadata | include | include | Used for gate/readiness reporting. |
| `path_evidence` | compact proof required | include verified proof paths | include proof plus audit paths | include proof plus debug paths | Must be sufficient for context packet gate. |
| `path_evidence_lookup` | compact proof required | include | include | include | Required for context_pack latency. |
| `path_evidence_edges` | compact proof required | include | include | include | Ordered proof edge IDs. |
| `path_evidence_symbols` | compact proof required | include | include | include | Critical symbol recall. |
| `path_evidence_tests` | compact proof required | include test-impact proof refs | include | include | Expected tests recall. |
| `path_evidence_files` | compact proof required | include | include | include | File-scoped invalidation. |
| `file_entities` | compact proof required | include | include | include | Incremental stale cleanup. |
| `file_edges` | compact proof required | include | include | include | Incremental stale cleanup. |
| `file_source_spans` | compact proof required | include | include | include | Incremental stale cleanup. |
| `file_path_evidence` | compact proof required | include | include | include | Dirty PathEvidence invalidation. |
| `file_fts_rows` | compact proof optional | omit unless text index enabled | include if audit FTS enabled | include if debug FTS enabled | FTS must not store source snippets by default. |
| `file_graph_digests` | compact proof required | include | include | include | Incremental graph hash. |
| `repo_graph_digest` | compact proof required | include | include | include | Worker determinism and unchanged checks. |
| `derived_edges` | derived/cache relation | include only provenance-valid derived facts | include derived attempts | include derived attempts and traces | Duplicate of proof `edges` should be avoided in target schema. |
| `bench_tasks` | temporary/build-only | omit | optional sidecar | include | Benchmark artifacts do not belong in proof DB. |
| `bench_runs` | temporary/build-only | omit | optional sidecar | include | Benchmark artifacts do not belong in proof DB. |
| `retrieval_traces` | audit/debug only | omit | include sampled traces | include full traces | Runtime trace bloat must be sidecar-only. |
| `stage0_fts` | compact proof optional | optional external-content FTS only | include if requested | include if requested | No source snippet storage in proof by default. |
| `stage0_fts_config` | compact proof optional | follows `stage0_fts` | follows `stage0_fts` | follows `stage0_fts` | SQLite FTS shadow table. |
| `stage0_fts_content` | audit/debug only | omit by default | include if requested | include if requested | Stores FTS content, not proof essential. |
| `stage0_fts_data` | compact proof optional | follows `stage0_fts` | follows `stage0_fts` | follows `stage0_fts` | SQLite FTS shadow table. |
| `stage0_fts_docsize` | compact proof optional | follows `stage0_fts` | follows `stage0_fts` | follows `stage0_fts` | SQLite FTS shadow table. |
| `stage0_fts_idx` | compact proof optional | follows `stage0_fts` | follows `stage0_fts` | follows `stage0_fts` | SQLite FTS shadow table. |
| `qualified_name_lookup` | compact proof optional | view only | view only | view only | No storage; allowed for human lookup. |
| `qualified_name_debug` | audit/debug only | omit from proof schema if view cost matters | include | include | Debug view. |
| `object_id_debug` | audit/debug only | omit from proof schema if view cost matters | include | include | Debug view. |

## Proof-Mode Column Contract

Proof mode should retain these column families only:

- File/snapshot: `path_id`, `file_hash`, `language_id`, `size_bytes`, `indexed_at_unix_ms`.
- Entity identity: compact `id_key`, `kind_id`, `name_id`, `qualified_name_id` or hierarchy-derived qname, `path_id`, `span_id`, `parent_id`, `file_id`, `scope_id`, `declaration_span_id`, `created_from_id`, `confidence`.
- Proof edges: `id_key`, `head_id_key`, `relation_id`, `tail_id_key`, normalized `span_id`, `extractor_id`, `confidence`, `exactness_id`, `edge_class_id`, `context_id`, `derived`, compact provenance reference.
- PathEvidence: path ID, task class, source/target entity IDs, relation signature, ordered proof edge IDs, symbol/test/file maps, confidence.
- Incremental maps: file-to-entity, file-to-edge, file-to-span, file-to-path-evidence, file digest, repo digest.

Proof mode should not retain:

- repeated `file_hash` per entity/edge/structural/callsite row.
- broad `metadata_json` blobs on proof rows.
- unresolved/static heuristic facts.
- raw LocalFactBundle payloads.
- retrieval traces.
- benchmark rows.
- source snippets or copied source text.

## Relation Classification

| Relation | Classification | Proof-mode rule |
| --- | --- | --- |
| `CONTAINS` | structural relation | derive from entity hierarchy or compact structural table, not proof edge |
| `DEFINED_IN` | structural relation | derive from `entities.file_id` / declaration span, not proof edge |
| `DEFINES` | structural relation | keep only if needed for import/export proof; otherwise derive |
| `DECLARES` | structural relation | derive from parent/scope attributes |
| `EXPORTS` | proof-edge relation | include only exact/span-valid export facts |
| `IMPORTS` | proof-edge relation | include only exact/span-valid import facts |
| `REEXPORTS` | proof-edge relation | include exact/span-valid re-export facts |
| `BELONGS_TO` | structural relation | derive from ownership hierarchy |
| `CONFIGURES` | proof-edge relation | include only exact config facts |
| `TYPE_OF` | proof-edge relation | include only exact supported type facts; heuristic type guesses go to audit |
| `RETURNS` | proof-edge relation | include only exact supported return facts |
| `IMPLEMENTS` | proof-edge relation | include exact supported inheritance/interface facts |
| `EXTENDS` | proof-edge relation | include exact supported inheritance facts |
| `OVERRIDES` | proof-edge relation | include exact supported override facts |
| `INSTANTIATES` | proof-edge relation | include exact supported construction facts |
| `INJECTS` | proof-edge relation | include exact supported DI facts |
| `ALIASED_BY` | proof-edge relation | include exact resolver alias facts |
| `ALIAS_OF` | proof-edge relation | include exact resolver alias facts |
| `CALLS` | proof-edge relation | include exact resolved calls only; unresolved calls go to audit |
| `CALLED_BY` | derived/cache relation | derive inverse from `CALLS`; do not store as base proof edge |
| `CALLEE` | structural relation | callsite table only; proof only when needed for PathEvidence |
| `ARGUMENT_0` | structural relation | callsite arg table only; proof only when needed for flow evidence |
| `ARGUMENT_1` | structural relation | callsite arg table only; proof only when needed for flow evidence |
| `ARGUMENT_N` | structural relation | callsite arg table only; proof only when needed for flow evidence |
| `RETURNS_TO` | structural relation | callsite/return-site structure; audit unless proof path requires it |
| `SPAWNS` | proof-edge relation | include exact async/process spawn facts |
| `AWAITS` | proof-edge relation | include exact async await facts |
| `LISTENS_TO` | proof-edge relation | include exact event subscription facts |
| `READS` | proof-edge relation | include exact/span-valid read facts |
| `WRITES` | proof-edge relation | include exact/span-valid write facts |
| `MUTATES` | proof-edge relation | include exact/span-valid mutation facts |
| `MUTATED_BY` | derived/cache relation | derive inverse from `MUTATES`; do not store as base proof edge |
| `FLOWS_TO` | proof-edge relation | include exact/proven supported flow facts; heuristic flow goes to audit |
| `REACHING_DEF` | derived/cache relation | cache only with provenance; otherwise audit/debug |
| `ASSIGNED_FROM` | proof-edge relation | include exact assignment facts |
| `CONTROL_DEPENDS_ON` | derived/cache relation | include only provenance-backed cache; otherwise audit/debug |
| `DATA_DEPENDS_ON` | derived/cache relation | include only provenance-backed cache; otherwise audit/debug |
| `AUTHORIZES` | proof-edge relation | include exact supported auth facts |
| `CHECKS_ROLE` | proof-edge relation | include exact supported role checks only |
| `CHECKS_PERMISSION` | proof-edge relation | include exact supported permission checks only |
| `SANITIZES` | proof-edge relation | include only proven sanitizer-on-flow facts |
| `VALIDATES` | proof-edge relation | include exact supported validation facts |
| `EXPOSES` | proof-edge relation | include exact supported route exposure facts |
| `TRUST_BOUNDARY` | proof-edge relation | include exact supported boundary facts; heuristic boundaries go to audit |
| `SOURCE_OF_TAINT` | proof-edge relation | include exact supported taint sources; heuristic broad matches go to audit |
| `SINKS_TO` | proof-edge relation | include exact supported sink facts; heuristic sinks go to audit |
| `PUBLISHES` | proof-edge relation | include exact event publication facts |
| `EMITS` | proof-edge relation | include exact event emission facts |
| `CONSUMES` | proof-edge relation | include exact consumer facts |
| `SUBSCRIBES_TO` | proof-edge relation | include exact subscription facts |
| `HANDLES` | proof-edge relation | include exact handler facts |
| `MIGRATES` | proof-edge relation | include exact migration facts |
| `READS_TABLE` | proof-edge relation | include exact DB read facts |
| `WRITES_TABLE` | proof-edge relation | include exact DB write facts |
| `ALTERS_COLUMN` | proof-edge relation | include exact migration column facts |
| `DEPENDS_ON_SCHEMA` | proof-edge relation | include exact schema dependency facts |
| `TESTS` | test/proof relation | include exact test relation; excluded from production proof paths |
| `ASSERTS` | test/proof relation | include exact assertion relation; excluded from production proof paths |
| `MOCKS` | test/proof relation | include exact mock relation; excluded from production proof paths |
| `STUBS` | test/proof relation | include exact stub relation; excluded from production proof paths |
| `COVERS` | test/proof relation | include exact coverage/test relation; excluded from production proof paths |
| `FIXTURES_FOR` | test/proof relation | include exact fixture relation; excluded from production proof paths |
| `MAY_MUTATE` | derived/cache relation | include only derived edge with resolvable provenance |
| `MAY_READ` | derived/cache relation | include only derived edge with resolvable provenance |
| `API_REACHES` | derived/cache relation | include only derived edge with resolvable provenance |
| `ASYNC_REACHES` | derived/cache relation | include only derived edge with resolvable provenance |
| `SCHEMA_IMPACT` | derived/cache relation | include only derived edge with resolvable provenance |

## High-Cardinality Relation Decisions

| Relation | Current count | Target proof-mode decision |
| --- | ---: | --- |
| `CONTAINS` | 907,236 | derive from entity hierarchy |
| `DEFINED_IN` | 897,965 | derive from `entities.file_id` / declaration span |
| `CALLEE` | 524,760 | compact callsite table, no standalone proof edge |
| `CALLS` | 516,392 | proof edge only when exact; heuristic calls to audit |
| `ARGUMENT_0` | 332,104 | compact callsite arg table only when flow evidence needs it |
| `DECLARES` | 214,406 | derive from parent/scope attributes |
| `FLOWS_TO` | 163,481 | proof edge only when supported/proven; heuristic flow to audit |
| `ARGUMENT_1` | 73,913 | compact callsite arg table only when flow evidence needs it |
| `DEFINES` | 72,897 | derive when structural; keep exact semantic define/export links only |
| `IMPORTS` | 72,626 | proof edge if exact/span-valid |
| `ARGUMENT_N` | 60,939 | compact callsite arg table only when flow evidence needs it |

## Gate Mapping

### Graph Truth Gate On Proof Mode

Graph Truth must run against proof DB only:

```text
codegraph-mcp bench graph-truth --storage-mode proof ...
```

Required assertions map as follows:

- expected entities: `entities`, `symbol_dict`, `qualified_name_lookup`, `files`
- expected edges: proof `edges`, derived/provenance-valid `derived_edges`, compact callsite facts only when proof-eligible
- forbidden edges/paths: proof `edges` and proof `path_evidence`
- source spans: `source_spans` or normalized span columns plus source loader validation
- context symbols: `path_evidence_symbols` and proof entity lookup
- expected tests: test/proof relations plus `path_evidence_tests`
- stale mutation checks: `file_entities`, `file_edges`, `file_source_spans`, `file_path_evidence`, digests

Heuristic/audit sidecar facts are invisible to Graph Truth unless the gate explicitly enters audit mode, and audit mode must never count as a proof pass.

### Context Packet Gate On Proof Mode

Context Packet must use proof DB first:

- `path_evidence_lookup` for candidate proof paths.
- `path_evidence_edges` for ordered proof edges.
- `path_evidence_symbols` for critical symbol recall.
- `path_evidence_tests` for expected tests recall.
- `source_spans` and source files on disk for snippets.
- proof `edges` only as bounded fallback.

Context Packet must not use audit/debug sidecar rows to satisfy proof-path coverage.

## Audit And Debug Sidecar Use

Relation/source-span audit commands should resolve samples in this order:

1. proof DB facts.
2. audit sidecar facts, if present.
3. debug sidecar raw extraction records, if requested.

Each sampled row must expose `storage_mode_origin`:

```text
proof
audit_sidecar
debug_sidecar
```

Manual labeling summaries must separate:

- proof false positives.
- audit-only heuristic false positives.
- unsupported patterns.
- wrong spans.
- stale/debug artifacts.

This prevents an audit-only unsupported pattern from looking like a proof-mode correctness failure, while still preserving the evidence needed to fix extractors.

## Migration Plan

1. Add `--storage-mode proof|audit|debug` to index and benchmark commands.
2. Add `storage_mode` to `repo_index_state.metadata_json` first; later promote to a real column.
3. Introduce proof writer routing without deleting existing debug paths.
4. Add audit sidecar writer for rejected/downgraded facts.
5. Add debug sidecar writer for raw bundle/trace payloads.
6. Move repeated `file_hash` from fact rows into `files` / file snapshot references.
7. Replace proof-row `metadata_json` with compact enum/bitset columns plus optional audit metadata rows.
8. Split heuristic/unresolved facts out of proof `edges`.
9. Normalize source spans and provenance references.
10. Re-run Graph Truth and Context Packet gates against proof mode only.
11. Measure proof DB family size separately from sidecars.

Existing schema-v9 DBs should be treated as `debug_legacy` or `audit_legacy`: readable, but not eligible for proof-mode storage target claims until rebuilt or migrated through the routing writer.

## Risks

- Too aggressive proof filtering could hide unsupported patterns instead of exposing them. Mitigation: audit sidecar records every rejected fact with reason.
- Moving qname/prefix data out of proof mode could weaken symbol query and context labels. Mitigation: keep compact qname views until hierarchy reconstruction is tested.
- Separating sidecars could make audit commands awkward. Mitigation: audit commands accept `--db` and discover sibling sidecars automatically, with explicit missing-sidecar warnings.
- Derived/cache facts can become stale if provenance invalidation misses a file. Mitigation: file-to-path-evidence maps remain proof-required.
- Test/mock facts are proof facts for test-impact mode but contaminants for production proof paths. Mitigation: edge context remains required and path verifier keeps production/test gates separate.

## Acceptance Mapping

| Requirement | Design answer |
| --- | --- |
| Every current table classified | yes, see Current Table Classification |
| Every high-cardinality relation classified | yes, see High-Cardinality Relation Decisions |
| Proof mode contains no heuristic/unresolved/debug-only facts by default | yes, enforced by proof eligibility predicate |
| Audit/debug mode preserves diagnostic information | yes, sidecars preserve rejected facts and traces |
| Graph Truth Gate mapped to proof tables | yes |
| Context Packet Gate mapped to proof tables | yes |
| No code behavior changes yet | yes, this is a report-only design phase |
| Proof target <=250 MiB | design target; not yet implemented |

## Next Implementation Sequence

1. Add storage-mode option and metadata, with no row-routing changes.
2. Add proof/audit/debug writer interfaces behind existing single-writer policy.
3. Route heuristic/unresolved `CALLS` and debug metadata to audit sidecar.
4. Remove repeated `file_hash` from proof fact rows via file snapshot IDs.
5. Replace proof `metadata_json` with compact flags.
6. Normalize source spans and compact provenance references.
7. Rebuild Autoresearch in proof mode and measure DB family size.
8. Only then consider qname/object-id binary compaction and duplicate-content templates.
