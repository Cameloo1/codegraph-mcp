# Agent Use

Use this profile when a coding agent needs compact, proof-grounded CodeGraph
context while working on a real repository. CodeGraph is not a replacement for
normal search, editing, or tests; it is the verified repo-context layer beside
those tools.

## Recommended Pattern

Build or install the release binary, keep the DB outside the source tree, and
ask for schema-versioned, budget-aware agent JSON:

```powershell
cargo build --release --bin codegraph-mcp

codegraph-mcp agent-use status --repo <repo> --json

codegraph-mcp agent-use index --repo <repo> --json

codegraph-mcp agent-use watch --repo <repo> `
  --once --changed src\file.ts `
  --json

codegraph-mcp agent-use validate-edit --repo <repo> `
  --changed src\file.ts `
  --agent-json

codegraph-mcp agent-use validate-edit --repo <repo> `
  --changed src\file.ts `
  --changed src\helper.ts `
  --fail-on-blocking `
  --agent-json

codegraph-mcp agent-use watch --repo <repo> --json

codegraph-mcp agent-use query symbols <symbol> --repo <repo> `
  --limit 5 --agent-json

codegraph-mcp agent-use query text "token or phrase" --repo <repo> `
  --limit 5 --agent-json

codegraph-mcp agent-use query files service --repo <repo> `
  --limit 5 --agent-json

codegraph-mcp agent-use query callers handle_request --repo <repo> `
  --limit 5 --agent-json

codegraph-mcp agent-use query callees handle_request --repo <repo> `
  --limit 5 --agent-json

codegraph-mcp agent-use query path handle_request save_record --repo <repo> `
  --limit 3 --agent-json

codegraph-mcp agent-use query unresolved-calls --repo <repo> `
  --path src\file.ts `
  --class repo_local_candidate `
  --language typescript `
  --limit 20 --agent-json

codegraph-mcp agent-use context-pack --repo <repo> `
  --task "Trace the change impact" `
  --agent-json

codegraph-mcp agent-use mcp-config --repo <repo> --json
```

The first-class `agent-use` namespace resolves the `production-agent-use`
profile to an external DB path under the platform data directory, for example
LocalAppData on Windows. It is collision-safe for repos with the same basename
and does not use normal repo-local `.codegraph` paths.

`agent-use status` is strictly read-only: it does not create the DB, the profile
parent directory, WAL/SHM files, or repo-local `.codegraph`. `agent-use index`
is the first mutating command and writes only to the external production profile
DB and its bounded profile artifacts. `agent-use query` and `agent-use
context-pack` use that same external DB and refuse unsafe DB states instead of
falling back to `.codegraph`. `agent-use mcp-config` emits config JSON only by
default; it does not write a config file.

`agent-use query` supports agent JSON output for the symbol, text, file,
caller, callee, path, chain, reference, definition, and unresolved-call read
surfaces when the underlying plain query supports them. Relation/navigation
output carries relation kind, exactness, source spans, evidence role,
`proof_status`, and
`proof_strength`. `graph_proof=true` is reserved for verified graph/source
relations or proof paths; definitions are symbol-location evidence, and
references distinguish graph references from text references.

The current release help for `agent-use` subcommands intentionally prints one
shared usage block instead of separate long help pages for each subcommand. Use
the command shapes in that shared help block and in this guide as the release
contract. Richer `--explain` and `--audit-json` modes are verified by the
release command matrix for `validate-edit` and `context-pack` even though the
shared help block keeps the quick usage compact. Those richer modes are
diagnostic packets, not the tight-loop compact packet shape.

## Agent Task Lifecycle

Use CodeGraph at decision points, not as one giant repo dump. Normal developer
tools stay first-class: search with `rg`, read files, edit code, and run the
project's compiler/tests as usual. CodeGraph adds lifecycle-checked repo context
and post-edit validation beside those tools.

Recommended task loop:

1. Start with `agent-use status --repo <repo> --json`.
   - If the graph DB is missing, stale, foreign, schema-mismatched, unsafe, or
     non-claimable, run `agent-use index --repo <repo> --json` before relying
     on graph facts.
   - If only candidate/vector/source-navigation sidecars are stale, graph proof
     can still be claimable, but candidate recall is degraded until reindex.
2. For non-trivial tasks, ask for a compact planning packet with
   `agent-use context-pack --task "<task>" --agent-json`.
   - Use this to identify likely files, symbols, call paths, proof labels,
     unknowns, and follow-up inspection targets.
   - Do not treat candidate, vector, text, or source-navigation hints as graph
     proof unless the packet reports graph/source verification.
3. During investigation, use focused read packets instead of broad dumps:
   - `agent-use query symbols <symbol> --agent-json`
   - `agent-use query files <path-or-text> --agent-json`
   - `agent-use query text "<phrase>" --agent-json`
   - `agent-use query callers|callees|path ... --agent-json`
   - `agent-use query unresolved-calls --path <file> --class <class>
     --language <language>
     --agent-json`
4. After each meaningful edit batch, run
   `agent-use validate-edit --changed <path> --agent-json`.
   - Parse the flags first: `status`, `final_severity`,
     `must_fix_before_continuing`, `hard_interrupt_available`,
     `can_continue_with_caution`, `should_run_tests`,
     `should_rerun_validation`, `should_recover_tool_state`, warning/blocking
     counts, and top findings.
   - Use `--explain` or `--audit-json` only when compact output is ambiguous or
     a finding needs deeper evidence.
5. Before answering "done", rerun validate-edit for the changed files and run
   the normal project tests/checks. CodeGraph validation does not replace those
   project gates.

Approximate packet cadence:

- Tiny one-file edits: status, maybe one focused query, then validate-edit.
- Normal bug fixes: status, one planning/context packet, several focused query
  packets, validate-edit, then final validation.
- Complex refactors: status, planning packet, repeated focused reads, repeated
  validate-edit after edit batches, explain/audit only for blockers or unclear
  warnings.

Do not keep every packet in the model prompt. Collapse older packets into short
notes and keep expansion handles or report paths for anything that must be
reopened. The default agent loop should be flag-first; detailed lifecycle,
proof-ladder, and DB diagnostics are supporting evidence, not the first thing an
agent should reason over.

Use CodeGraph when the task involves unfamiliar code, cross-file behavior,
renames/deletions, call paths, stale context risk, architecture planning, or
post-edit hallucination checks. Skip it for trivial text-only edits where normal
file reads are enough.

`agent-use query unresolved-calls` does not accept a positional symbol or text
query. Filter it with `--path`, `--class`, and/or `--language`. Accepted
classes are
`repo_local_candidate`, `external_dependency`, `builtin_or_std`,
`macro_or_codegen`, `dynamic_or_computed`, `compiler_required`,
`lsp_required`, `runtime_required`, `unsupported_language_or_relation`, and
`unknown`. The output reads the unresolved-reference lane, is queryable
immediately after validate-edit/index updates, and remains explicitly
`not_graph_proof`; unresolved-reference findings warn by default and can only
block through an explicit policy mode where repo-local capability metadata is
eligible.

Real-Time Delta Sync has a release-tested production-profile primitive:
`agent-use watch --once --changed <path>`. It updates the same external
production profile DB only when an existing graph DB is safe to write. It
rejects `--db`, does not auto-index a missing/stale DB, and does not fall back
to repo-local `.codegraph`. The persistent `agent-use watch --repo <repo>
--json` surface is scheduling over that same once primitive where used: it
debounces editor save bursts, coalesces changed paths, serializes writes,
retries transient locks within bounds, exposes queue depth and last-update
state, and refuses missing/stale/foreign/schema-mismatched DBs unless the
profile is explicitly indexed first.

RTDS output includes `watch_db`, `delta_sync_phase`, `delta_sync_state`,
publish-safety labels, queue/update summaries where applicable, and staged
availability for graph, candidate-spool, vector-runtime, routing/context, and
audit layers. Candidate/vector sidecars remain candidate-only and may be
reported stale after a changed-file graph update; rebuild with `agent-use
index` when those sidecars need to be refreshed.

## Validate-Edit After A Patch

The canonical agent/editor validation command is:

```powershell
codegraph-mcp agent-use validate-edit --repo <repo> `
  --changed <path> `
  --agent-json
```

Pass more than one changed file by repeating `--changed`:

```powershell
codegraph-mcp agent-use validate-edit --repo <repo> `
  --changed src\file.ts `
  --changed src\helper.ts `
  --agent-json
```

Use `--fail-on-blocking` when a hook or CI step should stop on a real hard
interrupt while still capturing the JSON packet:

```powershell
codegraph-mcp agent-use validate-edit --repo <repo> `
  --changed src\file.ts `
  --fail-on-blocking `
  --agent-json
```

Use `--explain` or `--audit-json` only when the caller needs richer diagnostic
detail:

```powershell
codegraph-mcp agent-use validate-edit --repo <repo> `
  --changed src\file.ts `
  --explain

codegraph-mcp agent-use validate-edit --repo <repo> `
  --changed src\file.ts `
  --audit-json
```

Compact output preserves the safety-critical severity surface:
`status`, `final_severity`, `severity_summary`, `must_fix_before_continuing`,
`hard_interrupt_available`, changed-file normalization, claimability,
lifecycle, stale/unsafe blockers, finding counts, top blocking source span and
recommended fix when present, recovery command pointers, `omitted_count`, and
`expansion_handles`. `--explain` and `--audit-json` add the per-finding severity
mapping, aggregation trace, precedence decision, lifecycle/claimability detail,
proof-ladder summary, and non-interrupt reasons. The `editor_policy` object is
advisory metadata for future editor callers; `safe_to_autofix`,
`source_edits_performed`, and `daemon_integration_available` are false.

The top-level compatibility alias `codegraph-mcp validate-edit ...` is still
deferred. Use the canonical `codegraph-mcp agent-use validate-edit --repo
<repo> --changed <path> --agent-json` command.

Exit-code policy:

- Exit 0 means validation ran and emitted stdout JSON, even when the packet
  status is `blocking_graph_error`.
- With `--fail-on-blocking`, exit 2 means validation ran, JSON was printed,
  and `hard_interrupt_available=true`.
- Other nonzero exits mean validation did not complete because of command,
  config, lifecycle, tool, runtime, or protocol failure.
- Agents should parse stdout JSON and must not infer proof from exit code
  alone.

Validation status meanings:

- `blocking` / `blocking_graph_error`: reverified graph/source proof, or an
  eligible integrity/lifecycle proof failure, says the agent must stop and fix
  before continuing.
- `warning`: useful risk signal, but not a default hard interrupt.
- `unknown`: unsupported, ambiguous, degraded, or incomplete evidence; do not
  treat it as proof.
- `diagnostic_only`: operator or lifecycle detail; useful for recovery, not
  source-code proof.

Recipes:

- AI coding agent after-patch hook: collect the edited repo-relative paths,
  run the canonical command with repeated `--changed`, parse stdout JSON, stop
  only when `hard_interrupt_available=true` or local policy says a warning is
  fatal, then run the project compiler/tests/typechecker as normal.
- Editor save hook: call the same command for saved repo-relative files. This
  is an explicit hook recipe, not a persistent editor daemon, editor plugin,
  background loop, or unsaved-buffer integration claim.
- Optional pre-commit or CI gate: run with `--fail-on-blocking`, treat exit 2
  as a validation blocker, capture stdout JSON as the audit artifact, and
  treat other nonzero exits as infrastructure/config failures.
- MCP client: call `codegraph.validate_edit` with `repo` and
  `changed_files`; handle validation blockers as structured tool results, not
  MCP protocol errors.

The release binary and separate DB keep routine agent reads away from
development, lab, and temporary self-test artifacts. These outputs are usable
coding-agent context, not public metric verdicts by themselves.

For local diagnostic evaluation of edit-time guardrails, see the
[Agent Reliability Benchmark Lab](agent-reliability-benchmark-lab.md). It covers
proof-discipline scoring, hallucination traps, and patch-outcome tests. Its
results remain local diagnostic evidence; they do not create a public benchmark
claim, a CodeGraph-over-`rg` claim, a real-agent patch-quality claim, or an
MVP4 readiness claim.

Optional candidate recall for harder tasks:

```powershell
codegraph-mcp agent-use index --repo <repo> --json
codegraph-mcp agent-use context-pack --repo <repo> `
  --task "Find rare config/test/name references" `
  --agent-json
```

The production profile builds bounded candidate spool and runtime vector
sidecar artifacts where supported. They can help find plausible files, symbols,
rare identifiers, config keys, route literals, test names, and no-extension
scripts, but they do not become graph proof until graph/source verification
succeeds. Optional vector audit artifacts are diagnostic-only and are not
runtime proof sources.

## Output Modes

- `--agent-json` emits a schema-versioned, budget-aware JSON envelope for
  coding-agent loops. It includes compact lifecycle state, claim flags,
  result counts, truncation fields, warnings/errors, timings, and top results.
  Current release verification found known over-budget exceptions for some
  path/unresolved-call query packets and rich validate-edit diagnostic modes,
  so clients must inspect truncation and budget metadata instead of relying on
  byte size alone.
- `--concise` emits compact human/machine output where supported without the
  full audit payload.
- `--verbose`, `--debug`, `--profile`, and audit/report commands preserve rich
  diagnostics when explicitly requested.
- `index --json` is concise by default. Use `--explain-scope`,
  `--print-included`, `--print-excluded`, `--verbose`, or `--audit-json` when
  you need full scope examples or audit-grade index detail.

The public agent JSON schemas live under `docs/schemas/agent-json/`, with the
versioning policy in [agent-json.md](agent-json.md).

MVP4.3 local micro-flow packets are active only for verified TypeScript `.ts`
production source. Context, routing, validate-edit, watch, and MCP surfaces are
handle-first: compact output carries packet handles and summary fields, while
opened packets use `local_micro_flow_packet_agent_json` with
`encoding: "dict_v1"` and a dictionary/path `packet_body`. Verbose
`ordered_steps` are explain/audit expansion output, not the default agent-loop
payload and not a stronger proof source. JavaScript, JSX, TSX, Python, Go,
Rust, C, C++, Java, C#, Ruby, PHP, and unsupported/text-only files do not emit
local-flow packet rows or `flow_proof` unless a later fixture-backed
implementation explicitly changes that status.

Telemetry fields distinguish measured, unknown, and aggregated values. Memory
is reported as `memory: "unknown"` with `memory_measured: false` unless it is
actually measured. Timing substages that cannot be separated yet are labeled as
unknown or aggregated rather than presented as precise measurements.

## Language Support Boundary

The Pre-MVP4.4 language hardening lane verified the current registered
frontends: JavaScript, JSX, TypeScript, TSX, Python, Go, Rust, Java, C#, C, C++,
Ruby, and PHP. Exact support is fixture-backed and surface-specific; registered
does not mean every relation is exact.

Current agent-use behavior:

- TypeScript `.ts` production files have the active MVP4.3 local-flow packet
  slice: local micro-nodes, local micro-edges, compact `dict_v1`
  `local_flow_packets`, and `flow_proof` only for complete eligible local
  chains.
- TypeScript `.mts`/`.cts`, TSX, JavaScript, JSX, Python, Go, Rust, C, C++,
  Java, C#, Ruby, and PHP remain useful through their verified parser, symbol,
  text, source-role, unresolved-reference, context-pack, validate-edit, watch,
  and MCP surfaces, but packet support is `not_implemented` unless the final
  language matrix says otherwise.
- Rust, Python, Go, TypeScript, and JavaScript have fixture-backed unresolved
  reference warning behavior where eligible. External, builtin/std,
  macro/codegen, dynamic, computed, runtime, compiler/LSP-required, and
  preprocessor-required cases remain warning/unknown/diagnostic rather than
  source-proof blockers by default.
- C and C++ support parser/source-span and include/symbol evidence where
  verified, while macros, inactive preprocessor branches, templates, generated
  headers, and function-pointer behavior require preprocessor/compiler evidence
  before any exact claim.
- Java and C# support syntax/entity/import/using evidence where verified, while
  virtual dispatch, reflection, framework annotations, and dependency injection
  require compiler/LSP or runtime evidence before any exact claim.
- Ruby and PHP support syntax/entity/require/include evidence where verified,
  while metaprogramming, magic methods, framework convention routes, and dynamic
  includes/calls remain unknown or heuristic.

Text, candidate, vector, nuance, and source-navigation evidence can orient an
agent and cite source text, but they are not typed graph proof. Route and bridge
future contracts remain inert: no current `ROUTES_TO`, `MOUNTS_ROUTER`, or
`BRIDGES_TO` edge is claimable unless a source-spanned exact extractor is
explicitly present. There is no public benchmark, CodeGraph-over-`rg`/CGC,
official SWE-bench, real-agent patch-quality, or security-vulnerability proof
claim in these language surfaces.

## Graph Verification Diagnostics

Candidates are retrieval inputs, not proof. Exact, text, lexical, vector,
binary, and nuance-rescue candidates only become graph proof after graph/source
verification returns a proof path. Text evidence can support source-text
existence, but it does not prove typed graph relations by itself.

Default `context-pack --agent-json` output stays compact. It reports
proof/no-proof status, claimability, short evidence summaries, and omitted
counts without dumping traversal traces. Add `--explain` when you need the
diagnostic trace for a bounded graph walk:

```powershell
codegraph-mcp --repo <repo> --db <agent-db> context-pack `
  --task "Trace the change impact" `
  --seed <symbol> `
  --mode production `
  --agent-json `
  --explain
```

Explain output includes traversal mode, relation allowlist, source-role filter,
candidate sources that requested graph verification, traversal budgets, visited
edge/node counts, cycle and structural-skip counters, budget stop reason, no
proof fallback reason, and PathEvidence lookup/hydration timings.

Traversal budgets include max depth, max paths, max edge visits,
max-neighbors-per-node, candidate caps, structural expansion caps, and timeout
metadata where available. Relation modes keep the walk scoped:
`production` is the default proof path mode, `test-impact` intentionally admits
test/mock paths and labels them, and debug/audit modes may expose more
diagnostic detail while remaining bounded.

PathEvidence lookup and hydration are bounded. Truncation and omission labels
such as `path_evidence_truncated`, `path_evidence_omitted_count`,
`hydration_budget_exhausted`, `source_snippet_omitted_count`, and the compact
`omitted_count` fields mean evidence was capped or omitted, not silently
promoted.

When graph verification cannot prove a path, output must remain
`no_proof_path_found` and may return claimable source/text fallback evidence.
That fallback is not typed graph proof.

Planning packets may include bounded `follow_up_queries` when the packet can
orient the next inspection step. They are query hints, not shell-ready commands
and not internally executed `rg` probes.

## CLI Flag Placement

Plain commands still support explicit global flags for lower-level and
diagnostic workflows. Put those global flags before the command:

```powershell
codegraph-mcp --repo <repo> --db <agent-db> query symbols <symbol> --agent-json
```

For routine production agent use, prefer the first-class `agent-use` commands
above. Plain `status --json` remains local `.codegraph` status and does not
silently redirect to the production profile.

Command-local flags stay after the command. For example, query `--json`,
`--agent-json`, `--concise`, `--limit`, and context-pack limits are local.

Ambiguous command-tail global flags return a targeted correction instead of
being swallowed as query text. For example, `query symbols greet --db <db>`
fails with guidance to place `--db` before `query`.

To search for a literal flag-like term, use the standard `--` escape:

```powershell
codegraph-mcp --repo <repo> --db <agent-db> query text --agent-json -- --db
```

## Evidence Roles

Context and proof evidence is labeled with one of:

- `production`
- `test`
- `mock`
- `stub`
- `generated`
- `mixed`
- `text_evidence`
- `unknown`

Default production context excludes test, mock, stub, generated, mixed,
text-evidence, and unknown evidence unless a production-only subpath can be
split safely. `test-impact` mode
intentionally includes test/mock evidence and labels it clearly.

Inline Rust tests in `src/lib.rs`, including `#[cfg(test)] mod tests` and
`#[test]` functions, are classified as test evidence. Production context-pack
output excludes them by default.

## Claim Boundaries

- Absent proof-mode relations have no precision claim.
- Candidate, vector, text, and source-navigation evidence are not graph proof.
- CodeGraph does not replace compilers, tests, type checkers, linters, runtime
  checks, or security review.
- Hard interrupts derive only from reverified graph/source proof or eligible
  integrity/lifecycle proof failures. Text, candidate, vector, and
  source-navigation evidence cannot hard-interrupt by themselves.
- Unknown, unsupported, degraded, and diagnostic-only findings do not interrupt
  by default. Unsafe DB state is a lifecycle blocker, not source-code proof.
- MVP4.3 local micro-flow packets are active only for verified TypeScript `.ts`
  production source. They do not create packet proof for TSX, JavaScript, JSX,
  Python, Go, Rust, C, C++, Java, C#, Ruby, PHP, text-only files, or unsupported
  source roles.
- Local diagnostic metrics do not become public product claims unless they are
  intentionally promoted and claim-reviewed.

## Related Docs

- [Operational Profiles](operational-profiles.md) explains the profile split
  between default CLI, development/self-test, benchmark, and production
  agent-use databases.
- [Agent JSON Contract](agent-json.md) documents the agent-facing schema and
  compatibility policy.
- [MCP Reference](mcp-reference.md) covers the read-mostly MCP tools exposed to
  agent clients.
- [CLI Reference](cli-reference.md) lists the lower-level command and flag
  surface.
- [Troubleshooting](troubleshooting.md) covers stale DBs, unsafe lifecycle
  states, empty packets, and flag-placement corrections.
