# MVP4 all-language production readiness fixtures

This directory is an executable readiness contract, not a manifest-only oracle
catalog. The bench runner invokes a production `codegraph-mcp` binary, creates
disposable Git repositories for all 13 registered languages, indexes them
through `agent-use` with an external `CODEGRAPH_AGENT_USE_DATA_ROOT`, inspects
the persisted SQLite rows read-only, and queries the packet surface through the
production CLI and MCP server.

Manifest validation always returns `manifest_validated_execution_required` and
never authorizes MVP4.4. A language with unsupported, missing, heuristic-only,
nonclaimable, stale, or incomplete packet evidence is an explicit `unmet` row.
All 13 rows must be `ready` before the report can set
`ready_to_enter_mvp4_4=true`.

## Release product-surface contract

Contract `mvp4_3l_all_language_static_flow_readiness_v3` requires each language
row to prove the same persisted production packets through the release product
surfaces. The CLI checks agent-use status, doctor, an exact file/language/
production compact local-flow query, packet-id body open, context-pack handle
resolution, and a non-error validate-edit result. The MCP checks use one
newline-delimited JSON-RPC session for initialize, status, the same filtered
query, and context pack, then a fresh session that initializes and replays the
exact concrete `mcp_expansion` packet-open descriptor. The second session must
return the packet body while leaving ordered steps omitted by default.

Every command uses an external `CODEGRAPH_DB_PATH` and `CODEGRAPH_TRACE_ROOT`;
the fixture repository must not gain `.codegraph`, trace, or `target` output.
Each MCP stdin transcript is at most 64 KiB, contains one JSON value per line,
and is rejected before audit logging if it contains a secret-bearing field.
Secret environment and argument values are redacted, and children are killed
and reaped after stdin failure or timeout. All recorded CLI and MCP stdout and
stderr are scanned for `phantom_dynamic_target`. This section defines required
evidence only; it does not claim that any language row or the full gate passed.

The compatibility catalog must report Tier 5 for every canonical frontend, but
that label is not proof by itself. Syntax/span requirements use the broad
catalog capability rows; local binding, read/write, dataflow, and packet
requirements must use accepted `same_file_intraprocedural` scoped-readiness
rows with a non-empty proof boundary. Compiler, LSP, build-database, and runtime
requirements outside that verified subset remain explicit.

The canonical `typescript` row is extension-scoped rather than a
whole-frontend claim. It executes one `.mts` source as the representative
production check for the activated `.mts`/`.cts` ParserFactsV1 lane. Ordinary
`.ts` remains on the bounded legacy v1 adapter, and `.d.ts` remains inactive. A
ready `typescript` row therefore proves only that representative ParserFactsV1
lane; it must not be cited as full TypeScript-extension readiness. Migrating
ordinary `.ts` requires a separate bounded cap/omission, packet, lifecycle, and
stale-profile plan before compression work.

Each row carries all six semantic fixture classes: core binding flow,
branch/return paths, mutation, direct-call resolution, assert/check/sanitize,
and a comment-only dynamic-boundary negative. Uniform fixture, sanitizer, and
assertion comments validate source shape only. The required sanitizer,
assertion, and record calls are direct, unqualified, same-file calls; property
access lives in a separate probe so it cannot contaminate the primary packet's
primitive/local return path. Runtime readiness comes only from claimable,
source-spanned persisted node/edge/packet evidence, production CLI agreement,
and the absence of a `phantom_dynamic_target` leak into persisted or queried
packet evidence. The gate requires all 13 micro-node kinds and all 11 local
edge kinds, including sanitizer and assertion relations.

For C, C++, Ruby, and PHP, the `@codegraph-assertion` marker is the assertion
oracle. The marked helper is self-contained and returns its input instead of
invoking a macro, raising, or depending on runtime assertion configuration. The
retained C/C++ standard header include exercises ordinary include parsing only;
macro expansion and preprocessor proof remain outside this fixture's claim and
stay covered by dedicated parser hard-negative tests. The Ruby positive row
uses `element_reference` property syntax and a regular `if` branch; receiver
dispatch and modifier control flow are not inferred from fixture markers.

Run from the repository root with an already-built release binary:

```text
cargo run -p codegraph-bench --bin mvp4_language_readiness -- --binary <release-codegraph-mcp> --run-root <new-empty-directory-outside-the-repo>
```

Exit code `0` means all 13 production rows passed. Exit code `2` means the
runner completed but readiness remains unmet. Exit code `1` means the runner or
environment failed. Generated repositories, databases, command logs, and the
JSON report stay under the external run root; this fixture directory must not
receive generated artifacts or a `.codegraph` directory.
