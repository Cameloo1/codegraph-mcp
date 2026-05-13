# PathEvidence Sample Audit

Database: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/fixture_workers4.sqlite`

Limit: `20`

Seed: `17`

Stored PathEvidence rows: `0`

Generated fallback samples: `20`

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
- path_id: `generated://audit/edge-key:-9203119335740638116`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/af824afc40aa9bc54b31726d12081117`
- target: `repo://e/978efa52f9bfd5d3eb88b7cf3839b181`
- relation_sequence: `CALLS`
- exactness: `static_heuristic`
- confidence: `0.55`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/af824afc40aa9bc54b31726d12081117 -CALLS-> repo://e/978efa52f9bfd5d3eb88b7cf3839b181`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9203119335740638116` | `CALLS` | `head_to_tail` | `static_heuristic` | 0.55 | `false` | `production_inferred` | `base_heuristic` | `` |

### Source Spans

- `dynamic_import_marked_heuristic/repo/src/loader.ts:2-2`

```text
1: export async function loadPlugin(name: string) {
2:   const mod = await import("./plugins/" + name);
3:   return mod.default();
4: }
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
- path_id: `generated://audit/edge-key:-9186605581599358930`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/6a1b0c3cc14d15484c5a384f59464b76`
- target: `repo://e/d06e57aa4f843b868d3d6c69e446827c`
- relation_sequence: `MOCKS`
- exactness: `static_heuristic`
- confidence: `0.64`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `mock_or_stub_inferred`
- missing_metadata: `repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/6a1b0c3cc14d15484c5a384f59464b76 -MOCKS-> repo://e/d06e57aa4f843b868d3d6c69e446827c`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9186605581599358930` | `MOCKS` | `head_to_tail` | `static_heuristic` | 0.64 | `false` | `mock_or_stub_inferred` | `test_or_mock` | `` |

### Source Spans

- `test_mock_not_production_call/repo/tests/checkout.test.ts:6-6`

```text
4: 
5: vi.mock("../src/service", () => ({
6:   chargeCard: vi.fn(() => "mocked")
7: }));
8: 
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
- path_id: `generated://audit/edge-key:-9184959491801231768`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/0cf32c78426c163d2d92ecc618c7354b`
- target: `repo://e/0570030f46c7d0bfeb2dd40aa28332ad`
- relation_sequence: `DEFINED_IN`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/0cf32c78426c163d2d92ecc618c7354b -DEFINED_IN-> repo://e/0570030f46c7d0bfeb2dd40aa28332ad`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9184959491801231768` | `DEFINED_IN` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `same_function_name_only_one_imported/repo/src/a.ts:2-2`

```text
1: export function chooseUser(id: string) {
2:   return `A:${id}`;
3: }
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
- path_id: `generated://audit/edge-key:-9183397129905989238`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/51fff005ee0ed8b8cd46f751b4c4f2fa`
- target: `repo://e/6a74682889eab28dc7f2e0f88178020f`
- relation_sequence: `CONTAINS`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/51fff005ee0ed8b8cd46f751b4c4f2fa -CONTAINS-> repo://e/6a74682889eab28dc7f2e0f88178020f`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9183397129905989238` | `CONTAINS` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `admin_user_middleware_role_separation/repo/src/auth.ts:6-6`

```text
4: 
5: export function requireUser(user: any) {
6:   return checkRole(user, "user");
7: }
8: 
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
- path_id: `generated://audit/edge-key:-9167314706337965205`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/49f0d2e7f224b22c0a5a39e7cb800d4e`
- target: `repo://e/dab42b501d98f20fd7dd491e0d09b2fd`
- relation_sequence: `RETURNS_TO`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/49f0d2e7f224b22c0a5a39e7cb800d4e -RETURNS_TO-> repo://e/dab42b501d98f20fd7dd491e0d09b2fd`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9167314706337965205` | `RETURNS_TO` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `derived_closure_edge_requires_provenance/repo/src/service.ts:4-4`

```text
2: 
3: export function submitOrder(order: any) {
4:   return saveOrder(order);
5: }
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
- path_id: `generated://audit/edge-key:-9167193223427871390`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/cc687a46b4296d6066e5d993fe4126d2`
- target: `repo://e/b63e935a95390a36f7562e1f7da82b00`
- relation_sequence: `DEFINES`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/cc687a46b4296d6066e5d993fe4126d2 -DEFINES-> repo://e/b63e935a95390a36f7562e1f7da82b00`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9167193223427871390` | `DEFINES` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `derived_closure_edge_requires_provenance/repo/src/service.ts:1-6`

```text
1: import { saveOrder, ordersTable } from "./store";
2: 
3: export function submitOrder(order: any) {
4:   return saveOrder(order);
5: }
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
- path_id: `generated://audit/edge-key:-9158827528441336654`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/0b91a650eb5eef675fce7c9c05416fdd`
- target: `repo://e/0912589e6c1abf6348d02f568b2030fd`
- relation_sequence: `DEFINED_IN`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `test_or_fixture_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/0b91a650eb5eef675fce7c9c05416fdd -DEFINED_IN-> repo://e/0912589e6c1abf6348d02f568b2030fd`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9158827528441336654` | `DEFINED_IN` | `head_to_tail` | `parser_verified` | 1 | `false` | `test_or_fixture_inferred` | `test_or_mock` | `` |

### Source Spans

- `test_mock_not_production_call/repo/tests/checkout.test.ts:2-2`

```text
1: import { describe, expect, it, vi } from "vitest";
2: import { checkout } from "../src/checkout";
3: import { chargeCard } from "../src/service";
4: 
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
- path_id: `generated://audit/edge-key:-9147749043063313696`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/d35b8c8a8a398c6063a17b843698622e`
- target: `repo://e/b862883df700d4054c0554d737fc20fb`
- relation_sequence: `EXPORTS`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/d35b8c8a8a398c6063a17b843698622e -EXPORTS-> repo://e/b862883df700d4054c0554d737fc20fb`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9147749043063313696` | `EXPORTS` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `barrel_export_default_export_resolution/repo/src/use.ts:3-5`

```text
1: import { defaultFeature, namedFeature } from "./index";
2: 
3: export function runDefault() {
4:   return defaultFeature();
5: }
6: 
7: export function runNamed() {
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
- path_id: `generated://audit/edge-key:-9134365703709946445`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/026e70c08ba2aa164b666205b41ce45c`
- target: `repo://e/fae56d36230b390c08499fad73310c7a`
- relation_sequence: `DECLARES`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/026e70c08ba2aa164b666205b41ce45c -DECLARES-> repo://e/fae56d36230b390c08499fad73310c7a`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9134365703709946445` | `DECLARES` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `source_span_exact_callsite/repo/src/main.ts:3-3`

```text
1: import { first, second } from "./actions";
2: 
3: export function run(flag: boolean) {
4:   first();
5:   if (flag) {
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
- path_id: `generated://audit/edge-key:-9120530291440236237`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/512f01d9d7f11941423642663ef4fc13`
- target: `repo://e/026e70c08ba2aa164b666205b41ce45c`
- relation_sequence: `DEFINED_IN`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/512f01d9d7f11941423642663ef4fc13 -DEFINED_IN-> repo://e/026e70c08ba2aa164b666205b41ce45c`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9120530291440236237` | `DEFINED_IN` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `source_span_exact_callsite/repo/src/main.ts:6-6`

```text
4:   first();
5:   if (flag) {
6:     second();
7:   }
8: }
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
- path_id: `generated://audit/edge-key:-9118877849179843568`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/17d04388e2d4fc14e84f74a37d2b2542`
- target: `repo://e/32edfe2d4112bbc88343b15291b17b76`
- relation_sequence: `CONTAINS`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/17d04388e2d4fc14e84f74a37d2b2542 -CONTAINS-> repo://e/32edfe2d4112bbc88343b15291b17b76`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9118877849179843568` | `CONTAINS` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `same_function_name_only_one_imported/repo/src/main.ts:5-5`

```text
3: 
4: export function handler(id: string) {
5:   audit(id);
6:   return chooseUser(id);
7: }
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
- path_id: `generated://audit/edge-key:-9109762047521311757`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/17d04388e2d4fc14e84f74a37d2b2542`
- target: `repo://e/54496b38ba7d6a9bdb39c05b89dad921`
- relation_sequence: `CONTAINS`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/17d04388e2d4fc14e84f74a37d2b2542 -CONTAINS-> repo://e/54496b38ba7d6a9bdb39c05b89dad921`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9109762047521311757` | `CONTAINS` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `same_function_name_only_one_imported/repo/src/main.ts:4-4`

```text
2: import { audit } from "./audit";
3: 
4: export function handler(id: string) {
5:   audit(id);
6:   return chooseUser(id);
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
- path_id: `generated://audit/edge-key:-9103623495338762252`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/1e8138c4ee352bcc68d7846dfea0e982`
- target: `repo://e/3e43ce296abfc27ecf18b6aeaa2a3048`
- relation_sequence: `EXPORTS`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/1e8138c4ee352bcc68d7846dfea0e982 -EXPORTS-> repo://e/3e43ce296abfc27ecf18b6aeaa2a3048`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9103623495338762252` | `EXPORTS` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `sanitizer_exists_but_not_on_flow/repo/src/register.ts:9-11`

```text
7: }
8: 
9: export function writeComment(comment: string) {
10:   return comment;
11: }
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
- path_id: `generated://audit/edge-key:-9088743095391623643`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/461f0dca6b2c90b0858db12ab865238a`
- target: `repo://e/20521b048eda9f1074d2c2bfe49956c6`
- relation_sequence: `ARGUMENT_0`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `test_or_fixture_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/461f0dca6b2c90b0858db12ab865238a -ARGUMENT_0-> repo://e/20521b048eda9f1074d2c2bfe49956c6`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9088743095391623643` | `ARGUMENT_0` | `head_to_tail` | `parser_verified` | 1 | `false` | `test_or_fixture_inferred` | `test_or_mock` | `` |

### Source Spans

- `test_mock_not_production_call/repo/tests/checkout.test.ts:10-10`

```text
8: 
9: describe("checkout", () => {
10:   it("uses a test double for chargeCard", () => {
11:     expect(checkout(5)).toBe("mocked");
12:   });
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
- path_id: `generated://audit/edge-key:-9078797523463538200`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/efd09837b97864ef2c254211542f3549`
- target: `repo://e/8bf153b30e86765deef61e18f0f307ff`
- relation_sequence: `DEFINES`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/efd09837b97864ef2c254211542f3549 -DEFINES-> repo://e/8bf153b30e86765deef61e18f0f307ff`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9078797523463538200` | `DEFINES` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `stale_graph_cache_after_edit_delete/repo/src/use.ts:1-6`

```text
1: import { liveTarget } from "./live";
2: 
3: export function run() {
4:   return liveTarget();
5: }
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
- path_id: `generated://audit/edge-key:-9063415924526603170`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/e678885a1a134ee45d3e7cc7644e209a`
- target: `repo://e/af824afc40aa9bc54b31726d12081117`
- relation_sequence: `DEFINED_IN`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/e678885a1a134ee45d3e7cc7644e209a -DEFINED_IN-> repo://e/af824afc40aa9bc54b31726d12081117`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9063415924526603170` | `DEFINED_IN` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `dynamic_import_marked_heuristic/repo/src/loader.ts:1-1`

```text
1: export async function loadPlugin(name: string) {
2:   const mod = await import("./plugins/" + name);
3:   return mod.default();
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
- path_id: `generated://audit/edge-key:-9022168102169518164`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/2502aaef833dfe1bc42a9250fdc892e9`
- target: `repo://e/2c59e6b2e0f7105f5edab60a3650c91d`
- relation_sequence: `DEFINED_IN`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/2502aaef833dfe1bc42a9250fdc892e9 -DEFINED_IN-> repo://e/2c59e6b2e0f7105f5edab60a3650c91d`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9022168102169518164` | `DEFINED_IN` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `import_alias_change_updates_target/repo/src/use.ts:4-4`

```text
2: 
3: export function run() {
4:   return selected();
5: }
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
- path_id: `generated://audit/edge-key:-9003926772312636661`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/d761b66ee661b4b41f607209710d3d2e`
- target: `repo://e/d35b8c8a8a398c6063a17b843698622e`
- relation_sequence: `DEFINED_IN`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `production_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/d761b66ee661b4b41f607209710d3d2e -DEFINED_IN-> repo://e/d35b8c8a8a398c6063a17b843698622e`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-9003926772312636661` | `DEFINED_IN` | `head_to_tail` | `parser_verified` | 1 | `false` | `production_inferred` | `base_exact` | `` |

### Source Spans

- `barrel_export_default_export_resolution/repo/src/use.ts:3-5`

```text
1: import { defaultFeature, namedFeature } from "./index";
2: 
3: export function runDefault() {
4:   return defaultFeature();
5: }
6: 
7: export function runNamed() {
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
- path_id: `generated://audit/edge-key:-8968814412825094128`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/cfcf410ad684a71bcd537969a699a56d`
- target: `repo://e/d427c2624c11d7876a35e3e5e0924529`
- relation_sequence: `FLOWS_TO`
- exactness: `parser_verified`
- confidence: `1`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `test_or_fixture_inferred`
- missing_metadata: `metadata_empty, repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/cfcf410ad684a71bcd537969a699a56d -FLOWS_TO-> repo://e/d427c2624c11d7876a35e3e5e0924529`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-8968814412825094128` | `FLOWS_TO` | `head_to_tail` | `parser_verified` | 1 | `false` | `test_or_fixture_inferred` | `test_or_mock` | `` |

### Source Spans

- `test_mock_not_production_call/repo/tests/checkout.test.ts:5-7`

```text
3: import { chargeCard } from "../src/service";
4: 
5: vi.mock("../src/service", () => ({
6:   chargeCard: vi.fn(() => "mocked")
7: }));
8: 
9: describe("checkout", () => {
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
- path_id: `generated://audit/edge-key:-8965697013326455734`
- generated_by_audit: `true`
- task_or_query: `unknown`
- source: `repo://e/5b7a9754c6efd7b41fee9d0d38d24ae2`
- target: `repo://e/3aa0e63195a6247173e37727d85a1d4b`
- relation_sequence: `ASSERTS`
- exactness: `static_heuristic`
- confidence: `0.66`
- derived_provenance_label: `base_or_heuristic_edges`
- context: `test_or_fixture_inferred`
- missing_metadata: `repo_commit_missing`
- summary: `Generated one-edge audit path: repo://e/5b7a9754c6efd7b41fee9d0d38d24ae2 -ASSERTS-> repo://e/3aa0e63195a6247173e37727d85a1d4b`

### Edge List

| Edge | Relation | Direction | Exactness | Confidence | Derived | Context | Fact class | Provenance |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `edge-key:-8965697013326455734` | `ASSERTS` | `head_to_tail` | `static_heuristic` | 0.66 | `false` | `test_or_fixture_inferred` | `test_or_mock` | `` |

### Source Spans

- `test_mock_not_production_call/repo/tests/checkout.test.ts:11-11`

```text
9: describe("checkout", () => {
10:   it("uses a test double for chargeCard", () => {
11:     expect(checkout(5)).toBe("mocked");
12:   });
13: });
```


## Notes

- Classification fields are intentionally blank in markdown for human review.
- PathEvidence rows are sampled from path_evidence when present; when the table is empty, this audit generates one-edge PathEvidence-shaped samples from existing edges without writing them to storage.
- Stored PathEvidence samples used before fallback: 0.
