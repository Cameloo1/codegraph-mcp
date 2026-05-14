# CodeGraph Pre-Comparison Gate

| Field | Value |
|---|---|
| `phase` | codegraph_gate |
| `cargo_build_result` | pass |
| `cargo_test_result` | pass |
| `help_result` | pass |
| `graph_truth_result` | `{"status": "failed", "passed": null, "total": null}` |
| `context_packet_result` | `{"status": "passed", "passed": null, "total": null}` |
| `comprehensive_result` | `{"exists": true, "verdict": "fail", "reason_for_failure": "failed targets: bytes_per_proof_edge, cold_proof_build_total_wall_ms, proof_db_mib_stretch, symbol_lookup"}` |
| `db_integrity_status` | ok |
| `proof_db_mib` | 171.18359375 |
| `proof_build_only_ms` | 224729.0 |
| `repeat_unchanged_ms` | 1674.0 |
| `single_file_update_ms` | 336.0 |
| `context_pack_p95_ms` | 198.41049999999998 |
| `unresolved_calls_p95_ms` | 119.4571 |
| `manual_relation_precision_status` | reported_partial |
| `intended_gate_from_existing_latest` | `{"exists": true, "path": "reports/final/intended_tool_quality_gate.json", "verdict": "fail", "reason": "Fresh comprehensive gate fails proof_build_only_ms target; official CGC comparison remains blocked/diagnostic only.", "failed_targets": ["proof_build_only_ms"], "official_comparison_allowed": false, "proof_build_only_ms": 184297}` |
| `official_comparison_allowed` | False |
| `official_comparison_blockers` | `["cold_proof_build_total_wall_ms target not met or fresh comprehensive result unavailable", "bytes_per_proof_edge", "cold_proof_build_total_wall_ms", "proof_db_mib_stretch", "symbol_lookup"]` |
| `commands` | `[{"name": "phase5_cargo_build_workspace", "command": ["cargo", "build", "--workspace"], "cwd": "<REPO_ROOT>", "exit_code": 0, "timed_out": false, "started_at": "2026-05-14T00:41:49-05:00", "ended_at": "2026-05-14T00:41:50-05:00", "duration_ms": 202, "stdout": "reports/comparison/cgc_full_run/logs/phase5_cargo_build_workspace.stdout.txt", "stderr": "reports/comparison/cgc_full_run/logs/phase5_cargo_build_workspace.stderr.txt", "stdout_bytes": 0, "stderr_b` |
