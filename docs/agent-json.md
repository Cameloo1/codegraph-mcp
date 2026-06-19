# Agent JSON Contract

This document defines the compact agent JSON contract for stable
`--agent-json` and compact lifecycle output modes. The schemas are public,
machine-readable contracts, and Rust regression tests assert that emitted agent
JSON includes the required top-level fields and stays under documented size
targets.

Schema files live in `docs/schemas/agent-json/`:

- `index_agent_json.schema.json`
- `query_symbols_agent_json.schema.json`
- `query_text_agent_json.schema.json`
- `query_files_agent_json.schema.json`
- `query_unresolved_calls_agent_json.schema.json`
- `context_pack_agent_json.schema.json`
- `callers_callees_agent_json.schema.json`
- `status_compact_json.schema.json`
- `doctor_compact_json.schema.json`
- `validation_packet_agent_json.schema.json`
- `common.schema.json`

## Versioning

Every agent JSON response has:

- `schema_name`
- `schema_version`
- `command`
- `repo`
- `db`

The initial compact contract uses `schema_version: 1`. Additive fields are
allowed under the same major version. Clients must ignore unknown fields.
Removing a field, changing field meaning, changing enum semantics, or changing
the truncation/lifecycle/evidence rules requires a version bump.

## Unknowns

Unknown optional fields are omitted, not set to `null`. Required fields use
explicit booleans, empty arrays, or bounded strings instead of `null`.
Telemetry fields that look measured must be labeled as measured, aggregated,
or unknown. For example, unmeasured memory is reported as `memory: "unknown"`
with `memory_measured: false`, not as a fake zero-byte measurement.

Examples:

- If `total_available` is cheap, include it.
- If it is not cheap, omit `total_available` and set
  `total_available_unknown: true`.
- If a source span is unavailable, omit that source span object rather than
  emitting a null span.

## Truncation

Agent modes must limit result ids before expensive hydration or serialization.
Every response includes a `truncation` object with:

- `returned_count`
- `limit_applied`
- `omitted_count`
- `total_available_unknown`

If exact totals are cheap, include `total_available` and make `omitted_count`
exact. If exact totals are not cheap, implementations should use a cheap
read-ahead where possible, set `total_available_unknown: true`, and mark
`omitted_count_is_lower_bound: true` when the omitted count is a lower bound.

## Lifecycle

Every response includes compact lifecycle state in `lifecycle`.

Required lifecycle fields:

- `claimable`
- `diagnostic_only`

The same claim fields are also repeated at the response top level so tight
agent loops can branch without walking nested lifecycle structures.

Full DB passport/preflight detail belongs only in verbose, profile, audit, or
debug output. Compact agent JSON must not embed raw lifecycle payloads by
default.

## Evidence

When returning context or proof evidence, responses include:

- `evidence_role`
- `source_spans` when available
- `classification_reason` when the role is inferred, fallback-derived, mixed,
  unknown, test, mock, or otherwise non-obvious
- candidate provenance fields such as `candidate_source`, `candidate_sources`,
  and `candidate_source_counts` where the surface returns retrieval candidates

Valid evidence roles are `production`, `test`, `mock`, `stub`, `generated`,
`mixed`, `text_evidence`, and `unknown`.
Default production context must not silently treat `test`, `mock`, `mixed`, or
`unknown` evidence as production proof.

Candidate sources such as exact seeds, text evidence, lexical search,
deterministic token-projection vector recall, binary-vector recall, nuance rescue, graph-neighborhood
expansion, PathEvidence, and fallback text evidence are not graph proof by
themselves. Context-pack output should use `no_proof_path_found` when it
returns bounded source-text fallback without a verified graph path.

Planning fields such as `follow_up_queries` are bounded query hints. They are
not shell-ready commands and do not imply internal command execution.

## Status, Warnings, Errors

Every response includes:

- `status`: `ok`, `warning`, or `error`
- `warnings`: bounded list
- `errors`: bounded list
- `timings`: compact timing summary such as `wall_ms` when measured

Raw debug data, full audit payloads, and large lifecycle objects are excluded
from compact agent JSON by default.

## Size Guards

Regression tests enforce these default size targets:

- `index_agent_json`: 4 KiB
- query agent JSON surfaces: 12 KiB
- `context_pack_agent_json`: 16 KiB by default, or the requested
  `--max-output-bytes` value when supplied
- `agent-use` profile envelopes (status/query/context-pack under the
  `agent-use` namespace): 12 KiB by default. Compaction preserves the
  schema-required fields and trims optional diagnostic sections first; the
  `--explain` and audit modes raise this bound for richer output.

Agent JSON must not include non-empty `scope.included_examples` or
`scope.excluded_examples` arrays. Scope examples and full audit payloads remain
available through explicit audit/verbose flags.

## Surface Schemas

`index_agent_json` reports compact index outcome, counts, lifecycle state, and
truncation metadata without audit-grade scope examples by default.

`query_symbols_agent_json`, `query_text_agent_json`, and
`query_files_agent_json` return bounded result arrays with source spans where
available and compact lifecycle/truncation metadata.

`callers_callees_agent_json` returns bounded call edges, resolved-entity
metadata when available, source spans, exactness/confidence labels, and evidence
roles.

`query_unresolved_calls_agent_json` returns bounded unresolved-reference lane
rows plus lifecycle, claimability, and pagination state. It may include a
legacy `calls` array, but the stable MVP3 release contract is the
`unresolved_references` block with `filters`, `items`, `rows`,
`not_graph_proof: true`, and pagination. This surface is warning/query parity
for references that could not be resolved; it is not graph relation proof.

`validation_packet_agent_json` may include an `unresolved_references` block for
new or resolved unresolved references in changed files. That block is capped,
contains counts and top escalated entries where present, and always remains
non-graph evidence unless a separate exact graph/source finding proves a
blocking relation or lifecycle violation.

`context_pack_agent_json` returns bounded symbols, snippets, and proof paths
with evidence role, classification reason/source when relevant, and production
proof eligibility.

`status_compact_json` and `doctor_compact_json` define the compact lifecycle
shape for status-like agent surfaces. The existing `status` and `doctor --json`
commands still expose their historical rich diagnostic objects unless a compact
mode is added; clients should treat these schemas as the compact lifecycle
contract, not as a claim that the rich diagnostics were removed.

`languages --json` is release capability metadata for language frontend
support. It is intentionally outside the agent JSON packet schema set unless a
future release promotes it as a stable coding-agent packet surface.
