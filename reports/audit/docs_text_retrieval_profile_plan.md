# Docs/Text Retrieval Profile Plan

Timestamp: 2026-05-15 23:49:48 -05:00

## Source Of Truth

- `MVP.md` was read before planning.
- Active worktree note: `MVP.md` is not present in `C:\Users\wamin\.codex\worktrees\4160\codegraph-mcp`; the canonical sibling file at `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read.
- No implementation was done in this pass beyond this plan/report.
- The requested AGENTS.md MVP timestamp was not written because the active worktree does not contain `MVP.md`, and writing the sibling checkout would be outside this workspace.

## Current State Observed

- `context-pack` currently accepts free-form `--mode`, but the implementation uses graph/path-evidence relation filters. It does not define `docs` or `repo-text` as first-class text modes.
- `codegraph-store` already has `stage0_fts` for file/entity/snippet BM25 rows, but the compact proof profile intentionally avoids redundant full source/snippet storage.
- Index scope treats source-bearing `docs/` as allowed for source-like files, while generated report payloads such as `reports/final/artifacts`, raw CGC comparison payloads, smoke logs, DB files, WAL/SHM, and logs are excluded.
- Markdown, PowerShell, stable report summaries, README, and config files are not currently modeled as typed graph proof. That is correct and should remain true.

## Principle

Docs/report retrieval is useful evidence for agent work, but it is not typed program graph proof.

The evidence class must be:

- `text_evidence`
- not `proof_edge`
- not `path_evidence`
- not `CALLS`, `READS`, `WRITES`, `FLOWS_TO`, or any other typed relation claim

Any output combining docs and graph must keep them in separate sections with separate claim labels.

## Docs/Text Profile Definition

Add a first-class profile named `docs_text` with these corpus categories:

| Category | Include |
| --- | --- |
| Markdown | `*.md`, `*.mdx` |
| README | `README.md`, `README.*`, repo-root overview docs |
| Docs | `docs/**`, `reports/audit/README.md`, stable documentation pages |
| Reports | stable compact summaries under `reports/audit/*.md`, `reports/final/*.md`, `reports/comparison/*.md` |
| PowerShell/scripts | `*.ps1`, `*.psm1`, selected `scripts/**` shell/Python/PowerShell operational scripts |
| Config | `*.toml`, `*.yml`, `*.yaml`, `*.json`, `*.jsonc`, `*.ini`, `.github/workflows/*.yml`, install/package metadata |

Default exclusions for `docs_text`:

- DBs: `*.sqlite`, `*.sqlite3`, `*.db`, WAL/SHM sidecars
- Raw logs: `*.log`, smoke logs, trace payloads unless explicitly requested
- Generated payloads: `reports/final/artifacts/**`, `reports/audit/artifacts/**`, raw CGC full-run/recovery payloads
- Build/dependency dirs: `target/**`, `node_modules/**`, `.git/**`, `.venv/**`, `dist/**`, generated UI bundles
- Secrets and local env files: `.env`, `.env.*`, credentials, tokens

Stable generated summaries may be included; bulky generated payloads stay out.

## Storage Plan

Prefer a separate text sidecar DB by default:

- Default path: `.codegraph/codegraph_text.sqlite` for local self-test profile.
- Production-agent profile path: outside the repo under LocalAppData, beside but distinct from the production proof DB.
- Passport: separate `codegraph_text_passport`, with repo root, git identity, docs scope hash, source discovery policy, schema version, chunking version, and freshness.
- Lifecycle: same exact-path preflight family as proof DB reads, but output claimability is `claimable_text_evidence`, never `claimable_graph_proof`.

Rationale:

- Keeps compact proof DB size and proof-gate metrics uncontaminated.
- Allows more aggressive chunking and full-text indexing without changing graph schema or proof semantics.
- Lets docs/report indexing be rebuilt independently when reports change.

Fallback option for a smaller implementation:

- Add dedicated text tables in the existing DB only when `--docs-text` or equivalent is explicitly enabled.
- These tables must be named and gated clearly, for example `text_documents`, `text_chunks`, `text_fts`.
- They must never feed `edges`, `path_evidence`, `derived_edges`, or typed relation counts.

## Context Modes

Add explicit context modes:

- `context-pack --mode docs`
- `context-pack --mode repo-text`

Mode behavior:

- `docs`: prioritize docs/report/config/script text snippets. Return `text_evidence` sections only unless explicit graph seeds are also supplied.
- `repo-text`: search broad repo text corpus and return snippets; useful for README, scripts, config, report, troubleshooting, and architecture tasks.
- Existing graph modes such as `impact`, `test-impact`, and `debug` keep graph/path-evidence semantics unchanged.

Output sections:

- `text_evidence_results`
- `graph_proof_results`
- `mixed_context_summary`
- `warnings`

Required invariant:

- `text_evidence_results` must not be copied into `verified_paths`.
- `graph_proof_results` must not include docs-only snippets unless they are attached as non-proof explanatory context.

## Output Schema

Each text result should include:

- `evidence_class: "text_evidence"`
- `claim_strength: "text_match"`
- `corpus_profile: "docs_text"`
- `repo_relative_path`
- `start_line`
- `end_line`
- `snippet`
- `title`
- `text_match_score`
- `matched_terms`
- `source_kind`: `markdown | powershell | script | report | readme | config | docs`
- `generated_status`: `source_doc | stable_summary | generated_payload_excluded`
- `claimable_as_graph_proof: false`
- `relations_claimed: []`

Do not emit graph relation labels for text-only evidence.

## CLI Plan

New or extended commands:

- `codegraph-mcp text-index --profile docs-text [repo]`
- `codegraph-mcp query text --profile docs-text <query>`
- `codegraph-mcp context-pack --mode docs --task <task>`
- `codegraph-mcp context-pack --mode repo-text --task <task>`

Keep `rg`-style workflow compatibility:

- Include exact paths and line spans.
- Provide enough snippet context to replace many manual `rg` passes.
- Keep raw matching explainable: BM25 score, matched terms, chunk id, chunk version.

## MCP Plan

Extend MCP without weakening existing tools:

- `codegraph.search_text` accepts `corpus_profile: "proof" | "docs_text" | "all_text"`.
- `codegraph.context_pack` accepts `mode: "docs" | "repo-text"` and returns `text_evidence_results`.
- `codegraph.status` reports text sidecar status separately from proof DB status.

MCP safety:

- Search/analyze tools must keep proof and docs lanes separate.
- Status must say if docs text index is stale/missing without marking graph DB unsafe.
- If docs DB is unsafe or stale, return `text_index_problem`, not `db_problem` for the proof DB.

## Indexing And Chunking Plan

Chunking defaults:

- Markdown: chunk by headings, with fallback paragraph windows.
- PowerShell/scripts: chunk by function, comment block, or bounded line window.
- Reports: chunk by heading section, keep report title/date/verdict metadata.
- Config: chunk by top-level keys or bounded line windows.

Metadata:

- `document_id`
- `chunk_id`
- `repo_relative_path`
- `source_kind`
- `heading_path`
- `line_range`
- `content_hash`
- `chunk_hash`
- `generated_status`
- `profile_scope_hash`

Freshness:

- Hash-based per-file refresh.
- Delete stale chunks when docs/report files are removed or become excluded.
- Do not couple docs text freshness to proof graph freshness.

## Quality Gates

Add tests before enabling by default:

- Markdown README query returns `text_evidence`, path, line span, snippet, score.
- PowerShell script query returns `text_evidence` and never creates graph edges.
- Stable report summary query can find verdict/metric text.
- Raw generated artifact path is excluded unless explicitly included.
- `context-pack --mode docs` returns docs snippets and no `verified_paths`.
- `context-pack --mode repo-text` can combine broad text evidence while labeling it non-proof.
- Graph Truth Gate remains unchanged and does not consume docs FTS rows.
- Context Packet Gate adds a separate text-mode fixture; graph-mode gates remain unchanged.
- DB integrity checks include text sidecar integrity separately.
- Lifecycle preflight checks exact text DB path.

## Implementation Phases

1. Schema contract only:
   - Add structs/enums for `TextEvidence`, `TextCorpusProfile`, and `TextEvidenceClass`.
   - Add JSON schema docs and tests for labels.
   - No indexing yet.

2. Read-only corpus discovery:
   - Implement `docs_text` scope discovery dry-run.
   - Report included/excluded files and reasons.
   - No DB writes yet.

3. Sidecar FTS index:
   - Build `codegraph_text.sqlite` with passport and read-only inspection.
   - Implement incremental stale cleanup for text chunks.

4. Query surface:
   - Add CLI/MCP text-profile search.
   - Return snippets, paths, line ranges, scores, and evidence class.

5. Context modes:
   - Add `context-pack --mode docs` and `--mode repo-text`.
   - Keep graph proof output sections separate.

6. Gates and docs:
   - Add regression fixtures.
   - Update CLI/MCP docs after implementation is proven.

## Non-Negotiable Invariants

- No docs text may create proof edges.
- No docs text may be counted as Graph Truth relation precision/recall.
- No docs text may enter `PathEvidence` unless represented as a separate non-proof text result type.
- No benchmark may claim graph-proof improvement from docs text retrieval.
- Existing compact proof DB thresholds and graph semantic gates must not be weakened.
- `rg` remains a valid companion/fallback until docs text retrieval is proven on real docs/report tasks.

## Verification For This Planning Pass

- Plan-only report created.
- JSON companion validates.
- No runtime/source behavior was changed for this plan.
