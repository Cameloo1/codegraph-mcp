# 07 Source Span Proof Gate Audit

Source of truth: `MVP.md`, especially the directives that the graph is truth, exact graph/source verification proves, every edge must include a source span, and final context packets must contain verified paths with source spans and snippets.

## Verdict

**PathEvidence is not proof-grade by default.** The model stores source spans on every `Edge`, and context packets include spans/snippets, but prior path generation did not validate that proof-sensitive edge spans resolve to source text or point at the relation syntax site. This phase adds a proof-grade source-span validation gate in the query layer and labels invalid context-packet paths as non-proof instead of deleting edges.

## Proof-Path Relations

The proof-grade validation gate treats these relations as proof-path relations:

```text
CALLS, READS, WRITES, FLOWS_TO, MUTATES,
AUTHORIZES, CHECKS_ROLE, SANITIZES, EXPOSES,
INJECTS, INSTANTIATES, PUBLISHES, EMITS,
CONSUMES, LISTENS_TO, TESTS, MOCKS, STUBS, ASSERTS
```

`IMPORTS`, `EXPORTS`, `REEXPORTS`, `ALIAS_OF`, and `ALIASED_BY` are also source-span validated when present because import/export/alias facts are common proof dependencies, but they are not counted as proof-path relations in the prompt's explicit allowed list.

## Code Inspected

| Area | Files and locations | Finding |
| --- | --- | --- |
| Edge and span model | `crates/codegraph-core/src/model.rs` | `Edge.source_span` is required, while `PathEvidence.source_spans` is a vector copied from traversed edges. |
| SQLite edge/source-span schema | `crates/codegraph-store/src/sqlite.rs` | Edge rows store inline span coordinates; standalone `source_spans` exists separately. |
| Context packet builder | `crates/codegraph-query/src/lib.rs:1390`, `crates/codegraph-query/src/lib.rs:1811` | Context packets now build `PathEvidence` with source-span validation metadata. |
| Validation function | `crates/codegraph-query/src/lib.rs:118`, `crates/codegraph-query/src/lib.rs:197` | New proof relation classifier and `validate_proof_path_source_spans` function. |
| Snippet extraction | `crates/codegraph-query/src/lib.rs` | Snippets remain best-effort, but proof labeling no longer depends on snippet presence alone. |
| CLI/MCP source loading | `crates/codegraph-cli/src/lib.rs`, `crates/codegraph-mcp-server/src/lib.rs` | Both load indexed source files into the context-packet source map when files exist. |
| Prior audits | `reports/audit/02_graph_schema_edge_taxonomy.md`, `reports/audit/04_relation_counts_and_samples.md`, `reports/audit/05_stable_symbol_identity.md`, `reports/audit/06_manifest_hashing_stale_updates.md` | Prior phases found that graph facts are not proof-grade as a whole, standalone span rows are missing in the latest DB, and stale/identity guarantees remain partial. |

## What Changed

Added `validate_proof_path_source_spans(path, sources)` in `codegraph-query`.

For each proof-sensitive edge, validation now requires:

- source span path is non-empty;
- start/end lines are non-zero and ordered;
- source file text is loaded, otherwise the path is marked non-proof with `source_unavailable`;
- span resolves to non-empty text;
- span line/column range is in bounds;
- relation-specific syntax is plausible for the span, so a whole containing file or unrelated line does not silently pass as proof.

Context packet generation now uses validated `PathEvidence`. Invalid paths are still returned, but are labeled:

- `exactness = inferred`;
- `confidence <= 0.49`;
- `metadata.proof_grade_source_spans = false`;
- `metadata.source_span_validation = "failed"`;
- `metadata.source_span_issues = [...]`.

Valid proof-span paths receive `metadata.proof_grade_source_spans = true`.

## Tests Added

Tests were added in `crates/codegraph-query/src/lib.rs`:

| Test | Line | Status | Covers |
| --- | ---: | --- | --- |
| `proof_span_validation_accepts_exact_callsite_span` | 3663 | Pass | Exact `CALLS` callsite span. |
| `proof_span_validation_accepts_exact_import_span` | 3679 | Pass | Exact `IMPORTS` span for dependency proof support. |
| `proof_span_validation_accepts_exact_role_check_span` | 3693 | Pass | Exact `CHECKS_ROLE` span. |
| `proof_span_validation_accepts_exact_assertion_span` | 3707 | Pass | Exact `ASSERTS` span. |
| `proof_span_validation_rejects_missing_span` | 3721 | Pass | Empty path/zero line span fails. |
| `proof_span_validation_rejects_wrong_file_span` | 3735 | Pass | Wrong source file line fails syntax-site validation. |
| `proof_span_validation_rejects_out_of_range_span` | 3762 | Pass | Out-of-range line fails. |
| `context_packet_labels_invalid_span_paths_non_proof` | 3777 | Pass | Context packet keeps invalid path but labels it non-proof. |

## Current Source-Span Risk

The latest relation-count audit showed that every relation in the large DB had `missing_source_span_rows = edge_count` when joined against the standalone `source_spans` table. Inline edge span coordinates are present, so this is not total span absence, but it means edge-id-keyed standalone source-span auditability is currently weak.

Relations most concerning for proof paths:

- `CALLS`: 271509 edges; 250745 static-heuristic; all missing standalone span rows.
- `CALLEE`: 276611 edges; mostly static-heuristic; not in the explicit proof list but tightly coupled to `CALLS`.
- `FLOWS_TO`: 94236 edges; parser-verified syntax does not prove semantic flow correctness.
- `READS`: 5335 edges; inline source-span count was lower than edge count in the artifact.
- `ASSERTS`, `TESTS`, `MOCKS`: test/mock context is inferred, not first-class.
- `CHECKS_ROLE`, `SANITIZES`, `CONSUMES`, `LISTENS_TO`: small enough to manually inspect, but currently static-heuristic.

## Remaining Gaps

- `PathEvidence` created through `path_evidence()` without source maps is still only graph provenance, not proof-grade source provenance.
- CLI/MCP `trace_path` and impact sections still serialize path evidence through the older non-validating path conversion; context packets are the protected output now.
- The validation uses relation-specific syntax heuristics, not AST node identity or compiler/LSP proof.
- Missing source files are marked non-proof, not resolved through an explicit unavailable-source manifest.
- Standalone `source_spans` rows are still not a reliable edge-id-keyed audit source in the latest DB.

## Recommended Next Fixes

1. Add a first-class `proof_grade` or `source_span_validated` field to `PathEvidence`, not just metadata.
2. Persist source-span validation status for saved `PathEvidence`.
3. Make `trace_path`, impact analysis, and MCP path responses use the same validation mode when source files are available.
4. Replace syntax heuristics with parser/AST node-kind validation for high-risk relations.
5. Store explicit unavailable-source reasons in the manifest when source text cannot be loaded.
6. Restore or populate standalone edge-id-keyed `source_spans` rows, or document inline edge spans as the canonical source-span table equivalent.
7. Manually sample and classify `CALLS`, `FLOWS_TO`, `READS`, `ASSERTS`, `TESTS`, `MOCKS`, `CHECKS_ROLE`, and `SANITIZES` for wrong-span rates.

## Bottom Line

Current context packets can now distinguish graph paths with proof-grade source-span provenance from paths that are merely graph-connected. Existing edges are preserved; invalid or unavailable spans downgrade the path to non-proof instead of being silently presented as verified source proof.
