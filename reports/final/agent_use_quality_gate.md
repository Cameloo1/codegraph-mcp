# Agent Use Quality Gate

## Verdict

Overall status: **PASS**

Agent-facing output ready: **true**

This gate covers output shape, context-pack evidence classification, CLI flag UX, schema contract tests, docs checks, and one isolated release-binary self-use smoke. It does not claim a final intended-performance pass, CodeGraph-vs-CGC superiority, storage-gate pass, cold-build pass, or real-world recall.

## Stable Report Decision

Decision: **promote `reports/final/agent_use_quality_gate.md` and `.json` as stable public reports**.

Rationale: this is the canonical final pass/fail report for the verified agent-use output contract. `.gitignore` exceptions were added for these two files only; raw logs and smoke artifacts remain under ignored audit artifacts.

## Required Gates

| Gate | Status |
| --- | --- |
| cargo_build_workspace | PASS |
| cargo_test_workspace | PASS |
| release_build | PASS |
| context_pack_source_role_tests | PASS |
| query_agent_json_tests | PASS |
| index_json_concise_tests | PASS |
| index_explain_scope_tests | PASS |
| cli_global_flag_tests | PASS |
| agent_json_schema_tests | PASS |
| isolated_self_use_smoke | PASS |
| docs_link_and_hygiene_checks | PASS |
| production_context_excludes_inline_tests | PASS |
| test_impact_includes_inline_tests | PASS |
| query_symbols_limit_or_targeted_correction | PASS |
| agent_json_outputs_bounded | PASS |
| index_json_concise_by_default | PASS |
| audit_scope_only_explicit | PASS |
| global_flag_after_command_targeted | PASS |

## Isolated Self-Use Smoke

- Release binary: `target/release/codegraph-mcp.exe`
- Fixture repo: `C:\Users\wamin\Desktop\development\codegraph-mcp\reports\audit\artifacts\agent_use_quality_gate\isolated_smoke\repo`
- External DB: `C:\Users\wamin\Desktop\development\codegraph-mcp\reports\audit\artifacts\agent_use_quality_gate\isolated_smoke\agent_use_quality_gate.sqlite`
- Audit DB: `C:\Users\wamin\Desktop\development\codegraph-mcp\reports\audit\artifacts\agent_use_quality_gate\isolated_smoke\agent_use_quality_gate_explain_scope.sqlite`
- Normal `.codegraph` DB created: `False`
- DB under fixture repo: `False`
- SQLite files inside fixture repo: `0`

Output sizes:

| Command | Bytes |
| --- | ---: |
| release `--help` | 1818 |
| index `--agent-json` | 2248 |
| index `--json` concise | 2543 |
| query symbols `--agent-json --limit 5` | 3181 |
| context-pack production `--agent-json` | 3863 |
| context-pack test-impact `--agent-json` | 6923 |
| index `--json --explain-scope` | 3251 |

Evidence classification:

- Production context evidence roles: `production`
- Test-impact evidence roles: `mixed, unknown`
- Inline tests appear in production mode: `False`
- Test-impact includes inline test evidence intentionally: `True`

CLI UX:

- Global flags before command worked: `True`
- Global `--db` after query returned targeted correction: `True`

## Commands

| Name | Status | Exit | Wall ms | stdout | stderr |
| --- | --- | ---: | ---: | --- | --- |
| read_sequence_reports | pass | 0 | 411.833 | read_sequence_reports.stdout.log | read_sequence_reports.stderr.log |
| cargo_build_workspace | pass | 0 | 155.378 | cargo_build_workspace.stdout.log | cargo_build_workspace.stderr.log |
| cargo_test_workspace | pass | 0 | 60516.152 | cargo_test_workspace.stdout.log | cargo_test_workspace.stderr.log |
| cargo_build_release_codegraph_mcp | pass | 0 | 143.086 | cargo_build_release_codegraph_mcp.stdout.log | cargo_build_release_codegraph_mcp.stderr.log |
| test_context_pack_source_role_query_crate_pkg | pass | 0 | 100.192 | test_context_pack_source_role_query_crate_pkg.stdout.log | test_context_pack_source_role_query_crate_pkg.stderr.log |
| test_context_pack_cli_unit_surface | pass | 0 | 2425.175 | test_context_pack_cli_unit_surface.stdout.log | test_context_pack_cli_unit_surface.stderr.log |
| test_context_pack_cli_surface | pass | 0 | 1723.843 | test_context_pack_cli_surface.stdout.log | test_context_pack_cli_surface.stderr.log |
| test_query_symbols_agent_json_unit | pass | 0 | 1155.505 | test_query_symbols_agent_json_unit.stdout.log | test_query_symbols_agent_json_unit.stderr.log |
| test_query_text_files_agent_json_unit | pass | 0 | 1184.474 | test_query_text_files_agent_json_unit.stdout.log | test_query_text_files_agent_json_unit.stderr.log |
| test_callers_callees_agent_json_unit | pass | 0 | 1044.083 | test_callers_callees_agent_json_unit.stdout.log | test_callers_callees_agent_json_unit.stderr.log |
| test_index_json_concise_unit | pass | 0 | 809.234 | test_index_json_concise_unit.stdout.log | test_index_json_concise_unit.stderr.log |
| test_index_json_explain_scope_unit | pass | 0 | 2715.73 | test_index_json_explain_scope_unit.stdout.log | test_index_json_explain_scope_unit.stderr.log |
| test_warm_noop_index_json_size_unit | pass | 0 | 988.881 | test_warm_noop_index_json_size_unit.stdout.log | test_warm_noop_index_json_size_unit.stderr.log |
| test_index_agent_json_schema_unit | pass | 0 | 817.288 | test_index_agent_json_schema_unit.stdout.log | test_index_agent_json_schema_unit.stderr.log |
| test_global_flag_placement_cli_smoke | pass | 0 | 1542.929 | test_global_flag_placement_cli_smoke.stdout.log | test_global_flag_placement_cli_smoke.stderr.log |
| test_query_parser_global_flags_unit | pass | 0 | 164.352 | test_query_parser_global_flags_unit.stdout.log | test_query_parser_global_flags_unit.stderr.log |
| test_doctor_global_flag_error_unit | pass | 0 | 156.958 | test_doctor_global_flag_error_unit.stdout.log | test_doctor_global_flag_error_unit.stderr.log |
| test_agent_json_schema_files_unit | pass | 0 | 158.794 | test_agent_json_schema_files_unit.stdout.log | test_agent_json_schema_files_unit.stderr.log |
| docs_check_readme_artifacts | pass | 0 | 81.833 | docs_check_readme_artifacts.stdout.log | docs_check_readme_artifacts.stderr.log |
| docs_check_markdown_links | pass | 0 | 106.79 | docs_check_markdown_links.stdout.log | docs_check_markdown_links.stderr.log |
| docs_check_hygiene | pass | 0 | 116.813 | docs_check_hygiene.stdout.log | docs_check_hygiene.stderr.log |
| isolated_smoke_setup_abs | pass | 0 | 399.942 | isolated_smoke_setup_abs.stdout.log | isolated_smoke_setup_abs.stderr.log |
| isolated_self_use_smoke_abs_matrix | pass | 0 | 2478.011 | isolated_self_use_smoke_abs_matrix.stdout.log | isolated_self_use_smoke_abs_matrix.stderr.log |
| isolated_smoke_path_check | pass | 0 | 359.902 | isolated_smoke_path_check.stdout.log | isolated_smoke_path_check.stderr.log |
| smoke_abs_release_help | pass | 0 | 32.235 | smoke_abs_release_help.stdout.log | smoke_abs_release_help.stderr.log |
| smoke_abs_index_agent_json_fresh | pass | 0 | 651.347 | smoke_abs_index_agent_json_fresh.stdout.log | smoke_abs_index_agent_json_fresh.stderr.log |
| smoke_abs_index_json_concise_warm | pass | 0 | 177.191 | smoke_abs_index_json_concise_warm.stdout.log | smoke_abs_index_json_concise_warm.stderr.log |
| smoke_abs_query_symbols_agent_json_global_before | pass | 0 | 255.228 | smoke_abs_query_symbols_agent_json_global_before.stdout.log | smoke_abs_query_symbols_agent_json_global_before.stderr.log |
| smoke_abs_context_pack_production_agent_json | pass | 0 | 132.714 | smoke_abs_context_pack_production_agent_json.stdout.log | smoke_abs_context_pack_production_agent_json.stderr.log |
| smoke_abs_context_pack_test_impact_agent_json | pass | 0 | 137.471 | smoke_abs_context_pack_test_impact_agent_json.stdout.log | smoke_abs_context_pack_test_impact_agent_json.stderr.log |
| smoke_abs_index_json_explain_scope_audit | pass | 0 | 640.993 | smoke_abs_index_json_explain_scope_audit.stdout.log | smoke_abs_index_json_explain_scope_audit.stderr.log |
| smoke_abs_query_symbols_global_after_targeted_error | fail | 1 | 14.878 | smoke_abs_query_symbols_global_after_targeted_error.stdout.log | smoke_abs_query_symbols_global_after_targeted_error.stderr.log |

Raw command logs are under `reports/audit/artifacts/agent_use_quality_gate/logs/`.

## Prior Reports Read

Read sequence report count: `11`.

## Claim Boundaries

- No final intended-performance pass is claimed from this gate.
- No CodeGraph vs CGC superiority claim is made.
- No real-world recall claim is made.
- Manual relation precision remains sampled precision only unless separate stable reports prove otherwise.
- Absent proof-mode relations have no precision claim from this gate.

## Final Git Status

```text
M .gitignore
 M README.md
 M crates/codegraph-cli/src/lib.rs
 M crates/codegraph-cli/tests/cli_smoke.rs
 M crates/codegraph-core/src/kinds.rs
 M crates/codegraph-core/src/lib.rs
 M crates/codegraph-core/src/model.rs
 M crates/codegraph-parser/src/lib.rs
 M crates/codegraph-query/src/lib.rs
 M docs/cli-reference.md
 M docs/mcp-reference.md
 M docs/operational-profiles.md
 M docs/troubleshooting.md
 M scripts/check_docs_hygiene.py
?? docs/agent-json.md
?? docs/agent-use.md
?? docs/schemas/
?? reports/final/agent_use_quality_gate.json
?? reports/final/agent_use_quality_gate.md
```
