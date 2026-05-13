# Edge Sample Audit

Database: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/fixture_workers4.sqlite`

Relation filter: `CALLS`

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
- edge_id: `edge-key:-9203119335740638116`
- head: `repo://e/af824afc40aa9bc54b31726d12081117` (`loadPlugin`)
- relation: `CALLS`
- tail: `repo://e/978efa52f9bfd5d3eb88b7cf3839b181` (`import`)
- source_span: `dynamic_import_marked_heuristic/repo/src/loader.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:3e60612ecd044bf9`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-8696642248883918410`
- head: `repo://e/af824afc40aa9bc54b31726d12081117` (`loadPlugin`)
- relation: `CALLS`
- tail: `repo://e/2e736d5395099bde52b33cf6a5c3565c` (`mod.default`)
- source_span: `dynamic_import_marked_heuristic/repo/src/loader.ts:3-3`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:3e60612ecd044bf9`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-8662264075891754807`
- head: `repo://e/7daf5588a140438c270aca6865a43346` (`requireAdmin`)
- relation: `CALLS`
- tail: `repo://e/3d8884f0875f031b8bf4ff01273a3325` (`checkRole`)
- source_span: `admin_user_middleware_role_separation/repo/src/auth.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:6e4fdc2810899c3a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-8179267985489797771`
- head: `repo://e/8c42b628a8dcd354c3ac087c299618fe` (`sanitizeHtml`)
- relation: `CALLS`
- tail: `repo://e/06a8684f41123816068b0252267b9d78` (`value.replace`)
- source_span: `sanitizer_exists_but_not_on_flow/repo/src/sanitize.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:99924f9bb6f3a9af`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function sanitizeHtml(value: string) {
2:   return value.replace(/</g, "&lt;");
3: }
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
- edge_id: `edge-key:-7122978112970569375`
- head: `repo://e/1f89b459da26c0283f1355c5c7b2b612` (`saveComment`)
- relation: `CALLS`
- tail: `repo://e/fa36d2933f8c7addf7c41013d58ec3eb` (`writeComment`)
- source_span: `sanitizer_exists_but_not_on_flow/repo/src/register.ts:6-6`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:19c73fc7fae08363`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
4:   const raw = req.body.comment;
5:   const normalized = raw.trim();
6:   return writeComment(normalized);
7: }
8: 
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
- edge_id: `edge-key:-6090242022326402403`
- head: `repo://e/1f89b459da26c0283f1355c5c7b2b612` (`saveComment`)
- relation: `CALLS`
- tail: `repo://e/f47e57a4a20d8fbacc9507f582953f00` (`raw.trim`)
- source_span: `sanitizer_exists_but_not_on_flow/repo/src/register.ts:5-5`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:19c73fc7fae08363`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-5697957415253836435`
- head: `repo://e/2c59e6b2e0f7105f5edab60a3650c91d` (`run`)
- relation: `CALLS`
- tail: `repo://e/d707437a5a25615240d9c03d38589620` (`selected`)
- source_span: `import_alias_change_updates_target/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:1e9cf1922c903253`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-4831390274733362438`
- head: `repo://e/17d04388e2d4fc14e84f74a37d2b2542` (`handler`)
- relation: `CALLS`
- tail: `repo://e/d0e2ec1facf1378a9adefb308ff9cd30` (`chooseUser`)
- source_span: `same_function_name_only_one_imported/repo/src/main.ts:6-6`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:5a21db5546e843e6`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-4173915550597516060`
- head: `repo://e/d761b66ee661b4b41f607209710d3d2e` (`runDefault`)
- relation: `CALLS`
- tail: `repo://e/38da982d83c293a7e059192abd14c1d1` (`defaultFeature`)
- source_span: `barrel_export_default_export_resolution/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:b78e07e1c1d1325a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-3629816440212955401`
- head: `repo://e/dab42b501d98f20fd7dd491e0d09b2fd` (`submitOrder`)
- relation: `CALLS`
- tail: `repo://e/2d1fd27de58c0dfed8d79b7007fe88d4` (`saveOrder`)
- source_span: `derived_closure_edge_requires_provenance/repo/src/service.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:70fe6892e457bced`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-2540463547328484458`
- head: `repo://e/026e70c08ba2aa164b666205b41ce45c` (`run`)
- relation: `CALLS`
- tail: `repo://e/16a0071e4b453e90aae73f9f1c1bc526` (`first`)
- source_span: `source_span_exact_callsite/repo/src/main.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:c33aed76d826c2bd`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export function run(flag: boolean) {
4:   first();
5:   if (flag) {
6:     second();
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
- edge_id: `edge-key:-2524722484245579099`
- head: `repo://e/f7f5b8bb291ea408f740296b8634313e` (`run`)
- relation: `CALLS`
- tail: `repo://e/e9ec75b240cb492fcee7fa8fc2bdd8e5` (`liveTarget`)
- source_span: `stale_graph_cache_after_edit_delete/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:30f1952f31260f70`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-2075607768432037724`
- head: `repo://e/51fff005ee0ed8b8cd46f751b4c4f2fa` (`requireUser`)
- relation: `CALLS`
- tail: `repo://e/3d8884f0875f031b8bf4ff01273a3325` (`checkRole`)
- source_span: `admin_user_middleware_role_separation/repo/src/auth.ts:6-6`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:6e4fdc2810899c3a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-2074202350736276718`
- head: `repo://e/17d04388e2d4fc14e84f74a37d2b2542` (`handler`)
- relation: `CALLS`
- tail: `repo://e/c2b2bb68bf959c78b87bb8e585a8b156` (`audit`)
- source_span: `same_function_name_only_one_imported/repo/src/main.ts:5-5`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:5a21db5546e843e6`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-1562217487425745418`
- head: `repo://e/af9c034dae17a9c6cb6370af63ddcab0` (`run`)
- relation: `CALLS`
- tail: `repo://e/1188118511f277ff46d31653606c24a1` (`renamedTarget`)
- source_span: `file_rename_prunes_old_path/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:f5dec5972740224e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-1342439232913117547`
- head: `repo://e/026e70c08ba2aa164b666205b41ce45c` (`run`)
- relation: `CALLS`
- tail: `repo://e/3db73fe94b292fc0cf39b4681af3e0c6` (`second`)
- source_span: `source_span_exact_callsite/repo/src/main.ts:6-6`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:c33aed76d826c2bd`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
4:   first();
5:   if (flag) {
6:     second();
7:   }
8: }
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
- edge_id: `edge-key:-1202743239963183456`
- head: `repo://e/a77fa400e86d4626945c0623148a30dc` (`checkout`)
- relation: `CALLS`
- tail: `repo://e/922a2204fe6355d5d3b732f36c808abb` (`chargeCard`)
- source_span: `test_mock_not_production_call/repo/src/checkout.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:55b1ab90b879fc31`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
2: 
3: export function checkout(total: number) {
4:   return chargeCard(total);
5: }
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
- edge_id: `edge-key:-668374398689844391`
- head: `repo://e/a44e599d360a8d33f56937e3b00c9c5d` (`checkRole`)
- relation: `CALLS`
- tail: `repo://e/3af04cb1e484b04ea2e4dc19ee369cb0` (`user.roles.includes`)
- source_span: `admin_user_middleware_role_separation/repo/src/auth.ts:10-10`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:6e4fdc2810899c3a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
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
- edge_id: `edge-key:-401414492922706285`
- head: `repo://e/24c4a7f913965abcf707618e6e42286e` (`runNamed`)
- relation: `CALLS`
- tail: `repo://e/7806e73a1c8539772a0cf717c5963789` (`namedFeature`)
- source_span: `barrel_export_default_export_resolution/repo/src/use.ts:8-8`
- relation_direction: `head_to_tail`
- exactness: `static_heuristic`
- confidence: `0.55`
- repo_commit: `unknown`
- file_hash: `fnv64:b78e07e1c1d1325a`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_heuristic`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
6: 
7: export function runNamed() {
8:   return namedFeature();
9: }
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
- edge_id: `edge://12200ba7e3282f58751724ead67a4a92`
- head: `repo://e/24c4a7f913965abcf707618e6e42286e` (`runNamed`)
- relation: `CALLS`
- tail: `repo://e/b79a25df7113fd72a75d77faab211438` (`feature`)
- source_span: `barrel_export_default_export_resolution/repo/src/use.ts:8-8`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b78e07e1c1d1325a`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
6: 
7: export function runNamed() {
8:   return namedFeature();
9: }
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
- edge_id: `edge://3f647be31db88b514e20f120eff2133f`
- head: `repo://e/17d04388e2d4fc14e84f74a37d2b2542` (`handler`)
- relation: `CALLS`
- tail: `repo://e/ae8cb7e8a3c7549ff1e6c1c799ab7d65` (`audit`)
- source_span: `same_function_name_only_one_imported/repo/src/main.ts:5-5`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5a21db5546e843e6`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
3: 
4: export function handler(id: string) {
5:   audit(id);
6:   return chooseUser(id);
7: }
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
- edge_id: `edge://5fe8cf8b0e910d8de4787d002e2324ef`
- head: `repo://e/17d04388e2d4fc14e84f74a37d2b2542` (`handler`)
- relation: `CALLS`
- tail: `repo://e/0570030f46c7d0bfeb2dd40aa28332ad` (`chooseUser`)
- source_span: `same_function_name_only_one_imported/repo/src/main.ts:6-6`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:5a21db5546e843e6`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
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
- edge_id: `edge://75221d107a591d340593138ea77be632`
- head: `repo://e/a77fa400e86d4626945c0623148a30dc` (`checkout`)
- relation: `CALLS`
- tail: `repo://e/8587fc0d9c811a8d0afd1a42eb291687` (`chargeCard`)
- source_span: `test_mock_not_production_call/repo/src/checkout.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:55b1ab90b879fc31`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
2: 
3: export function checkout(total: number) {
4:   return chargeCard(total);
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
- edge_id: `edge://9b68e7cc497dadbc640e6a51d1ec88da`
- head: `repo://e/f7f5b8bb291ea408f740296b8634313e` (`run`)
- relation: `CALLS`
- tail: `repo://e/f65ac73af13dce69a7720deae1cb94e3` (`liveTarget`)
- source_span: `stale_graph_cache_after_edit_delete/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:30f1952f31260f70`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
2: 
3: export function run() {
4:   return liveTarget();
5: }
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
- edge_id: `edge://a1535acbeacf4155df648463bdbd2e4f`
- head: `repo://e/2c59e6b2e0f7105f5edab60a3650c91d` (`run`)
- relation: `CALLS`
- tail: `repo://e/28b6aa60f7fdef6657975eee462fc940` (`alphaTarget`)
- source_span: `import_alias_change_updates_target/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:1e9cf1922c903253`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
2: 
3: export function run() {
4:   return selected();
5: }
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
- edge_id: `edge://aa9bc1c26433df231810f2923a3d4545`
- head: `repo://e/af9c034dae17a9c6cb6370af63ddcab0` (`run`)
- relation: `CALLS`
- tail: `repo://e/4eb1c532ec9e2c673220f981de546f7d` (`renamedTarget`)
- source_span: `file_rename_prunes_old_path/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:f5dec5972740224e`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
2: 
3: export function run() {
4:   return renamedTarget();
5: }
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
- edge_id: `edge://c592b76791d136e522e70edff7056ef3`
- head: `repo://e/d761b66ee661b4b41f607209710d3d2e` (`runDefault`)
- relation: `CALLS`
- tail: `repo://e/67fb7868db0c8a412bede37dfd847cd7` (`default`)
- source_span: `barrel_export_default_export_resolution/repo/src/use.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:b78e07e1c1d1325a`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
2: 
3: export function runDefault() {
4:   return defaultFeature();
5: }
6: 
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
- edge_id: `edge://cfab76bb0580290e6a2f79ff675b2b50`
- head: `repo://e/026e70c08ba2aa164b666205b41ce45c` (`run`)
- relation: `CALLS`
- tail: `repo://e/d2613e83348d21098fdbd85ebc73d087` (`second`)
- source_span: `source_span_exact_callsite/repo/src/main.ts:6-6`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:c33aed76d826c2bd`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
4:   first();
5:   if (flag) {
6:     second();
7:   }
8: }
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
- edge_id: `edge://d691f242ee0f318408f475bc7997882e`
- head: `repo://e/dab42b501d98f20fd7dd491e0d09b2fd` (`submitOrder`)
- relation: `CALLS`
- tail: `repo://e/2d245887cb4eb027b3601e090eb8ebc1` (`saveOrder`)
- source_span: `derived_closure_edge_requires_provenance/repo/src/service.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:70fe6892e457bced`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
2: 
3: export function submitOrder(order: any) {
4:   return saveOrder(order);
5: }
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
- edge_id: `edge://fbd5ddd96d214c416810511f85c63f33`
- head: `repo://e/026e70c08ba2aa164b666205b41ce45c` (`run`)
- relation: `CALLS`
- tail: `repo://e/cf508bd2fa95d71741c19aa9aa67a60d` (`first`)
- source_span: `source_span_exact_callsite/repo/src/main.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:c33aed76d826c2bd`
- derived: `false`
- extractor: `codegraph-index-static-import-resolver`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing`
- span_loaded: `true`

```text
2: 
3: export function run(flag: boolean) {
4:   first();
5:   if (flag) {
6:     second();
```

