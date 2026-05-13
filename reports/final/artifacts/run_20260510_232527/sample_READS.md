# Edge Sample Audit

Database: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/fixture_workers4.sqlite`

Relation filter: `READS`

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
- edge_id: `edge-key:-8951574106955898787`
- head: `repo://e/7daf5588a140438c270aca6865a43346` (`requireAdmin`)
- relation: `READS`
- tail: `repo://e/094291a4f7d50fdbc8b8cc3ea5abbd79` (`checkRole`)
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
- edge_id: `edge-key:-8830330131378442026`
- head: `repo://e/a44e599d360a8d33f56937e3b00c9c5d` (`checkRole`)
- relation: `READS`
- tail: `repo://e/b420a0b9a264a0909d6d514f5e078862` (`role`)
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
- edge_id: `edge-key:-7498651423641075307`
- head: `repo://e/af824afc40aa9bc54b31726d12081117` (`loadPlugin`)
- relation: `READS`
- tail: `repo://e/a24e6d8368307ba3b83fd00e9b292759` (`mod`)
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
- edge_id: `edge-key:-7085950704099197853`
- head: `repo://e/8587fc0d9c811a8d0afd1a42eb291687` (`chargeCard`)
- relation: `READS`
- tail: `repo://e/aebf94feb6411da12b258d25c510ea07` (`total`)
- source_span: `test_mock_not_production_call/repo/src/service.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:f5bfd52a349b646e`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `test_or_mock`
- context: `test_or_fixture_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function chargeCard(total: number) {
2:   return `charged:${total}`;
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
- edge_id: `edge-key:-6773604984353227930`
- head: `repo://e/ae8cb7e8a3c7549ff1e6c1c799ab7d65` (`audit`)
- relation: `READS`
- tail: `repo://e/2d9d3632255a706923b28781739d78cf` (`id`)
- source_span: `same_function_name_only_one_imported/repo/src/audit.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:f1b5f737f0036e72`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function audit(id: string) {
2:   return `audit:${id}`;
3: }
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
- edge_id: `edge-key:-6387565098441156913`
- head: `repo://e/2c59e6b2e0f7105f5edab60a3650c91d` (`run`)
- relation: `READS`
- tail: `repo://e/52d1d3d02e8ef0ee8fb9e75858f22098` (`selected`)
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
- edge_id: `edge-key:-6055961913509234733`
- head: `repo://e/7daf5588a140438c270aca6865a43346` (`requireAdmin`)
- relation: `READS`
- tail: `repo://e/0e48c5246aaba7401cdac81450c04432` (`user`)
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
- edge_id: `edge-key:-5430917370423224198`
- head: `repo://e/8c42b628a8dcd354c3ac087c299618fe` (`sanitizeHtml`)
- relation: `READS`
- tail: `repo://e/2ce175c0d631a565eb15a612f0ba2eaf` (`value`)
- source_span: `sanitizer_exists_but_not_on_flow/repo/src/sanitize.ts:2-2`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
- repo_commit: `unknown`
- file_hash: `fnv64:99924f9bb6f3a9af`
- derived: `false`
- extractor: `tree-sitter-basic`
- fact_classification: `base_exact`
- context: `production_inferred`
- provenance_edges: ``
- missing_metadata: `repo_commit_missing, metadata_empty`
- span_loaded: `true`

```text
1: export function sanitizeHtml(value: string) {
2:   return value.replace(/</g, "&lt;");
3: }
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
- edge_id: `edge-key:-4909498656739122022`
- head: `repo://e/51fff005ee0ed8b8cd46f751b4c4f2fa` (`requireUser`)
- relation: `READS`
- tail: `repo://e/b1c6e189126bca833f07ed6a50f5fb71` (`checkRole`)
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
- edge_id: `edge-key:-4788022195110301196`
- head: `repo://e/1f89b459da26c0283f1355c5c7b2b612` (`saveComment`)
- relation: `READS`
- tail: `repo://e/b55ea67b7846ebb44e562bc12a9dc806` (`req`)
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
- edge_id: `edge-key:-4616353073752889738`
- head: `repo://e/1f89b459da26c0283f1355c5c7b2b612` (`saveComment`)
- relation: `READS`
- tail: `repo://e/f4457197e2c1f1f9986134cd215e9ee3` (`writeComment`)
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
- edge_id: `edge-key:-3945652436233745553`
- head: `repo://e/dab42b501d98f20fd7dd491e0d09b2fd` (`submitOrder`)
- relation: `READS`
- tail: `repo://e/07c38dd88a5532c92cfb2d7e4c2ecc0f` (`order`)
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
- edge_id: `edge-key:-3880587309966665884`
- head: `repo://e/af824afc40aa9bc54b31726d12081117` (`loadPlugin`)
- relation: `READS`
- tail: `repo://e/e678885a1a134ee45d3e7cc7644e209a` (`name`)
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
- edge_id: `edge-key:-3725722781358078594`
- head: `repo://e/d761b66ee661b4b41f607209710d3d2e` (`runDefault`)
- relation: `READS`
- tail: `repo://e/8263732118a61e1112c3cc68fd9afc97` (`defaultFeature`)
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
- edge_id: `edge-key:-3225699846625946388`
- head: `repo://e/a77fa400e86d4626945c0623148a30dc` (`checkout`)
- relation: `READS`
- tail: `repo://e/b0327ed51694247f769823795ebe4b81` (`chargeCard`)
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
- edge_id: `edge-key:-2836162608334653390`
- head: `repo://e/1f89b459da26c0283f1355c5c7b2b612` (`saveComment`)
- relation: `READS`
- tail: `repo://e/7725451ed037301988efdd7f414beb63` (`raw`)
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
- edge_id: `edge-key:-2825872457515553417`
- head: `repo://e/6e9f5b89cc0c2d9ea7dd6052c40f3cd8` (`chooseUser`)
- relation: `READS`
- tail: `repo://e/261ec05113e924ba103807e71a6ae4bc` (`id`)
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
- edge_id: `edge-key:-2822560138677879045`
- head: `repo://e/24c4a7f913965abcf707618e6e42286e` (`runNamed`)
- relation: `READS`
- tail: `repo://e/d95cf2fa95591ad14f497a0419e0daf7` (`namedFeature`)
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
- edge_id: `edge-key:-2772153859073977003`
- head: `repo://e/17d04388e2d4fc14e84f74a37d2b2542` (`handler`)
- relation: `READS`
- tail: `repo://e/54496b38ba7d6a9bdb39c05b89dad921` (`id`)
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
- edge_id: `edge-key:-2564217349532046620`
- head: `repo://e/a44e599d360a8d33f56937e3b00c9c5d` (`checkRole`)
- relation: `READS`
- tail: `repo://e/0f32f8af0b050a37999f1e30ef0d0901` (`user`)
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
- edge_id: `edge-key:-2169610515863014134`
- head: `repo://e/dab42b501d98f20fd7dd491e0d09b2fd` (`submitOrder`)
- relation: `READS`
- tail: `repo://e/839eb5a245df15a8f8a9af0da2b16efa` (`saveOrder`)
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
- edge_id: `edge-key:-2003782212418409357`
- head: `repo://e/51fff005ee0ed8b8cd46f751b4c4f2fa` (`requireUser`)
- relation: `READS`
- tail: `repo://e/5699ea75123aa1779a1b7ff62f7c5b31` (`user`)
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
- edge_id: `edge-key:-2001264255659101451`
- head: `repo://e/1f89b459da26c0283f1355c5c7b2b612` (`saveComment`)
- relation: `READS`
- tail: `repo://e/6231fc5dca04885693d7ef9c5c0e88c4` (`normalized`)
- source_span: `sanitizer_exists_but_not_on_flow/repo/src/register.ts:6-6`
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
4:   const raw = req.body.comment;
5:   const normalized = raw.trim();
6:   return writeComment(normalized);
7: }
8: 
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
- edge_id: `edge-key:-1918073339163376418`
- head: `repo://e/2d245887cb4eb027b3601e090eb8ebc1` (`saveOrder`)
- relation: `READS`
- tail: `repo://e/db9622c37041311dc1c332c3b6af550b` (`ordersTable`)
- source_span: `derived_closure_edge_requires_provenance/repo/src/store.ts:4-4`
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
2: 
3: export function saveOrder(order: any) {
4:   return ordersTable;
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
- edge_id: `edge-key:-1604816454417718436`
- head: `repo://e/0570030f46c7d0bfeb2dd40aa28332ad` (`chooseUser`)
- relation: `READS`
- tail: `repo://e/7180e161047362d396e94b7c84153681` (`id`)
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
- edge_id: `edge-key:-1393554533152118966`
- head: `repo://e/a77fa400e86d4626945c0623148a30dc` (`checkout`)
- relation: `READS`
- tail: `repo://e/4772f45c649a7bb16fcf312d511927cb` (`total`)
- source_span: `test_mock_not_production_call/repo/src/checkout.ts:4-4`
- relation_direction: `head_to_tail`
- exactness: `parser_verified`
- confidence: `1`
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
- edge_id: `edge-key:-1078538732771348825`
- head: `repo://e/af9c034dae17a9c6cb6370af63ddcab0` (`run`)
- relation: `READS`
- tail: `repo://e/feb243ed8be75375c9b131740ffadfcb` (`renamedTarget`)
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
- edge_id: `edge-key:-1055227141490625092`
- head: `repo://e/17d04388e2d4fc14e84f74a37d2b2542` (`handler`)
- relation: `READS`
- tail: `repo://e/470cf15df9a70d66a374a9c005ad1c50` (`chooseUser`)
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
- edge_id: `edge-key:-744270777804890043`
- head: `repo://e/17d04388e2d4fc14e84f74a37d2b2542` (`handler`)
- relation: `READS`
- tail: `repo://e/54496b38ba7d6a9bdb39c05b89dad921` (`id`)
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
- edge_id: `edge-key:-458875916231284018`
- head: `repo://e/f7f5b8bb291ea408f740296b8634313e` (`run`)
- relation: `READS`
- tail: `repo://e/afc11748298c080b4f9a161e8170d07d` (`liveTarget`)
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
- edge_id: `edge-key:-331606334840065404`
- head: `repo://e/4c5ddf27ae830699fb0cf437d28b1e3f` (`writeComment`)
- relation: `READS`
- tail: `repo://e/23a1c8be735c27b5cb755ba1ba662ae7` (`comment`)
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

