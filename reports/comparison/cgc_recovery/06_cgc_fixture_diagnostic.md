# 06 CGC Fixture Diagnostic

Generated: 2026-05-13 08:51:00 -05:00

Status: completed_not_comparable

- CGC fixture runs: 5
- Skipped: 0
- Total index latency: 9289 ms
- Total query latency: 19034 ms
- Average file R@10: 0.0
- Average symbol R@10: 0.4066666666666666
- Average path R@10: 0.0
- Average relation F1: 0.0

Classification: completed_not_comparable. CGC was actually invoked for every external fixture in the existing harness, but its normalized output did not expose comparable proof paths, relation sequences, source spans, or final DB artifact semantics.

Raw fixture artifacts are under reports/comparison/cgc_recovery/artifacts/fixture_diagnostic/.