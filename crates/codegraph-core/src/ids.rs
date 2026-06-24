use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    dirty_evidence::ProofLadderLevel, validation::ValidationClassification, EntityKind,
    EvidenceRole, RelationKind, SourceSpan,
};

pub const MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION: &str =
    "mvp4.2-typescript-local-returns-to-v1";
pub const MVP4_2_MICRO_EDGE_ROW_SCHEMA_VERSION: u32 = 1;
pub const MVP4_2_MICRO_EDGE_PAYLOAD_VERSION: u32 = 1;

pub fn normalize_repo_relative_path(path: impl AsRef<str>) -> String {
    let path = path.as_ref().trim().replace('\\', "/");
    let mut normalized = path.as_str();

    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped;
    }

    while let Some(stripped) = normalized.strip_prefix('/') {
        normalized = stripped;
    }

    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        parts.push(part);
    }

    parts.join("/")
}

pub fn stable_entity_id(
    repo_relative_path: impl AsRef<str>,
    semantic_identity: impl AsRef<str>,
) -> String {
    let repo_relative_path = normalize_repo_relative_path(repo_relative_path);
    let semantic_identity = semantic_identity.as_ref().trim();
    format!(
        "repo://e/{}",
        stable_digest_128(["entity", &repo_relative_path, semantic_identity])
    )
}

pub fn stable_entity_id_for_kind(
    repo_relative_path: impl AsRef<str>,
    kind: EntityKind,
    name: impl AsRef<str>,
    signature_hash: Option<&str>,
) -> String {
    let mut semantic_identity = format!("{}:{}", kind.id_prefix(), name.as_ref().trim());
    if let Some(signature_hash) = signature_hash {
        semantic_identity.push('(');
        semantic_identity.push_str(signature_hash.trim());
        semantic_identity.push(')');
    }

    stable_entity_id(repo_relative_path, semantic_identity)
}

pub fn stable_edge_id(
    head_id: impl AsRef<str>,
    relation: RelationKind,
    tail_id: impl AsRef<str>,
    source_span: &SourceSpan,
) -> String {
    let relation = relation.to_string();
    let source_span = source_span.to_string();
    format!(
        "edge://{}",
        stable_digest_128([
            "edge",
            head_id.as_ref().trim(),
            &relation,
            tail_id.as_ref().trim(),
            &source_span,
        ])
    )
}

pub fn stable_fact_identity_key<'a>(
    fact_kind: impl AsRef<str>,
    parts: impl IntoIterator<Item = &'a str>,
) -> String {
    stable_prefixed_digest("fact", fact_kind.as_ref(), parts)
}

pub fn stable_fact_hash<'a>(
    fact_kind: impl AsRef<str>,
    parts: impl IntoIterator<Item = &'a str>,
) -> String {
    stable_prefixed_digest("fact-hash", fact_kind.as_ref(), parts)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicroNodeKind {
    FunctionFrame,
    Parameter,
    LocalBinding,
    PropertyAccess,
    CallSite,
    ReturnSite,
    AssignmentSite,
    MutationSite,
    ConditionSite,
    LiteralKey,
    ImportBinding,
    ExportBinding,
    TestAssertion,
    RouteLiteral,
    RouteBinding,
    AuthLiteral,
    SanitizerCall,
}

impl MicroNodeKind {
    pub const ALL: &'static [Self] = &[
        Self::FunctionFrame,
        Self::Parameter,
        Self::LocalBinding,
        Self::PropertyAccess,
        Self::CallSite,
        Self::ReturnSite,
        Self::AssignmentSite,
        Self::MutationSite,
        Self::ConditionSite,
        Self::LiteralKey,
        Self::ImportBinding,
        Self::ExportBinding,
        Self::TestAssertion,
        Self::RouteLiteral,
        Self::RouteBinding,
        Self::AuthLiteral,
        Self::SanitizerCall,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FunctionFrame => "function_frame",
            Self::Parameter => "parameter",
            Self::LocalBinding => "local_binding",
            Self::PropertyAccess => "property_access",
            Self::CallSite => "call_site",
            Self::ReturnSite => "return_site",
            Self::AssignmentSite => "assignment_site",
            Self::MutationSite => "mutation_site",
            Self::ConditionSite => "condition_site",
            Self::LiteralKey => "literal_key",
            Self::ImportBinding => "import_binding",
            Self::ExportBinding => "export_binding",
            Self::TestAssertion => "test_assertion",
            Self::RouteLiteral => "route_literal",
            Self::RouteBinding => "route_binding",
            Self::AuthLiteral => "auth_literal",
            Self::SanitizerCall => "sanitizer_call",
        }
    }
}

impl fmt::Display for MicroNodeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicroEdgeKind {
    LocalReads,
    LocalWrites,
    LocalFlowsTo,
    LocalCalls,
    LocalReturnsTo,
    LocalMutates,
    LocalChecks,
    LocalSanitizes,
    LocalGuards,
    LocalAsserts,
    LocalBranchesTo,
}

impl MicroEdgeKind {
    pub const ALL: &'static [Self] = &[
        Self::LocalReads,
        Self::LocalWrites,
        Self::LocalFlowsTo,
        Self::LocalCalls,
        Self::LocalReturnsTo,
        Self::LocalMutates,
        Self::LocalChecks,
        Self::LocalSanitizes,
        Self::LocalGuards,
        Self::LocalAsserts,
        Self::LocalBranchesTo,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalReads => "local_reads",
            Self::LocalWrites => "local_writes",
            Self::LocalFlowsTo => "local_flows_to",
            Self::LocalCalls => "local_calls",
            Self::LocalReturnsTo => "local_returns_to",
            Self::LocalMutates => "local_mutates",
            Self::LocalChecks => "local_checks",
            Self::LocalSanitizes => "local_sanitizes",
            Self::LocalGuards => "local_guards",
            Self::LocalAsserts => "local_asserts",
            Self::LocalBranchesTo => "local_branches_to",
        }
    }
}

impl fmt::Display for MicroEdgeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicroExactness {
    Exact,
    DerivedWithProvenance,
    Heuristic,
    Unsupported,
    Unknown,
}

impl MicroExactness {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::DerivedWithProvenance => "derived_with_provenance",
            Self::Heuristic => "heuristic",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicroIdentityStability {
    StableIdentityGuarantee,
    BestEffortCorrelation,
    IntentionalRekey,
    UnknownAmbiguousCorrelation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicroIdentityChangeKind {
    RepeatedUnchangedIndexing,
    WhitespaceOnlyEdit,
    CommentOnlyEdit,
    AddedNeighboringStatement,
    DeletedNeighboringStatement,
    SameNameDifferentScope,
    ShadowingNestedScope,
    ClosureCapture,
    IdenticalCodeDifferentFile,
    FileRename,
    FunctionRename,
    ParameterRename,
    DestructuringShapeChange,
    MultipleSameKindCallsites,
    ParseRecoveryNode,
    PartiallyBrokenCode,
    LanguageFrontendDifference,
    RouteOverload,
    BridgeOverload,
    SourceConstructChanged,
    Ambiguous,
}

pub fn classify_micro_identity_change(change: MicroIdentityChangeKind) -> MicroIdentityStability {
    match change {
        MicroIdentityChangeKind::RepeatedUnchangedIndexing
        | MicroIdentityChangeKind::WhitespaceOnlyEdit
        | MicroIdentityChangeKind::CommentOnlyEdit
        | MicroIdentityChangeKind::AddedNeighboringStatement
        | MicroIdentityChangeKind::DeletedNeighboringStatement
        | MicroIdentityChangeKind::SameNameDifferentScope
        | MicroIdentityChangeKind::ShadowingNestedScope
        | MicroIdentityChangeKind::ClosureCapture
        | MicroIdentityChangeKind::IdenticalCodeDifferentFile
        | MicroIdentityChangeKind::MultipleSameKindCallsites
        | MicroIdentityChangeKind::RouteOverload
        | MicroIdentityChangeKind::BridgeOverload => {
            MicroIdentityStability::StableIdentityGuarantee
        }
        MicroIdentityChangeKind::FileRename
        | MicroIdentityChangeKind::FunctionRename
        | MicroIdentityChangeKind::ParameterRename
        | MicroIdentityChangeKind::DestructuringShapeChange
        | MicroIdentityChangeKind::SourceConstructChanged => {
            MicroIdentityStability::IntentionalRekey
        }
        MicroIdentityChangeKind::LanguageFrontendDifference => {
            MicroIdentityStability::BestEffortCorrelation
        }
        MicroIdentityChangeKind::ParseRecoveryNode
        | MicroIdentityChangeKind::PartiallyBrokenCode
        | MicroIdentityChangeKind::Ambiguous => MicroIdentityStability::UnknownAmbiguousCorrelation,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicroDerivationKind {
    DirectAstExtraction,
    ResolverBackedBinding,
    LocalSequenceDerivation,
    LocalAssignmentChainDerivation,
    RouteAdapterDerivation,
    BridgeAdapterDerivation,
    CompilerLspBackedDerivation,
    HeuristicPattern,
    Unsupported,
}

impl MicroDerivationKind {
    pub const fn requires_source_facts(self) -> bool {
        !matches!(self, Self::DirectAstExtraction | Self::Unsupported)
    }

    pub const fn is_claimable_derivation(self) -> bool {
        matches!(
            self,
            Self::DirectAstExtraction
                | Self::ResolverBackedBinding
                | Self::LocalSequenceDerivation
                | Self::LocalAssignmentChainDerivation
                | Self::RouteAdapterDerivation
                | Self::BridgeAdapterDerivation
                | Self::CompilerLspBackedDerivation
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicroSourceRole {
    Production,
    Test,
    Mock,
    Stub,
    Generated,
    SourceText,
    Unknown,
    Mixed,
}

impl MicroSourceRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Test => "test",
            Self::Mock => "mock",
            Self::Stub => "stub",
            Self::Generated => "generated",
            Self::SourceText => "source_text",
            Self::Unknown => "unknown",
            Self::Mixed => "mixed",
        }
    }

    pub const fn can_support_local_production_proof(self) -> bool {
        matches!(self, Self::Production)
    }

    pub const fn as_current_evidence_role(self) -> EvidenceRole {
        match self {
            Self::Production => EvidenceRole::Production,
            Self::Test => EvidenceRole::Test,
            Self::Mock | Self::Stub => EvidenceRole::Mock,
            Self::Mixed => EvidenceRole::Mixed,
            Self::Generated | Self::SourceText | Self::Unknown => EvidenceRole::Unknown,
        }
    }
}

pub fn micro_source_roles_allow_local_production_proof(roles: &[MicroSourceRole]) -> bool {
    !roles.is_empty()
        && roles
            .iter()
            .copied()
            .all(MicroSourceRole::can_support_local_production_proof)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroNodeIdentityInput {
    pub repo_relative_path: String,
    pub language: String,
    pub kind: MicroNodeKind,
    pub function_entity_id: Option<String>,
    pub scope_path: Vec<String>,
    pub binding_id: Option<String>,
    pub symbol: Option<String>,
    pub structural_path: Vec<String>,
    pub occurrence_index: u32,
    pub source_span: Option<SourceSpan>,
    pub parse_recovery: bool,
}

impl MicroNodeIdentityInput {
    pub fn stability(&self) -> MicroIdentityStability {
        if self.parse_recovery {
            return MicroIdentityStability::UnknownAmbiguousCorrelation;
        }
        if self.binding_id.as_deref().is_some_and(non_empty) {
            return MicroIdentityStability::StableIdentityGuarantee;
        }
        if self.function_entity_id.as_deref().is_some_and(non_empty)
            && (!self.scope_path.is_empty() || !self.structural_path.is_empty())
        {
            return MicroIdentityStability::StableIdentityGuarantee;
        }
        if self.symbol.as_deref().is_some_and(non_empty) && !self.structural_path.is_empty() {
            return MicroIdentityStability::BestEffortCorrelation;
        }
        MicroIdentityStability::UnknownAmbiguousCorrelation
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroEdgeIdentityInput {
    pub repo_relative_path: String,
    pub language: String,
    pub kind: MicroEdgeKind,
    pub function_entity_id: Option<String>,
    pub scope_path: Vec<String>,
    pub source_micro_node_id: String,
    pub target_micro_node_id: String,
    pub structural_path: Vec<String>,
    pub occurrence_index: u32,
    pub source_span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroPacketIdentityInput {
    pub repo_relative_path: String,
    pub language: String,
    pub function_entity_id: String,
    pub packet_kind: String,
    pub packet_version: u32,
    pub extraction_version: String,
    pub step_set_version: String,
    pub ordered_step_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteBridgeIdentityKind {
    Route,
    Bridge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteBridgeIdentityInput {
    pub kind: RouteBridgeIdentityKind,
    pub repo_relative_path: String,
    pub language: String,
    pub adapter_version: String,
    pub framework: String,
    pub source_node_id: String,
    pub target_node_id: Option<String>,
    pub literal_or_pattern: Option<String>,
    pub overload_discriminator: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroFactProvenance {
    pub derivation_kind: MicroDerivationKind,
    pub source_fact_ids: Vec<String>,
    pub source_spans: Vec<SourceSpan>,
    pub extractor_or_adapter_version: String,
    pub exactness: MicroExactness,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalReturnsToExactnessRequirement {
    SupportedTypeScriptProductionTsFile,
    CurrentReturnSite,
    CurrentFunctionFrame,
    ReturnSiteSourceSpan,
    FunctionFrameSourceSpan,
    DeterministicNearestEnclosingFunctionOwnership,
    IdenticalNormalizedFileIdentity,
    IdenticalEnclosingFunctionIdentityDomain,
    EndpointsRetainedAfterCaps,
    CurrentExtractionVersions,
    ClaimableLifecyclePassport,
    DirectAstProvenance,
    NoParserRecoveryAmbiguity,
}

impl LocalReturnsToExactnessRequirement {
    pub const ALL: &'static [Self] = &[
        Self::SupportedTypeScriptProductionTsFile,
        Self::CurrentReturnSite,
        Self::CurrentFunctionFrame,
        Self::ReturnSiteSourceSpan,
        Self::FunctionFrameSourceSpan,
        Self::DeterministicNearestEnclosingFunctionOwnership,
        Self::IdenticalNormalizedFileIdentity,
        Self::IdenticalEnclosingFunctionIdentityDomain,
        Self::EndpointsRetainedAfterCaps,
        Self::CurrentExtractionVersions,
        Self::ClaimableLifecyclePassport,
        Self::DirectAstProvenance,
        Self::NoParserRecoveryAmbiguity,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SupportedTypeScriptProductionTsFile => "supported_typescript_production_ts_file",
            Self::CurrentReturnSite => "current_return_site",
            Self::CurrentFunctionFrame => "current_function_frame",
            Self::ReturnSiteSourceSpan => "return_site_source_span",
            Self::FunctionFrameSourceSpan => "function_frame_source_span",
            Self::DeterministicNearestEnclosingFunctionOwnership => {
                "deterministic_nearest_enclosing_function_ownership"
            }
            Self::IdenticalNormalizedFileIdentity => "identical_normalized_file_identity",
            Self::IdenticalEnclosingFunctionIdentityDomain => {
                "identical_enclosing_function_identity_domain"
            }
            Self::EndpointsRetainedAfterCaps => "endpoints_retained_after_caps",
            Self::CurrentExtractionVersions => "current_extraction_versions",
            Self::ClaimableLifecyclePassport => "claimable_lifecycle_passport",
            Self::DirectAstProvenance => "direct_ast_provenance",
            Self::NoParserRecoveryAmbiguity => "no_parser_recovery_ambiguity",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalReturnsToExactnessInput {
    pub supported_typescript_production_ts_file: bool,
    pub current_return_site: bool,
    pub current_function_frame: bool,
    pub return_site_source_span: bool,
    pub function_frame_source_span: bool,
    pub deterministic_nearest_enclosing_function_ownership: bool,
    pub identical_normalized_file_identity: bool,
    pub identical_enclosing_function_identity_domain: bool,
    pub endpoints_retained_after_caps: bool,
    pub current_extraction_versions: bool,
    pub claimable_lifecycle_passport: bool,
    pub direct_ast_provenance: bool,
    pub no_parser_recovery_ambiguity: bool,
}

impl LocalReturnsToExactnessInput {
    pub const fn exact_claimable() -> Self {
        Self {
            supported_typescript_production_ts_file: true,
            current_return_site: true,
            current_function_frame: true,
            return_site_source_span: true,
            function_frame_source_span: true,
            deterministic_nearest_enclosing_function_ownership: true,
            identical_normalized_file_identity: true,
            identical_enclosing_function_identity_domain: true,
            endpoints_retained_after_caps: true,
            current_extraction_versions: true,
            claimable_lifecycle_passport: true,
            direct_ast_provenance: true,
            no_parser_recovery_ambiguity: true,
        }
    }

    pub fn missing_requirements(&self) -> Vec<LocalReturnsToExactnessRequirement> {
        let checks = [
            (
                LocalReturnsToExactnessRequirement::SupportedTypeScriptProductionTsFile,
                self.supported_typescript_production_ts_file,
            ),
            (
                LocalReturnsToExactnessRequirement::CurrentReturnSite,
                self.current_return_site,
            ),
            (
                LocalReturnsToExactnessRequirement::CurrentFunctionFrame,
                self.current_function_frame,
            ),
            (
                LocalReturnsToExactnessRequirement::ReturnSiteSourceSpan,
                self.return_site_source_span,
            ),
            (
                LocalReturnsToExactnessRequirement::FunctionFrameSourceSpan,
                self.function_frame_source_span,
            ),
            (
                LocalReturnsToExactnessRequirement::DeterministicNearestEnclosingFunctionOwnership,
                self.deterministic_nearest_enclosing_function_ownership,
            ),
            (
                LocalReturnsToExactnessRequirement::IdenticalNormalizedFileIdentity,
                self.identical_normalized_file_identity,
            ),
            (
                LocalReturnsToExactnessRequirement::IdenticalEnclosingFunctionIdentityDomain,
                self.identical_enclosing_function_identity_domain,
            ),
            (
                LocalReturnsToExactnessRequirement::EndpointsRetainedAfterCaps,
                self.endpoints_retained_after_caps,
            ),
            (
                LocalReturnsToExactnessRequirement::CurrentExtractionVersions,
                self.current_extraction_versions,
            ),
            (
                LocalReturnsToExactnessRequirement::ClaimableLifecyclePassport,
                self.claimable_lifecycle_passport,
            ),
            (
                LocalReturnsToExactnessRequirement::DirectAstProvenance,
                self.direct_ast_provenance,
            ),
            (
                LocalReturnsToExactnessRequirement::NoParserRecoveryAmbiguity,
                self.no_parser_recovery_ambiguity,
            ),
        ];

        checks
            .into_iter()
            .filter_map(|(requirement, present)| (!present).then_some(requirement))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalReturnsToExactnessDecision {
    pub exactness: MicroExactness,
    pub claimable: bool,
    pub persist_proof_row: bool,
    pub proof_ladder_level: Option<ProofLadderLevel>,
    pub missing_requirements: Vec<LocalReturnsToExactnessRequirement>,
    pub failure_action: &'static str,
}

pub fn decide_local_returns_to_exactness(
    input: &LocalReturnsToExactnessInput,
) -> LocalReturnsToExactnessDecision {
    let missing = input.missing_requirements();
    if missing.is_empty() {
        LocalReturnsToExactnessDecision {
            exactness: MicroExactness::Exact,
            claimable: true,
            persist_proof_row: true,
            proof_ladder_level: Some(ProofLadderLevel::GraphRelationProof),
            missing_requirements: missing,
            failure_action: "persist_exact_local_returns_to_row_when_extraction_prompt_allows",
        }
    } else {
        LocalReturnsToExactnessDecision {
            exactness: MicroExactness::Unknown,
            claimable: false,
            persist_proof_row: false,
            proof_ladder_level: None,
            missing_requirements: missing,
            failure_action: "omit_exact_edge_or_emit_non_claimable_diagnostic_candidate_only",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalReturnsToIdentityContractInput {
    pub repo_relative_path: String,
    pub language: String,
    pub function_entity_id: String,
    pub scope_path: Vec<String>,
    pub head_return_site_micro_node_id: String,
    pub tail_function_frame_micro_node_id: String,
    pub return_structural_path: Vec<String>,
    pub occurrence_index: u32,
    pub source_role: MicroSourceRole,
    pub row_schema_version: u32,
    pub payload_version: u32,
    pub extraction_version: String,
    pub relation_span: Option<SourceSpan>,
}

pub fn local_returns_to_identity_input(
    input: &LocalReturnsToIdentityContractInput,
) -> MicroEdgeIdentityInput {
    let mut structural_path = vec![
        "relation:local_returns_to".to_string(),
        "ownership:nearest_enclosing_function".to_string(),
        format!("source_role:{}", input.source_role.as_str()),
        format!("row_schema_version:{}", input.row_schema_version),
        format!("payload_version:{}", input.payload_version),
        format!("extraction_version:{}", input.extraction_version.trim()),
    ];
    structural_path.extend(input.return_structural_path.iter().cloned());

    MicroEdgeIdentityInput {
        repo_relative_path: normalize_repo_relative_path(&input.repo_relative_path),
        language: input.language.trim().to_string(),
        kind: MicroEdgeKind::LocalReturnsTo,
        function_entity_id: Some(input.function_entity_id.trim().to_string()),
        scope_path: input.scope_path.clone(),
        source_micro_node_id: input.head_return_site_micro_node_id.trim().to_string(),
        target_micro_node_id: input.tail_function_frame_micro_node_id.trim().to_string(),
        structural_path,
        occurrence_index: input.occurrence_index,
        source_span: input.relation_span.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalReturnsToCapContract {
    pub edge_count_bounded_by_retained_return_sites: bool,
    pub max_edges_per_retained_return_site: u32,
    pub function_and_file_caps_required: bool,
    pub omission_count_required: bool,
    pub omission_reason_required: bool,
    pub deterministic_retention_required: bool,
    pub exact_complete_claim_allowed_after_truncation: bool,
}

pub const LOCAL_RETURNS_TO_CAP_CONTRACT: LocalReturnsToCapContract = LocalReturnsToCapContract {
    edge_count_bounded_by_retained_return_sites: true,
    max_edges_per_retained_return_site: 1,
    function_and_file_caps_required: true,
    omission_count_required: true,
    omission_reason_required: true,
    deterministic_retention_required: true,
    exact_complete_claim_allowed_after_truncation: false,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicroEdgeLayerState {
    Ready,
    Unavailable,
    Stale,
    Incompatible,
    Corrupt,
    Truncated,
    UpdatingPublishing,
    NotApplicable,
}

impl MicroEdgeLayerState {
    pub const ALL: &'static [Self] = &[
        Self::Ready,
        Self::Unavailable,
        Self::Stale,
        Self::Incompatible,
        Self::Corrupt,
        Self::Truncated,
        Self::UpdatingPublishing,
        Self::NotApplicable,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Unavailable => "unavailable",
            Self::Stale => "stale",
            Self::Incompatible => "incompatible",
            Self::Corrupt => "corrupt",
            Self::Truncated => "truncated",
            Self::UpdatingPublishing => "updating/publishing",
            Self::NotApplicable => "not_applicable",
        }
    }

    pub const fn graph_claimability_separate(self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalReturnsToLinterClass {
    NormalSourceDelta,
    ReverifiedProofIntegrityFinding,
    UnsupportedDegradedCondition,
}

impl LocalReturnsToLinterClass {
    pub const fn validation_classification(self) -> ValidationClassification {
        match self {
            Self::NormalSourceDelta => ValidationClassification::Diagnostic,
            Self::ReverifiedProofIntegrityFinding => ValidationClassification::Block,
            Self::UnsupportedDegradedCondition => ValidationClassification::Degraded,
        }
    }

    pub const fn is_user_source_defect_by_default(self) -> bool {
        false
    }

    pub const fn may_block_micro_edge_proof_availability(self) -> bool {
        matches!(self, Self::ReverifiedProofIntegrityFinding)
    }

    pub const fn may_create_source_code_hard_interrupt_without_reverification(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalReturnsToSourceDeltaKind {
    ReturnAdded,
    ReturnRemoved,
    ReturnMovedToNestedFunction,
    FunctionRenamed,
}

impl LocalReturnsToSourceDeltaKind {
    pub const fn linter_class(self) -> LocalReturnsToLinterClass {
        LocalReturnsToLinterClass::NormalSourceDelta
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalReturnsToIntegrityFindingKind {
    PersistedEdgeHeadMissing,
    PersistedEdgeTailMissing,
    InvalidEndpointKind,
    EdgeCrossesFiles,
    EdgeCrossesFunctionOwnershipUnexpectedly,
    WrongEnclosingFunctionFrame,
    ExactEdgeMissingSourceSpan,
    ExactEdgeMissingProvenance,
    EdgeExtractionVersionConflictsWithEndpoints,
    StaleExactEdgeAfterSourceChange,
}

impl LocalReturnsToIntegrityFindingKind {
    pub const fn linter_class(self) -> LocalReturnsToLinterClass {
        LocalReturnsToLinterClass::ReverifiedProofIntegrityFinding
    }

    pub const fn recommended_action(self) -> &'static str {
        match self {
            Self::PersistedEdgeHeadMissing
            | Self::PersistedEdgeTailMissing
            | Self::InvalidEndpointKind
            | Self::EdgeCrossesFiles
            | Self::EdgeCrossesFunctionOwnershipUnexpectedly
            | Self::WrongEnclosingFunctionFrame
            | Self::ExactEdgeMissingSourceSpan
            | Self::ExactEdgeMissingProvenance
            | Self::EdgeExtractionVersionConflictsWithEndpoints
            | Self::StaleExactEdgeAfterSourceChange => {
                "reindex_or_repair_codegraph_micro_edge_state"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalReturnsToUnsupportedConditionKind {
    CapOmission,
    UnsupportedSourceKind,
    ParserRecovery,
    UnsupportedLanguage,
    ResolverUnavailable,
    StaleOptionalLayer,
    OldDbWithoutEdgeTable,
}

impl LocalReturnsToUnsupportedConditionKind {
    pub const fn linter_class(self) -> LocalReturnsToLinterClass {
        LocalReturnsToLinterClass::UnsupportedDegradedCondition
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalReturnsToContract {
    pub relation: MicroEdgeKind,
    pub head_micro_node: MicroNodeKind,
    pub tail_micro_node: MicroNodeKind,
    pub canonical_meaning: &'static str,
    pub forbidden_interpretations: &'static [&'static str],
    pub exact_requirements: &'static [LocalReturnsToExactnessRequirement],
    pub proof_ladder_level_for_exact_edge: ProofLadderLevel,
    pub mutation_proof_activated: bool,
    pub flow_proof_activated: bool,
    pub row_schema_version: u32,
    pub payload_version: u32,
    pub extraction_version: &'static str,
    pub default_relation_span: &'static str,
}

pub const LOCAL_RETURNS_TO_FORBIDDEN_INTERPRETATIONS: &[&str] = &[
    "returned_expression_flows_to_function_output",
    "runtime_return_value",
    "control_flow_reachability",
    "complete_return_coverage",
    "function_always_returns",
    "function_returns_particular_type",
    "interprocedural_return_relation",
    "local_flows_to",
    "flow_proof",
];

pub const LOCAL_RETURNS_TO_CONTRACT: LocalReturnsToContract = LocalReturnsToContract {
    relation: MicroEdgeKind::LocalReturnsTo,
    head_micro_node: MicroNodeKind::ReturnSite,
    tail_micro_node: MicroNodeKind::FunctionFrame,
    canonical_meaning: "The source-spanned return statement represented by the ReturnSite is structurally owned by the source-spanned enclosing function represented by the FunctionFrame.",
    forbidden_interpretations: LOCAL_RETURNS_TO_FORBIDDEN_INTERPRETATIONS,
    exact_requirements: LocalReturnsToExactnessRequirement::ALL,
    proof_ladder_level_for_exact_edge: ProofLadderLevel::GraphRelationProof,
    mutation_proof_activated: false,
    flow_proof_activated: false,
    row_schema_version: MVP4_2_MICRO_EDGE_ROW_SCHEMA_VERSION,
    payload_version: MVP4_2_MICRO_EDGE_PAYLOAD_VERSION,
    extraction_version: MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION,
    default_relation_span: "return_site_span",
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicroProvenanceError {
    message: String,
}

impl MicroProvenanceError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MicroProvenanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MicroProvenanceError {}

pub fn validate_micro_fact_provenance(
    provenance: &MicroFactProvenance,
) -> Result<(), MicroProvenanceError> {
    if provenance.extractor_or_adapter_version.trim().is_empty() {
        return Err(MicroProvenanceError::new(
            "micro fact provenance requires extractor_or_adapter_version",
        ));
    }
    if provenance.source_spans.is_empty() {
        return Err(MicroProvenanceError::new(
            "micro fact provenance requires source_spans",
        ));
    }
    if provenance.derivation_kind.requires_source_facts() && provenance.source_fact_ids.is_empty() {
        return Err(MicroProvenanceError::new(
            "derived micro facts require source_fact_ids",
        ));
    }
    if provenance.exactness == MicroExactness::DerivedWithProvenance
        && provenance.source_fact_ids.is_empty()
    {
        return Err(MicroProvenanceError::new(
            "derived_with_provenance exactness requires source_fact_ids",
        ));
    }
    if provenance.derivation_kind == MicroDerivationKind::Unsupported
        && !matches!(
            provenance.exactness,
            MicroExactness::Unsupported | MicroExactness::Unknown
        )
    {
        return Err(MicroProvenanceError::new(
            "unsupported derivation cannot carry proof-grade exactness",
        ));
    }
    Ok(())
}

pub fn stable_micro_node_id(input: &MicroNodeIdentityInput) -> String {
    let parts = micro_node_identity_parts(input);
    stable_prefixed_digest(
        "micro-node",
        input.kind.as_str(),
        parts.iter().map(String::as_str),
    )
}

pub fn stable_micro_edge_id(input: &MicroEdgeIdentityInput) -> String {
    let parts = micro_edge_identity_parts(input);
    stable_prefixed_digest(
        "micro-edge",
        input.kind.as_str(),
        parts.iter().map(String::as_str),
    )
}

pub fn stable_micro_packet_id(input: &MicroPacketIdentityInput) -> String {
    let parts = micro_packet_identity_parts(input);
    stable_prefixed_digest(
        "micro-packet",
        &input.packet_kind,
        parts.iter().map(String::as_str),
    )
}

pub fn stable_route_bridge_identity_id(input: &RouteBridgeIdentityInput) -> String {
    let fact_kind = match input.kind {
        RouteBridgeIdentityKind::Route => "route",
        RouteBridgeIdentityKind::Bridge => "bridge",
    };
    let parts = route_bridge_identity_parts(input);
    stable_prefixed_digest("route-bridge", fact_kind, parts.iter().map(String::as_str))
}

fn stable_prefixed_digest<'a>(
    prefix: &str,
    fact_kind: &str,
    parts: impl IntoIterator<Item = &'a str>,
) -> String {
    let mut bytes = Vec::new();
    for part in [prefix, fact_kind.trim()] {
        bytes.extend_from_slice(part.as_bytes());
        bytes.push(0);
    }
    for part in parts {
        bytes.extend_from_slice(part.as_bytes());
        bytes.push(0);
    }

    let high = fnv64_with_seed(&bytes, 0xcbf29ce484222325);
    let low = fnv64_with_seed(&bytes, 0x9e3779b185ebca87);
    format!("{prefix}://{high:016x}{low:016x}")
}

fn stable_digest_128<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut bytes = Vec::new();
    for part in parts {
        bytes.extend_from_slice(part.as_bytes());
        bytes.push(0);
    }

    let high = fnv64_with_seed(&bytes, 0xcbf29ce484222325);
    let low = fnv64_with_seed(&bytes, 0x9e3779b185ebca87);
    format!("{high:016x}{low:016x}")
}

fn fnv64_with_seed(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = seed;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn micro_node_identity_parts(input: &MicroNodeIdentityInput) -> Vec<String> {
    let mut parts = Vec::new();
    push_part(
        &mut parts,
        "file",
        &normalize_repo_relative_path(&input.repo_relative_path),
    );
    push_part(&mut parts, "language", &input.language);
    push_optional_part(&mut parts, "function", input.function_entity_id.as_deref());
    push_vec_part(&mut parts, "scope", &input.scope_path);
    push_optional_part(&mut parts, "binding", input.binding_id.as_deref());
    push_optional_part(&mut parts, "symbol", input.symbol.as_deref());
    push_vec_part(&mut parts, "structural", &input.structural_path);
    push_part(
        &mut parts,
        "occurrence",
        &input.occurrence_index.to_string(),
    );
    push_part(
        &mut parts,
        "parse_recovery",
        bool_part(input.parse_recovery),
    );
    parts
}

fn micro_edge_identity_parts(input: &MicroEdgeIdentityInput) -> Vec<String> {
    let mut parts = Vec::new();
    push_part(
        &mut parts,
        "file",
        &normalize_repo_relative_path(&input.repo_relative_path),
    );
    push_part(&mut parts, "language", &input.language);
    push_optional_part(&mut parts, "function", input.function_entity_id.as_deref());
    push_vec_part(&mut parts, "scope", &input.scope_path);
    push_part(&mut parts, "source", &input.source_micro_node_id);
    push_part(&mut parts, "target", &input.target_micro_node_id);
    push_vec_part(&mut parts, "structural", &input.structural_path);
    push_part(
        &mut parts,
        "occurrence",
        &input.occurrence_index.to_string(),
    );
    parts
}

fn micro_packet_identity_parts(input: &MicroPacketIdentityInput) -> Vec<String> {
    let mut parts = Vec::new();
    push_part(
        &mut parts,
        "file",
        &normalize_repo_relative_path(&input.repo_relative_path),
    );
    push_part(&mut parts, "language", &input.language);
    push_part(&mut parts, "function", &input.function_entity_id);
    push_part(
        &mut parts,
        "packet_version",
        &input.packet_version.to_string(),
    );
    push_part(&mut parts, "extraction_version", &input.extraction_version);
    push_part(&mut parts, "step_set_version", &input.step_set_version);
    push_vec_part(&mut parts, "ordered_steps", &input.ordered_step_ids);
    parts
}

fn route_bridge_identity_parts(input: &RouteBridgeIdentityInput) -> Vec<String> {
    let mut parts = Vec::new();
    push_part(
        &mut parts,
        "file",
        &normalize_repo_relative_path(&input.repo_relative_path),
    );
    push_part(&mut parts, "language", &input.language);
    push_part(&mut parts, "adapter_version", &input.adapter_version);
    push_part(&mut parts, "framework", &input.framework);
    push_part(&mut parts, "source", &input.source_node_id);
    push_optional_part(&mut parts, "target", input.target_node_id.as_deref());
    push_optional_part(
        &mut parts,
        "literal_or_pattern",
        input.literal_or_pattern.as_deref(),
    );
    push_optional_part(
        &mut parts,
        "overload",
        input.overload_discriminator.as_deref(),
    );
    parts
}

fn push_optional_part(parts: &mut Vec<String>, label: &str, value: Option<&str>) {
    push_part(
        parts,
        label,
        value.filter(|value| non_empty(value)).unwrap_or("<none>"),
    );
}

fn push_vec_part(parts: &mut Vec<String>, label: &str, values: &[String]) {
    let joined = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| non_empty(value))
        .collect::<Vec<_>>()
        .join(">");
    push_part(
        parts,
        label,
        if joined.is_empty() {
            "<empty>"
        } else {
            joined.as_str()
        },
    );
}

fn push_part(parts: &mut Vec<String>, label: &str, value: &str) {
    parts.push(format!("{}={}", label.trim(), value.trim()));
}

fn non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn bool_part(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn function_input() -> MicroNodeIdentityInput {
        MicroNodeIdentityInput {
            repo_relative_path: "src/auth.ts".to_string(),
            language: "typescript".to_string(),
            kind: MicroNodeKind::FunctionFrame,
            function_entity_id: Some("repo://e/login".to_string()),
            scope_path: vec!["module:auth".to_string(), "function:login".to_string()],
            binding_id: Some("repo://e/login".to_string()),
            symbol: Some("login".to_string()),
            structural_path: vec!["function:login".to_string()],
            occurrence_index: 0,
            source_span: Some(SourceSpan::with_columns("src/auth.ts", 10, 1, 18, 2)),
            parse_recovery: false,
        }
    }

    fn binding_input(
        scope: &[&str],
        symbol: &str,
        occurrence_index: u32,
    ) -> MicroNodeIdentityInput {
        MicroNodeIdentityInput {
            repo_relative_path: "src/auth.ts".to_string(),
            language: "typescript".to_string(),
            kind: MicroNodeKind::LocalBinding,
            function_entity_id: Some("repo://e/login".to_string()),
            scope_path: scope.iter().map(|value| (*value).to_string()).collect(),
            binding_id: None,
            symbol: Some(symbol.to_string()),
            structural_path: vec!["block".to_string(), symbol.to_string()],
            occurrence_index,
            source_span: Some(SourceSpan::with_columns("src/auth.ts", 12, 7, 12, 12)),
            parse_recovery: false,
        }
    }

    fn return_site_input(
        function_entity_id: &str,
        structural_path: &[&str],
    ) -> MicroNodeIdentityInput {
        MicroNodeIdentityInput {
            repo_relative_path: "src/auth.ts".to_string(),
            language: "typescript".to_string(),
            kind: MicroNodeKind::ReturnSite,
            function_entity_id: Some(function_entity_id.to_string()),
            scope_path: vec!["module:auth".to_string(), "function:login".to_string()],
            binding_id: None,
            symbol: Some("return".to_string()),
            structural_path: structural_path
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            occurrence_index: 0,
            source_span: Some(SourceSpan::with_columns("src/auth.ts", 16, 3, 16, 15)),
            parse_recovery: false,
        }
    }

    fn local_returns_to_identity_fixture() -> LocalReturnsToIdentityContractInput {
        let function = function_input();
        let return_site = return_site_input("repo://e/login", &["function:login", "return#0"]);
        LocalReturnsToIdentityContractInput {
            repo_relative_path: "src/auth.ts".to_string(),
            language: "typescript".to_string(),
            function_entity_id: "repo://e/login".to_string(),
            scope_path: vec!["module:auth".to_string(), "function:login".to_string()],
            head_return_site_micro_node_id: stable_micro_node_id(&return_site),
            tail_function_frame_micro_node_id: stable_micro_node_id(&function),
            return_structural_path: vec!["function:login".to_string(), "return#0".to_string()],
            occurrence_index: 0,
            source_role: MicroSourceRole::Production,
            row_schema_version: MVP4_2_MICRO_EDGE_ROW_SCHEMA_VERSION,
            payload_version: MVP4_2_MICRO_EDGE_PAYLOAD_VERSION,
            extraction_version: MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION.to_string(),
            relation_span: Some(SourceSpan::with_columns("src/auth.ts", 16, 3, 16, 15)),
        }
    }

    #[test]
    fn local_returns_to_contract_locks_direction_and_forbidden_meanings() {
        assert_eq!(
            LOCAL_RETURNS_TO_CONTRACT.relation,
            MicroEdgeKind::LocalReturnsTo
        );
        assert_eq!(
            LOCAL_RETURNS_TO_CONTRACT.head_micro_node,
            MicroNodeKind::ReturnSite
        );
        assert_eq!(
            LOCAL_RETURNS_TO_CONTRACT.tail_micro_node,
            MicroNodeKind::FunctionFrame
        );
        assert!(LOCAL_RETURNS_TO_CONTRACT
            .canonical_meaning
            .contains("structurally owned"));
        for forbidden in [
            "returned_expression_flows_to_function_output",
            "runtime_return_value",
            "control_flow_reachability",
            "complete_return_coverage",
            "local_flows_to",
            "flow_proof",
        ] {
            assert!(
                LOCAL_RETURNS_TO_FORBIDDEN_INTERPRETATIONS.contains(&forbidden),
                "missing forbidden interpretation {forbidden}"
            );
        }
        assert!(!LOCAL_RETURNS_TO_CONTRACT.mutation_proof_activated);
        assert!(!LOCAL_RETURNS_TO_CONTRACT.flow_proof_activated);
    }

    #[test]
    fn local_returns_to_exactness_requires_spans_versions_and_lifecycle() {
        let exact =
            decide_local_returns_to_exactness(&LocalReturnsToExactnessInput::exact_claimable());
        assert_eq!(exact.exactness, MicroExactness::Exact);
        assert!(exact.claimable);
        assert!(exact.persist_proof_row);
        assert_eq!(
            exact.proof_ladder_level,
            Some(ProofLadderLevel::GraphRelationProof)
        );

        let mut missing_span = LocalReturnsToExactnessInput::exact_claimable();
        missing_span.return_site_source_span = false;
        let downgraded = decide_local_returns_to_exactness(&missing_span);
        assert_eq!(downgraded.exactness, MicroExactness::Unknown);
        assert!(!downgraded.claimable);
        assert!(!downgraded.persist_proof_row);
        assert_eq!(downgraded.proof_ladder_level, None);
        assert_eq!(
            downgraded.failure_action,
            "omit_exact_edge_or_emit_non_claimable_diagnostic_candidate_only"
        );
        assert!(downgraded
            .missing_requirements
            .contains(&LocalReturnsToExactnessRequirement::ReturnSiteSourceSpan));

        let requirement_names = LocalReturnsToExactnessRequirement::ALL
            .iter()
            .map(|requirement| requirement.as_str())
            .collect::<BTreeSet<_>>();
        assert!(requirement_names.contains("current_extraction_versions"));
        assert!(requirement_names.contains("claimable_lifecycle_passport"));
        assert!(requirement_names.contains("direct_ast_provenance"));
        assert!(requirement_names.contains("no_parser_recovery_ambiguity"));
    }

    #[test]
    fn local_returns_to_identity_is_deterministic_and_rekeys_on_endpoint_domain() {
        let fixture = local_returns_to_identity_fixture();
        let first = local_returns_to_identity_input(&fixture);
        let mut whitespace_shift = fixture.clone();
        whitespace_shift.relation_span =
            Some(SourceSpan::with_columns("src/auth.ts", 20, 3, 20, 15));
        let second = local_returns_to_identity_input(&whitespace_shift);
        assert_eq!(stable_micro_edge_id(&first), stable_micro_edge_id(&second));

        let mut moved_return = fixture.clone();
        moved_return.return_structural_path = vec![
            "function:login".to_string(),
            "if:authenticated".to_string(),
            "return#0".to_string(),
        ];
        assert_ne!(
            stable_micro_edge_id(&first),
            stable_micro_edge_id(&local_returns_to_identity_input(&moved_return))
        );

        let mut renamed_function = fixture.clone();
        renamed_function.function_entity_id = "repo://e/loginRenamed".to_string();
        assert_ne!(
            stable_micro_edge_id(&first),
            stable_micro_edge_id(&local_returns_to_identity_input(&renamed_function))
        );

        let mut renamed_file = fixture.clone();
        renamed_file.repo_relative_path = "src/auth-renamed.ts".to_string();
        assert_ne!(
            stable_micro_edge_id(&first),
            stable_micro_edge_id(&local_returns_to_identity_input(&renamed_file))
        );

        let mut duplicate_text_other_function = fixture.clone();
        duplicate_text_other_function.function_entity_id = "repo://e/logout".to_string();
        duplicate_text_other_function.scope_path =
            vec!["module:auth".to_string(), "function:logout".to_string()];
        duplicate_text_other_function.tail_function_frame_micro_node_id =
            stable_micro_node_id(&MicroNodeIdentityInput {
                function_entity_id: Some("repo://e/logout".to_string()),
                symbol: Some("logout".to_string()),
                structural_path: vec!["function:logout".to_string()],
                ..function_input()
            });
        assert_ne!(
            stable_micro_edge_id(&first),
            stable_micro_edge_id(&local_returns_to_identity_input(
                &duplicate_text_other_function
            ))
        );
    }

    #[test]
    fn local_returns_to_provenance_is_direct_ast_but_still_required() {
        let relation_span = SourceSpan::with_columns("src/auth.ts", 16, 3, 16, 15);
        let function_span = SourceSpan::with_columns("src/auth.ts", 10, 1, 18, 2);
        let valid = MicroFactProvenance {
            derivation_kind: MicroDerivationKind::DirectAstExtraction,
            source_fact_ids: Vec::new(),
            source_spans: vec![relation_span.clone(), function_span],
            extractor_or_adapter_version: MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION.to_string(),
            exactness: MicroExactness::Exact,
            limitations: vec![
                "local_ast_containment_only".to_string(),
                "does_not_prove_returned_value_flow".to_string(),
            ],
        };
        assert!(validate_micro_fact_provenance(&valid).is_ok());

        let missing_spans = MicroFactProvenance {
            source_spans: Vec::new(),
            ..valid.clone()
        };
        assert!(validate_micro_fact_provenance(&missing_spans).is_err());

        let missing_version = MicroFactProvenance {
            extractor_or_adapter_version: String::new(),
            ..valid
        };
        assert!(validate_micro_fact_provenance(&missing_version).is_err());
        assert_eq!(
            LOCAL_RETURNS_TO_CONTRACT.default_relation_span,
            "return_site_span"
        );
    }

    #[test]
    fn local_returns_to_cap_lifecycle_and_linter_contracts_separate_concerns() {
        assert!(LOCAL_RETURNS_TO_CAP_CONTRACT.edge_count_bounded_by_retained_return_sites);
        assert_eq!(
            LOCAL_RETURNS_TO_CAP_CONTRACT.max_edges_per_retained_return_site,
            1
        );
        assert!(LOCAL_RETURNS_TO_CAP_CONTRACT.omission_count_required);
        assert!(LOCAL_RETURNS_TO_CAP_CONTRACT.omission_reason_required);
        assert!(!LOCAL_RETURNS_TO_CAP_CONTRACT.exact_complete_claim_allowed_after_truncation);

        let lifecycle_names = MicroEdgeLayerState::ALL
            .iter()
            .map(|state| state.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            lifecycle_names,
            vec![
                "ready",
                "unavailable",
                "stale",
                "incompatible",
                "corrupt",
                "truncated",
                "updating/publishing",
                "not_applicable"
            ]
        );
        assert!(MicroEdgeLayerState::Corrupt.graph_claimability_separate());
        assert!(MicroEdgeLayerState::NotApplicable.graph_claimability_separate());

        let normal_delta = LocalReturnsToSourceDeltaKind::ReturnAdded.linter_class();
        assert_eq!(normal_delta, LocalReturnsToLinterClass::NormalSourceDelta);
        assert_eq!(
            normal_delta.validation_classification(),
            ValidationClassification::Diagnostic
        );
        assert!(!normal_delta.is_user_source_defect_by_default());
        assert!(!normal_delta.may_create_source_code_hard_interrupt_without_reverification());

        let integrity = LocalReturnsToIntegrityFindingKind::WrongEnclosingFunctionFrame;
        assert_eq!(
            integrity.linter_class(),
            LocalReturnsToLinterClass::ReverifiedProofIntegrityFinding
        );
        assert!(integrity
            .linter_class()
            .may_block_micro_edge_proof_availability());
        assert_eq!(
            integrity.recommended_action(),
            "reindex_or_repair_codegraph_micro_edge_state"
        );
        assert!(!integrity.linter_class().is_user_source_defect_by_default());

        let unsupported = LocalReturnsToUnsupportedConditionKind::CapOmission.linter_class();
        assert_eq!(
            unsupported,
            LocalReturnsToLinterClass::UnsupportedDegradedCondition
        );
        assert_eq!(
            unsupported.validation_classification(),
            ValidationClassification::Degraded
        );
        assert!(!unsupported.may_create_source_code_hard_interrupt_without_reverification());
    }

    #[test]
    fn local_returns_to_exact_edge_uses_graph_relation_proof_only() {
        assert_eq!(
            LOCAL_RETURNS_TO_CONTRACT.proof_ladder_level_for_exact_edge,
            ProofLadderLevel::GraphRelationProof
        );
        assert!(!LOCAL_RETURNS_TO_CONTRACT.mutation_proof_activated);
        assert!(!LOCAL_RETURNS_TO_CONTRACT.flow_proof_activated);
        assert_ne!(
            LOCAL_RETURNS_TO_CONTRACT.proof_ladder_level_for_exact_edge,
            ProofLadderLevel::MutationProof
        );
        assert_ne!(
            LOCAL_RETURNS_TO_CONTRACT.proof_ladder_level_for_exact_edge,
            ProofLadderLevel::FlowProof
        );
    }

    #[test]
    fn micro_identity_deterministic_repeat_and_whitespace_stability() {
        let first = function_input();
        let mut second = first.clone();
        second.source_span = Some(SourceSpan::with_columns("src/auth.ts", 14, 3, 22, 4));

        assert_eq!(stable_micro_node_id(&first), stable_micro_node_id(&second));
        assert_eq!(
            first.stability(),
            MicroIdentityStability::StableIdentityGuarantee
        );
    }

    #[test]
    fn micro_identity_scope_and_shadowing_are_separate() {
        let outer = binding_input(&["function:login", "block:outer"], "token", 0);
        let inner = binding_input(
            &["function:login", "block:outer", "block:inner"],
            "token",
            0,
        );

        assert_ne!(stable_micro_node_id(&outer), stable_micro_node_id(&inner));
        assert_eq!(
            outer.stability(),
            MicroIdentityStability::StableIdentityGuarantee
        );
    }

    #[test]
    fn micro_identity_duplicate_files_do_not_collide() {
        let first = binding_input(&["function:login"], "token", 0);
        let mut second = first.clone();
        second.repo_relative_path = "src/copy/auth.ts".to_string();

        assert_ne!(stable_micro_node_id(&first), stable_micro_node_id(&second));
    }

    #[test]
    fn micro_identity_rename_and_rekey_classification_is_explicit() {
        assert_eq!(
            classify_micro_identity_change(MicroIdentityChangeKind::FileRename),
            MicroIdentityStability::IntentionalRekey
        );
        assert_eq!(
            classify_micro_identity_change(MicroIdentityChangeKind::FunctionRename),
            MicroIdentityStability::IntentionalRekey
        );
        assert_eq!(
            classify_micro_identity_change(MicroIdentityChangeKind::WhitespaceOnlyEdit),
            MicroIdentityStability::StableIdentityGuarantee
        );
        assert_eq!(
            classify_micro_identity_change(MicroIdentityChangeKind::ParseRecoveryNode),
            MicroIdentityStability::UnknownAmbiguousCorrelation
        );
    }

    #[test]
    fn micro_identity_partial_parse_is_deterministic_but_ambiguous() {
        let mut recovered = binding_input(&["function:login"], "broken", 2);
        recovered.parse_recovery = true;
        recovered.binding_id = None;

        assert_eq!(
            stable_micro_node_id(&recovered),
            stable_micro_node_id(&recovered)
        );
        assert_eq!(
            recovered.stability(),
            MicroIdentityStability::UnknownAmbiguousCorrelation
        );
    }

    #[test]
    fn micro_edge_packet_route_bridge_collision_corpus_is_distinct() {
        let source = binding_input(&["function:login"], "token", 0);
        let target = binding_input(&["function:login"], "result", 0);
        let source_id = stable_micro_node_id(&source);
        let target_id = stable_micro_node_id(&target);
        let edge = MicroEdgeIdentityInput {
            repo_relative_path: "src/auth.ts".to_string(),
            language: "typescript".to_string(),
            kind: MicroEdgeKind::LocalFlowsTo,
            function_entity_id: Some("repo://e/login".to_string()),
            scope_path: vec!["function:login".to_string()],
            source_micro_node_id: source_id.clone(),
            target_micro_node_id: target_id.clone(),
            structural_path: vec!["return-flow".to_string()],
            occurrence_index: 0,
            source_span: Some(SourceSpan::new("src/auth.ts", 12, 15)),
        };
        let edge_id = stable_micro_edge_id(&edge);
        let packet = MicroPacketIdentityInput {
            repo_relative_path: "src/auth.ts".to_string(),
            language: "typescript".to_string(),
            function_entity_id: "repo://e/login".to_string(),
            packet_kind: "function_local_flow_packet".to_string(),
            packet_version: 1,
            extraction_version: "micro-extractor-v1".to_string(),
            step_set_version: "steps-v1".to_string(),
            ordered_step_ids: vec![edge_id.clone()],
        };
        let route = RouteBridgeIdentityInput {
            kind: RouteBridgeIdentityKind::Route,
            repo_relative_path: "src/routes.ts".to_string(),
            language: "typescript".to_string(),
            adapter_version: "express-adapter-v1".to_string(),
            framework: "express".to_string(),
            source_node_id: "micro-node://route-literal".to_string(),
            target_node_id: Some("repo://e/login".to_string()),
            literal_or_pattern: Some("GET /login".to_string()),
            overload_discriminator: Some("handler#0".to_string()),
        };
        let bridge = RouteBridgeIdentityInput {
            kind: RouteBridgeIdentityKind::Bridge,
            repo_relative_path: "src/client.ts".to_string(),
            language: "typescript".to_string(),
            adapter_version: "bridge-adapter-v1".to_string(),
            framework: "openapi".to_string(),
            source_node_id: "micro-node://client-call".to_string(),
            target_node_id: Some("unindexed_side:server".to_string()),
            literal_or_pattern: Some("GET /login".to_string()),
            overload_discriminator: Some("client#0".to_string()),
        };

        let ids = [
            source_id,
            target_id,
            edge_id,
            stable_micro_packet_id(&packet),
            stable_route_bridge_identity_id(&route),
            stable_route_bridge_identity_id(&bridge),
        ];
        let unique = ids.iter().collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), ids.len());
        assert!(ids.iter().all(|id| id.contains("://")));
    }

    #[test]
    fn micro_packet_identity_rekeys_on_function_or_step_version_change() {
        let base = MicroPacketIdentityInput {
            repo_relative_path: "src/auth.ts".to_string(),
            language: "typescript".to_string(),
            function_entity_id: "repo://e/login".to_string(),
            packet_kind: "function_local_flow_packet".to_string(),
            packet_version: 1,
            extraction_version: "micro-extractor-v1".to_string(),
            step_set_version: "steps-v1".to_string(),
            ordered_step_ids: vec!["micro-edge://a".to_string(), "micro-edge://b".to_string()],
        };
        let mut renamed = base.clone();
        renamed.function_entity_id = "repo://e/login-renamed".to_string();
        let mut new_steps = base.clone();
        new_steps.step_set_version = "steps-v2".to_string();

        assert_ne!(
            stable_micro_packet_id(&base),
            stable_micro_packet_id(&renamed)
        );
        assert_ne!(
            stable_micro_packet_id(&base),
            stable_micro_packet_id(&new_steps)
        );
    }

    #[test]
    fn micro_provenance_required_for_derived_fact() {
        let span = SourceSpan::new("src/auth.ts", 12, 14);
        let valid = MicroFactProvenance {
            derivation_kind: MicroDerivationKind::LocalAssignmentChainDerivation,
            source_fact_ids: vec![
                "micro-node://source".to_string(),
                "micro-edge://step".to_string(),
            ],
            source_spans: vec![span.clone()],
            extractor_or_adapter_version: "micro-extractor-v1".to_string(),
            exactness: MicroExactness::DerivedWithProvenance,
            limitations: vec!["function-local only".to_string()],
        };
        assert!(validate_micro_fact_provenance(&valid).is_ok());

        let missing_sources = MicroFactProvenance {
            source_fact_ids: Vec::new(),
            ..valid.clone()
        };
        assert!(validate_micro_fact_provenance(&missing_sources).is_err());

        let direct_ast = MicroFactProvenance {
            derivation_kind: MicroDerivationKind::DirectAstExtraction,
            source_fact_ids: Vec::new(),
            source_spans: vec![span],
            extractor_or_adapter_version: "micro-extractor-v1".to_string(),
            exactness: MicroExactness::Exact,
            limitations: vec!["existence only".to_string()],
        };
        assert!(validate_micro_fact_provenance(&direct_ast).is_ok());
    }

    #[test]
    fn micro_source_role_separates_local_production_proof() {
        assert!(micro_source_roles_allow_local_production_proof(&[
            MicroSourceRole::Production
        ]));
        assert!(!micro_source_roles_allow_local_production_proof(&[
            MicroSourceRole::Production,
            MicroSourceRole::Test
        ]));
        assert!(!micro_source_roles_allow_local_production_proof(&[
            MicroSourceRole::Generated
        ]));
        assert!(!micro_source_roles_allow_local_production_proof(&[
            MicroSourceRole::SourceText
        ]));
        assert_eq!(
            MicroSourceRole::Stub.as_current_evidence_role(),
            EvidenceRole::Mock
        );
    }
}
