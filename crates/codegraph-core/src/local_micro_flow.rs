use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    decide_local_micro_flow_proof_eligibility, normalize_repo_relative_path,
    stable_micro_packet_id, LocalMicroFlowPacketLinterClass,
    LocalMicroFlowProofEligibilityDecision, LocalMicroFlowProofEligibilityInput, MicroEdgeKind,
    MicroExactness, MicroNodeKind, MicroPacketIdentityInput, MicroSourceRole, ProofLadderLevel,
    SourceSpan, MVP4_2B_LOCAL_FLOWS_TO_EXTRACTION_VERSION, MVP4_2B_LOCAL_READS_EXTRACTION_VERSION,
    MVP4_2B_LOCAL_WRITES_EXTRACTION_VERSION, MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION,
    MVP4_3_LOCAL_MICRO_FLOW_PACKET_EXTRACTION_VERSION, MVP4_3_LOCAL_MICRO_FLOW_PACKET_KIND,
    MVP4_3_LOCAL_MICRO_FLOW_PACKET_PAYLOAD_VERSION, MVP4_3_LOCAL_MICRO_FLOW_PACKET_SCHEMA_VERSION,
    MVP4_3_TYPESCRIPT_FIRST_SLICE_RELATIONS,
};

pub const LOCAL_MICRO_FLOW_AGENT_JSON_SCHEMA_NAME: &str = "local_micro_flow_packet_agent_json";
pub const LOCAL_MICRO_FLOW_AGENT_JSON_SCHEMA_VERSION: u32 = 2;
pub const LOCAL_MICRO_FLOW_AGENT_JSON_PACKET_KIND: &str = "function_local_flow_packet";
pub const LOCAL_MICRO_FLOW_DICT_V1_ENCODING: &str = "dict_v1";
pub const LOCAL_MICRO_FLOW_DICT_V1_CODEC_VERSION: &str = "mvp4.3-dict-v1-codec-v1";
pub const LOCAL_MICRO_FLOW_DICT_V1_STEP_SET_VERSION: &str = "mvp4.3-dict-v1-step-set-v1";
pub const LOCAL_MICRO_FLOW_MAX_LABEL_BYTES: usize = 240;
pub const LOCAL_MICRO_FLOW_MAX_SKELETON_TEXT_BYTES: usize = 512;
pub const LOCAL_MICRO_FLOW_MAX_PACKET_STEPS: usize = 64;
pub const LOCAL_MICRO_FLOW_MAX_PACKET_BODY_BYTES: u32 = 160 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalMicroFlowPacketSupportStatus {
    ExactCapable,
    NotImplemented,
    Unsupported,
    Unknown,
}

impl LocalMicroFlowPacketSupportStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactCapable => "exact_capable",
            Self::NotImplemented => "not_implemented",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }

    pub const fn default_exact_support(self) -> bool {
        matches!(self, Self::ExactCapable)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LocalMicroFlowPacketLanguageCapability {
    pub language: &'static str,
    pub frontend: Option<&'static str>,
    pub packet_kind: &'static str,
    pub activation_status: LocalMicroFlowPacketSupportStatus,
    pub supported_node_kinds: &'static [MicroNodeKind],
    pub supported_edge_kinds: &'static [MicroEdgeKind],
    pub supported_path_patterns: &'static [&'static str],
    pub branch_model_capability: &'static str,
    pub return_path_model_capability: &'static str,
    pub binding_resolver_capability: &'static str,
    pub source_role_policy: &'static str,
    pub gap_classifier: &'static str,
    pub proof_eligibility_classifier: &'static str,
    pub unsupported_reasons: &'static [&'static str],
    pub fixture_ids: &'static [&'static str],
    pub extraction_version: Option<&'static str>,
    pub schema_version: u32,
    pub payload_version: u32,
    pub cap_policy: &'static str,
    pub explain_audit_rendering: &'static str,
}

impl LocalMicroFlowPacketLanguageCapability {
    pub const fn not_implemented(language: &'static str) -> Self {
        Self {
            language,
            frontend: None,
            packet_kind: MVP4_3_LOCAL_MICRO_FLOW_PACKET_KIND,
            activation_status: LocalMicroFlowPacketSupportStatus::NotImplemented,
            supported_node_kinds: &[],
            supported_edge_kinds: &[],
            supported_path_patterns: &[],
            branch_model_capability: "not_implemented",
            return_path_model_capability: "not_implemented",
            binding_resolver_capability: "not_implemented",
            source_role_policy: "not_applicable",
            gap_classifier: "unsupported_language_gap_classifier",
            proof_eligibility_classifier: "unsupported_language_no_flow_proof",
            unsupported_reasons: MVP4_3_PACKET_LANGUAGE_NOT_IMPLEMENTED_REASONS,
            fixture_ids: &[],
            extraction_version: None,
            schema_version: MVP4_3_LOCAL_MICRO_FLOW_PACKET_SCHEMA_VERSION,
            payload_version: MVP4_3_LOCAL_MICRO_FLOW_PACKET_PAYLOAD_VERSION,
            cap_policy: "not_applicable",
            explain_audit_rendering: "generic_unsupported_language_status",
        }
    }

    pub fn supports_claimable_packets(self, source_role: MicroSourceRole) -> bool {
        self.activation_status == LocalMicroFlowPacketSupportStatus::ExactCapable
            && source_role == MicroSourceRole::Production
            && self.extraction_version.is_some()
    }
}

const MVP4_3_TYPESCRIPT_PACKET_NODE_KINDS: &[MicroNodeKind] = &[
    MicroNodeKind::FunctionFrame,
    MicroNodeKind::Parameter,
    MicroNodeKind::LocalBinding,
    MicroNodeKind::AssignmentSite,
    MicroNodeKind::ReturnSite,
    MicroNodeKind::CallSite,
    MicroNodeKind::PropertyAccess,
    MicroNodeKind::ValueUse,
];
const MVP4_3_TYPESCRIPT_PACKET_EDGE_KINDS: &[MicroEdgeKind] =
    MVP4_3_TYPESCRIPT_FIRST_SLICE_RELATIONS;
const MVP4_3_TYPESCRIPT_PACKET_PATH_PATTERNS: &[&str] = &[
    "function_local_assignment_chain_to_return",
    "function_local_return_containment_summary",
    "function_local_partial_flow_with_explicit_gaps",
];
const MVP4_3_TYPESCRIPT_PACKET_FIXTURE_IDS: &[&str] = &[
    "ts_packet_param_assignment_return",
    "ts_packet_alias_assignment_chain",
    "ts_packet_branch_return_paths",
    "ts_packet_shadowed_binding_identity",
];
const MVP4_3_PACKET_LANGUAGE_NOT_IMPLEMENTED_REASONS: &[&str] = &[
    "packet_adapter_not_implemented_for_language",
    "language_specific_micro_node_micro_edge_fixture_backlog_required",
    "no_default_exact_packet_support",
];

pub const MVP4_3_TYPESCRIPT_LOCAL_MICRO_FLOW_PACKET_CAPABILITY:
    LocalMicroFlowPacketLanguageCapability = LocalMicroFlowPacketLanguageCapability {
    language: "typescript",
    frontend: Some("tree-sitter-typescript"),
    packet_kind: MVP4_3_LOCAL_MICRO_FLOW_PACKET_KIND,
    activation_status: LocalMicroFlowPacketSupportStatus::ExactCapable,
    supported_node_kinds: MVP4_3_TYPESCRIPT_PACKET_NODE_KINDS,
    supported_edge_kinds: MVP4_3_TYPESCRIPT_PACKET_EDGE_KINDS,
    supported_path_patterns: MVP4_3_TYPESCRIPT_PACKET_PATH_PATTERNS,
    branch_model_capability: "source_spanned_branch_identity_when_micro_facts_preserve_it",
    return_path_model_capability: "source_spanned_return_path_identity_from_ReturnSite_edges",
    binding_resolver_capability: "resolver_proven_local_reads_writes_and_shadowing",
    source_role_policy: "production_only",
    gap_classifier: "typescript_function_local_micro_flow_gap_classifier",
    proof_eligibility_classifier: "dict_v1_lossless_function_local_flow_proof_classifier",
    unsupported_reasons: &[],
    fixture_ids: MVP4_3_TYPESCRIPT_PACKET_FIXTURE_IDS,
    extraction_version: Some(MVP4_3_LOCAL_MICRO_FLOW_PACKET_EXTRACTION_VERSION),
    schema_version: MVP4_3_LOCAL_MICRO_FLOW_PACKET_SCHEMA_VERSION,
    payload_version: MVP4_3_LOCAL_MICRO_FLOW_PACKET_PAYLOAD_VERSION,
    cap_policy: "mvp4_3_local_micro_flow_packet_caps",
    explain_audit_rendering: "dict_v1_with_ordered_steps_audit_expansion",
};

pub const MVP4_3_LOCAL_MICRO_FLOW_PACKET_LANGUAGE_CAPABILITIES:
    &[LocalMicroFlowPacketLanguageCapability] = &[
    MVP4_3_TYPESCRIPT_LOCAL_MICRO_FLOW_PACKET_CAPABILITY,
    LocalMicroFlowPacketLanguageCapability::not_implemented("javascript"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("jsx"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("tsx"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("rust"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("python"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("go"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("c"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("cpp"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("c_cpp"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("java"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("csharp"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("ruby"),
    LocalMicroFlowPacketLanguageCapability::not_implemented("php"),
];

pub fn mvp4_3_local_micro_flow_packet_language_capability(
    language: &str,
) -> LocalMicroFlowPacketLanguageCapability {
    let normalized = language.trim().to_ascii_lowercase();
    MVP4_3_LOCAL_MICRO_FLOW_PACKET_LANGUAGE_CAPABILITIES
        .iter()
        .copied()
        .find(|capability| capability.language == normalized)
        .unwrap_or_else(|| LocalMicroFlowPacketLanguageCapability::not_implemented("unknown"))
}

pub fn mvp4_3_local_micro_flow_packet_source_supported(
    language: &str,
    source_role: MicroSourceRole,
) -> bool {
    mvp4_3_local_micro_flow_packet_language_capability(language)
        .supports_claimable_packets(source_role)
}

pub fn mvp4_3_local_micro_flow_packet_supported_node_kinds(
    language: &str,
) -> &'static [MicroNodeKind] {
    mvp4_3_local_micro_flow_packet_language_capability(language).supported_node_kinds
}

pub fn mvp4_3_local_micro_flow_packet_supported_edge_kinds(
    language: &str,
) -> &'static [MicroEdgeKind] {
    mvp4_3_local_micro_flow_packet_language_capability(language).supported_edge_kinds
}

pub fn mvp4_3_local_micro_flow_packet_active_languages() -> Vec<&'static str> {
    MVP4_3_LOCAL_MICRO_FLOW_PACKET_LANGUAGE_CAPABILITIES
        .iter()
        .filter(|capability| {
            capability.activation_status == LocalMicroFlowPacketSupportStatus::ExactCapable
        })
        .map(|capability| capability.language)
        .collect()
}

pub fn mvp4_3_default_local_micro_flow_packet_query_language() -> Option<&'static str> {
    mvp4_3_local_micro_flow_packet_active_languages()
        .into_iter()
        .next()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowCanonicalPacket {
    pub packet_id: String,
    pub file: LocalMicroFlowFileRef,
    pub function_identity: LocalMicroFlowFunctionIdentity,
    pub function_span: SourceSpan,
    pub claimability: String,
    pub packet_status: crate::LocalMicroFlowPacketStatus,
    pub proof_strength: ProofLadderLevel,
    pub budget: LocalMicroFlowBudget,
    pub risks_limitations: Vec<String>,
    pub unknown_unsupported_gaps: Vec<LocalMicroFlowGap>,
    pub expansion_handles: Vec<LocalMicroFlowExpansionHandle>,
    pub paths: Vec<LocalMicroFlowCanonicalPath>,
}

impl LocalMicroFlowCanonicalPacket {
    pub fn proof_classification(&self) -> LocalMicroFlowPacketProofClassification {
        classify_local_micro_flow_packet_proof(self)
    }

    pub fn encode_dict_v1(&self) -> Result<DictV1PacketBody, LocalMicroFlowCodecError> {
        DictV1Encoder::encode(self)
    }

    pub fn to_agent_json(
        &self,
        include_ordered_steps: bool,
    ) -> Result<Value, LocalMicroFlowCodecError> {
        let packet_body = self.encode_dict_v1()?;
        let proof_classification = self.proof_classification();
        let ordered_steps = if include_ordered_steps {
            Some(packet_body.to_ordered_steps()?)
        } else {
            None
        };

        let mut value = json!({
            "schema_name": LOCAL_MICRO_FLOW_AGENT_JSON_SCHEMA_NAME,
            "schema_version": LOCAL_MICRO_FLOW_AGENT_JSON_SCHEMA_VERSION,
            "packet_kind": LOCAL_MICRO_FLOW_AGENT_JSON_PACKET_KIND,
            "packet_id": self.packet_id,
            "packet_version": 1,
            "extraction_version": MVP4_3_LOCAL_MICRO_FLOW_PACKET_EXTRACTION_VERSION,
            "step_set_version": LOCAL_MICRO_FLOW_DICT_V1_STEP_SET_VERSION,
            "file": self.file,
            "function_identity": self.function_identity,
            "function_span": self.function_span,
            "claimability": self.claimability,
            "proof_status": proof_status_schema_value(self.packet_status, self.proof_strength),
            "packet_status": self.packet_status.as_str(),
            "proof_strength": proof_ladder_level_str(self.proof_strength),
            "proof_classification": proof_classification,
            "encoding": LOCAL_MICRO_FLOW_DICT_V1_ENCODING,
            "packet_body": packet_body,
            "unknown_unsupported_gaps": self.unknown_unsupported_gaps,
            "risks_limitations": self.risks_limitations,
            "omitted_count": self.budget.omitted_count,
            "truncation_reason": self.budget.truncation_reason,
            "expansion_handles": self.expansion_handles,
            "budget": self.budget,
            "budget_contract": LocalMicroFlowBudgetContract::compact_default(),
            "generation_state": LocalMicroFlowGenerationState::inactive()
        });

        if let Some(ordered_steps) = ordered_steps {
            value
                .as_object_mut()
                .expect("agent packet JSON is an object")
                .insert(
                    "ordered_steps".to_string(),
                    serde_json::to_value(ordered_steps)?,
                );
        }

        validate_local_micro_flow_agent_json_contract(&value)?;
        Ok(value)
    }

    pub fn to_ordered_steps(
        &self,
    ) -> Result<Vec<LocalMicroFlowAuditStep>, LocalMicroFlowCodecError> {
        self.encode_dict_v1()?.to_ordered_steps()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowPersistedFactInput {
    pub repo_relative_path: String,
    pub node_layer_current: bool,
    pub edge_layer_current: bool,
    pub node_cap_omitted_count: u64,
    pub edge_cap_omitted_count: u64,
    pub raw_ast_truth_bypass_count: u64,
    pub nodes: Vec<LocalMicroFlowPersistedNodeFact>,
    pub edges: Vec<LocalMicroFlowPersistedEdgeFact>,
}

impl LocalMicroFlowPersistedFactInput {
    pub fn new(
        repo_relative_path: impl AsRef<str>,
        nodes: Vec<LocalMicroFlowPersistedNodeFact>,
        edges: Vec<LocalMicroFlowPersistedEdgeFact>,
    ) -> Self {
        Self {
            repo_relative_path: normalize_repo_relative_path(repo_relative_path),
            node_layer_current: true,
            edge_layer_current: true,
            node_cap_omitted_count: 0,
            edge_cap_omitted_count: 0,
            raw_ast_truth_bypass_count: 0,
            nodes,
            edges,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowPersistedNodeFact {
    pub micro_node_id: String,
    pub file_id: String,
    pub function_entity_id: Option<String>,
    pub scope_entity_id: Option<String>,
    pub micro_kind: Option<MicroNodeKind>,
    pub raw_micro_kind: String,
    pub symbol: Option<String>,
    pub source_span: Option<SourceSpan>,
    pub exactness: MicroExactness,
    pub provenance_id: Option<String>,
    pub source_role: MicroSourceRole,
    pub language: String,
    pub schema_version: u32,
    pub payload_version: u32,
    pub extraction_version: String,
    pub claimability: String,
    pub lifecycle_binding: String,
}

impl LocalMicroFlowPersistedNodeFact {
    pub fn function_domain(&self) -> Option<&str> {
        self.function_entity_id
            .as_deref()
            .or(self.scope_entity_id.as_deref())
    }

    fn is_claimable(&self) -> bool {
        self.claimability.trim().starts_with("claimable_")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowPersistedEdgeFact {
    pub micro_edge_id: String,
    pub file_id: String,
    pub function_entity_id: Option<String>,
    pub scope_entity_id: Option<String>,
    pub source_micro_node_id: String,
    pub target_micro_node_id: String,
    pub relation_kind: Option<MicroEdgeKind>,
    pub raw_relation_kind: String,
    pub source_span: Option<SourceSpan>,
    pub exactness: MicroExactness,
    pub provenance_id: Option<String>,
    pub source_role: MicroSourceRole,
    pub language: String,
    pub frontend: String,
    pub schema_version: u32,
    pub payload_version: u32,
    pub extraction_version: String,
    pub claimability: String,
    pub lifecycle_binding: String,
}

impl LocalMicroFlowPersistedEdgeFact {
    pub fn function_domain(&self) -> Option<&str> {
        self.function_entity_id
            .as_deref()
            .or(self.scope_entity_id.as_deref())
    }

    fn is_claimable(&self) -> bool {
        self.claimability.trim().starts_with("claimable_")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowPacketCandidateSet {
    pub candidates: Vec<LocalMicroFlowPacketCandidate>,
    pub diagnostics: Vec<LocalMicroFlowPacketCandidateDiagnostic>,
    pub raw_ast_truth_bypass_count: u64,
    pub unsupported_relation_inclusion_count: u64,
    pub missing_endpoint_count: u64,
    pub missing_span_count: u64,
    pub missing_provenance_count: u64,
}

impl LocalMicroFlowPacketCandidateSet {
    pub fn local_flow_packet_rows_emitted(&self) -> u64 {
        0
    }

    pub fn flow_proof_activated(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowPacketCandidate {
    pub packet_id: String,
    pub packet_kind: String,
    pub function_micro_node_id: String,
    pub function_entity_id: Option<String>,
    pub file_id: String,
    pub language: String,
    pub source_role: MicroSourceRole,
    pub packet_status: crate::LocalMicroFlowPacketStatus,
    pub proof_strength: ProofLadderLevel,
    pub node_ref_count: usize,
    pub edge_ref_count: usize,
    pub gap_count: usize,
    pub source_micro_node_extraction_versions: Vec<String>,
    pub source_micro_edge_extraction_versions: Vec<String>,
    pub packet: Option<LocalMicroFlowCanonicalPacket>,
    pub diagnostics: Vec<LocalMicroFlowPacketCandidateDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowPacketCandidateDiagnostic {
    pub diagnostic_kind: String,
    pub severity: String,
    pub fact_id: Option<String>,
    pub message: String,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalMicroFlowProofClassificationState {
    FlowProof,
    PartialMicroFlow,
    GraphRelationProofOnly,
    NoMicroFlowPathFound,
    Unsupported,
    Unknown,
    Truncated,
    Stale,
    Unavailable,
    Corrupt,
    Incompatible,
}

impl LocalMicroFlowProofClassificationState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FlowProof => "flow_proof",
            Self::PartialMicroFlow => "partial_micro_flow",
            Self::GraphRelationProofOnly => "graph_relation_proof_only",
            Self::NoMicroFlowPathFound => "no_micro_flow_path_found",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
            Self::Truncated => "truncated",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
            Self::Corrupt => "corrupt",
            Self::Incompatible => "incompatible",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowPacketProofClassification {
    pub packet_status: crate::LocalMicroFlowPacketStatus,
    pub packet_proof_strength: ProofLadderLevel,
    pub strongest_path_state: LocalMicroFlowProofClassificationState,
    pub strongest_path_proof_strength: ProofLadderLevel,
    pub flow_proof_path_count: usize,
    pub non_flow_path_count: usize,
    pub packet_summary_no_overclaim: bool,
    pub dict_v1_lossless_to_audit_ordered_steps: bool,
    pub path_classifications: Vec<LocalMicroFlowPathProofClassification>,
    pub linter_mapping: LocalMicroFlowProofLinterMapping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowPathProofClassification {
    pub path_id: String,
    pub return_path_id: Option<String>,
    pub classification_state: LocalMicroFlowProofClassificationState,
    pub packet_status: crate::LocalMicroFlowPacketStatus,
    pub proof_strength: ProofLadderLevel,
    pub flow_proof_eligible: bool,
    pub missing_requirements: Vec<String>,
    pub gap_count: usize,
    pub cap_omission_count: usize,
    pub graph_relation_step_count: usize,
    pub flow_step_count: usize,
    pub read_step_count: usize,
    pub write_step_count: usize,
    pub return_step_count: usize,
    pub linter_mapping: LocalMicroFlowProofLinterMapping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowProofLinterMapping {
    pub linter_class: String,
    pub validation_classification: String,
    pub hard_interrupt_available: bool,
    pub must_fix_before_continuing: bool,
    pub recommended_action_kind: String,
}

impl LocalMicroFlowPacketCandidateDiagnostic {
    fn integrity(
        diagnostic_kind: impl Into<String>,
        fact_id: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            diagnostic_kind: diagnostic_kind.into(),
            severity: "packet_integrity".to_string(),
            fact_id,
            message: message.into(),
            recommended_action: "reindex_or_repair_codegraph_micro_flow_dependencies".to_string(),
        }
    }

    fn unsupported(
        diagnostic_kind: impl Into<String>,
        fact_id: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            diagnostic_kind: diagnostic_kind.into(),
            severity: "unsupported_or_unknown".to_string(),
            fact_id,
            message: message.into(),
            recommended_action: "preserve_gap_or_not_applicable_state_without_source_blocker"
                .to_string(),
        }
    }
}

pub fn build_local_micro_flow_packet_candidates_from_persisted_facts(
    input: &LocalMicroFlowPersistedFactInput,
) -> LocalMicroFlowPacketCandidateSet {
    let repo_relative_path = normalize_repo_relative_path(&input.repo_relative_path);
    let mut diagnostics = Vec::new();
    if input.raw_ast_truth_bypass_count > 0 {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "raw_ast_truth_bypass",
            None,
            "packet candidates must be built from persisted micro-node and micro-edge facts only",
        ));
    }

    let mut nodes_by_id: BTreeMap<&str, &LocalMicroFlowPersistedNodeFact> = BTreeMap::new();
    for node in &input.nodes {
        nodes_by_id.insert(node.micro_node_id.as_str(), node);
    }

    let function_frames: Vec<&LocalMicroFlowPersistedNodeFact> = input
        .nodes
        .iter()
        .filter(|node| node.micro_kind == Some(MicroNodeKind::FunctionFrame))
        .collect();

    let mut candidates = Vec::new();
    for function_frame in function_frames {
        let candidate = build_function_packet_candidate(
            input,
            &repo_relative_path,
            &nodes_by_id,
            function_frame,
        );
        diagnostics.extend(candidate.diagnostics.iter().cloned());
        candidates.push(candidate);
    }

    let unsupported_relation_inclusion_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.diagnostic_kind == "unsupported_relation")
        .count() as u64;
    let missing_endpoint_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.diagnostic_kind == "missing_edge_endpoint")
        .count() as u64;
    let missing_span_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.diagnostic_kind == "missing_source_span")
        .count() as u64;
    let missing_provenance_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.diagnostic_kind == "missing_provenance")
        .count() as u64;

    LocalMicroFlowPacketCandidateSet {
        candidates,
        diagnostics,
        raw_ast_truth_bypass_count: input.raw_ast_truth_bypass_count,
        unsupported_relation_inclusion_count,
        missing_endpoint_count,
        missing_span_count,
        missing_provenance_count,
    }
}

fn build_function_packet_candidate<'a>(
    input: &'a LocalMicroFlowPersistedFactInput,
    repo_relative_path: &str,
    nodes_by_id: &BTreeMap<&'a str, &'a LocalMicroFlowPersistedNodeFact>,
    function_frame: &'a LocalMicroFlowPersistedNodeFact,
) -> LocalMicroFlowPacketCandidate {
    let function_domain = function_frame
        .function_domain()
        .unwrap_or(function_frame.micro_node_id.as_str())
        .to_string();
    let function_nodes: Vec<&LocalMicroFlowPersistedNodeFact> = input
        .nodes
        .iter()
        .filter(|node| {
            normalize_repo_relative_path(&node.file_id) == repo_relative_path
                && same_function_domain(node.function_domain(), &function_domain)
        })
        .collect();
    let function_node_ids: BTreeSet<&str> = function_nodes
        .iter()
        .map(|node| node.micro_node_id.as_str())
        .collect();

    let mut candidate_diagnostics = Vec::new();
    if !input.node_layer_current || !input.edge_layer_current {
        candidate_diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "stale_fact_layer",
            Some(function_frame.micro_node_id.clone()),
            "node or edge layer is not current for packet candidate construction",
        ));
    }
    if function_frame.source_span.is_none() {
        candidate_diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "missing_source_span",
            Some(function_frame.micro_node_id.clone()),
            "FunctionFrame endpoint is missing a source span",
        ));
    }
    if !is_supported_packet_source(function_frame) {
        candidate_diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::unsupported(
            "unsupported_source_role_or_language",
            Some(function_frame.micro_node_id.clone()),
            "MVP4.3 first slice supports only languages with an exact packet adapter and production source role",
        ));
    }
    if !function_frame.is_claimable() {
        candidate_diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "non_claimable_function_frame",
            Some(function_frame.micro_node_id.clone()),
            "FunctionFrame endpoint is not claimable",
        ));
    }

    for node in &function_nodes {
        if node.source_span.is_none() {
            candidate_diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
                "missing_source_span",
                Some(node.micro_node_id.clone()),
                "proof-bearing micro-node is missing a source span",
            ));
        }
        if !is_allowed_packet_node(&node.language, node.micro_kind) {
            candidate_diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::unsupported(
                "unsupported_node_kind",
                Some(node.micro_node_id.clone()),
                format!(
                    "node kind {} is not in the MVP4.3 packet input set",
                    node.raw_micro_kind
                ),
            ));
        }
    }

    let mut eligible_edges = Vec::new();
    for edge in &input.edges {
        if normalize_repo_relative_path(&edge.file_id) != repo_relative_path {
            continue;
        }
        let endpoint_in_function = function_node_ids.contains(edge.source_micro_node_id.as_str())
            || function_node_ids.contains(edge.target_micro_node_id.as_str())
            || same_function_domain(edge.function_domain(), &function_domain);
        if !endpoint_in_function {
            continue;
        }
        validate_packet_edge(
            edge,
            nodes_by_id,
            &function_domain,
            &mut candidate_diagnostics,
        );
        if edge_is_eligible(edge, nodes_by_id, &function_domain) {
            eligible_edges.push(edge);
        }
    }

    eligible_edges.sort_by(|left, right| {
        edge_order_key(left)
            .cmp(&edge_order_key(right))
            .then_with(|| left.micro_edge_id.cmp(&right.micro_edge_id))
    });

    let has_integrity_failure = candidate_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == "packet_integrity");
    let unsupported = candidate_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == "unsupported_or_unknown")
        || !is_supported_packet_source(function_frame);
    let cap_omitted_count = input.node_cap_omitted_count + input.edge_cap_omitted_count;

    let initial_packet_status = if has_integrity_failure {
        if !input.node_layer_current || !input.edge_layer_current {
            crate::LocalMicroFlowPacketStatus::MicroFlowStale
        } else {
            crate::LocalMicroFlowPacketStatus::MicroFlowCorrupt
        }
    } else if unsupported {
        crate::LocalMicroFlowPacketStatus::MicroFlowUnsupported
    } else if cap_omitted_count > 0 {
        crate::LocalMicroFlowPacketStatus::MicroFlowTruncated
    } else if eligible_edges.is_empty() {
        crate::LocalMicroFlowPacketStatus::NoMicroFlowPathFound
    } else if has_relation(&eligible_edges, MicroEdgeKind::LocalFlowsTo)
        && has_relation(&eligible_edges, MicroEdgeKind::LocalReturnsTo)
        && (has_relation(&eligible_edges, MicroEdgeKind::LocalReads)
            || has_relation(&eligible_edges, MicroEdgeKind::LocalWrites))
    {
        crate::LocalMicroFlowPacketStatus::MicroFlowFound
    } else {
        crate::LocalMicroFlowPacketStatus::PartialMicroFlowFound
    };

    let initial_proof_strength = match initial_packet_status {
        crate::LocalMicroFlowPacketStatus::MicroFlowFound
        | crate::LocalMicroFlowPacketStatus::PartialMicroFlowFound
        | crate::LocalMicroFlowPacketStatus::MicroFlowTruncated => {
            ProofLadderLevel::GraphRelationProof
        }
        crate::LocalMicroFlowPacketStatus::NoMicroFlowPathFound => ProofLadderLevel::Unknown,
        _ => ProofLadderLevel::DiagnosticOnly,
    };

    let mut packet = if has_integrity_failure || unsupported || function_frame.source_span.is_none()
    {
        None
    } else {
        Some(build_canonical_packet(
            repo_relative_path,
            function_frame,
            &function_nodes,
            &eligible_edges,
            initial_packet_status,
            initial_proof_strength,
            input.node_cap_omitted_count,
            input.edge_cap_omitted_count,
        ))
    };
    let (packet_status, proof_strength) = if let Some(packet) = packet.as_mut() {
        let classification = packet.proof_classification();
        packet.packet_status = classification.packet_status;
        packet.proof_strength = classification.packet_proof_strength;
        (
            classification.packet_status,
            classification.packet_proof_strength,
        )
    } else {
        (initial_packet_status, initial_proof_strength)
    };

    let packet_id = packet
        .as_ref()
        .map(|packet| packet.packet_id.clone())
        .unwrap_or_else(|| {
            stable_micro_packet_id(&MicroPacketIdentityInput {
                repo_relative_path: repo_relative_path.to_string(),
                language: function_frame.language.clone(),
                function_entity_id: function_domain.clone(),
                packet_kind: MVP4_3_LOCAL_MICRO_FLOW_PACKET_KIND.to_string(),
                packet_version: 1,
                extraction_version: MVP4_3_LOCAL_MICRO_FLOW_PACKET_EXTRACTION_VERSION.to_string(),
                step_set_version: LOCAL_MICRO_FLOW_DICT_V1_STEP_SET_VERSION.to_string(),
                ordered_step_ids: vec!["unavailable".to_string()],
            })
        });

    LocalMicroFlowPacketCandidate {
        packet_id,
        packet_kind: MVP4_3_LOCAL_MICRO_FLOW_PACKET_KIND.to_string(),
        function_micro_node_id: function_frame.micro_node_id.clone(),
        function_entity_id: function_frame.function_entity_id.clone(),
        file_id: repo_relative_path.to_string(),
        language: function_frame.language.clone(),
        source_role: function_frame.source_role,
        packet_status,
        proof_strength,
        node_ref_count: function_nodes.len(),
        edge_ref_count: eligible_edges.len(),
        gap_count: packet
            .as_ref()
            .map(|packet| packet.unknown_unsupported_gaps.len())
            .unwrap_or(0),
        source_micro_node_extraction_versions: unique_sorted_versions(
            function_nodes
                .iter()
                .map(|node| node.extraction_version.as_str()),
        ),
        source_micro_edge_extraction_versions: unique_sorted_versions(
            source_micro_edge_extraction_versions_for_candidate(
                function_frame.language.as_str(),
                &eligible_edges,
            )
            .iter()
            .map(String::as_str),
        ),
        packet,
        diagnostics: candidate_diagnostics,
    }
}

fn unique_sorted_versions<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    values
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn source_micro_edge_extraction_versions_for_candidate(
    language: &str,
    eligible_edges: &[&LocalMicroFlowPersistedEdgeFact],
) -> Vec<String> {
    let versions = unique_sorted_versions(
        eligible_edges
            .iter()
            .map(|edge| edge.extraction_version.as_str()),
    );
    if !versions.is_empty() {
        return versions;
    }

    match language.trim().to_ascii_lowercase().as_str() {
        "typescript" => [
            MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION,
            MVP4_2B_LOCAL_READS_EXTRACTION_VERSION,
            MVP4_2B_LOCAL_WRITES_EXTRACTION_VERSION,
            MVP4_2B_LOCAL_FLOWS_TO_EXTRACTION_VERSION,
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        _ => Vec::new(),
    }
}

fn build_canonical_packet(
    repo_relative_path: &str,
    function_frame: &LocalMicroFlowPersistedNodeFact,
    function_nodes: &[&LocalMicroFlowPersistedNodeFact],
    eligible_edges: &[&LocalMicroFlowPersistedEdgeFact],
    packet_status: crate::LocalMicroFlowPacketStatus,
    proof_strength: ProofLadderLevel,
    node_cap_omitted_count: u64,
    edge_cap_omitted_count: u64,
) -> LocalMicroFlowCanonicalPacket {
    let node_refs = node_refs_by_id(function_nodes);
    let mut cap_omissions = build_packet_cap_omissions(
        function_frame,
        node_cap_omitted_count,
        edge_cap_omitted_count,
    );
    let (mut paths, mut gaps) = construct_deterministic_packet_paths(
        function_frame,
        function_nodes,
        eligible_edges,
        &node_refs,
        &cap_omissions,
    );
    let packet_step_omitted_count =
        apply_packet_step_cap(&mut paths, &mut gaps, &mut cap_omissions, function_frame);
    let total_omitted_count =
        node_cap_omitted_count + edge_cap_omitted_count + packet_step_omitted_count;
    let step_ids = paths
        .iter()
        .flat_map(|path| path.steps.iter().map(|step| step.step_id.clone()))
        .collect::<Vec<_>>();
    let function_domain = function_frame
        .function_domain()
        .unwrap_or(function_frame.micro_node_id.as_str())
        .to_string();
    let packet_id = stable_micro_packet_id(&MicroPacketIdentityInput {
        repo_relative_path: repo_relative_path.to_string(),
        language: function_frame.language.clone(),
        function_entity_id: function_domain.clone(),
        packet_kind: MVP4_3_LOCAL_MICRO_FLOW_PACKET_KIND.to_string(),
        packet_version: 1,
        extraction_version: MVP4_3_LOCAL_MICRO_FLOW_PACKET_EXTRACTION_VERSION.to_string(),
        step_set_version: LOCAL_MICRO_FLOW_DICT_V1_STEP_SET_VERSION.to_string(),
        ordered_step_ids: step_ids,
    });

    let mut packet = LocalMicroFlowCanonicalPacket {
        packet_id,
        file: LocalMicroFlowFileRef {
            repo_relative_path: repo_relative_path.to_string(),
            language: function_frame.language.clone(),
            source_role: function_frame.source_role.as_str().to_string(),
            file_id: Some(function_frame.file_id.clone()),
            lifecycle_binding: Some(function_frame.lifecycle_binding.clone()),
        },
        function_identity: LocalMicroFlowFunctionIdentity {
            function_id: function_domain,
            function_entity_id: function_frame.function_entity_id.clone(),
            function_name: function_frame.symbol.clone(),
            identity_status: "persisted_function_frame_identity".to_string(),
            scope_path: function_frame
                .scope_entity_id
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            structural_path: vec![function_frame.micro_node_id.clone()],
        },
        function_span: function_frame
            .source_span
            .clone()
            .unwrap_or_else(|| SourceSpan::new(repo_relative_path, 1, 1)),
        claimability: "packet_candidate_from_persisted_micro_facts".to_string(),
        packet_status,
        proof_strength,
        budget: LocalMicroFlowBudget {
            max_packet_steps: Some(LOCAL_MICRO_FLOW_MAX_PACKET_STEPS as u32),
            max_packet_bytes: Some(LOCAL_MICRO_FLOW_MAX_PACKET_BODY_BYTES),
            omitted_count: total_omitted_count,
            truncation_reason: if packet_step_omitted_count > 0 {
                "packet_step_cap_omission".to_string()
            } else if total_omitted_count > 0 {
                "packet_input_cap_omission".to_string()
            } else {
                "none".to_string()
            },
            cap_omissions,
        },
        risks_limitations: vec![
            "candidate_only_not_persisted".to_string(),
            "flow_proof_inactive_until_later_packet_gate".to_string(),
            "function_local_only".to_string(),
        ],
        unknown_unsupported_gaps: gaps,
        expansion_handles: vec![LocalMicroFlowExpansionHandle {
            handle: format!(
                "expand://local-micro-flow/{}/audit",
                function_frame.micro_node_id
            ),
            handle_kind: "audit_trace_placeholder".to_string(),
            available: false,
            proof_boundary:
                "Prompt 5 path-construction candidate only; no agent-facing packet expansion active"
                    .to_string(),
        }],
        paths,
    };
    apply_packet_body_byte_cap(&mut packet, function_frame);
    packet
}

fn build_packet_cap_omissions(
    function_frame: &LocalMicroFlowPersistedNodeFact,
    node_cap_omitted_count: u64,
    edge_cap_omitted_count: u64,
) -> Vec<LocalMicroFlowCapOmission> {
    let mut cap_omissions = Vec::new();
    if node_cap_omitted_count > 0 {
        cap_omissions.push(LocalMicroFlowCapOmission {
            omission_id: format!("cap:{}:nodes", function_frame.micro_node_id),
            omitted_count: node_cap_omitted_count,
            reason: "retained micro-node cap omitted packet input facts".to_string(),
            completeness_label: "incomplete_after_node_cap_omission".to_string(),
        });
    }
    if edge_cap_omitted_count > 0 {
        cap_omissions.push(LocalMicroFlowCapOmission {
            omission_id: format!("cap:{}:edges", function_frame.micro_node_id),
            omitted_count: edge_cap_omitted_count,
            reason: "retained micro-edge cap omitted packet input facts".to_string(),
            completeness_label: "incomplete_after_edge_cap_omission".to_string(),
        });
    }
    cap_omissions
}

fn apply_packet_step_cap(
    paths: &mut Vec<LocalMicroFlowCanonicalPath>,
    packet_gaps: &mut Vec<LocalMicroFlowGap>,
    cap_omissions: &mut Vec<LocalMicroFlowCapOmission>,
    function_frame: &LocalMicroFlowPersistedNodeFact,
) -> u64 {
    let original_step_count = paths.iter().map(|path| path.steps.len()).sum::<usize>();
    if original_step_count <= LOCAL_MICRO_FLOW_MAX_PACKET_STEPS {
        return 0;
    }

    let retained_step_budget = LOCAL_MICRO_FLOW_MAX_PACKET_STEPS.saturating_sub(1);
    let mut remaining = retained_step_budget;
    let mut last_retained_path_index = None;

    for (path_index, path) in paths.iter_mut().enumerate() {
        if remaining >= path.steps.len() {
            if !path.steps.is_empty() {
                last_retained_path_index = Some(path_index);
            }
            remaining -= path.steps.len();
        } else {
            path.steps.truncate(remaining);
            if remaining > 0 {
                last_retained_path_index = Some(path_index);
            }
            remaining = 0;
        }
    }

    let retained_step_count = paths.iter().map(|path| path.steps.len()).sum::<usize>();
    let omitted_count = original_step_count.saturating_sub(retained_step_count) as u64;
    if omitted_count == 0 {
        return 0;
    }

    let cap_omission = LocalMicroFlowCapOmission {
        omission_id: format!("cap:{}:packet-steps", function_frame.micro_node_id),
        omitted_count,
        reason: format!(
            "local micro-flow packet step cap {} omitted compact packet steps before storage",
            LOCAL_MICRO_FLOW_MAX_PACKET_STEPS
        ),
        completeness_label: "incomplete_after_packet_step_cap_omission".to_string(),
    };
    let gap = cap_omission_gap(function_frame, &cap_omission);
    if !packet_gaps
        .iter()
        .any(|existing| existing.gap_id == gap.gap_id)
    {
        packet_gaps.push(gap.clone());
    }

    let target_path_index = last_retained_path_index.unwrap_or(0);
    let mut cap_step = gap_to_canonical_step(&gap, retained_step_count as u32);
    cap_step.cap_omission = Some(cap_omission.clone());
    cap_step.return_path_identity = paths.get(target_path_index).and_then(|path| {
        path.steps
            .iter()
            .find_map(|step| step.return_path_identity.clone())
    });
    cap_step.return_path_id = cap_step
        .return_path_identity
        .as_ref()
        .map(|identity| identity.return_path_id.clone());
    cap_step.order_key.return_path_order = cap_step.return_path_id.clone();
    cap_step
        .limitations
        .push("packet_step_cap_omission_makes_packet_completeness_unknown".to_string());

    if let Some(path) = paths.get_mut(target_path_index) {
        path.steps.push(cap_step);
    }
    cap_omissions.push(cap_omission);
    omitted_count
}

fn apply_packet_body_byte_cap(
    packet: &mut LocalMicroFlowCanonicalPacket,
    function_frame: &LocalMicroFlowPersistedNodeFact,
) -> u64 {
    let Ok(body) = packet.encode_dict_v1() else {
        return 0;
    };
    let Ok(body_json) = serde_json::to_string(&body) else {
        return 0;
    };
    let max_bytes = LOCAL_MICRO_FLOW_MAX_PACKET_BODY_BYTES as usize;
    if body_json.len() <= max_bytes {
        return 0;
    }

    let original_step_count = packet
        .paths
        .iter()
        .map(|path| path.steps.len())
        .sum::<usize>();
    let omitted_count = original_step_count.max(1) as u64;
    let cap_omission = LocalMicroFlowCapOmission {
        omission_id: format!("cap:{}:packet-body-bytes", function_frame.micro_node_id),
        omitted_count,
        reason: format!(
            "local micro-flow packet body cap {} bytes omitted compact packet details before storage; encoded_body_bytes={}",
            LOCAL_MICRO_FLOW_MAX_PACKET_BODY_BYTES,
            body_json.len()
        ),
        completeness_label: "incomplete_after_packet_body_byte_cap_omission".to_string(),
    };
    let gap = cap_omission_gap(function_frame, &cap_omission);
    if !packet
        .unknown_unsupported_gaps
        .iter()
        .any(|existing| existing.gap_id == gap.gap_id)
    {
        packet.unknown_unsupported_gaps.push(gap.clone());
    }
    let mut cap_step = gap_to_canonical_step(&gap, 0);
    cap_step.cap_omission = Some(cap_omission.clone());
    cap_step
        .limitations
        .push("packet_body_byte_cap_omission_makes_packet_completeness_unknown".to_string());
    packet.paths = vec![LocalMicroFlowCanonicalPath {
        path_id: format!("path:{}:packet-body-byte-cap", function_frame.micro_node_id),
        branch_id: None,
        return_path_id: None,
        steps: vec![cap_step],
    }];
    packet.budget.omitted_count = packet.budget.omitted_count.saturating_add(omitted_count);
    packet.budget.truncation_reason = "packet_body_byte_cap_omission".to_string();
    packet.budget.cap_omissions.push(cap_omission);
    packet.packet_status = crate::LocalMicroFlowPacketStatus::MicroFlowTruncated;
    packet.proof_strength = ProofLadderLevel::Unknown;
    omitted_count
}

fn construct_deterministic_packet_paths(
    function_frame: &LocalMicroFlowPersistedNodeFact,
    function_nodes: &[&LocalMicroFlowPersistedNodeFact],
    eligible_edges: &[&LocalMicroFlowPersistedEdgeFact],
    node_refs: &BTreeMap<String, LocalMicroFlowMicroNodeRef>,
    cap_omissions: &[LocalMicroFlowCapOmission],
) -> (Vec<LocalMicroFlowCanonicalPath>, Vec<LocalMicroFlowGap>) {
    let flow_edges =
        ordered_flow_chain_edges(&relation_edges(eligible_edges, MicroEdgeKind::LocalFlowsTo));
    let read_write_edges = sorted_edges(
        eligible_edges
            .iter()
            .copied()
            .filter(|edge| {
                matches!(
                    edge.relation_kind,
                    Some(MicroEdgeKind::LocalReads | MicroEdgeKind::LocalWrites)
                )
            })
            .collect(),
    );
    let return_edges = sorted_edges(relation_edges(
        eligible_edges,
        MicroEdgeKind::LocalReturnsTo,
    ));
    let mut packet_gaps = BTreeMap::<String, LocalMicroFlowGap>::new();
    let mut paths = Vec::new();

    let branch_gap = (return_edges.len() > 1
        && !has_deterministic_branch_support(function_nodes, eligible_edges))
    .then(|| unsupported_branch_structure_gap(function_frame, &return_edges));
    let dynamic_call_gap = function_nodes
        .iter()
        .any(|node| node.micro_kind == Some(MicroNodeKind::CallSite))
        .then(|| dynamic_call_target_gap(function_frame, function_nodes));
    let member_target_gap = function_nodes
        .iter()
        .any(|node| node.micro_kind == Some(MicroNodeKind::PropertyAccess))
        .then(|| member_or_global_target_gap(function_frame, function_nodes));

    if return_edges.is_empty() {
        let return_path_id = None;
        let mut path_steps = Vec::new();
        let mut emitted_edges = BTreeSet::new();
        push_flow_inventory_steps(
            &mut path_steps,
            &mut emitted_edges,
            &flow_edges,
            &read_write_edges,
            node_refs,
            return_path_id.as_deref(),
        );
        let no_return_gap = if eligible_edges.is_empty() {
            no_local_micro_flow_path_gap(function_frame)
        } else {
            missing_return_anchor_gap(function_frame)
        };
        record_gap(&mut packet_gaps, no_return_gap.clone());
        push_gap_step_for_path(&mut path_steps, &no_return_gap, return_path_id.as_deref());
        push_optional_gap_step(
            &mut path_steps,
            &mut packet_gaps,
            dynamic_call_gap.as_ref(),
            return_path_id.as_deref(),
        );
        push_optional_gap_step(
            &mut path_steps,
            &mut packet_gaps,
            member_target_gap.as_ref(),
            return_path_id.as_deref(),
        );
        push_cap_omission_steps(
            &mut path_steps,
            &mut packet_gaps,
            function_frame,
            cap_omissions,
            return_path_id.as_deref(),
        );
        paths.push(LocalMicroFlowCanonicalPath {
            path_id: format!("path:{}:no-return-anchor", function_frame.micro_node_id),
            branch_id: None,
            return_path_id,
            steps: path_steps,
        });
    } else {
        for return_edge in &return_edges {
            let return_path_id = Some(format!("return-path:{}", return_edge.micro_edge_id));
            let mut path_steps = Vec::new();
            let mut emitted_edges = BTreeSet::new();
            push_flow_inventory_steps(
                &mut path_steps,
                &mut emitted_edges,
                &flow_edges,
                &read_write_edges,
                node_refs,
                return_path_id.as_deref(),
            );
            if flow_edges.is_empty() {
                let gap = missing_local_flows_to_gap(function_frame, return_edge);
                record_gap(&mut packet_gaps, gap.clone());
                push_gap_step_for_path(&mut path_steps, &gap, return_path_id.as_deref());
            } else if read_write_edges.is_empty() {
                let gap = missing_read_write_provenance_gap(function_frame, return_edge);
                record_gap(&mut packet_gaps, gap.clone());
                push_gap_step_for_path(&mut path_steps, &gap, return_path_id.as_deref());
            }
            push_optional_gap_step(
                &mut path_steps,
                &mut packet_gaps,
                branch_gap.as_ref(),
                return_path_id.as_deref(),
            );
            push_optional_gap_step(
                &mut path_steps,
                &mut packet_gaps,
                dynamic_call_gap.as_ref(),
                return_path_id.as_deref(),
            );
            push_optional_gap_step(
                &mut path_steps,
                &mut packet_gaps,
                member_target_gap.as_ref(),
                return_path_id.as_deref(),
            );
            push_edge_step_for_path(
                &mut path_steps,
                &mut emitted_edges,
                return_edge,
                node_refs,
                return_path_id.as_deref(),
            );
            push_cap_omission_steps(
                &mut path_steps,
                &mut packet_gaps,
                function_frame,
                cap_omissions,
                return_path_id.as_deref(),
            );
            paths.push(LocalMicroFlowCanonicalPath {
                path_id: format!(
                    "path:{}:{}",
                    function_frame.micro_node_id, return_edge.micro_edge_id
                ),
                branch_id: None,
                return_path_id,
                steps: path_steps,
            });
        }
    }

    paths.sort_by(|left, right| {
        path_order_key(left)
            .cmp(&path_order_key(right))
            .then_with(|| left.path_id.cmp(&right.path_id))
    });
    (
        paths,
        packet_gaps
            .into_iter()
            .map(|(_, gap)| gap)
            .collect::<Vec<_>>(),
    )
}

fn relation_edges<'a>(
    edges: &[&'a LocalMicroFlowPersistedEdgeFact],
    relation_kind: MicroEdgeKind,
) -> Vec<&'a LocalMicroFlowPersistedEdgeFact> {
    edges
        .iter()
        .copied()
        .filter(|edge| edge.relation_kind == Some(relation_kind))
        .collect()
}

fn sorted_edges<'a>(
    mut edges: Vec<&'a LocalMicroFlowPersistedEdgeFact>,
) -> Vec<&'a LocalMicroFlowPersistedEdgeFact> {
    edges.sort_by(|left, right| {
        edge_order_key(left)
            .cmp(&edge_order_key(right))
            .then_with(|| left.micro_edge_id.cmp(&right.micro_edge_id))
    });
    edges
}

fn ordered_flow_chain_edges<'a>(
    flow_edges: &[&'a LocalMicroFlowPersistedEdgeFact],
) -> Vec<&'a LocalMicroFlowPersistedEdgeFact> {
    let mut by_source: BTreeMap<String, Vec<&'a LocalMicroFlowPersistedEdgeFact>> = BTreeMap::new();
    let mut targets = BTreeSet::new();
    for edge in flow_edges {
        by_source
            .entry(edge.source_micro_node_id.clone())
            .or_default()
            .push(*edge);
        targets.insert(edge.target_micro_node_id.clone());
    }
    for edges in by_source.values_mut() {
        *edges = sorted_edges(std::mem::take(edges));
    }

    let mut ordered = Vec::new();
    let mut emitted = BTreeSet::new();
    let starts = flow_edges
        .iter()
        .filter(|edge| !targets.contains(edge.source_micro_node_id.as_str()))
        .map(|edge| edge.source_micro_node_id.clone())
        .collect::<BTreeSet<_>>();
    for start in starts {
        append_flow_chain_from(&start, &by_source, &mut emitted, &mut ordered);
    }
    for edge in sorted_edges(flow_edges.to_vec()) {
        if emitted.contains(edge.micro_edge_id.as_str()) {
            continue;
        }
        append_flow_chain_from(
            &edge.source_micro_node_id,
            &by_source,
            &mut emitted,
            &mut ordered,
        );
        if emitted.insert(edge.micro_edge_id.clone()) {
            ordered.push(edge);
        }
    }
    ordered
}

fn append_flow_chain_from<'a>(
    source_id: &str,
    by_source: &BTreeMap<String, Vec<&'a LocalMicroFlowPersistedEdgeFact>>,
    emitted: &mut BTreeSet<String>,
    ordered: &mut Vec<&'a LocalMicroFlowPersistedEdgeFact>,
) {
    let mut stack = vec![source_id.to_string()];
    while let Some(source) = stack.pop() {
        let Some(edges) = by_source.get(&source) else {
            continue;
        };
        for edge in edges {
            if emitted.insert(edge.micro_edge_id.clone()) {
                ordered.push(*edge);
                stack.push(edge.target_micro_node_id.clone());
            }
        }
    }
}

fn push_flow_inventory_steps(
    steps: &mut Vec<LocalMicroFlowCanonicalStep>,
    emitted_edges: &mut BTreeSet<String>,
    flow_edges: &[&LocalMicroFlowPersistedEdgeFact],
    read_write_edges: &[&LocalMicroFlowPersistedEdgeFact],
    node_refs: &BTreeMap<String, LocalMicroFlowMicroNodeRef>,
    return_path_id: Option<&str>,
) {
    if flow_edges.is_empty() {
        for support_edge in read_write_edges {
            push_edge_step_for_path(
                steps,
                emitted_edges,
                support_edge,
                node_refs,
                return_path_id,
            );
        }
        return;
    }

    for flow_edge in flow_edges {
        for support_edge in read_write_edges
            .iter()
            .filter(|support_edge| support_edge_supports_flow(support_edge, flow_edge))
        {
            push_edge_step_for_path(
                steps,
                emitted_edges,
                support_edge,
                node_refs,
                return_path_id,
            );
        }
        push_edge_step_for_path(steps, emitted_edges, flow_edge, node_refs, return_path_id);
    }
}

fn support_edge_supports_flow(
    support_edge: &LocalMicroFlowPersistedEdgeFact,
    flow_edge: &LocalMicroFlowPersistedEdgeFact,
) -> bool {
    let flow_endpoint_ids = [
        flow_edge.source_micro_node_id.as_str(),
        flow_edge.target_micro_node_id.as_str(),
    ];
    flow_endpoint_ids.contains(&support_edge.source_micro_node_id.as_str())
        || flow_endpoint_ids.contains(&support_edge.target_micro_node_id.as_str())
}

fn push_edge_step_for_path(
    steps: &mut Vec<LocalMicroFlowCanonicalStep>,
    emitted_edges: &mut BTreeSet<String>,
    edge: &LocalMicroFlowPersistedEdgeFact,
    node_refs: &BTreeMap<String, LocalMicroFlowMicroNodeRef>,
    return_path_id: Option<&str>,
) {
    if !emitted_edges.insert(edge.micro_edge_id.clone()) {
        return;
    }
    let Some(step) = edge_to_canonical_step(edge, node_refs, steps.len() as u32) else {
        return;
    };
    let _ = return_path_id;
    steps.push(step);
}

fn push_optional_gap_step(
    steps: &mut Vec<LocalMicroFlowCanonicalStep>,
    packet_gaps: &mut BTreeMap<String, LocalMicroFlowGap>,
    gap: Option<&LocalMicroFlowGap>,
    return_path_id: Option<&str>,
) {
    let Some(gap) = gap else {
        return;
    };
    record_gap(packet_gaps, gap.clone());
    push_gap_step_for_path(steps, gap, return_path_id);
}

fn push_gap_step_for_path(
    steps: &mut Vec<LocalMicroFlowCanonicalStep>,
    gap: &LocalMicroFlowGap,
    return_path_id: Option<&str>,
) {
    let step = gap_to_canonical_step(gap, steps.len() as u32);
    let _ = return_path_id;
    steps.push(step);
}

fn push_cap_omission_steps(
    steps: &mut Vec<LocalMicroFlowCanonicalStep>,
    packet_gaps: &mut BTreeMap<String, LocalMicroFlowGap>,
    function_frame: &LocalMicroFlowPersistedNodeFact,
    cap_omissions: &[LocalMicroFlowCapOmission],
    return_path_id: Option<&str>,
) {
    for cap_omission in cap_omissions {
        let gap = cap_omission_gap(function_frame, cap_omission);
        record_gap(packet_gaps, gap.clone());
        let mut step = gap_to_canonical_step(&gap, steps.len() as u32);
        step.cap_omission = Some(cap_omission.clone());
        let _ = return_path_id;
        step.limitations
            .push("cap_omission_makes_packet_completeness_unknown".to_string());
        steps.push(step);
    }
}

fn record_gap(packet_gaps: &mut BTreeMap<String, LocalMicroFlowGap>, gap: LocalMicroFlowGap) {
    packet_gaps.entry(gap.gap_id.clone()).or_insert(gap);
}

fn has_deterministic_branch_support(
    function_nodes: &[&LocalMicroFlowPersistedNodeFact],
    eligible_edges: &[&LocalMicroFlowPersistedEdgeFact],
) -> bool {
    function_nodes
        .iter()
        .any(|node| node.micro_kind == Some(MicroNodeKind::ConditionSite))
        && eligible_edges
            .iter()
            .any(|edge| edge.relation_kind == Some(MicroEdgeKind::LocalBranchesTo))
}

fn missing_local_flows_to_gap(
    function_frame: &LocalMicroFlowPersistedNodeFact,
    return_edge: &LocalMicroFlowPersistedEdgeFact,
) -> LocalMicroFlowGap {
    LocalMicroFlowGap {
        gap_id: format!(
            "gap:{}:{}:missing-local-flows-to",
            function_frame.micro_node_id, return_edge.micro_edge_id
        ),
        gap_kind: "missing_local_flows_to".to_string(),
        reason: "Return anchor is present but no retained LOCAL_FLOWS_TO chain connects local bindings for this return path".to_string(),
        exactness: MicroExactness::Unknown,
        proof_status: "unknown".to_string(),
        source_spans: gap_spans(function_frame, Some(return_edge)),
    }
}

fn missing_read_write_provenance_gap(
    function_frame: &LocalMicroFlowPersistedNodeFact,
    return_edge: &LocalMicroFlowPersistedEdgeFact,
) -> LocalMicroFlowGap {
    LocalMicroFlowGap {
        gap_id: format!(
            "gap:{}:{}:missing-read-write-provenance",
            function_frame.micro_node_id, return_edge.micro_edge_id
        ),
        gap_kind: "missing_local_reads_writes_provenance".to_string(),
        reason: "LOCAL_FLOWS_TO exists, but supporting LOCAL_READS/LOCAL_WRITES provenance is absent from the retained relation inventory".to_string(),
        exactness: MicroExactness::Unknown,
        proof_status: "unknown".to_string(),
        source_spans: gap_spans(function_frame, Some(return_edge)),
    }
}

fn no_local_micro_flow_path_gap(
    function_frame: &LocalMicroFlowPersistedNodeFact,
) -> LocalMicroFlowGap {
    LocalMicroFlowGap {
        gap_id: format!("gap:{}:no-local-micro-flow-path", function_frame.micro_node_id),
        gap_kind: "no_micro_flow_path_found".to_string(),
        reason: "no retained LOCAL_READS/LOCAL_WRITES/LOCAL_FLOWS_TO/LOCAL_RETURNS_TO facts for this function".to_string(),
        exactness: MicroExactness::Unknown,
        proof_status: "unknown".to_string(),
        source_spans: gap_spans(function_frame, None),
    }
}

fn missing_return_anchor_gap(
    function_frame: &LocalMicroFlowPersistedNodeFact,
) -> LocalMicroFlowGap {
    LocalMicroFlowGap {
        gap_id: format!("gap:{}:missing-return-anchor", function_frame.micro_node_id),
        gap_kind: "missing_return_anchor".to_string(),
        reason:
            "local relation facts exist but no retained LOCAL_RETURNS_TO return anchor is available"
                .to_string(),
        exactness: MicroExactness::Unknown,
        proof_status: "unknown".to_string(),
        source_spans: gap_spans(function_frame, None),
    }
}

fn unsupported_branch_structure_gap(
    function_frame: &LocalMicroFlowPersistedNodeFact,
    return_edges: &[&LocalMicroFlowPersistedEdgeFact],
) -> LocalMicroFlowGap {
    let mut source_spans = return_edges
        .iter()
        .filter_map(|edge| edge.source_span.clone())
        .collect::<Vec<_>>();
    if source_spans.is_empty() {
        source_spans = gap_spans(function_frame, None);
    }
    LocalMicroFlowGap {
        gap_id: format!("gap:{}:unsupported-branch-structure", function_frame.micro_node_id),
        gap_kind: "unsupported_branch_structure".to_string(),
        reason: "multiple return paths exist, but persisted branch/condition identity is not available in the current packet slice".to_string(),
        exactness: MicroExactness::Unknown,
        proof_status: "unsupported".to_string(),
        source_spans,
    }
}

fn dynamic_call_target_gap(
    function_frame: &LocalMicroFlowPersistedNodeFact,
    function_nodes: &[&LocalMicroFlowPersistedNodeFact],
) -> LocalMicroFlowGap {
    LocalMicroFlowGap {
        gap_id: format!("gap:{}:dynamic-call-target", function_frame.micro_node_id),
        gap_kind: "dynamic_or_unresolved_call_target".to_string(),
        reason: "CallSite nodes are present, but LOCAL_CALLS is not implemented for MVP4.3 first-slice packet paths".to_string(),
        exactness: MicroExactness::Unknown,
        proof_status: "unsupported".to_string(),
        source_spans: node_kind_spans(function_nodes, MicroNodeKind::CallSite)
            .unwrap_or_else(|| gap_spans(function_frame, None)),
    }
}

fn member_or_global_target_gap(
    function_frame: &LocalMicroFlowPersistedNodeFact,
    function_nodes: &[&LocalMicroFlowPersistedNodeFact],
) -> LocalMicroFlowGap {
    LocalMicroFlowGap {
        gap_id: format!("gap:{}:member-global-target", function_frame.micro_node_id),
        gap_kind: "member_or_global_target_unsupported".to_string(),
        reason: "PropertyAccess nodes may exist, but member/global target proof is outside the current local packet relation inventory".to_string(),
        exactness: MicroExactness::Unknown,
        proof_status: "unsupported".to_string(),
        source_spans: node_kind_spans(function_nodes, MicroNodeKind::PropertyAccess)
            .unwrap_or_else(|| gap_spans(function_frame, None)),
    }
}

fn cap_omission_gap(
    function_frame: &LocalMicroFlowPersistedNodeFact,
    cap_omission: &LocalMicroFlowCapOmission,
) -> LocalMicroFlowGap {
    LocalMicroFlowGap {
        gap_id: format!(
            "gap:{}:{}",
            function_frame.micro_node_id, cap_omission.omission_id
        ),
        gap_kind: "cap_omission".to_string(),
        reason: cap_omission.reason.clone(),
        exactness: MicroExactness::Unknown,
        proof_status: "truncated".to_string(),
        source_spans: gap_spans(function_frame, None),
    }
}

fn gap_spans(
    function_frame: &LocalMicroFlowPersistedNodeFact,
    edge: Option<&LocalMicroFlowPersistedEdgeFact>,
) -> Vec<SourceSpan> {
    edge.and_then(|edge| edge.source_span.clone())
        .into_iter()
        .chain(function_frame.source_span.clone())
        .collect()
}

fn node_kind_spans(
    function_nodes: &[&LocalMicroFlowPersistedNodeFact],
    micro_kind: MicroNodeKind,
) -> Option<Vec<SourceSpan>> {
    let spans = function_nodes
        .iter()
        .filter(|node| node.micro_kind == Some(micro_kind))
        .filter_map(|node| node.source_span.clone())
        .collect::<Vec<_>>();
    (!spans.is_empty()).then_some(spans)
}

fn path_order_key(path: &LocalMicroFlowCanonicalPath) -> (u32, u32, String) {
    let first_span = path
        .steps
        .iter()
        .flat_map(|step| step.source_spans.iter())
        .next();
    (
        first_span.map(|span| span.start_line).unwrap_or(u32::MAX),
        first_span
            .and_then(|span| span.start_column)
            .unwrap_or(u32::MAX),
        path.return_path_id.clone().unwrap_or_default(),
    )
}

fn classify_local_micro_flow_packet_proof(
    packet: &LocalMicroFlowCanonicalPacket,
) -> LocalMicroFlowPacketProofClassification {
    let dict_v1_lossless_to_audit_ordered_steps = packet
        .encode_dict_v1()
        .and_then(|body| body.to_ordered_steps().map(|_| body))
        .is_ok();
    let path_classifications = packet
        .paths
        .iter()
        .map(|path| {
            classify_local_micro_flow_path_proof(
                packet,
                path,
                dict_v1_lossless_to_audit_ordered_steps,
            )
        })
        .collect::<Vec<_>>();

    let flow_proof_path_count = path_classifications
        .iter()
        .filter(|classification| classification.flow_proof_eligible)
        .count();
    let non_flow_path_count = path_classifications
        .len()
        .saturating_sub(flow_proof_path_count);
    let strongest_path_proof_strength = strongest_proof_strength(
        path_classifications
            .iter()
            .map(|classification| classification.proof_strength),
    );
    let strongest_path_state = path_classifications
        .iter()
        .max_by_key(|classification| proof_strength_rank(classification.proof_strength))
        .map(|classification| classification.classification_state)
        .unwrap_or(LocalMicroFlowProofClassificationState::NoMicroFlowPathFound);
    let packet_status = aggregate_packet_status(&path_classifications, packet.packet_status);
    let packet_proof_strength = strongest_path_proof_strength;
    let packet_summary_no_overclaim = !(flow_proof_path_count > 0
        && non_flow_path_count > 0
        && packet_status == crate::LocalMicroFlowPacketStatus::MicroFlowFound);
    let linter_mapping =
        linter_mapping_for_state(aggregate_linter_state(&path_classifications, packet_status));

    LocalMicroFlowPacketProofClassification {
        packet_status,
        packet_proof_strength,
        strongest_path_state,
        strongest_path_proof_strength,
        flow_proof_path_count,
        non_flow_path_count,
        packet_summary_no_overclaim,
        dict_v1_lossless_to_audit_ordered_steps,
        path_classifications,
        linter_mapping,
    }
}

fn classify_local_micro_flow_path_proof(
    packet: &LocalMicroFlowCanonicalPacket,
    path: &LocalMicroFlowCanonicalPath,
    dict_v1_lossless_to_audit_ordered_steps: bool,
) -> LocalMicroFlowPathProofClassification {
    let graph_relation_steps = path
        .steps
        .iter()
        .filter(|step| step.proof_contribution == ProofLadderLevel::GraphRelationProof)
        .collect::<Vec<_>>();
    let relation_kinds = graph_relation_steps
        .iter()
        .flat_map(|step| {
            step.micro_edge_refs
                .iter()
                .map(|edge| edge.relation_kind.as_str())
        })
        .collect::<Vec<_>>();
    let flow_step_count = relation_kinds
        .iter()
        .filter(|relation| **relation == "LOCAL_FLOWS_TO")
        .count();
    let read_step_count = relation_kinds
        .iter()
        .filter(|relation| **relation == "LOCAL_READS")
        .count();
    let write_step_count = relation_kinds
        .iter()
        .filter(|relation| **relation == "LOCAL_WRITES")
        .count();
    let return_step_count = relation_kinds
        .iter()
        .filter(|relation| **relation == "LOCAL_RETURNS_TO")
        .count();
    let gap_count = path.steps.iter().filter(|step| step.gap.is_some()).count();
    let cap_omission_count = path
        .steps
        .iter()
        .filter(|step| step.cap_omission.is_some())
        .count();
    let source_spanned_proof_bearing_steps = graph_relation_steps
        .iter()
        .all(|step| !step.source_spans.is_empty());
    let backed_by_current_persisted_micro_facts = graph_relation_steps.iter().all(|step| {
        !step.micro_edge_refs.is_empty()
            && !step.micro_node_refs.is_empty()
            && step
                .micro_edge_refs
                .iter()
                .all(|edge| !edge.micro_edge_id.trim().is_empty())
    });
    let derived_steps_have_provenance = graph_relation_steps.iter().all(|step| {
        let has_derived_edge = step
            .micro_edge_refs
            .iter()
            .any(|edge| edge.relation_kind == "LOCAL_FLOWS_TO");
        !has_derived_edge
            || (!step.provenance.is_empty()
                && step
                    .micro_edge_refs
                    .iter()
                    .filter(|edge| edge.relation_kind == "LOCAL_FLOWS_TO")
                    .all(|edge| edge.exactness == MicroExactness::DerivedWithProvenance))
    });
    let exact_steps_preserve_exactness = graph_relation_steps.iter().all(|step| {
        step.micro_edge_refs
            .iter()
            .all(|edge| match edge.relation_kind.as_str() {
                "LOCAL_READS" | "LOCAL_WRITES" | "LOCAL_RETURNS_TO" => {
                    edge.exactness == MicroExactness::Exact
                }
                "LOCAL_FLOWS_TO" => edge.exactness == MicroExactness::DerivedWithProvenance,
                _ => false,
            })
    });
    let endpoint_facts_current_and_claimable = graph_relation_steps
        .iter()
        .all(|step| step.claimability.trim().starts_with("claimable_"));
    let production_safe_source_role = packet.file.source_role
        == MicroSourceRole::Production.as_str()
        && graph_relation_steps
            .iter()
            .all(|step| step.source_role == MicroSourceRole::Production);
    let branch_identity_preserved = !path.steps.iter().any(|step| {
        step.gap
            .as_ref()
            .is_some_and(|gap| gap.gap_kind == "unsupported_branch_structure")
    }) && path.branch_id.as_ref().is_none_or(|branch_id| {
        path.steps.iter().any(|step| {
            step.branch_identity
                .as_ref()
                .is_some_and(|branch| &branch.branch_id == branch_id)
        })
    });
    let return_path_identity_preserved = if return_step_count > 0 {
        path.return_path_id.as_ref().is_some_and(|return_path_id| {
            path.steps.iter().any(|step| {
                step.return_path_identity
                    .as_ref()
                    .is_some_and(|return_path| &return_path.return_path_id == return_path_id)
            })
        })
    } else {
        false
    };
    let shadowed_binding_identity_preserved = path.steps.iter().all(|step| {
        let node_ids = step
            .micro_node_refs
            .iter()
            .map(|node| node.micro_node_id.as_str())
            .collect::<Vec<_>>();
        let unique_node_ids = node_ids.iter().copied().collect::<BTreeSet<_>>();
        node_ids.len() == unique_node_ids.len()
    });
    let no_packet_integrity_finding = !path.steps.iter().any(|step| {
        step.gap.as_ref().is_some_and(|gap| {
            matches!(
                gap.gap_kind.as_str(),
                "missing_source_span"
                    | "missing_provenance"
                    | "missing_endpoint"
                    | "packet_integrity"
                    | "dict_v1_integrity"
            )
        })
    });
    let input = LocalMicroFlowProofEligibilityInput {
        deterministic_proof_bearing_steps: true,
        source_spanned_proof_bearing_steps,
        backed_by_current_persisted_micro_facts,
        derived_steps_have_provenance,
        exact_steps_preserve_exactness,
        endpoint_facts_current_and_claimable,
        production_safe_source_role,
        branch_identity_preserved,
        return_path_identity_preserved,
        shadowed_binding_identity_preserved,
        claimed_path_not_truncated: cap_omission_count == 0,
        cap_omissions_outside_claimed_segment: cap_omission_count == 0,
        gaps_outside_claimed_path: gap_count == 0,
        claimable_lifecycle_passport: endpoint_facts_current_and_claimable,
        dict_v1_lossless_to_audit_ordered_steps,
        no_packet_integrity_finding,
        includes_local_flows_to: flow_step_count > 0,
        includes_required_reads_and_writes: read_step_count > 0
            && (write_step_count > 0 || flow_step_count > 0),
        local_returns_to_only: return_step_count > 0
            && flow_step_count == 0
            && read_step_count == 0
            && write_step_count == 0,
        contains_claimable_graph_relation_summary: !graph_relation_steps.is_empty(),
    };
    let decision = decide_local_micro_flow_proof_eligibility(&input);
    let classification_state = classify_decision_state(&decision, &input);
    let linter_mapping = linter_mapping_for_state(classification_state);

    LocalMicroFlowPathProofClassification {
        path_id: path.path_id.clone(),
        return_path_id: path.return_path_id.clone(),
        classification_state,
        packet_status: decision.packet_status,
        proof_strength: decision.proof_ladder_level,
        flow_proof_eligible: decision.flow_proof_eligible,
        missing_requirements: decision
            .missing_requirements
            .iter()
            .map(|requirement| requirement.as_str().to_string())
            .collect(),
        gap_count,
        cap_omission_count,
        graph_relation_step_count: graph_relation_steps.len(),
        flow_step_count,
        read_step_count,
        write_step_count,
        return_step_count,
        linter_mapping,
    }
}

fn classify_decision_state(
    decision: &LocalMicroFlowProofEligibilityDecision,
    input: &LocalMicroFlowProofEligibilityInput,
) -> LocalMicroFlowProofClassificationState {
    if decision.flow_proof_eligible {
        return LocalMicroFlowProofClassificationState::FlowProof;
    }
    if !input.no_packet_integrity_finding {
        return LocalMicroFlowProofClassificationState::Corrupt;
    }
    if !input.production_safe_source_role {
        return LocalMicroFlowProofClassificationState::Unsupported;
    }
    if !input.claimed_path_not_truncated || !input.cap_omissions_outside_claimed_segment {
        return LocalMicroFlowProofClassificationState::Truncated;
    }
    if !input.backed_by_current_persisted_micro_facts
        || !input.endpoint_facts_current_and_claimable
        || !input.claimable_lifecycle_passport
    {
        return LocalMicroFlowProofClassificationState::Stale;
    }
    if input.local_returns_to_only {
        return LocalMicroFlowProofClassificationState::GraphRelationProofOnly;
    }
    if input.contains_claimable_graph_relation_summary {
        if input.source_spanned_proof_bearing_steps
            && input.derived_steps_have_provenance
            && input.exact_steps_preserve_exactness
        {
            LocalMicroFlowProofClassificationState::PartialMicroFlow
        } else {
            LocalMicroFlowProofClassificationState::Unknown
        }
    } else {
        LocalMicroFlowProofClassificationState::NoMicroFlowPathFound
    }
}

fn aggregate_packet_status(
    path_classifications: &[LocalMicroFlowPathProofClassification],
    fallback: crate::LocalMicroFlowPacketStatus,
) -> crate::LocalMicroFlowPacketStatus {
    if path_classifications.is_empty() {
        return fallback;
    }
    if path_classifications.iter().any(|classification| {
        classification.classification_state == LocalMicroFlowProofClassificationState::Corrupt
    }) {
        return crate::LocalMicroFlowPacketStatus::MicroFlowCorrupt;
    }
    if path_classifications.iter().any(|classification| {
        classification.classification_state == LocalMicroFlowProofClassificationState::Incompatible
    }) {
        return crate::LocalMicroFlowPacketStatus::MicroFlowIncompatible;
    }
    if path_classifications.iter().any(|classification| {
        classification.classification_state == LocalMicroFlowProofClassificationState::Stale
    }) {
        return crate::LocalMicroFlowPacketStatus::MicroFlowStale;
    }
    if path_classifications.iter().any(|classification| {
        classification.classification_state == LocalMicroFlowProofClassificationState::Truncated
    }) {
        return crate::LocalMicroFlowPacketStatus::MicroFlowTruncated;
    }
    if path_classifications.iter().all(|classification| {
        classification.classification_state == LocalMicroFlowProofClassificationState::Unsupported
    }) {
        return crate::LocalMicroFlowPacketStatus::MicroFlowUnsupported;
    }
    if path_classifications.iter().all(|classification| {
        classification.classification_state
            == LocalMicroFlowProofClassificationState::NoMicroFlowPathFound
    }) {
        return crate::LocalMicroFlowPacketStatus::NoMicroFlowPathFound;
    }
    let flow_count = path_classifications
        .iter()
        .filter(|classification| classification.flow_proof_eligible)
        .count();
    if flow_count == path_classifications.len() {
        crate::LocalMicroFlowPacketStatus::MicroFlowFound
    } else {
        crate::LocalMicroFlowPacketStatus::PartialMicroFlowFound
    }
}

fn aggregate_linter_state(
    path_classifications: &[LocalMicroFlowPathProofClassification],
    packet_status: crate::LocalMicroFlowPacketStatus,
) -> LocalMicroFlowProofClassificationState {
    if matches!(
        packet_status,
        crate::LocalMicroFlowPacketStatus::MicroFlowCorrupt
            | crate::LocalMicroFlowPacketStatus::MicroFlowIncompatible
    ) {
        return LocalMicroFlowProofClassificationState::Corrupt;
    }
    if matches!(
        packet_status,
        crate::LocalMicroFlowPacketStatus::MicroFlowStale
    ) {
        return LocalMicroFlowProofClassificationState::Stale;
    }
    if matches!(
        packet_status,
        crate::LocalMicroFlowPacketStatus::MicroFlowTruncated
    ) {
        return LocalMicroFlowProofClassificationState::Truncated;
    }
    if path_classifications
        .iter()
        .any(|classification| classification.flow_proof_eligible)
    {
        LocalMicroFlowProofClassificationState::FlowProof
    } else if path_classifications
        .iter()
        .any(|classification| classification.proof_strength == ProofLadderLevel::GraphRelationProof)
    {
        LocalMicroFlowProofClassificationState::GraphRelationProofOnly
    } else {
        LocalMicroFlowProofClassificationState::Unknown
    }
}

fn strongest_proof_strength(
    levels: impl IntoIterator<Item = ProofLadderLevel>,
) -> ProofLadderLevel {
    levels
        .into_iter()
        .max_by_key(|level| proof_strength_rank(*level))
        .unwrap_or(ProofLadderLevel::Unknown)
}

fn proof_strength_rank(level: ProofLadderLevel) -> u8 {
    match level {
        ProofLadderLevel::DiagnosticOnly => 0,
        ProofLadderLevel::Unsupported => 1,
        ProofLadderLevel::Unknown => 2,
        ProofLadderLevel::CandidateEvidence => 3,
        ProofLadderLevel::TextEvidence => 4,
        ProofLadderLevel::SourceNavigationEvidence => 5,
        ProofLadderLevel::SymbolEvidence => 6,
        ProofLadderLevel::GraphRelationProof => 7,
        ProofLadderLevel::MutationProof => 8,
        ProofLadderLevel::FlowProof => 9,
    }
}

fn linter_mapping_for_state(
    state: LocalMicroFlowProofClassificationState,
) -> LocalMicroFlowProofLinterMapping {
    let linter_class = match state {
        LocalMicroFlowProofClassificationState::Corrupt
        | LocalMicroFlowProofClassificationState::Incompatible => {
            LocalMicroFlowPacketLinterClass::PacketProofIntegrityFinding
        }
        LocalMicroFlowProofClassificationState::FlowProof
        | LocalMicroFlowProofClassificationState::GraphRelationProofOnly => {
            LocalMicroFlowPacketLinterClass::NormalSourceDelta
        }
        _ => LocalMicroFlowPacketLinterClass::UnsupportedDegradedState,
    };
    let validation_classification = linter_class.validation_classification();
    LocalMicroFlowProofLinterMapping {
        linter_class: local_micro_flow_packet_linter_class_str(linter_class).to_string(),
        validation_classification: validation_classification_str(validation_classification)
            .to_string(),
        hard_interrupt_available: false,
        must_fix_before_continuing: false,
        recommended_action_kind: match linter_class {
            LocalMicroFlowPacketLinterClass::PacketProofIntegrityFinding => "reindex_or_repair",
            LocalMicroFlowPacketLinterClass::UnsupportedDegradedState => "unsupported_or_unknown",
            LocalMicroFlowPacketLinterClass::NormalSourceDelta => "safe_to_continue",
        }
        .to_string(),
    }
}

fn local_micro_flow_packet_linter_class_str(
    linter_class: LocalMicroFlowPacketLinterClass,
) -> &'static str {
    match linter_class {
        LocalMicroFlowPacketLinterClass::NormalSourceDelta => "normal_source_delta",
        LocalMicroFlowPacketLinterClass::PacketProofIntegrityFinding => {
            "packet_proof_integrity_finding"
        }
        LocalMicroFlowPacketLinterClass::UnsupportedDegradedState => "unsupported_degraded_state",
    }
}

fn validation_classification_str(classification: crate::ValidationClassification) -> &'static str {
    match classification {
        crate::ValidationClassification::Block => "block",
        crate::ValidationClassification::Warn => "warn",
        crate::ValidationClassification::Unknown => "unknown",
        crate::ValidationClassification::Unsupported => "unsupported",
        crate::ValidationClassification::Degraded => "degraded",
        crate::ValidationClassification::Diagnostic => "diagnostic",
    }
}

fn validate_packet_edge(
    edge: &LocalMicroFlowPersistedEdgeFact,
    nodes_by_id: &BTreeMap<&str, &LocalMicroFlowPersistedNodeFact>,
    function_domain: &str,
    diagnostics: &mut Vec<LocalMicroFlowPacketCandidateDiagnostic>,
) {
    let Some(relation_kind) = edge.relation_kind else {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::unsupported(
            "unsupported_relation",
            Some(edge.micro_edge_id.clone()),
            format!(
                "unknown persisted micro-edge relation {}",
                edge.raw_relation_kind
            ),
        ));
        return;
    };
    if !mvp4_3_local_micro_flow_packet_supported_edge_kinds(edge.language.as_str())
        .contains(&relation_kind)
    {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::unsupported(
            "unsupported_relation",
            Some(edge.micro_edge_id.clone()),
            format!(
                "relation {relation_kind} is not supported by the active MVP4.3 packet adapter"
            ),
        ));
    }
    let Some(head) = nodes_by_id.get(edge.source_micro_node_id.as_str()).copied() else {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "missing_edge_endpoint",
            Some(edge.micro_edge_id.clone()),
            format!("missing source micro-node {}", edge.source_micro_node_id),
        ));
        return;
    };
    let Some(tail) = nodes_by_id.get(edge.target_micro_node_id.as_str()).copied() else {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "missing_edge_endpoint",
            Some(edge.micro_edge_id.clone()),
            format!("missing target micro-node {}", edge.target_micro_node_id),
        ));
        return;
    };
    if edge.source_span.is_none() {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "missing_source_span",
            Some(edge.micro_edge_id.clone()),
            "proof-bearing micro-edge is missing a source span",
        ));
    }
    if edge.provenance_id.is_none() {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "missing_provenance",
            Some(edge.micro_edge_id.clone()),
            "proof-bearing micro-edge is missing provenance",
        ));
    }
    if !same_function_domain(head.function_domain(), function_domain)
        || !same_function_domain(tail.function_domain(), function_domain)
        || !same_function_domain(edge.function_domain(), function_domain)
    {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "cross_function_edge",
            Some(edge.micro_edge_id.clone()),
            "micro-edge crosses the packet function ownership boundary",
        ));
    }
    let edge_file = normalize_repo_relative_path(&edge.file_id);
    if normalize_repo_relative_path(&head.file_id) != edge_file
        || normalize_repo_relative_path(&tail.file_id) != edge_file
    {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "cross_file_edge",
            Some(edge.micro_edge_id.clone()),
            "micro-edge endpoint file does not match edge file",
        ));
    }
    if !is_supported_packet_source(head) || !is_supported_packet_source(tail) {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::unsupported(
            "unsupported_endpoint_source",
            Some(edge.micro_edge_id.clone()),
            "edge endpoint source role or language is outside the active packet adapter scope",
        ));
    }
    if !edge.is_claimable() || !head.is_claimable() || !tail.is_claimable() {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "non_claimable_dependency",
            Some(edge.micro_edge_id.clone()),
            "micro-flow candidate dependency is not claimable",
        ));
    }
    if !edge_exactness_matches_relation(edge) {
        diagnostics.push(LocalMicroFlowPacketCandidateDiagnostic::integrity(
            "edge_exactness_mismatch",
            Some(edge.micro_edge_id.clone()),
            "micro-edge exactness does not match the active relation contract",
        ));
    }
}

fn edge_is_eligible(
    edge: &LocalMicroFlowPersistedEdgeFact,
    nodes_by_id: &BTreeMap<&str, &LocalMicroFlowPersistedNodeFact>,
    function_domain: &str,
) -> bool {
    let Some(relation_kind) = edge.relation_kind else {
        return false;
    };
    if !mvp4_3_local_micro_flow_packet_supported_edge_kinds(edge.language.as_str())
        .contains(&relation_kind)
        || edge.source_span.is_none()
        || edge.provenance_id.is_none()
        || !edge.is_claimable()
        || !edge_exactness_matches_relation(edge)
        || !mvp4_3_local_micro_flow_packet_source_supported(&edge.language, edge.source_role)
    {
        return false;
    }
    let (Some(head), Some(tail)) = (
        nodes_by_id.get(edge.source_micro_node_id.as_str()).copied(),
        nodes_by_id.get(edge.target_micro_node_id.as_str()).copied(),
    ) else {
        return false;
    };
    head.source_span.is_some()
        && tail.source_span.is_some()
        && is_supported_packet_source(head)
        && is_supported_packet_source(tail)
        && head.is_claimable()
        && tail.is_claimable()
        && same_function_domain(head.function_domain(), function_domain)
        && same_function_domain(tail.function_domain(), function_domain)
        && same_function_domain(edge.function_domain(), function_domain)
        && normalize_repo_relative_path(&edge.file_id)
            == normalize_repo_relative_path(&head.file_id)
        && normalize_repo_relative_path(&edge.file_id)
            == normalize_repo_relative_path(&tail.file_id)
}

fn edge_exactness_matches_relation(edge: &LocalMicroFlowPersistedEdgeFact) -> bool {
    match edge.relation_kind {
        Some(MicroEdgeKind::LocalFlowsTo) => {
            edge.exactness == MicroExactness::DerivedWithProvenance
        }
        Some(MicroEdgeKind::LocalReads)
        | Some(MicroEdgeKind::LocalWrites)
        | Some(MicroEdgeKind::LocalReturnsTo) => edge.exactness == MicroExactness::Exact,
        _ => false,
    }
}

fn is_supported_packet_source(node: &LocalMicroFlowPersistedNodeFact) -> bool {
    mvp4_3_local_micro_flow_packet_source_supported(&node.language, node.source_role)
}

fn same_function_domain(value: Option<&str>, expected: &str) -> bool {
    value.is_some_and(|value| value.trim() == expected)
}

fn is_allowed_packet_node(language: &str, kind: Option<MicroNodeKind>) -> bool {
    kind.is_some_and(|kind| {
        mvp4_3_local_micro_flow_packet_supported_node_kinds(language).contains(&kind)
    })
}

fn has_relation(edges: &[&LocalMicroFlowPersistedEdgeFact], kind: MicroEdgeKind) -> bool {
    edges.iter().any(|edge| edge.relation_kind == Some(kind))
}

fn edge_order_key(edge: &LocalMicroFlowPersistedEdgeFact) -> (u32, u32, u32) {
    edge.source_span
        .as_ref()
        .map(|span| {
            (
                span.start_line,
                span.start_column.unwrap_or(0),
                span.end_column.unwrap_or(0),
            )
        })
        .unwrap_or((u32::MAX, u32::MAX, u32::MAX))
}

fn node_refs_by_id(
    nodes: &[&LocalMicroFlowPersistedNodeFact],
) -> BTreeMap<String, LocalMicroFlowMicroNodeRef> {
    nodes
        .iter()
        .filter_map(|node| {
            let micro_kind = node.micro_kind?;
            let source_span = node.source_span.clone()?;
            Some((
                node.micro_node_id.clone(),
                LocalMicroFlowMicroNodeRef::new(
                    node.micro_node_id.clone(),
                    micro_kind,
                    source_span,
                    if node.function_entity_id.is_some() || node.scope_entity_id.is_some() {
                        "resolver_or_scope_linked"
                    } else {
                        "binding_unknown"
                    },
                    node.function_entity_id
                        .clone()
                        .or_else(|| node.scope_entity_id.clone()),
                ),
            ))
        })
        .collect()
}

fn edge_to_canonical_step(
    edge: &LocalMicroFlowPersistedEdgeFact,
    node_refs: &BTreeMap<String, LocalMicroFlowMicroNodeRef>,
    source_order: u32,
) -> Option<LocalMicroFlowCanonicalStep> {
    let relation_kind = edge.relation_kind?;
    let source_span = edge.source_span.clone()?;
    let edge_ref = LocalMicroFlowMicroEdgeRef::new(
        edge.micro_edge_id.clone(),
        relation_kind,
        source_span.clone(),
        edge.exactness,
        edge.provenance_id.clone(),
    );
    let mut micro_node_refs = Vec::new();
    if let Some(head) = node_refs.get(&edge.source_micro_node_id) {
        micro_node_refs.push(head.clone());
    }
    if let Some(tail) = node_refs.get(&edge.target_micro_node_id) {
        micro_node_refs.push(tail.clone());
    }
    let provenance = LocalMicroFlowProvenance {
        provenance_id: edge
            .provenance_id
            .clone()
            .unwrap_or_else(|| format!("prov:{}", edge.micro_edge_id)),
        derivation_kind: match relation_kind {
            MicroEdgeKind::LocalReads | MicroEdgeKind::LocalWrites => "resolver_backed_binding",
            MicroEdgeKind::LocalFlowsTo => "local_assignment_chain_derivation",
            MicroEdgeKind::LocalReturnsTo => "direct_ast_ownership",
            _ => "unsupported",
        }
        .to_string(),
        source_fact_ids: vec![edge.micro_edge_id.clone()],
        source_spans: vec![source_span.clone()],
        extractor_or_adapter_version: edge.extraction_version.clone(),
        exactness: edge.exactness,
        limitations: vec![
            "persisted_micro_edge_fact_only".to_string(),
            "no_packet_flow_proof_in_prompt_5".to_string(),
        ],
    };
    let return_path_identity = (relation_kind == MicroEdgeKind::LocalReturnsTo).then(|| {
        LocalMicroFlowReturnPathIdentity {
            return_path_id: format!("return-path:{}", edge.micro_edge_id),
            path_kind: "explicit_return_anchor".to_string(),
        }
    });
    Some(LocalMicroFlowCanonicalStep {
        step_id: format!("step:{}:{}", relation_kind.as_str(), edge.micro_edge_id),
        step_kind: match relation_kind {
            MicroEdgeKind::LocalReads => LocalMicroFlowStepKind::Read,
            MicroEdgeKind::LocalWrites => LocalMicroFlowStepKind::Write,
            MicroEdgeKind::LocalFlowsTo => LocalMicroFlowStepKind::Assignment,
            MicroEdgeKind::LocalReturnsTo => LocalMicroFlowStepKind::Return,
            _ => LocalMicroFlowStepKind::UnsupportedGap,
        },
        order_key: LocalMicroFlowOrderKey {
            source_order,
            structural_order: vec![edge.micro_edge_id.clone()],
            branch_order: None,
            return_path_order: return_path_identity
                .as_ref()
                .map(|identity| identity.return_path_id.clone()),
        },
        skeleton_text: format!(
            "{} from persisted micro-edge {}",
            micro_edge_kind_schema_value(relation_kind),
            edge.micro_edge_id
        ),
        micro_node_refs,
        micro_edge_refs: vec![edge_ref],
        source_spans: vec![source_span],
        provenance: vec![provenance],
        exactness: edge.exactness,
        claimability: edge.claimability.clone(),
        source_role: edge.source_role,
        branch_id: None,
        branch_identity: None,
        return_path_id: return_path_identity
            .as_ref()
            .map(|identity| identity.return_path_id.clone()),
        return_path_identity,
        gap: None,
        cap_omission: None,
        proof_contribution: ProofLadderLevel::GraphRelationProof,
        limitations: vec![
            "function_local_only".to_string(),
            "packet_candidate_not_persisted".to_string(),
        ],
    })
}

fn gap_to_canonical_step(
    gap: &LocalMicroFlowGap,
    source_order: u32,
) -> LocalMicroFlowCanonicalStep {
    LocalMicroFlowCanonicalStep {
        step_id: format!("step:gap:{}", gap.gap_id),
        step_kind: LocalMicroFlowStepKind::UnknownGap,
        order_key: LocalMicroFlowOrderKey {
            source_order,
            structural_order: vec![gap.gap_id.clone()],
            branch_order: None,
            return_path_order: None,
        },
        skeleton_text: gap.reason.clone(),
        micro_node_refs: Vec::new(),
        micro_edge_refs: Vec::new(),
        source_spans: gap.source_spans.clone(),
        provenance: Vec::new(),
        exactness: gap.exactness,
        claimability: "unknown".to_string(),
        source_role: MicroSourceRole::Production,
        branch_id: None,
        branch_identity: None,
        return_path_id: None,
        return_path_identity: None,
        gap: Some(gap.clone()),
        cap_omission: None,
        proof_contribution: ProofLadderLevel::Unknown,
        limitations: vec!["gap_breaks_complete_flow_proof".to_string()],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowFileRef {
    pub repo_relative_path: String,
    pub language: String,
    pub source_role: String,
    pub file_id: Option<String>,
    pub lifecycle_binding: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowFunctionIdentity {
    pub function_id: String,
    pub function_entity_id: Option<String>,
    pub function_name: Option<String>,
    pub identity_status: String,
    pub scope_path: Vec<String>,
    pub structural_path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowBudget {
    pub max_packet_steps: Option<u32>,
    pub max_packet_bytes: Option<u32>,
    pub omitted_count: u64,
    pub truncation_reason: String,
    pub cap_omissions: Vec<LocalMicroFlowCapOmission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowBudgetContract {
    pub mode: String,
    pub compact_default: bool,
    pub explain_audit_expansion: bool,
    pub handles_in_context_surfaces: bool,
    pub numeric_caps_set: bool,
    pub max_packet_steps: Option<u32>,
    pub max_packet_bytes: Option<u32>,
    pub budget_source: String,
    pub full_source_bodies_allowed: bool,
    pub candidate_rank_changes_proof_strength: bool,
}

impl LocalMicroFlowBudgetContract {
    pub fn compact_default() -> Self {
        Self {
            mode: "compact".to_string(),
            compact_default: true,
            explain_audit_expansion: true,
            handles_in_context_surfaces: true,
            numeric_caps_set: true,
            max_packet_steps: Some(LOCAL_MICRO_FLOW_MAX_PACKET_STEPS as u32),
            max_packet_bytes: Some(LOCAL_MICRO_FLOW_MAX_PACKET_BODY_BYTES),
            budget_source: "MVP4.3 packet builder cap policy; compact packet steps are capped before the 160 KiB sparse row body limit".to_string(),
            full_source_bodies_allowed: false,
            candidate_rank_changes_proof_strength: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowGenerationState {
    pub production_packet_generation_active: bool,
    pub parser_emission_active: bool,
    pub proof_strength_active: bool,
    pub context_entry_command_active: bool,
}

impl LocalMicroFlowGenerationState {
    pub const fn inactive() -> Self {
        Self {
            production_packet_generation_active: false,
            parser_emission_active: false,
            proof_strength_active: false,
            context_entry_command_active: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowCanonicalPath {
    pub path_id: String,
    pub branch_id: Option<String>,
    pub return_path_id: Option<String>,
    pub steps: Vec<LocalMicroFlowCanonicalStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowCanonicalStep {
    pub step_id: String,
    pub step_kind: LocalMicroFlowStepKind,
    pub order_key: LocalMicroFlowOrderKey,
    pub skeleton_text: String,
    pub micro_node_refs: Vec<LocalMicroFlowMicroNodeRef>,
    pub micro_edge_refs: Vec<LocalMicroFlowMicroEdgeRef>,
    pub source_spans: Vec<SourceSpan>,
    pub provenance: Vec<LocalMicroFlowProvenance>,
    pub exactness: MicroExactness,
    pub claimability: String,
    pub source_role: MicroSourceRole,
    pub branch_id: Option<String>,
    pub branch_identity: Option<LocalMicroFlowBranchIdentity>,
    pub return_path_id: Option<String>,
    pub return_path_identity: Option<LocalMicroFlowReturnPathIdentity>,
    pub gap: Option<LocalMicroFlowGap>,
    pub cap_omission: Option<LocalMicroFlowCapOmission>,
    pub proof_contribution: ProofLadderLevel,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalMicroFlowStepKind {
    Read,
    Write,
    Assignment,
    Call,
    Return,
    Mutation,
    Condition,
    Guard,
    Sanitizer,
    Assertion,
    Branch,
    UnknownGap,
    UnsupportedGap,
}

impl LocalMicroFlowStepKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Assignment => "assignment",
            Self::Call => "call",
            Self::Return => "return",
            Self::Mutation => "mutation",
            Self::Condition => "condition",
            Self::Guard => "guard",
            Self::Sanitizer => "sanitizer",
            Self::Assertion => "assertion",
            Self::Branch => "branch",
            Self::UnknownGap => "unknown_gap",
            Self::UnsupportedGap => "unsupported_gap",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowOrderKey {
    pub source_order: u32,
    pub structural_order: Vec<String>,
    pub branch_order: Option<String>,
    pub return_path_order: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowMicroNodeRef {
    pub micro_node_id: String,
    pub micro_kind: String,
    pub source_span: SourceSpan,
    pub binding_status: String,
    pub stable_binding_identity: Option<String>,
}

impl LocalMicroFlowMicroNodeRef {
    pub fn new(
        micro_node_id: impl Into<String>,
        micro_kind: MicroNodeKind,
        source_span: SourceSpan,
        binding_status: impl Into<String>,
        stable_binding_identity: Option<String>,
    ) -> Self {
        Self {
            micro_node_id: micro_node_id.into(),
            micro_kind: micro_kind.as_str().to_string(),
            source_span,
            binding_status: binding_status.into(),
            stable_binding_identity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowMicroEdgeRef {
    pub micro_edge_id: String,
    pub relation_kind: String,
    pub source_span: SourceSpan,
    pub exactness: MicroExactness,
    pub provenance_id: Option<String>,
}

impl LocalMicroFlowMicroEdgeRef {
    pub fn new(
        micro_edge_id: impl Into<String>,
        relation_kind: MicroEdgeKind,
        source_span: SourceSpan,
        exactness: MicroExactness,
        provenance_id: Option<String>,
    ) -> Self {
        Self {
            micro_edge_id: micro_edge_id.into(),
            relation_kind: micro_edge_kind_schema_value(relation_kind).to_string(),
            source_span,
            exactness,
            provenance_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowProvenance {
    pub provenance_id: String,
    pub derivation_kind: String,
    pub source_fact_ids: Vec<String>,
    pub source_spans: Vec<SourceSpan>,
    pub extractor_or_adapter_version: String,
    pub exactness: MicroExactness,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowBranchIdentity {
    pub branch_id: String,
    pub branch_kind: String,
    pub branch_label: String,
    pub condition_micro_node_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowReturnPathIdentity {
    pub return_path_id: String,
    pub path_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowGap {
    pub gap_id: String,
    pub gap_kind: String,
    pub reason: String,
    pub exactness: MicroExactness,
    pub proof_status: String,
    pub source_spans: Vec<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowCapOmission {
    pub omission_id: String,
    pub omitted_count: u64,
    pub reason: String,
    pub completeness_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowExpansionHandle {
    pub handle: String,
    pub handle_kind: String,
    pub available: bool,
    pub proof_boundary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictV1PacketBody {
    pub schema_version: u32,
    pub payload_version: u32,
    pub codec_version: String,
    pub dictionary: DictV1Dictionary,
    pub paths: Vec<DictV1Path>,
    pub compression_contract: DictV1CompressionContract,
}

impl DictV1PacketBody {
    pub fn to_ordered_steps(
        &self,
    ) -> Result<Vec<LocalMicroFlowAuditStep>, LocalMicroFlowCodecError> {
        let mut ordered = Vec::new();
        for path in &self.paths {
            let branch = path
                .branch_id
                .as_ref()
                .and_then(|id| self.dictionary.branch_identities.get(id))
                .cloned();
            let return_path = path
                .return_path_id
                .as_ref()
                .and_then(|id| self.dictionary.return_path_identities.get(id))
                .cloned();

            for (offset, compact_ref) in path.steps.iter().enumerate() {
                let Some(step) = self.dictionary.steps.get(compact_ref.ref_id()) else {
                    return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                        "missing compact step {}",
                        compact_ref.ref_id()
                    )));
                };
                let spans = resolve_refs(&step.span_refs, &self.dictionary.spans, "span")?;
                let node_refs = resolve_refs(&step.node_refs, &self.dictionary.nodes, "node")?;
                let edge_refs = resolve_refs(&step.edge_refs, &self.dictionary.edges, "edge")?;
                let provenance = resolve_refs(
                    &step.provenance_refs,
                    &self.dictionary.provenance,
                    "provenance",
                )?;
                let skeleton_text = self
                    .dictionary
                    .labels
                    .get(&step.skeleton_label_ref)
                    .cloned()
                    .ok_or_else(|| {
                        LocalMicroFlowCodecError::InvalidPacket(format!(
                            "missing label {}",
                            step.skeleton_label_ref
                        ))
                    })?;
                let exactness = self
                    .dictionary
                    .exactness
                    .get(&step.exactness_ref)
                    .copied()
                    .ok_or_else(|| {
                        LocalMicroFlowCodecError::InvalidPacket(format!(
                            "missing exactness {}",
                            step.exactness_ref
                        ))
                    })?;
                let claimability = self
                    .dictionary
                    .claimability
                    .get(&step.claimability_ref)
                    .cloned()
                    .ok_or_else(|| {
                        LocalMicroFlowCodecError::InvalidPacket(format!(
                            "missing claimability {}",
                            step.claimability_ref
                        ))
                    })?;
                let source_role = self
                    .dictionary
                    .source_roles
                    .get(&step.source_role_ref)
                    .copied()
                    .ok_or_else(|| {
                        LocalMicroFlowCodecError::InvalidPacket(format!(
                            "missing source role {}",
                            step.source_role_ref
                        ))
                    })?;
                let gap = step
                    .gap_ref
                    .as_ref()
                    .map(|id| {
                        self.dictionary.gaps.get(id).cloned().ok_or_else(|| {
                            LocalMicroFlowCodecError::InvalidPacket(format!("missing gap {id}"))
                        })
                    })
                    .transpose()?;
                let cap_omission = step
                    .cap_omission_ref
                    .as_ref()
                    .map(|id| {
                        self.dictionary
                            .cap_omissions
                            .get(id)
                            .cloned()
                            .ok_or_else(|| {
                                LocalMicroFlowCodecError::InvalidPacket(format!(
                                    "missing cap omission {id}"
                                ))
                            })
                    })
                    .transpose()?;

                ordered.push(LocalMicroFlowAuditStep {
                    step_number: ordered.len(),
                    path_id: path.path_id.clone(),
                    branch_id: path.branch_id.clone(),
                    return_path_id: path.return_path_id.clone(),
                    step_id: step.step_id.clone(),
                    step_kind: step.step_kind,
                    skeleton_text,
                    micro_node_refs: node_refs,
                    micro_edge_refs: edge_refs,
                    source_spans: spans,
                    exactness,
                    claimability,
                    provenance,
                    source_role,
                    proof_contribution: step.proof_contribution,
                    branch_identity: step.branch_identity.clone().or_else(|| branch.clone()),
                    return_path_identity: step
                        .return_path_identity
                        .clone()
                        .or_else(|| return_path.clone()),
                    gap,
                    cap_omission,
                    limitations: step.limitations.clone(),
                    compact_path_offset: offset,
                });
            }
        }
        Ok(ordered)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictV1Dictionary {
    pub spans: BTreeMap<String, SourceSpan>,
    pub nodes: BTreeMap<String, LocalMicroFlowMicroNodeRef>,
    pub edges: BTreeMap<String, LocalMicroFlowMicroEdgeRef>,
    pub provenance: BTreeMap<String, LocalMicroFlowProvenance>,
    pub labels: BTreeMap<String, String>,
    pub steps: BTreeMap<String, DictV1Step>,
    pub exactness: BTreeMap<String, MicroExactness>,
    pub claimability: BTreeMap<String, String>,
    pub source_roles: BTreeMap<String, MicroSourceRole>,
    pub branch_identities: BTreeMap<String, LocalMicroFlowBranchIdentity>,
    pub return_path_identities: BTreeMap<String, LocalMicroFlowReturnPathIdentity>,
    pub gaps: BTreeMap<String, LocalMicroFlowGap>,
    pub cap_omissions: BTreeMap<String, LocalMicroFlowCapOmission>,
    pub proof_metadata: BTreeMap<String, ProofLadderLevel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictV1Step {
    pub step_id: String,
    pub step_kind: LocalMicroFlowStepKind,
    pub order_key: LocalMicroFlowOrderKey,
    pub skeleton_label_ref: String,
    pub node_refs: Vec<String>,
    pub edge_refs: Vec<String>,
    pub span_refs: Vec<String>,
    pub provenance_refs: Vec<String>,
    pub exactness_ref: String,
    pub claimability_ref: String,
    pub source_role_ref: String,
    pub branch_ref: Option<String>,
    pub return_path_ref: Option<String>,
    pub gap_ref: Option<String>,
    pub cap_omission_ref: Option<String>,
    pub proof_metadata_ref: String,
    pub proof_contribution: ProofLadderLevel,
    pub branch_identity: Option<LocalMicroFlowBranchIdentity>,
    pub return_path_identity: Option<LocalMicroFlowReturnPathIdentity>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictV1Path {
    pub path_id: String,
    pub branch_id: Option<String>,
    pub return_path_id: Option<String>,
    pub steps: Vec<DictV1CompactStepRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictV1CompactStepRef(
    pub String,
    pub String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub Option<String>,
);

impl DictV1CompactStepRef {
    fn from_step(step: &DictV1Step) -> Self {
        let ref_kind = match step.step_kind {
            LocalMicroFlowStepKind::UnknownGap | LocalMicroFlowStepKind::UnsupportedGap => "gap",
            LocalMicroFlowStepKind::Branch => "branch",
            LocalMicroFlowStepKind::Return => {
                if step.return_path_ref.is_some() {
                    "return_path"
                } else {
                    "node"
                }
            }
            _ => "node",
        };
        Self(
            ref_kind.to_string(),
            step.step_id.clone(),
            step.branch_ref.clone(),
            step.return_path_ref.clone(),
        )
    }

    fn ref_id(&self) -> &str {
        &self.1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictV1CompressionContract {
    pub lossless_to_audit_ordered_steps: bool,
    pub source_spans_preserved: bool,
    pub provenance_preserved: bool,
    pub exactness_preserved: bool,
    pub source_roles_preserved: bool,
    pub branch_and_return_paths_preserved: bool,
    pub shadowed_binding_identity_preserved: bool,
    pub unknown_gaps_preserved: bool,
    pub cap_omissions_preserved: bool,
    pub packet_level_proof_status_preserved: bool,
    pub per_step_proof_metadata_preserved: bool,
    pub full_source_bodies_allowed: bool,
    pub verbose_ordered_steps_default_inline: bool,
}

impl DictV1CompressionContract {
    pub const fn strict() -> Self {
        Self {
            lossless_to_audit_ordered_steps: true,
            source_spans_preserved: true,
            provenance_preserved: true,
            exactness_preserved: true,
            source_roles_preserved: true,
            branch_and_return_paths_preserved: true,
            shadowed_binding_identity_preserved: true,
            unknown_gaps_preserved: true,
            cap_omissions_preserved: true,
            packet_level_proof_status_preserved: true,
            per_step_proof_metadata_preserved: true,
            full_source_bodies_allowed: false,
            verbose_ordered_steps_default_inline: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowAuditStep {
    pub step_number: usize,
    pub path_id: String,
    pub branch_id: Option<String>,
    pub return_path_id: Option<String>,
    pub step_id: String,
    pub step_kind: LocalMicroFlowStepKind,
    pub skeleton_text: String,
    pub micro_node_refs: Vec<LocalMicroFlowMicroNodeRef>,
    pub micro_edge_refs: Vec<LocalMicroFlowMicroEdgeRef>,
    pub source_spans: Vec<SourceSpan>,
    pub exactness: MicroExactness,
    pub claimability: String,
    pub provenance: Vec<LocalMicroFlowProvenance>,
    pub source_role: MicroSourceRole,
    pub proof_contribution: ProofLadderLevel,
    pub branch_identity: Option<LocalMicroFlowBranchIdentity>,
    pub return_path_identity: Option<LocalMicroFlowReturnPathIdentity>,
    pub gap: Option<LocalMicroFlowGap>,
    pub cap_omission: Option<LocalMicroFlowCapOmission>,
    pub limitations: Vec<String>,
    pub compact_path_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalMicroFlowCodecError {
    InvalidPacket(String),
    Serde(String),
}

impl std::fmt::Display for LocalMicroFlowCodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPacket(message) => formatter.write_str(message),
            Self::Serde(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for LocalMicroFlowCodecError {}

impl From<serde_json::Error> for LocalMicroFlowCodecError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serde(error.to_string())
    }
}

struct DictV1Encoder {
    spans: InternPool<SourceSpan>,
    nodes: InternPool<LocalMicroFlowMicroNodeRef>,
    edges: InternPool<LocalMicroFlowMicroEdgeRef>,
    provenance: InternPool<LocalMicroFlowProvenance>,
    labels: InternPool<String>,
    exactness: InternPool<MicroExactness>,
    claimability: InternPool<String>,
    source_roles: InternPool<MicroSourceRole>,
    branch_identities: InternPool<LocalMicroFlowBranchIdentity>,
    return_path_identities: InternPool<LocalMicroFlowReturnPathIdentity>,
    gaps: InternPool<LocalMicroFlowGap>,
    cap_omissions: InternPool<LocalMicroFlowCapOmission>,
    proof_metadata: InternPool<ProofLadderLevel>,
    steps: BTreeMap<String, DictV1Step>,
}

impl DictV1Encoder {
    fn encode(
        packet: &LocalMicroFlowCanonicalPacket,
    ) -> Result<DictV1PacketBody, LocalMicroFlowCodecError> {
        let mut encoder = Self::default();
        let mut paths = packet.paths.clone();
        paths.sort_by(|left, right| left.path_id.cmp(&right.path_id));

        let mut dict_paths = Vec::new();
        for path in paths {
            let branch_ref = path.branch_id.as_ref().map(|id| {
                let branch = path
                    .steps
                    .iter()
                    .filter_map(|step| step.branch_identity.as_ref())
                    .find(|branch| &branch.branch_id == id)
                    .cloned()
                    .unwrap_or_else(|| LocalMicroFlowBranchIdentity {
                        branch_id: id.clone(),
                        branch_kind: "unknown".to_string(),
                        branch_label: id.clone(),
                        condition_micro_node_id: None,
                    });
                encoder.branch_identities.intern(branch)
            });
            let return_path_ref = path.return_path_id.as_ref().map(|id| {
                let return_path = path
                    .steps
                    .iter()
                    .filter_map(|step| step.return_path_identity.as_ref())
                    .find(|return_path| &return_path.return_path_id == id)
                    .cloned()
                    .unwrap_or_else(|| LocalMicroFlowReturnPathIdentity {
                        return_path_id: id.clone(),
                        path_kind: "unknown".to_string(),
                    });
                encoder.return_path_identities.intern(return_path)
            });

            let mut steps = path.steps;
            steps.sort_by(|left, right| {
                left.order_key
                    .source_order
                    .cmp(&right.order_key.source_order)
                    .then_with(|| left.step_id.cmp(&right.step_id))
            });

            let mut compact_step_refs = Vec::new();
            for step in steps {
                encoder.validate_step(&step)?;
                let dict_step = encoder.encode_step(step)?;
                compact_step_refs.push(DictV1CompactStepRef::from_step(&dict_step));
                encoder.steps.insert(dict_step.step_id.clone(), dict_step);
            }

            dict_paths.push(DictV1Path {
                path_id: path.path_id,
                branch_id: branch_ref,
                return_path_id: return_path_ref,
                steps: compact_step_refs,
            });
        }

        Ok(DictV1PacketBody {
            schema_version: MVP4_3_LOCAL_MICRO_FLOW_PACKET_SCHEMA_VERSION,
            payload_version: MVP4_3_LOCAL_MICRO_FLOW_PACKET_PAYLOAD_VERSION,
            codec_version: LOCAL_MICRO_FLOW_DICT_V1_CODEC_VERSION.to_string(),
            dictionary: encoder.finish_dictionary(),
            paths: dict_paths,
            compression_contract: DictV1CompressionContract::strict(),
        })
    }

    fn validate_step(
        &self,
        step: &LocalMicroFlowCanonicalStep,
    ) -> Result<(), LocalMicroFlowCodecError> {
        reject_oversized_or_source_body(
            "step_id",
            &step.step_id,
            LOCAL_MICRO_FLOW_MAX_LABEL_BYTES,
        )?;
        reject_oversized_or_source_body(
            "skeleton_text",
            &step.skeleton_text,
            LOCAL_MICRO_FLOW_MAX_SKELETON_TEXT_BYTES,
        )?;
        reject_oversized_or_source_body(
            "claimability",
            &step.claimability,
            LOCAL_MICRO_FLOW_MAX_LABEL_BYTES,
        )?;
        for limitation in &step.limitations {
            reject_oversized_or_source_body("limitation", limitation, 400)?;
        }
        if step.source_spans.is_empty() && step.proof_contribution.is_graph_proof_level() {
            return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                "proof-bearing step {} is missing source spans",
                step.step_id
            )));
        }
        if step.provenance.is_empty() && step.proof_contribution.is_graph_proof_level() {
            return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                "proof-bearing step {} is missing provenance",
                step.step_id
            )));
        }
        if step.branch_id.is_some() && step.branch_identity.is_none() {
            return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                "step {} has branch_id without branch identity",
                step.step_id
            )));
        }
        if step.return_path_id.is_some() && step.return_path_identity.is_none() {
            return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                "step {} has return_path_id without return path identity",
                step.step_id
            )));
        }
        Ok(())
    }

    fn encode_step(
        &mut self,
        step: LocalMicroFlowCanonicalStep,
    ) -> Result<DictV1Step, LocalMicroFlowCodecError> {
        let span_refs = step
            .source_spans
            .into_iter()
            .map(|span| self.spans.intern(span))
            .collect();
        let node_refs = step
            .micro_node_refs
            .into_iter()
            .map(|node| self.nodes.intern(node))
            .collect();
        let edge_refs = step
            .micro_edge_refs
            .into_iter()
            .map(|edge| self.edges.intern(edge))
            .collect();
        let provenance_refs = step
            .provenance
            .into_iter()
            .map(|provenance| self.provenance.intern(provenance))
            .collect();
        let skeleton_label_ref = self.labels.intern(step.skeleton_text);
        let exactness_ref = self.exactness.intern(step.exactness);
        let claimability_ref = self.claimability.intern(step.claimability);
        let source_role_ref = self.source_roles.intern(step.source_role);
        let proof_metadata_ref = self.proof_metadata.intern(step.proof_contribution);
        let branch_ref = step
            .branch_identity
            .clone()
            .map(|branch| self.branch_identities.intern(branch));
        let return_path_ref = step
            .return_path_identity
            .clone()
            .map(|return_path| self.return_path_identities.intern(return_path));
        let gap_ref = step.gap.map(|gap| self.gaps.intern(gap));
        let cap_omission_ref = step
            .cap_omission
            .map(|cap_omission| self.cap_omissions.intern(cap_omission));

        Ok(DictV1Step {
            step_id: step.step_id,
            step_kind: step.step_kind,
            order_key: step.order_key,
            skeleton_label_ref,
            node_refs,
            edge_refs,
            span_refs,
            provenance_refs,
            exactness_ref,
            claimability_ref,
            source_role_ref,
            branch_ref,
            return_path_ref,
            gap_ref,
            cap_omission_ref,
            proof_metadata_ref,
            proof_contribution: step.proof_contribution,
            branch_identity: step.branch_identity,
            return_path_identity: step.return_path_identity,
            limitations: step.limitations,
        })
    }

    fn finish_dictionary(self) -> DictV1Dictionary {
        DictV1Dictionary {
            spans: self.spans.entries,
            nodes: self.nodes.entries,
            edges: self.edges.entries,
            provenance: self.provenance.entries,
            labels: self.labels.entries,
            steps: self.steps,
            exactness: self.exactness.entries,
            claimability: self.claimability.entries,
            source_roles: self.source_roles.entries,
            branch_identities: self.branch_identities.entries,
            return_path_identities: self.return_path_identities.entries,
            gaps: self.gaps.entries,
            cap_omissions: self.cap_omissions.entries,
            proof_metadata: self.proof_metadata.entries,
        }
    }
}

impl Default for DictV1Encoder {
    fn default() -> Self {
        Self {
            spans: InternPool::new("s"),
            nodes: InternPool::new("n"),
            edges: InternPool::new("e"),
            provenance: InternPool::new("p"),
            labels: InternPool::new("l"),
            exactness: InternPool::new("x"),
            claimability: InternPool::new("c"),
            source_roles: InternPool::new("r"),
            branch_identities: InternPool::new("b"),
            return_path_identities: InternPool::new("rp"),
            gaps: InternPool::new("g"),
            cap_omissions: InternPool::new("o"),
            proof_metadata: InternPool::new("pm"),
            steps: BTreeMap::new(),
        }
    }
}

struct InternPool<T> {
    prefix: &'static str,
    entries: BTreeMap<String, T>,
    keys: BTreeMap<String, String>,
}

impl<T> InternPool<T>
where
    T: Clone + Serialize,
{
    fn new(prefix: &'static str) -> Self {
        Self {
            prefix,
            entries: BTreeMap::new(),
            keys: BTreeMap::new(),
        }
    }

    fn intern(&mut self, value: T) -> String {
        let key = serde_json::to_string(&value).expect("dict_v1 values serialize");
        if let Some(existing) = self.keys.get(&key) {
            return existing.clone();
        }
        let id = format!("{}{}", self.prefix, self.entries.len());
        self.keys.insert(key, id.clone());
        self.entries.insert(id.clone(), value);
        id
    }
}

pub fn validate_local_micro_flow_agent_json_contract(
    value: &Value,
) -> Result<(), LocalMicroFlowCodecError> {
    let object = value.as_object().ok_or_else(|| {
        LocalMicroFlowCodecError::InvalidPacket("packet must be a JSON object".to_string())
    })?;

    for required in [
        "packet_kind",
        "encoding",
        "packet_body",
        "claimability",
        "proof_status",
        "proof_strength",
        "budget_contract",
    ] {
        if !object.contains_key(required) {
            return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                "missing required field {required}"
            )));
        }
    }

    if object.get("encoding").and_then(Value::as_str) != Some(LOCAL_MICRO_FLOW_DICT_V1_ENCODING) {
        return Err(LocalMicroFlowCodecError::InvalidPacket(
            "encoding must be dict_v1".to_string(),
        ));
    }
    if contains_forbidden_full_source_body_field(value) {
        return Err(LocalMicroFlowCodecError::InvalidPacket(
            "full source body fields are forbidden".to_string(),
        ));
    }

    let packet_body = object
        .get("packet_body")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            LocalMicroFlowCodecError::InvalidPacket("packet_body must be an object".to_string())
        })?;
    let compression_contract = packet_body
        .get("compression_contract")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            LocalMicroFlowCodecError::InvalidPacket(
                "packet_body.compression_contract is required".to_string(),
            )
        })?;
    for (field, expected) in [
        ("lossless_to_audit_ordered_steps", true),
        ("source_spans_preserved", true),
        ("provenance_preserved", true),
        ("exactness_preserved", true),
        ("source_roles_preserved", true),
        ("branch_and_return_paths_preserved", true),
        ("shadowed_binding_identity_preserved", true),
        ("unknown_gaps_preserved", true),
        ("cap_omissions_preserved", true),
        ("full_source_bodies_allowed", false),
        ("verbose_ordered_steps_default_inline", false),
    ] {
        if compression_contract.get(field).and_then(Value::as_bool) != Some(expected) {
            return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                "compression contract field {field} must be {expected}"
            )));
        }
    }

    let dictionary = packet_body
        .get("dictionary")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            LocalMicroFlowCodecError::InvalidPacket(
                "packet_body.dictionary is required".to_string(),
            )
        })?;
    let steps = dictionary
        .get("steps")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            LocalMicroFlowCodecError::InvalidPacket(
                "packet_body.dictionary.steps is required".to_string(),
            )
        })?;
    let paths = packet_body
        .get("paths")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            LocalMicroFlowCodecError::InvalidPacket("packet_body.paths is required".to_string())
        })?;
    let branches = dictionary
        .get("branch_identities")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let return_paths = dictionary
        .get("return_path_identities")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    for path in paths {
        let path_object = path.as_object().ok_or_else(|| {
            LocalMicroFlowCodecError::InvalidPacket("packet path must be an object".to_string())
        })?;
        if let Some(branch_id) = path_object.get("branch_id").and_then(Value::as_str) {
            if !branches.contains_key(branch_id) {
                return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                    "path references missing branch identity {branch_id}"
                )));
            }
        }
        if let Some(return_path_id) = path_object.get("return_path_id").and_then(Value::as_str) {
            if !return_paths.contains_key(return_path_id) {
                return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                    "path references missing return path identity {return_path_id}"
                )));
            }
        }
    }

    for (step_id, step) in steps {
        let step_object = step.as_object().ok_or_else(|| {
            LocalMicroFlowCodecError::InvalidPacket(format!("step {step_id} must be an object"))
        })?;
        let proof_contribution = step_object
            .get("proof_contribution")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let proof_bearing = matches!(
            proof_contribution,
            "graph_relation_proof" | "mutation_proof" | "flow_proof"
        );
        if proof_bearing {
            if step_object
                .get("span_refs")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty)
            {
                return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                    "proof-bearing step {step_id} is missing source span refs"
                )));
            }
            if step_object
                .get("provenance_refs")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty)
            {
                return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                    "proof-bearing step {step_id} is missing provenance refs"
                )));
            }
        }
        if let Some(branch_ref) = step_object.get("branch_ref").and_then(Value::as_str) {
            if !branches.contains_key(branch_ref) {
                return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                    "step {step_id} references missing branch identity {branch_ref}"
                )));
            }
        }
        if let Some(return_path_ref) = step_object.get("return_path_ref").and_then(Value::as_str) {
            if !return_paths.contains_key(return_path_ref) {
                return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
                    "step {step_id} references missing return path identity {return_path_ref}"
                )));
            }
        }
    }

    if let Some(ordered_steps) = object.get("ordered_steps") {
        for step in ordered_steps.as_array().ok_or_else(|| {
            LocalMicroFlowCodecError::InvalidPacket("ordered_steps must be an array".to_string())
        })? {
            let step_object = step.as_object().ok_or_else(|| {
                LocalMicroFlowCodecError::InvalidPacket(
                    "ordered step must be an object".to_string(),
                )
            })?;
            let proof_contribution = step_object
                .get("proof_contribution")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let proof_bearing = matches!(
                proof_contribution,
                "graph_relation_proof" | "mutation_proof" | "flow_proof"
            );
            if proof_bearing {
                if step_object
                    .get("source_spans")
                    .and_then(Value::as_array)
                    .is_none_or(Vec::is_empty)
                {
                    return Err(LocalMicroFlowCodecError::InvalidPacket(
                        "proof-bearing ordered step missing source spans".to_string(),
                    ));
                }
                if step_object
                    .get("provenance")
                    .and_then(Value::as_array)
                    .is_none_or(Vec::is_empty)
                {
                    return Err(LocalMicroFlowCodecError::InvalidPacket(
                        "proof-bearing ordered step missing provenance".to_string(),
                    ));
                }
            }
        }
    }

    Ok(())
}

fn reject_oversized_or_source_body(
    field: &str,
    value: &str,
    max_bytes: usize,
) -> Result<(), LocalMicroFlowCodecError> {
    if value.len() > max_bytes {
        return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
            "{field} exceeds {max_bytes} bytes"
        )));
    }
    if value.contains('\n') || value.contains('\r') {
        return Err(LocalMicroFlowCodecError::InvalidPacket(format!(
            "{field} must not contain full source bodies"
        )));
    }
    Ok(())
}

fn contains_forbidden_full_source_body_field(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            matches!(
                key.as_str(),
                "full_source_body" | "source_body" | "source_code_body"
            ) || contains_forbidden_full_source_body_field(value)
        }),
        Value::Array(values) => values.iter().any(contains_forbidden_full_source_body_field),
        _ => false,
    }
}

fn resolve_refs<T>(
    refs: &[String],
    dictionary: &BTreeMap<String, T>,
    label: &str,
) -> Result<Vec<T>, LocalMicroFlowCodecError>
where
    T: Clone,
{
    refs.iter()
        .map(|id| {
            dictionary.get(id).cloned().ok_or_else(|| {
                LocalMicroFlowCodecError::InvalidPacket(format!("missing {label} ref {id}"))
            })
        })
        .collect()
}

fn proof_ladder_level_str(level: ProofLadderLevel) -> &'static str {
    match level {
        ProofLadderLevel::TextEvidence => "text_evidence",
        ProofLadderLevel::SymbolEvidence => "symbol_evidence",
        ProofLadderLevel::CandidateEvidence => "candidate_evidence",
        ProofLadderLevel::SourceNavigationEvidence => "source_navigation_evidence",
        ProofLadderLevel::GraphRelationProof => "graph_relation_proof",
        ProofLadderLevel::MutationProof => "mutation_proof",
        ProofLadderLevel::FlowProof => "flow_proof",
        ProofLadderLevel::Unknown => "unknown",
        ProofLadderLevel::Unsupported => "unsupported",
        ProofLadderLevel::DiagnosticOnly => "diagnostic_only",
    }
}

fn proof_status_schema_value(
    packet_status: crate::LocalMicroFlowPacketStatus,
    proof_strength: ProofLadderLevel,
) -> &'static str {
    match (packet_status, proof_strength) {
        (crate::LocalMicroFlowPacketStatus::MicroFlowFound, ProofLadderLevel::FlowProof) => {
            "flow_proof"
        }
        (crate::LocalMicroFlowPacketStatus::MicroFlowFound, _) => "claimable_local_flow",
        (crate::LocalMicroFlowPacketStatus::PartialMicroFlowFound, _) => "partial_local_flow",
        (crate::LocalMicroFlowPacketStatus::NoMicroFlowPathFound, _) => "unknown",
        (crate::LocalMicroFlowPacketStatus::MicroFlowUnavailable, _) => "unavailable",
        (crate::LocalMicroFlowPacketStatus::MicroFlowTruncated, _) => "truncated",
        (crate::LocalMicroFlowPacketStatus::MicroFlowUnsupported, _) => "unsupported",
        (crate::LocalMicroFlowPacketStatus::MicroFlowStale, _) => "stale",
        (
            crate::LocalMicroFlowPacketStatus::MicroFlowCorrupt
            | crate::LocalMicroFlowPacketStatus::MicroFlowIncompatible,
            _,
        ) => "non_claimable",
    }
}

fn micro_edge_kind_schema_value(kind: MicroEdgeKind) -> &'static str {
    match kind {
        MicroEdgeKind::LocalReads => "LOCAL_READS",
        MicroEdgeKind::LocalWrites => "LOCAL_WRITES",
        MicroEdgeKind::LocalFlowsTo => "LOCAL_FLOWS_TO",
        MicroEdgeKind::LocalCalls => "LOCAL_CALLS",
        MicroEdgeKind::LocalReturnsTo => "LOCAL_RETURNS_TO",
        MicroEdgeKind::LocalMutates => "LOCAL_MUTATES",
        MicroEdgeKind::LocalChecks => "LOCAL_CHECKS",
        MicroEdgeKind::LocalSanitizes => "LOCAL_SANITIZES",
        MicroEdgeKind::LocalGuards => "LOCAL_GUARDS",
        MicroEdgeKind::LocalAsserts => "LOCAL_ASSERTS",
        MicroEdgeKind::LocalBranchesTo => "LOCAL_BRANCHES_TO",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalMicroFlowPacketStatus;
    use std::collections::BTreeSet;

    fn sample_span(start_column: u32, end_column: u32) -> SourceSpan {
        SourceSpan::with_columns("src/auth.ts", 3, start_column, 3, end_column)
    }

    fn provenance(id: &str, exactness: MicroExactness) -> LocalMicroFlowProvenance {
        LocalMicroFlowProvenance {
            provenance_id: id.to_string(),
            derivation_kind: "local_assignment_chain_derivation".to_string(),
            source_fact_ids: vec!["micro-edge://read-token".to_string()],
            source_spans: vec![sample_span(10, 15)],
            extractor_or_adapter_version: MVP4_3_LOCAL_MICRO_FLOW_PACKET_EXTRACTION_VERSION
                .to_string(),
            exactness,
            limitations: vec!["function_local_only".to_string()],
        }
    }

    fn node(id: &str, kind: MicroNodeKind, start: u32, end: u32) -> LocalMicroFlowMicroNodeRef {
        LocalMicroFlowMicroNodeRef::new(
            id,
            kind,
            sample_span(start, end),
            "resolver_proven",
            Some(format!("binding://{id}")),
        )
    }

    fn edge(id: &str, kind: MicroEdgeKind, start: u32, end: u32) -> LocalMicroFlowMicroEdgeRef {
        LocalMicroFlowMicroEdgeRef::new(
            id,
            kind,
            sample_span(start, end),
            MicroExactness::DerivedWithProvenance,
            Some("prov-flow".to_string()),
        )
    }

    fn step(
        step_id: &str,
        kind: LocalMicroFlowStepKind,
        source_order: u32,
        branch: Option<LocalMicroFlowBranchIdentity>,
        return_path: Option<LocalMicroFlowReturnPathIdentity>,
    ) -> LocalMicroFlowCanonicalStep {
        LocalMicroFlowCanonicalStep {
            step_id: step_id.to_string(),
            step_kind: kind,
            order_key: LocalMicroFlowOrderKey {
                source_order,
                structural_order: vec![format!("stmt:{source_order}")],
                branch_order: branch.as_ref().map(|branch| branch.branch_id.clone()),
                return_path_order: return_path
                    .as_ref()
                    .map(|return_path| return_path.return_path_id.clone()),
            },
            skeleton_text: format!("step {source_order} uses resolver-backed micro facts"),
            micro_node_refs: vec![
                node(
                    "micro-node://binding-token",
                    MicroNodeKind::LocalBinding,
                    10,
                    15,
                ),
                node(
                    "micro-node://binding-result",
                    MicroNodeKind::LocalBinding,
                    20,
                    26,
                ),
            ],
            micro_edge_refs: vec![edge(
                "micro-edge://flow-token-result",
                MicroEdgeKind::LocalFlowsTo,
                10,
                26,
            )],
            source_spans: vec![sample_span(10, 26)],
            provenance: vec![provenance(
                "prov-flow",
                MicroExactness::DerivedWithProvenance,
            )],
            exactness: MicroExactness::DerivedWithProvenance,
            claimability: "claimable_local_flow".to_string(),
            source_role: MicroSourceRole::Production,
            branch_id: branch.as_ref().map(|branch| branch.branch_id.clone()),
            branch_identity: branch,
            return_path_id: return_path
                .as_ref()
                .map(|return_path| return_path.return_path_id.clone()),
            return_path_identity: return_path,
            gap: None,
            cap_omission: None,
            proof_contribution: ProofLadderLevel::GraphRelationProof,
            limitations: vec!["no_runtime_value_claim".to_string()],
        }
    }

    fn gap_step() -> LocalMicroFlowCanonicalStep {
        LocalMicroFlowCanonicalStep {
            step_id: "step-gap-dynamic-member".to_string(),
            step_kind: LocalMicroFlowStepKind::UnknownGap,
            order_key: LocalMicroFlowOrderKey {
                source_order: 7,
                structural_order: vec!["stmt:7".to_string()],
                branch_order: None,
                return_path_order: None,
            },
            skeleton_text: "dynamic member target unknown".to_string(),
            micro_node_refs: Vec::new(),
            micro_edge_refs: Vec::new(),
            source_spans: vec![sample_span(30, 42)],
            provenance: vec![provenance("prov-gap", MicroExactness::Unknown)],
            exactness: MicroExactness::Unknown,
            claimability: "unknown".to_string(),
            source_role: MicroSourceRole::Production,
            branch_id: None,
            branch_identity: None,
            return_path_id: None,
            return_path_identity: None,
            gap: Some(LocalMicroFlowGap {
                gap_id: "gap-dynamic-member".to_string(),
                gap_kind: "dynamic_lookup".to_string(),
                reason: "computed member target is not resolver-proven".to_string(),
                exactness: MicroExactness::Unknown,
                proof_status: "unknown".to_string(),
                source_spans: vec![sample_span(30, 42)],
            }),
            cap_omission: Some(LocalMicroFlowCapOmission {
                omission_id: "cap-omission-1".to_string(),
                omitted_count: 3,
                reason: "max_packet_steps_exceeded".to_string(),
                completeness_label: "incomplete_after_truncation".to_string(),
            }),
            proof_contribution: ProofLadderLevel::Unknown,
            limitations: vec!["gap breaks complete flow_proof".to_string()],
        }
    }

    fn sample_packet(repeat_count: usize) -> LocalMicroFlowCanonicalPacket {
        let branch = LocalMicroFlowBranchIdentity {
            branch_id: "branch-if-then".to_string(),
            branch_kind: "if_consequent".to_string(),
            branch_label: "then".to_string(),
            condition_micro_node_id: Some("micro-node://condition".to_string()),
        };
        let return_path = LocalMicroFlowReturnPathIdentity {
            return_path_id: "return-path-explicit-1".to_string(),
            path_kind: "explicit_return".to_string(),
        };
        let mut steps = Vec::new();
        for index in 0..repeat_count {
            steps.push(step(
                &format!("step-flow-{index}"),
                LocalMicroFlowStepKind::Assignment,
                index as u32,
                Some(branch.clone()),
                Some(return_path.clone()),
            ));
        }
        steps.push(gap_step());

        LocalMicroFlowCanonicalPacket {
            packet_id: "micro-packet://auth-login-flow".to_string(),
            file: LocalMicroFlowFileRef {
                repo_relative_path: "src/auth.ts".to_string(),
                language: "typescript".to_string(),
                source_role: MicroSourceRole::Production.as_str().to_string(),
                file_id: Some("file://src/auth.ts".to_string()),
                lifecycle_binding: Some("passport:test".to_string()),
            },
            function_identity: LocalMicroFlowFunctionIdentity {
                function_id: "function://login".to_string(),
                function_entity_id: Some("repo://e/auth-login".to_string()),
                function_name: Some("login".to_string()),
                identity_status: "stable_identity_guarantee".to_string(),
                scope_path: vec!["function:login".to_string()],
                structural_path: vec!["program".to_string(), "function_declaration:0".to_string()],
            },
            function_span: SourceSpan::with_columns("src/auth.ts", 1, 1, 8, 2),
            claimability: "claimable_local_flow".to_string(),
            packet_status: LocalMicroFlowPacketStatus::PartialMicroFlowFound,
            proof_strength: ProofLadderLevel::GraphRelationProof,
            budget: LocalMicroFlowBudget {
                max_packet_steps: Some(128),
                max_packet_bytes: Some(16_384),
                omitted_count: 3,
                truncation_reason: "max_steps_exceeded".to_string(),
                cap_omissions: vec![LocalMicroFlowCapOmission {
                    omission_id: "cap-omission-1".to_string(),
                    omitted_count: 3,
                    reason: "max_packet_steps_exceeded".to_string(),
                    completeness_label: "incomplete_after_truncation".to_string(),
                }],
            },
            risks_limitations: vec!["function_local_only".to_string()],
            unknown_unsupported_gaps: vec![LocalMicroFlowGap {
                gap_id: "gap-dynamic-member".to_string(),
                gap_kind: "dynamic_lookup".to_string(),
                reason: "computed member target is not resolver-proven".to_string(),
                exactness: MicroExactness::Unknown,
                proof_status: "unknown".to_string(),
                source_spans: vec![sample_span(30, 42)],
            }],
            expansion_handles: vec![LocalMicroFlowExpansionHandle {
                handle: "expand://micro-packet/auth-login-flow/audit".to_string(),
                handle_kind: "audit_trace".to_string(),
                available: true,
                proof_boundary: "audit expansion renders compact dict_v1 facts only".to_string(),
            }],
            paths: vec![LocalMicroFlowCanonicalPath {
                path_id: "path-main".to_string(),
                branch_id: Some(branch.branch_id),
                return_path_id: Some(return_path.return_path_id),
                steps,
            }],
        }
    }

    fn persisted_node(
        id: &str,
        kind: MicroNodeKind,
        symbol: Option<&str>,
        start: u32,
    ) -> LocalMicroFlowPersistedNodeFact {
        LocalMicroFlowPersistedNodeFact {
            micro_node_id: id.to_string(),
            file_id: "src/auth.ts".to_string(),
            function_entity_id: Some("function://login".to_string()),
            scope_entity_id: Some("scope://login".to_string()),
            micro_kind: Some(kind),
            raw_micro_kind: kind.as_str().to_string(),
            symbol: symbol.map(str::to_string),
            source_span: Some(SourceSpan::with_columns(
                "src/auth.ts",
                3,
                start,
                3,
                start + 3,
            )),
            exactness: MicroExactness::Exact,
            provenance_id: Some(format!("prov-node-{id}")),
            source_role: MicroSourceRole::Production,
            language: "typescript".to_string(),
            schema_version: 1,
            payload_version: 1,
            extraction_version: "mvp4.1-typescript-micro-nodes-v1".to_string(),
            claimability: format!("claimable_source_spanned_{}", kind.as_str()),
            lifecycle_binding: "db_passport".to_string(),
        }
    }

    fn persisted_edge(
        id: &str,
        relation: MicroEdgeKind,
        head: &str,
        tail: &str,
        start: u32,
    ) -> LocalMicroFlowPersistedEdgeFact {
        let exactness = if relation == MicroEdgeKind::LocalFlowsTo {
            MicroExactness::DerivedWithProvenance
        } else {
            MicroExactness::Exact
        };
        LocalMicroFlowPersistedEdgeFact {
            micro_edge_id: id.to_string(),
            file_id: "src/auth.ts".to_string(),
            function_entity_id: Some("function://login".to_string()),
            scope_entity_id: Some("scope://login".to_string()),
            source_micro_node_id: head.to_string(),
            target_micro_node_id: tail.to_string(),
            relation_kind: Some(relation),
            raw_relation_kind: relation.as_str().to_string(),
            source_span: Some(SourceSpan::with_columns(
                "src/auth.ts",
                4,
                start,
                4,
                start + 5,
            )),
            exactness,
            provenance_id: Some(format!("prov-edge-{id}")),
            source_role: MicroSourceRole::Production,
            language: "typescript".to_string(),
            frontend: "tree-sitter-typescript".to_string(),
            schema_version: 1,
            payload_version: 1,
            extraction_version: match relation {
                MicroEdgeKind::LocalReads => "mvp4.2b-typescript-local-reads-v1",
                MicroEdgeKind::LocalWrites => "mvp4.2b-typescript-local-writes-v1",
                MicroEdgeKind::LocalFlowsTo => "mvp4.2b-typescript-local-flows-to-v1",
                MicroEdgeKind::LocalReturnsTo => "mvp4.2-typescript-local-returns-to-v1",
                _ => "unsupported",
            }
            .to_string(),
            claimability: format!("claimable_source_spanned_{}", relation.as_str()),
            lifecycle_binding: "db_passport".to_string(),
        }
    }

    fn packet_candidate_nodes() -> Vec<LocalMicroFlowPersistedNodeFact> {
        vec![
            persisted_node("fn-login", MicroNodeKind::FunctionFrame, Some("login"), 1),
            persisted_node("param-seed", MicroNodeKind::Parameter, Some("seed"), 5),
            persisted_node("assign-total", MicroNodeKind::AssignmentSite, None, 9),
            persisted_node("use-seed", MicroNodeKind::ValueUse, Some("seed"), 13),
            persisted_node(
                "local-total",
                MicroNodeKind::LocalBinding,
                Some("total"),
                17,
            ),
            persisted_node("return-total", MicroNodeKind::ReturnSite, None, 21),
        ]
    }

    fn complete_packet_candidate_input() -> LocalMicroFlowPersistedFactInput {
        LocalMicroFlowPersistedFactInput::new(
            "src/auth.ts",
            packet_candidate_nodes(),
            vec![
                persisted_edge(
                    "read-seed",
                    MicroEdgeKind::LocalReads,
                    "use-seed",
                    "param-seed",
                    3,
                ),
                persisted_edge(
                    "write-total",
                    MicroEdgeKind::LocalWrites,
                    "assign-total",
                    "local-total",
                    9,
                ),
                persisted_edge(
                    "flow-seed-total",
                    MicroEdgeKind::LocalFlowsTo,
                    "param-seed",
                    "local-total",
                    13,
                ),
                persisted_edge(
                    "return-total-login",
                    MicroEdgeKind::LocalReturnsTo,
                    "return-total",
                    "fn-login",
                    21,
                ),
            ],
        )
    }

    fn complete_packet_candidate_packet() -> LocalMicroFlowCanonicalPacket {
        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(
            &complete_packet_candidate_input(),
        );
        result.candidates[0]
            .packet
            .clone()
            .expect("complete packet")
    }

    fn first_path_classification(
        packet: &LocalMicroFlowCanonicalPacket,
    ) -> LocalMicroFlowPathProofClassification {
        packet.proof_classification().path_classifications[0].clone()
    }

    #[test]
    fn mvp4_3_packet_candidate_uses_persisted_rows_without_storage_activation() {
        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(
            &complete_packet_candidate_input(),
        );
        assert_eq!(result.raw_ast_truth_bypass_count, 0);
        assert_eq!(result.local_flow_packet_rows_emitted(), 0);
        assert!(!result.flow_proof_activated());
        assert_eq!(result.candidates.len(), 1);

        let candidate = &result.candidates[0];
        assert_eq!(
            candidate.packet_status,
            LocalMicroFlowPacketStatus::MicroFlowFound
        );
        assert_eq!(candidate.proof_strength, ProofLadderLevel::FlowProof);
        let packet = candidate.packet.as_ref().expect("candidate packet");
        assert_eq!(packet.paths[0].steps.len(), 4);
        let classification = packet.proof_classification();
        assert_eq!(classification.flow_proof_path_count, 1);
        assert_eq!(classification.non_flow_path_count, 0);
        assert_eq!(
            classification.strongest_path_state,
            LocalMicroFlowProofClassificationState::FlowProof
        );
        assert!(classification.packet_summary_no_overclaim);
        assert!(classification.dict_v1_lossless_to_audit_ordered_steps);
        let compact = packet.to_agent_json(false).expect("compact packet");
        assert_eq!(compact["proof_strength"], "flow_proof");
        assert_eq!(
            compact["generation_state"]["proof_strength_active"],
            serde_json::json!(false)
        );
        assert!(compact.get("ordered_steps").is_none());
    }

    #[test]
    fn mvp4_3_packet_candidate_detects_missing_endpoint_span_and_provenance() {
        let mut input = complete_packet_candidate_input();
        input.edges = vec![
            persisted_edge(
                "missing-endpoint",
                MicroEdgeKind::LocalReads,
                "use-seed",
                "missing-local",
                3,
            ),
            {
                let mut edge = persisted_edge(
                    "missing-span",
                    MicroEdgeKind::LocalWrites,
                    "assign-total",
                    "local-total",
                    9,
                );
                edge.source_span = None;
                edge
            },
            {
                let mut edge = persisted_edge(
                    "missing-provenance",
                    MicroEdgeKind::LocalFlowsTo,
                    "param-seed",
                    "local-total",
                    13,
                );
                edge.provenance_id = None;
                edge
            },
        ];
        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        assert_eq!(result.missing_endpoint_count, 1);
        assert_eq!(result.missing_span_count, 1);
        assert_eq!(result.missing_provenance_count, 1);
        assert_eq!(
            result.candidates[0].packet_status,
            LocalMicroFlowPacketStatus::MicroFlowCorrupt
        );
        assert!(result.candidates[0].packet.is_none());
    }

    #[test]
    fn mvp4_3_packet_candidate_excludes_non_production_source_role() {
        let mut input = complete_packet_candidate_input();
        for node in &mut input.nodes {
            node.source_role = MicroSourceRole::Test;
        }
        for edge in &mut input.edges {
            edge.source_role = MicroSourceRole::Test;
        }
        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        assert_eq!(
            result.candidates[0].packet_status,
            LocalMicroFlowPacketStatus::MicroFlowUnsupported
        );
        assert!(result.candidates[0].packet.is_none());
    }

    #[test]
    fn mvp4_3_returns_to_only_candidate_remains_below_flow_proof() {
        let input = LocalMicroFlowPersistedFactInput::new(
            "src/auth.ts",
            packet_candidate_nodes(),
            vec![persisted_edge(
                "return-total-login",
                MicroEdgeKind::LocalReturnsTo,
                "return-total",
                "fn-login",
                21,
            )],
        );
        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        let candidate = &result.candidates[0];
        assert_eq!(
            candidate.packet_status,
            LocalMicroFlowPacketStatus::PartialMicroFlowFound
        );
        assert_eq!(
            candidate.proof_strength,
            ProofLadderLevel::GraphRelationProof
        );
        assert_eq!(candidate.gap_count, 1);
        let packet = candidate.packet.as_ref().expect("partial packet");
        assert!(packet
            .unknown_unsupported_gaps
            .iter()
            .any(|gap| gap.gap_kind == "missing_local_flows_to"));
    }

    #[test]
    fn mvp4_3_proof_classification_downgrades_incomplete_chain_and_returns_only() {
        let mut incomplete = complete_packet_candidate_input();
        incomplete
            .edges
            .retain(|edge| edge.relation_kind != Some(MicroEdgeKind::LocalFlowsTo));
        let incomplete_result =
            build_local_micro_flow_packet_candidates_from_persisted_facts(&incomplete);
        let incomplete_packet = incomplete_result.candidates[0]
            .packet
            .as_ref()
            .expect("incomplete packet");
        let incomplete_path = first_path_classification(incomplete_packet);
        assert!(!incomplete_path.flow_proof_eligible);
        assert_eq!(
            incomplete_path.classification_state,
            LocalMicroFlowProofClassificationState::PartialMicroFlow
        );
        assert!(incomplete_path
            .missing_requirements
            .contains(&"includes_local_flows_to".to_string()));

        let returns_only = LocalMicroFlowPersistedFactInput::new(
            "src/auth.ts",
            packet_candidate_nodes(),
            vec![persisted_edge(
                "return-total-login",
                MicroEdgeKind::LocalReturnsTo,
                "return-total",
                "fn-login",
                21,
            )],
        );
        let returns_only_result =
            build_local_micro_flow_packet_candidates_from_persisted_facts(&returns_only);
        let returns_only_packet = returns_only_result.candidates[0]
            .packet
            .as_ref()
            .expect("returns-only packet");
        let returns_only_path = first_path_classification(returns_only_packet);
        assert!(!returns_only_path.flow_proof_eligible);
        assert_eq!(
            returns_only_path.classification_state,
            LocalMicroFlowProofClassificationState::GraphRelationProofOnly
        );
        assert_eq!(returns_only_path.flow_step_count, 0);
        assert_eq!(returns_only_path.return_step_count, 1);
    }

    #[test]
    fn mvp4_3_proof_classification_downgrades_missing_provenance_span_and_stale_facts() {
        let mut missing_provenance = complete_packet_candidate_packet();
        let flow_step = missing_provenance.paths[0]
            .steps
            .iter_mut()
            .find(|step| {
                step.micro_edge_refs
                    .iter()
                    .any(|edge| edge.relation_kind == "LOCAL_FLOWS_TO")
            })
            .expect("flow step");
        flow_step.provenance.clear();
        let missing_provenance_path = first_path_classification(&missing_provenance);
        assert!(!missing_provenance_path.flow_proof_eligible);
        assert_eq!(
            missing_provenance_path.classification_state,
            LocalMicroFlowProofClassificationState::Unknown
        );
        assert!(missing_provenance_path
            .missing_requirements
            .contains(&"derived_steps_have_provenance".to_string()));

        let mut missing_span = complete_packet_candidate_packet();
        missing_span.paths[0].steps[0].source_spans.clear();
        let missing_span_path = first_path_classification(&missing_span);
        assert!(!missing_span_path.flow_proof_eligible);
        assert_eq!(
            missing_span_path.classification_state,
            LocalMicroFlowProofClassificationState::Unknown
        );
        assert!(missing_span_path
            .missing_requirements
            .contains(&"source_spanned_proof_bearing_steps".to_string()));

        let mut stale = complete_packet_candidate_packet();
        stale.paths[0].steps[0].claimability = "non_claimable_stale_edge".to_string();
        let stale_path = first_path_classification(&stale);
        assert!(!stale_path.flow_proof_eligible);
        assert_eq!(
            stale_path.classification_state,
            LocalMicroFlowProofClassificationState::Stale
        );
    }

    #[test]
    fn mvp4_3_proof_classification_downgrades_branch_return_shadow_dynamic_cap_and_recovery() {
        let mut branch_input = complete_packet_candidate_input();
        branch_input.nodes.push(persisted_node(
            "return-early",
            MicroNodeKind::ReturnSite,
            None,
            7,
        ));
        branch_input.edges.push(persisted_edge(
            "return-early-login",
            MicroEdgeKind::LocalReturnsTo,
            "return-early",
            "fn-login",
            7,
        ));
        let branch_result =
            build_local_micro_flow_packet_candidates_from_persisted_facts(&branch_input);
        let branch_packet = branch_result.candidates[0]
            .packet
            .as_ref()
            .expect("branch packet");
        let branch_path = first_path_classification(branch_packet);
        assert!(!branch_path.flow_proof_eligible);
        assert!(branch_path
            .missing_requirements
            .contains(&"branch_identity_preserved".to_string()));

        let mut return_path_collapsed = complete_packet_candidate_packet();
        return_path_collapsed.paths[0].return_path_id = None;
        let return_path = first_path_classification(&return_path_collapsed);
        assert!(!return_path.flow_proof_eligible);
        assert!(return_path
            .missing_requirements
            .contains(&"return_path_identity_preserved".to_string()));

        let mut shadow_collapsed = complete_packet_candidate_packet();
        let duplicate_node = shadow_collapsed.paths[0].steps[0].micro_node_refs[0].clone();
        shadow_collapsed.paths[0].steps[0]
            .micro_node_refs
            .push(duplicate_node);
        let shadow_path = first_path_classification(&shadow_collapsed);
        assert!(!shadow_path.flow_proof_eligible);
        assert!(shadow_path
            .missing_requirements
            .contains(&"shadowed_binding_identity_preserved".to_string()));

        let mut dynamic_input = complete_packet_candidate_input();
        dynamic_input.nodes.push(persisted_node(
            "call-dynamic",
            MicroNodeKind::CallSite,
            Some("lookup"),
            25,
        ));
        let dynamic_result =
            build_local_micro_flow_packet_candidates_from_persisted_facts(&dynamic_input);
        let dynamic_packet = dynamic_result.candidates[0]
            .packet
            .as_ref()
            .expect("dynamic packet");
        let dynamic_path = first_path_classification(dynamic_packet);
        assert!(!dynamic_path.flow_proof_eligible);
        assert!(dynamic_path
            .missing_requirements
            .contains(&"gaps_outside_claimed_path".to_string()));

        let mut capped = complete_packet_candidate_input();
        capped.node_cap_omitted_count = 1;
        let capped_result = build_local_micro_flow_packet_candidates_from_persisted_facts(&capped);
        let capped_packet = capped_result.candidates[0]
            .packet
            .as_ref()
            .expect("capped packet");
        let capped_path = first_path_classification(capped_packet);
        assert!(!capped_path.flow_proof_eligible);
        assert_eq!(
            capped_path.classification_state,
            LocalMicroFlowProofClassificationState::Truncated
        );

        let mut recovery = complete_packet_candidate_packet();
        let gap = LocalMicroFlowGap {
            gap_id: "gap-parser-recovery".to_string(),
            gap_kind: "parser_recovery".to_string(),
            reason: "parser recovery intersects the claimed local path".to_string(),
            exactness: MicroExactness::Unknown,
            proof_status: "unknown".to_string(),
            source_spans: vec![sample_span(40, 44)],
        };
        recovery.unknown_unsupported_gaps.push(gap.clone());
        recovery.paths[0]
            .steps
            .push(gap_to_canonical_step(&gap, 99));
        let recovery_path = first_path_classification(&recovery);
        assert!(!recovery_path.flow_proof_eligible);
        assert!(recovery_path
            .missing_requirements
            .contains(&"gaps_outside_claimed_path".to_string()));
    }

    #[test]
    fn mvp4_3_proof_classification_packet_summary_scopes_mixed_paths_and_linter_mapping() {
        let mut packet = complete_packet_candidate_packet();
        let mut unknown_path = packet.paths[0].clone();
        unknown_path.path_id = "path-with-dynamic-gap".to_string();
        unknown_path.return_path_id = Some("return-path:dynamic-gap".to_string());
        let gap = LocalMicroFlowGap {
            gap_id: "gap-dynamic-target".to_string(),
            gap_kind: "dynamic_target".to_string(),
            reason: "dynamic target is preserved as an unknown gap".to_string(),
            exactness: MicroExactness::Unknown,
            proof_status: "unknown".to_string(),
            source_spans: vec![sample_span(45, 52)],
        };
        unknown_path.steps.push(gap_to_canonical_step(&gap, 99));
        packet.unknown_unsupported_gaps.push(gap);
        packet.paths.push(unknown_path);

        let classification = packet.proof_classification();
        assert_eq!(classification.flow_proof_path_count, 1);
        assert_eq!(classification.non_flow_path_count, 1);
        assert_eq!(
            classification.packet_status,
            LocalMicroFlowPacketStatus::PartialMicroFlowFound
        );
        assert_eq!(
            classification.packet_proof_strength,
            ProofLadderLevel::FlowProof
        );
        assert!(classification.packet_summary_no_overclaim);
        assert_eq!(
            classification.linter_mapping.recommended_action_kind,
            "safe_to_continue"
        );

        let mut corrupt_packet = complete_packet_candidate_packet();
        let corrupt_gap = LocalMicroFlowGap {
            gap_id: "gap-missing-provenance".to_string(),
            gap_kind: "missing_provenance".to_string(),
            reason: "proof-bearing step lacks provenance".to_string(),
            exactness: MicroExactness::Unknown,
            proof_status: "non_claimable".to_string(),
            source_spans: vec![sample_span(50, 55)],
        };
        corrupt_packet.paths[0]
            .steps
            .push(gap_to_canonical_step(&corrupt_gap, 100));
        let corrupt_classification = corrupt_packet.proof_classification();
        assert_eq!(
            corrupt_classification.packet_status,
            LocalMicroFlowPacketStatus::MicroFlowCorrupt
        );
        assert_eq!(
            corrupt_classification
                .linter_mapping
                .recommended_action_kind,
            "reindex_or_repair"
        );
        assert!(
            !corrupt_classification
                .linter_mapping
                .hard_interrupt_available
        );
    }

    #[test]
    fn mvp4_3_packet_candidate_handles_no_flow_and_multiple_assignments() {
        let no_flow = LocalMicroFlowPersistedFactInput::new(
            "src/auth.ts",
            packet_candidate_nodes(),
            Vec::new(),
        );
        let no_flow_result =
            build_local_micro_flow_packet_candidates_from_persisted_facts(&no_flow);
        assert_eq!(
            no_flow_result.candidates[0].packet_status,
            LocalMicroFlowPacketStatus::NoMicroFlowPathFound
        );
        assert!(no_flow_result.candidates[0]
            .packet
            .as_ref()
            .expect("no-flow diagnostic packet")
            .unknown_unsupported_gaps
            .iter()
            .any(|gap| gap.gap_kind == "no_micro_flow_path_found"));

        let mut multi = complete_packet_candidate_input();
        multi.nodes.push(persisted_node(
            "assign-final",
            MicroNodeKind::AssignmentSite,
            None,
            29,
        ));
        multi.nodes.push(persisted_node(
            "local-final",
            MicroNodeKind::LocalBinding,
            Some("finalValue"),
            33,
        ));
        multi.edges.push(persisted_edge(
            "write-final",
            MicroEdgeKind::LocalWrites,
            "assign-final",
            "local-final",
            29,
        ));
        multi.edges.push(persisted_edge(
            "flow-total-final",
            MicroEdgeKind::LocalFlowsTo,
            "local-total",
            "local-final",
            33,
        ));
        let multi_result = build_local_micro_flow_packet_candidates_from_persisted_facts(&multi);
        let packet = multi_result.candidates[0]
            .packet
            .as_ref()
            .expect("multi packet");
        assert!(packet.paths[0].steps.len() >= 6);
        assert!(packet.paths[0]
            .steps
            .iter()
            .any(|step| step.step_id.contains("write-final")));
        assert!(packet.paths[0]
            .steps
            .iter()
            .any(|step| step.step_id.contains("flow-total-final")));
        let step_ids = packet.paths[0]
            .steps
            .iter()
            .map(|step| step.step_id.as_str())
            .collect::<Vec<_>>();
        let first_flow = step_ids
            .iter()
            .position(|step_id| step_id.contains("flow-seed-total"))
            .expect("first flow step");
        let second_flow = step_ids
            .iter()
            .position(|step_id| step_id.contains("flow-total-final"))
            .expect("second flow step");
        assert!(first_flow < second_flow);
    }

    #[test]
    fn mvp4_3_packet_candidate_keeps_p2_sized_chain_flow_proof() {
        let mut input = complete_packet_candidate_input();
        let mut previous = "local-total".to_string();
        for index in 0..58 {
            let node_id = format!("local-p2-chain-{index}");
            input.nodes.push(persisted_node(
                &node_id,
                MicroNodeKind::LocalBinding,
                Some("value"),
                40 + index as u32,
            ));
            input.edges.push(persisted_edge(
                &format!("flow-p2-chain-{index}"),
                MicroEdgeKind::LocalFlowsTo,
                &previous,
                &node_id,
                40 + index as u32,
            ));
            previous = node_id;
        }

        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        let candidate = &result.candidates[0];
        assert_eq!(
            candidate.packet_status,
            LocalMicroFlowPacketStatus::MicroFlowFound
        );
        assert_eq!(candidate.proof_strength, ProofLadderLevel::FlowProof);
        let packet = candidate.packet.as_ref().expect("P2-sized packet");
        assert_eq!(packet.budget.omitted_count, 0);
        assert!(packet.budget.cap_omissions.is_empty());
        let step_count = packet
            .paths
            .iter()
            .map(|path| path.steps.len())
            .sum::<usize>();
        assert_eq!(step_count, 62, "P2-sized chain must not be capped");
        assert!(
            step_count <= LOCAL_MICRO_FLOW_MAX_PACKET_STEPS,
            "step_count={step_count}"
        );
        let classification = packet.proof_classification();
        assert_eq!(classification.flow_proof_path_count, 1);
        assert_eq!(
            classification.strongest_path_state,
            LocalMicroFlowProofClassificationState::FlowProof
        );
    }

    #[test]
    fn mvp4_3_packet_candidate_flow_edge_can_stand_for_write_side() {
        let mut input = complete_packet_candidate_input();
        input
            .edges
            .retain(|edge| edge.relation_kind != Some(MicroEdgeKind::LocalWrites));

        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        let candidate = &result.candidates[0];
        assert_eq!(
            candidate.packet_status,
            LocalMicroFlowPacketStatus::MicroFlowFound
        );
        assert_eq!(candidate.proof_strength, ProofLadderLevel::FlowProof);
        let packet = candidate
            .packet
            .as_ref()
            .expect("packet without write step");
        let classification = packet.proof_classification();
        assert_eq!(classification.flow_proof_path_count, 1);
        assert_eq!(classification.path_classifications[0].write_step_count, 0);
        assert_eq!(
            classification.strongest_path_state,
            LocalMicroFlowProofClassificationState::FlowProof
        );
    }

    #[test]
    fn mvp4_3_packet_candidate_reports_layer_cap_language_and_bypass_boundaries() {
        let mut stale = complete_packet_candidate_input();
        stale.edge_layer_current = false;
        let stale_result = build_local_micro_flow_packet_candidates_from_persisted_facts(&stale);
        assert_eq!(
            stale_result.candidates[0].packet_status,
            LocalMicroFlowPacketStatus::MicroFlowStale
        );
        assert!(stale_result.candidates[0].packet.is_none());

        let mut capped = complete_packet_candidate_input();
        capped.node_cap_omitted_count = 2;
        capped.edge_cap_omitted_count = 1;
        let capped_result = build_local_micro_flow_packet_candidates_from_persisted_facts(&capped);
        let capped_packet = capped_result.candidates[0]
            .packet
            .as_ref()
            .expect("capped packet");
        assert_eq!(
            capped_result.candidates[0].packet_status,
            LocalMicroFlowPacketStatus::MicroFlowTruncated
        );
        assert_eq!(capped_packet.budget.omitted_count, 3);
        assert_eq!(capped_packet.budget.cap_omissions.len(), 2);
        assert!(capped_packet
            .unknown_unsupported_gaps
            .iter()
            .any(|gap| gap.gap_kind == "cap_omission"));
        assert!(capped_packet.paths[0]
            .steps
            .iter()
            .any(|step| step.cap_omission.is_some()));

        let mut unsupported_language = complete_packet_candidate_input();
        for node in &mut unsupported_language.nodes {
            node.language = "python".to_string();
        }
        for edge in &mut unsupported_language.edges {
            edge.language = "python".to_string();
        }
        let unsupported_result =
            build_local_micro_flow_packet_candidates_from_persisted_facts(&unsupported_language);
        assert_eq!(
            unsupported_result.candidates[0].packet_status,
            LocalMicroFlowPacketStatus::MicroFlowUnsupported
        );
        assert_eq!(
            unsupported_result.candidates[0].proof_strength,
            ProofLadderLevel::DiagnosticOnly
        );
        assert!(!unsupported_result.flow_proof_activated());
        assert!(unsupported_result.candidates[0]
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == "unsupported_or_unknown"));

        let mut bypass = complete_packet_candidate_input();
        bypass.raw_ast_truth_bypass_count = 1;
        let bypass_result = build_local_micro_flow_packet_candidates_from_persisted_facts(&bypass);
        assert_eq!(bypass_result.raw_ast_truth_bypass_count, 1);
        assert!(bypass_result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.diagnostic_kind == "raw_ast_truth_bypass"));
    }

    #[test]
    fn mvp4_3_packet_candidate_caps_packet_steps_before_sparse_row_limit() {
        let mut input = complete_packet_candidate_input();
        let mut previous = "local-total".to_string();
        for index in 0..(LOCAL_MICRO_FLOW_MAX_PACKET_STEPS + 40) {
            let node_id = format!("local-extra-{index}");
            input.nodes.push(persisted_node(
                &node_id,
                MicroNodeKind::LocalBinding,
                Some("total"),
                40 + index as u32,
            ));
            input.edges.push(persisted_edge(
                &format!("flow-extra-{index}"),
                MicroEdgeKind::LocalFlowsTo,
                &previous,
                &node_id,
                40 + index as u32,
            ));
            previous = node_id;
        }

        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        let candidate = &result.candidates[0];
        assert_eq!(
            candidate.packet_status,
            LocalMicroFlowPacketStatus::MicroFlowTruncated
        );
        let packet = candidate.packet.as_ref().expect("capped packet");
        let step_count = packet
            .paths
            .iter()
            .map(|path| path.steps.len())
            .sum::<usize>();
        assert!(
            step_count <= LOCAL_MICRO_FLOW_MAX_PACKET_STEPS,
            "step_count={step_count}"
        );
        assert_eq!(
            packet.budget.max_packet_steps,
            Some(LOCAL_MICRO_FLOW_MAX_PACKET_STEPS as u32)
        );
        assert_eq!(
            packet.budget.max_packet_bytes,
            Some(LOCAL_MICRO_FLOW_MAX_PACKET_BODY_BYTES)
        );
        assert!(packet.budget.omitted_count > 0);
        assert_eq!(packet.budget.truncation_reason, "packet_step_cap_omission");
        assert!(packet
            .budget
            .cap_omissions
            .iter()
            .any(|omission| omission.omission_id.ends_with(":packet-steps")));
        assert!(packet
            .unknown_unsupported_gaps
            .iter()
            .any(|gap| gap.gap_kind == "cap_omission"));
        assert!(packet
            .paths
            .iter()
            .any(|path| { path.steps.iter().any(|step| step.cap_omission.is_some()) }));
        let body = packet.encode_dict_v1().expect("dict_v1 body");
        let body_json = serde_json::to_string(&body).expect("serialized dict_v1 body");
        assert!(
            body_json.len() <= LOCAL_MICRO_FLOW_MAX_PACKET_BODY_BYTES as usize,
            "body_json_len={}",
            body_json.len()
        );
    }

    #[test]
    fn mvp4_3_packet_candidate_caps_packet_body_bytes_before_store_limit() {
        let mut input = complete_packet_candidate_input();
        let mut previous = "local-total".to_string();
        for index in 0..(LOCAL_MICRO_FLOW_MAX_PACKET_STEPS + 20) {
            let node_id = format!("local-byte-heavy-{index}");
            let long_path = format!(
                "src/{}/byte-heavy-{index}.ts",
                "very/deep/generated/path/".repeat(180)
            );
            let mut node = persisted_node(
                &node_id,
                MicroNodeKind::LocalBinding,
                Some("total"),
                40 + index as u32,
            );
            node.source_span = Some(SourceSpan::with_columns(
                long_path.clone(),
                40 + index as u32,
                1,
                40 + index as u32,
                8,
            ));
            input.nodes.push(node);

            let mut edge = persisted_edge(
                &format!("flow-byte-heavy-{index}"),
                MicroEdgeKind::LocalFlowsTo,
                &previous,
                &node_id,
                40 + index as u32,
            );
            edge.source_span = Some(SourceSpan::with_columns(
                long_path,
                40 + index as u32,
                9,
                40 + index as u32,
                16,
            ));
            input.edges.push(edge);
            previous = node_id;
        }

        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        let candidate = &result.candidates[0];
        assert_eq!(
            candidate.packet_status,
            LocalMicroFlowPacketStatus::MicroFlowTruncated
        );
        assert_eq!(candidate.proof_strength, ProofLadderLevel::Unknown);
        assert!(!result.flow_proof_activated());
        let packet = candidate.packet.as_ref().expect("byte-capped packet");
        assert_eq!(
            packet.budget.truncation_reason,
            "packet_body_byte_cap_omission"
        );
        assert!(packet.budget.omitted_count > 0);
        assert!(packet
            .budget
            .cap_omissions
            .iter()
            .any(|omission| omission.omission_id.ends_with(":packet-body-bytes")));
        assert_eq!(
            packet
                .paths
                .iter()
                .map(|path| path.steps.len())
                .sum::<usize>(),
            1
        );
        let body = packet.encode_dict_v1().expect("dict_v1 body");
        let body_json = serde_json::to_string(&body).expect("serialized dict_v1 body");
        assert!(
            body_json.len() <= LOCAL_MICRO_FLOW_MAX_PACKET_BODY_BYTES as usize,
            "body_json_len={}",
            body_json.len()
        );
    }

    #[test]
    fn mvp4_3_packet_adapter_registry_defaults_unsupported_and_typescript_only_active() {
        let typescript = mvp4_3_local_micro_flow_packet_language_capability("typescript");
        assert_eq!(
            typescript.activation_status,
            LocalMicroFlowPacketSupportStatus::ExactCapable
        );
        assert!(typescript
            .supported_edge_kinds
            .contains(&MicroEdgeKind::LocalFlowsTo));
        assert!(typescript
            .supported_node_kinds
            .contains(&MicroNodeKind::ValueUse));
        assert!(mvp4_3_local_micro_flow_packet_source_supported(
            "typescript",
            MicroSourceRole::Production
        ));
        assert!(!mvp4_3_local_micro_flow_packet_source_supported(
            "typescript",
            MicroSourceRole::Test
        ));

        for language in [
            "javascript",
            "jsx",
            "tsx",
            "rust",
            "python",
            "go",
            "c",
            "cpp",
            "c_cpp",
            "java",
            "csharp",
            "ruby",
            "php",
        ] {
            let capability = mvp4_3_local_micro_flow_packet_language_capability(language);
            assert_eq!(
                capability.activation_status,
                LocalMicroFlowPacketSupportStatus::NotImplemented,
                "{language} must default to not implemented"
            );
            assert!(!capability.activation_status.default_exact_support());
            assert!(capability.supported_node_kinds.is_empty());
            assert!(capability.supported_edge_kinds.is_empty());
            assert!(!mvp4_3_local_micro_flow_packet_source_supported(
                language,
                MicroSourceRole::Production
            ));
        }

        let unknown = mvp4_3_local_micro_flow_packet_language_capability("haskell");
        assert_eq!(unknown.language, "unknown");
        assert_eq!(
            unknown.activation_status,
            LocalMicroFlowPacketSupportStatus::NotImplemented
        );
        assert_eq!(
            mvp4_3_local_micro_flow_packet_active_languages(),
            vec!["typescript"]
        );
        assert_eq!(
            mvp4_3_default_local_micro_flow_packet_query_language(),
            Some("typescript")
        );
    }

    #[test]
    fn mvp4_3_packet_identity_includes_language_context() {
        let mut typescript_identity = MicroPacketIdentityInput {
            repo_relative_path: "src/auth.ts".to_string(),
            language: "typescript".to_string(),
            function_entity_id: "function://login".to_string(),
            packet_kind: MVP4_3_LOCAL_MICRO_FLOW_PACKET_KIND.to_string(),
            packet_version: 1,
            extraction_version: MVP4_3_LOCAL_MICRO_FLOW_PACKET_EXTRACTION_VERSION.to_string(),
            step_set_version: LOCAL_MICRO_FLOW_DICT_V1_STEP_SET_VERSION.to_string(),
            ordered_step_ids: vec!["step:flow".to_string(), "step:return".to_string()],
        };
        let typescript_id = stable_micro_packet_id(&typescript_identity);

        let mut javascript_identity = typescript_identity.clone();
        javascript_identity.language = "javascript".to_string();
        let javascript_id = stable_micro_packet_id(&javascript_identity);

        let mut rust_identity = typescript_identity.clone();
        rust_identity.language = "rust".to_string();
        let rust_id = stable_micro_packet_id(&rust_identity);

        assert_ne!(typescript_id, javascript_id);
        assert_ne!(typescript_id, rust_id);
        assert_ne!(javascript_id, rust_id);

        typescript_identity.repo_relative_path = "fixtures/duplicate/src/auth.ts".to_string();
        assert_ne!(
            typescript_id,
            stable_micro_packet_id(&typescript_identity),
            "same function and steps under a different fixture root must not collide"
        );
    }

    #[test]
    fn mvp4_3_packet_candidate_preserves_shadowed_bindings_by_node_id() {
        let mut nodes = packet_candidate_nodes();
        nodes.push(persisted_node(
            "local-total-shadow",
            MicroNodeKind::LocalBinding,
            Some("total"),
            25,
        ));
        let input = LocalMicroFlowPersistedFactInput::new(
            "src/auth.ts",
            nodes,
            vec![persisted_edge(
                "flow-total-shadow",
                MicroEdgeKind::LocalFlowsTo,
                "local-total",
                "local-total-shadow",
                25,
            )],
        );
        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        let packet = result.candidates[0].packet.as_ref().expect("packet");
        let ordered = packet.to_ordered_steps().expect("ordered");
        let node_ids: BTreeSet<_> = ordered[0]
            .micro_node_refs
            .iter()
            .map(|node| node.micro_node_id.as_str())
            .collect();
        assert!(node_ids.contains("local-total"));
        assert!(node_ids.contains("local-total-shadow"));
        assert_eq!(node_ids.len(), 2);
    }

    #[test]
    fn mvp4_3_path_construction_preserves_distinct_return_paths_and_branch_gap() {
        let mut input = complete_packet_candidate_input();
        input.nodes.push(persisted_node(
            "return-early",
            MicroNodeKind::ReturnSite,
            None,
            7,
        ));
        input.edges.push(persisted_edge(
            "return-early-login",
            MicroEdgeKind::LocalReturnsTo,
            "return-early",
            "fn-login",
            7,
        ));

        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        let packet = result.candidates[0].packet.as_ref().expect("packet");
        assert_eq!(packet.paths.len(), 2);

        let return_path_ids = packet
            .paths
            .iter()
            .map(|path| path.return_path_id.as_deref().expect("return path"))
            .collect::<BTreeSet<_>>();
        assert_eq!(return_path_ids.len(), 2);
        assert!(return_path_ids.contains("return-path:return-early-login"));
        assert!(return_path_ids.contains("return-path:return-total-login"));
        assert_eq!(
            packet.paths[0].return_path_id.as_deref(),
            Some("return-path:return-early-login")
        );
        assert!(packet
            .unknown_unsupported_gaps
            .iter()
            .any(|gap| gap.gap_kind == "unsupported_branch_structure"));
        assert_eq!(
            packet
                .paths
                .iter()
                .filter(|path| path.branch_id.is_none())
                .count(),
            2
        );
    }

    #[test]
    fn mvp4_3_path_construction_is_deterministic_for_shuffled_edges() {
        let input = complete_packet_candidate_input();
        let mut shuffled = input.clone();
        shuffled.edges.reverse();

        let first = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        let second = build_local_micro_flow_packet_candidates_from_persisted_facts(&shuffled);
        let first_packet = first.candidates[0].packet.as_ref().expect("first packet");
        let second_packet = second.candidates[0].packet.as_ref().expect("second packet");

        assert_eq!(first_packet.packet_id, second_packet.packet_id);
        assert_eq!(first_packet.paths, second_packet.paths);
        assert_eq!(
            first_packet.to_agent_json(false).expect("first compact"),
            second_packet.to_agent_json(false).expect("second compact")
        );
    }

    #[test]
    fn mvp4_3_path_construction_keeps_unsupported_call_and_member_gaps_explicit() {
        let mut input = complete_packet_candidate_input();
        input.nodes.push(persisted_node(
            "call-dynamic",
            MicroNodeKind::CallSite,
            Some("lookup"),
            25,
        ));
        input.nodes.push(persisted_node(
            "member-client-send",
            MicroNodeKind::PropertyAccess,
            Some("send"),
            29,
        ));

        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        let packet = result.candidates[0].packet.as_ref().expect("packet");
        let gap_kinds = packet
            .unknown_unsupported_gaps
            .iter()
            .map(|gap| gap.gap_kind.as_str())
            .collect::<BTreeSet<_>>();
        assert!(gap_kinds.contains("dynamic_or_unresolved_call_target"));
        assert!(gap_kinds.contains("member_or_global_target_unsupported"));
        assert!(packet.paths[0].steps.iter().any(|step| {
            step.gap
                .as_ref()
                .is_some_and(|gap| gap.gap_kind == "dynamic_or_unresolved_call_target")
        }));
        assert!(packet.paths[0].steps.iter().any(|step| {
            step.gap
                .as_ref()
                .is_some_and(|gap| gap.gap_kind == "member_or_global_target_unsupported")
        }));
    }

    #[test]
    fn mvp4_3_path_construction_reports_missing_read_write_support_for_flows() {
        let input = LocalMicroFlowPersistedFactInput::new(
            "src/auth.ts",
            packet_candidate_nodes(),
            vec![
                persisted_edge(
                    "flow-seed-total",
                    MicroEdgeKind::LocalFlowsTo,
                    "param-seed",
                    "local-total",
                    13,
                ),
                persisted_edge(
                    "return-total-login",
                    MicroEdgeKind::LocalReturnsTo,
                    "return-total",
                    "fn-login",
                    21,
                ),
            ],
        );
        let result = build_local_micro_flow_packet_candidates_from_persisted_facts(&input);
        let packet = result.candidates[0].packet.as_ref().expect("packet");
        assert!(packet
            .unknown_unsupported_gaps
            .iter()
            .any(|gap| gap.gap_kind == "missing_local_reads_writes_provenance"));
        assert!(packet.paths[0]
            .steps
            .iter()
            .any(|step| step.step_id.contains("flow-seed-total")));
    }

    #[test]
    fn mvp4_3_dict_v1_compact_packet_omits_ordered_steps_by_default() {
        let packet = sample_packet(2);
        let compact = packet.to_agent_json(false).expect("compact packet");
        assert_eq!(compact["encoding"], LOCAL_MICRO_FLOW_DICT_V1_ENCODING);
        assert!(compact.get("packet_body").is_some());
        assert!(compact.get("ordered_steps").is_none());
        assert_eq!(
            compact["packet_body"]["compression_contract"]["verbose_ordered_steps_default_inline"],
            false
        );
        validate_local_micro_flow_agent_json_contract(&compact).expect("valid compact packet");
    }

    #[test]
    fn mvp4_3_dict_v1_audit_expansion_round_trips_semantically() {
        let packet = sample_packet(3);
        let canonical_steps = packet.to_ordered_steps().expect("canonical expansion");
        let body = packet.encode_dict_v1().expect("dict body");
        let expanded_steps = body.to_ordered_steps().expect("dict expansion");
        let serialized_body = serde_json::to_value(&body).expect("serialized dict body");
        let reparsed_body: DictV1PacketBody =
            serde_json::from_value(serialized_body).expect("reparsed dict body");
        let reparsed_steps = reparsed_body
            .to_ordered_steps()
            .expect("reparsed dict expansion");
        assert_eq!(canonical_steps, expanded_steps);
        assert_eq!(expanded_steps, reparsed_steps);
        assert!(expanded_steps.iter().any(|step| step.branch_id.is_some()));
        assert!(expanded_steps
            .iter()
            .any(|step| step.return_path_id.is_some()));
        assert!(expanded_steps.iter().any(|step| step
            .gap
            .as_ref()
            .is_some_and(|gap| gap.gap_kind == "dynamic_lookup")));
        assert!(expanded_steps.iter().any(|step| step
            .cap_omission
            .as_ref()
            .is_some_and(|cap| cap.omitted_count == 3)));

        let audit_json = packet.to_agent_json(true).expect("audit packet");
        assert!(audit_json.get("ordered_steps").is_some());
        validate_local_micro_flow_agent_json_contract(&audit_json).expect("valid audit packet");
    }

    #[test]
    fn mvp4_3_dict_v1_interns_repeated_structures_and_is_smaller_than_audit() {
        let packet = sample_packet(16);
        let compact = packet.to_agent_json(false).expect("compact packet");
        let audit = packet.to_agent_json(true).expect("audit packet");
        let compact_body = compact.get("packet_body").expect("packet body");
        let dictionary = &compact_body["dictionary"];

        assert_eq!(dictionary["spans"].as_object().expect("spans").len(), 2);
        assert_eq!(dictionary["nodes"].as_object().expect("nodes").len(), 2);
        assert_eq!(dictionary["edges"].as_object().expect("edges").len(), 1);
        assert_eq!(
            dictionary["provenance"]
                .as_object()
                .expect("provenance")
                .len(),
            2
        );
        assert!(
            serde_json::to_string(&compact).unwrap().len()
                < serde_json::to_string(&audit).unwrap().len()
        );
    }

    #[test]
    fn mvp4_3_dict_v1_encoding_is_byte_deterministic() {
        let packet = sample_packet(5);
        let left = serde_json::to_string(&packet.to_agent_json(false).unwrap()).unwrap();
        let right = serde_json::to_string(&packet.to_agent_json(false).unwrap()).unwrap();
        assert_eq!(left, right);
    }

    #[test]
    fn mvp4_3_dict_v1_schema_contract_rejects_required_field_and_encoding_errors() {
        let packet = sample_packet(1);
        let mut missing_encoding = packet.to_agent_json(false).unwrap();
        missing_encoding.as_object_mut().unwrap().remove("encoding");
        assert!(validate_local_micro_flow_agent_json_contract(&missing_encoding).is_err());

        let mut missing_body = packet.to_agent_json(false).unwrap();
        missing_body.as_object_mut().unwrap().remove("packet_body");
        assert!(validate_local_micro_flow_agent_json_contract(&missing_body).is_err());

        let mut wrong_encoding = packet.to_agent_json(false).unwrap();
        wrong_encoding["encoding"] = json!("ordered_steps_v0");
        assert!(validate_local_micro_flow_agent_json_contract(&wrong_encoding).is_err());
    }

    #[test]
    fn mvp4_3_dict_v1_rejects_full_source_body_and_missing_proof_metadata() {
        let packet = sample_packet(1);
        let mut with_source_body = packet.to_agent_json(false).unwrap();
        with_source_body["full_source_body"] = json!("function login() { return token; }");
        assert!(validate_local_micro_flow_agent_json_contract(&with_source_body).is_err());

        let mut missing_provenance = packet.to_agent_json(false).unwrap();
        let steps = missing_provenance["packet_body"]["dictionary"]["steps"]
            .as_object_mut()
            .unwrap();
        let first_step = steps.values_mut().next().unwrap().as_object_mut().unwrap();
        first_step.insert("provenance_refs".to_string(), json!([]));
        first_step.insert(
            "proof_contribution".to_string(),
            json!(proof_ladder_level_str(ProofLadderLevel::GraphRelationProof)),
        );
        assert!(validate_local_micro_flow_agent_json_contract(&missing_provenance).is_err());

        let mut missing_span = packet.to_agent_json(false).unwrap();
        let steps = missing_span["packet_body"]["dictionary"]["steps"]
            .as_object_mut()
            .unwrap();
        let first_step = steps.values_mut().next().unwrap().as_object_mut().unwrap();
        first_step.insert("span_refs".to_string(), json!([]));
        first_step.insert(
            "proof_contribution".to_string(),
            json!(proof_ladder_level_str(ProofLadderLevel::GraphRelationProof)),
        );
        assert!(validate_local_micro_flow_agent_json_contract(&missing_span).is_err());
    }

    #[test]
    fn mvp4_3_dict_v1_requires_branch_and_return_identity_refs_when_present() {
        let packet = sample_packet(1);
        let mut missing_branch = packet.to_agent_json(false).unwrap();
        missing_branch["packet_body"]["dictionary"]["branch_identities"] = json!({});
        assert!(validate_local_micro_flow_agent_json_contract(&missing_branch).is_err());

        let mut missing_return = packet.to_agent_json(false).unwrap();
        missing_return["packet_body"]["dictionary"]["return_path_identities"] = json!({});
        assert!(validate_local_micro_flow_agent_json_contract(&missing_return).is_err());
    }

    #[test]
    fn mvp4_3_dict_v1_preserves_shadowed_binding_identity_without_source_bodies() {
        let packet = sample_packet(2);
        let compact = packet.to_agent_json(false).unwrap();
        let nodes = compact["packet_body"]["dictionary"]["nodes"]
            .as_object()
            .unwrap();
        let binding_ids = nodes
            .values()
            .filter_map(|node| node["stable_binding_identity"].as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(binding_ids.len(), 2);
        let serialized = serde_json::to_string(&compact).unwrap();
        assert!(!serialized.contains("function login"));
        assert!(!serialized.contains("full_source_body"));
    }
}
