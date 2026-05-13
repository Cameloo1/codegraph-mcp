# PathEvidence Sample Audit

Database: `reports/final/artifacts/comprehensive_proof_1778642765060.sqlite`

Mode: `proof`

Limit: `100`

Seed: `13`

Max edge load: `2000`

Timeout ms: `10000`

Stored PathEvidence rows: `4096`

Candidate path IDs: `100`

Loaded materialized path edges: `100`

Edge load truncated: `false`

Generated fallback samples: `0`

## Sampler Timing

| Stage | ms |
| --- | ---: |
| `total` | 35 |
| `open_db` | 0 |
| `repo_roots` | 1 |
| `count` | 0 |
| `candidate_select` | 0 |
| `path_rows_load` | 0 |
| `path_edges_load` | 1 |
| `endpoint_load` | 19 |
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
SELECT id FROM path_evidence WHERE rowid >= 1 ORDER BY rowid LIMIT 100
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
SELECT path_id, ordinal, edge_id, head_id, relation, tail_id, source_span_path, exactness, confidence, derived, edge_class, context, provenance_edges_json FROM path_evidence_edges WHERE path_id IN ('path://sample') ORDER BY path_id, ordinal LIMIT 2000
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

## Path Sample 21

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://64904ddd77270317`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/eead757f66917054b86eb51fd2d3370e`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/eead757f66917054b86eb51fd2d3370e via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://2df91850c91b83849ecd6f9b67983a22` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://e3a450a7f215c575cb7c542c4d2d46f3, edge://3b380d49a753a257caf7be2a7e657959` |

### Source Spans

- `packages/cli/src/arl.mjs:717-717`

```text
715:     ok = await runtimeVersions(repoRoot);
716:   } else if (command === "runtime" && subcommand === "install") {
717:     ok = await runtimeInstall(repoRoot, rest[0]);
718:   } else if (command === "runtime" && subcommand === "pin") {
719:     ok = await runtimePin(repoRoot, rest[0]);
```

## Path Sample 22

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://aa2155dac113f41a`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/ea811ab0b12b43e6c41738adfb8ccc58`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/ea811ab0b12b43e6c41738adfb8ccc58 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://2e896f64376b606c6c37d0d8236bd7ce` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://e147c2799d6caecba04ed0ffc9aedf7d` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
```

## Path Sample 23

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://5189f164fc8bb30e`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/95d0dc4874977ffc81395a1a579ba2fe`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/95d0dc4874977ffc81395a1a579ba2fe via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://2fb104fc1aac460ad37e22d690a03a38` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://88315076e517d6e493f092c68e708fb6, edge://61e2401b2157b384179c0eea8feb2482` |

### Source Spans

- `packages/cli/src/arl.mjs:730-730`

```text
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
731:   } else if (command === "release" && subcommand === "update-check") {
732:     ok = await releaseUpdateCheck(repoRoot, rest);
```

## Path Sample 24

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://af9babcbcee28e59`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/9ccac779ba95bd74a0991824cbd2d2aa`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/9ccac779ba95bd74a0991824cbd2d2aa via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://327fda135a206c0a91e5db53361ddd88` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://a8d56048ffca0189d51b1776c7fab97b` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
```

## Path Sample 25

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://d65ba951d9e1ef90`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/24dd3eda6acbcf4ce9cd5df39a8f677a`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/24dd3eda6acbcf4ce9cd5df39a8f677a via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://33405e9c6f0af5fb742181f663ccb349` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://8b5d6ae5630cfc50746c278cd3a8029a, edge://2688b6f47d7e41cdea72d8af0aaa1fa7` |

### Source Spans

- `packages/cli/src/arl.mjs:728-728`

```text
726:     ok = await dashboardDev(repoRoot, rest);
727:   } else if (command === "release" && subcommand === "package") {
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
```

## Path Sample 26

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://d5346e2470be3d11`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/50c94d2e31853dc4439d2596a7ba652a`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/50c94d2e31853dc4439d2596a7ba652a via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://351cbd4162481d619676d57c01220abf` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://d70a13f5febc14bf40dce35a33fd8be1, edge://29b6ed23e5492f1cec6363d78b86511e` |

### Source Spans

- `packages/cli/src/arl.mjs:719-719`

```text
717:     ok = await runtimeInstall(repoRoot, rest[0]);
718:   } else if (command === "runtime" && subcommand === "pin") {
719:     ok = await runtimePin(repoRoot, rest[0]);
720:   } else if (command === "codex" && subcommand === "upstream-status") {
721:     console.log("upstream/openai-codex pinned at 163eac9306e86b38c5ab3986eefd5fd3be616b06");
```

## Path Sample 27

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://606dcaa103f02268`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/c484f2ef00b28272b6ca19ef596e308c`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/c484f2ef00b28272b6ca19ef596e308c via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://36eb4c3cbcd8ad8a51ace40ca63bf4b8` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://acfcbd30636acecaebe40a6072293f84, edge://9ad6f1c2adb43e7c759ad7e2140ec032` |

### Source Spans

- `packages/cli/src/arl.mjs:703-703`

```text
701:     ok = await approvalsList(repoRoot, rest);
702:   } else if (command === "approvals" && subcommand === "respond") {
703:     ok = await approvalsRespond(repoRoot, rest[0], rest[1], rest.slice(2).join(" ") || undefined);
704:   } else if (command === "evidence" && subcommand === "check") {
705:     ok = await evidenceCheck(repoRoot, rest[0]);
```

## Path Sample 28

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://720eb0b83cbf58ef`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/67a40c39abf3dd5e22ec5f6017776768`
- target: `repo://e/7a055ffd9551c5ab0f2227d3d3bf21e1`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/67a40c39abf3dd5e22ec5f6017776768 reaches repo://e/7a055ffd9551c5ab0f2227d3d3bf21e1 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://380166d0dd35842242f971bdba585e1c` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://2886ceb37c0293cceaf32d773847bf86, edge://ab32ff269e3d6e19247c95620acbea27` |

### Source Spans

- `packages/cli/src/arl.mjs:660-660`

```text
658:     return false;
659:   }
660:   const result = await runFreeformAgentPrompt(prompt, repoRoot);
661:   console.log(`objective=${result.objectivePath}`);
662:   console.log(`run_id=${result.runId}`);
```

## Path Sample 29

- true_positive:
- false_positive:
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

## Path Sample 30

- true_positive:
- false_positive:
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

## Path Sample 31

- true_positive:
- false_positive:
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

## Path Sample 32

- true_positive:
- false_positive:
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

## Path Sample 33

- true_positive:
- false_positive:
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

## Path Sample 34

- true_positive:
- false_positive:
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

## Path Sample 35

- true_positive:
- false_positive:
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

## Path Sample 36

- true_positive:
- false_positive:
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

## Path Sample 37

- true_positive:
- false_positive:
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

## Path Sample 38

- true_positive:
- false_positive:
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

## Path Sample 39

- true_positive:
- false_positive:
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

## Path Sample 40

- true_positive:
- false_positive:
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

## Path Sample 41

- true_positive:
- false_positive:
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

## Path Sample 42

- true_positive:
- false_positive:
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

## Path Sample 43

- true_positive:
- false_positive:
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

## Path Sample 44

- true_positive:
- false_positive:
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

## Path Sample 45

- true_positive:
- false_positive:
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

## Path Sample 46

- true_positive:
- false_positive:
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

## Path Sample 47

- true_positive:
- false_positive:
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

## Path Sample 48

- true_positive:
- false_positive:
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

## Path Sample 49

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://e314ebd2b2f4fdcf`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/33689dd5e7b5b0e142013301d3edad27`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/33689dd5e7b5b0e142013301d3edad27 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://525409bcf0234adccd537f9917ca78e2` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://d70a13f5febc14bf40dce35a33fd8be1, edge://32d1b2b9de09f14476e727f35b91d6ce` |

### Source Spans

- `packages/cli/src/arl.mjs:719-719`

```text
717:     ok = await runtimeInstall(repoRoot, rest[0]);
718:   } else if (command === "runtime" && subcommand === "pin") {
719:     ok = await runtimePin(repoRoot, rest[0]);
720:   } else if (command === "codex" && subcommand === "upstream-status") {
721:     console.log("upstream/openai-codex pinned at 163eac9306e86b38c5ab3986eefd5fd3be616b06");
```

## Path Sample 50

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://2eda44b4f2730945`
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
| `edge://54d266c80d0480df7f7486253aed5685` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://15e91fce6bd8bc9fdc56a5a1c2ef656d, edge://a7135feea88c4fa068b10a7913a0413e` |

### Source Spans

- `packages/cli/src/arl.mjs:713-713`

```text
711:     ok = await bundleCommand(repoRoot, subcommand, rest[0]);
712:   } else if (command === "runtime" && subcommand === "doctor") {
713:     ok = await doctor(repoRoot);
714:   } else if (command === "runtime" && subcommand === "versions") {
715:     ok = await runtimeVersions(repoRoot);
```

## Path Sample 51

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://107474d7c63aa8c7`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/2ac9b3ca8e0ed99bf7c34ddb2623d591`
- target: `repo://e/09befc4f86fddabb0d9f21bf740c0d1d`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/2ac9b3ca8e0ed99bf7c34ddb2623d591 reaches repo://e/09befc4f86fddabb0d9f21bf740c0d1d via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://5641a76334fe851eec31684b9f780410` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://fb1252ec13153cf7b0d6f851122672e9, edge://c6ea7037e9e28c2ea31b0d2d9b5a5720` |

### Source Spans

- `packages/cli/src/arl.mjs:334-334`

```text
332: 
333: async function approvalsList(repoRoot, args) {
334:   const approvals = await listApprovalRequests(repoRoot, { includeDecided: !args.includes("--pending") });
335:   if (approvals.length === 0) {
336:     console.log("no approvals");
```

## Path Sample 52

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://52cb6fcbc5e0519b`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/fb90038466510167c65c2d49291ae84d`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/fb90038466510167c65c2d49291ae84d via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://5bef5f037d66d44a663f42dcc042cbd4` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://8b5d6ae5630cfc50746c278cd3a8029a, edge://1160f64b7bd8bfaf2ac3c470166c5495` |

### Source Spans

- `packages/cli/src/arl.mjs:728-728`

```text
726:     ok = await dashboardDev(repoRoot, rest);
727:   } else if (command === "release" && subcommand === "package") {
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
```

## Path Sample 53

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://adc060f9d8842138`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/fd26113e8acbff9e81cc883d5942c268`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/fd26113e8acbff9e81cc883d5942c268 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://5ca87c44836ad6e854ab0ae5cc2295c6` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://58fef6d741d6b840260e3fe088fd406a, edge://452934f35b037084b79e7eb3960337f2` |

### Source Spans

- `packages/cli/src/arl.mjs:699-699`

```text
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
700:   } else if (command === "approvals" && subcommand === "list") {
701:     ok = await approvalsList(repoRoot, rest);
```

## Path Sample 54

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://61514b0560f37fde`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/9ce46550cc644ef94f3a0eb5aeb984db`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/9ce46550cc644ef94f3a0eb5aeb984db via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://6123d00c5de97a3a1881842ac8a567f8` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://3be5f241492272d356edbafabad51071` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
```

## Path Sample 55

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://02d9f042f26f9f81`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/e79c2fd45f73da82ca9a95064a450f98`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/e79c2fd45f73da82ca9a95064a450f98 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://615cf8d6ebb9a5c496e589a273ef0ad6` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://fe1439e651d83267dc1f1c74b4d3778d, edge://8f3f76ec5391336ff490e2bbf5375039` |

### Source Spans

- `packages/cli/src/arl.mjs:732-732`

```text
730:     ok = await releaseVerify(repoRoot, rest[0]);
731:   } else if (command === "release" && subcommand === "update-check") {
732:     ok = await releaseUpdateCheck(repoRoot, rest);
733:   } else {
734:     console.error(usage());
```

## Path Sample 56

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://fd10158150b97328`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/f07b7cbbebeff2b5dbee422f64f4a6df`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/f07b7cbbebeff2b5dbee422f64f4a6df via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://62832519ac70fc79347306e1b0d9725f` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://47bd9ebe74e692c74034b1d5aa54f511, edge://3f9ba0f0f872e1a5efd0bf5fbf1d2ba3` |

### Source Spans

- `packages/cli/src/arl.mjs:705-705`

```text
703:     ok = await approvalsRespond(repoRoot, rest[0], rest[1], rest.slice(2).join(" ") || undefined);
704:   } else if (command === "evidence" && subcommand === "check") {
705:     ok = await evidenceCheck(repoRoot, rest[0]);
706:   } else if (command === "verify") {
707:     ok = await verifyCommand(repoRoot, subcommand);
```

## Path Sample 57

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://5aec13c6a559e616`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/a7c420ed9ef6226172fce1506b62c37f`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/a7c420ed9ef6226172fce1506b62c37f via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://62ffe875740d6c1dfa488a36e2b54577` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://f6b5c5718beab1af544a45681b25d1e5` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
```

## Path Sample 58

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://54549c2e490bc8f3`
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
| `edge://63a5bb3f8dcc313308aa9c17fb888951` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://4e5a6839a13d5925c8101abc4c0c87f7, edge://4831781750511076804f6183d2bc7830` |

### Source Spans

- `packages/cli/src/arl.mjs:622-622`

```text
620:     repoRoot,
621:     manifestSource: flagValue(args, "--manifest"),
622:     currentVersion: flagValue(args, "--current-version")
623:   });
624:   if (args.includes("--json")) {
```

## Path Sample 59

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://35d2e5ae2c8067b5`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/1c110b1feb0ff75be8e8993723f8d78d`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/1c110b1feb0ff75be8e8993723f8d78d via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://68004adf71fcf720e4c255424f453492` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://3499bdf5d37f1433bd8cc0e1b69f0075, edge://aa29e288af092337703be1cc2e69e849` |

### Source Spans

- `packages/cli/src/arl.mjs:673-673`

```text
671: async function main() {
672:   const repoRoot = process.cwd();
673:   await loadLocalEnv(repoRoot);
674:   const [command, subcommand, ...rest] = process.argv.slice(2);
675:   let ok = false;
```

## Path Sample 60

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://bba216dafd05ad75`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/ac5d3dc87bb7ec5132f4f2f577ed40e7`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/ac5d3dc87bb7ec5132f4f2f577ed40e7 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://6b922a14517beb7aca4328a0570d82a0` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://8b5d6ae5630cfc50746c278cd3a8029a, edge://ce08a79c60c6c76192098daf94ad64f3` |

### Source Spans

- `packages/cli/src/arl.mjs:728-728`

```text
726:     ok = await dashboardDev(repoRoot, rest);
727:   } else if (command === "release" && subcommand === "package") {
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
```

## Path Sample 61

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://43e573d861f47531`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/5796bc0452bda4061a0c08e694776a30`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/5796bc0452bda4061a0c08e694776a30 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://701f7e0da1e3c0d576923b950822615b` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://8b5d6ae5630cfc50746c278cd3a8029a, edge://91eddd9f21a82d35d879982ed7216523` |

### Source Spans

- `packages/cli/src/arl.mjs:728-728`

```text
726:     ok = await dashboardDev(repoRoot, rest);
727:   } else if (command === "release" && subcommand === "package") {
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
```

## Path Sample 62

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://d62821174da59d86`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/70d47ce0e5038f28adda19ded6901de2`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/70d47ce0e5038f28adda19ded6901de2 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://70893191889f41ad919f950bd0c6d997` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://500312198d9af9813be2b5e6074a59b3` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
```

## Path Sample 63

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://369ca5bd147ea485`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/b71810f1651f0b5f0be779eec34176bd`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/b71810f1651f0b5f0be779eec34176bd via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://709c093b97fa6308127cdd8c0005864a` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://28b144c3bcc4444da2a563d45cdb31b3, edge://840d44a8b838faee880c54e53fb8c2c8` |

### Source Spans

- `packages/cli/src/arl.mjs:697-697`

```text
695:     ok = await runsShow(repoRoot, rest[0]);
696:   } else if (command === "runs" && subcommand === "replay") {
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
```

## Path Sample 64

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://6affc0be9aa458b6`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/300fed9fcf8b50a47be18968401cf79e`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/300fed9fcf8b50a47be18968401cf79e via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://72fd1b8a14a82304902a6da3d5aea94e` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://650600ef52b2d90a174e846aff556f38, edge://8fbc07333362ef9845ed03695e90c796` |

### Source Spans

- `packages/cli/src/arl.mjs:693-693`

```text
691:     ok = await agentRunCommand(repoRoot, rest);
692:   } else if (command === "runs" && subcommand === "list") {
693:     ok = await runsList(repoRoot);
694:   } else if (command === "runs" && subcommand === "show") {
695:     ok = await runsShow(repoRoot, rest[0]);
```

## Path Sample 65

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://7a0a613155638b3c`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/d792ccf38cb0640f64ba5f7d9d48de85`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/d792ccf38cb0640f64ba5f7d9d48de85 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://736d54573a49dbbfb2252d50de027461` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://9bb421b698ee32e8c5421b11c2b1e0ea, edge://f76d5f9f47687dfefd953562ff284b64` |

### Source Spans

- `packages/cli/src/arl.mjs:695-695`

```text
693:     ok = await runsList(repoRoot);
694:   } else if (command === "runs" && subcommand === "show") {
695:     ok = await runsShow(repoRoot, rest[0]);
696:   } else if (command === "runs" && subcommand === "replay") {
697:     ok = await runsReplay(repoRoot, rest[0]);
```

## Path Sample 66

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://7958d59806d2617a`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/b5a1a01f886a43c39d13d2e420a1ccd5`
- target: `repo://e/992efbbc5b461e734095ca62f7c8375d`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/b5a1a01f886a43c39d13d2e420a1ccd5 reaches repo://e/992efbbc5b461e734095ca62f7c8375d via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://73b31dc74a736a34078a547fddf9e322` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://ce145c6dbe8dacce9c0b363ae14c68fc, edge://8e832262d6a57334eef27cbcc7fe3ece` |

### Source Spans

- `packages/cli/src/arl.mjs:356-356`

```text
354:     return false;
355:   }
356:   const approval = await respondApprovalRequest(repoRoot, approvalId, decision, reason);
357:   console.log(`approval_id=${approval.approval_id}`);
358:   console.log(`status=${approval.status}`);
```

## Path Sample 67

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://5c225016f952d12a`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/b5a1a01f886a43c39d13d2e420a1ccd5`
- target: `repo://e/cd576bcf1669b1f7deaf783759f6a1b9`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/b5a1a01f886a43c39d13d2e420a1ccd5 reaches repo://e/cd576bcf1669b1f7deaf783759f6a1b9 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://76b617c41c7491cec2df8e301e7387e0` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://ce145c6dbe8dacce9c0b363ae14c68fc, edge://a8da2cb51cf18749b2e24691ef0b5c57` |

### Source Spans

- `packages/cli/src/arl.mjs:356-356`

```text
354:     return false;
355:   }
356:   const approval = await respondApprovalRequest(repoRoot, approvalId, decision, reason);
357:   console.log(`approval_id=${approval.approval_id}`);
358:   console.log(`status=${approval.status}`);
```

## Path Sample 68

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://712bad7aecc85a65`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/2d82c41a88e8153c1ff37e2ed8070dce`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/2d82c41a88e8153c1ff37e2ed8070dce via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://76d5f6404aec84a382891546d6987671` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://28b144c3bcc4444da2a563d45cdb31b3, edge://4a43542924dc1f9e4201680ca8eb7258` |

### Source Spans

- `packages/cli/src/arl.mjs:697-697`

```text
695:     ok = await runsShow(repoRoot, rest[0]);
696:   } else if (command === "runs" && subcommand === "replay") {
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
```

## Path Sample 69

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://37c8a65c6a03e785`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/dc2dab64f630a7dd445b21c9e6ccb413`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/dc2dab64f630a7dd445b21c9e6ccb413 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://7744666895bf4e9a36185822fc329cb4` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://3499bdf5d37f1433bd8cc0e1b69f0075, edge://1c81b5d5e1478e729316d005bd58c868` |

### Source Spans

- `packages/cli/src/arl.mjs:673-673`

```text
671: async function main() {
672:   const repoRoot = process.cwd();
673:   await loadLocalEnv(repoRoot);
674:   const [command, subcommand, ...rest] = process.argv.slice(2);
675:   let ok = false;
```

## Path Sample 70

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://4c4b0767aa4ecae0`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/3ddae319fb297ddb19c73bdd56544951`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/3ddae319fb297ddb19c73bdd56544951 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://77be19acb699ae99817ccf9e8b8b6ef3` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://e972e5ef67805c3313eff621f0dd8489` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
```

## Path Sample 71

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://66de7e06054a77e5`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/7808fc9b214424f9d9fafc8bb1aa318f`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/7808fc9b214424f9d9fafc8bb1aa318f via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://7882ac320e20702c224b807e6e6128ca` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://28b144c3bcc4444da2a563d45cdb31b3, edge://8d0e7101d3269e40ffd8b108edcdbd16` |

### Source Spans

- `packages/cli/src/arl.mjs:697-697`

```text
695:     ok = await runsShow(repoRoot, rest[0]);
696:   } else if (command === "runs" && subcommand === "replay") {
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
```

## Path Sample 72

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://9c17722915817a35`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/1f84523f608815b19ab7fabccad2e153`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/1f84523f608815b19ab7fabccad2e153 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://7a5bc397df8c2c267073ebaed60f6158` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://36cab132e43152124e07e1f671a4c304` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
```

## Path Sample 73

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://dc2a76ddb8ae2f87`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/a42c323ea6338fe792b59f78604d45c5`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/a42c323ea6338fe792b59f78604d45c5 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://7db3c995710eba88b8f054d396a7b666` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://5b20da893a06b001decdadd31e345c0f, edge://c05f3627a5740c7cf9d0aa99b8d06a52` |

### Source Spans

- `packages/cli/src/arl.mjs:682-682`

```text
680:     ok = await doctor(repoRoot);
681:   } else if (command === "preflight") {
682:     ok = await preflightCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
683:   } else if (command === "init") {
684:     console.log("Autoresearch workspace already initialized by repository scaffold.");
```

## Path Sample 74

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://1478bdfa9b82875d`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/3e3c45010cea2c6c4dec2ae115f1d202`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/3e3c45010cea2c6c4dec2ae115f1d202 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://829fd745cdbe3120c7c892bcb4f35a9e` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://df4f5e0fa3bc4aa5f87751c47ffa95ef, edge://96e59e9ff87ffc7f83758454b46848c1` |

### Source Spans

- `packages/cli/src/arl.mjs:691-691`

```text
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
692:   } else if (command === "runs" && subcommand === "list") {
693:     ok = await runsList(repoRoot);
```

## Path Sample 75

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://2b881b656ed08c0e`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/95aaca4dc083924ca6abb2f500e1d006`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/95aaca4dc083924ca6abb2f500e1d006 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://837756ebb4841314fdc577a55751c13a` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://58fef6d741d6b840260e3fe088fd406a, edge://44acaddffaa909461775d662ddc43e64` |

### Source Spans

- `packages/cli/src/arl.mjs:699-699`

```text
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
700:   } else if (command === "approvals" && subcommand === "list") {
701:     ok = await approvalsList(repoRoot, rest);
```

## Path Sample 76

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://05a29b6d9c155a49`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/70a1d01a9934fb36fc420f34e398fce8`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/70a1d01a9934fb36fc420f34e398fce8 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://83ab227cc550ff0c005b151e6167109a` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://4cf72335dac3c932fc2db0ed9cce7d70, edge://41f36c925926110ce15f14a75d915a06` |

### Source Spans

- `packages/cli/src/arl.mjs:726-726`

```text
724:     ok = await codexPatchStatus(repoRoot, rest);
725:   } else if (command === "dashboard" && subcommand === "dev") {
726:     ok = await dashboardDev(repoRoot, rest);
727:   } else if (command === "release" && subcommand === "package") {
728:     ok = await releasePackage(repoRoot, rest);
```

## Path Sample 77

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://9b78cca3c8e20e14`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/47563d40361e18e6b8a0c5a91e1e7aa4`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/47563d40361e18e6b8a0c5a91e1e7aa4 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://8422d5d2e62e97dddfdf54115171ab33` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://c05154316dbbc93d0b6314fa4e6914a3` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
```

## Path Sample 78

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://b21db79b67c41d0a`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/6c32f59c8e3d06f92a40e0d158f61f9b`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/6c32f59c8e3d06f92a40e0d158f61f9b via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://860a187983761476249cfd36bbaf07d0` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://9cd9851a154afdf1dbbe73c029e3b5ab` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
```

## Path Sample 79

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://a351d160877ad648`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/6983ee8f4273f3d255c9648faf2c595c`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/6983ee8f4273f3d255c9648faf2c595c via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://86729adc871e18062375d3062e6b886c` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://28b144c3bcc4444da2a563d45cdb31b3, edge://cf6067da5f695633f51a834b5944f19d` |

### Source Spans

- `packages/cli/src/arl.mjs:697-697`

```text
695:     ok = await runsShow(repoRoot, rest[0]);
696:   } else if (command === "runs" && subcommand === "replay") {
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
```

## Path Sample 80

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://3307e07e807a4e16`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/68c39b07d1877c237e8b1d5ae5b00259`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/68c39b07d1877c237e8b1d5ae5b00259 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://89c745d06a114dfb40adece000653d99` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://5b20da893a06b001decdadd31e345c0f, edge://01e4e2d612a53ec3946a75947ab0d6a9` |

### Source Spans

- `packages/cli/src/arl.mjs:682-682`

```text
680:     ok = await doctor(repoRoot);
681:   } else if (command === "preflight") {
682:     ok = await preflightCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
683:   } else if (command === "init") {
684:     console.log("Autoresearch workspace already initialized by repository scaffold.");
```

## Path Sample 81

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://576962a9311c4844`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/1958c675a937055cce4063af8d74502e`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/1958c675a937055cce4063af8d74502e via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://8a666f6aaa68b564650985d86ccb4b2a` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://88315076e517d6e493f092c68e708fb6, edge://7e894de975868f8eb0c70477a1740224` |

### Source Spans

- `packages/cli/src/arl.mjs:730-730`

```text
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
731:   } else if (command === "release" && subcommand === "update-check") {
732:     ok = await releaseUpdateCheck(repoRoot, rest);
```

## Path Sample 82

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://5bddc39cc4768b3b`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/9ee4b056b285b0419d6fb731a7376a43`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/9ee4b056b285b0419d6fb731a7376a43 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://8a7fdeeae7b081a8afabf8bc2d849a5e` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://a7e18ffbdcad5d634c12f9a35a456c19` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
```

## Path Sample 83

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://2b20d9ffff83831f`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/bf072b838f536179535ce9264182125f`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/bf072b838f536179535ce9264182125f via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://8b7c2c8e4e03e03e3f417db3b1dbf6cc` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://8b5d6ae5630cfc50746c278cd3a8029a, edge://1755bcbec044a09b68a6647acc8f27e5` |

### Source Spans

- `packages/cli/src/arl.mjs:728-728`

```text
726:     ok = await dashboardDev(repoRoot, rest);
727:   } else if (command === "release" && subcommand === "package") {
728:     ok = await releasePackage(repoRoot, rest);
729:   } else if (command === "release" && subcommand === "verify") {
730:     ok = await releaseVerify(repoRoot, rest[0]);
```

## Path Sample 84

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://0397e1c75826a906`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/166c9e49e94fe354f59d3c3f51d3b64a`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/166c9e49e94fe354f59d3c3f51d3b64a via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://8d2bf8c4ba3d24f3a5140e4c4ca69091` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://4aa07cf8663b0ab996b4f9cd0096ae83` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
```

## Path Sample 85

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://ea8899ec9fc4c0df`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/d6ce1c942e9d3ac419f305bf160e862e`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/d6ce1c942e9d3ac419f305bf160e862e via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://8dc7a9f8582ba9c938e425cb8d0de1e3` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://cd6983b6c761af7f996b1384dbef53b1, edge://4434220612e980309876da93f7e81042` |

### Source Spans

- `packages/cli/src/arl.mjs:724-724`

```text
722:     ok = true;
723:   } else if (command === "codex" && subcommand === "patch-status") {
724:     ok = await codexPatchStatus(repoRoot, rest);
725:   } else if (command === "dashboard" && subcommand === "dev") {
726:     ok = await dashboardDev(repoRoot, rest);
```

## Path Sample 86

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://9b39b3db5944ded1`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/00c50396d8ac0dd0744565489e86d2b6`
- target: `repo://e/4be8e06a4b2b2eea437c87e7be6a4af8`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/00c50396d8ac0dd0744565489e86d2b6 reaches repo://e/4be8e06a4b2b2eea437c87e7be6a4af8 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://8f3a69bd8a228b24101c05f545d38efa` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://0e73905431239e4650af4cec4016af20, edge://4831781750511076804f6183d2bc7830` |

### Source Spans

- `packages/cli/src/arl.mjs:639-639`

```text
637: 
638: async function dashboardDev(repoRoot, args) {
639:   const portRaw = flagValue(args, "--port") ?? process.env.ARL_DASHBOARD_PORT ?? "4177";
640:   const port = Number(portRaw);
641:   if (!Number.isInteger(port) || port < 0 || port > 65535) {
```

## Path Sample 87

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://47cf2dcee9382ddc`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/7756f85b6e286128043b8712087e663e`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/7756f85b6e286128043b8712087e663e via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://90dabc9f92f97ef0d0706ebe3ab8b8c2` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://9edf97b1443bc3a38230566f26420159, edge://b3cfae5d43edadc52b4ab98e600da7af` |

### Source Spans

- `packages/cli/src/arl.mjs:687-687`

```text
685:     ok = true;
686:   } else if (command === "objective" && subcommand === "validate") {
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
```

## Path Sample 88

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://47d8fe23c8e96973`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/b5a1a01f886a43c39d13d2e420a1ccd5`
- target: `repo://e/903929dbea82483a72ffac4297338460`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/b5a1a01f886a43c39d13d2e420a1ccd5 reaches repo://e/903929dbea82483a72ffac4297338460 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://936fd93ed2d535b9a7e7490cd69b3f5f` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://ce145c6dbe8dacce9c0b363ae14c68fc, edge://bcd7d56326553667310f0dc413429a65` |

### Source Spans

- `packages/cli/src/arl.mjs:356-356`

```text
354:     return false;
355:   }
356:   const approval = await respondApprovalRequest(repoRoot, approvalId, decision, reason);
357:   console.log(`approval_id=${approval.approval_id}`);
358:   console.log(`status=${approval.status}`);
```

## Path Sample 89

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://432610a361e091fd`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/592e1594634de3afcc6b162f2ecaa519`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/592e1594634de3afcc6b162f2ecaa519 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://94505cab4972b28774dd53f60b508dbd` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://28b144c3bcc4444da2a563d45cdb31b3, edge://23cdcb6f8e212d7fd5a7e79bd284ba35` |

### Source Spans

- `packages/cli/src/arl.mjs:697-697`

```text
695:     ok = await runsShow(repoRoot, rest[0]);
696:   } else if (command === "runs" && subcommand === "replay") {
697:     ok = await runsReplay(repoRoot, rest[0]);
698:   } else if (command === "runs" && subcommand === "cancel") {
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
```

## Path Sample 90

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://55157cc7180a35fb`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/a742fc728f0cad2d5bd64a1552ca97ff`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/a742fc728f0cad2d5bd64a1552ca97ff via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://957edd0a5f753c648bd598109cfe9c6a` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://b8c86e0ea94c6f3b38818311ab01fbdd, edge://4e58be589a38003a1e2560ec56e37998` |

### Source Spans

- `packages/cli/src/arl.mjs:701-701`

```text
699:     ok = await runsCancel(repoRoot, rest[0], rest.slice(1).join(" ") || undefined);
700:   } else if (command === "approvals" && subcommand === "list") {
701:     ok = await approvalsList(repoRoot, rest);
702:   } else if (command === "approvals" && subcommand === "respond") {
703:     ok = await approvalsRespond(repoRoot, rest[0], rest[1], rest.slice(2).join(" ") || undefined);
```

## Path Sample 91

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://119a3480acb1cf28`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/9f9846cdf4c196a6a00a9d07d65ab548`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/9f9846cdf4c196a6a00a9d07d65ab548 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://9b270367ae32db6a8636e41679a53fdc` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://17925e350e9f3730be7d833c46de8882` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
```

## Path Sample 92

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://5166c72fa0098aa7`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/67a40c39abf3dd5e22ec5f6017776768`
- target: `repo://e/eac5c0406c9d8a8e56c46a02873e9fa4`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/67a40c39abf3dd5e22ec5f6017776768 reaches repo://e/eac5c0406c9d8a8e56c46a02873e9fa4 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://9c3aa6d4961f1b3ef485575ddab862b8` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://2886ceb37c0293cceaf32d773847bf86, edge://504366eb63e01e06a66725309932b7ec` |

### Source Spans

- `packages/cli/src/arl.mjs:660-660`

```text
658:     return false;
659:   }
660:   const result = await runFreeformAgentPrompt(prompt, repoRoot);
661:   console.log(`objective=${result.objectivePath}`);
662:   console.log(`run_id=${result.runId}`);
```

## Path Sample 93

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://f0c3bd0313c1c040`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/b875c2343837c341c2ccb3e2963ac3df`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/b875c2343837c341c2ccb3e2963ac3df via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://9d445a7172979b701e3fc49d444e2142` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://9b4a865ad20c16595bc64e128e959ccf` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
```

## Path Sample 94

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://aeb83a44937276a4`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/67a40c39abf3dd5e22ec5f6017776768`
- target: `repo://e/803b18e173f586a1c2d09bb9b03985e3`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/67a40c39abf3dd5e22ec5f6017776768 reaches repo://e/803b18e173f586a1c2d09bb9b03985e3 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://9e75a98d0116deca15a763a327fbd40c` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://2886ceb37c0293cceaf32d773847bf86, edge://0a132fa527caa487bda704db4efd1519` |

### Source Spans

- `packages/cli/src/arl.mjs:660-660`

```text
658:     return false;
659:   }
660:   const result = await runFreeformAgentPrompt(prompt, repoRoot);
661:   console.log(`objective=${result.objectivePath}`);
662:   console.log(`run_id=${result.runId}`);
```

## Path Sample 95

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://c497ccfeaf5d10ad`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/7ec0fa5bc039c7a724cbcdcc1a685631`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/7ec0fa5bc039c7a724cbcdcc1a685631 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://9eb0be15c02feddd9f80b41fa62a6eb7` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a28cdb37e175697ce45b92d1ba7ed356, edge://50e97c444edfaf1e6f214ffe6b251b88` |

### Source Spans

- `packages/cli/src/arl.mjs:689-689`

```text
687:     ok = await validateCommand(repoRoot, rest[0]);
688:   } else if (command === "run") {
689:     ok = await runCommand(repoRoot, [subcommand, ...rest].filter(Boolean));
690:   } else if (command === "agent" && subcommand === "run") {
691:     ok = await agentRunCommand(repoRoot, rest);
```

## Path Sample 96

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://78646bccf59889fb`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/0932b5502f710714c684f9d17a45fb72`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/0932b5502f710714c684f9d17a45fb72 via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://a2cbbe14999c3f5e2cdca473a8ec4134` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://94609b0e1d0183685086447fa25d1f1e` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
```

## Path Sample 97

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://ba3b0122abf843bc`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/4a2278bc920360444b92b49ebca96a76`
- target: `repo://e/b0aa0402e18461fe5dab7ec40730cabc`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/4a2278bc920360444b92b49ebca96a76 reaches repo://e/b0aa0402e18461fe5dab7ec40730cabc via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://a3bad1940f7c4bb18dd8f9b6c6a9fdeb` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://c67120505f0fd02d9b6e94292bc3ec57, edge://5bc5556adca537c2ac41a9bd151d5ae8` |

### Source Spans

- `packages/cli/src/arl.mjs:192-192`

```text
190:   }
191:   if (runtime === "multi-agent-shim") {
192:     const result = await runMultiAgentFixture(objectivePath, repoRoot);
193:     console.log(`run_id=${result.runId}`);
194:     console.log(`run_root=${path.relative(repoRoot, result.runRoot)}`);
```

## Path Sample 98

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://875fd901b211883c`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb`
- target: `repo://e/7bed18ebff53c66ea00b5b759a28536c`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/cc889cc8f82a59b18e3abb3dfa54dddb reaches repo://e/7bed18ebff53c66ea00b5b759a28536c via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://a3c4e61aa8d62bc0d04f4193c64698e6` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://ae18d50a8f4c0efedcf6417d2f0426fc, edge://37d03e84e5bf6c1c8a5c52d026f443da` |

### Source Spans

- `packages/cli/src/arl.mjs:715-715`

```text
713:     ok = await doctor(repoRoot);
714:   } else if (command === "runtime" && subcommand === "versions") {
715:     ok = await runtimeVersions(repoRoot);
716:   } else if (command === "runtime" && subcommand === "install") {
717:     ok = await runtimeInstall(repoRoot, rest[0]);
```

## Path Sample 99

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://5430c9e1a8df26c8`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/aafd7770c65d6a3b976c57748f027d2d`
- target: `repo://e/0cadd519ffb64a5b5e156b1e0d37de8d`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/aafd7770c65d6a3b976c57748f027d2d reaches repo://e/0cadd519ffb64a5b5e156b1e0d37de8d via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://a3fc29f5cbcd04bcbd88e932ce5f5782` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://9a1c4162a1c72c6576343fc8f1a37347, edge://7b5fcb4cc40828da5d7e560221552e50` |

### Source Spans

- `packages/cli/src/arl.mjs:472-472`

```text
470: 
471: async function runtimeVersions(repoRoot) {
472:   const paths = runtimePaths(repoRoot);
473:   const execBinary = await locateRuntimeBinary(repoRoot, "arl-codex-exec");
474:   const daemonBinary = await locateRuntimeBinary(repoRoot, "arl-codexd");
```

## Path Sample 100

- true_positive:
- false_positive:
- wrong_direction:
- wrong_target:
- wrong_span:
- stale:
- duplicate:
- unresolved_mislabeled_exact:
- test_mock_leaked:
- derived_missing_provenance:
- unsure:
- path_id: `path://c7d86c024b0b9bc8`
- generated_by_audit: `false`
- task_or_query: `index://stored-path-evidence`
- source: `repo://e/1dbe62be52214311598aff62eac2f1fb`
- target: `repo://e/0cadd519ffb64a5b5e156b1e0d37de8d`
- relation_sequence: `MAY_MUTATE`
- exactness: `derived_from_verified_edges`
- confidence: `0.8695652173913044`
- derived_provenance_label: `derived_with_provenance`
- context: `production_inferred`
- missing_metadata: ``
- summary: `repo://e/1dbe62be52214311598aff62eac2f1fb reaches repo://e/0cadd519ffb64a5b5e156b1e0d37de8d via MAY_MUTATE`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge://aa9729b51224098dcd44d0db4152ef23` | `MAY_MUTATE` | `head_to_tail` | `derived_from_verified_edges` | 1 | `true` | `production` | `derived` | `edge://a7a2518283a30016406c018092916150, edge://7b5fcb4cc40828da5d7e560221552e50` |

### Source Spans

- `packages/cli/src/arl.mjs:427-427`

```text
425:     return false;
426:   }
427:   const paths = runtimePaths(repoRoot);
428:   const execBinary = await locateRuntimeBinary(repoRoot, "arl-codex-exec");
429:   const daemonBinary = await locateRuntimeBinary(repoRoot, "arl-codexd");
```


## Notes

- Classification fields are intentionally blank in markdown for human review.
- PathEvidence sampling is bounded: candidate path IDs are selected first, details are batch-loaded only for those IDs, and snippets are loaded only for sampled spans when requested.
- Mode `proof` disables generated fallback paths.
- Path edge materialization load cap: 2000 rows; truncated: false.
- Stored PathEvidence samples used before fallback: 100.
