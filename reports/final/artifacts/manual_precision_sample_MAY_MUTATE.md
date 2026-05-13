# Edge Sample Audit

Database: `reports/final/artifacts/comprehensive_proof_1778642765060.sqlite`

Relation filter: `MAY_MUTATE`

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
- edge_id: `edge-key:-9219820407857292361`
- head: `repo://e/610321cdeb8caa1cd02ee9e29235ebea` (`parseArray`)
- relation: `MAY_MUTATE`
- tail: `repo://e/75e300bff58f07878326a80ce1b58bbd` (`expr@1748`)
- source_span: `packages/core/src/simple-yaml.mjs:98-98`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:7441dc47b2a509a8`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://aeb90bdcf1659ca2f2898dfd5503cf10, edge://649be275bd55df130f60143f9e7fd269`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
96:       }
97:       while (index < lines.length && lines[index].indent > indent) {
98:         const [extra, next] = parseObject(lines, index, lines[index].indent);
99:         Object.assign(item, extra);
100:         index = next;
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
- edge_id: `edge-key:-9219762901901535455`
- head: `repo://e/6135c46a53c2efb9db26e112df69e3d3` (`createLocalReleasePackage`)
- relation: `MAY_MUTATE`
- tail: `repo://e/313cc1675975346247a966707e1a9ca4` (`manifest`)
- source_span: `packages/core/src/release/index.mjs:399-405`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:dab331d28fc8723d`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://8ee876084c2ac6713079bcf913322553, edge://b4c51ea1d88d9d5465e74a57719dc526`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
397:   }
398:   copied.push("SIGNING_STATUS.json");
399:   const manifest = await writeReleaseManifest({
400:     repoRoot: root,
401:     releaseRoot: plan.release_root,
402:     version: plan.version,
403:     copied: copied.sort(),
404:     missingOptional
405:   });
406:   return {
407:     ...plan,
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
- edge_id: `edge-key:-9218969952372935841`
- head: `repo://e/4a2278bc920360444b92b49ebca96a76` (`runCommand`)
- relation: `MAY_MUTATE`
- tail: `repo://e/04688a55c5f6b8dc2dc68777bb4b210e` (`handoffValidation`)
- source_span: `packages/cli/src/arl.mjs:192-192`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://cca9bfce2ab165427c87bf67f19bf3c4`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
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
- edge_id: `edge-key:-9218850384089096944`
- head: `repo://e/cfdda9ed837e3c34054fd3b7cefacb3a` (`runArlCodexdObjective`)
- relation: `MAY_MUTATE`
- tail: `repo://e/d48f77861ace7da97d891bd3d465376f` (`hypotheses`)
- source_span: `services/orchestrator/src/run-manager.mjs:305-305`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:67b86b73b8254b08`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://b064916150da46af64a422c3fcfa3da9, edge://ebbc6dd75ab9949161680f4d15c566f7`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
303: 
304: export async function runArlCodexdObjective(objectivePath, repoRoot = process.cwd(), { runId = null } = {}) {
305:   const result = await runAppServerObjective(objectivePath, repoRoot, { runId });
306:   const loaded = await loadObjective(objectivePath);
307:   const manifestPath = path.join(result.runRoot, "manifest.json");
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
- edge_id: `edge-key:-9213604290441932349`
- head: `repo://e/d990681f687d9340461841c15b3d154a` (`validateObjective`)
- relation: `MAY_MUTATE`
- tail: `repo://e/1397fc5afb008be64724ffeffd67e814` (`errors`)
- source_span: `packages/core/src/objective/index.mjs:16-16`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:dd4eb69d550e8045`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://8f1ead5955b868c3d356acc88af19bcd, edge://a809fbfdfde3b59a67ed7e45ab92972c`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
14: export async function validateObjective(objective, repoRoot = process.cwd()) {
15:   const schemaPath = path.join(repoRoot, "schemas", "objective.schema.json");
16:   const schemaResult = await validateWithSchema(objective, schemaPath);
17:   const policyResult = validateObjectivePolicy(objective);
18:   return {
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
- edge_id: `edge-key:-9195435034108089155`
- head: `repo://e/b429ff7d1bccdf91deeff575de946bf3` (`loadToolForge`)
- relation: `MAY_MUTATE`
- tail: `repo://e/a686093f16d531077152957ab88adf99` (`target`)
- source_span: `packages/ui-autoresearch/src/app.js:398-398`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:52a5b8c00049eaea`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://3882f83e0977a1eb39018fdc48023bcd, edge://a11898828c034e1339984f5682dc6fdd`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
396:     if (!response.ok) throw new Error(`tool-forge ${response.status}`);
397:     const payload = await response.json();
398:     renderList("tool-forge", payload.tools ?? [], (tool) => `<div class="row"><b>${escapeHtml(tool.status)}</b><span>${escapeHtml(tool.name ?? tool.tool_id)} ${escapeHtml(tool.tests)} tests</span></div>`);
399:   } catch {
400:     renderList("tool-forge", [], () => "");
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
- edge_id: `edge-key:-9137095727609987610`
- head: `repo://e/1492c40ff5bb388717fa41cefe9520e9` (`run`)
- relation: `MAY_MUTATE`
- tail: `repo://e/d1bfee1cb4794e8e92183651de728a8c` (`candidateEvents`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:509-509`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://f04ef831356852490b2a5881c083a157, edge://b0bbc886b4e01661e5e8ecb1cfd85463`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
507:     await writeJson(path.join(runRoot, "evidence.json"), evidence);
508:     await this.collectAgentArtifacts({ runRoot, runId: foundation.runId });
509:     const verification = await refreshRunArtifacts({ runRoot, objective, manifest });
510: 
511:     return {
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
- edge_id: `edge-key:-9135843586310485845`
- head: `repo://e/b429ff7d1bccdf91deeff575de946bf3` (`loadToolForge`)
- relation: `MAY_MUTATE`
- tail: `repo://e/a686093f16d531077152957ab88adf99` (`target`)
- source_span: `packages/ui-autoresearch/src/app.js:400-400`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:52a5b8c00049eaea`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://50fb04e97f4423631893a02e2dc946b1, edge://a11898828c034e1339984f5682dc6fdd`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
398:     renderList("tool-forge", payload.tools ?? [], (tool) => `<div class="row"><b>${escapeHtml(tool.status)}</b><span>${escapeHtml(tool.name ?? tool.tool_id)} ${escapeHtml(tool.tests)} tests</span></div>`);
399:   } catch {
400:     renderList("tool-forge", [], () => "");
401:   }
402: }
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
- edge_id: `edge-key:-9135531104771632223`
- head: `repo://e/c47b432c50a38310098e107afa661dca` (`walkFiles`)
- relation: `MAY_MUTATE`
- tail: `repo://e/61e9d9de7cd5518d68530f97f4d6fa87` (`files`)
- source_span: `packages/core/src/fs-utils.mjs:55-55`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:dfa4ba890134237c`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://f6668534d94440c7cddd744405267199, edge://a4ddecba37da47ea67bfc88f97de5f04`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
53:     const full = path.join(root, entry.name);
54:     if (entry.isDirectory()) {
55:       files.push(...await walkFiles(full));
56:     } else {
57:       files.push(full);
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
- edge_id: `edge-key:-9130887175481767900`
- head: `repo://e/1b3fb031b810207a3f4f4b0371c36df8` (`createDashboardServer`)
- relation: `MAY_MUTATE`
- tail: `repo://e/a7174e4d3e075c87099a8341e517a349` (`evidence`)
- source_span: `packages/cli/src/dashboard-server.mjs:1057-1057`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://5d21fa7d5cfd89e5b012e9dae9673bbf, edge://72182e295401ac64eed0dfc8c1fe8026`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
1055:       if (url.pathname.startsWith("/api/runs/")) {
1056:         const runId = decodeURIComponent(url.pathname.slice("/api/runs/".length));
1057:         json(response, 200, await loadRunDetail(repoRoot, runId));
1058:         return;
1059:       }
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
- edge_id: `edge-key:-9126786430349816382`
- head: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb` (`main`)
- relation: `MAY_MUTATE`
- tail: `repo://e/d3dcfecfaea8426208720297b13c7918` (`checks`)
- source_span: `packages/cli/src/arl.mjs:713-713`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://15e91fce6bd8bc9fdc56a5a1c2ef656d, edge://a7135feea88c4fa068b10a7913a0413e`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
711:     ok = await bundleCommand(repoRoot, subcommand, rest[0]);
712:   } else if (command === "runtime" && subcommand === "doctor") {
713:     ok = await doctor(repoRoot);
714:   } else if (command === "runtime" && subcommand === "versions") {
715:     ok = await runtimeVersions(repoRoot);
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
- edge_id: `edge-key:-9126258435566441673`
- head: `repo://e/1b3fb031b810207a3f4f4b0371c36df8` (`createDashboardServer`)
- relation: `MAY_MUTATE`
- tail: `repo://e/c3d5763054ecebc874bba7daedd988d2` (`body`)
- source_span: `packages/cli/src/dashboard-server.mjs:962-962`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://873ad652964381cbe645613d18004775, edge://9065478642546596a775f9df00626704`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
960:       }
961:       if (url.pathname === "/api/approvals/respond" && request.method === "POST") {
962:         const body = await readBody(request);
963:         json(response, 200, {
964:           approval: await respondApprovalRequest(repoRoot, body.approval_id, body.decision, body.reason ?? "")
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
- edge_id: `edge-key:-9125053967938333149`
- head: `repo://e/3254884b72adaed54c0f22b359430ef7` (`runAgentPrompt`)
- relation: `MAY_MUTATE`
- tail: `repo://e/2aa45ae2879503330a8a845adbb2c3a9` (`runStartBusy`)
- source_span: `packages/ui-autoresearch/src/app.js:488-488`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:52a5b8c00049eaea`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://6952b7d16757d66616bc7ed4c74f9dac, edge://5fff9acf795fb83e1434cf5ba6a6aaac`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
486:     const streaming = startEventStream(currentRunId, async (streamStatus) => {
487:       text("start-status", streamStatus);
488:       setRunBusy(false);
489:       await loadDashboard();
490:       await loadArtifacts(currentRunId);
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
- edge_id: `edge-key:-9117253083937627115`
- head: `repo://e/9c002c70d834159d418707f8b23f5017` (`checkPreflight`)
- relation: `MAY_MUTATE`
- tail: `repo://e/18f7a986e1975872be7d798f9e59d90c` (`manifest`)
- source_span: `packages/core/src/preflight/index.mjs:148-148`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:e5d2731bd32d3ab1`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://b12558ea38249bef999a6a1e68f11ba1, edge://50946797c9a2f22b34559318ce492759`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
146:   const openRouterKeyConfigured =
147:     Boolean(process.env.OPENROUTER_API_KEY) || (await hasEnvFileKey(root, "OPENROUTER_API_KEY"));
148:   const openRouterLiveSmoke = await findLatestOpenRouterLiveSmoke(root);
149: 
150:   return {
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
- edge_id: `edge-key:-9110101756682213977`
- head: `repo://e/532463487753546bab313f0bf17540f5` (`renderConsole`)
- relation: `MAY_MUTATE`
- tail: `repo://e/1a896320727622ed92393a2d9b3d9db3` (`raw`)
- source_span: `packages/ui-autoresearch/src/app.js:209-209`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:52a5b8c00049eaea`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://dc14a96760d0ae6e7945c183631f9a04, edge://8141e4c6d40b09a7313dc9ba77b29cd1`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
207:         <span class="console-lane">${escapeHtml(lane)}</span>
208:         <span class="console-type">${escapeHtml(event.type ?? "event")}</span>
209:         <span class="console-text">${escapeHtml(eventText(event))}</span>
210:       </div>
211:     `;
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
- edge_id: `edge-key:-9106006564839525539`
- head: `repo://e/dcb17abf8ea985d296f7d3c6ff7f0fb8` (`listRunArtifacts`)
- relation: `MAY_MUTATE`
- tail: `repo://e/6e38e6ab34d4c2c1de21d0e4545f7647` (`full`)
- source_span: `packages/cli/src/dashboard-server.mjs:451-451`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://35f81fa2f5ebf713b7a6ed0ee9c148fd, edge://15028d7af2bc8a17624bfcafc4000cfd`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
449:   for (const artifactRoot of artifactRoots) {
450:     if (await pathExists(artifactRoot)) {
451:       files.push(...await walkFiles(artifactRoot));
452:     }
453:   }
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
- edge_id: `edge-key:-9090281203665215016`
- head: `repo://e/6135c46a53c2efb9db26e112df69e3d3` (`createLocalReleasePackage`)
- relation: `MAY_MUTATE`
- tail: `repo://e/9face43fd9c7f92f685f3d506c649ced` (`packageJson`)
- source_span: `packages/core/src/release/index.mjs:354-354`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:dab331d28fc8723d`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://91833ce2183e97433b6434ca21ffcff1, edge://e827cafc25f54b79b44e9a646287ac1f`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
352: export async function createLocalReleasePackage({ repoRoot, outRoot, clean = true, signing = null } = {}) {
353:   const root = path.resolve(repoRoot ?? process.cwd());
354:   const plan = await planLocalReleasePackage({ repoRoot, outRoot });
355:   assertSafeReleaseRoot(plan.output_root, plan.release_root);
356: 
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
- edge_id: `edge-key:-9083671228135239613`
- head: `repo://e/4a2278bc920360444b92b49ebca96a76` (`runCommand`)
- relation: `MAY_MUTATE`
- tail: `repo://e/33bf0c78cf0b7a0e5cb5a7fae2837184` (`thread`)
- source_span: `packages/cli/src/arl.mjs:192-192`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://dcc1d7c2e57d51d2e185b0b149186d2c`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
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
- edge_id: `edge-key:-9062057332267039577`
- head: `repo://e/361f02fa936d8a6dbf41037d332a191f` (`createDryRunFolder`)
- relation: `MAY_MUTATE`
- tail: `repo://e/b2b522210246c30298499ead66db142c` (`unknowns`)
- source_span: `packages/core/src/run-ledger/index.mjs:154-154`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:c546747cf6f02fba`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://d2173cbbbf025c6412f0227d706c1f3e, edge://d7fea63e2a1fbd8dc045b8574c497b43`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
152:   const evidenceHealth = await computeEvidenceHealth({ claims, evidence, runRoot });
153:   const verifierReport = await verifyRun(runRoot);
154:   const report = buildReport({ objective, manifest, hypotheses, experimentPlan, claims, evidence, evidenceHealth, verifierReport });
155:   await writeText(path.join(runRoot, "report.md"), report);
156: 
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
- edge_id: `edge-key:-9045533710969073580`
- head: `repo://e/7e571fb9e43c3e1197c7e6db414b13d7` (`readDashboardRuns`)
- relation: `MAY_MUTATE`
- tail: `repo://e/a7174e4d3e075c87099a8341e517a349` (`evidence`)
- source_span: `packages/cli/src/dashboard-server.mjs:413-413`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://71845ff7848765d0f9cff99271a08ba2, edge://72182e295401ac64eed0dfc8c1fe8026`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
411:   for (const runId of runIds) {
412:     try {
413:       const detail = await loadRunDetail(repoRoot, runId);
414:       runs.push({
415:         run_id: detail.run_id,
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
- edge_id: `edge-key:-9036510497356550750`
- head: `repo://e/00c50396d8ac0dd0744565489e86d2b6` (`dashboardDev`)
- relation: `MAY_MUTATE`
- tail: `repo://e/d9ee3babe49a6c6cbe3ba47a55ced37e` (`server`)
- source_span: `packages/cli/src/arl.mjs:645-645`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://791d519735221be7464e5ae607c21539, edge://181cd8fcdadac1ee412508fd3d3d8464`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
643:     return false;
644:   }
645:   const { server, url } = await startDashboardServer({ repoRoot, port });
646:   console.log(`dashboard_url=${url}`);
647:   console.log("Press Ctrl+C to stop.");
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
- edge_id: `edge-key:-9029123523542654457`
- head: `repo://e/3254884b72adaed54c0f22b359430ef7` (`runAgentPrompt`)
- relation: `MAY_MUTATE`
- tail: `repo://e/5ac0be2422b04de735f91b8503547355` (`expr@3260`)
- source_span: `packages/ui-autoresearch/src/app.js:488-488`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:52a5b8c00049eaea`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://6952b7d16757d66616bc7ed4c74f9dac, edge://528d6913f74d98fd1cad170749dd918b`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
486:     const streaming = startEventStream(currentRunId, async (streamStatus) => {
487:       text("start-status", streamStatus);
488:       setRunBusy(false);
489:       await loadDashboard();
490:       await loadArtifacts(currentRunId);
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
- edge_id: `edge-key:-9021568044918614389`
- head: `repo://e/266cb24c8291651a300c0d8fac2b2f84` (`main`)
- relation: `MAY_MUTATE`
- tail: `repo://e/a7e638f4724626d2d4a06be3eaf763fc` (`manifest`)
- source_span: `scripts/verify-local-release.mjs:35-35`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6d932e651b0ef5c3`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://0b22daf97a6a6cf34f72004890481121, edge://82e6a5869e2ad0d68b6dd3a258edfb14`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
33:     return;
34:   }
35:   const result = await verifyLocalReleasePackage({ releaseRoot: args.root });
36:   if (args.json) {
37:     console.log(JSON.stringify(result, null, 2));
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
- edge_id: `edge-key:-9021117164903328855`
- head: `repo://e/4a2278bc920360444b92b49ebca96a76` (`runCommand`)
- relation: `MAY_MUTATE`
- tail: `repo://e/d6edf7e6fd2c8ba8ebb35d3801dc5c16` (`expr@3575`)
- source_span: `packages/cli/src/arl.mjs:192-192`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://c59c76173fc4a143b3dd5161502cc041`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
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
- edge_id: `edge-key:-8973495700798700298`
- head: `repo://e/6135c46a53c2efb9db26e112df69e3d3` (`createLocalReleasePackage`)
- relation: `MAY_MUTATE`
- tail: `repo://e/78ae15d3ce2e77c85da9e31a28c923b2` (`base`)
- source_span: `packages/core/src/release/index.mjs:369-369`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:dab331d28fc8723d`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://0c0fc03bc51bee0d93245ef5d5c761a7, edge://53421bfd08b2f58b3c088aaf836e8611`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
367:   const copied = [];
368:   for (const entry of plan.required) {
369:     await copyEntry(root, plan.release_root, entry);
370:     copied.push(entry.to);
371:   }
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
- edge_id: `edge-key:-8971696601841222161`
- head: `repo://e/1b3fb031b810207a3f4f4b0371c36df8` (`createDashboardServer`)
- relation: `MAY_MUTATE`
- tail: `repo://e/1778c34172cf9de5e9bd3d0730f2fdff` (`maxCost`)
- source_span: `packages/cli/src/dashboard-server.mjs:947-947`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://972b8b84314b6da2b67d0e9ad145962c, edge://8ccef32b01d99822c86ffe5c32afa830`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
945:       if (url.pathname === "/api/objectives") {
946:         if (request.method === "POST") {
947:           json(response, 200, { objective: await createDashboardObjective(repoRoot, await readBody(request)) });
948:           return;
949:         }
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
- edge_id: `edge-key:-8954156449287337493`
- head: `repo://e/1b3fb031b810207a3f4f4b0371c36df8` (`createDashboardServer`)
- relation: `MAY_MUTATE`
- tail: `repo://e/0126a4fbc874a5cd347f67da0c1e0d93` (`runIds`)
- source_span: `packages/cli/src/dashboard-server.mjs:1006-1006`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://659cce859a49ef73125b3943316a52e9, edge://3567da480cbb73ac357a479b888bea1e`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
1004:       }
1005:       if (url.pathname === "/api/runs") {
1006:         const runs = await readDashboardRuns(repoRoot);
1007:         const selected = runs[0] ? await loadRunDetail(repoRoot, runs[0].run_id) : null;
1008:         json(response, 200, { runs, selected });
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
- edge_id: `edge-key:-8949542723981015571`
- head: `repo://e/96a8118fec38e93059bcdf6019955f1e` (`startDashboardRun`)
- relation: `MAY_MUTATE`
- tail: `repo://e/01386855907c75c3b50916a10358f3fd` (`evidenceHealth`)
- source_span: `packages/cli/src/dashboard-server.mjs:759-759`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://ab940ba68c6d5c31f580b7a782c4cd8f, edge://b2c344baaf21c2fa842814e3f6063b1c`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
757:     throw new Error(`unsupported dashboard runtime: ${runtime}`);
758:   }
759:   return loadRunDetail(repoRoot, result.runId);
760: }
761: 
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
- edge_id: `edge-key:-8942650656890787570`
- head: `repo://e/1b3fb031b810207a3f4f4b0371c36df8` (`createDashboardServer`)
- relation: `MAY_MUTATE`
- tail: `repo://e/94ce25a2c54a65bb54bd41612fe30a1d` (`expr@27503`)
- source_span: `packages/cli/src/dashboard-server.mjs:1016-1016`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://e59870d6e0fb93601fbe1a3eee75cd5e, edge://1f3d7b2bf37daf26714a11ff0c099758`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
1014:       }
1015:       if (url.pathname === "/api/runs/start-live" && request.method === "POST") {
1016:         json(response, 202, { run: await startDashboardLiveRun(repoRoot, await readBody(request), activeRuns) });
1017:         return;
1018:       }
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
- edge_id: `edge-key:-8940729114327229104`
- head: `repo://e/1b3fb031b810207a3f4f4b0371c36df8` (`createDashboardServer`)
- relation: `MAY_MUTATE`
- tail: `repo://e/5cd8e3f2bc7a5200af5fd28a813a8e12` (`liveRuntimes`)
- source_span: `packages/cli/src/dashboard-server.mjs:1016-1016`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://e59870d6e0fb93601fbe1a3eee75cd5e, edge://22ee7bd5860389da7690c18a0d29add0`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
1014:       }
1015:       if (url.pathname === "/api/runs/start-live" && request.method === "POST") {
1016:         json(response, 202, { run: await startDashboardLiveRun(repoRoot, await readBody(request), activeRuns) });
1017:         return;
1018:       }
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
- edge_id: `edge-key:-8939178780844950967`
- head: `repo://e/9dc31946b2b79846be32652b72bd275c` (`createEvidenceCandidate`)
- relation: `MAY_MUTATE`
- tail: `repo://e/22c11cd182922f629b02a4abc5657774` (`method`)
- source_span: `packages/core/src/evidence/index.mjs:56-56`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:8860fed84f027108`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://f9bf5194a27f0603e0c02038546318ed, edge://1a537f3d2f0ebf1aaaf10a7bfb9ddedc`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
54: 
55: export function createEvidenceCandidate({ runId, sequence, producer, event }) {
56:   const classification = evidenceTypeForRuntimeEvent(event);
57:   if (!classification) return null;
58:   const sequenceLabel = String(sequence).padStart(6, "0");
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
- edge_id: `edge-key:-8935857027362754566`
- head: `repo://e/4be08de4c2658305e644b3b2dd6fd893` (`collectAgentArtifacts`)
- relation: `MAY_MUTATE`
- tail: `repo://e/61e9d9de7cd5518d68530f97f4d6fa87` (`files`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:142-142`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6471ef598a00c183`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://86adb6cbbf50b5dc0f0a0e75a68f460a, edge://a4ddecba37da47ea67bfc88f97de5f04`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
140:       const root = path.join(runRoot, rootName);
141:       if (!(await pathExists(root))) continue;
142:       for (const file of await walkFiles(root)) {
143:         const uri = path.relative(runRoot, file).replaceAll("\\", "/");
144:         if ([
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
- edge_id: `edge-key:-8917174359845020588`
- head: `repo://e/0adf36ba28e917588f7acf5126fc6afa` (`refreshRunArtifacts`)
- relation: `MAY_MUTATE`
- tail: `repo://e/f4db671b6c4768ee61c22eff25ff5e4c` (`claims`)
- source_span: `packages/core/src/run-ledger/finalize.mjs:33-33`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:4a367677c0432073`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://2684f84b7039533f3c54eae9cb4da6d5, edge://524beeb988ba5308f54f7aef94562e42`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
31:   const experimentPlan = await readJson(path.join(runRoot, "artifacts", "experiment_plan.json"));
32:   const evidenceHealth = await computeEvidenceHealth({ claims, evidence, runRoot });
33:   const verifierReport = await verifyRun(runRoot);
34:   const report = buildReport({
35:     objective,
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
- edge_id: `edge-key:-8910729836292166942`
- head: `repo://e/ad34b4b3bfc2c20be5b17731d6039a29` (`startDashboardAgentPromptRun`)
- relation: `MAY_MUTATE`
- tail: `repo://e/b5d2b0079467c87b5c445c2b4ae012a1` (`multiAgentPlan`)
- source_span: `packages/cli/src/dashboard-server.mjs:861-861`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://5c6a7e8a348e2ea31c2806f4cd757629, edge://32d012c855cae67abfe7e49a5d79e00c`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
859:     .then(async (result) => {
860:       activeRun.status = result?.cancelled ? "cancelled" : result?.exitCode === 0 ? "completed" : "failed";
861:       activeRun.detail = await loadRunDetail(repoRoot, started.runId);
862:       return activeRun.detail;
863:     })
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
- edge_id: `edge-key:-8905137135176013886`
- head: `repo://e/f0ac14429d544ef05a972c69cb7d794e` (`preflightCommand`)
- relation: `MAY_MUTATE`
- tail: `repo://e/4be8e06a4b2b2eea437c87e7be6a4af8` (`index`)
- source_span: `packages/cli/src/arl.mjs:533-533`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://2f28d05aa564e16c74879a9bfce8149a, edge://4831781750511076804f6183d2bc7830`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
531: 
532: async function preflightCommand(repoRoot, args) {
533:   const releaseRoot = flagValue(args, "--release-root");
534:   const result = await checkPreflight({ repoRoot, releaseRoot });
535:   if (args.includes("--json")) {
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
- edge_id: `edge-key:-8894581826620024657`
- head: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb` (`main`)
- relation: `MAY_MUTATE`
- tail: `repo://e/95d0dc4874977ffc81395a1a579ba2fe` (`result`)
- source_span: `packages/cli/src/arl.mjs:730-730`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://88315076e517d6e493f092c68e708fb6, edge://61e2401b2157b384179c0eea8feb2482`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
731:   } else if (command === "release" && subcommand === "update-check") {
732:     ok = await releaseUpdateCheck(repoRoot, rest);
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
- edge_id: `edge-key:-8870915153162703033`
- head: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb` (`main`)
- relation: `MAY_MUTATE`
- tail: `repo://e/f00348c9c0daae03d60f14403cc63981` (`model`)
- source_span: `packages/cli/src/arl.mjs:689-689`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://e4bb1c8112aea4e04f95ff49e22578a6`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
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
- edge_id: `edge-key:-8864578666614764349`
- head: `repo://e/cfdda9ed837e3c34054fd3b7cefacb3a` (`runArlCodexdObjective`)
- relation: `MAY_MUTATE`
- tail: `repo://e/52818ae1c2a5273bf866c01ad29193d1` (`expr@5234`)
- source_span: `services/orchestrator/src/run-manager.mjs:305-305`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:67b86b73b8254b08`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://b064916150da46af64a422c3fcfa3da9, edge://95946ea00b2811adaec09704e6f1a4cf`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
303: 
304: export async function runArlCodexdObjective(objectivePath, repoRoot = process.cwd(), { runId = null } = {}) {
305:   const result = await runAppServerObjective(objectivePath, repoRoot, { runId });
306:   const loaded = await loadObjective(objectivePath);
307:   const manifestPath = path.join(result.runRoot, "manifest.json");
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
- edge_id: `edge-key:-8859674633742876252`
- head: `repo://e/ea34faff8dff2f52fca08295aa9e2174` (`buildReproducibilitySummary`)
- relation: `MAY_MUTATE`
- tail: `repo://e/a33dc653c4939643fffdb802dfe56401` (`counts`)
- source_span: `packages/core/src/reproducibility/index.mjs:77-77`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:fe8b58ac616f91b1`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://409d91fac0ac9dda59c518bf98028d88, edge://205fc7d73d6dfb8057c175fde1c390fe`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
75:   const claims = await readOptionalJson(path.join(runRoot, "claims.json"), []);
76:   const verifierReport = await readOptionalJson(path.join(runRoot, "verifier_report.json"), {});
77:   const claimCounts = countClaims(Array.isArray(claims) ? claims : []);
78:   return {
79:     replay: {
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
- edge_id: `edge-key:-8854533891123525694`
- head: `repo://e/3f32f8c07d377a5816462b6303935896` (`findLatestOpenRouterLiveSmoke`)
- relation: `MAY_MUTATE`
- tail: `repo://e/ca684f3ad5375e7df00b808b6a5bff7b` (`label`)
- source_span: `packages/core/src/preflight/index.mjs:116-116`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:e5d2731bd32d3ab1`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://c636a7aeb2cef5bf978620a53e4ed539, edge://d2108090047686b618a961aaed78bdec`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
114:     const claims = await readJson(path.join(runRoot, "claims.json"));
115:     const evidence = await readJson(path.join(runRoot, "evidence.json"));
116:     const health = await computeEvidenceHealth({ claims, evidence, runRoot });
117:     if (health.label === "invalid") continue;
118:     const verifierPath = path.join(runRoot, "verifier_report.json");
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
- edge_id: `edge-key:-8844420397752575631`
- head: `repo://e/1b3fb031b810207a3f4f4b0371c36df8` (`createDashboardServer`)
- relation: `MAY_MUTATE`
- tail: `repo://e/01b040ce8aecf694ec158e732674621e` (`claims`)
- source_span: `packages/cli/src/dashboard-server.mjs:1052-1052`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:9453dc0bb38c212e`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://b4c9b5fd13be743fb5b93d3b8c365b99, edge://85c742ad219a2fbb039de37806fd463d`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
1050:       if (url.pathname.match(/^\/api\/runs\/[^/]+\/agent-turn$/) && request.method === "POST") {
1051:         const runId = decodeURIComponent(url.pathname.split("/")[3]);
1052:         json(response, 200, { run: await appendDashboardAgentTurn(repoRoot, runId, await readBody(request)) });
1053:         return;
1054:       }
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
- edge_id: `edge-key:-8833655438274806703`
- head: `repo://e/ed335e96c50a4dde40285b3a0d4be4f4` (`main`)
- relation: `MAY_MUTATE`
- tail: `repo://e/1a89a31692cc091e674acb9cd8b8731c` (`plan`)
- source_span: `scripts/package-local-release.mjs:100-100`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:2a30e7e72f3ab481`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://7b8e565116f0899624a19b1b352f7a3c, edge://69600f58ee7f0b91fe25ffa125a45853`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
98:     return;
99:   }
100:   const result = await createLocalReleasePackage({ repoRoot, outRoot, signing: args.signing });
101:   if (args.verify) {
102:     result.verification = await verifyLocalReleasePackage({ releaseRoot: result.release_root });
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
- edge_id: `edge-key:-8815011197805247827`
- head: `repo://e/0413dbfce237f6db4ab555ac375169f1` (`verifyLocalReleasePackage`)
- relation: `MAY_MUTATE`
- tail: `repo://e/84f4f7cdfe35c3a27710d02c68ea2c2c` (`relative`)
- source_span: `packages/core/src/release/index.mjs:295-295`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:dab331d28fc8723d`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://7e749c226c45f850e62f4b634e35606a, edge://6d642ffc8500674f6a145035d5558189`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
293:   for (const relative of manifest.files ?? []) {
294:     const file = path.join(root, relative);
295:     if (!inside(root, file)) {
296:       errors.push(`${relative} escapes release root`);
297:       continue;
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
- edge_id: `edge-key:-8813299951664219945`
- head: `repo://e/0adf36ba28e917588f7acf5126fc6afa` (`refreshRunArtifacts`)
- relation: `MAY_MUTATE`
- tail: `repo://e/7406383e37c4487445e3eea072612f2a` (`supportedClaims`)
- source_span: `packages/core/src/run-ledger/finalize.mjs:32-32`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:4a367677c0432073`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://0851573dd3d998b9f49095a36392070f, edge://aef397dba95c8e4f05a810fa49d9db69`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
30:   const hypotheses = await readJson(path.join(runRoot, "artifacts", "hypotheses.json"));
31:   const experimentPlan = await readJson(path.join(runRoot, "artifacts", "experiment_plan.json"));
32:   const evidenceHealth = await computeEvidenceHealth({ claims, evidence, runRoot });
33:   const verifierReport = await verifyRun(runRoot);
34:   const report = buildReport({
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
- edge_id: `edge-key:-8810810114769138617`
- head: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb` (`main`)
- relation: `MAY_MUTATE`
- tail: `repo://e/8dff277826599effc8318c57c6455fd1` (`expr@13341`)
- source_span: `packages/cli/src/arl.mjs:699-699`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:569fb7a80d03a924`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://58fef6d741d6b840260e3fe088fd406a, edge://2c41399c08cbfb6a18c901f6f169ce18`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
700:   } else if (command === "approvals" && subcommand === "list") {
701:     ok = await approvalsList(repoRoot, rest);
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
- edge_id: `edge-key:-8810240614056624530`
- head: `repo://e/9410fe958e8e2a03c13648a8ae94c1e9` (`run`)
- relation: `MAY_MUTATE`
- tail: `repo://e/4236a693e3f9ed82eb66b970b837c39c` (`hashes`)
- source_span: `packages/core/src/runtime-adapters/dry-run.mjs:10-10`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:03a9ba0cddc8dbda`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://866f32af47a2b137f0744ff0605c1211, edge://dd37e470ba1037f16472af8a6dd43e4f`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
8: 
9:   async run({ objective, objectiveRaw, repoRoot, runId }) {
10:     return createDryRunFolder({ objective, objectiveRaw, repoRoot, runId });
11:   }
12: }
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
- edge_id: `edge-key:-8804056520487007053`
- head: `repo://e/524493e51ede3dae9f03883e22a83eb0` (`loadDomainPacks`)
- relation: `MAY_MUTATE`
- tail: `repo://e/a686093f16d531077152957ab88adf99` (`target`)
- source_span: `packages/ui-autoresearch/src/app.js:387-387`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:52a5b8c00049eaea`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://7e6dd1c603a8c59983cd71a14ca4fcd3, edge://a11898828c034e1339984f5682dc6fdd`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
385:     if (!response.ok) throw new Error(`domain-packs ${response.status}`);
386:     const payload = await response.json();
387:     renderList("domain-packs", payload.domain_packs ?? [], (pack) => `<div class="row"><b>${escapeHtml(pack.domain)}</b><span>${escapeHtml(pack.valid ? "valid" : "invalid")} ${escapeHtml(pack.capabilities?.length ?? 0)} capabilities</span></div>`);
388:   } catch {
389:     renderList("domain-packs", [], () => "");
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
- edge_id: `edge-key:-8800715633062847442`
- head: `repo://e/706624125c0c823dae125e779563113b` (`loadDashboard`)
- relation: `MAY_MUTATE`
- tail: `repo://e/027924db02fdfe9a0bdbb178bfd4026c` (`completedRuns`)
- source_span: `packages/ui-autoresearch/src/app.js:527-527`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:52a5b8c00049eaea`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://a133d8e0b4cb5c041960f2499f6ce892, edge://bab337da76cacc38be5aea7e725d219e`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
525:     render(payload.selected ? payload : samplePayload);
526:   } catch {
527:     render(samplePayload);
528:   }
529: }
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
- edge_id: `edge-key:-8780122847374215323`
- head: `repo://e/266cb24c8291651a300c0d8fac2b2f84` (`main`)
- relation: `MAY_MUTATE`
- tail: `repo://e/e4cc6f337c80b1b42e1f9fc70db9134e` (`arg`)
- source_span: `scripts/verify-local-release.mjs:30-30`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6d932e651b0ef5c3`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://9662fccc3207df2038dd968d0363c062, edge://826805906ddbfe1057adfb7200362f62`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
28: 
29: async function main() {
30:   const args = parseArgs(process.argv.slice(2));
31:   if (args.help) {
32:     console.log(usage());
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
- edge_id: `edge-key:-8778886124772100430`
- head: `repo://e/361f02fa936d8a6dbf41037d332a191f` (`createDryRunFolder`)
- relation: `MAY_MUTATE`
- tail: `repo://e/3b7b84b44883dca2f3dbd28d36234590` (`artifactPath`)
- source_span: `packages/core/src/run-ledger/index.mjs:152-152`
- relation_direction: `head_to_tail`
- exactness: `derived_from_verified_edges`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:c546747cf6f02fba`
- derived: `true`
- extractor: `codegraph-index-derived-closure`
- fact_classification: `derived`
- context: `production_inferred`
- provenance_edges: `edge://d0980ee38634f52ed86ab9fc455efbe8, edge://a57ca8d18faec72e016f306c0290fb94`
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
150:   await writeText(path.join(runRoot, "report.md"), "Draft report pending verifier results.\n");
151: 
152:   const evidenceHealth = await computeEvidenceHealth({ claims, evidence, runRoot });
153:   const verifierReport = await verifyRun(runRoot);
154:   const report = buildReport({ objective, manifest, hypotheses, experimentPlan, claims, evidence, evidenceHealth, verifierReport });
```

