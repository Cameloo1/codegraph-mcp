//! Local graph storage abstractions and SQLite implementation.
//!
//! Phase 15 persists the graph model, Stage 0 FTS/BM25 indexes, and retrieval
//! trace JSON via `rusqlite`. Parser extraction, binary-vector retrieval, MCP
//! behavior, UI behavior, and RocksDB are intentionally not implemented here.

#![forbid(unsafe_code)]

mod error;
mod sqlite;
mod traits;

pub use error::{StoreError, StoreResult};
pub use sqlite::{
    reset_sqlite_profile, take_sqlite_profile, SqliteGraphStore, SqliteProfileSpan,
    StorageAccountingRow, SCHEMA_VERSION,
};
pub use traits::{GraphStore, RetrievalTraceRecord, TextSearchHit, TextSearchKind};
