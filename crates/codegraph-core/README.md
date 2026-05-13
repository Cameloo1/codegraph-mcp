# codegraph-core

Core crate for deterministic graph domain types.

`MVP.md` is the source of truth for this crate's phase boundary and acceptance
criteria.

Phase 02 defines serializable entity kinds, relation kinds, exactness labels,
source spans, entities, edges, file/index records, path evidence, derived
closure edges, context packets, stable ID helpers, and relation endpoint
validation.

Guardrail: no database persistence, parser extraction, vector retrieval, MCP,
UI, or benchmark implementation lives here.
