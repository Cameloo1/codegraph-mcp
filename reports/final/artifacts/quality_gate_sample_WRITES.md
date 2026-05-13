# Edge Sample Audit

Database: `reports/final/artifacts/compact_gate_autoresearch_proof.sqlite`

Relation filter: `WRITES`

Limit: `50`

Seed: `44`

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
- edge_id: `edge-key:-9215340196587489650`
- head: `repo://e/5e3a9c7b20f3d7718002b0dbfd34c2b7` (`mapCodexEvent`)
- relation: `WRITES`
- tail: `repo://e/4cb8712fc975fae910567846f97a2223` (`expr@1318`)
- source_span: `packages/core/src/codex-event-mapper.mjs:40-40`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:63e71cbf4a3bf24b`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
38:     event: codexEvent
39:   });
40:   if (evidenceCandidate) mapped.evidence_candidate = evidenceCandidate;
41:   return mapped;
42: }
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
- edge_id: `edge-key:-9208564640050135149`
- head: `repo://e/cd1163511c941bc1e83aeb9bd3e4243b` (`codexPatchStatus`)
- relation: `WRITES`
- tail: `repo://e/8e00619b31839586e69789fa9ef2a558` (`patchset`)
- source_span: `packages/cli/src/arl.mjs:502-502`
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
500:     return false;
501:   }
502:   const patchset = await readJson(patchsetPath);
503:   if (args.includes("--json")) {
504:     console.log(JSON.stringify(patchset, null, 2));
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
- edge_id: `edge-key:-9202905102190226028`
- head: `repo://e/8d799aad3765ece9dbe32c5f532f60cb` (`computeEvidenceHealth`)
- relation: `WRITES`
- tail: `repo://e/a8c98225d30e9237f174e05963713ee9` (`weakEvidence`)
- source_span: `packages/core/src/evidence/index.mjs:143-143`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:8860fed84f027108`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
141: 
142:   const contradicted = claims.filter((claim) => claim.status === "contradicted").length;
143:   const weakEvidence = evidence.filter((item) => item.strength === "weak" || item.strength === "unknown").length;
144:   const label = issues.some((issue) => ["missing_artifact", "hash_mismatch", "unsupported_supported_claim"].includes(issue.kind))
145:     ? "invalid"
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
- edge_id: `edge-key:-9202542174198038300`
- head: `repo://e/39feb7d8c786454817c6c43d93e1dcb6` (`readObjectives`)
- relation: `WRITES`
- tail: `repo://e/db98bf22857bd4dda97a08ed28e28f4b` (`objectives`)
- source_span: `packages/cli/src/dashboard-server.mjs:83-83`
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
81:     path.join(repoRoot, ".arl", "objectives")
82:   ];
83:   const objectives = [];
84:   for (const root of roots) {
85:     if (!(await pathExists(root))) continue;
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
- edge_id: `edge-key:-9200855074761782271`
- head: `repo://e/fed901831c175728f877e68f803da562` (`runOpenRouterAgentObjective`)
- relation: `WRITES`
- tail: `repo://e/3f8d8430de2b5659c2717a1b93b7db27` (`expr@1869`)
- source_span: `services/orchestrator/src/run-manager.mjs:54-54`
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
52:   const manifestPath = path.join(run.runRoot, "manifest.json");
53:   const manifest = await readJson(manifestPath);
54:   manifest.runtime.kind = "openrouter_single_agent";
55:   manifest.runtime.codex_source = "model_runtime";
56:   manifest.summary.single_agent = true;
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
- edge_id: `edge-key:-9192023821006431121`
- head: `repo://e/ad34b4b3bfc2c20be5b17731d6039a29` (`startDashboardAgentPromptRun`)
- relation: `WRITES`
- tail: `repo://e/4e9b7652f8021233d2bee0cc731512c5` (`cleanup`)
- source_span: `packages/cli/src/dashboard-server.mjs:872-872`
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
870:       activeRun.done = true;
871:       activeRun.finished_at = new Date().toISOString();
872:       const cleanup = setTimeout(() => activeRuns.delete(started.runId), 300000);
873:       cleanup.unref?.();
874:     });
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
- edge_id: `edge-key:-9180823349458361676`
- head: `repo://e/7c92464e5b7b171e46d422cd54ad7844` (`toggleDeveloper`)
- relation: `WRITES`
- tail: `repo://e/ae0386960ba887e701cc9e2536fe3855` (`drawer`)
- source_span: `packages/ui-autoresearch/src/app.js:532-532`
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
530: 
531: function toggleDeveloper(open) {
532:   const drawer = document.getElementById("developer-drawer");
533:   drawer.classList.toggle("open", open);
534:   drawer.setAttribute("aria-hidden", open ? "false" : "true");
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
- edge_id: `edge-key:-9180652435139003731`
- head: `repo://e/7af59e8a472dea9e968d7a1e82badaf4` (`runBundledCodexObjective`)
- relation: `WRITES`
- tail: `repo://e/49f6b323a65103839569ba493bd28241` (`claims`)
- source_span: `services/orchestrator/src/run-manager.mjs:275-275`
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
273:   const infoHash = await sha256File(path.join(run.runRoot, "logs", "bundled-runtime-info.json"));
274:   const evidence = await readJson(path.join(run.runRoot, "evidence.json"));
275:   const claims = await readJson(path.join(run.runRoot, "claims.json"));
276:   evidence.push({
277:     evidence_id: "evidence_bundled_runtime_001",
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
- edge_id: `edge-key:-9174860681805008942`
- head: `repo://e/be5f762336ae802e761e4cdb950539e0` (`startFreeformAgentPrompt`)
- relation: `WRITES`
- tail: `repo://e/375e6454803284f28d5e809599809a78` (`preparedRunId`)
- source_span: `services/orchestrator/src/freeform-agent-runner.mjs:217-217`
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
215:   const created = await createFreeformObjectiveFile(prompt, repoRoot, { now });
216:   const loaded = await loadObjective(created.path);
217:   const preparedRunId = runId ?? undefined;
218:   const runRoot = preparedRunId ? path.join(repoRoot, ".arl", "runs", preparedRunId) : null;
219:   const workspace = runRoot ? path.join(runRoot, "workspace") : null;
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
- edge_id: `edge-key:-9165705249699042036`
- head: `repo://e/df6b6705e2f1a4b3d74851dc4dafddf9` (`start`)
- relation: `WRITES`
- tail: `repo://e/dcd952bb43cdfded29be7fada1f50d13` (`manifestPath`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:206-206`
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
204:     const pendingWrites = [];
205: 
206:     const manifestPath = path.join(runRoot, "manifest.json");
207:     const manifest = await readJson(manifestPath);
208:     manifest.runtime.kind = "external_codex_cli";
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
- edge_id: `edge-key:-9145116956986869643`
- head: `repo://e/94e44b598f56da775171b69264b47e51` (`resolveCurrentVersion`)
- relation: `WRITES`
- tail: `repo://e/efc5d7903e5c963e1b32e2641d2262d8` (`packageJson`)
- source_span: `packages/core/src/release/update-checker.mjs:46-46`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:e41f3f3805b4c45d`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
44:   const packagePath = path.join(repoRoot, "package.json");
45:   if (!(await pathExists(packagePath))) return null;
46:   const packageJson = await readJson(packagePath);
47:   return packageJson.version ?? null;
48: }
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
- edge_id: `edge-key:-9133652800344259384`
- head: `repo://e/df6b6705e2f1a4b3d74851dc4dafddf9` (`start`)
- relation: `WRITES`
- tail: `repo://e/d96a124c6ec57bb76028c8fb07c8685d` (`stdoutBuffer`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:201-201`
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
199:     let sequence = 1000;
200:     let cancelled = false;
201:     let stdoutBuffer = "";
202:     let mappedEvents = 0;
203:     let parseFailureEmitted = false;
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
- edge_id: `edge-key:-9128238369934451022`
- head: `repo://e/4469d79a5efe2d3aaa9b0f070fdff1ec` (`loadLocalEnv`)
- relation: `WRITES`
- tail: `repo://e/dc2dab64f630a7dd445b21c9e6ccb413` (`key`)
- source_span: `packages/cli/src/arl.mjs:86-86`
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
84:       const separator = trimmed.indexOf("=");
85:       if (separator === -1) continue;
86:       const key = trimmed.slice(0, separator).trim();
87:       let value = trimmed.slice(separator + 1).trim();
88:       if ((value.startsWith("\"") && value.endsWith("\"")) || (value.startsWith("'") && value.endsWith("'"))) {
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
- edge_id: `edge-key:-9120571736833999021`
- head: `repo://e/baeefa615b9e301d5d2a1f9e8adb77c3` (`writeReleaseManifest`)
- relation: `WRITES`
- tail: `repo://e/313cc1675975346247a966707e1a9ca4` (`manifest`)
- source_span: `packages/core/src/release/index.mjs:271-280`
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
269:   await writeText(path.join(releaseRoot, "RELEASE_CHECKSUMS.txt"), `${checksumLines}\n`);
270:   hashes["RELEASE_CHECKSUMS.txt"] = await sha256File(path.join(releaseRoot, "RELEASE_CHECKSUMS.txt"));
271:   const manifest = {
272:     name: "autoresearch-lab",
273:     version,
274:     created_at: new Date().toISOString(),
275:     source_root: repoRoot,
276:     copied,
277:     missing_optional: missingOptional,
278:     files: Object.keys(hashes).sort(),
279:     hashes
280:   };
281:   await writeJson(path.join(releaseRoot, "RELEASE_MANIFEST.json"), manifest);
282:   return manifest;
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
- edge_id: `edge-key:-9106978301333808965`
- head: `repo://e/be5f762336ae802e761e4cdb950539e0` (`startFreeformAgentPrompt`)
- relation: `WRITES`
- tail: `repo://e/cb558437d2162bafe9db415eda03d929` (`loaded`)
- source_span: `services/orchestrator/src/freeform-agent-runner.mjs:216-216`
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
214: ) {
215:   const created = await createFreeformObjectiveFile(prompt, repoRoot, { now });
216:   const loaded = await loadObjective(created.path);
217:   const preparedRunId = runId ?? undefined;
218:   const runRoot = preparedRunId ? path.join(repoRoot, ".arl", "runs", preparedRunId) : null;
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
- edge_id: `edge-key:-9098133026961486043`
- head: `repo://e/466845c5e7febbb213e0844ee060badc` (`loadRunDetail`)
- relation: `WRITES`
- tail: `repo://e/2514afbc2cf141dfd3bc3ea69584f9b5` (`appserverEvents`)
- source_span: `packages/cli/src/dashboard-server.mjs:276-276`
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
274:   const evidence = await maybeReadJson(path.join(runRoot, "evidence.json"), []);
275:   const verifier = await maybeReadJson(path.join(runRoot, "verifier_report.json"), null);
276:   const appserverEvents = await maybeReadJsonl(path.join(runRoot, "logs", "appserver-events.jsonl"));
277:   const multiAgentPlan = await maybeReadJson(path.join(runRoot, "artifacts", "multi_agent_plan.json"), null);
278:   const evidenceHealth = claims.length > 0 || evidence.length > 0
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
- edge_id: `edge-key:-9097652859107275988`
- head: `repo://e/df6b6705e2f1a4b3d74851dc4dafddf9` (`start`)
- relation: `WRITES`
- tail: `repo://e/328ac3a5394445d0626d6f3076404ee6` (`expr@11684`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:339-339`
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
337: 
338:         const finalManifest = await readJson(manifestPath);
339:         finalManifest.status = cancelled ? "cancelled" : code === 0 ? "completed" : "failed";
340:         finalManifest.summary.external_exit_code = code;
341:         finalManifest.summary.external_signal = signal;
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
- edge_id: `edge-key:-9094927754688540153`
- head: `repo://e/6135c46a53c2efb9db26e112df69e3d3` (`createLocalReleasePackage`)
- relation: `WRITES`
- tail: `repo://e/bb009d6dc71a26b06e2bfa91345f9902` (`root`)
- source_span: `packages/core/src/release/index.mjs:353-353`
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
351: 
352: export async function createLocalReleasePackage({ repoRoot, outRoot, clean = true, signing = null } = {}) {
353:   const root = path.resolve(repoRoot ?? process.cwd());
354:   const plan = await planLocalReleasePackage({ repoRoot, outRoot });
355:   assertSafeReleaseRoot(plan.output_root, plan.release_root);
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
- edge_id: `edge-key:-9092542865943923434`
- head: `repo://e/785b03f7c0fdd87c331e09b97cc61606` (`planLocalReleasePackage`)
- relation: `WRITES`
- tail: `repo://e/e2d0dcf255e77ae72ba55baae3ecb23d` (`version`)
- source_span: `packages/core/src/release/index.mjs:331-331`
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
329:   const root = path.resolve(repoRoot ?? process.cwd());
330:   const packageJson = await readJson(path.join(root, "package.json"));
331:   const version = packageJson.version;
332:   const outputRoot = path.resolve(outRoot ?? path.join(root, "dist"));
333:   const releaseRoot = path.join(outputRoot, releaseDirName(version));
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
- edge_id: `edge-key:-9090993692629347788`
- head: `repo://e/a9cb13629807385d928f76eac994babf` (`parseKeyValue`)
- relation: `WRITES`
- tail: `repo://e/338b4aeaa7dac78e5211d38a1c0c10c8` (`key`)
- source_span: `packages/core/src/simple-yaml.mjs:37-37`
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
35:     return [text, undefined];
36:   }
37:   const key = text.slice(0, colon).trim();
38:   const rest = text.slice(colon + 1).trim();
39:   return [key, rest.length === 0 ? undefined : parseScalar(rest)];
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
- edge_id: `edge-key:-9089622480571185939`
- head: `repo://e/4a2278bc920360444b92b49ebca96a76` (`runCommand`)
- relation: `WRITES`
- tail: `repo://e/471c5907a5f0607dfa744387b33d9f23` (`reasoningEffort`)
- source_span: `packages/cli/src/arl.mjs:204-204`
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
202:     const model = flagValue(args, "--model") ?? process.env.OPENROUTER_MODEL;
203:     const endpoint = flagValue(args, "--endpoint") ?? process.env.OPENROUTER_BASE_URL;
204:     const reasoningEffort = flagValue(args, "--reasoning");
205:     const adapter = new OpenRouterRuntimeAdapter({ model, endpoint });
206:     const located = adapter.locate();
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
- edge_id: `edge-key:-9083576562086082197`
- head: `repo://e/81fadbac75286413785474f905f73a65` (`startDashboardLiveRun`)
- relation: `WRITES`
- tail: `repo://e/fb2b8f912d53f1f9c986a23beab3c4c7` (`loaded`)
- source_span: `packages/cli/src/dashboard-server.mjs:768-768`
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
766:     throw new Error(`objective path escapes workspace: ${objectivePath}`);
767:   }
768:   const loaded = await loadObjective(absoluteObjectivePath);
769:   const validation = await validateObjective(loaded.data, repoRoot);
770:   if (!validation.valid) {
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
- edge_id: `edge-key:-9081117072560275925`
- head: `repo://e/b2391f6afb3aebab03c902195ca9aff1` (`streamRunEvents`)
- relation: `WRITES`
- tail: `repo://e/ec5fb0f5de50d64c33c2d13b45163f6e` (`closed`)
- source_span: `packages/cli/src/dashboard-server.mjs:374-374`
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
372:         const cleanup = () => {
373:           if (closed) return;
374:           closed = true;
375:           activeRun.listeners?.delete(listener);
376:           resolve();
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
- edge_id: `edge-key:-9077241256724152616`
- head: `repo://e/a834d4979bd40b385462a7c8de111366` (`runAppServerObjective`)
- relation: `WRITES`
- tail: `repo://e/a8a5d31f1520246c5ac8ac66f218c5ea` (`hashes`)
- source_span: `services/orchestrator/src/run-manager.mjs:212-212`
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
210:     }));
211: 
212:     const hashes = {};
213:     for (const file of await walkFiles(run.runRoot)) {
214:       const relative = path.relative(run.runRoot, file).replaceAll("\\", "/");
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
- edge_id: `edge-key:-9063180648632075272`
- head: `repo://e/81fadbac75286413785474f905f73a65` (`startDashboardLiveRun`)
- relation: `WRITES`
- tail: `repo://e/5cd8e3f2bc7a5200af5fd28a813a8e12` (`liveRuntimes`)
- source_span: `packages/cli/src/dashboard-server.mjs:774-789`
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
772:   }
773:   const runtime = body.runtime ?? "dry_run";
774:   const liveRuntimes = new Set([
775:     "dry_run",
776:     "app_server_shim",
777:     "bundled_codex",
778:     "bundled-codex",
779:     "arl_codexd",
780:     "arl-codexd",
781:     "multi_agent_shim",
782:     "external_codex_cli",
783:     "external-codex",
784:     "openrouter",
785:     "openrouter_model",
786:     "openrouter_single_agent",
787:     "openrouter-agent",
788:     "openrouter_agent"
789:   ]);
790:   if (!liveRuntimes.has(runtime)) {
791:     throw new Error(`unsupported live dashboard runtime: ${runtime}`);
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
- edge_id: `edge-key:-9047897783447882203`
- head: `repo://e/0413dbfce237f6db4ab555ac375169f1` (`verifyLocalReleasePackage`)
- relation: `WRITES`
- tail: `repo://e/6601becf269c36c842849209a8f79b02` (`signing`)
- source_span: `packages/core/src/release/index.mjs:316-318`
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
314:   }
315:   const signingPath = path.join(root, "SIGNING_STATUS.json");
316:   const signing = await pathExists(signingPath)
317:     ? await readJson(signingPath)
318:     : { status: "missing", signed: false, reason: "SIGNING_STATUS.json missing" };
319:   return {
320:     valid: errors.length === 0,
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
- edge_id: `edge-key:-9032001772324398413`
- head: `repo://e/7af59e8a472dea9e968d7a1e82badaf4` (`runBundledCodexObjective`)
- relation: `WRITES`
- tail: `repo://e/a0f60d72dec78b035c6f7b107b794229` (`verification`)
- source_span: `services/orchestrator/src/run-manager.mjs:300-300`
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
298:   await writeJson(path.join(run.runRoot, "claims.json"), claims);
299: 
300:   const verification = await refreshRunArtifacts({ runRoot: run.runRoot, objective: loaded.data, manifest });
301:   return { ...run, manifest, runtimeInfo, verification };
302: }
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
- edge_id: `edge-key:-9015413048156194913`
- head: `repo://e/593402e83ab3c00b67d06fb02128254d` (`runExternalCodexObjective`)
- relation: `WRITES`
- tail: `repo://e/d87d040c4d70dc3ab777164f84d31034` (`adapter`)
- source_span: `services/orchestrator/src/run-manager.mjs:117-117`
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
115:     throw new Error(validation.errors.join("\n"));
116:   }
117:   const adapter = new ExternalCodexExecAdapter();
118:   return adapter.run({
119:     objective: loaded.data,
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
- edge_id: `edge-key:-9011741606627076278`
- head: `repo://e/b7ed1c10439a9633f97ad8d8b21b603d` (`runsShow`)
- relation: `WRITES`
- tail: `repo://e/d792ccf38cb0640f64ba5f7d9d48de85` (`manifestPath`)
- source_span: `packages/cli/src/arl.mjs:267-267`
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
265: 
266: async function runsShow(repoRoot, runId) {
267:   const manifestPath = path.join(repoRoot, ".arl", "runs", runId, "manifest.json");
268:   if (!(await pathExists(manifestPath))) {
269:     console.error(`run not found: ${runId}`);
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
- edge_id: `edge-key:-8999559970389029821`
- head: `repo://e/df10a5807c4516d00e4abfeb6866c37e` (`readPromptRegistrySummary`)
- relation: `WRITES`
- tail: `repo://e/ad76175782f62b5f3e11772bad32e005` (`prompts`)
- source_span: `packages/cli/src/dashboard-server.mjs:558-558`
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
556: 
557: async function readPromptRegistrySummary(repoRoot) {
558:   const prompts = await readRegistryJsonFiles(path.join(repoRoot, ".arl", "prompts"));
559:   return {
560:     prompts: prompts.map(({ path: promptPath, record }) => ({
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
- edge_id: `edge-key:-8997187641966570880`
- head: `repo://e/1544a636541b7add226fe4fa49064947` (`startFakeOpenRouter`)
- relation: `WRITES`
- tail: `repo://e/9fd325703580aaa5dfcfb37eaaf90cf3` (`address`)
- source_span: `tests/openrouter-adapter.test.mjs:44-44`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:62aed147c51df242`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
42:   return new Promise((resolve) => {
43:     server.listen(0, "127.0.0.1", () => {
44:       const address = server.address();
45:       resolve({
46:         server,
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
- edge_id: `edge-key:-8972474331098962160`
- head: `repo://e/afb22024d68b612c5c6ee74d2771a1b2` (`renderList`)
- relation: `WRITES`
- tail: `repo://e/a686093f16d531077152957ab88adf99` (`target`)
- source_span: `packages/ui-autoresearch/src/app.js:74-74`
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
72: 
73: function renderList(id, rows, formatter) {
74:   const target = document.getElementById(id);
75:   if (!target) return;
76:   target.innerHTML = rows.length === 0 ? "<div class=\"empty\">None</div>" : rows.map(formatter).join("");
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
- edge_id: `edge-key:-8967458845026127082`
- head: `repo://e/65cd7e8e450b406dc4f0203c711c09c3` (`refreshRunArtifacts`)
- relation: `WRITES`
- tail: `repo://e/aa7b68f99ce52096cfbbcbc163677364` (`relative`)
- source_span: `services/orchestrator/src/multi-agent-runner.mjs:63-63`
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
61:   const hashes = {};
62:   for (const file of await walkFiles(runRoot)) {
63:     const relative = path.relative(runRoot, file).replaceAll("\\", "/");
64:     if (relative === "hashes.json") continue;
65:     if (!(await pathExists(file))) continue;
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
- edge_id: `edge-key:-8965308638854400202`
- head: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb` (`main`)
- relation: `WRITES`
- tail: `repo://e/4445deeb8c986e4cc85277d92de614de` (`ok`)
- source_span: `packages/cli/src/arl.mjs:693-693`
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
691:     ok = await agentRunCommand(repoRoot, rest);
692:   } else if (command === "runs" && subcommand === "list") {
693:     ok = await runsList(repoRoot);
694:   } else if (command === "runs" && subcommand === "show") {
695:     ok = await runsShow(repoRoot, rest[0]);
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
- edge_id: `edge-key:-8953577217891063241`
- head: `repo://e/2cbc2b6a0e14781c9fe0a936243da302` (`compareVersions`)
- relation: `WRITES`
- tail: `repo://e/cf35f3505ad2747383b9ac18db5788cd` (`width`)
- source_span: `packages/core/src/release/update-checker.mjs:14-14`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:e41f3f3805b4c45d`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
12:   const leftParts = versionParts(left);
13:   const rightParts = versionParts(right);
14:   const width = Math.max(leftParts.length, rightParts.length);
15:   for (let index = 0; index < width; index += 1) {
16:     const leftPart = leftParts[index] ?? 0;
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
- edge_id: `edge-key:-8953425695233741466`
- head: `repo://e/a834d4979bd40b385462a7c8de111366` (`runAppServerObjective`)
- relation: `WRITES`
- tail: `repo://e/9fb53f1ab57f2161f2059c18aea4cb0b` (`eventLogHash`)
- source_span: `services/orchestrator/src/run-manager.mjs:161-161`
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
159:     await session.drain();
160:     const eventLog = path.join(run.runRoot, "logs", "appserver-events.jsonl");
161:     const eventLogHash = await sha256File(eventLog);
162:     const evidence = await readJson(path.join(run.runRoot, "evidence.json"));
163:     const claims = await readJson(path.join(run.runRoot, "claims.json"));
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
- edge_id: `edge-key:-8952414981480967847`
- head: `repo://e/6cc104eab113bce8cfd8e4f32771e0ea` (`start`)
- relation: `WRITES`
- tail: `repo://e/0e35e484fb14121cd3d59063550187ca` (`rl`)
- source_span: `packages/appserver-client/src/jsonrpc-client.mjs:22-22`
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
20:       stdio: ["pipe", "pipe", "pipe"]
21:     });
22:     const rl = readline.createInterface({ input: this.process.stdout });
23:     rl.on("line", (line) => {
24:       if (!line.trim()) return;
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
- edge_id: `edge-key:-8944346721100125203`
- head: `repo://e/2993c700973004d60a10ea15d1379e18` (`evidenceCheck`)
- relation: `WRITES`
- tail: `repo://e/340bee055ecbfe05c22dafb2d72ee57f` (`claims`)
- source_span: `packages/cli/src/arl.mjs:365-365`
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
363: async function evidenceCheck(repoRoot, runId) {
364:   const runRoot = path.join(repoRoot, ".arl", "runs", runId);
365:   const claims = await readJson(path.join(runRoot, "claims.json"));
366:   const evidence = await readJson(path.join(runRoot, "evidence.json"));
367:   const health = await computeEvidenceHealth({ claims, evidence, runRoot });
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
- edge_id: `edge-key:-8943212322656166210`
- head: `repo://e/9dbd5ce7b7f975e0c58de4e139d2ed96` (`validateToolPolicy`)
- relation: `WRITES`
- tail: `repo://e/d566bedff7704e79d4b60751ee9642f3` (`toolId`)
- source_span: `packages/core/src/permissions/policy.mjs:87-87`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:911cf7048e46e0e8`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
85:     require_manifests: true
86:   };
87:   const toolId = request.tool_id ?? request.name ?? "";
88:   if ((tools.deny ?? []).includes(toolId)) {
89:     return { allowed: false, reason: `tool ${toolId || "unknown"} is denied` };
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
- edge_id: `edge-key:-8919897977830147980`
- head: `repo://e/7ffd3503439db213c215f55c1fa418ad` (`bundleCommand`)
- relation: `WRITES`
- tail: `repo://e/9a03c389988a03eb8e1f5f4daae16145` (`manifest`)
- source_span: `packages/cli/src/arl.mjs:396-396`
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
394:   }
395:   if (action === "create") {
396:     const manifest = await createBundleManifest(runRoot);
397:     console.log(`bundle_manifest=${path.relative(repoRoot, path.join(runRoot, "bundle_manifest.json"))}`);
398:     console.log(`files=${manifest.files.length}`);
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
- edge_id: `edge-key:-8919476110978504447`
- head: `repo://e/ea34faff8dff2f52fca08295aa9e2174` (`buildReproducibilitySummary`)
- relation: `WRITES`
- tail: `repo://e/4e16613ac4d169e10e58491e3681849b` (`claims`)
- source_span: `packages/core/src/reproducibility/index.mjs:75-75`
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
73: async function buildReproducibilitySummary(runRoot) {
74:   const manifest = await readOptionalJson(path.join(runRoot, "manifest.json"), {});
75:   const claims = await readOptionalJson(path.join(runRoot, "claims.json"), []);
76:   const verifierReport = await readOptionalJson(path.join(runRoot, "verifier_report.json"), {});
77:   const claimCounts = countClaims(Array.isArray(claims) ? claims : []);
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
- edge_id: `edge-key:-8906806787292127771`
- head: `repo://e/3d0dc448bebd8592894074a6dd2f4ebc` (`serveStatic`)
- relation: `WRITES`
- tail: `repo://e/c0e865d37abe2c1b788453ebce1e0e71` (`relative`)
- source_span: `packages/cli/src/dashboard-server.mjs:924-924`
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
922: async function serveStatic(repoRoot, requestPath, response) {
923:   const uiRoot = path.join(repoRoot, "packages", "ui-autoresearch");
924:   const relative = requestPath === "/" ? "index.html" : decodeURIComponent(requestPath.slice(1));
925:   const target = path.join(uiRoot, relative);
926:   if (!isPathInside(uiRoot, target) || !(await pathExists(target))) {
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
- edge_id: `edge-key:-8903775164370378693`
- head: `repo://e/4bb4520ae6ec7c94b8660e5d9d61c9da` (`createObjectiveFromFreeformPrompt`)
- relation: `WRITES`
- tail: `repo://e/b92ba42224e157a2a5df2b529a4c2964` (`networkMode`)
- source_span: `services/orchestrator/src/freeform-agent-runner.mjs:39-39`
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
37:   if (!trimmed) throw new Error("prompt is required");
38:   const id = `${slugify(trimmed, "agent-objective")}-${shortHash(trimmed)}`;
39:   const networkMode = wantsNetwork(trimmed) ? "allowlist" : "none";
40:   return {
41:     id,
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
- edge_id: `edge-key:-8901578736393364825`
- head: `repo://e/194374fdc015ef33d9755a3b74941b89` (`verifyRun`)
- relation: `WRITES`
- tail: `repo://e/24d73ba9a4e1ae2d975e90853ef077f7` (`evidence`)
- source_span: `packages/core/src/verifier/index.mjs:7-7`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:09d3127b96919f09`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
5: export async function verifyRun(runRoot) {
6:   const claims = await readJson(path.join(runRoot, "claims.json"));
7:   const evidence = await readJson(path.join(runRoot, "evidence.json"));
8:   const manifest = await readJson(path.join(runRoot, "manifest.json"));
9:   const required = [
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
- edge_id: `edge-key:-8901544956913911669`
- head: `repo://e/ad34b4b3bfc2c20be5b17731d6039a29` (`startDashboardAgentPromptRun`)
- relation: `WRITES`
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
- edge_id: `edge-key:-8889412470864569422`
- head: `repo://e/81fadbac75286413785474f905f73a65` (`startDashboardLiveRun`)
- relation: `WRITES`
- tail: `repo://e/56881a60e4d741a5742041c4b599a763` (`objectivePath`)
- source_span: `packages/cli/src/dashboard-server.mjs:763-763`
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
761: 
762: async function startDashboardLiveRun(repoRoot, body, activeRuns) {
763:   const objectivePath = body.objective_path ?? "examples/objectives/code-bugfix.yaml";
764:   const absoluteObjectivePath = path.resolve(repoRoot, objectivePath);
765:   if (!isPathInside(repoRoot, absoluteObjectivePath)) {
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
- edge_id: `edge-key:-8871751888364828719`
- head: `repo://e/aee2f7bdea85439bb2c8dec26ec8fbb9` (`releasePackage`)
- relation: `WRITES`
- tail: `repo://e/24dd3eda6acbcf4ce9cd5df39a8f677a` (`verification`)
- source_span: `packages/cli/src/arl.mjs:594-594`
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
592:     signing: Object.keys(signing).length > 0 ? signing : null
593:   });
594:   const verification = verify ? await verifyLocalReleasePackage({ releaseRoot: result.release_root }) : null;
595:   console.log(`release_root=${path.relative(repoRoot, result.release_root)}`);
596:   console.log(`files=${result.manifest.files.length}`);
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
- edge_id: `edge-key:-8859665890066271334`
- head: `repo://e/7f536bc0546080420a1dbfb8d727fc50` (`summarizeResult`)
- relation: `WRITES`
- tail: `repo://e/1093ce268359610d2acc9c1e732602ab` (`lines`)
- source_span: `scripts/package-local-release.mjs:71-76`
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
69: 
70: function summarizeResult(result) {
71:   const lines = [
72:     `release_root=${path.relative(process.cwd(), result.release_root) || "."}`,
73:     `files=${result.manifest.files.length}`,
74:     `missing_optional=${result.missing_optional.length === 0 ? "none" : result.missing_optional.join(",")}`,
75:     "status=packaged"
76:   ];
77:   if (result.verification) {
78:     lines.push(`verified=${result.verification.valid}`);
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
- edge_id: `edge-key:-8844447273245716914`
- head: `repo://e/eedb6020cc98bbc557c1b9aabc1f9c37` (`emitAvailableEvents`)
- relation: `WRITES`
- tail: `repo://e/607d7b676b23afd095fec9f0ccead546` (`index`)
- source_span: `packages/cli/src/dashboard-server.mjs:346-346`
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
344:   async function emitAvailableEvents() {
345:     const events = await readRunEvents(repoRoot, runId);
346:     for (let index = 0; index < events.length; index += 1) {
347:       const event = events[index];
348:       const key = eventKey(event, index);
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
- edge_id: `edge-key:-8830616634248625129`
- head: `repo://e/3bf1afc4e9ca7f0fd72cd9eaab7fdaf1` (`proposeDashboardTool`)
- relation: `WRITES`
- tail: `repo://e/7373068a504833d21d7bcc87301e1fd8` (`tool`)
- source_span: `packages/cli/src/dashboard-server.mjs:540-549`
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
538: 
539: async function proposeDashboardTool(repoRoot, body) {
540:   const tool = {
541:     tool_id: body.tool_id,
542:     name: body.name ?? body.tool_id,
543:     input_schema: body.input_schema ?? {},
544:     output_schema: body.output_schema ?? {},
545:     permission_manifest: body.permission_manifest ?? defaultPermissionManifest(),
546:     tests: body.tests ?? [],
547:     docs: body.docs ?? "",
548:     verifier_approved: body.verifier_approved === true
549:   };
550:   return proposeTool({ registryRoot: path.join(repoRoot, ".arl", "tools"), tool });
551: }
```

