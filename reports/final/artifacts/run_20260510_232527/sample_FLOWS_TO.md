# Edge Sample Audit

Database: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/fixture_workers4.sqlite`

Relation filter: `FLOWS_TO`

Limit: `50`

Seed: `17`

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
- edge_id: `edge-key:-8968814412825094128`
- head: `repo://e/cfcf410ad684a71bcd537969a699a56d` (`expr@167`)
- relation: `FLOWS_TO`
- tail: `repo://e/d427c2624c11d7876a35e3e5e0924529` (`argument_1@167`)
- source_span: `test_mock_not_production_call/repo/tests/checkout.test.ts:5-7`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b64b0c12323d4acf`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
3: import { chargeCard } from "../src/service";
4: 
5: vi.mock("../src/service", () => ({
6:   chargeCard: vi.fn(() => "mocked")
7: }));
8: 
9: describe("checkout", () => {
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
- edge_id: `edge-key:-8932595346684089157`
- head: `repo://e/a7598a228d6833f626b14e028ae7f2dc` (`expr@196`)
- relation: `FLOWS_TO`
- tail: `repo://e/bf932aed1d28e7680df8cc5d0e6dfce6` (`argument_0@196`)
- source_span: `test_mock_not_production_call/repo/tests/checkout.test.ts:6-6`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b64b0c12323d4acf`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
4: 
5: vi.mock("../src/service", () => ({
6:   chargeCard: vi.fn(() => "mocked")
7: }));
8: 
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
- edge_id: `edge-key:-8931892587166734238`
- head: `repo://e/bd52616095175547fc2cb33f53bb3659` (`adminPanel`)
- relation: `FLOWS_TO`
- tail: `repo://e/31348e45e1d3e5cf2be36c2993909f9d` (`argument_3@116`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:3-3`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: import { requireAdmin, requireUser } from "./auth";
2: 
3: export const adminRoute = route("GET", "/admin", requireAdmin, adminPanel);
4: export const userRoute = route("GET", "/account", requireUser, userHome);
5: 
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
- edge_id: `edge-key:-8882367084918076338`
- head: `repo://e/876c9247ff409210d48f13d2094a2c46` (`expr@27`)
- relation: `FLOWS_TO`
- tail: `repo://e/db9622c37041311dc1c332c3b6af550b` (`ordersTable`)
- source_span: `derived_closure_edge_requires_provenance/repo/src/store.ts:1-1`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:775c8dd6ebfb6f14`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export const ordersTable = "orders";
2: 
3: export function saveOrder(order: any) {
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
- edge_id: `edge-key:-8862595974082022165`
- head: `repo://e/dbeca7802a844b642692a52eceeaba42` (`expr@252`)
- relation: `FLOWS_TO`
- tail: `repo://e/20521b048eda9f1074d2c2bfe49956c6` (`argument_0@252`)
- source_span: `test_mock_not_production_call/repo/tests/checkout.test.ts:10-10`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b64b0c12323d4acf`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
8: 
9: describe("checkout", () => {
10:   it("uses a test double for chargeCard", () => {
11:     expect(checkout(5)).toBe("mocked");
12:   });
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
- edge_id: `edge-key:-8701454121290662982`
- head: `repo://e/0e48c5246aaba7401cdac81450c04432` (`user`)
- relation: `FLOWS_TO`
- tail: `repo://e/551e6ab6d6f9f7043452b47f853e0d16` (`argument_0@61`)
- source_span: `admin_user_middleware_role_separation/repo/src/auth.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6e4fdc2810899c3a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function requireAdmin(user: any) {
2:   return checkRole(user, "admin");
3: }
4: 
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
- edge_id: `edge-key:-8523535827599030407`
- head: `repo://e/1b7f758538140d13fa92c916d78e3df1` (`expr@71`)
- relation: `FLOWS_TO`
- tail: `repo://e/16d14006b658c80188df1cce358096db` (`return@64`)
- source_span: `stale_graph_cache_after_edit_delete/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:30f1952f31260f70`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export function run() {
4:   return liveTarget();
5: }
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
- edge_id: `edge-key:-8514931106740183051`
- head: `repo://e/608cd4da8964e24f3a0756ae8c37c8ad` (`expr@76`)
- relation: `FLOWS_TO`
- tail: `repo://e/86800f842ed2c385caaf12ad19952fbb` (`argument_0@76`)
- source_span: `dynamic_import_marked_heuristic/repo/src/loader.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3e60612ecd044bf9`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export async function loadPlugin(name: string) {
2:   const mod = await import("./plugins/" + name);
3:   return mod.default();
4: }
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
- edge_id: `edge-key:-8505375487769567490`
- head: `repo://e/0d43fcd6c7b55a3cc6412e90a535b592` (`expr@239`)
- relation: `FLOWS_TO`
- tail: `repo://e/cc7256b59aacdc9865071e9295a55bd6` (`argument_1@239`)
- source_span: `test_mock_not_production_call/repo/tests/checkout.test.ts:9-13`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b64b0c12323d4acf`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
7: }));
8: 
9: describe("checkout", () => {
10:   it("uses a test double for chargeCard", () => {
11:     expect(checkout(5)).toBe("mocked");
12:   });
13: });
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
- edge_id: `edge-key:-8280935406507169843`
- head: `repo://e/986197ad1e1935c6f98d4f25eb47c0b4` (`expr@107`)
- relation: `FLOWS_TO`
- tail: `repo://e/db10bb46ffcdf70cdc32e29c5f0dff3a` (`return@100`)
- source_span: `dynamic_import_marked_heuristic/repo/src/loader.ts:3-3`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3e60612ecd044bf9`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export async function loadPlugin(name: string) {
2:   const mod = await import("./plugins/" + name);
3:   return mod.default();
4: }
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
- edge_id: `edge-key:-8193059087951499845`
- head: `repo://e/3aa0e63195a6247173e37727d85a1d4b` (`expr@308`)
- relation: `FLOWS_TO`
- tail: `repo://e/7e0cd6e33b651d25d9a9e543668d53ef` (`argument_0@308`)
- source_span: `test_mock_not_production_call/repo/tests/checkout.test.ts:11-11`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b64b0c12323d4acf`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
9: describe("checkout", () => {
10:   it("uses a test double for chargeCard", () => {
11:     expect(checkout(5)).toBe("mocked");
12:   });
13: });
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
- edge_id: `edge-key:-7616031965667234723`
- head: `repo://e/da41133289e67c007d3f1b9ff6daf382` (`expr@317`)
- relation: `FLOWS_TO`
- tail: `repo://e/7e010abf0fdc1bbcef0c757ed6636eae` (`argument_0@317`)
- source_span: `test_mock_not_production_call/repo/tests/checkout.test.ts:11-11`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b64b0c12323d4acf`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
9: describe("checkout", () => {
10:   it("uses a test double for chargeCard", () => {
11:     expect(checkout(5)).toBe("mocked");
12:   });
13: });
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
- edge_id: `edge-key:-7591096624008635989`
- head: `repo://e/4b683281f40b14100389da1cad083d26` (`expr@77`)
- relation: `FLOWS_TO`
- tail: `repo://e/2d809a952545e3880db7a73cf0953d0e` (`return@70`)
- source_span: `file_rename_prunes_old_path/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:f5dec5972740224e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export function run() {
4:   return renamedTarget();
5: }
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
- edge_id: `edge-key:-7412905055993568993`
- head: `repo://e/23a1c8be735c27b5cb755ba1ba662ae7` (`comment`)
- relation: `FLOWS_TO`
- tail: `repo://e/415893b95d59e69b51783bc94149dc95` (`return@237`)
- source_span: `sanitizer_exists_but_not_on_flow/repo/src/register.ts:10-10`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:19c73fc7fae08363`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
8: 
9: export function writeComment(comment: string) {
10:   return comment;
11: }
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
- edge_id: `edge-key:-7069541534056199184`
- head: `repo://e/5d38f9a6da6f47f8ad4e3db6466c3b12` (`expr@289`)
- relation: `FLOWS_TO`
- tail: `repo://e/22cdfee577f4f3a6423373530c4101e8` (`argument_1@289`)
- source_span: `test_mock_not_production_call/repo/tests/checkout.test.ts:10-12`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b64b0c12323d4acf`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
8: 
9: describe("checkout", () => {
10:   it("uses a test double for chargeCard", () => {
11:     expect(checkout(5)).toBe("mocked");
12:   });
13: });
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
- edge_id: `edge-key:-6997957043352048496`
- head: `repo://e/fe17fd24045c96ace8a605d4283e4896` (`expr@403`)
- relation: `FLOWS_TO`
- tail: `repo://e/988ef875e412095cfc8684f9e3c9130e` (`return@396`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:10-10`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
8: }
9: function adminPanel() { return "admin"; }
10: function userHome() { return "user"; }
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
- edge_id: `edge-key:-6934739664744838281`
- head: `repo://e/4066baad0aa1e6c3e87ceb2c56d942bd` (`userHome`)
- relation: `FLOWS_TO`
- tail: `repo://e/16d08340fb8fca0b57c4d02d15780015` (`argument_3@192`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export const adminRoute = route("GET", "/admin", requireAdmin, adminPanel);
4: export const userRoute = route("GET", "/account", requireUser, userHome);
5: 
6: function route(method: string, path: string, guard: Function, handler: Function) {
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
- edge_id: `edge-key:-6810267495257665014`
- head: `repo://e/4624ebecc2902fac3622d514e0cc411a` (`expr@40`)
- relation: `FLOWS_TO`
- tail: `repo://e/9a9227bd63fc5f5ce22fcc7667aedebe` (`return@33`)
- source_span: `import_alias_change_updates_target/repo/src/beta.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:c68a1c5437ceecae`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function betaTarget() {
2:   return "beta";
3: }
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
- edge_id: `edge-key:-6706365188965007484`
- head: `repo://e/c63e151a48b32d66da87e14588b4109c` (`expr@43`)
- relation: `FLOWS_TO`
- tail: `repo://e/7fc2a160871ef1a8a3dcbf895ce8fdbe` (`return@36`)
- source_span: `file_rename_prunes_old_path/repo/src/oldName.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:15b4bd294bdb44a1`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function renamedTarget() {
2:   return "old";
3: }
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
- edge_id: `edge-key:-6494554155358846890`
- head: `repo://e/3619d607c79eea97f8cfbbf03781fe09` (`expr@85`)
- relation: `FLOWS_TO`
- tail: `repo://e/b8d04e0b99b4ccbf72d0c204a8771bd1` (`argument_0@85`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:3-3`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: import { requireAdmin, requireUser } from "./auth";
2: 
3: export const adminRoute = route("GET", "/admin", requireAdmin, adminPanel);
4: export const userRoute = route("GET", "/account", requireUser, userHome);
5: 
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
- edge_id: `edge-key:-6441590203583740025`
- head: `repo://e/24b93934c40cf00781d70d0bd9869b31` (`expr@167`)
- relation: `FLOWS_TO`
- tail: `repo://e/7eb81e0654046b7bcc46d60f27ef3855` (`argument_1@167`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export const adminRoute = route("GET", "/admin", requireAdmin, adminPanel);
4: export const userRoute = route("GET", "/account", requireUser, userHome);
5: 
6: function route(method: string, path: string, guard: Function, handler: Function) {
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
- edge_id: `edge-key:-6437636834494543462`
- head: `repo://e/54496b38ba7d6a9bdb39c05b89dad921` (`id`)
- relation: `FLOWS_TO`
- tail: `repo://e/406af35f71be914057be10f3ff9d4862` (`argument_0@114`)
- source_span: `same_function_name_only_one_imported/repo/src/main.ts:5-5`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5a21db5546e843e6`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
3: 
4: export function handler(id: string) {
5:   audit(id);
6:   return chooseUser(id);
7: }
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
- edge_id: `edge-key:-6396346272525705294`
- head: `repo://e/0dae406bf90fd2e62dd89c179e4ea7d0` (`expr@85`)
- relation: `FLOWS_TO`
- tail: `repo://e/fcafd47d222168ac3b17f82495f97282` (`return@78`)
- source_span: `import_alias_change_updates_target/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:1e9cf1922c903253`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export function run() {
4:   return selected();
5: }
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
- edge_id: `edge-key:-6160310627128939728`
- head: `repo://e/466babe8a2ba69317ac6749988df63b7` (`expr@41`)
- relation: `FLOWS_TO`
- tail: `repo://e/dc44eefa6d2000736049d53ac97583c1` (`return@34`)
- source_span: `import_alias_change_updates_target/repo/src/alpha.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:92fa0580bd67d218`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function alphaTarget() {
2:   return "alpha";
3: }
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
- edge_id: `edge-key:-6087773232331521904`
- head: `repo://e/30aef0a703220d75467c766864b4efff` (`expr@363`)
- relation: `FLOWS_TO`
- tail: `repo://e/57c54cfed41ad0a40bce2034d3a6f8a2` (`return@356`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:9-9`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
7:   return { method, path, guard, handler };
8: }
9: function adminPanel() { return "admin"; }
10: function userHome() { return "user"; }
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
- edge_id: `edge-key:-5954688276726876742`
- head: `repo://e/653fe976f2ad7a9b70a811d3d144f765` (`requireAdmin`)
- relation: `FLOWS_TO`
- tail: `repo://e/83eb7e21230dcd81cc529aa09a51791f` (`argument_2@102`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:3-3`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: import { requireAdmin, requireUser } from "./auth";
2: 
3: export const adminRoute = route("GET", "/admin", requireAdmin, adminPanel);
4: export const userRoute = route("GET", "/account", requireUser, userHome);
5: 
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
- edge_id: `edge-key:-5604131598595792980`
- head: `repo://e/f2f62519afd7dc4ddea1a41af88e8b23` (`expr@43`)
- relation: `FLOWS_TO`
- tail: `repo://e/e4eea2c2aa9fd823759f27433c544249` (`return@36`)
- source_span: `dynamic_import_marked_heuristic/repo/src/plugins/alpha.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:e04673e671ebeb76`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export default function alpha() {
2:   return "alpha";
3: }
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
- edge_id: `edge-key:-5551799344325528117`
- head: `repo://e/26d36f18dc869ba1ff4a3c12be001adf` (`expr@160`)
- relation: `FLOWS_TO`
- tail: `repo://e/5ed0c96e31040a2502b309f1bef386a3` (`argument_0@160`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export const adminRoute = route("GET", "/admin", requireAdmin, adminPanel);
4: export const userRoute = route("GET", "/account", requireUser, userHome);
5: 
6: function route(method: string, path: string, guard: Function, handler: Function) {
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
- edge_id: `edge-key:-5348629650215903429`
- head: `repo://e/44d8596f10b6a5ed8286de6f1897d61f` (`expr@130`)
- relation: `FLOWS_TO`
- tail: `repo://e/6a74682889eab28dc7f2e0f88178020f` (`return@123`)
- source_span: `admin_user_middleware_role_separation/repo/src/auth.ts:6-6`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6e4fdc2810899c3a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
4: 
5: export function requireUser(user: any) {
6:   return checkRole(user, "user");
7: }
8: 
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
- edge_id: `edge-key:-5320812356318544599`
- head: `repo://e/3b714a4d9b9a7158fbdeca066c9ddf52` (`expr@128`)
- relation: `FLOWS_TO`
- tail: `repo://e/c60132ec231d8bdcf1f81d3a35609eba` (`return@121`)
- source_span: `same_function_name_only_one_imported/repo/src/main.ts:6-6`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5a21db5546e843e6`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
4: export function handler(id: string) {
5:   audit(id);
6:   return chooseUser(id);
7: }
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
- edge_id: `edge-key:-5140489319949848871`
- head: `repo://e/6c16fc15c2546ee679a0eab5975b3f64` (`expr@63`)
- relation: `FLOWS_TO`
- tail: `repo://e/a24e6d8368307ba3b83fd00e9b292759` (`mod`)
- source_span: `dynamic_import_marked_heuristic/repo/src/loader.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:3e60612ecd044bf9`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export async function loadPlugin(name: string) {
2:   const mod = await import("./plugins/" + name);
3:   return mod.default();
4: }
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
- edge_id: `edge-key:-5126588501475850851`
- head: `repo://e/5699ea75123aa1779a1b7ff62f7c5b31` (`user`)
- relation: `FLOWS_TO`
- tail: `repo://e/ba0fd9b8e7a6def43ca277a30c912166` (`argument_0@140`)
- source_span: `admin_user_middleware_role_separation/repo/src/auth.ts:6-6`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6e4fdc2810899c3a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
4: 
5: export function requireUser(user: any) {
6:   return checkRole(user, "user");
7: }
8: 
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
- edge_id: `edge-key:-5083306148284553816`
- head: `repo://e/5f53961b108e4e5101779aa985511943` (`expr@146`)
- relation: `FLOWS_TO`
- tail: `repo://e/8e96c639a44258810b65b5a4b6da5e23` (`argument_1@146`)
- source_span: `admin_user_middleware_role_separation/repo/src/auth.ts:6-6`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6e4fdc2810899c3a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
4: 
5: export function requireUser(user: any) {
6:   return checkRole(user, "user");
7: }
8: 
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
- edge_id: `edge-key:-5024586843951785344`
- head: `repo://e/9c3cf8e3a636b4a69eb88638eef0d66c` (`expr@102`)
- relation: `FLOWS_TO`
- tail: `repo://e/49f0d2e7f224b22c0a5a39e7cb800d4e` (`return@95`)
- source_span: `derived_closure_edge_requires_provenance/repo/src/service.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:70fe6892e457bced`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export function submitOrder(order: any) {
4:   return saveOrder(order);
5: }
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
- edge_id: `edge-key:-4985521945923698675`
- head: `repo://e/07c38dd88a5532c92cfb2d7e4c2ecc0f` (`order`)
- relation: `FLOWS_TO`
- tail: `repo://e/1c5dbd149f7fa4e6cb96515e5f61b7ec` (`argument_0@112`)
- source_span: `derived_closure_edge_requires_provenance/repo/src/service.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:70fe6892e457bced`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export function submitOrder(order: any) {
4:   return saveOrder(order);
5: }
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
- edge_id: `edge-key:-4922606014487661581`
- head: `repo://e/f6fd602418bc57fe824e4d1c308726d0` (`expr@67`)
- relation: `FLOWS_TO`
- tail: `repo://e/e9cab248724971f8e04da6ddc10e851a` (`argument_1@67`)
- source_span: `admin_user_middleware_role_separation/repo/src/auth.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6e4fdc2810899c3a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function requireAdmin(user: any) {
2:   return checkRole(user, "admin");
3: }
4: 
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
- edge_id: `edge-key:-4921715565400873968`
- head: `repo://e/8c0907c4fdb4fc0fee66242a5044aeed` (`expr@230`)
- relation: `FLOWS_TO`
- tail: `repo://e/295cb83e851bfeb99c6caa36c4e65faf` (`return@223`)
- source_span: `admin_user_middleware_role_separation/repo/src/auth.ts:10-10`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6e4fdc2810899c3a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
8: 
9: export function checkRole(user: any, role: "admin" | "user") {
10:   return user.roles.includes(role);
11: }
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
- edge_id: `edge-key:-4629578602212942928`
- head: `repo://e/238e88b05a371346138698d105d522ac` (`expr@92`)
- relation: `FLOWS_TO`
- tail: `repo://e/15d2e8fc0dc096f6b92b3d14f796167c` (`argument_1@92`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:3-3`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: import { requireAdmin, requireUser } from "./auth";
2: 
3: export const adminRoute = route("GET", "/admin", requireAdmin, adminPanel);
4: export const userRoute = route("GET", "/account", requireUser, userHome);
5: 
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
- edge_id: `edge-key:-4576367243493334573`
- head: `repo://e/363f254f7673dce3b1b89972aa5126d5` (`requireUser`)
- relation: `FLOWS_TO`
- tail: `repo://e/96d71477bb7a4bdaa4c78f09296a6560` (`argument_2@179`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export const adminRoute = route("GET", "/admin", requireAdmin, adminPanel);
4: export const userRoute = route("GET", "/account", requireUser, userHome);
5: 
6: function route(method: string, path: string, guard: Function, handler: Function) {
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
- edge_id: `edge-key:-4458932643606904775`
- head: `repo://e/aeb302509e6a71c80fc7381020c66c8a` (`expr@227`)
- relation: `FLOWS_TO`
- tail: `repo://e/838bfc1a36be1c88728131560c9a25ba` (`argument_0@227`)
- source_span: `test_mock_not_production_call/repo/tests/checkout.test.ts:9-9`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b64b0c12323d4acf`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
7: }));
8: 
9: describe("checkout", () => {
10:   it("uses a test double for chargeCard", () => {
11:     expect(checkout(5)).toBe("mocked");
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
- edge_id: `edge-key:-4250109911766913739`
- head: `repo://e/c80c1a529bfe70994a226ea5e00e5ba7` (`expr@40`)
- relation: `FLOWS_TO`
- tail: `repo://e/9bc7659f245907e9ec5a852a446f525b` (`return@33`)
- source_span: `stale_graph_cache_after_edit_delete/repo/src/live.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:29f7b8cf890f5296`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function liveTarget() {
2:   return "live";
3: }
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
- edge_id: `edge-key:-4155883514244134927`
- head: `repo://e/d72b2584a91a474b21d8984274bbfb05` (`expr@296`)
- relation: `FLOWS_TO`
- tail: `repo://e/f9845aed6baf9bdb980ad5dedec0b1b9` (`return@289`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:7-7`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
5: 
6: function route(method: string, path: string, guard: Function, handler: Function) {
7:   return { method, path, guard, handler };
8: }
9: function adminPanel() { return "admin"; }
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
- edge_id: `edge-key:-4026250569370077681`
- head: `repo://e/4366f062148d5a2e3e7fa1638ff5bb24` (`expr@50`)
- relation: `FLOWS_TO`
- tail: `repo://e/0cf32c78426c163d2d92ecc618c7354b` (`return@43`)
- source_span: `same_function_name_only_one_imported/repo/src/a.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:351153d29e763283`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function chooseUser(id: string) {
2:   return `A:${id}`;
3: }
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
- edge_id: `edge-key:-3974071443315218837`
- head: `repo://e/b73ff1bf7a79df8da133bd5fb89285cf` (`expr@97`)
- relation: `FLOWS_TO`
- tail: `repo://e/1d4c0b08b1265befc997ad10928b1405` (`return@90`)
- source_span: `barrel_export_default_export_resolution/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b78e07e1c1d1325a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export function runDefault() {
4:   return defaultFeature();
5: }
6: 
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
- edge_id: `edge-key:-3703913481392354043`
- head: `repo://e/fe977969441065dd3d031b6084d02d4f` (`expr@79`)
- relation: `FLOWS_TO`
- tail: `repo://e/d4bc193dfd49a94711c3f8688d0af605` (`adminRoute`)
- source_span: `admin_user_middleware_role_separation/repo/src/routes.ts:3-3`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5566c6c9c44b2095`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: import { requireAdmin, requireUser } from "./auth";
2: 
3: export const adminRoute = route("GET", "/admin", requireAdmin, adminPanel);
4: export const userRoute = route("GET", "/account", requireUser, userHome);
5: 
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
- edge_id: `edge-key:-3325242497072510788`
- head: `repo://e/5dee5fbc1449c3852e32e3b7f462ad13` (`expr@137`)
- relation: `FLOWS_TO`
- tail: `repo://e/6231fc5dca04885693d7ef9c5c0e88c4` (`normalized`)
- source_span: `sanitizer_exists_but_not_on_flow/repo/src/register.ts:5-5`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:19c73fc7fae08363`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
3: export function saveComment(req: any) {
4:   const raw = req.body.comment;
5:   const normalized = raw.trim();
6:   return writeComment(normalized);
7: }
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
- edge_id: `edge-key:-3300763334872427753`
- head: `repo://e/1d156092518882c83332b0db85b1dd0a` (`expr@98`)
- relation: `FLOWS_TO`
- tail: `repo://e/7725451ed037301988efdd7f414beb63` (`raw`)
- source_span: `sanitizer_exists_but_not_on_flow/repo/src/register.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:19c73fc7fae08363`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export function saveComment(req: any) {
4:   const raw = req.body.comment;
5:   const normalized = raw.trim();
6:   return writeComment(normalized);
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
- edge_id: `edge-key:-2914225770881291994`
- head: `repo://e/b420a0b9a264a0909d6d514f5e078862` (`role`)
- relation: `FLOWS_TO`
- tail: `repo://e/79aabe9eabf9640e100aea8b153bde90` (`argument_0@250`)
- source_span: `admin_user_middleware_role_separation/repo/src/auth.ts:10-10`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:6e4fdc2810899c3a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
8: 
9: export function checkRole(user: any, role: "admin" | "user") {
10:   return user.roles.includes(role);
11: }
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
- edge_id: `edge-key:-2830140751435055111`
- head: `repo://e/a8e9edf05547719f50602a3c545b8cbd` (`expr@45`)
- relation: `FLOWS_TO`
- tail: `repo://e/7c56fca29fd783007c05fc69a71f622e` (`return@38`)
- source_span: `barrel_export_default_export_resolution/repo/src/defaultFeature.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:45613ff73fc1dfb3`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export default function feature() {
2:   return "default";
3: }
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
- edge_id: `edge-key:-2735513688193329234`
- head: `repo://e/b85fe404894bd92b7d06adae739a3d35` (`expr@50`)
- relation: `FLOWS_TO`
- tail: `repo://e/e7ec13c8cf6bf05eea04eaf06830eb10` (`return@43`)
- source_span: `same_function_name_only_one_imported/repo/src/b.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:361d3481c248e618`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function chooseUser(id: string) {
2:   return `B:${id}`;
3: }
```

