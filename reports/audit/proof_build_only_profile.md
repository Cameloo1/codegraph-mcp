# Proof Build Only Profile

Source of truth: `MVP.md`.

Generated: 2026-05-13 00:56:13 -05:00

## Verdict

Proof-build-only profile: **58991 ms** (target <= 60,000 ms): **pass**.

No stage over 5% is reported as unknown. Skipped audit/reporting/CGC stages are recorded as skipped/0 ms.

## Top 10 Stages

| Stage | ms | Count | Items |
| --- | ---: | ---: | ---: |
| extract_entities_and_relations | 21783 | 4975 | 4975 |
| reducer | 16485.788 | 39 | 4975 |
| edge_insert | 8879.595 | 170484 | 170484 |
| local_fact_bundle_creation | 8282 | 4975 | 4975 |
| global_resolver_workspace_load | 8132.441 | 1 | 1160 |
| parse | 7993 | 4975 | 4975 |
| parse_extract_workers_wall | 5224.666 | 39 | 4975 |
| entity_insert | 4641.822 | 89580 | 89580 |
| dictionary_lookup_insert | 4110.092 | 1699517 | 1699517 |
| sqlite.dictionary_lookup_insert | 4110.092 | 1699517 | 1699517 |

## Summary

- Workers: 64
- Files walked: 8564
- Files read/hashed: 4975 / 4975
- Files parsed: 2555
- Entities/edges: 1397095 / 508643
- DB write: 35337 ms
- Parse/extract CPU-sum: 7993 / 21783 ms

Raw profile artifact: `reports/audit/artifacts/proof_build_only_profile_publish_skip_w64_20260513_004355.stdout.json`
