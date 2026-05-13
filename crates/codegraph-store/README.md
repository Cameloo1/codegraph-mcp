# codegraph-store

Local graph storage crate.

`MVP.md` is the source of truth for this crate's phase boundary and acceptance
criteria.

Phase 03 implements `GraphStore` and `SqliteGraphStore` using `rusqlite`.
Migrations create the MVP graph tables with schema versioning and in-memory
SQLite test support. Later MVP phases added the Stage 0 SQLite FTS5 index for
files, entities, and snippets, read-side list/count helpers, and retrieval
trace persistence for query and MCP surfaces.

Guardrail: no parser extraction, binary-vector retrieval, UI behavior, RocksDB
backend, or benchmark execution lives here.
