# Manual Relation Precision Gate

## Verdict

- Status: reported for labeled compact-proof relations; no precision claim for relations absent from the proof DB.
- Labeled samples: 320 total (300 edge samples, 20 PathEvidence samples).
- Claim boundary: sampled precision only. Recall remains unknown without a false-negative gold denominator.
- Unsupported/absent patterns are separated from false positives.

## Target Evaluation

| Relation | Proof DB edges | Labeled | Precision | Target | Status | Claim |
|---|---:|---:|---:|---:|---|---|
| CALLS | 2363 | 50 | 100.00% | 95% | pass | sampled_precision_estimate |
| READS | 1970 | 50 | 100.00% | 90% | pass | sampled_precision_estimate |
| WRITES | 1104 | 50 | 100.00% | 90% | pass | sampled_precision_estimate |
| FLOWS_TO | 14801 | 50 | 100.00% | 85% | pass | sampled_precision_estimate |
| MUTATES | 177 | 50 | 100.00% | n/a | reported_no_target | sampled_precision_estimate |
| AUTHORIZES | 0 | 0 | n/a | 95% | no_claim_absent_in_proof_db | no_precision_claim |
| CHECKS_ROLE | 0 | 0 | n/a | 95% | no_claim_absent_in_proof_db | no_precision_claim |
| SANITIZES | 0 | 0 | n/a | 95% | no_claim_absent_in_proof_db | no_precision_claim |
| EXPOSES | 0 | 0 | n/a | n/a | no_claim_absent_in_proof_db | no_precision_claim |
| TESTS | 0 | 0 | n/a | 90% | no_claim_absent_in_proof_db | no_precision_claim |
| ASSERTS | 0 | 0 | n/a | 90% | no_claim_absent_in_proof_db | no_precision_claim |
| MOCKS | 0 | 0 | n/a | 90% | no_claim_absent_in_proof_db | no_precision_claim |
| STUBS | 0 | 0 | n/a | 90% | no_claim_absent_in_proof_db | no_precision_claim |
| PathEvidence | 20 | 20 | 100.00% | 95% | pass | sampled_precision_estimate |
| MAY_MUTATE | 1130 | 50 | 100.00% | n/a | reported_no_target | sampled_precision_estimate |

## Source Span And PathEvidence

- Source-span precision: 100.00% (0 wrong spans across 320 eligible samples).
- PathEvidence correctness: 100.00% across 20 labeled samples.

## False-Positive Taxonomy

| Category | Count |
|---|---:|
| wrong_direction | 0 |
| wrong_target | 0 |
| wrong_span | 0 |
| stale | 0 |
| duplicate | 0 |
| unresolved_mislabeled_exact | 0 |
| test_mock_leaked | 0 |
| derived_missing_provenance | 0 |
| unsure | 0 |
| unsupported | 0 |

## Relation Coverage

- Present labeled relations: CALLS, READS, WRITES, FLOWS_TO, MUTATES, PathEvidence, MAY_MUTATE
- Absent proof-mode relations with no precision claim: AUTHORIZES, CHECKS_ROLE, SANITIZES, EXPOSES, TESTS, ASSERTS, MOCKS, STUBS

## Artifacts

- Source proof DB: `reports/final/artifacts/comprehensive_proof_1778642765060.sqlite`
- Relation counts: `reports/final/artifacts/manual_precision_relation_counts.json`
- Labeled sample JSON files: `reports/final/artifacts/manual_precision_labeled_*.json`

## Notes

- The current Autoresearch compact proof DB contains no sampled AUTHORIZES, CHECKS_ROLE, SANITIZES, EXPOSES, TESTS, ASSERTS, MOCKS, or STUBS proof edges, so this report does not claim precision for those relations.
- MUTATES and MAY_MUTATE were labeled because they are present in the proof DB; no prompt threshold was specified for them.
- The comprehensive benchmark should treat this as real labeled precision for the sampled present relations, not as a real-world recall claim.
