# Edge Sample Audit

Database: `reports/final/artifacts/compact_gate_autoresearch_proof.sqlite`

Relation filter: `READS`

Limit: `50`

Seed: `43`

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
- edge_id: `edge-key:-9215154492584872662`
- head: `repo://e/361f02fa936d8a6dbf41037d332a191f` (`createDryRunFolder`)
- relation: `READS`
- tail: `repo://e/eebba379b5c0d9ddcd3764f2aa118827` (`runRoot`)
- source_span: `packages/core/src/run-ledger/index.mjs:35-35`
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
33:   const workspaceRoot = path.join(runRoot, "workspace");
34:   const artifactsRoot = path.join(runRoot, "artifacts");
35:   const logsRoot = path.join(runRoot, "logs");
36:   await ensureDir(workspaceRoot);
37:   await ensureDir(artifactsRoot);
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
- edge_id: `edge-key:-9210500912914085573`
- head: `repo://e/63ceb4904f1c4ff0cdf0a94c2db1912e` (`run`)
- relation: `READS`
- tail: `repo://e/4b39a99d2313c194c576215aded81a9e` (`runRoot`)
- source_span: `packages/core/src/runtime-adapters/openrouter.mjs:150-150`
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
148:       payload: { model: this.model, endpoint: this.endpoint, reasoning: Boolean(reasoningEffort) }
149:     });
150:     await writeJson(path.join(runRoot, "logs", "openrouter-request.json"), redactRequest({ endpoint: this.endpoint, headers, body }));
151: 
152:     let httpStatus = 0;
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
- edge_id: `edge-key:-9204156138496148058`
- head: `repo://e/1b3fb031b810207a3f4f4b0371c36df8` (`createDashboardServer`)
- relation: `READS`
- tail: `repo://e/81fadbac75286413785474f905f73a65` (`startDashboardLiveRun`)
- source_span: `packages/cli/src/dashboard-server.mjs:1016-1016`
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
1014:       }
1015:       if (url.pathname === "/api/runs/start-live" && request.method === "POST") {
1016:         json(response, 202, { run: await startDashboardLiveRun(repoRoot, await readBody(request), activeRuns) });
1017:         return;
1018:       }
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
- edge_id: `edge-key:-9199085964572674028`
- head: `repo://e/1b3fb031b810207a3f4f4b0371c36df8` (`createDashboardServer`)
- relation: `READS`
- tail: `repo://e/6e92182da05fdbc6df0653ec3bd60504` (`json`)
- source_span: `packages/cli/src/dashboard-server.mjs:1062-1062`
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
1060:       await serveStatic(repoRoot, url.pathname, response);
1061:     } catch (error) {
1062:       json(response, 500, { error: error.message });
1063:     }
1064:   });
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
- edge_id: `edge-key:-9198725396124638409`
- head: `repo://e/4a2278bc920360444b92b49ebca96a76` (`runCommand`)
- relation: `READS`
- tail: `repo://e/5593da3abc038e95a69c455f61f39413` (`located`)
- source_span: `packages/cli/src/arl.mjs:208-208`
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
206:     const located = adapter.locate();
207:     if (!located.found) {
208:       console.error(`missing OpenRouter runtime input: ${located.missing.join(", ")}`);
209:       return false;
210:     }
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
- edge_id: `edge-key:-9197003963194729112`
- head: `repo://e/d07fbe86e774ae4ef3f100dcbed24854` (`releaseUpdateCheck`)
- relation: `READS`
- tail: `repo://e/e79c2fd45f73da82ca9a95064a450f98` (`result`)
- source_span: `packages/cli/src/arl.mjs:628-628`
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
626:     return result.status !== "unknown";
627:   }
628:   console.log(`status=${result.status}`);
629:   console.log(`update_available=${result.update_available}`);
630:   console.log(`current_version=${result.current_version ?? "unknown"}`);
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
- edge_id: `edge-key:-9193216128478557094`
- head: `repo://e/6c8f3ad8b546fca794d33589a6683e75` (`parseScalar`)
- relation: `READS`
- tail: `repo://e/357ac43d9fd5af2c5d7e5135b7b38da6` (`trimmed`)
- source_span: `packages/core/src/simple-yaml.mjs:27-27`
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
25:     (trimmed.startsWith("'") && trimmed.endsWith("'"))
26:   ) {
27:     return trimmed.slice(1, -1);
28:   }
29:   return trimmed;
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
- edge_id: `edge-key:-9171850979892593936`
- head: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb` (`main`)
- relation: `READS`
- tail: `repo://e/e8311f4ebaddb60cc81d76d4b4e153f6` (`repoRoot`)
- source_span: `packages/cli/src/arl.mjs:697-697`
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
695:     ok = await runsShow(repoRoot, rest[0]);
696:   } else if (command === "runs" && subcommand === "replay") {
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
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
- edge_id: `edge-key:-9158209197094362958`
- head: `repo://e/82981122f21b1c7c271bf4f71dff4a1e` (`render`)
- relation: `READS`
- tail: `repo://e/49ea343831c589fa7e4a581ce52df450` (`escapeHtml`)
- source_span: `packages/ui-autoresearch/src/app.js:155-155`
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
153:   renderList("claims", detail.claims ?? [], (claim) => `<div class="row"><b>${escapeHtml(claim.status)}</b><span>${escapeHtml(claim.text)}</span></div>`);
154:   renderList("evidence", detail.evidence ?? [], (item) => `<div class="row"><b>${escapeHtml(item.evidence_id)}</b><span>${escapeHtml(item.summary)} (${escapeHtml(item.strength)})</span></div>`);
155:   renderList("agents", detail.agent_threads ?? [], (agent) => `<div class="row"><b>${escapeHtml(agent.role)}</b><span>${escapeHtml(agent.thread_id ?? "no-thread")} ${escapeHtml(agent.output_mode ?? "")}</span></div>`);
156:   renderList("hypothesis-board", detail.hypotheses ?? [], (item) => `<div class="row"><b>${escapeHtml(item.status ?? item.id)}</b><span>${escapeHtml(item.text ?? item.summary ?? item.id)}</span></div>`);
157:   renderList("experiment-board", detail.experiments ?? [], (item) => `<div class="row"><b>${escapeHtml(item.status ?? item.id)}</b><span>${escapeHtml(item.text ?? item.summary ?? item.id)}</span></div>`);
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
- edge_id: `edge-key:-9157717653402140392`
- head: `repo://e/98f5fcd5a722b064fcf27900ee0546ca` (`entryStatus`)
- relation: `READS`
- tail: `repo://e/fcd9cfde15125dbd241abde97ece6f97` (`source`)
- source_span: `packages/core/src/release/index.mjs:97-97`
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
95:     optional,
96:     source,
97:     present: await pathExists(source)
98:   };
99: }
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
- edge_id: `edge-key:-9156798400716170403`
- head: `repo://e/1492c40ff5bb388717fa41cefe9520e9` (`run`)
- relation: `READS`
- tail: `repo://e/bc8598c4d09ba4a82f13d41a44edf36e` (`foundation`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:462-462`
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
460:       await writeText(path.join(runRoot, "codex-events.jsonl"), rawEvents.map((event) => JSON.stringify(event)).join("\n") + (rawEvents.length ? "\n" : ""));
461:       mapped = rawEvents.map((event, index) => {
462:         const mappedEvent = mapCodexEvent(event, foundation.runId, index + 1000);
463:         mappedEvent.payload = { raw: event };
464:         return mappedEvent;
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
- edge_id: `edge-key:-9143679266306456732`
- head: `repo://e/466845c5e7febbb213e0844ee060badc` (`loadRunDetail`)
- relation: `READS`
- tail: `repo://e/9bd746f1e78cdfbb282ca296f373818d` (`maybeReadJsonl`)
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
- edge_id: `edge-key:-9134920894923319830`
- head: `repo://e/466845c5e7febbb213e0844ee060badc` (`loadRunDetail`)
- relation: `READS`
- tail: `repo://e/d7aab6af01c333ed29a0e30f14288cb7` (`maybeReadJson`)
- source_span: `packages/cli/src/dashboard-server.mjs:275-275`
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
273:   const claims = await maybeReadJson(path.join(runRoot, "claims.json"), []);
274:   const evidence = await maybeReadJson(path.join(runRoot, "evidence.json"), []);
275:   const verifier = await maybeReadJson(path.join(runRoot, "verifier_report.json"), null);
276:   const appserverEvents = await maybeReadJsonl(path.join(runRoot, "logs", "appserver-events.jsonl"));
277:   const multiAgentPlan = await maybeReadJson(path.join(runRoot, "artifacts", "multi_agent_plan.json"), null);
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
- edge_id: `edge-key:-9129781499102827322`
- head: `repo://e/0cd675c30d845df68e3a77ac5b609428` (`writeUpdateChannelManifest`)
- relation: `READS`
- tail: `repo://e/2a7c5adbf2275797c30b565b68da2199` (`manifest`)
- source_span: `packages/core/src/release/index.mjs:190-190`
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
188:     ]
189:   };
190:   await writeJson(path.join(releaseRoot, "UPDATE_CHANNEL.json"), manifest);
191:   return manifest;
192: }
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
- edge_id: `edge-key:-9128803610675048386`
- head: `repo://e/4be08de4c2658305e644b3b2dd6fd893` (`collectAgentArtifacts`)
- relation: `READS`
- tail: `repo://e/aadddd6c46327b9446c8d02581403262` (`uri`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:153-153`
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
151:           evidenceId,
152:           uri,
153:           summary: `Agent-produced file captured from ${rootName}: ${uri}`,
154:           strength: rootName === "artifacts" ? "strong" : "medium"
155:         });
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
- edge_id: `edge-key:-9124629156882348484`
- head: `repo://e/466845c5e7febbb213e0844ee060badc` (`loadRunDetail`)
- relation: `READS`
- tail: `repo://e/afb51dd2d443b4e809aa587f1503adae` (`objective`)
- source_span: `packages/cli/src/dashboard-server.mjs:295-295`
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
293:     run_id: runId,
294:     objective,
295:     objective_title: objective.title ?? objective.id ?? runId,
296:     manifest,
297:     claims,
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
- edge_id: `edge-key:-9118516597195036050`
- head: `repo://e/94299cf089dadff9c7f593e158039b57` (`runMultiAgentFixture`)
- relation: `READS`
- tail: `repo://e/3e8be65033fd06eda8f5cc4e340f02ab` (`output`)
- source_span: `services/orchestrator/src/multi-agent-runner.mjs:128-128`
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
126:       };
127:       agentOutputs.push(output);
128:       await writeJson(path.join(run.runRoot, "artifacts", "agents", `${agent.agent_id}.json`), output);
129:     }
130:     await session.drain();
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
- edge_id: `edge-key:-9104084511085449194`
- head: `repo://e/3f32f8c07d377a5816462b6303935896` (`findLatestOpenRouterLiveSmoke`)
- relation: `READS`
- tail: `repo://e/2d2004ca67b2167fe958ae147306f601` (`matches`)
- source_span: `packages/core/src/preflight/index.mjs:131-131`
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
129:   }
130:   matches.sort((left, right) => String(right.completed_at).localeCompare(String(left.completed_at)));
131:   return matches[0] ?? null;
132: }
133: 
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
- edge_id: `edge-key:-9102024600024439367`
- head: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb` (`main`)
- relation: `READS`
- tail: `repo://e/e8311f4ebaddb60cc81d76d4b4e153f6` (`repoRoot`)
- source_span: `packages/cli/src/arl.mjs:724-724`
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
722:     ok = true;
723:   } else if (command === "codex" && subcommand === "patch-status") {
724:     ok = await codexPatchStatus(repoRoot, rest);
725:   } else if (command === "dashboard" && subcommand === "dev") {
726:     ok = await dashboardDev(repoRoot, rest);
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
- edge_id: `edge-key:-9101897715944272278`
- head: `repo://e/b5a1a01f886a43c39d13d2e420a1ccd5` (`approvalsRespond`)
- relation: `READS`
- tail: `repo://e/c484f2ef00b28272b6ca19ef596e308c` (`approval`)
- source_span: `packages/cli/src/arl.mjs:359-359`
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
357:   console.log(`approval_id=${approval.approval_id}`);
358:   console.log(`status=${approval.status}`);
359:   console.log(`decided_at=${approval.decided_at}`);
360:   return true;
361: }
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
- edge_id: `edge-key:-9100227826065126017`
- head: `repo://e/ad34b4b3bfc2c20be5b17731d6039a29` (`startDashboardAgentPromptRun`)
- relation: `READS`
- tail: `repo://e/aea6e229217b133647d364380dcdc718` (`activeRun`)
- source_span: `packages/cli/src/dashboard-server.mjs:875-875`
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
873:       cleanup.unref?.();
874:     });
875:   activeRuns.set(started.runId, activeRun);
876:   return {
877:     run_id: started.runId,
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
- edge_id: `edge-key:-9096734638751391079`
- head: `repo://e/928989a1e07141373d91f4324b483195` (`saveObjective`)
- relation: `READS`
- tail: `repo://e/324d4579dca1ecc102177a1d5e139717` (`response`)
- source_span: `packages/ui-autoresearch/src/app.js:310-310`
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
308:       })
309:     });
310:     if (!response.ok) throw await startError(response, "save");
311:     await loadObjectives();
312:     text("start-status", "saved");
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
- edge_id: `edge-key:-9095078040935618984`
- head: `repo://e/4a2278bc920360444b92b49ebca96a76` (`runCommand`)
- relation: `READS`
- tail: `repo://e/f1a14c95acd93daf969119b1bace0a89` (`arg`)
- source_span: `packages/cli/src/arl.mjs:138-138`
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
136:   for (let index = 0; index < args.length; index += 1) {
137:     const arg = args[index];
138:     if (valueFlags.has(arg)) {
139:       index += 1;
140:     } else if (!arg.startsWith("--")) {
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
- edge_id: `edge-key:-9094274594408553820`
- head: `repo://e/7f536bc0546080420a1dbfb8d727fc50` (`summarizeResult`)
- relation: `READS`
- tail: `repo://e/1093ce268359610d2acc9c1e732602ab` (`lines`)
- source_span: `scripts/package-local-release.mjs:84-84`
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
82:     lines.push(`signing=${result.signing.status}`);
83:   }
84:   return lines.join("\n");
85: }
86: 
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
- edge_id: `edge-key:-9086865516347134004`
- head: `repo://e/2ce98b1fd830a81db10215877da41cf3` (`createDashboardObjective`)
- relation: `READS`
- tail: `repo://e/6e0cae65db7fff266695785c3ea93a3c` (`domain`)
- source_span: `packages/cli/src/dashboard-server.mjs:189-189`
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
187: title: ${title}
188: goal: ${goal}
189: domain: ${domain}
190: risk_level: ${riskLevel}
191: created_at: ${new Date().toISOString()}
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
- edge_id: `edge-key:-9076229981111458631`
- head: `repo://e/df6b6705e2f1a4b3d74851dc4dafddf9` (`start`)
- relation: `READS`
- tail: `repo://e/b023ba6e3b4c4dd02ec2e36cf6bca25a` (`text`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:282-282`
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
280:       const text = chunk.toString("utf8");
281:       stdoutLog.write(text);
282:       stdoutBuffer += text;
283:       const lines = stdoutBuffer.split(/\r?\n/);
284:       stdoutBuffer = lines.pop() ?? "";
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
- edge_id: `edge-key:-9070979286311978309`
- head: `repo://e/1fb7a8b1083c02268ea908a2d7201b24` (`parseEnvKeys`)
- relation: `READS`
- tail: `repo://e/d6acaa09b9c6dd3b8de0dea83cc17ea5` (`keys`)
- source_span: `packages/core/src/preflight/index.mjs:28-28`
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
26:     if (key) keys.add(key);
27:   }
28:   return keys;
29: }
30: 
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
- edge_id: `edge-key:-9070767160806778402`
- head: `repo://e/c8b569286234694498b643e0eebd758a` (`releaseVerify`)
- relation: `READS`
- tail: `repo://e/95d0dc4874977ffc81395a1a579ba2fe` (`result`)
- source_span: `packages/cli/src/arl.mjs:611-611`
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
609:   const result = await verifyLocalReleasePackage({ releaseRoot });
610:   console.log(`release_root=${path.relative(repoRoot, releaseRoot)}`);
611:   console.log(`verified=${result.valid}`);
612:   console.log(`checked_files=${result.checked_files}`);
613:   console.log(`signing=${result.signing?.status ?? "unknown"}`);
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
- edge_id: `edge-key:-9066838896206081445`
- head: `repo://e/ae73dd0e2ae05d232ce580675746c51d` (`request`)
- relation: `READS`
- tail: `repo://e/d25975621507a1ab11c8c39092c9aee1` (`payload`)
- source_span: `packages/appserver-client/src/jsonrpc-client.mjs:46-46`
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
44:       this.pending.set(id, { resolve, reject });
45:     });
46:     this.process.stdin.write(`${JSON.stringify(payload)}\n`);
47:     return promise;
48:   }
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
- edge_id: `edge-key:-9061621413218347273`
- head: `repo://e/1b3fb031b810207a3f4f4b0371c36df8` (`createDashboardServer`)
- relation: `READS`
- tail: `repo://e/6e92182da05fdbc6df0653ec3bd60504` (`json`)
- source_span: `packages/cli/src/dashboard-server.mjs:994-994`
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
992:       if (url.pathname === "/api/prompt-registry") {
993:         if (request.method === "POST") {
994:           json(response, 200, { prompt: await registerDashboardPrompt(repoRoot, await readBody(request)) });
995:           return;
996:         }
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
- edge_id: `edge-key:-9058036353032998194`
- head: `repo://e/df6b6705e2f1a4b3d74851dc4dafddf9` (`start`)
- relation: `READS`
- tail: `repo://e/d96a124c6ec57bb76028c8fb07c8685d` (`stdoutBuffer`)
- source_span: `packages/core/src/runtime-adapters/external-codex.mjs:282-282`
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
280:       const text = chunk.toString("utf8");
281:       stdoutLog.write(text);
282:       stdoutBuffer += text;
283:       const lines = stdoutBuffer.split(/\r?\n/);
284:       stdoutBuffer = lines.pop() ?? "";
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
- edge_id: `edge-key:-9049808095499909450`
- head: `repo://e/93c7b7217aa635552041df38091049b7` (`loadSchema`)
- relation: `READS`
- tail: `repo://e/ed62adcc522c5e2b4dabeeee62ebd9ad` (`schemaCache`)
- source_span: `packages/core/src/schemas/validation.mjs:11-11`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b8c070b9f8123f4e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
9:     schemaCache.set(absolute, await readJson(absolute));
10:   }
11:   return schemaCache.get(absolute);
12: }
13: 
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
- edge_id: `edge-key:-9040125607661701042`
- head: `repo://e/1dbe62be52214311598aff62eac2f1fb` (`runtimeInstall`)
- relation: `READS`
- tail: `repo://e/eead757f66917054b86eb51fd2d3370e` (`daemonBinary`)
- source_span: `packages/cli/src/arl.mjs:441-441`
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
439:     binaries: {
440:       arl_codex_exec: path.relative(repoRoot, execBinary).replaceAll("\\", "/"),
441:       arl_codexd: path.relative(repoRoot, daemonBinary).replaceAll("\\", "/")
442:     }
443:   });
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
- edge_id: `edge-key:-9025277761288134228`
- head: `repo://e/9c002c70d834159d418707f8b23f5017` (`checkPreflight`)
- relation: `READS`
- tail: `repo://e/a9dd8d162573d295033808ebbdb615e7` (`OPENROUTER_LIVE_SMOKE`)
- source_span: `packages/core/src/preflight/index.mjs:175-175`
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
173:       required_objective_path: OPENROUTER_LIVE_SMOKE.objective_path,
174:       required_model: OPENROUTER_LIVE_SMOKE.model,
175:       required_endpoint: OPENROUTER_LIVE_SMOKE.endpoint,
176:       approval_command: OPENROUTER_LIVE_SMOKE.command,
177:       helper_command: OPENROUTER_LIVE_SMOKE.helper_command,
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
- edge_id: `edge-key:-9010269592487304905`
- head: `repo://e/411f07880b7d1519dfeef2f8a67f294f` (`summarizePlan`)
- relation: `READS`
- tail: `repo://e/c65aa52e68bf03bc29fb9a5755e2e33e` (`missingRequired`)
- source_span: `scripts/package-local-release.mjs:65-65`
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
63:     `release_root=${path.relative(process.cwd(), plan.release_root) || "."}`,
64:     `required_inputs=${plan.required.length}`,
65:     `missing_required=${missingRequired.length === 0 ? "none" : missingRequired.join(",")}`,
66:     `missing_optional=${missingOptional.length === 0 ? "none" : missingOptional.join(",")}`
67:   ].join("\n");
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
- edge_id: `edge-key:-9007576143688695283`
- head: `repo://e/610321cdeb8caa1cd02ee9e29235ebea` (`parseArray`)
- relation: `READS`
- tail: `repo://e/6c8f3ad8b546fca794d33589a6683e75` (`parseScalar`)
- source_span: `packages/core/src/simple-yaml.mjs:106-106`
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
104:     }
105: 
106:     output.push(parseScalar(rest));
107:   }
108:   return [output, index];
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
- edge_id: `edge-key:-9004093376995080862`
- head: `repo://e/8bbadd512e4b13e0118b15de8c85083e` (`inferDomain`)
- relation: `READS`
- tail: `repo://e/21c97bd3ec61f518c7624d413098beba` (`text`)
- source_span: `services/orchestrator/src/freeform-agent-runner.mjs:23-23`
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
21:   const text = String(prompt ?? "").toLowerCase();
22:   if (/\b(cve|xss|csrf|sqli|security|vulnerability|threat)\b/.test(text)) return "security";
23:   if (/\b(robot|sim|simulation|isaac|openusd|omniverse|unity)\b/.test(text)) return "simulation";
24:   if (/\b(rl|reinforcement|policy|reward|gym)\b/.test(text)) return "rl";
25:   if (/\b(market|price|btc|bitcoin|stock|backtest|trading|portfolio)\b/.test(text)) return "quant";
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
- edge_id: `edge-key:-9001811195544157506`
- head: `repo://e/f0ac14429d544ef05a972c69cb7d794e` (`preflightCommand`)
- relation: `READS`
- tail: `repo://e/906b1a88b19bbc9f348776ca9c19c3b1` (`blocked`)
- source_span: `packages/cli/src/arl.mjs:565-565`
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
563:     !result.release.verified ||
564:     result.openrouter_live_smoke.status !== "ready";
565:   console.log(`overall_status=${blocked ? "blocked" : "ready"}`);
566:   return true;
567: }
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
- edge_id: `edge-key:-8999857716524972589`
- head: `repo://e/d0453aa04521bfd2d514f941eb03d66c` (`runtimePin`)
- relation: `READS`
- tail: `repo://e/50c94d2e31853dc4439d2596a7ba652a` (`paths`)
- source_span: `packages/cli/src/arl.mjs:456-456`
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
454:   const paths = runtimePaths(repoRoot);
455:   let installed = null;
456:   if (await pathExists(paths.installed)) installed = await readJson(paths.installed);
457:   if (installed?.version !== version) {
458:     console.error(`runtime version is not installed: ${version}`);
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
- edge_id: `edge-key:-8998457574914876958`
- head: `repo://e/9d52ce65a1d8f80a72f7e94c1b98be84` (`runsList`)
- relation: `READS`
- tail: `repo://e/300fed9fcf8b50a47be18968401cf79e` (`runsRoot`)
- source_span: `packages/cli/src/arl.mjs:257-257`
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
255: async function runsList(repoRoot) {
256:   const runsRoot = path.join(repoRoot, ".arl", "runs");
257:   if (!(await pathExists(runsRoot))) {
258:     console.log("no runs");
259:     return true;
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
- edge_id: `edge-key:-8995542919378820641`
- head: `repo://e/9dc31946b2b79846be32652b72bd275c` (`createEvidenceCandidate`)
- relation: `READS`
- tail: `repo://e/0945a83ed8cf51ea4bac386dd2b30db0` (`classification`)
- source_span: `packages/core/src/evidence/index.mjs:64-64`
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
62:     type: classification.type,
63:     candidate_kind: classification.kind,
64:     summary: `${classification.kind.replaceAll("_", " ")} candidate from runtime event`,
65:     uri: artifactUri(event, sequence),
66:     hash: event?.hash ?? event?.payload?.hash ?? null,
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
- edge_id: `edge-key:-8993426740409288434`
- head: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb` (`main`)
- relation: `READS`
- tail: `repo://e/e8311f4ebaddb60cc81d76d4b4e153f6` (`repoRoot`)
- source_span: `packages/cli/src/arl.mjs:691-691`
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
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
692:   } else if (command === "runs" && subcommand === "list") {
693:     ok = await runsList(repoRoot);
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
- edge_id: `edge-key:-8989347120969962541`
- head: `repo://e/3d0dc448bebd8592894074a6dd2f4ebc` (`serveStatic`)
- relation: `READS`
- tail: `repo://e/7ee89f94d0078691868d7a4ceb9cd99f` (`uiRoot`)
- source_span: `packages/cli/src/dashboard-server.mjs:926-926`
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
924:   const relative = requestPath === "/" ? "index.html" : decodeURIComponent(requestPath.slice(1));
925:   const target = path.join(uiRoot, relative);
926:   if (!isPathInside(uiRoot, target) || !(await pathExists(target))) {
927:     response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
928:     response.end("not found");
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
- edge_id: `edge-key:-8984367128002858889`
- head: `repo://e/cfdda9ed837e3c34054fd3b7cefacb3a` (`runArlCodexdObjective`)
- relation: `READS`
- tail: `repo://e/e7aac895ee677b33588d3768ed7177c1` (`repoRoot`)
- source_span: `services/orchestrator/src/run-manager.mjs:305-305`
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
303: 
304: export async function runArlCodexdObjective(objectivePath, repoRoot = process.cwd(), { runId = null } = {}) {
305:   const result = await runAppServerObjective(objectivePath, repoRoot, { runId });
306:   const loaded = await loadObjective(objectivePath);
307:   const manifestPath = path.join(result.runRoot, "manifest.json");
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
- edge_id: `edge-key:-8969353019983144639`
- head: `repo://e/3d6093be379f4225eb237c4f38134e53` (`createFreeformObjectiveFile`)
- relation: `READS`
- tail: `repo://e/c04082283bfc4e896502342035a14527` (`objectivePath`)
- source_span: `services/orchestrator/src/freeform-agent-runner.mjs:169-169`
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
167:   return {
168:     objective,
169:     path: objectivePath,
170:     relativePath: path.relative(repoRoot, objectivePath).replaceAll("\\", "/")
171:   };
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
- edge_id: `edge-key:-8962613877296389714`
- head: `repo://e/9c002c70d834159d418707f8b23f5017` (`checkPreflight`)
- relation: `READS`
- tail: `repo://e/fae40f3a3b959662c68f535d0c48bffc` (`hasEnvFileKey`)
- source_span: `packages/core/src/preflight/index.mjs:147-147`
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
145:   const certs = listCurrentUserCodeSigningThumbprints({ run: runCommand ?? spawnSync });
146:   const openRouterKeyConfigured =
147:     Boolean(process.env.OPENROUTER_API_KEY) || (await hasEnvFileKey(root, "OPENROUTER_API_KEY"));
148:   const openRouterLiveSmoke = await findLatestOpenRouterLiveSmoke(root);
149: 
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
- edge_id: `edge-key:-8961016370992942599`
- head: `repo://e/a834d4979bd40b385462a7c8de111366` (`runAppServerObjective`)
- relation: `READS`
- tail: `repo://e/5bf4dcbc5b3c92cf0b0dd5f1f27b7071` (`run`)
- source_span: `services/orchestrator/src/run-manager.mjs:185-185`
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
183:       created_by: "app-server-orchestrator"
184:     });
185:     await writeJson(path.join(run.runRoot, "evidence.json"), evidence);
186:     await writeJson(path.join(run.runRoot, "claims.json"), claims);
187: 
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
- edge_id: `edge-key:-8944533034748786028`
- head: `repo://e/9cfb15d4583a4afa15c15c77bc3bdec0` (`validateDomainPack`)
- relation: `READS`
- tail: `repo://e/366196b224b18d6a0da232d785e8c074` (`result`)
- source_span: `packages/core/src/domain-packs/index.mjs:12-12`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b6c68fc77e18d5d0`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
10:   const pack = await loadDomainPack(repoRoot, domain);
11:   const result = await validateWithSchema(pack, path.join(repoRoot, "schemas", "domain-pack.schema.json"));
12:   return { pack, ...result };
13: }
14: 
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
- edge_id: `edge-key:-8936465998144858739`
- head: `repo://e/94299cf089dadff9c7f593e158039b57` (`runMultiAgentFixture`)
- relation: `READS`
- tail: `repo://e/7f67fcffed2a4305ac153e5a80eb0f93` (`loaded`)
- source_span: `services/orchestrator/src/multi-agent-runner.mjs:80-80`
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
78: 
79:   const agents = createAgentPlan(loaded.data);
80:   enforceAgentBudget(loaded.data, agents.map((agent) => agent.role));
81:   const handoffs = agents.slice(0, -1).map((agent, index) => ({
82:     from: agent.agent_id,
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
- edge_id: `edge-key:-8927918114656783034`
- head: `repo://e/4469d79a5efe2d3aaa9b0f070fdff1ec` (`loadLocalEnv`)
- relation: `READS`
- tail: `repo://e/38f539f6561fe52418304a330d206a4a` (`trimmed`)
- source_span: `packages/cli/src/arl.mjs:84-84`
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
82:       const trimmed = line.trim();
83:       if (!trimmed || trimmed.startsWith("#")) continue;
84:       const separator = trimmed.indexOf("=");
85:       if (separator === -1) continue;
86:       const key = trimmed.slice(0, separator).trim();
```

