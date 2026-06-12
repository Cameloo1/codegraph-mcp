# MVP_3_9_5_ISSUES_AND_FIXES.md — Validate-Edit Dogfood Gaps: Evidence, Root Causes, Fix Designs

Generated: 2026-06-09
Source of truth relationship: companion working document for the `MVP3.9.5 — Dogfood Hardening` phase defined in `MVP_3.md`. `MVP_3.md` carries the normative phase contract; this document carries the full investigation evidence, code-level root causes, and detailed fix designs. Read `MVP.md` first, `MVP_2.md` second, `MVP_3.md` third, then this document.

Origin: first human-directed agent dogfood of the MVP3 validate-edit loop (2026-06-09), executed against a release binary built from the current dev tree (all MVP3.1–3.9 work present). Probe targets: a disposable 2-file Rust fixture repo and the standalone clone at `..\codegraph-tool` (165 files, 17,418 entities, 72,927 edges) re-indexed from scratch with the same binary. The full probe log lives in `..\codegraph-tool-evaluation.md` §11. Nothing in this document is aspirational: every symptom below was reproduced live, and every root cause was confirmed by reading the current code at the cited locations.

---

## 0. Executive Summary

Four issues, in priority order:

| # | Issue | Severity | One-line root cause |
|---|---|---|---|
| 1 | Forward hallucinations invisible: a NEW call/import to a nonexistent symbol validates `ok` with zero findings (Rust + Python) | P0 | Unresolved-reference facts are extracted but dropped at persist time in `StorageMode::Proof` (the production agent-use mode); every MVP3 CALLS/IMPORTS rule operates only on stored edges |
| 2 | validate-edit crashed (exit 255, silent) then hung (612 s CPU) on a production-size file, and the failed run still published its graph update | P0 | DB commit happens before validation with the publish-state marker cleared immediately post-commit; the post-commit pipeline is unbounded, double-computes the delta, and has N+1/scan store reads |
| 3 | validate-edit is non-idempotent: a blocking finding fires once, then rerunning the identical command on still-broken source returns `ok` | P1 | Findings are derived purely from the old-vs-new delta; the first run makes the broken graph the new baseline and no validation outcome is persisted |
| 4 | Envelope and conversion ergonomics: 38 KB "compact" packet vs declared 12 KiB; `EXPLAIN QUERY PLAN` in compact JSON; context-pack returns `no_proof_path_found` + zero snippets for an exact seed with 5+ proof-grade callers | P1 | validate-edit budget enforcer is single-pass and gives up; findings are serialized three times; context-pack stage-3 path search has no single-seed relation-hydration fallback; budget enforcer prefers metadata over evidence |

What works and must not regress: the deleted-callee direction is excellent (precise span, correct rule, actionable message), exact callers/callees are correct at scale, and the proof boundary held in every probe. All fixes below are designed to preserve those properties and the standing claim rules (deterministic structural validation only; unsupported/heuristic evidence is warning/unknown, never blocking proof).

An independent agent verdict from the same dogfood window (2026-06-09) is integrated in **§7**: it confirms these four issues from a second usage session and adds three cross-cutting normative requirements (evidence-first budgeting, adversarial-use gating, honest competitive framing) plus one sequencing decision (Benchmark v1 runs before any MVP4 work). **§8** is the implementation plan for the first work package.

---

## 1. Issue 1 — Forward Hallucination Blindness (Q4)

### 1.1 Symptom (reproduced)

Fixture: 2-file Rust crate (`src/main.rs`, `src/auth.rs`), indexed via `agent-use index` (26 entities, 98 edges). All of the following edits returned `status=ok`, `blocking=0`, `warnings=0`, `unknowns=0`, guidance "Continue":

```text
add `auth::revoke_token(&session.token)`   — nonexistent fn, cross-module path call
add `audit_login_attempt(user)`            — nonexistent fn, same-file unqualified call
add `use crate::auth::nonexistent_thing;`  — import of nonexistent symbol
add tools.py calling `summarize_results()` — undefined fn, Python
```

`agent-use query unresolved-calls` returned an empty list in every case. Direct DB inspection of the fixture profile DB: `unresolved_references` table rows = 0, `heuristic_edges` rows = 0, `static_references` rows = 0, and the hallucinated names do not appear in `symbol_dict` at all.

Contrast (works): deleting the *target* of an existing parser-verified CALLS edge produced `blocking_graph_error` with rule `CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED`, an exact source span, and a hard-interrupt packet. The validator is backward-looking only.

This is the MVP_3.md §8 flagship example (`dangling_call` / `jwt_timeout` / "No definition found for called function") failing in the forward direction — the direction agents actually hallucinate in.

### 1.2 Root cause (confirmed in code, layer by layer)

The pipeline already extracts everything needed and then throws it away:

1. **Parser emits the facts.** `crates/codegraph-parser/src/lib.rs` `callee_entity` (~2503, second impl ~5582): an unresolved callee creates a *reference entity* via `push_reference_entity(..., "unresolved-callee", 0.55)` with metadata `resolution="unresolved-callee"`, `created_from="tree-sitter-static-heuristic"`, and `extract_call` (~2039) emits a `RelationKind::Calls` edge to it with `Exactness::StaticHeuristic`. Same pattern for imports/dynamic imports.
2. **The index bundles them.** `crates/codegraph-index/src/lib.rs` `LocalFactBundle::new` (~5932) routes every `StaticHeuristic`/unresolved edge through `unresolved_reference_for_edge` (~9844) into `bundle.unresolved_references` (a `LocalFactReference` with name, relation, span, exactness, extractor).
3. **Proof-mode persistence drops them.** In the per-file persist loop (~11740):
   - `persist_debug_sidecars` (~11889) — the ONLY writer of `static_references`, `heuristic_edges`, and `unresolved_references` — is gated by `options.storage_mode.preserves_heuristic_sidecars()` (~11763), which is `matches!(self, Audit | Debug)` (~1112). The production agent-use profile indexes in `StorageMode::Proof`.
   - `should_persist_entity` (~24217) excludes reference entities from the `entities` table (`should_route_static_reference_entity` ~24285 / `should_route_unresolved_entity` ~24294 match on `static_reference:` qualified names, `unknown_callee`, `created_from` containing `static-heuristic`, metadata `resolution` containing `unresolved`, metadata `heuristic=true`).
   - `should_persist_edge` (~24246) returns false for any edge routed heuristic (`should_route_heuristic_edge`).
   So in Proof mode the unresolved reference, its entity, and its edge are all dropped. Nothing reaches the DB.
4. **The store is already able to hold them.** `crates/codegraph-store/src/sqlite.rs`: the `unresolved_references` table is created unconditionally in every DB (~9942, with `idx_unresolved_references_path`), `insert_unresolved_reference_after_file_delete` exists (~1382), per-file invalidation on update/delete exists (`DELETE FROM unresolved_references WHERE source_span_path = ?1`, ~6713), and the heuristic-edge downgrade-before-storage path is tested (`unresolved_exact_edges_are_downgraded_before_storage`, ~12535). No schema migration is required for the lane itself.
5. **Validation only sees edges.** Every CALLS/IMPORTS rule in `crates/codegraph-cli/src/agent_use.rs` (`CG_MVP3_CALLS_DANGLING_TARGET`, `_REMOVED_CALLEE_STILL_REFERENCED`, `_RENAMED_CALLEE_NOT_UPDATED`, `_TARGET_ROLE_MISMATCH`, `_MISSING_SOURCE_SPAN`, `_DERIVED_MISSING_PROVENANCE`; imports equivalents) iterates stored edges / edge deltas. A reference that never became an edge is structurally undetectable. `query unresolved-calls` reads the (empty) lane.

Why it was built this way: most unresolved references are *legitimate* — calls into std/core, external crates, language builtins, macros (`format!`, `println!`), and dynamic dispatch. An unfiltered lane is noise, so it was relegated to debug sidecars. The fix must solve the noise problem, not just flip the persistence switch.

### 1.3 Fix design — Unresolved Reference Lane with metadata-tiered escalation

Concept (matches the product intuition "flag anything unlinked, then escalate by metadata"): persist every unlinked reference as a compact, explicitly non-proof fact; classify it by metadata (qualification shape, scope, source role, language tier); escalate severity at validation time only when a cheap repo-graph lookup confirms the target does not exist anywhere in the indexed repo; surface the result in the agent packet under existing proof-ladder labels.

#### 1.3.1 Persistence (index + store)

- Split `persist_debug_sidecars`: a new `persist_unresolved_reference_lane(...)` writes `bundle.unresolved_references` rows in **all** storage modes (Proof included); `static_references`/`heuristic_edges` stay Audit/Debug-only. The existing per-file `DELETE ... WHERE source_span_path` invalidation already keeps the lane fresh on update/delete — verify with a regression test.
- Restrict the lane to reference-shaped relations: `Calls`, `Callee`, `Imports`, `AliasOf/AliasedBy`, `Reexports`. Exclude `Reads/Writes/FlowsTo/Argument*` classes (local-dataflow noise; they remain debug-sidecar material).
- Bounds (storage-budget rules): cap rows per file (default 256) and bytes per row (compact metadata only); on cap hit, write a per-file `extraction_warnings` row `unresolved_reference_lane_truncated` so validation can label the file `bounded/unknown` instead of silently passing. Add a storage-budget test (lane bytes on the codegraph-tool rebuild must stay within single-digit MB; record actuals before setting the final threshold — do not invent pass thresholds without evidence).

#### 1.3.2 Classification (`reference_class`, stored in `metadata_json` — no schema migration)

Computed at bundle build time from information the extractor already has:

```text
repo_local_candidate   unqualified name; or path-qualified whose first segment is
                       crate/self/super; or first segment matches a sibling module
                       file/declared module of the same crate
external_dependency    first segment matches a declared workspace dependency
                       (Cargo.toml [dependencies], package.json deps, pyproject),
                       or an import binding in the same file resolves the root
                       segment to a non-repo source
builtin_or_std         std/core/alloc + per-language builtin allowlists
                       (println!/format!/len/print/range/console.log/...)
macro_or_codegen       Rust macro invocation shapes (name!), build-script/codegen
                       hints; always non-escalating
dynamic_or_computed    method calls on unresolved receivers, computed callees,
                       reflection shapes (existing heuristic kinds)
```

Language-tier guard: only languages whose coverage matrix supports Tier-2 calls (Rust, TS/JS, Python, Go) may produce `repo_local_candidate`; everything else is at most `dynamic_or_computed` → unknown. No language gets escalation without fixtures.

#### 1.3.3 Snapshot + delta integration

- Extend `NormalizedFactSnapshot` (`codegraph-index`) with `unresolved_references` facts (`NormalizedFactSnapshotOptions.include_unresolved_references`, on for validate-edit/watch). Stable identity key = `(path, span, relation, name)`.
- Add `classify_unresolved_reference_delta_entries` to `compute_entity_source_role_delta` (same BTreeMap added/removed/changed pattern as the other classifiers — O(n log n)). New report fields: `unresolved_references_added/removed` + counts by `reference_class`. Per the standing delta rule, these entries are labeled non-proof and must never feed `proof_ladder_changes` above the text/candidate rungs.

#### 1.3.4 Validation rules (cli `agent_use.rs`, new rule family `CG_MVP3_REF_*`)

For each `unresolved_references_added` entry on a changed file:

```text
CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL
  class=repo_local_candidate AND relation=Calls
  escalation lookup: symbol_dict/qualified-name lookup over the CURRENT DB
  (indexed, O(log n)) for the referenced name (and, for qualified paths, the
  path-resolved module): if NO defining entity exists anywhere in the repo
  graph -> severity=warning (escalated wording: "likely hallucinated symbol"),
  evidence = the failed lookup itself ("no entity named X in repo graph",
  lookup scope cited); if definition candidates EXIST somewhere ->
  DIAGNOSTIC ONLY (counted in by_class, no warning) — AMENDED 2026-06-11:
  the parser cannot see cross-file resolution, so candidates-exist is the
  normal state of every valid new cross-module call; the original
  plain-wording warning fired on the very edit that fixes a hallucination
  (adversarial pass finding) and would train agents to ignore warnings
CG_MVP3_REF_NEW_UNRESOLVED_IMPORT
  same contract for Imports/AliasOf/Reexports (covers `use crate::x::missing`)
CG_MVP3_REF_EXTERNAL_OR_BUILTIN
  class=external_dependency|builtin_or_std|macro_or_codegen -> diagnostic-only
  count; no loud finding, never a warning
CG_MVP3_REF_DYNAMIC
  class=dynamic_or_computed -> unknown (existing heuristic boundary unchanged)
```

Severity boundary decision (must be preserved in review): the default ceiling for new unresolved references is **warning**, not blocking. An unresolved reference is *absence of a link*, not deterministic proof of error — macros, conditional compilation, codegen, and linker-level symbols can define targets the graph cannot see. This is exactly MVP3.1's "It must not invent correctness claims" and MVP3.3's warning tier ("computed import target unknown"). A policy opt-in (`--block-on-unresolved-local`, default off) may promote the escalated case to blocking for teams that want it, and may become the default for a language only after fixture evidence shows a near-zero false-positive rate for that language's `repo_local_candidate` class. Hard-interrupt eligibility stays unchanged (block-class findings only).

Resolution bookkeeping: `unresolved_references_removed` entries on changed files are reported as `resolved_count` (positive signal, diagnostic-only).

#### 1.3.5 Agent packet integration (the agent-use contract)

validate-edit and `watch --once` packets gain one compact block, budgeted inside the existing envelope:

```json
{
  "unresolved_references": {
    "schema_version": 1,
    "new_count": 2,
    "resolved_count": 0,
    "by_class": {"repo_local_candidate": 2, "external_dependency": 0, "builtin_or_std": 5, "dynamic_or_computed": 1},
    "escalated": [
      {
        "name": "revoke_token",
        "relation": "CALLS",
        "reference_class": "repo_local_candidate",
        "file": "src/main.rs",
        "span": {"line_start": 9, "line_end": 9},
        "repo_graph_lookup": "no_entity_named_revoke_token",
        "severity": "warning",
        "proof_strength": "text_evidence",
        "claimability": "claimable_as_source_text_reference_only",
        "recommended_fix": "Define revoke_token with a source span, fix the name, or mark the dependency external."
      }
    ],
    "escalated_omitted_count": 0,
    "expansion_handle": "validation_packet:unresolved_references",
    "not_graph_proof": true
  }
}
```

Contract rules: top-N escalated items inline (default 3), counts always present, `proof_strength` capped at `text_evidence`/`symbol_evidence` rungs (never `graph_relation_proof`), warnings flow into the existing `warnings[]`/severity model (MVP3.6) unchanged, and the block participates in budget shedding *after* blocking errors but *before* lifecycle metadata. `query unresolved-calls` reads the lane (bounded, filterable `--class`, `--path`); MCP `codegraph.validate_edit` mirrors the block.

#### 1.3.6 Fixtures and gate (extends MVP3.8)

```text
same-file unqualified call to nonexistent fn       Rust/Py/TS  -> warning (escalated)
cross-module qualified call (auth::missing)        Rust        -> warning (escalated)
use crate::x::nonexistent import                   Rust        -> warning (escalated)
call to std/format!/println!/console.log           all         -> NO warning (diagnostic count only)
call into declared external dependency             all         -> NO warning
macro-generated symbol referenced                  Rust        -> NO blocking ever (negative)
dynamic/computed callee                            TS          -> unknown
fix the reference -> rerun                         all         -> warning cleared, resolved_count=1
lane truncation (file over cap)                    synthetic   -> bounded label, no silent pass
storage budget on real-repo rebuild                clone       -> lane bytes within recorded budget
```

Gate passes when every fixture matches, the hallucination-interrupt gate (MVP3.9) is re-run with the forward cases added, and zero claimability violations appear (no unresolved-reference output ever labeled graph proof).

---

## 2. Issue 2 — Scale Crash/Hang + Update Published On Failure (Q5)

### 2.1 Symptom (reproduced)

Probe: rename `open_store_with_preflight` (20 known callers) in `crates/codegraph-mcp-server/src/lib.rs` of the codegraph-tool clone (283 KB, 6,783 lines), then `agent-use validate-edit --changed <file> --agent-json`.

```text
run 1: 66 s wall -> exit 255, EMPTY stdout and stderr (no packet, no error JSON)
run 2 (identical): CPU-bound, killed manually at 612 s CPU
after restoring the source: the profile DB had absorbed the renamed-state facts
  (original symbol count=0, renamed symbol count=1), status=ok, claimable=true,
  no staleness blocker, publish_state absent
control: plain `agent-use index` resynced the same file in seconds
```

Two distinct failures: (a) the run that *failed* still left its graph update committed with all safety markers cleared — the agent's contract "command failed ⇒ nothing happened, or I am told what happened" is violated, and any blocking findings that validation *would* have produced are lost forever (see Issue 3); (b) the post-commit pipeline is slow/unbounded enough to crash or hang on an ordinary production-size file, and it dies silently instead of degrading to a bounded packet.

### 2.2 Root cause (confirmed in code)

`run_agent_use_watch_once_delta` (`crates/codegraph-cli/src/agent_use.rs:2516`), which validate-edit wraps:

```text
1. write preflight + path preflight
2. RTDS dependency closure + OLD normalized snapshot   (reads)
3. write_agent_use_publish_state("updating")           (:2595)
4. update_changed_files_to_db                          (:2597)  <- THE COMMIT
5. clear_agent_use_publish_state                       (:2618)  <- marker GONE
6. NEW snapshot, delta x2, validation, packet build    (:2687+) <- crash window
```

- **Atomicity**: the publish-state marker protects only step 4. From step 5 onward, a crash leaves a committed update, no marker, no packet, no persisted findings. `publish_safety.partial_update_claimability = "non_claimable_if_publish_state_interrupted"` is therefore true but useless for this window. Nothing persists the *intent to validate*.
- **Silent death**: there is no `catch_unwind`/structured-error wrapper around the post-commit phase; whatever killed run 1 (exit 255 ≈ abnormal termination; OOM-abort from unbounded delta retention is the leading suspect, stack overflow second) produced no JSON. The per-stage timing JSON (`agent_use_graph_delta_timing_json`) exists but is only emitted at the very end, so it dies with the process.
- **Unbounded + duplicated work**:
  - the validation delta is computed with `max_items_per_category: usize::MAX` (:2698) — every added/removed/changed entity/edge/span/text-evidence fact across changed + closure files is retained as a fully-hydrated `*DeltaEntry` (each serializes to ~1–5 KB with old/new/claimability copies);
  - in compact mode the **entire delta is computed a second time** (:2711) with the top-limit, instead of deriving the compact view from the first result;
  - the `ValidationPacket` and its JSON are then cloned again for the packet body, the hard-interrupt mirror, and the top-level mirrors.
- **Store-read hot spots** (multiplied by 2 snapshots × closure size):
  - `list_edges_by_file` (`codegraph-store/src/sqlite.rs:2494`) resolves edge ids, then calls `get_edge(id)` **per edge** — an N+1 of prepared statements with dictionary joins; a crate-root `lib.rs` participates in thousands of edges;
  - `list_text_search_hits_by_file` (:3155) falls back to `SELECT ... FROM stage0_fts WHERE repo_relative_path = ?1` — a column filter on an FTS5 virtual table = full FTS scan per path;
  - the RTDS closure (`codegraph-index/src/lib.rs:13716`) is budgeted, but materializes up to `max_edges_inspected` edges via `list_edges(limit)` into memory to filter in Rust instead of filtering in SQL on the indexed head/tail columns.
- The delta *classifiers* themselves (`classify_entity/edge/span/text_evidence_delta_entries`) are BTreeMap-based O(n log n) — not the problem.

### 2.3 Fix design

Fully addressing this issue means validate-edit becomes a **journaled, bounded, attributable** operation: a failed run is always visible, always recoverable, and never silent; and the pipeline finishes ordinary production files comfortably inside a budget, degrading to labeled `bounded`/`unknown` output instead of dying when it cannot.

#### 2.3.1 Validation journal + state machine (atomicity)

New sidecar next to the existing delta-state: `production-agent-use.validation-journal.json` (same single-writer profile dir).

```text
phase A  preflights pass
phase B  write journal: {journal_version, started_unix_ms, changed_files,
         closure_files, old_snapshot: <normalized facts for changed+closure
         paths, compact form>, state: "update_pending"}
phase C  write publish_state "updating" -> update_changed_files_to_db -> commit
phase D  journal state -> "validating"; publish_state cleared (the DB itself
         is now consistent with source; what is pending is VALIDATION)
phase E  snapshots/delta/validation/packet
phase F  persist validation outcome to delta-state (see Issue 3), delete journal
```

Crash semantics:

```text
crash in C  -> existing behavior preserved: publish_state "updating"/"interrupted"
               -> DB non-claimable until recovered (unchanged contract)
crash in D/E-> journal exists with state "validating" + the OLD snapshot:
               - status/doctor/watch/validate-edit surfaces report
                 validation_state="incomplete" with the changed files cited
                 (diagnostic lane; graph claimability is NOT revoked — the
                 committed graph is source-accurate; what is unknown is whether
                 the edit broke contracts)
               - the next validate-edit (any files) FIRST replays the pending
                 validation: old facts from the journal vs current DB ->
                 delta -> findings -> outcome persisted; then proceeds
               - `agent-use index` clears the journal (full reindex supersedes)
```

Journal bounds: store only normalized fact rows for changed+closure paths (the same compact `NormalizedFact*` forms the snapshot already produces), cap at a few MB; if the closure exceeds the cap, journal the changed files only and record `journal_scope: "changed_files_only"` — replayed validation is then labeled `bounded` for closure-dependent rules. Journal writes are atomic (temp + rename).

This kills the poisoning: a failed run leaves either a non-claimable DB (crash during commit — existing behavior) or a claimable DB with a visible, replayable "validation incomplete" state (crash after commit — the new behavior). It is also the natural home for Issue 3's persisted outcome.

#### 2.3.2 No silent failure

- Wrap the post-commit pipeline (phases D–F) in `catch_unwind`; on panic: write journal state `"failed: <panic msg>"`, emit a minimal structured error packet (`status="error"`, `validation_state="incomplete"`, stage name, recovery commands), exit nonzero. A validate-edit invocation must never end without JSON on stdout.
- Stage progress markers: append the current stage name + start timestamp into the journal as each stage begins (8 tiny writes per run). A post-mortem then answers "which stage was active when it died" — the MVP2 M-Index attribution principle applied to validate-edit. `--explain`/`--audit-json` additionally emit per-stage wall/memory telemetry inline.

#### 2.3.3 Bound the pipeline

- Validation delta: replace `usize::MAX` with an explicit cap (`validation_max_items_per_category`, default 10,000) + per-category `omitted_count`. If a blocking-relevant category (entities_removed, edges_removed, edges_added) overflows, the packet's `final_status` is capped at `unknown` with reason `graph_delta_bounded` and a recommended `agent-use index` + re-validate — per the standing MVP3 rule that bounded support must be labeled, never silently passed.
- Compute the full-cap delta **once**; derive the compact view by truncating the report (the compact recompute at `agent_use.rs:2711` is deleted). Hydrate `*DeltaEntry` old/new payloads lazily — counts first, full entries only for categories the rules actually consume.
- Whole-pipeline wall budget (`--max-validation-ms`, default 60,000): checked between stages; on breach, finish the current stage, emit the packet with `validation_state="bounded_timeout"` + what was and was not validated. Never exceed silently.

#### 2.3.4 Store-read fixes

- `list_edges_by_file`: single batched query (chunked `WHERE id_key IN (...)` joined to the dictionary tables) replacing the per-id `get_edge` loop.
- `list_text_search_hits_by_file`: ensure the `file_fts_rows` rowid map is populated on every write path so the per-path FTS full-scan fallback is cold-path only; add a counter so the fallback's use is visible in read-path metrics.
- RTDS closure: filter edges in SQL (`WHERE head_id_key IN (changed) OR tail_id_key IN (changed)`, using existing head/tail indexes) instead of materializing `list_edges(limit)` and filtering in Rust.

#### 2.3.5 Acceptance gate

```text
rename probe (283 KB lib.rs, 20 callers) completes < 10 s p95 release-binary,
  emits the blocking finding, zero crash, 5 consecutive runs
kill -9 injected between commit and packet emission (chaos failpoint) ->
  journal present, status reports validation_state=incomplete, next
  validate-edit replays and emits the SAME blocking finding
panic injected in delta stage -> structured error JSON on stdout, exit nonzero,
  journal state failed:<msg>
delta cap forced low on a large delta -> final_status=unknown with
  graph_delta_bounded reason (no silent ok)
existing MVP3.8 fixture matrix still green; index path timings unchanged
```

---

## 3. Issue 3 — Non-Idempotent Blocking (Q6)

### 3.1 Symptom (reproduced)

Fixture: delete `issue_token` (target of a verified CALLS edge) → validate-edit returns `blocking_graph_error`, `must_fix_before_continuing=true`. Rerun the **identical command on the unchanged, still-broken source**: `status=ok`, `blocking=0`, guidance "Continue"; `--fail-on-blocking` exits 0. The hard-interrupt message itself instructs "Restore the target or update the call, **then rerun CodeGraph validation**" — following that instruction without fixing anything blesses the broken code.

### 3.2 Root cause

All `CG_MVP3_*` findings are derived from the old-vs-new **delta** of the triggering run. The first run commits the post-edit graph (correctly — it matches source), so the second run's old and new snapshots are identical: no delta, no findings. No validation outcome is persisted anywhere — the delta-state file (`production-agent-use.delta-state.json`, written by `persist_agent_use_last_delta_state`, `agent_use.rs:13601`) records only an update summary. The dangling fact itself cannot be re-derived from the DB alone: the unresolved call site produces no edge (Issue 1), so post-commit the graph contains no contradiction to find.

### 3.3 Fix design — persisted, re-verified blocking state ("sticky blockers")

Extend the delta-state record (same file, additive field):

```json
{
  "last_validation": {
    "schema_version": 1,
    "completed_unix_ms": 0,
    "changed_files": ["src/auth.rs"],
    "final_severity": "blocking",
    "open_blockers": [
      {
        "finding_id": "finding://mvp3_3/calls/CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED/edge://...",
        "validation_rule_id": "CG_MVP3_CALLS_REMOVED_CALLEE_STILL_REFERENCED",
        "relation_kind": "CALLS",
        "file": "src/auth.rs",
        "source_span": {"line_start": 19, "line_end": 19},
        "caller_identity_key": "fact://...",
        "missing_target_name": "src::auth.issue_token",
        "reason": "exact CALLS edge pointed to a missing callee entity",
        "recommended_fix": "..."
      }
    ]
  }
}
```

Behavior:

- **Write**: every validate-edit/watch-once run persists its outcome (severity + block-class findings in the compact form above). `agent-use index` (full or incremental over the affected files) re-derives: after a full update, open blockers are re-checked once and cleared/kept accordingly.
- **Recheck on every run**: before emitting a packet, load `open_blockers`; for each, **re-verify the contradiction against the current store + source** using the existing reverification machinery (`agent_use_reverify_edge_source_span` family + indexed symbol lookups): does the caller's span still contain the referencing text, and is the target still absent from the repo graph? Still contradicted → re-emit the finding (marked `"origin": "persisted_open_blocker"`) with full blocking semantics, including `--fail-on-blocking` exit 2 and hard-interrupt eligibility. Resolved → drop it, record in `resolved_blockers_count`. Un-checkable (file deleted, span unreadable) → re-emit as `unknown` with the reverification failure reason, never as a silent clear and never as an unverified blocking claim.
- **Surface everywhere**: `agent-use status`, `watch`, validate-edit, and context-pack gain a compact `validation_state` block — `{state: ok|blocked|incomplete, open_blocker_count, since_unix_ms, expansion_handle}`. Blocked state does **not** change graph claimability (the graph honestly reflects broken source); it is a labeled workflow/diagnostic lane. MCP `validate_edit`/`status` mirror it.
- **Boundary (critical for the proof contract)**: a persisted blocker is *never replayed as proof*. Re-emission requires fresh re-verification each run; the packet cites the re-verification evidence, not the stored record. This keeps "no stale proof" intact.

Fixtures: blocking → rerun unchanged → still blocking (the exact dogfood failure); blocking → fix source → rerun → ok + `resolved_blockers_count=1`; blocking → validate a *different* file → blocked state still surfaced; blocking → full `agent-use index` → blocker re-checked once (kept while source still broken); crash-after-commit (Issue 2 journal) → replay produces the same persisted blocker; blocker file deleted → unknown with reason.

---

## 4. Issue 4 — Envelope Bloat + Conversion Gaps (FYI59)

### 4.1 Symptoms (reproduced)

```text
a. validate-edit compact (--agent-json) = 38,135 B against its own declared
   max_output_bytes=12288; agent_json_budget self-reports
   max_output_bytes_exceeded=true and ships anyway; the single blocking finding
   is serialized ~3x (top-level mirrors + validation_packet + hard_interrupt,
   each with full old/new affected_delta copies)
b. compact `query unresolved-calls` embeds a full SQLite EXPLAIN QUERY PLAN
   (instrumentation.explain_query_plan) -> 13.2 KB for an empty result
c. context-pack with an exact symbol seed (resolve_agent_use_profile: exists,
   5+ proof-grade callers via `query callers`) -> proof_status=
   no_proof_path_found, 0 proof paths, 0 snippets, 0 candidates; the 11.7 KB
   packet is entirely lifecycle/status metadata with omitted_by_budget=33
   (the evidence was budgeted away, the ~40 metadata sections survived)
d. NL-task seed extraction produced junk seeds ("Markdown", "agent-use")
e. zero-result callees envelope = 5.7 KB of boilerplate
```

### 4.2 Root causes

- (a) `agent_use_validate_edit_finalize_budget` (`agent_use.rs:2428`) is **single-pass**: over budget → null `source_update_packet`, truncate `warnings/unknowns/diagnostics` to 1, truncate validation-packet lists → re-measure once → if still over, set `max_output_bytes_exceeded=true` and return. It never sheds the dozens of top-level metadata sections (`severity_trace`, `proof_ladder_changes`, `dirty_evidence_summary`, full `graph_delta`, duplicated lifecycle/claimability/recovery blocks) and never dedups the triple-serialized finding. Contrast: the context-pack enforcer (`enforce_context_agent_max_output_bytes`) is iterative with a protected set.
- (b) the instrumentation block is attached unconditionally on the unresolved-calls read path instead of behind `--explain`/`--audit-json`.
- (c) `crates/codegraph-query/src/lib.rs` stage 3 (~4568): for each retrieval candidate, `context_paths_for_seed(id, query_limits)` must return at least one stored/traversed path or the seed is dropped (`stage3_dropped`). For Rust seeds this path search comes back empty even when the seed has direct proof-grade CALLS edges — there is no fallback that hydrates the seed's immediate relation neighborhood as single-hop verified paths. (Carried finding from eval report §10.2; re-confirmed on the current build.) Additionally, the context-pack budget enforcer prioritizes status sections over fallback evidence in the no-proof branch.
- (d) task-derived seeds are not filtered against the symbol/path dictionaries before being treated as seeds.

### 4.3 Fix design

#### 4.3.1 validate-edit envelope (compact contract enforcement)

- Replace the single-pass finalize with the iterative protected-set enforcer pattern already proven on context-pack:

```text
protected (never shed): schema fields, status, final_status, final_severity,
  must_fix_before_continuing, blocking summary = top finding {rule_id, file,
  span, message, recommended_fix}, error/warning/unknown counts,
  unresolved_references counts (Issue 1), validation_state (Issue 3),
  claimability (compact), lifecycle (compact), expansion_handles,
  agent_json_budget
shedding order (first to go): source_update_packet, severity_trace,
  proof_ladder_changes detail -> counts, dirty_evidence per-surface detail ->
  summary booleans, graph_delta lists -> counts, hard_interrupt.errors ->
  top-1 + aggregate_counts, duplicated top-level mirrors, recovery long forms,
  input diagnostics, per-path status arrays
```

- Dedup the finding: the full finding object lives ONCE under `validation_packet.blocking_errors`; the hard-interrupt entry carries `finding_id` + message + span + expansion handle; top level carries only the summary. (Byte-budget lesson from the P2b regression applies: never add per-item bytes to fallback paths that fire in at-budget fixtures — verify against the 12 KiB fixtures after every change.)
- Acceptance: blocking fixture compact ≤ 12,288 B with span+fix preserved; `max_output_bytes_exceeded=true` becomes possible only in `--explain`/`--audit-json` modes; existing compact-contract tests (`max_output_bytes_exceeded` acceptable only with safety fields preserved) updated to the strict bound.

#### 4.3.2 Instrumentation gating

`instrumentation.explain_query_plan` and verbose read-path internals move behind `--explain`/`--audit-json` on every agent-use query surface; compact keeps `elapsed_ms` totals only. Acceptance: empty-result compact envelopes ≤ ~3 KB.

#### 4.3.3 Context-pack single-seed proof conversion

- In stage 3, when a seed **resolved exactly** but `context_paths_for_seed` is empty: hydrate a bounded one-hop proof neighborhood directly from the store (incoming/outgoing `CALLS`, `IMPORTS`, `WRITES/READS/MUTATES` proof-grade edges with spans, existing `query_limits` caps) and emit each as a verified single-hop path (`path_length=1`, `hydration="demand_driven_one_hop"`). These are genuine `graph_relation_proof` (typed edge + span + claimable lifecycle) — no boundary change, just conversion of proof the graph already holds. Multi-hop search continues to run first; the hydration is a fallback, labeled as such in the retrieval trace.
- Budget priority in the no-proof/fallback branch: evidence sections (verified paths, snippets, fallback text evidence, candidates) survive ahead of optional status metadata; the "33 evidence items omitted while ~40 status sections kept" case becomes a named regression test.
- Acceptance: a context-pack seeded with a known exact Rust symbol that has proof-grade edges returns ≥ 1 verified path + ≥ 1 snippet within the default budget; the JS flagship path is unchanged; nonsense seeds still return honest `no_evidence_found`.

#### 4.3.4 Seed hygiene (smaller)

Filter task-derived seed tokens against `symbol_dict`/path tokens before they become seeds; non-matching prose tokens stay query terms for text evidence only. Acceptance: the dogfood task no longer yields "Markdown" as a symbol seed.

---

## 5. Cross-Issue Sequencing (implementation order)

Dependencies make this order strongly preferred:

```text
9.5.1  Instrumentation + structured failure (2.3.2 markers/catch_unwind, 4.3.2)
       -> reproduce the rename-probe death WITH stage attribution before
          optimizing; no behavior change otherwise
9.5.2  Journal state machine + sticky blockers (2.3.1 + Issue 3)
       -> one shared delta-state/journal design; fixes poisoning + idempotency
9.5.3  Pipeline bounds + store-read perf (2.3.3, 2.3.4)
       -> gate on the rename probe p95 and chaos failpoints
9.5.4  Unresolved Reference Lane (Issue 1, all of §1.3)
       -> the biggest feature; lands after the loop is safe and fast
9.5.5  validate-edit envelope enforcement + dedup (4.3.1)
       -> after 9.5.4 so the new unresolved_references block is budgeted in
9.5.6  Context-pack conversion + budget priority + seeds (4.3.3, 4.3.4)
9.5.7  Final regression gate (see MVP_3.md MVP3.9.5 gate)
```

Every phase: targeted tests + release-binary smoke against disposable temp repos/external profile DBs, no normal `.codegraph` mutation, reports under `reports/audit/`, no stage/commit/push by agents, no public claims. Note for whoever implements: validate-edit currently mutates the profile DB baseline on every probe — resync with `agent-use index` between manual probes until 9.5.2 lands.

## 6. Claim-Boundary Review Summary

```text
unresolved reference            -> warning ceiling by default; never blocking
                                   without explicit policy opt-in + fixture
                                   evidence; never above text/symbol proof rungs
persisted blocker               -> re-verified against current store/source
                                   before every re-emission; never replayed
                                   stale; un-checkable -> unknown with reason
bounded delta / timeout         -> final_status capped at unknown with reason;
                                   bounded support is labeled, never a pass
journal "validating" state      -> graph claimability NOT revoked (DB is
                                   source-accurate); validation completeness is
                                   the thing reported incomplete
one-hop hydrated paths          -> real typed-edge proof; no new proof source,
                                   conversion only
external/builtin/macro refs     -> diagnostic counts only; no warning noise
```

No fix above creates a new proof source, weakens an exactness label, or lets candidate/text evidence imply graph proof.

---

## 7. External Verdict Integration (2026-06-09)

A second agent ran an independent dogfood session in the same window and wrote a strategic verdict (originally `docs/verdict1temp.md`, a temp file; substance preserved here). It confirms Issues 1–4 from a separate usage path and adds judgments this phase adopts as normative.

### 7.1 What the verdict independently confirms

- The proof boundary held in every probe it ran too — nonsense seeds, stale DBs, foreign repos, heuristic lanes; "nothing overclaimed, ever." It calls this the genuinely rare asset ("your 30k-star competitor doesn't even try") and the right long-term bet: *"trust compounds: an agent that can rely on 'this tool never lies' can skip re-verification work, and that's where the token savings actually come from."* Every fix in this document must preserve it.
- The empty-but-honest packet failure (Issue 4), seen from the consumer side: an 11.7 KB context-pack with zero snippets, zero paths, ~40 lifecycle sections, while 33 evidence items were budget-dropped — *"worse than rg for an agent, at higher token cost."* The 38 KB validate-edit packet against its own 12 KiB contract.
- Issues 1–3 (flagship-scenario miss, crash/hang, baseline poisoning) were all found within ~30 minutes of real use, despite dozens of green gates and 31/31 fixture matrices.

### 7.2 Normative requirements adopted

1. **Evidence-first budgeting (all agent-use surfaces).** *"The value-to-metadata ratio is inverted... Honest labels on an empty packet are still an empty packet."* When any budget forces shedding, evidence (findings, spans, fixes, paths, snippets, candidates) outlives optional lifecycle/status metadata. "Evidence omitted while metadata kept" is a named regression class with a fixture (the 33-omitted case). This promotes §4.3.1/§4.3.3 from two local fixes into a contract rule across every enforcer.
2. **Adversarial-use gating.** *"The process has been optimizing for self-defined gates, not usage... every gate should include at least one adversarial use pass, not just fixture replay."* The two dogfood sessions falsified gate-greenness as a quality proxy within 30 minutes. Adopted: every MVP3.9.5 sub-phase gate and the MVP3 final gate require at least one live adversarial use pass (real edits against a repo not used during development, through the release binary, trying to break the loop), with findings filed, before the gate may be called green. (Added to the MVP3 Final Gate checklist in `MVP_3.md` §13.)
3. **Honest competitive framing.** For compiled languages the real competitor is `cargo check`/`tsc`/LSP diagnostics, which catch dangling references better than the graph ever will. validate-edit's defensible niche: (a) dynamic languages with no compiler gate (Python, plain JS); (b) monorepos where a build takes minutes and the graph check seconds; (c) cross-file staleness and test/prod leakage that diagnostics do not model; (d) output structured for an agent loop rather than human eyeballs. Consequences adopted: dynamic-language fixtures are first-class in every MVP3.9.5 fixture set (Python/JS forward-hallucination cases gate the lane, not just Rust), and all docs/claims frame validate-edit as complementary to compiler diagnostics for compiled languages — never their replacement.
4. **Sequencing: 9.5 → Benchmark v1 → only then MVP4.** *"Right now the core effectiveness claim is faith-based, and you've correctly forbidden yourself from claiming it."* The A/B patch-outcome benchmark runs after the 9.5.7 gate and before any MVP4 work. The verdict flags MVP4 micro-flow packets as the roadmap item most exposed to model-side improvement (retrieval marginal value is shrinking each model generation; verification/grounding value is not, but it does get commoditized by compilers/LSPs — so speed, language breadth, and agent-loop integration are where to win). MVP4 scope is revisited with Benchmark v1 data in hand.

### 7.3 What the verdict does NOT change

- No fifth issue; no fix design in §§1–4 needed revision.
- The warning-ceiling decision (§1.3.4) stands and is *reinforced*: for compiled languages the compiler is the blocking authority on dangling references; the lane's escalated warning is the in-loop agent signal, with `--block-on-unresolved-local` as the opt-in.
- The proof-boundary/lifecycle discipline is explicitly the thing to keep while fixing everything else; the verdict's three conditions for the tool being "substantially effective" map exactly onto this phase: validate-edit that catches inventions, doesn't crash, and can't be forgiven by rerunning (Issues 1–3); packets where evidence survives and metadata dies first (Issue 4 + §7.2.1); and Benchmark v1 actually run (§7.2.4).

---

## 8. Implementation Plan — First Work Package (planned 2026-06-09)

Scope decision: the first package is **9.5.1 + 9.5.2** (stage attribution, structured failure, validation journal, sticky blockers — the Q5/Q6 safety core). Rationale: every later phase needs a validate-edit loop that cannot die silently or poison its own baseline *while being tested* — including the lane work itself, whose fixtures hammer validate-edit repeatedly. The unresolved-reference lane (9.5.4) is planned in the same file-level detail in §8.4 and starts immediately after; 9.5.3 (perf) is interposed only if the rename probe is still over budget once 9.5.2's duplicate-delta removal lands.

### 8.1 Step 1 — Stage markers + panic wrap + instrumentation gating (9.5.1)

No change to validation semantics. All in `crates/codegraph-cli/src/agent_use.rs` unless noted.

1. `ValidationStageTracker` (new, small): writes `<profile dir>/production-agent-use.validation-stage.json` — `{run_id, started_unix_ms, stages: [{name, started_unix_ms}]}` — at each stage start; deleted on clean finish. Stage names: `preflight, closure, old_snapshot, publish, commit, lifecycle_reads, new_snapshot, delta, validation, packet, persist_outcome, envelope`. (Step 2 folds this file into the journal; the struct survives.)
2. Wrap the post-commit phase of `run_agent_use_watch_once_delta` (everything after `clear_agent_use_publish_state`) in `catch_unwind(AssertUnwindSafe(..))`. On panic: emit a minimal structured packet on stdout — `{schema fields, status:"error", validation_state:"incomplete", failed_stage, panic_message, recovery commands}` — and exit nonzero. Invariant: a validate-edit invocation never ends without JSON on stdout.
3. Gate `instrumentation.explain_query_plan` and verbose read-path internals behind `--explain`/`--audit-json` (unresolved-calls surface first, then sweep all agent-use query surfaces). Compact keeps `elapsed_ms` totals only.
4. Re-run the 283 KB rename probe on a fresh release build and capture which stage the exit-255 death lands in (the OOM-from-`usize::MAX`-delta hypothesis vs stack overflow). This measurement decides whether 9.5.3 must precede 9.5.4.

Tests: a test-only failpoint (reusing the existing `CODEGRAPH_WRITE_PATH_FAILPOINT` chaos env with names `agent_use_validation_panic_at_<stage>`, checked at stage starts) → structured error JSON + nonzero exit + stage file present; compact `query unresolved-calls` envelope contains no `explain_query_plan`; existing cli_smoke/compact-contract tests stay green.

> **Implementation status 2026-06-09:** items 1–2 implemented (`cli/validation_journal.rs` new module; tracker + catch_unwind + panic failpoint wired into `run_agent_use_watch_once_delta`; validate-edit passes the pipeline-error packet through with `_cli_exit_code=1` so stdout always carries JSON; the sidecar survives caught panics and hard deaths, and is removed on every clean or structured-error finish via Drop). Unit + end-to-end failpoint tests green. Item 3 (instrumentation gating) deferred to a later slice by request. Full `--lib` suite note: 8 failures present in the shared tree are verified pre-existing (all pass at clean HEAD `0812a81`; filed to the bench agent as Q7).
>
> **Item 4 probe results 2026-06-10 (release binary, codegraph-tool clone, same `open_store_with_preflight` rename, 11 in-file occurrences):**
>
> ```text
> outcome: COMPLETED (first time) — exit 0, status=blocking_graph_error,
>          1 blocking finding, must_fix_before_continuing=true; the exit-255
>          crash did not reproduce, and the prior "hang" is reattributed:
>          the pipeline is simply this slow (the 612 s CPU kill was premature)
> wall: 2,433.8 s total. Stage attribution from the new tracker:
>   preflight 1.3 s | closure 0.4 s | old_snapshot 125.8 s |
>   publish+commit 11.3 s | lifecycle_reads 0.4 s | new_snapshot 364.2 s |
>   delta 0.1 s | validation→envelope ≈ 1,930 s (dominant)
> envelope: 293,278 B compact packet vs declared max_output_bytes=12288,
>   exceeded=true self-reported; `timings` emptied by compaction while 293 KB
>   of other content shipped (the evidence-first inversion, at scale)
> ```
>
> Consequences for sequencing and scope:
> 1. **9.5.3 must precede 9.5.4** — the loop is unusable at production scale
>    (40.5 min/run) until the pipeline is fast and bounded.
> 2. **The §2.2 OOM-from-`usize::MAX`-delta hypothesis is corrected by
>    measurement**: delta compute is 0.1 s. The measured cost centers are
>    (a) the validation/reverification fan-out + packet build (~1,930 s) and
>    (b) the two normalized snapshots (126 s + 364 s = store-read hot spots,
>    §2.3.4). 9.5.3 scope must add per-rule/per-stage instrumentation inside
>    the validation stage and bound the reverification fan-out, in addition
>    to the §2.3.4 SQL fixes.
> 3. The 9.5.5 envelope work has a production-scale fixture now: blocking
>    finding at 24× budget with timings shed before findings detail.

### 8.2 Step 2 — Validation journal (9.5.2a — Q5 atomicity)

1. New module `crates/codegraph-cli/src/validation_journal.rs` (follows the F4 module-split pattern): `ValidationJournal {journal_version, run_id, started_unix_ms, changed_files, closure_files, journal_scope, state, stages, old_facts}` with `state ∈ update_pending | validating | failed:<msg>`; atomic write/load/clear; size cap (compact normalized facts for changed+closure paths, few MB; on overflow journal changed files only + `journal_scope:"changed_files_only"`, replayed closure-dependent rules labeled `bounded`).
2. Reorder `run_agent_use_watch_once_delta` into phases A–F of §2.3.1. The OLD snapshot is already computed pre-commit — serialize it into the journal *before* `write_agent_use_publish_state` (:2595). After commit + `clear_agent_use_publish_state`, transition the journal to `validating` (absorbing the 8.1 stage file). Delete the journal only after the outcome is persisted (8.3).
3. Replay: at validate-edit/watch-once entry (after preflights), a surviving journal in `validating`/`failed` triggers replay FIRST — journaled old facts vs current DB → delta → findings → outcome persisted with origin `journal_replay` → journal cleared — then the requested validation proceeds. `agent-use index` clears the journal (full reindex supersedes; logged).
4. Surfaces: `status`/`doctor`/`watch`/validate-edit report `validation_state:"incomplete"` with the changed files cited while a journal exists. Graph claimability unchanged (§2.3.1 boundary: the committed graph is source-accurate; validation completeness is what is unknown).

Fixtures: failpoint kill between commit and packet → journal present, status incomplete, next run replays and emits the SAME blocking finding (the exact dogfood poisoning case, now recovered); journal overflow → bounded label; `agent-use index` clears it; corrupt journal file → treated as `failed` and surfaced, never a panic.

> **Implementation status 2026-06-10: 9.5.2 IMPLEMENTED (8.2 + 8.3), targeted tests green.**
> `cli/validation_journal.rs` grew the full machinery; `agent_use.rs` wires it. Deviations from the plan above, all deliberate:
> 1. **Sticky outcome lives in a separate sidecar** `production-agent-use.validation-state.json`, not inside the delta-state record — the delta-state writer sits in the bench agent's 14k-line in-flight diff and a separate additive file avoids collision; operationally identical (nothing else read `last_validation`).
> 2. **Stage tracker stays a separate file** rather than being absorbed into the journal: stage marks rewrite their file at every stage start, and the journal carries megabytes of `old_facts` that must not be rewritten 12×/run.
> 3. **Persist/apply seam** is inside `agent_use_exact_calls_validation_packet` (single production caller + the replay path reuse it for free): persisted blockers are re-verified and re-emitted as findings BEFORE packet construction, so severity/hard-interrupt/`--fail-on-blocking` rollups happen through the normal path.
> 4. **Re-verification correctness detail**: entity ids are opaque digests, so missing-target names persist from `EntityDeltaEntry.name/qualified_name`; the "target now exists" graph check counts **definition-kind entities only** (import/export bindings, call sites, locals with the same name must not resolve a blocker — this was caught by the fixture, which initially passed because the consumer's import binding matched).
> 5. **Status surface uses a byte-compact variant** (`{"state":"ok"}` when nothing to report, full block when blocked/incomplete) because the status packet sits exactly at its 12 KiB edge.
> Tests green: crash-after-commit → journal survives (state `failed:*`), status reports `incomplete`, next run replays and re-emits the same blocking finding with exit 2; blocking → rerun unchanged → still blocking with `origin: persisted_open_blocker`; source fixed → cleared + `resolved_blockers_total` counted; `agent-use index` clears journal + rechecks blockers; corrupt journal surfaced + cleared, never a panic. Full suite delta vs the 8 known pre-existing failures: none caused by this work (one transient size regression on the status surface was fixed by deviation 5).

### 8.3 Step 3 — Sticky blockers (9.5.2b — Q6)

1. Extend the delta-state record written by `persist_agent_use_last_delta_state` (:13601) with the additive, schema-versioned `last_validation` block (§3.3 shape).
2. On every run (after/within replay): load `open_blockers`; re-verify each against current store + source via the existing `agent_use_reverify_edge_source_span` family + indexed symbol-dict lookups. Still contradicted → re-emit with `origin:"persisted_open_blocker"` and full blocking semantics (`--fail-on-blocking` exit 2, hard-interrupt eligible). Resolved → cleared + `resolved_blockers_count`. Un-checkable → `unknown` with the reverification failure reason.
3. Compact `validation_state` block `{state: ok|blocked|incomplete, open_blocker_count, since_unix_ms, expansion_handle}` added to status/watch/validate-edit/context-pack + MCP mirrors, and protected in every budget enforcer (evidence-first rule §7.2.1).

Fixtures: §3.3 list verbatim — blocking → rerun unchanged → still blocking; fix source → cleared + counted; different-file run still surfaces blocked state; full `agent-use index` re-checks once; journal replay produces the persisted blocker; blocker file deleted → unknown with reason.

### 8.3.5 Interposed step — Pipeline bounds + store-read perf (9.5.3; sequenced before 9.5.4 per §8.1 probe)

> **Implementation status 2026-06-10: 9.5.3 IMPLEMENTED (§2.3.3 + §2.3.4 + the probe-mandated instrumentation/fan-out scope), targeted tests green. Release-probe gate (§2.3.5) pending.**
>
> Delivered, with deviations from the §2.3 sketch marked:
> 1. **Single delta compute + cap** (`agent_use.rs`): the validation delta cap replaces `usize::MAX` (default 10,000, env `CODEGRAPH_VALIDATION_MAX_DELTA_ITEMS`); the compact view is **derived** from the full report via new `EntitySourceRoleDeltaReport::truncated_to_max_items` (index crate) — the duplicate compact recompute is deleted. Blocking-relevant omission (entities_removed / edges_removed / edges_added) emits an unknown-classified finding under new rule **`CG_MVP3_GRAPH_DELTA_BOUNDED`**, capping `final_status` at unknown through the normal aggregation (not a hand-set status).
> 2. **Per-substage instrumentation** (`ValidationSubstageMeter`): 13 substages inside `agent_use_exact_calls_validation_packet` (lifecycle_integrity, proof_integrity, source_role_tests, activation_gated_contract, the four CALLS fan-out loops, exact_imports, persisted_open_blockers, bounded_labeling, packet_build, persist_outcome) are marked live on the 9.5.1 stage-tracker sidecar (`validation:<name>`, also chaos-failpoint injectable) and summarized with per-substage ms + fan-out counters; the summary ships inline as `validation_substage_summary` in `--explain`/`--audit-json` output and as a final `validation:substage_summary` mark in the sidecar for compact runs.
> 3. **Reverification fan-out bound**: one shared per-run budget (default 10,000, env `CODEGRAPH_VALIDATION_MAX_EDGE_REVERIFICATIONS`) across all four per-edge CALLS loops; exhaustion emits `edge_reverification_bounded` through the same `CG_MVP3_GRAPH_DELTA_BOUNDED` rule. Lifecycle-integrity findings and **persisted-open-blocker re-verification are never skipped** (skipping the sticky lane would resurrect Q6 rerun-forgiveness).
> 4. **Wall budget** (`--max-validation-ms`, env `CODEGRAPH_MAX_VALIDATION_MS`, default 60,000): anchored at watch-once run entry (pre-commit snapshots consume it) and enforced at validation substage boundaries — current item finishes, remaining skippable substages are skipped and named in a bounded-unknown finding. *Deviation from §2.3.3 wording*: the timeout is surfaced as top-level `validation_wall_bounded` + the finding, NOT by overloading the `validation_state` key (that key is the 9.5.2 journal/blocker lane with values ok|blocked|incomplete). Commit semantics are never aborted mid-flight.
> 5. **Store-read fixes** (§2.3.4): `list_edges_by_file` hydrates via compact-key batch resolution + chunked `IN` queries (per-id `get_edge` only as cold fallback for legacy/synthesized ids); the per-path FTS full-scan fallback in `list_text_search_hits_by_file` is skipped when a once-per-store count check proves `file_fts_rows` fully maps `stage0_fts` (legacy DBs keep the fallback, now visible as `fts_file_fallback_scan_sql` in read metrics); the RTDS closure uses new `GraphStore::list_edges_touching_paths` (SQL filter on indexed head/tail columns, limit+1 overfetch so truncation is labeled) instead of `list_edges(limit)` + Rust filtering — the `count_edges`-based premature `degraded` marking is removed (it described the old access pattern).
> 6. **Reverify source cache** (probe-mandated): `agent_use_reverify_source_span_text` read the whole source file per validated edge; contents are now cached per thread keyed by (mtime, len) — an edited file is always re-read, never served stale.
>
> Fixture tests green: `--max-validation-ms 0` → `validation_wall_bounded=true`, status ≠ ok, finding names the budget; delta cap forced to 1 on a 3-target removal → `graph_delta_bounded` labeled, no silent ok; edge budget forced to 0 → `edge_reverification_bounded` labeled, no silent ok; all 9.5.1/9.5.2 fixtures still green.
>
> **§2.3.5 gate run 2026-06-10 (release binary, codegraph-tool clone, `open_store_with_preflight` rename ×11 in-file, 5 consecutive `validate-edit --fail-on-blocking` runs).** The adversarial pass found and fixed a REGRESSION CLASS before the gate could pass, exactly per §7.2.2:
>
> 1. **Bounded-run baseline absorption (found, fixed, fixture added).** With the wall at its original 60 s default, run 1 (real edit, then ~126–151 s) breached the wall → blocking detection skipped (honestly labeled `unknown`/`validation_wall_bounded`) → the run still committed + retired its journal → runs 2–5 reported plain `ok`. That is Q5-poisoning re-introduced through the budget lane. **Fix:** a bounded validation no longer retires its journal (`validation_bounded` note from the packet fn; watch-once keeps the journal, `validation_state` reports `incomplete`), so the next run REPLAYS the pending delta and recovers the findings through the 9.5.2 machinery. Fixture: `agent_use_validate_edit_bounded_run_keeps_journal_and_rerun_recovers_blocking`.
> 2. **Wall default re-sized by measurement: 60 s → 300 s.** The genuine cost of validating a real large-file edit at this repo scale is ~100–130 s; with a 60 s wall even the replay re-bounded itself and the pending delta could never complete (journal thrash, observed live). The wall exists to stop pathological hangs (the 40-min class), not to bound honest work below its real cost. `--max-validation-ms`/`CODEGRAPH_MAX_VALIDATION_MS` still override.
> 3. **Documented residual:** a replay that is ITSELF bounded clears the journal (keeping it cannot survive the next run's own journal overwrite — single journal file); the unknown outcome is persisted + labeled (`replay_validation_bounded`), recovery is `agent-use index`. Rare double-failure case post-(2).
>
> **Final gate numbers (wall 300 s):** run 1 = 120.6 s, `blocking_graph_error`, 1 blocking finding (the removed/renamed-callee transition interrupt), exit 2, `validation_state=blocked`; runs 2–5 = 10.5–10.7 s, status ok with the blocker correctly RESOLVED by re-verification (the rename is self-consistent — all 11 occurrences renamed; `resolved_blockers_total` accounted; genuinely-broken-source stickiness is separately proven by the 9.5.2 sticky fixture). Zero crashes, no wall breach, journal retired cleanly, probe file restored byte-identical, DB resynced (64.4 s, 17,418/72,927 — in line with the 62.5 s pre-change baseline; the one 85.4 s resync included the one-time `idx_entities_entity_hash` build).
>
> **Gate verdict: <10 s p95 NOT met** — edit-run 120.6 s, steady-state 10.5–10.7 s (vs 2,433.8 s pre-9.5.3 = **20–230×**). Every other §2.3.5 criterion met. Post-instrumentation cost centers for the residual perf work (measured, steady-state): first-touch collector hydration ~5 s (proof_integrity + source_role_tests pay cache population), old/new snapshots ~6.3 s incl. multi-MB journal serialize, lifecycle/commit/envelope ~2 s. Additional store fixes landed during attribution: `prepare_cached` for the per-fact UNION selects + 64-slot statement cache; **permanent `idx_entities_entity_hash`** (the build-time index was dropped after bulk builds, leaving the hot id resolver a full-table scan + 2 schema pragmas per call); memoized read-path id resolution; immutable-read-only-connection entity/edge-list caches; line-index source cache. Residual target: share first-touch hydration across collectors + snapshot reads; tracked for a 9.5.3 follow-up slice before 9.5.4 fixtures hammer the loop.
>
> **Residual slice 2026-06-10 (same day, follow-up): <10 s p95 NOW MET.** Re-gate (release binary, same rename probe, fresh resync baseline): run 1 (edit) = **106.1 s**, `blocking_graph_error`, exact span (lib.rs:1499), exit 2; runs 2–5 = **7.8 / 8.0 / 7.9 / 8.0 s** → steady-state p95 ≈ 8.0 s, **under the 10 s target with ~2 s margin**. Probe file restored byte-identical (SHA256 verified), resyncs 64.1–64.3 s (baseline 62.5–64.4 s — index timings unchanged), no stray sidecars. What the slice found and changed:
>
> 1. **The 9.5.3 read caches were dead in production (root cause of the residual).** `sqlite_immutable_read_safe` gates the row caches on the absence of WAL sidecars, and a live poll during a probe run showed `production-agent-use.sqlite-wal`/`-shm` present for essentially the whole run: the write phase creates them and overlapping reader connections keep them alive until process exit, so every mid-run `open_read_only` saw sidecars → `read_cache_enabled=false`. The caches only ever worked in unit tests (no writer precedes the read there). Fix: `SqliteGraphStore::enable_session_read_caches()` — explicit opt-in on a read-only connection whose caller owns the no-concurrent-writer guarantee for the store's lifetime (an agent-use run phase, which owns publish-state writer exclusion). The connection stays a normal locking WAL reader (no unsafe `immutable=1`); only row-level memoization is added. Runtime-confirmed via the new `read_cache_enabled` note in `validation_substage_summary`.
> 2. **One read session per pipeline phase** (`open_normalized_fact_snapshot_session`, index crate): one lifecycle preflight + one cached store now serve ALL reads of a phase — pre-commit (RTDS closure + old snapshot + bounded-journal re-snapshot; session dropped before the write phase, a session never spans a commit), post-commit (new snapshot + every validation collector via new `shared_read_store` param on `agent_use_exact_calls_validation_packet`), and journal replay. Previously closure/old-snapshot/journal-resnapshot/new-snapshot/validation each opened a fresh connection and re-paid hydration for the same files. Measured effect: proof_integrity 2,998 → **122 ms**; window-unaccounted time 3.0 → 0.8 s; validation total ~4.5 → ~2.3 s.
> 3. **Journal serialized once** (`write_validation_journal_bytes`): the size-bound check and the write share one `serde_json::to_vec`; the post-commit `update_pending → validating` transition writes from the in-memory journal instead of reload-parse-rewrite of the multi-MB file. Two new row caches (`get_edge`, `list_entities_by_file`) join the 9.5.3 entity/edges-by-file ones.
>
> Remaining attributed steady-state cost (~8 s, recorded for any future slice — NOT gating): old+new snapshot hydration ~3.7 s (one cold pass each over the changed+closure files on a post-edit DB; inherent unless snapshots get incremental), `source_role_tests` ~2.3 s (per-edge entity resolution over 666 current edges), fixed lifecycle/preflight/commit ~1.1 s. Suite: store 49/0, index 236/0, cli targeted green; full `--lib` gate = no NEW failures vs the 8 known bench-WIP ones (Q7).
>
> **Suite-gate side find (2026-06-11, test hygiene — fixed):** the first two full parallel `--lib` runs showed 10 failures (8 known + 2 rotating failpoint tests, all passing in isolation). Root cause: `CODEGRAPH_WRITE_PATH_FAILPOINT` is process-wide and 8 of its 9 setters serialize on `ENV_TEST_LOCK`, but `bundle_replace_failpoints_preserve_old_db_and_success_replaces_atomically` held only `BUNDLE_TEST_LOCK` — so it raced the agent-use failpoint tests and they clobbered each other's failpoint ("replace failpoint must fail" fired because the env was gone). Pre-existing race; the 9.5.3 speedups merely shifted timing so the overlap landed reliably. Fix: that test now also takes `lock_env_test()` (bundle→env ordering only, no reverse path → no deadlock). Verified: parallel failpoint subset 1 passed/2 failed → 3/3; full parallel `--lib` = **394 passed / 8 failed = exactly the known Q7 set. Suite gate met.**

### 8.4 Step 4 — Unresolved Reference Lane (9.5.4 — Q4; next package, planned now)

The extraction side already exists end to end: the parser emits a reference entity + `StaticHeuristic` CALLS edge for every unresolved callee (`callee_entity`, parser lib.rs ~2503/~5582), `LocalFactBundle::new` (index lib.rs ~5932) bundles them into `unresolved_references`, and the `unresolved_references` table + per-file invalidation already exist in every DB (store sqlite.rs ~9942, ~6713). The work is a persistence-lane decision plus the escalation ladder:

1. **index**: split `persist_debug_sidecars` (~11889) — new `persist_unresolved_reference_lane` writes `bundle.unresolved_references` in ALL storage modes (the `preserves_heuristic_sidecars` gate at ~11763 no longer applies to this lane; `static_references`/`heuristic_edges` stay Audit/Debug-only). Relation filter (Calls/Callee/Imports/alias/reexport), 256-rows/file cap + `extraction_warnings` truncation row on overflow.
2. **index**: `reference_class` computed at bundle build time (§1.3.2 rules: qualification shape, import bindings, workspace dependency manifests, per-language builtin lists, Tier-2 language guard), stored in the row's `metadata_json` — no schema migration.
3. **index**: snapshot + delta extension (§1.3.3) — `include_unresolved_references` snapshot option (on for validate-edit/watch), identity key `(path, span, relation, name)`, BTreeMap delta classifier, non-proof labeling.
4. **cli**: `CG_MVP3_REF_*` rule family (§1.3.4) with the validation-time symbol-dict escalation lookup; warning ceiling enforced through the MVP3.6 severity model; `unresolved_references` packet block (§1.3.5) wired into validate-edit/watch/MCP with its counts in the budget enforcer's protected set.
5. **query/cli**: `query unresolved-calls` reads the populated lane with `--class`/`--path` filters, bounded.

Gate: the §1.3.6 fixture table with **Python/JS forward cases gating first** (per §7.2.3 — the no-compiler niche is the headline value), then Rust; storage-budget measurement on the codegraph-tool clone rebuild (record actuals before fixing thresholds); MVP3.9 Hallucination Interrupt Gate re-run with forward cases and claim text updated to cover both directions.

> **Implementation status 2026-06-11 — steps 1–5 IMPLEMENTED (gate 9.5.4f pending).** All suites green at each step: store 49/0, index 241/0, core 183/0, cli full `--lib` 397/8 = exactly the known Q7 bench-WIP failures. Notes and deliberate deviations:
>
> 1. **(8.4.1) Lane persists in BOTH write paths.** The incremental path (`update_changed_files_with_cache_to_db`) — the one validate-edit actually uses — persisted nothing to `unresolved_references` in ANY mode; the plan's `persist_debug_sidecars` split alone would have fixed only full indexes. The lane persister is called from `persist_local_fact_bundles` AND inline in the incremental loop. Side find: lane rows stored opaque digest names (`reference_display_name` predates the digest-id migration); names now resolve from the extraction's entity map (`unresolved_reference_for_edge_named`) so rows carry `auth::revoke_token`, not `afc5b8...`. CALLEE rows mirror CALLS rows at the same callsite — rule/packet consumers dedupe by skipping CALLEE; raw lane counts do not.
> 2. **(8.4.2) Classification runs at persist time, not bundle build time** (same extractor-local inputs, plus manifests): bundle construction is threaded (`thread::spawn`) and the classifier holds a filesystem-memo `RefCell`; the single-threaded persist path classifies each row into `metadata_json.reference_class`. `UnresolvedReferenceClassifier::for_repo` parses Cargo.toml/package.json/pyproject (bounded walk, depth 4 / 128 manifests) for dependency roots AND workspace member names (member crate paths classify repo-local). Sibling-module checks are filesystem-existence probes memoized per (dir, segment).
> 3. **(8.4.3) As planned** plus `#[serde(default)]` on the new snapshot/report/options fields so pre-9.5.4 validation journals still parse (an in-flight journal across the upgrade replays without lane facts — one-time, documented).
> 4. **(8.4.4) Warning ceiling verified against the MVP3.6 severity model both ways.** Default path: `CG_MVP3_REF_NEW_UNRESOLVED_LOCAL_CALL/_IMPORT` findings carry non-graph evidence → severity Warning, never blocking, exit 0 even with `--fail-on-blocking` (fixture-proven). Promotion path (`--block-on-unresolved-local` flag / `CODEGRAPH_BLOCK_ON_UNRESOLVED_LOCAL` env, default off): a Block-classified finding with only non-graph evidence is DEMOTED by `severity_non_graph_only` (correct guard) — the promoted finding therefore cites the same fresh contradiction pair the 9.5.2 sticky blockers cite (current source references the span + fresh whole-graph lookup finds no defining entity) as graph-source evidence; fixture proves exit 2. `entity_kind_defines_symbol` moved to codegraph-core (shared by cli + mcp). The §1.3.5 block rides INSIDE `ValidationPacket` (new `unresolved_references` field, `#[serde(default)]`) so it flows to watch/validate-edit/replay without per-surface copying; validate-edit also copies it top-level (budget-enforcer survival, like `validation_substage_summary`). MCP `codegraph.validate_edit` mirrors the block (counts + top-3 escalated with the lookup) via `mcp_validate_edit_unresolved_references_block`; CG_MVP3_REF warning-finding parity in the MCP packet is NOT yet implemented (the MCP packet builder is a separate implementation) — acceptable v1, revisit if MCP becomes a primary agent surface.
> 5. **(8.4.5) `query unresolved-calls`** keeps the legacy `calls` array (Audit/Debug heuristic-edge sidecar, empty on Proof DBs — that contract is unchanged) and gains a `unresolved_references` block reading the lane (populated in all modes) with `--class`/`--path` filters, bounded by the same limit/offset.
>
> New fixtures (all green): proof-mode lane persistence, incremental persist + per-file invalidation, 256-cap + truncation warning, 23-case classification matrix, snapshot/delta added/resolved + proof-ladder guard, validate-edit warns-then-clears (Q4 flagship, JS), builtin-stays-quiet, promotion exit 2, lane query with class/path filters.
>
> **Pre-existing failure surfaced (NOT caused by 9.5.4; parked for 9.5.5):** `codegraph-mcp-server` test `context_pack_mcp_default_is_compact_and_bounded` fails at 16,731 B vs its 16 KiB bound — accumulated envelope bloat (`rtds_freshness` 2,720 B, `staged_availability` 2,430 B, `refreshed_evidence`/`task_profile`/`unavailable_evidence` ~950 B each), none of it lane-related (key-size audit confirmed; today's keys don't appear). First mcp suite run since the 9.5.x stretch began. This is §4 envelope-enforcement scope — fix the budget there, do not bump the test bound.
>
> **9.5.4f gate results (2026-06-11).**
>
> *Fixture matrix (§1.3.6):* Python first — same-file undefined call, sibling-module undefined call → escalated warnings; `print` quiet; fix clears with `resolved_count≥2`. Rust — cross-module `auth::revoke_token`, same-file `audit_login_attempt`, and `use crate::auth::nonexistent_thing` all escalate; `println!` (macro) and `serde_json` (declared dep) quiet; exit 0 with `--fail-on-blocking` (warning ceiling). JS — builtin/dynamic/hallucinated split correct. Promotion fixture: exit 2. **Product gap found+fixed during the gate:** the parser emitted NO fact for unresolved import targets (only the local binding entity) — both extractor impls now emit the import TARGET as a reference entity + StaticHeuristic IMPORTS edge (`unresolved_import_target_label`: Rust use-paths, Python module.name, Go specifiers; JS relative-path specifiers deferred — they classify dynamic). **Deliberate deviation from §1.3.4:** for IMPORTS, candidates-exist does NOT warn (the parser cannot see cross-file resolution, so candidates-exist is the normal state of every valid new import; only the no-definition case escalates). Calls keep the both-tier contract. Parser suite 82/0.
>
> *Storage budget (codegraph-tool clone, fresh `--fresh` rebuild, release binary):* 165 files / 17,413 entities / 72,848 edges in 60.2 s (56-64 s baseline band — no index-time cost). Lane: **5,409 rows ≈ 2.4 MB on a 43.7 MB fresh DB (5.5%)** — within the single-digit-MB budget; recorded actuals: by class dynamic_or_computed 4,343 / repo_local_candidate 842 / builtin_or_std 167 / external_dependency 57; by relation CALLS 3,380 / CALLEE 1,945 / IMPORTS 84. NOTE: fresh-rebuild graph counts drifted slightly vs the 2026-06-09 baseline (17,418/72,927 → 17,413/72,848; −5 entities/−79 edges) — accumulated across the 9.5.x sessions, not investigated; the bench graph-truth harness owns count regression detection.
>
> *Release probes (clone):* forward probe on `core/src/ids.rs` — same-file `fabricated_token_helper()` + cross-module `crate::validation::nonexistent_probe_fn()` both escalated with exact spans and `no_defining_entity_named_*` lookups, status `warning`, exit 0, 2.8 s. Rerun on unchanged broken source → `ok`, `new_count=0`: **warnings are delta-triggered and non-sticky by design** (only block-class findings persist via 9.5.2; the promotion opt-in inherits stickiness). Probe file restored SHA-identical; resynced; no stray sidecars; validation-state ok/0 blockers.
>
> *Perf:* the new `unresolved_references` substage measured **0 ms** on steady-state runs (`validation_substage_summary`, audit run). Wall numbers were contaminated by concurrent bench-agent load (70% CPU): fresh-DB steady 3.8-3.9 s; post-edit steady 9-11 s vs the 8.0 s idle baseline. No lane-attributable regression is visible in the substage attribution; the formal §2.3.5 p95 re-gate should be re-run on an idle machine before any perf claim is refreshed.
>
> *Adversarial pass (fresh Py+JS repo, never used in development, release binary):* (a) typo of a real sibling fn (`db.save_recrod`) → escalated "likely hallucinated symbol" — the highest-value real case works; (b) call-shaped text in comments and strings produced nothing (no text-bait false positives); (c) `window[action](payload)` computed → dynamic/quiet, `console.log` → builtin/quiet; (d) fix run cleared everything with `resolved_count=5`. **Main adversarial finding (filed, decision pending):** the §1.3.4 candidates-exist plain-wording warning fires on every VALID new cross-module call — including the very call that fixes a typo (the fix run warned on corrected `db.save_recrod`→`db.save_record`). This is the documented spec contract, but it is real noise; recommend deciding (human/MVP_3.md) whether candidates-exist CALLS should downgrade to diagnostic like imports once more dogfood data exists.
>
> *Claim text:* MVP_3.md §13 Hallucination Interrupt Gate now states both directions explicitly (backward blocking; forward escalated-warning ceiling + policy opt-in).

### 8.5 Working rules for the package

Release-binary smoke against disposable TEMP fixture repos + the `..\codegraph-tool` clone profile DB; resync with `agent-use index` after every manual probe until 8.2 lands (validate-edit still mutates the baseline); no normal-`.codegraph` mutation; reports under `reports/audit/`; no staging/commits/pushes by agents; each step ends with targeted tests + the existing MVP3.8 matrix; the package ends with an adversarial live-use pass (§7.2.2) on a repo not used during development.

### 8.6 Re-dogfood preflight blocked (2026-06-12)

Step-0 re-dogfood was started against the mixed `fix-f1` workspace and the
existing release binary (`target\release\codegraph-mcp.exe`, 45,540,864 bytes,
SHA256 `077A9ED3976AB52325709766DB44A684D774AE8C51AAE84ED1C60C6DF612BB98`,
`codegraph-mcp 0.0.0 (unknown)`). The run stopped before mutating or reindexing
the `..\codegraph-tool` clone because the required pre-dogfood suite gate was
red:

`cargo test -p codegraph-cli --lib -- --test-threads=1`

Result: **FAILED**, 409 passed / 11 failed / 420 total, finished in 312.18s.
This is a dogfood blocker, not a readiness signal. The failures are directly in
the surfaces the live-use pass needs to trust:

- `agent_use_validate_edit_crash_after_commit_replays_blocking_finding`
- `agent_use_validate_edit_bounded_run_keeps_journal_and_rerun_recovers_blocking`
- `agent_use_validate_edit_exhausted_edge_budget_is_labeled_bounded`
- `agent_use_validate_edit_low_delta_cap_is_labeled_graph_delta_bounded`
- `validate_edit_forward_fixture_matrix_rust`
- `agent_use_watch_once_updates_external_profile_db_without_dot_codegraph`
- `cli_surface_outputs_validation_packet`
- `agent_use_watch_once_dirty_sidecars_do_not_masquerade_as_fresh_context`
- `agent_use_multi_repo_local_agents_do_not_cross_contaminate_during_update`
- `agent_use_profile_durability_labels_lock_sidecars_permission_and_publish_state`
- `agent_use_readers_see_old_good_db_during_uncommitted_delta_update`

Most relevant observed failure shape: the Rust forward-reference matrix reached
`final_status="warning"` and `severity_summary.counts_by_severity.warning=3`,
but compact top-level `unresolved_references.escalated` exposed only the import
entry while omitting the two CALLS entries (`auth::revoke_token` and
`audit_login_attempt`) behind `escalated_omitted_count=2`; the nested
`validation_packet.unresolved_references` still contained all three. That keeps
the findings-anchor contract unsettled for an ordinary agent consuming compact
JSON. The lifecycle/journal/bounded tests being red also invalidates using this
binary/tree combination for a real clone mutation pass.

No `agent-use index --fresh`, validate-edit mutation, normal `.codegraph`
mutation, stage, commit, push, public claim, benchmark claim, real-agent
patch-quality claim, or MVP3.10/MVP4 work was performed in this blocked pass.

### 8.7 Supersession of the 8.6 blocked preflight (2026-06-12)

The 8.6 blocked preflight is historical evidence, not the current active
state. It is superseded by the completed MVP3.9.5 final dogfood regression gate,
the fresh MVP3.9 gate rerun, the Benchmark v1 pre-MVP4 decision, and the MVP3.10
Route/Bridge quality gate recorded in `reports/final/PRODUCT_READINESS.md`.

Direct superseding check: `reports/audit/artifacts/mvp3_9_5_dogfood_closure/original_failure_final_retest_summary.json`
shows all 11 previously failing tests passed, including the Rust forward
unresolved-reference fixture, crash-after-commit replay, bounded-run replay,
edge/delta-budget labeling, external-profile packet shape, dirty-sidecar
freshness, profile durability, old-good DB read safety, and multi-repo
isolation cases.

Current release binary for follow-up dogfood review: `target\release\codegraph-mcp.exe`,
45,616,640 bytes, SHA256
`D69D6CF725F1AE16858DBBAD29101EDFE4B0D91802CB58C0B38E28C0A1897CDE`,
`codegraph-mcp 0.0.0 (unknown)`. The next allowed work is additional dogfood
hardening / Benchmark v1 external unblock; MVP4 remains not ready and is not
started from this evidence.

### 8.8 Additional `codegraph-tool` dogfood review (2026-06-12)

Follow-up review used a clean disposable clone at
`benchmarks/workspaces/codegraph-tool-dogfood-20260612-001` because the original
`..\codegraph-tool` worktree had pre-existing dirty UI work. The release binary
remained SHA256
`D69D6CF725F1AE16858DBBAD29101EDFE4B0D91802CB58C0B38E28C0A1897CDE`.

Superseding confirmation gates passed before the dogfood loop:

- `cargo test -p codegraph-mcp-server --lib -- --test-threads=1`: 59/59 passed.
- `cargo test -p codegraph-cli context_pack --lib -- --test-threads=1`: 48/48 passed.
- `cargo test -p codegraph-cli unresolved --lib -- --test-threads=1`: 5/5 passed.
- `cargo test -p codegraph-cli budget --lib -- --test-threads=1`: 13/13 passed.
- `cargo build --release --bin codegraph-mcp`: passed.

Fresh production-profile indexing required LocalAppData write access and
completed in 51.8 s against the disposable clone: 165 indexed files, 90,261
source spans, 43.7 MB DB, candidate spool/query sidecars present, vector runtime
present, no normal `.codegraph` creation.

Real edit-loop result:

- Intentional unresolved calls in `crates/codegraph-core/src/ids.rs` produced
  `validate-edit` status `warning`, exit 0, two escalated repo-local CALLS
  findings with exact spans (`fabricated_token_helper`, line 32;
  `fabricated_relation_hint`, line 33), no blocking error, no `.codegraph`
  mutation.
- Adding definitions for those helpers cleared the findings:
  `validate-edit` status `ok`, exit 0, warning count 0, escalated unresolved
  total 0.
- Restoring the original file left the disposable clone clean and no
  `.codegraph` existed. Final elevated `agent-use status` was `ok` with graph
  freshness current and the external DB at
  `%LOCALAPPDATA%\CodeGraphMCP\agent-indexes\codegraph-tool-dogfood-20260612-001-e4cc966adef49729fcb46f5d532388e7\production-agent-use.sqlite`.

New follow-up issues from the review:

1. Non-elevated/sandboxed status and mcp-config resolved the same clone to a
   different production profile identity (`bb1b04...`) than elevated
   write-capable commands (`e4cc96...`), causing `status=not_indexed` despite an
   indexed DB. This appears tied to Windows extended-path/Git remote identity
   resolution and should be fixed before relying on agent-use profile continuity
   in restricted agent environments.
2. `agent-use query unresolved-calls <symbol>` is advertised by top-level help
   but rejected by the actual query parser; the working shape is filter-based
   (`--path`, `--class`). Help/docs need alignment.
3. `validate-edit` reported the two unresolved calls, but
   `agent-use query unresolved-calls --path crates/codegraph-core/src/ids.rs`
   and the unfiltered query returned zero calls. The validation packet and query
   surface are not aligned for this live profile.
4. After restoring original source, `validate-edit` returned top-level
   `status=unknown` even with zero warnings/blocking errors, validation state
   `ok`, open blockers 0, and only dynamic/computed unresolved references. The
   severity/status mapping should be tightened.
5. `query callers stable_entity_id` and `query callees stable_entity_id`
   returned warnings/no proof path while symbol and file queries worked. This is
   a relation-query proof/readiness gap for the dogfood corpus.
6. `watch --once` after edits returned `degraded` because candidate/vector
   sidecars were stale/truncated, while validation itself was `ok`. Final status
   refreshed candidate state but vector remained stale; recovery guidance for
   optional sidecars should be more direct.
