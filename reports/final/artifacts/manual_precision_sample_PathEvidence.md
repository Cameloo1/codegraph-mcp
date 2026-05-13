# PathEvidence Sample Audit

Database: `reports/final/artifacts/comprehensive_proof_1778642765060.sqlite`

Mode: `proof`

Limit: `20`

Seed: `41`

Max edge load: `500`

Timeout ms: `2000`

Stored PathEvidence rows: `4096`

Candidate path IDs: `20`

Loaded materialized path edges: `20`

Edge load truncated: `false`

Generated fallback samples: `0`

## Sampler Timing

| Stage | ms |
| --- | ---: |
| `total` | 27 |
| `open_db` | 0 |
| `repo_roots` | 1 |
| `count` | 0 |
| `candidate_select` | 0 |
| `path_rows_load` | 0 |
| `path_edges_load` | 0 |
| `endpoint_load` | 18 |
| `snippet_load` | 0 |
| `sample_build` | 0 |
| `explain` | 0 |
| `index_check` | 0 |

## Index Status

| Object | Required shape | Present | Satisfied by | Columns |
| --- | --- | --- | --- | --- |
| `path_evidence` | `path_evidence(id primary key; logical path_id)` | `true` | `sqlite_autoindex_path_evidence_1` | `id` |
| `path_evidence_edges` | `path_evidence_edges(path_id, ordinal)` | `true` | `idx_path_evidence_edges_path_ordinal` | `path_id, ordinal` |
| `path_evidence_symbols` | `path_evidence_symbols(path_id, entity_id)` | `true` | `idx_path_evidence_symbols_path` | `path_id, entity_id` |
| `path_evidence_files` | `path_evidence_files(file_id, path_id)` | `true` | `idx_path_evidence_files_file` | `file_id, path_id` |
| `path_evidence_tests` | `path_evidence_tests(path_id, test_id)` | `true` | `sqlite_autoindex_path_evidence_tests_1` | `path_id, test_id, relation` |

## Query Plans

### `candidate_path_ids_rowid`

```sql
SELECT id FROM path_evidence WHERE rowid >= 1 ORDER BY rowid LIMIT 20
```

- `SEARCH path_evidence USING INTEGER PRIMARY KEY (rowid>?)`

### `endpoint_batch_lookup`

```sql
SELECT lower(hex(e.entity_hash)) AS entity_hash_hex, name.value AS name, kind.value AS kind FROM entities e LEFT JOIN symbol_dict name ON name.id = e.name_id LEFT JOIN entity_kind_dict kind ON kind.id = e.kind_id WHERE e.entity_hash IN (X'00000000000000000000000000000000')
```

- `SCAN e`
- `SEARCH name USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN`
- `SEARCH kind USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN`
- full_scans: `SCAN e`

### `path_edges_by_path_id`

```sql
SELECT path_id, ordinal, edge_id, head_id, relation, tail_id, source_span_path, exactness, confidence, derived, edge_class, context, provenance_edges_json FROM path_evidence_edges WHERE path_id IN ('path://sample') ORDER BY path_id, ordinal LIMIT 500
```

- `SEARCH path_evidence_edges USING PRIMARY KEY (path_id=?)`

### `path_rows_by_id`

```sql
SELECT id, source, target, summary, metapath_json, edges_json, source_spans_json, exactness, length, confidence, metadata_json FROM path_evidence WHERE id IN ('path://sample')
```

- `SEARCH path_evidence USING INDEX sqlite_autoindex_path_evidence_1 (id=?)`

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
- path_id: `path://16fe3d901d2767ea`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/aee2f7bdea85439bb2c8dec26ec8fbb9`
- target: `repo://e/4be8e06a4b2b2eea437c87e7be6a4af8`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/aee2f7bdea85439bb2c8dec26ec8fbb9 reaches repo://e/4be8e06a4b2b2eea437c87e7be6a4af8 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://3852aafde4101009ae2ff989442f671b` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://29e85f0d82c55647a21952d43645a115, edge://4831781750511076804f6183d2bc7830` |

### Source Spans

- `packages/cli/src/arl.mjs:575-575`

```text
573:   const signing = {};
574:   const signThumbprint = flagValue(args, "--sign-thumbprint");
575:   const timestampServer = flagValue(args, "--timestamp-server");
576:   if (signThumbprint) signing.certThumbprint = signThumbprint;
577:   if (timestampServer) signing.timestampServer = timestampServer;
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
- path_id: `path://6a6110e78b5b5221`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/3c3f03e28ded381e7924e9685b228434`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/3c3f03e28ded381e7924e9685b228434 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://3a9caf057dfefd0a8b176acb9e47b22c` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://27444d9488fb6fbeecd5fe42826e9990` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
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
- path_id: `path://0a5cc4db3c8ecdf3`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/5106f3def9df025a88b7ae63496a0bd0`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/5106f3def9df025a88b7ae63496a0bd0 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://3c2f777c0339b2e3ccda602cbf1f1ad1` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://47bd9ebe74e692c74034b1d5aa54f511, edge://e35fa8f9a6621ec88f2d4c4f67d4237a` |

### Source Spans

- `packages/cli/src/arl.mjs:705-705`

```text
703:     ok = await approvalsRespond(repoRoot, rest[0], rest[1], rest.slice(2).join(" ") || undefined);
704:   } else if (command === "evidence" && subcommand === "check") {
705:     ok = await evidenceCheck(repoRoot, rest[0]);
706:   } else if (command === "verify") {
707:     ok = await verifyCommand(repoRoot, subcommand);
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
- path_id: `path://14e3df0913748040`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/00c50396d8ac0dd0744565489e86d2b6`
- target: `repo://e/a0c59aa0232a9bbf4fa47a58f89ab445`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/00c50396d8ac0dd0744565489e86d2b6 reaches repo://e/a0c59aa0232a9bbf4fa47a58f89ab445 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://3c47fe08900bbafd8fff2b592e75747f` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://791d519735221be7464e5ae607c21539, edge://227d3a715f423e25377a178c258972ab` |

### Source Spans

- `packages/cli/src/arl.mjs:645-645`

```text
643:     return false;
644:   }
645:   const { server, url } = await startDashboardServer({ repoRoot, port });
646:   console.log(`dashboard_url=${url}`);
647:   console.log("Press Ctrl+C to stop.");
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
- path_id: `path://5e358951773480f7`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/cf6d3a74ae3ccb7a186e82742f58423c`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/cf6d3a74ae3ccb7a186e82742f58423c via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://401471e536a1695f370808b55cd5bfd1` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://8b5d6ae5630cfc50746c278cd3a8029a, edge://850c93cc1480d69943b3468deed3498f` |

### Source Spans

- `packages/cli/src/arl.mjs:728-728`

```text
726:     ok = await dashboardDev(repoRoot, rest);
727:   } else if (command === "release" && subcommand === "package") {
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
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
- path_id: `path://ac209085fc2be091`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/4ce5de5f06fadc7076af382cffbd9c1a`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/4ce5de5f06fadc7076af382cffbd9c1a via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://41479c0f5cca52e6480829be189bed50` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://8b5d6ae5630cfc50746c278cd3a8029a, edge://5a375412a2fbd354e35e3143b96b1ed6` |

### Source Spans

- `packages/cli/src/arl.mjs:728-728`

```text
726:     ok = await dashboardDev(repoRoot, rest);
727:   } else if (command === "release" && subcommand === "package") {
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
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
- path_id: `path://46ce5e87a3d7b509`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/4d4f9384f2052a479488e4a75f26b61d`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/4d4f9384f2052a479488e4a75f26b61d via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://450698e8d667b6e11fd1db5ac68acb93` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://3ae9618cdf80ff23d32cbe53f4f79ecd, edge://a6a2a4c6839207d5cf0a4ce93d24aadb` |

### Source Spans

- `packages/cli/src/arl.mjs:711-711`

```text
709:     ok = await reportOpen(repoRoot, rest[0]);
710:   } else if (command === "bundle" && (subcommand === "create" || subcommand === "validate")) {
711:     ok = await bundleCommand(repoRoot, subcommand, rest[0]);
712:   } else if (command === "runtime" && subcommand === "doctor") {
713:     ok = await doctor(repoRoot);
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
- path_id: `path://73e648035c67d03b`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/f1db9619966cc56a3bf8d596f58b62e4`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/f1db9619966cc56a3bf8d596f58b62e4 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://450d076dd7dd77556fb7a156dd79a4bb` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://28b144c3bcc4444da2a563d45cdb31b3, edge://3f7cabc46c199a4e6b98caaee57c5e78` |

### Source Spans

- `packages/cli/src/arl.mjs:697-697`

```text
695:     ok = await runsShow(repoRoot, rest[0]);
696:   } else if (command === "runs" && subcommand === "replay") {
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
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
- path_id: `path://2a53d300dd4de481`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/698af9d04477604c65eb06db6e0693de`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/698af9d04477604c65eb06db6e0693de via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://466473bff63bcfece7c0a7410ed7e306` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://b4328cb54f789e4f0e1da19f5b60dd0d, edge://fc59dfa077f0363f88aac093e2c7375d` |

### Source Spans

- `packages/cli/src/arl.mjs:707-707`

```text
705:     ok = await evidenceCheck(repoRoot, rest[0]);
706:   } else if (command === "verify") {
707:     ok = await verifyCommand(repoRoot, subcommand);
708:   } else if (command === "report" && subcommand === "open") {
709:     ok = await reportOpen(repoRoot, rest[0]);
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
- path_id: `path://e39d2cdfc44468d2`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/2371a114927d0037dea1cb9cbf3dee01`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/2371a114927d0037dea1cb9cbf3dee01 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://466c39c995bcd7ad7bfcdc6014872833` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://6bfecd1102ca38fb7347a916a2a4ede1` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
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
- path_id: `path://37d9d1b1c9ab3940`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/67a40c39abf3dd5e22ec5f6017776768`
- target: `repo://e/1cc0c088760556a02425ce81601dad5a`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/67a40c39abf3dd5e22ec5f6017776768 reaches repo://e/1cc0c088760556a02425ce81601dad5a via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://47698f8153549abda83970204b0bcb73` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://2886ceb37c0293cceaf32d773847bf86, edge://fd0b1f0ac1106e93ca7b415baf80fde1` |

### Source Spans

- `packages/cli/src/arl.mjs:660-660`

```text
658:     return false;
659:   }
660:   const result = await runFreeformAgentPrompt(prompt, repoRoot);
661:   console.log(`objective=${result.objectivePath}`);
662:   console.log(`run_id=${result.runId}`);
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
- path_id: `path://f9966baea768ab8d`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/00c50396d8ac0dd0744565489e86d2b6`
- target: `repo://e/d9ee3babe49a6c6cbe3ba47a55ced37e`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/00c50396d8ac0dd0744565489e86d2b6 reaches repo://e/d9ee3babe49a6c6cbe3ba47a55ced37e via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://4d167a3f04a556af55aba584338578d5` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://791d519735221be7464e5ae607c21539, edge://181cd8fcdadac1ee412508fd3d3d8464` |

### Source Spans

- `packages/cli/src/arl.mjs:645-645`

```text
643:     return false;
644:   }
645:   const { server, url } = await startDashboardServer({ repoRoot, port });
646:   console.log(`dashboard_url=${url}`);
647:   console.log("Press Ctrl+C to stop.");
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
- path_id: `path://833b30ec1b5129d0`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/2f0470a440cfdaf323f77407555ea34d`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/2f0470a440cfdaf323f77407555ea34d via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://4dcccdb645143181430ad72bcf0a2d3b` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://3499bdf5d37f1433bd8cc0e1b69f0075, edge://f60e12fe53afd64b283380f824bd530d` |

### Source Spans

- `packages/cli/src/arl.mjs:673-673`

```text
671: async function main() {
672:   const repoRoot = process.cwd();
673:   await loadLocalEnv(repoRoot);
674:   const [command, subcommand, ...rest] = process.argv.slice(2);
675:   let ok = false;
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
- path_id: `path://6ad86dc34719852d`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/fc0b3c9c010eed65b78ed163602a0947`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/fc0b3c9c010eed65b78ed163602a0947 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://4eaf66c2bc9ba6844bd9d53eec201d16` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://8b5d6ae5630cfc50746c278cd3a8029a, edge://e999ad1ea8291a1aeef9d61e399654c0` |

### Source Spans

- `packages/cli/src/arl.mjs:728-728`

```text
726:     ok = await dashboardDev(repoRoot, rest);
727:   } else if (command === "release" && subcommand === "package") {
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
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
- path_id: `path://b7f7ae0fe0f196b8`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/10345a6437c5103464d574680a451b32`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/10345a6437c5103464d574680a451b32 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://4f34f22967acb41a11ea89ecc85f7c8c` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://8b5d6ae5630cfc50746c278cd3a8029a, edge://efd7e9db494b333ee750f0db09048024` |

### Source Spans

- `packages/cli/src/arl.mjs:728-728`

```text
726:     ok = await dashboardDev(repoRoot, rest);
727:   } else if (command === "release" && subcommand === "package") {
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
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
- path_id: `path://75cfc9e03af93e95`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/8260716e5b3bb3fb083a548e43b47dad`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/8260716e5b3bb3fb083a548e43b47dad via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://4facfacd2bff1206fb64a953dec2c3ac` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://d8c16ae0bd8b4900e1984d42489626ae` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
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
- path_id: `path://4db98dca38c6ef58`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/06d794d3c8bbd205c4422908294a529b`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/06d794d3c8bbd205c4422908294a529b via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://4fcc2779ce588bb41a30926cf91ca9ce` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://28b144c3bcc4444da2a563d45cdb31b3, edge://381f5296e7819c1c5dcc01a32b45a31e` |

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
- path_id: `path://3289154380c9d55c`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/cfb5f31d4f23272c80389e5316a1b00a`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/cfb5f31d4f23272c80389e5316a1b00a via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://501f2528d109cebfe6acde60fc171101` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://7ac75e0c97f4a1168d66a1dcf8433cfc` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
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
- path_id: `path://1d91ba79d190cf3a`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/e9841a0ca3cdced2ca584f5fad732e70`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/e9841a0ca3cdced2ca584f5fad732e70 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://51b73c293cd2457059dd032475106dca` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://15cb00843d9d7f99bfaabf9c79078d5f` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
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
- path_id: `path://40b41d54fbcb4026`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/04688a55c5f6b8dc2dc68777bb4b210e`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/04688a55c5f6b8dc2dc68777bb4b210e via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://520b3c0032637271e5fec01de4c851fb` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://cca9bfce2ab165427c87bf67f19bf3c4` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
```


## Notes

- Classification fields are intentionally blank in markdown for human review.
- PathEvidence sampling is bounded: candidate path IDs are selected first, details are batch-loaded only for those IDs, and snippets are loaded only for sampled spans when requested.
- Mode `proof` disables generated fallback paths.
- Path edge materialization load cap: 500 rows; truncated: false.
- Stored PathEvidence samples used before fallback: 20.
