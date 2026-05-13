# 02 Graph Truth Manifest Schema

Source of truth: `MVP.md`.

Baseline context: `reports/baselines/BASELINE_LATEST.md` freezes the current benchmark as `diagnostic_unknown`. It records that CodeGraph completed the Autoresearch index, but graph quality remains unknown, CGC was incomplete, and fake-agent output is not model evidence. This schema exists to turn later benchmark numbers into correctness claims only when expected and forbidden graph facts are checked.

## Verdict

Status: `complete`.

The graph-truth manifest schema now supports strict positive and negative assertions, mutation/update scenarios, source-span proof requirements, context packet expectations, and per-case performance expectations. This phase does not implement or change runner scoring.

## Schema Location

- `benchmarks/graph_truth/schemas/graph_truth_case.schema.json`
- Fixture root: `benchmarks/graph_truth/fixtures/`

Required top-level manifest fields:

- `case_id`
- `description`
- `repo_fixture_path`
- `task_prompt`
- `expected_entities`
- `expected_edges`
- `forbidden_edges`
- `expected_paths`
- `forbidden_paths`
- `expected_source_spans`
- `expected_context_symbols`
- `forbidden_context_symbols`
- `expected_tests`
- `forbidden_tests`
- `mutation_steps`
- `performance_expectations`
- `notes`

## Supported Assertion Types

Entity assertions identify expected graph entities by stable selectors such as `id`, `qualified_name`, or `name` plus `source_file`.

Edge assertions support:

- `head`
- `relation`
- `tail`
- `source_file`
- `source_span`
- `exactness_required`
- `context_kind`: `production`, `test`, `mock`, `mixed`, or `unknown`
- `derived_allowed`
- `provenance_required`
- `extractor_kind`

The schema also retains legacy runner fields (`exactness`, `context`, `derived`, and `provenance_edges`) so current benchmark code can keep reading existing manifests while future phases move to the canonical fields.

Path assertions support:

- `ordered_edges`
- `relation_sequence`
- `max_length`
- `source_span_required`
- `production_only`
- `derived_allowed`
- `provenance_required`

Context assertions support required and forbidden symbols, critical symbols, distractor thresholds, and expected or forbidden tests.

Mutation steps support:

- `edit_file`
- `rename_file`
- `delete_file`
- `change_import_alias`
- `add_file`
- `remove_file`
- `reindex`
- `query_again`

## Failure Rules

The schema defines these failure rules for future gates:

- Required edge missing => fail.
- Forbidden edge present => fail.
- Wrong edge direction => fail.
- Source span missing or wrong => fail.
- Unresolved relation labeled exact => fail.
- Mock/test edge enters production proof path => fail.
- Derived edge lacks provenance => fail.
- Stale edge survives edit/rename/delete => fail.
- Same-name symbol collision => fail.
- Context packet misses critical symbol => fail.
- Forbidden context symbol included => fail.
- Expected test omitted => fail.
- Too many distractors fail when a threshold is specified.

## Performance Criterion

The test suite includes a 100-manifest schema validation timing check and requires it to complete in under 1 second. This keeps fixture validation cheap enough to run before graph-truth gates.

## Tests Added Or Updated

- `graph_truth_case_schema_accepts_strict_case`
- `graph_truth_case_schema_rejects_malformed_cases`
- `graph_truth_case_schema_defines_failure_rules`
- `graph_truth_case_schema_validation_is_fast_for_100_manifests`
- `adversarial_graph_truth_fixture_cases_validate`

Focused validation run:

```text
cargo test -p codegraph-bench graph_truth_case_schema -- --nocapture
cargo test -p codegraph-bench adversarial_graph_truth_fixture_cases_validate -- --nocapture
```

Both focused checks passed.

## Notes

- This phase intentionally does not claim graph correctness.
- This phase intentionally does not implement the runner.
- Existing fixture manifests were mechanically migrated to include the new top-level `mutation_steps` and `performance_expectations` fields plus canonical edge/path assertion fields.
- The next phase should build or refresh the ten fixture repos using this manifest contract.

