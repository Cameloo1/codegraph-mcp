# PathEvidence Sample Audit

Database: `reports/final/artifacts/comprehensive_proof_1778642765060.sqlite`

Mode: `proof`

Limit: `20`

Seed: `13`

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
| `repo_roots` | 5 |
| `count` | 0 |
| `candidate_select` | 0 |
| `path_rows_load` | 0 |
| `path_edges_load` | 0 |
| `endpoint_load` | 16 |
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
- path_id: `path://5fd81eb1d7fc0de5`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/f61aa8517c3f55fe79d1cce3cb5a08ec`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/f61aa8517c3f55fe79d1cce3cb5a08ec via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://12e614d74d68441a6bf49c9f053fd280` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://e1ca38fc3ce09c05646d8bca5f0e956b, edge://ed57da348e325b0dbdcc98f0c69931c3` |

### Source Spans

- `packages/cli/src/arl.mjs:680-680`

```text
678:     ok = true;
679:   } else if (command === "doctor") {
680:     ok = await doctor(repoRoot);
681:   } else if (command === "preflight") {
682:     ok = await preflightCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
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
- path_id: `path://6c7f49e2c0ed6d18`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/ae32e56f090c463cf981593586ee94ba`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/ae32e56f090c463cf981593586ee94ba via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://132c2ae63dd285da5402a27155bdd74c` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://58fef6d741d6b840260e3fe088fd406a, edge://91f80e3a5d33837d985a416e7d52e96f` |

### Source Spans

- `packages/cli/src/arl.mjs:699-699`

```text
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
700:   } else if (command === "approvals" && subcommand === "list") {
701:     ok = await approvalsList(repoRoot, rest);
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
- path_id: `path://92744037395faecf`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/f00348c9c0daae03d60f14403cc63981`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/f00348c9c0daae03d60f14403cc63981 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://14d02c236a2bd159e15f0c2e1ed69c83` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://e4bb1c8112aea4e04f95ff49e22578a6` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
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
- path_id: `path://de1d9d007cc39534`
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
| `edge://162ca41b797c042a9df2292286efd718` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://1283e37350d6f95c912cdc3fef93b64e, edge://4831781750511076804f6183d2bc7830` |

### Source Spans

- `packages/cli/src/arl.mjs:574-574`

```text
572:   const verify = args.includes("--verify");
573:   const signing = {};
574:   const signThumbprint = flagValue(args, "--sign-thumbprint");
575:   const timestampServer = flagValue(args, "--timestamp-server");
576:   if (signThumbprint) signing.certThumbprint = signThumbprint;
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
- path_id: `path://e8190d1fb7fce1a3`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/397a7588f2a6107c61cfc87a0fb4faf6`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/397a7588f2a6107c61cfc87a0fb4faf6 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://18af6dad0319719d16bb5310a5e28b93` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://41ab1f5e982288395d30d91dadae2307, edge://f43b15b376785150e19873ec40313842` |

### Source Spans

- `packages/cli/src/arl.mjs:709-709`

```text
707:     ok = await verifyCommand(repoRoot, subcommand);
708:   } else if (command === "report" && subcommand === "open") {
709:     ok = await reportOpen(repoRoot, rest[0]);
710:   } else if (command === "bundle" && (subcommand === "create" || subcommand === "validate")) {
711:     ok = await bundleCommand(repoRoot, subcommand, rest[0]);
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
- path_id: `path://17228cc4d2de09fa`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/391683de6db88c36c0f2fc2010c50344`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/391683de6db88c36c0f2fc2010c50344 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://1a211b94ec67b00ad0eabe7326a7ff40` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://e3a450a7f215c575cb7c542c4d2d46f3, edge://83bf6c503ba782af94f5e7ab9dac6355` |

### Source Spans

- `packages/cli/src/arl.mjs:717-717`

```text
715:     ok = await runtimeVersions(repoRoot);
716:   } else if (command === "runtime" && subcommand === "install") {
717:     ok = await runtimeInstall(repoRoot, rest[0]);
718:   } else if (command === "runtime" && subcommand === "pin") {
719:     ok = await runtimePin(repoRoot, rest[0]);
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
- path_id: `path://4648b519db8df937`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/daf3ee1523e4cf8fde373e1fa6c2042d`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/daf3ee1523e4cf8fde373e1fa6c2042d via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://1be2f5d896b60ddc89ee4aae3203a7f6` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://15e91fce6bd8bc9fdc56a5a1c2ef656d, edge://25a8ce7bf8d74fd66aaa9001e6e86768` |

### Source Spans

- `packages/cli/src/arl.mjs:713-713`

```text
711:     ok = await bundleCommand(repoRoot, subcommand, rest[0]);
712:   } else if (command === "runtime" && subcommand === "doctor") {
713:     ok = await doctor(repoRoot);
714:   } else if (command === "runtime" && subcommand === "versions") {
715:     ok = await runtimeVersions(repoRoot);
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
- path_id: `path://4b4a13c1ea098150`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/8e00619b31839586e69789fa9ef2a558`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/8e00619b31839586e69789fa9ef2a558 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://1bf31ddde5798a1083096eb345485e8e` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://cd6983b6c761af7f996b1384dbef53b1, edge://42581e3b28dfaa35efe8fb586aadf35b` |

### Source Spans

- `packages/cli/src/arl.mjs:724-724`

```text
722:     ok = true;
723:   } else if (command === "codex" && subcommand === "patch-status") {
724:     ok = await codexPatchStatus(repoRoot, rest);
725:   } else if (command === "dashboard" && subcommand === "dev") {
726:     ok = await dashboardDev(repoRoot, rest);
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
- path_id: `path://c076424954d40862`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/d07fbe86e774ae4ef3f100dcbed24854`
- target: `repo://e/4be8e06a4b2b2eea437c87e7be6a4af8`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/d07fbe86e774ae4ef3f100dcbed24854 reaches repo://e/4be8e06a4b2b2eea437c87e7be6a4af8 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://1f621db5c81407baca70743b641b80bc` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://59406e345ff7ad88f7a3220baadbcef6, edge://4831781750511076804f6183d2bc7830` |

### Source Spans

- `packages/cli/src/arl.mjs:621-621`

```text
619:   const result = await checkForUpdates({
620:     repoRoot,
621:     manifestSource: flagValue(args, "--manifest"),
622:     currentVersion: flagValue(args, "--current-version")
623:   });
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
- path_id: `path://db23a7a8d39572be`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/cf30d9af83a42adcc4cdf65db1cc1612`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/cf30d9af83a42adcc4cdf65db1cc1612 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://1fbbe6b70a97493df46a2d0067e59f73` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://ae18d50a8f4c0efedcf6417d2f0426fc, edge://3a84c3a7f3188e01017480b2b3d04727` |

### Source Spans

- `packages/cli/src/arl.mjs:715-715`

```text
713:     ok = await doctor(repoRoot);
714:   } else if (command === "runtime" && subcommand === "versions") {
715:     ok = await runtimeVersions(repoRoot);
716:   } else if (command === "runtime" && subcommand === "install") {
717:     ok = await runtimeInstall(repoRoot, rest[0]);
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
- path_id: `path://3a5d228c92056d38`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/d3dcfecfaea8426208720297b13c7918`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/d3dcfecfaea8426208720297b13c7918 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://20b37ff5de0c60458ed26436f92b7f63` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://e1ca38fc3ce09c05646d8bca5f0e956b, edge://a7135feea88c4fa068b10a7913a0413e` |

### Source Spans

- `packages/cli/src/arl.mjs:680-680`

```text
678:     ok = true;
679:   } else if (command === "doctor") {
680:     ok = await doctor(repoRoot);
681:   } else if (command === "preflight") {
682:     ok = await preflightCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
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
- path_id: `path://ae1430d2de355ac2`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/8dff277826599effc8318c57c6455fd1`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/8dff277826599effc8318c57c6455fd1 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://21110e90278c242d74b9f4fdd568a127` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://58fef6d741d6b840260e3fe088fd406a, edge://2c41399c08cbfb6a18c901f6f169ce18` |

### Source Spans

- `packages/cli/src/arl.mjs:699-699`

```text
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
700:   } else if (command === "approvals" && subcommand === "list") {
701:     ok = await approvalsList(repoRoot, rest);
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
- path_id: `path://de4888911cef4f4e`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/f0ac14429d544ef05a972c69cb7d794e`
- target: `repo://e/4be8e06a4b2b2eea437c87e7be6a4af8`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/f0ac14429d544ef05a972c69cb7d794e reaches repo://e/4be8e06a4b2b2eea437c87e7be6a4af8 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://22bf2c0da90f2d46fa529be4a2677e58` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://2f28d05aa564e16c74879a9bfce8149a, edge://4831781750511076804f6183d2bc7830` |

### Source Spans

- `packages/cli/src/arl.mjs:533-533`

```text
531: 
532: async function preflightCommand(repoRoot, args) {
533:   const releaseRoot = flagValue(args, "--release-root");
534:   const result = await checkPreflight({ repoRoot, releaseRoot });
535:   if (args.includes("--json")) {
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
- path_id: `path://bf1814aac72e0510`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/2069cb5baec9bc7a79156438dd5de96c`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/2069cb5baec9bc7a79156438dd5de96c via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://22d575c48e04736b7094800b1768d8cd` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://5ee24e98083dfaeb634d02a9949ca13d` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
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
- path_id: `path://9559380196954c8f`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/00c50396d8ac0dd0744565489e86d2b6`
- target: `repo://e/4a41a6a600e6b319a77215de4abbebaf`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/00c50396d8ac0dd0744565489e86d2b6 reaches repo://e/4a41a6a600e6b319a77215de4abbebaf via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://2421a245196ac6e02058919f3cc0b936` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://791d519735221be7464e5ae607c21539, edge://9cb3315e059a4af833111c0546519862` |

### Source Spans

- `packages/cli/src/arl.mjs:645-645`

```text
643:     return false;
644:   }
645:   const { server, url } = await startDashboardServer({ repoRoot, port });
646:   console.log(`dashboard_url=${url}`);
647:   console.log("Press Ctrl+C to stop.");
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
- path_id: `path://3f430432fb2f7969`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/cfba52750f09f33d51597925947e5423`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/cfba52750f09f33d51597925947e5423 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://26606c6bfd5c852ffcc805a71c2c330d` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://9edf97b1443bc3a38230566f26420159, edge://7393fb16b7fc1e6bd682f766ae3e4f69` |

### Source Spans

- `packages/cli/src/arl.mjs:687-687`

```text
685:     ok = true;
686:   } else if (command === "objective" && subcommand === "validate") {
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
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
- path_id: `path://57ca6bfe3cd64f4b`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/72581de24a1b087abda605692c4d7a8c`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/72581de24a1b087abda605692c4d7a8c via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://2736a9e0c6951e14a867b2251a739b82` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://28b144c3bcc4444da2a563d45cdb31b3, edge://5ddf43cb37e53ae5bc6270a8d772b9af` |

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
- path_id: `path://0b839cf85c84d4dd`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/906b1a88b19bbc9f348776ca9c19c3b1`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/906b1a88b19bbc9f348776ca9c19c3b1 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://273c6b997934eed42e83bee0f1df2352` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://5b20da893a06b001decdadd31e345c0f, edge://6a099381a6e693325253b9f1bc0d119c` |

### Source Spans

- `packages/cli/src/arl.mjs:682-682`

```text
680:     ok = await doctor(repoRoot);
681:   } else if (command === "preflight") {
682:     ok = await preflightCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
683:   } else if (command === "init") {
684:     console.log("Autoresearch workspace already initialized by repository scaffold.");
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
- path_id: `path://76f1be3a802213ba`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/36d16ef6047b2ee7e00694d38abfb2fd`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/36d16ef6047b2ee7e00694d38abfb2fd via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://28d736c3bdd7cef185bd96dca6d651c3` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://eb2f4822542c1c1cf718981df29a9492` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
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
- path_id: `path://1d37c3efe7a52730`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/c6926aa7636338526c8f04974a3c09b8`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/c6926aa7636338526c8f04974a3c09b8 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://28da6233516a2c4bd425ec876a23499d` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://2bbbee57293268783f032faa626f46d6` |

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
