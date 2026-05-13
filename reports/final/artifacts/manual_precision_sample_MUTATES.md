# Edge Sample Audit

Database: `reports/final/artifacts/comprehensive_proof_1778642765060.sqlite`

Relation filter: `MUTATES`

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
- edge_id: `edge-key:-9193168405296192605`
- head: `repo://e/6a486ebec92286854054b12dd817e137` (`runsCancel`)
- relation: `MUTATES`
- tail: `repo://e/8dff277826599effc8318c57c6455fd1` (`expr@13341`)
- source_span: `packages/cli/src/arl.mjs:313-313`
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
311:   }
312:   const manifest = await readJson(manifestPath);
313:   manifest.status = "cancelled";
314:   manifest.updated_at = new Date().toISOString();
315:   manifest.cancellation = {
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
- edge_id: `edge-key:-9103950600910128452`
- head: `repo://e/df6b6705e2f1a4b3d74851dc4dafddf9` (`start`)
- relation: `MUTATES`
- tail: `repo://e/4329eb7f06d145fc14f67e71c65c36a6` (`expr@9599`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:274-274`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
272:       }
273:       const mapped = mapCodexEvent(raw, actualRunId, sequence++);
274:       mapped.payload = { raw };
275:       mappedEvents += 1;
276:       await this.appendEvent({ runRoot, onEvent, event: mapped });
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
- edge_id: `edge-key:-9056603389246153720`
- head: `repo://e/b72741d4810fb3f51c2ee06fd2066433` (`parseArgs`)
- relation: `MUTATES`
- tail: `repo://e/539b32118629c137f10296b749874341` (`expr@1404`)
- source_span: `scripts/package-local-release.mjs:41-41`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:2a30e7e72f3ab481`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
39:       index += 1;
40:     } else if (arg === "--timestamp-server") {
41:       parsed.signing ??= {};
42:       parsed.signing.timestampServer = argv[index + 1];
43:       index += 1;
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
- edge_id: `edge-key:-8921582261850152386`
- head: `repo://e/63ceb4904f1c4ff0cdf0a94c2db1912e` (`run`)
- relation: `MUTATES`
- tail: `repo://e/398ed14687e86e21a3392a9b83fccda7` (`expr@6462`)
- source_span: `packages/core/src/runtime-adapters/openrouter.mjs:198-198`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:ede9c6076a25837e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
196:     manifest.summary.codex_execution = false;
197:     manifest.summary.model_execution = true;
198:     manifest.summary.openrouter_http_status = httpStatus;
199:     manifest.summary.openrouter_usage = responseJson.usage ?? null;
200:     manifest.summary.reasoning_header_used = Boolean(reasoningEffort);
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
- edge_id: `edge-key:-8879937476983064468`
- head: `repo://e/df6b6705e2f1a4b3d74851dc4dafddf9` (`start`)
- relation: `MUTATES`
- tail: `repo://e/5bd70371171248a51e5d375f8618f6e7` (`expr@7199`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:212-212`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
210:     manifest.status = "running";
211:     manifest.summary.dry_run = false;
212:     manifest.summary.codex_execution = true;
213:     manifest.summary.agent_workspace = path.relative(repoRoot, workspace).replaceAll("\\", "/");
214:     manifest.summary.agent_artifacts_dir = path.relative(repoRoot, workspaceArtifactsDir).replaceAll("\\", "/");
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
- edge_id: `edge-key:-8872094823550374238`
- head: `repo://e/69a7fca88c30bf81da6bd2a310292adf` (`constructor`)
- relation: `MUTATES`
- tail: `repo://e/25d667cf7d6a6cee5814326ab2e8b858` (`expr@309`)
- source_span: `packages/appserver-client/src/jsonrpc-client.mjs:11-11`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9f5ceab056a85bfc`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
9:     this.options = options;
10:     this.nextId = 1;
11:     this.pending = new Map();
12:     this.process = null;
13:     this.events = new EventEmitter();
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
- edge_id: `edge-key:-8807554559573846452`
- head: `repo://e/ad34b4b3bfc2c20be5b17731d6039a29` (`startDashboardAgentPromptRun`)
- relation: `MUTATES`
- tail: `repo://e/9f1f6b89ca5cffcb15286a0fe88ffce5` (`expr@29221`)
- source_span: `packages/cli/src/dashboard-server.mjs:865-865`
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
863:     })
864:     .catch((error) => {
865:       activeRun.error = error.message;
866:       activeRun.status = "failed";
867:       return null;
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
- edge_id: `edge-key:-8803594694445583318`
- head: `repo://e/a834d4979bd40b385462a7c8de111366` (`runAppServerObjective`)
- relation: `MUTATES`
- tail: `repo://e/068357fe5edc4ebdd4e8397c1cb4f117` (`expr@5357`)
- source_span: `services/orchestrator/src/run-manager.mjs:141-141`
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
139:   manifest.runtime.kind = "app_server_shim";
140:   manifest.runtime.app_server_protocol_version = "arl-app-server-shim-0.1.0";
141:   manifest.summary.app_server_streaming = true;
142:   manifest.summary.codex_execution = false;
143:   await writeJson(manifestPath, manifest);
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
- edge_id: `edge-key:-8664245944669047952`
- head: `repo://e/f353f7a021403b74753342fee5fb79aa` (`respondApprovalRequest`)
- relation: `MUTATES`
- tail: `repo://e/cd576bcf1669b1f7deaf783759f6a1b9` (`expr@1799`)
- source_span: `services/orchestrator/src/approval-store.mjs:49-49`
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
47:   }
48:   const decided = decideApproval(approvals[index], decision);
49:   decided.decision_reason = reason;
50:   approvals[index] = decided;
51:   await saveApprovals(repoRoot, approvals);
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
- edge_id: `edge-key:-8556075575670837579`
- head: `repo://e/69a7fca88c30bf81da6bd2a310292adf` (`constructor`)
- relation: `MUTATES`
- tail: `repo://e/d59e276f65e429fdca0c827b5ab70cfb` (`expr@238`)
- source_span: `packages/appserver-client/src/jsonrpc-client.mjs:8-8`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9f5ceab056a85bfc`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
6:   constructor(command, args = [], options = {}) {
7:     this.command = command;
8:     this.args = args;
9:     this.options = options;
10:     this.nextId = 1;
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
- edge_id: `edge-key:-8382489760569415909`
- head: `repo://e/7af59e8a472dea9e968d7a1e82badaf4` (`runBundledCodexObjective`)
- relation: `MUTATES`
- tail: `repo://e/3d9acd9e5861ecdc157a84a7cca18486` (`expr@9883`)
- source_span: `services/orchestrator/src/run-manager.mjs:257-257`
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
255:   const manifest = await readJson(manifestPath);
256:   manifest.runtime.kind = "bundled_upstream_codex";
257:   manifest.runtime.codex_source = "bundled_pinned_runtime";
258:   manifest.runtime.binary = path.relative(repoRoot, runtimeBinary).replaceAll("\\", "/");
259:   manifest.summary.bundled_runtime_smoke = true;
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
- edge_id: `edge-key:-8203870345952862866`
- head: `repo://e/a1581d4f129101bef527fe351f553eec` (`createBundleManifest`)
- relation: `MUTATES`
- tail: `repo://e/ff50e06f5ea34142f1bfd9196a3a1240` (`expr@4546`)
- source_span: `packages/core/src/reproducibility/index.mjs:113-113`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:fe8b58ac616f91b1`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
111:     const relative = path.relative(runRoot, file).replaceAll("\\", "/");
112:     if (relative === "bundle_manifest.json") continue;
113:     hashes[relative] = await sha256File(file);
114:   }
115:   const manifest = {
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
- edge_id: `edge-key:-8115490472845560779`
- head: `repo://e/65cd7e8e450b406dc4f0203c711c09c3` (`refreshRunArtifacts`)
- relation: `MUTATES`
- tail: `repo://e/da3d71f05d31253d623b71dd507a3fb7` (`expr@2247`)
- source_span: `services/orchestrator/src/multi-agent-runner.mjs:66-66`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:515d478182d71479`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
64:     if (relative === "hashes.json") continue;
65:     if (!(await pathExists(file))) continue;
66:     hashes[relative] = await sha256File(file);
67:   }
68:   await writeJson(path.join(runRoot, "hashes.json"), hashes);
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
- edge_id: `edge-key:-8110141957325592552`
- head: `repo://e/1492c40ff5bb388717fa41cefe9520e9` (`run`)
- relation: `MUTATES`
- tail: `repo://e/9fd0d4a896df5e7fabe47256510917dd` (`expr@17103`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:484-484`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
482:     manifest.runtime.kind = "external_codex_cli";
483:     manifest.runtime.codex_source = "stock";
484:     manifest.status = result.status === 0 ? "completed" : "failed";
485:     manifest.summary.dry_run = false;
486:     manifest.summary.codex_execution = true;
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
- edge_id: `edge-key:-8082538590738088215`
- head: `repo://e/139717a3d0f8f019e35f1a8a80d70927` (`appendDashboardAgentTurn`)
- relation: `MUTATES`
- tail: `repo://e/d9ac54f3d7b9599c0259664a91d6fd22` (`expr@23229`)
- source_span: `packages/cli/src/dashboard-server.mjs:705-705`
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
703:   const objective = (await loadObjective(path.join(runRoot, "objective.yaml"))).data;
704:   const manifest = await readJson(path.join(runRoot, "manifest.json"));
705:   manifest.summary.dashboard_steering = true;
706:   manifest.updated_at = now;
707:   await writeJson(path.join(runRoot, "manifest.json"), manifest);
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
- edge_id: `edge-key:-7925818858643454051`
- head: `repo://e/82981122f21b1c7c271bf4f71dff4a1e` (`render`)
- relation: `MUTATES`
- tail: `repo://e/f44b0605b8d1d05ad8f1cd47bfc7a95c` (`expr@5359`)
- source_span: `packages/ui-autoresearch/src/app.js:139-145`
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
137:   `;
138:   document.getElementById("run-list").innerHTML = runs.length === 0 ? "<div class=\"empty\">No runs yet</div>" : runRows(runs);
139:   document.getElementById("objective-detail-panel").innerHTML = `
140:     <dl>
141:       <dt>Objective</dt><dd>${escapeHtml(detail.objective?.title ?? detail.objective_title ?? "unknown")}</dd>
142:       <dt>Domain</dt><dd>${escapeHtml(detail.objective?.domain ?? "unknown")}</dd>
143:       <dt>Risk</dt><dd>${escapeHtml(detail.objective?.risk_level ?? "unknown")}</dd>
144:     </dl>
145:   `;
146: 
147:   renderList("evidence-health-detail", [
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
- edge_id: `edge-key:-7905740507405506723`
- head: `repo://e/561e5c217a9f8803461ff05823be1a1d` (`constructor`)
- relation: `MUTATES`
- tail: `repo://e/05e01e20abe74150bc42b82e7de4789e` (`expr@461`)
- source_span: `services/orchestrator/src/appserver-run-session.mjs:11-11`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b57bbd64c72c169e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
9:     this.runId = runId;
10:     this.client = new ArlCodexdClient({ repoRoot, runRoot });
11:     this.sequence = 1;
12:     this.notifications = [];
13:     this.pendingWrites = new Set();
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
- edge_id: `edge-key:-7897862752746996844`
- head: `repo://e/9a43e8f6c0646eeb461a57d356619925` (`loadArtifactContent`)
- relation: `MUTATES`
- tail: `repo://e/43851f0bae456fd62b2682f6b283f640` (`expr@19269`)
- source_span: `packages/ui-autoresearch/src/app.js:437-437`
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
435: async function loadArtifactContent(runId, artifactPath) {
436:   const preview = document.getElementById("artifact-preview");
437:   preview.textContent = "loading";
438:   const lowerPath = artifactPath.toLowerCase();
439:   const rawUrl = `/api/runs/${encodeURIComponent(runId)}/artifact-raw?path=${encodeURIComponent(artifactPath)}`;
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
- edge_id: `edge-key:-7771050652828913538`
- head: `repo://e/69a7fca88c30bf81da6bd2a310292adf` (`constructor`)
- relation: `MUTATES`
- tail: `repo://e/e75f8296dc2cb79ef4db26f9647a0270` (`expr@260`)
- source_span: `packages/appserver-client/src/jsonrpc-client.mjs:9-9`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9f5ceab056a85bfc`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
7:     this.command = command;
8:     this.args = args;
9:     this.options = options;
10:     this.nextId = 1;
11:     this.pending = new Map();
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
- edge_id: `edge-key:-7760635837625246555`
- head: `repo://e/cfdda9ed837e3c34054fd3b7cefacb3a` (`runArlCodexdObjective`)
- relation: `MUTATES`
- tail: `repo://e/698f9b84dc83ac2b780508bff5d9c9e1` (`expr@12244`)
- source_span: `services/orchestrator/src/run-manager.mjs:309-309`
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
307:   const manifestPath = path.join(result.runRoot, "manifest.json");
308:   const manifest = await readJson(manifestPath);
309:   manifest.runtime.kind = "arl_codexd";
310:   manifest.runtime.codex_source = "forked_runtime_shim";
311:   manifest.summary.arl_codexd_runtime = true;
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
- edge_id: `edge-key:-7694672689698756419`
- head: `repo://e/4682d45e5ffbfd081558180867407156` (`parseObject`)
- relation: `MUTATES`
- tail: `repo://e/a76183074cb2b8a9a3f56b60b5937bef` (`expr@1936`)
- source_span: `packages/core/src/simple-yaml.mjs:64-64`
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
62:     if (index < lines.length && lines[index].indent > indent) {
63:       const [child, next] = parseBlock(lines, index, lines[index].indent);
64:       output[key] = child;
65:       index = next;
66:     } else {
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
- edge_id: `edge-key:-7658861457107264948`
- head: `repo://e/b72741d4810fb3f51c2ee06fd2066433` (`parseArgs`)
- relation: `MUTATES`
- tail: `repo://e/bd449bfe8e8f1ac6c57eac5009c862c8` (`expr@1116`)
- source_span: `scripts/package-local-release.mjs:33-33`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:2a30e7e72f3ab481`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
31:       index += 1;
32:     } else if (arg === "--dry-run") {
33:       parsed.dryRun = true;
34:     } else if (arg === "--verify") {
35:       parsed.verify = true;
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
- edge_id: `edge-key:-7586270613344395773`
- head: `repo://e/df6b6705e2f1a4b3d74851dc4dafddf9` (`start`)
- relation: `MUTATES`
- tail: `repo://e/1bee09987264402e9a152fcd56a4e728` (`expr@7341`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:214-214`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
212:     manifest.summary.codex_execution = true;
213:     manifest.summary.agent_workspace = path.relative(repoRoot, workspace).replaceAll("\\", "/");
214:     manifest.summary.agent_artifacts_dir = path.relative(repoRoot, workspaceArtifactsDir).replaceAll("\\", "/");
215:     manifest.summary.run_artifacts_dir = path.relative(repoRoot, runArtifactsDir).replaceAll("\\", "/");
216:     manifest.updated_at = new Date().toISOString();
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
- edge_id: `edge-key:-7541555374973642958`
- head: `repo://e/ad34b4b3bfc2c20be5b17731d6039a29` (`startDashboardAgentPromptRun`)
- relation: `MUTATES`
- tail: `repo://e/9df759b5c06a31a261185fec6e21bbd0` (`expr@29342`)
- source_span: `packages/cli/src/dashboard-server.mjs:870-870`
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
868:     })
869:     .finally(() => {
870:       activeRun.done = true;
871:       activeRun.finished_at = new Date().toISOString();
872:       const cleanup = setTimeout(() => activeRuns.delete(started.runId), 300000);
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
- edge_id: `edge-key:-7515567087941564816`
- head: `repo://e/a834d4979bd40b385462a7c8de111366` (`runAppServerObjective`)
- relation: `MUTATES`
- tail: `repo://e/2405e114c7458cb818927981981897ea` (`expr@8364`)
- source_span: `services/orchestrator/src/run-manager.mjs:217-217`
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
215:       if (relative === "hashes.json") continue;
216:       if (!(await pathExists(file))) continue;
217:       hashes[relative] = await sha256File(file);
218:     }
219:     await writeJson(path.join(run.runRoot, "hashes.json"), hashes);
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
- edge_id: `edge-key:-7486887082724404718`
- head: `repo://e/69a7fca88c30bf81da6bd2a310292adf` (`constructor`)
- relation: `MUTATES`
- tail: `repo://e/03dca1c3a4b29aec93ceb62a7de5c226` (`expr@210`)
- source_span: `packages/appserver-client/src/jsonrpc-client.mjs:7-7`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9f5ceab056a85bfc`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
5: export class JsonRpcClient {
6:   constructor(command, args = [], options = {}) {
7:     this.command = command;
8:     this.args = args;
9:     this.options = options;
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
- edge_id: `edge-key:-7470414463075579658`
- head: `repo://e/0adf36ba28e917588f7acf5126fc6afa` (`refreshRunArtifacts`)
- relation: `MUTATES`
- tail: `repo://e/ef8af0c4b608825d6b4b46b5c9f994a7` (`expr@2450`)
- source_span: `packages/core/src/run-ledger/finalize.mjs:59-59`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:4a367677c0432073`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
57:     if (relative === "hashes.json") continue;
58:     if (!(await pathExists(file))) continue;
59:     hashes[relative] = await sha256File(file);
60:   }
61:   await writeJson(path.join(runRoot, "hashes.json"), hashes);
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
- edge_id: `edge-key:-7377399662367889986`
- head: `repo://e/b72741d4810fb3f51c2ee06fd2066433` (`parseArgs`)
- relation: `MUTATES`
- tail: `repo://e/67e9b0fcde513f95bcf75020452eaa17` (`expr@1255`)
- source_span: `scripts/package-local-release.mjs:37-37`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:2a30e7e72f3ab481`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
35:       parsed.verify = true;
36:     } else if (arg === "--sign-thumbprint") {
37:       parsed.signing ??= {};
38:       parsed.signing.certThumbprint = argv[index + 1];
39:       index += 1;
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
- edge_id: `edge-key:-7304008756579068383`
- head: `repo://e/63ceb4904f1c4ff0cdf0a94c2db1912e` (`run`)
- relation: `MUTATES`
- tail: `repo://e/7d76e05c94109817a9098a46f3ad51ad` (`expr@6417`)
- source_span: `packages/core/src/runtime-adapters/openrouter.mjs:197-197`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:ede9c6076a25837e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
195:     manifest.runtime.openrouter_endpoint = this.endpoint;
196:     manifest.summary.codex_execution = false;
197:     manifest.summary.model_execution = true;
198:     manifest.summary.openrouter_http_status = httpStatus;
199:     manifest.summary.openrouter_usage = responseJson.usage ?? null;
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
- edge_id: `edge-key:-7202109217750214910`
- head: `repo://e/baeefa615b9e301d5d2a1f9e8adb77c3` (`writeReleaseManifest`)
- relation: `MUTATES`
- tail: `repo://e/7d0e52b29d8c97e7f2711585a8cbdeed` (`expr@11084`)
- source_span: `packages/core/src/release/index.mjs:270-270`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:dab331d28fc8723d`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
268:     .join("\n");
269:   await writeText(path.join(releaseRoot, "RELEASE_CHECKSUMS.txt"), `${checksumLines}\n`);
270:   hashes["RELEASE_CHECKSUMS.txt"] = await sha256File(path.join(releaseRoot, "RELEASE_CHECKSUMS.txt"));
271:   const manifest = {
272:     name: "autoresearch-lab",
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
- edge_id: `edge-key:-7172162984946727200`
- head: `repo://e/afb22024d68b612c5c6ee74d2771a1b2` (`renderList`)
- relation: `MUTATES`
- tail: `repo://e/7c8e7a593e308f70a7de548088f3dbd2` (`expr@2768`)
- source_span: `packages/ui-autoresearch/src/app.js:76-76`
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
74:   const target = document.getElementById(id);
75:   if (!target) return;
76:   target.innerHTML = rows.length === 0 ? "<div class=\"empty\">None</div>" : rows.map(formatter).join("");
77: }
78: 
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
- edge_id: `edge-key:-7149695787689062274`
- head: `repo://e/b72741d4810fb3f51c2ee06fd2066433` (`parseArgs`)
- relation: `MUTATES`
- tail: `repo://e/a3bfc2c3e810e8c7e53bbbe5213da0c1` (`expr@1020`)
- source_span: `scripts/package-local-release.mjs:30-30`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:2a30e7e72f3ab481`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
28:     const arg = argv[index];
29:     if (arg === "--out") {
30:       parsed.outRoot = argv[index + 1];
31:       index += 1;
32:     } else if (arg === "--dry-run") {
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
- edge_id: `edge-key:-7131106087349945156`
- head: `repo://e/4be08de4c2658305e644b3b2dd6fd893` (`collectAgentArtifacts`)
- relation: `MUTATES`
- tail: `repo://e/16959b5e8d945db5151d9c092363865f` (`expr@4755`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:135-135`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
133:         item.hash = finalMessageHash;
134:         item.summary = "External Codex final message artifact exists and is hashed.";
135:         item.producer = "external-codex-adapter";
136:       }
137:     }
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
- edge_id: `edge-key:-7128533755177220418`
- head: `repo://e/9a43e8f6c0646eeb461a57d356619925` (`loadArtifactContent`)
- relation: `MUTATES`
- tail: `repo://e/3b491b75ad10a886f2b604c9fa4b71ec` (`expr@19624`)
- source_span: `packages/ui-autoresearch/src/app.js:441-441`
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
439:   const rawUrl = `/api/runs/${encodeURIComponent(runId)}/artifact-raw?path=${encodeURIComponent(artifactPath)}`;
440:   if (lowerPath.endsWith(".svg") || lowerPath.endsWith(".png") || lowerPath.endsWith(".jpg") || lowerPath.endsWith(".jpeg") || lowerPath.endsWith(".gif")) {
441:     preview.innerHTML = `<img class="artifact-media" src="${escapeHtml(rawUrl)}" alt="${escapeHtml(artifactPath)}">`;
442:     return;
443:   }
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
- edge_id: `edge-key:-7122462982407612167`
- head: `repo://e/6cc104eab113bce8cfd8e4f32771e0ea` (`start`)
- relation: `MUTATES`
- tail: `repo://e/4ae0c70043b5a46a99ec69c020485464` (`expr@419`)
- source_span: `packages/appserver-client/src/jsonrpc-client.mjs:17-21`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9f5ceab056a85bfc`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
15: 
16:   start() {
17:     this.process = spawn(this.command, this.args, {
18:       cwd: this.options.cwd,
19:       env: this.options.env,
20:       stdio: ["pipe", "pipe", "pipe"]
21:     });
22:     const rl = readline.createInterface({ input: this.process.stdout });
23:     rl.on("line", (line) => {
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
- edge_id: `edge-key:-7089482159604208593`
- head: `repo://e/dd7c823654a09f3206bcf3b48a4ffe6c` (`countClaims`)
- relation: `MUTATES`
- tail: `repo://e/382c1d2fd702442e2a511363ad7951e4` (`expr@678`)
- source_span: `packages/core/src/reproducibility/index.mjs:20-20`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:fe8b58ac616f91b1`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
18:     if (claim.status === "unknown") counts.unknown_claims += 1;
19:     if (claim.status === "not_tested") counts.not_tested_claims += 1;
20:     if (claim.status === "contradicted") counts.contradicted_claims += 1;
21:     if (claim.status === "supported") counts.supported_claims += 1;
22:   }
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
- edge_id: `edge-key:-6998665646795131405`
- head: `repo://e/aee2f7bdea85439bb2c8dec26ec8fbb9` (`releasePackage`)
- relation: `MUTATES`
- tail: `repo://e/fc0b3c9c010eed65b78ed163602a0947` (`expr@24010`)
- source_span: `packages/cli/src/arl.mjs:576-576`
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
574:   const signThumbprint = flagValue(args, "--sign-thumbprint");
575:   const timestampServer = flagValue(args, "--timestamp-server");
576:   if (signThumbprint) signing.certThumbprint = signThumbprint;
577:   if (timestampServer) signing.timestampServer = timestampServer;
578:   if (args.includes("--sign-include-binaries")) signing.includeBinaries = true;
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
- edge_id: `edge-key:-6948883265168403404`
- head: `repo://e/f4cd6322b3b37caee468bfdb89ff02c8` (`constructor`)
- relation: `MUTATES`
- tail: `repo://e/29b7ff8fadd6e5d0ef4c58d7698c096a` (`expr@1730`)
- source_span: `packages/core/src/runtime-adapters/openrouter.mjs:58-58`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:ede9c6076a25837e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
56:   } = {}) {
57:     super(RUNTIME_KINDS.openrouterModel);
58:     this.apiKey = apiKey;
59:     this.endpoint = endpoint;
60:     this.model = model;
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
- edge_id: `edge-key:-6905506438416681646`
- head: `repo://e/ad34b4b3bfc2c20be5b17731d6039a29` (`startDashboardAgentPromptRun`)
- relation: `MUTATES`
- tail: `repo://e/2b2c1bab03521759adb97e917bd587a7` (`expr@29371`)
- source_span: `packages/cli/src/dashboard-server.mjs:871-871`
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
869:     .finally(() => {
870:       activeRun.done = true;
871:       activeRun.finished_at = new Date().toISOString();
872:       const cleanup = setTimeout(() => activeRuns.delete(started.runId), 300000);
873:       cleanup.unref?.();
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
- edge_id: `edge-key:-6862744288750390352`
- head: `repo://e/94299cf089dadff9c7f593e158039b57` (`runMultiAgentFixture`)
- relation: `MUTATES`
- tail: `repo://e/4a4112432477eaddc9385cb09a1cd2db` (`expr@3497`)
- source_span: `services/orchestrator/src/multi-agent-runner.mjs:96-96`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:515d478182d71479`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
94:   const manifest = await readJson(manifestPath);
95:   manifest.runtime.kind = "multi_agent_app_server_shim";
96:   manifest.runtime.app_server_protocol_version = "arl-app-server-shim-0.1.0";
97:   manifest.summary.multi_agent = true;
98:   manifest.summary.agent_count = agents.length;
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
- edge_id: `edge-key:-6840756807244992718`
- head: `repo://e/1492c40ff5bb388717fa41cefe9520e9` (`run`)
- relation: `MUTATES`
- tail: `repo://e/464ba9431132972706eee61e401300b1` (`expr@17369`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:489-489`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
487:     manifest.summary.external_exit_code = result.status;
488:     manifest.summary.codex_events_mapped = mapped.length;
489:     manifest.summary.agent_workspace = path.relative(repoRoot, workspace).replaceAll("\\", "/");
490:     manifest.summary.agent_artifacts_dir = path.relative(repoRoot, workspaceArtifactsDir).replaceAll("\\", "/");
491:     manifest.summary.run_artifacts_dir = path.relative(repoRoot, runArtifactsDir).replaceAll("\\", "/");
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
- edge_id: `edge-key:-6825628308735616691`
- head: `repo://e/63ceb4904f1c4ff0cdf0a94c2db1912e` (`run`)
- relation: `MUTATES`
- tail: `repo://e/d243afbeee13d9f92672752cdfb2c7cf` (`expr@7645`)
- source_span: `packages/core/src/runtime-adapters/openrouter.mjs:224-224`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:ede9c6076a25837e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
222:         item.hash = finalMessageHash;
223:         item.summary = "Final message artifact exists and is hashed after OpenRouter model execution.";
224:         item.producer = "openrouter-adapter";
225:       }
226:     }
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
- edge_id: `edge-key:-6775444492198278353`
- head: `repo://e/1492c40ff5bb388717fa41cefe9520e9` (`run`)
- relation: `MUTATES`
- tail: `repo://e/43d9ac10cc490ac1fb4ff7edab825a83` (`expr@17008`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:482-482`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
480: 
481:     const manifest = await readJson(path.join(runRoot, "manifest.json"));
482:     manifest.runtime.kind = "external_codex_cli";
483:     manifest.runtime.codex_source = "stock";
484:     manifest.status = result.status === 0 ? "completed" : "failed";
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
- edge_id: `edge-key:-6764174658964779383`
- head: `repo://e/df6b6705e2f1a4b3d74851dc4dafddf9` (`start`)
- relation: `MUTATES`
- tail: `repo://e/a99995bcd3a3296a1f03648fabf79a1c` (`expr@11778`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:340-340`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
338:         const finalManifest = await readJson(manifestPath);
339:         finalManifest.status = cancelled ? "cancelled" : code === 0 ? "completed" : "failed";
340:         finalManifest.summary.external_exit_code = code;
341:         finalManifest.summary.external_signal = signal;
342:         finalManifest.summary.codex_events_mapped = mappedEvents;
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
- edge_id: `edge-key:-6685588034384275021`
- head: `repo://e/81fadbac75286413785474f905f73a65` (`startDashboardLiveRun`)
- relation: `MUTATES`
- tail: `repo://e/60f2a6ea1259528ce931b84dd663492e` (`expr@27393`)
- source_span: `packages/cli/src/dashboard-server.mjs:807-807`
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
805:   activeRun.promise = (async () => {
806:     try {
807:       activeRun.detail = await startDashboardRun(repoRoot, { ...body, run_id: runId });
808:     } catch (error) {
809:       activeRun.error = error.message;
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
- edge_id: `edge-key:-6672239849473342182`
- head: `repo://e/df6b6705e2f1a4b3d74851dc4dafddf9` (`start`)
- relation: `MUTATES`
- tail: `repo://e/d1e3634014f037b895a719cc70698af2` (`expr@11891`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:342-342`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
340:         finalManifest.summary.external_exit_code = code;
341:         finalManifest.summary.external_signal = signal;
342:         finalManifest.summary.codex_events_mapped = mappedEvents;
343:         finalManifest.updated_at = new Date().toISOString();
344:         await writeJson(manifestPath, finalManifest);
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
- edge_id: `edge-key:-6633635061558865913`
- head: `repo://e/46b50f34b5e6f9c8c72a8ba02df5dcb2` (`stopDashboardRun`)
- relation: `MUTATES`
- tail: `repo://e/773be7bedfd8b939ff8f38a6e94f0e3f` (`expr@30547`)
- source_span: `packages/cli/src/dashboard-server.mjs:904-904`
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
902:   manifest.status = "cancelled";
903:   manifest.updated_at = new Date().toISOString();
904:   manifest.cancellation = { reason, cancelled_at: manifest.updated_at };
905:   await writeJson(manifestPath, manifest);
906:   await appendJsonl(path.join(runRoot, "events.jsonl"), {
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
- edge_id: `edge-key:-6622665071508406009`
- head: `repo://e/63ceb4904f1c4ff0cdf0a94c2db1912e` (`run`)
- relation: `MUTATES`
- tail: `repo://e/762bff4018dacda9f89cd1336e2bef97` (`expr@6371`)
- source_span: `packages/core/src/runtime-adapters/openrouter.mjs:196-196`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:ede9c6076a25837e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
194:     manifest.runtime.openrouter_model = responseJson.model ?? this.model;
195:     manifest.runtime.openrouter_endpoint = this.endpoint;
196:     manifest.summary.codex_execution = false;
197:     manifest.summary.model_execution = true;
198:     manifest.summary.openrouter_http_status = httpStatus;
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
- edge_id: `edge-key:-6526204274370451514`
- head: `repo://e/f4cd6322b3b37caee468bfdb89ff02c8` (`constructor`)
- relation: `MUTATES`
- tail: `repo://e/388e2dec6387c32fdceed46e88806de1` (`expr@1834`)
- source_span: `packages/core/src/runtime-adapters/openrouter.mjs:62-62`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:ede9c6076a25837e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
60:     this.model = model;
61:     this.title = title;
62:     this.referer = referer;
63:     this.fetchImpl = fetchImpl;
64:   }
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
- edge_id: `edge-key:-6447267344500767913`
- head: `repo://e/f4cd6322b3b37caee468bfdb89ff02c8` (`constructor`)
- relation: `MUTATES`
- tail: `repo://e/499ca8ac7ff22ec7c5ce580a5d5fae0d` (`expr@1810`)
- source_span: `packages/core/src/runtime-adapters/openrouter.mjs:61-61`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:ede9c6076a25837e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
59:     this.endpoint = endpoint;
60:     this.model = model;
61:     this.title = title;
62:     this.referer = referer;
63:     this.fetchImpl = fetchImpl;
```

