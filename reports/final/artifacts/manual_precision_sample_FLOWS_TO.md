# Edge Sample Audit

Database: `reports/final/artifacts/comprehensive_proof_1778642765060.sqlite`

Relation filter: `FLOWS_TO`

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
- edge_id: `edge-key:-9222375146152285250`
- head: `repo://e/4d1adf9fe5d3002ba6965ab435207269` (`expr@10700`)
- relation: `FLOWS_TO`
- tail: `repo://e/ab1a0c4d914223190cba39a7ad64e1f7` (`argument_0@10700`)
- source_span: `tests/dashboard-pathguard.test.mjs:232-232`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3e82e66e8bd26ae4`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
230:     const body = await stream.text();
231:     assert.ok(body.includes("event: arl-event"));
232:     assert.ok(body.includes("objective.validated"));
233:     assert.ok(body.includes("runtime.turn_completed"));
234:     assert.ok(body.includes("event: arl-stream-end"));
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
- edge_id: `edge-key:-9221931757814997061`
- head: `repo://e/b3b0f286d2997efbbc6876e230fe1d25` (`expr@19303`)
- relation: `FLOWS_TO`
- tail: `repo://e/4071e04458c05c816379cbbe6bc4fe53` (`argument_0@19303`)
- source_span: `packages/cli/src/arl.mjs:478-478`
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
476:   console.log(`app_server_shim: ${daemonBinary ? path.relative(repoRoot, daemonBinary).replaceAll("\\", "/") : "missing"}`);
477:   console.log(`bundled_upstream_codex: ${execBinary ? path.relative(repoRoot, execBinary).replaceAll("\\", "/") : "missing"}`);
478:   console.log(`arl_codexd: ${daemonBinary ? path.relative(repoRoot, daemonBinary).replaceAll("\\", "/") : "missing"}`);
479:   console.log(`multi_agent_shim: ${daemonBinary ? path.relative(repoRoot, daemonBinary).replaceAll("\\", "/") : "missing"}`);
480:   console.log(`openrouter_model: ${process.env.OPENROUTER_MODEL ?? "openai/gpt-5.4-mini"}`);
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
- edge_id: `edge-key:-9219109558606463493`
- head: `repo://e/61bcdd3121cbae33fe37bffea5e71729` (`turn_context`)
- relation: `FLOWS_TO`
- tail: `repo://e/fbfbfecac47462353bf82aed162fa3f7` (`argument_0@243817`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/tests.rs:6549-6549`
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
6547:         sess: Arc::clone(&session),
6548:         turn_context: Arc::clone(&turn_context),
6549:         tool_runtime: test_tool_runtime(Arc::clone(&session), Arc::clone(&turn_context)),
6550:         cancellation_token: CancellationToken::new(),
6551:     };
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
- edge_id: `edge-key:-9218944060473864450`
- head: `repo://e/e619730e13e7251ec5e8779d1690c68c` (`expr@8747`)
- relation: `FLOWS_TO`
- tail: `repo://e/f7cf781ba4759e5c68164958bef4c882` (`argument_0@8747`)
- source_span: `tests/appserver-orchestrator.test.mjs:179-182`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:da6d9e5370713a1f`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
177: test("agent planner enforces budgets and rejects handoff cycles", () => {
178:   assert.throws(() => enforceAgentBudget({ budgets: { max_agent_turns: 2 } }, ["A", "B", "C"]));
179:   assert.deepEqual(validateAcyclicHandoffs([
180:     { from: "a", to: "b" },
181:     { from: "b", to: "c" }
182:   ]), { valid: true, error: null });
183:   assert.equal(validateAcyclicHandoffs([
184:     { from: "a", to: "b" },
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
- edge_id: `edge-key:-9218612296043580410`
- head: `repo://e/c7128619a479dd26e36a8f406489b104` (`expr@4408`)
- relation: `FLOWS_TO`
- tail: `repo://e/24b7b5c87ffb0010b0fb911690213aee` (`argument_2@4408`)
- source_span: `tests/runtime-shim-binaries.test.mjs:98-98`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:a0697d447128e0b9`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
96: 
97:   const cli = path.join(repoRoot, "packages", "cli", "src", "arl.mjs");
98:   const status = spawnSync(process.execPath, [cli, "codex", "patch-status"], { encoding: "utf8" });
99:   assert.equal(status.status, 0, status.stderr);
100:   assert.match(status.stdout, /patches=8/);
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
- edge_id: `edge-key:-9218312691263397029`
- head: `repo://e/ab12abc2fba0cf7c5ac9c8961e59dd5a` (`issue_reaction_events`)
- relation: `FLOWS_TO`
- tail: `repo://e/dbdfcd686894ea38a77b9e2dca9e9aae` (`argument_0@19121`)
- source_span: `upstream/openai-codex/.codex/skills/codex-issue-digest/scripts/collect_issue_digest.py:596-596`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6eebdf094d0b4da9`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
594:     issue_reactions = reaction_summary(issue)
595:     issue_reaction_events_summary = reaction_event_summary(
596:         issue_reaction_events, since, until
597:     )
598:     comment_reaction_events_summary = reaction_event_summary(
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
- edge_id: `edge-key:-9217511199534045108`
- head: `repo://e/7a314adec294c5997182322d3c28832f` (`path`)
- relation: `FLOWS_TO`
- tail: `repo://e/4d45eff87355520dbd2395ed650cf3cf` (`argument_0@143022`)
- source_span: `upstream/openai-codex/codex-rs/protocol/src/protocol.rs:3961-3961`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:314a8b7e209ff04e`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
3959:             .get_writable_roots_with_cwd(cwd)
3960:             .iter()
3961:             .any(|root| root.is_path_writable(path))
3962:     }
3963: 
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
- edge_id: `edge-key:-9217311191821069100`
- head: `repo://e/e2e67c24081ec2f9c788286e96157e0f` (`request_id`)
- relation: `FLOWS_TO`
- tail: `repo://e/82be5778b31ae3541136b0bf267448a6` (`argument_0@30011`)
- source_span: `runtime/arl-codex/codex-rs/app-server/src/message_processor.rs:765-765`
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
763:         let result = handle_arl_request(&request.method, params)
764:             .ok_or_else(|| invalid_request(format!("Unknown ARL method: {}", request.method)))?;
765:         self.outgoing.send_jsonrpc_result(request_id, result).await;
766:         Ok(())
767:     }
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
- edge_id: `edge-key:-9217224678085383670`
- head: `repo://e/86475d5418e85bba1af41cd1b377b020` (`host`)
- relation: `FLOWS_TO`
- tail: `repo://e/c47cd4c392d4ac34e3314a7436f0a0e2` (`argument_1@71917`)
- source_span: `upstream/openai-codex/codex-rs/core/src/session/mod.rs:1830-1830`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:cfb27d495e18c3a4`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1828:             .append_network_rule_and_update(
1829:                 &codex_home,
1830:                 &host,
1831:                 execpolicy_amendment.protocol,
1832:                 execpolicy_amendment.decision,
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
- edge_id: `edge-key:-9216610074181141715`
- head: `repo://e/9628a3b9cb418f795d73d57893c48043` (`expr@10575`)
- relation: `FLOWS_TO`
- tail: `repo://e/2345c879300a99413a5320a240c1eee3` (`argument_1@10575`)
- source_span: `tests/dashboard-pathguard.test.mjs:229-229`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3e82e66e8bd26ae4`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
227:     const stream = await fetch(`${url}api/runs/${encodeURIComponent(runId)}/events/stream`);
228:     assert.equal(stream.status, 200);
229:     assert.match(stream.headers.get("content-type") ?? "", /text\/event-stream/);
230:     const body = await stream.text();
231:     assert.ok(body.includes("event: arl-event"));
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
- edge_id: `edge-key:-9215539168597013560`
- head: `repo://e/67c9a4f5e4f7643608e1edf58aa24278` (`expr@6031`)
- relation: `FLOWS_TO`
- tail: `repo://e/4daa38679147a6bb3b9daaef5a6e18a5` (`validation`)
- source_span: `services/orchestrator/src/freeform-agent-runner.mjs:165-165`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:0c57f99d30ac8b17`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
163:   await ensureDir(root);
164:   await writeJson(objectivePath, objective);
165:   const validation = await validateObjective(objective, repoRoot);
166:   if (!validation.valid) throw new Error(validation.errors.join("\n"));
167:   return {
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
- edge_id: `edge-key:-9214376014152742481`
- head: `repo://e/7937cfe8834dfc99660653bd7e504cbf` (`codex_home`)
- relation: `FLOWS_TO`
- tail: `repo://e/ee200fc9ec2caf228ddac15907c8a330` (`argument_0@51360`)
- source_span: `runtime/arl-codex/codex-rs/app-server/tests/suite/v2/account.rs:1481-1481`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:8ddb33a432c2923f`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1479:     )?;
1480: 
1481:     let mut mcp = McpProcess::new_with_env(codex_home.path(), &[("OPENAI_API_KEY", None)]).await?;
1482:     timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;
1483: 
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
- edge_id: `edge-key:-9214038049252098376`
- head: `repo://e/1414b30e0ba10b092fdb195dbd043d6b` (`expr@9036`)
- relation: `FLOWS_TO`
- tail: `repo://e/ff8865060f42cc46b256ebe188b5b8f0` (`adapter`)
- source_span: `services/orchestrator/src/run-manager.mjs:241-241`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:67b86b73b8254b08`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
239:   }
240: 
241:   const adapter = new DryRunRuntimeAdapter();
242:   const run = await adapter.run({ objective: loaded.data, objectiveRaw: loaded.raw, repoRoot, ...(runId ? { runId } : {}) });
243:   const runtimeBinary = await locateRuntimeBinary(repoRoot, "arl-codex-exec");
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
- edge_id: `edge-key:-9213706667613585830`
- head: `repo://e/ce0782bf6f12b7c162780ad6d01e5b83` (`pr_spec`)
- relation: `FLOWS_TO`
- tail: `repo://e/c091d5ed866c08da45749886bcf877b0` (`argument_0@4767`)
- source_span: `upstream/openai-codex/.codex/skills/babysit-pr/scripts/gh_pr_watch.py:157-157`
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
155: 
156: def resolve_pr(pr_spec, repo_override=None):
157:     parsed = parse_pr_spec(pr_spec)
158:     cmd = ["pr", "view"]
159:     if parsed["value"] is not None:
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
- edge_id: `edge-key:-9213626568789383071`
- head: `repo://e/f187973a5e9390a4312bf5be9c89c962` (`expr@5541`)
- relation: `FLOWS_TO`
- tail: `repo://e/af8e18e2a630bc823b5a26b628700660` (`argument_0@5541`)
- source_span: `services/orchestrator/src/run-manager.mjs:145-149`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:67b86b73b8254b08`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
143:   await writeJson(manifestPath, manifest);
144: 
145:   const session = await createAppServerRunSession({
146:     repoRoot,
147:     runRoot: run.runRoot,
148:     runId: run.runId
149:   });
150:   try {
151:     const completed = session.waitForNotification("turn/completed");
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
- edge_id: `edge-key:-9213424926998488073`
- head: `repo://e/1a3f989d2fd569eb0ce37bcb30bd0995` (`expr@5995`)
- relation: `FLOWS_TO`
- tail: `repo://e/234e1dc6ea6e3f0f18da690d783e5f71` (`argument_1@5995`)
- source_span: `packages/core/src/run-ledger/index.mjs:155-155`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:c546747cf6f02fba`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
153:   const verifierReport = await verifyRun(runRoot);
154:   const report = buildReport({ objective, manifest, hypotheses, experimentPlan, claims, evidence, evidenceHealth, verifierReport });
155:   await writeText(path.join(runRoot, "report.md"), report);
156: 
157:   const graph = buildResearchGraph({ objective, manifest, hypotheses, experimentPlan, claims, evidence });
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
- edge_id: `edge-key:-9212674550788966966`
- head: `repo://e/a3c804188f8a30e0498b270685e5880e` (`runRoot`)
- relation: `FLOWS_TO`
- tail: `repo://e/d0a92c19ca64fe35de01b7a5a31216ff` (`argument_0@4166`)
- source_span: `packages/core/src/preflight/index.mjs:115-115`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:e5d2731bd32d3ab1`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
113:     if (!requiredArtifactsPresent) continue;
114:     const claims = await readJson(path.join(runRoot, "claims.json"));
115:     const evidence = await readJson(path.join(runRoot, "evidence.json"));
116:     const health = await computeEvidenceHealth({ claims, evidence, runRoot });
117:     if (health.label === "invalid") continue;
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
- edge_id: `edge-key:-9211222613764542937`
- head: `repo://e/f106cccdd2afd86daa3bd42fadfc13ab` (`decision`)
- relation: `FLOWS_TO`
- tail: `repo://e/eb8d7381991c7ded541f23344d7650cf` (`argument_0@94968`)
- source_span: `upstream/openai-codex/codex-rs/core/src/session/mod.rs:2420-2420`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:cfb27d495e18c3a4`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2418:         match entry {
2419:             Some(tx_approve) => {
2420:                 tx_approve.send(decision).ok();
2421:             }
2422:             None => {
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
- edge_id: `edge-key:-9210910364687852929`
- head: `repo://e/81ae1bcd5030083c673fd92bc932b2c2` (`expr@14169`)
- relation: `FLOWS_TO`
- tail: `repo://e/595c04ad2e143634994327d6471200f2` (`argument_2@14169`)
- source_span: `packages/ui-autoresearch/src/app.js:320-333`
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
318: function renderApprovals(approvals) {
319:   currentApprovals = approvals;
320:   renderList("approvals", approvals, (approval) => `
321:     <div class="row">
322:       <b>${escapeHtml(approval.status)}</b>
323:       <span>
324:         ${escapeHtml(approval.command_or_tool)} ${escapeHtml(approval.policy_reason)}
325:         ${approval.status === "pending" ? `
326:           <span class="approval-actions">
327:             <button type="button" data-approval="${escapeHtml(approval.approval_id)}" data-decision="approve_once">Approve</button>
328:             <button type="button" data-approval="${escapeHtml(approval.approval_id)}" data-decision="deny">Deny</button>
329:           </span>
330:         ` : ""}
331:       </span>
332:     </div>
333:   `);
334: }
335: 
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
- edge_id: `edge-key:-9210083010549630671`
- head: `repo://e/22156f9f9ec4493b447af9e2c1ec216d` (`expr@20346`)
- relation: `FLOWS_TO`
- tail: `repo://e/87ced54b7a8759afa891d4aa6b620659` (`argument_0@20346`)
- source_span: `packages/cli/src/dashboard-server.mjs:624-624`
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
622:   const eventsPath = path.join(runRoot, "events.jsonl");
623:   if (!(await pathExists(eventsPath))) return 0;
624:   const lines = (await readText(eventsPath)).split(/\r?\n/).filter((line) => line.trim().length > 0);
625:   let max = -1;
626:   for (const line of lines) {
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
- edge_id: `edge-key:-9209467631585878767`
- head: `repo://e/89f9e2a43563f4cf1196b0eade4bb101` (`turn_context`)
- relation: `FLOWS_TO`
- tail: `repo://e/14f09cb3b756073d4e8a3ea8cfe7293f` (`argument_0@83169`)
- source_span: `upstream/openai-codex/codex-rs/core/src/session/turn.rs:2092-2092`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b5f252d6223a2cf5`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2090:                     .swap(true, Ordering::Relaxed)
2091:                 {
2092:                     sess.emit_model_verification(&turn_context, verifications)
2093:                         .await;
2094:                 }
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
- edge_id: `edge-key:-9208946067807989222`
- head: `repo://e/33a7cb3b4e466c7ff693dfb8e17b3535` (`options`)
- relation: `FLOWS_TO`
- tail: `repo://e/96b07b737a806eab8c0e0c4ce0fd3461` (`argument_10@17446`)
- source_span: `runtime/arl-codex/codex-rs/core/src/agent/control.rs:444-444`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:d148db64013f7322`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
442:                 inherited_shell_snapshot,
443:                 inherited_exec_policy,
444:                 options.environments.clone(),
445:             )
446:             .await
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
- edge_id: `edge-key:-9208247948990924910`
- head: `repo://e/c1e15535ef3fec4413f1ee317056ba02` (`expr@20555`)
- relation: `FLOWS_TO`
- tail: `repo://e/7353e5e07c69ef598d1a66edf43115db` (`consoleEvents`)
- source_span: `packages/ui-autoresearch/src/app.js:464-464`
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
462:   const prompt = document.getElementById("agent-prompt").value.trim();
463:   if (!prompt) return;
464:   consoleEvents = [];
465:   addConsoleEvent({
466:     event_id: `event_ui_start_${Date.now()}`,
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
- edge_id: `edge-key:-9207402759074389164`
- head: `repo://e/297fbedeed77761c64bce7b16567f6ea` (`output`)
- relation: `FLOWS_TO`
- tail: `repo://e/9973117aa87bcdf8797ec5676be849a6` (`argument_0@15636`)
- source_span: `upstream/openai-codex/codex-rs/exec/tests/suite/resume.rs:460-460`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:90edadff41114b91`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
458:     assert!(output.status.success(), "resume run failed: {output:?}");
459: 
460:     let stderr = String::from_utf8(output.stderr)?;
461:     assert!(
462:         stderr.contains("model: gpt-5.1-high"),
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
- edge_id: `edge-key:-9207347629358524289`
- head: `repo://e/4b935d52043ae8f625862bde44fdba44` (`expr@18502`)
- relation: `FLOWS_TO`
- tail: `repo://e/ec23f24349c5f71d698d124ba69ce22f` (`title`)
- source_span: `tests/dashboard-pathguard.test.mjs:406-406`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3e82e66e8bd26ae4`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
404: test("dashboard API creates objectives and handles approval decisions", async () => {
405:   const { server, url } = await startDashboardServer({ repoRoot, port: 0 });
406:   const title = `Dashboard Objective ${Date.now()}`;
407:   const objectiveId = title.toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");
408:   const objectivePath = path.join(repoRoot, ".arl", "objectives", `${objectiveId}.yaml`);
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
- edge_id: `edge-key:-9206683191710999248`
- head: `repo://e/561d59bb36b1bbf92ac0f28eaf1c87e3` (`connection_id`)
- relation: `FLOWS_TO`
- tail: `repo://e/e520cd672226356563defd85a8b252f7` (`argument_1@17553`)
- source_span: `upstream/openai-codex/codex-rs/app-server/src/outgoing_message.rs:501-501`
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
499:                 self.send_outgoing_message_to_connection(
500:                     request_context,
501:                     connection_id,
502:                     outgoing_message,
503:                     "response",
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
- edge_id: `edge-key:-9205910137624642660`
- head: `repo://e/5fb2c7c76c049d8bc3da40961de05c4d` (`expr@5931`)
- relation: `FLOWS_TO`
- tail: `repo://e/ee23c1243c5b5c6bf0e6b4474aefbcd5` (`argument_0@5931`)
- source_span: `tests/preflight.test.mjs:120-120`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:14f6cd50d43e1b35`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
118:     await mkdir(path.join(runRoot, "logs"), { recursive: true });
119:     await mkdir(path.join(runRoot, "artifacts"), { recursive: true });
120:     await writeFile(path.join(runRoot, "manifest.json"), JSON.stringify({
121:       updated_at: "2026-05-08T03:00:00.000Z",
122:       runtime: {
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
- edge_id: `edge-key:-9205447429075263982`
- head: `repo://e/536565cf3d973ad62d4826f90acd42e0` (`marketplace_root`)
- relation: `FLOWS_TO`
- tail: `repo://e/b035644a640e5ea63d078ae21a250928` (`argument_0@84288`)
- source_span: `upstream/openai-codex/codex-rs/app-server/src/config/external_agent_config_tests.rs:2386-2388`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:8d6d3d8225093ff3`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2384:     .expect("write settings");
2385:     fs::write(
2386:         marketplace_root
2387:             .join(EXTERNAL_AGENT_PLUGIN_MANIFEST_DIR)
2388:             .join("marketplace.json"),
2389:         r#"{
2390:           "name": "my-plugins",
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
- edge_id: `edge-key:-9205036870288983649`
- head: `repo://e/58b5722668753485f318df17149dfe37` (`expr@8814`)
- relation: `FLOWS_TO`
- tail: `repo://e/f94bd847052ac42d4fc576078e286f7f` (`argument_0@8814`)
- source_span: `tests/dashboard-pathguard.test.mjs:189-192`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3e82e66e8bd26ae4`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
187:       method: "POST",
188:       headers: { "content-type": "application/json" },
189:       body: JSON.stringify({
190:         role: "Reviewer",
191:         input: "Check local evidence links."
192:       })
193:     });
194:     assert.equal(steered.status, 200);
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
- edge_id: `edge-key:-9204501170391386997`
- head: `repo://e/befc52008fa7ce23bae00111ba861f99` (`sess`)
- relation: `FLOWS_TO`
- tail: `repo://e/32f979fe4e9d9ab4789d2d66926051f6` (`argument_0@92202`)
- source_span: `upstream/openai-codex/codex-rs/core/src/session/tests.rs:2534-2534`
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
2532:         .await;
2533: 
2534:     handlers::thread_rollback(&sess, "sub-1".to_string(), /*num_turns*/ 0).await;
2535: 
2536:     let error_event = wait_for_thread_rollback_failed(&rx).await;
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
- edge_id: `edge-key:-9202398831853818219`
- head: `repo://e/078a32dd7ec67175c28eedac5f578e67` (`expr@1034`)
- relation: `FLOWS_TO`
- tail: `repo://e/d12845d50817607b1fe6385b51d9854d` (`argument_1@1034`)
- source_span: `tests/dry-run.test.mjs:23-23`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:8eda92805ec416ac`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
21:     const claims = await readJson(path.join(result.runRoot, "claims.json"));
22:     const evidence = await readJson(path.join(result.runRoot, "evidence.json"));
23:     const verifier = await readJson(path.join(result.runRoot, "verifier_report.json"));
24:     assert.equal(manifest.runtime.kind, "dry_run");
25:     assert.equal(manifest.summary.codex_execution, false);
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
- edge_id: `edge-key:-9201487267676835882`
- head: `repo://e/ba20cfd1fa351198a087b9d06e2d505e` (`expr@5417`)
- relation: `FLOWS_TO`
- tail: `repo://e/474f4ec1fac109e2b7188853ee8ca6d0` (`argument_0@5417`)
- source_span: `tests/release-package.test.mjs:75-75`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:f5d3c5a55f45de2f`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
73:       true
74:     );
75:     const updateChannel = await readJson(path.join(result.release_root, "UPDATE_CHANNEL.json"));
76:     assert.equal(updateChannel.latest_version, "0.1.0");
77:     assert.equal(updateChannel.update_policy.automatic_installs, false);
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
- edge_id: `edge-key:-9201393005633830236`
- head: `repo://e/8322ea4b75fc1f41c4d6a3c5824ee5d3` (`expr@7597`)
- relation: `FLOWS_TO`
- tail: `repo://e/82eda123256993d32647cfa5d68597ed` (`verifierReport`)
- source_span: `services/orchestrator/src/run-manager.mjs:191-191`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:67b86b73b8254b08`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
189:     const experimentPlan = await readJson(path.join(run.runRoot, "artifacts", "experiment_plan.json"));
190:     const evidenceHealth = await computeEvidenceHealth({ claims, evidence, runRoot: run.runRoot });
191:     const verifierReport = await verifyRun(run.runRoot);
192:     const report = buildReport({
193:       objective: loaded.data,
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
- edge_id: `edge-key:-9201119517240278568`
- head: `repo://e/1de4ac7a0b38a7448e397cd7d0b43db6` (`sess`)
- relation: `FLOWS_TO`
- tail: `repo://e/437238e784c979766212c253831f3458` (`argument_0@13019`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/turn.rs:314-314`
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
312:         let response_item: ResponseItem = initial_input_for_turn.clone().into();
313:         let user_prompt_submit_outcome = run_user_prompt_submit_hooks(
314:             &sess,
315:             &turn_context,
316:             UserMessageItem::new(&input).message(),
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
- edge_id: `edge-key:-9201094684161368044`
- head: `repo://e/bd17c8af5d7b49fe5965ed097e114148` (`marketplace_root`)
- relation: `FLOWS_TO`
- tail: `repo://e/60f158aade7197d473c0682aeac55146` (`argument_0@63379`)
- source_span: `upstream/openai-codex/codex-rs/app-server/src/config/external_agent_config_tests.rs:1770-1773`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:8d6d3d8225093ff3`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1768:     .expect("create sample plugin");
1769:     fs::create_dir_all(
1770:         marketplace_root
1771:             .join("plugins")
1772:             .join("available")
1773:             .join(".codex-plugin"),
1774:     )
1775:     .expect("create available plugin");
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
- edge_id: `edge-key:-9200668780656543823`
- head: `repo://e/d4f73e4134ff4ea244dc9ad6ae06fce0` (`expr@5642`)
- relation: `FLOWS_TO`
- tail: `repo://e/55b3a27fc0265c08bf080cb6530e6466` (`argument_0@5642`)
- source_span: `tests/release-package.test.mjs:78-78`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:f5d3c5a55f45de2f`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
76:     assert.equal(updateChannel.latest_version, "0.1.0");
77:     assert.equal(updateChannel.update_policy.automatic_installs, false);
78:     const packageJson = await readJson(path.join(result.release_root, "package.json"));
79:     assert.equal(packageJson.bin.arl, "./packages/cli/src/arl.mjs");
80: 
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
- edge_id: `edge-key:-9200032258411943600`
- head: `repo://e/96c8532956d0e6f1605e9233fe37ccc3` (`harness`)
- relation: `FLOWS_TO`
- tail: `repo://e/e1aaa7ee084f968c34cefd23e3f2ab76` (`argument_0@5421`)
- source_span: `runtime/arl-codex/codex-rs/core/tests/suite/shell_command.rs:170-170`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:004403fc2e3281d0`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
168:     let call_id = "shell-command-call-first-extra-login";
169:     mount_shell_responses(
170:         &harness,
171:         call_id,
172:         "echo 'first line\nsecond line'",
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
- edge_id: `edge-key:-9199228076898234834`
- head: `repo://e/e8311f4ebaddb60cc81d76d4b4e153f6` (`repoRoot`)
- relation: `FLOWS_TO`
- tail: `repo://e/9d52ce65a1d8f80a72f7e94c1b98be84` (`runsList`)
- source_span: `packages/cli/src/arl.mjs:693-693`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `false`
- extractor: `codegraph-index-security-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
691:     ok = await agentRunCommand(repoRoot, rest);
692:   } else if (command === "runs" && subcommand === "list") {
693:     ok = await runsList(repoRoot);
694:   } else if (command === "runs" && subcommand === "show") {
695:     ok = await runsShow(repoRoot, rest[0]);
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
- edge_id: `edge-key:-9199007952973338522`
- head: `repo://e/f5e54054a57791fe50a3e066f7aaaeb8` (`data`)
- relation: `FLOWS_TO`
- tail: `repo://e/7c351f37b4585c74959aee0ccbaf2432` (`argument_0@9808`)
- source_span: `upstream/openai-codex/.codex/skills/babysit-pr/scripts/gh_pr_watch.py:310-310`
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
308:         repo=repo,
309:     )
310:     if not isinstance(data, dict):
311:         raise GhCommandError("Unexpected payload from actions runs API")
312:     runs = data.get("workflow_runs") or []
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
- edge_id: `edge-key:-9198069804142456098`
- head: `repo://e/ae0819ae08e0145ac9b62e8738b922f0` (`expr@2124`)
- relation: `FLOWS_TO`
- tail: `repo://e/ec78031611d4e8662bb760dd9c2652ac` (`argument_0@2124`)
- source_span: `tests/dashboard-pathguard.test.mjs:52-52`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3e82e66e8bd26ae4`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
50:     }));
51:   });
52:   await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
53:   const address = server.address();
54:   return {
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
- edge_id: `edge-key:-9197812045896575897`
- head: `repo://e/c6928085ea2c5d2436eb98af34865372` (`skill_items`)
- relation: `FLOWS_TO`
- tail: `repo://e/80bbe5bb9e12ef6ccacf04e6b6709dba` (`argument_0@54103`)
- source_span: `upstream/openai-codex/codex-rs/core/src/session/tests.rs:1519-1519`
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
1517: 
1518:     let connector_ids =
1519:         collect_explicit_app_ids_from_skill_items(&skill_items, &connectors, &HashMap::new());
1520: 
1521:     assert_eq!(connector_ids, HashSet::from(["calendar".to_string()]));
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
- edge_id: `edge-key:-9196204684331236374`
- head: `repo://e/9bc837c65a26a585b1fef5ccf44641cf` (`marker`)
- relation: `FLOWS_TO`
- tail: `repo://e/17ea590a2a98caea4612c8eaf817747c` (`argument_1@4739`)
- source_span: `upstream/openai-codex/codex-rs/exec/tests/suite/resume.rs:136-136`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:90edadff41114b91`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
134:     // Find the created session file containing the marker.
135:     let sessions_dir = test.home_path().join("sessions");
136:     let path = find_session_file_containing_marker(&sessions_dir, &marker)
137:         .expect("no session file found after first run");
138: 
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
- edge_id: `edge-key:-9195615041322030625`
- head: `repo://e/3e5843eaba4f9bd29839cc68dc25b418` (`expr@13737`)
- relation: `FLOWS_TO`
- tail: `repo://e/82fbfe5882538a185d3fe79d5b6d0502` (`return@13730`)
- source_span: `packages/cli/src/dashboard-server.mjs:442-442`
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
440:   const runRoot = path.join(runsRoot, runId);
441:   if (!isPathInside(runsRoot, runRoot)) {
442:     return [];
443:   }
444:   const artifactRoots = [
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
- edge_id: `edge-key:-9195410717298753584`
- head: `repo://e/a82006d4db3ac907ab0ecba2169c1cfd` (`config`)
- relation: `FLOWS_TO`
- tail: `repo://e/30267d0e2321c23ccb9ef0504e695c02` (`argument_0@19859`)
- source_span: `upstream/openai-codex/codex-rs/core/src/agent/control.rs:504-504`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:1ad2e3c3306c4f37`
- derived: `false`
- extractor: `tree-sitter-language-frontend`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
502:                     match self
503:                         .resume_single_agent_from_rollout(
504:                             config.clone(),
505:                             child_thread_id,
506:                             child_session_source,
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
- edge_id: `edge-key:-9194389825111649952`
- head: `repo://e/b45cc2ed49be8854215bb7a045cafdaa` (`expr@1077`)
- relation: `FLOWS_TO`
- tail: `repo://e/3dcb10f330816534b4777ab87355ea0a` (`argument_0@1077`)
- source_span: `tests/dry-run.test.mjs:24-24`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:8eda92805ec416ac`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
22:     const evidence = await readJson(path.join(result.runRoot, "evidence.json"));
23:     const verifier = await readJson(path.join(result.runRoot, "verifier_report.json"));
24:     assert.equal(manifest.runtime.kind, "dry_run");
25:     assert.equal(manifest.summary.codex_execution, false);
26:     assert.equal(claims.some((claim) => claim.status === "not_tested"), true);
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
- edge_id: `edge-key:-9194239277081897287`
- head: `repo://e/17cd30e50ae24aa4e2e3b565a3cd188e` (`expr@19488`)
- relation: `FLOWS_TO`
- tail: `repo://e/379f25d411890a9a667584adc8b06bcc` (`argument_0@19488`)
- source_span: `packages/ui-autoresearch/src/app.js:440-440`
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
438:   const lowerPath = artifactPath.toLowerCase();
439:   const rawUrl = `/api/runs/${encodeURIComponent(runId)}/artifact-raw?path=${encodeURIComponent(artifactPath)}`;
440:   if (lowerPath.endsWith(".svg") || lowerPath.endsWith(".png") || lowerPath.endsWith(".jpg") || lowerPath.endsWith(".jpeg") || lowerPath.endsWith(".gif")) {
441:     preview.innerHTML = `<img class="artifact-media" src="${escapeHtml(rawUrl)}" alt="${escapeHtml(artifactPath)}">`;
442:     return;
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
- edge_id: `edge-key:-9193421849802213931`
- head: `repo://e/c4299fd352028f537e67b47883720741` (`requested_permissions`)
- relation: `FLOWS_TO`
- tail: `repo://e/f550fd8e5b347b5504f6d57a64a425bf` (`argument_0@91252`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/mod.rs:2319-2319`
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
2317:         RequestPermissionsResponse {
2318:             permissions: intersect_permission_profiles(
2319:                 requested_permissions.into(),
2320:                 response.permissions.into(),
2321:                 cwd,
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
- edge_id: `edge-key:-9192743492400450984`
- head: `repo://e/c3376e97f9d7de50d0d0d79f8ebab8ae` (`expr@1062`)
- relation: `FLOWS_TO`
- tail: `repo://e/537f17dd2cf1a15210e41a4e290ddf4c` (`return@1055`)
- source_span: `packages/core/src/simple-yaml.mjs:39-39`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:7441dc47b2a509a8`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
37:   const key = text.slice(0, colon).trim();
38:   const rest = text.slice(colon + 1).trim();
39:   return [key, rest.length === 0 ? undefined : parseScalar(rest)];
40: }
41: 
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
- edge_id: `edge-key:-9192618089609539689`
- head: `repo://e/b515664e55fc51960c14a3d94d8a457c` (`expr@6521`)
- relation: `FLOWS_TO`
- tail: `repo://e/bb3df4c12603787a9357576c844d51e0` (`argument_2@6521`)
- source_span: `tests/preflight.test.mjs:130-130`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:14f6cd50d43e1b35`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
128:     }), "utf8");
129:     await writeFile(path.join(runRoot, "logs", "openrouter-request.json"), JSON.stringify({ endpoint: "https://openrouter.ai/api/v1/chat/completions" }), "utf8");
130:     await writeFile(path.join(runRoot, "logs", "openrouter-response.json"), JSON.stringify({ model: "openai/gpt-5.4-mini" }), "utf8");
131:     await writeFile(path.join(runRoot, "artifacts", "final-message.md"), "mock agent live smoke response\n", "utf8");
132:     await writeFile(path.join(runRoot, "evidence.json"), JSON.stringify([
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
- edge_id: `edge-key:-9192103583244113947`
- head: `repo://e/9f750a16029c8d2df2cdae04fa34448b` (`turn_context`)
- relation: `FLOWS_TO`
- tail: `repo://e/d8bc50b278d618ea45d186039df0a7b0` (`argument_0@67772`)
- source_span: `runtime/arl-codex/codex-rs/core/src/session/turn.rs:1728-1728`
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
1726:                 })
1727:             });
1728:         sess.emit_turn_item_started(turn_context, &start_item).await;
1729:         state
1730:             .started_agent_message_items
```

