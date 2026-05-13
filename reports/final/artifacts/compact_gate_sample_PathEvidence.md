# PathEvidence Sample Audit

Database: `reports/final/artifacts/compact_gate_autoresearch_proof.sqlite`

Limit: `20`

Seed: `20260512`

Stored PathEvidence rows: `4096`

Generated fallback samples: `0`

Allowed manual classifications: true_positive, false_positive, wrong_direction, wrong_target, wrong_span, stale, duplicate, unresolved_mislabeled_exact, test_mock_leaked, derived_missing_provenance, unsure

## Path Sample 1

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://000f6ab07d3d8967`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/34aea3539f6507f9adc0d24e4f73dc17`
- target: `repo://e/d9a939a219ad342db0a69f7912e0a783`
- relation_sequence: `FLOWS_TO`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/34aea3539f6507f9adc0d24e4f73dc17 reaches repo://e/d9a939a219ad342db0a69f7912e0a783 via FLOWS_TO`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-3516829023588926675` | `FLOWS_TO` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/dashboard-server.mjs:985-985`

```text
983:           return;
984:         }
985:         json(response, 200, await readToolForgeSummary(repoRoot));
986:         return;
987:       }
```

## Path Sample 2

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://0028f4ab2dbfacd9`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/529dacaee527121d62572bd710a56adb`
- target: `repo://e/b1c2141ae1d62a7fd32512267eaaee75`
- relation_sequence: `FLOWS_TO`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/529dacaee527121d62572bd710a56adb reaches repo://e/b1c2141ae1d62a7fd32512267eaaee75 via FLOWS_TO`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-3321506668327517003` | `FLOWS_TO` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/dashboard-server.mjs:872-872`

```text
870:       activeRun.done = true;
871:       activeRun.finished_at = new Date().toISOString();
872:       const cleanup = setTimeout(() => activeRuns.delete(started.runId), 300000);
873:       cleanup.unref?.();
874:     });
```

## Path Sample 3

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://00346aaf974a1cf8`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/8edb68c90d435bd743168b0ca6b7c2fd`
- target: `repo://e/bc66b9daebfce575131a12c5e4c90e97`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, repo_commit_missing`
- summary: `repo://e/8edb68c90d435bd743168b0ca6b7c2fd reaches repo://e/bc66b9daebfce575131a12c5e4c90e97 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-5549951008192744714` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production_inferred` | `derived` | `edge://dd828e908207a7ad71297327ccd4e71b, edge://b0f820a9862def1bec09d20106ce272d` |

### Source Spans

- `packages/core/src/runtime-adapters/external-codex.mjs:54-54`

```text
52: 
53:   async prepareRun({ objective, objectiveRaw, repoRoot, runId, prompt = null, promptFactory = null }) {
54:     const foundation = await createDryRunFolder({ objective, objectiveRaw, repoRoot, runId });
55:     const runRoot = foundation.runRoot;
56:     const workspace = path.join(runRoot, "workspace");
```

## Path Sample 4

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://0035618874d1e5d9`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/266cb24c8291651a300c0d8fac2b2f84`
- target: `repo://e/cea0f5e851a688266d6d799164bf4ce0`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, repo_commit_missing`
- summary: `repo://e/266cb24c8291651a300c0d8fac2b2f84 reaches repo://e/cea0f5e851a688266d6d799164bf4ce0 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-2482227605811890096` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production_inferred` | `derived` | `edge://9662fccc3207df2038dd968d0363c062, edge://b88962a51c5d42afed57727def828a51` |

### Source Spans

- `scripts/verify-local-release.mjs:30-30`

```text
28: 
29: async function main() {
30:   const args = parseArgs(process.argv.slice(2));
31:   if (args.help) {
32:     console.log(usage());
```

## Path Sample 5

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://0053439a0d3096c8`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/c7ab045554da1b0af4dde5647a11dd78`
- target: `repo://e/c7955010c264c22ce675d45356315dbe`
- relation_sequence: `FLOWS_TO`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/c7ab045554da1b0af4dde5647a11dd78 reaches repo://e/c7955010c264c22ce675d45356315dbe via FLOWS_TO`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-7191613768243689815` | `FLOWS_TO` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/dashboard-server.mjs:363-365`

```text
361:     writeEvent("arl-event", event);
362:   };
363:   const heartbeat = setInterval(() => {
364:     if (!response.destroyed) response.write(": ping\n\n");
365:   }, 15000);
366:   heartbeat.unref?.();
367: 
```

## Path Sample 6

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://005881f82976a26c`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/0adf36ba28e917588f7acf5126fc6afa`
- target: `repo://e/e486cbb5747a80a2fa2dc45c7deb511c`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, repo_commit_missing`
- summary: `repo://e/0adf36ba28e917588f7acf5126fc6afa reaches repo://e/e486cbb5747a80a2fa2dc45c7deb511c via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-7779000502432671427` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production_inferred` | `derived` | `edge://91a13abba5490a07535790e370d155d1, edge://e6a71783a4e3501ccfc1851ee959d952` |

### Source Spans

- `packages/core/src/run-ledger/finalize.mjs:55-55`

```text
53: 
54:   const hashes = {};
55:   for (const file of await walkFiles(runRoot)) {
56:     const relative = path.relative(runRoot, file).replaceAll("\\", "/");
57:     if (relative === "hashes.json") continue;
```

## Path Sample 7

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://006a84fe4e55ca47`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/81fadbac75286413785474f905f73a65`
- target: `repo://e/5cd8e3f2bc7a5200af5fd28a813a8e12`
- relation_sequence: `WRITES`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/81fadbac75286413785474f905f73a65 reaches repo://e/5cd8e3f2bc7a5200af5fd28a813a8e12 via WRITES`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9063180648632075272` | `WRITES` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/dashboard-server.mjs:774-789`

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

## Path Sample 8

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://006cd3767eea3f10`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/f007743889642d9b4de9ba52223862b1`
- target: `repo://e/1439ffea233b6b5f4e039f28c5f4989d`
- relation_sequence: `FLOWS_TO`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/f007743889642d9b4de9ba52223862b1 reaches repo://e/1439ffea233b6b5f4e039f28c5f4989d via FLOWS_TO`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-7429234402655601086` | `FLOWS_TO` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/dashboard-server.mjs:1052-1052`

```text
1050:       if (url.pathname.match(/^\/api\/runs\/[^/]+\/agent-turn$/) && request.method === "POST") {
1051:         const runId = decodeURIComponent(url.pathname.split("/")[3]);
1052:         json(response, 200, { run: await appendDashboardAgentTurn(repoRoot, runId, await readBody(request)) });
1053:         return;
1054:       }
```

## Path Sample 9

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://007e444ae8d86b3d`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cbfdecc8a82c1898741c5e15535fba66`
- target: `repo://e/daf3ee1523e4cf8fde373e1fa6c2042d`
- relation_sequence: `READS`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/cbfdecc8a82c1898741c5e15535fba66 reaches repo://e/daf3ee1523e4cf8fde373e1fa6c2042d via READS`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-265882038320402033` | `READS` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/arl.mjs:110-110`

```text
108:     ["dry_run_adapter", true, true],
109:     ["arl_codex_exec_binary", Boolean(runtimeExec), true],
110:     ["arl_codexd_binary", Boolean(runtimeDaemon), true],
111:     ["codex_execution_default_disabled", false, false]
112:   ];
```

## Path Sample 10

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://00ccb64b47a90c2d`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/10499e07c63bf12b4f833676523af899`
- target: `repo://e/923c9d59a554335bb316a3f0d3be3729`
- relation_sequence: `FLOWS_TO`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/10499e07c63bf12b4f833676523af899 reaches repo://e/923c9d59a554335bb316a3f0d3be3729 via FLOWS_TO`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-5842922007579210012` | `FLOWS_TO` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/arl.mjs:461-465`

```text
459:     return false;
460:   }
461:   await writeJson(paths.pinned, {
462:     version,
463:     pinned_at: new Date().toISOString(),
464:     source_manifest: path.relative(repoRoot, paths.installed).replaceAll("\\", "/")
465:   });
466:   console.log(`pinned_runtime=${version}`);
467:   console.log(`manifest=${path.relative(repoRoot, paths.pinned)}`);
```

## Path Sample 11

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://00d5eff06985f9c9`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/c374e064e7b93b4b278b28c621f4eae5`
- target: `repo://e/4a41a6a600e6b319a77215de4abbebaf`
- relation_sequence: `READS`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/c374e064e7b93b4b278b28c621f4eae5 reaches repo://e/4a41a6a600e6b319a77215de4abbebaf via READS`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-8762122557730659073` | `READS` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/dashboard-server.mjs:1074-1074`

```text
1072:   });
1073:   const address = server.address();
1074:   const actualPort = typeof address === "object" && address ? address.port : port;
1075:   return {
1076:     server,
```

## Path Sample 12

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://01018e17e1832992`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/96a8118fec38e93059bcdf6019955f1e`
- target: `repo://e/1355a75ed605f2d59e544a6928ef43b7`
- relation_sequence: `READS`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/96a8118fec38e93059bcdf6019955f1e reaches repo://e/1355a75ed605f2d59e544a6928ef43b7 via READS`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-5798325092714862386` | `READS` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/dashboard-server.mjs:759-759`

```text
757:     throw new Error(`unsupported dashboard runtime: ${runtime}`);
758:   }
759:   return loadRunDetail(repoRoot, result.runId);
760: }
761: 
```

## Path Sample 13

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://0124f1d62b2ecb85`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/1b3fb031b810207a3f4f4b0371c36df8`
- target: `repo://e/d943441e72a2d241d9997df2ce11b273`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, repo_commit_missing`
- summary: `repo://e/1b3fb031b810207a3f4f4b0371c36df8 reaches repo://e/d943441e72a2d241d9997df2ce11b273 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-6473008800543585111` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production_inferred` | `derived` | `edge://21d3ff4230959ef956121e3138f7da8f, edge://9c5622d9fb332d1b6a30339501fcdb25` |

### Source Spans

- `packages/cli/src/dashboard-server.mjs:969-969`

```text
967:       }
968:       if (url.pathname === "/api/runtime") {
969:         json(response, 200, { runtime: await readRuntimeStatus(repoRoot) });
970:         return;
971:       }
```

## Path Sample 14

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://0125c79a394917a6`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/aafd7770c65d6a3b976c57748f027d2d`
- target: `repo://e/94289fdce685c8c13821b17dc218485b`
- relation_sequence: `CALLS`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, repo_commit_missing`
- summary: `repo://e/aafd7770c65d6a3b976c57748f027d2d reaches repo://e/94289fdce685c8c13821b17dc218485b via CALLS`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-8755588125988282955` | `CALLS` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/arl.mjs:485-485`

```text
483:   console.log(`arl-codex-exec: ${execBinary ? path.relative(repoRoot, execBinary).replaceAll("\\", "/") : "missing"}`);
484:   console.log(`arl-codexd: ${daemonBinary ? path.relative(repoRoot, daemonBinary).replaceAll("\\", "/") : "missing"}`);
485:   if (await pathExists(paths.installed)) {
486:     const installed = await readJson(paths.installed);
487:     console.log(`installed_runtime: ${installed.version}`);
```

## Path Sample 15

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://0126715ccf36b8e2`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/361f02fa936d8a6dbf41037d332a191f`
- target: `repo://e/c9ff720d3292e2a438f7d17da9fd34be`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, repo_commit_missing`
- summary: `repo://e/361f02fa936d8a6dbf41037d332a191f reaches repo://e/c9ff720d3292e2a438f7d17da9fd34be via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-8436003815957229160` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production_inferred` | `derived` | `edge://67f1e1a4437c7d4864fd155c43250052, edge://c06d9bca36c1b0f370c2b6684cdc4511` |

### Source Spans

- `packages/core/src/run-ledger/index.mjs:157-157`

```text
155:   await writeText(path.join(runRoot, "report.md"), report);
156: 
157:   const graph = buildResearchGraph({ objective, manifest, hypotheses, experimentPlan, claims, evidence });
158:   await writeJson(path.join(runRoot, "research_graph.json"), graph);
159:   await writeJson(path.join(runRoot, "environment.json"), manifest.system);
```

## Path Sample 16

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://012798a9e11020ec`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/ad34b4b3bfc2c20be5b17731d6039a29`
- target: `repo://e/2514afbc2cf141dfd3bc3ea69584f9b5`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, repo_commit_missing`
- summary: `repo://e/ad34b4b3bfc2c20be5b17731d6039a29 reaches repo://e/2514afbc2cf141dfd3bc3ea69584f9b5 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-935348241303641440` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production_inferred` | `derived` | `edge://5c6a7e8a348e2ea31c2806f4cd757629, edge://aefec4388e0be4c795f6857b098afad5` |

### Source Spans

- `packages/cli/src/dashboard-server.mjs:861-861`

```text
859:     .then(async (result) => {
860:       activeRun.status = result?.cancelled ? "cancelled" : result?.exitCode === 0 ? "completed" : "failed";
861:       activeRun.detail = await loadRunDetail(repoRoot, started.runId);
862:       return activeRun.detail;
863:     })
```

## Path Sample 17

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://012c2f0859d17515`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/2cfd393d6cd7a66f8d08061fb8b27e49`
- target: `repo://e/4445deeb8c986e4cc85277d92de614de`
- relation_sequence: `FLOWS_TO`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/2cfd393d6cd7a66f8d08061fb8b27e49 reaches repo://e/4445deeb8c986e4cc85277d92de614de via FLOWS_TO`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-2170153134222382779` | `FLOWS_TO` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/arl.mjs:697-697`

```text
695:     ok = await runsShow(repoRoot, rest[0]);
696:   } else if (command === "runs" && subcommand === "replay") {
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
```

## Path Sample 18

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://0132bb73a88c2e3c`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/2993c700973004d60a10ea15d1379e18`
- relation_sequence: `READS`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/2993c700973004d60a10ea15d1379e18 via READS`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-2079480494063939985` | `READS` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/arl.mjs:705-705`

```text
703:     ok = await approvalsRespond(repoRoot, rest[0], rest[1], rest.slice(2).join(" ") || undefined);
704:   } else if (command === "evidence" && subcommand === "check") {
705:     ok = await evidenceCheck(repoRoot, rest[0]);
706:   } else if (command === "verify") {
707:     ok = await verifyCommand(repoRoot, subcommand);
```

## Path Sample 19

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://017fe3d30e2f9310`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/e1b0cb5fcfe5cbcf5b8ff2d44f714099`
- target: `repo://e/8d5f50d2916c4648ee81847cae3493e6`
- relation_sequence: `WRITES`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/e1b0cb5fcfe5cbcf5b8ff2d44f714099 reaches repo://e/8d5f50d2916c4648ee81847cae3493e6 via WRITES`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-2570844410810623834` | `WRITES` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/dashboard-server.mjs:503-503`

```text
501: async function readRegistryJsonFiles(root) {
502:   if (!(await pathExists(root))) return [];
503:   const files = await walkFiles(root);
504:   const records = [];
505:   for (const file of files.filter((item) => item.endsWith(".json"))) {
```

## Path Sample 20

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://0186d7e06b326e25`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/9bd746f1e78cdfbb282ca296f373818d`
- target: `repo://e/fcab29ef51fea4cd1c35254208d62347`
- relation_sequence: `WRITES`
- exactness: `parser_verified`
- confidence: `0.9523809523809523`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `file_hash_missing, metadata_empty, repo_commit_missing`
- summary: `repo://e/9bd746f1e78cdfbb282ca296f373818d reaches repo://e/fcab29ef51fea4cd1c35254208d62347 via WRITES`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-3547010053491208052` | `WRITES` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `packages/cli/src/dashboard-server.mjs:65-65`

```text
63: async function maybeReadJsonl(filePath) {
64:   if (!(await pathExists(filePath))) return [];
65:   const text = await readText(filePath);
66:   return text
67:     .split(/\r?\n/)
```


## Notes

- Classification fields are intentionally blank in markdown for human review.
- PathEvidence rows are sampled from path_evidence when present; when the table is empty, this audit generates one-edge PathEvidence-shaped samples from existing edges without writing them to storage.
- Stored PathEvidence samples used before fallback: 20.
