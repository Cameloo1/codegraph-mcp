# Architecture Overview ASCII Chart Variants

Status: draft options for replacing the current ASCII chart in
`docs/architecture.md`. These variants are intentionally separate from the
public architecture page until one is selected.

## Update Plan

1. Keep the chart centered on current verified behavior, not future claims.
2. Show the MVP3 validation loop as a first-class agent-use surface.
3. Show MVP4.2 micro-edges as a proof-enrichment foundation, not full
   `flow_proof`, `mutation_proof`, or local-flow packets.
4. Keep normal search, editing, and tests in the agent workflow.
5. Preserve the proof boundary: candidate lanes route attention; lifecycle-valid
   graph/source evidence decides claimability.
6. Choose one chart later, then replace only the current chart block in
   `docs/architecture.md`.

## Shared Terms For All Options

- `agent-use`: production profile for coding agents, with an external DB outside
  the source tree.
- `status`: read-only lifecycle and recovery preflight.
- `index`: explicit full graph build or refresh.
- `watch`: changed-file delta update over the same external profile DB.
- `validate-edit`: MVP3 linter-style validation packet for changed files.
- `context-pack`: compact planning/investigation packet.
- `candidate lanes`: text, symbol, path, vector, binary, nuance, and spool
  retrieval signals. They are not graph proof by themselves.
- `MVP4.2 micro-edge`: current exact `LOCAL_RETURNS_TO` structural containment
  for TypeScript `.ts`, persisted in `ast_micro_edges` and surfaced through
  status, doctor, audit, validate-edit, CLI, and MCP.
- `future MVP4 flow`: local flow and mutation proof remain future work until
  exact micro-node expansion, binding resolution, and packet gates are complete.

## Option 1 - Minimal Change

This keeps the shape of the current chart, but updates labels for MVP3 and
MVP4.2.

```text
                         +----------------------+
                         |     coding agent     |
                         | search / edit / test |
                         +----------+-----------+
                                    |
                                    | CLI / MCP
                                    v
                         +----------------------+
                         | agent-use status     |
                         | index / watch        |
                         +----------+-----------+
                                    |
        +---------------------------+---------------------------+
        v                           v                           v
+-----------------+       +---------------------+      +------------------+
| Typed graph     |       | Candidate lanes      |      | MVP3 validator   |
| entities/edges  |       | text/vector/path/    |      | validate-edit    |
| spans/provenance|       | binary/nuance/spool  |      | blockers/unknown |
+--------+--------+       +----------+----------+      +--------+---------+
         |                           |                          |
         +---------------------------+--------------------------+
                                     |
                                     v
                         +----------------------+
                         | graph/source verify  |
                         | proof or no proof    |
                         +----------+-----------+
                                    |
                 +------------------+------------------+
                 v                                     v
      +----------------------+              +----------------------+
      | compact context      |              | MVP4.2 micro-edge    |
      | micro-flow packets*  |              | MVP3 linter signal   |
      | planning packet      |              | containment only     |
      +----------------------+              | no value-flow proof  |
                                            +----------------------+
```

`*` Micro-flow packets are a future packet lane in this chart. Current MVP4.2
data is `LOCAL_RETURNS_TO` structural containment only, surfaced through MVP3
validation/status/watch/MCP paths without activating `flow_proof`.

Best when: the architecture page should remain familiar and low-risk.

Tradeoff: MVP3 validation and MVP4.2 micro-edges are visible, but the workflow
loop is still less obvious.

### Option 1 Output-Box Label Alternatives

These are phrasing choices for the two bottom boxes before final selection.

Recommended current-safe version:

```text
+----------------------+              +----------------------+
| compact context      |              | MVP4.2 micro-edge    |
| micro-flow packets*  |              | MVP3 linter signal   |
| planning packet      |              | containment only     |
+----------------------+              | no value-flow proof  |
                                      +----------------------+
```

Linter-first version:

```text
+----------------------+              +----------------------+
| context + packet     |              | MVP3 linter loop     |
| micro-flow path*     |              | MVP4.2 micro-edge    |
| plan/investigate     |              | blockers + unknowns  |
+----------------------+              +----------------------+
```

Proof-boundary version:

```text
+----------------------+              +----------------------+
| compact packet       |              | linter evidence      |
| future flow packet*  |              | LOCAL_RETURNS_TO     |
| planning context     |              | containment only     |
+----------------------+              +----------------------+
```

Product-facing version:

```text
+----------------------+              +----------------------+
| agent context packet |              | post-edit guardrail  |
| micro-flow packets*  |              | micro-edge checks    |
| plan + inspect       |              | recovery guidance    |
+----------------------+              +----------------------+
```

MVP4-forward version:

```text
+----------------------+              +----------------------+
| planning packet      |              | micro-edge layer     |
| micro-flow packets*  |              | linter integration   |
| context surface      |              | flow proof later     |
+----------------------+              +----------------------+
```

The safest final choice is the recommended current-safe version. The
product-facing version reads best for a less technical audience, but it hides
the exact `LOCAL_RETURNS_TO` boundary.

## Option 2 - Separate Systems, One Shared Proof Line

This separates the major systems while keeping one horizontal line as the
simplifying concept.

```text
 normal tools             codegraph surfaces              validation outputs
+-------------+          +-------------------+           +------------------+
| rg/read/edit|          | agent-use CLI/MCP |           | context-pack     |
| tests       |          | status/index/watch|           | validate-edit    |
+------+------+          +---------+---------+           +---------+--------+
       |                           |                               |
       +---------------------------+-------------------------------+
                                   |
                    shared proof/evidence spine
                                   |
   lifecycle-valid SQLite graph + source spans + provenance + proof labels
                                   |
       +---------------------------+-------------------------------+
       |                           |                               |
+------v------+          +---------v---------+           +---------v--------+
| candidate   |          | typed graph       |           | MVP4.2 local     |
| retrieval   |          | entities/relations|           | micro-edge layer |
| lanes       |          | exactness/source  |           | returns contain. |
+-------------+          +-------------------+           +------------------+
```

Best when: the page needs a clean mental model: many systems, one evidence
spine.

Tradeoff: less chronological than a workflow diagram.

## Option 3 - Agent Workflow Loop

This focuses on when an agent uses CodeGraph during a real task.

```text
                          assigned task
                              |
                              v
                   +----------------------+
                   | normal investigation |
                   | rg / files / tests   |
                   +----------+-----------+
                              |
                              v
                   +----------------------+
                   | codegraph context    |
                   | status + context-pack|
                   +----------+-----------+
                              |
                              v
                   +----------------------+
                   | plan and edit        |
                   | normal source tools  |
                   +----------+-----------+
                              |
                              v
                   +----------------------+
                   | MVP3 validate-edit   |
                   | blockers/warnings/   |
                   | unknowns/recovery    |
                   +----------+-----------+
                              |
                   fix / retry / recheck
                              |
                              v
                         answer done

 side rail:
 candidate lanes -> graph/source verification -> proof labels
 MVP4.2 LOCAL_RETURNS_TO -> extra exact containment signal where available
```

Best when: the goal is to teach agent behavior and practical usage.

Tradeoff: less detail about storage and internal components.

## Option 4 - Proof Ladder View

This makes the evidence boundary the main point.

```text
 attention routers
 +---------------------------------------------------------------+
 | rg/text | symbols | path | vector | binary | nuance | spool   |
 +-------------------------------+-------------------------------+
                                 |
                                 v
 candidate evidence, source-navigation evidence, matched seeds
                                 |
                 not proof until verified
                                 |
                                 v
 +-------------------------------+-------------------------------+
 | lifecycle-valid typed graph   | source spans + provenance      |
 | exactness + source roles      | path evidence where present    |
 +-------------------------------+-------------------------------+
                                 |
                                 v
 +-------------------------------+-------------------------------+
 | graph relation proof or source-text/no-proof fallback          |
 +-------------------------------+-------------------------------+
                                 |
            +--------------------+--------------------+
            v                                         v
 +----------------------+              +--------------------------+
 | MVP3 validation      |              | MVP4.2 micro-edge signal |
 | severity + recovery  |              | LOCAL_RETURNS_TO only    |
 +----------------------+              +--------------------------+
```

Best when: the architecture page should strongly prevent proof overclaims.

Tradeoff: less friendly as a product onboarding diagram.

## Option 5 - Data And State View

This focuses on DBs, sidecars, and state transitions.

```text
 source repo
    |
    v
+-------------------+        +-----------------------------+
| parser/indexer    |        | agent-use profile resolver  |
| tree-sitter + FTS |------->| external DB outside repo    |
+---------+---------+        +-------------+---------------+
          |                                |
          v                                v
+-------------------+        +-----------------------------+
| SQLite graph DB   |        | lifecycle/passport/status   |
| entities/edges/   |        | safe/stale/foreign/unsafe   |
| spans/provenance  |        +-------------+---------------+
+---------+---------+                      |
          |                                v
          |                  +-----------------------------+
          |                  | reads and updates            |
          |                  | query/context/watch/validate |
          |                  +-------------+---------------+
          |                                |
          +--------------------------------+
                           |
                           v
             +-----------------------------+
             | bounded agent JSON packets  |
             | proof labels + recovery     |
             +-----------------------------+

 optional bounded sidecars:
 candidate spool | vector/text artifacts | MVP4.2 ast_micro_edges
```

Best when: the page should explain why the production agent-use profile is safe
and why state does not belong in normal source directories.

Tradeoff: less emphasis on agent reasoning.

## Option 6 - Compact README-Style Version

This is the shortest front-facing chart.

```text
 agent uses normal tools
        |
        v
 agent-use status / index / watch
        |
        v
 typed graph + candidate lanes + source spans
        |
        v
 graph/source verification
        |
        +--> context-pack for planning
        |
        +--> validate-edit for blockers, warnings, unknowns
        |
        +--> MVP4.2 LOCAL_RETURNS_TO containment signal where available

 candidate lanes suggest; graph/source verification decides claimability
```

Best when: the architecture page needs a simple replacement and the deeper
details can stay in the surrounding prose.

Tradeoff: too compact if readers need to understand the internal subsystems.

## Recommendation

Use Option 2 if the goal is a durable architecture diagram. It shows the new
systems separately without making the page feel like a wall of boxes, and the
single shared proof/evidence line gives the reader one simple rule.

Use Option 1 if the goal is minimal churn.

Use Option 3 as a companion chart in `docs/agent-use.md`, not necessarily as
the architecture overview.
