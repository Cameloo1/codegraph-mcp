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
    let (follow_up_queries, omitted_follow_up_queries) = routing_follow_up_queries(
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
    let unknowns = routing_unknowns(
        task_kind,
        graph_proof,
        evidence_status,
        &artifact_inspection_requirements,
        &db_inspection_requirements,
    );
    let validation_steps = routing_validation_steps(
        task_kind,
        &critical_files,
        &ranked_evidence,
        &artifact_inspection_requirements,
        &formulas_or_accounting_notes,
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

    json!({
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
        "unknowns": unknowns,
        "risks": risks,
        "validation_steps": validation_steps,
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
    })
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
        "source_navigation_is_graph_proof": false,
        "snippet": snippet,
    });
    if let Some(object) = value.as_object_mut() {
        copy_context_candidate_degradation_fields(snippet, object);
    }
    Some(value)
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
            symbols.push(json!({
                "symbol": symbol,
                "evidence_ids": evidence_ids,
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
