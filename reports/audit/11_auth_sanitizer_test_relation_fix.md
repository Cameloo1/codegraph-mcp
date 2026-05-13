# Auth, Sanitizer, and Test Relation Fix

Date: 2026-05-10 20:11:45 -05:00

## Verdict

Baseline security and test relations are now precise for the supported fixture patterns. The requested graph-truth fixtures all pass:

- `admin_user_middleware_role_separation`
- `sanitizer_exists_but_not_on_flow`
- `test_mock_not_production_call`

Full Graph Truth Gate result: 11 total, 10 passed, 1 failed. The remaining failure is `derived_closure_edge_requires_provenance`, which still lacks table-write and derived mutation provenance semantics.

## Supported Patterns

- Exact `CHECKS_ROLE` for direct `requireRole("role")` and `checkRole(..., "role")` calls when the role literal is an actual call argument.
- Role helper functions such as `requireAdmin` and `requireUser` are reified as `Middleware` when their body contains a supported exact role-check call.
- Role entities use stable role selectors such as `role:admin` and `role:user`, keeping helper names and nearby text out of the role identity.
- Obvious route factory exposure: `route("GET", "/path", guard, handler)` assigned to a local/exported const creates exact `Route -EXPOSES-> Endpoint`.
- Route factory guard argument creates exact `Route -AUTHORIZES-> Middleware` when the guard resolves to a supported role middleware.
- Direct sanitizer call on a property value, such as `sanitizeHtml(req.body.comment)`, creates exact `SANITIZES`.
- Direct sanitizer call on a local variable is supported only when the local variable has a simple proven assignment origin.
- Simple local dataflow through aliases and method calls, such as `raw -> normalized = raw.trim() -> writeComment(normalized)`, creates `FLOWS_TO`.
- Static Vitest/Jest-style module factory mocks plus assertions remain test/mock context only.

## Unsupported Patterns

- Dynamic or computed roles are not exact.
- Role checks inferred only from comments, strings, or nearby literals are ignored as exact facts.
- Sanitizer existence or unused sanitizer imports do not create `SANITIZES`.
- Complex interprocedural sanitizer/dataflow paths remain unsupported unless represented by the direct local patterns above.
- Unsupported `authorize(req.user)`-style calls remain heuristic evidence from the parser layer, not exact proof.
- Framework-specific route syntaxes beyond the supported route factory and existing parser heuristics remain heuristic/unsupported unless a syntax pattern is added.

## Guardrails Added

- Line-scanned call records now reject matches inside quoted strings and after line comments.
- Exact `CHECKS_ROLE` requires a real direct role-check call with a string literal argument.
- `requireAdmin` and `requireUser` produce distinct middleware and role edges.
- Sanitizer edges require an actual sanitizer call on a value; unused imports do not count.
- Test/mock edges remain non-production context and do not enter production proof paths by default.

## Fixture Results

Artifact: `reports/audit/artifacts/11_graph_truth_security_test_run.json`

| Fixture | Status | Notes |
| --- | --- | --- |
| `admin_user_middleware_role_separation` | passed | Distinct middleware-to-role edges for admin and user; no conflation. |
| `sanitizer_exists_but_not_on_flow` | passed | Raw comment flows to write sink; unused sanitizer does not create false `SANITIZES`. |
| `test_mock_not_production_call` | passed | Production `CALLS`, test `MOCKS`/`ASSERTS`, and mock `STUBS` stay separated. |

Full gate remaining failure:

- `derived_closure_edge_requires_provenance`: missing `src/store.ordersTable`, `WRITES`, `MAY_MUTATE`, and provenance path.

## Tests

Added or strengthened coverage:

- Admin/user role helpers stay distinct and produce exact middleware role edges.
- Comment/string role mentions do not become exact `CHECKS_ROLE` facts.
- Route factory exposure and guard authorization are exact for the supported syntax.
- Unused sanitizer imports do not create sanitized flow.
- Sanitizer calls on direct property values are exact.
- Sanitizer calls on local variables require proven local assignment flow.
- Raw local value flow through a simple alias reaches the write sink.
- Mock/stub/assert edges are stored as test/mock context, not production context.

Verification:

- `cargo test -q -p codegraph-index` passed.
- `cargo test -q` passed.
- Graph Truth Gate emitted `reports/audit/artifacts/11_graph_truth_security_test_run.json` and `.md`.

## Remaining Security Relation Risks

- Complex role wrappers, higher-order middleware, framework decorators, and computed route guards are not exact.
- Dataflow is intentionally shallow and local; cross-file or multi-hop sanitizer proofs still need provenance-aware path evidence.
- `AUTHORIZES` is exact for the supported route factory guard pattern, but broader auth policy patterns remain heuristic.
- The highest-priority remaining semantic bug is still derived/persistence provenance: table sink entities plus `WRITES` and `MAY_MUTATE` closure edges.
