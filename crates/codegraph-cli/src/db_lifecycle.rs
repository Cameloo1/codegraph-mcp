//! Repo-root resolution, DB open, and read-path lifecycle/passport guards
//! shared across CLI commands.
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use std::path::{Path, PathBuf};

use codegraph_query::ExactGraphQueryEngine;
use codegraph_store::{GraphStore, SqliteGraphStore};
use serde_json::{json, Value};

use crate::*;

pub(crate) fn resolve_repo_root(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!(
            "repository path does not exist: {}",
            path.display()
        ));
    }
    fs::canonicalize(path).map_err(|error| error.to_string())
}

pub(crate) fn current_repo_root() -> Result<PathBuf, String> {
    std::env::current_dir()
        .map_err(|error| error.to_string())
        .and_then(|path| resolve_repo_root(&path))
}

pub(crate) fn open_existing_store(repo_root: &Path) -> Result<SqliteGraphStore, String> {
    let db_path = resolved_db_path_for_repo(repo_root);
    let _ = read_db_lifecycle_guard(
        repo_root,
        &db_path,
        allow_stale_read_enabled(),
        allow_foreign_read_enabled(),
        None,
    )?;
    SqliteGraphStore::open_read_only(db_path).map_err(|error| error.to_string())
}

pub(crate) fn allow_stale_read_enabled() -> bool {
    std::env::var("CODEGRAPH_ALLOW_STALE_READ")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub(crate) fn allow_foreign_read_enabled() -> bool {
    std::env::var("CODEGRAPH_ALLOW_FOREIGN_DB")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub(crate) fn with_lifecycle_override_env<F, T>(
    allow_stale_read: bool,
    allow_foreign_db: bool,
    operation: F,
) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    let previous_allow_stale = std::env::var("CODEGRAPH_ALLOW_STALE_READ").ok();
    let previous_allow_foreign = std::env::var("CODEGRAPH_ALLOW_FOREIGN_DB").ok();
    if allow_stale_read {
        std::env::set_var("CODEGRAPH_ALLOW_STALE_READ", "1");
    }
    if allow_foreign_db {
        std::env::set_var("CODEGRAPH_ALLOW_FOREIGN_DB", "1");
    }
    let result = operation();
    match previous_allow_stale {
        Some(value) => std::env::set_var("CODEGRAPH_ALLOW_STALE_READ", value),
        None => std::env::remove_var("CODEGRAPH_ALLOW_STALE_READ"),
    }
    match previous_allow_foreign {
        Some(value) => std::env::set_var("CODEGRAPH_ALLOW_FOREIGN_DB", value),
        None => std::env::remove_var("CODEGRAPH_ALLOW_FOREIGN_DB"),
    }
    result
}

pub(crate) fn inspect_read_db_lifecycle_preflight(
    repo_root: &Path,
    db_path: &Path,
    explicit_scope_policy: Option<IndexScopeOptions>,
) -> Result<DbLifecyclePreflight, String> {
    inspect_db_lifecycle_preflight(repo_root, db_path, explicit_scope_policy)
        .map_err(|error| error.to_string())
}

pub(crate) fn read_db_lifecycle_guard(
    repo_root: &Path,
    db_path: &Path,
    allow_stale_read: bool,
    allow_foreign_db: bool,
    explicit_scope_policy: Option<IndexScopeOptions>,
) -> Result<Value, String> {
    let preflight = inspect_read_db_lifecycle_preflight(repo_root, db_path, explicit_scope_policy)?;
    if preflight.safe {
        return Ok(db_lifecycle_preflight_json(
            &preflight,
            true,
            allow_stale_read,
            allow_foreign_db,
        ));
    }
    if allow_stale_read || allow_foreign_db {
        let remaining_blockers = preflight
            .blockers
            .iter()
            .filter(|blocker| {
                !((allow_stale_read && lifecycle_blocker_is_stale_or_missing_passport(blocker))
                    || (allow_foreign_db && lifecycle_blocker_is_foreign_repo(blocker)))
            })
            .collect::<Vec<_>>();
        if remaining_blockers.is_empty() {
            return Ok(db_lifecycle_preflight_json(
                &preflight,
                false,
                allow_stale_read,
                allow_foreign_db,
            ));
        }
    }
    if let Some(mismatch) = preflight.scope_mismatch.as_ref() {
        return Err(scope_mismatch_message(db_path, mismatch));
    }
    let outside_note = preflight
        .outside_workspace_note
        .as_deref()
        .map(|note| format!("; {note}"))
        .unwrap_or_default();
    Err(format!(
        "CodeGraph DB is not safe to read at {}: kind={}; {}{}; run `codegraph-mcp index . --fresh`; pass --allow-stale-read for stale/passport diagnostic output or --allow-foreign-db for foreign-repo diagnostic output",
        db_path.display(),
        preflight
            .db_problem_kind
            .as_deref()
            .unwrap_or("unknown"),
        preflight.blockers.join("; "),
        outside_note
    ))
}

pub(crate) fn lifecycle_blocker_is_foreign_repo(blocker: &str) -> bool {
    blocker.contains("repo root mismatch") || blocker.contains("git remote mismatch")
}

pub(crate) fn lifecycle_blocker_is_stale_or_missing_passport(blocker: &str) -> bool {
    blocker.contains("previous run did not complete")
        || blocker.contains("previous integrity gate was not ok")
        || blocker.contains("repo head mismatch")
        || blocker.contains("codegraph_db_passport table is missing")
        || blocker.contains("codegraph_db_passport row is missing")
        || blocker.contains("passport scope_policy_json is missing")
}

pub(crate) fn scope_mismatch_message(
    db_path: &Path,
    mismatch: &codegraph_index::ScopeMismatchDetails,
) -> String {
    format!(
        "scope_mismatch: {} at {}: expected passport_scope_hash={}, observed explicit_scope_hash={}",
        mismatch.message,
        db_path.display(),
        mismatch.expected_scope_hash.as_deref().unwrap_or("unknown"),
        mismatch.observed_scope_hash.as_deref().unwrap_or("unknown")
    )
}

pub(crate) fn db_lifecycle_preflight_json(
    preflight: &DbLifecyclePreflight,
    claimable: bool,
    allow_stale_read: bool,
    allow_foreign_db: bool,
) -> Value {
    json!({
        "decision": if preflight.safe { "read_reuse" } else { "diagnostic_stale_reuse" },
        "db_problem_kind": preflight.db_problem_kind.clone(),
        "path_access_status": preflight.path_access_status.clone(),
        "path_access_error": preflight.path_access_error.clone(),
        "passport_status": preflight.db_health.passport_status,
        "claimable": claimable && preflight.safe,
        "diagnostic_only": !(claimable && preflight.safe),
        "contaminated": !preflight.safe,
        "allow_stale_read": allow_stale_read,
        "allow_foreign_db": allow_foreign_db,
        "reasons": preflight.db_health.reasons.clone(),
        "sqlite_sidecars": preflight.db_health.sqlite_sidecars.clone(),
        "sidecar_status": preflight.db_health.sidecar_status.clone(),
        "orphan_sidecars": preflight.db_health.orphan_sidecars.clone(),
        "orphan_sidecars_deprecated": true,
        "safety_labels": db_lifecycle_safety_labels(preflight, None),
        "blockers": preflight.blockers.clone(),
        "warnings": preflight.warnings.clone(),
        "exact_db_path_checked": preflight.exact_db_path_checked.clone(),
        "repo_root_expected": preflight.repo_root_expected.clone(),
        "db_path_outside_workspace": preflight.db_path_outside_workspace,
        "outside_workspace_note": preflight.outside_workspace_note.clone(),
        "repo_root_status": preflight.repo_root_status.clone(),
        "schema_status": preflight.schema_status.clone(),
        "storage_mode_status": preflight.storage_mode_status.clone(),
        "artifact_freshness": read_lifecycle_artifact_freshness(preflight),
        "scope_status": preflight.scope_status.clone(),
        "scope_source": preflight.scope_source.clone(),
        "passport_scope_hash": preflight.passport_scope_hash.clone(),
        "explicit_scope_hash": preflight.explicit_scope_hash.clone(),
        "scope_mismatch": preflight.scope_mismatch.clone(),
        "passport_scope_policy": preflight.passport_scope_policy.clone(),
        "explicit_scope_policy": preflight.explicit_scope_policy.clone(),
    })
}

pub(crate) fn db_lifecycle_safety_labels(
    preflight: &DbLifecyclePreflight,
    sqlite_sidecars: Option<&Value>,
) -> Vec<String> {
    let mut labels = BTreeSet::new();
    if let Some(kind) = preflight.db_problem_kind.as_deref() {
        labels.insert(kind.to_string());
        match kind {
            "repo_root_mismatch" => {
                labels.insert("repo_mismatch".to_string());
                labels.insert("foreign".to_string());
            }
            "repo_head_mismatch" | "scope_mismatch" | "storage_mismatch" => {
                labels.insert("stale".to_string());
            }
            "db_missing" => {
                labels.insert("not_indexed".to_string());
            }
            _ => {}
        }
    }
    match preflight.path_access_status.as_str() {
        "db_missing" => {
            labels.insert("not_indexed".to_string());
        }
        "permission_denied" => {
            labels.insert("permission_denied".to_string());
        }
        "filesystem_inaccessible" => {
            labels.insert("filesystem_inaccessible".to_string());
        }
        _ => {}
    }
    match preflight.db_health.passport_status.as_str() {
        "missing" => {
            labels.insert("passport_missing".to_string());
        }
        "corrupt" => {
            labels.insert("passport_corrupt".to_string());
        }
        "locked" => {
            labels.insert("db_locked".to_string());
        }
        _ => {}
    }
    if read_lifecycle_artifact_freshness(preflight)
        .as_deref()
        .is_some_and(|freshness| freshness.starts_with("incomplete:"))
    {
        labels.insert("stale".to_string());
    }
    for blocker in &preflight.blockers {
        if lifecycle_blocker_is_foreign_repo(blocker) {
            labels.insert("repo_mismatch".to_string());
            labels.insert("foreign".to_string());
        }
        if lifecycle_blocker_is_stale_or_missing_passport(blocker) {
            labels.insert("stale".to_string());
        }
        if blocker.contains("permission denied") {
            labels.insert("permission_denied".to_string());
        }
        if blocker.contains("filesystem") {
            labels.insert("filesystem_inaccessible".to_string());
        }
        if blocker.contains("locked") || blocker.contains("database is busy") {
            labels.insert("db_locked".to_string());
        }
    }
    if preflight.db_health.sidecar_status == "orphan_without_main_db" {
        labels.insert("orphan_without_main_db".to_string());
    }
    if preflight.db_health.sidecar_status == "stale_cleanup_candidate" {
        labels.insert("sidecar_only_change".to_string());
    }
    if let Some(sqlite_sidecars) = sqlite_sidecars {
        for key in ["sidecar_status", "sidecar_change_classification", "status"] {
            if let Some(value) = sqlite_sidecars.get(key).and_then(Value::as_str) {
                match value {
                    "orphan_without_main_db" => {
                        labels.insert("orphan_without_main_db".to_string());
                    }
                    "sidecar_only_change" => {
                        labels.insert("sidecar_only_change".to_string());
                    }
                    _ => {}
                }
            }
        }
        if sqlite_sidecars
            .get("sidecar_only_change")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            labels.insert("sidecar_only_change".to_string());
        }
    }
    if !preflight.safe {
        labels.insert("diagnostic_only".to_string());
        labels.insert("blocked".to_string());
    }
    labels.into_iter().collect()
}

pub(crate) fn agent_use_safety_labels(
    profile: &AgentUseProfile,
    preflight: &DbLifecyclePreflight,
    sqlite_sidecars: Option<&Value>,
) -> Vec<String> {
    let mut labels = db_lifecycle_safety_labels(preflight, sqlite_sidecars)
        .into_iter()
        .collect::<BTreeSet<_>>();
    if let Some(publish_status) = agent_use_publish_state_status(profile) {
        match publish_status.as_str() {
            "updating" => {
                labels.insert("updating".to_string());
            }
            "publishing" => {
                labels.insert("publishing".to_string());
            }
            "interrupted" => {
                labels.insert("interrupted".to_string());
                if preflight.safe {
                    labels.insert("recovered".to_string());
                } else {
                    labels.insert("blocked".to_string());
                }
            }
            other if !other.is_empty() => {
                labels.insert(other.to_string());
            }
            _ => {}
        }
    }
    if agent_use_publish_state_active(profile) {
        labels.insert("publishing".to_string());
    }
    labels.into_iter().collect()
}

pub(crate) fn query_engine(store: &SqliteGraphStore) -> Result<ExactGraphQueryEngine, String> {
    let edges = store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    Ok(ExactGraphQueryEngine::new(edges))
}

pub(crate) fn profile_span_json(
    name: &str,
    elapsed: Duration,
    count: u64,
    items: u64,
    detail: Value,
) -> Value {
    json!({
        "name": name,
        "elapsed_ms": elapsed.as_secs_f64() * 1000.0,
        "count": count,
        "items": items,
        "detail": detail,
    })
}
