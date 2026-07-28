//! Context-pack routing and evidence-classification layer.
//!
//! Extracted verbatim from `lib.rs` (F4 module split); behavior unchanged.

use std::collections::{BTreeMap, BTreeSet};

use codegraph_core::ContextPacket;
use serde_json::{json, Value};

use crate::*;

#[derive(Debug, Clone, Copy)]
pub(crate) struct RoutingPacketBudgets {
    critical_files_budget: usize,
    critical_symbols_budget: usize,
    proof_paths_budget: usize,
    text_evidence_budget: usize,
    source_navigation_evidence_budget: usize,
    fallback_snippets_budget: usize,
    follow_up_queries_budget: usize,
    risks_budget: usize,
    validation_steps_budget: usize,
    edit_plan_budget: usize,
    expansion_handles_budget: usize,
    micro_flow_handles_budget: usize,
    artifact_inspection_requirements_budget: usize,
    db_inspection_requirements_budget: usize,
    formulas_or_accounting_notes_budget: usize,
    explain_debug_budget: usize,
}

impl Default for RoutingPacketBudgets {
    fn default() -> Self {
        Self {
            critical_files_budget: 8,
            critical_symbols_budget: 12,
            proof_paths_budget: 3,
            text_evidence_budget: 5,
            source_navigation_evidence_budget: 6,
            fallback_snippets_budget: 3,
            follow_up_queries_budget: 6,
            risks_budget: 8,
            validation_steps_budget: 6,
            edit_plan_budget: 6,
            expansion_handles_budget: 6,
            micro_flow_handles_budget: 4,
            artifact_inspection_requirements_budget: 3,
            db_inspection_requirements_budget: 3,
            formulas_or_accounting_notes_budget: 4,
            explain_debug_budget: 1,
        }
    }
}

impl RoutingPacketBudgets {
    fn to_json(self) -> Value {
        json!({
            "critical_files_budget": self.critical_files_budget,
            "critical_symbols_budget": self.critical_symbols_budget,
            "proof_paths_budget": self.proof_paths_budget,
            "text_evidence_budget": self.text_evidence_budget,
            "source_navigation_evidence_budget": self.source_navigation_evidence_budget,
            "fallback_snippets_budget": self.fallback_snippets_budget,
            "follow_up_queries_budget": self.follow_up_queries_budget,
            "risks_budget": self.risks_budget,
            "validation_steps_budget": self.validation_steps_budget,
            "edit_plan_budget": self.edit_plan_budget,
            "expansion_handles_budget": self.expansion_handles_budget,
            "micro_flow_handles_budget": self.micro_flow_handles_budget,
            "artifact_inspection_requirements_budget": self.artifact_inspection_requirements_budget,
            "db_inspection_requirements_budget": self.db_inspection_requirements_budget,
            "formulas_or_accounting_notes_budget": self.formulas_or_accounting_notes_budget,
            "explain_debug_budget": self.explain_debug_budget,
        })
    }
}

pub(crate) fn context_pack_routing_packet_json(
    options: &ContextPackOptions,
    packet: &ContextPacket,
    db_lifecycle_read: &Value,
    planning_packet: &Value,
    candidate_set: &ContextAgentCandidateSet,
    paths: &[Value],
    fallback_evidence: &[Value],
    language_capability_fallback_evidence: &[Value],
    snippets: &[Value],
    lifecycle_claimable: bool,
    proof_status: &str,
    graph_proof: bool,
    evidence_status: &str,
    upstream_omitted_count: usize,
) -> Value {
    let budgets = RoutingPacketBudgets::default();
    let (task_intent, task_profile, retrieval_plan) = plan_task_retrieval(&packet.task);
    let task_kind = task_intent.task_kind.as_str();
    let ranked_evidence = context_pack_routing_ranked_evidence(
        task_kind,
        &retrieval_plan,
        &candidate_set.candidates,
        paths,
        fallback_evidence,
        snippets,
    );
    let mut language_capability_evidence = ranked_evidence.clone();
    let mut language_capability_evidence_ids = language_capability_evidence
        .iter()
        .map(routing_evidence_id)
        .collect::<BTreeSet<_>>();
    for item in language_capability_fallback_evidence {
        if language_capability_evidence_ids.insert(routing_evidence_id(item)) {
            language_capability_evidence.push(item.clone());
        }
    }
    let (critical_files, omitted_critical_files) =
        routing_critical_files(&ranked_evidence, budgets.critical_files_budget);
    let (critical_symbols, omitted_critical_symbols) =
        routing_critical_symbols(packet, &ranked_evidence, budgets.critical_symbols_budget);
    let verified_paths = routing_verified_paths(
        paths,
        lifecycle_claimable,
        graph_proof,
        budgets.proof_paths_budget,
    );
    let omitted_verified_paths = if graph_proof {
        paths.len().saturating_sub(verified_paths.len())
    } else {
        0
    };
    let (text_evidence, omitted_text_evidence) =
        routing_text_evidence(&ranked_evidence, budgets.text_evidence_budget);
    let (source_navigation_evidence, omitted_source_navigation_evidence) =
        routing_source_navigation_evidence(
            task_kind,
            &ranked_evidence,
            budgets.source_navigation_evidence_budget,
        );
    let (fallback_snippets, omitted_fallback_snippets) =
        routing_fallback_snippets(snippets, &text_evidence, budgets.fallback_snippets_budget);
    let (mut follow_up_queries, omitted_follow_up_queries) = routing_follow_up_queries(
        planning_packet,
        &retrieval_plan,
        budgets.follow_up_queries_budget,
    );
    let artifact_inspection_requirements = routing_artifact_inspection_requirements(
        task_kind,
        packet,
        &ranked_evidence,
        budgets.artifact_inspection_requirements_budget,
    );
    let db_inspection_requirements = routing_db_inspection_requirements(
        task_kind,
        &ranked_evidence,
        budgets.db_inspection_requirements_budget,
    );
    let formulas_or_accounting_notes = routing_formulas_or_accounting_notes(
        task_kind,
        packet,
        budgets.formulas_or_accounting_notes_budget,
    );
    let mut risks = routing_risks(
        task_kind,
        packet,
        &text_evidence,
        &formulas_or_accounting_notes,
        budgets.risks_budget,
    );
    let staged_availability = staged_availability_for_packet(packet, db_lifecycle_read);
    if let Some(staged_risks) = staged_availability.get("risks").and_then(Value::as_array) {
        let mut risk_ids = risks
            .iter()
            .filter_map(|risk| risk.get("risk_id").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        for risk in staged_risks {
            let risk_id = risk.get("risk_id").and_then(Value::as_str).unwrap_or("");
            if !risk_id.is_empty() && risk_ids.insert(risk_id.to_string()) {
                risks.push(risk.clone());
            }
            if risks.len() >= budgets.risks_budget {
                break;
            }
        }
    }
    risks.truncate(budgets.risks_budget);
    let mut unknowns = routing_unknowns(
        task_kind,
        graph_proof,
        evidence_status,
        &artifact_inspection_requirements,
        &db_inspection_requirements,
    );
    let all_micro_flow_handles = packet
        .metadata
        .get("micro_flow_handles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let micro_flow_handles = all_micro_flow_handles
        .iter()
        .take(budgets.micro_flow_handles_budget)
        .cloned()
        .collect::<Vec<_>>();
    let omitted_micro_flow_handles = all_micro_flow_handles
        .len()
        .saturating_sub(micro_flow_handles.len());
    let micro_flow_packet_summary = packet
        .metadata
        .get("micro_flow_packet_summary")
        .cloned()
        .unwrap_or_else(|| context_pack_micro_flow_handle_summary_json(&[]));
    let language_capability_plan = routing_language_capability_plan(
        task_kind,
        &language_capability_evidence,
        &micro_flow_packet_summary,
        graph_proof,
    );
    unknowns.extend(routing_language_boundary_unknowns(
        &language_capability_plan,
    ));
    routing_annotate_follow_up_queries(&mut follow_up_queries, &language_capability_plan);
    let mut validation_steps = routing_validation_steps(
        task_kind,
        &critical_files,
        &ranked_evidence,
        &artifact_inspection_requirements,
        &formulas_or_accounting_notes,
        budgets.validation_steps_budget,
    );
    routing_annotate_validation_steps(&mut validation_steps, &language_capability_plan);
    let language_validation_steps = routing_language_validation_steps(
        &language_capability_plan,
        budgets.validation_steps_budget,
    );
    let edit_plan = routing_edit_plan(
        task_kind,
        &critical_files,
        &ranked_evidence,
        &artifact_inspection_requirements,
        &formulas_or_accounting_notes,
        budgets.edit_plan_budget,
    );
    let expansion_handles =
        routing_expansion_handles(&ranked_evidence, budgets.expansion_handles_budget);
    let implementation_trace_handles = matches!(
        task_kind,
        "implementation_trace"
            | "persistence_path_trace"
            | "indexing_summary_trace"
            | "storage_accounting_trace"
            | "artifact_math_trace"
            | "benchmark_metric_trace"
    )
    .then(|| micro_flow_handles.clone())
    .unwrap_or_default();
    let agent_investigation_layer = routing_agent_investigation_layer_json(
        task_kind,
        &micro_flow_handles,
        &micro_flow_packet_summary,
        &language_capability_plan,
        graph_proof,
    );
    let retrieval_plan_summary =
        routing_retrieval_plan_summary(&retrieval_plan, &ranked_evidence, budgets);
    let claimability = routing_claimability_json(
        lifecycle_claimable,
        db_lifecycle_read,
        proof_status,
        graph_proof,
        evidence_status,
        !text_evidence.is_empty(),
    );
    let task_roles = retrieval_plan
        .query_atoms
        .iter()
        .map(|atom| atom.role.clone())
        .collect::<Vec<_>>();
    let deterministic_sentences = routing_deterministic_sentences(
        task_kind,
        &task_intent.to_json(),
        graph_proof,
        verified_paths.len(),
        &text_evidence,
        &unknowns,
        &risks,
        &critical_files,
        &artifact_inspection_requirements,
        &db_inspection_requirements,
        &formulas_or_accounting_notes,
        db_lifecycle_read,
    );
    let section_omitted_by_budget = BTreeMap::from([
        ("critical_files", omitted_critical_files),
        ("critical_symbols", omitted_critical_symbols),
        ("verified_paths", omitted_verified_paths),
        ("text_evidence", omitted_text_evidence),
        (
            "source_navigation_evidence",
            omitted_source_navigation_evidence,
        ),
        ("fallback_snippets", omitted_fallback_snippets),
        ("follow_up_queries", omitted_follow_up_queries),
        ("micro_flow_handles", omitted_micro_flow_handles),
    ]);
    let omitted_by_budget = upstream_omitted_count
        + section_omitted_by_budget.values().copied().sum::<usize>()
        + candidate_set.omitted_count;
    let omitted_by_dedup = packet
        .metadata
        .get("omitted_by_dedup")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let budget_status = json!({
        "status": if omitted_by_budget == 0 { "within_budget" } else { "bounded_with_omissions" },
        "budgets": budgets.to_json(),
        "separate_budgets": true,
        "explain_debug_budget_separate": true,
        "fallback_snippets_survive_compaction": true,
        "source_navigation_evidence_survives_for_implementation_trace": true,
        "omitted_by_budget": omitted_by_budget,
        "omitted_by_dedup": omitted_by_dedup,
        "candidate_omitted_by_candidate_cap": candidate_set.omitted_count,
        "section_omitted_by_budget": section_omitted_by_budget,
        "max_output_bytes": options.max_output_bytes.unwrap_or(DEFAULT_CONTEXT_AGENT_MAX_OUTPUT_BYTES),
    });

    let mut value = json!({
        "packet_kind": "agent_routing_packet",
        "schema_version": 1,
        "task_intent": task_intent.to_json(),
        "task_profile": {
            "profile_id": task_profile.profile_id,
            "profile_name": task_profile.profile_name,
            "graph_expectation": task_profile.graph_expectation,
            "fallback_policy": task_profile.fallback_policy,
            "role_budget": task_profile.role_budget,
        },
        "task_roles": task_roles,
        "retrieval_plan_summary": retrieval_plan_summary,
        "available_layers": staged_availability.get("available_layers").cloned().unwrap_or_else(|| json!([])),
        "missing_layers": staged_availability.get("missing_layers").cloned().unwrap_or_else(|| json!([])),
        "layer_readiness": staged_availability.get("layer_readiness").cloned().unwrap_or(Value::Null),
        "candidate_context_available": staged_availability.get("candidate_context_available").cloned().unwrap_or_else(|| json!(false)),
        "graph_proof_available": staged_availability.get("graph_proof_available").cloned().unwrap_or_else(|| json!(graph_proof)),
        "recommended_next_step": staged_availability.get("recommended_next_step").cloned().unwrap_or_else(|| json!(if graph_proof { "inspect candidate spans" } else { "run final graph verification" })),
        "claimability": claimability,
        "critical_files": critical_files,
        "critical_symbols": critical_symbols,
        "verified_paths": verified_paths,
        "text_evidence": text_evidence,
        "source_navigation_evidence": source_navigation_evidence,
        "fallback_snippets": fallback_snippets,
        "micro_flow_handles": micro_flow_handles,
        "micro_flow_packet_summary": micro_flow_packet_summary,
        "implementation_trace_micro_flow_handles": implementation_trace_handles,
        "agent_investigation_layer": agent_investigation_layer,
        "language_capability_plan": language_capability_plan,
        "micro_flow_handle_policy": {
            "packet_handles_do_not_create_proof": true,
            "compact_default_full_packet_body_inline": false,
            "compact_default_ordered_steps_inline": false,
            "context_entry_command_activated": false,
            "route_bridge_pull_forward_count": 0
        },
        "unknowns": unknowns,
        "risks": risks,
        "validation_steps": validation_steps,
        "language_validation_steps": language_validation_steps,
        "validation_steps_language_aware": true,
        "follow_up_queries": follow_up_queries,
        "expansion_command_available": false,
        "expansion_handles": expansion_handles,
        "edit_plan": edit_plan,
        "artifact_inspection_requirements": artifact_inspection_requirements,
        "db_inspection_requirements": db_inspection_requirements,
        "formulas_or_accounting_notes": formulas_or_accounting_notes,
        "omitted_count": omitted_by_budget + omitted_by_dedup,
        "budget_status": budget_status,
        "deterministic_summary": deterministic_sentences,
        "explain_summary": {
            "candidate_total_count": candidate_set.total_count,
            "candidate_returned_count": candidate_set.candidates.len(),
            "candidate_sources": context_pack_candidate_sources_json(&candidate_set.candidates),
            "omitted_candidates_preview": candidate_set.omitted_candidates.iter().take(budgets.explain_debug_budget).cloned().collect::<Vec<_>>(),
            "proof_contract": "only verified graph/source evidence can set graph_proof=true; text, vector, binary, nuance, and follow-up query evidence remain candidate or source-text evidence"
        }
    });
    add_agent_use_dirty_evidence_output_fields(&mut value, "agent-routing-packet", options.explain);
    value
}

pub(crate) fn context_pack_routing_ranked_evidence(
    task_kind: &str,
    retrieval_plan: &codegraph_query::RetrievalPlan,
    candidates: &[Value],
    paths: &[Value],
    fallback_evidence: &[Value],
    snippets: &[Value],
) -> Vec<Value> {
    let mut items = Vec::new();
    items.extend(candidates.iter().filter_map(|candidate| {
        routing_evidence_from_candidate(task_kind, retrieval_plan, candidate)
    }));
    items.extend(
        paths
            .iter()
            .filter_map(|path| routing_evidence_from_proof_path(task_kind, retrieval_plan, path)),
    );
    items.extend(fallback_evidence.iter().filter_map(|evidence| {
        routing_evidence_from_fallback(task_kind, retrieval_plan, evidence)
    }));
    items.extend(
        snippets.iter().filter_map(|snippet| {
            routing_evidence_from_snippet(task_kind, retrieval_plan, snippet)
        }),
    );

    let mut deduped = BTreeMap::<String, Value>::new();
    for mut item in items {
        let key = routing_evidence_dedup_key(&item);
        if let Some(existing) = deduped.get_mut(&key) {
            routing_merge_evidence(existing, &mut item);
        } else {
            deduped.insert(key, item);
        }
    }
    let mut ranked = deduped.into_values().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        routing_evidence_rank_score(task_kind, right)
            .cmp(&routing_evidence_rank_score(task_kind, left))
            .then_with(|| routing_evidence_id(left).cmp(&routing_evidence_id(right)))
    });

    let role_order = retrieval_plan
        .query_atoms
        .iter()
        .map(|atom| atom.role.as_str())
        .collect::<Vec<_>>();
    let mut selected = Vec::new();
    let mut selected_keys = BTreeSet::new();
    for role in role_order {
        if let Some(index) = ranked.iter().position(|item| {
            item.get("role").and_then(Value::as_str) == Some(role)
                && !selected_keys.contains(&routing_evidence_dedup_key(item))
        }) {
            let item = ranked[index].clone();
            selected_keys.insert(routing_evidence_dedup_key(&item));
            selected.push(item);
        }
    }
    for item in ranked {
        let key = routing_evidence_dedup_key(&item);
        if selected_keys.insert(key) {
            selected.push(item);
        }
    }
    for (index, item) in selected.iter_mut().enumerate() {
        if let Some(object) = item.as_object_mut() {
            object.insert("rank".to_string(), json!(index + 1));
        }
    }
    selected
}

pub(crate) fn routing_evidence_from_candidate(
    task_kind: &str,
    retrieval_plan: &codegraph_query::RetrievalPlan,
    candidate: &Value,
) -> Option<Value> {
    let candidate = context_pack_public_candidate_json(candidate);
    let file = routing_candidate_file(&candidate)?;
    let role = routing_role_for_value(task_kind, retrieval_plan, &candidate, &file);
    let evidence_id = candidate
        .get("candidate_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("candidate://{}", context_agent_stable_component(&file)));
    let graph_proof = candidate
        .get("graph_proof")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let proof_status = candidate
        .get("proof_status")
        .and_then(Value::as_str)
        .unwrap_or(if graph_proof {
            "proof_path_found"
        } else {
            "candidate_only"
        });
    let matched_signal = routing_matched_signal(&candidate);
    let mut evidence = json!({
        "evidence_id": evidence_id,
        "role": role,
        "file": file,
        "span": candidate.get("span").cloned().unwrap_or_else(|| candidate.get("source_span").cloned().unwrap_or(Value::Null)),
        "candidate_sources": candidate.get("candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "evidence_role": candidate.get("evidence_role").cloned().unwrap_or_else(|| json!("unknown")),
        "proof_status": proof_status,
        "graph_proof": graph_proof,
        "claimability": routing_candidate_claimability(&candidate),
        "matched_signal": matched_signal,
        "reason": candidate.get("reason").cloned().unwrap_or_else(|| json!("retrieval candidate")),
        "rank_score": routing_evidence_rank_score(task_kind, &candidate),
        "source_navigation_is_graph_proof": false,
        "candidate": candidate,
    });
    if let Some(object) = evidence.as_object_mut() {
        if let Some(candidate) = object.get("candidate").cloned() {
            if let Some(capability) = candidate.get("language_capability").cloned() {
                object.insert("language_capability".to_string(), capability);
            }
            copy_context_candidate_degradation_fields(&candidate, object);
        }
    }
    Some(evidence)
}

pub(crate) fn routing_evidence_from_proof_path(
    task_kind: &str,
    retrieval_plan: &codegraph_query::RetrievalPlan,
    path: &Value,
) -> Option<Value> {
    let source_spans = path.get("source_spans").and_then(Value::as_array)?;
    let first_span = source_spans.first()?;
    let file = first_span
        .get("file")
        .and_then(Value::as_str)
        .or_else(|| first_span.get("repo_relative_path").and_then(Value::as_str))?
        .to_string();
    let role = routing_role_for_value(task_kind, retrieval_plan, path, &file);
    let evidence_id = path
        .get("path_id")
        .and_then(Value::as_str)
        .map(|id| format!("proof-path://{id}"))
        .unwrap_or_else(|| format!("proof-path://{}", context_agent_stable_component(&file)));
    Some(json!({
        "evidence_id": evidence_id,
        "role": role,
        "file": file,
        "span": first_span,
        "candidate_sources": ["path_evidence", "graph_neighbor"],
        "evidence_role": path.get("evidence_role").cloned().unwrap_or_else(|| json!("unknown")),
        "proof_status": "proof_path_found",
        "graph_proof": true,
        "claimability": {
            "claimable_as": ["graph_relation_proof", "source_text_existence"],
            "not_claimable_as": []
        },
        "matched_signal": path.get("classification_reason").and_then(Value::as_str).unwrap_or("verified graph path"),
        "reason": path.get("classification_reason").cloned().unwrap_or_else(|| json!("verified graph path")),
        "language_capability": path.get("language_capability").cloned().unwrap_or(Value::Null),
        "source_navigation_is_graph_proof": false,
        "path": path,
    }))
}

pub(crate) fn routing_evidence_from_fallback(
    task_kind: &str,
    retrieval_plan: &codegraph_query::RetrievalPlan,
    evidence: &Value,
) -> Option<Value> {
    let file = context_planning_value_string(evidence, "file").or_else(|| {
        evidence
            .get("source_span")
            .and_then(|span| context_planning_value_string(span, "file"))
    })?;
    let role = routing_role_for_value(task_kind, retrieval_plan, evidence, &file);
    let evidence_id = context_planning_value_string(evidence, "id")
        .unwrap_or_else(|| format!("text-evidence://{}", context_agent_stable_component(&file)));
    let proof_status = evidence
        .get("proof_status")
        .and_then(Value::as_str)
        .unwrap_or("no_proof_path_found");
    let evidence_role = evidence
        .get("evidence_role")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let fallback_source = evidence
        .get("fallback_source")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let candidate_sources = if evidence_role == "text_evidence" {
        json!(["text_evidence", "lexical_fts"])
    } else if fallback_source.contains("retrieval_plan_atom/source_navigation") {
        json!(["source_navigation", "retrieval_plan_atom"])
    } else {
        json!(["source_navigation", "fallback"])
    };
    let mut value = json!({
        "evidence_id": evidence_id,
        "role": role,
        "file": file,
        "span": evidence.get("source_span").cloned().unwrap_or_else(|| evidence.get("span").cloned().unwrap_or(Value::Null)),
        "candidate_sources": candidate_sources,
        "evidence_role": evidence.get("evidence_role").cloned().unwrap_or_else(|| json!("text_evidence")),
        "proof_status": proof_status,
        "graph_proof": false,
        "claimability": {
            "claimable_as": ["source_text_existence"],
            "not_claimable_as": ["graph_relation_proof"]
        },
        "matched_signal": evidence.get("classification_reason").and_then(Value::as_str).unwrap_or("source text fallback"),
        "reason": evidence.get("classification_reason").cloned().unwrap_or_else(|| json!("source text fallback")),
        "language_capability": evidence.get("language_capability").cloned().unwrap_or(Value::Null),
        "source_navigation_is_graph_proof": false,
        "fallback": evidence,
    });
    if let Some(object) = value.as_object_mut() {
        copy_context_candidate_degradation_fields(evidence, object);
    }
    Some(value)
}

pub(crate) fn routing_evidence_from_snippet(
    task_kind: &str,
    retrieval_plan: &codegraph_query::RetrievalPlan,
    snippet: &Value,
) -> Option<Value> {
    let file = context_planning_value_string(snippet, "file")?;
    let role = routing_role_for_value(task_kind, retrieval_plan, snippet, &file);
    let evidence_id = format!(
        "snippet://{}:{}",
        context_agent_stable_component(&file),
        snippet
            .get("lines")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    let mut value = json!({
        "evidence_id": evidence_id,
        "role": role,
        "file": file,
        "span": snippet.get("source_span").cloned().unwrap_or_else(|| snippet.get("span").cloned().unwrap_or(Value::Null)),
        "candidate_sources": ["fallback_snippet"],
        "evidence_role": snippet.get("evidence_role").cloned().unwrap_or_else(|| json!("unknown")),
        "proof_status": snippet.get("proof_status").and_then(Value::as_str).unwrap_or("not_graph_proof"),
        "graph_proof": false,
        "claimability": {
            "claimable_as": ["source_text_existence"],
            "not_claimable_as": ["graph_relation_proof"]
        },
        "matched_signal": snippet.get("reason").and_then(Value::as_str).unwrap_or("fallback snippet"),
        "reason": snippet.get("reason").cloned().unwrap_or_else(|| json!("fallback snippet")),
        "language_capability": snippet.get("language_capability").cloned().unwrap_or(Value::Null),
        "source_navigation_is_graph_proof": false,
        "snippet": snippet,
    });
    if let Some(object) = value.as_object_mut() {
        copy_context_candidate_degradation_fields(snippet, object);
    }
    Some(value)
}

pub(crate) fn routing_language_capability_for_item(item: &Value, evidence_kind: &str) -> Value {
    if let Some(capability) = item
        .get("language_capability")
        .filter(|capability| capability.is_object())
    {
        return capability.clone();
    }
    let file = item.get("file").and_then(Value::as_str);
    let graph_proof = item
        .get("graph_proof")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let proof_status = item
        .get("proof_status")
        .and_then(Value::as_str)
        .unwrap_or(if graph_proof {
            "proof_path_found"
        } else {
            "not_graph_proof"
        });
    let source_role = item
        .get("evidence_role")
        .and_then(Value::as_str)
        .or_else(|| item.get("source_role").and_then(Value::as_str))
        .unwrap_or(if evidence_kind == "text_evidence" {
            "text_evidence"
        } else {
            "unknown"
        });
    let exactness = if graph_proof {
        "exact"
    } else if evidence_kind == "text_evidence" {
        "textual_exact_match"
    } else {
        "source_navigation"
    };
    context_pack_source_language_capability_json(
        file,
        evidence_kind,
        proof_status,
        graph_proof,
        exactness,
        source_role,
    )
}

pub(crate) fn routing_compact_language_capability_for_item(
    item: &Value,
    evidence_kind: &str,
) -> Value {
    let capability = routing_language_capability_for_item(item, evidence_kind);
    json!({
        "language": capability.get("language").cloned().unwrap_or_else(|| json!("unknown")),
        "frontend": capability.get("frontend").cloned().unwrap_or_else(|| json!("unknown")),
        "source_role": capability.get("source_role").cloned().unwrap_or_else(|| json!("unknown")),
        "capability_status": capability.get("capability_status").cloned().unwrap_or_else(|| json!("unknown")),
        "exactness": capability.get("exactness").cloned().unwrap_or_else(|| json!("unknown")),
        "resolver_status": capability.get("resolver_status").cloned().unwrap_or_else(|| json!("unknown")),
        "proof_strength": capability.get("proof_strength").cloned().unwrap_or_else(|| json!("unknown")),
        "not_graph_proof": capability.get("not_graph_proof").cloned().unwrap_or_else(|| json!(true)),
    })
}

pub(crate) fn routing_language_capability_plan(
    task_kind: &str,
    evidence: &[Value],
    micro_flow_packet_summary: &Value,
    graph_proof: bool,
) -> Value {
    let mut languages = BTreeMap::<String, usize>::new();
    let mut frontends = BTreeSet::<String>::new();
    let mut source_roles = BTreeMap::<String, usize>::new();
    let mut capability_status_values = BTreeSet::<String>::new();
    let mut exactness_values = BTreeSet::<String>::new();
    let mut resolver_status_values = BTreeSet::<String>::new();
    let mut proof_strength_values = BTreeSet::<String>::new();
    let mut evidence_rows = Vec::new();
    let mut exact_or_parser_rows = 0usize;
    let mut unsupported_or_unknown_rows = 0usize;
    let mut resolver_claim_rows = 0usize;
    let mut resolver_claims_missing_provenance = 0usize;

    for item in evidence {
        let capability = routing_language_capability_for_item(item, "source_navigation_evidence");
        let language = routing_capability_string(&capability, "language", "unknown");
        let frontend = routing_capability_string(&capability, "frontend", language.as_str());
        let source_role = routing_capability_string(&capability, "source_role", "unknown");
        let status = routing_capability_string(&capability, "capability_status", "unknown");
        let exactness = routing_capability_string(&capability, "exactness", "unknown");
        let resolver_status = routing_capability_string(&capability, "resolver_status", "unknown");
        let proof_strength = routing_capability_string(&capability, "proof_strength", "unknown");
        *languages.entry(language.clone()).or_insert(0) += 1;
        frontends.insert(frontend.clone());
        *source_roles.entry(source_role.clone()).or_insert(0) += 1;
        capability_status_values.insert(status.clone());
        exactness_values.insert(exactness.clone());
        resolver_status_values.insert(resolver_status.clone());
        proof_strength_values.insert(proof_strength.clone());
        if matches!(
            exactness.as_str(),
            "exact" | "parser_verified" | "textual_exact_match" | "source_navigation"
        ) {
            exact_or_parser_rows += 1;
        }
        if routing_status_is_unknown_or_unsupported(&status)
            || capability.get("unknown_boundary_reason").is_some()
            || capability.get("unsupported_reason").is_some()
        {
            unsupported_or_unknown_rows += 1;
        }
        if matches!(
            exactness.as_str(),
            "compiler_verified" | "lsp_verified" | "resolver_verified"
        ) || routing_resolver_status_claims_exactness(&resolver_status)
        {
            resolver_claim_rows += 1;
            if capability
                .get("provenance_ref")
                .and_then(Value::as_str)
                .is_none()
            {
                resolver_claims_missing_provenance += 1;
            }
        }
        if evidence_rows.len() < 8 {
            evidence_rows.push(json!({
                "evidence_id": routing_evidence_id(item),
                "file": item.get("file").cloned().unwrap_or(Value::Null),
                "language": language,
                "frontend": frontend,
                "source_role": source_role,
                "capability_status": status,
                "exactness": exactness,
                "resolver_status": resolver_status,
                "proof_strength": proof_strength,
                "graph_proof": item.get("graph_proof").and_then(Value::as_bool).unwrap_or(false),
                "not_graph_proof": !item.get("graph_proof").and_then(Value::as_bool).unwrap_or(false),
            }));
        }
    }

    let language_names = languages.keys().cloned().collect::<Vec<_>>();
    let packet_language_registry = mvp4_local_flow_packet_language_registry_json();
    let active_packet_languages = packet_language_registry["active_packet_languages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let active_non_typescript_packet_languages = packet_language_registry
        ["active_non_typescript_packet_languages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let inactive_packet_languages = language_names
        .iter()
        .filter(|language| {
            language.as_str() != "unknown"
                && !active_packet_languages
                    .iter()
                    .any(|active| active.eq_ignore_ascii_case(language))
        })
        .cloned()
        .collect::<Vec<_>>();
    let unknown_dynamic_risks = routing_language_dynamic_risks(task_kind, evidence);
    let unsupported_relations =
        routing_language_unsupported_relations(&inactive_packet_languages, &unknown_dynamic_risks);

    json!({
        "status": if evidence.is_empty() { "no_language_scoped_evidence" } else { "language_capability_aware" },
        "task_kind": task_kind,
        "usable_for": [
            "plan_change",
            "explain_behavior",
            "verify_claim",
            "trace_boundary",
            "validate_change"
        ],
        "languages": language_names,
        "frontends": frontends.into_iter().collect::<Vec<_>>(),
        "evidence_rows": evidence_rows,
        "evidence_language_counts": languages,
        "source_role_counts": source_roles,
        "capability_status_values": capability_status_values.into_iter().collect::<Vec<_>>(),
        "exactness_values": exactness_values.into_iter().collect::<Vec<_>>(),
        "resolver_status_values": resolver_status_values.into_iter().collect::<Vec<_>>(),
        "proof_strength_values": proof_strength_values.into_iter().collect::<Vec<_>>(),
        "exact_partial_unsupported_status": {
            "exact_or_parser_source_rows": exact_or_parser_rows,
            "unsupported_or_unknown_rows": unsupported_or_unknown_rows,
            "graph_proof_available": graph_proof,
            "parser_or_source_facts_are_not_caller_callee_proof": true
        },
        "resolver_compiler_availability": {
            "resolver_claim_rows": resolver_claim_rows,
            "resolver_claims_missing_provenance": resolver_claims_missing_provenance,
            "semantic_exactness_requires_recorded_provenance": true,
            "compiler_or_lsp_unavailable_is_unknown_not_blocker": true
        },
        "unknown_dynamic_risks": unknown_dynamic_risks,
        "unsupported_relations": unsupported_relations,
        "local_flow_packet_boundary": {
            "active_languages": active_packet_languages,
            "active_packet_languages": active_packet_languages,
            "active_packet_language_count": packet_language_registry["active_packet_language_count"],
            "active_non_typescript_packet_languages": active_non_typescript_packet_languages,
            "active_non_typescript_packet_language_count": packet_language_registry["active_non_typescript_packet_language_count"],
            "default_packet_query_language": packet_language_registry["default_packet_query_language"],
            "packet_language_registry": packet_language_registry,
            "micro_flow_packet_summary": micro_flow_packet_summary,
            "inactive_packet_languages_seen": inactive_packet_languages,
            "inactive_packet_language_count": inactive_packet_languages.len(),
            "inactive_packet_support": MVP4_3_LOCAL_FLOW_PACKET_INACTIVE_SUPPORT,
            "inactive_packet_overclaim_count": 0,
            "active_non_typescript_packet_support": MVP4_3_LOCAL_FLOW_PACKET_REGISTRY_SCOPED_SUPPORT,
            "active_non_typescript_packet_overclaim_count": 0,
            "non_typescript_packet_languages_seen": active_non_typescript_packet_languages,
            "non_typescript_packet_language_count": packet_language_registry["active_non_typescript_packet_language_count"],
            "non_typescript_packet_support": MVP4_3_LOCAL_FLOW_PACKET_REGISTRY_SCOPED_SUPPORT,
            "non_typescript_packet_overclaim_count": 0,
            "compatibility_aliases": {
                "non_typescript_packet_languages_seen": {
                    "deprecated": true,
                    "alias_of": "active_non_typescript_packet_languages"
                },
                "non_typescript_packet_language_count": {
                    "deprecated": true,
                    "alias_of": "active_non_typescript_packet_language_count"
                },
                "non_typescript_packet_support": {
                    "deprecated": true,
                    "alias_of": "active_non_typescript_packet_support"
                },
                "non_typescript_packet_overclaim_count": {
                    "deprecated": true,
                    "alias_of": "active_non_typescript_packet_overclaim_count"
                }
            },
            "typescript_packet_handles_preserved": packet_language_registry["typescript_packet_handles_preserved"],
            "packet_handles_do_not_create_proof": true,
            "flow_proof_not_emitted_for_unsupported_languages": true,
            "context_entry_command_activated": false
        },
        "proof_boundary": {
            "capability_metadata_does_not_create_graph_proof": true,
            "candidate_or_text_evidence_not_graph_proof": true,
            "unsupported_relations_are_not_blockers": true,
            "route_bridge_context_entry_activated": false,
            "mutation_proof_activated": false
        }
    })
}

fn routing_capability_string(capability: &Value, key: &str, default: &str) -> String {
    capability
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(default)
        .to_string()
}

fn routing_status_is_unknown_or_unsupported(status: &str) -> bool {
    matches!(
        status,
        "unknown"
            | "unsupported"
            | "not_implemented"
            | "runtime_required"
            | "compiler_required"
            | "lsp_required"
            | "macro_required"
            | "preprocessor_required"
            | "requires_runtime"
            | "requires_compiler"
            | "requires_lsp"
            | "requires_macro_expansion"
            | "requires_preprocessor"
    )
}

fn routing_resolver_status_claims_exactness(status: &str) -> bool {
    matches!(
        status,
        "compiler_verified"
            | "lsp_verified"
            | "resolver_verified"
            | "resolved"
            | "resolved_with_provenance"
            | "implemented_with_provenance"
            | "available_with_provenance"
    )
}

fn routing_language_dynamic_risks(task_kind: &str, evidence: &[Value]) -> Vec<Value> {
    let mut risks = BTreeMap::<String, Value>::new();
    for item in evidence {
        let capability = routing_language_capability_for_item(item, "source_navigation_evidence");
        let language = routing_capability_string(&capability, "language", "unknown");
        let source_role = routing_capability_string(&capability, "source_role", "unknown");
        let text = routing_value_text(item).to_ascii_lowercase();
        let file = item
            .get("file")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let evidence_id = routing_evidence_id(item);
        let mut add_risk = |risk_id: &str, reason: &str, boundary: &str| {
            risks.entry(risk_id.to_string()).or_insert_with(|| {
                json!({
                    "risk_id": risk_id,
                    "language": language.clone(),
                    "source_role": source_role.clone(),
                    "file": file,
                    "evidence_ids": [evidence_id.clone()],
                    "reason": reason,
                    "boundary": boundary,
                    "proof_status": "unknown",
                    "blocking": false,
                    "not_graph_proof": true
                })
            });
        };
        match language.as_str() {
            "typescript" | "tsx" => {
                if text.contains("decorator")
                    || text.contains("dynamic import")
                    || text.contains("computed")
                    || text.contains("react")
                    || text.contains("jsx")
                    || text.contains("props")
                {
                    add_risk(
                        "ts_tsx_framework_or_dynamic_runtime_unknown",
                        "decorator, JSX/component, computed, or dynamic TypeScript behavior is runtime/framework-sensitive unless resolver evidence proves it",
                        "framework_heuristic_or_runtime_unknown",
                    );
                }
            }
            "javascript" | "jsx" => {
                if text.contains("dynamic import")
                    || text.contains("computed")
                    || text.contains("prototype")
                    || text.contains("monkeypatch")
                    || text.contains("react")
                    || text.contains("event handler")
                {
                    add_risk(
                        "js_jsx_dynamic_framework_unknown",
                        "JavaScript dynamic import, computed property, prototype mutation, or framework callback behavior is not graph proof",
                        "runtime_unknown_or_framework_heuristic",
                    );
                }
            }
            "python" => {
                if text.contains("importlib")
                    || text.contains("__import__")
                    || text.contains("getattr")
                    || text.contains("setattr")
                    || text.contains("monkeypatch")
                {
                    add_risk(
                        "python_dynamic_runtime_unknown",
                        "Python importlib/getattr/setattr/monkeypatch behavior is runtime-dynamic unless separately modeled",
                        "runtime_unknown",
                    );
                }
            }
            "go" => {
                if text.contains("interface")
                    || text.contains("build tag")
                    || text.contains("go:build")
                    || text.contains("goroutine")
                    || text.contains("channel")
                {
                    add_risk(
                        "go_interface_build_tag_unknown",
                        "Go interface dispatch, build tags, goroutine, and channel behavior require compiler/runtime-aware handling before exact targets are claimable",
                        "compiler_required_or_runtime_unknown",
                    );
                }
            }
            "rust" => {
                if text.contains("macro")
                    || text.contains("cfg")
                    || text.contains("feature")
                    || text.contains("unsafe")
                    || text.contains("trait")
                {
                    add_risk(
                        "rust_macro_cfg_trait_unknown",
                        "Rust macro expansion, cfg/feature gating, unsafe, and trait target behavior are unknown without exact compiler/resolver evidence",
                        "macro_required_or_compiler_required",
                    );
                }
            }
            "java" => {
                if text.contains("reflection")
                    || text.contains("class.forname")
                    || text.contains("getmethod")
                    || text.contains("annotation")
                    || text.contains("di")
                {
                    add_risk(
                        "java_reflection_annotation_runtime_unknown",
                        "Java reflection, annotation processors, and DI are runtime/compiler-generated boundaries unless explicitly modeled",
                        "runtime_unknown_or_compiler_required",
                    );
                }
            }
            "c" => {
                if text.contains("#define")
                    || text.contains("#if")
                    || text.contains("macro")
                    || text.contains("function pointer")
                    || text.contains("preprocessor")
                {
                    add_risk(
                        "c_macro_preprocessor_unknown",
                        "C macros, inactive branches, and function pointers are preprocessor/runtime target boundaries",
                        "preprocessor_required_or_macro_required",
                    );
                }
            }
            "cpp" => {
                if text.contains("template")
                    || text.contains("macro")
                    || text.contains("operator")
                    || text.contains("virtual")
                    || text.contains("function pointer")
                {
                    add_risk(
                        "cpp_template_macro_dispatch_unknown",
                        "C++ templates, macros, overloads/ADL, virtual dispatch, and function pointers require compiler proof before exact target claims",
                        "compiler_required_or_macro_required",
                    );
                }
            }
            "ruby" => {
                if text.contains("rails")
                    || text.contains("route")
                    || text.contains("send")
                    || text.contains("method_missing")
                    || text.contains("monkeypatch")
                    || text.contains("open class")
                {
                    add_risk(
                        "ruby_runtime_framework_unknown",
                        "Ruby send/method_missing/open classes and Rails conventions are runtime/framework boundaries unless exact source facts prove them",
                        "runtime_unknown_or_framework_heuristic",
                    );
                }
            }
            "php" => {
                if text.contains("dynamic include")
                    || text.contains("include")
                    || text.contains("require")
                    || text.contains("magic")
                    || text.contains("composer")
                    || text.contains("autoload")
                {
                    add_risk(
                        "php_dynamic_include_magic_unknown",
                        "PHP dynamic includes, magic methods, globals, and Composer/framework autoload behavior require resolver/runtime evidence before exact target claims",
                        "runtime_unknown_or_resolver_required",
                    );
                }
            }
            _ => {}
        }
        if matches!(
            task_kind,
            "dataflow_trace" | "security_review" | "implementation_trace"
        ) && text.contains("dynamic")
        {
            add_risk(
                "task_dynamic_boundary_unknown",
                "The task asks for behavior that may cross dynamic/runtime boundaries; keep it unknown until exact evidence exists",
                "runtime_unknown",
            );
        }
    }
    risks.into_values().take(8).collect()
}

fn routing_language_unsupported_relations(
    inactive_packet_languages: &[String],
    dynamic_risks: &[Value],
) -> Vec<Value> {
    let mut relations = Vec::new();
    if !inactive_packet_languages.is_empty() {
        let packet_registry = mvp4_local_flow_packet_language_registry_json();
        let active_languages = packet_registry["active_packet_languages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        let proof_boundary = if active_languages.is_empty() {
            "No local-flow packet language adapter is active; observed language evidence cannot carry packet proof."
                .to_string()
        } else {
            format!(
                "Only registry-active local-flow packet language adapters ({}) may carry flow_proof; inactive languages do not emit packet proof.",
                active_languages.join(", ")
            )
        };
        relations.push(json!({
            "relation": "local_flow_packets",
            "status": MVP4_3_LOCAL_FLOW_PACKET_INACTIVE_SUPPORT,
            "languages": inactive_packet_languages,
            "proof_boundary": proof_boundary,
            "blocking": false
        }));
    }
    if !dynamic_risks.is_empty() {
        relations.push(json!({
            "relation": "dynamic_runtime_macro_framework_targets",
            "status": "unknown_or_requires_exact_resolver",
            "proof_boundary": "Dynamic, runtime, macro, preprocessor, framework, and compiler-required targets stay unknown/non-proof unless exact evidence is recorded.",
            "blocking": false
        }));
    }
    relations.push(json!({
        "relation": "ROUTES_TO_MOUNTS_ROUTER_BRIDGES_TO_context_entry",
        "status": "inactive_in_this_lane",
        "proof_boundary": "Route, bridge, and context-entry activation is not part of this pre-MVP4.4 lane.",
        "blocking": false
    }));
    relations
}

fn routing_language_boundary_unknowns(language_plan: &Value) -> Vec<Value> {
    let mut unknowns = Vec::new();
    if language_plan
        .get("unknown_dynamic_risks")
        .and_then(Value::as_array)
        .is_some_and(|risks| !risks.is_empty())
    {
        unknowns.push(json!({
            "claim": "dynamic/runtime/macro/framework target proof",
            "reason": "language_capability_boundary_unknown",
            "sentence": "Language capability metadata found dynamic/runtime/macro/framework boundaries; treat those targets as unknown unless exact graph/resolver evidence later proves them.",
        }));
    }
    if language_plan
        .pointer("/local_flow_packet_boundary/inactive_packet_languages_seen")
        .and_then(Value::as_array)
        .is_some_and(|languages| !languages.is_empty())
    {
        unknowns.push(json!({
            "claim": "inactive-language local-flow packet proof",
            "reason": "unsupported_language_packet_boundary",
            "sentence": "Evidence from languages outside the registry-active packet set can orient the plan, but it does not carry local-flow packet proof in this lane.",
        }));
    }
    unknowns
}

fn routing_annotate_follow_up_queries(queries: &mut [Value], language_plan: &Value) {
    let language_scope = language_plan
        .get("languages")
        .cloned()
        .unwrap_or_else(|| json!(["unknown"]));
    for query in queries {
        let Some(object) = query.as_object_mut() else {
            continue;
        };
        object.insert("shell_ready".to_string(), json!(false));
        object.insert("language_scope".to_string(), language_scope.clone());
        object.insert(
            "capability_boundary".to_string(),
            json!("follow-up query results remain candidate/source evidence until graph/source/resolver verification proves the relation"),
        );
        object.insert(
            "unsupported_relations_are_not_blockers".to_string(),
            json!(true),
        );
    }
}

fn routing_annotate_validation_steps(steps: &mut [Value], language_plan: &Value) {
    let language_scope = language_plan
        .get("languages")
        .cloned()
        .unwrap_or_else(|| json!(["unknown"]));
    for step in steps {
        let Some(object) = step.as_object_mut() else {
            continue;
        };
        object.insert("language_aware".to_string(), json!(true));
        object.insert("language_scope".to_string(), language_scope.clone());
        object.insert(
            "capability_boundary".to_string(),
            json!("Block only on exact current source-spanned graph/resolver evidence; warn or keep unknown for unsupported/dynamic/compiler-required facts."),
        );
    }
}

fn routing_language_validation_steps(language_plan: &Value, limit: usize) -> Vec<Value> {
    let mut steps = Vec::new();
    if let Some(risks) = language_plan
        .get("unknown_dynamic_risks")
        .and_then(Value::as_array)
    {
        for risk in risks.iter().take(limit) {
            steps.push(json!({
                "description": format!(
                    "Inspect {} boundary in {} before claiming exact behavior.",
                    risk.get("boundary").and_then(Value::as_str).unwrap_or("language capability"),
                    risk.get("language").and_then(Value::as_str).unwrap_or("unknown language")
                ),
                "evidence_ids": risk.get("evidence_ids").cloned().unwrap_or_else(|| json!([])),
                "language": risk.get("language").cloned().unwrap_or_else(|| json!("unknown")),
                "source_role": risk.get("source_role").cloned().unwrap_or_else(|| json!("unknown")),
                "risk": risk.get("reason").cloned().unwrap_or_else(|| json!("language capability boundary")),
                "recommendation_kind": "language_capability_boundary_check",
                "blocking": false,
            }));
        }
    }
    if steps.len() < limit
        && language_plan
            .pointer("/local_flow_packet_boundary/inactive_packet_languages_seen")
            .and_then(Value::as_array)
            .is_some_and(|languages| !languages.is_empty())
    {
        steps.push(json!({
            "description": "Do not request or rely on local-flow packet proof for evidence outside the registry-active packet language set in this lane.",
            "language": "registry_inactive",
            "inactive_language_scope": true,
            "risk": "packet support is not implemented outside the registry-active source gate",
            "recommendation_kind": "packet_boundary_check",
            "blocking": false,
        }));
    }
    if steps.is_empty() {
        steps.push(json!({
            "description": "Use source spans and exactness labels before promoting language facts to proof.",
            "language": "all",
            "risk": "capability metadata is planning context, not graph proof by itself",
            "recommendation_kind": "language_capability_boundary_check",
            "blocking": false,
        }));
    }
    steps.truncate(limit);
    steps
}

pub(crate) fn routing_candidate_file(candidate: &Value) -> Option<String> {
    candidate
        .get("path")
        .and_then(Value::as_str)
        .or_else(|| candidate.get("file").and_then(Value::as_str))
        .or_else(|| candidate.get("file_id").and_then(Value::as_str))
        .or_else(|| {
            candidate
                .get("span")
                .and_then(|span| span.get("file").and_then(Value::as_str))
        })
        .filter(|file| !file.trim().is_empty())
        .map(str::to_string)
}

pub(crate) fn routing_candidate_claimability(candidate: &Value) -> Value {
    let graph_proof = candidate
        .get("graph_proof")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let claimable_for_text = candidate
        .get("claimable_for_text")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut claimable_as = Vec::new();
    if graph_proof {
        claimable_as.push("graph_relation_proof");
    }
    if claimable_for_text || graph_proof {
        claimable_as.push("source_text_existence");
    }
    json!({
        "claimable_as": claimable_as,
        "not_claimable_as": if graph_proof { Vec::<&str>::new() } else { vec!["graph_relation_proof"] },
    })
}

pub(crate) fn routing_role_for_value(
    task_kind: &str,
    retrieval_plan: &codegraph_query::RetrievalPlan,
    value: &Value,
    file: &str,
) -> String {
    if let Some(atom_role) = routing_plan_atom_role_from_value(retrieval_plan, value) {
        return atom_role;
    }
    if let Some(atom_role) = routing_matching_atom_role(retrieval_plan, value, file) {
        return atom_role;
    }
    let lower_file = context_planning_normalize_path(file).to_ascii_lowercase();
    let text = routing_value_text(value).to_ascii_lowercase();
    match task_kind {
        "build_system_package_authoring" => routing_buildroot_role(&lower_file, &text).to_string(),
        "codegraph_internal_debug" => routing_codegraph_debug_role(&lower_file, &text).to_string(),
        "storage_accounting_trace" | "artifact_math_trace" | "benchmark_metric_trace" => {
            routing_vector_metric_role(&lower_file, &text).to_string()
        }
        "implementation_trace"
        | "persistence_path_trace"
        | "indexing_summary_trace"
        | "schema_view_trace" => routing_implementation_role(&lower_file, &text).to_string(),
        "test_impact" => routing_test_impact_role(&lower_file, &text).to_string(),
        "dataflow_trace" => routing_dataflow_role(&lower_file, &text).to_string(),
        "security_review" => routing_security_role(&lower_file, &text).to_string(),
        "docs_lookup" => routing_docs_role(&lower_file, &text).to_string(),
        _ => if !lower_file.is_empty() {
            "file_lookup"
        } else {
            "text_evidence"
        }
        .to_string(),
    }
}

pub(crate) fn routing_plan_atom_role_from_value(
    retrieval_plan: &codegraph_query::RetrievalPlan,
    value: &Value,
) -> Option<String> {
    let mut candidates = Vec::new();
    for key in ["evidence_id", "id", "candidate_id"] {
        if let Some(raw) = value.get(key).and_then(Value::as_str) {
            candidates.push(raw.to_string());
        }
    }
    if let Some(fallback) = value.get("fallback") {
        for key in ["id", "evidence_id"] {
            if let Some(raw) = fallback.get(key).and_then(Value::as_str) {
                candidates.push(raw.to_string());
            }
        }
    }
    for candidate in candidates {
        let Some(rest) = candidate.strip_prefix("plan-atom://") else {
            continue;
        };
        let Some((role, _)) = rest.split_once(':') else {
            continue;
        };
        if retrieval_plan
            .query_atoms
            .iter()
            .any(|atom| atom.role == role)
        {
            return Some(role.to_string());
        }
    }
    None
}

pub(crate) fn routing_matching_atom_role(
    retrieval_plan: &codegraph_query::RetrievalPlan,
    value: &Value,
    file: &str,
) -> Option<String> {
    let normalized_file = context_planning_normalize_path(file).to_ascii_lowercase();
    let combined = format!(
        "{}\n{}",
        normalized_file,
        routing_value_text(value).to_ascii_lowercase()
    );
    for atom in &retrieval_plan.query_atoms {
        let path_match = atom
            .path_hints
            .iter()
            .any(|hint| routing_path_hint_matches(&normalized_file, &hint.to_ascii_lowercase()));
        let signal_match_count = atom
            .expected_signal
            .split(|character: char| {
                !(character.is_ascii_alphanumeric() || character == '_' || character == '-')
            })
            .filter(|part| part.len() > 3)
            .filter(|part| combined.contains(&part.to_ascii_lowercase()))
            .count();
        let signal_match = signal_match_count >= 2;
        if path_match || signal_match {
            return Some(atom.role.clone());
        }
    }
    None
}

pub(crate) fn routing_path_hint_matches(file: &str, hint: &str) -> bool {
    if hint.is_empty() {
        return false;
    }
    let hint = hint.trim_matches('*').trim_end_matches('/');
    if hint.is_empty() {
        return false;
    }
    file == hint || file.starts_with(hint) || file.contains(hint)
}

pub(crate) fn routing_buildroot_role(file: &str, text: &str) -> &'static str {
    if file.contains("docs/manual/adding-packages") || file.starts_with("docs/manual") {
        "authoring_docs"
    } else if file == "package/config.in" || text.contains("source \"package/") {
        "kconfig_wiring"
    } else if file == "package/pkg-download.mk" || file.starts_with("support/download") {
        "download_infrastructure"
    } else if file.starts_with("support/scripts") {
        "support_scripts"
    } else if file == "package/pkg-generic.mk" || text.contains("inner-generic-package") {
        "build_install_infrastructure"
    } else if file.ends_with(".mk") || text.contains("_version") || text.contains("_license") {
        "package_metadata"
    } else if file.ends_with("config.in") || text.contains("br2_package") {
        "kconfig_wiring"
    } else if file.starts_with("package/") {
        "examples"
    } else {
        "text_evidence"
    }
}

pub(crate) fn routing_codegraph_debug_role(file: &str, text: &str) -> &'static str {
    if text.contains("preflight") || text.contains("passport") || text.contains("stale db") {
        "lifecycle_preflight"
    } else if text.contains("open_read") || text.contains("sqlitegraphstore") {
        "store_open"
    } else if text.contains("doctor") || text.contains("status") {
        "status_doctor"
    } else if file.contains("/tests") || text.contains("#[test]") {
        "tests"
    } else if text.contains("schema") || file.ends_with(".md") {
        "schemas_docs"
    } else {
        "entrypoint_symbols"
    }
}

pub(crate) fn routing_implementation_role(file: &str, text: &str) -> &'static str {
    if file.contains("/tests") || text.contains("#[test]") {
        "related_tests"
    } else if text.contains("struct ") || text.contains("enum ") || text.contains(" type ") {
        "relevant_structs"
    } else if text.contains("const ") || text.contains("static ") {
        "relevant_constants"
    } else if text.contains("persist") || text.contains("write") || text.contains("artifact") {
        "persistence_path"
    } else if text.contains("count") || text.contains("summary") || text.contains("accounting") {
        "accounting_summary"
    } else if text.contains("helper") {
        "same_file_helpers"
    } else {
        "definitions"
    }
}

pub(crate) fn routing_vector_metric_role(file: &str, text: &str) -> &'static str {
    if text.contains("diversity_ranked_v1") || text.contains("input_order_cap") {
        "vector_chunk_selection"
    } else if text.contains("actual_index_file_bytes")
        || text.contains("estimated_f32_payload_bytes")
        || text.contains("vector_payload_compression")
    {
        "metric_reporting"
    } else if text.contains("write_vector") || text.contains("artifact") || file.ends_with(".json")
    {
        "artifact_writer"
    } else if file.contains("/tests") || text.contains("#[test]") {
        "tests"
    } else if text.contains("persisted") || text.contains("generated") || text.contains("selected")
    {
        "persisted_index_summary"
    } else {
        "vector_chunk_index_build"
    }
}

pub(crate) fn routing_test_impact_role(file: &str, text: &str) -> &'static str {
    if file.contains("/tests") || text.contains("#[test]") || text.contains("assert") {
        "test_files"
    } else if text.contains("mock") || text.contains("fixture") {
        "mocks_assertions"
    } else if text.contains("impact") {
        "test_impact_fallback"
    } else {
        "production_target"
    }
}

pub(crate) fn routing_dataflow_role(_file: &str, text: &str) -> &'static str {
    if text.contains("sanitize") || text.contains("validate") {
        "sanitizer"
    } else if text.contains("database") || text.contains("write") || text.contains("sink") {
        "sink"
    } else if text.contains("request") || text.contains("input") || text.contains("source") {
        "source"
    } else if text.contains("mutation") || text.contains("insert") || text.contains("update") {
        "mutation_write"
    } else {
        "intermediate_helper"
    }
}

pub(crate) fn routing_security_role(file: &str, text: &str) -> &'static str {
    if text.contains("role") || text.contains("admin") || text.contains("checkrole") {
        "role_check"
    } else if text.contains("permission") || text.contains("gate") || text.contains("rbac") {
        "permission_gate"
    } else if text.contains("sanitize") || text.contains("validate") {
        "sanitizer_validator"
    } else if text.contains("authorize") || text.contains("auth") {
        "auth_entrypoint"
    } else if file.contains("/tests") || text.contains("#[test]") {
        "tests"
    } else {
        "route_expose"
    }
}

pub(crate) fn routing_docs_role(file: &str, _text: &str) -> &'static str {
    if file.starts_with("docs/manual") {
        "docs_manual"
    } else if file.ends_with("readme.md") || file.starts_with("docs/") {
        "readme_docs"
    } else if file.ends_with(".toml") || file.ends_with(".json") || file.ends_with("config.in") {
        "config_reference_files"
    } else {
        "related_source"
    }
}

pub(crate) fn routing_value_text(value: &Value) -> String {
    let mut parts = Vec::new();
    for key in [
        "symbol",
        "kind",
        "reason",
        "classification_reason",
        "fallback_source",
        "text",
        "text_preview",
        "snippet",
        "matched_signal",
    ] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            parts.push(text.to_string());
        }
    }
    for nested in ["candidate", "fallback", "snippet"] {
        if let Some(value) = value.get(nested) {
            parts.push(routing_value_text(value));
        }
    }
    parts.join("\n")
}

pub(crate) fn routing_matched_signal(candidate: &Value) -> String {
    candidate
        .get("matched_seeds")
        .and_then(Value::as_array)
        .and_then(|seeds| seeds.iter().filter_map(Value::as_str).next())
        .or_else(|| candidate.get("matched_token").and_then(Value::as_str))
        .or_else(|| candidate.get("reason").and_then(Value::as_str))
        .unwrap_or("candidate retrieval signal")
        .to_string()
}

pub(crate) fn routing_evidence_dedup_key(item: &Value) -> String {
    let file = item
        .get("file")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let span = item.get("span").unwrap_or(&Value::Null);
    let start = span
        .get("start_line")
        .or_else(|| span.get("line"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let end = span
        .get("end_line")
        .and_then(Value::as_u64)
        .unwrap_or(start);
    format!("{file}:{start}:{end}")
}

pub(crate) fn routing_evidence_id(item: &Value) -> String {
    item.get("evidence_id")
        .and_then(Value::as_str)
        .or_else(|| item.get("id").and_then(Value::as_str))
        .or_else(|| item.get("candidate_id").and_then(Value::as_str))
        .or_else(|| item.get("path_id").and_then(Value::as_str))
        .unwrap_or("unknown")
        .to_string()
}

pub(crate) fn routing_merge_evidence(existing: &mut Value, incoming: &mut Value) {
    let sources = routing_string_values(existing, "candidate_sources")
        .into_iter()
        .chain(routing_string_values(incoming, "candidate_sources"))
        .collect::<BTreeSet<_>>();
    let Some(existing_object) = existing.as_object_mut() else {
        return;
    };
    existing_object.insert(
        "candidate_sources".to_string(),
        json!(sources.into_iter().collect::<Vec<_>>()),
    );
    let existing_graph = existing_object
        .get("graph_proof")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let incoming_graph = incoming
        .get("graph_proof")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if incoming_graph && !existing_graph {
        existing_object.insert("graph_proof".to_string(), json!(true));
        existing_object.insert("proof_status".to_string(), json!("proof_path_found"));
    }
    copy_context_candidate_degradation_fields(incoming, existing_object);
}

pub(crate) fn routing_string_values(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn routing_evidence_rank_score(task_kind: &str, item: &Value) -> i64 {
    let mut score = 0i64;
    if item
        .get("graph_proof")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        score += 5_000;
    }
    if routing_string_values(item, "candidate_sources")
        .iter()
        .any(|source| source == "exact_seed" || source == "file_path_seed")
    {
        score += 4_000;
    }
    if routing_string_values(item, "candidate_sources")
        .iter()
        .any(|source| source == "text_evidence")
    {
        score += 1_500;
    }
    let role = item
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    score += (100usize.saturating_sub(routing_role_priority(task_kind, role)) as i64) * 20;
    if let Some(file) = item.get("file").and_then(Value::as_str) {
        score += (40usize.saturating_sub(context_planning_central_file_score(file)) as i64) * 25;
        let lower = context_planning_normalize_path(file).to_ascii_lowercase();
        if lower.contains("/tests") || lower.ends_with("_test.rs") {
            if matches!(task_kind, "test_impact" | "security_review") {
                score += 600;
            } else {
                score -= 450;
            }
        }
    }
    score
}

pub(crate) fn routing_role_priority(task_kind: &str, role: &str) -> usize {
    let roles: &[&str] = match task_kind {
        "build_system_package_authoring" => &[
            "authoring_docs",
            "package_metadata",
            "kconfig_wiring",
            "makefile_inclusion",
            "download_infrastructure",
            "build_install_infrastructure",
            "support_scripts",
            "examples",
        ],
        "codegraph_internal_debug" => &[
            "entrypoint_symbols",
            "lifecycle_preflight",
            "store_open",
            "status_doctor",
            "tests",
            "schemas_docs",
        ],
        "storage_accounting_trace" | "artifact_math_trace" | "benchmark_metric_trace" => &[
            "vector_chunk_index_build",
            "vector_chunk_selection",
            "metric_reporting",
            "artifact_writer",
            "persisted_index_summary",
            "tests",
            "release_smoke_report_surfaces",
        ],
        "test_impact" => &[
            "changed_symbol",
            "production_target",
            "test_files",
            "mocks_assertions",
            "test_impact_fallback",
        ],
        "dataflow_trace" => &[
            "source",
            "intermediate_helper",
            "sanitizer",
            "sink",
            "mutation_write",
            "proof_path_attempt",
        ],
        "security_review" => &[
            "auth_entrypoint",
            "role_check",
            "permission_gate",
            "sanitizer_validator",
            "route_expose",
            "tests",
        ],
        "docs_lookup" => &[
            "docs_manual",
            "readme_docs",
            "config_reference_files",
            "related_source",
        ],
        _ => &[
            "definitions",
            "same_file_helpers",
            "relevant_structs",
            "relevant_constants",
            "callers",
            "callees",
            "related_tests",
            "persistence_path",
            "accounting_summary",
        ],
    };
    roles
        .iter()
        .position(|candidate| *candidate == role)
        .unwrap_or(roles.len() + 10)
}

pub(crate) fn routing_critical_files(evidence: &[Value], limit: usize) -> (Vec<Value>, usize) {
    let mut files = Vec::<Value>::new();
    let mut seen = BTreeSet::new();
    for item in evidence {
        let Some(file) = item.get("file").and_then(Value::as_str) else {
            continue;
        };
        let key = context_planning_normalize_path(file).to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        let mut file_entry = json!({
            "file": file,
            "role": item.get("role").cloned().unwrap_or_else(|| json!("unknown")),
            "language_capability": routing_compact_language_capability_for_item(item, "source_navigation_evidence"),
            "evidence_ids": [routing_evidence_id(item)],
            "why": format!(
                "selected as {} for this task",
                item.get("role").and_then(Value::as_str).unwrap_or("evidence")
            ),
            "proof_status": item.get("proof_status").cloned().unwrap_or_else(|| json!("unknown")),
            "graph_proof": item.get("graph_proof").and_then(Value::as_bool).unwrap_or(false),
        });
        if let Some(object) = file_entry.as_object_mut() {
            copy_context_candidate_degradation_fields(item, object);
        }
        files.push(file_entry);
    }
    let omitted = files.len().saturating_sub(limit);
    files.truncate(limit);
    (files, omitted)
}

pub(crate) fn routing_critical_symbols(
    packet: &ContextPacket,
    evidence: &[Value],
    limit: usize,
) -> (Vec<Value>, usize) {
    let mut symbols = Vec::new();
    let mut seen = BTreeSet::new();
    for symbol in &packet.symbols {
        if !context_planning_symbol_allowed(symbol) {
            continue;
        }
        if seen.insert(symbol.to_ascii_lowercase()) {
            let evidence_ids = evidence
                .iter()
                .filter(|item| routing_value_text(item).contains(symbol))
                .map(routing_evidence_id)
                .take(3)
                .collect::<Vec<_>>();
            let language_capability = evidence
                .iter()
                .find(|item| routing_value_text(item).contains(symbol))
                .map(|item| {
                    routing_compact_language_capability_for_item(item, "source_navigation_evidence")
                })
                .unwrap_or_else(|| {
                    context_pack_source_language_capability_json(
                        None,
                        "symbol_candidate",
                        "candidate_or_source_navigation",
                        false,
                        "unknown",
                        "unknown",
                    )
                });
            symbols.push(json!({
                "symbol": symbol,
                "evidence_ids": evidence_ids,
                "language_capability": language_capability,
                "proof_status": "candidate_or_source_navigation",
                "graph_proof": false,
            }));
        }
    }
    let omitted = symbols.len().saturating_sub(limit);
    symbols.truncate(limit);
    (symbols, omitted)
}

pub(crate) fn routing_verified_paths(
    paths: &[Value],
    lifecycle_claimable: bool,
    graph_proof: bool,
    limit: usize,
) -> Vec<Value> {
    if !graph_proof {
        return Vec::new();
    }
    paths
        .iter()
        .take(limit)
        .map(|path| {
            let mut path = path.clone();
            if let Some(object) = path.as_object_mut() {
                object.insert("proof_status".to_string(), json!("proof_path_found"));
                object.insert("graph_proof".to_string(), json!(true));
                object.insert(
                    "claimability".to_string(),
                    json!({
                        "claimable": lifecycle_claimable,
                        "claimable_as": if lifecycle_claimable { vec!["graph_relation_proof", "source_text_existence"] } else { Vec::<&str>::new() },
                        "not_claimable_as": if lifecycle_claimable { Vec::<&str>::new() } else { vec!["graph_relation_proof"] },
                    }),
                );
            }
            path
        })
        .collect()
}

pub(crate) fn routing_text_evidence(evidence: &[Value], limit: usize) -> (Vec<Value>, usize) {
    let mut selected = evidence
        .iter()
        .filter(|item| {
            !item
                .get("graph_proof")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && (routing_string_values(item, "candidate_sources")
                    .iter()
                    .any(|source| {
                        matches!(
                            source.as_str(),
                            "text_evidence" | "lexical_fts" | "fallback_snippet"
                        )
                    })
                    || item.get("evidence_role").and_then(Value::as_str) == Some("text_evidence"))
        })
        .map(|item| routing_compact_evidence_json(item, false, "source_text_evidence"))
        .collect::<Vec<_>>();
    let omitted = selected.len().saturating_sub(limit);
    selected.truncate(limit);
    (selected, omitted)
}

pub(crate) fn routing_source_navigation_evidence(
    task_kind: &str,
    evidence: &[Value],
    limit: usize,
) -> (Vec<Value>, usize) {
    let include = matches!(
        task_kind,
        "implementation_trace"
            | "storage_accounting_trace"
            | "artifact_math_trace"
            | "persistence_path_trace"
            | "indexing_summary_trace"
            | "schema_view_trace"
            | "benchmark_metric_trace"
            | "codegraph_internal_debug"
            | "test_impact"
            | "dataflow_trace"
            | "security_review"
    );
    if !include {
        return (Vec::new(), 0);
    }
    let mut selected = evidence
        .iter()
        .filter(|item| item.get("file").and_then(Value::as_str).is_some())
        .map(|item| routing_compact_evidence_json(item, false, "source_navigation_evidence"))
        .collect::<Vec<_>>();
    let omitted = selected.len().saturating_sub(limit);
    selected.truncate(limit);
    (selected, omitted)
}

pub(crate) fn routing_compact_evidence_json(
    item: &Value,
    graph_proof: bool,
    evidence_type: &str,
) -> Value {
    let mut value = json!({
        "evidence_id": item.get("evidence_id").cloned().unwrap_or(Value::Null),
        "role": item.get("role").cloned().unwrap_or_else(|| json!("unknown")),
        "file": item.get("file").cloned().unwrap_or(Value::Null),
        "span": item.get("span").cloned().unwrap_or(Value::Null),
        "candidate_sources": item.get("candidate_sources").cloned().unwrap_or_else(|| json!([])),
        "evidence_role": item.get("evidence_role").cloned().unwrap_or_else(|| json!("unknown")),
        "evidence_type": evidence_type,
        "proof_status": if graph_proof {
            item.get("proof_status").cloned().unwrap_or_else(|| json!("proof_path_found"))
        } else {
            json!(item.get("proof_status").and_then(Value::as_str).unwrap_or("not_graph_proof"))
        },
        "graph_proof": graph_proof,
        "claimability": if graph_proof {
            json!({"claimable_as": ["graph_relation_proof", "source_text_existence"], "not_claimable_as": []})
        } else {
            json!({"claimable_as": ["source_text_existence"], "not_claimable_as": ["graph_relation_proof"]})
        },
        "matched_signal": item.get("matched_signal").cloned().unwrap_or_else(|| json!("source signal")),
        "reason": item.get("reason").cloned().unwrap_or_else(|| json!("source evidence")),
        "language_capability": routing_compact_language_capability_for_item(item, evidence_type),
    });
    if let Some(object) = value.as_object_mut() {
        copy_context_candidate_degradation_fields(item, object);
    }
    value
}

pub(crate) fn routing_fallback_snippets(
    snippets: &[Value],
    text_evidence: &[Value],
    limit: usize,
) -> (Vec<Value>, usize) {
    let mut selected = snippets
        .iter()
        .filter(|snippet| snippet.get("fallback_source").is_some())
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        for evidence in text_evidence.iter().take(limit) {
            selected.push(json!({
                "file": evidence.get("file").cloned().unwrap_or(Value::Null),
                "span": evidence.get("span").cloned().unwrap_or(Value::Null),
                "evidence_id": evidence.get("evidence_id").cloned().unwrap_or(Value::Null),
                "fallback_source": "text_evidence_compact_fallback",
                "proof_status": evidence.get("proof_status").cloned().unwrap_or_else(|| json!("no_proof_path_found")),
                "graph_proof": false,
            }));
        }
    }
    let total = snippets
        .iter()
        .filter(|snippet| snippet.get("fallback_source").is_some())
        .count()
        .max(selected.len());
    let omitted = total.saturating_sub(limit);
    selected.truncate(limit);
    (selected, omitted)
}

pub(crate) fn routing_follow_up_queries(
    planning_packet: &Value,
    retrieval_plan: &codegraph_query::RetrievalPlan,
    limit: usize,
) -> (Vec<Value>, usize) {
    let mut queries = planning_packet
        .get("follow_up_queries")
        .and_then(Value::as_array)
        .map(|queries| {
            queries
                .iter()
                .map(|query| {
                    json!({
                        "query_text": query.get("query_text").cloned().unwrap_or(Value::Null),
                        "path_scope": query.get("path_scope").cloned().unwrap_or_else(|| json!("*")),
                        "why": query.get("why").or_else(|| query.get("reason")).cloned().unwrap_or_else(|| json!("derived from current evidence")),
                        "expected_signal": query.get("expected_signal").cloned().unwrap_or_else(|| json!("source signal")),
                        "risk": query.get("risk").cloned().unwrap_or_else(|| json!("candidate_only_until_verified")),
                        "max_results_hint": query.get("max_results_hint").cloned().unwrap_or_else(|| json!(10)),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if queries.is_empty() {
        queries.extend(retrieval_plan.query_atoms.iter().map(|atom| {
            json!({
                "query_text": atom.query_text,
                "path_scope": if atom.path_hints.is_empty() { "*".to_string() } else { atom.path_hints.join(",") },
                "why": atom.why,
                "expected_signal": atom.expected_signal,
                "risk": "candidate_only_until_verified",
                "max_results_hint": atom.max_candidates,
            })
        }));
    }
    let omitted = queries.len().saturating_sub(limit);
    queries.truncate(limit);
    (queries, omitted)
}

pub(crate) fn routing_artifact_inspection_requirements(
    task_kind: &str,
    packet: &ContextPacket,
    evidence: &[Value],
    limit: usize,
) -> Vec<Value> {
    if !routing_task_requires_artifact_or_db_inspection(task_kind, packet) {
        return Vec::new();
    }
    let evidence_ids = evidence
        .iter()
        .map(routing_evidence_id)
        .take(3)
        .collect::<Vec<_>>();
    let mut requirements = vec![json!({
        "claim": "final persisted artifact values",
        "why": "source code alone does not prove the final persisted value",
        "evidence_ids": evidence_ids,
        "required": true,
        "sentence": "Artifact or DB inspection is required for final persisted artifact values; source code alone does not prove the final persisted value.",
    })];
    if routing_task_touches_vector_metrics(task_kind, packet) {
        requirements.push(json!({
            "claim": "actual vector runtime sidecar and audit artifact bytes",
            "fields": [
                "runtime_sidecar_bytes",
                "audit_artifact_bytes",
                "actual_index_file_bytes",
                "estimated_f32_payload_bytes",
                "index_artifact_format",
                "vector_payload_compression",
                "pretty_json_overhead"
            ],
            "required": true,
            "sentence": "Inspect the runtime sidecar, optional audit artifact, and DB passport before claiming vector artifact byte math; source code alone does not prove persisted values.",
        }));
    }
    requirements.truncate(limit);
    requirements
}

pub(crate) fn routing_db_inspection_requirements(
    task_kind: &str,
    evidence: &[Value],
    limit: usize,
) -> Vec<Value> {
    if !matches!(
        task_kind,
        "implementation_trace"
            | "storage_accounting_trace"
            | "artifact_math_trace"
            | "persistence_path_trace"
            | "indexing_summary_trace"
            | "codegraph_internal_debug"
    ) {
        return Vec::new();
    }
    let evidence_ids = evidence
        .iter()
        .map(routing_evidence_id)
        .take(3)
        .collect::<Vec<_>>();
    let mut requirements = vec![json!({
        "claim": "final persisted DB rows or lifecycle state",
        "why": "source code alone does not prove DB row counts, stale-DB state, or persisted values",
        "evidence_ids": evidence_ids,
        "required": true,
        "sentence": "Artifact or DB inspection is required for final persisted DB rows or lifecycle state; source code alone does not prove the final persisted value.",
    })];
    requirements.truncate(limit);
    requirements
}

pub(crate) fn routing_task_requires_artifact_or_db_inspection(
    task_kind: &str,
    packet: &ContextPacket,
) -> bool {
    matches!(
        task_kind,
        "implementation_trace"
            | "storage_accounting_trace"
            | "artifact_math_trace"
            | "persistence_path_trace"
            | "indexing_summary_trace"
            | "benchmark_metric_trace"
    ) || routing_task_touches_vector_metrics(task_kind, packet)
}

pub(crate) fn routing_task_touches_vector_metrics(task_kind: &str, packet: &ContextPacket) -> bool {
    matches!(
        task_kind,
        "storage_accounting_trace" | "artifact_math_trace" | "benchmark_metric_trace"
    ) || packet.task.to_ascii_lowercase().contains("vector")
        || packet.task.contains("actual_index_file_bytes")
        || packet.task.contains("estimated_f32_payload_bytes")
}

pub(crate) fn routing_formulas_or_accounting_notes(
    task_kind: &str,
    packet: &ContextPacket,
    limit: usize,
) -> Vec<Value> {
    let mut notes = Vec::new();
    if routing_task_touches_vector_metrics(task_kind, packet) {
        notes.push(routing_vector_metric_truth_note(packet));
    }
    if matches!(
        task_kind,
        "implementation_trace" | "storage_accounting_trace" | "artifact_math_trace"
    ) {
        notes.push(json!({
            "note_id": "implementation_accounting_not_relation_proof",
            "claim": "implementation accounting trace",
            "proof_status": "not_graph_relation_proof",
            "sentence": "This packet traces implementation accounting, not graph relation proof.",
        }));
    }
    notes.truncate(limit);
    notes
}

pub(crate) fn routing_vector_metric_truth_note(packet: &ContextPacket) -> Value {
    let metrics = packet
        .metadata
        .get("vector_candidate_trace")
        .and_then(|trace| trace.get("vector_index_metrics"));
    let metric_value = |key: &str| metrics.and_then(|metrics| metrics.get(key)).cloned();
    let actual = metric_value("actual_index_file_bytes").unwrap_or_else(|| json!("unknown"));
    let estimated = metric_value("estimated_f32_payload_bytes").unwrap_or_else(|| json!("unknown"));
    let artifact_kind = metric_value("artifact_kind").unwrap_or_else(|| json!("unknown"));
    json!({
        "note_id": "vector_metric_truthfulness",
        "artifact_kind": artifact_kind,
        "actual_index_file_bytes": actual,
        "runtime_sidecar_bytes": metric_value("runtime_sidecar_bytes").unwrap_or_else(|| json!("unknown")),
        "audit_artifact_bytes": metric_value("audit_artifact_bytes").unwrap_or_else(|| json!("unknown")),
        "pretty_json_overhead": metric_value("pretty_json_overhead").unwrap_or_else(|| json!("unknown")),
        "estimated_f32_payload_bytes": estimated,
        "index_artifact_format": metric_value("index_artifact_format").unwrap_or_else(|| json!("pretty_json")),
        "vector_payload_compression": metric_value("vector_payload_compression").unwrap_or_else(|| json!("none")),
        "stores_chunk_text": metric_value("stores_chunk_text").unwrap_or_else(|| json!("unknown")),
        "stores_chunk_metadata": metric_value("stores_chunk_metadata").unwrap_or_else(|| json!("unknown")),
        "stores_full_source_body": metric_value("stores_full_source_body").unwrap_or_else(|| json!("unknown")),
        "generated_total_chunks": metric_value("generated_total_chunks").unwrap_or_else(|| json!("unknown")),
        "selected_total_chunks": metric_value("selected_total_chunks").unwrap_or_else(|| json!("unknown")),
        "persisted_total_chunks": metric_value("persisted_total_chunks").unwrap_or_else(|| json!("unknown")),
        "chunk_selection_strategy": metric_value("chunk_selection_strategy").unwrap_or_else(|| json!("diversity_ranked_v1")),
        "input_order_cap": metric_value("input_order_cap").unwrap_or_else(|| json!(false)),
        "not_compressed_vector_storage_claim": true,
        "sentence": routing_vector_metric_sentence(&metric_value("artifact_kind").unwrap_or_else(|| json!("unknown")), &metric_value("actual_index_file_bytes").unwrap_or_else(|| json!("unknown")), &metric_value("estimated_f32_payload_bytes").unwrap_or_else(|| json!("unknown"))),
    })
}

pub(crate) fn routing_vector_metric_sentence(
    artifact_kind: &Value,
    actual: &Value,
    estimated: &Value,
) -> String {
    format!(
        "{} reports {} as the inspected artifact size for that runtime/audit/legacy artifact; {} is the estimated raw float32 payload size. This is not a compressed-vector storage claim.",
        routing_sentence_value(artifact_kind),
        routing_sentence_value(actual),
        routing_sentence_value(estimated)
    )
}

pub(crate) fn routing_sentence_value(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

pub(crate) fn routing_risks(
    task_kind: &str,
    packet: &ContextPacket,
    text_evidence: &[Value],
    formulas_or_accounting_notes: &[Value],
    limit: usize,
) -> Vec<Value> {
    let mut risks = vec![json!({
        "risk_id": "text_evidence_not_graph_proof",
        "forbidden_claim": "graph relation proof",
        "evidence_type": "text evidence",
        "sentence": "Do not infer graph relation proof from text evidence.",
    })];
    if !text_evidence.is_empty() {
        risks.push(json!({
            "risk_id": "source_navigation_not_graph_proof",
            "forbidden_claim": "verified graph path",
            "evidence_type": "source navigation evidence",
            "sentence": "Do not infer verified graph path from source navigation evidence.",
        }));
    }
    if routing_task_touches_vector_metrics(task_kind, packet) {
        risks.push(json!({
            "risk_id": "pretty_json_not_compressed_storage",
            "forbidden_claim": "compressed-vector storage",
            "evidence_type": "pretty_json vector artifact",
            "sentence": "Do not infer compressed-vector storage from pretty_json vector artifact.",
        }));
        risks.push(json!({
            "risk_id": "vector_sidecar_not_complete_path_index",
            "forbidden_claim": "complete file/path/symbol index",
            "evidence_type": "selected vector runtime sidecar",
            "sentence": "Do not treat the selected vector runtime sidecar as a complete file/path/symbol index.",
        }));
        risks.push(json!({
            "risk_id": "audit_artifact_not_runtime_source",
            "forbidden_claim": "runtime retrieval source",
            "evidence_type": "vector audit artifact",
            "sentence": "Do not treat the vector audit artifact as a runtime retrieval source.",
        }));
    }
    if formulas_or_accounting_notes.iter().any(|note| {
        note.get("note_id").and_then(Value::as_str) == Some("vector_metric_truthfulness")
    }) {
        risks.push(json!({
            "risk_id": "artifact_bytes_not_payload_bytes",
            "forbidden_claim": "raw float32 payload size equals JSON artifact bytes",
            "evidence_type": "vector metric labels",
            "sentence": "Do not infer raw float32 payload size equals JSON artifact bytes from vector metric labels.",
        }));
    }
    if matches!(task_kind, "security_review") {
        risks.push(json!({
            "risk_id": "strings_comments_not_authorization_proof",
            "forbidden_claim": "authorization enforcement",
            "evidence_type": "comments or strings",
            "sentence": "Do not infer authorization enforcement from comments or strings.",
        }));
    }
    risks.truncate(limit);
    risks
}

pub(crate) fn routing_unknowns(
    task_kind: &str,
    graph_proof: bool,
    evidence_status: &str,
    artifact_requirements: &[Value],
    db_requirements: &[Value],
) -> Vec<Value> {
    let mut unknowns = Vec::new();
    if !graph_proof {
        unknowns.push(json!({
            "claim": "graph relation proof",
            "reason": "no_proof_path_found",
            "sentence": "I could not prove graph relation proof. Treat this as unknown unless a later graph/source verification step proves it.",
        }));
    }
    if evidence_status == "no_evidence_found" {
        unknowns.push(json!({
            "claim": "relevant source evidence",
            "reason": "no_evidence_found",
            "sentence": "I could not prove relevant source evidence. Treat this as unknown unless a later graph/source verification step proves it.",
        }));
    }
    if !artifact_requirements.is_empty() {
        unknowns.push(json!({
            "claim": "final persisted artifact value",
            "reason": "artifact_inspection_required",
            "sentence": "I could not prove final persisted artifact value. Treat this as unknown unless a later graph/source verification step proves it.",
        }));
    }
    if !db_requirements.is_empty() {
        unknowns.push(json!({
            "claim": "final persisted DB value",
            "reason": "db_inspection_required",
            "sentence": "I could not prove final persisted DB value. Treat this as unknown unless a later graph/source verification step proves it.",
        }));
    }
    if task_kind == "unknown" {
        unknowns.push(json!({
            "claim": "task-specific routing intent",
            "reason": "unknown_or_ambiguous_task",
            "sentence": "I could not prove task-specific routing intent. Treat this as unknown unless a later graph/source verification step proves it.",
        }));
    }
    unknowns
}

pub(crate) fn routing_validation_steps(
    task_kind: &str,
    critical_files: &[Value],
    evidence: &[Value],
    artifact_requirements: &[Value],
    formulas: &[Value],
    limit: usize,
) -> Vec<Value> {
    let evidence_ids = evidence
        .iter()
        .map(routing_evidence_id)
        .take(4)
        .collect::<Vec<_>>();
    let mut steps = match task_kind {
        "build_system_package_authoring" => vec![
            routing_validation_step("Validate menuconfig visibility for the new package Config.in entry.", &evidence_ids, "package menu wiring can be wrong even when text evidence is present", "Buildroot package authoring", "general_validation_hint"),
            routing_validation_step("Validate the package build target after adding the .mk file.", &evidence_ids, "metadata and build hooks require a build check", "Buildroot package authoring", "general_validation_hint"),
            routing_validation_step("Validate download hashes when a .hash file or download URL is used.", &evidence_ids, "download evidence does not prove hash correctness", "Buildroot download infrastructure", "general_validation_hint"),
        ],
        "storage_accounting_trace" | "artifact_math_trace" | "benchmark_metric_trace" => vec![
            routing_validation_step("Validate release JSON/report output uses actual_index_file_bytes for JSON artifact bytes.", &evidence_ids, "metric label drift can create false storage claims", "vector metric reporting", "exact_recommendation"),
            routing_validation_step("Validate estimated_f32_payload_bytes is reported as estimated raw float32 payload.", &evidence_ids, "estimated payload bytes are not artifact bytes", "vector metric reporting", "exact_recommendation"),
            routing_validation_step("Inspect the vector index artifact before claiming final persisted bytes.", &routing_requirement_ids(artifact_requirements), "source code alone does not prove persisted artifact values", "artifact inspection", "exact_recommendation"),
        ],
        "implementation_trace" | "persistence_path_trace" | "indexing_summary_trace" => vec![
            routing_validation_step("Inspect definition, helper, and related test spans before editing.", &evidence_ids, "implementation traces can miss same-file helper behavior", "implementation trace", "exact_recommendation"),
            routing_validation_step("Inspect artifact or DB evidence before claiming final persisted values.", &routing_requirement_ids(artifact_requirements), "source code alone does not prove persisted values", "artifact/DB inspection", "exact_recommendation"),
        ],
        "test_impact" => vec![routing_validation_step(
            "Validate production target and affected test files together.",
            &evidence_ids,
            "test-only evidence does not prove production behavior",
            "test impact",
            "exact_recommendation",
        )],
        "dataflow_trace" => vec![routing_validation_step(
            "Validate source, sanitizer, and sink spans before claiming dataflow.",
            &evidence_ids,
            "source/sink text alone is not a verified dataflow path",
            "dataflow",
            "exact_recommendation",
        )],
        "security_review" => vec![routing_validation_step(
            "Validate authorization gate and role check execution path before claiming enforcement.",
            &evidence_ids,
            "comments and role strings are not authorization proof",
            "security",
            "exact_recommendation",
        )],
        "docs_lookup" => vec![routing_validation_step(
            "Validate the docs source text before using it as implementation guidance.",
            &evidence_ids,
            "documentation text is not graph proof",
            "docs",
            "general_validation_hint",
        )],
        _ => vec![routing_validation_step(
            "Inspect the top candidate files before making task-specific claims.",
            &critical_files
                .iter()
                .filter_map(|file| file.get("evidence_ids").and_then(Value::as_array))
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .take(3)
                .collect::<Vec<_>>(),
            "unknown task routing is conservative",
            "unknown",
            "general_validation_hint",
        )],
    };
    if !formulas.is_empty()
        && !steps.iter().any(|step| {
            step.get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .contains("actual_index_file_bytes")
        })
    {
        steps.push(routing_validation_step(
            "Compare artifact bytes vs estimated payload bytes only through the correctly labeled fields.",
            &evidence_ids,
            "field confusion can imply compressed storage that does not exist",
            "vector metric reporting",
            "exact_recommendation",
        ));
    }
    steps.truncate(limit);
    steps
}

pub(crate) fn routing_validation_step(
    description: &str,
    evidence_ids: &[String],
    risk: &str,
    scope: &str,
    recommendation_kind: &str,
) -> Value {
    json!({
        "description": description,
        "command_hint": Value::Null,
        "evidence_ids": evidence_ids,
        "risk": risk,
        "scope": scope,
        "recommendation_kind": recommendation_kind,
    })
}

pub(crate) fn routing_requirement_ids(requirements: &[Value]) -> Vec<String> {
    requirements
        .iter()
        .enumerate()
        .map(|(index, requirement)| {
            requirement
                .get("claim")
                .and_then(Value::as_str)
                .map(|claim| format!("requirement://{}", context_agent_stable_component(claim)))
                .unwrap_or_else(|| format!("requirement://{}", index + 1))
        })
        .collect()
}

pub(crate) fn routing_edit_plan(
    task_kind: &str,
    critical_files: &[Value],
    evidence: &[Value],
    artifact_requirements: &[Value],
    formulas: &[Value],
    limit: usize,
) -> Vec<Value> {
    let evidence_ids = evidence
        .iter()
        .map(routing_evidence_id)
        .take(4)
        .collect::<Vec<_>>();
    let mut steps = match task_kind {
        "build_system_package_authoring" => vec![
            routing_edit_step("Create package/<name>/Config.in.", &evidence_ids, "source_text_evidence", "package name, prompts, dependencies, and selects remain task-specific", "Validate menuconfig visibility."),
            routing_edit_step("Add a source line to package/Config.in.", &evidence_ids, "source_text_evidence", "top-level menu placement may vary by package category", "Validate menuconfig/package menu wiring."),
            routing_edit_step("Create package/<name>/<name>.mk with VERSION, SITE, LICENSE, and DEPENDENCIES as needed.", &evidence_ids, "source_navigation_evidence", "metadata values are package-specific and not proven by examples", "Validate package build target."),
            routing_edit_step("End the package makefile with $(eval $(generic-package)) or the appropriate package macro.", &evidence_ids, "source_navigation_evidence", "host or specialized package macros may be required for some packages", "Inspect package infrastructure docs before choosing the macro."),
            routing_edit_step("Add package/<name>/<name>.hash if downloads are used.", &evidence_ids, "source_text_evidence", "download URLs do not prove hash correctness", "Validate hashes after fetching."),
        ],
        "storage_accounting_trace" | "artifact_math_trace" | "benchmark_metric_trace" => vec![
            routing_edit_step("Identify vector chunk index build and selection spans.", &evidence_ids, "source_navigation_evidence", "source spans do not prove final persisted artifact bytes", "Inspect artifact output before claiming final values."),
            routing_edit_step("Preserve generated, selected, and persisted chunk counts as distinct values.", &evidence_ids, "source_navigation_evidence", "count collapse would hide selection behavior", "Validate release JSON/report output."),
            routing_edit_step("Compare artifact bytes vs estimated payload bytes only through actual_index_file_bytes and estimated_f32_payload_bytes.", &evidence_ids, "source_navigation_evidence", "metric label confusion can imply nonexistent compression", "Validate metric labels in release output."),
        ],
        "implementation_trace" | "persistence_path_trace" | "indexing_summary_trace" => vec![
            routing_edit_step("Identify definition and same-file helper spans before editing.", &evidence_ids, "source_navigation_evidence", "helper behavior can be missed by a definition-only trace", "Inspect definition and helper spans first."),
            routing_edit_step("Inspect artifact or DB evidence when source code alone cannot prove final persisted values.", &routing_requirement_ids(artifact_requirements), "artifact_or_db_inspection_required", "source code alone is not persisted value proof", "Inspect artifact/DB evidence before claiming final values."),
        ],
        "test_impact" => vec![routing_edit_step(
            "Edit the production helper only after mapping affected tests and assertions.",
            &evidence_ids,
            "source_navigation_evidence",
            "test-only evidence can miss production callers",
            "Run targeted affected tests after editing.",
        )],
        _ => Vec::new(),
    };
    if !formulas.is_empty()
        && !steps.iter().any(|step| {
            step.get("step")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .contains("estimated_f32_payload_bytes")
        })
    {
        steps.push(routing_edit_step(
            "Preserve vector metric truthfulness if this edit touches vector accounting surfaces.",
            &evidence_ids,
            "source_navigation_evidence",
            "pretty_json artifact bytes are not compressed-vector storage",
            "Validate actual_index_file_bytes and estimated_f32_payload_bytes labels.",
        ));
    }
    steps.truncate(limit);
    let _ = critical_files;
    steps
}

pub(crate) fn routing_edit_step(
    step: &str,
    evidence_ids: &[String],
    claimability: &str,
    unknowns: &str,
    validation_hint: &str,
) -> Value {
    json!({
        "step": step,
        "evidence_ids": evidence_ids,
        "claimability": claimability,
        "risk": "requires source inspection before edit",
        "unknowns": [unknowns],
        "validation_hint": validation_hint,
    })
}

pub(crate) fn routing_expansion_handles(evidence: &[Value], limit: usize) -> Vec<Value> {
    evidence
        .iter()
        .take(limit)
        .enumerate()
        .map(|(index, item)| {
            json!({
                "handle_id": format!("routing-evidence-{}", index + 1),
                "role": item.get("role").cloned().unwrap_or_else(|| json!("unknown")),
                "description": format!(
                    "Expand {} evidence around {}",
                    item.get("role").and_then(Value::as_str).unwrap_or("source"),
                    item.get("file").and_then(Value::as_str).unwrap_or("unknown")
                ),
                "estimated_bytes": 2048,
                "evidence_ids": [routing_evidence_id(item)],
                "safety_label": "opaque_future_expansion_handle",
                "claimability": item.get("claimability").cloned().unwrap_or_else(|| json!({"claimable_as": [], "not_claimable_as": ["graph_relation_proof"]})),
                "valid_for_db_passport": true,
                "expansion_command_available": false,
            })
        })
        .collect()
}

pub(crate) fn routing_retrieval_plan_summary(
    retrieval_plan: &codegraph_query::RetrievalPlan,
    evidence: &[Value],
    budgets: RoutingPacketBudgets,
) -> Value {
    let query_atoms = retrieval_plan
        .query_atoms
        .iter()
        .map(|atom| {
            let matched = evidence
                .iter()
                .filter(|item| item.get("role").and_then(Value::as_str) == Some(atom.role.as_str()))
                .collect::<Vec<_>>();
            let candidate_sources = matched
                .iter()
                .flat_map(|item| routing_string_values(item, "candidate_sources"))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            json!({
                "role": atom.role,
                "query_text": atom.query_text,
                "path_hints": atom.path_hints,
                "file_kind_hints": atom.file_kind_hints,
                "evidence_role_filter": atom.evidence_role_filter,
                "candidate_source_preference": atom.candidate_source_preference,
                "max_candidates": atom.max_candidates,
                "why": atom.why,
                "expected_signal": atom.expected_signal,
                "proof_expectation": atom.proof_expectation,
                "matched_candidate_count": matched.len(),
                "selected_evidence_ids": matched.iter().map(|item| routing_evidence_id(item)).take(atom.max_candidates.min(3)).collect::<Vec<_>>(),
                "candidate_sources_seen": candidate_sources,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "plan_id": retrieval_plan.plan_id,
        "task_intent_id": retrieval_plan.task_intent_id,
        "profile_id": retrieval_plan.profile_id,
        "query_atoms": query_atoms,
        "candidate_branches": retrieval_plan.candidate_branches,
        "role_budget": retrieval_plan.role_budget,
        "proof_attempt_policy": retrieval_plan.proof_attempt_policy,
        "fallback_policy": retrieval_plan.fallback_policy,
        "max_candidates": retrieval_plan.max_candidates,
        "max_files": retrieval_plan.max_files,
        "max_snippets": retrieval_plan.max_snippets,
        "max_output_bytes": retrieval_plan.max_output_bytes,
        "explain_level": retrieval_plan.explain_level,
        "packet_role_budget": budgets.critical_files_budget,
    })
}

pub(crate) fn routing_agent_investigation_layer_json(
    task_kind: &str,
    micro_flow_handles: &[Value],
    micro_flow_packet_summary: &Value,
    language_capability_plan: &Value,
    graph_proof: bool,
) -> Value {
    let handle_refs = micro_flow_handles
        .iter()
        .take(4)
        .map(|handle| {
            json!({
                "handle_id": handle.get("handle_id").cloned().unwrap_or(Value::Null),
                "packet_id": handle.get("packet_id").cloned().unwrap_or(Value::Null),
                "file": handle.get("file").cloned().unwrap_or(Value::Null),
                "function_identity": handle.get("function_identity").cloned().unwrap_or(Value::Null),
                "proof_status": handle.get("proof_status").cloned().unwrap_or(Value::Null),
                "proof_strength": handle.get("proof_strength").cloned().unwrap_or(Value::Null),
                "unknown_count": handle.get("unknown_count").cloned().unwrap_or_else(|| json!(0)),
                "omitted_count": handle.get("omitted_count").cloned().unwrap_or_else(|| json!(0)),
                "expansion_handle": handle.get("expansion_handle").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "layer": "agent_investigation",
        "task_kind": task_kind,
        "micro_flow_handles": handle_refs,
        "micro_flow_handle_count": micro_flow_handles.len(),
        "micro_flow_packet_summary": micro_flow_packet_summary,
        "language_capability_plan": {
            "status": language_capability_plan.get("status").cloned().unwrap_or_else(|| json!("unknown")),
            "languages": language_capability_plan.get("languages").cloned().unwrap_or_else(|| json!([])),
            "source_role_counts": language_capability_plan.get("source_role_counts").cloned().unwrap_or_else(|| json!({})),
            "capability_status_values": language_capability_plan.get("capability_status_values").cloned().unwrap_or_else(|| json!([])),
            "resolver_compiler_availability": language_capability_plan.get("resolver_compiler_availability").cloned().unwrap_or(Value::Null),
            "unknown_dynamic_risks": language_capability_plan.get("unknown_dynamic_risks").cloned().unwrap_or_else(|| json!([])),
            "unsupported_relations": language_capability_plan.get("unsupported_relations").cloned().unwrap_or_else(|| json!([])),
            "local_flow_packet_boundary": language_capability_plan.get("local_flow_packet_boundary").cloned().unwrap_or(Value::Null),
            "proof_boundary": language_capability_plan.get("proof_boundary").cloned().unwrap_or(Value::Null),
        },
        "usable_for": [
            "trace_boundary",
            "explain_behavior",
            "verify_claim",
            "plan_change",
            "validate_change"
        ],
        "proof_boundary": {
            "packet_handles_do_not_create_proof": true,
            "handle_can_support_investigation": !micro_flow_handles.is_empty(),
            "graph_proof_available": graph_proof,
            "packet_proof_cannot_prove_runtime_or_external_behavior": true,
            "candidate_evidence_not_raised_to_proof": true,
            "capability_metadata_does_not_create_graph_proof": true,
            "unsupported_relations_are_not_blockers": true,
            "non_typescript_packet_handles_are_not_proof": true,
        },
        "expansion_required_for_dict_v1_body": true,
        "ordered_steps_audit_only": true,
    })
}

pub(crate) fn routing_claimability_json(
    lifecycle_claimable: bool,
    db_lifecycle_read: &Value,
    proof_status: &str,
    graph_proof: bool,
    evidence_status: &str,
    text_evidence_available: bool,
) -> Value {
    let diagnostic_only = db_lifecycle_read
        .get("diagnostic_only")
        .and_then(Value::as_bool)
        .unwrap_or(!lifecycle_claimable);
    let claimable =
        lifecycle_claimable && !diagnostic_only && (graph_proof || text_evidence_available);
    let claimable_as = if graph_proof && claimable {
        vec!["graph_relation_proof", "source_text_existence"]
    } else if text_evidence_available && claimable {
        vec!["source_text_existence"]
    } else {
        Vec::new()
    };
    json!({
        "claimable": claimable,
        "diagnostic_only": diagnostic_only,
        "claimable_as": claimable_as,
        "not_claimable_as": if graph_proof && claimable { Vec::<&str>::new() } else { vec!["graph_relation_proof"] },
        "proof_status": proof_status,
        "graph_proof": graph_proof,
        "evidence_status": evidence_status,
        "lifecycle_blocker": db_lifecycle_read.get("reason").or_else(|| db_lifecycle_read.get("status")).cloned().unwrap_or(Value::Null),
    })
}

pub(crate) fn routing_deterministic_sentences(
    task_kind: &str,
    task_intent: &Value,
    graph_proof: bool,
    proof_path_count: usize,
    text_evidence: &[Value],
    unknowns: &[Value],
    risks: &[Value],
    critical_files: &[Value],
    artifact_requirements: &[Value],
    db_requirements: &[Value],
    formulas: &[Value],
    db_lifecycle_read: &Value,
) -> Value {
    let signals = task_intent
        .get("signals")
        .and_then(Value::as_array)
        .map(|signals| {
            signals
                .iter()
                .filter_map(Value::as_str)
                .take(4)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|signals| !signals.is_empty())
        .unwrap_or_else(|| "no strong signals".to_string());
    let intent = format!("I classified this as {task_kind} because I found {signals}.");
    let proof = if graph_proof {
        format!("I found {proof_path_count} verified graph proof path(s).")
    } else {
        "I found no verified graph proof path. This packet uses source text evidence only."
            .to_string()
    };
    let implementation_trace = matches!(
        task_kind,
        "implementation_trace"
            | "storage_accounting_trace"
            | "artifact_math_trace"
            | "persistence_path_trace"
            | "indexing_summary_trace"
            | "benchmark_metric_trace"
    )
    .then(|| {
        "I did not find a verified graph proof path, but I found source-navigation evidence for the implementation surface.".to_string()
    });
    let accounting_trace = matches!(
        task_kind,
        "storage_accounting_trace" | "artifact_math_trace" | "benchmark_metric_trace"
    )
    .then(|| "This packet traces implementation accounting, not graph relation proof.".to_string());
    let text_evidence_sentences = text_evidence
        .iter()
        .take(3)
        .map(|evidence| {
            let file = evidence
                .get("file")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let role = evidence
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("source evidence");
            let matched = evidence
                .get("matched_signal")
                .and_then(Value::as_str)
                .unwrap_or("a source text signal");
            format!("{file} is included as {role} because {matched}. This is source text evidence, not graph proof.")
        })
        .collect::<Vec<_>>();
    let unknown_sentences = unknowns
        .iter()
        .take(3)
        .filter_map(|unknown| {
            unknown
                .get("sentence")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    let risk_sentences = risks
        .iter()
        .take(3)
        .filter_map(|risk| {
            risk.get("sentence")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    let next_inspection = critical_files.first().map(|file| {
        format!(
            "Inspect {} first because {}.",
            file.get("file")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            file.get("why")
                .and_then(Value::as_str)
                .unwrap_or("it is the top-ranked role-diverse evidence")
        )
    });
    let artifact_db = artifact_requirements
        .first()
        .or_else(|| db_requirements.first())
        .and_then(|requirement| requirement.get("sentence").and_then(Value::as_str))
        .map(str::to_string);
    let diagnostic = db_lifecycle_read
        .get("diagnostic_only")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        .then(|| {
            format!(
                "This packet is diagnostic_only because {}. Do not use it as claimable evidence.",
                db_lifecycle_read
                    .get("reason")
                    .or_else(|| db_lifecycle_read.get("status"))
                    .and_then(Value::as_str)
                    .unwrap_or("lifecycle_blocker")
            )
        });
    let vector_metric_truthfulness = formulas
        .iter()
        .find(|note| {
            note.get("note_id").and_then(Value::as_str) == Some("vector_metric_truthfulness")
        })
        .and_then(|note| note.get("sentence").and_then(Value::as_str))
        .map(str::to_string);
    json!({
        "intent": intent,
        "proof": proof,
        "implementation_trace": implementation_trace,
        "accounting_trace": accounting_trace,
        "text_evidence": text_evidence_sentences,
        "unknown": unknown_sentences,
        "risk": risk_sentences,
        "next_inspection": next_inspection,
        "artifact_db_inspection": artifact_db,
        "diagnostic": diagnostic,
        "vector_metric_truthfulness": vector_metric_truthfulness,
    })
}
