# 16 Auth And Security Extraction Fix

Source of truth: `MVP.md`.

## Verdict

**Complete with a context-packet caveat.** The baseline indexer now emits proof-grade `CHECKS_ROLE`, `SANITIZES`, and `FLOWS_TO` facts only for narrow, directly supported syntax patterns. Admin/user roles are no longer conflated, an unused sanitizer import does not create a sanitized flow, and unsupported auth calls remain heuristic rather than exact.

The `sanitizer_exists_but_not_on_flow` graph-truth run still reports one failure because the current graph-truth context builder seeds `expected_entities`, including the sanitizer entity, and the same fixture also lists that sanitizer as a forbidden context symbol. The required graph facts pass: expected raw `FLOWS_TO` is matched, forbidden `SANITIZES` is not matched, forbidden sanitizer path is not matched, and source spans pass.

## Supported Patterns

| Relation | Supported exact pattern | Guardrail |
| --- | --- | --- |
| `CHECKS_ROLE` | Imported helper such as `requireAdmin(req.user)` when the imported helper body directly compares `user.role` to a string literal. | Role comes from the resolved target helper body, not the caller name or nearby comments. |
| `CHECKS_ROLE` | Direct `requireRole("admin")` or `checkRole("admin")` / `checkRole(req.user, "admin")`. | Only string-literal direct calls are exact. Other auth calls stay heuristic/unsupported. |
| `SANITIZES` | Direct call to an imported sanitizer function with a property argument, e.g. `sanitizeEmail(req.body.email)`. | Importing or defining a sanitizer is not enough to create `SANITIZES`. |
| `FLOWS_TO` | Local property binding into a same-file call argument, e.g. `const email = req.body.email; saveUser(email)`. | Only simple local binding/call argument flow is exact; no name-only sanitizer flow is inferred. |
| `EXPOSES` | Existing parser heuristic for obvious Express-style route declarations remains in place. | No new unsupported framework route pattern is promoted to exact. |
| `AUTHORIZES` | Existing parser heuristic remains in place for broad authorize/authenticate calls. | No unsupported auth pattern is promoted to proof-grade exactness. |

## Code Locations

| Surface | Files / functions |
| --- | --- |
| Narrow post-index security resolver | `crates/codegraph-index/src/lib.rs`: `resolve_security_edges_for_store` |
| Role helper proof | `crates/codegraph-index/src/lib.rs`: `role_literal_for_function`, `direct_role_check_calls` |
| Sanitizer proof | `crates/codegraph-index/src/lib.rs`: `looks_like_sanitizer_name`, direct sanitizer call handling in `resolve_security_edges_for_store` |
| Simple raw dataflow proof | `crates/codegraph-index/src/lib.rs`: `local_property_flows`, `local_property_assignments`, `parameter_declarations_for_function` |
| Edge/entity metadata | `crates/codegraph-index/src/lib.rs`: `resolved_security_edge`, projection entity helpers |
| Existing broad heuristics inspected | `crates/codegraph-parser/src/lib.rs`: `extract_route_call`, `extract_security_call` |

## Graph Truth Results

| Fixture | Result | Notes |
| --- | --- | --- |
| `admin_user_roles_not_conflated` | Passed | 2/2 expected `CHECKS_ROLE` edges matched; 0 forbidden role-conflation hits; 0 span failures. |
| `sanitizer_exists_but_not_on_flow` | Failed with caveat | 1/1 expected `FLOWS_TO` matched; 0 forbidden `SANITIZES` hits; 0 forbidden path hits; 0 span failures; 1 context failure from fixture/harness seeding conflict. |

Artifacts:

- `reports/audit/artifacts/16_graph_truth_admin_user_roles_not_conflated.json`
- `reports/audit/artifacts/16_graph_truth_admin_user_roles_not_conflated.md`
- `reports/audit/artifacts/16_graph_truth_sanitizer_exists_but_not_on_flow.json`
- `reports/audit/artifacts/16_graph_truth_sanitizer_exists_but_not_on_flow.md`

## Tests Added

| Test | Purpose |
| --- | --- |
| `audit_admin_user_role_helpers_are_not_conflated` | `requireAdmin` maps only to `admin`; `requireUser` maps only to `user`; spans point to exact call expressions. |
| `audit_unused_sanitizer_import_does_not_create_sanitized_flow` | An unused sanitizer import still allows raw `FLOWS_TO` proof but creates no `SANITIZES` edge. |
| `audit_sanitizer_call_on_value_flow_is_explicit` | A real sanitizer call creates explicit `SANITIZES` evidence. |
| `audit_unsupported_authorize_pattern_remains_heuristic` | Broad authorize-style calls remain `static_heuristic`, not proof-grade. |
| Existing MCP compact-index test | Ensures new projection entities do not reintroduce the old full FTS indexing path. |

## Unsupported Patterns

- Role detection through comments, nearby strings, naming alone, config files, decorators, route metadata, framework plugins, or indirect middleware chains.
- Sanitized flow through aliases, object destructuring, reassignment, nested/multiline expressions, higher-order functions, or sanitizer wrappers.
- General framework route extraction beyond the existing obvious Express-style heuristic.
- Policy/authz proof for arbitrary `authorize(...)`, `authenticate(...)`, `can(...)`, or config-driven auth calls.
- Multiline call expressions in the phase 16 post-index resolver.

## False Positives Prevented

- `requireAdmin(req.user)` no longer creates a `CHECKS_ROLE user` edge.
- `requireUser(req.user)` no longer creates a `CHECKS_ROLE admin` edge.
- `import { sanitizeEmail } ...` no longer implies `SANITIZES`.
- A sanitizer definition existing elsewhere no longer creates a raw-input-through-sanitizer path.
- Unsupported authorize calls are visible as heuristic evidence without being labeled exact.

## Remaining Risks

- The post-index resolver is line-scanned for its focused patterns. It is exact for the tested single-line fixtures, but should eventually become AST-backed like the parser callsite spans.
- Security projection entities are intentionally inserted through the compact record path so they do not bloat the old FTS table; symbol search may need explicit neighbor-text coverage if humans need to search these projections directly.
- The sanitizer graph-truth fixture has a context expectation conflict: the sanitizer is an expected entity but also a forbidden context symbol. Future context-packet evaluation should avoid seeding expected distractor entities or should distinguish "must exist in graph" from "must appear in packet."

## Verification

Commands run:

- `cargo test -p codegraph-index audit_ --lib`
- `cargo test -p codegraph-mcp-server mcp_index_repo_uses_shared_compact_indexer_with_external_db --lib`
- `cargo run -p codegraph-cli -- bench graph-truth --cases benchmarks/graph_truth/fixtures/admin_user_roles_not_conflated --fixture-root . --out-json reports/audit/artifacts/16_graph_truth_admin_user_roles_not_conflated.json --out-md reports/audit/artifacts/16_graph_truth_admin_user_roles_not_conflated.md --fail-on-forbidden --fail-on-missing-source-span`
- `cargo run -p codegraph-cli -- bench graph-truth --cases benchmarks/graph_truth/fixtures/sanitizer_exists_but_not_on_flow --fixture-root . --out-json reports/audit/artifacts/16_graph_truth_sanitizer_exists_but_not_on_flow.json --out-md reports/audit/artifacts/16_graph_truth_sanitizer_exists_but_not_on_flow.md --fail-on-forbidden --fail-on-missing-source-span`
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
