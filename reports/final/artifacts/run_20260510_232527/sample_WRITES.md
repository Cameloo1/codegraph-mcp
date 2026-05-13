# Edge Sample Audit

Database: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/fixture_workers4.sqlite`

Relation filter: `WRITES`

Limit: `50`

Seed: `17`

Allowed manual classifications: true_positive, false_positive, wrong_direction, wrong_target, wrong_span, stale, duplicate, unresolved_mislabeled_exact, test_mock_leaked, derived_missing_provenance, unsure

## Sample 1

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- edge_id: `edge-key:-7412890628781895775`
- head: `repo://e/af824afc40aa9bc54b31726d12081117` (`loadPlugin`)
- relation: `WRITES`
- tail: `repo://e/a24e6d8368307ba3b83fd00e9b292759` (`mod`)
- source_span: `dynamic_import_marked_heuristic/repo/src/loader.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3e60612ecd044bf9`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export async function loadPlugin(name: string) {
2:   const mod = await import("./plugins/" + name);
3:   return mod.default();
4: }
```

## Sample 2

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- edge_id: `edge-key:-5748113415438238707`
- head: `repo://e/1f89b459da26c0283f1355c5c7b2b612` (`saveComment`)
- relation: `WRITES`
- tail: `repo://e/6231fc5dca04885693d7ef9c5c0e88c4` (`normalized`)
- source_span: `sanitizer_exists_but_not_on_flow/repo/src/register.ts:5-5`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:19c73fc7fae08363`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
3: export function saveComment(req: any) {
4:   const raw = req.body.comment;
5:   const normalized = raw.trim();
6:   return writeComment(normalized);
7: }
```

## Sample 3

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- edge_id: `edge-key:-997635485439463650`
- head: `repo://e/1f89b459da26c0283f1355c5c7b2b612` (`saveComment`)
- relation: `WRITES`
- tail: `repo://e/7725451ed037301988efdd7f414beb63` (`raw`)
- source_span: `sanitizer_exists_but_not_on_flow/repo/src/register.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:19c73fc7fae08363`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export function saveComment(req: any) {
4:   const raw = req.body.comment;
5:   const normalized = raw.trim();
6:   return writeComment(normalized);
```

