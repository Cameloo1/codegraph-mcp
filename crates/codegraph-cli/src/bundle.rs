//! Bundle export/import commands and their DB publish/passport helpers.
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use std::path::{Path, PathBuf};

use codegraph_store::{GraphStore, SqliteGraphStore, SCHEMA_VERSION};
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::*;

pub(crate) fn run_bundle_export(args: &[String]) -> Result<Value, String> {
    let output = parse_output_arg(args)?;
    let repo_root = current_repo_root()?;
    let store = open_existing_store(&repo_root)?;
    let passport = store.get_db_passport().map_err(|error| error.to_string())?;
    let files = store
        .list_files(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let entities = store
        .list_entities(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let edges = store
        .list_edges(UNBOUNDED_STORE_READ_LIMIT)
        .map_err(|error| error.to_string())?;
    let graph_digest = graph_fact_hash(&entities, &edges);
    let default_scope = IndexScopeOptions::default();
    let scope_hash = passport
        .as_ref()
        .map(|passport| passport.index_scope_policy_hash.clone())
        .unwrap_or_else(|| {
            scope_policy_hash(&default_scope).unwrap_or_else(|_| "unknown".to_string())
        });
    let scope_policy_json = passport
        .as_ref()
        .map(|passport| passport.scope_policy_json.clone())
        .unwrap_or_else(|| {
            serde_json::to_string(&default_scope).unwrap_or_else(|_| "{}".to_string())
        });
    let storage_mode = passport
        .as_ref()
        .map(|passport| passport.storage_mode.clone())
        .unwrap_or_else(|| StorageMode::Proof.as_str().to_string());
    let bundle = CodeGraphBundle {
        manifest: BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            created_by: format!("{BIN_NAME} phase {PHASE}"),
            created_at_unix_ms: unix_time_ms(),
            repo_root: path_string(&repo_root),
            repo_identity: Some(bundle_repo_identity(&repo_root)),
            canonical_repo_root: Some(path_string(&repo_root)),
            repo_head: git_head(&repo_root),
            scope_hash: Some(scope_hash),
            scope_policy_json: Some(scope_policy_json),
            storage_mode: Some(storage_mode),
            db_schema_version: Some(store.schema_version().map_err(|error| error.to_string())?),
            graph_digest: Some(graph_digest),
            file_count: files.len(),
            entity_count: entities.len(),
            edge_count: edges.len(),
        },
        files,
        entities,
        edges,
    };
    let encoded = serde_json::to_string_pretty(&bundle).map_err(|error| error.to_string())?;
    fs::write(&output, encoded).map_err(|error| error.to_string())?;

    Ok(json!({
        "status": "exported",
        "output": path_string(&output),
        "manifest": bundle.manifest,
    }))
}

pub(crate) fn run_bundle_import(args: &[String]) -> Result<Value, String> {
    let options = parse_bundle_import_options(args)?;
    let input = options.input;
    let source = fs::read_to_string(&input).map_err(|error| error.to_string())?;
    let bundle: CodeGraphBundle =
        serde_json::from_str(&source).map_err(|error| error.to_string())?;
    if bundle.manifest.schema_version != BUNDLE_SCHEMA_VERSION {
        return Err(format!(
            "bundle schema mismatch: expected {}, got {}",
            BUNDLE_SCHEMA_VERSION, bundle.manifest.schema_version
        ));
    }

    let repo_root = current_repo_root()?;
    let evidence = validate_bundle_manifest(&bundle, &repo_root)?;
    if options.mode == BundleImportMode::Merge {
        return Ok(json!({
            "status": "merge_refused",
            "mode": options.mode.as_str(),
            "input": path_string(&input),
            "repo_root": path_string(&repo_root),
            "manifest": bundle.manifest,
            "claimable": false,
            "diagnostic_only": true,
            "database_mutated": false,
            "provenance": {
                "bundle_repo_identity": evidence.repo_identity,
                "bundle_graph_digest": evidence.graph_digest,
                "merge_policy": "refused_until_per_fact_provenance_is_defined"
            },
            "message": "bundle import --merge is explicit but currently diagnostic-only: merge mutation is refused until merged facts have a product-grade provenance and claimability contract"
        }));
    }

    let db_path = selected_db_path_for_repo(&repo_root).path;
    let target_state = inspect_bundle_target_state(&db_path);
    if options.mode == BundleImportMode::Fresh
        && !matches!(
            target_state,
            BundleTargetState::Missing | BundleTargetState::Empty
        )
    {
        return Err(format!(
            "bundle import refuses to write into {} DB at {} without --replace or explicit --merge",
            target_state.as_str(),
            db_path.display()
        ));
    }

    let temp_db_path = bundle_import_temp_db_path(&db_path);
    let import_result = (|| {
        remove_sqlite_file_family(&temp_db_path)?;
        import_bundle_to_temp_db(&temp_db_path, &repo_root, &bundle, &evidence, true)?;
        bundle_import_chaos_failpoint("bundle_after_temp_db_creation_before_validation")?;
        let temp_preflight =
            inspect_db_lifecycle_surface_preflight(DbLifecycleSurfacePreflightRequest {
                repo_root: repo_root.clone(),
                db_path: temp_db_path.clone(),
                surface_name: "cli.bundle.import.temp".to_string(),
                operation_kind: DbLifecycleOperationKind::NormalRead,
                allow_stale_read: false,
                allow_foreign_repo: false,
                required_storage_mode: Some(evidence.storage_mode),
                expected_scope: None,
            })
            .map_err(|error| error.to_string())?;
        if !temp_preflight.safe_to_read {
            return Err(format!(
                "bundle import temp DB failed lifecycle validation: {}",
                temp_preflight.blockers.join("; ")
            ));
        }
        bundle_import_chaos_failpoint("bundle_after_validation_before_publish")?;
        publish_bundle_import_db(&temp_db_path, &db_path)?;
        let final_preflight =
            inspect_db_lifecycle_surface_preflight(DbLifecycleSurfacePreflightRequest {
                repo_root: repo_root.clone(),
                db_path: db_path.clone(),
                surface_name: "cli.bundle.import.final".to_string(),
                operation_kind: DbLifecycleOperationKind::NormalRead,
                allow_stale_read: false,
                allow_foreign_repo: false,
                required_storage_mode: Some(evidence.storage_mode),
                expected_scope: None,
            })
            .map_err(|error| error.to_string())?;
        if !final_preflight.safe_to_read {
            return Err(format!(
                "bundle import final DB failed lifecycle validation: {}",
                final_preflight.blockers.join("; ")
            ));
        }
        Ok(final_preflight)
    })();
    if import_result.is_err() {
        let _ = remove_sqlite_file_family(&temp_db_path);
    }
    let final_preflight = import_result?;

    Ok(json!({
        "status": "imported",
        "mode": options.mode.as_str(),
        "input": path_string(&input),
        "repo_root": path_string(&repo_root),
        "db_path": path_string(&db_path),
        "target_state_before_import": target_state.as_str(),
        "old_db_replaced": target_state != BundleTargetState::Missing,
        "atomic_publish": true,
        "claimable": true,
        "diagnostic_only": false,
        "db_lifecycle_read": db_lifecycle_surface_preflight_json(&final_preflight),
        "manifest": bundle.manifest,
    }))
}

pub(crate) fn parse_bundle_import_options(args: &[String]) -> Result<BundleImportOptions, String> {
    let mut input = None;
    let mut mode = BundleImportMode::Fresh;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--replace" => {
                if mode != BundleImportMode::Fresh {
                    return Err(
                        "bundle import accepts only one of --replace or --merge".to_string()
                    );
                }
                mode = BundleImportMode::Replace;
            }
            "--merge" => {
                if mode != BundleImportMode::Fresh {
                    return Err(
                        "bundle import accepts only one of --replace or --merge".to_string()
                    );
                }
                mode = BundleImportMode::Merge;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown bundle import option: {value}"));
            }
            value => {
                if input.is_some() {
                    return Err(bundle_import_usage());
                }
                input = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    let Some(input) = input else {
        return Err(bundle_import_usage());
    };
    Ok(BundleImportOptions { input, mode })
}

pub(crate) fn bundle_import_usage() -> String {
    "Usage: codegraph-mcp bundle import repo.cgc-bundle [--replace|--merge]".to_string()
}

pub(crate) fn validate_bundle_manifest(
    bundle: &CodeGraphBundle,
    repo_root: &Path,
) -> Result<BundleManifestEvidence, String> {
    if bundle.manifest.file_count != bundle.files.len() {
        return Err(format!(
            "bundle file_count mismatch: manifest {}, observed {}",
            bundle.manifest.file_count,
            bundle.files.len()
        ));
    }
    if bundle.manifest.entity_count != bundle.entities.len() {
        return Err(format!(
            "bundle entity_count mismatch: manifest {}, observed {}",
            bundle.manifest.entity_count,
            bundle.entities.len()
        ));
    }
    if bundle.manifest.edge_count != bundle.edges.len() {
        return Err(format!(
            "bundle edge_count mismatch: manifest {}, observed {}",
            bundle.manifest.edge_count,
            bundle.edges.len()
        ));
    }

    let repo_identity = required_bundle_string(&bundle.manifest.repo_identity, "repo_identity")?;
    let canonical_repo_root =
        required_bundle_string(&bundle.manifest.canonical_repo_root, "canonical_repo_root")?;
    let expected_identity = bundle_repo_identity(repo_root);
    let expected_canonical = path_string(repo_root);
    if repo_identity != expected_identity || canonical_repo_root != expected_canonical {
        return Err(format!(
            "bundle repo identity mismatch: expected repo_identity={} canonical_repo_root={}, observed repo_identity={} canonical_repo_root={}",
            expected_identity, expected_canonical, repo_identity, canonical_repo_root
        ));
    }
    let expected_repo_head = git_head(repo_root);
    match (&expected_repo_head, &bundle.manifest.repo_head) {
        (Some(expected), Some(observed)) if expected != observed => {
            return Err(format!(
                "bundle repo head mismatch: expected {}, observed {}",
                expected, observed
            ));
        }
        (Some(expected), None) => {
            return Err(format!(
                "bundle repo head mismatch: expected {}, observed unknown",
                expected
            ));
        }
        _ => {}
    }

    let scope_hash = required_bundle_string(&bundle.manifest.scope_hash, "scope_hash")?;
    let scope_policy_json =
        required_bundle_string(&bundle.manifest.scope_policy_json, "scope_policy_json")?;
    let scope_policy: IndexScopeOptions =
        serde_json::from_str(&scope_policy_json).map_err(|error| {
            format!("bundle scope_policy_json is not a valid IndexScopeOptions value: {error}")
        })?;
    let computed_scope_hash =
        scope_policy_hash(&scope_policy).map_err(|error| error.to_string())?;
    if computed_scope_hash != scope_hash {
        return Err(format!(
            "bundle scope hash mismatch: manifest {}, computed {}",
            scope_hash, computed_scope_hash
        ));
    }

    let storage_mode_text = required_bundle_string(&bundle.manifest.storage_mode, "storage_mode")?;
    let storage_mode = storage_mode_text.parse::<StorageMode>()?;
    let db_schema_version = bundle
        .manifest
        .db_schema_version
        .ok_or_else(|| "bundle manifest missing required field db_schema_version".to_string())?;
    if db_schema_version != SCHEMA_VERSION {
        return Err(format!(
            "bundle DB schema mismatch: expected {}, got {}",
            SCHEMA_VERSION, db_schema_version
        ));
    }

    let graph_digest = required_bundle_string(&bundle.manifest.graph_digest, "graph_digest")?;
    let computed_graph_digest = graph_fact_hash(&bundle.entities, &bundle.edges);
    if computed_graph_digest != graph_digest {
        return Err(format!(
            "bundle graph digest mismatch: manifest {}, computed {}",
            graph_digest, computed_graph_digest
        ));
    }

    Ok(BundleManifestEvidence {
        repo_identity,
        canonical_repo_root,
        repo_head: bundle.manifest.repo_head.clone(),
        scope_hash,
        scope_policy_json,
        storage_mode,
        db_schema_version,
        graph_digest,
    })
}

pub(crate) fn required_bundle_string(value: &Option<String>, name: &str) -> Result<String, String> {
    value
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| format!("bundle manifest missing required field {name}"))
}

pub(crate) fn bundle_repo_identity(repo_root: &Path) -> String {
    format!("repo-root:{}", path_string(repo_root))
}

pub(crate) fn git_head(repo_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub(crate) fn bundle_import_chaos_failpoint_enabled(name: &str) -> bool {
    std::env::var("CODEGRAPH_WRITE_PATH_FAILPOINT")
        .ok()
        .is_some_and(|raw| {
            raw.split(',')
                .map(str::trim)
                .any(|value| value == name || value == "bundle_all")
        })
}

pub(crate) fn bundle_import_chaos_failpoint(name: &str) -> Result<(), String> {
    if bundle_import_chaos_failpoint_enabled(name) {
        Err(format!("chaos_failpoint:{name}"))
    } else {
        Ok(())
    }
}

pub(crate) fn inspect_bundle_target_state(db_path: &Path) -> BundleTargetState {
    if !db_path.exists() {
        return BundleTargetState::Missing;
    }
    let Ok(connection) = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return BundleTargetState::Uninspectable;
    };
    let mut total_rows = 0u64;
    for table in ["files", "entities", "edges", "codegraph_db_passport"] {
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get::<_, bool>(0),
            )
            .unwrap_or(false);
        if exists {
            total_rows += connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, u64>(0)
                })
                .unwrap_or(1);
        }
    }
    if total_rows == 0 {
        BundleTargetState::Empty
    } else {
        BundleTargetState::NonEmpty
    }
}

pub(crate) fn import_bundle_to_temp_db(
    temp_db_path: &Path,
    repo_root: &Path,
    bundle: &CodeGraphBundle,
    evidence: &BundleManifestEvidence,
    claimable: bool,
) -> Result<(), String> {
    if let Some(parent) = temp_db_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let store = SqliteGraphStore::open(temp_db_path).map_err(|error| error.to_string())?;
    store
        .transaction(|tx| {
            for file in &bundle.files {
                tx.upsert_file(file)?;
            }
            for entity in &bundle.entities {
                tx.upsert_entity(entity)?;
                if let Some(span) = &entity.source_span {
                    tx.upsert_source_span(&entity.id, span)?;
                }
            }
            for edge in &bundle.edges {
                tx.upsert_edge(edge)?;
                tx.upsert_source_span(&edge.id, &edge.source_span)?;
            }
            Ok(())
        })
        .map_err(|error| error.to_string())?;
    store
        .quick_integrity_gate()
        .map_err(|error| error.to_string())?;
    upsert_cli_db_passport_with_policy(
        &store,
        repo_root,
        evidence,
        bundle.manifest.file_count,
        bundle.manifest.file_count,
        if claimable {
            "ok"
        } else {
            "diagnostic_merge_non_claimable"
        },
    )?;
    store
        .quick_integrity_gate()
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn bundle_import_temp_db_path(final_db_path: &Path) -> PathBuf {
    let parent = final_db_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = final_db_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("codegraph.sqlite");
    parent.join(format!(
        ".{file_name}.bundle-import-tmp-{}-{}",
        std::process::id(),
        unix_time_ms()
    ))
}

pub(crate) fn bundle_import_backup_db_path(final_db_path: &Path) -> PathBuf {
    let parent = final_db_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = final_db_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("codegraph.sqlite");
    parent.join(format!(
        ".{file_name}.bundle-import-backup-{}-{}",
        std::process::id(),
        unix_time_ms()
    ))
}

pub(crate) fn publish_bundle_import_db(
    temp_db_path: &Path,
    final_db_path: &Path,
) -> Result<(), String> {
    let parent = final_db_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let backup_db_path = bundle_import_backup_db_path(final_db_path);
    let had_old_db = final_db_path.exists();
    if had_old_db {
        if let Err(error) = rename_sqlite_file_family(final_db_path, &backup_db_path) {
            let _ = rename_sqlite_file_family(&backup_db_path, final_db_path);
            return Err(error);
        }
    } else {
        remove_sqlite_sidecars(final_db_path)?;
    }
    if let Err(error) = bundle_import_chaos_failpoint("bundle_during_publish") {
        let _ = remove_sqlite_file_family(final_db_path);
        if had_old_db {
            let _ = rename_sqlite_file_family(&backup_db_path, final_db_path);
        }
        return Err(error);
    }

    match fs::rename(temp_db_path, final_db_path) {
        Ok(()) => {
            remove_sqlite_sidecars(temp_db_path)?;
            if had_old_db {
                remove_sqlite_file_family(&backup_db_path)?;
            }
            Ok(())
        }
        Err(error) => {
            let _ = remove_sqlite_file_family(final_db_path);
            if had_old_db {
                let _ = rename_sqlite_file_family(&backup_db_path, final_db_path);
            }
            Err(error.to_string())
        }
    }
}

pub(crate) fn rename_sqlite_file_family(from: &Path, to: &Path) -> Result<(), String> {
    rename_file_if_exists(from, to)?;
    rename_file_if_exists(
        &sqlite_sidecar_path(from, "wal"),
        &sqlite_sidecar_path(to, "wal"),
    )?;
    rename_file_if_exists(
        &sqlite_sidecar_path(from, "shm"),
        &sqlite_sidecar_path(to, "shm"),
    )?;
    Ok(())
}

pub(crate) fn remove_sqlite_file_family(path: &Path) -> Result<(), String> {
    remove_file_if_exists(path)?;
    remove_sqlite_sidecars(path)
}

pub(crate) fn remove_sqlite_sidecars(path: &Path) -> Result<(), String> {
    remove_file_if_exists(&sqlite_sidecar_path(path, "wal"))?;
    remove_file_if_exists(&sqlite_sidecar_path(path, "shm"))?;
    Ok(())
}

pub(crate) fn rename_file_if_exists(from: &Path, to: &Path) -> Result<(), String> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn upsert_cli_db_passport_with_policy(
    store: &SqliteGraphStore,
    repo_root: &Path,
    evidence: &BundleManifestEvidence,
    files_seen: usize,
    files_indexed: usize,
    integrity_gate_result: &str,
) -> Result<(), String> {
    let canonical_repo_root = resolve_repo_root(repo_root)?;
    let now = unix_time_ms();
    let passport = DbPassport {
        passport_version: DB_PASSPORT_VERSION,
        codegraph_schema_version: SCHEMA_VERSION,
        storage_mode: evidence.storage_mode.as_str().to_string(),
        index_scope_policy_hash: evidence.scope_hash.clone(),
        scope_policy_json: evidence.scope_policy_json.clone(),
        canonical_repo_root: path_string(&canonical_repo_root),
        git_remote: None,
        worktree_root: Some(path_string(&canonical_repo_root)),
        repo_head: evidence.repo_head.clone(),
        source_discovery_policy_version: "scope-policy-v1".to_string(),
        codegraph_build_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        last_successful_index_timestamp: Some(now),
        last_completed_run_id: Some(format!("cli-import-{now}-{}", std::process::id())),
        last_run_status: "completed".to_string(),
        integrity_gate_result: integrity_gate_result.to_string(),
        files_seen: files_seen as u64,
        files_indexed: files_indexed as u64,
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
    };
    store
        .upsert_db_passport(&passport)
        .map_err(|error| error.to_string())
}
