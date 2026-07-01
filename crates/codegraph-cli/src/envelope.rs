//! Agent-use agent-json envelope compaction and output-budget enforcement.
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use serde_json::{json, Value};

use crate::*;

pub(crate) fn enforce_agent_use_context_pack_max_output_bytes(
    value: &mut Value,
    max_output_bytes: usize,
) {
    for _ in 0..64 {
        if serialized_json_len(value) <= max_output_bytes {
            break;
        }
        let _ = enforce_context_agent_max_output_bytes(value, max_output_bytes);
        if compact_context_agent_staged_availability(value) {
            continue;
        }
        if compact_context_agent_micro_flow_handles(value) {
            continue;
        }
        if compact_context_agent_db_lifecycle_read(value) {
            continue;
        }
        if compact_context_agent_patch_assist_packet_minimal(value) {
            continue;
        }
        if compact_context_agent_publish_state(value) {
            continue;
        }
        for key in [
            "fallback_evidence",
            "follow_up_queries",
            "likely_files",
            "paths",
            "proof_paths",
            "recommended_tests",
            "risks",
            "snippets",
            "agent_use_profile_root",
            "candidate_spool_query_index_path",
            "candidate_spool_query_index_bytes",
            "candidate_spool_query_index_kind",
            "candidate_spool_query_index_record_count",
            "normal_dot_codegraph_path",
            "resolved_db",
            "repo_root",
            "symbols",
            "critical_symbols",
            "candidate_total_count",
            "candidate_omitted_count",
            "candidate_sources",
            "blocked_labels",
            "retryable_labels",
        ] {
            if remove_context_agent_field(value, key) {
                break;
            }
        }
    }
    let output_bytes = serialized_json_len(value);
    if let Some(object) = value.as_object_mut() {
        if let Some(truncation) = object.get_mut("truncation").and_then(Value::as_object_mut) {
            truncation.insert("output_bytes".to_string(), json!(output_bytes));
        }
    }
}

pub(crate) fn compact_context_agent_publish_state(value: &mut Value) -> bool {
    let Some(publish_state) = value.get("publish_state").cloned() else {
        return false;
    };
    let compact = compact_agent_use_publish_state_summary(&publish_state);
    if compact == publish_state {
        return false;
    }
    if let Some(object) = value.as_object_mut() {
        object.insert("publish_state".to_string(), compact);
        return true;
    }
    false
}

pub(crate) fn compact_context_agent_micro_flow_handles(value: &mut Value) -> bool {
    let Some(handles) = value.get("micro_flow_handles").and_then(Value::as_array) else {
        return false;
    };
    let handle_count = handles.len();
    let compacted = handles
        .iter()
        .take(1)
        .map(compact_context_agent_micro_flow_handle)
        .collect::<Vec<_>>();
    let returned_count = compacted.len();
    let omitted_count = handle_count.saturating_sub(returned_count) as u64;
    let changed = handle_count > returned_count
        || handles
            .iter()
            .zip(compacted.iter())
            .any(|(before, after)| before != after);
    if !changed {
        return false;
    }
    if let Some(object) = value.as_object_mut() {
        object.insert("micro_flow_handles".to_string(), json!(compacted));
        if let Some(summary) = object
            .get_mut("micro_flow_packet_summary")
            .and_then(Value::as_object_mut)
        {
            summary.insert("handle_count_returned".to_string(), json!(returned_count));
            summary.insert("handle_omitted_count".to_string(), json!(omitted_count));
        }
        return true;
    }
    false
}

fn compact_context_agent_micro_flow_handle(handle: &Value) -> Value {
    json!({
        "handle_id": handle.get("handle_id").cloned().unwrap_or(Value::Null),
        "packet_id": handle.get("packet_id").cloned().unwrap_or(Value::Null),
        "file": handle.get("file").cloned().unwrap_or(Value::Null),
        "function_identity": handle.get("function_identity").cloned().unwrap_or(Value::Null),
        "function_frame_micro_node_id": handle.get("function_frame_micro_node_id").cloned().unwrap_or(Value::Null),
        "proof_status": handle.get("proof_status").cloned().unwrap_or(Value::Null),
        "proof_strength": handle.get("proof_strength").cloned().unwrap_or(Value::Null),
        "currentness": handle.get("currentness").cloned().unwrap_or(Value::Null),
        "packet_kind": handle.get("packet_kind").cloned().unwrap_or(Value::Null),
        "expansion_handle": handle.get("expansion_handle").cloned().unwrap_or(Value::Null),
        "ordered_steps_inline": false,
        "packet_body_inline": false,
        "full_source_body_output": false,
        "handle_creates_proof": false,
        "agent_json_compacted": true,
    })
}

pub(crate) fn compact_agent_use_agent_json_envelope(
    value: &mut Value,
    profile: &AgentUseProfile,
    detail_mode: AgentUseDetailMode,
    max_output_bytes: usize,
    staged_availability: Option<&Value>,
) {
    let mut truncated_sections = Vec::new();
    let mut omitted_count = 0u64;
    agent_use_ensure_required_agent_fields(value, profile);
    agent_use_dedupe_recovery_commands(value, profile, &mut omitted_count);

    if detail_mode.preserves_full_details() {
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "agent_json_detail_mode".to_string(),
                json!(match detail_mode {
                    AgentUseDetailMode::Compact => "compact",
                    AgentUseDetailMode::Explain => "explain",
                    AgentUseDetailMode::Audit => "audit",
                }),
            );
        }
    } else {
        agent_use_compact_recovery_fields(
            value,
            profile,
            &mut truncated_sections,
            &mut omitted_count,
        );
        agent_use_compact_lifecycle_fields(value, &mut truncated_sections, &mut omitted_count);
        agent_use_compact_staged_availability_field(
            value,
            staged_availability,
            &mut truncated_sections,
            &mut omitted_count,
        );
        agent_use_compact_local_flow_packet_visibility_field(
            value,
            &mut truncated_sections,
            &mut omitted_count,
        );
        agent_use_compact_rtds_freshness_field(value, &mut truncated_sections, &mut omitted_count);
        agent_use_compact_stale_candidate_layers_field(
            value,
            &mut truncated_sections,
            &mut omitted_count,
        );
        // Preserve a minimal top-level last_delta_update_summary (status + counts)
        // rather than dropping it entirely; agents and the delta contract need
        // `last_delta_update_summary.status` after a watch/delta update.
        if let Some(summary) = value.get("last_delta_update_summary").cloned() {
            if !summary.is_null() {
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "last_delta_update_summary".to_string(),
                        compact_agent_use_last_delta_update_summary(&summary),
                    );
                }
            }
        }
        if let Some(update_queue_state) = value.get("update_queue_state").cloned() {
            if !update_queue_state.is_null() {
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "update_queue_state".to_string(),
                        compact_agent_use_update_queue_state_summary(&update_queue_state),
                    );
                }
                truncated_sections.push("update_queue_state".to_string());
                omitted_count = omitted_count.saturating_add(1);
            }
        }
        agent_use_compact_publish_state_field(value, &mut truncated_sections, &mut omitted_count);
        agent_use_compact_lock_state_field(value, &mut truncated_sections, &mut omitted_count);
        agent_use_compact_discovery_fields(value, &mut truncated_sections, &mut omitted_count);
        agent_use_compact_read_path_metrics_field(
            value,
            &mut truncated_sections,
            &mut omitted_count,
        );
        agent_use_compact_query_results_field(value, &mut truncated_sections, &mut omitted_count);
        let command = value.get("command").and_then(Value::as_str);
        let preserve_candidate_source_safety =
            matches!(command, Some("context-pack" | "status" | "watch"));
        for key in [
            "agent_use_profile_name",
            "output_mode",
            "repo_source",
            "resolved_repo",
            "profile",
            "agent_use_profile",
            "db_health",
            "sqlite_sidecars",
            "agent_use_profile_root",
            "normal_dot_codegraph_path",
            "repo_root",
            "resolved_db",
            "db_exists",
            "db_created",
            "profile_parent_exists",
            "profile_parent_created",
            "candidate_spool_query_index_path",
            "candidate_spool_query_index_bytes",
            "candidate_spool_query_index_kind",
            "candidate_spool_query_index_record_count",
            "vector_runtime_path",
            "vector_audit_path",
            "scope_examples",
            "sidecar_manifests",
            "blocked_labels",
            "blockers",
            "active_candidate_sources",
            "candidate_context_truncated",
            "candidate_spool_unavailable",
            "lifecycle_decision",
            "outside_workspace_note",
            "path_access_error",
            "path_access_status",
            "files",
            "entities",
            "relation_facts",
            "source_span_facts",
            "edges",
            "source_spans",
            "languages",
        ] {
            if preserve_candidate_source_safety && key == "active_candidate_sources" {
                continue;
            }
            if agent_use_remove_field(value, key) {
                truncated_sections.push(key.to_string());
                omitted_count = omitted_count.saturating_add(1);
            }
        }
    }

    // Evidence-first budgeting (MVP_3.md section 14): reduce the large MVP4.3
    // micro-flow handle bodies, each recoverable via its audit/explain
    // expansion handle and already summarized in micro_flow_packet_summary,
    // BEFORE the budget enforcers shed small, contract-required agent-state
    // (staged_availability / read_path_metrics / sidecar_statuses). Otherwise a
    // context-pack packet carrying two full handles crowds that state out of
    // budget (large expandable evidence must go before small required state).
    if serialized_json_len(value) > max_output_bytes {
        compact_context_agent_micro_flow_handles(value);
    }

    let enforcement_budget = max_output_bytes
        .saturating_sub(1024)
        .max(max_output_bytes.min(MIN_CONTEXT_AGENT_MAX_OUTPUT_BYTES));
    agent_use_enforce_total_output_budget(
        value,
        enforcement_budget,
        &mut truncated_sections,
        &mut omitted_count,
    );
    if serialized_json_len(value) > max_output_bytes {
        let hard_budget = if max_output_bytes <= 4096 {
            enforcement_budget
        } else {
            max_output_bytes
        };
        agent_use_enforce_hard_agent_json_budget(
            value,
            hard_budget,
            max_output_bytes <= 4096,
            &mut truncated_sections,
            &mut omitted_count,
        );
    }
    agent_use_finalize_agent_json_budget(
        value,
        detail_mode,
        max_output_bytes,
        truncated_sections,
        omitted_count,
    );
}

pub(crate) fn agent_use_ensure_required_agent_fields(value: &mut Value, profile: &AgentUseProfile) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let command = object
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    object
        .entry("schema_version".to_string())
        .or_insert_with(|| json!(AGENT_JSON_SCHEMA_VERSION));
    if !object.contains_key("schema_name") {
        let schema_name = match command.as_str() {
            "status" => Some("status_compact_json"),
            "context-pack" | "context" => Some("context_pack_agent_json"),
            _ => None,
        };
        if let Some(schema_name) = schema_name {
            object.insert("schema_name".to_string(), json!(schema_name));
        }
    }
    object
        .entry("repo".to_string())
        .or_insert_with(|| json!(path_string(&profile.repo_root)));
    object
        .entry("db".to_string())
        .or_insert_with(|| json!(path_string(&profile.db_path)));
    object
        .entry("db_path".to_string())
        .or_insert_with(|| json!(path_string(&profile.db_path)));
    object
        .entry("db_source".to_string())
        .or_insert_with(|| json!("agent-use profile"));
    object
        .entry("external_db_used".to_string())
        .or_insert_with(|| json!(true));
    object
        .entry("profile_name".to_string())
        .or_insert_with(|| json!(profile.profile_name.clone()));
    object
        .entry("repo_identity_label".to_string())
        .or_insert_with(|| json!(profile.repo_identity_label.clone()));
    object
        .entry("repo_identity_hash".to_string())
        .or_insert_with(|| json!(profile.repo_identity_hash.clone()));
    object
        .entry("repo_identity_short_hash".to_string())
        .or_insert_with(|| json!(agent_use_repo_identity_short_hash(profile)));
    object
        .entry("warnings".to_string())
        .or_insert_with(|| json!([]));
    object
        .entry("errors".to_string())
        .or_insert_with(|| json!([]));
    object
        .entry("public_claim".to_string())
        .or_insert_with(|| json!(false));
    object
        .entry("normal_dot_codegraph_mutated".to_string())
        .or_insert_with(|| json!(false));

    let claimable = object
        .get("claimable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let diagnostic_only = object
        .get("diagnostic_only")
        .and_then(Value::as_bool)
        .unwrap_or(!claimable);
    let graph_proof = object
        .get("graph_proof")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let proof_status = object
        .get("proof_status")
        .and_then(Value::as_str)
        .unwrap_or(if graph_proof {
            "proof_path_found"
        } else {
            "not_graph_proof"
        })
        .to_string();
    let proof_strength = object
        .get("proof_strength")
        .and_then(Value::as_str)
        .unwrap_or(if graph_proof {
            "graph_source_verified"
        } else {
            "none"
        })
        .to_string();
    object
        .entry("graph_proof".to_string())
        .or_insert_with(|| json!(graph_proof));
    object
        .entry("proof_status".to_string())
        .or_insert_with(|| json!(proof_status));
    object
        .entry("proof_strength".to_string())
        .or_insert_with(|| json!(proof_strength));
    object.entry("claimability".to_string()).or_insert_with(|| {
        json!({
            "claimable": claimable,
            "diagnostic_only": diagnostic_only,
            "graph_proof": graph_proof,
            "proof_status": proof_status,
            "proof_strength": proof_strength,
            "graph_proof_only_from_graph_source_verification": true,
            "candidate_evidence_is_not_graph_proof": true,
            "text_evidence_is_not_graph_proof": true,
            "vector_evidence_is_not_graph_proof": true,
        })
    });
}

pub(crate) fn agent_use_recovery_reference_json(profile: &AgentUseProfile) -> Value {
    let recovery = agent_use_recovery_json(profile);
    json!({
        "id": "agent_use_recovery",
        "commands_ref": "recovery_commands",
        "agent_use_index_command": recovery.get("agent_use_index_command").cloned().unwrap_or(Value::Null),
        "agent_use_status_command": recovery.get("agent_use_status_command").cloned().unwrap_or(Value::Null),
        "agent_use_mcp_config_command": recovery.get("agent_use_mcp_config_command").cloned().unwrap_or(Value::Null),
        "agent_use_query_symbols_command": recovery.get("agent_use_query_symbols_command").cloned().unwrap_or(Value::Null),
        "agent_use_query_text_command": recovery.get("agent_use_query_text_command").cloned().unwrap_or(Value::Null),
        "agent_use_query_files_command": recovery.get("agent_use_query_files_command").cloned().unwrap_or(Value::Null),
        "agent_use_context_pack_command": recovery.get("agent_use_context_pack_command").cloned().unwrap_or(Value::Null),
        "agent_use_watch_once_command": recovery.get("agent_use_watch_once_command").cloned().unwrap_or(Value::Null),
        "agent_use_mcp_config_available": true,
        "agent_use_mcp_config_status": "implemented",
        "agent_use_query_available": true,
        "agent_use_query_status": "implemented",
        "agent_use_watch_available": true,
        "agent_use_watch_status": "implemented_once_changed",
    })
}

pub(crate) fn agent_use_dedupe_recovery_commands(
    value: &mut Value,
    profile: &AgentUseProfile,
    omitted_count: &mut u64,
) {
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "recovery_commands".to_string(),
            json!(profile.recovery_commands.clone()),
        );
        object.insert(
            "recovery".to_string(),
            agent_use_recovery_reference_json(profile),
        );
    }
    agent_use_dedupe_nested_recovery_commands(value, true, omitted_count);
}

pub(crate) fn agent_use_compact_recovery_fields(
    value: &mut Value,
    profile: &AgentUseProfile,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    if let Some(object) = value.as_object_mut() {
        if object.get("command").and_then(Value::as_str) == Some("watch") {
            object.insert(
                "recovery_commands".to_string(),
                json!(profile.recovery_commands.clone()),
            );
            object.insert(
                "recovery".to_string(),
                agent_use_recovery_reference_json(profile),
            );
            return;
        }
        object.insert(
            "recovery_commands".to_string(),
            compact_agent_use_recovery_commands_json(),
        );
        object.insert(
            "recovery".to_string(),
            json!({
                "id": "agent_use_recovery",
                "commands_ref": "recovery_commands",
                "repo_ref": "repo",
                "db_ref": "db",
                "agent_use_index_command": format!("{BIN_NAME} agent-use index --repo <repo> --json"),
                "placeholder_policy": "substitute <repo> with the top-level repo field",
                "agent_use_mcp_config_available": true,
                "agent_use_mcp_config_status": "implemented",
                "agent_use_query_available": true,
                "agent_use_query_status": "implemented",
                "agent_use_watch_available": true,
                "agent_use_watch_status": "implemented_once_changed",
            }),
        );
    }
    truncated_sections.push("recovery_commands".to_string());
    *omitted_count = omitted_count.saturating_add(1);
}

pub(crate) fn compact_agent_use_recovery_commands_json() -> Value {
    json!({
        "kind": "agent_use_recovery_commands",
        "repo_ref": "repo",
        "db_ref": "db",
        "commands": {
            "status": format!("{BIN_NAME} agent-use status --repo <repo> --json"),
            "index": format!("{BIN_NAME} agent-use index --repo <repo> --json"),
            "mcp_config": format!("{BIN_NAME} agent-use mcp-config --repo <repo> --json"),
            "context_pack": format!("{BIN_NAME} agent-use context-pack --repo <repo> --task \"<task>\" --agent-json"),
            "watch_once": format!("{BIN_NAME} agent-use watch --repo <repo> --once --changed <path> --json"),
        },
    })
}

pub(crate) fn agent_use_dedupe_nested_recovery_commands(
    value: &mut Value,
    is_root: bool,
    omitted_count: &mut u64,
) {
    match value {
        Value::Object(object) => {
            if !is_root && object.remove("recovery_commands").is_some() {
                object.insert(
                    "recovery_commands_ref".to_string(),
                    json!("recovery_commands"),
                );
                *omitted_count = omitted_count.saturating_add(1);
            }
            let looks_like_recovery = object.contains_key("agent_use_index_command")
                || object.contains_key("agent_use_status_command")
                || object.contains_key("agent_use_context_pack_command");
            if looks_like_recovery && object.remove("commands").is_some() {
                object.insert("commands_ref".to_string(), json!("recovery_commands"));
                *omitted_count = omitted_count.saturating_add(1);
            }
            for child in object.values_mut() {
                agent_use_dedupe_nested_recovery_commands(child, false, omitted_count);
            }
        }
        Value::Array(array) => {
            for child in array {
                agent_use_dedupe_nested_recovery_commands(child, false, omitted_count);
            }
        }
        _ => {}
    }
}

pub(crate) fn agent_use_compact_lifecycle_fields(
    value: &mut Value,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    let lifecycle = value.get("db_lifecycle_read").cloned();
    if let Some(lifecycle) = lifecycle {
        let compact = compact_agent_use_lifecycle_summary(&lifecycle);
        let public = compact_agent_use_public_lifecycle_summary(&compact);
        if let Some(object) = value.as_object_mut() {
            object.insert("db_lifecycle_read".to_string(), compact.clone());
            object.insert("lifecycle".to_string(), public);
        }
        truncated_sections.push("db_lifecycle_read".to_string());
        *omitted_count = omitted_count.saturating_add(1);
    } else if let Some(lifecycle) = value.get("lifecycle").cloned() {
        let compact = compact_agent_use_public_lifecycle_summary(
            &compact_agent_use_lifecycle_summary(&lifecycle),
        );
        if let Some(object) = value.as_object_mut() {
            object.insert("lifecycle".to_string(), compact);
        }
        truncated_sections.push("lifecycle".to_string());
        *omitted_count = omitted_count.saturating_add(1);
    }
}

pub(crate) fn compact_agent_use_lifecycle_summary(lifecycle: &Value) -> Value {
    json!({
        "claimable": lifecycle.get("claimable").cloned().unwrap_or_else(|| json!(false)),
        "diagnostic_only": lifecycle.get("diagnostic_only").cloned().unwrap_or_else(|| json!(true)),
        "decision": lifecycle.get("decision").cloned().unwrap_or_else(|| json!("unknown")),
        "db_problem_kind": lifecycle.get("db_problem_kind").cloned().unwrap_or(Value::Null),
        "path_access_status": lifecycle.get("path_access_status").cloned().unwrap_or(Value::Null),
        "path_access_error": lifecycle.get("path_access_error").cloned().unwrap_or(Value::Null),
        "artifact_freshness": lifecycle.get("artifact_freshness").cloned().unwrap_or(Value::Null),
        "passport_status": lifecycle.get("passport_status").cloned().unwrap_or(Value::Null),
        "schema_status": lifecycle.get("schema_status").cloned().unwrap_or(Value::Null),
        "storage_mode_status": lifecycle.get("storage_mode_status").cloned().unwrap_or(Value::Null),
        "scope_status": lifecycle.get("scope_status").cloned().unwrap_or(Value::Null),
        "repo_root_status": lifecycle.get("repo_root_status").cloned().unwrap_or(Value::Null),
        "sidecar_status": lifecycle.get("sidecar_status").cloned().unwrap_or(Value::Null),
        "exact_db_path_checked": lifecycle.get("exact_db_path_checked").cloned().unwrap_or(Value::Null),
        "db_path_outside_workspace": lifecycle.get("db_path_outside_workspace").cloned().unwrap_or(Value::Null),
        "allow_stale_read": lifecycle.get("allow_stale_read").cloned().unwrap_or(Value::Null),
        "allow_foreign_db": lifecycle.get("allow_foreign_db").cloned().unwrap_or(Value::Null),
        "blocker_count": lifecycle.get("blockers").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        "blockers_ref": "errors",
        "warning_count": lifecycle.get("warnings").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        "warnings_ref": "warnings",
        "agent_json_compacted": true,
    })
}

pub(crate) fn compact_agent_use_public_lifecycle_summary(lifecycle: &Value) -> Value {
    json!({
        "claimable": lifecycle.get("claimable").cloned().unwrap_or_else(|| json!(false)),
        "diagnostic_only": lifecycle.get("diagnostic_only").cloned().unwrap_or_else(|| json!(true)),
        "decision": lifecycle.get("decision").cloned().unwrap_or_else(|| json!("unknown")),
        "db_problem_kind": lifecycle.get("db_problem_kind").cloned().unwrap_or(Value::Null),
        "path_access_status": lifecycle.get("path_access_status").cloned().unwrap_or(Value::Null),
        "artifact_freshness": lifecycle.get("artifact_freshness").cloned().unwrap_or(Value::Null),
        "passport_status": lifecycle.get("passport_status").cloned().unwrap_or(Value::Null),
        "schema_status": lifecycle.get("schema_status").cloned().unwrap_or(Value::Null),
        "scope_status": lifecycle.get("scope_status").cloned().unwrap_or(Value::Null),
        "sidecar_status": lifecycle.get("sidecar_status").cloned().unwrap_or(Value::Null),
        "blocker_count": lifecycle.get("blocker_count").cloned().unwrap_or_else(|| json!(0)),
        "warning_count": lifecycle.get("warning_count").cloned().unwrap_or_else(|| json!(0)),
        "agent_json_compacted": true,
    })
}

pub(crate) fn agent_use_compact_staged_availability_field(
    value: &mut Value,
    staged_availability: Option<&Value>,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    let source = value
        .get("staged_availability")
        .or(staged_availability)
        .cloned();
    let Some(staged) = source else {
        return;
    };
    let compact = compact_agent_use_staged_availability_summary(&staged);
    if let Some(object) = value.as_object_mut() {
        object.insert("staged_availability".to_string(), compact);
    }
    truncated_sections.push("staged_availability".to_string());
    *omitted_count = omitted_count.saturating_add(1);
}

pub(crate) fn agent_use_compact_local_flow_packet_visibility_field(
    value: &mut Value,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    let Some(layer) = value.get("mvp4_local_flow_packets").cloned() else {
        return;
    };
    let compact = compact_agent_use_local_flow_packet_visibility_summary(&layer);
    if compact == layer {
        return;
    }
    if let Some(object) = value.as_object_mut() {
        object.insert("mvp4_local_flow_packets".to_string(), compact);
    }
    truncated_sections.push("mvp4_local_flow_packets".to_string());
    *omitted_count = omitted_count.saturating_add(1);
}

pub(crate) fn compact_agent_use_local_flow_packet_visibility_summary(layer: &Value) -> Value {
    json!({
        "status": layer.get("status").cloned().unwrap_or(Value::Null),
        "ready": layer.get("ready").cloned().unwrap_or_else(|| json!(false)),
        "schema_version": layer.get("schema_version").cloned().unwrap_or(Value::Null),
        "total_rows": layer.get("total_rows").cloned().unwrap_or_else(|| json!(0)),
        "cap_hit_count": layer.get("cap_hit_count").cloned().unwrap_or_else(|| json!(0)),
        "omitted_count": layer.get("omitted_count").cloned().unwrap_or_else(|| json!(0)),
        "currentness_status": layer.get("currentness_status").cloned().unwrap_or(Value::Null),
        "ordered_steps_inline": false,
        "packet_body_inline": false,
        "proof_boundary_ref": "audit local-flow-packets",
        "agent_json_compacted": true,
    })
}

pub(crate) fn compact_agent_use_staged_availability_summary(staged: &Value) -> Value {
    json!({
        "graph_db_status": staged.get("graph_db_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_status": staged.get("candidate_spool_status").cloned().unwrap_or(Value::Null),
        "candidate_spool_query_index_status": staged.get("candidate_spool_query_index_status").cloned().unwrap_or(Value::Null),
        "vector_runtime_status": staged.get("vector_runtime_status").cloned().unwrap_or(Value::Null),
        "vector_audit_status": staged.get("vector_audit_status").cloned().unwrap_or(Value::Null),
        "graph_proof_available": staged.get("graph_proof_available").cloned().unwrap_or(Value::Null),
        "candidate_only_available": staged.get("candidate_only_available").cloned().unwrap_or(Value::Null),
        "candidate_context_available": staged.get("candidate_context_available").cloned().unwrap_or(Value::Null),
        "active_candidate_sources": staged.get("active_candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "claimability": staged.get("claimability").cloned().unwrap_or(Value::Null),
        "layer_readiness": compact_agent_use_layer_readiness_summary(staged.get("layer_readiness")),
        "blocker_count": staged.get("blockers").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        "warning_count": staged.get("warnings").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        "agent_json_compacted": true,
        "public_claim": false,
    })
}

/// Compact per-layer readiness: keep only the small boolean/status flags an
/// agent (and the contract tests) need, dropping verbose path/blocker/reason
/// detail that would re-bloat the compact envelope past its byte budget.
pub(crate) fn compact_agent_use_layer_readiness_summary(layer_readiness: Option<&Value>) -> Value {
    let Some(layers) = layer_readiness.and_then(Value::as_object) else {
        return Value::Null;
    };
    let mut compact = serde_json::Map::new();
    for (name, layer) in layers {
        let pick = |key: &str| layer.get(key).cloned().unwrap_or(Value::Null);
        compact.insert(
            name.clone(),
            json!({
                "status": pick("status"),
                "ready": pick("ready"),
                "graph_proof": pick("graph_proof"),
                "candidate_only": pick("candidate_only"),
                "diagnostic_only": pick("diagnostic_only"),
                "runtime_dependency": pick("runtime_dependency"),
                "query_index_status": pick("query_index_status"),
            }),
        );
    }
    Value::Object(compact)
}

/// Minimal last-delta-update summary: keep the small contract fields (status and
/// the changed/dirty counts an agent acts on), drop verbose per-file detail.
pub(crate) fn compact_agent_use_last_delta_update_summary(summary: &Value) -> Value {
    if summary.is_null() {
        return Value::Null;
    }
    json!({
        "status": summary.get("status").cloned().unwrap_or(Value::Null),
        "changed_files": summary.get("changed_files").cloned().unwrap_or(Value::Null),
        "delta_state": summary.get("delta_state").cloned().unwrap_or(Value::Null),
        "new_graph_valid": summary.get("new_graph_valid").cloned().unwrap_or(Value::Null),
        "old_db_preserved": summary.get("old_db_preserved").cloned().unwrap_or(Value::Null),
        "agent_json_compacted": true,
    })
}

/// Minimal update-queue state for compact status/watch/context envelopes.  The
/// full nested `last_update_summary` can repeat the entire delta packet and
/// crowd out spans/actions under budget pressure; the separate
/// `last_delta_update_summary` and `dirty_evidence_summary` fields carry the
/// actionable evidence.
pub(crate) fn compact_agent_use_update_queue_state_summary(state: &Value) -> Value {
    if state.is_null() {
        return Value::Null;
    }
    json!({
        "status": state.get("status").cloned().unwrap_or(Value::Null),
        "active_update": state.get("active_update").cloned().unwrap_or(Value::Null),
        "auto_index_enabled": state.get("auto_index_enabled").cloned().unwrap_or(Value::Null),
        "updates_attempted": state.get("updates_attempted").cloned().unwrap_or(Value::Null),
        "updates_succeeded": state.get("updates_succeeded").cloned().unwrap_or(Value::Null),
        "updates_failed": state.get("updates_failed").cloned().unwrap_or(Value::Null),
        "old_db_preserved": state.get("old_db_preserved").cloned().unwrap_or(Value::Null),
        "unrelated_repo_blocking": state.get("unrelated_repo_blocking").cloned().unwrap_or(Value::Null),
        "last_error": state.get("last_error").cloned().unwrap_or(Value::Null),
        "last_update_summary": state
            .get("last_update_summary")
            .map(compact_agent_use_last_delta_update_summary)
            .unwrap_or(Value::Null),
        "agent_json_compacted": true,
    })
}

pub(crate) fn agent_use_compact_rtds_freshness_field(
    value: &mut Value,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    let Some(rtds) = value.get("rtds_freshness").cloned() else {
        return;
    };
    let compact = json!({
        "schema_version": rtds.get("schema_version").cloned().unwrap_or_else(|| json!(1)),
        "graph_freshness": rtds.get("graph_freshness").cloned().unwrap_or(Value::Null),
        "dirty_state": rtds.get("dirty_state").cloned().unwrap_or(Value::Null),
        "delta_state": rtds.get("delta_state").cloned().unwrap_or(Value::Null),
        "publish_state": rtds.get("publish_state").map(compact_agent_use_publish_state_summary).unwrap_or(Value::Null),
        "stale_candidate_layers": rtds.get("stale_candidate_layers").map(compact_agent_use_stale_candidate_layers).unwrap_or_else(|| json!([])),
        "candidate_only_available": rtds.get("candidate_only_available").cloned().unwrap_or(Value::Null),
        "graph_proof_available": rtds.get("graph_proof_available").cloned().unwrap_or(Value::Null),
        "candidate_context_available": rtds.get("candidate_context_available").cloned().unwrap_or(Value::Null),
        "candidate_context_policy": rtds.get("candidate_context_policy").cloned().unwrap_or(Value::Null),
        "startup_auto_index": rtds.get("startup_auto_index").cloned().unwrap_or(Value::Null),
        "dot_codegraph_fallback": rtds.get("dot_codegraph_fallback").cloned().unwrap_or(Value::Null),
        "last_delta_update_summary": rtds.get("last_delta_update_summary").map(compact_agent_use_last_delta_update_summary).unwrap_or(Value::Null),
        "blocked_label_count": rtds.get("blocked_labels").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        "blocked_labels_ref": "errors",
        "retryable_labels": rtds.get("retryable_labels").cloned().unwrap_or_else(|| json!([])),
        "recovery_commands_ref": "recovery_commands",
        "agent_json_compacted": true,
        "public_claim": false,
    });
    if let Some(object) = value.as_object_mut() {
        object.insert("rtds_freshness".to_string(), compact);
    }
    truncated_sections.push("rtds_freshness".to_string());
    *omitted_count = omitted_count.saturating_add(1);
}

pub(crate) fn agent_use_compact_stale_candidate_layers_field(
    value: &mut Value,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    let Some(layers) = value.get("stale_candidate_layers").cloned() else {
        return;
    };
    let compact = compact_agent_use_stale_candidate_layers(&layers);
    if compact == layers {
        return;
    }
    if let Some(object) = value.as_object_mut() {
        object.insert("stale_candidate_layers".to_string(), compact);
    }
    truncated_sections.push("stale_candidate_layers".to_string());
    *omitted_count = omitted_count.saturating_add(1);
}

pub(crate) fn compact_agent_use_stale_candidate_layers(layers: &Value) -> Value {
    let Some(array) = layers.as_array() else {
        return layers.clone();
    };
    let compacted: Vec<Value> = array
        .iter()
        .take(8)
        .map(|layer| {
            let reason = layer
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let reason_summary = reason
                .split(':')
                .next()
                .unwrap_or(reason)
                .chars()
                .take(96)
                .collect::<String>();
            json!({
                "layer": layer.get("layer").cloned().unwrap_or(Value::Null),
                "status": layer.get("status").cloned().unwrap_or(Value::Null),
                "graph_proof": layer.get("graph_proof").cloned().unwrap_or_else(|| json!(false)),
                "reason_summary": reason_summary,
                "path_ref": if layer.get("path").is_some() {
                    json!("profile_sidecar")
                } else {
                    Value::Null
                },
            })
        })
        .collect();
    json!(compacted)
}

pub(crate) fn agent_use_compact_publish_state_field(
    value: &mut Value,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    let Some(publish_state) = value.get("publish_state").cloned() else {
        return;
    };
    let compact = compact_agent_use_publish_state_summary(&publish_state);
    if let Some(object) = value.as_object_mut() {
        object.insert("publish_state".to_string(), compact);
    }
    truncated_sections.push("publish_state".to_string());
    *omitted_count = omitted_count.saturating_add(1);
}

pub(crate) fn compact_agent_use_publish_state_summary(publish_state: &Value) -> Value {
    json!({
        "status": publish_state.get("status").cloned().unwrap_or(Value::Null),
        "active": publish_state.get("active").cloned().unwrap_or(Value::Null),
        "updating": publish_state.get("updating").cloned().unwrap_or(Value::Null),
        "publishing": publish_state.get("publishing").cloned().unwrap_or(Value::Null),
        "visible_db_mutation_claim": publish_state.get("visible_db_mutation_claim").cloned().unwrap_or(Value::Null),
        "temp_db_claimability": publish_state.get("temp_db_claimability").cloned().unwrap_or(Value::Null),
        "updated_unix_ms": publish_state.get("updated_unix_ms").cloned().unwrap_or(Value::Null),
        "agent_json_compacted": true,
    })
}

pub(crate) fn agent_use_compact_lock_state_field(
    value: &mut Value,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    let Some(lock_state) = value.get("lock_state").cloned() else {
        return;
    };
    let compact = json!({
        "active_update": lock_state.get("active_update").cloned().unwrap_or(Value::Null),
        "publish_status": lock_state.get("publish_status").cloned().unwrap_or(Value::Null),
        "db_locked": lock_state.get("db_locked").cloned().unwrap_or(Value::Null),
        "retryable": lock_state.get("retryable").cloned().unwrap_or(Value::Null),
        "retryable_labels": lock_state.get("retryable_labels").cloned().unwrap_or_else(|| json!([])),
        "blocked_label_count": lock_state.get("blocked_labels").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        "lock_retry_count": lock_state.get("lock_retry_count").cloned().unwrap_or(Value::Null),
        "max_concurrent_writers": lock_state.get("max_concurrent_writers").cloned().unwrap_or(Value::Null),
        "writer_queue_serialized": lock_state.get("writer_queue_serialized").cloned().unwrap_or(Value::Null),
        "old_db_preserved": lock_state.get("old_db_preserved").cloned().unwrap_or(Value::Null),
        "temp_db_claimable": lock_state.get("temp_db_claimable").cloned().unwrap_or(Value::Null),
        "unrelated_repo_blocking": lock_state.get("unrelated_repo_blocking").cloned().unwrap_or(Value::Null),
        "no_dot_codegraph_fallback": lock_state.get("no_dot_codegraph_fallback").cloned().unwrap_or(Value::Null),
        "scope": lock_state.get("scope").cloned().unwrap_or(Value::Null),
        "db_ref": "db",
        "profile_root_ref": "profile_root",
        "agent_json_compacted": true,
    });
    if let Some(object) = value.as_object_mut() {
        object.insert("lock_state".to_string(), compact);
    }
    truncated_sections.push("lock_state".to_string());
    *omitted_count = omitted_count.saturating_add(1);
}

pub(crate) fn agent_use_compact_discovery_fields(
    value: &mut Value,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    let env_discovery = value.get("env_discovery").cloned();
    let config_discovery = value.get("config_discovery").cloned();

    if let Some(env) = env_discovery {
        let compact = json!({
            "status": env.get("status").cloned().unwrap_or_else(|| json!("ok")),
            "data_root_source": env.get("data_root_source").cloned().unwrap_or(Value::Null),
            "explicit_data_root": env.get("explicit_data_root").map(compact_agent_use_path_status).unwrap_or(Value::Null),
            "platform_data_dir": env.get("platform_data_dir").map(compact_agent_use_path_status).unwrap_or(Value::Null),
            "db_path_ref": env.get("db_path_ref").cloned().unwrap_or_else(|| json!("db")),
            "profile_root_ref": env.get("profile_root_ref").cloned().unwrap_or_else(|| json!("profile_root")),
            "agent_json_compacted": true,
        });
        if let Some(object) = value.as_object_mut() {
            object.insert("env_discovery".to_string(), compact);
        }
        truncated_sections.push("env_discovery".to_string());
        *omitted_count = omitted_count.saturating_add(1);
    }

    if let Some(config) = config_discovery {
        let compact = json!({
            "status": config.get("status").cloned().unwrap_or(Value::Null),
            "required": config.get("required").cloned().unwrap_or_else(|| json!(false)),
            "diagnostic_only": config.get("diagnostic_only").cloned().unwrap_or_else(|| json!(true)),
            "unknown_field_policy": config.get("unknown_field_policy").cloned().unwrap_or(Value::Null),
            "error_count": config.get("errors").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
            "unknown_field_count": config.get("unknown_fields").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
            "recovery_ref": "recovery_commands",
            "agent_json_compacted": true,
        });
        if let Some(object) = value.as_object_mut() {
            object.insert("config_discovery".to_string(), compact);
        }
        truncated_sections.push("config_discovery".to_string());
        *omitted_count = omitted_count.saturating_add(1);
    }
}

pub(crate) fn compact_agent_use_path_status(value: &Value) -> Value {
    json!({
        "status": value.get("status").cloned().unwrap_or(Value::Null),
        "exists": value.get("exists").cloned().unwrap_or(Value::Null),
        "writable": value.get("writable").cloned().unwrap_or(Value::Null),
        "readable": value.get("readable").cloned().unwrap_or(Value::Null),
        "path_ref": value.get("path_ref").cloned().unwrap_or(Value::Null),
        "agent_json_compacted": true,
    })
}

pub(crate) fn agent_use_compact_read_path_metrics_field(
    value: &mut Value,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    let Some(metrics) = value.get("read_path_metrics").cloned() else {
        return;
    };
    let compact = json!({
        "schema_version": metrics.get("schema_version").cloned().unwrap_or_else(|| json!(1)),
        "surface": metrics.get("surface").cloned().unwrap_or(Value::Null),
        "lookup_strategy": metrics.get("lookup_strategy").cloned().unwrap_or(Value::Null),
        "indexed_lookup_count": metrics.get("indexed_lookup_count").cloned().unwrap_or(Value::Null),
        "symbol_dictionary_lookup_count": metrics.get("symbol_dictionary_lookup_count").cloned().unwrap_or(Value::Null),
        "full_scan_count": metrics.get("full_scan_count").cloned().unwrap_or_else(|| json!(0)),
        "source_file_load_count": metrics.get("source_file_load_count").cloned().unwrap_or_else(|| json!(0)),
        "disk_fallback_used": metrics.get("disk_fallback_used").cloned().unwrap_or_else(|| json!(false)),
        "debug_broad_scan": metrics.get("debug_broad_scan").cloned().unwrap_or_else(|| json!(false)),
        "limits_apply_before_hydration": metrics.get("limits_apply_before_hydration").cloned().unwrap_or_else(|| json!(true)),
        "budget_hit": metrics.get("budget_hit").cloned().unwrap_or_else(|| json!(false)),
        "elapsed_ms": metrics.get("elapsed_ms").cloned().unwrap_or(Value::Null),
        "agent_json_compacted": true,
    });
    if let Some(object) = value.as_object_mut() {
        object.insert("read_path_metrics".to_string(), compact);
    }
    truncated_sections.push("read_path_metrics".to_string());
    *omitted_count = omitted_count.saturating_add(1);
}

pub(crate) fn agent_use_compact_query_results_field(
    value: &mut Value,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    let Some(results) = value.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    let mut changed = false;
    for result in results {
        let Some(object) = result.as_object_mut() else {
            continue;
        };
        if object.contains_key("edge") {
            changed |= object.remove("caller").is_some();
            changed |= object.remove("callee").is_some();
        }
        if object.contains_key("source_span") {
            changed |= object.remove("span").is_some();
        }
    }
    if changed {
        truncated_sections.push("results".to_string());
        *omitted_count = omitted_count.saturating_add(1);
    }
}

pub(crate) fn agent_use_enforce_total_output_budget(
    value: &mut Value,
    max_output_bytes: usize,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    for _ in 0..128 {
        if serialized_json_len(value) <= max_output_bytes {
            return;
        }
        if compact_context_agent_patch_assist_packet_minimal(value) {
            truncated_sections.push("patch_assist_packet".to_string());
            *omitted_count = omitted_count.saturating_add(1);
            continue;
        }
        if compact_context_agent_graph_verification(value) {
            truncated_sections.push("graph_verification".to_string());
            *omitted_count = omitted_count.saturating_add(1);
            continue;
        }
        if pop_context_agent_array_item(value, "snippets") {
            truncated_sections.push("snippets".to_string());
            *omitted_count = omitted_count.saturating_add(1);
            continue;
        }
        if pop_context_agent_array_item(value, "fallback_evidence") {
            truncated_sections.push("fallback_evidence".to_string());
            *omitted_count = omitted_count.saturating_add(1);
            continue;
        }
        if pop_context_agent_array_item(value, "paths") {
            truncated_sections.push("paths".to_string());
            *omitted_count = omitted_count.saturating_add(1);
            continue;
        }
        if pop_context_agent_array_item(value, "candidates") {
            truncated_sections.push("candidates".to_string());
            *omitted_count = omitted_count.saturating_add(1);
            continue;
        }
        let mut removed = false;
        for key in [
            "retrieval_explain",
            "planning_packet",
            "routing_packet",
            "telemetry",
            "timings",
            "instrumentation",
            "candidate_source_counts",
            "candidate_sources",
            "critical_symbols",
            "likely_files",
            "follow_up_queries",
            "recommended_tests",
            "risks",
        ] {
            if key == "instrumentation" && compact_agent_use_instrumentation_marker(value) {
                truncated_sections.push(key.to_string());
                *omitted_count = omitted_count.saturating_add(1);
                removed = true;
                break;
            }
            if agent_use_remove_field(value, key) {
                truncated_sections.push(key.to_string());
                *omitted_count = omitted_count.saturating_add(1);
                removed = true;
                break;
            }
        }
        if !removed {
            return;
        }
    }
}

pub(crate) fn agent_use_enforce_hard_agent_json_budget(
    value: &mut Value,
    max_output_bytes: usize,
    allow_omit_patch_assist: bool,
    truncated_sections: &mut Vec<String>,
    omitted_count: &mut u64,
) {
    let command = value.get("command").and_then(Value::as_str);
    let preserve_update_safety_state = matches!(command, Some("status" | "watch"));
    let preserve_staged_safety_state = matches!(command, Some("status" | "watch" | "context-pack"));
    let preserve_rtds_safety_state = matches!(command, Some("status" | "watch" | "context-pack"));
    let preserve_recovery_commands = matches!(command, Some("status" | "watch" | "context-pack"));
    for key in [
        // Candidate diagnostics are large and not contract-required for compact
        // context-pack/status output, so they go before evidence and safety
        // fields under hard budget pressure.
        "candidate_spool_trace",
        "instrumentation",
        "update_queue_state",
        "lock_state",
        "artifact_hygiene",
        "staged_availability",
        "rtds_freshness",
        "db_lifecycle_read",
        "graph_verification",
        "limits",
        "omitted",
        "candidate_cap",
        "candidate_count",
        "candidate_only_available",
        "candidate_payload_compacted",
        "candidate_spool_query_index_status",
        "candidate_spool_status",
        "candidates",
        "delta_state",
        "dirty_state",
        "evidence_status",
        "graph_db_status",
        "graph_freshness",
        "graph_proof_available",
        "mode",
        "normal_dot_codegraph_created",
        "omitted_by_budget",
        "omitted_by_dedup",
        "proof_failure_reason",
        "proof_path_available",
        "publishing",
        "staged_claimability",
        "task",
        "truncated_sections",
        "vector_audit_status",
        "vector_runtime_status",
        // candidate_spool is a verbose lifecycle blob; the compact
        // candidate_spool_status scalar already conveys readiness, so drop the
        // blob before sacrificing the patch_assist contract section.
        "candidate_spool",
    ] {
        if serialized_json_len(value) <= max_output_bytes {
            return;
        }
        if key == "instrumentation" && compact_agent_use_instrumentation_marker(value) {
            truncated_sections.push(key.to_string());
            *omitted_count = omitted_count.saturating_add(1);
            continue;
        }
        if preserve_update_safety_state
            && matches!(key, "update_queue_state" | "lock_state" | "graph_db_status")
        {
            continue;
        }
        if preserve_staged_safety_state && key == "staged_availability" {
            continue;
        }
        if preserve_rtds_safety_state
            && matches!(
                key,
                "rtds_freshness"
                    | "db_lifecycle_read"
                    | "graph_freshness"
                    | "dirty_state"
                    | "graph_proof_available"
                    | "candidate_spool_status"
                    | "vector_runtime_status"
                    | "vector_audit_status"
                    | "candidate_only_available"
                    | "active_candidate_sources"
            )
        {
            continue;
        }
        if agent_use_remove_field(value, key) {
            truncated_sections.push(key.to_string());
            *omitted_count = omitted_count.saturating_add(1);
        }
    }
    // Reduce patch_assist to its minimal proof stub (keeps first_use_state /
    // graph_proof / bounded candidate_evidence) before any full removal: those
    // scalar fields are the patch-assist contract surface.
    if serialized_json_len(value) > max_output_bytes
        && compact_context_agent_patch_assist_packet_minimal(value)
    {
        truncated_sections.push("patch_assist_packet".to_string());
        *omitted_count = omitted_count.saturating_add(1);
    }
    if allow_omit_patch_assist
        && serialized_json_len(value) > max_output_bytes
        && agent_use_remove_field(value, "patch_assist_packet")
    {
        truncated_sections.push("patch_assist_packet".to_string());
        *omitted_count = omitted_count.saturating_add(1);
    }
    if serialized_json_len(value) > max_output_bytes
        && !preserve_recovery_commands
        && agent_use_remove_field(value, "recovery_commands")
    {
        if let Some(recovery) = value.get_mut("recovery").and_then(Value::as_object_mut) {
            recovery.insert(
                "commands_status".to_string(),
                json!("omitted_by_max_output_bytes"),
            );
        }
        truncated_sections.push("recovery_commands".to_string());
        *omitted_count = omitted_count.saturating_add(1);
    }
}

pub(crate) fn agent_use_finalize_agent_json_budget(
    value: &mut Value,
    detail_mode: AgentUseDetailMode,
    max_output_bytes: usize,
    mut truncated_sections: Vec<String>,
    omitted_count: u64,
) {
    truncated_sections.sort();
    truncated_sections.dedup();
    let truncated_section_count = truncated_sections.len();
    let mut displayed_truncated_sections =
        truncated_sections.into_iter().take(12).collect::<Vec<_>>();
    if truncated_section_count > displayed_truncated_sections.len() {
        displayed_truncated_sections.push("additional_sections_omitted".to_string());
    }
    let output_bytes = serialized_json_len(value);
    let truncated = omitted_count > 0 || output_bytes > max_output_bytes;
    let mode = match detail_mode {
        AgentUseDetailMode::Compact => "compact",
        AgentUseDetailMode::Explain => "explain",
        AgentUseDetailMode::Audit => "audit",
    };
    if let Some(object) = value.as_object_mut() {
        let truncated_sections_json = json!(displayed_truncated_sections);
        object.insert(
            "agent_json_budget".to_string(),
            json!({
                "mode": mode,
                "max_output_bytes": max_output_bytes,
                "output_bytes": output_bytes,
                "truncated": truncated,
                "omitted_count": omitted_count,
                "truncated_section_count": truncated_section_count,
                "truncated_sections_ref": "truncated_sections",
                "max_output_bytes_exceeded": output_bytes > max_output_bytes,
                "required_safety_fields_preserved": true,
            }),
        );
        object.insert("truncated".to_string(), json!(truncated));
        object.insert("truncated_sections".to_string(), truncated_sections_json);
        object.insert("omitted_count".to_string(), json!(omitted_count));
        if let Some(truncation) = object.get_mut("truncation").and_then(Value::as_object_mut) {
            truncation.insert("output_bytes".to_string(), json!(output_bytes));
            truncation.insert("max_output_bytes".to_string(), json!(max_output_bytes));
            truncation.insert("truncated".to_string(), json!(truncated));
            truncation.insert(
                "truncated_section_count".to_string(),
                json!(truncated_section_count),
            );
            truncation.insert(
                "truncated_sections_ref".to_string(),
                json!("truncated_sections"),
            );
            let current_omitted = truncation
                .get("omitted_count")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            truncation.insert(
                "omitted_count".to_string(),
                json!(current_omitted.saturating_add(omitted_count)),
            );
            truncation.insert("required_safety_fields_preserved".to_string(), json!(true));
            truncation.insert(
                "max_output_bytes_exceeded".to_string(),
                json!(output_bytes > max_output_bytes),
            );
        } else {
            object.insert(
                "truncation".to_string(),
                json!({
                    "returned_count": object
                        .get("result_count")
                        .and_then(Value::as_u64)
                        .unwrap_or(1),
                    "limit_applied": truncated,
                    "omitted_count": omitted_count,
                    "total_available_unknown": true,
                    "output_bytes": output_bytes,
                    "max_output_bytes": max_output_bytes,
                    "truncated": truncated,
                    "truncated_section_count": truncated_section_count,
                    "truncated_sections_ref": "truncated_sections",
                    "required_safety_fields_preserved": true,
                    "max_output_bytes_exceeded": output_bytes > max_output_bytes,
                }),
            );
        }
    }
    let final_output_bytes = serialized_json_len(value);
    if let Some(object) = value.as_object_mut() {
        if let Some(budget) = object
            .get_mut("agent_json_budget")
            .and_then(Value::as_object_mut)
        {
            budget.insert("output_bytes".to_string(), json!(final_output_bytes));
            budget.insert(
                "max_output_bytes_exceeded".to_string(),
                json!(final_output_bytes > max_output_bytes),
            );
        }
        if let Some(truncation) = object.get_mut("truncation").and_then(Value::as_object_mut) {
            truncation.insert("output_bytes".to_string(), json!(final_output_bytes));
            truncation.insert(
                "max_output_bytes_exceeded".to_string(),
                json!(final_output_bytes > max_output_bytes),
            );
        }
    }
    let command = value.get("command").and_then(Value::as_str);
    let preserve_recovery_commands = matches!(command, Some("status" | "watch" | "context-pack"));
    let preserve_status_safety_state = matches!(command, Some("status" | "watch"));
    // Small, contract-required context-pack agent-state that the compact
    // context-pack contract expects to survive normal-budget truncation
    // (parallels preserve_recovery_commands and the hard-budget
    // preserve_staged_safety_state gate). Still sheddable under the extreme
    // <=4096 budget tier so user-forced tiny budgets can fit.
    let preserve_context_pack_required_state =
        matches!(command, Some("context-pack")) && max_output_bytes > 4096;
    let mut late_omitted = 0u64;
    if serialized_json_len(value) > max_output_bytes {
        for key in [
            "stale_candidate_layers",
            "stale_evidence",
            "refreshed_evidence",
            "invalidated_evidence",
            "unavailable_evidence",
            "stale_non_proof_reasons",
            "sidecar_statuses",
            "proof_ladder_change_counts",
            "severity_effect",
            "selected_role_coverage",
            "instrumentation",
            "profile_identity",
            "task_profile",
            "task_intent",
            "retrieval_plan_summary",
            "read_path_metrics",
            "env_discovery",
            "config_discovery",
            "active_candidate_sources",
            "rtds_freshness",
            "staged_availability",
            "db_lifecycle_read",
            "lock_state",
            "patch_assist_packet",
            "recovery_commands",
        ] {
            if serialized_json_len(value) <= max_output_bytes {
                break;
            }
            if preserve_recovery_commands && key == "recovery_commands" {
                continue;
            }
            if preserve_context_pack_required_state
                && matches!(
                    key,
                    "staged_availability" | "read_path_metrics" | "sidecar_statuses"
                )
            {
                continue;
            }
            if preserve_status_safety_state && key == "stale_candidate_layers" {
                continue;
            }
            if key == "instrumentation" && compact_agent_use_instrumentation_marker(value) {
                late_omitted = late_omitted.saturating_add(1);
                if let Some(sections) = value
                    .get_mut("truncated_sections")
                    .and_then(Value::as_array_mut)
                {
                    sections.push(json!(key));
                }
                continue;
            }
            if agent_use_remove_field(value, key) {
                late_omitted = late_omitted.saturating_add(1);
                if key == "recovery_commands" {
                    if let Some(recovery) = value.get_mut("recovery").and_then(Value::as_object_mut)
                    {
                        recovery.insert(
                            "commands_status".to_string(),
                            json!("omitted_by_max_output_bytes"),
                        );
                    }
                }
                if let Some(sections) = value
                    .get_mut("truncated_sections")
                    .and_then(Value::as_array_mut)
                {
                    sections.push(json!(key));
                }
            }
        }
    }
    let settled_output_bytes = serialized_json_len(value);
    if let Some(object) = value.as_object_mut() {
        if late_omitted > 0 {
            let total_omitted = object
                .get("omitted_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                .saturating_add(late_omitted);
            object.insert("omitted_count".to_string(), json!(total_omitted));
            if let Some(budget) = object
                .get_mut("agent_json_budget")
                .and_then(Value::as_object_mut)
            {
                budget.insert("omitted_count".to_string(), json!(total_omitted));
            }
            if let Some(truncation) = object.get_mut("truncation").and_then(Value::as_object_mut) {
                let current_omitted = truncation
                    .get("omitted_count")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                truncation.insert(
                    "omitted_count".to_string(),
                    json!(current_omitted.saturating_add(late_omitted)),
                );
            }
        }
        let truncated_section_count = object
            .get("truncated_sections")
            .and_then(Value::as_array)
            .map(|sections| sections.len())
            .unwrap_or_default();
        if let Some(budget) = object
            .get_mut("agent_json_budget")
            .and_then(Value::as_object_mut)
        {
            budget.insert("output_bytes".to_string(), json!(settled_output_bytes));
            budget.insert(
                "max_output_bytes_exceeded".to_string(),
                json!(settled_output_bytes > max_output_bytes),
            );
            budget.insert(
                "truncated_section_count".to_string(),
                json!(truncated_section_count),
            );
        }
        if let Some(truncation) = object.get_mut("truncation").and_then(Value::as_object_mut) {
            truncation.insert("output_bytes".to_string(), json!(settled_output_bytes));
            truncation.insert(
                "max_output_bytes_exceeded".to_string(),
                json!(settled_output_bytes > max_output_bytes),
            );
            truncation.insert(
                "truncated_section_count".to_string(),
                json!(truncated_section_count),
            );
        }
    }
}

fn compact_agent_use_instrumentation_marker(value: &mut Value) -> bool {
    let Some(instrumentation) = value.get("instrumentation").cloned() else {
        return false;
    };
    if instrumentation
        .get("agent_json_compacted")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return false;
    }
    let full_detail_handle = instrumentation
        .get("full_detail_handle")
        .cloned()
        .unwrap_or_else(|| json!("instrumentation:full"));
    let explain_query_plan_omitted = instrumentation
        .get("explain_query_plan_omitted")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| instrumentation.get("explain_query_plan").is_some());
    let compact = json!({
        "agent_json_compacted": true,
        "explain_query_plan_omitted": explain_query_plan_omitted,
        "full_detail_handle": full_detail_handle,
        "expansion_handle": "instrumentation:full",
    });
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    object.insert("instrumentation".to_string(), compact);
    true
}

pub(crate) fn agent_use_remove_field(value: &mut Value, key: &str) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    object.remove(key).is_some()
}
