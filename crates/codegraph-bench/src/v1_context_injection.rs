//! Benchmark Layer v1 CodeGraph B-arm context injection protocol.
//!
//! This module defines the compact context packet and attribution fields used by
//! the same-agent A/B harness. It is intentionally conservative: candidate,
//! text, and source-navigation evidence stay non-proof, stale or unavailable
//! graph state cannot receive attribution, and patch success alone is never
//! counted as CodeGraph help.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{BenchResult, BenchmarkError, V1PatchTask};

pub const V1_CONTEXT_INJECTION_SCHEMA_VERSION: u32 = 1;
pub const V1_CONTEXT_INJECTION_NAME: &str = "benchmark_v1_codegraph_context_injection";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1CodeGraphContextBudget {
    pub max_bytes: usize,
    pub max_tokens_estimate: usize,
}

pub fn default_v1_codegraph_context_budget() -> V1CodeGraphContextBudget {
    V1CodeGraphContextBudget {
        max_bytes: 6_000,
        max_tokens_estimate: 1_500,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum V1CodeGraphContextScenario {
    FreshCandidateOnly,
    CandidateOnlyNoProof,
    StaleGraphDb,
    Timeout,
    GraphProof,
    Unavailable,
}

impl V1CodeGraphContextScenario {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FreshCandidateOnly => "fresh_candidate_only",
            Self::CandidateOnlyNoProof => "candidate_only_no_proof",
            Self::StaleGraphDb => "stale_graph_db",
            Self::Timeout => "timeout",
            Self::GraphProof => "graph_proof",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1EvidenceItem {
    pub evidence_kind: String,
    pub label: String,
    pub content: String,
    pub file_hint: Option<String>,
    pub symbol_hint: Option<String>,
    pub graph_proof: bool,
    pub freshness: String,
    pub source_bound: bool,
    pub proof_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1ProofPath {
    pub path_id: String,
    pub status: String,
    pub summary: String,
    pub files: Vec<String>,
    pub symbols: Vec<String>,
    pub graph_proof: bool,
    pub source_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1CodeGraphContextPacket {
    pub schema_version: u32,
    pub protocol: String,
    pub task_id: String,
    pub codegraph_context_available: bool,
    pub codegraph_context_status: String,
    pub profile_db_status: String,
    pub db_claimable: bool,
    pub graph_proof_available: bool,
    pub candidate_only_available: bool,
    pub stale_evidence_present: bool,
    pub context_packet_bytes: usize,
    pub context_packet_tokens_estimate: usize,
    pub critical_files: Vec<String>,
    pub critical_symbols: Vec<String>,
    pub proof_paths: Vec<V1ProofPath>,
    pub text_evidence: Vec<V1EvidenceItem>,
    pub source_navigation_evidence: Vec<V1EvidenceItem>,
    pub candidate_evidence: Vec<V1EvidenceItem>,
    pub unknowns: Vec<String>,
    pub risks: Vec<String>,
    pub validation_steps: Vec<String>,
    pub recovery_commands: Vec<String>,
    pub proof_boundary_summary: String,
    pub graph_proof_overclaim_count: u64,
    pub no_proof_path_found_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1CodeGraphAttribution {
    pub codegraph_context_available: bool,
    pub codegraph_context_used_by_agent: bool,
    pub codegraph_context_cited_in_plan: bool,
    pub codegraph_context_files_touched: Vec<String>,
    pub codegraph_context_aligned_with_patch: bool,
    pub codegraph_context_preceded_correct_edit: bool,
    pub codegraph_context_warning_ignored: bool,
    pub codegraph_context_attribution_valid: bool,
    pub attribution_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1CodeGraphContextInjection {
    pub schema_version: u32,
    pub protocol: String,
    pub arm: String,
    pub codegraph_tools_available_to_agent: bool,
    pub normal_rg_search_edit_test_tools_available: bool,
    pub prompt_packet_injected: bool,
    pub packet: Option<V1CodeGraphContextPacket>,
    pub attribution: V1CodeGraphAttribution,
}

pub fn build_v1_codegraph_context_injection(
    task: &V1PatchTask,
    b_arm: bool,
    scenario: V1CodeGraphContextScenario,
    budget: V1CodeGraphContextBudget,
) -> BenchResult<V1CodeGraphContextInjection> {
    let packet = if b_arm {
        Some(build_packet(task, scenario, budget)?)
    } else {
        None
    };
    let attribution = match packet.as_ref() {
        Some(packet) => evaluate_v1_codegraph_attribution(packet, "", ""),
        None => V1CodeGraphAttribution {
            codegraph_context_available: false,
            codegraph_context_used_by_agent: false,
            codegraph_context_cited_in_plan: false,
            codegraph_context_files_touched: Vec::new(),
            codegraph_context_aligned_with_patch: false,
            codegraph_context_preceded_correct_edit: false,
            codegraph_context_warning_ignored: false,
            codegraph_context_attribution_valid: false,
            attribution_status: "invalid_unavailable".to_string(),
        },
    };
    Ok(V1CodeGraphContextInjection {
        schema_version: V1_CONTEXT_INJECTION_SCHEMA_VERSION,
        protocol: V1_CONTEXT_INJECTION_NAME.to_string(),
        arm: if b_arm {
            "rg_plus_codegraph"
        } else {
            "rg_only"
        }
        .to_string(),
        codegraph_tools_available_to_agent: b_arm,
        normal_rg_search_edit_test_tools_available: true,
        prompt_packet_injected: b_arm && packet.is_some(),
        packet,
        attribution,
    })
}

pub fn render_v1_codegraph_context_prompt(
    packet: &V1CodeGraphContextPacket,
) -> BenchResult<String> {
    let packet_json = serde_json::to_string_pretty(packet)
        .map_err(|error| BenchmarkError::Parse(error.to_string()))?;
    Ok(format!(
        "## CodeGraph Context Packet\n\nThis packet is available only in the B arm. Candidate, text, and source-navigation evidence are not graph proof. Use graph/source verification only for graph proof, and do not infer CodeGraph attribution from patch success.\n\n```json\n{packet_json}\n```\n"
    ))
}

pub fn evaluate_v1_codegraph_attribution(
    packet: &V1CodeGraphContextPacket,
    agent_plan_or_transcript: &str,
    patch_diff: &str,
) -> V1CodeGraphAttribution {
    let plan_lower = agent_plan_or_transcript.to_ascii_lowercase();
    let codegraph_context_used_by_agent =
        packet.codegraph_context_available && plan_lower.contains("codegraph");
    let codegraph_context_cited_in_plan = codegraph_context_used_by_agent
        && packet
            .evidence_labels()
            .iter()
            .any(|label| plan_lower.contains(&label.to_ascii_lowercase()));
    let touched_files = touched_context_files(packet, patch_diff);
    let codegraph_context_aligned_with_patch = !touched_files.is_empty();
    let codegraph_context_warning_ignored = packet.stale_evidence_present
        && codegraph_context_used_by_agent
        && !plan_lower.contains("stale");
    let codegraph_context_preceded_correct_edit =
        codegraph_context_cited_in_plan && codegraph_context_aligned_with_patch;
    let codegraph_context_attribution_valid = packet.codegraph_context_available
        && packet.db_claimable
        && packet.graph_proof_available
        && codegraph_context_preceded_correct_edit
        && !codegraph_context_warning_ignored;
    let attribution_status =
        if !packet.codegraph_context_available || packet.codegraph_context_status == "timed_out" {
            "invalid_unavailable"
        } else if packet.stale_evidence_present || stale_profile_status(&packet.profile_db_status) {
            "invalid_stale"
        } else if !codegraph_context_used_by_agent || !codegraph_context_cited_in_plan {
            "invalid_unused"
        } else if !codegraph_context_aligned_with_patch {
            "invalid_misaligned"
        } else if codegraph_context_attribution_valid {
            "valid"
        } else {
            "unknown"
        };
    V1CodeGraphAttribution {
        codegraph_context_available: packet.codegraph_context_available,
        codegraph_context_used_by_agent,
        codegraph_context_cited_in_plan,
        codegraph_context_files_touched: touched_files,
        codegraph_context_aligned_with_patch,
        codegraph_context_preceded_correct_edit,
        codegraph_context_warning_ignored,
        codegraph_context_attribution_valid,
        attribution_status: attribution_status.to_string(),
    }
}

fn stale_profile_status(status: &str) -> bool {
    matches!(
        status,
        "repo_head_mismatch" | "repo_root_mismatch" | "schema_mismatch" | "foreign_db"
    )
}

pub fn validate_v1_codegraph_packet_budget(
    packet: &V1CodeGraphContextPacket,
    budget: V1CodeGraphContextBudget,
) -> BenchResult<()> {
    if packet.context_packet_bytes > budget.max_bytes {
        return Err(BenchmarkError::Validation(format!(
            "CodeGraph context packet exceeded byte budget: {} > {}",
            packet.context_packet_bytes, budget.max_bytes
        )));
    }
    if packet.context_packet_tokens_estimate > budget.max_tokens_estimate {
        return Err(BenchmarkError::Validation(format!(
            "CodeGraph context packet exceeded token estimate budget: {} > {}",
            packet.context_packet_tokens_estimate, budget.max_tokens_estimate
        )));
    }
    Ok(())
}

pub fn v1_context_injection_to_value(
    injection: &V1CodeGraphContextInjection,
) -> BenchResult<Value> {
    serde_json::to_value(injection).map_err(|error| BenchmarkError::Parse(error.to_string()))
}

impl V1CodeGraphContextPacket {
    fn evidence_labels(&self) -> Vec<String> {
        self.text_evidence
            .iter()
            .chain(self.source_navigation_evidence.iter())
            .chain(self.candidate_evidence.iter())
            .map(|evidence| evidence.label.clone())
            .chain(self.proof_paths.iter().map(|path| path.path_id.clone()))
            .collect()
    }
}

fn build_packet(
    task: &V1PatchTask,
    scenario: V1CodeGraphContextScenario,
    budget: V1CodeGraphContextBudget,
) -> BenchResult<V1CodeGraphContextPacket> {
    let mut packet = base_packet(task, scenario);
    fit_packet_to_budget(&mut packet, budget)?;
    refresh_packet_size(&mut packet)?;
    validate_v1_codegraph_packet_budget(&packet, budget)?;
    Ok(packet)
}

fn base_packet(
    task: &V1PatchTask,
    scenario: V1CodeGraphContextScenario,
) -> V1CodeGraphContextPacket {
    let status = match scenario {
        V1CodeGraphContextScenario::FreshCandidateOnly => "candidate_only_current",
        V1CodeGraphContextScenario::CandidateOnlyNoProof => "no_proof_path_found",
        V1CodeGraphContextScenario::StaleGraphDb => "candidate_only_stale_graph_db",
        V1CodeGraphContextScenario::Timeout => "timed_out",
        V1CodeGraphContextScenario::GraphProof => "graph_proof_available",
        V1CodeGraphContextScenario::Unavailable => "unavailable",
    };
    let profile_db_status = match scenario {
        V1CodeGraphContextScenario::StaleGraphDb => "repo_head_mismatch",
        V1CodeGraphContextScenario::Timeout => "timeout",
        V1CodeGraphContextScenario::GraphProof => "claimable",
        V1CodeGraphContextScenario::Unavailable => "missing",
        _ => "not_claimable_protocol_scaffold",
    };
    let graph_proof_available = scenario == V1CodeGraphContextScenario::GraphProof;
    let codegraph_context_available = scenario != V1CodeGraphContextScenario::Unavailable
        && scenario != V1CodeGraphContextScenario::Timeout;
    let candidate_only_available = codegraph_context_available && !graph_proof_available;
    let critical_files = safe_visible_file_hints(task);
    let critical_symbols = safe_visible_symbol_hints(task);
    V1CodeGraphContextPacket {
        schema_version: V1_CONTEXT_INJECTION_SCHEMA_VERSION,
        protocol: V1_CONTEXT_INJECTION_NAME.to_string(),
        task_id: task.task_id.clone(),
        codegraph_context_available,
        codegraph_context_status: status.to_string(),
        profile_db_status: profile_db_status.to_string(),
        db_claimable: graph_proof_available,
        graph_proof_available,
        candidate_only_available,
        stale_evidence_present: scenario == V1CodeGraphContextScenario::StaleGraphDb,
        context_packet_bytes: 0,
        context_packet_tokens_estimate: 0,
        critical_files: critical_files.clone(),
        critical_symbols: critical_symbols.clone(),
        proof_paths: if graph_proof_available {
            vec![V1ProofPath {
                path_id: "cg-proof-visible-anchor".to_string(),
                status: "graph_source_verified".to_string(),
                summary: "Deterministic protocol fixture proof path over visible task anchors only."
                    .to_string(),
                files: critical_files.clone(),
                symbols: critical_symbols.clone(),
                graph_proof: true,
                source_verified: true,
            }]
        } else {
            Vec::new()
        },
        text_evidence: text_evidence(task, scenario),
        source_navigation_evidence: source_navigation_evidence(task, scenario),
        candidate_evidence: candidate_evidence(task, scenario),
        unknowns: unknowns_for_scenario(scenario),
        risks: risks_for_scenario(scenario),
        validation_steps: validation_steps_for_scenario(scenario),
        recovery_commands: recovery_commands_for_scenario(scenario),
        proof_boundary_summary: "Only graph/source verification is graph proof. Candidate, text, source-navigation, vector, RTDS, and benchmark-provider evidence remain non-graph-proof. Patch success does not prove CodeGraph helped.".to_string(),
        graph_proof_overclaim_count: 0,
        no_proof_path_found_count: u64::from(!graph_proof_available),
    }
}

fn text_evidence(task: &V1PatchTask, scenario: V1CodeGraphContextScenario) -> Vec<V1EvidenceItem> {
    if matches!(
        scenario,
        V1CodeGraphContextScenario::Timeout | V1CodeGraphContextScenario::Unavailable
    ) {
        return Vec::new();
    }
    task.visible_query_terms
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, term)| V1EvidenceItem {
            evidence_kind: "text_evidence".to_string(),
            label: format!("cg-text-visible-term-{}", index + 1),
            content: truncate(term, 180),
            file_hint: None,
            symbol_hint: None,
            graph_proof: false,
            freshness: freshness_label(scenario),
            source_bound: true,
            proof_label: "not_graph_proof".to_string(),
        })
        .collect()
}

fn source_navigation_evidence(
    task: &V1PatchTask,
    scenario: V1CodeGraphContextScenario,
) -> Vec<V1EvidenceItem> {
    if matches!(
        scenario,
        V1CodeGraphContextScenario::Timeout | V1CodeGraphContextScenario::Unavailable
    ) {
        return Vec::new();
    }
    safe_visible_file_hints(task)
        .into_iter()
        .take(3)
        .enumerate()
        .map(|(index, hint)| V1EvidenceItem {
            evidence_kind: "source_navigation_evidence".to_string(),
            label: format!("cg-source-nav-visible-hint-{}", index + 1),
            content: truncate(&hint, 180),
            file_hint: Some(hint),
            symbol_hint: None,
            graph_proof: false,
            freshness: freshness_label(scenario),
            source_bound: true,
            proof_label: "not_graph_proof".to_string(),
        })
        .collect()
}

fn candidate_evidence(
    task: &V1PatchTask,
    scenario: V1CodeGraphContextScenario,
) -> Vec<V1EvidenceItem> {
    if matches!(
        scenario,
        V1CodeGraphContextScenario::Timeout | V1CodeGraphContextScenario::Unavailable
    ) {
        return Vec::new();
    }
    safe_visible_symbol_hints(task)
        .into_iter()
        .take(3)
        .enumerate()
        .map(|(index, symbol)| V1EvidenceItem {
            evidence_kind: "candidate_evidence".to_string(),
            label: format!("cg-candidate-visible-symbol-{}", index + 1),
            content: truncate(&symbol, 180),
            file_hint: None,
            symbol_hint: Some(symbol),
            graph_proof: false,
            freshness: freshness_label(scenario),
            source_bound: true,
            proof_label: "not_graph_proof".to_string(),
        })
        .collect()
}

fn unknowns_for_scenario(scenario: V1CodeGraphContextScenario) -> Vec<String> {
    match scenario {
        V1CodeGraphContextScenario::GraphProof => vec![
            "Attribution is still invalid unless the agent cites and aligns with this context."
                .to_string(),
        ],
        V1CodeGraphContextScenario::Timeout => vec![
            "CodeGraph context generation timed out before a claimable packet was produced."
                .to_string(),
        ],
        V1CodeGraphContextScenario::Unavailable => {
            vec!["CodeGraph context was unavailable for this run.".to_string()]
        }
        _ => {
            vec!["No claimable graph proof path is available in this scaffold packet.".to_string()]
        }
    }
}

fn risks_for_scenario(scenario: V1CodeGraphContextScenario) -> Vec<String> {
    match scenario {
        V1CodeGraphContextScenario::StaleGraphDb => vec![
            "Graph DB is stale/non-claimable; no graph proof may be injected as fresh.".to_string(),
        ],
        V1CodeGraphContextScenario::Timeout => {
            vec!["Attribution must be invalid because CodeGraph context timed out.".to_string()]
        }
        V1CodeGraphContextScenario::GraphProof => {
            vec!["Patch success alone still does not establish CodeGraph attribution.".to_string()]
        }
        _ => vec![
            "Candidate/text/source-navigation evidence may guide search but is not graph proof."
                .to_string(),
        ],
    }
}

fn validation_steps_for_scenario(scenario: V1CodeGraphContextScenario) -> Vec<String> {
    let mut steps = vec![
        "Run normal task tests; CodeGraph validation never replaces tests.".to_string(),
        "Keep hidden evaluator gold out of prompts and context-provider queries.".to_string(),
        "Score CodeGraph attribution separately from patch success.".to_string(),
    ];
    if scenario == V1CodeGraphContextScenario::GraphProof {
        steps.push("Verify graph/source proof path freshness before attribution.".to_string());
    }
    steps
}

fn recovery_commands_for_scenario(scenario: V1CodeGraphContextScenario) -> Vec<String> {
    match scenario {
        V1CodeGraphContextScenario::StaleGraphDb => vec![
            "target/release/codegraph-mcp.exe agent-use status --repo <repo> --json".to_string(),
            "target/release/codegraph-mcp.exe agent-use index --repo <repo> --json".to_string(),
        ],
        V1CodeGraphContextScenario::Timeout => vec![
            "Rerun CodeGraph context prep with a larger explicit timeout or mark attribution invalid_unavailable.".to_string(),
        ],
        V1CodeGraphContextScenario::Unavailable => vec![
            "Restore the production agent-use profile before enabling B-arm attribution.".to_string(),
        ],
        _ => vec![
            "target/release/codegraph-mcp.exe agent-use context-pack --repo <repo> --json"
                .to_string(),
        ],
    }
}

fn freshness_label(scenario: V1CodeGraphContextScenario) -> String {
    match scenario {
        V1CodeGraphContextScenario::StaleGraphDb => {
            "current_source_bound_candidate_stale_graph_rejected".to_string()
        }
        V1CodeGraphContextScenario::GraphProof => "graph_source_verified".to_string(),
        _ => "current_source_bound_candidate".to_string(),
    }
}

fn safe_visible_file_hints(task: &V1PatchTask) -> Vec<String> {
    let mut values = task
        .visible_file_hints
        .iter()
        .filter(|hint| !hint.trim().is_empty())
        .map(|hint| truncate(hint, 120))
        .collect::<Vec<_>>();
    if values.is_empty() {
        values.push("visible_task_surface".to_string());
    }
    values
}

fn safe_visible_symbol_hints(task: &V1PatchTask) -> Vec<String> {
    task.visible_symbol_hints
        .iter()
        .filter(|hint| !hint.trim().is_empty())
        .map(|hint| truncate(hint, 120))
        .collect()
}

fn fit_packet_to_budget(
    packet: &mut V1CodeGraphContextPacket,
    budget: V1CodeGraphContextBudget,
) -> BenchResult<()> {
    refresh_packet_size(packet)?;
    if packet.context_packet_bytes <= budget.max_bytes
        && packet.context_packet_tokens_estimate <= budget.max_tokens_estimate
    {
        return Ok(());
    }
    packet.text_evidence.truncate(2);
    packet.source_navigation_evidence.truncate(2);
    packet.candidate_evidence.truncate(2);
    for evidence in packet
        .text_evidence
        .iter_mut()
        .chain(packet.source_navigation_evidence.iter_mut())
        .chain(packet.candidate_evidence.iter_mut())
    {
        evidence.content = truncate(&evidence.content, 96);
    }
    refresh_packet_size(packet)?;
    if packet.context_packet_bytes <= budget.max_bytes
        && packet.context_packet_tokens_estimate <= budget.max_tokens_estimate
    {
        return Ok(());
    }
    packet.text_evidence.truncate(1);
    packet.source_navigation_evidence.truncate(1);
    packet.candidate_evidence.clear();
    refresh_packet_size(packet)
}

fn refresh_packet_size(packet: &mut V1CodeGraphContextPacket) -> BenchResult<()> {
    let bytes = serde_json::to_vec(packet)
        .map_err(|error| BenchmarkError::Parse(error.to_string()))?
        .len();
    packet.context_packet_bytes = bytes;
    packet.context_packet_tokens_estimate = estimate_tokens(bytes);
    let bytes = serde_json::to_vec(packet)
        .map_err(|error| BenchmarkError::Parse(error.to_string()))?
        .len();
    packet.context_packet_bytes = bytes;
    packet.context_packet_tokens_estimate = estimate_tokens(bytes);
    Ok(())
}

fn estimate_tokens(bytes: usize) -> usize {
    bytes.div_ceil(4)
}

fn touched_context_files(packet: &V1CodeGraphContextPacket, patch_diff: &str) -> Vec<String> {
    packet
        .critical_files
        .iter()
        .filter(|file| {
            let trimmed = file.trim();
            !trimmed.is_empty()
                && (patch_diff.contains(&format!(" b/{trimmed}"))
                    || patch_diff.contains(&format!(" a/{trimmed}"))
                    || patch_diff.contains(trimmed))
        })
        .cloned()
        .collect()
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for character in value.chars().take(max_chars) {
        output.push(character);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_v1_patch_task_registry, v1_registry::v1_registry_path};
    use std::path::{Path, PathBuf};

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    }

    fn task(task_id: &str) -> V1PatchTask {
        load_v1_patch_task_registry(&v1_registry_path(&workspace_root()))
            .expect("registry")
            .tasks
            .into_iter()
            .find(|task| task.task_id == task_id)
            .expect("task")
    }

    #[test]
    fn v1_codegraph_packet_has_required_proof_labels() {
        let packet = build_packet(
            &task("local_wrong_file_trap_discount"),
            V1CodeGraphContextScenario::FreshCandidateOnly,
            default_v1_codegraph_context_budget(),
        )
        .expect("packet");
        assert!(packet.codegraph_context_available);
        assert!(!packet.graph_proof_available);
        assert_eq!(packet.graph_proof_overclaim_count, 0);
        assert!(packet
            .text_evidence
            .iter()
            .all(|evidence| !evidence.graph_proof));
        assert!(packet.proof_boundary_summary.contains("Only graph/source"));
    }

    #[test]
    fn v1_stale_graph_db_injects_no_claimable_graph_proof() {
        let packet = build_packet(
            &task("local_wrong_file_trap_discount"),
            V1CodeGraphContextScenario::StaleGraphDb,
            default_v1_codegraph_context_budget(),
        )
        .expect("packet");
        assert_eq!(packet.profile_db_status, "repo_head_mismatch");
        assert!(!packet.db_claimable);
        assert!(!packet.graph_proof_available);
        assert!(packet.stale_evidence_present);
    }

    #[test]
    fn v1_candidate_only_context_stays_non_graph_proof() {
        let packet = build_packet(
            &task("local_no_proof_fallback_runbook"),
            V1CodeGraphContextScenario::CandidateOnlyNoProof,
            default_v1_codegraph_context_budget(),
        )
        .expect("packet");
        assert_eq!(packet.codegraph_context_status, "no_proof_path_found");
        assert!(packet.candidate_only_available);
        assert!(!packet.graph_proof_available);
        assert_eq!(packet.no_proof_path_found_count, 1);
    }

    #[test]
    fn v1_codegraph_timeout_invalidates_attribution() {
        let packet = build_packet(
            &task("local_wrong_file_trap_discount"),
            V1CodeGraphContextScenario::Timeout,
            default_v1_codegraph_context_budget(),
        )
        .expect("packet");
        let attribution = evaluate_v1_codegraph_attribution(
            &packet,
            "Plan cites CodeGraph evidence.",
            "diff --git a/visible_task_surface b/visible_task_surface\n",
        );
        assert_eq!(attribution.attribution_status, "invalid_unavailable");
        assert!(!attribution.codegraph_context_attribution_valid);
    }

    #[test]
    fn v1_plan_citation_and_patch_alignment_are_detected() {
        let mut packet = build_packet(
            &task("local_wrong_file_trap_discount"),
            V1CodeGraphContextScenario::GraphProof,
            default_v1_codegraph_context_budget(),
        )
        .expect("packet");
        packet.critical_files = vec!["src/visible.py".to_string()];
        packet.proof_paths[0].path_id = "cg-proof-visible-anchor".to_string();
        let attribution = evaluate_v1_codegraph_attribution(
            &packet,
            "Plan: use CodeGraph cg-proof-visible-anchor before editing.",
            "diff --git a/src/visible.py b/src/visible.py\n+change\n",
        );
        assert!(attribution.codegraph_context_cited_in_plan);
        assert_eq!(
            attribution.codegraph_context_files_touched,
            vec!["src/visible.py".to_string()]
        );
        assert!(attribution.codegraph_context_aligned_with_patch);
        assert_eq!(attribution.attribution_status, "valid");
    }

    #[test]
    fn v1_context_budget_is_enforced() {
        let packet = build_packet(
            &task("local_wrong_file_trap_discount"),
            V1CodeGraphContextScenario::FreshCandidateOnly,
            V1CodeGraphContextBudget {
                max_bytes: 2_500,
                max_tokens_estimate: 625,
            },
        )
        .expect("packet");
        assert!(packet.context_packet_bytes <= 2_500);
        assert!(packet.context_packet_tokens_estimate <= 625);
    }
}
