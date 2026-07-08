//! Validate-edit atomicity, attribution, and idempotency sidecars
//! (MVP3.9.5 / 9.5.1 + 9.5.2).
//!
//! Three sidecars, all next to the profile delta-state file:
//!
//! 1. `ValidationStageTracker` (9.5.1) — records which pipeline stage of an
//!    agent-use watch-once / validate-edit run is active. Deleted on every
//!    clean or structured-error finish; survives a hard death (abort, OOM
//!    kill, power loss) or a caught panic, so a post-mortem can attribute the
//!    failure to an exact stage. Kept separate from the journal on purpose:
//!    stage marks rewrite their file on every stage start, and the journal
//!    carries megabytes of old facts that must not be rewritten that often.
//! 2. `ValidationJournal` (9.5.2a) — makes validate-edit a journaled
//!    two-phase operation. The pre-commit normalized facts are journaled
//!    before the graph update commits; a crash after the commit leaves the
//!    journal in `validating`/`failed:*` state, and the next run REPLAYS the
//!    pending validation from the journaled old facts so findings the crashed
//!    run would have produced are recovered instead of lost.
//! 3. `ValidationStateRecord` (9.5.2b) — persists each run's validation
//!    outcome, including compact block-class findings as "open blockers".
//!    Every subsequent run RE-VERIFIES each open blocker against the current
//!    source + graph before re-emitting it; a persisted blocker is never
//!    replayed as stale proof.
//!
//! Boundary notes: journal/state sidecars are workflow/diagnostic lanes. A
//! journal in `validating` state means validation completeness is unknown —
//! graph claimability is NOT revoked (the committed graph matches source).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    agent_use_exact_calls_validation_packet, agent_use_graph_delta_json, agent_use_recovery_json,
    cli_write_path_chaos_failpoint_enabled, inspect_read_db_lifecycle_preflight, path_string,
    AgentUseProfile, BIN_NAME,
};
use codegraph_core::{
    EntityKind, ValidationBlockingLevel, ValidationClassification, ValidationFinding,
    ValidationLifecycleState,
};
use codegraph_index::{
    compute_entity_source_role_delta, EntitySourceRoleDeltaOptions, NormalizedFactSnapshot,
    NormalizedFactSnapshotOptions,
};
use codegraph_store::{GraphStore, SqliteGraphStore};

pub(crate) const AGENT_USE_VALIDATION_STAGE_FILE_NAME: &str =
    "production-agent-use.validation-stage.json";

/// Chaos failpoint name prefix: setting
/// `CODEGRAPH_WRITE_PATH_FAILPOINT=agent_use_validation_panic_at_<stage>`
/// panics at the start of that stage (test-only, used to exercise the
/// catch_unwind structured-error path).
pub(crate) const AGENT_USE_VALIDATION_STAGE_PANIC_FAILPOINT_PREFIX: &str =
    "agent_use_validation_panic_at_";

pub(crate) fn agent_use_validation_stage_path(profile: &AgentUseProfile) -> PathBuf {
    profile
        .delta_state_path
        .parent()
        .map(|parent| parent.join(AGENT_USE_VALIDATION_STAGE_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(AGENT_USE_VALIDATION_STAGE_FILE_NAME))
}

fn unix_ms_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

pub(crate) struct ValidationStageTracker {
    path: PathBuf,
    surface: String,
    run_id: String,
    started_unix_ms: u128,
    stages: Vec<Value>,
    preserve_on_drop: bool,
}

impl ValidationStageTracker {
    pub(crate) fn start(profile: &AgentUseProfile, surface: &str) -> Self {
        let started_unix_ms = unix_ms_now();
        Self {
            path: agent_use_validation_stage_path(profile),
            surface: surface.to_string(),
            run_id: format!("{started_unix_ms}-{}", std::process::id()),
            started_unix_ms,
            stages: Vec::new(),
            preserve_on_drop: false,
        }
    }

    /// Records the start of a pipeline stage and persists the sidecar so a
    /// hard death after this point is attributable to `stage`.
    pub(crate) fn mark(&mut self, stage: &str) {
        self.stages.push(json!({
            "name": stage,
            "started_unix_ms": unix_ms_now(),
        }));
        self.write_best_effort();
        let failpoint = format!("{AGENT_USE_VALIDATION_STAGE_PANIC_FAILPOINT_PREFIX}{stage}");
        if cli_write_path_chaos_failpoint_enabled(&failpoint) {
            panic!("chaos_failpoint:{failpoint}");
        }
    }

    /// Like [`Self::mark`] but attaches extra attribution fields (substage
    /// timings, fan-out counters) to the stage record. Used by the validation
    /// stage's per-substage instrumentation (MVP3.9.5.3) so a live reader or
    /// post-mortem can attribute cost INSIDE the validation stage.
    pub(crate) fn mark_with(&mut self, stage: &str, detail: Value) {
        let mut record = json!({
            "name": stage,
            "started_unix_ms": unix_ms_now(),
        });
        if let (Some(object), Some(extra)) = (record.as_object_mut(), detail.as_object()) {
            for (key, value) in extra {
                object.insert(key.clone(), value.clone());
            }
        }
        self.stages.push(record);
        self.write_best_effort();
        let failpoint = format!("{AGENT_USE_VALIDATION_STAGE_PANIC_FAILPOINT_PREFIX}{stage}");
        if cli_write_path_chaos_failpoint_enabled(&failpoint) {
            panic!("chaos_failpoint:{failpoint}");
        }
    }

    pub(crate) fn current_stage(&self) -> Option<String> {
        self.stages
            .last()
            .and_then(|stage| stage.get("name"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Keeps the sidecar on disk past Drop so a caught panic leaves the same
    /// post-mortem artifact a hard death would.
    pub(crate) fn preserve_for_post_mortem(&mut self) {
        self.preserve_on_drop = true;
        self.write_best_effort();
    }

    fn state_json(&self) -> Value {
        json!({
            "schema_version": 1,
            "schema_name": "agent_use_validation_stage_json",
            "surface": self.surface,
            "run_id": self.run_id,
            "started_unix_ms": self.started_unix_ms,
            "stages": self.stages,
            "purpose": "attributes which pipeline stage was active if this run died; deleted on every clean or structured-error finish",
            "graph_claimability_effect": "none",
            "public_claim": false,
        })
    }

    fn write_best_effort(&self) {
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.state_json()) {
            let _ = fs::write(&self.path, bytes);
        }
    }
}

impl Drop for ValidationStageTracker {
    fn drop(&mut self) {
        if !self.preserve_on_drop {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub(crate) fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

/// Minimal structured packet emitted when the post-commit validate pipeline
/// panics. The graph update is already committed and source-accurate at that
/// point; what this packet reports as broken is validation completeness,
/// never graph claimability.
pub(crate) fn agent_use_watch_pipeline_panic_packet(
    profile: &AgentUseProfile,
    changed_paths: &[String],
    failed_stage: &str,
    panic_message: &str,
    stage_file: &Path,
) -> Value {
    let repo_string = path_string(&profile.repo_root);
    json!({
        "schema_version": 1,
        "schema_name": "validate_edit_pipeline_error_json",
        "packet_kind": "validation_pipeline_error",
        "command": "watch",
        "subcommand": "once",
        "status": "error",
        "error": "validation_pipeline_panicked",
        "validation_pipeline_error": true,
        "validation_state": "incomplete",
        "failed_stage": failed_stage,
        "panic_message": panic_message,
        "changed_paths": changed_paths,
        "graph_commit_state": "committed_update_matches_source",
        "graph_claimability_unchanged": true,
        "validation_completeness": "incomplete_no_findings_were_derived",
        "validation_claimability": {
            "claimable": false,
            "diagnostic_only": true,
            "reason": "validation pipeline panicked before findings were derived",
        },
        "must_fix_before_continuing": false,
        "validation_stage_file": path_string(stage_file),
        "recovery": agent_use_recovery_json(profile),
        "recovery_commands": [
            format!(
                "{BIN_NAME} agent-use validate-edit --repo \"{repo_string}\" --changed <path> --agent-json"
            ),
            format!("{BIN_NAME} agent-use index --repo \"{repo_string}\" --json"),
        ],
        "public_claim": false,
        "_cli_exit_code": 1,
    })
}

// ===========================================================================
// 9.5.2a — Validation journal (two-phase validate-edit)
// ===========================================================================

pub(crate) const AGENT_USE_VALIDATION_JOURNAL_FILE_NAME: &str =
    "production-agent-use.validation-journal.json";
pub(crate) const VALIDATION_JOURNAL_VERSION: u32 = 1;
/// Cap on the serialized journal. On overflow the caller re-journals with
/// changed files only and labels the scope, per the MVP3.9.5b bounds rule.
pub(crate) const VALIDATION_JOURNAL_MAX_BYTES: usize = 8 * 1024 * 1024;

pub(crate) const VALIDATION_JOURNAL_STATE_UPDATE_PENDING: &str = "update_pending";
pub(crate) const VALIDATION_JOURNAL_STATE_VALIDATING: &str = "validating";

pub(crate) fn agent_use_validation_journal_path(profile: &AgentUseProfile) -> PathBuf {
    profile
        .delta_state_path
        .parent()
        .map(|parent| parent.join(AGENT_USE_VALIDATION_JOURNAL_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(AGENT_USE_VALIDATION_JOURNAL_FILE_NAME))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ValidationJournal {
    pub journal_version: u32,
    pub run_id: String,
    pub surface: String,
    pub started_unix_ms: u128,
    /// `update_pending` | `validating` | `failed:<message>`
    pub state: String,
    /// `changed_and_closure` | `changed_files_only` (bounded on overflow)
    pub journal_scope: String,
    pub changed_files: Vec<String>,
    pub closure_files: Vec<String>,
    pub snapshot_options: NormalizedFactSnapshotOptions,
    pub old_facts: NormalizedFactSnapshot,
}

impl ValidationJournal {
    pub(crate) fn is_replayable(&self) -> bool {
        self.state == VALIDATION_JOURNAL_STATE_VALIDATING || self.state.starts_with("failed:")
    }
}

fn write_json_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes)
        .map_err(|error| format!("sidecar temp write failed at {}: {error}", temp.display()))?;
    // Windows rename does not overwrite; the temp file is complete, so the
    // worst interleaving here leaves either the old or the new file intact.
    let _ = fs::remove_file(path);
    fs::rename(&temp, path)
        .map_err(|error| format!("sidecar rename failed at {}: {error}", path.display()))
}

pub(crate) fn write_validation_journal(
    profile: &AgentUseProfile,
    journal: &ValidationJournal,
) -> Result<usize, String> {
    let bytes = serde_json::to_vec(journal).map_err(|error| error.to_string())?;
    write_validation_journal_bytes(profile, &bytes)
}

/// Writes pre-serialized journal bytes, so callers that already serialized
/// (for the size bound) do not pay the multi-MB serialization twice.
pub(crate) fn write_validation_journal_bytes(
    profile: &AgentUseProfile,
    bytes: &[u8],
) -> Result<usize, String> {
    write_json_file_atomic(&agent_use_validation_journal_path(profile), bytes)?;
    Ok(bytes.len())
}

/// `Ok(None)` = no journal on disk. A corrupt journal is surfaced as an error
/// string, never a panic.
pub(crate) fn load_validation_journal(
    profile: &AgentUseProfile,
) -> Result<Option<ValidationJournal>, String> {
    let path = agent_use_validation_journal_path(profile);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| {
        format!(
            "validation journal unreadable at {}: {error}",
            path.display()
        )
    })?;
    serde_json::from_str::<ValidationJournal>(&text)
        .map(Some)
        .map_err(|error| format!("validation journal corrupt at {}: {error}", path.display()))
}

pub(crate) fn transition_validation_journal_state(profile: &AgentUseProfile, new_state: &str) {
    if let Ok(Some(mut journal)) = load_validation_journal(profile) {
        journal.state = new_state.to_string();
        let _ = write_validation_journal(profile, &journal);
    }
}

pub(crate) fn clear_validation_journal(profile: &AgentUseProfile) {
    let _ = fs::remove_file(agent_use_validation_journal_path(profile));
}

// ===========================================================================
// 9.5.2b — Persisted validation outcome + sticky open blockers
// ===========================================================================

pub(crate) const AGENT_USE_VALIDATION_STATE_FILE_NAME: &str =
    "production-agent-use.validation-state.json";
pub(crate) const VALIDATION_STATE_VERSION: u32 = 1;
const VALIDATION_STATE_MAX_OPEN_BLOCKERS: usize = 64;

pub(crate) fn agent_use_validation_state_path(profile: &AgentUseProfile) -> PathBuf {
    profile
        .delta_state_path
        .parent()
        .map(|parent| parent.join(AGENT_USE_VALIDATION_STATE_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(AGENT_USE_VALIDATION_STATE_FILE_NAME))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedOpenBlocker {
    pub first_seen_unix_ms: u128,
    pub last_reverified_unix_ms: u128,
    /// Same key shape the live rules use for dedup: `<rule_id>:<edge_id>`.
    pub dedup_key: String,
    /// Names whose absence from the repo graph the finding asserts.
    pub missing_target_names: Vec<String>,
    pub finding: ValidationFinding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ValidationStateRecord {
    pub schema_version: u32,
    /// `ok` | `blocked`
    pub state: String,
    pub completed_unix_ms: u128,
    pub source: String,
    pub changed_files: Vec<String>,
    pub final_severity: String,
    pub open_blockers: Vec<PersistedOpenBlocker>,
    pub open_blockers_omitted: usize,
    pub reemitted_blockers_last_run: usize,
    pub resolved_blockers_last_run: usize,
    pub uncheckable_blockers_last_run: usize,
    pub resolved_blockers_total: u64,
}

pub(crate) fn load_validation_state_record(
    profile: &AgentUseProfile,
) -> Option<ValidationStateRecord> {
    let path = agent_use_validation_state_path(profile);
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str::<ValidationStateRecord>(&text).ok()
}

pub(crate) fn write_validation_state_record(
    profile: &AgentUseProfile,
    record: &ValidationStateRecord,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(record).map_err(|error| error.to_string())?;
    write_json_file_atomic(&agent_use_validation_state_path(profile), &bytes)
}

pub(crate) fn clear_validation_state_record(profile: &AgentUseProfile) {
    let _ = fs::remove_file(agent_use_validation_state_path(profile));
}

fn short_symbol_name(name: &str) -> String {
    name.rsplit("::")
        .next()
        .unwrap_or(name)
        .rsplit('.')
        .next()
        .unwrap_or(name)
        .trim()
        .to_string()
}

/// Extracts the names a block-class finding asserts to be missing from the
/// repo graph, from the structured finding payloads. Entity ids are opaque
/// hashes (`repo://e/<digest>`), so names come from the delta entries
/// (`EntityDeltaEntry.name/qualified_name`) and endpoint summaries; a finding
/// without any recoverable name re-verifies as un-checkable (unknown), never
/// as a silent clear.
fn missing_target_names_for_finding(finding: &ValidationFinding) -> Vec<String> {
    let mut names = BTreeSet::new();
    let relation_kind = finding
        .relation_kind
        .as_ref()
        .map(|kind| format!("{kind:?}"))
        .or_else(|| {
            finding
                .affected_delta
                .get("relation_kind")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .or_else(|| {
            finding
                .affected_edge
                .get("relation_kind")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .unwrap_or_default();
    let is_alias_edge = relation_kind == "AliasedBy" || relation_kind == "ALIASED_BY";
    if is_alias_edge {
        for source in [
            finding
                .affected_delta
                .get("source_endpoint")
                .and_then(|endpoint| endpoint.get("name")),
            finding
                .affected_delta
                .get("source_endpoint")
                .and_then(|endpoint| endpoint.get("qualified_name")),
        ] {
            if let Some(name) = source.and_then(Value::as_str) {
                let short = short_symbol_name(name);
                if short.len() >= 3 {
                    names.insert(short);
                }
            }
        }
    }
    for source in [
        finding.affected_delta.get("name"),
        finding.affected_delta.get("qualified_name"),
        finding
            .affected_delta
            .get("target_endpoint")
            .and_then(|endpoint| endpoint.get("name")),
        finding
            .affected_delta
            .get("target_endpoint")
            .and_then(|endpoint| endpoint.get("qualified_name")),
        finding
            .affected_edge
            .get("target")
            .and_then(|target| target.get("name")),
        finding
            .affected_edge
            .get("target")
            .and_then(|target| target.get("qualified_name")),
        finding.affected_entity.get("name"),
        finding.affected_entity.get("qualified_name"),
    ] {
        if let Some(name) = source.and_then(Value::as_str) {
            let short = short_symbol_name(name);
            // Very short tokens (e.g. file-extension fragments of qualified
            // names) would make the whole-file reference scan meaningless.
            if short.len() >= 3 {
                names.insert(short);
            }
        }
    }
    names.into_iter().collect()
}

fn finding_dedup_key(finding: &ValidationFinding) -> String {
    let edge_id = finding
        .affected_edge
        .get("edge_id")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| finding.finding_id.clone());
    format!("{}:{}", finding.validation_rule_id, edge_id)
}

pub(crate) fn entity_kind_defines_symbol(kind: EntityKind) -> bool {
    codegraph_core::entity_kind_defines_symbol(kind)
}

pub(crate) enum OpenBlockerReverification {
    StillContradicted { fresh_evidence: Vec<String> },
    Resolved { reason: String },
    Uncheckable { reason: String },
}

/// Fresh re-verification of a persisted blocker against the CURRENT source
/// and graph. Never trusts the stored record: it re-reads the referencing
/// file and re-queries the repo graph for the missing target on every call.
pub(crate) fn reverify_open_blocker(
    repo_root: &Path,
    store: &SqliteGraphStore,
    blocker: &PersistedOpenBlocker,
) -> OpenBlockerReverification {
    if blocker.missing_target_names.is_empty() {
        return OpenBlockerReverification::Uncheckable {
            reason: "no missing-target name was recorded for this blocker".to_string(),
        };
    }
    for name in &blocker.missing_target_names {
        match store.find_entities_by_exact_symbol(name) {
            Ok(entities) => {
                // Only definition-shaped entities resolve a missing target.
                // Import/export bindings and usage sites (call sites, locals)
                // share the target's name but do not define it — counting
                // them would silently clear real blockers.
                let definitions = entities
                    .iter()
                    .filter(|entity| entity_kind_defines_symbol(entity.kind))
                    .count();
                if definitions > 0 {
                    return OpenBlockerReverification::Resolved {
                        reason: format!(
                            "a defining entity named `{name}` now exists in the repo graph ({definitions} definition match(es))"
                        ),
                    };
                }
            }
            Err(error) => {
                return OpenBlockerReverification::Uncheckable {
                    reason: format!("repo graph lookup for `{name}` failed: {error}"),
                };
            }
        }
    }
    let Some(file) = blocker.finding.file.clone().or_else(|| {
        blocker
            .finding
            .source_span
            .as_ref()
            .map(|span| span.repo_relative_path.clone())
    }) else {
        return OpenBlockerReverification::Uncheckable {
            reason: "blocker has no referencing file recorded".to_string(),
        };
    };
    let absolute = repo_root.join(&file);
    let source = match fs::read_to_string(&absolute) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return OpenBlockerReverification::Resolved {
                reason: format!("referencing file {file} no longer exists"),
            };
        }
        Err(error) => {
            return OpenBlockerReverification::Uncheckable {
                reason: format!("referencing file {file} unreadable: {error}"),
            };
        }
    };
    let referenced_name = blocker.missing_target_names.iter().find(|name| {
        source.match_indices(name.as_str()).any(|(index, matched)| {
            let before_ok = index == 0
                || !source[..index]
                    .chars()
                    .next_back()
                    .is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
            let after = index + matched.len();
            let after_ok = after >= source.len()
                || !source[after..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
            before_ok && after_ok
        })
    });
    match referenced_name {
        Some(name) => OpenBlockerReverification::StillContradicted {
            fresh_evidence: vec![
                format!(
                    "fresh source read: {file} still references `{name}` (whole-file token scan)"
                ),
                format!(
                    "fresh graph lookup: no entity named `{name}` exists in the current repo graph (find_entities_by_exact_symbol)"
                ),
            ],
        },
        None => OpenBlockerReverification::Resolved {
            reason: format!(
                "{file} no longer references any recorded missing target name (fresh source read)"
            ),
        },
    }
}

pub(crate) struct OpenBlockerApplySummary {
    pub previous_open: usize,
    pub reemitted: usize,
    pub resolved: usize,
    pub uncheckable: usize,
}

/// Loads persisted open blockers, re-verifies each against the current
/// store + source, and pushes still-contradicted ones (origin
/// `persisted_open_blocker`) plus uncheckable ones (downgraded to unknown)
/// into `findings`. Blockers whose dedup key the fresh delta already produced
/// are skipped — the fresh finding supersedes the persisted one.
pub(crate) fn agent_use_apply_persisted_open_blockers(
    profile: &AgentUseProfile,
    store: &SqliteGraphStore,
    lifecycle: ValidationLifecycleState,
    findings: &mut Vec<ValidationFinding>,
    seen_edge_rule: &mut BTreeSet<String>,
) -> OpenBlockerApplySummary {
    let mut summary = OpenBlockerApplySummary {
        previous_open: 0,
        reemitted: 0,
        resolved: 0,
        uncheckable: 0,
    };
    let Some(record) = load_validation_state_record(profile) else {
        return summary;
    };
    summary.previous_open = record.open_blockers.len();
    let now = unix_ms_now();
    for blocker in &record.open_blockers {
        if seen_edge_rule.contains(&blocker.dedup_key) {
            // The fresh delta re-derived this contradiction; its finding is
            // already in `findings` with current evidence.
            summary.reemitted += 1;
            continue;
        }
        match reverify_open_blocker(&profile.repo_root, store, blocker) {
            OpenBlockerReverification::StillContradicted { fresh_evidence } => {
                let mut finding = blocker.finding.clone();
                finding.lifecycle = lifecycle.clone();
                finding
                    .diagnostics
                    .push(format!("origin: persisted_open_blocker (first seen unix_ms {}, re-verified unix_ms {now})", blocker.first_seen_unix_ms));
                finding.diagnostics.extend(fresh_evidence);
                seen_edge_rule.insert(blocker.dedup_key.clone());
                findings.push(finding);
                summary.reemitted += 1;
            }
            OpenBlockerReverification::Resolved { .. } => {
                summary.resolved += 1;
            }
            OpenBlockerReverification::Uncheckable { reason } => {
                let mut finding = blocker.finding.clone();
                finding.classification = ValidationClassification::Unknown;
                finding.blocking_level = ValidationBlockingLevel::Unknown;
                finding.reverified_graph_source_proof = false;
                finding.lifecycle = lifecycle.clone();
                finding.reason = format!(
                    "previously blocking finding could not be re-verified: {reason} (original: {})",
                    finding.reason
                );
                finding
                    .diagnostics
                    .push("origin: persisted_open_blocker (re-verification failed; downgraded to unknown, never silently cleared)".to_string());
                seen_edge_rule.insert(blocker.dedup_key.clone());
                findings.push(finding);
                summary.uncheckable += 1;
            }
        }
    }
    summary
}

/// Persists this run's outcome: block-class findings become the new open
/// blockers (first-seen timestamps carried over for findings persisted
/// before).
pub(crate) fn persist_validation_outcome(
    profile: &AgentUseProfile,
    source: &str,
    changed_files: &[String],
    final_severity: &str,
    blocking_findings: &[ValidationFinding],
    apply_summary: &OpenBlockerApplySummary,
) {
    let now = unix_ms_now();
    let previous = load_validation_state_record(profile);
    let previous_first_seen = previous
        .as_ref()
        .map(|record| {
            record
                .open_blockers
                .iter()
                .map(|blocker| (blocker.dedup_key.clone(), blocker.first_seen_unix_ms))
                .collect::<std::collections::BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let resolved_blockers_total = previous
        .as_ref()
        .map(|record| record.resolved_blockers_total)
        .unwrap_or(0)
        + apply_summary.resolved as u64;
    let mut open_blockers = Vec::new();
    let mut omitted = 0usize;
    for finding in blocking_findings {
        if open_blockers.len() >= VALIDATION_STATE_MAX_OPEN_BLOCKERS {
            omitted += 1;
            continue;
        }
        let dedup_key = finding_dedup_key(finding);
        open_blockers.push(PersistedOpenBlocker {
            first_seen_unix_ms: previous_first_seen.get(&dedup_key).copied().unwrap_or(now),
            last_reverified_unix_ms: now,
            dedup_key,
            missing_target_names: missing_target_names_for_finding(finding),
            finding: finding.clone(),
        });
    }
    let record = ValidationStateRecord {
        schema_version: VALIDATION_STATE_VERSION,
        state: if open_blockers.is_empty() {
            "ok".to_string()
        } else {
            "blocked".to_string()
        },
        completed_unix_ms: now,
        source: source.to_string(),
        changed_files: changed_files.to_vec(),
        final_severity: final_severity.to_string(),
        open_blockers,
        open_blockers_omitted: omitted,
        reemitted_blockers_last_run: apply_summary.reemitted,
        resolved_blockers_last_run: apply_summary.resolved,
        uncheckable_blockers_last_run: apply_summary.uncheckable,
        resolved_blockers_total,
    };
    let _ = write_validation_state_record(profile, &record);
}

// ===========================================================================
// 9.5.2b — Journal replay + post-index recheck
// ===========================================================================

/// Replays a pending validation left behind by a run that died after its
/// graph commit. Journaled old facts vs the CURRENT DB → delta → validation
/// packet (which persists its outcome, including open blockers, through the
/// normal path) → journal cleared. Replayed findings reach the agent through
/// this run's validation packet after fresh re-verification — never as a
/// stale replayed claim. Returns a compact summary; never panics and never
/// hard-fails the surrounding run.
pub(crate) fn agent_use_replay_pending_validation(profile: &AgentUseProfile) -> Value {
    let journal = match load_validation_journal(profile) {
        Ok(None) => return json!({"replayed": false, "reason": "no_pending_journal"}),
        Ok(Some(journal)) => journal,
        Err(error) => {
            // Corrupt journal: surface it and clear — a validation of unknown
            // scope cannot be replayed, and the next runs must not brick.
            clear_validation_journal(profile);
            return json!({
                "replayed": false,
                "reason": "journal_corrupt_cleared",
                "error": error,
                "note": "a previous validation's completeness is unknown; run agent-use validate-edit on your changed files or agent-use index to re-derive",
            });
        }
    };
    if !journal.is_replayable() {
        // `update_pending` = the previous run died before/during its commit;
        // publish-state recovery owns that case and there is nothing to
        // replay (the DB never moved).
        clear_validation_journal(profile);
        return json!({
            "replayed": false,
            "reason": "pre_commit_journal_cleared",
            "journal_state": journal.state,
        });
    }
    let replay_result = (|| -> Result<Value, String> {
        let changed_paths = journal
            .changed_files
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let closure_paths = journal
            .closure_files
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        // Replay reads a committed baseline; one session feeds the snapshot
        // and the validation packet below (MVP3.9.5.3 residual).
        let replay_read_session =
            crate::open_normalized_fact_snapshot_session(&profile.repo_root, &profile.db_path)
                .map_err(|error| format!("replay snapshot failed: {error}"))?;
        let current_snapshot = replay_read_session
            .snapshot_for_paths(
                &changed_paths,
                &closure_paths,
                journal.snapshot_options.clone(),
            )
            .map_err(|error| format!("replay snapshot failed: {error}"))?;
        let delta = compute_entity_source_role_delta(
            &journal.old_facts,
            &current_snapshot,
            EntitySourceRoleDeltaOptions {
                max_items_per_category: crate::agent_use_validation_graph_delta_max_items(),
            },
        );
        let post_preflight = inspect_read_db_lifecycle_preflight(
            &profile.repo_root,
            &profile.db_path,
            Some(profile.scope_policy.clone()),
        )?;
        let graph_delta_json = agent_use_graph_delta_json(&delta);
        let (packet, replay_substages) = agent_use_exact_calls_validation_packet(
            profile,
            &post_preflight,
            &delta,
            graph_delta_json,
            journal.changed_files.clone(),
            None,
            crate::agent_use_validation_wall(None),
            Some(replay_read_session.store()),
        )?;
        // Residual (documented): a replay that is ITSELF bounded still clears
        // the journal below — the unknown outcome is persisted and labeled,
        // but skipped delta-derived findings are unrecoverable once the next
        // run's own journal overwrites this one; recovery is `agent-use
        // index`. Keeping the journal here cannot survive that overwrite.
        let replay_bounded = replay_substages
            .get("validation_bounded")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(json!({
            "replayed": true,
            "origin": "journal_replay",
            "journal_state": journal.state,
            "journal_scope": journal.journal_scope,
            "journal_started_unix_ms": journal.started_unix_ms,
            "changed_files": journal.changed_files,
            "final_status": serde_json::to_value(&packet.final_status).unwrap_or(Value::Null),
            "blocking_error_count": packet.blocking_errors.len(),
            "warning_count": packet.warnings.len(),
            "unknown_count": packet.unknowns.len(),
            "outcome_persisted": true,
            "bounded": journal.journal_scope == "changed_files_only" || replay_bounded,
            "replay_validation_bounded": replay_bounded,
            "note": "replayed block-class findings are persisted as open blockers and re-emitted by this run's validation packet after fresh re-verification",
        }))
    })();
    match replay_result {
        Ok(summary) => {
            clear_validation_journal(profile);
            summary
        }
        Err(error) => {
            transition_validation_journal_state(profile, &format!("failed:replay:{error}"));
            json!({
                "replayed": false,
                "reason": "replay_failed_journal_kept",
                "error": error,
                "recovery": format!("{BIN_NAME} agent-use index --repo \"{}\" --json clears the journal (a full reindex supersedes the pending validation)", path_string(&profile.repo_root)),
            })
        }
    }
}

/// After a full/incremental `agent-use index`, open blockers are re-checked
/// once against the freshly indexed store: still contradicted → kept,
/// resolved → cleared and counted, un-checkable → kept (never silently
/// cleared). The journal is cleared by the caller (reindex supersedes it).
pub(crate) fn agent_use_recheck_open_blockers_after_index(profile: &AgentUseProfile) -> Value {
    let Some(mut record) = load_validation_state_record(profile) else {
        return json!({"open_blockers_rechecked": 0});
    };
    if record.open_blockers.is_empty() {
        return json!({"open_blockers_rechecked": 0});
    }
    let store = match SqliteGraphStore::open_read_only(&profile.db_path) {
        Ok(store) => store,
        Err(error) => {
            return json!({
                "open_blockers_rechecked": 0,
                "error": format!("db unreadable for recheck: {error}"),
            });
        }
    };
    let now = unix_ms_now();
    let total = record.open_blockers.len();
    let mut kept = Vec::new();
    let mut resolved = 0usize;
    let mut uncheckable = 0usize;
    for mut blocker in record.open_blockers {
        match reverify_open_blocker(&profile.repo_root, &store, &blocker) {
            OpenBlockerReverification::StillContradicted { .. } => {
                blocker.last_reverified_unix_ms = now;
                kept.push(blocker);
            }
            OpenBlockerReverification::Resolved { .. } => {
                resolved += 1;
            }
            OpenBlockerReverification::Uncheckable { .. } => {
                uncheckable += 1;
                kept.push(blocker);
            }
        }
    }
    record.state = if kept.is_empty() {
        "ok".to_string()
    } else {
        "blocked".to_string()
    };
    record.completed_unix_ms = now;
    record.source = "agent_use_index_recheck".to_string();
    record.open_blockers = kept;
    record.resolved_blockers_last_run = resolved;
    record.uncheckable_blockers_last_run = uncheckable;
    record.reemitted_blockers_last_run = 0;
    record.resolved_blockers_total += resolved as u64;
    let kept_count = record.open_blockers.len();
    let _ = write_validation_state_record(profile, &record);
    json!({
        "open_blockers_rechecked": total,
        "open_blockers_kept": kept_count,
        "open_blockers_resolved": resolved,
        "open_blockers_uncheckable_kept": uncheckable,
    })
}

/// Compact `validation_state` block for status/watch/validate-edit surfaces.
/// `state`: `incomplete` (a journal survives — a validation never finished),
/// `blocked` (re-verifiable open blockers persisted), or `ok`.
pub(crate) fn agent_use_validation_state_block(profile: &AgentUseProfile) -> Value {
    let journal = load_validation_journal(profile);
    let record = load_validation_state_record(profile);
    let journal_pending = match &journal {
        Ok(Some(journal)) => journal.is_replayable(),
        Ok(None) => false,
        // Corrupt journal = a validation whose completeness is unknown.
        Err(_) => true,
    };
    let open_blocker_count = record
        .as_ref()
        .map(|record| record.open_blockers.len())
        .unwrap_or(0);
    let state = if journal_pending {
        "incomplete"
    } else if open_blocker_count > 0 {
        "blocked"
    } else {
        "ok"
    };
    json!({
        "state": state,
        "open_blocker_count": open_blocker_count,
        "since_unix_ms": record.as_ref().map(|record| record.completed_unix_ms),
        "journal_pending": journal_pending,
        "journal_state": match &journal {
            Ok(Some(journal)) => json!(journal.state),
            Ok(None) => Value::Null,
            Err(error) => json!(format!("corrupt: {error}")),
        },
        "resolved_blockers_total": record.as_ref().map(|record| record.resolved_blockers_total).unwrap_or(0),
        "graph_claimability_effect": "none",
        "expansion_handle": "sidecar:production-agent-use.validation-state.json",
        "public_claim": false,
    })
}

/// Byte-compact variant for size-edge surfaces (status): the full block only
/// when it carries signal; a bare `{"state":"ok"}` otherwise.
pub(crate) fn agent_use_validation_state_block_compact(profile: &AgentUseProfile) -> Value {
    let block = agent_use_validation_state_block(profile);
    if block["state"] == "ok" && block["open_blocker_count"].as_u64() == Some(0) {
        json!({"state": "ok"})
    } else {
        block
    }
}

#[cfg(test)]
mod validation_stage_tracker_tests {
    use super::*;
    use crate::IndexScopeOptions;

    fn stage_test_profile(root: &Path) -> AgentUseProfile {
        AgentUseProfile {
            profile_name: "test".to_string(),
            repo_root: root.to_path_buf(),
            repo_identity_label: "test".to_string(),
            repo_identity_hash: "test".to_string(),
            profile_root: root.to_path_buf(),
            db_path: root.join("validation.sqlite"),
            candidate_spool_path: root.join("candidate.jsonl"),
            candidate_spool_query_index_path: root.join("candidate.sqlite"),
            vector_runtime_path: root.join("vector-runtime.json"),
            vector_audit_path: root.join("vector-audit.json"),
            lock_or_publish_state_path: root.join("publish-state.json"),
            delta_state_path: root.join("delta-state.json"),
            lifecycle_expectations: Vec::new(),
            recovery_commands: Vec::new(),
            mcp_args: Vec::new(),
            binary_profile: "test".to_string(),
            scope_policy: IndexScopeOptions::default(),
        }
    }

    fn alias_open_blocker_finding() -> ValidationFinding {
        ValidationFinding {
            finding_id: "finding://test/alias".to_string(),
            validation_rule_id: "CG_MVP3_IMPORTS_ALIAS_TARGET_MISMATCH".to_string(),
            invariant: "exact import alias edges must resolve".to_string(),
            classification: ValidationClassification::Block,
            blocking_level: ValidationBlockingLevel::Blocking,
            integrity_kind: Some("broken_contract".to_string()),
            proof_level: "graph_source_reverified".to_string(),
            proof_strength: "deterministic_graph_source".to_string(),
            proof_status: codegraph_core::ValidationProofStatus::ReverifiedGraphSourceProof,
            affected_evidence: json!([]),
            affected_delta: json!({
                "relation_kind": "ALIASED_BY",
                "source_endpoint": {
                    "name": "targetTs",
                    "qualified_name": "src::ts::service.targetTs"
                },
                "target_endpoint": {
                    "name": "callTarget",
                    "qualified_name": "src::ts::consumer.import:callTarget"
                }
            }),
            affected_edge: json!({
                "relation_kind": "ALIASED_BY",
                "target": {
                    "name": "callTarget",
                    "qualified_name": "src::ts::consumer.import:callTarget"
                }
            }),
            affected_entity: json!({
                "entity_id": "repo://e/targetTs",
                "missing": true
            }),
            file: Some("src/ts/consumer.ts".to_string()),
            affected_file: Some("src/ts/consumer.ts".to_string()),
            source_span: None,
            source_role: None,
            relation_kind: Some(codegraph_core::RelationKind::AliasedBy),
            exactness: None,
            provenance: json!({}),
            rule_capability_contract: codegraph_core::ValidationRuleCapabilityContract::default(),
            capability_evaluation: codegraph_core::ValidationCapabilityEvaluation::default(),
            old_fact_claim_state: "unknown".to_string(),
            new_fact_claim_state: "claimable_current".to_string(),
            lifecycle: ValidationLifecycleState::claimable_current(),
            reverified_graph_source_proof: true,
            evidence_items: Vec::new(),
            reason: "exact import alias edge points to a missing target".to_string(),
            recommended_fix: None,
            suggested_next_steps: Vec::new(),
            unknowns: Vec::new(),
            diagnostics: Vec::new(),
            expansion_handle: None,
        }
    }

    #[test]
    fn alias_open_blocker_records_exported_target_name_for_reverify() {
        let names = missing_target_names_for_finding(&alias_open_blocker_finding());

        assert!(
            names.iter().any(|name| name == "targetTs"),
            "ALIASED_BY blockers must record the exported/source endpoint so restore reverify can clear: {names:?}"
        );
        assert!(
            names.iter().any(|name| name == "callTarget"),
            "alias binding name remains useful diagnostic context: {names:?}"
        );
    }

    #[test]
    fn validation_stage_tracker_sidecar_lifecycle() {
        let temp = std::env::temp_dir().join(format!(
            "cg-validation-stage-tracker-{}",
            std::process::id()
        ));
        fs::create_dir_all(&temp).expect("create temp dir");
        let profile = stage_test_profile(&temp);
        let stage_path = agent_use_validation_stage_path(&profile);

        {
            let mut tracker = ValidationStageTracker::start(&profile, "test.surface");
            tracker.mark("preflight");
            tracker.mark("delta");
            assert!(stage_path.exists(), "mark must persist the stage sidecar");
            let state: Value =
                serde_json::from_str(&fs::read_to_string(&stage_path).expect("read sidecar"))
                    .expect("sidecar json");
            assert_eq!(state["schema_name"], "agent_use_validation_stage_json");
            assert_eq!(state["surface"], "test.surface");
            assert_eq!(state["stages"][0]["name"], "preflight");
            assert_eq!(state["stages"][1]["name"], "delta");
            assert_eq!(state["public_claim"], false);
            assert_eq!(tracker.current_stage().as_deref(), Some("delta"));
        }
        assert!(
            !stage_path.exists(),
            "clean drop must remove the stage sidecar"
        );

        {
            let mut tracker = ValidationStageTracker::start(&profile, "test.surface");
            tracker.mark("validation");
            tracker.preserve_for_post_mortem();
        }
        assert!(
            stage_path.exists(),
            "preserved tracker must survive drop for post-mortem"
        );

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn corrupt_journal_is_surfaced_and_cleared_never_panics() {
        let temp = std::env::temp_dir().join(format!(
            "cg-validation-journal-corrupt-{}",
            std::process::id()
        ));
        fs::create_dir_all(&temp).expect("create temp dir");
        let profile = stage_test_profile(&temp);
        let journal_path = agent_use_validation_journal_path(&profile);
        fs::write(&journal_path, b"{not json at all").expect("write corrupt journal");

        // While the corrupt journal exists, surfaces must report incomplete:
        // a validation of unknown scope may never have finished.
        let block = agent_use_validation_state_block(&profile);
        assert_eq!(block["state"], "incomplete", "{block}");
        assert!(
            block["journal_state"]
                .as_str()
                .unwrap_or_default()
                .starts_with("corrupt:"),
            "{block}"
        );

        // Replay surfaces the corruption and clears the journal — no panic,
        // no permanent brick.
        let summary = agent_use_replay_pending_validation(&profile);
        assert_eq!(summary["replayed"], false, "{summary}");
        assert_eq!(summary["reason"], "journal_corrupt_cleared", "{summary}");
        assert!(!journal_path.exists());
        assert_eq!(
            agent_use_validation_state_block(&profile)["state"],
            "ok",
            "after clearing, no pending journal remains"
        );

        let _ = fs::remove_dir_all(&temp);
    }
}
