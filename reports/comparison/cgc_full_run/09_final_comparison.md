# Final CodeGraph vs CGC Diagnostic Comparison

Official comparison remains blocked unless CodeGraph intended quality gate passes and CGC completes fairly.
No CodeGraph superiority claim is made from diagnostic, timed-out, or partial CGC evidence.

| Field | Value |
|---|---|
| `schema_version` | 1 |
| `generated_at` | 2026-05-14T01:07:26-05:00 |
| `source_of_truth` | MVP.md |
| `fixed_fork_path` | <CGC_FORK_ROOT> |
| `fixed_fork_exists` | True |
| `fixed_fork_commit` | fcf03925f640541eed4a69e085af22a778bdaf30 |
| `fixed_fork_version` | 0.4.8 |
| `cgc_install_mode` | fork_editable |
| `cgc_executable` | <REPO_ROOT>\target\cgc-competitor\cgc-fork-venv\Scripts\cgc.exe |
| `cgc_smoke_test_passed` | True |
| `cgc_fixture_diagnostic_run` | True |
| `cgc_fixture_diagnostic_completed_runs` | 5 |
| `cgc_normal_repo_diagnostic_run` | True |
| `cgc_normal_repo_index_completed` | False |
| `cgc_autoresearch_180s_completed` | False |
| `cgc_autoresearch_180s_timed_out` | True |
| `cgc_autoresearch_600s_completed` | False |
| `cgc_autoresearch_600s_timed_out` | True |
| `codegraph_cargo_build_passed` | True |
| `codegraph_cargo_test_passed` | True |
| `codegraph_intended_quality_gate_passed` | False |
| `existing_latest_intended_gate` | `{"exists": true, "path": "reports/final/intended_tool_quality_gate.json", "verdict": "fail", "reason": "Fresh comprehensive gate fails proof_build_only_ms target; official CGC comparison remains blocked/diagnostic only.", "failed_targets": ["proof_build_only_ms"], "official_comparison_allowed": false, "proof_build_only_ms": 184297}` |
| `official_comparison_allowed` | False |
| `official_comparison_verdict` | blocked_by_codegraph_gate |
| `diagnostic_cgc_verdict` | partial_pass_smoke_and_fixture_diagnostic_autoresearch_incomplete |
| `no_codegraph_superiority_claim` | True |
| `proof_build_only_ms_failure_visible` | 184297 |
| `manual_relation_precision_boundary` | 320 labeled samples, sampled precision only, recall unknown, no precision claim for absent proof-mode relations. |
| `raw_log_paths` | reports/comparison/cgc_full_run/logs |
| `raw_cgc_artifacts` | `{"phase3_smoke_home": "reports/comparison/cgc_full_run/cgc/home_smoke", "phase4_harness_output": "reports/comparison/cgc_full_run/artifacts/harness_cgc_comparison", "phase7_normal_home": "reports/comparison/cgc_full_run/cgc/home_normal_repo", "phase8_autoresearch_180s_home": "reports/comparison/cgc_full_run/cgc/home_autoresearch_180s", "phase8_autoresearch_600s_home": "reports/comparison/cgc_full_run/cgc/home_autoresearch_600s"}` |
| `report_paths` | `{"status_md": "reports/comparison/cgc_full_run/CGC_FULL_RUN_STATUS.md", "status_json": "reports/comparison/cgc_full_run/cgc_full_run_status.json", "fork_inspection_md": "reports/comparison/cgc_full_run/01_fork_inspection.md", "fork_inspection_json": "reports/comparison/cgc_full_run/01_fork_inspection.json", "fork_install_md": "reports/comparison/cgc_full_run/02_fork_install.md", "fork_install_json": "reports/comparison/cgc_full_run/02_fork_install.json", "smoke_md": "reports/comparison/cgc_full_` |
