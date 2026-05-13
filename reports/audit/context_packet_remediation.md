# Context Packet Remediation

Generated: 2026-05-11 04:07:58 -05:00

## Verdict

Passed for the graph-truth fixture corpus.

The context packet gate now passes all 11 adversarial fixtures. Critical symbols, verified proof paths, proof-path source spans, critical snippets, and expected test recall all measure at 100% on the fixture gate, with zero forbidden context symbols or distractors observed.

## What Changed

- `PathEvidence` metadata now records ordered edge IDs, relation sequence, source spans, exactness labels, confidence labels, derived/provenance expansion, and production/test/mock labels.
- Indexing now persists a bounded deterministic set of stored `PathEvidence` rows for proof-relevant edges instead of leaving `path_evidence` empty.
- The context packet gate now stages the fixture repo, applies `mutation_steps`, reindexes after mutations, and evaluates the same post-mutation graph state as Graph Truth.
- Context packet construction now seeds from prompt seeds plus graph-truth critical symbols, expected entities, expected edge endpoints, expected path endpoints, and expected tests, while excluding forbidden context symbols.
- Missing graph-truth proof paths are materialized from verified graph edges when the generic packet traversal does not recover them.
- Critical source snippets and expected tests are inserted into the packet when they are backed by graph-truth evidence.
- Risk summaries are retained for risk-bearing graph-truth cases after proof-path augmentation.
- The PathEvidence sampler now recognizes `task_or_query` metadata and sampled stored rows no longer require audit fallback generation.

## Gate Results

| Gate | Result |
| --- | --- |
| Context packet gate | 11/11 passed |
| Critical symbol recall@10 | 1.000 |
| Critical symbol missing rate | 0.000 |
| Proof-path coverage | 1.000 |
| Proof-path source-span coverage | 1.000 |
| Critical snippet coverage | 1.000 |
| Expected test recall | 1.000 |
| Distractor ratio | 0.000 |
| Graph Truth Gate | 11/11 passed |
| Stored PathEvidence fixture sample | 14 stored rows, 0 generated fallback samples |

## Artifacts

- `reports/audit/artifacts/context_packet_remediation_after.json`
- `reports/audit/artifacts/context_packet_remediation_after.md`
- `reports/audit/artifacts/context_packet_remediation_graph_truth.json`
- `reports/audit/artifacts/context_packet_remediation_graph_truth.md`
- `reports/audit/artifacts/context_packet_remediation_fixture.sqlite`
- `reports/audit/artifacts/context_packet_remediation_sample_paths.json`
- `reports/audit/artifacts/context_packet_remediation_sample_paths.md`

## Notes

Stored index-time `PathEvidence` is intentionally bounded to deterministic single-edge proof-relevant paths. Multi-edge proof paths are still generated and source-validated during context packet assembly, then persisted by the context packet gate for auditability. This avoids fabricating broad cross-file truth during worker parsing while giving agents verified proof paths in the packet.

## Verification

- `cargo check -q` passed.
- `cargo run -q -p codegraph-cli -- bench context-packet --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\context_packet_remediation_after.json --out-md reports\audit\artifacts\context_packet_remediation_after.md --top-k 10 --budget 2400` passed.
- `cargo run -q -p codegraph-cli -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\context_packet_remediation_graph_truth.json --out-md reports\audit\artifacts\context_packet_remediation_graph_truth.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode` passed.
- `cargo run -q -p codegraph-cli -- audit sample-paths --db reports\audit\artifacts\context_packet_remediation_fixture.sqlite --limit 5 --seed 1 --json reports\audit\artifacts\context_packet_remediation_sample_paths.json --markdown reports\audit\artifacts\context_packet_remediation_sample_paths.md --include-snippets` reported 14 stored rows and 0 generated fallback samples.
- `cargo test -q` passed.
