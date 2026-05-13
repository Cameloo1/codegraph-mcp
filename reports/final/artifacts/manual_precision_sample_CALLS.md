# Edge Sample Audit

Database: `reports/final/artifacts/comprehensive_proof_1778642765060.sqlite`

Relation filter: `CALLS`

Limit: `50`

Seed: `41`

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
- edge_id: `edge-key:-9219686753999769194`
- head: `repo://e/f353f7a021403b74753342fee5fb79aa` (`respondApprovalRequest`)
- relation: `CALLS`
- tail: `repo://e/af75cf852a747d194ec30a236ff188b7` (`listApprovalRequests`)
- source_span: `services/orchestrator/src/approval-store.mjs:43-43`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:485f29268cd8e8eb`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
41: 
42: export async function respondApprovalRequest(repoRoot, approvalId, decision, reason = "") {
43:   const approvals = await listApprovalRequests(repoRoot);
44:   const index = approvals.findIndex((item) => item.approval_id === approvalId);
45:   if (index === -1) {
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
- edge_id: `edge-key:-9218329364481355278`
- head: `repo://e/df4541a73c7c7ecdfab2b77f013df3db` (`pre_sampling_compact_runs_on_switch_to_smaller_context_model`)
- relation: `CALLS`
- tail: `repo://e/f91751499a70ff441a9e002f367e6cf6` (`assert_compaction_uses_turn_lifecycle_id`)
- source_span: `runtime/arl-codex/codex-rs/core/tests/suite/compact.rs:2048-2048`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:1b0601dd0410e139`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2046:         .await
2047:         .expect("submit second user turn");
2048:     assert_compaction_uses_turn_lifecycle_id(&test.codex).await;
2049: 
2050:     let requests = request_log.requests();
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
- edge_id: `edge-key:-9207033643377571536`
- head: `repo://e/f8c031010d77b297257b26248308ae49` (`manual_compact_uses_custom_prompt`)
- relation: `CALLS`
- tail: `repo://e/47e31767d0dbb3eff725331fd9c83239` (`non_openai_model_provider`)
- source_span: `runtime/arl-codex/codex-rs/core/tests/suite/compact.rs:703-703`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:1b0601dd0410e139`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
701:     let custom_prompt = "Use this compact prompt instead";
702: 
703:     let model_provider = non_openai_model_provider(&server);
704:     let mut builder = test_codex().with_config(move |config| {
705:         config.model_provider = model_provider;
```

## Sample 4

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
- edge_id: `edge-key:-9204612225682906844`
- head: `repo://e/27656b159d54d78d111f2fb148e040ab` (`thread_rollback_recomputes_previous_turn_settings_and_reference_context_from_replay`)
- relation: `CALLS`
- tail: `repo://e/3f1602df5fe97e53d17d17be955890c5` (`assistant_message`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/tests.rs:2224-2224`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9ef4f9f90c663f41`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2222:         .expect("turn context should have turn_id");
2223:     let turn_one_user = user_message("turn 1 user");
2224:     let turn_one_assistant = assistant_message("turn 1 assistant");
2225:     let turn_two_user = user_message("turn 2 user");
2226:     let turn_two_assistant = assistant_message("turn 2 assistant");
```

## Sample 5

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
- edge_id: `edge-key:-9203917153979327882`
- head: `repo://e/452c512aea4ba41584a962f30cc8421b` (`ensure_source_prerequisites`)
- relation: `CALLS`
- tail: `repo://e/d013e611173856997770c0b76164cc9b` (`require_command`)
- source_span: `upstream/openai-codex/tools/argument-comment-lint/wrapper_common.py:189-197`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5d8a582c9b31529f`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
187:         "  cargo install --locked cargo-dylint dylint-link",
188:     )
189:     require_command(
190:         "rustup",
191:         "argument-comment-lint source wrapper requires rustup.\n"
192:         f"Install the {TOOLCHAIN_CHANNEL} toolchain with:\n"
193:         f"  rustup toolchain install {TOOLCHAIN_CHANNEL} \\\n"
194:         "    --component llvm-tools-preview \\\n"
195:         "    --component rustc-dev \\\n"
196:         "    --component rust-src",
197:     )
198:     toolchains = run_capture(["rustup", "toolchain", "list"], env=env)
199:     if not any(line.startswith(TOOLCHAIN_CHANNEL) for line in toolchains.splitlines()):
```

## Sample 6

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
- edge_id: `edge-key:-9203193041256805099`
- head: `repo://e/4dde9772c13d887f6158b3cb23f05e51` (`emit_turn_item_in_plan_mode`)
- relation: `CALLS`
- tail: `repo://e/9e08a923bfb19c490651902c569246f7` (`emit_agent_message_in_plan_mode`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/turn.rs:1749-1749`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:ba16f39d3aae7e75`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1747:     match turn_item {
1748:         TurnItem::AgentMessage(agent_message) => {
1749:             emit_agent_message_in_plan_mode(sess, turn_context, agent_message, state).await;
1750:         }
1751:         _ => {
```

## Sample 7

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
- edge_id: `edge-key:-9200221164749853995`
- head: `repo://e/a834d4979bd40b385462a7c8de111366` (`runAppServerObjective`)
- relation: `CALLS`
- tail: `repo://e/c47b432c50a38310098e107afa661dca` (`walkFiles`)
- source_span: `services/orchestrator/src/run-manager.mjs:213-213`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:67b86b73b8254b08`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
211: 
212:     const hashes = {};
213:     for (const file of await walkFiles(run.runRoot)) {
214:       const relative = path.relative(run.runRoot, file).replaceAll("\\", "/");
215:       if (relative === "hashes.json") continue;
```

## Sample 8

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
- edge_id: `edge-key:-9194174111317313219`
- head: `repo://e/aee869929cad097078054296d1bc8be2` (`connection_closed_clears_registered_request_contexts`)
- relation: `CALLS`
- tail: `repo://e/79f829d4591c37c53e72203556bf944b` (`register_request_context`)
- source_span: `runtime/arl-codex/codex-rs/app-server/src/outgoing_message.rs:1122-1127`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:ce36408d6991fa2d`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1120:         };
1121: 
1122:         outgoing
1123:             .register_request_context(RequestContext::new(
1124:                 closed_connection_request,
1125:                 tracing::info_span!("app_server.request", rpc.method = "turn/interrupt"),
1126:                 /*parent_trace*/ None,
1127:             ))
1128:             .await;
1129:         outgoing
```

## Sample 9

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
- edge_id: `edge-key:-9191016095031720750`
- head: `repo://e/d2f28d388b8d6b456368202203ceb0cf` (`manual_compact_retries_after_context_window_error`)
- relation: `CALLS`
- tail: `repo://e/57d691bcfa1187b91384cc6275c69473` (`set_test_compact_prompt`)
- source_span: `runtime/arl-codex/codex-rs/core/tests/suite/compact.rs:2378-2378`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:1b0601dd0410e139`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2376:     let mut builder = test_codex().with_config(move |config| {
2377:         config.model_provider = model_provider;
2378:         set_test_compact_prompt(config);
2379:         config.model_auto_compact_token_limit = Some(200_000);
2380:     });
```

## Sample 10

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
- edge_id: `edge-key:-9189397946054752705`
- head: `repo://e/55c2be7a3b816ed8a709ed88e8c11936` (`test_environment_rejects_sandboxed_filesystem_without_runtime_paths`)
- relation: `CALLS`
- tail: `repo://e/a12fabda04a7334d3907a5bb79d0e897` (`get_filesystem`)
- source_span: `upstream/openai-codex/codex-rs/exec-server/src/environment.rs:567-568`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6dbf4f6e17fd9efd`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
565:         );
566: 
567:         let err = environment
568:             .get_filesystem()
569:             .read_file(&path, Some(&sandbox))
570:             .await
```

## Sample 11

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
- edge_id: `edge-key:-9188943476723159600`
- head: `repo://e/82981122f21b1c7c271bf4f71dff4a1e` (`render`)
- relation: `CALLS`
- tail: `repo://e/afb22024d68b612c5c6ee74d2771a1b2` (`renderList`)
- source_span: `packages/ui-autoresearch/src/app.js:147-152`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:52a5b8c00049eaea`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
145:   `;
146: 
147:   renderList("evidence-health-detail", [
148:     { label: "label", value: detail.evidence_health?.label ?? "unknown" },
149:     { label: "unknowns", value: unknownClaimCount(detail) },
150:     { label: "claims", value: detail.claims?.length ?? 0 },
151:     { label: "evidence", value: detail.evidence?.length ?? 0 }
152:   ], (row) => `<div class="row"><b>${escapeHtml(row.label)}</b><span>${escapeHtml(row.value)}</span></div>`);
153:   renderList("claims", detail.claims ?? [], (claim) => `<div class="row"><b>${escapeHtml(claim.status)}</b><span>${escapeHtml(claim.text)}</span></div>`);
154:   renderList("evidence", detail.evidence ?? [], (item) => `<div class="row"><b>${escapeHtml(item.evidence_id)}</b><span>${escapeHtml(item.summary)} (${escapeHtml(item.strength)})</span></div>`);
```

## Sample 12

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
- edge_id: `edge-key:-9180476430850508159`
- head: `repo://e/a9ff49a6db1f03f97e72eef0139e1dcb` (`histogram_sum`)
- relation: `CALLS`
- tail: `repo://e/9aba5d174a51897a4425341cecd4add8` (`find_metric`)
- source_span: `upstream/openai-codex/codex-rs/core/src/session/tests.rs:238-238`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3df730b4b3fcd1f0`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
236: 
237: fn histogram_sum(resource_metrics: &ResourceMetrics, name: &str) -> u64 {
238:     let metric = find_metric(resource_metrics, name);
239:     match metric.data() {
240:         AggregatedMetrics::F64(data) => match data {
```

## Sample 13

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
- edge_id: `edge-key:-9175448200043001236`
- head: `repo://e/0c1ccdf49c1e9aeeb79d78e378415710` (`legacy_non_tty_powershell_emits_output`)
- relation: `CALLS`
- tail: `repo://e/9c656e690399dd0e70249a476165f5c4` (`collect_stdout_and_exit`)
- source_span: `runtime/arl-codex/codex-rs/windows-sandbox-rs/src/unified_exec/tests.rs:218-218`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:adb1bc7ba0597be9`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
216:         println!("pwsh spawn returned");
217:         let (stdout, exit_code) =
218:             collect_stdout_and_exit(spawned, codex_home.path(), Duration::from_secs(10)).await;
219:         println!("pwsh collect returned exit_code={exit_code}");
220:         let stdout = String::from_utf8_lossy(&stdout);
```

## Sample 14

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
- edge_id: `edge-key:-9173920181625997413`
- head: `repo://e/82981122f21b1c7c271bf4f71dff4a1e` (`render`)
- relation: `CALLS`
- tail: `repo://e/6070032f045615e1f8f660209494ae43` (`runRows`)
- source_span: `packages/ui-autoresearch/src/app.js:138-138`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:52a5b8c00049eaea`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
136:     </dl>
137:   `;
138:   document.getElementById("run-list").innerHTML = runs.length === 0 ? "<div class=\"empty\">No runs yet</div>" : runRows(runs);
139:   document.getElementById("objective-detail-panel").innerHTML = `
140:     <dl>
```

## Sample 15

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
- edge_id: `edge-key:-9173892207857001214`
- head: `repo://e/7222436ec99b2a2b0a6160f8227ae759` (`make_session_with_history_source_and_agent_control_and_rx`)
- relation: `CALLS`
- tail: `repo://e/d7c94fe6402d241308c79d3cd958202d` (`build_test_config`)
- source_span: `upstream/openai-codex/codex-rs/core/src/session/tests.rs:4084-4084`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3df730b4b3fcd1f0`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
4082: ) -> anyhow::Result<(Arc<Session>, async_channel::Receiver<Event>)> {
4083:     let codex_home = tempfile::tempdir().expect("create temp dir");
4084:     let mut config = build_test_config(codex_home.path()).await;
4085:     config.ephemeral = true;
4086:     let config = Arc::new(config);
```

## Sample 16

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
- edge_id: `edge-key:-9157570605903685370`
- head: `repo://e/306274c0e8bba5338a8951669d32c635` (`external_goal_mutation_accounts_active_turn_before_status_change`)
- relation: `CALLS`
- tail: `repo://e/34802c2349b413668514916807278800` (`set_total_token_usage`)
- source_span: `upstream/openai-codex/codex-rs/core/src/session/tests.rs:7966-7966`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3df730b4b3fcd1f0`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
7964:     )
7965:     .await;
7966:     set_total_token_usage(&sess, post_goal_token_usage()).await;
7967: 
7968:     sess.goal_runtime_apply(GoalRuntimeEvent::ExternalMutationStarting)
```

## Sample 17

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
- edge_id: `edge-key:-9155260334438884468`
- head: `repo://e/1c51ecf7122d1f0803d9b358e87aee06` (`exec_resume_accepts_images_after_subcommand`)
- relation: `CALLS`
- tail: `repo://e/f8cdbb07b82e54259e39a7bee0c9f6e7` (`exec_repo_root`)
- source_span: `runtime/arl-codex/codex-rs/exec/tests/suite/resume.rs:490-490`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:f86492bec41b5865`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
488:     let test = test_codex_exec();
489:     let fixture = exec_fixture()?;
490:     let repo_root = exec_repo_root()?;
491: 
492:     let marker = format!("resume-image-{}", Uuid::new_v4());
```

## Sample 18

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
- edge_id: `edge-key:-9154203036610331841`
- head: `repo://e/aee869929cad097078054296d1bc8be2` (`connection_closed_clears_registered_request_contexts`)
- relation: `CALLS`
- tail: `repo://e/23984f32a37c6749497aa019139a6893` (`connection_closed`)
- source_span: `runtime/arl-codex/codex-rs/app-server/src/outgoing_message.rs:1138-1138`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:ce36408d6991fa2d`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1136:         assert_eq!(outgoing.request_context_count().await, 2);
1137: 
1138:         outgoing.connection_closed(ConnectionId(9)).await;
1139: 
1140:         assert_eq!(outgoing.request_context_count().await, 1);
```

## Sample 19

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
- edge_id: `edge-key:-9152681992855170393`
- head: `repo://e/2ce98b1fd830a81db10215877da41cf3` (`createDashboardObjective`)
- relation: `CALLS`
- tail: `repo://e/7f66eaa097d2b28f55465d38defbf959` (`yamlString`)
- source_span: `packages/cli/src/dashboard-server.mjs:156-156`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
154:     throw new Error(`objective already exists: ${id}`);
155:   }
156:   const domain = yamlString(body.domain ?? "code") || "code";
157:   const riskLevel = yamlString(body.risk_level ?? "low") || "low";
158:   const title = yamlString(body.title ?? id) || id;
```

## Sample 20

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
- edge_id: `edge-key:-9152578509063404769`
- head: `repo://e/c7140c01e274147e3291557d11392a08` (`record_context_updates_and_set_reference_context_item`)
- relation: `CALLS`
- tail: `repo://e/daa6c38ab6d6c9ac800cced52ee1916e` (`record_conversation_items`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/mod.rs:2848-2848`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:c800723a11df89d4`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2846:         let turn_context_item = turn_context.to_turn_context_item();
2847:         if !context_items.is_empty() {
2848:             self.record_conversation_items(turn_context, &context_items)
2849:                 .await;
2850:         }
```

## Sample 21

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
- edge_id: `edge-key:-9141581984135587689`
- head: `repo://e/4a2278bc920360444b92b49ebca96a76` (`runCommand`)
- relation: `CALLS`
- tail: `repo://e/94299cf089dadff9c7f593e158039b57` (`runMultiAgentFixture`)
- source_span: `packages/cli/src/arl.mjs:192-192`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
```

## Sample 22

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
- edge_id: `edge-key:-9138460836748129052`
- head: `repo://e/cbfdecc8a82c1898741c5e15535fba66` (`doctor`)
- relation: `CALLS`
- tail: `repo://e/94289fdce685c8c13821b17dc218485b` (`pathExists`)
- source_span: `packages/cli/src/arl.mjs:107-107`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
105:     ["schemas", await pathExists(path.join(repoRoot, "schemas", "objective.schema.json")), true],
106:     ["local_rg", await pathExists(localRg), await pathExists(path.join(repoRoot, ".codex-tools"))],
107:     ["upstream_codex_pin", await pathExists(upstreamRev), true],
108:     ["dry_run_adapter", true, true],
109:     ["arl_codex_exec_binary", Boolean(runtimeExec), true],
```

## Sample 23

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
- edge_id: `edge-key:-9130907523969240159`
- head: `repo://e/361f02fa936d8a6dbf41037d332a191f` (`createDryRunFolder`)
- relation: `CALLS`
- tail: `repo://e/598117e31f42dad0761ef656c8dc7772` (`ensureDir`)
- source_span: `packages/core/src/run-ledger/index.mjs:36-36`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:c546747cf6f02fba`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
34:   const artifactsRoot = path.join(runRoot, "artifacts");
35:   const logsRoot = path.join(runRoot, "logs");
36:   await ensureDir(workspaceRoot);
37:   await ensureDir(artifactsRoot);
38:   await ensureDir(logsRoot);
```

## Sample 24

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
- edge_id: `edge-key:-9126065031123564016`
- head: `repo://e/71c7e766218a84a169139915dd24cc2b` (`make_session_and_context_with_auth_config_home_and_rx`)
- relation: `CALLS`
- tail: `repo://e/d524cdb358439de179cf875ab35cfb3b` (`build_test_config`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/tests.rs:5487-5487`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9ef4f9f90c663f41`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
5485: {
5486:     let (tx_event, rx_event) = async_channel::unbounded();
5487:     let mut config = build_test_config(codex_home).await;
5488:     configure_config(&mut config);
5489:     let state_db = if config.features.enabled(Feature::Goals) {
```

## Sample 25

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
- edge_id: `edge-key:-9120489193624736370`
- head: `repo://e/f038b485046ddd592e1105c9e7c30893` (`cancel_requests_for_thread_cancels_all_thread_requests`)
- relation: `CALLS`
- tail: `repo://e/d2055b54d2a15bd61a9e0fbf525abcb0` (`cancel_requests_for_thread`)
- source_span: `upstream/openai-codex/codex-rs/app-server/src/outgoing_message.rs:1255-1256`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:fa10a97b8a4a53e7`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1253:         let error = internal_error("tracked request cancelled");
1254: 
1255:         outgoing
1256:             .cancel_requests_for_thread(thread_id, Some(error.clone()))
1257:             .await;
1258: 
```

## Sample 26

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
- edge_id: `edge-key:-9112745193504538070`
- head: `repo://e/830d58f1fe45cdee6cd23b825910c4b8` (`refresh_mcp_servers_is_deferred_until_next_turn`)
- relation: `CALLS`
- tail: `repo://e/59da69ab780b97862e581864e6826530` (`make_session_and_context`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/tests.rs:5783-5783`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9ef4f9f90c663f41`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
5781: #[tokio::test]
5782: async fn refresh_mcp_servers_is_deferred_until_next_turn() {
5783:     let (session, turn_context) = make_session_and_context().await;
5784:     let old_token = session.mcp_startup_cancellation_token().await;
5785:     assert!(!old_token.is_cancelled());
```

## Sample 27

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
- edge_id: `edge-key:-9106514634442348822`
- head: `repo://e/310919a015ba1de4d8b1fd471820456a` (`exec_resume_accepts_global_flags_after_subcommand`)
- relation: `CALLS`
- tail: `repo://e/a9172846c0a4bff14cb68403701c917b` (`exec_fixture`)
- source_span: `runtime/arl-codex/codex-rs/exec/tests/suite/resume.rs:329-329`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:f86492bec41b5865`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
327: fn exec_resume_accepts_global_flags_after_subcommand() -> anyhow::Result<()> {
328:     let test = test_codex_exec();
329:     let fixture = exec_fixture()?;
330: 
331:     // Seed a session.
```

## Sample 28

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
- edge_id: `edge-key:-9104049820924482317`
- head: `repo://e/df6b6705e2f1a4b3d74851dc4dafddf9` (`start`)
- relation: `CALLS`
- tail: `repo://e/b4cce79cb3d9fb2a8f1aa80ad9e4208c` (`writeJson`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:217-217`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
215:     manifest.summary.run_artifacts_dir = path.relative(repoRoot, runArtifactsDir).replaceAll("\\", "/");
216:     manifest.updated_at = new Date().toISOString();
217:     await writeJson(manifestPath, manifest);
218: 
219:     await this.appendEvent({
```

## Sample 29

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
- edge_id: `edge-key:-9103749106578594669`
- head: `repo://e/d47f3e303b17f719a467c372a5a50277` (`_handle_connection`)
- relation: `CALLS`
- tail: `repo://e/0a4e7333dada7c893c4c2dc686c84ff3` (`recv_json`)
- source_span: `upstream/openai-codex/scripts/mock_responses_websocket_server.py:119-119`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:464388bfd4fe2d93`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `unknown_not_first_class`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
117: 
118:     # Request 2: expect appended tool output; send final assistant message.
119:     await recv_json("req2")
120:     await send_event(_event_response_created("resp-2"))
121:     await send_event(_event_assistant_message("msg-1", ASSISTANT_TEXT))
```

## Sample 30

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
- edge_id: `edge-key:-9100751433761581788`
- head: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb` (`main`)
- relation: `CALLS`
- tail: `repo://e/0093d78d0bf2639aeb1d0699485665f0` (`verifyCommand`)
- source_span: `packages/cli/src/arl.mjs:707-707`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
705:     ok = await evidenceCheck(repoRoot, rest[0]);
706:   } else if (command === "verify") {
707:     ok = await verifyCommand(repoRoot, subcommand);
708:   } else if (command === "report" && subcommand === "open") {
709:     ok = await reportOpen(repoRoot, rest[0]);
```

## Sample 31

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
- edge_id: `edge-key:-9094616922695647144`
- head: `repo://e/b179fba10a081e94cc35f6b2f486a4d2` (`handle_jsonrpc_stdio`)
- relation: `CALLS`
- tail: `repo://e/0c4f86f89e2371fb5ec578b0cedd8fe9` (`json_string_field`)
- source_span: `runtime/arl-codex/shim/src/lib.rs:209-209`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:e7ca90f3674e3e89`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
207:         if json_method(&line, "thread/start") || json_method(&line, "arl.thread.create_for_role") {
208:             let role = json_string_field(&line, "role").unwrap_or_else(|| "agent".to_string());
209:             let objective_id = json_string_field(&line, "objective_id").unwrap_or_else(|| "objective".to_string());
210:             let thread_id = format!("arl-thread-{}-{}", objective_id, thread_counter);
211:             thread_counter += 1;
```

## Sample 32

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
- edge_id: `edge-key:-9084986007038172354`
- head: `repo://e/89fa1db9a1d6bdae12614899385040e8` (`environment_manager_carries_local_runtime_paths`)
- relation: `CALLS`
- tail: `repo://e/fab8fa3cbe59678831dc28d9b669a5ea` (`default_environment`)
- source_span: `upstream/openai-codex/codex-rs/exec-server/src/environment.rs:484-484`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6dbf4f6e17fd9efd`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
482:         .await;
483: 
484:         let environment = manager.default_environment().expect("default environment");
485: 
486:         assert_eq!(environment.local_runtime_paths(), Some(&runtime_paths));
```

## Sample 33

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
- edge_id: `edge-key:-9079661291070287650`
- head: `repo://e/6a4e029cd55feae500eb32329da1d05b` (`thread_rollback_persists_marker_and_replays_cumulatively`)
- relation: `CALLS`
- tail: `repo://e/3f1602df5fe97e53d17d17be955890c5` (`assistant_message`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/tests.rs:2451-2451`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9ef4f9f90c663f41`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2449:         RolloutItem::TurnContext(turn_context_item.clone()),
2450:         RolloutItem::ResponseItem(user_message("turn 1 user")),
2451:         RolloutItem::ResponseItem(assistant_message("turn 1 assistant")),
2452:         RolloutItem::EventMsg(EventMsg::TurnComplete(TurnCompleteEvent {
2453:             turn_id: "turn-1".to_string(),
```

## Sample 34

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
- edge_id: `edge-key:-9079232366376572273`
- head: `repo://e/0adf36ba28e917588f7acf5126fc6afa` (`refreshRunArtifacts`)
- relation: `CALLS`
- tail: `repo://e/e31966b4c2aac33b96763852a5994845` (`readJson`)
- source_span: `packages/core/src/run-ledger/finalize.mjs:31-31`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:4a367677c0432073`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
29:   }
30:   const hypotheses = await readJson(path.join(runRoot, "artifacts", "hypotheses.json"));
31:   const experimentPlan = await readJson(path.join(runRoot, "artifacts", "experiment_plan.json"));
32:   const evidenceHealth = await computeEvidenceHealth({ claims, evidence, runRoot });
33:   const verifierReport = await verifyRun(runRoot);
```

## Sample 35

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
- edge_id: `edge-key:-9072974522009405318`
- head: `repo://e/4c76da96cbc9b710fbd350cc52825ab6` (`thread_start_jsonrpc_span_exports_server_span_and_parents_children`)
- relation: `CALLS`
- tail: `repo://e/32f82373dab6755a18c8eae6743e6950` (`span_attr`)
- source_span: `runtime/arl-codex/codex-rs/app-server/src/message_processor_tracing_tests.rs:610-610`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:c4e09e8213d08dbd`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
608:                 spans.iter().any(|span| {
609:                     span.span_kind == SpanKind::Server
610:                         && span_attr(span, "rpc.method") == Some("thread/start")
611:                 })
612:             })
```

## Sample 36

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
- edge_id: `edge-key:-9064041375540811618`
- head: `repo://e/3063cd96eee6aaed9e338b5ce3f8014b` (`auto_compact_persists_rollout_entries`)
- relation: `CALLS`
- tail: `repo://e/d992ce7093765c57b178368221d5fab1` (`body_contains_text`)
- source_span: `runtime/arl-codex/codex-rs/core/tests/suite/compact.rs:2232-2232`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:1b0601dd0410e139`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2230:         body.contains(FIRST_AUTO_MSG)
2231:             && !body.contains(SECOND_AUTO_MSG)
2232:             && !body_contains_text(body, SUMMARIZATION_PROMPT)
2233:     };
2234:     mount_sse_once_match(&server, first_matcher, sse1).await;
```

## Sample 37

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
- edge_id: `edge-key:-9059168678235437306`
- head: `repo://e/b0310d06dd2e039dad0d22ddd62f876b` (`thread_start_jsonrpc_span_exports_server_span_and_parents_children`)
- relation: `CALLS`
- tail: `repo://e/f5a6a655a85ad0e1d87851c5d44a1d9f` (`span_attr`)
- source_span: `upstream/openai-codex/codex-rs/app-server/src/message_processor_tracing_tests.rs:601-601`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:f019e1cb6e1969fd`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
599:                 spans.iter().any(|span| {
600:                     span.span_kind == SpanKind::Server
601:                         && span_attr(span, "rpc.method") == Some("thread/start")
602:                         && span.span_context.trace_id() == remote_trace_id
603:                 }) && spans.iter().any(|span| {
```

## Sample 38

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
- edge_id: `edge-key:-9057137899338545144`
- head: `repo://e/b5516c43f4d05a6671612cb2e9e9e2bc` (`main`)
- relation: `CALLS`
- tail: `repo://e/d7a306194d826f23220f8826d5920fd9` (`_serve`)
- source_span: `upstream/openai-codex/scripts/mock_responses_websocket_server.py:189-189`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:464388bfd4fe2d93`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `unknown_not_first_class`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
187: 
188:     try:
189:         return asyncio.run(_serve(args.port))
190:     except KeyboardInterrupt:
191:         return 0
```

## Sample 39

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
- edge_id: `edge-key:-9053502937740038339`
- head: `repo://e/d47f3e303b17f719a467c372a5a50277` (`_handle_connection`)
- relation: `CALLS`
- tail: `repo://e/e681fdf8c5715ed071b9d5313454e446` (`_utc_iso`)
- source_span: `upstream/openai-codex/scripts/mock_responses_websocket_server.py:124-124`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:464388bfd4fe2d93`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `unknown_not_first_class`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
122:     await send_event(_event_response_completed("resp-2"))
123: 
124:     sys.stdout.write(f"[conn] {_utc_iso()} closing\n")
125:     sys.stdout.flush()
126:     await websocket.close()
```

## Sample 40

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
- edge_id: `edge-key:-9052519494226975129`
- head: `repo://e/0953a74b92e8d20c6e80485332c6cc9e` (`build_initial_context_emits_thread_start_skill_warning_on_repeated_builds`)
- relation: `CALLS`
- tail: `repo://e/e45426aca7659c6ca5c1e437d784fd06` (`make_session_and_context_with_rx`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/tests.rs:6474-6474`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9ef4f9f90c663f41`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
6472: #[tokio::test]
6473: async fn build_initial_context_emits_thread_start_skill_warning_on_repeated_builds() {
6474:     let (session, turn_context, rx) = make_session_and_context_with_rx().await;
6475:     let mut turn_context = Arc::into_inner(turn_context).expect("sole turn context owner");
6476:     let mut outcome = SkillLoadOutcome::default();
```

## Sample 41

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
- edge_id: `edge-key:-9050224252766646807`
- head: `repo://e/df4541a73c7c7ecdfab2b77f013df3db` (`pre_sampling_compact_runs_on_switch_to_smaller_context_model`)
- relation: `CALLS`
- tail: `repo://e/260c096cd686fd12d30bbe672a103f48` (`disabled_permission_user_turn`)
- source_span: `runtime/arl-codex/codex-rs/core/tests/suite/compact.rs:2041-2045`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:1b0601dd0410e139`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2039: 
2040:     test.codex
2041:         .submit(disabled_permission_user_turn(
2042:             "after switch",
2043:             test.cwd.path().to_path_buf(),
2044:             next_model.to_string(),
2045:         ))
2046:         .await
2047:         .expect("submit second user turn");
```

## Sample 42

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
- edge_id: `edge-key:-9048160255253830301`
- head: `repo://e/b179fba10a081e94cc35f6b2f486a4d2` (`handle_jsonrpc_stdio`)
- relation: `CALLS`
- tail: `repo://e/af9a53755cad0ba2f3a9b64277eedd84` (`json_method`)
- source_span: `runtime/arl-codex/shim/src/lib.rs:226-226`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:e7ca90f3674e3e89`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
224:             continue;
225:         }
226:         if json_method(&line, "turn/start") {
227:             let thread_id = json_string_field(&line, "thread_id").unwrap_or_else(|| "arl-thread-unknown".to_string());
228:             let role = json_string_field(&line, "role").unwrap_or_else(|| "agent".to_string());
```

## Sample 43

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
- edge_id: `edge-key:-9046347112424857064`
- head: `repo://e/cfd3be5da4874cbf44804738b38d8229` (`build_settings_update_items_emits_realtime_end_when_session_stops_being_live`)
- relation: `CALLS`
- tail: `repo://e/59da69ab780b97862e581864e6826530` (`make_session_and_context`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/tests.rs:6063-6063`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9ef4f9f90c663f41`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
6061: #[tokio::test]
6062: async fn build_settings_update_items_emits_realtime_end_when_session_stops_being_live() {
6063:     let (session, mut previous_context) = make_session_and_context().await;
6064:     previous_context.realtime_active = true;
6065:     let mut current_context = previous_context
```

## Sample 44

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
- edge_id: `edge-key:-9038502750529297448`
- head: `repo://e/1b3fb031b810207a3f4f4b0371c36df8` (`createDashboardServer`)
- relation: `CALLS`
- tail: `repo://e/76f03199eccb024f07c40f3f563314c5` (`readBody`)
- source_span: `packages/cli/src/dashboard-server.mjs:982-982`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
980:       if (url.pathname === "/api/tool-forge") {
981:         if (request.method === "POST") {
982:           json(response, 200, { tool: await proposeDashboardTool(repoRoot, await readBody(request)) });
983:           return;
984:         }
```

## Sample 45

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
- edge_id: `edge-key:-9037548711169898310`
- head: `repo://e/01b45dbc9f2fb75d0ba28f4a6cb59673` (`summarize_context_three_requests_and_instructions`)
- relation: `CALLS`
- tail: `repo://e/807bed54d1bc86b51698d6df480f8ea3` (`non_openai_model_provider`)
- source_span: `upstream/openai-codex/codex-rs/core/tests/suite/compact.rs:362-362`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9dcd56b047063dfb`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
360: 
361:     // Build config pointing to the mock server and spawn Codex.
362:     let model_provider = non_openai_model_provider(&server);
363:     let mut builder = test_codex().with_config(move |config| {
364:         config.model_provider = model_provider;
```

## Sample 46

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
- edge_id: `edge-key:-9035728481260840138`
- head: `repo://e/8667d8fb1cadbc2f6100461987ca09f5` (`subscribe_running_assistant_turn_count`)
- relation: `CALLS`
- tail: `repo://e/8667d8fb1cadbc2f6100461987ca09f5` (`subscribe_running_assistant_turn_count`)
- source_span: `runtime/arl-codex/codex-rs/app-server/src/message_processor.rs:692-693`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:0a0a002075bc73fa`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
690: 
691:     pub(crate) fn subscribe_running_assistant_turn_count(&self) -> watch::Receiver<usize> {
692:         self.thread_processor
693:             .subscribe_running_assistant_turn_count()
694:     }
695: 
```

## Sample 47

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
- edge_id: `edge-key:-9031276788898217171`
- head: `repo://e/b307a5a2e89893bee70346ba24999998` (`build_initial_context_uses_previous_realtime_state`)
- relation: `CALLS`
- tail: `repo://e/3158761d11f2230e4fcb29dcf45f4b4c` (`developer_input_texts`)
- source_span: `upstream/openai-codex/codex-rs/core/src/session/tests.rs:6108-6108`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3df730b4b3fcd1f0`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
6106:     }
6107:     let resumed_context = session.build_initial_context(&turn_context).await;
6108:     let resumed_developer_texts = developer_input_texts(&resumed_context);
6109:     assert!(
6110:         !resumed_developer_texts
```

## Sample 48

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
- edge_id: `edge-key:-9028314406606205006`
- head: `repo://e/361f02fa936d8a6dbf41037d332a191f` (`createDryRunFolder`)
- relation: `CALLS`
- tail: `repo://e/b4cce79cb3d9fb2a8f1aa80ad9e4208c` (`writeJson`)
- source_span: `packages/core/src/run-ledger/index.mjs:96-96`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:c546747cf6f02fba`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
94:   };
95: 
96:   await writeJson(path.join(artifactsRoot, "hypotheses.json"), hypotheses);
97:   await writeJson(path.join(artifactsRoot, "experiment_plan.json"), experimentPlan);
98:   await writeText(path.join(artifactsRoot, "final-message.md"), "Dry-run completed. No Codex or shell execution occurred.\n");
```

## Sample 49

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
- edge_id: `edge-key:-9027935844295319503`
- head: `repo://e/0a4e7333dada7c893c4c2dc686c84ff3` (`recv_json`)
- relation: `CALLS`
- tail: `repo://e/f7df04396e77c4311d205b24411cb803` (`_print_request`)
- source_span: `upstream/openai-codex/scripts/mock_responses_websocket_server.py:105-105`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:464388bfd4fe2d93`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `unknown_not_first_class`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
103:         else:
104:             payload = json.loads(msg)
105:         _print_request(f"[{label}] recv", payload)
106:         return payload
107: 
```

## Sample 50

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
- edge_id: `edge-key:-9024713504740049076`
- head: `repo://e/820a9f22b558409a725753aafc843e7c` (`fetch_new_review_items`)
- relation: `CALLS`
- tail: `repo://e/8d1ac2e64628bc509f45f3d9ed66594e` (`gh_api_list_paginated`)
- source_span: `upstream/openai-codex/.codex/skills/babysit-pr/scripts/gh_pr_watch.py:533-533`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6b773bf0ddf94c00`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
531: 
532:     issue_payload = gh_api_list_paginated(endpoints["issue_comment"], repo=repo)
533:     review_comment_payload = gh_api_list_paginated(endpoints["review_comment"], repo=repo)
534:     review_payload = gh_api_list_paginated(endpoints["review"], repo=repo)
535: 
```

